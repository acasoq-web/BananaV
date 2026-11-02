//! Levels through BananaV's OpenFX host, as any host runs it: described
//! as VEGAS's, rendered on one channel and on alpha, and its presets.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Context, Instance, Plugin, RenderRequest, Setup,
    Value,
};

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
        .find(|p| p.info.identifier == "org.bananav.Levels")
        .expect("Levels")
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
        size: (2.0, 1.0),
        pixel_aspect: 1.0,
        interactive: true,
    };
    Instance::new(plugin(), Context::Filter, &setup).expect("an instance")
}

fn set(inst: &Instance, values: &[(&str, f64)]) {
    let values: Vec<(String, Animation)> = values
        .iter()
        .map(|(n, v)| (n.to_string(), Animation::constant(Value::Num(vec![*v]))))
        .collect();
    inst.set_values(&values);
    let names: Vec<String> = values.into_iter().map(|(n, _)| n).collect();
    inst.changed(&names, ChangeReason::User, 0.0);
}

/// Two pixels, premultiplied: an opaque one and a half transparent one.
fn picture() -> Arc<ImageRgbaF32> {
    let mut img = ImageRgbaF32::transparent(2, 1);
    *img.pixel_mut(0, 0) = [0.3, 0.6, 0.2, 1.0];
    *img.pixel_mut(1, 0) = [0.15, 0.3, 0.1, 0.5];
    Arc::new(img)
}

fn render(inst: &Instance) -> ImageRgbaF32 {
    let src = picture();
    let source: Arc<dyn ClipSource> = Arc::new(Still(Arc::clone(&src)));
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

fn close(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
}

#[test]
fn levels_describes_itself_as_vegas_does() {
    let filter = plugin().info.context(Context::Filter).unwrap();
    let labels: Vec<&str> = filter.params.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Channel",
            "Input start",
            "Input end",
            "Output start",
            "Output end",
            "Gamma"
        ]
    );
    assert!(filter.params.iter().all(|p| p.animates), "all have clocks");
    assert_eq!(
        filter.params[0].options,
        ["All", "Red", "Green", "Blue", "Alpha"]
    );
    let gamma = &filter.params[5];
    assert_eq!((gamma.min[0], gamma.max[0]), (0.45, 5.0));
    let names: Vec<&str> = bv_effects::presets("org.bananav.Levels")
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(
        names,
        [
            "Brighten",
            "Darken",
            "Increase Gamma",
            "Decrease Gamma",
            "Studio RGB to Computer RGB",
            "Computer RGB to Studio RGB"
        ]
    );
}

/// Red alone, its input 0–0.5 stretched to 0–1: the others as they were,
/// on the straight colors (the half transparent pixel's too).
#[test]
fn one_channel_changes_alone() {
    let inst = instance();
    set(&inst, &[("channel", 1.0), ("inputEnd", 0.5)]);
    let out = render(&inst);
    assert!(
        close(out.pixel(0, 0), [0.6, 0.6, 0.2, 1.0]),
        "{:?}",
        out.pixel(0, 0)
    );
    assert!(
        close(out.pixel(1, 0), [0.3, 0.3, 0.1, 0.5]),
        "{:?}",
        out.pixel(1, 0)
    );
}

/// Alpha's levels: Output end 0.5 halves it, the colors kept.
#[test]
fn alpha_has_its_levels_too() {
    let inst = instance();
    set(&inst, &[("channel", 4.0), ("outputEnd", 0.5)]);
    let out = render(&inst);
    assert!(
        close(out.pixel(0, 0), [0.15, 0.3, 0.1, 0.5]),
        "{:?}",
        out.pixel(0, 0)
    );
    assert!(
        close(out.pixel(1, 0), [0.075, 0.15, 0.05, 0.25]),
        "{:?}",
        out.pixel(1, 0)
    );
}

/// All: Increase Gamma lightens the three colors alike, alpha kept.
#[test]
fn gamma_lightens_every_color() {
    let inst = instance();
    set(&inst, &[("gamma", 2.0)]);
    let p = render(&inst).pixel(0, 0);
    assert!(p[0] > 0.3 && p[1] > 0.6 && p[2] > 0.2, "{p:?}");
    assert_eq!(p[3], 1.0);
}
