//! Text drawn into pictures: Arimo Bold's glyphs ([`crate::arimo`]) laid
//! along a baseline, and how much of each pixel they cover: their
//! quadratic outlines flattened into lines, the area each line leaves on
//! the pixels it crosses accumulated and summed along each row (the
//! accumulation rasterizer: exact for outlines that do not overlap, as a
//! glyph's do not).

use crate::arimo::{GLYPHS, Glyph, UNITS};

fn glyph(ch: char) -> Option<&'static Glyph> {
    GLYPHS.iter().find(|g| g.ch == ch)
}

/// How far the pen moves across `text`, in ems (a character the font
/// lacks moves it as a digit does).
pub(crate) fn advance(text: &str) -> f64 {
    let digit = glyph('0').map_or(0.0, |g| g.advance as f64);
    text.chars()
        .map(|c| glyph(c).map_or(digit, |g| g.advance as f64))
        .sum::<f64>()
        / UNITS
}

/// How much of each pixel of a `w` × `h` picture (rows from the top) text
/// covers, 0–1.
#[derive(Debug)]
pub(crate) struct Coverage {
    pub w: usize,
    pub h: usize,
    acc: Vec<f32>,
}

impl Coverage {
    pub fn new(w: usize, h: usize) -> Coverage {
        Coverage {
            w,
            h,
            acc: vec![0.0; w * h + 2],
        }
    }

    /// Adds `text`: its pen starting at `origin` (pixels: the baseline's
    /// left end, y down), `em` pixels to the em across and up.
    pub fn text(&mut self, text: &str, origin: [f64; 2], em: [f64; 2]) {
        let k = [em[0] / UNITS, em[1] / UNITS];
        let mut x = origin[0];
        let digit = glyph('0').map_or(0.0, |g| g.advance as f64);
        for c in text.chars() {
            let Some(g) = glyph(c) else {
                x += digit * k[0];
                continue;
            };
            let place =
                |p: (i16, i16, bool)| [x + p.0 as f64 * k[0], origin[1] - p.1 as f64 * k[1]];
            let mut start = 0;
            for &end in g.ends {
                let contour: Vec<([f64; 2], bool)> = g.points[start..=end as usize]
                    .iter()
                    .map(|&p| (place(p), p.2))
                    .collect();
                self.contour(&contour);
                start = end as usize + 1;
            }
            x += g.advance as f64 * k[0];
        }
    }

    /// A closed contour of TrueType points: on the outline, or quadratic
    /// control points (two in a row imply the on-curve point between).
    fn contour(&mut self, pts: &[([f64; 2], bool)]) {
        let n = pts.len();
        if n < 2 {
            return;
        }
        let mid = |a: [f64; 2], b: [f64; 2]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        // Start on the outline.
        let first = pts.iter().position(|p| p.1);
        let (start, rest): ([f64; 2], Vec<([f64; 2], bool)>) = match first {
            Some(i) => (
                pts[i].0,
                pts[i + 1..].iter().chain(&pts[..i]).copied().collect(),
            ),
            None => (
                mid(pts[0].0, pts[1].0),
                pts[1..].iter().chain(&pts[..1]).copied().collect(),
            ),
        };
        let mut cur = start;
        let mut ctrl: Option<[f64; 2]> = None;
        for (p, on) in rest.into_iter().chain(std::iter::once((start, true))) {
            match (on, ctrl) {
                (true, None) => {
                    self.line(cur, p);
                    cur = p;
                }
                (true, Some(c)) => {
                    self.curve(cur, c, p);
                    cur = p;
                    ctrl = None;
                }
                (false, None) => ctrl = Some(p),
                (false, Some(c)) => {
                    let m = mid(c, p);
                    self.curve(cur, c, m);
                    cur = m;
                    ctrl = Some(p);
                }
            }
        }
    }

    /// A straight piece of an outline (pixels, y down).
    pub fn segment(&mut self, p0: [f64; 2], p1: [f64; 2]) {
        self.line(p0, p1);
    }

    /// A quadratic piece of an outline.
    pub fn quad(&mut self, p0: [f64; 2], c: [f64; 2], p1: [f64; 2]) {
        self.curve(p0, c, p1);
    }

    /// A cubic piece of an outline (CFF fonts'), as lines no farther than
    /// about a fifth of a pixel from it.
    pub fn cubic(&mut self, p0: [f64; 2], c0: [f64; 2], c1: [f64; 2], p1: [f64; 2]) {
        let bend = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
            (a[0] - 2.0 * b[0] + c[0]).abs() + (a[1] - 2.0 * b[1] + c[1]).abs()
        };
        let dd = bend(p0, c0, c1).max(bend(c0, c1, p1));
        let n = ((dd * 0.75 / 0.2).sqrt().ceil() as usize).clamp(1, 96);
        let mut prev = p0;
        for i in 1..=n {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            let q = [
                a * p0[0] + b * c0[0] + c * c1[0] + d * p1[0],
                a * p0[1] + b * c0[1] + c * c1[1] + d * p1[1],
            ];
            self.line(prev, q);
            prev = q;
        }
    }

    /// A quadratic curve, as lines no farther than a fifth of a pixel from
    /// it.
    fn curve(&mut self, p0: [f64; 2], c: [f64; 2], p1: [f64; 2]) {
        let dd = (p0[0] - 2.0 * c[0] + p1[0]).abs() + (p0[1] - 2.0 * c[1] + p1[1]).abs();
        let n = ((dd / 0.2).sqrt().ceil() as usize).clamp(1, 64);
        let mut prev = p0;
        for i in 1..=n {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            let q = [
                u * u * p0[0] + 2.0 * t * u * c[0] + t * t * p1[0],
                u * u * p0[1] + 2.0 * t * u * c[1] + t * t * p1[1],
            ];
            self.line(prev, q);
            prev = q;
        }
    }

    /// A line of the outline: the area it leaves on each pixel of the rows
    /// it crosses, signed by its direction (summed along the row later).
    fn line(&mut self, p0: [f64; 2], p1: [f64; 2]) {
        let ([x0, y0], [x1, y1]) = (p0, p1);
        if y0 == y1 {
            return;
        }
        let (dir, x0, y0, x1, y1) = if y0 < y1 {
            (1.0, x0, y0, x1, y1)
        } else {
            (-1.0, x1, y1, x0, y0)
        };
        let w = self.w as f64;
        let dxdy = (x1 - x0) / (y1 - y0);
        let mut x = x0;
        if y0 < 0.0 {
            x -= y0 * dxdy;
        }
        let first = y0.max(0.0) as usize;
        let last = (y1.ceil().max(0.0) as usize).min(self.h);
        for y in first..last {
            let row = y * self.w;
            let dy = (y as f64 + 1.0).min(y1) - (y as f64).max(y0);
            let xnext = x + dxdy * dy;
            let d = (dy * dir) as f32;
            let (a, b) = if x < xnext { (x, xnext) } else { (xnext, x) };
            let a = a.clamp(0.0, w - 1e-6);
            let b = b.clamp(0.0, w - 1e-6);
            let fa = a.floor();
            let ia = fa as usize;
            let cb = b.ceil();
            let ib = cb as usize;
            // What a piece leaves past the right edge belongs to no pixel
            // of this row (and must not reach the next row's first).
            if ib <= ia + 1 {
                let xm = (0.5 * (a + b) - fa) as f32;
                self.acc[row + ia] += d - d * xm;
                if ia + 1 < self.w {
                    self.acc[row + ia + 1] += d * xm;
                }
            } else {
                let s = 1.0 / (b - a);
                let fx = a - fa;
                let a0 = 0.5 * s * (1.0 - fx) * (1.0 - fx);
                let fx1 = b - cb + 1.0;
                let am = 0.5 * s * fx1 * fx1;
                self.acc[row + ia] += d * a0 as f32;
                if ib == ia + 2 {
                    self.acc[row + ia + 1] += d * (1.0 - a0 - am) as f32;
                } else {
                    let a1 = s * (1.5 - fx);
                    self.acc[row + ia + 1] += d * (a1 - a0) as f32;
                    for xi in ia + 2..ib - 1 {
                        self.acc[row + xi] += d * s as f32;
                    }
                    let a2 = a1 + (ib - ia - 3) as f64 * s;
                    self.acc[row + ib - 1] += d * (1.0 - a2 - am) as f32;
                }
                if ib < self.w {
                    self.acc[row + ib] += d * am as f32;
                }
            }
            x = xnext;
        }
    }

    /// Each pixel's coverage, rows from the top.
    pub fn finish(self) -> Vec<f32> {
        let mut out = vec![0.0f32; self.w * self.h];
        for y in 0..self.h {
            let mut sum = 0.0f32;
            for x in 0..self.w {
                sum += self.acc[y * self.w + x];
                out[y * self.w + x] = sum.abs().min(1.0);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_square_covers_its_pixels_and_halves_at_its_edges() {
        let mut c = Coverage::new(8, 8);
        // A square from (1.5, 1.5) to (5.5, 5.5), counterclockwise as
        // TrueType's outer contours are clockwise in y up.
        c.contour(&[
            ([1.5, 1.5], true),
            ([5.5, 1.5], true),
            ([5.5, 5.5], true),
            ([1.5, 5.5], true),
        ]);
        let v = c.finish();
        let at = |x: usize, y: usize| v[y * 8 + x];
        assert!((at(3, 3) - 1.0).abs() < 1e-6);
        assert!((at(1, 3) - 0.5).abs() < 1e-6);
        assert!((at(5, 3) - 0.5).abs() < 1e-6);
        assert!((at(1, 1) - 0.25).abs() < 1e-6);
        assert_eq!(at(7, 7), 0.0);
        assert_eq!(at(0, 3), 0.0);
    }

    #[test]
    fn a_shape_past_the_right_edge_leaves_the_next_rows_alone() {
        let mut c = Coverage::new(4, 4);
        // A square from (1.5, 0.5) to (6.5, 2.5), half of it outside.
        c.contour(&[
            ([1.5, 0.5], true),
            ([6.5, 0.5], true),
            ([6.5, 2.5], true),
            ([1.5, 2.5], true),
        ]);
        let v = c.finish();
        let at = |x: usize, y: usize| v[y * 4 + x];
        assert!((at(3, 1) - 1.0).abs() < 1e-4);
        assert!((at(1, 1) - 0.5).abs() < 1e-6);
        assert_eq!(at(0, 1), 0.0);
        assert_eq!(at(0, 3), 0.0);
        assert!(at(3, 3) < 1e-6, "{}", at(3, 3));
    }

    #[test]
    fn digits_are_as_tall_and_wide_as_arimos() {
        // At 100 pixels to the em, a "0" is 0.71 em tall (with its
        // overshoot) and the pen moves 0.556 em.
        assert!((advance("0") - 1139.0 / 2048.0).abs() < 1e-12);
        assert!((advance("00:00") - (4.0 * 1139.0 + 682.0) / 2048.0).abs() < 1e-12);
        let mut c = Coverage::new(80, 120);
        c.text("0", [10.0, 100.0], [100.0, 100.0]);
        let v = c.finish();
        let rows: Vec<usize> = (0..120)
            .filter(|&y| (0..80).any(|x| v[y * 80 + x] > 0.5))
            .collect();
        let tall = rows.last().unwrap() - rows.first().unwrap() + 1;
        assert!((tall as f64 - 1450.0 / 20.48).abs() <= 1.5, "{tall}");
        // The ring's hole is empty, its stroke full.
        assert!(v[65 * 80 + 37] < 0.01, "{}", v[65 * 80 + 37]);
        assert!(v[65 * 80 + 20] > 0.99, "{}", v[65 * 80 + 20]);
    }
}
