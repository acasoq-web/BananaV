//! SVG's geometry: points and transforms, path data (SVG 1.1, 8.3, with
//! its arcs drawn as cubic curves as its implementation notes, F.6,
//! describe), the basic shapes as paths, and paths flattened into lines.

use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct P {
    pub x: f64,
    pub y: f64,
}

pub(super) const fn p(x: f64, y: f64) -> P {
    P { x, y }
}

impl std::ops::Add for P {
    type Output = P;
    fn add(self, o: P) -> P {
        p(self.x + o.x, self.y + o.y)
    }
}

impl std::ops::Sub for P {
    type Output = P;
    fn sub(self, o: P) -> P {
        p(self.x - o.x, self.y - o.y)
    }
}

impl std::ops::Mul<f64> for P {
    type Output = P;
    fn mul(self, k: f64) -> P {
        p(self.x * k, self.y * k)
    }
}

impl P {
    pub fn len(self) -> f64 {
        self.x.hypot(self.y)
    }

    pub fn dot(self, o: P) -> f64 {
        self.x * o.x + self.y * o.y
    }

    pub fn cross(self, o: P) -> f64 {
        self.x * o.y - self.y * o.x
    }

    /// At length 1 (the zero vector stays).
    pub fn unit(self) -> P {
        let l = self.len();
        if l > 0.0 { self * (1.0 / l) } else { self }
    }

    /// Turned a quarter: (−y, x).
    pub fn perp(self) -> P {
        p(-self.y, self.x)
    }
}

/// A rectangle: its corners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    pub fn of(points: impl IntoIterator<Item = P>) -> Option<Rect> {
        let mut r: Option<Rect> = None;
        for q in points {
            if !(q.x.is_finite() && q.y.is_finite()) {
                continue;
            }
            r = Some(match r {
                None => Rect {
                    x0: q.x,
                    y0: q.y,
                    x1: q.x,
                    y1: q.y,
                },
                Some(r) => Rect {
                    x0: r.x0.min(q.x),
                    y0: r.y0.min(q.y),
                    x1: r.x1.max(q.x),
                    y1: r.y1.max(q.y),
                },
            });
        }
        r
    }

    pub fn union(self, o: Rect) -> Rect {
        Rect {
            x0: self.x0.min(o.x0),
            y0: self.y0.min(o.y0),
            x1: self.x1.max(o.x1),
            y1: self.y1.max(o.y1),
        }
    }

    pub fn grow(self, by: f64) -> Rect {
        Rect {
            x0: self.x0 - by,
            y0: self.y0 - by,
            x1: self.x1 + by,
            y1: self.y1 + by,
        }
    }

    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    pub fn corners(&self) -> [P; 4] {
        [
            p(self.x0, self.y0),
            p(self.x1, self.y0),
            p(self.x1, self.y1),
            p(self.x0, self.y1),
        ]
    }
}

/// An affine transform: x' = a·x + c·y + e, y' = b·x + d·y + f (SVG's
/// `matrix(a b c d e f)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Mat {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Mat {
    pub const IDENTITY: Mat = Mat {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn translate(x: f64, y: f64) -> Mat {
        Mat {
            e: x,
            f: y,
            ..Mat::IDENTITY
        }
    }

    pub fn scale(x: f64, y: f64) -> Mat {
        Mat {
            a: x,
            d: y,
            ..Mat::IDENTITY
        }
    }

    /// Turned `deg` degrees (clockwise, y down).
    pub fn rotate(deg: f64) -> Mat {
        let (s, c) = deg.to_radians().sin_cos();
        Mat {
            a: c,
            b: s,
            c: -s,
            d: c,
            e: 0.0,
            f: 0.0,
        }
    }

    /// `self` after `inner`: `inner` applies first.
    pub fn then_after(&self, inner: &Mat) -> Mat {
        let m = self;
        Mat {
            a: m.a * inner.a + m.c * inner.b,
            b: m.b * inner.a + m.d * inner.b,
            c: m.a * inner.c + m.c * inner.d,
            d: m.b * inner.c + m.d * inner.d,
            e: m.a * inner.e + m.c * inner.f + m.e,
            f: m.b * inner.e + m.d * inner.f + m.f,
        }
    }

    pub fn apply(&self, q: P) -> P {
        p(
            self.a * q.x + self.c * q.y + self.e,
            self.b * q.x + self.d * q.y + self.f,
        )
    }

    pub fn inverse(&self) -> Option<Mat> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-12 || !det.is_finite() {
            return None;
        }
        let i = 1.0 / det;
        Some(Mat {
            a: self.d * i,
            b: -self.b * i,
            c: -self.c * i,
            d: self.a * i,
            e: (self.c * self.f - self.d * self.e) * i,
            f: (self.b * self.e - self.a * self.f) * i,
        })
    }

    /// How much it stretches at most (its larger singular value).
    pub fn max_scale(&self) -> f64 {
        let (a, b, c, d) = (self.a, self.b, self.c, self.d);
        let s = a * a + b * b + c * c + d * d;
        let det = a * d - b * c;
        let disc = (s * s - 4.0 * det * det).max(0.0).sqrt();
        ((s + disc) / 2.0).sqrt()
    }

    /// The rectangle `r` covers once transformed (its corners').
    pub fn rect(&self, r: &Rect) -> Rect {
        Rect::of(r.corners().map(|q| self.apply(q))).unwrap_or(*r)
    }
}

/// Numbers, as SVG writes them in its lists: separated by white space or a
/// comma, or by nothing where a sign or a second point starts the next one
/// (`1.5.5` is 1.5 then .5, `1-2` is 1 then −2).
#[derive(Debug)]
pub(super) struct Scanner<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Scanner<'a> {
    pub fn new(s: &'a str) -> Scanner<'a> {
        Scanner {
            s: s.as_bytes(),
            i: 0,
        }
    }

    fn skip_separators(&mut self) {
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',')
        {
            self.i += 1;
        }
    }

    /// The next byte, without taking it (separators skipped).
    pub fn peek(&mut self) -> Option<u8> {
        self.skip_separators();
        self.s.get(self.i).copied()
    }

    /// The next number.
    pub fn number(&mut self) -> Option<f64> {
        self.skip_separators();
        let s = self.s;
        let start = self.i;
        let mut i = self.i;
        if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
            i += 1;
        }
        let mut digits = false;
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
            digits = true;
        }
        if i < s.len() && s[i] == b'.' {
            i += 1;
            while i < s.len() && s[i].is_ascii_digit() {
                i += 1;
                digits = true;
            }
        }
        if !digits {
            return None;
        }
        if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
            let mut j = i + 1;
            if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
                j += 1;
            }
            if j < s.len() && s[j].is_ascii_digit() {
                while j < s.len() && s[j].is_ascii_digit() {
                    j += 1;
                }
                i = j;
            }
        }
        let v = std::str::from_utf8(&s[start..i])
            .ok()?
            .parse::<f64>()
            .ok()?;
        self.i = i;
        v.is_finite().then_some(v)
    }

    /// An arc's flag: a single `0` or `1` (written with no separator after
    /// it, too).
    pub fn flag(&mut self) -> Option<bool> {
        self.skip_separators();
        let c = *self.s.get(self.i)?;
        if c == b'0' || c == b'1' {
            self.i += 1;
            Some(c == b'1')
        } else {
            None
        }
    }

    /// What is left (a unit after a number).
    pub fn rest(&self) -> &'a str {
        std::str::from_utf8(&self.s[self.i..]).unwrap_or("")
    }

    pub fn advance(&mut self, n: usize) {
        self.i = (self.i + n).min(self.s.len());
    }
}

/// All the numbers of a list (`points`, `viewBox`…).
pub(super) fn numbers(s: &str) -> Vec<f64> {
    let mut sc = Scanner::new(s);
    let mut out = Vec::new();
    while let Some(v) = sc.number() {
        out.push(v);
    }
    out
}

/// A path's segment (quadratic curves and arcs made cubic).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Seg {
    Move(P),
    Line(P),
    Cubic(P, P, P),
    Close,
}

/// Path data (`d`): its segments, up to the first error (as SVG renders a
/// path with an error in it).
pub(super) fn parse(d: &str) -> Vec<Seg> {
    let mut sc = Scanner::new(d);
    let mut out = Vec::new();
    let mut cur = P::default();
    let mut start = P::default();
    // The last curve's second control point, for S and T.
    let mut last_cubic: Option<P> = None;
    let mut last_quad: Option<P> = None;
    let mut cmd = 0u8;
    loop {
        let next = match sc.peek() {
            None => break,
            Some(c) => c,
        };
        if next.is_ascii_alphabetic() {
            cmd = next;
            sc.advance(1);
        } else if cmd == 0 || cmd == b'z' || cmd == b'Z' {
            break;
        } else if cmd == b'M' {
            // Numbers after a move are lines.
            cmd = b'L';
        } else if cmd == b'm' {
            cmd = b'l';
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { cur } else { P::default() };
        let pt = |sc: &mut Scanner<'_>| -> Option<P> {
            let x = sc.number()?;
            let y = sc.number()?;
            Some(base + p(x, y))
        };
        let mut cubic = None;
        let mut quad = None;
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let Some(q) = pt(&mut sc) else { break };
                cur = q;
                start = q;
                out.push(Seg::Move(q));
            }
            b'L' => {
                let Some(q) = pt(&mut sc) else { break };
                cur = q;
                out.push(Seg::Line(q));
            }
            b'H' => {
                let Some(x) = sc.number() else { break };
                cur = p(if rel { cur.x + x } else { x }, cur.y);
                out.push(Seg::Line(cur));
            }
            b'V' => {
                let Some(y) = sc.number() else { break };
                cur = p(cur.x, if rel { cur.y + y } else { y });
                out.push(Seg::Line(cur));
            }
            b'C' => {
                let (Some(c1), Some(c2), Some(e)) = (pt(&mut sc), pt(&mut sc), pt(&mut sc)) else {
                    break;
                };
                out.push(Seg::Cubic(c1, c2, e));
                cubic = Some(c2);
                cur = e;
            }
            b'S' => {
                let (Some(c2), Some(e)) = (pt(&mut sc), pt(&mut sc)) else {
                    break;
                };
                let c1 = last_cubic.map_or(cur, |l| cur + (cur - l));
                out.push(Seg::Cubic(c1, c2, e));
                cubic = Some(c2);
                cur = e;
            }
            b'Q' => {
                let (Some(c), Some(e)) = (pt(&mut sc), pt(&mut sc)) else {
                    break;
                };
                out.push(quad_to_cubic(cur, c, e));
                quad = Some(c);
                cur = e;
            }
            b'T' => {
                let Some(e) = pt(&mut sc) else { break };
                let c = last_quad.map_or(cur, |l| cur + (cur - l));
                out.push(quad_to_cubic(cur, c, e));
                quad = Some(c);
                cur = e;
            }
            b'A' => {
                let (Some(rx), Some(ry), Some(rot)) = (sc.number(), sc.number(), sc.number())
                else {
                    break;
                };
                let (Some(large), Some(sweep)) = (sc.flag(), sc.flag()) else {
                    break;
                };
                let Some(e) = pt(&mut sc) else { break };
                arc(&mut out, cur, rx, ry, rot, large, sweep, e);
                cur = e;
            }
            b'Z' => {
                out.push(Seg::Close);
                cur = start;
            }
            _ => break,
        }
        last_cubic = cubic;
        last_quad = quad;
    }
    out
}

fn quad_to_cubic(from: P, c: P, to: P) -> Seg {
    Seg::Cubic(
        from + (c - from) * (2.0 / 3.0),
        to + (c - to) * (2.0 / 3.0),
        to,
    )
}

/// An elliptical arc from `from` to `to` (SVG's `A`), as cubic curves.
#[allow(clippy::too_many_arguments)]
fn arc(out: &mut Vec<Seg>, from: P, rx: f64, ry: f64, rot: f64, large: bool, sweep: bool, to: P) {
    if from == to {
        return;
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 {
        out.push(Seg::Line(to));
        return;
    }
    let (s, c) = rot.to_radians().sin_cos();
    // F.6.5: the center from the end points.
    let h = (from - to) * 0.5;
    let x1 = c * h.x + s * h.y;
    let y1 = -s * h.x + c * h.y;
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let k = lambda.sqrt();
        rx *= k;
        ry *= k;
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coef = if den > 0.0 {
        (num / den).max(0.0).sqrt()
    } else {
        0.0
    };
    if large == sweep {
        coef = -coef;
    }
    let cx1 = coef * rx * y1 / ry;
    let cy1 = -coef * ry * x1 / rx;
    let mid = (from + to) * 0.5;
    let center = p(c * cx1 - s * cy1 + mid.x, s * cx1 + c * cy1 + mid.y);
    let angle = |u: P, v: P| {
        let a = u.cross(v).atan2(u.dot(v));
        if a.is_finite() { a } else { 0.0 }
    };
    let u = p((x1 - cx1) / rx, (y1 - cy1) / ry);
    let v = p((-x1 - cx1) / rx, (-y1 - cy1) / ry);
    let theta1 = angle(p(1.0, 0.0), u);
    let mut delta = angle(u, v);
    if !sweep && delta > 0.0 {
        delta -= 2.0 * PI;
    } else if sweep && delta < 0.0 {
        delta += 2.0 * PI;
    }
    // Pieces of at most a quarter turn, each one cubic curve.
    let n = (delta.abs() / (PI / 2.0)).ceil().max(1.0) as usize;
    let step = delta / n as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |t: f64| {
        let (st, ct) = t.sin_cos();
        p(
            center.x + c * rx * ct - s * ry * st,
            center.y + s * rx * ct + c * ry * st,
        )
    };
    let tangent = |t: f64| {
        let (st, ct) = t.sin_cos();
        p(-c * rx * st - s * ry * ct, -s * rx * st + c * ry * ct)
    };
    for i in 0..n {
        let t0 = theta1 + step * i as f64;
        let t1 = t0 + step;
        let p0 = point(t0);
        let p3 = if i + 1 == n { to } else { point(t1) };
        out.push(Seg::Cubic(p0 + tangent(t0) * k, p3 - tangent(t1) * k, p3));
    }
}

/// A rectangle, its corners rounded by `rx`, `ry` (SVG 1.1, 9.2).
pub(super) fn rect(x: f64, y: f64, w: f64, h: f64, rx: f64, ry: f64) -> Vec<Seg> {
    if w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }
    let rx = rx.clamp(0.0, w / 2.0);
    let ry = ry.clamp(0.0, h / 2.0);
    if rx == 0.0 || ry == 0.0 {
        return vec![
            Seg::Move(p(x, y)),
            Seg::Line(p(x + w, y)),
            Seg::Line(p(x + w, y + h)),
            Seg::Line(p(x, y + h)),
            Seg::Close,
        ];
    }
    let mut out = vec![Seg::Move(p(x + rx, y)), Seg::Line(p(x + w - rx, y))];
    arc(
        &mut out,
        p(x + w - rx, y),
        rx,
        ry,
        0.0,
        false,
        true,
        p(x + w, y + ry),
    );
    out.push(Seg::Line(p(x + w, y + h - ry)));
    arc(
        &mut out,
        p(x + w, y + h - ry),
        rx,
        ry,
        0.0,
        false,
        true,
        p(x + w - rx, y + h),
    );
    out.push(Seg::Line(p(x + rx, y + h)));
    arc(
        &mut out,
        p(x + rx, y + h),
        rx,
        ry,
        0.0,
        false,
        true,
        p(x, y + h - ry),
    );
    out.push(Seg::Line(p(x, y + ry)));
    arc(
        &mut out,
        p(x, y + ry),
        rx,
        ry,
        0.0,
        false,
        true,
        p(x + rx, y),
    );
    out.push(Seg::Close);
    out
}

/// An ellipse (a circle with `rx` = `ry`).
pub(super) fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64) -> Vec<Seg> {
    if rx <= 0.0 || ry <= 0.0 {
        return Vec::new();
    }
    let mut out = vec![Seg::Move(p(cx + rx, cy))];
    arc(
        &mut out,
        p(cx + rx, cy),
        rx,
        ry,
        0.0,
        false,
        true,
        p(cx, cy + ry),
    );
    arc(
        &mut out,
        p(cx, cy + ry),
        rx,
        ry,
        0.0,
        false,
        true,
        p(cx - rx, cy),
    );
    arc(
        &mut out,
        p(cx - rx, cy),
        rx,
        ry,
        0.0,
        false,
        true,
        p(cx, cy - ry),
    );
    arc(
        &mut out,
        p(cx, cy - ry),
        rx,
        ry,
        0.0,
        false,
        true,
        p(cx + rx, cy),
    );
    out.push(Seg::Close);
    out
}

/// A polyline's or a polygon's (`closed`) points.
pub(super) fn poly(points: &str, closed: bool) -> Vec<Seg> {
    let v = numbers(points);
    let pts: Vec<P> = v.as_chunks::<2>().0.iter().map(|c| p(c[0], c[1])).collect();
    if pts.len() < 2 {
        return Vec::new();
    }
    let mut out = vec![Seg::Move(pts[0])];
    out.extend(pts[1..].iter().map(|&q| Seg::Line(q)));
    if closed {
        out.push(Seg::Close);
    }
    out
}

/// A path's subpaths as lines, each within `tol` of its curves: their
/// points and whether each one is closed.
pub(super) fn flatten(segs: &[Seg], tol: f64) -> Vec<(Vec<P>, bool)> {
    let tol = tol.max(1e-6);
    // Points a path may become: past them, its curves are drawn as their
    // chords (a hostile file's huge curves, not a drawing's).
    let mut budget: usize = 4_000_000;
    let mut out: Vec<(Vec<P>, bool)> = Vec::new();
    let mut cur: Vec<P> = Vec::new();
    let mut at = P::default();
    let mut start = P::default();
    let finish = |cur: &mut Vec<P>, out: &mut Vec<(Vec<P>, bool)>, closed: bool| {
        if !cur.is_empty() {
            out.push((std::mem::take(cur), closed));
        }
    };
    for s in segs {
        match *s {
            Seg::Move(q) => {
                finish(&mut cur, &mut out, false);
                cur.push(q);
                at = q;
                start = q;
            }
            Seg::Line(q) => {
                if cur.is_empty() {
                    cur.push(at);
                }
                cur.push(q);
                at = q;
            }
            Seg::Cubic(c1, c2, e) => {
                if cur.is_empty() {
                    cur.push(at);
                }
                // The curve's bend bounds how far lines stray from it.
                let dd = (at - c1 * 2.0 + c2).len().max((c1 - c2 * 2.0 + e).len());
                let n = ((0.75 * dd / tol).sqrt().ceil() as usize)
                    .clamp(1, 1000)
                    .min(budget.max(1));
                budget = budget.saturating_sub(n);
                for i in 1..=n {
                    let t = i as f64 / n as f64;
                    let u = 1.0 - t;
                    let q = at * (u * u * u)
                        + c1 * (3.0 * u * u * t)
                        + c2 * (3.0 * u * t * t)
                        + e * (t * t * t);
                    cur.push(q);
                }
                at = e;
            }
            Seg::Close => {
                if !cur.is_empty() {
                    finish(&mut cur, &mut out, true);
                }
                at = start;
            }
        }
    }
    finish(&mut cur, &mut out, false);
    out
}

/// The path's control points' bounds (the path within them).
pub(super) fn bounds(segs: &[Seg]) -> Option<Rect> {
    Rect::of(segs.iter().flat_map(|s| match *s {
        Seg::Move(q) | Seg::Line(q) => vec![q],
        Seg::Cubic(a, b, c) => vec![a, b, c],
        Seg::Close => Vec::new(),
    }))
}

/// A `transform` list (SVG 1.1, 7.6): `matrix`, `translate`, `scale`,
/// `rotate` (about a point too), `skewX`, `skewY`; `None` when it is
/// written wrong (the element is not drawn).
pub(super) fn transform(s: &str) -> Option<Mat> {
    let mut m = Mat::IDENTITY;
    let mut rest = s.trim();
    while !rest.is_empty() {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        if rest.is_empty() {
            break;
        }
        let open = rest.find('(')?;
        let close = rest.find(')')?;
        if close < open {
            return None;
        }
        let name = rest[..open].trim();
        let v = numbers(&rest[open + 1..close]);
        let t = match (name, v.as_slice()) {
            ("matrix", [a, b, c, d, e, f]) => Mat {
                a: *a,
                b: *b,
                c: *c,
                d: *d,
                e: *e,
                f: *f,
            },
            ("translate", [x]) => Mat::translate(*x, 0.0),
            ("translate", [x, y]) => Mat::translate(*x, *y),
            ("scale", [k]) => Mat::scale(*k, *k),
            ("scale", [x, y]) => Mat::scale(*x, *y),
            ("rotate", [a]) => Mat::rotate(*a),
            ("rotate", [a, x, y]) => Mat::translate(*x, *y)
                .then_after(&Mat::rotate(*a))
                .then_after(&Mat::translate(-x, -y)),
            ("skewX", [a]) => Mat {
                c: a.to_radians().tan(),
                ..Mat::IDENTITY
            },
            ("skewY", [a]) => Mat {
                b: a.to_radians().tan(),
                ..Mat::IDENTITY
            },
            _ => return None,
        };
        m = m.then_after(&t);
        rest = &rest[close + 1..];
    }
    Some(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: P, b: P) -> bool {
        (a - b).len() < 1e-6
    }

    #[test]
    fn numbers_split_as_svg_writes_them() {
        assert_eq!(numbers("1.5.5-2e1,3 4"), vec![1.5, 0.5, -20.0, 3.0, 4.0]);
        // Arc flags written together.
        let segs = parse("M0 0a10 10 0 1110 10");
        assert!(matches!(segs.last(), Some(Seg::Cubic(_, _, e)) if close(*e, p(10.0, 10.0))));
    }

    #[test]
    fn relative_commands_and_implicit_lines() {
        let segs = parse("m10 10 20 0 0 20 h-20 z");
        assert_eq!(
            segs,
            vec![
                Seg::Move(p(10.0, 10.0)),
                Seg::Line(p(30.0, 10.0)),
                Seg::Line(p(30.0, 30.0)),
                Seg::Line(p(10.0, 30.0)),
                Seg::Close,
            ]
        );
    }

    #[test]
    fn an_arc_reaches_its_end_through_its_center() {
        // A half circle of radius 10 from (0, 0) to (20, 0), sweeping down
        // (y down): its middle at (10, 10).
        let segs = parse("M0 0 A10 10 0 0 0 20 0");
        let lines = flatten(&segs, 0.001);
        let pts = &lines[0].0;
        assert!(close(*pts.last().unwrap(), p(20.0, 0.0)));
        let lowest = pts.iter().map(|q| q.y).fold(f64::MIN, f64::max);
        assert!((lowest - 10.0).abs() < 0.01, "{lowest}");
        for q in pts {
            assert!(((*q - p(10.0, 0.0)).len() - 10.0).abs() < 0.01);
        }
    }

    #[test]
    fn transforms_compose_in_order() {
        let m = transform("translate(10,0) scale(2) rotate(90)").expect("a list");
        // (1, 0): turned to (0, 1), doubled, moved.
        assert!(close(m.apply(p(1.0, 0.0)), p(10.0, 2.0)));
        let r = transform("rotate(90 5 5)").expect("about a point");
        assert!(close(r.apply(p(5.0, 5.0)), p(5.0, 5.0)));
        assert!(transform("scale(1 2 3)").is_none());
        let inv = m.inverse().expect("invertible");
        assert!(close(inv.apply(m.apply(p(3.0, 4.0))), p(3.0, 4.0)));
    }
}
