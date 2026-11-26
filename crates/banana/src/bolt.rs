//! Lightning: a path worked out here, through any places, and lit by the
//! `bolt` kernels.
//!
//! A bolt follows a curve ([`Guide`]) measured along its length. It strays
//! from it by noise of whole bends, each half as long as the last and
//! weaker, with corners where the bends meet; the longest are parts of the
//! bolt's length when it is short and alike when it is long, and how far
//! they stray is a part of how long they are: a bolt of any size and curve
//! is as crooked as any other. Branches are bolts of their own. All is
//! counted in the frame's heights and hangs on no picture's size.

use crate::fx::B;
use crate::generated::k;
use crate::graph::Slot;
use crate::nums;

type P = [f32; 2];

fn add(a: P, b: P, k: f32) -> P {
    [a[0] + b[0] * k, a[1] + b[1] * k]
}

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

fn len(a: P) -> f32 {
    a[0].hypot(a[1])
}

fn turn(a: P, by: f32) -> P {
    let (s, c) = by.sin_cos();
    [a[0] * c - a[1] * s, a[0] * s + a[1] * c]
}

/// 0 to 1 as `x` goes from 0 to `over`, softly.
fn ramp(x: f32, over: f32) -> f32 {
    let t = (x / over.max(1e-9)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A number 0 to 1 of two others (the kernels' own noise).
fn rnd(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    (h >> 8) as f32 / 16_777_216.0
}

/// A curve through places, measured along it.
#[derive(Clone, Debug)]
pub struct Guide {
    points: Vec<P>,
    /// How far along each of them is.
    along: Vec<f32>,
    /// How far along each place given is.
    marks: Vec<f32>,
}

impl Guide {
    /// Pieces between two places.
    const PIECES: usize = 24;

    /// The curve through `places`: round, or straight from each to the
    /// next.
    pub fn through(places: &[P], round: bool) -> Guide {
        let mut points: Vec<P> = Vec::new();
        let mut marks = Vec::new();
        let at = |i: isize| -> P {
            let last = places.len() as isize - 1;
            let get = |i: isize| places[i.clamp(0, last) as usize];
            // Before the first and after the last it would go on as it does.
            if i < 0 {
                add(get(0), sub(get(1), get(0)), -1.0)
            } else if i > last {
                add(get(last), sub(get(last), get(last - 1)), 1.0)
            } else {
                get(i)
            }
        };
        if let Some(p) = places.first() {
            points.push(*p);
            marks.push(0);
        }
        for i in 0..places.len().saturating_sub(1) as isize {
            let [p0, p1, p2, p3] = [at(i - 1), at(i), at(i + 1), at(i + 2)];
            if round && places.len() > 2 {
                // As fast as the places are apart, so that it neither loops
                // nor swings out between near ones.
                let gap = |a: P, b: P| len(sub(b, a)).sqrt().max(1e-4);
                let (t01, t12, t23) = (gap(p0, p1), gap(p1, p2), gap(p2, p3));
                let m = |n: usize| -> (f32, f32) {
                    let d = p2[n] - p1[n];
                    let first = (p1[n] - p0[n]) / t01 - (p2[n] - p0[n]) / (t01 + t12);
                    let second = (p3[n] - p2[n]) / t23 - (p3[n] - p1[n]) / (t12 + t23);
                    (d + t12 * first, d + t12 * second)
                };
                let (mx, my) = (m(0), m(1));
                for j in 1..=Self::PIECES {
                    let t = j as f32 / Self::PIECES as f32;
                    let (t2, t3) = (t * t, t * t * t);
                    let on = |n: usize, (a, b): (f32, f32)| {
                        (2.0 * t3 - 3.0 * t2 + 1.0) * p1[n]
                            + (t3 - 2.0 * t2 + t) * a
                            + (3.0 * t2 - 2.0 * t3) * p2[n]
                            + (t3 - t2) * b
                    };
                    points.push([on(0, mx), on(1, my)]);
                }
            } else {
                points.push(p2);
            }
            marks.push(points.len() - 1);
        }
        let mut along = Vec::with_capacity(points.len());
        let mut s = 0.0;
        for (i, p) in points.iter().enumerate() {
            if i > 0 {
                s += len(sub(*p, points[i - 1]));
            }
            along.push(s);
        }
        let marks = marks.into_iter().map(|i| along[i]).collect();
        Guide {
            points,
            along,
            marks,
        }
    }

    pub fn len(&self) -> f32 {
        self.along.last().copied().unwrap_or(0.0)
    }

    /// How far along each of the places it goes through is.
    pub fn marks(&self) -> &[f32] {
        &self.marks
    }

    /// The points it is drawn with.
    pub fn points(&self) -> &[P] {
        &self.points
    }

    /// The place `s` along it.
    pub fn at(&self, s: f32) -> P {
        let i = self.along.partition_point(|a| *a <= s);
        if i == 0 || i >= self.points.len() {
            let end = if i == 0 { 0 } else { self.points.len() - 1 };
            return self.points.get(end).copied().unwrap_or([0.0; 2]);
        }
        let (a, b) = (self.along[i - 1], self.along[i]);
        let t = (s - a) / (b - a).max(1e-9);
        add(
            self.points[i - 1],
            sub(self.points[i], self.points[i - 1]),
            t,
        )
    }

    /// Which way it goes about `s`, `over` to each side.
    fn way(&self, s: f32, over: f32) -> P {
        let d = sub(
            self.at((s + over).min(self.len())),
            self.at((s - over).max(0.0)),
        );
        let l = len(d);
        if l < 1e-7 {
            // It turns back on itself: as from end to end.
            let d = sub(self.at(self.len()), self.at(0.0));
            let l = len(d);
            return if l < 1e-7 {
                [1.0, 0.0]
            } else {
                [d[0] / l, d[1] / l]
            };
        }
        [d[0] / l, d[1] / l]
    }
}

/// A place of a path: where, how wide to each side and how bright.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Knot {
    pub at: P,
    pub w: f32,
    pub light: f32,
}

/// How bolts look. Lengths are in frame heights.
#[derive(Clone, Copy, Debug)]
pub struct Bolts {
    pub bolts: i32,
    /// Branches each.
    pub branches: i32,
    /// How bright branches are, and how long (1: as they come).
    pub branch: f32,
    pub branch_long: f32,
    /// How far they stray.
    pub stray: f32,
    /// How fine their bends are.
    pub fine: f32,
    /// How near they keep to the places they go through (0 to 1).
    pub hold: f32,
    /// How much of the way they have come.
    pub reach: f32,
    /// How they thin to their ends.
    pub thin: f32,
    /// How their brightness varies.
    pub flicker: f32,
    /// A bolt's whole width, and its glow's.
    pub width: f32,
    pub glow: f32,
    /// How far they have writhed.
    pub writhe: f32,
    /// The strike, and the bolts' own number.
    pub strike: i32,
    pub seed: u32,
}

/// The longest bends are no longer than this, and the shortest no shorter.
const LONGEST: f32 = 0.5;
const SHORTEST: f32 = 0.0008;
/// The most bends each half as long as the last, and corners on a path.
const OCTAVES: usize = 6;
const CORNERS: f32 = 2048.0;

/// A path straying from its guide.
#[derive(Debug)]
struct Path<'a> {
    guide: &'a Guide,
    /// The longest bends' length.
    bend: f32,
    stray: f32,
    hold: f32,
    writhe: f32,
    seed: u32,
}

impl<'a> Path<'a> {
    fn new(guide: &'a Guide, o: &Bolts, stray: f32, hold: f32, seed: u32) -> Path<'a> {
        let l = guide.len().max(1e-6);
        let bend = 1.0 / (2.0 / l).hypot(o.fine.max(0.1) / LONGEST);
        Path {
            guide,
            bend: bend.max(l * 32.0 / CORNERS),
            stray,
            hold,
            writhe: o.writhe,
            seed,
        }
    }

    /// How much bends of octave `o` matter.
    fn weight(&self, o: usize) -> f32 {
        let l = self.bend / (1u32 << o) as f32;
        0.55f32.powi(o as i32) * (l / SHORTEST).log2().clamp(0.0, 1.0)
    }

    /// How far apart its corners are.
    fn step(&self) -> f32 {
        self.bend / (1u32 << (OCTAVES - 1)) as f32
    }

    /// How far off its guide it is at `s`, to its side and along it: noise
    /// straight from corner to corner, about 1 for the longest bends.
    fn off(&self, s: f32) -> P {
        let mut sum = [0.0; 2];
        for o in 0..OCTAVES {
            let w = self.weight(o) * self.keep(s, self.bend / (1u32 << o) as f32 * 0.6);
            if w <= 0.0 {
                continue;
            }
            let x = s * (1u32 << o) as f32 / self.bend;
            // A corner reckoned a hair before itself is itself.
            let i = (x + 1e-3).floor();
            let f = (x - i).clamp(0.0, 1.0);
            // Each corner walks through numbers of its own, the fine ones
            // faster.
            let t = self.writhe * (1.0 + 0.7 * o as f32);
            let n = t.floor();
            let u = ramp(t - n, 1.0);
            for (which, sum) in sum.iter_mut().enumerate() {
                let seed = self
                    .seed
                    .wrapping_add(which as u32 * 5003)
                    .wrapping_add(o as u32 * 101)
                    .wrapping_add((n as i32 as u32).wrapping_mul(7919));
                let corner = |c: i32| {
                    let a = rnd(c, o as i32, seed);
                    let b = rnd(c, o as i32, seed.wrapping_add(7919));
                    (a + (b - a) * u) * 2.0 - 1.0
                };
                let (a, b) = (corner(i as i32), corner(i as i32 + 1));
                *sum += (a + (b - a) * f) * w;
            }
        }
        sum
    }

    /// How much bends that take `over` to swell stray at `s`: not at its
    /// ends, and less at the places it keeps to.
    fn keep(&self, s: f32, over: f32) -> f32 {
        let mut k = ramp(s, over) * ramp(self.guide.len() - s, over);
        let marks = self.guide.marks();
        if self.hold > 0.0 && marks.len() > 2 {
            for m in &marks[1..marks.len() - 1] {
                k *= 1.0 - self.hold * (1.0 - ramp((s - m).abs(), over));
            }
        }
        k
    }

    /// The place `s` along it.
    fn at(&self, s: f32) -> P {
        let far = self.stray * self.bend * 1.6;
        let off = self.off(s);
        let way = self.guide.way(s, self.bend * 0.5);
        let p = add(self.guide.at(s), [-way[1], way[0]], off[0] * far);
        add(p, way, off[1] * far * 0.35)
    }

    /// The corner nearest `s`.
    fn corner(&self, s: f32) -> f32 {
        let step = self.step();
        (s / step).round() * step
    }

    /// Its corners as far as `reach` (0 to 1) of it, `look` telling how
    /// wide and bright it is at each part of its length.
    fn knots(&self, reach: f32, look: impl Fn(f32) -> (f32, f32)) -> Vec<Knot> {
        let l = self.guide.len();
        let end = l * reach.clamp(0.0, 1.0);
        if end <= 1e-6 {
            return Vec::new();
        }
        let step = self.step();
        let n = ((end / step) as usize).min(2 * CORNERS as usize);
        let knot = |s: f32| {
            let (w, light) = look(s / l);
            Knot {
                at: self.at(s),
                w: w * (0.35 + 0.65 * ramp(end - s, (l * 0.06).min(end))),
                light,
            }
        };
        let mut v: Vec<Knot> = (0..=n).map(|j| knot(j as f32 * step)).collect();
        if end - n as f32 * step > step * 1e-3 {
            v.push(knot(end));
        }
        v
    }
}

/// How much thinner and nearer its glow a short bolt is: it stays a bolt.
fn fit(l: f32) -> f32 {
    (l / 0.4).clamp(0.0, 1.0).sqrt()
}

/// The paths of the bolts along `guide` and their branches.
pub fn paths(guide: &Guide, o: &Bolts) -> Vec<Vec<Knot>> {
    let mut v = Vec::new();
    let l = guide.len();
    if l < 1e-4 || o.reach <= 0.0 {
        return v;
    }
    let half = o.width * 0.5 * fit(l);
    for n in 0..o.bolts {
        let seed = o
            .seed
            .wrapping_add(n as u32 * 1013)
            .wrapping_add((o.strike as u32).wrapping_mul(37));
        let alive = 1.0 - o.flicker * rnd(n * 3 + o.strike, 17, o.seed.wrapping_add(5));
        // Bolts beside the first part from it.
        let stray = o.stray * (1.0 + 0.22 * n as f32);
        let hold = o.hold * (1.0 - 0.1 * n as f32);
        let path = Path::new(guide, o, stray, hold, seed);
        let knots = path.knots(o.reach, |t| (half * (1.0 - o.thin * t), alive));
        if knots.len() < 2 {
            continue;
        }
        v.push(knots);
        branches(
            &mut v,
            &path,
            o,
            o.branches,
            half,
            alive * o.branch,
            o.reach,
            0,
        );
    }
    v
}

/// The branches of a path `half` wide, as far as it has come.
#[allow(clippy::too_many_arguments)]
fn branches(
    v: &mut Vec<Vec<Knot>>,
    from: &Path<'_>,
    o: &Bolts,
    count: i32,
    half: f32,
    light: f32,
    reach: f32,
    level: i32,
) {
    if light <= 0.0 || o.branch_long <= 0.0 {
        return;
    }
    let l = from.guide.len();
    for n in 0..count {
        let seed = from
            .seed
            .wrapping_add(n as u32 * 71 + 9)
            .wrapping_mul(2_654_435_761);
        let h = |k: i32| rnd(k, level, seed);
        let t0 = 0.08 + 0.78 * (n as f32 + h(1)) / count as f32;
        if reach <= t0 {
            continue;
        }
        let s0 = from.corner(t0 * l);
        let side = if h(2) < 0.5 { -1.0 } else { 1.0 };
        let root = from.at(s0);
        let way = turn(
            from.guide.way(s0, from.bend * 0.5),
            side * (0.35 + 0.75 * h(3)),
        );
        let part = if level == 0 { 0.17 } else { 0.3 } + 0.24 * h(4);
        let long = l * part * (1.0 - 0.4 * t0) * o.branch_long;
        // It bends back the way of the bolt it comes from.
        let bent = turn(way, -side * (0.1 + 0.35 * h(5)));
        let middle = add(root, way, long * 0.5);
        let guide = Guide::through(&[root, middle, add(middle, bent, long * 0.5)], true);
        let path = Path::new(&guide, o, o.stray * 1.15, 0.0, seed.wrapping_add(3));
        let come = ((reach - t0) / (1.0 - t0).min(0.22)).clamp(0.0, 1.0);
        let w = half * (0.68 - 0.14 * level as f32);
        let knots = path.knots(come, |t| (w * (1.0 - 0.75 * t), light * (1.0 - 0.6 * t)));
        if knots.len() < 2 {
            continue;
        }
        v.push(knots);
        if level == 0 {
            let more = ((count + 1) / 3).min(3);
            branches(v, &path, o, more, w, light * 0.8, come, 1);
        }
    }
}

/// Pieces a row holds, and rows a circle goes round.
const ROW: usize = 8;
const ROWS: usize = 8;
/// A row: its circle, its places, and every [`ROWS`]th the rows' circle.
const WIDE: usize = ROW + 3;

/// The circle round circles (middle, radius, the widest in them).
fn round(all: &[[f32; 4]]) -> [f32; 4] {
    let mut lo = [f32::MAX; 2];
    let mut hi = [f32::MIN; 2];
    for c in all {
        for n in 0..2 {
            lo[n] = lo[n].min(c[n] - c[2]);
            hi[n] = hi[n].max(c[n] + c[2]);
        }
    }
    let m = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
    let r = all
        .iter()
        .map(|c| len(sub([c[0], c[1]], m)) + c[2])
        .fold(0.0, f32::max);
    [m[0], m[1], r, all.iter().map(|c| c[3]).fold(0.0, f32::max)]
}

/// The paths as the kernel reads them, and how many rows they are.
fn rows(paths: &[Vec<Knot>]) -> (Vec<[f32; 4]>, usize) {
    let mut data: Vec<[f32; 4]> = Vec::new();
    for path in paths {
        for first in (0..path.len().saturating_sub(1)).step_by(ROW) {
            let knots: [Knot; ROW + 1] =
                std::array::from_fn(|n| path[(first + n).min(path.len() - 1)]);
            let dots = knots.map(|k| [k.at[0], k.at[1], 0.0, k.w]);
            data.push(round(&dots));
            data.extend(knots.map(|k| [k.at[0], k.at[1], k.w, k.light]));
            data.push([0.0; 4]);
        }
    }
    let n = data.len() / WIDE;
    for first in (0..n).step_by(ROWS) {
        let circles: Vec<[f32; 4]> = (first..(first + ROWS).min(n))
            .map(|r| data[r * WIDE])
            .collect();
        data[first * WIDE + WIDE - 1] = round(&circles);
    }
    if data.is_empty() {
        data.resize(WIDE, [0.0; 4]);
    }
    (data, n)
}

/// Light's colors and strength.
#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub brightness: f32,
    pub core: [f32; 3],
    pub glow: [f32; 3],
}

/// The bolts along `guide` lit: light alone, as see-through as it is dark.
pub fn draw(b: &mut B<'_>, guide: &Guide, o: &Bolts, c: Light) -> Slot {
    let all = paths(guide, o);
    let (data, n) = rows(&all);
    let height = data.len() / WIDE;
    let data = b.data([WIDE as u32, height as u32], data);
    let shape = b.at.shape;
    let (w, h) = (b.w(), b.h());
    // A pixel of the picture is a soft dot as wide as itself.
    let px = [shape / w as f32, 1.0 / h as f32];
    let at = [-0.5 * shape + 0.5 * px[0], -0.5 + 0.5 * px[1]];
    let soft = 0.6 * px[0].max(px[1]);
    let core = b.run(k::BOLT, &[data], nums![at, px, soft], [n as i32, 0, 0, 0]);
    let size = fit(guide.len());
    let half = (o.width * 0.5 * size).max(1e-6);
    let glow = o.glow.max(1e-4) * size;
    let near = halo(b, data, n, glow * 0.5, half, 0.62);
    let wide = halo(b, data, n, glow * 1.7, half, 0.2);
    let numbers = nums![
        c.brightness,
        c.core,
        c.glow,
        0.0,
        near.from,
        near.per,
        near.gain,
        wide.from,
        wide.per,
        wide.gain,
    ];
    b.run(
        k::BOLT_LIGHT,
        &[core, near.slot, wide.slot],
        numbers,
        [0; 4],
    )
}

/// A glow, at a size of its own.
#[derive(Debug)]
struct Halo {
    slot: Slot,
    /// From the frame's middle to its first pixel, its pixels a frame
    /// height, and its strength.
    from: P,
    per: f32,
    gain: f32,
}

/// The paths blurred `wide` (a Gaussian's sigma), as bright as `peak` over
/// a straight bolt `half` wide: drawn anew, as big as the blur needs and
/// past the frame's edges, whatever the frame's size in pixels.
fn halo(b: &mut B<'_>, data: Slot, rows: usize, wide: f32, half: f32, peak: f32) -> Halo {
    /// The blur's sigma in pixels.
    const SIGMA: f32 = 4.0;
    let shape = b.at.shape;
    let per = (SIGMA / wide).min(b.h().clamp(64, 1080) as f32);
    let pad = wide * 3.2;
    let size = [
        ((shape + 2.0 * pad) * per).ceil().clamp(1.0, 16384.0) as u32,
        ((1.0 + 2.0 * pad) * per).ceil().clamp(1.0, 16384.0) as u32,
    ];
    let from = [
        -0.5 * size[0] as f32 / per + 0.5 / per,
        -0.5 * size[1] as f32 / per + 0.5 / per,
    ];
    let numbers = nums![from, 1.0 / per, 1.0 / per, 1.0 / per];
    let ints = [rows as i32, 0, 0, 0];
    let dots = b.run_sized(k::BOLT, &[data], numbers, ints, size, 0);
    // What drawing the dots and enlarging them blurs already.
    let sigma = wide * per;
    let left = (sigma * sigma - 0.5 - 1.0 / 3.0).max(0.0).sqrt();
    let slot = if left >= 0.05 {
        let reach = ((left * 3.0).ceil() as i32).clamp(1, 24);
        let ints = [0, reach, 0, 0];
        let a = b.run_sized(k::GAUSS_X, &[dots], nums![left], ints, size, 0);
        b.run_sized(k::GAUSS_Y, &[a], nums![left], ints, size, 0)
    } else {
        dots
    };
    Halo {
        slot,
        from,
        per,
        gain: peak * std::f32::consts::SQRT_2 * wide.max(0.9 / per) / half,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bolts() -> Bolts {
        Bolts {
            bolts: 2,
            branches: 4,
            branch: 0.7,
            branch_long: 1.0,
            stray: 0.18,
            fine: 1.0,
            hold: 0.5,
            reach: 1.0,
            thin: 0.4,
            flicker: 0.45,
            width: 0.005,
            glow: 0.03,
            writhe: 0.7,
            strike: 3,
            seed: 1,
        }
    }

    /// A round curve goes through its places, in order, and never far out
    /// between them.
    #[test]
    fn a_guide_goes_through_its_places() {
        let places = [[-0.8, 0.0], [-0.1, 0.4], [-0.05, 0.38], [0.6, -0.3]];
        for round in [false, true] {
            let g = Guide::through(&places, round);
            assert_eq!(g.marks().len(), places.len());
            for (m, p) in g.marks().iter().zip(&places) {
                let d = len(sub(g.at(*m), *p));
                assert!(d < 1e-4, "{d} from {p:?}");
            }
            assert!(
                g.points()
                    .iter()
                    .all(|p| p[0].abs() < 0.9 && p[1].abs() < 0.5)
            );
            assert!(g.len() >= 1.8 && g.len() < 2.2, "{}", g.len());
        }
    }

    /// However long it is and whatever its curve, a bolt is as crooked:
    /// the slope of its straying stays the same, and it ends where told.
    #[test]
    fn a_bolt_of_any_size_is_as_crooked() {
        let crooked = |l: f32, curve: f32, seed: u32| {
            let places = [[0.0, 0.0], [l * 0.5, l * curve], [l, 0.0]];
            let g = Guide::through(&places, true);
            let o = Bolts {
                bolts: 1,
                branches: 0,
                hold: 0.0,
                seed,
                ..bolts()
            };
            let v = paths(&g, &o);
            let k = &v[0];
            assert!(len(sub(k[0].at, places[0])) < 1e-5);
            assert!(len(sub(k[k.len() - 1].at, places[2])) < l * 1e-3);
            let run: f32 = k.windows(2).map(|w| len(sub(w[1].at, w[0].at))).sum();
            run / g.len()
        };
        for l in [0.02, 0.06, 0.3, 1.0, 2.0, 5.0] {
            for curve in [0.0, 0.2, 0.6] {
                let c: f32 = (0..32).map(|s| crooked(l, curve, s)).sum::<f32>() / 32.0;
                assert!((1.05..1.3).contains(&c), "{l}, {curve}: {c}");
            }
        }
    }

    /// Making it longer moves it little by little: nothing jumps.
    #[test]
    fn a_bolt_moves_smoothly_as_it_grows() {
        let o = bolts();
        let make = |l: f32| paths(&Guide::through(&[[0.0, 0.0], [l, 0.1]], true), &o);
        let mut l = 0.05;
        while l < 1.6 {
            let (a, b) = (make(l), make(l + 0.0005));
            assert_eq!(a.len(), b.len());
            for (x, y) in a.iter().zip(&b) {
                // Halfway along both.
                let at = |v: &[Knot]| v[v.len() / 2].at;
                let d = len(sub(at(x), at(y)));
                assert!(d < 0.03, "{d} at {l}");
            }
            l += 0.013;
        }
    }

    /// Every piece is inside the circles that tell where it is.
    #[test]
    fn the_circles_hold_their_pieces() {
        let g = Guide::through(&[[-0.8, -0.4], [0.2, 0.3], [0.7, -0.2]], true);
        let all = paths(&g, &bolts());
        let (data, n) = rows(&all);
        assert_eq!(data.len(), n * WIDE);
        let inside = |c: [f32; 4], k: [f32; 4]| {
            len(sub([c[0], c[1]], [k[0], k[1]])) <= c[2] + 1e-5 && k[2] <= c[3] + 1e-9
        };
        for r in 0..n {
            let row = &data[r * WIDE..(r + 1) * WIDE];
            let group = data[r / ROWS * ROWS * WIDE + WIDE - 1];
            for k in &row[1..=ROW + 1] {
                assert!(inside(row[0], *k) && inside(group, *k));
            }
        }
        let pieces: usize = all.iter().map(|p| (p.len() - 1).div_ceil(ROW)).sum();
        assert_eq!(n, pieces);
    }

    /// What no one would set draws nothing, or something, and never hangs.
    #[test]
    fn strange_bolts_are_drawn_too() {
        for l in [0.0, 1e-6, 1e-3, 280.0] {
            for (bolts_n, stray, fine, reach) in [(6, 3.2, 4.0, 1.0), (1, 0.0, 0.5, 0.001)] {
                let o = Bolts {
                    bolts: bolts_n,
                    branches: 8,
                    stray,
                    fine,
                    reach,
                    ..bolts()
                };
                let g = Guide::through(&[[0.0, 0.0], [l, 0.0], [l, 0.0], [0.0, l]], true);
                for p in paths(&g, &o).iter().flatten() {
                    assert!(p.at.iter().all(|v| v.is_finite()), "{l}");
                    assert!(p.w >= 0.0 && p.light.is_finite());
                }
            }
        }
    }
}
