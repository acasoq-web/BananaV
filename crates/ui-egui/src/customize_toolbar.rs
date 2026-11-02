//! Options › Customize Toolbar and Customize Timeline Toolbar: VEGAS's
//! "Customize Toolbar" window. Two lists (the buttons not on the toolbar,
//! and the toolbar's own in order) with Add, Remove, Move Up and Move Down;
//! Reset brings VEGAS's default back. Every change shows on the toolbar at
//! once and is saved.

use crate::app::BananaApp;
use crate::egui::{self, Align2, FontId, Sense, Stroke, pos2, vec2};
use crate::i18n::tr;
use crate::theme;
use crate::toolbar_buttons::{TbButton, ToolbarKind};

const ROW_H: f32 = 18.0;
const ROWS: f32 = 8.0;
const LIST_W: f32 = 196.0;

/// The buttons that can be added: a separator, then those not on the
/// toolbar.
pub fn available(kind: ToolbarKind, current: &[TbButton]) -> Vec<TbButton> {
    std::iter::once(TbButton::Separator)
        .chain(
            kind.all_buttons()
                .into_iter()
                .filter(|b| !current.contains(b)),
        )
        .collect()
}

/// Inserts `b` before position `at` (the end when `at` is past it).
pub fn add(current: &mut Vec<TbButton>, at: usize, b: TbButton) -> usize {
    let at = at.min(current.len());
    current.insert(at, b);
    at
}

/// Moves item `i` up (-1) or down (+1); returns its new position.
pub fn shift(current: &mut [TbButton], i: usize, dir: i32) -> usize {
    let j = i as i32 + dir;
    if i >= current.len() || j < 0 || j as usize >= current.len() {
        return i;
    }
    current.swap(i, j as usize);
    j as usize
}

#[derive(Debug)]
pub struct ToolbarDialog {
    kind: ToolbarKind,
    /// Selected row of the available list.
    available_sel: usize,
    /// Selected row of the toolbar list; its length is the empty last row
    /// (adding there appends).
    current_sel: usize,
}

impl ToolbarDialog {
    pub fn new(kind: ToolbarKind) -> ToolbarDialog {
        ToolbarDialog {
            kind,
            available_sel: 0,
            current_sel: 0,
        }
    }

    /// Draws the window; `false` once it is closed.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let mut open = true;
        let mut close = false;
        let mut current = crate::toolbar_buttons::current(app, self.kind);
        let before = current.clone();
        let avail = available(self.kind, &current);
        self.available_sel = self.available_sel.min(avail.len().saturating_sub(1));
        self.current_sel = self.current_sel.min(current.len());
        crate::floating::Window::new(tr("Customize Toolbar"))
            .id(egui::Id::new((
                "customize-toolbar",
                self.kind == ToolbarKind::Main,
            )))
            .collapsible(false)
            .resizable(false)
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    // Available buttons.
                    ui.vertical(|ui| {
                        ui.label(tr("Available toolbar buttons:"));
                        let rows: Vec<(TbButton, bool)> =
                            avail.iter().map(|b| (*b, false)).collect();
                        if let Some(i) =
                            list(ui, "toolbar-available", &rows, &mut self.available_sel)
                        {
                            self.available_sel = i;
                            self.add(&mut current, &avail);
                        }
                    });
                    // Add / Remove.
                    ui.vertical(|ui| {
                        ui.add_space(52.0);
                        let size = vec2(64.0, 22.0);
                        if ui
                            .add_enabled(
                                !avail.is_empty(),
                                egui::Button::new(tr("Add ->")).min_size(size),
                            )
                            .clicked()
                        {
                            self.add(&mut current, &avail);
                        }
                        ui.add_space(4.0);
                        let removable = self.current_sel < current.len();
                        if ui
                            .add_enabled(
                                removable,
                                egui::Button::new(tr("<- Remove")).min_size(size),
                            )
                            .clicked()
                        {
                            self.remove(&mut current);
                        }
                    });
                    // The toolbar, with its empty last row.
                    ui.vertical(|ui| {
                        ui.label(tr("Current toolbar buttons:"));
                        let rows: Vec<(TbButton, bool)> = current
                            .iter()
                            .map(|b| (*b, false))
                            .chain(std::iter::once((TbButton::Separator, true)))
                            .collect();
                        if let Some(i) = list(ui, "toolbar-current", &rows, &mut self.current_sel) {
                            self.current_sel = i;
                            self.remove(&mut current);
                        }
                    });
                    // Close, Reset, Help · Move Up, Move Down.
                    ui.vertical(|ui| {
                        let size = vec2(84.0, 22.0);
                        ui.add_space(14.0);
                        if ui
                            .add(egui::Button::new(tr("Close")).min_size(size))
                            .clicked()
                        {
                            close = true;
                        }
                        if ui
                            .add(egui::Button::new(tr("Reset")).min_size(size))
                            .clicked()
                        {
                            current = self.kind.default_buttons().to_vec();
                            self.current_sel = current.len();
                        }
                        if ui
                            .add(egui::Button::new(tr("Help")).min_size(size))
                            .clicked()
                        {
                            app.menu_dialogs.more.help = Some(0);
                        }
                        ui.add_space(30.0);
                        let sel = self.current_sel;
                        let up = sel > 0 && sel < current.len();
                        if ui
                            .add_enabled(up, egui::Button::new(tr("Move Up")).min_size(size))
                            .clicked()
                        {
                            self.current_sel = shift(&mut current, sel, -1);
                        }
                        let down = sel + 1 < current.len();
                        if ui
                            .add_enabled(down, egui::Button::new(tr("Move Down")).min_size(size))
                            .clicked()
                        {
                            self.current_sel = shift(&mut current, sel, 1);
                        }
                    });
                });
            });
        if current != before {
            crate::toolbar_buttons::set_current(app, self.kind, current);
        }
        open && !close
    }

    fn add(&mut self, current: &mut Vec<TbButton>, avail: &[TbButton]) {
        let Some(&b) = avail.get(self.available_sel) else {
            return;
        };
        let at = add(current, self.current_sel, b);
        // The row after the added one stays selected, like VEGAS: adding
        // again puts the next button after this one.
        self.current_sel = at + 1;
        if b != TbButton::Separator {
            self.available_sel = self.available_sel.min(avail.len().saturating_sub(2));
        }
    }

    fn remove(&mut self, current: &mut Vec<TbButton>) {
        if self.current_sel < current.len() {
            current.remove(self.current_sel);
        }
    }
}

/// A list box of buttons (icon and name). Returns the row double-clicked.
fn list(ui: &mut egui::Ui, id: &str, rows: &[(TbButton, bool)], sel: &mut usize) -> Option<usize> {
    let mut double = None;
    let height = ROW_H * ROWS;
    egui::Frame::new()
        .fill(theme::panel_dark())
        .stroke(Stroke::new(1.0, theme::separator()))
        .show(ui, |ui| {
            ui.set_width(LIST_W);
            egui::ScrollArea::vertical()
                .id_salt(id)
                .max_height(height)
                .min_scrolled_height(height)
                .auto_shrink([false, false])
                .show_rows(ui, ROW_H, rows.len(), |ui, range| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for i in range {
                        let (b, placeholder) = rows[i];
                        let (r, resp) = ui
                            .allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                        let p = ui.painter();
                        if *sel == i {
                            p.rect_filled(r, 0.0, theme::tab_on());
                        } else if resp.hovered() {
                            p.rect_filled(r, 0.0, theme::hover());
                        }
                        if let Some(icon) = b.icon() {
                            let ir = egui::Rect::from_min_size(
                                pos2(r.left() + 3.0, r.top() + 1.0),
                                vec2(16.0, 16.0),
                            );
                            crate::icons::paint(p, ir, icon, theme::text());
                        }
                        let color = if placeholder {
                            theme::text_dim()
                        } else {
                            theme::text()
                        };
                        let x = if b == TbButton::Separator { 20.0 } else { 24.0 };
                        crate::widgets::fit_text(
                            p,
                            pos2(r.left() + x, r.center().y),
                            Align2::LEFT_CENTER,
                            b.label(),
                            FontId::proportional(12.5),
                            color,
                            r.width() - x - 4.0,
                        );
                        let resp = resp.on_hover_text(b.label());
                        resp.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                true,
                                *sel == i,
                                b.label(),
                            )
                        });
                        if resp.clicked() {
                            *sel = i;
                        }
                        if resp.double_clicked() && !placeholder {
                            double = Some(i);
                        }
                    }
                });
        });
    double
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_remove_and_move_like_vegas() {
        let kind = ToolbarKind::Timeline;
        let mut current = kind.default_buttons().to_vec();
        let avail = available(kind, &current);
        assert_eq!(avail[0], TbButton::Separator);
        assert_eq!(avail[1], TbButton::NewAudioTrack);
        assert_eq!(avail.len(), 23, "a separator and VEGAS's 22 other buttons");
        // Adding at the empty last row appends.
        let n = current.len();
        add(&mut current, n, TbButton::NewAudioTrack);
        assert_eq!(current.last(), Some(&TbButton::NewAudioTrack));
        assert!(!available(kind, &current).contains(&TbButton::NewAudioTrack));
        // Move it to the front.
        let mut i = current.len() - 1;
        while i > 0 {
            i = shift(&mut current, i, -1);
        }
        assert_eq!(current[0], TbButton::NewAudioTrack);
        assert_eq!(shift(&mut current, 0, -1), 0, "the first row cannot go up");
        // Removing a default button puts it back in the available list.
        current.retain(|b| *b != TbButton::Record);
        assert!(available(kind, &current).contains(&TbButton::Record));
        // The main toolbar has its own list.
        let main = available(ToolbarKind::Main, ToolbarKind::Main.default_buttons());
        assert_eq!(main[1], TbButton::NewProject);
        assert_eq!(main.len(), 49, "a separator and VEGAS's 48 other buttons");
    }
}
