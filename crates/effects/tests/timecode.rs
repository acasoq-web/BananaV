//! Timecode through BananaV's OpenFX host, as any host runs it: described
//! as VEGAS's (its formats, alignments, sizes and presets) and the time
//! written over the picture where its alignment says.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ClipSource, Context, Instance, Plugin, RenderRequest, Setup, Value,
};

const TIMECODE: &str = "org.bananav.Timecode";
const NTSC: f64 = 30000.0 / 1001.0;

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
        .find(|p| p.info.identifier == TIMECODE)
        .expect(TIMECODE)
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

const W: u32 = 320;
const H: u32 = 180;

fn instance(values: &[(&str, f64)]) -> Instance {
    let setup = Setup {
        frame_rate: NTSC,
        duration: 1000.0,
        size: (W as f64, H as f64),
        pixel_aspect: 1.0,
        interactive: false,
    };
    let inst = Instance::new(plugin(), Context::Filter, &setup).expect("an instance");
    let values: Vec<_> = values
        .iter()
        .map(|(n, v)| (n.to_string(), Animation::constant(Value::Num(vec![*v]))))
        .collect();
    inst.set_values(&values);
    inst
}

fn render(inst: &Instance, time: f64, timeline: Option<f64>) -> ImageRgbaF32 {
    let gray = Arc::new(ImageRgbaF32::filled(W, H, [0.5, 0.5, 0.5, 1.0]));
    let source: Arc<dyn ClipSource> = Arc::new(Still(gray));
    let req = RenderRequest {
        time,
        width: W,
        height: H,
        scale: 1.0,
        draft: false,
        interactive: false,
        sequential: false,
        timeline,
    };
    inst.render_with(&[("Source", source)], &[], &req)
        .expect("it renders")
}

/// Where the picture is no longer the gray it was: the box and its
/// letters (x from the left, y from the top).
fn written(img: &ImageRgbaF32) -> Option<(u32, u32, u32, u32)> {
    let mut b: Option<(u32, u32, u32, u32)> = None;
    for (i, p) in img.data.iter().enumerate() {
        if (p[0] - 0.5).abs() > 0.05 {
            let (x, y) = (i as u32 % img.width, i as u32 / img.width);
            b = Some(match b {
                None => (x, y, x + 1, y + 1),
                Some((l, t, r, bt)) => (l.min(x), t.min(y), r.max(x + 1), bt.max(y + 1)),
            });
        }
    }
    b
}

#[test]
fn timecode_describes_itself_as_vegas_does() {
    let info = &plugin().info;
    assert_eq!(info.label, "Timecode");
    assert_eq!(info.grouping, "BananaV/Utility");
    let filter = info.context(Context::Filter).unwrap();
    let labels: Vec<_> = filter.params.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Timecode format",
            "Horizontal alignment",
            "Vertical alignment",
            "Width",
            "Height"
        ]
    );
    let format = &filter.params[0];
    assert_eq!(
        format.options,
        [
            "Time",
            "Seconds",
            "Time & Frames",
            "Absolute Frames",
            "Feet and Frames 16mm (40fpf)",
            "Feet and Frames 35mm (16fpf)",
            "SMPTE Film Sync IVTC (23.976 fps, Video)",
            "SMPTE Film Sync (24 fps, Film)",
            "SMPTE EBU (25 fps, Video)",
            "SMPTE Non-Drop (29.97 fps, Video)",
            "SMPTE Drop (29.97 fps, Video)",
            "SMPTE 30 (30 fps, Audio)"
        ]
    );
    assert_eq!(format.default.num(0), 10.0);
    assert_eq!(filter.params[1].options, ["Left", "Middle", "Right"]);
    assert_eq!(filter.params[2].options, ["Top", "Middle", "Bottom"]);
    assert_eq!(filter.params[1].default.num(0), 2.0);
    assert_eq!(filter.params[2].default.num(0), 2.0);
    for size in &filter.params[3..] {
        // VEGAS's sliders: 0.5 to 2, at 1 (a third of the way).
        assert_eq!(
            (
                size.display_min[0],
                size.display_max[0],
                size.default.num(0)
            ),
            (0.5, 2.0, 1.0)
        );
    }
    let names: Vec<_> = bv_effects::presets(TIMECODE)
        .iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(
        names,
        [
            "SMPTE Drop (29.97 fps)",
            "SMPTE Drop (29.97 fps), Small",
            "SMPTE Non-Drop (29.97 fps)",
            "SMPTE Film IVTC (23.976 fps)",
            "SMPTE Film (24 fps)",
            "Seconds",
            "Absolute Frames",
            "Time & Frames"
        ]
    );
}

#[test]
fn the_time_is_written_in_the_lower_right_on_black() {
    let t = 183.0 / NTSC;
    let img = render(&instance(&[]), 183.0, Some(t));
    let (l, top, r, b) = written(&img).expect("written");
    // Inside the title-safe area's lower right, as VEGAS's.
    assert!(
        r as f64 > W as f64 * 0.88 && (r as f64) < W as f64 * 0.93,
        "{l}..{r}"
    );
    assert!(
        b as f64 > H as f64 * 0.89 && (b as f64) < H as f64 * 0.93,
        "{top}..{b}"
    );
    assert!(l as f64 > W as f64 * 0.6, "{l}");
    // The box black, the letters white.
    let px = |x: u32, y: u32| img.data[(y * W + x) as usize];
    let inside: Vec<[f32; 4]> = (top..b)
        .flat_map(|y| (l..r).map(move |x| (x, y)))
        .map(|(x, y)| px(x, y))
        .collect();
    assert!(inside.iter().any(|p| p[0] < 0.01 && p[3] == 1.0));
    assert!(inside.iter().any(|p| p[0] > 0.99));
    // Elsewhere, the picture.
    assert_eq!(px(10, 10), [0.5, 0.5, 0.5, 1.0]);
}

#[test]
fn other_hosts_time_is_their_projects() {
    // Without BananaV's timeline time, the effect's own time is the
    // project's (frames at its rate): the same picture.
    let inst = instance(&[]);
    let ours = render(&inst, 183.0, Some(183.0 / NTSC));
    let theirs = render(&inst, 183.0, None);
    assert_eq!(ours.data, theirs.data);
    // Another time, another text.
    let later = render(&inst, 183.0, Some(3600.0));
    assert_ne!(ours.data, later.data);
}

#[test]
fn alignments_move_the_box() {
    let t = Some(61.0);
    let top_left = render(&instance(&[("horizontal", 0.0), ("vertical", 0.0)]), 0.0, t);
    let (l, top, _, _) = written(&top_left).unwrap();
    assert!(
        (l as f64) < W as f64 * 0.1 && (top as f64) < H as f64 * 0.12,
        "{l} {top}"
    );
    let middle = render(&instance(&[("horizontal", 1.0), ("vertical", 1.0)]), 0.0, t);
    let (l, top, r, b) = written(&middle).unwrap();
    let (cx, cy) = ((l + r) as f64 / 2.0, (top + b) as f64 / 2.0);
    assert!((cx - W as f64 / 2.0).abs() < 3.0, "{cx}");
    assert!((cy - H as f64 / 2.0).abs() < 4.0, "{cy}");
    // Twice as wide and half as tall.
    let wide = render(&instance(&[("width", 2.0), ("height", 0.5)]), 0.0, t);
    let base = render(&instance(&[]), 0.0, t);
    let (l2, t2, r2, b2) = written(&wide).unwrap();
    let (l1, t1, r1, b1) = written(&base).unwrap();
    assert!((r2 - l2) as f64 > 1.8 * (r1 - l1) as f64);
    assert!(((b2 - t2) as f64) < 0.6 * (b1 - t1) as f64);
}
