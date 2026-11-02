//! Brightness and Contrast through BananaV's OpenFX host, as any host runs
//! it: described as VEGAS's, Calculate Brightness & Contrast reading the
//! frame at the cursor, and the light the host measured for Current Clip
//! and for Set for each Frame independently (the three keyed at every
//! frame, as VEGAS's).

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_effects::brightness_contrast::Light;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Context, Instance, Interp, Plugin, Setup, Value,
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
        .find(|p| p.info.identifier == "org.bananav.BrightnessContrast")
        .expect("Brightness and Contrast")
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
        size: (100.0, 10.0),
        pixel_aspect: 1.0,
        interactive: true,
    };
    Instance::new(plugin(), Context::Filter, &setup).expect("an instance")
}

const MINIMUM: f64 = 15.0 / 255.0;
const MAXIMUM: f64 = 235.0 / 255.0;

fn set(inst: &Instance, name: &str, value: Value) {
    inst.set_values(&[(name.into(), Animation::constant(value))]);
}

fn animation(inst: &Instance, name: &str) -> Animation {
    inst.values()
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, a)| a)
        .unwrap_or_else(|| panic!("no {name}"))
}

fn number(inst: &Instance, name: &str) -> f64 {
    match animation(inst, name).value {
        Value::Num(v) => v[0],
        other => panic!("{other:?}"),
    }
}

/// A picture whose dark end is black (a tenth of it) and the rest a
/// grey ramp up to `top`.
fn picture(top: f32) -> Arc<ImageRgbaF32> {
    let mut img = ImageRgbaF32::transparent(100, 10);
    for y in 0..10 {
        for x in 0..100 {
            let v = if y == 0 { 0.0 } else { top * x as f32 / 99.0 };
            *img.pixel_mut(x, y) = [v, v, v, 1.0];
        }
    }
    Arc::new(img)
}

/// The light of [`picture`].
fn light(top: f32) -> Light {
    let img = picture(top);
    let mut l = Light::new();
    for p in &img.data {
        l.add([p[0], p[1], p[2]]);
    }
    l
}

#[test]
fn brightness_and_contrast_describes_itself_as_vegas_does() {
    let filter = plugin().info.context(Context::Filter).unwrap();
    let param = |name: &str| {
        filter
            .params
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("no {name}"))
    };
    assert_eq!(
        param("minimumBrightness").default,
        Value::Num(vec![MINIMUM])
    );
    assert_eq!(
        param("maximumBrightness").default,
        Value::Num(vec![MAXIMUM])
    );
    assert_eq!(param("analysisMode").default, Value::Num(vec![1.0]));
    // Only the three at the top have keyframes.
    let animated: Vec<&str> = filter
        .params
        .iter()
        .filter(|p| p.animates && p.ty.has_value())
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(animated, ["brightness", "contrast", "contrastCenter"]);
}

/// Calculate asks the host for the clip's light, each frame's, or nothing
/// (the frame at the cursor the plug-in measures itself).
#[test]
fn calculate_asks_the_host_as_analysis_mode_says() {
    let inst = instance();
    let analysis = |inst: &Instance| {
        inst.param_infos()
            .into_iter()
            .find(|p| p.name == "calculate")
            .map(|p| p.analysis)
            .unwrap()
    };
    assert_eq!(analysis(&inst), "Histogram");
    for (mode, asks) in [(0.0, ""), (2.0, "Histograms"), (1.0, "Histogram")] {
        set(&inst, "analysisMode", Value::Num(vec![mode]));
        inst.changed(&["analysisMode".into()], ChangeReason::User, 0.0);
        assert_eq!(analysis(&inst), asks, "mode {mode}");
        // Nothing greyed out, as VEGAS's.
        for p in inst.param_infos() {
            assert!(p.enabled, "{} greyed out in mode {mode}", p.name);
        }
    }
}

/// Current Frame: the picture at the cursor measured, its black end at
/// Minimum (Brightness and Contrast center both Minimum, as VEGAS's on
/// the card and on a photograph), its bright end at Maximum.
#[test]
fn calculate_measures_the_frame_at_the_cursor() {
    let inst = instance();
    set(&inst, "analysisMode", Value::Num(vec![0.0]));
    inst.connect("Source", Arc::new(Still(picture(0.8))));
    let changed = inst.changed(&["calculate".into()], ChangeReason::User, 0.0);
    for name in ["brightness", "contrast", "contrastCenter"] {
        assert!(changed.iter().any(|n| n == name), "{name}: {changed:?}");
    }
    assert_eq!(number(&inst, "brightness"), MINIMUM);
    assert_eq!(number(&inst, "contrastCenter"), MINIMUM);
    // The ramp's top (less the cut) at Maximum: a spread above 1.
    let k = 1.0 / (1.0 - number(&inst, "contrast"));
    let top = (MAXIMUM - MINIMUM) / k;
    assert!((0.78..0.80).contains(&top), "{top}");
}

/// Current Clip: the light the host measured sets the three, and is not
/// kept.
#[test]
fn the_clips_light_sets_the_three() {
    let inst = instance();
    set(&inst, "clipLight", Value::Text(light(0.6).text()));
    inst.changed(&["clipLight".into()], ChangeReason::User, 0.0);
    assert_eq!(number(&inst, "brightness"), MINIMUM);
    assert_eq!(number(&inst, "contrastCenter"), MINIMUM);
    assert!(number(&inst, "contrast") > 0.0);
    assert_eq!(
        animation(&inst, "clipLight").value,
        Value::Text(String::new())
    );
    assert!(animation(&inst, "brightness").keys.is_empty());
}

/// Set for each Frame independently: each frame's light keys Brightness,
/// Contrast and Contrast center at that frame (the keys before gone).
#[test]
fn set_for_each_frame_keys_the_three_at_every_frame() {
    let inst = instance();
    set(&inst, "analysisMode", Value::Num(vec![2.0]));
    // An older key, replaced.
    let mut old = Animation::constant(Value::Num(vec![0.3]));
    old.set_key(40.0, Value::Num(vec![0.3]), Interp::default());
    inst.set_values(&[("brightness".into(), old)]);
    let frames = vec![(0.0, light(0.4)), (1.0, light(0.6)), (2.0, light(0.9))];
    set(&inst, "clipLight", Value::Text(Light::frames_text(&frames)));
    inst.changed(&["clipLight".into()], ChangeReason::User, 1.0);
    let keys = |name: &str| -> Vec<(f64, f64)> {
        animation(&inst, name)
            .keys
            .iter()
            .map(|k| match &k.value {
                Value::Num(v) => (k.time, v[0]),
                other => panic!("{other:?}"),
            })
            .collect()
    };
    let times = |name: &str| keys(name).iter().map(|k| k.0).collect::<Vec<_>>();
    for name in ["brightness", "contrast", "contrastCenter"] {
        assert_eq!(times(name), [0.0, 1.0, 2.0], "{name}");
    }
    for (_, v) in keys("brightness").into_iter().chain(keys("contrastCenter")) {
        assert_eq!(v, MINIMUM);
    }
    // The brighter the frame's top, the less spread.
    let spread: Vec<f64> = keys("contrast").iter().map(|k| k.1).collect();
    assert!(spread[0] > spread[1] && spread[1] > spread[2], "{spread:?}");
    assert_eq!(
        animation(&inst, "clipLight").value,
        Value::Text(String::new())
    );
}
