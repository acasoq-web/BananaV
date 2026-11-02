//! Add Noise: VEGAS's. Random noise added to the picture: gray or colored,
//! spread evenly or as a bell curve, still or new in every frame.
//!
//! VEGAS's math, measured on its pictures: every pixel gets a noise of its
//! own, the same in red, green and blue when Monochromatic (one for each
//! channel otherwise), added to its color (of the values the host hands
//! over: in BananaV's 8-bit projects, as VEGAS's, the encoded ones); spread
//! evenly over a range 2⁄3 × Noise level wide, or with Gaussian noise as a
//! bell curve whose standard deviation is 2⁄3 × Noise level × (1 − Noise
//! level ⁄ 4) — VEGAS's strong bell curves spread less than the weak ones
//! (its prints at 0.07 and 0.84). What goes past black or white comes back
//! the other way (folded) instead of stopping there: VEGAS's strongest
//! noise covers the picture in specks of every shade, not in black and
//! white ones, and a light color keeps some of its tint. Animate draws a
//! new noise in every frame.
//!
//! The noise is the same wherever it is drawn: each pixel's comes from a
//! hash of its place (from the frame's lower left, at render scale), the
//! channel and the frame's time, which the graphics card's kernel works
//! out alike.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.AddNoise";

const LEVEL: &CStr = c"level";
const MONOCHROMATIC: &CStr = c"monochromatic";
const GAUSSIAN: &CStr = c"gaussian";
const ANIMATE: &CStr = c"animate";

/// How wide the noise is per unit of Noise level: the even noise's range,
/// a weak bell curve's standard deviation.
pub(crate) const SPREAD: f32 = 2.0 / 3.0;

/// The seed of a noise that stays still.
const STILL: u32 = 0x2545_f491;

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub level: f64,
    pub monochromatic: bool,
    pub gaussian: bool,
    pub animate: bool,
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        Settings {
            level: v.f64(LEVEL, 0.5),
            monochromatic: v.int(MONOCHROMATIC, 1) != 0,
            gaussian: v.int(GAUSSIAN, 0) != 0,
            animate: v.int(ANIMATE, 1) != 0,
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            level: fx.param(LEVEL).f64_at(time),
            monochromatic: fx.param(MONOCHROMATIC).int_at(time) != 0,
            gaussian: fx.param(GAUSSIAN).int_at(time) != 0,
            animate: fx.param(ANIMATE).int_at(time) != 0,
        }
    }

    pub(crate) fn changes_nothing(&self) -> bool {
        self.level <= 0.0
    }

    /// What the noise (even over −0.5–0.5, or a bell curve of standard
    /// deviation 1) is scaled by.
    pub(crate) fn width(&self) -> f32 {
        let level = self.level.max(0.0) as f32;
        if self.gaussian {
            SPREAD * level * (1.0 - level / 4.0)
        } else {
            SPREAD * level
        }
    }

    /// The seed of the noise at `time` (frames): its own for each time
    /// when it animates, else always the same.
    pub(crate) fn seed(&self, time: f64) -> u32 {
        if !self.animate {
            return STILL;
        }
        let bits = time.to_bits();
        hash(hash(bits as u32) ^ (bits >> 32) as u32)
    }

    /// A straight color with the noise of pixel `x`, `y` (from the frame's
    /// lower left) added: clipped to 0–1, then what goes past black or
    /// white folded back.
    pub(crate) fn add(&self, c: [f32; 3], seed: u32, x: u32, y: u32) -> [f32; 3] {
        let width = self.width();
        let gray = noise(seed, x, y, 0, self.gaussian);
        std::array::from_fn(|k| {
            let n = if self.monochromatic || k == 0 {
                gray
            } else {
                noise(seed, x, y, k as u32, self.gaussian)
            };
            fold(c[k].clamp(0.0, 1.0) + width * n)
        })
    }
}

/// A 32-bit integer hash (Chris Wellons's lowbias32; the card's
/// `noise_hash`).
pub(crate) fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// The noise of pixel `x`, `y`, channel `c`: even over −0.5–0.5, or
/// Gaussian (standard deviation 1; Marsaglia's polar method, its points
/// whole numbers so the card picks the same ones).
pub(crate) fn noise(seed: u32, x: u32, y: u32, c: u32, gaussian: bool) -> f32 {
    let h = hash(seed ^ c.wrapping_mul(0x9e37_79b9));
    let h = hash(h ^ x);
    let h = hash(h ^ y.wrapping_mul(0x85eb_ca6b));
    if !gaussian {
        return (h >> 8) as f32 / 16_777_216.0 - 0.5;
    }
    for k in 0..8u32 {
        let p = hash(h.wrapping_add(k));
        let u = (p >> 16) as i32 - 32768;
        let v = (p & 0xffff) as i32 - 32768;
        let s = (u * u) as u32 + (v * v) as u32;
        if s > 0 && s < 1 << 30 {
            let s = s as f32 / 1_073_741_824.0;
            return u as f32 / 32768.0 * (-2.0 * s.ln() / s).sqrt();
        }
    }
    0.0
}

/// `v` folded into 0–1: past 1 it comes back down, below 0 back up.
pub(crate) fn fold(v: f32) -> f32 {
    let t = v - 2.0 * (v * 0.5).floor();
    if t > 1.0 { 2.0 - t } else { t }
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
        render(fx, args)
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        if Settings::at(fx, args.dbl(kOfxPropTime, 0)).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxImageEffectActionGetClipPreferences) {
        // A new noise in every frame (when it animates): the frames differ
        // even where the picture does not.
        out.set_int(kOfxImageEffectFrameVarying, 1);
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
    p.set_str(kOfxPropLabel, c"Add Noise");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Creative");
    p.set_str(
        kOfxPropPluginDescription,
        c"Adds random noise to the picture, gray or colored, still or new in every frame: film grain, or a screen with no signal.",
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

    let p = fx.define_param(kOfxParamTypeDouble, LEVEL, c"Noise level");
    p.set_dbl(kOfxParamPropDefault, 0.5);
    p.set_dbl(kOfxParamPropMin, 0.0);
    p.set_dbl(kOfxParamPropMax, 1.0);
    p.set_dbl(kOfxParamPropDisplayMin, 0.0);
    p.set_dbl(kOfxParamPropDisplayMax, 1.0);
    p.set_dbl(kOfxParamPropIncrement, 0.01);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(kOfxParamPropHint, c"How strong the noise is.");
    let check = |name: &CStr, label: &CStr, default: bool, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeBoolean, name, label);
        p.set_int(kOfxParamPropDefault, default as c_int);
        // They animate, as VEGAS's do (each with its clock).
        p.set_int(kOfxParamPropAnimates, 1);
        p.set_str(kOfxParamPropHint, hint);
    };
    check(
        MONOCHROMATIC,
        c"Monochromatic",
        true,
        c"Gray noise, the same in red, green and blue; unchecked, each channel gets its own and the specks are colored.",
    );
    check(
        GAUSSIAN,
        c"Gaussian noise",
        false,
        c"Noise spread as a bell curve: stronger, with more specks far from the picture's colors. Unchecked, it is spread evenly.",
    );
    check(
        ANIMATE,
        c"Animate",
        true,
        c"A new noise in every frame; unchecked, the same noise stays still.",
    );
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let s = Settings::at(fx, time);
    let seed = s.seed(time);
    crate::colors::render_at(fx, args, &|[x, y], c| s.add(c, seed, x, y))
}

// Presets.

/// The factory presets: VEGAS's.
pub(crate) fn presets() -> Vec<Preset> {
    let preset = |name: &'static str, level: f64, monochromatic: bool, gaussian: bool| Preset {
        name,
        values: vec![
            ("level", PresetValue::Num(vec![level])),
            (
                "monochromatic",
                PresetValue::Num(vec![monochromatic as u8 as f64]),
            ),
            ("gaussian", PresetValue::Num(vec![gaussian as u8 as f64])),
            ("animate", PresetValue::Num(vec![1.0])),
        ],
    };
    vec![
        preset("Medium", 0.5, true, false),
        preset("Grainy", 0.07, true, true),
        preset("Extreme", 0.84, true, true),
        preset("Color", 1.0, false, false),
        preset("Screen", 0.214, false, false),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(level: f64, monochromatic: bool, gaussian: bool, animate: bool) -> Settings {
        Settings {
            level,
            monochromatic,
            gaussian,
            animate,
        }
    }

    /// The noise's mean and standard deviation over many pixels.
    fn spread(gaussian: bool) -> (f64, f64) {
        let n = 200_000u32;
        let values: Vec<f64> = (0..n)
            .map(|i| noise(7, i % 500, i / 500, 0, gaussian) as f64)
            .collect();
        let mean = values.iter().sum::<f64>() / n as f64;
        let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;
        (mean, var.sqrt())
    }

    #[test]
    fn the_noise_is_even_or_a_bell_curve() {
        let (mean, sd) = spread(false);
        assert!(
            mean.abs() < 0.005 && (sd - 1.0 / 12f64.sqrt()).abs() < 0.005,
            "{mean} {sd}"
        );
        let (mean, sd) = spread(true);
        assert!(mean.abs() < 0.01 && (sd - 1.0).abs() < 0.01, "{mean} {sd}");
        // Even: within ±0.5; neighbors unrelated.
        assert!((0..1000).all(|x| noise(1, x, 3, 0, false).abs() <= 0.5));
        assert_ne!(noise(1, 10, 3, 0, false), noise(1, 11, 3, 0, false));
        assert_ne!(noise(1, 10, 3, 0, false), noise(1, 10, 4, 0, false));
    }

    #[test]
    fn its_width_is_vegass() {
        // VEGAS's prints: Noise level 0.5 spread evenly leaves the colors
        // 24 (in 255) from the picture's on average, Gaussian 0.07 12;
        // Gaussian 0.84 spreads as 0.52 (not 2/3) times its level would.
        let sd = |level: f64, gaussian: bool| {
            let w = s(level, true, gaussian, true).width() as f64;
            w * if gaussian { 1.0 } else { 1.0 / 12f64.sqrt() } * 255.0
        };
        assert!((sd(0.5, false) - 24.0).abs() < 1.0, "{}", sd(0.5, false));
        assert!((sd(0.07, true) - 12.0).abs() < 0.5, "{}", sd(0.07, true));
        assert!((sd(0.84, true) / 255.0 / 0.84 - 0.52).abs() < 0.01);
        // Even noise grows with its level all the way.
        assert_eq!(s(1.0, false, false, true).width(), SPREAD);
    }

    #[test]
    fn past_black_and_white_it_folds_back() {
        assert_eq!(fold(0.25), 0.25);
        assert!((fold(1.25) - 0.75).abs() < 1e-6);
        assert!((fold(-0.25) - 0.25).abs() < 1e-6);
        assert!((fold(2.5) - 0.5).abs() < 1e-6);
        assert!((fold(-1.5) - 0.5).abs() < 1e-6);
        assert_eq!(fold(1.0), 1.0);
        assert_eq!(fold(0.0), 0.0);
    }

    #[test]
    fn monochromatic_noise_is_gray() {
        let mono = s(0.5, true, false, true);
        let out = mono.add([0.5, 0.5, 0.5], 9, 4, 4);
        assert!(out[0] == out[1] && out[1] == out[2]);
        let color = s(0.5, false, false, true);
        let out = color.add([0.5, 0.5, 0.5], 9, 4, 4);
        assert!(out[0] != out[1] && out[1] != out[2]);
    }

    #[test]
    fn animate_draws_a_new_noise_every_frame() {
        let moving = s(0.5, true, false, true);
        assert_ne!(moving.seed(10.0), moving.seed(11.0));
        assert_eq!(moving.seed(10.0), moving.seed(10.0));
        let still = s(0.5, true, false, false);
        assert_eq!(still.seed(10.0), still.seed(11.0));
    }

    #[test]
    fn the_presets_are_vegass() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(names, ["Medium", "Grainy", "Extreme", "Color", "Screen"]);
        assert!(s(0.0, true, false, true).changes_nothing());
        assert!(!s(0.07, true, true, true).changes_nothing());
    }
}
