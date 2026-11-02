//! Takes: an event's other media, as VEGAS keeps them — alternate versions
//! (voiceovers, camera angles) of which one, the active take, plays. The
//! active take is the event's own `media` and `source_in`; [`Clip::takes`]
//! lists all of them in order when there is more than one, each placed by
//! its offset from the active take's `source_in`, so trims and splits move
//! them all together and switching keeps each where it was.

use serde::{Deserialize, Serialize};

use crate::edit::EditError;
use crate::model::{Clip, ClipId, MediaId, Project};
use bv_core::Tick;

type Result<T> = std::result::Result<T, EditError>;

/// One of an event's takes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Take {
    pub media: MediaId,
    /// Where it starts in its media, against the active take's
    /// `source_in` (media time).
    #[serde(default)]
    pub offset: Tick,
    /// Its name (`None`: its media's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// An empty take — a multicamera event's "(no camera)", where that
    /// camera did not record —: it never plays.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub none: bool,
}

impl Clip {
    /// How many takes the event has (1 without others).
    pub fn take_count(&self) -> usize {
        self.takes.len().max(1)
    }

    /// The active take's index among [`take_list`](Clip::take_list).
    pub fn active_take(&self) -> usize {
        if self.takes.is_empty() {
            0
        } else {
            self.active_take.min(self.takes.len() - 1)
        }
    }

    /// The event's takes in order, the active one included (its media as
    /// the event has it now).
    pub fn take_list(&self) -> Vec<Take> {
        if self.takes.is_empty() {
            return vec![Take {
                media: self.media,
                offset: Tick::ZERO,
                name: None,
                none: false,
            }];
        }
        let mut v = self.takes.clone();
        let a = self.active_take();
        v[a].media = self.media;
        v[a].offset = Tick::ZERO;
        v
    }

    /// Makes take `k` the active one: the event plays its media from where
    /// it is placed, and the others keep their places.
    fn activate(&mut self, k: usize) {
        if self.takes.is_empty() || k >= self.takes.len() || self.takes[k].none {
            return;
        }
        let a = self.active_take();
        self.takes[a].media = self.media;
        self.takes[a].offset = Tick::ZERO;
        let shift = self.takes[k].offset;
        for t in &mut self.takes {
            t.offset -= shift;
        }
        self.media = self.takes[k].media;
        self.source_in += shift;
        self.active_take = k;
    }

    /// The active take moved by `delta` in its media alone (a slip that
    /// leaves the other takes where they were).
    pub(crate) fn slip_active_take_only(&mut self, delta: Tick) {
        let a = self.active_take();
        for (i, t) in self.takes.iter_mut().enumerate() {
            if i != a {
                t.offset -= delta;
            }
        }
    }

    /// The event's media in all its takes.
    pub fn take_media(&self) -> impl Iterator<Item = MediaId> + '_ {
        std::iter::once(self.media).chain(self.takes.iter().map(|t| t.media))
    }
}

impl Project {
    fn clip_or_err(&mut self, id: ClipId) -> Result<&mut Clip> {
        self.clip_mut(id).ok_or(EditError::UnknownClip(id))
    }

    /// Adds `media` to event `id` as its active take, from the media's
    /// start (VEGAS's Add as Takes); the take it had stays one of them.
    pub fn add_take(&mut self, id: ClipId, media: MediaId) -> Result<()> {
        self.add_take_at(id, media, Tick::ZERO)
    }

    /// As `add_take`, the event's start showing media time `at` of it (a
    /// recording's pass through the event).
    pub fn add_take_at(&mut self, id: ClipId, media: MediaId, at: Tick) -> Result<()> {
        let kind = {
            let (t, _) = self.find_clip(id).ok_or(EditError::UnknownClip(id))?;
            self.tracks[t].kind
        };
        let m = self.media(media).ok_or(EditError::UnknownMedia(media))?;
        if !m.has(kind) {
            return Err(EditError::WrongTrackKind);
        }
        let clip = self.clip_or_err(id)?;
        if clip.takes.is_empty() {
            clip.takes.push(Take {
                media: clip.media,
                offset: Tick::ZERO,
                name: None,
                none: false,
            });
            clip.active_take = 0;
        }
        // Its media's `at` where the event begins.
        let offset = at - clip.source_in;
        clip.takes.push(Take {
            media,
            offset,
            name: None,
            none: false,
        });
        let k = clip.takes.len() - 1;
        clip.activate(k);
        Ok(())
    }

    /// Makes take `k` of event `id` the active one.
    pub fn set_active_take(&mut self, id: ClipId, k: usize) -> Result<()> {
        let clip = self.clip_or_err(id)?;
        if k >= clip.take_count() || clip.take_list()[k].none {
            return Err(EditError::NoTake);
        }
        clip.activate(k);
        Ok(())
    }

    /// The next take of event `id` (back to the first after the last), or
    /// the previous one (`back`).
    pub fn step_take(&mut self, id: ClipId, back: bool) -> Result<()> {
        let clip = self.clip_or_err(id)?;
        let n = clip.take_count();
        if n < 2 {
            return Err(EditError::NoTake);
        }
        // The next one that plays (empty takes are passed over).
        let list = clip.take_list();
        let a = clip.active_take();
        let k = (1..n)
            .map(|i| if back { (a + n - i) % n } else { (a + i) % n })
            .find(|&k| !list[k].none)
            .ok_or(EditError::NoTake)?;
        clip.activate(k);
        Ok(())
    }

    /// Deletes take `k` of event `id` (never its only one): when it was the
    /// active one, the next becomes active.
    pub fn delete_take(&mut self, id: ClipId, k: usize) -> Result<()> {
        let clip = self.clip_or_err(id)?;
        let n = clip.take_count();
        if n < 2 || k >= n {
            return Err(EditError::NoTake);
        }
        let a = clip.active_take();
        if k == a {
            // The next take that plays takes its place.
            let list = clip.take_list();
            let next = (1..n)
                .map(|i| (a + i) % n)
                .find(|&j| !list[j].none)
                .ok_or(EditError::NoTake)?;
            clip.activate(next);
        }
        let a = clip.active_take();
        clip.takes.remove(k);
        clip.active_take = if k < a { a - 1 } else { a };
        if clip.takes.len() == 1 {
            clip.takes.clear();
            clip.active_take = 0;
        }
        Ok(())
    }

    /// Names take `k` of event `id` (empty: its media's name again).
    pub fn rename_take(&mut self, id: ClipId, k: usize, name: &str) -> Result<()> {
        let clip = self.clip_or_err(id)?;
        if k >= clip.take_count() {
            return Err(EditError::NoTake);
        }
        if clip.takes.is_empty() {
            clip.takes.push(Take {
                media: clip.media,
                offset: Tick::ZERO,
                name: None,
                none: false,
            });
            clip.active_take = 0;
        }
        let name = name.trim();
        clip.takes[k].name = (!name.is_empty()).then(|| name.to_string());
        Ok(())
    }

    /// Slip (see [`Project::slip_clip`]); without Options › Slip All Takes
    /// (`slip_all_takes`), the event's other takes stay where they were.
    pub fn slip_takes(&mut self, id: ClipId, delta: Tick) -> Result<Tick> {
        let all_takes = self.slip_all_takes;
        let before: Vec<(ClipId, Tick)> = self
            .group_members(id)
            .into_iter()
            .filter_map(|m| self.clip(m).map(|c| (m, c.source_in)))
            .collect();
        let applied = self.slip_clip(id, delta)?;
        if !all_takes {
            for (m, was) in before {
                if let Some(c) = self.clip_mut(m) {
                    let moved = c.source_in - was;
                    c.slip_active_take_only(moved);
                }
            }
        }
        Ok(applied)
    }

    /// Take `k` of event `id`'s name: its own, or its media's.
    pub fn take_name(&self, id: ClipId, k: usize) -> Option<String> {
        let clip = self.clip(id)?;
        let take = clip.take_list().into_iter().nth(k)?;
        if take.none {
            return None;
        }
        Some(take.name.unwrap_or_else(|| {
            self.media(take.media)
                .map_or_else(String::new, |m| m.name.clone())
        }))
    }

    /// Drops the takes that play `media` (removed from the project): an
    /// event whose active take it was switches to another (the events it
    /// was the only take of are left to the caller).
    pub(crate) fn drop_takes_of(&mut self, media: MediaId) {
        for t in &mut self.tracks {
            for c in &mut t.clips {
                if c.takes.is_empty() {
                    continue;
                }
                if c.media == media
                    && let Some(k) = c.take_list().iter().position(|tk| tk.media != media)
                {
                    c.activate(k);
                }
                let list = c.take_list();
                let a = c.active_take();
                let keep: Vec<usize> = (0..list.len())
                    .filter(|&i| list[i].media != media || i == a)
                    .collect();
                let new_active = keep.iter().position(|&i| i == a).unwrap_or(0);
                c.takes = keep.iter().map(|&i| list[i].clone()).collect();
                c.active_take = new_active;
                if c.takes.len() <= 1 {
                    c.takes.clear();
                    c.active_take = 0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Generator, MediaInfo, ProjectSettings};

    /// A project with an event of solid red (5 s at 1 s on video track 0)
    /// and two other media, green and blue.
    fn setup() -> (Project, ClipId, MediaId, MediaId) {
        let mut p = Project::new(ProjectSettings::default());
        let solid = |r: f32, g: f32, b: f32| {
            MediaInfo::generated(
                Generator::SolidColor {
                    rgba: [r, g, b, 1.0],
                },
                &ProjectSettings::default(),
            )
        };
        let red = p.add_media(solid(1.0, 0.0, 0.0));
        let green = p.add_media(solid(0.0, 1.0, 0.0));
        let blue = p.add_media(solid(0.0, 0.0, 1.0));
        let v = p.tracks[0].id;
        let clip = p
            .insert_media(red, Tick::from_seconds(1), Some(v), None)
            .unwrap()[0];
        (p, clip, green, blue)
    }

    #[test]
    fn takes_are_added_chosen_and_stepped_through() {
        let (mut p, clip, green, blue) = setup();
        let red = p.clip(clip).unwrap().media;
        assert_eq!(p.clip(clip).unwrap().take_count(), 1);
        p.add_take(clip, green).unwrap();
        p.add_take(clip, blue).unwrap();
        let c = p.clip(clip).unwrap();
        // The one added last plays; the others stay, in order.
        assert_eq!((c.take_count(), c.active_take(), c.media), (3, 2, blue));
        let order: Vec<MediaId> = c.take_list().iter().map(|t| t.media).collect();
        assert_eq!(order, vec![red, green, blue]);
        p.step_take(clip, false).unwrap();
        assert_eq!(p.clip(clip).unwrap().media, red);
        p.step_take(clip, true).unwrap();
        assert_eq!(p.clip(clip).unwrap().media, blue);
        p.set_active_take(clip, 1).unwrap();
        assert_eq!(p.clip(clip).unwrap().media, green);
        p.check_invariants().unwrap();
    }

    #[test]
    fn takes_keep_their_places_through_trims_slips_and_switches() {
        let (mut p, clip, green, _) = setup();
        // The red take starts 2 s into its media.
        p.clip_mut(clip).unwrap().source_in = Tick::from_seconds(2);
        p.add_take(clip, green).unwrap();
        // Green from its start.
        assert_eq!(p.clip(clip).unwrap().source_in, Tick::ZERO);
        // A trim of the event's start moves both.
        p.clip_mut(clip).unwrap().source_in = Tick::from_seconds(1);
        p.step_take(clip, false).unwrap();
        assert_eq!(p.clip(clip).unwrap().source_in, Tick::from_seconds(3));
        // A slip of the active take alone leaves the other where it was.
        {
            let c = p.clip_mut(clip).unwrap();
            c.source_in += Tick::from_seconds(1);
            c.slip_active_take_only(Tick::from_seconds(1));
        }
        p.step_take(clip, false).unwrap();
        assert_eq!(p.clip(clip).unwrap().source_in, Tick::from_seconds(1));
    }

    #[test]
    fn deleting_and_renaming_takes() {
        let (mut p, clip, green, blue) = setup();
        p.add_take(clip, green).unwrap();
        p.add_take(clip, blue).unwrap();
        p.rename_take(clip, 1, "Second try").unwrap();
        assert_eq!(p.take_name(clip, 1).as_deref(), Some("Second try"));
        // The active (blue) deleted: the next, back at the first, plays.
        p.delete_take(clip, 2).unwrap();
        let c = p.clip(clip).unwrap();
        assert_eq!((c.take_count(), c.active_take()), (2, 0));
        p.delete_take(clip, 0).unwrap();
        let c = p.clip(clip).unwrap();
        assert_eq!((c.take_count(), c.media), (1, green));
        assert!(p.delete_take(clip, 0).is_err());
        p.check_invariants().unwrap();
    }

    #[test]
    fn removing_a_media_drops_its_takes() {
        let (mut p, clip, green, blue) = setup();
        p.add_take(clip, green).unwrap();
        p.add_take(clip, blue).unwrap();
        p.remove_media(blue).unwrap();
        let c = p.clip(clip).unwrap();
        assert_eq!(c.take_count(), 2);
        assert_ne!(c.media, blue);
        p.check_invariants().unwrap();
    }
}
