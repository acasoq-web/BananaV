//! Multicamera editing, as VEGAS's (Tools › Multicamera): the camera
//! tracks made into one track whose events have a take per camera — an
//! empty "(no camera)" one where a camera did not record —, the take
//! switched at a time (the event split there) or over a time selection,
//! and a multicamera track expanded back into a track per camera.

use std::collections::BTreeMap;

use crate::edit::EditError;
use crate::model::{Clip, ClipId, GroupId, Project, TrackId, TrackKind};
use crate::takes::Take;
use bv_core::{Tick, TimeRange};

type Result<T> = std::result::Result<T, EditError>;

impl Project {
    /// Create Multicamera Track: `tracks`, the cameras (take 1 the topmost),
    /// become one track of each kind among them, where they were: a new
    /// event wherever a camera starts or stops, with a take per camera —
    /// an empty one where it did not record. Each video event is grouped
    /// with the audio one of the same time. The cameras' tracks go (their
    /// envelopes, effects and motion with them, as VEGAS warns). Returns
    /// the new tracks.
    pub fn create_multicamera_track(&mut self, tracks: &[TrackId]) -> Result<Vec<TrackId>> {
        let mut made = Vec::new();
        let mut events: BTreeMap<(Tick, Tick), Vec<ClipId>> = BTreeMap::new();
        for kind in [TrackKind::Video, TrackKind::Audio] {
            // The cameras of this kind, in the track list's order.
            let cams: Vec<TrackId> = self
                .tracks
                .iter()
                .filter(|t| t.kind == kind && tracks.contains(&t.id))
                .map(|t| t.id)
                .collect();
            if cams.is_empty() {
                continue;
            }
            let first = self
                .track_index(cams[0])
                .ok_or(EditError::UnknownTrack(cams[0]))?;
            let clips: Vec<Vec<Clip>> = cams
                .iter()
                .map(|&id| self.track(id).map(|t| t.clips.clone()).unwrap_or_default())
                .collect();
            let mut cuts: Vec<Tick> = clips
                .iter()
                .flatten()
                .flat_map(|c| [c.start, c.end()])
                .collect();
            cuts.sort();
            cuts.dedup();
            let mut new_clips = Vec::new();
            for w in cuts.windows(2) {
                let (a, b) = (w[0], w[1]);
                if b <= a {
                    continue;
                }
                let mid = a + Tick((b - a).0 / 2);
                // Each camera's event there: the rightmost of those that
                // overlap, as VEGAS shows the topmost.
                let covering: Vec<Option<&Clip>> = clips
                    .iter()
                    .map(|cs| cs.iter().rfind(|c| c.start <= mid && mid < c.end()))
                    .collect();
                let Some(active) = covering.iter().position(Option::is_some) else {
                    continue;
                };
                let lead = covering[active].expect("found");
                let id = ClipId(self.alloc_id());
                let source = lead.source_time(a);
                let mut clip = Clip::new(id, lead.media, a, b - a);
                clip.source_in = source;
                clip.looped = lead.looped;
                clip.hold_last = lead.hold_last;
                clip.audio_stream = lead.audio_stream;
                if covering.len() > 1 {
                    clip.takes = covering
                        .iter()
                        .map(|c| match c {
                            Some(c) => Take {
                                media: c.media,
                                offset: c.source_time(a) - source,
                                name: None,
                                none: false,
                            },
                            None => Take {
                                media: lead.media,
                                offset: Tick::ZERO,
                                name: None,
                                none: true,
                            },
                        })
                        .collect();
                    clip.active_take = active;
                }
                events.entry((a, b)).or_default().push(id);
                new_clips.push(clip);
            }
            let track = self.insert_track_at(kind, first);
            if let Some(t) = self.tracks.iter_mut().find(|t| t.id == track) {
                t.name = String::new();
                t.clips = new_clips;
                t.sort();
            }
            for id in cams {
                self.remove_track(id)?;
            }
            made.push(track);
        }
        // Video and audio of the same time, grouped.
        for ids in events.into_values().filter(|v| v.len() > 1) {
            let g = GroupId(self.alloc_id());
            for id in ids {
                if let Some(c) = self.clip_mut(id) {
                    c.group = Some(g);
                }
            }
        }
        Ok(made)
    }

    /// Expand to Multiple Tracks: multicamera track `track` becomes a
    /// track per take, where it was, each event's takes on theirs — the
    /// active ones playing, the others muted (`keep_unused`) or left out.
    /// Returns the new tracks.
    pub fn expand_multicamera_track(
        &mut self,
        track: TrackId,
        keep_unused: bool,
    ) -> Result<Vec<TrackId>> {
        let at = self
            .track_index(track)
            .ok_or(EditError::UnknownTrack(track))?;
        let source = self.tracks[at].clone();
        let n = source.clips.iter().map(Clip::take_count).max().unwrap_or(1);
        let ids: Vec<TrackId> = (0..n)
            .map(|k| self.insert_track_at(source.kind, at + k))
            .collect();
        for c in &source.clips {
            let active = c.active_take();
            for (k, take) in c.take_list().into_iter().enumerate() {
                if take.none || (k != active && !keep_unused) {
                    continue;
                }
                let id = ClipId(self.alloc_id());
                let clip = Clip {
                    id,
                    media: take.media,
                    source_in: c.source_in + take.offset,
                    takes: Vec::new(),
                    active_take: 0,
                    muted: c.muted || k != active,
                    group: None,
                    ..c.clone()
                };
                if let Some(t) = self.tracks.iter_mut().find(|t| t.id == ids[k]) {
                    t.clips.push(clip);
                    t.sort();
                }
            }
        }
        self.remove_track(track)?;
        Ok(ids)
    }

    /// Multicamera editing's cut: event `clip` plays take `k` from `at` on
    /// — split there, or whole when `at` is its start — and so do its
    /// group's events (`with_group`: Edit Multicamera Audio with Video)
    /// that have that take. Returns the events that play it from `at`.
    pub fn switch_take_at(
        &mut self,
        clip: ClipId,
        at: Tick,
        k: usize,
        with_group: bool,
    ) -> Result<Vec<ClipId>> {
        let c = self.clip(clip).ok_or(EditError::UnknownClip(clip))?;
        if k >= c.take_count() || c.take_list()[k].none {
            return Err(EditError::NoTake);
        }
        if at < c.start || at >= c.end() {
            return Err(EditError::NotInsideClip);
        }
        let targets: Vec<ClipId> = if at > c.start {
            let was = self.ignore_groups;
            self.ignore_groups |= !with_group;
            let right = self.split(clip, at);
            self.ignore_groups = was;
            right?
        } else if with_group {
            self.group_members(clip)
        } else {
            vec![clip]
        };
        let mut done = Vec::new();
        for id in targets {
            let fits = self
                .clip(id)
                .is_some_and(|c| k < c.take_count() && !c.take_list()[k].none);
            if fits && self.set_active_take(id, k).is_ok() {
                done.push(id);
            }
        }
        Ok(done)
    }

    /// Multicamera editing's recut of a time selection: track `track`'s
    /// multicamera events over `range` play take `k` — split at its ends,
    /// those that have the take switched to it, the others left as they
    /// are — and so do their groups' (`with_group`); neighbours that then
    /// play on from each other in one take become one event again (as
    /// VEGAS merges them). Returns the events that play `k` there.
    pub fn recut_takes(
        &mut self,
        track: TrackId,
        range: TimeRange,
        k: usize,
        with_group: bool,
    ) -> Result<Vec<ClipId>> {
        self.track(track).ok_or(EditError::UnknownTrack(track))?;
        if range.is_empty() {
            return Ok(Vec::new());
        }
        let was = self.ignore_groups;
        self.ignore_groups |= !with_group;
        let done = self.recut(track, range, k);
        self.ignore_groups = was;
        done
    }

    fn recut(&mut self, track: TrackId, range: TimeRange, k: usize) -> Result<Vec<ClipId>> {
        let events = |p: &Project, keep: &dyn Fn(&Clip) -> bool| -> Vec<ClipId> {
            p.track(track).map_or_else(Vec::new, |t| {
                t.clips
                    .iter()
                    .filter(|c| c.take_count() > 1 && keep(c))
                    .map(|c| c.id)
                    .collect()
            })
        };
        for at in [range.end(), range.start] {
            for id in events(self, &|c| c.start < at && at < c.end()) {
                self.split(id, at)?;
            }
        }
        for id in events(self, &|c| range.start <= c.start && c.end() <= range.end()) {
            for m in self.group_members(id) {
                let fits = self
                    .clip(m)
                    .is_some_and(|c| k < c.take_count() && !c.take_list()[k].none);
                if fits {
                    self.set_active_take(m, k)?;
                }
            }
        }
        for id in events(self, &|c| c.end() >= range.start && c.start <= range.end()) {
            while self.clip(id).is_some() && self.rejoin(id)? {}
        }
        Ok(events(self, &|c| {
            c.start < range.end() && range.start < c.end() && c.active_take() == k
        }))
    }

    /// The topmost event at `at` with more than one take — on the topmost
    /// video track that has one; of those overlapping there, the rightmost
    /// (VEGAS's multicamera view).
    pub fn multicamera_event_at(&self, at: Tick) -> Option<ClipId> {
        self.tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Video)
            .find_map(|t| {
                t.clips
                    .iter()
                    .rfind(|c| c.start <= at && at < c.end() && c.take_count() > 1)
                    .map(|c| c.id)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Generator, MediaInfo, ProjectSettings};

    fn s(n: i64) -> Tick {
        Tick::from_seconds(n)
    }

    /// Two cameras on two video tracks: A from 0 to 10 s, B from 4 to
    /// 12 s.
    fn cameras() -> (Project, TrackId, TrackId) {
        let mut p = Project::new(ProjectSettings::default());
        let solid = |r: f32| {
            MediaInfo::generated(
                Generator::SolidColor {
                    rgba: [r, 0.0, 0.0, 1.0],
                },
                &ProjectSettings::default(),
            )
        };
        let a = p.add_media(solid(1.0));
        let b = p.add_media(solid(0.5));
        let t1 = p.tracks[0].id;
        let t2 = p.insert_track_at(TrackKind::Video, 1);
        let ca = p.insert_media(a, Tick::ZERO, Some(t1), None).unwrap()[0];
        p.clip_mut(ca).unwrap().duration = s(10);
        let cb = p.insert_media(b, s(4), Some(t2), None).unwrap()[0];
        p.clip_mut(cb).unwrap().duration = s(8);
        (p, t1, t2)
    }

    #[test]
    fn camera_tracks_become_one_track_of_takes() {
        let (mut p, t1, t2) = cameras();
        let made = p.create_multicamera_track(&[t1, t2]).unwrap();
        assert_eq!(made.len(), 1);
        let t = p.track(made[0]).unwrap();
        // 0–4 (A only), 4–10 (A and B), 10–12 (B only).
        let spans: Vec<(Tick, Tick)> = t.clips.iter().map(|c| (c.start, c.duration)).collect();
        assert_eq!(spans, vec![(s(0), s(4)), (s(4), s(6)), (s(10), s(2))]);
        let first = &t.clips[0];
        assert_eq!(first.take_count(), 2);
        assert!(first.take_list()[1].none, "no camera B there");
        let last = &t.clips[2];
        assert_eq!(last.active_take(), 1, "only B");
        assert!(p.track(t1).is_none() && p.track(t2).is_none());
        p.check_invariants().unwrap();
    }

    #[test]
    fn switching_takes_cuts_the_event() {
        let (mut p, t1, t2) = cameras();
        let track = p.create_multicamera_track(&[t1, t2]).unwrap()[0];
        let middle = p.track(track).unwrap().clips[1].id;
        // Camera B from 7 s.
        let done = p.switch_take_at(middle, s(7), 1, true).unwrap();
        assert_eq!(done.len(), 1);
        let t = p.track(track).unwrap();
        assert_eq!(t.clips.len(), 4);
        assert_eq!(t.clips[1].active_take(), 0);
        assert_eq!(t.clips[2].active_take(), 1);
        assert_eq!(t.clips[2].start, s(7));
        // An empty take cannot be chosen.
        let first = t.clips[0].id;
        assert!(p.switch_take_at(first, s(1), 1, true).is_err());
        assert_eq!(p.multicamera_event_at(s(8)), Some(done[0]));
        p.check_invariants().unwrap();
    }

    #[test]
    fn a_time_selection_is_recut_to_one_take() {
        let (mut p, t1, t2) = cameras();
        let track = p.create_multicamera_track(&[t1, t2]).unwrap()[0];
        let middle = p.track(track).unwrap().clips[1].id;
        p.switch_take_at(middle, s(7), 1, true).unwrap();
        let spans = |p: &Project| -> Vec<(Tick, Tick, usize)> {
            p.track(track)
                .unwrap()
                .clips
                .iter()
                .map(|c| (c.start, c.end(), c.active_take()))
                .collect()
        };
        // 5–9 s: camera A again, and what was cut at 7 s is one event.
        let done = p
            .recut_takes(track, TimeRange::from_bounds(s(5), s(9)), 0, true)
            .unwrap();
        assert_eq!(done.len(), 1);
        assert_eq!(
            spans(&p),
            vec![
                (s(0), s(4), 0),
                (s(4), s(9), 0),
                (s(9), s(10), 1),
                (s(10), s(12), 1)
            ]
        );
        // Camera B over all of it: where it did not record, A stays; the
        // parts of the one event join, those of the camera's edges not.
        p.recut_takes(track, TimeRange::from_bounds(s(0), s(12)), 1, true)
            .unwrap();
        assert_eq!(
            spans(&p),
            vec![(s(0), s(4), 0), (s(4), s(10), 1), (s(10), s(12), 1)]
        );
        p.check_invariants().unwrap();
    }

    #[test]
    fn a_multicamera_track_expands_back() {
        let (mut p, t1, t2) = cameras();
        let track = p.create_multicamera_track(&[t1, t2]).unwrap()[0];
        let made = p.expand_multicamera_track(track, true).unwrap();
        assert_eq!(made.len(), 2);
        let a = p.track(made[0]).unwrap();
        let b = p.track(made[1]).unwrap();
        // Camera A's events: two playing, B's first muted under A.
        assert_eq!(a.clips.len(), 2);
        assert!(a.clips.iter().all(|c| !c.muted));
        assert_eq!(b.clips.len(), 2);
        assert!(b.clips[0].muted && !b.clips[1].muted);
        p.check_invariants().unwrap();
    }
}
