//! HSL Adjust: VEGAS's. Every color's hue turned (Add to hue: a whole turn
//! is 1), its saturation and its lightness scaled, in the HSL model.
//!
//! VEGAS's math, measured on its pictures: the HSL of the values the host
//! hands over (in BananaV's 8-bit projects, as VEGAS's, the encoded ones),
//! the lightness (max + min) ⁄ 2 — Desaturate's grays —; the saturation
//! scaled with no ceiling (past 1 the colors leave the cube: 8-bit
//! projects clip them there, Max Saturation's strong colors), the
//! lightness scaled and held within 0–1; the result is not clipped
//! (VEGAS's float effects keep what passes white or black; 8-bit projects
//! hold each effect's result within 0–1).
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.HslAdjust";

const HUE: &CStr = c"hue";
const SATURATION: &CStr = c"saturation";
const LUMINANCE: &CStr = c"luminance";

/// A color's hue (0–1, red at 0), saturation and lightness, HSL's.
pub(crate) fn hsl([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d <= 1e-7 {
        return [0.0, 0.0, l];
    }
    let s = if l <= 0.5 {
        d / (max + min)
    } else {
        d / (2.0 - max - min)
    };
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h / 6.0, s, l]
}

/// The color of a hue, saturation and lightness (a saturation past 1
/// leaves the cube).
pub(crate) fn rgb([h, s, l]: [f32; 3]) -> [f32; 3] {
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let channel = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0)]
}

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    /// Added to the hue: a whole turn is 1.
    pub hue: f64,
    pub saturation: f64,
    pub luminance: f64,
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        Settings {
            hue: v.f64(HUE, 0.0),
            saturation: v.f64(SATURATION, 1.0).max(0.0),
            luminance: v.f64(LUMINANCE, 1.0).max(0.0),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            hue: fx.param(HUE).f64_at(time),
            saturation: fx.param(SATURATION).f64_at(time).max(0.0),
            luminance: fx.param(LUMINANCE).f64_at(time).max(0.0),
        }
    }

    pub(crate) fn changes_nothing(&self) -> bool {
        self.hue.rem_euclid(1.0) == 0.0 && self.saturation == 1.0 && self.luminance == 1.0
    }

    /// A color adjusted.
    pub(crate) fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let [h, s, l] = hsl(c.map(|v| v.clamp(0.0, 1.0)));
        let h = (h + self.hue as f32).rem_euclid(1.0);
        let s = s * self.saturation as f32;
        let l = (l * self.luminance as f32).clamp(0.0, 1.0);
        rgb([h, s, l])
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
    p.set_str(kOfxPropLabel, c"HSL Adjust");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"Turns the colors' hue and scales their saturation and lightness.",
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
        HUE,
        c"Add to hue",
        [0.0, 1.0, 0.0],
        c"Turns every color's hue: a whole turn is 1, half way gives the opposite colors.",
    );
    number(
        SATURATION,
        c"Saturation",
        [0.0, 2.0, 1.0],
        c"Scales the colors' saturation: 0 is gray, 2 twice as strong.",
    );
    number(
        LUMINANCE,
        c"Luminance",
        [0.0, 2.0, 1.0],
        c"Scales the colors' lightness.",
    );
}

// Presets.

/// The factory presets: VEGAS's.
pub(crate) fn presets() -> Vec<Preset> {
    let preset = |name: &'static str, hue: f64, saturation: f64, luminance: f64| Preset {
        name,
        values: vec![
            ("hue", PresetValue::Num(vec![hue])),
            ("saturation", PresetValue::Num(vec![saturation])),
            ("luminance", PresetValue::Num(vec![luminance])),
        ],
    };
    vec![
        preset("Desaturate", 0.0, 0.0, 1.0),
        preset("Invert Color", 0.5, 1.0, 1.0),
        preset("Max Saturation", 0.0, 2.0, 1.0),
        preset("Half Saturation", 0.0, 0.5, 1.0),
        preset("Saturation and Luminance Boost", 0.0, 2.0, 1.05),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(hue: f64, saturation: f64, luminance: f64) -> Settings {
        Settings {
            hue,
            saturation,
            luminance,
        }
    }

    /// As an 8-bit project keeps it (VEGAS's pictures are 8-bit).
    fn bytes(c: [f32; 3]) -> [f32; 3] {
        c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round())
    }

    #[test]
    fn hsl_goes_both_ways() {
        for c in [
            [0.97, 0.84, 0.4],
            [0.06, 0.88, 1.0],
            [0.5, 0.5, 0.5],
            [0.9, 0.1, 0.6],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
        ] {
            let back = rgb(hsl(c));
            assert!(
                c.iter().zip(back).all(|(a, b)| (a - b).abs() < 1e-5),
                "{c:?}: {back:?}"
            );
        }
    }

    #[test]
    fn the_colors_change_as_vegass() {
        // VEGAS's pictures: its sky (a flat 16 226 255) and its sand (249
        // 213 100, a texture: a looser match) through its presets.
        let sky = [16.0, 226.0, 255.0].map(|v: f32| v / 255.0);
        let sand = [249.0, 213.0, 100.0].map(|v: f32| v / 255.0);
        for ([hue, saturation, luminance], c, vegas, within) in [
            ([0.0, 0.0, 1.0], sky, [136.0, 136.0, 136.0], 1.5),
            ([0.5, 1.0, 1.0], sky, [255.0, 46.0, 16.0], 1.5),
            ([0.0, 2.0, 1.0], sky, [1.0, 255.0, 255.0], 1.5),
            ([0.0, 0.5, 1.0], sky, [76.0, 180.0, 196.0], 1.5),
            ([0.0, 2.0, 1.05], sky, [1.0, 255.0, 255.0], 1.5),
            ([0.0, 0.0, 1.0], sand, [176.0, 176.0, 176.0], 2.0),
            ([0.5, 1.0, 1.0], sand, [102.0, 140.0, 252.0], 4.5),
            ([0.0, 2.0, 1.0], sand, [254.0, 248.0, 27.0], 4.0),
            ([0.0, 0.5, 1.0], sand, [213.0, 194.0, 138.0], 1.5),
            ([0.0, 2.0, 1.05], sand, [254.0, 248.0, 54.0], 4.0),
        ] {
            let out = s(hue, saturation, luminance)
                .apply(c)
                .map(|v| v.clamp(0.0, 1.0));
            for k in 0..3 {
                assert!(
                    (out[k] * 255.0 - vegas[k]).abs() <= within,
                    "{hue} {saturation} {luminance}: {:?}",
                    bytes(out)
                );
            }
        }
        // Luminance: the lightness scaled, white held.
        assert_eq!(bytes(s(0.0, 1.0, 2.0).apply([0.9; 3])), [255.0; 3]);
        assert_eq!(bytes(s(0.0, 1.0, 0.5).apply([0.5; 3])), [64.0; 3]);
    }

    #[test]
    fn the_default_changes_nothing() {
        assert!(s(0.0, 1.0, 1.0).changes_nothing());
        assert!(s(1.0, 1.0, 1.0).changes_nothing());
        assert!(!s(0.5, 1.0, 1.0).changes_nothing());
        let c = [0.3, 0.6, 0.9];
        let out = s(0.0, 1.0, 1.0).apply(c);
        assert!(c.iter().zip(out).all(|(a, b)| (a - b).abs() < 1e-5));
    }

    #[test]
    fn the_presets_are_vegass() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            [
                "Desaturate",
                "Invert Color",
                "Max Saturation",
                "Half Saturation",
                "Saturation and Luminance Boost"
            ]
        );
    }
}
