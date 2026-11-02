//! Credit Roll: VEGAS's credits generator, as its Help describes it — the
//! credits typed as lines in VEGAS's own text format (a line beginning
//! with a space is a title, a tab parts two columns, `//` begins a
//! comment, the others are items; quotation marks keep spaces at an
//! entry's ends), each kind of line in its own style (font, size, bold,
//! italic, color, snapping, kerning, tracking, the room above and below
//! it),
//! scrolling over the event's length (up or down) or shown a few lines at
//! a time, one group after another (a timed sequence, with their ways in
//! and out), between the left and right margins (from the top one in a
//! timed sequence, dragged in the Position box: [`position_interface`]),
//! over a background color. Titles & Text ([`crate::titles`]) draws the
//! lines. The credits come from and go to text files, the other settings
//! to properties files (XML), through file parameters read or written as
//! soon as one is picked ([`file_picked`]).
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::{Path, PathBuf};

use bv_ofx_sys::extensions::kBananaVParamPropInteract;
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::fonts;
use crate::kit::smooth;
use crate::ofx::{Draw, Fx, Props, for_rows, interact_props, is, suites};
use crate::titles::{self, Picture};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.CreditRoll";

const TEXT: &CStr = c"text";
const EFFECT: &CStr = c"effect";
const DIRECTION: &CStr = c"direction";
const WAY_IN: &CStr = c"in";
const WAY_OUT: &CStr = c"out";
const DISPLAY: &CStr = c"display";
const LEFT: &CStr = c"left";
const RIGHT: &CStr = c"right";
const TOP: &CStr = c"top";
const BACKGROUND: &CStr = c"background";
const CENTER_WIDTH: &CStr = c"centerWidth";
const POSITION: &CStr = c"position";
const FILES: &CStr = c"files";
const IMPORT_TEXT: &CStr = c"importText";
const IMPORT_PROPERTIES: &CStr = c"importProperties";
const EXPORT_TEXT: &CStr = c"exportText";
const EXPORT_PROPERTIES: &CStr = c"exportProperties";

/// The Position box's size, pixels.
const POSITION_SIZE: [c_int; 2] = [180, 110];
/// The least room the margins leave between them, a part of the width.
const LEAST_WIDTH: f64 = 0.02;

/// A point's size, a part of the frame's height (as Titles & Text's).
const POINT: f64 = 4.0 / 3.0 / 1080.0;
/// A line's height without the room above and below it, in ems.
const LINE: f64 = 1.25;

const EFFECTS: [&CStr; 2] = [c"Scrolling Credits", c"Timed Sequence"];
const DIRECTIONS: [&CStr; 2] = [c"Bottom to Top", c"Top to Bottom"];
const WAYS: [&CStr; 6] = [
    c"Cut",
    c"Fade",
    c"Slide Up",
    c"Slide Down",
    c"Slide Left",
    c"Slide Right",
];
const SNAPPING: [&CStr; 3] = [c"Left", c"Center", c"Right"];

/// The credits a new roll has.
const SAMPLE: &str = " Credits\nDirected by\tYour Name\nEdited by\tYour Name\n Cast\nThe Hero\tSomeone\nThe Friend\tSomeone Else\n Thanks\nEveryone who helped";

/// A kind of line's look.
#[derive(Clone, Debug, PartialEq)]
struct Style {
    font: String,
    size: f64,
    bold: bool,
    italic: bool,
    color: [f64; 4],
    /// 0 left, 1 center, 2 right.
    snap: i32,
    /// The font's own kerning between pairs of letters.
    kerning: bool,
    /// Between letters, ems.
    tracking: f64,
    /// The room above and below the line, ems.
    above: f64,
    below: f64,
}

/// A style's parameters (Two Columns has no snapping: its columns meet at
/// the middle).
struct Names {
    group: &'static CStr,
    label: &'static CStr,
    font: &'static CStr,
    size: &'static CStr,
    bold: &'static CStr,
    italic: &'static CStr,
    color: &'static CStr,
    snap: Option<&'static CStr>,
    kerning: &'static CStr,
    tracking: &'static CStr,
    above: &'static CStr,
    below: &'static CStr,
    /// Its defaults: size, bold, snapping, room above and below.
    defaults: (f64, bool, i32, f64, f64),
}

const TITLE: Names = Names {
    group: c"titleStyle",
    label: c"Title Style",
    font: c"titleFont",
    size: c"titleSize",
    bold: c"titleBold",
    italic: c"titleItalic",
    color: c"titleColor",
    snap: Some(c"titleSnapping"),
    kerning: c"titleKerning",
    tracking: c"titleTracking",
    above: c"titleSpaceAbove",
    below: c"titleSpaceBelow",
    defaults: (44.0, true, 1, 0.8, 0.3),
};
const ITEM: Names = Names {
    group: c"itemStyle",
    label: c"Item Style",
    font: c"itemFont",
    size: c"itemSize",
    bold: c"itemBold",
    italic: c"itemItalic",
    color: c"itemColor",
    snap: Some(c"itemSnapping"),
    kerning: c"itemKerning",
    tracking: c"itemTracking",
    above: c"itemSpaceAbove",
    below: c"itemSpaceBelow",
    defaults: (34.0, false, 1, 0.0, 0.15),
};
const COLUMNS: Names = Names {
    group: c"columnsStyle",
    label: c"Two Columns Style",
    font: c"columnsFont",
    size: c"columnsSize",
    bold: c"columnsBold",
    italic: c"columnsItalic",
    color: c"columnsColor",
    snap: None,
    kerning: c"columnsKerning",
    tracking: c"columnsTracking",
    above: c"columnsSpaceAbove",
    below: c"columnsSpaceBelow",
    defaults: (34.0, false, 1, 0.0, 0.15),
};

impl Style {
    fn at(fx: Fx, n: &Names, time: f64) -> Style {
        let p = |name| fx.param(name);
        Style {
            font: p(n.font).text_at(time),
            size: p(n.size).f64_at(time),
            bold: p(n.bold).int_at(time) != 0,
            italic: p(n.italic).int_at(time) != 0,
            color: p(n.color).rgba_at(time),
            snap: n.snap.map_or(1, |s| p(s).int_at(time)),
            kerning: p(n.kerning).int_at(time) != 0,
            tracking: p(n.tracking).f64_at(time),
            above: p(n.above).f64_at(time),
            below: p(n.below).f64_at(time),
        }
    }

    /// The em, a part of the frame's height `fh`.
    fn em(&self, fh: f64) -> f64 {
        (self.size * POINT * fh).max(0.0)
    }

    /// A line's height in this style, the room above and below it too.
    fn height(&self, fh: f64) -> f64 {
        (self.above.max(0.0) + LINE + self.below.max(0.0)) * self.em(fh)
    }
}

/// The settings at a time.
#[derive(Clone, Debug, PartialEq)]
struct Settings {
    text: String,
    timed: bool,
    /// Scrolling from the top down.
    down: bool,
    way_in: i32,
    way_out: i32,
    /// Lines at once, in a timed sequence.
    display: usize,
    left: f64,
    right: f64,
    top: f64,
    background: [f64; 4],
    title: Style,
    item: Style,
    columns: Style,
    /// The room between the two columns, a part of the frame's width.
    center_width: f64,
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        let p = |name| fx.param(name);
        Settings {
            text: p(TEXT).text_at(time),
            timed: p(EFFECT).int_at(time) == 1,
            down: p(DIRECTION).int_at(time) == 1,
            way_in: p(WAY_IN).int_at(time),
            way_out: p(WAY_OUT).int_at(time),
            display: p(DISPLAY).int_at(time).max(1) as usize,
            left: p(LEFT).f64_at(time),
            right: p(RIGHT).f64_at(time),
            top: p(TOP).f64_at(time),
            background: p(BACKGROUND).rgba_at(time),
            title: Style::at(fx, &TITLE, time),
            item: Style::at(fx, &ITEM, time),
            columns: Style::at(fx, &COLUMNS, time),
            center_width: p(CENTER_WIDTH).f64_at(time),
        }
    }

    fn style(&self, line: &Line) -> &Style {
        match line {
            Line::Title(_) => &self.title,
            Line::Item(_) => &self.item,
            Line::Columns(..) => &self.columns,
        }
    }
}

/// One line of the credits.
#[derive(Clone, Debug, PartialEq)]
enum Line {
    Title(String),
    Item(String),
    Columns(String, String),
}

/// An entry without the spaces at its ends, or what its quotation marks
/// hold.
fn entry(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

/// The credits' lines, read as VEGAS's credit roll text files are.
fn lines(text: &str) -> Vec<Line> {
    let mut v: Vec<Line> = text
        .replace("\r\n", "\n")
        .split('\n')
        .filter(|raw| !raw.trim_start().starts_with("//"))
        .map(|raw| {
            if raw.starts_with(' ') {
                Line::Title(entry(raw))
            } else if let Some((a, b)) = raw.split_once('\t') {
                Line::Columns(entry(a), entry(b))
            } else {
                Line::Item(entry(raw))
            }
        })
        .collect();
    while v.last() == Some(&Line::Item(String::new())) {
        v.pop();
    }
    v
}

/// Where a line goes: its top (the frame's units, from the frame's top),
/// how far across it is moved and how opaque it is.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Place {
    top: f64,
    shift: f64,
    alpha: f32,
}

/// Where each shown line is at `time` (frames) of a roll `duration`
/// frames long, in a frame `fw` × `fh`.
fn places(
    s: &Settings,
    all: &[Line],
    time: f64,
    duration: f64,
    fw: f64,
    fh: f64,
) -> Vec<(usize, Place)> {
    let heights: Vec<f64> = all.iter().map(|l| s.style(l).height(fh)).collect();
    let p = (time / duration.max(1.0)).clamp(0.0, 1.0);
    if !s.timed {
        let total: f64 = heights.iter().sum();
        // From just below the frame to just above it, or the other way.
        let first = if s.down {
            -total + p * (fh + total)
        } else {
            fh - p * (fh + total)
        };
        let mut y = first;
        let mut v = Vec::new();
        for (i, h) in heights.iter().enumerate() {
            if y + h > 0.0 && y < fh {
                v.push((
                    i,
                    Place {
                        top: y,
                        shift: 0.0,
                        alpha: 1.0,
                    },
                ));
            }
            y += h;
        }
        return v;
    }
    // A timed sequence: groups of Display lines, one after another, each
    // coming in for its first fifth and going out for its last.
    let groups = all.len().div_ceil(s.display).max(1);
    let at = p * groups as f64;
    let g = (at.floor() as usize).min(groups - 1);
    let t = at - g as f64;
    let enter = smooth(0.0, 0.2, t);
    let leave = smooth(0.8, 1.0, t);
    let way = |kind: i32, k: f64, coming: bool| -> (f64, f64, f32) {
        // How far it still has to go (1: all the way), toward which side.
        let d = if coming { 1.0 - k } else { k };
        let sign = if coming { -1.0 } else { 1.0 };
        match kind {
            1 => (0.0, 0.0, (1.0 - d) as f32),
            2 => (0.0, -sign * d * fh, 1.0),
            3 => (0.0, sign * d * fh, 1.0),
            4 => (-sign * d * fw, 0.0, 1.0),
            5 => (sign * d * fw, 0.0, 1.0),
            _ => (0.0, 0.0, if d >= 1.0 { 0.0 } else { 1.0 }),
        }
    };
    let (ix, iy, ia) = way(s.way_in, enter, true);
    let (ox, oy, oa) = way(s.way_out, leave, false);
    let mut y = s.top * fh + iy + oy;
    let first = g * s.display;
    let mut v = Vec::new();
    for (i, h) in heights.iter().enumerate().skip(first).take(s.display) {
        v.push((
            i,
            Place {
                top: y,
                shift: ix + ox,
                alpha: ia * oa,
            },
        ));
        y += h;
    }
    v
}

/// The frame drawn: `fw` × `fh` units shown `k` pixels to a unit across
/// and up, as `w` × `h` pixels.
#[derive(Clone, Copy, Debug)]
struct Frame {
    fw: f64,
    fh: f64,
    k: [f64; 2],
    w: usize,
    h: usize,
}

/// The pictures of `line` placed at `at` in frame `f`.
fn draw(s: &Settings, line: &Line, at: Place, f: Frame) -> Vec<Picture> {
    let Frame { fw, fh, k, w, h } = f;
    let style = s.style(line);
    let top = at.top + style.above.max(0.0) * style.em(fh);
    let title = |text: &str, x: f64, snap: i32| titles::Settings {
        text: text.to_string(),
        font: style.font.clone(),
        size: style.size,
        bold: style.bold,
        italic: style.italic,
        justify: snap,
        color: style.color,
        scale: 1.0,
        location: [(x + at.shift) / fw, 1.0 - top / fh],
        // The box's top: left, middle or right.
        anchor: snap.clamp(0, 2),
        background: [0.0; 4],
        tracking: style.tracking,
        kerning: style.kerning,
        line_spacing: 1.0,
        outline_width: 0.0,
        shadow: false,
        ..titles::Settings::default()
    };
    let (left, right) = (s.left * fw, s.right * fw);
    let middle = (left + right) / 2.0;
    let texts = match line {
        Line::Title(t) | Line::Item(t) => {
            let x = match style.snap {
                0 => left,
                2 => right,
                _ => middle,
            };
            vec![title(t, x, style.snap.clamp(0, 2))]
        }
        Line::Columns(a, b) => {
            let gap = s.center_width.max(0.0) * fw / 2.0;
            vec![title(a, middle - gap, 2), title(b, middle + gap, 0)]
        }
    };
    texts
        .iter()
        .filter(|t| !t.text.trim().is_empty())
        .filter_map(|t| titles::picture(t, fw, fh, k, w, h))
        .map(|mut p| {
            if at.alpha < 1.0 {
                for c in &mut p.rgba {
                    *c = c.map(|v| v * at.alpha.max(0.0));
                }
            }
            p
        })
        .collect()
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
        let (origin, size) = fx.frame();
        out.set_dbls(
            kOfxImageEffectPropRegionOfDefinition,
            &[
                origin[0],
                origin[1],
                origin[0] + size[0],
                origin[1] + size[1],
            ],
        );
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionGetClipPreferences) {
        out.set_int(kOfxImageEffectFrameVarying, 1);
        out.set_str(kOfxImageClipPropFieldOrder, kOfxImageFieldNone);
        out.set_str(kOfxImageEffectPropPreMultiplication, kOfxImagePreMultiplied);
        kOfxStatOK
    } else if is(action, kOfxActionInstanceChanged) {
        let time = args.dbl(kOfxPropTime, 0);
        if args.is(kOfxPropChangeReason, kOfxChangeUserEdited) {
            file_picked(fx, &args.text(kOfxPropName, 0), time);
        }
        enable(fx, time);
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) || is(action, kOfxActionBeginInstanceEdit) {
        enable(fx, 0.0);
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

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Credit Roll");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Text");
    p.set_str(
        kOfxPropPluginDescription,
        c"Credits scrolling over the event's length, or shown a few lines at a time: titles, items and two columns, each in its own style.",
    );
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[
            kOfxImageEffectContextGenerator,
            kOfxImageEffectContextGeneral,
        ],
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
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);
    let choice = |name: &CStr,
                  label: &CStr,
                  options: &[&CStr],
                  v: i32,
                  parent: Option<&CStr>,
                  hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeChoice, name, label);
        p.set_strs(kOfxParamPropChoiceOption, options);
        p.set_int(kOfxParamPropDefault, v);
        p.set_str(kOfxParamPropHint, hint);
        if let Some(parent) = parent {
            p.set_str(kOfxParamPropParent, parent);
        }
    };
    let number = |name: &CStr,
                  label: &CStr,
                  [min, max, default]: [f64; 3],
                  parent: Option<&CStr>,
                  hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, default);
        p.set_dbl(kOfxParamPropMin, min);
        p.set_dbl(kOfxParamPropMax, max);
        p.set_dbl(kOfxParamPropDisplayMin, min);
        p.set_dbl(kOfxParamPropDisplayMax, max);
        p.set_dbl(kOfxParamPropIncrement, (max - min) / 100.0);
        p.set_int(kOfxParamPropDigits, 3);
        p.set_str(kOfxParamPropHint, hint);
        if let Some(parent) = parent {
            p.set_str(kOfxParamPropParent, parent);
        }
    };
    let toggle = |name: &CStr, label: &CStr, v: bool, parent: Option<&CStr>, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeBoolean, name, label);
        p.set_int(kOfxParamPropDefault, c_int::from(v));
        p.set_str(kOfxParamPropHint, hint);
        if let Some(parent) = parent {
            p.set_str(kOfxParamPropParent, parent);
        }
    };
    let color = |name: &CStr, label: &CStr, v: [f64; 4], parent: Option<&CStr>, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeRGBA, name, label);
        p.set_dbls(kOfxParamPropDefault, &v);
        p.set_str(kOfxParamPropHint, hint);
        if let Some(parent) = parent {
            p.set_str(kOfxParamPropParent, parent);
        }
    };

    let p = fx.define_param(kOfxParamTypeString, TEXT, c"Credits Text");
    p.set_str(kOfxParamPropStringMode, kOfxParamStringIsMultiLine);
    let sample = CString::new(SAMPLE).unwrap_or_default();
    p.set_str(kOfxParamPropDefault, &sample);
    p.set_str(
        kOfxParamPropHint,
        c"A line each: begin a title with a space, part two columns with a tab, begin a comment with //; quotation marks keep spaces.",
    );
    // VEGAS's toolbar's imports and exports: a file each, read or written
    // once picked (the path then goes back to empty).
    let g = fx.define_param(kOfxParamTypeGroup, FILES, c"Import and Export");
    g.set_int(kOfxParamPropGroupOpen, 0);
    for (name, label, write, hint) in [
        (
            IMPORT_TEXT,
            c"Import Text",
            false,
            c"Adds the credits of a text file (.txt) after these, or puts them in place of the sample ones.",
        ),
        (
            IMPORT_PROPERTIES,
            c"Import Properties",
            false,
            c"Takes the settings a properties file (.xml) keeps: the effect, the position, the background and the styles.",
        ),
        (
            EXPORT_TEXT,
            c"Export Text",
            true,
            c"Saves the credits as a text file (.txt).",
        ),
        (
            EXPORT_PROPERTIES,
            c"Export Properties",
            true,
            c"Saves the settings but the credits as a properties file (.xml).",
        ),
    ] {
        let p = fx.define_param(kOfxParamTypeString, name, label);
        p.set_str(kOfxParamPropStringMode, kOfxParamStringIsFilePath);
        p.set_int(kOfxParamPropStringFilePathExists, c_int::from(!write));
        p.set_int(kOfxParamPropAnimates, 0);
        p.set_str(kOfxParamPropHint, hint);
        p.set_str(kOfxParamPropParent, FILES);
    }
    choice(
        EFFECT,
        c"Effect",
        &EFFECTS,
        0,
        None,
        c"The credits scrolling evenly over the event, or shown a few lines at a time.",
    );
    // The margins, with VEGAS's box: the frame, the credits' part of it
    // and the handles that drag the margins.
    let g = fx.define_param(kOfxParamTypeGroup, POSITION, c"Position");
    g.set_ptr(kBananaVParamPropInteract, position_interface as *mut c_void);
    g.set_ints(kOfxParamPropInteractPreferedSize, &POSITION_SIZE);
    number(
        LEFT,
        c"Left",
        [0.0, 1.0, 0.1],
        Some(POSITION),
        c"Where the credits' left side is: a part of the frame's width.",
    );
    number(
        RIGHT,
        c"Right",
        [0.0, 1.0, 0.9],
        Some(POSITION),
        c"Where the credits' right side is: a part of the frame's width.",
    );
    number(
        TOP,
        c"Top",
        [0.0, 1.0, 0.1],
        Some(POSITION),
        c"Where a timed sequence's lines begin: a part of the frame's height, from its top.",
    );
    choice(
        DIRECTION,
        c"Scroll Direction",
        &DIRECTIONS,
        0,
        None,
        c"Which way scrolling credits move.",
    );
    choice(
        WAY_IN,
        c"In",
        &WAYS,
        1,
        None,
        c"How each group of a timed sequence comes in.",
    );
    choice(
        WAY_OUT,
        c"Out",
        &WAYS,
        1,
        None,
        c"How each group of a timed sequence goes out.",
    );
    let p = fx.define_param(kOfxParamTypeInteger, DISPLAY, c"Display");
    p.set_int(kOfxParamPropDefault, 3);
    p.set_int(kOfxParamPropMin, 1);
    p.set_int(kOfxParamPropMax, 20);
    p.set_int(kOfxParamPropDisplayMin, 1);
    p.set_int(kOfxParamPropDisplayMax, 10);
    p.set_str(
        kOfxParamPropHint,
        c"How many lines a timed sequence shows at once.",
    );
    color(
        BACKGROUND,
        c"Background",
        [0.0, 0.0, 0.0, 0.0],
        None,
        c"The frame's color behind the credits (transparent: what is below shows).",
    );
    let families: Vec<CString> = fonts::families()
        .into_iter()
        .filter_map(|f| CString::new(f).ok())
        .collect();
    let names: Vec<&CStr> = families.iter().map(|c| c.as_c_str()).collect();
    let default_font = CString::new(fonts::default_family()).unwrap_or_default();
    for n in [&TITLE, &ITEM, &COLUMNS] {
        let (size, bold, snap, above, below) = n.defaults;
        let g = fx.define_param(kOfxParamTypeGroup, n.group, n.label);
        g.set_int(kOfxParamPropGroupOpen, 0);
        let p = fx.define_param(kOfxParamTypeStrChoice, n.font, c"Font");
        p.set_strs(kOfxParamPropChoiceOption, &names);
        p.set_strs(kOfxParamPropChoiceEnum, &names);
        p.set_str(kOfxParamPropDefault, &default_font);
        p.set_str(kOfxParamPropHint, c"The typeface.");
        p.set_str(kOfxParamPropParent, n.group);
        number(
            n.size,
            c"Size",
            [1.0, 400.0, size],
            Some(n.group),
            c"The font's size in points, for a frame 1080 lines tall (the same part of other frames).",
        );
        toggle(
            n.bold,
            c"Bold",
            bold,
            Some(n.group),
            c"The family's bold face.",
        );
        toggle(
            n.italic,
            c"Italic",
            false,
            Some(n.group),
            c"The family's italic face.",
        );
        color(
            n.color,
            c"Text Color",
            [1.0, 1.0, 1.0, 1.0],
            Some(n.group),
            c"The letters' color.",
        );
        if let Some(name) = n.snap {
            choice(
                name,
                c"Snapping",
                &SNAPPING,
                snap,
                Some(n.group),
                c"The lines against the left margin, in the middle or against the right margin.",
            );
        } else {
            number(
                CENTER_WIDTH,
                c"Center Width",
                [0.0, 0.5, 0.04],
                Some(n.group),
                c"The room between the two columns, a part of the frame's width.",
            );
        }
        // Off, as VEGAS's Styles tab shows it.
        toggle(
            n.kerning,
            c"Kerning",
            false,
            Some(n.group),
            c"The font's own spacing between pairs of letters, as in AV or To.",
        );
        number(
            n.tracking,
            c"Tracking",
            [-0.5, 1.0, 0.0],
            Some(n.group),
            c"The room between letters, in ems.",
        );
        number(
            n.above,
            c"Space Above",
            [0.0, 4.0, above],
            Some(n.group),
            c"The room above each line, in ems.",
        );
        number(
            n.below,
            c"Space Below",
            [0.0, 4.0, below],
            Some(n.group),
            c"The room below each line, in ems.",
        );
    }
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|s| if s > 0.0 { s } else { 1.0 });
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let s = Settings::at(fx, time);
    // The roll lasts the event (ten seconds where the host does not say).
    let props = fx.props();
    let rate = props.dbl_or(kOfxImageEffectPropFrameRate, 30.0).max(1.0);
    let duration = props.dbl_or(kOfxImageEffectInstancePropEffectDuration, 0.0);
    let duration = if duration >= 1.0 && duration < rate * 3600.0 {
        duration
    } else {
        rate * 10.0
    };
    let (origin, frame) = fx.frame();
    let par = if out.par > 0.0 { out.par } else { 1.0 };
    let k = [scale[0] / par, scale[1]];
    let first = [
        (origin[0] * k[0]).round() as c_int,
        (origin[1] * k[1]).round() as c_int,
    ];
    let w = ((frame[0] * k[0]).round() as usize).max(1);
    let h = ((frame[1] * k[1]).round() as usize).max(1);
    let all = lines(&s.text);
    let f = Frame {
        fw: frame[0],
        fh: frame[1],
        k,
        w,
        h,
    };
    let pictures: Vec<Picture> = places(&s, &all, time, duration, frame[0], frame[1])
        .into_iter()
        .flat_map(|(i, at)| draw(&s, &all[i], at, f))
        .collect();
    let back = {
        let a = s.background[3].clamp(0.0, 1.0) as f32;
        [
            s.background[0] as f32 * a,
            s.background[1] as f32 * a,
            s.background[2] as f32 * a,
            a,
        ]
    };
    let straight = out.channels == 4 && out.unpremultiplied;
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(row) = out.row(y) else { return };
        // The frame's row, from its top.
        let fy = h as i64 - 1 - (y - first[1]) as i64;
        let on_row: Vec<&Picture> = pictures
            .iter()
            .filter(|p| fy >= p.region.y && fy < p.region.y + p.region.h as i64)
            .collect();
        for x in columns.clone() {
            let fx_ = (x - first[0]) as i64;
            let mut v = back;
            for p in &on_row {
                let (i, j) = (fx_ - p.region.x, fy - p.region.y);
                if i >= 0 && (i as usize) < p.region.w {
                    let t = p.rgba[j as usize * p.region.w + i as usize];
                    let k = 1.0 - t[3];
                    v = [
                        t[0] + v[0] * k,
                        t[1] + v[1] * k,
                        t[2] + v[2] * k,
                        t[3] + v[3] * k,
                    ];
                }
            }
            if straight && v[3] > 0.0 {
                v = [v[0] / v[3], v[1] / v[3], v[2] / v[3], v[3]];
            }
            // SAFETY: a row the output has, a column inside its bounds.
            unsafe { crate::colors::put_pixel(&out, row, x, v) };
        }
    };
    for_rows(y1.max(out.bounds[1])..y2.min(out.bounds[3]), &fill);
    drop(out);
    kOfxStatOK
}

/// What each effect uses, as VEGAS shows it: scrolling credits their
/// direction, a timed sequence its ways in and out, how many lines at
/// once and its top.
fn enable(fx: Fx, time: f64) {
    let timed = fx.param(EFFECT).int_at(time) == 1;
    fx.param(DIRECTION).set_secret(timed);
    for name in [WAY_IN, WAY_OUT, DISPLAY] {
        fx.param(name).set_secret(!timed);
    }
    fx.param(TOP).set_enabled(timed);
}

// Importing and exporting.

/// A file picked under Import and Export (parameter `name`), read or
/// written at once; its path then goes back to empty, so the same file can
/// be picked again. A file that fails says so until one works.
fn file_picked(fx: Fx, name: &str, time: f64) {
    let Some(which) = [
        IMPORT_TEXT,
        IMPORT_PROPERTIES,
        EXPORT_TEXT,
        EXPORT_PROPERTIES,
    ]
    .into_iter()
    .find(|n| n.to_bytes() == name.as_bytes()) else {
        return;
    };
    let param = fx.param(which);
    let path = param.text_at(time);
    let path = PathBuf::from(path.trim());
    if path.as_os_str().is_empty() {
        return;
    }
    let failed = if which == IMPORT_TEXT {
        match std::fs::read(&path) {
            Ok(bytes) => {
                let now = fx.param(TEXT).text_at(time);
                fx.param(TEXT)
                    .set_text(&with_imported(&now, &decode(&bytes)));
                None
            }
            Err(_) => Some(c"The credits text file could not be read."),
        }
    } else if which == EXPORT_TEXT {
        let text = fx.param(TEXT).text_at(time);
        std::fs::write(with_extension(&path, "txt"), windows_lines(&text))
            .err()
            .map(|_| c"The credits text file could not be written.")
    } else if which == IMPORT_PROPERTIES {
        match std::fs::read(&path) {
            Ok(bytes) if set_properties(fx, &read_properties(&decode(&bytes))) > 0 => None,
            Ok(_) => Some(c"The file holds no credit roll properties."),
            Err(_) => Some(c"The properties file could not be read."),
        }
    } else {
        let xml = properties_xml(&properties(fx, time));
        std::fs::write(with_extension(&path, "xml"), xml)
            .err()
            .map(|_| c"The properties file could not be written.")
    };
    fx.persistent_message(failed);
    param.set_text("");
}

/// The credits once `new` is imported: in place of the sample a new roll
/// has (or of none), else after the credits there are, as VEGAS adds them
/// when no row is picked.
fn with_imported(now: &str, new: &str) -> String {
    let new = new.replace("\r\n", "\n");
    let new = new.trim_end_matches('\n');
    if now == SAMPLE || now.trim().is_empty() {
        new.to_string()
    } else {
        format!("{}\n{new}", now.trim_end_matches(['\r', '\n']))
    }
}

/// `path`, given extension `ext` when it has none.
fn with_extension(path: &Path, ext: &str) -> PathBuf {
    if path.extension().is_some() {
        path.to_path_buf()
    } else {
        path.with_extension(ext)
    }
}

/// A text with Windows' line ends, as VEGAS's text files have them.
fn windows_lines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

/// A text file's text: UTF-8 (with or without its mark), UTF-16 with its
/// mark, else Windows' Western code page, as Notepad saved it for years.
fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    let utf16 = |rest: &[u8], big: bool| {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| {
                if big {
                    u16::from_be_bytes(c)
                } else {
                    u16::from_le_bytes(c)
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, false);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, true);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes
            .iter()
            .map(|&b| match b {
                0x80..=0x9F => WINDOWS_1252[usize::from(b - 0x80)],
                _ => char::from(b),
            })
            .collect(),
    }
}

/// Windows-1252's characters 0x80 to 0x9F (the others are Latin-1's).
const WINDOWS_1252: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{2C6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8D}', '\u{17D}', '\u{8F}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{2DC}', '\u{2122}', '\u{161}', '\u{203A}', '\u{153}', '\u{9D}', '\u{17E}', '\u{178}',
];

/// How a setting a properties file keeps reads.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Int,
    Num,
    Color,
    Text,
}

/// The settings a properties file keeps: all but the credits.
fn kept() -> Vec<(&'static CStr, Kind)> {
    let mut v = vec![
        (EFFECT, Kind::Int),
        (LEFT, Kind::Num),
        (RIGHT, Kind::Num),
        (TOP, Kind::Num),
        (DIRECTION, Kind::Int),
        (WAY_IN, Kind::Int),
        (WAY_OUT, Kind::Int),
        (DISPLAY, Kind::Int),
        (BACKGROUND, Kind::Color),
        (CENTER_WIDTH, Kind::Num),
    ];
    for n in [&TITLE, &ITEM, &COLUMNS] {
        v.extend([
            (n.font, Kind::Text),
            (n.size, Kind::Num),
            (n.bold, Kind::Int),
            (n.italic, Kind::Int),
            (n.color, Kind::Color),
        ]);
        if let Some(snap) = n.snap {
            v.push((snap, Kind::Int));
        }
        v.extend([
            (n.kerning, Kind::Int),
            (n.tracking, Kind::Num),
            (n.above, Kind::Num),
            (n.below, Kind::Num),
        ]);
    }
    v
}

/// The settings at `time`, by name, as a properties file writes them.
fn properties(fx: Fx, time: f64) -> Vec<(String, String)> {
    kept()
        .into_iter()
        .map(|(name, kind)| {
            let p = fx.param(name);
            let value = match kind {
                Kind::Int => p.int_at(time).to_string(),
                Kind::Num => p.f64_at(time).to_string(),
                Kind::Color => p.rgba_at(time).map(|v| v.to_string()).join(" "),
                Kind::Text => p.text_at(time),
            };
            (name.to_string_lossy().into_owned(), value)
        })
        .collect()
}

/// Sets the settings a properties file gave; how many of them it knew.
fn set_properties(fx: Fx, read: &[(String, String)]) -> usize {
    let mut known = 0;
    for (name, kind) in kept() {
        let Some((_, v)) = read.iter().find(|(n, _)| n.as_bytes() == name.to_bytes()) else {
            continue;
        };
        let p = fx.param(name);
        match kind {
            Kind::Int => {
                if let Ok(i) = v.trim().parse::<c_int>() {
                    p.set_int(i);
                    known += 1;
                }
            }
            Kind::Num => {
                if let Ok(x) = v.trim().parse::<f64>()
                    && x.is_finite()
                {
                    p.set_f64(x);
                    known += 1;
                }
            }
            Kind::Color => {
                let n: Vec<f64> = v
                    .split_whitespace()
                    .filter_map(|t| t.parse().ok())
                    .collect();
                if let [r, g, b, a] = n[..] {
                    p.set_rgba([r, g, b, a]);
                    known += 1;
                }
            }
            Kind::Text => {
                p.set_text(v);
                known += 1;
            }
        }
    }
    known
}

/// A properties file: the settings as XML, an element each.
fn properties_xml(values: &[(String, String)]) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<CreditRollProperties version=\"1\">\r\n",
    );
    for (name, value) in values {
        s.push_str(&format!(
            "  <Property name=\"{}\" value=\"{}\"/>\r\n",
            escape(name),
            escape(value)
        ));
    }
    s.push_str("</CreditRollProperties>\r\n");
    s
}

/// A properties file's settings, by name (other elements left out).
fn read_properties(xml: &str) -> Vec<(String, String)> {
    let mut v = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<Property") {
        rest = &rest[i + "<Property".len()..];
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        if let (Some(name), Some(value)) = (attribute(tag, "name"), attribute(tag, "value")) {
            v.push((name, value));
        }
        rest = &rest[end..];
    }
    v
}

/// Attribute `name`'s value in a tag's text.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    loop {
        let i = rest.find(name)?;
        let before = rest[..i].chars().next_back();
        let after = &rest[i + name.len()..];
        rest = after;
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(after) = after.trim_start().strip_prefix('=') else {
            continue;
        };
        let after = after.trim_start();
        let quote = after.chars().next().filter(|q| matches!(q, '"' | '\''))?;
        let body = &after[1..];
        let end = body.find(quote)?;
        return Some(unescape(&body[..end]));
    }
}

/// A text as an XML attribute's value.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c => out.push(c),
        }
    }
    out
}

/// An XML attribute's value as a text: its entities and character
/// references read.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else {
            break;
        };
        let entity = &rest[1..end];
        let c = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .map(|h| u32::from_str_radix(h, 16))
                .or_else(|| entity.strip_prefix('#').map(str::parse::<u32>))
                .and_then(Result::ok)
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

// The Position box.

/// How near the pen must be to a handle to take it, pixels.
const HANDLE_REACH: f64 = 6.0;

/// Where the Position box draws the frame in its pixels (y up), in the
/// project's shape: left, bottom, right and top.
fn frame_box(size: [f64; 2], aspect: f64) -> [f64; 4] {
    let (w, h) = ((size[0] - 8.0).max(8.0), (size[1] - 8.0).max(8.0));
    let aspect = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        16.0 / 9.0
    };
    let (bw, bh) = if w / h > aspect {
        (h * aspect, h)
    } else {
        (w, w / aspect)
    };
    let y0 = (size[1] - bh) / 2.0;
    [4.0, y0, 4.0 + bw, y0 + bh]
}

/// The margins' handles in frame box `b`: left and right, at the top of
/// a timed sequence (`top`) or halfway up scrolling credits.
fn handles(b: [f64; 4], left: f64, right: f64, top: Option<f64>) -> [[f64; 2]; 2] {
    let [x0, y0, x1, y1] = b;
    let y = top.map_or((y0 + y1) / 2.0, |t| y1 - t.clamp(0.0, 1.0) * (y1 - y0));
    let x = |v: f64| x0 + v.clamp(0.0, 1.0) * (x1 - x0);
    [[x(left), y], [x(right), y]]
}

/// The handle under the pen, the nearest.
fn handle_at(hs: [[f64; 2]; 2], pen: [f64; 2]) -> Option<usize> {
    let d = |h: [f64; 2]| (h[0] - pen[0]).abs().max((h[1] - pen[1]).abs());
    (0..2)
        .filter(|&i| d(hs[i]) <= HANDLE_REACH)
        .min_by(|&a, &b| d(hs[a]).total_cmp(&d(hs[b])))
}

/// The left, right and top margins once handle `which` (0 the left) is
/// dragged to `pen` in frame box `b`: the margins never cross, and the
/// top only moves in a timed sequence.
fn dragged(
    b: [f64; 4],
    which: usize,
    pen: [f64; 2],
    [left, right, top]: [f64; 3],
    timed: bool,
) -> [f64; 3] {
    let [x0, y0, x1, y1] = b;
    let x = ((pen[0] - x0) / (x1 - x0)).clamp(0.0, 1.0);
    let top = if timed {
        ((y1 - pen[1]) / (y1 - y0)).clamp(0.0, 1.0)
    } else {
        top
    };
    if which == 0 {
        [x.min(right - LEAST_WIDTH).max(0.0), right, top]
    } else {
        [left, x.max(left + LEAST_WIDTH).min(1.0), top]
    }
}

/// What the Position box remembers: the handle being dragged.
#[derive(Debug, Default)]
struct PositionState {
    drag: Option<usize>,
}

/// The Position box (VEGAS's): the frame, the credits' part of it, and
/// the yellow handles that drag the left and right margins — and the top,
/// in a timed sequence.
unsafe extern "C" fn position_interface(
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
        let state = Box::into_raw(Box::new(PositionState::default()));
        props.set_ptr(kOfxPropInstanceData, state as *mut c_void);
        return kOfxStatOK;
    }
    let state = props.ptr(kOfxPropInstanceData) as *mut PositionState;
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
    let (_, frame) = fx.frame();
    let b = frame_box([size[0] as f64, size[1] as f64], frame[0] / frame[1]);
    let timed = fx.param(EFFECT).int_at(time) == 1;
    let params = [fx.param(LEFT), fx.param(RIGHT), fx.param(TOP)];
    let now = params.map(|p| p.f64_at(time));
    let top = timed.then_some(now[2]);
    if is(action, kOfxInteractActionDraw) {
        if let Some(d) = Draw::of(args) {
            draw_position(&d, b, now, top, state.drag);
        }
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    if is(action, kOfxInteractActionPenDown) {
        match handle_at(handles(b, now[0], now[1], top), pen) {
            Some(i) => {
                state.drag = Some(i);
                kOfxStatOK
            }
            None => kOfxStatReplyDefault,
        }
    } else if is(action, kOfxInteractActionPenMotion) {
        let Some(i) = state.drag else {
            return kOfxStatReplyDefault;
        };
        let to = dragged(b, i, pen, now, timed);
        for ((p, v), was) in params.into_iter().zip(to).zip(now) {
            if v != was {
                p.set_f64(v);
            }
        }
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenUp) {
        if state.drag.take().is_some() {
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else {
        kOfxStatReplyDefault
    }
}

/// The box as VEGAS draws it: the frame grey, the credits' part of it
/// blue (below the top of a timed sequence) with lines of text at its top,
/// the handles yellow (lighter while dragged).
fn draw_position(
    d: &Draw,
    b: [f64; 4],
    [left, right, _]: [f64; 3],
    top: Option<f64>,
    hot: Option<usize>,
) {
    let rect = |r: [f64; 4]| [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]];
    let [_, y0, _, y1] = b;
    d.colour([0.45, 0.45, 0.45, 1.0]);
    d.fill(&rect(b));
    let hs = handles(b, left, right, top);
    let [[lx, hy], [rx, _]] = hs;
    let area_top = if top.is_some() { hy } else { y1 };
    d.colour([0.03, 0.03, 0.5, 1.0]);
    d.fill(&rect([lx, y0, rx.max(lx), area_top]));
    let em = ((y1 - y0) / 14.0).max(2.0);
    let (mid, half) = ((lx + rx) / 2.0, (rx - lx).max(0.0) / 2.0);
    d.colour([1.0, 1.0, 1.0, 0.9]);
    for (i, part) in [0.6, 0.4, 0.5].into_iter().enumerate() {
        let y = area_top - em * (1.0 + 1.5 * i as f64);
        if y - em * 0.6 < y0 {
            break;
        }
        d.fill(&rect([
            mid - half * part,
            y - em * 0.6,
            mid + half * part,
            y,
        ]));
    }
    d.colour([0.2, 0.2, 0.2, 1.0]);
    d.width(1.0);
    d.outline(&rect(b));
    for (i, [hx, y]) in hs.into_iter().enumerate() {
        let r = [hx - 4.0, y - 4.0, hx + 4.0, y + 4.0];
        d.colour(if hot == Some(i) {
            [1.0, 0.95, 0.6, 1.0]
        } else {
            [1.0, 0.82, 0.1, 1.0]
        });
        d.fill(&rect(r));
        d.colour([0.1, 0.1, 0.1, 1.0]);
        d.outline(&rect(r));
    }
}

/// Credit Roll's factory presets.
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    let p = |name: &'static str, values: Vec<(&'static str, PresetValue)>| Preset { name, values };
    vec![
        p("Scrolling Credits", vec![]),
        p("Scrolling Down", vec![("direction", num(&[1.0]))]),
        p(
            "Over Black",
            vec![("background", num(&[0.0, 0.0, 0.0, 1.0]))],
        ),
        p(
            "Timed Sequence, Fades",
            vec![("effect", num(&[1.0])), ("display", num(&[3.0]))],
        ),
        p(
            "Timed Sequence, Slides",
            vec![
                ("effect", num(&[1.0])),
                ("display", num(&[2.0])),
                ("in", num(&[4.0])),
                ("out", num(&[4.0])),
            ],
        ),
        p(
            "Left Aligned",
            vec![
                ("titleSnapping", num(&[0.0])),
                ("itemSnapping", num(&[0.0])),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(size: f64) -> Style {
        Style {
            font: String::new(),
            size,
            bold: false,
            italic: false,
            color: [1.0; 4],
            snap: 1,
            kerning: false,
            tracking: 0.0,
            above: 0.0,
            below: 0.0,
        }
    }

    fn settings(timed: bool) -> Settings {
        Settings {
            text: SAMPLE.into(),
            timed,
            down: false,
            way_in: 1,
            way_out: 1,
            display: 2,
            left: 0.1,
            right: 0.9,
            top: 0.1,
            background: [0.0; 4],
            title: style(44.0),
            item: style(34.0),
            columns: style(34.0),
            center_width: 0.04,
        }
    }

    #[test]
    fn the_text_reads_as_vegas_files_do() {
        let l = lines(" Cast\nThe Hero\tSomeone\n// a note\nEveryone\n\"  spaced \"\n\n");
        assert_eq!(
            l,
            vec![
                Line::Title("Cast".into()),
                Line::Columns("The Hero".into(), "Someone".into()),
                Line::Item("Everyone".into()),
                Line::Item("  spaced ".into()),
            ]
        );
    }

    #[test]
    fn scrolling_credits_cross_the_frame_over_the_event() {
        let s = settings(false);
        let all = lines(&s.text);
        let (fw, fh) = (1920.0, 1080.0);
        // At the start everything is below the frame, at the end above it.
        assert!(places(&s, &all, 0.0, 300.0, fw, fh).is_empty());
        assert!(places(&s, &all, 300.0, 300.0, fw, fh).is_empty());
        // Halfway some lines show, moving up.
        let a = places(&s, &all, 150.0, 300.0, fw, fh);
        let b = places(&s, &all, 160.0, 300.0, fw, fh);
        assert!(!a.is_empty());
        let top =
            |v: &[(usize, Place)], i: usize| v.iter().find(|(j, _)| *j == i).map(|(_, p)| p.top);
        let i = a[a.len() / 2].0;
        assert!(top(&b, i).unwrap() < top(&a, i).unwrap());
    }

    #[test]
    fn a_timed_sequence_shows_its_groups_in_turn() {
        let s = settings(true);
        let all = lines(&s.text);
        let (fw, fh) = (1920.0, 1080.0);
        let groups = all.len().div_ceil(2);
        let d = 100.0 * groups as f64;
        // The middle of each group's time: its lines, opaque, in place.
        for g in 0..groups {
            let v = places(&s, &all, d * (g as f64 + 0.5) / groups as f64, d, fw, fh);
            assert_eq!(v[0].0, g * 2);
            assert!((v[0].1.alpha - 1.0).abs() < 1e-6);
            assert!((v[0].1.top - 0.1 * fh).abs() < 1e-6);
        }
        // Fading in at a group's start.
        let v = places(&s, &all, 1.0, d, fw, fh);
        assert!(v[0].1.alpha < 0.5);
    }

    #[test]
    fn properties_files_read_back_what_they_wrote() {
        let values: Vec<(String, String)> = [
            ("effect", "1"),
            ("titleFont", "Tom & \"Jerry's\" <Sans>"),
            ("background", "0 0.5 1 0.25"),
            ("odd", "a\tb\nc"),
        ]
        .map(|(n, v)| (n.to_string(), v.to_string()))
        .into();
        let xml = properties_xml(&values);
        assert_eq!(read_properties(&xml), values);
        // Others' spelling: single quotes, spaces, references; other
        // elements left out.
        let read = read_properties(
            "<x><Property  value = '&#65;&#x42;&lt;' name='left' /><Other name=\"a\" value=\"b\"/></x>",
        );
        assert_eq!(read, vec![("left".to_string(), "AB<".to_string())]);
        // Every setting but the credits, once.
        let names: Vec<&CStr> = kept().iter().map(|(n, _)| *n).collect();
        let unique: std::collections::BTreeSet<&CStr> = names.iter().copied().collect();
        assert_eq!(unique.len(), names.len());
        assert!(!names.contains(&TEXT));
    }

    #[test]
    fn text_files_read_in_their_encodings() {
        assert_eq!(decode(b"\xEF\xBB\xBFCaf\xC3\xA9"), "Café");
        assert_eq!(decode(b"Caf\xC3\xA9"), "Café");
        assert_eq!(decode(&[0xFF, 0xFE, b'H', 0, 0xE9, 0]), "Hé");
        assert_eq!(decode(&[0xFE, 0xFF, 0, b'H', 0, 0xE9]), "Hé");
        // Windows' code page: its quotation marks and dashes.
        assert_eq!(
            decode(b"\x93Caf\xE9\x94 \x96 x"),
            "\u{201C}Café\u{201D} \u{2013} x"
        );
    }

    #[test]
    fn imported_credits_replace_the_sample_or_follow_the_others() {
        assert_eq!(with_imported(SAMPLE, " Cast\r\nHero\r\n"), " Cast\nHero");
        assert_eq!(with_imported("", "A"), "A");
        assert_eq!(with_imported("A\n", "B\n"), "A\nB");
        assert_eq!(windows_lines("A\nB\r\nC"), "A\r\nB\r\nC");
        assert_eq!(
            with_extension(Path::new("c/credits"), "txt"),
            PathBuf::from("c/credits.txt")
        );
        assert_eq!(
            with_extension(Path::new("c/credits.text"), "txt"),
            PathBuf::from("c/credits.text")
        );
    }

    #[test]
    fn the_position_box_drags_the_margins() {
        let b = frame_box([180.0, 110.0], 16.0 / 9.0);
        // The frame in its shape, inside the box.
        assert!(((b[2] - b[0]) / (b[3] - b[1]) - 16.0 / 9.0).abs() < 1e-9);
        assert!(b[0] >= 0.0 && b[2] <= 180.0 && b[1] >= 0.0 && b[3] <= 110.0);
        let hs = handles(b, 0.1, 0.9, None);
        assert_eq!(handle_at(hs, hs[1]), Some(1));
        assert_eq!(handle_at(hs, [hs[0][0] + 20.0, hs[0][1]]), None);
        // Scrolling credits: only across, never past the other margin.
        let to = dragged(b, 0, [b[2], b[1]], [0.1, 0.9, 0.1], false);
        assert!((to[0] - (0.9 - LEAST_WIDTH)).abs() < 1e-9);
        assert_eq!(to[1..], [0.9, 0.1]);
        // A timed sequence: the top too, from the frame's top.
        let at = [b[0] + 0.5 * (b[2] - b[0]), b[3] - 0.25 * (b[3] - b[1])];
        let to = dragged(b, 1, at, [0.1, 0.9, 0.1], true);
        assert!((to[1] - 0.5).abs() < 1e-9 && (to[2] - 0.25).abs() < 1e-9);
        let hs = handles(b, 0.1, 0.5, Some(0.25));
        assert!((hs[1][0] - at[0]).abs() < 1e-9 && (hs[1][1] - at[1]).abs() < 1e-9);
    }
}
