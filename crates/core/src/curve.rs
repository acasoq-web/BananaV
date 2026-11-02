//! Curves of points (an OpenFX parametric parameter's, as BananaV keeps
//! and edits them): between two points the curve is a cubic Bézier whose
//! inner control points are the points' handles. A point the user never
//! gave handles is passed smoothly, without overshoot (monotone cubic,
//! Fritsch–Carlson); one with handles bends the curve around it with them,
//! as VEGAS's envelope tangents do.

/// A point of a curve and, when the user set them, its handles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurvePoint {
    pub x: f64,
    pub y: f64,
    pub handles: Option<Handles>,
}

/// A point's handles: where the curve's Bézier control points before and
/// after it are, from the point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handles {
    pub before: [f64; 2],
    pub after: [f64; 2],
    /// Each moves on its own (VEGAS's "Lock Tangent(s)" cleared); locked,
    /// the two stay in line.
    pub split: bool,
}

impl CurvePoint {
    /// A point the curve passes smoothly.
    pub const fn new(x: f64, y: f64) -> CurvePoint {
        CurvePoint {
            x,
            y,
            handles: None,
        }
    }

    /// A point with handles (locked: in line).
    pub const fn with(x: f64, y: f64, before: [f64; 2], after: [f64; 2]) -> CurvePoint {
        CurvePoint {
            x,
            y,
            handles: Some(Handles {
                before,
                after,
                split: false,
            }),
        }
    }

    /// A point with handles that move apart (a corner).
    pub const fn split(x: f64, y: f64, before: [f64; 2], after: [f64; 2]) -> CurvePoint {
        CurvePoint {
            x,
            y,
            handles: Some(Handles {
                before,
                after,
                split: true,
            }),
        }
    }
}

/// The straight line from (0, 0) to (1, 1): a curve that changes nothing.
pub const IDENTITY: [CurvePoint; 2] = [CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 1.0)];

/// The slope of the segment after point `i`.
fn slope(points: &[CurvePoint], i: usize) -> f64 {
    let (a, b) = (points[i], points[i + 1]);
    let dx = b.x - a.x;
    if dx.abs() < 1e-12 {
        0.0
    } else {
        (b.y - a.y) / dx
    }
}

/// The curve's slope at point `i` when it has no handles: Fritsch–Carlson's,
/// which never overshoots (flat where the curve turns back).
fn auto_tangent(points: &[CurvePoint], i: usize) -> f64 {
    let n = points.len();
    if n < 2 {
        return 0.0;
    }
    if i == 0 {
        return slope(points, 0);
    }
    if i == n - 1 {
        return slope(points, n - 2);
    }
    let (a, b) = (slope(points, i - 1), slope(points, i));
    if a * b <= 0.0 {
        0.0
    } else {
        2.0 / (1.0 / a + 1.0 / b)
    }
}

/// Point `i`'s handles: its own, or those of a smooth curve (along its
/// slope, a third of the way to each neighbour).
pub fn handles(points: &[CurvePoint], i: usize) -> Handles {
    if let Some(h) = points[i].handles {
        return h;
    }
    let m = auto_tangent(points, i);
    let before = if i > 0 {
        (points[i].x - points[i - 1].x) / 3.0
    } else {
        0.0
    };
    let after = if i + 1 < points.len() {
        (points[i + 1].x - points[i].x) / 3.0
    } else {
        0.0
    };
    Handles {
        before: [-before, -before * m],
        after: [after, after * m],
        split: false,
    }
}

/// The four Bézier control points of the segment after point `i`, the
/// inner two kept within the segment's width (so it never turns back).
pub fn segment(points: &[CurvePoint], i: usize) -> [[f64; 2]; 4] {
    let (a, b) = (points[i], points[i + 1]);
    let (ha, hb) = (handles(points, i), handles(points, i + 1));
    let clamp_x = |x: f64| x.clamp(a.x.min(b.x), a.x.max(b.x));
    [
        [a.x, a.y],
        [clamp_x(a.x + ha.after[0]), a.y + ha.after[1]],
        [clamp_x(b.x + hb.before[0]), b.y + hb.before[1]],
        [b.x, b.y],
    ]
}

fn bezier(p: [f64; 4], t: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * p[0] + 3.0 * u * u * t * p[1] + 3.0 * u * t * t * p[2] + t * t * t * p[3]
}

/// The curve's value at `x`: the first point's before it, the last one's
/// after it. A curve without points is 0.
pub fn curve_at(points: &[CurvePoint], x: f64) -> f64 {
    match points {
        [] => 0.0,
        [p] => p.y,
        _ => {
            let n = points.len();
            if x <= points[0].x {
                return points[0].y;
            }
            if x >= points[n - 1].x {
                return points[n - 1].y;
            }
            let i = (points.partition_point(|p| p.x <= x) - 1).min(n - 2);
            let s = segment(points, i);
            let xs = [s[0][0], s[1][0], s[2][0], s[3][0]];
            let ys = [s[0][1], s[1][1], s[2][1], s[3][1]];
            let h = xs[3] - xs[0];
            if h.abs() < 1e-12 {
                return ys[0];
            }
            // Handles a third of the way: x moves evenly with t.
            let even =
                (xs[1] - xs[0] - h / 3.0).abs() < 1e-9 && (xs[3] - xs[2] - h / 3.0).abs() < 1e-9;
            let t = if even {
                (x - xs[0]) / h
            } else {
                // x grows with t (the handles stay within the segment).
                let (mut lo, mut hi) = (0.0, 1.0);
                for _ in 0..48 {
                    let mid = 0.5 * (lo + hi);
                    if bezier(xs, mid) < x {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                0.5 * (lo + hi)
            };
            bezier(ys, t)
        }
    }
}

/// The curve's points from `x0` to `x1`, `n` + 1 of them (to draw it).
pub fn trace(points: &[CurvePoint], x0: f64, x1: f64, n: usize) -> Vec<[f64; 2]> {
    let n = n.max(1);
    (0..=n)
        .map(|i| {
            let x = x0 + (x1 - x0) * i as f64 / n as f64;
            [x, curve_at(points, x)]
        })
        .collect()
}

/// `a` and `b` mixed (`t`: 0 is `a`, 1 is `b`) when they have as many
/// points, each with handles or each without; `None` otherwise.
pub fn mix(a: &[CurvePoint], b: &[CurvePoint], t: f64) -> Option<Vec<CurvePoint>> {
    if a.len() != b.len() {
        return None;
    }
    let l = |p: f64, q: f64| p + (q - p) * t;
    let l2 = |p: [f64; 2], q: [f64; 2]| [l(p[0], q[0]), l(p[1], q[1])];
    a.iter()
        .zip(b)
        .map(|(p, q)| {
            let handles = match (p.handles, q.handles) {
                (None, None) => None,
                (Some(h), Some(k)) => Some(Handles {
                    before: l2(h.before, k.before),
                    after: l2(h.after, k.after),
                    split: h.split || k.split,
                }),
                _ => return None,
            };
            Some(CurvePoint {
                x: l(p.x, q.x),
                y: l(p.y, q.y),
                handles,
            })
        })
        .collect()
}

/// Saved as a list of numbers: x and y, then — with handles — the one
/// before, the one after and 1 when they move apart. Older files have the
/// two numbers only.
#[cfg(feature = "serde")]
mod serde_impl {
    use super::{CurvePoint, Handles};
    use serde::de::{Error, SeqAccess, Visitor};
    use serde::ser::SerializeSeq;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    impl Serialize for CurvePoint {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut v = vec![self.x, self.y];
            if let Some(h) = self.handles {
                v.extend([h.before[0], h.before[1], h.after[0], h.after[1]]);
                if h.split {
                    v.push(1.0);
                }
            }
            let mut seq = s.serialize_seq(Some(v.len()))?;
            for x in v {
                seq.serialize_element(&x)?;
            }
            seq.end()
        }
    }

    struct Numbers;

    impl<'de> Visitor<'de> for Numbers {
        type Value = CurvePoint;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a curve point: 2, 6 or 7 numbers")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<CurvePoint, A::Error> {
            let mut v = Vec::new();
            while let Some(x) = seq.next_element::<f64>()? {
                v.push(x);
            }
            match v.len() {
                2 => Ok(CurvePoint::new(v[0], v[1])),
                6 | 7 => Ok(CurvePoint {
                    x: v[0],
                    y: v[1],
                    handles: Some(Handles {
                        before: [v[2], v[3]],
                        after: [v[4], v[5]],
                        split: v.get(6).is_some_and(|&s| s != 0.0),
                    }),
                }),
                n => Err(A::Error::invalid_length(n, &self)),
            }
        }
    }

    impl<'de> Deserialize<'de> for CurvePoint {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<CurvePoint, D::Error> {
            d.deserialize_seq(Numbers)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn without_handles_the_curve_is_smooth_and_never_overshoots() {
        assert!(near(curve_at(&IDENTITY, 0.3), 0.3));
        let pts = [
            CurvePoint::new(0.0, 0.0),
            CurvePoint::new(0.5, 1.0),
            CurvePoint::new(1.0, 1.0),
        ];
        // Flat where it reaches its top: never above 1.
        for i in 0..=100 {
            let y = curve_at(&pts, i as f64 / 100.0);
            assert!((0.0..=1.0 + 1e-12).contains(&y), "{y}");
        }
        assert!(near(curve_at(&pts, 0.5), 1.0));
        // Before the first point and after the last, their values.
        assert!(near(curve_at(&pts, -1.0), 0.0) && near(curve_at(&pts, 2.0), 1.0));
    }

    #[test]
    fn handles_bend_the_curve() {
        // Flat at both ends: an S through the middle.
        let s = [
            CurvePoint::with(0.0, 0.0, [0.0, 0.0], [0.5, 0.0]),
            CurvePoint::with(1.0, 1.0, [-0.5, 0.0], [0.0, 0.0]),
        ];
        assert!(near(curve_at(&s, 0.5), 0.5));
        assert!(curve_at(&s, 0.2) < 0.2 && curve_at(&s, 0.8) > 0.8);
        // Handles a third of the way along the line: still the line.
        let line = [
            CurvePoint::with(0.0, 0.0, [0.0, 0.0], [1.0 / 3.0, 1.0 / 3.0]),
            CurvePoint::with(1.0, 1.0, [-1.0 / 3.0, -1.0 / 3.0], [0.0, 0.0]),
        ];
        for x in [0.1, 0.45, 0.9] {
            assert!(near(curve_at(&line, x), x));
        }
        // A handle reaching past its neighbour does not turn the curve back.
        let wild = [
            CurvePoint::with(0.0, 0.0, [0.0, 0.0], [3.0, 1.0]),
            CurvePoint::new(1.0, 1.0),
        ];
        let mut last = -1.0;
        for i in 0..=50 {
            let x = i as f64 / 50.0;
            let y = curve_at(&wild, x);
            assert!(y.is_finite());
            last = y.max(last);
        }
        assert!(last > 0.0);
    }

    #[test]
    fn curves_with_as_many_points_mix() {
        let a = [CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 1.0)];
        let b = [CurvePoint::new(0.0, 0.5), CurvePoint::new(1.0, 0.5)];
        let m = mix(&a, &b, 0.5).expect("mixable");
        assert!(near(m[0].y, 0.25) && near(m[1].y, 0.75));
        assert!(mix(&a, &b[..1], 0.5).is_none());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn points_read_back_and_older_files_still_read() {
        let pts = vec![
            CurvePoint::new(0.0, 0.25),
            CurvePoint::with(0.5, 0.5, [-0.1, 0.0], [0.1, 0.2]),
            CurvePoint::split(1.0, 1.0, [-0.2, 0.0], [0.0, 0.0]),
        ];
        let json = serde_json::to_string(&pts).expect("json");
        assert_eq!(
            json,
            "[[0.0,0.25],[0.5,0.5,-0.1,0.0,0.1,0.2],[1.0,1.0,-0.2,0.0,0.0,0.0,1.0]]"
        );
        let back: Vec<CurvePoint> = serde_json::from_str(&json).expect("read");
        assert_eq!(back, pts);
        let old: Vec<CurvePoint> = serde_json::from_str("[[0,0],[1,1]]").expect("old");
        assert_eq!(old, IDENTITY.to_vec());
    }
}
