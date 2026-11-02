//! The Render dialog's own drawings: tree folders, use-case pictures,
//! template pages, the favorite star, the filter funnel and the view grid.

use crate::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use crate::theme;

use super::templates::UseCase;

const FOLDER: Color32 = Color32::from_rgb(0xE8, 0xB8, 0x3A);
const FOLDER_DARK: Color32 = Color32::from_rgb(0xB8, 0x86, 0x1E);
pub const BLUE: Color32 = Color32::from_rgb(0x2D, 0x7C, 0xD6);
const BLUE_DARK: Color32 = Color32::from_rgb(0x22, 0x5E, 0xA8);
const PAGE: Color32 = Color32::from_rgb(0xD8, 0xDA, 0xE0);

/// What a tree folder holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderKind {
    Plain,
    Video,
    Images,
    Audio,
    Star,
}

/// A folder, with a badge for what it holds.
pub fn folder(p: &Painter, r: Rect, kind: FolderKind) {
    let r = r.shrink(1.0);
    let tab = Rect::from_min_size(
        r.min + vec2(0.0, 1.0),
        vec2(r.width() * 0.45, r.height() * 0.3),
    );
    p.rect_filled(tab, 1.5, FOLDER_DARK);
    let body = Rect::from_min_max(pos2(r.left(), r.top() + r.height() * 0.22), r.max);
    p.rect_filled(body, 1.5, FOLDER);
    let badge = Rect::from_center_size(body.center() + vec2(1.0, 1.0), vec2(8.0, 7.0));
    let dark = Color32::from_rgb(0x5A, 0x40, 0x0C);
    match kind {
        FolderKind::Plain => {}
        FolderKind::Video => {
            p.rect_filled(badge, 1.0, dark);
            for i in 0..3 {
                let y = badge.top() + 1.5 + i as f32 * 2.0;
                p.line_segment(
                    [pos2(badge.left() + 1.5, y), pos2(badge.right() - 1.5, y)],
                    Stroke::new(0.8, FOLDER),
                );
            }
        }
        FolderKind::Images => {
            p.rect_stroke(
                badge,
                0.5,
                Stroke::new(1.0, dark),
                crate::egui::StrokeKind::Inside,
            );
            p.add(Shape::convex_polygon(
                vec![
                    pos2(badge.left() + 1.0, badge.bottom() - 1.0),
                    pos2(badge.center().x, badge.top() + 2.5),
                    pos2(badge.right() - 1.0, badge.bottom() - 1.0),
                ],
                dark,
                Stroke::NONE,
            ));
        }
        FolderKind::Audio => {
            let c = badge.center();
            p.circle_filled(pos2(c.x - 1.5, c.y + 2.0), 1.8, dark);
            p.line_segment(
                [pos2(c.x, c.y + 2.0), pos2(c.x, c.y - 3.5)],
                Stroke::new(1.2, dark),
            );
            p.line_segment(
                [pos2(c.x, c.y - 3.5), pos2(c.x + 3.0, c.y - 2.0)],
                Stroke::new(1.2, dark),
            );
        }
        FolderKind::Star => star(p, badge.center(), 4.0, dark, true),
    }
}

/// A five-pointed star; `filled` or an outline.
pub fn star(p: &Painter, c: Pos2, radius: f32, color: Color32, filled: bool) {
    let points: Vec<Pos2> = (0..10)
        .map(|i| {
            let a = std::f32::consts::PI * (i as f32 / 5.0) - std::f32::consts::FRAC_PI_2;
            let r = if i % 2 == 0 { radius } else { radius * 0.45 };
            pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    if filled {
        // Not convex: fan of triangles from the centre.
        for i in 0..10 {
            p.add(Shape::convex_polygon(
                vec![c, points[i], points[(i + 1) % 10]],
                color,
                Stroke::NONE,
            ));
        }
    } else {
        let mut closed = points.clone();
        closed.push(points[0]);
        p.add(Shape::line(closed, Stroke::new(1.2, color)));
    }
}

/// A page with a folded corner (a template); greyed when not available.
pub fn page(p: &Painter, r: Rect, enabled: bool) {
    let color = if enabled {
        PAGE
    } else {
        theme::text_disabled()
    };
    let r = Rect::from_center_size(r.center(), vec2(r.height() * 0.62, r.height() * 0.8));
    let fold = r.width() * 0.38;
    p.add(Shape::convex_polygon(
        vec![
            r.left_top(),
            pos2(r.right() - fold, r.top()),
            pos2(r.right(), r.top() + fold),
            r.right_bottom(),
            r.left_bottom(),
        ],
        color,
        Stroke::NONE,
    ));
    p.add(Shape::convex_polygon(
        vec![
            pos2(r.right() - fold, r.top()),
            pos2(r.right() - fold, r.top() + fold),
            pos2(r.right(), r.top() + fold),
        ],
        Color32::from_rgb(0x8A, 0x8C, 0x92),
        Stroke::NONE,
    ));
    for i in 0..3 {
        let y = r.top() + r.height() * (0.45 + i as f32 * 0.17);
        p.line_segment(
            [pos2(r.left() + 2.0, y), pos2(r.right() - 2.0, y)],
            Stroke::new(0.8, Color32::from_rgb(0x70, 0x72, 0x78)),
        );
    }
}

/// The filter funnel.
pub fn funnel(p: &Painter, r: Rect, color: Color32) {
    let r = r.shrink(3.0);
    let mid = r.center().x;
    let neck = r.top() + r.height() * 0.45;
    let pts = vec![
        r.left_top(),
        r.right_top(),
        pos2(mid + 1.5, neck),
        pos2(mid + 1.5, r.bottom()),
        pos2(mid - 1.5, r.bottom() - 2.0),
        pos2(mid - 1.5, neck),
        r.left_top(),
    ];
    p.add(Shape::line(pts, Stroke::new(1.3, color)));
}

/// The view button: a grid of small rectangles.
pub fn view_grid(p: &Painter, r: Rect, color: Color32) {
    let r = r.shrink(3.5);
    let (w, h) = (r.width() / 2.0 - 1.0, r.height() / 2.0 - 1.0);
    for (i, j) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let cell = Rect::from_min_size(
            pos2(
                r.left() + i as f32 * (w + 2.0),
                r.top() + j as f32 * (h + 2.0),
            ),
            vec2(w, h),
        );
        p.rect_stroke(
            cell,
            0.5,
            Stroke::new(1.1, color),
            crate::egui::StrokeKind::Inside,
        );
    }
}

/// A small "▾" for buttons that open a menu.
pub fn drop_arrow(p: &Painter, c: Pos2, color: Color32) {
    p.add(Shape::convex_polygon(
        vec![
            pos2(c.x - 3.5, c.y - 1.5),
            pos2(c.x + 3.5, c.y - 1.5),
            pos2(c.x, c.y + 2.5),
        ],
        color,
        Stroke::NONE,
    ));
}

/// The loop region bracket of the "Render loop region only" toggle.
pub fn loop_bracket(p: &Painter, r: Rect, on: bool) {
    let color = if on { theme::banana() } else { theme::marker() };
    let r = r.shrink(2.5);
    let s = Stroke::new(1.6, color);
    p.line_segment([r.left_top(), r.right_top()], s);
    p.line_segment([r.left_top(), r.left_bottom()], s);
    p.line_segment([r.right_top(), r.right_bottom()], s);
    p.rect_filled(
        Rect::from_min_max(r.left_top(), pos2(r.right(), r.top() + 3.0)),
        0.0,
        color,
    );
}

/// The picture of a use case: a simple drawing on a blue base.
pub fn use_case(p: &Painter, r: Rect, case: UseCase) {
    let c = r.center();
    let base = Rect::from_center_size(pos2(c.x, r.bottom() - 6.0), vec2(r.width() * 0.9, 9.0));
    p.add(Shape::convex_polygon(
        vec![
            pos2(base.left(), base.top() + 3.0),
            pos2(base.center().x, base.top()),
            pos2(base.right(), base.top() + 3.0),
            pos2(base.center().x, base.bottom()),
        ],
        BLUE_DARK,
        Stroke::NONE,
    ));
    let light = Color32::from_rgb(0xE8, 0xEA, 0xEE);
    let ink = Color32::from_rgb(0x30, 0x32, 0x38);
    let s = r.width() / 48.0;
    let at = |x: f32, y: f32| pos2(r.left() + x * s, r.top() + y * s);
    match case {
        UseCase::YouTube => {
            let screen = Rect::from_min_max(at(8.0, 10.0), at(40.0, 32.0));
            p.rect_filled(screen, 5.0 * s, Color32::from_rgb(0xD9, 0x4A, 0x3D));
            p.add(Shape::convex_polygon(
                vec![at(20.0, 15.0), at(30.0, 21.0), at(20.0, 27.0)],
                light,
                Stroke::NONE,
            ));
        }
        UseCase::SocialMedia => {
            let phone = Rect::from_min_max(at(16.0, 4.0), at(32.0, 36.0));
            p.rect_filled(phone, 3.0 * s, theme::banana());
            p.rect_filled(
                phone.shrink(2.0 * s),
                1.5 * s,
                Color32::from_rgb(0xF4, 0xE2, 0xA8),
            );
            p.circle_filled(at(24.0, 18.0), 4.0 * s, Color32::from_rgb(0xE0, 0x6A, 0x8A));
        }
        UseCase::WebStreaming => {
            let screen = Rect::from_min_max(at(8.0, 8.0), at(40.0, 30.0));
            p.rect_filled(screen, 2.0 * s, light);
            p.rect_filled(screen.shrink(2.5 * s), 1.0 * s, BLUE);
            p.line_segment(
                [at(24.0, 30.0), at(24.0, 35.0)],
                Stroke::new(2.5 * s, light),
            );
            for k in 1..=3 {
                p.circle_stroke(at(24.0, 23.0), 3.5 * k as f32 * s, Stroke::new(1.0, light));
            }
        }
        UseCase::VfxMaster => {
            for i in 0..3 {
                let y = 10.0 + i as f32 * 8.0;
                let slab = Rect::from_min_max(at(8.0, y), at(40.0, y + 6.0));
                p.rect_filled(slab, 1.5 * s, light);
                p.circle_filled(
                    at(34.0, y + 3.0),
                    1.2 * s,
                    Color32::from_rgb(0x5D, 0xC0, 0x7A),
                );
            }
        }
        UseCase::ImageCompositing => {
            for (i, color) in [(0, PAGE), (1, light), (2, Color32::WHITE)] {
                let o = i as f32 * 4.0;
                let sheet = Rect::from_min_max(at(10.0 + o, 6.0 + o), at(30.0 + o, 30.0 + o));
                p.rect_filled(sheet, 1.0 * s, color);
                p.rect_stroke(
                    sheet,
                    1.0 * s,
                    Stroke::new(1.0, ink),
                    crate::egui::StrokeKind::Inside,
                );
            }
        }
        UseCase::DvdBluray => {
            p.circle_filled(
                at(24.0, 20.0),
                15.0 * s,
                Color32::from_rgb(0xC8, 0xCC, 0xD6),
            );
            p.circle_stroke(
                at(24.0, 20.0),
                10.0 * s,
                Stroke::new(1.0, Color32::from_rgb(0x9A, 0xA0, 0xB0)),
            );
            p.circle_filled(at(24.0, 20.0), 3.0 * s, ink);
        }
        UseCase::AudioMaster => {
            p.circle_filled(at(18.0, 30.0), 5.0 * s, light);
            p.line_segment([at(22.5, 30.0), at(22.5, 8.0)], Stroke::new(2.5 * s, light));
            p.add(Shape::convex_polygon(
                vec![
                    at(22.5, 8.0),
                    at(32.0, 12.0),
                    at(32.0, 17.0),
                    at(22.5, 13.0),
                ],
                light,
                Stroke::NONE,
            ));
        }
        UseCase::Podcast => {
            let head = Rect::from_min_max(at(18.0, 5.0), at(30.0, 25.0));
            p.rect_filled(head, 6.0 * s, light);
            p.line_segment(
                [at(24.0, 29.0), at(24.0, 34.0)],
                Stroke::new(2.0 * s, light),
            );
            p.add(Shape::line(
                vec![
                    at(14.0, 18.0),
                    at(15.0, 25.0),
                    at(24.0, 29.0),
                    at(33.0, 25.0),
                    at(34.0, 18.0),
                ],
                Stroke::new(1.8 * s, light),
            ));
        }
    }
}
