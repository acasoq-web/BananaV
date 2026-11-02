//! Bézier Masking, as VEGAS's: up to five masks — an oval, a rectangle (at
//! first), a diamond, or a curve of Bézier anchors drawn on the preview —,
//! each placed by its Width, Height, Angle and Location (on the preview:
//! inside it moves it, its white corners scale it, its yellow sides
//! stretch it, the ring in its middle turns it), weighed by its Opacity
//! and softened by its Feather (in percent of the frame's shorter side,
//! into the shape, out of it or both ways); the masks add up. The General
//! Options are Invert, Mask FX — the effects before it in the chain show
//! only inside the masks: the rest is the chain's input, which BananaV's
//! host gives on the optional "Original" clip — and Blend, which brings
//! back what the masks leave out.
//!
//! A curve is drawn in its Edit Mode, masking nothing meanwhile: a press
//! adds an anchor and a drag pulls out its tangents, a click on the first
//! anchor closes the curve; the Edit Tool moves anchors and tangents
//! (Normal), adds one on the curve (Add), deletes anchors (Delete) or adds
//! and splits tangents (Split). The curve is a custom parameter (its anchors in the mask's own
//! box), interpolated between keyframes. Each mask's Tracking › Start
//! has the host follow its box from the cursor and keyframe its Location
//! (with its Angle, and its Width and Height, as Mode says;
//! `kBananaVParamPropMaskTracking`).
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::time::Instant;

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::kit::{Buf, for_chunks};
use crate::ofx::{Clicks, Draw, Fx, HANDLE_REACH, Props, interact_props, is, redraw, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.BezierMasking";

/// How many masks the plug-in has, and their sections' names.
pub(crate) const MASKS: usize = 5;
const MASK_LABELS: [&CStr; MASKS] = [c"Mask 1", c"Mask 2", c"Mask 3", c"Mask 4", c"Mask 5"];

/// Each mask's numbered labels, as VEGAS's panel has them: Enable, Width,
/// Height, Angle, Opacity, Feather, Feather Style, Location and Curve.
const LABELS: [[&CStr; 9]; MASKS] = [
    [
        c"Enable 1",
        c"Width 1",
        c"Height 1",
        c"Angle 1",
        c"Opacity 1",
        c"Feather 1",
        c"Feather Style 1",
        c"Location 1",
        c"Curve 1",
    ],
    [
        c"Enable 2",
        c"Width 2",
        c"Height 2",
        c"Angle 2",
        c"Opacity 2",
        c"Feather 2",
        c"Feather Style 2",
        c"Location 2",
        c"Curve 2",
    ],
    [
        c"Enable 3",
        c"Width 3",
        c"Height 3",
        c"Angle 3",
        c"Opacity 3",
        c"Feather 3",
        c"Feather Style 3",
        c"Location 3",
        c"Curve 3",
    ],
    [
        c"Enable 4",
        c"Width 4",
        c"Height 4",
        c"Angle 4",
        c"Opacity 4",
        c"Feather 4",
        c"Feather Style 4",
        c"Location 4",
        c"Curve 4",
    ],
    [
        c"Enable 5",
        c"Width 5",
        c"Height 5",
        c"Angle 5",
        c"Opacity 5",
        c"Feather 5",
        c"Feather Style 5",
        c"Location 5",
        c"Curve 5",
    ],
];

/// The clip a host may connect to the chain's input (Mask FX).
pub(crate) const ORIGINAL: &CStr = c"Original";

const INVERT: &CStr = c"invert";
const MASK_FX: &CStr = c"maskFx";
const BLEND: &CStr = c"blend";

/// A mask's parameter's name: `base` and its number (1–5).
fn name(base: &str, n: usize) -> CString {
    CString::new(format!("{base}{n}")).expect("a name without NUL")
}

// The masks.

/// A mask's shape, in its own box: −½ to ½ across and up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Rectangle,
    Oval,
    Diamond,
    Curve,
}

impl Kind {
    /// Type's option `i` (VEGAS's order: the oval first).
    fn of(i: c_int) -> Kind {
        match i {
            0 => Kind::Oval,
            2 => Kind::Diamond,
            3 => Kind::Curve,
            _ => Kind::Rectangle,
        }
    }
}

/// Which way a mask's feather goes from its edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Feather {
    Out,
    In,
    Both,
}

impl Feather {
    /// Its Feather Style option: In, Out, Both.
    pub(crate) fn index(self) -> u32 {
        match self {
            Feather::In => 0,
            Feather::Out => 1,
            Feather::Both => 2,
        }
    }

    /// Feather Style's option `i`: In, Out, Both.
    pub(crate) fn of(i: c_int) -> Feather {
        match i {
            0 => Feather::In,
            1 => Feather::Out,
            _ => Feather::Both,
        }
    }
}

/// A curve's anchor in the mask's box: its point and its tangents' ends
/// (offsets from it, toward the anchor before and after), and whether its
/// tangents are split (they turn apart).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Anchor {
    pub at: [f64; 2],
    pub before: [f64; 2],
    pub after: [f64; 2],
    pub split: bool,
}

impl Anchor {
    fn corner(at: [f64; 2]) -> Anchor {
        Anchor {
            at,
            before: [0.0; 2],
            after: [0.0; 2],
            split: false,
        }
    }
}

/// A curve: its anchors and whether it is closed (a mask's shape only
/// then).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Curve {
    pub anchors: Vec<Anchor>,
    pub closed: bool,
}

impl Curve {
    /// The curve `text` writes ("c" closed or "o" open, then each anchor's
    /// point, tangents and split flag); none: no curve.
    pub(crate) fn parse(text: &str) -> Curve {
        let mut it = text.split_ascii_whitespace();
        let closed = match it.next() {
            Some("c") => true,
            Some("o") => false,
            _ => return Curve::default(),
        };
        let v: Option<Vec<f64>> = it
            .map(|s| s.parse::<f64>().ok().filter(|v| v.is_finite()))
            .collect();
        let Some(v) = v.filter(|v| v.len() % 7 == 0) else {
            return Curve::default();
        };
        Curve {
            anchors: v
                .chunks(7)
                .map(|c| Anchor {
                    at: [c[0], c[1]],
                    before: [c[2], c[3]],
                    after: [c[4], c[5]],
                    split: c[6] != 0.0,
                })
                .collect(),
            closed,
        }
    }

    /// As text: nothing for no curve.
    pub(crate) fn text(&self) -> String {
        if self.anchors.is_empty() {
            return String::new();
        }
        let mut s = String::from(if self.closed { "c" } else { "o" });
        for a in &self.anchors {
            for v in [
                a.at[0],
                a.at[1],
                a.before[0],
                a.before[1],
                a.after[0],
                a.after[1],
                a.split as u8 as f64,
            ] {
                s.push(' ');
                s.push_str(&number(v));
            }
        }
        s
    }

    /// The curve `k` of the way to `other` (of as many anchors; else it
    /// changes at the second).
    fn lerp(&self, other: &Curve, k: f64) -> Curve {
        if self.anchors.len() != other.anchors.len() || self.closed != other.closed {
            return if k < 1.0 { self.clone() } else { other.clone() };
        }
        let l = |a: [f64; 2], b: [f64; 2]| [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k];
        Curve {
            anchors: self
                .anchors
                .iter()
                .zip(&other.anchors)
                .map(|(a, b)| Anchor {
                    at: l(a.at, b.at),
                    before: l(a.before, b.before),
                    after: l(a.after, b.after),
                    split: a.split,
                })
                .collect(),
            closed: self.closed,
        }
    }

    /// Its outline as a polygon (each segment a cubic Bézier, 24 steps),
    /// in the mask's box.
    fn polygon(&self) -> Vec<[f64; 2]> {
        let n = self.anchors.len();
        let mut out = Vec::new();
        let segments = if self.closed { n } else { n.saturating_sub(1) };
        for i in 0..segments {
            let (a, b) = (self.anchors[i], self.anchors[(i + 1) % n]);
            let p = [
                a.at,
                [a.at[0] + a.after[0], a.at[1] + a.after[1]],
                [b.at[0] + b.before[0], b.at[1] + b.before[1]],
                b.at,
            ];
            for s in 0..24 {
                out.push(bezier(&p, s as f64 / 24.0));
            }
        }
        if !self.closed
            && let Some(last) = self.anchors.last()
        {
            out.push(last.at);
        }
        out
    }
}

/// A cubic Bézier's point at `t`.
fn bezier(p: &[[f64; 2]; 4], t: f64) -> [f64; 2] {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    [
        a * p[0][0] + b * p[1][0] + c * p[2][0] + d * p[3][0],
        a * p[0][1] + b * p[1][1] + c * p[2][1] + d * p[3][1],
    ]
}

/// `v` with at most five decimals, as short as it writes.
fn number(v: f64) -> String {
    let s = format!("{v:.5}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

/// One mask at one time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Mask {
    pub kind: Kind,
    /// Its box's width and height (parts of the frame's width and height).
    pub size: [f64; 2],
    /// Counterclockwise, in degrees.
    pub angle: f64,
    /// Its middle (the frame 0–1 across and up).
    pub center: [f64; 2],
    /// How much of it counts (0–1): the masks add up.
    pub opacity: f64,
    /// How far its edge softens, in percent of the frame's shorter side.
    pub feather: f64,
    pub style: Feather,
    pub curve: Curve,
}

impl Mask {
    /// A point of its box (−½ to ½) in the frame's units — `frame` its size
    /// in canonical units, square on the screen —, from the frame's
    /// corner.
    fn place(&self, p: [f64; 2], frame: [f64; 2]) -> [f64; 2] {
        let x = p[0] * self.size[0] * frame[0];
        let y = p[1] * self.size[1] * frame[1];
        let (sin, cos) = self.angle.to_radians().sin_cos();
        [
            self.center[0] * frame[0] + x * cos - y * sin,
            self.center[1] * frame[1] + x * sin + y * cos,
        ]
    }

    /// A point of the frame (canonical units from its corner) in its box.
    fn local(&self, q: [f64; 2], frame: [f64; 2]) -> [f64; 2] {
        let dx = q[0] - self.center[0] * frame[0];
        let dy = q[1] - self.center[1] * frame[1];
        let (sin, cos) = self.angle.to_radians().sin_cos();
        let x = dx * cos + dy * sin;
        let y = -dx * sin + dy * cos;
        [
            x / (self.size[0] * frame[0]).max(1e-9),
            y / (self.size[1] * frame[1]).max(1e-9),
        ]
    }

    /// Its outline in its box: the shape's polygon (a curve's only when
    /// closed).
    fn outline(&self) -> Vec<[f64; 2]> {
        match self.kind {
            Kind::Rectangle => vec![[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]],
            Kind::Diamond => vec![[0.0, -0.5], [0.5, 0.0], [0.0, 0.5], [-0.5, 0.0]],
            Kind::Oval => (0..96)
                .map(|i| {
                    let a = std::f64::consts::TAU * i as f64 / 96.0;
                    [0.5 * a.cos(), 0.5 * a.sin()]
                })
                .collect(),
            Kind::Curve if self.curve.closed && self.curve.anchors.len() >= 2 => {
                self.curve.polygon()
            }
            Kind::Curve => Vec::new(),
        }
    }
}

/// The masks' settings at one time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    /// The enabled masks (but a curve being drawn).
    pub masks: Vec<Mask>,
    pub invert: bool,
    pub mask_fx: bool,
    pub blend: f64,
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        // A curve in its Edit Mode is being drawn: it masks nothing yet.
        let masks = (1..=MASKS)
            .filter(|&n| fx.param(&name("enable", n)).int_at(time) != 0)
            .map(|n| (n, mask_at(fx, n, time)))
            .filter(|(n, m)| {
                m.kind != Kind::Curve || fx.param(&name("editMode", *n)).int_at(time) == 0
            })
            .map(|(_, m)| m)
            .collect();
        Settings {
            masks,
            invert: fx.param(INVERT).int_at(time) != 0,
            mask_fx: fx.param(MASK_FX).int_at(time) != 0,
            blend: fx.param(BLEND).f64_at(time).clamp(0.0, 1.0),
        }
    }

    /// The masks' settings in `values` (the frame's, as the card draws it).
    pub(crate) fn of(values: Values<'_>) -> Settings {
        let masks = (1..=MASKS)
            .filter(|&n| values.int(&name("enable", n), c_int::from(n == 1)) != 0)
            .map(|n| (n, mask_of(values, n)))
            .filter(|(n, m)| m.kind != Kind::Curve || values.int(&name("editMode", *n), 0) == 0)
            .map(|(_, m)| m)
            .collect();
        Settings {
            masks,
            invert: values.int(INVERT, 0) != 0,
            mask_fx: values.int(MASK_FX, 0) != 0,
            blend: values.f64(BLEND, 0.0).clamp(0.0, 1.0),
        }
    }

    /// What the card draws on a frame `frame` canonical units big: the
    /// masks that show, their outlines from the frame's lower left, their
    /// weights and feathers (canonical units).
    pub(crate) fn plan(&self, frame: [f64; 2]) -> crate::accel::Plan {
        let shorter = frame[0].min(frame[1]);
        let shapes = self
            .masks
            .iter()
            .filter(|m| m.opacity > 0.0)
            .filter_map(|m| {
                let outline: Vec<[f32; 2]> = m
                    .outline()
                    .into_iter()
                    .map(|p| m.place(p, frame).map(|v| v as f32))
                    .collect();
                (outline.len() >= 3).then(|| crate::accel::MaskShape {
                    outline,
                    weight: m.opacity.min(1.0) as f32,
                    feather: (m.feather / 100.0 * shorter) as f32,
                    style: m.style.index(),
                })
            })
            .collect();
        crate::accel::Plan::Masks {
            shapes,
            any: !self.masks.is_empty(),
            invert: self.invert,
            mask_fx: self.mask_fx,
            blend: self.blend as f32,
        }
    }

    /// Whether it leaves the picture as it is: all of Blend brings it all
    /// back; with Mask FX, no mask and no Blend keep the effects before it
    /// everywhere, and so do inverted masks with all of Blend.
    fn changes_nothing(&self) -> bool {
        if !self.mask_fx {
            self.blend >= 1.0
        } else if self.masks.is_empty() {
            self.blend <= 0.0
        } else {
            self.invert && self.blend >= 1.0
        }
    }
}

/// Mask `n`'s box (1–5) with `values` on a frame `aspect` wide for 1
/// high: its corners 0–1 across and up, round it from the top left (a
/// curve's the box around it); `None` when the mask is off or has no
/// shape. Motion Tracking imports it as a region.
pub(crate) fn mask_box(values: Values<'_>, n: usize, aspect: f64) -> Option<[[f64; 2]; 4]> {
    if values.int(&name("enable", n), c_int::from(n == 1)) == 0 {
        return None;
    }
    let m = mask_of(values, n);
    let outline = m.outline();
    if outline.is_empty() {
        return None;
    }
    let (lo, hi) = outline.iter().fold(
        ([f64::MAX, f64::MAX], [f64::MIN, f64::MIN]),
        |(lo, hi), p| {
            (
                [lo[0].min(p[0]), lo[1].min(p[1])],
                [hi[0].max(p[0]), hi[1].max(p[1])],
            )
        },
    );
    let frame = [aspect, 1.0];
    let corners = [
        [lo[0], hi[1]],
        [hi[0], hi[1]],
        [hi[0], lo[1]],
        [lo[0], lo[1]],
    ];
    Some(corners.map(|p| {
        let q = m.place(p, frame);
        [q[0] / aspect, q[1]]
    }))
}

/// Mask `n` in `values`.
fn mask_of(values: Values<'_>, n: usize) -> Mask {
    let v = |base: &str| name(base, n);
    Mask {
        kind: Kind::of(values.int(&v("type"), 1)),
        size: [
            values.f64(&v("width"), 0.5).max(0.0),
            values.f64(&v("height"), 0.5).max(0.0),
        ],
        angle: values.f64(&v("angle"), 0.0),
        center: values.array(&v("location"), [0.5, 0.5]),
        opacity: values.f64(&v("opacity"), 1.0).clamp(0.0, 1.0),
        feather: values.f64(&v("feather"), 0.0).max(0.0),
        style: Feather::of(values.int(&v("featherStyle"), 2)),
        curve: Curve::parse(values.text(&v("curve")).unwrap_or("")),
    }
}

/// Mask `n` at `time`.
fn mask_at(fx: Fx, n: usize, time: f64) -> Mask {
    let p = |base: &str| fx.param(&name(base, n));
    Mask {
        kind: Kind::of(p("type").int_at(time)),
        size: [
            p("width").f64_at(time).max(0.0),
            p("height").f64_at(time).max(0.0),
        ],
        angle: p("angle").f64_at(time),
        center: p("location").xy_at(time),
        opacity: p("opacity").f64_at(time).clamp(0.0, 1.0),
        feather: p("feather").f64_at(time).max(0.0),
        style: Feather::of(p("featherStyle").int_at(time)),
        curve: Curve::parse(&p("curve").text_at(time)),
    }
}

// The mask's coverage.

/// How much of each pixel stays (0–1) — with Mask FX, how much of what
/// the effects before it made shows over the chain's input —, row by row
/// from the bottom as OpenFX counts them, over the picture's window
/// `bounds`. `frame` is the frame's size in canonical units and `origin`
/// where it starts; `scale` the render scale, `par` the pixels' aspect.
pub(crate) fn coverage(
    s: &Settings,
    bounds: [c_int; 4],
    origin: [f64; 2],
    frame: [f64; 2],
    scale: [f64; 2],
    par: f64,
) -> Vec<f32> {
    let (w, h) = (
        (bounds[2] - bounds[0]).max(0) as usize,
        (bounds[3] - bounds[1]).max(0) as usize,
    );
    // A point of the frame (canonical, from its corner) as a pixel of the
    // window, its middle at whole numbers.
    let pixel = |q: [f64; 2]| {
        [
            (q[0] + origin[0]) * scale[0] / par - 0.5 - bounds[0] as f64,
            (q[1] + origin[1]) * scale[1] - 0.5 - bounds[1] as f64,
        ]
    };
    let blend = s.blend as f32;
    // No mask: Blend of the picture (with Mask FX, 1 − Blend of the
    // effects).
    if s.masks.is_empty() {
        let k = if s.mask_fx { 1.0 - blend } else { blend };
        return vec![k; w * h];
    }
    let shorter = (frame[0] * scale[0] / par).min(frame[1] * scale[1]);
    // Each mask's coverage weighed by its Opacity, added up to all at
    // most.
    let mut sum = vec![0.0f32; w * h];
    for m in s.masks.iter().filter(|m| m.opacity > 0.0) {
        let outline: Vec<[f64; 2]> = m
            .outline()
            .into_iter()
            .map(|p| pixel(m.place(p, frame)))
            .collect();
        if outline.len() < 3 {
            continue;
        }
        let feather = m.feather / 100.0 * shorter;
        let cover = shape_coverage(&outline, w, h, feather, m.style);
        let weight = m.opacity.min(1.0) as f32;
        for (t, c) in sum.iter_mut().zip(&cover) {
            *t = (*t + c * weight).min(1.0);
        }
    }
    // Blend brings back what the masks leave out: the picture; with Mask
    // FX the chain's input, or, inverted, the effects.
    for k in &mut sum {
        let m = *k;
        *k = match (s.invert, s.mask_fx) {
            (false, false) => m + (1.0 - m) * blend,
            (false, true) => m * (1.0 - blend),
            (true, _) => (1.0 - m) + m * blend,
        };
    }
    sum
}

/// Where a mask's edge softens, in pixels inside its outline (negative:
/// outside): over `feather` pixels into it (style In), out of it (Out) or
/// half each way (Both); a pixel across it at least, as anti-aliasing.
pub(crate) fn softening(feather: f64, style: Feather) -> [f64; 2] {
    if feather < 1.0 {
        return [-0.5, 0.5];
    }
    match style {
        Feather::In => [0.0, feather],
        Feather::Out => [-feather, 0.0],
        Feather::Both => [-feather / 2.0, feather / 2.0],
    }
}

/// One shape's coverage of a `w` × `h` window (its outline in the
/// window's pixels, their middles at whole numbers): inside 1, outside 0,
/// its edge softened by the exact distance to the outline as
/// [`softening`] says. Only the pixels that near the outline are measured.
fn shape_coverage(
    outline: &[[f64; 2]],
    w: usize,
    h: usize,
    feather: f64,
    style: Feather,
) -> Vec<f32> {
    let [lo, hi] = softening(feather, style);
    let reach = hi.max(-lo);
    let edges: Vec<([f64; 2], [f64; 2])> = (0..outline.len())
        .map(|i| (outline[i], outline[(i + 1) % outline.len()]))
        .collect();
    let mut cover = vec![0.0f32; w * h];
    for_chunks(&mut cover, w, |y, row| {
        let fy = y as f64;
        // Inside: from each crossing of the row's middle to the next
        // (even-odd).
        let mut xs: Vec<f64> = edges
            .iter()
            .filter(|(a, b)| (a[1] <= fy) != (b[1] <= fy))
            .map(|(a, b)| a[0] + (fy - a[1]) / (b[1] - a[1]) * (b[0] - a[0]))
            .collect();
        xs.sort_by(f64::total_cmp);
        for pair in xs.chunks(2) {
            if let [x0, x1] = pair {
                let from = x0.ceil().clamp(0.0, w as f64) as usize;
                let to = x1.ceil().clamp(0.0, w as f64) as usize;
                row[from..to.max(from)].fill(1.0);
            }
        }
        // Within reach of an edge: the distance to the nearest.
        let mut near = vec![f64::INFINITY; w];
        for (a, b) in &edges {
            let (low, top) = (a[1].min(b[1]), a[1].max(b[1]));
            if fy < low - reach || fy > top + reach {
                continue;
            }
            // Across, the edge's part within reach of the row, and reach
            // more each way.
            let (x0, x1) = if top > low {
                let at = |v: f64| a[0] + (v.clamp(low, top) - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                let (p, q) = (at(fy - reach), at(fy + reach));
                (p.min(q), p.max(q))
            } else {
                (a[0].min(b[0]), a[0].max(b[0]))
            };
            let from = (x0 - reach).ceil().clamp(0.0, w as f64) as usize;
            let to = ((x1 + reach).floor() + 1.0).clamp(0.0, w as f64) as usize;
            for (x, n) in near.iter_mut().enumerate().take(to).skip(from) {
                *n = n.min(segment_distance([x as f64, fy], *a, *b));
            }
        }
        for (c, &n) in row.iter_mut().zip(&near) {
            if n < reach {
                let d = if *c > 0.5 { n } else { -n };
                *c = smooth(lo, hi, d) as f32;
            }
        }
    });
    cover
}

/// The distance from `p` to the segment from `a` to `b`.
fn segment_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let e = [b[0] - a[0], b[1] - a[1]];
    let length = e[0] * e[0] + e[1] * e[1];
    let t = if length > 0.0 {
        (((p[0] - a[0]) * e[0] + (p[1] - a[1]) * e[1]) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - a[0] - t * e[0]).hypot(p[1] - a[1] - t * e[1])
}

fn smooth(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0).max(1e-9)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// The plug-in.

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
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &roi);
        out.set_dbls(c"OfxImageClipPropRoI_Original", &roi);
        kOfxStatOK
    } else if is(action, kOfxActionInstanceChanged) {
        show(fx, args.dbl(kOfxPropTime, 0));
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

/// A curve mask's Edit Mode and Edit Tool show only for curves.
fn show(fx: Fx, time: f64) {
    for n in 1..=MASKS {
        let curve = fx.param(&name("type", n)).int_at(time) == 3;
        fx.param(&name("editMode", n)).set_enabled(curve);
        fx.param(&name("editTool", n)).set_enabled(curve);
    }
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Bézier Masking");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Utility");
    p.set_str(
        kOfxPropPluginDescription,
        c"Up to five masks — rectangles, ovals, diamonds or Bézier curves — that cut the picture out, or keep the effects before it inside them.",
    );
    // The General context: its optional "Original" clip.
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextGeneral],
    );
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    p.set_int(kOfxImageEffectPropSupportsTiles, 0);
    p.set_int(kOfxImageEffectPropSupportsMultiResolution, 1);
    p.set_int(kOfxImageEffectPluginPropHostFrameThreading, 0);
    p.set_str(
        kOfxImageEffectPluginRenderThreadSafety,
        kOfxImageEffectRenderFullySafe,
    );
    p.set_ptr(
        kOfxImageEffectPluginPropOverlayInteractV2,
        overlay as *mut c_void,
    );
}

fn describe_in_context(fx: Fx) {
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);
    // The chain's input, for Mask FX (BananaV's host connects it).
    let original = fx.define_clip(ORIGINAL, &comps);
    original.set_int(kOfxImageClipPropOptional, 1);

    let choice =
        |n: &CStr, label: &CStr, options: &[&CStr], default: c_int, parent: &CStr, hint: &CStr| {
            let p = fx.define_param(kOfxParamTypeChoice, n, label);
            p.set_strs(kOfxParamPropChoiceOption, options);
            p.set_int(kOfxParamPropDefault, default);
            p.set_str(kOfxParamPropParent, parent);
            p.set_str(kOfxParamPropHint, hint);
            p
        };
    let toggle = |n: &CStr, label: &CStr, default: bool, parent: &CStr, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeBoolean, n, label);
        p.set_int(kOfxParamPropDefault, default as c_int);
        p.set_str(kOfxParamPropParent, parent);
        p.set_str(kOfxParamPropHint, hint);
        p
    };
    let number = |n: &CStr,
                  label: &CStr,
                  [min, max, lo, hi, default, step]: [f64; 6],
                  parent: &CStr,
                  hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble, n, label);
        p.set_dbl(kOfxParamPropDefault, default);
        p.set_dbl(kOfxParamPropMin, min);
        p.set_dbl(kOfxParamPropMax, max);
        p.set_dbl(kOfxParamPropDisplayMin, lo);
        p.set_dbl(kOfxParamPropDisplayMax, hi);
        p.set_dbl(kOfxParamPropIncrement, step);
        p.set_int(kOfxParamPropDigits, 3);
        p.set_str(kOfxParamPropParent, parent);
        p.set_str(kOfxParamPropHint, hint);
        p
    };
    // The General Options first, as VEGAS lists them.
    let general = c"general";
    let g = fx.define_param(kOfxParamTypeGroup, general, c"General Options");
    g.set_int(kOfxParamPropGroupOpen, 1);
    toggle(
        INVERT,
        c"Invert",
        false,
        general,
        c"Leaves out what is inside the masks instead.",
    );
    toggle(
        MASK_FX,
        c"Mask FX",
        false,
        general,
        c"Masks the effects before it in the chain instead of the picture: they show only inside the masks.",
    );
    number(
        BLEND,
        c"Blend",
        [0.0, 1.0, 0.0, 1.0, 0.0, 0.02],
        general,
        c"How much of what the masks leave out comes back: 0 none of it.",
    );
    for n in 1..=MASKS {
        let [
            enable,
            width,
            height,
            angle,
            opacity,
            feather,
            style,
            location,
            curve,
        ] = LABELS[n - 1];
        let group = name("mask", n);
        let g = fx.define_param(kOfxParamTypeGroup, &group, MASK_LABELS[n - 1]);
        g.set_int(kOfxParamPropGroupOpen, (n == 1) as c_int);
        toggle(
            &name("enable", n),
            enable,
            n == 1,
            &group,
            c"Uses this mask.",
        );
        choice(
            &name("type", n),
            c"Type",
            &[c"Oval", c"Rectangle", c"Diamond", c"Curve"],
            1,
            &group,
            c"The mask's shape: an oval, a rectangle, a diamond, or a curve drawn on the preview.",
        );
        number(
            &name("width", n),
            width,
            [0.0, 1.0, 0.0, 1.0, 0.5, 0.01],
            &group,
            c"The mask's width, a part of the frame's.",
        );
        number(
            &name("height", n),
            height,
            [0.0, 1.0, 0.0, 1.0, 0.5, 0.01],
            &group,
            c"The mask's height, a part of the frame's.",
        );
        let p = number(
            &name("angle", n),
            angle,
            [-180.0, 180.0, -180.0, 180.0, 0.0, 0.1],
            &group,
            c"How far the mask is turned, in degrees.",
        );
        p.set_str(kOfxParamPropDoubleType, kOfxParamDoubleTypeAngle);
        let p = fx.define_param(kOfxParamTypeDouble2D, &name("location", n), location);
        p.set_str(
            kOfxParamPropDoubleType,
            kOfxParamDoubleTypeNormalisedXYAbsolute,
        );
        p.set_dbls(kOfxParamPropDefault, &[0.5, 0.5]);
        p.set_dbls(kOfxParamPropMin, &[-1.0, -1.0]);
        p.set_dbls(kOfxParamPropMax, &[2.0, 2.0]);
        p.set_dbls(kOfxParamPropDisplayMin, &[0.0, 0.0]);
        p.set_dbls(kOfxParamPropDisplayMax, &[1.0, 1.0]);
        p.set_dbls(kOfxParamPropIncrement, &[0.1]);
        p.set_int(kOfxParamPropDigits, 3);
        p.set_str(kOfxParamPropParent, &group);
        p.set_str(kOfxParamPropHint, c"The mask's middle on the frame.");
        number(
            &name("opacity", n),
            opacity,
            [0.0, 1.0, 0.0, 1.0, 1.0, 0.02],
            &group,
            c"How much the mask counts: lower fades what is inside it.",
        );
        number(
            &name("feather", n),
            feather,
            [0.0, 50.0, 0.0, 50.0, 0.0, 2.5],
            &group,
            c"How far the mask's edge softens, in percent of the frame's shorter side.",
        );
        choice(
            &name("featherStyle", n),
            style,
            &[c"In", c"Out", c"Both"],
            2,
            &group,
            c"Which way the feather goes from the edge: into the mask, out of it, or both.",
        );
        toggle(
            &name("editMode", n),
            c"Edit Mode",
            false,
            &group,
            c"Draws the curve on the preview: a press adds an anchor, a drag its tangents, a click on the first anchor closes it.",
        )
        .set_int(kOfxParamPropAnimates, 0);
        choice(
            &name("editTool", n),
            c"Edit Tool",
            &[c"Normal", c"Add", c"Delete", c"Split"],
            0,
            &group,
            c"What a click on the curve does: move, add an anchor, delete one, or add one whose tangents are split.",
        )
        .set_int(kOfxParamPropAnimates, 0);
        let p = fx.define_param(kOfxParamTypeCustom, &name("curve", n), curve);
        p.set_str(kOfxParamPropDefault, c"");
        p.set_int(kOfxParamPropAnimates, 1);
        p.set_str(
            kOfxParamPropHint,
            c"The curve's anchors, drawn on the preview in Edit Mode: animate it to change the shape over time.",
        );
        p.set_str(kOfxParamPropParent, &group);
        p.set_ptr(
            kOfxParamPropCustomInterpCallbackV1,
            interpolate as OfxCustomParamInterpFuncV1 as *mut c_void,
        );
        // Motion tracking: the host follows the mask's box.
        let tracking = name("tracking", n);
        let t = fx.define_param(kOfxParamTypeGroup, &tracking, c"Tracking");
        t.set_str(kOfxParamPropParent, &group);
        t.set_int(kOfxParamPropGroupOpen, 0);
        let p = fx.define_param(kOfxParamTypePushButton, &name("trackStart", n), c"Start");
        p.set_str(kOfxParamPropParent, &tracking);
        p.set_str(
            kOfxParamPropHint,
            c"Follows the masked object from the cursor, keyframing the mask's location (and its angle and size, as Mode says).",
        );
        let spec = [
            "location",
            "angle",
            "width",
            "height",
            "precision",
            "trackMode",
            "direction",
            "keyInterval",
        ]
        .iter()
        .fold(n.to_string(), |s, base| {
            format!("{s} {}", name(base, n).to_string_lossy())
        });
        if let Ok(spec) = CString::new(spec) {
            p.set_str(bv_ofx_sys::extensions::kBananaVParamPropMaskTracking, &spec);
        }
        choice(
            &name("precision", n),
            c"Precision",
            &[c"Normal", c"High Precision"],
            0,
            &tracking,
            c"Faster or more accurate tracking.",
        );
        choice(
            &name("trackMode", n),
            c"Mode",
            &[
                c"Location",
                c"Rotation & Location",
                c"Size, Rotation & Location",
            ],
            0,
            &tracking,
            c"What the tracking keyframes: the location, the angle too, or the size too.",
        );
        choice(
            &name("direction", n),
            c"Direction",
            &[c"Forward", c"Backward", c"Both"],
            0,
            &tracking,
            c"Which way in time the object is followed.",
        );
        let p = fx.define_param(
            kOfxParamTypeInteger,
            &name("keyInterval", n),
            c"Keyframe Interval",
        );
        // 5, as the Help's picture shows it.
        p.set_int(kOfxParamPropDefault, 5);
        p.set_int(kOfxParamPropMin, 1);
        p.set_int(kOfxParamPropMax, 100);
        p.set_int(kOfxParamPropDisplayMin, 1);
        p.set_int(kOfxParamPropDisplayMax, 30);
        p.set_str(kOfxParamPropParent, &tracking);
        p.set_str(
            kOfxParamPropHint,
            c"How many frames apart the tracking's keyframes are.",
        );
    }
}

/// A curve between two keyframes (OpenFX's custom parameters'
/// interpolation).
unsafe extern "C" fn interpolate(
    _set: OfxParamSetHandle,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    let (args, out) = (Props(in_args), Props(out_args));
    let a = Curve::parse(&args.text(kOfxParamPropCustomValue, 0));
    let b = Curve::parse(&args.text(kOfxParamPropCustomValue, 1));
    let k = args
        .dbl(kOfxParamPropInterpolationAmount, 0)
        .clamp(0.0, 1.0);
    match CString::new(a.lerp(&b, k).text()) {
        Ok(text) => {
            out.set_str(kOfxParamPropCustomValue, &text);
            kOfxStatOK
        }
        Err(_) => kOfxStatFailed,
    }
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|v| if v > 0.0 { v } else { 1.0 });
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let Some(src) = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time) else {
        return kOfxStatFailed;
    };
    if src.depth != out.depth {
        return kOfxStatErrImageFormat;
    }
    let s = Settings::at(fx, time);
    let (origin, frame) = fx.frame();
    let source = Buf::read(Some(&src));
    drop(src);
    // Mask FX: what the effects before it made, kept inside the masks, the
    // chain's input around it.
    let original = (s.mask_fx)
        .then(|| fx.clip(ORIGINAL))
        .filter(|c| c.connected())
        .and_then(|c| c.image(time))
        .map(|img| Buf::read(Some(&img)));
    let keep = coverage(&s, source.bounds, origin, frame, scale, out.par);
    let w = source.width().max(0) as usize;
    let result = source.map(|x, y| {
        let i = (y - source.bounds[1]) as usize * w + (x - source.bounds[0]) as usize;
        let k = keep.get(i).copied().unwrap_or(1.0);
        let c = source.at(x, y);
        match &original {
            Some(o) => {
                let u = o.inside(x, y);
                std::array::from_fn(|j| u[j] + (c[j] - u[j]) * k)
            }
            None if s.mask_fx => c,
            None => c.map(|v| v * k),
        }
    });
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

// The overlay.

/// What a press on the preview took.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grip {
    /// Inside the mask: it moves.
    Body,
    /// A corner (−1 or 1 across and up): the mask scales, its aspect kept.
    Corner(i8, i8),
    /// A side's middle: the mask stretches one way (across: 0, up: 1).
    Side(u8, i8),
    /// The ring in its middle: the mask turns.
    Ring,
    /// A curve's anchor, or the end of its tangent before or after.
    Anchor(usize),
    Before(usize),
    After(usize),
}

/// A drag under way: which mask, what it took, where the pen was then and
/// the mask as it was.
#[derive(Clone, Debug)]
struct Drag {
    mask: usize,
    grip: Grip,
    from: [f64; 2],
    base: Mask,
    /// A new anchor being pulled out: its tangents follow the pen.
    pulling: bool,
}

/// What the overlay keeps between the pen's actions.
#[derive(Debug, Default)]
struct Overlay {
    hover: Option<(usize, Grip)>,
    drag: Option<Drag>,
    clicks: Clicks,
}

/// The frame on the preview: where it starts and its size (canonical
/// units).
#[derive(Clone, Copy, Debug)]
struct Frame {
    origin: [f64; 2],
    size: [f64; 2],
}

impl Frame {
    /// A point of `m`'s box on the preview (canonical).
    fn at(&self, m: &Mask, p: [f64; 2]) -> [f64; 2] {
        let q = m.place(p, self.size);
        [self.origin[0] + q[0], self.origin[1] + q[1]]
    }

    /// The pen (canonical) in `m`'s box.
    fn local(&self, m: &Mask, pen: [f64; 2]) -> [f64; 2] {
        m.local(
            [pen[0] - self.origin[0], pen[1] - self.origin[1]],
            self.size,
        )
    }
}

/// The masks being edited on the preview: number (1–5) and settings.
fn shown(fx: Fx, time: f64) -> Vec<(usize, Mask, bool)> {
    (1..=MASKS)
        .filter(|&n| fx.param(&name("enable", n)).int_at(time) != 0)
        .map(|n| {
            let m = mask_at(fx, n, time);
            let editing = m.kind == Kind::Curve && fx.param(&name("editMode", n)).int_at(time) != 0;
            (n, m, editing)
        })
        .collect()
}

/// What of mask `m` is under the pen (canonical; `ps` the pixel scale).
fn grip(m: &Mask, editing: bool, f: &Frame, pen: [f64; 2], ps: [f64; 2]) -> Option<Grip> {
    let near = |p: [f64; 2]| {
        let q = f.at(m, p);
        ((pen[0] - q[0]) / ps[0]).hypot((pen[1] - q[1]) / ps[1]) <= HANDLE_REACH
    };
    if editing {
        for (i, a) in m.curve.anchors.iter().enumerate() {
            if near([a.at[0] + a.before[0], a.at[1] + a.before[1]]) && a.before != [0.0; 2] {
                return Some(Grip::Before(i));
            }
            if near([a.at[0] + a.after[0], a.at[1] + a.after[1]]) && a.after != [0.0; 2] {
                return Some(Grip::After(i));
            }
        }
        return m
            .curve
            .anchors
            .iter()
            .position(|a| near(a.at))
            .map(Grip::Anchor);
    }
    if near([0.0, 0.0]) {
        return Some(Grip::Ring);
    }
    for (sx, sy) in [(-1, -1), (1, -1), (1, 1), (-1, 1)] {
        if near([sx as f64 * 0.5, sy as f64 * 0.5]) {
            return Some(Grip::Corner(sx, sy));
        }
    }
    for (axis, side) in [(0u8, -1i8), (0, 1), (1, -1), (1, 1)] {
        let p = if axis == 0 {
            [side as f64 * 0.5, 0.0]
        } else {
            [0.0, side as f64 * 0.5]
        };
        if near(p) {
            return Some(Grip::Side(axis, side));
        }
    }
    let l = f.local(m, pen);
    (l[0].abs() <= 0.5 && l[1].abs() <= 0.5).then_some(Grip::Body)
}

/// Writes mask `n`'s changes (those different from `old`).
fn write(fx: Fx, n: usize, old: &Mask, new: &Mask) {
    let p = |base: &str| fx.param(&name(base, n));
    if new.size != old.size {
        p("width").set_f64(new.size[0]);
        p("height").set_f64(new.size[1]);
    }
    if new.angle != old.angle {
        p("angle").set_f64(new.angle);
    }
    if new.center != old.center {
        p("location").set_xy(new.center);
    }
    if new.curve != old.curve {
        p("curve").set_text(&new.curve.text());
    }
}

/// The edit tool chosen for mask `n`: 0 Normal, 1 Add, 2 Delete, 3 Split.
fn tool(fx: Fx, n: usize, time: f64) -> c_int {
    fx.param(&name("editTool", n)).int_at(time)
}

/// The overlay's entry point.
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
        let state = Box::into_raw(Box::new(Overlay::default()));
        props.set_ptr(kOfxPropInstanceData, state as *mut c_void);
        return kOfxStatOK;
    }
    let state = props.ptr(kOfxPropInstanceData) as *mut Overlay;
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
    let (origin, size) = fx.frame();
    let f = Frame { origin, size };
    let masks = shown(fx, time);
    if is(action, kOfxInteractActionDraw) {
        draw(args, state, &masks, &f, ps);
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    if is(action, kOfxInteractActionPenDown) {
        let double = state
            .clicks
            .press([pen[0] / ps[0], pen[1] / ps[1]], Instant::now());
        // A curve being drawn takes every press.
        if let Some((n, m, _)) = masks.iter().find(|(_, _, e)| *e) {
            return press_curve(fx, state, *n, m, &f, pen, ps, time, double, handle);
        }
        let Some((n, m, g)) = masks
            .iter()
            .rev()
            .find_map(|(n, m, e)| grip(m, *e, &f, pen, ps).map(|g| (*n, m, g)))
        else {
            return kOfxStatReplyDefault;
        };
        state.drag = Some(Drag {
            mask: n,
            grip: g,
            from: pen,
            base: m.clone(),
            pulling: false,
        });
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        let Some(drag) = state.drag.clone() else {
            let hover = masks
                .iter()
                .rev()
                .find_map(|(n, m, e)| grip(m, *e, &f, pen, ps).map(|g| (*n, g)));
            if hover != state.hover {
                state.hover = hover;
                redraw(handle);
            }
            return kOfxStatReplyDefault;
        };
        let now = moved(&drag, &f, pen);
        if let Some((_, m, _)) = masks.iter().find(|(n, _, _)| *n == drag.mask) {
            write(fx, drag.mask, m, &now);
        }
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenUp) {
        if state.drag.take().is_some() {
            redraw(handle);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else {
        kOfxStatReplyDefault
    }
}

/// A press while mask `n`'s curve is being edited: on an anchor or a
/// tangent's end, the Edit Tool's work (Normal and Split move it, Delete
/// deletes the anchor; the first anchor of an open curve closes it); else
/// a new anchor — at the end of an open curve, or on the closed curve with
/// Add and Split —, its tangents pulled out by the drag.
#[allow(clippy::too_many_arguments)]
fn press_curve(
    fx: Fx,
    state: &mut Overlay,
    n: usize,
    m: &Mask,
    f: &Frame,
    pen: [f64; 2],
    ps: [f64; 2],
    time: f64,
    double: bool,
    handle: *const c_void,
) -> OfxStatus {
    let tool = tool(fx, n, time);
    let mut curve = m.curve.clone();
    let at = f.local(m, pen);
    let hit = grip(m, true, f, pen, ps);
    let _ = double;
    match hit {
        Some(Grip::Anchor(0)) if !curve.closed && curve.anchors.len() >= 3 => {
            curve.closed = true;
        }
        Some(Grip::Anchor(i)) if tool == 2 => {
            curve.anchors.remove(i);
            if curve.anchors.len() < 3 {
                curve.closed = false;
            }
        }
        Some(g) => {
            if let (Grip::Anchor(i), 3) = (g, tool)
                && let Some(a) = curve.anchors.get_mut(i)
            {
                a.split = true;
            }
            state.drag = Some(Drag {
                mask: n,
                grip: g,
                from: pen,
                base: Mask {
                    curve: curve.clone(),
                    ..m.clone()
                },
                pulling: false,
            });
            if curve != m.curve {
                write_curve(fx, n, &curve);
            }
            redraw(handle);
            return kOfxStatOK;
        }
        None if !curve.closed => {
            curve.anchors.push(Anchor::corner(at));
            let i = curve.anchors.len() - 1;
            start_pull(state, n, m, &curve, i, pen);
        }
        None if tool == 1 || tool == 3 => {
            let Some(i) = nearest_segment(&curve, at) else {
                return kOfxStatReplyDefault;
            };
            let mut a = Anchor::corner(at);
            a.split = tool == 3;
            curve.anchors.insert(i + 1, a);
            start_pull(state, n, m, &curve, i + 1, pen);
        }
        None => return kOfxStatReplyDefault,
    }
    write_curve(fx, n, &curve);
    redraw(handle);
    kOfxStatOK
}

/// A new anchor `i` whose tangents the drag pulls out.
fn start_pull(state: &mut Overlay, n: usize, m: &Mask, curve: &Curve, i: usize, pen: [f64; 2]) {
    state.drag = Some(Drag {
        mask: n,
        grip: Grip::After(i),
        from: pen,
        base: Mask {
            curve: curve.clone(),
            ..m.clone()
        },
        pulling: true,
    });
}

fn write_curve(fx: Fx, n: usize, curve: &Curve) {
    fx.param(&name("curve", n)).set_text(&curve.text());
}

/// The segment of a closed curve nearest `at` (its first anchor).
fn nearest_segment(curve: &Curve, at: [f64; 2]) -> Option<usize> {
    let n = curve.anchors.len();
    if !curve.closed || n < 2 {
        return None;
    }
    (0..n)
        .map(|i| {
            let (a, b) = (curve.anchors[i], curve.anchors[(i + 1) % n]);
            let p = [
                a.at,
                [a.at[0] + a.after[0], a.at[1] + a.after[1]],
                [b.at[0] + b.before[0], b.at[1] + b.before[1]],
                b.at,
            ];
            let d = (0..=32)
                .map(|s| {
                    let q = bezier(&p, s as f64 / 32.0);
                    (q[0] - at[0]).hypot(q[1] - at[1])
                })
                .fold(f64::INFINITY, f64::min);
            (i, d)
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(i, _)| i)
}

/// The mask as `drag` leaves it with the pen at `pen`.
fn moved(drag: &Drag, f: &Frame, pen: [f64; 2]) -> Mask {
    let mut m = drag.base.clone();
    let b = &drag.base;
    match drag.grip {
        Grip::Body => {
            m.center = [
                b.center[0] + (pen[0] - drag.from[0]) / f.size[0],
                b.center[1] + (pen[1] - drag.from[1]) / f.size[1],
            ];
        }
        Grip::Corner(..) => {
            // Scaled about its middle, its aspect kept: by how far the pen
            // is from the middle now against then.
            let c = f.at(b, [0.0, 0.0]);
            let d0 = (drag.from[0] - c[0]).hypot(drag.from[1] - c[1]).max(1e-9);
            let d1 = (pen[0] - c[0]).hypot(pen[1] - c[1]);
            let k = d1 / d0;
            m.size = [b.size[0] * k, b.size[1] * k];
        }
        Grip::Side(axis, side) => {
            let l = f.local(b, pen);
            let v = (l[axis as usize] * side as f64 * 2.0).max(0.0);
            m.size[axis as usize] = b.size[axis as usize] * v;
        }
        Grip::Ring => {
            let c = f.at(b, [0.0, 0.0]);
            let a0 = (drag.from[1] - c[1]).atan2(drag.from[0] - c[0]);
            let a1 = (pen[1] - c[1]).atan2(pen[0] - c[0]);
            m.angle = b.angle + (a1 - a0).to_degrees();
        }
        Grip::Anchor(i) => {
            let (l0, l1) = (f.local(b, drag.from), f.local(b, pen));
            if let Some(a) = m.curve.anchors.get_mut(i) {
                a.at = [a.at[0] + l1[0] - l0[0], a.at[1] + l1[1] - l0[1]];
            }
        }
        Grip::Before(i) | Grip::After(i) => {
            let l = f.local(b, pen);
            if let Some(a) = m.curve.anchors.get_mut(i) {
                let d = [l[0] - a.at[0], l[1] - a.at[1]];
                let after = matches!(drag.grip, Grip::After(_));
                // A tangent's end; the other turns with it unless they are
                // split (a new anchor's: both, opposite).
                let (this, other) = if after {
                    (&mut a.after, &mut a.before)
                } else {
                    (&mut a.before, &mut a.after)
                };
                *this = d;
                if drag.pulling || !a.split {
                    let len = if drag.pulling {
                        d[0].hypot(d[1])
                    } else {
                        other[0].hypot(other[1])
                    };
                    let dl = d[0].hypot(d[1]).max(1e-12);
                    *other = [-d[0] / dl * len, -d[1] / dl * len];
                }
            }
        }
    }
    m
}

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 0.95];
const YELLOW: [f32; 4] = [1.0, 0.85, 0.2, 1.0];
const DARK: [f32; 4] = [0.0, 0.0, 0.0, 0.7];

/// The masks' outlines, dashed while a curve is drawn, light over dark;
/// their handles: white corners, yellow sides, the ring in the middle; a
/// curve's anchors and tangents while it is edited.
fn draw(args: Props, state: &Overlay, masks: &[(usize, Mask, bool)], f: &Frame, ps: [f64; 2]) {
    let Some(d) = Draw::of(args) else { return };
    let square = |c: [f64; 2], r: f64| {
        let (rx, ry) = (r * ps[0], r * ps[1]);
        [
            [c[0] - rx, c[1] - ry],
            [c[0] + rx, c[1] - ry],
            [c[0] + rx, c[1] + ry],
            [c[0] - rx, c[1] + ry],
        ]
    };
    for (n, m, editing) in masks {
        let hot = |g: Grip| {
            state.hover == Some((*n, g))
                || state
                    .drag
                    .as_ref()
                    .is_some_and(|d| d.mask == *n && d.grip == g)
        };
        // The outline: the shape's (an open curve's line so far).
        let outline: Vec<[f64; 2]> = if m.kind == Kind::Curve {
            m.curve.polygon().into_iter().map(|p| f.at(m, p)).collect()
        } else {
            m.outline().into_iter().map(|p| f.at(m, p)).collect()
        };
        let mut lines = Vec::new();
        let closed = m.kind != Kind::Curve || m.curve.closed;
        let count = outline.len();
        let segments = if closed {
            count
        } else {
            count.saturating_sub(1)
        };
        for i in 0..segments {
            // Dashed while drawn: every other piece.
            if *editing && !m.curve.closed && i % 2 == 1 {
                continue;
            }
            lines.push(outline[i]);
            lines.push(outline[(i + 1) % count]);
        }
        for (colour, width) in [(DARK, 3.0), (WHITE, 1.2)] {
            d.colour(colour);
            d.width(width);
            d.lines(&lines);
        }
        if *editing {
            for (i, a) in m.curve.anchors.iter().enumerate() {
                let at = f.at(m, a.at);
                for (end, g) in [(a.before, Grip::Before(i)), (a.after, Grip::After(i))] {
                    if end == [0.0; 2] {
                        continue;
                    }
                    let e = f.at(m, [a.at[0] + end[0], a.at[1] + end[1]]);
                    d.colour(YELLOW);
                    d.width(1.0);
                    d.lines(&[at, e]);
                    d.colour(if hot(g) { WHITE } else { YELLOW });
                    d.fill(&square(e, 3.0));
                }
                d.colour(if hot(Grip::Anchor(i)) || i == 0 {
                    YELLOW
                } else {
                    WHITE
                });
                d.fill(&square(at, 4.0));
                d.colour(DARK);
                d.width(1.0);
                d.outline(&square(at, 4.0));
            }
            continue;
        }
        if m.kind == Kind::Curve && !m.curve.closed {
            continue;
        }
        // Corners white, sides yellow, the ring in the middle.
        for (sx, sy) in [(-1i8, -1i8), (1, -1), (1, 1), (-1, 1)] {
            let c = f.at(m, [sx as f64 * 0.5, sy as f64 * 0.5]);
            let r = if hot(Grip::Corner(sx, sy)) { 5.0 } else { 4.0 };
            d.colour(WHITE);
            d.fill(&square(c, r));
            d.colour(DARK);
            d.width(1.0);
            d.outline(&square(c, r));
        }
        for (axis, side) in [(0u8, -1i8), (0, 1), (1, -1), (1, 1)] {
            let p = if axis == 0 {
                [side as f64 * 0.5, 0.0]
            } else {
                [0.0, side as f64 * 0.5]
            };
            let c = f.at(m, p);
            let r = if hot(Grip::Side(axis, side)) {
                5.0
            } else {
                4.0
            };
            d.colour(YELLOW);
            d.fill(&square(c, r));
            d.colour(DARK);
            d.width(1.0);
            d.outline(&square(c, r));
        }
        let c = f.at(m, [0.0, 0.0]);
        let ring: Vec<[f64; 2]> = (0..=24)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 24.0;
                [c[0] + 9.0 * ps[0] * a.cos(), c[1] + 9.0 * ps[1] * a.sin()]
            })
            .collect();
        for (colour, width) in [
            (DARK, 3.0),
            (if hot(Grip::Ring) { YELLOW } else { WHITE }, 1.2),
        ] {
            d.colour(colour);
            d.width(width);
            d.outline(&ring);
        }
    }
}

// Presets.

/// The factory presets.
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: f64| PresetValue::Num(vec![v]);
    // VEGAS's: a shape 0.3 of the frame across and 0.533 up (as wide as
    // high on a 16:9 frame) in the middle.
    let shape = |name: &'static str, kind: f64| Preset {
        name,
        values: vec![
            ("type1", num(kind)),
            ("width1", num(0.3)),
            ("height1", num(0.533)),
        ],
    };
    vec![
        shape("Circle", 0.0),
        shape("Square", 1.0),
        shape("Diamond", 2.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(masks: Vec<Mask>) -> Settings {
        Settings {
            masks,
            invert: false,
            mask_fx: false,
            blend: 0.0,
        }
    }

    fn mask(kind: Kind) -> Mask {
        Mask {
            kind,
            size: [0.5, 0.5],
            angle: 0.0,
            center: [0.5, 0.5],
            opacity: 1.0,
            feather: 0.0,
            style: Feather::Out,
            curve: Curve::default(),
        }
    }

    /// The coverage of a 100 × 100 frame (square pixels, whole scale).
    fn cover(s: &Settings) -> Vec<f32> {
        coverage(
            s,
            [0, 0, 100, 100],
            [0.0, 0.0],
            [100.0, 100.0],
            [1.0, 1.0],
            1.0,
        )
    }

    fn at(c: &[f32], x: usize, y: usize) -> f32 {
        c[y * 100 + x]
    }

    /// A rectangle keeps what is inside it; an oval and a diamond their
    /// shapes; Invert the rest; Opacity fades the inside; Blend brings back
    /// what is left out.
    #[test]
    fn shapes_keep_their_insides() {
        let c = cover(&settings(vec![mask(Kind::Rectangle)]));
        assert_eq!(at(&c, 50, 50), 1.0);
        assert_eq!(at(&c, 5, 5), 0.0);
        assert_eq!(at(&c, 30, 70), 1.0);
        let c = cover(&settings(vec![mask(Kind::Oval)]));
        assert_eq!(at(&c, 50, 50), 1.0);
        assert_eq!(at(&c, 27, 27), 0.0, "the oval leaves its box's corners");
        let c = cover(&settings(vec![mask(Kind::Diamond)]));
        assert_eq!(at(&c, 50, 30), 1.0);
        assert_eq!(at(&c, 30, 30), 0.0);
        let mut s = settings(vec![mask(Kind::Rectangle)]);
        s.invert = true;
        let c = cover(&s);
        assert_eq!(at(&c, 50, 50), 0.0);
        assert_eq!(at(&c, 5, 5), 1.0);
        let mut m = mask(Kind::Rectangle);
        m.opacity = 0.25;
        let c = cover(&settings(vec![m]));
        assert_eq!(at(&c, 50, 50), 0.25);
        assert_eq!(at(&c, 5, 5), 0.0);
        let mut s = settings(vec![mask(Kind::Rectangle)]);
        s.blend = 0.5;
        assert!((at(&cover(&s), 5, 5) - 0.5).abs() < 1e-6);
    }

    /// The feather softens the edge out of the shape, into it, or both
    /// ways; an angle turns the shape.
    #[test]
    fn feathers_and_angles() {
        let mut m = mask(Kind::Rectangle);
        m.feather = 10.0;
        let out = cover(&settings(vec![m.clone()]));
        // 10 pixels out of the edge at 25: half way at about 20.
        assert_eq!(at(&out, 30, 50), 1.0);
        assert!(at(&out, 20, 50) > 0.2 && at(&out, 20, 50) < 0.8);
        assert_eq!(at(&out, 10, 50), 0.0);
        m.style = Feather::In;
        let inward = cover(&settings(vec![m.clone()]));
        assert_eq!(at(&inward, 20, 50), 0.0);
        assert!(at(&inward, 30, 50) > 0.2 && at(&inward, 30, 50) < 0.8);
        m.style = Feather::Both;
        let both = cover(&settings(vec![m]));
        assert!((at(&both, 25, 50) - 0.5).abs() < 0.2);
        // A thin bar turned a quarter: up and down instead of across.
        let mut bar = mask(Kind::Rectangle);
        bar.size = [0.8, 0.1];
        bar.angle = 90.0;
        let c = cover(&settings(vec![bar]));
        assert_eq!(at(&c, 50, 15), 1.0);
        assert_eq!(at(&c, 15, 50), 0.0);
    }

    /// Masks add up by their Opacity; no mask keeps Blend of the picture
    /// (with Mask FX, 1 − Blend of the effects); with Mask FX, Blend brings
    /// back the chain's input, or, inverted, the effects.
    #[test]
    fn masks_add_up_and_blend_as_vegas_does() {
        let mut a = mask(Kind::Rectangle);
        a.opacity = 0.5;
        let mut b = a.clone();
        b.center = [0.6, 0.5];
        let c = cover(&settings(vec![a.clone(), b]));
        assert_eq!(at(&c, 30, 50), 0.5, "one");
        assert_eq!(at(&c, 55, 50), 1.0, "both");
        let mut s = settings(Vec::new());
        s.blend = 0.25;
        assert_eq!(at(&cover(&s), 50, 50), 0.25);
        s.mask_fx = true;
        assert_eq!(at(&cover(&s), 50, 50), 0.75);
        let mut s = settings(vec![mask(Kind::Rectangle)]);
        s.mask_fx = true;
        s.blend = 0.25;
        let c = cover(&s);
        assert_eq!((at(&c, 50, 50), at(&c, 5, 5)), (0.75, 0.0));
        s.invert = true;
        let c = cover(&s);
        assert_eq!((at(&c, 50, 50), at(&c, 5, 5)), (0.25, 1.0));
        // VEGAS's option orders and its three presets.
        assert_eq!(Kind::of(0), Kind::Oval);
        assert_eq!(Kind::of(1), Kind::Rectangle);
        assert_eq!(Feather::of(0), Feather::In);
        assert_eq!(Feather::of(2), Feather::Both);
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(names, ["Circle", "Square", "Diamond"]);
    }

    /// A curve's mask once closed; its text written and read back; two
    /// curves of as many anchors in between.
    #[test]
    fn curves_close_into_masks() {
        let mut m = mask(Kind::Curve);
        m.curve = Curve {
            anchors: vec![
                Anchor::corner([-0.5, -0.5]),
                Anchor::corner([0.5, -0.5]),
                Anchor::corner([0.0, 0.5]),
            ],
            closed: false,
        };
        assert!(cover(&settings(vec![m.clone()])).iter().all(|v| *v == 0.0));
        m.curve.closed = true;
        let c = cover(&settings(vec![m.clone()]));
        assert_eq!(at(&c, 50, 40), 1.0);
        assert_eq!(at(&c, 30, 70), 0.0);
        let text = m.curve.text();
        assert_eq!(Curve::parse(&text), m.curve);
        let mut moved = m.curve.clone();
        moved.anchors[2].at = [0.0, 0.3];
        let half = m.curve.lerp(&moved, 0.5);
        assert!((half.anchors[2].at[1] - 0.4).abs() < 1e-9);
        assert_eq!(Curve::parse("x 1 2"), Curve::default());
    }

    /// The distance to a segment: across it, past its ends; the edge
    /// softens by it, half each way with Both.
    #[test]
    fn edges_soften_by_their_distance() {
        assert_eq!(segment_distance([1.0, 3.0], [0.0, 0.0], [4.0, 0.0]), 3.0);
        assert_eq!(segment_distance([7.0, 4.0], [0.0, 0.0], [4.0, 0.0]), 5.0);
        assert_eq!(softening(10.0, Feather::Both), [-5.0, 5.0]);
        assert_eq!(softening(0.5, Feather::In), [-0.5, 0.5]);
        // A square 20 pixels across, its edge softened 8 pixels out of it.
        let square = [[10.0, 10.0], [30.0, 10.0], [30.0, 30.0], [10.0, 30.0]];
        let c = shape_coverage(&square, 40, 40, 8.0, Feather::Out);
        assert_eq!(c[20 * 40 + 20], 1.0);
        assert_eq!(c[20 * 40 + 1], 0.0);
        let half = smooth(-8.0, 0.0, -4.0) as f32;
        assert!((c[20 * 40 + 6] - half).abs() < 1e-6, "{}", c[20 * 40 + 6]);
    }
}
