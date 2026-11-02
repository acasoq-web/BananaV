//! OpenColorIO's BuiltinTransforms BananaV does, by their styles' names:
//! ACES's encodings, Looks and SDR output transforms, cameras' log
//! encodings and gamuts to ACES2065-1, and displays' encodings from the
//! CIE XYZ (D65) display connection space.

use std::sync::Arc;

use crate::aces::{self, SdrOutput, Surround};
use crate::curves::Curve;
use crate::math::{self, AP0, AP1, BRADFORD, CAT02, D65, M3, Primaries};
use crate::ops::{Negatives, Op};

/// A camera's log decoded, then its gamut to AP0 — the whites adapted as
/// its maker's matrices are: CAT02 for ARRI's, Sony's, Panasonic's and
/// Canon's, Bradford for RED's and for Apple Log's Rec. 2020.
fn to_aces(curve: Curve, gamut: &Primaries, cat: &M3) -> Vec<Op> {
    vec![
        Op::Curve(curve, true),
        Op::Matrix(math::rgb_to_rgb_by(cat, gamut, &AP0), [0.0; 3]),
    ]
}

/// Canon Log (2 or 3) with Cinema Gamut to AP0, its linear scaled by 0.9.
fn canon(curve: Curve) -> Vec<Op> {
    let m = math::rgb_to_rgb_by(&CAT02, &math::CANON_CGAMUT, &AP0);
    vec![
        Op::Curve(curve, true),
        Op::Matrix(math::mul(&m, &math::scale(0.9)), [0.0; 3]),
    ]
}

/// CIE XYZ (D65) → a display's RGB (its white adapted from D65 by
/// Bradford), clipped to what it shows (as ACES's ODTs clip), then its
/// encoding.
fn display(p: &Primaries, encoding: Op) -> Vec<Op> {
    let m = math::mul(&math::xyz_to_rgb(p), &math::bradford(D65, p.w));
    let clip = Op::Range {
        scale: 1.0,
        offset: 0.0,
        lo: Some(0.0),
        hi: Some(1.0),
    };
    vec![Op::Matrix(m, [0.0; 3]), clip, encoding]
}

/// The same for HDR's PQ displays, 1 being 100 cd/m²: up to 10,000.
fn display_pq(p: &Primaries) -> Vec<Op> {
    let m = math::mul(&math::xyz_to_rgb(p), &math::bradford(D65, p.w));
    let clip = Op::Range {
        scale: 1.0,
        offset: 0.0,
        lo: Some(0.0),
        hi: Some(100.0),
    };
    vec![
        Op::Matrix(m, [0.0; 3]),
        clip,
        Op::Curve(Curve::Pq { scale: 100.0 }, false),
    ]
}

fn power(gamma: f64) -> Op {
    Op::Power([1.0 / gamma; 3], Negatives::Clamp)
}

/// A builtin's steps, forward; `None` when BananaV does not know it.
pub fn ops(style: &str) -> Option<Vec<Op>> {
    Some(match style {
        "IDENTITY" => Vec::new(),
        "UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD" => {
            vec![Op::Matrix(math::rgb_to_xyz_d65(&AP0), [0.0; 3])]
        }
        "UTILITY - ACES-AP1_to_CIE-XYZ-D65_BFD" => {
            vec![Op::Matrix(math::rgb_to_xyz_d65(&AP1), [0.0; 3])]
        }
        "UTILITY - ACES-AP1_to_LINEAR-REC709_BFD" => {
            vec![Op::Matrix(math::rgb_to_rgb(&AP1, &math::REC709), [0.0; 3])]
        }
        "UTILITY - ACES-AP0_to_AP1" | "ACES-AP0_to_AP1" => {
            vec![Op::Matrix(math::rgb_to_rgb(&AP0, &AP1), [0.0; 3])]
        }
        "CURVE - ACEScct-LOG_to_LINEAR" => vec![Op::Curve(Curve::AcesCct, true)],
        // AP1 and AP0 share their white: no adaptation.
        "ACEScct_to_ACES2065-1" => to_aces(Curve::AcesCct, &AP1, &BRADFORD),
        "ACEScc_to_ACES2065-1" => to_aces(Curve::AcesCc, &AP1, &BRADFORD),
        "ACESproxy10i_to_ACES2065-1" => to_aces(Curve::AcesProxy10, &AP1, &BRADFORD),
        "ACEScg_to_ACES2065-1" => vec![Op::Matrix(math::rgb_to_rgb(&AP1, &AP0), [0.0; 3])],
        "ACES-LMT - BLUE_LIGHT_ARTIFACT_FIX" => {
            vec![Op::Matrix(aces::blue_light_fix(), [0.0; 3])]
        }
        "ACES-LMT - ACES 1.3 Reference Gamut Compression" => vec![Op::GamutCompress(false)],
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO_1.0" => {
            vec![Op::AcesSdr(Arc::new(SdrOutput::new(Surround::Video, D65)))]
        }
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA_1.0" => {
            vec![Op::AcesSdr(Arc::new(SdrOutput::new(Surround::Cinema, D65)))]
        }
        "ARRI_ALEXA-LOGC-EI800-AWG_to_ACES2065-1" => {
            to_aces(Curve::ArriLogC3Ei800, &math::ARRI_WG3, &CAT02)
        }
        "ARRI_LOGC4_to_ACES2065-1" => to_aces(Curve::ArriLogC4, &math::ARRI_WG4, &CAT02),
        "CURVE - ARRI_LOGC4_to_LINEAR" => vec![Op::Curve(Curve::ArriLogC4, true)],
        "SONY_SLOG3-SGAMUT3_to_ACES2065-1" => {
            to_aces(Curve::SonySlog3, &math::SONY_SGAMUT3, &CAT02)
        }
        "SONY_SLOG3-SGAMUT3.CINE_to_ACES2065-1" => {
            to_aces(Curve::SonySlog3, &math::SONY_SGAMUT3_CINE, &CAT02)
        }
        "SONY_SLOG3-SGAMUT3-VENICE_to_ACES2065-1" => {
            to_aces(Curve::SonySlog3, &math::SONY_VENICE_SGAMUT3, &CAT02)
        }
        "SONY_SLOG3-SGAMUT3.CINE-VENICE_to_ACES2065-1" => {
            to_aces(Curve::SonySlog3, &math::SONY_VENICE_SGAMUT3_CINE, &CAT02)
        }
        "CURVE - SONY_SLOG3_to_LINEAR" => vec![Op::Curve(Curve::SonySlog3, true)],
        // Canon's logs reach 1 at 90% reflectance: their linear is scaled
        // by 0.9, as Canon's IDTs and OpenColorIO's builtins do.
        "CANON_CLOG2-CGAMUT_to_ACES2065-1" => canon(Curve::CanonLog2),
        "CANON_CLOG3-CGAMUT_to_ACES2065-1" => canon(Curve::CanonLog3),
        "CURVE - CANON_CLOG2_to_LINEAR" => vec![
            Op::Curve(Curve::CanonLog2, true),
            Op::Matrix(math::scale(0.9), [0.0; 3]),
        ],
        "CURVE - CANON_CLOG3_to_LINEAR" => vec![
            Op::Curve(Curve::CanonLog3, true),
            Op::Matrix(math::scale(0.9), [0.0; 3]),
        ],
        "PANASONIC_VLOG-VGAMUT_to_ACES2065-1" => {
            to_aces(Curve::PanasonicVlog, &math::PANASONIC_VGAMUT, &CAT02)
        }
        "CURVE - PANASONIC_VLOG_to_LINEAR" => vec![Op::Curve(Curve::PanasonicVlog, true)],
        "RED_LOG3G10-RWG_to_ACES2065-1" => to_aces(Curve::RedLog3G10, &math::RED_WG, &BRADFORD),
        "CURVE - RED_LOG3G10_to_LINEAR" => vec![Op::Curve(Curve::RedLog3G10, true)],
        "APPLE_LOG_to_ACES2065-1" => to_aces(Curve::AppleLog, &math::REC2020, &BRADFORD),
        "CURVE - APPLE_LOG_to_LINEAR" => vec![Op::Curve(Curve::AppleLog, true)],
        "CURVE - ST-2084_to_LINEAR" => vec![Op::Curve(Curve::Pq { scale: 100.0 }, true)],
        "CURVE - LINEAR_to_ST-2084" => vec![Op::Curve(Curve::Pq { scale: 100.0 }, false)],
        "DISPLAY - CIE-XYZ-D65_to_sRGB" => display(&math::REC709, Op::Curve(Curve::Srgb, false)),
        "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709" => display(&math::REC709, power(2.4)),
        "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020" => display(&math::REC2020, power(2.4)),
        "DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709" => display(&math::REC709, power(2.2)),
        "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65" => display(&math::P3_D65, power(2.6)),
        "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-DCI-BFD" => display(&math::P3_DCI, power(2.6)),
        "DISPLAY - CIE-XYZ-D65_to_DisplayP3" => {
            display(&math::P3_D65, Op::Curve(Curve::Srgb, false))
        }
        "DISPLAY - CIE-XYZ-D65_to_REC.2100-PQ" => display_pq(&math::REC2020),
        "DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65" => display_pq(&math::P3_D65),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(style: &str, c: [f64; 3]) -> [f64; 3] {
        ops(style)
            .unwrap_or_else(|| panic!("{style}"))
            .iter()
            .fold(c, |c, op| op.apply(c))
    }

    #[test]
    fn acescct_and_acescg_reach_aces2065_1() {
        // 18% grey through ACEScct: 0.4135884 is ACES's 0.18.
        let g = run("ACEScct_to_ACES2065-1", [0.413_588_4; 3]);
        assert!(g.iter().all(|v| (v - 0.18).abs() < 1e-6), "{g:?}");
        let g = run("ACEScg_to_ACES2065-1", [0.18; 3]);
        assert!(g.iter().all(|v| (v - 0.18).abs() < 1e-9), "{g:?}");
        // A camera's grey: S-Log3's 420 is 18%.
        let g = run("SONY_SLOG3-SGAMUT3.CINE_to_ACES2065-1", [420.0 / 1023.0; 3]);
        assert!(g.iter().all(|v| (v - 0.18).abs() < 1e-6), "{g:?}");
    }

    #[test]
    fn canons_logs_scale_their_linear_by_0_9() {
        // Canon Log 2's black is its 0.092864125; its curve's 0.2 is
        // 18% grey once scaled by 0.9 (Canon's 90% reflectance white).
        let k = run("CURVE - CANON_CLOG2_to_LINEAR", [0.092_864_125; 3]);
        assert!(k.iter().all(|v| v.abs() < 1e-12), "{k:?}");
        let x = Curve::CanonLog2.from_linear(0.2);
        let g = run("CURVE - CANON_CLOG2_to_LINEAR", [x; 3]);
        assert!(g.iter().all(|v| (v - 0.18).abs() < 1e-9), "{g:?}");
        let x = Curve::CanonLog3.from_linear(0.2);
        let g = run("CANON_CLOG3-CGAMUT_to_ACES2065-1", [x; 3]);
        // A grey stays grey through Cinema Gamut's matrix.
        assert!(g.iter().all(|v| (v - 0.18).abs() < 1e-6), "{g:?}");
    }

    #[test]
    fn displays_encode_their_white() {
        // D65 white in XYZ: the displays' full signal.
        let white = math::apply(&math::rgb_to_xyz(&math::REC709), [1.0; 3]);
        for style in [
            "DISPLAY - CIE-XYZ-D65_to_sRGB",
            "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709",
            "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020",
            "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65",
        ] {
            let c = run(style, white);
            assert!(c.iter().all(|v| (v - 1.0).abs() < 1e-6), "{style}: {c:?}");
        }
        // PQ: the display connection's 1 is 100 cd/m².
        let c = run("DISPLAY - CIE-XYZ-D65_to_REC.2100-PQ", white);
        assert!(c.iter().all(|v| (v - 0.508_078).abs() < 1e-4), "{c:?}");
    }
}
