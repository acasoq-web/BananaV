//! 3 × 3 matrices on column vectors, RGB spaces from their primaries'
//! chromaticities, and chromatic adaptation: Bradford, as ACES's own
//! transforms use, and CAT02, as the camera makers' matrices to ACES.

/// A 3 × 3 matrix, rows of columns: `m · v`.
pub type M3 = [[f64; 3]; 3];

pub const IDENTITY: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

pub fn mul(a: &M3, b: &M3) -> M3 {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

/// `k` times the identity.
pub fn scale(k: f64) -> M3 {
    [[k, 0.0, 0.0], [0.0, k, 0.0], [0.0, 0.0, k]]
}

pub fn apply(m: &M3, v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

pub fn inverse(m: &M3) -> M3 {
    let [[a, b, c], [d, e, f], [g, h, i]] = *m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    let k = 1.0 / det;
    [
        [
            (e * i - f * h) * k,
            (c * h - b * i) * k,
            (b * f - c * e) * k,
        ],
        [
            (f * g - d * i) * k,
            (a * i - c * g) * k,
            (c * d - a * f) * k,
        ],
        [
            (d * h - e * g) * k,
            (b * g - a * h) * k,
            (a * e - b * d) * k,
        ],
    ]
}

pub fn transpose(m: &M3) -> M3 {
    [0, 1, 2].map(|i| [0, 1, 2].map(|j| m[j][i]))
}

/// An RGB space's primaries and white, as CIE 1931 x, y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Primaries {
    pub r: [f64; 2],
    pub g: [f64; 2],
    pub b: [f64; 2],
    pub w: [f64; 2],
}

pub const D65: [f64; 2] = [0.3127, 0.3290];
/// ACES's white (≈ D60).
pub const ACES_WHITE: [f64; 2] = [0.32168, 0.33767];

pub const AP0: Primaries = Primaries {
    r: [0.7347, 0.2653],
    g: [0.0, 1.0],
    b: [0.0001, -0.077],
    w: ACES_WHITE,
};
pub const AP1: Primaries = Primaries {
    r: [0.713, 0.293],
    g: [0.165, 0.830],
    b: [0.128, 0.044],
    w: ACES_WHITE,
};
pub const REC709: Primaries = Primaries {
    r: [0.64, 0.33],
    g: [0.30, 0.60],
    b: [0.15, 0.06],
    w: D65,
};
pub const REC2020: Primaries = Primaries {
    r: [0.708, 0.292],
    g: [0.170, 0.797],
    b: [0.131, 0.046],
    w: D65,
};
pub const P3_D65: Primaries = Primaries {
    r: [0.680, 0.320],
    g: [0.265, 0.690],
    b: [0.150, 0.060],
    w: D65,
};
pub const P3_D60: Primaries = Primaries {
    w: ACES_WHITE,
    ..P3_D65
};
pub const P3_DCI: Primaries = Primaries {
    w: [0.314, 0.351],
    ..P3_D65
};
pub const ARRI_WG3: Primaries = Primaries {
    r: [0.684, 0.313],
    g: [0.221, 0.848],
    b: [0.0861, -0.102],
    w: D65,
};
pub const ARRI_WG4: Primaries = Primaries {
    r: [0.7347, 0.2653],
    g: [0.1424, 0.8576],
    b: [0.0991, -0.0308],
    w: D65,
};
pub const SONY_SGAMUT3: Primaries = Primaries {
    r: [0.730, 0.280],
    g: [0.140, 0.855],
    b: [0.100, -0.050],
    w: D65,
};
pub const SONY_SGAMUT3_CINE: Primaries = Primaries {
    r: [0.766, 0.275],
    g: [0.225, 0.800],
    b: [0.089, -0.087],
    w: D65,
};
pub const SONY_VENICE_SGAMUT3: Primaries = Primaries {
    r: [0.740464264304292, 0.279364374750660],
    g: [0.089241145423286, 0.893809528608105],
    b: [0.110488236673827, -0.052579333080476],
    w: D65,
};
pub const SONY_VENICE_SGAMUT3_CINE: Primaries = Primaries {
    r: [0.775901871567345, 0.274502392854799],
    g: [0.188682902773355, 0.828684937020288],
    b: [0.101337382499301, -0.089187517306263],
    w: D65,
};
pub const CANON_CGAMUT: Primaries = Primaries {
    r: [0.74, 0.27],
    g: [0.17, 1.14],
    b: [0.08, -0.10],
    w: D65,
};
pub const RED_WG: Primaries = Primaries {
    r: [0.780308, 0.304253],
    g: [0.121595, 1.493994],
    b: [0.095612, -0.084589],
    w: D65,
};
pub const PANASONIC_VGAMUT: Primaries = Primaries {
    r: [0.730, 0.280],
    g: [0.165, 0.840],
    b: [0.100, -0.030],
    w: D65,
};
pub const BMD_WG_GEN5: Primaries = Primaries {
    r: [0.7177215, 0.3171181],
    g: [0.2280410, 0.8615690],
    b: [0.1005841, -0.0820452],
    w: D65,
};
pub const DAVINCI_WG: Primaries = Primaries {
    r: [0.8000, 0.3130],
    g: [0.1682, 0.9877],
    b: [0.0790, -0.1155],
    w: D65,
};

fn xyz_of(xy: [f64; 2]) -> [f64; 3] {
    [xy[0] / xy[1], 1.0, (1.0 - xy[0] - xy[1]) / xy[1]]
}

/// RGB (these primaries) → CIE XYZ, white at Y 1.
pub fn rgb_to_xyz(p: &Primaries) -> M3 {
    let [r, g, b] = [p.r, p.g, p.b].map(xyz_of);
    let m = transpose(&[r, g, b]);
    let s = apply(&inverse(&m), xyz_of(p.w));
    [0, 1, 2].map(|i| [m[i][0] * s[0], m[i][1] * s[1], m[i][2] * s[2]])
}

pub fn xyz_to_rgb(p: &Primaries) -> M3 {
    inverse(&rgb_to_xyz(p))
}

pub const BRADFORD: M3 = [
    [0.8951, 0.2664, -0.1614],
    [-0.7502, 1.7135, 0.0367],
    [0.0389, -0.0685, 1.0296],
];

/// CIECAM02's adaptation space: the camera makers' (ARRI's, Sony's)
/// published matrices to ACES are worked out through it.
pub const CAT02: M3 = [
    [0.7328, 0.4296, -0.1624],
    [-0.7036, 1.6975, 0.0061],
    [0.0030, 0.0136, 0.9834],
];

/// A von Kries adaptation in the space of `cat` from white `from` to
/// white `to` (XYZ → XYZ).
pub fn adapt(cat: &M3, from: [f64; 2], to: [f64; 2]) -> M3 {
    if from == to {
        return IDENTITY;
    }
    let s = apply(cat, xyz_of(from));
    let d = apply(cat, xyz_of(to));
    let k = [
        [d[0] / s[0], 0.0, 0.0],
        [0.0, d[1] / s[1], 0.0],
        [0.0, 0.0, d[2] / s[2]],
    ];
    mul(&inverse(cat), &mul(&k, cat))
}

/// The Bradford adaptation from white `from` to white `to` (XYZ → XYZ).
pub fn bradford(from: [f64; 2], to: [f64; 2]) -> M3 {
    adapt(&BRADFORD, from, to)
}

/// RGB in `from` → RGB in `to`, the whites adapted by Bradford.
pub fn rgb_to_rgb(from: &Primaries, to: &Primaries) -> M3 {
    rgb_to_rgb_by(&BRADFORD, from, to)
}

/// RGB in `from` → RGB in `to`, the whites adapted in the space of `cat`.
pub fn rgb_to_rgb_by(cat: &M3, from: &Primaries, to: &Primaries) -> M3 {
    mul(
        &xyz_to_rgb(to),
        &mul(&adapt(cat, from.w, to.w), &rgb_to_xyz(from)),
    )
}

/// RGB in `from` → CIE XYZ under D65 (Bradford), the display connection.
pub fn rgb_to_xyz_d65(from: &Primaries) -> M3 {
    mul(&bradford(from.w, D65), &rgb_to_xyz(from))
}

/// A matrix of a 4 × 4 one written by rows (OpenColorIO's), its offset
/// apart.
pub fn of_rows16(m: &[f64]) -> (M3, [f64; 3]) {
    (
        [[m[0], m[1], m[2]], [m[4], m[5], m[6]], [m[8], m[9], m[10]]],
        [0.0; 3],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: &M3, b: &M3, by: f64) -> bool {
        (0..3).all(|i| (0..3).all(|j| (a[i][j] - b[i][j]).abs() <= by))
    }

    #[test]
    fn known_matrices() {
        // ACES2065-1 → ACEScg, as S-2014-004 gives it.
        let ap0_ap1 = rgb_to_rgb(&AP0, &AP1);
        let want = [
            [1.4514393161, -0.2365107469, -0.2149285693],
            [-0.0765537734, 1.1762296998, -0.0996759264],
            [0.0083161484, -0.0060324498, 0.9977163014],
        ];
        assert!(near(&ap0_ap1, &want, 1e-9), "{ap0_ap1:?}");
        // AP0 → XYZ (S-2008-001).
        let want = [
            [0.9525523959, 0.0, 0.0000936786],
            [0.3439664498, 0.7281660966, -0.0721325464],
            [0.0, 0.0, 1.0088251844],
        ];
        assert!(near(&rgb_to_xyz(&AP0), &want, 1e-9));
        // Rec.709 → XYZ, D65 as its x, y (0.3127, 0.3290).
        let want = [
            [0.4123907993, 0.3575843394, 0.1804807884],
            [0.2126390059, 0.7151686788, 0.0721923154],
            [0.0193308187, 0.1191947798, 0.9505321522],
        ];
        assert!(near(&rgb_to_xyz(&REC709), &want, 1e-9));
        // Round trips.
        let m = rgb_to_rgb(&REC709, &AP0);
        assert!(near(&mul(&rgb_to_rgb(&AP0, &REC709), &m), &IDENTITY, 1e-12));
        // White stays white across whites.
        let w = apply(&m, [1.0; 3]);
        assert!(w.iter().all(|v| (v - 1.0).abs() < 1e-9), "{w:?}");
    }

    // The makers' digits, as they publish them.
    #[allow(clippy::excessive_precision)]
    #[test]
    fn camera_makers_matrices() {
        // ARRI's IDT (ALEXA LogC3, EI 800): ARRI Wide Gamut 3 → AP0.
        let want = [
            [0.680206, 0.236137, 0.083658],
            [0.085415, 1.017471, -0.102886],
            [0.002057, -0.062563, 1.060506],
        ];
        assert!(near(&rgb_to_rgb_by(&CAT02, &ARRI_WG3, &AP0), &want, 1e-6));
        // ARRI's LogC4 IDT: ARRI Wide Gamut 4 → AP0.
        let want = [
            [
                0.750957362824734131,
                0.144422786709757084,
                0.104619850465508965,
            ],
            [
                0.000821837079380207,
                1.007397584885003194,
                -0.008219421964383583,
            ],
            [
                -0.000499952143533471,
                -0.000854177231436971,
                1.001354129374970370,
            ],
        ];
        assert!(near(&rgb_to_rgb_by(&CAT02, &ARRI_WG4, &AP0), &want, 1e-9));
        // S-Gamut3 and S-Gamut3.Cine → AP0, as their maker's IDTs.
        let want = [
            [0.7529825954, 0.1433702162, 0.1036471884],
            [0.0217076974, 1.0153188355, -0.0370265329],
            [-0.0094160528, 0.0033704179, 1.0060456349],
        ];
        assert!(near(
            &rgb_to_rgb_by(&CAT02, &SONY_SGAMUT3, &AP0),
            &want,
            1e-9
        ));
        let want = [
            [0.6387886672, 0.2723514337, 0.0888598991],
            [-0.0039159060, 1.0880732309, -0.0841573249],
            [-0.0299072021, -0.0264325799, 1.0563397820],
        ];
        assert!(near(
            &rgb_to_rgb_by(&CAT02, &SONY_SGAMUT3_CINE, &AP0),
            &want,
            1e-9
        ));
    }
}
