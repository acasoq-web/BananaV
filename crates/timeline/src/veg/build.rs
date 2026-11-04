//! The project a read `.veg` makes, once its media are found.

use std::collections::HashMap;

use bv_core::{FrameRate, SampleRate, Tick, TimeRange};

use super::read::{DocEnvelope, DocMedia, DocMotion, DocMotionKey, Document};
use super::*;
use crate::bins::{BinId, MediaBin};
use crate::busses::SendTo;
use crate::cd::{CdIndex, CdTrack};
use crate::commands::CommandMarker;
use crate::envelope::{Envelope, EnvelopeKind, EnvelopePoint};
use crate::fx::{Effect, Transform, TransformKey};
use crate::model::{
    AudioStreamInfo, Clip, ClipId, CompositeMode, Generator, GroupId, MarkerId, MediaId, MediaInfo,
    MediaSource, PixelAspect, PixelFormat, Project, ProjectSettings, RulerFormat, Summary, Tempo,
    TrackKind, VideoStreamInfo,
};
use crate::motion::{Halo, HaloKey, HaloLook, TrackMotion};
use crate::space::{Box3d, Box3dKey};
use crate::takes::Take;

/// A project made from a `.veg`, and what it could not bring in.
#[derive(Clone, Debug)]
pub struct Imported {
    pub project: Project,
    /// The media files that were not found (their names): their events
    /// stay, offline.
    pub missing: Vec<String>,
    /// Events of media BananaV does not make (the project's generated
    /// media) and events without a take.
    pub skipped_events: usize,
    /// Effects of plug-ins BananaV does not have, envelopes it has no
    /// place for, audio effects BananaV does not have (or whose chain it
    /// could not read) left out.
    pub effects_left: usize,
    pub envelopes_left: usize,
    pub audio_fx_left: usize,
    /// The transitions of events' fades out: BananaV's stand at the
    /// entrances.
    pub transitions_left: usize,
}

/// BananaV's effects for an audio chain's plug-ins, counting those it does
/// not have (and a chain it could not read) in `left`.
fn audio_fx(chain: &[super::read::DocAudioFx], unread: bool, left: &mut usize) -> Vec<Effect> {
    *left += usize::from(unread);
    chain
        .iter()
        .filter_map(|x| {
            let mut e = super::audio::effect(x.class, &x.settings);
            *left += usize::from(e.is_none());
            if let Some(e) = e.as_mut().filter(|_| x.bypass) {
                e.enabled = false;
            }
            e
        })
        .collect()
}

/// The rate a file's frames per second stand for: NTSC's 1000/1001 rates
/// exactly, whole ones as they are.
fn frame_rate(fps: f64) -> Option<FrameRate> {
    if !(fps.is_finite() && fps > 0.5 && fps < 1000.0) {
        return None;
    }
    let ntsc = (fps * 1.001).round();
    if (ntsc * 1000.0 / 1001.0 - fps).abs() < 1e-4 && (fps - fps.round()).abs() > 1e-3 {
        return Some(FrameRate::new(ntsc as u32 * 1000, 1001));
    }
    if (fps - fps.round()).abs() < 1e-6 {
        return Some(FrameRate::new(fps.round() as u32, 1));
    }
    Some(FrameRate::new((fps * 1000.0).round() as u32, 1000))
}

/// A time of the file as its frame's when it is that, within the file's
/// step.
fn on_frame(t: Tick, rate: FrameRate) -> Tick {
    let near = rate.snap_nearest(t);
    if (near - t).abs() <= ticks(1) {
        near
    } else {
        t
    }
}

/// The settings of the file's project: `base`'s with its frame, rate,
/// pixel format, sample rate, tempo and ruler.
fn settings_of(doc: &Document, base: &ProjectSettings) -> ProjectSettings {
    let s = &doc.settings;
    let mut out = *base;
    if s.width > 0 && s.height > 0 {
        out.width = s.width;
        out.height = s.height;
    }
    if let Some(rate) = frame_rate(s.frame_rate) {
        out.frame_rate = rate;
    }
    out.pixel_format = match s.pixel_format {
        32 => PixelFormat::Float32Full,
        _ => PixelFormat::Full8Bit,
    };
    if (s.pixel_aspect - 1.0).abs() > 1e-4 && s.pixel_aspect > 0.0 {
        out.pixel_aspect = PixelAspect {
            num: (s.pixel_aspect * 10_000.0).round() as u32,
            den: 10_000,
        };
    }
    if (8_000..=384_000).contains(&s.sample_rate) {
        out.sample_rate = SampleRate(s.sample_rate);
    }
    if s.beats_per_minute.is_finite() && s.beats_per_minute > 0.0 {
        out.tempo = Tempo {
            beats_per_minute: s.beats_per_minute,
            beats_per_measure: u32::from(s.beats_per_measure.clamp(1, 32)),
            beat_note: match s.beat_note {
                1 | 2 | 4 | 8 | 16 | 32 => u32::from(s.beat_note),
                _ => 4,
            },
        };
    }
    out.ruler_format = match s.timecode {
        7 => Some(RulerFormat::Smpte30),
        8 => Some(RulerFormat::SmpteDrop),
        9 => Some(RulerFormat::SmpteEbu),
        _ => None,
    };
    out
}

/// The compositing modes the file numbers (Add 1, Source Alpha 4 and 3D
/// Source Alpha 12 measured; Subtract and Multiply between them, in VEGAS's
/// list).
fn composite_mode(n: u32) -> CompositeMode {
    match n {
        1 => CompositeMode::Add,
        2 => CompositeMode::Subtract,
        3 => CompositeMode::MultiplyMask,
        12 => CompositeMode::ThreeDSourceAlpha,
        _ => CompositeMode::SourceAlpha,
    }
}

pub(crate) fn composite_number(m: CompositeMode) -> u32 {
    match m {
        CompositeMode::Add => 1,
        CompositeMode::Subtract => 2,
        CompositeMode::MultiplyMask => 3,
        CompositeMode::ThreeDSourceAlpha => 12,
        _ => 4,
    }
}

/// A file's linear gain in dB.
fn decibels(gain: f64) -> f32 {
    if gain > 0.0 {
        (20.0 * gain.log10()) as f32
    } else {
        -120.0
    }
}

/// A Track Motion's keyframe as a framing of the frame-sized picture: the
/// file measures from half the frame's height (where) and from its height
/// (how big), and in radians.
fn motion_framing(k: &DocMotionKey, aspect: f64) -> Transform {
    let scale = k.size[0] / aspect;
    Transform {
        x: (k.position[0] / (2.0 * aspect)) as f32,
        y: (k.position[1] / 2.0) as f32,
        scale: scale as f32,
        stretch: if scale.abs() > 1e-9 {
            (k.size[1] / scale) as f32
        } else {
            1.0
        },
        rotation_deg: k.rotation[2].to_degrees() as f32,
        pivot: [
            (k.pivot[0] / (2.0 * k.size[0]).abs().max(1e-9)) as f32,
            (k.pivot[1] / (2.0 * k.size[1]).abs().max(1e-9)) as f32,
        ],
        ..Transform::default()
    }
}

/// The same keyframe as a box in space, for a 3D track.
fn motion_box(k: &DocMotionKey, aspect: f64) -> Box3d {
    Box3d {
        x: (k.position[0] / (2.0 * aspect)) as f32,
        y: (k.position[1] / 2.0) as f32,
        z: (k.position[2] / (2.0 * aspect)) as f32,
        width: (k.size[0] / aspect) as f32,
        height: k.size[1] as f32,
        rotation: k.rotation.map(|r| r.to_degrees() as f32),
        ..Box3d::default()
    }
}

/// A shadow's or a glow's keyframe as its look.
fn motion_look(k: &DocMotionKey, aspect: f64) -> HaloLook {
    let [blue, green, red, alpha] = k.color;
    HaloLook {
        x: (k.position[0] / (2.0 * aspect)) as f32,
        y: (k.position[1] / 2.0) as f32,
        size: k.size[0] as f32,
        blur: k.blur as f32,
        intensity: k.intensity as f32,
        color: [red, green, blue, alpha],
    }
}

/// A list of keyframes into a shadow or a glow, on when `on`.
fn motion_halo(keys: &[DocMotionKey], on: bool, aspect: f64, rate: FrameRate, was: Halo) -> Halo {
    let Some(first) = keys.first().filter(|_| on) else {
        return was;
    };
    let mut h = was;
    h.on = true;
    h.look = motion_look(first, aspect);
    if keys.len() > 1 {
        h.keys = keys
            .iter()
            .map(|k| HaloKey {
                at: on_frame(ticks(k.at.max(0)), rate),
                value: motion_look(k, aspect),
                curve: super::pancrop::curve(k.curve as u32),
            })
            .collect();
    }
    h
}

/// A track's motion from the file; `None` while it leaves the picture
/// where it is, without a shadow or a glow.
fn track_motion(m: &DocMotion, three_d: bool, rate: FrameRate) -> Option<TrackMotion> {
    let aspect = if m.aspect > 1e-6 { m.aspect } else { 1.0 };
    let (shadow_on, glow_on) = (m.flags & 8 != 0, m.flags & 0x10 != 0);
    let mut out = TrackMotion::default();
    let plain = Transform::default();
    let flat = out.space;
    if let Some(first) = m.moves.first() {
        if three_d {
            out.space = motion_box(first, aspect);
            if m.moves.len() > 1 {
                out.space_keys = m
                    .moves
                    .iter()
                    .map(|k| Box3dKey {
                        at: on_frame(ticks(k.at.max(0)), rate),
                        value: motion_box(k, aspect),
                        curve: super::pancrop::curve(k.curve as u32),
                    })
                    .collect();
            }
        } else {
            out.transform = motion_framing(first, aspect);
            if m.moves.len() > 1 {
                out.keys = m
                    .moves
                    .iter()
                    .map(|k| {
                        let mut key = TransformKey::new(
                            on_frame(ticks(k.at.max(0)), rate),
                            motion_framing(k, aspect),
                            super::pancrop::curve(k.curve as u32),
                        );
                        key.smoothness = (k.smoothness * 100.0) as f32;
                        key
                    })
                    .collect();
            }
        }
    }
    let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
    let still = out.keys.is_empty()
        && out.space_keys.is_empty()
        && near(out.transform.x, plain.x)
        && near(out.transform.y, plain.y)
        && near(out.transform.scale, plain.scale)
        && near(out.transform.stretch, plain.stretch)
        && near(out.transform.rotation_deg, plain.rotation_deg)
        && out.space == flat;
    if still && !shadow_on && !glow_on {
        return None;
    }
    out.shadow = motion_halo(&m.shadow, shadow_on, aspect, rate, out.shadow);
    out.glow = motion_halo(&m.glow, glow_on, aspect, rate, out.glow);
    Some(out)
}

/// A file's envelope kind on a track, as BananaV's (`None`: one it has no
/// place for): volume, pan, a bus's send (A is 3), the compositing level,
/// the fade to color and the mute.
fn envelope_kind(n: i32, kind: TrackKind, busses: &[crate::busses::BusId]) -> Option<EnvelopeKind> {
    match (n, kind) {
        (0, TrackKind::Audio) => Some(EnvelopeKind::Volume),
        (1, TrackKind::Audio) => Some(EnvelopeKind::Pan),
        (3..=28, TrackKind::Audio) => busses
            .get((n - 3) as usize)
            .map(|&b| EnvelopeKind::Send(SendTo::Bus(b))),
        (29, TrackKind::Video) => Some(EnvelopeKind::CompositeLevel),
        (30, TrackKind::Video) => Some(EnvelopeKind::FadeToColor),
        (31, _) => Some(EnvelopeKind::Mute),
        _ => None,
    }
}

fn envelope(e: &DocEnvelope, kind: EnvelopeKind) -> Option<Envelope> {
    let mut points: Vec<EnvelopePoint> = e
        .points
        .iter()
        .filter(|p| p.1.is_finite())
        .map(|&(at, value, curve)| EnvelopePoint {
            at: ticks(at.max(0)),
            value,
            curve: key_curve(curve),
        })
        .collect();
    points.sort_by_key(|p| p.at);
    points.dedup_by_key(|p| p.at);
    if points.is_empty() {
        return None;
    }
    let mut env = Envelope::new(kind);
    env.points = points;
    Some(env)
}

/// The media `m` stands for: the file found, or an offline one where the
/// project says it is, its streams the ones its events use.
fn media_info(
    m: &DocMedia,
    found: Option<MediaInfo>,
    used: (bool, bool),
    reach: Tick,
    s: &ProjectSettings,
) -> MediaInfo {
    if let Some(f) = found {
        return f;
    }
    let name = m
        .path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(&m.path)
        .to_string();
    let mut info = MediaInfo::generated(Generator::ColorBars, s);
    info.source = MediaSource::File {
        path: m.path.clone(),
    };
    info.name = name;
    info.video = used.0.then_some(VideoStreamInfo {
        width: s.width,
        height: s.height,
        frame_rate: s.frame_rate,
        pixel_aspect: Default::default(),
        field_order: Default::default(),
        spherical: false,
        full_range: false,
        reading: Default::default(),
    });
    info.audio = used.1.then_some(AudioStreamInfo {
        sample_rate: s.sample_rate,
        channels: s.channels.channel_count() as u16,
    });
    info.still = false;
    info.duration = reach.max(s.frame_rate.frame_duration());
    info
}

/// An audio event's channels from the file's number: as they are, one
/// side switched off, mixed to one or swapped (the rest, a side muted, has no
/// place here).
fn event_channels(n: u32) -> crate::model::ChannelMap {
    use crate::model::ChannelMap as C;
    match n {
        1001 => C::RightOnly,
        1002 => C::LeftOnly,
        1005 => C::Combine,
        1006 => C::Swap,
        _ => C::Both,
    }
}

/// A video event's resample mode from its switches: the project's, the
/// flow's, or the frames mixed or not.
fn video_resample(flags: u32) -> Option<crate::model::ResampleMode> {
    use crate::model::ResampleMode as R;
    if flags & 0x200 != 0 {
        None
    } else if flags & 0x800 != 0 {
        Some(R::OpticalFlow)
    } else if flags & 0x480 != 0 {
        Some(R::FrameBlend)
    } else {
        Some(R::Disable)
    }
}

/// A transition's progress envelope that says nothing: straight from 0 to 1.
fn progress_is_straight(e: &DocEnvelope) -> bool {
    matches!(e.points.as_slice(),
        [(0, a, 1), (1_000_000, b, _)] if a.abs() < 1e-6 && (b - 1.0).abs() < 1e-6)
}

/// Makes the project. `found[i]` is `doc.media[i]` probed (`None`: not
/// found); `effect` turns another program's effect into BananaV's
/// (`None`: no plug-in of its own for it), at the project's frame rate, in
/// which its keyframes count.
pub fn build(
    doc: &Document,
    found: &[Option<MediaInfo>],
    base: &ProjectSettings,
    effect: &dyn Fn(&DocEffect, FrameRate) -> Option<Effect>,
) -> Imported {
    let settings = settings_of(doc, base);
    let mut p = Project::new(settings);
    p.tracks.clear();
    let mut out = Imported {
        project: Project::new(settings),
        missing: Vec::new(),
        skipped_events: 0,
        effects_left: 0,
        envelopes_left: 0,
        audio_fx_left: 0,
        transitions_left: 0,
    };
    let rate = p.settings.frame_rate;
    let fx = |list: &[&DocEffect], left: &mut usize| -> Vec<Effect> {
        list.iter()
            .filter_map(|e| {
                let mut made = effect(e, rate);
                match &mut made {
                    Some(m) => m.enabled = m.enabled && !e.bypass,
                    None => *left += 1,
                }
                made
            })
            .collect()
    };

    // How each media is used: on which kinds of tracks, how far into it.
    let index: HashMap<u32, usize> = doc
        .media
        .iter()
        .enumerate()
        .map(|(i, m)| (m.id, i))
        .collect();
    let mut used = vec![(false, false); doc.media.len()];
    let mut reach = vec![Tick::ZERO; doc.media.len()];
    for t in &doc.tracks {
        for e in &t.events {
            for take in &e.takes {
                if let Some(&i) = index.get(&take.media) {
                    if t.kind == VIDEO {
                        used[i].0 = true;
                    } else {
                        used[i].1 = true;
                    }
                    reach[i] = reach[i].max(ticks(take.offset.max(0) + take.remain.max(0)));
                }
            }
        }
    }
    let mut media: HashMap<u32, MediaId> = HashMap::new();
    for (i, m) in doc.media.iter().enumerate() {
        if m.generated {
            continue;
        }
        let probed = found.get(i).cloned().flatten();
        if probed.is_none() {
            out.missing.push(
                m.path
                    .rsplit(['\\', '/'])
                    .next()
                    .unwrap_or(&m.path)
                    .to_string(),
            );
        }
        let info = media_info(m, probed, used[i], reach[i], &p.settings);
        media.insert(m.id, p.add_media(info));
    }

    // The busses, A to Z, before the tracks send to them.
    for _ in 0..doc.busses.min(26) {
        p.add_bus();
    }
    let busses: Vec<_> = p.busses.iter().map(|b| b.id).collect();

    // Each group's events, by their ids.
    let mut groups: HashMap<u64, GroupId> = HashMap::new();
    for members in doc.groups.iter().filter(|m| m.len() > 1) {
        let g = GroupId(p.alloc_id());
        for &id in members {
            groups.insert(id, g);
        }
    }

    let mut order = Vec::new();
    for t in &doc.tracks {
        let kind = match t.kind {
            AUDIO => TrackKind::Audio,
            VIDEO => TrackKind::Video,
            _ => continue,
        };
        let id = p.add_track(kind);
        order.push(id);
        let mut clips = Vec::new();
        for e in &t.events {
            let active = if (0..e.takes.len() as i32).contains(&e.active) {
                e.active as usize
            } else {
                0
            };
            let Some(take) = e.takes.get(active) else {
                out.skipped_events += 1;
                continue;
            };
            let Some(&m) = media.get(&take.media) else {
                out.skipped_events += 1;
                continue;
            };
            // The file's 100 ns steps round a frame's time: on the frame's
            // again, or a cut lands a frame late.
            let start = on_frame(ticks(e.start), rate);
            let duration = (on_frame(ticks(e.start + e.length.max(1)), rate) - start).max(ticks(1));
            let mut c = Clip::new(ClipId(p.alloc_id()), m, start, duration);
            c.source_in = ticks(take.offset);
            c.muted = e.muted;
            c.rate = e.speed;
            c.level = if kind == TrackKind::Video {
                e.level.min(1.0)
            } else {
                e.level
            };
            c.fade_in = ticks(e.fade_in.max(0)).min(duration);
            c.fade_out = ticks(e.fade_out.max(0)).min(duration);
            c.fade_in_curve = fade_curve(e.curves[0]);
            c.fade_out_curve = fade_curve(e.curves[1]);
            c.group = groups.get(&e.id).copied();
            if kind == TrackKind::Audio && e.pitch.abs() > 1e-9 {
                c.pitch = e.pitch as f32;
            }
            // Its takes, the active one's place theirs start from.
            let takes: Vec<Take> = e
                .takes
                .iter()
                .filter_map(|k| {
                    Some(Take {
                        media: *media.get(&k.media)?,
                        offset: ticks(k.offset) - ticks(take.offset),
                        name: (!k.name.is_empty()).then(|| k.name.clone()),
                        none: false,
                    })
                })
                .collect();
            if takes.len() > 1 {
                c.active_take = takes
                    .iter()
                    .position(|k| k.media == m && k.offset == Tick::ZERO)
                    .unwrap_or(0);
                c.takes = takes;
            }
            // Those that work before its Pan/Crop first, as its chain has them.
            let (before, after): (Vec<&DocEffect>, Vec<&DocEffect>) =
                e.effects.iter().partition(|x| x.before_pan_crop);
            c.effects = fx(&before, &mut out.effects_left);
            c.pan_crop_at = c.effects.len();
            c.effects.extend(fx(&after, &mut out.effects_left));
            c.effects.extend(audio_fx(
                &e.audio_fx,
                e.audio_unread,
                &mut out.audio_fx_left,
            ));
            for env in &e.envelopes {
                let made = (env.kind == VELOCITY && kind == TrackKind::Video)
                    .then(|| envelope(env, EnvelopeKind::Velocity))
                    .flatten();
                match made {
                    Some(made) => c.envelopes.push(made),
                    None => out.envelopes_left += 1,
                }
            }
            match kind {
                TrackKind::Video => {
                    if let Some(pan_crop) = &e.pan_crop {
                        c.pan_crop = pan_crop.clone();
                    }
                    c.pan_crop.keep_aspect = e.keep_aspect;
                    c.mask.clone_from(&e.mask);
                    c.undersample = e.undersample as f32;
                    c.reduce_flicker = e.video_flags & 0x100 != 0;
                    c.resample = video_resample(e.video_flags);
                    if let Some(t) = &e.transition_in {
                        match effect(t, rate).and_then(|x| x.plugin) {
                            Some(made) => {
                                c.transition_fx = Some(made);
                                c.transition_bypass = t.bypass;
                            }
                            None => out.effects_left += 1,
                        }
                    }
                    out.transitions_left += usize::from(e.transition_out.is_some());
                    // Its progress over the fade: the file's places are its
                    // parts of a million.
                    if let Some(env) = &e.progress_in
                        && e.transition_in.is_some()
                        && !progress_is_straight(env)
                    {
                        let across = DocEnvelope {
                            kind: env.kind,
                            points: env
                                .points
                                .iter()
                                .map(|&(place, value, curve)| {
                                    let share = place as f64 / 1_000_000.0;
                                    let at = (e.fade_in.max(0) as f64 * share).round() as i64;
                                    (at, value, curve)
                                })
                                .collect(),
                        };
                        c.envelopes
                            .extend(envelope(&across, EnvelopeKind::TransitionProgress));
                    }
                }
                TrackKind::Audio => {
                    c.normalize = e.normalize;
                    c.invert_phase = e.invert_phase;
                    c.channels = event_channels(e.channels);
                }
            }
            clips.push(c);
        }
        clips.sort_by_key(|c| (c.start, c.id.0));
        let track_fx: Vec<&DocEffect> = t.effects.iter().collect();
        let mut effects = fx(&track_fx, &mut out.effects_left);
        effects.extend(audio_fx(
            &t.audio_fx,
            t.audio_unread,
            &mut out.audio_fx_left,
        ));
        let mut envelopes = Vec::new();
        for e in &t.envelopes {
            match envelope_kind(e.kind, kind, &busses).and_then(|k| envelope(e, k)) {
                Some(env) => envelopes.push(env),
                None => out.envelopes_left += 1,
            }
        }
        if let Ok(track) = p.track_mut(id) {
            track.name = t.name.clone();
            track.muted = t.muted;
            match kind {
                TrackKind::Audio => {
                    track.level = t.volume;
                    track.pan = t.pan;
                    track.invert_phase = t.invert_phase;
                    for (k, &gain) in t.sends.iter().enumerate() {
                        if let (Some(&bus), true) = (busses.get(k), gain.is_finite() && gain > 0.0)
                        {
                            track.sends.push(crate::busses::Send {
                                to: SendTo::Bus(bus),
                                gain_db: decibels(f64::from(gain)),
                                pre_volume: false,
                                link_pan: true,
                                held: 1.0,
                            });
                        }
                    }
                }
                TrackKind::Video => {
                    track.level = t.composite_level;
                    track.composite_mode = composite_mode(t.composite_mode);
                    track.composite_depth = t.composite_depth;
                    let three_d = track.composite_mode == CompositeMode::ThreeDSourceAlpha;
                    track.motion = t
                        .motion
                        .as_ref()
                        .and_then(|m| track_motion(m, three_d, rate));
                    track.parent_motion = t
                        .parent_motion
                        .as_ref()
                        .and_then(|m| track_motion(m, false, rate));
                }
            }
            track.clips = clips;
            track.effects = effects;
            // A send's envelope rides on its send.
            for e in &envelopes {
                if let EnvelopeKind::Send(to) = e.kind
                    && !track.sends.iter().any(|s| s.to == to)
                {
                    track.sends.push(crate::busses::Send {
                        to,
                        gain_db: 0.0,
                        pre_volume: false,
                        link_pan: true,
                        held: 1.0,
                    });
                }
            }
            track.envelopes = envelopes;
        }
    }
    // The tracks in the file's order.
    p.tracks
        .sort_by_key(|t| order.iter().position(|&o| o == t.id));

    // The mixer's busses: their faders, switches, effects and envelopes.
    for b in &doc.bus_list {
        match b.id {
            2 => {
                p.master.gain_db = decibels(b.gain);
                p.master.muted = b.muted;
                p.master_fx = audio_fx(&b.effects, false, &mut out.audio_fx_left);
                out.envelopes_left += b.envelopes.len();
            }
            id if id >= 3 && id != 5 && id != 7 => {
                let Some(bus) = p.busses.get_mut(id as usize - 3) else {
                    continue;
                };
                let c = &mut bus.channel;
                c.gain_db = decibels(b.gain);
                c.muted = b.muted;
                c.solo = b.solo;
                c.effects = audio_fx(&b.effects, false, &mut out.audio_fx_left);
                let default = format!("Bus {}", crate::busses::bus_letter(id as usize - 3));
                if !b.name.is_empty() && b.name != default {
                    c.name.clone_from(&b.name);
                }
                for e in &b.envelopes {
                    let made = match e.kind {
                        0 => envelope(e, EnvelopeKind::Volume),
                        1 => envelope(e, EnvelopeKind::Pan),
                        31 => envelope(e, EnvelopeKind::Mute),
                        _ => None,
                    };
                    match made {
                        Some(made) => c.envelopes.push(made),
                        None => out.envelopes_left += 1,
                    }
                }
            }
            _ => {}
        }
    }

    for m in &doc.markers {
        let name = m.label.clone();
        if m.region {
            let id = p.add_region(TimeRange::new(ticks(m.time), ticks(m.length.max(0))));
            if let Some(r) = p.regions.iter_mut().find(|r| r.id == id) {
                r.name = name;
            }
        } else {
            let id = p.add_marker(ticks(m.time));
            if let Some(k) = p.markers.iter_mut().find(|k| k.id == id) {
                k.name = name;
            }
        }
    }
    for c in &doc.commands {
        let id = MarkerId(p.alloc_id());
        p.commands.push(CommandMarker {
            id,
            time: ticks(c.time),
            command: command_in(&c.command),
            parameter: c.parameter.clone(),
            comment: c.comment.clone(),
        });
    }
    for c in &doc.cd {
        let id = MarkerId(p.alloc_id());
        if c.index {
            p.cd_indices.push(CdIndex {
                id,
                time: ticks(c.time),
            });
        } else {
            p.cd_tracks.push(CdTrack {
                id,
                range: TimeRange::new(ticks(c.time), ticks(c.length.max(0))),
                name: c.name.clone(),
                protect: false,
                emphasis: false,
                isrc: String::new(),
            });
        }
    }
    let [title, artist, engineer, copyright, comments] = doc.summary.clone();
    p.summary = Summary {
        title,
        artist,
        engineer,
        copyright,
        comments,
    };
    let ids: Vec<BinId> = doc.bins.iter().map(|_| BinId(p.alloc_id())).collect();
    p.bins = doc
        .bins
        .iter()
        .zip(&ids)
        .map(|(b, &id)| MediaBin {
            id,
            name: b.name.clone(),
            parent: b.parent.map(|i| ids[i]),
            media: b
                .media
                .iter()
                .filter_map(|m| media.get(m).copied())
                .collect(),
        })
        .collect();
    let output_fx: Vec<&DocEffect> = doc.output_fx.iter().collect();
    p.video_output_fx = fx(&output_fx, &mut out.effects_left);
    // The video bus's: the fade to color over everything, its motion blur
    // and its supersampling.
    for e in &doc.envelopes {
        let made = match e.kind {
            30 => envelope(e, EnvelopeKind::FadeToColor),
            32 => envelope(e, EnvelopeKind::MotionBlur),
            33 => envelope(e, EnvelopeKind::Supersampling),
            _ => None,
        };
        match made {
            Some(made) => p.video_bus.envelopes.push(made),
            None => out.envelopes_left += 1,
        }
    }
    out.project = p;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_rates_come_back_exact() {
        assert_eq!(frame_rate(30.0), Some(FrameRate::new(30, 1)));
        assert_eq!(frame_rate(25.0), Some(FrameRate::new(25, 1)));
        assert_eq!(
            frame_rate(29.970_029_970_029_97),
            Some(FrameRate::new(30_000, 1001))
        );
        assert_eq!(
            frame_rate(23.976_023_976),
            Some(FrameRate::new(24_000, 1001))
        );
        assert_eq!(
            frame_rate(59.940_059_94),
            Some(FrameRate::new(60_000, 1001))
        );
        assert_eq!(frame_rate(0.0), None);
    }
}
