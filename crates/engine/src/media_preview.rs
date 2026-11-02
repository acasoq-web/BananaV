//! Previews of media (Explorer's and Project Media's Start Preview, the
//! Trimmer's Play), as VEGAS's Help describes them: a media plays alone,
//! through the preview bus — the Master Bus window's Preview fader —, not
//! the project's mix. The engine plays a project of its own holding one
//! event of the media, so the playback's machinery (devices, buffering,
//! resampling) serves it as it serves the timeline; the timeline's
//! playback stops meanwhile.

use std::sync::Arc;

use bv_core::{Tick, TimeRange};
use bv_timeline::{MediaInfo, MediaSource, Project, ProjectSettings, TrackKind};

use crate::Engine;
use crate::playback::{PlayOptions, Playback};

/// A preview playing.
pub(crate) struct MediaPreview {
    pub(crate) playback: Playback,
    project: Arc<Project>,
}

impl MediaPreview {
    /// Whether it plays media `info` (the file the Trimmer shows).
    pub(crate) fn plays(&self, info: &MediaInfo) -> bool {
        self.project.media.iter().any(|m| m.source == info.source)
    }
}

/// The project a preview plays: media `info` from the timeline's start on a
/// track of each kind it has, the master at `gain_db`.
pub(crate) fn preview_project(
    settings: &ProjectSettings,
    info: &MediaInfo,
    gain_db: f32,
) -> Project {
    let mut p = Project::new(*settings);
    let video = p
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Video)
        .map(|t| t.id);
    let audio = p
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Audio)
        .map(|t| t.id);
    let m = p.add_media(info.clone());
    let _ = p.insert_media(m, Tick::ZERO, info.video.and(video), info.audio.and(audio));
    // Stills and generated media play as long as their event.
    p.master.gain_db = gain_db;
    p
}

/// How long a still or a generated media plays in a sequence's preview.
const STILL: Tick = Tick::from_seconds(5);

/// The project a sequence's preview plays (a storyboard's cells): each
/// media, or its part, end to end from the timeline's start, on a track
/// of each kind; stills and generated media five seconds.
pub(crate) fn sequence_project(
    settings: &ProjectSettings,
    items: &[(MediaInfo, Option<TimeRange>)],
    gain_db: f32,
) -> Project {
    let mut p = Project::new(*settings);
    let track = |p: &Project, k| p.tracks.iter().find(|t| t.kind == k).map(|t| t.id);
    let (video, audio) = (track(&p, TrackKind::Video), track(&p, TrackKind::Audio));
    let mut at = Tick::ZERO;
    for (info, range) in items {
        let still = info.still || matches!(info.source, MediaSource::Generated(_));
        let (from, length) = match range {
            Some(r) => (r.start, r.duration),
            None if still => (Tick::ZERO, STILL),
            None => (Tick::ZERO, info.duration),
        };
        if length <= Tick::ZERO {
            continue;
        }
        let m = p.add_media(info.clone());
        let _ = p.insert_media_range(
            m,
            at,
            from,
            length,
            info.video.and(video),
            info.audio.and(audio),
        );
        at += length;
    }
    p.master.gain_db = gain_db;
    p
}

impl Engine {
    /// Plays media `info` alone through the preview bus from `from` (its
    /// time), at `gain_db` (the Preview fader); the timeline's playback
    /// stops.
    pub fn start_media_preview(&mut self, info: &MediaInfo, from: Tick, gain_db: f32) {
        let project = preview_project(&self.project.settings, info, gain_db);
        self.play_preview(project, from);
    }

    /// Plays `items` (media, or their parts) one after another through the
    /// preview bus: a storyboard's Start Preview.
    pub fn start_sequence_preview(
        &mut self,
        items: &[(MediaInfo, Option<TimeRange>)],
        gain_db: f32,
    ) {
        let project = sequence_project(&self.project.settings, items, gain_db);
        self.play_preview(project, Tick::ZERO);
    }

    fn play_preview(&mut self, project: Project, from: Tick) {
        self.stop();
        self.media_preview = None;
        let project = Arc::new(project);
        let playback = Playback::start(
            project.clone(),
            from.max(Tick::ZERO),
            PlayOptions::default(),
            self.audio_out.clone(),
        );
        self.media_preview = Some(MediaPreview { playback, project });
    }

    /// Stops the preview; where it was.
    pub fn stop_media_preview(&mut self) -> Option<Tick> {
        self.media_preview.take().map(|p| p.playback.position())
    }

    /// Where the preview is, while it plays (`None` once it has played to
    /// the media's end or was stopped).
    pub fn media_preview_position(&self) -> Option<Tick> {
        self.media_preview
            .as_ref()
            .filter(|p| !p.playback.finished())
            .map(|p| p.playback.position())
    }

    /// The preview's peaks (linear, a channel each) since the last call:
    /// the Preview fader's meter.
    pub fn take_preview_peaks(&self) -> Option<Vec<f32>> {
        self.media_preview
            .as_ref()
            .and_then(|p| p.playback.take_peaks())
    }

    /// The Preview fader moved: the preview plays at `gain_db`.
    pub fn set_preview_gain(&mut self, gain_db: f32) {
        let Some(p) = &mut self.media_preview else {
            return;
        };
        if p.project.master.gain_db == gain_db {
            return;
        }
        let mut project = (*p.project).clone();
        project.master.gain_db = gain_db;
        p.project = Arc::new(project);
        p.playback.set_project(p.project.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preview_plays_the_media_alone() {
        let settings = ProjectSettings::default();
        let tone = MediaInfo::generated(
            bv_timeline::Generator::TestTone {
                frequency_hz: 440.0,
                level_db: -6.0,
            },
            &settings,
        );
        let p = preview_project(&settings, &tone, -3.0);
        let clips: Vec<_> = p.clips().collect();
        assert_eq!(clips.len(), 1, "one event: its sound");
        assert_eq!(clips[0].0.kind, TrackKind::Audio);
        assert_eq!(clips[0].1.start, Tick::ZERO);
        assert_eq!(p.master.gain_db, -3.0);
    }

    /// The Trimmer's project is made once for a media (each frame of its
    /// playback asks for one), and anew when the media is read otherwise.
    #[test]
    fn the_trimmers_project_is_made_once_for_its_media() {
        let engine = Engine::new(Arc::new(|| {}));
        let settings = ProjectSettings::default();
        let bars = MediaInfo::generated(bv_timeline::Generator::ColorBars, &settings);
        let kept = |e: &Engine| e.trimmer_project.lock().unwrap().clone().unwrap();
        engine.request_trimmer(&bars, Tick::ZERO, 320, 180);
        let first = kept(&engine);
        engine.request_trimmer(&bars, Tick::from_seconds(1), 320, 180);
        assert!(Arc::ptr_eq(&first.1, &kept(&engine).1), "the same project");
        let mut turned = bars.clone();
        turned.video = turned
            .video
            .map(|v| v.turned_to(bv_timeline::MediaRotation::Clockwise));
        engine.request_trimmer(&turned, Tick::from_seconds(1), 320, 180);
        assert_ne!(first.0, kept(&engine).0, "turned: another picture");
    }

    #[test]
    fn a_sequence_plays_its_media_end_to_end() {
        let settings = ProjectSettings::default();
        let bars = MediaInfo::generated(bv_timeline::Generator::ColorBars, &settings);
        let s = Tick::from_seconds;
        let part = TimeRange::new(s(1), s(2));
        let p = sequence_project(&settings, &[(bars.clone(), None), (bars, Some(part))], 0.0);
        let mut clips: Vec<_> = p
            .clips()
            .map(|(_, c)| (c.start, c.duration, c.source_in))
            .collect();
        clips.sort();
        // A generated media five seconds, then the part of it.
        assert_eq!(clips, [(s(0), s(5), s(0)), (s(5), s(2), s(1))]);
    }
}
