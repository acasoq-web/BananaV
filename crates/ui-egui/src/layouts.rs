//! Window layouts kept as files, as VEGAS's (View › Window Layouts): a
//! layout is a `.bvlayout` file — its name and the places and sizes of the
//! windows, docked and floating, the timeline's among them — in a folder
//! (`Layouts` beside BananaV's settings unless another is chosen). Up to
//! ten are in the menu, recalled with Alt+D and their number (Ctrl+Alt+D
//! and the number saves there). Save Layout As names a layout and gives it
//! its number and its folder; Organize Layouts lists the folder's layouts
//! and the menu's, and assigns, moves, activates, clears and deletes them.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::egui::{self, Id, RichText, Ui, vec2};
use crate::i18n::tr;
use crate::settings::{SavedLayout, UiSettings};
use crate::theme;

/// The layout files' extension.
pub const EXTENSION: &str = "bvlayout";

/// What a layout file holds.
#[derive(Serialize, Deserialize)]
struct LayoutFile {
    /// What the file is, for whoever opens it.
    kind: String,
    name: String,
    dock: serde_json::Value,
    #[serde(default)]
    timeline_top: bool,
}

const KIND: &str = "BananaV window layout";

/// The file a layout named `name` goes to in `folder` (what file names
/// cannot hold becomes `_`).
pub fn path_for(folder: &Path, name: &str) -> PathBuf {
    let clean: String = name
        .chars()
        .map(|c| {
            if r#"<>:"/\|?*"#.contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_end_matches('.');
    let stem = if clean.is_empty() { "Layout" } else { clean };
    folder.join(format!("{stem}.{EXTENSION}"))
}

pub fn write(path: &Path, layout: &SavedLayout) -> Result<(), String> {
    let file = LayoutFile {
        kind: KIND.into(),
        name: layout.name.clone(),
        dock: layout.dock.clone(),
        timeline_top: layout.timeline_top,
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

pub fn read(path: &Path) -> Result<SavedLayout, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let file: LayoutFile = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(SavedLayout {
        name: file.name,
        dock: file.dock,
        timeline_top: file.timeline_top,
        file: Some(path.to_path_buf()),
    })
}

/// The layouts of `folder`, by name.
pub fn list(folder: &Path) -> Vec<SavedLayout> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<SavedLayout> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|x| x.eq_ignore_ascii_case(EXTENSION))
        })
        .filter_map(|p| read(&p).ok())
        .collect();
    found.sort_by_key(|l| l.name.to_lowercase());
    found
}

/// A layout's windows from what was saved. The panes' places (worked out
/// again each frame) of a layout saved before it was drawn are NaN, which
/// JSON keeps as `null`: they come back as 0.
pub fn dock_from_value(
    value: &serde_json::Value,
) -> Result<egui_dock::DockState<crate::dock::Tab>, String> {
    fn fill(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(map) => {
                for (key, item) in map.iter_mut() {
                    if item.is_null() && (key == "x" || key == "y") {
                        *item = serde_json::Value::from(0.0);
                    } else {
                        fill(item);
                    }
                }
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(fill),
            _ => {}
        }
    }
    let mut value = value.clone();
    fill(&mut value);
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// "Alt+D, 4": the keys that recall slot `n`.
fn shortcut(n: usize) -> String {
    format!("Alt+D, {n}")
}

/// Save Layout As, as VEGAS's: the layout's name, its number in the menu
/// (Alt+D and it recalls the layout) and the folder its file goes to.
#[derive(Debug)]
pub struct SaveAs {
    name: String,
    slot: usize,
    folder: String,
}

impl SaveAs {
    /// For the layout in use (`current`), else the first free number.
    pub fn new(settings: &UiSettings, current: Option<usize>) -> SaveAs {
        let slot = current
            .or_else(|| settings.layouts.iter().position(Option::is_none))
            .unwrap_or(0);
        let name = settings
            .layouts
            .get(slot)
            .and_then(|l| l.as_ref())
            .filter(|_| current.is_some())
            .map_or_else(|| trf!("Layout {}", slot), |l| l.name.clone());
        let folder = settings
            .layouts_folder()
            .map_or(String::new(), |p| p.display().to_string());
        SaveAs { name, slot, folder }
    }

    /// Shows the window; `false` once it closes.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let mut open = true;
        let mut ok = false;
        let mut cancel = false;
        crate::floating::Window::new(tr("Save Layout As"))
            .id(Id::new("save-layout-as"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("save-layout-as-grid")
                    .num_columns(2)
                    .spacing(vec2(8.0, 8.0))
                    .show(ui, |ui| {
                        ui.label(tr("Name:"));
                        ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(300.0));
                        ui.end_row();
                        ui.label(tr("Shortcut:"));
                        egui::ComboBox::from_id_salt("save-layout-slot")
                            .width(300.0)
                            .selected_text(shortcut(self.slot))
                            .show_ui(ui, |ui| {
                                for n in 0..10 {
                                    let used = app
                                        .settings
                                        .layouts
                                        .get(n)
                                        .and_then(|l| l.as_ref())
                                        .map_or(String::new(), |l| format!("  ({})", l.name));
                                    ui.selectable_value(
                                        &mut self.slot,
                                        n,
                                        format!("{}{used}", shortcut(n)),
                                    );
                                }
                            });
                        ui.end_row();
                        ui.label(tr("Folder:"));
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.folder).desired_width(220.0),
                            );
                            if ui.button(tr("Browse…")).clicked() {
                                let mut picker = rfd::FileDialog::new();
                                if !self.folder.trim().is_empty() {
                                    picker = picker.set_directory(self.folder.trim());
                                }
                                if let Some(dir) = picker.pick_folder() {
                                    self.folder = dir.display().to_string();
                                }
                            }
                        });
                        ui.end_row();
                    });
                ui.add_space(10.0);
                // One row (a bare right-to-left layout takes all the
                // height there is).
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        cancel |= ui
                            .add(egui::Button::new(tr("Cancel")).min_size(vec2(80.0, 24.0)))
                            .clicked();
                        ok = ui
                            .add_enabled(
                                !self.name.trim().is_empty(),
                                egui::Button::new(tr("OK")).min_size(vec2(80.0, 24.0)),
                            )
                            .clicked();
                    });
                });
            });
        if ok {
            let folder = Some(self.folder.trim())
                .filter(|f| !f.is_empty())
                .map(PathBuf::from);
            app.save_layout_as(self.slot, self.name.trim().to_string(), folder);
            return false;
        }
        open && !cancel
    }
}

/// Which list's layout is picked in Organize Layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Picked {
    File(usize),
    Slot(usize),
}

/// Organize Layouts, as VEGAS's: the layouts of the folder and the ones in
/// the menu; Assign (Replace) puts the folder's picked layout at the
/// menu's picked number, Move Up and Move Down reorder the menu, Activate
/// puts the picked layout's windows in place, Clear takes it out of the
/// menu and Delete Layout deletes a file the menu does not hold. OK keeps
/// the menu as it was left.
#[derive(Debug)]
pub struct Organize {
    folder: String,
    available: Vec<SavedLayout>,
    menu: Vec<Option<SavedLayout>>,
    picked: Option<Picked>,
    /// The menu's picked number (where Assign puts a layout).
    slot: usize,
}

impl Organize {
    pub fn new(settings: &UiSettings) -> Organize {
        let folder = settings
            .layouts_folder()
            .map_or(String::new(), |p| p.display().to_string());
        let mut menu = settings.layouts.clone();
        menu.resize(10, None);
        Organize {
            available: list(Path::new(&folder)),
            folder,
            menu,
            picked: None,
            slot: 0,
        }
    }

    fn picked_layout(&self) -> Option<&SavedLayout> {
        match self.picked? {
            Picked::File(i) => self.available.get(i),
            Picked::Slot(n) => self.menu.get(n)?.as_ref(),
        }
    }

    /// Whether the menu holds the file `path`.
    fn in_menu(&self, path: Option<&Path>) -> bool {
        path.is_some_and(|p| {
            self.menu
                .iter()
                .flatten()
                .any(|l| l.file.as_deref() == Some(p))
        })
    }

    /// Shows the window; `false` once it closes.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let mut open = true;
        let mut ok = false;
        let mut cancel = false;
        let mut activate = false;
        crate::floating::Window::new(tr("Organize Layouts"))
            .id(Id::new("organize-layouts"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("Current layout folder:"));
                    let r =
                        ui.add(egui::TextEdit::singleline(&mut self.folder).desired_width(360.0));
                    let mut reload = r.lost_focus();
                    if ui.button(tr("Browse…")).clicked() {
                        let mut picker = rfd::FileDialog::new();
                        if !self.folder.trim().is_empty() {
                            picker = picker.set_directory(self.folder.trim());
                        }
                        if let Some(dir) = picker.pick_folder() {
                            self.folder = dir.display().to_string();
                            reload = true;
                        }
                    }
                    if reload {
                        self.available = list(Path::new(self.folder.trim()));
                        self.picked = None;
                    }
                });
                ui.add_space(6.0);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.label(tr("Available layouts in current folder:"));
                        activate |= self.available_list(ui);
                        ui.add_space(4.0);
                        ui.horizontal(|ui| self.file_buttons(ui, app));
                    });
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.label(tr("Current layouts in menu:"));
                        activate |= self.menu_list(ui);
                        ui.add_space(4.0);
                        ui.horizontal(|ui| self.menu_buttons(ui));
                    });
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    activate |= ui
                        .add_enabled(
                            self.picked_layout().is_some(),
                            egui::Button::new(tr("Activate")),
                        )
                        .clicked();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        cancel |= ui
                            .add(egui::Button::new(tr("Cancel")).min_size(vec2(80.0, 24.0)))
                            .clicked();
                        ok |= ui
                            .add(egui::Button::new(tr("OK")).min_size(vec2(80.0, 24.0)))
                            .clicked();
                    });
                });
            });
        if activate && let Some(layout) = self.picked_layout().cloned() {
            let slot = match self.picked {
                Some(Picked::Slot(n)) => Some(n),
                _ => None,
            };
            app.activate_layout(&layout, slot);
        }
        if ok {
            app.organized_layouts(self.menu.clone(), PathBuf::from(self.folder.trim()));
            return false;
        }
        open && !cancel
    }

    /// The folder's layouts; `true` when one was double-clicked.
    fn available_list(&mut self, ui: &mut Ui) -> bool {
        let mut activate = false;
        list_frame(ui, "available-layouts", |ui| {
            if self.available.is_empty() {
                ui.label(RichText::new(tr("(None)")).color(theme::text_dim()));
            }
            for (i, l) in self.available.iter().enumerate() {
                let r = ui.selectable_label(self.picked == Some(Picked::File(i)), &l.name);
                if r.clicked() {
                    self.picked = Some(Picked::File(i));
                }
                if r.double_clicked() {
                    self.picked = Some(Picked::File(i));
                    activate = true;
                }
            }
        });
        activate
    }

    /// The menu's ten numbers; `true` when one was double-clicked.
    fn menu_list(&mut self, ui: &mut Ui) -> bool {
        let mut activate = false;
        list_frame(ui, "menu-layouts", |ui| {
            for n in 0..10 {
                let text = match &self.menu[n] {
                    Some(l) => format!("{n}  {}", l.name),
                    None => format!("{n}  {}", tr("<EMPTY>")),
                };
                let r = ui.selectable_label(self.slot == n, text);
                if r.clicked() {
                    self.slot = n;
                    self.picked = Some(Picked::Slot(n));
                }
                if r.double_clicked() && self.menu[n].is_some() {
                    activate = true;
                }
            }
        });
        activate
    }

    fn file_buttons(&mut self, ui: &mut Ui, app: &mut BananaApp) {
        let file = match self.picked {
            Some(Picked::File(i)) => self.available.get(i).cloned(),
            _ => None,
        };
        // Into the menu's picked number.
        let label = if self.menu[self.slot].is_some() {
            tr("Replace")
        } else {
            tr("Assign")
        };
        if ui
            .add_enabled(file.is_some(), egui::Button::new(label))
            .on_hover_text(trf!(
                "Puts the layout at {} in the menu.",
                shortcut(self.slot)
            ))
            .clicked()
            && let Some(f) = &file
        {
            self.menu[self.slot] = Some(f.clone());
        }
        // Only files the menu does not hold.
        let held = self.in_menu(file.as_ref().and_then(|f| f.file.as_deref()));
        let delete = ui
            .add_enabled(
                file.is_some() && !held,
                egui::Button::new(tr("Delete Layout")),
            )
            .on_disabled_hover_text(tr(
                "A layout in the menu is cleared from it before it is deleted.",
            ));
        if delete.clicked()
            && let Some(path) = file.and_then(|f| f.file)
        {
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    self.available = list(Path::new(self.folder.trim()));
                    self.picked = None;
                }
                Err(e) => app.report_error(trf!("Could not delete {}: {}", path.display(), e)),
            }
        }
    }

    fn menu_buttons(&mut self, ui: &mut Ui) {
        let n = self.slot;
        if ui
            .add_enabled(n > 0, egui::Button::new(tr("Move Up")))
            .clicked()
        {
            self.menu.swap(n, n - 1);
            self.slot = n - 1;
            self.picked = Some(Picked::Slot(n - 1));
        }
        if ui
            .add_enabled(n < 9, egui::Button::new(tr("Move Down")))
            .clicked()
        {
            self.menu.swap(n, n + 1);
            self.slot = n + 1;
            self.picked = Some(Picked::Slot(n + 1));
        }
        if ui
            .add_enabled(self.menu[n].is_some(), egui::Button::new(tr("Clear")))
            .clicked()
        {
            self.menu[n] = None;
        }
    }
}

/// A list's framed, scrolling box (`id`: its own).
fn list_frame(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(theme::panel_dark())
        .stroke(egui::Stroke::new(1.0, theme::separator()))
        .inner_margin(4.0)
        .show(ui, |ui| {
            ui.set_width(260.0);
            ui.set_height(240.0);
            egui::ScrollArea::vertical().id_salt(id).show(ui, add);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_go_to_files_and_come_back() {
        let dir = std::env::temp_dir().join(format!("bananav-layouts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let layout = SavedLayout {
            name: "Edit: big preview?".into(),
            dock: serde_json::to_value(crate::dock::default_layout()).unwrap(),
            timeline_top: true,
            file: None,
        };
        let path = path_for(&dir, &layout.name);
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            "Edit_ big preview_.bvlayout",
            "what file names cannot hold"
        );
        write(&path, &layout).unwrap();
        let back = read(&path).unwrap();
        assert_eq!(back.name, layout.name);
        // Never drawn: its panes' places are NaN, saved as null.
        let dock = dock_from_value(&back.dock).unwrap();
        let tabs = |d: &egui_dock::DockState<crate::dock::Tab>| {
            d.iter_all_tabs().map(|(_, t)| *t).collect::<Vec<_>>()
        };
        assert_eq!(tabs(&dock), tabs(&crate::dock::default_layout()));
        assert!(back.timeline_top);
        assert_eq!(back.file.as_deref(), Some(path.as_path()));
        let found = list(&dir);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, layout.name);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Save Layout As writes the file and marks the layout in use; Reload
    /// Selected Layout brings it back; Organize Layouts' OK keeps it marked
    /// where it moved (the settings are the tests', never written).
    #[test]
    fn saved_layouts_are_files_the_menu_recalls() {
        use crate::app::actions::LayoutPreset;
        use crate::dock::Tab;
        let dir = std::env::temp_dir().join(format!("bananav-menu-layouts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = BananaApp::new(egui::Context::default());
        app.settings.layout_folder = Some(dir.clone());
        app.apply_layout(LayoutPreset::SourceWindow);
        app.save_layout_as(3, "Mine".into(), None);
        assert!(dir.join("Mine.bvlayout").exists());
        assert_eq!(app.layout_slot, Some(3));
        assert_eq!(
            app.settings.layouts[3].as_ref().map(|l| l.name.as_str()),
            Some("Mine")
        );
        app.apply_layout(LayoutPreset::AudioMixing);
        assert_eq!(app.layout_slot, None, "a built-in layout");
        app.load_layout(3);
        app.reload_layout();
        let dock = app.dock.as_ref().unwrap();
        assert!(dock.find_tab(&Tab::Trimmer).is_some());
        assert!(dock.find_tab(&Tab::MixingConsole).is_none());
        // Moved to 5 in Organize Layouts: still the one in use.
        let mut menu = app.settings.layouts.clone();
        menu.swap(3, 5);
        app.organized_layouts(menu, dir.clone());
        assert_eq!(app.layout_slot, Some(5));
        assert_eq!(list(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
