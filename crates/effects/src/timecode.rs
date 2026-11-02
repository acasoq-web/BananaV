//! Timecode: VEGAS's. The frame's time as the project's ruler shows it,
//! written over the picture in white on a black box: in one of the
//! ruler's formats, at a side, a corner or the middle, its size changed.
//!
//! VEGAS's look, measured on its pictures: its letters have Arial Bold's
//! shapes (Arimo Bold's here, the same metrics), an em 1⁄14 of the frame's
//! height; at Right and Bottom the text ends about a tenth of the frame's
//! width from its right side (the title-safe area's edge: 0.102) and its
//! baseline is 0.895 of the frame's height down; the box goes a third of an em beyond
//! the text's sides, 0.91 em above its baseline and 0.157 em below. Width
//! and Height scale the box, and the letters a little more: VEGAS's at
//! 0.75 are 0.71 of their size at 1 (the size to the power of 1.2). Left,
//! Top and Middle are the same rules turned around (VEGAS's pictures show
//! Right and Bottom only). Time & Frames counts the frames into the
//! second (VEGAS's 6.106 s at 29.97 fps is 6 and 3 frames), Absolute
//! Frames shows three decimals, and numbers take the interface's decimal
//! separator, as VEGAS's take the computer's.
//!
//! The time comes from BananaV's host (`kBananaVImageEffectPropTimelineTime`:
//! an event's effect counts its own time from the event's start); other
//! hosts' time is their project's already.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_core::{FrameRate, Tick, timecode};
use bv_ofx_sys::extensions::kBananaVImageEffectPropTimelineTime;
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::{Overlay, Values};
use crate::ofx::{Fx, Props, for_rows, is, suites};
use crate::text::{self, Coverage};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.Timecode";

const FORMAT: &CStr = c"format";
const HORIZONTAL: &CStr = c"horizontal";
const VERTICAL: &CStr = c"vertical";
const WIDTH: &CStr = c"width";
const HEIGHT: &CStr = c"height";

/// The formats, as VEGAS lists them.
const FORMATS: [&CStr; 12] = [
    c"Time",
    c"Seconds",
    c"Time & Frames",
    c"Absolute Frames",
    c"Feet and Frames 16mm (40fpf)",
    c"Feet and Frames 35mm (16fpf)",
    c"SMPTE Film Sync IVTC (23.976 fps, Video)",
    c"SMPTE Film Sync (24 fps, Film)",
    c"SMPTE EBU (25 fps, Video)",
    c"SMPTE Non-Drop (29.97 fps, Video)",
    c"SMPTE Drop (29.97 fps, Video)",
    c"SMPTE 30 (30 fps, Audio)",
];
const SMPTE_DROP: i32 = 10;
const HORIZONTALS: [&CStr; 3] = [c"Left", c"Middle", c"Right"];
const VERTICALS: [&CStr; 3] = [c"Top", c"Middle", c"Bottom"];

/// The em, a part of the frame's height.
pub(crate) const EM: f64 = 1.0 / 14.0;
/// The letters' size against the box's, by Width and Height: to the
/// power of this.
const LETTERS: f64 = 0.2;
/// How far in from the frame's sides the text keeps (a part of the frame):
/// about the title-safe area's tenth (VEGAS's prints: 0.102).
pub(crate) const SAFE: f64 = 0.102;
/// Where the baseline is at Bottom (a part of the frame's height, down).
pub(crate) const BASELINE: f64 = 0.895;
/// The box beyond the text: across (ems), above and below its baseline.
const BOX_SIDE: f64 = 1.0 / 3.0;
const BOX_TOP: f64 = 0.91;
const BOX_BOTTOM: f64 = 0.157;
/// How far above the baseline the font reaches (ems).
const ASCENT: f64 = crate::arimo::ASCENT / crate::arimo::UNITS;

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub format: i32,
    /// 0 left, 1 middle, 2 right.
    pub horizontal: i32,
    /// 0 top, 1 middle, 2 bottom.
    pub vertical: i32,
    pub width: f64,
    pub height: f64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            format: SMPTE_DROP,
            horizontal: 2,
            vertical: 2,
            width: 1.0,
            height: 1.0,
        }
    }
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        let d = Settings::default();
        Settings {
            format: v.int(FORMAT, d.format),
            horizontal: v.int(HORIZONTAL, d.horizontal),
            vertical: v.int(VERTICAL, d.vertical),
            width: v.f64(WIDTH, d.width),
            height: v.f64(HEIGHT, d.height),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            format: fx.param(FORMAT).int_at(time),
            horizontal: fx.param(HORIZONTAL).int_at(time),
            vertical: fx.param(VERTICAL).int_at(time),
            width: fx.param(WIDTH).f64_at(time),
            height: fx.param(HEIGHT).f64_at(time),
        }
    }

    /// What it writes at `seconds` on the ruler, the project's frames
    /// `rate` a second, numbers with `separator` for their decimals.
    pub(crate) fn text(&self, seconds: f64, rate: f64, separator: char) -> String {
        format_time(self.format, seconds, rate, separator)
    }

    /// The text and its box over a frame `w` × `h` pixels, its pixels
    /// `par` times as wide as tall.
    pub(crate) fn overlay(&self, text: &str, w: usize, h: usize, par: f64) -> Option<Overlay> {
        overlay(self, text, w, h, par)
    }
}

/// `seconds` (on the ruler) in format `format`: frames counted at `rate`
/// a second (the project's) for the formats of no rate of their own,
/// decimals after `separator`.
pub(crate) fn format_time(format: i32, seconds: f64, rate: f64, separator: char) -> String {
    let sign = if seconds < 0.0 { "-" } else { "" };
    let s = seconds.abs();
    let rate = if rate > 0.0 { rate } else { 30.0 };
    let decimals = |v: f64| format!("{v:.3}").replace('.', &separator.to_string());
    let hms = |whole: u64| (whole / 3600, (whole / 60) % 60, whole % 60);
    // A frame's start, a hair below a whole number, still counts as it.
    let frames = |v: f64| (v + 1e-6).floor() as u64;
    let t = Tick::from_seconds_f64(seconds);
    match format {
        0 => {
            let ms = (s * 1000.0).round() as u64;
            let (h, m, sec) = hms(ms / 1000);
            format!("{sign}{h:02}:{m:02}:{sec:02}{separator}{:03}", ms % 1000)
        }
        1 => format!("{sign}{}", decimals(s)),
        2 => {
            let whole = frames(s);
            let into = frames((s - whole as f64) * rate);
            let (h, m, sec) = hms(whole);
            format!("{sign}{h:02}:{m:02}:{sec:02}{separator}{into:02}")
        }
        3 => format!("{sign}{}", decimals(s * rate)),
        4 | 5 => {
            let per_foot = if format == 4 { 40 } else { 16 };
            let f = frames(s * rate);
            format!("{sign}{}+{:02}", f / per_foot, f % per_foot)
        }
        6 => timecode::format_frame(FrameRate::FPS_23_976.frame_at(t), FrameRate::FPS_24),
        7 => timecode::format(t, FrameRate::FPS_24),
        8 => timecode::format(t, FrameRate::FPS_25),
        9 => timecode::format_frame(FrameRate::FPS_29_97.frame_at(t), FrameRate::FPS_30),
        11 => timecode::format(t, FrameRate::FPS_30),
        _ => timecode::format(t, FrameRate::FPS_29_97),
    }
}

/// The text and its box over a frame `w` × `h` pixels (rows from the top),
/// its pixels `par` times as wide as tall: the box black, the letters
/// white, as much of each pixel as they cover; `None` when none of it is
/// in the frame.
fn overlay(s: &Settings, text: &str, w: usize, h: usize, par: f64) -> Option<Overlay> {
    if w == 0 || h == 0 || text.is_empty() {
        return None;
    }
    let (fw, fh) = (w as f64, h as f64);
    let par = if par > 0.0 { par } else { 1.0 };
    let (sx, sy) = (s.width.max(0.01), s.height.max(0.01));
    // The box's em in columns and in rows (a column `par` times as wide as
    // a row is tall), and the letters'.
    let em = [fh * EM * sx / par, fh * EM * sy];
    let letters = [em[0] * sx.powf(LETTERS), em[1] * sy.powf(LETTERS)];
    let advance = text::advance(text) * letters[0];
    let left = match s.horizontal {
        0 => SAFE * fw,
        1 => (fw - advance) / 2.0,
        _ => (1.0 - SAFE) * fw - advance,
    };
    let baseline = match s.vertical {
        0 => SAFE * fh + ASCENT * letters[1],
        1 => (fh + ASCENT * letters[1]) / 2.0,
        _ => BASELINE * fh,
    };
    let bx = [left - BOX_SIDE * em[0], left + advance + BOX_SIDE * em[0]];
    let by = [baseline - BOX_TOP * em[1], baseline + BOX_BOTTOM * em[1]];
    // The pixels it covers: the box, and the letters (their descent too).
    let x0 = bx[0].min(left).floor().max(0.0);
    let x1 = bx[1].max(left + advance).ceil().min(fw);
    let y0 = by[0].min(baseline - ASCENT * letters[1]).floor().max(0.0);
    let y1 = by[1].max(baseline + 0.25 * letters[1]).ceil().min(fh);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (ow, oh) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let mut cover = Coverage::new(ow, oh);
    cover.text(text, [left - x0, baseline - y0], letters);
    let letters = cover.finish();
    // How much of a pixel `i` (from `from`) the span `lo`–`hi` covers.
    let span = |i: usize, from: f64, lo: f64, hi: f64| {
        let a = from + i as f64;
        ((hi.min(a + 1.0) - lo.max(a)).max(0.0)) as f32
    };
    let mut rgba = Vec::with_capacity(ow * oh);
    for j in 0..oh {
        let in_y = span(j, y0, by[0], by[1]);
        for i in 0..ow {
            let b = in_y * span(i, x0, bx[0], bx[1]);
            let c = letters[j * ow + i];
            rgba.push([c, c, c, c + b * (1.0 - c)]);
        }
    }
    Some(Overlay {
        x: x0 as i32,
        y: y0 as i32,
        w: ow as u32,
        h: oh as u32,
        rgba,
    })
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
    } else if is(action, kOfxImageEffectActionGetRegionOfDefinition) {
        // The picture, and the frame: the box may be beyond the picture.
        let time = args.dbl(kOfxPropTime, 0);
        let (origin, size) = fx.frame();
        let frame = [
            origin[0],
            origin[1],
            origin[0] + size[0],
            origin[1] + size[1],
        ];
        let rod = match fx.clip(kOfxImageEffectSimpleSourceClipName).rod(time) {
            Some(r) => [
                r[0].min(frame[0]),
                r[1].min(frame[1]),
                r[2].max(frame[2]),
                r[3].max(frame[3]),
            ],
            None => frame,
        };
        out.set_dbls(kOfxImageEffectPropRegionOfDefinition, &rod);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionGetClipPreferences) {
        // A new time in every frame: the frames differ even where the
        // picture does not.
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
    p.set_str(kOfxPropLabel, c"Timecode");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Utility");
    p.set_str(
        kOfxPropPluginDescription,
        c"Writes the frame's time, as the project's ruler shows it, over the picture, in white on a black box.",
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
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);

    let choice = |name: &CStr, label: &CStr, options: &[&CStr], default: c_int, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeChoice, name, label);
        p.set_strs(kOfxParamPropChoiceOption, options);
        p.set_int(kOfxParamPropDefault, default);
        // It may change over time, as VEGAS's (its clock).
        p.set_int(kOfxParamPropAnimates, 1);
        p.set_str(kOfxParamPropHint, hint);
    };
    choice(
        FORMAT,
        c"Timecode format",
        &FORMATS,
        SMPTE_DROP,
        c"How the time is written: as the ruler's formats write it.",
    );
    choice(
        HORIZONTAL,
        c"Horizontal alignment",
        &HORIZONTALS,
        2,
        c"Where the time is across the frame, inside its title-safe area.",
    );
    choice(
        VERTICAL,
        c"Vertical alignment",
        &VERTICALS,
        2,
        c"Where the time is up the frame, inside its title-safe area.",
    );
    let number = |name: &CStr, label: &CStr, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, 1.0);
        p.set_dbl(kOfxParamPropMin, 0.5);
        p.set_dbl(kOfxParamPropMax, 2.0);
        p.set_dbl(kOfxParamPropDisplayMin, 0.5);
        p.set_dbl(kOfxParamPropDisplayMax, 2.0);
        p.set_dbl(kOfxParamPropIncrement, 0.01);
        p.set_int(kOfxParamPropDigits, 3);
        p.set_str(kOfxParamPropHint, hint);
    };
    number(
        WIDTH,
        c"Width",
        c"How wide the time and its box are: 1 as they come.",
    );
    number(
        HEIGHT,
        c"Height",
        c"How tall the time and its box are: 1 as they come.",
    );
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|s| if s > 0.0 { s } else { 1.0 });
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    if let Some(s) = &src
        && s.depth != out.depth
    {
        return kOfxStatErrImageFormat;
    }
    let s = Settings::at(fx, time);
    let rate = fx.props().dbl(kOfxImageEffectPropFrameRate, 0);
    let rate = if rate > 0.0 { rate } else { 30.0 };
    // The frame's time on the ruler: BananaV's host says it; others' time
    // is their project's.
    let seconds = args.dbl_or(kBananaVImageEffectPropTimelineTime, f64::NAN);
    let seconds = if seconds.is_finite() {
        seconds
    } else {
        time / rate
    };
    let text = s.text(seconds, rate, bv_core::locale::decimal_separator());
    let (origin, frame) = fx.frame();
    let par = out.par;
    let first = [
        (origin[0] * scale[0] / par).round() as c_int,
        (origin[1] * scale[1]).round() as c_int,
    ];
    let w = ((frame[0] * scale[0] / par).round() as usize).max(1);
    let h = ((frame[1] * scale[1]).round() as usize).max(1);
    let overlay = s.overlay(&text, w, h, par);
    let premultiplied = src
        .as_ref()
        .is_some_and(|i| i.channels < 4 || !i.unpremultiplied);
    let out_straight = out.channels == 4 && out.unpremultiplied;
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(row) = out.row(y) else { return };
        // The frame's row, from its top.
        let fy = h as c_int - 1 - (y - first[1]);
        for x in columns.clone() {
            let px = src
                .as_ref()
                .and_then(|i| crate::colors::pixel(i, x, y))
                .map_or([0.0; 4], |c| {
                    if premultiplied {
                        c
                    } else {
                        [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
                    }
                });
            let mut v = match &overlay {
                Some(o) => o.over(x - first[0], fy, px),
                None => px,
            };
            if out_straight && v[3] > 0.0 {
                v = [v[0] / v[3], v[1] / v[3], v[2] / v[3], v[3]];
            }
            // SAFETY: a row the output has, a column inside its bounds.
            unsafe { crate::colors::put_pixel(&out, row, x, v) };
        }
    };
    for_rows(y1.max(out.bounds[1])..y2.min(out.bounds[3]), &fill);
    drop(src);
    drop(out);
    kOfxStatOK
}

// Presets.

/// The factory presets: VEGAS's.
pub(crate) fn presets() -> Vec<Preset> {
    let preset = |name: &'static str, format: i32, size: f64| Preset {
        name,
        values: vec![
            ("format", PresetValue::Num(vec![format as f64])),
            ("width", PresetValue::Num(vec![size])),
            ("height", PresetValue::Num(vec![size])),
        ],
    };
    vec![
        preset("SMPTE Drop (29.97 fps)", SMPTE_DROP, 1.0),
        preset("SMPTE Drop (29.97 fps), Small", SMPTE_DROP, 0.75),
        preset("SMPTE Non-Drop (29.97 fps)", 9, 1.0),
        preset("SMPTE Film IVTC (23.976 fps)", 6, 1.0),
        preset("SMPTE Film (24 fps)", 7, 1.0),
        preset("Seconds", 1, 1.0),
        preset("Absolute Frames", 3, 1.0),
        preset("Time & Frames", 2, 1.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VEGAS's frame 183 at 29.97 fps (6.106 s).
    const T: f64 = 183.0 * 1001.0 / 30000.0;
    const NTSC: f64 = 30000.0 / 1001.0;

    #[test]
    fn the_formats_write_what_vegass_do() {
        let f = |format: i32, sep: char| format_time(format, T, NTSC, sep);
        // VEGAS's pictures, in Portuguese (decimal commas).
        assert_eq!(f(10, ','), "00:00:06;03");
        assert_eq!(f(9, ','), "00:00:06:03");
        assert_eq!(f(6, ','), "00:00:06:02");
        assert_eq!(f(7, ','), "00:00:06:02");
        assert_eq!(f(1, ','), "6,106");
        assert_eq!(f(3, ','), "183,000");
        assert_eq!(f(2, ','), "00:00:06,03");
        // And the others, in English.
        assert_eq!(f(0, '.'), "00:00:06.106");
        assert_eq!(f(4, '.'), "4+23");
        assert_eq!(f(5, '.'), "11+07");
        assert_eq!(f(8, '.'), "00:00:06:02");
        assert_eq!(f(11, '.'), "00:00:06:03");
        // An hour and a minute in, before the project starts.
        assert_eq!(format_time(0, 3661.5, 25.0, '.'), "01:01:01.500");
        assert_eq!(format_time(1, -2.0, 25.0, '.'), "-2.000");
    }

    #[test]
    fn the_box_sits_as_vegass() {
        // VEGAS's picture 716 × 403: the box 177.5 × 31 pixels, 62.5 from
        // the right and 37.6 from the bottom; the text's end a tenth of
        // the width in, its baseline 0.895 down.
        let s = Settings::default();
        let o = s.overlay("00:00:06;03", 716, 403, 1.0).unwrap();
        let alpha = |x: i32, y: i32| {
            let (i, j) = (x - o.x, y - o.y);
            if i < 0 || j < 0 || i >= o.w as i32 || j >= o.h as i32 {
                return 0.0;
            }
            o.rgba[(j as u32 * o.w + i as u32) as usize][3]
        };
        let cols: Vec<i32> = (0..716).filter(|&x| alpha(x, 340) > 0.5).collect();
        let rows: Vec<i32> = (0..403).filter(|&y| alpha(cols[3], y) > 0.5).collect();
        let (left, right) = (cols[0], *cols.last().unwrap() + 1);
        let (top, bottom) = (rows[0], *rows.last().unwrap() + 1);
        assert!(
            ((right - left) as f64 - 177.5).abs() <= 2.0,
            "{left}..{right}"
        );
        assert!((716.0 - right as f64 - 62.5).abs() <= 2.0, "{right}");
        assert!(
            ((bottom - top) as f64 - 31.0).abs() <= 1.5,
            "{top}..{bottom}"
        );
        assert!((403.0 - bottom as f64 - 37.6).abs() <= 1.5, "{bottom}");
        // Inside, white letters on black.
        assert!(o.rgba.iter().any(|p| p[0] > 0.99 && p[3] > 0.99));
        assert!(o.rgba.iter().any(|p| p[0] == 0.0 && p[3] > 0.99));
    }

    #[test]
    fn small_letters_shrink_more_than_their_box() {
        // VEGAS's Small (0.75): the box 126 × 24, the letters 0.71 of
        // their size.
        let s = Settings {
            width: 0.75,
            height: 0.75,
            ..Settings::default()
        };
        let o = s.overlay("00:00:06;03", 716, 403, 1.0).unwrap();
        let wide = (o.w as f64, o.h as f64);
        assert!((wide.0 - 126.3).abs() <= 3.0, "{wide:?}");
        assert!((0.75f64.powf(1.2) - 0.708).abs() < 0.001);
    }

    #[test]
    fn the_presets_are_vegass() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            [
                "SMPTE Drop (29.97 fps)",
                "SMPTE Drop (29.97 fps), Small",
                "SMPTE Non-Drop (29.97 fps)",
                "SMPTE Film IVTC (23.976 fps)",
                "SMPTE Film (24 fps)",
                "Seconds",
                "Absolute Frames",
                "Time & Frames"
            ]
        );
    }
}
