//! Edits that ripple the rest of the timeline (insert space, remove a time
//! range, close gaps, post-edit ripple) and the operations behind the
//! slip, slide and rejoin tools.

use bv_core::{Tick, TimeRange};

use crate::edit::{EditError, clamp_fades};
use crate::model::{ClipId, Project, TrackId, TrackKind};

type Result<T> = std::result::Result<T, EditError>;

/// What a ripple moves (Options › Ripple, the Auto Ripple button's menu,
/// Edit › Post-Edit Ripple), as VEGAS's three: each one's events, their
/// envelopes and keyframes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RippleScope {
    /// Only the tracks of the edited events.
    AffectedTracks,
    /// The affected tracks, the bus tracks (the video bus's, the busses'
    /// and the effects chains': their envelopes and keyframes), the
    /// markers, regions, CD track and index markers and command markers.
    AffectedTracksAndMarkers,
    /// Every track, the bus tracks, the markers and regions.
    All,
}

/// A point or keyframe on the timeline, whose time a ripple moves.
trait Timed {
    fn at(&self) -> Tick;
    fn set_at(&mut self, t: Tick);
}

macro_rules! timed {
    ($($t:ty),*) => {$(
        impl Timed for $t {
            fn at(&self) -> Tick {
                self.at
            }
            fn set_at(&mut self, t: Tick) {
                self.at = t;
            }
        }
    )*};
}

timed!(
    crate::envelope::EnvelopePoint,
    crate::fx::TransformKey,
    crate::fx::PluginKey,
    crate::motion::HaloKey,
    crate::space::Box3dKey
);

/// `keys` (in time order) at the times `to` gives them, which keeps their
/// order: those it drops go, and of two landing at the same time the later
/// one stays (what followed the time that closed).
fn remap<K: Timed>(keys: &mut Vec<K>, to: &dyn Fn(Tick) -> Option<Tick>) {
    if keys.is_empty() {
        return;
    }
    let mut out: Vec<K> = Vec::with_capacity(keys.len());
    for mut k in keys.drain(..) {
        let Some(t) = to(k.at()) else { continue };
        k.set_at(t);
        if out.last().is_some_and(|l| l.at() == t) {
            out.pop();
        }
        out.push(k);
    }
    *keys = out;
}

/// The keyframes of `effects`' plug-ins (on the timeline: a track's, a
/// bus's), remapped by `to`.
fn remap_effects(effects: &mut [crate::fx::Effect], to: &dyn Fn(Tick) -> Option<Tick>) {
    for e in effects {
        if let Some(fx) = e.plugin.as_deref_mut() {
            remap_plugin(fx, to);
        }
    }
}

fn remap_plugin(fx: &mut crate::fx::PluginFx, to: &dyn Fn(Tick) -> Option<Tick>) {
    for p in fx.params.values_mut() {
        remap(&mut p.keys, to);
    }
}

/// Where time `t` goes when `holes` close by `closings`
/// ([`crate::edit::closed`]); `None` inside a part of one that closed:
/// what was there went with the events deleted.
fn closed_or_gone(t: Tick, holes: &[TimeRange], closings: &[Tick]) -> Option<Tick> {
    holes
        .iter()
        .zip(closings)
        .all(|(h, &c)| t <= h.start || t >= h.start + c)
        .then(|| crate::edit::closed(t, holes, closings))
}

/// Where time `t` goes when what is from `from` on moves `delta`; `None`
/// in the time it moves back over.
fn shifted_or_gone(t: Tick, from: Tick, delta: Tick) -> Option<Tick> {
    if t >= from {
        Some((t + delta).max(Tick::ZERO))
    } else if delta < Tick::ZERO && t > from + delta {
        None
    } else {
        Some(t)
    }
}

impl Project {
    fn target_tracks(&self, tracks: &[TrackId]) -> Vec<TrackId> {
        if tracks.is_empty() {
            self.tracks.iter().map(|t| t.id).collect()
        } else {
            tracks.to_vec()
        }
    }

    /// Splits every clip of `tracks` that strictly contains `at` (without
    /// touching group members on other tracks).
    fn split_tracks_at(&mut self, at: Tick, tracks: &[TrackId]) {
        let ids: Vec<ClipId> = self
            .tracks
            .iter()
            .filter(|t| tracks.contains(&t.id))
            .flat_map(|t| t.clips.iter())
            .filter(|c| at > c.start && at < c.end())
            .map(|c| c.id)
            .collect();
        for id in ids {
            // Splitting one member may already have split the others.
            if self.clip(id).is_some_and(|c| at > c.start && at < c.end()) {
                let _ = self.split(id, at);
            }
        }
    }

    fn shift_markers_from(&mut self, from: Tick, delta: Tick) {
        for m in &mut self.markers {
            if m.time >= from {
                m.time = (m.time + delta).max(Tick::ZERO);
            }
        }
        for r in &mut self.regions {
            if r.range.start >= from {
                r.range.start = (r.range.start + delta).max(Tick::ZERO);
            }
        }
        for c in &mut self.commands {
            if c.time >= from {
                c.time = (c.time + delta).max(Tick::ZERO);
            }
        }
        for t in &mut self.cd_tracks {
            if t.range.start >= from {
                t.range.start = (t.range.start + delta).max(Tick::ZERO);
            }
        }
        for i in &mut self.cd_indices {
            if i.time >= from {
                i.time = (i.time + delta).max(Tick::ZERO);
            }
        }
        self.sort_commands();
        self.sort_cd();
    }

    /// A ripple's move of the automation of tracks `tracks` and, with
    /// `busses`, of the bus tracks (the video bus's, the busses' and the
    /// effects chains'): their envelopes, Track Motion's keyframes and
    /// their effects' and compositors' keyframes — all on the timeline —
    /// at the times `to` gives them.
    fn ripple_automation(
        &mut self,
        tracks: &[TrackId],
        busses: bool,
        to: &dyn Fn(Tick) -> Option<Tick>,
    ) {
        for t in self.tracks.iter_mut().filter(|t| tracks.contains(&t.id)) {
            for e in &mut t.envelopes {
                remap(&mut e.points, to);
            }
            for m in [&mut t.motion, &mut t.parent_motion].into_iter().flatten() {
                remap(&mut m.keys, to);
                remap(&mut m.shadow.keys, to);
                remap(&mut m.glow.keys, to);
                remap(&mut m.space_keys, to);
            }
            remap_effects(&mut t.effects, to);
            for c in [&mut t.compositor, &mut t.parent_compositor]
                .into_iter()
                .flatten()
            {
                remap_plugin(c, to);
            }
        }
        if !busses {
            return;
        }
        for e in &mut self.video_bus.envelopes {
            remap(&mut e.points, to);
        }
        remap_effects(&mut self.video_output_fx, to);
        for s in self.strips() {
            if let Some(ch) = self.strip_channel_mut(s) {
                for e in &mut ch.envelopes {
                    remap(&mut e.points, to);
                }
                remap_effects(&mut ch.effects, to);
            }
        }
    }

    /// What a Track Select tool picks with a press at `at` (a BananaV mod,
    /// as other editors' tools): every event reaching past `at` — or, with
    /// `back`, starting before it —, on track `track` only or on all tracks
    /// (`None`); with `grouped`, their groups' members too. Sorted.
    pub fn track_selection(
        &self,
        at: Tick,
        track: Option<TrackId>,
        back: bool,
        grouped: bool,
    ) -> Vec<ClipId> {
        let picked: Vec<ClipId> = self
            .tracks
            .iter()
            .filter(|t| track.is_none_or(|id| t.id == id))
            .flat_map(|t| t.clips.iter())
            .filter(|c| if back { c.start < at } else { c.end() > at })
            .map(|c| c.id)
            .collect();
        self.edit_reach(&picked, grouped)
    }

    /// Opens `dur` of empty time at `at` on `tracks` (all tracks when empty):
    /// clips crossing `at` are split and everything from `at` on moves right.
    /// With `markers`, markers and regions after `at` move too.
    pub fn insert_space(&mut self, at: Tick, dur: Tick, tracks: &[TrackId], markers: bool) {
        if dur <= Tick::ZERO {
            return;
        }
        let tracks = self.target_tracks(tracks);
        self.split_tracks_at(at, &tracks);
        for t in self.tracks.iter_mut().filter(|t| tracks.contains(&t.id)) {
            for c in t.clips.iter_mut().filter(|c| c.start >= at) {
                c.start += dur;
            }
            t.sort();
        }
        if markers {
            self.shift_markers_from(at, dur);
        }
    }

    /// Removes the time range `r` from `tracks` (all when empty) and closes
    /// the hole. With `markers`, markers inside the range are removed and the
    /// ones after it move left. Returns how many clips were removed.
    pub fn delete_range(&mut self, r: TimeRange, tracks: &[TrackId], markers: bool) -> usize {
        if r.duration <= Tick::ZERO {
            return 0;
        }
        let tracks = self.target_tracks(tracks);
        self.split_tracks_at(r.start, &tracks);
        self.split_tracks_at(r.end(), &tracks);
        let mut removed = 0;
        for t in self.tracks.iter_mut().filter(|t| tracks.contains(&t.id)) {
            let before = t.clips.len();
            t.clips
                .retain(|c| !(c.start >= r.start && c.end() <= r.end()));
            removed += before - t.clips.len();
            for c in t.clips.iter_mut().filter(|c| c.start >= r.end()) {
                c.start -= r.duration;
            }
            t.sort();
        }
        if markers {
            self.markers
                .retain(|m| !(m.time >= r.start && m.time < r.end()));
            self.regions
                .retain(|g| !(g.range.start >= r.start && g.range.start < r.end()));
            self.commands
                .retain(|c| !(c.time >= r.start && c.time < r.end()));
            self.cd_tracks
                .retain(|t| !(t.range.start >= r.start && t.range.start < r.end()));
            self.cd_indices
                .retain(|i| !(i.time >= r.start && i.time < r.end()));
            self.shift_markers_from(r.end(), -r.duration);
        }
        removed
    }

    /// Edit › Smart Split: removes `r` from `tracks` (all when empty) as
    /// [`Self::delete_range`] does, then slides what follows the cut back
    /// `overlap` over what precedes it, so that the two parts left on each
    /// side overlap and a transition joins them — on video tracks plug-in
    /// `video`'s (VEGAS's Warp Flow), a crossfade where there is none; each
    /// pair is grouped (Options › Preferences › Editing: "Smart split
    /// default length"). Returns how many events were removed.
    pub fn smart_split(
        &mut self,
        r: TimeRange,
        tracks: &[TrackId],
        markers: bool,
        overlap: Tick,
        video: Option<&crate::fx::PluginFx>,
    ) -> usize {
        let targets = self.target_tracks(tracks);
        let removed = self.delete_range(r, tracks, markers);
        // The parts that meet at the cut, on each track.
        let mut pairs: Vec<(ClipId, ClipId, TrackKind)> = Vec::new();
        for t in self.tracks.iter().filter(|t| targets.contains(&t.id)) {
            let left = t.clips.iter().rfind(|c| c.end() == r.start);
            let right = t.clips.iter().find(|c| c.start == r.start);
            if let (Some(l), Some(rt)) = (left, right) {
                pairs.push((l.id, rt.id, t.kind));
            }
        }
        // No longer than the shortest part.
        let overlap = pairs
            .iter()
            .filter_map(|&(l, rt, _)| Some(self.clip(l)?.duration.min(self.clip(rt)?.duration)))
            .fold(overlap, Tick::min);
        if pairs.is_empty() || overlap <= Tick::ZERO {
            return removed;
        }
        for t in self.tracks.iter_mut().filter(|t| targets.contains(&t.id)) {
            for c in t.clips.iter_mut().filter(|c| c.start >= r.start) {
                c.start -= overlap;
            }
            t.sort();
        }
        if markers {
            self.shift_markers_from(r.start, -overlap);
        }
        // Each pair crossfades (also with Automatic Crossfades off), and the
        // parts and their groups become one group.
        let mut joined: Vec<ClipId> = Vec::new();
        for &(l, rt, kind) in &pairs {
            // The crossfade first: the fade each part had at its other end
            // keeps what the part has left.
            if let Some(c) = self.clip_mut(l) {
                c.fade_out_at_least(overlap);
            }
            if let Some(c) = self.clip_mut(rt) {
                c.fade_in_at_least(overlap);
                if kind == TrackKind::Video
                    && let Some(fx) = video
                {
                    c.transition_fx = Some(Box::new(fx.clone()));
                }
            }
            joined.extend(self.edit_reach(&[l, rt], true));
        }
        self.group_clips(&joined);
        removed
    }

    /// Turns the cut where event `id` meets the event before it on its
    /// track into an overlap for a transition (Options › Preferences ›
    /// Editing: "Cut-to-overlap conversion"): the event grows back `before`
    /// the cut and the one before it grows `after` it, each with its group,
    /// as far as their media goes (Loop repeats it). Returns the overlap
    /// made: zero when no event ends where this one starts.
    pub fn overlap_cut(&mut self, id: ClipId, before: Tick, after: Tick) -> Result<Tick> {
        let clip = self.clip(id).ok_or(EditError::UnknownClip(id))?.clone();
        let track = self.clip_track(id).ok_or(EditError::UnknownClip(id))?;
        let Some(prev) = track
            .clips
            .iter()
            .filter(|c| c.id != id && c.end() == clip.start && c.start < clip.start)
            .max_by_key(|c| c.start)
            .cloned()
        else {
            return Ok(Tick::ZERO);
        };
        // The media left after the event before, and before this one.
        let tail = match self.media(prev.media) {
            Some(m) if !prev.looped && prev.freeze.is_none() => {
                let used = prev.source_in + prev.to_source(prev.duration);
                prev.to_timeline((m.duration - used).max(Tick::ZERO))
            }
            _ => after,
        };
        let head = if clip.looped || clip.freeze.is_some() {
            before
        } else {
            clip.to_timeline(clip.source_in)
        };
        let grow_prev = after.min(tail).max(Tick::ZERO);
        let grow_clip = before.min(head).min(clip.start).max(Tick::ZERO);
        let mut made = Tick::ZERO;
        if grow_prev > Tick::ZERO {
            made += self.trim_end(prev.id, prev.end() + grow_prev)?;
        }
        if grow_clip > Tick::ZERO {
            made -= self.trim_start(id, clip.start - grow_clip)?;
        }
        Ok(made)
    }

    /// Packs the given clips on each of their tracks so that no gap remains
    /// between consecutive ones. Returns how many clips moved.
    pub fn close_gaps(&mut self, ids: &[ClipId]) -> usize {
        let mut moved = 0;
        for t in &mut self.tracks {
            let mut chosen: Vec<usize> = (0..t.clips.len())
                .filter(|&i| ids.contains(&t.clips[i].id))
                .collect();
            chosen.sort_by_key(|&i| t.clips[i].start);
            let mut end: Option<Tick> = None;
            for i in chosen {
                let c = &mut t.clips[i];
                if let Some(e) = end
                    && c.start > e
                    && !c.locked
                {
                    c.start = e;
                    moved += 1;
                }
                end = Some(c.end());
            }
            t.sort();
        }
        moved
    }

    /// Post-edit ripple: moves everything after the edited clip `id` so that
    /// the next clip on its track starts exactly where `id` now ends. Returns
    /// the shift applied (positive = pushed right).
    pub fn post_edit_ripple(&mut self, id: ClipId, scope: RippleScope) -> Result<Tick> {
        let clip = self.clip(id).ok_or(EditError::UnknownClip(id))?.clone();
        let track = self.clip_track(id).ok_or(EditError::UnknownClip(id))?;
        let group = self.group_members(id);
        let Some(next_start) = track
            .clips
            .iter()
            .filter(|c| !group.contains(&c.id) && c.start > clip.start)
            .map(|c| c.start)
            .min()
        else {
            return Ok(Tick::ZERO);
        };
        let delta = clip.end() - next_start;
        if delta == Tick::ZERO {
            return Ok(Tick::ZERO);
        }
        let tracks: Vec<TrackId> = match scope {
            RippleScope::All => self.tracks.iter().map(|t| t.id).collect(),
            _ => group
                .iter()
                .filter_map(|&m| self.clip_track(m).map(|t| t.id))
                .collect(),
        };
        for t in self.tracks.iter_mut().filter(|t| tracks.contains(&t.id)) {
            for c in t
                .clips
                .iter_mut()
                .filter(|c| !group.contains(&c.id) && c.start >= next_start)
            {
                c.start = (c.start + delta).max(Tick::ZERO);
            }
            t.sort();
        }
        let busses = scope != RippleScope::AffectedTracks;
        self.ripple_automation(&tracks, busses, &|t| shifted_or_gone(t, next_start, delta));
        if busses {
            self.shift_markers_from(next_start, delta);
        }
        Ok(delta)
    }

    /// Joins the clip `id` (and its group members) with the clip right after
    /// it when both halves come from the same split: same media, same rate,
    /// the same takes, contiguous on the timeline and in the media. Returns
    /// whether anything was joined.
    /// Whether Event Heal would join event `id` (or one of its group) to
    /// the next part of its split.
    pub fn can_rejoin(&self, id: ClipId) -> bool {
        self.group_members(id)
            .into_iter()
            .any(|m| self.rejoin_partner(m).is_some())
    }

    /// Whether Close Gaps would move any of events `ids`: one of them after
    /// a gap from the one before it on its track, not locked.
    pub fn has_gaps(&self, ids: &[ClipId]) -> bool {
        self.tracks.iter().any(|t| {
            let mut chosen: Vec<&crate::Clip> =
                t.clips.iter().filter(|c| ids.contains(&c.id)).collect();
            chosen.sort_by_key(|c| c.start);
            chosen
                .windows(2)
                .any(|w| w[1].start > w[0].end() && !w[1].locked)
        })
    }

    pub fn rejoin(&mut self, id: ClipId) -> Result<bool> {
        self.find_clip(id).ok_or(EditError::UnknownClip(id))?;
        let mut joined = false;
        for m in self.group_members(id) {
            joined |= self.rejoin_single(m);
        }
        Ok(joined)
    }

    /// The event that goes on from event `id`'s split: its track and its
    /// place there.
    fn rejoin_partner(&self, id: ClipId) -> Option<(usize, usize)> {
        let (ti, ci) = self.find_clip(id)?;
        let t = &self.tracks[ti];
        let a = &t.clips[ci];
        let bi = t.clips.iter().position(|b| {
            b.id != a.id
                && b.start == a.end()
                && b.media == a.media
                && b.rate == a.rate
                && b.freeze.is_none()
                && a.freeze.is_none()
                && (b.source_in - (a.source_in + a.to_source(a.duration))).abs() <= Tick(1)
                && b.active_take() == a.active_take()
                && b.take_list() == a.take_list()
                && crate::mask::PanCropMask::joined(&a.mask, &b.mask, a.duration).is_some()
        })?;
        Some((ti, bi))
    }

    fn rejoin_single(&mut self, id: ClipId) -> bool {
        let Some((ti, bi)) = self.rejoin_partner(id) else {
            return false;
        };
        let b = self.tracks[ti].clips.remove(bi);
        let ci = self.tracks[ti]
            .clips
            .iter()
            .position(|c| c.id == id)
            .expect("the left half is still there");
        let a = &mut self.tracks[ti].clips[ci];
        let offset = a.duration;
        if let Some(mask) = crate::mask::PanCropMask::joined(&a.mask, &b.mask, offset) {
            a.mask = mask;
        }
        let size = if a.pan_crop.size[0] > 0 {
            a.pan_crop.own_size()
        } else {
            b.pan_crop.own_size()
        };
        a.pan_crop.joined(&b.pan_crop, offset, size);
        a.duration += b.duration;
        a.fade_out = b.fade_out;
        a.fade_out_curve = b.fade_out_curve;
        clamp_fades(a);
        true
    }

    /// Deletes the clips (with their groups) and closes the holes like
    /// Options › Ripple: on the affected tracks only; also moving the bus
    /// tracks' automation, the markers and regions; or on every track. The
    /// tracks' envelopes and keyframes go as their events do (those in the
    /// time that closed go with the events deleted).
    pub fn delete_clips_rippled(&mut self, ids: &[ClipId], scope: RippleScope) -> Result<()> {
        self.delete_events_rippled(ids, scope, true)
    }

    /// [`Self::delete_clips_rippled`]; with `grouped` off, only the events
    /// given go (Options › Preferences › Editing: "Cut, copy, and delete
    /// grouped events").
    pub fn delete_events_rippled(
        &mut self,
        ids: &[ClipId],
        scope: RippleScope,
        grouped: bool,
    ) -> Result<()> {
        let doomed = self.edit_reach(ids, grouped);
        let mut ranges = Vec::new();
        let mut affected = Vec::new();
        for &id in &doomed {
            let c = self.clip(id).ok_or(EditError::UnknownClip(id))?;
            ranges.push(c.range());
            if let Some(t) = self.clip_track(id) {
                affected.push(t.id);
            }
        }
        // How far each hole closes for the markers and the other tracks:
        // as far as on the track it is on — less than its length where an
        // event left there starts inside it (`hole_closings`).
        let holes = crate::edit::union(&ranges);
        let mut closings: Vec<Tick> = holes.iter().map(|h| h.duration).collect();
        // Each affected track's own holes and how far they close: its
        // envelopes and keyframes go as its events do.
        let mut own_holes: Vec<(TrackId, Vec<TimeRange>, Vec<Tick>)> = Vec::new();
        for t in &self.tracks {
            let (gone, left): (Vec<_>, Vec<_>) = t
                .clips
                .iter()
                .partition(|c| doomed.binary_search(&c.id).is_ok());
            let own: Vec<TimeRange> = gone.iter().map(|c| c.range()).collect();
            let own = crate::edit::union(&own);
            let starts: Vec<Tick> = left.iter().map(|c| c.start).collect();
            let own_closings = crate::edit::hole_closings(&own, &starts);
            for (h, &closing) in own.iter().zip(&own_closings) {
                let whole = holes
                    .iter()
                    .position(|u| u.start <= h.start && h.end() <= u.end());
                if let Some(k) = whole {
                    closings[k] = closings[k].min(holes[k].duration - (h.duration - closing));
                }
            }
            if !own.is_empty() {
                own_holes.push((t.id, own, own_closings));
            }
        }
        self.delete_events(&doomed, true, false)?;
        for (track, own, own_closings) in &own_holes {
            self.ripple_automation(&[*track], false, &|t| closed_or_gone(t, own, own_closings));
        }
        if scope == RippleScope::AffectedTracks {
            return Ok(());
        }
        self.ripple_automation(&[], true, &|t| closed_or_gone(t, &holes, &closings));
        // Inside a hole: lands where the hole began.
        let shifted = |t: Tick| crate::edit::closed(t, &holes, &closings);
        for m in &mut self.markers {
            m.time = shifted(m.time);
        }
        for r in &mut self.regions {
            let end = shifted(r.range.end());
            r.range.start = shifted(r.range.start);
            r.range.duration = (end - r.range.start).max(Tick(1));
        }
        for c in &mut self.commands {
            c.time = shifted(c.time);
        }
        for t in &mut self.cd_tracks {
            let end = shifted(t.range.end());
            t.range.start = shifted(t.range.start);
            t.range.duration = (end - t.range.start).max(Tick(1));
        }
        for i in &mut self.cd_indices {
            i.time = shifted(i.time);
        }
        self.sort_commands();
        self.sort_cd();
        if scope == RippleScope::All {
            for t in self.tracks.iter_mut().filter(|t| !affected.contains(&t.id)) {
                for c in &mut t.clips {
                    c.start = shifted(c.start);
                }
                t.sort();
            }
            let others: Vec<TrackId> = self
                .tracks
                .iter()
                .map(|t| t.id)
                .filter(|id| !affected.contains(id))
                .collect();
            self.ripple_automation(&others, false, &|t| closed_or_gone(t, &holes, &closings));
        }
        Ok(())
    }

    /// Removes media that no event uses (Tools › Clean Project Media) —
    /// a subclip's media is in use —, each as [`Self::remove_media`] does:
    /// out of the bins, the tags and the storyboards too. Returns how many
    /// were removed.
    pub fn remove_unused_media(&mut self) -> usize {
        let used: std::collections::HashSet<crate::model::MediaId> = self
            .clips()
            .flat_map(|(_, c)| c.take_media())
            .chain(self.subclips.iter().map(|s| s.parent))
            .collect();
        let unused: Vec<crate::model::MediaId> = self
            .media
            .iter()
            .map(|m| m.id)
            .filter(|id| !used.contains(id))
            .collect();
        for &id in &unused {
            // None of them has events or subclips: only what names it goes.
            let _ = self.remove_media(id);
        }
        unused.len()
    }

    /// Slip: shows a different part of the media inside the clip (and its
    /// group) without moving it. Positive `delta` (timeline time) reveals
    /// earlier media, like dragging the content to the right. Returns the
    /// shift applied after clamping to the available media.
    pub fn slip_clip(&mut self, id: ClipId, delta: Tick) -> Result<Tick> {
        let members: Vec<(usize, usize)> = self
            .group_members(id)
            .into_iter()
            .filter_map(|m| self.find_clip(m))
            .collect();
        if members.is_empty() {
            return Err(EditError::UnknownClip(id));
        }
        let mut delta = delta;
        for &(t, c) in &members {
            let m = &self.tracks[t].clips[c];
            if m.locked {
                return Ok(Tick::ZERO);
            }
            let media = self
                .media(m.media)
                .ok_or(EditError::UnknownMedia(m.media))?;
            // Loop repeats the media: it slips freely.
            if media.still || m.freeze.is_some() || m.looped {
                continue;
            }
            let right_room = m.to_timeline(m.source_in);
            let used = m.to_source(m.duration);
            let left_room = m.to_timeline((media.duration - m.source_in - used).max(Tick::ZERO));
            delta = delta.clamp(-left_room, right_room);
        }
        for &(t, c) in &members {
            let m = &mut self.tracks[t].clips[c];
            m.source_in -= m.to_source(delta);
            if !m.looped {
                m.source_in = m.source_in.max(Tick::ZERO);
            }
        }
        Ok(delta)
    }

    /// Slide: moves the clip (and its group) by `delta` while the clips
    /// touching it on its track stretch or shrink to stay attached. Returns
    /// the shift applied.
    pub fn slide_clip(&mut self, id: ClipId, delta: Tick) -> Result<Tick> {
        let clip = self.clip(id).ok_or(EditError::UnknownClip(id))?.clone();
        let track = self.clip_track(id).ok_or(EditError::UnknownClip(id))?;
        let group = self.group_members(id);
        let prev = track
            .clips
            .iter()
            .filter(|c| !group.contains(&c.id) && c.end() >= clip.start && c.start < clip.start)
            .max_by_key(|c| c.end())
            .map(|c| (c.id, c.end()));
        let next = track
            .clips
            .iter()
            .filter(|c| !group.contains(&c.id) && c.start <= clip.end() && c.start > clip.start)
            .min_by_key(|c| c.start)
            .map(|c| (c.id, c.start));
        // Find the largest shift that both neighbours accept.
        let mut d = delta;
        for _ in 0..3 {
            let mut probe = self.clone();
            let mut applied = d;
            if let Some((p, end)) = prev {
                applied = probe.trim_end(p, end + applied)?;
            }
            if let Some((n, start)) = next {
                applied = probe.trim_start(n, start + applied)?;
            }
            if applied == d {
                break;
            }
            d = applied;
        }
        if let Some((p, end)) = prev {
            self.trim_end(p, end + d)?;
        }
        if let Some((n, start)) = next {
            self.trim_start(n, start + d)?;
        }
        self.move_clip(id, clip.start + d, None)
    }

    /// Slip trim (Alt+drag of an edge, like VEGAS's): the edge (`end`: the
    /// event's end) goes to `edge` and the media moves with it — the frame
    /// at that edge stays, and the other edge keeps its place showing
    /// another one. With the event's group. Returns the shift applied to
    /// the edge.
    pub fn slip_trim(&mut self, id: ClipId, end: bool, edge: Tick) -> Result<Tick> {
        let c = self.clip(id).ok_or(EditError::UnknownClip(id))?.clone();
        if self.edges_held(id) {
            return Ok(Tick::ZERO);
        }
        let members: Vec<crate::model::Clip> = self
            .group_members(id)
            .iter()
            .filter_map(|&m| self.clip(m).cloned())
            .collect();
        // No event shorter than a frame.
        let shortest = members
            .iter()
            .map(|m| m.duration)
            .min()
            .unwrap_or(c.duration);
        let shrink = (shortest - self.min_clip_duration()).max(Tick::ZERO);
        if end {
            let mut d = (edge - c.end()).max(-shrink);
            // Longer: the media before each event's start comes in (Loop
            // repeats it; a still or a frozen frame has it all).
            for m in &members {
                let free =
                    m.looped || m.freeze.is_some() || self.media(m.media).is_none_or(|x| x.still);
                if !free {
                    d = d.min(m.to_timeline(m.source_in));
                }
            }
            if d > Tick::ZERO {
                // The event goes on with the edge; its start comes back.
                self.move_clip(id, c.start + d, None)?;
                self.trim_start(id, c.start)?;
            } else if d < Tick::ZERO {
                self.trim_start(id, c.start - d)?;
                self.move_clip(id, c.start, None)?;
            }
            Ok(self.clip(id).map_or(Tick::ZERO, |n| n.end() - c.end()))
        } else {
            // The event goes with the edge; its end comes back.
            let d = (edge - c.start).min(shrink);
            self.move_clip(id, c.start + d, None)?;
            self.trim_end(id, c.end())?;
            Ok(self.clip(id).map_or(Tick::ZERO, |n| n.start - c.start))
        }
    }

    /// Trim adjacent (Ctrl+Alt+drag of an edge, like VEGAS's): the edge of
    /// `id` (`end`: its end) goes to `edge` and the edge of the event
    /// touching or overlapping it there on its track moves with it — no
    /// gap opens and a crossfade keeps its length (Ctrl+Alt+drag over one
    /// slides it). Without such an event, a plain trim. Returns the shift
    /// applied.
    pub fn trim_adjacent(&mut self, id: ClipId, end: bool, edge: Tick) -> Result<Tick> {
        let clip = self.clip(id).ok_or(EditError::UnknownClip(id))?.clone();
        if self.edges_held(id) {
            return Ok(Tick::ZERO);
        }
        let track = self.clip_track(id).ok_or(EditError::UnknownClip(id))?;
        let group = self.group_members(id);
        let other = if end {
            track
                .clips
                .iter()
                .filter(|c| !group.contains(&c.id) && c.start <= clip.end() && c.start > clip.start)
                .min_by_key(|c| c.start)
                .map(|c| (c.id, c.start))
        } else {
            track
                .clips
                .iter()
                .filter(|c| !group.contains(&c.id) && c.end() >= clip.start && c.start < clip.start)
                .max_by_key(|c| c.end())
                .map(|c| (c.id, c.end()))
        };
        let own = if end { clip.end() } else { clip.start };
        // The largest shift both edges accept.
        let mut d = edge - own;
        for _ in 0..3 {
            let mut probe = self.clone();
            let applied = probe.roll_edges(id, end, own, other, d)?;
            if applied == d {
                break;
            }
            d = applied;
        }
        self.roll_edges(id, end, own, other, d)
    }

    /// Trims the edge of `id` at `own` by `d`, then the facing edge of
    /// `other` (at its time) by as much as that allowed.
    fn roll_edges(
        &mut self,
        id: ClipId,
        end: bool,
        own: Tick,
        other: Option<(ClipId, Tick)>,
        d: Tick,
    ) -> Result<Tick> {
        let mut applied = if end {
            self.trim_end(id, own + d)?
        } else {
            self.trim_start(id, own + d)?
        };
        if let Some((o, at)) = other {
            applied = if end {
                self.trim_start(o, at + applied)?
            } else {
                self.trim_end(o, at + applied)?
            };
        }
        Ok(applied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        AudioStreamInfo, MediaId, MediaInfo, MediaSource, ProjectSettings, VideoStreamInfo,
    };
    use bv_core::{FrameRate, SampleRate};

    fn s(x: i64) -> Tick {
        Tick::from_seconds(x)
    }

    fn media(seconds: i64) -> MediaInfo {
        MediaInfo {
            id: MediaId(0),
            source: MediaSource::File {
                path: "clip.mp4".into(),
            },
            name: "clip".into(),
            duration: s(seconds),
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
                sample_rate: SampleRate::HZ_48000,
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
        }
    }

    /// Two 4 s video clips at 0 s and 6 s on the first track (no audio).
    fn two_clips() -> (Project, ClipId, ClipId) {
        let mut p = Project::new(ProjectSettings::default());
        let v = p.tracks[0].id;
        let m = p.add_media(media(20));
        let a = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        let _ = p.trim_end(a, s(4));
        let b = p.insert_media(m, s(6), Some(v), None).unwrap()[0];
        let _ = p.trim_end(b, s(10));
        (p, a, b)
    }

    #[test]
    fn insert_space_splits_and_pushes() {
        let (mut p, a, b) = two_clips();
        p.add_marker(s(7));
        p.insert_space(s(2), s(3), &[], true);
        assert_eq!(p.clip(a).unwrap().duration, s(2));
        assert_eq!(p.clip(b).unwrap().start, s(9));
        assert_eq!(p.markers[0].time, s(10));
        // The right half of the split starts after the inserted space.
        let starts: Vec<Tick> = p.tracks[0].clips.iter().map(|c| c.start).collect();
        assert_eq!(starts, vec![Tick::ZERO, s(5), s(9)]);
        p.check_invariants().unwrap();
    }

    #[test]
    fn delete_range_closes_the_hole() {
        let (mut p, a, b) = two_clips();
        let removed = p.delete_range(TimeRange::new(s(1), s(5)), &[], true);
        assert_eq!(removed, 1, "the 1–4 s part of the first clip goes away");
        assert_eq!(p.clip(a).unwrap().duration, s(1));
        assert_eq!(p.clip(b).unwrap().start, s(1));
        p.check_invariants().unwrap();
    }

    #[test]
    fn close_gaps_packs_selected_clips() {
        let (mut p, a, b) = two_clips();
        assert!(p.has_gaps(&[a, b]));
        assert_eq!(p.close_gaps(&[a, b]), 1);
        assert!(!p.has_gaps(&[a, b]), "no gap left");
        assert_eq!(p.clip(b).unwrap().start, s(4));
    }

    #[test]
    fn post_edit_ripple_follows_a_trim() {
        let (mut p, a, b) = two_clips();
        p.add_marker(s(8));
        let _ = p.trim_end(a, s(5));
        let d = p.post_edit_ripple(a, RippleScope::AffectedTracks).unwrap();
        assert_eq!(d, -s(1));
        assert_eq!(p.clip(b).unwrap().start, s(5));
        assert_eq!(
            p.markers[0].time,
            s(8),
            "markers stay without the marker scope"
        );
        let _ = p.trim_end(a, s(3));
        p.post_edit_ripple(a, RippleScope::All).unwrap();
        assert_eq!(p.clip(b).unwrap().start, s(3));
        assert_eq!(p.markers[0].time, s(6));
    }

    #[test]
    fn rejoin_undoes_a_split() {
        let (mut p, a, _) = two_clips();
        assert!(!p.can_rejoin(a));
        let right = p.split(a, s(1)).unwrap()[0];
        assert!(p.can_rejoin(a));
        assert!(p.rejoin(a).unwrap());
        assert!(p.clip(right).is_none());
        assert_eq!(p.clip(a).unwrap().duration, s(4));
        // Clips that are not halves of the same split stay apart.
        let (mut p, a, _) = two_clips();
        assert!(!p.rejoin(a).unwrap());
    }

    #[test]
    fn rippled_delete_scopes() {
        let (mut p, a, b) = two_clips();
        let other = p.add_track(crate::model::TrackKind::Audio);
        let m = p.clip(a).unwrap().media;
        let c = p.insert_media(m, s(8), None, Some(other)).unwrap()[0];
        p.add_marker(s(5));
        let mut q = p.clone();
        q.delete_clips_rippled(&[a], RippleScope::AffectedTracks)
            .unwrap();
        assert_eq!(q.clip(b).unwrap().start, s(2));
        assert_eq!(q.clip(c).unwrap().start, s(8));
        assert_eq!(q.markers[0].time, s(5));
        let mut q = p.clone();
        q.delete_clips_rippled(&[a], RippleScope::AffectedTracksAndMarkers)
            .unwrap();
        assert_eq!(q.markers[0].time, s(1));
        assert_eq!(q.clip(c).unwrap().start, s(8));
        p.delete_clips_rippled(&[a], RippleScope::All).unwrap();
        assert_eq!(p.clip(c).unwrap().start, s(4));
        assert_eq!(p.remove_unused_media(), 0);
    }

    /// The Track Select tools: forward, every event reaching past the
    /// press (the one under it too); back, every event starting before it;
    /// on all tracks or one, groups whole unless left out.
    #[test]
    fn track_selection_picks_from_the_press_onward_or_back() {
        // A from 0 to 4 s and B from 6 to 10 s on the first track; on
        // another, C from 2 to 8 s grouped with B, and D from 9 s.
        let (mut p, a, b) = two_clips();
        let v = p.tracks[0].id;
        let other = p.add_track(crate::model::TrackKind::Audio);
        let m = p.clip(a).unwrap().media;
        let c = p.insert_media(m, s(2), None, Some(other)).unwrap()[0];
        p.trim_end(c, s(8)).unwrap();
        let d = p.insert_media(m, s(9), None, Some(other)).unwrap()[0];
        p.group_clips(&[b, c]);
        let sorted = |mut v: Vec<ClipId>| {
            v.sort();
            v
        };
        // Forward from 3 s, all tracks: A (still under it), B, C, D.
        assert_eq!(
            p.track_selection(s(3), None, false, true),
            sorted(vec![a, b, c, d])
        );
        // From 5 s: A ended before, C reaches past it.
        assert_eq!(
            p.track_selection(s(5), None, false, true),
            sorted(vec![b, c, d])
        );
        // Its track only (Shift): B, and C with it, its group.
        assert_eq!(
            p.track_selection(s(5), Some(v), false, true),
            sorted(vec![b, c])
        );
        // Its track only, groups left out (Alt+Shift): B alone.
        assert_eq!(p.track_selection(s(5), Some(v), false, false), vec![b]);
        // Back from 5 s: A and C start before; C's group brings B.
        assert_eq!(
            p.track_selection(s(5), None, true, true),
            sorted(vec![a, b, c])
        );
        assert_eq!(p.track_selection(s(5), Some(other), true, false), vec![c]);
    }

    /// The times of an envelope's points.
    fn times(e: &crate::envelope::Envelope) -> Vec<Tick> {
        e.points.iter().map(|p| p.at).collect()
    }

    /// An envelope of `kind` with points at 0 and at `at` (seconds).
    fn envelope(kind: crate::EnvelopeKind, at: &[i64]) -> crate::envelope::Envelope {
        let mut e = crate::envelope::Envelope::new(kind);
        for &t in at {
            e.points.push(crate::envelope::EnvelopePoint {
                at: s(t),
                ..e.points[0]
            });
        }
        e
    }

    /// The three ripples move envelopes and keyframes as VEGAS's: the
    /// affected tracks' always (those in the time that closed going with
    /// the events deleted); the bus tracks' — the video bus's, a bus's —
    /// with Affected Tracks, Bus Tracks, Markers, and Regions; the other
    /// tracks' with All Tracks, Markers, and Regions.
    #[test]
    fn ripples_move_envelopes_and_keyframes_by_scope() {
        use crate::EnvelopeKind as K;
        // A from 0 to 4 s and B from 6 s on the first track; another
        // track, a bus and the video bus with automation from 7 s.
        let (mut p, a, _) = two_clips();
        let v = p.tracks[0].id;
        let other = p.add_track(crate::model::TrackKind::Video);
        let bus = p.add_bus().unwrap();
        p.track_by_id_mut(v).unwrap().envelopes = vec![envelope(K::CompositeLevel, &[2, 5, 8])];
        p.track_by_id_mut(v).unwrap().motion = Some(crate::motion::TrackMotion {
            keys: [2, 7]
                .map(|t| {
                    crate::fx::TransformKey::new(
                        s(t),
                        crate::fx::Transform::default(),
                        crate::fx::KeyCurve::Linear,
                    )
                })
                .to_vec(),
            ..Default::default()
        });
        p.track_by_id_mut(other).unwrap().envelopes = vec![envelope(K::CompositeLevel, &[7])];
        p.bus_mut(bus).unwrap().channel.envelopes = vec![envelope(K::Volume, &[7])];
        p.video_bus.envelopes = vec![envelope(K::FadeToColor, &[7])];
        let track = |p: &Project, id| times(&p.track(id).unwrap().envelopes[0]);
        let bus_times = |p: &Project| times(&p.bus(bus).unwrap().channel.envelopes[0]);
        let video_bus = |p: &Project| times(&p.video_bus.envelopes[0]);
        let zero = Tick::ZERO;

        // Affected Tracks: the first track's (2 s went with A, the hole
        // of 4 s closed), no other.
        let mut q = p.clone();
        q.delete_clips_rippled(&[a], RippleScope::AffectedTracks)
            .unwrap();
        assert_eq!(track(&q, v), vec![zero, s(1), s(4)]);
        let keys: Vec<Tick> = q
            .track(v)
            .unwrap()
            .motion
            .as_ref()
            .unwrap()
            .keys
            .iter()
            .map(|k| k.at)
            .collect();
        assert_eq!(keys, vec![s(3)], "Track Motion's keyframes too");
        assert_eq!(track(&q, other), vec![zero, s(7)]);
        assert_eq!(bus_times(&q), vec![zero, s(7)]);
        assert_eq!(video_bus(&q), vec![zero, s(7)]);
        // Affected Tracks, Bus Tracks, Markers, and Regions: the bus
        // tracks' too.
        let mut q = p.clone();
        q.delete_clips_rippled(&[a], RippleScope::AffectedTracksAndMarkers)
            .unwrap();
        assert_eq!(track(&q, v), vec![zero, s(1), s(4)]);
        assert_eq!(track(&q, other), vec![zero, s(7)]);
        assert_eq!(bus_times(&q), vec![zero, s(3)]);
        assert_eq!(video_bus(&q), vec![zero, s(3)]);
        // All Tracks, Markers, and Regions: every track's.
        let mut q = p.clone();
        q.delete_clips_rippled(&[a], RippleScope::All).unwrap();
        assert_eq!(track(&q, other), vec![zero, s(3)]);
        assert_eq!(bus_times(&q), vec![zero, s(3)]);

        // A post-edit ripple after A grew to 5 s: B and what follows on
        // its track come 1 s earlier, the point in the second it moved
        // back over goes.
        let mut q = p.clone();
        q.track_by_id_mut(v).unwrap().envelopes = vec![envelope(K::CompositeLevel, &[2, 5, 6, 8])];
        let _ = q.trim_end(a, s(5));
        q.post_edit_ripple(a, RippleScope::AffectedTracks).unwrap();
        assert_eq!(track(&q, v), vec![zero, s(2), s(5), s(7)]);
        assert_eq!(bus_times(&q), vec![zero, s(7)]);
        let _ = q.trim_end(a, s(4));
        q.post_edit_ripple(a, RippleScope::AffectedTracksAndMarkers)
            .unwrap();
        assert_eq!(bus_times(&q), vec![zero, s(6)]);
        assert_eq!(video_bus(&q), vec![zero, s(6)]);
        p.check_invariants().unwrap();
    }

    #[test]
    fn a_rippled_delete_moves_markers_and_other_tracks_as_far_as_its_track() {
        // A (0 to 10 s) crossfades into B (8 to 15 s) and C follows at
        // 15 s, where another track's event starts too.
        let mut p = Project::new(ProjectSettings::default());
        let v = p.tracks[0].id;
        let m = p.add_media(media(60));
        let place = |p: &mut Project, from: i64, to: i64| {
            let id = p.insert_media(m, s(from), Some(v), None).unwrap()[0];
            p.trim_end(id, s(to)).unwrap();
            id
        };
        let a = place(&mut p, 0, 10);
        let b = place(&mut p, 8, 15);
        let c = place(&mut p, 15, 20);
        let other = p.add_track(crate::model::TrackKind::Audio);
        let sound = p.insert_media(m, s(15), None, Some(other)).unwrap()[0];
        for at in [4, 9, 15] {
            p.add_marker(s(at));
        }
        p.delete_clips_rippled(&[a], RippleScope::All).unwrap();
        // The hole closes up to B, 8 s: what was from there on comes 8 s
        // earlier, on every track.
        assert_eq!(p.clip(b).unwrap().start, Tick::ZERO);
        assert_eq!(p.clip(c).unwrap().start, s(7));
        assert_eq!(p.clip(sound).unwrap().start, s(7));
        let marks: Vec<Tick> = p.markers.iter().map(|m| m.time).collect();
        assert_eq!(marks, vec![Tick::ZERO, s(1), s(7)]);
        p.check_invariants().unwrap();
    }

    #[test]
    fn cleaning_the_project_media_takes_what_named_the_media_too() {
        use crate::storyboards::StoryCell;
        let (mut p, a, _) = two_clips();
        let used = p.clip(a).unwrap().media;
        let unused = p.add_media(media(5));
        let parent = p.add_media(media(8));
        // The unused media in a bin, tagged and on a storyboard; another
        // with no event either, but a subclip.
        let bin = p.add_bin(None, "Shots").unwrap();
        p.add_to_bin(bin, used).unwrap();
        p.add_to_bin(bin, unused).unwrap();
        assert!(p.add_tag(unused, "beach"));
        let board = p.add_storyboard("Cut");
        p.storyboard_drop(board, 0, StoryCell::of(unused, None), true);
        p.storyboard_drop(board, 1, StoryCell::of(used, None), true);
        let part = p
            .add_subclip(parent, TimeRange::new(s(1), s(2)), false, "part")
            .unwrap();
        assert_eq!(p.remove_unused_media(), 1);
        assert!(p.media(unused).is_none());
        assert!(p.media(parent).is_some(), "a subclip's media stays");
        assert!(p.subclip(part).is_some());
        assert_eq!(p.bin(bin).unwrap().media, vec![used]);
        assert!(p.media_tags.iter().all(|t| t.media != unused));
        let cells: Vec<_> = p.storyboard(board).unwrap().cells.to_vec();
        assert_eq!(cells, vec![StoryCell::of(used, None)]);
        assert_eq!(p.remove_unused_media(), 0);
        p.check_invariants().unwrap();
    }

    #[test]
    fn slip_moves_media_inside_the_clip() {
        let (mut p, _, b) = two_clips();
        // b shows media 0–4 s; with Loop it slips freely (the media
        // repeats), without it slipping right is limited by the media start.
        assert_eq!(p.slip_clip(b, s(1)).unwrap(), s(1));
        assert_eq!(p.clip(b).unwrap().source_in, -s(1));
        p.slip_clip(b, -s(1)).unwrap();
        p.clip_mut(b).unwrap().looped = false;
        assert_eq!(p.slip_clip(b, s(1)).unwrap(), Tick::ZERO);
        assert_eq!(p.slip_clip(b, -s(2)).unwrap(), -s(2));
        assert_eq!(p.clip(b).unwrap().source_in, s(2));
        assert_eq!(p.clip(b).unwrap().start, s(6), "the clip itself stays");
    }

    #[test]
    fn slide_keeps_neighbours_attached() {
        let (mut p, a, b) = two_clips();
        // Make the clips touch: a ends at 6 s, where b starts.
        let _ = p.trim_end(a, s(6));
        let c = {
            let m = p.clip(a).unwrap().media;
            let v = p.tracks[0].id;
            let c = p.insert_media(m, s(10), Some(v), None).unwrap()[0];
            let _ = p.trim_end(c, s(14));
            c
        };
        let d = p.slide_clip(b, s(1)).unwrap();
        assert_eq!(d, s(1));
        assert_eq!(p.clip(a).unwrap().end(), s(7));
        assert_eq!(p.clip(b).unwrap().start, s(7));
        assert_eq!(p.clip(c).unwrap().start, s(11));
        assert_eq!(p.clip(c).unwrap().end(), s(14));
        p.check_invariants().unwrap();
    }

    #[test]
    fn smart_split_slides_the_parts_left_over_each_other() {
        // One 10 s event: taking out 4–6 s leaves 0–4 s, and the part from
        // 6 s slides back over it by 1 s.
        let mut p = Project::new(ProjectSettings::default());
        let v = p.tracks[0].id;
        let m = p.add_media(media(20));
        let a = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        let _ = p.trim_end(a, s(10));
        let warp = crate::fx::PluginFx {
            id: "org.bananav.WarpFlow".into(),
            version: [1, 0],
            name: "Warp Flow".into(),
            params: Default::default(),
        };
        let removed = p.smart_split(TimeRange::new(s(4), s(2)), &[], true, s(1), Some(&warp));
        assert_eq!(removed, 1);
        let clips = &p.tracks[0].clips;
        assert_eq!(clips.len(), 2);
        let (l, r) = (&clips[0], &clips[1]);
        assert_eq!((l.start, l.end()), (Tick::ZERO, s(4)));
        assert_eq!(
            (r.start, r.end()),
            (s(3), s(7)),
            "slid back over the left part"
        );
        assert_eq!(r.source_in, s(6), "it starts with what followed the part");
        assert!(l.group.is_some() && l.group == r.group, "grouped");
        assert_eq!((l.fade_out, r.fade_in), (s(1), s(1)), "a crossfade");
        assert_eq!(
            r.transition_fx.as_deref(),
            Some(&warp),
            "Warp Flow on the video track"
        );
        // A part with no event in it: nothing slides.
        let removed = p.smart_split(TimeRange::new(s(15), s(1)), &[], true, s(1), None);
        assert_eq!(removed, 0);
        assert_eq!(p.tracks[0].clips[1].start, s(3));
    }

    #[test]
    fn smart_split_keeps_the_fades_within_short_parts() {
        // A 10 s event fading in and out over 1 s; all of it taken out but
        // a tenth of a second at each end, 5 frames of overlap asked for.
        let mut p = Project::new(ProjectSettings::default());
        let v = p.tracks[0].id;
        let m = p.add_media(media(20));
        let a = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        let _ = p.trim_end(a, s(10));
        p.set_fades(a, Some(s(1)), Some(s(1))).unwrap();
        let tenth = Tick(s(1).0 / 10);
        let overlap = Tick(p.min_clip_duration().0 * 5);
        let cut = TimeRange::from_bounds(tenth, s(10) - tenth);
        assert_eq!(p.smart_split(cut, &[], true, overlap, None), 1);
        p.check_invariants().unwrap();
        // The parts overlap whole: the crossfade is all of each, nothing
        // left of the fades they had.
        let clips = &p.tracks[0].clips;
        let (l, r) = (&clips[0], &clips[1]);
        assert_eq!((l.duration, r.duration), (tenth, tenth));
        assert_eq!((l.fade_in, l.fade_out), (Tick::ZERO, tenth));
        assert_eq!((r.fade_in, r.fade_out), (tenth, Tick::ZERO));
        // Longer parts keep what fits beside the crossfade.
        let mut p = Project::new(ProjectSettings::default());
        let v = p.tracks[0].id;
        let m = p.add_media(media(20));
        let a = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        let _ = p.trim_end(a, s(10));
        p.set_fades(a, Some(s(3)), None).unwrap();
        p.smart_split(TimeRange::new(s(2), s(4)), &[], true, s(1), None);
        let l = &p.tracks[0].clips[0];
        assert_eq!((l.duration, l.fade_in, l.fade_out), (s(2), s(1), s(1)));
        p.check_invariants().unwrap();
    }

    #[test]
    fn a_slip_trimmed_edge_keeps_its_frame() {
        // The second event shows 2 s–6 s of its media from 6 s to 10 s
        // (without Loop: no media before it).
        let setup = || {
            let (mut p, _, b) = two_clips();
            p.clip_mut(b).unwrap().looped = false;
            p.slip_clip(b, -s(2)).unwrap();
            (p, b)
        };
        let span = |p: &Project, b| {
            let c = p.clip(b).unwrap();
            (c.start, c.end(), c.source_in)
        };
        // The end goes on a second: the media comes along, the start stays.
        let (mut p, b) = setup();
        assert_eq!(p.slip_trim(b, true, s(11)).unwrap(), s(1));
        assert_eq!(span(&p, b), (s(6), s(11), s(1)));
        // No more than the media before it.
        let (mut p, b) = setup();
        assert_eq!(p.slip_trim(b, true, s(13)).unwrap(), s(2));
        assert_eq!(span(&p, b), (s(6), s(12), Tick::ZERO));
        // Shorter: the end still shows 6 s of the media.
        let (mut p, b) = setup();
        assert_eq!(p.slip_trim(b, true, s(9)).unwrap(), -s(1));
        assert_eq!(span(&p, b), (s(6), s(9), s(3)));
        // The start: it keeps showing 2 s of the media, the end stays.
        let (mut p, b) = setup();
        assert_eq!(p.slip_trim(b, false, s(5)).unwrap(), -s(1));
        assert_eq!(span(&p, b), (s(5), s(10), s(2)));
        let (mut p, b) = setup();
        assert_eq!(p.slip_trim(b, false, s(8)).unwrap(), s(2));
        assert_eq!(span(&p, b), (s(8), s(10), s(2)));
        // Never shorter than a frame.
        let (mut p, b) = setup();
        p.slip_trim(b, false, s(20)).unwrap();
        let frame = p.min_clip_duration();
        assert_eq!(span(&p, b), (s(10) - frame, s(10), s(2)));
    }

    #[test]
    fn trimming_adjacent_edges_opens_no_gap() {
        // The second event right after the first, from 2 s of its media
        // (without Loop).
        let setup = || {
            let (mut p, a, b) = two_clips();
            p.clip_mut(b).unwrap().looped = false;
            p.slip_clip(b, -s(2)).unwrap();
            p.move_clip(b, s(4), None).unwrap();
            (p, a, b)
        };
        let edges = |p: &Project, a, b| (p.clip(a).unwrap().end(), p.clip(b).unwrap().start);
        let (mut p, a, b) = setup();
        assert_eq!(p.trim_adjacent(a, true, s(5)).unwrap(), s(1));
        assert_eq!(edges(&p, a, b), (s(5), s(5)));
        assert_eq!(p.clip(b).unwrap().end(), s(8), "its other edge stays");
        // From the second event's start, as far as its media allows.
        let (mut p, a, b) = setup();
        assert_eq!(p.trim_adjacent(b, false, s(1)).unwrap(), -s(2));
        assert_eq!(edges(&p, a, b), (s(2), s(2)));
        // A crossfade keeps its length.
        let (mut p, a, b) = setup();
        p.trim_end(a, s(5)).unwrap();
        p.trim_adjacent(a, true, s(6)).unwrap();
        assert_eq!(edges(&p, a, b), (s(6), s(5)));
        // With nothing beside it, a plain trim.
        let (mut p, a, b) = two_clips();
        assert_eq!(p.trim_adjacent(a, true, s(5)).unwrap(), s(1));
        assert_eq!(edges(&p, a, b), (s(5), s(6)));
    }

    #[test]
    fn a_cut_becomes_an_overlap_before_around_or_after_it() {
        // Two events cut at 4 s: the first from 0 s of a 20 s media, the
        // second from 10 s.
        let setup = || {
            let mut p = Project::new(ProjectSettings::default());
            let v = p.tracks[0].id;
            let m = p.add_media(media(20));
            let a = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
            let _ = p.trim_end(a, s(4));
            let b = p
                .insert_media_range(m, s(4), s(10), s(4), Some(v), None)
                .unwrap()[0];
            (p, a, b)
        };
        let half = Tick(s(1).0 / 2);
        // Centered on the cut: each grows half a second over it.
        let (mut p, a, b) = setup();
        assert_eq!(p.overlap_cut(b, half, half).unwrap(), s(1));
        assert_eq!(p.clip(a).unwrap().end(), s(4) + half);
        assert_eq!(p.clip(b).unwrap().start, s(4) - half);
        assert_eq!(p.clip(b).unwrap().source_in, s(10) - half);
        // Before the cut: only the second event grows back.
        let (mut p, a, b) = setup();
        p.overlap_cut(b, s(1), Tick::ZERO).unwrap();
        let ends = (p.clip(a).unwrap().end(), p.clip(b).unwrap().start);
        assert_eq!(ends, (s(4), s(3)));
        // After it: only the first one grows on.
        let (mut p, a, b) = setup();
        p.overlap_cut(b, Tick::ZERO, s(1)).unwrap();
        let ends = (p.clip(a).unwrap().end(), p.clip(b).unwrap().start);
        assert_eq!(ends, (s(5), s(4)));
        // No media before the second event (without Loop): it cannot grow.
        let (mut p, _, b) = setup();
        let c = p.clip_mut(b).unwrap();
        (c.source_in, c.looped) = (Tick::ZERO, false);
        assert_eq!(p.overlap_cut(b, s(1), Tick::ZERO).unwrap(), Tick::ZERO);
        // Not a cut: no event ends where the first one starts.
        let (mut p, a, _) = setup();
        assert_eq!(p.overlap_cut(a, s(1), s(1)).unwrap(), Tick::ZERO);
    }
}
