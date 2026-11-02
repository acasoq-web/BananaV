//! Configure Mackie Control (Options › Preferences › External Control &
//! Automation, a double-click on a Mackie Control; VEGAS's Help, "Using a
//! Mackie Control"): its Device Type (a Mackie Control or an Extender),
//! the tracks its strips control (Channel Mapping), its MIDI ports, and
//! the user's mappings of its buttons to BananaV's commands — the keyboard
//! map's — over the defaults: Option with F1 to F16 (F9–F16 with Shift),
//! the footswitches and any other button. Assign, Clear, Clear All and
//! Default All as VEGAS's.

use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::egui::{self, RichText, vec2};
use crate::i18n::tr;
use crate::keymap::{Action, Context, registry};
use crate::settings::ControlDevice;
use crate::surface::{MAX_EXTENDERS, STRIPS, note};

/// A button of a Mackie Control mapped by the user, with Shift and Option
/// as held: to a command of the keyboard map, or to nothing (`None`:
/// cleared).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MackieMapping {
    pub note: u8,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shift: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub option: bool,
    pub command: Option<String>,
}

/// A control the user may map: its button, with Shift and Option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Control {
    pub note: u8,
    pub shift: bool,
    pub option: bool,
}

/// The buttons, named as the overlay labels them.
const BUTTONS: &[(u8, &str)] = &[
    (note::OUTPUT, "Output / Compositing"),
    (note::INPUT, "Input / Motion Blur"),
    (note::PAN, "Pan / Fade to Color"),
    (note::SENDS, "Sends"),
    (note::INSERTS, "Inserts"),
    (note::SETTINGS, "Settings"),
    (note::BANK_LEFT, "Bank <"),
    (note::BANK_RIGHT, "Bank >"),
    (note::CHANNEL_LEFT, "Channel <"),
    (note::CHANNEL_RIGHT, "Channel >"),
    (note::FLIP, "Flip"),
    (note::VIDEO, "Video"),
    (note::METERS, "Meters / Values"),
    (note::TIME_FORMAT, "Time Fmt / Zero"),
    (note::F1, "Marker 1/9 · Automation Mode"),
    (note::F1 + 1, "Marker 2/10 · Pan Mode"),
    (note::F1 + 2, "Marker 3/11 · Track Phase"),
    (note::F1 + 3, "Marker 4/12 · Input Monitor"),
    (note::F1 + 4, "Marker 5/13 · Pre/Post"),
    (note::F1 + 5, "Marker 6/14 · Default"),
    (note::F1 + 6, "Marker 7/15"),
    (note::F1 + 7, "Marker 8/16"),
    (note::ADD_TRACK, "Add New Track"),
    (note::ADD_BUS, "Add New Bus"),
    (note::MASTER_BUS, "Master Bus"),
    (note::VIDEO_PREVIEW, "Video Preview"),
    (note::PLUG_INS, "Plug-Ins"),
    (note::BUS_TRACKS, "Bus Tracks"),
    (note::DOCK_AREA, "Dock Area"),
    (note::TRACK_LIST, "Track List"),
    (note::AUTOMATION, "Automation"),
    (note::BYPASS_FX, "Bypass FX"),
    (note::METRONOME, "Metronome"),
    (note::SURROUND, "Surround / Ext Monitor"),
    (note::DOWNMIX, "Downmix / Split Screen"),
    (note::DIM, "Dim / Overlays"),
    (note::SAVE, "Save"),
    (note::UNDO, "Undo / Redo"),
    (note::OK, "OK"),
    (note::CANCEL, "Cancel"),
    (note::MARKER, "Marker / CD Index"),
    (note::REGION, "Region / CD Track"),
    (note::LOOP, "Loop / Select"),
    (note::MARK_IN, "Mark In / Go to In"),
    (note::MARK_OUT, "Mark Out / Go to Out"),
    (note::EVENT_TRIM, "Event Trim / Center Cursor"),
    (note::RTZ, "RTZ / End"),
    (note::REWIND, "Rewind"),
    (note::FAST_FORWARD, "Fast Forward"),
    (note::STOP, "Stop"),
    (note::PLAY, "Play"),
    (note::RECORD, "Record"),
    (note::UP, "Up"),
    (note::DOWN, "Down"),
    (note::LEFT, "Left"),
    (note::RIGHT, "Right"),
    (note::ZOOM, "Zoom"),
    (note::SCRUB, "Scrub"),
    (note::FOOT_A, "User Switch A"),
    (note::FOOT_B, "User Switch B"),
];

/// The controls of the User defined surface control mappings list: Option
/// with F1–F16 first, then the buttons.
pub(crate) fn controls() -> Vec<Control> {
    let f = (0..16u8).map(|k| Control {
        note: note::F1 + k % 8,
        shift: k >= 8,
        option: true,
    });
    f.chain(BUTTONS.iter().map(|&(n, _)| Control {
        note: n,
        shift: false,
        option: false,
    }))
    .collect()
}

/// A control's name in the list.
pub(crate) fn control_name(c: Control) -> String {
    if c.option {
        let k = c.note - note::F1 + 1 + if c.shift { 8 } else { 0 };
        return format!("Option+F{k}");
    }
    BUTTONS
        .iter()
        .find(|(n, _)| *n == c.note)
        .map_or_else(String::new, |(_, name)| tr(name).to_string())
}

/// The keyboard map's commands a control may run (the TrackView list's,
/// as Customize Keyboard shows them, then the menus' others), by their
/// place in the registry, in their names' order.
fn host_functions() -> Vec<usize> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<usize> = [Context::TrackView, Context::Global]
        .iter()
        .flat_map(|&ctx| {
            registry()
                .iter()
                .enumerate()
                .filter(move |(_, c)| c.context == ctx && c.action != Action::Soon)
        })
        .filter(|(_, c)| seen.insert(c.id.clone()))
        .map(|(i, _)| i)
        .collect();
    out.sort_by(|&a, &b| registry()[a].id.cmp(&registry()[b].id));
    out
}

/// Runs the keyboard map's command `id` (the TrackView list's, else the
/// Global one's).
pub(crate) fn run_host_function(app: &mut BananaApp, id: &str) {
    let action = [Context::TrackView, Context::Global]
        .iter()
        .find_map(|&ctx| {
            registry()
                .iter()
                .find(|c| c.context == ctx && c.id == id)
                .map(|c| c.action)
        });
    match action {
        Some(Action::Cmd(c)) => {
            if c.enabled(app) {
                c.run(app);
            }
        }
        Some(Action::Extra(e)) => app.run_extra(e),
        _ => {}
    }
}

/// What the Configure Mackie Control dialog shows between frames.
#[derive(Debug, Default)]
pub(crate) struct MackieUi {
    /// The control picked in the mappings list (its place).
    control: Option<usize>,
    /// The host function picked (its place in the registry).
    function: Option<usize>,
    filter: String,
}

impl MackieUi {
    /// The dialog with a control and a function picked (screen tests).
    pub(crate) fn picked(control: usize, function: &str) -> MackieUi {
        MackieUi {
            control: Some(control),
            function: host_functions()
                .into_iter()
                .find(|&i| registry()[i].id == function),
            filter: String::new(),
        }
    }
}

/// The Channel Mapping list's ranges: tracks 1–8, 9–16…
fn mappings() -> impl Iterator<Item = u32> {
    (0..=MAX_EXTENDERS as u32).map(|k| k * STRIPS as u32)
}

fn mapping_name(first: u32) -> String {
    trf!("Channels {}–{}", first + 1, first + STRIPS as u32)
}

/// The Configure Mackie Control dialog for `d` (its ports among `inputs`
/// and `outputs`); false once it is closed.
pub(crate) fn window(
    ctx: &egui::Context,
    app: &mut BananaApp,
    d: &mut ControlDevice,
    (inputs, outputs): (&[String], &[String]),
    s: &mut MackieUi,
) -> bool {
    // Each device shows its channel mapping meanwhile.
    app.mackie_mapping_shown = true;
    let mut open = true;
    let mut close = false;
    let none = tr("(None)");
    crate::floating::Window::new(tr("Configure Mackie Control"))
        .id(egui::Id::new("preferencias-mackie"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_min_width(640.0);
            ui.spacing_mut().item_spacing.y = 6.0;
            egui::Grid::new("mackie-topo")
                .num_columns(2)
                .spacing(vec2(10.0, 6.0))
                .show(ui, |ui| {
                    ui.label(tr("Device type:"));
                    let device_name = |ext: bool| {
                        if ext {
                            tr("Mackie Control Extender")
                        } else {
                            tr("Mackie Control")
                        }
                    };
                    egui::ComboBox::from_id_salt("mackie-tipo")
                        .width(240.0)
                        .selected_text(device_name(d.extender))
                        .show_ui(ui, |ui| {
                            for ext in [false, true] {
                                ui.selectable_value(&mut d.extender, ext, device_name(ext));
                            }
                        });
                    ui.end_row();
                    ui.label(tr("Channel mapping:"));
                    egui::ComboBox::from_id_salt("mackie-canais")
                        .width(240.0)
                        .selected_text(mapping_name(d.first_track))
                        .show_ui(ui, |ui| {
                            for first in mappings() {
                                ui.selectable_value(&mut d.first_track, first, mapping_name(first));
                            }
                        });
                    ui.end_row();
                    ui.label(tr("MIDI input:"));
                    crate::preferences::port_list(ui, "mackie-in", &mut d.input, inputs, 240.0);
                    ui.end_row();
                    ui.label(tr("MIDI output:"));
                    crate::preferences::port_list(ui, "mackie-out", &mut d.output, outputs, 240.0);
                    ui.end_row();
                });
            let controls = controls();
            let functions = host_functions();
            ui.columns(2, |cols| {
                // The controls and what they do.
                let ui = &mut cols[0];
                ui.label(tr("User defined surface control mappings:"));
                egui::Frame::new()
                    .stroke(egui::Stroke::new(1.0, crate::theme::separator()))
                    .inner_margin(4.0)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("mackie-controles")
                            .max_height(280.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                egui::Grid::new("mackie-controles-grade")
                                    .num_columns(2)
                                    .striped(true)
                                    .min_col_width(110.0)
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(tr("Control")).strong());
                                        ui.label(RichText::new(tr("Function")).strong());
                                        ui.end_row();
                                        for (i, c) in controls.iter().enumerate() {
                                            let picked = s.control == Some(i);
                                            if ui
                                                .add(
                                                    egui::Button::selectable(picked, control_name(*c))
                                                        .min_size(vec2(150.0, 18.0))
                                                        .right_text(""),
                                                )
                                                .clicked()
                                            {
                                                s.control = Some(i);
                                            }
                                            let mapped = d.mackie_map.iter().find(|m| {
                                                (m.note, m.shift, m.option) == (c.note, c.shift, c.option)
                                            });
                                            let text = match mapped {
                                                Some(m) => m.command.clone().unwrap_or_else(|| none.to_string()),
                                                None if c.option => none.to_string(),
                                                None => tr("(Default)").to_string(),
                                            };
                                            ui.label(text);
                                            ui.end_row();
                                        }
                                    });
                            });
                    });
                // BananaV's commands.
                let ui = &mut cols[1];
                ui.label(tr("Available host functions:"));
                ui.add(
                    egui::TextEdit::singleline(&mut s.filter)
                        .hint_text(tr("Search"))
                        .desired_width(f32::INFINITY),
                );
                let filter = s.filter.to_lowercase();
                egui::Frame::new()
                    .stroke(egui::Stroke::new(1.0, crate::theme::separator()))
                    .inner_margin(4.0)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("mackie-funcoes")
                            .max_height(254.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for &i in &functions {
                                    let id = &registry()[i].id;
                                    if !filter.is_empty() && !id.to_lowercase().contains(&filter) {
                                        continue;
                                    }
                                    let picked = s.function == Some(i);
                                    if ui
                                        .add(
                                            egui::Button::selectable(picked, id.as_str())
                                                .min_size(vec2(ui.available_width(), 18.0)),
                                        )
                                        .clicked()
                                    {
                                        s.function = Some(i);
                                    }
                                }
                            });
                    });
            });
            ui.horizontal(|ui| {
                let control = s.control.and_then(|i| controls.get(i)).copied();
                let set = |d: &mut ControlDevice, c: Control, command: Option<String>| {
                    d.mackie_map
                        .retain(|m| (m.note, m.shift, m.option) != (c.note, c.shift, c.option));
                    d.mackie_map.push(MackieMapping {
                        note: c.note,
                        shift: c.shift,
                        option: c.option,
                        command,
                    });
                };
                let can_assign = control.is_some() && s.function.is_some();
                if ui
                    .add_enabled(can_assign, egui::Button::new(tr("Assign")))
                    .clicked()
                    && let (Some(c), Some(f)) = (control, s.function)
                {
                    set(d, c, Some(registry()[f].id.clone()));
                }
                if ui
                    .add_enabled(control.is_some(), egui::Button::new(tr("Clear")))
                    .clicked()
                    && let Some(c) = control
                {
                    set(d, c, None);
                }
                if ui.button(tr("Clear All")).clicked() {
                    for c in &controls {
                        set(d, *c, None);
                    }
                }
                if ui.button(tr("Default All")).clicked() {
                    d.mackie_map.clear();
                }
            });
            crate::tabs::hint(
                ui,
                tr(
                    "Its ports are the ones made available in the MIDI tab. A Mackie Control takes up to four Extenders, each over its own eight tracks; Bank and Channel move them together. Options › External Control turns the surface on.",
                ),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Close")).clicked() {
                    close = true;
                }
            });
        });
    open && !close
}

impl BananaApp {
    /// Options › Preferences › External Control & Automation with a Mackie
    /// Control and an Extender, Configure Mackie Control open on the
    /// first, Option+F1 mapped to the metronome and footswitch A cleared
    /// (screen tests; the settings in memory).
    pub fn mackie_control_for_review(&mut self) {
        self.settings = crate::settings::UiSettings::in_memory();
        let device = |extender: bool, first_track: u32, mackie_map| ControlDevice {
            kind: crate::settings::SurfaceKind::MackieControl,
            input: None,
            output: None,
            first_track,
            generic: Default::default(),
            extender,
            mackie_map,
        };
        self.settings.prefs.control_devices = vec![
            device(
                false,
                0,
                vec![
                    MackieMapping {
                        note: note::F1,
                        shift: false,
                        option: true,
                        command: Some("Transport.LoopPlayback".into()),
                    },
                    MackieMapping {
                        note: note::FOOT_A,
                        shift: false,
                        option: false,
                        command: None,
                    },
                ],
            ),
            device(true, STRIPS as u32, Vec::new()),
        ];
        let mut d = crate::preferences::PreferencesDialog::new(self);
        d.mackie_for_review(0, MackieUi::picked(0, "Transport.LoopPlayback"));
        self.menu_dialogs.more.preferences = Some(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_control_has_a_name_and_runs_a_known_command() {
        let controls = controls();
        assert_eq!(controls.len(), 16 + BUTTONS.len());
        assert_eq!(control_name(controls[0]), "Option+F1");
        assert_eq!(control_name(controls[15]), "Option+F16");
        assert!(controls.iter().all(|c| !control_name(*c).is_empty()));
        // The commands offered are the keyboard map's, and run.
        let functions = host_functions();
        assert!(functions.len() > 100);
        let mut app = BananaApp::new(crate::egui::Context::default());
        let on = app.loop_enabled;
        run_host_function(&mut app, "Transport.LoopPlayback");
        assert_ne!(app.loop_enabled, on);
    }
}
