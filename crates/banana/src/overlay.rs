//! What the pack's effects draw on the preview to drag: places, lines,
//! paths, circles, arrows, corners and the mask, with the draw suite.
#![allow(unsafe_code)]

use std::ffi::{c_char, c_void};
use std::time::Instant;

use bv_effects::host::{Clicks, Draw, Fx as Host, Props, interact_props, is, near_handle, redraw};
use bv_ofx_sys::extensions::kBananaVInteractPropModifiers;
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::bolt::Guide;
use crate::fx::{Fx, Handle, Kind, POINTS, V, common};
use crate::ofx::{c, values};

/// Whether an effect draws anything.
pub(crate) fn has(fx: &Fx) -> bool {
    !fx.handles.is_empty() || fx.kind != Kind::Transition || fx.params.iter().any(is_point)
}

fn is_point(p: &crate::fx::Param) -> bool {
    p.shape == crate::fx::Shape::Point
}

/// What may be dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grip {
    /// A place's setting.
    Point(&'static str),
    /// A path's new place, before the place of that number between its ends.
    Add(usize),
    /// A circle's radius round a place.
    Radius(&'static str, &'static str),
    /// An angle's end, from a place ("" the frame's middle).
    Angle(&'static str, &'static str),
    /// The mask's width and height.
    MaskSide(bool),
    MaskAngle,
}

/// What an overlay remembers: the grip held, from where the pen took it,
/// and the one under the pen.
#[derive(Debug, Default)]
struct State {
    drag: Option<(Grip, [f64; 2])>,
    hover: Option<Grip>,
    /// Where the pen went down.
    down: [f64; 2],
    clicks: Clicks,
}

/// The frame's place, in the overlay's own units.
#[derive(Clone, Copy, Debug)]
struct Space {
    origin: [f64; 2],
    size: [f64; 2],
    /// A screen pixel's size.
    ps: [f64; 2],
}

impl Space {
    fn of(&self, p: [f64; 2]) -> [f64; 2] {
        [
            self.origin[0] + p[0] * self.size[0],
            self.origin[1] + p[1] * self.size[1],
        ]
    }

    fn back(&self, p: [f64; 2]) -> [f64; 2] {
        [
            (p[0] - self.origin[0]) / self.size[0],
            (p[1] - self.origin[1]) / self.size[1],
        ]
    }

    /// A length in pixels of the screen.
    fn px(&self, n: f64) -> f64 {
        n * self.ps[1]
    }
}

/// How far from its place an angle's end is, in the frame's height.
const ARROW: f64 = 0.16;

fn at(v: &V<'_>, name: &str, s: Space) -> [f64; 2] {
    if name.is_empty() {
        return s.of([0.5, 0.5]);
    }
    s.of(v.xy(name))
}

fn polar(from: [f64; 2], degrees: f64, r: f64) -> [f64; 2] {
    let a = degrees.to_radians();
    [from[0] + a.cos() * r, from[1] + a.sin() * r]
}

/// The mask's middle, half sizes and angle, when it is on.
fn mask(v: &V<'_>, s: Space) -> Option<([f64; 2], [f64; 2], f64, i32)> {
    if !v.has(common::MASK) {
        return None;
    }
    let shape = v.i(common::MASK);
    (shape > 0).then(|| {
        (
            at(v, common::MASK_CENTER, s),
            [
                0.5 * v.f64(common::MASK_WIDTH) * s.size[0],
                0.5 * v.f64(common::MASK_HEIGHT) * s.size[1],
            ],
            v.f64(common::MASK_ANGLE),
            shape,
        )
    })
}

/// An effect's path: its ends' settings.
pub(crate) fn path_of(fx: &Fx) -> Option<(&'static str, &'static str)> {
    fx.handles.iter().find_map(|h| match *h {
        Handle::Path(a, b) => Some((a, b)),
        _ => None,
    })
}

/// A path from `a` to `b`, in frame heights (as its effect takes it).
fn guide(v: &V<'_>, a: &str, b: &str, s: Space) -> Guide {
    let wide = s.size[0] / s.size[1].max(1e-9);
    let at = |n: &str| {
        let [x, y] = v.xy(n);
        [(x * wide) as f32, y as f32]
    };
    let mut places = vec![at(a)];
    places.extend(v.between().iter().map(|n| at(n)));
    places.push(at(b));
    Guide::through(&places, v.i(common::CURVE) == 0)
}

/// A place of a path as the overlay counts.
fn on_path([x, y]: [f32; 2], s: Space) -> [f64; 2] {
    [
        s.origin[0] + f64::from(x) * s.size[1],
        s.origin[1] + f64::from(y) * s.size[1],
    ]
}

/// The place of a path nearest `pen`: how far it is in pixels of the
/// screen, where, and the number of the place that would go there.
fn nearest(g: &Guide, pen: [f64; 2], s: Space) -> Option<(f64, [f64; 2], usize)> {
    let points: Vec<[f64; 2]> = g.points().iter().map(|p| on_path(*p, s)).collect();
    let mut best: Option<(f64, [f64; 2], usize)> = None;
    let mut along = 0.0f32;
    for (n, w) in points.windows(2).enumerate() {
        let [a, b] = [w[0], w[1]];
        let d = [b[0] - a[0], b[1] - a[1]];
        let long = (d[0] * d[0] + d[1] * d[1]).max(1e-12);
        let t = (((pen[0] - a[0]) * d[0] + (pen[1] - a[1]) * d[1]) / long).clamp(0.0, 1.0);
        let at = [a[0] + d[0] * t, a[1] + d[1] * t];
        let far = ((pen[0] - at[0]) / s.ps[0]).hypot((pen[1] - at[1]) / s.ps[1]);
        let [p, q] = [g.points()[n], g.points()[n + 1]];
        let piece = (q[0] - p[0]).hypot(q[1] - p[1]);
        if best.is_none_or(|b| far < b.0) {
            let here = along + piece * t as f32;
            let before = g.marks().iter().filter(|m| **m <= here).count();
            let k = before.clamp(1, g.marks().len().max(2) - 1) - 1;
            best = Some((far, at, k));
        }
        along += piece;
    }
    best
}

/// Puts a place in a path, the `k`th between its ends, at `at` (parts of
/// the frame): those after it move on by one, their keys too.
fn put_in(host: Host, v: &V<'_>, k: usize, at: [f64; 2]) -> bool {
    let n = v.between().len();
    if n >= POINTS.len() || k > n {
        return false;
    }
    let param = |j: usize| host.param(&c(POINTS[j]));
    for j in (k..n).rev() {
        param(j + 1).copy_from(param(j));
    }
    param(k).delete_keys();
    param(k).set_xy(at);
    host.param(&c(common::POINTS)).set_int(n as i32 + 1);
    let placed = v.i(common::PLACED).max(n as i32 + 1);
    host.param(&c(common::PLACED)).set_int(placed);
    true
}

/// Takes the `k`th place between a path's ends away.
fn take_out(fx: &Fx, host: Host, v: &V<'_>, k: usize) {
    let n = v.between().len();
    if k >= n {
        return;
    }
    let param = |j: usize| host.param(&c(POINTS[j]));
    for j in k..n - 1 {
        param(j).copy_from(param(j + 1));
    }
    // The last is one no longer in use, as it was at first.
    param(n - 1).delete_keys();
    if let Some(p) = fx.params.iter().find(|p| p.name == POINTS[n - 1]) {
        param(n - 1).set_xy([p.default[0], p.default[1]]);
    }
    host.param(&c(common::POINTS)).set_int(n as i32 - 1);
    host.param(&c(common::PLACED)).set_int(n as i32 - 1);
}

/// Every grip and where it is.
fn grips(fx: &Fx, v: &V<'_>, s: Space) -> Vec<(Grip, [f64; 2])> {
    let mut g: Vec<(Grip, [f64; 2])> = Vec::new();
    let point = |g: &mut Vec<(Grip, [f64; 2])>, name: &'static str| {
        if !g.iter().any(|(x, _)| *x == Grip::Point(name)) {
            g.push((Grip::Point(name), at(v, name, s)));
        }
    };
    for h in &fx.handles {
        match *h {
            Handle::Point(p) => point(&mut g, p),
            Handle::Line(a, b) => {
                point(&mut g, a);
                point(&mut g, b);
            }
            Handle::Quad(q) => q.into_iter().for_each(|p| point(&mut g, p)),
            Handle::Path(a, b) => {
                // Under the places: a place near one takes the pen.
                let path = guide(v, a, b, s);
                if v.between().len() < POINTS.len() {
                    for (k, m) in path.marks().windows(2).enumerate() {
                        let gap = f64::from(m[1] - m[0]) * s.size[1];
                        if gap > s.px(56.0) {
                            g.push((Grip::Add(k), on_path(path.at((m[0] + m[1]) * 0.5), s)));
                        }
                    }
                }
                point(&mut g, a);
                point(&mut g, b);
                for p in v.between() {
                    point(&mut g, p);
                }
            }
            Handle::Ring(p, r) => {
                let c = at(v, p, s);
                let r1 = v.f64(r) / 100.0 * s.size[1];
                g.push((Grip::Radius(p, r), [c[0] + r1, c[1]]));
            }
            Handle::Arrow(p, a) => {
                let c = at(v, p, s);
                g.push((Grip::Angle(p, a), polar(c, v.f64(a), ARROW * s.size[1])));
            }
        }
    }
    // Places no handle names are grips too.
    let path = path_of(fx).is_some();
    for p in fx.params.iter().filter(|p| is_point(p)) {
        // But the mask's, drawn with it, and a path's not in use.
        if p.name != common::MASK_CENTER && !(path && POINTS.contains(&p.name)) {
            point(&mut g, p.name);
        }
    }
    if let Some((c, half, angle, _)) = mask(v, s) {
        g.push((Grip::Point(common::MASK_CENTER), c));
        g.push((Grip::MaskSide(true), polar(c, angle, half[0])));
        g.push((Grip::MaskSide(false), polar(c, angle + 90.0, half[1])));
        g.push((
            Grip::MaskAngle,
            polar(c, angle + 90.0, half[1] + s.px(34.0)),
        ));
    }
    g
}

const LIGHT: [f32; 4] = [1.0, 1.0, 1.0, 0.95];
const DARK: [f32; 4] = [0.0, 0.0, 0.0, 0.7];
const WARM: [f32; 4] = [1.0, 0.84, 0.25, 1.0];

/// A shape drawn dark under light.
fn both(d: &Draw, color: [f32; 4], f: &dyn Fn()) {
    for (c, w) in [(DARK, 3.0), (color, 1.4)] {
        d.colour(c);
        d.width(w);
        f();
    }
}

fn ring(c: [f64; 2], r: [f64; 2], turn: f64) -> Vec<[f64; 2]> {
    let (s, k) = turn.to_radians().sin_cos();
    (0..72)
        .map(|i| {
            let (y, x) = (std::f64::consts::TAU * f64::from(i) / 72.0).sin_cos();
            let (x, y) = (x * r[0], y * r[1]);
            [c[0] + x * k - y * s, c[1] + x * s + y * k]
        })
        .collect()
}

fn turned(c: [f64; 2], turn: f64, points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let (s, k) = turn.to_radians().sin_cos();
    points
        .iter()
        .map(|p| [c[0] + p[0] * k - p[1] * s, c[1] + p[0] * s + p[1] * k])
        .collect()
}

fn draw(fx: &Fx, v: &V<'_>, s: Space, d: &Draw, state: &State) {
    let hot = |g: Grip| state.hover == Some(g) || state.drag.is_some_and(|(x, _)| x == g);
    for h in &fx.handles {
        match *h {
            Handle::Line(a, b) => {
                let (a, b) = (at(v, a, s), at(v, b, s));
                both(d, LIGHT, &|| d.lines(&[a, b]));
            }
            Handle::Path(a, b) => {
                let g = guide(v, a, b, s);
                let line: Vec<[f64; 2]> = g
                    .points()
                    .windows(2)
                    .flat_map(|w| [on_path(w[0], s), on_path(w[1], s)])
                    .collect();
                both(d, LIGHT, &|| d.lines(&line));
            }
            Handle::Ring(p, r) => {
                let r1 = v.f64(r) / 100.0 * s.size[1];
                let color = if hot(Grip::Radius(p, r)) { WARM } else { LIGHT };
                let c = ring(at(v, p, s), [r1, r1], 0.0);
                both(d, color, &|| d.outline(&c));
            }
            Handle::Arrow(p, a) => {
                let c = at(v, p, s);
                let e = polar(c, v.f64(a), ARROW * s.size[1]);
                let color = if hot(Grip::Angle(p, a)) { WARM } else { LIGHT };
                let back = |t: f64| polar(e, v.f64(a) + t, s.px(12.0));
                let lines = [c, e, e, back(152.0), e, back(-152.0)];
                both(d, color, &|| d.lines(&lines));
            }
            Handle::Quad(q) => {
                let c = q.map(|p| at(v, p, s));
                both(d, LIGHT, &|| d.outline(&c));
            }
            Handle::Point(_) => {}
        }
    }
    if let Some((c, half, angle, shape)) = mask(v, s) {
        let outline = match shape {
            1 => ring(c, half, angle),
            3 => turned(
                c,
                angle,
                &[
                    [half[0], 0.0],
                    [0.0, half[1]],
                    [-half[0], 0.0],
                    [0.0, -half[1]],
                ],
            ),
            _ => turned(
                c,
                angle,
                &[
                    [half[0], half[1]],
                    [-half[0], half[1]],
                    [-half[0], -half[1]],
                    [half[0], -half[1]],
                ],
            ),
        };
        both(d, WARM, &|| d.outline(&outline));
        let f = v.f64(common::MASK_FEATHER) / 100.0 * s.size[1] * 0.5;
        if f > s.px(2.0) && shape == 1 {
            let o = ring(c, [half[0] + f, half[1] + f], angle);
            d.colour([1.0, 0.84, 0.25, 0.4]);
            d.width(1.0);
            d.outline(&o);
        }
        let top = polar(c, angle + 90.0, half[1]);
        let end = polar(c, angle + 90.0, half[1] + s.px(34.0));
        both(d, WARM, &|| d.lines(&[top, end]));
    }
    for (g, p) in grips(fx, v, s) {
        match g {
            Grip::Point(_) => d.handle(p, s.ps, hot(g)),
            // A small plus: a place to come.
            Grip::Add(_) => {
                let r = if hot(g) { 7.0 } else { 5.5 };
                let dot = |r: f64| ring(p, [r * s.ps[0], r * s.ps[1]], 0.0);
                d.colour(DARK);
                d.fill(&dot(r + 1.5));
                d.colour(if hot(g) { WARM } else { LIGHT });
                d.fill(&dot(r));
                let arm = r - 2.0;
                d.colour([0.0, 0.0, 0.0, 1.0]);
                d.width(1.6);
                d.lines(&[
                    [p[0] - arm * s.ps[0], p[1]],
                    [p[0] + arm * s.ps[0], p[1]],
                    [p[0], p[1] - arm * s.ps[1]],
                    [p[0], p[1] + arm * s.ps[1]],
                ]);
            }
            _ => {
                let r = if hot(g) { 5.5 } else { 4.5 };
                let dot = ring(p, [r * s.ps[0], r * s.ps[1]], 0.0);
                d.colour(DARK);
                let under = ring(p, [(r + 1.5) * s.ps[0], (r + 1.5) * s.ps[1]], 0.0);
                d.fill(&under);
                d.colour(if hot(g) { WARM } else { LIGHT });
                d.fill(&dot);
            }
        }
    }
}

/// A grip dragged to `to` (the overlay's units); `shift` the Shift key.
fn drag(host: Host, v: &V<'_>, s: Space, g: Grip, to: [f64; 2], shift: bool) {
    let step = |a: f64| {
        if shift { (a / 15.0).round() * 15.0 } else { a }
    };
    // Turned on from where it is: whole turns keyed stay.
    let follow = |a: f64, was: f64| a + ((was - a) / 360.0).round() * 360.0;
    match g {
        Grip::Point(name) => host.param(&c(name)).set_xy(s.back(to)),
        Grip::Add(_) => {}
        Grip::Radius(p, r) => {
            let o = at(v, p, s);
            let len = (to[0] - o[0]).hypot(to[1] - o[1]) / s.size[1] * 100.0;
            host.param(&c(r)).set_f64(len);
        }
        Grip::Angle(p, a) => {
            let o = at(v, p, s);
            let deg = (to[1] - o[1]).atan2(to[0] - o[0]).to_degrees();
            host.param(&c(a)).set_f64(follow(step(deg), v.f64(a)));
        }
        Grip::MaskSide(wide) => {
            let Some((o, _, angle, _)) = mask(v, s) else {
                return;
            };
            let a = angle.to_radians()
                + if wide {
                    0.0
                } else {
                    std::f64::consts::FRAC_PI_2
                };
            let along = (to[0] - o[0]) * a.cos() + (to[1] - o[1]) * a.sin();
            let (name, size) = if wide {
                (common::MASK_WIDTH, s.size[0])
            } else {
                (common::MASK_HEIGHT, s.size[1])
            };
            let n = (along.abs() * 2.0 / size).clamp(0.01, 3.0);
            host.param(&c(name)).set_f64(n);
            if shift {
                // Both sides alike.
                let was = v.f64(name).max(1e-6);
                let other = if wide {
                    common::MASK_HEIGHT
                } else {
                    common::MASK_WIDTH
                };
                let m = (v.f64(other) * n / was).clamp(0.01, 3.0);
                host.param(&c(other)).set_f64(m);
            }
        }
        Grip::MaskAngle => {
            let Some((o, _, angle, _)) = mask(v, s) else {
                return;
            };
            let deg = (to[1] - o[1]).atan2(to[0] - o[0]).to_degrees() - 90.0;
            host.param(&c(common::MASK_ANGLE))
                .set_f64(follow(step(deg), angle));
        }
    }
}

pub(crate) fn main(
    fx: &'static Fx,
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
) -> OfxStatus {
    if is(action, kOfxActionDescribe) {
        return kOfxStatOK;
    }
    let Some(props) = interact_props(handle) else {
        return kOfxStatReplyDefault;
    };
    if is(action, kOfxActionCreateInstance) {
        let state = Box::into_raw(Box::new(State::default()));
        props.set_ptr(kOfxPropInstanceData, state as *mut c_void);
        return kOfxStatOK;
    }
    let state = props.ptr(kOfxPropInstanceData) as *mut State;
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
    // SAFETY: this overlay's state; hosts run one of its actions at a time.
    let state = unsafe { &mut *state };
    let args = Props(in_args);
    let host = Host(args.ptr(kOfxPropEffectInstance) as OfxImageEffectHandle);
    if host.0.is_null() {
        return kOfxStatReplyDefault;
    }
    let time = args.dbl(kOfxPropTime, 0);
    let ps: [f64; 2] = args.dbls(kOfxInteractPropPixelScale);
    let ps = ps.map(|v| if v.abs() > 1e-12 { v.abs() } else { 1.0 });
    let (origin, size) = host.frame();
    let s = Space { origin, size, ps };
    let v = values(fx, host, time);
    if is(action, kOfxInteractActionDraw) {
        if let Some(d) = Draw::of(args) {
            draw(fx, &v, s, &d, state);
        }
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    let keys = args.int(kBananaVInteractPropModifiers, 0);
    if is(action, kOfxInteractActionPenDown) {
        // The last drawn is over the others.
        let near = grips(fx, &v, s)
            .into_iter()
            .rev()
            .find(|(_, p)| near_handle(pen, *p, ps));
        state.down = pen;
        if let Some((a, b)) = path_of(fx) {
            let twice = state
                .clicks
                .press([pen[0] / ps[0], pen[1] / ps[1]], Instant::now());
            match near {
                Some((Grip::Point(name), _)) => {
                    let k = v.between().iter().position(|n| *n == name);
                    // Control or Alt.
                    if let Some(k) = k
                        && (twice || keys & 6 != 0)
                    {
                        take_out(fx, host, &v, k);
                        state.hover = None;
                        redraw(handle);
                        return kOfxStatOK;
                    }
                }
                Some((Grip::Add(_), _)) | None if twice => {
                    if let Some((far, at, k)) = nearest(&guide(&v, a, b, s), pen, s)
                        && far <= 9.0
                        && put_in(host, &v, k, s.back(at))
                    {
                        state.drag =
                            Some((Grip::Point(POINTS[k]), [pen[0] - at[0], pen[1] - at[1]]));
                        state.hover = None;
                        redraw(handle);
                        return kOfxStatOK;
                    }
                }
                _ => {}
            }
        }
        let Some((g, p)) = near else {
            return kOfxStatReplyDefault;
        };
        state.drag = Some((g, [pen[0] - p[0], pen[1] - p[1]]));
        redraw(handle);
        return kOfxStatOK;
    }
    if is(action, kOfxInteractActionPenMotion) {
        if let Some((g, grip)) = state.drag {
            let to = [pen[0] - grip[0], pen[1] - grip[1]];
            if let Grip::Add(k) = g {
                // A path's new place, once the pen drags it out.
                let way = [pen[0] - state.down[0], pen[1] - state.down[1]];
                if (way[0] / ps[0]).hypot(way[1] / ps[1]) >= 3.0 && put_in(host, &v, k, s.back(to))
                {
                    state.drag = Some((Grip::Point(POINTS[k]), grip));
                    state.hover = None;
                    redraw(handle);
                }
                return kOfxStatOK;
            }
            drag(host, &v, s, g, to, keys & 1 != 0);
            return kOfxStatOK;
        }
        let near = grips(fx, &v, s)
            .into_iter()
            .rev()
            .find(|(_, p)| near_handle(pen, *p, ps))
            .map(|(g, _)| g);
        if near != state.hover {
            state.hover = near;
            redraw(handle);
        }
        return kOfxStatReplyDefault;
    }
    if is(action, kOfxInteractActionPenUp) && state.drag.take().is_some() {
        redraw(handle);
        return kOfxStatOK;
    }
    kOfxStatReplyDefault
}
