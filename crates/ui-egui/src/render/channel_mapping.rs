//! Render's Channel Mapping dialog (VEGAS's): with "Enable multichannel
//! mapping", the busses whose sound goes to a Wave or Wave64 file's
//! channels, in order — a check box each, "(Mono downmix)" to make one
//! channel of a bus's, Move Up and Move Down. OK keeps the mapping in the
//! project (one undo); Render to New Track uses it too.

use bv_timeline::{MappedStrip, Project};

use crate::app::BananaApp;
use crate::egui::{self, RichText, Sense, vec2};
use crate::i18n::tr;
use crate::theme;

#[derive(Debug)]
pub struct ChannelMapping {
    rows: Vec<MappedStrip>,
    /// The line Move Up and Move Down move.
    picked: usize,
}

impl ChannelMapping {
    pub fn new(project: &Project) -> ChannelMapping {
        ChannelMapping {
            rows: project.channel_mapping_rows(),
            picked: 0,
        }
    }

    /// Shows the dialog; false once it is closed.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        let layout = app.project().settings.channels.channel_count();
        let names: Vec<String> = self
            .rows
            .iter()
            .map(|m| match m.bus {
                None => tr("Master").to_string(),
                Some(id) => crate::i18n::bus_name(app.project(), id),
            })
            .collect();
        crate::floating::Window::new(tr("Channel Mapping"))
            .id(egui::Id::new("render-channel-mapping"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_min_width(380.0);
                ui.label(tr(
                    "The busses whose sound goes to the rendered file, each in channels of its own, in this order:",
                ));
                ui.add_space(4.0);
                egui::Frame::new()
                    .stroke(egui::Stroke::new(1.0, theme::separator()))
                    .inner_margin(4.0)
                    .show(ui, |ui| {
                        ui.set_min_width(360.0);
                        for (i, (row, name)) in self.rows.iter_mut().zip(&names).enumerate() {
                            ui.horizontal(|ui| {
                                crate::widgets::checkbox(ui, &mut row.on, "");
                                // The line's name picks it for Move Up and
                                // Move Down.
                                let text = RichText::new(name.as_str());
                                let r = ui.add(
                                    egui::Button::selectable(self.picked == i, text)
                                        .min_size(vec2(170.0, 20.0))
                                        .right_text(""),
                                );
                                if r.clicked() {
                                    self.picked = i;
                                }
                                ui.add_enabled_ui(row.on, |ui| {
                                    crate::widgets::checkbox(ui, &mut row.mono, tr("(Mono downmix)"));
                                });
                            });
                        }
                    });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let last = self.rows.len().saturating_sub(1);
                    if ui
                        .add_enabled(self.picked > 0, egui::Button::new(tr("Move Up")))
                        .clicked()
                    {
                        self.rows.swap(self.picked, self.picked - 1);
                        self.picked -= 1;
                    }
                    if ui
                        .add_enabled(self.picked < last, egui::Button::new(tr("Move Down")))
                        .clicked()
                    {
                        self.rows.swap(self.picked, self.picked + 1);
                        self.picked += 1;
                    }
                });
                let channels: usize = self
                    .rows
                    .iter()
                    .filter(|m| m.on)
                    .map(|m| if m.mono { 1 } else { layout })
                    .sum();
                ui.add_space(4.0);
                ui.label(trn!(
                    channels,
                    "{} channel mapped (a template with more renders the rest silent).",
                    "{} channels mapped (a template with more renders the rest silent).",
                    channels
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ok = ui.button(tr("OK")).clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
                ui.allocate_exact_size(vec2(0.0, 0.0), Sense::hover());
            });
        if ok {
            let rows = self.rows.clone();
            app.modify(move |p| p.channel_mapping = rows);
        }
        open && !ok && !cancel
    }
}

impl BananaApp {
    /// The Render window with multichannel mapping on and its Channel
    /// Mapping open over a project of two busses, the first one mapped
    /// (screen tests; the settings in memory).
    pub fn channel_mapping_for_review(&mut self) {
        self.settings = crate::settings::UiSettings::in_memory();
        self.modify(|p| {
            let a = p.add_bus();
            p.add_bus();
            p.channel_mapping = vec![MappedStrip {
                bus: a,
                on: true,
                mono: false,
            }];
        });
        self.settings.render.options.multichannel = true;
        let mut d = super::RenderDialog::new(self);
        d.channels = Some(ChannelMapping::new(self.project()));
        self.dialogs.render = Some(d);
    }
}
