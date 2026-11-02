//! Invert: VEGAS's. Each color channel taken toward its opposite (1 less
//! it) by Blend amount: a negative at 1, gray at 0.5.
//!
//! VEGAS's math, measured on the calibration card (8-bit, every level of
//! each channel) at Blend amount 1 and 0.6 (its 60% Inversion): `c + (1 −
//! 2c) × amount`, every level exact. Alpha stays. Its presets: 60%, 75%
//! and 100% Inversion.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.Invert";

/// The parameter (BananaV's earlier Invert's name, so its projects open).
const AMOUNT: &CStr = c"amount";

/// How far under the exact value BananaV's results come: a 32nd of an
/// 8-bit step, so their halves round down as VEGAS's do.
const TIE: f32 = 1.0 / (32.0 * 255.0);

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub amount: f64,
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        Settings {
            amount: v.f64(AMOUNT, 1.0),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            amount: fx.param(AMOUNT).f64_at(time),
        }
    }

    /// Whether it changes nothing.
    pub(crate) fn changes_nothing(&self) -> bool {
        self.amount == 0.0
    }

    /// The straight line each channel goes through: its slope (1 − 2 ×
    /// amount) and offset (amount).
    pub(crate) fn line(&self) -> (f32, f32) {
        let a = self.amount.clamp(0.0, 1.0) as f32;
        (1.0 - 2.0 * a, a - TIE)
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
        let (k, offset) = Settings::at(fx, args.dbl(kOfxPropTime, 0)).line();
        crate::colors::render(fx, args, &|c| c.map(|v| (v * k + offset).max(0.0)))
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
    p.set_str(kOfxPropLabel, c"Invert");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"The colors and the brightness turned around: a photo negative.",
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
    let p = fx.define_param(kOfxParamTypeDouble, AMOUNT, c"Blend amount");
    p.set_dbl(kOfxParamPropDefault, 1.0);
    p.set_dbl(kOfxParamPropMin, 0.0);
    p.set_dbl(kOfxParamPropMax, 1.0);
    p.set_dbl(kOfxParamPropDisplayMin, 0.0);
    p.set_dbl(kOfxParamPropDisplayMax, 1.0);
    p.set_dbl(kOfxParamPropIncrement, 0.001);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"How much the colors and the brightness are turned around: a negative at 1.",
    );
}

/// The factory presets: VEGAS's, in its list's order.
pub(crate) fn presets() -> Vec<Preset> {
    [
        ("60% Inversion", 0.6),
        ("75% Inversion", 0.75),
        ("100% Inversion", 1.0),
    ]
    .into_iter()
    .map(|(name, a)| Preset {
        name,
        values: vec![("amount", PresetValue::Num(vec![a]))],
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn byte(s: &Settings, x: u8) -> u8 {
        let (k, o) = s.line();
        ((x as f32 / 255.0 * k + o).clamp(0.0, 1.0) * 255.0).round() as u8
    }

    /// VEGAS's Invert on the card at 1 and 0.6 (its 60% Inversion).
    #[test]
    fn invert_draws_what_vegas_draws() {
        let full = Settings { amount: 1.0 };
        for x in [0u8, 1, 127, 128, 200, 255] {
            assert_eq!(byte(&full, x), 255 - x);
        }
        let sixty = Settings { amount: 0.6 };
        for (x, y) in [
            (0, 153),
            (2, 153),
            (64, 140),
            (100, 133),
            (127, 128),
            (128, 127),
            (192, 115),
            (255, 102),
        ] {
            assert_eq!(byte(&sixty, x), y, "{x}");
        }
        assert!(Settings { amount: 0.0 }.changes_nothing());
        assert_eq!(presets().len(), 3);
    }
}
