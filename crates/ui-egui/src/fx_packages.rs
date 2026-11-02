//! Filter packages: VEGAS's FX packages — an effect chain with all its
//! settings, saved under a name to apply again at any level (an event, a
//! track, a media file, the video output, a bus). Each is a file of its
//! own in a folder of the user's (`%APPDATA%\BananaV\FX Packages`), to
//! copy to other computers; the Video FX window's Filter Packages
//! category, the Plug-In Chooser and the Plug-In Manager list them. The
//! FX window's Save Plug-In Chain As Filter Package and the chooser's
//! Save As… name them (the Save Plug-In Package dialog).

use std::path::{Path, PathBuf};

use bv_timeline::Effect;
use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::egui::{self, RichText};
use crate::i18n::tr;
use crate::theme;

/// The packages' files' extension.
pub(crate) const EXTENSION: &str = "bvfx";

/// What a package's file says it is.
const FORMAT: &str = "BananaV filter package";

/// A saved chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Package {
    pub name: String,
    /// An audio chain's (audio chains' Plug-In Chooser lists it), else a
    /// video one's.
    pub audio: bool,
    pub chain: Vec<Effect>,
}

/// A package's file.
#[derive(Serialize, Deserialize)]
struct File {
    format: String,
    version: u32,
    #[serde(flatten)]
    package: Package,
}

/// The package files in a folder, and the dialogs about them.
#[derive(Debug, Default)]
pub(crate) struct FxPackages {
    /// Their folder; `None`: kept in memory only (tests).
    dir: Option<PathBuf>,
    /// Each with its file (none in memory), read when first asked for.
    list: Vec<(Package, Option<PathBuf>)>,
    read: bool,
    /// The Save Plug-In Package dialog (or Rename), while open.
    dialog: Option<NameDialog>,
}

/// The Save Plug-In Package dialog: a name for a chain, or a package's
/// new one.
#[derive(Debug)]
struct NameDialog {
    /// The chain to save; `None`: renaming package `renaming`.
    chain: Option<Vec<Effect>>,
    renaming: Option<String>,
    audio: bool,
    name: String,
    /// The name's field takes the keys when it opens.
    focus: bool,
}

impl FxPackages {
    /// The user's, in `%APPDATA%\BananaV\FX Packages`.
    pub(crate) fn load() -> FxPackages {
        FxPackages {
            dir: bv_platform::config_dir().map(|d| d.join("BananaV").join("FX Packages")),
            ..FxPackages::default()
        }
    }

    /// Those in `dir`.
    #[cfg(test)]
    pub(crate) fn at(dir: PathBuf) -> FxPackages {
        FxPackages {
            dir: Some(dir),
            ..FxPackages::default()
        }
    }

    /// Kept in memory only, never written (tests).
    pub(crate) fn in_memory() -> FxPackages {
        FxPackages {
            read: true,
            ..FxPackages::default()
        }
    }

    /// Their folder (none in memory).
    pub(crate) fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Reads the folder again when next asked (the Filter Packages
    /// category's Refresh: files copied in from elsewhere).
    pub(crate) fn refresh(&mut self) {
        if self.dir.is_some() {
            self.read = false;
        }
    }

    fn ensure(&mut self) {
        if self.read {
            return;
        }
        self.read = true;
        self.list.clear();
        let Some(entries) = self.dir.as_ref().and_then(|d| std::fs::read_dir(d).ok()) else {
            return;
        };
        for path in entries.flatten().map(|e| e.path()) {
            let ours = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION));
            // A file that is not one is left out.
            if ours && let Some(p) = read_file(&path) {
                self.list.push((p, Some(path)));
            }
        }
        self.sort();
    }

    fn sort(&mut self) {
        self.list
            .sort_by_cached_key(|(p, _)| (p.name.to_lowercase(), p.name.clone()));
    }

    fn position(&self, name: &str, audio: bool) -> Option<usize> {
        let name = name.trim();
        self.list
            .iter()
            .position(|(p, _)| p.audio == audio && p.name == name)
    }

    /// The packages of audio chains (`audio`) or of video ones, by name.
    pub(crate) fn list(&mut self, audio: bool) -> Vec<Package> {
        self.ensure();
        self.list
            .iter()
            .filter(|(p, _)| p.audio == audio)
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// Package `name` of audio chains (`audio`) or of video ones.
    pub(crate) fn get(&mut self, name: &str, audio: bool) -> Option<Package> {
        self.ensure();
        self.position(name, audio).map(|i| self.list[i].0.clone())
    }

    /// Saves `chain` as package `name`, replacing one of that name.
    pub(crate) fn save(
        &mut self,
        name: &str,
        audio: bool,
        chain: Vec<Effect>,
    ) -> std::io::Result<()> {
        self.ensure();
        let package = Package {
            name: name.trim().to_string(),
            audio,
            chain,
        };
        let at = self.position(&package.name, audio);
        let file = match &self.dir {
            None => None,
            Some(dir) => {
                let own = at.and_then(|i| self.list[i].1.clone());
                let file = match own {
                    Some(f) => f,
                    None => {
                        std::fs::create_dir_all(dir)?;
                        free_file(dir, &package.name, None)
                    }
                };
                write_file(&file, &package)?;
                Some(file)
            }
        };
        match at {
            Some(i) => self.list[i] = (package, file),
            None => self.list.push((package, file)),
        }
        self.sort();
        Ok(())
    }

    /// Package `name` named `new` (its file too); one already named so
    /// is replaced.
    pub(crate) fn rename(&mut self, name: &str, audio: bool, new: &str) -> std::io::Result<()> {
        self.ensure();
        let new = new.trim();
        let Some(i) = self.position(name, audio) else {
            return Ok(());
        };
        if new == name.trim() {
            return Ok(());
        }
        if self.position(new, audio).is_some() {
            self.remove(new, audio)?;
        }
        let Some(i) = self.position(name, audio).or(Some(i)) else {
            return Ok(());
        };
        let (mut package, old) = self.list[i].clone();
        package.name = new.to_string();
        let file = match &self.dir {
            None => None,
            Some(dir) => {
                std::fs::create_dir_all(dir)?;
                let file = free_file(dir, new, old.as_deref());
                write_file(&file, &package)?;
                if let Some(old) = old.filter(|o| *o != file) {
                    let _ = std::fs::remove_file(old);
                }
                Some(file)
            }
        };
        self.list[i] = (package, file);
        self.sort();
        Ok(())
    }

    /// Deletes package `name` and its file.
    pub(crate) fn remove(&mut self, name: &str, audio: bool) -> std::io::Result<()> {
        self.ensure();
        let Some(i) = self.position(name, audio) else {
            return Ok(());
        };
        if let Some(f) = &self.list[i].1 {
            match std::fs::remove_file(f) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        self.list.remove(i);
        Ok(())
    }

    /// Opens the Save Plug-In Package dialog for `chain`.
    pub(crate) fn ask_save(&mut self, chain: Vec<Effect>, audio: bool) {
        self.dialog = Some(NameDialog {
            chain: Some(chain),
            renaming: None,
            audio,
            name: String::new(),
            focus: true,
        });
    }

    /// Opens the dialog that renames package `name`.
    pub(crate) fn ask_rename(&mut self, name: &str, audio: bool) {
        self.dialog = Some(NameDialog {
            chain: None,
            renaming: Some(name.to_string()),
            audio,
            name: name.to_string(),
            focus: true,
        });
    }
}

/// A package's name as a file's: what Windows cannot have in one turned
/// into `_`, never a device's name.
fn file_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let s = s.trim().trim_end_matches('.').trim_end();
    let s = if s.is_empty() { "Package" } else { s };
    let upper = s.to_ascii_uppercase();
    let numbered = upper.len() == 4
        && (upper.starts_with("COM") || upper.starts_with("LPT"))
        && upper.as_bytes()[3].is_ascii_digit();
    if numbered || ["CON", "PRN", "AUX", "NUL"].contains(&upper.as_str()) {
        format!("{s}_")
    } else {
        s.to_string()
    }
}

/// The file for package `name` in `dir`: its name's, or with " (2)", …
/// when another file has it (`own`, the package's file so far, is free).
fn free_file(dir: &Path, name: &str, own: Option<&Path>) -> PathBuf {
    let stem = file_stem(name);
    (1..)
        .map(|n| {
            let file = if n == 1 {
                format!("{stem}.{EXTENSION}")
            } else {
                format!("{stem} ({n}).{EXTENSION}")
            };
            dir.join(file)
        })
        .find(|f| !f.exists() || own.is_some_and(|o| same_file(o, f)))
        .expect("a free name")
}

/// The same file, its name's letters in any case (as Windows has them).
fn same_file(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

fn read_file(path: &Path) -> Option<Package> {
    let text = std::fs::read_to_string(path).ok()?;
    let file: File = serde_json::from_str(&text).ok()?;
    (file.format == FORMAT).then_some(file.package)
}

fn write_file(path: &Path, package: &Package) -> std::io::Result<()> {
    let file = File {
        format: FORMAT.to_string(),
        version: 1,
        package: package.clone(),
    };
    let text = serde_json::to_string_pretty(&file).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

/// The Save Plug-In Package dialog (or the one renaming a package), while
/// open.
pub(crate) fn dialog(ctx: &egui::Context, app: &mut BananaApp) {
    let Some(mut d) = app.fx_packages.dialog.take() else {
        return;
    };
    let name = d.name.trim().to_string();
    let taken = !name.is_empty()
        && d.renaming.as_deref() != Some(name.as_str())
        && app.fx_packages.get(&name, d.audio).is_some();
    let title = if d.renaming.is_some() {
        tr("Rename Filter Package")
    } else {
        tr("Save Plug-In Package")
    };
    let (mut open, mut ok, mut cancel) = (true, false, false);
    crate::floating::Window::new(title)
        .id(egui::Id::new("fx-package-name"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(tr("Name:"));
            let edit = ui.add(
                egui::TextEdit::singleline(&mut d.name)
                    .desired_width(280.0)
                    .hint_text(tr("The filter package's name")),
            );
            if std::mem::take(&mut d.focus) {
                edit.request_focus();
            }
            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                ok = true;
            }
            if taken {
                ui.label(
                    RichText::new(tr("A filter package has this name: OK replaces it."))
                        .color(theme::error()),
                );
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let can = !d.name.trim().is_empty();
                if ui
                    .add_enabled(can, egui::Button::new(RichText::new("OK").strong()))
                    .clicked()
                {
                    ok = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    let name = d.name.trim().to_string();
    if ok && !name.is_empty() {
        let done = match (d.chain.take(), &d.renaming) {
            (Some(chain), _) => app.fx_packages.save(&name, d.audio, chain),
            (None, Some(old)) => app.fx_packages.rename(old, d.audio, &name),
            (None, None) => Ok(()),
        };
        match done {
            Ok(()) if d.renaming.is_some() => {
                app.notify(trf!("Filter package renamed: {}", name));
            }
            Ok(()) => app.notify(trf!("Saved as the filter package {}.", name)),
            Err(e) => app.report_error(trf!("Could not save the filter package: {}", e)),
        }
    } else if open && !cancel {
        app.fx_packages.dialog = Some(d);
    }
}

/// Asks before package `name` and its file are deleted.
pub(crate) fn ask_delete(app: &mut BananaApp, name: &str, audio: bool) {
    app.ask_user(crate::ask::Ask {
        title: tr("Delete Filter Package").into(),
        text: trf!(
            "Delete the filter package {}? Its file is deleted from the filter packages' folder.",
            name
        ),
        what: crate::ask::Asked::DeleteFxPackage {
            name: name.to_string(),
            audio,
        },
        check: None,
    });
}

/// Opens the packages' folder in the system's file manager (made first
/// when there is none yet): the Filter Packages category's Explore
/// Effect Filters Folder.
pub(crate) fn explore(app: &mut BananaApp) {
    let Some(dir) = app.fx_packages.dir().map(Path::to_path_buf) else {
        return;
    };
    let opened = std::fs::create_dir_all(&dir)
        .and_then(|()| crate::app::more_actions::open_externally(&dir));
    if let Err(e) = opened {
        app.report_error(trf!("Could not open the folder: {}", e));
    }
}

impl BananaApp {
    /// Filter packages kept in memory, never read from or written to the
    /// user's folder (screen tests).
    pub fn fx_packages_for_review(&mut self) {
        self.fx_packages = FxPackages::in_memory();
    }

    /// The Video FX window's category `name` chosen ("Third Party",
    /// "Filter Packages"…), for screen tests.
    pub fn video_fx_category_for_review(&mut self, name: &str) {
        let names = [
            "AI/ML",
            "Creative",
            "Color",
            "Utility",
            "Blur",
            "360°",
            "Third Party",
            "Filter Packages",
        ];
        if let Some(i) = names.iter().position(|n| *n == name) {
            self.choosers.effects.category = i + 2;
            self.choosers.effects.provider = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::EffectKind;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bv-fx-packages-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn chain() -> Vec<Effect> {
        let mut sepia = Effect::new(EffectKind::Sepia);
        sepia.enabled = false;
        vec![Effect::new(EffectKind::BrightnessContrast), sepia]
    }

    #[test]
    fn packages_are_files_of_their_own_read_back_by_name() {
        let dir = temp("files");
        let mut packages = FxPackages::at(dir.clone());
        assert!(packages.list(false).is_empty(), "no folder yet: none");
        packages.save("Warm look", false, chain()).unwrap();
        packages
            .save("Voice", true, vec![Effect::new(EffectKind::Gain)])
            .unwrap();
        // The folder holds a file each; another reading finds them, with
        // their settings, the bypassed effect bypassed.
        let files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(files.len(), 2);
        assert!(dir.join("Warm look.bvfx").exists());
        let mut again = FxPackages::at(dir.clone());
        let video = again.list(false);
        assert_eq!(video.len(), 1);
        assert_eq!(video[0].name, "Warm look");
        assert_eq!(video[0].chain, chain());
        assert_eq!(again.list(true)[0].name, "Voice");
        // Saved again under its name: replaced, still one file.
        again
            .save("Warm look", false, vec![Effect::new(EffectKind::Invert)])
            .unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        assert_eq!(again.get("Warm look", false).unwrap().chain.len(), 1);
        // A file that is not a package is left out; Refresh finds a file
        // copied in.
        std::fs::write(dir.join("junk.bvfx"), "not a package").unwrap();
        std::fs::copy(dir.join("Warm look.bvfx"), dir.join("Copy.bvfx")).unwrap();
        again.refresh();
        assert_eq!(
            again.list(false).len(),
            2,
            "the copy (named as the original) and the original"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn renaming_and_deleting_follow_the_files() {
        let dir = temp("rename");
        let mut packages = FxPackages::at(dir.clone());
        packages.save("A", false, chain()).unwrap();
        packages
            .save("B", false, vec![Effect::new(EffectKind::Invert)])
            .unwrap();
        packages.rename("A", false, "Night").unwrap();
        assert!(dir.join("Night.bvfx").exists() && !dir.join("A.bvfx").exists());
        assert_eq!(
            FxPackages::at(dir.clone())
                .get("Night", false)
                .map(|p| p.chain),
            Some(chain())
        );
        // Onto another's name: that one is replaced.
        packages.rename("Night", false, "B").unwrap();
        let left = packages.list(false);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].chain, chain());
        packages.remove("B", false).unwrap();
        assert!(packages.list(false).is_empty());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_windows_cannot_have_are_made_files_it_can() {
        assert_eq!(file_stem("Look: warm/cool?"), "Look_ warm_cool_");
        assert_eq!(file_stem("  Final. "), "Final");
        assert_eq!(file_stem("con"), "con_");
        assert_eq!(file_stem("COM1"), "COM1_");
        assert_eq!(file_stem("..."), "Package");
        // Two names, one file name: the second gets a number.
        let dir = temp("names");
        let mut packages = FxPackages::at(dir.clone());
        packages.save("a/b", false, chain()).unwrap();
        packages.save("a?b", false, chain()).unwrap();
        assert!(dir.join("a_b.bvfx").exists() && dir.join("a_b (2).bvfx").exists());
        assert_eq!(FxPackages::at(dir.clone()).list(false).len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn in_memory_nothing_is_written() {
        let mut packages = FxPackages::in_memory();
        packages.save("Look", false, chain()).unwrap();
        packages.rename("Look", false, "Look 2").unwrap();
        assert_eq!(packages.list(false)[0].name, "Look 2");
        assert!(packages.dir().is_none());
        packages.remove("Look 2", false).unwrap();
        assert!(packages.list(false).is_empty());
    }

    #[test]
    fn a_chain_saved_as_a_package_goes_onto_other_events_and_replaces_a_chain() {
        use crate::egui::{self, vec2};
        use bv_timeline::TrackKind;
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        app.load_demo();
        let p = app.project();
        let first = app.primary_clip().expect("the demo's selected event");
        let chain = p.clip(first).unwrap().effects.clone();
        assert!(!chain.is_empty(), "the demo's event has effects");
        let other = p
            .clips()
            .find(|(t, c)| t.kind == TrackKind::Video && c.id != first)
            .map(|(_, c)| c.id)
            .expect("another video event");
        let before = p.clip(other).unwrap().effects.len();
        let mut h = Harness::builder()
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
        // The event's FX window: Save Plug-In Chain As Filter Package, a
        // name typed, OK.
        h.state_mut().open_clip_window("fx");
        h.run_steps(4);
        h.get_by_label("Save Plug-In Chain As Filter Package")
            .click();
        h.run_steps(4);
        h.event(egui::Event::Text("Warm look".into()));
        h.run_steps(2);
        h.get_by_label("OK").click();
        h.run_steps(3);
        let saved = h
            .state_mut()
            .fx_packages
            .get("Warm look", false)
            .expect("saved");
        assert_eq!(saved.chain, chain);
        h.get_by_label("Close window").click();
        h.run_steps(3);
        // Video FX › Filter Packages: the package, its one preset; its
        // menu's Apply to Selected Events puts the chain on the other event.
        {
            let app = h.state_mut();
            app.selection.clear();
            app.select_clip(other, false);
            app.choosers.effects.category = super::super::tabs::effects::FILTER_PACKAGES + 2;
            app.show_tab_named("Video FX");
        }
        h.run_steps(6);
        // The preset (the list's row has the name too).
        let cell = h
            .get_all_by_label("Warm look")
            .find(|n| n.rect().height() > 60.0)
            .expect("the package's preset");
        cell.click_secondary();
        h.run_steps(3);
        h.get_by_label("Apply to Selected Events").click();
        h.run_steps(3);
        let effects = h.state().project().clip(other).unwrap().effects.clone();
        assert_eq!(effects.len(), before + chain.len());
        assert_eq!(effects[before..], chain[..]);
        // The Plug-In Chooser: the package in its folder; Replace makes it
        // the whole chain, OK gives it to the event.
        h.state_mut().show_tab_named("Project Media");
        h.state_mut().open_clip_window("fx");
        h.run_steps(4);
        h.get_by_label("Plug-In Chain…").click();
        h.run_steps(4);
        h.get_by_label("Filter Packages").click();
        h.run_steps(2);
        h.get_by_label("Warm look").click();
        h.run_steps(2);
        h.get_by_label("Replace").click();
        h.run_steps(2);
        h.get_by_label("OK").click();
        h.run_steps(3);
        assert_eq!(h.state().project().clip(other).unwrap().effects, chain);
    }
}
