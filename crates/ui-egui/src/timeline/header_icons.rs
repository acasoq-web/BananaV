//! The track headers' button icons, after VEGAS 23's as the user's prints
//! show them, in BananaV's own shapes: Maximize's arrows leaving a bar and
//! Minimize's closing on it, Bypass Motion Blur's square and its trail
//! crossed out, Color Grading's three colors, Track Motion's square going
//! onto a smaller one, and the compositing buttons' layers. Each is drawn
//! in a square `rect` (about 14 pixels) in `color`, with the colors of its
//! own where VEGAS's have them (dimmed with `color` when it is greyed).

use crate::egui::{self, Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, pos2, vec2};
use crate::theme;

/// The colors of the icons' marks, toned down by `color` when greyed.
const RED: Color32 = Color32::from_rgb(0xF0, 0x6A, 0x4E);
const BLUE: Color32 = Color32::from_rgb(0x5C, 0x7C, 0xF2);
const GREEN: Color32 = Color32::from_rgb(0x2C, 0xD4, 0x74);

/// `mark` as bright as `color` is against the normal text.
fn toned(mark: Color32, color: Color32) -> Color32 {
    if color == theme::text_disabled() {
        mark.lerp_to_gamma(color, 0.7)
    } else {
        mark
    }
}

fn triangle(painter: &egui::Painter, points: [Pos2; 3], color: Color32) {
    painter.add(Shape::convex_polygon(points.to_vec(), color, Stroke::NONE));
}

/// Maximize: arrows leaving a thick bar, up and down.
pub(crate) fn maximize(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    triangle(
        painter,
        [
            pos2(c.x, c.y - 6.5),
            pos2(c.x + 4.0, c.y - 3.0),
            pos2(c.x - 4.0, c.y - 3.0),
        ],
        color,
    );
    painter.rect_filled(Rect::from_center_size(c, vec2(12.0, 3.5)), 0.0, color);
    triangle(
        painter,
        [
            pos2(c.x, c.y + 6.5),
            pos2(c.x - 4.0, c.y + 3.0),
            pos2(c.x + 4.0, c.y + 3.0),
        ],
        color,
    );
}

/// Minimize: arrows closing on a thin bar.
pub(crate) fn minimize(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    triangle(
        painter,
        [
            pos2(c.x - 4.0, c.y - 6.5),
            pos2(c.x + 4.0, c.y - 6.5),
            pos2(c.x, c.y - 2.5),
        ],
        color,
    );
    painter.rect_filled(Rect::from_center_size(c, vec2(12.0, 1.5)), 0.0, color);
    triangle(
        painter,
        [
            pos2(c.x + 4.0, c.y + 6.5),
            pos2(c.x - 4.0, c.y + 6.5),
            pos2(c.x, c.y + 2.5),
        ],
        color,
    );
}

/// Bypass Motion Blur: a square with its trail behind it, crossed out.
pub(crate) fn bypass_motion_blur(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let red = toned(RED, color);
    painter.line_segment(
        [pos2(c.x - 1.0, c.y - 5.75), pos2(c.x + 5.75, c.y - 5.75)],
        Stroke::new(1.5, red),
    );
    painter.line_segment(
        [pos2(c.x + 5.75, c.y - 5.75), pos2(c.x + 5.75, c.y + 1.0)],
        Stroke::new(1.5, red),
    );
    painter.rect_filled(
        Rect::from_min_max(pos2(c.x - 6.5, c.y - 2.0), pos2(c.x + 2.5, c.y + 6.5)),
        0.0,
        color,
    );
    painter.line_segment(
        [pos2(c.x - 5.5, c.y - 6.5), pos2(c.x + 6.0, c.y + 6.5)],
        Stroke::new(1.5, red),
    );
}

/// Color Grading: three colors, red over blue and green.
pub(crate) fn color_grading(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    painter.circle_filled(pos2(c.x, c.y - 3.0), 3.2, toned(RED, color));
    painter.circle_filled(pos2(c.x - 3.6, c.y + 3.2), 3.2, toned(BLUE, color));
    painter.circle_filled(pos2(c.x + 3.6, c.y + 3.2), 3.2, toned(GREEN, color));
}

/// Track Motion: a square going down onto a smaller one.
pub(crate) fn track_motion(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let square = Rect::from_min_max(pos2(c.x - 6.5, c.y - 6.5), pos2(c.x + 1.5, c.y + 1.5));
    painter.rect_filled(square, 0.0, toned(BLUE, color));
    let dark = Color32::from_gray(0x24);
    let (a, b) = (pos2(c.x - 4.5, c.y - 4.5), pos2(c.x - 0.5, c.y - 0.5));
    painter.line_segment([a, b], Stroke::new(1.2, dark));
    painter.line_segment([b, pos2(b.x - 3.0, b.y)], Stroke::new(1.2, dark));
    painter.line_segment([b, pos2(b.x, b.y - 3.0)], Stroke::new(1.2, dark));
    painter.rect_filled(
        Rect::from_min_max(pos2(c.x + 2.0, c.y + 2.0), pos2(c.x + 6.5, c.y + 6.5)),
        0.0,
        color,
    );
}

/// A layer seen from above: a flat square turned on its corner, its
/// middle `at`.
fn layer(at: Pos2) -> Vec<Pos2> {
    vec![
        pos2(at.x, at.y - 3.5),
        pos2(at.x + 5.5, at.y),
        pos2(at.x, at.y + 3.5),
        pos2(at.x - 5.5, at.y),
    ]
}

/// The edge of the layer under one at `at`: its lower half.
fn layer_under(painter: &egui::Painter, at: Pos2, stroke: Stroke) {
    painter.add(Shape::line(
        vec![
            pos2(at.x - 5.5, at.y + 3.0),
            pos2(at.x, at.y + 6.5),
            pos2(at.x + 5.5, at.y + 3.0),
        ],
        stroke,
    ));
}

/// Compositing Mode: two layers and the mode's mark (α: the picture over
/// what is below by its alpha, as the default Source Alpha).
pub(crate) fn compositing_mode(painter: &egui::Painter, rect: Rect, color: Color32, mark: &str) {
    let at = pos2(rect.center().x - 1.5, rect.center().y - 2.5);
    painter.add(Shape::convex_polygon(layer(at), color, Stroke::NONE));
    layer_under(painter, at, Stroke::new(1.3, color));
    painter.text(
        pos2(rect.right() + 1.5, rect.bottom() + 1.0),
        Align2::RIGHT_BOTTOM,
        mark,
        FontId::proportional(9.5),
        color,
    );
}

/// Make Compositing Parent: two layers, drawn as outlines.
pub(crate) fn make_parent(painter: &egui::Painter, rect: Rect, color: Color32) {
    let at = pos2(rect.center().x, rect.center().y - 2.0);
    let stroke = Stroke::new(1.3, color);
    painter.add(Shape::closed_line(layer(at), stroke));
    layer_under(painter, at, stroke);
}

/// Make Compositing Child: two layers, the upper one filled.
pub(crate) fn make_child(painter: &egui::Painter, rect: Rect, color: Color32) {
    let at = pos2(rect.center().x, rect.center().y - 2.0);
    painter.add(Shape::convex_polygon(layer(at), color, Stroke::NONE));
    layer_under(painter, at, Stroke::new(1.3, color));
}

/// A menu's arrow under a button's icon: a small triangle pointing down.
pub(crate) fn menu_arrow(painter: &egui::Painter, at: Pos2, color: Color32) {
    triangle(
        painter,
        [
            pos2(at.x - 3.5, at.y - 1.5),
            pos2(at.x + 3.5, at.y - 1.5),
            pos2(at.x, at.y + 2.5),
        ],
        color,
    );
}
