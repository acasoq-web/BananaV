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
//! Its settings count as VEGAS's do, so a `.veg`'s numbers and the panel's
//! are the same: a point is a 200th of the frame's height; tracking is in
//! twentieths of it; the outline is a stroke along the letters' edges
//! (half of it outside them), a 400th of the frame's height wide for each
//! unit of its width; the shadow's offsets and blur are tenths of the
//! frame's width. The defaults are VEGAS's, its Animation list moves the
//! text ([`crate::title_animations`]), and a see-through text color shows
//! what is below through the background (the letters cut it out).
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::fonts;
use crate::kerning::Kerning;
use crate::ofx::{Draw, Fx, Props, for_rows, interact_props, is, redraw, suites};
use crate::text::Coverage;
use crate::title_animations::{self, Animation, draw, scene};
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
/// Transfer Text Properties: a push button the host answers.
const TRANSFER: &CStr = c"transferTextProperties";
const COLOR: &CStr = c"textColor";
const SCALE: &CStr = c"scale";
const LOCATION: &CStr = c"location";
const ANCHOR: &CStr = c"anchor";
const ADVANCED: &CStr = c"advanced";
const BACKGROUND: &CStr = c"background";
/// The background only behind the text.
const FIT_BACKGROUND: &CStr = c"fitBackgroundColor";
/// The animation a `.veg`'s title had (hidden: kept for writing it back).
const ANIMATION: &CStr = c"animationName";
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

/// A point's size, a part of the frame's height: a 200th (VEGAS's rich
/// text's half points each a 400th; 48 points make an em of 0.24 of it).
pub(crate) const POINT: f64 = 1.0 / 200.0;
/// The tracking's and the animations' unit, a part of the frame's height.
pub(crate) const UNIT: f64 = 1.0 / 20.0;
/// The outline's width for each unit of its setting, a part of the frame's
/// height.
pub(crate) const STROKE: f64 = 1.0 / 400.0;
/// The thinnest outline drawn, in pixels (a thinner one is drawn this
/// wide, as VEGAS's are).
pub(crate) const THINNEST: f64 = 1.0;
/// Where VEGAS's titles fall on the pixels: half a pixel right of and
/// below where BananaV's pixel grid puts them (their renders' letters, and
/// their backgrounds' boxes).
pub(crate) const HALF: f64 = 0.5;
/// How far a missing bold face's letters are thickened each way (ems), and
/// how far a missing italic's lean (across per up).
pub(crate) const EMBOLDEN: f64 = 0.022;
pub(crate) const SLANT: f64 = 0.21;

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
    /// Between letters, [`UNIT`]s (times the scale).
    pub tracking: f64,
    /// The font's own kerning between pairs of letters.
    pub kerning: bool,
    /// Lines' distance, times the font's.
    pub line_spacing: f64,
    /// The stroke's width, [`STROKE`]s (times the scale); 0: none.
    pub outline_width: f64,
    pub outline_color: [f64; 4],
    pub shadow: bool,
    pub shadow_color: [f64; 4],
    /// Tenths of the frame's width, right and down.
    pub shadow_offset: [f64; 2],
    /// Tenths of the frame's width: the blur's diameter.
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

/// VEGAS's: "Sample Text" in white 48-point Verdana, left justified, in
/// the frame's middle; a white outline and a black shadow down and to the
/// right, both off.
impl Default for Settings {
    fn default() -> Settings {
        Settings {
            text: SAMPLE.into(),
            font: String::new(),
            size: 48.0,
            bold: false,
            italic: false,
            justify: 0,
            color: [1.0, 1.0, 1.0, 1.0],
            scale: 1.0,
            location: [0.5, 0.5],
            anchor: 4,
            background: [0.0, 0.0, 0.0, 0.0],
            tracking: 0.0,
            kerning: true,
            line_spacing: 1.0,
            outline_width: 0.0,
            outline_color: [1.0, 1.0, 1.0, 1.0],
            shadow: false,
            shadow_color: [0.0, 0.0, 0.0, 1.0],
            shadow_offset: [0.2, 0.2],
            shadow_blur: 0.4,
            runs: Vec::new(),
        }
    }
}

/// A new title's typeface: Verdana, as VEGAS's, or the computer's usual
/// one without it.
fn default_font() -> String {
    fonts::families()
        .into_iter()
        .find(|f| f.eq_ignore_ascii_case("Verdana"))
        .unwrap_or_else(fonts::default_family)
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
pub(crate) struct Font {
    data: std::sync::Arc<Vec<u8>>,
    index: u32,
    pub embolden: bool,
    pub slant: bool,
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

    pub(crate) fn face(&self) -> Option<FontRef<'_>> {
        FontRef::from_index(&self.data, self.index).ok()
    }
}

/// `s`'s faces: its own, then its runs'.
fn fonts_of(keys: &[StyleKey], s: &Settings) -> Option<Vec<Font>> {
    let base = Font::of(s)?;
    Some(
        keys.iter()
            .enumerate()
            .map(|(i, k)| match i {
                0 => base.clone(),
                _ => Font::of_key(k).unwrap_or_else(|| base.clone()),
            })
            .collect(),
    )
}

/// The characters whose style's face lacks them (Chinese, Japanese,
/// Korean, other scripts typed into a Latin font) drawn from a face that
/// has them: each such style and face a style of its own after the others
/// (`fonts` and `sizes` grow), the same size, bold and italic.
fn with_fallbacks(
    chars: &mut [(char, usize)],
    keys: &[StyleKey],
    fonts: &mut Vec<Font>,
    sizes: &mut Vec<f64>,
) {
    let missing: Vec<usize> = {
        let faces: Vec<Option<FontRef<'_>>> = fonts.iter().map(Font::face).collect();
        let maps: Vec<Option<skrifa::charmap::Charmap<'_>>> = faces
            .iter()
            .map(|f| f.as_ref().map(|f| f.charmap()))
            .collect();
        chars
            .iter()
            .enumerate()
            .filter(|(_, (c, st))| {
                !c.is_control()
                    && maps
                        .get(*st)
                        .and_then(Option::as_ref)
                        .is_some_and(|m| m.map(*c).is_none_or(|g| g == GlyphId::NOTDEF))
            })
            .map(|(i, _)| i)
            .collect()
    };
    if missing.is_empty() {
        return;
    }
    let cjk = fonts::Cjk::of(chars.iter().map(|c| c.0));
    // Each style added: the style it stands in for, its face (file and
    // place) and what is made up; and its place.
    type Added = (usize, &'static std::path::Path, u32, bool, bool);
    let mut added: Vec<(Added, usize)> = Vec::new();
    for i in missing {
        let (c, st) = chars[i];
        let Some(key) = keys.get(st) else {
            continue;
        };
        let Some((face, embolden, slant)) = fonts::fallback(c, cjk, key.bold, key.italic) else {
            continue;
        };
        let k = (st, face.path.as_path(), face.index, embolden, slant);
        let style = match added.iter().find(|(a, _)| *a == k) {
            Some(&(_, n)) => n,
            None => {
                let Some(data) = fonts::data(&face.path) else {
                    continue;
                };
                fonts.push(Font {
                    data,
                    index: face.index,
                    embolden,
                    slant,
                });
                sizes.push(sizes.get(st).copied().unwrap_or_default());
                added.push((k, fonts.len() - 1));
                fonts.len() - 1
            }
        };
        chars[i].1 = style;
    }
}

/// `s`'s faces (its own, then its runs') and its text laid out with them
/// in a frame `w` × `h`.
fn laid_out(s: &Settings, w: f64, h: f64) -> Option<(Vec<Font>, Layout)> {
    let (mut chars, keys) = styled_text(s);
    let mut fonts = fonts_of(&keys, s)?;
    let mut sizes: Vec<f64> = keys.iter().map(|k| k.size).collect();
    with_fallbacks(&mut chars, &keys, &mut fonts, &mut sizes);
    let faces = fonts.iter().map(Font::face).collect::<Option<Vec<_>>>()?;
    let l = layout_styled(s, &faces, &sizes, &chars, w, h);
    drop(faces);
    Some((fonts, l))
}

/// `s`'s faces and its text shaped with them for a frame `h` tall (its
/// lines, not yet placed).
fn shaped(s: &Settings, h: f64) -> Option<(Vec<Font>, Shaped)> {
    let (mut chars, keys) = styled_text(s);
    let mut fonts = fonts_of(&keys, s)?;
    let mut sizes: Vec<f64> = keys.iter().map(|k| k.size).collect();
    with_fallbacks(&mut chars, &keys, &mut fonts, &mut sizes);
    let faces = fonts.iter().map(Font::face).collect::<Option<Vec<_>>>()?;
    let sh = shape_styled(s, &faces, &sizes, &chars, h)?;
    drop(faces);
    Some((fonts, sh))
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
pub(crate) type Laid = (GlyphId, f64, usize);

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

/// The text shaped, not yet placed: each line's glyphs (advance and
/// style) and characters, its ascent, descent and leading (its styles'
/// largest); the em, each style's, and the room tracking leaves after a
/// letter (all in the frame's units).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Shaped {
    pub lines: Vec<ShapedLine>,
    pub em: f64,
    pub ems: Vec<f64>,
    pub gap: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ShapedLine {
    pub glyphs: Vec<Laid>,
    pub chars: Vec<char>,
    pub ascent: f64,
    pub descent: f64,
    pub leading: f64,
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
    match shape_styled(s, faces, sizes, chars, h) {
        Some(sh) => place(s, &sh, w, h),
        None => Layout::default(),
    }
}

/// Shapes `s`'s text — its characters `chars`, each with its style's place
/// in `faces` (sized `sizes` points) — for a frame `h` tall: its lines'
/// glyphs and heights.
fn shape_styled(
    s: &Settings,
    faces: &[FontRef<'_>],
    sizes: &[f64],
    chars: &[(char, usize)],
    h: f64,
) -> Option<Shaped> {
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
            let unit = em / f64::from(m.units_per_em.max(1));
            // A line's box: the font's cell as Windows measures it (its
            // ascent and descent there), as VEGAS's titles are laid out,
            // and lines that far apart plus the font's external leading
            // (its line gap less what the cell already holds of it), times
            // Line Spacing.
            let (ascent, descent, leading) = match (font.os2(), font.hhea()) {
                (Ok(t), Ok(hh)) if t.us_win_ascent() > 0 => {
                    let win = f64::from(t.us_win_ascent()) + f64::from(t.us_win_descent());
                    let own =
                        f64::from(hh.ascender().to_i16()) - f64::from(hh.descender().to_i16());
                    let gap = (f64::from(hh.line_gap().to_i16()) - (win - own)).max(0.0);
                    (
                        f64::from(t.us_win_ascent()) * unit,
                        f64::from(t.us_win_descent()) * unit,
                        gap * unit,
                    )
                }
                _ => (
                    m.ascent as f64,
                    (m.descent as f64).abs(),
                    (m.leading as f64).max(0.0),
                ),
            };
            Face {
                em,
                ascent,
                descent,
                leading,
                unit,
                advances: font.glyph_metrics(size, LocationRef::default()),
                charmap: font.charmap(),
                kerning: s.kerning.then(|| Kerning::of(font)),
            }
        })
        .collect();
    if styles.is_empty() {
        return None;
    }
    // Tracking: the same room after every letter whatever its size, in
    // units of the frame's height (scaled with the text).
    let gap = s.tracking * UNIT * h * s.scale.max(0.0);
    // Each line's glyphs (advance and style) and characters, and the style
    // of the character that began it (an empty line's height).
    let mut lines: Vec<(Vec<Laid>, Vec<char>, usize)> = vec![(Vec::new(), Vec::new(), 0)];
    for &(c, st) in chars {
        let st = st.min(styles.len() - 1);
        if c == '\n' {
            lines.push((Vec::new(), Vec::new(), st));
            continue;
        }
        let f = &styles[st];
        let g = f.charmap.map(c).unwrap_or(GlyphId::NOTDEF);
        let adv = f.advances.advance_width(g).unwrap_or(0.0) as f64;
        let Some((line, letters, _)) = lines.last_mut() else {
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
        letters.push(c);
    }
    Some(Shaped {
        lines: lines
            .into_iter()
            .map(|(glyphs, chars, first)| {
                // Its ascent, descent and leading: its styles' largest.
                let used: Vec<usize> = if glyphs.is_empty() {
                    vec![first]
                } else {
                    glyphs.iter().map(|g| g.2).collect()
                };
                let (ascent, descent, leading) =
                    used.iter().fold((0.0f64, 0.0f64, 0.0f64), |(a, d, g), &i| {
                        let f = &styles[i];
                        (a.max(f.ascent), d.max(f.descent), g.max(f.leading))
                    });
                ShapedLine {
                    glyphs,
                    chars,
                    ascent,
                    descent,
                    leading,
                }
            })
            .collect(),
        em: base,
        ems: styles.iter().map(|f| f.em).collect(),
        gap,
    })
}

/// Shaped text placed in a frame `w` × `h`: its box on the anchor at the
/// location, its lines justified in it.
fn place(s: &Settings, sh: &Shaped, w: f64, h: f64) -> Layout {
    let gap = sh.gap;
    let lines = &sh.lines;
    let width =
        |l: &[Laid]| l.iter().map(|g| g.1).sum::<f64>() + gap * l.len().saturating_sub(1) as f64;
    let widths: Vec<f64> = lines.iter().map(|l| width(&l.glyphs)).collect();
    let metrics: Vec<(f64, f64, f64)> = lines
        .iter()
        .map(|l| (l.ascent, l.descent, l.leading))
        .collect();
    // Under 0 the lines go up from the first, as VEGAS's do.
    let spacing = if s.line_spacing.is_finite() {
        s.line_spacing
    } else {
        1.0
    };
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
    for ((line, lw), baseline) in lines.iter().zip(&widths).zip(&baselines) {
        let mut x = left + (box_w - lw) * j;
        for &(g, adv, st) in &line.glyphs {
            glyphs.push((g, [x, top + baseline], st));
            x += adv + gap;
        }
    }
    Layout {
        glyphs,
        em: sh.em,
        ems: sh.ems.clone(),
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
                corner: [r.x as f64 - HALF, r.y as f64 - HALF],
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
/// large number elsewhere) to the nearest that is, along one line, and
/// which sample that is (Felzenszwalb and Huttenlocher's lower envelope of
/// parabolas).
fn distance_line(f: &[f64], out: &mut [f64], near: &mut [usize]) {
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
    for (q, (o, m)) in out.iter_mut().zip(near.iter_mut()).enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let d = q as f64 - v[k] as f64;
        *o = d * d + f[v[k]];
        *m = v[k];
    }
}

/// Far from everything (a squared distance).
const FAR: f64 = 1e12;

/// For each pixel of a picture `w` × `h`, the distance to the nearest
/// pixel `site` says is one, and that pixel's index (`FAR` and any index:
/// none is).
fn nearest_sites(site: &[bool], w: usize, h: usize) -> (Vec<f64>, Vec<usize>) {
    let mut grid: Vec<f64> = site.iter().map(|&s| if s { 0.0 } else { FAR }).collect();
    // Down each column: the nearest site's row there.
    let mut rows = vec![0usize; w * h];
    let mut col = vec![0.0; h];
    let mut out = vec![0.0; h.max(w)];
    let mut near = vec![0usize; h.max(w)];
    for x in 0..w {
        for y in 0..h {
            col[y] = grid[y * w + x];
        }
        distance_line(&col, &mut out[..h], &mut near[..h]);
        for y in 0..h {
            grid[y * w + x] = out[y];
            rows[y * w + x] = near[y];
        }
    }
    // Along each row: the nearest of those.
    let mut at = vec![0usize; w * h];
    let mut row = vec![0.0; w];
    for y in 0..h {
        row.copy_from_slice(&grid[y * w..(y + 1) * w]);
        distance_line(&row, &mut out[..w], &mut near[..w]);
        for x in 0..w {
            let c = near[x];
            grid[y * w + x] = if out[x] >= FAR / 2.0 {
                FAR
            } else {
                out[x].sqrt()
            };
            at[y * w + x] = rows[y * w + c] * w + c;
        }
    }
    (grid, at)
}

/// How far each pixel's middle is from the letters' edge (pixels; less
/// than 0 inside), from their coverage `cover`: the edge crosses the
/// nearest pixel they partly cover where its coverage says (a pixel half
/// covered has it through its middle). Inside the letters only when
/// `inside` (else far inside).
pub(crate) fn edge_distances(cover: &[f32], w: usize, h: usize, inside: bool) -> Vec<f64> {
    // Less than this is the rows' sums' rounding, not the letters.
    const NONE: f32 = 0.001;
    const FULL: f32 = 1.0 - NONE;
    let touched: Vec<bool> = cover.iter().map(|&c| c > NONE).collect();
    let (out_d, out_at) = nearest_sites(&touched, w, h);
    let (in_d, in_at) = if inside {
        let open: Vec<bool> = cover.iter().map(|&c| c < FULL).collect();
        nearest_sites(&open, w, h)
    } else {
        (Vec::new(), Vec::new())
    };
    cover
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            if c <= NONE {
                if out_d[i] >= FAR {
                    FAR
                } else {
                    out_d[i] + 0.5 - f64::from(cover[out_at[i]])
                }
            } else if c < FULL {
                0.5 - f64::from(c)
            } else if inside && in_d[i] < FAR {
                -(in_d[i] - 0.5 + f64::from(cover[in_at[i]]))
            } else {
                -FAR
            }
        })
        .collect()
}

/// What a stroke `width` pixels wide along the letters' edges covers of
/// each pixel — half of it outside them, half inside —, from their
/// coverage `cover`. Without `inside` (an opaque fill hides it), all of
/// what they cover too.
pub(crate) fn stroke_cover(
    cover: &[f32],
    w: usize,
    h: usize,
    width: f64,
    inside: bool,
) -> Vec<f32> {
    let half = width / 2.0;
    edge_distances(cover, w, h, inside)
        .iter()
        .map(|&d| {
            if d <= -FAR / 2.0 {
                return 1.0;
            }
            // A pixel's span across the edge against the stroke's.
            let t = d.abs();
            ((t + 0.5).min(half) - (t - 0.5).max(-half)).clamp(0.0, 1.0) as f32
        })
        .collect()
}

/// The picture of a title over a region of the frame: premultiplied RGBA,
/// rows from its top; and, when its text color is see-through, how much
/// of each pixel its letters cover (they cut the background out there).
pub(crate) struct Picture {
    pub region: Region,
    pub rgba: Vec<[f32; 4]>,
    pub cut: Option<Vec<f32>>,
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
    let par = if k[0] > 0.0 { k[1] / k[0] } else { 1.0 };
    picture_in(
        s,
        draw::View {
            frame: [fw, fh],
            par,
            k,
            size: [w, h],
        },
    )
}

/// Draws `s`, still, in `view`: its letters over their outline, over
/// their shadow.
pub(crate) fn picture_in(s: &Settings, view: draw::View) -> Option<Picture> {
    let [fw, fh] = view.frame;
    let k = view.k;
    let [w, h] = view.size;
    let (fonts, l) = laid_out(s, fw, fh)?;
    if l.glyphs.is_empty() || l.em <= 0.0 {
        return None;
    }
    // The largest letters reach the farthest.
    let big = l.ems.iter().copied().fold(l.em, f64::max);
    // The pixels the letters, their outline and their shadow can reach.
    let px = |v: f64| v * k[1];
    let stroke = if s.outline_width > 0.0 {
        px(s.outline_width * STROKE * fh * s.scale.max(0.0)).max(THINNEST)
    } else {
        0.0
    };
    let shadow = s.shadow.then(|| draw::shadow_of(s, view));
    let mut margin = stroke + px(big) * 0.5 + 2.0;
    if let Some((shift, sigma)) = shadow {
        margin += shift[0].abs().max(shift[1].abs()) + 3.0 * sigma[0].max(sigma[1]) + 2.0;
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
    // A see-through fill shows the inner half of the outline through it.
    let see_through = s.color[3] < 1.0;
    let edge = (stroke > 0.0).then(|| stroke_cover(&text, rw, rh, stroke, see_through));
    let pre = |c: [f64; 4], cover: f32| {
        let a = (c[3] as f32 * cover).clamp(0.0, 1.0);
        [c[0] as f32 * a, c[1] as f32 * a, c[2] as f32 * a, a]
    };
    let mut rgba = Vec::with_capacity(rw * rh);
    for i in 0..rw * rh {
        let mut v = [0.0f32; 4];
        if let Some(e) = &edge {
            v = pre(s.outline_color, e[i]);
        }
        v = draw::over(pre(s.color, text[i]), v);
        rgba.push(v);
    }
    if let Some((shift, sigma)) = shadow {
        draw::lay_shadow(&mut rgba, rw, rh, s.shadow_color, shift, sigma);
    }
    Some(Picture {
        region,
        rgba,
        cut: see_through.then_some(text),
    })
}

/// Draws `s` moved by `anim` at the moment `u` (the part of its media
/// played) in `view`: its picture (none: nothing shows), and the box a
/// fitted background takes then (frame units: left, top, right, bottom).
pub(crate) fn animated(
    s: &Settings,
    anim: &'static Animation,
    u: f64,
    view: draw::View,
) -> Option<(Option<Picture>, [f64; 4])> {
    // Laid out at a scale of 1: the animation scales the block itself.
    let one = Settings {
        scale: 1.0,
        ..s.clone()
    };
    let (fonts, sh) = shaped(&one, view.frame[1])?;
    if sh.em <= 0.0 {
        return None;
    }
    let text = scene::Text {
        lines: sh
            .lines
            .iter()
            .map(|l| scene::TextLine {
                chars: l.chars.clone(),
                advances: l.glyphs.iter().map(|g| g.1).collect(),
                ems: l
                    .glyphs
                    .iter()
                    .map(|g| sh.ems.get(g.2).copied().unwrap_or(sh.em))
                    .collect(),
                ascent: l.ascent,
                descent: l.descent,
                leading: l.leading,
            })
            .collect(),
        justify: s.justify.clamp(0, 2) as f64 * 0.5,
        spacing: s.line_spacing,
        gap: sh.gap,
    };
    let glyphs: Vec<Vec<(GlyphId, usize)>> = sh
        .lines
        .iter()
        .map(|l| l.glyphs.iter().map(|g| (g.0, g.2)).collect())
        .collect();
    let place = scene::Place {
        frame: view.frame,
        par: view.par,
        location: s.location,
        anchor: s.anchor,
        scale: s.scale,
    };
    let pose = scene::Prepared::new(&text, anim, place).pose(u);
    let ink = draw::Ink {
        fonts: &fonts,
        ems: &sh.ems,
        em: sh.em,
        glyphs: &glyphs,
    };
    Some((draw::picture(s, &ink, &pose, view), pose.card))
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
        p
    };
    // Colors written as red, green, blue and alpha, as VEGAS's are here.
    let color = |name: &CStr, label: &CStr, parent: Option<&CStr>, v: [f64; 4], hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeRGBA, name, label);
        p.set_dbls(kOfxParamPropDefault, &v);
        p.set_str(kOfxParamPropHint, hint);
        p.set_str(bv_ofx_sys::extensions::kBananaVParamPropColorModel, c"RGB");
        if let Some(parent) = parent {
            p.set_str(kOfxParamPropParent, parent);
        }
    };
    // Switches animate (in steps), as VEGAS's do.
    let toggle = |name: &CStr, label: &CStr, parent: Option<&CStr>, v: bool, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeBoolean, name, label);
        p.set_int(kOfxParamPropDefault, v as c_int);
        p.set_str(kOfxParamPropHint, hint);
        p.set_int(kOfxParamPropAnimates, 1);
        if let Some(parent) = parent {
            p.set_str(kOfxParamPropParent, parent);
        }
        p
    };
    let choice = |name: &CStr, label: &CStr, options: &[&CStr], v: i32, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeChoice, name, label);
        p.set_strs(kOfxParamPropChoiceOption, options);
        p.set_int(kOfxParamPropDefault, v);
        p.set_str(kOfxParamPropHint, hint);
        p
    };

    // The text box's bar above it, on one line (OpenFX's no-new-line
    // hint): the typeface, its size (with a word processor's sizes), bold,
    // italic, the justification's three buttons and, at its end, Transfer
    // Text Properties.
    let on_one_line = |p: Props| {
        p.set_int(kOfxParamPropLayoutHint, kOfxParamPropLayoutHintNoNewLine);
    };
    // The computer's font families, as the text box's list.
    let families: Vec<CString> = fonts::families()
        .into_iter()
        .filter_map(|f| CString::new(f).ok())
        .collect();
    let names: Vec<&CStr> = families.iter().map(|c| c.as_c_str()).collect();
    let p = fx.define_param(kOfxParamTypeStrChoice, FONT, c"Font");
    p.set_strs(kOfxParamPropChoiceOption, &names);
    p.set_strs(kOfxParamPropChoiceEnum, &names);
    let default = CString::new(default_font()).unwrap_or_default();
    p.set_str(kOfxParamPropDefault, &default);
    p.set_str(kOfxParamPropHint, c"The typeface.");
    on_one_line(p);
    let p = number(
        SIZE,
        c"Size",
        None,
        [1.0, 1000.0, 8.0, 144.0, d.size],
        c"The font's size in points (a point is a 200th of the frame's height).",
    );
    p.set_dbl(kOfxParamPropIncrement, 1.0);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_dbls(
        bv_ofx_sys::extensions::kBananaVParamPropSuggested,
        &[
            8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0,
            72.0,
        ],
    );
    on_one_line(p);
    let p = toggle(BOLD, c"Bold", None, d.bold, c"The family's bold face.");
    p.set_str(kOfxPropShortLabel, c"B");
    p.set_str(
        bv_ofx_sys::extensions::kBananaVParamPropButtonStyle,
        c"bold",
    );
    p.set_int(kOfxParamPropAnimates, 0);
    on_one_line(p);
    let p = toggle(
        ITALIC,
        c"Italic",
        None,
        d.italic,
        c"The family's italic face.",
    );
    p.set_str(kOfxPropShortLabel, c"I");
    p.set_str(
        bv_ofx_sys::extensions::kBananaVParamPropButtonStyle,
        c"italic",
    );
    p.set_int(kOfxParamPropAnimates, 0);
    on_one_line(p);
    let p = choice(
        JUSTIFY,
        c"Justification",
        &JUSTIFICATIONS,
        d.justify,
        c"How the lines line up in the text's box.",
    );
    p.set_strs(
        bv_ofx_sys::extensions::kBananaVParamPropOptionIcons,
        &[c"alignLeft", c"alignCenter", c"alignRight"],
    );
    on_one_line(p);
    let p = fx.define_param(
        kOfxParamTypePushButton,
        TRANSFER,
        c"Transfer Text Properties",
    );
    p.set_str(
        kOfxParamPropHint,
        c"The other Titles & Text events of this track take this one's look (font, size, colors, place…), keeping their own text.",
    );
    p.set_str(bv_ofx_sys::extensions::kBananaVParamPropIcon, c"transfer");
    p.set_str(
        bv_ofx_sys::extensions::kBananaVParamPropTransfer,
        c"text styles",
    );
    let p = fx.define_param(kOfxParamTypeString, TEXT, c"Text");
    p.set_str(kOfxParamPropStringMode, kOfxParamStringIsMultiLine);
    let sample = CString::new(SAMPLE).unwrap_or_default();
    p.set_str(kOfxParamPropDefault, &sample);
    p.set_str(kOfxParamPropHint, c"The text; Enter starts another line.");
    // It animates in steps, as VEGAS's does.
    p.set_int(kOfxParamPropAnimates, 1);
    // Part of it selected, the font, size, bold and italic set it alone.
    p.set_str(
        bv_ofx_sys::extensions::kBananaVParamPropRichText,
        c"styles font size bold italic",
    );
    let p = fx.define_param(kOfxParamTypeString, STYLES, c"Styles");
    p.set_str(kOfxParamPropDefault, c"");
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_int(kOfxParamPropSecret, 1);
    color(COLOR, c"Text color", None, d.color, c"The letters' color.");
    // The Animation list: kept by its name, as a `.veg` keeps it.
    let p = fx.define_param(kOfxParamTypeStrChoice, ANIMATION, c"Animation");
    let values: Vec<&CStr> = title_animations::ANIMATIONS.iter().map(|a| a.0).collect();
    let labels: Vec<&CStr> = title_animations::ANIMATIONS.iter().map(|a| a.1).collect();
    p.set_strs(kOfxParamPropChoiceOption, &labels);
    p.set_strs(kOfxParamPropChoiceEnum, &values);
    p.set_str(kOfxParamPropDefault, title_animations::NONE);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(
        kOfxParamPropHint,
        c"How the text moves over the event: its lines, words or letters coming in and going out.",
    );
    let p = number(
        SCALE,
        c"Scale",
        None,
        [0.001, 1000.0, 0.1, 10.0, d.scale],
        c"1 leaves the text's size as it is; more enlarges it, less shrinks it (about the anchor point).",
    );
    p.set_dbl(kOfxParamPropIncrement, 0.1);
    let p = fx.define_param(kOfxParamTypeDouble2D, LOCATION, c"Location");
    p.set_str(
        kOfxParamPropDoubleType,
        kOfxParamDoubleTypeNormalisedXYAbsolute,
    );
    p.set_dbls(kOfxParamPropDefault, &d.location);
    p.set_dbls(kOfxParamPropIncrement, &[0.01]);
    p.set_int(kOfxParamPropDigits, 2);
    p.set_str(
        kOfxParamPropHint,
        c"Where the anchor point goes: 0, 0 is the frame's lower left, 0.5, 0.5 its middle, 1, 1 its upper right.",
    );
    let p = choice(
        ANCHOR,
        c"Anchor Point",
        &ANCHORS,
        d.anchor,
        c"The point of the text's box that stays at the location: the text scales about it.",
    );
    p.set_int(kOfxParamPropAnimates, 1);
    group(ADVANCED, c"Advanced", false);
    toggle(
        FIT_BACKGROUND,
        c"Crop background to text",
        Some(ADVANCED),
        false,
        c"The background color only behind the text, in a box with rounded corners.",
    );
    color(
        BACKGROUND,
        c"Background",
        Some(ADVANCED),
        d.background,
        c"The frame's color behind the text (transparent: what is below shows).",
    );
    let p = number(
        TRACKING,
        c"Tracking",
        Some(ADVANCED),
        [-10.0, 10.0, -5.0, 5.0, d.tracking],
        c"The room added after each letter, in twentieths of the frame's height.",
    );
    p.set_dbl(kOfxParamPropIncrement, 0.1);
    let p = number(
        LINE_SPACING,
        c"Line spacing",
        Some(ADVANCED),
        [-10.0, 10.0, -5.0, 5.0, d.line_spacing],
        c"The distance between lines, times the font's own (under 0 they go up).",
    );
    p.set_dbl(kOfxParamPropIncrement, 0.1);
    group(OUTLINE, c"Outline", false);
    let p = number(
        OUTLINE_WIDTH,
        c"Outline width",
        Some(OUTLINE),
        [0.0, 100.0, 0.0, 10.0, d.outline_width],
        c"The outline's width along the letters' edges, half of it outside them: each unit a 400th of the frame's height (0: none).",
    );
    p.set_dbl(kOfxParamPropIncrement, 0.1);
    color(
        OUTLINE_COLOR,
        c"Outline color",
        Some(OUTLINE),
        d.outline_color,
        c"The outline's color.",
    );
    group(SHADOW, c"Shadow", false);
    toggle(
        SHADOW_ON,
        c"Shadow enable",
        Some(SHADOW),
        d.shadow,
        c"A shadow behind the text.",
    );
    color(
        SHADOW_COLOR,
        c"Shadow color",
        Some(SHADOW),
        d.shadow_color,
        c"The shadow's color.",
    );
    number(
        SHADOW_X,
        c"Shadow offset X",
        Some(SHADOW),
        [-2.0, 2.0, -1.0, 1.0, d.shadow_offset[0]],
        c"How far the shadow is to the right of the text, in tenths of the frame's width.",
    );
    number(
        SHADOW_Y,
        c"Shadow offset Y",
        Some(SHADOW),
        [-2.0, 2.0, -1.0, 1.0, d.shadow_offset[1]],
        c"How far the shadow is below the text, in tenths of the frame's width.",
    );
    number(
        SHADOW_BLUR,
        c"Shadow blur",
        Some(SHADOW),
        [0.0, 2.0, 0.0, 2.0, d.shadow_blur],
        c"How soft the shadow is: its blur's size, in tenths of the frame's width.",
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
    let fitted = fx.param(FIT_BACKGROUND).int_at(time) != 0;
    let view = draw::View {
        frame,
        par,
        k,
        size: [w, h],
    };
    // The animation, if any, plays over the generated media's length (its
    // frames; five seconds where the host does not say).
    let moving = title_animations::find(&fx.param(ANIMATION).text_at(time)).map(|anim| {
        let props = fx.props();
        let rate = props.dbl_or(kOfxImageEffectPropFrameRate, 30.0).max(1.0);
        let length = props.dbl_or(kOfxImageEffectInstancePropEffectDuration, 0.0);
        let length = if length >= 1.0 && length < rate * 3600.0 {
            length
        } else {
            rate * 5.0
        };
        animated(&s, anim, time / length, view)
    });
    let (pic, boxed) = match moving {
        Some(Some((pic, card))) => (pic, Some(card)),
        Some(None) => (None, None),
        None => (
            picture_in(&s, view),
            fitted
                .then(|| laid_out(&s, frame[0], frame[1]))
                .flatten()
                .map(|(_, l)| l.bounds),
        ),
    };
    // Fit Background Color: the background in a box around the text's
    // lines, rounded (a margin and a radius a part of the frame's height).
    let card = boxed.filter(|_| fitted).map(|[x0, y0, x1, y1]| {
        let m = 0.02 * frame[1];
        (
            [
                (x0 - m) * k[0],
                (y0 - m) * k[1],
                (x1 + m) * k[0],
                (y1 + m) * k[1],
            ],
            0.0095 * frame[1] * k[1],
        )
    });
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
            let mut v = match card {
                Some((b, r)) => {
                    let k = rounded_box(b, r, [fx_ as f64 + 0.5 - HALF, fy as f64 + 0.5 - HALF]);
                    back.map(|c| c * k)
                }
                None => back,
            };
            if let Some(p) = &pic {
                let (i, j) = (fx_ - p.region.x, fy - p.region.y);
                if i >= 0 && j >= 0 && (i as usize) < p.region.w && (j as usize) < p.region.h {
                    let at = j as usize * p.region.w + i as usize;
                    let t = p.rgba[at];
                    // Letters of a see-through color cut the background
                    // out (VEGAS's Rough Day shows what is below them).
                    let under = p.cut.as_ref().map_or(t[3], |c| t[3].max(c[at]));
                    let k = 1.0 - under;
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

/// How much of pixel `p` (its middle) a box `b` (left, top, right,
/// bottom) with corners rounded `r` covers.
fn rounded_box(b: [f64; 4], r: f64, p: [f64; 2]) -> f32 {
    let half = [(b[2] - b[0]) / 2.0, (b[3] - b[1]) / 2.0];
    let r = r.min(half[0]).min(half[1]).max(0.0);
    let q = [
        (p[0] - (b[0] + half[0])).abs() - (half[0] - r),
        (p[1] - (b[1] + half[1])).abs() - (half[1] - r),
    ];
    let outside = q[0].max(0.0).hypot(q[1].max(0.0));
    let d = outside + q[0].max(q[1]).min(0.0) - r;
    (0.5 - d).clamp(0.0, 1.0) as f32
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

/// The factory presets: the plain title, then the Title01 to Title25
/// layouts and the animated ones, each with its animation, and the
/// placeholder and subtitles looks.
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    let text = |t: &str| PresetValue::Text(t.to_string());
    let p = |name: &'static str, values: Vec<(&'static str, PresetValue)>| Preset { name, values };
    vec![
        p("(Default)", vec![]),
        p(
            "Title01",
            vec![
                ("text", text("UNIQUE\nTYPOGRAPHY")),
                ("font", text("Arial")),
                ("size", num(&[24.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title01")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.56])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title02",
            vec![
                ("text", text("DREAM BIG\nAND DARE\nTO FAIL")),
                ("font", text("Arial")),
                ("size", num(&[24.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("anchor", num(&[7.0])),
                ("animationName", text("_Title02")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title03",
            vec![
                ("text", text("FREE CUSTOMER\nSUPPORT")),
                ("styles", text("14 21 size=26.0\n")),
                ("font", text("Arial")),
                ("size", num(&[14.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title03")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.52, 0.589474])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.3])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title04",
            vec![
                (
                    "text",
                    text(
                        "The timeline editing tools make editing fast and easy.\nBut more importantly, they bring out your creativity,\nbecause ideas flow freely when you're not\npreoccupied by clumsy editing tools.",
                    ),
                ),
                ("font", text("Arial")),
                ("size", num(&[10.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title04")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title05",
            vec![
                ("text", text("THE\nWORLD\nFASHION\nFESTIVAL 2022")),
                ("font", text("Arial")),
                ("size", num(&[20.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title05")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title06",
            vec![
                ("text", text("VERY\nEASY\nTO USE")),
                ("styles", text("5 9 size=22.0\n")),
                ("font", text("Arial")),
                ("size", num(&[20.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title06")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title07",
            vec![
                ("text", text("BANANAV TITLES\n TITLES & TEXT PACK")),
                ("styles", text("15 34 size=8.0\n")),
                ("font", text("Arial")),
                ("size", num(&[22.0])),
                ("bold", num(&[1.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title07")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[-0.06])),
            ],
        ),
        p(
            "Title08",
            vec![
                ("text", text("20:00\nLIVE\nSTREAM")),
                ("styles", text("6 11 size=36.0\n11 17 size=19.0\n")),
                ("font", text("Arial")),
                ("size", num(&[10.0])),
                ("bold", num(&[1.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[2.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title08")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.825])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.125])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title09",
            vec![
                ("text", text("WELL\nORGANIZED\nPRESETS")),
                (
                    "styles",
                    text("4 5 size=24.0\n5 14 size=24.0;bold=1\n14 15 size=24.0\n"),
                ),
                ("font", text("Arial")),
                ("size", num(&[20.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title09")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title10",
            vec![
                ("text", text("BUSINESS CORPORATE\nTitle Presets Pack")),
                (
                    "styles",
                    text("18 19 size=24.0;bold=0\n19 37 size=14.0;bold=0\n"),
                ),
                ("font", text("Arial")),
                ("size", num(&[16.0])),
                ("bold", num(&[1.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title10")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title11",
            vec![
                ("text", text("AWESOME\nTYPOGRAPHY\nfor Titles & Text")),
                (
                    "styles",
                    text(
                        "7 8 size=14.0\n8 18 size=20.0;bold=1\n18 19 size=14.0\n19 36 size=11.0\n",
                    ),
                ),
                ("font", text("Arial")),
                ("size", num(&[16.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title11")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title12",
            vec![
                ("text", text("SMOOTH\nMOVEMENT\nAND CLEAN\n  DESIGN")),
                (
                    "styles",
                    text("6 7 size=24.0;bold=0\n7 25 size=12.0;bold=0\n25 26 size=10.0;bold=0\n"),
                ),
                ("font", text("Arial")),
                ("size", num(&[22.0])),
                ("bold", num(&[1.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title12")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title13",
            vec![
                (
                    "text",
                    text("HAPPY          \nHOLIDAYS\n   CELEBRATION\n OPEN NOW!"),
                ),
                (
                    "styles",
                    text(
                        "15 16 size=14.0\n16 24 size=24.0;bold=1\n24 25 size=24.0\n39 40 size=14.0\n46 49 size=24.0;bold=1\n49 50 size=24.0\n",
                    ),
                ),
                ("font", text("Arial")),
                ("size", num(&[20.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title13")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title14",
            vec![
                ("text", text("SMOOTH TITLE\nANIMATION\nPACKAGE")),
                (
                    "styles",
                    text(
                        "12 13 size=14.0\n13 22 size=24.0;bold=1\n22 23 size=24.0\n23 30 size=18.0\n",
                    ),
                ),
                ("font", text("Arial")),
                ("size", num(&[12.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title14")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.85])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title15",
            vec![
                ("text", text("#ONLY ON\nTitles & Text")),
                ("font", text("Arial")),
                ("size", num(&[20.0])),
                ("bold", num(&[1.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title15")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title16",
            vec![
                ("text", text("UNIQUE TITLES\nFOR YOUR PROJECTS")),
                ("styles", text("13 14 size=24.0\n14 31 size=13.0\n")),
                ("font", text("Arial")),
                ("size", num(&[19.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[7.0])),
                ("animationName", text("_Title16")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title17",
            vec![
                ("text", text("BANANAV STUDIO\nCREATIVE COMMUNITY")),
                ("styles", text("14 15 size=24.0\n15 33 size=11.0\n")),
                ("font", text("Arial")),
                ("size", num(&[18.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[7.0])),
                ("animationName", text("_Title17")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title18",
            vec![
                ("text", text("BANANAV LIBRARY\nTitles & Text Pack")),
                (
                    "styles",
                    text("15 16 size=24.0;bold=0\n16 34 size=12.0;bold=0\n"),
                ),
                ("font", text("Arial")),
                ("size", num(&[20.0])),
                ("bold", num(&[1.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title18")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title19",
            vec![
                ("text", text("DREAM BIG\nAND DARE\nTO FAIL")),
                ("font", text("Arial")),
                ("size", num(&[24.0])),
                ("bold", num(&[1.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[2.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title19")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.725])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title20",
            vec![
                ("text", text("TITLES & TEXT\nPACK FOR\nYOUR PROJECTS")),
                (
                    "styles",
                    text("13 14 size=14.0\n14 22 size=30.0\n22 23 size=14.0\n23 36 size=19.0\n"),
                ),
                ("font", text("Arial")),
                ("size", num(&[20.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title20")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title21",
            vec![
                ("text", text("The Good\nDesign\nAwards\n2022")),
                ("font", text("Arial")),
                ("size", num(&[22.0])),
                ("bold", num(&[1.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title21")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title22",
            vec![
                ("text", text("BANANAV\nPRESENTS")),
                ("font", text("Arial")),
                ("size", num(&[14.0])),
                ("bold", num(&[1.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title22")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title23",
            vec![
                ("text", text("MODERN\nUNIQUE\nDESIGN")),
                ("font", text("Arial")),
                ("size", num(&[20.0])),
                ("bold", num(&[1.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title23")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title24",
            vec![
                ("text", text("FOR YOUR\nPROMO")),
                ("styles", text("8 9 size=14.0\n9 14 size=24.0\n")),
                ("font", text("Arial")),
                ("size", num(&[16.0])),
                ("bold", num(&[1.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title24")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Title25",
            vec![
                ("text", text("EVENT")),
                ("font", text("Arial")),
                ("size", num(&[30.0])),
                ("bold", num(&[1.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("anchor", num(&[4.0])),
                ("animationName", text("_Title25")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.5])),
            ],
        ),
        p(
            "Action Flip",
            vec![
                ("text", text("Action Flip")),
                ("font", text("Tahoma")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Action_Flip")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[4.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[1.0, 0.0, 0.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Bounce",
            vec![
                ("text", text("Bounce")),
                ("font", text("Tahoma")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Bounce")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.391466, 0.636964])),
                ("outlineColor", num(&[0.5673, 0.93, 0.5673, 1.0])),
                ("outlineWidth", num(&[6.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[0.0, 0.5, 0.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Coming at You",
            vec![
                ("text", text("Coming at You")),
                ("font", text("Palatino Linotype")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Coming_at_You")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[1.0, 1.0, 0.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Double Flash Glow",
            vec![
                ("text", text("Double Flash")),
                ("font", text("Palatino Linotype")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Double_Flash_Glow")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.5, 0.25])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[1.0])),
                ("shadowColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[1.0, 0.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Drop Split",
            vec![
                ("text", text("Drop Split")),
                ("font", text("Arial")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Drop_split")),
                ("background", num(&[0.0, 1.0, 1.0, 1.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.543599, 0.250825])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[1.0])),
                ("shadowColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Dropping Words",
            vec![
                ("text", text("Dropping Words")),
                ("font", text("Arial")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Dropping_words")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.497217, 0.50165])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[1.0])),
                ("shadowColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Earthquake",
            vec![
                ("text", text("Earthquake")),
                ("font", text("Verdana")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Earthquake")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[0.151387, 0.064063, 0.906736, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Fall Down",
            vec![
                ("text", text("Fall Down")),
                ("font", text("Georgia")),
                ("size", num(&[48.0])),
                ("bold", num(&[1.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Fall_Down")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[0.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Float and Pop",
            vec![
                ("text", text("Float and Pop")),
                ("font", text("Georgia")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Float_and_pop")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.5, 0.2])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[1.0, 0.0, 0.5, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Fly in from Right",
            vec![
                ("text", text("Fly in from Right")),
                ("font", text("Palatino Linotype")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Fly_in_from_Right")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.51577, 0.79538])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[3.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[0.5, 0.0, 0.5, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Fly In",
            vec![
                ("text", text("Fly In")),
                ("font", text("Georgia")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Fly_in")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.254174, 0.234323])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[0.75])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Jump",
            vec![
                ("text", text("Jump")),
                ("font", text("Palatino Linotype")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Jump")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.263451, 0.39934])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[1.0, 0.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Menace",
            vec![
                ("text", text("Menace")),
                ("font", text("Verdana")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Menace")),
                ("background", num(&[1.0, 1.0, 1.0, 1.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.510204, 0.267327])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[3.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Popup",
            vec![
                ("text", text("Popup")),
                ("font", text("Verdana")),
                ("size", num(&[48.0])),
                ("bold", num(&[1.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Popup")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.324675, 0.709571])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Rolling Glow and Enlarge",
            vec![
                ("text", text("Rolling Glow")),
                ("font", text("Verdana")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Rolling_Glow_and_Enlarge")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.499072, 0.666667])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[0.0, 0.5, 0.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Rough Day",
            vec![
                ("text", text("Rough Day")),
                ("font", text("Verdana")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Rough_Day")),
                ("background", num(&[1.0, 1.0, 0.0, 1.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.653061, 0.260726])),
                ("outlineColor", num(&[0.0, 0.0, 0.0, 0.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[0.75])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[0.0, 0.0, 0.0, 0.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Scroll",
            vec![
                ("text", text("Scroll")),
                ("font", text("Verdana")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Scroll")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.187384, 0.2])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Scroll Left",
            vec![
                ("text", text("Scroll Left")),
                ("font", text("Verdana")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Scroll_Left")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.187384, 0.8])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[8.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[1.0, 0.25, 0.25, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Slide Down",
            vec![
                ("text", text("Slide Down")),
                ("font", text("Verdana")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[1.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Slide_Down")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.5, 0.33])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[0.75])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[0.0, 0.5, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Slide Left",
            vec![
                ("text", text("Slide Left")),
                ("font", text("Verdana")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Slide_Left")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.33, 0.33])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[0.0, 0.5, 0.5, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Slide Right",
            vec![
                ("text", text("Slide Right")),
                ("font", text("Verdana")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Slide_Right")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.5, 0.67])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[0.0, 0.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Slide Up",
            vec![
                ("text", text("Slide Up")),
                ("font", text("Verdana")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Slide_Up")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.5, 0.67])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[0.0, 0.0, 0.5, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Slide",
            vec![
                ("text", text("Slide")),
                ("font", text("Verdana")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[1.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Slide")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.153989, 0.79868])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[0.5])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[0.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Speedy",
            vec![
                ("text", text("Speedy")),
                ("font", text("Verdana")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Speedy")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.5, 0.33])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[1.0, 1.0, 0.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Twist In",
            vec![
                ("text", text("Twist In")),
                ("font", text("Verdana")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_Twist_In")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[1.0, 1.0, 0.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Placeholder",
            vec![
                ("text", text("Placeholder")),
                ("font", text("Tahoma")),
                ("size", num(&[48.0])),
                ("bold", num(&[0.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[0.0])),
                ("animationName", text("_None")),
                ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ("lineSpacing", num(&[1.0])),
                ("location", num(&[0.5, 0.5])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.4])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[0.0])),
                ("shadowOffsetX", num(&[0.2])),
                ("shadowOffsetY", num(&[0.2])),
                ("textColor", num(&[1.0, 0.0, 0.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
        p(
            "Subtitles",
            vec![
                ("text", text("Subtitles")),
                ("font", text("Arial")),
                ("size", num(&[10.0])),
                ("bold", num(&[1.0])),
                ("italic", num(&[0.0])),
                ("justification", num(&[1.0])),
                ("animationName", text("_None")),
                ("background", num(&[1.0, 1.0, 1.0, 0.0])),
                ("lineSpacing", num(&[0.8])),
                ("location", num(&[0.5, 0.1])),
                ("outlineColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("outlineWidth", num(&[0.0])),
                ("scale", num(&[1.0])),
                ("shadowBlur", num(&[0.01])),
                ("shadowColor", num(&[0.0, 0.0, 0.0, 1.0])),
                ("shadowEnable", num(&[1.0])),
                ("shadowOffsetX", num(&[0.01])),
                ("shadowOffsetY", num(&[0.01])),
                ("textColor", num(&[1.0, 1.0, 1.0, 1.0])),
                ("tracking", num(&[0.0])),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_edge_is_found_inside_the_pixels_it_crosses() {
        // Columns 0–3 covered, column 4 half (the edge through its
        // middle, 4.5 across), the rest not.
        let (w, h) = (10, 3);
        let cover: Vec<f32> = (0..w * h)
            .map(|i| match i % w {
                0..=3 => 1.0,
                4 => 0.5,
                _ => 0.0,
            })
            .collect();
        let d = edge_distances(&cover, w, h, true);
        let row = |x: usize| d[w + x];
        assert!(row(4).abs() < 1e-9);
        assert!((row(5) - 1.0).abs() < 1e-9 && (row(7) - 3.0).abs() < 1e-9);
        assert!((row(3) + 1.0).abs() < 1e-9 && (row(1) + 3.0).abs() < 1e-9);
        // A stroke 2 pixels wide: the edge's pixel, and half of each
        // beside it.
        let s = stroke_cover(&cover, w, h, 2.0, true);
        let row = |x: usize| s[w + x];
        assert_eq!(row(4), 1.0);
        assert!((row(3) - 0.5).abs() < 1e-6 && (row(5) - 0.5).abs() < 1e-6);
        assert_eq!((row(2), row(6)), (0.0, 0.0));
        let total: f32 = (0..w).map(row).sum();
        assert!((total - 2.0).abs() < 1e-6, "{total}");
        // Under an opaque fill: all the letters cover too.
        let s = stroke_cover(&cover, w, h, 2.0, false);
        assert_eq!((s[w + 1], s[w + 3], s[w + 6]), (1.0, 1.0, 0.0));
    }

    /// Chinese, Japanese and Korean typed in a Latin font are drawn from
    /// the computer's faces that have them, not as empty boxes; the Latin
    /// letters stay in the font asked for.
    #[test]
    #[cfg(windows)]
    fn letters_the_font_lacks_come_from_faces_that_have_them() {
        let s = Settings {
            font: "Arial".into(),
            text: "Hi 日本語 かな 한국어 中文".into(),
            ..Settings::default()
        };
        let (fonts, l) = laid_out(&s, 1920.0, 1080.0).expect("laid out");
        assert!(fonts.len() > 1, "a face of their own for the Asian letters");
        assert!(
            l.glyphs.iter().all(|g| g.0 != GlyphId::NOTDEF),
            "{:?}",
            l.glyphs
        );
        // "Hi" in Arial itself (the first style).
        assert_eq!((l.glyphs[0].2, l.glyphs[1].2), (0, 0));
        // Kana makes the text Japanese: its Han in a Japanese face.
        assert_eq!(fonts::Cjk::of("日本語 かな".chars()), fonts::Cjk::Japanese);
        assert_eq!(fonts::Cjk::of("한국어 中文".chars()), fonts::Cjk::Korean);
        assert_eq!(fonts::Cjk::of("中文".chars()), fonts::Cjk::Chinese);
        let (face, _, _) = fonts::fallback('日', fonts::Cjk::Japanese, false, false).unwrap();
        assert!(
            ["Yu Gothic", "Meiryo", "MS Gothic"].contains(&face.family.as_str()),
            "{face:?}"
        );
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
        assert!((l.em - 259.2).abs() < 1e-9, "48 points: 0.24 of 1080 lines");
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
        // Advances come in 64ths of a pixel: the same to a few of those.
        assert!((wide(&b) / wide(&a) - 2.0).abs() < 1e-4);
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
                outline_width: 4.0,
                outline_color: [0.0, 0.0, 0.0, 1.0],
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

    /// A preset's settings, its animation and whether its background is
    /// fitted to the text.
    fn preset_settings(name: &str) -> Option<(Settings, String, bool)> {
        let p = presets().into_iter().find(|p| p.name == name)?;
        // The plug-in's defaults where the preset says nothing.
        let mut s = Settings {
            font: default_font(),
            ..Settings::default()
        };
        let (mut anim, mut fitted) = (String::new(), false);
        for (k, v) in &p.values {
            match v {
                PresetValue::Text(t) => match *k {
                    "text" => s.text = t.clone(),
                    "styles" => s.runs = parse_runs(t),
                    "font" => s.font = t.clone(),
                    "animationName" => anim = t.clone(),
                    _ => {}
                },
                PresetValue::Num(v) => {
                    let x = v.first().copied().unwrap_or(0.0);
                    let rgba = |v: &[f64]| [0, 1, 2, 3].map(|i| v.get(i).copied().unwrap_or(1.0));
                    match *k {
                        "size" => s.size = x,
                        "bold" => s.bold = x >= 0.5,
                        "italic" => s.italic = x >= 0.5,
                        "justification" => s.justify = x as i32,
                        "anchor" => s.anchor = x as i32,
                        "textColor" => s.color = rgba(v),
                        "background" => s.background = rgba(v),
                        "outlineColor" => s.outline_color = rgba(v),
                        "shadowColor" => s.shadow_color = rgba(v),
                        "lineSpacing" => s.line_spacing = x,
                        "location" => s.location = [x, v.get(1).copied().unwrap_or(0.5)],
                        "outlineWidth" => s.outline_width = x,
                        "scale" => s.scale = x,
                        "shadowBlur" => s.shadow_blur = x,
                        "shadowEnable" => s.shadow = x >= 0.5,
                        "shadowOffsetX" => s.shadow_offset[0] = x,
                        "shadowOffsetY" => s.shadow_offset[1] = x,
                        "tracking" => s.tracking = x,
                        "fitBackgroundColor" => fitted = x >= 0.5,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        Some((s, anim, fitted))
    }

    /// A render's settings as VEGAS keeps them (the same units): the rich
    /// text's first font, size and justification, and the rest by name;
    /// its animation and whether its background is fitted to the text.
    fn vegas_settings(p: &serde_json::Value) -> Option<(Settings, String, bool)> {
        let rtf = p["Text"].as_str()?;
        let font = rtf
            .split("fcharset0 ")
            .nth(1)
            .and_then(|r| r.split(';').next())
            .unwrap_or("Arial")
            .trim()
            .to_string();
        let fs = rtf
            .split("\\fs")
            .nth(1)
            .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|n| n.parse::<f64>().ok())
            .unwrap_or(24.0);
        let justify = if rtf.contains("\\qc") {
            1
        } else if rtf.contains("\\qr") {
            2
        } else {
            0
        };
        let nums = |k: &str| -> Vec<f64> {
            p[k].as_array()
                .map(|a| a.iter().filter_map(|v| v.as_f64()).collect())
                .unwrap_or_default()
        };
        let num = |k: &str, d: f64| p[k].as_f64().unwrap_or(d);
        let rgba = |k: &str, d: [f64; 4]| {
            let v = nums(k);
            if v.len() >= 3 {
                [v[0], v[1], v[2], v.get(3).copied().unwrap_or(1.0)]
            } else {
                d
            }
        };
        let flag = |k: &str| p[k].as_bool().unwrap_or(false);
        let at = nums("Location");
        let mut s = Settings {
            font,
            size: fs / 2.0,
            justify,
            color: rgba("TextColor", [1.0; 4]),
            scale: num("Scale", 1.0),
            location: [
                at.first().copied().unwrap_or(0.5),
                at.get(1).copied().unwrap_or(0.5),
            ],
            anchor: p["Alignment"]["indice"].as_i64().unwrap_or(4) as i32,
            background: rgba("Background", [0.0; 4]),
            tracking: num("Tracking", 0.0),
            line_spacing: num("LineSpacing", 1.0),
            outline_width: num("OutlineWidth", 0.0),
            outline_color: rgba("OutlineColor", [1.0; 4]),
            shadow: flag("ShadowEnable"),
            shadow_color: rgba("ShadowColor", [0.0, 0.0, 0.0, 1.0]),
            shadow_offset: [num("ShadowOffsetX", 0.2), num("ShadowOffsetY", 0.2)],
            shadow_blur: num("ShadowBlur", 0.4),
            ..Settings::default()
        };
        let (mut text, runs) = rich_text(rtf, &s);
        // The last paragraph's mark ends the text, as a `.veg`'s reader
        // takes it.
        if text.ends_with('\n') {
            text.pop();
        }
        s.text = text;
        s.runs = runs;
        let anim = p["AnimationName"].as_str().unwrap_or("").to_string();
        Some((s, anim, flag("FitBackgroundColor")))
    }

    /// The text of a title's rich text (its paragraphs, letters' sizes and
    /// bold or italic), with the runs that differ from `s`'s style.
    fn rich_text(rtf: &str, s: &Settings) -> (String, Vec<Run>) {
        let b = rtf.as_bytes();
        let (mut i, mut depth) = (0usize, 0i32);
        let (mut size, mut bold, mut italic) = (s.size, s.bold, s.italic);
        let mut out: Vec<(char, f64, bool, bool)> = Vec::new();
        let mut table = false;
        while i < b.len() {
            match b[i] {
                b'{' => {
                    depth += 1;
                    i += 1;
                    if rtf[i..].starts_with("\\fonttbl") {
                        table = true;
                    }
                }
                b'}' => {
                    depth -= 1;
                    i += 1;
                    if table && depth == 1 {
                        table = false;
                    }
                    if depth == 0 {
                        break;
                    }
                }
                b'\\' => {
                    let start = i + 1;
                    let mut j = start;
                    while j < b.len() && b[j].is_ascii_alphabetic() {
                        j += 1;
                    }
                    if j == start {
                        // An escaped character or a hex one.
                        if b.get(start) == Some(&b'\'') {
                            let hex = rtf.get(start + 1..start + 3).unwrap_or("3f");
                            let c = u8::from_str_radix(hex, 16).unwrap_or(b'?') as char;
                            if !table {
                                out.push((c, size, bold, italic));
                            }
                            i = start + 3;
                        } else {
                            if !table && let Some(&c) = b.get(start) {
                                out.push((c as char, size, bold, italic));
                            }
                            i = start + 1;
                        }
                        continue;
                    }
                    let word = &rtf[start..j];
                    let mut k = j;
                    while k < b.len() && (b[k].is_ascii_digit() || b[k] == b'-') {
                        k += 1;
                    }
                    let n: Option<i64> = rtf[j..k].parse().ok();
                    if k < b.len() && b[k] == b' ' {
                        k += 1;
                    }
                    i = k;
                    if table {
                        continue;
                    }
                    match word {
                        "par" => out.push(('\n', size, bold, italic)),
                        "fs" => size = n.unwrap_or(24) as f64 / 2.0,
                        "b" => bold = n != Some(0),
                        "i" => italic = n != Some(0),
                        _ => {}
                    }
                }
                b'\r' | b'\n' => i += 1,
                _ => {
                    let c = rtf[i..].chars().next().unwrap_or(' ');
                    if !table && depth > 0 {
                        out.push((c, size, bold, italic));
                    }
                    i += c.len_utf8();
                }
            }
        }
        if out.last().is_some_and(|c| c.0 == '\n') {
            out.pop();
        }
        let text: String = out.iter().map(|c| c.0).collect();
        let mut runs: Vec<Run> = Vec::new();
        for (i, &(_, size, bold, italic)) in out.iter().enumerate() {
            let run = Run {
                start: i,
                end: i + 1,
                font: None,
                size: (size != s.size).then_some(size),
                bold: (bold != s.bold).then_some(bold),
                italic: (italic != s.italic).then_some(italic),
            };
            if run.size.is_none() && run.bold.is_none() && run.italic.is_none() {
                continue;
            }
            match runs.last_mut() {
                Some(r)
                    if r.end == i
                        && Run {
                            start: r.start,
                            end: i,
                            ..run.clone()
                        } == *r =>
                {
                    r.end = i + 1;
                }
                _ => runs.push(run),
            }
        }
        (text, runs)
    }

    /// The measured renders' Titles & Text presets drawn here at their
    /// size and moment: `BV_TITLES_RENDERS` is the folder of their
    /// `casos.json` and `quadros_cpu` (the processor's renders), made from
    /// ten-second generated media; `BV_TITLES_OUT`, when set, a folder for
    /// ours. Each one's mean and largest difference (of 255), and how far
    /// its ink's box is from theirs (pixels: left, top, right, bottom).
    #[test]
    #[ignore = "needs the measured renders"]
    fn titles_draw_as_the_measured_frames() {
        let Ok(dir) = std::env::var("BV_TITLES_RENDERS") else {
            panic!("BV_TITLES_RENDERS: the renders' folder");
        };
        let dir = std::path::PathBuf::from(dir);
        let out = std::env::var("BV_TITLES_OUT")
            .ok()
            .map(std::path::PathBuf::from);
        let cases: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("casos.json")).unwrap()).unwrap();
        let fps = cases["fps"].as_f64().unwrap_or(30.0);
        let (w, h) = (
            cases["largura"].as_u64().unwrap_or(640) as usize,
            cases["altura"].as_u64().unwrap_or(360) as usize,
        );
        let length = 10.0 * fps;
        let mut worst: f64 = 0.0;
        for q in cases["quadros"].as_array().unwrap() {
            let name = q["preset"]
                .as_str()
                .or(q["variacao"].as_str())
                .unwrap_or("-");
            if q["gerador"].as_str() != Some("Titles & Text") {
                continue;
            }
            // A preset as BananaV has it; the rest as the render was set.
            let set = if q["variacao"].as_str() == Some("preset") {
                preset_settings(name)
            } else {
                vegas_settings(&q["parametros"])
            };
            let Some((mut s, anim, fitted)) = set else {
                eprintln!("{name}: no such preset");
                continue;
            };
            // A preset whose text BananaV words its own way: the render's.
            if let Some(rtf) = q["parametros"]["Text"].as_str() {
                let (text, runs) = rich_text(rtf, &s);
                if text != s.text {
                    s.text = text;
                    s.runs = runs;
                }
            }
            let at = q["offset_quadros"].as_f64().unwrap_or(0.0);
            let view = draw::View {
                frame: [w as f64, h as f64],
                par: 1.0,
                k: [1.0, 1.0],
                size: [w, h],
            };
            let (pic, card) = match title_animations::find(&anim) {
                Some(a) => match animated(&s, a, at / length, view) {
                    Some((p, c)) => (p, Some(c)),
                    None => (None, None),
                },
                None => (
                    picture(&s, w as f64, h as f64, [1.0, 1.0], w, h),
                    laid_out(&s, w as f64, h as f64).map(|(_, l)| l.bounds),
                ),
            };
            // As the plug-in lays it over the background.
            let back = {
                let a = s.background[3].clamp(0.0, 1.0) as f32;
                [0, 1, 2]
                    .map(|c| s.background[c] as f32 * a)
                    .map(|v| v * 255.0)
            };
            let back = [back[0], back[1], back[2], s.background[3] as f32 * 255.0];
            let mut ours = vec![[0.0f32; 4]; w * h];
            for y in 0..h {
                for x in 0..w {
                    let mut v = back.map(|c| c / 255.0);
                    if fitted && let Some([x0, y0, x1, y1]) = card {
                        let m = 0.02 * h as f64;
                        let r = 0.0095 * h as f64;
                        let k = rounded_box(
                            [x0 - m, y0 - m, x1 + m, y1 + m],
                            r,
                            [x as f64 + 0.5 - HALF, y as f64 + 0.5 - HALF],
                        );
                        v = v.map(|c| c * k);
                    }
                    if let Some(p) = &pic {
                        let (i, j) = (x as i64 - p.region.x, y as i64 - p.region.y);
                        if i >= 0
                            && j >= 0
                            && (i as usize) < p.region.w
                            && (j as usize) < p.region.h
                        {
                            let at = j as usize * p.region.w + i as usize;
                            let t = p.rgba[at];
                            let under = p.cut.as_ref().map_or(t[3], |c| t[3].max(c[at]));
                            v = [0, 1, 2, 3].map(|c| t[c] + v[c] * (1.0 - under));
                        }
                    }
                    ours[y * w + x] = v.map(|c| (c.clamp(0.0, 1.0) * 255.0).round());
                }
            }
            let n = q["quadro"].as_u64().unwrap_or(0);
            let file = dir.join("quadros_cpu").join(format!("quadros_{n:06}.png"));
            let Ok(png) = std::fs::read(&file) else {
                eprintln!("{name}: no {}", file.display());
                continue;
            };
            // Their frames keep premultiplied colors.
            let img = bv_png::decode(&png).unwrap();
            let theirs: Vec<[f32; 4]> = img
                .to_rgba8()
                .chunks(4)
                .map(|p| [0, 1, 2, 3].map(|c| f32::from(p[c])))
                .collect();
            assert_eq!(theirs.len(), w * h, "{name}");
            let (mut sum, mut most) = (0.0f64, 0.0f32);
            for (a, b) in ours.iter().zip(&theirs) {
                for c in 0..4 {
                    let d = (a[c] - b[c]).abs();
                    sum += f64::from(d);
                    most = most.max(d);
                }
            }
            let mean = sum / (w * h * 4) as f64;
            // The ink: what differs from the background.
            let ink = |img: &[[f32; 4]]| {
                let mut b = [usize::MAX, usize::MAX, 0, 0];
                let mut any = false;
                for (i, p) in img.iter().enumerate() {
                    if (0..4).any(|c| (p[c] - back[c]).abs() > 64.0) {
                        let (x, y) = (i % w, i / w);
                        b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
                        any = true;
                    }
                }
                any.then_some(b)
            };
            let line = match (ink(&ours), ink(&theirs)) {
                (Some(a), Some(b)) => {
                    let d: [i64; 4] = std::array::from_fn(|i| a[i] as i64 - b[i] as i64);
                    if title_animations::find(&anim).is_some() {
                        let far = d.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
                        worst = worst.max(far as f64);
                    }
                    format!("ink {d:?} (theirs {b:?})")
                }
                (a, b) => format!("ink ours {a:?} theirs {b:?}"),
            };
            eprintln!("{n} {name} ({anim}): mean {mean:.2}, max {most:.0}, {line}");
            if let Some(o) = &out {
                std::fs::create_dir_all(o).unwrap();
                let rgba: Vec<u8> = ours.iter().flat_map(|p| p.map(|c| c as u8)).collect();
                let png = bv_png::encode_rgba8(w as u32, h as u32, &rgba, 6);
                std::fs::write(o.join(format!("bv_{n:06}.png")), png).unwrap();
            }
        }
        eprintln!("the animated ones' ink boxes at most {worst} pixels off");
    }

    /// The animated presets at moments of their media, for looking at:
    /// `cargo test -p bv-effects titles_animated_for_review -- --ignored`
    /// writes `target/telas/titles_anim_*.png` (eight moments each, over a
    /// gray checkerboard) and how long a 1080-line frame takes.
    #[test]
    #[ignore = "renders images for visual review"]
    fn titles_animated_for_review() {
        if settings().is_none() {
            return;
        }
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
        std::fs::create_dir_all(&dir).unwrap();
        let (w, h) = (640usize, 360usize);
        let moments = [0.01, 0.03, 0.06, 0.1, 0.2, 0.5, 0.8, 0.97];
        for p in presets() {
            let Some((s, anim, _)) = preset_settings(p.name) else {
                continue;
            };
            let Some(a) = title_animations::find(&anim) else {
                continue;
            };
            let view = draw::View {
                frame: [w as f64, h as f64],
                par: 1.0,
                k: [1.0, 1.0],
                size: [w, h],
            };
            let (sw, sh) = (w * 2, h * 4);
            let mut rgba = vec![0u8; sw * sh * 4];
            for (m, &u) in moments.iter().enumerate() {
                let pic = animated(&s, a, u, view).and_then(|(p, _)| p);
                let (ox, oy) = ((m % 2) * w, (m / 2) * h);
                for y in 0..h {
                    for x in 0..w {
                        let gray = if (x / 16 + y / 16) % 2 == 0 {
                            0.35
                        } else {
                            0.45
                        };
                        let mut v = [0.0f32; 4];
                        if let Some(p) = &pic {
                            let (i, j) = (x as i64 - p.region.x, y as i64 - p.region.y);
                            if i >= 0
                                && j >= 0
                                && (i as usize) < p.region.w
                                && (j as usize) < p.region.h
                            {
                                v = p.rgba[j as usize * p.region.w + i as usize];
                            }
                        }
                        let at = ((oy + y) * sw + ox + x) * 4;
                        for c in 0..3 {
                            let out = v[c] + gray * (1.0 - v[3]);
                            rgba[at + c] = (out.clamp(0.0, 1.0) * 255.0).round() as u8;
                        }
                        rgba[at + 3] = 255;
                    }
                }
            }
            let png = bv_png::encode_rgba8(sw as u32, sh as u32, &rgba, 6);
            let name = p.name.to_lowercase().replace(' ', "_");
            std::fs::write(dir.join(format!("titles_anim_{name}.png")), png).unwrap();
            // A full-size frame's time, early on (the most moving).
            let big = draw::View {
                frame: [1920.0, 1080.0],
                par: 1.0,
                k: [1.0, 1.0],
                size: [1920, 1080],
            };
            let t = std::time::Instant::now();
            for &u in &moments {
                let _ = animated(&s, a, u, big);
            }
            eprintln!(
                "{}: {:.1} ms a 1080-line frame",
                p.name,
                t.elapsed().as_secs_f64() * 1000.0 / moments.len() as f64
            );
        }
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
                            let at = j as usize * p.region.w + i as usize;
                            let t = p.rgba[at];
                            let under = p.cut.as_ref().map_or(t[3], |c| t[3].max(c[at]));
                            v = [0, 1, 2, 3].map(|c| t[c] + v[c] * (1.0 - under));
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
        let real = [
            TEXT,
            STYLES,
            FONT,
            SIZE,
            BOLD,
            ITALIC,
            JUSTIFY,
            COLOR,
            ANIMATION,
            SCALE,
            LOCATION,
            ANCHOR,
            FIT_BACKGROUND,
            BACKGROUND,
            TRACKING,
            LINE_SPACING,
            OUTLINE_WIDTH,
            OUTLINE_COLOR,
            SHADOW_ON,
            SHADOW_COLOR,
            SHADOW_X,
            SHADOW_Y,
            SHADOW_BLUR,
        ];
        for p in presets() {
            for (name, value) in &p.values {
                assert!(
                    real.iter().any(|r| r.to_bytes() == name.as_bytes()),
                    "{name}"
                );
                // Each animation one of the list's.
                if *name == "animationName" {
                    let PresetValue::Text(a) = value else {
                        panic!("{}", p.name)
                    };
                    assert!(
                        title_animations::ANIMATIONS
                            .iter()
                            .any(|(n, _)| n.to_bytes() == a.as_bytes()),
                        "{a}"
                    );
                }
            }
        }
    }
}
