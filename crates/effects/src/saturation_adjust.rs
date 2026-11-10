//! Saturation Adjust: VEGAS's. The colors' saturation changed by how
//! saturated they are: by Amount around Center, a range Spread wide, by
//! Low below it and by High above it.
//!
//! VEGAS's math, measured on its pictures: each color (of the values the
//! host hands over: in BananaV's 8-bit projects, as VEGAS's, the encoded
//! ones) moves toward or away from its gray, BT.601's luma — the gray it
//! keeps when its color is taken away —, scaled by 1 + the adjustment for
//! its saturation; the saturation is the color's distance from that gray,
//! a part of pure green's (the farthest any color gets: 1). The adjustment
//! is Amount at Center and fades smoothly (a smoothstep) into Low below and
//! High above, reaching them Spread ⁄ 2 away from Center. The colors are
//! taken within 0–1 and the result is not clipped (in 32-bit projects
//! VEGAS's effects keep what passes white or black; 8-bit projects hold
//! each effect's result within 0–1).
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.SaturationAdjust";

const AMOUNT: &CStr = c"amount";
const CENTER: &CStr = c"center";
const SPREAD: &CStr = c"spread";
const LOW: &CStr = c"low";
const HIGH: &CStr = c"high";

/// BT.601's luma weights: a color's gray.
pub(crate) const LUMA: [f32; 3] = [0.299, 0.587, 0.114];

/// Pure green's distance from its gray, the farthest any color gets:
/// √(2 · 0.587² + 0.413²). Saturations count in it.
pub(crate) const GREEN: f32 = 0.927_204;

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub amount: f64,
    pub center: f64,
    pub spread: f64,
    pub low: f64,
    pub high: f64,
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        Settings {
            amount: v.f64(AMOUNT, 0.0),
            center: v.f64(CENTER, 0.5),
            spread: v.f64(SPREAD, 0.5).max(0.0),
            low: v.f64(LOW, 0.0),
            high: v.f64(HIGH, 0.0),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        let v = |name| fx.param(name).f64_at(time);
        Settings {
            amount: v(AMOUNT),
            center: v(CENTER),
            spread: v(SPREAD).max(0.0),
            low: v(LOW),
            high: v(HIGH),
        }
    }

    pub(crate) fn changes_nothing(&self) -> bool {
        self.amount == 0.0 && self.low == 0.0 && self.high == 0.0
    }

    /// How much the saturation of a color `s` saturated changes (−1 takes
    /// its color away, 1 doubles it): Amount at Center, fading smoothly
    /// into Low below and High above, which it reaches Spread ⁄ 2 away.
    pub(crate) fn adjustment(&self, s: f32) -> f32 {
        let center = self.center as f32;
        let half = (self.spread as f32 / 2.0).max(1e-6);
        let t = (1.0 - (s - center).abs() / half).clamp(0.0, 1.0);
        let w = t * t * (3.0 - 2.0 * t);
        let side = if s < center { self.low } else { self.high } as f32;
        self.amount as f32 * w + side * (1.0 - w)
    }

    /// A color adjusted.
    pub(crate) fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let c = c.map(|v| v.clamp(0.0, 1.0));
        let y = c[0] * LUMA[0] + c[1] * LUMA[1] + c[2] * LUMA[2];
        let d = c.map(|v| v - y);
        let s = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() / GREEN;
        let k = 1.0 + self.adjustment(s);
        d.map(|v| y + k * v)
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
    p.set_str(kOfxPropLabel, c"Saturation Adjust");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"Changes the colors' saturation by how saturated they are: a range of saturations, and the colors below and above it.",
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

fn describe_in_context(fx: Fx) {
    let comps = [
        kOfxImageComponentRGBA,
        kOfxImageComponentRGB,
        kOfxImageComponentAlpha,
    ];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);

    let number = |name: &CStr, label: &CStr, [min, max, default]: [f64; 3], hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, default);
        p.set_dbl(kOfxParamPropMin, min);
        p.set_dbl(kOfxParamPropMax, max);
        p.set_dbl(kOfxParamPropDisplayMin, min);
        p.set_dbl(kOfxParamPropDisplayMax, max);
        p.set_dbl(kOfxParamPropIncrement, 0.01);
        p.set_int(kOfxParamPropDigits, 3);
        p.set_str(kOfxParamPropHint, hint);
    };
    number(
        AMOUNT,
        c"Amount",
        [-1.0, 1.0, 0.0],
        c"How much the saturation of the colors around Center changes: -1 takes their color away, 1 doubles it.",
    );
    number(
        CENTER,
        c"Center",
        [0.0, 1.0, 0.5],
        c"The saturation Amount changes most, from gray (0) to the strongest colors (1).",
    );
    number(
        SPREAD,
        c"Spread",
        [0.0, 1.0, 0.5],
        c"How wide the range of saturations around Center is: across it, Amount fades into Low and High.",
    );
    number(
        LOW,
        c"Low",
        [-1.0, 1.0, 0.0],
        c"How much the saturation of the colors less saturated than the range changes.",
    );
    number(
        HIGH,
        c"High",
        [-1.0, 1.0, 0.0],
        c"How much the saturation of the colors more saturated than the range changes.",
    );
}

// Presets.

/// The factory presets: VEGAS's.
pub(crate) fn presets() -> Vec<Preset> {
    let preset = |name: &'static str, [amount, center, spread, low, high]: [f64; 5]| Preset {
        name,
        values: vec![
            ("amount", PresetValue::Num(vec![amount])),
            ("center", PresetValue::Num(vec![center])),
            ("spread", PresetValue::Num(vec![spread])),
            ("low", PresetValue::Num(vec![low])),
            ("high", PresetValue::Num(vec![high])),
        ],
    };
    vec![
        preset("Boost Midtones", [0.5, 0.4, 0.7, 0.0, 0.0]),
        preset("Reduce Minor Color Noise", [-1.0, 0.0, 0.3, -1.0, 0.0]),
        preset("Reduce Oversaturated Colors", [-0.3, 0.75, 0.8, 0.0, -0.3]),
        preset("Selectively Desaturate", [-1.0, 0.5, 0.16, 0.0, 0.0]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(amount: f64, center: f64, spread: f64, low: f64, high: f64) -> Settings {
        Settings {
            amount,
            center,
            spread,
            low,
            high,
        }
    }

    /// As an 8-bit project keeps it (VEGAS's pictures are 8-bit).
    fn bytes(c: [f32; 3]) -> [f32; 3] {
        c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round())
    }

    #[test]
    fn green_is_the_farthest_from_its_gray() {
        let far = |c: [f32; 3]| {
            let y = c[0] * LUMA[0] + c[1] * LUMA[1] + c[2] * LUMA[2];
            c.map(|v| (v - y) * (v - y)).iter().sum::<f32>().sqrt()
        };
        assert!((far([0.0, 1.0, 0.0]) - GREEN).abs() < 1e-6);
        assert!((far([1.0, 0.0, 1.0]) - GREEN).abs() < 1e-6);
        for c in [
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
        ] {
            assert!(far(c) < GREEN, "{c:?}");
        }
    }

    #[test]
    fn the_adjustment_fades_from_amount_into_low_and_high() {
        // VEGAS's pictures at Center 0.5, Spread 0.5: Low reached at 0.25,
        // High at 0.75, half way at 0.375 and 0.625.
        let low = s(0.0, 0.5, 0.5, -1.0, 0.0);
        assert_eq!(low.adjustment(0.1), -1.0);
        assert_eq!(low.adjustment(0.25), -1.0);
        assert!((low.adjustment(0.375) + 0.5).abs() < 1e-6);
        assert_eq!(low.adjustment(0.5), 0.0);
        assert_eq!(low.adjustment(0.9), 0.0);
        let high = s(0.0, 0.5, 0.5, 0.0, -1.0);
        assert_eq!(high.adjustment(0.3), 0.0);
        assert!((high.adjustment(0.625) + 0.5).abs() < 1e-6);
        assert_eq!(high.adjustment(0.8), -1.0);
        // Amount at Center, smoothly (a smoothstep) gone Spread / 2 away.
        let bump = s(-1.0, 0.5, 0.16, 0.0, 0.0);
        assert_eq!(bump.adjustment(0.5), -1.0);
        assert!((bump.adjustment(0.54) + 0.5).abs() < 1e-5);
        assert_eq!(bump.adjustment(0.42), 0.0);
        // No spread: Low below Center, High above.
        let step = s(0.5, 0.5, 0.0, -0.25, 0.75);
        assert_eq!(step.adjustment(0.49), -0.25);
        assert_eq!(step.adjustment(0.51), 0.75);
    }

    #[test]
    fn the_colors_change_as_vegass() {
        // VEGAS's pictures where they are flat (7 × 7 pixels' means): a
        // pale sky (saturation 0.24), the sky (0.44), the sand (0.52) and
        // the deep sky (0.78), through its presets and Low and High at
        // their ends.
        let color = |c: [f32; 3]| c.map(|v| v / 255.0);
        let pale = color([175.0, 244.0, 254.0]);
        let sky = color([121.0, 242.0, 255.0]);
        let sand = color([254.0, 197.0, 90.0]);
        let deep = color([16.0, 226.0, 255.0]);
        let boost = s(0.5, 0.4, 0.7, 0.0, 0.0);
        let reduce = s(-0.3, 0.75, 0.8, 0.0, -0.3);
        let selectively = s(-1.0, 0.5, 0.16, 0.0, 0.0);
        for (settings, c, vegas) in [
            (boost, pale, [160.0, 250.0, 254.0]),
            (boost, sky, [81.0, 255.0, 255.0]),
            (reduce, sand, [247.0, 199.0, 104.0]),
            (reduce, deep, [63.0, 207.0, 229.0]),
            // The sand nearly gray, the deep sky kept.
            (selectively, sand, [212.0, 202.0, 179.0]),
            (selectively, sky, [134.0, 236.0, 249.0]),
            (selectively, deep, [16.0, 226.0, 255.0]),
            // Low 1: the pale sky's color twice as strong; Low -1: gray.
            (s(0.0, 0.5, 0.5, 1.0, 0.0), pale, [123.0, 255.0, 254.0]),
            (s(0.0, 0.5, 0.5, -1.0, 0.0), pale, [224.0, 224.0, 224.0]),
            // High -1: the deep sky gray, the pale one kept.
            (s(0.0, 0.5, 0.5, 0.0, -1.0), deep, [168.0, 167.0, 168.0]),
            (s(0.0, 0.5, 0.5, 0.0, -1.0), pale, [175.0, 244.0, 254.0]),
        ] {
            let out = bytes(settings.apply(c));
            for k in 0..3 {
                assert!(
                    (out[k] - vegas[k]).abs() <= 4.5,
                    "{settings:?} {c:?}: {out:?}, VEGAS's {vegas:?}"
                );
            }
        }
    }

    #[test]
    fn the_default_changes_nothing() {
        assert!(s(0.0, 0.5, 0.5, 0.0, 0.0).changes_nothing());
        assert!(s(0.0, 0.2, 0.9, 0.0, 0.0).changes_nothing());
        assert!(!s(0.0, 0.5, 0.5, 0.1, 0.0).changes_nothing());
        let c = [0.3, 0.6, 0.9];
        let out = s(0.0, 0.5, 0.5, 0.0, 0.0).apply(c);
        assert!(c.iter().zip(out).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn the_presets_are_vegass() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            [
                "Boost Midtones",
                "Reduce Minor Color Noise",
                "Reduce Oversaturated Colors",
                "Selectively Desaturate"
            ]
        );
    }
}
