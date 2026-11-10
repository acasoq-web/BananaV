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
const CONNECTOR: &CStr = c"connector";
const COLUMNS_LEFT: &CStr = c"columnsLeftSnapping";
const COLUMNS_RIGHT: &CStr = c"columnsRightSnapping";
const POSITION: &CStr = c"position";
const FILES: &CStr = c"files";
/// The style the Styles tab shows (VEGAS's Name list).
const STYLE_SHOWN: &CStr = c"styleShown";
/// The tabs: the properties of the roll, and the styles of its lines.
const PROPERTIES_TAB: &CStr = c"propertiesTab";
const STYLES_TAB: &CStr = c"stylesTab";
/// The Name list's styles, as VEGAS names them.
const STYLE_NAMES: [&CStr; 3] = [c"Header", c"Single Item", c"Dual Item"];
const IMPORT_TEXT: &CStr = c"importText";
const IMPORT_PROPERTIES: &CStr = c"importProperties";
const EXPORT_TEXT: &CStr = c"exportText";
const EXPORT_PROPERTIES: &CStr = c"exportProperties";

/// The Position box's size, pixels.
const POSITION_SIZE: [c_int; 2] = [180, 110];
/// The least room the margins leave between them, a part of the width.
const LEAST_WIDTH: f64 = 0.02;

/// A point's size, a part of the frame's height: VEGAS's sizes are for
/// a frame 480 lines tall.
const POINT: f64 = 4.0 / 3.0 / 480.0;
/// A size here in Titles & Text's points (each a 200th of the frame's
/// height).
const TO_TITLES: f64 = POINT / titles::POINT;
/// A line's height without the room above and below it, in ems.
const LINE: f64 = 1.2;

const EFFECTS: [&CStr; 2] = [c"Scrolling Credits", c"Timed Sequence"];
const DIRECTIONS: [&CStr; 2] = [c"Bottom to Top", c"Top to Bottom"];
/// A timed sequence's ways in, VEGAS's (and its ways out, the same
/// kinds the other way).
const IN_WAYS: [&CStr; 16] = [
    c"None",
    c"Fade In",
    c"Fast Fade In",
    c"Enter from Left",
    c"Enter from Right",
    c"Enter from Top",
    c"Enter from Bottom",
    c"Slow Enter from Left",
    c"Slow Enter from Right",
    c"Slow Enter from Top",
    c"Slow Enter from Bottom",
    c"Zoom In",
    c"Wipe In Left",
    c"Wipe In Right",
    c"Wipe In Up",
    c"Wipe In Down",
];
const OUT_WAYS: [&CStr; 16] = [
    c"None",
    c"Fade Out",
    c"Fast Fade Out",
    c"Exit to Left",
    c"Exit to Right",
    c"Exit to Top",
    c"Exit to Bottom",
    c"Slow Exit to Left",
    c"Slow Exit to Right",
    c"Slow Exit to Top",
    c"Slow Exit to Bottom",
    c"Zoom Out",
    c"Wipe Out Left",
    c"Wipe Out Right",
    c"Wipe Out Up",
    c"Wipe Out Down",
];
/// How a timed sequence groups its lines into cards.
const DISPLAYS: [&CStr; 4] = [
    c"Up to Next Header",
    c"One at a Time",
    c"Header and Single Subitem",
    c"Up to Next Header or Single Item",
];
/// What fills a dual line between its two sides.
const CONNECTORS: [&CStr; 10] = [
    c"Spaces",
    c"Dots",
    c"Underline",
    c"Dashes",
    c"Centered Circles",
    c"Centered Dots",
    c"Solid Squares",
    c"Hollow Squares",
    c"Sparse Dots",
    c"Very Sparse Dots",
];
/// Each connector's text, repeated (VEGAS's: a `.veg` keeps it).
pub(crate) const CONNECTOR_TEXT: [&str; 10] = [
    "", ".", "_", "-", "\u{2022}", "\u{2219}", "\u{25AA}", "\u{25AB}", ". ", " . ",
];
const SNAPPING: [&CStr; 3] = [c"Left", c"Center", c"Right"];

/// The credits a new roll has.
const SAMPLE: &str = " Credits\nDirected by\tYour Name\nEdited by\tYour Name\n Cast\nThe Hero\tSomeone\nThe Friend\tSomeone Else\n Thanks\nEveryone who helped";

/// How long a way in or out takes, a part of its card's time: fades and
/// enters a quarter, a fast fade a tenth, slow enters a half.
fn way_length(way: i32) -> f64 {
    match way {
        1 | 3..=6 | 11..=15 => 0.25,
        2 => 0.1,
        7..=10 => 0.5,
        _ => 0.0,
    }
}

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
    /// The room above and below the line, parts of the frame's height
    /// (not above the first line, not below the last).
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
    /// Its defaults: size (points for 480 lines), bold, snapping, room
    /// above and below (parts of the frame's height).
    defaults: (f64, bool, i32, f64, f64),
}

const TITLE: Names = Names {
    group: c"titleStyle",
    label: c"Header",
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
    defaults: (20.0, true, 1, 0.0, 0.05),
};
const ITEM: Names = Names {
    group: c"itemStyle",
    label: c"Single Item",
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
    defaults: (20.0, false, 1, 0.0, 0.05),
};
const COLUMNS: Names = Names {
    group: c"columnsStyle",
    label: c"Dual Item",
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
    defaults: (20.0, false, 1, 0.0, 0.05),
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

    /// A line's height in this style, without the room above and below.
    fn height(&self, fh: f64) -> f64 {
        LINE * self.em(fh)
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
    /// How a timed sequence groups its lines ([`DISPLAYS`]).
    display: i32,
    left: f64,
    right: f64,
    top: f64,
    background: [f64; 4],
    title: Style,
    item: Style,
    columns: Style,
    /// The room between a dual line's sides, a part of the frame's width.
    center_width: f64,
    /// What fills it ([`CONNECTORS`]).
    connector: usize,
    /// The sides' snapping: 0 left, 1 center, 2 right.
    columns_snap: [i32; 2],
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
            display: p(DISPLAY).int_at(time).clamp(0, 3),
            left: p(LEFT).f64_at(time),
            right: p(RIGHT).f64_at(time),
            top: p(TOP).f64_at(time),
            background: p(BACKGROUND).rgba_at(time),
            title: Style::at(fx, &TITLE, time),
            item: Style::at(fx, &ITEM, time),
            columns: Style::at(fx, &COLUMNS, time),
            center_width: p(CENTER_WIDTH).f64_at(time),
            connector: p(CONNECTOR).int_at(time).clamp(0, 9) as usize,
            columns_snap: [
                p(COLUMNS_LEFT).int_at(time).clamp(0, 2),
                p(COLUMNS_RIGHT).int_at(time).clamp(0, 2),
            ],
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
/// hold; a tilde is a quotation mark (VEGAS's).
fn entry(s: &str) -> String {
    let t = s.trim();
    let t = if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        &t[1..t.len() - 1]
    } else {
        t
    };
    t.replace('~', "\"")
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
/// how far across it is moved, how much of its own color shows (the rest
/// the background's: a fade), its size (a zoom's, toward the frame's
/// middle) and the part of the frame a wipe shows (left, top, right,
/// bottom).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Place {
    top: f64,
    shift: f64,
    show: f32,
    scale: f64,
    clip: [f64; 4],
}

/// A timed sequence's cards (lines' places in `all`), as `display` groups
/// them: up to the next header, one line at a time, each item with its
/// header, or up to the next header or single item.
fn cards(display: i32, all: &[Line]) -> Vec<Vec<usize>> {
    let header = |i: usize| matches!(all[i], Line::Title(_));
    let mut cards: Vec<Vec<usize>> = Vec::new();
    match display {
        1 => cards.extend((0..all.len()).map(|i| vec![i])),
        2 => {
            let mut head: Option<(usize, bool)> = None;
            for i in 0..all.len() {
                if header(i) {
                    if let Some((h, false)) = head {
                        cards.push(vec![h]);
                    }
                    head = Some((i, false));
                } else {
                    cards.push(head.map(|(h, _)| h).into_iter().chain([i]).collect());
                    if let Some(h) = head.as_mut() {
                        h.1 = true;
                    }
                }
            }
            if let Some((h, false)) = head {
                cards.push(vec![h]);
            }
        }
        _ => {
            let mut card = Vec::new();
            for (i, line) in all.iter().enumerate() {
                if header(i) && !card.is_empty() {
                    cards.push(std::mem::take(&mut card));
                }
                card.push(i);
                if display == 3 && matches!(line, Line::Item(_)) {
                    cards.push(std::mem::take(&mut card));
                }
            }
            if !card.is_empty() {
                cards.push(card);
            }
        }
    }
    cards
}

/// Lines `which` stacked from `y`: each one's top, the room above a line
/// (but the first) and below it (but the last) between them; and where
/// the stack ends.
fn stacked(s: &Settings, all: &[Line], which: &[usize], mut y: f64, fh: f64) -> (Vec<f64>, f64) {
    let mut tops = Vec::with_capacity(which.len());
    for (n, &i) in which.iter().enumerate() {
        let style = s.style(&all[i]);
        if n > 0 {
            y += fh * style.above.max(0.0);
        }
        tops.push(y);
        y += style.height(fh);
        if n + 1 < which.len() {
            y += fh * style.below.max(0.0);
        }
    }
    (tops, y)
}

/// Where each shown line is at `time` (frames) of a roll `duration`
/// frames long (its media's, VEGAS's way), in a frame `fw` × `fh`.
fn places(
    s: &Settings,
    all: &[Line],
    time: f64,
    duration: f64,
    fw: f64,
    fh: f64,
) -> Vec<(usize, Place)> {
    let p = (time / duration.max(1.0)).clamp(0.0, 1.0);
    let whole = [0.0, 0.0, fw, fh];
    let place = |top: f64| Place {
        top,
        shift: 0.0,
        show: 1.0,
        scale: 1.0,
        clip: whole,
    };
    if !s.timed {
        // Stacked from the frame's bottom edge, the stack and the frame's
        // height crossed evenly over the roll (backward: the other way).
        let every: Vec<usize> = (0..all.len()).collect();
        let (tops, total) = stacked(s, all, &every, fh, fh);
        let moved = total * if s.down { 1.0 - p } else { p };
        return tops
            .iter()
            .enumerate()
            .filter_map(|(i, &top)| {
                let top = top - moved;
                let h = s.style(&all[i]).height(fh);
                (top + h > 0.0 && top < fh).then_some((i, place(top)))
            })
            .collect();
    }
    // A timed sequence: its cards one after another, each from the top
    // margin, coming in and going out its ways.
    let cards = cards(s.display, all);
    if cards.is_empty() {
        return Vec::new();
    }
    let n = cards.len();
    let at = p * n as f64;
    let (k, q) = if at >= n as f64 {
        (n - 1, 1.0)
    } else {
        (at.floor() as usize, at - at.floor())
    };
    let (tops, _) = stacked(s, all, &cards[k], (fh * s.top + 0.5).floor(), fh);
    // How far a way has gone: in, from 0 to 1 over its length from the
    // card's start; out, over its length to the card's end.
    let (way, f, coming) = if q <= 0.5 {
        let d = way_length(s.way_in);
        (s.way_in, if d > 0.0 && q <= d { q / d } else { 1.0 }, true)
    } else {
        let d = way_length(s.way_out);
        let f = if d > 0.0 && q >= 1.0 - d {
            1.0 - (1.0 - q) / d
        } else {
            0.0
        };
        (s.way_out, f, false)
    };
    // How much of the way is left to go in (or gone out).
    let v = if coming { 1.0 - f } else { f };
    let mut at = place(0.0);
    let mut down = 0.0;
    match way {
        1 | 2 => at.show = (1.0 - v) as f32,
        3 | 7 => at.shift = -v * fw,
        4 | 8 => at.shift = v * fw,
        5 | 9 => down = -v * fh,
        6 | 10 => down = v * fh,
        11 => at.scale = 1.0 - v,
        12..=15 => {
            // The part shown, from the side the wipe starts at.
            let (shown, from_end) = match (way, coming) {
                (12, true) | (13, false) => (1.0 - v, false),
                (13, true) | (12, false) => (1.0 - v, true),
                (14, true) | (15, false) => (1.0 - v, true),
                _ => (1.0 - v, false),
            };
            let horizontal = way <= 13;
            let span = if horizontal { fw } else { fh };
            let (a, b) = if from_end {
                (span * (1.0 - shown), span)
            } else {
                (0.0, span * shown)
            };
            at.clip = if horizontal {
                [a, 0.0, b, fh]
            } else {
                [0.0, a, fw, b]
            };
        }
        _ => {}
    }
    cards[k]
        .iter()
        .zip(tops)
        .map(|(&i, top)| {
            (
                i,
                Place {
                    top: top + down,
                    ..at
                },
            )
        })
        .collect()
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

/// A color (straight) mixed toward `background` (straight), `show` of it
/// left, premultiplied as VEGAS's fades are; straight again.
fn faded(c: [f64; 4], background: [f64; 4], show: f32) -> [f64; 4] {
    if show >= 1.0 {
        return c;
    }
    let w = f64::from(show.max(0.0));
    let (a, b) = (c[3], background[3]);
    let mixed = [
        c[0] * a * w + background[0] * b * (1.0 - w),
        c[1] * a * w + background[1] * b * (1.0 - w),
        c[2] * a * w + background[2] * b * (1.0 - w),
        a * w + b * (1.0 - w),
    ];
    if mixed[3] > 0.0 {
        [
            mixed[0] / mixed[3],
            mixed[1] / mixed[3],
            mixed[2] / mixed[3],
            mixed[3],
        ]
    } else {
        [0.0; 4]
    }
}

/// The pictures of `line` placed at `at` in frame `f`: headers and single
/// items against the left margin, the right one or between them; a dual
/// line's sides each in its half (the center width between them), its
/// connector repeated between them when both have text.
fn draw(s: &Settings, line: &Line, at: Place, f: Frame) -> Vec<Picture> {
    let Frame { fw, fh, k, w, h } = f;
    let style = s.style(line);
    let middle = [fw / 2.0, fh / 2.0];
    // A zoom draws everything toward the frame's middle.
    let zoom = |v: f64, m: f64| m + (v - m) * at.scale;
    let top = zoom(at.top, middle[1]);
    let size = style.size * TO_TITLES * at.scale;
    let title = |text: &str, x: f64, snap: i32| titles::Settings {
        text: text.to_string(),
        font: style.font.clone(),
        size,
        bold: style.bold,
        italic: style.italic,
        justify: snap,
        color: faded(style.color, s.background, at.show),
        scale: 1.0,
        location: [zoom(x + at.shift, middle[0]) / fw, 1.0 - top / fh],
        // The box's top: left, middle or right.
        anchor: snap.clamp(0, 2),
        background: [0.0; 4],
        // Ems of the line's size here, units of the frame's height there.
        tracking: style.tracking * size * titles::POINT / titles::UNIT,
        kerning: style.kerning,
        line_spacing: 1.0,
        outline_width: 0.0,
        shadow: false,
        ..titles::Settings::default()
    };
    // The margins on whole pixels, as VEGAS's.
    let (left, right) = ((fw * s.left + 0.5).floor(), (fw * s.right + 0.5).floor());
    let center = (left + right) / 2.0;
    let gap = s.center_width.max(0.0) * fw;
    let mut pictures: Vec<Picture> = Vec::new();
    // Draws `t`: where its pixels begin across, and where they end.
    let mut draw = |t: &titles::Settings| -> Option<(i64, i64)> {
        if t.text.trim().is_empty() {
            return None;
        }
        let p = titles::picture(t, fw, fh, k, w, h)?;
        let across = (p.region.x, p.region.x + p.region.w as i64);
        pictures.push(p);
        Some(across)
    };
    match line {
        Line::Title(t) | Line::Item(t) => {
            let x = match style.snap {
                0 => left,
                2 => right,
                _ => center,
            };
            draw(&title(t, x, style.snap.clamp(0, 2)));
        }
        Line::Columns(a, b) => {
            let [ls, rs] = s.columns_snap;
            let lx = match ls {
                0 => left,
                2 => center - gap / 2.0,
                _ => (left + center) / 2.0 - gap / 4.0,
            };
            let rx = match rs {
                0 => center + gap / 2.0,
                2 => right,
                _ => (center + right) / 2.0 + gap / 4.0,
            };
            let lp = draw(&title(a, lx, ls));
            let rp = draw(&title(b, rx, rs));
            // The connector from the left side's end to the right side's
            // start, cut there.
            let filler = CONNECTOR_TEXT[s.connector.min(9)];
            if let (Some(lp), Some(rp), false) = (lp, rp, filler.is_empty()) {
                let (from, to) = (lp.1 as f64, rp.0 as f64);
                if to > from {
                    let em = style.em(fh) * k[0];
                    let n = ((to - from) / (em * 0.2).max(1.0)).ceil() as usize + 1;
                    let x = from / k[0] - at.shift;
                    let mut t = title(&filler.repeat(n.min(4000)), x, 0);
                    t.location[0] = (from / k[0]) / fw;
                    if let Some(mut p) = titles::picture(&t, fw, fh, k, w, h) {
                        let end = to as i64;
                        for j in 0..p.region.h {
                            for i in 0..p.region.w {
                                if p.region.x + i as i64 >= end {
                                    p.rgba[j * p.region.w + i] = [0.0; 4];
                                }
                            }
                        }
                        pictures.push(p);
                    }
                }
            }
        }
    }
    // A wipe shows only its part of the frame.
    let [cx0, cy0, cx1, cy1] = at.clip;
    if at.clip != [0.0, 0.0, fw, fh] {
        let (x0, x1) = (cx0 * k[0], cx1 * k[0]);
        let (y0, y1) = (cy0 * k[1], cy1 * k[1]);
        for p in &mut pictures {
            for j in 0..p.region.h {
                let y = (p.region.y + j as i64) as f64 + 0.5;
                for i in 0..p.region.w {
                    let x = (p.region.x + i as i64) as f64 + 0.5;
                    if x < x0 || x >= x1 || y < y0 || y >= y1 {
                        p.rgba[j * p.region.w + i] = [0.0; 4];
                    }
                }
            }
        }
    }
    pictures
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
        &IN_WAYS,
        0,
        None,
        c"How each card of a timed sequence comes in.",
    );
    choice(
        WAY_OUT,
        c"Out",
        &OUT_WAYS,
        0,
        None,
        c"How each card of a timed sequence goes out.",
    );
    choice(
        DISPLAY,
        c"Display",
        &DISPLAYS,
        0,
        None,
        c"Which lines a timed sequence shows together.",
    );
    // VEGAS's Styles tab: the style shown, picked by name.
    let p = fx.define_param(kOfxParamTypeChoice, STYLE_SHOWN, c"Name");
    p.set_strs(kOfxParamPropChoiceOption, &STYLE_NAMES);
    p.set_int(kOfxParamPropDefault, 0);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(
        kOfxParamPropHint,
        c"The style shown below: the headers', the single items' or the dual items' (two columns).",
    );
    color(
        BACKGROUND,
        c"Background color",
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
        g.set_int(kOfxParamPropGroupOpen, 1);
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
            c"The font's size in points, for a frame 480 lines tall (the same part of other frames).",
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
            choice(
                COLUMNS_LEFT,
                c"Left Snapping",
                &SNAPPING,
                1,
                Some(n.group),
                c"The left sides against the left margin, in the middle of their half or against the center.",
            );
            choice(
                COLUMNS_RIGHT,
                c"Right Snapping",
                &SNAPPING,
                1,
                Some(n.group),
                c"The right sides against the center, in the middle of their half or against the right margin.",
            );
            number(
                CENTER_WIDTH,
                c"Center Width",
                [0.0, 0.5, 0.05],
                Some(n.group),
                c"The room between the two columns, a part of the frame's width.",
            );
            choice(
                CONNECTOR,
                c"Connect Sides With",
                &CONNECTORS,
                0,
                Some(n.group),
                c"What fills the room between the two sides.",
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
            [0.0, 1.0, above],
            Some(n.group),
            c"The room above each line but the first, a part of the frame's height.",
        );
        number(
            n.below,
            c"Space Below",
            [0.0, 1.0, below],
            Some(n.group),
            c"The room below each line but the last, a part of the frame's height.",
        );
    }
    // VEGAS's tabs: the roll's properties and its lines' styles (the
    // credits and their files stay above them).
    for (name, label, children) in [
        (
            PROPERTIES_TAB,
            c"Properties",
            &[EFFECT, POSITION, DIRECTION, WAY_IN, WAY_OUT, DISPLAY][..],
        ),
        (
            STYLES_TAB,
            c"Styles",
            &[
                STYLE_SHOWN,
                BACKGROUND,
                TITLE.group,
                ITEM.group,
                COLUMNS.group,
            ][..],
        ),
    ] {
        let p = fx.define_param(kOfxParamTypePage, name, label);
        p.set_strs(kOfxParamPropPageChild, children);
        p.set_int(bv_ofx_sys::extensions::kBananaVParamPropTab, 1);
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
    // The background premultiplied: in an 8-bit project each byte cut, as
    // VEGAS's.
    let eight_bit = args.int(bv_ofx_sys::extensions::kBananaVImageEffectPropEightBit, 0) != 0;
    let back = {
        let a = s.background[3].clamp(0.0, 1.0);
        let c = |v: f64| {
            if eight_bit {
                ((a * v * 255.0) as i32).clamp(0, 255) as f32 / 255.0
            } else {
                (v * a) as f32
            }
        };
        [
            c(s.background[0]),
            c(s.background[1]),
            c(s.background[2]),
            c(1.0),
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
    // The Styles tab shows the style its Name list picks.
    let shown = fx.param(STYLE_SHOWN).int_at(time).clamp(0, 2) as usize;
    for (i, n) in [&TITLE, &ITEM, &COLUMNS].into_iter().enumerate() {
        fx.param(n.group).set_secret(i != shown);
    }
    // The sides against the margins: no room of their own between them.
    let apart =
        fx.param(COLUMNS_LEFT).int_at(time) == 0 && fx.param(COLUMNS_RIGHT).int_at(time) == 2;
    fx.param(CENTER_WIDTH).set_enabled(!apart);
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
        (CONNECTOR, Kind::Int),
        (COLUMNS_LEFT, Kind::Int),
        (COLUMNS_RIGHT, Kind::Int),
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
/// VEGAS's eight: the effect, the margins, the ways in and out, the
/// background and the styles (the credits stay as they are).
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    let text = |t: &str| PresetValue::Text(t.to_string());
    let p = |name: &'static str, values: Vec<(&'static str, PresetValue)>| Preset { name, values };
    vec![
        p(
            "Plain Scrolling on Black",
            vec![
                ("effect", num(&[0.0])),
                ("left", num(&[0.1])),
                ("right", num(&[0.9])),
                ("top", num(&[0.3])),
                ("in", num(&[0.0])),
                ("out", num(&[0.0])),
                ("display", num(&[0.0])),
                ("connector", num(&[0.0])),
                ("background", num(&[0.0, 0.0, 0.0, 1.0])),
                ("direction", num(&[0.0])),
                ("centerWidth", num(&[0.05])),
                ("titleFont", text("Arial")),
                ("titleSize", num(&[20.0])),
                ("titleBold", num(&[1.0])),
                ("titleItalic", num(&[0.0])),
                ("titleColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("titleSnapping", num(&[1.0])),
                ("titleKerning", num(&[0.0])),
                ("titleTracking", num(&[0.0])),
                ("titleSpaceAbove", num(&[0.0])),
                ("titleSpaceBelow", num(&[0.05])),
                ("itemFont", text("Arial")),
                ("itemSize", num(&[20.0])),
                ("itemBold", num(&[0.0])),
                ("itemItalic", num(&[0.0])),
                ("itemColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("itemSnapping", num(&[1.0])),
                ("itemKerning", num(&[0.0])),
                ("itemTracking", num(&[0.0])),
                ("itemSpaceAbove", num(&[0.0])),
                ("itemSpaceBelow", num(&[0.05])),
                ("columnsFont", text("Arial")),
                ("columnsSize", num(&[20.0])),
                ("columnsBold", num(&[0.0])),
                ("columnsItalic", num(&[0.0])),
                ("columnsColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("columnsKerning", num(&[0.0])),
                ("columnsTracking", num(&[0.0])),
                ("columnsSpaceAbove", num(&[0.0])),
                ("columnsSpaceBelow", num(&[0.05])),
                ("columnsLeftSnapping", num(&[1.0])),
                ("columnsRightSnapping", num(&[1.0])),
            ],
        ),
        p(
            "Scrolling on Transparent",
            vec![
                ("effect", num(&[0.0])),
                ("left", num(&[0.1])),
                ("right", num(&[0.9])),
                ("top", num(&[0.3])),
                ("in", num(&[0.0])),
                ("out", num(&[0.0])),
                ("display", num(&[0.0])),
                ("connector", num(&[1.0])),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("direction", num(&[0.0])),
                ("centerWidth", num(&[0.05])),
                ("titleFont", text("Arial")),
                ("titleSize", num(&[36.0])),
                ("titleBold", num(&[1.0])),
                ("titleItalic", num(&[0.0])),
                ("titleColor", num(&[1.0, 0.0, 0.0, 1.0])),
                ("titleSnapping", num(&[1.0])),
                ("titleKerning", num(&[0.0])),
                ("titleTracking", num(&[0.0])),
                ("titleSpaceAbove", num(&[0.0])),
                ("titleSpaceBelow", num(&[0.1])),
                ("itemFont", text("Verdana")),
                ("itemSize", num(&[24.0])),
                ("itemBold", num(&[0.0])),
                ("itemItalic", num(&[0.0])),
                ("itemColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("itemSnapping", num(&[1.0])),
                ("itemKerning", num(&[0.0])),
                ("itemTracking", num(&[0.0])),
                ("itemSpaceAbove", num(&[0.0])),
                ("itemSpaceBelow", num(&[0.05])),
                ("columnsFont", text("Verdana")),
                ("columnsSize", num(&[20.0])),
                ("columnsBold", num(&[0.0])),
                ("columnsItalic", num(&[0.0])),
                ("columnsColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("columnsKerning", num(&[0.0])),
                ("columnsTracking", num(&[0.0])),
                ("columnsSpaceAbove", num(&[0.0])),
                ("columnsSpaceBelow", num(&[0.01])),
                ("columnsLeftSnapping", num(&[0.0])),
                ("columnsRightSnapping", num(&[2.0])),
            ],
        ),
        p(
            "Timed, Fade on Black",
            vec![
                ("effect", num(&[1.0])),
                ("left", num(&[0.1])),
                ("right", num(&[0.9])),
                ("top", num(&[0.3])),
                ("in", num(&[1.0])),
                ("out", num(&[1.0])),
                ("display", num(&[2.0])),
                ("connector", num(&[8.0])),
                ("background", num(&[0.0, 0.0, 0.0, 1.0])),
                ("direction", num(&[0.0])),
                ("centerWidth", num(&[0.05])),
                ("titleFont", text("Arial")),
                ("titleSize", num(&[36.0])),
                ("titleBold", num(&[1.0])),
                ("titleItalic", num(&[0.0])),
                ("titleColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("titleSnapping", num(&[1.0])),
                ("titleKerning", num(&[0.0])),
                ("titleTracking", num(&[0.0])),
                ("titleSpaceAbove", num(&[0.0])),
                ("titleSpaceBelow", num(&[0.1])),
                ("itemFont", text("Arial")),
                ("itemSize", num(&[24.0])),
                ("itemBold", num(&[1.0])),
                ("itemItalic", num(&[0.0])),
                (
                    "itemColor",
                    num(&[0.800000011920929, 0.800000011920929, 0.800000011920929, 1.0]),
                ),
                ("itemSnapping", num(&[1.0])),
                ("itemKerning", num(&[0.0])),
                ("itemTracking", num(&[0.0])),
                ("itemSpaceAbove", num(&[0.0])),
                ("itemSpaceBelow", num(&[0.05])),
                ("columnsFont", text("Arial")),
                ("columnsSize", num(&[24.0])),
                ("columnsBold", num(&[0.0])),
                ("columnsItalic", num(&[0.0])),
                (
                    "columnsColor",
                    num(&[0.699999988079071, 0.699999988079071, 0.699999988079071, 1.0]),
                ),
                ("columnsKerning", num(&[0.0])),
                ("columnsTracking", num(&[0.0])),
                ("columnsSpaceAbove", num(&[0.0])),
                ("columnsSpaceBelow", num(&[0.01])),
                ("columnsLeftSnapping", num(&[1.0])),
                ("columnsRightSnapping", num(&[1.0])),
            ],
        ),
        p(
            "Timed, Zoom In/Out on Blue",
            vec![
                ("effect", num(&[1.0])),
                ("left", num(&[0.1])),
                ("right", num(&[0.9])),
                ("top", num(&[0.3])),
                ("in", num(&[11.0])),
                ("out", num(&[11.0])),
                ("display", num(&[0.0])),
                ("connector", num(&[1.0])),
                ("background", num(&[0.0, 0.0, 1.0, 0.5])),
                ("direction", num(&[0.0])),
                ("centerWidth", num(&[0.05])),
                ("titleFont", text("Arial")),
                ("titleSize", num(&[36.0])),
                ("titleBold", num(&[1.0])),
                ("titleItalic", num(&[0.0])),
                (
                    "titleColor",
                    num(&[0.800000011920929, 0.800000011920929, 1.0, 1.0]),
                ),
                ("titleSnapping", num(&[1.0])),
                ("titleKerning", num(&[0.0])),
                ("titleTracking", num(&[0.0])),
                ("titleSpaceAbove", num(&[0.0])),
                ("titleSpaceBelow", num(&[0.1])),
                ("itemFont", text("Verdana")),
                ("itemSize", num(&[24.0])),
                ("itemBold", num(&[1.0])),
                ("itemItalic", num(&[0.0])),
                ("itemColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("itemSnapping", num(&[1.0])),
                ("itemKerning", num(&[0.0])),
                ("itemTracking", num(&[0.0])),
                ("itemSpaceAbove", num(&[0.0])),
                ("itemSpaceBelow", num(&[0.05])),
                ("columnsFont", text("Times New Roman")),
                ("columnsSize", num(&[24.0])),
                ("columnsBold", num(&[0.0])),
                ("columnsItalic", num(&[0.0])),
                ("columnsColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("columnsKerning", num(&[0.0])),
                ("columnsTracking", num(&[0.0])),
                ("columnsSpaceAbove", num(&[0.0])),
                ("columnsSpaceBelow", num(&[0.01])),
                ("columnsLeftSnapping", num(&[2.0])),
                ("columnsRightSnapping", num(&[0.0])),
            ],
        ),
        p(
            "Scrolling, Left Side, on Semi-Black",
            vec![
                ("effect", num(&[0.0])),
                ("left", num(&[0.05])),
                ("right", num(&[0.65])),
                ("top", num(&[0.3])),
                ("in", num(&[0.0])),
                ("out", num(&[0.0])),
                ("display", num(&[0.0])),
                ("connector", num(&[1.0])),
                ("background", num(&[0.0, 0.0, 0.0, 0.5])),
                ("direction", num(&[0.0])),
                ("centerWidth", num(&[0.05])),
                ("titleFont", text("Verdana")),
                ("titleSize", num(&[36.0])),
                ("titleBold", num(&[1.0])),
                ("titleItalic", num(&[0.0])),
                (
                    "titleColor",
                    num(&[0.800000011920929, 1.0, 0.800000011920929, 1.0]),
                ),
                ("titleSnapping", num(&[1.0])),
                ("titleKerning", num(&[0.0])),
                ("titleTracking", num(&[0.0])),
                ("titleSpaceAbove", num(&[0.0])),
                ("titleSpaceBelow", num(&[0.1])),
                ("itemFont", text("Arial")),
                ("itemSize", num(&[20.0])),
                ("itemBold", num(&[0.0])),
                ("itemItalic", num(&[0.0])),
                ("itemColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("itemSnapping", num(&[1.0])),
                ("itemKerning", num(&[0.0])),
                ("itemTracking", num(&[0.0])),
                ("itemSpaceAbove", num(&[0.0])),
                ("itemSpaceBelow", num(&[0.05])),
                ("columnsFont", text("Arial")),
                ("columnsSize", num(&[16.0])),
                ("columnsBold", num(&[0.0])),
                ("columnsItalic", num(&[0.0])),
                ("columnsColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("columnsKerning", num(&[0.0])),
                ("columnsTracking", num(&[0.0])),
                ("columnsSpaceAbove", num(&[0.0])),
                ("columnsSpaceBelow", num(&[0.01])),
                ("columnsLeftSnapping", num(&[2.0])),
                ("columnsRightSnapping", num(&[0.0])),
            ],
        ),
        p(
            "Timed, Scroll Right, on Frosty White",
            vec![
                ("effect", num(&[1.0])),
                ("left", num(&[0.05])),
                ("right", num(&[0.95])),
                ("top", num(&[0.3])),
                ("in", num(&[3.0])),
                ("out", num(&[4.0])),
                ("display", num(&[3.0])),
                ("connector", num(&[3.0])),
                ("background", num(&[1.0, 1.0, 1.0, 0.8500000238418579])),
                ("direction", num(&[0.0])),
                ("centerWidth", num(&[0.05])),
                ("titleFont", text("Verdana")),
                ("titleSize", num(&[48.0])),
                ("titleBold", num(&[1.0])),
                ("titleItalic", num(&[1.0])),
                ("titleColor", num(&[0.0, 0.0, 1.0, 0.8500000238418579])),
                ("titleSnapping", num(&[1.0])),
                ("titleKerning", num(&[0.0])),
                ("titleTracking", num(&[0.0])),
                ("titleSpaceAbove", num(&[0.0])),
                ("titleSpaceBelow", num(&[0.1])),
                ("itemFont", text("Arial")),
                ("itemSize", num(&[24.0])),
                ("itemBold", num(&[0.0])),
                ("itemItalic", num(&[1.0])),
                ("itemColor", num(&[0.0, 0.0, 0.0, 0.8500000238418579])),
                ("itemSnapping", num(&[1.0])),
                ("itemKerning", num(&[0.0])),
                ("itemTracking", num(&[0.0])),
                ("itemSpaceAbove", num(&[0.0])),
                ("itemSpaceBelow", num(&[0.05])),
                ("columnsFont", text("Arial")),
                ("columnsSize", num(&[24.0])),
                ("columnsBold", num(&[0.0])),
                ("columnsItalic", num(&[0.0])),
                ("columnsColor", num(&[0.0, 0.0, 0.0, 0.8500000238418579])),
                ("columnsKerning", num(&[0.0])),
                ("columnsTracking", num(&[0.0])),
                ("columnsSpaceAbove", num(&[0.0])),
                ("columnsSpaceBelow", num(&[0.01])),
                ("columnsLeftSnapping", num(&[2.0])),
                ("columnsRightSnapping", num(&[0.0])),
            ],
        ),
        p(
            "Timed, Wipe In/Out, on Black",
            vec![
                ("effect", num(&[1.0])),
                ("left", num(&[0.1])),
                ("right", num(&[0.9])),
                ("top", num(&[0.1])),
                ("in", num(&[13.0])),
                ("out", num(&[13.0])),
                ("display", num(&[0.0])),
                ("connector", num(&[1.0])),
                ("background", num(&[0.0, 0.0, 0.0, 1.0])),
                ("direction", num(&[0.0])),
                ("centerWidth", num(&[0.05])),
                ("titleFont", text("Arial")),
                ("titleSize", num(&[42.0])),
                ("titleBold", num(&[1.0])),
                ("titleItalic", num(&[1.0])),
                ("titleColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("titleSnapping", num(&[1.0])),
                ("titleKerning", num(&[0.0])),
                ("titleTracking", num(&[0.0])),
                ("titleSpaceAbove", num(&[0.0])),
                ("titleSpaceBelow", num(&[0.1])),
                ("itemFont", text("Verdana")),
                ("itemSize", num(&[24.0])),
                ("itemBold", num(&[0.0])),
                ("itemItalic", num(&[1.0])),
                ("itemColor", num(&[1.0, 0.0, 0.0, 1.0])),
                ("itemSnapping", num(&[1.0])),
                ("itemKerning", num(&[0.0])),
                ("itemTracking", num(&[0.0])),
                ("itemSpaceAbove", num(&[0.0])),
                ("itemSpaceBelow", num(&[0.05])),
                ("columnsFont", text("Verdana")),
                ("columnsSize", num(&[20.0])),
                ("columnsBold", num(&[0.0])),
                ("columnsItalic", num(&[0.0])),
                (
                    "columnsColor",
                    num(&[0.800000011920929, 0.800000011920929, 0.800000011920929, 1.0]),
                ),
                ("columnsKerning", num(&[0.0])),
                ("columnsTracking", num(&[0.0])),
                ("columnsSpaceAbove", num(&[0.0])),
                ("columnsSpaceBelow", num(&[0.01])),
                ("columnsLeftSnapping", num(&[0.0])),
                ("columnsRightSnapping", num(&[2.0])),
            ],
        ),
        p(
            "Backward Scrolling, Primary Colors",
            vec![
                ("effect", num(&[0.0])),
                ("left", num(&[0.1])),
                ("right", num(&[0.9])),
                ("top", num(&[0.3])),
                ("in", num(&[0.0])),
                ("out", num(&[0.0])),
                ("display", num(&[0.0])),
                ("connector", num(&[1.0])),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("direction", num(&[1.0])),
                ("centerWidth", num(&[0.05])),
                ("titleFont", text("Verdana")),
                ("titleSize", num(&[36.0])),
                ("titleBold", num(&[1.0])),
                ("titleItalic", num(&[0.0])),
                ("titleColor", num(&[0.0, 0.0, 1.0, 1.0])),
                ("titleSnapping", num(&[1.0])),
                ("titleKerning", num(&[0.0])),
                ("titleTracking", num(&[0.0])),
                ("titleSpaceAbove", num(&[0.0])),
                ("titleSpaceBelow", num(&[0.1])),
                ("itemFont", text("Verdana")),
                ("itemSize", num(&[24.0])),
                ("itemBold", num(&[0.0])),
                ("itemItalic", num(&[0.0])),
                ("itemColor", num(&[0.0, 1.0, 0.0, 1.0])),
                ("itemSnapping", num(&[1.0])),
                ("itemKerning", num(&[0.0])),
                ("itemTracking", num(&[0.0])),
                ("itemSpaceAbove", num(&[0.0])),
                ("itemSpaceBelow", num(&[0.01])),
                ("columnsFont", text("Verdana")),
                ("columnsSize", num(&[20.0])),
                ("columnsBold", num(&[0.0])),
                ("columnsItalic", num(&[0.0])),
                ("columnsColor", num(&[1.0, 0.0, 0.0, 1.0])),
                ("columnsKerning", num(&[0.0])),
                ("columnsTracking", num(&[0.0])),
                ("columnsSpaceAbove", num(&[0.0])),
                ("columnsSpaceBelow", num(&[0.03])),
                ("columnsLeftSnapping", num(&[0.0])),
                ("columnsRightSnapping", num(&[2.0])),
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
            below: 0.05,
        }
    }

    fn settings(timed: bool) -> Settings {
        Settings {
            text: SAMPLE.into(),
            timed,
            down: false,
            way_in: 1,
            way_out: 1,
            display: 1,
            left: 0.1,
            right: 0.9,
            top: 0.1,
            background: [0.0; 4],
            title: style(20.0),
            item: style(20.0),
            columns: style(20.0),
            center_width: 0.05,
            connector: 0,
            columns_snap: [1, 1],
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
    fn scrolling_crosses_the_stack_and_the_frame_as_measured() {
        // VEGAS's sample at 640 × 360: four lines 24 pixels tall (Arial 20
        // points for 480 lines), 18 below each but the last: the stack is
        // 150 pixels, the roll 510; the title's top at 324 a tenth of the
        // way, at 105 halfway.
        let s = Settings {
            text: " Title Text\nSubitem Text\nItem Left\tItem Right\nLonger Item Left\tLonger Item Right".into(),
            ..settings(false)
        };
        let all = lines(&s.text);
        let (fw, fh) = (640.0, 360.0);
        let top = |t: f64| {
            places(&s, &all, t, 300.0, fw, fh)
                .iter()
                .find(|(i, _)| *i == 0)
                .map(|(_, p)| p.top)
        };
        assert!((top(30.0).unwrap() - (360.0 - 51.0)).abs() < 1e-9);
        assert!((top(150.0).unwrap() - (360.0 - 255.0)).abs() < 1e-9);
    }

    #[test]
    fn a_timed_sequence_shows_its_cards_in_turn() {
        let s = settings(true);
        let all = lines(&s.text);
        let (fw, fh) = (1920.0, 1080.0);
        // One at a time: as many cards as lines.
        let n = cards(1, &all).len();
        assert_eq!(n, all.len());
        let d = 100.0 * n as f64;
        // The middle of each card's time: its line, its own color, at the
        // top margin (on a whole pixel).
        for g in 0..n {
            let v = places(&s, &all, d * (g as f64 + 0.5) / n as f64, d, fw, fh);
            assert_eq!(v[0].0, g);
            assert!((v[0].1.show - 1.0).abs() < 1e-6);
            assert!((v[0].1.top - (0.1 * fh + 0.5).floor()).abs() < 1e-6);
        }
        // Fading in at a card's start.
        let v = places(&s, &all, 1.0, d, fw, fh);
        assert!(v[0].1.show < 0.5);
        // Up to the next header: a header and what follows it.
        let groups = cards(0, &all);
        assert!(
            groups
                .iter()
                .all(|c| matches!(all[c[0]], Line::Title(_)) || c[0] == 0)
        );
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
