//! Color Gradient's page, as VEGAS's: the Control Points box — the
//! gradient over the frame, its center and its numbered points on the
//! angle's axis, a click choosing one and a drag moving it (the center
//! takes the others along; a point slides on the axis) —, the bar that
//! adds, deletes, chooses and reorders points, and the chosen point's own
//! settings: the center's place, or a point's distance and color.
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_int, c_void};

use bv_ofx_sys::extensions::kBananaVParamPropInteract;
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::color_gradient;
use crate::kit::{Spec, define};
use crate::ofx::{Draw, Fx, Props, interact_props, is};

/// The group holding the page's box, its bar and the chosen point.
pub(crate) const GROUP: &CStr = c"controlPointsEditor";
const COUNT: &CStr = c"controlPoints";
const CENTER: &CStr = c"center";
const TYPE: &CStr = c"gradientType";
const ANGLE: &CStr = c"aspectRatioAngle";

/// The bar's buttons: a point added (after the others, at 70 % of the way
/// to the chosen one, in its color), the chosen one deleted, the one
/// before or after chosen, the chosen one moved a place back or forward.
const ADD: &CStr = c"addPoint";
const DELETE: &CStr = c"deletePoint";
const PREVIOUS: &CStr = c"previousPoint";
const NEXT: &CStr = c"nextPoint";
const EARLIER: &CStr = c"movePointBack";
const LATER: &CStr = c"movePointForward";
/// The bar's buttons' names, for the instance's changes.
pub(crate) const BAR_NAMES: [&CStr; 6] = [ADD, DELETE, PREVIOUS, NEXT, EARLIER, LATER];
const BAR: [(&CStr, &CStr, &CStr); 6] = [
    (
        ADD,
        c"Add",
        c"Adds a point after the others, in the chosen one's color.",
    ),
    (
        DELETE,
        c"Delete",
        c"Deletes the chosen point (two stay at least).",
    ),
    (PREVIOUS, c"Previous", c"Chooses the point before."),
    (NEXT, c"Next", c"Chooses the point after."),
    (
        EARLIER,
        c"Back",
        c"Moves the chosen point a place back in the list.",
    ),
    (
        LATER,
        c"Forward",
        c"Moves the chosen point a place forward in the list.",
    ),
];

/// Each color point's distance and color, as [`crate::generators`]
/// describes them.
pub(crate) const POINTS: [(&CStr, &CStr); 9] = [
    (c"distance1", c"color1"),
    (c"distance2", c"color2"),
    (c"distance3", c"color3"),
    (c"distance4", c"color4"),
    (c"distance5", c"color5"),
    (c"distance6", c"color6"),
    (c"distance7", c"color7"),
    (c"distance8", c"color8"),
    (c"distance9", c"color9"),
];

/// The box's size in the panel, in screen pixels.
const BOX: [c_int; 2] = [320, 190];
/// How near the pen a point is grabbed (pixels).
const REACH: f64 = 9.0;
/// The gradient's cells in the box.
const CELLS: [usize; 2] = [64, 36];

/// What the page remembers while it is open: the chosen point (0 the
/// center).
#[derive(Debug)]
pub(crate) struct Page {
    pub chosen: usize,
}

impl Default for Page {
    fn default() -> Page {
        Page { chosen: 1 }
    }
}

/// The page of instance `fx`, made when it is.
fn page(fx: Fx) -> Option<&'static mut Page> {
    let p = fx.props().ptr(kOfxPropInstanceData) as *mut Page;
    // SAFETY: made in `created` and kept until `destroyed`; hosts run one
    // of an instance's actions at a time.
    (!p.is_null()).then(|| unsafe { &mut *p })
}

/// An instance made: its page.
pub(crate) fn created(fx: Fx) {
    if page(fx).is_none() {
        let p = Box::into_raw(Box::new(Page::default()));
        fx.props().set_ptr(kOfxPropInstanceData, p as *mut c_void);
    }
}

/// An instance gone: its page too.
pub(crate) fn destroyed(fx: Fx) {
    let p = fx.props().ptr(kOfxPropInstanceData) as *mut Page;
    if !p.is_null() {
        // SAFETY: made in `created`, freed once.
        drop(unsafe { Box::from_raw(p) });
        fx.props()
            .set_ptr(kOfxPropInstanceData, std::ptr::null_mut());
    }
}

/// Defines the page's group (with its box), its bar and the points'
/// settings `rest` (the center, the count and the points), all in it.
pub(crate) fn define_page(fx: Fx, rest: &[Spec]) {
    let g = fx.define_param(kOfxParamTypeGroup, GROUP, c"Control Points");
    g.set_int(kOfxParamPropGroupOpen, 1);
    g.set_ptr(kBananaVParamPropInteract, points_box as *mut c_void);
    g.set_ints(kOfxParamPropInteractPreferedSize, &BOX);
    for (i, (name, label, hint)) in BAR.iter().enumerate() {
        let b = fx.define_param(kOfxParamTypePushButton, name, label);
        b.set_str(kOfxParamPropParent, GROUP);
        b.set_str(kOfxParamPropHint, hint);
        if i + 1 < BAR.len() {
            b.set_int(kOfxParamPropLayoutHint, kOfxParamPropLayoutHintNoNewLine);
        }
    }
    for &spec in rest {
        if let Some(p) = define(fx, spec) {
            p.set_str(kOfxParamPropParent, GROUP);
            // The count follows the bar's buttons.
            if matches!(spec, Spec::Int(name, ..) if name == COUNT) {
                p.set_int(kOfxParamPropSecret, 1);
            }
        }
    }
}

/// How many color points there are (the center apart).
fn count(fx: Fx, time: f64) -> usize {
    fx.param(COUNT).int_at(time).clamp(1, 9) as usize
}

/// The page as the chosen point has it: only its settings shown, the
/// bar's buttons that can do something enabled.
pub(crate) fn shown(fx: Fx, time: f64) {
    let n = count(fx, time);
    let chosen = page(fx).map_or(1, |p| {
        p.chosen = p.chosen.min(n);
        p.chosen
    });
    fx.param(CENTER).set_secret(chosen != 0);
    for (i, (d, c)) in POINTS.iter().enumerate() {
        let hidden = i + 1 != chosen;
        fx.param(d).set_secret(hidden);
        fx.param(c).set_secret(hidden);
    }
    // Ten points at most, the center among them; two at least.
    fx.param(ADD).set_enabled(n < 9);
    fx.param(DELETE).set_enabled(chosen != 0 && n >= 2);
    fx.param(PREVIOUS).set_enabled(chosen > 0);
    fx.param(NEXT).set_enabled(chosen < n);
    fx.param(EARLIER).set_enabled(chosen > 1);
    fx.param(LATER).set_enabled(chosen >= 1 && chosen < n);
}

/// A point's distance and color.
fn point(fx: Fx, time: f64, i: usize) -> (f64, [f64; 4]) {
    let (d, c) = POINTS[i];
    (fx.param(d).f64_at(time), fx.param(c).rgba_at(time))
}

fn set_point(fx: Fx, i: usize, (d, c): (f64, [f64; 4])) {
    let (dn, cn) = POINTS[i];
    fx.param(dn).set_f64(d);
    fx.param(cn).set_rgba(c);
}

/// A bar button `name` pressed: the points as it says. Whether it was one.
pub(crate) fn pressed(fx: Fx, time: f64, name: &CStr) -> bool {
    if !BAR.iter().any(|b| b.0 == name) {
        return false;
    }
    let Some(page) = page(fx) else {
        return true;
    };
    let n = count(fx, time);
    let chosen = page.chosen.min(n);
    if name == ADD && n < 9 {
        // In the chosen point's color, 70 % of the way to it from the
        // center (the first point's color for the center).
        let (d, c) = point(fx, time, chosen.max(1) - 1);
        let d = if chosen == 0 { 0.0 } else { d * 0.7 };
        set_point(fx, n, (d, c));
        fx.param(COUNT).set_int((n + 1) as c_int);
        page.chosen = n + 1;
    } else if name == DELETE && chosen >= 1 && n >= 2 {
        for i in chosen..n {
            let next = point(fx, time, i);
            set_point(fx, i - 1, next);
        }
        fx.param(COUNT).set_int((n - 1) as c_int);
        page.chosen = (chosen - 1).max(1).min(n - 1);
    } else if name == PREVIOUS {
        page.chosen = chosen.saturating_sub(1);
    } else if name == NEXT {
        page.chosen = (chosen + 1).min(n);
    } else if (name == EARLIER && chosen > 1) || (name == LATER && chosen >= 1 && chosen < n) {
        let other = if name == EARLIER {
            chosen - 1
        } else {
            chosen + 1
        };
        let (a, b) = (point(fx, time, chosen - 1), point(fx, time, other - 1));
        set_point(fx, chosen - 1, b);
        set_point(fx, other - 1, a);
        page.chosen = other;
    }
    shown(fx, time);
    true
}

/// The frame's place in the box: [left, bottom, right, top] (the box's
/// y up), as wide as the frame's shape lets it.
fn frame_box(size: [f64; 2], aspect: f64) -> [f64; 4] {
    let (w, h) = ((size[0] - 8.0).max(8.0), (size[1] - 8.0).max(8.0));
    let aspect = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        16.0 / 9.0
    };
    let (bw, bh) = if w / h > aspect {
        (h * aspect, h)
    } else {
        (w, w / aspect)
    };
    let (x0, y0) = ((size[0] - bw) / 2.0, (size[1] - bh) / 2.0);
    [x0, y0, x0 + bw, y0 + bh]
}

/// The gradient's settings at `time`, as the generator reads them.
fn settings(fx: Fx, time: f64) -> color_gradient::Settings {
    let n = count(fx, time);
    let center = fx.param(CENTER).xy_at(time);
    color_gradient::Settings {
        kind: fx.param(TYPE).int_at(time).clamp(0, 2) as u32,
        angle: fx.param(ANGLE).f64_at(time).to_radians(),
        center: [center[0], 1.0 - center[1]],
        points: (0..n)
            .map(|i| {
                let (d, c) = point(fx, time, i);
                (d, c.map(|v| v as f32))
            })
            .collect(),
    }
}

/// Where each point is in box `b`: the center first, then the color
/// points on the axis.
fn places(s: &color_gradient::Settings, b: [f64; 4]) -> Vec<[f64; 2]> {
    let [x0, y0, x1, y1] = b;
    let at = |nx: f64, ny: f64| [x0 + nx * (x1 - x0), y1 - ny * (y1 - y0)];
    let (cos, sin) = (s.angle.cos(), s.angle.sin());
    let [cx, cy] = s.center;
    let unsigned = s.kind != 0;
    std::iter::once(at(cx, cy))
        .chain(s.points.iter().map(|&(r, _)| {
            let r = if unsigned { r.abs() } else { r };
            at(cos * r + cx, sin * r + cy)
        }))
        .collect()
}

/// What the box remembers: the point being dragged.
#[derive(Debug, Default)]
struct BoxState {
    drag: Option<usize>,
}

/// The Control Points box.
unsafe extern "C" fn points_box(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if is(action, kOfxActionDescribe) {
        return kOfxStatOK;
    }
    let Some(props) = interact_props(handle) else {
        return kOfxStatReplyDefault;
    };
    if is(action, kOfxActionCreateInstance) {
        let state = Box::into_raw(Box::new(BoxState::default()));
        props.set_ptr(kOfxPropInstanceData, state as *mut c_void);
        return kOfxStatOK;
    }
    let state = props.ptr(kOfxPropInstanceData) as *mut BoxState;
    if is(action, kOfxActionDestroyInstance) {
        if !state.is_null() {
            // SAFETY: made above, freed once.
            drop(unsafe { Box::from_raw(state) });
            props.set_ptr(kOfxPropInstanceData, std::ptr::null_mut());
        }
        return kOfxStatOK;
    }
    if state.is_null() {
        return kOfxStatReplyDefault;
    }
    // SAFETY: this box's state; hosts run one of its actions at a time.
    let state = unsafe { &mut *state };
    let args = Props(in_args);
    let fx = Fx(args.ptr(kOfxPropEffectInstance) as OfxImageEffectHandle);
    if fx.0.is_null() {
        return kOfxStatReplyDefault;
    }
    let time = args.dbl(kOfxPropTime, 0);
    let size: [c_int; 2] = props.ints(kOfxInteractPropViewportSize);
    let (_, frame) = fx.frame();
    let b = frame_box([size[0] as f64, size[1] as f64], frame[0] / frame[1]);
    let s = settings(fx, time);
    let chosen = page(fx).map_or(1, |p| p.chosen);
    if is(action, kOfxInteractActionDraw) {
        if let Some(d) = Draw::of(args) {
            draw(&d, b, &s, chosen, state.drag);
        }
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    if is(action, kOfxInteractActionPenDown) {
        let near = |p: &[f64; 2]| (p[0] - pen[0]).hypot(p[1] - pen[1]);
        // The nearest within reach, the chosen one first among equals.
        let hit = places(&s, b)
            .iter()
            .enumerate()
            .filter(|(_, p)| near(p) <= REACH)
            .min_by(|a, c| {
                near(a.1)
                    .total_cmp(&near(c.1))
                    .then((a.0 != chosen).cmp(&(c.0 != chosen)))
            })
            .map(|(i, _)| i);
        let Some(i) = hit else {
            return kOfxStatReplyDefault;
        };
        state.drag = Some(i);
        if let Some(p) = page(fx)
            && p.chosen != i
        {
            p.chosen = i;
            shown(fx, time);
        }
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        let Some(i) = state.drag else {
            return kOfxStatReplyDefault;
        };
        let [x0, y0, x1, y1] = b;
        // The pen as parts of the frame from its top left.
        let nx = (pen[0] - x0) / (x1 - x0);
        let ny = (y1 - pen[1]) / (y1 - y0);
        if i == 0 {
            // The center, up from the bottom as the parameter has it.
            fx.param(CENTER)
                .set_xy([nx.clamp(0.0, 1.0), (1.0 - ny).clamp(0.0, 1.0)]);
        } else {
            // On the axis: the distance along it.
            let (cos, sin) = (s.angle.cos(), s.angle.sin());
            let d = (nx - s.center[0]) * cos + (ny - s.center[1]) * sin;
            let d = if s.kind == 0 {
                d.clamp(-1.0, 1.0)
            } else {
                d.clamp(0.0, 1.0)
            };
            fx.param(POINTS[i - 1].0)
                .set_f64((d * 1000.0).round() / 1000.0);
        }
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenUp) {
        if state.drag.take().is_some() {
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else {
        kOfxStatReplyDefault
    }
}

/// The box: the gradient in cells over the frame, its axis, the center
/// (a ring) and the numbered points (squares), the chosen one yellow.
fn draw(d: &Draw, b: [f64; 4], s: &color_gradient::Settings, chosen: usize, drag: Option<usize>) {
    let rect = |r: [f64; 4]| [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]];
    let [x0, y0, x1, y1] = b;
    let [cw, ch] = CELLS;
    let g = color_gradient::Gradient::new(s, cw, ch, false);
    let (sw, sh) = ((x1 - x0) / cw as f64, (y1 - y0) / ch as f64);
    for row in 0..ch {
        let colors = g.row(row);
        let top = y1 - row as f64 * sh;
        for (col, c) in colors.iter().enumerate().take(cw) {
            // Premultiplied over a mid gray, as see-through shows.
            let over = |v: f32| v + 0.35 * (1.0 - c[3]);
            d.colour([over(c[0]), over(c[1]), over(c[2]), 1.0]);
            let left = x0 + col as f64 * sw;
            d.fill(&rect([left, top - sh, left + sw + 0.5, top + 0.5]));
        }
    }
    d.colour([0.15, 0.15, 0.15, 1.0]);
    d.width(1.0);
    d.outline(&rect(b));
    let at = places(s, b);
    // The axis through the center, as far as the frame's diagonal.
    let (cos, sin) = (s.angle.cos(), s.angle.sin());
    let reach = (x1 - x0).hypot(y1 - y0);
    let c = at[0];
    let way = [cos * (x1 - x0), -sin * (y1 - y0)];
    let len = way[0].hypot(way[1]).max(1e-9);
    let (ux, uy) = (way[0] / len, way[1] / len);
    d.colour([1.0, 1.0, 1.0, 0.55]);
    d.lines(&[
        [c[0] - ux * reach, c[1] - uy * reach],
        [c[0] + ux * reach, c[1] + uy * reach],
    ]);
    for (i, p) in at.iter().enumerate() {
        let hot = i == chosen || drag == Some(i);
        let fill = if hot {
            [1.0, 0.82, 0.1, 1.0]
        } else {
            [0.95, 0.95, 0.95, 1.0]
        };
        if i == 0 {
            let ring: Vec<[f64; 2]> = (0..20)
                .map(|k| {
                    let t = std::f64::consts::TAU * k as f64 / 20.0;
                    [p[0] + 6.0 * t.cos(), p[1] + 6.0 * t.sin()]
                })
                .collect();
            d.colour(fill);
            d.fill(&ring);
            d.colour([0.1, 0.1, 0.1, 1.0]);
            d.outline(&ring);
        } else {
            let r = [p[0] - 7.0, p[1] - 7.0, p[0] + 7.0, p[1] + 7.0];
            d.colour(fill);
            d.fill(&rect(r));
            d.colour([0.1, 0.1, 0.1, 1.0]);
            d.outline(&rect(r));
            if let Ok(n) = CString::new(i.to_string()) {
                // Centered both ways on the square.
                let middle = kOfxDrawTextAlignmentLeft
                    | kOfxDrawTextAlignmentRight
                    | kOfxDrawTextAlignmentTop
                    | kOfxDrawTextAlignmentBaseline;
                d.text(&n, *p, middle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_sit_on_the_axis_in_the_box() {
        let s = color_gradient::Settings {
            kind: 0,
            angle: 0.0,
            center: [0.5, 0.5],
            points: vec![(-0.5, [1.0; 4]), (0.5, [0.0, 0.0, 0.0, 1.0])],
        };
        let b = [0.0, 0.0, 160.0, 90.0];
        let at = places(&s, b);
        assert_eq!(at, vec![[80.0, 45.0], [0.0, 45.0], [160.0, 45.0]]);
        // An ellipse's distances have no side.
        let e = color_gradient::Settings { kind: 1, ..s };
        assert_eq!(places(&e, b)[1], [160.0, 45.0]);
    }

    #[test]
    fn the_frame_fits_the_box() {
        let b = frame_box([320.0, 190.0], 16.0 / 9.0);
        assert!((b[2] - b[0]) / (b[3] - b[1]) - 16.0 / 9.0 < 1e-9);
        assert!(b[0] >= 4.0 - 1e-9 && b[3] <= 186.0 + 1e-9);
    }
}
