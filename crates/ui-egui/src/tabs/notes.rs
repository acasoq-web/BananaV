//! Project Notes (View › Window › Project Notes), as VEGAS 2026's Help and
//! its picture show the window: Hide resolved notes at the top; each note
//! a bar in its color — its label ("Untitled" when it has none) and when it
//! was written; Update Timecode to Timeline Cursor, Jump to Timecode, its
//! timecode (click it to type another), Resolve/Unresolve, Delete and More
//! (its color, its label) — over its text; at the bottom the Note Default
//! Color, the default label and Add New Note (a note at the cursor). The
//! notes are `bv_timeline::notes`.

use bv_timeline::notes::{NOTE_COLORS, ProjectNote};

use crate::app::BananaApp;
use crate::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke, pos2, vec2};
use crate::i18n::tr;
use crate::theme;
use crate::timefmt::RulerFormatExt;

/// The window's state: notes resolved hidden; a timecode or a label being
/// typed.
#[derive(Debug, Default)]
pub(crate) struct NotesView {
    pub hide_resolved: bool,
    typing_time: Option<(u32, String)>,
    typing_label: Option<(u32, String)>,
}

/// A note color's name, as its lists show it.
fn color_name(i: usize) -> &'static str {
    [
        tr("Teal"),
        tr("Red"),
        tr("Orange"),
        tr("Yellow"),
        tr("Green"),
        tr("Blue"),
        tr("Purple"),
        tr("Gray"),
    ][i % 8]
}

fn color32(i: u8) -> Color32 {
    let [r, g, b] = NOTE_COLORS[i as usize % NOTE_COLORS.len()];
    Color32::from_rgb(r, g, b)
}

/// When a note was written, as the window writes it ("Monday, November 7,
/// 2022 1:20:01 PM" in English).
fn written(n: &ProjectNote) -> String {
    if n.created == 0 {
        return String::new();
    }
    crate::fmt::long_date_time(n.created, n.utc_offset)
}

pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    let mut view = std::mem::take(&mut app.notes_view);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            crate::widgets::checkbox(ui, &mut view.hide_resolved, tr("Hide resolved notes"));
        });
    });
    ui.separator();
    let bottom_h = 30.0;
    let list_h = (ui.available_height() - bottom_h).max(40.0);
    let notes: Vec<ProjectNote> = app.project().project_notes.clone();
    egui::ScrollArea::vertical()
        .max_height(list_h)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let hide = view.hide_resolved;
            for n in notes.iter().filter(|n| !(hide && n.resolved)) {
                note_ui(ui, app, &mut view, n);
                ui.add_space(4.0);
            }
            if notes.is_empty() {
                crate::tabs::hint(
                    ui,
                    tr("No notes yet: Add New Note leaves one at the cursor."),
                );
            }
        });
    ui.separator();
    bottom_bar(ui, app);
    app.notes_view = view;
}

/// One note: its bar and its text.
fn note_ui(ui: &mut egui::Ui, app: &mut BananaApp, view: &mut NotesView, n: &ProjectNote) {
    let id = n.id;
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    let p = ui.painter().clone();
    p.rect_filled(bar, 2.0, theme::panel());
    // The label and the date, on the note's color.
    let label = if n.label.is_empty() {
        tr("Untitled").to_string()
    } else {
        n.label.clone()
    };
    let date = written(n);
    let font = FontId::proportional(12.0);
    let galley_w = |text: &str| {
        p.layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE)
            .size()
            .x
    };
    let tag_w = (galley_w(&label) + galley_w(&date) + 26.0).min(bar.width() * 0.55);
    let tag = egui::Rect::from_min_size(bar.min + vec2(2.0, 2.0), vec2(tag_w, 20.0));
    p.rect_filled(tag, 2.0, color32(n.color));
    p.rect_stroke(
        tag,
        2.0,
        Stroke::new(1.0, Color32::from_gray(20)),
        egui::StrokeKind::Inside,
    );
    let lx = tag.left() + 6.0;
    let lg = p.text(
        pos2(lx, tag.center().y),
        Align2::LEFT_CENTER,
        &label,
        if n.label.is_empty() {
            FontId::new(12.0, egui::FontFamily::Proportional)
        } else {
            font.clone()
        },
        if n.label.is_empty() {
            Color32::from_gray(200)
        } else {
            Color32::WHITE
        },
    );
    p.text(
        pos2(lg.right() + 12.0, tag.center().y),
        Align2::LEFT_CENTER,
        &date,
        font.clone(),
        Color32::WHITE,
    );
    // The buttons, from the right: More, Delete, Resolve, the timecode, the
    // clock, Jump, Update.
    let mut x = bar.right() - 4.0;
    let mut button = |w: f32| {
        x -= w;
        let r = egui::Rect::from_min_size(pos2(x, bar.top() + 2.0), vec2(w, 20.0));
        x -= 4.0;
        r
    };
    let more_r = button(22.0);
    let del_r = button(20.0);
    let res_r = button(20.0);
    let time_r = button(92.0);
    let clock_r = button(16.0);
    let jump_r = button(20.0);
    let upd_r = button(20.0);
    let hot = |ui: &egui::Ui, r: egui::Rect, salt: &str, tip: &str| -> egui::Response {
        let resp = ui.interact(r, egui::Id::new(("note", id, salt)), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r, 2.0, theme::hover());
        }
        resp.on_hover_text(tip)
    };
    // Update Timecode to Timeline Cursor: a pin with a +.
    let r = hot(
        ui,
        upd_r,
        "update",
        tr("Update Timecode to Timeline Cursor"),
    );
    paint_pin(&p, upd_r, true);
    if r.clicked() {
        let at = app.cursor;
        app.modify(|p| {
            if let Some(n) = p.note_mut(id) {
                n.time = at;
            }
        });
    }
    // Jump to Timecode: an arrow out of a pin.
    let r = hot(ui, jump_r, "jump", tr("Jump to Timecode"));
    paint_pin(&p, jump_r, false);
    if r.clicked() {
        app.seek_playback(n.time);
    }
    // The clock and the timecode (click to type another).
    paint_clock(&p, clock_r);
    if let Some((_, text)) = view.typing_time.as_mut().filter(|(t, _)| *t == id) {
        let resp = ui.put(
            time_r,
            egui::TextEdit::singleline(text).font(FontId::monospace(12.0)),
        );
        if !resp.has_focus() && !resp.lost_focus() {
            resp.request_focus();
        }
        if resp.lost_focus() {
            let typed = view.typing_time.take().map(|(_, t)| t).unwrap_or_default();
            let settings = app.project().settings;
            let offset = app.project().ruler_offset;
            if !ui.input(|i| i.key_pressed(egui::Key::Escape))
                && let Some(t) = settings.effective_ruler_format().parse(&typed, &settings)
            {
                let at = (t - offset).max(bv_core::Tick::ZERO);
                app.modify(|p| {
                    if let Some(n) = p.note_mut(id) {
                        n.time = at;
                    }
                });
            }
        }
    } else {
        let text = app.ruler_text(n.time);
        let r = hot(
            ui,
            time_r,
            "time",
            tr("Select Current Timecode: click to type another"),
        );
        p.text(
            time_r.center(),
            Align2::CENTER_CENTER,
            &text,
            FontId::monospace(12.0),
            theme::text(),
        );
        if r.clicked() {
            view.typing_time = Some((id, text));
        }
    }
    // Resolve / Unresolve.
    let tip = if n.resolved {
        tr("Unresolve Note")
    } else {
        tr("Resolve Note")
    };
    let r = hot(ui, res_r, "resolve", tip);
    let c = res_r.center();
    if n.resolved {
        p.circle_filled(c, 6.0, theme::banana());
        p.line_segment(
            [c + vec2(-3.0, 0.0), c + vec2(-1.0, 2.5)],
            Stroke::new(1.5, Color32::BLACK),
        );
        p.line_segment(
            [c + vec2(-1.0, 2.5), c + vec2(3.5, -2.5)],
            Stroke::new(1.5, Color32::BLACK),
        );
    } else {
        p.circle_stroke(c, 6.0, Stroke::new(1.3, theme::text()));
    }
    if r.clicked() {
        let v = !n.resolved;
        app.modify(|p| {
            if let Some(n) = p.note_mut(id) {
                n.resolved = v;
            }
        });
    }
    // Delete.
    let r = hot(ui, del_r, "delete", tr("Delete Note"));
    let c = del_r.center();
    let st = Stroke::new(1.4, theme::text());
    p.line_segment([c + vec2(-4.0, -4.0), c + vec2(4.0, 4.0)], st);
    p.line_segment([c + vec2(-4.0, 4.0), c + vec2(4.0, -4.0)], st);
    if r.clicked() {
        app.modify(|p| p.remove_note(id));
        return;
    }
    // More: its color, its label.
    let r = hot(ui, more_r, "more", tr("More"));
    for k in [-5.0, 0.0, 5.0] {
        p.circle_filled(more_r.center() + vec2(k, 0.0), 1.5, theme::text());
    }
    egui::Popup::menu(&r).show(|ui| {
        for i in 0..NOTE_COLORS.len() {
            if crate::widgets::radio(ui, n.color as usize == i, color_name(i)).clicked() {
                app.modify(|p| {
                    if let Some(n) = p.note_mut(id) {
                        n.color = i as u8;
                    }
                });
                ui.close();
            }
        }
        ui.separator();
        if ui.button(tr("Label…")).clicked() {
            view.typing_label = Some((id, n.label.clone()));
            ui.close();
        }
    });
    // The label being typed, over the bar's tag.
    if let Some((_, text)) = view.typing_label.as_mut().filter(|(t, _)| *t == id) {
        let resp = ui.put(tag, egui::TextEdit::singleline(text));
        if !resp.has_focus() && !resp.lost_focus() {
            resp.request_focus();
        }
        if resp.lost_focus() {
            let typed = view.typing_label.take().map(|(_, t)| t).unwrap_or_default();
            if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                app.modify(|p| {
                    if let Some(n) = p.note_mut(id) {
                        n.label = typed.trim().to_string();
                    }
                });
            }
        }
    }
    // Its text.
    let mut text = n.text.clone();
    let color = if n.resolved {
        theme::text_dim()
    } else {
        theme::text()
    };
    let resp = ui.add(
        egui::TextEdit::multiline(&mut text)
            .id(egui::Id::new(("note-text", id)))
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .text_color(color)
            .hint_text(tr("Type the note here")),
    );
    if resp.changed() {
        app.modify_merged(resp.id, |p| {
            if let Some(n) = p.note_mut(id) {
                n.text = text;
            }
        });
    }
}

/// The bottom bar: the Note Default Color, the default label, Add New Note.
fn bottom_bar(ui: &mut egui::Ui, app: &mut BananaApp) {
    ui.horizontal(|ui| {
        let color = app.settings.note_color;
        let (r, resp) = ui.allocate_exact_size(vec2(30.0, 20.0), Sense::click());
        ui.painter().rect_filled(
            egui::Rect::from_min_size(r.min + vec2(2.0, 3.0), vec2(14.0, 14.0)),
            1.0,
            color32(color),
        );
        let c = pos2(r.right() - 7.0, r.center().y);
        ui.painter().add(egui::Shape::convex_polygon(
            vec![c + vec2(-3.5, 2.0), c + vec2(3.5, 2.0), c + vec2(0.0, -2.5)],
            theme::text(),
            Stroke::NONE,
        ));
        let resp = resp.on_hover_text(tr("Note Default Color"));
        egui::Popup::menu(&resp).show(|ui| {
            for i in 0..NOTE_COLORS.len() {
                if crate::widgets::radio(ui, color as usize == i, color_name(i)).clicked() {
                    app.settings.note_color = i as u8;
                    ui.close();
                }
            }
        });
        let mut label = app.settings.note_label.clone();
        if ui
            .add(
                egui::TextEdit::singleline(&mut label)
                    .desired_width(200.0)
                    .hint_text(RichText::new(tr("Enter default label")).italics()),
            )
            .changed()
        {
            app.settings.note_label = label;
        }
        if ui.button(tr("Add new note")).clicked() {
            let created = (unix_now(), bv_platform::utc_offset());
            let (at, label, color) = (
                app.cursor,
                app.settings.note_label.clone(),
                app.settings.note_color,
            );
            app.modify(|p| p.add_note(at, &label, color, created));
        }
    });
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// A map pin: with a + (Update Timecode), or with an arrow (Jump).
fn paint_pin(p: &egui::Painter, r: egui::Rect, plus: bool) {
    let c = r.center() + vec2(-2.0, 0.0);
    let blue = Color32::from_rgb(90, 140, 230);
    p.circle_filled(c + vec2(0.0, -3.0), 3.5, blue);
    p.line_segment(
        [c + vec2(0.0, -1.0), c + vec2(0.0, 6.0)],
        Stroke::new(1.5, theme::text()),
    );
    let st = Stroke::new(1.3, theme::text());
    let k = c + vec2(6.0, 3.0);
    if plus {
        p.line_segment([k + vec2(-2.5, 0.0), k + vec2(2.5, 0.0)], st);
        p.line_segment([k + vec2(0.0, -2.5), k + vec2(0.0, 2.5)], st);
    } else {
        p.line_segment([k + vec2(-3.0, 0.0), k + vec2(2.5, 0.0)], st);
        p.line_segment([k + vec2(2.5, 0.0), k + vec2(0.0, -2.0)], st);
        p.line_segment([k + vec2(2.5, 0.0), k + vec2(0.0, 2.0)], st);
    }
}

fn paint_clock(p: &egui::Painter, r: egui::Rect) {
    let c = r.center();
    p.circle_filled(c, 5.5, theme::text());
    let st = Stroke::new(1.3, theme::panel());
    p.line_segment([c, c + vec2(0.0, -3.5)], st);
    p.line_segment([c, c + vec2(2.5, 0.0)], st);
}

impl BananaApp {
    /// Project Notes open with three notes — one untitled at the start, a
    /// labelled one in red, one resolved —, a default label typed (screen
    /// tests; the settings in memory).
    pub fn project_notes_for_review(&mut self) {
        use bv_core::Tick;
        self.settings = crate::settings::UiSettings::in_memory();
        self.settings.note_label = "Review".into();
        self.settings.note_color = 4;
        // Monday, November 7, 2022 1:20:01 PM (the Help's picture).
        let when = (1_667_827_201, 0);
        self.modify(|p| {
            let a = p.add_note(Tick::ZERO, "", 0, when);
            if let Some(n) = p.note_mut(a) {
                n.text = "This is a note to remind you of something that has to be done".into();
            }
            let b = p.add_note(Tick::from_seconds(4), "Color", 1, (when.0 + 3600, 0));
            if let Some(n) = p.note_mut(b) {
                n.text = "The gradient is too warm here.".into();
            }
            let c = p.add_note(Tick::from_seconds(10), "Sound", 5, (when.0 + 7200, 0));
            if let Some(n) = p.note_mut(c) {
                n.text = "The tone is louder than the dialogue.".into();
                n.resolved = true;
            }
        });
        self.show_tab(crate::dock::Tab::Notes, false);
    }
}
