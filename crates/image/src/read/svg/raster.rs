//! Shapes into pixels: polygons filled by a fill rule into a coverage mask
//! (each pixel's covered part, its edges smooth: eight lines of samples a
//! pixel, each covering exactly across), and strokes made polygons — each
//! segment's band, the joins (miter, round, bevel) and the caps (butt,
//! round, square), dashes first — filled together.

use super::path::{P, p};

/// A rectangle of pixels: from (`x0`, `y0`), `w` × `h`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Area {
    pub x0: i32,
    pub y0: i32,
    pub w: usize,
    pub h: usize,
}

impl Area {
    pub fn x1(&self) -> i32 {
        self.x0 + self.w as i32
    }

    pub fn y1(&self) -> i32 {
        self.y0 + self.h as i32
    }

    /// The pixels both cover.
    pub fn meet(&self, o: &Area) -> Option<Area> {
        let x0 = self.x0.max(o.x0);
        let y0 = self.y0.max(o.y0);
        let x1 = self.x1().min(o.x1());
        let y1 = self.y1().min(o.y1());
        (x1 > x0 && y1 > y0).then(|| Area {
            x0,
            y0,
            w: (x1 - x0) as usize,
            h: (y1 - y0) as usize,
        })
    }

    /// The pixels a rectangle (`x0`, `y0`)–(`x1`, `y1`) touches.
    pub fn around(x0: f64, y0: f64, x1: f64, y1: f64) -> Option<Area> {
        if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
            return None;
        }
        let lim = |v: f64| v.clamp(-1e7, 1e7);
        let (ax, ay) = (lim(x0).floor() as i32, lim(y0).floor() as i32);
        let (bx, by) = (lim(x1).ceil() as i32, lim(y1).ceil() as i32);
        (bx > ax && by > ay).then(|| Area {
            x0: ax,
            y0: ay,
            w: (bx - ax) as usize,
            h: (by - ay) as usize,
        })
    }
}

/// How much of each pixel of `area` a shape covers (0–1).
#[derive(Clone, Debug)]
pub(super) struct Mask {
    pub area: Area,
    pub cov: Vec<f32>,
}

impl Mask {
    /// The coverage at pixel (`x`, `y`) (0 outside).
    pub fn at(&self, x: i32, y: i32) -> f32 {
        let a = &self.area;
        if x < a.x0 || y < a.y0 || x >= a.x1() || y >= a.y1() {
            return 0.0;
        }
        self.cov[(y - a.y0) as usize * a.w + (x - a.x0) as usize]
    }
}

/// Lines of samples a pixel row.
const SUB: usize = 8;

#[derive(Clone, Copy, Debug)]
struct Edge {
    x0: f64,
    y0: f64,
    y1: f64,
    dxdy: f64,
    /// +1 going down, −1 going up.
    dir: i32,
}

/// The polygons `polys` (each closed by itself) filled — by the even-odd
/// rule, or the nonzero one — within `clip`; `None` when nothing of them
/// is in it.
pub(super) fn fill(polys: &[Vec<P>], evenodd: bool, clip: &Area) -> Option<Mask> {
    let mut edges: Vec<Edge> = Vec::new();
    let (mut minx, mut miny, mut maxx, mut maxy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for poly in polys {
        let n = poly.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
                continue;
            }
            minx = minx.min(a.x);
            maxx = maxx.max(a.x);
            miny = miny.min(a.y);
            maxy = maxy.max(a.y);
            if a.y == b.y {
                continue;
            }
            let (top, bottom, dir) = if a.y < b.y { (a, b, 1) } else { (b, a, -1) };
            edges.push(Edge {
                x0: top.x,
                y0: top.y,
                y1: bottom.y,
                dxdy: (bottom.x - top.x) / (bottom.y - top.y),
                dir,
            });
        }
    }
    if edges.is_empty() {
        return None;
    }
    let area = Area::around(minx, miny, maxx, maxy)?.meet(clip)?;
    edges.sort_by(|a, b| a.y0.total_cmp(&b.y0));
    let (w, h) = (area.w, area.h);
    let mut cov = vec![0.0f32; w * h];
    let mut delta = vec![0.0f32; w + 1];
    let mut row = vec![0.0f32; w];
    let mut active: Vec<usize> = Vec::new();
    let mut next = 0usize;
    let mut xs: Vec<(f64, i32)> = Vec::new();
    let weight = 1.0 / SUB as f32;
    let left = f64::from(area.x0);
    let right = f64::from(area.x1());
    for ry in 0..h {
        let y = f64::from(area.y0) + ry as f64;
        // The edges this row meets.
        while next < edges.len() && edges[next].y0 < y + 1.0 {
            active.push(next);
            next += 1;
        }
        active.retain(|&i| edges[i].y1 > y);
        if active.is_empty() {
            continue;
        }
        row.fill(0.0);
        delta.fill(0.0);
        let mut any = false;
        for k in 0..SUB {
            let sy = y + (k as f64 + 0.5) / SUB as f64;
            xs.clear();
            for &i in &active {
                let e = &edges[i];
                if e.y0 <= sy && sy < e.y1 {
                    xs.push((e.x0 + (sy - e.y0) * e.dxdy, e.dir));
                }
            }
            if xs.len() < 2 {
                continue;
            }
            xs.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut wind = 0i32;
            for j in 0..xs.len() - 1 {
                wind += xs[j].1;
                let inside = if evenodd { wind & 1 != 0 } else { wind != 0 };
                if !inside {
                    continue;
                }
                let xa = xs[j].0.clamp(left, right) - left;
                let xb = xs[j + 1].0.clamp(left, right) - left;
                if xb <= xa {
                    continue;
                }
                any = true;
                span(&mut row, &mut delta, xa, xb, weight);
            }
        }
        if !any {
            continue;
        }
        let out = &mut cov[ry * w..(ry + 1) * w];
        let mut run = 0.0f32;
        for x in 0..w {
            run += delta[x];
            out[x] = (row[x] + run).min(1.0);
        }
    }
    Some(Mask { area, cov })
}

/// Adds `c` × the part of each pixel the span `xa`–`xb` covers (its whole
/// pixels through `delta`, summed along the row after).
fn span(row: &mut [f32], delta: &mut [f32], xa: f64, xb: f64, c: f32) {
    let w = row.len();
    let ia = (xa.floor() as usize).min(w - 1);
    let ib = (xb.floor() as usize).min(w);
    if ia == ib || (ib == w && ia == w - 1 && xb >= w as f64 && xa.floor() as usize == w - 1) {
        let end = xb.min(ia as f64 + 1.0);
        row[ia] += c * (end - xa) as f32;
        return;
    }
    row[ia] += c * ((ia + 1) as f64 - xa) as f32;
    delta[ia + 1] += c;
    delta[ib] -= c;
    if ib < w {
        row[ib] += c * (xb - ib as f64) as f32;
    }
}

/// How a stroke's segments meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Join {
    Miter,
    Round,
    Bevel,
}

/// How an open stroke ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Cap {
    Butt,
    Round,
    Square,
}

/// A stroke's look (in the lines' units).
#[derive(Clone, Copy, Debug)]
pub(super) struct Pen {
    pub width: f64,
    pub join: Join,
    pub cap: Cap,
    pub miter_limit: f64,
    /// How far rounds may stray from circles.
    pub tol: f64,
}

/// `pts` turned so that their polygon goes the positive way: polygons all
/// going one way fill as their union by the nonzero rule.
fn positive(mut pts: Vec<P>) -> Vec<P> {
    let n = pts.len();
    let area: f64 = (0..n).map(|i| pts[i].cross(pts[(i + 1) % n])).sum();
    if area < 0.0 {
        pts.reverse();
    }
    pts
}

/// A circle as a polygon within `tol`.
fn disk(c: P, r: f64, tol: f64) -> Vec<P> {
    let n = ((std::f64::consts::PI / (1.0 - tol.min(r * 0.5) / r).acos().max(1e-3)).ceil()
        as usize)
        .clamp(8, 256);
    (0..n)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / n as f64;
            c + p(t.cos(), t.sin()) * r
        })
        .collect()
}

/// The polygons that stroke the lines `lines` (each with whether it is
/// closed) with `pen`, all going the positive way.
pub(super) fn stroke(lines: &[(Vec<P>, bool)], pen: &Pen) -> Vec<Vec<P>> {
    let hw = pen.width / 2.0;
    let mut out: Vec<Vec<P>> = Vec::new();
    if hw <= 0.0 {
        return out;
    }
    for (pts, closed) in lines {
        // Points that repeat give no direction.
        let mut v: Vec<P> = Vec::with_capacity(pts.len());
        for &q in pts {
            if v.last().is_none_or(|l: &P| (*l - q).len() > 1e-9) {
                v.push(q);
            }
        }
        let closed = *closed && v.len() > 2;
        if closed && (v[0] - v[v.len() - 1]).len() <= 1e-9 {
            v.pop();
        }
        if v.len() == 1 {
            // A single point: a round or square cap draws a dot.
            match pen.cap {
                Cap::Round => out.push(positive(disk(v[0], hw, pen.tol))),
                Cap::Square => {
                    let c = v[0];
                    out.push(positive(vec![
                        c + p(-hw, -hw),
                        c + p(hw, -hw),
                        c + p(hw, hw),
                        c + p(-hw, hw),
                    ]));
                }
                Cap::Butt => {}
            }
            continue;
        }
        let n = v.len();
        let segs = if closed { n } else { n - 1 };
        for i in 0..segs {
            let a = v[i];
            let b = v[(i + 1) % n];
            let d = (b - a).unit();
            let nrm = d.perp() * hw;
            let (mut a2, mut b2) = (a, b);
            if !closed && pen.cap == Cap::Square {
                if i == 0 {
                    a2 = a - d * hw;
                }
                if i == segs - 1 {
                    b2 = b + d * hw;
                }
            }
            out.push(positive(vec![a2 + nrm, b2 + nrm, b2 - nrm, a2 - nrm]));
        }
        // Joins: at every inner point, and all round a closed line.
        let joints: Vec<usize> = if closed {
            (0..n).collect()
        } else {
            (1..n - 1).collect()
        };
        for i in joints {
            let prev = v[(i + n - 1) % n];
            let at = v[i];
            let next = v[(i + 1) % n];
            let d1 = (at - prev).unit();
            let d2 = (next - at).unit();
            let turn = d1.cross(d2);
            if turn.abs() < 1e-12 && d1.dot(d2) > 0.0 {
                continue;
            }
            match pen.join {
                Join::Round => out.push(positive(disk(at, hw, pen.tol))),
                Join::Miter | Join::Bevel => {
                    // The outer side: away from where the line turns.
                    let side = if turn > 0.0 { -1.0 } else { 1.0 };
                    let n1 = d1.perp() * (hw * side);
                    let n2 = d2.perp() * (hw * side);
                    let bevel = vec![at, at + n1, at + n2];
                    let mut mitered = false;
                    if pen.join == Join::Miter {
                        let m = (n1 + n2).unit();
                        let cos_half = m.dot(n1.unit());
                        if cos_half > 1e-6 && 1.0 / cos_half <= pen.miter_limit {
                            let tip = at + m * (hw / cos_half);
                            out.push(positive(vec![at, at + n1, tip, at + n2]));
                            mitered = true;
                        }
                    }
                    if !mitered {
                        out.push(positive(bevel));
                    }
                }
            }
        }
        if !closed && pen.cap == Cap::Round {
            out.push(positive(disk(v[0], hw, pen.tol)));
            out.push(positive(disk(v[n - 1], hw, pen.tol)));
        }
    }
    out
}

/// The lines cut into dashes: `array`'s lengths on and off in turn (an odd
/// count of them twice), from `offset` into it.
pub(super) fn dash(lines: &[(Vec<P>, bool)], array: &[f64], offset: f64) -> Vec<(Vec<P>, bool)> {
    let mut pattern: Vec<f64> = array.iter().map(|v| v.max(0.0)).collect();
    if pattern.len() % 2 == 1 {
        pattern.extend_from_within(..);
    }
    let total: f64 = pattern.iter().sum();
    if pattern.is_empty() || total <= 1e-9 {
        return lines.to_vec();
    }
    let mut out = Vec::new();
    for (pts, closed) in lines {
        let mut v = pts.clone();
        if *closed && v.len() > 1 {
            v.push(v[0]);
        }
        // Where in the pattern the line starts.
        let mut idx = 0usize;
        let mut left = offset.rem_euclid(total);
        while left >= pattern[idx] {
            left -= pattern[idx];
            idx = (idx + 1) % pattern.len();
        }
        let mut remaining = pattern[idx] - left;
        let mut on = idx.is_multiple_of(2);
        let mut cur: Vec<P> = if on { vec![v[0]] } else { Vec::new() };
        for w in v.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = (b - a).len();
            let mut pos = 0.0;
            while len - pos > remaining {
                pos += remaining;
                let q = a + (b - a) * (pos / len);
                if on {
                    cur.push(q);
                    out.push((std::mem::take(&mut cur), false));
                } else {
                    cur = vec![q];
                }
                on = !on;
                idx = (idx + 1) % pattern.len();
                remaining = pattern[idx];
            }
            remaining -= len - pos;
            if on {
                cur.push(b);
            }
        }
        if on && cur.len() > 1 {
            out.push((cur, false));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<P> {
        vec![p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)]
    }

    const CLIP: Area = Area {
        x0: 0,
        y0: 0,
        w: 20,
        h: 20,
    };

    #[test]
    fn a_square_covers_its_pixels_and_parts_of_its_edges() {
        let m = fill(&[square(2.0, 2.0, 6.0, 6.0)], false, &CLIP).expect("inside");
        assert_eq!(m.at(3, 3), 1.0);
        assert_eq!(m.at(1, 3), 0.0);
        assert_eq!(m.at(6, 3), 0.0);
        // Half a pixel.
        let m = fill(&[square(2.5, 2.0, 6.0, 6.0)], false, &CLIP).expect("inside");
        assert!((m.at(2, 3) - 0.5).abs() < 1e-6, "{}", m.at(2, 3));
        // A quarter, in both directions.
        let m = fill(&[square(2.5, 2.5, 6.0, 6.0)], false, &CLIP).expect("inside");
        assert!((m.at(2, 2) - 0.25).abs() < 1e-6, "{}", m.at(2, 2));
        // Outside the clip: nothing.
        assert!(fill(&[square(30.0, 30.0, 40.0, 40.0)], false, &CLIP).is_none());
    }

    #[test]
    fn the_fill_rules() {
        // A square inside another, going the same way: nonzero fills the
        // hole, even-odd leaves it.
        let polys = [square(0.0, 0.0, 10.0, 10.0), square(3.0, 3.0, 7.0, 7.0)];
        assert_eq!(fill(&polys, false, &CLIP).unwrap().at(5, 5), 1.0);
        assert_eq!(fill(&polys, true, &CLIP).unwrap().at(5, 5), 0.0);
        assert_eq!(fill(&polys, true, &CLIP).unwrap().at(1, 1), 1.0);
    }

    #[test]
    fn a_stroke_covers_its_width_and_its_corners() {
        let pen = Pen {
            width: 2.0,
            join: Join::Miter,
            cap: Cap::Butt,
            miter_limit: 4.0,
            tol: 0.1,
        };
        let line = vec![(square(4.0, 4.0, 14.0, 14.0), true)];
        let polys = stroke(&line, &pen);
        let m = fill(&polys, false, &CLIP).expect("inside");
        // On the line, 1 either side; the corner mitered square.
        assert_eq!(m.at(8, 3), 1.0);
        assert_eq!(m.at(8, 4), 1.0);
        assert_eq!(m.at(8, 5), 0.0);
        assert_eq!(m.at(3, 3), 1.0, "the corner's miter");
        // Beveled, the corner's outer pixel is half covered.
        let bevel = Pen {
            join: Join::Bevel,
            ..pen
        };
        let m = fill(&stroke(&line, &bevel), false, &CLIP).expect("inside");
        assert!(m.at(3, 3) > 0.3 && m.at(3, 3) < 0.7, "{}", m.at(3, 3));
    }

    #[test]
    fn dashes_cut_a_line() {
        let line = vec![(vec![p(0.0, 0.0), p(10.0, 0.0)], false)];
        let d = dash(&line, &[2.0, 3.0], 0.0);
        let starts: Vec<f64> = d.iter().map(|(v, _)| v[0].x).collect();
        assert_eq!(starts, vec![0.0, 5.0]);
        assert!((d[0].0.last().unwrap().x - 2.0).abs() < 1e-9);
        // An offset moves into the pattern.
        let d = dash(&line, &[2.0, 3.0], 1.0);
        assert!((d[0].0.last().unwrap().x - 1.0).abs() < 1e-9);
    }
}
