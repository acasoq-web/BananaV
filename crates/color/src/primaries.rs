//! Conversão de gamut entre primárias, via XYZ.

use bv_core::ColorPrimaries;

pub type Mat3 = [[f32; 3]; 3];

pub const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Cromaticidades xy de R, G, B e do branco (todas com branco D65).
fn chromaticities(p: ColorPrimaries) -> [[f64; 2]; 4] {
    const D65: [f64; 2] = [0.3127, 0.3290];
    match p {
        ColorPrimaries::Bt709 => [[0.640, 0.330], [0.300, 0.600], [0.150, 0.060], D65],
        ColorPrimaries::Bt601_625 => [[0.640, 0.330], [0.290, 0.600], [0.150, 0.060], D65],
        ColorPrimaries::Bt601_525 => [[0.630, 0.340], [0.310, 0.595], [0.155, 0.070], D65],
        ColorPrimaries::Bt2020 => [[0.708, 0.292], [0.170, 0.797], [0.131, 0.046], D65],
        ColorPrimaries::DisplayP3 => [[0.680, 0.320], [0.265, 0.690], [0.150, 0.060], D65],
    }
}

type Mat3d = [[f64; 3]; 3];

fn mul(a: &Mat3d, b: &Mat3d) -> Mat3d {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

fn invert(m: &Mat3d) -> Mat3d {
    let [[a, b, c], [d, e, f], [g, h, i]] = *m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    assert!(det.abs() > 1e-12, "matriz singular");
    let inv = 1.0 / det;
    [
        [
            (e * i - f * h) * inv,
            (c * h - b * i) * inv,
            (b * f - c * e) * inv,
        ],
        [
            (f * g - d * i) * inv,
            (a * i - c * g) * inv,
            (c * d - a * f) * inv,
        ],
        [
            (d * h - e * g) * inv,
            (b * g - a * h) * inv,
            (a * e - b * d) * inv,
        ],
    ]
}

/// Matriz RGB linear → XYZ (SMPTE RP 177).
fn rgb_to_xyz(p: ColorPrimaries) -> Mat3d {
    let c = chromaticities(p);
    let xyz = |[x, y]: [f64; 2]| [x / y, 1.0, (1.0 - x - y) / y];
    let [r, g, b, w] = [xyz(c[0]), xyz(c[1]), xyz(c[2]), xyz(c[3])];
    let m = [[r[0], g[0], b[0]], [r[1], g[1], b[1]], [r[2], g[2], b[2]]];
    let inv = invert(&m);
    let s: Vec<f64> = (0..3)
        .map(|i| (0..3).map(|k| inv[i][k] * w[k]).sum())
        .collect();
    let mut out = m;
    for row in &mut out {
        for (j, v) in row.iter_mut().enumerate() {
            *v *= s[j];
        }
    }
    out
}

/// Matriz que leva RGB linear em `from` para RGB linear em `to`.
pub fn conversion(from: ColorPrimaries, to: ColorPrimaries) -> Mat3 {
    if from == to {
        return IDENTITY;
    }
    let m = mul(&invert(&rgb_to_xyz(to)), &rgb_to_xyz(from));
    m.map(|row| row.map(|v| v as f32))
}

#[inline]
pub fn apply(m: &Mat3, v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_para_xyz_bt709_conhecido() {
        // Linha Y da matriz BT.709 = coeficientes de luminância.
        let m = rgb_to_xyz(ColorPrimaries::Bt709);
        assert!((m[1][0] - 0.2126).abs() < 1e-4);
        assert!((m[1][1] - 0.7152).abs() < 1e-4);
        assert!((m[1][2] - 0.0722).abs() < 1e-4);
    }

    #[test]
    fn bt2020_para_bt709() {
        // Matriz conhecida (ITU-R BT.2087): primeira linha ≈ 1,6605 −0,5876 −0,0728.
        let m = conversion(ColorPrimaries::Bt2020, ColorPrimaries::Bt709);
        assert!((m[0][0] - 1.6605).abs() < 1e-3);
        assert!((m[0][1] + 0.5876).abs() < 1e-3);
        assert!((m[0][2] + 0.0728).abs() < 1e-3);
    }

    #[test]
    fn branco_continua_branco() {
        for p in [
            ColorPrimaries::Bt601_525,
            ColorPrimaries::Bt601_625,
            ColorPrimaries::Bt2020,
            ColorPrimaries::DisplayP3,
        ] {
            let w = apply(&conversion(p, ColorPrimaries::Bt709), [1.0, 1.0, 1.0]);
            for c in w {
                assert!((c - 1.0).abs() < 1e-4, "{p:?}");
            }
        }
    }
}
