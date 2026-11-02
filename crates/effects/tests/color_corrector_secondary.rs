//! Color Corrector (Secondary) through BananaV's OpenFX host, as any host
//! runs it: described as VEGAS's (its settings, groups, ranges and
//! presets), the picture through it (what its limits pick corrected, the
//! mask shown, the alpha), Chrominance's eyedropper and Select effect
//! range picking on the preview.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Context, Instance, PenAction, Plugin,
    RenderRequest, Setup, Value, View,
};

const SECONDARY: &str = "org.bananav.ColorCorrectorSecondary";

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
        .find(|p| p.info.identifier == SECONDARY)
        .expect(SECONDARY)
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

const W: u32 = 40;
const H: u32 = 6;

fn instance() -> Instance {
    let setup = Setup {
        frame_rate: 30.0,
        duration: 100.0,
        size: (W as f64, H as f64),
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

/// A parameter's value now (its numbers).
fn value(inst: &Instance, name: &str) -> Vec<f64> {
    match inst.params().get(name).map(|p| p.animation().value) {
        Some(Value::Num(v)) => v,
        other => panic!("{name}: {other:?}"),
    }
}

/// Four colors, ten columns each: a bright warm one, a dark blue, a middle
/// grey and a green; the last rows half see-through.
fn picture() -> Arc<ImageRgbaF32> {
    let colors = [
        [0.95, 0.85, 0.7],
        [0.1, 0.12, 0.35],
        [0.5, 0.5, 0.5],
        [0.25, 0.6, 0.2],
    ];
    let mut img = ImageRgbaF32::transparent(W, H);
    for x in 0..W {
        let c = colors[(x / 10) as usize];
        for y in 0..H {
            *img.pixel_mut(x, y) = if y < 4 {
                [c[0], c[1], c[2], 1.0]
            } else {
                [c[0] / 2.0, c[1] / 2.0, c[2] / 2.0, 0.5]
            };
        }
    }
    Arc::new(img)
}

fn luma(p: [f32; 4]) -> f32 {
    (0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]) / p[3]
}

#[test]
fn color_corrector_secondary_describes_itself_as_vegas_does() {
    let info = &plugin().info;
    assert_eq!(info.label, "Color Corrector (Secondary)");
    assert_eq!(info.grouping, "BananaV/Color");
    assert!(info.overlay, "it takes clicks on the preview");
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
            "Chrominance",
            "Rotate hue",
            "Saturation",
            "Gamma",
            "Gain",
            "Offset",
            "Alpha",
            "Select effect range",
            "Show mask",
            "Invert mask",
            "Limit luminance",
            "Enable",
            "Low",
            "High",
            "Smooth",
            "Limit saturation",
            "Enable",
            "Low",
            "High",
            "Smooth",
            "Limit hue",
            "Enable",
            "Center",
            "Width",
            "Smooth"
        ]
    );
    let get = |name: &str| filter.params.iter().find(|p| p.name == name).unwrap();
    // The wheel under its triangle, the limits' groups closed, as VEGAS's.
    let wheel = get("chrominance");
    assert_eq!(wheel.interact, Some([300.0, 136.0]));
    assert!(wheel.interact_closed);
    for group in ["limitLuminance", "limitSaturation", "limitHue"] {
        assert!(!get(group).open, "{group}");
    }
    assert_eq!(get("lumaHigh").parent, "limitLuminance");
    // VEGAS's ranges: luminance 0–255, saturation to pure red's 162, hue
    // in degrees, its Smooth to 180.
    let range = |name: &str| {
        let p = get(name);
        (p.display_min[0], p.display_max[0], p.default.num(0))
    };
    assert_eq!(range("lumaHigh"), (0.0, 255.0, 255.0));
    assert_eq!(range("lumaSmooth"), (0.0, 255.0, 0.0));
    assert_eq!(range("satHigh"), (0.0, 162.0, 162.0));
    assert_eq!(range("hueCenter"), (0.0, 360.0, 180.0));
    assert_eq!(range("hueWidth"), (0.0, 360.0, 360.0));
    assert_eq!(range("hueSmooth"), (0.0, 180.0, 0.0));
    assert_eq!(range("saturation"), (0.0, 3.0, 1.0));
    assert_eq!(range("alpha"), (0.0, 1.0, 1.0));
    let names: Vec<_> = bv_effects::presets(SECONDARY)
        .iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(
        names,
        [
            "Invert Color",
            "Desaturate All but Reds",
            "Desaturate Highs",
            "Boost Mids",
            "Darken Mids",
            "Saturate Greens"
        ]
    );
}

#[test]
fn it_corrects_what_its_limits_pick() {
    let inst = instance();
    let src = picture();
    // At first: nothing changes.
    let out = render(&inst, &src);
    assert_eq!(out.data, src.data);
    // Desaturate Highs: the bright warm color goes grey at its luma (its
    // alpha kept), the dark blue stays.
    set(&inst, "saturation", 0.0);
    set(&inst, "lumaLow", 192.0);
    set(&inst, "lumaSmooth", 96.0);
    let out = render(&inst, &src);
    for y in [0, 5] {
        let p = out.pixel(0, y);
        assert!(
            (p[0] - p[1]).abs() < 1e-5 && (p[1] - p[2]).abs() < 1e-5,
            "{p:?}"
        );
        assert!((luma(p) - luma(src.pixel(0, y))).abs() < 1e-5);
        assert_eq!(p[3], src.pixel(0, y)[3]);
    }
    assert_eq!(out.pixel(10, 0), src.pixel(10, 0));
    // Its mask: white where it picks, black elsewhere.
    set(&inst, "showMask", 1.0);
    let out = render(&inst, &src);
    assert_eq!(out.pixel(0, 0), [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(out.pixel(10, 0), [0.0, 0.0, 0.0, 1.0]);
    // Inverted, and see-through where it picks: the bright color stays,
    // the dark blue goes.
    set(&inst, "showMask", 0.0);
    set(&inst, "invertMask", 1.0);
    set(&inst, "alpha", 0.0);
    let out = render(&inst, &src);
    assert_eq!(out.pixel(0, 0), src.pixel(0, 0));
    assert_eq!(out.pixel(10, 0)[3], 0.0);
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
fn it_picks_on_the_preview() {
    let inst = instance();
    let src = picture();
    inst.connect("Source", Arc::new(Still(Arc::clone(&src))));
    let preview = View {
        viewport: [W as i32, H as i32],
        ..wheel_view()
    };
    // Chrominance's complement eyedropper (under the wheel's right), then
    // the dark blue on the preview: the wheel across from its hue.
    let dropper = [165.0 + 60.0 + 22.0, 68.0 - 0.75 * 60.0];
    let a = inst.param_pen(
        "chrominance",
        PenAction::Down,
        &wheel_view(),
        dropper,
        [247, 23],
        1.0,
    );
    assert!(a.taken);
    assert_eq!(value(&inst, "picking"), [2.0]);
    let a = inst.overlay_pen(PenAction::Down, &preview, [15.5, 2.5], [15, 2], 1.0);
    assert!(
        a.taken && a.changed.contains(&"chrominance".to_string()),
        "{a:?}"
    );
    let wheel = value(&inst, "chrominance");
    // The dark blue's hue is about 335°: across, about 155°.
    assert!(
        (wheel[0] - 155.0).abs() < 10.0 && wheel[1] > 0.2,
        "{wheel:?}"
    );
    assert_eq!(value(&inst, "picking"), [0.0]);
    // Select effect range, then a rectangle over the green and the grey:
    // the limits span them.
    inst.changed(&["selectRange".into()], ChangeReason::User, 0.0);
    assert_eq!(value(&inst, "picking"), [3.0]);
    let a = inst.overlay_pen(PenAction::Down, &preview, [22.2, 3.8], [22, 3], 1.0);
    assert!(a.taken);
    inst.overlay_pen(PenAction::Motion, &preview, [37.8, 1.2], [37, 1], 1.0);
    let marks = inst.overlay_draw(&preview);
    assert!(!marks.is_empty(), "the rectangle shows while it is dragged");
    let a = inst.overlay_pen(PenAction::Up, &preview, [37.8, 1.2], [37, 1], 1.0);
    assert!(a.changed.contains(&"lumaLow".to_string()), "{a:?}");
    let y = |c: [f32; 3]| (0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]) as f64 * 255.0;
    let (low, high) = (value(&inst, "lumaLow")[0], value(&inst, "lumaHigh")[0]);
    assert!(
        (low - y([0.25, 0.6, 0.2])).abs() < 0.01 && (high - y([0.5; 3])).abs() < 0.01,
        "{low} {high}"
    );
    // The grey's saturation is none; the hue is the green's.
    assert_eq!(value(&inst, "satLow"), [0.0]);
    let center = value(&inst, "hueCenter")[0];
    assert!((center - 230.0).abs() < 20.0, "{center}");
    assert_eq!(value(&inst, "picking"), [0.0]);
}
