//! The Video Preview's and the Trimmer's transport bars as VEGAS's Help
//! describes them ("Using the Video Preview window", "Using the
//! Trimmer"): the buttons a bar shows (`UiSettings::preview_transport`,
//! `trimmer_transport`), the others in its More menu, whose Edit Visible
//! Button Set ticks which show — check boxes, Show All, Hide All, OK. The
//! Trimmer's has its editing buttons too, drawn here as VEGAS 23's.

use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::egui::{self, Color32, Pos2, Rect, Stroke, pos2, vec2};
use crate::i18n::tr;
use crate::icons::Icon;
use crate::theme;

/// A transport bar's button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportButton {
    Loop,
    PlayFromStart,
    Play,
    Pause,
    Stop,
    GoStart,
    PrevFrame,
    NextFrame,
    GoEnd,
    /// The Trimmer's: Add to Timeline from Cursor and up to it, Fit to
    /// Fill, Enable Timeline Overwrite, Create Subclip, Set In Point and
    /// Set Out Point, Insert Marker, Insert Region, Save Markers/Regions,
    /// Detect Scenes and Add to Timeline from Cursor, Beat Detection.
    AddFromCursor,
    FitToFill,
    Overwrite,
    AddToCursor,
    CreateSubclip,
    SetIn,
    SetOut,
    InsertMarker,
    InsertRegion,
    SaveMarkers,
    DetectScenes,
    BeatDetection,
}

/// Which transport bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bar {
    Preview,
    Trimmer,
}

impl Bar {
    /// Its buttons, in their order: the Trimmer's as VEGAS 23's bar and
    /// More menu list them.
    pub fn all(self) -> &'static [TransportButton] {
        use TransportButton::*;
        match self {
            Bar::Preview => &[
                Loop,
                PlayFromStart,
                Play,
                Pause,
                Stop,
                GoStart,
                PrevFrame,
                NextFrame,
                GoEnd,
            ],
            Bar::Trimmer => &[
                Loop,
                PlayFromStart,
                Play,
                Pause,
                Stop,
                GoStart,
                GoEnd,
                PrevFrame,
                NextFrame,
                AddFromCursor,
                FitToFill,
                Overwrite,
                AddToCursor,
                CreateSubclip,
                SetIn,
                SetOut,
                InsertMarker,
                InsertRegion,
                SaveMarkers,
                DetectScenes,
                BeatDetection,
            ],
        }
    }

    /// What it shows when nothing was ticked: VEGAS's.
    fn default_shown(self) -> Vec<TransportButton> {
        use TransportButton::*;
        match self {
            Bar::Preview => vec![Loop, Play, Pause, Stop],
            Bar::Trimmer => vec![Loop, Play, Pause, Stop, AddFromCursor, FitToFill],
        }
    }
}

impl TransportButton {
    pub fn name(self) -> &'static str {
        match self {
            TransportButton::Loop => tr("Loop Playback"),
            TransportButton::PlayFromStart => tr("Play From Start"),
            TransportButton::Play => tr("Play"),
            TransportButton::Pause => tr("Pause"),
            TransportButton::Stop => tr("Stop"),
            TransportButton::GoStart => tr("Go to Start"),
            TransportButton::PrevFrame => tr("Previous Frame"),
            TransportButton::NextFrame => tr("Next Frame"),
            TransportButton::GoEnd => tr("Go to End"),
            TransportButton::AddFromCursor => tr("Add to Timeline from Cursor"),
            TransportButton::FitToFill => tr("Fit to Fill"),
            TransportButton::Overwrite => tr("Enable Timeline Overwrite"),
            TransportButton::AddToCursor => tr("Add to Timeline up to Cursor"),
            TransportButton::CreateSubclip => tr("Create Subclip…"),
            TransportButton::SetIn => tr("Set In Point"),
            TransportButton::SetOut => tr("Set Out Point"),
            TransportButton::InsertMarker => tr("Insert Marker"),
            TransportButton::InsertRegion => tr("Insert Region"),
            TransportButton::SaveMarkers => tr("Save Markers/Regions"),
            TransportButton::DetectScenes => tr("Detect Scenes and Add to Timeline from Cursor"),
            TransportButton::BeatDetection => tr("Beat Detection"),
        }
    }

    /// Its icon among BananaV's (the Trimmer's own are drawn by
    /// [`paint_trimmer_icon`]; this is their stand-in elsewhere).
    pub fn icon(self) -> Icon {
        match self {
            TransportButton::Loop => Icon::Loop,
            TransportButton::PlayFromStart => Icon::PlayFromStart,
            TransportButton::Play => Icon::Play,
            TransportButton::Pause => Icon::Pause,
            TransportButton::Stop => Icon::Stop,
            TransportButton::GoStart => Icon::GoStart,
            TransportButton::PrevFrame => Icon::PrevFrame,
            TransportButton::NextFrame => Icon::NextFrame,
            TransportButton::GoEnd => Icon::GoEnd,
            TransportButton::AddFromCursor | TransportButton::AddToCursor => Icon::Import,
            TransportButton::FitToFill => Icon::Trim,
            TransportButton::Overwrite => Icon::Paste,
            TransportButton::CreateSubclip => Icon::FilmStrip,
            TransportButton::SetIn => Icon::TrimStart,
            TransportButton::SetOut => Icon::TrimEnd,
            TransportButton::InsertMarker => Icon::Marker,
            TransportButton::InsertRegion => Icon::Region,
            TransportButton::SaveMarkers => Icon::Save,
            TransportButton::DetectScenes => Icon::DetectScenes,
            TransportButton::BeatDetection => Icon::Metronome,
        }
    }

    /// Whether it is one of the Trimmer's editing buttons, drawn as VEGAS's.
    fn trimmer_own(self) -> bool {
        use TransportButton::*;
        matches!(
            self,
            AddFromCursor
                | FitToFill
                | Overwrite
                | AddToCursor
                | CreateSubclip
                | SetIn
                | SetOut
                | InsertMarker
                | InsertRegion
                | SaveMarkers
                | BeatDetection
        )
    }
}

/// The blue of VEGAS's Add Media buttons' box.
const ADD_BLUE: Color32 = Color32::from_rgb(0x3C, 0x8C, 0xE6);
/// The yellow of its in and out points, the orange of its markers, the
/// green of its regions.
const POINT_YELLOW: Color32 = Color32::from_rgb(0xF2, 0xC2, 0x2E);
const MARKER_ORANGE: Color32 = Color32::from_rgb(0xF0, 0x8A, 0x1E);
const REGION_GREEN: Color32 = Color32::from_rgb(0x2E, 0xC8, 0x5A);

/// Draws button `b` in `r` (a square), as VEGAS 23's Trimmer draws it,
/// greyed unless `enabled`; the others as BananaV's icons.
pub(crate) fn paint_trimmer_icon(
    painter: &egui::Painter,
    r: Rect,
    b: TransportButton,
    enabled: bool,
) {
    let ink = if enabled {
        theme::text()
    } else {
        theme::text_disabled()
    };
    let tone = |c: Color32| {
        if enabled { c } else { c.gamma_multiply(0.35) }
    };
    if !b.trimmer_own() {
        crate::icons::paint(painter, r, b.icon(), ink);
        return;
    }
    // A 16-unit grid in the square.
    let u = r.width().min(r.height()) / 16.0;
    let at = |x: f32, y: f32| -> Pos2 { r.min + vec2(x * u, y * u) };
    let line = Stroke::new((1.2 * u).max(1.0), ink);
    // The cursor's pin: a head and its line.
    let pin = |x: f32, y: f32, h: f32| {
        painter.rect_filled(
            Rect::from_min_max(at(x - 2.0, y), at(x + 2.0, y + 3.0)),
            0.5,
            ink,
        );
        painter.add(egui::Shape::convex_polygon(
            vec![at(x - 2.0, y + 3.0), at(x + 2.0, y + 3.0), at(x, y + 5.0)],
            ink,
            Stroke::NONE,
        ));
        painter.line_segment([at(x, y + 4.0), at(x, y + h)], line);
    };
    // The blue box with its arrow, toward `way` (1 right, −1 left).
    let blue_box = |x: f32, y: f32, way: f32| {
        let bx = Rect::from_min_max(at(x, y), at(x + 9.0, y + 7.0));
        painter.rect_filled(bx, 1.0, tone(ADD_BLUE));
        let c = bx.center();
        let w = 2.6 * u;
        let s = Stroke::new((1.1 * u).max(1.0), Color32::WHITE);
        painter.line_segment([c - vec2(w * way, 0.0), c + vec2(w * way, 0.0)], s);
        painter.line_segment(
            [
                c + vec2(w * way, 0.0),
                c + vec2((w - 1.6 * u) * way, -1.6 * u),
            ],
            s,
        );
        painter.line_segment(
            [
                c + vec2(w * way, 0.0),
                c + vec2((w - 1.6 * u) * way, 1.6 * u),
            ],
            s,
        );
    };
    // A box with one of its top corners marked, as the time boxes'.
    let corner_box = |left: bool, color: Color32| {
        let bx = Rect::from_min_max(at(3.0, 3.5), at(13.0, 12.5));
        let (x, dx) = if left {
            (bx.left(), 6.0 * u)
        } else {
            (bx.right(), -6.0 * u)
        };
        painter.add(egui::Shape::convex_polygon(
            vec![
                pos2(x, bx.top()),
                pos2(x + dx, bx.top()),
                pos2(x, bx.top() + 6.0 * u),
            ],
            tone(color),
            Stroke::NONE,
        ));
    };
    match b {
        TransportButton::AddFromCursor => {
            pin(3.5, 1.0, 13.0);
            blue_box(6.0, 8.0, 1.0);
        }
        TransportButton::AddToCursor => {
            blue_box(1.0, 8.0, -1.0);
            pin(12.5, 1.0, 13.0);
        }
        TransportButton::FitToFill => {
            let bx = Rect::from_min_max(at(2.0, 3.0), at(14.0, 13.0));
            painter.rect_stroke(bx, 1.0, line, egui::StrokeKind::Middle);
            let y = bx.center().y;
            painter.line_segment(
                [pos2(r.left() + 4.5 * u, y), pos2(r.left() + 11.5 * u, y)],
                line,
            );
            for (tip, way) in [(4.5, 1.0), (11.5, -1.0)] {
                let p = pos2(r.left() + tip * u, y);
                painter.line_segment([p, p + vec2(2.0 * u * way, -2.0 * u)], line);
                painter.line_segment([p, p + vec2(2.0 * u * way, 2.0 * u)], line);
            }
        }
        TransportButton::Overwrite => {
            // An event laid over another.
            let under = Rect::from_min_max(at(1.0, 3.0), at(11.0, 9.0));
            painter.rect_stroke(under, 1.0, line, egui::StrokeKind::Middle);
            painter.rect_filled(
                Rect::from_min_max(at(5.0, 7.0), at(15.0, 13.0)),
                1.0,
                tone(ADD_BLUE),
            );
        }
        TransportButton::CreateSubclip => {
            // A strip of film, a part of it taken out.
            let strip = Rect::from_min_max(at(1.0, 2.0), at(10.0, 10.0));
            painter.rect_stroke(strip, 1.0, line, egui::StrokeKind::Middle);
            for k in 0..3 {
                let x = 2.5 + 3.0 * k as f32;
                painter.rect_filled(Rect::from_min_max(at(x, 3.0), at(x + 1.5, 4.0)), 0.0, ink);
                painter.rect_filled(Rect::from_min_max(at(x, 8.0), at(x + 1.5, 9.0)), 0.0, ink);
            }
            painter.rect_filled(
                Rect::from_min_max(at(7.0, 8.0), at(15.0, 14.0)),
                1.0,
                tone(Color32::WHITE),
            );
            painter.rect_stroke(
                Rect::from_min_max(at(7.0, 8.0), at(15.0, 14.0)),
                1.0,
                Stroke::new(1.0, Color32::from_gray(40)),
                egui::StrokeKind::Inside,
            );
        }
        TransportButton::SetIn => corner_box(true, POINT_YELLOW),
        TransportButton::SetOut => corner_box(false, POINT_YELLOW),
        TransportButton::InsertMarker => {
            painter.rect_filled(
                Rect::from_min_max(at(4.0, 2.0), at(12.0, 8.0)),
                0.5,
                tone(MARKER_ORANGE),
            );
            painter.line_segment(
                [at(4.5, 2.0), at(4.5, 14.0)],
                Stroke::new((1.4 * u).max(1.0), tone(MARKER_ORANGE)),
            );
        }
        TransportButton::InsertRegion => {
            for x in [2.0, 9.0] {
                painter.rect_filled(
                    Rect::from_min_max(at(x, 5.0), at(x + 5.0, 10.0)),
                    0.5,
                    tone(REGION_GREEN),
                );
            }
        }
        TransportButton::SaveMarkers => {
            painter.rect_filled(
                Rect::from_min_max(at(1.5, 1.5), at(7.5, 6.0)),
                0.5,
                tone(MARKER_ORANGE),
            );
            painter.line_segment(
                [at(2.0, 1.5), at(2.0, 9.0)],
                Stroke::new((1.2 * u).max(1.0), tone(MARKER_ORANGE)),
            );
            crate::icons::paint(
                painter,
                Rect::from_min_max(at(6.0, 6.0), at(15.5, 15.5)),
                Icon::Save,
                ink,
            );
        }
        TransportButton::BeatDetection => {
            // Orange beats, as VEGAS's.
            for (k, h) in [5.0f32, 9.0, 12.0, 7.0].into_iter().enumerate() {
                let x = 2.5 + 3.3 * k as f32;
                painter.rect_filled(
                    Rect::from_min_max(at(x, 8.0 - h / 2.0), at(x + 1.6, 8.0 + h / 2.0)),
                    0.5,
                    tone(MARKER_ORANGE),
                );
            }
        }
        _ => {}
    }
}

/// The dialog: the bar it edits and the boxes ticked.
#[derive(Clone, Debug)]
pub(crate) struct TransportDialog {
    bar: Bar,
    shown: Vec<TransportButton>,
}

impl BananaApp {
    /// The buttons `bar` shows, in its order.
    pub(crate) fn transport_shown(&self, bar: Bar) -> Vec<TransportButton> {
        let set = match bar {
            Bar::Preview => &self.settings.preview_transport,
            Bar::Trimmer => &self.settings.trimmer_transport,
        };
        let mut shown = set.clone().unwrap_or_else(|| bar.default_shown());
        let order = bar.all();
        shown.retain(|b| order.contains(b));
        shown.sort_by_key(|b| order.iter().position(|x| x == b));
        shown
    }

    /// The buttons `bar` keeps in its More menu.
    pub(crate) fn transport_hidden(&self, bar: Bar) -> Vec<TransportButton> {
        let shown = self.transport_shown(bar);
        bar.all()
            .iter()
            .copied()
            .filter(|b| !shown.contains(b))
            .collect()
    }

    /// Opens Edit Visible Button Set for `bar`.
    pub(crate) fn open_transport_buttons(&mut self, bar: Bar) {
        self.transport_dialog = Some(TransportDialog {
            bar,
            shown: self.transport_shown(bar),
        });
    }

    /// The dialog, while it is open.
    pub(crate) fn transport_buttons_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.transport_dialog.clone() else {
            return;
        };
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        let title = match d.bar {
            Bar::Preview => tr("Edit Visible Button Set (Video Preview)"),
            Bar::Trimmer => tr("Edit Visible Button Set (Trimmer)"),
        };
        crate::floating::Window::new(title)
            .id(egui::Id::new("transport-buttons"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_min_width(380.0);
                ui.label(tr(
                    "Buttons the transport bar shows (the others are in its More menu):",
                ));
                ui.add_space(4.0);
                for b in d.bar.all() {
                    let mut on = d.shown.contains(b);
                    crate::widgets::checkbox(ui, &mut on, b.name());
                    if on && !d.shown.contains(b) {
                        d.shown.push(*b);
                    } else if !on {
                        d.shown.retain(|x| x != b);
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button(tr("Show All")).clicked() {
                        d.shown = d.bar.all().to_vec();
                    }
                    if ui.button(tr("Hide All")).clicked() {
                        d.shown.clear();
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ok = ui.button(tr("OK")).clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if ok {
            // In the bar's order.
            let order = d.bar.all();
            let mut shown = d.shown.clone();
            shown.sort_by_key(|b| order.iter().position(|x| x == b));
            match d.bar {
                Bar::Preview => self.settings.preview_transport = Some(shown),
                Bar::Trimmer => self.settings.trimmer_transport = Some(shown),
            }
        }
        self.transport_dialog = (open && !ok && !cancel).then_some(d);
    }

    /// The Video Preview's bar showing Go to Start and the frame steps too,
    /// and its Edit Visible Button Set open (screen tests; the settings in
    /// memory).
    pub fn transport_buttons_for_review(&mut self) {
        self.settings = crate::settings::UiSettings::in_memory();
        use TransportButton::*;
        self.settings.preview_transport =
            Some(vec![Loop, GoStart, PrevFrame, Play, Pause, Stop, NextFrame]);
        self.open_transport_buttons(Bar::Preview);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VEGAS's bar at first; what is ticked shows, in the bar's order, and
    /// the rest goes to More.
    #[test]
    fn a_bar_shows_what_is_ticked_and_more_keeps_the_rest() {
        let mut app = BananaApp::new(egui::Context::default());
        app.settings = crate::settings::UiSettings::in_memory();
        use TransportButton::*;
        assert_eq!(app.transport_shown(Bar::Preview), [Loop, Play, Pause, Stop]);
        assert_eq!(
            app.transport_hidden(Bar::Preview),
            [PlayFromStart, GoStart, PrevFrame, NextFrame, GoEnd]
        );
        // The Trimmer's: VEGAS 23's bar, its More menu in VEGAS's order.
        assert_eq!(
            app.transport_shown(Bar::Trimmer),
            [Loop, Play, Pause, Stop, AddFromCursor, FitToFill]
        );
        assert_eq!(
            app.transport_hidden(Bar::Trimmer),
            [
                PlayFromStart,
                GoStart,
                GoEnd,
                PrevFrame,
                NextFrame,
                Overwrite,
                AddToCursor,
                CreateSubclip,
                SetIn,
                SetOut,
                InsertMarker,
                InsertRegion,
                SaveMarkers,
                DetectScenes,
                BeatDetection
            ]
        );
        app.settings.trimmer_transport = Some(vec![Play, Stop]);
        assert_eq!(
            app.transport_hidden(Bar::Trimmer)[..3],
            [Loop, PlayFromStart, Pause]
        );
    }
}
