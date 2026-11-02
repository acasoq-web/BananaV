//! File › Real-Time Render (VEGAS's): a Wave file is picked, the project
//! plays from the start of the timeline and what plays is written to it
//! (the engine's `realtime.rs`), the status bar showing how far; armed
//! tracks are unarmed first, and nothing records meanwhile. Stopping
//! playback stops it, the file keeping what played.

use std::path::{Path, PathBuf};

use bv_core::Tick;

use crate::app::BananaApp;
use crate::i18n::tr;

impl BananaApp {
    /// File › Real-Time Render…: where the file goes, then the render.
    pub(crate) fn real_time_render(&mut self) {
        let folder = self
            .project_path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .or_else(|| bv_platform::user_dir(bv_platform::UserDir::Music))
            .unwrap_or_else(|| PathBuf::from("."));
        let name = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or(tr("Untitled").to_string(), |s| {
                s.to_string_lossy().to_string()
            });
        let picked = rfd::FileDialog::new()
            .set_title(tr("Real-Time Render"))
            .set_directory(folder)
            .set_file_name(format!("{name}.wav"))
            .add_filter("Wave (*.wav)", &["wav"])
            .save_file();
        if let Some(path) = picked {
            self.start_real_time_render(&path);
        }
    }

    /// Starts rendering the project in real time into `path`.
    pub(crate) fn start_real_time_render(&mut self, path: &Path) {
        if self.project().duration() == Tick::ZERO {
            self.notify(tr("The timeline is empty."));
            return;
        }
        self.stop();
        // Armed tracks are unarmed: nothing records meanwhile.
        if self.project().tracks.iter().any(|t| t.armed) {
            self.modify_untracked(|p| {
                for t in &mut p.tracks {
                    t.armed = false;
                }
            });
        }
        self.engine.set_project(self.history.current());
        match self.engine.start_real_time_render(path) {
            Ok(()) => {
                self.playhead = Some(Tick::ZERO);
                crate::logs::log(
                    crate::logs::LogKind::Renders,
                    &[format!("Real-time render started: {}", path.display())],
                );
            }
            Err(e) => self.report_error(trf!("Could not render in real time: {}", e)),
        }
    }

    /// Each frame: how far a real-time render is, and its end.
    pub(crate) fn follow_real_time_render(&mut self) {
        if !self.engine.is_real_time_rendering() {
            return;
        }
        match self.engine.finish_real_time_render() {
            None => {
                if let Some(p) = self.engine.real_time_progress() {
                    self.notify(trf!(
                        "Rendering in real time: {} of {} seconds",
                        crate::fmt::num(p.done, 1),
                        crate::fmt::num(p.total, 1)
                    ));
                }
            }
            Some(Ok(done)) => {
                self.playhead = None;
                crate::logs::log(
                    crate::logs::LogKind::Renders,
                    &[format!(
                        "Real-time render {}: {}",
                        if done.whole { "finished" } else { "stopped" },
                        done.path.display()
                    )],
                );
                let name = done
                    .path
                    .file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().to_string());
                if done.whole {
                    self.notify(trf!("Rendered in real time: {}", name));
                } else {
                    self.notify(trf!(
                        "Real-time render stopped: {} keeps what played.",
                        name
                    ));
                }
            }
            Some(Err(e)) => {
                self.playhead = None;
                crate::logs::log(
                    crate::logs::LogKind::Renders,
                    &[format!("Real-time render failed: {e}")],
                );
                self.report_error(trf!("The real-time render failed: {}", e));
            }
        }
    }
}
