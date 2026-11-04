//! A Pan/Crop mask's 8-bit picture, the same as VEGAS's rendered frames pixel by
//! pixel (`pan_crop_renders` checks it): its paths' curves cut
//! into pieces going one way down, those followed line by line (sixteen
//! bands a line with anti-aliasing) and filled from one crossing to the next;
//! a feather is a recursive Gaussian over the filled path, a one-way one
//! bent by a curve; the paths then add up or take away, each as far as its
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

fn bent_out(v: u8) -> u8 {
    ONE_WAY.get(usize::from(v)).copied().unwrap_or(255)
}

fn bent_in(v: u8) -> u8 {
    255 - bent_out(255 - v)
}

/// A filled path feathered in place.
fn feather(buf: &mut [u8], w: usize, h: usize, style: MaskFeather, radius: [f64; 2]) {
    let (mut rx, mut ry) = (radius[0].abs(), radius[1].abs());
    let bend: Option<fn(u8) -> u8> = match style {
        MaskFeather::In => Some(bent_in),
        MaskFeather::Out => Some(bent_out),
        _ => None,
    };
    if bend.is_some() {
        rx += rx;
        ry += ry;
    }
    let longest = w.max(h);
    let across = Gaussian::new(rx);
    let mut blurred = vec![0.0f32; w * h];
    bv_color::par_rows(&mut blurred, w, |l, row| {
        let (mut y, mut z) = (vec![0.0; longest], vec![0.0; longest]);
        let line = &buf[l * w..(l + 1) * w];
        across.run(
            w,
            |i| f64::from(line[i]),
            &mut y,
            &mut z,
            |j, v| row[j] = v as f32,
        );
    });
    let down = Gaussian::new(ry);
    let (mut y, mut z) = (vec![0.0; longest], vec![0.0; longest]);
    for c in 0..w {
        down.run(
            h,
            |i| f64::from(blurred[i * w + c]),
            &mut y,
            &mut z,
            |j, v| {
                let q = to_level(v);
                buf[j * w + c] = bend.map_or(q, |b| b(q));
            },
        );
    }
}

/// One path filled into `buf`, zero before.
fn fill_path(
    buf: &mut [u8],
    w: usize,
    h: usize,
    path: &MaskPath,
    scale: [f64; 2],
    radius: [f64; 2],
) {
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
        feather(buf, w, h, path.feather, radius);
    }
}

/// `paths`, in order, on a picture `w` × `h` of the media, whose pixels are
/// its own times `scale` and `aspect` wide: how much of each pixel they keep,
/// 0–255.
pub(crate) fn alpha(
    paths: &[MaskPath],
    w: usize,
    h: usize,
    scale: [f64; 2],
    aspect: f64,
) -> Vec<u8> {
    let mut out = vec![0u8; w * h];
    let mut first = true;
    let mut scratch = Vec::new();
    let shorter = w.min(h) as f64;
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
            out.fill(255);
        }
        if opacity > 0.0 {
            let r = shorter * path.feather_pct / 100.0;
            let radius = if aspect > 1.0 {
                [r / aspect, r]
            } else {
                [r, r * aspect]
            };
            if apart {
                scratch.clear();
                scratch.resize(w * h, 0u8);
                fill_path(&mut scratch, w, h, path, scale, radius);
                let signed = if negative && opacity < 1.0 {
                    -opacity
                } else {
                    opacity
                };
                for (o, t) in out.iter_mut().zip(&scratch) {
                    *o = if opacity >= 1.0 {
                        if negative {
                            o.saturating_sub(*t)
                        } else {
                            o.saturating_add(*t)
                        }
                    } else {
                        to_level(f64::from(*t) * signed + f64::from(*o))
                    };
                }
            } else {
                fill_path(&mut out, w, h, path, scale, radius);
                if opacity < 1.0 {
                    for o in &mut out {
                        *o = nearest(f64::from(*o) * opacity + 1.5e-8) as u8;
                    }
                }
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
        assert_eq!(bent_out(0), 0);
        assert_eq!(bent_out(64), 213);
        assert_eq!(bent_out(128), 255);
        assert_eq!(bent_out(255), 255);
        assert_eq!(bent_in(255), 255);
        assert_eq!(bent_in(254), 254);
        assert_eq!(bent_in(100), 0);
    }

    #[test]
    fn a_rectangle_fills_as_far_as_it_covers() {
        let mut p = MaskPath::rectangle([10.25, 10.0], [30.0, 20.0]);
        let c = alpha(std::slice::from_ref(&p), 40, 30, [1.0, 1.0], 1.0);
        let at = |c: &[u8], x: usize, y: usize| c[y * 40 + x];
        assert_eq!(at(&c, 20, 15), 255);
        assert_eq!(at(&c, 2, 2), 0);
        // 0.75 of a pixel: 191.25.
        assert_eq!(at(&c, 10, 15), 191);
        assert_eq!(at(&c, 9, 15), 0);
        // Without anti-aliasing, whole or nothing.
        p.anti_alias = false;
        let c = alpha(&[p], 40, 30, [1.0, 1.0], 1.0);
        assert_eq!(at(&c, 10, 15), 255);
        assert_eq!(at(&c, 9, 15), 0);
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
        assert_eq!(c[15 * 40 + 20], 0);
        assert_eq!(c[2 * 40 + 2], 255);
    }

    #[test]
    fn paths_add_up_and_take_away() {
        let a = MaskPath::rectangle([0.0, 0.0], [20.0, 30.0]);
        let mut b = MaskPath::rectangle([5.0, 5.0], [15.0, 25.0]);
        b.mode = MaskMode::Negative;
        let c = alpha(&[a.clone(), b], 40, 30, [1.0, 1.0], 1.0);
        assert_eq!(c[15 * 40 + 10], 0, "the hole");
        assert_eq!(c[15 * 40 + 2], 255, "around the hole");
        let d = MaskPath::rectangle([20.0, 0.0], [40.0, 30.0]);
        let c = alpha(&[a, d], 40, 30, [1.0, 1.0], 1.0);
        assert_eq!(c[15 * 40 + 30], 255, "the second one added");
    }

    #[test]
    fn half_an_opacity_is_half_a_level() {
        let mut p = MaskPath::rectangle([0.0, 0.0], [40.0, 30.0]);
        p.opacity = 0.5;
        let c = alpha(&[p], 40, 30, [1.0, 1.0], 1.0);
        // 127.5, its tie to the even number with what the rounding adds.
        assert_eq!(c[0], 128);
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
