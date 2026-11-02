//! Black and White: VEGAS's. Each color taken toward its gray — the plain
//! average of red, green and blue — by Blend amount.
//!
//! VEGAS's math, measured on the calibration card (8-bit, every one of its
//! 263 488 colors): at Blend amount 1 each becomes (R + G + B) ⁄ 3, to the
//! level (no luma's weights: Rec. 601's misses by up to 65 levels); below
//! 1 the color and its gray mixed in proportion, as Invert's Blend amount
//! mixes (measured at 0.6). Its presets: 25%, 50%, 75% and 100% Black and
//! White.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.BlackAndWhite";

/// The parameter (BananaV's earlier Black and White's name, so its
/// projects open).
const AMOUNT: &CStr = c"blendAmount";

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

    /// The straight line the colors go through: each kept by 1 − amount,
    /// the gray added by amount (a third of each channel).
    pub(crate) fn affine(&self) -> ([[f32; 3]; 3], [f32; 3]) {
        let a = self.amount.clamp(0.0, 1.0) as f32;
        let matrix = std::array::from_fn(|i| {
            std::array::from_fn(|j| a / 3.0 + if i == j { 1.0 - a } else { 0.0 })
        });
        (matrix, [-TIE; 3])
    }

    /// A color through it, negatives clipped.
    pub(crate) fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let (m, o) = self.affine();
        std::array::from_fn(|i| (m[i][0] * c[0] + m[i][1] * c[1] + m[i][2] * c[2] + o[i]).max(0.0))
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
    p.set_str(kOfxPropLabel, c"Black and White");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"The color taken away, the brightness kept.",
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
        c"How much of the color is taken away, the brightness kept.",
    );
}

/// The factory presets: VEGAS's, in its list's order.
pub(crate) fn presets() -> Vec<Preset> {
    [
        ("25% Black and White", 0.25),
        ("50% Black and White", 0.5),
        ("75% Black and White", 0.75),
        ("100% Black and White", 1.0),
    ]
    .into_iter()
    .map(|(name, a)| Preset {
        name,
        values: vec![("blendAmount", PresetValue::Num(vec![a]))],
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(s: &Settings, c: [u8; 3]) -> [u8; 3] {
        s.apply(c.map(|v| v as f32 / 255.0))
            .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
    }

    /// VEGAS's Black and White at 1 on the card: the plain average.
    #[test]
    fn the_gray_is_the_average_as_vegas_draws_it() {
        let full = Settings { amount: 1.0 };
        for (c, y) in [
            ([255, 0, 0], 85),
            ([0, 255, 0], 85),
            ([0, 0, 255], 85),
            ([255, 255, 0], 170),
            ([10, 20, 31], 20),
            ([200, 100, 51], 117),
            ([128, 128, 128], 128),
        ] {
            assert_eq!(bytes(&full, c), [y; 3], "{c:?}");
        }
        // Halfway: halfway to the gray (42.5 rounding down).
        let half = Settings { amount: 0.5 };
        assert_eq!(bytes(&half, [255, 0, 0]), [170, 42, 42]);
        assert!(Settings { amount: 0.0 }.changes_nothing());
        assert_eq!(presets().len(), 4);
    }
}
