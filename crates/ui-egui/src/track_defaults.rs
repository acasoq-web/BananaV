//! A track header's Set Default Track Properties, as VEGAS's Help describes
//! it: check boxes for the settings of the track it was opened on that new
//! tracks of its kind start with — Volume (audio), Composite Mode (video),
//! Pan Type (audio), Height, Track FX, Input monitor (audio) and
//! Automation mode — and Restore original
//! defaults. The defaults are `UiSettings::track_defaults`; the tracks an
//! edit makes get them (`apply_track_defaults`, each frame, as part of
//! that edit's undo; undo, redo and opening a project give none).

use bv_timeline::{TrackId, TrackKind};

use crate::app::BananaApp;
use crate::egui::{self, RichText};
use crate::i18n::tr;
use crate::settings::KindDefaults;
use crate::theme;

/// The dialog's items, in VEGAS's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Item {
    Volume,
    CompositeMode,
    PanType,
    Height,
    TrackFx,
    InputMonitor,
    Automation,
}

impl Item {
    pub const ALL: [Item; 7] = [
        Item::Volume,
        Item::CompositeMode,
        Item::PanType,
        Item::Height,
        Item::TrackFx,
        Item::InputMonitor,
        Item::Automation,
    ];

    fn label(self) -> &'static str {
        match self {
            Item::Volume => tr("Volume"),
            Item::CompositeMode => tr("Composite Mode"),
            Item::PanType => tr("Pan Type"),
            Item::Height => tr("Height"),
            Item::TrackFx => tr("Track FX"),
            Item::InputMonitor => tr("Input monitor"),
            Item::Automation => tr("Automation mode"),
        }
    }

    /// The kinds of track it is for.
    fn applies(self, kind: TrackKind) -> bool {
        match self {
            Item::Volume | Item::PanType | Item::InputMonitor => kind == TrackKind::Audio,
            Item::CompositeMode => kind == TrackKind::Video,
            Item::Height | Item::TrackFx | Item::Automation => true,
        }
    }

    /// Whether BananaV has it (all of them now).
    fn available(self) -> bool {
        true
    }

    /// Whether `d` sets it.
    fn set_in(self, d: &KindDefaults) -> bool {
        match self {
            Item::Volume => d.volume.is_some(),
            Item::CompositeMode => d.composite_mode.is_some(),
            Item::PanType => d.pan_mode.is_some(),
            Item::Height => d.height.is_some(),
            Item::TrackFx => d.effects.is_some(),
            Item::InputMonitor => d.input_monitor.is_some(),
            Item::Automation => d.automation.is_some(),
        }
    }
}

/// The dialog: the track it takes the settings from, its kind, the boxes
/// ticked and Restore original defaults.
#[derive(Clone, Debug)]
pub(crate) struct DefaultsDialog {
    track: TrackId,
    kind: TrackKind,
    checks: [bool; 7],
    restore: bool,
}

/// The history as it was last frame: its edits and moves and the id
/// counter then (tracks with ids past it are new).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DefaultsWatch {
    edits: u64,
    moves: u64,
    counter: u64,
}

impl BananaApp {
    /// Opens Set Default Track Properties on track `track` (each box ticked
    /// where its kind's default is set now).
    pub(crate) fn open_track_defaults(&mut self, track: TrackId) {
        let Some(kind) = self.project().track(track).map(|t| t.kind) else {
            return;
        };
        let d = self.settings.track_defaults.of(kind);
        let mut checks = [false; 7];
        for (i, item) in Item::ALL.iter().enumerate() {
            checks[i] = item.set_in(d);
        }
        self.track_defaults_dialog = Some(DefaultsDialog {
            track,
            kind,
            checks,
            restore: false,
        });
    }

    /// The dialog, while it is open.
    pub(crate) fn track_defaults_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.track_defaults_dialog.clone() else {
            return;
        };
        if self.project().track(d.track).is_none() {
            self.track_defaults_dialog = None;
            return;
        }
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        crate::floating::Window::new(tr("Set Default Track Properties"))
            .id(egui::Id::new("track-defaults"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_min_width(340.0);
                ui.label(tr(
                    "Use this track's settings as the defaults for new tracks:",
                ));
                ui.add_space(4.0);
                for (i, item) in Item::ALL.iter().enumerate() {
                    let enabled = item.applies(d.kind) && item.available() && !d.restore;
                    ui.add_enabled_ui(enabled, |ui| {
                        let resp = crate::widgets::checkbox(ui, &mut d.checks[i], item.label());
                        if !item.available() {
                            resp.on_disabled_hover_text(tr("Coming soon to BananaV"));
                        }
                    });
                }
                ui.add_space(6.0);
                crate::widgets::checkbox(ui, &mut d.restore, tr("Restore original defaults"));
                if d.restore {
                    ui.label(
                        RichText::new(tr(
                            "OK gives new audio and video tracks BananaV's own defaults again.",
                        ))
                        .color(theme::text_dim()),
                    );
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ok = ui.button(tr("OK")).clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if ok {
            self.set_track_defaults(&d);
        }
        self.track_defaults_dialog = (open && !ok && !cancel).then_some(d);
    }

    /// The dialog's OK: the ticked settings of its track become its kind's
    /// defaults, the others BananaV's own; or every default restored.
    pub(crate) fn set_track_defaults(&mut self, d: &DefaultsDialog) {
        if d.restore {
            self.settings.track_defaults = Default::default();
            return;
        }
        let Some(t) = self.project().track(d.track).cloned() else {
            return;
        };
        let height = self.timeline.height_of(&t);
        let on = |item: Item| {
            Item::ALL
                .iter()
                .position(|i| *i == item)
                .is_some_and(|i| d.checks[i] && item.applies(d.kind) && item.available())
        };
        *self.settings.track_defaults.of_mut(d.kind) = KindDefaults {
            volume: on(Item::Volume).then_some(t.level),
            composite_mode: on(Item::CompositeMode).then_some(t.composite_mode),
            pan_mode: on(Item::PanType).then_some(t.pan_mode),
            height: on(Item::Height).then_some(height),
            effects: on(Item::TrackFx).then(|| t.effects.clone()),
            input_monitor: on(Item::InputMonitor).then_some(t.input_monitor),
            automation: on(Item::Automation).then_some(t.automation),
        };
    }

    /// Each frame: the tracks the last edits made get the defaults of
    /// their kind (in those edits' undo); undo, redo and opening a project
    /// give none.
    pub(crate) fn apply_track_defaults(&mut self) {
        let (edits, moves) = (self.history.edits(), self.history.moves());
        let counter = self.project().id_counter();
        let was = self.defaults_watch;
        self.defaults_watch = DefaultsWatch {
            edits,
            moves,
            counter,
        };
        if moves != was.moves || edits == was.edits {
            return;
        }
        let defaults = self.settings.track_defaults.clone();
        if defaults.is_empty() {
            return;
        }
        let new: Vec<(TrackId, TrackKind)> = self
            .project()
            .tracks
            .iter()
            .filter(|t| t.id.0 >= was.counter)
            .map(|t| (t.id, t.kind))
            .collect();
        if new.is_empty() {
            return;
        }
        self.history.amend(|p| {
            for &(id, kind) in &new {
                if let Some(t) = p.track_by_id_mut(id) {
                    defaults.of(kind).apply(t);
                }
            }
        });
        for &(id, kind) in &new {
            if let Some(h) = defaults.of(kind).height {
                self.timeline.set_track_height(id, h);
            }
        }
    }

    /// Set Default Track Properties open on the demo's audio track, Volume
    /// and Track FX ticked (screen tests; the settings in memory).
    pub fn track_defaults_for_review(&mut self) -> bool {
        self.settings = crate::settings::UiSettings::in_memory();
        let Some(audio) = self
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
        else {
            return false;
        };
        self.open_track_defaults(audio);
        if let Some(d) = &mut self.track_defaults_dialog {
            d.checks[0] = true;
            d.checks[4] = true;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::{Effect, EffectKind, PanMode};

    /// OK keeps the ticked settings of the track for its kind; a new track
    /// of that kind gets them in the edit that made it (one undo), a
    /// track brought back by undo does not, and Restore original defaults
    /// clears them.
    #[test]
    fn new_tracks_take_the_defaults() {
        let mut app = BananaApp::new(egui::Context::default());
        let a = app.modify(|p| p.add_track(TrackKind::Audio));
        app.modify(|p| {
            let t = p.track_by_id_mut(a).unwrap();
            t.level = 0.5;
            t.pan_mode = PanMode::ConstantPower;
            t.effects.push(Effect::new(EffectKind::Reverb));
        });
        app.apply_track_defaults();
        app.open_track_defaults(a);
        let mut d = app.track_defaults_dialog.clone().unwrap();
        d.checks = [true, true, false, false, true, true, false];
        app.set_track_defaults(&d);
        let defaults = app.settings.track_defaults.audio.clone();
        assert_eq!(defaults.volume, Some(0.5));
        assert_eq!(defaults.pan_mode, None, "not ticked");
        assert_eq!(defaults.composite_mode, None, "video only");
        assert_eq!(
            defaults.input_monitor,
            Some(bv_timeline::InputMonitor::Off),
            "ticked"
        );
        assert_eq!(defaults.effects.as_ref().map(Vec::len), Some(1));
        // A new audio track takes them; a video track not (a frame after
        // each edit).
        let b = app.modify(|p| p.add_track(TrackKind::Audio));
        app.apply_track_defaults();
        let v = app.modify(|p| p.add_track(TrackKind::Video));
        app.apply_track_defaults();
        let t = app.project().track(b).unwrap();
        assert_eq!(t.level, 0.5);
        assert_eq!(t.effects.len(), 1);
        assert_eq!(t.pan_mode, PanMode::default());
        assert!(app.project().track(v).unwrap().effects.is_empty());
        // Undo takes the track with its defaults away in one step.
        app.history.undo();
        app.apply_track_defaults();
        assert!(app.project().track(v).is_none());
        assert!(app.project().track(b).is_some());
        app.history.undo();
        app.apply_track_defaults();
        assert!(app.project().track(b).is_none());
        // Redo brings it back as it was: not given them twice.
        app.history.redo();
        app.apply_track_defaults();
        assert_eq!(app.project().track(b).unwrap().effects.len(), 1);
        // Restore original defaults.
        d.restore = true;
        app.set_track_defaults(&d);
        assert!(app.settings.track_defaults.is_empty());
    }
}
