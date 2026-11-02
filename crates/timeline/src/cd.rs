//! The audio CD layout (disc-at-once, Red Book), as VEGAS's Help describes
//! it: track regions on the CD layout bar (Insert › Audio CD Track
//! Region), index markers that subdivide them (Insert › Audio CD Track
//! Index), the track list Edit Details shows as Audio CD Track List — the
//! PQ sheet: each track's number, its indices, where it begins and ends,
//! its name, its copy protection and pre-emphasis flags and its ISRC — and
//! Tools › Lay Out Audio CD from Events. Burning the disc is still to come.

use bv_core::{Tick, TimeRange};
use serde::{Deserialize, Serialize};

use crate::model::{MarkerId, Project, TrackKind};

/// A Red Book disc holds up to 99 tracks, each up to 99 indices (index 1
/// is the track's start, so 98 index markers).
pub const MAX_CD_TRACKS: usize = 99;
pub const MAX_CD_INDICES: usize = 98;

/// The silence between tracks Project Media's Add as CD Track leaves, and
/// before the first one: two seconds.
pub const CD_PAUSE: Tick = Tick::from_seconds(2);

/// A track of the disc: a region of the CD layout bar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CdTrack {
    pub id: MarkerId,
    pub range: TimeRange,
    #[serde(default)]
    pub name: String,
    /// The Q subcode's copy protection flag (Prot).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub protect: bool,
    /// The Q subcode's pre-emphasis flag (Emph): BananaV only sets it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub emphasis: bool,
    /// The track's International Standard Recording Code.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub isrc: String,
}

/// An index marker inside a track.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CdIndex {
    pub id: MarkerId,
    pub time: Tick,
}

/// A line of the track list: a track (index 1) or one of its indices.
#[derive(Clone, Debug, PartialEq)]
pub struct CdRow {
    /// The track's id, or the index marker's.
    pub id: MarkerId,
    /// The track's number on the disc.
    pub track: u32,
    /// 1 for the track's start, 2… for its index markers.
    pub index: u32,
    pub start: Tick,
    /// Where the track (or the index's part of it) ends.
    pub end: Tick,
}

impl Project {
    /// A track over `range`: refused (`None`) when it is empty, overlaps
    /// another track or the disc is full.
    pub fn add_cd_track(&mut self, range: TimeRange) -> Option<MarkerId> {
        if range.duration <= Tick::ZERO
            || range.start < Tick::ZERO
            || self.cd_tracks.len() >= MAX_CD_TRACKS
            || self
                .cd_tracks
                .iter()
                .any(|t| t.range.start < range.end() && range.start < t.range.end())
        {
            return None;
        }
        let id = MarkerId(self.alloc_id());
        self.cd_tracks.push(CdTrack {
            id,
            range,
            name: String::new(),
            protect: false,
            emphasis: false,
            isrc: String::new(),
        });
        self.sort_cd();
        Some(id)
    }

    /// An index marker at `t`, inside a track past its start; refused when
    /// it is not, when one is there already or the track has 98.
    pub fn add_cd_index(&mut self, t: Tick) -> Option<MarkerId> {
        let track = self.cd_track_at(t)?.range;
        let inside = |time: Tick| time > track.start && time < track.end();
        if !inside(t)
            || self.cd_indices.iter().any(|i| i.time == t)
            || self.cd_indices.iter().filter(|i| inside(i.time)).count() >= MAX_CD_INDICES
        {
            return None;
        }
        let id = MarkerId(self.alloc_id());
        self.cd_indices.push(CdIndex { id, time: t });
        self.sort_cd();
        Some(id)
    }

    /// The track playing at `t` (its start included, its end not).
    pub fn cd_track_at(&self, t: Tick) -> Option<&CdTrack> {
        self.cd_tracks.iter().find(|c| c.range.contains(t))
    }

    pub fn cd_track(&self, id: MarkerId) -> Option<&CdTrack> {
        self.cd_tracks.iter().find(|c| c.id == id)
    }

    /// Changes a track, its range included (kept off the others: refused
    /// when it would overlap one or be empty).
    pub fn set_cd_track(&mut self, id: MarkerId, edit: impl FnOnce(&mut CdTrack)) -> bool {
        let Some(i) = self.cd_tracks.iter().position(|c| c.id == id) else {
            return false;
        };
        let mut t = self.cd_tracks[i].clone();
        edit(&mut t);
        t.id = id;
        let clash = self
            .cd_tracks
            .iter()
            .any(|o| o.id != id && o.range.start < t.range.end() && t.range.start < o.range.end());
        if t.range.duration <= Tick::ZERO || t.range.start < Tick::ZERO || clash {
            return false;
        }
        self.cd_tracks[i] = t;
        self.sort_cd();
        true
    }

    /// Moves an index marker (it stays inside a track past its start).
    pub fn move_cd_index(&mut self, id: MarkerId, t: Tick) -> bool {
        let ok = self
            .cd_track_at(t)
            .is_some_and(|c| t > c.range.start && t < c.range.end())
            && !self.cd_indices.iter().any(|i| i.time == t && i.id != id);
        if ok && let Some(i) = self.cd_indices.iter_mut().find(|i| i.id == id) {
            i.time = t;
            self.sort_cd();
            return true;
        }
        false
    }

    /// The disc's number of a track (Project Properties › Audio CD's first
    /// track number, then on in time order).
    pub fn cd_track_number(&self, id: MarkerId) -> Option<u32> {
        let first = self.audio_cd.first_track.clamp(1, 99);
        self.cd_tracks
            .iter()
            .position(|c| c.id == id)
            .map(|i| first + i as u32)
    }

    /// The track list: each track, then its index markers in order.
    pub fn cd_rows(&self) -> Vec<CdRow> {
        let mut rows = Vec::new();
        for t in &self.cd_tracks {
            let number = self.cd_track_number(t.id).unwrap_or(1);
            let marks: Vec<&CdIndex> = self
                .cd_indices
                .iter()
                .filter(|i| i.time > t.range.start && i.time < t.range.end())
                .collect();
            let first_end = marks.first().map_or(t.range.end(), |i| i.time);
            rows.push(CdRow {
                id: t.id,
                track: number,
                index: 1,
                start: t.range.start,
                end: first_end,
            });
            for (k, m) in marks.iter().enumerate() {
                rows.push(CdRow {
                    id: m.id,
                    track: number,
                    index: k as u32 + 2,
                    start: m.time,
                    end: marks.get(k + 1).map_or(t.range.end(), |n| n.time),
                });
            }
        }
        rows
    }

    /// Tools › Lay Out Audio CD from Events: a track for each run of audio
    /// events (events that touch or overlap are one track), in place of
    /// the layout there was. How many tracks it made.
    pub fn lay_out_cd_from_events(&mut self) -> usize {
        let mut spans: Vec<TimeRange> = self
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .flat_map(|t| t.clips.iter().map(|c| c.range()))
            .collect();
        spans.sort_by_key(|r| r.start);
        let mut runs: Vec<TimeRange> = Vec::new();
        for r in spans {
            match runs.last_mut() {
                Some(last) if r.start <= last.end() => {
                    *last = TimeRange::from_bounds(last.start, last.end().max(r.end()));
                }
                _ => runs.push(r),
            }
        }
        self.cd_tracks.clear();
        self.cd_indices.clear();
        runs.truncate(MAX_CD_TRACKS);
        for r in &runs {
            let _ = self.add_cd_track(*r);
        }
        runs.len()
    }

    /// Moves a track to start at `start`, its length and the index
    /// markers inside it with it (refused when it would overlap another).
    pub fn move_cd_track(&mut self, id: MarkerId, start: Tick) -> bool {
        let Some(old) = self.cd_track(id).map(|t| t.range) else {
            return false;
        };
        let delta = start.max(Tick::ZERO) - old.start;
        let inside: Vec<MarkerId> = self
            .cd_indices
            .iter()
            .filter(|i| i.time > old.start && i.time < old.end())
            .map(|i| i.id)
            .collect();
        if !self.set_cd_track(id, |t| {
            t.range = TimeRange::new(old.start + delta, old.duration)
        }) {
            return false;
        }
        for i in &mut self.cd_indices {
            if inside.contains(&i.id) {
                i.time += delta;
            }
        }
        self.sort_cd();
        true
    }

    /// The tracks and indices in time order. An index left outside every
    /// track (its track made shorter) stays — out of the track list, drawn
    /// dimmed — until a track covers it again or it is deleted.
    pub(crate) fn sort_cd(&mut self) {
        self.cd_tracks.sort_by_key(|c| (c.range.start, c.id));
        self.cd_indices.sort_by_key(|i| (i.time, i.id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(n: i64) -> Tick {
        Tick::from_seconds(n)
    }

    #[test]
    fn tracks_never_overlap_and_are_numbered_from_the_first_track_number() {
        let mut p = Project::default();
        let b = p.add_cd_track(TimeRange::new(s(20), s(10))).unwrap();
        let a = p.add_cd_track(TimeRange::new(s(2), s(15))).unwrap();
        assert!(
            p.add_cd_track(TimeRange::new(s(10), s(15))).is_none(),
            "overlaps"
        );
        assert!(
            p.add_cd_track(TimeRange::new(s(40), Tick::ZERO)).is_none(),
            "empty"
        );
        assert_eq!(p.cd_track_number(a), Some(1));
        assert_eq!(p.cd_track_number(b), Some(2));
        p.audio_cd.first_track = 5;
        assert_eq!(p.cd_track_number(b), Some(6));
        assert!(
            !p.set_cd_track(a, |t| t.range = TimeRange::new(s(2), s(19))),
            "would overlap"
        );
        assert!(p.set_cd_track(a, |t| t.name = "Overture".into()));
        assert_eq!(p.cd_track(a).unwrap().name, "Overture");
    }

    #[test]
    fn indices_live_inside_tracks_and_are_counted_from_two() {
        let mut p = Project::default();
        let t = p.add_cd_track(TimeRange::new(s(2), s(20))).unwrap();
        assert!(p.add_cd_index(s(1)).is_none(), "outside every track");
        assert!(
            p.add_cd_index(s(2)).is_none(),
            "the track's start is index 1"
        );
        let i3 = p.add_cd_index(s(15)).unwrap();
        let i2 = p.add_cd_index(s(8)).unwrap();
        assert!(p.add_cd_index(s(8)).is_none(), "one there already");
        let rows = p.cd_rows();
        assert_eq!(
            rows.iter()
                .map(|r| (r.id, r.index, r.start, r.end))
                .collect::<Vec<_>>(),
            vec![
                (t, 1, s(2), s(8)),
                (i2, 2, s(8), s(15)),
                (i3, 3, s(15), s(22)),
            ]
        );
        // A track made shorter keeps the index left outside it, out of the
        // list; moved, a track carries its indices.
        assert!(p.set_cd_track(t, |c| c.range = TimeRange::new(s(2), s(10))));
        assert_eq!(p.cd_indices.len(), 2);
        assert_eq!(p.cd_rows().len(), 2, "the track and the index inside it");
        assert!(p.move_cd_track(t, s(4)));
        assert_eq!(p.cd_rows()[1].start, s(10), "the index went along");
        assert!(p.move_cd_track(t, s(2)));
        assert!(p.move_cd_index(i2, s(5)));
        assert!(!p.move_cd_index(i2, s(30)), "outside");
    }

    #[test]
    fn a_layout_from_events_makes_a_track_of_each_run() {
        let mut p = Project::default();
        let m = p.add_media(crate::MediaInfo::generated(
            crate::Generator::TestTone {
                frequency_hz: 440.0,
                level_db: -6.0,
            },
            &p.settings,
        ));
        let audio = p.add_track(TrackKind::Audio);
        for (start, len) in [(2, 5), (7, 3), (14, 4)] {
            let id = p.insert_media(m, s(start), None, Some(audio)).unwrap()[0];
            p.clip_mut(id).unwrap().duration = s(len);
        }
        assert_eq!(
            p.lay_out_cd_from_events(),
            2,
            "the touching pair is one track"
        );
        assert_eq!(p.cd_tracks[0].range, TimeRange::new(s(2), s(8)));
        assert_eq!(p.cd_tracks[1].range, TimeRange::new(s(14), s(4)));
    }

    #[test]
    fn ripple_edits_carry_the_layout_with_the_markers() {
        let mut p = Project::default();
        let t = p.add_cd_track(TimeRange::new(s(4), s(6))).unwrap();
        let i = p.add_cd_index(s(7)).unwrap();
        p.insert_space(s(1), s(2), &[], true);
        assert_eq!(p.cd_track(t).unwrap().range, TimeRange::new(s(6), s(6)));
        assert_eq!(p.cd_indices[0].time, s(9));
        p.remove_marker(i);
        assert!(p.cd_indices.is_empty());
        p.remove_marker(t);
        assert!(p.cd_tracks.is_empty());
    }
}
