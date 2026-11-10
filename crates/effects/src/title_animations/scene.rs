//! An animation played on the user's text at a moment, as VEGAS's titles
//! play theirs.
//!
//! The text is a block of lines (and one more, empty, after them: VEGAS's
//! titles end their text with a paragraph mark, whose empty line counts in
//! lists of lines and nowhere else), each line of words — a run of letters
//! and the spaces after it, the spaces opening a line a word of their own —
//! and each word of letters, spaces included. Lengths are the frame's units at a scale of
//! 1; moves are in units of a twentieth of the frame's height, up for Y.
//!
//! The template goes on first: the attributes its nodes give the element
//! at their character's place, then each stagger's element's attributes
//! copied to the elements after it in its list. Then the rows play: each
//! sets its property at the moment, a staggered row once for each element
//! of its list, each a little later. The block goes where the settings put
//! it — the location, the anchor's half box, and a nudge by the template's
//! pivot — and its own offset rows play around that place.
//!
//! What comes out is each letter's place in the frame (an affine map from
//! its own frame: its baseline's left end, y down), its opacity, and the
//! elements whose effects need a picture of their own ([`Group`]).

use super::motion;
use super::{
    Across, Animation, Attrs, Down, Effects, Element, Key, Level, List, Pivot, Property, Row,
};

/// A map of the plane: `x' = a x + c y + e`, `y' = b x + d y + f`, as
/// `[a, b, c, d, e, f]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Affine(pub [f64; 6]);

impl Affine {
    pub fn translate(x: f64, y: f64) -> Affine {
        Affine([1.0, 0.0, 0.0, 1.0, x, y])
    }

    /// Clockwise on the screen (y down) for a positive angle.
    pub fn rotate(t: f64) -> Affine {
        let (s, c) = t.sin_cos();
        Affine([c, s, -s, c, 0.0, 0.0])
    }

    pub fn scale(x: f64, y: f64) -> Affine {
        Affine([x, 0.0, 0.0, y, 0.0, 0.0])
    }

    /// `x' = x + across y`, `y' = y + down x`.
    pub fn shear(across: f64, down: f64) -> Affine {
        Affine([1.0, down, across, 1.0, 0.0, 0.0])
    }

    /// `self` after `inner`.
    pub fn then(self, inner: Affine) -> Affine {
        let [a1, b1, c1, d1, e1, f1] = self.0;
        let [a2, b2, c2, d2, e2, f2] = inner.0;
        Affine([
            a1 * a2 + c1 * b2,
            b1 * a2 + d1 * b2,
            a1 * c2 + c1 * d2,
            b1 * c2 + d1 * d2,
            a1 * e2 + c1 * f2 + e1,
            b1 * e2 + d1 * f2 + f1,
        ])
    }

    pub fn apply(self, p: [f64; 2]) -> [f64; 2] {
        let [a, b, c, d, e, f] = self.0;
        [a * p[0] + c * p[1] + e, b * p[0] + d * p[1] + f]
    }

    /// How much it scales areas.
    pub fn det(self) -> f64 {
        self.0[0] * self.0[3] - self.0[1] * self.0[2]
    }
}

/// The text laid out at a scale of 1, without its place in the frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Text {
    pub lines: Vec<TextLine>,
    /// 0 left, 0.5 centered, 1 right.
    pub justify: f64,
    /// Times the font's distance between lines.
    pub spacing: f64,
    /// The tracking setting's room after each letter (frame units).
    pub gap: f64,
}

/// A line's letters (each a character), their advances and ems, and the
/// line's ascent, descent and leading (frame units).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TextLine {
    pub chars: Vec<char>,
    pub advances: Vec<f64>,
    pub ems: Vec<f64>,
    pub ascent: f64,
    pub descent: f64,
    pub leading: f64,
}

/// Where the block goes: the frame (square units), its pixels' width over
/// height, the location (0–1 across and up), the anchor (0 top left … 8
/// bottom right) and the scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Place {
    pub frame: [f64; 2],
    pub par: f64,
    pub location: [f64; 2],
    pub anchor: i32,
    pub scale: f64,
}

/// A letter at the moment: its line, its place in the line, the map from
/// its own frame to the frame's, and its opacity (over 1: more opaque).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placed {
    pub line: usize,
    pub index: usize,
    pub to_frame: Affine,
    pub opacity: f64,
}

/// Letters drawn together into one picture, blurred (across, down) and
/// glowing by these amounts (parts of the frame's width; 0: none) before
/// it goes on its parent's.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Group {
    pub blur: [f64; 2],
    pub glow: f64,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Item {
    Letter(Placed),
    Group(Group),
}

/// The moment: the block and what is in it; the box a fitted background
/// takes (frame units: left, top, right, bottom).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Pose {
    pub root: Group,
    pub card: [f64; 4],
}

/// A word's letters `start..end` of its line, its spaces from `ink` on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    start: usize,
    ink: usize,
    end: usize,
}

/// An element of this text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Block,
    Line(usize),
    Word(usize, usize),
    /// A line's letter.
    Char(usize, usize),
}

/// A row and the elements it sets, each with its delay; a block offset
/// row's keys moved to the block's place.
struct Played {
    row: &'static Row,
    keys: Vec<Key>,
    targets: Vec<(Target, f64)>,
}

/// The text's words and lists, the template applied to it, and the rows'
/// elements: what every moment of the event shares.
pub(crate) struct Prepared<'a> {
    text: &'a Text,
    anim: &'static Animation,
    place: Place,
    /// Each line's words (the extra empty line has none).
    words: Vec<Vec<Span>>,
    /// The letters before each line (its first letter's place in the
    /// block's).
    before: Vec<usize>,
    lines: Vec<Attrs>,
    word_attrs: Vec<Vec<Attrs>>,
    chars: Vec<Vec<Attrs>>,
    rows: Vec<Played>,
    /// The block's offset (units, up): the location, the anchor and the
    /// template's pivot.
    offset: [f64; 2],
    /// The path, in frame units, widened for the text.
    path: Option<Track>,
}

/// The words of a line: a word starts at a letter after a space.
fn words_of(chars: &[char]) -> Vec<Span> {
    let mut out = Vec::new();
    let n = chars.len();
    let mut start = 0;
    for i in 1..=n {
        if i == n || (!chars[i].is_whitespace() && chars[i - 1].is_whitespace()) {
            let ink = (start..i)
                .rev()
                .find(|&k| !chars[k].is_whitespace())
                .map_or(start, |k| k + 1);
            out.push(Span { start, ink, end: i });
            start = i;
        }
    }
    out
}

/// A pivot's point in a box (left, top, right, bottom; its baseline at 0).
fn pivot_point(p: Pivot, b: [f64; 4]) -> [f64; 2] {
    match p {
        Pivot::At(across, down) => [
            match across {
                Across::Middle => 0.5 * (b[0] + b[2]),
                Across::Far => b[2],
            },
            match down {
                Down::Middle => 0.5 * (b[1] + b[3]),
                Down::Baseline => 0.0,
            },
        ],
        Pivot::Part(x, y) => [b[0] + x * (b[2] - b[0]), b[1] + y * (b[3] - b[1])],
    }
}

/// A scale as it is drawn: none is a hairline (a ten-thousandth).
fn drawn(s: f64) -> f64 {
    if s.abs() < 1e-4 || s.is_nan() {
        1e-4
    } else {
        s
    }
}

/// An element's own map: slanted, scaled and turned about its pivot in its
/// box, then moved by its offset (units of `unit`, up).
fn local(a: &Attrs, b: [f64; 4], unit: f64) -> Affine {
    let p = pivot_point(a.pivot, b);
    Affine::translate(a.offset[0] * unit + p[0], -a.offset[1] * unit + p[1])
        .then(Affine::rotate(a.rotation))
        .then(Affine::scale(drawn(a.scale[0]), drawn(a.scale[1])))
        .then(Affine::shear(a.shear, 0.0))
        .then(Affine::translate(-p[0], -p[1]))
}

/// The blur and glow an element's effects ask for now (none: nothing).
fn active(e: Option<Effects>) -> Option<([f64; 2], f64)> {
    let e = e?;
    let blur = if e.blur_on {
        e.blur.map(|v| if v > 0.0 { v } else { 0.0 })
    } else {
        [0.0; 2]
    };
    let glow = if e.glow_on && e.glow > 0.0 {
        e.glow
    } else {
        0.0
    };
    (blur[0] > 0.0 || blur[1] > 0.0 || glow > 0.0).then_some((blur, glow))
}

impl<'a> Prepared<'a> {
    /// `anim`'s template applied to `text`, the block placed by `place`.
    pub(crate) fn new(text: &'a Text, anim: &'static Animation, place: Place) -> Prepared<'a> {
        let n = text.lines.len();
        // The lines and the extra empty one.
        let mut words: Vec<Vec<Span>> = text.lines.iter().map(|l| words_of(&l.chars)).collect();
        words.push(Vec::new());
        let mut before = Vec::with_capacity(n + 1);
        let mut count = 0;
        for l in &text.lines {
            before.push(count);
            count += l.chars.len();
        }
        before.push(count);
        let mut p = Prepared {
            text,
            anim,
            place,
            lines: vec![Attrs::PLAIN; n + 1],
            word_attrs: words.iter().map(|w| vec![Attrs::PLAIN; w.len()]).collect(),
            chars: text
                .lines
                .iter()
                .map(|l| vec![Attrs::PLAIN; l.chars.len()])
                .chain(std::iter::once(Vec::new()))
                .collect(),
            words,
            before,
            rows: Vec::new(),
            offset: [0.0; 2],
            path: None,
        };
        // The template's nodes, by their character's place.
        for node in anim.nodes {
            let Some((j, i)) = p.char_at(node.index) else {
                continue;
            };
            let target = match node.level {
                Level::Line => Target::Line(j),
                Level::Word => match p.words[j].iter().position(|w| w.start <= i && i < w.end) {
                    Some(w) => Target::Word(j, w),
                    None => continue,
                },
                Level::Char => Target::Char(j, i),
            };
            if let Some(a) = p.attrs_mut(target) {
                *a = node.attrs;
            }
        }
        // Each stagger's element's attributes copied along its list.
        for s in anim.staggers {
            let Some((list, k)) = p.list(s.at, s.list) else {
                continue;
            };
            let Some(&from) = p.attrs(list[k]) else {
                continue;
            };
            for &t in &list[k + 1..] {
                if let Some(a) = p.attrs_mut(t) {
                    a.offset = from.offset;
                    a.scale = from.scale;
                    a.shear = from.shear;
                    a.rotation = from.rotation;
                    a.pivot = from.pivot;
                    a.opacity = from.opacity;
                    a.effects = from.effects;
                }
            }
        }
        let unit = place.frame[1] / 20.0;
        // The box the text takes still (units, with the scale setting).
        let [w, h] = p.still_box().map(|v| v * place.scale / unit);
        p.offset = p.block_offset(w, h);
        p.path = anim.block.path.map(|path| {
            // Points far off the frame's sides pushed out by most of the
            // text's width, so long texts still start outside.
            let push = 0.7 * w * place.frame[0] / (place.par.max(1e-9) * place.frame[1]);
            let points = path
                .points
                .iter()
                .map(|q| {
                    let dx = if q.x[1] < -10.0 {
                        -push
                    } else if q.x[1] > 10.0 {
                        push
                    } else {
                        0.0
                    };
                    (q.x.map(|v| (v + dx) * unit), q.y.map(|v| v * unit))
                })
                .collect::<Vec<_>>();
            Track::new(&points, path.wrap)
        });
        // The rows' elements.
        let mut rows = Vec::with_capacity(anim.rows.len());
        for row in anim.rows {
            let mut targets: Vec<(Target, f64)> = match row
                .stagger
                .and_then(|i| anim.staggers.get(i))
            {
                Some(s) => match p.list(s.at, s.list) {
                    Some((list, k)) => list[k..]
                        .iter()
                        .enumerate()
                        .map(|(i, &t)| (t, motion::delay(s, motion::rank(k + i, k, list.len()))))
                        .collect(),
                    None => Vec::new(),
                },
                None => p.target(row.at).map(|t| (t, 0.0)).into_iter().collect(),
            };
            // Effects only where there are effects.
            if matches!(
                row.property,
                Property::BlurAcross | Property::BlurDown | Property::Glow
            ) {
                targets.retain(|&(t, _)| {
                    t == Target::Block || p.attrs(t).is_some_and(|a| a.effects.is_some())
                });
            }
            if targets.is_empty() {
                continue;
            }
            // The block's offset rows around its place; keys far off the
            // frame pushed out by most of the text's size.
            let keys = match (row.at, row.property) {
                (Element::Block, Property::OffsetX | Property::OffsetY) => {
                    let (at, size) = match row.property {
                        Property::OffsetX => (p.offset[0], w),
                        _ => (p.offset[1], h),
                    };
                    row.keys
                        .iter()
                        .map(|k| Key {
                            value: if k.value <= -10.0 {
                                k.value - 0.7 * size
                            } else if k.value >= 10.0 {
                                k.value + 0.7 * size
                            } else {
                                k.value + at
                            },
                            ..*k
                        })
                        .collect()
                }
                _ => row.keys.to_vec(),
            };
            rows.push(Played { row, keys, targets });
        }
        p.rows = rows;
        p
    }

    /// The line and the letter at a character's place in the text (a
    /// line's end is none).
    fn char_at(&self, index: usize) -> Option<(usize, usize)> {
        let mut start = 0;
        for (j, l) in self.text.lines.iter().enumerate() {
            let len = l.chars.len();
            if index < start + len {
                return index.checked_sub(start).map(|i| (j, i));
            }
            start += len + 1;
        }
        None
    }

    /// The element, when this text has it.
    fn target(&self, e: Element) -> Option<Target> {
        match e {
            Element::Block => Some(Target::Block),
            Element::Line(j) => (j < self.words.len()).then_some(Target::Line(j)),
            Element::Word(j, w) => self.words.get(j)?.get(w).map(|_| Target::Word(j, w)),
            Element::Char(j, w, k) => {
                let span = self.words.get(j)?.get(w)?;
                (span.start + k < span.end).then_some(Target::Char(j, span.start + k))
            }
        }
    }

    /// A stagger's list and its element's place in it.
    fn list(&self, at: Element, list: List) -> Option<(Vec<Target>, usize)> {
        let t = self.target(at)?;
        let all: Vec<Target> = match (list, t) {
            (List::Lines, _) => (0..self.words.len()).map(Target::Line).collect(),
            (List::LineWords, Target::Word(j, _)) => (0..self.words[j].len())
                .map(|w| Target::Word(j, w))
                .collect(),
            (List::Words, _) => self
                .words
                .iter()
                .enumerate()
                .flat_map(|(j, ws)| (0..ws.len()).map(move |w| Target::Word(j, w)))
                .collect(),
            (List::Chars, _) => self
                .chars
                .iter()
                .enumerate()
                .flat_map(|(j, cs)| (0..cs.len()).map(move |i| Target::Char(j, i)))
                .collect(),
            _ => return None,
        };
        let k = all.iter().position(|&x| x == t)?;
        Some((all, k))
    }

    fn attrs(&self, t: Target) -> Option<&Attrs> {
        match t {
            Target::Block => None,
            Target::Line(j) => self.lines.get(j),
            Target::Word(j, w) => self.word_attrs.get(j)?.get(w),
            Target::Char(j, i) => self.chars.get(j)?.get(i),
        }
    }

    fn attrs_mut(&mut self, t: Target) -> Option<&mut Attrs> {
        match t {
            Target::Block => None,
            Target::Line(j) => self.lines.get_mut(j),
            Target::Word(j, w) => self.word_attrs.get_mut(j)?.get_mut(w),
            Target::Char(j, i) => self.chars.get_mut(j)?.get_mut(i),
        }
    }

    /// The text's box at a scale of 1 without the animation: its widest
    /// line and its lines' height (frame units), as the still text has it.
    fn still_box(&self) -> [f64; 2] {
        let lines = &self.text.lines;
        let (Some(first), Some(last)) = (lines.first(), lines.last()) else {
            return [0.0; 2];
        };
        let width = lines
            .iter()
            .map(|l| {
                l.advances.iter().sum::<f64>()
                    + self.text.gap * l.advances.len().saturating_sub(1) as f64
            })
            .fold(0.0, f64::max);
        let spacing = self.text.spacing;
        let mut y = first.ascent;
        for w in lines.windows(2) {
            y += (w[0].descent + w[0].leading.max(w[1].leading) + w[1].ascent) * spacing;
        }
        [width, y + last.descent]
    }

    /// The block's place (units from the frame's middle, up): the
    /// location, moved by half the still box toward the anchor's side, and
    /// nudged by the template's pivot (by a part of the box: a baseline
    /// pivot down by its ascent's share of ten such parts).
    fn block_offset(&self, w: f64, h: f64) -> [f64; 2] {
        let pl = &self.place;
        let unit = pl.frame[1] / 20.0;
        let k = pl.anchor.clamp(0, 8);
        let (ax, ay) = (f64::from(k % 3) * 0.5, f64::from(k / 3) * 0.5);
        let mut at = [
            (pl.location[0] - 0.5) * pl.frame[0] / unit + (0.5 - ax) * w,
            (pl.location[1] - 0.5) * 20.0 - (0.5 - ay) * h,
        ];
        if let Pivot::At(across, down) = self.anim.block.pivot {
            let (lx, ly) = (0.035 * w, 0.035 * h);
            if across == Across::Far {
                at[0] += lx;
            }
            match down {
                Down::Baseline => {
                    let (a, d) = self
                        .text
                        .lines
                        .first()
                        .map_or((0.0, 0.0), |l| (l.ascent, l.descent));
                    let q = if a + d > 0.0 { 10.0 * a / (a + d) } else { 0.0 };
                    at[1] -= ly * q;
                }
                Down::Middle => {}
            }
        }
        at
    }

    /// The moment `u` (a part of the media): every letter placed.
    pub(crate) fn pose(&self, u: f64) -> Pose {
        let text = self.text;
        let tpl = &self.anim.block;
        let pl = &self.place;
        let unit = pl.frame[1] / 20.0;
        let mut lines = self.lines.clone();
        let mut words = self.word_attrs.clone();
        let mut chars = self.chars.clone();
        let mut block = Block {
            offset: self.offset,
            scale: [pl.scale; 2],
            rotation: tpl.rotation,
            shear: tpl.shear,
            opacity: tpl.opacity,
            effects: tpl.effects,
            tracking: None,
            position: tpl.path.map_or(0.0, |p| p.position),
            selection: tpl.selection,
        };
        for played in &self.rows {
            for &(t, delay) in &played.targets {
                let v = motion::value(&played.keys, played.row.waves, u - delay);
                let attrs = match t {
                    Target::Block => {
                        block.set(played.row.property, v);
                        continue;
                    }
                    Target::Line(j) => lines.get_mut(j),
                    Target::Word(j, w) => words.get_mut(j).and_then(|ws| ws.get_mut(w)),
                    Target::Char(j, i) => chars.get_mut(j).and_then(|cs| cs.get_mut(i)),
                };
                if let Some(a) = attrs {
                    set(a, played.row.property, v);
                }
            }
        }
        // The lines laid out: the tracking (the setting's, or the
        // animation's in units), each line's own; a letter's kerning moves
        // it and the rest of its line, its line still justified as before.
        let gap = block.tracking.map_or(text.gap, |t| t * unit);
        let mut xs: Vec<Vec<f64>> = Vec::with_capacity(text.lines.len());
        let mut widths = Vec::with_capacity(text.lines.len());
        for (j, l) in text.lines.iter().enumerate() {
            let g = gap + lines[j].tracking * unit;
            let mut x = 0.0;
            let mut kern = 0.0;
            let mut at = Vec::with_capacity(l.chars.len());
            for (i, adv) in l.advances.iter().enumerate() {
                kern += chars[j][i].kerning * l.ems.get(i).copied().unwrap_or(0.0);
                at.push(x + kern);
                x += adv + g;
            }
            xs.push(at);
            widths.push(if l.advances.is_empty() { 0.0 } else { x - g });
        }
        let wide = widths.iter().copied().fold(0.0, f64::max);
        let spacing = if tpl.spacing_floor {
            text.spacing.max(1.0)
        } else {
            text.spacing
        };
        let mut baselines = Vec::with_capacity(text.lines.len());
        let mut y = 0.0;
        for (j, l) in text.lines.iter().enumerate() {
            if j > 0 {
                let p = &text.lines[j - 1];
                y += (p.descent + p.leading.max(l.leading) + l.ascent) * spacing;
            }
            baselines.push(y);
        }
        let top = text.lines.first().map_or(0.0, |l| -l.ascent);
        let bottom =
            baselines.last().copied().unwrap_or(0.0) + text.lines.last().map_or(0.0, |l| l.descent);
        let block_box = [0.0, top, wide, bottom];
        let at = [
            0.5 * pl.frame[0] + block.offset[0] * unit,
            0.5 * pl.frame[1] - block.offset[1] * unit,
        ];
        let pivot = pivot_point(tpl.pivot, block_box);
        let to_frame = Affine::translate(at[0], at[1])
            .then(Affine::rotate(block.rotation))
            .then(Affine::scale(drawn(block.scale[0]), drawn(block.scale[1])))
            .then(Affine::shear(block.shear[0], block.shear[1]))
            .then(Affine::translate(-pivot[0], -pivot[1]));
        // The letters shown.
        let total = self.before.last().copied().unwrap_or(0);
        let shown = |c: usize| -> f64 {
            if total == 0 {
                return 1.0;
            }
            let n = total as f64;
            let lo = (c as f64 / n).max(block.selection[0]);
            let hi = ((c + 1) as f64 / n).min(block.selection[1]);
            let level = ((hi - lo) * n).clamp(0.0, 1.0);
            if tpl.fade_selection {
                level
            } else {
                level.round_ties_even()
            }
        };
        let (blur, glow) = active(Some(block.effects)).unwrap_or(([0.0; 2], 0.0));
        let mut root = Group {
            blur,
            glow,
            items: Vec::new(),
        };
        let mut ink = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for (j, l) in text.lines.iter().enumerate() {
            let align = (wide - widths[j]) * text.justify;
            let line_box = [0.0, -l.ascent, widths[j], l.descent];
            let line =
                Affine::translate(align, baselines[j]).then(local(&lines[j], line_box, unit));
            let mut line_items = Vec::new();
            for (wi, span) in self.words[j].iter().enumerate() {
                let x0 = xs[j].get(span.start).copied().unwrap_or(0.0);
                let ink_w = if span.ink > span.start {
                    xs[j][span.ink - 1] + l.advances[span.ink - 1] - x0
                } else {
                    0.0
                };
                let word = Affine::translate(x0, 0.0)
                    .then(local(
                        &words[j][wi],
                        [0.0, -l.ascent, ink_w, l.descent],
                        unit,
                    ))
                    .then(Affine::translate(-x0, 0.0));
                let mut word_items = Vec::new();
                for i in span.start..span.end {
                    let adv = l.advances[i];
                    let own = local(&chars[j][i], [0.0, -l.ascent, adv, l.descent], unit);
                    let place = match &self.path {
                        Some(track) => {
                            let s = align + xs[j][i] + 0.5 * adv + block.position * track.length;
                            let (q, angle) = track.at(s);
                            Affine::translate(q[0] - align, q[1])
                                .then(Affine::rotate(angle))
                                .then(Affine::translate(-0.5 * adv, 0.0))
                        }
                        None => Affine::translate(xs[j][i], 0.0),
                    };
                    let in_block = line.then(word).then(place).then(own);
                    for p in [
                        [0.0, -l.ascent],
                        [adv, -l.ascent],
                        [0.0, l.descent],
                        [adv, l.descent],
                    ] {
                        let q = in_block.apply(p);
                        ink = [
                            ink[0].min(q[0]),
                            ink[1].min(q[1]),
                            ink[2].max(q[0]),
                            ink[3].max(q[1]),
                        ];
                    }
                    let opacity = block.opacity
                        * lines[j].opacity
                        * words[j][wi].opacity
                        * chars[j][i].opacity
                        * shown(self.before[j] + i);
                    let letter = Item::Letter(Placed {
                        line: j,
                        index: i,
                        to_frame: to_frame.then(in_block),
                        opacity,
                    });
                    word_items.push(match active(chars[j][i].effects) {
                        Some((blur, glow)) => Item::Group(Group {
                            blur,
                            glow,
                            items: vec![letter],
                        }),
                        None => letter,
                    });
                }
                match active(words[j][wi].effects) {
                    Some((blur, glow)) => line_items.push(Item::Group(Group {
                        blur,
                        glow,
                        items: word_items,
                    })),
                    None => line_items.extend(word_items),
                }
            }
            match active(lines[j].effects) {
                Some((blur, glow)) => root.items.push(Item::Group(Group {
                    blur,
                    glow,
                    items: line_items,
                })),
                None => root.items.extend(line_items),
            }
        }
        // The fitted background: the letters' box at the scale setting,
        // around the block's place.
        let size = if ink[0] <= ink[2] {
            [(ink[2] - ink[0]) * pl.scale, (ink[3] - ink[1]) * pl.scale]
        } else {
            [0.0; 2]
        };
        let card = [
            at[0] - 0.5 * size[0],
            at[1] - 0.5 * size[1],
            at[0] + 0.5 * size[0],
            at[1] + 0.5 * size[1],
        ];
        Pose { root, card }
    }
}

/// The block's own attributes at the moment.
struct Block {
    offset: [f64; 2],
    scale: [f64; 2],
    rotation: f64,
    shear: [f64; 2],
    opacity: f64,
    effects: Effects,
    /// The animation's tracking, in units (none: the setting's).
    tracking: Option<f64>,
    position: f64,
    selection: [f64; 2],
}

impl Block {
    fn set(&mut self, p: Property, v: f64) {
        match p {
            Property::OffsetX => self.offset[0] = v,
            Property::OffsetY => self.offset[1] = v,
            Property::ScaleX => self.scale[0] = v,
            Property::ScaleY => self.scale[1] = v,
            Property::Rotation => self.rotation = v,
            Property::ShearX => self.shear[0] = v,
            Property::Opacity => self.opacity = v,
            Property::BlurAcross => self.effects.blur[0] = v,
            Property::BlurDown => self.effects.blur[1] = v,
            Property::Glow => self.effects.glow = v,
            Property::Tracking => self.tracking = Some(v),
            Property::PathPosition => self.position = v,
            Property::SelectionLeft => self.selection[0] = v,
            Property::SelectionRight => self.selection[1] = v,
        }
    }
}

/// A line's, word's or letter's property set.
fn set(a: &mut Attrs, p: Property, v: f64) {
    match p {
        Property::OffsetX => a.offset[0] = v,
        Property::OffsetY => a.offset[1] = v,
        Property::ScaleX => a.scale[0] = v,
        Property::ScaleY => a.scale[1] = v,
        Property::Rotation => a.rotation = v,
        Property::ShearX => a.shear = v,
        Property::Opacity => a.opacity = v,
        Property::BlurAcross | Property::BlurDown | Property::Glow => {
            if let Some(e) = &mut a.effects {
                match p {
                    Property::BlurAcross => e.blur[0] = v,
                    Property::BlurDown => e.blur[1] = v,
                    _ => e.glow = v,
                }
            }
        }
        Property::Tracking
        | Property::PathPosition
        | Property::SelectionLeft
        | Property::SelectionRight => {}
    }
}

/// A path measured along its length: each cubic piece sampled a thousand
/// times, the chords' lengths added up.
struct Track {
    /// Each piece's four points.
    pieces: Vec<[[f64; 2]; 4]>,
    /// The length run at each sample (a thousand and one a piece).
    run: Vec<f64>,
    length: f64,
    wrap: bool,
}

const SAMPLES: usize = 1000;

fn bezier(p: &[[f64; 2]; 4], t: f64) -> [f64; 2] {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    [
        a * p[0][0] + b * p[1][0] + c * p[2][0] + d * p[3][0],
        a * p[0][1] + b * p[1][1] + c * p[2][1] + d * p[3][1],
    ]
}

/// The direction a piece runs at `t`.
fn heading(p: &[[f64; 2]; 4], t: f64) -> [f64; 2] {
    let u = 1.0 - t;
    let d = |i: usize| {
        3.0 * u * u * (p[1][i] - p[0][i])
            + 6.0 * u * t * (p[2][i] - p[1][i])
            + 3.0 * t * t * (p[3][i] - p[2][i])
    };
    let v = [d(0), d(1)];
    if v[0] == 0.0 && v[1] == 0.0 {
        // A handle on its point: the chord's way.
        [p[3][0] - p[0][0], p[3][1] - p[0][1]]
    } else {
        v
    }
}

impl Track {
    /// Through `points` (each `([in, at, out] across, [in, at, out]
    /// down)`).
    fn new(points: &[([f64; 3], [f64; 3])], wrap: bool) -> Track {
        let pieces: Vec<[[f64; 2]; 4]> = points
            .windows(2)
            .map(|w| {
                let (a, b) = (w[0], w[1]);
                [
                    [a.0[1], a.1[1]],
                    [a.0[2], a.1[2]],
                    [b.0[0], b.1[0]],
                    [b.0[1], b.1[1]],
                ]
            })
            .collect();
        let mut run = Vec::with_capacity(pieces.len() * (SAMPLES + 1));
        let mut total = 0.0;
        for p in &pieces {
            let mut prev = p[0];
            run.push(total);
            for i in 1..=SAMPLES {
                let q = bezier(p, i as f64 / SAMPLES as f64);
                total += (q[0] - prev[0]).hypot(q[1] - prev[1]);
                run.push(total);
                prev = q;
            }
        }
        Track {
            pieces,
            run,
            length: total,
            wrap,
        }
    }

    /// The point `s` along it and the way it runs there (radians, y down);
    /// beyond its ends straight on along them (or around, wrapping).
    fn at(&self, s: f64) -> ([f64; 2], f64) {
        let (Some(first), Some(last)) = (self.pieces.first(), self.pieces.last()) else {
            return ([0.0; 2], 0.0);
        };
        let angle = |v: [f64; 2]| v[1].atan2(v[0]);
        let mut s = s;
        if self.wrap && self.length > 0.0 {
            s = s.rem_euclid(self.length);
        }
        if s <= 0.0 || self.length <= 0.0 {
            let v = heading(first, 0.0);
            let n = v[0].hypot(v[1]).max(1e-12);
            return (
                [first[0][0] + s * v[0] / n, first[0][1] + s * v[1] / n],
                angle(v),
            );
        }
        if s >= self.length {
            let v = heading(last, 1.0);
            let n = v[0].hypot(v[1]).max(1e-12);
            let e = s - self.length;
            return (
                [last[3][0] + e * v[0] / n, last[3][1] + e * v[1] / n],
                angle(v),
            );
        }
        // The sample just past `s`, and the parameter between it and the
        // one before.
        let i = self
            .run
            .partition_point(|&r| r < s)
            .clamp(1, self.run.len() - 1);
        let piece = (i / (SAMPLES + 1)).min(self.pieces.len() - 1);
        let k = i - piece * (SAMPLES + 1);
        let (k0, k1) = if k == 0 { (0, 1) } else { (k - 1, k) };
        let base = piece * (SAMPLES + 1);
        let (r0, r1) = (self.run[base + k0], self.run[base + k1]);
        let f = if r1 > r0 {
            ((s - r0) / (r1 - r0)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let t = (k0 as f64 + f) / SAMPLES as f64;
        let p = &self.pieces[piece];
        (bezier(p, t), angle(heading(p, t)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::title_animations::find;

    /// A text of these lines in a font whose letters are all `adv` wide,
    /// an em of 1, ascent 0.9 and descent 0.2.
    fn text(lines: &[&str], adv: f64) -> Text {
        Text {
            lines: lines
                .iter()
                .map(|l| TextLine {
                    chars: l.chars().collect(),
                    advances: vec![adv; l.chars().count()],
                    ems: vec![1.0; l.chars().count()],
                    ascent: 0.9,
                    descent: 0.2,
                    leading: 0.0,
                })
                .collect(),
            justify: 0.5,
            spacing: 1.0,
            gap: 0.0,
        }
    }

    const PLACE: Place = Place {
        frame: [640.0, 360.0],
        par: 1.0,
        location: [0.5, 0.5],
        anchor: 4,
        scale: 1.0,
    };

    fn letters(g: &Group, out: &mut Vec<Placed>) {
        for it in &g.items {
            match it {
                Item::Letter(p) => out.push(*p),
                Item::Group(g) => letters(g, out),
            }
        }
    }

    fn all(p: &Pose) -> Vec<Placed> {
        let mut v = Vec::new();
        letters(&p.root, &mut v);
        v
    }

    #[test]
    fn words_keep_their_spaces_and_a_line_s_opening_spaces_are_one() {
        let w = words_of(&"  AND CLEAN  x".chars().collect::<Vec<_>>());
        assert_eq!(
            w,
            vec![
                Span {
                    start: 0,
                    ink: 0,
                    end: 2
                },
                Span {
                    start: 2,
                    ink: 5,
                    end: 6
                },
                Span {
                    start: 6,
                    ink: 11,
                    end: 13
                },
                Span {
                    start: 13,
                    ink: 14,
                    end: 14
                },
            ]
        );
        assert!(words_of(&[]).is_empty());
    }

    #[test]
    fn the_typewriter_shows_106_of_184_letters_halfway() {
        let t = text(
            &[
                "The timeline editing tools make editing fast and easy.",
                "But more importantly, they bring out your creativity,",
                "because ideas flow freely when you're not",
                "preoccupied by clumsy editing tools.",
            ],
            5.0,
        );
        let p = Prepared::new(&t, find("_Title04").unwrap(), PLACE);
        let shown = |u: f64| all(&p.pose(u)).iter().filter(|l| l.opacity > 0.5).count();
        assert_eq!(shown(0.5), 106);
        assert_eq!(shown(0.0), 0);
        assert_eq!(shown(0.9), 184);
        // Without fading: whole or nothing.
        assert!(
            all(&p.pose(0.3))
                .iter()
                .all(|l| l.opacity == 0.0 || l.opacity == 1.0)
        );
    }

    #[test]
    fn float_and_pop_leaves_the_letters_measured_halfway() {
        let t = text(&["Float and Pop"], 10.0);
        let p = Prepared::new(&t, find("_Float_and_pop").unwrap(), PLACE);
        // The letters still there (a space shows nothing either way).
        let shown: Vec<usize> = all(&p.pose(0.5))
            .iter()
            .filter(|l| l.opacity > 0.01 && t.lines[0].chars[l.index] != ' ')
            .map(|l| l.index)
            .collect();
        assert_eq!(shown, vec![2, 6, 7, 11, 12], "o, a, n, o, p");
    }

    #[test]
    fn dropping_words_shows_only_the_first_word_halfway() {
        let t = text(&["Dropping Words"], 10.0);
        let p = Prepared::new(&t, find("_Dropping_words").unwrap(), PLACE);
        let letters = all(&p.pose(0.5));
        // The second word still 20 units up: off the frame.
        for l in &letters {
            let y = l.to_frame.apply([0.0, 0.0])[1];
            if l.index < 9 {
                assert!(y > 0.0 && y < 360.0, "{y}");
            } else {
                assert!(y < -150.0, "{y}");
            }
        }
    }

    #[test]
    fn slide_up_lands_on_its_location_and_curve() {
        // Center pivot: the box's middle at the location plus the curve.
        let t = text(&["Slide Up"], 20.0);
        let place = Place {
            location: [0.5, 0.67],
            ..PLACE
        };
        let p = Prepared::new(&t, find("_Slide_Up").unwrap(), place);
        let pose = p.pose(0.5);
        let first = all(&pose)[0];
        // The letter's baseline: the box's middle (0.35 below the
        // baseline's 0.9 − 0.55) up 3.498 units from the frame's middle.
        let y = first.to_frame.apply([0.0, 0.0])[1];
        let want = 180.0 - (0.098330 + 3.4) * 18.0 + 0.35;
        assert!((y - want).abs() < 1e-3, "{y} {want}");
        assert!((first.opacity - 1.227711).abs() < 1e-5);
    }

    #[test]
    fn a_baseline_pivot_sits_the_first_baseline_below_the_location() {
        // Title01 (center baseline): the first line's baseline 0.035 h q
        // under the location, the box's middle across on it.
        let t = text(&["UNIQUE", "TYPOGRAPHY"], 10.0);
        let p = Prepared::new(&t, find("_Title01").unwrap(), PLACE);
        let pose = p.pose(0.5);
        let ls = all(&pose);
        let h = (0.9 + (0.2 + 0.9) + 0.2) / 18.0;
        let q = 10.0 * 0.9 / 1.1;
        let y = ls[0].to_frame.apply([0.0, 0.0])[1];
        assert!((y - (180.0 + 0.035 * h * q * 18.0)).abs() < 1e-6, "{y}");
        // Line 0 settled at +3.05 units, line 1 at 0: "UNIQUE" 3.05 units
        // right of where it would be centered.
        let x0 = ls[0].to_frame.apply([0.0, 0.0])[0];
        let x1 = ls[6].to_frame.apply([0.0, 0.0])[0];
        assert!((x0 - (x1 + 20.0 + 3.05 * 18.0)).abs() < 1e-6, "{x0} {x1}");
    }

    #[test]
    fn staggers_copy_their_element_s_effects_and_pivot() {
        // Coming at You: every letter gets the first one's blur.
        let t = text(&["Coming at You"], 10.0);
        let p = Prepared::new(&t, find("_Coming_at_You").unwrap(), PLACE);
        assert!(
            p.chars[0]
                .iter()
                .all(|a| a.effects.is_some_and(|e| e.blur_on))
        );
        // Early on, the first letters blurred in groups of their own.
        let pose = p.pose(0.01);
        assert!(pose.root.items.iter().any(|i| matches!(i, Item::Group(_))));
        // Title22: the bottom middle for every letter.
        let t = text(&["BANANAV", "PRESENTS"], 10.0);
        let p = Prepared::new(&t, find("_Title22").unwrap(), PLACE);
        for c in p.chars.iter().flatten() {
            assert!(matches!(c.pivot, Pivot::Part(x, y) if x == 0.5 && y > 0.99));
        }
    }

    #[test]
    fn a_title_s_nodes_land_by_their_character_s_place() {
        // Title01's second node (character 7) on a long first line: both
        // on line 0, line 1 plain.
        let t = text(&["LONG FIRST LINE", "B"], 10.0);
        let p = Prepared::new(&t, find("_Title01").unwrap(), PLACE);
        assert_eq!(p.lines[0].offset, [25.0, 0.0]);
        assert_eq!(p.lines[1], Attrs::PLAIN);
        // Rows on line 1's blur find no effects there.
        assert!(p.rows.iter().all(|r| {
            !(r.row.property == Property::BlurAcross && r.targets[0].0 == Target::Line(1))
        }));
    }

    #[test]
    fn the_extra_empty_line_counts_in_a_list_of_lines() {
        // Title23: three lines and the empty one, ranks 0, 1/3, 2/3 and 1
        // through its shape (a smooth 0 to 1) times 0.03.
        let t = text(&["MODERN", "UNIQUE", "DESIGN"], 10.0);
        let p = Prepared::new(&t, find("_Title23").unwrap(), PLACE);
        let r = &p.rows[0];
        let delays: Vec<f64> = r.targets.iter().map(|t| t.1).collect();
        assert_eq!(delays.len(), 4);
        let smooth = |r: f64| 0.5 * r + 1.5 * r * r - r * r * r;
        for (i, d) in delays.iter().enumerate() {
            let want = 0.03 * smooth(i as f64 / 3.0);
            assert!((d - want).abs() < 1e-12, "{delays:?}");
        }
    }

    #[test]
    fn the_scroll_runs_its_path_around() {
        let t = text(&["Scroll"], 10.0);
        let p = Prepared::new(&t, find("_Scroll_Left").unwrap(), PLACE);
        let track = p.path.as_ref().unwrap();
        // A straight path: 50 units and the push, along x.
        let w = 60.0 / 18.0;
        let want = (50.0 + 0.7 * w * 640.0 / 360.0) * 18.0;
        assert!(
            (track.length - want).abs() < 1e-6 * want,
            "{}",
            track.length
        );
        let (q, a) = track.at(100.0);
        assert!((q[0] - 100.0).abs() < 1e-3 && q[1].abs() < 1e-9 && a.abs() < 1e-9);
        // Wrapping: one length on is where it started.
        let (q2, _) = track.at(100.0 + track.length);
        assert!((q2[0] - q[0]).abs() < 1e-6);
    }
}
