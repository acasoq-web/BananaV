//! Titles & Text: VEGAS's text generator, as its Help describes it — text
//! in the computer's fonts ([`crate::fonts`]), its size, bold, italic and
//! justification, its color; scaled, placed by a point of its box (the
//! anchor) at a location in the frame (0–1 from the lower left); behind it
//! a background color, around it an outline, under it a shadow (offset and
//! blurred); its letters spaced (tracking, after the font's own kerning,
//! [`crate::kerning`]) and its lines too; parts of the text selected set
//! in a font, size, bold or italic of their own (its [`Run`]s, which the
//! host writes: BananaV's `kBananaVParamPropRichText`). On the
//! preview its box can be dragged to move it and its corners to scale it;
//! the anchor shows as an empty square.
//!
//! Sizes count points at a frame 1080 lines tall (a point 4⁄3 of a pixel
//! there), the same part of other frames; tracking, the outline's width
//! and the shadow's offsets and blur count the em. VEGAS's own defaults
//! and its Animation list wait for its pictures.
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::fonts;
use crate::kerning::Kerning;
use crate::ofx::{Draw, Fx, Props, for_rows, interact_props, is, redraw, suites};
use crate::text::Coverage;
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.TitlesText";
const _: () = assert!(ID.to_bytes().len() == crate::TITLES_ID.len());

const TEXT: &CStr = c"text";
/// The text's runs (hidden).
const STYLES: &CStr = c"styles";
const FONT: &CStr = c"font";
const SIZE: &CStr = c"size";
const BOLD: &CStr = c"bold";
const ITALIC: &CStr = c"italic";
const JUSTIFY: &CStr = c"justification";
const COLOR: &CStr = c"textColor";
const SCALE: &CStr = c"scale";
const LOCATION: &CStr = c"location";
const ANCHOR: &CStr = c"anchor";
const ADVANCED: &CStr = c"advanced";
const BACKGROUND: &CStr = c"background";
const TRACKING: &CStr = c"tracking";
const LINE_SPACING: &CStr = c"lineSpacing";
const OUTLINE: &CStr = c"outline";
const OUTLINE_WIDTH: &CStr = c"outlineWidth";
const OUTLINE_COLOR: &CStr = c"outlineColor";
const SHADOW: &CStr = c"shadow";
const SHADOW_ON: &CStr = c"shadowEnable";
const SHADOW_COLOR: &CStr = c"shadowColor";
const SHADOW_X: &CStr = c"shadowOffsetX";
const SHADOW_Y: &CStr = c"shadowOffsetY";
const SHADOW_BLUR: &CStr = c"shadowBlur";

/// The text's justification, as the text box's buttons set it.
const JUSTIFICATIONS: [&CStr; 3] = [c"Left", c"Center", c"Right"];
/// The anchor points: the box's corners, sides' middles and middle.
const ANCHORS: [&CStr; 9] = [
    c"Top Left",
    c"Top Center",
    c"Top Right",
    c"Center Left",
    c"Center",
    c"Center Right",
    c"Bottom Left",
    c"Bottom Center",
    c"Bottom Right",
];

/// The text a new title has.
pub(crate) const SAMPLE: &str = "Sample Text";

/// A point's size, a part of the frame's height: 4⁄3 of a pixel at 1080
/// lines (96 points to the inch).
const POINT: f64 = 4.0 / 3.0 / 1080.0;
/// How far a missing bold face's letters are thickened each way (ems), and
/// how far a missing italic's lean (across per up).
const EMBOLDEN: f64 = 0.022;
const SLANT: f64 = 0.21;

/// The settings at a time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    pub text: String,
    pub font: String,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    /// 0 left, 1 center, 2 right.
    pub justify: i32,
    pub color: [f64; 4],
    pub scale: f64,
    /// 0–1 across and up the frame.
    pub location: [f64; 2],
    /// [`ANCHORS`]' index.
    pub anchor: i32,
    pub background: [f64; 4],
    /// Between letters, ems.
    pub tracking: f64,
    /// The font's own kerning between pairs of letters.
    pub kerning: bool,
    /// Lines' distance, times the font's.
    pub line_spacing: f64,
    /// Ems; 0: none.
    pub outline_width: f64,
    pub outline_color: [f64; 4],
    pub shadow: bool,
    pub shadow_color: [f64; 4],
    /// Ems, right and up.
    pub shadow_offset: [f64; 2],
    /// Ems.
    pub shadow_blur: f64,
    /// Parts of the text in a style of their own.
    pub runs: Vec<Run>,
}

/// A part of the text in a style of its own — Titles & Text's selected
/// text: its characters `start..end` (in the text as typed) and the
/// font, size, bold or italic it sets.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Run {
    pub start: usize,
    pub end: usize,
    pub font: Option<String>,
    pub size: Option<f64>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
}

/// The runs the styles parameter holds: a line each, `start end
/// name=value;…` (the later over the earlier).
pub(crate) fn parse_runs(text: &str) -> Vec<Run> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, ' ');
            let start = parts.next()?.parse().ok()?;
            let end = parts.next()?.parse().ok()?;
            let mut run = Run {
                start,
                end,
                ..Run::default()
            };
            let on = |v: &str| v.parse::<f64>().ok().map(|b| b >= 0.5);
            for pair in parts.next().unwrap_or("").split(';') {
                let Some((k, v)) = pair.split_once('=') else {
                    continue;
                };
                match k.trim() {
                    "font" => run.font = Some(v.trim().to_string()),
                    "size" => run.size = v.trim().parse().ok(),
                    "bold" => run.bold = on(v.trim()),
                    "italic" => run.italic = on(v.trim()),
                    _ => {}
                }
            }
            (end > start).then_some(run)
        })
        .collect()
}

/// A character's style: its font, size, bold and italic.
#[derive(Clone, Debug, PartialEq)]
struct StyleKey {
    font: String,
    size: f64,
    bold: bool,
    italic: bool,
}

/// `s`'s text as it is laid out — a carriage return before a line feed
/// left out, a tab four spaces —, each character with its style's place
/// among the styles found (the title's own first).
fn styled_text(s: &Settings) -> (Vec<(char, usize)>, Vec<StyleKey>) {
    let base = StyleKey {
        font: s.font.clone(),
        size: s.size,
        bold: s.bold,
        italic: s.italic,
    };
    let mut keys = vec![base.clone()];
    let mut out = Vec::new();
    let chars: Vec<char> = s.text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == '\r' && chars.get(i + 1) == Some(&'\n') {
            continue;
        }
        let mut k = base.clone();
        for r in s.runs.iter().filter(|r| r.start <= i && i < r.end) {
            if let Some(f) = &r.font {
                k.font = f.clone();
            }
            if let Some(v) = r.size {
                k.size = v;
            }
            if let Some(b) = r.bold {
                k.bold = b;
            }
            if let Some(b) = r.italic {
                k.italic = b;
            }
        }
        let st = match keys.iter().position(|x| *x == k) {
            Some(p) => p,
            None => {
                keys.push(k);
                keys.len() - 1
            }
        };
        if c == '\t' {
            out.extend(std::iter::repeat_n((' ', st), 4));
        } else {
            out.push((c, st));
        }
    }
    (out, keys)
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            text: SAMPLE.into(),
            font: String::new(),
            size: 48.0,
            bold: false,
            italic: false,
            justify: 1,
            color: [1.0, 1.0, 1.0, 1.0],
            scale: 1.0,
            location: [0.5, 0.5],
            anchor: 4,
            background: [0.0, 0.0, 0.0, 0.0],
            tracking: 0.0,
            kerning: true,
            line_spacing: 1.0,
            outline_width: 0.0,
            outline_color: [0.0, 0.0, 0.0, 1.0],
            shadow: false,
            shadow_color: [0.0, 0.0, 0.0, 0.75],
            shadow_offset: [0.06, -0.06],
            shadow_blur: 0.05,
            runs: Vec::new(),
        }
    }
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        let p = |name| fx.param(name);
        Settings {
            text: p(TEXT).text_at(time),
            font: p(FONT).text_at(time),
            size: p(SIZE).f64_at(time),
            bold: p(BOLD).int_at(time) != 0,
            italic: p(ITALIC).int_at(time) != 0,
            justify: p(JUSTIFY).int_at(time),
            color: p(COLOR).rgba_at(time),
            scale: p(SCALE).f64_at(time),
            location: p(LOCATION).xy_at(time),
            anchor: p(ANCHOR).int_at(time),
            background: p(BACKGROUND).rgba_at(time),
            tracking: p(TRACKING).f64_at(time),
            // Titles & Text always kerns.
            kerning: true,
            line_spacing: p(LINE_SPACING).f64_at(time),
            outline_width: p(OUTLINE_WIDTH).f64_at(time),
            outline_color: p(OUTLINE_COLOR).rgba_at(time),
            shadow: p(SHADOW_ON).int_at(time) != 0,
            shadow_color: p(SHADOW_COLOR).rgba_at(time),
            shadow_offset: [p(SHADOW_X).f64_at(time), p(SHADOW_Y).f64_at(time)],
            shadow_blur: p(SHADOW_BLUR).f64_at(time),
            runs: parse_runs(&p(STYLES).text_at(time)),
        }
    }

    /// The em, in the frame's units (its height `h`).
    fn em(&self, h: f64) -> f64 {
        (self.size * POINT * h * self.scale).max(0.0)
    }
}

/// The face the settings ask for, its file's bytes, and what is made up
/// (`embolden`, `slant`).
#[derive(Clone)]
struct Font {
    data: std::sync::Arc<Vec<u8>>,
    index: u32,
    embolden: bool,
    slant: bool,
}

impl Font {
    fn of(s: &Settings) -> Option<Font> {
        let (face, embolden, slant) = fonts::find(&s.font, s.bold, s.italic)?;
        Some(Font {
            data: fonts::data(&face.path)?,
            index: face.index,
            embolden,
            slant,
        })
    }

    /// A run's face.
    fn of_key(k: &StyleKey) -> Option<Font> {
        let (face, embolden, slant) = fonts::find(&k.font, k.bold, k.italic)?;
        Some(Font {
            data: fonts::data(&face.path)?,
            index: face.index,
            embolden,
            slant,
        })
    }

    fn face(&self) -> Option<FontRef<'_>> {
        FontRef::from_index(&self.data, self.index).ok()
    }
}

/// `s`'s faces (its own, then its runs') and its text laid out with them
/// in a frame `w` × `h`.
fn laid_out(s: &Settings, w: f64, h: f64) -> Option<(Vec<Font>, Layout)> {
    let (chars, keys) = styled_text(s);
    let base = Font::of(s)?;
    let fonts: Vec<Font> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| match i {
            0 => base.clone(),
            _ => Font::of_key(k).unwrap_or_else(|| base.clone()),
        })
        .collect();
    let faces = fonts.iter().map(Font::face).collect::<Option<Vec<_>>>()?;
    let sizes: Vec<f64> = keys.iter().map(|k| k.size).collect();
    let l = layout_styled(s, &faces, &sizes, &chars, w, h);
    drop(faces);
    Some((fonts, l))
}

/// The text laid out in a frame `w` × `h` (square units, y down from its
/// top): each glyph, where its pen starts (the baseline's point) and its
/// style's place, the em (and each style's), and the box the text takes
/// (left, top, right, bottom).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Layout {
    pub glyphs: Vec<(GlyphId, [f64; 2], usize)>,
    pub em: f64,
    pub ems: Vec<f64>,
    pub bounds: [f64; 4],
    pub anchor: [f64; 2],
}

/// Lays `s`'s text out with `font` alone (its runs left out) in a frame
/// `w` × `h`.
#[cfg(test)]
fn layout(s: &Settings, font: &FontRef<'_>, w: f64, h: f64) -> Layout {
    let plain = Settings {
        runs: Vec::new(),
        ..s.clone()
    };
    let (chars, _) = styled_text(&plain);
    layout_styled(s, std::slice::from_ref(font), &[s.size], &chars, w, h)
}

/// A glyph as a line holds it: its advance and its style's place.
type Laid = (GlyphId, f64, usize);

/// A style's face as the layout reads it: its em, metrics, advances,
/// characters and kerning.
struct Face<'a> {
    em: f64,
    ascent: f64,
    descent: f64,
    leading: f64,
    /// A font unit, in the frame's units.
    unit: f64,
    advances: skrifa::metrics::GlyphMetrics<'a>,
    charmap: skrifa::charmap::Charmap<'a>,
    kerning: Option<Kerning<'a>>,
}

/// Lays `s`'s text out — its characters `chars`, each with its style's
/// place in `faces` (sized `sizes` points) — in a frame `w` × `h`: each
/// line as high as its tallest style.
fn layout_styled(
    s: &Settings,
    faces: &[FontRef<'_>],
    sizes: &[f64],
    chars: &[(char, usize)],
    w: f64,
    h: f64,
) -> Layout {
    let base = s.em(h);
    let styles: Vec<Face<'_>> = faces
        .iter()
        .zip(sizes)
        .map(|(font, &points)| {
            let em = if s.size > 0.0 {
                base * points.max(0.0) / s.size
            } else {
                base
            };
            let size = Size::new(em as f32);
            let m = font.metrics(size, LocationRef::default());
            Face {
                em,
                ascent: m.ascent as f64,
                descent: (m.descent as f64).abs(),
                leading: (m.leading as f64).max(0.0),
                unit: em / f64::from(m.units_per_em.max(1)),
                advances: font.glyph_metrics(size, LocationRef::default()),
                charmap: font.charmap(),
                kerning: s.kerning.then(|| Kerning::of(font)),
            }
        })
        .collect();
    if styles.is_empty() {
        return Layout::default();
    }
    let gap = s.tracking * base;
    // Each line's glyphs (advance and style), and the style of the
    // character that began it (an empty line's height).
    let mut lines: Vec<(Vec<Laid>, usize)> = vec![(Vec::new(), 0)];
    for &(c, st) in chars {
        let st = st.min(styles.len() - 1);
        if c == '\n' {
            lines.push((Vec::new(), st));
            continue;
        }
        let f = &styles[st];
        let g = f.charmap.map(c).unwrap_or(GlyphId::NOTDEF);
        let adv = f.advances.advance_width(g).unwrap_or(0.0) as f64;
        let Some((line, _)) = lines.last_mut() else {
            continue;
        };
        // The pairs the font kerns nearer or farther apart (within a style).
        if let Some(prev) = line.last_mut()
            && prev.2 == st
            && let Some(k) = &f.kerning
        {
            prev.1 += f64::from(k.pair(prev.0, g)) * f.unit;
        }
        line.push((g, adv, st));
    }
    let width =
        |l: &[Laid]| l.iter().map(|g| g.1).sum::<f64>() + gap * l.len().saturating_sub(1) as f64;
    let widths: Vec<f64> = lines.iter().map(|(l, _)| width(l)).collect();
    // Each line's ascent, descent and leading: its styles' largest.
    let metrics: Vec<(f64, f64, f64)> = lines
        .iter()
        .map(|(l, first)| {
            let used: Vec<usize> = if l.is_empty() {
                vec![*first]
            } else {
                l.iter().map(|g| g.2).collect()
            };
            used.iter().fold((0.0f64, 0.0f64, 0.0f64), |(a, d, g), &i| {
                let f = &styles[i];
                (a.max(f.ascent), d.max(f.descent), g.max(f.leading))
            })
        })
        .collect();
    let spacing = s.line_spacing.max(0.0);
    let mut baselines = Vec::with_capacity(lines.len());
    let mut y = metrics[0].0;
    for i in 0..lines.len() {
        if i > 0 {
            let (prev, now) = (metrics[i - 1], metrics[i]);
            y += (prev.1 + prev.2.max(now.2) + now.0) * spacing;
        }
        baselines.push(y);
    }
    let box_w = widths.iter().copied().fold(0.0, f64::max);
    let box_h = baselines.last().copied().unwrap_or(0.0) + metrics.last().map_or(0.0, |m| m.1);
    let k = s.anchor.clamp(0, 8);
    let (ax, ay) = ((k % 3) as f64 * 0.5, (k / 3) as f64 * 0.5);
    let anchor = [s.location[0] * w, (1.0 - s.location[1]) * h];
    let (left, top) = (anchor[0] - ax * box_w, anchor[1] - ay * box_h);
    let j = s.justify.clamp(0, 2) as f64 * 0.5;
    let mut glyphs = Vec::new();
    for (((line, _), lw), baseline) in lines.iter().zip(&widths).zip(&baselines) {
        let mut x = left + (box_w - lw) * j;
        for &(g, adv, st) in line {
            glyphs.push((g, [x, top + baseline], st));
            x += adv + gap;
        }
    }
    Layout {
        glyphs,
        em: base,
        ems: styles.iter().map(|f| f.em).collect(),
        bounds: [left, top, left + box_w, top + box_h],
        anchor,
    }
}

/// An outline's path into a [`Coverage`]: glyph units (y up, the em's
/// size) placed at a pen point, slanted when italic is made up, then
/// scaled to pixels and taken from the region's corner.
struct Pen<'a> {
    cover: &'a mut Coverage,
    origin: [f64; 2],
    slant: f64,
    k: [f64; 2],
    corner: [f64; 2],
    start: [f64; 2],
    at: [f64; 2],
}

impl Pen<'_> {
    fn map(&self, x: f32, y: f32) -> [f64; 2] {
        let (x, y) = (x as f64, y as f64);
        [
            (self.origin[0] + x + self.slant * y) * self.k[0] - self.corner[0],
            (self.origin[1] - y) * self.k[1] - self.corner[1],
        ]
    }
}

impl OutlinePen for Pen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.start = self.map(x, y);
        self.at = self.start;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.map(x, y);
        self.cover.segment(self.at, p);
        self.at = p;
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (c, p) = (self.map(cx0, cy0), self.map(x, y));
        self.cover.quad(self.at, c, p);
        self.at = p;
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (c0, c1, p) = (self.map(cx0, cy0), self.map(cx1, cy1), self.map(x, y));
        self.cover.cubic(self.at, c0, c1, p);
        self.at = p;
    }

    fn close(&mut self) {
        if self.at != self.start {
            self.cover.segment(self.at, self.start);
            self.at = self.start;
        }
    }
}

/// A region of the frame in pixels (rows from its top) and what covers
/// each of its pixels.
pub(crate) struct Region {
    pub x: i64,
    pub y: i64,
    pub w: usize,
    pub h: usize,
}

/// The letters' coverage over `r`: the frame's units scaled by `k` to
/// pixels, each in its style's face (`fonts`) and em.
fn letters(fonts: &[Font], l: &Layout, k: [f64; 2], r: &Region) -> Vec<f32> {
    let faces: Vec<Option<FontRef<'_>>> = fonts.iter().map(Font::face).collect();
    let outlines: Vec<Option<skrifa::outline::OutlineGlyphCollection<'_>>> = faces
        .iter()
        .map(|f| f.as_ref().map(|f| f.outline_glyphs()))
        .collect();
    let mut cover = Coverage::new(r.w, r.h);
    for &(g, at, st) in &l.glyphs {
        let (Some(Some(outline)), Some(font)) = (outlines.get(st), fonts.get(st)) else {
            continue;
        };
        let em = l.ems.get(st).copied().unwrap_or(l.em);
        let size = Size::new(em as f32);
        let slant = if font.slant { SLANT } else { 0.0 };
        // A missing bold: the letters drawn a little to each side.
        let thick = if font.embolden {
            vec![-EMBOLDEN * em, 0.0, EMBOLDEN * em]
        } else {
            vec![0.0]
        };
        let Some(glyph) = outline.get(g) else {
            continue;
        };
        for &dx in &thick {
            let mut pen = Pen {
                cover: &mut cover,
                origin: [at[0] + dx, at[1]],
                slant,
                k,
                corner: [r.x as f64, r.y as f64],
                start: [0.0; 2],
                at: [0.0; 2],
            };
            let _ = glyph.draw(
                DrawSettings::unhinted(size, LocationRef::default()),
                &mut pen,
            );
            pen.close();
        }
    }
    cover.finish()
}

/// The squared distance of each sample of `f` (0 where something is, a
/// large number elsewhere) to the nearest that is, along one line
/// (Felzenszwalb and Huttenlocher's lower envelope of parabolas).
fn distance_line(f: &[f64], out: &mut [f64]) {
    let n = f.len();
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f64; n + 1];
    let mut k = 0usize;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        loop {
            let p = v[k];
            let s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q - p) as f64);
            if s <= z[k] && k > 0 {
                k -= 1;
                continue;
            }
            if s <= z[k] {
                // k == 0: the first parabola is replaced.
                v[0] = q;
                z[1] = f64::INFINITY;
                break;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f64::INFINITY;
            break;
        }
    }
    k = 0;
    for (q, o) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let d = q as f64 - v[k] as f64;
        *o = d * d + f[v[k]];
    }
}

/// How far each pixel is from the letters (pixels; 0 inside them).
fn distances(cover: &[f32], w: usize, h: usize) -> Vec<f64> {
    const FAR: f64 = 1e12;
    let mut grid: Vec<f64> = cover
        .iter()
        .map(|&c| if c >= 0.5 { 0.0 } else { FAR })
        .collect();
    let mut col = vec![0.0; h];
    let mut out = vec![0.0; h.max(w)];
    for x in 0..w {
        for y in 0..h {
            col[y] = grid[y * w + x];
        }
        distance_line(&col, &mut out[..h]);
        for y in 0..h {
            grid[y * w + x] = out[y];
        }
    }
    let mut row = vec![0.0; w];
    for y in 0..h {
        row.copy_from_slice(&grid[y * w..(y + 1) * w]);
        distance_line(&row, &mut out[..w]);
        for x in 0..w {
            grid[y * w + x] = out[x].sqrt();
        }
    }
    grid
}

/// A box blur of radius `r` along rows (`along_rows`) or columns, the
/// edges' outside empty.
fn box_blur(v: &mut [f32], w: usize, h: usize, r: usize, along_rows: bool) {
    if r == 0 {
        return;
    }
    let (n, lines) = if along_rows { (w, h) } else { (h, w) };
    let idx = |line: usize, i: usize| {
        if along_rows {
            line * w + i
        } else {
            i * w + line
        }
    };
    let inv = 1.0 / (2 * r + 1) as f32;
    let mut buf = vec![0.0f32; n];
    for line in 0..lines {
        for (i, b) in buf.iter_mut().enumerate() {
            *b = v[idx(line, i)];
        }
        let mut acc: f32 = buf.iter().take(r + 1).sum();
        for i in 0..n {
            v[idx(line, i)] = acc * inv;
            if i + r + 1 < n {
                acc += buf[i + r + 1];
            }
            if i >= r {
                acc -= buf[i - r];
            }
        }
    }
}

/// A Gaussian blur of standard deviation `sigma` pixels (three box blurs
/// each way).
fn blur(v: &mut [f32], w: usize, h: usize, sigma: f64) {
    if sigma < 0.3 {
        return;
    }
    let r = ((12.0 * sigma * sigma / 3.0 + 1.0).sqrt() / 2.0).round() as usize;
    for _ in 0..3 {
        box_blur(v, w, h, r, true);
        box_blur(v, w, h, r, false);
    }
}

/// The picture of a title over a region of the frame: premultiplied RGBA,
/// rows from its top.
pub(crate) struct Picture {
    pub region: Region,
    pub rgba: Vec<[f32; 4]>,
}

/// Draws `s` in a frame `fw` × `fh` (square units) shown `k` pixels to a
/// unit across and up, as `w` × `h` pixels (Credit Roll's lines too).
pub(crate) fn picture(
    s: &Settings,
    fw: f64,
    fh: f64,
    k: [f64; 2],
    w: usize,
    h: usize,
) -> Option<Picture> {
    let (fonts, l) = laid_out(s, fw, fh)?;
    if l.glyphs.is_empty() || l.em <= 0.0 {
        return None;
    }
    // The largest letters reach the farthest.
    let big = l.ems.iter().copied().fold(l.em, f64::max);
    // The pixels the letters, their outline and their shadow can reach.
    let px = |v: f64| v * k[1];
    let outline = px(s.outline_width.max(0.0) * l.em);
    let sigma = px(s.shadow_blur.max(0.0) * l.em);
    let shift = [
        s.shadow_offset[0] * l.em * k[0],
        -s.shadow_offset[1] * l.em * k[1],
    ];
    let mut margin = outline + px(big) * 0.5 + 2.0;
    if s.shadow {
        margin += shift[0].abs().max(shift[1].abs()) + 3.0 * sigma;
    }
    let [l0, t0, r0, b0] = l.bounds;
    let x0 = ((l0 * k[0] - margin).floor() as i64).max(0);
    let y0 = ((t0 * k[1] - margin).floor() as i64).max(0);
    let x1 = ((r0 * k[0] + margin + px(big)).ceil() as i64).min(w as i64);
    let y1 = ((b0 * k[1] + margin).ceil() as i64).min(h as i64);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let region = Region {
        x: x0,
        y: y0,
        w: (x1 - x0) as usize,
        h: (y1 - y0) as usize,
    };
    let (rw, rh) = (region.w, region.h);
    let text = letters(&fonts, &l, k, &region);
    let edge = if outline > 0.0 {
        let d = distances(&text, rw, rh);
        Some(
            d.iter()
                .zip(&text)
                .map(|(&d, &c)| ((outline - d + 0.5).clamp(0.0, 1.0) as f32).max(c))
                .collect::<Vec<f32>>(),
        )
    } else {
        None
    };
    let shadow = s.shadow.then(|| {
        let solid = edge.as_ref().unwrap_or(&text);
        let mut sh = vec![0.0f32; rw * rh];
        let (dx, dy) = (shift[0].round() as i64, shift[1].round() as i64);
        for y in 0..rh as i64 {
            let sy = y - dy;
            if sy < 0 || sy >= rh as i64 {
                continue;
            }
            for x in 0..rw as i64 {
                let sx = x - dx;
                if sx >= 0 && sx < rw as i64 {
                    sh[(y as usize) * rw + x as usize] = solid[sy as usize * rw + sx as usize];
                }
            }
        }
        blur(&mut sh, rw, rh, sigma);
        sh
    });
    let pre = |c: [f64; 4], cover: f32| {
        let a = (c[3] as f32 * cover).clamp(0.0, 1.0);
        [c[0] as f32 * a, c[1] as f32 * a, c[2] as f32 * a, a]
    };
    let over = |top: [f32; 4], under: [f32; 4]| {
        let k = 1.0 - top[3];
        [
            top[0] + under[0] * k,
            top[1] + under[1] * k,
            top[2] + under[2] * k,
            top[3] + under[3] * k,
        ]
    };
    let mut rgba = Vec::with_capacity(rw * rh);
    for i in 0..rw * rh {
        let mut v = [0.0f32; 4];
        if let Some(sh) = &shadow {
            v = over(pre(s.shadow_color, sh[i]), v);
        }
        if let Some(e) = &edge {
            v = over(pre(s.outline_color, e[i]), v);
        }
        v = over(pre(s.color, text[i]), v);
        rgba.push(v);
    }
    Some(Picture { region, rgba })
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
    p.set_str(kOfxPropLabel, c"Titles & Text");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Text");
    p.set_str(
        kOfxPropPluginDescription,
        c"Text in the computer's fonts, with its color, an outline, a shadow and a background, placed and scaled in the frame.",
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
    // The text's box and its anchor on the preview.
    p.set_ptr(
        kOfxImageEffectPluginPropOverlayInteractV2,
        overlay as *mut c_void,
    );
}

fn describe_in_context(fx: Fx) {
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);
    let d = Settings::default();

    let group = |name: &CStr, label: &CStr, open: bool| {
        let p = fx.define_param(kOfxParamTypeGroup, name, label);
        p.set_int(kOfxParamPropGroupOpen, open as c_int);
    };
    let number = |name: &CStr,
                  label: &CStr,
                  parent: Option<&CStr>,
                  [min, max, lo, hi, default]: [f64; 5],
                  hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, default);
        p.set_dbl(kOfxParamPropMin, min);
        p.set_dbl(kOfxParamPropMax, max);
        p.set_dbl(kOfxParamPropDisplayMin, lo);
        p.set_dbl(kOfxParamPropDisplayMax, hi);
        p.set_dbl(kOfxParamPropIncrement, 0.01);
        p.set_int(kOfxParamPropDigits, 3);
        p.set_str(kOfxParamPropHint, hint);
        if let Some(parent) = parent {
            p.set_str(kOfxParamPropParent, parent);
        }
    };
    let color = |name: &CStr, label: &CStr, parent: Option<&CStr>, v: [f64; 4], hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeRGBA, name, label);
        p.set_dbls(kOfxParamPropDefault, &v);
        p.set_str(kOfxParamPropHint, hint);
        if let Some(parent) = parent {
            p.set_str(kOfxParamPropParent, parent);
        }
    };
    let toggle = |name: &CStr, label: &CStr, parent: Option<&CStr>, v: bool, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeBoolean, name, label);
        p.set_int(kOfxParamPropDefault, v as c_int);
        p.set_str(kOfxParamPropHint, hint);
        if let Some(parent) = parent {
            p.set_str(kOfxParamPropParent, parent);
        }
    };
    let choice = |name: &CStr, label: &CStr, options: &[&CStr], v: i32, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeChoice, name, label);
        p.set_strs(kOfxParamPropChoiceOption, options);
        p.set_int(kOfxParamPropDefault, v);
        p.set_str(kOfxParamPropHint, hint);
    };

    let p = fx.define_param(kOfxParamTypeString, TEXT, c"Text");
    p.set_str(kOfxParamPropStringMode, kOfxParamStringIsMultiLine);
    let sample = CString::new(SAMPLE).unwrap_or_default();
    p.set_str(kOfxParamPropDefault, &sample);
    p.set_str(kOfxParamPropHint, c"The text; Enter starts another line.");
    // Part of it selected, the font, size, bold and italic set it alone.
    p.set_str(
        bv_ofx_sys::extensions::kBananaVParamPropRichText,
        c"styles font size bold italic",
    );
    let p = fx.define_param(kOfxParamTypeString, STYLES, c"Styles");
    p.set_str(kOfxParamPropDefault, c"");
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_int(kOfxParamPropSecret, 1);
    // The computer's font families, as the text box's list.
    let families: Vec<CString> = fonts::families()
        .into_iter()
        .filter_map(|f| CString::new(f).ok())
        .collect();
    let names: Vec<&CStr> = families.iter().map(|c| c.as_c_str()).collect();
    let p = fx.define_param(kOfxParamTypeStrChoice, FONT, c"Font");
    p.set_strs(kOfxParamPropChoiceOption, &names);
    p.set_strs(kOfxParamPropChoiceEnum, &names);
    let default = CString::new(fonts::default_family()).unwrap_or_default();
    p.set_str(kOfxParamPropDefault, &default);
    p.set_str(kOfxParamPropHint, c"The typeface.");
    number(
        SIZE,
        c"Size",
        None,
        [1.0, 2000.0, 8.0, 200.0, d.size],
        c"The font's size in points, for a frame 1080 lines tall (the same part of other frames).",
    );
    toggle(BOLD, c"Bold", None, d.bold, c"The family's bold face.");
    toggle(
        ITALIC,
        c"Italic",
        None,
        d.italic,
        c"The family's italic face.",
    );
    choice(
        JUSTIFY,
        c"Justification",
        &JUSTIFICATIONS,
        d.justify,
        c"How the lines line up in the text's box.",
    );
    color(COLOR, c"Text Color", None, d.color, c"The letters' color.");
    number(
        SCALE,
        c"Scale",
        None,
        [0.0, 100.0, 0.0, 4.0, d.scale],
        c"1 leaves the text's size as it is; more enlarges it, less shrinks it (about the anchor point).",
    );
    let p = fx.define_param(kOfxParamTypeDouble2D, LOCATION, c"Location");
    p.set_str(
        kOfxParamPropDoubleType,
        kOfxParamDoubleTypeNormalisedXYAbsolute,
    );
    p.set_dbls(kOfxParamPropDefault, &d.location);
    p.set_dbls(kOfxParamPropIncrement, &[0.01]);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"Where the anchor point goes: 0, 0 is the frame's lower left, 0.5, 0.5 its middle, 1, 1 its upper right.",
    );
    choice(
        ANCHOR,
        c"Anchor Point",
        &ANCHORS,
        d.anchor,
        c"The point of the text's box that stays at the location: the text scales about it.",
    );
    group(ADVANCED, c"Advanced", false);
    color(
        BACKGROUND,
        c"Background",
        Some(ADVANCED),
        d.background,
        c"The frame's color behind the text (transparent: what is below shows).",
    );
    number(
        TRACKING,
        c"Tracking",
        Some(ADVANCED),
        [-1.0, 5.0, -0.5, 1.0, d.tracking],
        c"The room between letters, in ems.",
    );
    number(
        LINE_SPACING,
        c"Line Spacing",
        Some(ADVANCED),
        [0.0, 10.0, 0.0, 3.0, d.line_spacing],
        c"The distance between lines, times the font's own.",
    );
    group(OUTLINE, c"Outline", false);
    number(
        OUTLINE_WIDTH,
        c"Outline Width",
        Some(OUTLINE),
        [0.0, 1.0, 0.0, 0.25, d.outline_width],
        c"How far the outline goes around the letters, in ems (0: none).",
    );
    color(
        OUTLINE_COLOR,
        c"Outline Color",
        Some(OUTLINE),
        d.outline_color,
        c"The outline's color.",
    );
    group(SHADOW, c"Shadow", false);
    toggle(
        SHADOW_ON,
        c"Shadow Enable",
        Some(SHADOW),
        d.shadow,
        c"A shadow behind the text.",
    );
    color(
        SHADOW_COLOR,
        c"Shadow Color",
        Some(SHADOW),
        d.shadow_color,
        c"The shadow's color.",
    );
    number(
        SHADOW_X,
        c"Shadow Offset X",
        Some(SHADOW),
        [-2.0, 2.0, -0.5, 0.5, d.shadow_offset[0]],
        c"How far the shadow is to the right of the text, in ems.",
    );
    number(
        SHADOW_Y,
        c"Shadow Offset Y",
        Some(SHADOW),
        [-2.0, 2.0, -0.5, 0.5, d.shadow_offset[1]],
        c"How far the shadow is above the text, in ems.",
    );
    number(
        SHADOW_BLUR,
        c"Shadow Blur",
        Some(SHADOW),
        [0.0, 1.0, 0.0, 0.5, d.shadow_blur],
        c"How softly the shadow blends into what is behind it, in ems.",
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
    let s = Settings::at(fx, time);
    let (origin, frame) = fx.frame();
    let par = if out.par > 0.0 { out.par } else { 1.0 };
    // The frame in pixels: a unit is `k` pixels across and up.
    let k = [scale[0] / par, scale[1]];
    let first = [
        (origin[0] * k[0]).round() as c_int,
        (origin[1] * k[1]).round() as c_int,
    ];
    let w = ((frame[0] * k[0]).round() as usize).max(1);
    let h = ((frame[1] * k[1]).round() as usize).max(1);
    let pic = picture(&s, frame[0], frame[1], k, w, h);
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
        for x in columns.clone() {
            let fx_ = (x - first[0]) as i64;
            let mut v = back;
            if let Some(p) = &pic {
                let (i, j) = (fx_ - p.region.x, fy - p.region.y);
                if i >= 0 && j >= 0 && (i as usize) < p.region.w && (j as usize) < p.region.h {
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

// The text's box on the preview.

/// What the pen is doing with the box: moving it (from where it took it,
/// against the anchor) or scaling it by a corner (the scale and the
/// corner's distance from the anchor when it took it).
#[derive(Clone, Copy, Debug)]
enum Grab {
    Move([f64; 2]),
    Scale { scale: f64, reach: f64 },
}

#[derive(Debug, Default)]
struct OverlayState {
    grab: Option<Grab>,
    hover: bool,
}

/// The box's corners and the anchor in canonical units (y up), at `time`.
fn box_of(fx: Fx, time: f64) -> Option<([[f64; 2]; 4], [f64; 2])> {
    let s = Settings::at(fx, time);
    let (origin, frame) = fx.frame();
    let (_, l) = laid_out(&s, frame[0], frame[1])?;
    let up = |p: [f64; 2]| [origin[0] + p[0], origin[1] + frame[1] - p[1]];
    let [x0, y0, x1, y1] = l.bounds;
    Some((
        [up([x0, y0]), up([x1, y0]), up([x1, y1]), up([x0, y1])],
        up(l.anchor),
    ))
}

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
        let state = Box::into_raw(Box::new(OverlayState::default()));
        props.set_ptr(kOfxPropInstanceData, state as *mut c_void);
        return kOfxStatOK;
    }
    let state = props.ptr(kOfxPropInstanceData) as *mut OverlayState;
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
    let ps: [f64; 2] = args.dbls(kOfxInteractPropPixelScale);
    let ps = ps.map(|v| if v.abs() > 1e-12 { v.abs() } else { 1.0 });
    let Some((corners, anchor)) = box_of(fx, time) else {
        return kOfxStatReplyDefault;
    };
    let reach = 7.0;
    let near = |p: [f64; 2], q: [f64; 2]| {
        ((p[0] - q[0]) / ps[0]).abs() <= reach && ((p[1] - q[1]) / ps[1]).abs() <= reach
    };
    let (lo, hi) = (
        [
            corners[3][0].min(corners[1][0]),
            corners[2][1].min(corners[0][1]),
        ],
        [
            corners[3][0].max(corners[1][0]),
            corners[2][1].max(corners[0][1]),
        ],
    );
    if is(action, kOfxInteractActionDraw) {
        if let Some(d) = Draw::of(args) {
            let hot = state.hover || state.grab.is_some();
            let light = if hot {
                [1.0, 0.9, 0.45, 1.0]
            } else {
                [1.0, 1.0, 1.0, 1.0]
            };
            let square = |c: [f64; 2], r: f64| {
                [
                    [c[0] - r * ps[0], c[1] - r * ps[1]],
                    [c[0] + r * ps[0], c[1] - r * ps[1]],
                    [c[0] + r * ps[0], c[1] + r * ps[1]],
                    [c[0] - r * ps[0], c[1] + r * ps[1]],
                ]
            };
            for (colour, width) in [([0.0, 0.0, 0.0, 0.8], 3.0), (light, 1.2)] {
                d.colour(colour);
                d.width(width);
                d.outline(&corners);
                for c in corners {
                    d.outline(&square(c, 3.5));
                }
                // The anchor: an empty square.
                d.outline(&square(anchor, 5.0));
            }
        }
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    let corner = corners.iter().any(|&c| near(pen, c));
    let inside = pen[0] >= lo[0] && pen[0] <= hi[0] && pen[1] >= lo[1] && pen[1] <= hi[1];
    let (origin, frame) = fx.frame();
    if is(action, kOfxInteractActionPenDown) {
        if corner {
            let reach = (pen[0] - anchor[0]).hypot(pen[1] - anchor[1]).max(1e-6);
            let scale = fx.param(SCALE).f64_at(time);
            state.grab = Some(Grab::Scale { scale, reach });
        } else if inside {
            state.grab = Some(Grab::Move([pen[0] - anchor[0], pen[1] - anchor[1]]));
        } else {
            return kOfxStatReplyDefault;
        }
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        match state.grab {
            Some(Grab::Move(grip)) => {
                let to = [
                    (pen[0] - grip[0] - origin[0]) / frame[0],
                    (pen[1] - grip[1] - origin[1]) / frame[1],
                ];
                fx.param(LOCATION).set_xy(to);
                kOfxStatOK
            }
            Some(Grab::Scale { scale, reach }) => {
                let now = (pen[0] - anchor[0]).hypot(pen[1] - anchor[1]);
                fx.param(SCALE)
                    .set_f64((scale * now / reach).clamp(0.01, 100.0));
                kOfxStatOK
            }
            None => {
                let over = corner || inside;
                if over != state.hover {
                    state.hover = over;
                    redraw(handle);
                }
                kOfxStatReplyDefault
            }
        }
    } else if is(action, kOfxInteractActionPenUp) {
        if state.grab.take().is_some() {
            redraw(handle);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else {
        kOfxStatReplyDefault
    }
}

// Presets.

/// The factory presets: BananaV's own looks of a title.
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    vec![
        Preset {
            name: "Default",
            values: vec![],
        },
        Preset {
            name: "Lower Third",
            values: vec![
                ("text", PresetValue::Text("Name\nTitle".into())),
                ("size", num(&[40.0])),
                ("justification", num(&[0.0])),
                ("anchor", num(&[6.0])),
                ("location", num(&[0.1, 0.15])),
                ("shadowEnable", num(&[1.0])),
            ],
        },
        Preset {
            name: "Large Title",
            values: vec![
                ("text", PresetValue::Text("Title".into())),
                ("size", num(&[110.0])),
                ("bold", num(&[1.0])),
            ],
        },
        Preset {
            name: "Outlined",
            values: vec![("size", num(&[72.0])), ("outlineWidth", num(&[0.06]))],
        },
        Preset {
            name: "Soft Shadow",
            values: vec![
                ("size", num(&[72.0])),
                ("shadowEnable", num(&[1.0])),
                ("shadowBlur", num(&[0.12])),
            ],
        },
        Preset {
            name: "Black on White",
            values: vec![
                ("textColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("background", num(&[1.0, 1.0, 1.0, 1.0])),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_measure_to_the_nearest_covered_pixel() {
        let (w, h) = (9, 7);
        let mut cover = vec![0.0f32; w * h];
        cover[3 * w + 4] = 1.0;
        let d = distances(&cover, w, h);
        assert_eq!(d[3 * w + 4], 0.0);
        assert!((d[3 * w + 7] - 3.0).abs() < 1e-9);
        assert!((d[w + 2] - (8.0f64).sqrt()).abs() < 1e-9);
        assert!((d[6 * w + 8] - 5.0).abs() < 1e-9);
    }

    #[test]
    fn a_blur_keeps_what_it_spreads() {
        let (w, h) = (31, 31);
        let mut v = vec![0.0f32; w * h];
        v[15 * w + 15] = 1.0;
        blur(&mut v, w, h, 2.0);
        let total: f32 = v.iter().sum();
        assert!((total - 1.0).abs() < 1e-4, "{total}");
        assert!(v[15 * w + 15] < 0.1 && v[15 * w + 15] > v[15 * w + 19]);
    }

    /// The computer's default family, or nothing to test with.
    fn settings() -> Option<Settings> {
        let font = fonts::default_family();
        fonts::find(&font, false, false)?;
        Some(Settings {
            font,
            ..Settings::default()
        })
    }

    #[test]
    fn the_box_sits_on_its_anchor_and_scales_about_it() {
        let Some(s) = settings() else { return };
        let font = Font::of(&s).unwrap();
        let face = font.face().unwrap();
        let l = layout(&s, &face, 1920.0, 1080.0);
        // Centered: the box's middle at the frame's.
        let mid = [
            (l.bounds[0] + l.bounds[2]) / 2.0,
            (l.bounds[1] + l.bounds[3]) / 2.0,
        ];
        assert!((mid[0] - 960.0).abs() < 1e-6 && (mid[1] - 540.0).abs() < 1e-6);
        assert!((l.em - 64.0).abs() < 1e-9, "48 points at 1080 lines");
        // Twice the scale, the box twice as wide, its lower left kept.
        let corner = Settings {
            anchor: 6,
            location: [0.1, 0.2],
            ..s.clone()
        };
        let a = layout(&corner, &face, 1920.0, 1080.0);
        let b = layout(
            &Settings {
                scale: 2.0,
                ..corner.clone()
            },
            &face,
            1920.0,
            1080.0,
        );
        assert!((a.bounds[0] - 192.0).abs() < 1e-6 && (a.bounds[3] - 864.0).abs() < 1e-6);
        assert!((b.bounds[0] - 192.0).abs() < 1e-6 && (b.bounds[3] - 864.0).abs() < 1e-6);
        let wide = |l: &Layout| l.bounds[2] - l.bounds[0];
        assert!((wide(&b) / wide(&a) - 2.0).abs() < 1e-6);
        // Two lines, the second right-justified under the first.
        let two = Settings {
            text: "Wide line\nend".into(),
            justify: 2,
            ..s
        };
        let l = layout(&two, &face, 1920.0, 1080.0);
        let last = l.glyphs.last().unwrap().1;
        assert!(last[1] > l.glyphs[0].1[1], "the second line is lower");
        assert!(last[0] > l.glyphs[0].1[0]);
    }

    /// Runs read back from their lines; a run set larger makes its part's
    /// letters and its line bigger.
    #[test]
    fn a_selected_part_takes_its_own_style() {
        let runs = parse_runs("0 5 size=96;bold=1\n6 9 font=Arial\nbad line\n4 2 size=1\n");
        assert_eq!(runs.len(), 2);
        assert_eq!((runs[0].size, runs[0].bold), (Some(96.0), Some(true)));
        assert_eq!(runs[1].font.as_deref(), Some("Arial"));
        let Some(s) = settings() else { return };
        let plain = Settings {
            text: "Hello world".into(),
            ..s.clone()
        };
        let big = Settings {
            runs: parse_runs("0 5 size=96"),
            ..plain.clone()
        };
        let (_, a) = laid_out(&plain, 1920.0, 1080.0).unwrap();
        let (_, b) = laid_out(&big, 1920.0, 1080.0).unwrap();
        let size = |l: &Layout| (l.bounds[2] - l.bounds[0], l.bounds[3] - l.bounds[1]);
        assert!(size(&b).0 > size(&a).0 && size(&b).1 > size(&a).1);
        assert_eq!(b.ems.len(), 2);
        assert!(
            (b.ems[1] / b.ems[0] - 2.0).abs() < 1e-9,
            "96 points over 48"
        );
        // "Hello" in the second style, " world" in the title's.
        assert!(b.glyphs[..5].iter().all(|g| g.2 == 1));
        assert!(b.glyphs[5..].iter().all(|g| g.2 == 0));
        assert!(picture(&big, 640.0, 360.0, [1.0, 1.0], 640, 360).is_some());
    }

    #[test]
    fn the_font_s_own_kerning_draws_pairs_nearer() {
        let Some(s) = settings() else { return };
        let font = Font::of(&s).unwrap();
        let face = font.face().unwrap();
        let wide = |kerning| {
            let text = "AVATAR To".into();
            let l = layout(
                &Settings {
                    text,
                    kerning,
                    ..s.clone()
                },
                &face,
                1920.0,
                1080.0,
            );
            l.bounds[2] - l.bounds[0]
        };
        assert!(wide(true) <= wide(false));
        // Arial's pairs (Windows' default family).
        if cfg!(windows) {
            assert!(
                wide(true) < wide(false) - 1.0,
                "{} {}",
                wide(true),
                wide(false)
            );
        }
    }

    #[test]
    fn letters_outline_and_shadow_cover_what_they_should() {
        let Some(s) = settings() else { return };
        let (w, h) = (640usize, 360usize);
        let plain = picture(&s, 640.0, 360.0, [1.0, 1.0], w, h).expect("a picture");
        let cover = |p: &Picture| p.rgba.iter().filter(|c| c[3] > 0.5).count();
        let letters = cover(&plain);
        assert!(letters > 100, "{letters}");
        // White letters, transparent around them.
        assert!(plain.rgba.iter().any(|c| c[0] > 0.99 && c[3] > 0.99));
        assert!(plain.rgba.iter().any(|c| c[3] == 0.0));
        let outlined = picture(
            &Settings {
                outline_width: 0.08,
                ..s.clone()
            },
            640.0,
            360.0,
            [1.0, 1.0],
            w,
            h,
        )
        .unwrap();
        assert!(cover(&outlined) > letters + letters / 4);
        // Black outline pixels: covered, dark.
        assert!(outlined.rgba.iter().any(|c| c[3] > 0.99 && c[0] < 0.01));
        let shadowed = picture(
            &Settings { shadow: true, ..s },
            640.0,
            360.0,
            [1.0, 1.0],
            w,
            h,
        )
        .unwrap();
        assert!(cover(&shadowed) > letters);
    }

    /// Titles over a gray checkerboard, for looking at:
    /// `cargo test -p bv-effects titles_for_review -- --ignored` writes
    /// `target/telas/titles_*.png`.
    #[test]
    #[ignore = "renders images for visual review"]
    fn titles_for_review() {
        let Some(s) = settings() else { return };
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
        std::fs::create_dir_all(&dir).unwrap();
        let (w, h) = (960usize, 540usize);
        let looks = [
            ("padrao", s.clone()),
            (
                "contorno_sombra",
                Settings {
                    text: "Titles & Text
BananaV"
                        .into(),
                    size: 90.0,
                    bold: true,
                    outline_width: 0.05,
                    shadow: true,
                    shadow_blur: 0.1,
                    ..s.clone()
                },
            ),
            (
                "terco_inferior",
                Settings {
                    text: "Name Surname
Title of the person"
                        .into(),
                    size: 40.0,
                    italic: true,
                    justify: 0,
                    anchor: 6,
                    location: [0.08, 0.12],
                    background: [0.1, 0.2, 0.5, 0.6],
                    ..s.clone()
                },
            ),
            (
                "trechos",
                Settings {
                    text: "Titles & Text
parts in their own style"
                        .into(),
                    size: 56.0,
                    // "Titles" larger and bold, "own" italic.
                    runs: parse_runs("0 6 size=96;bold=1\n29 32 italic=1"),
                    ..s
                },
            ),
        ];
        for (name, look) in looks {
            let p = picture(&look, w as f64, h as f64, [1.0, 1.0], w, h);
            let mut rgba = vec![0u8; w * h * 4];
            let back = {
                let a = look.background[3] as f32;
                [
                    look.background[0] as f32 * a,
                    look.background[1] as f32 * a,
                    look.background[2] as f32 * a,
                    a,
                ]
            };
            for y in 0..h {
                for x in 0..w {
                    let gray = if (x / 16 + y / 16) % 2 == 0 {
                        0.35
                    } else {
                        0.45
                    };
                    let mut v = back;
                    if let Some(p) = &p {
                        let (i, j) = (x as i64 - p.region.x, y as i64 - p.region.y);
                        if i >= 0
                            && j >= 0
                            && (i as usize) < p.region.w
                            && (j as usize) < p.region.h
                        {
                            let t = p.rgba[j as usize * p.region.w + i as usize];
                            v = [0, 1, 2, 3].map(|c| t[c] + v[c] * (1.0 - t[3]));
                        }
                    }
                    for c in 0..3 {
                        let out = v[c] + gray * (1.0 - v[3]);
                        rgba[(y * w + x) * 4 + c] = (out.clamp(0.0, 1.0) * 255.0).round() as u8;
                    }
                    rgba[(y * w + x) * 4 + 3] = 255;
                }
            }
            let png = bv_png::encode_rgba8(w as u32, h as u32, &rgba, 6);
            std::fs::write(dir.join(format!("titles_{name}.png")), png).unwrap();
        }
    }

    #[test]
    fn the_presets_name_real_parameters() {
        for p in presets() {
            for (name, _) in &p.values {
                assert!(
                    [
                        "text",
                        "size",
                        "bold",
                        "justification",
                        "anchor",
                        "location",
                        "shadowEnable",
                        "shadowBlur",
                        "outlineWidth",
                        "textColor",
                        "background",
                    ]
                    .contains(name),
                    "{name}"
                );
            }
        }
    }
}
