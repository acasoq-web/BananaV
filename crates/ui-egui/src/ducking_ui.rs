//! Auto Ducking, as VEGAS's: an audio track made a controller (its
//! header's menu: Auto Ducking Controller) lowers the listener tracks (Auto
//! Ducking Listener) while it sounds — by its Floor Reduction, and by its
//! Peak Reduction more as it gets louder. Its events' loudness is read in
//! the background (`Engine::start_ducking_analysis`) when it becomes one,
//! and when an event's menu asks (Re-Analyze Auto Ducking), whose dialog
//! then sets that event's own reductions. Listeners show what they get as
//! a dotted line (`timeline/envelopes.rs`).

use bv_timeline::{ClipId, DuckLevels, Ducking, TrackId};

use crate::app::BananaApp;
use crate::egui::{self, RichText};
use crate::i18n::tr;
use crate::theme;

/// What the Auto Ducking dialog sets: a controller track's reductions, or
/// one of its events' own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DuckTarget {
    Track(TrackId),
    Event(ClipId),
}

impl BananaApp {
    /// Auto Ducking Controller, on or off: on, its events' loudness is read
    /// (those not read yet) and its dialog opens.
    pub(crate) fn toggle_duck_controller(&mut self, track: TrackId) {
        let on = self
            .project()
            .track(track)
            .is_some_and(|t| t.ducking.is_some());
        self.modify(|p| {
            if let Some(t) = p.track_by_id_mut(track) {
                t.ducking = if on { None } else { Some(Ducking::default()) };
            }
        });
        if on {
            if self.duck_dialog == Some(DuckTarget::Track(track)) {
                self.duck_dialog = None;
            }
            return;
        }
        let unread: Vec<ClipId> = self
            .project()
            .track(track)
            .map(|t| {
                t.clips
                    .iter()
                    .filter(|c| c.duck_levels.is_none())
                    .map(|c| c.id)
                    .collect()
            })
            .unwrap_or_default();
        self.analyse_ducking(&unread);
        self.duck_dialog = Some(DuckTarget::Track(track));
    }

    /// Auto Ducking Listener, on or off.
    pub(crate) fn toggle_duck_listener(&mut self, track: TrackId) {
        self.modify(|p| {
            if let Some(t) = p.track_by_id_mut(track) {
                t.duck_listener = !t.duck_listener;
            }
        });
    }

    /// An event's menu's Re-Analyze Auto Ducking: its loudness read again,
    /// its dialog open.
    pub(crate) fn reanalyse_ducking(&mut self, clip: ClipId) {
        self.analyse_ducking(&[clip]);
        self.duck_dialog = Some(DuckTarget::Event(clip));
    }

    fn analyse_ducking(&mut self, clips: &[ClipId]) {
        if clips.is_empty() {
            return;
        }
        self.engine.set_project(self.history.current());
        if !self.engine.start_ducking_analysis(clips) {
            self.notify(tr("The events have no sound to read."));
        }
    }

    /// Once the analysis ended: each event's loudness (its own reductions
    /// kept), or what went wrong.
    pub(crate) fn poll_ducking(&mut self) {
        let Some(p) = self.engine.ducking_progress().filter(|p| p.finished) else {
            return;
        };
        self.engine.cancel_ducking_analysis();
        if !p.results.is_empty() {
            self.modify(|proj| {
                for (id, levels) in &p.results {
                    if let Some(c) = proj.clip_mut(*id) {
                        let own = c.duck_levels.as_ref().and_then(|l| l.own);
                        c.duck_levels = Some(DuckLevels {
                            own,
                            ..levels.clone()
                        });
                    }
                }
            });
        }
        if let Some((_, e)) = p.errors.first() {
            self.notify(trf!("The sound of an event could not be read: {}", e));
        }
    }

    /// The Auto Ducking dialog, while it is open.
    pub(crate) fn ducking_window(&mut self, ctx: &egui::Context) {
        self.poll_ducking();
        let Some(target) = self.duck_dialog else {
            return;
        };
        let p = self.project();
        // Its name, its reductions now, and (for an event) its track's.
        let found = match target {
            DuckTarget::Track(id) => p
                .track(id)
                .and_then(|t| Some((crate::i18n::track_name(t), t.ducking?, None))),
            DuckTarget::Event(id) => p.clip(id).and_then(|c| {
                let track = p.clip_track(id)?.ducking?;
                let own = c.duck_levels.as_ref().and_then(|l| l.own);
                let name = p.media(c.media).map(crate::i18n::media_name)?;
                Some((name, own.unwrap_or(track), Some(own.is_some())))
            }),
        };
        let Some((name, mut d, own)) = found else {
            self.duck_dialog = None;
            return;
        };
        let reading = self.engine.ducking_progress();
        let before = d;
        let mut open = true;
        let (mut close, mut use_track) = (false, false);
        let title = match target {
            DuckTarget::Track(_) => trf!("Auto Ducking: {}", name),
            DuckTarget::Event(_) => trf!("Auto Ducking (event): {}", name),
        };
        crate::floating::Window::new(title)
            .id(egui::Id::new(("auto-ducking", target)))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_min_width(340.0);
                egui::Grid::new("auto-ducking-grid")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(tr("Floor Reduction:"));
                        ui.add(
                            egui::Slider::new(&mut d.floor_db, -60.0..=0.0)
                                .fixed_decimals(1)
                                .suffix(" dB"),
                        )
                        .on_hover_text(tr("How far the listeners go down while this sounds."));
                        ui.end_row();
                        ui.label(tr("Peak Reduction:"));
                        let mut pct = d.peak * 100.0;
                        if ui
                            .add(
                                egui::Slider::new(&mut pct, 0.0..=100.0)
                                    .fixed_decimals(0)
                                    .suffix(" %"),
                            )
                            .on_hover_text(tr("How much further they go down as this gets louder."))
                            .changed()
                        {
                            d.peak = pct / 100.0;
                        }
                        ui.end_row();
                    });
                if own == Some(true) {
                    use_track = ui.button(tr("Use the Track's")).clicked();
                } else if own == Some(false) {
                    ui.label(
                        RichText::new(tr("The event takes its track's until these are changed."))
                            .color(theme::text_dim()),
                    );
                }
                if let Some(r) = reading.as_ref().filter(|r| !r.finished) {
                    ui.add_space(4.0);
                    ui.label(tr("Reading the sound…"));
                    ui.add(
                        egui::ProgressBar::new((r.done / r.total.max(1e-9)) as f32)
                            .desired_width(320.0)
                            .show_percentage(),
                    );
                    ui.ctx().request_repaint();
                }
                ui.add_space(6.0);
                close = ui.button(tr("Close")).clicked();
            });
        if use_track && let DuckTarget::Event(id) = target {
            self.modify(|p| {
                if let Some(l) = p.clip_mut(id).and_then(|c| c.duck_levels.as_mut()) {
                    l.own = None;
                }
            });
        } else if d != before {
            self.modify_merged(
                egui::Id::new(("auto-ducking-edit", target)),
                |p| match target {
                    DuckTarget::Track(id) => {
                        if let Some(t) = p.track_by_id_mut(id) {
                            t.ducking = Some(d);
                        }
                    }
                    DuckTarget::Event(id) => {
                        if let Some(l) = p.clip_mut(id).and_then(|c| c.duck_levels.as_mut()) {
                            l.own = Some(d);
                        }
                    }
                },
            );
        }
        if close || !open {
            self.duck_dialog = None;
        }
    }
}
