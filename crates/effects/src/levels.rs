//! Levels: VEGAS's. Each color channel — or the one Channel picks — taken
//! from Input start–Input end to 0–1 (clipped), through Gamma's curve, then
//! to Output start–Output end.
//!
//! VEGAS's math, measured on the calibration card through each of its
//! presets (8-bit, every level of each channel): under All one curve for
//! red, green and blue, `o0 + (o1 − o0) × G((v − i0) ⁄ (i1 − i0))` on the
//! values the host hands over (in 8-bit projects the encoded ones), the
//! input's span at least a 255th, its 8-bit halves rounding down as
//! VEGAS's (`TIE`).
//!
//! Gamma's curve `G` is VEGAS's: the power 1 ⁄ γ with a straight line near
//! black so it never rises infinitely steeply — the line `S t` (`S =
//! γ^1.883610`) up to `T = 0.0381838 γ^−0.941805`, then above 1 `(1 + o)
//! t^(1 ⁄ γ) − o` and below 1 `((t + o) ⁄ (1 + o))^(1 ⁄ γ)`, the offset `o`
//! joining them at `T`. Gamma's slider runs from 0.45 to 5, as VEGAS's.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.Levels";

/// The parameters (BananaV's earlier Levels' names, so its projects open).
const CHANNEL: &CStr = c"channel";
const INPUT_START: &CStr = c"inputStart";
const INPUT_END: &CStr = c"inputEnd";
const OUTPUT_START: &CStr = c"outputStart";
const OUTPUT_END: &CStr = c"outputEnd";
const GAMMA: &CStr = c"gamma";

/// Channel's options.
const CHANNELS: [&CStr; 5] = [c"All", c"Red", c"Green", c"Blue", c"Alpha"];

/// Gamma's range (VEGAS's slider: 5 at its right end, 0.45 where its knobs
/// put the left one).
const GAMMA_MIN: f64 = 0.45;
const GAMMA_MAX: f64 = 5.0;

/// How far under the exact value BananaV's results come: a 32nd of an
/// 8-bit step, so their halves round down as VEGAS's do.
const TIE: f64 = 1.0 / (32.0 * 255.0);

/// What Channel changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Channel {
    All,
    One(usize),
    Alpha,
}

impl Channel {
    fn of(index: c_int) -> Channel {
        match index {
            1..=3 => Channel::One(index as usize - 1),
            4 => Channel::Alpha,
            _ => Channel::All,
        }
    }
}

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub channel: Channel,
    pub input: (f64, f64),
    pub output: (f64, f64),
    pub gamma: f64,
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        Settings {
            channel: Channel::of(v.int(CHANNEL, 0)),
            input: (v.f64(INPUT_START, 0.0), v.f64(INPUT_END, 1.0)),
            output: (v.f64(OUTPUT_START, 0.0), v.f64(OUTPUT_END, 1.0)),
            gamma: v.f64(GAMMA, 1.0),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        let f = |name: &CStr| fx.param(name).f64_at(time);
        Settings {
            channel: Channel::of(fx.param(CHANNEL).int_at(time)),
            input: (f(INPUT_START), f(INPUT_END)),
            output: (f(OUTPUT_START), f(OUTPUT_END)),
            gamma: f(GAMMA),
        }
    }

    /// Whether it changes nothing.
    pub(crate) fn changes_nothing(&self) -> bool {
        self.input == (0.0, 1.0) && self.output == (0.0, 1.0) && self.gamma == 1.0
    }

    /// A value through the levels (VEGAS's halves rounding down included).
    pub(crate) fn level(&self, x: f64) -> f64 {
        let (i0, i1) = self.input;
        // VEGAS's: a span under a 255th is a 255th.
        let d = if (i1 - i0).abs() < 1.0 / 255.0 {
            1.0 / 255.0
        } else {
            i1 - i0
        };
        let t = ((x - i0) / d).clamp(0.0, 1.0);
        let (o0, o1) = self.output;
        o0 + (o1 - o0) * gamma_curve(t, self.gamma) - TIE
    }

    /// The curve as a table of [`crate::accel::TABLE`] values over 0–1
    /// (the graphics card's straight lines between them).
    pub(crate) fn table(&self) -> Vec<f32> {
        let n = crate::accel::TABLE;
        (0..n)
            .map(|i| self.level(i as f64 / (n - 1) as f64).max(0.0) as f32)
            .collect()
    }

    /// The tables of the three color channels (the ones Channel leaves
    /// alone straight); `None` for Alpha.
    pub(crate) fn tables(&self) -> Option<[Vec<f32>; 3]> {
        let n = crate::accel::TABLE;
        let straight = || -> Vec<f32> { (0..n).map(|i| i as f32 / (n - 1) as f32).collect() };
        match self.channel {
            Channel::All => {
                let t = self.table();
                Some([t.clone(), t.clone(), t])
            }
            Channel::One(c) => Some(std::array::from_fn(|k| {
                if k == c { self.table() } else { straight() }
            })),
            Channel::Alpha => None,
        }
    }
}

/// Gamma's curve on `t` (0–1), VEGAS's: the line `S t` near black, then
/// the power 1 ⁄ γ — above 1 `(1 + o) t^p − o`, below `((t + o) ⁄ (1 +
/// o))^p` —, the offset `o` joining them where the line ends.
pub(crate) fn gamma_curve(t: f64, gamma: f64) -> f64 {
    let g = gamma.clamp(GAMMA_MIN, GAMMA_MAX);
    if (g - 1.0).abs() < 1e-6 {
        return t;
    }
    let p = 1.0 / g;
    let reach = 0.038_183_8 * g.powf(-0.941_805);
    let slope = g.powf(1.883_610);
    let a = t.abs();
    let y = if a < reach {
        slope * a
    } else if g > 1.0 {
        let tp = reach.powf(p);
        let o = (slope * reach - tp) / (tp - 1.0);
        (1.0 + o) * a.powf(p) - o
    } else {
        let st = (slope * reach).powf(g);
        let o = (reach - st) / (st - 1.0);
        ((a + o) / (1.0 + o)).powf(p)
    };
    y.copysign(t)
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

/// Renders: the color channels (or the one Channel picks, or alpha)
/// through the levels, negatives clipped.
fn render(fx: Fx, args: Props) -> OfxStatus {
    let s = Settings::at(fx, args.dbl(kOfxPropTime, 0));
    let level = |v: f32| s.level(v as f64).max(0.0) as f32;
    match s.channel {
        Channel::All => crate::colors::render(fx, args, &|c| c.map(level)),
        Channel::One(k) => crate::colors::render(fx, args, &|mut c| {
            c[k] = level(c[k]);
            c
        }),
        Channel::Alpha => crate::colors::render_rgba(fx, args, &|mut c| {
            c[3] = level(c[3]).min(1.0);
            c
        }),
    }
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Levels");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"The colors' range, or one channel's, remapped: input and output ends and a gamma.",
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

/// A number from `min` to `max` with VEGAS's three decimals.
fn number(fx: Fx, name: &CStr, label: &CStr, (default, min, max): (f64, f64, f64), hint: &CStr) {
    let p = fx.define_param(kOfxParamTypeDouble, name, label);
    p.set_dbl(kOfxParamPropDefault, default);
    p.set_dbl(kOfxParamPropMin, min);
    p.set_dbl(kOfxParamPropMax, max);
    p.set_dbl(kOfxParamPropDisplayMin, min);
    p.set_dbl(kOfxParamPropDisplayMax, max);
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

    // In VEGAS's order; every one has keyframes, Channel too.
    let p = fx.define_param(kOfxParamTypeChoice, CHANNEL, c"Channel");
    p.set_strs(kOfxParamPropChoiceOption, &CHANNELS);
    p.set_int(kOfxParamPropDefault, 0);
    p.set_int(kOfxParamPropAnimates, 1);
    p.set_str(
        kOfxParamPropHint,
        c"The channel the levels change: all the colors (the brightness) or one.",
    );
    number(
        fx,
        INPUT_START,
        c"Input start",
        (0.0, 0.0, 1.0),
        c"The input value that becomes Output start: darker ones go there too.",
    );
    number(
        fx,
        INPUT_END,
        c"Input end",
        (1.0, 0.0, 1.0),
        c"The input value that becomes Output end: lighter ones go there too.",
    );
    number(
        fx,
        OUTPUT_START,
        c"Output start",
        (0.0, 0.0, 1.0),
        c"The darkest value out.",
    );
    number(
        fx,
        OUTPUT_END,
        c"Output end",
        (1.0, 0.0, 1.0),
        c"The lightest value out.",
    );
    number(
        fx,
        GAMMA,
        c"Gamma",
        (1.0, GAMMA_MIN, GAMMA_MAX),
        c"Above 1 lightens the middle tones, below 1 darkens them.",
    );
}

// Presets.

/// The factory presets: VEGAS's, in its list's order.
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: f64| PresetValue::Num(vec![v]);
    let p = |name: &'static str, values: Vec<(&'static str, f64)>| Preset {
        name,
        values: values.into_iter().map(|(k, v)| (k, num(v))).collect(),
    };
    vec![
        p("Brighten", vec![("inputEnd", 0.7)]),
        p("Darken", vec![("inputStart", 0.3)]),
        p("Increase Gamma", vec![("gamma", 2.0)]),
        p("Decrease Gamma", vec![("gamma", 0.7)]),
        p(
            "Studio RGB to Computer RGB",
            vec![("inputStart", 0.063), ("inputEnd", 0.92)],
        ),
        p(
            "Computer RGB to Studio RGB",
            vec![("outputStart", 0.06), ("outputEnd", 0.92)],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(input: (f64, f64), output: (f64, f64), gamma: f64) -> Settings {
        Settings {
            channel: Channel::All,
            input,
            output,
            gamma,
        }
    }

    /// The 8-bit result, as a host's conversion makes it.
    fn byte(s: &Settings, x: u8) -> u8 {
        (s.level(x as f64 / 255.0).clamp(0.0, 1.0) * 255.0).round() as u8
    }

    /// VEGAS's presets on the calibration card (its 8-bit snapshots).
    #[test]
    fn the_presets_draw_what_vegas_draws() {
        let same = s((0.0, 1.0), (0.0, 1.0), 1.0);
        assert!(same.changes_nothing());
        for x in [0, 1, 127, 128, 254, 255] {
            assert_eq!(byte(&same, x), x);
        }
        let cases: [(Settings, &[(u8, u8)]); 6] = [
            // Brighten: 128 ⁄ 0.7 = 182.9.
            (
                s((0.0, 0.7), (0.0, 1.0), 1.0),
                &[(1, 1), (2, 3), (128, 183), (178, 254), (179, 255)],
            ),
            // Darken.
            (
                s((0.3, 1.0), (0.0, 1.0), 1.0),
                &[(76, 0), (77, 1), (96, 28), (128, 74), (224, 211)],
            ),
            // Increase Gamma (2): a toe at black, 1 to 4.
            (
                s((0.0, 1.0), (0.0, 1.0), 2.0),
                &[
                    (1, 4),
                    (2, 7),
                    (16, 49),
                    (32, 77),
                    (64, 118),
                    (128, 175),
                    (240, 247),
                ],
            ),
            // Decrease Gamma (0.7).
            (
                s((0.0, 1.0), (0.0, 1.0), 0.7),
                &[(2, 1), (16, 8), (32, 17), (64, 40), (128, 99), (240, 234)],
            ),
            // Studio RGB to Computer RGB.
            (
                s((0.063, 0.92), (0.0, 1.0), 1.0),
                &[
                    (16, 0),
                    (32, 19),
                    (64, 56),
                    (128, 131),
                    (224, 243),
                    (235, 255),
                ],
            ),
            // Computer RGB to Studio RGB.
            (
                s((0.0, 1.0), (0.06, 0.92), 1.0),
                &[(0, 15), (1, 16), (16, 29), (64, 70), (192, 180), (255, 235)],
            ),
        ];
        for (settings, values) in cases {
            for &(x, y) in values {
                assert_eq!(byte(&settings, x), y, "{settings:?} at {x}");
            }
        }
    }

    /// Gamma's curve: from black on a straight line, through 1 at 1,
    /// brighter above 1 and darker below; smooth where the line ends.
    #[test]
    fn gamma_curves_rise_from_black() {
        for g in [0.45, 0.7, 1.25, 2.0, 3.0, 5.0] {
            assert!(gamma_curve(0.0, g).abs() < 1e-12);
            assert!((gamma_curve(1.0, g) - 1.0).abs() < 1e-9, "{g}");
            for t in [0.001, 0.01, 0.2, 0.5, 0.9] {
                let y = gamma_curve(t, g);
                assert!((y > t) == (g > 1.0), "{g} at {t}");
            }
            let reach = 0.038_183_8 * g.powf(-0.941_805);
            let (below, above) = (gamma_curve(reach - 1e-9, g), gamma_curve(reach + 1e-9, g));
            assert!((below - above).abs() < 1e-6, "{g}: {below} {above}");
        }
        // Past its ends Gamma holds them.
        assert_eq!(gamma_curve(0.5, 9.0), gamma_curve(0.5, GAMMA_MAX));
        assert_eq!(gamma_curve(0.5, 0.1), gamma_curve(0.5, GAMMA_MIN));
    }

    /// One channel: the others' tables straight; Alpha has none.
    #[test]
    fn channels_pick_their_tables() {
        let mut one = s((0.0, 0.5), (0.0, 1.0), 1.0);
        one.channel = Channel::One(1);
        let [r, g, b] = one.tables().expect("tables");
        let n = r.len() - 1;
        assert_eq!(r[100], 100.0 / n as f32);
        assert_eq!(b[n], 1.0);
        // Green from 0–0.5 to 0–1.
        let x = (n / 4) as f32 / n as f32;
        assert!((g[n / 4] - 2.0 * x).abs() < 1e-3, "{}", g[n / 4]);
        assert!((g[n] - 1.0).abs() < 1e-3);
        one.channel = Channel::Alpha;
        assert!(one.tables().is_none());
    }
}
