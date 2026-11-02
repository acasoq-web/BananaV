//! White Balance through BananaV's OpenFX host, as any host runs it:
//! described as VEGAS's (its numbers at their defaults), the neutral color
//! and the temperature and tint keeping each other up to date with the
//! amount that takes the light's color to gray, and the picture: unchanged
//! at first, the color picked gray, further at a larger amount; Equalize
//! Histogram over the whole frame.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Context, Instance, Plugin, RenderRequest, Setup,
    Value,
};

const WB: &str = "org.bananav.WhiteBalance";

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
        .find(|p| p.info.identifier == WB)
        .expect(WB)
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
        size: (8.0, 4.0),
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

fn nums(inst: &Instance, name: &str) -> Vec<f64> {
    match inst.values().into_iter().find(|(n, _)| n == name) {
        Some((_, a)) => match a.value {
            Value::Num(v) => v,
            other => panic!("{name}: {other:?}"),
        },
        None => panic!("no {name}"),
    }
}

/// Sets a parameter as the user does: the plug-in hears of it.
fn type_in(inst: &Instance, name: &str, v: &[f64]) {
    inst.set_values(&[(name.into(), Animation::constant(Value::Num(v.to_vec())))]);
    inst.changed(&[name.into()], ChangeReason::User, 0.0);
}

#[test]
fn white_balance_describes_itself_as_vegas_does() {
    let info = &plugin().info;
    assert_eq!(info.label, "White Balance");
    assert_eq!(info.grouping, "BananaV/Color");
    let filter = info.context(Context::Filter).unwrap();
    let labels: Vec<_> = filter.params.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Select neutral color",
            "Temperature",
            "Tint",
            "Amount of correction",
            "Brightness",
            "Equalize Histogram"
        ]
    );
    // Its color shows in Lab, as VEGAS's.
    assert_eq!(filter.params[0].color_model, "Lab");
    // Mid gray: 5999 K and 10, as VEGAS shows them.
    let t = filter.params[1].default.num(0);
    let tint = filter.params[2].default.num(0);
    assert_eq!((t.round(), tint.round()), (5999.0, 10.0));
    assert_eq!(
        (filter.params[1].min[0], filter.params[1].max[0]),
        (1700.0, 15000.0)
    );
    // Equalize Histogram, off at first.
    assert!(filter.params[5].enabled);
    assert_eq!(filter.params[5].default.num(0), 0.0);
}

#[test]
fn the_neutral_color_and_the_temperature_follow_each_other() {
    let inst = instance();
    // A warm color picked: a warm light, and an amount.
    type_in(&inst, "neutral", &[0.7, 0.5, 0.35]);
    let t = nums(&inst, "temperature")[0];
    assert!(t < 4000.0, "{t}");
    let warm = nums(&inst, "amount")[0];
    assert!(warm > 0.3, "{warm}");
    // A temperature typed in: the neutral color of that light, cool, and
    // the amount that takes it to gray (VEGAS's numbers: 3000 K and 10
    // give 0.578).
    type_in(&inst, "temperature", &[9000.0]);
    let n = nums(&inst, "neutral");
    assert!(n[2] > n[0], "{n:?}");
    type_in(&inst, "temperature", &[3000.0]);
    type_in(&inst, "tint", &[10.0]);
    assert!((nums(&inst, "amount")[0] - 0.578).abs() < 0.0006);
    // The two kept together.
    type_in(&inst, "temperature", &[9000.0]);
    let n = nums(&inst, "neutral");
    type_in(&inst, "neutral", &n);
    assert!((nums(&inst, "temperature")[0] - 9000.0).abs() < 2.0);
    // An amount typed changes nothing else.
    type_in(&inst, "amount", &[1.0]);
    assert!((nums(&inst, "temperature")[0] - 9000.0).abs() < 2.0);
    assert_eq!(nums(&inst, "neutral"), n);
}

#[test]
fn the_picture_goes_through_the_light() {
    let mut img = ImageRgbaF32::transparent(8, 4);
    for y in 0..4 {
        for x in 0..8 {
            *img.pixel_mut(x, y) = [0.6, 0.45, 0.3, 1.0];
        }
    }
    let src = Arc::new(img);
    let inst = instance();
    // At first: nothing changes.
    let out = render(&inst, &src);
    for (a, b) in out.data.iter().zip(&src.data) {
        assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5), "{a:?}");
    }
    // That color picked: gray, its luma kept.
    type_in(&inst, "neutral", &[0.6, 0.45, 0.3]);
    let px = render(&inst, &src).pixel(3, 2);
    assert!(
        (px[0] - px[1]).abs() < 0.001 && (px[1] - px[2]).abs() < 0.001,
        "{px:?}"
    );
    assert!((px[0] - (0.299 * 0.6 + 0.587 * 0.45 + 0.114 * 0.3)).abs() < 0.001);
    // A larger amount goes further: blue.
    type_in(&inst, "amount", &[1.0]);
    let px = render(&inst, &src).pixel(3, 2);
    assert!(px[2] > px[0] + 0.1, "{px:?}");
    // None: nothing changes.
    type_in(&inst, "amount", &[0.0]);
    let px = render(&inst, &src).pixel(3, 2);
    assert!(
        (px[0] - 0.6).abs() < 1e-5 && (px[2] - 0.3).abs() < 1e-5,
        "{px:?}"
    );
}

#[test]
fn equalize_stretches_each_channel_and_holds_red_and_green_under_blue() {
    // A frame from dark to light: orange, a sky blue, a dark and a light
    // gray.
    let mut img = ImageRgbaF32::transparent(8, 4);
    for y in 0..4 {
        for x in 0..8 {
            *img.pixel_mut(x, y) = match (x / 2, y / 2) {
                (0, _) => [0.95, 0.55, 0.1, 1.0],
                (1, _) => [0.06, 0.88, 1.0, 1.0],
                (2, 0) => [0.1, 0.1, 0.1, 1.0],
                _ => [0.9, 0.9, 0.9, 1.0],
            };
        }
    }
    let src = Arc::new(img);
    let inst = instance();
    type_in(&inst, "equalize", &[1.0]);
    let out = render(&inst, &src);
    let at = |x, y| out.pixel(x, y);
    // Each channel from its darkest to its lightest over the frame: blue
    // from 0.1 to 1 (all of it), red from 0.06 to 0.95.
    let close = |a: [f32; 4], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-4);
    // Orange: gray at its blue (0.1 is blue's lowest: black).
    assert!(close(at(0, 0), [0.0, 0.0, 0.0]), "{:?}", at(0, 0));
    // Sky blue: blue its highest, red its lowest, green stretched.
    let g = (0.88 - 0.1) / (0.9 - 0.1);
    assert!(close(at(2, 0), [0.0, g, 1.0]), "{:?}", at(2, 0));
    // The light gray: each channel stretched, red and green held under
    // blue.
    let b = (0.9 - 0.1) / (1.0 - 0.1);
    let r = (0.9f32 - 0.06) / (0.95 - 0.06);
    assert!(close(at(6, 3), [r.min(b), b.min(1.0), b]), "{:?}", at(6, 3));
    // Its region of interest: the whole frame, wherever the window is.
    type_in(&inst, "equalize", &[0.0]);
    let px = render(&inst, &src).pixel(0, 0);
    assert!((px[0] - 0.95).abs() < 1e-5, "{px:?}");
}
