//! Pictures convolved through the frequency domain, as VEGAS's Defocus and
//! Starburst convolve theirs, whatever the kernel's size: the picture (its
//! edges held) and the kernel on grids of powers of two, each value two
//! complex numbers — red and green, blue and alpha —, transformed by
//! Stockham's radix-2 steps (the card's `fft_pass` takes the same ones),
//! multiplied and taken back.

use std::ffi::c_int;

use crate::kit::{Buf, for_chunks};

/// The grid's size for a picture `w` × `h` convolved with a kernel
/// reaching `reach` pixels each way: powers of two holding the picture and
/// the kernel's reach on both sides, so the circular convolution never
/// wraps into what is kept.
pub(crate) fn grid_size(w: usize, h: usize, reach: usize) -> (usize, usize) {
    (
        (w + 2 * reach).next_power_of_two().max(2),
        (h + 2 * reach).next_power_of_two().max(2),
    )
}

/// One of Stockham's radix-2 steps over a line of `src.len()` values,
/// written to `dst`: butterflies half the line apart, of half size `p`;
/// `sign` −1 forward, 1 back.
fn step(src: &[[f32; 4]], dst: &mut [[f32; 4]], p: usize, sign: f32) {
    let half = src.len() / 2;
    for j in 0..half {
        let k = j & (p - 1);
        let angle = sign * std::f32::consts::PI * k as f32 / p as f32;
        let (s, c) = angle.sin_cos();
        let (a, b) = (src[j], src[j + half]);
        let wb = [
            c * b[0] - s * b[1],
            c * b[1] + s * b[0],
            c * b[2] - s * b[3],
            c * b[3] + s * b[2],
        ];
        let o = (j - k) * 2 + k;
        dst[o] = [a[0] + wb[0], a[1] + wb[1], a[2] + wb[2], a[3] + wb[3]];
        dst[o + p] = [a[0] - wb[0], a[1] - wb[1], a[2] - wb[2], a[3] - wb[3]];
    }
}

/// A line transformed in place.
fn line(data: &mut [[f32; 4]], sign: f32) {
    let n = data.len();
    let mut other = vec![[0.0f32; 4]; n];
    let mut p = 1;
    let mut in_data = true;
    while p < n {
        if in_data {
            step(data, &mut other, p, sign);
        } else {
            step(&other, data, p, sign);
        }
        in_data = !in_data;
        p *= 2;
    }
    if !in_data {
        data.copy_from_slice(&other);
    }
}

/// `data` (`w` wide, `h` high) turned so its columns become rows.
fn transposed(data: &[[f32; 4]], w: usize, h: usize) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0f32; 4]; w * h];
    for_chunks(&mut out, h, |x, col| {
        for (y, v) in col.iter_mut().enumerate() {
            *v = data[y * w + x];
        }
    });
    out
}

/// A grid's rows transformed, then its columns: the result turned (`h`
/// rows of `w`… become `w` rows of `h`), as the multiplication wants it.
fn forward(mut data: Vec<[f32; 4]>, w: usize, h: usize) -> Vec<[f32; 4]> {
    for_chunks(&mut data, w, |_, row| line(row, -1.0));
    let mut t = transposed(&data, w, h);
    for_chunks(&mut t, h, |_, row| line(row, -1.0));
    t
}

/// [`forward`] undone (without the division by the grid's size).
fn back(mut t: Vec<[f32; 4]>, w: usize, h: usize) -> Vec<[f32; 4]> {
    for_chunks(&mut t, h, |_, row| line(row, 1.0));
    let mut data = transposed(&t, h, w);
    for_chunks(&mut data, w, |_, row| line(row, 1.0));
    data
}

/// `src` convolved with the `size` × `size` `kernel` (rows up, its middle
/// at `size / 2`), each pixel through `pre` first and the result through
/// `post` — the picture's edges held. Rows go up, as `Buf`'s.
pub(crate) fn convolve(
    src: &Buf,
    kernel: &[f32],
    size: usize,
    pre: &(dyn Fn([f32; 4]) -> [f32; 4] + Sync),
    post: &(dyn Fn(c_int, c_int, [f32; 4]) -> [f32; 4] + Sync),
) -> Buf {
    let cells: Vec<[f32; 4]> = kernel.iter().map(|&v| [v, 0.0, v, 0.0]).collect();
    convolve_lanes(src, &cells, size, pre, post)
}

/// [`convolve`] with a kernel of two complex numbers a cell, one for each
/// of the picture's: a light in both real parts comes out red and green,
/// blue and alpha (Glint's colored streaks).
pub(crate) fn convolve_lanes(
    src: &Buf,
    kernel: &[[f32; 4]],
    size: usize,
    pre: &(dyn Fn([f32; 4]) -> [f32; 4] + Sync),
    post: &(dyn Fn(c_int, c_int, [f32; 4]) -> [f32; 4] + Sync),
) -> Buf {
    convolve_with(src, kernel, size, Edges::Held, pre, post)
}

/// How the picture goes on past its edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edges {
    /// The edge pixels repeated.
    Held,
    /// Reflected about the edge pixels (… c b | a b c … d e | d c …).
    Reflected,
}

impl Edges {
    /// The pixel standing for `i` on a line `n` long.
    pub(crate) fn at(self, i: isize, n: isize) -> isize {
        match self {
            Edges::Held => i.clamp(0, n - 1),
            Edges::Reflected if n == 1 => 0,
            Edges::Reflected => {
                let period = 2 * n - 2;
                let m = i.rem_euclid(period);
                if m < n { m } else { period - m }
            }
        }
    }
}

/// [`convolve_lanes`] with the picture going on past its edges as `edges`
/// says.
pub(crate) fn convolve_with(
    src: &Buf,
    kernel: &[[f32; 4]],
    size: usize,
    edges: Edges,
    pre: &(dyn Fn([f32; 4]) -> [f32; 4] + Sync),
    post: &(dyn Fn(c_int, c_int, [f32; 4]) -> [f32; 4] + Sync),
) -> Buf {
    let (w, h) = (src.width().max(0) as usize, src.height().max(0) as usize);
    if w == 0 || h == 0 || size == 0 {
        return src.clone();
    }
    let reach = size / 2;
    let (gw, gh) = grid_size(w, h, reach);
    let [x0, y0] = [src.bounds[0], src.bounds[1]];
    // The picture, `reach` in from the grid's corner, going on past its
    // edges as `edges` says.
    let mut image = vec![[0.0f32; 4]; gw * gh];
    for_chunks(&mut image, gw, |gy, row| {
        let y = edges.at(gy as isize - reach as isize, h as isize) as c_int;
        for (gx, v) in row.iter_mut().enumerate() {
            let x = edges.at(gx as isize - reach as isize, w as isize) as c_int;
            *v = pre(src.at(x0 + x, y0 + y));
        }
    });
    // The kernel, its middle at the grid's corner.
    let mut k = vec![[0.0f32; 4]; gw * gh];
    for ky in 0..size {
        for kx in 0..size {
            let gx = (kx + gw - reach) % gw;
            let gy = (ky + gh - reach) % gh;
            k[gy * gw + gx] = kernel[ky * size + kx];
        }
    }
    let k = forward(k, gw, gh);
    let mut f = forward(image, gw, gh);
    // Each complex number times the kernel's.
    for (v, q) in f.iter_mut().zip(&k) {
        *v = [
            v[0] * q[0] - v[1] * q[1],
            v[0] * q[1] + v[1] * q[0],
            v[2] * q[2] - v[3] * q[3],
            v[2] * q[3] + v[3] * q[2],
        ];
    }
    let out = back(f, gw, gh);
    let scale = 1.0 / (gw * gh) as f32;
    src.map(|x, y| {
        let gx = (x - x0) as usize + reach;
        let gy = (y - y0) as usize + reach;
        let v = out[gy * gw + gx];
        post(
            x,
            y,
            [v[0] * scale, v[1] * scale, v[2] * scale, v[3] * scale],
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The transform of a line and back is the line, times its length;
    /// an impulse's transform is flat.
    #[test]
    fn a_line_goes_and_comes_back() {
        let n = 16;
        let original: Vec<[f32; 4]> = (0..n)
            .map(|i| [i as f32, (i * i % 7) as f32, -(i as f32), 1.0])
            .collect();
        let mut v = original.clone();
        line(&mut v, -1.0);
        line(&mut v, 1.0);
        for (a, b) in v.iter().zip(&original) {
            for c in 0..4 {
                assert!((a[c] / n as f32 - b[c]).abs() < 1e-4, "{a:?} {b:?}");
            }
        }
        let mut impulse = vec![[0.0f32; 4]; n];
        impulse[0] = [1.0, 0.0, 0.0, 0.0];
        line(&mut impulse, -1.0);
        assert!(
            impulse
                .iter()
                .all(|v| (v[0] - 1.0).abs() < 1e-6 && v[1].abs() < 1e-6)
        );
    }

    /// A one-pixel kernel leaves the picture; a box spreads a dot into
    /// itself, the edges held.
    #[test]
    fn a_kernel_spreads_a_dot() {
        let mut b = Buf::solid([0, 0, 20, 12], [0.0, 0.0, 0.0, 1.0]);
        b.data[5 * 20 + 7] = [1.0, 0.5, 0.25, 1.0];
        let same = convolve(&b, &[1.0], 1, &|c| c, &|_, _, c| c);
        for (a, c) in same.data.iter().zip(&b.data) {
            for k in 0..4 {
                assert!((a[k] - c[k]).abs() < 1e-5);
            }
        }
        let box3 = vec![1.0 / 9.0; 9];
        let spread = convolve(&b, &box3, 3, &|c| c, &|_, _, c| c);
        assert!((spread.at(6, 4)[0] - 1.0 / 9.0).abs() < 1e-5);
        assert!((spread.at(8, 6)[1] - 0.5 / 9.0).abs() < 1e-5);
        assert!(spread.at(9, 5)[0].abs() < 1e-5);
        // Alpha stays whole: every pixel's neighbours are opaque.
        assert!((spread.at(0, 0)[3] - 1.0).abs() < 1e-5);
    }

    /// Reflected edges: the pixels past each end are those before it, the
    /// end itself once.
    #[test]
    fn reflected_edges_mirror_about_the_end_pixels() {
        let line: Vec<isize> = (-3..8).map(|i| Edges::Reflected.at(i, 5)).collect();
        assert_eq!(line, [3, 2, 1, 0, 1, 2, 3, 4, 3, 2, 1]);
        assert_eq!(Edges::Reflected.at(-7, 1), 0);
        assert_eq!(Edges::Held.at(-2, 5), 0);
        // A bright edge column blurred across: reflected, the column past
        // it is the one before, not the edge again.
        let mut b = Buf::solid([0, 0, 6, 1], [0.0; 4]);
        b.data[0] = [1.0; 4];
        b.data[1] = [0.5; 4];
        let k = [[1.0 / 3.0, 0.0, 1.0 / 3.0, 0.0]; 3];
        let row: Vec<[f32; 4]> = (0..3)
            .map(|i| if i == 1 { k[0] } else { [0.0; 4] })
            .collect();
        let mut kernel = vec![[0.0f32; 4]; 9];
        kernel[3..6].copy_from_slice(&k);
        let _ = row;
        let out = convolve_with(&b, &kernel, 3, Edges::Reflected, &|c| c, &|_, _, c| c);
        assert!(
            (out.at(0, 0)[0] - 2.0 / 3.0).abs() < 1e-5,
            "{:?}",
            out.at(0, 0)
        );
    }
}
