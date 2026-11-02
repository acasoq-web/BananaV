//! Options › Customize Keyboard, laid out like VEGAS: keyboard map, filter,
//! context tabs, the command list with its shortcuts, key capture with
//! Add / Replace / Remove, and what the captured keys are assigned to.

use crate::app::BananaApp;
use crate::egui::{self, Align2, FontId, RichText, Sense, Stroke, StrokeKind, pos2, vec2};
use crate::i18n::tr;
use crate::keymap::{
    Action, Binding, Context, DEFAULT_MAP, compact_names, default_bindings, registry,
};
use crate::theme;

/// Highlight of the selected context tab (the light blue of VEGAS).
const ROW_H: f32 = 17.0;

#[derive(Debug)]
pub struct KeyboardDialog {
    /// Map shown in the combo box (`None` = the default map).
    map: Option<String>,
    /// Working copy of the bindings, applied on OK.
    bindings: Vec<Vec<crate::keymap::Binding>>,
    filter: String,
    context: Context,
    /// Selected row: registry index and binding index (if the row has one).
    selected: Option<(usize, Option<usize>)>,
    captured: Option<Binding>,
    save_as: Option<String>,
    /// Scroll the list to the selected row on the next frame.
    reveal: bool,
}

impl KeyboardDialog {
    pub fn new(app: &BananaApp) -> KeyboardDialog {
        KeyboardDialog {
            map: app.keymap.active.clone(),
            bindings: app.keymap.bindings.clone(),
            filter: String::new(),
            context: Context::TrackView,
            selected: None,
            captured: None,
            save_as: None,
            reveal: false,
        }
    }

    /// Rows of the current tab: (registry index, binding index).
    fn rows(&self) -> Vec<(usize, Option<usize>)> {
        let filter = self.filter.to_lowercase();
        let mut rows = Vec::new();
        for (i, c) in registry().iter().enumerate() {
            if c.context != self.context || !c.id.to_lowercase().contains(&filter) {
                continue;
            }
            if self.bindings[i].is_empty() {
                rows.push((i, None));
            } else {
                rows.extend((0..self.bindings[i].len()).map(|b| (i, Some(b))));
            }
        }
        rows
    }

    /// Commands (any tab) that use `b`.
    fn assigned(&self, b: &Binding) -> Vec<usize> {
        (0..self.bindings.len())
            .filter(|&i| self.bindings[i].contains(b))
            .collect()
    }

    /// Draws the window; returns `false` when it closed.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let mut open = true;
        let mut close = false;
        crate::floating::Window::new(tr("Customize Keyboard"))
            .id(egui::Id::new("personalizar-teclado"))
            .collapsible(false)
            .resizable(true)
            .default_size([
                520.0,
                (ctx.content_rect().height() - 40.0).clamp(560.0, 900.0),
            ])
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .min_size([460.0, 600.0])
            .open(&mut open)
            .show(ctx, |ui| close = self.contents(ui, app));
        open && !close
    }

    fn contents(&mut self, ui: &mut egui::Ui, app: &mut BananaApp) -> bool {
        let mut close = false;
        // Keyboard map.
        ui.label(tr("Keyboard map:"));
        ui.horizontal(|ui| {
            let shown = self.map.clone().unwrap_or_else(|| tr(DEFAULT_MAP).into());
            let mut pick: Option<Option<String>> = None;
            egui::ComboBox::from_id_salt("mapa-teclado")
                .width((ui.available_width() - 190.0).max(120.0))
                .selected_text(&shown)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(self.map.is_none(), tr(DEFAULT_MAP))
                        .clicked()
                    {
                        pick = Some(None);
                    }
                    for name in app.keymap.map_names() {
                        if ui
                            .selectable_label(self.map.as_ref() == Some(&name), &name)
                            .clicked()
                        {
                            pick = Some(Some(name));
                        }
                    }
                });
            if let Some(p) = pick {
                self.bindings = match &p {
                    None => default_bindings(),
                    Some(n) => app.keymap.map_bindings(n).unwrap_or_else(default_bindings),
                };
                self.map = p;
                self.selected = None;
            }
            if ui.button(tr("Save As…")).clicked() {
                self.save_as = Some(self.map.clone().unwrap_or_else(|| tr("My map").into()));
            }
            if ui
                .add_enabled(self.map.is_some(), egui::Button::new(tr("Delete")))
                .clicked()
                && let Some(name) = self.map.take()
            {
                if let Err(e) = app.keymap.delete_map(&name) {
                    app.report_error(trf!("Could not delete the map: {}", e));
                }
                self.bindings = default_bindings();
            }
        });
        if let Some(mut name) = self.save_as.take() {
            let (mut ok, mut cancel) = (false, false);
            ui.horizontal(|ui| {
                ui.label(tr("Name:"));
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(220.0));
                ok |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ok |= ui.button(tr("Save")).clicked();
                cancel = ui.button(tr("Cancel")).clicked();
            });
            let n = name.trim().to_string();
            if ok && !n.is_empty() && n != DEFAULT_MAP && n != tr(DEFAULT_MAP) {
                match app.keymap.apply(Some(n.clone()), self.bindings.clone()) {
                    Ok(()) => self.map = Some(n),
                    Err(e) => app.report_error(trf!("Could not save the map: {}", e)),
                }
            } else if !cancel {
                self.save_as = Some(name);
            }
        }
        ui.add_space(4.0);
        ui.label(tr("Show commands containing:"));
        ui.add(egui::TextEdit::singleline(&mut self.filter).desired_width(f32::INFINITY));
        ui.add_space(4.0);
        self.tabs(ui);
        self.list(ui, app);
        ui.add_space(6.0);
        self.capture_row(ui);
        ui.add_space(4.0);
        self.assigned_box(ui);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_sized([86.0, 24.0], egui::Button::new(tr("Cancel")))
                    .clicked()
                {
                    close = true;
                }
                if ui
                    .add_sized(
                        [86.0, 24.0],
                        egui::Button::new(RichText::new("OK").strong()),
                    )
                    .clicked()
                {
                    // Changes to the default map become a map of their own.
                    let name = match &self.map {
                        Some(n) => Some(n.clone()),
                        None if self.bindings != default_bindings() => Some(tr("Custom").into()),
                        None => None,
                    };
                    match app.keymap.apply(name.clone(), self.bindings.clone()) {
                        Ok(()) if name.as_deref() == Some(tr("Custom")) && self.map.is_none() => {
                            app.notify(tr("Shortcuts saved in the \"Custom\" map."))
                        }
                        Ok(()) => {}
                        Err(e) => app.report_error(trf!("Could not save the shortcuts: {}", e)),
                    }
                    close = true;
                }
            });
        });
        close
    }

    fn tabs(&mut self, ui: &mut egui::Ui) {
        // Rows of tabs like VEGAS: four per row when the names fit (the
        // short English ones), fewer for languages with longer names.
        let w = ui.available_width();
        let widest = Context::ALL
            .into_iter()
            .map(|c| {
                ui.painter()
                    .layout_no_wrap(
                        c.label().to_string(),
                        FontId::proportional(12.5),
                        theme::text(),
                    )
                    .size()
                    .x
            })
            .fold(0.0f32, f32::max);
        let cols = ((w / (widest + 16.0)).floor() as usize).clamp(2, 4);
        let rows = Context::ALL.len().div_ceil(cols);
        let cell = vec2(w / cols as f32, 22.0);
        let (rect, _) = ui.allocate_exact_size(vec2(w, cell.y * rows as f32), Sense::hover());
        let cell_rect = |n: usize| {
            egui::Rect::from_min_size(
                rect.min + vec2(cell.x * (n % cols) as f32, cell.y * (n / cols) as f32),
                cell,
            )
            .shrink(1.0)
        };
        for n in Context::ALL.len()..rows * cols {
            ui.painter().rect(
                cell_rect(n),
                2.0,
                theme::panel_dark(),
                Stroke::new(1.0, theme::separator()),
                StrokeKind::Inside,
            );
        }
        for (n, c) in Context::ALL.into_iter().enumerate() {
            let r = cell_rect(n);
            let resp = ui.interact(r, ui.id().with(("aba-teclado", n)), Sense::click());
            let on = self.context == c;
            let fill = if on {
                theme::tab_on()
            } else if resp.hovered() {
                theme::hover()
            } else {
                theme::panel_dark()
            };
            ui.painter().rect(
                r,
                2.0,
                fill,
                Stroke::new(
                    1.0,
                    if on {
                        theme::snap()
                    } else {
                        theme::separator()
                    },
                ),
                StrokeKind::Inside,
            );
            crate::widgets::fit_text(
                ui.painter(),
                r.center(),
                Align2::CENTER_CENTER,
                c.label(),
                FontId::proportional(12.5),
                theme::text(),
                r.width() - 6.0,
            );
            resp.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, c.label())
            });
            if resp.clicked() {
                self.context = c;
                self.selected = None;
            }
        }
    }

    fn list(&mut self, ui: &mut egui::Ui, app: &BananaApp) {
        let rows = self.rows();
        let names = compact_names(rows.iter().map(|(i, _)| registry()[*i].id.as_str()));
        // Room left for the key capture, the assignments box and OK/Cancel.
        let height = (ui.available_height() - 200.0).max(160.0);
        let selected_row = self
            .selected
            .and_then(|s| rows.iter().position(|r| *r == s));
        egui::Frame::new()
            .fill(theme::panel_dark())
            .stroke(Stroke::new(1.0, theme::separator()))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let mut area = egui::ScrollArea::vertical()
                    .id_salt(("lista-teclado", self.context))
                    .max_height(height)
                    .min_scrolled_height(height)
                    .auto_shrink([false, false]);
                if std::mem::take(&mut self.reveal)
                    && let Some(r) = selected_row
                {
                    area = area.vertical_scroll_offset((r as f32 * ROW_H - height / 2.0).max(0.0));
                }
                area.show_rows(ui, ROW_H, rows.len(), |ui, range| {
                    for row in range {
                        let (i, b) = rows[row];
                        let c = &registry()[i];
                        let (hidden, name) = &names[row];
                        let (r, resp) = ui
                            .allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                        let selected = self.selected == Some((i, b));
                        let p = ui.painter();
                        if selected {
                            p.rect_filled(r, 0.0, theme::tab_on());
                        } else if resp.hovered() {
                            p.rect_filled(r, 0.0, theme::hover());
                        }
                        p.hline(
                            r.x_range(),
                            r.bottom(),
                            Stroke::new(1.0, theme::faint_line()),
                        );
                        p.circle_filled(pos2(r.left() + 7.0, r.center().y), 1.5, theme::text_dim());
                        let soon = c.action == Action::Soon;
                        let color = if soon {
                            theme::text_disabled()
                        } else {
                            theme::text()
                        };
                        // Indented by the width of the part shared with the
                        // row above, so the name stays where it is in the full id.
                        let font = FontId::proportional(12.5);
                        let indent = if *hidden == 0 {
                            0.0
                        } else {
                            p.layout_no_wrap(c.id[..*hidden].to_string(), font.clone(), color)
                                .size()
                                .x
                        };
                        let split = r.left() + r.width() * 0.58;
                        let x = r.left() + 16.0 + indent;
                        crate::widgets::fit_text(
                            p,
                            pos2(x, r.center().y),
                            Align2::LEFT_CENTER,
                            name,
                            font,
                            color,
                            split - 10.0 - x,
                        );
                        p.vline(
                            split - 6.0,
                            r.y_range(),
                            Stroke::new(1.0, theme::faint_line()),
                        );
                        if let Some(b) = b {
                            crate::widgets::fit_text(
                                p,
                                pos2(split, r.center().y),
                                Align2::LEFT_CENTER,
                                self.bindings[i][b].label(),
                                FontId::proportional(12.5),
                                if soon {
                                    theme::text_disabled()
                                } else {
                                    theme::text_dim()
                                },
                                r.right() - split - 4.0,
                            );
                        }
                        let resp = if soon {
                            resp.on_hover_text(tr("Not available in BananaV yet"))
                        } else if let Action::Cmd(cmd) = c.action {
                            resp.on_hover_text(cmd.label(app))
                        } else {
                            resp
                        };
                        if resp.clicked() {
                            self.selected = Some((i, b));
                        }
                    }
                });
            });
    }

    fn capture_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(tr("Shortcut keys:"));
            // Click the box, then press the keys: they are captured as typed.
            let (r, resp) = ui.allocate_exact_size(vec2(160.0, 22.0), Sense::click());
            let id = resp.id;
            if resp.clicked() {
                ui.memory_mut(|m| m.request_focus(id));
            }
            let focused = ui.memory(|m| m.has_focus(id));
            if focused {
                ui.memory_mut(|m| {
                    m.set_focus_lock_filter(
                        id,
                        egui::EventFilter {
                            tab: true,
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            escape: false,
                        },
                    );
                });
                let captured = ui.input_mut(|i| {
                    let mut got = None;
                    i.events.retain(|e| match e {
                        egui::Event::Key {
                            key,
                            physical_key,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            got = Some(Binding::from_event(*key, *physical_key, *modifiers));
                            false
                        }
                        _ => true,
                    });
                    got
                });
                if captured.is_some() {
                    self.captured = captured;
                }
            }
            ui.painter().rect(
                r,
                2.0,
                theme::panel_dark(),
                Stroke::new(
                    1.0,
                    if focused {
                        theme::snap()
                    } else {
                        theme::separator()
                    },
                ),
                StrokeKind::Inside,
            );
            let text = match &self.captured {
                Some(b) => b.label(),
                None if focused => tr("Press the keys…").into(),
                None => String::new(),
            };
            ui.painter().text(
                pos2(r.left() + 6.0, r.center().y),
                Align2::LEFT_CENTER,
                text,
                FontId::proportional(12.5),
                if self.captured.is_some() {
                    theme::text()
                } else {
                    theme::text_dim()
                },
            );
            let cmd_ok = self
                .selected
                .is_some_and(|(i, _)| registry()[i].action != Action::Soon);
            if ui
                .add_enabled(
                    cmd_ok && self.captured.is_some(),
                    egui::Button::new(tr("Add")),
                )
                .clicked()
                && let (Some((i, _)), Some(b)) = (self.selected, self.captured)
                && !self.bindings[i].contains(&b)
            {
                self.bindings[i].push(b);
                self.selected = Some((i, Some(self.bindings[i].len() - 1)));
            }
            let has_binding = self.selected.is_some_and(|(_, b)| b.is_some());
            if ui
                .add_enabled(
                    cmd_ok && has_binding && self.captured.is_some(),
                    egui::Button::new(tr("Replace")),
                )
                .clicked()
                && let (Some((i, Some(b))), Some(k)) = (self.selected, self.captured)
            {
                self.bindings[i][b] = k;
            }
            if ui
                .add_enabled(has_binding, egui::Button::new(tr("Remove")))
                .clicked()
                && let Some((i, Some(b))) = self.selected
            {
                self.bindings[i].remove(b);
                self.selected = Some((i, None));
            }
        });
    }

    fn assigned_box(&mut self, ui: &mut egui::Ui) {
        ui.label(tr("Shortcut currently assigned to:"));
        let users = self.captured.map(|b| self.assigned(&b)).unwrap_or_default();
        ui.horizontal(|ui| {
            egui::Frame::new()
                .fill(theme::panel_dark())
                .stroke(Stroke::new(1.0, theme::separator()))
                .inner_margin(4)
                .show(ui, |ui| {
                    ui.set_min_size(vec2((ui.available_width() - 100.0).max(200.0), 70.0));
                    for &i in &users {
                        let c = &registry()[i];
                        ui.label(format!("{}: {}", c.context.label(), c.id));
                    }
                });
            if ui
                .add_enabled(!users.is_empty(), egui::Button::new(tr("Locate")))
                .clicked()
                && let Some(&i) = users.first()
                && let Some(b) = self.captured
            {
                let c = &registry()[i];
                self.context = c.context;
                self.filter.clear();
                let bi = self.bindings[i].iter().position(|x| *x == b);
                self.selected = Some((i, bi));
                self.reveal = true;
            }
        });
    }
}
