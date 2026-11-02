//! Putting events in order and in step, as VEGAS's event commands do:
//! Synchronize (By Moving, By Slipping), Add Missing Stream, Shuffle
//! Events, the snap offset flag and Trim Event to Include All Frames.

use bv_core::Tick;

use crate::edit::{EditError, clamp_fades};
use crate::model::{Clip, ClipId, GroupId, Project, TrackKind};

type Result<T> = std::result::Result<T, EditError>;

impl Project {
    /// Synchronize › By Moving: event `id` alone (its group stays) goes
    /// `offset` back along the timeline, `offset` being how far its media is
    /// from its other part's (the out-of-sync badge's time). Returns the
    /// move made.
    pub fn sync_by_moving(&mut self, id: ClipId, offset: Tick) -> Result<Tick> {
        let start = self.clip(id).ok_or(EditError::UnknownClip(id))?.start;
        let ignore = std::mem::replace(&mut self.ignore_groups, true);
        let moved = self.move_clip(id, start - offset, None);
        self.ignore_groups = ignore;
        moved
    }

    /// Synchronize › By Slipping: event `id` alone stays where it is and
    /// its media moves `offset` within it (as far as the media allows).
    /// Returns the slip made, in timeline time.
    pub fn sync_by_slipping(&mut self, id: ClipId, offset: Tick) -> Result<Tick> {
        self.clip(id).ok_or(EditError::UnknownClip(id))?;
        let ignore = std::mem::replace(&mut self.ignore_groups, true);
        let slipped = self.slip_clip(id, -offset);
        self.ignore_groups = ignore;
        slipped.map(|d| -d)
    }

    /// The kind of stream Add Missing Stream for Selected Event would add
    /// for event `id`: its media's other one (the sound of a video event,
    /// the picture of an audio one), when no event of its group plays it.
    pub fn missing_stream(&self, id: ClipId) -> Option<TrackKind> {
        let clip = self.clip(id)?;
        let other = match self.clip_track(id)?.kind {
            TrackKind::Video => TrackKind::Audio,
            TrackKind::Audio => TrackKind::Video,
        };
        let media = self.media(clip.media)?;
        if !media.has(other) {
            return None;
        }
        let there = clip.group.is_some_and(|g| {
            self.clips()
                .any(|(t, c)| c.group == Some(g) && c.media == clip.media && t.kind == other)
        });
        (!there).then_some(other)
    }

    /// Add Missing Stream for Selected Event, as VEGAS's: an event of the
    /// same start and length playing the media's missing stream, on a new
    /// track right below the event's (sound) or right above it (picture),
    /// grouped with it. Returns the new event.
    pub fn add_missing_stream(&mut self, id: ClipId) -> Result<ClipId> {
        let kind = self.missing_stream(id).ok_or(EditError::NothingToInsert)?;
        let clip = self.clip(id).ok_or(EditError::UnknownClip(id))?.clone();
        let track = self.add_track(kind);
        let i = self.track_index(track).expect("just added");
        let new_track = self.tracks.remove(i);
        let (at, _) = self.find_clip(id).ok_or(EditError::UnknownClip(id))?;
        let at = if kind == TrackKind::Audio { at + 1 } else { at };
        self.tracks.insert(at, new_track);
        self.normalize_groups();
        let group = match clip.group {
            Some(g) => g,
            None => GroupId(self.alloc_id()),
        };
        if let Some(c) = self.clip_mut(id) {
            c.group = Some(group);
        }
        let new = ClipId(self.alloc_id());
        let event = Clip {
            group: Some(group),
            source_in: clip.source_in,
            rate: clip.rate,
            looped: clip.looped,
            preserve_pitch: clip.preserve_pitch,
            ..Clip::new(new, clip.media, clip.start, clip.duration)
        };
        self.track_mut(track)?.clips.push(event);
        Ok(new)
    }

    /// Shuffle Events (the right-drag's menu, the Shuffle tool), as VEGAS's:
    /// event `id` (with its group) goes to the edit point of its track
    /// nearest where it was dropped (`to`, its new start), and the events
    /// between there and where it was move over by its length into its
    /// place — the order changes, the span the events take does not.
    pub fn shuffle_events(&mut self, id: ClipId, to: Tick) -> Result<()> {
        let c = self.clip(id).ok_or(EditError::UnknownClip(id))?.clone();
        let members = self.group_members(id);
        let lanes: Vec<usize> = members
            .iter()
            .filter_map(|&m| self.find_clip(m).map(|(t, _)| t))
            .collect();
        if members
            .iter()
            .filter_map(|&m| self.clip(m))
            .any(|m| m.locked)
        {
            return Ok(());
        }
        let (ti, _) = self.find_clip(id).ok_or(EditError::UnknownClip(id))?;
        let (s, e, len) = (c.start, c.end(), c.duration);
        let others: Vec<(Tick, Tick)> = self.tracks[ti]
            .clips
            .iter()
            .filter(|o| !members.contains(&o.id))
            .map(|o| (o.start, o.end()))
            .collect();
        // Earlier: before the start of an event before it; later: its end
        // at the end of an event after it.
        let (delta, from, until, shift) = if to < s {
            let b = others
                .iter()
                .map(|&(start, _)| start)
                .filter(|&x| x < s)
                .chain([s])
                .min_by_key(|&x| (x - to).abs())
                .unwrap_or(s);
            (b - s, b, s, len)
        } else {
            let b = others
                .iter()
                .map(|&(_, end)| end)
                .filter(|&x| x > e)
                .chain([e])
                .min_by_key(|&x| (x - (to + len)).abs())
                .unwrap_or(e);
            (b - e, e, b, -len)
        };
        if delta == Tick::ZERO {
            return Ok(());
        }
        for &t in &lanes {
            for o in &mut self.tracks[t].clips {
                if members.contains(&o.id) {
                    o.start += delta;
                } else if o.start >= from && o.start < until && !o.locked {
                    o.start += shift;
                }
            }
            self.tracks[t].sort();
        }
        Ok(())
    }

    /// Drags event `id`'s snap offset flag to `offset` from its start
    /// (within the event).
    pub fn set_snap_offset(&mut self, id: ClipId, offset: Tick) -> Result<()> {
        let c = self.clip_mut(id).ok_or(EditError::UnknownClip(id))?;
        c.snap_offset = offset.clamp(Tick::ZERO, c.duration);
        Ok(())
    }

    /// Edit › Switches › Trim Event to Include All Frames on event `id`:
    /// on, it is as long as its media from where it starts in it, at its
    /// playback rate, and Loop and Hold Last Frame go off (VEGAS's event
    /// menu has the three as one choice).
    pub fn set_all_frames(&mut self, id: ClipId, on: bool) -> Result<()> {
        let (t, c) = self.find_clip(id).ok_or(EditError::UnknownClip(id))?;
        let clip = &mut self.tracks[t].clips[c];
        clip.all_frames = on;
        if on {
            clip.looped = false;
            clip.hold_last = false;
            self.fit_all_frames(t, c);
        }
        Ok(())
    }

    /// Whether event `id`'s edges hold (Trim Event to Include All Frames on
    /// it or on one of its group).
    pub(crate) fn edges_held(&self, id: ClipId) -> bool {
        self.group_members(id)
            .iter()
            .filter_map(|&m| self.clip(m))
            .any(|m| m.all_frames)
    }

    /// The event at `tracks[t].clips[c]` made as long as its media's frames
    /// from where it starts in it (a still, generated media or a frozen
    /// frame keeps its length).
    pub(crate) fn fit_all_frames(&mut self, t: usize, c: usize) {
        let min = self.min_clip_duration();
        let m = &self.tracks[t].clips[c];
        let Some(info) = self.media(m.media) else {
            return;
        };
        if !info.has_frames() || m.freeze.is_some() {
            return;
        }
        let len = info.duration;
        let m = &mut self.tracks[t].clips[c];
        m.duration = m.to_timeline((len - m.source_in).max(Tick::ZERO)).max(min);
        m.snap_offset = m.snap_offset.min(m.duration);
        clamp_fades(m);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AudioStreamInfo, MediaId, MediaInfo, MediaSource, VideoStreamInfo};
    use bv_core::FrameRate;

    fn movie(p: &mut Project, secs: i64) -> MediaId {
        p.add_media(MediaInfo {
            id: MediaId(0),
            source: MediaSource::File {
                path: "clip.mp4".into(),
            },
            name: "clip".into(),
            duration: Tick::from_seconds(secs),
            video: Some(VideoStreamInfo {
                width: 1920,
                height: 1080,
                frame_rate: FrameRate::FPS_30,
                pixel_aspect: Default::default(),
                field_order: Default::default(),
                spherical: false,
                full_range: false,
                reading: Default::default(),
            }),
            audio: Some(AudioStreamInfo {
                sample_rate: bv_core::SampleRate::HZ_48000,
                channels: 2,
            }),
            extra_audio: Vec::new(),
            sequence: None,
            still: false,
            effects: Vec::new(),
            markers: Vec::new(),
            color_space: None,
            reel: Default::default(),
            layer: None,
        })
    }

    #[test]
    fn out_of_sync_events_come_back_by_moving_or_by_slipping() {
        let mut p = Project::default();
        let (v, a) = (p.tracks[0].id, p.tracks[1].id);
        let m = movie(&mut p, 60);
        let ids = p.insert_media(m, Tick::ZERO, Some(v), Some(a)).unwrap();
        let (video, audio) = (ids[0], ids[1]);
        // The video trimmed 10 s in, then 2 s later on its own: its media
        // starts 2 s after the sound's.
        p.trim_start(video, Tick::from_seconds(10)).unwrap();
        p.trim_start(audio, Tick::from_seconds(10)).unwrap();
        p.ignore_groups = true;
        p.move_clip(video, Tick::from_seconds(12), None).unwrap();
        p.ignore_groups = false;
        let origin = |p: &Project, id| {
            let c: &Clip = p.clip(id).unwrap();
            c.start - c.to_timeline(c.source_in)
        };
        let offset = origin(&p, video) - origin(&p, audio);
        assert_eq!(offset, Tick::from_seconds(2));
        let mut moved = p.clone();
        moved.sync_by_moving(video, offset).unwrap();
        assert_eq!(origin(&moved, video), origin(&moved, audio));
        assert_eq!(moved.clip(audio).unwrap().start, Tick::from_seconds(10));
        let mut slipped = p.clone();
        slipped.sync_by_slipping(video, offset).unwrap();
        assert_eq!(origin(&slipped, video), origin(&slipped, audio));
        assert_eq!(slipped.clip(video).unwrap().start, Tick::from_seconds(12));
    }

    #[test]
    fn a_missing_stream_comes_back_on_a_track_of_its_own_grouped() {
        let mut p = Project::default();
        let v = p.tracks[0].id;
        let m = movie(&mut p, 30);
        let video = p
            .insert_media(m, Tick::from_seconds(3), Some(v), None)
            .unwrap()[0];
        p.trim_start(video, Tick::from_seconds(5)).unwrap();
        assert_eq!(p.missing_stream(video), Some(TrackKind::Audio));
        let audio = p.add_missing_stream(video).unwrap();
        let (a, b) = (p.clip(video).unwrap(), p.clip(audio).unwrap());
        assert_eq!(
            (a.start, a.duration, a.source_in),
            (b.start, b.duration, b.source_in)
        );
        assert!(a.group.is_some() && a.group == b.group);
        // Right below the video's track.
        let at = p.find_clip(audio).unwrap().0;
        assert_eq!(at, p.find_clip(video).unwrap().0 + 1);
        assert_eq!(p.tracks[at].kind, TrackKind::Audio);
        assert_eq!(p.missing_stream(video), None);
        assert_eq!(p.missing_stream(audio), None);
        p.check_invariants().unwrap();
    }

    #[test]
    fn shuffling_moves_an_event_between_others_and_closes_its_place() {
        let mut p = Project::default();
        let v = p.tracks[0].id;
        let m = movie(&mut p, 60);
        let mut ids = Vec::new();
        for k in 0..3 {
            let id = p
                .insert_media(m, Tick::from_seconds(4 * k), Some(v), None)
                .unwrap()[0];
            p.trim_end(id, Tick::from_seconds(4 * k + 4)).unwrap();
            ids.push(id);
        }
        // The third dropped near the second's start: 1, 3, 2.
        p.shuffle_events(ids[2], Tick::from_seconds(5)).unwrap();
        let start = |p: &Project, id| p.clip(id).unwrap().start;
        assert_eq!(start(&p, ids[0]), Tick::ZERO);
        assert_eq!(start(&p, ids[2]), Tick::from_seconds(4));
        assert_eq!(start(&p, ids[1]), Tick::from_seconds(8));
        // And back: the third (now second) dropped past the end.
        p.shuffle_events(ids[2], Tick::from_seconds(9)).unwrap();
        assert_eq!(start(&p, ids[1]), Tick::from_seconds(4));
        assert_eq!(start(&p, ids[2]), Tick::from_seconds(8));
        p.check_invariants().unwrap();
    }

    #[test]
    fn all_frames_follow_the_rate_and_hold_the_edges() {
        let mut p = Project::default();
        let v = p.tracks[0].id;
        let m = movie(&mut p, 20);
        let id = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        p.trim_start(id, Tick::from_seconds(4)).unwrap();
        p.trim_end(id, Tick::from_seconds(10)).unwrap();
        p.set_all_frames(id, true).unwrap();
        // From 4 s into the media to its end: 16 s.
        assert_eq!(p.clip(id).unwrap().duration, Tick::from_seconds(16));
        p.set_clip_rate(id, 0.5).unwrap();
        assert_eq!(p.clip(id).unwrap().duration, Tick::from_seconds(32));
        assert_eq!(p.trim_end(id, Tick::from_seconds(10)).unwrap(), Tick::ZERO);
        assert_eq!(
            p.clip(id).unwrap().duration,
            Tick::from_seconds(36) - Tick::from_seconds(4)
        );
    }
}
