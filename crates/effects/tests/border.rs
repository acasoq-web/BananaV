//! Border through BananaV's OpenFX host, as any host runs it: described as
//! VEGAS's (its types, settings and presets, the angle and the colors
//! shown only for the types that use them) and the frame through it.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Context, Instance, Plugin, RenderRequest, Setup,
    Value,
};

const BORDER: &str = "org.bananav.Border";

fn plugin() -> &'static Arc<Plugin> {
    static PLUGINS: OnceLock<Vec<Arc<Plugin>>> = OnceLock::new();
    PLUGINS
        .get_or_init(|| {
            let bin = Binary::built_in(
                bv_effects::BINARY,
                bv_effects::number_of_plugins,
                bv_effects::get_plugin,
                None,
            );
            let (plugins, failures) = bin.plugins().expect("BananaV's effects load");
            assert!(failures.is_empty(), "{failures:?}");
            plugins
        })
        .iter()
        .find(|p| p.info.identifier == BORDER)
        .expect(BORDER)
}

/// A still picture as a clip.
struct Still(Arc<ImageRgbaF32>);

impl ClipSource for Still {
    fn picture(&self, _time: f64) -> Option<Arc<ImageRgbaF32>> {
        Some(Arc::clone(&self.0))
    }

    fn full_size(&self) -> (f64, f64) {
        (self.0.width as f64, self.0.height as f64)
    }
}

fn instance(w: u32, h: u32) -> Instance {
    let setup = Setup {
        frame_rate: 30.0,
        duration: 100.0,
        size: (w as f64, h as f64),
        pixel_aspect: 1.0,
        interactive: true,
    };
    Instance::new(plugin(), Context::Filter, &setup).expect("an instance")
}

fn render(inst: &Instance, src: &Arc<ImageRgbaF32>) -> ImageRgbaF32 {
    let source: Arc<dyn ClipSource> = Arc::new(Still(Arc::clone(src)));
    let req = RenderRequest {
        time: 0.0,
        width: src.width,
        height: src.height,
        scale: 1.0,
        draft: false,
        interactive: true,
        sequential: false,
        timeline: None,
    };
    inst.render_with(&[("Source", source)], &[], &req)
        .expect("it renders")
}

fn set(inst: &Instance, name: &str, v: &[f64]) {
    inst.set_values(&[(name.into(), Animation::constant(Value::Num(v.to_vec())))]);
    inst.changed(&[name.into()], ChangeReason::User, 0.0);
}

fn hidden(inst: &Instance, name: &str) -> bool {
    inst.param_infos()
        .into_iter()
        .find(|p| p.name == name)
        .expect(name)
        .secret
}

#[test]
fn border_describes_itself_as_vegas_does() {
    let info = &plugin().info;
    assert_eq!(info.label, "Border");
    assert_eq!(info.grouping, "BananaV/Creative");
    let filter = info.context(Context::Filter).unwrap();
    let labels: Vec<_> = filter.params.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, ["Type", "Size", "Angle", "Color", "Color"]);
    let ty = filter.params.iter().find(|p| p.name == "type").unwrap();
    assert_eq!(ty.options, ["Blurred", "Beveled", "Solid"]);
    let size = filter.params.iter().find(|p| p.name == "size").unwrap();
    assert_eq!(
        (size.min[0], size.max[0], size.default.num(0)),
        (0.0, 1.0, 0.0)
    );
    let names: Vec<_> = bv_effects::presets(BORDER).iter().map(|p| p.name).collect();
    assert_eq!(
        names,
        [
            "Soft Edge",
            "Bevel",
            "Solid White Border",
            "Translucent Blue Border",
            "Pyramid"
        ]
    );
}

#[test]
fn each_type_shows_only_what_it_uses() {
    let inst = instance(64, 36);
    inst.editing(true);
    // Blurred: the size alone.
    assert!(hidden(&inst, "angle") && hidden(&inst, "bevelColor") && hidden(&inst, "color"));
    assert!(!hidden(&inst, "size"));
    // Beveled: the angle and its color.
    set(&inst, "type", &[1.0]);
    assert!(!hidden(&inst, "angle") && !hidden(&inst, "bevelColor"));
    assert!(hidden(&inst, "color"));
    // Solid: the color with its alpha.
    set(&inst, "type", &[2.0]);
    assert!(hidden(&inst, "angle") && hidden(&inst, "bevelColor"));
    assert!(!hidden(&inst, "color"));
}

#[test]
fn the_frame_gets_its_border() {
    // An orange frame 64 × 36: the border is Size × 32 pixels wide.
    let mut img = ImageRgbaF32::transparent(64, 36);
    for p in img.data.iter_mut() {
        *p = [0.8, 0.4, 0.2, 1.0];
    }
    let src = Arc::new(img);
    let inst = instance(64, 36);
    let close = |a: [f32; 4], b: [f32; 4]| (0..4).all(|i| (a[i] - b[i]).abs() < 1e-3);
    // At first: nothing changes.
    let out = render(&inst, &src);
    assert!(close(out.pixel(0, 18), [0.8, 0.4, 0.2, 1.0]));
    // Solid, half see-through blue, 0.25: 8 pixels on every side.
    set(&inst, "type", &[2.0]);
    set(&inst, "size", &[0.25]);
    set(&inst, "color", &[0.0, 0.0, 1.0, 0.5]);
    let out = render(&inst, &src);
    for (x, y) in [(3, 18), (60, 18), (32, 2), (32, 33)] {
        assert!(
            close(out.pixel(x, y), [0.0, 0.0, 0.5, 0.5]),
            "{x} {y}: {:?}",
            out.pixel(x, y)
        );
    }
    assert!(close(out.pixel(32, 18), [0.8, 0.4, 0.2, 1.0]));
    // Blurred: the edges transparent, the middle (10 pixels in from the
    // rectangle's edge, nearly 3 sigmas) nearly as it was.
    set(&inst, "type", &[0.0]);
    let out = render(&inst, &src);
    assert!(out.pixel(0, 18)[3] < 0.05, "{:?}", out.pixel(0, 18));
    assert!(out.pixel(32, 18)[3] > 0.99, "{:?}", out.pixel(32, 18));
    let edge = out.pixel(8, 18);
    assert!((edge[3] - 0.5).abs() < 0.08, "{edge:?}");
    // Beveled, white, lit from the top: the top facet lighter, the bottom
    // darker, clipped at white.
    set(&inst, "type", &[1.0]);
    set(&inst, "angle", &[0.0]);
    set(&inst, "bevelColor", &[1.0, 1.0, 1.0]);
    let out = render(&inst, &src);
    // Rows count from the top: row 1 is the top facet.
    let top = out.pixel(32, 1);
    let bottom = out.pixel(32, 34);
    assert!(close(top, [1.0, 0.4 * 1.75, 0.2 * 1.75, 1.0]), "{top:?}");
    assert!(
        close(bottom, [0.8 * 0.35, 0.4 * 0.35, 0.2 * 0.35, 1.0]),
        "{bottom:?}"
    );
}
