//! Transfer curves, each both ways: displays' (sRGB's, BT.1886's, pure
//! gammas, SMPTE ST 2084's PQ, BT.2100's HLG), ACES's log encodings
//! (ACEScc, ACEScct, ACESproxy) and cameras' (ARRI LogC3 and LogC4, Sony
//! S-Log3, Canon Log 2 and 3, Panasonic V-Log, RED Log3G10, Blackmagic
//! Film Gen 5, DaVinci Intermediate, Apple Log), from their makers'
//! published formulas.

/// A curve on one channel, `fwd` from its encoding to linear light (or,
/// for displays, from linear to the signal: see each).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    /// sRGB's piecewise curve (IEC 61966-2-1): signal → linear.
    Srgb,
    /// A monitor curve: (x + offset) ⁄ (1 + offset) to `gamma`, a line
    /// below (signal → linear; OpenColorIO's ExponentWithLinear).
    MonCurve {
        gamma: f64,
        offset: f64,
    },
    /// A pure power: signal → linear is x^gamma (negatives mirrored when
    /// `mirror`, else clamped to 0).
    Gamma {
        gamma: f64,
        mirror: bool,
    },
    /// SMPTE ST 2084: signal → linear, 1 = 10 000 cd/m² (OpenColorIO's
    /// builtin divides by 100: 1 = 100 cd/m²: `scale`).
    Pq {
        scale: f64,
    },
    /// BT.2100 HLG's OETF inverse: signal → scene linear (0–1).
    Hlg,
    AcesCc,
    AcesCct,
    /// ACESproxy, 10-bit code values over 0–1.
    AcesProxy10,
    ArriLogC3Ei800,
    ArriLogC4,
    SonySlog3,
    CanonLog2,
    CanonLog3,
    PanasonicVlog,
    RedLog3G10,
    BmdFilmGen5,
    DavinciIntermediate,
    AppleLog,
}

const HALF_MAX: f64 = 65504.0;

impl Curve {
    /// The encoded value's linear light.
    pub fn to_linear(self, x: f64) -> f64 {
        match self {
            Curve::Srgb => {
                if x.abs() <= 0.04045 {
                    x / 12.92
                } else {
                    x.signum() * ((x.abs() + 0.055) / 1.055).powf(2.4)
                }
            }
            Curve::MonCurve { gamma, offset } => moncurve_f(x, gamma, offset),
            Curve::Gamma { gamma, mirror } => {
                if mirror {
                    x.signum() * x.abs().powf(gamma)
                } else {
                    x.max(0.0).powf(gamma)
                }
            }
            Curve::Pq { scale } => pq_to_linear(x) * scale,
            Curve::Hlg => {
                const A: f64 = 0.17883277;
                const B: f64 = 0.28466892;
                const C: f64 = 0.55991073;
                if x <= 0.5 {
                    x.max(0.0).powi(2) / 3.0
                } else {
                    (((x - C) / A).exp() + B) / 12.0
                }
            }
            Curve::AcesCc => {
                if x < (9.72 - 15.0) / 17.52 {
                    (2f64.powf(x * 17.52 - 9.72) - 2f64.powi(-16)) * 2.0
                } else if x < (HALF_MAX.log2() + 9.72) / 17.52 {
                    2f64.powf(x * 17.52 - 9.72)
                } else {
                    HALF_MAX
                }
            }
            Curve::AcesCct => {
                const A: f64 = 10.540_237_741_654_5;
                const B: f64 = 0.072_905_534_195_835_5;
                const Y_BRK: f64 = 0.155_251_141_552_511;
                if x <= Y_BRK {
                    (x - B) / A
                } else if x < (HALF_MAX.log2() + 9.72) / 17.52 {
                    2f64.powf(x * 17.52 - 9.72)
                } else {
                    HALF_MAX
                }
            }
            Curve::AcesProxy10 => {
                let cv = x * 1023.0;
                2f64.powf((cv - 425.0) / 50.0 - 2.5)
            }
            Curve::ArriLogC3Ei800 => {
                const CUT: f64 = 0.010591;
                const A: f64 = 5.555556;
                const B: f64 = 0.052272;
                const C: f64 = 0.247190;
                const D: f64 = 0.385537;
                const E: f64 = 5.367655;
                const F: f64 = 0.092809;
                if x > E * CUT + F {
                    (10f64.powf((x - D) / C) - B) / A
                } else {
                    (x - F) / E
                }
            }
            Curve::ArriLogC4 => {
                let a = (2f64.powi(18) - 16.0) / 117.45;
                let b = (1023.0 - 95.0) / 1023.0;
                let c = 95.0 / 1023.0;
                let s = (7.0 * 2f64.ln() * 2f64.powf(7.0 - 14.0 * c / b)) / (a * b);
                let t = (2f64.powf(14.0 * (-c / b) + 6.0) - 64.0) / a;
                if x >= 0.0 {
                    (2f64.powf(14.0 * (x - c) / b + 6.0) - 64.0) / a
                } else {
                    x * s + t
                }
            }
            Curve::SonySlog3 => {
                if x >= 171.2102946929 / 1023.0 {
                    10f64.powf((x * 1023.0 - 420.0) / 261.5) * (0.18 + 0.01) - 0.01
                } else {
                    (x * 1023.0 - 95.0) * 0.01125 / (171.2102946929 - 95.0)
                }
            }
            Curve::CanonLog2 => {
                if x < 0.092864125 {
                    -(10f64.powf((0.092864125 - x) / 0.24136077) - 1.0) / 87.09937546
                } else {
                    (10f64.powf((x - 0.092864125) / 0.24136077) - 1.0) / 87.09937546
                }
            }
            Curve::CanonLog3 => {
                if x < 0.097465473 {
                    -(10f64.powf((0.12783901 - x) / 0.36726845) - 1.0) / 14.98325
                } else if x <= 0.15277891 {
                    (x - 0.12512219) / 1.9754798
                } else {
                    (10f64.powf((x - 0.12240537) / 0.36726845) - 1.0) / 14.98325
                }
            }
            Curve::PanasonicVlog => {
                const CUT2: f64 = 0.181;
                const B: f64 = 0.00873;
                const C: f64 = 0.241514;
                const D: f64 = 0.598206;
                if x < CUT2 {
                    (x - 0.125) / 5.6
                } else {
                    10f64.powf((x - D) / C) - B
                }
            }
            Curve::RedLog3G10 => {
                const A: f64 = 0.224282;
                const B: f64 = 155.975327;
                const C: f64 = 0.01;
                const G: f64 = 15.1927;
                if x < 0.0 {
                    x / G - C
                } else {
                    (10f64.powf(x / A) - 1.0) / B - C
                }
            }
            Curve::BmdFilmGen5 => {
                const A: f64 = 0.08692876065491224;
                const B: f64 = 0.005494072432257808;
                const C: f64 = 0.5300133392291939;
                const D: f64 = 8.283605932402494;
                const E: f64 = 0.09246575342465753;
                const LIN_CUT: f64 = 0.005;
                let log_cut = D * LIN_CUT + E;
                if x < log_cut {
                    (x - E) / D
                } else {
                    ((x - C) / A).exp() - B
                }
            }
            Curve::DavinciIntermediate => {
                const A: f64 = 0.0075;
                const B: f64 = 7.0;
                const C: f64 = 0.07329248;
                const M: f64 = 10.44426855;
                const LOG_CUT: f64 = 0.02740668;
                if x <= LOG_CUT {
                    x / M
                } else {
                    2f64.powf(x / C - B) - A
                }
            }
            Curve::AppleLog => {
                const R0: f64 = -0.05641088;
                const RT: f64 = 0.01;
                const C: f64 = 47.28711236;
                const B: f64 = 0.00964052;
                const G: f64 = 0.08550479;
                const D: f64 = 0.69336945;
                let pt = C * (RT - R0).powi(2);
                if x >= pt {
                    2f64.powf((x - D) / G) - B
                } else if x >= 0.0 {
                    (x / C).sqrt() + R0
                } else {
                    R0
                }
            }
        }
    }

    /// Linear light's encoded value.
    pub fn from_linear(self, y: f64) -> f64 {
        match self {
            Curve::Srgb => {
                if y.abs() <= 0.0031308 {
                    y * 12.92
                } else {
                    y.signum() * (1.055 * y.abs().powf(1.0 / 2.4) - 0.055)
                }
            }
            Curve::MonCurve { gamma, offset } => moncurve_r(y, gamma, offset),
            Curve::Gamma { gamma, mirror } => {
                if mirror {
                    y.signum() * y.abs().powf(1.0 / gamma)
                } else {
                    y.max(0.0).powf(1.0 / gamma)
                }
            }
            Curve::Pq { scale } => linear_to_pq(y / scale),
            Curve::Hlg => {
                const A: f64 = 0.17883277;
                const B: f64 = 0.28466892;
                const C: f64 = 0.55991073;
                if y <= 1.0 / 12.0 {
                    (3.0 * y.max(0.0)).sqrt()
                } else {
                    A * (12.0 * y - B).ln() + C
                }
            }
            Curve::AcesCc => {
                if y <= 0.0 {
                    (-16.0 + 9.72) / 17.52
                } else if y < 2f64.powi(-15) {
                    ((2f64.powi(-16) + y * 0.5).log2() + 9.72) / 17.52
                } else {
                    (y.log2() + 9.72) / 17.52
                }
            }
            Curve::AcesCct => {
                const A: f64 = 10.540_237_741_654_5;
                const B: f64 = 0.072_905_534_195_835_5;
                const X_BRK: f64 = 0.0078125;
                if y <= X_BRK {
                    A * y + B
                } else {
                    (y.log2() + 9.72) / 17.52
                }
            }
            Curve::AcesProxy10 => {
                // The specification's code values, not rounded to whole
                // ones here: a float pipeline goes back and forth.
                let cv = if y <= 2f64.powf(-9.72) {
                    64.0
                } else {
                    ((y.log2() + 2.5) * 50.0 + 425.0).clamp(64.0, 940.0)
                };
                cv / 1023.0
            }
            Curve::ArriLogC3Ei800 => {
                const CUT: f64 = 0.010591;
                const A: f64 = 5.555556;
                const B: f64 = 0.052272;
                const C: f64 = 0.247190;
                const D: f64 = 0.385537;
                const E: f64 = 5.367655;
                const F: f64 = 0.092809;
                if y > CUT {
                    C * (A * y + B).log10() + D
                } else {
                    E * y + F
                }
            }
            Curve::ArriLogC4 => {
                let a = (2f64.powi(18) - 16.0) / 117.45;
                let b = (1023.0 - 95.0) / 1023.0;
                let c = 95.0 / 1023.0;
                let s = (7.0 * 2f64.ln() * 2f64.powf(7.0 - 14.0 * c / b)) / (a * b);
                let t = (2f64.powf(14.0 * (-c / b) + 6.0) - 64.0) / a;
                if y >= t {
                    ((a * y + 64.0).log2() - 6.0) / 14.0 * b + c
                } else {
                    (y - t) / s
                }
            }
            Curve::SonySlog3 => {
                if y >= 0.01125 {
                    (420.0 + ((y + 0.01) / (0.18 + 0.01)).log10() * 261.5) / 1023.0
                } else {
                    (y * (171.2102946929 - 95.0) / 0.01125 + 95.0) / 1023.0
                }
            }
            Curve::CanonLog2 => {
                if y < 0.0 {
                    -0.24136077 * (1.0 - 87.09937546 * y).log10() + 0.092864125
                } else {
                    0.24136077 * (87.09937546 * y + 1.0).log10() + 0.092864125
                }
            }
            Curve::CanonLog3 => {
                if y < -0.014 {
                    -0.36726845 * (1.0 - 14.98325 * y).log10() + 0.12783901
                } else if y <= 0.014 {
                    1.9754798 * y + 0.12512219
                } else {
                    0.36726845 * (14.98325 * y + 1.0).log10() + 0.12240537
                }
            }
            Curve::PanasonicVlog => {
                const CUT1: f64 = 0.01;
                const B: f64 = 0.00873;
                const C: f64 = 0.241514;
                const D: f64 = 0.598206;
                if y < CUT1 {
                    5.6 * y + 0.125
                } else {
                    C * (y + B).log10() + D
                }
            }
            Curve::RedLog3G10 => {
                const A: f64 = 0.224282;
                const B: f64 = 155.975327;
                const C: f64 = 0.01;
                const G: f64 = 15.1927;
                let y = y + C;
                if y < 0.0 {
                    y * G
                } else {
                    A * (y * B + 1.0).log10()
                }
            }
            Curve::BmdFilmGen5 => {
                const A: f64 = 0.08692876065491224;
                const B: f64 = 0.005494072432257808;
                const C: f64 = 0.5300133392291939;
                const D: f64 = 8.283605932402494;
                const E: f64 = 0.09246575342465753;
                const LIN_CUT: f64 = 0.005;
                if y < LIN_CUT {
                    D * y + E
                } else {
                    A * (y + B).ln() + C
                }
            }
            Curve::DavinciIntermediate => {
                const A: f64 = 0.0075;
                const B: f64 = 7.0;
                const C: f64 = 0.07329248;
                const M: f64 = 10.44426855;
                const LIN_CUT: f64 = 0.00262409;
                if y <= LIN_CUT {
                    y * M
                } else {
                    ((y + A).log2() + B) * C
                }
            }
            Curve::AppleLog => {
                const R0: f64 = -0.05641088;
                const RT: f64 = 0.01;
                const C: f64 = 47.28711236;
                const B: f64 = 0.00964052;
                const G: f64 = 0.08550479;
                const D: f64 = 0.69336945;
                if y >= RT {
                    G * (y + B).log2() + D
                } else if y >= R0 {
                    C * (y - R0).powi(2)
                } else {
                    0.0
                }
            }
        }
    }
}

/// ACES's monitor curve forward: signal → linear.
pub fn moncurve_f(x: f64, gamma: f64, offs: f64) -> f64 {
    let fs = ((gamma - 1.0) / offs) * (offs * gamma / ((gamma - 1.0) * (1.0 + offs))).powf(gamma);
    let xb = offs / (gamma - 1.0);
    if x >= xb {
        ((x + offs) / (1.0 + offs)).powf(gamma)
    } else {
        x * fs
    }
}

/// ACES's monitor curve reverse: linear → signal.
pub fn moncurve_r(y: f64, gamma: f64, offs: f64) -> f64 {
    let yb = (offs * gamma / ((gamma - 1.0) * (1.0 + offs))).powf(gamma);
    let rs = ((gamma - 1.0) / offs).powf(gamma - 1.0) * ((1.0 + offs) / gamma).powf(gamma);
    if y >= yb {
        (1.0 + offs) * y.powf(1.0 / gamma) - offs
    } else {
        y * rs
    }
}

/// BT.1886's inverse EOTF with a 0 black: linear (1 = white) → signal.
pub fn bt1886_r(l: f64, gamma: f64) -> f64 {
    l.max(0.0).powf(1.0 / gamma)
}

const PQ_M1: f64 = 0.1593017578125;
const PQ_M2: f64 = 78.84375;
const PQ_C1: f64 = 0.8359375;
const PQ_C2: f64 = 18.8515625;
const PQ_C3: f64 = 18.6875;

/// ST 2084 signal → linear, 1 = 10 000 cd/m².
pub fn pq_to_linear(n: f64) -> f64 {
    let np = n.max(0.0).powf(1.0 / PQ_M2);
    let l = (np - PQ_C1).max(0.0) / (PQ_C2 - PQ_C3 * np);
    l.powf(1.0 / PQ_M1)
}

/// Linear (1 = 10 000 cd/m²) → ST 2084 signal.
pub fn linear_to_pq(l: f64) -> f64 {
    let lm = l.max(0.0).powf(PQ_M1);
    ((PQ_C1 + PQ_C2 * lm) / (1.0 + PQ_C3 * lm)).powf(PQ_M2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64, by: f64) -> bool {
        (a - b).abs() <= by
    }

    #[test]
    fn known_values() {
        // ACES's log encodings put 18% grey at 0.4135884.
        assert!(near(Curve::AcesCc.from_linear(0.18), 0.413_588_4, 1e-6));
        assert!(near(Curve::AcesCct.from_linear(0.18), 0.413_588_4, 1e-6));
        // ACEScct's toe: 0 at 0.0729055.
        assert!(near(Curve::AcesCct.from_linear(0.0), 0.072_905_534, 1e-9));
        // ACESproxy: 18% grey at 426.3, the specification's 426 (10-bit)
        // once rounded.
        assert!(near(
            Curve::AcesProxy10.from_linear(0.18) * 1023.0,
            426.303,
            1e-3
        ));
        // Cameras' 18% grey (their makers' white papers).
        assert!(near(
            Curve::ArriLogC3Ei800.from_linear(0.18),
            0.391_007,
            1e-5
        ));
        assert!(near(
            Curve::SonySlog3.from_linear(0.18),
            420.0 / 1023.0,
            1e-9
        ));
        assert!(near(
            Curve::PanasonicVlog.from_linear(0.18),
            0.423_311,
            1e-5
        ));
        assert!(near(Curve::RedLog3G10.from_linear(0.18), 1.0 / 3.0, 1e-5));
        assert!(near(Curve::ArriLogC4.from_linear(0.18), 0.278_395_8, 1e-6));
        // PQ: 100 cd/m² at 0.508; HLG: 1/12 at 0.5.
        assert!(near(linear_to_pq(0.01), 0.508_078, 1e-5));
        assert!(near(Curve::Hlg.from_linear(1.0 / 12.0), 0.5, 1e-9));
        assert!(near(Curve::Srgb.to_linear(0.5), 0.214_041, 1e-6));
    }

    #[test]
    fn every_curve_goes_back_and_forth() {
        let curves = [
            Curve::Srgb,
            Curve::MonCurve {
                gamma: 2.4,
                offset: 0.055,
            },
            Curve::Gamma {
                gamma: 2.6,
                mirror: false,
            },
            Curve::Pq { scale: 100.0 },
            Curve::Hlg,
            Curve::AcesCc,
            Curve::AcesCct,
            Curve::ArriLogC3Ei800,
            Curve::ArriLogC4,
            Curve::SonySlog3,
            Curve::CanonLog2,
            Curve::CanonLog3,
            Curve::PanasonicVlog,
            Curve::RedLog3G10,
            Curve::BmdFilmGen5,
            Curve::DavinciIntermediate,
            Curve::AppleLog,
        ];
        for c in curves {
            for &y in &[0.001, 0.01, 0.05, 0.18, 0.5, 0.9, 1.0, 2.5] {
                let back = c.to_linear(c.from_linear(y));
                // V-Log's published constants leave its two pieces 3e-8
                // apart at its break (0.01): a round trip there changes
                // piece.
                assert!(near(back, y, 1e-5 * y.max(1e-3)), "{c:?} at {y}: {back}");
            }
        }
    }
}
