//! Beat Detection and Tempo Detection, as VEGAS's: a media's beats become
//! its markers — the Trimmer shows them while Sensitivity is set (the
//! higher, the fewer), OK keeps them, and the events using the media show
//! them and snap to them — and its tempo becomes the project's, with the
//! ruler in measures and beats, the metronome and snapping to the grid as
//! asked. The engine reads the sound in the background
//! (`Engine::start_beat_detection`, `bv_mixer::beats`).

use bv_core::Tick;
use bv_timeline::{MediaId, MediaMarker};

use crate::app::BananaApp;
use crate::dock::Tab;
use crate::egui::{self, RichText};
use crate::i18n::tr;
use crate::theme;
use crate::timefmt::RulerFormat;

/// Beat Detection's dialog: its media and its sensitivity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BeatDialog {
    pub media: MediaId,
    /// 0–100: the higher, the fewer markers.
    pub sensitivity: f64,
}

/// Tempo Detection's dialog: its media and what OK does with the tempo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TempoDialog {
    pub media: MediaId,
    pub set_tempo: bool,
    pub ruler: bool,
    pub metronome: bool,
    pub snap: bool,
}

/// The sensitivity Beat Detection starts at.
pub(crate) const SENSITIVITY: f64 = 50.0;

/// A beat this near a marker already there adds none.
fn same_place(a: Tick, b: Tick) -> bool {
    (a.0 - b.0).abs() < Tick::from_seconds_f64(0.005).0
}

impl BananaApp {
    /// Beat Detection on `media` (an event's menu, Project Media's, the
    /// Trimmer's button): it opens in the Trimmer, its sound read in the
    /// background.
    pub(crate) fn beat_detection(&mut self, media: MediaId) {
        self.engine.set_project(self.history.current());
        if !self.engine.start_beat_detection(media) {
            self.notify(tr("The media has no sound to find beats in."));
            return;
        }
        self.trimmer.open(media);
        self.show_tab(Tab::Trimmer, false);
        self.tempo_dialog = None;
        self.beat_dialog = Some(BeatDialog {
            media,
            sensitivity: SENSITIVITY,
        });
    }

    /// Tempo Detection on `media` (Project Media's menu).
    pub(crate) fn tempo_detection(&mut self, media: MediaId) {
        self.engine.set_project(self.history.current());
        if !self.engine.start_beat_detection(media) {
            self.notify(tr("The media has no sound to find a tempo in."));
            return;
        }
        self.beat_dialog = None;
        self.tempo_dialog = Some(TempoDialog {
            media,
            set_tempo: true,
            ruler: true,
            metronome: false,
            snap: true,
        });
    }

    /// The markers Beat Detection would give `media` now: while its dialog
    /// is open and the sound has been read (the Trimmer shows them).
    pub(crate) fn beat_preview(&self, media: MediaId) -> Option<Vec<Tick>> {
        let d = self.beat_dialog.filter(|d| d.media == media)?;
        let p = self
            .engine
            .beat_progress()
            .filter(|p| p.finished && p.media == Some(media))?;
        Some(
            bv_engine::beats_kept(&p.beats, d.sensitivity)
                .iter()
                .map(|b| Tick::from_seconds_f64(b.time))
                .collect(),
        )
    }

    /// Beat Detection's OK: its markers become the media's (none twice).
    pub(crate) fn keep_beats(&mut self) {
        let Some(d) = self.beat_dialog else {
            return;
        };
        let times = self.beat_preview(d.media).unwrap_or_default();
        let mut added = 0usize;
        self.edit_media_markers(d.media, None, false, |m| {
            for &t in &times {
                if !m.iter().any(|x| same_place(x.time, t)) {
                    m.push(MediaMarker {
                        time: t,
                        ..MediaMarker::default()
                    });
                    added += 1;
                }
            }
        });
        self.notify(trn!(
            added,
            "{} beat marker added.",
            "{} beat markers added.",
            added
        ));
        self.close_beat_dialogs();
    }

    /// Tempo Detection's OK: the tempo found is the project's, as asked.
    pub(crate) fn apply_tempo(&mut self) {
        let Some(d) = self.tempo_dialog else {
            return;
        };
        let tempo = self
            .engine
            .beat_progress()
            .filter(|p| p.finished && p.media == Some(d.media))
            .and_then(|p| p.tempo)
            .map(|t| (t * 100.0).round() / 100.0);
        let set_tempo = d.set_tempo && tempo.is_some();
        if set_tempo || d.ruler {
            self.modify(|p| {
                if set_tempo && let Some(t) = tempo {
                    p.settings.tempo.beats_per_minute = t;
                }
                if d.ruler {
                    p.settings.ruler_format = Some(RulerFormat::MeasuresBeats);
                }
            });
        }
        if d.metronome && !self.metronome {
            self.set_metronome(true);
        }
        if d.snap {
            self.snapping = true;
            self.snap_grid = true;
        }
        self.close_beat_dialogs();
    }

    /// Both dialogs closed, the reading stopped.
    pub(crate) fn close_beat_dialogs(&mut self) {
        self.engine.cancel_beat_detection();
        self.beat_dialog = None;
        self.tempo_dialog = None;
    }

    /// The dialogs of Beat Detection and Tempo Detection, while one is
    /// open.
    pub(crate) fn beat_dialogs(&mut self, ctx: &egui::Context) {
        if self.beat_dialog.is_some() {
            self.beat_window(ctx);
        }
        if self.tempo_dialog.is_some() {
            self.tempo_window(ctx);
        }
    }

    fn beat_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.beat_dialog else {
            return;
        };
        let progress = self.engine.beat_progress();
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        crate::floating::Window::new(tr("Beat Detection"))
            .id(egui::Id::new("beat-detection"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                let ready = reading(ui, progress.as_ref(), tr("Finding the beats…"));
                if let Some(p) = progress.as_ref().filter(|_| ready) {
                    ui.horizontal(|ui| {
                        ui.label(tr("Sensitivity:"));
                        ui.add(
                            egui::Slider::new(&mut d.sensitivity, 0.0..=100.0).fixed_decimals(0),
                        );
                    });
                    let n = bv_engine::beats_kept(&p.beats, d.sensitivity).len();
                    ui.label(trn!(n, "{} marker", "{} markers", n));
                    ui.label(
                        RichText::new(tr("Raise the sensitivity for fewer markers."))
                            .color(theme::text_dim()),
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ok = ui.add_enabled(ready, egui::Button::new(tr("OK"))).clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        self.beat_dialog = Some(d);
        if ok {
            self.keep_beats();
        } else if cancel || !open {
            self.close_beat_dialogs();
        }
    }

    fn tempo_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.tempo_dialog else {
            return;
        };
        let progress = self.engine.beat_progress();
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        crate::floating::Window::new(tr("Tempo Detection"))
            .id(egui::Id::new("tempo-detection"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                let ready = reading(ui, progress.as_ref(), tr("Finding the tempo…"));
                if let Some(p) = progress.as_ref().filter(|_| ready) {
                    match p.tempo {
                        Some(t) => {
                            ui.horizontal(|ui| {
                                ui.label(tr("Detected tempo:"));
                                ui.label(
                                    RichText::new(trf!("{} BPM", crate::fmt::num(t, 2))).strong(),
                                );
                            });
                        }
                        None => {
                            ui.label(tr("No steady tempo was found."));
                        }
                    }
                    ui.add_space(4.0);
                    ui.add_enabled(
                        p.tempo.is_some(),
                        egui::Checkbox::new(&mut d.set_tempo, tr("Set as project tempo")),
                    );
                    ui.checkbox(
                        &mut d.ruler,
                        tr("Set ruler time format to measures and beats"),
                    );
                    ui.checkbox(&mut d.metronome, tr("Activate metronome"));
                    ui.checkbox(&mut d.snap, tr("Enable snapping to grid"));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ok = ui.add_enabled(ready, egui::Button::new(tr("OK"))).clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        self.tempo_dialog = Some(d);
        if ok {
            self.apply_tempo();
        } else if cancel || !open {
            self.close_beat_dialogs();
        }
    }
}

/// While the sound is read: `what` and how far; when it could not be, why.
/// Whether it was read.
fn reading(ui: &mut egui::Ui, p: Option<&bv_engine::BeatProgress>, what: &str) -> bool {
    match p {
        Some(p) if p.finished => match &p.error {
            Some(e) => {
                ui.label(trf!("The sound could not be read: {}", e));
                false
            }
            None => true,
        },
        Some(p) => {
            ui.label(what);
            ui.add(
                egui::ProgressBar::new((p.done / p.total.max(1e-9)) as f32)
                    .desired_width(300.0)
                    .show_percentage(),
            );
            ui.ctx().request_repaint();
            false
        }
        None => false,
    }
}
