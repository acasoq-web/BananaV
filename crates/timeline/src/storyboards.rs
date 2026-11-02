//! Storyboards, as VEGAS's Help describes them ("Using Storyboards"):
//! Project Media's Storyboard Bins. The Main Timeline storyboard is the
//! timeline's sequence — the events of its first track that has any, in
//! order — and what is done to it is done to the timeline; the others are
//! sequences of their own (cells of media, or placeholders noting what is
//! to come), to try orders in, preview and put on the timeline.

use bv_core::{Tick, TimeRange};
use serde::{Deserialize, Serialize};

use crate::bins::BinId;
use crate::edit::EditError;
use crate::model::{ClipId, MediaId, NewEvents, Project, TrackId, TrackKind};

/// A cell of a storyboard.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoryCell {
    /// Its media; `None`: a placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaId>,
    /// The part of the media it holds; `None`: all of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<TimeRange>,
    /// A placeholder's name (a note of what is to come there).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

impl StoryCell {
    pub fn of(media: MediaId, range: Option<TimeRange>) -> StoryCell {
        StoryCell {
            media: Some(media),
            range,
            name: String::new(),
        }
    }

    /// A placeholder named `name`.
    pub fn placeholder(name: &str) -> StoryCell {
        StoryCell {
            media: None,
            range: None,
            name: name.to_string(),
        }
    }
}

/// A storyboard of Storyboard Bins (not the Main Timeline's).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Storyboard {
    pub id: BinId,
    pub name: String,
    pub cells: Vec<StoryCell>,
}

/// A cell of the Main Timeline storyboard: an event of the timeline's
/// sequence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimelineCell {
    pub clip: ClipId,
    pub media: MediaId,
    /// Where the event starts on the timeline.
    pub start: Tick,
    pub length: Tick,
    /// The media time the event starts at (its picture).
    pub source_in: Tick,
}

/// The name placeholders get, the interface's language aside.
pub const PLACEHOLDER: &str = "Placeholder";

impl Project {
    /// The track the Main Timeline storyboard follows: the first video
    /// track with events, else the first audio one.
    pub fn storyboard_track(&self) -> Option<TrackId> {
        [TrackKind::Video, TrackKind::Audio]
            .into_iter()
            .find_map(|k| {
                self.tracks
                    .iter()
                    .find(|t| t.kind == k && !t.adjustment && !t.clips.is_empty())
            })
            .map(|t| t.id)
    }

    /// The Main Timeline storyboard's cells: the events of
    /// [`Self::storyboard_track`], by start.
    pub fn timeline_cells(&self) -> Vec<TimelineCell> {
        let Some(t) = self.storyboard_track().and_then(|id| self.track(id)) else {
            return Vec::new();
        };
        let mut cells: Vec<TimelineCell> = t
            .clips
            .iter()
            .map(|c| TimelineCell {
                clip: c.id,
                media: c.media,
                start: c.start,
                length: c.duration,
                source_in: c.source_in,
            })
            .collect();
        cells.sort_by_key(|c| (c.start, c.clip));
        cells
    }

    /// The Main Timeline storyboard's rearranging: event `clip` (with its
    /// group) goes before event `before` (`None`: after the last), as
    /// VEGAS's Help says — the events after where it was move left to
    /// close its gap, those from where it goes move right to make room.
    pub fn move_in_sequence(
        &mut self,
        clip: ClipId,
        before: Option<ClipId>,
    ) -> Result<(), EditError> {
        let cells = self.timeline_cells();
        let from = cells
            .iter()
            .position(|c| c.clip == clip)
            .ok_or(EditError::UnknownClip(clip))?;
        if before == Some(clip) {
            return Ok(());
        }
        let moved = cells[from];
        // The starts with the event taken out (the rest closing up), then
        // with it put in its place (the rest making room).
        let mut order: Vec<TimelineCell> = cells.clone();
        order.remove(from);
        let mut starts: Vec<Tick> = order
            .iter()
            .map(|c| {
                if c.start > moved.start {
                    (c.start - moved.length).max(Tick::ZERO)
                } else {
                    c.start
                }
            })
            .collect();
        let at = match before {
            Some(b) => order
                .iter()
                .position(|c| c.clip == b)
                .ok_or(EditError::UnknownClip(b))?,
            None => order.len(),
        };
        let new_start = match starts.get(at) {
            Some(&s) => s,
            // At the end: after the last one.
            None => order
                .iter()
                .zip(&starts)
                .map(|(c, &s)| s + c.length)
                .max()
                .unwrap_or(moved.start),
        };
        for s in starts.iter_mut().skip(at) {
            *s += moved.length;
        }
        // Each event, with its group, to its new start: those going right
        // first from the last, so none crosses another on the way.
        let mut moves: Vec<(ClipId, Tick)> = order.iter().map(|c| c.clip).zip(starts).collect();
        moves.push((moved.clip, new_start));
        moves.sort_by_key(|&(id, s)| {
            let old = cells.iter().find(|c| c.clip == id).map_or(s, |c| c.start);
            std::cmp::Reverse((s - old).0)
        });
        for (id, s) in moves {
            if self.clip(id).is_some_and(|c| c.start != s) {
                self.move_clip(id, s, None)?;
            }
        }
        Ok(())
    }

    /// A new, empty storyboard named `name`.
    pub fn add_storyboard(&mut self, name: &str) -> BinId {
        let id = BinId(self.alloc_id());
        self.storyboards.push(Storyboard {
            id,
            name: name.to_string(),
            cells: Vec::new(),
        });
        id
    }

    pub fn storyboard(&self, id: BinId) -> Option<&Storyboard> {
        self.storyboards.iter().find(|s| s.id == id)
    }

    pub fn storyboard_mut(&mut self, id: BinId) -> Option<&mut Storyboard> {
        self.storyboards.iter_mut().find(|s| s.id == id)
    }

    pub fn remove_storyboard(&mut self, id: BinId) {
        self.storyboards.retain(|s| s.id != id);
    }

    /// `cell` let go on cell `at` of storyboard `id`: past the last, it goes
    /// there and the empty cells before it get placeholders; on a cell, it
    /// takes its place (`insert`: goes before it).
    pub fn storyboard_drop(&mut self, id: BinId, at: usize, cell: StoryCell, insert: bool) {
        let Some(s) = self.storyboard_mut(id) else {
            return;
        };
        if at >= s.cells.len() {
            while s.cells.len() < at {
                s.cells.push(StoryCell::placeholder(PLACEHOLDER));
            }
            s.cells.push(cell);
        } else if insert {
            s.cells.insert(at, cell);
        } else {
            s.cells[at] = cell;
        }
    }

    /// Moves cell `from` of storyboard `id` before cell `to` (its place in
    /// the cells as they are; past the last: to the end).
    pub fn storyboard_move(&mut self, id: BinId, from: usize, to: usize) {
        let Some(s) = self.storyboard_mut(id) else {
            return;
        };
        if from >= s.cells.len() || to == from {
            return;
        }
        let cell = s.cells.remove(from);
        let to = if to > from { to - 1 } else { to };
        s.cells.insert(to.min(s.cells.len()), cell);
    }

    /// Takes cell `index` out of storyboard `id`.
    pub fn storyboard_remove(&mut self, id: BinId, index: usize) {
        if let Some(s) = self.storyboard_mut(id)
            && index < s.cells.len()
        {
            s.cells.remove(index);
        }
    }

    /// How long a cell plays: its part, or as long as an event of its
    /// media is when added (`new`: the editing preferences), a
    /// placeholder five seconds.
    pub fn cell_length(&self, cell: &StoryCell, new: &NewEvents) -> Tick {
        match (cell.range, cell.media) {
            (Some(r), _) => r.duration,
            (None, Some(m)) => self
                .media(m)
                .cloned()
                .or_else(|| self.subclip_info(m))
                .map_or(Tick::ZERO, |i| new.length(&i)),
            (None, None) => Tick::from_seconds(5),
        }
    }

    /// A media leaving the project leaves the storyboards too.
    pub(crate) fn forget_in_storyboards(&mut self, media: MediaId) {
        for s in &mut self.storyboards {
            s.cells.retain(|c| c.media != Some(media));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Generator, MediaInfo, ProjectSettings};

    /// Three events end to end on the first video track: 2, 3 and 1 s.
    fn sequence() -> (Project, [ClipId; 3]) {
        let mut p = Project::new(ProjectSettings::default());
        let track = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .map(|t| t.id);
        let s = Tick::from_seconds;
        let mut ids = Vec::new();
        for (at, len) in [(0, 2), (2, 3), (5, 1)] {
            let m = p.add_media(MediaInfo::generated(Generator::ColorBars, &p.settings));
            let clip = p.insert_media(m, s(at), track, None).unwrap()[0];
            p.set_clip_duration(clip, s(len)).unwrap();
            ids.push(clip);
        }
        (p, [ids[0], ids[1], ids[2]])
    }

    fn starts(p: &Project) -> Vec<(ClipId, i64)> {
        p.timeline_cells()
            .iter()
            .map(|c| (c.clip, (c.start.as_seconds_f64()).round() as i64))
            .collect()
    }

    #[test]
    fn the_main_timeline_storyboard_is_the_timeline_s_sequence() {
        let (mut p, [a, b, c]) = sequence();
        assert_eq!(starts(&p), [(a, 0), (b, 2), (c, 5)]);
        // C before A: A and B make room, C's place closes up.
        p.move_in_sequence(c, Some(a)).unwrap();
        assert_eq!(starts(&p), [(c, 0), (a, 1), (b, 3)]);
        // A to the end: B closes up, A after it.
        p.move_in_sequence(a, None).unwrap();
        assert_eq!(starts(&p), [(c, 0), (b, 1), (a, 4)]);
        assert!(p.move_in_sequence(ClipId(9999), None).is_err());
    }

    #[test]
    fn storyboards_hold_cells_with_placeholders_between() {
        let (mut p, _) = sequence();
        let m = p.media[0].id;
        let n = p.media[1].id;
        let id = p.add_storyboard("Try");
        // Let go on the fourth (empty) cell: placeholders before it.
        p.storyboard_drop(id, 3, StoryCell::of(m, None), false);
        let cells = &p.storyboard(id).unwrap().cells;
        assert_eq!(cells.len(), 4);
        assert!(
            cells[..3]
                .iter()
                .all(|c| c.media.is_none() && c.name == PLACEHOLDER)
        );
        // On a cell it takes its place; on its left edge it goes before.
        p.storyboard_drop(id, 1, StoryCell::of(n, None), false);
        p.storyboard_drop(id, 0, StoryCell::of(n, None), true);
        let media: Vec<Option<MediaId>> = p
            .storyboard(id)
            .unwrap()
            .cells
            .iter()
            .map(|c| c.media)
            .collect();
        assert_eq!(media, [Some(n), None, Some(n), None, Some(m)]);
        // Moved: the last before the first.
        p.storyboard_move(id, 4, 0);
        assert_eq!(p.storyboard(id).unwrap().cells[0].media, Some(m));
        assert_eq!(
            p.cell_length(&StoryCell::placeholder("x"), &NewEvents::default()),
            Tick::from_seconds(5)
        );
        // A media leaving the project leaves the storyboard.
        p.remove_media(n).unwrap();
        assert!(
            p.storyboard(id)
                .unwrap()
                .cells
                .iter()
                .all(|c| c.media != Some(n))
        );
        p.remove_storyboard(id);
        assert!(p.storyboards.is_empty());
    }
}
