//! Subclips, as VEGAS's Help describes them: a part of a media file kept
//! as an item of Project Media — made from the Trimmer's selection or from
//! the part an event plays (Create Subclip: a name, by default the file's
//! with a number, and Reverse; an event reversed makes one too) — whose
//! events play that part of its parent media, backward when reversed.
//! BananaV keeps them apart from the
//! media (`Project::subclips`) and makes their events of the parent media
//! itself, so everything that reads media reads the parent's file.

use bv_core::{Tick, TimeRange};
use serde::{Deserialize, Serialize};

use crate::model::{ClipId, MediaId, MediaInfo, Project};

/// A subclip: an item of Project Media standing for part of a media.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Subclip {
    /// Its own id, from the project's counter (never a media's).
    pub id: MediaId,
    pub parent: MediaId,
    /// The part of the parent, in its time.
    pub range: TimeRange,
    /// Its events play backward.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reverse: bool,
    pub name: String,
}

impl Project {
    /// A subclip of `parent` over `range` (within the media, at least a
    /// frame long); its id.
    pub fn add_subclip(
        &mut self,
        parent: MediaId,
        range: TimeRange,
        reverse: bool,
        name: &str,
    ) -> Option<MediaId> {
        let m = self.media(parent)?;
        let end = if m.still {
            range.end()
        } else {
            range.end().min(m.duration)
        };
        let range = TimeRange::from_bounds(range.start.max(Tick::ZERO), end);
        if range.duration < self.min_clip_duration() {
            return None;
        }
        let id = MediaId(self.alloc_id());
        self.subclips.push(Subclip {
            id,
            parent,
            range,
            reverse,
            name: name.trim().to_string(),
        });
        Some(id)
    }

    pub fn subclip(&self, id: MediaId) -> Option<&Subclip> {
        self.subclips.iter().find(|s| s.id == id)
    }

    /// A subclip as Project Media shows it: its parent's information under
    /// its own id and name, as long as its part.
    pub fn subclip_info(&self, id: MediaId) -> Option<MediaInfo> {
        let s = self.subclip(id)?;
        let mut m = self.media(s.parent)?.clone();
        m.id = s.id;
        m.name = s.name.clone();
        m.duration = s.range.duration;
        m.markers.clear();
        Some(m)
    }

    /// The name Create Subclip offers: the media's with the next number.
    pub fn next_subclip_name(&self, parent: MediaId) -> String {
        let base = self.media(parent).map_or_else(String::new, |m| {
            std::path::Path::new(&m.name)
                .file_stem()
                .map_or_else(|| m.name.clone(), |s| s.to_string_lossy().to_string())
        });
        let n = self.subclips.iter().filter(|s| s.parent == parent).count() + 1;
        format!("{base}_{n}")
    }

    /// Reverse's subclip — VEGAS's Help: the event reversed, "a subclip
    /// is created in the Project Media list for the reversed event" —:
    /// the part event `id` plays, backward, named as Create Subclip names
    /// them; the one there already when it is. None while it plays
    /// forward.
    pub fn add_reversed_subclip(&mut self, id: ClipId) -> Option<MediaId> {
        let c = self.clip(id)?;
        c.reverse?;
        let parent = c.media;
        let start = c.source_in.max(Tick::ZERO);
        let end = c.source_in + c.to_source(c.duration);
        let m = self.media(parent)?;
        let end = if m.still { end } else { end.min(m.duration) };
        let range = TimeRange::from_bounds(start, end.max(start));
        if let Some(s) = self
            .subclips
            .iter()
            .find(|s| s.parent == parent && s.reverse && s.range == range)
        {
            return Some(s.id);
        }
        let name = self.next_subclip_name(parent);
        self.add_subclip(parent, range, true, &name)
    }

    /// Removes a subclip (its events, of the parent media, stay).
    pub fn remove_subclip(&mut self, id: MediaId) {
        self.subclips.retain(|s| s.id != id);
        for b in &mut self.bins {
            b.media.retain(|&m| m != id);
        }
        self.forget_tags(id);
        self.forget_in_storyboards(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subclips_stand_for_part_of_their_parent() {
        let mut p = Project::default();
        let m = p.add_media(crate::MediaInfo::generated(
            crate::Generator::TestTone {
                frequency_hz: 440.0,
                level_db: -6.0,
            },
            &p.settings,
        ));
        let s = Tick::from_seconds;
        let name = p.next_subclip_name(m);
        let sub = p
            .add_subclip(m, TimeRange::new(s(1), s(2)), true, &name)
            .unwrap();
        assert!(p.media(sub).is_none(), "not a media of its own");
        let info = p.subclip_info(sub).unwrap();
        assert_eq!((info.id, info.duration), (sub, s(2)));
        assert!(p.next_subclip_name(m).ends_with("_2"));
        assert!(
            p.add_subclip(m, TimeRange::new(s(1), Tick::ZERO), false, "x")
                .is_none()
        );
        p.remove_media(m).unwrap();
        assert!(p.subclip(sub).is_none(), "gone with its parent");
    }
}
