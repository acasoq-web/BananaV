//! Track groups, as VEGAS's Help describes them ("Grouping tracks"):
//! adjacent tracks gathered under a header that collapses them out of
//! sight, renames them together and mutes or solos them together — the
//! group's Mute overriding its tracks' solo, its Solo their mute. A
//! track's group is `Track::group`; the groups, `Project::track_groups`.
//! A group's tracks are always next to one another: grouping takes
//! adjacent tracks (with the compositing parents and children of the video
//! ones), a track moved between two tracks of a group joins it, and
//! [`Project::normalize_groups`] keeps that true after every change of the
//! track list.

use serde::{Deserialize, Serialize};

use crate::edit::EditError;
use crate::model::{Project, Track, TrackId, TrackKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TrackGroupId(pub u32);

/// A track group: its name (its scribble strip), collapsed or not, muted
/// or soloed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackGroup {
    pub id: TrackGroupId,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub muted: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub solo: bool,
}

impl Project {
    pub fn track_group(&self, id: TrackGroupId) -> Option<&TrackGroup> {
        self.track_groups.iter().find(|g| g.id == id)
    }

    pub fn track_group_mut(&mut self, id: TrackGroupId) -> Option<&mut TrackGroup> {
        self.track_groups.iter_mut().find(|g| g.id == id)
    }

    /// The group track `t` is in.
    pub fn group_of(&self, t: &Track) -> Option<&TrackGroup> {
        t.group.and_then(|g| self.track_group(g))
    }

    /// The tracks of group `id`, top first.
    pub fn track_group_members(&self, id: TrackGroupId) -> Vec<TrackId> {
        self.tracks
            .iter()
            .filter(|t| t.group == Some(id))
            .map(|t| t.id)
            .collect()
    }

    /// Whether track `t` is muted: its group's Mute mutes it, its group's
    /// Solo overrides its own mute.
    pub fn track_muted(&self, t: &Track) -> bool {
        match self.group_of(t) {
            Some(g) if g.muted => true,
            Some(g) if g.solo => false,
            _ => t.muted,
        }
    }

    /// Whether track `t` is soloed: its group's Solo solos it, its group's
    /// Mute overrides its own solo.
    pub fn track_soloed(&self, t: &Track) -> bool {
        match self.group_of(t) {
            Some(g) if g.muted => false,
            Some(g) if g.solo => true,
            _ => t.solo,
        }
    }

    /// Track › Track Group › Group Selected Tracks: `ids` — with the
    /// compositing parents and children of the video ones — in a new
    /// group, named after its first track; they leave the groups they were
    /// in. They must be at least two and next to one another.
    pub fn group_tracks(&mut self, ids: &[TrackId]) -> Result<TrackGroupId, EditError> {
        let mut rows: Vec<usize> = Vec::new();
        for &id in ids {
            self.track_index(id).ok_or(EditError::UnknownTrack(id))?;
            for t in self.compositing_relatives(id) {
                if let Some(i) = self.track_index(t) {
                    rows.push(i);
                }
            }
        }
        rows.sort_unstable();
        rows.dedup();
        if rows.len() < 2 || rows.windows(2).any(|w| w[1] != w[0] + 1) {
            return Err(EditError::TracksNotAdjacent);
        }
        let id = TrackGroupId(
            self.track_groups
                .iter()
                .map(|g| g.id.0 + 1)
                .max()
                .unwrap_or(1),
        );
        self.track_groups.push(TrackGroup {
            id,
            name: String::new(),
            collapsed: false,
            muted: false,
            solo: false,
        });
        for &i in &rows {
            self.tracks[i].group = Some(id);
        }
        self.normalize_groups();
        Ok(id)
    }

    /// Track Group › Ungroup Selected Tracks: `ids` leave their groups; one
    /// from a group's middle moves just below it, so that the group stays
    /// together. Groups left without tracks go.
    pub fn ungroup_tracks(&mut self, ids: &[TrackId]) {
        // From the bottom up, so the indices above stay right.
        let mut rows: Vec<usize> = ids.iter().filter_map(|&id| self.track_index(id)).collect();
        rows.sort_unstable();
        rows.dedup();
        for &i in rows.iter().rev() {
            let Some(g) = self.tracks[i].group else {
                continue;
            };
            self.tracks[i].group = None;
            let last = self
                .tracks
                .iter()
                .rposition(|t| t.group == Some(g))
                .filter(|&l| l > i);
            if let Some(last) = last {
                let t = self.tracks.remove(i);
                self.tracks.insert(last, t);
            }
        }
        self.normalize_compositing();
        self.normalize_groups();
    }

    /// Ungroups every track of group `id` (Ungroup Selected Tracks on its
    /// header).
    pub fn clear_track_group(&mut self, id: TrackGroupId) {
        for t in &mut self.tracks {
            if t.group == Some(id) {
                t.group = None;
            }
        }
        self.normalize_groups();
    }

    /// Track `id` joins group `group` (or leaves its group, `None`), when
    /// that keeps the group's tracks together.
    pub fn set_track_group(&mut self, id: TrackId, group: Option<TrackGroupId>) {
        if let Some(t) = self.track_by_id_mut(id) {
            t.group = group;
        }
        self.normalize_groups();
    }

    /// Moves group `id`'s tracks together so that its first lands at
    /// position `index` of the tracks as they are now (0 = top), like
    /// dragging its header.
    pub fn move_track_group_to(&mut self, id: TrackGroupId, index: usize) {
        let rows: Vec<usize> = (0..self.tracks.len())
            .filter(|&i| self.tracks[i].group == Some(id))
            .collect();
        let (Some(&first), Some(&last)) = (rows.first(), rows.last()) else {
            return;
        };
        if index >= first && index <= last + 1 {
            return;
        }
        let block: Vec<Track> = self.tracks.drain(first..=last).collect();
        let at = if index > last {
            index - block.len()
        } else {
            index
        }
        .min(self.tracks.len());
        for (k, t) in block.into_iter().enumerate() {
            self.tracks.insert(at + k, t);
        }
        self.normalize_compositing();
        self.normalize_groups();
    }

    /// The tracks that must stay with video track `id` in a group: its
    /// compositing parent's whole family (itself alone when it has none,
    /// or for audio).
    fn compositing_relatives(&self, id: TrackId) -> Vec<TrackId> {
        let Some(i) = self.track_index(id) else {
            return Vec::new();
        };
        if self.tracks[i].kind != TrackKind::Video {
            return vec![id];
        }
        // Up to the parent at the top of its family.
        let mut root = i;
        while self.tracks[root].composite_depth > 0 {
            match (0..root)
                .rev()
                .find(|&j| self.tracks[j].kind == TrackKind::Video)
            {
                Some(j) => root = j,
                None => break,
            }
        }
        self.compositing_family(self.tracks[root].id)
    }

    /// Keeps the groups together: a track between two tracks of one group
    /// joins it, a group's tracks apart from its longest run of them leave
    /// it, and groups without tracks go.
    pub fn normalize_groups(&mut self) {
        let n = self.tracks.len();
        for i in 1..n.saturating_sub(1) {
            let (a, b) = (self.tracks[i - 1].group, self.tracks[i + 1].group);
            if self.tracks[i].group.is_none() && a.is_some() && a == b {
                self.tracks[i].group = a;
            }
        }
        let groups: Vec<TrackGroupId> = self.track_groups.iter().map(|g| g.id).collect();
        for g in groups {
            // Its runs of tracks: the longest stays (the first of equals).
            let mut runs: Vec<(usize, usize)> = Vec::new();
            for i in 0..n {
                if self.tracks[i].group == Some(g) {
                    match runs.last_mut() {
                        Some((_, end)) if *end == i => *end = i + 1,
                        _ => runs.push((i, i + 1)),
                    }
                }
            }
            let keep = runs
                .iter()
                .copied()
                .enumerate()
                .max_by_key(|(k, (s, e))| (e - s, std::cmp::Reverse(*k)))
                .map(|(_, r)| r);
            for (s, e) in runs {
                if Some((s, e)) != keep {
                    for t in &mut self.tracks[s..e] {
                        t.group = None;
                    }
                }
            }
        }
        // Tracks of groups that are gone leave them; groups without tracks
        // go.
        let known: std::collections::HashSet<TrackGroupId> =
            self.track_groups.iter().map(|g| g.id).collect();
        for t in &mut self.tracks {
            if t.group.is_some_and(|g| !known.contains(&g)) {
                t.group = None;
            }
        }
        let used: std::collections::HashSet<TrackGroupId> =
            self.tracks.iter().filter_map(|t| t.group).collect();
        self.track_groups.retain(|g| used.contains(&g.id));
    }

    /// Checks that each group's tracks are next to one another and every
    /// track's group exists.
    pub(crate) fn check_track_groups(&self) -> Result<(), String> {
        for g in &self.track_groups {
            let rows: Vec<usize> = (0..self.tracks.len())
                .filter(|&i| self.tracks[i].group == Some(g.id))
                .collect();
            if rows.is_empty() {
                return Err(format!("track group {:?} without tracks", g.id));
            }
            if rows.windows(2).any(|w| w[1] != w[0] + 1) {
                return Err(format!("track group {:?} not together", g.id));
            }
        }
        for t in &self.tracks {
            if let Some(g) = t.group
                && self.track_group(g).is_none()
            {
                return Err(format!("track {:?} in a missing group {:?}", t.id, g));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProjectSettings;

    fn project(n: usize) -> (Project, Vec<TrackId>) {
        let mut p = Project::new(ProjectSettings::default());
        for _ in 0..n {
            p.add_track(TrackKind::Audio);
        }
        let ids = p.tracks.iter().map(|t| t.id).collect();
        (p, ids)
    }

    /// Only adjacent tracks group; a group keeps its tracks together as
    /// tracks move and come and go.
    #[test]
    fn groups_are_adjacent_tracks_and_stay_together() {
        // Video 1, Audio 1, and four more audio tracks.
        let (mut p, t) = project(4);
        assert_eq!(
            p.group_tracks(&[t[1], t[3]]),
            Err(EditError::TracksNotAdjacent)
        );
        assert_eq!(p.group_tracks(&[t[2]]), Err(EditError::TracksNotAdjacent));
        let g = p.group_tracks(&[t[2], t[3], t[4]]).unwrap();
        assert_eq!(p.track_group_members(g), vec![t[2], t[3], t[4]]);
        p.check_track_groups().unwrap();
        // A track moved between two of its tracks joins it; one moved out
        // leaves it.
        p.move_track_to(t[5], 3).unwrap();
        assert_eq!(p.track_group_members(g), vec![t[2], t[5], t[3], t[4]]);
        p.move_track_to(t[3], 0).unwrap();
        assert_eq!(p.track_group_members(g), vec![t[2], t[5], t[4]]);
        p.check_track_groups().unwrap();
        // A track ungrouped from the middle moves below the group.
        p.ungroup_tracks(&[t[5]]);
        assert_eq!(p.track_group_members(g), vec![t[2], t[4]]);
        let order: Vec<TrackId> = p.tracks.iter().map(|x| x.id).collect();
        let at = |id| order.iter().position(|&x| x == id).unwrap();
        assert!(at(t[5]) > at(t[4]));
        // A track inserted inside the group joins it.
        let new = p.insert_track_at(TrackKind::Audio, at(t[4]));
        assert_eq!(p.track_group_members(g), vec![t[2], new, t[4]]);
        // Removed tracks leave it; the last gone takes the group along.
        for id in p.track_group_members(g) {
            p.remove_track(id).unwrap();
        }
        assert!(p.track_groups.is_empty());
        p.check_invariants().unwrap();
    }

    /// A group's header dragged moves its tracks together.
    #[test]
    fn a_group_moves_as_one() {
        let (mut p, t) = project(4);
        let g = p.group_tracks(&[t[3], t[4]]).unwrap();
        p.move_track_group_to(g, 1);
        let order: Vec<TrackId> = p.tracks.iter().map(|x| x.id).collect();
        assert_eq!(order, vec![t[0], t[3], t[4], t[1], t[2], t[5]]);
        p.move_track_group_to(g, 6);
        let order: Vec<TrackId> = p.tracks.iter().map(|x| x.id).collect();
        assert_eq!(order, vec![t[0], t[1], t[2], t[5], t[3], t[4]]);
        p.check_track_groups().unwrap();
    }

    /// The group's Mute overrides its tracks' solo, its Solo their mute;
    /// audio and video follow them.
    #[test]
    fn a_groups_mute_and_solo_override_its_tracks() {
        let (mut p, t) = project(2);
        let g = p.group_tracks(&[t[1], t[2]]).unwrap();
        p.track_by_id_mut(t[1]).unwrap().solo = true;
        p.track_group_mut(g).unwrap().muted = true;
        let audible = |p: &Project| -> Vec<TrackId> { p.audible_tracks().map(|x| x.id).collect() };
        // The group's mute wins over its track's solo (the other track,
        // outside the group, plays).
        assert_eq!(audible(&p), vec![t[3]]);
        p.track_group_mut(g).unwrap().muted = false;
        assert_eq!(audible(&p), vec![t[1]]);
        p.track_by_id_mut(t[1]).unwrap().solo = false;
        p.track_by_id_mut(t[2]).unwrap().muted = true;
        p.track_group_mut(g).unwrap().solo = true;
        // Soloed: both play (the muted one too), the ungrouped one not.
        assert_eq!(audible(&p), vec![t[1], t[2]]);
        assert!(!p.is_audible(p.track(t[3]).unwrap()));
    }

    /// A project saved and opened keeps its groups; a file whose group's
    /// tracks are apart (another program's) is brought back to one run.
    #[test]
    fn groups_survive_the_project_file() {
        let (mut p, t) = project(3);
        let g = p.group_tracks(&[t[3], t[4]]).unwrap();
        p.track_group_mut(g).unwrap().name = "Dialogue".into();
        let back = Project::from_json(&p.to_json()).unwrap();
        assert_eq!(back.track_group_members(g), vec![t[3], t[4]]);
        assert_eq!(back.track_group(g).unwrap().name, "Dialogue");
        // A group's tracks apart (its longest run stays) and a track in a
        // missing group.
        let mut odd = p.clone();
        odd.tracks[0].group = Some(g);
        odd.tracks[1].group = Some(TrackGroupId(99));
        let fixed = Project::from_json(&odd.to_json()).unwrap();
        fixed.check_invariants().unwrap();
        assert_eq!(fixed.track_group_members(g), vec![t[3], t[4]]);
        assert_eq!(fixed.tracks[0].group, None);
        assert_eq!(fixed.tracks[1].group, None);
    }

    /// VEGAS's Help's sample of mute and solo with compositing parents:
    /// track 1 holds 2 to 6, track 4 holds 5 and 6, track 7 stands alone.
    /// Muting 1 mutes 1 to 6; muting 4, 4 to 6. Soloing 1 plays 1 to 6;
    /// soloing 4 plays 4 to 6, bypasses its siblings 2 and 3, and 1 and 7
    /// play normally. Ctrl on Mute and Solo: that track alone, or none.
    #[test]
    fn mute_and_solo_follow_the_compositing_parents() {
        let mut p = Project::new(ProjectSettings::default());
        let mut v = vec![p.tracks[0].id];
        for i in 1..7 {
            v.push(p.insert_track_at(TrackKind::Video, i));
        }
        for (i, d) in [0u8, 1, 1, 1, 2, 2, 0].iter().enumerate() {
            p.track_by_id_mut(v[i]).unwrap().composite_depth = *d;
        }
        p.normalize_compositing();
        let shown = |p: &Project| -> Vec<usize> {
            p.visible_video_tracks()
                .map(|t| v.iter().position(|x| *x == t.id).unwrap() + 1)
                .collect()
        };
        assert_eq!(shown(&p), vec![1, 2, 3, 4, 5, 6, 7]);
        p.track_by_id_mut(v[0]).unwrap().muted = true;
        assert_eq!(shown(&p), vec![7]);
        p.track_by_id_mut(v[0]).unwrap().muted = false;
        p.track_by_id_mut(v[3]).unwrap().muted = true;
        assert_eq!(shown(&p), vec![1, 2, 3, 7]);
        p.track_by_id_mut(v[3]).unwrap().muted = false;
        p.track_by_id_mut(v[0]).unwrap().solo = true;
        assert_eq!(shown(&p), vec![1, 2, 3, 4, 5, 6]);
        p.track_by_id_mut(v[0]).unwrap().solo = false;
        p.track_by_id_mut(v[3]).unwrap().solo = true;
        assert_eq!(shown(&p), vec![1, 4, 5, 6, 7]);
        // Ctrl on Solo: track 2 alone; again on it, none.
        p.solo_alone(v[1]);
        assert_eq!(shown(&p), vec![1, 2, 7]);
        p.solo_alone(v[1]);
        assert!(p.tracks.iter().all(|t| !t.solo));
        // Ctrl on Mute: track 7 alone muted; again, none.
        p.track_by_id_mut(v[2]).unwrap().muted = true;
        p.mute_alone(v[6]);
        assert_eq!(shown(&p), vec![1, 2, 3, 4, 5, 6]);
        p.mute_alone(v[6]);
        assert!(p.tracks.iter().all(|t| !t.muted));
    }

    /// A video child and its parent group together.
    #[test]
    fn compositing_families_group_whole() {
        let mut p = Project::new(ProjectSettings::default());
        let v1 = p.tracks[0].id;
        let v2 = p.insert_track_at(TrackKind::Video, 1);
        let v3 = p.insert_track_at(TrackKind::Video, 2);
        p.track_by_id_mut(v2).unwrap().composite_depth = 1;
        p.normalize_compositing();
        // Grouping the child and the track below takes its parent along.
        let g = p.group_tracks(&[v2, v3]).unwrap();
        assert_eq!(p.track_group_members(g), vec![v1, v2, v3]);
    }
}
