//! A spline through keys (the 3D transitions' paths, Gradient Map's
//! colors): each piece a cubic Bézier between two points, its handles from
//! the points around them (curvature 0.5), pulled toward the straight
//! line by a point's weight under 1.

/// A spline point: its time, three values and its weight (1: smooth).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Point {
    pub t: f64,
    pub v: [f64; 3],
    pub w: f64,
}

/// One piece of a spline: from time `t[0]` to `t[1]`, each value's Bézier
/// (its start, its two handles and its end).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Piece {
    pub t: [f64; 2],
    pub v: [[f64; 4]; 3],
}

impl Piece {
    /// The values at time `t` (the piece's start where it has no length).
    pub(crate) fn at(&self, t: f64) -> [f64; 3] {
        let [t1, t2] = self.t;
        let u = if t2 - t1 > 0.0 {
            (t - t1) / (t2 - t1)
        } else {
            0.0
        };
        let bez = |[a, b, c, d]: [f64; 4]| {
            let ab = (b - a) * u + a;
            let bc = (c - b) * u + b;
            let cd = (d - c) * u + c;
            let abc = (bc - ab) * u + ab;
            let bcd = (cd - bc) * u + bc;
            (bcd - abc) * u + abc
        };
        self.v.map(bez)
    }
}

/// A spline through points (in order of time).
pub(crate) struct Spline<'a>(pub &'a [Point]);

impl Spline<'_> {
    fn handle(a: &Point, b: &Point, c: &Point, k: f64) -> Point {
        let len = |p: &Point, q: &Point| {
            let d: [f64; 3] = std::array::from_fn(|i| q.v[i] - p.v[i]);
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        };
        let (d1, d2) = (len(a, b), len(b, c));
        if d1 == 0.0 && d2 == 0.0 {
            return *b;
        }
        let (f, g) = if d2 <= d1 {
            ((d2 / d1) * k + 1.0, k)
        } else {
            (k + 1.0, (d1 / d2) * k)
        };
        let mix = |pa: f64, pb: f64, pc: f64| {
            let p = (pb - pa) * f + pa;
            let q = (pc - pb) * g + pb;
            (q - p) * 0.5 + p
        };
        Point {
            t: mix(a.t, b.t, c.t),
            v: std::array::from_fn(|i| mix(a.v[i], b.v[i], c.v[i])),
            w: mix(a.w, b.w, c.w),
        }
    }

    /// The piece before point `i` (from point `i` − 1 to it; the first
    /// and last points' own at the ends).
    pub(crate) fn piece(&self, i: usize) -> Piece {
        let pts = self.0;
        let n = pts.len() as i64;
        let i = i as i64;
        let get = |k: i64| pts[k.clamp(0, n - 1) as usize];
        let (p0, p1, p2, p3) = (get(i - 2), get(i - 1), get(i), get(i + 1));
        let mut c1 = Spline::handle(&p0, &p1, &p2, 0.5);
        let mut c2 = Spline::handle(&p3, &p2, &p1, 0.5);
        let third = |a: f64, b: f64| (b - a) * (1.0 / 3.0) + a;
        if p1.w < 1.0 {
            let lerp = |line: f64, h: f64| (h - line) * p1.w + line;
            c1 = Point {
                t: lerp(third(p1.t, p2.t), c1.t),
                v: std::array::from_fn(|k| lerp(third(p1.v[k], p2.v[k]), c1.v[k])),
                w: lerp(third(p1.w, p2.w), c1.w),
            };
        }
        if p2.w < 1.0 {
            let lerp = |line: f64, h: f64| (h - line) * p2.w + line;
            c2 = Point {
                t: lerp(third(p2.t, p1.t), c2.t),
                v: std::array::from_fn(|k| lerp(third(p2.v[k], p1.v[k]), c2.v[k])),
                w: lerp(third(p2.w, p1.w), c2.w),
            };
        }
        Piece {
            t: [p1.t, p2.t],
            v: std::array::from_fn(|k| [p1.v[k], c1.v[k], c2.v[k], p2.v[k]]),
        }
    }

    /// The values at time `t`.
    pub(crate) fn at(&self, t: f64) -> [f64; 3] {
        let pts = self.0;
        if pts.is_empty() {
            return [0.0; 3];
        }
        let i = pts.iter().position(|p| p.t > t).unwrap_or(pts.len());
        self.piece(i).at(t)
    }
}
