//! Saturation Adjust through BananaV's OpenFX host, as any host runs it:
//! described as VEGAS's (its settings, ranges and presets) and the picture
//! through it: each color moved toward or away from its luma by how
//! saturated it is.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Context, Instance, Plugin, RenderRequest, Setup,
    Value,
};

const SATURATION: &str = "org.bananav.SaturationAdjust";

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
        .find(|p| p.info.identifier == SATURATION)
        .expect(SATURATION)
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
fn saturation_adjust_describes_itself_as_vegas_does() {
    let info = &plugin().info;
    assert_eq!(info.label, "Saturation Adjust");
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
            ("Amount", -1.0, 1.0, 0.0),
            ("Center", 0.0, 1.0, 0.5),
            ("Spread", 0.0, 1.0, 0.5),
            ("Low", -1.0, 1.0, 0.0),
            ("High", -1.0, 1.0, 0.0)
        ]
    );
    let names: Vec<_> = bv_effects::presets(SATURATION)
        .iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(
        names,
        [
            "Boost Midtones",
            "Reduce Minor Color Noise",
            "Reduce Oversaturated Colors",
            "Selectively Desaturate"
        ]
    );
}

#[test]
fn the_colors_move_by_how_saturated_they_are() {
    // A pale blue gray, the deep sky of VEGAS's picture, and the sky half
    // see-through.
    let pale = [0.7, 0.75, 0.8];
    let deep = [16.0 / 255.0, 226.0 / 255.0, 1.0];
    let mut img = ImageRgbaF32::transparent(4, 2);
    for y in 0..2 {
        *img.pixel_mut(0, y) = [pale[0], pale[1], pale[2], 1.0];
        *img.pixel_mut(1, y) = [deep[0], deep[1], deep[2], 1.0];
        *img.pixel_mut(2, y) = [deep[0] / 2.0, deep[1] / 2.0, 0.5, 0.5];
    }
    let src = Arc::new(img);
    let inst = instance();
    let luma = |c: [f32; 3]| 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
    let close = |a: [f32; 4], b: [f32; 4]| (0..4).all(|i| (a[i] - b[i]).abs() < 1e-4);
    // At first: nothing changes.
    let out = render(&inst, &src);
    assert!(close(out.pixel(1, 0), [deep[0], deep[1], deep[2], 1.0]));
    // High -1: the strong colors gray at their luma, their alpha kept;
    // the pale one as it was.
    set(&inst, "high", -1.0);
    let out = render(&inst, &src);
    let g = luma(deep);
    assert!(
        close(out.pixel(1, 0), [g, g, g, 1.0]),
        "{:?}",
        out.pixel(1, 0)
    );
    assert!(
        close(out.pixel(2, 1), [g / 2.0, g / 2.0, g / 2.0, 0.5]),
        "{:?}",
        out.pixel(2, 1)
    );
    assert!(
        close(out.pixel(0, 0), [pale[0], pale[1], pale[2], 1.0]),
        "{:?}",
        out.pixel(0, 0)
    );
    // Low 1: the pale color twice as far from its gray, the strong ones
    // as they were.
    set(&inst, "high", 0.0);
    set(&inst, "low", 1.0);
    let out = render(&inst, &src);
    let y = luma(pale);
    let twice = pale.map(|v| y + 2.0 * (v - y));
    assert!(
        close(out.pixel(0, 1), [twice[0], twice[1], twice[2], 1.0]),
        "{:?}",
        out.pixel(0, 1)
    );
    assert!(
        close(out.pixel(1, 1), [deep[0], deep[1], deep[2], 1.0]),
        "{:?}",
        out.pixel(1, 1)
    );
}
