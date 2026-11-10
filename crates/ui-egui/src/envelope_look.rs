//! How envelopes look and how near the pointer takes them — BananaV's own
//! (Options › Envelope Appearance…, and an envelope's shortcut menu): the
//! line's width, the reach around lines and points (wide enough to grab
//! them on a high-resolution display), the points' size and shape (VEGAS's
//! diamonds, or squares). The window shows a sample envelope drawn as the
//! timeline draws them, its reach shaded; the pointer over it takes what
//! the timeline would take, and the timeline follows every change at once.

use std::ops::RangeInclusive;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::egui::{self, Color32, Pos2, Rect, RichText, Sense, Shape, Stroke, pos2, vec2};
use crate::i18n::tr;
use crate::theme;

/// What Options › Envelope Appearance… sets (saved with the preferences).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvelopeLook {
    /// The line's width (points).
    pub line: f32,
    /// How near a line the pointer takes it (points); a point's reach is
    /// two more.
    pub grab: f32,
    /// A point's size: from its middle to its corners (points).
    pub point: f32,
    /// Squares instead of diamonds.
    pub square: bool,
}

/// VEGAS's look (measured on the maintainer's prints).
const DEFAULT: EnvelopeLook = EnvelopeLook {
    line: 2.0,
    grab: 4.0,
    point: 4.0,
    square: false,
};

impl Default for EnvelopeLook {
    fn default() -> EnvelopeLook {
        DEFAULT
    }
}

impl EnvelopeLook {
    pub const LINE: RangeInclusive<f32> = 1.0..=8.0;
    pub const GRAB: RangeInclusive<f32> = 2.0..=24.0;
    pub const POINT: RangeInclusive<f32> = 3.0..=12.0;

    /// Within the ranges (a hand-edited settings file).
    pub fn clamped(self) -> EnvelopeLook {
        let clamp = |v: f32, r: &RangeInclusive<f32>, d: f32| {
            if v.is_finite() {
                v.clamp(*r.start(), *r.end())
            } else {
                d
            }
        };
        EnvelopeLook {
            line: clamp(self.line, &Self::LINE, DEFAULT.line),
            grab: clamp(self.grab, &Self::GRAB, DEFAULT.grab),
            point: clamp(self.point, &Self::POINT, DEFAULT.point),
            square: self.square,
        }
    }

    /// How near a line the pointer takes it: its reach, and never less
    /// than the line's own half width.
    pub fn line_reach(&self) -> f32 {
        self.grab.max(self.line * 0.5 + 1.0)
    }

    /// How near a point the pointer takes it: the reach and two more, and
    /// never less than the point itself.
    pub fn point_reach(&self) -> f32 {
        (self.grab + 2.0).max(self.point + 2.0)
    }

    /// A point's outline around `at`, `r` from its middle to its corners
    /// (a square as big to the eye: its half side 0.8 of that).
    pub fn point_shape(&self, at: Pos2, r: f32) -> Vec<Pos2> {
        if self.square {
            let h = (r * 0.8).round().max(2.0);
            vec![
                pos2(at.x - h, at.y - h),
                pos2(at.x + h, at.y - h),
                pos2(at.x + h, at.y + h),
                pos2(at.x - h, at.y + h),
            ]
        } else {
            vec![
                pos2(at.x, at.y - r),
                pos2(at.x + r, at.y),
                pos2(at.x, at.y + r),
                pos2(at.x - r, at.y),
            ]
        }
    }
}

static LOOK: RwLock<EnvelopeLook> = RwLock::new(DEFAULT);

/// The look in use (the timeline's envelopes draw and are taken by it).
pub(crate) fn get() -> EnvelopeLook {
    LOOK.read().map(|l| *l).unwrap_or_default()
}

/// Uses `look` from now on.
pub(crate) fn set(look: EnvelopeLook) {
    if let Ok(mut l) = LOOK.write() {
        *l = look.clamped();
    }
}

/// The sample envelope: its points, as fractions of the sample's width
/// and height (0 at the top).
const SAMPLE: [(f32, f32); 5] = [
    (0.06, 0.62),
    (0.28, 0.30),
    (0.50, 0.55),
    (0.72, 0.22),
    (0.94, 0.48),
];

/// What the pointer takes in the sample at `p` (its point, else its line).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Taken {
    Point(usize),
    Line,
}

/// The sample's points in `r`.
fn sample_points(r: Rect) -> Vec<Pos2> {
    SAMPLE
        .iter()
        .map(|&(x, y)| pos2(r.left() + x * r.width(), r.top() + y * r.height()))
        .collect()
}

/// How far `p` is from the segment `a`–`b`.
fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t - p).length()
}

/// What `look` lets the pointer at `p` take of the sample in `r`.
fn taken(look: &EnvelopeLook, r: Rect, p: Pos2) -> Option<Taken> {
    let pts = sample_points(r);
    if let Some(i) = pts
        .iter()
        .position(|q| (*q - p).length() <= look.point_reach())
    {
        return Some(Taken::Point(i));
    }
    pts.windows(2)
        .any(|w| segment_distance(p, w[0], w[1]) <= look.line_reach())
        .then_some(Taken::Line)
}

/// Draws the sample envelope in `r` with `look`: a track's lane, its
/// reach shaded (lighter where the pointer takes it), the line and the
/// points as the timeline draws them; what `pointer` is over shows as the
/// timeline shows it.
fn paint_sample(painter: &egui::Painter, r: Rect, look: &EnvelopeLook, pointer: Option<Pos2>) {
    painter.rect_filled(r, 2.0, theme::track_video());
    // The timeline's grid, faint.
    for i in 1..8 {
        let x = r.left() + r.width() * i as f32 / 8.0;
        painter.line_segment(
            [pos2(x, r.top()), pos2(x, r.bottom())],
            Stroke::new(1.0, theme::grid().gamma_multiply(0.5)),
        );
    }
    let color = Color32::from_rgb(0x4E, 0x9C, 0xF0);
    let pts = sample_points(r);
    let hot = pointer.and_then(|p| taken(look, r, p));
    // The reach: a band along the line, a disc around each point.
    let reach = Color32::from_rgba_unmultiplied(255, 214, 90, 34);
    let reach_hot = Color32::from_rgba_unmultiplied(255, 214, 90, 70);
    let band = if hot == Some(Taken::Line) {
        reach_hot
    } else {
        reach
    };
    painter.add(Shape::line(
        pts.clone(),
        Stroke::new(look.line_reach() * 2.0, band),
    ));
    for (i, q) in pts.iter().enumerate() {
        let fill = if hot == Some(Taken::Point(i)) {
            reach_hot
        } else {
            reach
        };
        painter.circle_filled(*q, look.point_reach(), fill);
    }
    // The line and its points, as `envelopes::paint_one` draws them.
    painter.add(Shape::line(
        pts.clone(),
        Stroke::new(look.line + 2.0, Color32::from_black_alpha(64)),
    ));
    painter.add(Shape::line(pts.clone(), Stroke::new(look.line, color)));
    for (i, q) in pts.iter().enumerate() {
        if hot == Some(Taken::Point(i)) {
            for (grow, gray) in [(2.0, 30), (1.0, 176), (0.0, 240)] {
                painter.add(Shape::convex_polygon(
                    look.point_shape(*q, look.point + grow),
                    Color32::from_gray(gray),
                    Stroke::NONE,
                ));
            }
        } else {
            painter.add(Shape::convex_polygon(
                look.point_shape(*q, look.point + 1.0),
                Color32::from_gray(30),
                Stroke::NONE,
            ));
            painter.add(Shape::convex_polygon(
                look.point_shape(*q, look.point),
                color,
                Stroke::NONE,
            ));
        }
    }
}

/// The Envelope Appearance window: what it began with (Cancel puts it
/// back).
#[derive(Debug)]
pub struct EnvelopeLookDialog {
    before: EnvelopeLook,
}

impl EnvelopeLookDialog {
    pub fn new(app: &BananaApp) -> EnvelopeLookDialog {
        EnvelopeLookDialog {
            before: app.settings.prefs.envelope_look,
        }
    }

    /// Draws the window; `false` once it is closed. Every change shows on
    /// the timeline at once; OK keeps it, Cancel puts back the look it
    /// began with.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let (mut open, mut ok, mut cancel) = (true, false, false);
        let mut look = app.settings.prefs.envelope_look;
        crate::floating::Window::new(tr("Envelope Appearance"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(250.0);
                        ui.spacing_mut().item_spacing.y = 6.0;
                        let slider = |ui: &mut egui::Ui,
                                      label: &str,
                                      value: &mut f32,
                                      range: RangeInclusive<f32>| {
                            ui.label(label);
                            ui.add(
                                egui::Slider::new(value, range)
                                    .step_by(0.5)
                                    .suffix(" px")
                                    .custom_formatter(|v, _| crate::fmt::num(v, 1)),
                            );
                        };
                        slider(ui, tr("Line width:"), &mut look.line, EnvelopeLook::LINE);
                        slider(ui, tr("Grab area:"), &mut look.grab, EnvelopeLook::GRAB);
                        slider(ui, tr("Point size:"), &mut look.point, EnvelopeLook::POINT);
                        ui.label(tr("Point shape:"));
                        ui.horizontal(|ui| {
                            crate::widgets::radio_value(ui, &mut look.square, false, tr("Diamond"));
                            crate::widgets::radio_value(ui, &mut look.square, true, tr("Square"));
                        });
                        ui.add_space(4.0);
                        if ui.button(tr("Default")).clicked() {
                            look = EnvelopeLook::default();
                        }
                    });
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(tr("Preview")).strong());
                        let (r, resp) = ui.allocate_exact_size(vec2(360.0, 150.0), Sense::hover());
                        let pointer = resp.hover_pos();
                        paint_sample(&ui.painter_at(r), r.shrink(1.0), &look, pointer);
                        ui.painter().rect_stroke(
                            r,
                            2.0,
                            Stroke::new(1.0, theme::separator()),
                            egui::StrokeKind::Inside,
                        );
                        crate::tabs::hint(
                            ui,
                            tr("The shaded area is where a click takes the line or a point: point at the sample to try it."),
                        );
                    });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ok = ui
                        .add(egui::Button::new("OK").min_size(vec2(72.0, 0.0)))
                        .clicked();
                    cancel = ui
                        .add(egui::Button::new(tr("Cancel")).min_size(vec2(72.0, 0.0)))
                        .clicked();
                });
            });
        let look = look.clamped();
        if look != app.settings.prefs.envelope_look {
            app.settings.prefs.envelope_look = look;
            set(look);
        }
        if ok {
            let _ = app.settings.save();
            return false;
        }
        if cancel || !open {
            app.settings.prefs.envelope_look = self.before;
            set(self.before);
            return false;
        }
        true
    }
}

impl BananaApp {
    /// Options › Envelope Appearance….
    pub(crate) fn open_envelope_look(&mut self) {
        if self.menu_dialogs.more.envelope_look.is_none() {
            self.menu_dialogs.more.envelope_look = Some(EnvelopeLookDialog::new(self));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaches_follow_the_settings_and_never_fall_inside() {
        let look = EnvelopeLook::default();
        assert_eq!((look.line_reach(), look.point_reach()), (4.0, 6.0));
        // A big point is taken all over, a thick line all across.
        let big = EnvelopeLook {
            line: 8.0,
            grab: 2.0,
            point: 12.0,
            square: true,
        };
        assert_eq!(big.point_reach(), 14.0);
        assert_eq!(big.line_reach(), 5.0);
        // Out of range or not a number: back within.
        let wild = EnvelopeLook {
            line: f32::NAN,
            grab: 99.0,
            point: 0.0,
            square: false,
        }
        .clamped();
        assert_eq!((wild.line, wild.grab, wild.point), (2.0, 24.0, 3.0));
    }

    #[test]
    fn the_sample_is_taken_as_the_timeline_takes_envelopes() {
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(360.0, 150.0));
        let pts = sample_points(r);
        let small = EnvelopeLook::default();
        let wide = EnvelopeLook {
            grab: 16.0,
            ..small
        };
        // On a point: the point.
        assert_eq!(taken(&small, r, pts[2]), Some(Taken::Point(2)));
        // 10 points above the middle of a segment: only the wide reach.
        let mid = pts[1] + (pts[2] - pts[1]) * 0.5 - vec2(0.0, 10.0);
        assert_eq!(taken(&small, r, mid), None);
        assert_eq!(taken(&wide, r, mid), Some(Taken::Line));
        // Squares or diamonds: four corners, the square a little smaller.
        let sq = EnvelopeLook {
            square: true,
            ..small
        };
        assert_eq!(sq.point_shape(pos2(0.0, 0.0), 5.0)[2], pos2(4.0, 4.0));
        assert_eq!(small.point_shape(pos2(0.0, 0.0), 5.0)[1], pos2(5.0, 0.0));
    }
}
