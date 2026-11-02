//! Tools › Audio › Apply Non-Real-Time Event FX, as VEGAS's: the sound an
//! audio event plays rendered through audio effects into a new file, which
//! joins the project and becomes the event's new take. BananaV renders the
//! event's own audio effects (set in its Audio Event FX window) and, as
//! asked, takes them off the event once the new take has them. The engine
//! renders in the background (`Engine::start_nonrealtime_fx`).

use std::path::{Path, PathBuf};

use bv_engine::NrtFormat;
use bv_timeline::{ClipId, Effect, MediaSource, TrackKind};

use crate::app::BananaApp;
use crate::egui::{self, RichText};
use crate::i18n::tr;
use crate::theme;

/// The dialog: the event, the file, its samples, whether the effects come
/// off the event, and whether it is rendering.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NrtDialog {
    pub clip: ClipId,
    pub path: String,
    pub format: NrtFormat,
    pub remove_fx: bool,
    pub rendering: bool,
}

/// `<stem> (FX).wav` in `folder`, numbered past the files already there.
fn free_path(folder: &Path, stem: &str) -> PathBuf {
    let name = |n: u32| {
        if n < 2 {
            format!("{stem} (FX).wav")
        } else {
            format!("{stem} (FX {n}).wav")
        }
    };
    (1..1000)
        .map(|n| folder.join(name(n)))
        .find(|p| !p.exists())
        .unwrap_or_else(|| folder.join(name(1)))
}

/// The audio effects of event `clip`.
fn audio_effects(app: &BananaApp, clip: ClipId) -> Vec<Effect> {
    app.project()
        .clip(clip)
        .map(|c| {
            c.effects
                .iter()
                .filter(|e| e.kind.is_audio())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

impl BananaApp {
    /// Tools › Audio › Apply Non-Real-Time Event FX: its dialog, for the
    /// selected audio event.
    pub(crate) fn open_nonrealtime_fx(&mut self) {
        let Some(clip) = self.first_selected(TrackKind::Audio) else {
            return;
        };
        let p = self.project();
        let Some(c) = p.clip(clip) else {
            return;
        };
        if c.reverse.is_some() || c.freeze.is_some() {
            self.notify(tr(
                "Non-real-time effects render events that play forward: this one is reversed or frozen.",
            ));
            return;
        }
        let media = p.media(c.media);
        let stem = media.map_or_else(
            || "Audio".to_string(),
            |m| {
                Path::new(&m.name)
                    .file_stem()
                    .map_or_else(|| m.name.clone(), |s| s.to_string_lossy().into_owned())
            },
        );
        // Beside the media's file, else the project's, else in the user's
        // music.
        let folder = media
            .and_then(|m| match &m.source {
                MediaSource::File { path } => Path::new(path).parent().map(Path::to_path_buf),
                _ => None,
            })
            .or_else(|| {
                self.project_path
                    .as_ref()
                    .and_then(|p| p.parent().map(Path::to_path_buf))
            })
            .or_else(|| {
                bv_platform::home_dir().map(|h| {
                    let music = h.join("Music");
                    if music.is_dir() { music } else { h }
                })
            })
            .unwrap_or_default();
        self.nrt_dialog = Some(NrtDialog {
            clip,
            path: free_path(&folder, &stem).display().to_string(),
            format: NrtFormat::Pcm24,
            remove_fx: true,
            rendering: false,
        });
    }

    /// The dialog's Render: the event's sound through its audio effects
    /// into the dialog's file, in the background.
    pub(crate) fn start_nonrealtime(&mut self) {
        let Some(mut d) = self.nrt_dialog.clone() else {
            return;
        };
        let path = PathBuf::from(d.path.trim());
        if path.exists() {
            self.notify(tr("A file of that name is there already: pick another."));
            return;
        }
        let effects = audio_effects(self, d.clip);
        self.engine.set_project(self.history.current());
        if !self
            .engine
            .start_nonrealtime_fx(d.clip, effects, path, d.format)
        {
            self.notify(tr("The event's sound cannot be rendered."));
            return;
        }
        d.rendering = true;
        self.nrt_dialog = Some(d);
    }

    /// Once the rendering ended: the new file joins the project and becomes
    /// the event's take (the effects taken off it, as asked), or what went
    /// wrong is told.
    pub(crate) fn poll_nonrealtime(&mut self) {
        let Some(mut d) = self.nrt_dialog.clone().filter(|d| d.rendering) else {
            return;
        };
        let Some(p) = self.engine.nonrealtime_progress().filter(|p| p.finished) else {
            return;
        };
        self.engine.cancel_nonrealtime_fx();
        if let Some(e) = p.error {
            if e != "canceled" {
                self.notify(trf!("The sound could not be rendered: {}", e));
            }
            d.rendering = false;
            self.nrt_dialog = Some(d);
            return;
        }
        let info = match bv_engine::Engine::probe_file(&p.path.to_string_lossy()) {
            Ok(i) => i,
            Err(e) => {
                self.notify(trf!("The rendered file could not be read: {}", e));
                self.nrt_dialog = None;
                return;
            }
        };
        let (clip, remove) = (d.clip, d.remove_fx);
        self.modify(|p| {
            let m = p.add_media(info);
            if p.add_take(clip, m).is_ok()
                && remove
                && let Some(c) = p.clip_mut(clip)
            {
                c.effects.retain(|e| !e.kind.is_audio());
            }
        });
        self.notify(tr("The processed sound is the event's new take."));
        self.nrt_dialog = None;
    }

    /// The dialog, while it is open.
    pub(crate) fn nonrealtime_window(&mut self, ctx: &egui::Context) {
        self.poll_nonrealtime();
        let Some(mut d) = self.nrt_dialog.clone() else {
            return;
        };
        let Some(name) = self
            .project()
            .clip(d.clip)
            .and_then(|c| self.project().media(c.media))
            .map(crate::i18n::media_name)
        else {
            self.nrt_dialog = None;
            return;
        };
        let effects = audio_effects(self, d.clip);
        let progress = self.engine.nonrealtime_progress();
        let mut open = true;
        let (mut render, mut cancel, mut edit_fx, mut browse) = (false, false, false, false);
        crate::floating::Window::new(tr("Apply Non-Real-Time Event FX"))
            .id(egui::Id::new("nonrealtime-fx"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_min_width(440.0);
                ui.label(trf!("Event: {}", name));
                ui.add_space(4.0);
                ui.label(RichText::new(tr("Effects:")).strong());
                if effects.is_empty() {
                    ui.label(
                        RichText::new(tr(
                            "The event has no audio effects yet: add them in its Audio Event FX window.",
                        ))
                        .color(theme::text_dim()),
                    );
                }
                for e in &effects {
                    let n = tr(e.kind.name());
                    if e.enabled {
                        ui.label(format!("• {n}"));
                    } else {
                        ui.label(
                            RichText::new(trf!("• {} (bypassed)", n)).color(theme::text_dim()),
                        );
                    }
                }
                edit_fx = ui
                    .add_enabled(!d.rendering, egui::Button::new(tr("Audio Event FX…")))
                    .clicked();
                ui.separator();
                ui.add_enabled_ui(!d.rendering, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(tr("File:"));
                        ui.add(egui::TextEdit::singleline(&mut d.path).desired_width(300.0));
                        browse = ui.button(tr("Browse…")).clicked();
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr("Format:"));
                        egui::ComboBox::from_id_salt("nonrealtime-format")
                            .selected_text(tr(d.format.name()))
                            .show_ui(ui, |ui| {
                                for f in NrtFormat::ALL {
                                    ui.selectable_value(&mut d.format, f, tr(f.name()));
                                }
                            });
                    });
                    ui.checkbox(
                        &mut d.remove_fx,
                        tr("Take the effects off the event once its new take has them"),
                    );
                });
                if let Some(p) = progress.as_ref().filter(|_| d.rendering) {
                    ui.add_space(4.0);
                    ui.label(tr("Rendering…"));
                    ui.add(
                        egui::ProgressBar::new((p.done / p.total.max(1e-9)) as f32)
                            .desired_width(420.0)
                            .show_percentage(),
                    );
                    ui.ctx().request_repaint();
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let ready = !d.rendering
                        && effects.iter().any(|e| e.enabled)
                        && !d.path.trim().is_empty();
                    render = ui
                        .add_enabled(ready, egui::Button::new(tr("Render")))
                        .clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if browse
            && let Some(p) = rfd::FileDialog::new()
                .add_filter("WAV", &["wav"])
                .set_file_name(
                    Path::new(&d.path)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                )
                .save_file()
        {
            d.path = p.display().to_string();
        }
        if cancel || !open {
            if d.rendering {
                self.engine.cancel_nonrealtime_fx();
            }
            self.nrt_dialog = None;
            return;
        }
        self.nrt_dialog = Some(d);
        if edit_fx {
            self.clip_windows.clip_fx = self.nrt_dialog.as_ref().map(|d| d.clip);
        }
        if render {
            self.start_nonrealtime();
        }
    }
}
