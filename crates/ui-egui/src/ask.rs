//! Questions BananaV asks before an action, as message boxes (Options ›
//! Preferences › General turns most of them on or off): a media file still
//! used by events, a track that holds events, the project's video settings
//! against the first video added, the project files' association.

use bv_timeline::{MediaId, TrackId};

use crate::app::BananaApp;
use crate::egui;
use crate::i18n::tr;
use crate::settings::Prefs;

/// What the answer "Yes" does.
#[derive(Clone, Debug)]
pub(crate) enum Asked {
    /// Removes the media file and its events from the project.
    RemoveMedia(MediaId),
    /// Deletes the tracks and their events.
    DeleteTracks(Vec<TrackId>),
    /// Sets the project's frame size and rate to the media's.
    MatchMedia {
        width: u32,
        height: u32,
        rate: bv_core::FrameRate,
        pixel_aspect: bv_timeline::PixelAspect,
    },
    /// Makes BananaV open its project files.
    Associate,
    /// Opens the copy of a project saved automatically (No deletes it).
    Recover {
        file: std::path::PathBuf,
        original: Option<std::path::PathBuf>,
    },
    /// Adds an audio track armed for recording and records into it.
    RecordNewTrack,
    /// Expand to Multiple Tracks: Yes keeps the unused takes as muted
    /// events, No deletes them (both expand).
    ExpandMulticam(bv_timeline::TrackId),
    /// Deletes a filter package and its file.
    DeleteFxPackage { name: String, audio: bool },
    /// The Explorer's Delete: the files and folders to the Recycle Bin.
    RecycleFiles(Vec<std::path::PathBuf>),
    /// Project Media's Remove from Project and Delete File(s): the media
    /// out of the project, their files to the Recycle Bin.
    RemoveAndRecycle(Vec<(MediaId, std::path::PathBuf)>),
    /// Creates the nested timeline, though its events may change.
    CreateNestedTimeline,
    /// Open Parent Timeline: Yes saves the nested timeline first, No
    /// leaves its changes.
    SaveNestedTimeline,
}

/// One of the preferences' switches.
pub(crate) type PrefSwitch = fn(&mut Prefs) -> &mut bool;

/// A question waiting for its answer.
#[derive(Clone, Debug)]
pub(crate) struct Ask {
    pub title: String,
    pub text: String,
    pub what: Asked,
    /// A check box under the text, bound to a preference (as "Check at
    /// startup").
    pub check: Option<(&'static str, PrefSwitch)>,
}

impl BananaApp {
    /// Draws the question being asked, if any, and does what "Yes" says.
    /// Asks `ask` now, or after the question being asked.
    pub(crate) fn ask_user(&mut self, ask: Ask) {
        match self.ask {
            Some(_) => self.asks_later.push_back(ask),
            None => self.ask = Some(ask),
        }
    }

    pub(crate) fn ask_dialog(&mut self, ctx: &egui::Context) {
        if self.ask.is_none() {
            self.ask = self.asks_later.pop_front();
        }
        let Some(ask) = self.ask.clone() else {
            return;
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::new("ask")).show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.heading(&ask.title);
            ui.add_space(4.0);
            ui.label(&ask.text);
            if let Some((label, value)) = ask.check {
                ui.add_space(6.0);
                crate::widgets::checkbox(ui, value(&mut self.settings.prefs), label);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(tr("Yes")).min_size(egui::vec2(72.0, 0.0)))
                    .clicked()
                {
                    answer = Some(true);
                }
                if ui
                    .add(egui::Button::new(tr("No")).min_size(egui::vec2(72.0, 0.0)))
                    .clicked()
                {
                    answer = Some(false);
                }
            });
        });
        let Some(yes) = answer else {
            return;
        };
        self.ask = None;
        if ask.check.is_some() {
            // The check box changed a preference.
            let _ = self.settings.save();
        }
        if yes {
            self.answer_yes(ask.what);
        } else if let Asked::Recover { file, .. } = ask.what {
            crate::autosave::forget(&file);
        } else if let Asked::ExpandMulticam(track) = ask.what {
            self.expand_multicam(track, false);
        } else if matches!(ask.what, Asked::SaveNestedTimeline) {
            self.back_to_parent(false);
        }
    }

    fn answer_yes(&mut self, what: Asked) {
        match what {
            Asked::ExpandMulticam(track) => self.expand_multicam(track, true),
            Asked::RemoveMedia(id) => self.remove_media(id),
            Asked::DeleteTracks(ids) => self.delete_tracks(&ids),
            Asked::MatchMedia {
                width,
                height,
                rate,
                pixel_aspect,
            } => {
                self.modify(|p| {
                    p.settings.width = width;
                    p.settings.height = height;
                    p.settings.frame_rate = rate;
                    p.settings.pixel_aspect = pixel_aspect;
                });
            }
            Asked::Associate => match bv_platform::associate_project_files(
                PROJECT_EXTENSION,
                PROJECT_TYPE,
                tr("BananaV Project"),
            ) {
                Ok(()) => self.notify(tr("BananaV now opens its project files (.bvproj).")),
                Err(e) => {
                    self.report_error(trf!("Could not make BananaV open its project files: {}", e))
                }
            },
            Asked::Recover { file, original } => self.recover(&file, original),
            Asked::RecordNewTrack => self.record_into_new_track(),
            Asked::DeleteFxPackage { name, audio } => match self.fx_packages.remove(&name, audio) {
                Ok(()) => self.notify(trf!("Filter package deleted: {}", name)),
                Err(e) => self.report_error(trf!("Could not delete the filter package: {}", e)),
            },
            Asked::RecycleFiles(paths) => self.recycle_files(&paths),
            Asked::RemoveAndRecycle(files) => self.remove_and_recycle(files),
            Asked::CreateNestedTimeline => self.create_nested_timeline(),
            Asked::SaveNestedTimeline => self.back_to_parent(true),
        }
    }

    /// Removes a media file from the project; when events use it, asks
    /// first (Options › Preferences › General: "Confirm media file deletion
    /// when still in use").
    pub(crate) fn request_remove_media(&mut self, id: MediaId) {
        // A subclip goes alone (its events are of its parent).
        if self.project().subclip(id).is_some() {
            self.modify(|p| p.remove_subclip(id));
            return;
        }
        let events = self
            .project()
            .clips()
            .filter(|(_, c)| c.media == id)
            .count();
        if events == 0 || !self.settings.prefs.confirm_media_removal {
            self.remove_media(id);
            return;
        }
        let name = self
            .project()
            .media(id)
            .map(crate::i18n::media_name)
            .unwrap_or_default();
        self.ask_user(Ask {
            title: tr("Remove Media").into(),
            text: trn!(
                events,
                "{1} is used by {0} event. Remove it from the project, and the event with it?",
                "{1} is used by {0} events. Remove it from the project, and the events with it?",
                events,
                name
            ),
            what: Asked::RemoveMedia(id),
            check: None,
        });
    }

    /// Deletes a track, the tracks selected with it when it is one of them
    /// (`tracks_with`), in one undo; when they hold events, asks first
    /// (Options › Preferences › General: "Provide a warning if a track
    /// being deleted holds at least one event").
    pub(crate) fn request_delete_track(&mut self, id: TrackId) {
        let ids = self.tracks_with(id);
        let p = self.project();
        let events: usize = ids
            .iter()
            .filter_map(|&t| p.track(t))
            .map(|t| t.clips.len())
            .sum();
        if events == 0 || !self.settings.prefs.warn_track_delete {
            self.delete_tracks(&ids);
            return;
        }
        let text = match ids.as_slice() {
            [one] => {
                let name = p
                    .track(*one)
                    .map(crate::i18n::track_name)
                    .unwrap_or_default();
                trn!(
                    events,
                    "{1} holds {0} event. Delete the track and its event?",
                    "{1} holds {0} events. Delete the track and its events?",
                    events,
                    name
                )
            }
            _ => trn!(
                events,
                "The tracks selected hold {0} event. Delete the tracks and their event?",
                "The tracks selected hold {0} events. Delete the tracks and their events?",
                events
            ),
        };
        self.ask_user(Ask {
            title: tr("Delete Track").into(),
            text,
            what: Asked::DeleteTracks(ids),
            check: None,
        });
    }

    /// At startup: when BananaV does not open its project files, offers to
    /// (Options › Preferences › General: "Check project file type
    /// associations at startup").
    pub(crate) fn check_associations(&mut self) {
        if !self.settings.prefs.check_associations
            || bv_platform::opens_project_files(PROJECT_EXTENSION) != Some(false)
        {
            return;
        }
        self.ask_user(Ask {
            title: tr("Project Files").into(),
            text: tr(
                "BananaV is not the program that opens BananaV projects (.bvproj). Make it the one?",
            )
            .into(),
            what: Asked::Associate,
            check: Some((
                tr("Check project file type associations at startup"),
                |p| &mut p.check_associations,
            )),
        });
    }
}

/// BananaV's project files.
pub(crate) const PROJECT_EXTENSION: &str = "bvproj";
/// The type Windows knows them by.
const PROJECT_TYPE: &str = "BananaV.Project";

#[cfg(test)]
mod tests {
    use super::*;
    use bv_core::{FrameRate, Tick};
    use bv_timeline::{MediaInfo, MediaSource, VideoStreamInfo};

    fn app() -> BananaApp {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        app
    }

    #[test]
    fn media_in_use_is_removed_after_asking() {
        let mut app = app();
        let used = app.project().clips().next().map(|(_, c)| c.media).unwrap();
        app.request_remove_media(used);
        assert!(app.project().media(used).is_some(), "asked first");
        assert!(matches!(
            app.ask.as_ref().map(|a| &a.what),
            Some(Asked::RemoveMedia(_))
        ));
        let what = app.ask.take().unwrap().what;
        app.answer_yes(what);
        assert!(app.project().media(used).is_none());
        assert!(app.project().clips().all(|(_, c)| c.media != used));
        // Without the preference, at once.
        let mut app = self::app();
        app.settings.prefs.confirm_media_removal = false;
        app.request_remove_media(used);
        assert!(app.ask.is_none() && app.project().media(used).is_none());
    }

    #[test]
    fn a_track_with_events_is_deleted_after_warning() {
        let mut app = app();
        let track = app.project().tracks[0].id;
        // VEGAS warns only when asked to.
        app.request_delete_track(track);
        assert!(app.ask.is_none() && app.project().track(track).is_none());
        let mut app = self::app();
        app.settings.prefs.warn_track_delete = true;
        let track = app.project().tracks[0].id;
        app.request_delete_track(track);
        assert!(app.project().track(track).is_some(), "warned first");
        let what = app.ask.take().unwrap().what;
        app.answer_yes(what);
        assert!(app.project().track(track).is_none());
    }

    #[test]
    fn fx_parameter_changes_without_undos() {
        let mut app = app();
        let (id, clip) = app
            .project()
            .clips()
            .find(|(_, c)| !c.effects.is_empty())
            .map(|(_, c)| (c.id, c.clone()))
            .expect("a clip with effects");
        let chain = move |p: &bv_timeline::Project| p.clip(id).map(|c| c.effects.clone());
        let set = move |p: &mut bv_timeline::Project, effects| {
            if let Some(c) = p.clip_mut(id) {
                c.effects = effects;
            }
        };
        let mut effects = clip.effects.clone();
        effects[0].params[0] = 0.25;
        let key = egui::Id::new("test");
        // Off: the change makes no undo, and undoing other edits keeps it.
        app.settings.prefs.fx_undo = false;
        app.modify(|p| p.loop_region = None);
        app.set_fx_params(key, effects.clone(), chain, set);
        assert!(app.history.undo());
        let now = |app: &BananaApp| app.project().clip(id).unwrap().effects[0].params[0];
        assert_eq!(now(&app), 0.25);
        // On: one undo.
        app.settings.prefs.fx_undo = true;
        effects[0].params[0] = 0.75;
        app.set_fx_params(key, effects, chain, set);
        assert_eq!(now(&app), 0.75);
        assert!(app.history.undo());
        assert_eq!(now(&app), 0.25);
    }

    #[test]
    fn the_first_video_offers_its_settings() {
        let mut app = BananaApp::new(egui::Context::default());
        let rate = FrameRate::FPS_25;
        let info = MediaInfo {
            id: bv_timeline::MediaId(0),
            source: MediaSource::File {
                path: "C:/nowhere/camera.mp4".into(),
            },
            name: "camera.mp4".into(),
            duration: Tick::from_seconds(5),
            video: Some(VideoStreamInfo {
                width: 3840,
                height: 2160,
                frame_rate: rate,
                pixel_aspect: Default::default(),
                field_order: Default::default(),
                spherical: false,
                full_range: false,
                reading: Default::default(),
            }),
            audio: None,
            extra_audio: Vec::new(),
            still: false,
            sequence: None,
            effects: Vec::new(),
            markers: Vec::new(),
            color_space: None,
            reel: Default::default(),
            layer: None,
        };
        let id = app.modify(|p| p.add_media(info));
        app.place_media(id, Tick::ZERO, None);
        assert_eq!(app.project().clips().count(), 1, "placed at once");
        assert_ne!(
            app.project().settings.width,
            3840,
            "not changed without asking"
        );
        let what = app.ask.take().expect("asked").what;
        app.answer_yes(what);
        let s = app.project().settings;
        assert_eq!((s.width, s.height, s.frame_rate), (3840, 2160, rate));
    }
}
