//! A marker's and a region's flag as VEGAS 23 draws them (measured on the
//! user's prints): an orange pennant — a region's green — edged dark, its
//! number dark in its middle, its lower left corner running down into its
//! pole; a region's end flag the same turned toward the left. The Trimmer's
//! loop bar and the timeline's marker bar draw them.

use crate::egui::{self, Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, pos2};

/// The orange of a marker and the green of a region, their dark edges and
/// numbers.
pub(crate) const MARKER: Color32 = Color32::from_rgb(0xEF, 0x8E, 0x2B);
pub(crate) const REGION: Color32 = Color32::from_rgb(0x34, 0xB4, 0x55);
const MARKER_EDGE: Color32 = Color32::from_rgb(0x2D, 0x0D, 0x00);
const REGION_EDGE: Color32 = Color32::from_rgb(0x08, 0x2A, 0x10);
const MARKER_INK: Color32 = Color32::from_rgb(0x5A, 0x2A, 0x00);
const REGION_INK: Color32 = Color32::from_rgb(0x06, 0x30, 0x12);
/// The pole's light middle and dark sides.
const MARKER_POLE: Color32 = Color32::from_rgb(0xB5, 0x99, 0x75);
const REGION_POLE: Color32 = Color32::from_rgb(0x8C, 0xC8, 0x9A);
/// The timeline's flags: their edges, numbers and their lines' sides.
const DARK: Color32 = Color32::from_rgb(0x14, 0x14, 0x14);
/// A picked flag's grey and its number.
const PICKED: Color32 = Color32::from_rgb(0x78, 0x78, 0x78);
const PICKED_INK: Color32 = Color32::from_rgb(0xF0, 0xF0, 0xF0);

/// Which flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Marker,
    /// A region's start, or its end (turned toward the left).
    RegionStart,
    RegionEnd,
}

impl Kind {
    fn colors(self) -> (Color32, Color32, Color32, Color32) {
        match self {
            Kind::Marker => (MARKER, MARKER_EDGE, MARKER_INK, MARKER_POLE),
            Kind::RegionStart | Kind::RegionEnd => (REGION, REGION_EDGE, REGION_INK, REGION_POLE),
        }
    }
}

/// A flag's size: VEGAS's are 19 points tall with their tails, in the
/// Trimmer and on the timeline.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Size {
    /// From its top to its tail's tip.
    pub height: f32,
    pub font: f32,
    /// Its rectangle's least width.
    pub width: f32,
    /// Its edge and number the timeline's dark grey, and its line down
    /// its own color between two dark ones.
    pub dark: bool,
}

pub(crate) const TRIMMER: Size = Size {
    height: 19.0,
    font: 10.0,
    width: 17.0,
    dark: false,
};
/// The timeline's, measured on the user's print: 19 points wide, its
/// rectangle 13 tall, its tail 6 more (the loop region's bar's height,
/// where it ends).
pub(crate) const TIMELINE: Size = Size {
    height: 19.0,
    font: 10.0,
    width: 19.0,
    dark: true,
};

impl Size {
    /// The rectangle's height (the tail takes the rest).
    pub(crate) fn body(self) -> f32 {
        (self.height * 0.68).round()
    }
}

/// The flag of `number` with its pole at `x`, its top at `top`: its
/// rectangle (where its name goes after it, and where a click finds it).
/// `picked` (the Marker Tool's), as VEGAS's: grey within a ring of its
/// color, its number white.
pub(crate) fn paint(
    painter: &egui::Painter,
    x: f32,
    top: f32,
    number: &str,
    kind: Kind,
    size: Size,
    picked: bool,
) -> Rect {
    let (fill, edge, ink, _) = kind.colors();
    let (edge, ink) = if size.dark { (DARK, DARK) } else { (edge, ink) };
    let ink = if picked { PICKED_INK } else { ink };
    let font = FontId::proportional(size.font);
    let text = painter.layout_no_wrap(number.to_string(), font, ink);
    let x = x.round() + 0.5;
    let body_h = size.body();
    let width = (text.size().x + 9.0).max(size.width).round();
    let tail = (body_h * 0.55).round();
    let way = if kind == Kind::RegionEnd { -1.0 } else { 1.0 };
    let at = |dx: f32, dy: f32| pos2(x + dx * way, top + dy);
    // The rectangle, then the tail down to the pole.
    let body = Rect::from_two_pos(at(0.0, 0.0), at(width, body_h));
    painter.rect_filled(body, 0.0, fill);
    painter.add(Shape::convex_polygon(
        ordered(vec![
            at(0.0, body_h - 0.5),
            at(tail, body_h - 0.5),
            at(0.0, size.height),
        ]),
        fill,
        Stroke::NONE,
    ));
    let outline = vec![
        at(0.0, 0.0),
        at(width, 0.0),
        at(width, body_h),
        at(tail, body_h),
        at(0.0, size.height),
    ];
    painter.add(Shape::closed_line(outline, Stroke::new(1.0, edge)));
    if picked {
        // Two points in: a dark edge around the grey, the flag's color
        // left as a ring between it and the outer edge.
        let inner = Rect::from_two_pos(at(2.0, 2.0), at(width - 2.0, body_h - 2.0));
        painter.rect_filled(inner, 0.0, PICKED);
        painter.add(Shape::convex_polygon(
            ordered(vec![
                at(2.0, body_h - 2.5),
                at(tail, body_h - 2.5),
                at(2.0, size.height - 4.0),
            ]),
            PICKED,
            Stroke::NONE,
        ));
        painter.add(Shape::closed_line(
            vec![
                at(2.0, 2.0),
                at(width - 2.0, 2.0),
                at(width - 2.0, body_h - 2.0),
                at(tail, body_h - 2.0),
                at(2.0, size.height - 4.0),
            ],
            Stroke::new(1.0, edge),
        ));
    }
    painter.galley(
        pos2(
            body.center().x - text.size().x / 2.0,
            body.center().y - text.size().y / 2.0,
        ),
        text,
        ink,
    );
    body
}

/// A convex polygon's points in egui's turning order (a flag turned left
/// lists them the other way round).
fn ordered(mut points: Vec<Pos2>) -> Vec<Pos2> {
    let area: f32 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum();
    if area < 0.0 {
        points.reverse();
    }
    points
}

/// A flag's pole from `top` down to `bottom` at `x`: a light line between
/// two dark ones, as VEGAS's Trimmer draws it; the timeline's (`size`
/// dark), the flag's own color between two dark lines, down the bars and
/// the tracks.
pub(crate) fn paint_pole(
    painter: &egui::Painter,
    x: f32,
    top: f32,
    bottom: f32,
    kind: Kind,
    size: Size,
) {
    if bottom <= top {
        return;
    }
    let (fill, edge, _, pole) = kind.colors();
    let (edge, pole) = if size.dark {
        (DARK, fill)
    } else {
        (edge, pole)
    };
    let x = x.round() + 0.5;
    for (dx, color) in [(-1.0, edge), (0.0, pole), (1.0, edge)] {
        painter.line_segment(
            [pos2(x + dx, top), pos2(x + dx, bottom)],
            Stroke::new(1.0, color),
        );
    }
}

/// A name after a flag, in the interface's light text.
pub(crate) fn paint_name(painter: &egui::Painter, flag: Rect, name: &str, size: Size, kind: Kind) {
    if name.is_empty() {
        return;
    }
    let (pos, align) = if kind == Kind::RegionEnd {
        (
            pos2(flag.left() - 3.0, flag.center().y),
            Align2::RIGHT_CENTER,
        )
    } else {
        (
            pos2(flag.right() + 3.0, flag.center().y),
            Align2::LEFT_CENTER,
        )
    };
    painter.text(
        pos,
        align,
        name,
        FontId::proportional(size.font),
        crate::theme::text(),
    );
}
