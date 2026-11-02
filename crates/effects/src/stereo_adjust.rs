//! Stereoscopic 3D Adjust, as VEGAS's: its panel — Horizontal offset, the
//! Corrections group (Vertical offset, Zoom, Keystone, Rotation, Auto
//! correct, Flip horizontal and vertical), Automatic crop and Crop, the
//! Floating windows group — and its math. Each eye gets its own map of
//! the picture's points to the frame's, every correction split in half
//! and given to the eyes in opposite senses: the convergence's move
//! across, the vertical move, the zoom about the middle, a keystone
//! trapezoid, a turn of Rotation × π ⁄ 365 about the middle (VEGAS's
//! divisor), each eye's flips; Crop zooms both in about the points their
//! maps take to the frame's middle, or Automatic crop works out how far
//! from where the lines from those points to the frame's corners cross
//! the eyes' pictures' edges — the crossing rounded half a pixel away,
//! as integer geometry rounds, so even at rest it zooms in a hair —
//! times 1 + 1.4 |Keystone|. The picture is drawn through the map: each
//! pixel read where the inverse map puts it, to a 32nd of a
//! pixel — an affine map's whole numbers worked out per column and per
//! row, a perspective one's per pixel in blocks of columns — between four
//! pixels, nothing past the picture; then the eye's floating window, an
//! opaque black band in 16,384ths of a pixel, its last column partly.
//! BananaV's projects are not stereoscopic yet: the picture is the left
//! eye's, as VEGAS draws a project in 2D, and Auto correct — VEGAS's
//! features followed from the left eye's picture to the right eye's and
//! the vertical offset, zoom, keystone and rotation that line them up,
//! cell by cell — finds both eyes alike. On the card `Plan::Stereo`,
//! `kit_stereo_warp` reading the same 32nds from `card`'s table.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::{Arc, Mutex};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::{Plan, Values};
use crate::kit::Buf;
use crate::ofx::{Fx, Props, is, suites};

pub(crate) const ID: &CStr = c"org.bananav.Stereoscopic3DAdjust";

const HORIZONTAL: &CStr = c"Horizontal";
const CORRECTIONS: &CStr = c"AdjustmentsGroup";
const VERTICAL: &CStr = c"Vertical";
const ZOOM: &CStr = c"Zoom";
const KEYSTONE: &CStr = c"Keystone";
const ROTATION: &CStr = c"Rotation";
const AUTO_ALIGN: &CStr = c"AutoAlignButton";
const FLIP_H: &CStr = c"FlipHorizontal";
const FLIP_V: &CStr = c"FlipVertical";
const AUTO_CROP: &CStr = c"AutoCrop";
const CROP: &CStr = c"Crop";
const WINDOWS: &CStr = c"FloatingWindowsGroup";
const WINDOW_LEFT: &CStr = c"FloatingWindowLeft";
const WINDOW_RIGHT: &CStr = c"FloatingWindowRight";

const FLIPS: [&CStr; 4] = [c"Off", c"Left", c"Right", c"Both"];

/// The eye BananaV draws (0 the left): its projects are not stereoscopic
/// yet, and VEGAS draws a project in 2D as the left eye.
const EYE: usize = 0;

/// A 3 × 3 map by rows, rows up.
type M3 = [[f64; 3]; 3];

/// VEGAS's π, to nine decimals.
#[allow(clippy::approx_constant)]
const VEGAS_PI: f64 = 3.141592654;

const I3: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn mul(a: &M3, b: &M3) -> M3 {
    std::array::from_fn(|i| {
        std::array::from_fn(|j| a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j])
    })
}

fn shift(x: f64, y: f64) -> M3 {
    [[1.0, 0.0, x], [0.0, 1.0, y], [0.0, 0.0, 1.0]]
}

fn scale(s: f64) -> M3 {
    [[s, 0.0, 0.0], [0.0, s, 0.0], [0.0, 0.0, 1.0]]
}

fn turn(a: f64) -> M3 {
    let (s, c) = (a.sin(), a.cos());
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

/// Where `m` takes the point `p`.
fn apply(m: &M3, [x, y]: [f64; 2]) -> [f64; 2] {
    let w = m[2][0] * x + m[2][1] * y + m[2][2];
    [
        (m[0][0] * x + m[0][1] * y + m[0][2]) / w,
        (m[1][0] * x + m[1][1] * y + m[1][2]) / w,
    ]
}

/// `m` undone: its adjugate over its determinant (nothing when it has
/// none).
fn inverse(m: &M3) -> M3 {
    let d = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if d == 0.0 {
        return [[0.0; 3]; 3];
    }
    let d = 1.0 / d;
    [
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * d,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * d,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * d,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * d,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * d,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * d,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * d,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * d,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * d,
        ],
    ]
}

/// The projective map taking the four points `from` to `to`, solved in
/// double precision by elimination.
fn perspective(from: [[f32; 2]; 4], to: [[f32; 2]; 4]) -> M3 {
    let mut a = [[0.0f64; 9]; 8];
    for i in 0..4 {
        let ([x, y], [u, v]) = (from[i].map(f64::from), to[i].map(f64::from));
        a[i] = [x, y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u, u];
        a[i + 4] = [0.0, 0.0, 0.0, x, y, 1.0, -x * v, -y * v, v];
    }
    for c in 0..8 {
        let p = (c..8)
            .max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))
            .unwrap_or(c);
        a.swap(c, p);
        if a[c][c] == 0.0 {
            return I3;
        }
        for r in c + 1..8 {
            let f = a[r][c] / a[c][c];
            let pivot = a[c];
            for (v, p) in a[r].iter_mut().zip(pivot).skip(c) {
                *v -= f * p;
            }
        }
    }
    let mut x = [0.0f64; 8];
    for r in (0..8).rev() {
        let s: f64 = (r + 1..8).map(|k| a[r][k] * x[k]).sum();
        x[r] = (a[r][8] - s) / a[r][r];
    }
    [[x[0], x[1], x[2]], [x[3], x[4], x[5]], [x[6], x[7], 1.0]]
}

/// Where segment `p1`–`p2` crosses segment `p3`–`p4`, as VEGAS finds it
/// (integer geometry's crossing, in doubles: half the divisor added to
/// each numerator away from zero, so the point lands half a pixel off).
fn crossing(p1: [f64; 2], p2: [f64; 2], p3: [f64; 2], p4: [f64; 2]) -> Option<[f64; 2]> {
    let apart = |r: f64, s: f64| r != 0.0 && s != 0.0 && (r > 0.0) == (s > 0.0);
    let (a1, b1) = (p2[1] - p1[1], p1[0] - p2[0]);
    let c1 = p2[0] * p1[1] - p1[0] * p2[1];
    let (r3, r4) = (p3[0] * a1 + p3[1] * b1 + c1, p4[0] * a1 + p4[1] * b1 + c1);
    if apart(r3, r4) {
        return None;
    }
    let (a2, b2) = (p4[1] - p3[1], p3[0] - p4[0]);
    let c2 = p4[0] * p3[1] - p4[1] * p3[0];
    let (r1, r2) = (p1[0] * a2 + p1[1] * b2 + c2, p2[0] * a2 + p2[1] * b2 + c2);
    let denom = b2 * a1 - a2 * b1;
    if apart(r1, r2) || denom == 0.0 {
        return None;
    }
    let offset = denom.abs() * 0.5;
    let away = |n: f64| if n >= 0.0 { n + offset } else { n - offset };
    Some([
        away(c2 * b1 - b2 * c1) / denom,
        away(a2 * c1 - c2 * a1) / denom,
    ])
}

/// What the eyes' maps are made of, as VEGAS hands them over.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Corrections {
    horizontal: f64,
    vertical: f64,
    zoom: f64,
    keystone: f64,
    rotation: f64,
    flips: [c_int; 2],
    crop: f64,
    auto_crop: bool,
}

/// The eyes' maps (the left's, the right's) of a `w` × `h` frame's
/// picture to the frame, rows up, as VEGAS works them out.
fn eyes(c: &Corrections, [w, h]: [f64; 2]) -> [M3; 2] {
    let (mut a, mut b) = (I3, I3);
    if c.horizontal != 0.0 {
        a = mul(&a, &shift(-c.horizontal * 0.5 * w, 0.0));
        b = mul(&b, &shift(c.horizontal * 0.5 * w, 0.0));
    }
    if c.vertical != 0.0 {
        a = mul(&a, &shift(0.0, c.vertical * 0.5 * h));
        b = mul(&b, &shift(0.0, -(c.vertical * 0.5) * h));
    }
    let (mid, back) = (shift(w * 0.5, h * 0.5), shift(-w * 0.5, -h * 0.5));
    let about = |m: &M3, x: &M3| mul(&mul(&mul(m, &mid), x), &back);
    if c.zoom != 0.0 {
        a = about(&a, &scale(-c.zoom * 0.5 + 1.0));
        b = about(&b, &scale(c.zoom * 0.5 + 1.0));
    }
    if c.keystone != 0.0 {
        let k = -c.keystone * 0.5;
        let (fw, fh) = (w as f32, h as f32);
        let corners = [[0.0, 0.0], [fw, 0.0], [0.0, fh], [fw, fh]];
        // The left edge narrowed and the right widened (the other way
        // for the right eye).
        let lean = |k: f64| {
            let t = k * h;
            [
                [0.0, (-h * k) as f32],
                [fw, t as f32],
                [0.0, (t + h) as f32],
                [fw, (h - t) as f32],
            ]
        };
        a = mul(&a, &perspective(corners, lean(k)));
        b = mul(&b, &perspective(corners, lean(-k)));
    }
    if c.rotation != 0.0 {
        let t = (-c.rotation / 365.0) * VEGAS_PI;
        a = about(&a, &turn(t));
        b = about(&b, &turn(-t));
    }
    let flip_x = [[-1.0, 0.0, w], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let flip_y = [[1.0, 0.0, 0.0], [0.0, -1.0, h], [0.0, 0.0, 1.0]];
    let [fh, fv] = c.flips;
    if matches!(fh, 1 | 3) {
        a = mul(&a, &flip_x);
    }
    if matches!(fv, 1 | 3) {
        a = mul(&a, &flip_y);
    }
    if matches!(fh, 2 | 3) {
        b = mul(&b, &flip_x);
    }
    if matches!(fv, 2 | 3) {
        b = mul(&b, &flip_y);
    }
    if c.crop == 0.0 && !c.auto_crop {
        return [a, b];
    }
    let middle = [w * 0.5, h * 0.5];
    let (ca, cb) = (apply(&inverse(&a), middle), apply(&inverse(&b), middle));
    let mut crop = c.crop;
    if c.auto_crop {
        let frame = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]];
        // How far past each eye's picture the frame's corners are, from
        // the point its map takes to the middle.
        let reach = |m: &M3, center: [f64; 2]| {
            let quad = frame.map(|p| apply(m, p));
            let dist = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).hypot(p[1] - q[1]);
            frame
                .iter()
                .map(|&corner| {
                    (0..4)
                        .find_map(|e| crossing(center, corner, quad[e], quad[(e + 1) % 4]))
                        .map_or(0.0, |x| {
                            let out = dist(corner, x);
                            out / (dist(corner, center) - out)
                        })
                })
                .fold(0.0, f64::max)
        };
        crop = reach(&a, ca).max(reach(&b, cb)) * ((c.keystone * 1.4).abs() + 1.0);
    }
    let zoom = |m: &M3, [x, y]: [f64; 2]| {
        let m = mul(&mul(m, &shift(x, y)), &scale(crop + 1.0));
        mul(&m, &shift(-x, -y))
    };
    // VEGAS's left eye zooms about its own point across and the right
    // eye's up.
    [zoom(&a, [ca[0], cb[1]]), zoom(&b, cb)]
}

/// The settings a frame is drawn with, in single precision as VEGAS
/// reads them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    horizontal: f32,
    vertical: f32,
    zoom: f32,
    keystone: f32,
    rotation: f32,
    flips: [c_int; 2],
    auto_crop: bool,
    crop: f32,
    windows: [f32; 2],
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        let f = |n: &CStr| fx.param(n).f64_at(time) as f32;
        Settings {
            horizontal: f(HORIZONTAL),
            vertical: f(VERTICAL),
            zoom: f(ZOOM),
            keystone: f(KEYSTONE),
            rotation: f(ROTATION),
            flips: [fx.param(FLIP_H).int_at(time), fx.param(FLIP_V).int_at(time)],
            auto_crop: fx.param(AUTO_CROP).int_at(time) != 0,
            crop: f(CROP),
            windows: [f(WINDOW_LEFT), f(WINDOW_RIGHT)],
        }
    }

    pub(crate) fn of_values(v: Values<'_>) -> Settings {
        let f = |n: &CStr| v.f64(n, 0.0) as f32;
        Settings {
            horizontal: f(HORIZONTAL),
            vertical: f(VERTICAL),
            zoom: f(ZOOM),
            keystone: f(KEYSTONE),
            rotation: f(ROTATION),
            flips: [v.int(FLIP_H, 0), v.int(FLIP_V, 0)],
            auto_crop: v.int(AUTO_CROP, 1) != 0,
            crop: f(CROP),
            windows: [f(WINDOW_LEFT), f(WINDOW_RIGHT)],
        }
    }

    /// The card's numbers and flips.
    fn numbers(&self) -> ([f32; 9], [u32; 2]) {
        (
            [
                self.horizontal,
                self.vertical,
                self.zoom,
                self.keystone,
                self.rotation,
                self.crop,
                f32::from(u8::from(self.auto_crop)),
                self.windows[0],
                self.windows[1],
            ],
            self.flips.map(|f| f.max(0) as u32),
        )
    }

    fn of_numbers(n: &[f32; 9], flips: [u32; 2]) -> Settings {
        Settings {
            horizontal: n[0],
            vertical: n[1],
            zoom: n[2],
            keystone: n[3],
            rotation: n[4],
            crop: n[5],
            auto_crop: n[6] != 0.0,
            windows: [n[7], n[8]],
            flips: flips.map(|f| f as c_int),
        }
    }

    fn corrections(&self) -> Corrections {
        Corrections {
            horizontal: f64::from(self.horizontal),
            vertical: f64::from(self.vertical),
            zoom: f64::from(self.zoom),
            keystone: f64::from(self.keystone),
            rotation: f64::from(self.rotation),
            flips: self.flips,
            crop: f64::from(self.crop),
            auto_crop: self.auto_crop,
        }
    }

    /// Whether the picture comes out as it went in: no correction, no
    /// crop and no window for the eye drawn (Automatic crop zooms in a
    /// hair even then).
    fn changes_nothing(&self) -> bool {
        let c = self.corrections();
        c.horizontal == 0.0
            && c.vertical == 0.0
            && c.zoom == 0.0
            && c.keystone == 0.0
            && c.rotation == 0.0
            && c.flips == [0, 0]
            && !c.auto_crop
            && c.crop == 0.0
            && self.windows[EYE] == 0.0
    }

    /// The eye drawn's map for a `w` × `h` picture.
    fn map(&self, w: usize, h: usize) -> M3 {
        eyes(&self.corrections(), [w as f64, h as f64])[EYE]
    }

    /// Whether the picture is drawn through an affine warp (no Keystone
    /// nor Rotation) rather than a perspective one.
    fn affine(&self) -> bool {
        self.keystone == 0.0 && self.rotation == 0.0
    }

    pub(crate) fn plan(&self) -> Plan {
        if self.changes_nothing() {
            return Plan::Same;
        }
        let (numbers, flips) = self.numbers();
        Plan::Stereo { numbers, flips }
    }
}

/// The floating window of the eye drawn on a frame `w` pixels wide, in
/// whole columns from and to, and the column partly covered with its
/// share, as VEGAS rounds it in 16,384ths of a pixel.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Band {
    from: usize,
    to: usize,
    part: Option<(usize, f32)>,
}

fn band(s: &Settings, w: usize) -> Band {
    let full = w as i64 * 16384;
    let reach = ((full as i32 as f32 * s.windows[EYE]) as f64 + 1.5e-8).round_ties_even() as i64;
    let (x0, x1) = if EYE == 0 {
        (0, reach)
    } else {
        (full - reach, full)
    };
    let from = (x0 + 16383).div_euclid(16384);
    let to = x1.div_euclid(16384);
    let mut share = (x1 - to * 16384) as f32 * 6.103_515_6e-5;
    let mut column = to;
    if share == 0.0 {
        share = (from * 16384 - x0) as f32 * 6.103_515_6e-5;
        column = from - 1;
    }
    Band {
        from: from.max(0) as usize,
        to: to.max(from) as usize,
        part: (share != 0.0 && column >= 0).then_some((column as usize, share)),
    }
}

/// Where a picture's output pixels read it, in 32nds of a pixel, rows up,
/// for a map of the picture to the frame:
/// an affine map's whole numbers per column and per row, or a perspective
/// map's per pixel.
#[derive(Clone, Debug, PartialEq)]
enum Reads {
    Affine {
        columns: Vec<[i32; 2]>,
        rows: Vec<[i32; 2]>,
    },
    Perspective(Arc<Vec<[i32; 2]>>),
}

impl Reads {
    fn at(&self, x: usize, y: usize, w: usize) -> [i32; 2] {
        match self {
            Reads::Affine { columns, rows } => {
                let (c, r) = (columns[x], rows[y]);
                [r[0].wrapping_add(c[0]) >> 5, r[1].wrapping_add(c[1]) >> 5]
            }
            Reads::Perspective(xy) => xy[y * w + x],
        }
    }
}

/// The last perspective reads worked out: VEGAS's settings rarely change
/// from frame to frame.
type Kept = Option<([u64; 9], [usize; 2], Arc<Vec<[i32; 2]>>)>;
static KEPT: Mutex<Kept> = Mutex::new(None);

/// A double rounded to an integer (half to even, held).
fn round(v: f64) -> i32 {
    v.round_ties_even() as i32
}

fn reads(m: &M3, affine: bool, [w, h]: [usize; 2]) -> Reads {
    if affine {
        // invertAffineTransform.
        let d = m[0][0] * m[1][1] - m[0][1] * m[1][0];
        let d = if d != 0.0 { 1.0 / d } else { 0.0 };
        let (a11, a22) = (m[1][1] * d, m[0][0] * d);
        let (a12, a21) = (m[0][1] * -d, m[1][0] * -d);
        let b1 = -a11 * m[0][2] - a12 * m[1][2];
        let b2 = -a21 * m[0][2] - a22 * m[1][2];
        const AB: f64 = 1024.0;
        return Reads::Affine {
            columns: (0..w)
                .map(|x| [round(a11 * x as f64 * AB), round(a21 * x as f64 * AB)])
                .collect(),
            rows: (0..h)
                .map(|y| {
                    let y = y as f64;
                    [
                        round((a12 * y + b1) * AB) + 16,
                        round((a22 * y + b2) * AB) + 16,
                    ]
                })
                .collect(),
        };
    }
    let m = inverse(m);
    let key = [m[0], m[1], m[2]].concat();
    let key: [u64; 9] = std::array::from_fn(|i| key[i].to_bits());
    if let Ok(kept) = KEPT.lock()
        && let Some((k, size, xy)) = kept.as_ref()
        && *k == key
        && *size == [w, h]
    {
        return Reads::Perspective(xy.clone());
    }
    // Blocks of columns, each starting its sums afresh.
    let block = (1024 / h.clamp(1, 16)).min(w).max(1);
    let mut xy = vec![[0i32; 2]; w * h];
    crate::kit::for_chunks(&mut xy, w, |y, row| {
        let y = y as f64;
        for (x, out) in row.iter_mut().enumerate() {
            let (xb, x1) = ((x / block * block) as f64, (x % block) as f64);
            let x0 = m[0][0] * xb + m[0][1] * y + m[0][2];
            let y0 = m[1][0] * xb + m[1][1] * y + m[1][2];
            let w0 = m[2][0] * xb + m[2][1] * y + m[2][2];
            let ww = w0 + m[2][0] * x1;
            let ww = if ww != 0.0 { 32.0 / ww } else { 0.0 };
            let held = |v: f64| v.clamp(f64::from(i32::MIN), f64::from(i32::MAX));
            *out = [
                round(held((x0 + m[0][0] * x1) * ww)),
                round(held((y0 + m[1][0] * x1) * ww)),
            ];
        }
    });
    let xy = Arc::new(xy);
    if let Ok(mut kept) = KEPT.lock() {
        *kept = Some((key, [w, h], xy.clone()));
    }
    Reads::Perspective(xy)
}

/// The picture (`w` × `h`, rows up) read at `xy` 32nds of a pixel, between
/// four pixels, bilinearly, nothing past it.
fn sample(src: &[[f32; 4]], [w, h]: [usize; 2], [x, y]: [i32; 2]) -> [f32; 4] {
    let (sx, sy) = (x >> 5, y >> 5);
    let (tx, ty) = ((x & 31) as f32 / 32.0, (y & 31) as f32 / 32.0);
    let at = |i: i32, j: i32| {
        if i >= 0 && j >= 0 && (i as usize) < w && (j as usize) < h {
            src[j as usize * w + i as usize]
        } else {
            [0.0; 4]
        }
    };
    let (p00, p10, p01, p11) = (
        at(sx, sy),
        at(sx + 1, sy),
        at(sx, sy + 1),
        at(sx + 1, sy + 1),
    );
    let k = [
        (1.0 - ty) * (1.0 - tx),
        (1.0 - ty) * tx,
        ty * (1.0 - tx),
        ty * tx,
    ];
    std::array::from_fn(|c| p00[c] * k[0] + p10[c] * k[1] + p01[c] * k[2] + p11[c] * k[3])
}

/// The picture drawn as the eye drawn sees it.
fn adjusted(source: &Buf, s: &Settings) -> Buf {
    let (w, h) = (
        source.width().max(0) as usize,
        source.height().max(0) as usize,
    );
    if w == 0 || h == 0 || s.changes_nothing() {
        return source.clone();
    }
    let r = reads(&s.map(w, h), s.affine(), [w, h]);
    let band = band(s, w);
    let [bx, by] = [source.bounds[0], source.bounds[1]];
    source.map(|x, y| {
        let (i, j) = ((x - bx) as usize, (y - by) as usize);
        let c = sample(&source.data, [w, h], r.at(i, j, w));
        if (band.from..band.to).contains(&i) {
            return [0.0, 0.0, 0.0, 1.0];
        }
        match band.part {
            Some((column, share)) if column == i => {
                let black = [0.0, 0.0, 0.0, 1.0];
                std::array::from_fn(|k| c[k] + (black[k] - c[k]) * share)
            }
            _ => c,
        }
    })
}

/// What the card draws a `w` × `h` picture with: whether the map is
/// affine; its table — an affine map's columns' whole numbers then its
/// rows' from the top, or each pixel's 32nds from the top, two pixels an
/// entry —; the floating window's whole columns and its partly covered
/// column (−1 for none) and share.
pub(crate) struct Card {
    pub affine: bool,
    pub table: Vec<[f32; 4]>,
    pub band: [i32; 3],
    pub share: f32,
}

pub(crate) fn card(numbers: &[f32; 9], flips: [u32; 2], w: u32, h: u32) -> Card {
    let s = Settings::of_numbers(numbers, flips);
    let (w, h) = (w as usize, h as usize);
    // Exact in single precision, far outside the picture when held.
    let f = |v: i32| v.clamp(-(1 << 23), 1 << 23) as f32;
    let table = match reads(&s.map(w, h), s.affine(), [w, h]) {
        Reads::Affine { columns, rows } => columns
            .iter()
            .map(|c| [f(c[0]), f(c[1]), 0.0, 0.0])
            .chain(rows.iter().rev().map(|r| [f(r[0]), f(r[1]), 0.0, 0.0]))
            .collect(),
        Reads::Perspective(xy) => {
            let top_down = (0..h).rev().flat_map(|y| xy[y * w..(y + 1) * w].iter());
            let flat: Vec<[f32; 2]> = top_down.map(|p| [f(p[0]), f(p[1])]).collect();
            flat.chunks(2)
                .map(|p| {
                    let b = p.get(1).copied().unwrap_or([0.0; 2]);
                    [p[0][0], p[0][1], b[0], b[1]]
                })
                .collect()
        }
    };
    let b = band(&s, w);
    let (column, share) = b.part.map_or((-1, 0.0), |(c, s)| (c as i32, s));
    Card {
        affine: s.affine(),
        table,
        band: [b.from as i32, b.to as i32, column],
        share,
    }
}

// Auto correct.

/// A picture as VEGAS's Auto correct looks at it: gray (BT.601) in 8-bit
/// steps, brought to 1080 lines, its histogram equalized, flipped as the
/// eye is; rows up, 0–1.
fn prepared(source: &Buf, [flip_x, flip_y]: [bool; 2]) -> bv_track::Gray {
    let (w, h) = (
        source.width().max(1) as usize,
        source.height().max(1) as usize,
    );
    let gray: Vec<f32> = source
        .data
        .iter()
        .map(|p| {
            let v = 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
            (v.clamp(0.0, 1.0) * 255.0).round()
        })
        .collect();
    let ratio = 1080.0 / h as f64;
    let (sw, sh) = (((w as f64 * ratio) as usize).max(1), 1080usize);
    let at = |x: isize, y: isize| {
        gray[y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize]
    };
    let mut steps = vec![0u8; sw * sh];
    for (j, row) in steps.chunks_mut(sw).enumerate() {
        let y = ((j as f64 + 0.5) / ratio - 0.5).max(0.0);
        let (y0, fy) = (y.floor(), (y - y.floor()) as f32);
        for (i, v) in row.iter_mut().enumerate() {
            let x = ((i as f64 + 0.5) * (w as f64 / sw as f64) - 0.5).max(0.0);
            let (x0, fx) = (x.floor(), (x - x.floor()) as f32);
            let (xi, yi) = (x0 as isize, y0 as isize);
            let low = at(xi, yi) + (at(xi + 1, yi) - at(xi, yi)) * fx;
            let high = at(xi, yi + 1) + (at(xi + 1, yi + 1) - at(xi, yi + 1)) * fx;
            *v = (low + (high - low) * fy).round().clamp(0.0, 255.0) as u8;
        }
    }
    // equalizeHist.
    let mut hist = [0usize; 256];
    for &v in &steps {
        hist[v as usize] += 1;
    }
    let first = hist.iter().position(|&n| n > 0).unwrap_or(0);
    let total = steps.len();
    let mut lut = [first as u8; 256];
    if hist[first] < total {
        let scale = 255.0 / (total - hist[first]) as f32;
        let mut sum = 0;
        lut[first] = 0;
        for i in first + 1..256 {
            sum += hist[i];
            lut[i] = (sum as f32 * scale).round().clamp(0.0, 255.0) as u8;
        }
    }
    let data = (0..sh)
        .flat_map(|j| (0..sw).map(move |i| (i, j)))
        .map(|(i, j)| {
            let i = if flip_x { sw - 1 - i } else { i };
            let j = if flip_y { sh - 1 - j } else { j };
            f32::from(lut[steps[j * sw + i] as usize]) / 255.0
        })
        .collect();
    bv_track::Gray {
        width: sw,
        height: sh,
        data,
    }
}

/// How far apart up and down the pairs (left, right) are once each eye is
/// corrected by VEGAS's solver's numbers `p` (each 0–1: Rotation, Keystone,
/// Zoom, Vertical offset), cell by cell of 360 pixels: the cells' mean
/// distances, in parts of the height.
fn disparity(
    pairs: &[(bv_track::Point, bv_track::Point)],
    [w, h]: [f64; 2],
    auto_crop: bool,
    p: [f64; 4],
) -> f64 {
    let c = Corrections {
        rotation: (p[0] - 0.5) * 89.0,
        keystone: ((p[1] - 0.5) * 30.0) / 100.0,
        zoom: p[2] - 0.5,
        vertical: p[3] - 0.5,
        auto_crop,
        ..Corrections::default()
    };
    let [a, b] = eyes(&c, [w, h]);
    let (columns, rows) = (((w / 360.0) as usize).max(1), ((h / 360.0) as usize).max(1));
    let mut cells = vec![(0usize, 0.0f64); columns * rows];
    for (l, r) in pairs {
        let (l, r) = (l.map(f64::from), r.map(f64::from));
        let col = ((l[0] / w * columns as f64) as usize).min(columns - 1);
        let row = ((l[1] / h * rows as f64) as usize).min(rows - 1);
        let cell = &mut cells[row * columns + col];
        cell.0 += 1;
        cell.1 += (apply(&a, l)[1] - apply(&b, r)[1]).abs() / h;
    }
    let used: Vec<f64> = cells
        .iter()
        .filter(|c| c.0 > 0)
        .map(|c| c.1 / c.0 as f64)
        .collect();
    used.iter().sum::<f64>() / used.len().max(1) as f64
}

/// The solver's numbers (each 0–1) that make `cost` least, from the
/// middle (no correction), by Nelder and Mead's simplex.
fn least(cost: impl Fn([f64; 4]) -> f64) -> [f64; 4] {
    let held = |p: [f64; 4]| p.map(|v| v.clamp(0.0, 1.0));
    let mut simplex: Vec<([f64; 4], f64)> = (0..5)
        .map(|k| {
            let mut p = [0.5; 4];
            if k > 0 {
                p[k - 1] += 0.05;
            }
            (p, cost(p))
        })
        .collect();
    for _ in 0..2000 {
        simplex.sort_by(|a, b| a.1.total_cmp(&b.1));
        if simplex[4].1 - simplex[0].1 < 1e-12 {
            break;
        }
        let mid: [f64; 4] =
            std::array::from_fn(|i| simplex[..4].iter().map(|s| s.0[i]).sum::<f64>() / 4.0);
        let toward = |t: f64| {
            held(std::array::from_fn(|i| {
                mid[i] + t * (simplex[4].0[i] - mid[i])
            }))
        };
        let r = toward(-1.0);
        let fr = cost(r);
        if fr < simplex[0].1 {
            let e = toward(-2.0);
            let fe = cost(e);
            simplex[4] = if fe < fr { (e, fe) } else { (r, fr) };
        } else if fr < simplex[3].1 {
            simplex[4] = (r, fr);
        } else {
            let c = toward(0.5);
            let fc = cost(c);
            if fc < simplex[4].1 {
                simplex[4] = (c, fc);
            } else {
                let best = simplex[0].0;
                for s in simplex.iter_mut().skip(1) {
                    s.0 = std::array::from_fn(|i| best[i] + 0.5 * (s.0[i] - best[i]));
                    s.1 = cost(s.0);
                }
            }
        }
    }
    simplex.sort_by(|a, b| a.1.total_cmp(&b.1));
    simplex[0].0
}

/// Auto correct: the picture at `time` as both eyes, its features followed
/// from the left eye's to the right eye's, the corrections that line them
/// up keyed at `time` (set when not animated); too few: VEGAS's message.
fn auto_correct(fx: Fx, time: f64) {
    fx.persistent_message(None);
    let Some(img) = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time) else {
        return;
    };
    let source = Buf::read(Some(&img));
    drop(img);
    if source.width() < 2 || source.height() < 2 {
        return;
    }
    let s = Settings::at(fx, time);
    let flipped = |k: c_int| s.flips.map(|f| f == k || f == 3);
    let (left, right) = (prepared(&source, flipped(1)), prepared(&source, flipped(2)));
    let (w, h) = (left.width as f32, left.height as f32);
    let area = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]];
    let tuning = bv_track::Tuning {
        features: 10_000,
        ..bv_track::Precision::Accurate.tuning()
    };
    let pairs =
        bv_track::follow_features_tuned(&left, &right, &area, false, tuning).unwrap_or_default();
    if pairs.len() < 30 {
        fx.persistent_message_as(
            kOfxMessageMessage,
            Some(c"Unable to find enough similarities in images to correct automatically."),
        );
        return;
    }
    let size = [f64::from(w), f64::from(h)];
    let p = least(|p| disparity(&pairs, size, s.auto_crop, p));
    let found = [
        (ROTATION, (p[0] - 0.5) * 89.0),
        (KEYSTONE, ((p[1] - 0.5) * 30.0) / 100.0),
        (ZOOM, p[2] - 0.5),
        (VERTICAL, p[3] - 0.5),
    ];
    for (name, v) in found {
        let param = fx.param(name);
        if param.props().int(kOfxParamPropIsAnimating, 0) != 0 {
            param.set_f64_at(time, v);
        } else {
            param.set_f64(v);
        }
    }
}

/// The plug-in's entry point.
pub(crate) unsafe extern "C" fn main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if suites().is_none() {
        return kOfxStatErrMissingHostFeature;
    }
    let fx = Fx(handle as OfxImageEffectHandle);
    let (args, out) = (Props(in_args), Props(out_args));
    if is(action, kOfxActionDescribe) {
        describe(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        describe_in_context(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        render(fx, args)
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        let time = args.dbl(kOfxPropTime, 0);
        if Settings::at(fx, time).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        let time = args.dbl(kOfxPropTime, 0);
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        let all = fx
            .clip(kOfxImageEffectSimpleSourceClipName)
            .rod(time)
            .unwrap_or(roi);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &all);
        kOfxStatOK
    } else if is(action, kOfxActionInstanceChanged) {
        if args.is(kOfxPropType, kOfxTypeParameter) {
            let time = args.dbl(kOfxPropTime, 0);
            if args.is(kOfxPropName, AUTO_CROP) {
                // VEGAS's rule, on a change of Automatic crop only.
                let auto = fx.param(AUTO_CROP).int_at(time) != 0;
                fx.param(CROP).set_enabled(!auto);
            } else if args.is(kOfxPropName, AUTO_ALIGN) {
                auto_correct(fx, time);
            }
        }
        kOfxStatOK
    } else if is(action, kOfxActionLoad)
        || is(action, kOfxActionUnload)
        || is(action, kOfxActionCreateInstance)
        || is(action, kOfxActionDestroyInstance)
    {
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Stereoscopic 3D Adjust");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Utility");
    p.set_str(
        kOfxPropPluginDescription,
        c"Provides adjustments for Stereoscopic 3D media.",
    );
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextFilter],
    );
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    p.set_int(kOfxImageEffectPropSupportsTiles, 0);
    p.set_int(kOfxImageEffectPropSupportsMultiResolution, 1);
    p.set_int(kOfxImageEffectPluginPropHostFrameThreading, 0);
    p.set_str(
        kOfxImageEffectPluginRenderThreadSafety,
        kOfxImageEffectRenderFullySafe,
    );
}

fn number(
    fx: Fx,
    (name, label): (&CStr, &CStr),
    (min, max): (f64, f64),
    (shown_min, shown_max): (f64, f64),
    (step, digits): (f64, c_int),
    parent: Option<&CStr>,
) -> crate::ofx::Props {
    let p = fx.define_param(kOfxParamTypeDouble, name, label);
    p.set_dbl(kOfxParamPropDefault, 0.0);
    p.set_dbl(kOfxParamPropMin, min);
    p.set_dbl(kOfxParamPropMax, max);
    p.set_dbl(kOfxParamPropDisplayMin, shown_min);
    p.set_dbl(kOfxParamPropDisplayMax, shown_max);
    p.set_dbl(kOfxParamPropIncrement, step);
    p.set_int(kOfxParamPropDigits, digits);
    if let Some(g) = parent {
        p.set_str(kOfxParamPropParent, g);
    }
    p
}

fn describe_in_context(fx: Fx) {
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentAlpha];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);
    let offsets = ((-0.5, 0.5), (-0.1, 0.1), (0.0001, 4));
    number(
        fx,
        (HORIZONTAL, c"Horizontal offset"),
        offsets.0,
        offsets.1,
        offsets.2,
        None,
    );
    fx.define_param(kOfxParamTypeGroup, CORRECTIONS, c"Corrections");
    let group = Some(CORRECTIONS);
    number(
        fx,
        (VERTICAL, c"Vertical offset"),
        offsets.0,
        offsets.1,
        offsets.2,
        group,
    );
    number(fx, (ZOOM, c"Zoom"), offsets.0, offsets.1, offsets.2, group);
    number(
        fx,
        (KEYSTONE, c"Keystone"),
        (-0.15, 0.15),
        (-0.1, 0.1),
        (0.0001, 4),
        group,
    );
    let p = number(
        fx,
        (ROTATION, c"Rotation"),
        (-44.999, 44.999),
        (-15.0, 15.0),
        (0.001, 3),
        group,
    );
    p.set_str(kOfxParamPropDoubleType, kOfxParamDoubleTypeAngle);
    let p = fx.define_param(kOfxParamTypePushButton, AUTO_ALIGN, c"Auto correct");
    p.set_str(kOfxParamPropParent, CORRECTIONS);
    for (name, label) in [(FLIP_H, c"Flip horizontal"), (FLIP_V, c"Flip vertical")] {
        let p = fx.define_param(kOfxParamTypeChoice, name, label);
        p.set_strs(kOfxParamPropChoiceOption, &FLIPS);
        p.set_int(kOfxParamPropDefault, 0);
        p.set_str(kOfxParamPropParent, CORRECTIONS);
    }
    let p = fx.define_param(kOfxParamTypeBoolean, AUTO_CROP, c"Automatic crop");
    p.set_int(kOfxParamPropDefault, 1);
    number(
        fx,
        (CROP, c"Crop"),
        (0.0, 0.5),
        (0.0, 0.5),
        (0.001, 3),
        None,
    );
    fx.define_param(kOfxParamTypeGroup, WINDOWS, c"Floating windows");
    for (name, label) in [(WINDOW_LEFT, c"Left"), (WINDOW_RIGHT, c"Right")] {
        number(
            fx,
            (name, label),
            (0.0, 0.5),
            (0.0, 0.25),
            (0.0001, 4),
            Some(WINDOWS),
        );
    }
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    if let Some(s) = &src
        && (s.depth != out.depth || s.channels != out.channels)
    {
        return kOfxStatErrImageFormat;
    }
    let source = Buf::read(src.as_ref());
    drop(src);
    let result = adjusted(&source, &Settings::at(fx, time));
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            horizontal: 0.0,
            vertical: 0.0,
            zoom: 0.0,
            keystone: 0.0,
            rotation: 0.0,
            flips: [0, 0],
            auto_crop: false,
            crop: 0.0,
            windows: [0.0, 0.0],
        }
    }

    fn near(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    #[test]
    fn each_eye_gets_half_of_each_correction_the_other_way() {
        let c = Corrections {
            horizontal: 0.1,
            vertical: 0.04,
            ..Corrections::default()
        };
        let [a, b] = eyes(&c, [1920.0, 1080.0]);
        // The left eye's picture goes left and up, the right's the other way.
        assert!(near(apply(&a, [960.0, 540.0]), [864.0, 561.6]));
        assert!(near(apply(&b, [960.0, 540.0]), [1056.0, 518.4]));
        // Zoom: smaller in the left eye, larger in the right, about the middle.
        let c = Corrections {
            zoom: 0.2,
            ..Corrections::default()
        };
        let [a, b] = eyes(&c, [1920.0, 1080.0]);
        assert!(near(apply(&a, [0.0, 0.0]), [96.0, 54.0]));
        assert!(near(apply(&b, [0.0, 0.0]), [-96.0, -54.0]));
    }

    #[test]
    fn rotation_turns_by_vegas_divisor_and_keystone_leans_the_edges() {
        let c = Corrections {
            rotation: 10.0,
            ..Corrections::default()
        };
        let [a, _] = eyes(&c, [100.0, 100.0]);
        let p = apply(&a, [100.0, 50.0]);
        let t: f64 = -10.0 / 365.0 * VEGAS_PI;
        assert!(
            near(p, [50.0 + 50.0 * t.cos(), 50.0 + 50.0 * t.sin()]),
            "{p:?}"
        );
        let c = Corrections {
            keystone: 0.1,
            ..Corrections::default()
        };
        let [a, b] = eyes(&c, [100.0, 100.0]);
        let close =
            |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).abs() < 1e-4 && (p[1] - q[1]).abs() < 1e-4;
        assert!(close(apply(&a, [0.0, 0.0]), [0.0, 5.0]));
        assert!(close(apply(&a, [100.0, 0.0]), [100.0, -5.0]));
        assert!(close(apply(&b, [0.0, 0.0]), [0.0, -5.0]));
        assert!(close(apply(&b, [100.0, 100.0]), [100.0, 95.0]));
    }

    #[test]
    fn automatic_crop_zooms_in_a_hair_even_at_rest() {
        let c = Corrections {
            auto_crop: true,
            ..Corrections::default()
        };
        let [a, _] = eyes(&c, [1920.0, 1080.0]);
        // VEGAS's crossing lands half a pixel off the corners.
        let out = 0.5f64.hypot(0.5);
        let crop = out / (960.0f64.hypot(540.0) - out);
        assert!((a[0][0] - (1.0 + crop)).abs() < 1e-12, "{}", a[0][0]);
        // A shift needs more.
        let c = Corrections {
            horizontal: 0.1,
            auto_crop: true,
            ..Corrections::default()
        };
        let [a, _] = eyes(&c, [1920.0, 1080.0]);
        assert!(a[0][0] > 1.09, "{}", a[0][0]);
    }

    #[test]
    fn the_crossing_rounds_as_integer_geometry() {
        let x = crossing([960.0, 540.0], [0.0, 0.0], [0.0, 0.0], [1920.0, 0.0]).unwrap();
        assert_eq!(x, [0.5, 0.5]);
        assert!(crossing([0.0, 1.0], [1.0, 2.0], [0.0, 0.0], [1.0, 0.0]).is_none());
    }

    #[test]
    fn at_rest_without_automatic_crop_nothing_changes() {
        let s = settings();
        assert!(s.changes_nothing());
        let mut moved = settings();
        moved.horizontal = 0.25;
        moved.auto_crop = true;
        assert!(!moved.changes_nothing());
        // An identity warp reads each pixel where it is.
        let r = reads(&I3, true, [8, 4]);
        assert_eq!(r.at(3, 2, 8), [96, 64]);
    }

    #[test]
    fn a_shift_moves_the_picture_whole_pixels_and_32nds() {
        let (w, h) = (8usize, 2usize);
        let data: Vec<[f32; 4]> = (0..w * h)
            .map(|i| [(i % w) as f32, 0.0, 0.0, 1.0])
            .collect();
        let source = Buf {
            bounds: [0, 0, w as c_int, h as c_int],
            data,
        };
        let mut s = settings();
        // The left eye moves left by an eighth of the width's half: half a pixel.
        s.horizontal = 0.125;
        let out = adjusted(&source, &s);
        assert!((out.data[2][0] - 2.5).abs() < 1e-6, "{:?}", out.data[2]);
        // The last column reads past the picture: half of it.
        assert!(
            (out.data[w - 1][3] - 0.5).abs() < 1e-6,
            "{:?}",
            out.data[w - 1]
        );
    }

    #[test]
    fn the_floating_window_is_black_with_its_last_column_partly() {
        let mut s = settings();
        s.windows = [0.3, 0.0];
        let b = band(&s, 10);
        // 3 columns exactly: no part.
        assert_eq!((b.from, b.to, b.part), (0, 3, None));
        s.windows = [0.25, 0.0];
        let b = band(&s, 10);
        assert_eq!((b.from, b.to), (0, 2));
        assert_eq!(b.part, Some((2, 0.5)));
    }

    #[test]
    fn a_perspective_map_reads_as_the_affine_one_where_they_agree() {
        let m = mul(&shift(3.25, -1.5), &scale(1.5));
        let (a, p) = (reads(&m, true, [64, 16]), reads(&m, false, [64, 16]));
        for (x, y) in [(0, 0), (17, 5), (63, 15)] {
            let (pa, pp) = (a.at(x, y, 64), p.at(x, y, 64));
            assert!(
                (pa[0] - pp[0]).abs() <= 1 && (pa[1] - pp[1]).abs() <= 1,
                "{pa:?} {pp:?}"
            );
        }
    }

    #[test]
    fn auto_correct_finds_nothing_between_alike_eyes() {
        let pairs: Vec<(bv_track::Point, bv_track::Point)> = (0..60)
            .map(|i| {
                let p = [(i * 31 % 1900) as f32, (i * 17 % 1060) as f32];
                (p, p)
            })
            .collect();
        let p = least(|p| disparity(&pairs, [1920.0, 1080.0], false, p));
        assert!(p.iter().all(|v| (v - 0.5).abs() < 1e-6), "{p:?}");
        // A vertical offset between them is found.
        let shifted: Vec<_> = pairs
            .iter()
            .map(|(l, r)| (*l, [r[0], r[1] + 21.6]))
            .collect();
        let p = least(|p| disparity(&shifted, [1920.0, 1080.0], false, p));
        // The right eye's points sit 21.6 higher: the left goes up half,
        // the right down half, a vertical offset of 0.02.
        assert!(((p[3] - 0.5) - 0.02).abs() < 2e-3, "{p:?}");
    }
}
