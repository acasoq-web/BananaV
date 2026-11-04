//! Color Grading's work on a color, as VEGAS's Color Grading does it with
//! the graphics card on (its usual way): the Camera LUT; the HSL Curves,
//! one after another, each keyed on what the one before made; the first
//! part — White Balance, Levels, the RL Color Wheels and the Color
//! Curves —; the second — Lift, then Gain, Offset and Gamma by channel,
//! then by Y —; the third — Saturation, Exposure, Brightness, Contrast and
//! the panel's Exposure — and the HSL, always, after it; the Look LUT.
//! Between the steps the color is held within 0–1, as VEGAS's steps keep
//! it, so that a color taken past white in one step is cut there; the
//! HSL that ends the third part takes it as it is, and a color past white
//! there turns white.
//!
//! The curves — Color Curves', HSL Curves' and the RL Color Wheels' —
//! are Bézier curves made tables (511 entries; 383 for the RL wheels): at
//! each x the curve's parameter comes from a single Newton step off the
//! straight guess more often than not (the step stops once it moves less
//! than the curve's x's sum over 15), so the tables sit a little off the
//! exact curve, as VEGAS's do. The tables are read straight between their
//! entries, the input held within 0–1.

use bv_lut::{Interpolation, Lut};

/// A curve's point: where it is and its handles into and out of it (from
/// the point), as the curves' text holds them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Knot {
    pub x: f32,
    pub y: f32,
    pub ix: f32,
    pub iy: f32,
    pub ox: f32,
    pub oy: f32,
}

impl Knot {
    pub fn new(x: f32, y: f32, ix: f32, iy: f32, ox: f32, oy: f32) -> Knot {
        Knot {
            x,
            y,
            ix,
            iy,
            ox,
            oy,
        }
    }
}

/// An untouched curve's text: the effect leaves such a curve out.
pub const DEFAULT_CURVE: &str = "2:0.0:0.0:-0.2:0.0:0.2:0.2:1.0:1.0:-0.2:-0.2:0.2:0.0";

/// The points of a curve's text, `N:x:y:ix:iy:ox:oy:…` (as many points as
/// it holds whole).
pub fn parse_curve(text: &str) -> Vec<Knot> {
    let mut parts = text.split(':').filter(|s| !s.is_empty());
    let n = parts
        .next()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let numbers: Vec<f32> = parts
        .map(|s| s.trim().parse::<f64>().unwrap_or(0.0) as f32)
        .collect();
    numbers
        .as_chunks::<6>()
        .0
        .iter()
        .take(n)
        .map(|v| Knot::new(v[0], v[1], v[2], v[3], v[4], v[5]))
        .collect()
}

/// A curve's text for `knots`, five decimals each, as the panel writes it.
pub fn curve_text(knots: &[Knot]) -> String {
    let mut s = format!("{}:", knots.len());
    for k in knots {
        for v in [k.x, k.y, k.ix, k.iy, k.ox, k.oy] {
            s.push_str(&format!("{:.5}:", v));
        }
    }
    s
}

/// Whether a curve's text is the untouched one (or nothing).
pub fn untouched(text: &str) -> bool {
    text.trim().is_empty() || text.trim() == DEFAULT_CURVE
}

/// The parameter at which the Bézier with x control values `x0`…`x3`
/// reaches `x`: Newton's steps off the straight guess, each kept within
/// 0–1, until one moves less than the x's sum over 15 (−1 when none
/// does in a hundred).
fn solve(x0: f64, x1: f64, x2: f64, x3: f64, x: f64) -> f64 {
    let mut t = (x - x0) / (x3 - x0);
    let close = (x0 + x1 + x2 + x3) / 15.0;
    for _ in 0..100 {
        let u = 1.0 - t;
        let f = u * u * u * x0 + 3.0 * t * u * u * x1 + 3.0 * t * t * u * x2 + t * t * t * x3 - x;
        let slope = 6.0 * t * u * (x2 - x1) + 3.0 * u * u * (x1 - x0) + 3.0 * t * t * (x3 - x2);
        let next = t - f / slope;
        if (next - t).abs() < close {
            return next;
        }
        t = next.clamp(0.0, 1.0);
    }
    -1.0
}

fn bezier(y0: f64, y1: f64, y2: f64, y3: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * y0 + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * y3
}

/// One section's value at `x`: the parameter solved on the section's x's
/// (moved by `shift`), the y's at it.
fn section(a: &Knot, b: &Knot, x: f64, shift: f64) -> f64 {
    let (ax, bx) = (f64::from(a.x) + shift, f64::from(b.x) + shift);
    let t = solve(ax, ax + f64::from(a.ox), bx + f64::from(b.ix), bx, x);
    bezier(
        f64::from(a.y),
        f64::from(a.y) + f64::from(a.oy),
        f64::from(b.y) + f64::from(b.iy),
        f64::from(b.y),
        t,
    )
}

/// A curve's table: its value at x = 0, 1/(n − 1), 2/(n − 1)… while x
/// stays within 1 (the steps summed, so the last may fall short), the
/// curve moving on by at most one section an entry; `shift` (the RL
/// Offset wheel) slides the entries by about n·0.9875 times it, the end
/// they leave repeated.
pub(crate) fn table(knots: &[Knot], n: usize, shift: f32) -> Vec<f32> {
    let mut out = Vec::with_capacity(n);
    if knots.len() < 2 {
        return out;
    }
    let step = 1.0 / (n - 1) as f64;
    let (mut i, mut x) = (1usize, 0.0f64);
    loop {
        if f64::from(knots[i].x) < x {
            if i + 1 >= knots.len() {
                break;
            }
            i += 1;
        }
        out.push(section(&knots[i - 1], &knots[i], x, 0.0) as f32);
        x += step;
        if x > 1.0 {
            break;
        }
    }
    if shift.abs() > 1e-6 && !out.is_empty() {
        let v = (-(n as f64) * 0.9875 * f64::from(shift)) as f32;
        let m = if v < 0.0 {
            -(-v).round() as i64
        } else {
            v.round() as i64
        };
        let len = out.len() as i64;
        let old = out.clone();
        for (k, o) in out.iter_mut().enumerate() {
            let from = (k as i64 - m).clamp(0, len - 1);
            *o = old[from as usize];
        }
    }
    out
}

/// A hue curve's table when its points run past 0 and 1 (the turn's
/// copies): the same, its x's moved so that its first point is at 0.
fn turning_table(knots: &[Knot], n: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(n);
    let d = -f64::from(knots[0].x);
    let step = 1.0 / (n - 1) as f64;
    let (mut i, mut s) = (1usize, d);
    while s <= d + 1.0 {
        if f64::from(knots[i].x) + d < s {
            if i + 1 >= knots.len() {
                return out;
            }
            i += 1;
        }
        out.push(section(&knots[i - 1], &knots[i], s, d) as f32);
        s += step;
    }
    out
}

/// An HSL curve's table (`None` for a curve of fewer than two points):
/// one past 0 and 1 with a single point of its own is a flat line at its
/// height.
fn hsl_table(knots: &[Knot], n: usize) -> Option<Vec<f32>> {
    if knots.len() < 2 {
        return None;
    }
    let past = knots[0].x < 0.0 || knots[knots.len() - 1].x > 1.0;
    let t = if past && knots.len() == 3 {
        let m = knots[1];
        let ends = [
            Knot::new(0.0, m.y, 0.0, 0.0, 0.1, 0.0),
            m,
            Knot::new(1.0, m.y, -0.1, 0.0, 0.0, 0.0),
        ];
        table(&ends, n, 0.0)
    } else if past {
        turning_table(knots, n)
    } else {
        table(knots, n, 0.0)
    };
    (!t.is_empty()).then_some(t)
}

/// A table at `v`: straight between its entries, `v` held within 0–1; no
/// table changes nothing.
pub(crate) fn lookup(t: &[f32], v: f64) -> f64 {
    let n = t.len();
    if n == 0 {
        return v;
    }
    if v.is_nan() || v <= 0.0 {
        return f64::from(t[0]);
    }
    if v >= 1.0 {
        return f64::from(t[n - 1]);
    }
    let p = v * (n - 1) as f64;
    let i = p.floor() as usize;
    let f = p - i as f64;
    if i + 1 < n {
        f64::from(t[i]) + (f64::from(t[i + 1]) - f64::from(t[i])) * f
    } else {
        f64::from(t[i])
    }
}

// The RL Color Wheels: each channel's curve.

fn toward(len: f64, angle: f64) -> (f64, f64) {
    (len * angle.cos(), len * angle.sin())
}

/// The shadows' point(s), below the lower threshold `t`: the black raised
/// to s·t, its handle a third of the way toward (t, t); or, lowered, all
/// below x0 = t − max((s + 1)·t, 0.035) black.
fn shadows(s: f64, t: f64) -> Vec<Knot> {
    let mut k = Vec::new();
    if t == 0.0 {
        return k;
    }
    if s < 0.0 {
        let f = ((s + 1.0) * t).max(0.035);
        let x0 = (t - f) as f32;
        if x0 <= 0.0 {
            return k;
        }
        let third = x0 / 3.0;
        k.push(Knot::new(0.0, 0.0, 0.0, 0.0, third, 0.0));
        let x0d = f64::from(x0);
        let (ox, oy) = toward((t - x0d).hypot(t) / 3.0, t.atan2(t - x0d));
        k.push(Knot::new(x0, 0.0, -third, 0.0, ox as f32, oy as f32));
    } else {
        let y0 = (s * t) as f32;
        let (ox, oy) = toward(
            t.hypot(t - f64::from(y0)) / 3.0,
            (t - f64::from(y0)).atan2(t),
        );
        k.push(Knot::new(0.0, y0, 0.0, 0.0, ox as f32, oy as f32));
    }
    k
}

/// The midtones' point at (t1, t1): its handle in a third of the way back
/// to the point before, its handle out up (brighter), along (none) or
/// across (darker) by m.
fn midtones(k: &mut Vec<Knot>, m: f64, t1: f64, t2: f64) {
    let d = ((t2 - t1) * 0.9875).max(0.0);
    let (mut x, mut ix, mut iy) = (0.0, 0.0, 0.0);
    if let Some(p) = k.last() {
        x = t1;
        let (px, py) = (f64::from(p.x), f64::from(p.y));
        (ix, iy) = toward((px - t1).hypot(py - t1) / 3.0, (py - t1).atan2(px - t1));
    }
    let (ox, oy) = if m > 0.0 {
        (0.0, (d * m).max(1e-6))
    } else if m < 0.0 {
        ((m.abs() * d).max(1e-6), 0.0)
    } else {
        (d / 3.0, d / 3.0)
    };
    k.push(Knot::new(
        x as f32, t1 as f32, ix as f32, iy as f32, ox as f32, oy as f32,
    ));
}

/// The point at the upper threshold (t2, t2), when it is above the lower:
/// its handle in by m, none out.
fn upper(k: &mut Vec<Knot>, m: f64, t1: f64, t2: f64) {
    if t1 >= t2 {
        return;
    }
    let d = (t2 - t1) * 0.9875;
    let (ix, iy) = if m == 0.0 {
        (-(d / 3.0), -(d / 3.0))
    } else if m < 0.0 {
        (0.0, -(m.abs() * d).max(0.035))
    } else {
        (-(m.abs() * d).max(0.035), 0.0)
    };
    k.push(Knot::new(
        t2 as f32, t2 as f32, ix as f32, iy as f32, 0.0, 0.0,
    ));
}

/// The highlights, from the last point to white: (1, 1) as it is; raised,
/// white reached before x = 1 (or the last point's handle out bent toward
/// it when barely); lowered, white brought down. The point before keeps
/// its handle out as it was, but in the slight raise, as VEGAS's curve
/// does.
fn highlights(k: &mut Vec<Knot>, h: f64, t2: f64) {
    if t2 == 1.0 || k.is_empty() {
        return;
    }
    let last = k[k.len() - 1];
    let (lx, ly) = (f64::from(last.x), f64::from(last.y));
    if h == 0.0 {
        k.push(Knot::new(
            1.0,
            1.0,
            (lx - 1.0) as f32,
            (ly - 1.0) as f32,
            0.0,
            0.0,
        ));
        return;
    }
    let r = 1.0 - ly;
    if h > 0.0 {
        let f = (r * h - 0.035) as f32;
        if r * h < 0.035 {
            let n = k.len();
            k[n - 1].oy = (r / 3.0) as f32;
            k[n - 1].ox = ((1.0 - lx) / 3.0) as f32;
        } else {
            let x6 = 1.0 - f;
            k.push(Knot::new(
                x6,
                1.0,
                (lx - f64::from(x6)) as f32,
                (ly - 1.0) as f32,
                1.0 - x6,
                0.0,
            ));
        }
        k.push(Knot::new(1.0, 1.0, -f.max(1e-6), 0.0, 0.0, 0.0));
        return;
    }
    let end = (1.0 - h.abs() * r) as f32;
    k.push(Knot::new(
        1.0,
        end,
        (lx - 1.0) as f32,
        (ly - f64::from(end)) as f32,
        0.0,
        0.0,
    ));
}

/// An RL channel's curve: its shadows, midtones and highlights within the
/// thresholds.
pub(crate) fn rl_curve(s: f64, m: f64, h: f64, t1: f64, t2: f64) -> Vec<Knot> {
    let mut k = shadows(s, t1);
    midtones(&mut k, m, t1, t2);
    upper(&mut k, m, t1, t2);
    highlights(&mut k, h, t2);
    k
}

// Colors.

/// The Gamma wheel's value made the power's inverse: e^(…) on four
/// stretches (2.78 at 0, 1 at 1, 0.284 at 2…).
pub(crate) fn gamma_scale(g: f64) -> f64 {
    let g = f64::from(g as f32);
    let e = if g > 1.7 {
        g - (2.462 - g * 0.86)
    } else if g < 1.0 {
        if g >= 0.8 {
            g - (1.55 - g * 0.55)
        } else {
            g - (g * 0.1111 + 1.0211)
        }
    } else {
        g - 1.0
    };
    e.exp()
}

/// HSL (hue 0–1, saturation, lightness) of a color.
fn to_hsl(r: f64, g: f64, b: f64) -> [f64; 3] {
    let lo = r.min(g).min(b);
    let hi = r.max(g).max(b);
    let l = (lo + hi) * 0.5;
    if l.is_nan() || l <= 0.0 {
        return [0.0, 0.0, l];
    }
    let d = hi - lo;
    if d.is_nan() || d <= 0.0 {
        return [0.0, d, l];
    }
    let s = d / if l > 0.5 { (2.0 - hi) - lo } else { lo + hi };
    let (dr, dg, db) = ((hi - r) / d, (hi - g) / d, (hi - b) / d);
    let h = if r == hi {
        if g == lo {
            (db + 5.0) / 6.0
        } else {
            (1.0 - dg) / 6.0
        }
    } else if g != hi {
        if r == lo {
            (dg + 3.0) / 6.0
        } else {
            (5.0 - dr) / 6.0
        }
    } else if b == lo {
        (dr + 1.0) / 6.0
    } else {
        (3.0 - db) / 6.0
    };
    [h, s, l]
}

/// A color of HSL `h` (0–1, its whole turns dropped), `s`, `l`.
fn from_hsl(h: f64, s: f64, l: f64) -> [f64; 3] {
    let h = h - h.floor();
    let q = if l > 0.5 {
        (l + s) - l * s
    } else {
        (s + 1.0) * l
    };
    if q <= 0.0 {
        return [0.0; 3];
    }
    let i = (h * 6.0).trunc();
    let p = (l + l) - q;
    let f = (h * 6.0 - i) * (q - p);
    let (down, up) = (q - f, f + p);
    match i as i64 {
        0 | 6 => [q, up, p],
        1 => [down, q, p],
        2 => [p, q, up],
        3 => [p, down, q],
        4 => [up, p, q],
        5 => [q, p, down],
        _ => [0.0; 3],
    }
}

/// HSV (hue 0–1) of a color, as the hue curve's step works it out.
fn to_hsv(r: f64, g: f64, b: f64) -> [f64; 3] {
    let (p0, p1, p2, p3) = if g < b {
        (b, g, -1.0, 2.0 / 3.0)
    } else {
        (g, b, 0.0, -1.0 / 3.0)
    };
    let (q0, q1, q2, q3) = if p0 < r {
        (r, p1, p2, p0)
    } else {
        (p0, p1, p3, r)
    };
    let d = q0 - q3.min(q1);
    let e = 1e-10;
    [(q2 + (q3 - q1) / (6.0 * d + e)).abs(), d / (q0 + e), q0]
}

fn from_hsv(h: f64, s: f64, v: f64) -> [f64; 3] {
    [1.0, 2.0 / 3.0, 1.0 / 3.0].map(|k| {
        let f = h + k;
        let x = ((f - f.floor()) * 6.0 - 3.0).abs() - 1.0;
        v * (1.0 + (x.clamp(0.0, 1.0) - 1.0) * s)
    })
}

/// The lightness L* at which white stands (sRGB, D65): what the
/// lightness steps hold L* within.
const WHITE_L: f64 = 95.580666;

/// L*a*b* (D65) of a color's values (sRGB's curve).
fn to_lab(r: f64, g: f64, b: f64) -> [f64; 3] {
    let lin = |c: f64| {
        if c > 0.04045 {
            ((c + 0.055) / 1.055).abs().powf(2.4)
        } else {
            c / 12.92
        }
    };
    let (r, g, b) = (lin(r), lin(g), lin(b));
    let f = |t: f64| {
        if t > 0.008856 {
            t.abs().powf(0.3333)
        } else {
            7.787 * t + 16.0 / 116.0
        }
    };
    let fx = f((r * 0.4124 + g * 0.3576 + b * 0.1805) / 0.95047);
    let fy = f(r * 0.2126 + g * 0.7152 + b * 0.0722);
    let fz = f((r * 0.0193 + g * 0.1192 + b * 0.9505) / 1.08883);
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn from_lab(l: f64, a: f64, bb: f64) -> [f64; 3] {
    let fy = (l + 16.0) / 116.0;
    let (fx, fz) = (a / 500.0 + fy, fy - bb / 200.0);
    let inv = |t: f64| {
        if t * t * t > 0.008856 {
            t * t * t
        } else {
            (t - 16.0 / 116.0) / 7.787
        }
    };
    let (x, y, z) = (inv(fx) * 0.95047, inv(fy), inv(fz) * 1.08883);
    let enc = |t: f64| {
        if t > 0.0031308 {
            1.055 * t.abs().powf(1.0 / 2.4) - 0.055
        } else {
            12.92 * t
        }
    };
    [
        enc(x * 3.2406 + y * -1.5372 + z * -0.4986),
        enc(x * -0.9689 + y * 1.8758 + z * 0.0415),
        enc(x * 0.0557 + y * -0.204 + z * 1.057),
    ]
}

// White Balance.

/// Robertson's isotemperature lines: reciprocal megakelvin, u, v, slope.
const ROBERTSON: [[f64; 4]; 31] = [
    [0.0, 0.18006, 0.26352, -0.24341],
    [10.0, 0.18066, 0.26589, -0.25479],
    [20.0, 0.18133, 0.26846, -0.26876],
    [30.0, 0.18208, 0.27119, -0.28539],
    [40.0, 0.18293, 0.27407, -0.30470],
    [50.0, 0.18388, 0.27709, -0.32675],
    [60.0, 0.18494, 0.28021, -0.35156],
    [70.0, 0.18611, 0.28342, -0.37915],
    [80.0, 0.18740, 0.28668, -0.40955],
    [90.0, 0.18880, 0.28997, -0.44278],
    [100.0, 0.19032, 0.29326, -0.47888],
    [125.0, 0.19462, 0.30141, -0.58204],
    [150.0, 0.19962, 0.30921, -0.70471],
    [175.0, 0.20525, 0.31647, -0.84901],
    [200.0, 0.21142, 0.32312, -1.0182],
    [225.0, 0.21807, 0.32909, -1.2168],
    [250.0, 0.22511, 0.33439, -1.4512],
    [275.0, 0.23247, 0.33904, -1.7298],
    [300.0, 0.24010, 0.34308, -2.0637],
    [325.0, 0.24702, 0.34655, -2.4681],
    [350.0, 0.25591, 0.34951, -2.9641],
    [375.0, 0.26400, 0.35200, -3.5814],
    [400.0, 0.27218, 0.35407, -4.3633],
    [425.0, 0.28039, 0.35577, -5.3762],
    [450.0, 0.28863, 0.35714, -6.7262],
    [475.0, 0.29685, 0.35823, -8.5955],
    [500.0, 0.30505, 0.35907, -11.324],
    [525.0, 0.31320, 0.35968, -15.628],
    [550.0, 0.32129, 0.36011, -23.325],
    [575.0, 0.32931, 0.36038, -40.770],
    [600.0, 0.33724, 0.36051, -116.45],
];

/// The chromaticity (x, y) of temperature `cct` (K) and `tint` (a 3000th
/// of the isotherm's normal), by Robertson's method.
pub(crate) fn chromaticity(cct: f64, tint: f64) -> (f64, f64) {
    let mired = 1e6 / cct;
    let mut i = 0;
    while !(mired < ROBERTSON[i + 1][0] || i == 29) {
        i += 1;
    }
    let (lo, hi) = (ROBERTSON[i], ROBERTSON[i + 1]);
    let w = (hi[0] - mired) / (hi[0] - lo[0]);
    let wh = 1.0 - w;
    let (nl, nh) = ((lo[3] * lo[3] + 1.0).sqrt(), (hi[3] * hi[3] + 1.0).sqrt());
    let du = (1.0 / nh) * wh + (1.0 / nl) * w;
    let dv = (hi[3] / nh) * wh + (lo[3] / nl) * w;
    let len = (dv * dv + du * du).sqrt();
    let v = wh * hi[2] + w * lo[2] + (dv / len) * tint * (-1.0 / 3000.0);
    let u = wh * hi[1] + w * lo[1] + (du / len) * tint * (-1.0 / 3000.0);
    let den = (u - v * 4.0) + 2.0;
    ((u * 1.5) / den, v / den)
}

type Matrix = [[f64; 3]; 3];

fn product(a: &Matrix, b: &Matrix) -> Matrix {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum()))
}

fn inverse(m: &Matrix) -> Matrix {
    let c = |r: usize, k: usize| {
        let (r1, r2) = ((r + 1) % 3, (r + 2) % 3);
        let (k1, k2) = ((k + 1) % 3, (k + 2) % 3);
        m[r1][k1] * m[r2][k2] - m[r1][k2] * m[r2][k1]
    };
    let det: f64 = (0..3).map(|k| m[0][k] * c(0, k)).sum();
    std::array::from_fn(|r| std::array::from_fn(|k| c(k, r) / det))
}

/// sRGB's (linear) to XYZ.
const RGB_XYZ: Matrix = [
    [0.412456, 0.357576, 0.180437],
    [0.212673, 0.715152, 0.072175],
    [0.019334, 0.119192, 0.950304],
];

/// The cone matrix White Balance adapts with: Bradford's, one term's sign
/// turned, which gives VEGAS's White Balance's colors.
const CONES: Matrix = [
    [0.8951, -0.7502, 0.0389],
    [0.2664, 1.7135, 0.0685],
    [-0.1614, 0.0367, 1.0296],
];

/// White Balance's matrix for `temperature` (K off D65's) and `tint`, as
/// it is applied to the colors' values raised to 2.4: the white the two
/// make over D65's, through the cones, its rows and columns swapped.
pub(crate) fn balance_matrix(temperature: f64, tint: f64) -> Matrix {
    let (x, y) = chromaticity(temperature + 6502.082197, tint + 9.766656463);
    let d: Matrix = [
        [(x / y) / 0.95047, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, ((1.0 - x - y) / y) / 1.08883],
    ];
    let m = product(
        &product(&product(&product(&RGB_XYZ, &inverse(&CONES)), &d), &CONES),
        &inverse(&RGB_XYZ),
    );
    std::array::from_fn(|r| std::array::from_fn(|c| m[c][r]))
}

// The settings.

/// A wheel's color (red, green, blue) and its Y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wheel {
    pub rgb: [f64; 3],
    pub y: f64,
}

impl Wheel {
    pub const fn neutral(v: f64) -> Wheel {
        Wheel { rgb: [v; 3], y: v }
    }

    fn at(&self, v: f64) -> bool {
        self.rgb == [v; 3] && self.y == v
    }
}

/// A LUT row's table: a `.cube` file's, read straight or by tetrahedra,
/// or the Apple Log curve BananaV knows (to Rec. 709's linear light, or
/// to its video).
#[derive(Clone, Debug)]
pub enum LutTable {
    Cube {
        lut: std::sync::Arc<Lut>,
        tetrahedral: bool,
    },
    AppleLog {
        video: bool,
    },
}

/// A LUT row: its table, mixed in by `gain`.
#[derive(Clone, Debug)]
pub struct LutStep {
    pub table: LutTable,
    pub gain: f64,
}

/// Every value Color Grading works with (the curves as their text).
#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    /// Lift, Gamma, Gain, Offset.
    pub wheels: [Wheel; 4],
    /// Shadows, Midtones, Highlights, Offset.
    pub rl: [Wheel; 4],
    /// The lower and the upper threshold.
    pub thresholds: [f64; 2],
    /// Red, green, blue, Y; then Hue vs Hue, Hue vs Saturation, Hue vs
    /// Luminance, Saturation vs Saturation, Saturation vs Luminance,
    /// Luminance vs Saturation.
    pub curves: [String; 10],
    pub saturation: f64,
    /// 2^E (not on the panel).
    pub exposure: f64,
    /// The panel's Exposure.
    pub exposure_ln: f64,
    pub brightness: f64,
    pub contrast: f64,
    pub contrast_center: f64,
    pub temperature: f64,
    pub tint: f64,
    /// Input min and max, output min and max.
    pub levels: [f64; 4],
    /// Hue, saturation, luminance.
    pub hsl: [f64; 3],
}

impl Default for Params {
    fn default() -> Params {
        Params {
            wheels: [
                Wheel::neutral(0.0),
                Wheel::neutral(1.0),
                Wheel::neutral(1.0),
                Wheel::neutral(0.0),
            ],
            rl: [Wheel::neutral(0.0); 4],
            thresholds: [0.25, 0.5],
            curves: std::array::from_fn(|_| DEFAULT_CURVE.to_string()),
            saturation: 1.0,
            exposure: 0.0,
            exposure_ln: 0.0,
            brightness: 0.0,
            contrast: 0.0,
            contrast_center: 0.5,
            temperature: 0.0,
            tint: 0.0,
            levels: [0.0, 1.0, 0.0, 1.0],
            hsl: [0.0, 1.0, 1.0],
        }
    }
}

/// The HSL Curves, in their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HslCurve {
    HueHue,
    HueSat,
    HueLum,
    SatSat,
    SatLum,
    LumSat,
}

impl HslCurve {
    pub const ALL: [HslCurve; 6] = [
        HslCurve::HueHue,
        HslCurve::HueSat,
        HslCurve::HueLum,
        HslCurve::SatSat,
        HslCurve::SatLum,
        HslCurve::LumSat,
    ];
}

/// A CDL step: slope, offset and power per channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Cdl {
    pub slope: [f64; 3],
    pub offset: [f64; 3],
    pub power: [f64; 3],
}

impl Cdl {
    fn apply(&self, v: f64, i: usize) -> f64 {
        let x = v * self.slope[i] + self.offset[i];
        if x > 0.0 { x.powf(self.power[i]) } else { x }
    }
}

/// The panel's Exposure: its table and what the color is scaled by
/// before it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExposureCurve {
    pub table: Vec<f32>,
    pub pre: f64,
}

/// The settings ready to color pixels: what changes nothing is left out.
#[derive(Clone, Debug, Default)]
pub struct Grade {
    pub(crate) camera: Option<LutStep>,
    pub(crate) hsl_curves: Vec<(HslCurve, Vec<f32>)>,
    pub(crate) balance: Option<Matrix>,
    pub(crate) levels: Option<[f64; 4]>,
    pub(crate) rl_rgb: Option<[Vec<f32>; 3]>,
    pub(crate) rl_y: Option<Vec<f32>>,
    pub(crate) curves_rgb: Option<[Vec<f32>; 3]>,
    pub(crate) curve_y: Option<Vec<f32>>,
    pub(crate) lift: Option<[f64; 3]>,
    pub(crate) cdl: [Option<Cdl>; 2],
    pub(crate) saturation: Option<f64>,
    pub(crate) exposure: Option<f64>,
    pub(crate) brightness: Option<f64>,
    /// The contrast's scale and center.
    pub(crate) contrast: Option<(f64, f64)>,
    pub(crate) exposure_ln: Option<ExposureCurve>,
    /// Hue (turns), saturation and luminance: the HSL that ends the third
    /// part.
    pub(crate) hsl: [f64; 3],
    pub(crate) look: Option<LutStep>,
    /// Each step's color made 8-bit as well, as VEGAS's pictures between
    /// its steps are: for measuring against VEGAS.
    pub quantize: bool,
}

/// The tables' entries.
const ENTRIES: usize = 511;
const RL_ENTRIES: usize = 383;

impl Grade {
    /// `p` ready, with the Camera and Look LUTs (each `None` when off).
    pub fn new(p: &Params, camera: Option<LutStep>, look: Option<LutStep>) -> Grade {
        let curves: Vec<Vec<Knot>> = p.curves.iter().map(|t| parse_curve(t)).collect();
        let touched = |i: usize| !untouched(&p.curves[i]);
        let hsl_curves = HslCurve::ALL
            .iter()
            .enumerate()
            .filter(|&(i, _)| touched(4 + i))
            .filter_map(|(i, &kind)| Some((kind, hsl_table(&curves[4 + i], ENTRIES)?)))
            .collect();
        let balance = (p.temperature.abs() > 1e-6 || p.tint.abs() > 1e-6)
            .then(|| balance_matrix(p.temperature, p.tint));
        let levels = {
            let [a, mut b, c, mut d] = p.levels.map(|v| f64::from(v as f32));
            (a != 0.0 || b != 1.0 || c != 0.0 || d != 1.0).then(|| {
                if a == c && b == d && a == b {
                    b += if b < 0.5 { -1e-6 } else { 1e-6 };
                } else {
                    if (a - b).abs() <= 1e-6 {
                        b += 1e-6;
                    }
                    if (c - d).abs() <= 1e-6 {
                        d += 1e-6;
                    }
                }
                [a, b, c, d]
            })
        };
        let [t1, t2] = p.thresholds.map(|v| f64::from(v as f32));
        let rl = |w: usize, c: usize| -> f64 {
            let wheel = &p.rl[w];
            f64::from((if c < 3 { wheel.rgb[c] } else { wheel.y }) as f32)
        };
        let rl_table = |c: usize| {
            table(
                &rl_curve(rl(0, c), rl(1, c), rl(2, c), t1, t2),
                RL_ENTRIES,
                rl(3, c) as f32,
            )
        };
        let any_rgb = (0..4).any(|w| (0..3).any(|c| rl(w, c) != 0.0));
        let any_y = (0..4).any(|w| rl(w, 3) != 0.0);
        let [lift, gamma, gain, offset] = p.wheels;
        let gamma_power = |g: f64| {
            let g = if g >= 1e-6 { g } else { 1e-6 };
            f64::from((1.0 / f64::from(gamma_scale(g) as f32)) as f32)
        };
        let first = (!gamma.at(1.0) || !gain.at(1.0) || !offset.at(0.0)).then(|| {
            let mut power = gamma.rgb.map(gamma_power);
            if (power[1] != 1.0 || power[2] != 1.0) && power[0] == 1.0 {
                power[0] = f64::from((power[0] - 1e-6) as f32);
            }
            if power[0] == 4.0 {
                power[0] = f64::from((power[0] + 1e-6) as f32);
            }
            Cdl {
                slope: gain.rgb,
                offset: offset.rgb.map(|o| o + offset.y),
                power,
            }
        });
        let second = (!gamma.at(1.0) || !gain.at(1.0)).then(|| Cdl {
            slope: [gain.y; 3],
            offset: [0.0; 3],
            power: [gamma_power(gamma.y); 3],
        });
        let exposure_ln = (f64::from(p.exposure_ln as f32).abs() > 0.0005).then(|| {
            let f = f64::from((f64::from(p.exposure_ln as f32) + 1.0) as f32);
            let f = if f <= 1.0 {
                f64::from(((f + 4.0) * 0.2) as f32)
            } else {
                f64::from(((f - 1.0) * 1.5 + 1.0) as f32)
            };
            let table = (0..ENTRIES)
                .rev()
                .map(|i| {
                    let x = f64::from((i as f64 / ENTRIES as f64) as f32);
                    (1.0 - f64::from(x.powf(f) as f32)) as f32
                })
                .collect();
            let pre = if f < 0.45 {
                1.0 - ((0.45 - f) * 0.36).min(0.36)
            } else {
                1.0
            };
            ExposureCurve { table, pre }
        });
        Grade {
            camera: camera.filter(|l| l.gain > 0.001),
            hsl_curves,
            balance,
            levels,
            rl_rgb: any_rgb.then(|| [rl_table(0), rl_table(1), rl_table(2)]),
            rl_y: any_y.then(|| rl_table(3)),
            curves_rgb: (touched(0) || touched(1) || touched(2))
                .then(|| std::array::from_fn(|c| table(&curves[c], ENTRIES, 0.0))),
            curve_y: touched(3).then(|| table(&curves[3], ENTRIES, 0.0)),
            lift: (!lift.at(0.0)).then(|| lift.rgb.map(|c| lift.y + c)),
            cdl: [first, second],
            saturation: (p.saturation != 1.0).then(|| f64::from(p.saturation as f32)),
            exposure: (p.exposure != 0.0)
                .then(|| f64::from(2f64.powf(f64::from(p.exposure as f32)) as f32)),
            brightness: (p.brightness.abs() > 0.0).then_some(p.brightness),
            contrast: (p.contrast.abs() > 1e-6).then(|| {
                let k = if p.contrast > 0.0 {
                    1.0 / (1.000001 - p.contrast) - 1.0
                } else {
                    p.contrast
                };
                (k, p.contrast_center)
            }),
            exposure_ln,
            hsl: [
                f64::from(p.hsl[0] as f32),
                f64::from(p.hsl[1] as f32),
                f64::from(p.hsl[2] as f32),
            ],
            look: look.filter(|l| l.gain > 0.001),
            quantize: false,
        }
    }

    /// Whether it changes nothing (a color within 0–1).
    pub fn changes_nothing(&self) -> bool {
        self.camera.is_none()
            && self.hsl_curves.is_empty()
            && self.balance.is_none()
            && self.levels.is_none()
            && self.rl_rgb.is_none()
            && self.rl_y.is_none()
            && self.curves_rgb.is_none()
            && self.curve_y.is_none()
            && self.lift.is_none()
            && self.cdl == [None, None]
            && self.saturation.is_none()
            && self.exposure.is_none()
            && self.brightness.is_none()
            && self.contrast.is_none()
            && self.exposure_ln.is_none()
            && self.hsl == [0.0, 1.0, 1.0]
            && self.look.is_none()
    }

    /// A step's end: the color held within 0–1 (and, when measuring, made
    /// 8-bit as VEGAS's steps are: cut to 12 bits, then rounded).
    fn hold(&self, c: &mut [f64; 3]) {
        for v in c.iter_mut() {
            *v = v.clamp(0.0, 1.0);
            if self.quantize {
                *v = ((*v * 4096.0).floor() / 4096.0 * 255.0).round() / 255.0;
            }
        }
    }

    fn lut(&self, step: &Option<LutStep>, c: &mut [f64; 3]) {
        if let Some(l) = step {
            let out = match &l.table {
                LutTable::Cube { lut, tetrahedral } => {
                    let how = if *tetrahedral {
                        Interpolation::Tetrahedral
                    } else {
                        Interpolation::Linear
                    };
                    lut.apply(c.map(|v| v as f32), how)
                }
                LutTable::AppleLog { video } => {
                    crate::color_grading::apple_log(c.map(|v| v as f32), *video)
                }
            };
            for (v, o) in c.iter_mut().zip(out) {
                *v += (f64::from(o) - *v) * l.gain;
            }
            self.hold(c);
        }
    }

    /// One HSL Curve on a color: its key from the color as it is.
    fn hsl_curve(kind: HslCurve, t: &[f32], [r, g, b]: [f64; 3]) -> [f64; 3] {
        let hsl = to_hsl(r, g, b);
        match kind {
            HslCurve::HueHue => {
                let [h, s, v] = to_hsv(r, g, b);
                let mut h = h - (lookup(t, h) - 0.5);
                if h < 0.0 {
                    h += 1.0;
                }
                if h > 1.0 {
                    h -= 1.0;
                }
                from_hsv(h, s, v)
            }
            HslCurve::HueSat | HslCurve::SatSat | HslCurve::LumSat => {
                let key = match kind {
                    HslCurve::HueSat => hsl[0],
                    HslCurve::SatSat => hsl[1],
                    _ => to_lab(r, g, b)[0] / WHITE_L,
                };
                let z = lookup(t, key);
                from_hsl(hsl[0], (hsl[1] * 2.0 * z).clamp(0.0, 1.0), hsl[2])
            }
            HslCurve::HueLum | HslCurve::SatLum => {
                let lab = to_lab(r, g, b);
                let scale = if kind == HslCurve::HueLum {
                    let w = lookup(t, hsl[0]);
                    let mut weight = 0.0;
                    if lab[0] > 30.0 {
                        weight = if lab[0] > 60.0 {
                            (-1.0 / 35.6) * lab[0] + 2.68
                        } else {
                            lab[0] / 30.0 - 1.0
                        };
                    }
                    let chroma = if hsl[2] <= 0.5 {
                        hsl[1].min(2.0 * hsl[2])
                    } else {
                        hsl[1].min(2.0 - 2.0 * hsl[2])
                    } - 1.0;
                    weight *= 1.75 / (1.0 + 18.0 * chroma * chroma);
                    1.0 - (1.0 - (2.25 * w - 0.125)) * weight
                } else {
                    2.0 * lookup(t, hsl[1])
                };
                from_lab((lab[0] * scale).clamp(0.0, WHITE_L), lab[1], lab[2])
            }
        }
    }

    /// A color (values, 0–1 in) through the grade.
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let mut c = rgb.map(f64::from);
        self.lut(&self.camera, &mut c);
        for (kind, t) in &self.hsl_curves {
            c = Grade::hsl_curve(*kind, t, c);
            self.hold(&mut c);
        }
        // The first part.
        if let Some(m) = &self.balance {
            let lin = c.map(|v| v.max(0.0).powf(2.4));
            c = std::array::from_fn(|i| {
                (m[i][0] * lin[0] + m[i][1] * lin[1] + m[i][2] * lin[2])
                    .max(0.0)
                    .powf(1.0 / 2.4)
            });
        }
        if let Some([a, b, lo, hi]) = self.levels {
            let k = (hi - lo) / (b - a);
            c = c.map(|v| lo + (v - a) * k);
        }
        if let Some(t) = &self.rl_rgb {
            c = std::array::from_fn(|i| lookup(&t[i], c[i]));
        }
        if let Some(t) = &self.rl_y {
            c = c.map(|v| lookup(t, v));
        }
        if let Some(t) = &self.curves_rgb {
            c = std::array::from_fn(|i| lookup(&t[i], c[i]));
        }
        if let Some(t) = &self.curve_y {
            c = c.map(|v| lookup(t, v));
        }
        self.hold(&mut c);
        // The second.
        if let Some(l) = self.lift {
            c = std::array::from_fn(|i| l[i] + c[i] * (1.0 - l[i]));
        }
        for cdl in self.cdl.iter().flatten() {
            c = std::array::from_fn(|i| cdl.apply(c[i], i));
        }
        self.hold(&mut c);
        // The third, and the HSL after it.
        if let Some(s) = self.saturation {
            let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            c = c.map(|v| y + s * (v - y));
        }
        if let Some(k) = self.exposure {
            c = c.map(|v| v * k);
        }
        if let Some(b) = self.brightness {
            c = c.map(|v| v + b);
        }
        if let Some((k, center)) = self.contrast {
            c = c.map(|v| v + k * (v - center));
        }
        if let Some(e) = &self.exposure_ln {
            c = c.map(|v| lookup(&e.table, v * e.pre));
        }
        let [h, s, l] = to_hsl(c[0], c[1], c[2]);
        c = from_hsl(
            h + self.hsl[0],
            (s * self.hsl[1]).clamp(0.0, 1.0),
            (l * self.hsl[2]).clamp(0.0, 1.0),
        );
        self.hold(&mut c);
        self.lut(&self.look, &mut c);
        c.map(|v| v as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grade(set: impl FnOnce(&mut Params)) -> Grade {
        let mut p = Params::default();
        set(&mut p);
        Grade::new(&p, None, None)
    }

    /// 8-bit values through `g`.
    fn bytes(g: &Grade, c: [u8; 3]) -> [u8; 3] {
        g.apply(c.map(|v| f32::from(v) / 255.0))
            .map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)
    }

    fn near(a: [u8; 3], b: [u8; 3], within: u8) -> bool {
        a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= within)
    }

    #[test]
    fn untouched_it_changes_nothing() {
        let g = grade(|_| {});
        assert!(g.changes_nothing());
        for c in [[0u8, 0, 0], [255, 255, 255], [17, 200, 99], [128, 128, 128]] {
            assert_eq!(bytes(&g, c), c);
        }
    }

    #[test]
    fn curves_read_and_write_their_text() {
        let k = parse_curve(DEFAULT_CURVE);
        assert_eq!(k.len(), 2);
        assert_eq!(k[1], Knot::new(1.0, 1.0, -0.2, -0.2, 0.2, 0.0));
        let text = curve_text(&k);
        assert!(text.starts_with("2:0.00000:0.00000:-0.20000:0.00000:0.20000:0.20000:"));
        assert_eq!(parse_curve(&text), k);
        assert!(untouched(DEFAULT_CURVE) && !untouched(&text));
        // The untouched curve's table is about the straight line: within
        // half an 8-bit step (its Newton step's parameter is not exact).
        let t = table(&k, ENTRIES, 0.0);
        assert!(t.len() >= ENTRIES - 1);
        for (i, v) in t.iter().enumerate() {
            assert!(
                (f64::from(*v) - i as f64 / 510.0).abs() < 0.5 / 255.0,
                "{i}: {v}"
            );
        }
    }

    #[test]
    fn the_gamma_wheel_scales_as_vegas() {
        let close = |a: f64, b: f64, e: f64| (a - b).abs() < e;
        assert!(close(gamma_scale(0.0), 0.360, 0.001));
        assert!(close(gamma_scale(1.0), 1.0, 1e-9));
        assert!(close(gamma_scale(2.0), 3.52, 0.01));
        assert!(close(gamma_scale(5.0), 933.0, 1.0));
    }

    #[test]
    fn white_balance_at_d65_is_neutral_and_warms_or_cools() {
        let (x, y) = chromaticity(6502.082197, 9.766656463);
        assert!(
            (x - 0.31273).abs() < 1e-4 && (y - 0.32902).abs() < 1e-4,
            "{x} {y}"
        );
        let m = balance_matrix(0.0, 0.0);
        for (r, row) in m.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                let want = if r == c { 1.0 } else { 0.0 };
                assert!((v - want).abs() < 1e-4, "{m:?}");
            }
        }
        let m = balance_matrix(2000.0, 0.0);
        let diag = [m[0][0], m[1][1], m[2][2]];
        assert!(
            (diag[0] - 0.985).abs() < 0.01
                && (diag[1] - 1.001).abs() < 0.01
                && (diag[2] - 1.243).abs() < 0.01,
            "{diag:?}"
        );
    }

    #[test]
    fn rl_shadows_bend_the_highlights_as_vegas() {
        // Only Shadows Y at 0.5: VEGAS's grey ramp steps in the lights.
        let g = grade(|p| p.rl[0].y = 0.5);
        for (input, out) in [
            (139, 140),
            (140, 145),
            (191, 193),
            (192, 201),
            (227, 228),
            (228, 236),
        ] {
            let got = bytes(&g, [input; 3]);
            assert!(near(got, [out; 3], 1), "{input}: {got:?} not {out}");
        }
    }

    #[test]
    fn linear_exposure_takes_colors_past_white_to_white() {
        let g = grade(|p| p.exposure = 1.0);
        for (input, out) in [
            ([255, 0, 0], [255, 255, 255]),
            ([128, 0, 0], [255, 1, 1]),
            ([191, 96, 48], [255, 234, 223]),
        ] {
            let got = bytes(&g, input);
            assert!(near(got, out, 1), "{input:?}: {got:?} not {out:?}");
        }
    }

    #[test]
    fn hue_turns_and_lightness_curves_hold_white_below_its_l() {
        let g = grade(|p| p.hsl[0] = 1.0 / 3.0);
        assert!(near(bytes(&g, [255, 0, 0]), [0, 255, 0], 1));
        // A lightness curve on: white comes out 242, where the curve is
        // neutral.
        let flat = curve_text(&[
            Knot::new(0.0, 0.5, 0.0, 0.0, 0.25, 0.0),
            Knot::new(1.0, 0.5, -0.25, 0.0, 0.0, 0.0),
        ]);
        let g = grade(|p| p.curves[8] = flat.clone());
        assert!(
            near(bytes(&g, [255; 3]), [242; 3], 1),
            "{:?}",
            bytes(&g, [255; 3])
        );
    }

    #[test]
    fn levels_and_the_panels_exposure() {
        let g = grade(|p| p.levels = [0.1, 0.9, 0.05, 0.95]);
        // 0.5 stays (the middle maps to the middle), the input's ends to the
        // output's.
        assert!(near(bytes(&g, [128; 3]), [128; 3], 1));
        assert!(near(bytes(&g, [26; 3]), [13; 3], 1));
        // Exposure: the dark lifted more than the light, black and white
        // kept.
        let g = grade(|p| p.exposure_ln = 1.0);
        assert!(near(bytes(&g, [255; 3]), [255; 3], 0));
        let mid = bytes(&g, [64; 3])[0];
        assert!(mid > 90, "{mid}");
    }
}
