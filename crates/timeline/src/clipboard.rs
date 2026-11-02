//! Copiar, recortar e colar clipes.

use std::collections::HashMap;

use bv_core::Tick;

use crate::model::{Clip, ClipId, GroupId, Project, TrackId, TrackKind};

/// Clipes copiados, com as trilhas de onde vieram. Os tempos são relativos
/// ao início do primeiro clipe.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClipboardData {
    items: Vec<(TrackId, TrackKind, Clip)>,
}

impl ClipboardData {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// O primeiro clipe copiado do tipo dado (fonte para "colar atributos").
    /// Tracks the clips were copied from (each once).
    pub fn tracks(&self) -> Vec<TrackId> {
        let mut v: Vec<TrackId> = self.items.iter().map(|(t, ..)| *t).collect();
        v.sort();
        v.dedup();
        v
    }

    /// Timeline span covered by the copied clips (from the first start to the
    /// last end).
    pub fn span(&self) -> Tick {
        self.items
            .iter()
            .map(|(_, _, c)| c.end())
            .max()
            .unwrap_or(Tick::ZERO)
    }

    pub fn first_of(&self, kind: TrackKind) -> Option<&Clip> {
        self.items
            .iter()
            .find(|(_, k, _)| *k == kind)
            .map(|(_, _, c)| c)
    }
}

impl Project {
    /// Copia os clipes (com seus grupos).
    pub fn copy_clips(&self, ids: &[ClipId]) -> ClipboardData {
        self.copy_events(ids, true)
    }

    /// [`Self::copy_clips`]; with `grouped` off, only the events given are
    /// copied (Options › Preferences › Editing: "Cut, copy, and delete
    /// grouped events").
    pub fn copy_events(&self, ids: &[ClipId], grouped: bool) -> ClipboardData {
        let all = self.edit_reach(ids, grouped);
        let mut items: Vec<(TrackId, TrackKind, Clip)> = all
            .iter()
            .filter_map(|&id| {
                let track = self.clip_track(id)?;
                Some((track.id, track.kind, self.clip(id)?.clone()))
            })
            .collect();
        let origin = items
            .iter()
            .map(|(_, _, c)| c.start)
            .min()
            .unwrap_or(Tick::ZERO);
        for (_, _, c) in &mut items {
            c.start -= origin;
        }
        ClipboardData { items }
    }

    /// Cola em `at`, nas mesmas trilhas (ou na primeira do mesmo tipo, se a
    /// trilha original não existir mais). Devolve os ids dos novos clipes.
    pub fn paste_clips(&mut self, data: &ClipboardData, at: Tick) -> Vec<ClipId> {
        let mut groups: HashMap<GroupId, GroupId> = HashMap::new();
        let mut out = Vec::new();
        for (track, kind, clip) in &data.items {
            if self.media(clip.media).is_none() {
                continue;
            }
            let target = match self.track(*track) {
                Some(t) if t.kind == *kind => *track,
                _ => match self.tracks.iter().find(|t| t.kind == *kind) {
                    Some(t) => t.id,
                    None => self.add_track(*kind),
                },
            };
            let id = ClipId(self.alloc_id());
            let group = clip.group.map(|g| match groups.get(&g) {
                Some(&n) => n,
                None => {
                    let n = GroupId(self.alloc_id());
                    groups.insert(g, n);
                    n
                }
            });
            let new = Clip {
                id,
                start: at.max(Tick::ZERO) + clip.start,
                group,
                ..clip.clone()
            };
            let t = self.track_by_id_mut(target).expect("trilha existe");
            t.clips.push(new);
            t.sort();
            out.push(id);
        }
        out
    }

    /// Copy Here (events dragged with the right mouse button): copies of
    /// events `ids` and their groups `delta` later, the copy of `main` on
    /// track `to` when given (one of its kind). Returns the copies' ids.
    pub fn copy_here(
        &mut self,
        ids: &[ClipId],
        main: ClipId,
        delta: Tick,
        to: Option<TrackId>,
    ) -> Vec<ClipId> {
        let (Some(m), Some(from)) = (
            self.clip(main).cloned(),
            self.clip_track(main).map(|t| t.id),
        ) else {
            return Vec::new();
        };
        let Some(first) = self
            .edit_reach(ids, true)
            .iter()
            .filter_map(|&id| self.clip(id))
            .map(|c| c.start)
            .min()
        else {
            return Vec::new();
        };
        let data = self.copy_events(ids, true);
        let copies = self.paste_clips(&data, first + delta);
        if let Some(to) = to.filter(|&t| t != from) {
            // The copy of `main`: where it went, on its track.
            let copy = copies.iter().copied().find(|&id| {
                self.clip_track(id).is_some_and(|t| t.id == from)
                    && self
                        .clip(id)
                        .is_some_and(|c| c.start == m.start + delta && c.media == m.media)
            });
            if let Some(c) = copy {
                let _ = self.move_clip(c, m.start + delta, Some(to));
            }
        }
        copies
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Generator, MediaInfo};

    #[test]
    fn copiar_e_colar_preserva_grupos_e_posicoes_relativas() {
        let mut p = Project::default();
        let v = p.tracks[0].id;
        let a = p.tracks[1].id;
        let bars = p.add_media(MediaInfo::generated(
            Generator::ColorBars,
            &p.settings.clone(),
        ));
        let tone = p.add_media(MediaInfo::generated(
            Generator::TestTone {
                frequency_hz: 440.0,
                level_db: -12.0,
            },
            &p.settings.clone(),
        ));
        let c1 = p
            .insert_media(bars, Tick::from_seconds(2), Some(v), None)
            .unwrap()[0];
        let c2 = p
            .insert_media(tone, Tick::from_seconds(3), None, Some(a))
            .unwrap()[0];
        let data = p.copy_clips(&[c1, c2]);
        assert_eq!(data.len(), 2);
        let new = p.paste_clips(&data, Tick::from_seconds(10));
        assert_eq!(new.len(), 2);
        assert_eq!(p.clip(new[0]).unwrap().start, Tick::from_seconds(10));
        assert_eq!(p.clip(new[1]).unwrap().start, Tick::from_seconds(11));
        assert_eq!(p.clip_track(new[1]).unwrap().id, a);
        p.check_invariants().unwrap();
    }

    #[test]
    fn copy_here_leaves_the_event_and_places_a_copy() {
        let mut p = Project::default();
        let v = p.tracks[0].id;
        let bars = p.add_media(MediaInfo::generated(
            Generator::ColorBars,
            &p.settings.clone(),
        ));
        let c = p
            .insert_media(bars, Tick::from_seconds(2), Some(v), None)
            .unwrap()[0];
        let copies = p.copy_here(&[c], c, Tick::from_seconds(5), None);
        assert_eq!(copies.len(), 1);
        assert_eq!(p.clip(c).unwrap().start, Tick::from_seconds(2));
        assert_eq!(p.clip(copies[0]).unwrap().start, Tick::from_seconds(7));
        // Onto another track of its kind.
        let other = p.add_track(TrackKind::Video);
        let copies = p.copy_here(&[c], c, Tick::from_seconds(1), Some(other));
        assert_eq!(p.clip_track(copies[0]).unwrap().id, other);
        assert_eq!(p.clip(copies[0]).unwrap().start, Tick::from_seconds(3));
        p.check_invariants().unwrap();
    }
}
