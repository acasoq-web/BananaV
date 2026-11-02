//! The 8×8 DCT of ISO/IEC 13818-2 (its definition, separable, in
//! floating point), its inverse, and the inverse quantisation a decoder
//! does (Section 7.4: saturation and mismatch control), so the encoder
//! predicts from the pictures decoders rebuild.

use std::sync::OnceLock;

/// cos((2x + 1)uπ/16) × C(u)/2, by [u][x].
fn basis() -> &'static [[f32; 8]; 8] {
    static B: OnceLock<[[f32; 8]; 8]> = OnceLock::new();
    B.get_or_init(|| {
        std::array::from_fn(|u| {
            std::array::from_fn(|x| {
                let c = if u == 0 {
                    std::f32::consts::FRAC_1_SQRT_2
                } else {
                    1.0
                };
                0.5 * c * ((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI / 16.0).cos()
            })
        })
    })
}

/// The forward DCT of a block (raster order).
pub(crate) fn forward(block: &[f32; 64]) -> [f32; 64] {
    let b = basis();
    let mut rows = [0.0f32; 64];
    for y in 0..8 {
        for u in 0..8 {
            rows[y * 8 + u] = (0..8).map(|x| b[u][x] * block[y * 8 + x]).sum();
        }
    }
    let mut out = [0.0f32; 64];
    for v in 0..8 {
        for u in 0..8 {
            out[v * 8 + u] = (0..8).map(|y| b[v][y] * rows[y * 8 + u]).sum();
        }
    }
    out
}

/// The inverse DCT, rounded to integers.
pub(crate) fn inverse(coefs: &[i32; 64]) -> [i32; 64] {
    let b = basis();
    let mut cols = [0.0f32; 64];
    // Vertically first: g(y, u) = Σ_v b[v][y] F(v, u).
    for y in 0..8 {
        for u in 0..8 {
            cols[y * 8 + u] = (0..8).map(|v| b[v][y] * coefs[v * 8 + u] as f32).sum();
        }
    }
    let mut out = [0i32; 64];
    for y in 0..8 {
        for x in 0..8 {
            let s: f32 = (0..8).map(|u| b[u][x] * cols[y * 8 + u]).sum();
            out[y * 8 + x] = s.round() as i32;
        }
    }
    out
}

/// A block's coefficients as a decoder rebuilds them from its levels
/// (raster order): intra (`intra_dc_mult` 8) or not, at `qscale` with
/// matrix `w`, saturated, with mismatch control — MPEG-2's (the sum made
/// odd) or MPEG-1's (each coefficient made odd).
pub(crate) fn dequantise(
    levels: &[i32; 64],
    w: &[u8; 64],
    qscale: i32,
    intra: bool,
    mpeg1: bool,
) -> [i32; 64] {
    let mut f = [0i32; 64];
    let mut sum = 0i32;
    for i in 0..64 {
        let q = levels[i];
        let v = if intra && i == 0 {
            8 * q
        } else if intra {
            (2 * q * w[i] as i32 * qscale) / 32
        } else if q == 0 {
            0
        } else {
            ((2 * q + q.signum()) * w[i] as i32 * qscale) / 32
        };
        let dc = intra && i == 0;
        let v = if mpeg1 && !dc && v & 1 == 0 {
            v - v.signum()
        } else {
            v
        };
        let v = v.clamp(-2048, 2047);
        f[i] = v;
        sum += v;
    }
    if !mpeg1 && sum & 1 == 0 {
        f[63] ^= 1;
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mpeg1_makes_each_coefficient_odd() {
        let mut levels = [0i32; 64];
        levels[0] = 10;
        levels[1] = 3;
        levels[2] = -2;
        let f = dequantise(&levels, &[16; 64], 4, true, true);
        // DC as it is; the rest odd, toward zero.
        assert_eq!(f[0], 80);
        assert_eq!((f[1], f[2]), (11, -7));
        assert_eq!(f[63], 0);
    }

    #[test]
    fn the_inverse_undoes_the_forward() {
        let block: [f32; 64] = std::array::from_fn(|i| ((i * 37) % 255) as f32);
        let f = forward(&block);
        assert!(
            (f[0] - block.iter().sum::<f32>() / 8.0).abs() < 1e-2,
            "DC is 8 × the mean"
        );
        let back = inverse(&std::array::from_fn(|i| f[i].round() as i32));
        for i in 0..64 {
            assert!((back[i] - block[i] as i32).abs() <= 1, "{i}");
        }
    }
}
