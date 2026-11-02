//! Picture In Picture through BananaV's OpenFX host, as any host runs it:
//! described as VEGAS's, its settings greyed out by mode, its corners and
//! its other settings kept together, the picture drawn smaller over a
//! transparent frame or by its corners, and handled on the preview.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Context, Instance, PenAction, Plugin,
    RenderRequest, Setup, Value, View,
};

const PIP: &str = "org.bananav.PictureInPicture";

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
        .find(|p| p.info.identifier == PIP)
        .expect(PIP)
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
fn set(inst: &Instance, name: &str, v: &[f64]) {
    inst.set_values(&[(name.into(), Animation::constant(Value::Num(v.to_vec())))]);
    inst.changed(&[name.into()], ChangeReason::User, 0.0);
}

fn enabled(inst: &Instance, name: &str) -> bool {
    inst.param_infos()
        .into_iter()
        .find(|p| p.name == name)
        .expect(name)
        .enabled
}

fn near(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
}

#[test]
fn picture_in_picture_describes_itself_as_vegas_does() {
    let info = &plugin().info;
    assert_eq!(info.label, "Picture In Picture");
    assert_eq!(info.grouping, "BananaV/Utility");
    assert!(info.overlay, "its handles on the preview");
    let filter = info.context(Context::Filter).unwrap();
    let labels: Vec<_> = filter.params.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Location",
            "Angle",
            "Scale in X",
            "Scale in Y",
            "Mode",
            "Shear",
            "Opacity",
            "Corner points",
            "Top left",
            "Top right",
            "Bottom left",
            "Bottom right"
        ]
    );
    let param = |label: &str| filter.params.iter().find(|p| p.label == label).unwrap();
    // The scales' sliders: logarithmic, 1 in their middle, 0.5 at first.
    let scale = param("Scale in X");
    assert!(scale.logarithmic);
    assert_eq!(
        (
            scale.display_min[0],
            scale.display_max[0],
            scale.default.num(0)
        ),
        (0.1, 10.0, 0.5)
    );
    assert_eq!(
        param("Mode").options,
        ["Fixed Shape", "Fixed Parallelogram", "Free Form"]
    );
    // The corners at first: the default picture's.
    assert_eq!(param("Top left").default.num(0), 0.25);
    assert_eq!(param("Top left").default.num(1), 0.75);
    let names: Vec<_> = bv_effects::presets(PIP).iter().map(|p| p.name).collect();
    assert_eq!(
        names,
        [
            "Upper Left, Scaled, No Angle, Opaque",
            "Upper Right, Scaled, No Angle, Opaque",
            "Lower Left, Scaled, No Angle, Opaque",
            "Lower Right, Scaled, No Angle, Opaque",
            "Middle Left, Scaled, Angled, Translucent"
        ]
    );
}

#[test]
fn the_mode_greys_out_what_it_does_not_use() {
    let inst = instance(64, 36);
    // Fixed Shape: no height of its own, no shear, no pins.
    assert!(!enabled(&inst, "scaleY") && !enabled(&inst, "shear"));
    assert!(!enabled(&inst, "topLeft"));
    // Its height follows its width.
    set(&inst, "scaleX", &[0.25]);
    assert_eq!(value(&inst, "scaleY"), [0.25]);
    // Fixed Parallelogram: the height and the shear.
    set(&inst, "mode", &[1.0]);
    assert!(enabled(&inst, "scaleY") && enabled(&inst, "shear"));
    assert!(!enabled(&inst, "bottomRight"));
    set(&inst, "scaleX", &[0.4]);
    assert_eq!(value(&inst, "scaleY"), [0.25]);
    // Free Form: the pins too.
    set(&inst, "mode", &[2.0]);
    assert!(enabled(&inst, "topLeft") && enabled(&inst, "bottomRight"));
}

/// A frame: its left half red, its right half blue.
fn halves() -> Arc<ImageRgbaF32> {
    let mut img = ImageRgbaF32::transparent(64, 36);
    for y in 0..36 {
        for x in 0..64 {
            *img.pixel_mut(x, y) = if x < 32 {
                [1.0, 0.0, 0.0, 1.0]
            } else {
                [0.0, 0.0, 1.0, 1.0]
            };
        }
    }
    Arc::new(img)
}

fn close(a: [f32; 4], b: [f32; 4]) -> bool {
    (0..4).all(|i| (a[i] - b[i]).abs() < 1e-4)
}

#[test]
fn the_picture_shrinks_into_the_middle_of_a_transparent_frame() {
    let src = halves();
    let inst = instance(64, 36);
    // At first: half the frame, in its middle (x 16–48, y 9–27).
    let out = render(&inst, &src);
    assert!(close(out.pixel(4, 4), [0.0; 4]), "{:?}", out.pixel(4, 4));
    assert!(
        close(out.pixel(20, 18), [1.0, 0.0, 0.0, 1.0]),
        "{:?}",
        out.pixel(20, 18)
    );
    assert!(
        close(out.pixel(43, 18), [0.0, 0.0, 1.0, 1.0]),
        "{:?}",
        out.pixel(43, 18)
    );
    assert!(close(out.pixel(60, 30), [0.0; 4]));
    // Its edges soft: half a pixel further right, the edge's pixel half
    // covered.
    set(&inst, "location", &[0.5 + 0.5 / 64.0, 0.5]);
    let edge = render(&inst, &src).pixel(16, 18)[3];
    assert!(edge > 0.4 && edge < 0.6, "{edge}");
    set(&inst, "location", &[0.5, 0.5]);
    // Half opaque.
    set(&inst, "opacity", &[0.5]);
    let px = render(&inst, &src).pixel(20, 18);
    assert!(close(px, [0.5, 0.0, 0.0, 0.5]), "{px:?}");
    // VEGAS's Upper Right: a third of the frame up there (rows 3–15 from
    // the top).
    set(&inst, "opacity", &[1.0]);
    set(&inst, "location", &[0.75, 0.75]);
    set(&inst, "scaleX", &[1.0 / 3.0]);
    let out = render(&inst, &src);
    assert!(
        close(out.pixel(42, 8), [1.0, 0.0, 0.0, 1.0]),
        "{:?}",
        out.pixel(42, 8)
    );
    assert!(close(out.pixel(53, 8), [0.0, 0.0, 1.0, 1.0]));
    assert!(close(out.pixel(42, 27), [0.0; 4]));
    assert!(close(out.pixel(20, 18), [0.0; 4]));
}

#[test]
fn free_form_pins_the_corners() {
    let src = halves();
    let inst = instance(64, 36);
    set(&inst, "mode", &[2.0]);
    // Its pins where they start: the same picture.
    let out = render(&inst, &src);
    assert!(close(out.pixel(20, 18), [1.0, 0.0, 0.0, 1.0]));
    // The top right pin at the frame's corner: the picture reaches it
    // (rows counted from the top).
    set(&inst, "topRight", &[1.0, 1.0]);
    let out = render(&inst, &src);
    assert!(out.pixel(61, 2)[3] > 0.9, "{:?}", out.pixel(61, 2));
    assert!(out.pixel(61, 2)[2] > 0.9, "the blue half");
    // The others stayed.
    assert!(close(out.pixel(4, 4), [0.0; 4]));
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
fn dragging_the_handles_moves_scales_and_turns_the_picture() {
    let inst = instance(640, 360);
    assert!(inst.has_overlay());
    assert!(!inst.overlay_draw(&view()).is_empty());
    let v = view();
    let pen =
        |a, at: [f64; 2]| inst.overlay_pen(a, &v, at, [at[0] as i32, 360 - at[1] as i32], 1.0);
    // Outside the picture: not the overlay's.
    assert!(!pen(PenAction::Down, [20.0, 20.0]).taken);
    // Inside: dragged, it moves.
    assert!(pen(PenAction::Down, [250.0, 130.0]).taken);
    pen(PenAction::Motion, [314.0, 166.0]);
    pen(PenAction::Up, [314.0, 166.0]);
    assert!(near(&value(&inst, "location"), &[0.6, 0.6]));
    // The top right corner (Fixed Shape), dragged twice as far from the
    // middle: twice as big, both ways.
    let middle = [384.0, 216.0];
    let corner = [middle[0] + 160.0, middle[1] + 90.0];
    assert!(pen(PenAction::Down, corner).taken);
    let far = [middle[0] + 320.0, middle[1] + 180.0];
    pen(PenAction::Motion, far);
    pen(PenAction::Up, far);
    assert!(
        near(&value(&inst, "scaleX"), &[1.0]),
        "{:?}",
        value(&inst, "scaleX")
    );
    assert!(near(&value(&inst, "scaleY"), &[1.0]));
    // The ring's handle (above the middle), dragged to its right and
    // twice as far: a quarter turn clockwise, twice as big.
    let handle = [middle[0], middle[1] + 0.07 * 640.0];
    assert!(pen(PenAction::Down, handle).taken);
    pen(PenAction::Motion, [middle[0] + 89.6, middle[1]]);
    pen(PenAction::Up, [middle[0] + 89.6, middle[1]]);
    let angle = value(&inst, "angle")[0];
    assert!((angle - 90.0).abs() < 1e-6, "{angle}");
    assert!(
        near(&value(&inst, "scaleX"), &[2.0]),
        "{:?}",
        value(&inst, "scaleX")
    );
    // The corners followed the picture all along: the top left one, two
    // frame heights up and one across from the middle once turned, in the
    // frame's proportions (16:9).
    let tl = value(&inst, "topLeft");
    assert!(near(&tl, &[0.6 + 9.0 / 16.0, 0.6 + 16.0 / 9.0]), "{tl:?}");
}

#[test]
fn the_corners_and_the_other_settings_stay_together() {
    let inst = instance(1280, 720);
    // Turned 45° (VEGAS's picture): the corners, greyed out, follow.
    set(&inst, "angle", &[45.0]);
    let tl = value(&inst, "topLeft");
    assert!(
        (tl[0] - 0.423).abs() < 6e-4 && (tl[1] - 0.991).abs() < 6e-4,
        "{tl:?}"
    );
    set(&inst, "angle", &[0.0]);
    // Free Form: a corner dragged by hand, the location becomes the
    // corners' mean (VEGAS's 0.550, 0.536).
    set(&inst, "mode", &[2.0]);
    set(&inst, "topRight", &[0.948, 0.894]);
    let loc = value(&inst, "location");
    assert!(near(&loc, &[0.5495, 0.536]), "{loc:?}");
    // The scale changed by hand: the corners grow about the middle.
    let before = value(&inst, "bottomLeft");
    let x = value(&inst, "scaleX")[0];
    set(&inst, "scaleX", &[2.0 * x]);
    let after = value(&inst, "bottomLeft");
    let across = |v: &[f64]| v[0] - loc[0];
    assert!(
        (across(&after) - 2.0 * across(&before)).abs() < 0.05,
        "{before:?} {after:?}"
    );
    assert!(!near(&value(&inst, "topRight"), &[0.948, 0.894]));
}
