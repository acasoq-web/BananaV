//! VEGAS's Color Gradient as its pictures have it: color points on an axis
//! through the center at the gradient's angle, each a distance along it
//! (parts of the frame's width and height), measured straight across the
//! frame (Linear), around the center in ellipses or in rectangles as wide
//! as the angle says (Elliptical, Rectangular); between two points their
//! colors mixed premultiplied, each byte rounded, all in double precision
//! step by step as VEGAS's are.
//!
//! The card draws it in single precision from the same points, measured
//! from the center: a level apart at most here and there.

/// The gradient's settings at a time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    /// 0 Linear, 1 Elliptical, 2 Rectangular.
    pub kind: u32,
    /// The axis's angle, radians (clockwise on screen, from the right).
    pub angle: f64,
    /// The center: parts of the frame's width and height from its top left.
    pub center: [f64; 2],
    /// Each color point's distance along the axis and its color (straight,
    /// single precision as VEGAS keeps them).
    pub points: Vec<(f64, [f32; 4])>,
}

/// How a pixel's distance is measured.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    /// Along `dir` from `origin`, half a million units back on it.
    Linear {
        origin: [f64; 2],
        dir: [f64; 2],
    },
    /// Around the center, across scaled by `k` (the axis's slope).
    Elliptical {
        k: f64,
    },
    Rectangular {
        k: f64,
    },
    /// One color everywhere (an ellipse or rectangle without a slope).
    Solid,
}

/// The gradient worked out for a picture.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Gradient {
    shape: Shape,
    center: [f64; 2],
    /// Each point's distance as the shape measures it, and its color.
    stops: Vec<(f64, [f32; 4])>,
    /// The solid color, premultiplied (0–255 in bytes, else 0–1).
    solid: [f32; 4],
    width: usize,
    height: usize,
    bytes: bool,
}

/// A byte from a float, as VEGAS's are: to the nearest, halves up.
fn byte(v: f32) -> u8 {
    ((f64::from(v) + 1.5e-8) + 6_755_399_441_055_744.0).to_bits() as u8
}

impl Gradient {
    /// The gradient of a picture `width` × `height` pixels, in bytes for an
    /// 8-bit project.
    pub(crate) fn new(s: &Settings, width: usize, height: usize, bytes: bool) -> Gradient {
        let (cos, sin) = (s.angle.cos(), s.angle.sin());
        let [cx, cy] = s.center;
        // The points on the axis (an ellipse's or a rectangle's distances
        // have no side).
        let unsigned = s.kind != 0;
        let placed: Vec<[f64; 2]> = s
            .points
            .iter()
            .map(|&(r, _)| {
                let r = if unsigned { r.abs() } else { r };
                [cos * r + cx, sin * r + cy]
            })
            .collect();
        let colors = s.points.iter().map(|p| p.1);
        let (shape, stops, solid) = match s.kind {
            0 => {
                // Toward the point farthest from the center (the first that
                // is), else the angle's way.
                let mut far = 0.0;
                let mut dir = [cos, sin];
                for p in &placed {
                    let (dx, dy) = (p[0] - cx, p[1] - cy);
                    let len = (dx * dx + dy * dy).sqrt();
                    if len > far {
                        far = len;
                        dir = [dx / len, dy / len];
                    }
                }
                let origin = [
                    cx - dir[0] * 1_000_000.0 * 0.5,
                    cy - dir[1] * 1_000_000.0 * 0.5,
                ];
                let stops = placed
                    .iter()
                    .zip(colors)
                    .map(|(p, c)| {
                        let (dx, dy) = (p[0] - origin[0], p[1] - origin[1]);
                        ((dx * dx + dy * dy).sqrt(), c)
                    })
                    .collect();
                (Shape::Linear { origin, dir }, stops, [0.0; 4])
            }
            _ => {
                // The slope of the point whose offsets add up the most (the
                // way VEGAS keeps its best), else the angle's.
                let mut best = 0.0;
                let mut chosen = None;
                for (i, p) in placed.iter().enumerate() {
                    let (dx, dy) = (p[0] - cx, p[1] - cy);
                    if dx.abs() + dy.abs() > best {
                        best = (dx + dy).abs();
                        chosen = Some((i, dx, dy));
                    }
                }
                let (i, dx, dy) = chosen.unwrap_or((0, cos, sin));
                if dx.abs() < 1e-6 || dy.abs() < 1e-6 {
                    let c = s.points.get(i).map_or([0.0; 4], |p| p.1);
                    let solid = if bytes {
                        let scale = c[3] * 255.0;
                        let cut = |v: f32| f32::from(v as i32 as u8);
                        [
                            cut(c[0] * scale),
                            cut(c[1] * scale),
                            cut(c[2] * scale),
                            cut(c[3] * 255.0),
                        ]
                    } else {
                        [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
                    };
                    (Shape::Solid, Vec::new(), solid)
                } else {
                    let k = dy / dx;
                    let elliptical = s.kind == 1;
                    let stops = placed
                        .iter()
                        .zip(colors)
                        .map(|(p, c)| {
                            let dy = p[1] - cy;
                            let d = if elliptical {
                                (2.0 * (dy * dy)).sqrt()
                            } else {
                                dy.abs()
                            };
                            (d, c)
                        })
                        .collect();
                    let shape = if elliptical {
                        Shape::Elliptical { k }
                    } else {
                        Shape::Rectangular { k }
                    };
                    (shape, stops, [0.0; 4])
                }
            }
        };
        Gradient {
            shape,
            center: s.center,
            stops,
            solid,
            width: width.max(1),
            height: height.max(1),
            bytes,
        }
    }

    /// Pixel `x`, `y`'s distance (from the top left).
    fn distance(&self, x: usize, y: usize) -> f64 {
        let (w, h) = (self.width as f64, self.height as f64);
        let [cx, cy] = self.center;
        match self.shape {
            Shape::Linear { origin, dir } => {
                ((x as f64 * (1.0 / w)) - origin[0]) * dir[0]
                    + ((y as f64 / h) - origin[1]) * dir[1]
            }
            Shape::Elliptical { k } => {
                let a = (x as f64 / w - cx) * k;
                let b = y as f64 / h - cy;
                (a * a + b * b).sqrt()
            }
            Shape::Rectangular { k } => {
                let a = ((x as f64 / w - cx) * k).abs();
                let b = (y as f64 / h - cy).abs();
                a.max(b)
            }
            Shape::Solid => 0.0,
        }
    }

    /// The points around distance `t`: the farthest below it and the
    /// nearest at or above it (the first of equals).
    fn around(&self, t: f64) -> (Option<usize>, Option<usize>) {
        let (mut lo, mut hi): (Option<usize>, Option<usize>) = (None, None);
        for (i, &(d, _)) in self.stops.iter().enumerate() {
            if d < t && lo.is_none_or(|l| d > self.stops[l].0) {
                lo = Some(i);
            }
            if d >= t && hi.is_none_or(|j| d < self.stops[j].0) {
                hi = Some(i);
            }
        }
        (lo, hi)
    }

    /// A point's color alone, premultiplied.
    fn alone(&self, i: usize) -> [f32; 4] {
        let c = self.stops[i].1;
        if self.bytes {
            let channel = |v: f32| f32::from(byte((v * c[3]) * 255.0));
            [
                channel(c[0]),
                channel(c[1]),
                channel(c[2]),
                f32::from(byte(c[3] * 255.0)),
            ]
        } else {
            [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
        }
    }

    /// Two points' colors mixed at `t`, premultiplied.
    fn between(&self, lo: usize, hi: usize, t: f64) -> [f32; 4] {
        let ((dl, cl), (dh, ch)) = (self.stops[lo], self.stops[hi]);
        let wl = ((dh - t) / (dh - dl)) * f64::from(cl[3]);
        let wh = ((t - dl) / (dh - dl)) * f64::from(ch[3]);
        let a = wl + wh;
        if a <= 0.0 {
            return [0.0; 4];
        }
        let mix = |k: usize| (f64::from(cl[k]) * wl + f64::from(ch[k]) * wh) as f32;
        if self.bytes {
            let channel = |v: f32| f32::from(byte(v * 255.0));
            [
                channel(mix(0)),
                channel(mix(1)),
                channel(mix(2)),
                channel(a as f32),
            ]
        } else {
            [mix(0), mix(1), mix(2), a as f32]
        }
    }

    /// Row `y` (from the top): premultiplied, 0–1 (bytes' values in an
    /// 8-bit project). Along a row, the points around one pixel's distance
    /// serve the next pixels while theirs stays between them, as VEGAS's.
    pub(crate) fn row(&self, y: usize) -> Vec<[f32; 4]> {
        let unit = |c: [f32; 4]| {
            if self.bytes { c.map(|v| v / 255.0) } else { c }
        };
        if self.shape == Shape::Solid {
            return vec![unit(self.solid); self.width];
        }
        let mut kept: Option<(usize, usize)> = None;
        (0..self.width)
            .map(|x| {
                let t = self.distance(x, y);
                let pair = match kept {
                    Some((l, h)) if self.stops[l].0 <= t && t <= self.stops[h].0 => {
                        (Some(l), Some(h))
                    }
                    _ => self.around(t),
                };
                let c = match pair {
                    (Some(l), Some(h)) => {
                        kept = Some((l, h));
                        self.between(l, h, t)
                    }
                    (None, Some(h)) => self.alone(h),
                    (Some(l), None) => self.alone(l),
                    (None, None) => [0.0; 4],
                };
                unit(c)
            })
            .collect()
    }

    /// What the card draws from: its numbers (`f`: the center, the linear
    /// direction or the slope; `j`: the shape — 0 linear, 1 elliptical,
    /// 2 rectangular, 3 solid —, the points, bytes) and its table — each
    /// point's distance (the linear one's measured from the center's) and
    /// color; a solid gradient's color first.
    pub(crate) fn plan(&self) -> (Vec<f32>, [u32; 4], Vec<[f32; 4]>) {
        let [cx, cy] = self.center;
        let (kind, f, shift) = match self.shape {
            Shape::Linear { origin, dir } => {
                // Distances from the center's: the half million back off.
                let back = ((cx - origin[0]) * dir[0]) + ((cy - origin[1]) * dir[1]);
                (0, [cx, cy, dir[0], dir[1]], back)
            }
            Shape::Elliptical { k } => (1, [cx, cy, k, 0.0], 0.0),
            Shape::Rectangular { k } => (2, [cx, cy, k, 0.0], 0.0),
            Shape::Solid => (3, [0.0; 4], 0.0),
        };
        let mut t = Vec::with_capacity(2 * self.stops.len() + 1);
        if self.shape == Shape::Solid {
            t.push(if self.bytes {
                self.solid.map(|v| v / 255.0)
            } else {
                self.solid
            });
        }
        for &(d, c) in &self.stops {
            t.push([(d - shift) as f32, 0.0, 0.0, 0.0]);
            t.push(c);
        }
        let j = [kind, self.stops.len() as u32, u32::from(self.bytes), 0];
        (f.map(|v| v as f32).to_vec(), j, t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

    fn linear() -> Settings {
        Settings {
            kind: 0,
            angle: 0.0,
            center: [0.5, 0.5],
            points: vec![(-0.5, WHITE), (0.5, BLACK)],
        }
    }

    #[test]
    fn white_to_black_runs_across() {
        let g = Gradient::new(&linear(), 640, 360, true);
        let row = g.row(100);
        assert_eq!(row[0], WHITE);
        let mid = row[320][0] * 255.0;
        assert!((mid - 127.5).abs() <= 0.5, "{mid}");
        assert!(row[639][0] < 0.01);
        // Bytes: each a whole step.
        assert!(row.iter().all(|c| (c[0] * 255.0).fract().abs() < 1e-4));
    }

    #[test]
    fn ellipses_follow_the_angle_and_flat_ones_are_one_color() {
        let s = Settings {
            kind: 1,
            angle: 45f64.to_radians(),
            points: vec![(0.0, WHITE), (0.5, BLACK)],
            ..linear()
        };
        let g = Gradient::new(&s, 200, 200, true);
        // White in the middle, darker toward the edges, alike all around.
        assert_eq!(g.row(100)[100], WHITE);
        assert_eq!(g.row(100)[40], g.row(40)[100]);
        let flat = Settings { angle: 0.0, ..s };
        let g = Gradient::new(&flat, 200, 200, true);
        assert_eq!(g.row(0)[0], g.row(199)[199]);
    }

    #[test]
    fn the_cards_numbers_measure_as_the_processor() {
        let g = Gradient::new(&linear(), 640, 360, false);
        let (f, j, t) = g.plan();
        assert_eq!(j[..3], [0, 2, 0]);
        // From the center, toward the first of the two farthest points
        // (the white one, on the left): it half the frame ahead.
        assert!(
            (t[0][0] - 0.5).abs() < 1e-6 && (t[2][0] + 0.5).abs() < 1e-6,
            "{t:?}"
        );
        assert_eq!(f[..2], [0.5, 0.5]);
    }
}
