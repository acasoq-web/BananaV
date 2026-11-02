//! Nested projects on the interface's side (VEGAS's "Nesting projects"): a
//! project file comes in as media — dragged to the timeline, or through
//! Import Media — unless it is the open project or nests it; Edit Source
//! Project opens it in another window; and nested projects saved meanwhile
//! (in that window, say) are described and read anew once BananaV is the
//! active program again — as are the media files changed meanwhile (a
//! file rendered again from its source project, which Edit Source Project
//! opened: the events follow it), their user's settings (pixel aspect,
//! field order) kept.

use std::path::Path;
use std::time::SystemTime;

use bv_timeline::{MediaId, MediaSource};

use crate::app::BananaApp;
use crate::i18n::tr;

/// Each media file, when it last changed and how long it is.
pub(crate) type FileStamps = Vec<(String, Option<SystemTime>, u64)>;

impl BananaApp {
    /// Whether the project file `path` can't be nested in the open project:
    /// it is the open project's own file, or nests it at some depth.
    pub(crate) fn nests_open_project(&self, path: &Path) -> bool {
        let Some(open) = self.project_path.as_deref() else {
            return false;
        };
        bv_engine::same_file(path, open)
            || bv_engine::nested_projects(&path.to_string_lossy())
                .iter()
                .any(|p| bv_engine::same_file(Path::new(p), open))
    }

    /// Whether the media `id` is a nested project.
    pub(crate) fn is_nested_project(&self, id: MediaId) -> bool {
        self.project().media(id).is_some_and(
            |m| matches!(&m.source, MediaSource::File { path } if bv_engine::is_project(path)),
        )
    }

    /// The open project's media files (nested projects too), as they are
    /// now; each once.
    fn file_stamps(&self) -> FileStamps {
        let mut out: FileStamps = Vec::new();
        for m in &self.project().media {
            let MediaSource::File { path } = &m.source else {
                continue;
            };
            if m.sequence.is_some() || out.iter().any(|(p, ..)| p == path) {
                continue;
            }
            let meta = std::fs::metadata(path).ok();
            let changed = meta.as_ref().and_then(|m| m.modified().ok());
            out.push((path.clone(), changed, meta.map_or(0, |m| m.len())));
        }
        out
    }

    /// BananaV left (`active` false) or came back: the media files changed
    /// meanwhile — a nested project saved, a file rendered again — are
    /// described again (no undo step) and read anew.
    pub(crate) fn follow_changed_files(&mut self, active: bool) {
        if !active {
            self.files_seen = Some(self.file_stamps());
            return;
        }
        let Some(before) = self.files_seen.take() else {
            return;
        };
        let (nested, media): (Vec<String>, Vec<String>) = self
            .file_stamps()
            .into_iter()
            .filter(|now| !before.contains(now))
            .map(|(path, ..)| path)
            .partition(|p| bv_engine::is_project(p));
        if !nested.is_empty() {
            self.read_nested_again(&nested);
        }
        if !media.is_empty() {
            self.read_media_again(&media);
        }
    }

    /// The media files in `paths` described again (no undo step) — their
    /// length, picture size and rate, sound and markers; the settings
    /// their user gives them kept — their waveforms made anew, and read
    /// anew. Files that can't be read (being written, gone) are left.
    pub(crate) fn read_media_again(&mut self, paths: &[String]) {
        let mut read = 0usize;
        for path in paths {
            let Ok(new) = bv_engine::Engine::probe_file(path) else {
                continue;
            };
            bv_engine::Engine::delete_peak_files(path);
            self.modify_untracked(|p| {
                for m in &mut p.media {
                    if !matches!(&m.source, MediaSource::File { path: at } if at == path) {
                        continue;
                    }
                    m.duration = new.duration;
                    m.video = match (m.video, new.video) {
                        // The file's size, turned as its Properties say
                        // (the settings back to the file's own way first).
                        (Some(old), Some(v)) => Some(
                            bv_timeline::VideoStreamInfo {
                                width: v.width,
                                height: v.height,
                                frame_rate: v.frame_rate,
                                ..old.turned_to(bv_timeline::MediaRotation::Original)
                            }
                            .turned_to(old.reading.rotation),
                        ),
                        (_, v) => v,
                    };
                    m.audio = new.audio;
                    m.markers.clone_from(&new.markers);
                }
            });
            read += 1;
        }
        if read == 0 {
            return;
        }
        self.engine.release_media();
        self.engine.refresh_thumbnails();
        self.notify(trn!(
            read,
            "{} media file changed on disk was read again.",
            "{} media files changed on disk were read again.",
            read
        ));
    }

    /// Whether the project has nested projects.
    pub(crate) fn has_nested_projects(&self) -> bool {
        self.project()
            .media
            .iter()
            .any(|m| matches!(&m.source, MediaSource::File { path } if bv_engine::is_project(path)))
    }

    /// Tools › Rebuild Offline Nested Audio: VEGAS renders a nested
    /// project's sound to a proxy file again; here, where it is mixed
    /// live, every nested project is read again from its file.
    pub(crate) fn rebuild_nested_audio(&mut self) {
        let all: Vec<String> = self
            .file_stamps()
            .into_iter()
            .map(|(p, ..)| p)
            .filter(|p| bv_engine::is_project(p))
            .collect();
        self.read_nested_again(&all);
    }

    /// The nested projects in `paths` described again (no undo step) and
    /// read anew.
    pub(crate) fn read_nested_again(&mut self, paths: &[String]) {
        for path in paths {
            let Ok(new) = bv_engine::Engine::probe_file(path) else {
                continue;
            };
            self.modify_untracked(|p| {
                for m in &mut p.media {
                    if matches!(&m.source, MediaSource::File { path: at } if at == path) {
                        m.duration = new.duration;
                        m.video = new.video;
                        m.audio = new.audio;
                        m.markers.clone_from(&new.markers);
                    }
                }
            });
        }
        self.engine.release_media();
        self.engine.refresh_thumbnails();
        self.notify(tr("The nested projects were read again."));
    }
}

#[cfg(test)]
mod tests {
    use bv_core::Tick;
    use bv_timeline::ProjectSettings;

    use super::*;

    #[test]
    fn a_file_rendered_again_is_described_again() {
        let dir = std::env::temp_dir().join(format!("bv-reread-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("render.wav");
        let wave = |seconds: u32| {
            let frames = 8_000 * seconds;
            let mut b = Vec::new();
            b.extend_from_slice(b"RIFF");
            b.extend_from_slice(&(36 + frames * 2).to_le_bytes());
            b.extend_from_slice(b"WAVEfmt ");
            b.extend_from_slice(&16u32.to_le_bytes());
            b.extend_from_slice(&1u16.to_le_bytes());
            b.extend_from_slice(&1u16.to_le_bytes());
            b.extend_from_slice(&8_000u32.to_le_bytes());
            b.extend_from_slice(&16_000u32.to_le_bytes());
            b.extend_from_slice(&2u16.to_le_bytes());
            b.extend_from_slice(&16u16.to_le_bytes());
            b.extend_from_slice(b"data");
            b.extend_from_slice(&(frames * 2).to_le_bytes());
            b.resize(b.len() + frames as usize * 2, 0);
            b
        };
        std::fs::write(&path, wave(2)).unwrap();
        let mut app = BananaApp::new(crate::egui::Context::default());
        let info = bv_engine::Engine::probe_file(&path.to_string_lossy()).unwrap();
        let id = app.modify(|p| {
            *p = bv_timeline::Project::new(ProjectSettings::default());
            p.add_media(info)
        });
        let length = |app: &BananaApp| app.project().media(id).unwrap().duration;
        assert_eq!(length(&app), Tick::from_seconds(2));
        // BananaV away; the file rendered again, longer.
        app.follow_changed_files(false);
        std::fs::write(&path, wave(3)).unwrap();
        app.follow_changed_files(true);
        assert_eq!(length(&app), Tick::from_seconds(3));
        // Nothing changed: nothing read.
        app.follow_changed_files(false);
        app.follow_changed_files(true);
        assert_eq!(length(&app), Tick::from_seconds(3));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
