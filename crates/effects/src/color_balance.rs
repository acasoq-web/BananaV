//! Color Balance: VEGAS's. Red, Green and Blue push the picture's colors
//! toward (or, below 0, away from) each, most where Type says: in the
//! light parts, the middle tones or the dark parts.
//!
//! VEGAS's math, measured on its pictures and on the calibration card:
//! each channel moves by its slider times a weight of the color's
//! intensity I, the channels' mean (of the values the host hands over: in
//! BananaV's 8-bit projects, as VEGAS's, the encoded ones) — I(2 − I) for
//! Highlights, 4I(1 − I) for Midtones and, mirroring Highlights, 1 − I²
//! for Shadow. Preserve luminosity takes half of each slider's move from
//! the other two channels, so the intensity stays: Red 0.1 in the middle
//! tones raises red by 0.1 and lowers green and blue by 0.05 (without it,
//! only red rises, by 0.1). The sliders run from −0.5 to 0.5, as VEGAS's.
//! The result is not clipped (VEGAS's float effects keep what passes white
//! or black; 8-bit projects hold each effect's result within 0–1).
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.ColorBalance";

const CHANNELS: [&CStr; 3] = [c"red", c"green", c"blue"];
const TYPE: &CStr = c"type";
const PRESERVE: &CStr = c"preserve";

/// Where the colors change most.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Range {
    Highlights,
    Midtones,
    Shadow,
}

impl Range {
    fn of(index: i32) -> Range {
        match index {
            0 => Range::Highlights,
            2 => Range::Shadow,
            _ => Range::Midtones,
        }
    }

    pub(crate) fn index(self) -> u32 {
        match self {
            Range::Highlights => 0,
            Range::Midtones => 1,
            Range::Shadow => 2,
        }
    }

    /// How much of the change a color of intensity `i` takes.
    pub(crate) fn weight(self, i: f32) -> f32 {
        let i = i.clamp(0.0, 1.0);
        match self {
            Range::Highlights => i * (2.0 - i),
            Range::Midtones => 4.0 * i * (1.0 - i),
            Range::Shadow => 1.0 - i * i,
        }
    }
}

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    /// Red's, green's and blue's sliders.
    pub rgb: [f64; 3],
    pub range: Range,
    pub preserve: bool,
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        Settings {
            rgb: [0, 1, 2].map(|k| v.f64(CHANNELS[k], 0.0)),
            range: Range::of(v.int(TYPE, 1)),
            preserve: v.int(PRESERVE, 1) != 0,
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            rgb: CHANNELS.map(|c| fx.param(c).f64_at(time)),
            range: Range::of(fx.param(TYPE).int_at(time)),
            preserve: fx.param(PRESERVE).int_at(time) != 0,
        }
    }

    pub(crate) fn changes_nothing(&self) -> bool {
        self.shifts() == [0.0; 3]
    }

    /// What each channel gains where the weight is whole: its slider, and
    /// with Preserve luminosity less half of the other two's (the
    /// channels' sum stays).
    pub(crate) fn shifts(&self) -> [f32; 3] {
        let k = self.rgb.map(|v| v as f32);
        if self.preserve {
            // k − (the others' sum) ⁄ 2, as 1.5 (k − their mean).
            let mean = (k[0] + k[1] + k[2]) / 3.0;
            k.map(|v| 1.5 * (v - mean))
        } else {
            k
        }
    }

    /// A straight color balanced.
    pub(crate) fn apply(shifts: [f32; 3], range: Range, c: [f32; 3]) -> [f32; 3] {
        let w = range.weight((c[0] + c[1] + c[2]) / 3.0);
        [0, 1, 2].map(|k| c[k] + shifts[k] * w)
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
        let (shifts, range) = (s.shifts(), s.range);
        crate::colors::render(fx, args, &|c| Settings::apply(shifts, range, c))
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        if Settings::at(fx, args.dbl(kOfxPropTime, 0)).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // Each output pixel needs the source's pixel there only.
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &roi);
        kOfxStatOK
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
    p.set_str(kOfxPropLabel, c"Color Balance");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"Moves the picture's colors toward red, green or blue, most in its light parts, its middle tones or its dark parts.",
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

    let labels = [c"Red", c"Green", c"Blue"];
    let hints = [
        c"Adds red where Type says (below 0, takes it away: cyan).",
        c"Adds green where Type says (below 0, takes it away: magenta).",
        c"Adds blue where Type says (below 0, takes it away: yellow).",
    ];
    for k in 0..3 {
        let p = fx.define_param(kOfxParamTypeDouble, CHANNELS[k], labels[k]);
        p.set_dbl(kOfxParamPropDefault, 0.0);
        p.set_dbl(kOfxParamPropMin, -1.0);
        p.set_dbl(kOfxParamPropMax, 1.0);
        // VEGAS's slider: 0.1 is a tenth of its way.
        p.set_dbl(kOfxParamPropDisplayMin, -0.5);
        p.set_dbl(kOfxParamPropDisplayMax, 0.5);
        p.set_dbl(kOfxParamPropIncrement, 0.01);
        p.set_int(kOfxParamPropDigits, 3);
        p.set_int(kOfxParamPropAnimates, 1);
        p.set_str(kOfxParamPropHint, hints[k]);
    }
    let p = fx.define_param(kOfxParamTypeChoice, TYPE, c"Type");
    p.set_strs(
        kOfxParamPropChoiceOption,
        &[c"Highlights", c"Midtones", c"Shadow"],
    );
    p.set_int(kOfxParamPropDefault, 1);
    p.set_int(kOfxParamPropAnimates, 1);
    p.set_str(
        kOfxParamPropHint,
        c"Where the colors change most: the light parts, the middle tones or the dark parts.",
    );
    let p = fx.define_param(kOfxParamTypeBoolean, PRESERVE, c"Preserve luminosity");
    p.set_int(kOfxParamPropDefault, 1);
    p.set_int(kOfxParamPropAnimates, 1);
    p.set_str(
        kOfxParamPropHint,
        c"Keeps the picture's brightness: what one color gains, the others lose.",
    );
}

// Presets.

/// The factory presets: VEGAS's.
pub(crate) fn presets() -> Vec<Preset> {
    let preset = |name: &'static str, rgb: [f64; 3], range: Range| Preset {
        name,
        values: vec![
            ("red", PresetValue::Num(vec![rgb[0]])),
            ("green", PresetValue::Num(vec![rgb[1]])),
            ("blue", PresetValue::Num(vec![rgb[2]])),
            ("type", PresetValue::Num(vec![range.index() as f64])),
            ("preserve", PresetValue::Num(vec![1.0])),
        ],
    };
    vec![
        preset("Red Midtones", [0.1, 0.0, 0.0], Range::Midtones),
        preset("Green Midtones", [0.0, 0.1, 0.0], Range::Midtones),
        preset("Blue Midtones", [0.0, 0.0, 0.1], Range::Midtones),
        preset("Red Highlight", [0.1, 0.0, 0.0], Range::Highlights),
        preset("Green Highlight", [0.0, 0.1, 0.0], Range::Highlights),
        preset("Blue Highlight", [0.0, 0.0, 0.1], Range::Highlights),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(rgb: [f64; 3], range: Range, preserve: bool) -> Settings {
        Settings {
            rgb,
            range,
            preserve,
        }
    }

    fn apply(s: &Settings, c: [f32; 3]) -> [f32; 3] {
        Settings::apply(s.shifts(), s.range, c)
    }

    fn near(a: [f32; 3], b: [f32; 3], by: f32) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() <= by)
    }

    #[test]
    fn the_weights_are_vegass() {
        // Measured on VEGAS's pictures by the channels' mean: the middle
        // tones' a parabola, the light parts' rising to white.
        assert_eq!(Range::Midtones.weight(0.5), 1.0);
        assert_eq!(Range::Midtones.weight(0.0), 0.0);
        assert!((Range::Midtones.weight(0.25) - 0.75).abs() < 1e-6);
        assert_eq!(Range::Highlights.weight(1.0), 1.0);
        assert_eq!(Range::Highlights.weight(0.0), 0.0);
        assert!((Range::Highlights.weight(0.5) - 0.75).abs() < 1e-6);
        assert_eq!(Range::Shadow.weight(0.0), 1.0);
        assert_eq!(Range::Shadow.weight(1.0), 0.0);
    }

    #[test]
    fn vegass_presets_move_greys_as_its_previews_show() {
        // VEGAS's pictures, greys' changes in 8-bit steps (flat parts,
        // binned by brightness): Red Midtones at 0.5 raises red 25.5 and
        // lowers green and blue 12.8; Red Highlight at 0.29 raises red 13.
        let grey = |v: f32| [v; 3];
        let step = |a: [f32; 3], b: [f32; 3]| [0, 1, 2].map(|k| (b[k] - a[k]) * 255.0);
        let red_mid = s([0.1, 0.0, 0.0], Range::Midtones, true);
        assert!(near(
            step(grey(0.5), apply(&red_mid, grey(0.5))),
            [25.5, -12.75, -12.75],
            0.01
        ));
        let red_high = s([0.1, 0.0, 0.0], Range::Highlights, true);
        let d = step(grey(0.293), apply(&red_high, grey(0.293)));
        assert!(
            (d[0] - 12.98).abs() < 0.5 && (d[1] + 6.55).abs() < 0.5,
            "{d:?}"
        );
        // Green Midtones at 0.1 of brightness: green up 9.2.
        let green_mid = s([0.0, 0.1, 0.0], Range::Midtones, true);
        let d = step(grey(0.102), apply(&green_mid, grey(0.102)));
        assert!((d[1] - 9.2).abs() < 0.3, "{d:?}");
        // The channels' mean stays.
        let c = [0.2, 0.5, 0.7];
        let out = apply(&red_mid, c);
        assert!((out.iter().sum::<f32>() - c.iter().sum::<f32>()).abs() < 1e-6);
    }

    #[test]
    fn without_preserve_luminosity_only_its_channel_moves() {
        // The calibration card through VEGAS: red up by its slider.
        let red = s([0.1, 0.0, 0.0], Range::Midtones, false);
        let out = apply(&red, [0.5; 3]);
        assert!(near(out, [0.6, 0.5, 0.5], 1e-6), "{out:?}");
        // Not clipped (8-bit projects clip what each effect leaves).
        let out = apply(&s([1.0, 0.0, -1.0], Range::Midtones, false), [0.5; 3]);
        assert!(near(out, [1.5, 0.5, -0.5], 1e-6), "{out:?}");
        // The slider's end, kept: each other channel loses half.
        let out = apply(&s([0.5, 0.0, 0.0], Range::Midtones, true), [0.5; 3]);
        assert!(near(out, [1.0, 0.25, 0.25], 1e-6), "{out:?}");
    }

    #[test]
    fn the_default_changes_nothing() {
        let d = s([0.0; 3], Range::Midtones, true);
        assert!(d.changes_nothing());
        // The same move for the three: nothing, once luminosity is kept.
        assert!(s([0.2; 3], Range::Shadow, true).changes_nothing());
        assert!(!s([0.2; 3], Range::Shadow, false).changes_nothing());
        let c = [0.3, 0.6, 0.9];
        assert_eq!(apply(&d, c), c);
    }

    #[test]
    fn the_presets_are_vegass() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            [
                "Red Midtones",
                "Green Midtones",
                "Blue Midtones",
                "Red Highlight",
                "Green Highlight",
                "Blue Highlight"
            ]
        );
    }
}
