//! Options › Preferences › General: "Enable autosave". Every few minutes a
//! changed project is copied to the temporary files folder. BananaV deletes
//! its copy when it closes normally, so a copy found when it starts means it
//! did not: BananaV offers to open it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bv_timeline::Project;

use crate::app::BananaApp;
use crate::ask::{Ask, Asked};
use crate::i18n::tr;
use crate::settings::Prefs;

/// How often a changed project is copied.
pub(crate) const EVERY: Duration = Duration::from_secs(5 * 60);

/// When the project was copied last, and as what.
#[derive(Debug)]
pub(crate) struct Autosave {
    at: Instant,
    last: Option<Project>,
}

impl Default for Autosave {
    fn default() -> Autosave {
        Autosave {
            at: Instant::now(),
            last: None,
        }
    }
}

/// Where the copies go.
fn folder(prefs: &Prefs) -> PathBuf {
    prefs.temp_dir().join("Autosave")
}

/// This session's copy, and the file telling which project it is.
fn files(prefs: &Prefs, session: u32) -> (PathBuf, PathBuf) {
    let dir = folder(prefs);
    (
        dir.join(format!("{session}.bvproj")),
        dir.join(format!("{session}.json")),
    )
}

/// The project a copy is of.
#[derive(serde::Serialize, serde::Deserialize)]
struct Original {
    path: Option<PathBuf>,
}

impl BananaApp {
    /// Copies the project when it changed since the last copy and enough
    /// time went by.
    pub(crate) fn autosave_tick(&mut self) {
        if !self.settings.prefs.autosave || self.autosave.at.elapsed() < EVERY {
            return;
        }
        self.autosave.at = Instant::now();
        let project = self.project().clone();
        if !self.is_dirty() || self.autosave.last.as_ref() == Some(&project) {
            return;
        }
        let (file, about) = files(&self.settings.prefs, std::process::id());
        let json = project.to_json();
        let original = serde_json::to_string(&Original {
            path: self.project_path.clone(),
        })
        .unwrap_or_default();
        self.autosave.last = Some(project);
        // Written off the interface's thread: a large project takes a moment.
        std::thread::spawn(move || {
            let write = || -> std::io::Result<()> {
                if let Some(dir) = file.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                let part = file.with_extension("part");
                std::fs::write(&part, json)?;
                std::fs::rename(&part, &file)?;
                std::fs::write(&about, original)
            };
            let _ = write();
        });
    }

    /// Deletes this session's copy (BananaV closes normally).
    pub(crate) fn autosave_cleanup(&self) {
        let (file, about) = files(&self.settings.prefs, std::process::id());
        let _ = std::fs::remove_file(file);
        let _ = std::fs::remove_file(about);
    }

    /// At startup: a copy left by a session that did not close normally is
    /// offered back.
    pub(crate) fn offer_recovery(&mut self) {
        let Ok(entries) = std::fs::read_dir(folder(&self.settings.prefs)) else {
            return;
        };
        let newest = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "bvproj"))
            .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
        let Some(file) = newest else {
            return;
        };
        let original = std::fs::read_to_string(file.with_extension("json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Original>(&s).ok())
            .and_then(|o| o.path);
        let name = original.as_deref().and_then(Path::file_name).map_or_else(
            || tr("Untitled").to_string(),
            |n| n.to_string_lossy().to_string(),
        );
        self.ask_user(Ask {
            title: tr("Autosave").into(),
            text: trf!(
                "BananaV did not close normally last time. Open the copy of {} it saved automatically?",
                name
            ),
            what: Asked::Recover { file, original },
            check: None,
        });
    }

    /// Opens a copy saved automatically, as unsaved changes to its project.
    pub(crate) fn recover(&mut self, file: &Path, original: Option<PathBuf>) {
        let loaded = std::fs::read_to_string(file)
            .map_err(|e| e.to_string())
            .and_then(|s| Project::from_json(&s).map_err(|e| e.to_string()));
        match loaded {
            Ok(p) => {
                self.stop();
                // Unsaved: told apart from the project as it was saved.
                let saved = original
                    .as_deref()
                    .and_then(|o| std::fs::read_to_string(o).ok())
                    .and_then(|s| Project::from_json(&s).ok())
                    .unwrap_or_default();
                self.history.reset(p);
                self.saved = saved;
                self.project_path = original;
                self.fit_timeline = true;
                forget(file);
            }
            Err(e) => self.report_error(trf!("Could not open the project: {}", e)),
        }
    }
}

/// Deletes a copy saved automatically.
pub(crate) fn forget(file: &Path) {
    let _ = std::fs::remove_file(file);
    let _ = std::fs::remove_file(file.with_extension("json"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_left_behind_is_offered_back() {
        let dir =
            std::env::temp_dir().join(format!("bananav-autosave-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.settings.prefs.temp_folder = Some(dir.clone());
        app.load_demo();
        app.modify(|p| p.add_marker(bv_core::Tick::from_seconds(1)));
        // Five minutes later, the changed project is copied.
        app.autosave.at = Instant::now() - EVERY;
        app.autosave_tick();
        let (file, _) = files(&app.settings.prefs, std::process::id());
        let start = Instant::now();
        while !file.exists() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(file.exists(), "copied");
        // BananaV did not close normally: the next start offers it.
        let mut next = BananaApp::new(crate::egui::Context::default());
        next.settings.prefs.temp_folder = Some(dir.clone());
        next.offer_recovery();
        let what = next.ask.take().expect("offered").what;
        assert!(matches!(what, Asked::Recover { .. }));
        if let Asked::Recover { file, original } = what {
            next.recover(&file, original);
        }
        assert_eq!(next.project(), app.project());
        assert!(next.is_dirty(), "unsaved");
        assert!(!file.exists(), "used up");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
