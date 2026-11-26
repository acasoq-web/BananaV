//! Tools › Selectively Prerender Video and Clean Up Prerendered Video: the
//! "Prerender Video" window, the progress, the parts kept by the project,
//! and what happens to their files when the project closes (Options ›
//! Preferences › General: "Save active prerenders on project close").

use std::path::PathBuf;

use bv_engine::{PrerenderFormat, PrerenderOptions};

use crate::app::BananaApp;
use crate::egui::{self, RichText};
use crate::i18n::tr;

/// The "Prerender Video" window.
#[derive(Debug, Default)]
pub(crate) struct PrerenderDialog {
    options: PrerenderOptions,
}

/// Deletes a prerendered part's file — only one BananaV made: the path
/// comes from the project file, and a damaged or edited one could name any
/// file of the computer.
fn remove_prerendered(file: &std::path::Path) {
    if is_prerendered(file) {
        let _ = std::fs::remove_file(file);
    }
}

/// A file named as the engine names prerendered parts
/// (`prerender_<fingerprint>_<frame>.mp4` or `.mov`).
fn is_prerendered(file: &std::path::Path) -> bool {
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
    name.starts_with("prerender_") && matches!(ext, "mp4" | "mov")
}

impl BananaApp {
    /// Where prerendered parts go: File › Properties › Video's folder, or
    /// BananaV's in the temporary files folder.
    fn prerender_folder(&self) -> PathBuf {
        let set = &self.project().prerender_folder;
        if set.is_empty() {
            self.settings.prefs.temp_dir().join("Prerender")
        } else {
            PathBuf::from(set)
        }
    }

    /// Tools › Selectively Prerender Video…
    pub(crate) fn open_prerender(&mut self) {
        self.prerender_dialog = Some(PrerenderDialog::default());
    }

    pub(crate) fn prerender_window(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.prerender_dialog else {
            return;
        };
        let mut open = true;
        let mut go = false;
        let mut cancel = false;
        crate::floating::Window::new(tr("Prerender Video"))
            .id(egui::Id::new("prerender-video"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.horizontal(|ui| {
                    ui.label(tr("Template:"));
                    let name = |f: PrerenderFormat| match f {
                        PrerenderFormat::H264 => tr("MP4 – H.264, project size (plays the most smoothly)"),
                        PrerenderFormat::ProRes => tr("MOV – ProRes 422, project size (best quality)"),
                    };
                    egui::ComboBox::from_id_salt("prerender-template")
                        .width(ui.available_width())
                        .selected_text(name(dialog.options.format))
                        .show_ui(ui, |ui| {
                            for f in [PrerenderFormat::H264, PrerenderFormat::ProRes] {
                                ui.selectable_value(&mut dialog.options.format, f, name(f));
                            }
                        });
                });
                ui.add_space(6.0);
                crate::widgets::checkbox(
                    ui,
                    &mut dialog.options.stretch,
                    tr("Stretch video to fill output frame size (do not letterbox)"),
                );
                crate::widgets::checkbox(
                    ui,
                    &mut dialog.options.rotate,
                    tr("Use project output rotation setting"),
                );
                ui.add_space(6.0);
                crate::tabs::hint(
                    ui,
                    tr("The parts of the time selection (or of the whole project) that do not play in real time — transitions, effects, compositing — are drawn into files the preview then plays."),
                );
                ui.add_space(8.0);
                let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
                let layout = egui::Layout::right_to_left(egui::Align::Center);
                ui.allocate_ui_with_layout(row, layout, |ui| {
                    if ui.button(tr("Cancel")).clicked() {
                        cancel = true;
                    }
                    if ui.button(RichText::new(tr("Render")).strong()).clicked() {
                        go = true;
                    }
                });
            });
        let options = dialog.options;
        if go {
            self.prerender_dialog = None;
            self.start_prerender(options);
        } else if cancel || !open {
            self.prerender_dialog = None;
        }
    }

    fn start_prerender(&mut self, options: PrerenderOptions) {
        let p = self.project();
        let range = p
            .loop_region
            .filter(|r| r.duration > bv_core::Tick::ZERO)
            .unwrap_or_else(|| bv_core::TimeRange::new(bv_core::Tick::ZERO, p.duration()));
        let folder = self.prerender_folder();
        self.engine.set_project(self.history.current());
        match self.engine.prerender(range, &folder, options) {
            0 => self.notify(tr("Nothing there needs prerendering: it plays as it is.")),
            n => self.notify(trn!(
                n,
                "Prerendering {} part…",
                "Prerendering {} parts…",
                n
            )),
        }
    }

    /// The prerendering going on: its progress; the parts done join the
    /// project (without an undo: they are not an edit).
    pub(crate) fn follow_prerender(&mut self, ctx: &egui::Context) {
        let Some((progress, made)) = self.engine.prerender_progress() else {
            return;
        };
        if !progress.finished {
            let percent = (progress.frames_done * 100)
                .checked_div(progress.frames)
                .unwrap_or(0);
            self.notify(trf!(
                "Prerendering video: part {} of {} ({}%)",
                (progress.parts_done + 1).min(progress.parts),
                progress.parts,
                percent
            ));
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
            return;
        }
        if !made.is_empty() {
            let new = made.clone();
            self.modify_untracked(move |p| {
                for part in &new {
                    p.prerenders.retain(|old| {
                        old.range.end() <= part.range.start || old.range.start >= part.range.end()
                    });
                    p.prerenders.push(part.clone());
                }
                p.prerenders.sort_by_key(|part| part.range.start);
            });
        }
        match progress.error {
            Some(e) => self.report_error(trf!("Prerendering stopped: {}", e)),
            None => self.notify(trn!(
                made.len(),
                "{} part prerendered.",
                "{} parts prerendered.",
                made.len()
            )),
        }
    }

    /// Tools › Clean Up Prerendered Video: the project's prerendered files
    /// deleted.
    pub(crate) fn clean_up_prerenders(&mut self) {
        self.engine.cancel_prerender();
        let n = self.project().prerenders.len();
        for part in &self.project().prerenders {
            remove_prerendered(std::path::Path::new(&part.file));
        }
        self.modify_untracked(|p| p.prerenders.clear());
        self.notify(trn!(
            n,
            "{} prerendered file deleted.",
            "{} prerendered files deleted.",
            n
        ));
    }

    /// The project closes: its prerendered files go, unless Options ›
    /// Preferences › General says to keep them ("Save active prerenders on
    /// project close").
    pub(crate) fn closing_project(&mut self) {
        // The parent timelines waiting go with it (they were saved).
        self.parent_timelines.clear();
        self.engine.cancel_prerender();
        // Its copy saved automatically goes with it too.
        self.autosave_forget_copy();
        // And what was of it: ids are counted in each project, so the next
        // one's events, tracks and media have the same numbers — an
        // analysis finishing would write into them, events copied would
        // paste with other media, the tracks would take these heights.
        self.engine.cancel_scene_detection();
        self.engine.cancel_audio_sync();
        self.engine.cancel_beat_detection();
        self.engine.cancel_ducking_analysis();
        self.engine.cancel_nonrealtime_fx();
        self.engine.cancel_loudness_scan();
        self.copy_job = None;
        self.clipboard = None;
        self.proxy_checks.clear();
        self.auto_normalized.clear();
        self.stabilization_points.clear();
        self.timeline.forget_project();
        if self.settings.prefs.keep_prerenders {
            return;
        }
        for part in &self.project().prerenders {
            remove_prerendered(std::path::Path::new(&part.file));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_core::{FrameRate, Tick, TimeRange};
    use bv_timeline::{Effect, EffectKind, Generator, MediaInfo, Project, ProjectSettings};
    use std::time::{Duration, Instant};

    /// What the view and the app keep by a project's ids goes with it: the
    /// next project's tracks and events have the same numbers.
    #[test]
    fn what_was_kept_of_a_project_goes_with_it() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.load_demo();
        let track = app.project().tracks[0].id;
        app.timeline.track_heights.insert(track, 200.0);
        app.timeline.layers.insert(track);
        app.selection = app.project().tracks[0].clips.iter().map(|c| c.id).collect();
        app.copy();
        assert!(app.clipboard.is_some());
        app.new_project();
        assert!(app.timeline.track_heights.is_empty());
        assert!(app.timeline.layers.is_empty());
        assert!(app.clipboard.is_none(), "nothing to paste with other media");
    }

    /// A key pressed while an event is dragged: the drag ends where it is
    /// (one undo) and the key's edit stays (the drag's next move used to
    /// rebuild the project from where it began, the edit lost, its undo
    /// left behind).
    #[test]
    fn an_edit_during_a_drag_ends_the_drag_and_stays() {
        let s = Tick::from_seconds;
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.load_demo();
        let before = app.project().markers.len();
        let g = app.history.begin_gesture();
        app.history.preview(&g, |p| {
            p.add_marker(s(3));
        });
        app.timeline.gesture = Some(g);
        app.modify(|p| {
            p.add_marker(s(7));
        });
        assert!(app.timeline.gesture.is_none(), "the drag ended");
        assert_eq!(app.project().markers.len(), before + 2, "both stay");
        app.undo();
        assert_eq!(
            app.project().markers.len(),
            before + 1,
            "the key's edit undone"
        );
        app.undo();
        assert_eq!(app.project().markers.len(), before, "then the drag");
    }

    /// Only BananaV's own prerendered files are deleted: the project names
    /// them, and a damaged project could name any file.
    #[test]
    fn only_prerendered_files_are_deleted() {
        assert!(is_prerendered(std::path::Path::new(
            "C:/Temp/BananaV/Prerender/prerender_00ab12cd34ef5678_0000120.mp4"
        )));
        assert!(is_prerendered(std::path::Path::new("prerender_1_2.mov")));
        assert!(!is_prerendered(std::path::Path::new(
            "C:/Users/me/Thesis.docx"
        )));
        assert!(!is_prerendered(std::path::Path::new(
            "C:/Users/me/Holiday.mp4"
        )));
        assert!(!is_prerendered(std::path::Path::new("prerender_notes.txt")));
        let dir = std::env::temp_dir().join(format!("bananav-prerender-rm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let mine = dir.join("Holiday.mp4");
        std::fs::write(&mine, b"x").expect("written");
        remove_prerendered(&mine);
        assert!(mine.exists(), "the user's file stays");
        let part = dir.join("prerender_0000000000000001_0000000.mp4");
        std::fs::write(&part, b"x").expect("written");
        remove_prerendered(&part);
        assert!(!part.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Inverted color bars, the first second selected.
    fn app(folder: &std::path::Path) -> BananaApp {
        let mut app = BananaApp::new(egui::Context::default());
        let mut p = Project::new(ProjectSettings {
            width: 256,
            height: 144,
            frame_rate: FrameRate::new(30, 1),
            ..ProjectSettings::default()
        });
        let v = p.tracks[0].id;
        let bars = p.add_media(MediaInfo::generated(Generator::ColorBars, &p.settings));
        let clip = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
        p.clip_mut(clip)
            .unwrap()
            .effects
            .push(Effect::new(EffectKind::Invert));
        p.loop_region = Some(TimeRange::new(Tick::ZERO, Tick::from_seconds(1)));
        p.prerender_folder = folder.to_string_lossy().into();
        app.modify_untracked(move |q| *q = p.clone());
        app
    }

    /// Tools › Selectively Prerender Video, followed to the end.
    fn prerender(app: &mut BananaApp) {
        let ctx = egui::Context::default();
        app.start_prerender(PrerenderOptions {
            format: PrerenderFormat::ProRes,
            ..PrerenderOptions::default()
        });
        let start = Instant::now();
        while app.engine.is_prerendering() {
            app.follow_prerender(&ctx);
            assert!(start.elapsed() < Duration::from_secs(60), "done in time");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn prerendered_files_are_kept_cleaned_up_and_deleted_on_close() {
        let folder = std::env::temp_dir().join(format!("bananav-prerender-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let mut app = app(&folder);
        prerender(&mut app);
        let parts = app.project().prerenders.clone();
        let second = TimeRange::new(Tick::ZERO, Tick::from_seconds(1));
        assert_eq!(parts.len(), 1, "one second: one part");
        assert_eq!(parts[0].range, second);
        let file = PathBuf::from(&parts[0].file);
        assert!(file.starts_with(&folder) && file.exists(), "{file:?}");
        assert!(!app.history.can_undo(), "not an edit");
        app.engine.set_project(app.history.current());
        assert_eq!(app.engine.prerendered(), &[second]);
        // An edit there: the part no longer counts.
        app.modify_untracked(|p| {
            let clip = p.tracks[0].clips[0].id;
            p.clip_mut(clip).unwrap().effects[0].enabled = false;
        });
        app.engine.set_project(app.history.current());
        assert!(app.engine.prerendered().is_empty());

        // Tools › Clean Up Prerendered Video.
        app.clean_up_prerenders();
        assert!(app.project().prerenders.is_empty() && !file.exists());

        // Closing the project: kept or deleted, as Options › Preferences ›
        // General says.
        app.modify_untracked(|p| {
            let clip = p.tracks[0].clips[0].id;
            p.clip_mut(clip).unwrap().effects[0].enabled = true;
        });
        prerender(&mut app);
        let file = PathBuf::from(&app.project().prerenders[0].file);
        app.settings.prefs.keep_prerenders = true;
        app.closing_project();
        assert!(file.exists(), "kept");
        app.settings.prefs.keep_prerenders = false;
        app.new_project();
        assert!(!file.exists(), "deleted with the project");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn plain_media_needs_no_prerendering() {
        let folder = std::env::temp_dir().join("bananav-prerender-plain");
        let mut app = app(&folder);
        app.modify_untracked(|p| p.tracks[0].clips[0].effects.clear());
        app.start_prerender(PrerenderOptions::default());
        assert!(!app.engine.is_prerendering());
        assert!(!folder.exists(), "nothing written");
    }
}
