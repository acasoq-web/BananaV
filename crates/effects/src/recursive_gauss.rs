//! VEGAS's Gaussian blur, as its effects and transitions blur (Flash's
//! diffusion, Cross Effect's blur): Deriche's fourth-order recursive
//! filter, across and then down, a constant beyond the picture's edges.
//! A blur's radius is where it falls to 1⁄255 of its middle: its sigma is
//! (|r| + 1.5) ⁄ √(2 ln 255) pixels, at least 0.6 — a radius of 0 still
//! blurs a little, as VEGAS's does.
//!
//! The processor runs the filter as VEGAS does, four poles at once in
//! doubles; the card runs the same filter as its two oscillations side by
//! side, each one complex pole (`sections`), which floats keep accurate
//! however wide the blur is.

use crate::kit::{Buf, for_chunks};

/// The sigma of a blur of radius `r` pixels.
pub(crate) fn sigma(r: f64) -> f64 {
    ((r.abs() + 1.5) / (2.0 * 255f64.ln()).sqrt()).max(0.6)
}

/// One of the filter's two damped oscillations: its response at a step
/// `m` ≥ 0 is rᵐ (a cos ωm + b sin ωm).
#[derive(Clone, Copy, Debug)]
struct Wave {
    a: f64,
    b: f64,
    r: f64,
    w: f64,
}

/// The two oscillations of a blur of `sigma` pixels (Deriche's constants,
/// scaled so the whole response sums to about 1).
fn waves(sigma: f64) -> [Wave; 2] {
    let s = sigma.max(0.6);
    let n = s * (2.0 * std::f64::consts::PI).sqrt();
    [
        Wave {
            a: 1.6803 / n,
            b: 3.735 / n,
            r: (-1.783 / s).exp(),
            w: 0.6318 / s,
        },
        Wave {
            a: -0.6803 / n,
            b: -0.2598 / n,
            r: (-1.723 / s).exp(),
            w: 1.997 / s,
        },
    ]
}

/// The filter along one line, as VEGAS runs it: the four poles' causal
/// and anticausal recursions in doubles.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Line {
    /// The causal numerator, a[0] to a[3].
    a: [f64; 4],
    /// The denominator, b[1] to b[4] (b[0] is 1).
    b: [f64; 4],
    /// The anticausal numerator, its taps 1 to 4 ahead.
    after: [f64; 4],
    /// What each pass settles to on a constant line, per unit of it.
    steady: f64,
    steady_after: f64,
}

impl Line {
    pub(crate) fn of_sigma(sigma: f64) -> Line {
        let [p, q] = waves(sigma);
        // Each oscillation: (α0 + α1 z⁻¹) ⁄ (1 + β1 z⁻¹ + β2 z⁻²).
        let section = |o: Wave| {
            let (c, s) = (o.w.cos(), o.w.sin());
            (o.a, o.r * (o.b * s - o.a * c), -2.0 * o.r * c, o.r * o.r)
        };
        let (p0, p1, p2, p3) = section(p);
        let (q0, q1, q2, q3) = section(q);
        // The two over one denominator.
        let a = [
            p0 + q0,
            p1 + p0 * q2 + q1 + q0 * p2,
            p1 * q2 + p0 * q3 + q1 * p2 + q0 * p3,
            p1 * q3 + q1 * p3,
        ];
        let b = [p2 + q2, p3 + q3 + p2 * q2, p2 * q3 + p3 * q2, p3 * q3];
        // The anticausal part leaves the middle tap out.
        let after = [
            a[1] - a[0] * b[0],
            a[2] - a[0] * b[1],
            a[3] - a[0] * b[2],
            -a[0] * b[3],
        ];
        let denominator = 1.0 + b.iter().sum::<f64>();
        let (steady, steady_after) = if denominator.abs() > 1e-17 {
            (
                a.iter().sum::<f64>() / denominator,
                after.iter().sum::<f64>() / denominator,
            )
        } else {
            (1.0, 0.0)
        };
        Line {
            a,
            b,
            after,
            steady,
            steady_after,
        }
    }

    /// Filters `line` (four channels a pixel) in place, `scratch` as long.
    pub(crate) fn run(&self, line: &mut [[f32; 4]], scratch: &mut Vec<[f64; 4]>) {
        let n = line.len();
        if n == 0 {
            return;
        }
        scratch.clear();
        scratch.resize(n, [0.0; 4]);
        let wide = |p: [f32; 4]| p.map(f64::from);
        // Before the line its first value, the pass settled on it.
        let first = wide(line[0]);
        let mut xs = [first; 3];
        let mut ys = [first.map(|v| v * self.steady); 4];
        for (i, out) in scratch.iter_mut().enumerate() {
            let x = wide(line[i]);
            let y: [f64; 4] = std::array::from_fn(|c| {
                self.a[0] * x[c]
                    + self.a[1] * xs[0][c]
                    + self.a[2] * xs[1][c]
                    + self.a[3] * xs[2][c]
                    - self.b[0] * ys[0][c]
                    - self.b[1] * ys[1][c]
                    - self.b[2] * ys[2][c]
                    - self.b[3] * ys[3][c]
            });
            *out = y;
            xs = [x, xs[0], xs[1]];
            ys = [y, ys[0], ys[1], ys[2]];
        }
        // After it its last value.
        let last = wide(line[n - 1]);
        let mut ahead = [last; 4];
        let mut zs = [last.map(|v| v * self.steady_after); 4];
        for i in (0..n).rev() {
            let z: [f64; 4] = std::array::from_fn(|c| {
                self.after[0] * ahead[0][c]
                    + self.after[1] * ahead[1][c]
                    + self.after[2] * ahead[2][c]
                    + self.after[3] * ahead[3][c]
                    - self.b[0] * zs[0][c]
                    - self.b[1] * zs[1][c]
                    - self.b[2] * zs[2][c]
                    - self.b[3] * zs[3][c]
            });
            let x = wide(line[i]);
            line[i] = std::array::from_fn(|c| (scratch[i][c] + z[c]) as f32);
            ahead = [x, ahead[0], ahead[1], ahead[2]];
            zs = [z, zs[0], zs[1], zs[2]];
        }
    }
}

/// The card's form of a blur of `sigma` pixels: each oscillation's
/// complex pole `p` (`z = x + p z` along the line, its response at a step
/// `m` the real part of `c pᵐ`), its weight `c`, and what its causal and
/// anticausal passes settle to per unit of a constant line, `1 ⁄ (1 − p)`
/// and `p ⁄ (1 − p)` — eight numbers each, sixteen in all.
pub fn sections(sigma: f64) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for (k, o) in waves(sigma).into_iter().enumerate() {
        let p = [o.r * o.w.cos(), o.r * o.w.sin()];
        let d = [1.0 - p[0], -p[1]];
        let m = d[0] * d[0] + d[1] * d[1];
        let settle = [d[0] / m, -d[1] / m];
        let ahead = [
            p[0] * settle[0] - p[1] * settle[1],
            p[0] * settle[1] + p[1] * settle[0],
        ];
        let numbers = [
            p[0], p[1], o.a, -o.b, settle[0], settle[1], ahead[0], ahead[1],
        ];
        for (i, v) in numbers.into_iter().enumerate() {
            out[k * 8 + i] = v as f32;
        }
    }
    out
}

/// `buf` blurred `radii` pixels across and down, as VEGAS blurs.
pub(crate) fn blur(buf: &mut Buf, radii: [f64; 2]) {
    let (w, h) = (buf.width().max(0) as usize, buf.height().max(0) as usize);
    if w == 0 || h == 0 {
        return;
    }
    let across = Line::of_sigma(sigma(radii[0]));
    for_chunks(&mut buf.data, w, |_, row| {
        across.run(row, &mut Vec::new());
    });
    // Down: the columns as rows of a turned copy.
    let mut turned = vec![[0.0f32; 4]; w * h];
    for (y, row) in buf.data.chunks(w).enumerate() {
        for (x, px) in row.iter().enumerate() {
            turned[x * h + y] = *px;
        }
    }
    let down = Line::of_sigma(sigma(radii[1]));
    for_chunks(&mut turned, h, |_, column| {
        down.run(column, &mut Vec::new());
    });
    for (x, column) in turned.chunks(h).enumerate() {
        for (y, px) in column.iter().enumerate() {
            buf.data[y * w + x] = *px;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The filter's response to one bright pixel in the middle of a long
    /// line.
    fn response(sigma: f64) -> Vec<f64> {
        let n = 401;
        let mut line = vec![[0.0f32; 4]; n];
        line[n / 2] = [1.0; 4];
        Line::of_sigma(sigma).run(&mut line, &mut Vec::new());
        line.iter().map(|p| f64::from(p[0])).collect()
    }

    #[test]
    fn the_filter_is_a_gaussian_of_its_sigma() {
        for sigma in [1.0, 4.0, 20.0] {
            let r = response(sigma);
            let mid = r.len() / 2;
            let total: f64 = r.iter().sum();
            assert!((total - 1.0).abs() < 2e-3, "{sigma}: {total}");
            for d in [0usize, 1, 3, 10] {
                let g = (-((d * d) as f64) / (2.0 * sigma * sigma)).exp()
                    / (sigma * (2.0 * std::f64::consts::PI).sqrt());
                assert!(
                    (r[mid + d] - g).abs() < 2e-3 && (r[mid - d] - r[mid + d]).abs() < 1e-6,
                    "{sigma} at {d}: {} against {g}",
                    r[mid + d]
                );
            }
        }
    }

    #[test]
    fn a_constant_line_stays_constant_and_the_radius_reaches_1_in_255() {
        let mut line = vec![[0.25f32; 4]; 50];
        Line::of_sigma(sigma(12.0)).run(&mut line, &mut Vec::new());
        assert!(line.iter().all(|p| (p[0] - 0.25).abs() < 1e-3), "{line:?}");
        // At r + 1.5 the Gaussian is 1⁄255 of its middle.
        let s = sigma(30.0);
        assert!(((-(31.5f64 * 31.5) / (2.0 * s * s)).exp() - 1.0 / 255.0).abs() < 1e-9);
        assert_eq!(sigma(0.0), 0.6);
    }

    #[test]
    fn the_cards_sections_add_up_to_the_same_filter() {
        // Each oscillation one complex pole at a time, from the card's
        // floats, against the four poles at once in doubles — wide blurs
        // too, where poles near 1 call for the care.
        for sigma in [0.6, 3.0, 50.0, 300.0] {
            let s = sections(sigma).map(f64::from);
            let n = 900;
            let x: Vec<f64> = (0..n).map(|i| ((i * 37) % 101) as f64 / 100.0).collect();
            let mut out = vec![0.0; n];
            for k in 0..2 {
                let c = &s[k * 8..k * 8 + 8];
                let mul = |a: [f64; 2], b: [f64; 2]| {
                    [a[0] * b[0] - a[1] * b[1], a[0] * b[1] + a[1] * b[0]]
                };
                let mut z = [x[0] * c[4], x[0] * c[5]];
                for i in 0..n {
                    let pz = mul([c[0], c[1]], z);
                    z = [x[i] + pz[0], pz[1]];
                    out[i] += c[2] * z[0] - c[3] * z[1];
                }
                let mut w = [x[n - 1] * c[6], x[n - 1] * c[7]];
                for i in (0..n).rev() {
                    out[i] += c[2] * w[0] - c[3] * w[1];
                    w = mul([c[0], c[1]], [x[i] + w[0], w[1]]);
                }
            }
            let mut line: Vec<[f32; 4]> = x.iter().map(|&v| [v as f32; 4]).collect();
            Line::of_sigma(sigma).run(&mut line, &mut Vec::new());
            for (i, (a, b)) in out.iter().zip(&line).enumerate() {
                assert!(
                    (a - f64::from(b[0])).abs() < 2e-5,
                    "{sigma} at {i}: {a} {}",
                    b[0]
                );
            }
        }
    }
}
