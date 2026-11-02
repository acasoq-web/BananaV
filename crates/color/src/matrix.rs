//! Matrizes Y′CbCr ↔ R′G′B′ e normalização de faixa (ITU-T H.273).

use bv_core::{ColorRange, MatrixCoefficients};

/// Coeficientes (Kr, Kb) da matriz. `None` para a matriz identidade (GBR).
pub fn kr_kb(m: MatrixCoefficients) -> Option<(f32, f32)> {
    match m {
        MatrixCoefficients::Bt709 => Some((0.2126, 0.0722)),
        MatrixCoefficients::Bt601 => Some((0.299, 0.114)),
        MatrixCoefficients::Bt2020Ncl => Some((0.2627, 0.0593)),
        MatrixCoefficients::Identity => None,
    }
}

/// Converte valores inteiros para Y′ ∈ [0, 1] e Cb, Cr ∈ [−0,5, 0,5].
#[derive(Clone, Copy, Debug)]
pub struct Normalizer {
    y_off: f32,
    y_range: f32,
    y_scale: f32,
    c_off: f32,
    c_range: f32,
    c_scale: f32,
}

impl Normalizer {
    pub fn new(range: ColorRange, bit_depth: u8) -> Normalizer {
        let s = (1u32 << (bit_depth - 8)) as f32;
        let max = ((1u32 << bit_depth) - 1) as f32;
        let half = (1u32 << (bit_depth - 1)) as f32;
        let (y_off, y_range, c_off, c_range) = match range {
            ColorRange::Limited => (16.0 * s, 219.0 * s, 128.0 * s, 224.0 * s),
            ColorRange::Full => (0.0, max, half, max),
        };
        Normalizer {
            y_off,
            y_range,
            y_scale: 1.0 / y_range,
            c_off,
            c_range,
            c_scale: 1.0 / c_range,
        }
    }

    /// Luma's offset and scale, then chroma's (for the graphics card).
    pub fn params(&self) -> [f32; 4] {
        [self.y_off, self.y_scale, self.c_off, self.c_scale]
    }

    /// Luma's offset and range, then chroma's: what
    /// [`luma_inv`](Self::luma_inv) and [`chroma_inv`](Self::chroma_inv)
    /// use (for the graphics card).
    pub fn inverse_params(&self) -> [f32; 4] {
        [self.y_off, self.y_range, self.c_off, self.c_range]
    }

    #[inline]
    pub fn luma(&self, v: f32) -> f32 {
        (v - self.y_off) * self.y_scale
    }

    #[inline]
    pub fn chroma(&self, v: f32) -> f32 {
        (v - self.c_off) * self.c_scale
    }

    #[inline]
    pub fn luma_inv(&self, y: f32) -> f32 {
        y * self.y_range + self.y_off
    }

    #[inline]
    pub fn chroma_inv(&self, c: f32) -> f32 {
        c * self.c_range + self.c_off
    }
}

/// Y′CbCr normalizado → R′G′B′.
#[inline]
pub fn ycbcr_to_rgb(kr: f32, kb: f32, y: f32, cb: f32, cr: f32) -> [f32; 3] {
    let kg = 1.0 - kr - kb;
    let r = y + 2.0 * (1.0 - kr) * cr;
    let b = y + 2.0 * (1.0 - kb) * cb;
    let g = (y - kr * r - kb * b) / kg;
    [r, g, b]
}

/// R′G′B′ → Y′CbCr normalizado.
#[inline]
pub fn rgb_to_ycbcr(kr: f32, kb: f32, rgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb;
    let y = kr * r + (1.0 - kr - kb) * g + kb * b;
    let cb = (b - y) / (2.0 * (1.0 - kb));
    let cr = (r - y) / (2.0 * (1.0 - kr));
    [y, cb, cr]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ida_e_volta() {
        for m in [
            MatrixCoefficients::Bt709,
            MatrixCoefficients::Bt601,
            MatrixCoefficients::Bt2020Ncl,
        ] {
            let (kr, kb) = kr_kb(m).unwrap();
            for rgb in [[1.0, 0.0, 0.0], [0.2, 0.7, 0.4], [1.0, 1.0, 1.0], [0.0; 3]] {
                let ycc = rgb_to_ycbcr(kr, kb, rgb);
                let back = ycbcr_to_rgb(kr, kb, ycc[0], ycc[1], ycc[2]);
                for i in 0..3 {
                    assert!((back[i] - rgb[i]).abs() < 1e-5, "{m:?} {rgb:?}");
                }
            }
        }
    }

    #[test]
    fn faixa_limitada() {
        let n = Normalizer::new(ColorRange::Limited, 8);
        let close = |a: f32, b: f32| assert!((a - b).abs() < 1e-6, "{a} != {b}");
        close(n.luma(16.0), 0.0);
        close(n.luma(235.0), 1.0);
        close(n.chroma(128.0), 0.0);
        close(n.chroma(240.0), 0.5);
        let n10 = Normalizer::new(ColorRange::Limited, 10);
        close(n10.luma(940.0), 1.0);
        close(n10.chroma_inv(-0.5), 64.0);
    }

    #[test]
    fn faixa_completa() {
        let n = Normalizer::new(ColorRange::Full, 8);
        assert!((n.luma(255.0) - 1.0).abs() < 1e-6);
        assert!(n.chroma(128.0).abs() < 1e-6);
    }
}
