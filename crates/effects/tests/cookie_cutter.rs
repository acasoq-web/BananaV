//! Cookie Cutter and Layer Dimensionality through BananaV's OpenFX host,
//! as any host runs them: described, rendered with their presets and
//! settings, Cookie Cutter's center dragged on the preview, Layer
//! Dimensionality's parameters greyed out by its settings and its light
//! placed in its own box and by its cross on the preview.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Context, Instance, Mark, PenAction, Plugin,
    RenderRequest, Setup, Value, View,
};

fn plugin(id: &str) -> &'static Arc<Plugin> {
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
        .find(|p| p.info.identifier == id)
        .expect(id)
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

/// A 160 × 90 frame, all of one color.
fn flat(c: [f32; 4]) -> Arc<ImageRgbaF32> {
    let mut img = ImageRgbaF32::transparent(160, 90);
    for y in 0..90 {
        for x in 0..160 {
            *img.pixel_mut(x, y) = c;
        }
    }
    Arc::new(img)
}

fn instance(id: &str) -> Instance {
    let setup = Setup {
        frame_rate: 30.0,
        duration: 100.0,
        size: (160.0, 90.0),
        pixel_aspect: 1.0,
        interactive: true,
    };
    Instance::new(plugin(id), Context::Filter, &setup).expect("an instance")
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
    match inst.params().get(name).map(|p| p.animation().value) {
        Some(Value::Num(v)) => v,
        other => panic!("{name}: {other:?}"),
    }
}

fn preset(inst: &Instance, id: &str, name: &str) {
    let p = bv_effects::presets(id)
        .into_iter()
        .find(|p| p.name == name)
        .expect(name);
    for (param, v) in &p.values {
        let bv_effects::PresetValue::Num(v) = v else {
            panic!("{v:?}")
        };
        set(inst, param, v);
    }
}

/// The pixel `x` across and `y` down from the top.
fn px(img: &ImageRgbaF32, x: u32, y: u32) -> [f32; 4] {
    img.pixel(x, y)
}

const COOKIE: &str = "org.bananav.CookieCutter";
const LAYER: &str = "org.bananav.LayerDimensionality";

#[test]
fn cookie_cutter_describes_itself_as_vegas_does() {
    let info = &plugin(COOKIE).info;
    assert_eq!(info.label, "Cookie Cutter");
    let filter = info.context(Context::Filter).unwrap();
    let labels: Vec<_> = filter.params.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Color",
            "Shape",
            "Method",
            "Feather",
            "Border",
            "Repeat X",
            "Repeat Y",
            "Size",
            "Center",
            "Stereoscopic 3D depth"
        ]
    );
    assert_eq!(filter.params[1].options.len(), 14);
    assert_eq!(filter.params[1].options[2], "Oval side");
    assert_eq!(
        filter.params[2].options,
        ["Cut away all but section", "Cut away section"]
    );
    // Stereoscopic 3D is to come: shown greyed out.
    assert!(!filter.params[9].enabled);
    assert!(info.overlay, "its center's handle on the preview");
}

#[test]
fn the_cookie_cutter_keeps_its_shape_and_draws_its_border() {
    let inst = instance(COOKIE);
    let src = flat([0.2, 0.4, 0.6, 1.0]);
    // (Default): a circle in the middle, nothing outside it.
    let out = render(&inst, &src);
    assert_eq!(px(&out, 80, 45), [0.2, 0.4, 0.6, 1.0]);
    assert_eq!(px(&out, 2, 2)[3], 0.0);
    // Square, Center, White Border: white along the inside of its edge.
    preset(&inst, COOKIE, "Square, Center, White Border");
    let out = render(&inst, &src);
    let half = (0.2 * 160f32.hypot(90.0) * std::f32::consts::PI.sqrt() / 2.0) as u32;
    let rim = px(&out, 80 + half - 3, 45);
    assert!(rim.iter().all(|&v| v > 0.95), "{rim:?}");
    assert_eq!(px(&out, 80, 45), [0.2, 0.4, 0.6, 1.0]);
    // Circle Cut Out: a hole in the middle, the picture around it.
    preset(&inst, COOKIE, "Circle Cut Out, Center, Blurred");
    let out = render(&inst, &src);
    assert!(px(&out, 80, 45)[3] < 0.05 && px(&out, 2, 2)[3] > 0.95);
    // Two Rectangles: a quarter and three quarters up, none between.
    preset(&inst, COOKIE, "Two Rectangles");
    let out = render(&inst, &src);
    assert!(px(&out, 80, 22)[3] > 0.99 && px(&out, 80, 67)[3] > 0.99);
    assert!(px(&out, 80, 45)[3] < 0.01);
}

#[test]
fn the_cookie_cutters_center_is_dragged_on_the_preview() {
    let inst = instance(COOKIE);
    let view = View {
        time: 0.0,
        render_scale: 1.0,
        pixel_scale: [1.0, 1.0],
        background: [0.0; 3],
        viewport: [160, 90],
        to_frame: None,
    };
    let marks = inst.overlay_draw(&view);
    assert!(marks.iter().any(|m| matches!(m, Mark::Lines { .. })));
    let a = inst.overlay_pen(PenAction::Down, &view, [80.0, 45.0], [80, 45], 1.0);
    assert!(a.taken);
    inst.overlay_pen(PenAction::Motion, &view, [120.0, 60.0], [120, 60], 1.0);
    inst.overlay_pen(PenAction::Up, &view, [120.0, 60.0], [120, 60], 1.0);
    let c = value(&inst, "center");
    assert!(
        (c[0] - 0.75).abs() < 1e-9 && (c[1] - 60.0 / 90.0).abs() < 1e-9,
        "{c:?}"
    );
    // Away from it, the preview keeps the pen.
    let a = inst.overlay_pen(PenAction::Down, &view, [10.0, 10.0], [10, 10], 1.0);
    assert!(!a.taken);
}

#[test]
fn layer_dimensionality_describes_itself_as_vegas_does() {
    let info = &plugin(LAYER).info;
    assert_eq!(info.label, "Layer Dimensionality");
    let filter = info.context(Context::Filter).unwrap();
    let groups: Vec<_> = filter
        .params
        .iter()
        .filter(|p| p.ty == bv_ofx::ParamType::Group)
        .map(|p| (p.label.as_str(), p.open))
        .collect();
    assert_eq!(
        groups,
        [("Shadow", false), ("Glow", false), ("Emboss", false)]
    );
    let light = filter
        .params
        .iter()
        .find(|p| p.name == "lightLocation")
        .unwrap();
    assert_eq!(light.default, Value::Num(vec![-1.0, 1.0, 3.0]));
    assert_eq!(light.interact, Some([300.0, 104.0]));
    // VEGAS's types, in its order.
    let options = |name: &str| {
        filter
            .params
            .iter()
            .find(|p| p.name == name)
            .unwrap()
            .options
            .clone()
    };
    assert_eq!(options("shadowType"), ["None", "Drop", "Inner"]);
    assert_eq!(
        options("glowType"),
        ["None", "Outer", "Inner", "Edge", "Dual"]
    );
    assert!(info.overlay, "the light's cross on the preview");
}

#[test]
fn layer_dimensionality_greys_out_what_its_settings_turn_off() {
    let inst = instance(LAYER);
    let enabled = |name: &str| {
        inst.param_infos()
            .into_iter()
            .find(|p| p.name == name)
            .expect(name)
            .enabled
    };
    // Nothing on: the shadow's, the glow's, the emboss's and the light's
    // numbers greyed out.
    for name in [
        "shadowHeight",
        "glowAmount",
        "lightIntensity",
        "lightLocation",
    ] {
        assert!(!enabled(name), "{name}");
    }
    assert!(enabled("shadowType") && enabled("embossForeground"));
    assert!(enabled("embossBackground"));
    // A dropped shadow: its numbers and the light come on; nothing around
    // the picture to emboss.
    set(&inst, "shadowType", &[1.0]);
    inst.changed(&["shadowType".into()], ChangeReason::User, 0.0);
    assert!(enabled("shadowHeight") && enabled("lightLocation"));
    assert!(!enabled("glowAmount"));
    assert!(enabled("embossForeground") && !enabled("embossBackground"));
    // An inner one: the picture is a hole, nothing raised.
    set(&inst, "shadowType", &[2.0]);
    inst.changed(&["shadowType".into()], ChangeReason::User, 0.0);
    assert!(!enabled("embossForeground") && enabled("embossBackground"));
    assert!(!enabled("lightIntensity"));
    // What is around it pressed in: the light's numbers come on; a
    // foreground the inner shadow turns off leaves them off.
    set(&inst, "embossForeground", &[0.5]);
    inst.changed(&["embossForeground".into()], ChangeReason::User, 0.0);
    assert!(!enabled("lightIntensity"));
    set(&inst, "embossBackground", &[-1.0]);
    inst.changed(&["embossBackground".into()], ChangeReason::User, 0.0);
    assert!(enabled("lightIntensity") && enabled("shininess"));
    // No shadow and nothing embossed: the light greyed out again.
    set(&inst, "shadowType", &[0.0]);
    set(&inst, "embossForeground", &[0.0]);
    set(&inst, "embossBackground", &[0.0]);
    inst.changed(&["shadowType".into()], ChangeReason::User, 0.0);
    assert!(!enabled("lightLocation") && !enabled("lightIntensity"));
}

#[test]
fn layer_dimensionality_shades_a_cut_out_picture() {
    let inst = instance(LAYER);
    // A white square in the middle of a clear frame.
    let mut img = ImageRgbaF32::transparent(160, 90);
    for y in 30..60 {
        for x in 65..95 {
            *img.pixel_mut(x, y) = [1.0; 4];
        }
    }
    let img = Arc::new(img);
    // At its defaults: as it was.
    assert_eq!(render(&inst, &img).data, img.data);
    // A dropped shadow, far: the light up and left (-1, 1, 3), the shadow
    // a sixth of the frame right (Height 0.5 over 3) and a sixth down —
    // dark below right of the square, the square as it was.
    set(&inst, "shadowType", &[1.0]);
    set(&inst, "shadowHeight", &[0.5]);
    set(&inst, "shadowBlur", &[0.0]);
    let out = render(&inst, &img);
    let shade = px(&out, 110, 65);
    assert!(shade[3] > 0.9 && shade[0] < 0.01, "{shade:?}");
    assert_eq!(px(&out, 120, 74)[3], shade[3]);
    assert_eq!(px(&out, 123, 65)[3], 0.0, "it ends 27 pixels right");
    assert_eq!(px(&out, 80, 45), [1.0; 4]);
    // Inside instead: the square's upper left edges darkened, a tenth as
    // far; nothing outside.
    set(&inst, "shadowType", &[2.0]);
    let out = render(&inst, &img);
    let edge = px(&out, 65, 45);
    assert!(edge[0] < 0.2 && edge[3] == 1.0, "{edge:?}");
    assert!(px(&out, 93, 45)[0] > 0.99 && px(&out, 80, 45)[0] > 0.99);
    assert_eq!(px(&out, 110, 65), [0.0; 4]);
    // The glow's color around it instead.
    set(&inst, "shadowType", &[0.0]);
    set(&inst, "glowType", &[1.0]);
    set(&inst, "glowColor", &[0.0, 1.0, 0.0]);
    set(&inst, "glowAmount", &[1.0]);
    let out = render(&inst, &img);
    let g = px(&out, 97, 45);
    assert!(g[1] > 0.2 && g[0] < 0.01, "{g:?}");
    // Hugging the edge: the same (VEGAS's look alike).
    set(&inst, "glowType", &[3.0]);
    assert_eq!(px(&render(&inst, &img), 97, 45), g);
}

#[test]
fn layer_dimensionalitys_presets_are_vegass() {
    let p = bv_effects::presets(LAYER);
    assert_eq!(p.len(), 18);
    let inst = instance(LAYER);
    let mut img = ImageRgbaF32::transparent(160, 90);
    for y in 30..60 {
        for x in 65..95 {
            *img.pixel_mut(x, y) = [1.0; 4];
        }
    }
    let img = Arc::new(img);
    // Each renders: the square's middle as it was where nothing covers it.
    for preset in &p {
        let fresh = instance(LAYER);
        self::preset(&fresh, LAYER, preset.name);
        let out = render(&fresh, &img);
        assert_eq!(px(&out, 80, 45)[3], 1.0, "{}", preset.name);
    }
    // Inner Shadow Top Left Light: the upper left edge dark, the lower
    // right one not.
    self::preset(&inst, LAYER, "Inner Shadow Top Left Light");
    let out = render(&inst, &img);
    assert!(px(&out, 65, 30)[0] < 0.5 && px(&out, 92, 57)[0] > 0.9);
}

#[test]
fn the_lights_cross_moves_it_on_the_preview() {
    let inst = instance(LAYER);
    let view = View {
        time: 0.0,
        render_scale: 1.0,
        pixel_scale: [1.0, 1.0],
        background: [0.0; 3],
        viewport: [160, 90],
        to_frame: None,
    };
    // The light does nothing yet: no cross, the pen left to the preview.
    assert!(inst.overlay_draw(&view).is_empty());
    let a = inst.overlay_pen(PenAction::Down, &view, [53.3, 60.0], [53, 60], 1.0);
    assert!(!a.taken);
    // With a shadow: the cross where the light is (-1, 1: a third of the
    // frame from its upper left), dragged to the frame's right edge.
    set(&inst, "shadowType", &[1.0]);
    assert!(
        inst.overlay_draw(&view)
            .iter()
            .any(|m| matches!(m, Mark::Lines { .. }))
    );
    let at = [160.0 / 3.0, 90.0 * 2.0 / 3.0];
    let a = inst.overlay_pen(PenAction::Down, &view, at, [53, 60], 1.0);
    assert!(a.taken);
    inst.overlay_pen(PenAction::Motion, &view, [200.0, at[1]], [200, 60], 1.0);
    inst.overlay_pen(PenAction::Up, &view, [200.0, at[1]], [200, 60], 1.0);
    let v = value(&inst, "lightLocation");
    assert!(
        (v[0] - 3.0).abs() < 1e-9 && (v[1] - 1.0).abs() < 1e-9 && v[2] == 3.0,
        "{v:?}"
    );
}

#[test]
fn the_lights_box_places_the_light() {
    let inst = instance(LAYER);
    // Its box is greyed out but drawn.
    let view = View {
        time: 0.0,
        render_scale: 1.0,
        pixel_scale: [1.0, 1.0],
        background: [0.16; 3],
        viewport: [300, 104],
        to_frame: None,
    };
    let marks = inst.param_draw("lightLocation", &view);
    assert!(marks.iter().any(|m| matches!(m, Mark::Polygon { .. })));
    // The box's middle: the light right over the frame's middle.
    let side = 96.0;
    let x0 = 300.0 * 0.55 - side / 2.0;
    let centre = [x0 + side / 2.0, 52.0];
    let a = inst.param_pen(
        "lightLocation",
        PenAction::Down,
        &view,
        centre,
        [165, 52],
        1.0,
    );
    assert!(a.taken && a.changed.contains(&"lightLocation".to_string()));
    let v = value(&inst, "lightLocation");
    assert!(
        v[0].abs() < 1e-9 && v[1].abs() < 1e-9 && v[2] == 3.0,
        "{v:?}"
    );
    inst.param_pen(
        "lightLocation",
        PenAction::Up,
        &view,
        centre,
        [165, 52],
        1.0,
    );
    // The slider's top: the light at its highest (6, as VEGAS's).
    let slider = [x0 + side + 36.0, 4.0 + side];
    inst.param_pen("lightLocation", PenAction::Down, &view, slider, [0, 0], 1.0);
    inst.param_pen("lightLocation", PenAction::Up, &view, slider, [0, 0], 1.0);
    assert_eq!(value(&inst, "lightLocation")[2], 6.0);
    // Pressed there again at once, a double click: back to its middle, 3.
    inst.param_pen("lightLocation", PenAction::Down, &view, slider, [0, 0], 1.0);
    inst.param_pen("lightLocation", PenAction::Up, &view, slider, [0, 0], 1.0);
    assert_eq!(value(&inst, "lightLocation"), [0.0, 0.0, 3.0]);
    // One on the box: the whole light where it starts.
    let corner = [x0 + 2.0, 6.0];
    for _ in 0..2 {
        inst.param_pen("lightLocation", PenAction::Down, &view, corner, [0, 0], 1.0);
        inst.param_pen("lightLocation", PenAction::Up, &view, corner, [0, 0], 1.0);
    }
    assert_eq!(value(&inst, "lightLocation"), [-1.0, 1.0, 3.0]);
}
