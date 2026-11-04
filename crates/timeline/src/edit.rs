//! Operações de edição. Todas mantêm as invariantes de [`Project::check_invariants`].
//!
//! Arrastos do mouse geram valores fora dos limites o tempo todo, então mover e
//! aparar *limitam* o valor pedido em vez de falhar, e devolvem o que aplicaram.

use std::collections::HashMap;

use bv_core::{Tick, TimeRange};

use crate::model::{
    ChannelMap, Clip, ClipId, GroupId, MediaId, MediaInfo, MediaSource, NewEvents, Project,
    TRACK_PALETTE, Track, TrackId, TrackKind,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditError {
    UnknownClip(ClipId),
    UnknownTrack(TrackId),
    UnknownMedia(MediaId),
    /// Trilha de vídeo onde se esperava áudio, ou vice-versa.
    WrongTrackKind,
    /// A mídia não tem nenhum stream que caiba nas trilhas indicadas.
    NothingToInsert,
    /// O instante não está estritamente dentro de nenhum clipe do grupo.
    NotInsideClip,
    UnknownBin(crate::bins::BinId),
    /// The event has no entrance transition (a fade-in or a crossfade).
    NoTransition,
    /// The event has no such take (or only one where two are needed).
    NoTake,
    /// Tracks to group must be two or more, next to one another.
    TracksNotAdjacent,
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::UnknownClip(id) => write!(f, "clip {id:?} does not exist"),
            EditError::UnknownTrack(id) => write!(f, "track {id:?} does not exist"),
            EditError::UnknownMedia(id) => write!(f, "media {id:?} does not exist"),
            EditError::WrongTrackKind => f.write_str("incompatible track type"),
            EditError::NothingToInsert => f.write_str("the media has no stream for these tracks"),
            EditError::NotInsideClip => f.write_str("the time is not inside the clip"),
            EditError::UnknownBin(id) => write!(f, "media bin {id:?} does not exist"),
            EditError::NoTransition => f.write_str("the event has no transition"),
            EditError::NoTake => f.write_str("the event has no such take"),
            EditError::TracksNotAdjacent => {
                f.write_str("only two or more adjacent tracks can be grouped")
            }
        }
    }
}

impl std::error::Error for EditError {}

type Result<T> = std::result::Result<T, EditError>;

impl Project {
    /// Menor duração permitida para um clipe: um quadro do projeto.
    pub fn min_clip_duration(&self) -> Tick {
        self.settings.frame_rate.frame_duration()
    }

    /// Registra uma mídia no projeto. O `id` de `info` é ignorado e substituído.
    pub fn add_media(&mut self, mut info: MediaInfo) -> MediaId {
        let id = MediaId(self.alloc_id());
        info.id = id;
        self.media.push(info);
        id
    }

    /// VEGAS's "Create a new copy of the source media": event `id`, of
    /// generated media, gets a copy of that media of its own, to be edited
    /// apart (its takes of it too). Returns the copy (`None`: not
    /// generated media, or an adjustment event's).
    pub fn copy_generated_media(&mut self, id: ClipId) -> Option<MediaId> {
        let old = self.clip(id)?.media;
        let info = self
            .media(old)
            .filter(|m| matches!(m.source, MediaSource::Generated(_)) && !m.is_adjustment())?
            .clone();
        let copy = self.add_media(info);
        let c = self.clip_mut(id)?;
        c.media = copy;
        for t in c.takes.iter_mut().filter(|t| t.media == old) {
            t.media = copy;
        }
        Some(copy)
    }

    /// Remove a mídia e todos os clipes que a usam.
    pub fn remove_media(&mut self, id: MediaId) -> Result<()> {
        let idx = self
            .media
            .iter()
            .position(|m| m.id == id)
            .ok_or(EditError::UnknownMedia(id))?;
        self.media.remove(idx);
        // Events with other takes play another; the others go.
        self.drop_takes_of(id);
        for t in &mut self.tracks {
            t.clips.retain(|c| c.media != id);
        }
        for b in &mut self.bins {
            b.media.retain(|&m| m != id);
        }
        self.forget_tags(id);
        self.forget_in_storyboards(id);
        // Its subclips go with it.
        let subclips: Vec<MediaId> = self
            .subclips
            .iter()
            .filter(|s| s.parent == id)
            .map(|s| s.id)
            .collect();
        for s in subclips {
            self.remove_subclip(s);
        }
        Ok(())
    }

    /// A new track: a video one below the last video track, an audio one
    /// at the end; its color the palette's by its place, as VEGAS gives
    /// new tracks theirs ([`Project::new_track_color`]).
    pub fn add_track(&mut self, kind: TrackKind) -> TrackId {
        let id = TrackId(self.alloc_id());
        let count = self.tracks.iter().filter(|t| t.kind == kind).count();
        let name = match kind {
            TrackKind::Video => format!("Video {}", count + 1),
            TrackKind::Audio => format!("Audio {}", count + 1),
        };
        let track = Track {
            id,
            kind,
            name,
            muted: false,
            solo: false,
            level: 1.0,
            pan: 0.0,
            pan_mode: Default::default(),
            composite_mode: Default::default(),
            parent_mode: Default::default(),
            compositor: None,
            parent_compositor: None,
            composite_depth: 0,
            adjustment: false,
            motion: None,
            parent_motion: None,
            fade_colors: None,
            effects: Vec::new(),
            composite_at: None,
            clips: Vec::new(),
            envelopes: Vec::new(),
            invert_phase: false,
            bypass_motion_blur: false,
            armed: false,
            record_input: None,
            input_monitor: Default::default(),
            automation: Default::default(),
            surround: None,
            group: None,
            color: None,
            ducking: None,
            duck_listener: false,
            output: Default::default(),
            sends: Vec::new(),
        };
        let at = self.new_track_place(kind);
        self.tracks.insert(at, track);
        self.color_by_place(id);
        self.normalize_groups();
        id
    }

    /// Where [`Project::add_track`] puts a new track of `kind`.
    fn new_track_place(&self, kind: TrackKind) -> usize {
        match kind {
            TrackKind::Video => self
                .tracks
                .iter()
                .rposition(|t| t.kind == TrackKind::Video)
                .map_or(0, |i| i + 1),
            TrackKind::Audio => self.tracks.len(),
        }
    }

    /// The palette's color a new track of `kind` gets (its index).
    pub fn new_track_color(&self, kind: TrackKind) -> usize {
        self.new_track_place(kind) % TRACK_PALETTE as usize
    }

    /// Gives track `id` the palette's color of its place (none, the
    /// kind's, when it is that one).
    fn color_by_place(&mut self, id: TrackId) {
        let Some(i) = self.track_index(id) else {
            return;
        };
        let t = &mut self.tracks[i];
        let color = (i % TRACK_PALETTE as usize) as u8;
        let kinds = match t.kind {
            TrackKind::Video => 0,
            TrackKind::Audio => 1,
        };
        t.color = (color != kinds).then_some(color);
    }

    /// The video tracks, top first.
    fn video_order(&self) -> Vec<usize> {
        (0..self.tracks.len())
            .filter(|&i| self.tracks[i].kind == TrackKind::Video)
            .collect()
    }

    /// Keeps compositing depths possible: the top video track is no child,
    /// and a child is at most one in from the video track above it.
    pub(crate) fn normalize_compositing(&mut self) {
        let mut above: Option<u8> = None;
        for i in self.video_order() {
            let t = &mut self.tracks[i];
            t.composite_depth = match above {
                None => 0,
                Some(d) => t.composite_depth.min(d + 1),
            };
            above = Some(t.composite_depth);
        }
        for t in &mut self.tracks {
            if t.kind == TrackKind::Audio {
                t.composite_depth = 0;
            }
        }
    }

    /// The video tracks `track` holds as a compositing parent, itself
    /// first: those right below it that are farther in.
    pub fn compositing_family(&self, track: TrackId) -> Vec<TrackId> {
        let order = self.video_order();
        let Some(at) = order.iter().position(|&i| self.tracks[i].id == track) else {
            return Vec::new();
        };
        let depth = self.tracks[order[at]].composite_depth;
        let mut out = vec![track];
        for &i in &order[at + 1..] {
            if self.tracks[i].composite_depth <= depth {
                break;
            }
            out.push(self.tracks[i].id);
        }
        out
    }

    /// Whether video track `track` is a compositing parent (it has
    /// children).
    pub fn is_compositing_parent(&self, track: TrackId) -> bool {
        self.compositing_family(track).len() > 1
    }

    /// Whether Make Compositing Child can take `track` in: a video track
    /// above it is as far in as it is, or farther.
    pub fn can_make_child(&self, track: TrackId) -> bool {
        let order = self.video_order();
        let Some(at) = order.iter().position(|&i| self.tracks[i].id == track) else {
            return false;
        };
        at > 0
            && self.tracks[order[at - 1]].composite_depth >= self.tracks[order[at]].composite_depth
    }

    /// Make Compositing Child: `track` one level in (with `family`, Ctrl's,
    /// its children with it), as VEGAS's.
    pub fn make_compositing_child(&mut self, track: TrackId, family: bool) -> Result<()> {
        if !self.can_make_child(track) {
            return Ok(());
        }
        let moving = if family {
            self.compositing_family(track)
        } else {
            vec![track]
        };
        for id in moving {
            if let Some(t) = self.track_by_id_mut(id) {
                t.composite_depth = t.composite_depth.saturating_add(1);
            }
        }
        self.normalize_compositing();
        Ok(())
    }

    /// Make Compositing Parent: `track` one level out (with `family`, its
    /// children with it); children left too far in come out with it.
    pub fn make_compositing_parent(&mut self, track: TrackId, family: bool) -> Result<()> {
        let moving = if family {
            self.compositing_family(track)
        } else {
            vec![track]
        };
        for id in moving {
            if let Some(t) = self.track_by_id_mut(id) {
                t.composite_depth = t.composite_depth.saturating_sub(1);
            }
        }
        self.normalize_compositing();
        Ok(())
    }

    /// Insert › Adjustment Track: a new adjustment track at position
    /// `index` (0 = top).
    pub fn insert_adjustment_track_at(&mut self, index: usize) -> TrackId {
        let id = self.insert_track_at(TrackKind::Video, index);
        let n = self.tracks.iter().filter(|t| t.adjustment).count() + 1;
        if let Some(t) = self.track_by_id_mut(id) {
            t.adjustment = true;
            t.name = format!("Adjustment {n}");
        }
        id
    }

    /// Resize … Adjustment Events to Project: event `id`, an adjustment
    /// event, from the timeline's start to the project's end.
    pub fn resize_adjustment_to_project(&mut self, id: ClipId) -> Result<()> {
        let end = self.duration();
        let adjustment = self
            .clip(id)
            .and_then(|c| self.media(c.media))
            .is_some_and(|m| m.is_adjustment());
        if !adjustment || end <= Tick::ZERO {
            return Ok(());
        }
        let c = self.clip_mut(id).ok_or(EditError::UnknownClip(id))?;
        if c.locked {
            return Ok(());
        }
        c.start = Tick::ZERO;
        c.duration = end;
        clamp_fades(c);
        self.sort_all();
        Ok(())
    }

    /// A new track of `kind` at position `index` (0 = top): Insert › Video
    /// Track's at the top, a track header's Insert … Track's above it, as
    /// VEGAS's.
    pub fn insert_track_at(&mut self, kind: TrackKind, index: usize) -> TrackId {
        let id = self.add_track(kind);
        let _ = self.move_track_to(id, index);
        self.color_by_place(id);
        id
    }

    /// Moves the track so that it ends up at position `index` (0 = top),
    /// like dragging its header in VEGAS's track list: between two tracks
    /// of a group it joins that group, elsewhere it is in none.
    pub fn move_track_to(&mut self, id: TrackId, index: usize) -> Result<()> {
        let from = self.track_index(id).ok_or(EditError::UnknownTrack(id))?;
        let track = self.tracks.remove(from);
        let to = index.min(self.tracks.len());
        self.tracks.insert(to, track);
        let above = to
            .checked_sub(1)
            .and_then(|i| self.tracks.get(i))
            .and_then(|t| t.group);
        let below = self.tracks.get(to + 1).and_then(|t| t.group);
        self.tracks[to].group = above.filter(|g| Some(*g) == below);
        self.normalize_compositing();
        self.normalize_groups();
        Ok(())
    }

    /// A copy of track `id` — its settings, events and envelopes — right
    /// below it (Duplicate Track). The copies of its events are grouped
    /// with nothing.
    pub fn duplicate_track(&mut self, id: TrackId) -> Result<TrackId> {
        let i = self.track_index(id).ok_or(EditError::UnknownTrack(id))?;
        let mut t = self.tracks[i].clone();
        t.id = TrackId(self.alloc_id());
        t.armed = false;
        for c in &mut t.clips {
            c.id = ClipId(self.alloc_id());
            c.group = None;
        }
        let new = t.id;
        self.tracks.insert(i + 1, t);
        self.normalize_groups();
        Ok(new)
    }

    pub fn remove_track(&mut self, id: TrackId) -> Result<()> {
        let idx = self.track_index(id).ok_or(EditError::UnknownTrack(id))?;
        self.tracks.remove(idx);
        self.normalize_groups();
        Ok(())
    }

    /// Gives the track one of the palette's colors (`None`: its kind's).
    pub fn set_track_color(&mut self, id: TrackId, color: Option<u8>) -> Result<()> {
        self.track_mut(id)?.color = color.map(|c| c % TRACK_PALETTE);
        Ok(())
    }

    pub fn set_track_muted(&mut self, id: TrackId, muted: bool) -> Result<()> {
        self.track_mut(id)?.muted = muted;
        Ok(())
    }

    pub fn set_track_solo(&mut self, id: TrackId, solo: bool) -> Result<()> {
        self.track_mut(id)?.solo = solo;
        Ok(())
    }

    pub fn set_clip_level(&mut self, id: ClipId, level: f32) -> Result<()> {
        let (t, c) = self.find_clip(id).ok_or(EditError::UnknownClip(id))?;
        self.tracks[t].clips[c].level = level.max(0.0);
        Ok(())
    }

    /// The curve of the event's fade-in (`out`: its fade-out), as VEGAS's
    /// popup of curves sets it: that event's alone.
    pub fn set_fade_curve(
        &mut self,
        id: ClipId,
        out: bool,
        curve: crate::fx::FadeCurve,
    ) -> Result<()> {
        let (t, c) = self.find_clip(id).ok_or(EditError::UnknownClip(id))?;
        let clip = &mut self.tracks[t].clips[c];
        if out {
            clip.fade_out_curve = curve;
        } else {
            clip.fade_in_curve = curve;
        }
        Ok(())
    }

    pub(crate) fn track_mut(&mut self, id: TrackId) -> Result<&mut Track> {
        self.tracks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or(EditError::UnknownTrack(id))
    }

    /// Coloca a mídia inteira na timeline em `at`. Cria um clipe para cada
    /// stream com trilha indicada; se forem dois, eles formam um grupo.
    pub fn insert_media(
        &mut self,
        media: MediaId,
        at: Tick,
        video_track: Option<TrackId>,
        audio_track: Option<TrackId>,
    ) -> Result<Vec<ClipId>> {
        self.insert_media_as(media, at, video_track, audio_track, &NewEvents::default())
    }

    /// [`Self::insert_media`] with the editor's settings for new events:
    /// their Loop switch, their pitch when stretched and the length of
    /// still images.
    pub fn insert_media_as(
        &mut self,
        media: MediaId,
        at: Tick,
        video_track: Option<TrackId>,
        audio_track: Option<TrackId>,
        new: &NewEvents,
    ) -> Result<Vec<ClipId>> {
        let info = self
            .media(media)
            .ok_or(EditError::UnknownMedia(media))?
            .clone();
        let mut targets = Vec::new();
        for (track, kind) in [
            (video_track, TrackKind::Video),
            (audio_track, TrackKind::Audio),
        ] {
            let Some(track) = track else { continue };
            let t = self.track(track).ok_or(EditError::UnknownTrack(track))?;
            if t.kind != kind {
                return Err(EditError::WrongTrackKind);
            }
            if info.has(kind) {
                targets.push(track);
            }
        }
        let duration = new.length(&info);
        if targets.is_empty() || duration <= Tick::ZERO {
            return Err(EditError::NothingToInsert);
        }
        // The media's other audio streams go to the audio tracks below the
        // first one, one each (new tracks when there are not enough), like
        // VEGAS's: a recording with the voice and the game on separate
        // streams keeps them apart.
        let mut targets: Vec<(TrackId, u16)> = targets.into_iter().map(|t| (t, 0)).collect();
        let first_audio = targets
            .iter()
            .map(|&(t, _)| t)
            .find(|&t| self.track(t).is_some_and(|t| t.kind == TrackKind::Audio));
        if let Some(first) = first_audio {
            for k in 1..=info.extra_audio.len() {
                let audio: Vec<TrackId> = self
                    .tracks
                    .iter()
                    .filter(|t| t.kind == TrackKind::Audio)
                    .map(|t| t.id)
                    .collect();
                let pos = audio.iter().position(|&t| t == first).unwrap_or(0);
                let track = match audio.get(pos + k) {
                    Some(&t) => t,
                    None => self.add_track(TrackKind::Audio),
                };
                targets.push((track, k as u16));
            }
        }
        let group = (targets.len() > 1).then(|| GroupId(self.alloc_id()));
        let mut ids = Vec::new();
        for (track, stream) in targets {
            let id = ClipId(self.alloc_id());
            let t = self.track_mut(track)?;
            let fades = match t.kind {
                TrackKind::Audio => new.audio_fades,
                TrackKind::Video => new.video_fades,
            };
            t.clips.push(Clip {
                group,
                audio_stream: stream,
                looped: new.looped,
                preserve_pitch: new.preserve_pitch,
                // The fade-out the same shape turned around (a Fast fade-in,
                // a Slow fade-out: VEGAS's dome).
                fade_in_curve: fades,
                fade_out_curve: fades.out_shape(),
                ..Clip::new(id, media, at.max(Tick::ZERO), duration)
            });
            t.sort();
            ids.push(id);
        }
        Ok(ids)
    }

    /// Splits a stereo audio event into two mono ones ("Import stereo as dual
    /// mono"): the event keeps the left channel, and a copy of it on the
    /// next audio track (a new one at the end when there is none) takes the
    /// right; they stay grouped with the event's others. Returns the copy and
    /// the two tracks.
    pub fn split_dual_mono(&mut self, id: ClipId) -> Result<(ClipId, TrackId, TrackId)> {
        let (ti, ci) = self.find_clip(id).ok_or(EditError::UnknownClip(id))?;
        let left = self.tracks[ti].id;
        if self.tracks[ti].kind != TrackKind::Audio {
            return Err(EditError::WrongTrackKind);
        }
        let audio: Vec<TrackId> = self
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
            .collect();
        let pos = audio.iter().position(|&t| t == left).unwrap_or(0);
        let right = match audio.get(pos + 1) {
            Some(&t) => t,
            None => self.add_track(TrackKind::Audio),
        };
        let group = match self.tracks[ti].clips[ci].group {
            Some(g) => g,
            None => GroupId(self.alloc_id()),
        };
        let copy_id = ClipId(self.alloc_id());
        let (ti, ci) = self.find_clip(id).ok_or(EditError::UnknownClip(id))?;
        let clip = &mut self.tracks[ti].clips[ci];
        clip.group = Some(group);
        clip.channels = ChannelMap::LeftOnly;
        let copy = Clip {
            id: copy_id,
            channels: ChannelMap::RightOnly,
            ..clip.clone()
        };
        let t = self.track_mut(right)?;
        t.clips.push(copy);
        t.sort();
        Ok((copy_id, left, right))
    }

    /// Coloca um trecho da mídia (`source_in` .. `source_in + duration`) na
    /// timeline em `at` (usado pela Unidade de corte).
    pub fn insert_media_range(
        &mut self,
        media: MediaId,
        at: Tick,
        source_in: Tick,
        duration: Tick,
        video_track: Option<TrackId>,
        audio_track: Option<TrackId>,
    ) -> Result<Vec<ClipId>> {
        let new = NewEvents::default();
        self.insert_media_range_as(
            media,
            at,
            source_in,
            duration,
            (video_track, audio_track),
            &new,
        )
    }

    /// [`Self::insert_media_range`] with the editor's settings for new
    /// events (see [`Self::insert_media_as`]); `tracks`: the video and the
    /// audio track.
    pub fn insert_media_range_as(
        &mut self,
        media: MediaId,
        at: Tick,
        source_in: Tick,
        duration: Tick,
        (video_track, audio_track): (Option<TrackId>, Option<TrackId>),
        new: &NewEvents,
    ) -> Result<Vec<ClipId>> {
        let info = self.media(media).ok_or(EditError::UnknownMedia(media))?;
        let source_in = source_in.clamp(Tick::ZERO, info.duration);
        let duration = duration.min(info.duration - source_in);
        if duration < self.min_clip_duration() {
            return Err(EditError::NothingToInsert);
        }
        let ids = self.insert_media_as(media, at, video_track, audio_track, new)?;
        for &id in &ids {
            let c = self.clip_mut(id).expect("acabou de ser criado");
            c.source_in = source_in;
            c.duration = duration;
        }
        Ok(ids)
    }

    /// Muda a velocidade do clipe (e do grupo) mantendo o mesmo trecho da
    /// mídia: o clipe encurta ao acelerar e alonga ao desacelerar.
    pub fn set_clip_rate(&mut self, id: ClipId, rate: f64) -> Result<()> {
        let rate = rate.clamp(0.05, 20.0);
        let min_dur = self.min_clip_duration();
        for (t, c) in self.member_positions(id)? {
            let media_len = {
                let m = &self.tracks[t].clips[c];
                self.media(m.media)
                    .ok_or(EditError::UnknownMedia(m.media))?
                    .duration
            };
            let m = &mut self.tracks[t].clips[c];
            let source_len = m.to_source(m.duration);
            m.rate = rate;
            let mut d = m.to_timeline(source_len).max(min_dur);
            if m.freeze.is_none() && !m.looped {
                d = d.min(m.to_timeline(media_len - m.source_in)).max(min_dur);
            }
            m.duration = d;
            clamp_fades(m);
            if m.all_frames {
                self.fit_all_frames(t, c);
            }
        }
        Ok(())
    }

    /// Conform to Project Frame Rate (VEGAS's Event Properties; Project
    /// Media's Add at Project Frame Rate): the event's (and its group's)
    /// playback rate set so that each frame of its high-frame-rate media is
    /// one of the project's — 0.25 for a 120 fps clip in a 30 fps project
    /// —, within 0.25 to 4; past that, its video's velocity envelope holds
    /// the rest (240 fps in a 24 fps project: 0.25 and 40 %), the sound no
    /// longer in step with it. `show_all`: the events as long as their part
    /// of the media then plays (new ones); else they keep their lengths, as
    /// VEGAS's do. `false` when the event's media has no frame rate.
    pub fn conform_to_project_rate(&mut self, id: ClipId, show_all: bool) -> Result<bool> {
        let c = self.clip(id).ok_or(EditError::UnknownClip(id))?;
        let Some(v) = self
            .media(c.media)
            .and_then(|m| m.video)
            .filter(|v| v.frame_rate.as_f64() > 0.0)
        else {
            return Ok(false);
        };
        let need = self.settings.frame_rate.as_f64() / v.frame_rate.as_f64();
        let rate = need.clamp(0.25, 4.0);
        let velocity = (need / rate) as f32;
        let min_dur = self.min_clip_duration();
        let mut video = Vec::new();
        for (t, c) in self.member_positions(id)? {
            let is_video = self.tracks[t].kind == TrackKind::Video;
            let media_len = {
                let m = &self.tracks[t].clips[c];
                self.media(m.media).map_or(Tick::ZERO, |i| i.duration)
            };
            let m = &mut self.tracks[t].clips[c];
            m.rate = rate;
            if show_all {
                let k = if is_video { velocity as f64 } else { 1.0 };
                let span = m.to_timeline(media_len - m.source_in);
                m.duration = Tick((span.0 as f64 / k) as i64).max(min_dur);
            }
            clamp_fades(m);
            if is_video {
                video.push(m.id);
            }
        }
        if (velocity - 1.0).abs() > 1e-6 {
            use crate::envelope::EnvelopeKind;
            for v in video {
                if self.clip(v).and_then(|c| c.velocity()).is_none() {
                    self.toggle_event_envelope(v, EnvelopeKind::Velocity)?;
                }
                // One velocity, the whole event.
                if let Some(env) = self.clip_mut(v).and_then(|c| {
                    c.envelopes
                        .iter_mut()
                        .find(|e| e.kind == EnvelopeKind::Velocity)
                }) && !env.points.is_empty()
                {
                    env.points.truncate(1);
                    env.points[0].at = Tick::ZERO;
                    env.points[0].value = velocity.clamp(-1.0, 3.0);
                }
            }
        }
        Ok(true)
    }

    /// Congela o quadro do instante `t` da timeline no clipe (ou descongela).
    /// Reverse (an event's shortcut menu): the event plays its part of its
    /// media backward — what it showed at its end it shows at its start —,
    /// or forward again, over the same part.
    pub fn reverse_event(&mut self, id: ClipId) -> Result<()> {
        let c = self.clip_mut(id).ok_or(EditError::UnknownClip(id))?;
        let span = c.source_offset(c.duration);
        match c.reverse.take() {
            None => c.reverse = Some(c.source_in + c.source_in + span),
            Some(mirror) => c.source_in = mirror - c.source_in - span,
        }
        Ok(())
    }

    pub fn set_clip_freeze(&mut self, id: ClipId, at: Option<Tick>) -> Result<()> {
        let (t, c) = self.find_clip(id).ok_or(EditError::UnknownClip(id))?;
        let clip = &mut self.tracks[t].clips[c];
        clip.freeze = None;
        clip.freeze = at.map(|t| clip.source_time(t.clamp(clip.start, clip.end() - Tick(1))));
        Ok(())
    }

    /// Define a duração exata do clipe (e do grupo), respeitando a mídia.
    pub fn set_clip_duration(&mut self, id: ClipId, duration: Tick) -> Result<Tick> {
        let start = self.clip(id).ok_or(EditError::UnknownClip(id))?.start;
        self.trim_end(id, start + duration)
    }

    /// What event `id`'s Pan/Crop lands on (`None`: no such event, or its
    /// media is gone).
    pub fn clip_place_in(&self, id: ClipId) -> Option<crate::pancrop::PlaceIn> {
        let c = self.clip(id)?;
        let info = self.media(c.media)?;
        Some(crate::pancrop::place_in(
            &self.settings,
            info,
            c.pan_crop.keep_aspect,
            c.pan_crop.fill,
        ))
    }

    /// The box of event `id`'s Pan/Crop at `t` on the timeline.
    pub fn clip_box_at(&self, id: ClipId, t: Tick) -> Option<crate::pancrop::PanCropKey> {
        let place = self.clip_place_in(id)?;
        let c = self.clip(id)?;
        Some(c.box_at(t, place.media, place.media_aspect))
    }

    /// Event `id`'s Pan/Crop box at `t` on the timeline becomes `value`,
    /// as [`crate::pancrop::PanCrop::set_at`] says; new keyframes as the
    /// editor's settings say (Options › Preferences › External Control &
    /// Automation: "New video motion keyframes", "Pan/Crop default
    /// smoothness"): the curve of the first ones (later ones take the curve
    /// of the keyframe before them) and their smoothness, 0–100.
    pub fn set_clip_box_at(
        &mut self,
        id: ClipId,
        t: Tick,
        value: crate::pancrop::PanCropKey,
        (curve, smoothness): (crate::fx::KeyCurve, f32),
    ) -> Result<()> {
        let rate = self.settings.frame_rate;
        let place = self.clip_place_in(id).ok_or(EditError::UnknownClip(id))?;
        let c = self.clip_mut(id).ok_or(EditError::UnknownClip(id))?;
        let offset = (rate.snap_nearest(t) - c.start).clamp(Tick::ZERO, c.duration);
        c.pan_crop.set_at(
            offset,
            value,
            place.media,
            (curve, f64::from(smoothness / 100.0)),
        );
        Ok(())
    }

    /// Event `id` framed by `tf` (where a fitted picture goes, how much
    /// zoomed and turned) all along.
    pub fn set_clip_framing(&mut self, id: ClipId, tf: &crate::fx::Transform) -> Result<()> {
        let place = self.clip_place_in(id).ok_or(EditError::UnknownClip(id))?;
        let key = crate::pancrop::key_from_transform(tf, &place);
        let c = self.clip_mut(id).ok_or(EditError::UnknownClip(id))?;
        c.pan_crop = crate::pancrop::PanCrop::holding(
            key,
            place.media,
            c.pan_crop.keep_aspect,
            c.pan_crop.fill,
        );
        Ok(())
    }

    /// Event `id` framed by `keys` (what [`Self::set_clip_framing`] takes,
    /// over time).
    pub fn set_clip_framing_keys(
        &mut self,
        id: ClipId,
        keys: &[crate::fx::TransformKey],
    ) -> Result<()> {
        let place = self.clip_place_in(id).ok_or(EditError::UnknownClip(id))?;
        let c = self.clip_mut(id).ok_or(EditError::UnknownClip(id))?;
        c.pan_crop.ensure_size(place.media);
        c.pan_crop.keys = keys
            .iter()
            .map(|k| crate::pancrop::PanCropKey {
                at: k.at,
                curve: k.curve,
                smoothness: f64::from(k.smoothness / 100.0),
                ..crate::pancrop::key_from_transform(&k.value, &place)
            })
            .collect();
        Ok(())
    }

    /// Event `id`'s box at `t` on the timeline becomes the one `tf` shows.
    pub fn set_clip_transform_at(
        &mut self,
        id: ClipId,
        t: Tick,
        tf: crate::fx::Transform,
    ) -> Result<()> {
        let place = self.clip_place_in(id).ok_or(EditError::UnknownClip(id))?;
        let key = crate::pancrop::key_from_transform(&tf, &place);
        self.set_clip_box_at(id, t, key, (crate::fx::KeyCurve::Smooth, 0.0))
    }

    /// Event `id`'s box at `t` as the closest framing: where a fitted
    /// picture goes, how much zoomed and turned.
    pub fn clip_transform_at(&self, id: ClipId, t: Tick) -> Option<crate::fx::Transform> {
        let place = self.clip_place_in(id)?;
        let key = self.clip_box_at(id, t)?;
        Some(crate::pancrop::transform_from_key(&key, &place))
    }

    /// Event `id`'s Pan/Crop keyframe at `offset` (from its start) gone.
    pub fn remove_clip_box_key(&mut self, id: ClipId, offset: Tick) -> Result<()> {
        let c = self.clip_mut(id).ok_or(EditError::UnknownClip(id))?;
        c.pan_crop.remove_key(offset);
        Ok(())
    }

    /// Agrupa os clipes (um grupo novo para todos).
    pub fn group_clips(&mut self, ids: &[ClipId]) {
        let g = GroupId(self.alloc_id());
        for &id in ids {
            if let Some(c) = self.clip_mut(id) {
                c.group = Some(g);
            }
        }
    }

    /// Tira os clipes de seus grupos.
    pub fn ungroup_clips(&mut self, ids: &[ClipId]) {
        for &id in ids {
            if let Some(c) = self.clip_mut(id) {
                c.group = None;
            }
        }
    }

    fn member_positions(&self, id: ClipId) -> Result<Vec<(usize, usize)>> {
        self.find_clip(id).ok_or(EditError::UnknownClip(id))?;
        Ok(self
            .group_members(id)
            .into_iter()
            .filter_map(|m| self.find_clip(m))
            .collect())
    }

    fn sort_all(&mut self) {
        for t in &mut self.tracks {
            t.sort();
        }
    }

    /// Move o clipe (e seu grupo) para começar em `new_start`. Se `target_track`
    /// for dado, só o clipe arrastado muda de trilha. Devolve o deslocamento aplicado.
    pub fn move_clip(
        &mut self,
        id: ClipId,
        new_start: Tick,
        target_track: Option<TrackId>,
    ) -> Result<Tick> {
        let members = self.member_positions(id)?;
        let (ti, ci) = self.find_clip(id).ok_or(EditError::UnknownClip(id))?;
        if let Some(target) = target_track {
            let kind = self.tracks[ti].kind;
            let t = self.track(target).ok_or(EditError::UnknownTrack(target))?;
            if t.kind != kind {
                return Err(EditError::WrongTrackKind);
            }
        }
        if members.iter().any(|&(t, c)| self.tracks[t].clips[c].locked) {
            return Ok(Tick::ZERO);
        }
        let earliest = members
            .iter()
            .map(|&(t, c)| self.tracks[t].clips[c].start)
            .min()
            .expect("o grupo contém o próprio clipe");
        let delta = (new_start - self.tracks[ti].clips[ci].start).max(-earliest);
        for &(t, c) in &members {
            self.tracks[t].clips[c].start += delta;
        }
        if let Some(target) = target_track.filter(|&tr| tr != self.tracks[ti].id) {
            let clip = self.tracks[ti].clips.remove(ci);
            self.track_mut(target)?.clips.push(clip);
        }
        self.sort_all();
        Ok(delta)
    }

    /// Apara o início do clipe (e do grupo) para `new_start`, mantendo o fim.
    /// Devolve o deslocamento aplicado ao início.
    pub fn trim_start(&mut self, id: ClipId, new_start: Tick) -> Result<Tick> {
        let members = self.member_positions(id)?;
        let min_dur = self.min_clip_duration();
        let clip = self.clip(id).ok_or(EditError::UnknownClip(id))?;
        if members.iter().any(|&(t, c)| {
            let m = &self.tracks[t].clips[c];
            m.locked || m.all_frames
        }) {
            return Ok(Tick::ZERO);
        }
        let mut delta = new_start - clip.start;
        for &(t, c) in &members {
            let m = &self.tracks[t].clips[c];
            // Não antes do início da mídia (a não ser com Loop, que a
            // repete) nem do zero da timeline…
            if m.freeze.is_none() && !m.looped {
                delta = delta.max(-m.to_timeline(m.source_in));
            }
            delta = delta.max(-m.start);
            // …e sem deixar o clipe menor que um quadro.
            delta = delta.min(m.duration - min_dur.min(m.duration));
        }
        for &(t, c) in &members {
            let m = &mut self.tracks[t].clips[c];
            rebase_keys(m, delta);
            m.start += delta;
            m.source_in += m.source_offset(delta);
            rebase_envelopes(m, delta);
            if !m.looped {
                m.source_in = m.source_in.max(Tick::ZERO);
            }
            m.duration -= delta;
            clamp_fades(m);
        }
        self.sort_all();
        Ok(delta)
    }

    /// Apara o fim do clipe (e do grupo) para `new_end`. Devolve o deslocamento
    /// aplicado ao fim.
    pub fn trim_end(&mut self, id: ClipId, new_end: Tick) -> Result<Tick> {
        let members = self.member_positions(id)?;
        let min_dur = self.min_clip_duration();
        let clip = self.clip(id).ok_or(EditError::UnknownClip(id))?;
        if members.iter().any(|&(t, c)| {
            let m = &self.tracks[t].clips[c];
            m.locked || m.all_frames
        }) {
            return Ok(Tick::ZERO);
        }
        // Events may run past the end of their media, like VEGAS's: with
        // Loop it repeats, without it the last frame holds (Hold Last Frame)
        // or nothing shows, and audio is silent.
        let mut delta = new_end - clip.end();
        for &(t, c) in &members {
            let m = &self.tracks[t].clips[c];
            delta = delta.max(min_dur.min(m.duration) - m.duration);
        }
        for &(t, c) in &members {
            let m = &mut self.tracks[t].clips[c];
            m.duration += delta;
            clamp_fades(m);
        }
        Ok(delta)
    }

    /// The Trimmer's Enable Timeline Overwrite: on the tracks of the events
    /// `ids` (just added), the other events lose the time those take —
    /// split at their edges and what is inside removed — instead of lying
    /// beneath them. Groups are ignored meanwhile: a mate on another track
    /// stays whole.
    pub fn overwrite_under(&mut self, ids: &[ClipId]) -> Result<()> {
        let ignore = std::mem::replace(&mut self.ignore_groups, true);
        let result = (|| {
            for &id in ids {
                let (ti, _) = self.find_clip(id).ok_or(EditError::UnknownClip(id))?;
                let r = self.clip(id).ok_or(EditError::UnknownClip(id))?.range();
                let under: Vec<ClipId> = self.tracks[ti]
                    .clips
                    .iter()
                    .filter(|c| !ids.contains(&c.id) && c.start < r.end() && r.start < c.end())
                    .map(|c| c.id)
                    .collect();
                for o in under {
                    let mut inside = o;
                    let start = self.clip(o).map_or(r.start, |c| c.start);
                    if start < r.start {
                        // The part from the new event's start on.
                        inside = self.split(o, r.start)?.first().copied().unwrap_or(o);
                    }
                    let end = self.clip(inside).map_or(r.end(), |c| c.end());
                    if end > r.end() {
                        self.split(inside, r.end())?;
                    }
                    self.delete_clips(&[inside], false)?;
                }
            }
            Ok(())
        })();
        self.ignore_groups = ignore;
        result
    }

    /// Divide o clipe (e o grupo) em `at`. Devolve os ids das metades da direita.
    pub fn split(&mut self, id: ClipId, at: Tick) -> Result<Vec<ClipId>> {
        let members = self.member_positions(id)?;
        let inside: Vec<(usize, usize)> = members
            .into_iter()
            .filter(|&(t, c)| {
                let m = &self.tracks[t].clips[c];
                at > m.start && at < m.end()
            })
            .collect();
        if inside.is_empty() {
            return Err(EditError::NotInsideClip);
        }
        let group = (inside.len() > 1).then(|| GroupId(self.alloc_id()));
        let mut new_ids = Vec::new();
        for (t, c) in inside {
            let new_id = ClipId(self.alloc_id());
            let left = &mut self.tracks[t].clips[c];
            let cut = at - left.start;
            let mut right = Clip {
                id: new_id,
                start: at,
                duration: left.duration - cut,
                source_in: left.source_in + left.source_offset(cut),
                group,
                fade_in: Tick::ZERO,
                ..left.clone()
            };
            // The envelopes go on where they were: the right half's from
            // the cut (with no transition of its own to progress), the left
            // half's end there.
            rebase_envelopes(&mut right, cut);
            right
                .envelopes
                .retain(|e| e.kind != crate::envelope::EnvelopeKind::TransitionProgress);
            for env in &mut left.envelopes {
                let value = env.value_at(cut);
                env.points.retain(|p| p.at < cut);
                env.points.push(crate::envelope::EnvelopePoint {
                    at: cut,
                    value,
                    curve: crate::fx::KeyCurve::Linear,
                });
            }
            // A metade da direita começa no valor do corte; a da esquerda
            // termina nele.
            let size = left.pan_crop.own_size();
            right.pan_crop = left.pan_crop.split_at(cut, size, 1.0);
            // Its masks' shapes too: the right half's start with those at
            // the cut.
            if let Some(m) = &mut left.mask {
                right.mask = Some(m.split_at(cut));
            }
            clamp_fades(&mut right);
            right.snap_offset = (left.snap_offset - cut).max(Tick::ZERO);
            if left.snap_offset >= cut {
                left.snap_offset = Tick::ZERO;
            }
            left.duration = cut;
            left.fade_out = Tick::ZERO;
            clamp_fades(left);
            self.tracks[t].clips.push(right);
            new_ids.push(new_id);
        }
        self.sort_all();
        Ok(new_ids)
    }

    /// Define os fades de entrada e saída do clipe (e do grupo), limitados à
    /// duração. `None` mantém o valor atual.
    pub fn set_fades(
        &mut self,
        id: ClipId,
        fade_in: Option<Tick>,
        fade_out: Option<Tick>,
    ) -> Result<()> {
        let members = self.member_positions(id)?;
        for (t, c) in members {
            let m = &mut self.tracks[t].clips[c];
            if let Some(f) = fade_in {
                m.fade_in = f.clamp(Tick::ZERO, m.duration);
                m.fade_out = m.fade_out.min(m.duration - m.fade_in);
            }
            if let Some(f) = fade_out {
                m.fade_out = f.clamp(Tick::ZERO, m.duration);
                m.fade_in = m.fade_in.min(m.duration - m.fade_out);
            }
        }
        Ok(())
    }

    /// Divide todos os clipes de todas as trilhas que contêm `at`.
    pub fn split_all_at(&mut self, at: Tick) -> Vec<ClipId> {
        let ids: Vec<ClipId> = self
            .clips()
            .filter(|(_, c)| at > c.start && at < c.end())
            .map(|(_, c)| c.id)
            .collect();
        let mut out = Vec::new();
        for id in ids {
            // Um clipe já dividido junto com o grupo não contém mais `at`.
            if let Ok(mut new) = self.split(id, at) {
                out.append(&mut new);
            }
        }
        out
    }

    /// Apaga os clipes (com seus grupos). Com `ripple`, os clipes posteriores
    /// das mesmas trilhas andam para a esquerda e fecham o buraco.
    pub fn delete_clips(&mut self, ids: &[ClipId], ripple: bool) -> Result<()> {
        self.delete_events(ids, ripple, true)
    }

    /// [`Self::delete_clips`]; with `grouped` off, only the events given go
    /// (Options › Preferences › Editing: "Cut, copy, and delete grouped
    /// events").
    pub fn delete_events(&mut self, ids: &[ClipId], ripple: bool, grouped: bool) -> Result<()> {
        for &id in ids {
            self.find_clip(id).ok_or(EditError::UnknownClip(id))?;
        }
        let doomed = self.edit_reach(ids, grouped);
        let mut removed: HashMap<TrackId, Vec<TimeRange>> = HashMap::new();
        for t in &mut self.tracks {
            t.clips.retain(|c| {
                let keep = doomed.binary_search(&c.id).is_err();
                if !keep {
                    removed.entry(t.id).or_default().push(c.range());
                }
                keep
            });
        }
        if ripple {
            for t in &mut self.tracks {
                let Some(ranges) = removed.get(&t.id) else {
                    continue;
                };
                let holes = union(ranges);
                for c in &mut t.clips {
                    let shift: i64 = holes
                        .iter()
                        .filter(|h| h.end() <= c.start)
                        .map(|h| h.duration.0)
                        .sum();
                    c.start -= Tick(shift);
                }
                t.sort();
            }
        }
        Ok(())
    }
}

/// Desloca os quadros-chave quando o início do clipe anda `delta`: o valor
/// no novo início vira o primeiro quadro-chave.
pub(crate) fn rebase_keys(c: &mut Clip, delta: Tick) {
    if let Some(m) = &mut c.mask {
        m.rebase(delta);
    }
    let size = c.pan_crop.own_size();
    c.pan_crop.rebase(delta, size, 1.0);
}

/// The event's own envelopes when its start moves `delta` later in it
/// (their times count from its start): what was at `delta` is now at 0, so
/// they keep their shapes where the event still is.
pub(crate) fn rebase_envelopes(c: &mut Clip, delta: Tick) {
    // Its motion tracks count from its start too.
    for t in &mut c.motion_tracks {
        t.rebase(delta);
    }
    for env in &mut c.envelopes {
        let first = crate::envelope::EnvelopePoint {
            at: Tick::ZERO,
            value: env.value_at(delta),
            curve: env
                .points
                .iter()
                .rev()
                .find(|p| p.at <= delta)
                .map_or(crate::fx::KeyCurve::Linear, |p| p.curve),
        };
        let mut points: Vec<_> = env
            .points
            .iter()
            .filter(|p| p.at > delta)
            .map(|p| crate::envelope::EnvelopePoint {
                at: p.at - delta,
                ..*p
            })
            .collect();
        points.insert(0, first);
        env.points = points;
    }
}

/// Garante que os fades caibam no clipe (o de saída cede primeiro).
pub(crate) fn clamp_fades(c: &mut Clip) {
    c.fade_in = c.fade_in.clamp(Tick::ZERO, c.duration);
    c.fade_out = c.fade_out.clamp(Tick::ZERO, c.duration - c.fade_in);
}

/// União de intervalos, ordenada e sem sobreposição.
pub(crate) fn union(ranges: &[TimeRange]) -> Vec<TimeRange> {
    let mut v = ranges.to_vec();
    v.sort_by_key(|r| r.start);
    let mut out: Vec<TimeRange> = Vec::new();
    for r in v {
        match out.last_mut() {
            Some(last) if r.start <= last.end() => {
                let end = last.end().max(r.end());
                *last = TimeRange::from_bounds(last.start, end);
            }
            _ => out.push(r),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_of_generated_media_is_the_events_own() {
        let mut p = Project::default();
        let settings = p.settings;
        let g = p.add_media(MediaInfo::generated(
            crate::model::Generator::Checkerboard { size: 30 },
            &settings,
        ));
        let v = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .unwrap()
            .id;
        let a = p.insert_media(g, Tick::ZERO, Some(v), None).unwrap()[0];
        let b = p.insert_media(g, s(10), Some(v), None).unwrap()[0];
        let copy = p.copy_generated_media(b).expect("a copy");
        assert_ne!(copy, g);
        assert_eq!(p.clip(a).unwrap().media, g);
        assert_eq!(p.clip(b).unwrap().media, copy);
        assert_eq!(p.media(copy).unwrap().source, p.media(g).unwrap().source);
        // A file's events keep theirs.
        let f = p.add_media(av_media(20));
        let c = p.insert_media(f, s(30), Some(v), None).unwrap()[0];
        assert_eq!(p.copy_generated_media(c), None);
        assert_eq!(p.clip(c).unwrap().media, f);
    }

    #[test]
    fn overwriting_takes_the_time_from_the_events_beneath() {
        let mut p = Project::default();
        let m = p.add_media(av_media(20));
        let v = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .unwrap()
            .id;
        let under = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        p.clip_mut(under).unwrap().duration = s(10);
        let new = p.insert_media(m, s(4), Some(v), None).unwrap()[0];
        p.clip_mut(new).unwrap().duration = s(2);
        p.overwrite_under(&[new]).unwrap();
        let spans: Vec<(Tick, Tick)> = p
            .track(v)
            .unwrap()
            .clips
            .iter()
            .map(|c| (c.start, c.end()))
            .collect();
        assert_eq!(spans, vec![(s(0), s(4)), (s(4), s(6)), (s(6), s(10))]);
        assert!(!p.ignore_groups, "grouping comes back");
    }

    #[test]
    fn stereo_as_dual_mono() {
        let mut p = Project::default();
        let m = p.add_media(MediaInfo::generated(
            crate::Generator::TestTone {
                frequency_hz: 440.0,
                level_db: -6.0,
            },
            &p.settings,
        ));
        let a = p.add_track(TrackKind::Audio);
        let id = p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap()[0];
        let (copy, left, right) = p.split_dual_mono(id).unwrap();
        assert_eq!(left, a);
        assert_ne!(right, a, "a track of its own");
        let (l, r) = (p.clip(id).unwrap(), p.clip(copy).unwrap());
        assert_eq!(
            (l.channels, r.channels),
            (ChannelMap::LeftOnly, ChannelMap::RightOnly)
        );
        assert!(l.group.is_some() && l.group == r.group, "grouped");
        assert_eq!((l.start, l.duration), (r.start, r.duration));
    }

    #[test]
    fn high_frame_rate_events_conform_to_the_project() {
        let mut p = Project::new(ProjectSettings {
            frame_rate: FrameRate::new(30, 1),
            ..ProjectSettings::default()
        });
        let (v, a) = (p.tracks[0].id, p.tracks[1].id);
        let mut info = av_media(4);
        info.video.as_mut().unwrap().frame_rate = FrameRate::new(120, 1);
        let m = p.add_media(info);
        let ids = p.insert_media(m, Tick::ZERO, Some(v), Some(a)).unwrap();
        // 120 fps in 30: a quarter of the speed, all of it shown.
        assert!(p.conform_to_project_rate(ids[0], true).unwrap());
        for id in &ids {
            let c = p.clip(*id).unwrap();
            assert_eq!((c.rate, c.duration), (0.25, s(16)));
        }
        assert!(p.clip(ids[0]).unwrap().velocity().is_none());
        // 240 fps in 24: 0.25 and the velocity envelope's 40 %, the
        // event's length kept.
        p.settings.frame_rate = FrameRate::new(24, 1);
        let media = p.media.iter_mut().find(|x| x.id == m).unwrap();
        media.video.as_mut().unwrap().frame_rate = FrameRate::new(240, 1);
        assert!(p.conform_to_project_rate(ids[0], false).unwrap());
        let c = p.clip(ids[0]).unwrap();
        assert_eq!((c.rate, c.duration), (0.25, s(16)));
        let env = c.velocity().expect("a velocity envelope");
        assert!((env.value_at(Tick::ZERO) - 0.4).abs() < 1e-6);
        assert!(
            p.clip(ids[1]).unwrap().velocity().is_none(),
            "not the sound's"
        );
        // A media without frames: nothing to do.
        let tone = p.add_media(MediaInfo::generated(
            crate::Generator::TestTone {
                frequency_hz: 440.0,
                level_db: -6.0,
            },
            &p.settings,
        ));
        let t = p.insert_media(tone, s(20), None, Some(a)).unwrap()[0];
        assert!(!p.conform_to_project_rate(t, true).unwrap());
    }

    #[test]
    fn a_duplicated_track_comes_below_with_events_of_its_own() {
        let mut p = Project::default();
        let m = p.add_media(MediaInfo::generated(
            crate::Generator::ColorBars,
            &p.settings,
        ));
        let v = p.tracks[0].id;
        let id = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        let copy = p.duplicate_track(v).unwrap();
        assert_eq!(p.track_index(copy), Some(p.track_index(v).unwrap() + 1));
        let c = &p.track(copy).unwrap().clips[0];
        assert_ne!(c.id, id, "events of its own");
        assert_eq!((c.start, c.duration, c.media), {
            let o = p.clip(id).unwrap();
            (o.start, o.duration, o.media)
        });
        assert!(p.duplicate_track(TrackId(999_999)).is_err());
    }
    use crate::model::{AudioStreamInfo, Generator, MediaSource, ProjectSettings, VideoStreamInfo};
    use bv_core::{FrameRate, SampleRate};

    fn s(seconds: i64) -> Tick {
        Tick::from_seconds(seconds)
    }

    fn av_media(seconds: i64) -> MediaInfo {
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
                frame_rate: FrameRate::FPS_29_97,
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

    /// Projeto com uma mídia A/V de 10 s inserida em 0.
    fn setup() -> (Project, TrackId, TrackId, ClipId, ClipId) {
        let mut p = Project::new(ProjectSettings::default());
        let v = p.tracks[0].id;
        let a = p.tracks[1].id;
        let m = p.add_media(av_media(10));
        let ids = p.insert_media(m, Tick::ZERO, Some(v), Some(a)).unwrap();
        p.check_invariants().unwrap();
        (p, v, a, ids[0], ids[1])
    }

    #[test]
    fn inserir_cria_par_agrupado() {
        let (p, v, a, cv, ca) = setup();
        assert_eq!(p.clip_track(cv).unwrap().id, v);
        assert_eq!(p.clip_track(ca).unwrap().id, a);
        assert!(p.clip(cv).unwrap().group.is_some());
        assert_eq!(p.clip(cv).unwrap().group, p.clip(ca).unwrap().group);
        assert_eq!(p.duration(), s(10));
    }

    #[test]
    fn inserir_em_trilha_errada_falha() {
        let (mut p, v, a, ..) = setup();
        let m = p.media[0].id;
        assert_eq!(
            p.insert_media(m, Tick::ZERO, Some(a), Some(v)),
            Err(EditError::WrongTrackKind)
        );
    }

    #[test]
    fn mover_leva_o_grupo_e_nao_passa_do_zero() {
        let (mut p, _, _, cv, ca) = setup();
        assert_eq!(p.move_clip(cv, s(5), None).unwrap(), s(5));
        assert_eq!(p.clip(ca).unwrap().start, s(5));
        assert_eq!(p.move_clip(ca, s(-3), None).unwrap(), s(-5));
        assert_eq!(p.clip(cv).unwrap().start, Tick::ZERO);
        p.check_invariants().unwrap();
    }

    #[test]
    fn mover_para_outra_trilha() {
        let (mut p, v1, _, cv, ca) = setup();
        let v2 = p.add_track(TrackKind::Video);
        assert_eq!(
            p.tracks[1].id, v2,
            "trilha de vídeo nova fica acima do áudio"
        );
        p.move_clip(cv, s(2), Some(v2)).unwrap();
        assert_eq!(p.clip_track(cv).unwrap().id, v2);
        assert!(p.track(v1).unwrap().clips.is_empty());
        assert_eq!(p.clip(ca).unwrap().start, s(2));
        let a2 = p.add_track(TrackKind::Audio);
        assert_eq!(
            p.move_clip(cv, s(0), Some(a2)),
            Err(EditError::WrongTrackKind)
        );
        p.check_invariants().unwrap();
    }

    #[test]
    fn aparar_respeita_a_midia() {
        let (mut p, _, _, cv, ca) = setup();
        // Without Loop the start cannot go before the media start.
        for id in [cv, ca] {
            p.clip_mut(id).unwrap().looped = false;
        }
        assert_eq!(p.trim_start(cv, s(-2)).unwrap(), Tick::ZERO);
        assert_eq!(p.trim_start(cv, s(3)).unwrap(), s(3));
        let c = p.clip(ca).unwrap();
        assert_eq!((c.start, c.source_in, c.duration), (s(3), s(3), s(7)));
        // Volta 1 s: só até onde a mídia permite.
        assert_eq!(p.trim_start(ca, s(1)).unwrap(), s(-2));
        // The end may run past the media's end, like VEGAS's (the last frame
        // holds or nothing shows).
        assert_eq!(p.trim_end(cv, s(20)).unwrap(), s(10));
        assert_eq!(p.trim_end(cv, s(4)).unwrap(), s(-16));
        // Nunca menor que um quadro.
        let d = p.trim_end(cv, Tick::ZERO).unwrap();
        assert_eq!(p.clip(cv).unwrap().duration, p.min_clip_duration());
        assert!(d < Tick::ZERO);
        p.check_invariants().unwrap();
    }

    #[test]
    fn dividir_cria_novo_grupo_com_as_metades() {
        let (mut p, _, _, cv, ca) = setup();
        let right = p.split(cv, s(4)).unwrap();
        assert_eq!(right.len(), 2);
        let r0 = p.clip(right[0]).unwrap().clone();
        let r1 = p.clip(right[1]).unwrap().clone();
        assert_eq!((r0.start, r0.source_in, r0.duration), (s(4), s(4), s(6)));
        assert_eq!(r0.group, r1.group);
        assert_ne!(r0.group, p.clip(cv).unwrap().group);
        assert_eq!(p.clip(ca).unwrap().duration, s(4));
        assert_eq!(p.split(cv, s(4)), Err(EditError::NotInsideClip));
        p.check_invariants().unwrap();
        // Mover a metade da direita não leva a da esquerda.
        p.move_clip(right[0], s(6), None).unwrap();
        assert_eq!(p.clip(cv).unwrap().start, Tick::ZERO);
        assert_eq!(p.clip(right[1]).unwrap().start, s(6));
    }

    #[test]
    fn apagar_com_ripple_fecha_o_buraco() {
        let (mut p, v, a, cv, _) = setup();
        let right = p.split(cv, s(4)).unwrap();
        p.split(right[0], s(7)).unwrap();
        // Três pares: [0,4) [4,7) [7,10). Apaga o do meio com ripple.
        p.delete_clips(&[right[0]], true).unwrap();
        for tr in [v, a] {
            let starts: Vec<Tick> = p.track(tr).unwrap().clips.iter().map(|c| c.start).collect();
            assert_eq!(starts, vec![s(0), s(4)]);
        }
        assert_eq!(p.duration(), s(7));
        p.check_invariants().unwrap();
    }

    #[test]
    fn apagar_sem_ripple_deixa_o_buraco() {
        let (mut p, _, _, cv, _) = setup();
        let right = p.split(cv, s(4)).unwrap();
        p.delete_clips(&[cv], false).unwrap();
        assert_eq!(p.clip(right[0]).unwrap().start, s(4));
        assert_eq!(p.clips().count(), 2);
    }

    #[test]
    fn camadas_de_video() {
        let (mut p, _, _, cv, _) = setup();
        // A trilha nova fica abaixo de "Vídeo 1".
        let v2 = p.add_track(TrackKind::Video);
        let m = p.media[0].id;
        let lower = p.insert_media(m, s(2), Some(v2), None).unwrap()[0];
        let l = p.video_layers_at(s(1));
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].clip.id, cv);
        // Em 3 s há duas camadas, de baixo para cima.
        let l = p.video_layers_at(s(3));
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].clip.id, lower);
        assert_eq!(l[0].source_time, s(1));
        assert_eq!(l[1].clip.id, cv, "a trilha do topo é composta por último");
    }

    #[test]
    fn json_ida_e_volta() {
        let (mut p, ..) = setup();
        p.split_all_at(s(5));
        let json = p.to_json();
        let back = Project::from_json(&json).unwrap();
        assert_eq!(back, p);
        back.check_invariants().unwrap();
    }

    #[test]
    fn midia_gerada_entra_com_cinco_segundos_e_pode_esticar() {
        let mut p = Project::default();
        let v = p.tracks[0].id;
        let m = p.add_media(MediaInfo::generated(Generator::ColorBars, &p.settings));
        let c = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        assert_eq!(p.clip(c).unwrap().duration, Generator::DEFAULT_CLIP);
        assert_eq!(p.trim_end(c, s(60)).unwrap(), s(55));
        p.check_invariants().unwrap();
    }

    #[test]
    fn fades_acompanham_aparar_e_dividir() {
        let (mut p, _, _, cv, ca) = setup();
        p.set_fades(cv, Some(s(2)), Some(s(3))).unwrap();
        assert_eq!(p.clip(ca).unwrap().fade_in, s(2), "o grupo recebe o fade");
        p.trim_end(cv, s(4)).unwrap();
        let c = p.clip(cv).unwrap();
        assert_eq!((c.fade_in, c.fade_out), (s(2), s(2)));
        let right = p.split(cv, s(1)).unwrap()[0];
        assert_eq!(p.clip(cv).unwrap().fade_out, Tick::ZERO);
        assert_eq!(p.clip(right).unwrap().fade_in, Tick::ZERO);
        assert_eq!(p.clip(right).unwrap().fade_out, s(2));
        p.check_invariants().unwrap();
    }

    #[test]
    fn clipe_travado_nao_move() {
        let (mut p, _, _, cv, ca) = setup();
        p.clip_mut(ca).unwrap().locked = true;
        assert_eq!(p.move_clip(cv, s(3), None).unwrap(), Tick::ZERO);
        assert_eq!(p.trim_end(cv, s(2)).unwrap(), Tick::ZERO);
        p.ungroup_clips(&[cv, ca]);
        assert_eq!(p.move_clip(cv, s(3), None).unwrap(), s(3));
    }

    #[test]
    fn trecho_da_midia() {
        let (mut p, v, a, ..) = setup();
        let m = p.media[0].id;
        let ids = p
            .insert_media_range(m, s(20), s(2), s(3), Some(v), Some(a))
            .unwrap();
        let c = p.clip(ids[0]).unwrap();
        assert_eq!((c.start, c.source_in, c.duration), (s(20), s(2), s(3)));
        p.check_invariants().unwrap();
    }

    #[test]
    fn velocidade_mantem_o_trecho_da_midia() {
        let (mut p, _, _, cv, ca) = setup();
        p.trim_end(cv, s(4)).unwrap();
        p.set_clip_rate(cv, 2.0).unwrap();
        let c = p.clip(cv).unwrap().clone();
        assert_eq!(c.duration, s(2));
        assert_eq!(c.source_time(s(1)), s(2));
        assert_eq!(p.clip(ca).unwrap().rate, 2.0, "o áudio do grupo acompanha");
        // At 2× the 10 s of media fill 5 s of timeline; with Loop the event
        // goes on (the media repeats).
        assert_eq!(p.trim_end(cv, s(5)).unwrap(), s(3));
        let right = p.split(cv, s(1)).unwrap()[0];
        assert_eq!(p.clip(right).unwrap().source_in, s(2));
        p.set_clip_rate(cv, 0.5).unwrap();
        assert_eq!(p.clip(cv).unwrap().duration, s(4));
        p.check_invariants().unwrap();
    }

    #[test]
    fn tracks_take_their_kinds_color_until_given_one() {
        let mut p = Project::default();
        let (v, a) = (p.tracks[0].id, p.tracks[1].id);
        assert_eq!(p.track(v).unwrap().color_index(), 0, "video: the first");
        assert_eq!(p.track(a).unwrap().color_index(), 1, "audio: the second");
        p.set_track_color(a, Some(5)).unwrap();
        assert_eq!(p.track(a).unwrap().color_index(), 5);
        // Out of the palette: wraps into it.
        p.set_track_color(v, Some(TRACK_PALETTE + 2)).unwrap();
        assert_eq!(p.track(v).unwrap().color_index(), 2);
        p.set_track_color(v, None).unwrap();
        assert_eq!(p.track(v).unwrap().color_index(), 0);
        // Kept in the project file, and absent while it is the kind's.
        let json = p.to_json();
        let back = Project::from_json(&json).unwrap();
        assert_eq!(back.track(a).unwrap().color, Some(5));
        assert_eq!(back.track(v).unwrap().color, None);
        assert_eq!(json.matches("\"color\"").count(), 1, "{json}");
    }

    /// As VEGAS's: each new track the palette's next color by its place,
    /// round again after the eighth.
    #[test]
    fn new_tracks_take_the_palettes_colors_in_turn() {
        let mut p = Project::default();
        let added: Vec<TrackId> = (0..9).map(|_| p.add_track(TrackKind::Audio)).collect();
        let colors: Vec<usize> = p.tracks.iter().map(|t| t.color_index()).collect();
        assert_eq!(colors, [0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2]);
        assert_eq!(p.new_track_color(TrackKind::Audio), 3);
        let top = p.insert_track_at(TrackKind::Video, 0);
        assert_eq!(p.track(top).unwrap().color_index(), 0);
        assert_eq!(p.track(added[0]).unwrap().color_index(), 2, "kept");
    }

    #[test]
    fn tracks_move_to_any_position() {
        let mut p = Project::default();
        let (v, a) = (p.tracks[0].id, p.tracks[1].id);
        let a2 = p.add_track(TrackKind::Audio);
        p.move_track_to(a2, 0).unwrap();
        assert_eq!(
            p.tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
            [a2, v, a]
        );
        p.move_track_to(a2, 99).unwrap();
        assert_eq!(
            p.tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
            [v, a, a2]
        );
        p.check_invariants().unwrap();
    }

    #[test]
    fn loop_repeats_the_media() {
        let (mut p, _, _, cv, _) = setup();
        // The media is 10 s long: stretched to 25 s, it starts over at 10 s
        // and at 20 s.
        p.trim_end(cv, s(25)).unwrap();
        let c = p.clip(cv).unwrap().clone();
        assert_eq!(c.duration, s(25));
        assert_eq!(c.media_time(s(12), s(10)), s(2));
        assert_eq!(c.loop_points(s(10)), vec![s(10), s(20)]);
        // Before the media start too: the start wraps to its end.
        p.trim_start(cv, s(-2)).unwrap_or_default();
        let c = p.clip(cv).unwrap().clone();
        assert!(c.start == Tick::ZERO, "not before the timeline start");
        // Without Loop the media time runs on past the end.
        let mut c = c;
        c.looped = false;
        assert_eq!(c.media_time(s(12), s(10)), s(12));
        assert!(c.loop_points(s(10)).is_empty());
    }

    #[test]
    fn extra_audio_streams_go_to_their_own_tracks() {
        let mut p = Project::default();
        let (v, a) = (p.tracks[0].id, p.tracks[1].id);
        let mut info = av_media(10);
        let more = info.audio.unwrap();
        info.extra_audio = vec![more, more];
        let m = p.add_media(info);
        let ids = p.insert_media(m, Tick::ZERO, Some(v), Some(a)).unwrap();
        assert_eq!(ids.len(), 4, "video + three audio streams");
        let audio: Vec<_> = p
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .collect();
        assert_eq!(audio.len(), 3, "two audio tracks were added");
        for (k, t) in audio.iter().enumerate() {
            assert_eq!(t.clips.len(), 1);
            assert_eq!(t.clips[0].audio_stream, k as u16);
        }
        // All in one group: they move together.
        let group = p.clip(ids[0]).unwrap().group;
        assert!(group.is_some());
        assert!(ids.iter().all(|&id| p.clip(id).unwrap().group == group));
        p.check_invariants().unwrap();
    }

    #[test]
    fn quadro_congelado() {
        let (mut p, _, _, cv, ca) = setup();
        // O áudio do grupo limitaria a extensão: congelar é coisa do vídeo.
        p.ungroup_clips(&[cv, ca]);
        p.set_clip_freeze(cv, Some(s(3))).unwrap();
        let c = p.clip(cv).unwrap();
        assert_eq!(c.source_time(Tick::ZERO), s(3));
        assert_eq!(c.source_time(s(9)), s(3));
        // Congelado, o clipe pode passar do fim da mídia.
        assert_eq!(p.trim_end(cv, s(30)).unwrap(), s(20));
        p.check_invariants().unwrap();
        p.set_clip_freeze(cv, None).unwrap();
        assert_eq!(p.clip(cv).unwrap().source_time(s(2)), s(2));
    }

    #[test]
    fn quadros_chave_do_enquadramento() {
        let (mut p, _, _, cv, _) = setup();
        let zoom = crate::fx::Transform {
            scale: 2.0,
            ..crate::fx::Transform::default()
        };
        p.set_clip_transform_at(cv, s(4), zoom).unwrap();
        let scale = |p: &Project, id: ClipId, t: Tick| p.clip_transform_at(id, t).unwrap().scale;
        assert_eq!(p.clip(cv).unwrap().pan_crop.keys.len(), 2);
        assert_eq!(scale(&p, cv, Tick::ZERO), 1.0);
        assert!((scale(&p, cv, s(4)) - 2.0).abs() < 1e-3);
        assert!((scale(&p, cv, s(9)) - 2.0).abs() < 1e-3);
        let mid = scale(&p, cv, s(2));
        assert!(mid > 1.0 && mid < 2.0, "{mid}");
        // Dividir no meio mantém a animação contínua dos dois lados.
        let right = p.split(cv, s(2)).unwrap()[0];
        assert!((scale(&p, cv, s(2) - Tick(1)) - mid).abs() < 1e-3);
        assert!((scale(&p, right, s(2)) - mid).abs() < 1e-3);
        assert!((scale(&p, right, s(4)) - 2.0).abs() < 1e-3);
        // Aparar o início re-baseia os quadros-chave.
        p.trim_start(right, s(3)).unwrap();
        assert_eq!(p.clip(right).unwrap().pan_crop.keys[0].at, Tick::ZERO);
        assert!((scale(&p, right, s(4)) - 2.0).abs() < 1e-3);
        p.remove_clip_box_key(right, s(1)).unwrap();
        p.check_invariants().unwrap();
    }

    #[test]
    fn solo_e_mute() {
        let (mut p, _, a, ..) = setup();
        let a2 = p.add_track(TrackKind::Audio);
        assert_eq!(p.audible_tracks().count(), 2);
        p.set_track_solo(a2, true).unwrap();
        assert_eq!(
            p.audible_tracks().map(|t| t.id).collect::<Vec<_>>(),
            vec![a2]
        );
        p.set_track_muted(a2, true).unwrap();
        assert_eq!(p.audible_tracks().count(), 0);
        p.set_track_solo(a2, false).unwrap();
        assert_eq!(
            p.audible_tracks().map(|t| t.id).collect::<Vec<_>>(),
            vec![a]
        );
    }

    #[test]
    fn new_events_follow_the_editing_preferences() {
        let mut p = Project::new(ProjectSettings::default());
        let v = p.tracks[0].id;
        let a = p.tracks[1].id;
        let m = p.add_media(av_media(10));
        let new = NewEvents {
            looped: false,
            preserve_pitch: true,
            still_length: s(3),
            audio_fades: crate::fx::FadeCurve::Fast,
            video_fades: crate::fx::FadeCurve::Sharp,
        };
        for id in p
            .insert_media_as(m, Tick::ZERO, Some(v), Some(a), &new)
            .unwrap()
        {
            let c = p.clip(id).unwrap();
            assert!(!c.looped && c.preserve_pitch);
            assert_eq!(c.duration, s(10), "a video file keeps its length");
            // Their fades as the editor's settings say, by kind.
            let curve = if p.clip_track(id).unwrap().kind == TrackKind::Audio {
                crate::fx::FadeCurve::Fast
            } else {
                crate::fx::FadeCurve::Sharp
            };
            assert_eq!(
                (c.fade_in_curve, c.fade_out_curve),
                (curve, curve.out_shape())
            );
        }
        let mut still = av_media(10);
        (still.still, still.audio, still.duration) = (true, None, Generator::DURATION);
        let m = p.add_media(still);
        let id = p.insert_media_as(m, s(20), Some(v), None, &new).unwrap()[0];
        assert_eq!(
            p.clip(id).unwrap().duration,
            s(3),
            "a still image lasts as asked"
        );
        // Generated media keeps its 5 seconds.
        let bars = p.add_media(MediaInfo::generated(
            Generator::ColorBars,
            &p.settings.clone(),
        ));
        let id = p.insert_media_as(bars, s(30), Some(v), None, &new).unwrap()[0];
        assert_eq!(p.clip(id).unwrap().duration, Generator::DEFAULT_CLIP);
    }

    #[test]
    fn events_deleted_and_copied_with_or_without_their_group() {
        let (mut p, _, _, cv, ca) = setup();
        p.delete_events(&[cv], false, false).unwrap();
        assert!(
            p.clip(cv).is_none() && p.clip(ca).is_some(),
            "only the video"
        );
        let (mut p, _, _, cv, ca) = setup();
        p.delete_events(&[cv], false, true).unwrap();
        assert!(p.clip(ca).is_none(), "grouped: the audio goes too");
        let (p, _, _, cv, _) = setup();
        assert_eq!(p.copy_events(&[cv], false).len(), 1);
        assert_eq!(p.copy_events(&[cv], true).len(), 2);
    }
}
