//! The bus tracks, as VEGAS's: below a splitter at the timeline's bottom,
//! the video bus track (View › Video Bus Track, Ctrl+Shift+B: the video
//! output's — Video Output FX, Mute, Bypass FX and Envelopes, Automation
//! Settings and its Motion Blur and Supersampling sliders; its Mute, Fade
//! to Color, Motion Blur Amount and Video Supersampling envelopes)
//! and the audio bus tracks (View › Audio Bus Tracks, B: a track for each
//! bus and each assignable effects chain — Bus FX, Mute, Solo, volume and
//! pan, and Automation Settings: volume, pan and Mute as automation
//! controls, as a track's; its volume, pan and mute envelopes). Each one's
//! menu inserts or removes its envelopes, sets its automation and
//! minimizes or maximizes it, its bottom edge
//! sizes it, and its lane's envelopes are edited as a track's: a point
//! dragged (one added where the line is dragged), a double-click on the
//! line adding one, the envelope's menu (a point's curve, Delete Point,
//! Add Point, Thin, Flip, Reset All Points, Remove Envelope).

use bv_core::Tick;
use bv_timeline::{EnvelopeKind, EnvelopeOwner, KeyCurve, Project, Strip};

use super::headers::{MAX_GAIN, MUTE_ON, STRIP_W, Thumb, automation_tip, letter_button, slider};
use super::{Layout, TimelineView};
use crate::app::BananaApp;
use crate::automation::{AutoOwner, TrackControl};
use crate::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, pos2, vec2};
use crate::fx_window::FxTarget;
use crate::i18n::tr;
use crate::menus::{sub, toggle_item};
use crate::theme;
use crate::widgets::mark_automation;

/// A bus track: the video bus's, or a bus's or a chain's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BusTrack {
    Video,
    Strip(Strip),
}

impl BusTrack {
    /// Whose envelopes its lane shows.
    fn owner(self) -> EnvelopeOwner {
        match self {
            BusTrack::Video => EnvelopeOwner::VideoBus,
            BusTrack::Strip(s) => EnvelopeOwner::Strip(s),
        }
    }

    /// The envelopes its menu inserts or removes.
    fn kinds(self) -> Vec<(EnvelopeKind, &'static str)> {
        match self {
            BusTrack::Video => vec![
                (EnvelopeKind::Mute, tr("Mute")),
                (EnvelopeKind::FadeToColor, tr("Fade to Color")),
                (EnvelopeKind::MotionBlur, tr("Motion Blur Amount")),
                (EnvelopeKind::Supersampling, tr("Video Supersampling")),
            ],
            BusTrack::Strip(_) => vec![
                (EnvelopeKind::Mute, tr("Mute")),
                (EnvelopeKind::Volume, tr("Volume")),
                (EnvelopeKind::Pan, tr("Pan")),
            ],
        }
    }

    /// Its envelope `kind` added, or removed when it has it.
    fn toggle(
        self,
        p: &mut Project,
        kind: EnvelopeKind,
        curve: KeyCurve,
    ) -> Result<bool, bv_timeline::EditError> {
        match self {
            BusTrack::Video => p.toggle_video_bus_envelope(kind, curve),
            BusTrack::Strip(s) => p.toggle_strip_envelope(s, kind, curve),
        }
    }
}

/// A bus track's place in the timeline: which, its top and height.
pub(crate) struct BusRow {
    pub track: BusTrack,
    pub top: f32,
    pub height: f32,
    /// Its Surround Pan Keyframes row's height, at its bottom (0: none).
    pub keys: f32,
}

impl BusRow {
    /// Without its keyframes row: where its envelopes are.
    pub fn body(&self, x0: f32, x1: f32) -> Rect {
        Rect::from_min_max(
            pos2(x0, self.top),
            pos2(x1, self.top + self.height - self.keys),
        )
    }

    /// Its keyframes row.
    pub fn keys_rect(&self, x0: f32, x1: f32) -> Rect {
        Rect::from_min_max(
            pos2(x0, self.top + self.height - self.keys),
            pos2(x1, self.top + self.height),
        )
    }
}

/// The splitter's height; a bus track's height and its least; the area's
/// height and its least.
pub(crate) const SPLIT_H: f32 = 5.0;
pub(crate) const BUS_H: f32 = 48.0;
pub(crate) const MIN_BUS_H: f32 = 26.0;
pub(crate) const AREA: f32 = 150.0;
pub(crate) const MIN_AREA: f32 = 30.0;

/// A bus track's envelope point being dragged, and the undo step the drag
/// makes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BusDrag {
    track: BusTrack,
    kind: EnvelopeKind,
    point: usize,
    key: egui::Id,
}

/// What a bus track's envelope menu is for: an envelope, a point of it or
/// a place on its line.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BusPoint {
    track: BusTrack,
    hit: super::envelopes::StripHit,
}

/// A bus track's height: its own, else the default.
pub(crate) fn height_of(view: &TimelineView, t: BusTrack) -> f32 {
    view.bus_heights.get(&t).copied().unwrap_or(BUS_H)
}

/// Its color band's color: VEGAS's busses and chains apart.
fn band_color(app: &BananaApp, project: &Project, s: Strip) -> Color32 {
    let k = match s {
        Strip::Bus(id) => project.bus_index(id).unwrap_or(0),
        Strip::Chain(id) => {
            project.busses.len()
                + project
                    .fx_chains
                    .iter()
                    .position(|c| c.id == id)
                    .unwrap_or(0)
        }
    };
    app.settings.prefs.track_color(k)
}

/// The bus tracks: the splitter, each one's header and lane, the needles
/// through them.
pub(crate) fn ui(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
    project: &Project,
) {
    if l.bus_split.height() <= 0.0 {
        view.bus_focus = false;
        return;
    }
    // The keys' focus: the bus area when it is clicked, the tracks when
    // they are (Ctrl+Shift+Up/Down size the bus tracks or the tracks).
    if let Some(p) = ui.input(|i| {
        i.pointer
            .any_pressed()
            .then(|| i.pointer.interact_pos())
            .flatten()
    }) {
        if l.bus_headers.contains(p) || l.bus_lanes.contains(p) {
            view.bus_focus = true;
        } else if l.lanes.contains(p) || l.headers.contains(p) {
            view.bus_focus = false;
        }
    }
    splitter(view, ui, l);
    let painter = ui.painter();
    painter.rect_filled(l.bus_headers, 0.0, theme::panel());
    let lanes = painter.with_clip_rect(l.bus_lanes);
    lanes.rect_filled(l.bus_lanes, 0.0, theme::bg());
    super::lanes::paint_grid(&lanes, view, l, app, l.bus_lanes.y_range());
    for row in &l.bus_rows {
        lane(view, ui, app, l, project, row);
        match row.track {
            BusTrack::Video => video_header(view, ui, app, l, project, row),
            BusTrack::Strip(s) => header(view, ui, app, l, project, row, s),
        }
    }
    needles(ui, app, view, l);
}

impl TimelineView {
    /// Ctrl+Shift+Up/Down with the bus area in focus: every bus track (the
    /// video bus's too while it shows) taller or shorter, by `k`.
    pub(crate) fn scale_bus_heights(&mut self, project: &Project, k: f32) {
        let mut all: Vec<BusTrack> = project
            .busses
            .iter()
            .map(|b| BusTrack::Strip(Strip::Bus(b.id)))
            .chain(
                project
                    .fx_chains
                    .iter()
                    .map(|c| BusTrack::Strip(Strip::Chain(c.id))),
            )
            .collect();
        if self.show_video_bus {
            all.push(BusTrack::Video);
        }
        for t in all {
            let h = (height_of(self, t) * k).clamp(MIN_BUS_H, super::MAX_TRACK_H);
            self.bus_heights.insert(t, h);
        }
    }
}

/// The splitter between the tracks and the bus tracks: dragged, it sizes
/// their area.
fn splitter(view: &mut TimelineView, ui: &egui::Ui, l: &Layout) {
    let resp = ui.interact(
        l.bus_split,
        egui::Id::new("bus-tracks-splitter"),
        Sense::drag(),
    );
    let painter = ui.painter();
    painter.rect_filled(l.bus_split, 0.0, theme::panel_dark());
    painter.hline(
        l.bus_split.x_range(),
        l.bus_split.center().y,
        Stroke::new(1.0, theme::separator()),
    );
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    if resp.dragged() {
        view.bus_area = (view.bus_area - resp.drag_delta().y).max(MIN_AREA);
    }
}

/// A bus track's header: its band (the bus's letter, or "fx" for a chain),
/// its name, Bus FX, Mute and Solo, its volume and pan; its menu; its
/// bottom edge sizes it.
fn header(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
    project: &Project,
    row: &BusRow,
    s: Strip,
) {
    let r = row.body(l.bus_headers.left(), l.bus_headers.right());
    if !r.intersects(l.bus_headers) {
        return;
    }
    let Some(ch) = project.strip_channel(s) else {
        return;
    };
    let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(l.bus_headers));
    cui.set_clip_rect(l.bus_headers);
    let ui = &mut cui;
    let id = |k: &str| egui::Id::new(("bus-track", s, k));
    let body = ui.interact(r, id("body"), Sense::click());
    let painter = ui.painter().clone();
    painter.rect_filled(r.shrink2(vec2(0.0, 0.5)), 0.0, theme::panel());
    let band = Rect::from_min_size(r.min, vec2(STRIP_W, r.height() - 1.0));
    painter.rect_filled(band, 0.0, band_color(app, project, s));
    painter.line_segment(
        [r.left_bottom(), r.right_bottom()],
        Stroke::new(1.0, theme::panel_dark()),
    );
    let mark = match s {
        Strip::Bus(id) => project.bus_index(id).map_or_else(String::new, |i| {
            bv_timeline::busses::bus_letter(i).to_string()
        }),
        Strip::Chain(_) => "fx".to_string(),
    };
    painter.text(
        pos2(band.center().x, r.top() + 12.0),
        Align2::CENTER_CENTER,
        mark,
        FontId::proportional(12.0),
        Color32::WHITE,
    );

    // Top row: the name, Bus FX, Mute and Solo.
    let top = r.top() + 4.0;
    let right = r.right() - 6.0;
    let solo_rect = Rect::from_min_size(pos2(right - 17.0, top), vec2(17.0, 17.0));
    let mute_rect = solo_rect.translate(vec2(-20.0, 0.0));
    let fx_rect = mute_rect.translate(vec2(-22.0, 0.0));
    let auto_rect = fx_rect.translate(vec2(-20.0, 0.0));
    let owner = AutoOwner::Strip(s);
    let auto = ch.automation;
    let (mute_tip, solo_tip, fx_tip, target) = match s {
        Strip::Bus(id) => (
            tr("Mute Bus"),
            tr("Solo: mutes the busses not soloed"),
            tr("Bus FX"),
            FxTarget::Bus(id),
        ),
        Strip::Chain(id) => (
            tr("Mute Assignable FX"),
            tr("Solo: mutes the busses and chains not soloed"),
            tr("Assignable FX"),
            FxTarget::Chain(id),
        ),
    };
    // Mute: with Automation Settings an automation control (the mute
    // envelope; with automation off, the whole bus).
    let muted = if auto.controls {
        app.owner_muted(owner)
    } else {
        ch.muted
    };
    let mute_tip = automation_tip(mute_tip, auto);
    if letter_button(ui, mute_rect, id("mute"), muted, "M", MUTE_ON, &mute_tip) {
        app.click_mute(owner, id("mute"));
    }
    if auto.controls {
        mark_automation(ui, mute_rect);
    }
    // Automation Settings: its controls trims or automation controls (its
    // mode in the menu).
    let resp = ui.interact(auto_rect, id("automation"), Sense::click());
    if auto.controls {
        painter.rect_filled(auto_rect, 2.0, theme::tab_on());
    } else if resp.hovered() {
        painter.rect_filled(auto_rect, 2.0, theme::hover());
    }
    crate::icons::paint(
        &painter,
        auto_rect.shrink(1.5),
        crate::icons::Icon::Automation,
        theme::text(),
    );
    let tip = trf!(
        "Automation Settings: {}",
        crate::commands::automation_mode_name(auto.mode)
    );
    if resp.on_hover_text(tip).clicked() {
        app.toggle_owner_automation_controls(owner);
    }
    if letter_button(
        ui,
        solo_rect,
        id("solo"),
        ch.solo,
        "S",
        theme::banana(),
        solo_tip,
    ) {
        app.modify(move |p| {
            if let Some(c) = p.strip_channel_mut(s) {
                c.solo = !c.solo;
            }
        });
    }
    // Bus FX: the Plug-In Chooser while it has none, else its FX window.
    if letter_button(
        ui,
        fx_rect,
        id("fx"),
        !ch.effects.is_empty(),
        "fx",
        theme::banana(),
        fx_tip,
    ) {
        app.clip_windows.fx_target = Some(target);
    }
    let name_rect = Rect::from_min_max(
        pos2(band.right() + 4.0, top),
        pos2(auto_rect.left() - 4.0, top + 17.0),
    );
    crate::widgets::fit_text(
        &painter,
        pos2(name_rect.left() + 2.0, name_rect.center().y),
        Align2::LEFT_CENTER,
        crate::tabs::console::strip_name(project, s),
        FontId::proportional(12.5),
        if muted {
            theme::text_dim()
        } else {
            theme::text()
        },
        name_rect.width() - 2.0,
    );

    // Second row: its volume and pan, room enough.
    if r.height() >= 40.0 {
        let y = r.top() + 33.0;
        let x0 = band.right() + 6.0;
        let font = FontId::proportional(11.0);
        // Volume and pan: trims, or with Automation Settings automation
        // controls.
        let gain = app.owner_value(owner, TrackControl::Volume);
        let label = if gain <= 1e-4 {
            "−∞ dB".to_string()
        } else {
            format!("{} dB", crate::fmt::num(20.0 * gain.log10() as f64, 1))
        };
        let text = painter.text(
            pos2(x0, y),
            Align2::LEFT_CENTER,
            label,
            font.clone(),
            theme::text(),
        );
        let vol = Rect::from_min_size(pos2(x0 + 44.0, y - 8.0), vec2(38.0, 16.0));
        let _ = text;
        let key = id("volume");
        let (moved, released) = slider(
            ui,
            vol,
            key,
            (gain / MAX_GAIN).max(0.0).sqrt(),
            (1.0 / MAX_GAIN).sqrt(),
            Thumb::Knob,
            &automation_tip(tr("Volume (double-click: 0 dB; Ctrl+drag: fine)"), auto),
            auto.controls,
        );
        if let Some(v) = moved {
            app.set_owner_control(owner, TrackControl::Volume, v * v * MAX_GAIN, key);
        }
        if released {
            app.release_owner_control(owner, TrackControl::Volume);
        }
        let surround = bv_timeline::SurroundOwner::Strip(s);
        if project.surround_pans(surround) {
            let top = y - 9.0;
            let side = (r.bottom() - 3.0 - top)
                .min(right - 2.0 - (vol.right() + 10.0))
                .min(36.0);
            if side >= 14.0 {
                let rect = Rect::from_min_size(pos2(right - 2.0 - side, top), vec2(side, side));
                crate::tabs::surround::mini_panner(
                    ui,
                    app,
                    rect,
                    surround,
                    id("surround"),
                    tr("Surround Panner (double-click: open; Ctrl+drag: fine)"),
                );
            }
        } else {
            let pan = app.owner_value(owner, TrackControl::Pan);
            let label = if pan.abs() < 0.005 {
                tr("Center").to_string()
            } else if pan < 0.0 {
                trf!("{}% L", (-pan * 100.0).round())
            } else {
                trf!("{}% R", (pan * 100.0).round())
            };
            let px = vol.right() + 10.0;
            painter.text(pos2(px, y), Align2::LEFT_CENTER, label, font, theme::text());
            let p_rect = Rect::from_min_max(
                pos2(px + 44.0, y - 8.0),
                pos2((right - 2.0).max(px + 74.0), y + 8.0),
            );
            let pkey = id("pan");
            let (moved, released) = slider(
                ui,
                p_rect,
                pkey,
                (pan + 1.0) / 2.0,
                0.5,
                Thumb::Triangle,
                &automation_tip(tr("Pan (double-click: center; Ctrl+drag: fine)"), auto),
                auto.controls,
            );
            if let Some(v) = moved {
                let v = (v * 2.0 - 1.0).clamp(-1.0, 1.0);
                app.set_owner_control(owner, TrackControl::Pan, v, pkey);
            }
            if released {
                app.release_owner_control(owner, TrackControl::Pan);
            }
        }
    }

    // Its menu, its bottom edge.
    let fx = match s {
        Strip::Bus(_) => tr("Bus FX"),
        Strip::Chain(_) => tr("Assignable FX"),
    };
    let area = l.bus_lanes.height();
    let track = BusTrack::Strip(s);
    body.context_menu(|ui| header_menu(ui, view, app, track, (fx, target), area));
    bottom_edge(view, ui, r, track, id("edge"));
}

/// The video bus track's header: its band, its name, Video Output FX,
/// Mute and Bypass FX and Envelopes; its menu; its bottom edge.
fn video_header(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
    project: &Project,
    row: &BusRow,
) {
    let r = row.body(l.bus_headers.left(), l.bus_headers.right());
    if !r.intersects(l.bus_headers) {
        return;
    }
    let bus = &project.video_bus;
    let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(l.bus_headers));
    cui.set_clip_rect(l.bus_headers);
    let ui = &mut cui;
    let id = |k: &str| egui::Id::new(("video-bus-track", k));
    let body = ui.interact(r, id("body"), Sense::click());
    let painter = ui.painter().clone();
    painter.rect_filled(r.shrink2(vec2(0.0, 0.5)), 0.0, theme::panel());
    let band = Rect::from_min_size(r.min, vec2(STRIP_W, r.height() - 1.0));
    painter.rect_filled(band, 0.0, theme::track_video());
    painter.line_segment(
        [r.left_bottom(), r.right_bottom()],
        Stroke::new(1.0, theme::panel_dark()),
    );
    crate::icons::paint(
        &painter,
        Rect::from_center_size(pos2(band.center().x, r.top() + 12.0), vec2(14.0, 14.0)),
        crate::icons::Icon::FileVideo,
        Color32::WHITE,
    );
    let top = r.top() + 4.0;
    let right = r.right() - 6.0;
    let bypass_rect = Rect::from_min_size(pos2(right - 17.0, top), vec2(17.0, 17.0));
    let mute_rect = bypass_rect.translate(vec2(-20.0, 0.0));
    let fx_rect = mute_rect.translate(vec2(-22.0, 0.0));
    let auto_rect = fx_rect.translate(vec2(-20.0, 0.0));
    let owner = AutoOwner::VideoBus;
    let auto = bus.automation;
    // Mute: with Automation Settings an automation control (the mute
    // envelope; with automation off, the whole output).
    let muted = if auto.controls {
        app.owner_muted(owner)
    } else {
        bus.muted
    };
    let mute_tip = automation_tip(tr("Mute the video output"), auto);
    if letter_button(ui, mute_rect, id("mute"), muted, "M", MUTE_ON, &mute_tip) {
        app.click_mute(owner, id("mute"));
    }
    if auto.controls {
        mark_automation(ui, mute_rect);
    }
    // Automation Settings: its sliders and Mute as automation controls
    // (its mode in the menu).
    let resp = ui.interact(auto_rect, id("automation"), Sense::click());
    if auto.controls {
        painter.rect_filled(auto_rect, 2.0, theme::tab_on());
    } else if resp.hovered() {
        painter.rect_filled(auto_rect, 2.0, theme::hover());
    }
    crate::icons::paint(
        &painter,
        auto_rect.shrink(1.5),
        crate::icons::Icon::Automation,
        theme::text(),
    );
    let tip = trf!(
        "Automation Settings: {}",
        crate::commands::automation_mode_name(auto.mode)
    );
    if resp.on_hover_text(tip).clicked() {
        app.toggle_owner_automation_controls(owner);
    }
    // Bypass FX and Envelopes: the output's effects and these envelopes
    // do nothing.
    let b = ui.interact(bypass_rect, id("bypass"), Sense::click());
    if bus.bypass {
        painter.rect_filled(bypass_rect, 2.0, MUTE_ON);
    } else if b.hovered() {
        painter.rect_filled(bypass_rect, 2.0, theme::hover());
    }
    crate::icons::paint(
        &painter,
        bypass_rect.shrink(2.0),
        crate::icons::Icon::BypassFx,
        if bus.bypass {
            Color32::WHITE
        } else {
            theme::text()
        },
    );
    if b.on_hover_text(tr("Bypass FX and Envelopes")).clicked() {
        app.modify(|p| p.video_bus.bypass = !p.video_bus.bypass);
    }
    // Video Output FX: the Plug-In Chooser while it has none, else its
    // window.
    let has = project.video_output_fx.iter().any(|e| e.enabled);
    let fx = tr("Video Output FX");
    if letter_button(ui, fx_rect, id("fx"), has, "fx", theme::banana(), fx) {
        app.clip_windows.fx_target = Some(FxTarget::VideoOutput);
    }
    let name_rect = Rect::from_min_max(
        pos2(band.right() + 4.0, top),
        pos2(auto_rect.left() - 4.0, top + 17.0),
    );
    // Its Motion Blur and Supersampling sliders, room enough: their own
    // values, or as automation controls the envelopes'.
    let rows = [
        (
            TrackControl::MotionBlur,
            tr("Motion blur"),
            33.0,
            tr(
                "Motion Blur Amount: the time each frame is blurred over (double-click: none; Ctrl+drag: fine)",
            ),
        ),
        (
            TrackControl::Supersampling,
            tr("Supersampling"),
            55.0,
            tr(
                "Video Supersampling: the frames worked out for each frame (double-click: none; Ctrl+drag: fine)",
            ),
        ),
    ];
    for (control, label, dy, hint) in rows {
        if r.height() < dy + 9.0 {
            break;
        }
        let y = r.top() + dy;
        let x0 = band.right() + 6.0;
        let value = app.owner_value(owner, control);
        let (pos, text) = match control {
            TrackControl::MotionBlur => (
                value.clamp(0.0, 1.0),
                trf!("{} s", crate::fmt::num(value as f64, 2)),
            ),
            _ => (
                ((value.max(1.0) - 1.0) / 7.0).clamp(0.0, 1.0),
                format!("{}", value.max(1.0).round()),
            ),
        };
        let font = FontId::proportional(11.0);
        crate::widgets::fit_text(
            &painter,
            pos2(x0, y),
            Align2::LEFT_CENTER,
            format!("{label}: {text}"),
            font,
            theme::text(),
            110.0,
        );
        let rect = Rect::from_min_max(
            pos2(x0 + 114.0, y - 8.0),
            pos2((right - 2.0).max(x0 + 150.0), y + 8.0),
        );
        let key = id(match control {
            TrackControl::MotionBlur => "motion-blur",
            _ => "supersampling",
        });
        let (moved, released) = slider(
            ui,
            rect,
            key,
            pos,
            0.0,
            Thumb::Knob,
            &automation_tip(hint, auto),
            auto.controls,
        );
        if let Some(v) = moved {
            let v = match control {
                TrackControl::MotionBlur => v,
                _ => 1.0 + (v * 7.0).round(),
            };
            app.set_owner_control(owner, control, v, key);
        }
        if released {
            app.release_owner_control(owner, control);
        }
    }
    crate::widgets::fit_text(
        &painter,
        pos2(name_rect.left() + 2.0, name_rect.center().y),
        Align2::LEFT_CENTER,
        tr("Video Output").to_string(),
        FontId::proportional(12.5),
        if bus.muted {
            theme::text_dim()
        } else {
            theme::text()
        },
        name_rect.width() - 2.0,
    );
    let area = l.bus_lanes.height();
    body.context_menu(|ui| {
        header_menu(
            ui,
            view,
            app,
            BusTrack::Video,
            (fx, FxTarget::VideoOutput),
            area,
        )
    });
    bottom_edge(view, ui, r, BusTrack::Video, id("edge"));
}

/// A bus track's bottom edge: dragged, its height.
fn bottom_edge(view: &mut TimelineView, ui: &egui::Ui, r: Rect, track: BusTrack, id: egui::Id) {
    let edge = Rect::from_min_max(
        pos2(r.left(), r.bottom() - 3.0),
        pos2(r.right(), r.bottom() + 2.0),
    );
    let e = ui.interact(edge, id, Sense::drag());
    if e.hovered() || e.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    if e.dragged() {
        let h = (height_of(view, track) + e.drag_delta().y).clamp(MIN_BUS_H, super::MAX_TRACK_H);
        view.bus_heights.insert(track, h);
    }
}

/// A bus track's menu: its envelopes, its effects, Minimize and Maximize.
fn header_menu(
    ui: &mut egui::Ui,
    view: &mut TimelineView,
    app: &mut BananaApp,
    track: BusTrack,
    (fx, target): (&str, FxTarget),
    area: f32,
) {
    let owner = track.owner();
    sub(ui, tr("Insert/Remove Envelope"), true, |ui| {
        // In 5.1, a bus's or a chain's Surround Pan Keyframes.
        if let BusTrack::Strip(s) = track
            && app
                .project()
                .surround_pans(bv_timeline::SurroundOwner::Strip(s))
        {
            let owner = bv_timeline::SurroundOwner::Strip(s);
            let on = app.project().surround(owner).is_some_and(|s| s.keyframes);
            if toggle_item(ui, tr("Surround Pan Keyframes"), on) {
                crate::tabs::surround::toggle_keyframes(app, owner);
                view.show_surround_keys = true;
            }
            ui.separator();
        }
        for (kind, label) in track.kinds() {
            let has = super::envelopes::envelope_of(app.project(), owner, kind).is_some();
            if toggle_item(ui, label, has) {
                let curve = app.settings.prefs.new_envelope_curve(kind);
                let added = app.edit(|p| track.toggle(p, kind, curve));
                if added == Some(true) {
                    view.show_envelopes = true;
                    match kind {
                        EnvelopeKind::Mute => view.show_env_mute = true,
                        EnvelopeKind::Pan => view.show_env_pan = true,
                        EnvelopeKind::FadeToColor => view.show_env_fade = true,
                        EnvelopeKind::MotionBlur => view.show_env_motion_blur = true,
                        EnvelopeKind::Supersampling => view.show_env_supersampling = true,
                        _ => view.show_env_volume = true,
                    }
                }
            }
        }
    });
    if ui.button(fx).clicked() {
        app.clip_windows.fx_target = Some(target);
        ui.close();
    }
    // Activate Automation Controls and Automation Settings, as a track's.
    {
        let (owner, auto) = match track {
            BusTrack::Strip(s) => (
                AutoOwner::Strip(s),
                app.project()
                    .strip_channel(s)
                    .map(|c| c.automation)
                    .unwrap_or_default(),
            ),
            BusTrack::Video => (AutoOwner::VideoBus, app.project().video_bus.automation),
        };
        ui.separator();
        if toggle_item(ui, tr("Activate Automation Controls"), auto.controls) {
            app.toggle_owner_automation_controls(owner);
        }
        sub(ui, tr("Automation Settings"), true, |ui| {
            for m in bv_timeline::AutomationMode::ALL {
                if toggle_item(ui, crate::commands::automation_mode_name(m), auto.mode == m) {
                    app.set_owner_automation_mode(owner, m);
                }
            }
        });
    }
    ui.separator();
    // The video bus's fade colors, as a video track's.
    if track == BusTrack::Video {
        sub(ui, tr("Fade Colors"), true, |ui| {
            super::shortcut_menus::bus_fade_color_items(ui, app)
        });
    }
    if ui.button(tr("Minimize")).clicked() {
        if view.bus_heights.get(&track) == Some(&MIN_BUS_H) {
            view.bus_heights.remove(&track);
        } else {
            view.bus_heights.insert(track, MIN_BUS_H);
        }
        ui.close();
    }
    if ui.button(tr("Maximize")).clicked() {
        let full = area.max(MIN_BUS_H);
        if view.bus_heights.get(&track) == Some(&full) {
            view.bus_heights.remove(&track);
        } else {
            view.bus_heights.insert(track, full);
        }
        ui.close();
    }
}

/// A bus track's lane: its envelopes, drawn and edited.
fn lane(
    view: &mut TimelineView,
    ui: &egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
    project: &Project,
    row: &BusRow,
) {
    let r = row.body(l.bus_lanes.left(), l.bus_lanes.right());
    if !r.intersects(l.bus_lanes) {
        return;
    }
    let track = row.track;
    let (envelopes, muted, color) = match track {
        BusTrack::Video => (
            &project.video_bus.envelopes,
            project.video_bus.muted,
            theme::track_video(),
        ),
        BusTrack::Strip(s) => match project.strip_channel(s) {
            Some(ch) => (&ch.envelopes, ch.muted, theme::track_audio()),
            None => return,
        },
    };
    let painter = ui.painter().with_clip_rect(l.bus_lanes);
    painter.rect_filled(
        r.shrink2(vec2(0.0, 0.5)),
        0.0,
        if muted { theme::panel_dark() } else { color },
    );
    super::lanes::paint_grid(&painter, view, l, app, r.y_range());
    painter.hline(
        r.x_range(),
        r.bottom() - 0.5,
        Stroke::new(1.0, theme::panel_dark()),
    );
    let env = r.shrink2(vec2(0.0, 4.0));
    super::envelopes::paint_strip(view, &painter, l, env, envelopes, &app.settings.prefs);

    // Editing.
    let resp = ui.interact(
        r.intersect(l.bus_lanes),
        egui::Id::new(("bus-lane", track)),
        Sense::click_and_drag(),
    );
    let owner = track.owner();
    let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
    let hit = pointer.and_then(|p| super::envelopes::strip_hit(view, l, env, envelopes, p));
    if resp.drag_started()
        && let Some(h) = hit
    {
        let key = egui::Id::new(("bus-envelope-drag", track, ui.input(|i| i.time).to_bits()));
        let point = match h.point {
            Some(i) => Some(i),
            // Dragging the line: a point where it was taken.
            None => app
                .modify_merged(key, |p| p.set_envelope_point(owner, h.kind, h.at, h.value))
                .ok(),
        };
        view.bus_drag = point.map(|point| BusDrag {
            track,
            kind: h.kind,
            point,
            key,
        });
    }
    if let (Some(d), Some(p)) = (view.bus_drag, pointer)
        && d.track == track
    {
        if resp.dragged() {
            let at = view.time_at(l, p.x).max(Tick::ZERO);
            let value = super::envelopes::value_at_y(env, d.kind, p.y);
            let _ = app.modify_merged(d.key, |pr| {
                pr.move_envelope_point(owner, d.kind, d.point, at, value)
            });
            // Where the point went (it stays between its neighbours).
            let (at, value) = super::envelopes::envelope_of(app.project(), owner, d.kind)
                .and_then(|e| e.points.get(d.point))
                .map_or((at, value), |pt| (pt.at, pt.value));
            let spot = pos2(view.x_of(l, at), super::envelopes::y_of(env, d.kind, value));
            let time = app.ruler_text(at);
            paint_value(ui, l, project, p, owner, (d.kind, &time, value), Some(spot));
        }
        if resp.drag_stopped() {
            view.bus_drag = None;
        }
    } else if let (Some(h), Some(p)) = (hit, pointer) {
        ui.ctx().set_cursor_icon(if h.point.is_some() {
            egui::CursorIcon::Move
        } else {
            egui::CursorIcon::PointingHand
        });
        let spot = h.point.map(|_| {
            pos2(
                view.x_of(l, h.at),
                super::envelopes::y_of(env, h.kind, h.value),
            )
        });
        let time = app.ruler_text(h.at);
        paint_value(ui, l, project, p, owner, (h.kind, &time, h.value), spot);
    }
    // A double-click on the line: a point there.
    if resp.double_clicked()
        && let Some(h) = hit.filter(|h| h.point.is_none())
    {
        app.edit(|p| p.set_envelope_point(owner, h.kind, h.at, h.value));
    }
    if resp.secondary_clicked() {
        view.bus_point = hit.map(|hit| BusPoint { track, hit });
    }
    if view.bus_point.is_some_and(|b| b.track == track) {
        resp.context_menu(|ui| envelope_menu(ui, view, app));
    }
}

/// A bus track's envelope menu, as a track's: the point's curve, Delete
/// Point (not the first) or Add Point, Thin and Flip (all of it, or within
/// the time selection), Reset All Points and Remove Envelope.
fn envelope_menu(ui: &mut egui::Ui, view: &mut TimelineView, app: &mut BananaApp) {
    let Some(BusPoint { track, hit: h }) = view.bus_point else {
        ui.close();
        return;
    };
    let (owner, kind) = (track.owner(), h.kind);
    ui.label(
        egui::RichText::new(trf!(
            "Envelope: {}",
            super::envelopes::label(app.project(), owner, kind)
        ))
        .color(super::envelopes::color(&app.settings.prefs, kind)),
    );
    ui.separator();
    match h.point {
        Some(i) => {
            let curve = super::envelopes::envelope_of(app.project(), owner, kind)
                .and_then(|e| e.points.get(i))
                .map(|p| p.curve);
            if kind != EnvelopeKind::Mute {
                for c in KeyCurve::ALL {
                    if crate::widgets::radio(ui, curve == Some(c), tr(c.name())).clicked() {
                        app.edit(|p| p.set_envelope_curve(owner, kind, i, c));
                        ui.close();
                    }
                }
                ui.separator();
            }
            if ui
                .add_enabled(i > 0, egui::Button::new(tr("Delete Point")))
                .clicked()
            {
                app.edit(|p| p.remove_envelope_point(owner, kind, i));
                ui.close();
            }
        }
        None => {
            if ui.button(tr("Add Point")).clicked() {
                app.edit(|p| p.set_envelope_point(owner, kind, h.at, h.value));
                ui.close();
            }
        }
    }
    ui.separator();
    let sel = app
        .project()
        .loop_region
        .filter(|r| r.duration > Tick::ZERO);
    if ui.button(tr("Thin All Points")).clicked() {
        app.edit(|p| p.thin_envelope(owner, kind, None));
        ui.close();
    }
    if ui
        .add_enabled(sel.is_some(), egui::Button::new(tr("Thin Selected Points")))
        .clicked()
    {
        app.edit(|p| p.thin_envelope(owner, kind, sel));
        ui.close();
    }
    if kind.flips() {
        if ui.button(tr("Flip All Points")).clicked() {
            app.edit(|p| p.flip_envelope(owner, kind, None));
            ui.close();
        }
        if ui
            .add_enabled(sel.is_some(), egui::Button::new(tr("Flip Selected Points")))
            .clicked()
        {
            app.edit(|p| p.flip_envelope(owner, kind, sel));
            ui.close();
        }
    }
    ui.separator();
    let curve = app.settings.prefs.new_envelope_curve(kind);
    if ui.button(tr("Reset All Points")).clicked() {
        app.edit(|p| -> Result<(), bv_timeline::EditError> {
            track.toggle(p, kind, curve)?;
            track.toggle(p, kind, curve)?;
            Ok(())
        });
        ui.close();
    }
    if ui.button(tr("Remove Envelope")).clicked() {
        app.edit(|p| track.toggle(p, kind, curve));
        view.bus_point = None;
        ui.close();
    }
}

/// The tooltip of the envelope `kind` under the pointer `at` (its time
/// and value there), over everything; the point at `spot` a white diamond.
fn paint_value(
    ui: &egui::Ui,
    l: &Layout,
    project: &Project,
    at: egui::Pos2,
    owner: bv_timeline::EnvelopeOwner,
    (kind, time, value): (EnvelopeKind, &str, f32),
    spot: Option<egui::Pos2>,
) {
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("bus-envelope-value"),
    ));
    if let Some(spot) = spot {
        super::envelopes::paint_hot_point(&painter.with_clip_rect(l.bus_lanes), spot);
    }
    super::envelopes::paint_value(&painter, project, at, owner, kind, time, value);
}

/// The edit cursor and the playback position through the bus tracks.
fn needles(ui: &egui::Ui, app: &BananaApp, view: &TimelineView, l: &Layout) {
    let painter = ui.painter().with_clip_rect(l.bus_lanes);
    let x = view.x_of(l, app.cursor);
    match app.playhead {
        Some(play) => {
            painter.vline(x, l.bus_lanes.y_range(), Stroke::new(1.5, theme::banana()));
            let x = view.x_of(l, play);
            painter.vline(x, l.bus_lanes.y_range(), Stroke::new(1.0, theme::cursor()));
        }
        None => {
            painter.vline(x, l.bus_lanes.y_range(), Stroke::new(1.0, theme::cursor()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ctrl+Shift+Up/Down with the bus area in focus size every bus track,
    /// within their limits.
    #[test]
    fn bus_tracks_are_sized_together() {
        let mut p = Project::new(bv_timeline::ProjectSettings::default());
        let a = p.add_bus().unwrap();
        let fx = p.add_fx_chain(Vec::new()).unwrap();
        let mut view = TimelineView::default();
        view.scale_bus_heights(&p, 1.25);
        for t in [
            BusTrack::Strip(Strip::Bus(a)),
            BusTrack::Strip(Strip::Chain(fx)),
        ] {
            assert_eq!(height_of(&view, t), BUS_H * 1.25);
        }
        for _ in 0..20 {
            view.scale_bus_heights(&p, 0.8);
        }
        assert_eq!(height_of(&view, BusTrack::Strip(Strip::Bus(a))), MIN_BUS_H);
    }

    /// View › Audio Bus Tracks: a row for each bus and chain under a
    /// splitter at the bottom, the tracks above it; each row its own
    /// height; the area never takes the tracks' last room.
    #[test]
    fn bus_tracks_take_the_timeline_bottom() {
        let mut p = Project::new(bv_timeline::ProjectSettings::default());
        p.add_bus();
        p.add_bus();
        p.add_fx_chain(Vec::new());
        let mut view = TimelineView::default();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 700.0));
        let hidden = view.layout(rect, &p);
        assert!(hidden.bus_rows.is_empty());
        assert_eq!(hidden.bus_split.height(), 0.0);
        view.show_bus_tracks = true;
        let shown = view.layout(rect, &p);
        assert_eq!(shown.bus_rows.len(), 3);
        assert!(shown.lanes.bottom() < hidden.lanes.bottom());
        assert!((shown.bus_split.top() - shown.lanes.bottom()).abs() < 1e-3);
        assert!((shown.bus_lanes.bottom() - hidden.lanes.bottom()).abs() < 1e-3);
        assert!((shown.bus_rows[0].top - shown.bus_lanes.top()).abs() < 1e-3);
        // A row's own height.
        view.bus_heights
            .insert(BusTrack::Strip(Strip::Bus(p.busses[0].id)), MIN_BUS_H);
        let own = view.layout(rect, &p);
        assert!((own.bus_rows[1].top - own.bus_rows[0].top - MIN_BUS_H).abs() < 1e-3);
        // A huge area leaves the tracks their room.
        view.bus_area = 5000.0;
        let huge = view.layout(rect, &p);
        assert!(huge.lanes.height() >= 39.0, "{}", huge.lanes.height());
        // The video bus track, first.
        view.bus_area = AREA;
        view.show_video_bus = true;
        let video = view.layout(rect, &p);
        assert_eq!(video.bus_rows.len(), 4);
        assert_eq!(video.bus_rows[0].track, BusTrack::Video);
        view.show_bus_tracks = false;
        assert_eq!(view.layout(rect, &p).bus_rows.len(), 1);
    }
}
