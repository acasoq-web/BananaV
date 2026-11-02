//! Color Corrector through BananaV's OpenFX host, as any host runs it:
//! described, rendered with its wheels and presets, its wheels' own
//! interface drawn and clicked, and an eyedropper picking a color on the
//! preview.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ClipSource, Context, Instance, Mark, PenAction, Plugin, RenderRequest,
    Setup, Value, View,
};

fn corrector() -> &'static Arc<Plugin> {
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
        .find(|p| p.info.identifier == "org.bananav.ColorCorrector")
        .expect("Color Corrector")
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

/// A grey ramp, black at the left, white at the right.
fn ramp(w: u32) -> Arc<ImageRgbaF32> {
    let mut img = ImageRgbaF32::transparent(w, 2);
    for y in 0..2 {
        for x in 0..w {
            let v = x as f32 / (w - 1) as f32;
            *img.pixel_mut(x, y) = [v, v, v, 1.0];
        }
    }
    Arc::new(img)
}

fn instance() -> Instance {
    let setup = Setup {
        frame_rate: 30.0,
        duration: 100.0,
        size: (256.0, 2.0),
        pixel_aspect: 1.0,
        interactive: true,
    };
    Instance::new(corrector(), Context::Filter, &setup).expect("an instance")
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
}

/// A parameter's value now (its numbers).
fn value(inst: &Instance, name: &str) -> Vec<f64> {
    match inst.params().get(name).map(|p| p.animation().value) {
        Some(Value::Num(v)) => v,
        other => panic!("{name}: {other:?}"),
    }
}

fn luma(p: [f32; 4]) -> f32 {
    0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]
}

#[test]
fn color_corrector_describes_itself_as_vegas_does() {
    let info = &corrector().info;
    assert_eq!(info.label, "Color Corrector");
    assert_eq!(info.grouping, "BananaV/Color");
    assert!(info.overlay, "its eyedroppers take clicks on the preview");
    let filter = info.context(Context::Filter).unwrap();
    let shown: Vec<_> = filter
        .params
        .iter()
        .filter(|p| !p.secret)
        .map(|p| p.label.as_str())
        .collect();
    assert_eq!(
        shown,
        [
            "Low",
            "Mid",
            "High",
            "Saturation",
            "Gamma",
            "Gain",
            "Offset"
        ]
    );
    let low = &filter.params[0];
    assert_eq!(low.ty, bv_ofx::ParamType::Double2D);
    assert_eq!(low.dimension_labels, ["Angle", "Magnitude"]);
    assert_eq!(low.default, Value::Num(vec![0.0, 0.0]));
    assert_eq!((low.max.clone(), low.animates), (vec![360.0, 1.0], true));
    // Its wheel, in BananaV's host.
    assert_eq!(low.interact, Some([300.0, 136.0]));
    let gamma = filter.params.iter().find(|p| p.name == "gamma").unwrap();
    assert_eq!(gamma.default, Value::Num(vec![1.0]));
    assert_eq!((gamma.display_min[0], gamma.display_max[0]), (0.0, 2.0));
}

#[test]
fn the_wheels_color_the_dark_middle_and_light_parts() {
    let inst = instance();
    let src = ramp(256);
    // At its defaults: the picture as it was.
    let same = render(&inst, &src);
    assert_eq!(same.data, src.data);
    // Red Midtones: the middle reddens, black and white barely, the light
    // stays.
    set(&inst, "mid", &[103.5, 0.5]);
    let out = render(&inst, &src);
    let mid = out.pixel(128, 0);
    assert!(
        mid[0] > mid[1] + 0.15 && (mid[1] - mid[2]).abs() < 0.01,
        "{mid:?}"
    );
    assert!((luma(mid) - luma(src.pixel(128, 0))).abs() < 1e-3);
    let (black, white) = (out.pixel(0, 0), out.pixel(255, 0));
    assert!(black[0] < 0.01 && white[1] > 0.99, "{black:?} {white:?}");
    // Blue Highlight: the light parts blue, the dark ones as they were.
    set(&inst, "mid", &[0.0, 0.0]);
    set(&inst, "high", &[347.1, 0.5]);
    let out = render(&inst, &src);
    let light = out.pixel(200, 0);
    assert!(light[2] > light[0] + 0.1, "{light:?}");
    let dark = out.pixel(30, 0);
    assert!((dark[2] - dark[0]).abs() < 0.01, "{dark:?}");
    // Saturation 0 greys; gain doubles the light.
    set(&inst, "saturation", &[0.0]);
    let out = render(&inst, &src);
    let p = out.pixel(200, 0);
    assert!((p[0] - p[2]).abs() < 1e-5, "{p:?}");
    set(&inst, "high", &[0.0, 0.0]);
    set(&inst, "saturation", &[1.0]);
    set(&inst, "gain", &[2.0]);
    let out = render(&inst, &src);
    assert!((out.pixel(64, 0)[1] - 2.0 * 64.0 / 255.0).abs() < 1e-4);
}

#[test]
fn remove_yellow_cast_does() {
    let inst = instance();
    for (name, v) in &bv_effects::presets("org.bananav.ColorCorrector")
        .into_iter()
        .find(|p| p.name == "Remove Yellow Cast")
        .expect("the preset")
        .values
    {
        let bv_effects::PresetValue::Num(v) = v else {
            panic!("{v:?}")
        };
        set(&inst, name, v);
    }
    // A yellowish grey comes out bluer: its blue up, red and green down.
    let mut img = ImageRgbaF32::transparent(4, 1);
    for x in 0..4 {
        *img.pixel_mut(x, 0) = [0.55, 0.52, 0.38, 1.0];
    }
    let img = Arc::new(img);
    let out = render(&inst, &img);
    let p = out.pixel(1, 0);
    assert!(p[2] > 0.38 + 0.08 && p[0] < 0.55, "{p:?}");
}

/// How a wheel's own interface is shown: 300 by 136 screen pixels.
fn wheel_view() -> View {
    View {
        time: 0.0,
        render_scale: 1.0,
        pixel_scale: [1.0, 1.0],
        background: [0.16; 3],
        viewport: [300, 136],
        to_frame: None,
    }
}

#[test]
fn a_wheel_draws_itself_and_takes_the_pen() {
    let inst = instance();
    let view = wheel_view();
    let marks = inst.param_draw("mid", &view);
    // The hues around, the greys over them, the names and degrees.
    let fills = marks
        .iter()
        .filter(|m| matches!(m, Mark::Polygon { .. }))
        .count();
    assert!(fills >= 150, "{fills}");
    for name in ["R", "Mg", "B", "Cy", "G", "Yl", "90°"] {
        assert!(
            marks
                .iter()
                .any(|m| matches!(m, Mark::Text { text, .. } if text == name)),
            "{name}"
        );
    }
    // A click on the wheel, straight up from its middle and half way out:
    // 90°, half the magnitude.
    let (cx, cy, r) = (165.0, 68.0, 60.0);
    let a = inst.param_pen(
        "mid",
        PenAction::Down,
        &view,
        [cx, cy + r / 2.0],
        [165, 98],
        1.0,
    );
    assert!(a.taken);
    assert!(a.changed.contains(&"mid".to_string()), "{a:?}");
    let v = value(&inst, "mid");
    assert!(
        (v[0] - 90.0).abs() < 1e-6 && (v[1] - 0.5).abs() < 1e-6,
        "{v:?}"
    );
    // Dragged to the right of the middle, all the way out: 0°, 1.
    inst.param_pen(
        "mid",
        PenAction::Motion,
        &view,
        [cx + 90.0, cy],
        [255, 68],
        1.0,
    );
    inst.param_pen("mid", PenAction::Up, &view, [cx + 90.0, cy], [255, 68], 1.0);
    assert_eq!(value(&inst, "mid"), [0.0, 1.0]);
    // Away from the wheel the pen is not its.
    let off = inst.param_pen("mid", PenAction::Down, &view, [10.0, 10.0], [10, 10], 1.0);
    assert!(!off.taken && off.changed.is_empty());
}

#[test]
fn an_eyedropper_picks_a_color_on_the_preview() {
    let inst = instance();
    let view = wheel_view();
    // A bluish grey picture: Low's complement eyedropper (under the
    // wheel's right) takes the blue out of the dark parts.
    let mut img = ImageRgbaF32::transparent(256, 2);
    for y in 0..2 {
        for x in 0..256 {
            *img.pixel_mut(x, y) = [0.10, 0.12, 0.20, 1.0];
        }
    }
    let img = Arc::new(img);
    inst.connect("Source", Arc::new(Still(Arc::clone(&img))));
    let dropper = [165.0 + 60.0 + 22.0, 68.0 - 0.75 * 60.0];
    let a = inst.param_pen("low", PenAction::Down, &view, dropper, [247, 23], 1.0);
    assert!(a.taken);
    assert_eq!(value(&inst, "picking"), [2.0]);
    // The click on the preview (canonical coordinates).
    let preview = View {
        viewport: [256, 2],
        ..wheel_view()
    };
    let a = inst.overlay_pen(PenAction::Down, &preview, [100.0, 1.0], [100, 1], 1.0);
    assert!(a.taken);
    assert!(a.changed.contains(&"low".to_string()), "{a:?}");
    assert_eq!(value(&inst, "picking"), [0.0]);
    let low = value(&inst, "low");
    // The cast's hue is at 331.8° (blue, a little cyan): across, 151.8°.
    assert!((low[0] - 151.8).abs() < 0.5 && low[1] > 0.1, "{low:?}");
    // Rendered: the cast is gone.
    let out = render(&inst, &img);
    let p = out.pixel(100, 0);
    assert!((p[2] - p[0]).abs() < 0.02, "{p:?}");
    // Without an eyedropper waiting, the preview keeps its clicks.
    let a = inst.overlay_pen(PenAction::Down, &preview, [10.0, 1.0], [10, 1], 1.0);
    assert!(!a.taken);
}
