//! The 3D transitions as VEGAS draws them — 3D Blinds, 3D Cascade, 3D Fly
//! In/Out and 3D Shuffle: the pictures as cards in space, each a polygon
//! moved by 4 × 4 matrices (VEGAS's, rows first, their rotations turning
//! the way VEGAS's turn), seen by a camera one frame's longer side in front
//! of the frame — a card at depth 1 fills it — and drawn one after another
//! over what is there: each pixel covered by the exact part of it the
//! polygon covers, its color the average of the picture over the pixel's
//! footprint (the best quality's average), lit
//! by VEGAS's two lights — the frame's light along the view, and a point
//! light whose strength follows the transition's Specular light — and laid
//! over by its share. The cards' paths are keyframes through a spline
//! ([`Spline`]); two curves — the point light's strength along the
//! transition and a flying card's opacity — follow plain bumps and ramps
//! until a VEGAS render measures them.

use crate::spline::{Point, Spline};
use std::ffi::{CStr, c_int};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use super::Kind;
use super::pixel::{Frame, framed};
use crate::accel::{TransitionKernel, TransitionPlan, TransitionSteps};
use crate::kit::{Buf, Spec, Vals, for_chunks};
use crate::ofx::{Fx, Props, for_rows};

const SPECULAR: Spec = Spec::Number(
    c"specularLight",
    c"Specular light",
    1.0,
    0.0,
    1.0,
    c"How bright the light shining on the moving pictures gets.",
);
const FOUR_WAYS: &[&CStr] = &[
    c"Left to Right",
    c"Right to Left",
    c"Top to Bottom",
    c"Bottom to Top",
];

pub(super) const BLINDS_3D: &[Spec] = &[
    Spec::Int(
        c"divisions",
        c"Divisions",
        8,
        1,
        64,
        c"How many blinds the picture is cut into.",
    ),
    Spec::Int(
        c"extraSpins",
        c"Extra spins",
        0,
        0,
        10,
        c"How many more half turns each blind makes on its way, two at a time.",
    ),
    Spec::Number(
        c"stagger",
        c"Stagger",
        0.0,
        0.0,
        1.0,
        c"0: the blinds turn together; higher, one after another.",
    ),
    SPECULAR,
    Spec::Choice(
        c"direction",
        c"Direction",
        FOUR_WAYS,
        0,
        c"The blinds' orientation and the order they turn in.",
    ),
];
pub(super) const CASCADE_3D: &[Spec] = &[
    Spec::Int(
        c"divisions",
        c"Divisions",
        5,
        1,
        64,
        c"How many strips the next picture falls in.",
    ),
    Spec::Number(
        c"twist",
        c"Twist",
        0.0,
        0.0,
        1.0,
        c"How much the strips tilt as they fall.",
    ),
    SPECULAR,
    Spec::Choice(
        c"direction",
        c"Direction",
        &[
            c"Left to Right",
            c"Top to Bottom",
            c"Right to Left",
            c"Bottom to Top",
        ],
        1,
        c"The strips' orientation and the order they fall in.",
    ),
];
pub(super) const FLY_3D: &[Spec] = &[
    Spec::Number(
        c"farX",
        c"Far X position",
        0.8,
        0.0,
        1.0,
        c"Where across the frame the far point is: 0 the left, 1 the right.",
    ),
    Spec::Number(
        c"farY",
        c"Far Y position",
        0.4,
        0.0,
        1.0,
        c"Where up the frame the far point is: 0 the top, 1 the bottom.",
    ),
    Spec::Number(
        c"farZ",
        c"Far Z position",
        8.0,
        0.0,
        20.0,
        c"How far away the far point is, in frames.",
    ),
    Spec::Number(
        c"rotationsX",
        c"X rotations",
        8.0,
        0.0,
        20.0,
        c"How many half turns the picture makes about its horizontal axis.",
    ),
    Spec::Number(
        c"rotationsY",
        c"Y rotations",
        1.0,
        0.0,
        20.0,
        c"How many half turns the picture makes about its vertical axis.",
    ),
    Spec::Number(
        c"rotationsZ",
        c"Z rotations",
        1.0,
        0.0,
        20.0,
        c"How many half turns the picture makes in the frame's plane.",
    ),
    SPECULAR,
    Spec::Choice(
        c"direction",
        c"Direction",
        &[c"Fly In", c"Fly Out"],
        0,
        c"The next picture flies in from the far point, or the first one flies out to it.",
    ),
];
pub(super) const SHUFFLE_3D: &[Spec] = &[SPECULAR];

/// The kinds this module draws.
pub(super) fn draws(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Blinds3D | Kind::Cascade3D | Kind::FlyInOut3D | Kind::Shuffle3D
    )
}

fn specs(kind: Kind) -> &'static [Spec] {
    match kind {
        Kind::Blinds3D => BLINDS_3D,
        Kind::Cascade3D => CASCADE_3D,
        Kind::FlyInOut3D => FLY_3D,
        _ => SHUFFLE_3D,
    }
}

/// A transition's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Cards {
    kind: Kind,
    divisions: i32,
    spins: i32,
    stagger: f64,
    twist: f64,
    far: [f64; 3],
    turns: [f64; 3],
    specular: f64,
    direction: i32,
}

impl Cards {
    fn read(kind: Kind, get: &dyn Fn(Spec) -> Vec<f64>) -> Cards {
        let v = Vals::of(specs(kind), get);
        let has = |name: &CStr| specs(kind).iter().any(|s| s.name() == name);
        let f = |name: &CStr| if has(name) { v.f(name) } else { 0.0 };
        Cards {
            kind,
            divisions: if has(c"divisions") {
                v.i(c"divisions").clamp(1, 64)
            } else {
                1
            },
            spins: if has(c"extraSpins") {
                v.i(c"extraSpins").max(0)
            } else {
                0
            },
            stagger: f(c"stagger"),
            twist: f(c"twist"),
            far: [f(c"farX"), f(c"farY"), f(c"farZ")],
            turns: [f(c"rotationsX"), f(c"rotationsY"), f(c"rotationsZ")],
            specular: v.f(c"specularLight"),
            direction: if has(c"direction") {
                v.i(c"direction").clamp(0, 3)
            } else {
                0
            },
        }
    }
}

// VEGAS's matrices.

/// A 4 × 4 matrix, rows first; a point is a column.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Mat([[f32; 4]; 4]);

impl Mat {
    fn identity() -> Mat {
        let mut m = [[0.0; 4]; 4];
        for (i, row) in m.iter_mut().enumerate() {
            row[i] = 1.0;
        }
        Mat(m)
    }

    /// This one after `a`: `a` · self, as VEGAS concatenates.
    fn after(&mut self, a: &Mat) {
        let b = self.0;
        let a = a.0;
        self.0 = std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                b[0][j] * a[i][0] + 0.0 + b[1][j] * a[i][1] + b[2][j] * a[i][2] + b[3][j] * a[i][3]
            })
        });
    }

    fn translate(&mut self, x: f32, y: f32, z: f32) {
        let mut t = Mat::identity();
        t.0[0][3] = x;
        t.0[1][3] = y;
        t.0[2][3] = z;
        self.after(&t);
    }

    fn scale(&mut self, x: f32, y: f32) {
        let mut s = Mat::identity();
        s.0[0][0] = x;
        s.0[1][1] = y;
        self.after(&s);
    }

    /// A turn by `angle` about `axis`, as VEGAS's.
    fn rotate(&mut self, [x, y, z]: [f32; 3], angle: f32) {
        let a = f64::from(angle);
        let k = (1.0 - a.cos()) as f32;
        let s = a.sin() as f32;
        let c = a.cos() as f32;
        let kx = k * x;
        let ky_z = k * y * z;
        let r = Mat([
            [kx * x + c, kx * y + s * z, kx * z - s * y, 0.0],
            [kx * y - s * z, k * y * y + c, ky_z + s * x, 0.0],
            [kx * z + s * y, ky_z - s * x, k * z * z + c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]);
        self.after(&r);
    }

    fn apply(&self, p: [f32; 4]) -> [f32; 4] {
        std::array::from_fn(|i| {
            let r = self.0[i];
            p[0] * r[0] + p[1] * r[1] + p[2] * r[2] + p[3] * r[3]
        })
    }
}

/// A corner of a card: its place (x, y, z, w) and its picture's place (s,
/// t, 0–1 from the top left).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Vertex {
    p: [f32; 4],
    s: f32,
    t: f32,
}

/// VEGAS's card of the frame's part `f0`–`f1` along `dir`'s way: 0 and 1
/// across (columns), 2 and 3 down (rows).
fn quad(dir: i32, f0: f32, f1: f32) -> [Vertex; 4] {
    let v = |x: f32, y: f32| Vertex {
        p: [x, y, 0.0, 1.0],
        s: x,
        t: y,
    };
    if dir < 2 {
        [v(f0, 0.0), v(f1, 0.0), v(f1, 1.0), v(f0, 1.0)]
    } else {
        [v(0.0, f0), v(1.0, f0), v(1.0, f1), v(0.0, f1)]
    }
}

fn sub(a: [f32; 4], b: [f32; 4]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// A polygon's unit normal, as VEGAS finds it: the first two edges from
/// its first corner that are not in line, from the last corner back.
fn normal(v: &[Vertex]) -> [f32; 3] {
    let mut n = [0.0, 0.0, 0.0];
    if v.len() < 3 {
        return n;
    }
    let e1 = sub(v[1].p, v[0].p);
    let mut len = 0.0f32;
    for k in (2..v.len()).rev() {
        n = cross(e1, sub(v[k].p, v[0].p));
        len = dot(n, n).sqrt();
        if len > 1e-6 {
            break;
        }
    }
    if len >= 1e-6 {
        n = n.map(|c| c / len);
    }
    n
}

/// `v` clipped to the side of the plane `n`·p ≥ `d` (Sutherland–Hodgman,
/// every value of a corner along the edges).
fn clip(v: &[Vertex], n: [f32; 3], d: f32) -> Vec<Vertex> {
    let side = |p: &Vertex| dot(n, [p.p[0], p.p[1], p.p[2]]) - d;
    let mut out = Vec::with_capacity(v.len() + 2);
    for i in 0..v.len() {
        let (a, b) = (v[i], v[(i + 1) % v.len()]);
        let (da, db) = (side(&a), side(&b));
        if da >= 0.0 {
            out.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            let k = da / (da - db);
            out.push(Vertex {
                p: std::array::from_fn(|c| a.p[c] + (b.p[c] - a.p[c]) * k),
                s: a.s + (b.s - a.s) * k,
                t: a.t + (b.t - a.t) * k,
            });
        }
    }
    out
}

/// A key of a card's path: its place, size and turns at a time.
struct Key {
    at: [f64; 3],
    turn: [f64; 3],
    t: f64,
}

/// A card's place and turns `t` along `keys`, through the spline.
fn along(keys: &[Key], t: f64) -> ([f64; 3], [f64; 3]) {
    let path = |turns: bool| -> Vec<Point> {
        keys.iter()
            .map(|k| Point {
                t: k.t,
                v: if turns { k.turn } else { k.at },
                w: 1.0,
            })
            .collect()
    };
    (Spline(&path(false)).at(t), Spline(&path(true)).at(t))
}

/// The point light's strength along the transition, its peak 1 at the
/// middle (VEGAS's curve not known yet).
fn shine(p: f64) -> f64 {
    let pts = [
        Point {
            t: 0.0,
            v: [0.0; 3],
            w: 1.0,
        },
        Point {
            t: 0.5,
            v: [1.0, 0.0, 0.0],
            w: 1.0,
        },
        Point {
            t: 1.0,
            v: [0.0; 3],
            w: 1.0,
        },
    ];
    Spline(&pts).at(p)[0]
}

/// A flying card's opacity at time `t` of its flight from the far point
/// (VEGAS's curve not known yet).
fn fade(t: f64) -> f64 {
    let pts = [
        Point {
            t: 0.0,
            v: [0.0; 3],
            w: 1.0,
        },
        Point {
            t: 0.2,
            v: [1.0, 0.0, 0.0],
            w: 1.0,
        },
        Point {
            t: 1.0,
            v: [1.0, 0.0, 0.0],
            w: 1.0,
        },
    ];
    Spline(&pts).at(t)[0].clamp(0.0, 1.0)
}

// The scene.

/// Which picture a card shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pic {
    First,
    Second,
}

/// A card ready to draw: its corners on the screen (x, y in pixels from
/// the top left; w VEGAS's depth term; s, t its picture's place), its
/// picture, its normal and opacity, lit or not.
#[derive(Clone, Debug, PartialEq)]
struct Card {
    v: Vec<Vertex>,
    pic: Pic,
    normal: [f32; 3],
    opacity: f32,
    lit: bool,
}

/// The light shining on the cards (VEGAS's point light), in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Light {
    at: [f32; 3],
    strength: f32,
}

/// What a frame of a 3D transition draws: what is under the cards, the
/// cards in order and the light.
#[derive(Clone, Debug, PartialEq)]
struct Scene {
    /// 0 nothing, 1 the first picture, 2 the second.
    under: u32,
    cards: Vec<Card>,
    light: Option<Light>,
}

/// The frame and its camera.
struct View {
    w: usize,
    h: usize,
    par: f64,
    /// The frame's longer side.
    m: f64,
    camera: Mat,
    /// The view's planes: the frame's edges seen from the camera, and near.
    planes: Vec<([f32; 3], f32)>,
}

impl View {
    fn new(w: usize, h: usize, par: f64) -> View {
        let m = w.max(h) as f64;
        let mut camera = Mat::identity();
        camera.0[0][0] = ((m / par) / m) as f32;
        camera.0[3][2] = (1.0 / m) as f32;
        camera.0[3][3] = 0.0;
        // The frame at depth 1, a hair inside.
        let hx = (w as f64 / m) * par * 0.5;
        let hy = (h as f64 / m) * 0.5;
        let corners = [
            [(1e-5 - hx) as f32, (1e-5 - hy) as f32, 1.0],
            [(hx - 1e-5) as f32, (1e-5 - hy) as f32, 1.0],
            [(hx - 1e-5) as f32, (hy - 1e-5) as f32, 1.0],
            [(1e-5 - hx) as f32, (hy - 1e-5) as f32, 1.0],
        ];
        let mut planes = Vec::new();
        for i in 0..4 {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            let n = cross(a, b);
            let len = dot(n, n).sqrt();
            planes.push((n.map(|c| c / len), 0.0));
        }
        planes.push(([0.0, 0.0, 1.0], 0.01));
        View {
            w,
            h,
            par,
            m,
            camera,
            planes,
        }
    }

    /// The card `v` moved by `model` as VEGAS draws it: back face showing
    /// `back`'s picture mirrored when `two_sided`, clipped to the view and
    /// put on the screen (`None`: out of sight).
    fn card(
        &self,
        v: &[Vertex],
        model: &Mat,
        pics: (Pic, Pic),
        mirror_dir: Option<i32>,
        opacity: f32,
    ) -> Option<Card> {
        let mut v: Vec<Vertex> = v
            .iter()
            .map(|x| Vertex {
                p: model.apply(x.p),
                ..*x
            })
            .collect();
        let n = normal(&v);
        let mut pic = pics.0;
        if let Some(dir) = mirror_dir {
            let p0 = [v[0].p[0], v[0].p[1], v[0].p[2]];
            if dot(n, p0) <= 0.0 {
                pic = pics.1;
                // The back shows its picture the right way round.
                if dir >= 2 {
                    let (t0, t3) = (v[0].t, v[3].t);
                    v[0].t = t3;
                    v[1].t = t3;
                    v[2].t = t0;
                    v[3].t = t0;
                } else {
                    let (s0, s1) = (v[0].s, v[1].s);
                    v[0].s = s1;
                    v[3].s = s1;
                    v[1].s = s0;
                    v[2].s = s0;
                }
            }
        }
        for &(pn, d) in &self.planes {
            v = clip(&v, pn, d);
            if v.len() < 3 {
                return None;
            }
        }
        let n = normal(&v);
        let (hw, hh) = (
            (self.m * (self.w as f64 / self.m) * 0.5) as f32,
            (self.m * (self.h as f64 / self.m) * 0.5) as f32,
        );
        for x in &mut v {
            let p = self.camera.apply(x.p);
            x.p = [p[0] / p[3] + hw, p[1] / p[3] + hh, p[2] / p[3], p[3]];
        }
        Some(Card {
            v,
            pic,
            normal: n,
            opacity,
            lit: true,
        })
    }

    /// The model matrix that makes the unit card the frame's size and
    /// shape, centered on the origin.
    fn sized(&self) -> Mat {
        let mut m = Mat::identity();
        m.translate(-0.5, -0.5, 0.0);
        m.scale(
            ((self.w as f64 / self.m) * self.par) as f32,
            (self.h as f64 / self.m) as f32,
        );
        m
    }

    /// The light at VEGAS's place `x`, `y` (parts of the frame from its
    /// middle) and `z` (frame sides away).
    fn light(&self, x: f32, y: f32, z: f32, strength: f32) -> Option<Light> {
        let (wm, hm) = (self.w as f64 / self.m, self.h as f64 / self.m);
        (strength > 0.0).then(|| Light {
            at: [
                (f64::from(x) * wm * self.m + self.m * wm * 0.5) as f32,
                (f64::from(y) * hm * self.m + self.m * hm * 0.5) as f32,
                z * self.m as f32,
            ],
            strength,
        })
    }
}

/// VEGAS's half turn of a falling strip (its own 3.14, not π).
#[allow(clippy::approx_constant)]
const HALF_TURN: f64 = 3.14;

impl Cards {
    /// The frame's scene at progress `p`.
    fn scene(&self, view: &View, p: f64) -> Scene {
        let p = f64::from(p as f32);
        let strength = (shine(p) * self.specular) as f32;
        match self.kind {
            Kind::Blinds3D => self.blinds(view, p, strength),
            Kind::Cascade3D => self.cascade(view, p, strength),
            Kind::FlyInOut3D => self.fly(view, p, strength),
            _ => self.shuffle(view, p, strength),
        }
    }

    fn blinds(&self, view: &View, p: f64, strength: f32) -> Scene {
        let n = self.divisions.max(1);
        let dir = self.direction;
        let reverse = dir == 1 || dir == 3;
        let dt = 1.0 / f64::from(n);
        let c = 1.0 / (1.0 - (dt - dt * self.stagger) * f64::from(n - 1));
        let dur = c * dt;
        let sign = if reverse { -1.0 } else { 1.0 };
        let mut angles = Vec::new();
        let (mut done, mut waiting) = (0, 0);
        for k in 0..n {
            let idx = if reverse { n - k - 1 } else { k };
            let start = f64::from(idx) * dt * self.stagger * c;
            angles.push(if p < start + dur {
                if start <= p {
                    let u = 1.0 - (p - start) / dur;
                    Some(
                        ((1.0 - u * u)
                            * f64::from(self.spins * 2 + 1)
                            * std::f64::consts::PI
                            * sign) as f32,
                    )
                } else {
                    waiting += 1;
                    None
                }
            } else {
                done += 1;
                Some(f32::NAN)
            });
        }
        let mut cards = Vec::new();
        let flat = || {
            let mut m = view.sized();
            m.translate(0.5, 0.5, 0.0);
            m.translate(-0.5, -0.5, 1.0);
            m
        };
        if done > 0 {
            let f0 = if reverse {
                (1.0 - f64::from(done) * dt) as f32
            } else {
                0.0
            };
            let f1 = (f64::from(done) * dt + f64::from(f0)) as f32;
            cards.extend(view.card(
                &quad(dir, f0, f1),
                &flat(),
                (Pic::Second, Pic::Second),
                None,
                1.0,
            ));
        }
        if waiting > 0 {
            let f0 = if dir == 0 || dir == 2 {
                (1.0 - f64::from(waiting) * dt) as f32
            } else {
                0.0
            };
            let f1 = (f64::from(waiting) * dt + f64::from(f0)) as f32;
            cards.extend(view.card(
                &quad(dir, f0, f1),
                &flat(),
                (Pic::First, Pic::Second),
                Some(dir),
                1.0,
            ));
        }
        for (k, angle) in angles.iter().enumerate() {
            let Some(angle) = *angle else { continue };
            if angle.is_nan() {
                continue;
            }
            let q = quad(dir, k as f32 / n as f32, (k + 1) as f32 / n as f32);
            let mut m = view.sized();
            m.translate(0.5, 0.5, 0.0);
            let placed: Vec<Vertex> = q
                .iter()
                .map(|x| Vertex {
                    p: m.apply(x.p),
                    ..*x
                })
                .collect();
            let cx = (placed[0].p[0] + placed[1].p[0]) * 0.5;
            let cy = (placed[0].p[1] + placed[3].p[1]) * 0.5;
            let mut turn = Mat::identity();
            turn.translate(-cx, -cy, 0.0);
            let axis = if dir < 2 {
                [0.0, 1.0, 0.0]
            } else {
                [1.0, 0.0, 0.0]
            };
            turn.rotate(axis, angle);
            turn.translate(cx - 0.5, cy - 0.5, 1.0);
            cards.extend(view.card(&placed, &turn, (Pic::First, Pic::Second), Some(dir), 1.0));
        }
        Scene {
            under: 0,
            cards,
            light: view.light(0.52, -0.4, 1.35, strength),
        }
    }

    fn cascade(&self, view: &View, p: f64, strength: f32) -> Scene {
        let n = self.divisions.max(1);
        let dir = self.direction;
        // Strips across (rows) falling one after another; the reverse
        // ways the scene turned over.
        let rows = dir == 1 || dir == 3;
        let flipped = dir >= 2;
        let tw = self.twist * std::f64::consts::FRAC_PI_3;
        let keys = [
            Key {
                at: [0.0, -1.5, -0.4],
                turn: [tw, -HALF_TURN, 0.0],
                t: 0.0,
            },
            Key {
                at: [0.0, -0.1, -0.3],
                turn: [tw * 0.375, -1.9, 0.0],
                t: 0.7,
            },
            Key {
                at: [0.0; 3],
                turn: [0.0; 3],
                t: 1.0,
            },
        ];
        let mut cards = Vec::new();
        let mut done = 0;
        let mut moving = Vec::new();
        for k in 0..n {
            let t = f64::from(n - 1 - k) / f64::from(n) + p;
            if t < 1.0 {
                moving.push((k, along(&keys, t)));
            } else {
                done += 1;
            }
        }
        if done > 0 {
            // The strips in place, the second picture as it is.
            let share = done as f32 / n as f32;
            let (f0, f1) = if flipped {
                (1.0 - share, 1.0)
            } else {
                (0.0, share)
            };
            let (w, h) = (view.w as f32, view.h as f32);
            let corner = |x: f32, y: f32| Vertex {
                p: [x * w, y * h, view.m as f32, 1.0 / view.m as f32],
                s: x,
                t: y,
            };
            let v = if rows {
                vec![
                    corner(0.0, f0),
                    corner(1.0, f0),
                    corner(1.0, f1),
                    corner(0.0, f1),
                ]
            } else {
                vec![
                    corner(f0, 0.0),
                    corner(f1, 0.0),
                    corner(f1, 1.0),
                    corner(f0, 1.0),
                ]
            };
            cards.push(Card {
                v,
                pic: Pic::Second,
                normal: [0.0, 0.0, 1.0],
                opacity: 1.0,
                lit: false,
            });
        }
        for (k, (at, turn)) in moving {
            let (f0, f1) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
            let mut q = quad(if rows { 2 } else { 0 }, f0, f1);
            if flipped {
                for x in &mut q {
                    x.s = 1.0 - x.s;
                    x.t = 1.0 - x.t;
                }
            }
            let mut m = view.sized();
            m.translate(0.5, 0.5, 0.0);
            let mid = k as f32 / n as f32 + 0.5 / n as f32;
            if rows {
                m.translate(-0.5, -mid, 0.0);
            } else {
                m.translate(-mid, -0.5, 0.0);
            }
            m.rotate([1.0, 0.0, 0.0], turn[0] as f32);
            m.rotate([0.0, 1.0, 0.0], turn[1] as f32);
            m.rotate([0.0, 0.0, 1.0], turn[2] as f32);
            m.translate(at[0] as f32, at[1] as f32, (at[2] + 1.0) as f32);
            let back = mid - 0.5;
            if rows {
                m.translate(0.0, back, 0.0);
            } else {
                m.translate(back, 0.0, 0.0);
            }
            if flipped {
                m.rotate([0.0, 0.0, 1.0], std::f32::consts::PI);
            }
            cards.extend(view.card(&q, &m, (Pic::Second, Pic::Second), None, 1.0));
        }
        let x = if flipped { -0.52 } else { 0.52 };
        Scene {
            under: 1,
            cards,
            light: view.light(x, 0.32, 0.9, strength),
        }
    }

    fn fly(&self, view: &View, p: f64, strength: f32) -> Scene {
        let fly_in = self.direction == 0;
        let z = self.far[2] + 1.0;
        let pi = std::f64::consts::PI;
        let far = Key {
            at: [(self.far[0] - 0.5) * z, (self.far[1] - 0.5) * z, z],
            turn: self.turns.map(|r| r * pi),
            t: 0.0,
        };
        let near = Key {
            at: [0.0; 3],
            turn: [0.0; 3],
            t: 1.0,
        };
        let keys = if fly_in {
            [far, near]
        } else {
            [
                Key {
                    at: near.at,
                    turn: near.turn,
                    t: 0.0,
                },
                Key {
                    at: far.at,
                    turn: far.turn,
                    t: 1.0,
                },
            ]
        };
        let (at, turn) = along(&keys, p);
        let mut m = view.sized();
        m.rotate([1.0, 0.0, 0.0], turn[0] as f32);
        m.rotate([0.0, 1.0, 0.0], turn[1] as f32);
        m.rotate([0.0, 0.0, 1.0], turn[2] as f32);
        m.translate(at[0] as f32, at[1] as f32, (at[2] + 1.0) as f32);
        let opacity = fade(if fly_in { p } else { 1.0 - p }) as f32;
        let (pic, under) = if fly_in {
            (Pic::Second, 1)
        } else {
            (Pic::First, 2)
        };
        let q = quad(0, 0.0, 1.0);
        Scene {
            under,
            cards: view
                .card(&q, &m, (pic, pic), None, opacity)
                .into_iter()
                .collect(),
            light: view.light(0.32, -0.3, 0.95, strength),
        }
    }

    fn shuffle(&self, view: &View, p: f64, strength: f32) -> Scene {
        let a = 0.523_333_333_333_333_3;
        let b = -1.046_666_666_666_666_6;
        // The second picture comes from behind; the first goes back.
        let second = [
            Key {
                at: [0.0, 0.0, 0.5],
                turn: [0.0; 3],
                t: 0.0,
            },
            Key {
                at: [-0.4, 0.4, 0.3],
                turn: [a, 0.0, b],
                t: 0.4,
            },
            Key {
                at: [0.0; 3],
                turn: [0.0; 3],
                t: 1.0,
            },
        ];
        let first = [
            Key {
                at: [0.0; 3],
                turn: [0.0; 3],
                t: 0.0,
            },
            Key {
                at: [0.4, -0.4, 0.3],
                turn: [a, 0.0, b],
                t: 0.4,
            },
            Key {
                at: [0.0, 0.0, 0.5],
                turn: [0.0; 3],
                t: 1.0,
            },
        ];
        let card = |keys: &[Key], pic: Pic| {
            let (at, turn) = along(keys, p);
            let mut m = view.sized();
            m.rotate([1.0, 0.0, 0.0], turn[0] as f32);
            m.rotate([0.0, 1.0, 0.0], turn[1] as f32);
            m.rotate([0.0, 0.0, 1.0], turn[2] as f32);
            m.translate(at[0] as f32, at[1] as f32, (at[2] + 1.0) as f32);
            view.card(&quad(0, 0.0, 1.0), &m, (pic, pic), None, 1.0)
        };
        let (s, f) = (card(&second, Pic::Second), card(&first, Pic::First));
        // The one in front drawn last: the first picture until halfway.
        let cards = if p < 0.5 { [s, f] } else { [f, s] };
        Scene {
            under: 0,
            cards: cards.into_iter().flatten().collect(),
            light: view.light(0.52, -0.52, 0.9, strength),
        }
    }
}

// Drawing.

/// A card's picture (rows from the top) as the cards read it.
struct Texture<'a> {
    pic: &'a [[f32; 4]],
    w: usize,
    h: usize,
}

impl<'a> Texture<'a> {
    fn new(pic: &'a [[f32; 4]], w: usize, h: usize) -> Texture<'a> {
        Texture { pic, w, h }
    }

    /// The average over the box from `u`, `v` `du` × `dv` (16,384ths of a
    /// pixel), as VEGAS reads a footprint: at least a pixel, and three
    /// times wider less two pixels when more, within the picture. VEGAS
    /// reads it through a summed-area table of floats; its pixels are
    /// summed here, each by the part of it the box covers, which is the
    /// same average without the table's rounding.
    fn average(&self, u: i32, v: i32, du: i32, dv: i32) -> [f32; 4] {
        let grow = |d: i32| {
            let d = d.max(0x4000);
            if d > 0x4000 { d * 3 - 0x8000 } else { d }
        };
        let (gu, gv) = (grow(du), grow(dv));
        let (x1, y1) = (gu.saturating_add(u), v.saturating_add(gv));
        let (w, h) = (self.w as i32, self.h as i32);
        let x0 = if u < 0 { 0 } else { u.min(w * 0x4000 - 0x4000) };
        let y0 = if v < 0 { 0 } else { v.min(h * 0x4000 - 0x4000) };
        let x1 = if x0 < x1 {
            x1.min(w * 0x4000 - 1)
        } else {
            x0 + 1
        };
        let y1 = if y0 < y1 {
            y1.min(h * 0x4000 - 1)
        } else {
            y0 + 1
        };
        let part =
            |lo: i32, hi: i32, k: i32| ((hi.min((k + 1) << 14) - lo.max(k << 14)).max(0)) as f32;
        let mut sum = [0.0f32; 4];
        for j in (y0 >> 14)..=((y1 - 1) >> 14) {
            let wy = part(y0, y1, j) * 6.103_515_6e-5;
            let row = &self.pic[j as usize * self.w..];
            for i in (x0 >> 14)..=((x1 - 1) >> 14) {
                let k = part(x0, x1, i) * 6.103_515_6e-5 * wy;
                let c = row[i as usize];
                for n in 0..4 {
                    sum[n] += c[n] * k;
                }
            }
        }
        let k = 1.0 / ((y1 - y0) as f32 * 6.103_515_6e-5 * (x1 - x0) as f32 * 6.103_515_6e-5);
        sum.map(|s| s * k)
    }
}

/// VEGAS's rounding of a float to a whole number (halves to even).
fn round(x: f64) -> i32 {
    x.round_ties_even() as i32
}

/// A card's values on the screen as planes: 1 ⁄ w, s ⁄ w and t ⁄ w at its
/// first corner and their steps across and down.
#[derive(Clone, Copy, Debug)]
struct Planes {
    x0: f32,
    y0: f32,
    v0: [f32; 3],
    dx: [f32; 3],
    dy: [f32; 3],
}

impl Planes {
    fn of(v: &[Vertex]) -> Planes {
        let q: Vec<[f32; 3]> = v
            .iter()
            .map(|x| {
                let q = 1.0 / x.p[3];
                [q, q * x.s, q * x.t]
            })
            .collect();
        let (a, b, c) = (v[0].p, v[1].p, v[2].p);
        let det = (b[0] - c[0]) * (a[1] - c[1]) - (a[0] - c[0]) * (b[1] - c[1]);
        let inv = if det.abs() >= 1e-7 { 1.0 / det } else { 1.0 };
        let dx = std::array::from_fn(|k| {
            ((a[1] - c[1]) * (q[1][k] - q[2][k]) - (b[1] - c[1]) * (q[0][k] - q[2][k])) * inv
        });
        let dy = std::array::from_fn(|k| {
            ((a[0] - c[0]) * (q[1][k] - q[2][k]) - (b[0] - c[0]) * (q[0][k] - q[2][k])) * -inv
        });
        Planes {
            x0: a[0],
            y0: a[1],
            v0: q[0],
            dx,
            dy,
        }
    }

    fn at(&self, x: f32, y: f32) -> [f32; 3] {
        let (ox, oy) = (x - self.x0, y - self.y0);
        std::array::from_fn(|k| self.v0[k] + ox * self.dx[k] + oy * self.dy[k])
    }
}

/// A card's outline on the screen, convex, for the parts of pixels it
/// covers.
struct Outline {
    p: Vec<[f64; 2]>,
    /// 1 when its corners turn counterclockwise (y down), else -1.
    turn: f64,
    bounds: [f64; 4],
}

impl Outline {
    fn of(v: &[Vertex]) -> Outline {
        let p: Vec<[f64; 2]> = v
            .iter()
            .map(|v| [f64::from(v.p[0]), f64::from(v.p[1])])
            .collect();
        let mut area = 0.0;
        let mut bounds = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for i in 0..p.len() {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            area += a[0] * b[1] - b[0] * a[1];
            bounds = [
                bounds[0].min(a[0]),
                bounds[1].min(a[1]),
                bounds[2].max(a[0]),
                bounds[3].max(a[1]),
            ];
        }
        Outline {
            p,
            turn: if area >= 0.0 { 1.0 } else { -1.0 },
            bounds,
        }
    }

    /// Whether `q` is inside every edge.
    fn inside(&self, q: [f64; 2]) -> bool {
        let n = self.p.len();
        (0..n).all(|i| {
            let (a, b) = (self.p[i], self.p[(i + 1) % n]);
            ((b[0] - a[0]) * (q[1] - a[1]) - (b[1] - a[1]) * (q[0] - a[0])) * self.turn >= 0.0
        })
    }

    /// The part of pixel `x`, `y` (rows from the top) the outline covers.
    fn cover(&self, x: f64, y: f64) -> f64 {
        if x + 1.0 <= self.bounds[0]
            || y + 1.0 <= self.bounds[1]
            || x >= self.bounds[2]
            || y >= self.bounds[3]
        {
            return 0.0;
        }
        if [[x, y], [x + 1.0, y], [x, y + 1.0], [x + 1.0, y + 1.0]]
            .into_iter()
            .all(|q| self.inside(q))
        {
            return 1.0;
        }
        let mut buf = [[0.0f64; 2]; 24];
        let mut tmp = [[0.0f64; 2]; 24];
        let mut n = self.p.len().min(16);
        buf[..n].copy_from_slice(&self.p[..n]);
        for (axis, bound, above) in [
            (0, x, true),
            (0, x + 1.0, false),
            (1, y, true),
            (1, y + 1.0, false),
        ] {
            let side = |q: [f64; 2]| {
                if above {
                    q[axis] - bound
                } else {
                    bound - q[axis]
                }
            };
            let mut m = 0;
            for i in 0..n {
                let (a, b) = (buf[i], buf[(i + 1) % n]);
                let (da, db) = (side(a), side(b));
                if da >= 0.0 && m < tmp.len() {
                    tmp[m] = a;
                    m += 1;
                }
                if (da >= 0.0) != (db >= 0.0) && m < tmp.len() {
                    let k = da / (da - db);
                    tmp[m] = [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k];
                    m += 1;
                }
            }
            if m < 3 {
                return 0.0;
            }
            buf[..m].copy_from_slice(&tmp[..m]);
            n = m;
        }
        let mut area = 0.0;
        for i in 0..n {
            let (a, b) = (buf[i], buf[(i + 1) % n]);
            area += a[0] * b[1] - b[0] * a[1];
        }
        (area * 0.5).abs().min(1.0)
    }
}

/// `c` (premultiplied) lit as VEGAS lights the pixel `x`, `y` of a card
/// with normal `n` at depth `z` in a frame `w` × `h` of pixels `par` wide.
#[allow(clippy::too_many_arguments)]
fn lit(
    c: [f32; 4],
    n: [f32; 3],
    light: Option<&Light>,
    x: f32,
    y: f32,
    z: f32,
    frame: (f32, f32),
    par: f32,
) -> [f32; 4] {
    let a = c[3];
    let s: [f32; 3] = if a <= 1e-6 {
        [0.0; 3]
    } else {
        [c[0] / a, c[1] / a, c[2] / a]
    };
    // The frame's light, along the view.
    let face = n[2].abs();
    let mut sum = [s[0] * face, s[1] * face, s[2] * face];
    if let Some(l) = light {
        let (w, h) = frame;
        let mut view = [(w * 0.5 - x) * par, h * 0.5 - y, 0.0 - z];
        let mut to = [(l.at[0] - x) * par, l.at[1] - y, l.at[2] - z];
        let d2 = (to[1] * to[1]) / (h * h) + (to[0] * to[0]) / (w * w) + (to[2] * to[2]) / (w * w);
        let unit = |v: &mut [f32; 3]| {
            let len = dot(*v, *v).sqrt();
            if len > 0.0 {
                *v = v.map(|c| c / len);
            }
        };
        let (mut near, mut spec, mut facing) = (0.0f32, 0.0f32, 0.0f32);
        unit(&mut view);
        if d2 >= 1e-5 {
            unit(&mut to);
            facing = dot(n, to);
            let mut r: [f32; 3] = std::array::from_fn(|k| (n[k] + n[k]) * facing - to[k]);
            let lean = if facing >= 0.0 {
                dot(view, r)
            } else {
                r = r.map(|c| -c);
                facing = -facing;
                -dot(view, r)
            };
            if lean >= 0.001 {
                spec = f64::from(dot(view, r)).powi(8) as f32;
            }
            near = 1.0;
            if d2 >= 1.0 {
                near = (1.0 / d2).min(1.0);
            }
        }
        let k = near * l.strength;
        for (ch, out) in sum.iter_mut().enumerate() {
            *out += (s[ch] * 0.8 * facing + spec) * k;
        }
    }
    [a * sum[0], a * sum[1], a * sum[2], a]
}

/// Draws `scene` into `out` (rows from the top), `first` and `second` the
/// pictures' tables.
fn draw(
    scene: &Scene,
    view: &View,
    first: &Texture<'_>,
    second: &Texture<'_>,
    out: &mut [[f32; 4]],
) {
    let (w, h) = (view.w, view.h);
    let frame = (w as f32, h as f32);
    let par = view.par as f32;
    for card in &scene.cards {
        let tex = match card.pic {
            Pic::First => first,
            Pic::Second => second,
        };
        let planes = Planes::of(&card.v);
        let outline = Outline::of(&card.v);
        let [x0, y0, x1, y1] = outline.bounds;
        let cols = (
            x0.floor().max(0.0) as usize,
            (x1.ceil().max(0.0) as usize).min(w),
        );
        let rows = (
            y0.floor().max(0.0) as usize,
            (y1.ceil().max(0.0) as usize).min(h),
        );
        if cols.0 >= cols.1 || rows.0 >= rows.1 {
            continue;
        }
        let light = if card.lit { scene.light.as_ref() } else { None };
        let n = if card.lit {
            card.normal
        } else {
            [0.0, 0.0, 1.0]
        };
        let band = &mut out[rows.0 * w..rows.1 * w];
        for_chunks(band, w, |j, row| {
            let y = rows.0 + j;
            for (x, d) in row.iter_mut().enumerate().take(cols.1).skip(cols.0) {
                let cov = outline.cover(x as f64, y as f64);
                if cov <= 0.0 {
                    continue;
                }
                let (xf, yf) = (x as f32, y as f32);
                let at = planes.at(xf, yf);
                let next = [
                    at[0] + planes.dx[0] + planes.dy[0],
                    at[1] + planes.dx[1] + planes.dy[1],
                    at[2] + planes.dx[2] + planes.dy[2],
                ];
                let (tw, th) = (f64::from(tex.w as i32), f64::from(tex.h as i32));
                let wq = 1.0 / at[0];
                let u = round(f64::from(at[1] * wq) * 16384.0 * tw);
                let v = round(f64::from(at[2] * wq) * 16384.0 * th);
                let wn = 1.0 / next[0];
                let du = (round(f64::from(next[1] * wn) * 16384.0 * tw) - u).abs();
                let dv = (round(f64::from(next[2] * wn) * 16384.0 * th) - v).abs();
                let c = tex.average(u, v, du, dv);
                let z = frame.0 * wq * frame.0;
                let c = lit(c, n, light, xf, yf, z, frame, par);
                let k = (cov.min(1.0) as f32) * card.opacity;
                if k >= 1.0 {
                    *d = c;
                } else {
                    for ch in 0..3 {
                        d[ch] = c[ch] * k + d[ch] * (1.0 - k);
                    }
                    d[3] += (c[3].min(1.0) - d[3]) * k;
                }
            }
        });
    }
}

/// The frame of a 3D transition at progress `p`: `a` and `b` the pictures
/// with rows from the bottom; the result likewise.
fn frame(cards: &Cards, f: &Frame, p: f64, a: &[[f32; 4]], b: &[[f32; 4]]) -> Vec<[f32; 4]> {
    let (w, h) = (f.w, f.h);
    let down =
        |pic: &[[f32; 4]]| -> Vec<[f32; 4]> { pic.chunks(w).rev().flatten().copied().collect() };
    let (a, b) = (down(a), down(b));
    let view = View::new(w, h, f.par);
    let scene = cards.scene(&view, p);
    let mut out = match scene.under {
        1 => a.clone(),
        2 => b.clone(),
        _ => vec![[0.0f32; 4]; w * h],
    };
    let (first, second) = (Texture::new(&a, w, h), Texture::new(&b, w, h));
    draw(&scene, &view, &first, &second, &mut out);
    down(&out)
}

/// Renders a 3D transition on the processor.
pub(super) fn render(kind: Kind, fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|v| if v > 0.0 { v } else { 1.0 });
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let from = fx
        .clip(kOfxImageEffectTransitionSourceFromClipName)
        .image(time);
    let to = fx
        .clip(kOfxImageEffectTransitionSourceToClipName)
        .image(time);
    for img in [&from, &to].into_iter().flatten() {
        if img.depth != out.depth || img.channels != out.channels {
            return kOfxStatErrImageFormat;
        }
    }
    let progress = fx.param(kOfxImageEffectTransitionParamName).f64_at(time);
    let cards = Cards::read(kind, &|spec| spec.read(fx, time));
    let (origin, frame_size) = fx.frame();
    let par = if out.par > 0.0 { out.par } else { 1.0 };
    let f = Frame {
        w: (frame_size[0] * scale[0] / par).round().max(1.0) as usize,
        h: (frame_size[1] * scale[1]).round().max(1.0) as usize,
        par,
    };
    let at = (
        (origin[0] * scale[0] / par).round() as c_int,
        (origin[1] * scale[1]).round() as c_int,
    );
    let (a, b) = (Buf::read(from.as_ref()), Buf::read(to.as_ref()));
    let (a, b) = (framed(&a, at, &f), framed(&b, at, &f));
    drop(from);
    drop(to);
    let drawn = frame(&cards, &f, progress, &a, &b);
    let out_straight = out.channels == 4 && out.unpremultiplied;
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(row) = out.row(y) else { return };
        let fy = y - at.1;
        for x in columns.clone() {
            let fxx = x - at.0;
            let mut c = if fxx < 0 || fy < 0 || fxx >= f.w as c_int || fy >= f.h as c_int {
                [0.0; 4]
            } else {
                drawn[fy as usize * f.w + fxx as usize]
            };
            if out_straight && c[3] > 0.0 {
                c = [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]];
            }
            // SAFETY: a row the output has, a column inside its bounds.
            unsafe { crate::colors::put_pixel(&out, row, x, c) };
        }
    };
    for_rows(y1.max(out.bounds[1])..y2.min(out.bounds[3]), &fill);
    drop(out);
    kOfxStatOK
}

/// The most corners a card has once clipped.
const CORNERS: usize = 16;

/// The scene as the card's table: a header (how many cards, what is
/// under them, whether there is a light), the light, the frame, then each
/// card: its picture, corners, lit or not and opacity; its normal; its
/// planes; its bounds; its corners — floats as their bits.
fn table(scene: &Scene, view: &View) -> Vec<[i32; 4]> {
    let b = |v: f32| v.to_bits() as i32;
    let mut t = vec![
        [
            scene.cards.len() as i32,
            scene.under as i32,
            i32::from(scene.light.is_some()),
            0,
        ],
        scene.light.map_or([0; 4], |l| {
            [b(l.at[0]), b(l.at[1]), b(l.at[2]), b(l.strength)]
        }),
        [b(view.w as f32), b(view.h as f32), b(view.par as f32), 0],
    ];
    for card in &scene.cards {
        let planes = Planes::of(&card.v);
        let outline = Outline::of(&card.v);
        let n = card.v.len().min(CORNERS);
        t.push([
            i32::from(card.pic == Pic::Second),
            n as i32,
            i32::from(card.lit),
            b(card.opacity),
        ]);
        t.push([b(card.normal[0]), b(card.normal[1]), b(card.normal[2]), 0]);
        t.push([b(planes.x0), b(planes.y0), b(planes.v0[0]), b(planes.v0[1])]);
        t.push([
            b(planes.v0[2]),
            b(planes.dx[0]),
            b(planes.dx[1]),
            b(planes.dx[2]),
        ]);
        t.push([b(planes.dy[0]), b(planes.dy[1]), b(planes.dy[2]), 0]);
        t.push(outline.bounds.map(|v| b(v as f32)));
        for k in 0..CORNERS / 2 {
            let corner = |i: usize| {
                if i < n {
                    [b(card.v[i].p[0]), b(card.v[i].p[1])]
                } else {
                    [0, 0]
                }
            };
            let (c0, c1) = (corner(2 * k), corner(2 * k + 1));
            t.push([c0[0], c0[1], c1[0], c1[1]]);
        }
    }
    t
}

/// What the card draws for 3D transition `kind` at `progress` in a frame
/// of `size` pixels (`canonical` its size in canonical units: the pixels'
/// shape): the scene as a table the kernel reads beside the pictures.
pub(super) fn plan(
    kind: Kind,
    values: crate::accel::Values<'_>,
    canonical: [f64; 2],
    size: [f64; 2],
    progress: f64,
) -> TransitionPlan {
    let cards = Cards::read(kind, &|spec| {
        values
            .nums(spec.name())
            .map(<[f64]>::to_vec)
            .unwrap_or_else(|| spec.default())
    });
    let par = if size[0] > 0.0 && size[1] > 0.0 && canonical[1] > 0.0 {
        (canonical[0] / size[0]) / (canonical[1] / size[1])
    } else {
        1.0
    };
    let view = View::new(size[0].max(1.0) as usize, size[1].max(1.0) as usize, par);
    let scene = cards.scene(&view, progress);
    TransitionPlan {
        kernel: TransitionKernel::Cards,
        f: [0.0; 24],
        i: [0; 4],
        j: [0; 4],
        steps: TransitionSteps::Cards {
            under: scene.under,
            table: table(&scene, &view),
        },
    }
}

/// VEGAS's presets.
pub(super) fn presets(kind: Kind) -> Vec<crate::Preset> {
    use crate::{Preset, PresetValue};
    let num = |v: f64| PresetValue::Num(vec![v]);
    let w = |name: &'static str, values: Vec<(&'static str, f64)>| Preset {
        name,
        values: values.into_iter().map(|(k, v)| (k, num(v))).collect(),
    };
    let blinds = |name, d: f64, spins: f64, stagger: f64, spec: f64| {
        w(
            name,
            vec![
                ("divisions", d),
                ("extraSpins", spins),
                ("stagger", stagger),
                ("specularLight", spec),
                ("direction", 0.0),
            ],
        )
    };
    let cascade = |name, d: f64, twist: f64, dir: f64| {
        w(
            name,
            vec![
                ("divisions", d),
                ("twist", twist),
                ("specularLight", 1.0),
                ("direction", dir),
            ],
        )
    };
    match kind {
        Kind::Blinds3D => vec![
            blinds("Simple", 8.0, 0.0, 0.0, 1.0),
            blinds("Left to Right", 4.0, 0.0, 0.2, 0.7),
            blinds("Slot Machine", 4.0, 4.0, 0.3, 1.0),
            blinds("Spin", 1.0, 0.0, 0.0, 1.0),
        ],
        Kind::Cascade3D => vec![
            cascade("Top to Bottom", 5.0, 0.0, 1.0),
            cascade("Curtain", 10.0, 0.4, 1.0),
            cascade("Left to Right", 10.0, 0.0, 0.0),
        ],
        Kind::FlyInOut3D => vec![
            w(
                "Tumble In",
                vec![
                    ("farX", 0.8),
                    ("farY", 0.4),
                    ("farZ", 8.0),
                    ("rotationsX", 8.0),
                    ("rotationsY", 1.0),
                    ("rotationsZ", 1.0),
                    ("specularLight", 1.0),
                    ("direction", 0.0),
                ],
            ),
            w(
                "Spin Away",
                vec![
                    ("farX", 0.12),
                    ("farY", 0.12),
                    ("farZ", 2.54),
                    ("rotationsX", 1.6),
                    ("rotationsY", 0.34),
                    ("rotationsZ", 0.0),
                    ("specularLight", 1.0),
                    ("direction", 1.0),
                ],
            ),
        ],
        _ => vec![
            w("Bright Light", vec![("specularLight", 1.0)]),
            w("Low Light", vec![("specularLight", 0.2)]),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
    const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

    fn settings(kind: Kind, preset: &str) -> Cards {
        let preset = presets(kind)
            .into_iter()
            .find(|p| p.name == preset)
            .expect("a preset");
        Cards::read(kind, &|spec| {
            let key = spec.name().to_str().unwrap_or("");
            preset
                .values
                .iter()
                .find(|(n, _)| *n == key)
                .and_then(|(_, v)| match v {
                    crate::PresetValue::Num(v) => Some(v.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| spec.default())
        })
    }

    fn render(kind: Kind, preset: &str, p: f64) -> Vec<[f32; 4]> {
        let f = Frame {
            w: 48,
            h: 27,
            par: 1.0,
        };
        let (a, b) = (vec![RED; f.w * f.h], vec![BLUE; f.w * f.h]);
        frame(&settings(kind, preset), &f, p, &a, &b)
    }

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 2e-3)
    }

    #[test]
    fn a_card_at_depth_one_fills_the_frame_with_its_picture() {
        let view = View::new(48, 27, 1.0);
        let mut m = view.sized();
        m.translate(0.5, 0.5, 0.0);
        m.translate(-0.5, -0.5, 1.0);
        let card = view
            .card(&quad(0, 0.0, 1.0), &m, (Pic::First, Pic::First), None, 1.0)
            .expect("in sight");
        let o = Outline::of(&card.v);
        assert!(
            o.bounds[0].abs() < 1e-3 && (o.bounds[2] - 48.0).abs() < 1e-3,
            "{:?}",
            o.bounds
        );
        assert!(
            o.bounds[1].abs() < 1e-3 && (o.bounds[3] - 27.0).abs() < 1e-3,
            "{:?}",
            o.bounds
        );
        // Each pixel reads its own: a ramp comes back unchanged.
        let ramp: Vec<[f32; 4]> = (0..48 * 27)
            .map(|i| {
                let v = (i % 48) as f32 / 47.0;
                [v, 1.0 - v, 0.5, 1.0]
            })
            .collect();
        let tex = Texture::new(&ramp, 48, 27);
        let scene = Scene {
            under: 0,
            cards: vec![card],
            light: None,
        };
        let mut out = vec![[0.0f32; 4]; 48 * 27];
        draw(&scene, &view, &tex, &tex, &mut out);
        for (k, (o, r)) in out.iter().zip(&ramp).enumerate() {
            assert!(close(*o, *r), "{k}: {o:?} {r:?}");
        }
    }

    #[test]
    fn the_spline_passes_its_points() {
        let pts = [
            Point {
                t: 0.0,
                v: [0.0, 1.0, 2.0],
                w: 1.0,
            },
            Point {
                t: 0.7,
                v: [1.0, 0.0, 0.0],
                w: 1.0,
            },
            Point {
                t: 1.0,
                v: [3.0, 3.0, 3.0],
                w: 1.0,
            },
        ];
        let s = Spline(&pts);
        assert_eq!(s.at(0.0), [0.0, 1.0, 2.0]);
        assert_eq!(s.at(0.7), [1.0, 0.0, 0.0]);
        assert_eq!(s.at(1.0), [3.0, 3.0, 3.0]);
        assert_eq!(s.at(2.0), [3.0, 3.0, 3.0]);
        let mid = s.at(0.35);
        assert!(mid[0] > 0.0 && mid[0] < 1.0, "{mid:?}");
    }

    #[test]
    fn blinds_and_shuffle_go_from_the_first_picture_to_the_second() {
        for (kind, preset) in [
            (Kind::Blinds3D, "Simple"),
            (Kind::Blinds3D, "Slot Machine"),
            (Kind::Shuffle3D, "Low Light"),
        ] {
            let start = render(kind, preset, 0.0);
            assert!(
                start.iter().all(|&c| close(c, RED)),
                "{preset} at 0: {:?}",
                start[0]
            );
            let end = render(kind, preset, 1.0);
            assert!(
                end.iter().all(|&c| close(c, BLUE)),
                "{preset} at 1: {:?}",
                end[0]
            );
            // On the way, something else than both.
            let mid = render(kind, preset, 0.45);
            assert!(
                mid.iter().any(|&c| !close(c, RED) && !close(c, BLUE)),
                "{preset}"
            );
        }
    }

    #[test]
    fn a_flying_picture_ends_in_place() {
        let start = render(Kind::FlyInOut3D, "Tumble In", 0.0);
        assert!(start.iter().all(|&c| close(c, RED)), "{:?}", start[0]);
        let end = render(Kind::FlyInOut3D, "Tumble In", 1.0);
        assert!(end.iter().all(|&c| close(c, BLUE)), "{:?}", end[0]);
        let away = render(Kind::FlyInOut3D, "Spin Away", 0.0);
        assert!(away.iter().all(|&c| close(c, RED)), "{:?}", away[0]);
        let gone = render(Kind::FlyInOut3D, "Spin Away", 1.0);
        let blue = gone.iter().filter(|&&c| close(c, BLUE)).count();
        assert!(blue * 10 > gone.len() * 9, "{blue}");
    }

    #[test]
    fn the_cascade_ends_on_the_second_picture() {
        for preset in ["Top to Bottom", "Curtain", "Left to Right"] {
            let end = render(Kind::Cascade3D, preset, 1.0);
            assert!(
                end.iter().all(|&c| close(c, BLUE)),
                "{preset}: {:?}",
                end[0]
            );
            let mid = render(Kind::Cascade3D, preset, 0.5);
            assert!(mid.iter().any(|&c| close(c, BLUE)), "{preset}");
        }
    }
}
