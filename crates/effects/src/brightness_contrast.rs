//! Brightness and Contrast: VEGAS's. Brightness lightens or darkens every
//! channel, Contrast spreads them from (or draws them toward) Contrast
//! center; Automatic Contrast's Calculate Brightness & Contrast sets the
//! three from the picture.
//!
//! VEGAS's math, measured on the calibration card through each of its
//! presets (8-bit, the card's every color and ramp): each channel — of the
//! values the host hands over (in BananaV's 8-bit projects, as VEGAS's,
//! the encoded ones), the same curve for the three — first moved by
//! Brightness, then spread from Contrast center by 1 + Contrast below 0
//! and 1 ⁄ (1 − Contrast) above it: `center + (v + brightness − center)
//! × k`. VEGAS's 8-bit results round their halves down (Brighter's +25.5
//! is +25, Darker's −25.5 is −26): BananaV's come a 32nd of a step under
//! the exact value, which the conversion to 8 bits rounds the same way —
//! every preset's 2048 values then match VEGAS's but 8, a step apart.
//! Negatives are clipped, what passes white is kept (8-bit projects hold
//! each effect's result within 0–1).
//!
//! Automatic Contrast (VEGAS 23's), as VEGAS's Calculate set it on the
//! calibration card and on a photograph: Contrast center at Minimum
//! Brightness, Brightness Minimum less the picture's dark end (both
//! pictures' was black: Brightness was Minimum), Contrast the spread that
//! takes the picture's range to Minimum–Maximum — the dark end lands on
//! Minimum, the bright one on Maximum. The ends: the picture's light
//! (Rec. 709's weights), every pixel counted, with Histogram Cut-Off of
//! them left out at each end (VEGAS's bright end comes a little higher:
//! Contrast −0.127 on the card where this gives −0.120, −0.052 on the
//! photograph for −0.048 — how it cuts awaits prints). Minimum and Maximum
//! Brightness are 15 ⁄ 255 and 235 ⁄ 255 (0.059 and 0.922 shown: the
//! lines VEGAS drew start 15.0 levels up). Over the frame at the cursor
//! (Current Frame), the whole event (Current Clip: the host measures it,
//! BananaV's `kBananaVParamPropAnalysis` "Histogram") or each of its
//! frames ("Histograms": Set for each Frame independently keys Brightness,
//! Contrast and Contrast center at every frame, as VEGAS's).
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.BrightnessContrast";

/// The parameters.
const BRIGHTNESS: &CStr = c"brightness";
const CONTRAST: &CStr = c"contrast";
const CENTER: &CStr = c"contrastCenter";
const AUTOMATIC: &CStr = c"automaticContrast";
const INPUT: &CStr = c"automaticInput";
const OUTPUT: &CStr = c"automaticOutput";
const CUT_OFF: &CStr = c"histogramCutOff";
const MINIMUM: &CStr = c"minimumBrightness";
const MAXIMUM: &CStr = c"maximumBrightness";
const MODE: &CStr = c"analysisMode";
const CALCULATE: &CStr = c"calculate";
/// Where the host keeps the clip's light it measured (hidden).
const CLIP_LIGHT: &CStr = c"clipLight";

/// What Calculate measures, in the order of Analysis Mode's options.
const MODES: [&CStr; 3] = [
    c"Current Frame",
    c"Current Clip",
    c"Set for each Frame independently",
];

/// Analysis Mode's options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Frame,
    Clip,
    EachFrame,
}

impl Mode {
    fn of(index: c_int) -> Mode {
        match index {
            0 => Mode::Frame,
            2 => Mode::EachFrame,
            _ => Mode::Clip,
        }
    }

    /// What Calculate asks the host to measure (BananaV's
    /// `kBananaVParamPropAnalysis`): nothing for the frame at the cursor,
    /// which the plug-in measures itself.
    fn analysis(self) -> &'static CStr {
        match self {
            Mode::Frame => c"",
            Mode::Clip => c"Histogram",
            Mode::EachFrame => c"Histograms",
        }
    }
}

/// Minimum and Maximum Brightness at first: 15 and 235 of 255.
const MINIMUM_AT_FIRST: f64 = 15.0 / 255.0;
const MAXIMUM_AT_FIRST: f64 = 235.0 / 255.0;

/// How far under the exact value BananaV's results come: a 32nd of an
/// 8-bit step, so their halves round down as VEGAS's do.
const TIE: f32 = 1.0 / (32.0 * 255.0);

/// The steepest contrast: Contrast 1 spreads by this much.
const STEEPEST: f64 = 1e4;

/// How much Contrast `contrast` spreads the channels.
pub(crate) fn spread(contrast: f64) -> f64 {
    let c = contrast.clamp(-1.0, 1.0);
    if c <= 0.0 {
        1.0 + c
    } else {
        (1.0 / (1.0 - c)).min(STEEPEST)
    }
}

/// The Contrast that spreads the channels by `k`.
fn contrast_of(k: f64) -> f64 {
    if k >= 1.0 {
        1.0 - 1.0 / k.min(STEEPEST)
    } else {
        k.max(0.0) - 1.0
    }
}

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub brightness: f64,
    pub contrast: f64,
    pub center: f64,
    pub cut_off: f64,
    pub minimum: f64,
    pub maximum: f64,
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        Settings {
            brightness: v.f64(BRIGHTNESS, 0.0),
            contrast: v.f64(CONTRAST, 0.0),
            center: v.f64(CENTER, 0.5),
            cut_off: v.f64(CUT_OFF, 0.01),
            minimum: v.f64(MINIMUM, MINIMUM_AT_FIRST),
            maximum: v.f64(MAXIMUM, MAXIMUM_AT_FIRST),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        let f = |name: &CStr| fx.param(name).f64_at(time);
        Settings {
            brightness: f(BRIGHTNESS),
            contrast: f(CONTRAST),
            center: f(CENTER),
            cut_off: f(CUT_OFF),
            minimum: f(MINIMUM),
            maximum: f(MAXIMUM),
        }
    }

    /// Whether it changes nothing.
    pub(crate) fn changes_nothing(&self) -> bool {
        self.brightness == 0.0 && self.contrast == 0.0
    }

    /// The straight line each channel goes through, its slope and its
    /// offset (VEGAS's halves rounding down included).
    pub(crate) fn line(&self) -> (f32, f32) {
        let k = spread(self.contrast);
        let c = self.center;
        let offset = c * (1.0 - k) + self.brightness.clamp(-1.0, 1.0) * k;
        (k as f32, offset as f32 - TIE)
    }

    /// The settings with Brightness, Contrast and Contrast center as
    /// Automatic Contrast sets them for a picture of light `light`.
    pub(crate) fn automatic(&self, light: &Light) -> Settings {
        let (brightness, contrast, center) =
            automatic(light.range(self.cut_off), (self.minimum, self.maximum));
        Settings {
            brightness,
            contrast,
            center,
            ..*self
        }
    }
}

/// A channel through the line `(k, offset)`, negatives clipped.
pub(crate) fn apply((k, offset): (f32, f32), v: f32) -> f32 {
    (v * k + offset).max(0.0)
}

/// Brightness, Contrast and Contrast center that take the light's range
/// `lo`–`hi` to `minimum`–`maximum`, as VEGAS's: the center at `minimum`,
/// the brightness that brings `lo` there, the spread the ranges' widths
/// ask for.
pub(crate) fn automatic((lo, hi): (f64, f64), (minimum, maximum): (f64, f64)) -> (f64, f64, f64) {
    let k = ((maximum - minimum).max(0.0) / (hi - lo).max(1.0 / 255.0)).min(STEEPEST);
    let center = minimum.clamp(0.0, 1.0);
    ((center - lo).clamp(-1.0, 1.0), contrast_of(k), center)
}

/// A picture's light (Rec. 709's weights of its straight colors) counted
/// in [`BINS`] bins over 0–1.
#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    pub counts: Vec<u64>,
}

/// How many bins a picture's light is counted in.
pub const BINS: usize = 1024;

impl Light {
    pub fn new() -> Light {
        Light {
            counts: vec![0; BINS],
        }
    }

    /// The light of a straight color.
    pub fn of(rgb: [f32; 3]) -> f32 {
        0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
    }

    /// Counts a straight color.
    pub fn add(&mut self, rgb: [f32; 3]) {
        let bin = (Light::of(rgb).clamp(0.0, 1.0) * (BINS - 1) as f32).round() as usize;
        self.counts[bin.min(BINS - 1)] += 1;
    }

    /// The light from which `part` of the pixels are darker, and the one
    /// from which `part` are brighter (0–1).
    fn range(&self, part: f64) -> (f64, f64) {
        let total: u64 = self.counts.iter().sum();
        if total == 0 {
            return (0.0, 1.0);
        }
        let part = part.clamp(0.0, 0.5);
        let at = |goal: f64| {
            let mut seen = 0u64;
            for (i, &n) in self.counts.iter().enumerate() {
                seen += n;
                if seen as f64 > goal {
                    return i as f64 / (BINS - 1) as f64;
                }
            }
            1.0
        };
        let lo = at(part * total as f64);
        let hi = at((1.0 - part) * total as f64 - 1.0).max(lo);
        (lo, hi)
    }

    /// As the host writes it into the analysis's parameter: a first line
    /// `bvlight 1 <bins>`, then the counts.
    pub fn text(&self) -> String {
        let counts: Vec<String> = self.counts.iter().map(u64::to_string).collect();
        format!("bvlight 1 {}\n{}\n", self.counts.len(), counts.join(" "))
    }

    /// The light [`Light::text`] wrote.
    pub fn parse(text: &str) -> Option<Light> {
        let mut lines = text.lines();
        let head: Vec<&str> = lines.next()?.split_whitespace().collect();
        if head.first() != Some(&"bvlight") || head.get(1) != Some(&"1") {
            return None;
        }
        let bins: usize = head.get(2)?.parse().ok()?;
        let counts: Vec<u64> = lines
            .flat_map(str::split_whitespace)
            .filter_map(|n| n.parse().ok())
            .collect();
        (bins > 1 && counts.len() == bins).then_some(Light { counts })
    }

    /// Frames' lights as the host writes them for "Histograms": a first
    /// line `bvlights 1 <bins>`, then a line a frame — its time (the
    /// effect's, in frames), then its counts.
    pub fn frames_text(frames: &[(f64, Light)]) -> String {
        let bins = frames.first().map_or(BINS, |(_, l)| l.counts.len());
        let mut text = format!("bvlights 1 {bins}\n");
        for (time, light) in frames {
            text.push_str(&time.to_string());
            for n in &light.counts {
                text.push(' ');
                text.push_str(&n.to_string());
            }
            text.push('\n');
        }
        text
    }

    /// The frames' lights [`Light::frames_text`] wrote.
    pub fn parse_frames(text: &str) -> Option<Vec<(f64, Light)>> {
        let mut lines = text.lines();
        let head: Vec<&str> = lines.next()?.split_whitespace().collect();
        if head.first() != Some(&"bvlights") || head.get(1) != Some(&"1") {
            return None;
        }
        let bins: usize = head.get(2)?.parse().ok()?;
        lines
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let mut words = l.split_whitespace();
                let time: f64 = words.next()?.parse().ok()?;
                let counts: Vec<u64> = words.map(|n| n.parse().ok()).collect::<Option<_>>()?;
                (bins > 1 && counts.len() == bins).then_some((time, Light { counts }))
            })
            .collect()
    }
}

impl Default for Light {
    fn default() -> Light {
        Light::new()
    }
}

/// The light of the source's picture at `time`, every pixel counted (a
/// pixel in three across and down puts the card's bright end several
/// levels lower).
fn picture_light(fx: Fx, time: f64) -> Option<Light> {
    let img = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time)?;
    if img.channels < 3 {
        return None;
    }
    let premultiplied = img.channels == 4 && !img.unpremultiplied;
    let [x1, y1, x2, y2] = img.bounds;
    let mut light = Light::new();
    for y in y1..y2 {
        for x in x1..x2 {
            if let Some(c) = crate::colors::pixel(&img, x, y)
                .and_then(|p| crate::colors::straight(p, premultiplied))
            {
                light.add(c);
            }
        }
    }
    Some(light)
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
    } else if is(action, kOfxActionInstanceChanged) {
        let time = args.dbl(kOfxPropTime, 0);
        if args.is(kOfxPropName, CALCULATE) {
            calculate(fx, time);
        } else if args.is(kOfxPropName, CLIP_LIGHT) {
            from_clip(fx, time);
        }
        show(fx, time);
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) || is(action, kOfxActionBeginInstanceEdit) {
        show(fx, 0.0);
        kOfxStatOK
    } else if is(action, kOfxActionLoad)
        || is(action, kOfxActionUnload)
        || is(action, kOfxActionDestroyInstance)
    {
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

/// Renders: each channel through the line.
fn render(fx: Fx, args: Props) -> OfxStatus {
    let line = Settings::at(fx, args.dbl(kOfxPropTime, 0)).line();
    crate::colors::render(fx, args, &|c| c.map(|v| apply(line, v)))
}

/// Analysis Mode's say: what Calculate asks the host to measure.
fn show(fx: Fx, time: f64) {
    let mode = Mode::of(fx.param(MODE).int_at(time));
    fx.param(CALCULATE).props().set_str(
        bv_ofx_sys::extensions::kBananaVParamPropAnalysis,
        mode.analysis(),
    );
}

/// What Calculate sets: Brightness, Contrast and Contrast center.
const SET: [&CStr; 3] = [BRIGHTNESS, CONTRAST, CENTER];

/// Sets Brightness, Contrast and Contrast center at `time` (keys there
/// when they are animated) from Automatic Contrast's settings and `light`.
fn set_from(fx: Fx, time: f64, light: &Light) {
    let s = Settings::at(fx, time).automatic(light);
    for (name, v) in SET.into_iter().zip([s.brightness, s.contrast, s.center]) {
        fx.param(name).set_f64(v);
    }
}

/// Calculate Brightness & Contrast: for Current Frame the picture at the
/// cursor measured (the others the host measures, and the light it found
/// arrives in [`CLIP_LIGHT`]).
fn calculate(fx: Fx, time: f64) {
    if Mode::of(fx.param(MODE).int_at(time)) != Mode::Frame {
        return;
    }
    match picture_light(fx, time) {
        Some(light) => set_from(fx, time, &light),
        None => {
            if crate::ofx::tracing() {
                eprintln!("bananav brightness and contrast: no picture to measure at {time}");
            }
        }
    }
}

/// The light the host measured: the clip's, Brightness, Contrast and
/// Contrast center set from it; each frame's, the three keyed at every
/// frame (their keys before gone), as VEGAS's Set for each Frame
/// independently does. What was measured is not kept.
fn from_clip(fx: Fx, time: f64) {
    let text = fx.param(CLIP_LIGHT).text_at(time);
    if let Some(light) = Light::parse(&text) {
        set_from(fx, time, &light);
    } else if let Some(frames) = Light::parse_frames(&text) {
        let s = Settings::at(fx, time);
        for name in SET {
            fx.param(name).delete_keys();
        }
        for (at, light) in &frames {
            let a = s.automatic(light);
            for (name, v) in SET.into_iter().zip([a.brightness, a.contrast, a.center]) {
                fx.param(name).set_f64_at(*at, v);
            }
        }
    } else {
        return;
    }
    fx.param(CLIP_LIGHT).set_text("");
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Brightness and Contrast");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"Lighter or darker, more or less contrast, around a center you choose; or set from the picture's range.",
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

/// A number from `min` to `max`, VEGAS's three decimals, under `parent`
/// (none: at the top). Only those at the top animate, as VEGAS's do:
/// Automatic Contrast's settings have no keyframes.
fn number(
    fx: Fx,
    name: &CStr,
    label: &CStr,
    (default, min, max): (f64, f64, f64),
    parent: Option<&CStr>,
    hint: &CStr,
) {
    let p = fx.define_param(kOfxParamTypeDouble, name, label);
    p.set_dbl(kOfxParamPropDefault, default);
    p.set_dbl(kOfxParamPropMin, min);
    p.set_dbl(kOfxParamPropMax, max);
    p.set_dbl(kOfxParamPropDisplayMin, min);
    p.set_dbl(kOfxParamPropDisplayMax, max);
    p.set_dbl(kOfxParamPropIncrement, 0.001);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_int(kOfxParamPropAnimates, parent.is_none() as c_int);
    p.set_str(kOfxParamPropHint, hint);
    if let Some(parent) = parent {
        p.set_str(kOfxParamPropParent, parent);
    }
}

/// A group, open or closed at first, under `parent`.
fn group(fx: Fx, name: &CStr, label: &CStr, open: bool, parent: Option<&CStr>) {
    let p = fx.define_param(kOfxParamTypeGroup, name, label);
    p.set_int(kOfxParamPropGroupOpen, open as c_int);
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

    number(
        fx,
        BRIGHTNESS,
        c"Brightness",
        (0.0, -1.0, 1.0),
        None,
        c"Added to every channel: lighter to the right (1, white), darker to the left (−1, black).",
    );
    number(
        fx,
        CONTRAST,
        c"Contrast",
        (0.0, -1.0, 1.0),
        None,
        c"The channels spread from Contrast center (to the right) or drawn toward it (to the left).",
    );
    number(
        fx,
        CENTER,
        c"Contrast center",
        (0.5, 0.0, 1.0),
        None,
        c"The value contrast spreads the others from: 0 black, 0.5 the middle, 1 white.",
    );
    group(fx, AUTOMATIC, c"Automatic Contrast", false, None);
    group(fx, INPUT, c"Input", true, Some(AUTOMATIC));
    number(
        fx,
        CUT_OFF,
        c"Histogram Cut-Off",
        (0.01, 0.0, 0.5),
        Some(INPUT),
        c"The part of the darkest and of the brightest pixels Calculate leaves out of the picture's range.",
    );
    group(fx, OUTPUT, c"Output", true, Some(AUTOMATIC));
    number(
        fx,
        MINIMUM,
        c"Minimum Brightness",
        (MINIMUM_AT_FIRST, 0.0, 1.0),
        Some(OUTPUT),
        c"Where Calculate puts the picture's darkest part.",
    );
    number(
        fx,
        MAXIMUM,
        c"Maximum Brightness",
        (MAXIMUM_AT_FIRST, 0.0, 1.0),
        Some(OUTPUT),
        c"Where Calculate puts the picture's brightest part.",
    );
    let p = fx.define_param(kOfxParamTypeChoice, MODE, c"Analysis Mode");
    p.set_strs(kOfxParamPropChoiceOption, &MODES);
    p.set_int(kOfxParamPropDefault, 1);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(kOfxParamPropParent, AUTOMATIC);
    p.set_str(
        kOfxParamPropHint,
        c"What Calculate measures: the frame at the cursor, the whole event, or each of its frames (Brightness, Contrast and Contrast center keyed at every frame).",
    );
    let p = fx.define_param(
        kOfxParamTypePushButton,
        CALCULATE,
        c"Calculate Brightness & Contrast",
    );
    p.set_str(kOfxParamPropParent, AUTOMATIC);
    p.set_str(
        kOfxParamPropHint,
        c"Sets Brightness, Contrast and Contrast center so that the picture's range fills Minimum to Maximum Brightness.",
    );
    p.set_str(
        bv_ofx_sys::extensions::kBananaVParamPropAnalysis,
        Mode::Clip.analysis(),
    );
    p.set_str(
        bv_ofx_sys::extensions::kBananaVParamPropAnalysisParam,
        CLIP_LIGHT,
    );
    let p = fx.define_param(kOfxParamTypeString, CLIP_LIGHT, c"Clip Light");
    p.set_str(kOfxParamPropDefault, c"");
    p.set_int(kOfxParamPropSecret, 1);
    p.set_int(kOfxParamPropAnimates, 0);
}

// Presets.

/// The factory presets: VEGAS's, in its list's order.
pub(crate) fn presets() -> Vec<Preset> {
    let p = |name: &'static str, brightness: f64, contrast: f64| Preset {
        name,
        values: vec![
            ("brightness", PresetValue::Num(vec![brightness])),
            ("contrast", PresetValue::Num(vec![contrast])),
        ],
    };
    vec![
        p("Brighter", 0.1, 0.0),
        p("Very Bright", 0.25, 0.0),
        p("Darker", -0.1, 0.0),
        p("Very Dark", -0.25, 0.0),
        p("Brighter, More Contrast", 0.15, 0.15),
        p("Brighter, Less Contrast", 0.15, -0.15),
        p("Darker, More Contrast", -0.15, 0.15),
        p("Darker, Less Contrast", -0.15, -0.15),
        p("Flat", 0.0, -0.5),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(brightness: f64, contrast: f64, center: f64) -> Settings {
        Settings {
            brightness,
            contrast,
            center,
            cut_off: 0.01,
            minimum: MINIMUM_AT_FIRST,
            maximum: MAXIMUM_AT_FIRST,
        }
    }

    /// The 8-bit result, as a host's conversion makes it.
    fn byte(s: &Settings, x: u8) -> u8 {
        (apply(s.line(), x as f32 / 255.0).clamp(0.0, 1.0) * 255.0).round() as u8
    }

    /// VEGAS's presets on the calibration card's gray ramp (its 8-bit
    /// snapshots), every value but the few a step apart.
    #[test]
    fn the_presets_draw_what_vegas_draws() {
        // Brighter: +25.5 rounds down; Darker: −25.5 too.
        assert_eq!(byte(&s(0.1, 0.0, 0.5), 0), 25);
        assert_eq!(byte(&s(0.1, 0.0, 0.5), 100), 125);
        assert_eq!(byte(&s(-0.1, 0.0, 0.5), 100), 74);
        assert_eq!(byte(&s(-0.1, 0.0, 0.5), 26), 0);
        assert_eq!(byte(&s(-0.1, 0.0, 0.5), 27), 1);
        // Very Bright and Very Dark: ±63.75.
        assert_eq!(byte(&s(0.25, 0.0, 0.5), 0), 64);
        assert_eq!(byte(&s(-0.25, 0.0, 0.5), 85), 21);
        // Brighter, More Contrast (×1/0.85 about the middle, after +0.15).
        let more = s(0.15, 0.15, 0.5);
        for (x, y) in [(0, 22), (64, 98), (128, 173), (192, 248), (198, 255)] {
            assert_eq!(byte(&more, x), y, "{x}");
        }
        // Darker, Less Contrast (×0.85).
        let less = s(-0.15, -0.15, 0.5);
        for (x, y) in [(16, 0), (17, 1), (64, 41), (128, 95), (255, 203)] {
            assert_eq!(byte(&less, x), y, "{x}");
        }
        // Flat (half the spread about the middle): 63.75 + x ⁄ 2.
        let flat = s(0.0, -0.5, 0.5);
        for (x, y) in [(0, 64), (1, 64), (128, 128), (255, 191)] {
            assert_eq!(byte(&flat, x), y, "{x}");
        }
        // Another center (one of the user's: −0.252, 0.116 about 0.293).
        let other = s(-0.2517, 0.1158, 0.2925);
        for (x, y) in [(73, 0), (74, 1), (128, 62), (255, 206)] {
            assert_eq!(byte(&other, x), y, "{x}");
        }
    }

    #[test]
    fn nothing_changes_at_the_defaults_and_negatives_are_clipped() {
        assert!(s(0.0, 0.0, 0.5).changes_nothing());
        assert!(!s(0.1, 0.0, 0.5).changes_nothing());
        // Full contrast about the top: everything below it black.
        let (k, offset) = s(-1.0, 1.0, 1.0).line();
        assert_eq!(apply((k, offset), 1.0), 0.0);
        assert!(k >= 1000.0);
    }

    /// Automatic Contrast spreads the light's range over Minimum to
    /// Maximum Brightness, as VEGAS's: the center at Minimum, Brightness
    /// Minimum less the dark end.
    #[test]
    fn automatic_contrast_fills_the_range_asked_for() {
        let mut light = Light::new();
        // A picture from 0.2 to 0.6, evenly.
        for i in 0..=400 {
            let v = 0.2 + 0.4 * i as f32 / 400.0;
            light.add([v, v, v]);
        }
        let auto = s(0.0, 0.0, 0.5).automatic(&light);
        let line = auto.line();
        let (lo, hi) = light.range(0.01);
        assert_eq!(auto.center, MINIMUM_AT_FIRST);
        assert!((auto.brightness - (MINIMUM_AT_FIRST - lo)).abs() < 1e-12);
        assert!(
            (apply(line, lo as f32) - 15.0 / 255.0).abs() < 0.001,
            "{auto:?}"
        );
        assert!(
            (apply(line, hi as f32) - 235.0 / 255.0).abs() < 0.001,
            "{auto:?}"
        );
        assert!(auto.contrast > 0.0);
        // Written and read back.
        assert_eq!(Light::parse(&light.text()), Some(light.clone()));
        assert_eq!(Light::parse("bvmotion 1 2\n1 2"), None);
        let frames = vec![(0.0, light.clone()), (1.5, Light::new())];
        assert_eq!(
            Light::parse_frames(&Light::frames_text(&frames)),
            Some(frames)
        );
        assert_eq!(Light::parse_frames(&light.text()), None);
        assert_eq!(Light::parse(&Light::frames_text(&[])), None);
    }

    /// VEGAS's Calculate on a picture whose dark end is black (the card,
    /// a photograph): Brightness and Contrast center both Minimum, the
    /// black at Minimum and the bright end at Maximum.
    #[test]
    fn a_black_dark_end_leaves_brightness_at_minimum() {
        let mut light = Light::new();
        for i in 0..=1000 {
            let v = i as f32 / 1000.0 * 0.9;
            light.add([v, v, v]);
        }
        for _ in 0..50 {
            light.add([0.0; 3]);
        }
        let auto = s(0.0, 0.0, 0.5).automatic(&light);
        assert_eq!(auto.brightness, MINIMUM_AT_FIRST);
        assert_eq!(auto.center, MINIMUM_AT_FIRST);
        assert!(auto.contrast < 0.0, "{auto:?}");
        let line = auto.line();
        assert_eq!((apply(line, 0.0) * 255.0).round(), 15.0);
        let (_, hi) = light.range(0.01);
        assert!((apply(line, hi as f32) * 255.0 - 235.0).abs() < 0.1);
    }
}
