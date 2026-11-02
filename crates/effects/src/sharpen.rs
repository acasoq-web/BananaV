//! Sharpen: VEGAS's. Edges and detail made crisper: each pixel moved away
//! from its neighbours' average.
//!
//! VEGAS's math, measured on the calibration card through each of its
//! presets: the neighbours' average is the binomial 3 × 3 blur (1 2 1 each
//! way, the picture's edge pixels held beyond it), and each channel moves
//! from it by 16 over the kernel's sum times its difference — the kernel
//! `[−1 −2 −1; −2 c −2; −1 −2 −1] ⁄ (c − 12)`, its middle `c` VEGAS's
//! whole number from Amount by pieces (`middle`: 29 − ⌊16a⌋ from 0.25,
//! steeper lines below, 100 at 0) —, its halves rounding down as VEGAS's
//! (Heavy every pixel exact, Light and Medium all but a few in a
//! thousand). Below an Amount of 10⁻⁵ nothing changes.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::kit::Buf;
use crate::ofx::{Fx, Props, is, suites};
use crate::spatial::{TIE, convolve_buf, straight};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.SharpenFx";

/// The parameter (BananaV's earlier Sharpen's name, so its projects open).
const AMOUNT: &CStr = c"amount";

/// Amount at first: VEGAS's.
const AMOUNT_AT_FIRST: f64 = 0.5;

/// The binomial blur's taps, each way.
const BINOMIAL: [f32; 3] = [0.25, 0.5, 0.25];

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub amount: f64,
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            amount: fx.param(AMOUNT).f64_at(time),
        }
    }

    /// The settings in `values` (the frame's, as the card draws it).
    pub(crate) fn of(values: Values<'_>) -> Settings {
        Settings {
            amount: values.f64(AMOUNT, AMOUNT_AT_FIRST),
        }
    }

    /// The neighbours' average's taps, across and up.
    pub(crate) fn taps() -> [f32; 3] {
        BINOMIAL
    }

    /// Nothing changes: VEGAS's, an Amount within 10⁻⁵ of 0.
    pub(crate) fn changes_nothing(&self) -> bool {
        (self.amount as f32).abs() <= 1e-5
    }

    /// The kernel's middle weight, VEGAS's: a whole number from Amount by
    /// pieces — 29 − ⌊16a⌋ from 0.25, steeper lines below it.
    pub(crate) fn middle(&self) -> i32 {
        let a = self.amount.clamp(0.0, 1.0);
        if a >= 0.25 {
            return 29 - (a * 16.0) as i32;
        }
        let (start, slope) = if a >= 0.125 {
            (63.0, 152.0)
        } else if a >= f64::from(0.063f32) {
            (88.961538, 269.230769)
        } else {
            (100.451613, 451.612903)
        };
        (start - a * slope) as i32
    }

    /// How far each channel moves per its difference from the average:
    /// 16 over the kernel's sum.
    pub(crate) fn gain(&self) -> f32 {
        16.0 / (self.middle() - 12) as f32
    }

    /// A straight color sharpened against `around`, its neighbours'
    /// average (straight too); unclipped.
    pub(crate) fn apply(&self, c: [f32; 3], around: [f32; 3]) -> [f32; 3] {
        let g = self.gain();
        std::array::from_fn(|i| c[i] + g * (c[i] - around[i]) - TIE)
    }

    /// The picture sharpened: each pixel's straight color, its alpha kept.
    pub(crate) fn process(&self, src: &Buf) -> Buf {
        let blurred = convolve_buf(src, &BINOMIAL, &BINOMIAL);
        let data = src
            .data
            .iter()
            .zip(blurred)
            .map(|(&p, b)| {
                let a = p[3];
                if a <= 0.0 {
                    return [0.0; 4];
                }
                // The neighbours' average by their alpha (all transparent
                // around: the pixel's own color).
                let c = straight(p);
                let around = if b[3] > 0.0 { straight(b) } else { c };
                let o = self.apply(c, around);
                [o[0] * a, o[1] * a, o[2] * a, a]
            })
            .collect();
        Buf {
            bounds: src.bounds,
            data,
        }
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
        crate::spatial::render(fx, args, &|_, src| s.process(src))
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        if Settings::at(fx, args.dbl(kOfxPropTime, 0)).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        crate::spatial::whole_source(fx, args, out)
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
    p.set_str(kOfxPropLabel, c"Sharpen");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Blur");
    p.set_str(
        kOfxPropPluginDescription,
        c"Edges and contrast made crisper.",
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
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);
    let p = fx.define_param(kOfxParamTypeDouble, AMOUNT, c"Amount");
    p.set_dbl(kOfxParamPropDefault, AMOUNT_AT_FIRST);
    p.set_dbl(kOfxParamPropMin, 0.0);
    p.set_dbl(kOfxParamPropMax, 1.0);
    p.set_dbl(kOfxParamPropDisplayMin, 0.0);
    p.set_dbl(kOfxParamPropDisplayMax, 1.0);
    p.set_dbl(kOfxParamPropIncrement, 0.001);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"How strongly edges and detail are sharpened: 1 the most.",
    );
}

/// The factory presets: VEGAS's, in its list's order.
pub(crate) fn presets() -> Vec<Preset> {
    let p = |name: &'static str, amount: f64| Preset {
        name,
        values: vec![("amount", PresetValue::Num(vec![amount]))],
    };
    vec![p("Medium", 0.5), p("Light", 0.25), p("Heavy", 0.75)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gain_is_vegass() {
        // The card's prints: Light 1.231, Medium 1.778, Heavy 3.2; between
        // and below them VEGAS's whole middle weights.
        for (amount, middle) in [
            (0.25, 25),
            (0.5, 21),
            (0.75, 17),
            (1.0, 13),
            (0.3, 25),
            (0.2, 32),
            (0.1, 62),
            (0.05, 77),
            (0.001, 100),
        ] {
            let s = Settings { amount };
            assert_eq!(s.middle(), middle, "{amount}");
            let gain = 16.0 / (middle - 12) as f32;
            assert!((s.gain() - gain).abs() < 1e-6, "{amount}: {}", s.gain());
        }
        assert!((Settings { amount: 0.75 }.gain() - 3.2).abs() < 1e-6);
        assert!(Settings { amount: 0.0 }.changes_nothing());
        assert!(!Settings { amount: 0.001 }.changes_nothing());
    }

    #[test]
    fn a_step_overshoots_both_ways() {
        // Heavy at a dark pixel beside light ones: away from the average.
        let heavy = Settings { amount: 0.75 };
        let o = heavy.apply([0.2, 0.2, 0.2], [0.4, 0.4, 0.4]);
        for v in o {
            assert!((v - (0.2 - 3.2 * 0.2) + TIE).abs() < 1e-6, "{o:?}");
        }
        // A flat picture stays (its halves a 32nd of a step down).
        let o = heavy.apply([0.5; 3], [0.5; 3]);
        assert!(o.iter().all(|&v| (v - (0.5 - TIE)).abs() < 1e-7));
    }

    #[test]
    fn the_presets_are_vegass() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(names, ["Medium", "Light", "Heavy"]);
    }
}
