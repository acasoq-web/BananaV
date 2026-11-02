//! Nested timelines (VEGAS's "Nested Timelines"): the selected events made
//! into a project of their own, saved beside the parent, which takes their
//! place as one event (a video and an audio event, grouped) — Create
//! Nested Timeline —, and a nested project edited in this window — Open
//! Nested Timeline — while the parent waits as it was, until Open Parent
//! Timeline brings it back with the nested project read again.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use bv_core::Tick;
use bv_timeline::{ClipId, History, MediaSource, Project};

use crate::app::BananaApp;
use crate::ask::{Ask, Asked};
use crate::i18n::tr;

/// A parent timeline waiting while one of its nested timelines is open.
pub(crate) struct ParentTimeline {
    history: History<Project>,
    saved: Project,
    path: Option<PathBuf>,
    cursor: Tick,
    selection: BTreeSet<ClipId>,
}

impl BananaApp {
    /// The file of the nested project the selected event shows, if it
    /// shows one.
    pub(crate) fn selected_nested_project(&self) -> Option<PathBuf> {
        let p = self.project();
        let clip = p.clip(self.primary_clip()?)?;
        match &p.media(clip.media)?.source {
            MediaSource::File { path } if bv_engine::is_project(path) => Some(PathBuf::from(path)),
            _ => None,
        }
    }

    /// Whether a nested timeline is open (its parent waiting).
    pub(crate) fn in_nested_timeline(&self) -> bool {
        !self.parent_timelines.is_empty()
    }

    /// Create Nested Timeline (Alt+C): first, when the events taken out
    /// may change how they look or sound, a question (Options ›
    /// Preferences › General: "Warn if creating a nested timeline may
    /// adversely affect the project").
    pub(crate) fn request_nested_timeline(&mut self) {
        if self.selection.is_empty() {
            self.notify(tr("Select the events to nest first."));
            return;
        }
        if self.settings.prefs.warn_nesting && self.project().nesting_risks(&self.selection) {
            self.ask_user(Ask {
                title: tr("Create Nested Timeline").into(),
                text: tr("Some of these events are grouped with events left out, or are on tracks with effects, envelopes, motion or a compositing mode of their own, which will no longer apply to them. Create the nested timeline anyway?").into(),
                what: Asked::CreateNestedTimeline,
                check: Some((
                    tr("Warn if creating a nested timeline may adversely affect the project"),
                    |p| &mut p.warn_nesting,
                )),
            });
            return;
        }
        self.create_nested_timeline();
    }

    /// The selected events into a project of their own: the parent saved
    /// first if it never was, the nested project saved where the user says
    /// (beside the parent, "<parent>_Untitled Timeline"), an event of it in
    /// their place (one undo step), and the nested timeline opened.
    pub(crate) fn create_nested_timeline(&mut self) {
        if self.project_path.is_none() && !self.save(false) {
            return;
        }
        if self.project().nested_timeline(&self.selection).is_none() {
            return;
        }
        let parent = self.project_path.clone().unwrap_or_default();
        let stem = parent
            .file_stem()
            .map_or(String::new(), |s| s.to_string_lossy().to_string());
        let mut dialog = rfd::FileDialog::new()
            .set_title(tr("Save Nested Timeline"))
            .add_filter(tr("BananaV Project"), &["bvproj"])
            .set_file_name(format!("{stem}_{}.bvproj", tr("Untitled Timeline")));
        if let Some(dir) = parent.parent() {
            dialog = dialog.set_directory(dir);
        }
        if let Some(file) = dialog.save_file() {
            self.nest_into(file);
        }
    }

    /// The selected events nested in a project saved at `file`, an event
    /// of it in their place (one undo step), and the nested timeline
    /// opened.
    pub(crate) fn nest_into(&mut self, file: PathBuf) {
        let Some(nest) = self.project().nested_timeline(&self.selection) else {
            return;
        };
        let parent = self.project_path.clone().unwrap_or_default();
        if bv_engine::same_file(&file, &parent) {
            self.report_error(tr("A project can't be nested in itself"));
            return;
        }
        if let Err(e) = std::fs::write(&file, nest.project.to_json()) {
            self.report_save_error(&file, &e);
            return;
        }
        let ids: Vec<ClipId> = self.selection.iter().copied().collect();
        let placed = self.as_one_undo(|app| {
            app.edit(|p| p.delete_events(&ids, false, false))?;
            let m = *app.import_paths(std::slice::from_ref(&file)).first()?;
            app.edit(|p| p.insert_media(m, nest.start, nest.video_track, nest.audio_track))
        });
        let Some(placed) = placed else {
            return;
        };
        self.selection = placed.into_iter().collect();
        self.open_nested_timeline_at(file);
    }

    /// Open Nested Timeline (Alt+N): the selected event's nested project,
    /// in this window.
    pub(crate) fn open_nested_timeline(&mut self) {
        match self.selected_nested_project() {
            Some(path) => self.open_nested_timeline_at(path),
            None => self.notify(tr("Select an event of a nested project first.")),
        }
    }

    /// The nested project at `path` opened in this window, the parent
    /// waiting (saved first when changed, so that what happens next never
    /// loses it).
    pub(crate) fn open_nested_timeline_at(&mut self, path: PathBuf) {
        let loaded = bv_engine::read_project(&path.to_string_lossy());
        let nested = match loaded {
            Ok(p) => p,
            Err(e) => {
                self.report_error(trf!(
                    "Could not open the nested timeline: {}",
                    crate::media_files::error_text(&e)
                ));
                return;
            }
        };
        if self.is_dirty() && !self.save(false) {
            return;
        }
        self.stop();
        let parent = ParentTimeline {
            history: std::mem::replace(&mut self.history, History::new(nested.clone())),
            saved: std::mem::replace(&mut self.saved, nested),
            path: self.project_path.replace(path.clone()),
            cursor: std::mem::replace(&mut self.cursor, Tick::ZERO),
            selection: std::mem::take(&mut self.selection),
        };
        self.parent_timelines.push(parent);
        self.fit_timeline = true;
        self.notify(trf!(
            "Nested timeline: {}. Open Parent Timeline (Alt+P) goes back.",
            file_name(&path)
        ));
    }

    /// Open Parent Timeline (Alt+P): a changed nested timeline is saved
    /// first — after asking, with Options › Preferences › General's "Warn
    /// to save modified nested timeline prior to opening parent timeline"
    /// (No leaves the changes) —, then the parent comes back.
    pub(crate) fn open_parent_timeline(&mut self) {
        if !self.in_nested_timeline() {
            return;
        }
        if !self.is_dirty() {
            self.back_to_parent(false);
        } else if self.settings.prefs.warn_nested_save {
            let name = self
                .project_path
                .as_deref()
                .map(file_name)
                .unwrap_or_default();
            self.ask_user(Ask {
                title: tr("Open Parent Timeline").into(),
                text: trf!(
                    "Save the changes to {} before opening the parent timeline?",
                    name
                ),
                what: Asked::SaveNestedTimeline,
                check: Some((
                    tr("Warn to save modified nested timeline prior to opening parent timeline"),
                    |p| &mut p.warn_nested_save,
                )),
            });
        } else {
            self.back_to_parent(true);
        }
    }

    /// The parent timeline back (the nested one saved first with `save`),
    /// the nested project it shows read again.
    pub(crate) fn back_to_parent(&mut self, save: bool) {
        if save && !self.save(false) {
            return;
        }
        let Some(parent) = self.parent_timelines.pop() else {
            return;
        };
        let nested = self.project_path.take();
        self.stop();
        self.history = parent.history;
        self.saved = parent.saved;
        self.project_path = parent.path;
        self.cursor = parent.cursor;
        self.selection = parent.selection;
        self.fit_timeline = true;
        if let Some(n) = nested {
            self.read_nested_again(&[n.to_string_lossy().to_string()]);
        }
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map_or_else(
        || p.display().to_string(),
        |n| n.to_string_lossy().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui;
    use bv_timeline::{Generator, MediaInfo, ProjectSettings, TrackKind};

    #[test]
    fn events_nest_and_the_parent_comes_back() {
        let dir = std::env::temp_dir().join(format!("bv-nested-timeline-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = BananaApp::new(egui::Context::default());
        let mut p = Project::new(ProjectSettings::default());
        let v = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .map(|t| t.id);
        let red = p.add_media(MediaInfo::generated(
            Generator::SolidColor {
                rgba: [1.0, 0.0, 0.0, 1.0],
            },
            &p.settings,
        ));
        let event = p.insert_media(red, Tick::from_seconds(2), v, None).unwrap()[0];
        app.history.reset(p.clone());
        app.saved = p;
        let parent = dir.join("Show.bvproj");
        std::fs::write(&parent, app.project().to_json()).unwrap();
        app.project_path = Some(parent.clone());
        app.settings.prefs.warn_nested_save = false;
        app.selection = [event].into();
        let nested = dir.join("Show_Untitled Timeline.bvproj");
        app.nest_into(nested.clone());
        // In the nested timeline: the event alone, at zero.
        assert!(app.in_nested_timeline());
        assert_eq!(app.project_path.as_deref(), Some(nested.as_path()));
        let starts: Vec<Tick> = app.project().clips().map(|(_, c)| c.start).collect();
        assert_eq!(starts, [Tick::ZERO]);
        // The parent was saved with an event of the nested project in its
        // place.
        let saved = bv_engine::read_project(&parent.to_string_lossy()).unwrap();
        let (_, e) = saved.clips().next().unwrap();
        assert_eq!(e.start, Tick::from_seconds(2));
        let source = &saved.media(e.media).unwrap().source;
        assert!(matches!(source, MediaSource::File { path } if bv_engine::is_project(path)));
        // Back in the parent, the nested event selected.
        app.open_parent_timeline();
        assert!(!app.in_nested_timeline());
        assert_eq!(app.project_path.as_deref(), Some(parent.as_path()));
        assert_eq!(app.selected_nested_project(), Some(nested.clone()));
        // And into it again.
        app.open_nested_timeline();
        assert_eq!(app.project_path.as_deref(), Some(nested.as_path()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
