//! Black Bar Fill through BananaV's OpenFX host, as any host runs it:
//! described as VEGAS's (its groups, settings and presets), a portrait
//! picture's bars filled, and its crosshairs on the preview.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Context, Instance, PenAction, Plugin,
    RenderRequest, Setup, Value, View,
};

const FILL: &str = "org.bananav.BlackBarFill";

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
        .find(|p| p.info.identifier == FILL)
        .expect(FILL)
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

fn value(inst: &Instance, name: &str) -> Vec<f64> {
    match inst.values().into_iter().find(|(n, _)| n == name) {
        Some((_, a)) => match a.value {
            Value::Num(v) => v,
            other => panic!("{name}: {other:?}"),
        },
        None => panic!("no {name}"),
    }
}

/// A portrait picture in a landscape frame: an orange strip 220 pixels
/// wide in the middle of 640 × 360 (enlarged 3.185 times, wider than the
/// frame), the bars transparent.
fn portrait() -> Arc<ImageRgbaF32> {
    let mut img = ImageRgbaF32::transparent(640, 360);
    for y in 0..360 {
        for x in 210..430 {
            *img.pixel_mut(x, y) = [0.9, 0.5, 0.1, 1.0];
        }
    }
    Arc::new(img)
}

#[test]
fn black_bar_fill_describes_itself_as_vegas_does() {
    let info = &plugin().info;
    assert_eq!(info.label, "Black Bar Fill");
    assert_eq!(info.grouping, "BananaV/Utility");
    let filter = info.context(Context::Filter).unwrap();
    let rows: Vec<_> = filter
        .params
        .iter()
        .map(|p| (p.label.as_str(), p.parent.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            ("Foreground", ""),
            ("Location", "foreground"),
            ("Scale", "foreground"),
            ("Shadow Blur", "foreground"),
            ("Background", ""),
            ("Location", "background"),
            ("Scale", "background"),
            ("Blur", "background"),
        ]
    );
    let default = |name: &str| {
        let p = filter.params.iter().find(|p| p.name == name).unwrap();
        p.default.num(0)
    };
    assert_eq!(
        [
            default("fgScale"),
            default("shadowBlur"),
            default("bgScale"),
            default("bgBlur")
        ],
        [1.0, 0.2, 3.185, 0.071]
    );
    let names: Vec<_> = bv_effects::presets(FILL).iter().map(|p| p.name).collect();
    assert_eq!(names.len(), 8);
    assert_eq!(names[0], "Foreground Scaled");
    assert_eq!(names[7], "Background Maximum Scale");
}

#[test]
fn the_bars_fill_with_the_picture() {
    let src = portrait();
    let inst = instance(640, 360);
    let out = render(&inst, &src);
    // Everything shows: the bars hold the picture enlarged (an orange
    // all over), the picture in its place.
    assert!(out.data.iter().all(|p| p[3] > 0.999));
    let close = |a: [f32; 4], b: [f32; 4]| (0..4).all(|i| (a[i] - b[i]).abs() < 2e-3);
    assert!(close(out.pixel(320, 180), [0.9, 0.5, 0.1, 1.0]));
    assert!(
        close(out.pixel(5, 180), [0.9, 0.5, 0.1, 1.0]),
        "{:?}",
        out.pixel(5, 180)
    );
    // The shadow (2.8 pixels) darkens the background by the picture's
    // sides.
    let by = out.pixel(207, 180);
    assert!(by[0] < 0.8 && by[3] > 0.999, "{by:?}");
    // No shadow, the picture moved to the left: the background shows on
    // its right, as it was.
    set(&inst, "shadowBlur", &[0.0]);
    set(&inst, "fgLocation", &[0.25, 0.5]);
    let out = render(&inst, &src);
    assert!(close(out.pixel(300, 180), [0.9, 0.5, 0.1, 1.0]));
}

#[test]
fn the_crosshairs_move_the_layers() {
    let inst = instance(640, 360);
    assert!(inst.has_overlay());
    let v = View {
        time: 0.0,
        render_scale: 1.0,
        pixel_scale: [1.0, 1.0],
        background: [0.0; 3],
        viewport: [640, 360],
        to_frame: None,
    };
    assert!(!inst.overlay_draw(&v).is_empty());
    let pen =
        |a, at: [f64; 2]| inst.overlay_pen(a, &v, at, [at[0] as i32, 360 - at[1] as i32], 1.0);
    // Away from the crosshairs: not the overlay's.
    assert!(!pen(PenAction::Down, [100.0, 100.0]).taken);
    // Both in the middle: the foreground's taken first.
    assert!(pen(PenAction::Down, [322.0, 182.0]).taken);
    pen(PenAction::Motion, [386.0, 218.0]);
    pen(PenAction::Up, [386.0, 218.0]);
    let fg = value(&inst, "fgLocation");
    assert!(
        (fg[0] - 0.6).abs() < 1e-9 && (fg[1] - 0.6).abs() < 1e-9,
        "{fg:?}"
    );
    // Then the background's, left in the middle.
    assert!(pen(PenAction::Down, [320.0, 180.0]).taken);
    pen(PenAction::Motion, [256.0, 180.0]);
    pen(PenAction::Up, [256.0, 180.0]);
    let bg = value(&inst, "bgLocation");
    assert!(
        (bg[0] - 0.4).abs() < 1e-9 && (bg[1] - 0.5).abs() < 1e-9,
        "{bg:?}"
    );
}
