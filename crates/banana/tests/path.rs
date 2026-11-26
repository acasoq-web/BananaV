//! Lightning's path on the preview, through BananaV's OpenFX host: places
//! put in, dragged and taken out, with their keys, and asked for by number.

use std::sync::{Arc, OnceLock};

use bv_banana::fx::POINTS;
use bv_ofx::interact::{ALT, CONTROL, set_modifiers};
use bv_ofx::{
    Animation, Binary, ChangeReason, Context, Instance, Interp, Mark, PenAction, Plugin, Setup,
    Value, View,
};

const SIZE: (f64, f64) = (1920.0, 1080.0);

fn plugin() -> &'static Arc<Plugin> {
    static PLUGINS: OnceLock<Vec<Arc<Plugin>>> = OnceLock::new();
    PLUGINS
        .get_or_init(|| {
            let bin = Binary::built_in(
                bv_banana::BINARY,
                bv_banana::number_of_plugins,
                bv_banana::get_plugin,
                None,
            );
            let (plugins, failures) = bin.plugins().expect("the pack loads");
            assert!(failures.is_empty(), "{failures:?}");
            plugins
        })
        .iter()
        .find(|p| p.info.identifier == "org.bananav.bananasplit.Lightning")
        .expect("Lightning")
}

fn lightning() -> Instance {
    let setup = Setup {
        frame_rate: 30.0,
        duration: 90.0,
        size: SIZE,
        pixel_aspect: 1.0,
        interactive: true,
    };
    let inst = Instance::new(plugin(), Context::Generator, &setup).expect("an instance");
    inst.editing(true);
    inst
}

fn view(time: f64) -> View {
    View {
        time,
        render_scale: 1.0,
        pixel_scale: [1.0, 1.0],
        background: [0.0; 3],
        viewport: [1920, 1080],
        to_frame: None,
    }
}

fn at(inst: &Instance, name: &str, time: f64) -> Vec<f64> {
    let (_, a) = inst
        .values()
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("{name}"));
    let ty = inst
        .param_infos()
        .into_iter()
        .find(|p| p.name == name)
        .expect(name)
        .ty;
    match a.at(time, ty) {
        Value::Num(v) => v,
        v => panic!("{name}: {v:?}"),
    }
}

fn place(inst: &Instance, name: &str) -> [f64; 2] {
    let v = at(inst, name, 0.0);
    [v[0], v[1]]
}

fn count(inst: &Instance) -> usize {
    at(inst, "points", 0.0)[0] as usize
}

fn set(inst: &Instance, name: &str, v: &[f64]) {
    inst.set_values(&[(name.into(), Animation::constant(Value::Num(v.to_vec())))]);
}

/// The pen, at a part of the frame.
fn pen(inst: &Instance, action: PenAction, [x, y]: [f64; 2]) -> bool {
    let p = [x * SIZE.0, y * SIZE.1];
    inst.overlay_pen(
        action,
        &view(0.0),
        p,
        [p[0] as i32, (SIZE.1 - p[1]) as i32],
        1.0,
    )
    .taken
}

fn drag(inst: &Instance, from: [f64; 2], to: [f64; 2]) -> bool {
    let taken = pen(inst, PenAction::Down, from);
    let half = [(from[0] + to[0]) * 0.5, (from[1] + to[1]) * 0.5];
    pen(inst, PenAction::Motion, half);
    pen(inst, PenAction::Motion, to);
    pen(inst, PenAction::Up, to);
    taken
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 2e-3 && (a[1] - b[1]).abs() < 2e-3
}

fn shown(inst: &Instance) -> Vec<String> {
    inst.param_infos()
        .into_iter()
        .filter(|p| !p.secret && POINTS.contains(&p.name.as_str()))
        .map(|p| p.name)
        .collect()
}

/// One after another: the modifier keys are the interface's, one set for
/// all overlays.
#[test]
fn a_path_s_places_are_put_in_dragged_and_taken_out() {
    let inst = lightning();
    set(&inst, "from", &[0.1, 0.5]);
    set(&inst, "to", &[0.9, 0.5]);
    assert_eq!(count(&inst), 0);
    assert!(shown(&inst).is_empty(), "{:?}", shown(&inst));
    let marks = inst.overlay_draw(&view(0.0));
    assert!(marks.iter().any(|m| matches!(m, Mark::Lines { .. })));
    // A press on the small plus adds nothing; dragged, it does.
    assert!(pen(&inst, PenAction::Down, [0.5, 0.5]));
    pen(&inst, PenAction::Up, [0.5, 0.5]);
    assert_eq!(count(&inst), 0);
    std::thread::sleep(std::time::Duration::from_millis(600));
    assert!(drag(&inst, [0.5, 0.5], [0.5, 0.8]));
    assert_eq!(count(&inst), 1);
    assert!(near(place(&inst, "point1"), [0.5, 0.8]));
    assert_eq!(shown(&inst), ["point1"]);
    // Dragged again.
    assert!(drag(&inst, [0.5, 0.8], [0.45, 0.75]));
    assert!(near(place(&inst, "point1"), [0.45, 0.75]));
    // Keyed, it keeps its keys when another goes before it.
    let keyed = |inst: &Instance, name: &str| {
        let (_, a) = inst
            .values()
            .into_iter()
            .find(|(n, _)| n == name)
            .expect(name);
        a.keys.len()
    };
    let mut a = Animation::constant(Value::Num(vec![0.45, 0.75]));
    a.set_key(0.0, Value::Num(vec![0.45, 0.75]), Interp::Linear);
    a.set_key(30.0, Value::Num(vec![0.6, 0.9]), Interp::Linear);
    inst.set_values(&[("point1".into(), a)]);
    // Twice on the line between the start and the first place.
    std::thread::sleep(std::time::Duration::from_millis(600));
    let g = bv_banana::bolt::Guide::through(
        &[
            [0.1 * 16.0 / 9.0, 0.5],
            [0.45 * 16.0 / 9.0, 0.75],
            [0.9 * 16.0 / 9.0, 0.5],
        ],
        true,
    );
    let on = g.at(g.marks()[1] * 0.3);
    let on = [f64::from(on[0]) * 9.0 / 16.0, f64::from(on[1])];
    let first = pen(&inst, PenAction::Down, on);
    pen(&inst, PenAction::Up, on);
    assert!(!first, "a press on the line alone is not the path's");
    assert!(pen(&inst, PenAction::Down, on));
    pen(&inst, PenAction::Up, on);
    assert_eq!(count(&inst), 2);
    assert!(
        near(place(&inst, "point1"), on),
        "{:?}",
        place(&inst, "point1")
    );
    assert!(near(place(&inst, "point2"), [0.45, 0.75]));
    assert_eq!((keyed(&inst, "point1"), keyed(&inst, "point2")), (0, 2));
    assert_eq!(at(&inst, "point2", 30.0), [0.6, 0.9]);
    assert_eq!(shown(&inst), ["point1", "point2"]);
    // Control takes one away: the other comes back as it was.
    std::thread::sleep(std::time::Duration::from_millis(600));
    set_modifiers(CONTROL);
    assert!(pen(&inst, PenAction::Down, on));
    pen(&inst, PenAction::Up, on);
    set_modifiers(0);
    assert_eq!(count(&inst), 1);
    assert_eq!(keyed(&inst, "point1"), 2);
    assert_eq!(at(&inst, "point1", 30.0), [0.6, 0.9]);
    assert_eq!(shown(&inst), ["point1"]);
    // The path's ends stay.
    set_modifiers(ALT);
    pen(&inst, PenAction::Down, [0.1, 0.5]);
    pen(&inst, PenAction::Up, [0.1, 0.5]);
    set_modifiers(0);
    assert_eq!(count(&inst), 1);
    // Twice on a place takes it away too.
    std::thread::sleep(std::time::Duration::from_millis(600));
    pen(&inst, PenAction::Down, [0.45, 0.75]);
    pen(&inst, PenAction::Up, [0.45, 0.75]);
    assert_eq!(count(&inst), 1);
    pen(&inst, PenAction::Down, [0.45, 0.75]);
    pen(&inst, PenAction::Up, [0.45, 0.75]);
    assert_eq!(count(&inst), 0);

    // As many as fit, and no more.
    let inst = lightning();
    set(&inst, "from", &[0.02, 0.5]);
    set(&inst, "to", &[0.98, 0.5]);
    for n in 0..12 {
        // Halfway along the last gap, if it is wide enough for a plus.
        let last = if n == 0 {
            0.02
        } else {
            place(&inst, POINTS[count(&inst) - 1])[0]
        };
        let mid = [(last + 0.98) * 0.5, 0.5];
        drag(&inst, mid, [last + 0.08, 0.5]);
        assert_eq!(count(&inst), (n + 1).min(POINTS.len()));
    }
    assert_eq!(shown(&inst).len(), POINTS.len());
    // In their order along it.
    let xs: Vec<f64> = POINTS.iter().map(|p| place(&inst, p)[0]).collect();
    assert!(xs.windows(2).all(|w| w[0] < w[1]), "{xs:?}");
}

/// Asked for by their number alone, places lie along the way.
#[test]
fn places_asked_for_by_number_lie_along_the_way() {
    let inst = lightning();
    set(&inst, "from", &[0.1, 0.1]);
    set(&inst, "to", &[0.9, 0.5]);
    let ask = |n: f64| {
        set(&inst, "points", &[n]);
        inst.changed(&["points".into()], ChangeReason::User, 0.0);
    };
    ask(3.0);
    assert!(near(place(&inst, "point1"), [0.3, 0.2]));
    assert!(near(place(&inst, "point2"), [0.5, 0.3]));
    assert!(near(place(&inst, "point3"), [0.7, 0.4]));
    assert_eq!(shown(&inst).len(), 3);
    // Fewer, then more: they are where they were.
    set(&inst, "point2", &[0.5, 0.9]);
    ask(1.0);
    assert_eq!(shown(&inst), ["point1"]);
    ask(4.0);
    assert!(near(place(&inst, "point2"), [0.5, 0.9]));
    assert!(near(place(&inst, "point4"), [0.8, 0.45]));
    // An agent's places are left alone.
    let inst = lightning();
    set(&inst, "point1", &[0.2, 0.8]);
    set(&inst, "points", &[1.0]);
    inst.changed(&["points".into()], ChangeReason::User, 0.0);
    assert!(near(place(&inst, "point1"), [0.2, 0.8]));
}
