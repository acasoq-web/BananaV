//! Insert › Command's Command Properties dialog, as VEGAS's Help describes
//! it: a template to recall (or the settings to keep as one, its Save
//! button; Delete forgets it), the command — one of VEGAS's list or one
//! typed in —, the parameter it is given, a comment and its position in
//! the ruler's format (the cursor's at first). The same dialog edits a
//! command marker (a double-click on its flag, its menu's Edit…). The
//! templates are kept in the settings (`UiSettings::command_templates`,
//! VEGAS's cmdtemp.xml).

use crate::i18n::tr;
use bv_core::Tick;
use bv_timeline::MarkerId;
use bv_timeline::captions::{channel_of, has_markup, pop_on, shown_text};
use bv_timeline::commands::COMMANDS;
use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::egui::{self, Ui, vec2};
use crate::icons::Icon;
use crate::theme;

/// A saved set of command settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CommandTemplate {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub parameter: String,
    #[serde(default)]
    pub comment: String,
    /// The position as it was typed (empty: wherever the cursor is).
    #[serde(default)]
    pub position: String,
}

/// The Command Properties dialog.
#[derive(Debug)]
pub struct CommandDialog {
    /// The command marker edited (`None`: a new one).
    editing: Option<MarkerId>,
    template: String,
    command: String,
    parameter: String,
    comment: String,
    position: String,
    bad_position: bool,
    /// The Parameter and the Comment as the dialog opened (a caption's
    /// Parameter typed in makes its caption again).
    opened: (String, String),
}

const FIELD_W: f32 = 300.0;

impl CommandDialog {
    /// A new command at `at`.
    pub fn new(app: &BananaApp, at: Tick) -> CommandDialog {
        CommandDialog {
            editing: None,
            template: String::new(),
            command: COMMANDS[0].to_string(),
            parameter: String::new(),
            comment: String::new(),
            position: app.ruler_text(at),
            bad_position: false,
            opened: Default::default(),
        }
    }

    /// The command marker `id`, to edit.
    pub fn edit(app: &BananaApp, id: MarkerId) -> Option<CommandDialog> {
        let c = app.project().command(id)?;
        Some(CommandDialog {
            editing: Some(id),
            template: String::new(),
            command: c.command.clone(),
            parameter: c.parameter.clone(),
            comment: c.comment.clone(),
            position: app.ruler_text(c.time),
            bad_position: false,
            opened: (c.parameter.clone(), c.comment.clone()),
        })
    }

    /// Draws the dialog; `false` once it is closed.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let (mut open, mut ok, mut cancel) = (true, false, false);
        crate::floating::Window::new(tr("Command Properties"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("command-properties")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        self.template_row(ui, app);
                        ui.end_row();
                        ui.label(tr("Command:"));
                        let commands: Vec<String> =
                            COMMANDS.iter().map(|c| c.to_string()).collect();
                        if let Some(k) =
                            editable_combo(ui, "command-list", &mut self.command, &commands)
                        {
                            self.command = commands[k].clone();
                        }
                        ui.end_row();
                        ui.label(tr("Parameter:"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.parameter).desired_width(FIELD_W),
                        );
                        ui.end_row();
                        ui.label(tr("Comment:"));
                        ui.add(
                            egui::TextEdit::multiline(&mut self.comment)
                                .desired_width(FIELD_W)
                                .desired_rows(3),
                        );
                        ui.end_row();
                        ui.label(tr("Position:"));
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut self.position)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(140.0),
                        );
                        ok |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        ui.end_row();
                    });
                if self.bad_position {
                    ui.colored_label(
                        theme::error(),
                        tr("Type the position in the ruler's format."),
                    );
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ok |= ui.button("OK").clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if ok && self.apply(app) {
            return false;
        }
        open && !cancel
    }

    /// The template box with its Save and Delete buttons.
    fn template_row(&mut self, ui: &mut Ui, app: &mut BananaApp) {
        ui.label(tr("Template:"));
        ui.horizontal(|ui| {
            let names: Vec<String> = app
                .settings
                .command_templates
                .iter()
                .map(|t| t.name.clone())
                .collect();
            if let Some(k) = editable_combo(ui, "command-templates", &mut self.template, &names) {
                let t = app.settings.command_templates[k].clone();
                self.template = t.name;
                self.command = t.command;
                self.parameter = t.parameter;
                self.comment = t.comment;
                if !t.position.is_empty() {
                    self.position = t.position;
                }
            }
            let name = self.template.trim().to_string();
            if crate::widgets::icon_button_ex(
                ui,
                Icon::Save,
                tr("Save the settings as this template"),
                false,
                !name.is_empty(),
            )
            .clicked()
            {
                let t = CommandTemplate {
                    name: name.clone(),
                    command: self.command.clone(),
                    parameter: self.parameter.clone(),
                    comment: self.comment.clone(),
                    position: self.position.clone(),
                };
                let list = &mut app.settings.command_templates;
                match list.iter_mut().find(|o| o.name == name) {
                    Some(o) => *o = t,
                    None => list.push(t),
                }
                let _ = app.settings.save();
            }
            let known = names.contains(&name);
            if crate::widgets::icon_button_ex(
                ui,
                Icon::Delete,
                tr("Delete this template"),
                false,
                known,
            )
            .clicked()
            {
                app.settings.command_templates.retain(|t| t.name != name);
                self.template.clear();
                let _ = app.settings.save();
            }
        });
    }

    /// OK: the command made or changed; `false` while the position cannot
    /// be read.
    fn apply(&mut self, app: &mut BananaApp) -> bool {
        let Some(at) = app.parse_ruler_text(&self.position) else {
            self.bad_position = true;
            return false;
        };
        let (command, mut parameter, mut comment) = (
            self.command.trim().to_string(),
            self.parameter.clone(),
            self.comment.clone(),
        );
        if channel_of(&command).is_some() {
            (parameter, comment) = caption_fields(&parameter, &comment, &self.opened);
        }
        match self.editing {
            Some(id) => app.modify(|p| {
                p.set_command(id, |c| {
                    c.time = at;
                    c.command = command;
                    c.parameter = parameter;
                    c.comment = comment;
                })
            }),
            None => {
                app.modify(|p| p.add_command(at, &command, &parameter, &comment));
            }
        }
        true
    }
}

/// A caption's Parameter and Comment (VEGAS's Help): the Comment holds
/// its markup and the Parameter shows its text, `[BR]` between rows.
/// Plain text typed in the Comment — or in the Parameter alone, the
/// Comment as `opened` — is made a pop-on caption.
fn caption_fields(parameter: &str, comment: &str, opened: &(String, String)) -> (String, String) {
    let typed_text = parameter != opened.0 && comment == opened.1;
    let markup = if typed_text && !has_markup(parameter) {
        pop_on(parameter)
    } else if has_markup(comment) {
        comment.to_string()
    } else if has_markup(parameter) {
        parameter.to_string()
    } else if comment.trim().is_empty() {
        pop_on(parameter)
    } else {
        pop_on(comment)
    };
    (shown_text(&markup), markup)
}

/// A text box with a list beside it to pick from (Windows' editable combo
/// box): the index picked.
pub(crate) fn editable_combo(
    ui: &mut Ui,
    id: &str,
    text: &mut String,
    items: &[String],
) -> Option<usize> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add(egui::TextEdit::singleline(text).desired_width(FIELD_W - 22.0));
        let button = egui::Button::new("⏷").min_size(vec2(22.0, 0.0));
        ui.add_enabled_ui(!items.is_empty(), |ui| {
            egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
                ui.push_id(id, |ui| {
                    for (k, item) in items.iter().enumerate() {
                        if ui.selectable_label(*text == *item, item).clicked() {
                            picked = Some(k);
                            ui.close();
                        }
                    }
                });
            });
        });
    });
    picked
}

impl BananaApp {
    /// Command Properties open on a new command at 2 seconds, with a
    /// template saved and a URL typed (screen tests; the settings in
    /// memory).
    pub fn command_properties_for_review(&mut self) {
        self.settings = crate::settings::UiSettings::in_memory();
        self.settings.command_templates.push(CommandTemplate {
            name: "Station site".into(),
            command: "URL".into(),
            parameter: "https://www.example.com".into(),
            comment: "Shows the station's site".into(),
            position: String::new(),
        });
        let mut d = CommandDialog::new(self, Tick::from_seconds(2));
        d.template = "Station site".into();
        d.parameter = "https://www.example.com".into();
        d.comment = "Shows the station's site".into();
        self.menu_dialogs.more.command = Some(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captions_keep_their_markup_in_the_comment_and_text_in_the_parameter() {
        let none = (String::new(), String::new());
        // Text typed in the Comment: a pop-on caption.
        let (p, c) = caption_fields("", "Hello[BR]there", &none);
        assert_eq!(p, "Hello[BR]there");
        assert!(c.starts_with("{RCL}{ENM}") && c.ends_with("{EOC}"), "{c}");
        // Markup typed: kept, its text shown.
        let roll = "{RU2}{CR}{R15In00Wh}Live";
        assert_eq!(
            caption_fields("", roll, &none),
            ("Live".into(), roll.into())
        );
        // The Parameter's text changed on a caption: made again from it.
        let opened = ("Live".to_string(), roll.to_string());
        let (p, c) = caption_fields("Changed", roll, &opened);
        assert_eq!(
            (p.as_str(), c.as_str()),
            ("Changed", pop_on("Changed").as_str())
        );
    }
}
