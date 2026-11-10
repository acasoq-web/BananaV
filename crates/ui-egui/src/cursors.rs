//! BananaV's own mouse pointers: the ones an editor's timeline, Pan/Crop
//! and other windows ask for and the system has not — trimming an event's
//! edge, slipping its media, stretching its time, fading it, painting an
//! envelope, turning a frame, a mask's pen —, each drawn by BananaV and
//! handed to the system as its pointer (sharp at any scale, never clipped
//! by the window, as fast as the system's own).
//!
//! Where each one shows follows VEGAS's choices (the timeline's zones, keys
//! and tools, Pan/Crop's handles…); the drawings are BananaV's. They share
//! one system, so they read as a set:
//!
//! - A grid of 32 units, a pixel each at 100 % (more pixels on larger
//!   screens: drawn again at that size, never stretched).
//! - A white body in a black outline a unit wide, its details black on
//!   top; nothing but black and white, so a pointer reads on any picture.
//! - A few parts make them all: the arrow, double arrows, an event's edge
//!   and its body, a filmstrip (the media), hands, a magnifier, a pen's
//!   nib, a pencil, and badges (+, −, ×, ++, ↕, ↔).
//!
//! Where VEGAS shows the system's pointer (the arrow, the I-beam, waiting,
//! "no") BananaV shows the system's too: [`system`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2, PI};
use std::sync::Arc;

use crate::egui::{self, Color32, ColorImage, CursorIcon, Pos2, Rect, TextureHandle, vec2};

/// A pointer of BananaV's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Cursor {
    // An event on the timeline.
    /// Its start's edge: trims it.
    TrimStart,
    /// Its end's edge: trims it.
    TrimEnd,
    /// Its fade-in's handle.
    FadeIn,
    /// Its fade-out's handle.
    FadeOut,
    /// Ctrl on its start's edge: stretches (or compresses) its time.
    StretchStart,
    /// Ctrl on its end's edge.
    StretchEnd,
    /// Alt on its start's edge: trims it, the media moving with the edge.
    SlipTrimStart,
    /// Alt on its end's edge (Alt+Shift anywhere on it).
    SlipTrimEnd,
    /// [`Cursor::SlipTrimStart`] for all its takes.
    SlipTrimStartAll,
    /// [`Cursor::SlipTrimEnd`] for all its takes.
    SlipTrimEndAll,
    /// Alt on its middle: its media slips, its edges stay.
    Slip,
    /// [`Cursor::Slip`] for all its takes.
    SlipAll,
    /// Ctrl+Alt on its middle: it slides, its neighbours' edges following.
    Slide,
    /// Ctrl+Alt over a crossfade: the crossfade slides.
    SlideCrossfade,
    /// Ctrl+Alt on an edge: it and the adjacent event's are trimmed.
    TrimAdjacent,
    /// Ctrl+Alt+Shift on its middle (or Split Trim's tool): splits it there
    /// and trims from the split.
    SplitTrim,
    /// Ctrl by the edit cursor: scrubs.
    Scrub,

    // The timeline's tools.
    /// The Selection tool's.
    SelectTool,
    /// The Envelope tool's.
    EnvelopeTool,
    /// The Zoom tool's.
    Magnifier,
    /// Zooms in (Pan/Crop's Zoom tool).
    ZoomIn,
    /// Zooms out (the same, the right button down).
    ZoomOut,

    // Pan/Crop's and Track Motion's frame.
    /// Inside the frame: moves it.
    MoveFrame,
    /// Resizes across (the frame's sides; a selection's edges, the edit
    /// cursor's line, a scroll bar's ends).
    ResizeEw,
    /// Resizes up and down.
    ResizeNs,
    /// Resizes along the rising diagonal.
    ResizeNesw,
    /// Resizes along the falling diagonal.
    ResizeNwse,
    /// Outside the frame: turns it.
    Rotate,
    /// The center's circle: moves only the center.
    MoveCenter,

    // Masks' paths.
    /// Creates an anchor.
    Pen,
    /// Adds an anchor on a segment.
    PenAdd,
    /// Deletes an anchor.
    PenDelete,
    /// Splits an anchor's tangents.
    PenSplitTangent,
    /// Drags a tangent.
    PenTangent,
    /// Draws a rectangle or an oval mask.
    Crosshair,
    /// A crosshair adding a rectangle (VEGAS has no known use for it).
    #[allow(dead_code)]
    CrosshairRectPlus,
    /// A crosshair that can't (VEGAS has no known use for it).
    #[allow(dead_code)]
    CrosshairRectNo,

    // Hands.
    /// Something to click (an event's button, a label, a meter's peak).
    HandPoint,
    /// Shift on an envelope's line: a new point (BananaV's Shift paints envelopes instead).
    #[allow(dead_code)]
    HandPlus,
    /// An event's level line; an envelope's line held.
    HandUpDown,
    /// An event's snap offset.
    HandLeftRight,
    /// Dragging an envelope's point.
    HandMove,
    /// A view to drag (the timeline's overview, a zoomed preview).
    HandOpen,
    /// Dragging it.
    HandClosed,

    // Splitters and handles.
    /// A scroll bar's zoom handle, across.
    ResizeBarEw,
    /// A scroll bar's zoom handle, up and down (BananaV's scroll bars have
    /// none).
    #[allow(dead_code)]
    ResizeBarNs,
    /// The divider between the track headers and the timeline.
    SplitterEw,
    /// A track's bottom edge (its height); a window's divider.
    SplitterNs,

    // Arrows with a mark.
    /// The arrow (BananaV's; where the system's shows, it is the system's).
    Arrow,
    /// The arrow, mirrored (VEGAS has no known use for it).
    #[allow(dead_code)]
    ArrowMirrored,
    /// The arrow with ↕ (VEGAS has no known use for it).
    #[allow(dead_code)]
    ArrowUpDown,
    /// A marker's or a region's flag: moves it.
    ArrowLeftRight,
    /// Something dragged (Shuffle's events, media dragged in).
    Drag,
    /// Something dragged to be copied (BananaV's drags don't copy by the pointer yet).
    #[allow(dead_code)]
    DragCopy,
    /// Several things dragged to be copied (BananaV's drags don't copy by the pointer yet).
    #[allow(dead_code)]
    DragCopyMany,
    /// Where something dragged can't go.
    ArrowNo,
    /// The arrow adding (VEGAS has no known use for it).
    #[allow(dead_code)]
    ArrowPlus,
    /// The arrow taking away (VEGAS has no known use for it).
    #[allow(dead_code)]
    ArrowMinus,
    /// The arrow deleting (VEGAS has no known use for it).
    #[allow(dead_code)]
    ArrowCross,
    /// Dragging, marked C (VEGAS has no known use for it).
    #[allow(dead_code)]
    DragC,
    /// Dragging, marked P (VEGAS has no known use for it).
    #[allow(dead_code)]
    DragP,
    /// Dragging, marked M (VEGAS has no known use for it).
    #[allow(dead_code)]
    DragM,
    /// The arrow marked L, a plug-in's left side (BananaV's plug-in windows
    /// have no use for it yet).
    #[allow(dead_code)]
    ArrowL,
    /// The arrow marked R, a plug-in's right side (BananaV's plug-in windows
    /// have no use for it yet).
    #[allow(dead_code)]
    ArrowR,

    // A grid's columns and rows.
    /// Moves a column (its header).
    ColumnMove,
    /// Moves columns (BananaV's grids don't do it yet).
    #[allow(dead_code)]
    ColumnsMove,
    /// Moves a row (BananaV's grids don't do it yet).
    #[allow(dead_code)]
    RowMove,
    /// Moves rows (BananaV's grids don't do it yet).
    #[allow(dead_code)]
    RowsMove,
    /// Inserts a column (BananaV's grids don't do it yet).
    #[allow(dead_code)]
    ColumnInsert,
    /// Inserts columns (BananaV's grids don't do it yet).
    #[allow(dead_code)]
    ColumnsInsert,
    /// Inserts a row (BananaV's grids don't do it yet).
    #[allow(dead_code)]
    RowInsert,
    /// Inserts rows (BananaV's grids don't do it yet).
    #[allow(dead_code)]
    RowsInsert,
    /// Deletes a column.
    ColumnDelete,

    // Drawing.
    /// A pencil (VEGAS has no known use for it).
    #[allow(dead_code)]
    Pencil,
    /// Shift on an envelope with painting on: paints it.
    PencilCrosshair,
    /// Picks a color from the screen.
    Dropper,

    // The wheel's button held: scrolling toward a side.
    ScrollUp,
    ScrollRight,
    ScrollDown,
    ScrollLeft,
    ScrollUpRight,
    ScrollDownRight,
    ScrollUpLeft,
    ScrollDownLeft,
    /// Not scrolling, up and down the only way (BananaV's wheel scrolling goes both ways).
    #[allow(dead_code)]
    ScrollStopNs,
    /// Not scrolling, across the only way (BananaV's wheel scrolling goes both ways).
    #[allow(dead_code)]
    ScrollStopEw,
    /// Not scrolling, any way.
    ScrollStopAll,
    /// The mark where the scrolling started, up and down (BananaV's wheel
    /// scrolling goes both ways).
    #[allow(dead_code)]
    OriginNs,
    /// The same, across (and where a scrub started).
    OriginEw,
    /// The same, any way.
    OriginAll,
}

impl Cursor {
    /// Every one, for looking at them together.
    #[cfg(test)]
    pub(crate) const ALL: [Cursor; 90] = [
        Cursor::TrimStart,
        Cursor::TrimEnd,
        Cursor::FadeIn,
        Cursor::FadeOut,
        Cursor::StretchStart,
        Cursor::StretchEnd,
        Cursor::SlipTrimStart,
        Cursor::SlipTrimEnd,
        Cursor::SlipTrimStartAll,
        Cursor::SlipTrimEndAll,
        Cursor::Slip,
        Cursor::SlipAll,
        Cursor::Slide,
        Cursor::SlideCrossfade,
        Cursor::TrimAdjacent,
        Cursor::SplitTrim,
        Cursor::Scrub,
        Cursor::SelectTool,
        Cursor::EnvelopeTool,
        Cursor::Magnifier,
        Cursor::ZoomIn,
        Cursor::ZoomOut,
        Cursor::MoveFrame,
        Cursor::ResizeEw,
        Cursor::ResizeNs,
        Cursor::ResizeNesw,
        Cursor::ResizeNwse,
        Cursor::Rotate,
        Cursor::MoveCenter,
        Cursor::Pen,
        Cursor::PenAdd,
        Cursor::PenDelete,
        Cursor::PenSplitTangent,
        Cursor::PenTangent,
        Cursor::Crosshair,
        Cursor::CrosshairRectPlus,
        Cursor::CrosshairRectNo,
        Cursor::HandPoint,
        Cursor::HandPlus,
        Cursor::HandUpDown,
        Cursor::HandLeftRight,
        Cursor::HandMove,
        Cursor::HandOpen,
        Cursor::HandClosed,
        Cursor::ResizeBarEw,
        Cursor::ResizeBarNs,
        Cursor::SplitterEw,
        Cursor::SplitterNs,
        Cursor::Arrow,
        Cursor::ArrowMirrored,
        Cursor::ArrowUpDown,
        Cursor::ArrowLeftRight,
        Cursor::Drag,
        Cursor::DragCopy,
        Cursor::DragCopyMany,
        Cursor::ArrowNo,
        Cursor::ArrowPlus,
        Cursor::ArrowMinus,
        Cursor::ArrowCross,
        Cursor::DragC,
        Cursor::DragP,
        Cursor::DragM,
        Cursor::ArrowL,
        Cursor::ArrowR,
        Cursor::ColumnMove,
        Cursor::ColumnsMove,
        Cursor::RowMove,
        Cursor::RowsMove,
        Cursor::ColumnInsert,
        Cursor::ColumnsInsert,
        Cursor::RowInsert,
        Cursor::RowsInsert,
        Cursor::ColumnDelete,
        Cursor::Pencil,
        Cursor::PencilCrosshair,
        Cursor::Dropper,
        Cursor::ScrollUp,
        Cursor::ScrollRight,
        Cursor::ScrollDown,
        Cursor::ScrollLeft,
        Cursor::ScrollUpRight,
        Cursor::ScrollDownRight,
        Cursor::ScrollUpLeft,
        Cursor::ScrollDownLeft,
        Cursor::ScrollStopNs,
        Cursor::ScrollStopEw,
        Cursor::ScrollStopAll,
        Cursor::OriginNs,
        Cursor::OriginEw,
        Cursor::OriginAll,
    ];

    /// The system's pointer nearest it, where BananaV's can't show.
    pub(crate) fn fallback(self) -> CursorIcon {
        use Cursor::*;
        match self {
            TrimStart | TrimEnd | FadeIn | FadeOut | StretchStart | StretchEnd | SlipTrimStart
            | SlipTrimEnd | SlipTrimStartAll | SlipTrimEndAll | Slip | SlipAll | Slide
            | SlideCrossfade | TrimAdjacent | SplitTrim | Scrub | ResizeEw | ArrowLeftRight
            | ScrollStopEw | OriginEw => CursorIcon::ResizeHorizontal,
            ResizeNs | ArrowUpDown | ScrollStopNs | OriginNs => CursorIcon::ResizeVertical,
            ResizeNesw => CursorIcon::ResizeNeSw,
            ResizeNwse => CursorIcon::ResizeNwSe,
            ResizeBarEw | SplitterEw | ColumnMove | ColumnsMove | ColumnInsert | ColumnsInsert
            | ColumnDelete => CursorIcon::ResizeColumn,
            ResizeBarNs | SplitterNs | RowMove | RowsMove | RowInsert | RowsInsert => {
                CursorIcon::ResizeRow
            }
            SelectTool | Pen | PenAdd | PenDelete | PenSplitTangent | PenTangent | Crosshair
            | CrosshairRectPlus | Pencil | PencilCrosshair | Dropper => CursorIcon::Crosshair,
            Magnifier | ZoomIn => CursorIcon::ZoomIn,
            ZoomOut => CursorIcon::ZoomOut,
            MoveFrame | MoveCenter | HandMove => CursorIcon::Move,
            Rotate | HandOpen => CursorIcon::Grab,
            HandClosed => CursorIcon::Grabbing,
            HandPoint | HandPlus | HandUpDown | HandLeftRight => CursorIcon::PointingHand,
            DragCopy | DragCopyMany | ArrowPlus => CursorIcon::Copy,
            ArrowNo | CrosshairRectNo => CursorIcon::NoDrop,
            ScrollUp | ScrollRight | ScrollDown | ScrollLeft | ScrollUpRight | ScrollDownRight
            | ScrollUpLeft | ScrollDownLeft | ScrollStopAll | OriginAll => CursorIcon::AllScroll,
            EnvelopeTool | Arrow | ArrowMirrored | Drag | ArrowMinus | ArrowCross | DragC
            | DragP | DragM | ArrowL | ArrowR => CursorIcon::Default,
        }
    }
}

// Showing them.

/// Shows `c` as the pointer this frame (the window's, while the pointer is
/// over it): BananaV's drawing at the screen's scale, the system's nearest
/// where that can't show.
pub(crate) fn set(ctx: &egui::Context, c: Cursor) {
    ctx.set_cursor_icon(c.fallback());
    ctx.set_cursor_image(Some(image(c, size_for(ctx.pixels_per_point()))));
}

/// Shows the system's own pointer `icon` this frame (none of BananaV's).
pub(crate) fn system(ctx: &egui::Context, icon: CursorIcon) {
    ctx.set_cursor_icon(icon);
    ctx.set_cursor_image(None);
}

/// The pointer's size in pixels at `pixels_per_point`: 32 at 100 %.
fn size_for(pixels_per_point: f32) -> u16 {
    let ppp = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
        pixels_per_point
    } else {
        1.0
    };
    (UNITS * ppp).round().clamp(16.0, 256.0) as u16
}

thread_local! {
    /// Each pointer drawn at each size, once: the system keeps its own by
    /// these pictures' addresses, so they live as long as BananaV.
    static PICTURES: RefCell<HashMap<(Cursor, u16), egui::CustomCursorImage>> =
        RefCell::new(HashMap::new());
}

/// `c` drawn `size` pixels square, for the system.
pub(crate) fn image(c: Cursor, size: u16) -> egui::CustomCursorImage {
    PICTURES.with(|p| {
        p.borrow_mut()
            .entry((c, size))
            .or_insert_with(|| {
                let d = c.drawing();
                let k = f32::from(size) / UNITS;
                let spot = |v: f32| ((v * k).floor().max(0.0) as u16).min(size - 1);
                egui::CustomCursorImage {
                    rgba: Arc::from(render(&d, size)),
                    size: [size, size],
                    hotspot: [spot(d.hotspot[0]), spot(d.hotspot[1])],
                }
            })
            .clone()
    })
}

/// Paints `c` with its hot spot at `at` (the marks left where the wheel's
/// scrolling or a scrub began), as large as the pointer.
pub(crate) fn paint(painter: &egui::Painter, c: Cursor, at: Pos2) {
    let ctx = painter.ctx();
    let ppp = ctx.pixels_per_point();
    let size = size_for(ppp);
    let id = egui::Id::new(("bananav-cursor", c, size));
    let texture = match ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        Some(t) => t,
        None => {
            let img = image(c, size);
            let n = usize::from(size);
            let pixels = img
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
                .collect();
            let t = ctx.load_texture(
                format!("bananav-cursor-{c:?}-{size}"),
                ColorImage::new([n, n], pixels),
                egui::TextureOptions::LINEAR,
            );
            ctx.data_mut(|d| d.insert_temp(id, t.clone()));
            t
        }
    };
    let side = f32::from(size) / ppp;
    let hot = c.drawing().hotspot;
    let min = at - vec2(hot[0], hot[1]) * (side / UNITS);
    painter.image(
        texture.id(),
        Rect::from_min_size(min, vec2(side, side)),
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}

// Drawing them.

/// The grid's side.
const UNITS: f32 = 32.0;
/// The outline's width, units.
const OUTLINE: f32 = 1.0;

type P = [f32; 2];

/// A part of a drawing, in units (y down).
#[derive(Clone, Debug)]
enum Shape {
    /// A filled polygon (even-odd).
    Poly(Vec<P>),
    /// A line `w` wide with round ends.
    Line(P, P, f32),
    /// A disk: center, radius.
    Disk(P, f32),
    /// A circle's line: center, radius to its middle, width.
    Ring(P, f32, f32),
    /// Part of a circle's line, clockwise from angle `a0` to `a1`
    /// (radians, 0 to the right, π/2 down), round at its ends: center,
    /// radius, width, a0, a1.
    Arc(P, f32, f32, f32, f32),
}

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

fn len(v: P) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

/// How far `p` is from the segment `a`–`b`.
fn to_segment(p: P, a: P, b: P) -> f32 {
    let ab = sub(b, a);
    let ap = sub(p, a);
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if l2 > 0.0 {
        ((ap[0] * ab[0] + ap[1] * ab[1]) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    len(sub(ap, [ab[0] * t, ab[1] * t]))
}

impl Shape {
    /// How far `p` is outside it (less than 0 inside).
    fn distance(&self, p: P) -> f32 {
        match self {
            Shape::Poly(pts) => {
                let mut d = f32::INFINITY;
                let mut inside = false;
                let n = pts.len();
                for i in 0..n {
                    let (a, b) = (pts[i], pts[(i + 1) % n]);
                    d = d.min(to_segment(p, a, b));
                    if (a[1] > p[1]) != (b[1] > p[1])
                        && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
                    {
                        inside = !inside;
                    }
                }
                if inside { -d } else { d }
            }
            Shape::Line(a, b, w) => to_segment(p, *a, *b) - w / 2.0,
            Shape::Disk(c, r) => len(sub(p, *c)) - r,
            Shape::Ring(c, r, w) => (len(sub(p, *c)) - r).abs() - w / 2.0,
            Shape::Arc(c, r, w, a0, a1) => {
                let v = sub(p, *c);
                let mut t = v[1].atan2(v[0]);
                while t < *a0 {
                    t += 2.0 * PI;
                }
                if t <= *a1 {
                    (len(v) - r).abs() - w / 2.0
                } else {
                    let end = |a: f32| [c[0] + r * a.cos(), c[1] + r * a.sin()];
                    len(sub(p, end(*a0))).min(len(sub(p, end(*a1)))) - w / 2.0
                }
            }
        }
    }

    /// Moved by `f`, its widths times `k`; angles turned by `turn` and,
    /// with `mirror`, mirrored across.
    fn map(&self, f: &dyn Fn(P) -> P, k: f32, turn: f32, mirror: bool) -> Shape {
        let angles = |a0: f32, a1: f32| {
            if mirror {
                (PI - a1 + turn, PI - a0 + turn)
            } else {
                (a0 + turn, a1 + turn)
            }
        };
        match self {
            Shape::Poly(pts) => Shape::Poly(pts.iter().map(|&q| f(q)).collect()),
            Shape::Line(a, b, w) => Shape::Line(f(*a), f(*b), w * k),
            Shape::Disk(c, r) => Shape::Disk(f(*c), r * k),
            Shape::Ring(c, r, w) => Shape::Ring(f(*c), r * k, w * k),
            Shape::Arc(c, r, w, a0, a1) => {
                let (b0, b1) = angles(*a0, *a1);
                Shape::Arc(f(*c), r * k, w * k, b0, b1)
            }
        }
    }
}

/// A pointer's drawing: its white body (outlined), its black details and
/// its hot spot (the point that clicks), in units.
#[derive(Clone, Debug, Default)]
struct Drawing {
    white: Vec<Shape>,
    black: Vec<Shape>,
    hotspot: P,
}

impl Drawing {
    fn at(x: f32, y: f32) -> Drawing {
        Drawing {
            hotspot: [x, y],
            ..Drawing::default()
        }
    }

    fn w(mut self, s: Shape) -> Drawing {
        self.white.push(s);
        self
    }

    fn b(mut self, s: Shape) -> Drawing {
        self.black.push(s);
        self
    }

    /// `other`'s parts too (this one's hot spot).
    fn with(mut self, other: Drawing) -> Drawing {
        self.white.extend(other.white);
        self.black.extend(other.black);
        self
    }

    fn map(self, f: &dyn Fn(P) -> P, k: f32, turn: f32, mirror: bool) -> Drawing {
        Drawing {
            white: self
                .white
                .iter()
                .map(|s| s.map(f, k, turn, mirror))
                .collect(),
            black: self
                .black
                .iter()
                .map(|s| s.map(f, k, turn, mirror))
                .collect(),
            hotspot: f(self.hotspot),
        }
    }

    /// Mirrored across (left for right).
    fn mirror(self) -> Drawing {
        self.map(&|p| [UNITS - p[0], p[1]], 1.0, 0.0, true)
    }

    /// Turned a quarter clockwise about the middle.
    fn turn(self) -> Drawing {
        let c = UNITS / 2.0;
        self.map(&|p| [c - (p[1] - c), c + (p[0] - c)], 1.0, FRAC_PI_2, false)
    }

    /// Scaled `k` about `about`, then moved by `by`.
    fn scaled(self, about: P, k: f32, by: P) -> Drawing {
        self.map(
            &|p| {
                [
                    about[0] + (p[0] - about[0]) * k + by[0],
                    about[1] + (p[1] - about[1]) * k + by[1],
                ]
            },
            k,
            0.0,
            false,
        )
    }
}

// The parts.

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Shape {
    Shape::Poly(vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
}

/// The arrow, its tip at (1.4, 1.4).
fn arrow() -> Drawing {
    Drawing::at(1.0, 1.0).w(Shape::Poly(vec![
        [1.4, 1.4],
        [1.4, 21.2],
        [6.1, 16.8],
        [9.2, 23.9],
        [12.5, 22.5],
        [9.4, 15.6],
        [15.8, 15.6],
    ]))
}

/// A double arrow from `a` to `b`: its shaft `shaft` wide, its heads
/// `head` wide and `long` long.
fn double(a: P, b: P, shaft: f32, head: f32, long: f32) -> Shape {
    let d = sub(b, a);
    let l = len(d).max(1e-3);
    let u = [d[0] / l, d[1] / l];
    let n = [-u[1], u[0]];
    let at = |o: P, along: f32, across: f32| {
        [
            o[0] + u[0] * along + n[0] * across,
            o[1] + u[1] * along + n[1] * across,
        ]
    };
    Shape::Poly(vec![
        a,
        at(a, long, head / 2.0),
        at(a, long, shaft / 2.0),
        at(b, -long, shaft / 2.0),
        at(b, -long, head / 2.0),
        b,
        at(b, -long, -head / 2.0),
        at(b, -long, -shaft / 2.0),
        at(a, long, -shaft / 2.0),
        at(a, long, -head / 2.0),
    ])
}

/// A double arrow of the usual size.
fn arrows(a: P, b: P) -> Shape {
    double(a, b, 2.2, 7.4, 5.0)
}

/// A small double arrow (a badge's).
fn small_arrows(a: P, b: P) -> Shape {
    double(a, b, 1.8, 5.8, 3.8)
}

/// An arrow's head with its tip at `tip`, pointing along `dir`.
fn head(tip: P, dir: P, wide: f32, long: f32) -> Shape {
    let l = len(dir).max(1e-3);
    let u = [dir[0] / l, dir[1] / l];
    let n = [-u[1], u[0]];
    let base = [tip[0] - u[0] * long, tip[1] - u[1] * long];
    Shape::Poly(vec![
        tip,
        [base[0] + n[0] * wide / 2.0, base[1] + n[1] * wide / 2.0],
        [base[0] - n[0] * wide / 2.0, base[1] - n[1] * wide / 2.0],
    ])
}

/// An event's edge: a bar at `x`.
fn bar(x: f32, y0: f32, y1: f32) -> Shape {
    rect(x - 1.1, y0, x + 1.1, y1)
}

/// An event's body: black, framed in white.
fn block(x0: f32, y0: f32, x1: f32, y1: f32) -> Drawing {
    Drawing::default()
        .w(rect(x0, y0, x1, y1))
        .b(rect(x0 + 1.2, y0 + 1.2, x1 - 1.2, y1 - 1.2))
}

/// The media: a white filmstrip with black holes along its edges.
fn film(x0: f32, y0: f32, x1: f32, y1: f32) -> Drawing {
    let mut d = Drawing::default().w(rect(x0, y0, x1, y1));
    let mut x = x0 + 1.1;
    while x + 1.4 <= x1 - 0.9 {
        d = d
            .b(rect(x, y0 + 0.9, x + 1.4, y0 + 2.3))
            .b(rect(x, y1 - 2.3, x + 1.4, y1 - 0.9));
        x += 2.9;
    }
    d
}

/// A spring: time stretched.
fn spring(x0: f32, x1: f32, y0: f32, y1: f32) -> Drawing {
    let mut d = Drawing::default();
    let n = 5;
    let step = (x1 - x0) / n as f32;
    for i in 0..n {
        let (xa, xb) = (x0 + step * i as f32, x0 + step * (i + 1) as f32);
        let (ya, yb) = if i % 2 == 0 { (y1, y0) } else { (y0, y1) };
        d = d.w(Shape::Line([xa, ya], [xb, yb], 1.7));
    }
    d
}

fn plus(c: P, s: f32) -> Drawing {
    Drawing::default()
        .w(Shape::Line([c[0] - s, c[1]], [c[0] + s, c[1]], 2.0))
        .w(Shape::Line([c[0], c[1] - s], [c[0], c[1] + s], 2.0))
}

fn minus(c: P, s: f32) -> Drawing {
    Drawing::default().w(Shape::Line([c[0] - s, c[1]], [c[0] + s, c[1]], 2.0))
}

fn cross(c: P, s: f32) -> Drawing {
    Drawing::default()
        .w(Shape::Line([c[0] - s, c[1] - s], [c[0] + s, c[1] + s], 2.0))
        .w(Shape::Line([c[0] - s, c[1] + s], [c[0] + s, c[1] - s], 2.0))
}

/// "++": for all of them.
fn plus_plus(c: P) -> Drawing {
    plus([c[0] - 3.4, c[1]], 2.4).with(plus([c[0] + 3.4, c[1]], 2.4))
}

/// The "no" sign: a circle struck through.
fn no_sign(c: P, r: f32) -> Drawing {
    let s = (r - 0.9) * FRAC_1_SQRT_2;
    Drawing::default()
        .w(Shape::Disk(c, r + 1.0))
        .b(Shape::Ring(c, r - 0.6, 1.8))
        .b(Shape::Line([c[0] - s, c[1] + s], [c[0] + s, c[1] - s], 1.8))
}

/// A rectangle's dashed border, white.
fn dashed(x0: f32, y0: f32, x1: f32, y1: f32) -> Drawing {
    let mut d = Drawing::default();
    let mut edge = |a: P, b: P| {
        let l = len(sub(b, a));
        let n = ((l + 1.4) / 3.6).floor().max(1.0) as usize;
        for i in 0..n {
            let t0 = i as f32 / n as f32;
            let t1 = t0 + 2.2 / l.max(1e-3);
            let t1 = t1.min(1.0);
            let p = |t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            d = std::mem::take(&mut d).w(Shape::Line(p(t0), p(t1), 1.6));
        }
    };
    edge([x0, y0], [x1, y0]);
    edge([x1, y0], [x1, y1]);
    edge([x1, y1], [x0, y1]);
    edge([x0, y1], [x0, y0]);
    d
}

/// A white card with a black dashed border: something dragged.
fn card(x0: f32, y0: f32, x1: f32, y1: f32) -> Drawing {
    let mut d = Drawing::default().w(rect(x0, y0, x1, y1));
    let (i0, j0, i1, j1) = (x0 + 1.5, y0 + 1.5, x1 - 1.5, y1 - 1.5);
    let mut dash = |a: P, b: P| {
        let l = len(sub(b, a));
        let n = ((l + 1.0) / 2.6).floor().max(1.0) as usize;
        for i in 0..n {
            let t0 = i as f32 / n as f32;
            let t1 = (t0 + 1.4 / l.max(1e-3)).min(1.0);
            let p = |t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            d = std::mem::take(&mut d).b(Shape::Line(p(t0), p(t1), 1.0));
        }
    };
    dash([i0, j0], [i1, j0]);
    dash([i1, j0], [i1, j1]);
    dash([i1, j1], [i0, j1]);
    dash([i0, j1], [i0, j0]);
    d
}

/// A small upright bar (a column), white with a black core.
fn column(x: f32, y0: f32, y1: f32) -> Drawing {
    Drawing::default()
        .w(rect(x - 1.2, y0, x + 1.2, y1))
        .b(Shape::Line([x, y0 + 1.6], [x, y1 - 1.6], 0.9))
}

/// A small lying bar (a row).
fn row(y: f32, x0: f32, x1: f32) -> Drawing {
    Drawing::default()
        .w(rect(x0, y - 1.2, x1, y + 1.2))
        .b(Shape::Line([x0 + 1.6, y], [x1 - 1.6, y], 0.9))
}

/// A crosshair about `c`, its arms `arm` long past a gap of `gap`.
fn crosshair(c: P, gap: f32, arm: f32) -> Drawing {
    let [x, y] = c;
    Drawing::at(x, y)
        .w(Shape::Line([x, y - gap - arm], [x, y - gap], 1.9))
        .w(Shape::Line([x, y + gap], [x, y + gap + arm], 1.9))
        .w(Shape::Line([x - gap - arm, y], [x - gap, y], 1.9))
        .w(Shape::Line([x + gap, y], [x + gap + arm, y], 1.9))
}

/// A point along the diagonal from `tip` (a pen's, a pencil's): `a` along
/// `u`, `b` across.
fn diag(tip: P, u: P, a: f32, b: f32) -> P {
    let n = [-u[1], u[0]];
    [tip[0] + u[0] * a + n[0] * b, tip[1] + u[1] * a + n[1] * b]
}

/// A pen's nib, its tip at (2.5, 2.5) pointing up and left.
fn nib() -> Drawing {
    let t = [2.5, 2.5];
    let u = [FRAC_1_SQRT_2, FRAC_1_SQRT_2];
    let p = |a, b| diag(t, u, a, b);
    Drawing::at(2.0, 2.0)
        .w(Shape::Poly(vec![
            p(0.0, 0.0),
            p(9.0, 4.6),
            p(15.5, 4.6),
            p(15.5, -4.6),
            p(9.0, -4.6),
        ]))
        .w(Shape::Poly(vec![
            p(16.3, 3.8),
            p(20.3, 3.8),
            p(20.3, -3.8),
            p(16.3, -3.8),
        ]))
        .b(Shape::Line(p(1.6, 0.0), p(8.0, 0.0), 0.9))
        .b(Shape::Disk(p(8.8, 0.0), 1.25))
}

/// A pencil, its point at `tip` toward the lower left, `long` long.
fn pencil_at(tip: P, long: f32) -> Drawing {
    let u = [FRAC_1_SQRT_2, -FRAC_1_SQRT_2];
    let p = |a, b| diag(tip, u, a, b);
    let k = long / 26.5;
    Drawing::default()
        .w(Shape::Poly(vec![
            p(0.0, 0.0),
            p(5.5 * k, 2.9 * k),
            p(24.0 * k, 2.9 * k),
            p(26.5 * k, 0.0),
            p(24.0 * k, -2.9 * k),
            p(5.5 * k, -2.9 * k),
        ]))
        .b(Shape::Poly(vec![
            p(0.0, 0.0),
            p(2.2 * k, 1.15 * k),
            p(2.2 * k, -1.15 * k),
        ]))
        .b(Shape::Line(p(5.5 * k, 2.4 * k), p(5.5 * k, -2.4 * k), 0.8))
        .b(Shape::Line(
            p(20.5 * k, 2.4 * k),
            p(20.5 * k, -2.4 * k),
            0.9,
        ))
}

/// The magnifier, its lens's middle at (13, 13).
fn magnifier() -> Drawing {
    Drawing::at(13.0, 13.0)
        .w(Shape::Disk([13.0, 13.0], 9.6))
        .w(Shape::Line([19.6, 19.6], [28.2, 28.2], 5.0))
        .b(Shape::Ring([13.0, 13.0], 7.6, 1.8))
        .b(Shape::Line([20.4, 20.4], [27.4, 27.4], 2.6))
}

/// A hand pointing up, its fingertip at (11, 1.3).
fn hand() -> Drawing {
    Drawing::at(11.0, 1.0)
        .w(Shape::Line([11.0, 3.6], [11.0, 15.5], 4.6))
        .w(Shape::Line([15.3, 12.6], [15.3, 16.5], 4.3))
        .w(Shape::Line([19.4, 13.4], [19.4, 17.2], 4.1))
        .w(Shape::Line([23.2, 14.8], [23.2, 18.2], 3.8))
        .w(Shape::Poly(vec![
            [8.8, 13.0],
            [25.1, 15.6],
            [25.1, 22.4],
            [22.0, 28.6],
            [11.8, 28.6],
            [8.8, 25.0],
        ]))
        .w(Shape::Line([8.9, 21.8], [4.3, 16.4], 4.3))
        .b(Shape::Line([13.15, 15.0], [13.15, 18.4], 0.9))
        .b(Shape::Line([17.35, 15.5], [17.35, 19.0], 0.9))
        .b(Shape::Line([21.3, 16.3], [21.3, 19.3], 0.9))
}

/// The scrolling's ring, with arrows' heads toward `dirs`.
fn scroll(dirs: &[P]) -> Drawing {
    let c = [16.0, 16.0];
    let mut d = Drawing::at(16.0, 16.0).w(Shape::Ring(c, 4.4, 2.2));
    for &dir in dirs {
        let l = len(dir);
        let u = [dir[0] / l, dir[1] / l];
        d = d.w(head([c[0] + u[0] * 14.0, c[1] + u[1] * 14.0], u, 8.0, 5.4));
    }
    d
}

/// Where the scrolling began: a black dot in white, the ways it can go.
fn origin(dirs: &[P]) -> Drawing {
    let c = [16.0, 16.0];
    let mut d = Drawing::at(16.0, 16.0)
        .w(Shape::Disk(c, 5.2))
        .b(Shape::Disk(c, 3.4));
    for &dir in dirs {
        d = d.w(head(
            [c[0] + dir[0] * 13.0, c[1] + dir[1] * 13.0],
            dir,
            7.0,
            4.4,
        ));
    }
    d
}

const UP: P = [0.0, -1.0];
const DOWN: P = [0.0, 1.0];
const LEFT: P = [-1.0, 0.0];
const RIGHT: P = [1.0, 0.0];

impl Cursor {
    /// Its drawing.
    fn drawing(self) -> Drawing {
        use Cursor::*;
        match self {
            TrimStart => Drawing::at(16.0, 16.0)
                .w(bar(16.0, 5.0, 27.0))
                .w(arrows([3.5, 16.0], [28.5, 16.0]))
                .with(block(18.5, 20.5, 26.0, 26.5)),
            TrimEnd => TrimStart.drawing().mirror(),
            StretchStart => Drawing::at(16.0, 16.0)
                .w(bar(16.0, 5.0, 27.0))
                .w(arrows([3.5, 16.0], [28.5, 16.0]))
                .with(spring(18.5, 28.5, 21.3, 25.7)),
            StretchEnd => StretchStart.drawing().mirror(),
            SlipTrimStart => Drawing::at(16.0, 16.0)
                .w(bar(16.0, 5.0, 27.0))
                .w(arrows([3.5, 16.0], [28.5, 16.0]))
                .with(film(18.5, 20.5, 28.5, 27.0)),
            SlipTrimEnd => SlipTrimStart.drawing().mirror(),
            SlipTrimStartAll => SlipTrimStart.drawing().with(plus_plus([23.5, 6.5])),
            SlipTrimEndAll => SlipTrimStartAll.drawing().mirror(),
            Slip => Drawing::at(16.0, 16.0)
                .w(bar(4.5, 7.0, 25.0))
                .w(bar(27.5, 7.0, 25.0))
                .w(arrows([8.0, 13.5], [24.0, 13.5]))
                .with(film(8.5, 19.0, 23.5, 25.5)),
            SlipAll => Slip.drawing().with(plus_plus([16.0, 5.0])),
            Slide => Drawing::at(16.0, 16.0)
                .with(block(11.0, 9.5, 21.0, 22.5))
                .w(head([2.0, 16.0], LEFT, 8.0, 5.4))
                .w(rect(6.8, 15.0, 11.0, 17.0))
                .w(head([30.0, 16.0], RIGHT, 8.0, 5.4))
                .w(rect(21.0, 15.0, 25.2, 17.0)),
            SlideCrossfade => Drawing::at(16.0, 15.0)
                .w(Shape::Line([6.5, 8.0], [25.5, 22.0], 2.3))
                .w(Shape::Line([6.5, 22.0], [25.5, 8.0], 2.3))
                .w(double([5.0, 27.6], [27.0, 27.6], 1.9, 6.2, 4.2)),
            TrimAdjacent => Drawing::at(16.0, 13.0)
                .w(bar(16.0, 4.0, 28.0))
                .w(arrows([3.5, 13.0], [28.5, 13.0]))
                .with(block(6.5, 19.0, 14.6, 26.0))
                .with(block(17.4, 19.0, 25.5, 26.0)),
            SplitTrim => {
                let mut d = Drawing::at(16.0, 16.0).w(arrows([3.5, 16.0], [28.5, 16.0]));
                for (a, b) in [(3.0, 6.5), (9.0, 11.5), (20.5, 23.0), (25.5, 29.0)] {
                    d = d.w(Shape::Line([16.0, a], [16.0, b], 2.2));
                }
                d
            }
            FadeIn => {
                let mut d = Drawing::at(16.0, 17.0).w(small_arrows([3.0, 6.5], [19.0, 6.5]));
                let curve = |t: f32| {
                    let s = t * t * (3.0 - 2.0 * t);
                    [5.0 + 22.0 * t, 26.0 - 16.0 * s]
                };
                for i in 0..12 {
                    let (a, b) = (i as f32 / 12.0, (i + 1) as f32 / 12.0);
                    d = d.w(Shape::Line(curve(a), curve(b), 2.4));
                }
                d
            }
            FadeOut => FadeIn.drawing().mirror(),
            Scrub => arrow()
                .w(Shape::Poly(vec![
                    [18.0, 22.0],
                    [21.5, 22.0],
                    [26.5, 17.5],
                    [26.5, 31.0],
                    [21.5, 26.5],
                    [18.0, 26.5],
                ]))
                .w(small_arrows([17.5, 11.5], [30.5, 11.5])),
            SelectTool => {
                let mut d = dashed(2.5, 2.5, 13.5, 13.5);
                d.hotspot = [9.0, 9.0];
                d.with(arrow().scaled([1.4, 1.4], 0.92, [7.6, 7.6]))
            }
            EnvelopeTool => {
                let pts = [[12.0, 28.5], [19.5, 21.0], [28.5, 24.5]];
                let mut d = arrow()
                    .w(Shape::Line(pts[0], pts[1], 2.0))
                    .w(Shape::Line(pts[1], pts[2], 2.0));
                for p in pts {
                    d = d
                        .w(rect(p[0] - 2.2, p[1] - 2.2, p[0] + 2.2, p[1] + 2.2))
                        .b(rect(p[0] - 1.1, p[1] - 1.1, p[0] + 1.1, p[1] + 1.1));
                }
                d
            }
            Magnifier => magnifier(),
            ZoomIn => magnifier()
                .b(Shape::Line([9.4, 13.0], [16.6, 13.0], 1.8))
                .b(Shape::Line([13.0, 9.4], [13.0, 16.6], 1.8)),
            ZoomOut => magnifier().b(Shape::Line([9.4, 13.0], [16.6, 13.0], 1.8)),
            MoveFrame => Drawing::at(16.0, 16.0)
                .w(arrows([2.0, 16.0], [30.0, 16.0]))
                .w(arrows([16.0, 2.0], [16.0, 30.0])),
            ResizeEw => Drawing::at(16.0, 16.0).w(double([2.5, 16.0], [29.5, 16.0], 2.6, 8.4, 6.0)),
            ResizeNs => ResizeEw.drawing().turn(),
            ResizeNwse => {
                Drawing::at(16.0, 16.0).w(double([5.5, 5.5], [26.5, 26.5], 2.6, 8.4, 6.0))
            }
            ResizeNesw => ResizeNwse.drawing().mirror(),
            Rotate => {
                let (a0, a1) = (150f32.to_radians(), 400f32.to_radians());
                let (c, r) = ([16.0, 16.0], 10.0);
                let end = [c[0] + r * a1.cos(), c[1] + r * a1.sin()];
                let dir = [-a1.sin(), a1.cos()];
                Drawing::at(16.0, 16.0)
                    .w(Shape::Arc(c, r, 2.6, a0, a1))
                    .w(head(
                        [end[0] + dir[0] * 3.6, end[1] + dir[1] * 3.6],
                        dir,
                        8.0,
                        5.6,
                    ))
                    .w(Shape::Disk(c, 2.6))
                    .b(Shape::Disk(c, 1.3))
            }
            MoveCenter => {
                let c = [16.0, 16.0];
                let mut d = Drawing::at(16.0, 16.0).w(Shape::Ring(c, 4.6, 2.2));
                for dir in [UP, DOWN, LEFT, RIGHT] {
                    let tip = [c[0] + dir[0] * 13.5, c[1] + dir[1] * 13.5];
                    d = d.w(head(tip, dir, 7.0, 5.0)).w(Shape::Line(
                        [c[0] + dir[0] * 7.0, c[1] + dir[1] * 7.0],
                        [c[0] + dir[0] * 9.0, c[1] + dir[1] * 9.0],
                        2.2,
                    ));
                }
                d
            }
            Pen => nib(),
            PenAdd => nib().with(plus([25.5, 25.5], 3.4)),
            PenDelete => nib().with(minus([25.5, 25.5], 3.4)),
            PenSplitTangent => nib()
                .w(Shape::Line([21.5, 28.0], [25.5, 22.5], 1.9))
                .w(Shape::Line([25.5, 22.5], [29.5, 28.0], 1.9)),
            PenTangent => Drawing::at(3.0, 3.0)
                .w(Shape::Line([3.0, 3.0], [11.0, 23.0], 2.0))
                .w(Shape::Line([3.0, 3.0], [23.0, 11.0], 2.0))
                .w(Shape::Disk([11.0, 23.0], 2.6))
                .w(Shape::Disk([23.0, 11.0], 2.6))
                .b(Shape::Disk([11.0, 23.0], 1.2))
                .b(Shape::Disk([23.0, 11.0], 1.2)),
            Crosshair => crosshair([16.0, 16.0], 3.5, 10.0),
            CrosshairRectPlus => crosshair([9.0, 9.0], 2.4, 5.6)
                .with(dashed(14.5, 15.5, 25.5, 24.5))
                .with(plus([27.5, 28.0], 2.6)),
            CrosshairRectNo => crosshair([9.0, 9.0], 2.4, 5.6)
                .with(dashed(14.5, 15.5, 25.5, 24.5))
                .with(no_sign([25.0, 25.0], 4.8)),
            HandPoint => hand(),
            HandPlus => hand().with(plus([27.0, 6.0], 3.2)),
            HandUpDown => hand().w(small_arrows([27.5, 1.6], [27.5, 13.6])),
            HandLeftRight => hand().w(small_arrows([16.6, 5.5], [30.6, 5.5])),
            HandMove => hand()
                .w(small_arrows([18.4, 7.0], [31.0, 7.0]))
                .w(small_arrows([24.7, 0.8], [24.7, 13.2])),
            HandOpen => Drawing::at(15.0, 15.0)
                .w(Shape::Poly(vec![
                    [7.6, 14.4],
                    [24.4, 15.0],
                    [25.0, 22.0],
                    [21.6, 28.6],
                    [11.6, 28.6],
                    [7.6, 23.4],
                ]))
                .w(Shape::Line([8.2, 20.5], [3.4, 14.0], 4.0))
                .w(Shape::Line([10.6, 14.5], [9.0, 4.4], 3.9))
                .w(Shape::Line([14.6, 13.8], [14.6, 2.6], 3.9))
                .w(Shape::Line([18.6, 14.0], [19.9, 4.2], 3.9))
                .w(Shape::Line([22.4, 16.0], [24.8, 8.4], 3.5)),
            HandClosed => Drawing::at(15.0, 17.0)
                .w(Shape::Poly(vec![
                    [6.4, 13.0],
                    [25.2, 13.0],
                    [26.0, 22.0],
                    [22.2, 28.6],
                    [11.0, 28.6],
                    [6.4, 22.0],
                ]))
                .w(Shape::Disk([9.4, 12.6], 2.6))
                .w(Shape::Disk([13.6, 11.6], 2.6))
                .w(Shape::Disk([17.8, 11.6], 2.6))
                .w(Shape::Disk([22.0, 12.4], 2.6))
                .w(Shape::Line([7.0, 18.0], [12.5, 20.5], 3.4))
                .b(Shape::Line([11.5, 12.6], [11.5, 16.4], 0.9))
                .b(Shape::Line([15.7, 12.0], [15.7, 16.4], 0.9))
                .b(Shape::Line([19.9, 12.3], [19.9, 16.4], 0.9)),
            ResizeBarEw => Drawing::at(16.0, 16.0)
                .w(arrows([2.5, 16.0], [29.5, 16.0]))
                .w(rect(14.6, 8.5, 17.4, 23.5))
                .b(Shape::Line([16.0, 10.2], [16.0, 21.8], 0.9)),
            ResizeBarNs => ResizeBarEw.drawing().turn(),
            SplitterEw => Drawing::at(16.0, 16.0)
                .w(rect(12.6, 6.0, 14.8, 26.0))
                .w(rect(17.2, 6.0, 19.4, 26.0))
                .w(head([2.0, 16.0], LEFT, 7.6, 5.2))
                .w(rect(6.8, 15.0, 12.6, 17.0))
                .w(head([30.0, 16.0], RIGHT, 7.6, 5.2))
                .w(rect(19.4, 15.0, 25.2, 17.0)),
            SplitterNs => SplitterEw.drawing().turn(),
            Arrow => arrow(),
            ArrowMirrored => arrow().mirror(),
            ArrowUpDown => arrow().w(small_arrows([25.0, 13.5], [25.0, 30.5])),
            ArrowLeftRight => arrow().w(small_arrows([17.0, 7.0], [30.5, 7.0])),
            Drag => arrow().with(card(14.5, 18.5, 29.5, 29.5)),
            DragCopy => arrow()
                .with(card(14.0, 17.5, 25.5, 26.5))
                .with(plus([28.0, 28.4], 2.6)),
            DragCopyMany => arrow()
                .with(card(13.0, 16.5, 23.5, 25.0))
                .with(plus_plus([26.5, 29.0])),
            ArrowNo => arrow().with(no_sign([23.5, 23.5], 5.6)),
            ArrowPlus => arrow().with(plus([24.5, 6.5], 3.4)),
            ArrowMinus => arrow().with(minus([24.5, 6.5], 3.4)),
            ArrowCross => arrow().with(cross([24.5, 6.5], 3.0)),
            DragC | DragP | DragM => {
                let base = arrow().mirror().with(card(2.5, 18.0, 16.0, 29.5));
                match self {
                    DragC => base.b(Shape::Arc(
                        [9.2, 23.8],
                        2.7,
                        1.3,
                        40f32.to_radians(),
                        320f32.to_radians(),
                    )),
                    DragP => base
                        .b(Shape::Line([7.6, 27.0], [7.6, 20.6], 1.3))
                        .b(Shape::Arc([8.9, 22.4], 1.75, 1.3, -FRAC_PI_2, FRAC_PI_2))
                        .b(Shape::Line([7.6, 20.65], [8.9, 20.65], 1.3))
                        .b(Shape::Line([7.6, 24.15], [8.9, 24.15], 1.3)),
                    _ => base
                        .b(Shape::Line([6.0, 27.0], [6.0, 20.6], 1.2))
                        .b(Shape::Line([6.0, 20.6], [9.2, 24.6], 1.2))
                        .b(Shape::Line([9.2, 24.6], [12.4, 20.6], 1.2))
                        .b(Shape::Line([12.4, 20.6], [12.4, 27.0], 1.2)),
                }
            }
            ArrowL => arrow()
                .w(Shape::Line([20.5, 19.5], [20.5, 29.0], 2.2))
                .w(Shape::Line([20.5, 29.0], [27.5, 29.0], 2.2)),
            ArrowR => arrow()
                .w(Shape::Line([20.5, 29.5], [20.5, 19.0], 2.2))
                .w(Shape::Arc([22.6, 21.7], 2.7, 2.2, -FRAC_PI_2, FRAC_PI_2))
                .w(Shape::Line([20.5, 19.0], [22.6, 19.0], 2.2))
                .w(Shape::Line([20.5, 24.4], [22.6, 24.4], 2.2))
                .w(Shape::Line([22.6, 24.4], [27.0, 29.5], 2.2)),
            ColumnMove => arrow()
                .w(small_arrows([17.5, 5.0], [30.5, 5.0]))
                .with(column(24.0, 17.0, 30.0)),
            ColumnsMove => arrow()
                .w(small_arrows([17.5, 5.0], [30.5, 5.0]))
                .with(column(19.5, 17.0, 30.0))
                .with(column(23.5, 17.0, 30.0))
                .with(column(27.5, 17.0, 30.0)),
            RowMove => arrow()
                .w(small_arrows([26.0, 1.5], [26.0, 13.5]))
                .with(row(24.0, 16.5, 30.0)),
            RowsMove => arrow()
                .w(small_arrows([26.0, 1.5], [26.0, 13.5]))
                .with(row(19.5, 16.5, 30.0))
                .with(row(23.5, 16.5, 30.0))
                .with(row(27.5, 16.5, 30.0)),
            ColumnInsert => arrow()
                .with(plus([25.5, 6.0], 3.2))
                .with(column(24.0, 17.0, 30.0)),
            ColumnsInsert => arrow()
                .with(plus([25.5, 6.0], 3.2))
                .with(column(19.5, 17.0, 30.0))
                .with(column(23.5, 17.0, 30.0))
                .with(column(27.5, 17.0, 30.0)),
            RowInsert => arrow()
                .with(plus([25.5, 6.0], 3.2))
                .with(row(24.0, 16.5, 30.0)),
            RowsInsert => arrow()
                .with(plus([25.5, 6.0], 3.2))
                .with(row(19.5, 16.5, 30.0))
                .with(row(23.5, 16.5, 30.0))
                .with(row(27.5, 16.5, 30.0)),
            ColumnDelete => arrow()
                .with(cross([25.5, 6.0], 2.9))
                .with(column(24.0, 17.0, 30.0)),
            Pencil => {
                let mut d = pencil_at([3.0, 29.0], 26.5);
                d.hotspot = [3.0, 29.0];
                d
            }
            PencilCrosshair => crosshair([7.5, 24.5], 2.2, 4.6).with(pencil_at([11.0, 21.0], 20.0)),
            Dropper => {
                let tip = [3.0, 29.0];
                let u = [FRAC_1_SQRT_2, -FRAC_1_SQRT_2];
                let p = |a, b| diag(tip, u, a, b);
                Drawing::at(3.0, 29.0)
                    .w(Shape::Poly(vec![
                        p(0.0, 0.0),
                        p(3.0, 1.7),
                        p(14.5, 1.7),
                        p(14.5, -1.7),
                        p(3.0, -1.7),
                    ]))
                    .w(Shape::Poly(vec![
                        p(14.0, 3.6),
                        p(17.4, 3.6),
                        p(17.4, -3.6),
                        p(14.0, -3.6),
                    ]))
                    .w(Shape::Disk(p(21.4, 0.0), 4.4))
                    .b(Shape::Line(p(2.4, 0.0), p(12.6, 0.0), 1.0))
            }
            ScrollUp => scroll(&[UP]),
            ScrollRight => scroll(&[RIGHT]),
            ScrollDown => scroll(&[DOWN]),
            ScrollLeft => scroll(&[LEFT]),
            ScrollUpRight => scroll(&[[1.0, -1.0]]),
            ScrollDownRight => scroll(&[[1.0, 1.0]]),
            ScrollUpLeft => scroll(&[[-1.0, -1.0]]),
            ScrollDownLeft => scroll(&[[-1.0, 1.0]]),
            ScrollStopNs => scroll(&[UP, DOWN]),
            ScrollStopEw => scroll(&[LEFT, RIGHT]),
            ScrollStopAll => scroll(&[UP, DOWN, LEFT, RIGHT]),
            OriginNs => origin(&[UP, DOWN]),
            OriginEw => origin(&[LEFT, RIGHT]),
            OriginAll => origin(&[UP, DOWN, LEFT, RIGHT]),
        }
    }
}

/// `d` as `size`×`size` pixels, straight RGBA: its outline black, its body
/// white over it, its details black on top, each edge smoothed over a
/// pixel.
fn render(d: &Drawing, size: u16) -> Vec<u8> {
    let n = usize::from(size);
    let k = f32::from(size) / UNITS;
    let nearest = |shapes: &[Shape], p: P| {
        shapes
            .iter()
            .map(|s| s.distance(p))
            .fold(f32::INFINITY, f32::min)
    };
    let cover = |dist: f32| (0.5 - dist * k).clamp(0.0, 1.0);
    let mut rgba = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let p = [(x as f32 + 0.5) / k, (y as f32 + 0.5) / k];
            let dw = nearest(&d.white, p);
            let db = nearest(&d.black, p);
            let (line, body, detail) = (cover(dw - OUTLINE), cover(dw), cover(db));
            // Premultiplied: the outline, the body over it, the details.
            let mut a = line;
            let mut v = 0.0;
            v = body + v * (1.0 - body);
            a = body + a * (1.0 - body);
            v *= 1.0 - detail;
            a = detail + a * (1.0 - detail);
            let shade = if a > 0.0 {
                (v / a).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let at = (y * n + x) * 4;
            let level = (shade * 255.0).round() as u8;
            rgba[at] = level;
            rgba[at + 1] = level;
            rgba[at + 2] = level;
            rgba[at + 3] = (a * 255.0).round() as u8;
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How much of a picture is drawn (its pixels' alpha, summed).
    fn ink(img: &egui::CustomCursorImage) -> f32 {
        img.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| f32::from(p[3]) / 255.0)
            .sum()
    }

    #[test]
    fn every_pointer_is_drawn_with_its_hot_spot_inside() {
        for c in Cursor::ALL {
            for size in [32u16, 48, 64] {
                let img = image(c, size);
                assert_eq!(img.size, [size, size]);
                assert_eq!(img.rgba.len(), usize::from(size) * usize::from(size) * 4);
                assert!(img.hotspot[0] < size && img.hotspot[1] < size, "{c:?}");
                // Something to see, and nothing filling the square.
                let drawn = ink(&img) / f32::from(size).powi(2);
                assert!(drawn > 0.04 && drawn < 0.6, "{c:?} at {size}: {drawn}");
            }
        }
    }

    #[test]
    fn a_larger_screen_draws_them_again_larger() {
        assert_eq!(size_for(1.0), 32);
        assert_eq!(size_for(1.5), 48);
        assert_eq!(size_for(2.0), 64);
        assert_eq!(size_for(f32::NAN), 32);
        // Twice the pixels each way: four times the ink, give or take the
        // edges' smoothing.
        for c in [Cursor::TrimStart, Cursor::HandPoint, Cursor::Rotate] {
            let (a, b) = (ink(&image(c, 32)), ink(&image(c, 64)));
            assert!((b / a - 4.0).abs() < 0.35, "{c:?}: {a} {b}");
        }
        assert_eq!(image(Cursor::TrimStart, 64).hotspot, [32, 32]);
    }

    #[test]
    fn the_same_picture_comes_back() {
        let a = image(Cursor::Slip, 32);
        let b = image(Cursor::Slip, 32);
        assert!(Arc::ptr_eq(&a.rgba, &b.rgba));
    }

    #[test]
    fn mirrored_pairs_are_each_others_mirror() {
        for (a, b) in [
            (Cursor::TrimStart, Cursor::TrimEnd),
            (Cursor::StretchStart, Cursor::StretchEnd),
            (Cursor::SlipTrimStart, Cursor::SlipTrimEnd),
            (Cursor::FadeIn, Cursor::FadeOut),
        ] {
            let (x, y) = (image(a, 32), image(b, 32));
            let n = 32;
            let mut off = 0;
            for row in 0..n {
                for col in 0..n {
                    let p = (row * n + col) * 4 + 3;
                    let q = (row * n + (n - 1 - col)) * 4 + 3;
                    off += usize::from(x.rgba[p].abs_diff(y.rgba[q]) > 8);
                }
            }
            // The grid's middle is 16, a pixel's edge: the mirror lands
            // a pixel over, so allow its smoothing.
            assert!(off < 120, "{a:?} and {b:?}: {off}");
        }
    }

    #[test]
    fn the_outline_is_black_and_the_body_white() {
        let img = image(Cursor::Arrow, 32);
        let px = |x: usize, y: usize| {
            let at = (y * 32 + x) * 4;
            [img.rgba[at], img.rgba[at + 3]]
        };
        // Inside the arrow: white, solid.
        assert_eq!(px(4, 12), [255, 255]);
        // Its left edge: black.
        assert!(px(0, 12)[0] < 40 && px(0, 12)[1] > 100);
        // Far from it: nothing.
        assert_eq!(px(28, 4)[1], 0);
    }

    /// Every pointer at 32 and 64 pixels over light and dark gray, with its
    /// hot spot marked, for looking at them:
    /// `target/telas/ponteiros.png`.
    #[test]
    #[ignore = "renders images for visual review"]
    fn cursors_for_review() {
        let cols = 10usize;
        let cell = 112usize;
        let rows = Cursor::ALL.len().div_ceil(cols);
        let (w, h) = (cols * cell, rows * cell);
        let mut out = vec![0u8; w * h * 4];
        for (i, c) in Cursor::ALL.iter().enumerate() {
            let (cx, cy) = ((i % cols) * cell, (i / cols) * cell);
            for y in 0..cell {
                for x in 0..cell {
                    let light = x < cell / 2;
                    let g = if light { 0xD8 } else { 0x30 };
                    let at = ((cy + y) * w + cx + x) * 4;
                    out[at..at + 4].copy_from_slice(&[g, g, g, 255]);
                }
            }
            for (size, ox, oy) in [(32u16, 12usize, 8usize), (64, 40, 40)] {
                let img = image(*c, size);
                let n = usize::from(size);
                for y in 0..n {
                    for x in 0..n {
                        let s = &img.rgba[(y * n + x) * 4..(y * n + x) * 4 + 4];
                        let (px, py) = (cx + ox + x, cy + oy + y);
                        if px >= w || py >= h {
                            continue;
                        }
                        let at = (py * w + px) * 4;
                        let a = f32::from(s[3]) / 255.0;
                        for ch in 0..3 {
                            let under = f32::from(out[at + ch]);
                            out[at + ch] = (f32::from(s[ch]) * a + under * (1.0 - a)) as u8;
                        }
                    }
                }
                let (hx, hy) = (
                    cx + ox + usize::from(img.hotspot[0]),
                    cy + oy + usize::from(img.hotspot[1]),
                );
                if hx < w && hy < h {
                    let at = (hy * w + hx) * 4;
                    out[at..at + 4].copy_from_slice(&[255, 40, 40, 255]);
                }
            }
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
        std::fs::create_dir_all(dir.join("ponteiros")).unwrap();
        let png = bv_png::encode_rgba8(w as u32, h as u32, &out, 6);
        std::fs::write(dir.join("ponteiros.png"), png).unwrap();
        // Each one alone at 64 pixels, for tables of them.
        for c in Cursor::ALL {
            let img = image(c, 64);
            let png = bv_png::encode_rgba8(64, 64, &img.rgba, 6);
            std::fs::write(dir.join("ponteiros").join(format!("{c:?}.png")), png).unwrap();
        }
    }
}
