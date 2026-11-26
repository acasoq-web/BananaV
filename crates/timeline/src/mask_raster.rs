//! A Pan/Crop mask's picture, the same as VEGAS's rendered frames pixel by
//! pixel where no feather reaches (`pan_crop_renders` checks it): its
//! paths' curves cut into pieces going one way down, those followed line by
//! line (sixteen bands a line with anti-aliasing) and filled from one
//! crossing to the next, in 8-bit steps; a feather is a recursive Gaussian
//! over the filled path, a one-way one bent by a curve, kept at full
//! precision so it fades without bands (BananaV's own: VEGAS's comes out in
//! whole steps); the paths then add up or take away, each as far as its
//! opacity says.

use crate::mask::{MaskAnchor, MaskFeather, MaskMode, MaskPath};

/// A cubic of the path, a piece going one way down (or a straight line).
#[derive(Clone, Copy, Debug)]
struct Piece {
    p0: [f64; 2],
    p3: [f64; 2],
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
    /// Drawn upward along the path: put the other way round here.
    reversed: bool,
}

/// Rounds to the nearest whole number, ties to the even one.
fn nearest(x: f64) -> f64 {
    x.round_ties_even()
}

fn clamp8(v: f64) -> f64 {
    v.clamp(0.0, 255.0)
}

/// The rounding of a value about to be a pixel's.
fn to_level(v: f64) -> u8 {
    nearest(clamp8(v) + 1.5e-8) as u8
}

const NEAR_HALF: f64 = 0.499_999_985;

fn lerp2(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [(b[0] - a[0]) * t + a[0], (b[1] - a[1]) * t + a[1]]
}

fn straight(p: &[[f64; 2]; 4]) -> bool {
    (p[0][0] - p[1][0]).abs() < 1e-6
        && (p[0][1] - p[1][1]).abs() < 1e-6
        && (p[2][0] - p[3][0]).abs() < 1e-6
        && (p[2][1] - p[3][1]).abs() < 1e-6
}

/// A cubic's power form: `a t³ + b t² + c t + p0`.
fn cubic(p: &[[f64; 2]; 4]) -> ([f64; 2], [f64; 2], [f64; 2]) {
    let c = [(p[1][0] - p[0][0]) * 3.0, (p[1][1] - p[0][1]) * 3.0];
    let b = [
        (p[2][0] - p[1][0]) * 3.0 - c[0],
        (p[2][1] - p[1][1]) * 3.0 - c[1],
    ];
    let a = [
        p[3][0] - c[0] - b[0] - p[0][0],
        p[3][1] - c[1] - b[1] - p[0][1],
    ];
    (a, b, c)
}

/// Where a cubic of power form `(a, b, c)` turns back, inside (0, 1), in order.
fn turning_points(a: f64, b: f64, c: f64) -> Vec<f64> {
    let inside = |t: f64| t > 0.0 && t < 1.0;
    let mut out = Vec::new();
    if a == 0.0 {
        if b != 0.0 && c != 0.0 {
            let t = c / (b * -2.0);
            if inside(t) {
                out.push(t);
            }
        }
    } else if c == 0.0 {
        if b != 0.0 {
            let t = (b * -2.0) / (a * 3.0);
            if inside(t) {
                out.push(t);
            }
        }
    } else {
        let d = b * b - a * 3.0 * c;
        if d > 0.0 {
            let m = if a >= 0.0 { -b } else { b };
            let q = d.sqrt();
            for t in [(m - q) / (a.abs() * 3.0), (m + q) / (a.abs() * 3.0)] {
                if inside(t) {
                    out.push(t);
                }
            }
        }
    }
    out
}

fn split(p: &[[f64; 2]; 4], t: f64) -> ([[f64; 2]; 4], [[f64; 2]; 4]) {
    let (ab, bc, cd) = (
        lerp2(p[0], p[1], t),
        lerp2(p[1], p[2], t),
        lerp2(p[2], p[3], t),
    );
    let (abc, bcd) = (lerp2(ab, bc, t), lerp2(bc, cd, t));
    let m = lerp2(abc, bcd, t);
    ([p[0], ab, abc, m], [m, bcd, cd, p[3]])
}

/// The pieces of a segment.
fn pieces_of(p: [[f64; 2]; 4], out: &mut Vec<Piece>) {
    let mut push = |q: [[f64; 2]; 4]| {
        let reversed = q[0][1] > q[3][1];
        let o = if reversed {
            [q[3], q[2], q[1], q[0]]
        } else {
            q
        };
        let (a, b, c) = if straight(&q) {
            ([0.0; 2], [0.0; 2], [o[3][0] - o[0][0], o[3][1] - o[0][1]])
        } else {
            cubic(&o)
        };
        out.push(Piece {
            p0: o[0],
            p3: o[3],
            a,
            b,
            c,
            reversed,
        });
    };
    if straight(&p) {
        push(p);
        return;
    }
    let (a, b, c) = cubic(&p);
    let turns = turning_points(a[1], b[1], c[1]);
    match turns.as_slice() {
        [t] => {
            let (l, r) = split(&p, *t);
            push(l);
            push(r);
        }
        [t0, t1] => {
            let (first, rest) = split(&p, *t0);
            let (second, third) = split(&rest, (t1 - t0) / (1.0 - t0));
            push(first);
            push(second);
            push(third);
        }
        _ => push(p),
    }
}

/// Pieces in a row going the same way down.
#[derive(Clone, Debug)]
struct Group {
    pieces: Vec<Piece>,
    top: f64,
    base: f64,
}

/// A path's groups, in the picture's pixels, by where they start down.
fn groups_of(anchors: &[MaskAnchor], scale: [f64; 2]) -> Vec<Group> {
    let n = anchors.len();
    let at = |p: [f64; 2]| [p[0] * scale[0], p[1] * scale[1]];
    let mut pieces = Vec::with_capacity(n * 3);
    for i in 0..n {
        let (a, b) = (&anchors[i], &anchors[(i + 1) % n]);
        pieces_of(
            [
                at(a.at),
                at([a.at[0] + a.after[0], a.at[1] + a.after[1]]),
                at([b.at[0] + b.before[0], b.at[1] + b.before[1]]),
                at(b.at),
            ],
            &mut pieces,
        );
    }
    // From where one way begins.
    for _ in 0..pieces.len() {
        if pieces[0].reversed == pieces[pieces.len() - 1].reversed {
            pieces.rotate_left(1);
        } else {
            break;
        }
    }
    let mut groups: Vec<Group> = pieces
        .chunk_by(|a, b| a.reversed == b.reversed)
        .map(|run| {
            let mut g = run.to_vec();
            if g[0].reversed {
                g.reverse();
            }
            Group {
                top: g[0].p0[1],
                base: g[g.len() - 1].p3[1],
                pieces: g,
            }
        })
        .collect();
    groups.sort_by(|a, b| a.top.total_cmp(&b.top));
    groups
}

/// Where a piece that goes one way down is at `y`.
fn along(s: &Piece, y: f64) -> f64 {
    let t = if s.a[1] == 0.0 && s.b[1] == 0.0 {
        if s.c[1] == 0.0 {
            0.0
        } else {
            (y - s.p0[1]) / s.c[1]
        }
    } else {
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        for _ in 0..60 {
            let m = (lo + hi) * 0.5;
            if ((s.a[1] * m + s.b[1]) * m + s.c[1]) * m + s.p0[1] < y {
                lo = m;
            } else {
                hi = m;
            }
        }
        (lo + hi) * 0.5
    };
    ((s.a[0] * t + s.b[0]) * t + s.c[0]) * t + s.p0[0]
}

/// Where a group crosses the band `[y0, y1)`, measured at its middle; `None`
/// when it does not.
fn crossing(g: &Group, y0: f64, y1: f64) -> Option<f64> {
    let y = (y0 + y1) * 0.5;
    if g.top > y {
        return (g.top < y1).then(|| g.pieces[0].p0[0]);
    }
    if y > g.base {
        return (y0 < g.base).then(|| g.pieces[g.pieces.len() - 1].p3[0]);
    }
    let mut i = 0;
    while i + 1 < g.pieces.len() && g.pieces[i].p3[1] < y {
        i += 1;
    }
    let s = &g.pieces[i];
    if y - s.p0[1] < 0.001 {
        return Some(s.p0[0]);
    }
    if s.p3[1] - y < 0.001 {
        return Some(s.p3[0]);
    }
    (s.p0[1] < y && y < s.p3[1]).then(|| along(s, y))
}

fn whole(v: f64, w: usize) -> usize {
    v.clamp(0.0, w as f64) as usize
}

/// A line without anti-aliasing: each pair of crossings filled.
fn plain_line(row: &mut [u8], xs: &[f64]) {
    let w = row.len();
    for pair in xs.as_chunks::<2>().0 {
        let a = nearest(pair[0] + 1.5e-8);
        if w as f64 <= a {
            return;
        }
        let from = whole(a, w);
        let to = whole(nearest(pair[1] + 1.5e-8), w);
        if from < to {
            row[from..to].fill(255);
        }
    }
}

/// A group's crossings of a line's sixteen bands.
#[derive(Clone, Copy)]
struct Bands {
    x: [f64; 16],
    seen: [bool; 16],
    min: f64,
    max: f64,
    count: u32,
}

/// A line whose groups do not overlap across and all cross its sixteen bands:
/// the coverage summed before it is rounded, the middle whole.
fn fast_line(row: &mut [u8], groups: &[Bands]) {
    let w = row.len();
    let dv = 255.0 / 16.0;
    for pair in groups.as_chunks::<2>().0 {
        let (l, r) = (&pair[0], &pair[1]);
        let a = nearest(l.min - NEAR_HALF);
        if w as f64 <= a {
            return;
        }
        let from = whole(a, w);
        let end = whole(nearest(r.max + NEAR_HALF), w);
        let lead = whole(nearest(l.max + NEAR_HALF).max(from as f64), end);
        let middle = whole(nearest(r.min - NEAR_HALF).max(lead as f64), end);
        for (p, v) in row.iter_mut().enumerate().take(lead).skip(from) {
            let (q, q1) = (p as f64, p as f64 + 1.0);
            let mut sum = 0.0;
            for k in 0..16 {
                if l.x[k] < q1 {
                    sum += r.x[k].min(q1) - if l.x[k] <= q { q } else { l.x[k] };
                }
            }
            *v = to_level(sum * dv);
        }
        if lead < middle {
            row[lead..middle].fill(255);
        }
        for (p, v) in row.iter_mut().enumerate().take(end).skip(middle) {
            let (q, q1) = (p as f64, p as f64 + 1.0);
            let mut sum = 0.0;
            for k in 0..16 {
                if q < r.x[k] {
                    sum += (if q1 <= r.x[k] { q1 } else { r.x[k] }) - q.max(l.x[k]);
                }
            }
            *v = to_level(sum * dv);
        }
    }
}

/// One of a line's sixteen bands added to it.
fn add_band(row: &mut [u8], xs: &[f64]) {
    let w = row.len();
    let dv = 255.0 / 16.0;
    for pair in xs.as_chunks::<2>().0 {
        let (x0, x1) = (pair[0], pair[1]);
        let a = nearest(x0 - NEAR_HALF);
        if w as f64 <= a {
            break;
        }
        let from = whole(a, w);
        let lead = whole(nearest(x0 + NEAR_HALF).max(from as f64), w);
        let end = whole(nearest(x1 + NEAR_HALF), w);
        let middle = whole(nearest(x1 - NEAR_HALF), end);
        let mut p = from;
        while p < lead {
            let c = (x1.min(p as f64 + 1.0) - x0) * dv + f64::from(row[p]);
            row[p] = to_level(c);
            p += 1;
        }
        while p < middle {
            row[p] = row[p].saturating_add(16);
            p += 1;
        }
        while p < end {
            let c = (x1 - (p as f64).max(x0)) * dv + f64::from(row[p]);
            row[p] = to_level(c);
            p += 1;
        }
    }
}

/// One line of an anti-aliased path.
fn smooth_line(row: &mut [u8], groups: &[Group], l: usize) {
    let line = l as f64;
    let mut seen: Vec<Bands> = Vec::with_capacity(groups.len());
    for g in groups {
        if line > g.base || line + 1.0 < g.top {
            continue;
        }
        let mut b = Bands {
            x: [0.0; 16],
            seen: [false; 16],
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            count: 0,
        };
        for k in 0..16 {
            let y0 = line + k as f64 / 16.0;
            if let Some(x) = crossing(g, y0, y0 + 1.0 / 16.0) {
                b.x[k] = x;
                b.seen[k] = true;
                b.count += 1;
                b.min = b.min.min(x);
                b.max = b.max.max(x);
            }
        }
        if b.count > 0 {
            seen.push(b);
        }
    }
    if seen.len() < 2 {
        return;
    }
    seen.sort_by(|a, b| a.min.total_cmp(&b.min));
    let apart = seen.iter().all(|b| b.count == 16) && seen.windows(2).all(|p| p[1].min >= p[0].max);
    if apart {
        fast_line(row, &seen);
        return;
    }
    let mut xs = Vec::with_capacity(seen.len());
    for k in 0..16 {
        xs.clear();
        xs.extend(seen.iter().filter(|b| b.seen[k]).map(|b| b.x[k]));
        if xs.len() > 1 {
            xs.sort_by(f64::total_cmp);
            add_band(row, &xs);
        }
    }
}

/// A recursive Gaussian of the fourth order (Deriche's), forward and back.
struct Gaussian {
    n: [f64; 5],
    d: [f64; 5],
    m: [f64; 5],
    bp: [f64; 5],
    bm: [f64; 5],
}

impl Gaussian {
    /// For a blur reaching `r` pixels, where its tail is a level of 255.
    fn new(r: f64) -> Gaussian {
        let s = (-(r * r) / (2.0 * (1.0f64 / 255.0).ln())).sqrt().max(0.6);
        let k = s * 2.506_628_274_63;
        let (a0, a1) = (f64::from(1.6803f32) / k, f64::from(-0.6803f32) / k);
        let (c0, c1) = (f64::from(3.735f32) / k, f64::from(-0.2598f32) / k);
        let (b0, b1) = (f64::from(-1.783f32) / s, f64::from(-1.723f32) / s);
        let (w0, w1) = (f64::from(0.6318f32) / s, f64::from(1.997f32) / s);
        let (sin0, cos0) = w0.sin_cos();
        let (sin1, cos1) = w1.sin_cos();
        let (e0, e1) = (b0.exp(), b1.exp());
        let both = (b0 + b1).exp();
        let (e00, e11) = ((b0 + b0).exp(), (b1 + b1).exp());
        let f = sin1 * c1;
        let g = sin0 * c0;
        let n = [
            a1 + a0,
            (g - cos0 * (a1 + a1 + a0)) * e0 + (f - cos1 * (a0 + a0 + a1)) * e1,
            (cos1 * (a1 + a0) * cos0 - cos1 * c0 * sin0 - cos0 * c1 * sin1) * (both * 2.0)
                + e00 * a1
                + e11 * a0,
            (f - cos1 * a1) * (b0 + b0 + b1).exp() + (g - cos0 * a0) * (b1 + b1 + b0).exp(),
            0.0,
        ];
        let d = [
            0.0,
            (cos1 * e1 + cos0 * e0) * -2.0,
            cos1 * 4.0 * cos0 * both + e11 + e00,
            ((b0 + b0 + b1).exp() * cos1 + (b1 + b1 + b0).exp() * cos0) * -2.0,
            (b0 + b1 + b0 + b1).exp(),
        ];
        let m = [
            0.0,
            n[1] - n[0] * d[1],
            n[2] - n[0] * d[2],
            n[3] - n[0] * d[3],
            n[4] - n[0] * d[4],
        ];
        let sum: f64 = d.iter().sum::<f64>() + 1.0;
        let (kp, km) = if sum.abs() >= 1e-15 {
            (n.iter().sum::<f64>() / sum, m.iter().sum::<f64>() / sum)
        } else {
            (1.0, 1.0)
        };
        Gaussian {
            n,
            d,
            m,
            bp: d.map(|v| v * kp),
            bm: d.map(|v| v * km),
        }
    }

    /// One line through it: `x(i)` in, `out(i, value)` out; its edges held.
    fn run(
        &self,
        n: usize,
        x: impl Fn(usize) -> f64,
        y: &mut [f64],
        z: &mut [f64],
        mut out: impl FnMut(usize, f64),
    ) {
        if n == 0 {
            return;
        }
        let (first, last) = (x(0), x(n - 1));
        for j in 0..n {
            let (mut p, mut q) = (0.0, 0.0);
            let reach = j.min(5);
            for k in 0..reach {
                p += x(j - k) * self.n[k] - self.d[k] * y[j - k];
                q += x(n - 1 - j + k) * self.m[k] - self.d[k] * z[j - k];
            }
            for k in reach..5 {
                p += (self.n[k] - self.bp[k]) * first;
                q += (self.m[k] - self.bm[k]) * last;
            }
            y[j] = if p.abs() <= 1e-15 { 0.0 } else { p };
            z[j] = if q.abs() <= 1e-15 { 0.0 } else { q };
        }
        for j in 0..n {
            out(j, z[n - 1 - j] + y[j]);
        }
    }
}

/// How a one-way feather bends what its blur leaves, going out of the shape:
/// measured on VEGAS's renders; into it, the same turned round.
#[rustfmt::skip]
const ONE_WAY: [u8; 129] = [
    0, 1, 3, 6, 9, 12, 16, 20, 24, 29, 33, 38, 42, 47, 52, 56, 61, 66, 71, 75, 80, 84, 89, 93, 98,
    102, 106, 111, 115, 119, 123, 127, 130, 134, 138, 141, 145, 148, 151, 155, 158, 161, 164, 167,
    170, 172, 175, 178, 180, 183, 185, 188, 190, 192, 194, 196, 198, 201, 202, 204, 206, 208, 210,
    211, 213, 214, 216, 217, 219, 220, 222, 223, 224, 225, 227, 228, 229, 230, 231, 232, 233, 234,
    235, 236, 236, 237, 238, 239, 239, 240, 241, 242, 242, 243, 243, 244, 245, 245, 246, 246, 247,
    247, 247, 248, 248, 249, 249, 249, 250, 250, 250, 251, 251, 251, 252, 252, 252, 252, 253, 253,
    253, 253, 253, 254, 254, 254, 254, 254, 255,
];

/// Steps of the bend's samples in a level of the blur.
const FINE: usize = 32;

/// The one-way bend sampled every `FINE`th of a level over the blur's
/// levels the table covers: a smooth curve that keeps rising (a monotone
/// cubic) through the measured points where they rise — their repeats,
/// whole levels' flat steps, left out; never more than a level from them.
static BEND: std::sync::LazyLock<Vec<f32>> = std::sync::LazyLock::new(|| {
    let mut knots = vec![(0.0f64, f64::from(ONE_WAY[0]))];
    for (i, &v) in ONE_WAY.iter().enumerate().skip(1) {
        if f64::from(v) > knots[knots.len() - 1].1 {
            knots.push((i as f64, f64::from(v)));
        }
    }
    let n = knots.len();
    let secant: Vec<f64> = knots
        .windows(2)
        .map(|k| (k[1].1 - k[0].1) / (k[1].0 - k[0].0))
        .collect();
    // Each knot's slope: the secants' weighted harmonic mean (Fritsch and
    // Butland's), the secant itself at the ends.
    let mut slope = vec![0.0f64; n];
    slope[0] = secant[0];
    slope[n - 1] = secant[n - 2];
    for k in 1..n - 1 {
        let (h0, h1) = (knots[k].0 - knots[k - 1].0, knots[k + 1].0 - knots[k].0);
        let (w0, w1) = (h1 + h1 + h0, h1 + h0 + h0);
        slope[k] = (w0 + w1) / (w0 / secant[k - 1] + w1 / secant[k]);
    }
    let last = ONE_WAY.len() - 1;
    let mut k = 0;
    (0..=last * FINE)
        .map(|i| {
            let x = i as f64 / FINE as f64;
            while k + 2 < n && knots[k + 1].0 <= x {
                k += 1;
            }
            let ((x0, y0), (x1, y1)) = (knots[k], knots[k + 1]);
            let h = x1 - x0;
            let t = ((x - x0) / h).clamp(0.0, 1.0);
            let (t2, t3) = (t * t, t * t * t);
            let v = (t3 + t3 - t2 - t2 - t2 + 1.0) * y0
                + (t3 - t2 - t2 + t) * h * slope[k]
                + (t2 + t2 + t2 - t3 - t3) * y1
                + (t3 - t2) * h * slope[k + 1];
            v as f32
        })
        .collect()
});

/// The one-way bend of a blurred level `v` (0–255, any fraction), going out
/// of the shape, smooth all along: a feather's levels keep changing pixel
/// by pixel (the blur taken in whole levels through the table came out in
/// steps up to five levels apart, with flat bands between them).
fn bent_out(v: f64) -> f64 {
    let last = ONE_WAY.len() - 1;
    if v.is_nan() || v <= 0.0 {
        return 0.0;
    }
    if v >= last as f64 {
        return 255.0;
    }
    let x = v * FINE as f64;
    let i = x as usize;
    let (a, b) = (f64::from(BEND[i]), f64::from(BEND[i + 1]));
    (b - a) * (x - i as f64) + a
}

fn bent_in(v: f64) -> f64 {
    255.0 - bent_out(255.0 - v)
}

/// A filled path feathered: how much of each pixel it keeps (0–255), at
/// full precision — never cut down to whole levels on the way, so a wide
/// feather fades without bands. Both passes run on every core: the one
/// down goes along the picture's columns, each a line of its own.
fn feather(buf: &[u8], w: usize, h: usize, style: MaskFeather, radius: [f64; 2]) -> Vec<f32> {
    let (mut rx, mut ry) = (radius[0].abs(), radius[1].abs());
    let bend: Option<fn(f64) -> f64> = match style {
        MaskFeather::In => Some(bent_in),
        MaskFeather::Out => Some(bent_out),
        _ => None,
    };
    if bend.is_some() {
        rx += rx;
        ry += ry;
    }
    let across = Gaussian::new(rx);
    let mut blurred = vec![0.0f32; w * h];
    bv_color::par_rows(&mut blurred, w, |l, row| {
        let (mut y, mut z) = (vec![0.0; w], vec![0.0; w]);
        let line = &buf[l * w..(l + 1) * w];
        across.run(
            w,
            |i| f64::from(line[i]),
            &mut y,
            &mut z,
            |j, v| row[j] = v as f32,
        );
    });
    // Down: column `c` of the picture is line `c` here.
    let down = Gaussian::new(ry);
    let mut columns = vec![0.0f32; w * h];
    bv_color::par_rows(&mut columns, h, |c, column| {
        let (mut y, mut z) = (vec![0.0; h], vec![0.0; h]);
        down.run(
            h,
            |i| f64::from(blurred[i * w + c]),
            &mut y,
            &mut z,
            |j, v| column[j] = v as f32,
        );
    });
    // Back to lines, bent: `blurred` is not needed any more.
    bv_color::par_rows(&mut blurred, w, |l, row| {
        for (c, v) in row.iter_mut().enumerate() {
            let x = clamp8(f64::from(columns[c * h + l]));
            *v = bend.map_or(x, |b| b(x)) as f32;
        }
    });
    blurred
}

/// What a path keeps of each pixel, 0–255: its fill as it is (whole levels,
/// the edges' bands summed) or feathered (any fraction).
enum Cover<'a> {
    Filled(&'a [u8]),
    Feathered(Vec<f32>),
}

impl Cover<'_> {
    /// Each pixel's value through `f` (with whether it is feathered) into
    /// `out`.
    fn each(&self, out: &mut [f32], f: impl Fn(f32, f32, bool) -> f32) {
        match self {
            Cover::Filled(b) => {
                for (o, &t) in out.iter_mut().zip(b.iter()) {
                    *o = f(*o, f32::from(t), false);
                }
            }
            Cover::Feathered(v) => {
                for (o, &t) in out.iter_mut().zip(v) {
                    *o = f(*o, t, true);
                }
            }
        }
    }
}

/// One path filled into `buf` (zero before), then feathered when it is.
fn fill_path<'a>(
    buf: &'a mut [u8],
    w: usize,
    h: usize,
    path: &MaskPath,
    scale: [f64; 2],
    radius: [f64; 2],
) -> Cover<'a> {
    let groups = groups_of(&path.anchors, scale);
    let smooth = path.anti_alias;
    bv_color::par_rows(buf, w, |l, row| {
        if smooth {
            smooth_line(row, &groups, l);
        } else {
            let line = l as f64;
            let mut xs: Vec<f64> = groups
                .iter()
                .filter(|g| line <= g.base && line + 1.0 >= g.top)
                .filter_map(|g| crossing(g, line, line + 1.0))
                .collect();
            if xs.len() > 1 {
                xs.sort_by(f64::total_cmp);
                plain_line(row, &xs);
            }
        }
    });
    if path.feather != MaskFeather::None && path.feather_pct > 0.0 {
        return Cover::Feathered(feather(buf, w, h, path.feather, radius));
    }
    Cover::Filled(buf)
}

/// `paths`, in order, on a picture `w` × `h` of the media, whose pixels are
/// its own times `scale` and `aspect` wide: how much of each pixel they keep,
/// 0–255 — in whole levels where no feather reaches, any fraction where one
/// does.
pub(crate) fn alpha(
    paths: &[MaskPath],
    w: usize,
    h: usize,
    scale: [f64; 2],
    aspect: f64,
) -> Vec<f32> {
    let mut out = vec![0.0f32; w * h];
    let mut first = true;
    let mut scratch = Vec::new();
    let shorter = w.min(h) as f64;
    // A sum of a path's and what was there: whole levels for a fill,
    // as it comes for a feather.
    let level = |v: f64, soft: bool| {
        if soft {
            clamp8(v) as f32
        } else {
            f32::from(to_level(v))
        }
    };
    for path in paths {
        if !path.closed || path.anchors.len() < 2 || path.mode == MaskMode::Disabled {
            continue;
        }
        let negative = path.mode == MaskMode::Negative;
        let opacity = path.opacity;
        if !(opacity > 0.0 || (first && negative)) {
            continue;
        }
        let apart = !first || negative;
        if first && negative {
            out.fill(255.0);
        }
        if opacity > 0.0 {
            let r = shorter * path.feather_pct / 100.0;
            let radius = if aspect > 1.0 {
                [r / aspect, r]
            } else {
                [r, r * aspect]
            };
            scratch.clear();
            scratch.resize(w * h, 0u8);
            let cover = fill_path(&mut scratch, w, h, path, scale, radius);
            if apart {
                let signed = if negative && opacity < 1.0 {
                    -opacity
                } else {
                    opacity
                };
                cover.each(&mut out, |o, t, soft| {
                    if opacity >= 1.0 {
                        if negative {
                            (o - t).max(0.0)
                        } else {
                            (o + t).min(255.0)
                        }
                    } else {
                        level(f64::from(t) * signed + f64::from(o), soft)
                    }
                });
            } else if opacity < 1.0 {
                cover.each(&mut out, |_, t, soft| level(f64::from(t) * opacity, soft));
            } else {
                cover.each(&mut out, |_, t, _| t);
            }
        }
        first = false;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_one_way_curves_are_each_others_turned_round() {
        assert_eq!(bent_out(0.0), 0.0);
        assert_eq!(bent_out(64.0), 213.0);
        assert_eq!(bent_out(128.0), 255.0);
        assert_eq!(bent_out(255.0), 255.0);
        assert_eq!(bent_in(255.0), 255.0);
        assert_eq!(bent_in(254.0), 254.0);
        assert_eq!(bent_in(100.0), 0.0);
        // Between the measured points and never a level away from them,
        // rising all along.
        assert!(bent_out(2.5) > 3.0 && bent_out(2.5) < 6.0);
        assert_eq!(bent_in(252.5), 255.0 - bent_out(2.5));
        let mut before = -1.0;
        for i in 0..=128 * FINE {
            let x = i as f64 / FINE as f64;
            let v = bent_out(x);
            assert!(v > before || v == 255.0, "rising at {x}");
            if i % FINE == 0 {
                let measured = f64::from(ONE_WAY[i / FINE]);
                assert!((v - measured).abs() < 1.0, "{x}: {v} for {measured}");
            }
            before = v;
        }
    }

    /// A wide feather fades pixel by pixel: no flat bands with steps of
    /// several levels between them (its blur taken in whole levels made
    /// bands up to eighteen pixels wide and steps of up to five levels).
    #[test]
    fn a_wide_feather_fades_without_bands() {
        for style in [MaskFeather::Out, MaskFeather::In, MaskFeather::Both] {
            let mut p = MaskPath::rectangle([400.0, 120.0], [880.0, 600.0]);
            p.feather = style;
            p.feather_pct = 20.0;
            let c = alpha(&[p], 1280, 720, [1.0, 1.0], 1.0);
            // The left edge's fade, from outside in.
            let row = &c[360 * 1280..360 * 1280 + 640];
            let mut worst = 0.0f32;
            for p in row.windows(2) {
                let step = p[1] - p[0];
                // One way (the recursive Gaussian wavers by a hair).
                assert!(step > -0.01, "{style:?}: {step} at {}", p[0]);
                if p[0] > 0.5 && p[1] < 254.5 {
                    assert!(step > 0.0, "{style:?}: a flat band at {}", p[0]);
                }
                worst = worst.max(step);
            }
            assert!(worst < 3.0, "{style:?}: a step of {worst}");
        }
    }

    #[test]
    fn a_rectangle_fills_as_far_as_it_covers() {
        let mut p = MaskPath::rectangle([10.25, 10.0], [30.0, 20.0]);
        let c = alpha(std::slice::from_ref(&p), 40, 30, [1.0, 1.0], 1.0);
        let at = |c: &[f32], x: usize, y: usize| c[y * 40 + x];
        assert_eq!(at(&c, 20, 15), 255.0);
        assert_eq!(at(&c, 2, 2), 0.0);
        // 0.75 of a pixel: 191.25.
        assert_eq!(at(&c, 10, 15), 191.0);
        assert_eq!(at(&c, 9, 15), 0.0);
        // Without anti-aliasing, whole or nothing.
        p.anti_alias = false;
        let c = alpha(&[p], 40, 30, [1.0, 1.0], 1.0);
        assert_eq!(at(&c, 10, 15), 255.0);
        assert_eq!(at(&c, 9, 15), 0.0);
    }

    #[test]
    fn a_gaussian_keeps_a_flat_line_flat() {
        let g = Gaussian::new(10.0);
        let (mut y, mut z) = (vec![0.0; 64], vec![0.0; 64]);
        let mut out = vec![0.0; 64];
        g.run(64, |_| 200.0, &mut y, &mut z, |j, v| out[j] = v);
        for v in out {
            assert!((v - 200.0).abs() < 0.1, "{v}");
        }
    }

    #[test]
    fn a_negative_path_cuts_what_is_inside_it_alone() {
        let mut p = MaskPath::rectangle([10.0, 10.0], [30.0, 20.0]);
        p.mode = MaskMode::Negative;
        let c = alpha(&[p], 40, 30, [1.0, 1.0], 1.0);
        assert_eq!(c[15 * 40 + 20], 0.0);
        assert_eq!(c[2 * 40 + 2], 255.0);
    }

    #[test]
    fn paths_add_up_and_take_away() {
        let a = MaskPath::rectangle([0.0, 0.0], [20.0, 30.0]);
        let mut b = MaskPath::rectangle([5.0, 5.0], [15.0, 25.0]);
        b.mode = MaskMode::Negative;
        let c = alpha(&[a.clone(), b], 40, 30, [1.0, 1.0], 1.0);
        assert_eq!(c[15 * 40 + 10], 0.0, "the hole");
        assert_eq!(c[15 * 40 + 2], 255.0, "around the hole");
        let d = MaskPath::rectangle([20.0, 0.0], [40.0, 30.0]);
        let c = alpha(&[a, d], 40, 30, [1.0, 1.0], 1.0);
        assert_eq!(c[15 * 40 + 30], 255.0, "the second one added");
    }

    #[test]
    fn half_an_opacity_is_half_a_level() {
        let mut p = MaskPath::rectangle([0.0, 0.0], [40.0, 30.0]);
        p.opacity = 0.5;
        let c = alpha(&[p], 40, 30, [1.0, 1.0], 1.0);
        // 127.5, its tie to the even number with what the rounding adds.
        assert_eq!(c[0], 128.0);
    }

    #[test]
    fn a_curve_is_cut_where_it_turns_back() {
        // An oval's two sides: two groups a line.
        let o = MaskPath::oval([0.0, 0.0], [40.0, 30.0]);
        let g = groups_of(&o.anchors, [1.0, 1.0]);
        assert_eq!(g.len(), 2);
        assert_eq!((g[0].top, g[0].base), (0.0, 30.0));
        // Crossing it at its middle line: its sides.
        let xs: Vec<f64> = g.iter().filter_map(|g| crossing(g, 15.0, 15.0)).collect();
        assert_eq!(xs.len(), 2);
        assert!(xs.iter().any(|x| x.abs() < 1e-6) && xs.iter().any(|x| (x - 40.0).abs() < 1e-6));
    }
}
