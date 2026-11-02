//! Small windows opened from the menus: Go To, Paste Repeat, the welcome
//! screen, window layout management and clearing the edit history.

use crate::i18n::tr;
use bv_core::{Tick, timecode};

use crate::app::BananaApp;
use crate::egui::{self, RichText};
use crate::settings::UiSettings;
use crate::theme;

/// What the Go To window's time sets: the cursor (Ctrl+G), the time
/// selection's end (Ctrl+Shift+G) or its length.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GoToField {
    #[default]
    Cursor,
    SelectionEnd,
    SelectionLength,
}

#[derive(Debug, Default)]
pub struct MenuDialogs {
    go_to: Option<(String, bool)>,
    go_to_field: GoToField,
    paste_repeat: Option<PasteRepeat>,
    pub welcome: crate::welcome::Welcome,
    /// View › Window Layouts › Organize Layouts.
    pub layouts: Option<crate::layouts::Organize>,
    /// View › Window Layouts › Save Layout As.
    save_layout: Option<crate::layouts::SaveAs>,
    pub clear_history: bool,
    /// Windows of the Insert, Tools, Options and Help menus.
    pub more: crate::more_dialogs::MoreDialogs,
}

#[derive(Debug)]
struct PasteRepeat {
    times: u32,
    even: bool,
    every: String,
    error: bool,
}

impl MenuDialogs {
    pub fn open_go_to(&mut self, current: String) {
        self.open_go_to_field(current, GoToField::Cursor);
    }

    pub fn open_go_to_field(&mut self, current: String, field: GoToField) {
        self.go_to = Some((current, false));
        self.go_to_field = field;
    }

    pub fn open_paste_repeat(&mut self) {
        self.paste_repeat = Some(PasteRepeat {
            times: 2,
            even: false,
            every: "00:00:01;00".into(),
            error: false,
        });
    }

    /// Save Layout As, for the layout in use (`current`) if any.
    pub fn open_save_layout(&mut self, settings: &UiSettings, current: Option<usize>) {
        self.save_layout = Some(crate::layouts::SaveAs::new(settings, current));
    }

    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        self.go_to_window(ctx, app);
        self.paste_repeat_window(ctx, app);
        self.welcome.show(ctx, app);
        if let Some(w) = &mut self.layouts
            && !w.show(ctx, app)
        {
            self.layouts = None;
        }
        if let Some(w) = &mut self.save_layout
            && !w.show(ctx, app)
        {
            self.save_layout = None;
        }
        self.clear_history_window(ctx, app);
        self.more.show(ctx, app);
    }

    fn go_to_window(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        let Some((mut text, mut error)) = self.go_to.take() else {
            return;
        };
        let mut open = true;
        let mut cancel = false;
        let mut go = false;
        let field = self.go_to_field;
        let title = match field {
            GoToField::Cursor => tr("Go To"),
            GoToField::SelectionEnd => tr("Selection End"),
            GoToField::SelectionLength => tr("Selection Length"),
        };
        crate::floating::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(tr("Time (timecode or seconds):"));
                let r = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(160.0),
                );
                r.request_focus();
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    go = true;
                }
                if error {
                    ui.colored_label(
                        theme::error(),
                        tr("Use HH:MM:SS;FF or a number of seconds."),
                    );
                }
                ui.horizontal(|ui| {
                    go |= ui.button("OK").clicked();
                    cancel |= ui.button(tr("Cancel")).clicked();
                });
            });
        if go {
            match timecode::parse(&text, app.frame_rate()) {
                Some(t) => {
                    match field {
                        GoToField::Cursor => {
                            app.seek_playback(t);
                            app.center_on_cursor();
                        }
                        // The selection from its start (or the cursor) to
                        // the time typed, or that long.
                        GoToField::SelectionEnd | GoToField::SelectionLength => {
                            let start = app.project().loop_region.map_or(app.cursor, |r| r.start);
                            let end = if field == GoToField::SelectionEnd {
                                t
                            } else {
                                start + t
                            };
                            let r = bv_core::TimeRange::from_bounds(start.min(end), start.max(end));
                            app.set_loop_region((r.duration > Tick::ZERO).then_some(r));
                        }
                    }
                    return;
                }
                None => error = true,
            }
        }
        if open && !cancel {
            self.go_to = Some((text, error));
        }
    }

    fn paste_repeat_window(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        let Some(mut d) = self.paste_repeat.take() else {
            return;
        };
        let mut open = true;
        let mut cancel = false;
        let mut apply = false;
        crate::floating::Window::new(tr("Paste Repeat"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("Number of times to paste:"));
                    ui.add(egui::DragValue::new(&mut d.times).range(1..=999));
                });
                ui.label(
                    RichText::new(tr("Spacing"))
                        .color(theme::text_dim())
                        .small(),
                );
                crate::widgets::radio_value(ui, &mut d.even, false, tr("End to end"));
                crate::widgets::radio_value(ui, &mut d.even, true, tr("Even spacing"));
                ui.add_enabled_ui(d.even, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(tr("Paste every:"));
                        ui.add(
                            egui::TextEdit::singleline(&mut d.every)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(110.0),
                        );
                    });
                });
                if d.error {
                    ui.colored_label(theme::error(), tr("Invalid interval."));
                }
                ui.horizontal(|ui| {
                    apply = ui.button("OK").clicked();
                    cancel |= ui.button(tr("Cancel")).clicked();
                });
            });
        if apply {
            let every = if d.even {
                match timecode::parse(&d.every, app.frame_rate()) {
                    Some(t) if t > Tick::ZERO => Some(t),
                    _ => {
                        d.error = true;
                        self.paste_repeat = Some(d);
                        return;
                    }
                }
            } else {
                None
            };
            app.paste_repeat(d.times as usize, every);
            return;
        }
        if open && !cancel {
            self.paste_repeat = Some(d);
        }
    }

    fn clear_history_window(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        if !self.clear_history {
            return;
        }
        let mut answer = None;
        egui::Modal::new(egui::Id::new("limpar-historico")).show(ctx, |ui| {
            ui.heading(tr("Clear the edit history?"));
            ui.label(tr(
                "You will no longer be able to undo or redo the edits made so far.",
            ));
            ui.horizontal(|ui| {
                if ui.button(tr("Clear")).clicked() {
                    answer = Some(true);
                }
                if ui.button(tr("Cancel")).clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                app.clear_history();
                self.clear_history = false;
            }
            Some(false) => self.clear_history = false,
            None => {}
        }
    }
}
