//! HSL Adjust through BananaV's OpenFX host, as any host runs it:
//! described as VEGAS's (its settings, ranges and presets) and the picture
//! through it: grays at (max + min) ⁄ 2, the hue turned, the saturation
//! past the cube clipped there.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Context, Instance, Plugin, RenderRequest, Setup,
    Value,
};

const HSL: &str = "org.bananav.HslAdjust";

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
        .find(|p| p.info.identifier == HSL)
        .expect(HSL)
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

fn instance() -> Instance {
    let setup = Setup {
        frame_rate: 30.0,
        duration: 100.0,
        size: (4.0, 2.0),
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

fn set(inst: &Instance, name: &str, v: f64) {
    inst.set_values(&[(name.into(), Animation::constant(Value::Num(vec![v])))]);
    inst.changed(&[name.into()], ChangeReason::User, 0.0);
}

#[test]
fn hsl_adjust_describes_itself_as_vegas_does() {
    let info = &plugin().info;
    assert_eq!(info.label, "HSL Adjust");
    assert_eq!(info.grouping, "BananaV/Color");
    let filter = info.context(Context::Filter).unwrap();
    let rows: Vec<_> = filter
        .params
        .iter()
        .map(|p| (p.label.as_str(), p.min[0], p.max[0], p.default.num(0)))
        .collect();
    assert_eq!(
        rows,
        [
            ("Add to hue", 0.0, 1.0, 0.0),
            ("Saturation", 0.0, 2.0, 1.0),
            ("Luminance", 0.0, 2.0, 1.0)
        ]
    );
    let names: Vec<_> = bv_effects::presets(HSL).iter().map(|p| p.name).collect();
    assert_eq!(
        names,
        [
            "Desaturate",
            "Invert Color",
            "Max Saturation",
            "Half Saturation",
            "Saturation and Luminance Boost"
        ]
    );
}

#[test]
fn the_colors_go_through_hsl() {
    // An orange, a sky blue (VEGAS's picture's) and half see-through.
    let mut img = ImageRgbaF32::transparent(4, 2);
    for y in 0..2 {
        *img.pixel_mut(0, y) = [0.9, 0.5, 0.1, 1.0];
        *img.pixel_mut(1, y) = [16.0 / 255.0, 226.0 / 255.0, 1.0, 1.0];
        *img.pixel_mut(2, y) = [0.45, 0.25, 0.05, 0.5];
    }
    let src = Arc::new(img);
    let inst = instance();
    let close = |a: [f32; 4], b: [f32; 4]| (0..4).all(|i| (a[i] - b[i]).abs() < 1e-4);
    // At first: nothing changes.
    let out = render(&inst, &src);
    assert!(close(out.pixel(0, 0), [0.9, 0.5, 0.1, 1.0]));
    // Desaturated: gray at (max + min) / 2, its alpha kept.
    set(&inst, "saturation", 0.0);
    let out = render(&inst, &src);
    assert!(
        close(out.pixel(0, 0), [0.5, 0.5, 0.5, 1.0]),
        "{:?}",
        out.pixel(0, 0)
    );
    assert!(
        close(out.pixel(2, 1), [0.25, 0.25, 0.25, 0.5]),
        "{:?}",
        out.pixel(2, 1)
    );
    // Half a turn: the opposite colors (VEGAS's sky went 255 46 16).
    set(&inst, "saturation", 1.0);
    set(&inst, "hue", 0.5);
    let out = render(&inst, &src);
    assert!(
        close(out.pixel(0, 0), [0.1, 0.5, 0.9, 1.0]),
        "{:?}",
        out.pixel(0, 0)
    );
    let sky = out.pixel(1, 0).map(|v| v * 255.0);
    assert!(
        (sky[0] - 255.0).abs() < 1.5 && (sky[1] - 46.0).abs() < 1.5 && (sky[2] - 16.0).abs() < 1.5,
        "{sky:?}"
    );
    // Twice the saturation: past the cube (8-bit projects clip what effects
    // leave there).
    set(&inst, "hue", 0.0);
    set(&inst, "saturation", 2.0);
    let out = render(&inst, &src);
    assert!(
        close(out.pixel(0, 0), [1.3, 0.5, -0.3, 1.0]),
        "{:?}",
        out.pixel(0, 0)
    );
}
