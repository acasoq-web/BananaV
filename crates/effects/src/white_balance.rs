//! White Balance: VEGAS's. Takes a light's cast away: the light is the
//! neutral color picked (what should be gray) or, the same thing, its color
//! temperature and tint; Amount of correction is how far the colors move
//! against it, set by itself whenever the light changes to what takes the
//! light's own color exactly to gray; Brightness scales the picture's luma.
//!
//! VEGAS's numbers, worked out on its pictures: the neutral color's numbers
//! are taken as ACES (AP0) RGB, their chromaticity gives the temperature
//! and tint as Adobe's DNG SDK works them out (Robertson's isotemperature
//! lines, the tint −3000 times the distance off the Planckian locus), and a
//! temperature and tint give back the color of that light with half
//! luminance: mid gray is 5999 K and 10, VEGAS's defaults (the ACES white,
//! D60). The picture moves in Y′UV, as VEGAS's Color Corrector works
//! (BT.601's luma, analog TV's U and V; the values the host hands over: in
//! BananaV's 8-bit projects, as VEGAS's, the encoded ones): every color's
//! U and V shift against the light's — its temperature and tint's, so a
//! color picked past the Tint slider's end counts as that end — by
//! Amount² ⁄ 2, and Y scales by Brightness; negatives are clipped. The
//! amount set by itself is the light's color's own √(2·|UV|): the color
//! picked's when one is, its temperature and tint's neutral color's when
//! they are typed. Equalize Histogram, after all that (clipped to 0–1, as
//! VEGAS's 8-bit pictures are), stretches each channel from its lowest to
//! its highest value over the frame and then holds red and green under
//! blue, VEGAS's way: warm colors go gray, cool ones stay.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.WhiteBalance";

const NEUTRAL: &CStr = c"neutral";
const TEMPERATURE: &CStr = c"temperature";
const TINT: &CStr = c"tint";
const AMOUNT: &CStr = c"amount";
const BRIGHTNESS: &CStr = c"brightness";
const EQUALIZE: &CStr = c"equalize";

/// The temperatures and tints a light can have (VEGAS's sliders).
const TEMPERATURES: [f64; 2] = [1700.0, 15000.0];
const TINTS: [f64; 2] = [-100.0, 100.0];

type M3 = [[f64; 3]; 3];

/// ACES 2065-1 (AP0) RGB to XYZ, and back: its white is D60.
const AP0: M3 = [
    [0.952_552_395_9, 0.0, 0.000_093_678_6],
    [0.343_966_449_8, 0.728_166_096_6, -0.072_132_546_4],
    [0.0, 0.0, 1.008_825_184_4],
];
const AP0_INV: M3 = [
    [1.049_811_017_5, 0.0, -0.000_097_484_5],
    [-0.495_903_023_1, 1.373_313_045_8, 0.098_240_036_1],
    [0.0, 0.0, 0.991_252_018_2],
];

/// BT.601's luma weights (Y′UV's, as the Color Corrector's).
const LUMA: [f64; 3] = [0.299, 0.587, 0.114];

fn mul(m: &M3, v: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// Robertson's isotemperature lines: reciprocal megakelvin, u, v and the
/// slope (DNG SDK's table).
const ROBERTSON: [[f64; 4]; 31] = [
    [0.0, 0.18006, 0.26352, -0.24341],
    [10.0, 0.18066, 0.26589, -0.25479],
    [20.0, 0.18133, 0.26846, -0.26876],
    [30.0, 0.18208, 0.27119, -0.28539],
    [40.0, 0.18293, 0.27407, -0.30470],
    [50.0, 0.18388, 0.27709, -0.32675],
    [60.0, 0.18494, 0.28021, -0.35156],
    [70.0, 0.18611, 0.28342, -0.37915],
    [80.0, 0.18740, 0.28668, -0.40955],
    [90.0, 0.18880, 0.28997, -0.44278],
    [100.0, 0.19032, 0.29326, -0.47888],
    [125.0, 0.19462, 0.30141, -0.58204],
    [150.0, 0.19962, 0.30921, -0.70471],
    [175.0, 0.20525, 0.31647, -0.84901],
    [200.0, 0.21142, 0.32312, -1.0182],
    [225.0, 0.21807, 0.32909, -1.2168],
    [250.0, 0.22511, 0.33439, -1.4512],
    [275.0, 0.23247, 0.33904, -1.7298],
    [300.0, 0.24010, 0.34308, -2.0637],
    [325.0, 0.24702, 0.34655, -2.4681],
    [350.0, 0.25591, 0.34951, -2.9641],
    [375.0, 0.26400, 0.35200, -3.5814],
    [400.0, 0.27218, 0.35407, -4.3633],
    [425.0, 0.28039, 0.35577, -5.3762],
    [450.0, 0.28863, 0.35714, -6.7262],
    [475.0, 0.29685, 0.35823, -8.5955],
    [500.0, 0.30505, 0.35907, -11.324],
    [525.0, 0.31320, 0.35968, -15.628],
    [550.0, 0.32129, 0.36011, -23.325],
    [575.0, 0.32931, 0.36038, -40.770],
    [600.0, 0.33724, 0.36051, -116.45],
];

const TINT_SCALE: f64 = -3000.0;

/// The temperature and tint of chromaticity `xy`.
pub(crate) fn temperature_tint([x, y]: [f64; 2]) -> [f64; 2] {
    let d = 1.5 - x + 6.0 * y;
    let (u, v) = (2.0 * x / d, 3.0 * y / d);
    let (mut last_dt, mut last_du, mut last_dv) = (0.0, 0.0, 0.0);
    let last = ROBERTSON.len() - 1;
    for i in 1..=last {
        let len = (1.0 + ROBERTSON[i][3] * ROBERTSON[i][3]).sqrt();
        let (du, dv) = (1.0 / len, ROBERTSON[i][3] / len);
        let (uu, vv) = (u - ROBERTSON[i][1], v - ROBERTSON[i][2]);
        let dt = -uu * dv + vv * du;
        if dt <= 0.0 || i == last {
            let dt = -dt.min(0.0);
            let f = if i == 1 { 0.0 } else { dt / (last_dt + dt) };
            let r = ROBERTSON[i - 1][0] * f + ROBERTSON[i][0] * (1.0 - f);
            let temperature = 1.0e6 / r;
            let uu = u - (ROBERTSON[i - 1][1] * f + ROBERTSON[i][1] * (1.0 - f));
            let vv = v - (ROBERTSON[i - 1][2] * f + ROBERTSON[i][2] * (1.0 - f));
            let (du, dv) = (du * (1.0 - f) + last_du * f, dv * (1.0 - f) + last_dv * f);
            let len = du.hypot(dv);
            return [temperature, (uu * du / len + vv * dv / len) * TINT_SCALE];
        }
        (last_dt, last_du, last_dv) = (dt, du, dv);
    }
    [6500.0, 0.0]
}

/// The chromaticity of a light of `temperature` and `tint`.
pub(crate) fn chromaticity(temperature: f64, tint: f64) -> [f64; 2] {
    let r = 1.0e6 / temperature.max(1.0);
    let offset = tint / TINT_SCALE;
    let last = ROBERTSON.len() - 2;
    for i in 0..=last {
        if r < ROBERTSON[i + 1][0] || i == last {
            let (a, b) = (ROBERTSON[i], ROBERTSON[i + 1]);
            let f = (b[0] - r) / (b[0] - a[0]);
            let mut u = a[1] * f + b[1] * (1.0 - f);
            let mut v = a[2] * f + b[2] * (1.0 - f);
            let unit = |t: f64| {
                let len = (1.0 + t * t).sqrt();
                (1.0 / len, t / len)
            };
            let ((u1, v1), (u2, v2)) = (unit(a[3]), unit(b[3]));
            let (u3, v3) = (u1 * f + u2 * (1.0 - f), v1 * f + v2 * (1.0 - f));
            let len = u3.hypot(v3);
            u += u3 / len * offset;
            v += v3 / len * offset;
            let d = u - 4.0 * v + 2.0;
            return [1.5 * u / d, v / d];
        }
    }
    [0.3127, 0.329]
}

/// The temperature and tint of a neutral color (its numbers as AP0 RGB),
/// within the sliders' ranges; `None` for black.
pub(crate) fn temperature_of(neutral: [f64; 3]) -> Option<[f64; 2]> {
    let xyz = mul(&AP0, neutral);
    let sum = xyz[0] + xyz[1] + xyz[2];
    if !(sum > 1e-9 && xyz[1] > 1e-9) {
        return None;
    }
    let [t, tint] = temperature_tint([xyz[0] / sum, xyz[1] / sum]);
    Some([
        t.clamp(TEMPERATURES[0], TEMPERATURES[1]),
        tint.clamp(TINTS[0], TINTS[1]),
    ])
}

/// The neutral color of a light of `temperature` and `tint`: its color at
/// half luminance, as AP0 RGB numbers.
pub(crate) fn neutral_of(temperature: f64, tint: f64) -> [f64; 3] {
    let [x, y] = chromaticity(temperature, tint);
    let xyz = [0.5 * x / y, 0.5, 0.5 * (1.0 - x - y) / y];
    mul(&AP0_INV, xyz).map(|v| v.max(0.0))
}

/// The temperature and tint of mid gray: the sliders' defaults.
fn gray_light() -> [f64; 2] {
    temperature_of([0.5; 3]).unwrap_or([6000.0, 10.0])
}

/// A color's U and V.
fn uv(c: [f64; 3]) -> [f64; 2] {
    let [_, u, v] = crate::color_corrector::yuv(c.map(|x| x as f32));
    [u, v]
}

/// The amount that takes `color` exactly to gray, VEGAS's: what it sets
/// when a neutral color is picked or a temperature or tint typed.
pub(crate) fn amount_for(color: [f64; 3]) -> f64 {
    let [u, v] = uv(color);
    (2.0 * u.hypot(v)).sqrt().min(1.0)
}

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub temperature: f64,
    pub tint: f64,
    pub amount: f64,
    pub brightness: f64,
    pub equalize: bool,
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        let [t, tint] = gray_light();
        Settings {
            temperature: v.f64(TEMPERATURE, t),
            tint: v.f64(TINT, tint),
            amount: v.f64(AMOUNT, 0.0).clamp(0.0, 1.0),
            brightness: v.f64(BRIGHTNESS, 1.0).max(0.0),
            equalize: v.int(EQUALIZE, 0) != 0,
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            temperature: fx.param(TEMPERATURE).f64_at(time),
            tint: fx.param(TINT).f64_at(time),
            amount: fx.param(AMOUNT).f64_at(time).clamp(0.0, 1.0),
            brightness: fx.param(BRIGHTNESS).f64_at(time).max(0.0),
            equalize: fx.param(EQUALIZE).int_at(time) != 0,
        }
    }

    /// How every color moves: against the light's color, in U and V, by
    /// Amount² ⁄ 2 (an RGB step with no luma).
    fn shift(&self) -> [f64; 3] {
        let t = self.temperature.clamp(TEMPERATURES[0], TEMPERATURES[1]);
        let tint = self.tint.clamp(TINTS[0], TINTS[1]);
        let [u, v] = uv(neutral_of(t, tint));
        let size = u.hypot(v);
        if !size.is_finite() || size <= 1e-9 || self.amount <= 0.0 {
            return [0.0; 3];
        }
        let k = -self.amount * self.amount / 2.0 / size;
        crate::color_corrector::rgb([0.0, k * u, k * v])
    }

    /// What the picture's colors go through: `matrix` (rows) times the
    /// color, plus `offset`, then negatives clipped. The matrix scales the
    /// luma by the brightness; the offset is the light's shift.
    pub(crate) fn affine(&self) -> ([[f32; 3]; 3], [f32; 3]) {
        let k = self.brightness - 1.0;
        let m: M3 = std::array::from_fn(|i| {
            std::array::from_fn(|j| f64::from(u8::from(i == j)) + k * LUMA[j])
        });
        (
            m.map(|r| r.map(|v| v as f32)),
            self.shift().map(|v| v as f32),
        )
    }

    fn changes_nothing(&self) -> bool {
        let (m, o) = self.affine();
        !self.equalize
            && (0..3).all(|i| {
                o[i].abs() < 1e-7
                    && (0..3).all(|j| (m[i][j] - f32::from(u8::from(i == j))).abs() < 1e-7)
            })
    }
}

/// A color through `matrix` and `offset` (`Settings::affine`).
pub(crate) fn apply(m: &[[f32; 3]; 3], o: &[f32; 3], c: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| (m[i][0] * c[0] + m[i][1] * c[1] + m[i][2] * c[2] + o[i]).max(0.0))
}

/// Equalize Histogram's levels: each channel's lowest and highest value
/// over the frame, the white balance done (clipped to 0–1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Levels {
    pub lo: [f32; 3],
    pub hi: [f32; 3],
}

impl Levels {
    /// Of no color yet.
    pub(crate) fn none() -> Levels {
        Levels {
            lo: [f32::MAX; 3],
            hi: [f32::MIN; 3],
        }
    }

    /// Takes `c` in.
    pub(crate) fn add(&mut self, c: [f32; 3]) {
        for ((lo, hi), v) in self.lo.iter_mut().zip(&mut self.hi).zip(c) {
            *lo = lo.min(v);
            *hi = hi.max(v);
        }
    }

    /// A white-balanced color (`balanced`) stretched, each channel from
    /// the frame's lowest to its highest (a channel all one value stays),
    /// then red and green held under blue.
    pub(crate) fn equalize(&self, c: [f32; 3]) -> [f32; 3] {
        let y: [f32; 3] = std::array::from_fn(|i| {
            let span = self.hi[i] - self.lo[i];
            if span > 1e-6 {
                ((c[i] - self.lo[i]) / span).clamp(0.0, 1.0)
            } else {
                c[i]
            }
        });
        [y[0].min(y[2]), y[1].min(y[2]), y[2]]
    }
}

/// A color through the white balance, clipped to 0–1: what Equalize
/// Histogram works on.
pub(crate) fn balanced(m: &[[f32; 3]; 3], o: &[f32; 3], c: [f32; 3]) -> [f32; 3] {
    apply(m, o, c).map(|v| v.min(1.0))
}

/// The levels of the source's whole picture at `time`, white-balanced;
/// `None` when it has no picture.
fn frame_levels(fx: Fx, time: f64, m: &[[f32; 3]; 3], o: &[f32; 3]) -> Option<Levels> {
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time)?;
    let premultiplied = src.channels == 4 && !src.unpremultiplied;
    let [x1, y1, x2, y2] = src.bounds;
    let mut levels = Levels::none();
    for y in y1..y2 {
        for x in x1..x2 {
            let color = crate::colors::pixel(&src, x, y)
                .and_then(|px| crate::colors::straight(px, premultiplied));
            if let Some(c) = color {
                levels.add(balanced(m, o, c));
            }
        }
    }
    Some(levels)
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
        let time = args.dbl(kOfxPropTime, 0);
        let s = Settings::at(fx, time);
        let (m, o) = s.affine();
        if s.equalize {
            // The whole frame's levels first (the regions of interest ask
            // for all of it).
            let levels = frame_levels(fx, time, &m, &o).unwrap_or_else(Levels::none);
            crate::colors::render(fx, args, &|c| levels.equalize(balanced(&m, &o, c)))
        } else {
            crate::colors::render(fx, args, &|c| apply(&m, &o, c))
        }
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        if Settings::at(fx, args.dbl(kOfxPropTime, 0)).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxActionInstanceChanged) {
        // The light picked one way shows the other, as VEGAS's does: the
        // neutral color's temperature and tint, or the color of the
        // temperature and tint; and the amount becomes what takes that
        // color to gray.
        if args.is(kOfxPropType, kOfxTypeParameter)
            && args.is(kOfxPropChangeReason, kOfxChangeUserEdited)
        {
            let time = args.dbl(kOfxPropTime, 0);
            if args.is(kOfxPropName, NEUTRAL) {
                let neutral = fx.param(NEUTRAL).rgb_at(time);
                if let Some([t, tint]) = temperature_of(neutral) {
                    fx.param(TEMPERATURE).set_f64(t);
                    fx.param(TINT).set_f64(tint);
                    fx.param(AMOUNT).set_f64(amount_for(neutral));
                }
            } else if args.is(kOfxPropName, TEMPERATURE) || args.is(kOfxPropName, TINT) {
                let t = fx.param(TEMPERATURE).f64_at(time);
                let tint = fx.param(TINT).f64_at(time);
                let neutral = neutral_of(t, tint);
                fx.param(NEUTRAL).set_rgb(neutral);
                fx.param(AMOUNT).set_f64(amount_for(neutral));
            }
        }
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // Each output pixel needs the source's pixel there only; Equalize
        // Histogram, the whole picture's levels.
        let time = args.dbl(kOfxPropTime, 0);
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        let roi = if Settings::at(fx, time).equalize {
            fx.clip(kOfxImageEffectSimpleSourceClipName)
                .rod(time)
                .unwrap_or(roi)
        } else {
            roi
        };
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
    p.set_str(kOfxPropLabel, c"White Balance");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"Takes a light's cast away: the color that should be gray, or the light's temperature and tint.",
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

    let gray = [0.5; 3];
    let [t0, tint0] = gray_light();

    let p = fx.define_param(kOfxParamTypeRGB, NEUTRAL, c"Select neutral color");
    p.set_dbls(kOfxParamPropDefault, &gray);
    // VEGAS shows it in Lab.
    p.set_str(bv_ofx_sys::extensions::kBananaVParamPropColorModel, c"Lab");
    p.set_str(
        kOfxParamPropHint,
        c"A color of the picture that should be gray: pick it with the eyedropper.",
    );

    let number = |name: &CStr, label: &CStr, [min, max, default]: [f64; 3], digits, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, default);
        p.set_dbl(kOfxParamPropMin, min);
        p.set_dbl(kOfxParamPropMax, max);
        p.set_dbl(kOfxParamPropDisplayMin, min);
        p.set_dbl(kOfxParamPropDisplayMax, max);
        p.set_dbl(kOfxParamPropIncrement, if digits == 0 { 1.0 } else { 0.01 });
        p.set_int(kOfxParamPropDigits, digits);
        p.set_str(kOfxParamPropHint, hint);
    };
    number(
        TEMPERATURE,
        c"Temperature",
        [TEMPERATURES[0], TEMPERATURES[1], t0],
        0,
        c"The light's color temperature, in kelvin.",
    );
    number(
        TINT,
        c"Tint",
        [TINTS[0], TINTS[1], tint0],
        0,
        c"The light's tint, toward green or magenta.",
    );
    number(
        AMOUNT,
        c"Amount of correction",
        [0.0, 1.0, 0.0],
        3,
        c"How far the colors move against the light: set by itself to take the light's color exactly to gray; more goes further.",
    );
    number(
        BRIGHTNESS,
        c"Brightness",
        [0.1, 2.0, 1.0],
        3,
        c"The picture's brightness (its luma), its colors kept.",
    );

    let p = fx.define_param(kOfxParamTypeBoolean, EQUALIZE, c"Equalize Histogram");
    p.set_int(kOfxParamPropDefault, 0);
    p.set_str(
        kOfxParamPropHint,
        c"Stretches each channel over the frame's whole range, then holds red and green under blue: warm colors go gray.",
    );
}

// Presets.

/// The factory presets: VEGAS's has none; BananaV's lights to take away
/// (a cool one's taken away warms the picture), at the amount that takes
/// the light's color to gray unless one is given.
pub(crate) fn presets() -> Vec<Preset> {
    let preset = |name: &'static str, [t, tint]: [f64; 2], amount: Option<f64>, brightness: f64| {
        let n = neutral_of(t, tint);
        let amount = amount.unwrap_or_else(|| amount_for(n));
        Preset {
            name,
            values: vec![
                ("neutral", PresetValue::Num(n.to_vec())),
                ("temperature", PresetValue::Num(vec![t])),
                ("tint", PresetValue::Num(vec![tint])),
                ("amount", PresetValue::Num(vec![amount])),
                ("brightness", PresetValue::Num(vec![brightness])),
            ],
        }
    };
    vec![
        preset("Tungsten Light Corrected", [3200.0, 0.0], None, 1.0),
        preset("Fluorescent Light Corrected", [4000.0, 20.0], None, 1.0),
        preset("Shade Corrected", [7500.0, 10.0], None, 1.0),
        preset("Warm Light", [9000.0, 10.0], None, 1.0),
        preset("Golden Hour", [12000.0, 30.0], None, 1.05),
        preset("Cool Light", [4500.0, 10.0], None, 1.0),
        preset("Moonlight", [3000.0, 0.0], Some(0.8), 0.7),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rec. 709 (sRGB) RGB to XYZ, and back: the picker's Lab numbers.
    const REC709: M3 = [
        [0.412_456_4, 0.357_576_1, 0.180_437_5],
        [0.212_672_9, 0.715_152_2, 0.072_175],
        [0.019_333_9, 0.119_192, 0.950_304_1],
    ];
    const REC709_INV: M3 = [
        [3.240_454_2, -1.537_138_5, -0.498_531_4],
        [-0.969_266, 1.876_010_8, 0.041_556],
        [0.055_643_4, -0.204_025_9, 1.057_225_2],
    ];

    /// A color's Lab numbers (sRGB, D65), as VEGAS's picker shows them.
    fn lab(rgb: [f64; 3]) -> [f64; 3] {
        let lin = rgb.map(|v| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        });
        let xyz = mul(&REC709, lin);
        let white = [0.950_47, 1.0, 1.088_83];
        let f = |t: f64| {
            if t > 216.0 / 24389.0 {
                t.cbrt()
            } else {
                (24389.0 / 27.0 * t + 16.0) / 116.0
            }
        };
        let [fx, fy, fz] = [0, 1, 2].map(|i| f(xyz[i] / white[i]));
        [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
    }

    /// The sRGB color of Lab numbers (D65).
    fn from_lab([l, a, b]: [f64; 3]) -> [f64; 3] {
        let fy = (l + 16.0) / 116.0;
        let f = [fy + a / 500.0, fy, fy - b / 200.0];
        let white = [0.950_47, 1.0, 1.088_83];
        let xyz: [f64; 3] = std::array::from_fn(|i| {
            let t = f[i].powi(3);
            white[i]
                * if t > 216.0 / 24389.0 {
                    t
                } else {
                    (116.0 * f[i] - 16.0) / (24389.0 / 27.0)
                }
        });
        mul(&REC709_INV, xyz).map(|v| {
            if v <= 0.003_130_8 {
                12.92 * v
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            }
        })
    }

    #[test]
    fn the_numbers_are_vegass() {
        // (Default): mid gray is 5999 K and 10.
        let [t, tint] = temperature_of([0.5; 3]).unwrap();
        assert_eq!((t.round(), tint.round()), (5999.0, 10.0));
        // VEGAS's pictures: these neutral colors (Lab) show these numbers.
        for (l, expected) in [
            ([53.53, 2.27, -2.42], [6000.0, 0.0]),
            ([59.06, 0.14, -30.29], [15000.0, 100.0]),
            ([51.99, 46.66, 29.46], [1700.0, -100.0]),
        ] {
            let [t, tint] = temperature_of(from_lab(l)).unwrap();
            assert!(
                (t - expected[0]).abs() < 15.0 && (tint - expected[1]).abs() < 1.0,
                "{l:?}: {t} {tint}"
            );
        }
        // And back: a light's neutral color (VEGAS's at the sliders' ends).
        let n = lab(neutral_of(15000.0, 100.0));
        assert!(
            (n[0] - 59.06).abs() < 0.3 && (n[1] - 0.14).abs() < 0.3 && (n[2] + 30.29).abs() < 0.3,
            "{n:?}"
        );
        let n = lab(neutral_of(1700.0, -100.0));
        assert!(
            (n[0] - 51.99).abs() < 0.3 && (n[1] - 46.66).abs() < 0.5 && (n[2] - 29.46).abs() < 0.5,
            "{n:?}"
        );
    }

    #[test]
    fn temperature_and_tint_go_both_ways() {
        for (t, tint) in [
            (3200.0, 0.0),
            (6500.0, 10.0),
            (9000.0, -40.0),
            (1700.0, 100.0),
        ] {
            let [t2, tint2] = temperature_tint(chromaticity(t, tint));
            assert!(
                (t2 - t).abs() < 0.5 && (tint2 - tint).abs() < 0.01,
                "{t} {tint}: {t2} {tint2}"
            );
        }
    }

    #[test]
    fn the_amount_set_by_itself_is_vegass() {
        // VEGAS's pictures: a temperature and tint typed set these amounts.
        for ([t, tint], expected) in [
            ([3000.0, 10.0], 0.578),
            ([6000.0, -50.0], 0.337),
            ([6000.0, 0.0], 0.135),
        ] {
            let a = amount_for(neutral_of(t, tint));
            assert!((a - expected).abs() < 0.0006, "{t} {tint}: {a}");
        }
        // And a color picked (a yellow green, Lab as VEGAS showed it): its
        // own, the tint past the slider's end.
        let picked = from_lab([91.37, -56.87, 87.59]);
        assert!((amount_for(picked) - 0.896).abs() < 0.0015);
        assert_eq!(temperature_of(picked).unwrap()[1], 100.0);
        // Mid gray: none.
        assert!(amount_for([0.5; 3]) < 1e-6);
    }

    #[test]
    fn the_colors_move_as_vegass() {
        // VEGAS's pictures: each light at each amount moved every color by
        // these steps (of 255), whatever the color.
        for ([t, tint, amount], steps) in [
            ([3000.0, 0.0, 1.0], [-93.0, 8.0, 200.0]),
            ([3000.0, 10.0, 0.578], [-29.0, 1.0, 69.0]),
            ([6000.0, -50.0, 0.337], [-10.0, 10.0, -23.0]),
            ([4676.0, 100.0, 0.896], [13.5, -47.0, 206.5]),
        ] {
            let s = Settings {
                temperature: t,
                tint,
                amount,
                brightness: 1.0,
                equalize: false,
            };
            let (_, o) = s.affine();
            for i in 0..3 {
                assert!(
                    (f64::from(o[i]) * 255.0 - steps[i]).abs() < 1.0,
                    "{t} {tint} {amount}: {o:?}"
                );
            }
        }
    }

    #[test]
    fn amount_0_changes_nothing_and_the_color_picked_becomes_gray() {
        let s = |[temperature, tint]: [f64; 2], amount| Settings {
            temperature,
            tint,
            amount,
            brightness: 1.0,
            equalize: false,
        };
        assert!(s(gray_light(), 0.0).changes_nothing());
        assert!(s([1700.0, -100.0], 0.0).changes_nothing());
        // A warm color picked, at the amount picking it sets: gray, its
        // luma kept.
        let warm = [0.6, 0.45, 0.3];
        let light = temperature_of(warm).unwrap();
        let (m, o) = s(light, amount_for(warm)).affine();
        let g = apply(&m, &o, warm.map(|v| v as f32));
        assert!(
            (g[0] - g[1]).abs() < 1e-4 && (g[1] - g[2]).abs() < 1e-4,
            "{g:?}"
        );
        let luma = 0.299 * 0.6 + 0.587 * 0.45 + 0.114 * 0.3;
        assert!((f64::from(g[0]) - luma).abs() < 1e-4);
        // More amount goes further: blue.
        let (m, o) = s(light, 1.0).affine();
        let b = apply(&m, &o, warm.map(|v| v as f32));
        assert!(b[2] > b[0] + 0.1, "{b:?}");
    }

    #[test]
    fn brightness_scales_the_luma_alone() {
        let bright = |brightness| {
            let [temperature, tint] = gray_light();
            Settings {
                temperature,
                tint,
                amount: 0.0,
                brightness,
                equalize: false,
            }
            .affine()
        };
        // VEGAS's picture at 0.1: the sand (in its preview, 247 213 102)
        // went to 60 22 0, the sky (16 226 255) to 0 74 107.
        let (m, o) = bright(0.1);
        for (from, to) in [
            ([247.0, 213.0, 102.0], [60.0, 22.0, 0.0]),
            ([16.0, 226.0, 255.0], [0.0, 74.0, 107.0]),
        ] {
            let out = apply(&m, &o, from.map(|v: f32| v / 255.0));
            for i in 0..3 {
                assert!((out[i] * 255.0 - to[i]).abs() < 3.0, "{from:?}: {out:?}");
            }
        }
        // Gray doubles at 2.
        let (m, o) = bright(2.0);
        assert!(
            apply(&m, &o, [0.25; 3])
                .iter()
                .all(|v| (v - 0.5).abs() < 1e-6)
        );
    }

    #[test]
    fn equalize_is_vegass() {
        let settings = |[temperature, tint]: [f64; 2], amount| Settings {
            temperature,
            tint,
            amount,
            brightness: 1.0,
            equalize: true,
        };
        let level = |s: &Settings, frame: &[[f32; 3]]| {
            let (m, o) = s.affine();
            let mut l = Levels::none();
            for c in frame {
                l.add(balanced(&m, &o, c.map(|v| v / 255.0)));
            }
            (m, o, l)
        };
        // A frame from black to white: no stretch. VEGAS's picture
        // (default, Equalize alone): the sand went gray at its blue, the
        // sky stayed.
        let frame = [[0.0, 0.0, 0.0], [255.0, 255.0, 255.0]];
        let s = settings(gray_light(), 0.0);
        assert!(!s.changes_nothing());
        let (m, o, l) = level(&s, &frame);
        for (from, to) in [
            ([254.0, 217.0, 104.0], [104.0, 104.0, 104.0]),
            ([14.0, 224.0, 255.0], [14.0, 224.0, 255.0]),
            ([230.0, 80.0, 120.0], [120.0, 80.0, 120.0]),
        ] {
            let out = l.equalize(balanced(&m, &o, from.map(|v: f32| v / 255.0)));
            for i in 0..3 {
                assert!((out[i] * 255.0 - to[i]).abs() < 0.5, "{from:?}: {out:?}");
            }
        }
        // 3000 K at its own amount (VEGAS's picture): the blue's shift
        // lifted the darkest blue to 69, stretched back down; the sand
        // (253 209 101) went gray at 141.
        let s = settings([3000.0, 10.0], 0.578);
        let (m, o, l) = level(&s, &frame);
        assert!((l.lo[2] * 255.0 - 69.0).abs() < 1.0, "{l:?}");
        let out = l.equalize(balanced(
            &m,
            &o,
            [253.0, 209.0, 101.0].map(|v: f32| v / 255.0),
        ));
        assert!(
            out.iter().all(|v| (v * 255.0 - 141.0).abs() < 4.0),
            "{out:?}"
        );
        // A channel all one value is left as it is.
        let flat = Levels {
            lo: [0.5; 3],
            hi: [0.5, 1.0, 1.0],
        };
        assert_eq!(flat.equalize([0.5, 0.75, 1.0]), [0.5, 0.5, 1.0]);
    }

    #[test]
    fn the_presets_are_lights_taken_away() {
        let p = presets();
        assert_eq!(p.len(), 7);
        for preset in &p {
            // Each one's neutral color shows its temperature and tint.
            let num = |n: &str| match preset.values.iter().find(|(k, _)| *k == n) {
                Some((_, PresetValue::Num(v))) => v.clone(),
                other => panic!("{n}: {other:?}"),
            };
            let n = num("neutral");
            let [t, tint] = temperature_of([n[0], n[1], n[2]]).unwrap();
            assert!((t - num("temperature")[0]).abs() < 1.0, "{}", preset.name);
            assert!((tint - num("tint")[0]).abs() < 0.1, "{}", preset.name);
        }
    }
}
