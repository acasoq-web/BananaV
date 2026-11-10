//! A moment of an animated title drawn, as VEGAS's titles draw theirs:
//! each letter by itself in the text's order, over what is drawn before
//! it, its colors' opacity times the letter's (in steps of 1⁄255, over 1
//! making a see-through color more opaque) — the outlines, though, under
//! all of a picture's letters, as still titles have them, so a wide one
//! does not hide the letter before; an element with a blur or a glow
//! drawn on a picture of its own first, that picture blurred or given a
//! yellow glow under it, then laid over its parent's; the block's shadow
//! last, under it all.

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{GlyphId, MetadataProvider};

use super::motion;
use super::scene::{Affine, Group, Item, Placed, Pose};
use crate::recursive_gauss::Line;
use crate::titles::{
    EMBOLDEN, Font, HALF, Picture, Region, SLANT, STROKE, Settings, THINNEST, stroke_cover,
};

/// The glow's color: an opaque yellow.
const GLOW: [f32; 3] = [252.0 / 255.0, 1.0, 19.0 / 255.0];

/// The letters' glyphs and faces.
pub(crate) struct Ink<'a> {
    /// Each style's face.
    pub fonts: &'a [Font],
    /// Each style's em (frame units, at a scale of 1).
    pub ems: &'a [f64],
    /// The title's own em (frame units, at a scale of 1).
    pub em: f64,
    /// Each line's letters: their glyph and style.
    pub glyphs: &'a [Vec<(GlyphId, usize)>],
}

/// The frame: its size in square units, its pixels' width over height,
/// pixels to a unit across and down, and its size in pixels.
#[derive(Clone, Copy, Debug)]
pub(crate) struct View {
    pub frame: [f64; 2],
    pub par: f64,
    pub k: [f64; 2],
    pub size: [usize; 2],
}

impl View {
    /// The frame's width in pixels as a blur's or a shadow's size counts
    /// it across and down (as wide as the pixels are across and as tall as
    /// they are down), and the pixels drawn for each of those (fewer at a
    /// reduced size).
    fn widths(&self) -> ([f64; 2], [f64; 2]) {
        let par = if self.par > 0.0 { self.par } else { 1.0 };
        let width = self.frame[0] / par;
        let base = if par >= 1.0 {
            [width / par, width]
        } else {
            [width, width * par]
        };
        (base, [self.k[0] * par, self.k[1]])
    }

    /// The sigmas (pixels across and down) of a blur or glow of `v`.
    fn sigmas(&self, v: [f64; 2]) -> [f64; 2] {
        let (base, scale) = self.widths();
        [
            motion::sigma(v[0], base[0]) * scale[0],
            motion::sigma(v[1], base[1]) * scale[1],
        ]
    }
}

/// The shadow's offset (pixels right and down) and blur (sigmas, pixels
/// across and down) of `s` in `view`: tenths of the frame's width, as
/// VEGAS's titles' shadow counts them.
pub(crate) fn shadow_of(s: &Settings, view: View) -> ([f64; 2], [f64; 2]) {
    let (base, scale) = view.widths();
    let shift = [
        s.shadow_offset[0] / 10.0 * base[0] * scale[0],
        s.shadow_offset[1] / 10.0 * base[1] * scale[1],
    ];
    let blur = s.shadow_blur.max(0.0) / 10.0;
    (shift, view.sigmas([blur, blur]))
}

/// `px` (premultiplied, rows `w` long, `h` of them) laid over its shadow:
/// its alpha moved `shift` pixels right and down (between pixels too),
/// blurred `sigma` and in `color`.
pub(crate) fn lay_shadow(
    px: &mut [[f32; 4]],
    w: usize,
    h: usize,
    color: [f64; 4],
    shift: [f64; 2],
    sigma: [f64; 2],
) {
    if w == 0 || h == 0 || !shift.iter().all(|v| v.is_finite()) {
        return;
    }
    let (fx, fy) = (shift[0].floor(), shift[1].floor());
    let (ax, ay) = ((shift[0] - fx) as f32, (shift[1] - fy) as f32);
    let (dx, dy) = (fx as i64, fy as i64);
    let alpha = |x: i64, y: i64| -> f32 {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
            0.0
        } else {
            px[y as usize * w + x as usize][3]
        }
    };
    let mut sh = vec![[0.0f32; 4]; w * h];
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let (sx, sy) = (x - dx, y - dy);
            let a = alpha(sx, sy) * (1.0 - ax) * (1.0 - ay)
                + alpha(sx - 1, sy) * ax * (1.0 - ay)
                + alpha(sx, sy - 1) * (1.0 - ax) * ay
                + alpha(sx - 1, sy - 1) * ax * ay;
            sh[y as usize * w + x as usize][3] = a;
        }
    }
    blur_picture(&mut sh, w, h, sigma);
    let c = color.map(|v| v as f32);
    for (o, d) in px.iter_mut().zip(&sh) {
        let a = (c[3] * d[3]).clamp(0.0, 1.0);
        *o = over(*o, [c[0] * a, c[1] * a, c[2] * a, a]);
    }
}

/// A picture (rows `w` long, `h` of them) blurred `sigma` pixels across
/// and down, transparent around it.
pub(crate) fn blur_picture(px: &mut [[f32; 4]], w: usize, h: usize, sigma: [f64; 2]) {
    if sigma[0] > 0.2 {
        blur_lines(px, w, sigma[0]);
    }
    if sigma[1] > 0.2 {
        let mut turned = vec![[0.0f32; 4]; w * h];
        for (y, row) in px.chunks(w).enumerate() {
            for (x, p) in row.iter().enumerate() {
                turned[x * h + y] = *p;
            }
        }
        blur_lines(&mut turned, h, sigma[1]);
        for (x, col) in turned.chunks(h).enumerate() {
            for (y, p) in col.iter().enumerate() {
                px[y * w + x] = *p;
            }
        }
    }
}

/// A piece of an outline, in pixels.
#[derive(Clone, Copy, Debug)]
enum Piece {
    Line([f64; 2], [f64; 2]),
    Quad([f64; 2], [f64; 2], [f64; 2]),
    Cubic([f64; 2], [f64; 2], [f64; 2], [f64; 2]),
}

/// A glyph's outline mapped to pixels, as pieces, and the box they keep
/// to.
struct Record {
    map: Affine,
    pieces: Vec<Piece>,
    start: [f64; 2],
    at: [f64; 2],
    lo: [f64; 2],
    hi: [f64; 2],
}

impl Record {
    fn new(map: Affine) -> Record {
        Record {
            map,
            pieces: Vec::new(),
            start: [0.0; 2],
            at: [0.0; 2],
            lo: [f64::INFINITY; 2],
            hi: [f64::NEG_INFINITY; 2],
        }
    }

    fn put(&mut self, x: f32, y: f32) -> [f64; 2] {
        let p = self.map.apply([f64::from(x), f64::from(y)]);
        self.lo = [self.lo[0].min(p[0]), self.lo[1].min(p[1])];
        self.hi = [self.hi[0].max(p[0]), self.hi[1].max(p[1])];
        p
    }
}

impl OutlinePen for Record {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.start = self.put(x, y);
        self.at = self.start;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.put(x, y);
        self.pieces.push(Piece::Line(self.at, p));
        self.at = p;
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (c, p) = (self.put(cx0, cy0), self.put(x, y));
        self.pieces.push(Piece::Quad(self.at, c, p));
        self.at = p;
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (c0, c1, p) = (self.put(cx0, cy0), self.put(cx1, cy1), self.put(x, y));
        self.pieces.push(Piece::Cubic(self.at, c0, c1, p));
        self.at = p;
    }

    fn close(&mut self) {
        if self.at != self.start {
            self.pieces.push(Piece::Line(self.at, self.start));
            self.at = self.start;
        }
    }
}

/// A letter ready to draw: its outline's pieces, their box (pixels), its
/// fill and outline colors (premultiplied, whole) and its outline's width
/// (pixels).
struct Letter {
    pieces: Vec<Piece>,
    lo: [f64; 2],
    hi: [f64; 2],
    fill: [f32; 4],
    line: [f32; 4],
    width: f64,
}

/// A group ready to draw: its effects' sigmas (pixels), how far they
/// reach, and what is in it.
struct Layer {
    blur: [f64; 2],
    glow: [f64; 2],
    reach: [f64; 2],
    items: Vec<Part>,
    /// What its letters cover (pixels: left, top, right, bottom).
    extent: Option<[f64; 4]>,
}

enum Part {
    Letter(Letter),
    Layer(Layer),
}

/// An opacity's 8-bit alpha `a` times `c`, as VEGAS's titles step it.
fn alpha(a: f64, c: f64) -> f32 {
    let a8 = (a.clamp(0.0, 1.0) * 255.0).round();
    let c = if c.is_nan() { 0.0 } else { c };
    ((a8 * c + 0.5).trunc().clamp(0.0, 255.0) / 255.0) as f32
}

fn premultiplied(c: [f64; 4], a: f32) -> [f32; 4] {
    [c[0] as f32 * a, c[1] as f32 * a, c[2] as f32 * a, a]
}

fn union(a: Option<[f64; 4]>, b: [f64; 4]) -> Option<[f64; 4]> {
    Some(match a {
        Some(a) => [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ],
        None => b,
    })
}

/// How far a blur of `sigma` reaches (pixels).
fn reach(sigma: f64) -> f64 {
    if sigma > 0.0 {
        (3.0 * sigma).ceil() + 2.0
    } else {
        0.0
    }
}

/// What draws the letters: the settings' colors and outline, the faces'
/// outlines, the frame.
struct Drawer<'a> {
    s: &'a Settings,
    ink: &'a Ink<'a>,
    view: View,
    outlines: Vec<Option<skrifa::outline::OutlineGlyphCollection<'a>>>,
    /// The glows drawn (not when only what the letters cover is wanted).
    glows: bool,
}

impl<'a> Drawer<'a> {
    fn new(
        s: &'a Settings,
        ink: &'a Ink<'a>,
        view: View,
        faces: &'a [Option<skrifa::FontRef<'a>>],
        glows: bool,
    ) -> Drawer<'a> {
        Drawer {
            s,
            ink,
            view,
            outlines: faces
                .iter()
                .map(|f| f.as_ref().map(|f| f.outline_glyphs()))
                .collect(),
            glows,
        }
    }

    fn layer(&self, g: &Group) -> Layer {
        let blur = self.view.sigmas(g.blur);
        let glow = if self.glows {
            self.view.sigmas([g.glow; 2])
        } else {
            [0.0; 2]
        };
        let mut items = Vec::with_capacity(g.items.len());
        let mut extent = None;
        for it in &g.items {
            match it {
                Item::Letter(p) => {
                    if let Some(letter) = self.letter(p) {
                        // Its outline and a pixel or two around it.
                        let w = letter.width.ceil() + 2.0;
                        extent = union(
                            extent,
                            [
                                letter.lo[0] - w,
                                letter.lo[1] - w,
                                letter.hi[0] + w,
                                letter.hi[1] + w,
                            ],
                        );
                        items.push(Part::Letter(letter));
                    }
                }
                Item::Group(g) => {
                    let l = self.layer(g);
                    if let Some(e) = l.extent {
                        let r = l.reach;
                        extent =
                            union(extent, [e[0] - r[0], e[1] - r[1], e[2] + r[0], e[3] + r[1]]);
                        items.push(Part::Layer(l));
                    }
                }
            }
        }
        Layer {
            blur,
            glow,
            reach: [
                reach(blur[0]) + reach(glow[0]),
                reach(blur[1]) + reach(glow[1]),
            ],
            items,
            extent,
        }
    }

    /// A letter's outline in pixels and its colors (none: nothing shows).
    fn letter(&self, p: &Placed) -> Option<Letter> {
        let s = self.s;
        let fill_a = alpha(s.color[3], p.opacity);
        let line_a = if s.outline_width > 0.0 {
            alpha(s.outline_color[3], p.opacity)
        } else {
            0.0
        };
        if fill_a <= 0.0 && line_a <= 0.0 {
            return None;
        }
        let &(glyph, style) = self.ink.glyphs.get(p.line)?.get(p.index)?;
        let font = self.ink.fonts.get(style)?;
        let outline = self.outlines.get(style)?.as_ref()?.get(glyph)?;
        let em = self.ink.ems.get(style).copied().unwrap_or(self.ink.em);
        let to_pixels = Affine::translate(HALF, HALF)
            .then(Affine::scale(self.view.k[0], self.view.k[1]))
            .then(p.to_frame);
        let slant = if font.slant { SLANT } else { 0.0 };
        let thick: &[f64] = if font.embolden {
            &[-EMBOLDEN, 0.0, EMBOLDEN]
        } else {
            &[0.0]
        };
        let mut pieces = Vec::new();
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for &dx in thick {
            // The glyph's units (y up) to the letter's own frame (y down).
            let flip = Affine([1.0, 0.0, slant, -1.0, dx * em, 0.0]);
            let mut rec = Record::new(to_pixels.then(flip));
            let _ = outline.draw(
                DrawSettings::unhinted(Size::new(em as f32), LocationRef::default()),
                &mut rec,
            );
            rec.close();
            lo = [lo[0].min(rec.lo[0]), lo[1].min(rec.lo[1])];
            hi = [hi[0].max(rec.hi[0]), hi[1].max(rec.hi[1])];
            pieces.append(&mut rec.pieces);
        }
        if pieces.is_empty() || !(lo[0] <= hi[0] && lo[1] <= hi[1]) {
            return None;
        }
        // The outline's stroke: a part of the frame's height for each unit
        // of its width, scaled with the letter, a pixel at least.
        let width = if line_a > 0.0 {
            (s.outline_width * STROKE * self.view.frame[1] * to_pixels.det().abs().sqrt())
                .max(THINNEST)
        } else {
            0.0
        };
        Some(Letter {
            pieces,
            lo,
            hi,
            fill: premultiplied(s.color, fill_a),
            line: premultiplied(s.outline_color, line_a),
            width,
        })
    }
}

/// What a letter covers of a picture, from its pixel `x`, `y`, `w` wide:
/// its fill, and its outline when it has one.
struct Raster {
    x: i64,
    y: i64,
    w: usize,
    fill: Vec<f32>,
    edge: Option<Vec<f32>>,
}

/// A picture of part of the frame (pixels `x`, `y` on, rows from the top).
struct Canvas {
    x: i64,
    y: i64,
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
}

impl Canvas {
    /// An empty one over `r` (left, top, right, bottom) clipped to `clip`.
    fn new(r: [f64; 4], clip: [i64; 4]) -> Option<Canvas> {
        if !r.iter().all(|v| v.is_finite()) {
            return None;
        }
        let x0 = (r[0].floor() as i64).max(clip[0]);
        let y0 = (r[1].floor() as i64).max(clip[1]);
        let x1 = (r[2].ceil() as i64).min(clip[2]);
        let y1 = (r[3].ceil() as i64).min(clip[3]);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
        Some(Canvas {
            x: x0,
            y: y0,
            w,
            h,
            px: vec![[0.0; 4]; w.checked_mul(h)?],
        })
    }

    /// `top` laid over this, where they meet.
    fn lay(&mut self, top: &Canvas) {
        let x0 = self.x.max(top.x);
        let x1 = (self.x + self.w as i64).min(top.x + top.w as i64);
        let y0 = self.y.max(top.y);
        let y1 = (self.y + self.h as i64).min(top.y + top.h as i64);
        for y in y0..y1 {
            let (ri, rt) = ((y - self.y) as usize * self.w, (y - top.y) as usize * top.w);
            for x in x0..x1 {
                let t = top.px[rt + (x - top.x) as usize];
                if t[3] <= 0.0 && t[0] == 0.0 && t[1] == 0.0 && t[2] == 0.0 {
                    continue;
                }
                let o = &mut self.px[ri + (x - self.x) as usize];
                *o = over(t, *o);
            }
        }
    }

    /// What a letter covers of this picture: its fill and, with an
    /// outline, its outline (the fill and around it).
    fn raster(&self, s: &Letter) -> Option<Raster> {
        let m = s.width.ceil() + 2.0;
        let x0 = ((s.lo[0] - m).floor() as i64).max(self.x);
        let y0 = ((s.lo[1] - m).floor() as i64).max(self.y);
        let x1 = ((s.hi[0] + m).ceil() as i64).min(self.x + self.w as i64);
        let y1 = ((s.hi[1] + m).ceil() as i64).min(self.y + self.h as i64);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let (rw, rh) = ((x1 - x0) as usize, (y1 - y0) as usize);
        let mut cover = crate::text::Coverage::new(rw, rh);
        let c = [x0 as f64, y0 as f64];
        let at = |p: [f64; 2]| [p[0] - c[0], p[1] - c[1]];
        for p in &s.pieces {
            match *p {
                Piece::Line(a, b) => cover.segment(at(a), at(b)),
                Piece::Quad(a, q, b) => cover.quad(at(a), at(q), at(b)),
                Piece::Cubic(a, q, r, b) => cover.cubic(at(a), at(q), at(r), at(b)),
            }
        }
        let fill = cover.finish();
        // A see-through fill shows the inner half of the stroke.
        let edge = (s.width > 0.0).then(|| stroke_cover(&fill, rw, rh, s.width, s.fill[3] < 1.0));
        Some(Raster {
            x: x0,
            y: y0,
            w: rw,
            fill,
            edge,
        })
    }

    /// A letter's outline (`outline`) or fill laid on it in `color`.
    fn paint_raster(&mut self, r: &Raster, color: [f32; 4], outline: bool) {
        let cover = match (&r.edge, outline) {
            (Some(e), true) => e,
            (None, true) => return,
            (_, false) => &r.fill,
        };
        for (j, row) in cover.chunks(r.w.max(1)).enumerate() {
            let at = ((r.y - self.y) as usize + j) * self.w + (r.x - self.x) as usize;
            for (i, &k) in row.iter().enumerate() {
                if k <= 0.0 {
                    continue;
                }
                let o = &mut self.px[at + i];
                *o = over(color.map(|c| c * k), *o);
            }
        }
    }

    /// Blurred `sigma` pixels across and down.
    fn blur(&mut self, sigma: [f64; 2]) {
        blur_picture(&mut self.px, self.w, self.h, sigma);
    }

    /// A yellow glow under it, as far as its letters' edges blurred
    /// `sigma` reach.
    fn glow(&mut self, sigma: [f64; 2]) {
        if sigma[0] <= 0.2 && sigma[1] <= 0.2 {
            return;
        }
        let mut halo = Canvas {
            x: self.x,
            y: self.y,
            w: self.w,
            h: self.h,
            px: self.px.iter().map(|p| [p[3]; 4]).collect(),
        };
        halo.blur(sigma);
        for (o, g) in self.px.iter_mut().zip(&halo.px) {
            let a = g[3].clamp(0.0, 1.0);
            let glow = [GLOW[0] * a, GLOW[1] * a, GLOW[2] * a, a];
            *o = over(*o, glow);
        }
    }
}

/// `top` over `under` (premultiplied).
pub(crate) fn over(top: [f32; 4], under: [f32; 4]) -> [f32; 4] {
    let k = 1.0 - top[3];
    [
        top[0] + under[0] * k,
        top[1] + under[1] * k,
        top[2] + under[2] * k,
        top[3] + under[3] * k,
    ]
}

/// Rows `w` pixels long blurred `sigma` pixels along them, transparent
/// beyond their ends (the pictures are wider than what they hold).
fn blur_lines(px: &mut [[f32; 4]], w: usize, sigma: f64) {
    if w == 0 {
        return;
    }
    if sigma >= 0.6 {
        let line = Line::of_sigma(sigma);
        crate::kit::for_chunks(px, w, |_, row| line.run(row, &mut Vec::new()));
        return;
    }
    let taps = crate::spatial::gaussian(sigma);
    let r = taps.len() / 2;
    crate::kit::for_chunks(px, w, |_, row| {
        let src = row.to_vec();
        for (x, out) in row.iter_mut().enumerate() {
            let mut acc = [0.0f32; 4];
            for (t, k) in taps.iter().enumerate() {
                let Some(i) = (x + t).checked_sub(r) else {
                    continue;
                };
                if let Some(p) = src.get(i) {
                    for c in 0..4 {
                        acc[c] += p[c] * k;
                    }
                }
            }
            *out = acc;
        }
    });
}

impl Layer {
    /// This group drawn on a picture of its own — no wider than `clip`,
    /// the part of the frame (and around it) that can still show —, its
    /// effects run.
    fn draw(&self, clip: [i64; 4]) -> Option<Canvas> {
        let e = self.extent?;
        let r = self.reach;
        let mut c = Canvas::new([e[0] - r[0], e[1] - r[1], e[2] + r[0], e[3] + r[1]], clip)?;
        self.paint(&mut c, clip);
        if self.blur[0] > 0.0 || self.blur[1] > 0.0 {
            c.blur(self.blur);
        }
        c.glow(self.glow);
        Some(c)
    }

    /// Its letters and its groups on `c`, in order — its letters'
    /// outlines first, under all of its letters (as still titles have
    /// them).
    fn paint(&self, c: &mut Canvas, clip: [i64; 4]) {
        let rasters: Vec<Option<Raster>> = self
            .items
            .iter()
            .map(|it| match it {
                Part::Letter(s) => c.raster(s),
                Part::Layer(_) => None,
            })
            .collect();
        for (it, r) in self.items.iter().zip(&rasters) {
            if let (Part::Letter(s), Some(r)) = (it, r) {
                c.paint_raster(r, s.line, true);
            }
        }
        for (it, r) in self.items.iter().zip(&rasters) {
            match it {
                Part::Letter(s) => {
                    if let Some(r) = r {
                        c.paint_raster(r, s.fill, false);
                    }
                }
                Part::Layer(l) => {
                    let (rx, ry) = (l.reach[0].ceil() as i64, l.reach[1].ceil() as i64);
                    let inner = [clip[0] - rx, clip[1] - ry, clip[2] + rx, clip[3] + ry];
                    if let Some(p) = l.draw(inner) {
                        c.lay(&p);
                    }
                }
            }
        }
    }
}

/// `pose` drawn in `view`: the letters, their effects and the shadow,
/// over the part of the frame they cover.
pub(crate) fn picture(s: &Settings, ink: &Ink<'_>, pose: &Pose, view: View) -> Option<Picture> {
    let faces: Vec<Option<skrifa::FontRef<'_>>> = ink.fonts.iter().map(Font::face).collect();
    let root = Drawer::new(s, ink, view, &faces, true).layer(&pose.root);
    // Letters of a see-through color cut the background out: what they
    // cover at their opacity, drawn again opaque (no outline, no glow, no
    // shadow), blurred as they are.
    let plain;
    let cutter = if s.color[3] < 1.0 {
        plain = Settings {
            color: [1.0; 4],
            outline_width: 0.0,
            shadow: false,
            ..s.clone()
        };
        Some(Drawer::new(&plain, ink, view, &faces, false).layer(&pose.root))
    } else {
        None
    };
    let e = match (root.extent, cutter.as_ref().and_then(|c| c.extent)) {
        (Some(a), Some(b)) => [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ],
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => return None,
    };
    let [w, h] = view.size.map(|v| v as i64);
    // The shadow, as far and as soft as the still title's.
    let shadow = s.shadow.then(|| shadow_of(s, view));
    let far = shadow.map_or(0, |(shift, sigma)| {
        (shift[0].abs().max(shift[1].abs()) + 3.0 * sigma[0].max(sigma[1])).ceil() as i64 + 2
    });
    let reach = [0, 1].map(|i| {
        root.reach[i]
            .max(cutter.as_ref().map_or(0.0, |c| c.reach[i]))
            .ceil()
    });
    let (rx, ry) = (reach[0] as i64 + far, reach[1] as i64 + far);
    let clip = [-rx, -ry, w + rx, h + ry];
    let grow = [reach[0] + far as f64, reach[1] + far as f64];
    let mut c = Canvas::new(
        [
            e[0] - grow[0],
            e[1] - grow[1],
            e[2] + grow[0],
            e[3] + grow[1],
        ],
        clip,
    )?;
    root.paint(&mut c, clip);
    if root.blur[0] > 0.0 || root.blur[1] > 0.0 {
        c.blur(root.blur);
    }
    c.glow(root.glow);
    if let Some((shift, sigma)) = shadow {
        lay_shadow(&mut c.px, c.w, c.h, s.shadow_color, shift, sigma);
    }
    // What is in the frame.
    let x0 = c.x.max(0);
    let y0 = c.y.max(0);
    let x1 = (c.x + c.w as i64).min(w);
    let y1 = (c.y + c.h as i64).min(h);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (rw, rh) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let mut rgba = Vec::with_capacity(rw * rh);
    for y in y0..y1 {
        let row = (y - c.y) as usize * c.w;
        let from = row + (x0 - c.x) as usize;
        rgba.extend_from_slice(&c.px[from..from + rw]);
    }
    let cut = cutter.map(|cutter| {
        let m = Canvas::new(
            [x0 as f64, y0 as f64, x1 as f64, y1 as f64],
            [x0, y0, x1, y1],
        );
        m.map_or_else(
            || vec![0.0; rw * rh],
            |mut m| {
                cutter.paint(&mut m, clip);
                if cutter.blur[0] > 0.0 || cutter.blur[1] > 0.0 {
                    m.blur(cutter.blur);
                }
                m.px.iter().map(|p| p[3]).collect()
            },
        )
    });
    Some(Picture {
        region: Region {
            x: x0,
            y: y0,
            w: rw,
            h: rh,
        },
        rgba,
        cut,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_blurs_alike_across_and_down() {
        let mut c = Canvas::new([0.0, 0.0, 61.0, 41.0], [0, 0, 100, 100]).unwrap();
        let mid = 20 * 61 + 30;
        c.px[mid] = [1.0; 4];
        c.blur([3.0, 3.0]);
        let total: f32 = c.px.iter().map(|p| p[3]).sum();
        assert!((total - 1.0).abs() < 0.01, "{total}");
        let at = |x: usize, y: usize| c.px[y * 61 + x][3];
        assert!(
            (at(33, 20) - at(30, 23)).abs() < 1e-4,
            "{} {}",
            at(33, 20),
            at(30, 23)
        );
        assert!((at(27, 20) - at(33, 20)).abs() < 1e-4);
        assert!(at(30, 20) < 0.03 && at(30, 20) > at(33, 20));
    }

    #[test]
    fn a_letter_s_opacity_steps_its_colors_alpha() {
        assert_eq!(alpha(1.0, 1.0), 1.0);
        assert_eq!(alpha(1.0, 0.5), 128.0 / 255.0);
        // Over 1: an opaque color stays so, a see-through one gets more.
        assert_eq!(alpha(1.0, 1.3), 1.0);
        assert_eq!(alpha(0.6, 1.2), 184.0 / 255.0);
        assert_eq!(alpha(1.0, -0.5), 0.0);
        assert_eq!(alpha(1.0, f64::NAN), 0.0);
    }

    #[test]
    fn a_glow_is_yellow_under_the_letters() {
        let mut c = Canvas::new([0.0, 0.0, 40.0, 40.0], [0, 0, 40, 40]).unwrap();
        // A white bar from 15 to 25 across.
        for y in 0..40 {
            for x in 15..25 {
                c.px[y * 40 + x] = [1.0; 4];
            }
        }
        c.glow([2.0, 2.0]);
        // Inside: still white; beside it, premultiplied yellow fading out.
        assert_eq!(c.px[20 * 40 + 20], [1.0; 4]);
        let near = c.px[20 * 40 + 13];
        let far = c.px[20 * 40 + 9];
        assert!(near[3] > far[3] && far[3] > 0.0, "{near:?} {far:?}");
        assert!((near[0] - GLOW[0] * near[3]).abs() < 1e-6);
        assert!((near[2] - GLOW[2] * near[3]).abs() < 1e-6);
        assert!(c.px[20 * 40][3] < 0.01);
    }
}
