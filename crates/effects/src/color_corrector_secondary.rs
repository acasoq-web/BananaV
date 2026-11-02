//! Color Corrector (Secondary): VEGAS's. The Color Corrector's corrections
//! — a color added (Chrominance's wheel, with its eyedroppers), the hue
//! turned, the saturation scaled, the light's gamma, gain and offset, and
//! the alpha — on the parts of the picture its limits pick: a range of
//! luminance, of saturation and of hue, each softened by its Smooth. Show
//! mask shows what is picked, white; Invert mask picks the rest; Select
//! effect range sets the limits from a click or a rectangle on the
//! preview.
//!
//! VEGAS's math, measured on its pictures (its six presets): in Y′UV,
//! BT.601's, as the Color Corrector's — luminance is Y × 255, saturation
//! |UV| × 255 (pure red's 0.632 is the slider's 162), hue the
//! vectorscope's angle. Each limit is whole inside its range and falls
//! away over Smooth outside it — luminance's as a smoothstep, hue's in a
//! straight line (saturation's, not shown by VEGAS's presets, as
//! luminance's) — and the three's product, squared, is the mask that mixes
//! the corrected color in: Rotate hue turns U and V, Chrominance adds to
//! them as the Color Corrector's wheels do where they have all the weight,
//! Saturation scales them, and the luma goes through Gamma's curve (the
//! Color Corrector's [`gamma_curve`]) times Gain plus Offset (8-bit steps,
//! as the Color Corrector's). The colors are not
//! clipped, as the Color Corrector's; Alpha scales the alpha where the
//! mask is.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::extensions::{kBananaVParamPropInteract, kBananaVParamPropInteractOpen};
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::color_corrector::{self, WHEEL_SIZE, amount, exponent, gamma_curve, rgb, yuv};
use crate::ofx::{Draw, Fx, Props, interact_props, is, redraw, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.ColorCorrectorSecondary";

const CHROMINANCE: &CStr = c"chrominance";
const ROTATE: &CStr = c"rotateHue";
const SATURATION: &CStr = c"saturation";
const GAMMA: &CStr = c"gamma";
const GAIN: &CStr = c"gain";
const OFFSET: &CStr = c"offset";
const ALPHA: &CStr = c"alpha";
const SELECT: &CStr = c"selectRange";
const SHOW: &CStr = c"showMask";
const INVERT: &CStr = c"invertMask";
/// Each limit's group and its parameters: enable, low (center), high
/// (width), smooth.
const LIMITS: [(&CStr, &CStr, [&CStr; 4]); 3] = [
    (
        c"limitLuminance",
        c"Limit luminance",
        [c"lumaEnable", c"lumaLow", c"lumaHigh", c"lumaSmooth"],
    ),
    (
        c"limitSaturation",
        c"Limit saturation",
        [c"satEnable", c"satLow", c"satHigh", c"satSmooth"],
    ),
    (
        c"limitHue",
        c"Limit hue",
        [c"hueEnable", c"hueCenter", c"hueWidth", c"hueSmooth"],
    ),
];
/// What an eyedropper or Select effect range waits for (0: nothing): the
/// wheel's color 1, its complement 2 (as the Color Corrector's wheel 0),
/// a range 3.
const PICKING: &CStr = c"picking";
const RANGE: c_int = 3;
/// Where a pick ended (canonical coordinates), and where a range's
/// rectangle started.
const PICKED: &CStr = c"picked";
const PICKED_FROM: &CStr = c"pickedFrom";

/// The saturation slider's end: pure red's (and cyan's) |UV|, × 255.
pub(crate) const SATURATION_MAX: f64 = 162.0;

/// Below this chroma a color is grey: its hue 0°, as an 8-bit grey's.
const GREY: f64 = 1e-6;

/// A limit of luminance or saturation (0–255 steps): whole from `low` to
/// `high`, gone `smooth` beyond.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Range {
    pub enabled: bool,
    pub low: f64,
    pub high: f64,
    pub smooth: f64,
}

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    /// Chrominance's angle (degrees) and magnitude.
    pub chrominance: [f64; 2],
    pub rotate: f64,
    pub saturation: f64,
    pub gamma: f64,
    pub gain: f64,
    pub offset: f64,
    pub alpha: f64,
    pub show_mask: bool,
    pub invert: bool,
    pub luminance: Range,
    pub saturation_range: Range,
    /// The hue limit: center, width and smooth in degrees.
    pub hue: Range,
}

impl Default for Settings {
    fn default() -> Settings {
        let range = |high| Range {
            enabled: true,
            low: 0.0,
            high,
            smooth: 0.0,
        };
        Settings {
            chrominance: [0.0, 0.0],
            rotate: 0.0,
            saturation: 1.0,
            gamma: 1.0,
            gain: 1.0,
            offset: 0.0,
            alpha: 1.0,
            show_mask: false,
            invert: false,
            luminance: range(255.0),
            saturation_range: range(SATURATION_MAX),
            hue: Range {
                enabled: true,
                low: 180.0,
                high: 360.0,
                smooth: 0.0,
            },
        }
    }
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        let num = |n: &CStr| fx.param(n).f64_at(time);
        let flag = |n: &CStr| fx.param(n).int_at(time) != 0;
        let range = |k: usize| {
            let [enable, low, high, smooth] = LIMITS[k].2;
            Range {
                enabled: flag(enable),
                low: num(low),
                high: num(high),
                smooth: num(smooth),
            }
        };
        Settings {
            chrominance: fx.param(CHROMINANCE).xy_at(time),
            rotate: num(ROTATE),
            saturation: num(SATURATION),
            gamma: num(GAMMA),
            gain: num(GAIN),
            offset: num(OFFSET),
            alpha: num(ALPHA),
            show_mask: flag(SHOW),
            invert: flag(INVERT),
            luminance: range(0),
            saturation_range: range(1),
            hue: range(2),
        }
    }

    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        let d = Settings::default();
        let range = |k: usize, d: Range| {
            let [enable, low, high, smooth] = LIMITS[k].2;
            Range {
                enabled: v.int(enable, d.enabled as c_int) != 0,
                low: v.f64(low, d.low),
                high: v.f64(high, d.high),
                smooth: v.f64(smooth, d.smooth),
            }
        };
        Settings {
            chrominance: v.array(CHROMINANCE, d.chrominance),
            rotate: v.f64(ROTATE, d.rotate),
            saturation: v.f64(SATURATION, d.saturation),
            gamma: v.f64(GAMMA, d.gamma),
            gain: v.f64(GAIN, d.gain),
            offset: v.f64(OFFSET, d.offset),
            alpha: v.f64(ALPHA, d.alpha),
            show_mask: v.int(SHOW, 0) != 0,
            invert: v.int(INVERT, 0) != 0,
            luminance: range(0, d.luminance),
            saturation_range: range(1, d.saturation_range),
            hue: range(2, d.hue),
        }
    }

    /// Whether it leaves every pixel as it is.
    pub(crate) fn changes_nothing(&self) -> bool {
        !self.show_mask
            && self.chrominance[1] == 0.0
            && self.rotate.rem_euclid(360.0) == 0.0
            && self.saturation == 1.0
            && self.gamma == 1.0
            && self.gain == 1.0
            && self.offset == 0.0
            && self.alpha == 1.0
    }
}

/// The settings made ready: the limits in the picture's units (0–1, hue
/// in degrees), the corrections' numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Secondary {
    /// Luminance's and saturation's limits (low, high, smooth; 0–1), the
    /// hue's (center, half its width, smooth; degrees); `None`: off.
    pub luminance: Option<[f64; 3]>,
    pub saturation_range: Option<[f64; 3]>,
    pub hue: Option<[f64; 3]>,
    pub invert: bool,
    pub show_mask: bool,
    /// Rotate hue's cosine and sine.
    pub turn: [f64; 2],
    /// What Chrominance adds to U and V.
    pub shift: [f64; 2],
    pub saturation: f64,
    pub power: f64,
    pub gain: f64,
    pub offset: f64,
    pub alpha: f64,
}

/// 1 inside `lo`–`hi`, falling in a straight line to 0 over `smooth`
/// beyond (at once without it).
fn ramp(x: f64, lo: f64, hi: f64, smooth: f64) -> f64 {
    let d = (lo - x).max(x - hi);
    if d <= 0.0 {
        1.0
    } else if smooth <= 0.0 {
        0.0
    } else {
        (1.0 - d / smooth).max(0.0)
    }
}

fn smoothstep(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

impl Secondary {
    pub(crate) fn new(s: &Settings) -> Secondary {
        let unit = |r: Range| {
            r.enabled
                .then_some([r.low / 255.0, r.high / 255.0, r.smooth.max(0.0) / 255.0])
        };
        let t = s.rotate.to_radians();
        let [angle, m] = s.chrominance;
        let a = amount(m);
        Secondary {
            luminance: unit(s.luminance),
            saturation_range: unit(s.saturation_range),
            hue: s.hue.enabled.then_some([
                s.hue.low,
                s.hue.high.max(0.0) / 2.0,
                s.hue.smooth.max(0.0),
            ]),
            invert: s.invert,
            show_mask: s.show_mask,
            turn: [t.cos(), t.sin()],
            shift: [a * angle.to_radians().cos(), a * angle.to_radians().sin()],
            saturation: s.saturation.max(0.0),
            power: exponent(s.gamma),
            gain: s.gain,
            offset: s.offset * color_corrector::OFFSET_UNIT,
            alpha: s.alpha.clamp(0.0, 1.0),
        }
    }

    /// How much of the correction a color of luma `y` and chroma `u`, `v`
    /// takes.
    pub(crate) fn mask(&self, [y, u, v]: [f64; 3]) -> f64 {
        let mut e = 1.0;
        if let Some([lo, hi, smooth]) = self.luminance {
            e *= smoothstep(ramp(y.clamp(0.0, 1.0), lo, hi, smooth));
        }
        let chroma = u.hypot(v);
        if let Some([lo, hi, smooth]) = self.saturation_range {
            let s = chroma.min(SATURATION_MAX / 255.0);
            e *= smoothstep(ramp(s, lo, hi, smooth));
        }
        if let Some([center, half, smooth]) = self.hue {
            // A grey's hue is 0° (its rounding's would be anything).
            let h = if chroma < GREY {
                0.0
            } else {
                v.atan2(u).to_degrees()
            };
            let d = ((h - center + 180.0).rem_euclid(360.0) - 180.0).abs();
            e *= ramp(d, 0.0, half, smooth);
        }
        let m = e * e;
        if self.invert { 1.0 - m } else { m }
    }

    /// A straight color and its alpha, corrected where the mask says.
    pub(crate) fn apply(&self, [r, g, b, a]: [f32; 4]) -> [f32; 4] {
        let [y, u, v] = yuv([r, g, b]);
        let m = self.mask([y, u, v]);
        if self.show_mask {
            let k = m as f32;
            return [k, k, k, a];
        }
        let [c, s] = self.turn;
        let u1 = (u * c - v * s + self.shift[0]) * self.saturation;
        let v1 = (u * s + v * c + self.shift[1]) * self.saturation;
        let y1 = self.gain * gamma_curve(y, self.power) + self.offset;
        let out = rgb([y + m * (y1 - y), u + m * (u1 - u), v + m * (v1 - v)]);
        let alpha = a * (1.0 + m as f32 * (self.alpha as f32 - 1.0));
        [out[0] as f32, out[1] as f32, out[2] as f32, alpha]
    }
}

/// The limits picked colors span: luminance's and saturation's lowest and
/// highest (0–255 steps), the hue's center and width (degrees; `None` when
/// they are all grey).
pub(crate) type Span = ([f64; 2], [f64; 2], Option<[f64; 2]>);

/// The limits a set of straight colors (picked on the preview) spans.
pub(crate) fn span(colors: &[[f32; 3]]) -> Option<Span> {
    if colors.is_empty() {
        return None;
    }
    let yuvs: Vec<[f64; 3]> = colors.iter().map(|c| yuv(*c)).collect();
    let (mut luma, mut sat) = ([f64::MAX, f64::MIN], [f64::MAX, f64::MIN]);
    let (mut cx, mut cy) = (0.0, 0.0);
    for &[y, u, v] in &yuvs {
        let y = y.clamp(0.0, 1.0) * 255.0;
        let s = (u.hypot(v) * 255.0).min(SATURATION_MAX);
        luma = [luma[0].min(y), luma[1].max(y)];
        sat = [sat[0].min(s), sat[1].max(s)];
        // The hues' mean, each as strong as its color.
        cx += u;
        cy += v;
    }
    // Greys' hues mean nothing: a hue only from colors (2 steps' chroma).
    let colored: Vec<f64> = yuvs
        .iter()
        .filter(|[_, u, v]| u.hypot(*v) * 255.0 >= 2.0)
        .map(|[_, u, v]| v.atan2(*u).to_degrees())
        .collect();
    let hue = (!colored.is_empty() && cx.hypot(cy) > 1e-9).then(|| {
        let center = cy.atan2(cx).to_degrees().rem_euclid(360.0);
        let reach = colored
            .iter()
            .map(|h| ((h - center + 180.0).rem_euclid(360.0) - 180.0).abs())
            .fold(0.0, f64::max);
        [center, (2.0 * reach).min(360.0)]
    });
    let round = |v: f64| (v * 1000.0).round() / 1000.0;
    Some((luma.map(round), sat.map(round), hue.map(|h| h.map(round))))
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
        let s = Secondary::new(&Settings::at(fx, args.dbl(kOfxPropTime, 0)));
        crate::colors::render_rgba(fx, args, &|p| s.apply(p))
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        if Settings::at(fx, args.dbl(kOfxPropTime, 0)).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxActionInstanceChanged) {
        let time = args.dbl(kOfxPropTime, 0);
        if args.is(kOfxPropName, PICKED) {
            pick(fx, time);
        } else if args.is(kOfxPropName, SELECT) {
            // Select effect range waits for the preview (again: not).
            let picking = fx.param(PICKING);
            picking.set_int(if picking.int_at(time) == RANGE {
                0
            } else {
                RANGE
            });
        }
        kOfxStatOK
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
    p.set_str(kOfxPropLabel, c"Color Corrector (Secondary)");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"Corrects only the colors it picks: a range of brightness, of saturation and of hue.",
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
    // Its eyedroppers and Select effect range take their clicks on the
    // preview.
    p.set_ptr(
        kOfxImageEffectPluginPropOverlayInteractV2,
        overlay as *mut c_void,
    );
}

/// Defines a number's parameter: its default, limits, the slider's range
/// and its hint (under `parent`, a group, when there is one).
fn number(
    fx: Fx,
    name: &CStr,
    label: &CStr,
    [default, min, max, lo, hi]: [f64; 5],
    hint: &CStr,
    parent: Option<&CStr>,
) {
    let p = fx.define_param(kOfxParamTypeDouble, name, label);
    p.set_dbl(kOfxParamPropDefault, default);
    p.set_dbl(kOfxParamPropMin, min);
    p.set_dbl(kOfxParamPropMax, max);
    p.set_dbl(kOfxParamPropDisplayMin, lo);
    p.set_dbl(kOfxParamPropDisplayMax, hi);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_dbl(kOfxParamPropIncrement, 0.01);
    p.set_int(kOfxParamPropAnimates, 1);
    p.set_str(kOfxParamPropHint, hint);
    if let Some(parent) = parent {
        p.set_str(kOfxParamPropParent, parent);
    }
}

/// Defines a check box.
fn check(fx: Fx, name: &CStr, label: &CStr, default: bool, hint: &CStr, parent: Option<&CStr>) {
    let p = fx.define_param(kOfxParamTypeBoolean, name, label);
    p.set_int(kOfxParamPropDefault, default as c_int);
    p.set_int(kOfxParamPropAnimates, 1);
    p.set_str(kOfxParamPropHint, hint);
    if let Some(parent) = parent {
        p.set_str(kOfxParamPropParent, parent);
    }
}

fn describe_in_context(fx: Fx) {
    let comps = [
        kOfxImageComponentRGBA,
        kOfxImageComponentRGB,
        kOfxImageComponentAlpha,
    ];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);

    let p = fx.define_param(kOfxParamTypeDouble2D, CHROMINANCE, c"Chrominance");
    p.set_str(kOfxParamPropDoubleType, kOfxParamDoubleTypePlain);
    p.set_dbls(kOfxParamPropDefault, &[0.0, 0.0]);
    p.set_dbls(kOfxParamPropMin, &[0.0, 0.0]);
    p.set_dbls(kOfxParamPropMax, &[360.0, 1.0]);
    p.set_dbls(kOfxParamPropDisplayMin, &[0.0, 0.0]);
    p.set_dbls(kOfxParamPropDisplayMax, &[360.0, 1.0]);
    p.set_strs(kOfxParamPropDimensionLabel, &[c"Angle", c"Magnitude"]);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_dbl(kOfxParamPropIncrement, 0.01);
    p.set_int(kOfxParamPropAnimates, 1);
    p.set_str(
        kOfxParamPropHint,
        c"The color added to what the limits pick: its hue (the angle) and how much of it (the magnitude).",
    );
    // Its wheel, the Color Corrector's, under its triangle at first, as
    // VEGAS's.
    p.set_ptr(kBananaVParamPropInteract, chrominance_wheel as *mut c_void);
    p.set_ints(kOfxParamPropInteractPreferedSize, &WHEEL_SIZE);
    p.set_int(kBananaVParamPropInteractOpen, 0);
    number(
        fx,
        ROTATE,
        c"Rotate hue",
        [0.0, 0.0, 360.0, 0.0, 360.0],
        c"Turns the hues of what the limits pick, in degrees around the vectorscope.",
        None,
    );
    number(
        fx,
        SATURATION,
        c"Saturation",
        [1.0, 0.0, 10.0, 0.0, 3.0],
        c"The strength of the picked colors: 0 leaves greys, 1 the colors as they are.",
        None,
    );
    number(
        fx,
        GAMMA,
        c"Gamma",
        [1.0, 0.01, 10.0, 0.0, 2.0],
        c"The brightness of what the limits pick: above 1 brighter, below 1 darker.",
        None,
    );
    number(
        fx,
        GAIN,
        c"Gain",
        [1.0, 0.0, 10.0, 0.0, 2.0],
        c"Multiplies the light of what the limits pick.",
        None,
    );
    number(
        fx,
        OFFSET,
        c"Offset",
        // As the Color Corrector's: -254 to 254 steps.
        [0.0, -254.0, 254.0, -254.0, 254.0],
        c"Adds to the brightness of what the limits pick, in 8-bit steps.",
        None,
    );
    number(
        fx,
        ALPHA,
        c"Alpha",
        [1.0, 0.0, 1.0, 0.0, 1.0],
        c"How opaque what the limits pick stays: 0 makes it see-through.",
        None,
    );
    let p = fx.define_param(kOfxParamTypePushButton, SELECT, c"Select effect range");
    p.set_str(
        kOfxParamPropHint,
        c"Click a color on the preview, or drag a rectangle over the colors, to set the limits to them.",
    );
    check(
        fx,
        SHOW,
        c"Show mask",
        false,
        c"Shows what the limits pick in white, the rest in black.",
        None,
    );
    check(
        fx,
        INVERT,
        c"Invert mask",
        false,
        c"Corrects what the limits leave out instead.",
        None,
    );
    // Each limit's low (center), high (width) and smooth: default, limits
    // and slider (as `number` takes them), and their hints.
    type Limit<'a> = ([f64; 5], [f64; 5], [f64; 5], [&'a CStr; 3]);
    let limits: [Limit<'_>; 3] = [
        (
            [0.0, 0.0, 255.0, 0.0, 255.0],
            [255.0, 0.0, 255.0, 0.0, 255.0],
            [0.0, 0.0, 255.0, 0.0, 255.0],
            [
                c"The darkest luminance picked (0–255).",
                c"The lightest luminance picked (0–255).",
                c"How far beyond the range the pick fades away.",
            ],
        ),
        (
            [0.0, 0.0, SATURATION_MAX, 0.0, SATURATION_MAX],
            [SATURATION_MAX, 0.0, SATURATION_MAX, 0.0, SATURATION_MAX],
            [0.0, 0.0, SATURATION_MAX, 0.0, SATURATION_MAX],
            [
                c"The least saturation picked (162: pure red's).",
                c"The most saturation picked (162: pure red's).",
                c"How far beyond the range the pick fades away.",
            ],
        ),
        (
            [180.0, 0.0, 360.0, 0.0, 360.0],
            [360.0, 0.0, 360.0, 0.0, 360.0],
            [0.0, 0.0, 180.0, 0.0, 180.0],
            [
                c"The hue in the middle of the range, in degrees around the vectorscope.",
                c"How wide the range of hues is, in degrees.",
                c"How many degrees beyond the range the pick fades away.",
            ],
        ),
    ];
    let enable_hints = [
        c"Picks by brightness.",
        c"Picks by how saturated the colors are.",
        c"Picks by hue.",
    ];
    for (k, (group, label, [enable, low, high, smooth])) in LIMITS.iter().enumerate() {
        let g = fx.define_param(kOfxParamTypeGroup, group, label);
        g.set_int(kOfxParamPropGroupOpen, 0);
        let (lo, hi, sm, hints) = limits[k];
        check(fx, enable, c"Enable", true, enable_hints[k], Some(group));
        let (low_label, high_label) = if k == 2 {
            (c"Center", c"Width")
        } else {
            (c"Low", c"High")
        };
        number(fx, low, low_label, lo, hints[0], Some(group));
        number(fx, high, high_label, hi, hints[1], Some(group));
        number(fx, smooth, c"Smooth", sm, hints[2], Some(group));
    }
    // The eyedroppers' and the range's state: not kept, not shown.
    let p = fx.define_param(kOfxParamTypeInteger, PICKING, c"Picking");
    p.set_int(kOfxParamPropSecret, 1);
    p.set_int(kOfxParamPropPersistant, 0);
    p.set_int(kOfxParamPropAnimates, 0);
    for (name, label) in [(PICKED, c"Picked"), (PICKED_FROM, c"Picked from")] {
        let p = fx.define_param(kOfxParamTypeDouble2D, name, label);
        p.set_str(kOfxParamPropDoubleType, kOfxParamDoubleTypeXYAbsolute);
        p.set_int(kOfxParamPropSecret, 1);
        p.set_int(kOfxParamPropPersistant, 0);
        p.set_int(kOfxParamPropAnimates, 0);
    }
}

// Picking on the preview.

/// A click (or Select effect range's rectangle) that was waited for, on
/// the picture at `time`.
fn pick(fx: Fx, time: f64) {
    let picking = fx.param(PICKING);
    let code = picking.int_at(time);
    picking.set_int(0);
    let to = fx.param(PICKED).xy_at(time);
    match code {
        1 | 2 => {
            let Some(c) = color_corrector::color_at(fx, time, to) else {
                return;
            };
            fx.param(CHROMINANCE).set_xy(wheel_for(c, code == 2));
        }
        RANGE => {
            let from = fx.param(PICKED_FROM).xy_at(time);
            let Some((luma, sat, hue)) = colors_in(fx, time, from, to).as_deref().and_then(span)
            else {
                return;
            };
            let set = |name: &CStr, v: f64| fx.param(name).set_f64(v);
            let flag = |name: &CStr| fx.param(name).set_int(1);
            let [l, s, h] = LIMITS.map(|l| l.2);
            flag(l[0]);
            set(l[1], luma[0]);
            set(l[2], luma[1]);
            flag(s[0]);
            set(s[1], sat[0]);
            set(s[2], sat[1]);
            if let Some([center, width]) = hue {
                flag(h[0]);
                set(h[1], center);
                set(h[2], width);
            }
        }
        _ => {}
    }
}

/// The Chrominance a color picked on the preview sets: its hue, or
/// (`complement`) the one across, as much as takes it away there.
pub(crate) fn wheel_for(c: [f32; 3], complement: bool) -> [f64; 2] {
    let [_, u, v] = yuv(c);
    let chroma = u.hypot(v);
    if chroma < 1e-6 {
        return [0.0, 0.0];
    }
    let angle = v.atan2(u).to_degrees() + if complement { 180.0 } else { 0.0 };
    [
        angle.rem_euclid(360.0),
        (chroma / color_corrector::STRENGTH).sqrt().min(1.0),
    ]
}

/// The source's straight colors in the rectangle from `from` to `to`
/// (canonical coordinates; a click: the few pixels around it), at `time`.
fn colors_in(fx: Fx, time: f64, from: [f64; 2], to: [f64; 2]) -> Option<Vec<[f32; 3]>> {
    let img = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time)?;
    let premultiplied = img.channels == 4 && !img.unpremultiplied;
    let [x1, y1, x2, y2] = img.bounds;
    let (_, size) = fx.frame();
    let scale = [
        (x2 - x1) as f64 / size[0] / img.par,
        (y2 - y1) as f64 / size[1],
    ];
    let px = |p: [f64; 2]| {
        [
            x1 + (p[0] * scale[0]).floor() as c_int,
            y1 + (p[1] * scale[1]).floor() as c_int,
        ]
    };
    let (a, b) = (px(from), px(to));
    let (mut lo, mut hi) = (
        [a[0].min(b[0]), a[1].min(b[1])],
        [a[0].max(b[0]), a[1].max(b[1])],
    );
    if lo == hi {
        // A click: the 5 × 5 pixels around it, as an eyedropper's.
        lo = [lo[0] - 2, lo[1] - 2];
        hi = [hi[0] + 2, hi[1] + 2];
    }
    let (lo, hi) = (
        [lo[0].max(x1), lo[1].max(y1)],
        [hi[0].min(x2 - 1), hi[1].min(y2 - 1)],
    );
    let mut out = Vec::new();
    for y in lo[1]..=hi[1] {
        for x in lo[0]..=hi[0] {
            if let Some(c) = crate::colors::pixel(&img, x, y)
                .and_then(|p| crate::colors::straight(p, premultiplied))
            {
                out.push(c);
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

/// What the overlay remembers: a rectangle being dragged (its corners,
/// canonical coordinates).
#[derive(Debug, Default)]
struct Overlay {
    drag: Option<([f64; 2], [f64; 2])>,
}

/// The overlay: while an eyedropper waits, it takes the click on the
/// preview; while Select effect range waits, a click or a rectangle,
/// drawn as it is dragged.
unsafe extern "C" fn overlay(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if is(action, kOfxActionDescribe) {
        return kOfxStatOK;
    }
    let Some(props) = interact_props(handle) else {
        return kOfxStatReplyDefault;
    };
    if is(action, kOfxActionCreateInstance) {
        let state = Box::into_raw(Box::new(Overlay::default()));
        props.set_ptr(kOfxPropInstanceData, state as *mut c_void);
        return kOfxStatOK;
    }
    let state = props.ptr(kOfxPropInstanceData) as *mut Overlay;
    if is(action, kOfxActionDestroyInstance) {
        if !state.is_null() {
            // SAFETY: made above, freed once.
            drop(unsafe { Box::from_raw(state) });
            props.set_ptr(kOfxPropInstanceData, std::ptr::null_mut());
        }
        return kOfxStatOK;
    }
    if state.is_null() {
        return kOfxStatReplyDefault;
    }
    // SAFETY: this overlay's state; hosts run one of its actions at a time.
    let state = unsafe { &mut *state };
    let args = Props(in_args);
    let fx = Fx(args.ptr(kOfxPropEffectInstance) as OfxImageEffectHandle);
    if fx.0.is_null() {
        return kOfxStatReplyDefault;
    }
    let time = args.dbl(kOfxPropTime, 0);
    if is(action, kOfxInteractActionDraw) {
        if let (Some((a, b)), Some(d)) = (state.drag, Draw::of(args)) {
            let rect = [a, [b[0], a[1]], b, [a[0], b[1]]];
            for (colour, width) in [([0.0, 0.0, 0.0, 0.8], 3.0), ([1.0, 1.0, 1.0, 1.0], 1.2)] {
                d.colour(colour);
                d.width(width);
                d.outline(&rect);
            }
        }
        return kOfxStatOK;
    }
    let picking = fx.param(PICKING).int_at(time);
    if picking <= 0 {
        return kOfxStatReplyDefault;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    if is(action, kOfxInteractActionPenDown) {
        if picking == RANGE {
            state.drag = Some((pen, pen));
            redraw(handle);
        } else {
            fx.param(PICKED).set_xy(pen);
        }
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        let Some((from, _)) = state.drag else {
            return kOfxStatReplyDefault;
        };
        state.drag = Some((from, pen));
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenUp) {
        let Some((from, _)) = state.drag.take() else {
            return kOfxStatReplyDefault;
        };
        redraw(handle);
        fx.param(PICKED_FROM).set_xy(from);
        fx.param(PICKED).set_xy(pen);
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

/// Chrominance's wheel: the Color Corrector's (wheel 0's eyedroppers).
unsafe extern "C" fn chrominance_wheel(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    color_corrector::wheel(CHROMINANCE, 0, action, handle, in_args)
}

// Presets.

/// The factory presets: VEGAS's.
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: f64| PresetValue::Num(vec![v]);
    let preset =
        |name: &'static str, values: Vec<(&'static str, PresetValue)>| Preset { name, values };
    let mids = |gamma: f64, saturation: f64| {
        vec![
            ("saturation", num(saturation)),
            ("gamma", num(gamma)),
            ("lumaLow", num(128.0)),
            ("lumaHigh", num(128.0)),
            ("lumaSmooth", num(128.0)),
        ]
    };
    vec![
        preset("Invert Color", vec![("rotateHue", num(180.0))]),
        preset(
            "Desaturate All but Reds",
            vec![
                ("saturation", num(0.0)),
                ("hueCenter", num(283.5)),
                ("hueWidth", num(300.0)),
                ("hueSmooth", num(15.0)),
            ],
        ),
        preset(
            "Desaturate Highs",
            vec![
                ("saturation", num(0.0)),
                ("lumaLow", num(192.0)),
                ("lumaSmooth", num(96.0)),
            ],
        ),
        preset("Boost Mids", mids(1.25, 1.25)),
        preset("Darken Mids", mids(0.85, 1.0)),
        preset(
            "Saturate Greens",
            vec![
                ("saturation", num(2.5)),
                ("hueCenter", num(240.7)),
                ("hueWidth", num(30.0)),
                ("hueSmooth", num(30.0)),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64, by: f64) -> bool {
        (a - b).abs() <= by
    }

    fn with(f: impl FnOnce(&mut Settings)) -> Secondary {
        let mut s = Settings::default();
        f(&mut s);
        Secondary::new(&s)
    }

    #[test]
    fn at_its_defaults_it_picks_everything_and_changes_nothing() {
        let s = Settings::default();
        assert!(s.changes_nothing());
        let c = Secondary::new(&s);
        for col in [
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.2, 0.7, 0.4],
        ] {
            assert_eq!(c.mask(yuv(col)), 1.0, "{col:?}");
            let out = c.apply([col[0], col[1], col[2], 1.0]);
            for k in 0..3 {
                assert!(near(out[k] as f64, col[k] as f64, 1e-6), "{col:?} {out:?}");
            }
        }
    }

    #[test]
    fn the_limits_fall_away_as_vegass_pictures_show() {
        // Desaturate Highs (luminance 192–255, Smooth 96): 30 below the
        // range a smoothstep's square — VEGAS's pictures, 0.549 at
        // 156–168 and 0.826 around 104.
        let highs = with(|s| {
            s.luminance.low = 192.0;
            s.luminance.smooth = 96.0;
        });
        let grey = |v: f64| yuv([v as f32; 3]);
        assert_eq!(highs.mask(grey(200.0 / 255.0)), 1.0);
        assert!(near(highs.mask(grey(162.0 / 255.0)), 0.549, 0.045));
        assert_eq!(highs.mask(grey(90.0 / 255.0)), 0.0);
        let mids = with(|s| {
            s.luminance.low = 128.0;
            s.luminance.high = 128.0;
            s.luminance.smooth = 128.0;
        });
        assert!(near(mids.mask(grey(104.0 / 255.0)), 0.824, 0.01));
        // Desaturate All but Reds (hue 283.5 ± 150, Smooth 15): a straight
        // line squared — VEGAS's, 0.637 3° beyond and 0.361 6° beyond.
        let reds = with(|s| {
            s.hue = Range {
                enabled: true,
                low: 283.5,
                high: 300.0,
                smooth: 15.0,
            };
        });
        let at = |deg: f64| {
            let t = deg.to_radians();
            [0.5, 0.2 * t.cos(), 0.2 * t.sin()]
        };
        assert!(near(reds.mask(at(130.5)), 0.64, 1e-9));
        assert!(near(reds.mask(at(127.5)), 0.36, 1e-9));
        assert_eq!(reds.mask(at(103.5)), 0.0);
        assert_eq!(reds.mask(at(200.0)), 1.0);
        // Inverted: the rest.
        let inverted = Secondary {
            invert: true,
            ..reds
        };
        assert_eq!(inverted.mask(at(103.5)), 1.0);
    }

    #[test]
    fn the_corrections_are_vegass() {
        // Invert Color: U and V across, the luma kept.
        let invert = with(|s| s.rotate = 180.0);
        let c = [0.8f32, 0.4, 0.2];
        let [y0, u0, v0] = yuv(c);
        let out = invert.apply([c[0], c[1], c[2], 1.0]);
        let [y1, u1, v1] = yuv([out[0], out[1], out[2]]);
        assert!(near(y1, y0, 1e-6) && near(u1, -u0, 1e-6) && near(v1, -v0, 1e-6));
        // Saturate Greens: a green's color 2.5 times as strong.
        let greens = Secondary::new(&{
            let mut s = Settings::default();
            for (name, value) in presets()
                .into_iter()
                .find(|p| p.name == "Saturate Greens")
                .unwrap()
                .values
            {
                let PresetValue::Num(v) = value else { panic!() };
                match name {
                    "saturation" => s.saturation = v[0],
                    "hueCenter" => s.hue.low = v[0],
                    "hueWidth" => s.hue.high = v[0],
                    "hueSmooth" => s.hue.smooth = v[0],
                    _ => panic!("{name}"),
                }
            }
            s
        });
        let green = [0.35f32, 0.45, 0.33];
        let [_, u, v] = yuv(green);
        assert!(near(v.atan2(u).to_degrees().rem_euclid(360.0), 240.0, 15.0));
        let out = greens.apply([green[0], green[1], green[2], 1.0]);
        let [_, u2, v2] = yuv([out[0], out[1], out[2]]);
        assert!(near(u2.hypot(v2) / u.hypot(v), 2.5, 1e-4));
        // Darken Mids: a middle grey darker, as its picture's (about the
        // power 1.262).
        let darken = with(|s| s.gamma = 0.85);
        let out = darken.apply([0.5, 0.5, 0.5, 1.0]);
        assert!(near(out[1] as f64, 0.5f64.powf(1.262), 0.01), "{out:?}");
        // Alpha: what is picked goes see-through; Show mask shows it.
        let clear = with(|s| {
            s.alpha = 0.0;
            s.luminance.low = 128.0;
        });
        assert_eq!(clear.apply([0.9, 0.9, 0.9, 1.0])[3], 0.0);
        assert_eq!(clear.apply([0.1, 0.1, 0.1, 1.0])[3], 1.0);
        let show = Secondary {
            show_mask: true,
            ..clear
        };
        assert_eq!(show.apply([0.9, 0.9, 0.9, 1.0]), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(show.apply([0.1, 0.1, 0.1, 0.5]), [0.0, 0.0, 0.0, 0.5]);
    }

    #[test]
    fn a_range_spans_the_colors_picked() {
        let (luma, sat, hue) = span(&[[0.8, 0.3, 0.3], [0.6, 0.2, 0.25], [0.5, 0.5, 0.5]]).unwrap();
        let y = |c: [f32; 3]| yuv(c)[0] * 255.0;
        assert!(near(luma[0], y([0.6, 0.2, 0.25]), 1e-3) && near(luma[1], y([0.5; 3]), 1e-3));
        // The grey's saturation is 0; the reds' hues about red's.
        assert_eq!(sat[0], 0.0);
        let [center, width] = hue.unwrap();
        assert!(
            near(center, 103.5, 10.0) && width < 30.0,
            "{center} {width}"
        );
        // Greys only: no hue.
        assert!(span(&[[0.3; 3], [0.7; 3]]).unwrap().2.is_none());
        // The eyedroppers: a red's hue, or the one across.
        let w = wheel_for([0.8, 0.3, 0.3], true);
        assert!(near(w[0], 283.5, 1.0) && w[1] > 0.5, "{w:?}");
    }

    #[test]
    fn the_presets_are_vegass() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            [
                "Invert Color",
                "Desaturate All but Reds",
                "Desaturate Highs",
                "Boost Mids",
                "Darken Mids",
                "Saturate Greens"
            ]
        );
    }
}
