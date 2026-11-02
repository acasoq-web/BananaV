//! Takes (Edit › Take, an event's Take submenu), as VEGAS's: Next and
//! Previous Take (T, Shift+T) for the selected events, Choose Active… and
//! Delete… (the Take Chooser, whose Play and Stop preview the take picked
//! through the preview bus), Delete Active, Rename Active, the takes
//! listed to pick one, Add as Takes (media dropped on an event with the
//! right mouse button) and Options › Slip All Takes (the project's
//! `slip_all_takes`, `Project::slip_takes`).

use bv_core::Tick;
use bv_timeline::{ClipId, MediaId};

use crate::app::BananaApp;
use crate::egui;
use crate::i18n::tr;

/// The Take Chooser: event `clip`'s takes, one picked — to make it the
/// active take, or to delete it.
pub(crate) struct TakeChooser {
    clip: ClipId,
    delete: bool,
    picked: usize,
}

/// Rename Active: the active take's name, being typed.
pub(crate) struct TakeRename {
    clip: ClipId,
    take: usize,
    text: String,
    focus: bool,
}

impl BananaApp {
    /// The selected events that have more than one take.
    fn events_with_takes(&self) -> Vec<ClipId> {
        self.selected()
            .into_iter()
            .filter(|&id| self.project().clip(id).is_some_and(|c| c.take_count() > 1))
            .collect()
    }

    pub(crate) fn can_step_takes(&self) -> bool {
        !self.events_with_takes().is_empty()
    }

    /// Next Take (T) or Previous Take (Shift+T, `back`) for the selected
    /// events.
    pub(crate) fn step_takes(&mut self, back: bool) {
        let ids = self.events_with_takes();
        if ids.is_empty() {
            return;
        }
        self.modify(|p| {
            for id in ids {
                let _ = p.step_take(id, back);
            }
        });
    }

    /// Delete Active: each selected event's active take goes, the next one
    /// plays.
    pub(crate) fn delete_active_takes(&mut self) {
        let ids = self.events_with_takes();
        if ids.is_empty() {
            return;
        }
        self.modify(|p| {
            for id in ids {
                if let Some(k) = p.clip(id).map(|c| c.active_take()) {
                    let _ = p.delete_take(id, k);
                }
            }
        });
    }

    /// The event the take dialogs are about: the selected one (the first
    /// picked).
    fn take_event(&self) -> Option<ClipId> {
        self.primary_clip()
    }

    pub(crate) fn can_choose_take(&self) -> bool {
        self.take_event()
            .and_then(|id| self.project().clip(id))
            .is_some_and(|c| c.take_count() > 1)
    }

    pub(crate) fn can_rename_take(&self) -> bool {
        self.take_event().is_some()
    }

    /// Choose Active… — or Delete… (`delete`) —: the Take Chooser.
    pub(crate) fn open_take_chooser(&mut self, delete: bool) {
        let Some(clip) = self.take_event() else {
            return;
        };
        let picked = self.project().clip(clip).map_or(0, |c| c.active_take());
        self.take_chooser = Some(TakeChooser {
            clip,
            delete,
            picked,
        });
    }

    /// Rename Active: a name for the active take.
    pub(crate) fn open_take_rename(&mut self) {
        let Some(clip) = self.take_event() else {
            return;
        };
        let take = self.project().clip(clip).map_or(0, |c| c.active_take());
        let text = self.project().take_name(clip, take).unwrap_or_default();
        self.take_rename = Some(TakeRename {
            clip,
            take,
            text,
            focus: true,
        });
    }

    /// Makes take `k` of event `clip` the active one (the Take submenu's
    /// list).
    pub(crate) fn choose_take(&mut self, clip: ClipId, k: usize) {
        self.modify(|p| {
            let _ = p.set_active_take(clip, k);
        });
    }

    /// Add as Takes: `media` becomes event `clip`'s active take, the take
    /// it had one of its others.
    pub(crate) fn add_take(&mut self, clip: ClipId, media: MediaId) {
        let p = self.project();
        let fits = p
            .clip_track(clip)
            .zip(p.media(media))
            .is_some_and(|(t, m)| m.has(t.kind));
        if !fits {
            self.notify(tr(
                "That media has nothing of the event's kind: it cannot be one of its takes.",
            ));
            return;
        }
        self.modify(|p| {
            let _ = p.add_take(clip, media);
        });
    }

    /// The take dialogs, when open.
    pub(crate) fn take_windows(&mut self, ctx: &egui::Context) {
        if let Some(mut w) = self.take_chooser.take()
            && w.show(ctx, self)
            && self.take_chooser.is_none()
        {
            self.take_chooser = Some(w);
        }
        if let Some(mut w) = self.take_rename.take()
            && w.show(ctx, self)
            && self.take_rename.is_none()
        {
            self.take_rename = Some(w);
        }
    }
}

/// The names of event `clip`'s takes, and which one is active.
pub(crate) fn take_names(app: &BananaApp, clip: ClipId) -> (Vec<String>, usize) {
    let p = app.project();
    let Some(c) = p.clip(clip) else {
        return (Vec::new(), 0);
    };
    let names = (0..c.take_count())
        .map(|k| {
            p.take_name(clip, k)
                .unwrap_or_else(|| tr("Media").to_string())
        })
        .collect();
    (names, c.active_take())
}

impl TakeChooser {
    /// Shows the dialog; whether it stays open.
    fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let p = app.project();
        let Some(clip) = p.clip(self.clip) else {
            return false;
        };
        let rate = p.settings.frame_rate;
        let list = clip.take_list();
        // Where each take's media starts playing (the active one's own
        // start, the others' offsets from it).
        let starts: Vec<(MediaId, Tick)> = list
            .iter()
            .enumerate()
            .map(|(k, t)| {
                let shift = if k == clip.active_take() {
                    Tick::ZERO
                } else {
                    t.offset
                };
                (t.media, (clip.source_in + shift).max(Tick::ZERO))
            })
            .collect();
        let rows: Vec<(String, String)> = list
            .iter()
            .enumerate()
            .map(|(k, t)| {
                let name = p.take_name(self.clip, k).unwrap_or_default();
                let length = p
                    .media(t.media)
                    .map_or_else(String::new, |m| bv_core::timecode::format(m.duration, rate));
                (name, length)
            })
            .collect();
        let active = clip.active_take();
        let mut open = true;
        let (mut ok, mut cancel, mut play, mut stop) = (false, false, false, false);
        let title = if self.delete {
            tr("Delete Take")
        } else {
            tr("Take Chooser")
        };
        crate::floating::Window::new(title)
            .id(egui::Id::new("take-chooser"))
            .collapsible(false)
            .resizable(true)
            .default_size([440.0, 260.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(if self.delete {
                    tr("The take to delete:")
                } else {
                    tr("The take the event plays:")
                });
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .max_height(170.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        egui::Grid::new("take-chooser-grid")
                            .striped(true)
                            .num_columns(3)
                            .show(ui, |ui| {
                                ui.strong(tr("Take"));
                                ui.strong(tr("Name"));
                                ui.strong(tr("Length"));
                                ui.end_row();
                                for (k, (name, length)) in rows.iter().enumerate() {
                                    let number = if k == active {
                                        format!("{} ▶", k + 1)
                                    } else {
                                        (k + 1).to_string()
                                    };
                                    let picked = self.picked == k;
                                    let a = ui.selectable_label(picked, number);
                                    let b = ui.selectable_label(picked, name);
                                    ui.label(length);
                                    ui.end_row();
                                    if a.clicked() || b.clicked() {
                                        self.picked = k;
                                    }
                                    if a.double_clicked() || b.double_clicked() {
                                        ok = true;
                                    }
                                }
                            });
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    play = ui.button(tr("Play")).clicked();
                    stop = ui.button(tr("Stop")).clicked();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(tr("Cancel")).clicked() {
                            cancel = true;
                        }
                        if ui.button(tr("OK")).clicked() {
                            ok = true;
                        }
                    });
                });
            });
        // Play: the take picked, through the preview bus, from where the
        // event plays it.
        if play && let Some(&(media, from)) = starts.get(self.picked) {
            app.preview_media(media, from);
        }
        let closing = ok || cancel || !open;
        if (stop || closing) && app.previewing() {
            app.stop_preview(true);
        }
        if ok {
            let (id, k) = (self.clip, self.picked);
            if self.delete {
                app.modify(|p| {
                    let _ = p.delete_take(id, k);
                });
            } else {
                app.choose_take(id, k);
            }
            return false;
        }
        open && !cancel
    }
}

impl TakeRename {
    /// Shows the dialog; whether it stays open.
    fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        if app.project().clip(self.clip).is_none() {
            return false;
        }
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        crate::floating::Window::new(tr("Rename Active Take"))
            .id(egui::Id::new("take-rename"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(tr("Name:"));
                let edit = ui.add(egui::TextEdit::singleline(&mut self.text).desired_width(280.0));
                if self.focus {
                    edit.request_focus();
                    self.focus = false;
                }
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    ok = true;
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(tr("Cancel")).clicked() {
                            cancel = true;
                        }
                        if ui.button(tr("OK")).clicked() {
                            ok = true;
                        }
                    });
                });
            });
        if ok {
            let (id, k, name) = (self.clip, self.take, self.text.clone());
            app.modify(|p| {
                let _ = p.rename_take(id, k, &name);
            });
            return false;
        }
        open && !cancel
    }
}
