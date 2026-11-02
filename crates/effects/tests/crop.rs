//! Crop through BananaV's OpenFX host, as any host runs it: described,
//! rendered, its crops changed by hand keeping the anchor, and handled on
//! the preview.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Context, Instance, Mark, PenAction, Plugin,
    RenderRequest, Setup, Value, View,
};

fn crop() -> &'static Arc<Plugin> {
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
        .find(|p| p.info.identifier == "org.bananav.Crop")
        .expect("Crop")
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

/// A picture whose every pixel differs: red across, green down.
fn picture(w: u32, h: u32) -> Arc<ImageRgbaF32> {
    let mut img = ImageRgbaF32::transparent(w, h);
    for y in 0..h {
        for x in 0..w {
            *img.pixel_mut(x, y) = [x as f32 / w as f32, y as f32 / h as f32, 0.5, 1.0];
        }
    }
    Arc::new(img)
}

fn instance(w: u32, h: u32) -> Instance {
    let setup = Setup {
        frame_rate: 30.0,
        duration: 100.0,
        size: (w as f64, h as f64),
        pixel_aspect: 1.0,
        interactive: true,
    };
    Instance::new(crop(), Context::Filter, &setup).expect("an instance")
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

fn value(inst: &Instance, name: &str) -> Vec<f64> {
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
    set(inst, name, v);
    inst.changed(&[name.into()], ChangeReason::User, 0.0);
}

fn close(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
}

fn near(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
}

#[test]
fn crop_describes_itself_as_vegas_does() {
    let info = &crop().info;
    assert_eq!(info.label, "Crop");
    assert_eq!(info.grouping, "BananaV/Utility");
    assert_eq!(info.contexts, [Context::Filter, Context::General]);
    assert!(info.overlay, "handles on the preview");
    let filter = info.context(Context::Filter).unwrap();
    let labels: Vec<_> = filter.params.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Anchor Point",
            "X Crop",
            "Y Crop",
            "Location",
            "Angle",
            "Opacity"
        ]
    );
    let anchor = &filter.params[0];
    assert_eq!(anchor.options[2], "Right");
    assert_eq!(anchor.default, Value::Num(vec![2.0]));
    assert!(!anchor.animates);
    assert!(filter.params[1..].iter().all(|p| p.animates));
    // The location counts the frame 0–1, as VEGAS shows it: the window's
    // middle, in the frame's at first.
    let inst = instance(640, 360);
    assert_eq!(value(&inst, "location"), [0.5, 0.5]);
    // VEGAS's four presets.
    let names: Vec<_> = bv_effects::presets("org.bananav.Crop")
        .iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(
        names,
        [
            "Crop Left Half, No Angle, Opaque",
            "Crop Right Half, No Angle, Opaque",
            "Crop Top Half, No Angle, Opaque",
            "Crop Bottom Half, No Angle, Opaque"
        ]
    );
}

#[test]
fn at_its_defaults_it_changes_nothing() {
    let src = picture(64, 36);
    let out = render(&instance(64, 36), &src);
    assert_eq!(out.data, src.data);
}

#[test]
fn the_crop_keeps_the_anchors_side() {
    let src = picture(64, 36);
    let inst = instance(64, 36);
    // Anchored right (the default), half the width typed in: the right
    // half stays, the window's middle going to 0.75 — VEGAS's Crop Left
    // Half.
    type_in(&inst, "xCrop", &[0.5]);
    assert_eq!(value(&inst, "location"), [0.75, 0.5]);
    let out = render(&inst, &src);
    assert!(
        close(out.pixel(31, 10), [0.0; 4]),
        "{:?}",
        out.pixel(31, 10)
    );
    assert!(close(out.pixel(32, 10), src.pixel(32, 10)));
    assert!(close(out.pixel(63, 35), src.pixel(63, 35)));
    // Anchored at the top, a third of the height: the top third.
    let inst = instance(64, 36);
    type_in(&inst, "anchor", &[3.0]);
    type_in(&inst, "yCrop", &[1.0 / 3.0]);
    let out = render(&inst, &src);
    assert!(close(out.pixel(5, 11), src.pixel(5, 11)));
    assert!(close(out.pixel(5, 12), [0.0; 4]));
    // Given with the crop (a preset, an undo), the location stays as given.
    set(&inst, "xCrop", &[0.5]);
    set(&inst, "location", &[0.25, 0.5]);
    inst.changed(
        &["location".into(), "xCrop".into()],
        ChangeReason::User,
        0.0,
    );
    assert_eq!(value(&inst, "location"), [0.25, 0.5]);
}

#[test]
fn vegass_presets_keep_its_halves() {
    let src = picture(64, 36);
    let inst = instance(64, 36);
    // A pixel each preset keeps and one it cuts away (from the top).
    let pixels = [
        ([48, 18], [16, 18]),
        ([16, 18], [48, 18]),
        ([32, 30], [32, 5]),
        ([32, 5], [32, 30]),
    ];
    for (p, (kept, gone)) in bv_effects::presets("org.bananav.Crop").iter().zip(pixels) {
        for (name, v) in &p.values {
            let bv_effects::PresetValue::Num(v) = v else {
                panic!("{v:?}")
            };
            set(&inst, name, v);
        }
        let out = render(&inst, &src);
        let [x, y] = kept;
        assert!(close(out.pixel(x, y), src.pixel(x, y)), "{}", p.name);
        let [x, y] = gone;
        assert!(close(out.pixel(x, y), [0.0; 4]), "{}", p.name);
    }
}

#[test]
fn location_angle_and_opacity() {
    let src = picture(64, 36);
    let inst = instance(64, 36);
    // The window moved right by a quarter: the picture stays, the left
    // quarter goes.
    set(&inst, "location", &[0.75, 0.5]);
    let out = render(&inst, &src);
    assert!(close(out.pixel(15, 18), [0.0; 4]));
    assert!(close(out.pixel(16, 18), src.pixel(16, 18)));
    assert!(close(out.pixel(63, 18), src.pixel(63, 18)));
    // Half opacity: everything shown at half.
    set(&inst, "location", &[0.5, 0.5]);
    set(&inst, "opacity", &[0.5]);
    let out = render(&inst, &src);
    let p = src.pixel(20, 20);
    assert!(close(out.pixel(20, 20), p.map(|c| c * 0.5)));
    // A 36×36 square turned 45°: its corners cut, its middle whole.
    set(&inst, "opacity", &[1.0]);
    set(&inst, "anchor", &[0.0]);
    set(&inst, "xCrop", &[36.0 / 64.0]);
    set(&inst, "angle", &[45.0]);
    let out = render(&inst, &src);
    assert!(close(out.pixel(32, 18), src.pixel(32, 18)));
    // (50, 2) was inside the square unturned, and is out once turned.
    assert!(close(out.pixel(48, 2), [0.0; 4]), "{:?}", out.pixel(48, 2));
    // 20 px right of the middle is inside the turned square.
    assert!(close(out.pixel(52, 18), src.pixel(52, 18)));
}

fn view() -> View {
    View {
        time: 0.0,
        render_scale: 1.0,
        pixel_scale: [1.0, 1.0],
        background: [0.0; 3],
        viewport: [640, 360],
        to_frame: None,
    }
}

#[test]
fn the_overlay_draws_the_window_its_handles_and_the_rotation_control() {
    let inst = instance(640, 360);
    assert!(inst.has_overlay());
    let marks = inst.overlay_draw(&view());
    // The window's outline: the frame.
    let outline = marks.iter().find_map(|m| match m {
        Mark::Strip {
            points,
            closed: true,
            ..
        } if points.len() == 4 => Some(points.clone()),
        _ => None,
    });
    assert_eq!(
        outline,
        Some(vec![[0.0, 0.0], [640.0, 0.0], [640.0, 360.0], [0.0, 360.0]])
    );
    // Three yellow handles filled, the anchor's (right) hollow.
    let yellow = [1.0, 0.9, 0.0, 1.0];
    let filled = marks
        .iter()
        .filter(|m| matches!(m, Mark::Polygon { colour, .. } if *colour == yellow))
        .count();
    assert_eq!(filled, 3);
}

#[test]
fn dragging_the_handles_crops_moves_and_turns_the_window() {
    let inst = instance(640, 360);
    let v = view();
    let pen =
        |a, at: [f64; 2]| inst.overlay_pen(a, &v, at, [at[0] as i32, 360 - at[1] as i32], 1.0);
    // Far outside the window: not the overlay's.
    assert!(!pen(PenAction::Down, [-50.0, -50.0]).taken);
    // The left edge's handle, dragged to a quarter of the width: anchored
    // right, three quarters stay, the window's middle following.
    assert!(pen(PenAction::Down, [1.0, 180.0]).taken);
    let moved = pen(PenAction::Motion, [160.0, 180.0]);
    assert!(
        moved.changed.contains(&"xCrop".to_string())
            && moved.changed.contains(&"location".to_string()),
        "{:?}",
        moved.changed
    );
    assert!(pen(PenAction::Up, [160.0, 180.0]).taken);
    assert_eq!(value(&inst, "xCrop"), [0.75]);
    assert!(near(&value(&inst, "location"), &[0.625, 0.5]));
    // The right edge's handle: the left one becomes the anchor and stays,
    // the right edge coming in to half the width.
    let down = pen(PenAction::Down, [640.0, 181.0]);
    assert!(down.taken);
    assert!(down.changed.contains(&"anchor".to_string()));
    assert_eq!(value(&inst, "anchor"), [1.0]);
    pen(PenAction::Motion, [480.0, 180.0]);
    pen(PenAction::Up, [480.0, 180.0]);
    assert_eq!(value(&inst, "xCrop"), [0.5]);
    assert!(near(&value(&inst, "location"), &[0.5, 0.5]));
    // Inside the window: dragged, it moves.
    assert!(pen(PenAction::Down, [300.0, 100.0]).taken);
    pen(PenAction::Motion, [332.0, 118.0]);
    pen(PenAction::Up, [332.0, 118.0]);
    assert!(near(&value(&inst, "location"), &[0.55, 0.55]));
    // The rotation handle (above the location), dragged to its left: a
    // quarter turn counterclockwise, an Angle of −90 (VEGAS's turns
    // clockwise).
    let handle = [352.0, 198.0 + 0.07 * 640.0];
    assert!(pen(PenAction::Down, handle).taken);
    pen(PenAction::Motion, [352.0 - 44.8, 198.0]);
    pen(PenAction::Up, [352.0 - 44.8, 198.0]);
    let angle = value(&inst, "angle")[0];
    assert!((angle + 90.0).abs() < 1e-6, "{angle}");
}
