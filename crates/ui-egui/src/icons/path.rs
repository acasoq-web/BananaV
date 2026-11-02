//! Path data for the icons: a subset of SVG's path syntax (`M L H V C S Q A
//! Z`, absolute and relative) read into lines and cubic Béziers on the
//! icons' 24-unit grid, flattened to polylines at paint time, and the
//! triangulation that fills them.

use crate::egui::{Pos2, Vec2, pos2, vec2};

/// One piece of a path, in grid units.
#[derive(Clone, Copy, Debug)]
pub(super) enum Seg {
    Line(Pos2),
    Cubic(Pos2, Pos2, Pos2),
}

/// A run of segments from a start point, open or closed.
#[derive(Clone, Debug)]
pub(super) struct Sub {
    pub start: Pos2,
    pub segs: Vec<Seg>,
    pub closed: bool,
}

impl Sub {
    /// The path as a polyline in pixels: curves cut into pieces of about
    /// two pixels. A closed path does not repeat its first point.
    pub fn flatten(&self, to_px: &impl Fn(Pos2) -> Pos2) -> Vec<Pos2> {
        let mut out = vec![to_px(self.start)];
        let mut at = out[0];
        for seg in &self.segs {
            match *seg {
                Seg::Line(p) => {
                    at = to_px(p);
                    out.push(at);
                }
                Seg::Cubic(a, b, c) => {
                    let (a, b, c) = (to_px(a), to_px(b), to_px(c));
                    let length = (a - at).length() + (b - a).length() + (c - b).length();
                    let n = (length / 2.0).ceil().clamp(4.0, 48.0) as usize;
                    for i in 1..=n {
                        let t = i as f32 / n as f32;
                        let u = 1.0 - t;
                        let p = at.to_vec2() * (u * u * u)
                            + a.to_vec2() * (3.0 * u * u * t)
                            + b.to_vec2() * (3.0 * u * t * t)
                            + c.to_vec2() * (t * t * t);
                        out.push(p.to_pos2());
                    }
                    at = c;
                }
            }
        }
        // Repeated points (a path that returns to its start, curves that
        // end where they began) would give strokes zero-length pieces.
        out.dedup_by(|b, a| (*b - *a).length() < 1e-3);
        if self.closed && out.len() > 1 && (out[0] - out[out.len() - 1]).length() < 1e-3 {
            out.pop();
        }
        out
    }
}

/// Reads path data. Panics on text that is not path data: the icons' data
/// is part of the program, and the tests read all of it.
pub(super) fn parse(data: &str) -> Vec<Sub> {
    match try_parse(data) {
        Ok(subs) => subs,
        Err(why) => panic!("icon path {data:?}: {why}"),
    }
}

fn try_parse(data: &str) -> Result<Vec<Sub>, String> {
    let mut scan = Scan {
        text: data.as_bytes(),
        at: 0,
    };
    let mut subs: Vec<Sub> = Vec::new();
    let mut cmd = b' ';
    let mut cur = pos2(0.0, 0.0);
    let mut first = cur;
    // The last control point of a curve, for the smooth commands.
    let mut ctrl: Option<Pos2> = None;
    loop {
        scan.skip_separators();
        let Some(c) = scan.peek() else { break };
        if c.is_ascii_alphabetic() {
            cmd = c;
            scan.at += 1;
            if matches!(cmd, b'Z' | b'z') {
                let sub = subs.last_mut().ok_or("Z before M")?;
                sub.closed = true;
                cur = first;
                ctrl = None;
                continue;
            }
        } else if cmd == b' ' || matches!(cmd, b'Z' | b'z') {
            return Err(format!("unexpected {:?}", c as char));
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { cur } else { pos2(0.0, 0.0) };
        let mut next_ctrl = None;
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let p = base + scan.pair()?;
                subs.push(Sub {
                    start: p,
                    segs: Vec::new(),
                    closed: false,
                });
                cur = p;
                first = p;
                // More pairs after a move are lines.
                cmd = if rel { b'l' } else { b'L' };
            }
            b'L' => {
                let p = base + scan.pair()?;
                push(&mut subs, Seg::Line(p))?;
                cur = p;
            }
            b'H' => {
                let x = scan.number()?;
                let p = pos2(if rel { cur.x + x } else { x }, cur.y);
                push(&mut subs, Seg::Line(p))?;
                cur = p;
            }
            b'V' => {
                let y = scan.number()?;
                let p = pos2(cur.x, if rel { cur.y + y } else { y });
                push(&mut subs, Seg::Line(p))?;
                cur = p;
            }
            b'C' => {
                let a = base + scan.pair()?;
                let b = base + scan.pair()?;
                let p = base + scan.pair()?;
                push(&mut subs, Seg::Cubic(a, b, p))?;
                next_ctrl = Some(b);
                cur = p;
            }
            b'S' => {
                let a = match ctrl {
                    Some(c) => cur + (cur - c),
                    None => cur,
                };
                let b = base + scan.pair()?;
                let p = base + scan.pair()?;
                push(&mut subs, Seg::Cubic(a, b, p))?;
                next_ctrl = Some(b);
                cur = p;
            }
            b'Q' => {
                let q = base + scan.pair()?;
                let p = base + scan.pair()?;
                let a = cur + (q - cur) * (2.0 / 3.0);
                let b = p + (q - p) * (2.0 / 3.0);
                push(&mut subs, Seg::Cubic(a, b, p))?;
                cur = p;
            }
            b'A' => {
                let rx = scan.number()?;
                let ry = scan.number()?;
                let turn = scan.number()?;
                let large = scan.number()? != 0.0;
                let sweep = scan.number()? != 0.0;
                let p = base + scan.pair()?;
                for seg in arc(cur, rx, ry, turn, large, sweep, p) {
                    push(&mut subs, seg)?;
                }
                cur = p;
            }
            other => return Err(format!("unknown command {:?}", other as char)),
        }
        ctrl = next_ctrl;
    }
    Ok(subs)
}

fn push(subs: &mut [Sub], seg: Seg) -> Result<(), String> {
    subs.last_mut().ok_or("a segment before M")?.segs.push(seg);
    Ok(())
}

struct Scan<'a> {
    text: &'a [u8],
    at: usize,
}

impl Scan<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    fn skip_separators(&mut self) {
        while matches!(self.peek(), Some(b' ' | b',' | b'\n' | b'\t')) {
            self.at += 1;
        }
    }

    fn number(&mut self) -> Result<f32, String> {
        self.skip_separators();
        let start = self.at;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.at += 1;
        }
        let mut dot = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.at += 1;
            } else if c == b'.' && !dot {
                dot = true;
                self.at += 1;
            } else {
                break;
            }
        }
        std::str::from_utf8(&self.text[start..self.at])
            .ok()
            .and_then(|t| t.parse().ok())
            .ok_or_else(|| format!("a number was expected at {start}"))
    }

    fn pair(&mut self) -> Result<Vec2, String> {
        Ok(vec2(self.number()?, self.number()?))
    }
}

/// An SVG elliptical arc from `from` to `to`, as cubic Béziers of at most a
/// quarter turn each (SVG's implementation notes, F.6.5).
fn arc(
    from: Pos2,
    rx: f32,
    ry: f32,
    turn_degrees: f32,
    large: bool,
    sweep: bool,
    to: Pos2,
) -> Vec<Seg> {
    if from == to {
        return Vec::new();
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 {
        return vec![Seg::Line(to)];
    }
    let (sin, cos) = turn_degrees.to_radians().sin_cos();
    let half = (from - to) / 2.0;
    let x1 = cos * half.x + sin * half.y;
    let y1 = -sin * half.x + cos * half.y;
    // Radii too small to reach are scaled up until they do.
    let scale = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if scale > 1.0 {
        rx *= scale.sqrt();
        ry *= scale.sqrt();
    }
    let sign = if large == sweep { -1.0 } else { 1.0 };
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let k = sign * (num / den).max(0.0).sqrt();
    let (cx1, cy1) = (k * rx * y1 / ry, -k * ry * x1 / rx);
    let center = pos2(
        cos * cx1 - sin * cy1 + (from.x + to.x) / 2.0,
        sin * cx1 + cos * cy1 + (from.y + to.y) / 2.0,
    );
    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let start = angle(1.0, 0.0, (x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut delta = angle(
        (x1 - cx1) / rx,
        (y1 - cy1) / ry,
        (-x1 - cx1) / rx,
        (-y1 - cy1) / ry,
    );
    use std::f32::consts::TAU;
    if !sweep && delta > 0.0 {
        delta -= TAU;
    } else if sweep && delta < 0.0 {
        delta += TAU;
    }
    let pieces = (delta.abs() / std::f32::consts::FRAC_PI_2 - 1e-4)
        .ceil()
        .max(1.0);
    let step = delta / pieces;
    let handle = 4.0 / 3.0 * (step / 4.0).tan();
    let at = |a: f32| {
        let (s, c) = a.sin_cos();
        // The point on the unit circle, then onto the ellipse.
        let (x, y) = (rx * c, ry * s);
        pos2(cos * x - sin * y + center.x, sin * x + cos * y + center.y)
    };
    let slope = |a: f32| {
        let (s, c) = a.sin_cos();
        let (x, y) = (-rx * s, ry * c);
        vec2(cos * x - sin * y, sin * x + cos * y)
    };
    (0..pieces as usize)
        .map(|i| {
            let (a, b) = (start + step * i as f32, start + step * (i + 1) as f32);
            let end = if i + 1 == pieces as usize { to } else { at(b) };
            Seg::Cubic(at(a) + slope(a) * handle, end - slope(b) * handle, end)
        })
        .collect()
}

/// Triangles (as indices into `poly`) that fill a simple polygon. Convex
/// polygons are a fan; others are cut into ears.
pub(super) fn triangulate(poly: &[Pos2]) -> Vec<u32> {
    let n = poly.len();
    if n < 3 {
        return Vec::new();
    }
    let cross = |a: Pos2, b: Pos2, c: Pos2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let area: f32 = (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let convex =
        (0..n).all(|i| cross(poly[i], poly[(i + 1) % n], poly[(i + 2) % n]) * sign >= -1e-3);
    if convex {
        return (1..n as u32 - 1).flat_map(|i| [0, i, i + 1]).collect();
    }
    let mut rest: Vec<usize> = (0..n).collect();
    let mut out = Vec::new();
    while rest.len() > 3 {
        let m = rest.len();
        let ear = (0..m).find(|&i| {
            let (a, b, c) = (rest[(i + m - 1) % m], rest[i], rest[(i + 1) % m]);
            if cross(poly[a], poly[b], poly[c]) * sign <= 1e-6 {
                return false;
            }
            // No other point inside the ear.
            rest.iter().all(|&k| {
                if k == a || k == b || k == c {
                    return true;
                }
                let p = poly[k];
                let (d1, d2, d3) = (
                    cross(poly[a], poly[b], p) * sign,
                    cross(poly[b], poly[c], p) * sign,
                    cross(poly[c], poly[a], p) * sign,
                );
                !(d1 >= 0.0 && d2 >= 0.0 && d3 >= 0.0)
            })
        });
        match ear {
            Some(i) => {
                let (a, b, c) = (rest[(i + m - 1) % m], rest[i], rest[(i + 1) % m]);
                out.extend([a as u32, b as u32, c as u32]);
                rest.remove(i);
            }
            None => {
                // A degenerate polygon: what is left as a fan.
                for k in 1..rest.len() - 1 {
                    out.extend([rest[0] as u32, rest[k] as u32, rest[k + 1] as u32]);
                }
                return out;
            }
        }
    }
    out.extend(rest.iter().map(|&i| i as u32));
    out
}

/// A polyline with its sharp corners rounded by arcs of `radius` pixels
/// (as much as the pieces beside them allow). Corners that turn less than
/// about 25°, as the points of a flattened curve do, are left as they are.
pub(super) fn round_corners(points: &[Pos2], closed: bool, radius: f32) -> Vec<Pos2> {
    let n = points.len();
    if n < 3 || radius < 0.3 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        if !closed && (i == 0 || i + 1 == n) {
            out.push(points[i]);
            continue;
        }
        let cur = points[i];
        let (to_prev, to_next) = (points[(i + n - 1) % n] - cur, points[(i + 1) % n] - cur);
        let (len_prev, len_next) = (to_prev.length(), to_next.length());
        let (a, b) = (to_prev / len_prev, to_next / len_next);
        // Cosine of the angle between the two pieces: -1 for a straight line.
        let cos = a.dot(b);
        // (Nor where the path turns right back on itself.)
        if !(-0.9..=0.999).contains(&cos) || len_prev < 1e-3 || len_next < 1e-3 {
            out.push(cur);
            continue;
        }
        let half = cos.clamp(-1.0, 1.0).acos() / 2.0;
        // How far from the corner the arc starts, and the radius that gives.
        let reach = (radius / half.tan())
            .min(len_prev * 0.5)
            .min(len_next * 0.5);
        let r = reach * half.tan();
        let center = cur + (a + b).normalized() * (r / half.sin());
        let (from, to) = (cur + a * reach - center, cur + b * reach - center);
        // The way round the short side.
        let sweep = (from.x * to.y - from.y * to.x).atan2(from.dot(to));
        let steps = (sweep.abs() / 0.3).ceil().max(2.0) as usize;
        let start = from.y.atan2(from.x);
        for k in 0..=steps {
            let angle = start + sweep * k as f32 / steps as f32;
            out.push(center + vec2(angle.cos(), angle.sin()) * r);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(p: Pos2) -> Pos2 {
        p
    }

    #[test]
    fn reads_lines_and_relative_moves() {
        let subs = parse("M1 2h3v4l-3 0z");
        assert_eq!(subs.len(), 1);
        assert!(subs[0].closed);
        assert_eq!(
            subs[0].flatten(&same),
            [
                pos2(1.0, 2.0),
                pos2(4.0, 2.0),
                pos2(4.0, 6.0),
                pos2(1.0, 6.0)
            ]
        );
    }

    #[test]
    fn numbers_may_run_together() {
        // "-" starts a new number, and so does a second ".".
        let subs = parse("M1.5-2.5L.5.25");
        assert_eq!(subs[0].flatten(&same), [pos2(1.5, -2.5), pos2(0.5, 0.25)]);
    }

    #[test]
    fn an_arc_stays_on_its_circle() {
        // The upper half of the circle round (5, 0): sweep 1 runs clockwise.
        let points = parse("M0 0A5 5 0 0 1 10 0")[0].flatten(&same);
        for p in &points {
            let off = (*p - pos2(5.0, 0.0)).length() - 5.0;
            assert!(off.abs() < 0.05, "{p:?} is {off} off the circle");
        }
        assert!(points.iter().any(|p| p.y < -4.9));
        assert!(points.iter().all(|p| p.y <= 1e-3));
    }

    #[test]
    fn a_smooth_curve_continues_the_last_one() {
        // S reflects the previous control point: a straight run here.
        let points = parse("M0 0C1 0 2 0 3 0S5 0 6 0")[0].flatten(&same);
        assert!(points.iter().all(|p| p.y.abs() < 1e-4));
        assert!((points[points.len() - 1].x - 6.0).abs() < 1e-4);
    }

    #[test]
    fn fills_a_concave_polygon_exactly() {
        // An L: 3 by 3 with a 2 by 2 notch, area 5.
        let l = [
            pos2(0.0, 0.0),
            pos2(3.0, 0.0),
            pos2(3.0, 1.0),
            pos2(1.0, 1.0),
            pos2(1.0, 3.0),
            pos2(0.0, 3.0),
        ];
        let triangles = triangulate(&l);
        assert_eq!(triangles.len(), 3 * (l.len() - 2));
        let area: f32 = triangles
            .chunks(3)
            .map(|t| {
                let (a, b, c) = (l[t[0] as usize], l[t[1] as usize], l[t[2] as usize]);
                ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs() / 2.0
            })
            .sum();
        assert!((area - 5.0).abs() < 1e-4, "{area}");
    }

    #[test]
    fn rounds_sharp_corners_only() {
        let square = [
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(10.0, 10.0),
            pos2(0.0, 10.0),
        ];
        let rounded = round_corners(&square, true, 2.0);
        assert!(rounded.len() > square.len());
        // The corner itself is cut off, the arcs stay inside the square.
        assert!(!rounded.contains(&pos2(10.0, 0.0)));
        for p in &rounded {
            let inside = |v: f32| (-1e-3..=10.0 + 1e-3).contains(&v);
            assert!(inside(p.x) && inside(p.y), "{p:?} left the square");
        }
        // A nearly straight run is left alone.
        let bent = [pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(20.0, 1.0)];
        assert_eq!(round_corners(&bent, false, 2.0), bent);
    }
}
