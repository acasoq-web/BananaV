//! Actions behind the Insert, Tools and Options menus.

use crate::i18n::tr;
use std::path::{Path, PathBuf};

use bv_core::{Tick, TimeRange, timecode};
use bv_timeline::{
    ChannelMap, ClipId, EnvelopeKind, Generator, MediaId, MediaInfo, MediaSource, TrackId,
    TrackKind,
};

use super::BananaApp;

/// Built-in scripts listed under Tools › Scripting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Script {
    ExportChapters,
    /// The closed captions as SubRip, SAMI, QuickTime text or DVD
    /// subtitles (VEGAS's "Export Closed Captioning for…" scripts).
    ExportCaptionsSubRip,
    ExportCaptionsSami,
    ExportCaptionsQuickTime,
    ExportCaptionsDvd,
    ExportRegionsAsSubtitles,
    GroupVideoAndAudio,
    /// The captions of the selected events' media as caption markers.
    PromoteMediaCaptions,
    RemoveLetterboxing,
    StereoToMono,
}

impl Script {
    /// Script names as VEGAS shows them (they are file names there).
    pub fn label(self) -> &'static str {
        match self {
            Script::ExportChapters => "Export Chapters",
            Script::ExportCaptionsSubRip => "Export Closed Captioning as SubRip",
            Script::ExportCaptionsSami => "Export Closed Captioning as SAMI",
            Script::ExportCaptionsQuickTime => "Export Closed Captioning as QuickTime Text",
            Script::ExportCaptionsDvd => "Export Closed Captioning as DVD Subtitles",
            Script::ExportRegionsAsSubtitles => "Export Regions as Subtitles",
            Script::GroupVideoAndAudio => "Group Video and Audio Events",
            Script::PromoteMediaCaptions => "Promote Media Closed Captioning",
            Script::RemoveLetterboxing => "Remove Letterboxing",
            Script::StereoToMono => "Stereo to Mono",
        }
    }
}

/// "00:01:02,345" for SubRip files.
fn srt_time(t: Tick) -> String {
    let ms = (t.as_seconds_f64() * 1000.0).round().max(0.0) as u64;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        ms % 1000
    )
}

/// Opens `path` with the program Windows associates with it.
/// Opens `path` with `editor` (Options › Preferences › Audio: "Preferred
/// audio editor"), or with the system's program for it.
fn open_in_editor(editor: Option<&Path>, path: &Path) -> std::io::Result<()> {
    match editor {
        Some(e) => std::process::Command::new(e).arg(path).spawn().map(|_| ()),
        None => open_externally(path),
    }
}

pub(crate) fn open_externally(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(path)
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
    }
}

/// The system's file manager on the folder of `path`, the file picked
/// (Project Media's Explore Containing Folder).
pub(crate) fn reveal_in_folder(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(windows))]
    {
        open_externally(path.parent().unwrap_or(path))
    }
}

/// `dir/name`, or `dir/stem (2).ext`, … when that name is taken.
fn free_name(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p
        .file_stem()
        .map_or(String::new(), |s| s.to_string_lossy().to_string());
    let ext = p
        .extension()
        .map_or(String::new(), |s| format!(".{}", s.to_string_lossy()));
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|c| !c.exists())
        .expect("some free name")
}

impl BananaApp {
    // ---- tracks -----------------------------------------------------------------

    /// The track an Insert/Tools command acts on: the selected track or the
    /// selected event's track when it is of `kind`, otherwise the first
    /// track of `kind`.
    pub(crate) fn current_track(&self, kind: TrackKind) -> Option<TrackId> {
        let p = self.project();
        self.selected_track
            .and_then(|id| p.track(id))
            .filter(|t| t.kind == kind)
            .or_else(|| self.primary_clip().and_then(|id| p.clip_track(id)))
            .filter(|t| t.kind == kind)
            .or_else(|| p.tracks.iter().find(|t| t.kind == kind))
            .map(|t| t.id)
    }

    /// Insert › Video Track (at the top of the track list) and Audio Track
    /// (at its end), as VEGAS's; the new track is selected.
    pub(crate) fn insert_track(&mut self, kind: TrackKind) {
        let t = self.modify(|p| match kind {
            TrackKind::Video => p.insert_track_at(kind, 0),
            TrackKind::Audio => {
                let end = p.tracks.len();
                p.insert_track_at(kind, end)
            }
        });
        self.selected_track = Some(t);
    }

    /// Insert › Adjustment Track (at the top, as Insert › Video Track), or a
    /// track menu's Insert Adjustment Track (above `above`); selected.
    /// A video track's Bypass Motion Blur (its header's button and More
    /// menu; the selected video tracks with it), toggled.
    pub(crate) fn toggle_bypass_motion_blur(&mut self, track: bv_timeline::TrackId) {
        let on = !self
            .project()
            .track(track)
            .is_some_and(|t| t.bypass_motion_blur);
        let mut ids = self.track_selection();
        if !ids.contains(&track) {
            ids = vec![track];
        }
        self.modify(|p| {
            for id in ids {
                if let Some(t) = p.track_by_id_mut(id).filter(|t| t.kind == TrackKind::Video) {
                    t.bypass_motion_blur = on;
                }
            }
        });
    }

    pub(crate) fn insert_adjustment_track(&mut self, above: Option<bv_timeline::TrackId>) {
        // With its Composite envelope along its top, as VEGAS's: the
        // effects' strength over time (its points as Preferences says).
        let curve = self
            .settings
            .prefs
            .new_envelope_curve(EnvelopeKind::CompositeLevel);
        let t = self.modify(|p| {
            let at = above.and_then(|t| p.track_index(t)).unwrap_or(0);
            let id = p.insert_adjustment_track_at(at);
            let _ = p.toggle_envelope_as(id, EnvelopeKind::CompositeLevel, curve);
            id
        });
        self.selected_track = Some(t);
        self.timeline.show_envelopes = true;
    }

    /// Insert › Adjustment Event: one at the cursor on the selected video
    /// track (the first other one when that is an adjustment track).
    pub(crate) fn insert_adjustment_event(&mut self) {
        let track = self.selected_track.filter(|&t| {
            self.project()
                .track(t)
                .is_some_and(|t| t.kind == TrackKind::Video && !t.adjustment)
        });
        let at = self.cursor;
        self.add_generator(bv_timeline::Generator::Adjustment, at, track);
    }

    /// Edit › Resize Selected / All Adjustment Events to Project.
    pub(crate) fn resize_adjustments(&mut self, all: bool) {
        let p = self.project();
        let ids: Vec<bv_timeline::ClipId> = p
            .clips()
            .filter(|(_, c)| all || self.selection.contains(&c.id))
            .filter(|(_, c)| p.media(c.media).is_some_and(|m| m.is_adjustment()))
            .map(|(_, c)| c.id)
            .collect();
        self.modify(|p| {
            for id in ids {
                let _ = p.resize_adjustment_to_project(id);
            }
        });
    }

    /// Whether the selection holds an adjustment event (`all`: whether the
    /// project does).
    pub(crate) fn has_adjustments(&self, all: bool) -> bool {
        let p = self.project();
        p.clips().any(|(_, c)| {
            (all || self.selection.contains(&c.id))
                && p.media(c.media).is_some_and(|m| m.is_adjustment())
        })
    }

    /// A track's shortcut menu's Insert Video/Audio Track: a new track of
    /// `kind` right above `track`, selected.
    pub(crate) fn insert_track_above(&mut self, kind: TrackKind, track: bv_timeline::TrackId) {
        let t = self.modify(|p| {
            let at = p.track_index(track).unwrap_or(0);
            p.insert_track_at(kind, at)
        });
        self.selected_track = Some(t);
    }

    /// Duplicate Track: a copy below it, selected.
    pub(crate) fn duplicate_track(&mut self, track: TrackId) {
        if let Some(t) = self.edit(|p| p.duplicate_track(track)) {
            self.selected_track = Some(t);
        }
    }

    /// An event's Select in Project Media list: its media chosen there, in
    /// view.
    pub(crate) fn select_in_project_media(&mut self, media: bv_timeline::MediaId) {
        self.selected_media = Some(media);
        self.media_window.bin = crate::tabs::media::BinChoice::All;
        self.show_tab(crate::dock::Tab::ProjectMedia, false);
    }

    /// Select Events to End: each selected event and every event after it
    /// on its track.
    pub(crate) fn select_events_to_end(&mut self) {
        let p = self.project();
        let mut add = Vec::new();
        for &id in &self.selection {
            if let (Some(c), Some(t)) = (p.clip(id), p.clip_track(id)) {
                add.extend(t.clips.iter().filter(|o| o.start >= c.start).map(|o| o.id));
            }
        }
        self.selection.extend(add);
    }

    /// Whether the current track of `kind` has the envelope (`None`
    /// without such a track).
    pub(crate) fn has_envelope(&self, track_kind: TrackKind, env: EnvelopeKind) -> Option<bool> {
        let t = self.current_track(track_kind)?;
        Some(
            self.project()
                .track(t)
                .is_some_and(|t| t.envelopes.iter().any(|e| e.kind == env)),
        )
    }

    pub(crate) fn toggle_envelope(&mut self, track_kind: TrackKind, env: EnvelopeKind) {
        let Some(t) = self.current_track(track_kind) else {
            return;
        };
        // A track's first Fade to Color envelope takes the default colors
        // (Options › Preferences › Video: "Default track fade colors").
        let colors = [
            self.settings.prefs.fade_top,
            self.settings.prefs.fade_bottom,
        ];
        // Its points start as Options › Preferences › External Control &
        // Automation says ("New audio envelopes", "New video envelopes").
        let curve = self.settings.prefs.new_envelope_curve(env);
        let added = self.edit(|p| {
            let added = p.toggle_envelope_as(t, env, curve)?;
            if added
                && env == EnvelopeKind::FadeToColor
                && let Some(track) = p.tracks.iter_mut().find(|tr| tr.id == t)
            {
                track.fade_colors.get_or_insert(colors);
            }
            Ok::<_, bv_timeline::EditError>(added)
        });
        if added == Some(true) {
            self.timeline.show_envelopes = true;
            match env {
                EnvelopeKind::Volume | EnvelopeKind::Send(_) => {
                    self.timeline.show_env_volume = true
                }
                EnvelopeKind::Pan => self.timeline.show_env_pan = true,
                EnvelopeKind::Mute => self.timeline.show_env_mute = true,
                EnvelopeKind::CompositeLevel => self.timeline.show_env_composite = true,
                EnvelopeKind::FadeToColor => self.timeline.show_env_fade = true,
                EnvelopeKind::Velocity => self.timeline.show_env_velocity = true,
                EnvelopeKind::TransitionProgress => self.timeline.show_env_progress = true,
                EnvelopeKind::MotionBlur => self.timeline.show_env_motion_blur = true,
                EnvelopeKind::Supersampling => self.timeline.show_env_supersampling = true,
                EnvelopeKind::FxParam { .. } | EnvelopeKind::FxBypass(_) => {}
            }
        }
    }

    /// View › Audio Envelopes' Volume and Pan as VEGAS's V and P ("Insert/
    /// show/hide track volume envelope"): the selected audio tracks without
    /// the envelope get it, shown; when they all have it, that kind is
    /// shown or hidden. With no track selected, the current audio track's.
    pub(crate) fn show_track_envelope(&mut self, env: EnvelopeKind) {
        let selection = self.track_selection();
        let p = self.project();
        let mut tracks: Vec<TrackId> = selection
            .iter()
            .copied()
            .filter(|&t| p.track(t).is_some_and(|t| t.kind == TrackKind::Audio))
            .collect();
        if selection.is_empty() {
            tracks.extend(self.current_track(TrackKind::Audio));
        }
        let missing: Vec<TrackId> = tracks
            .into_iter()
            .filter(|&t| {
                p.track(t)
                    .is_some_and(|t| !t.envelopes.iter().any(|e| e.kind == env))
            })
            .collect();
        let view = &self.timeline;
        let shown = view.show_envelopes
            && match env {
                EnvelopeKind::Pan => view.show_env_pan,
                _ => view.show_env_volume,
            };
        if missing.is_empty() {
            self.set_audio_envelope_shown(env, !shown);
            return;
        }
        // Its points start as Options › Preferences › External Control &
        // Automation says ("New audio envelopes"); one undo for them all.
        let curve = self.settings.prefs.new_envelope_curve(env);
        self.edit(|p| -> Result<(), bv_timeline::EditError> {
            for &t in &missing {
                p.toggle_envelope_as(t, env, curve)?;
            }
            Ok(())
        });
        self.set_audio_envelope_shown(env, true);
    }

    /// The volume (or pan) envelopes shown or hidden, envelopes shown with
    /// them.
    fn set_audio_envelope_shown(&mut self, env: EnvelopeKind, on: bool) {
        let v = &mut self.timeline;
        if on {
            v.show_envelopes = true;
        }
        match env {
            EnvelopeKind::Pan => v.show_env_pan = on,
            _ => v.show_env_volume = on,
        }
    }

    /// The selected video events that can have a `kind` envelope (a
    /// transition progress envelope needs a transition).
    fn event_envelope_targets(&self, kind: EnvelopeKind) -> Vec<ClipId> {
        let p = self.project();
        self.selected_of(TrackKind::Video)
            .into_iter()
            .filter(|&id| {
                kind != EnvelopeKind::TransitionProgress
                    || p.clip(id)
                        .zip(p.clip_track(id))
                        .is_some_and(|(c, t)| t.effective_fades(c, p.auto_crossfade).0 > Tick::ZERO)
            })
            .collect()
    }

    /// Whether every selected video event that can have it has its `kind`
    /// envelope (`None`: none can).
    pub(crate) fn has_event_envelope(&self, kind: EnvelopeKind) -> Option<bool> {
        let ids = self.event_envelope_targets(kind);
        if ids.is_empty() {
            return None;
        }
        let p = self.project();
        Some(
            ids.iter()
                .all(|&id| p.clip(id).is_some_and(|c| c.envelope(kind).is_some())),
        )
    }

    /// Insert › Video Envelopes › Event Velocity / Transition Progress, and
    /// an event's Insert/Remove Envelope: the selected video events' `kind`
    /// envelope added — removed when they all have it.
    pub(crate) fn toggle_event_envelopes(&mut self, kind: EnvelopeKind) {
        let Some(all) = self.has_event_envelope(kind) else {
            return;
        };
        let ids = self.event_envelope_targets(kind);
        self.modify(|p| {
            for id in ids {
                if p.clip(id).is_some_and(|c| c.envelope(kind).is_some()) == all {
                    let _ = p.toggle_event_envelope(id, kind);
                }
            }
        });
        if !all {
            self.timeline.show_envelopes = true;
            match kind {
                EnvelopeKind::TransitionProgress => self.timeline.show_env_progress = true,
                _ => self.timeline.show_env_velocity = true,
            }
        }
    }

    /// Insert/Remove Envelope › Freeze Frame at Cursor: on each selected
    /// video event under the cursor, the velocity drops to 0 there, the
    /// frame held to the event's end.
    pub(crate) fn freeze_frames_at_cursor(&mut self) {
        let at = self.cursor;
        let ids: Vec<ClipId> = {
            let p = self.project();
            self.selected_of(TrackKind::Video)
                .into_iter()
                .filter(|&id| p.clip(id).is_some_and(|c| at > c.start && at < c.end()))
                .collect()
        };
        if ids.is_empty() {
            return;
        }
        self.modify(|p| {
            for id in ids {
                let _ = p.freeze_frame_at(id, at);
            }
        });
        self.timeline.show_envelopes = true;
        self.timeline.show_env_velocity = true;
    }

    /// Whether Freeze Frame at Cursor has an event to work on.
    pub(crate) fn can_freeze_at_cursor(&self) -> bool {
        let p = self.project();
        let at = self.cursor;
        self.selected_of(TrackKind::Video)
            .iter()
            .any(|&id| p.clip(id).is_some_and(|c| at > c.start && at < c.end()))
    }

    /// Reverse (an event's shortcut menu): the selected events play their
    /// media backward — each reversed one's subclip in Project Media —, or
    /// forward again.
    pub(crate) fn reverse_selected(&mut self) {
        let ids = self.selected();
        if ids.is_empty() {
            return;
        }
        self.modify(|p| {
            for id in ids {
                if p.reverse_event(id).is_ok() {
                    p.add_reversed_subclip(id);
                }
            }
        });
    }

    // ---- Insert ----------------------------------------------------------------------

    /// Insert › Empty Event: a blank event over the time selection (or 5 s
    /// at the cursor) on the selected event's track.
    pub(crate) fn insert_empty_event(&mut self) {
        let kind = self
            .primary_clip()
            .and_then(|id| self.project().clip_track(id).map(|t| t.kind))
            .unwrap_or(TrackKind::Video);
        let Some(track) = self.current_track(kind) else {
            self.notify(tr("There is no track for the empty event."));
            return;
        };
        let range = self
            .project()
            .loop_region
            .filter(|r| r.duration > Tick::ZERO)
            .unwrap_or_else(|| TimeRange::new(self.cursor, Tick::from_seconds(5)));
        let generator = match kind {
            TrackKind::Video => Generator::SolidColor {
                rgba: [0.0, 0.0, 0.0, 0.0],
            },
            TrackKind::Audio => Generator::TestTone {
                frequency_hz: 1000.0,
                level_db: -200.0,
            },
        };
        let mut info = MediaInfo::generated(generator, &self.project().settings);
        info.name = tr("Empty Event").into();
        let ids = self.edit(|p| {
            let m = p.add_media(info);
            let (v, a) = match kind {
                TrackKind::Video => (Some(track), None),
                TrackKind::Audio => (None, Some(track)),
            };
            p.insert_media_range(m, range.start, Tick::ZERO, range.duration, v, a)
        });
        if let Some(ids) = ids {
            self.selection = ids.into_iter().collect();
        }
    }

    /// Insert › Time: opens `dur` of empty time at the cursor on every track.
    pub(crate) fn insert_time(&mut self, dur: Tick) {
        let at = self.cursor;
        self.modify(|p| p.insert_space(at, dur, &[], true));
    }

    /// Insert › Slideshow: places the images one after the other at the
    /// cursor, each `per_image` long, overlapping by `overlap` (crossfade).
    pub(crate) fn insert_slideshow(&mut self, paths: &[PathBuf], per_image: Tick, overlap: Tick) {
        let ids = self.import_paths(paths);
        let Some(track) = self
            .current_track(TrackKind::Video)
            .or_else(|| Some(self.modify(|p| p.add_track(TrackKind::Video))))
        else {
            return;
        };
        let start = self.cursor;
        let step = (per_image - overlap).max(self.frame_rate().frame_duration());
        let placed = self.edit(|p| -> Result<Vec<ClipId>, bv_timeline::EditError> {
            let mut out = Vec::new();
            let mut at = start;
            for &m in &ids {
                if p.media(m).is_none_or(|i| i.video.is_none()) {
                    continue;
                }
                out.extend(p.insert_media_range(
                    m,
                    at,
                    Tick::ZERO,
                    per_image,
                    Some(track),
                    None,
                )?);
                at += step;
            }
            Ok(out)
        });
        if let Some(placed) = placed {
            self.notify(trn!(
                placed.len(),
                "Slideshow: {} image.",
                "Slideshow: {} images.",
                placed.len()
            ));
            self.selection = placed.into_iter().collect();
            self.fit_timeline = true;
        }
    }

    // ---- Tools ---------------------------------------------------------------------

    pub(crate) fn first_selected(&self, kind: TrackKind) -> Option<ClipId> {
        let p = self.project();
        self.selection
            .iter()
            .copied()
            .find(|&id| p.clip_track(id).is_some_and(|t| t.kind == kind))
    }

    pub(crate) fn has_selected(&self, kind: TrackKind) -> bool {
        self.first_selected(kind).is_some()
    }

    /// Tools › Audio › Open in Audio Editor (with `copy`, edits a copy of
    /// the event's file, which VEGAS keeps as a take of the event: the
    /// copy becomes its active take, the original one of its others).
    pub(crate) fn open_in_audio_editor(&mut self, copy: bool) {
        let Some(id) = self.first_selected(TrackKind::Audio) else {
            return;
        };
        let p = self.project();
        if p.clip(id).is_some_and(|c| self.is_nested_project(c.media)) {
            self.notify(tr(
                "A nested project's sound can't be edited in an audio editor.",
            ));
            return;
        }
        let Some(path) = p
            .clip(id)
            .and_then(|c| p.media(c.media))
            .and_then(|m| match &m.source {
                MediaSource::File { path } => Some(PathBuf::from(path)),
                _ => None,
            })
        else {
            self.notify(tr("This event does not come from an audio file."));
            return;
        };
        let target = if copy {
            let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
            let stem = path
                .file_stem()
                .map_or(String::new(), |s| s.to_string_lossy().to_string());
            let ext = path
                .extension()
                .map_or(String::new(), |s| s.to_string_lossy().to_string());
            let dest = free_name(&dir, &trf!("{} copy.{}", stem, ext));
            if let Err(e) = std::fs::copy(&path, &dest) {
                self.report_error(trf!("Could not copy: {}", e));
                return;
            }
            if let Some(&m) = self.import_paths(std::slice::from_ref(&dest)).first() {
                self.modify(|p| {
                    let _ = p.add_take(id, m);
                });
            }
            dest
        } else {
            path
        };
        let editor = self.settings.prefs.audio_editor.clone();
        match open_in_editor(editor.as_deref(), &target) {
            Ok(()) => self.notify(trf!("Opening {} in the audio editor.", target.display())),
            Err(e) => self.report_error(trf!("Could not open the editor: {}", e)),
        }
    }

    /// Tools › Open in Audio Editor for a media file (the Trimmer's).
    /// The Trimmer's Open Copy in Audio Editor: a copy of the media's file
    /// beside it ("name copy.ext"), opened in the audio editor.
    pub(crate) fn open_media_copy_in_audio_editor(&mut self, media: MediaId) {
        if self.is_nested_project(media) {
            self.notify(tr(
                "A nested project's sound can't be edited in an audio editor.",
            ));
            return;
        }
        let path = self.project().media(media).and_then(|m| match &m.source {
            MediaSource::File { path } => Some(PathBuf::from(path)),
            _ => None,
        });
        let Some(path) = path else {
            self.notify(tr("This media does not come from a file."));
            return;
        };
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let stem = path
            .file_stem()
            .map_or(String::new(), |s| s.to_string_lossy().to_string());
        let ext = path
            .extension()
            .map_or(String::new(), |s| s.to_string_lossy().to_string());
        let dest = free_name(&dir, &trf!("{} copy.{}", stem, ext));
        if let Err(e) = std::fs::copy(&path, &dest) {
            self.report_error(trf!("Could not copy the file: {}", e));
            return;
        }
        let editor = self.settings.prefs.audio_editor.clone();
        match open_in_editor(editor.as_deref(), &dest) {
            Ok(()) => self.notify(trf!("Opening {} in the audio editor.", dest.display())),
            Err(e) => self.report_error(trf!("Could not open the editor: {}", e)),
        }
    }

    /// The Trimmer's Save Snapshot to File: the media's frame at `t` (the
    /// Trimmer's cursor, in its file), its own size, as a PNG picture,
    /// which joins Project Media.
    pub(crate) fn save_trimmer_snapshot(&mut self, info: &MediaInfo, t: Tick) {
        let Some(png) = bv_engine::Engine::media_snapshot_png(info, t) else {
            self.notify(tr("This media has no picture to save."));
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr("Save Snapshot to File"))
            .add_filter("PNG", &["png"])
            .set_file_name(trf!("{} snapshot.png", crate::i18n::media_name(info)))
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, png) {
            Ok(()) => {
                self.import_paths(std::slice::from_ref(&path));
                self.notify(trf!("Saved to {}", path.display()));
            }
            Err(e) => self.report_save_error(&path, &e),
        }
    }

    pub(crate) fn open_media_in_audio_editor(&mut self, media: MediaId) {
        if self.is_nested_project(media) {
            self.notify(tr(
                "A nested project's sound can't be edited in an audio editor.",
            ));
            return;
        }
        let path = self.project().media(media).and_then(|m| match &m.source {
            MediaSource::File { path } => Some(PathBuf::from(path)),
            _ => None,
        });
        let Some(path) = path else {
            self.notify(tr("This media does not come from a file."));
            return;
        };
        let editor = self.settings.prefs.audio_editor.clone();
        match open_in_editor(editor.as_deref(), &path) {
            Ok(()) => self.notify(trf!("Opening {} in the audio editor.", path.display())),
            Err(e) => self.report_error(trf!("Could not open the editor: {}", e)),
        }
    }

    pub(crate) fn open_event_fx(&mut self, kind: TrackKind) {
        if let Some(id) = self.first_selected(kind) {
            self.clip_windows.clip_fx = Some(id);
        }
    }

    pub(crate) fn open_track_fx(&mut self, kind: TrackKind) {
        if let Some(t) = self.current_track(kind) {
            self.clip_windows.track_fx = Some(t);
        }
    }

    pub(crate) fn open_media_fx(&mut self) {
        if let Some(c) = self
            .first_selected(TrackKind::Video)
            .and_then(|id| self.project().clip(id))
        {
            self.clip_windows.media_fx = Some((c.media, false));
        }
    }

    pub(crate) fn open_pan_crop(&mut self) {
        if let Some(id) = self.first_selected(TrackKind::Video) {
            self.clip_windows.pan_crop = Some(id);
        }
    }

    /// Tools › Advanced Save: copies every media file into a folder and
    /// saves the project there, pointing at the copies.
    pub(crate) fn advanced_save(&mut self) {
        let Some(dir) = rfd::FileDialog::new()
            .set_title(tr("Advanced Save: folder for the project and media"))
            .pick_folder()
        else {
            return;
        };
        let media_dir = dir.join("Media");
        if let Err(e) = std::fs::create_dir_all(&media_dir) {
            self.report_error(trf!("Could not create the folder: {}", e));
            return;
        }
        let mut moved = Vec::new();
        for m in &self.project().media {
            if let MediaSource::File { path } = &m.source {
                let src = PathBuf::from(path);
                let name = src
                    .file_name()
                    .map_or(tr("media").to_string(), |n| n.to_string_lossy().to_string());
                let dest = free_name(&media_dir, &name);
                match std::fs::copy(&src, &dest) {
                    Ok(_) => moved.push((m.id, dest)),
                    Err(e) => {
                        self.report_error(trf!("Could not copy {}: {}", src.display(), e));
                        return;
                    }
                }
            }
        }
        let n = moved.len();
        self.modify(|p| {
            for (id, dest) in moved {
                if let Some(m) = p.media.iter_mut().find(|m| m.id == id) {
                    m.source = MediaSource::File {
                        path: dest.to_string_lossy().to_string(),
                    };
                }
            }
        });
        let name = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or(tr("Project.bvproj").to_string(), |n| {
                n.to_string_lossy().to_string()
            });
        self.project_path = Some(dir.join(name));
        if self.save(false) {
            self.notify(trn!(
                n,
                "Project and {} media file saved to {}",
                "Project and {} media files saved to {}",
                n,
                dir.display()
            ));
        }
    }

    pub(crate) fn clean_project_media(&mut self) {
        let n = self.modify(|p| p.remove_unused_media());
        self.notify(if n == 0 {
            tr("All project media is in use.").to_string()
        } else {
            trn!(
                n,
                "{} unused media file removed from the project.",
                "{} unused media files removed from the project.",
                n
            )
        });
    }

    /// Tools › Scripting.
    pub(crate) fn run_script(&mut self, s: Script) {
        match s {
            Script::ExportChapters => self.export_markers_text(false),
            Script::ExportRegionsAsSubtitles => self.export_markers_text(true),
            Script::ExportCaptionsSubRip => {
                self.export_closed_captioning(crate::captions_ui::CaptionFormat::SubRip)
            }
            Script::ExportCaptionsSami => {
                self.export_closed_captioning(crate::captions_ui::CaptionFormat::Sami)
            }
            Script::ExportCaptionsQuickTime => {
                self.export_closed_captioning(crate::captions_ui::CaptionFormat::QuickTime)
            }
            Script::ExportCaptionsDvd => {
                self.export_closed_captioning(crate::captions_ui::CaptionFormat::DvdSubtitles)
            }
            Script::PromoteMediaCaptions => {
                self.promote_media_captions();
            }
            Script::GroupVideoAndAudio => {
                let p = self.project();
                let video: Vec<_> = p
                    .clips()
                    .filter(|(t, _)| t.kind == TrackKind::Video)
                    .map(|(_, c)| c.clone())
                    .collect();
                let audio: Vec<_> = p
                    .clips()
                    .filter(|(t, _)| t.kind == TrackKind::Audio)
                    .map(|(_, c)| c.clone())
                    .collect();
                let pairs: Vec<[ClipId; 2]> = video
                    .iter()
                    .filter_map(|v| {
                        audio
                            .iter()
                            .find(|a| {
                                a.media == v.media
                                    && a.start == v.start
                                    && !(a.group.is_some() && a.group == v.group)
                            })
                            .map(|a| [v.id, a.id])
                    })
                    .collect();
                let n = pairs.len();
                self.modify(|p| {
                    for pair in pairs {
                        p.group_clips(&pair);
                    }
                });
                self.notify(trn!(
                    n,
                    "{} video and audio pair grouped.",
                    "{} video and audio pairs grouped.",
                    n
                ));
            }
            Script::RemoveLetterboxing => {
                let p = self.project();
                let s = p.settings;
                let ids: Vec<ClipId> = if self.selection.is_empty() {
                    p.clips()
                        .filter(|(t, _)| t.kind == TrackKind::Video)
                        .map(|(_, c)| c.id)
                        .collect()
                } else {
                    self.selection.iter().copied().collect()
                };
                self.modify(|p| {
                    for id in ids {
                        let Some(v) = p
                            .clip(id)
                            .and_then(|c| p.media(c.media))
                            .and_then(|m| m.video)
                        else {
                            continue;
                        };
                        let (sx, sy) = (
                            s.width as f32 / v.width as f32,
                            s.height as f32 / v.height as f32,
                        );
                        let _ = p.set_clip_framing(
                            id,
                            &bv_timeline::Transform {
                                scale: sx.max(sy) / sx.min(sy),
                                ..bv_timeline::Transform::default()
                            },
                        );
                    }
                });
            }
            Script::StereoToMono => {
                let p = self.project();
                let ids: Vec<ClipId> = if self.selection.is_empty() {
                    p.clips()
                        .filter(|(t, _)| t.kind == TrackKind::Audio)
                        .map(|(_, c)| c.id)
                        .collect()
                } else {
                    self.selection.iter().copied().collect()
                };
                self.modify(|p| {
                    for id in ids {
                        if p.clip_track(id).is_some_and(|t| t.kind == TrackKind::Audio)
                            && let Some(c) = p.clip_mut(id)
                        {
                            c.channels = ChannelMap::Combine;
                        }
                    }
                });
            }
        }
    }

    /// Markers as a chapter list, or regions as a SubRip subtitle file.
    fn export_markers_text(&mut self, subtitles: bool) {
        let (title, filter, ext) = if subtitles {
            (tr("Export Regions as Subtitles"), "SubRip", "srt")
        } else {
            (tr("Export Chapters"), tr("Text"), "txt")
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title(title)
            .add_filter(filter, &[ext])
            .set_file_name(trf!("Chapters.{}", ext))
            .save_file()
        else {
            return;
        };
        let p = self.project();
        let text = if subtitles {
            p.regions
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    format!(
                        "{}\n{} --> {}\n{}\n",
                        i + 1,
                        srt_time(r.range.start),
                        srt_time(r.range.end()),
                        r.name
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            let rate = p.settings.frame_rate;
            p.markers
                .iter()
                .map(|m| format!("{} {}\n", timecode::format(m.time, rate), m.name))
                .collect()
        };
        match std::fs::write(&path, text) {
            Ok(()) => self.notify(trf!("Saved to {}", path.display())),
            Err(e) => self.report_save_error(&path, &e),
        }
    }

    // ---- Options ---------------------------------------------------------------------

    /// Options › Ruler Format › Set Time at Cursor: the ruler shows `shown`
    /// at the cursor from now on.
    pub(crate) fn set_time_at_cursor(&mut self, shown: Tick) {
        let offset = shown - self.cursor;
        self.modify(|p| p.ruler_offset = offset);
    }

    pub(crate) fn toggle_mute_all_audio(&mut self) {
        self.modify(|p| p.master.muted = !p.master.muted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srt_times() {
        assert_eq!(srt_time(Tick::from_seconds_f64(3723.456)), "01:02:03,456");
    }
}
