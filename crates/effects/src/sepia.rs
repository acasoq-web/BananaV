//! Sepia: VEGAS's. The picture's color taken away toward its gray, then
//! the gray tinted toward Color — Blending strength says how much of each,
//! Blending falloff spares the brighter parts the tint.
//!
//! VEGAS's math, measured on the calibration card through each of its
//! presets (8-bit, every one of its colors): the gray is the plain average
//! of red, green and blue (as Black and White's); with `A = 1.5 ×
//! strength`, each color goes toward its gray by `min(A, 1)`, then toward
//! Color by `A − 0.5 − falloff × gray` (within 0–1) — so up to a third of
//! the strength only takes color away, two thirds take it all, and the
//! falloff lets the tint fade out toward white. Every preset's color within
//! a level of VEGAS's, most exact. Its presets' colors are 8-bit values
//! (the picker shows them rounded in HSV: Neutral's 166, 166, 0 is "60,
//! 1.0, 0.65"). At its defaults (strength 0) it changes nothing.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.Sepia";

/// The parameters (BananaV's earlier Sepia's names, so its projects open).
const COLOR: &CStr = c"color";
const STRENGTH: &CStr = c"blendingStrength";
const FALLOFF: &CStr = c"blendingFalloff";

/// Color at first: VEGAS's, 166, 166, 0.
const COLOR_AT_FIRST: [f64; 3] = [166.0 / 255.0, 166.0 / 255.0, 0.0];

/// How far under the exact value BananaV's results come: a 32nd of an
/// 8-bit step, so their halves round down as VEGAS's do.
const TIE: f32 = 1.0 / (32.0 * 255.0);

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub color: [f64; 3],
    pub strength: f64,
    pub falloff: f64,
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        Settings {
            color: v.array(COLOR, COLOR_AT_FIRST),
            strength: v.f64(STRENGTH, 0.0),
            falloff: v.f64(FALLOFF, 0.0),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            color: fx.param(COLOR).rgb_at(time),
            strength: fx.param(STRENGTH).f64_at(time),
            falloff: fx.param(FALLOFF).f64_at(time),
        }
    }

    /// Whether it changes nothing.
    pub(crate) fn changes_nothing(&self) -> bool {
        self.strength <= 0.0
    }

    /// How far toward the gray, and how far toward Color before the
    /// falloff.
    fn amounts(&self) -> (f32, f32) {
        let a = 1.5 * self.strength.clamp(0.0, 1.0) as f32;
        (a.min(1.0), a - 0.5)
    }

    /// A color through it, negatives clipped.
    pub(crate) fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let (gray, tint) = self.amounts();
        let y = (c[0] + c[1] + c[2]) / 3.0;
        let t = (tint - self.falloff as f32 * y).clamp(0.0, 1.0);
        std::array::from_fn(|i| {
            let d = c[i] + (y - c[i]) * gray;
            (d + (self.color[i] as f32 - d) * t - TIE).max(0.0)
        })
    }

    /// Without falloff, the straight line the colors go through (a matrix
    /// and an offset); with it, `None`.
    pub(crate) fn affine(&self) -> Option<([[f32; 3]; 3], [f32; 3])> {
        if self.falloff != 0.0 {
            return None;
        }
        let (gray, tint) = self.amounts();
        let t = tint.clamp(0.0, 1.0);
        let keep = (1.0 - t) * (1.0 - gray);
        let mix = (1.0 - t) * gray / 3.0;
        let matrix =
            std::array::from_fn(|i| std::array::from_fn(|j| mix + if i == j { keep } else { 0.0 }));
        Some((
            matrix,
            std::array::from_fn(|i| t * self.color[i] as f32 - TIE),
        ))
    }
}

/// The plug-in's entry point.
pub(crate) unsafe extern "C" fn main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if suites().is_none() {
        return kOfxStatErrMissingHostFeature;
    }
    let fx = Fx(handle as OfxImageEffectHandle);
    let (args, out) = (Props(in_args), Props(out_args));
    if is(action, kOfxActionDescribe) {
        describe(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        describe_in_context(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        let s = Settings::at(fx, args.dbl(kOfxPropTime, 0));
        crate::colors::render(fx, args, &|c| s.apply(c))
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        if Settings::at(fx, args.dbl(kOfxPropTime, 0)).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxActionLoad)
        || is(action, kOfxActionUnload)
        || is(action, kOfxActionCreateInstance)
        || is(action, kOfxActionDestroyInstance)
    {
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Sepia");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"The picture in one tone, as old movies: sepia, or any color you like.",
    );
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextFilter, kOfxImageEffectContextGeneral],
    );
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    p.set_int(kOfxImageEffectPropSupportsTiles, 1);
    p.set_int(kOfxImageEffectPropSupportsMultiResolution, 1);
    p.set_int(kOfxImageEffectPluginPropHostFrameThreading, 0);
    p.set_str(
        kOfxImageEffectPluginRenderThreadSafety,
        kOfxImageEffectRenderFullySafe,
    );
}

/// A number from 0 to 1 with VEGAS's three decimals.
fn number(fx: Fx, name: &CStr, label: &CStr, hint: &CStr) {
    let p = fx.define_param(kOfxParamTypeDouble, name, label);
    p.set_dbl(kOfxParamPropDefault, 0.0);
    p.set_dbl(kOfxParamPropMin, 0.0);
    p.set_dbl(kOfxParamPropMax, 1.0);
    p.set_dbl(kOfxParamPropDisplayMin, 0.0);
    p.set_dbl(kOfxParamPropDisplayMax, 1.0);
    p.set_dbl(kOfxParamPropIncrement, 0.001);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(kOfxParamPropHint, hint);
}

fn describe_in_context(fx: Fx) {
    let comps = [
        kOfxImageComponentRGBA,
        kOfxImageComponentRGB,
        kOfxImageComponentAlpha,
    ];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);
    // Its picker in HSV, as VEGAS's opens.
    let p = fx.define_param(kOfxParamTypeRGB, COLOR, c"Color");
    p.set_dbls(kOfxParamPropDefault, &COLOR_AT_FIRST);
    p.set_str(kOfxParamPropHint, c"The tone the picture takes.");
    number(
        fx,
        STRENGTH,
        c"Blending strength",
        c"How strongly the tone covers the picture: the color goes first, then the tone comes.",
    );
    number(
        fx,
        FALLOFF,
        c"Blending falloff",
        c"How much the tone spares the brighter parts, white the most.",
    );
}

/// The factory presets: VEGAS's, in its list's order (their colors 8-bit
/// values, as VEGAS's pictures show them).
pub(crate) fn presets() -> Vec<Preset> {
    let p = |name: &'static str, rgb: [f64; 3], strength: f64, falloff: f64| Preset {
        name,
        values: vec![
            ("color", PresetValue::Num(rgb.map(|v| v / 255.0).to_vec())),
            ("blendingStrength", PresetValue::Num(vec![strength])),
            ("blendingFalloff", PresetValue::Num(vec![falloff])),
        ],
    };
    vec![
        p("Neutral", [166.0, 166.0, 0.0], 0.72, 0.63),
        p("Warm", [203.0, 114.0, 0.0], 0.72, 0.63),
        p("Cold", [98.0, 140.0, 98.0], 0.72, 0.63),
        p("Old Photographs", [166.0, 63.0, 0.0], 0.51, 0.0),
        p("Old Newspaper", [166.0, 63.0, 0.0], 0.48, 0.0),
        p("Currency", [255.0, 211.0, 26.0], 0.48, 0.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(s: &Settings, c: [u8; 3]) -> [u8; 3] {
        s.apply(c.map(|v| v as f32 / 255.0))
            .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
    }

    fn preset(name: &str) -> Settings {
        let p = presets().into_iter().find(|p| p.name == name).unwrap();
        let num = |k: &str| match &p.values.iter().find(|(n, _)| *n == k).unwrap().1 {
            PresetValue::Num(v) => v.clone(),
            _ => unreachable!(),
        };
        let c = num("color");
        Settings {
            color: [c[0], c[1], c[2]],
            strength: num("blendingStrength")[0],
            falloff: num("blendingFalloff")[0],
        }
    }

    /// VEGAS's presets on the card (its 8-bit snapshots).
    #[test]
    fn the_presets_draw_what_vegas_draws() {
        for (name, cases) in [
            (
                "Neutral",
                &[
                    ([0, 0, 0], [96, 96, 0]),
                    ([128, 128, 128], [138, 138, 94]),
                    ([255, 255, 255], [255, 255, 255]),
                    ([255, 0, 0], [115, 115, 54]),
                    ([255, 255, 0], [169, 169, 143]),
                ][..],
            ),
            (
                "Old Newspaper",
                &[
                    ([0, 0, 0], [36, 14, 0]),
                    ([128, 128, 128], [136, 114, 100]),
                    ([255, 255, 255], [235, 213, 199]),
                    ([255, 0, 0], [140, 62, 48]),
                    ([0, 0, 255], [84, 62, 103]),
                ][..],
            ),
            (
                "Currency",
                &[
                    ([0, 0, 0], [56, 46, 6]),
                    ([255, 255, 255], [255, 245, 205]),
                    ([0, 255, 0], [104, 150, 53]),
                ][..],
            ),
        ] {
            let s = preset(name);
            for &(c, out) in cases {
                assert_eq!(bytes(&s, c), out, "{name} {c:?}");
            }
        }
    }

    /// At its defaults nothing changes; without falloff, the straight line
    /// the graphics card draws with gives the same colors.
    #[test]
    fn defaults_and_the_straight_line() {
        let s = Settings {
            color: COLOR_AT_FIRST,
            strength: 0.0,
            falloff: 0.0,
        };
        assert!(s.changes_nothing());
        assert_eq!(bytes(&s, [10, 200, 30]), [10, 200, 30]);
        let s = preset("Old Photographs");
        let (m, o) = s.affine().expect("no falloff");
        for c in [[0.1f32, 0.5, 0.9], [1.0, 0.0, 0.3]] {
            let line: [f32; 3] =
                std::array::from_fn(|i| m[i][0] * c[0] + m[i][1] * c[1] + m[i][2] * c[2] + o[i]);
            let direct = s.apply(c);
            for i in 0..3 {
                assert!((line[i] - direct[i]).abs() < 1e-6, "{c:?}");
            }
        }
        assert!(preset("Neutral").affine().is_none());
    }
}
