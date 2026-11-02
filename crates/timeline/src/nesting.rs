//! Nested timelines (VEGAS's Create Nested Timeline): selected events taken
//! out into a project of their own, which then takes their place as one
//! event (`bv_media`'s nested projects).

use std::collections::{BTreeMap, BTreeSet};

use bv_core::Tick;

use crate::model::{ClipId, CompositeMode, GroupId, MediaId, Project, TrackId, TrackKind};

/// Events made into a project of their own.
#[derive(Clone, Debug)]
pub struct NestedTimeline {
    /// The new project: the same settings; for each track holding some of
    /// the events, a new track of its kind and name with only them, the
    /// first starting at zero; the media they use. Its tracks are plain:
    /// the track the nested event lands on keeps applying its own effects,
    /// envelopes and motion.
    pub project: Project,
    /// Where the first of the events started.
    pub start: Tick,
    /// The topmost video and audio tracks among the events', where the
    /// nested event goes.
    pub video_track: Option<TrackId>,
    pub audio_track: Option<TrackId>,
}

impl Project {
    /// The events `ids` as a project of their own (`None` when none of them
    /// is in the project). Events grouped together stay grouped, and sync
    /// links between them stay.
    pub fn nested_timeline(&self, ids: &BTreeSet<ClipId>) -> Option<NestedTimeline> {
        let start = self
            .clips()
            .filter(|(_, c)| ids.contains(&c.id))
            .map(|(_, c)| c.start)
            .min()?;
        let mut n = Project::new(self.settings);
        n.tracks.clear();
        let mut media: BTreeMap<MediaId, MediaId> = BTreeMap::new();
        let mut groups: BTreeMap<GroupId, GroupId> = BTreeMap::new();
        let mut moved: BTreeMap<ClipId, ClipId> = BTreeMap::new();
        let (mut video_track, mut audio_track) = (None, None);
        for t in &self.tracks {
            if !t.clips.iter().any(|c| ids.contains(&c.id)) {
                continue;
            }
            match t.kind {
                TrackKind::Video => video_track = video_track.or(Some(t.id)),
                TrackKind::Audio => audio_track = audio_track.or(Some(t.id)),
            }
            let track = n.add_track(t.kind);
            let mut clips = Vec::new();
            for c in t.clips.iter().filter(|c| ids.contains(&c.id)) {
                let mut c = c.clone();
                let id = ClipId(n.alloc_id());
                moved.insert(c.id, id);
                c.id = id;
                c.start -= start;
                c.media = n.media_from(self, c.media, &mut media);
                for take in &mut c.takes {
                    take.media = n.media_from(self, take.media, &mut media);
                }
                c.group = c
                    .group
                    .map(|g| *groups.entry(g).or_insert_with(|| GroupId(n.alloc_id())));
                clips.push(c);
            }
            if let Ok(nt) = n.track_mut(track) {
                nt.name.clone_from(&t.name);
                nt.color = t.color;
                nt.adjustment = t.adjustment;
                nt.clips = clips;
                nt.sort();
            }
        }
        for c in n.tracks.iter_mut().flat_map(|t| t.clips.iter_mut()) {
            c.sync_link = c.sync_link.take().and_then(|mut l| {
                l.to = *moved.get(&l.to)?;
                Some(l)
            });
        }
        Some(NestedTimeline {
            project: n,
            start,
            video_track,
            audio_track,
        })
    }

    /// The media `id` of `from`, added to this project once (`map` keeps
    /// what was added).
    fn media_from(
        &mut self,
        from: &Project,
        id: MediaId,
        map: &mut BTreeMap<MediaId, MediaId>,
    ) -> MediaId {
        if let Some(&m) = map.get(&id) {
            return m;
        }
        let Some(info) = from.media(id).cloned().or_else(|| from.subclip_info(id)) else {
            return id;
        };
        let m = self.add_media(info);
        map.insert(id, m);
        m
    }

    /// Whether nesting the events `ids` may change how they look or sound
    /// (Options › Preferences › General: "Warn if creating a nested
    /// timeline may adversely affect the project"): some are grouped with
    /// events left out, or one of their tracks — other than the topmost of
    /// its kind, where the nested event lands — has effects, envelopes,
    /// motion or a compositing mode of its own, which stop applying to
    /// them.
    pub fn nesting_risks(&self, ids: &BTreeSet<ClipId>) -> bool {
        let picked = |c: &crate::model::Clip| ids.contains(&c.id);
        let groups: BTreeSet<GroupId> = self
            .clips()
            .filter(|(_, c)| picked(c))
            .filter_map(|(_, c)| c.group)
            .collect();
        let split = self
            .clips()
            .any(|(_, c)| !picked(c) && c.group.is_some_and(|g| groups.contains(&g)));
        let mut topmost: Vec<TrackKind> = Vec::new();
        let own = self
            .tracks
            .iter()
            .filter(|t| t.clips.iter().any(picked))
            .filter(|t| {
                let below = topmost.contains(&t.kind);
                topmost.push(t.kind);
                below
            })
            .any(|t| {
                !t.effects.is_empty()
                    || !t.envelopes.is_empty()
                    || t.motion.is_some()
                    || t.composite_mode != CompositeMode::default()
            });
        split || own
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Generator, MediaInfo, ProjectSettings};

    /// Red on the first video track at 2 s and blue on a second one at 3 s,
    /// a tone under the red (grouped with it).
    fn project() -> (Project, ClipId, ClipId, ClipId) {
        let mut p = Project::new(ProjectSettings::default());
        let v1 = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .unwrap()
            .id;
        let a = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .unwrap()
            .id;
        let v2 = p.insert_track_at(TrackKind::Video, 1);
        let color = |p: &mut Project, rgba| {
            p.add_media(MediaInfo::generated(
                Generator::SolidColor { rgba },
                &p.settings,
            ))
        };
        let red = color(&mut p, [1.0, 0.0, 0.0, 1.0]);
        let blue = color(&mut p, [0.0, 0.0, 1.0, 1.0]);
        let tone = p.add_media(MediaInfo::generated(
            Generator::TestTone {
                frequency_hz: 440.0,
                level_db: -6.0,
            },
            &p.settings,
        ));
        let r = p
            .insert_media(red, Tick::from_seconds(2), Some(v1), None)
            .unwrap()[0];
        let b = p
            .insert_media(blue, Tick::from_seconds(3), Some(v2), None)
            .unwrap()[0];
        let t = p
            .insert_media(tone, Tick::from_seconds(2), None, Some(a))
            .unwrap()[0];
        p.group_clips(&[r, t]);
        (p, r, b, t)
    }

    #[test]
    fn nested_events_start_at_zero_on_tracks_of_their_own() {
        let (p, r, b, t) = project();
        let ids: BTreeSet<ClipId> = [r, b, t].into();
        let n = p.nested_timeline(&ids).unwrap();
        assert_eq!(n.start, Tick::from_seconds(2));
        assert_eq!(n.video_track, Some(p.tracks[0].id));
        let q = &n.project;
        q.check_invariants().unwrap();
        assert_eq!(q.tracks.len(), 3);
        let starts: Vec<Tick> = q.tracks.iter().map(|t| t.clips[0].start).collect();
        assert_eq!(
            starts,
            [Tick::ZERO, Tick::from_seconds(1), Tick::ZERO],
            "the order of the tracks kept"
        );
        assert_eq!(q.media.len(), 3);
        // The red and the tone are still a group.
        let g = |k: usize| q.tracks[k].clips[0].group;
        assert!(g(0).is_some());
        assert_eq!(g(0), g(2));
        assert!(!p.nesting_risks(&ids));
    }

    #[test]
    fn leaving_out_a_grouped_event_is_a_risk() {
        let (p, r, b, _) = project();
        assert!(p.nesting_risks(&[r, b].into()));
        assert!(p.nested_timeline(&BTreeSet::new()).is_none());
    }

    #[test]
    fn a_lower_track_s_effects_are_a_risk() {
        let (mut p, r, b, t) = project();
        let ids: BTreeSet<ClipId> = [r, b, t].into();
        // The topmost video track's are not (they go on applying).
        p.tracks[0].composite_mode = CompositeMode::Screen;
        assert!(!p.nesting_risks(&ids));
        p.tracks[1].composite_mode = CompositeMode::Screen;
        assert!(p.nesting_risks(&ids));
    }
}
