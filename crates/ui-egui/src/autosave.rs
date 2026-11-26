//! Options › Preferences › General: "Enable autosave". Every few minutes a
//! changed project is copied to the temporary files folder. BananaV deletes
//! its copy when it closes normally, so a copy found when it starts means it
//! did not: BananaV offers to open it.
//!
//! A copy is only of changes not saved: a save, a project closed or
//! replaced, or the project found unchanged at a tick deletes it, so what
//! is offered back is never older than the project's file. Each session
//! holds a lock beside its copy while it runs: another BananaV started
//! meanwhile leaves that copy alone (it is not a session that ended badly).

use std::fs::File;
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
    /// This session's lock, held from its first copy until it closes.
    lock: Option<File>,
}

impl Default for Autosave {
    fn default() -> Autosave {
        Autosave {
            at: Instant::now(),
            last: None,
            lock: None,
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

/// The lock a running session holds beside its copy `file`.
fn lock_of(file: &Path) -> PathBuf {
    file.with_extension("lock")
}

/// Whether the session that wrote `file` still runs: its lock is held.
/// A session that ended, however it ended, holds none.
fn still_running(file: &Path) -> bool {
    let Ok(lock) = File::options().read(true).write(true).open(lock_of(file)) else {
        return false;
    };
    matches!(lock.try_lock(), Err(std::fs::TryLockError::WouldBlock))
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
        if !self.is_dirty() {
            // Saved (or changed back) since the copy: nothing to keep.
            if self.autosave.last.is_some() {
                self.autosave_forget_copy();
            }
            return;
        }
        let project = self.project().clone();
        if self.autosave.last.as_ref() == Some(&project) {
            return;
        }
        let (file, about) = files(&self.settings.prefs, std::process::id());
        if self.autosave.lock.is_none() {
            self.autosave.lock = hold_lock(&file);
        }
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

    /// Deletes this session's copy: the project was saved, closed or
    /// replaced, and the copy is of changes that are no longer unsaved.
    pub(crate) fn autosave_forget_copy(&mut self) {
        if self.autosave.last.take().is_none() {
            return;
        }
        let (file, about) = files(&self.settings.prefs, std::process::id());
        let _ = std::fs::remove_file(file);
        let _ = std::fs::remove_file(about);
    }

    /// Deletes this session's copy and lock (BananaV closes normally).
    pub(crate) fn autosave_cleanup(&mut self) {
        let (file, about) = files(&self.settings.prefs, std::process::id());
        self.autosave.last = None;
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_file(about);
        if self.autosave.lock.take().is_some() {
            let _ = std::fs::remove_file(lock_of(&file));
        }
    }

    /// At startup: a copy left by a session that did not close normally is
    /// offered back.
    pub(crate) fn offer_recovery(&mut self) {
        let Ok(entries) = std::fs::read_dir(folder(&self.settings.prefs)) else {
            return;
        };
        let own = files(&self.settings.prefs, std::process::id()).0;
        let mut copies = Vec::new();
        for path in entries.flatten().map(|e| e.path()) {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if path == own || still_running(&path) {
                continue;
            }
            match ext {
                "bvproj" => copies.push(path),
                // What a session that ended left half-written, or its lock.
                "part" | "lock" => {
                    let _ = std::fs::remove_file(&path);
                }
                _ => {}
            }
        }
        let newest = copies
            .into_iter()
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
        let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let saved_after = original
            .as_deref()
            .and_then(modified)
            .zip(modified(&file))
            .is_some_and(|(saved, copied)| saved > copied);
        let mut text = trf!(
            "BananaV did not close normally last time. Open the copy of {} it saved automatically?",
            name
        );
        if saved_after {
            // Saved by another window after the copy: the copy may be older.
            text.push_str("\n\n");
            text.push_str(tr(
                "The project's file was saved after this copy was made: what the copy holds may be older.",
            ));
        }
        self.ask_user(Ask {
            title: tr("Autosave").into(),
            text,
            what: Asked::Recover { file, original },
            check: None,
        });
    }

    /// Opens a copy saved automatically, as unsaved changes to its project.
    /// The copy becomes this session's: kept until the project is saved or
    /// BananaV closes normally, so a second crash before the next copy
    /// loses nothing.
    pub(crate) fn recover(&mut self, file: &Path, original: Option<PathBuf>) {
        let loaded = std::fs::read_to_string(file)
            .map_err(|e| e.to_string())
            .and_then(|s| Project::from_json(&s).map_err(|e| e.to_string()))
            .and_then(crate::app::mended);
        match loaded {
            Ok((p, mends)) => {
                if !mends.is_empty() {
                    self.report_error(crate::app::said_mends(&mends));
                }
                self.stop();
                // Unsaved: told apart from the project as it was saved.
                let saved = original
                    .as_deref()
                    .and_then(|o| std::fs::read_to_string(o).ok())
                    .and_then(|s| Project::from_json(&s).ok())
                    .unwrap_or_default();
                self.history.reset(p.clone());
                self.set_saved(saved);
                self.project_path = original;
                self.fit_timeline = true;
                let (own, about) = files(&self.settings.prefs, std::process::id());
                if self.autosave.lock.is_none() {
                    self.autosave.lock = hold_lock(&own);
                }
                let adopted = std::fs::rename(file, &own).is_ok()
                    && std::fs::rename(file.with_extension("json"), &about).is_ok();
                if adopted {
                    self.autosave.last = Some(p);
                } else {
                    forget(file);
                }
            }
            Err(e) => self.report_error(trf!("Could not open the project: {}", e)),
        }
    }
}

/// This session's lock beside its copy `file`, held while it runs.
fn hold_lock(file: &Path) -> Option<File> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).ok()?;
    }
    let lock = File::create(lock_of(file)).ok()?;
    lock.try_lock().ok()?;
    Some(lock)
}

/// Deletes a copy saved automatically.
pub(crate) fn forget(file: &Path) {
    let _ = std::fs::remove_file(file);
    let _ = std::fs::remove_file(file.with_extension("json"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder_for(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bananav-autosave-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// An app whose copies go to `dir`, its project changed.
    fn changed(dir: &Path) -> BananaApp {
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.settings.prefs.temp_folder = Some(dir.to_path_buf());
        app.load_demo();
        app.modify(|p| p.add_marker(bv_core::Tick::from_seconds(1)));
        app
    }

    /// Five minutes later: the copy, written by its thread.
    fn tick(app: &mut BananaApp) -> PathBuf {
        app.autosave.at = Instant::now() - EVERY;
        app.autosave_tick();
        let (file, _) = files(&app.settings.prefs, std::process::id());
        let start = Instant::now();
        while !file.exists() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(10));
        }
        file
    }

    /// A session that ended badly: its lock no longer held (its copy and
    /// its file renamed as another process's).
    fn ended(app: &mut BananaApp, file: &Path) -> PathBuf {
        app.autosave.lock = None;
        let gone = file.with_file_name("4000000001.bvproj");
        std::fs::rename(file, &gone).expect("renamed");
        std::fs::rename(file.with_extension("json"), gone.with_extension("json")).expect("renamed");
        std::fs::rename(lock_of(file), lock_of(&gone)).expect("renamed");
        gone
    }

    #[test]
    fn a_copy_left_behind_is_offered_back() {
        let dir = folder_for("left");
        let mut app = changed(&dir);
        let file = tick(&mut app);
        assert!(file.exists(), "copied");
        let left = ended(&mut app, &file);
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
        // Kept, as the session's own copy, until it is saved or closed.
        assert!(!left.exists(), "no longer the old session's");
        let (own, _) = files(&next.settings.prefs, std::process::id());
        assert!(own.exists(), "this session's copy now");
        next.autosave_cleanup();
        assert!(!own.exists(), "closed normally");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A save makes the copy's changes saved ones: the copy goes, so it is
    /// never offered later in place of the newer file.
    #[test]
    fn a_save_or_another_project_deletes_the_copy() {
        let dir = folder_for("saved");
        let mut app = changed(&dir);
        let file = tick(&mut app);
        assert!(file.exists(), "copied");
        // Saved: the next tick finds nothing unsaved and deletes it (Save
        // itself also does, through the same function).
        app.set_saved(app.project().clone());
        app.autosave.at = Instant::now() - EVERY;
        app.autosave_tick();
        assert!(!file.exists(), "no copy of a saved project");
        // Changed again and copied, then another project opened.
        app.modify(|p| p.add_marker(bv_core::Tick::from_seconds(2)));
        let file = tick(&mut app);
        assert!(file.exists(), "copied again");
        app.new_project();
        assert!(!file.exists(), "the copy went with its project");
        app.autosave_cleanup();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Another BananaV started while this one runs: this one's copy is not
    /// a session that ended badly — neither offered nor deleted.
    #[test]
    fn a_running_sessions_copy_is_left_alone() {
        let dir = folder_for("running");
        let mut app = changed(&dir);
        let file = tick(&mut app);
        assert!(file.exists(), "copied");
        // Seen from another process: the same files under another name,
        // its lock held by this test.
        let other = file.with_file_name("4000000002.bvproj");
        std::fs::copy(&file, &other).expect("copied");
        let held = hold_lock(&other).expect("locked");
        let mut next = BananaApp::new(crate::egui::Context::default());
        next.settings.prefs.temp_folder = Some(dir.clone());
        next.offer_recovery();
        assert!(next.ask.is_none(), "nothing offered");
        assert!(other.exists() && file.exists(), "nothing deleted");
        drop(held);
        app.autosave_cleanup();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
