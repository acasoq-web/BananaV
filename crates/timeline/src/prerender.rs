//! Prerendered video (Tools › Selectively Prerender Video): parts of the
//! project drawn ahead into files, so that heavy parts play at full rate.
//! A part's fingerprint says what it shows: an edit there changes it, and
//! the file is no longer used (edits elsewhere leave it alone).

use bv_core::{Tick, TimeRange};
use serde::{Deserialize, Serialize};

use crate::model::{Clip, Project, TrackKind};

/// The most frames a prerendered part holds.
pub const PART_FRAMES: i64 = 300;

/// A part of the project drawn into a file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prerender {
    pub range: TimeRange,
    /// The file it was drawn into.
    pub file: String,
    /// What the part showed then (see [`Project::fingerprint`]).
    pub fingerprint: u64,
}

impl Project {
    /// What the frames of `range` show, as a number: the video settings,
    /// the video tracks and events there, their media and the output
    /// effects. Equal numbers mean the same frames.
    pub fn fingerprint(&self, range: TimeRange) -> u64 {
        #[derive(Serialize)]
        struct Shown<'a> {
            width: u32,
            height: u32,
            rate: (u32, u32),
            pixel_aspect: (u32, u32),
            format: String,
            gamma: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            color: Option<crate::ColorManagement>,
            quality: String,
            mute_all: bool,
            output_fx: &'a [crate::Effect],
            tracks: Vec<TrackShown<'a>>,
        }
        #[derive(Serialize)]
        struct TrackShown<'a> {
            level: f32,
            muted: bool,
            solo: bool,
            effects: &'a [crate::Effect],
            envelopes: &'a [crate::envelope::Envelope],
            automation: crate::TrackAutomation,
            fade_colors: Option<[[u8; 4]; 2]>,
            clips: Vec<(&'a Clip, Option<&'a crate::MediaInfo>)>,
        }
        let s = &self.settings;
        let shown = Shown {
            width: s.width,
            height: s.height,
            rate: (s.frame_rate.num, s.frame_rate.den),
            pixel_aspect: (s.pixel_aspect.num, s.pixel_aspect.den),
            format: format!("{:?}", s.pixel_format),
            gamma: format!("{:?}", s.compositing_gamma),
            color: s.color_managed().then_some(s.color),
            quality: format!("{:?}", s.render_quality),
            mute_all: self.mute_all_video,
            output_fx: &self.video_output_fx,
            tracks: self
                .tracks
                .iter()
                .filter(|t| t.kind == TrackKind::Video)
                .map(|t| TrackShown {
                    level: t.level,
                    muted: self.track_muted(t),
                    solo: self.track_soloed(t),
                    effects: &t.effects,
                    envelopes: &t.envelopes,
                    automation: t.automation,
                    fade_colors: t.fade_colors,
                    clips: t
                        .clips
                        .iter()
                        .filter(|c| c.start < range.end() && c.end() > range.start)
                        .map(|c| (c, self.media(c.media)))
                        .collect(),
                })
                .collect(),
        };
        let bytes = serde_json::to_vec(&shown).unwrap_or_default();
        // FNV-1a: the same number in every session (saved with the project).
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
            (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
        })
    }

    /// Whether the frames of `range` need more than reading one media file
    /// (transitions, effects, compositing, framing): only those parts are
    /// prerendered.
    pub fn needs_prerender(&self, range: TimeRange) -> bool {
        if !self.video_output_fx.iter().all(|e| !e.enabled) {
            return true;
        }
        let mut layers = 0;
        for t in self.tracks.iter().filter(|t| t.kind == TrackKind::Video) {
            let here: Vec<&Clip> = t
                .clips
                .iter()
                .filter(|c| !c.muted && c.start < range.end() && c.end() > range.start)
                .collect();
            if here.is_empty() {
                continue;
            }
            layers += 1;
            let plain = t.effects.iter().all(|e| !e.enabled)
                && t.envelopes.is_empty()
                && t.level >= 1.0
                && here.len() == 1
                && here.iter().all(|c| {
                    let media_fx = self
                        .media(c.media)
                        .is_some_and(|m| m.effects.iter().all(|e| !e.enabled));
                    c.effects.iter().all(|e| !e.enabled)
                        && media_fx
                        && c.pan_crop.is_default()
                        && !c.mask.as_ref().is_some_and(|m| m.active())
                        && c.level >= 1.0
                        && c.fade_in == Tick::ZERO
                        && c.fade_out == Tick::ZERO
                        && (c.rate - 1.0).abs() < 1e-9
                });
            if !plain || layers > 1 {
                return true;
            }
        }
        false
    }

    /// The parts of `range` (the time selection, or the whole project) that
    /// need prerendering, at most [`PART_FRAMES`] frames each, on the frame
    /// grid.
    pub fn prerender_parts(&self, range: TimeRange) -> Vec<TimeRange> {
        let rate = self.settings.frame_rate;
        let first = rate.frame_at(range.start.max(Tick::ZERO));
        let end = rate.frame_at(range.end() - Tick(1)) + 1;
        let mut parts = Vec::new();
        let mut k = first;
        while k < end {
            let to = (k + PART_FRAMES).min(end);
            let part = TimeRange::from_bounds(rate.frame_start(k), rate.frame_start(to));
            if self.needs_prerender(part) {
                parts.push(part);
            }
            k = to;
        }
        parts
    }

    /// The prerendered part showing `t` that still shows what it did.
    pub fn prerender_at(&self, t: Tick) -> Option<&Prerender> {
        self.prerenders
            .iter()
            .find(|p| p.range.contains(t) && self.fingerprint(p.range) == p.fingerprint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Effect, EffectKind, Generator, MediaInfo, ProjectSettings};

    fn project() -> (Project, crate::ClipId, crate::ClipId) {
        let mut p = Project::new(ProjectSettings::default());
        let v = p.tracks[0].id;
        let bars = p.add_media(MediaInfo::generated(Generator::ColorBars, &p.settings));
        let a = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
        let b = p
            .insert_media(bars, Tick::from_seconds(30), Some(v), None)
            .unwrap()[0];
        (p, a, b)
    }

    #[test]
    fn only_heavy_parts_are_prerendered() {
        let (mut p, a, _) = project();
        let all = TimeRange::new(Tick::ZERO, Tick::from_seconds(40));
        assert!(
            p.prerender_parts(all).is_empty(),
            "plain media plays as it is"
        );
        p.clip_mut(a)
            .unwrap()
            .effects
            .push(Effect::new(EffectKind::Invert));
        let parts = p.prerender_parts(all);
        // The first event lasts the generator's length: its frames, in parts
        // of 300 frames at most.
        assert!(!parts.is_empty());
        let rate = p.settings.frame_rate;
        assert!(
            parts
                .iter()
                .all(|r| rate.frame_at(r.duration) <= PART_FRAMES)
        );
        assert_eq!(parts[0].start, Tick::ZERO);
    }

    #[test]
    fn an_edit_changes_only_its_parts() {
        let (mut p, a, b) = project();
        let early = TimeRange::new(Tick::ZERO, Tick::from_seconds(2));
        let late = TimeRange::new(Tick::from_seconds(31), Tick::from_seconds(2));
        let (f_early, f_late) = (p.fingerprint(early), p.fingerprint(late));
        p.clip_mut(b)
            .unwrap()
            .effects
            .push(Effect::new(EffectKind::Invert));
        assert_eq!(p.fingerprint(early), f_early, "elsewhere: still good");
        assert_ne!(p.fingerprint(late), f_late);
        p.clip_mut(a).unwrap().level = 0.5;
        assert_ne!(p.fingerprint(early), f_early);
    }
}
