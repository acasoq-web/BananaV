//! BananaV's logo, drawn as vector shapes in its own colors: the transparent
//! version of the official logo (`packaging/icons/bananav-logo-transparent.svg`,
//! whose paths these are). It is sharp at any size, sits on any background
//! and is the one drawing of the interface that keeps its colors whatever the
//! theme says.

use std::sync::OnceLock;

use crate::egui::{Color32, Mesh, Painter, Pos2, Rect, Shape, Vec2, pos2, vec2};

use super::path::{self, Sub};

/// The part of the SVG's square (600 units) that is drawn into the
/// rectangle: the logo covers 100 to 500 of it, and a unit more on each side
/// leaves room for its soft edges.
const FROM: f32 = 99.0;
const SPAN: f32 = 402.0;
/// Under this many pixels across, nothing is drawn.
const LEAST_PIXELS: f32 = 4.0;
/// How far the corner of a soft edge may reach where the outline turns back
/// sharply, as a share of the bend (a bend of 110° would reach 2.4 times the
/// edge's width): the edge stays near the point instead of shooting out of it.
const LEAST_BEND: f32 = 0.35;

const YELLOW: Color32 = Color32::from_rgb(0xFD, 0xD8, 0x25);
const AMBER: Color32 = Color32::from_rgb(0xDA, 0x9E, 0x08);
const BROWN: Color32 = Color32::from_rgb(0x6B, 0x3E, 0x12);

/// The shapes, back to front as the SVG paints them: the bar of the V, the
/// banana, its shade, its tip and the end of its stem.
const SHAPES: [(&str, Color32); 5] = [
    (
        "M500 100H400L293.7 356.63A285 285 0 0 0 370.08 413.66Z",
        YELLOW,
    ),
    (
        "M216.17 124.93C204 134 194 152 195 172C195.3 182 200.9 190.2 203.86 208A300 300 0 0 0 369.23 430C386 441 403 451 411 462L402 477C347 467 320.62 455.53 260 420.53A220 220 0 0 1 150 230C150 185 172 150 195 119Z",
        YELLOW,
    ),
    (
        "M195 119C172 150 155 185 155 230A215 215 0 0 0 262.5 416.2C323.12 451.2 346 463 401 473L404 491C399 494 394 497.5 388 499.4A270 270 0 0 1 100.19 240C100.19 215 108 190 130 170C150 152 168 142 176 122C178.5 114 186.83 105.07 185.83 102.07Z",
        AMBER,
    ),
    (
        "M411 462C417 470 416 484 404 491C395 483 395 468 411 462Z",
        BROWN,
    ),
    // The end of the stem: an ellipse of 19 × 9.5 turned 37° about
    // (201, 113.5), as two half turns between the ends of its long axis.
    (
        "M185.826 102.065A19 9.5 37 1 1 216.174 124.935A19 9.5 37 1 1 185.826 102.065Z",
        BROWN,
    ),
];

/// The shapes' paths, read the first time the logo is drawn.
fn shapes() -> &'static [(Vec<Sub>, Color32)] {
    static READ: OnceLock<Vec<(Vec<Sub>, Color32)>> = OnceLock::new();
    READ.get_or_init(|| {
        SHAPES
            .iter()
            .map(|&(data, color)| (path::parse(data), color))
            .collect()
    })
}

/// Draws the logo inside `rect` (the square that fits it, centered), in its
/// own colors and on no background.
pub fn paint(painter: &Painter, rect: Rect) {
    let s = rect.width().min(rect.height());
    let ppp = painter.ctx().pixels_per_point();
    if !(s * ppp).is_finite() || s * ppp < LEAST_PIXELS {
        return;
    }
    let unit = s / SPAN;
    // The square's corner on a whole pixel, as the icons' is.
    let snap = |v: f32| (v * ppp).round() / ppp;
    let corner = rect.center() - vec2(s, s) / 2.0;
    let corner = pos2(snap(corner.x), snap(corner.y));
    let to_px = |p: Pos2| corner + (p - pos2(FROM, FROM)) * unit;
    for (subs, color) in shapes() {
        for sub in subs {
            let (points, _) = super::tidy(sub.flatten(&to_px), true, ppp, false);
            smooth_fill(painter, &points, *color);
        }
    }
}

/// Fills a polygon (concave ones too) so that its outline is where the color
/// is half there: whole half a pixel inside it and gone half a pixel outside,
/// which is what the coverage of a rasterizer gives it.
fn smooth_fill(painter: &Painter, points: &[Pos2], color: Color32) {
    let n = points.len();
    if n < 3 {
        return;
    }
    let winding: f32 = (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    // The normal of the side from point `i` to the next, pointing out.
    let out = |i: usize| -> Vec2 {
        let along = (points[(i + 1) % n] - points[i]).normalized();
        let normal = vec2(along.y, -along.x);
        if winding >= 0.0 { normal } else { -normal }
    };
    // Half a pixel: the soft edge is a pixel wide, centered on the outline.
    let half = 0.5 / painter.ctx().pixels_per_point();
    let bends: Vec<Vec2> = (0..n)
        .map(|i| {
            let (before, after) = (out((i + n - 1) % n), out(i));
            // The corner of the edge, as far from both sides as the edge is
            // wide.
            (before + after) / (1.0 + before.dot(after)).max(LEAST_BEND)
        })
        .collect();
    let mut mesh = Mesh::default();
    for (&p, &bend) in points.iter().zip(&bends) {
        mesh.colored_vertex(p - bend * half, color);
    }
    for (&p, &bend) in points.iter().zip(&bends) {
        mesh.colored_vertex(p + bend * half, Color32::TRANSPARENT);
    }
    // The inside is the polygon's own triangles, moved in with its points.
    mesh.indices = path::triangulate(points);
    for i in 0..n {
        let j = (i + 1) % n;
        let (a, b, c, d) = (i as u32, j as u32, (n + i) as u32, (n + j) as u32);
        mesh.add_triangle(a, b, d);
        mesh.add_triangle(a, d, c);
    }
    painter.add(Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logo_is_five_closed_shapes_inside_its_box() {
        let same = |p: Pos2| p;
        assert_eq!(shapes().len(), SHAPES.len());
        for (subs, _) in shapes() {
            assert_eq!(subs.len(), 1);
            assert!(subs[0].closed);
            for p in subs[0].flatten(&same) {
                let inside = FROM..=FROM + SPAN;
                assert!(
                    inside.contains(&p.x) && inside.contains(&p.y),
                    "a point outside the logo's box: {p:?}"
                );
            }
        }
    }

    /// The end of the stem is the SVG's ellipse (19 × 9.5, turned 37°): its
    /// extent is that of the turned ellipse.
    #[test]
    fn the_end_of_the_stem_is_the_turned_ellipse() {
        let same = |p: Pos2| p;
        let points = shapes()[4].0[0].flatten(&same);
        let (cx, cy, rx, ry) = (201.0_f32, 113.5_f32, 19.0_f32, 9.5_f32);
        let (sin, cos) = 37.0_f32.to_radians().sin_cos();
        let half_width = (rx * rx * cos * cos + ry * ry * sin * sin).sqrt();
        let half_height = (rx * rx * sin * sin + ry * ry * cos * cos).sqrt();
        let least = |f: fn(&Pos2) -> f32| points.iter().map(f).fold(f32::MAX, f32::min);
        let most = |f: fn(&Pos2) -> f32| points.iter().map(f).fold(f32::MIN, f32::max);
        for (found, wanted) in [
            (least(|p| p.x), cx - half_width),
            (most(|p| p.x), cx + half_width),
            (least(|p| p.y), cy - half_height),
            (most(|p| p.y), cy + half_height),
        ] {
            assert!((found - wanted).abs() < 0.15, "{found} against {wanted}");
        }
    }

    /// The logo draws at any size and scale: shapes with finite points in
    /// the logo's three colors, and none at all where there is no room.
    #[test]
    fn the_logo_paints_at_any_size_and_scale() {
        use crate::egui::{Context, RawInput, epaint::Primitive};
        for ppp in [1.0, 1.5, 2.0] {
            let ctx = Context::default();
            ctx.set_pixels_per_point(ppp);
            for size in [3.0, 8.0, 14.0, 16.0, 20.0, 26.0, 33.0, 84.0, 200.0, 600.0] {
                let rect = Rect::from_min_size(pos2(10.3, 7.7), vec2(size, size));
                let mut out = ctx.run_ui(RawInput::default(), |ui| paint(ui.painter(), rect));
                out.textures_delta.clear();
                let painted = ctx.tessellate(out.shapes, ppp);
                assert_eq!(painted.is_empty(), size * ppp < LEAST_PIXELS, "{size} px");
                let mut colors = Vec::new();
                for primitive in painted {
                    let Primitive::Mesh(mesh) = primitive.primitive else {
                        continue;
                    };
                    assert!(mesh.vertices.iter().all(|v| v.pos.is_finite()));
                    assert!(
                        mesh.indices
                            .iter()
                            .all(|&i| (i as usize) < mesh.vertices.len())
                    );
                    colors.extend(mesh.vertices.iter().map(|v| v.color));
                }
                if size >= 84.0 {
                    for color in [YELLOW, AMBER, BROWN] {
                        assert!(colors.contains(&color), "{size} px has no {color:?}");
                    }
                }
            }
        }
    }
}
