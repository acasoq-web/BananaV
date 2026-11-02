//! The project a read document makes, once its media are found.

use std::collections::{HashMap, HashSet};

use bv_core::{SampleRate, Tick, TimeRange};

use super::read::{DocChannels, DocEvent, DocTrack, Document, MediaRef, MediaRefSource};
use crate::model::{
    AudioStreamInfo, ChannelMap, Clip, ClipId, Generator, GroupId, Marker, MarkerId, MediaId,
    MediaInfo, MediaMarker, MediaSource, PixelAspect, Project, ProjectSettings, Region, Track,
    TrackId, TrackKind, VideoStreamInfo,
};

/// A project made from an OpenTimelineIO document.
#[derive(Clone, Debug)]
pub struct Imported {
    pub project: Project,
    /// The media files the document names that were not found (their
    /// names): their events stay, offline.
    pub missing: Vec<String>,
    /// What the project leaves out: a nested timeline's other tracks,
    /// tracks of other kinds, clips without a length, events whose media
    /// lacks their track's kind of stream.
    pub skipped: usize,
}

/// The sample rates a project may have (Project Properties' list).
const SAMPLE_RATES: [u32; 10] = [
    8_000, 11_025, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
];

/// The settings of a project of another program's document: `base`'s,
/// with the document's frame rate, its frame (else its first video's) and
/// its sample rate.
fn settings_for(
    doc: &Document,
    found: &[Option<MediaInfo>],
    base: &ProjectSettings,
) -> ProjectSettings {
    let mut s = *base;
    if let Some(rate) = doc.rate {
        s.frame_rate = rate;
    }
    let first_video = doc
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Video)
        .flat_map(|t| &t.events)
        .find_map(|e| found.get(e.media)?.as_ref()?.video);
    if let Some(v) = first_video {
        s.width = v.width;
        s.height = v.height;
        s.pixel_aspect = v.pixel_aspect;
    }
    if let Some((width, height)) = doc.frame {
        if (s.width, s.height) != (width, height) {
            s.pixel_aspect = PixelAspect::SQUARE;
        }
        s.width = width;
        s.height = height;
    }
    if let Some(hz) = doc.sample_rate.filter(|hz| SAMPLE_RATES.contains(hz)) {
        s.sample_rate = SampleRate(hz);
    }
    s
}

/// Where the document's channels `ch` of media `m` are: the stream with
/// its left one, and how the event plays that stream's channels.
fn channel_place(m: &MediaInfo, ch: DocChannels) -> (u16, ChannelMap) {
    let ch = DocChannels {
        left: ch.left.max(1),
        right: ch.right.max(1),
        ..ch
    };
    let streams: Vec<u16> = m
        .audio
        .iter()
        .chain(&m.extra_audio)
        .map(|a| a.channels.max(1))
        .collect();
    // Each stream's first channel (1-based).
    let mut first = 1u16;
    for (s, &n) in streams.iter().enumerate() {
        if ch.left < first.saturating_add(n) {
            let left = ch.left - first;
            let right = ch.right.checked_sub(first).filter(|&r| r < n);
            let map = match (n, right) {
                (1, _) | (_, None) => ChannelMap::Both,
                (_, Some(r)) if ch.centered && r != left => ChannelMap::Combine,
                (_, Some(r)) if r == left && left == 0 => ChannelMap::LeftOnly,
                (_, Some(r)) if r == left && left == 1 => ChannelMap::RightOnly,
                (_, Some(1)) if left == 0 => ChannelMap::Both,
                (_, Some(0)) if left == 1 => ChannelMap::Swap,
                _ => ChannelMap::Both,
            };
            return (s as u16, map);
        }
        first = first.saturating_add(n);
    }
    // Past its channels: its first stream.
    (0, ChannelMap::Both)
}

/// Gives an offline media the streams its events need, and the length.
fn offline(info: &mut MediaInfo, used: (bool, bool), reach: Tick, s: &ProjectSettings) {
    if used.0 && info.video.is_none() {
        info.video = Some(VideoStreamInfo {
            width: s.width,
            height: s.height,
            frame_rate: s.frame_rate,
            pixel_aspect: Default::default(),
            field_order: Default::default(),
            spherical: false,
            full_range: false,
            reading: Default::default(),
        });
    }
    if used.1 && info.audio.is_none() {
        info.audio = Some(AudioStreamInfo {
            sample_rate: s.sample_rate,
            channels: s.channels.channel_count() as u16,
        });
    }
    info.duration = info.duration.max(reach).max(s.frame_rate.frame_duration());
}

/// The media `r` stands for: the file probed (`found`) with what BananaV
/// saved of it (its effects, markers, color space), BananaV's generator,
/// or, when the file was not found, an offline media where the document
/// says it is.
fn media_info(r: &MediaRef, found: Option<MediaInfo>, s: &ProjectSettings) -> MediaInfo {
    let path = match &r.source {
        MediaRefSource::File(p) => Some(p.clone()),
        MediaRefSource::Sequence { first, .. } => Some(first.clone()),
        _ => None,
    };
    if let Some(mut f) = found {
        if let Some(saved) = &r.saved {
            f.name = saved.name.clone();
            f.effects = saved.effects.clone();
            f.markers = saved.markers.clone();
            f.color_space = saved.color_space;
        }
        return f;
    }
    match (&r.saved, &r.source) {
        (Some(saved), MediaRefSource::Generated(_)) => saved.clone(),
        (None, MediaRefSource::Generated(g)) => {
            let mut m = MediaInfo::generated(g.clone(), s);
            if !r.name.is_empty() {
                m.name = r.name.clone();
            }
            m
        }
        (saved, _) => {
            // Offline: the media BananaV saved, or one made of what the
            // document says (its name and length; `offline` adds streams).
            let mut m = saved.clone().unwrap_or_else(|| {
                let mut m = MediaInfo::generated(Generator::ColorBars, s);
                m.video = None;
                m.audio = None;
                m.duration = r.available.unwrap_or(Tick::ZERO);
                m.name = r.name.clone();
                m
            });
            // Where the document says, else where BananaV saved it was.
            match path {
                Some(path) => m.source = MediaSource::File { path },
                None if saved.is_none() => {
                    m.source = MediaSource::File {
                        path: r.name.clone(),
                    }
                }
                None => {}
            }
            if let MediaRefSource::Sequence { count, .. } = r.source
                && m.sequence.is_none()
            {
                m.sequence = Some(crate::model::ImageSequence { count });
            }
            m
        }
    }
}

/// The settings BananaV saved of a track, or a new track's.
fn track_settings(p: &mut Project, dt: &DocTrack, ours: bool) -> TrackId {
    let id = p.add_track(dt.kind);
    let t = p.track_by_id_mut(id).expect("just added");
    if let Some(saved) = dt
        .saved
        .as_ref()
        .and_then(|v| serde_json::from_value::<Track>(v.clone()).ok())
        && saved.kind == dt.kind
    {
        *t = Track {
            id,
            clips: Vec::new(),
            ..saved
        };
    }
    if !dt.name.is_empty() {
        t.name = dt.name.clone();
    }
    t.muted = !dt.enabled;
    if ours {
        t.effects = dt.effects.clone();
    }
    id
}

/// Keeps a clip's fades within it.
fn clamp_fades(c: &mut Clip) {
    c.fade_in = c.fade_in.clamp(Tick::ZERO, c.duration);
    c.fade_out = c.fade_out.clamp(Tick::ZERO, c.duration - c.fade_in);
}

/// Cuts `cut` off the start of `c` (`false`: nothing is left).
fn trim_head(c: &mut Clip, cut: Tick) -> bool {
    if cut <= Tick::ZERO {
        return true;
    }
    c.start += cut;
    c.duration -= cut;
    if c.freeze.is_none() {
        c.source_in += c.to_source(cut);
    }
    c.fade_in = (c.fade_in - cut).max(Tick::ZERO);
    c.duration > Tick::ZERO
}

/// An event of another program's document playing a media's sound: its
/// track, id, start, end and stream.
type Playing = (usize, ClipId, Tick, Tick, u16);

struct Builder {
    p: Project,
    media: Vec<MediaId>,
    /// BananaV's groups then, as now.
    groups: HashMap<u64, GroupId>,
    /// The fades BananaV saved, of events whose crossfades may hide them.
    saved_fades: HashMap<ClipId, (Tick, Tick)>,
    /// Events of other programs, which may pair as video and its sound.
    foreign: HashSet<ClipId>,
    /// The document's groups of events (another program's links) as now.
    links: HashMap<usize, GroupId>,
    /// Events whose channels the document named.
    hinted: HashSet<ClipId>,
    media_markers: HashMap<MediaId, Vec<MediaMarker>>,
    skipped: usize,
}

impl Builder {
    /// Event `e` as a clip, or `None` when nothing of it is left.
    fn event(&mut self, e: &DocEvent) -> Option<Clip> {
        let media = self.media[e.media];
        let id = ClipId(self.p.alloc_id());
        let saved: Option<Clip> = e
            .saved
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        let mut c = match &saved {
            Some(s) => {
                let mut c = s.clone();
                c.id = id;
                c.media = media;
                c.group = s.group.map(|g| {
                    let p = &mut self.p;
                    *self
                        .groups
                        .entry(g.0)
                        .or_insert_with(|| GroupId(p.alloc_id()))
                });
                c
            }
            None => Clip::new(id, media, e.start, e.duration),
        };
        // The document's timing wins: another program may have edited it.
        c.start = e.start;
        c.duration = e.duration;
        c.freeze = e.frozen.then_some(e.source.max(Tick::ZERO));
        // Frozen, the media's time is the frame held: BananaV's own event
        // keeps where it starts in the media, for when it plays again.
        match &saved {
            Some(s) if e.frozen => c.source_in = s.source_in,
            // Its velocity or its reversal the document's times cannot say:
            // BananaV's own event keeps its own where the document kept it.
            Some(s)
                if (s.velocity().is_some() || s.reverse.is_some())
                    && (s.start, s.duration) == (e.start, e.duration) =>
            {
                c.source_in = s.source_in
            }
            _ => c.source_in = e.source,
        }
        if !e.frozen {
            let speed = e.speed.clamp(0.01, 100.0);
            let same = saved
                .as_ref()
                .is_some_and(|s| (s.rate - speed).abs() <= 1e-9 * s.rate.abs());
            if !same {
                c.rate = speed;
            }
        }
        c.muted = !e.enabled;
        c.fade_in = e.fade_in;
        c.fade_out = e.fade_out;
        if let Some(t) = e.transition {
            c.transition = t;
        }
        c.effects = e.effects.clone();
        if let Some(s) = &saved {
            self.saved_fades.insert(id, (s.fade_in, s.fade_out));
        } else {
            self.foreign.insert(id);
            // What another program's document says of it.
            if let Some(level) = e.level {
                c.level = level.max(0.0);
            }
            if let Some(curve) = e.curves[0] {
                c.fade_in_curve = curve;
            }
            if let Some(curve) = e.curves[1] {
                c.fade_out_curve = curve;
            }
            if let Some(ch) = e.channels
                && let Some(m) = self.p.media(media)
            {
                (c.audio_stream, c.channels) = channel_place(m, ch);
                self.hinted.insert(id);
            }
            if let Some(k) = e.link {
                let p = &mut self.p;
                let group = *self.links.entry(k).or_insert_with(|| GroupId(p.alloc_id()));
                c.group = Some(group);
            }
        }
        // Nothing before the timeline's start, nor (unless BananaV looped
        // it) before the media's.
        let early = -c.start;
        if early > Tick::ZERO && !trim_head(&mut c, early) {
            return None;
        }
        if c.source_in < Tick::ZERO && c.freeze.is_none() && (saved.is_none() || !c.looped) {
            let cut = c.to_timeline(-c.source_in);
            if !trim_head(&mut c, cut) {
                return None;
            }
            c.source_in = c.source_in.max(Tick::ZERO);
        }
        if c.duration <= Tick::ZERO {
            return None;
        }
        clamp_fades(&mut c);
        for (time, length, name) in &e.markers {
            let m = MediaMarker {
                time: (*time).max(Tick::ZERO),
                length: *length,
                name: name.clone(),
            };
            self.media_markers.entry(media).or_default().push(m);
        }
        Some(c)
    }

    /// Restores, where a crossfade hides them, the fades BananaV saved: the
    /// document had the crossfade there.
    fn hidden_fades(&mut self) {
        for t in &mut self.p.tracks {
            for i in 0..t.clips.len() {
                let Some(&(fade_in, fade_out)) = self.saved_fades.get(&t.clips[i].id) else {
                    continue;
                };
                let (cross_in, cross_out) = t.crossfades(&t.clips[i]);
                let c = &mut t.clips[i];
                if cross_in.is_some() {
                    c.fade_in = fade_in;
                }
                if cross_out.is_some() {
                    c.fade_out = fade_out;
                }
                clamp_fades(c);
            }
        }
    }

    /// Several tracks playing the same channels of one media at once while
    /// it has streams none plays: a document that could not say which
    /// stream each plays (VEGAS's names its first stream's channels for
    /// all). Each track takes the first stream the earlier ones playing
    /// with it at once leave, for all its events of the media.
    fn spread_streams(&mut self) {
        // Each media's events: track, id, start, end and stream.
        let mut by_media: HashMap<MediaId, Vec<Playing>> = HashMap::new();
        for (t, track) in self.p.tracks.iter().enumerate() {
            for c in track.clips.iter().filter(|c| self.hinted.contains(&c.id)) {
                by_media.entry(c.media).or_default().push((
                    t,
                    c.id,
                    c.start,
                    c.end(),
                    c.audio_stream,
                ));
            }
        }
        for (media, clips) in by_media {
            let streams = self
                .p
                .media(media)
                .map_or(1, |m| m.extra_audio.len() + usize::from(m.audio.is_some()));
            if streams < 2 || clips.iter().any(|c| c.4 != 0) {
                continue;
            }
            let mut tracks: Vec<usize> = clips.iter().map(|c| c.0).collect();
            tracks.sort_unstable();
            tracks.dedup();
            let mut given: HashMap<usize, u16> = HashMap::new();
            for &t in &tracks {
                let mut taken = HashSet::new();
                for a in clips.iter().filter(|a| a.0 == t) {
                    for b in clips.iter().filter(|b| b.0 != t && a.2 < b.3 && b.2 < a.3) {
                        if let Some(&s) = given.get(&b.0) {
                            taken.insert(s);
                        }
                    }
                }
                let stream = (0..streams as u16)
                    .find(|s| !taken.contains(s))
                    .unwrap_or(0);
                given.insert(t, stream);
            }
            for (t, id, ..) in clips {
                if let Some(c) = self.p.clip_mut(id) {
                    c.audio_stream = given[&t];
                }
            }
        }
    }

    /// Groups another program's video events with their sound (the same
    /// media, at the same time), as BananaV imports a file.
    fn pair(&mut self) {
        let half = self.p.settings.frame_rate.frame_duration().0 / 2;
        let near = |a: Tick, b: Tick| (a - b).0.abs() <= half;
        let loose: Vec<(TrackKind, ClipId, MediaId, Tick, Tick, Tick)> = self
            .p
            .clips()
            .filter(|(_, c)| self.foreign.contains(&c.id) && c.group.is_none())
            .map(|(t, c)| (t.kind, c.id, c.media, c.start, c.duration, c.source_in))
            .collect();
        let mut taken = HashSet::new();
        for v in loose.iter().filter(|l| l.0 == TrackKind::Video) {
            let sound: Vec<ClipId> = loose
                .iter()
                .filter(|a| {
                    a.0 == TrackKind::Audio
                        && !taken.contains(&a.1)
                        && a.2 == v.2
                        && near(a.3, v.3)
                        && near(a.4, v.4)
                        && near(a.5, v.5)
                })
                .map(|a| a.1)
                .collect();
            if sound.is_empty() {
                continue;
            }
            let g = GroupId(self.p.alloc_id());
            for id in std::iter::once(v.1).chain(sound) {
                taken.insert(id);
                if let Some(c) = self.p.clip_mut(id) {
                    c.group = Some(g);
                }
            }
        }
    }
}

/// The project of `doc`: `found[i]` is the media file `doc.media[i]`
/// names as the caller probed it (`None`: not found, or no file); `base`
/// the settings of a project of another program's document.
pub fn build(
    doc: &Document,
    found: &[Option<MediaInfo>],
    base: &ProjectSettings,
) -> Result<Imported, String> {
    let saved = doc
        .saved
        .as_ref()
        .and_then(|v| Project::from_json(&v.to_string()).ok());
    let format = doc
        .saved
        .as_ref()
        .and_then(|v| v.get("format_version")?.as_u64())
        .map_or(Project::FORMAT_VERSION, |v| v as u32);
    let ours = saved.is_some();
    let mut p = saved.unwrap_or_else(|| Project::new(settings_for(doc, found, base)));
    let bins = std::mem::take(&mut p.bins);
    let unused = std::mem::take(&mut p.media);
    p.tracks.clear();
    p.markers.clear();
    p.regions.clear();
    p.ruler_offset = doc.start;
    if let Some(fx) = &doc.output_fx {
        p.video_output_fx = fx.clone();
    }

    // How each media is used: on which kinds of tracks, how far into it.
    let mut used = vec![(false, false); doc.media.len()];
    let mut reach = vec![Tick::ZERO; doc.media.len()];
    for t in &doc.tracks {
        for e in &t.events {
            match t.kind {
                TrackKind::Video => used[e.media].0 = true,
                TrackKind::Audio => used[e.media].1 = true,
            }
            let end = if e.frozen {
                e.source + p.settings.frame_rate.frame_duration()
            } else {
                e.source + Tick((e.duration.0 as f64 * e.speed).round() as i64)
            };
            reach[e.media] = reach[e.media].max(end);
        }
    }
    let mut missing = Vec::new();
    let mut media = Vec::with_capacity(doc.media.len());
    let mut renumbered = HashMap::new();
    for (i, r) in doc.media.iter().enumerate() {
        let probed = found.get(i).cloned().flatten();
        let lost = probed.is_none() && !matches!(r.source, MediaRefSource::Generated(_));
        let mut info = media_info(r, probed, &p.settings);
        if lost {
            missing.push(r.name.clone());
            offline(&mut info, used[i], reach[i], &p.settings);
        }
        let id = p.add_media(info);
        if let Some(s) = &r.saved {
            renumbered.insert(s.id, id);
        }
        media.push(id);
    }
    for m in unused {
        let old = m.id;
        renumbered.insert(old, p.add_media(m));
    }
    p.bins = bins
        .into_iter()
        .map(|mut b| {
            b.media = b
                .media
                .iter()
                .filter_map(|m| renumbered.get(m).copied())
                .collect();
            b
        })
        .collect();

    let mut b = Builder {
        p,
        media,
        groups: HashMap::new(),
        saved_fades: HashMap::new(),
        foreign: HashSet::new(),
        links: HashMap::new(),
        hinted: HashSet::new(),
        media_markers: HashMap::new(),
        skipped: doc.skipped,
    };
    // A BananaV track's lanes become it again.
    let mut lanes: HashMap<(bool, u64), TrackId> = HashMap::new();
    let (mut video, mut audio) = (Vec::new(), Vec::new());
    for dt in &doc.tracks {
        let video_kind = dt.kind == TrackKind::Video;
        let known = dt.key.and_then(|k| lanes.get(&(video_kind, k)).copied());
        let id = match known {
            Some(id) => id,
            None => {
                let id = track_settings(&mut b.p, dt, ours);
                if let Some(k) = dt.key {
                    lanes.insert((video_kind, k), id);
                }
                if video_kind { &mut video } else { &mut audio }.push(id);
                id
            }
        };
        for e in &dt.events {
            let fits = b.p.media(b.media[e.media]).is_some_and(|m| m.has(dt.kind));
            let clip = if fits { b.event(e) } else { None };
            match clip {
                Some(c) => b.p.track_by_id_mut(id).expect("made above").clips.push(c),
                None => b.skipped += 1,
            }
        }
    }
    for t in &mut b.p.tracks {
        t.sort();
    }
    b.hidden_fades();
    b.spread_streams();
    b.pair();

    // The media's markers the events showed.
    for (id, marks) in std::mem::take(&mut b.media_markers) {
        if let Some(m) = b.p.media.iter_mut().find(|m| m.id == id) {
            for mark in marks {
                if !m.markers.contains(&mark) {
                    m.markers.push(mark);
                }
            }
            m.markers.sort_by_key(|k| (k.time, k.length));
        }
    }
    for m in &doc.markers {
        let id = MarkerId(b.p.alloc_id());
        let start = m.start.max(Tick::ZERO);
        if m.duration > Tick::ZERO {
            let name = if m.name.is_empty() {
                (b.p.regions.len() + 1).to_string()
            } else {
                m.name.clone()
            };
            b.p.regions.push(Region {
                id,
                range: TimeRange::new(start, m.duration),
                name,
            });
        } else {
            let name = if m.name.is_empty() {
                (b.p.markers.len() + 1).to_string()
            } else {
                m.name.clone()
            };
            b.p.markers.push(Marker {
                id,
                time: start,
                name,
            });
        }
    }
    b.p.markers.sort_by_key(|m| (m.time, m.id));
    b.p.regions.sort_by_key(|r| (r.range.start, r.id));

    // The video tracks from the top, as BananaV lists them.
    let order: Vec<TrackId> = video.iter().rev().chain(&audio).copied().collect();
    b.p.tracks
        .sort_by_key(|t| order.iter().position(|&id| id == t.id));

    // Through the project file's reading, so what an older BananaV saved
    // is brought up to date as when its project opens.
    let mut p = b.p;
    p.format_version = format;
    let p = Project::from_json(&p.to_json()).map_err(|e| e.to_string())?;
    p.check_invariants()?;
    Ok(Imported {
        project: p,
        missing,
        skipped: b.skipped,
    })
}
