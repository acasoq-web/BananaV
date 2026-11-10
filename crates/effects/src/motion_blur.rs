//! Motion Blur: VEGAS's. What moves blurred along the frame's motion, as
//! a camera's shutter would.
//!
//! As VEGAS's renders show: the motion from the frame to the next (the last frame:
//! from the one before), each pixel's by the optical flow; the pixels
//! moving more than Motion Threshold (min) and no more than (max) — parts
//! of the frame's width a frame — make the mask. Their mean direction
//! picks a line through the middle of a square kernel — upright, level or
//! diagonal, the nearest —, turned the rest of the way by an affine warp
//! (bilinear, in 32nds of a pixel, nothing past its edges) and
//! summing to 1; its reach Blur Amount thousandths of the frame's width
//! (Fixed Blur), or the mask's mean motion times Blur Amount, at most
//! Maximum Blur Amount thousandths (Motion-adaptive Blur). The mask spread
//! by the kernel picks where the picture blurred by it shows; both reflect
//! the picture past its edges.

use crate::fft::{Edges, convolve_with};
use crate::kit::Buf;

/// The settings that shape the blur.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub adaptive: bool,
    /// Motion Threshold's min and max, percentages of the frame's width.
    pub range: [f64; 2],
    /// Fixed Blur's Blur Amount, thousandths of the frame's width.
    pub strength: f64,
    /// Motion-adaptive Blur's Blur Amount and Maximum Blur Amount.
    pub magnifier: f64,
    pub most: f64,
}

/// What the motion makes of a frame: the mask (rows up, as the picture's)
/// and the kernel's side and weights (rows up, as `fft::convolve_with`
/// takes them: VEGAS's correlation as a convolution).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Blur {
    pub mask: Vec<bool>,
    pub size: usize,
    pub kernel: Vec<f32>,
}

impl Settings {
    /// Motion Threshold's ends, swapped when the max is under the min
    /// (VEGAS's).
    fn range(&self) -> [f64; 2] {
        let [lo, hi] = self.range;
        if hi < lo { [hi, lo] } else { [lo, hi] }
    }

    /// The blur of a picture `w` × `h` pixels whose pixel `x`, `y` (from
    /// its lower left) moves `motion(x, y)` pixels (y up) a frame; `None`
    /// when nothing moves within the range.
    pub(crate) fn blur(
        &self,
        w: usize,
        h: usize,
        motion: &dyn Fn(usize, usize) -> [f64; 2],
    ) -> Option<Blur> {
        let width = w as f64;
        let [lo, hi] = self.range();
        let (min, max) = ((width * 0.01 * lo) as f32, (width * 0.01 * hi) as f32);
        let mut mask = vec![false; w * h];
        let (mut sines, mut cosines, mut sum, mut count) = (0.0f64, 0.0f64, 0.0f64, 0usize);
        for y in 0..h {
            for x in 0..w {
                // The image's coordinates: y down.
                let m = motion(x, y);
                let (u, v) = (m[0] as f32, -m[1] as f32);
                let length = u.hypot(v);
                let kept = if length > min { length } else { 0.0 };
                let kept = if kept > max { 0.0 } else { kept };
                if kept > min {
                    mask[y * w + x] = true;
                    let angle = v.atan2(u);
                    sines += f64::from(angle.sin());
                    cosines += f64::from(angle.cos());
                    sum += f64::from(kept);
                    count += 1;
                }
            }
        }
        if count == 0 {
            return None;
        }
        // VEGAS's: the cosines' sum over the sines', within half a turn.
        let mut phi = cosines.atan2(sines).to_degrees();
        if phi < 0.0 {
            phi += 360.0;
        }
        if phi > 179.9999998807907 {
            phi -= 180.0;
        }
        let reach = if self.adaptive {
            (sum / count as f64 * self.magnifier).min(width * 0.001 * self.most)
        } else {
            width * 0.001 * self.strength
        };
        let r = ((reach + 0.5) as usize).max(1);
        let (size, kernel) = kernel(phi, r);
        Some(Blur { mask, size, kernel })
    }
}

/// The kernel of direction `phi` (degrees, 0–180) reaching `r` pixels:
/// its side and weights, rows up, mirrored across so the frequency
/// domain's convolution is VEGAS's correlation.
pub(crate) fn kernel(phi: f64, r: usize) -> (usize, Vec<f32>) {
    let n = 2 * r + 1;
    // A line of ones through the middle, rows from the top.
    let mut line = vec![0.0f64; n * n];
    let angle = if phi <= 22.5 || phi >= 157.5 {
        for k in 0..n {
            line[k * n + r] = 1.0;
        }
        if phi > 22.5 { 180.0 - phi } else { phi }
    } else if (67.5..=112.5).contains(&phi) {
        for k in 0..n {
            line[r * n + k] = 1.0;
        }
        90.0 - phi
    } else if phi <= 67.5 {
        for k in 0..n {
            line[(n - 1 - k) * n + k] = 1.0;
        }
        45.0 - phi
    } else {
        for k in 0..n {
            line[k * n + k] = 1.0;
        }
        135.0 - phi
    };
    let turned = rotated(&line, n, r as f64, angle);
    let total: f64 = turned.iter().sum();
    let scale = if total != 0.0 { 1.0 / total } else { 0.0 };
    let mut out = vec![0.0f32; n * n];
    for ky in 0..n {
        for kx in 0..n {
            out[ky * n + kx] = (turned[ky * n + (n - 1 - kx)] * scale) as f32;
        }
    }
    (n, out)
}

/// `src` (`n` × `n`, rows from the top) turned `angle` degrees about
/// (`c`, `c`), bilinear and zero past the edges: each pixel read where
/// the inverse map puts it, to a 32nd of a pixel.
fn rotated(src: &[f64], n: usize, c: f64, angle: f64) -> Vec<f64> {
    let a = angle * std::f64::consts::PI / 180.0;
    let (alpha, beta) = (a.cos(), a.sin());
    let m = [
        alpha,
        beta,
        (1.0 - alpha) * c - beta * c,
        -beta,
        alpha,
        beta * c + (1.0 - alpha) * c,
    ];
    // invertAffineTransform.
    let d = m[0] * m[4] - m[1] * m[3];
    let d = if d != 0.0 { 1.0 / d } else { 0.0 };
    let (a11, a22, a12, a21) = (m[4] * d, m[0] * d, -m[1] * d, -m[3] * d);
    let im = [
        a11,
        a12,
        -a11 * m[2] - a12 * m[5],
        a21,
        a22,
        -a21 * m[2] - a22 * m[5],
    ];
    const AB_SCALE: f64 = 1024.0;
    let round = |v: f64| v.round_ties_even() as i64;
    let at = |x: i64, y: i64| {
        if x < 0 || y < 0 || x >= n as i64 || y >= n as i64 {
            0.0
        } else {
            src[y as usize * n + x as usize]
        }
    };
    let mut out = vec![0.0f64; n * n];
    for y in 0..n {
        let x0 = round((im[1] * y as f64 + im[2]) * AB_SCALE) + 16;
        let y0 = round((im[4] * y as f64 + im[5]) * AB_SCALE) + 16;
        for x in 0..n {
            let fx = (x0 + round(im[0] * x as f64 * AB_SCALE)) >> 5;
            let fy = (y0 + round(im[3] * x as f64 * AB_SCALE)) >> 5;
            let (sx, sy) = (fx >> 5, fy >> 5);
            let (wx, wy) = ((fx & 31) as f64 / 32.0, (fy & 31) as f64 / 32.0);
            out[y * n + x] = at(sx, sy) * (1.0 - wx) * (1.0 - wy)
                + at(sx + 1, sy) * wx * (1.0 - wy)
                + at(sx, sy + 1) * (1.0 - wx) * wy
                + at(sx + 1, sy + 1) * wx * wy;
        }
    }
    out
}

/// Where `blur`'s mask, spread by its kernel, reaches on the picture
/// `src` (rows up): VEGAS's, where the spread mask's 8 bits are not 0.
pub(crate) fn spread(src: &Buf, blur: &Blur) -> Vec<bool> {
    let mask = Buf {
        bounds: src.bounds,
        data: blur
            .mask
            .iter()
            .map(|&m| if m { [255.0; 4] } else { [0.0; 4] })
            .collect(),
    };
    convolve_with(
        &mask,
        &scalar(&blur.kernel),
        blur.size,
        Edges::Reflected,
        &|c| c,
        &|_, _, c| c,
    )
    .data
    .iter()
    .map(|v| v[0] > 0.5)
    .collect()
}

/// The picture `src` blurred where `blur`'s mask, spread by its kernel,
/// reaches.
pub(crate) fn apply(src: &Buf, blur: &Blur) -> Buf {
    let reached = spread(src, blur);
    let blurred = convolve_with(
        src,
        &scalar(&blur.kernel),
        blur.size,
        Edges::Reflected,
        &|c| c,
        &|_, _, c| c,
    );
    let mut out = src.clone();
    for ((o, b), &r) in out.data.iter_mut().zip(&blurred.data).zip(&reached) {
        if r {
            *o = *b;
        }
    }
    out
}

/// A kernel of plain weights as the frequency domain's cells.
fn scalar(kernel: &[f32]) -> Vec<[f32; 4]> {
    kernel.iter().map(|&v| [v, 0.0, v, 0.0]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            adaptive: false,
            range: [0.5, 100.0],
            strength: 2.0,
            magnifier: 1.0,
            most: 10.0,
        }
    }

    #[test]
    fn the_kernel_lines_up_with_the_motion() {
        // Upright motion (φ near 0): an upright line; level (φ 90): a
        // level one; each summing to 1.
        for (phi, check) in [(0.0, [true, false]), (90.0, [false, true])] {
            let (n, k) = kernel(phi, 3);
            assert_eq!(n, 7);
            let total: f32 = k.iter().sum();
            assert!((total - 1.0).abs() < 1e-6, "{total}");
            let column: f32 = (0..7).map(|y| k[y * 7 + 3]).sum();
            let row: f32 = (0..7).map(|x| k[3 * 7 + x]).sum();
            assert_eq!([column > 0.99, row > 0.99], check, "{phi}: {k:?}");
        }
        // Halfway between: spread across both diagonals' neighbours, the
        // middle kept.
        let (_, k) = kernel(30.0, 4);
        assert!(k[4 * 9 + 4] > 0.0);
    }

    #[test]
    fn moving_pixels_make_the_mask_and_the_direction() {
        // A picture 100 wide, its left half moving 3 pixels right a frame
        // (more than half a percent): the mask there, a level kernel,
        // Blur Amount 2 thousandths of the width: a reach of 1.
        let (w, h) = (100, 10);
        let motion = |x: usize, _: usize| if x < 50 { [3.0, 0.0] } else { [0.0, 0.0] };
        let b = settings().blur(w, h, &motion).expect("moves");
        assert!(b.mask[5 * w + 10] && !b.mask[5 * w + 80]);
        assert_eq!(b.size, 3);
        let row: f32 = (0..3).map(|x| b.kernel[3 + x]).sum();
        assert!(row > 0.99, "{:?}", b.kernel);
        // Nothing moves past the threshold: no blur.
        let still = |_: usize, _: usize| [0.1, 0.0];
        assert!(settings().blur(w, h, &still).is_none());
        // Motion-adaptive: the mean motion times Blur Amount, at most the
        // maximum (here 10 pixels, then 2).
        let adaptive = Settings {
            adaptive: true,
            magnifier: 2.0,
            most: 100.0,
            ..settings()
        };
        assert_eq!(adaptive.blur(w, h, &motion).unwrap().size, 13);
        let capped = Settings {
            most: 20.0,
            magnifier: 50.0,
            ..adaptive
        };
        assert_eq!(capped.blur(w, h, &motion).unwrap().size, 5);
    }

    #[test]
    fn only_the_spread_mask_is_blurred() {
        let mut src = Buf::solid([0, 0, 40, 10], [0.0, 0.0, 0.0, 1.0]);
        for y in 0..10 {
            src.data[y * 40 + 5] = [1.0, 1.0, 1.0, 1.0];
            src.data[y * 40 + 35] = [1.0, 1.0, 1.0, 1.0];
        }
        let (size, kernel) = kernel(90.0, 2);
        let mut mask = vec![false; 400];
        for y in 0..10 {
            mask[y * 40 + 5] = true;
        }
        let out = apply(&src, &Blur { mask, size, kernel });
        // Around the moving column: spread across; the other stays.
        assert!((out.at(4, 5)[0] - 0.2).abs() < 1e-4, "{:?}", out.at(4, 5));
        assert_eq!(out.at(34, 5), src.at(34, 5));
        assert_eq!(out.at(35, 5), src.at(35, 5));
    }
}
