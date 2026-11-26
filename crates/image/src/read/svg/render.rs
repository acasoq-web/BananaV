//! An SVG document drawn: its elements walked in order, each one's style
//! worked out (SVG 1.1, 6: presentation attributes, then the style sheet's
//! rules, then its `style`; the inherited properties from the element
//! holding it), its shapes filled then stroked with their paints, and what
//! a group's opacity, clip path or mask changes drawn apart, over the
//! pixels its content covers, then put over the rest (14, 15.3).

use std::collections::HashMap;

use super::path::{self, Mat, P, Rect, Seg, p};
use super::raster::{self, Area, Cap, Join, Mask, Pen};
use super::style::{self, Rgba, Rule};
use super::xml::Element;
use crate::read::{Picture, PictureData};

/// How far a curve's lines may stray from it, in pixels.
const TOL: f64 = 0.12;

/// How deep groups, `use` and nested drawings may go.
const MAX_DEPTH: usize = 64;

/// How many elements a document may draw (and measure) in all: `use`
/// references reusing groups of `use` references multiply them, and a
/// hostile file can make that more than any drawing needs.
const MAX_WORK: usize = 2_000_000;

/// Elements that draw nothing where they are (used through references, or
/// not drawings).
const NOT_DRAWN: &[&str] = &[
    "defs",
    "clipPath",
    "mask",
    "linearGradient",
    "radialGradient",
    "pattern",
    "marker",
    "symbol",
    "style",
    "script",
    "title",
    "desc",
    "metadata",
    "filter",
    "font",
    "font-face",
    "foreignObject",
    "cursor",
    "view",
    "animate",
    "animateMotion",
    "animateTransform",
    "animateColor",
    "set",
];

/// A paint as a style gives it.
#[derive(Clone, Debug, PartialEq)]
enum PaintSpec {
    None,
    Color(Rgba),
    Current,
    /// A paint server, and what to paint when it is not there.
    Url(String, Option<Box<PaintSpec>>),
}

/// An element's style, worked out.
#[derive(Clone, Debug)]
struct Style {
    // Inherited.
    fill: PaintSpec,
    fill_opacity: f32,
    fill_evenodd: bool,
    clip_evenodd: bool,
    stroke: PaintSpec,
    stroke_width: f64,
    stroke_opacity: f32,
    join: Join,
    cap: Cap,
    miter_limit: f64,
    dash: Option<Vec<f64>>,
    dash_offset: f64,
    visible: bool,
    color: Rgba,
    // Not inherited.
    opacity: f32,
    clip_path: Option<String>,
    mask: Option<String>,
    shown: bool,
    stop_color: Rgba,
    stop_opacity: f32,
}

impl Style {
    /// The initial values (SVG 1.1, appendix N).
    fn initial() -> Style {
        Style {
            fill: PaintSpec::Color([0.0, 0.0, 0.0, 1.0]),
            fill_opacity: 1.0,
            fill_evenodd: false,
            clip_evenodd: false,
            stroke: PaintSpec::None,
            stroke_width: 1.0,
            stroke_opacity: 1.0,
            join: Join::Miter,
            cap: Cap::Butt,
            miter_limit: 4.0,
            dash: None,
            dash_offset: 0.0,
            visible: true,
            color: [0.0, 0.0, 0.0, 1.0],
            opacity: 1.0,
            clip_path: None,
            mask: None,
            shown: true,
            stop_color: [0.0, 0.0, 0.0, 1.0],
            stop_opacity: 1.0,
        }
    }

    /// Its inherited properties, the others back to their initial values
    /// (a child's starting point).
    fn inherited(&self) -> Style {
        let i = Style::initial();
        Style {
            opacity: i.opacity,
            clip_path: i.clip_path,
            mask: i.mask,
            shown: i.shown,
            stop_color: i.stop_color,
            stop_opacity: i.stop_opacity,
            ..self.clone()
        }
    }

    /// Property `k` set to `v` (`vp`: the viewport, for percentages).
    fn set(&mut self, k: &str, v: &str, vp: (f64, f64)) {
        let v = v.trim();
        if v == "inherit" {
            return;
        }
        let diag = ((vp.0 * vp.0 + vp.1 * vp.1) / 2.0).sqrt();
        match k {
            "fill" => {
                if let Some(p) = paint_spec(v) {
                    self.fill = p;
                }
            }
            "stroke" => {
                if let Some(p) = paint_spec(v) {
                    self.stroke = p;
                }
            }
            "fill-opacity" => set_some(&mut self.fill_opacity, style::fraction(v)),
            "stroke-opacity" => set_some(&mut self.stroke_opacity, style::fraction(v)),
            "opacity" => set_some(&mut self.opacity, style::fraction(v)),
            "stop-opacity" => set_some(&mut self.stop_opacity, style::fraction(v)),
            "fill-rule" => self.fill_evenodd = v == "evenodd",
            "clip-rule" => self.clip_evenodd = v == "evenodd",
            "stroke-width" => {
                if let Some(w) = style::length(v, diag).filter(|w| *w >= 0.0) {
                    self.stroke_width = w;
                }
            }
            "stroke-linejoin" => {
                self.join = match v {
                    "round" => Join::Round,
                    "bevel" => Join::Bevel,
                    _ => Join::Miter,
                }
            }
            "stroke-linecap" => {
                self.cap = match v {
                    "round" => Cap::Round,
                    "square" => Cap::Square,
                    _ => Cap::Butt,
                }
            }
            "stroke-miterlimit" => {
                if let Ok(m) = v.parse::<f64>()
                    && m >= 1.0
                {
                    self.miter_limit = m;
                }
            }
            "stroke-dasharray" => {
                self.dash = if v == "none" {
                    None
                } else {
                    let list: Option<Vec<f64>> = v
                        .split(|c: char| c == ',' || c.is_whitespace())
                        .filter(|s| !s.is_empty())
                        .map(|s| style::length(s, diag).filter(|l| *l >= 0.0))
                        .collect();
                    list.filter(|l| l.iter().any(|v| *v > 0.0))
                }
            }
            "stroke-dashoffset" => {
                if let Some(o) = style::length(v, diag) {
                    self.dash_offset = o;
                }
            }
            "visibility" => self.visible = v == "visible",
            "display" => self.shown = v != "none",
            "color" => {
                if let Some(c) = style::color(v) {
                    self.color = c;
                }
            }
            "stop-color" => {
                if v == "currentColor" {
                    self.stop_color = self.color;
                } else if let Some(c) = style::color(v) {
                    self.stop_color = c;
                }
            }
            "clip-path" => self.clip_path = url_id(v),
            "mask" => self.mask = url_id(v),
            _ => {}
        }
    }
}

fn set_some<T>(slot: &mut T, v: Option<T>) {
    if let Some(v) = v {
        *slot = v;
    }
}

/// The properties a style sets (also as attributes of their names).
const PROPERTIES: &[&str] = &[
    "fill",
    "fill-opacity",
    "fill-rule",
    "clip-rule",
    "stroke",
    "stroke-width",
    "stroke-opacity",
    "stroke-linejoin",
    "stroke-linecap",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "visibility",
    "display",
    "color",
    "opacity",
    "clip-path",
    "mask",
    "stop-color",
    "stop-opacity",
];

/// `url(#id)`'s id; `None` for `none` or anything else.
fn url_id(v: &str) -> Option<String> {
    let inner = v.trim().strip_prefix("url(")?;
    let close = inner.find(')')?;
    let id = inner[..close].trim().trim_matches(['"', '\'']);
    Some(id.strip_prefix('#').unwrap_or(id).to_string())
}

fn paint_spec(v: &str) -> Option<PaintSpec> {
    let v = v.trim();
    if v == "none" {
        return Some(PaintSpec::None);
    }
    if v == "currentColor" {
        return Some(PaintSpec::Current);
    }
    if v.starts_with("url(") {
        let id = url_id(v)?;
        let after = &v[v.find(')')? + 1..];
        let fallback = paint_spec(after).map(Box::new);
        return Some(PaintSpec::Url(id, fallback));
    }
    style::color(v).map(PaintSpec::Color)
}

/// Where an element draws: its transform to the pixels, the viewport its
/// percentages measure, how deep it is and the `use` references followed
/// to it.
#[derive(Clone, Debug)]
struct Ctx<'a> {
    ctm: Mat,
    vp: (f64, f64),
    depth: usize,
    uses: Vec<&'a Element>,
}

/// Pixels being drawn: color multiplied by alpha, display-encoded.
#[derive(Clone, Debug)]
pub(super) struct Canvas {
    pub area: Area,
    pub px: Vec<[f32; 4]>,
}

impl Canvas {
    pub fn new(area: Area) -> Canvas {
        Canvas {
            area,
            px: vec![[0.0; 4]; area.w * area.h],
        }
    }

    /// `paint` put over where `mask` covers, at `opacity`.
    fn fill(&mut self, mask: &Mask, paint: &Paint, opacity: f32) {
        let Some(a) = mask.area.meet(&self.area) else {
            return;
        };
        for y in a.y0..a.y1() {
            let my = (y - mask.area.y0) as usize;
            let cy = (y - self.area.y0) as usize;
            for x in a.x0..a.x1() {
                let c = mask.cov[my * mask.area.w + (x - mask.area.x0) as usize] * opacity;
                if c <= 0.0 {
                    continue;
                }
                let src = paint.at(x, y);
                let k = c;
                let s = [src[0] * k, src[1] * k, src[2] * k, src[3] * k];
                let d = &mut self.px[cy * self.area.w + (x - self.area.x0) as usize];
                let inv = 1.0 - s[3];
                for i in 0..4 {
                    d[i] = s[i] + d[i] * inv;
                }
            }
        }
    }

    /// `layer` put over, at `opacity`, where `clip` covers (all of it
    /// without one).
    fn over(&mut self, layer: &Canvas, opacity: f32, clip: Option<&Mask>) {
        let Some(a) = layer.area.meet(&self.area) else {
            return;
        };
        for y in a.y0..a.y1() {
            let ly = (y - layer.area.y0) as usize;
            let cy = (y - self.area.y0) as usize;
            for x in a.x0..a.x1() {
                let k = opacity * clip.map_or(1.0, |m| m.at(x, y));
                if k <= 0.0 {
                    continue;
                }
                let src = layer.px[ly * layer.area.w + (x - layer.area.x0) as usize];
                if src[3] <= 0.0 {
                    continue;
                }
                let s = [src[0] * k, src[1] * k, src[2] * k, src[3] * k];
                let d = &mut self.px[cy * self.area.w + (x - self.area.x0) as usize];
                let inv = 1.0 - s[3];
                for i in 0..4 {
                    d[i] = s[i] + d[i] * inv;
                }
            }
        }
    }
}

/// How a gradient goes on past its ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Spread {
    Pad,
    Reflect,
    Repeat,
}

#[derive(Clone, Copy, Debug)]
enum Shape {
    Linear {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    },
    /// Its focal point within its circle.
    Radial {
        cx: f64,
        cy: f64,
        r: f64,
        fx: f64,
        fy: f64,
    },
}

/// A paint, ready for the pixels: colors multiplied by alpha.
#[derive(Clone, Debug)]
enum Paint {
    Solid([f32; 4]),
    Gradient {
        /// From the pixels to the gradient's space.
        inv: Mat,
        shape: Shape,
        spread: Spread,
        stops: Vec<(f32, [f32; 4])>,
    },
}

fn premultiplied(c: Rgba, opacity: f32) -> [f32; 4] {
    let a = c[3] * opacity;
    [c[0] * a, c[1] * a, c[2] * a, a]
}

impl Paint {
    /// Its color at pixel (`x`, `y`)'s middle.
    fn at(&self, x: i32, y: i32) -> [f32; 4] {
        match self {
            Paint::Solid(c) => *c,
            Paint::Gradient {
                inv,
                shape,
                spread,
                stops,
            } => {
                let q = inv.apply(p(f64::from(x) + 0.5, f64::from(y) + 0.5));
                let t = match *shape {
                    Shape::Linear { x1, y1, x2, y2 } => {
                        let d = p(x2 - x1, y2 - y1);
                        let dd = d.dot(d);
                        if dd <= 0.0 {
                            1.0
                        } else {
                            (q - p(x1, y1)).dot(d) / dd
                        }
                    }
                    Shape::Radial { cx, cy, r, fx, fy } => {
                        // The circle through `q`, among those from the focal
                        // point (t = 0) to the gradient's own (t = 1).
                        let d = q - p(fx, fy);
                        let cd = p(cx - fx, cy - fy);
                        let a = cd.dot(cd) - r * r;
                        let b = d.dot(cd);
                        let c = d.dot(d);
                        if a.abs() < 1e-12 {
                            if b.abs() < 1e-12 { 0.0 } else { c / (2.0 * b) }
                        } else {
                            let disc = (b * b - a * c).max(0.0).sqrt();
                            (b - disc) / a
                        }
                    }
                };
                let t = match spread {
                    Spread::Pad => t.clamp(0.0, 1.0),
                    Spread::Repeat => t - t.floor(),
                    Spread::Reflect => {
                        let m = t.rem_euclid(2.0);
                        if m > 1.0 { 2.0 - m } else { m }
                    }
                } as f32;
                stop_color(stops, t)
            }
        }
    }
}

fn stop_color(stops: &[(f32, [f32; 4])], t: f32) -> [f32; 4] {
    let first = stops[0];
    if t <= first.0 {
        return first.1;
    }
    for w in stops.windows(2) {
        let (a, b) = (w[0], w[1]);
        if t <= b.0 {
            let span = b.0 - a.0;
            let k = if span > 0.0 { (t - a.0) / span } else { 1.0 };
            return std::array::from_fn(|i| a.1[i] + (b.1[i] - a.1[i]) * k);
        }
    }
    stops[stops.len() - 1].1
}

/// The document with what its drawing needs: its elements by id, its style
/// sheet, and how the pictures it holds are read.
pub(super) struct Doc<'a> {
    root: &'a Element,
    ids: HashMap<&'a str, &'a Element>,
    rules: Vec<Rule>,
    images: &'a dyn Fn(&str) -> Option<Picture>,
    /// Elements drawn and measured so far ([`MAX_WORK`]).
    work: std::cell::Cell<usize>,
}

impl std::fmt::Debug for Doc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Doc")
            .field("ids", &self.ids.len())
            .finish_non_exhaustive()
    }
}

/// Every element under `e` (and `e`), depth first.
fn walk<'a>(e: &'a Element, f: &mut dyn FnMut(&'a Element)) {
    let mut stack = vec![e];
    while let Some(x) = stack.pop() {
        f(x);
        for c in x.elements().collect::<Vec<_>>().into_iter().rev() {
            stack.push(c);
        }
    }
}

/// The size a document gives itself (its `width` and `height`, else its
/// `viewBox`'s, else 300 × 150) and its `viewBox`.
pub(super) fn intrinsic(root: &Element) -> (f64, f64, Option<[f64; 4]>) {
    let vb = root.attr("viewBox").and_then(|v| {
        let n = path::numbers(v);
        (n.len() == 4 && n[2] > 0.0 && n[3] > 0.0).then(|| [n[0], n[1], n[2], n[3]])
    });
    let abs = |name: &str| {
        root.attr(name)
            .filter(|v| !v.trim().ends_with('%'))
            .and_then(|v| style::length(v, 0.0))
            .filter(|v| *v > 0.0)
    };
    let (w, h) = match (abs("width"), abs("height"), vb) {
        (Some(w), Some(h), _) => (w, h),
        (Some(w), None, Some(v)) => (w, w * v[3] / v[2]),
        (None, Some(h), Some(v)) => (h * v[2] / v[3], h),
        (None, None, Some(v)) => (v[2], v[3]),
        (Some(w), None, None) => (w, 150.0),
        (None, Some(h), None) => (300.0, h),
        (None, None, None) => (300.0, 150.0),
    };
    (w, h, vb)
}

/// The transform a `viewBox` (`vb`) shown in a viewport `w` × `h` takes,
/// as its `preserveAspectRatio` (`par`) says (7.8).
fn view_box(vb: [f64; 4], w: f64, h: f64, par: Option<&str>) -> Mat {
    let par = par.unwrap_or("xMidYMid meet");
    let mut words = par.split_whitespace().filter(|w| *w != "defer");
    let align = words.next().unwrap_or("xMidYMid");
    let slice = words.next() == Some("slice");
    let (sx, sy) = (w / vb[2], h / vb[3]);
    if align == "none" {
        return Mat::scale(sx, sy).then_after(&Mat::translate(-vb[0], -vb[1]));
    }
    let s = if slice { sx.max(sy) } else { sx.min(sy) };
    let (ax, ay) = if align.len() >= 8 {
        (&align[1..4], &align[5..8])
    } else {
        ("Mid", "Mid")
    };
    let part = |a: &str| match a {
        "Min" => 0.0,
        "Max" => 1.0,
        _ => 0.5,
    };
    let tx = (w - vb[2] * s) * part(ax);
    let ty = (h - vb[3] * s) * part(ay);
    Mat::translate(tx, ty)
        .then_after(&Mat::scale(s, s))
        .then_after(&Mat::translate(-vb[0], -vb[1]))
}

impl<'a> Doc<'a> {
    pub fn new(root: &'a Element, images: &'a dyn Fn(&str) -> Option<Picture>) -> Doc<'a> {
        let mut ids = HashMap::new();
        let mut rules = Vec::new();
        walk(root, &mut |e| {
            if let Some(id) = e.attr("id") {
                ids.entry(id).or_insert(e);
            }
            if e.name == "style"
                && e.attr("type")
                    .is_none_or(|t| t.trim() == "text/css" || t.trim().is_empty())
            {
                style::sheet(&e.text(), &mut rules);
            }
        });
        Doc {
            root,
            ids,
            rules,
            images,
            work: std::cell::Cell::new(0),
        }
    }

    /// One more element drawn or measured; `false` once there were too
    /// many.
    fn spend(&self) -> bool {
        let n = self.work.get() + 1;
        self.work.set(n);
        n <= MAX_WORK
    }

    fn lookup(&self, reference: &str) -> Option<&'a Element> {
        let id = reference.trim();
        let id = id.strip_prefix('#').unwrap_or(id);
        self.ids.get(id).copied()
    }

    /// The document drawn into `w` × `h` pixels.
    pub fn render(&self, w: u32, h: u32) -> Canvas {
        let (iw, ih, vb) = intrinsic(self.root);
        let mut ctm = Mat::scale(f64::from(w) / iw, f64::from(h) / ih);
        if let Some(v) = vb {
            ctm = ctm.then_after(&view_box(v, iw, ih, self.root.attr("preserveAspectRatio")));
        }
        let vp = vb.map_or((iw, ih), |v| (v[2], v[3]));
        let ctx = Ctx {
            ctm,
            vp,
            depth: 0,
            uses: Vec::new(),
        };
        let mut canvas = Canvas::new(Area {
            x0: 0,
            y0: 0,
            w: w as usize,
            h: h as usize,
        });
        self.draw(self.root, &Style::initial(), &ctx, &mut canvas);
        canvas
    }

    /// Element `e`'s style, from `parent`'s.
    fn cascade(&self, e: &Element, parent: &Style, vp: (f64, f64)) -> Style {
        let mut s = parent.inherited();
        for &k in PROPERTIES {
            if let Some(v) = e.attr(k) {
                s.set(k, v, vp);
            }
        }
        for (k, v) in style::matching(&self.rules, e) {
            s.set(k, v, vp);
        }
        if let Some(st) = e.attr("style") {
            for (k, v) in style::declarations(st) {
                s.set(&k, &v, vp);
            }
        }
        s
    }

    /// A length attribute: along x (`Some(true)`), y (`Some(false)`) or
    /// neither (the viewport's diagonal), `default` without one.
    fn len(
        &self,
        e: &Element,
        name: &str,
        axis: Option<bool>,
        vp: (f64, f64),
        default: f64,
    ) -> f64 {
        let of = match axis {
            Some(true) => vp.0,
            Some(false) => vp.1,
            None => ((vp.0 * vp.0 + vp.1 * vp.1) / 2.0).sqrt(),
        };
        e.attr(name)
            .and_then(|v| style::length(v, of))
            .unwrap_or(default)
    }

    /// A shape's path, in its user space.
    fn geometry(&self, e: &Element, vp: (f64, f64)) -> Option<Vec<Seg>> {
        let x = |n: &str| self.len(e, n, Some(true), vp, 0.0);
        let y = |n: &str| self.len(e, n, Some(false), vp, 0.0);
        let segs = match e.name.as_str() {
            "path" => path::parse(e.attr("d")?),
            "rect" => {
                let (w, h) = (x("width"), y("height"));
                let rx = e.attr("rx").and_then(|v| style::length(v, vp.0));
                let ry = e.attr("ry").and_then(|v| style::length(v, vp.1));
                // One of them alone sets both.
                let (rx, ry) = match (rx, ry) {
                    (Some(a), Some(b)) => (a, b),
                    (Some(a), None) => (a, a),
                    (None, Some(b)) => (b, b),
                    (None, None) => (0.0, 0.0),
                };
                path::rect(x("x"), y("y"), w, h, rx, ry)
            }
            "circle" => {
                let r = self.len(e, "r", None, vp, 0.0);
                path::ellipse(x("cx"), y("cy"), r, r)
            }
            "ellipse" => path::ellipse(x("cx"), y("cy"), x("rx"), y("ry")),
            "line" => vec![
                Seg::Move(p(x("x1"), y("y1"))),
                Seg::Line(p(x("x2"), y("y2"))),
            ],
            "polyline" => path::poly(e.attr("points")?, false),
            "polygon" => path::poly(e.attr("points")?, true),
            _ => return None,
        };
        (!segs.is_empty()).then_some(segs)
    }

    /// Draws `e` (and what it holds) over `canvas`.
    fn draw(&self, e: &'a Element, parent: &Style, ctx: &Ctx<'a>, canvas: &mut Canvas) {
        if ctx.depth > MAX_DEPTH || NOT_DRAWN.contains(&e.name.as_str()) || !self.spend() {
            return;
        }
        let style = self.cascade(e, parent, ctx.vp);
        if !style.shown {
            return;
        }
        let mut ctx = ctx.clone();
        ctx.depth += 1;
        if let Some(t) = e.attr("transform") {
            match path::transform(t) {
                Some(m) => ctx.ctm = ctx.ctm.then_after(&m),
                None => return,
            }
        }
        let apart = style.opacity < 1.0 || style.clip_path.is_some() || style.mask.is_some();
        if !apart {
            self.content(e, &style, &ctx, canvas);
            return;
        }
        if style.opacity <= 0.0 {
            return;
        }
        let Some(b) = self.bounds(e, &style, &ctx, true) else {
            return;
        };
        let Some(area) = Area::around(b.x0, b.y0, b.x1, b.y1).and_then(|a| a.meet(&canvas.area))
        else {
            return;
        };
        let mut layer = Canvas::new(area);
        self.content(e, &style, &ctx, &mut layer);
        // The box clip paths' and masks' fractions are of: the element's
        // in its own space, its strokes left out.
        let own = Ctx {
            ctm: Mat::IDENTITY,
            ..ctx.clone()
        };
        let bbox = self.bounds(e, &style, &own, false);
        let mut keep: Option<Mask> = None;
        if let Some(id) = &style.clip_path
            && let Some(m) = self.clip_mask(id, bbox, &ctx, &area)
        {
            keep = Some(m);
        }
        if let Some(id) = &style.mask
            && let Some(m) = self.mask(id, bbox, &ctx, &area)
        {
            keep = Some(match keep {
                Some(mut k) => {
                    for (a, b) in k.cov.iter_mut().zip(&m.cov) {
                        *a *= b;
                    }
                    k
                }
                None => m,
            });
        }
        canvas.over(&layer, style.opacity, keep.as_ref());
    }

    /// What `e` itself draws, its style worked out and its transform
    /// applied.
    fn content(&self, e: &'a Element, style: &Style, ctx: &Ctx<'a>, canvas: &mut Canvas) {
        match e.name.as_str() {
            "svg" | "g" | "a" | "switch" => {
                let mut inner = ctx.clone();
                if e.name == "svg" && !std::ptr::eq(e, self.root) {
                    let (ctm, vp) = self.viewport(e, ctx, None);
                    inner.ctm = ctm;
                    inner.vp = vp;
                }
                for c in e.elements() {
                    self.draw(c, style, &inner, canvas);
                    // A switch draws its first child alone.
                    if e.name == "switch" && !NOT_DRAWN.contains(&c.name.as_str()) {
                        break;
                    }
                }
            }
            "use" => self.draw_use(e, style, ctx, canvas),
            "image" => self.image(e, style, ctx, canvas),
            _ => self.shape(e, style, ctx, canvas),
        }
    }

    /// A nested viewport's transform and size: an `<svg>`'s, or a symbol's
    /// drawn by a `use` (its `width` and `height` given by `sized`).
    fn viewport(&self, e: &Element, ctx: &Ctx<'a>, sized: Option<&Element>) -> (Mat, (f64, f64)) {
        let vp = ctx.vp;
        let size_of = sized.unwrap_or(e);
        let x = if sized.is_some() {
            0.0
        } else {
            self.len(e, "x", Some(true), vp, 0.0)
        };
        let y = if sized.is_some() {
            0.0
        } else {
            self.len(e, "y", Some(false), vp, 0.0)
        };
        let w = size_of
            .attr("width")
            .or_else(|| e.attr("width"))
            .and_then(|v| style::length(v, vp.0))
            .unwrap_or(vp.0);
        let h = size_of
            .attr("height")
            .or_else(|| e.attr("height"))
            .and_then(|v| style::length(v, vp.1))
            .unwrap_or(vp.1);
        let vb = e.attr("viewBox").and_then(|v| {
            let n = path::numbers(v);
            (n.len() == 4 && n[2] > 0.0 && n[3] > 0.0).then(|| [n[0], n[1], n[2], n[3]])
        });
        let mut ctm = ctx.ctm.then_after(&Mat::translate(x, y));
        if let Some(v) = vb {
            ctm = ctm.then_after(&view_box(v, w, h, e.attr("preserveAspectRatio")));
        }
        (ctm, vb.map_or((w, h), |v| (v[2], v[3])))
    }

    fn draw_use(&self, e: &'a Element, style: &Style, ctx: &Ctx<'a>, canvas: &mut Canvas) {
        let Some(target) = e.href().and_then(|h| self.lookup(h)) else {
            return;
        };
        if ctx.uses.iter().any(|u| std::ptr::eq(*u, target)) || std::ptr::eq(target, e) {
            return;
        }
        let mut inner = ctx.clone();
        inner.uses.push(target);
        let x = self.len(e, "x", Some(true), ctx.vp, 0.0);
        let y = self.len(e, "y", Some(false), ctx.vp, 0.0);
        inner.ctm = ctx.ctm.then_after(&Mat::translate(x, y));
        if target.name == "symbol" {
            let sym = self.cascade(target, style, inner.vp);
            if !sym.shown {
                return;
            }
            let (ctm, vp) = self.viewport(target, &inner, Some(e));
            inner.ctm = ctm;
            inner.vp = vp;
            for c in target.elements() {
                self.draw(c, &sym, &inner, canvas);
            }
        } else {
            self.draw(target, style, &inner, canvas);
        }
    }

    /// A shape filled, then stroked.
    fn shape(&self, e: &Element, style: &Style, ctx: &Ctx<'a>, canvas: &mut Canvas) {
        if !style.visible {
            return;
        }
        let Some(segs) = self.geometry(e, ctx.vp) else {
            return;
        };
        let scale = ctx.ctm.max_scale();
        if scale.is_nan() || scale <= 1e-12 {
            return;
        }
        let tol = TOL / scale;
        let lines = path::flatten(&segs, tol);
        let bbox = path::bounds(&segs);
        let to_pixels = |polys: &[Vec<P>]| -> Vec<Vec<P>> {
            polys
                .iter()
                .map(|v| v.iter().map(|q| ctx.ctm.apply(*q)).collect())
                .collect()
        };
        if style.fill != PaintSpec::None
            && let Some(paint) = self.paint(&style.fill, style, bbox, ctx, style.fill_opacity)
        {
            let polys: Vec<Vec<P>> = lines.iter().map(|(v, _)| v.clone()).collect();
            if let Some(m) = raster::fill(&to_pixels(&polys), style.fill_evenodd, &canvas.area) {
                canvas.fill(&m, &paint, 1.0);
            }
        }
        if style.stroke != PaintSpec::None
            && style.stroke_width > 0.0
            && let Some(paint) = self.paint(&style.stroke, style, bbox, ctx, style.stroke_opacity)
        {
            let dashed;
            let lines = match &style.dash {
                Some(d) => {
                    dashed = raster::dash(&lines, d, style.dash_offset);
                    &dashed
                }
                None => &lines,
            };
            let pen = Pen {
                width: style.stroke_width,
                join: style.join,
                cap: style.cap,
                miter_limit: style.miter_limit,
                tol,
            };
            let polys = raster::stroke(lines, &pen);
            if let Some(m) = raster::fill(&to_pixels(&polys), false, &canvas.area) {
                canvas.fill(&m, &paint, 1.0);
            }
        }
    }

    /// The paint `spec` stands for, at `opacity`; `None` for none.
    fn paint(
        &self,
        spec: &PaintSpec,
        style: &Style,
        bbox: Option<Rect>,
        ctx: &Ctx<'a>,
        opacity: f32,
    ) -> Option<Paint> {
        match spec {
            PaintSpec::None => None,
            PaintSpec::Color(c) => Some(Paint::Solid(premultiplied(*c, opacity))),
            PaintSpec::Current => Some(Paint::Solid(premultiplied(style.color, opacity))),
            PaintSpec::Url(id, fallback) => match self.lookup(id) {
                Some(g) if g.name == "linearGradient" || g.name == "radialGradient" => {
                    self.gradient(g, bbox, ctx, opacity)
                }
                _ => fallback
                    .as_deref()
                    .and_then(|f| self.paint(f, style, bbox, ctx, opacity)),
            },
        }
    }

    /// A gradient as paint (13.2): its attributes and stops, from those it
    /// refers to (`href`) where it has none of its own.
    fn gradient(
        &self,
        g: &'a Element,
        bbox: Option<Rect>,
        ctx: &Ctx<'a>,
        opacity: f32,
    ) -> Option<Paint> {
        let mut chain = vec![g];
        while chain.len() < 16 {
            let last = chain[chain.len() - 1];
            match last.href().and_then(|h| self.lookup(h)) {
                Some(n)
                    if (n.name == "linearGradient" || n.name == "radialGradient")
                        && !chain.iter().any(|c| std::ptr::eq(*c, n)) =>
                {
                    chain.push(n)
                }
                _ => break,
            }
        }
        let attr = |name: &str| chain.iter().find_map(|e| e.attr(name));
        let user = attr("gradientUnits").is_some_and(|u| u.trim() == "userSpaceOnUse");
        let base = if user {
            Mat::IDENTITY
        } else {
            let b = bbox.filter(|b| b.width() > 0.0 && b.height() > 0.0)?;
            Mat {
                a: b.width(),
                b: 0.0,
                c: 0.0,
                d: b.height(),
                e: b.x0,
                f: b.y0,
            }
        };
        let gt = match attr("gradientTransform") {
            Some(t) => path::transform(t)?,
            None => Mat::IDENTITY,
        };
        let m = ctx.ctm.then_after(&base).then_after(&gt);
        // Coordinates: fractions of the box, or user lengths.
        let coord = |name: &str, axis: Option<bool>, default: f64| -> f64 {
            let Some(v) = attr(name) else {
                return default;
            };
            if user {
                let of = match axis {
                    Some(true) => ctx.vp.0,
                    Some(false) => ctx.vp.1,
                    None => ((ctx.vp.0 * ctx.vp.0 + ctx.vp.1 * ctx.vp.1) / 2.0).sqrt(),
                };
                style::length(v, of).unwrap_or(default)
            } else {
                let v = v.trim();
                match v.strip_suffix('%') {
                    Some(pc) => pc.trim().parse::<f64>().map_or(default, |x| x / 100.0),
                    None => style::length(v, 1.0).unwrap_or(default),
                }
            }
        };
        let full = |axis: Option<bool>| -> f64 {
            if !user {
                return 1.0;
            }
            match axis {
                Some(true) => ctx.vp.0,
                Some(false) => ctx.vp.1,
                None => ((ctx.vp.0 * ctx.vp.0 + ctx.vp.1 * ctx.vp.1) / 2.0).sqrt(),
            }
        };
        let shape = if g.name == "linearGradient" {
            Shape::Linear {
                x1: coord("x1", Some(true), 0.0),
                y1: coord("y1", Some(false), 0.0),
                x2: coord("x2", Some(true), full(Some(true))),
                y2: coord("y2", Some(false), 0.0),
            }
        } else {
            let cx = coord("cx", Some(true), full(Some(true)) * 0.5);
            let cy = coord("cy", Some(false), full(Some(false)) * 0.5);
            let r = coord("r", None, full(None) * 0.5);
            let mut fx = coord("fx", Some(true), cx);
            let mut fy = coord("fy", Some(false), cy);
            // A focal point outside the circle moves onto it (13.2.3).
            let d = p(fx - cx, fy - cy);
            if d.len() > r * 0.999 {
                let q = p(cx, cy) + d.unit() * (r * 0.999);
                fx = q.x;
                fy = q.y;
            }
            Shape::Radial { cx, cy, r, fx, fy }
        };
        let spread = match attr("spreadMethod").map(str::trim) {
            Some("reflect") => Spread::Reflect,
            Some("repeat") => Spread::Repeat,
            _ => Spread::Pad,
        };
        let holder = chain
            .iter()
            .find(|e| e.elements().any(|s| s.name == "stop"))?;
        let mut stops: Vec<(f32, [f32; 4])> = Vec::new();
        let mut last = 0.0f32;
        for s in holder.elements().filter(|s| s.name == "stop") {
            let st = self.cascade(s, &Style::initial(), ctx.vp);
            let at = s
                .attr("offset")
                .and_then(style::fraction)
                .unwrap_or(0.0)
                .max(last);
            last = at;
            stops.push((at, premultiplied(st.stop_color, st.stop_opacity * opacity)));
        }
        let degenerate = match shape {
            Shape::Radial { r, .. } => r <= 0.0,
            Shape::Linear { .. } => false,
        };
        match stops.len() {
            0 => None,
            1 => Some(Paint::Solid(stops[0].1)),
            _ if degenerate => Some(Paint::Solid(stops[stops.len() - 1].1)),
            _ => Some(Paint::Gradient {
                inv: m.inverse()?,
                shape,
                spread,
                stops,
            }),
        }
    }

    /// A clip path's coverage over `area` (14.3): its shapes' union, each
    /// by its clip rule.
    fn clip_mask(&self, id: &str, bbox: Option<Rect>, ctx: &Ctx<'a>, area: &Area) -> Option<Mask> {
        let clip = self.lookup(id).filter(|c| c.name == "clipPath")?;
        let mut ctm = ctx.ctm;
        if clip
            .attr("clipPathUnits")
            .is_some_and(|u| u.trim() == "objectBoundingBox")
        {
            let b = bbox.filter(|b| b.width() > 0.0 && b.height() > 0.0);
            let Some(b) = b else {
                return Some(Mask {
                    area: *area,
                    cov: vec![0.0; area.w * area.h],
                });
            };
            ctm = ctm.then_after(&Mat {
                a: b.width(),
                b: 0.0,
                c: 0.0,
                d: b.height(),
                e: b.x0,
                f: b.y0,
            });
        }
        if let Some(t) = clip.attr("transform").and_then(path::transform) {
            ctm = ctm.then_after(&t);
        }
        let base = self.cascade(clip, &Style::initial(), ctx.vp);
        let mut acc = Mask {
            area: *area,
            cov: vec![0.0; area.w * area.h],
        };
        for child in clip.elements() {
            let (shape, extra) = if child.name == "use" {
                let Some(t) = child.href().and_then(|h| self.lookup(h)) else {
                    continue;
                };
                let x = self.len(child, "x", Some(true), ctx.vp, 0.0);
                let y = self.len(child, "y", Some(false), ctx.vp, 0.0);
                (t, Mat::translate(x, y))
            } else {
                (child, Mat::IDENTITY)
            };
            let st = self.cascade(child, &base, ctx.vp);
            let st = if std::ptr::eq(shape, child) {
                st
            } else {
                self.cascade(shape, &st, ctx.vp)
            };
            if !st.shown || !st.visible {
                continue;
            }
            let mut m = ctm;
            if let Some(t) = child.attr("transform").and_then(path::transform) {
                m = m.then_after(&t);
            }
            m = m.then_after(&extra);
            if !std::ptr::eq(shape, child)
                && let Some(t) = shape.attr("transform").and_then(path::transform)
            {
                m = m.then_after(&t);
            }
            let Some(segs) = self.geometry(shape, ctx.vp) else {
                continue;
            };
            let tol = TOL / m.max_scale().max(1e-12);
            let polys: Vec<Vec<P>> = path::flatten(&segs, tol)
                .into_iter()
                .map(|(v, _)| v.into_iter().map(|q| m.apply(q)).collect())
                .collect();
            if let Some(cov) = raster::fill(&polys, st.clip_evenodd, area) {
                for y in cov.area.y0..cov.area.y1() {
                    for x in cov.area.x0..cov.area.x1() {
                        let c = cov.at(x, y);
                        let i = (y - area.y0) as usize * area.w + (x - area.x0) as usize;
                        acc.cov[i] = acc.cov[i] + c - acc.cov[i] * c;
                    }
                }
            }
        }
        Some(acc)
    }

    /// A mask's coverage over `area` (14.4): its content's luminance times
    /// its alpha, within the mask's region.
    fn mask(&self, id: &str, bbox: Option<Rect>, ctx: &Ctx<'a>, area: &Area) -> Option<Mask> {
        let mask = self.lookup(id).filter(|m| m.name == "mask")?;
        let empty = || Mask {
            area: *area,
            cov: vec![0.0; area.w * area.h],
        };
        let units_user = mask
            .attr("maskUnits")
            .is_some_and(|u| u.trim() == "userSpaceOnUse");
        let content_bbox = mask
            .attr("maskContentUnits")
            .is_some_and(|u| u.trim() == "objectBoundingBox");
        let frac = |name: &str, default: f64| -> f64 {
            mask.attr(name).map_or(default, |v| {
                let v = v.trim();
                match v.strip_suffix('%') {
                    Some(pc) => pc.trim().parse::<f64>().map_or(default, |x| x / 100.0),
                    None => v.parse::<f64>().unwrap_or(default),
                }
            })
        };
        let region = if units_user {
            Rect {
                x0: self.len(mask, "x", Some(true), ctx.vp, -0.1 * ctx.vp.0),
                y0: self.len(mask, "y", Some(false), ctx.vp, -0.1 * ctx.vp.1),
                x1: 0.0,
                y1: 0.0,
            }
            .with_size(
                self.len(mask, "width", Some(true), ctx.vp, 1.2 * ctx.vp.0),
                self.len(mask, "height", Some(false), ctx.vp, 1.2 * ctx.vp.1),
            )
        } else {
            let Some(b) = bbox.filter(|b| b.width() > 0.0 && b.height() > 0.0) else {
                return Some(empty());
            };
            Rect {
                x0: b.x0 + frac("x", -0.1) * b.width(),
                y0: b.y0 + frac("y", -0.1) * b.height(),
                x1: 0.0,
                y1: 0.0,
            }
            .with_size(
                frac("width", 1.2) * b.width(),
                frac("height", 1.2) * b.height(),
            )
        };
        let region_pixels: Vec<Vec<P>> = vec![region.corners().map(|q| ctx.ctm.apply(q)).to_vec()];
        let Some(inside) = raster::fill(&region_pixels, false, area) else {
            return Some(empty());
        };
        let mut inner = ctx.clone();
        if content_bbox {
            let Some(b) = bbox.filter(|b| b.width() > 0.0 && b.height() > 0.0) else {
                return Some(empty());
            };
            inner.ctm = ctx.ctm.then_after(&Mat {
                a: b.width(),
                b: 0.0,
                c: 0.0,
                d: b.height(),
                e: b.x0,
                f: b.y0,
            });
        }
        // CSS Masking's `mask-type` (SVG 2): its content's alpha alone, or
        // its luminance (the default).
        let mut alpha = mask.attr("mask-type").is_some_and(|t| t.trim() == "alpha");
        for (k, v) in style::matching(&self.rules, mask)
            .into_iter()
            .cloned()
            .chain(
                mask.attr("style")
                    .map(style::declarations)
                    .unwrap_or_default(),
            )
        {
            if k == "mask-type" {
                alpha = v.trim() == "alpha";
            }
        }
        let base = self.cascade(mask, &Style::initial(), ctx.vp);
        let mut layer = Canvas::new(*area);
        for c in mask.elements() {
            self.draw(c, &base, &inner, &mut layer);
        }
        let mut out = empty();
        for y in area.y0..area.y1() {
            for x in area.x0..area.x1() {
                let i = (y - area.y0) as usize * area.w + (x - area.x0) as usize;
                let c = layer.px[i];
                // Luminance of the color multiplied by alpha already.
                let lum = if alpha {
                    c[3]
                } else {
                    0.2125 * c[0] + 0.7154 * c[1] + 0.0721 * c[2]
                };
                out.cov[i] = lum.clamp(0.0, 1.0) * inside.at(x, y);
            }
        }
        Some(out)
    }

    /// A picture the document holds (`<image>`), read by the caller.
    fn image(&self, e: &Element, style: &Style, ctx: &Ctx<'a>, canvas: &mut Canvas) {
        if !style.visible {
            return;
        }
        let Some(pic) = e.href().and_then(|h| (self.images)(h.trim())) else {
            return;
        };
        if pic.width == 0 || pic.height == 0 {
            return;
        }
        let (pw, ph) = (f64::from(pic.width), f64::from(pic.height));
        let x = self.len(e, "x", Some(true), ctx.vp, 0.0);
        let y = self.len(e, "y", Some(false), ctx.vp, 0.0);
        let w = e
            .attr("width")
            .and_then(|v| style::length(v, ctx.vp.0))
            .unwrap_or(pw);
        let h = e
            .attr("height")
            .and_then(|v| style::length(v, ctx.vp.1))
            .unwrap_or(ph);
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let place = Mat::translate(x, y).then_after(&view_box(
            [0.0, 0.0, pw, ph],
            w,
            h,
            e.attr("preserveAspectRatio"),
        ));
        let m = ctx.ctm.then_after(&place);
        let Some(inv) = m.inverse() else {
            return;
        };
        let frame = Rect {
            x0: x,
            y0: y,
            x1: x + w,
            y1: y + h,
        };
        // Where the picture lands, within its frame (a sliced picture
        // overflows it).
        let poly = vec![frame.corners().map(|q| ctx.ctm.apply(q)).to_vec()];
        let Some(frame_cov) = raster::fill(&poly, false, &canvas.area) else {
            return;
        };
        let placed = Rect {
            x0: 0.0,
            y0: 0.0,
            x1: pw,
            y1: ph,
        };
        let poly = vec![placed.corners().map(|q| m.apply(q)).to_vec()];
        let Some(cov) = raster::fill(&poly, false, &frame_cov.area) else {
            return;
        };
        let px = picture_rgba(&pic);
        let (iw, ih) = (pic.width as usize, pic.height as usize);
        let sample = |sx: f64, sy: f64| -> [f32; 4] {
            // Bilinear, its edge pixels going on past it (its edges are
            // the coverage's).
            let fx = sx - 0.5;
            let fy = sy - 0.5;
            let x0 = fx.floor();
            let y0 = fy.floor();
            let (tx, ty) = ((fx - x0) as f32, (fy - y0) as f32);
            let get = |xx: f64, yy: f64| -> [f32; 4] {
                let xi = xx.clamp(0.0, (iw - 1) as f64) as usize;
                let yi = yy.clamp(0.0, (ih - 1) as f64) as usize;
                px[yi * iw + xi]
            };
            let (a, b) = (get(x0, y0), get(x0 + 1.0, y0));
            let (c, d) = (get(x0, y0 + 1.0), get(x0 + 1.0, y0 + 1.0));
            std::array::from_fn(|i| {
                let top = a[i] + (b[i] - a[i]) * tx;
                let bottom = c[i] + (d[i] - c[i]) * tx;
                top + (bottom - top) * ty
            })
        };
        let area = cov.area;
        for yy in area.y0..area.y1() {
            for xx in area.x0..area.x1() {
                let k = cov.at(xx, yy) * frame_cov.at(xx, yy);
                if k <= 0.0 {
                    continue;
                }
                let q = inv.apply(p(f64::from(xx) + 0.5, f64::from(yy) + 0.5));
                let s = sample(q.x, q.y);
                if s[3] <= 0.0 {
                    continue;
                }
                let i =
                    (yy - canvas.area.y0) as usize * canvas.area.w + (xx - canvas.area.x0) as usize;
                let d = &mut canvas.px[i];
                let inv_a = 1.0 - s[3] * k;
                for c in 0..4 {
                    d[c] = s[c] * k + d[c] * inv_a;
                }
            }
        }
    }

    /// Where `e` draws, in the pixels of `ctx`'s transform: its shapes'
    /// box (their strokes' too, with `strokes`), its content's; `None`
    /// when it draws nothing.
    fn bounds(&self, e: &'a Element, style: &Style, ctx: &Ctx<'a>, strokes: bool) -> Option<Rect> {
        if ctx.depth > MAX_DEPTH || !self.spend() {
            return None;
        }
        match e.name.as_str() {
            "svg" | "g" | "a" | "switch" => {
                let mut inner = ctx.clone();
                if e.name == "svg" && !std::ptr::eq(e, self.root) {
                    let (ctm, vp) = self.viewport(e, ctx, None);
                    inner.ctm = ctm;
                    inner.vp = vp;
                }
                let mut out: Option<Rect> = None;
                for c in e.elements() {
                    if let Some(b) = self.child_bounds(c, style, &inner, strokes) {
                        out = Some(out.map_or(b, |o| o.union(b)));
                    }
                }
                out
            }
            "use" => {
                let target = e.href().and_then(|h| self.lookup(h))?;
                if ctx.uses.iter().any(|u| std::ptr::eq(*u, target)) || std::ptr::eq(target, e) {
                    return None;
                }
                let mut inner = ctx.clone();
                inner.uses.push(target);
                inner.depth += 1;
                let x = self.len(e, "x", Some(true), ctx.vp, 0.0);
                let y = self.len(e, "y", Some(false), ctx.vp, 0.0);
                inner.ctm = ctx.ctm.then_after(&Mat::translate(x, y));
                if target.name == "symbol" {
                    let (ctm, vp) = self.viewport(target, &inner, Some(e));
                    inner.ctm = ctm;
                    inner.vp = vp;
                    let mut out: Option<Rect> = None;
                    for c in target.elements() {
                        if let Some(b) = self.child_bounds(c, style, &inner, strokes) {
                            out = Some(out.map_or(b, |o| o.union(b)));
                        }
                    }
                    out
                } else {
                    self.child_bounds(target, style, &inner, strokes)
                }
            }
            "image" => {
                let x = self.len(e, "x", Some(true), ctx.vp, 0.0);
                let y = self.len(e, "y", Some(false), ctx.vp, 0.0);
                let w = e.attr("width").and_then(|v| style::length(v, ctx.vp.0))?;
                let h = e.attr("height").and_then(|v| style::length(v, ctx.vp.1))?;
                Some(ctx.ctm.rect(&Rect {
                    x0: x,
                    y0: y,
                    x1: x + w,
                    y1: y + h,
                }))
            }
            _ => {
                let segs = self.geometry(e, ctx.vp)?;
                let b = path::bounds(&segs)?;
                let mut r = ctx.ctm.rect(&b);
                if strokes && style.stroke != PaintSpec::None {
                    let reach = style.stroke_width / 2.0
                        * if style.join == Join::Miter {
                            style.miter_limit.max(1.5)
                        } else {
                            1.5
                        };
                    r = r.grow(reach * ctx.ctm.max_scale() + 1.0);
                }
                Some(r.grow(1.0))
            }
        }
    }

    /// A child's bounds, its own style and transform applied.
    fn child_bounds(
        &self,
        c: &'a Element,
        parent: &Style,
        ctx: &Ctx<'a>,
        strokes: bool,
    ) -> Option<Rect> {
        if NOT_DRAWN.contains(&c.name.as_str()) {
            return None;
        }
        let st = self.cascade(c, parent, ctx.vp);
        if !st.shown {
            return None;
        }
        let mut inner = ctx.clone();
        inner.depth += 1;
        if let Some(t) = c.attr("transform") {
            inner.ctm = inner.ctm.then_after(&path::transform(t)?);
        }
        self.bounds(c, &st, &inner, strokes)
    }
}

impl Rect {
    /// From its corner, `w` × `h`.
    fn with_size(self, w: f64, h: f64) -> Rect {
        Rect {
            x1: self.x0 + w,
            y1: self.y0 + h,
            ..self
        }
    }
}

/// A picture's pixels as colors multiplied by alpha, display-encoded.
fn picture_rgba(pic: &Picture) -> Vec<[f32; 4]> {
    let encode = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        if v <= 0.003_130_8 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }
    };
    let px = |r: f32, g: f32, b: f32, a: f32| -> [f32; 4] {
        let a = a.clamp(0.0, 1.0);
        let (mut r, mut g, mut b) = (r, g, b);
        if pic.premultiplied && a > 0.0 {
            r /= a;
            g /= a;
            b /= a;
        }
        if pic.linear {
            r = encode(r);
            g = encode(g);
            b = encode(b);
        }
        [
            r.clamp(0.0, 1.0) * a,
            g.clamp(0.0, 1.0) * a,
            b.clamp(0.0, 1.0) * a,
            a,
        ]
    };
    match &pic.data {
        PictureData::Rgba8(d) => d
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| {
                px(
                    f32::from(c[0]) / 255.0,
                    f32::from(c[1]) / 255.0,
                    f32::from(c[2]) / 255.0,
                    f32::from(c[3]) / 255.0,
                )
            })
            .collect(),
        PictureData::Rgba16(d) => d
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| {
                px(
                    f32::from(c[0]) / 65535.0,
                    f32::from(c[1]) / 65535.0,
                    f32::from(c[2]) / 65535.0,
                    f32::from(c[3]) / 65535.0,
                )
            })
            .collect(),
        PictureData::RgbaF32(d) => d
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| px(c[0], c[1], c[2], c[3]))
            .collect(),
    }
}
