//! Color Corrector: VEGAS's three-way color corrector. Low, Mid and High
//! each add a color — a hue, the angle where a vectorscope shows it, and
//! how much of it, the magnitude — to the dark, middle and light parts of
//! the picture; Saturation scales its colors, and Gamma, Gain and Offset
//! its light. Each wheel has two eyedroppers that take a color from the
//! preview: one adds the color itself, the other its complement (to take a
//! cast away).
//!
//! The work happens in Y′UV, BT.601's — a vectorscope's, whose angles the
//! wheels use (red at 103.5°, green at 240.7°, blue at 347.1°). A pixel's
//! luma says how much of each wheel it takes: all of Low's at black, of
//! Mid's at 0.5 and of High's at white, half of it a third of the way
//! from black or white and a fifth from 0.5, less and less beyond — bells,
//! 2^−(distance ⁄ half)². Each wheel moves U and V toward its angle by
//! half its magnitude squared times that weight, the luma kept (VEGAS's
//! presets, measured on its previews to about an 8-bit step: Remove
//! Yellow Cast's 0.3 and 0.4 move a ninth and a quarter of what 1 would).
//! Saturation scales U and V, then the luma becomes gain · γ(y) + offset
//! (in 8-bit steps: [`OFFSET_UNIT`]),
//! γ Gamma's curve: the luma raised to a power ([`exponent`]) in BT.709's
//! encoding of it ([`gamma_curve`]). The calibration card through VEGAS
//! shows Saturation (0 and 2), Gamma (0.5 and 2), Gain (0.5 and 1.5) and
//! Offset (-0.2 and 0.2) so, to an 8-bit step. Nothing is clipped
//! (VEGAS's float effects keep what passes white or black; 8-bit projects
//! hold each effect's result within 0–1).
//! Colors are taken unpremultiplied, on the values the host hands over
//! (encoded in BananaV's 8-bit projects, as VEGAS's).
//!
//! The wheels are parameters of two numbers, the angle and the magnitude,
//! with an interface of their own drawn with the draw suite
//! (`kBananaVParamPropInteract`, BananaV's): in another host they are the
//! two numbers. An eyedropper waits for a click on the preview, which the
//! overlay takes: it notes where (a parameter), and the plug-in, told of
//! it, reads its picture there.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};
use std::time::Instant;

use bv_ofx_sys::extensions::kBananaVParamPropInteract;
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Clicks, Draw, Fx, Props, interact_props, is, redraw, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.ColorCorrector";

/// The wheels, from the dark parts up.
pub(crate) const WHEELS: [&CStr; 3] = [c"low", c"mid", c"high"];
const WHEEL_LABELS: [&CStr; 3] = [c"Low", c"Mid", c"High"];
const WHEEL_HINTS: [&CStr; 3] = [
    c"The color added to the dark parts: its hue (the angle) and how much of it (the magnitude).",
    c"The color added to the middle tones: its hue (the angle) and how much of it (the magnitude).",
    c"The color added to the light parts: its hue (the angle) and how much of it (the magnitude).",
];
pub(crate) const SATURATION: &CStr = c"saturation";
pub(crate) const GAMMA: &CStr = c"gamma";
pub(crate) const GAIN: &CStr = c"gain";
pub(crate) const OFFSET: &CStr = c"offset";
/// The eyedropper waiting for a click on the preview: 0, none; else
/// 1 + 2 × its wheel, plus 1 for the complement's.
pub(crate) const PICKING: &CStr = c"picking";
/// Where the click was (canonical coordinates).
pub(crate) const PICKED: &CStr = c"picked";

/// How far, in U and V, a magnitude of 1 moves a color all of whose
/// weight is its wheel's; a magnitude m moves it m² as far (VEGAS's
/// presets measured: 0.125 at their 0.5).
pub(crate) const STRENGTH: f64 = 0.5;

/// Where each wheel's weight is whole (a luma: Low's, Mid's, High's), and
/// how far from there it is half.
const CENTRES: [f64; 3] = [0.0, 0.5, 1.0];
const HALVES: [f64; 3] = [1.0 / 3.0, 0.2, 1.0 / 3.0];

/// A wheel's own interface: its size in screen pixels.
pub(crate) const WHEEL_SIZE: [c_int; 2] = [300, 136];

// Y′UV, BT.601's: luma weights and U and V's scales.
const KR: f64 = 0.299;
const KB: f64 = 0.114;
const KG: f64 = 1.0 - KR - KB;
const SU: f64 = 0.492111;
const SV: f64 = 0.877283;

/// A color's luma, U and V.
pub(crate) fn yuv(c: [f32; 3]) -> [f64; 3] {
    let [r, g, b] = c.map(f64::from);
    let y = KR * r + KG * g + KB * b;
    [y, SU * (b - y), SV * (r - y)]
}

/// The color of a luma, U and V.
pub(crate) fn rgb([y, u, v]: [f64; 3]) -> [f64; 3] {
    let r = y + v / SV;
    let b = y + u / SU;
    [r, (y - KR * r - KB * b) / KG, b]
}

/// Where a vectorscope puts a color's hue (degrees, 0 to the right, up
/// counterclockwise).
pub(crate) fn hue_angle(c: [f32; 3]) -> f64 {
    let [_, u, v] = yuv(c);
    v.atan2(u).to_degrees().rem_euclid(360.0)
}

/// How much of each wheel a color of luma `y` takes: Low's, Mid's and
/// High's, 2^−(distance ⁄ half)² from where each is whole.
pub(crate) fn weights(y: f64) -> [f64; 3] {
    let y = y.clamp(0.0, 1.0);
    [0, 1, 2].map(|k| {
        let d = (y - CENTRES[k]) / HALVES[k];
        (-d * d).exp2()
    })
}

/// The power Gamma raises the luma to, in BT.709's encoding of it (see
/// [`gamma_curve`]). The calibration card through VEGAS's Color Corrector
/// (Gamma 0.5: 1.832, 2: 0.274) and Color Corrector (Secondary)'s presets
/// (Darken Mids' 0.85: 1.262, Boost Mids' 1.25: 0.819) lie on two lines
/// through 1: 1 + 1.663 (1 − gamma) below it, 1 − 0.726 (gamma − 1) above,
/// which reaches 0 at 2.38; past 2.36 (typed values; unmeasured) it stays
/// at 0.01.
pub(crate) fn exponent(gamma: f64) -> f64 {
    let g = gamma.max(0.01);
    if g < 1.0 {
        1.0 + 1.663 * (1.0 - g)
    } else {
        (1.0 - 0.7262 * (g - 1.0)).max(0.01)
    }
}

/// Gamma's curve on the luma `y`: raised to `power` in BT.709's encoding
/// of it, as if it were light — the calibration card through VEGAS's
/// Color Corrector, Gamma 0.5 and 2, within half an 8-bit step on average
/// (a plain power on the luma misses by up to 4 and 36 steps near black).
/// U and V are kept.
pub(crate) fn gamma_curve(y: f64, power: f64) -> f64 {
    if power == 1.0 {
        return y;
    }
    let encoded = if y < 0.018 {
        4.5 * y.max(0.0)
    } else {
        1.099 * y.powf(0.45) - 0.099
    };
    let v = encoded.powf(power);
    if v < 0.081 {
        v / 4.5
    } else {
        ((v + 0.099) / 1.099).powf(1.0 / 0.45)
    }
}

/// What Offset adds to the luma for each of its units: an 8-bit step (its
/// slider runs from -254 to 254, as VEGAS's). The calibration card through
/// VEGAS: Offset 0.2 brightens by a fifth of a step (a level in the darks
/// through ACES's view), and in 8-bit projects -0.2 and 0.2 vanish in the
/// rounding.
pub(crate) const OFFSET_UNIT: f64 = 1.0 / 255.0;

/// How far a wheel at magnitude `m` moves U and V where it has all the
/// weight.
pub(crate) fn amount(m: f64) -> f64 {
    let m = m.clamp(0.0, 1.0);
    STRENGTH * m * m
}

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    /// Each wheel's angle (degrees) and magnitude (0–1).
    pub wheels: [[f64; 2]; 3],
    pub saturation: f64,
    pub gamma: f64,
    pub gain: f64,
    pub offset: f64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            wheels: [[0.0; 2]; 3],
            saturation: 1.0,
            gamma: 1.0,
            gain: 1.0,
            offset: 0.0,
        }
    }
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            wheels: WHEELS.map(|w| fx.param(w).xy_at(time)),
            saturation: fx.param(SATURATION).f64_at(time),
            gamma: fx.param(GAMMA).f64_at(time),
            gain: fx.param(GAIN).f64_at(time),
            offset: fx.param(OFFSET).f64_at(time),
        }
    }

    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        let d = Settings::default();
        Settings {
            wheels: [0, 1, 2].map(|w| v.array(WHEELS[w], d.wheels[w])),
            saturation: v.f64(SATURATION, d.saturation),
            gamma: v.f64(GAMMA, d.gamma),
            gain: v.f64(GAIN, d.gain),
            offset: v.f64(OFFSET, d.offset),
        }
    }

    /// Whether it leaves every color as it is.
    pub(crate) fn changes_nothing(&self) -> bool {
        self.wheels.iter().all(|w| w[1] == 0.0)
            && self.saturation == 1.0
            && self.gamma == 1.0
            && self.gain == 1.0
            && self.offset == 0.0
    }
}

/// The settings made ready to correct colors.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Corrector {
    /// What each wheel adds to U and V where it has all the weight.
    pub shifts: [[f64; 2]; 3],
    pub saturation: f64,
    pub power: f64,
    pub gain: f64,
    pub offset: f64,
}

impl Corrector {
    pub(crate) fn new(s: &Settings) -> Corrector {
        Corrector {
            shifts: s.wheels.map(|[angle, m]| {
                let t = angle.to_radians();
                let a = amount(m);
                [a * t.cos(), a * t.sin()]
            }),
            saturation: s.saturation.max(0.0),
            power: exponent(s.gamma),
            gain: s.gain,
            offset: s.offset * OFFSET_UNIT,
        }
    }

    /// A straight color, corrected (unclipped).
    pub(crate) fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let [y, mut u, mut v] = yuv(c);
        let w = weights(y);
        for (k, [du, dv]) in self.shifts.iter().enumerate() {
            u += w[k] * du;
            v += w[k] * dv;
        }
        u *= self.saturation;
        v *= self.saturation;
        let y = gamma_curve(y, self.power);
        rgb([self.gain * y + self.offset, u, v]).map(|x| x as f32)
    }
}

/// The wheel's setting a color picked on the preview makes: its hue, or
/// (`complement`) the hue across from it, to take that color away from
/// pixels like it — as much of it as the wheel adds there.
pub(crate) fn wheel_for(c: [f32; 3], wheel: usize, complement: bool) -> [f64; 2] {
    let [y, u, v] = yuv(c);
    let chroma = u.hypot(v);
    if chroma < 1e-6 {
        return [0.0, 0.0];
    }
    let angle = v.atan2(u).to_degrees() + if complement { 180.0 } else { 0.0 };
    let weight = if complement {
        weights(y)[wheel.min(2)].max(0.2)
    } else {
        1.0
    };
    [
        angle.rem_euclid(360.0),
        (chroma / (STRENGTH * weight)).sqrt().min(1.0),
    ]
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
        let c = Corrector::new(&s);
        crate::colors::render(fx, args, &|rgb| c.apply(rgb))
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        if Settings::at(fx, args.dbl(kOfxPropTime, 0)).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxActionInstanceChanged) {
        if args.is(kOfxPropName, PICKED) {
            pick(fx, args.dbl(kOfxPropTime, 0));
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
    p.set_str(kOfxPropLabel, c"Color Corrector");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"Adds colors to the dark, middle and light parts of the picture and changes its saturation and light.",
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
    // The eyedroppers take their click on the preview.
    p.set_ptr(
        kOfxImageEffectPluginPropOverlayInteractV2,
        overlay as *mut c_void,
    );
}

/// Defines a number's parameter: its default, limits, the slider's range
/// and its hint.
fn number(fx: Fx, name: &CStr, label: &CStr, [default, min, max, lo, hi]: [f64; 5], hint: &CStr) {
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
}

fn describe_in_context(fx: Fx) {
    let comps = [
        kOfxImageComponentRGBA,
        kOfxImageComponentRGB,
        kOfxImageComponentAlpha,
    ];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);

    let entries: [OfxPluginEntryPoint; 3] = [low_wheel, mid_wheel, high_wheel];
    for k in 0..3 {
        let p = fx.define_param(kOfxParamTypeDouble2D, WHEELS[k], WHEEL_LABELS[k]);
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
        p.set_str(kOfxParamPropHint, WHEEL_HINTS[k]);
        // Its wheel, where the host draws with the draw suite (others
        // refuse the property and show the two numbers).
        p.set_ptr(kBananaVParamPropInteract, entries[k] as *mut c_void);
        p.set_ints(kOfxParamPropInteractPreferedSize, &WHEEL_SIZE);
    }
    number(
        fx,
        SATURATION,
        c"Saturation",
        [1.0, 0.0, 10.0, 0.0, 3.0],
        c"The colors' strength: 0 leaves greys, 1 the colors as they are.",
    );
    number(
        fx,
        GAMMA,
        c"Gamma",
        [1.0, 0.01, 10.0, 0.0, 2.0],
        c"The overall brightness: above 1 brighter, below 1 darker.",
    );
    number(
        fx,
        GAIN,
        c"Gain",
        [1.0, 0.0, 10.0, 0.0, 2.0],
        c"Multiplies the picture's light.",
    );
    number(
        fx,
        OFFSET,
        c"Offset",
        // VEGAS's slider: -254 to 254 steps.
        [0.0, -254.0, 254.0, -254.0, 254.0],
        c"Adds to the picture's brightness, in 8-bit steps.",
    );
    // The eyedroppers' state: not kept, not shown.
    let p = fx.define_param(kOfxParamTypeInteger, PICKING, c"Picking");
    p.set_int(kOfxParamPropSecret, 1);
    p.set_int(kOfxParamPropPersistant, 0);
    p.set_int(kOfxParamPropAnimates, 0);
    let p = fx.define_param(kOfxParamTypeDouble2D, PICKED, c"Picked");
    p.set_str(kOfxParamPropDoubleType, kOfxParamDoubleTypeXYAbsolute);
    p.set_int(kOfxParamPropSecret, 1);
    p.set_int(kOfxParamPropPersistant, 0);
    p.set_int(kOfxParamPropAnimates, 0);
}

// The eyedroppers.

/// The click an eyedropper waited for, on the picture at `time`: its
/// color there (a few pixels' average) sets the eyedropper's wheel.
fn pick(fx: Fx, time: f64) {
    let picking = fx.param(PICKING);
    let code = picking.int_at(time);
    picking.set_int(0);
    if code <= 0 {
        return;
    }
    let wheel = (((code - 1) / 2) as usize).min(2);
    let complement = (code - 1) % 2 == 1;
    let at = fx.param(PICKED).xy_at(time);
    let Some(c) = color_at(fx, time, at) else {
        if crate::ofx::tracing() {
            eprintln!("bananav color corrector: no picture to pick from at {time}");
        }
        return;
    };
    fx.param(WHEELS[wheel])
        .set_xy(wheel_for(c, wheel, complement));
}

/// The source's average straight color around the canonical point `at`,
/// at `time`.
pub(crate) fn color_at(fx: Fx, time: f64, at: [f64; 2]) -> Option<[f32; 3]> {
    let img = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time)?;
    let premultiplied = img.channels == 4 && !img.unpremultiplied;
    let [x1, y1, x2, y2] = img.bounds;
    let (_, size) = fx.frame();
    // Its pixels over the full size's (a preview's may be smaller).
    let scale = [
        (x2 - x1) as f64 / size[0] / img.par,
        (y2 - y1) as f64 / size[1],
    ];
    let px = [
        x1 + (at[0] * scale[0]).floor() as c_int,
        y1 + (at[1] * scale[1]).floor() as c_int,
    ];
    let (mut sum, mut n) = ([0.0f32; 3], 0);
    for y in px[1] - 2..=px[1] + 2 {
        for x in px[0] - 2..=px[0] + 2 {
            if let Some(c) = crate::colors::pixel(&img, x, y)
                .and_then(|p| crate::colors::straight(p, premultiplied))
            {
                for k in 0..3 {
                    sum[k] += c[k];
                }
                n += 1;
            }
        }
    }
    (n > 0).then(|| sum.map(|v| v / n as f32))
}

/// The overlay: while an eyedropper waits, it takes the click on the
/// preview and notes where.
unsafe extern "C" fn overlay(
    action: *const c_char,
    _handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if is(action, kOfxActionDescribe)
        || is(action, kOfxActionCreateInstance)
        || is(action, kOfxActionDestroyInstance)
        || is(action, kOfxInteractActionDraw)
    {
        return kOfxStatOK;
    }
    let args = Props(in_args);
    let fx = Fx(args.ptr(kOfxPropEffectInstance) as OfxImageEffectHandle);
    if fx.0.is_null() {
        return kOfxStatReplyDefault;
    }
    let time = args.dbl(kOfxPropTime, 0);
    if is(action, kOfxInteractActionPenDown) && fx.param(PICKING).int_at(time) > 0 {
        let at: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
        fx.param(PICKED).set_xy(at);
        return kOfxStatOK;
    }
    kOfxStatReplyDefault
}

// The wheels' own interface.

/// Where a wheel's interface has its parts, in its pixels (y up): the
/// wheel, and the eyedroppers to its right — the color's above, the
/// complement's below.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WheelLayout {
    pub centre: [f64; 2],
    pub radius: f64,
    pub droppers: [[f64; 2]; 2],
}

impl WheelLayout {
    pub(crate) fn of(size: [f64; 2]) -> WheelLayout {
        let r = ((size[1] - 16.0) / 2.0).clamp(16.0, 60.0);
        let cx = (size[0] * 0.55).min(size[0] - r - 48.0).max(r + 2.0);
        let centre = [cx, size[1] / 2.0];
        let x = cx + r + 22.0;
        WheelLayout {
            centre,
            radius: r,
            droppers: [[x, centre[1] + 0.68 * r], [x, centre[1] - 0.75 * r]],
        }
    }

    /// The angle and magnitude a point stands for.
    pub(crate) fn value_at(&self, p: [f64; 2]) -> [f64; 2] {
        let (dx, dy) = (p[0] - self.centre[0], p[1] - self.centre[1]);
        let m = (dx.hypot(dy) / self.radius).min(1.0);
        let a = if m > 0.0 {
            dy.atan2(dx).to_degrees().rem_euclid(360.0)
        } else {
            0.0
        };
        [a, m]
    }

    /// Where a value's point is.
    pub(crate) fn point_of(&self, [a, m]: [f64; 2]) -> [f64; 2] {
        let t = a.to_radians();
        let d = self.radius * m.clamp(0.0, 1.0);
        [self.centre[0] + d * t.cos(), self.centre[1] + d * t.sin()]
    }

    fn on_wheel(&self, p: [f64; 2]) -> bool {
        (p[0] - self.centre[0]).hypot(p[1] - self.centre[1]) <= self.radius + 4.0
    }

    /// The eyedropper at `p`: 0 the color's, 1 the complement's.
    pub(crate) fn dropper_at(&self, p: [f64; 2]) -> Option<usize> {
        self.droppers
            .iter()
            .position(|d| (p[0] - d[0]).abs() <= 12.0 && (p[1] - d[1]).abs() <= 12.0)
    }
}

/// What a wheel's interface remembers: a drag on it, and its presses, to
/// tell a double click.
#[derive(Debug, Default)]
struct WheelState {
    dragging: bool,
    clicks: Clicks,
}

unsafe extern "C" fn low_wheel(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    wheel(WHEELS[0], 0, action, handle, in_args)
}

unsafe extern "C" fn mid_wheel(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    wheel(WHEELS[1], 1, action, handle, in_args)
}

unsafe extern "C" fn high_wheel(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    wheel(WHEELS[2], 2, action, handle, in_args)
}

/// The interface of wheel `w`, parameter `param` (Color Corrector
/// (Secondary)'s one wheel too): it draws the wheel and its eyedroppers; a
/// click or a drag on the wheel sets it, a double click puts it back in the
/// middle, a click on an eyedropper has it wait for a click on the preview
/// (again: not) — `PICKING` 1 + 2w, plus 1 for the complement's.
pub(crate) fn wheel(
    param: &CStr,
    w: usize,
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
) -> OfxStatus {
    if is(action, kOfxActionDescribe) {
        return kOfxStatOK;
    }
    let Some(props) = interact_props(handle) else {
        return kOfxStatReplyDefault;
    };
    if is(action, kOfxActionCreateInstance) {
        let state = Box::into_raw(Box::new(WheelState::default()));
        props.set_ptr(kOfxPropInstanceData, state as *mut c_void);
        return kOfxStatOK;
    }
    let state = props.ptr(kOfxPropInstanceData) as *mut WheelState;
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
    // SAFETY: this interface's state; hosts run one of its actions at a
    // time.
    let state = unsafe { &mut *state };
    let args = Props(in_args);
    let fx = Fx(args.ptr(kOfxPropEffectInstance) as OfxImageEffectHandle);
    if fx.0.is_null() {
        return kOfxStatReplyDefault;
    }
    let time = args.dbl(kOfxPropTime, 0);
    let size: [c_int; 2] = props.ints(kOfxInteractPropViewportSize);
    let layout = WheelLayout::of([size[0] as f64, size[1] as f64]);
    let value = fx.param(param);
    if is(action, kOfxInteractActionDraw) {
        if let Some(d) = Draw::of(args) {
            let picking = fx.param(PICKING).int_at(time);
            draw_wheel(&d, &layout, value.xy_at(time), picking, w);
        }
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    if is(action, kOfxInteractActionPenDown) {
        if let Some(kind) = layout.dropper_at(pen) {
            let code = 1 + 2 * w as c_int + kind as c_int;
            let picking = fx.param(PICKING);
            picking.set_int(if picking.int_at(time) == code {
                0
            } else {
                code
            });
            redraw(handle);
            return kOfxStatOK;
        }
        if layout.on_wheel(pen) {
            if state.clicks.press(pen, Instant::now()) {
                // A double click puts it back in the middle, as VEGAS's.
                value.set_xy([0.0, 0.0]);
                return kOfxStatOK;
            }
            state.dragging = true;
            value.set_xy(layout.value_at(pen));
            return kOfxStatOK;
        }
        kOfxStatReplyDefault
    } else if is(action, kOfxInteractActionPenMotion) {
        if !state.dragging {
            return kOfxStatReplyDefault;
        }
        value.set_xy(layout.value_at(pen));
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenUp) {
        if std::mem::take(&mut state.dragging) {
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else {
        kOfxStatReplyDefault
    }
}

/// The wheel's grey in its middle, and the colors of its lines and names.
const MIDDLE: f32 = 0.55;
const LINES: [f32; 4] = [0.42, 0.42, 0.42, 0.6];
const DEGREES: [f32; 4] = [0.48, 0.48, 0.48, 0.85];
const NAMES: [f32; 4] = [0.1, 0.1, 0.1, 0.75];
/// Text centred on its point.
const CENTRED: c_int = kOfxDrawTextAlignmentLeft
    | kOfxDrawTextAlignmentRight
    | kOfxDrawTextAlignmentTop
    | kOfxDrawTextAlignmentBaseline;

/// A circle's outline as `n` points.
fn circle(c: [f64; 2], r: f64, n: usize) -> Vec<[f64; 2]> {
    (0..n)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / n as f64;
            [c[0] + r * t.cos(), c[1] + r * t.sin()]
        })
        .collect()
}

/// The wheel's color at its rim at `angle`: the hue a vectorscope shows
/// there, bright and strong (its channels from 0.2 to 1, as VEGAS's).
pub(crate) fn rim(angle: f64) -> [f32; 4] {
    let t = angle.to_radians();
    let c = rgb([0.5, 0.1 * t.cos(), 0.1 * t.sin()]);
    let (hi, lo) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
    let k = |x: f64| (0.2 + 0.8 * (x - lo) / (hi - lo).max(1e-9)) as f32;
    [k(c[0]), k(c[1]), k(c[2]), 1.0]
}

/// Draws wheel `w` with its value (angle, magnitude) and its eyedroppers
/// (`picking` says which waits).
fn draw_wheel(d: &Draw, l: &WheelLayout, value: [f64; 2], picking: c_int, w: usize) {
    let ([cx, cy], r) = (l.centre, l.radius);
    let at = |a: f64, k: f64| {
        let t = a.to_radians();
        [cx + k * t.cos(), cy + k * t.sin()]
    };
    // The hues around it, strongest at the rim…
    const SECTORS: usize = 120;
    let step = 360.0 / SECTORS as f64;
    for k in 0..SECTORS {
        let a = k as f64 * step;
        d.colour(rim(a + step / 2.0));
        d.fill(&[[cx, cy], at(a, r), at(a + step / 2.0, r), at(a + step, r)]);
    }
    // …grey toward its middle: discs of grey over them, each fainter than
    // the one inside it, so the rings in between go from the hue to grey.
    const RINGS: usize = 30;
    for k in 1..=RINGS {
        let alpha = 1.0 / (k as f32 + 0.5);
        d.colour([MIDDLE, MIDDLE, MIDDLE, alpha]);
        d.fill(&circle([cx, cy], r * k as f64 / RINGS as f64, 72));
    }
    d.colour([0.08, 0.08, 0.08, 1.0]);
    d.width(1.5);
    d.outline(&circle([cx, cy], r, 96));
    // The axes, with room for their degrees, and a tick every 15°.
    d.colour(LINES);
    d.width(1.0);
    let mut lines = Vec::new();
    for k in 0..24 {
        let a = k as f64 * 15.0;
        if k % 6 == 0 {
            lines.extend([at(a, 0.65 * r), at(a, r), at(a, 0.0), at(a, 0.32 * r)]);
        } else {
            lines.extend([at(a, 0.84 * r), at(a, r)]);
        }
    }
    d.lines(&lines);
    d.colour(DEGREES);
    for (a, text) in [
        (0.0, c"0°"),
        (90.0, c"90°"),
        (180.0, c"180°"),
        (270.0, c"270°"),
    ] {
        d.text(text, at(a, 0.5 * r), CENTRED);
    }
    // The colors' names, where a vectorscope puts them.
    d.colour(NAMES);
    let names: [(&CStr, [f32; 3]); 6] = [
        (c"R", [1.0, 0.0, 0.0]),
        (c"Mg", [1.0, 0.0, 1.0]),
        (c"B", [0.0, 0.0, 1.0]),
        (c"Cy", [0.0, 1.0, 1.0]),
        (c"G", [0.0, 1.0, 0.0]),
        (c"Yl", [1.0, 1.0, 0.0]),
    ];
    for (text, c) in names {
        d.text(text, at(hue_angle(c), 0.84 * r), CENTRED);
    }
    // The value: a black ring and a white cross.
    let p = l.point_of(value);
    d.colour([0.0, 0.0, 0.0, 1.0]);
    d.width(2.5);
    d.outline(&circle(p, 7.5, 24));
    d.colour([1.0, 1.0, 1.0, 1.0]);
    d.lines(&[
        [p[0] - 15.0, p[1]],
        [p[0] + 15.0, p[1]],
        [p[0], p[1] - 15.0],
        [p[0], p[1] + 15.0],
    ]);
    for (kind, &at) in l.droppers.iter().enumerate() {
        let on = picking == 1 + 2 * w as c_int + kind as c_int;
        eyedropper(d, at, kind == 1, on);
    }
}

/// An eyedropper at `at`: a pipette and its badge — a green plus for the
/// color, a red minus for its complement; boxed while it waits.
fn eyedropper(d: &Draw, [x, y]: [f64; 2], complement: bool, on: bool) {
    if on {
        let b = [
            [x - 12.0, y - 12.0],
            [x + 12.0, y - 12.0],
            [x + 12.0, y + 12.0],
            [x - 12.0, y + 12.0],
        ];
        d.colour([0.0, 0.47, 0.84, 0.35]);
        d.fill(&b);
        d.colour([0.0, 0.47, 0.84, 1.0]);
        d.width(1.0);
        d.outline(&b);
    }
    // The pipette, from its tip (down left) to its bulb.
    d.colour([0.92, 0.92, 0.92, 1.0]);
    d.width(3.0);
    d.lines(&[[x - 6.0, y - 6.0], [x + 3.0, y + 3.0]]);
    d.colour([0.3, 0.3, 0.3, 1.0]);
    d.width(1.2);
    d.lines(&[[x - 8.5, y - 8.5], [x - 6.0, y - 6.0]]);
    d.colour([0.62, 0.62, 0.62, 1.0]);
    d.fill(&circle([x + 5.0, y + 5.0], 3.4, 12));
    let (b, colour) = if complement {
        ([x + 6.5, y - 5.5], [0.93, 0.42, 0.36, 1.0])
    } else {
        ([x - 6.5, y + 5.5], [0.1, 0.8, 0.44, 1.0])
    };
    d.colour(colour);
    d.fill(&circle(b, 5.2, 16));
    d.colour([1.0, 1.0, 1.0, 1.0]);
    d.width(1.6);
    let mut sign = vec![[b[0] - 2.8, b[1]], [b[0] + 2.8, b[1]]];
    if !complement {
        sign.extend([[b[0], b[1] - 2.8], [b[0], b[1] + 2.8]]);
    }
    d.lines(&sign);
}

// Presets.

/// Where a vectorscope puts red, green and blue, as VEGAS's presets give
/// them.
const RED: f64 = 103.5;
const GREEN: f64 = 240.7;
const BLUE: f64 = 347.1;

/// The factory presets, VEGAS's: their names and their wheels.
pub(crate) fn presets() -> Vec<Preset> {
    let wheel = |name: &'static str, angle: f64, magnitude: f64| {
        (name, PresetValue::Num(vec![angle, magnitude]))
    };
    let mid = |name, angle| Preset {
        name,
        values: vec![wheel("mid", angle, 0.5)],
    };
    let high = |name, angle| Preset {
        name,
        values: vec![wheel("high", angle, 0.5)],
    };
    vec![
        mid("Red Midtones", RED),
        mid("Green Midtones", GREEN),
        mid("Blue Midtones", BLUE),
        high("Red Highlight", RED),
        high("Green Highlight", GREEN),
        high("Blue Highlight", BLUE),
        Preset {
            name: "Remove Yellow Cast",
            values: vec![
                wheel("low", BLUE, 0.3),
                wheel("mid", BLUE, 0.4),
                wheel("high", BLUE, 0.3),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64, by: f64) -> bool {
        (a - b).abs() <= by
    }

    #[test]
    fn the_wheels_angles_are_a_vectorscopes() {
        // VEGAS's presets: red 103.5°, green 240.7°, blue 347.1°.
        assert!(near(hue_angle([1.0, 0.0, 0.0]), RED, 0.05));
        assert!(near(hue_angle([0.0, 1.0, 0.0]), GREEN, 0.05));
        assert!(near(hue_angle([0.0, 0.0, 1.0]), BLUE, 0.05));
        // Y′UV there and back.
        let c = [0.2f32, 0.7, 0.4];
        let back = rgb(yuv(c));
        for k in 0..3 {
            assert!(near(back[k], c[k] as f64, 1e-6));
        }
        // The weights: the dark parts Low's, the middle Mid's, the light
        // High's — whole at black, 0.5 and white, half a third of the way
        // from black and white and a fifth from 0.5.
        assert_eq!(weights(0.0)[0], 1.0);
        assert_eq!(weights(0.5)[1], 1.0);
        assert_eq!(weights(1.0)[2], 1.0);
        assert!(near(weights(1.0 / 3.0)[0], 0.5, 1e-12));
        assert!(near(weights(0.3)[1], 0.5, 1e-12) && near(weights(0.7)[1], 0.5, 1e-12));
        assert!(near(weights(2.0 / 3.0)[2], 0.5, 1e-12));
        assert!(weights(1.0)[0] < 0.002 && weights(0.0)[2] < 0.002);
    }

    /// How far a grey of luma `y` moves along `angle` under `s`.
    fn moved(s: &Settings, y: f32, angle: f64) -> f64 {
        let [_, u, v] = yuv(Corrector::new(s).apply([y; 3]));
        let t = angle.to_radians();
        u * t.cos() + v * t.sin()
    }

    #[test]
    fn vegass_presets_move_greys_as_its_previews_show() {
        // Measured on VEGAS's previews of its presets (flat parts of one
        // picture, binned by luma): how far U and V moved along the wheel.
        let preset = |name: &str| {
            let p = presets().into_iter().find(|p| p.name == name).unwrap();
            let mut s = Settings::default();
            for (wheel, value) in p.values {
                let PresetValue::Num(v) = value else {
                    panic!("{value:?}")
                };
                let k = WHEELS.iter().position(|w| w.to_str() == Ok(wheel)).unwrap();
                s.wheels[k] = [v[0], v[1]];
            }
            s
        };
        for (name, angle, measured) in [
            (
                "Red Midtones",
                RED,
                &[
                    (0.11, 0.0087),
                    (0.31, 0.0677),
                    (0.49, 0.1241),
                    (0.69, 0.0653),
                ][..],
            ),
            (
                "Green Midtones",
                GREEN,
                &[(0.21, 0.0290), (0.51, 0.1247), (0.79, 0.0275)],
            ),
            (
                "Blue Highlight",
                BLUE,
                &[
                    (0.31, 0.0051),
                    (0.51, 0.0293),
                    (0.69, 0.0692),
                    (0.79, 0.0960),
                ],
            ),
            (
                "Red Highlight",
                RED,
                &[(0.41, 0.0137), (0.61, 0.0498), (0.77, 0.0884)],
            ),
            (
                "Remove Yellow Cast",
                BLUE,
                &[
                    (0.085, 0.0472),
                    (0.264, 0.0611),
                    (0.505, 0.0982),
                    (0.745, 0.0598),
                ],
            ),
        ] {
            let s = preset(name);
            for &(y, want) in measured {
                let got = moved(&s, y as f32, angle);
                assert!(near(got, want, 0.0025), "{name} at {y}: {got} vs {want}");
            }
        }
        // Light past white stays (8-bit projects clip it, as VEGAS's 8-bit
        // pictures): white goes redder than red's full channel, its green
        // and blue drop.
        let white = Corrector::new(&preset("Red Highlight")).apply([1.0; 3]);
        assert!(white[0] > 1.0, "{white:?}");
        assert!(near(white[1] as f64, 0.941, 0.003), "{white:?}");
    }

    #[test]
    fn red_midtones_redden_the_middle_and_keep_its_light() {
        let s = Settings {
            wheels: [[0.0, 0.0], [RED, 0.5], [0.0, 0.0]],
            ..Settings::default()
        };
        let c = Corrector::new(&s);
        let grey = [0.5f32; 3];
        let out = c.apply(grey);
        assert!(out[0] > 0.6 && out[1] < 0.47 && out[2] < 0.47, "{out:?}");
        assert!(near(yuv(out)[0], 0.5, 1e-5));
        // Black and white barely move.
        let black = c.apply([0.0; 3]);
        assert!(black.iter().all(|&v| v < 0.01), "{black:?}");
        // At its defaults it changes nothing.
        assert!(Settings::default().changes_nothing());
        let same = Corrector::new(&Settings::default()).apply([0.3, 0.6, 0.9]);
        for (a, b) in same.iter().zip([0.3, 0.6, 0.9]) {
            assert!(near(*a as f64, b, 1e-6));
        }
    }

    #[test]
    fn saturation_gamma_gain_and_offset() {
        let c = |s: Settings, x: [f32; 3]| Corrector::new(&s).apply(x);
        let grey = c(
            Settings {
                saturation: 0.0,
                ..Settings::default()
            },
            [0.8, 0.2, 0.4],
        );
        assert!(
            near(grey[0] as f64, grey[2] as f64, 1e-6)
                && near(grey[0] as f64, grey[1] as f64, 1e-6)
        );
        // Gamma as the calibration card through VEGAS shows it: its grays
        // (0–255) at 0.5 and 2, to two 8-bit steps.
        for (gamma, grays) in [
            (
                0.5,
                [
                    (12.0, 3.0),
                    (64.0, 24.0),
                    (128.0, 74.0),
                    (192.0, 152.0),
                    (240.0, 228.0),
                ],
            ),
            (
                2.0,
                [
                    (4.0, 62.0),
                    (24.0, 126.0),
                    (64.0, 172.0),
                    (128.0, 211.0),
                    (224.0, 246.0),
                ],
            ),
        ] {
            for (grey, vegas) in grays {
                let out = c(
                    Settings {
                        gamma,
                        ..Settings::default()
                    },
                    [grey / 255.0; 3],
                );
                assert!(
                    (out[0] * 255.0 - vegas).abs() < 2.0,
                    "{gamma} at {grey}: {out:?}"
                );
            }
        }
        // Its colors keep U and V.
        let [_, u, v] = yuv([0.8, 0.3, 0.2]);
        let out = c(
            Settings {
                gamma: 2.0,
                ..Settings::default()
            },
            [0.8, 0.3, 0.2],
        );
        let [_, u2, v2] = yuv(out);
        assert!(near(u, u2, 1e-6) && near(v, v2, 1e-6));
        let doubled = c(
            Settings {
                gain: 2.0,
                ..Settings::default()
            },
            [0.2; 3],
        );
        assert!(near(doubled[1] as f64, 0.4, 1e-6));
        // Offset counts 8-bit steps.
        let raised = c(
            Settings {
                offset: 0.5,
                ..Settings::default()
            },
            [0.2; 3],
        );
        assert!(near(raised[2] as f64, 0.2 + 0.5 / 255.0, 1e-6));
    }

    #[test]
    fn the_complements_eyedropper_takes_a_cast_away() {
        // A middle grey gone blue.
        let cast = [0.42f32, 0.47, 0.62];
        let v = wheel_for(cast, 1, true);
        let s = Settings {
            wheels: [[0.0, 0.0], v, [0.0, 0.0]],
            ..Settings::default()
        };
        let out = Corrector::new(&s).apply(cast);
        let [_, u, w] = yuv(out);
        assert!(u.hypot(w) < 0.01, "{out:?}");
        // The color's own: its hue.
        let red = wheel_for([0.8, 0.3, 0.3], 2, false);
        assert!(near(red[0], RED, 1.0) && red[1] > 0.5);
    }

    #[test]
    fn the_wheel_gives_back_the_value_of_a_point() {
        let l = WheelLayout::of([300.0, 136.0]);
        assert_eq!(l.radius, 60.0);
        for v in [[103.5, 0.5], [240.7, 1.0], [347.1, 0.25]] {
            let back = l.value_at(l.point_of(v));
            assert!(near(back[0], v[0], 1e-9) && near(back[1], v[1], 1e-9));
        }
        // Outside the rim: as far as it goes.
        assert_eq!(l.value_at([l.centre[0] + 200.0, l.centre[1]])[1], 1.0);
        assert_eq!(l.dropper_at(l.droppers[1]), Some(1));
        assert_eq!(l.dropper_at(l.centre), None);
    }

    #[test]
    fn the_presets_are_vegass() {
        let p = presets();
        let names: Vec<&str> = p.iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            [
                "Red Midtones",
                "Green Midtones",
                "Blue Midtones",
                "Red Highlight",
                "Green Highlight",
                "Blue Highlight",
                "Remove Yellow Cast"
            ]
        );
        assert_eq!(p[6].values[1], ("mid", PresetValue::Num(vec![347.1, 0.4])));
    }
}
