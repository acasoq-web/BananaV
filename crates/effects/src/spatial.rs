//! What the effects that work a whole picture out at once share (Gaussian
//! Blur, Sharpen, Vignette, Mirror and Glow): the source read once, the
//! result written over the render window, and Gaussian blurs with the
//! picture's edges held.
#![allow(unsafe_code)]

use std::ffi::c_int;
use std::sync::Mutex;

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::kit::{Buf, Grid};
use crate::ofx::{Fx, Props, for_rows};

/// How far under the exact value results come for the effects that round
/// as VEGAS's do: a 32nd of an 8-bit step, so their halves round down.
pub(crate) const TIE: f32 = 1.0 / (32.0 * 255.0);

/// Renders the effect: the source read whole (premultiplied), `work` gives
/// the picture over the source's pixels, which goes over the render
/// window.
pub(crate) fn render(fx: Fx, args: Props, work: &dyn Fn(&Grid, &Buf) -> Buf) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|v| if v > 0.0 { v } else { 1.0 });
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    if let Some(s) = &src
        && (s.depth != out.depth || s.channels != out.channels)
    {
        return kOfxStatErrImageFormat;
    }
    let (origin, frame) = fx.frame();
    let grid = Grid {
        origin,
        frame,
        scale,
        par: out.par,
    };
    let source = Buf::read(src.as_ref());
    drop(src);
    let result = work(&grid, &source);
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let rows = y1.max(out.bounds[1])..y2.min(out.bounds[3]);
    crate::colors::write_out(&out, &result, columns, rows);
    drop(out);
    kOfxStatOK
}

/// Answers the regions-of-interest action of an effect that reads its
/// whole source: all of it.
pub(crate) fn whole_source(fx: Fx, args: Props, out: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
    let roi = fx
        .clip(kOfxImageEffectSimpleSourceClipName)
        .rod(time)
        .unwrap_or(roi);
    out.set_dbls(c"OfxImageClipPropRoI_Source", &roi);
    kOfxStatOK
}

/// A Gaussian's taps for a sigma of `sigma` pixels: three sigmas each way,
/// adding up to 1 (VEGAS's kernels end there: its widest blur measures so).
/// One tap, nothing blurred, under a fifth of a pixel.
pub(crate) fn gaussian(sigma: f64) -> Vec<f32> {
    if sigma.is_nan() || sigma < 0.2 {
        return vec![1.0];
    }
    let r = (3.0 * sigma).ceil() as usize;
    let taps: Vec<f64> = (0..=2 * r)
        .map(|i| {
            let x = (i as f64 - r as f64) / sigma;
            (-0.5 * x * x).exp()
        })
        .collect();
    let sum: f64 = taps.iter().sum();
    taps.iter().map(|t| (t / sum) as f32).collect()
}

/// `data` — `w` × `h` pixels of `c` numbers each, row after row — through
/// the taps `kx` across and `ky` up and down (odd counts, centered), the
/// picture's edge pixels held beyond it; rows on the host's threads.
pub(crate) fn convolve(
    data: &[f32],
    w: usize,
    h: usize,
    c: usize,
    kx: &[f32],
    ky: &[f32],
) -> Vec<f32> {
    let n = w * c;
    if n == 0 || h == 0 || data.len() < n * h {
        return data.to_vec();
    }
    let across = if kx.len() > 1 {
        let r = kx.len() / 2;
        let mut across = vec![0.0f32; n * h];
        let rows: Vec<Mutex<&mut [f32]>> = across.chunks_mut(n).map(Mutex::new).collect();
        for_rows(0..h as c_int, &|y| {
            let y = y as usize;
            let line = &data[y * n..(y + 1) * n];
            // The row with its edge pixels repeated `r` times each side.
            let mut padded = Vec::with_capacity(n + 2 * r * c);
            for _ in 0..r {
                padded.extend_from_slice(&line[..c]);
            }
            padded.extend_from_slice(line);
            for _ in 0..r {
                padded.extend_from_slice(&line[n - c..]);
            }
            let mut sum = vec![0.0f32; n];
            for (t, &k) in kx.iter().enumerate() {
                for (s, &v) in sum.iter_mut().zip(&padded[t * c..t * c + n]) {
                    *s += k * v;
                }
            }
            if let Ok(mut row) = rows[y].lock() {
                row.copy_from_slice(&sum);
            }
        });
        drop(rows);
        across
    } else {
        data[..n * h].to_vec()
    };
    if ky.len() <= 1 {
        return across;
    }
    let r = (ky.len() / 2) as isize;
    let mut out = vec![0.0f32; n * h];
    let rows: Vec<Mutex<&mut [f32]>> = out.chunks_mut(n).map(Mutex::new).collect();
    for_rows(0..h as c_int, &|y| {
        let mut sum = vec![0.0f32; n];
        for (t, &k) in ky.iter().enumerate() {
            let j = (y as isize + t as isize - r).clamp(0, h as isize - 1) as usize;
            for (s, &v) in sum.iter_mut().zip(&across[j * n..(j + 1) * n]) {
                *s += k * v;
            }
        }
        if let Ok(mut row) = rows[y as usize].lock() {
            row.copy_from_slice(&sum);
        }
    });
    drop(rows);
    out
}

/// A picture's pixels through [`convolve`] (premultiplied, all four
/// channels).
pub(crate) fn convolve_buf(src: &Buf, kx: &[f32], ky: &[f32]) -> Vec<[f32; 4]> {
    let (w, h) = (src.width().max(0) as usize, src.height().max(0) as usize);
    convolve(src.data.as_flattened(), w, h, 4, kx, ky)
        .as_chunks::<4>()
        .0
        .to_vec()
}

/// The straight color of a premultiplied pixel (black where it has none).
pub(crate) fn straight(p: [f32; 4]) -> [f32; 3] {
    if p[3] > 0.0 {
        [p[0] / p[3], p[1] / p[3], p[2] / p[3]]
    } else {
        [0.0; 3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_taps_are_a_gaussian_three_sigmas_each_way() {
        let k = gaussian(2.0);
        assert_eq!(k.len(), 13);
        assert!((k.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        // Symmetric, highest in the middle, e^-1/2 one sigma away.
        assert_eq!(k[0], k[12]);
        assert!((k[8] / k[6] - (-0.5f32).exp()).abs() < 1e-6);
        assert_eq!(gaussian(0.1), vec![1.0]);
        assert_eq!(gaussian(0.0), vec![1.0]);
    }

    #[test]
    fn the_edges_are_held() {
        // A row of 5: 1 at its left end, blurred across by [1/4 1/2 1/4]:
        // the pixel before the edge is the edge's own.
        let data = [1.0, 0.0, 0.0, 0.0, 0.0];
        let out = convolve(&data, 5, 1, 1, &[0.25, 0.5, 0.25], &[1.0]);
        assert_eq!(out, vec![0.75, 0.25, 0.0, 0.0, 0.0]);
        // The same up and down: a column of 5.
        let out = convolve(&data, 1, 5, 1, &[1.0], &[0.25, 0.5, 0.25]);
        assert_eq!(out, vec![0.75, 0.25, 0.0, 0.0, 0.0]);
        // A flat picture stays flat, whatever the blur.
        let flat = vec![0.4f32; 6 * 4 * 2];
        let k = gaussian(3.0);
        for v in convolve(&flat, 6, 4, 2, &k, &k) {
            assert!((v - 0.4).abs() < 1e-6);
        }
    }
}
