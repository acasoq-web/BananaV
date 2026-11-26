//! BananaV's mods (Options › BananaV Mods…): features of BananaV's own,
//! beyond the workflow it follows, each one switched on in their window —
//! all off at first, so BananaV works as that workflow does until the user
//! chooses them:
//!
//! - Transform: the Transform window (View › Window) and the Video
//!   Preview's Transform mode;
//! - Envelope Appearance: the envelopes' line, grab area and points
//!   (Options › Envelope Appearance…); off, envelopes look as by default;
//! - Track Select tools: Track Select Forward (A) and Track Select
//!   Backward (Shift+A), as other editors have them
//!   ([`crate::timeline`]'s track selection);
//! - Envelope points with events: the selection rectangle and the Track
//!   Select tools pick the track envelopes' points with the events, which
//!   move and are deleted with them;
//! - AI agents: the open project edited by AI agents through the Model
//!   Context Protocol ([`crate::agents`]: `bananav mcp --live`), only
//!   from this computer, saving, rendering and opening with the user's
//!   yes.

use serde::{Deserialize, Serialize};

use crate::app::{BananaApp, Tool};
use crate::dock::Tab;
use crate::egui::{self, RichText, vec2};
use crate::i18n::tr;

/// Which mods are on (saved with the interface's settings).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mods {
    pub transform: bool,
    pub envelope_look: bool,
    pub track_select: bool,
    pub agents: bool,
    pub env_with_events: bool,
}

/// What an agent's program is told to start to reach the open window
/// (Claude Desktop's `mcpServers` and the like): this BananaV's own path.
fn mcp_configuration() -> String {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "bananav".into());
    let config = serde_json::json!({
        "mcpServers": {
            "bananav": {"command": exe, "args": ["mcp", "--live"]},
        },
    });
    serde_json::to_string_pretty(&config).unwrap_or_default()
}

/// The BananaV Mods window (open while it is `Some`): the mods whose
/// description is shown (each one folded at first, its arrow unfolds it).
#[derive(Debug, Default)]
pub struct ModsDialog {
    unfolded: Vec<&'static str>,
}

impl ModsDialog {
    /// Draws the window; `false` once it is closed. A box switched applies
    /// and is saved at once.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let mut open = true;
        let mut close = false;
        let mut mods = app.settings.mods;
        let mut look = false;
        let mut copied = false;
        crate::floating::Window::new(tr("BananaV Mods"))
            .id(egui::Id::new("bananav-mods"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(460.0);
                crate::tabs::hint(
                    ui,
                    tr("Features of BananaV's own, beyond its usual workflow. Each one is off until you turn it on here."),
                );
                ui.add_space(6.0);
                let unfolded = &mut self.unfolded;
                let mut row = |ui: &mut egui::Ui, on: &mut bool, name: &'static str, what: &str| {
                    let open = unfolded.contains(&name);
                    ui.horizontal(|ui| {
                        ui.checkbox(on, RichText::new(name).strong());
                        if fold_arrow(ui, open, name).clicked() {
                            if open {
                                unfolded.retain(|n| *n != name);
                            } else {
                                unfolded.push(name);
                            }
                        }
                    });
                    if open {
                        ui.horizontal(|ui| {
                            ui.add_space(26.0);
                            ui.add(egui::Label::new(what).wrap());
                        });
                    }
                    ui.add_space(6.0);
                };
                row(
                    ui,
                    &mut mods.transform,
                    tr("Transform"),
                    tr("The Transform window (View › Window) and the Video Preview's Transform mode: move, resize and turn an event on the preview."),
                );
                row(
                    ui,
                    &mut mods.envelope_look,
                    tr("Envelope Appearance"),
                    tr("The envelopes' line width, grab area and points' size and shape. Off, envelopes look as by default."),
                );
                ui.horizontal(|ui| {
                    ui.add_space(26.0);
                    look = ui
                        .add_enabled(
                            mods.envelope_look,
                            egui::Button::new(tr("Envelope Appearance…")),
                        )
                        .clicked();
                });
                ui.add_space(6.0);
                row(
                    ui,
                    &mut mods.track_select,
                    tr("Track Select tools"),
                    tr("Track Select Forward (A) and Track Select Backward (Shift+A): a click selects every event from there to the end, or to the start, on all tracks. Shift held: only that track. Alt+Shift: only that track, its events' groups left out."),
                );
                row(
                    ui,
                    &mut mods.env_with_events,
                    tr("Envelope points with events"),
                    tr("Selecting events with the selection rectangle (the Selection Edit Tool, or a right-drag) or the Track Select tools picks the track envelopes' points in that time too: they move with the events and are deleted with them."),
                );
                row(
                    ui,
                    &mut mods.agents,
                    tr("AI agents (MCP)"),
                    tr("AI agents edit the open project through the Model Context Protocol: their program starts \"bananav mcp --live\". Only programs on this computer connect; saving, rendering and opening another project wait for your yes, and every edit is an undo step."),
                );
                ui.horizontal(|ui| {
                    ui.add_space(26.0);
                    copied = ui
                        .add_enabled(mods.agents, egui::Button::new(tr("Copy MCP configuration")))
                        .on_hover_text(tr("The settings an agent's program takes to start BananaV's MCP server (Claude Desktop's \"mcpServers\"), with this BananaV's path."))
                        .clicked();
                    let (on, failed) = app.agents_listening();
                    if let Some(e) = failed {
                        ui.colored_label(crate::theme::error(), trf!("Could not listen: {}", e));
                    } else if on {
                        ui.label(RichText::new(tr("Listening (this computer only)")).color(crate::theme::text_dim()));
                    }
                });
                ui.add_space(6.0);
                ui.add_space(4.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = ui
                        .add(egui::Button::new(tr("Close")).min_size(vec2(72.0, 0.0)))
                        .clicked();
                });
            });
        if mods != app.settings.mods {
            app.set_mods(mods);
        }
        if look {
            app.open_envelope_look();
        }
        if copied {
            ctx.copy_text(mcp_configuration());
            app.notify(tr("The MCP configuration was copied."));
        }
        open && !close
    }
}

/// The arrow after a mod's name that unfolds its description (`open`:
/// it shows).
fn fold_arrow(ui: &mut egui::Ui, open: bool, name: &str) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::click());
    let tip = if open {
        trf!("Hide the description of {}", name)
    } else {
        trf!("Show the description of {}", name)
    };
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &tip));
    let c = if resp.hovered() {
        crate::theme::text_strong()
    } else {
        crate::theme::text_dim()
    };
    let m = r.center();
    let points = if open {
        vec![
            m + vec2(-4.0, -2.0),
            m + vec2(4.0, -2.0),
            m + vec2(0.0, 3.0),
        ]
    } else {
        vec![
            m + vec2(-2.0, -4.0),
            m + vec2(3.0, 0.0),
            m + vec2(-2.0, 4.0),
        ]
    };
    ui.painter()
        .add(egui::Shape::convex_polygon(points, c, egui::Stroke::NONE));
    resp.on_hover_text(tip)
}

impl BananaApp {
    /// Options › BananaV Mods….
    pub(crate) fn open_mods(&mut self) {
        if self.menu_dialogs.more.mods.is_none() {
            self.menu_dialogs.more.mods = Some(ModsDialog::default());
        }
    }

    /// Options › BananaV Mods… open, the settings never saved from then on
    /// (screen tests).
    pub fn open_mods_for_review(&mut self) {
        self.settings = self.settings.unsaved();
        self.open_mods();
    }

    /// Uses `mods` from now on, and saves them: what a mod switched off
    /// showed goes (its window, its mode, its tool), the keyboard map
    /// follows.
    pub(crate) fn set_mods(&mut self, mods: Mods) {
        self.settings.mods = mods;
        let _ = self.settings.save();
        self.follow_mods();
    }

    /// What the mods switched off leave out: the Transform window and
    /// mode, the Track Select tools; the keyboard map's commands of the
    /// mods. Cheap: at every frame.
    pub(crate) fn follow_mods(&mut self) {
        let mods = self.settings.mods;
        if !mods.transform {
            if self.preview.transform {
                self.preview.transform = false;
                self.preview.transform_drag = None;
            }
            if self.is_tab_open(Tab::Transform) {
                self.show_tab(Tab::Transform, true);
            }
        }
        if !mods.track_select && matches!(self.tool, Tool::TrackSelect { .. }) {
            self.tool = Tool::Edit;
        }
        if self.keymap.mods() != mods {
            self.keymap.set_mods(mods);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All off at first, and when the settings file has none.
    #[test]
    fn mods_are_off_by_default() {
        assert_eq!(
            Mods::default(),
            Mods {
                transform: false,
                envelope_look: false,
                track_select: false,
                agents: false,
                env_with_events: false,
            }
        );
        let read: Mods = serde_json::from_str("{}").expect("an empty object");
        assert_eq!(read, Mods::default());
        let settings: crate::settings::UiSettings =
            serde_json::from_str("{}").expect("an empty settings file");
        assert_eq!(settings.mods, Mods::default());
    }

    /// Options › BananaV Mods…: a box ticked turns its mod on at once —
    /// its menu items show —, ticked again off (the settings are never
    /// saved here).
    #[test]
    fn the_window_turns_mods_on_and_off() {
        use egui_kittest::kittest::Queryable;
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        app.settings = app.settings.unsaved();
        app.set_mods(Mods::default());
        app.load_demo();
        app.open_mods();
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(1600.0, 960.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(4);
        for (name, on) in [
            (
                "Transform",
                Box::new(|m: &Mods| m.transform) as Box<dyn Fn(&Mods) -> bool>,
            ),
            ("Envelope Appearance", Box::new(|m: &Mods| m.envelope_look)),
            ("Track Select tools", Box::new(|m: &Mods| m.track_select)),
            ("AI agents (MCP)", Box::new(|m: &Mods| m.agents)),
            (
                "Envelope points with events",
                Box::new(|m: &Mods| m.env_with_events),
            ),
        ] {
            assert!(!on(&h.state().settings.mods), "{name} off at first");
            h.get_by_label(name).click();
            h.run_steps(3);
            assert!(on(&h.state().settings.mods), "{name} on");
        }
        assert!(crate::commands::Cmd::EnvelopeAppearance.enabled(h.state()));
        assert!(
            crate::commands::Cmd::UseTool(Tool::TrackSelect { back: false }).enabled(h.state())
        );
        h.get_by_label("Track Select tools").click();
        h.run_steps(3);
        assert!(!h.state().settings.mods.track_select, "off again");
        // Agents on: the window listens (its address in the tests' own
        // file), and stops when they are turned off.
        assert!(h.state().agents_listening().0, "listening");
        h.get_by_label("AI agents (MCP)").click();
        h.run_steps(3);
        assert!(!h.state().agents_listening().0, "not listening");
        assert!(
            h.state().menu_dialogs.more.mods.is_some(),
            "the window open"
        );
        // The descriptions folded at first; the arrow unfolds one.
        let about = "The Transform window (View";
        assert!(h.query_by_label_contains(about).is_none(), "folded");
        h.get_by_label("Show the description of Transform").click();
        h.run_steps(3);
        assert!(h.query_by_label_contains(about).is_some(), "unfolded");
        h.get_by_label("Hide the description of Transform").click();
        h.run_steps(3);
        assert!(h.query_by_label_contains(about).is_none(), "folded again");
        h.get_by_label("Close").click();
        h.run_steps(3);
        assert!(h.state().menu_dialogs.more.mods.is_none(), "closed");
    }
}
