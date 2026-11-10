//! A `.veg` read: its chunks' structures decoded into a [`Document`].

use super::riff::{self, Chunk, VegError};
use super::*;

/// A media of the project: a file (its path as the project has it) or a
/// generated one (`META:\…`), by the id its takes use.
#[derive(Clone, Debug, PartialEq)]
pub struct DocMedia {
    pub id: u32,
    pub path: String,
    pub generated: bool,
    /// A generated one's generator: its settings and its picture's.
    pub generator: Option<DocGenerator>,
}

/// What a generated media keeps: how long it is, its picture's size and
/// pixel shape, its fields, alpha and rotation, and the plug-in that draws
/// it (an OpenFX generator's settings, or the record of one of the file's
/// own kind with its keyframes).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocGenerator {
    /// In 100 ns.
    pub duration: i64,
    /// Its number among the project's generated media (its path's `(n)`).
    pub number: i64,
    /// The plug-in's name as the file has it ("VEGAS Solid Color").
    pub name: String,
    /// Its pictures' size in pixels, and the size asked for (the pixels
    /// are as many as that size takes at its pixel aspect).
    pub width: i32,
    pub height: i32,
    pub asked: (i32, i32),
    /// Its pixels' aspect (width over height).
    pub pixel_aspect: f64,
    /// Its stream's number and how many times its length or size changed
    /// (the file keeps them; written back as read).
    pub stream_id: i64,
    pub changes: u32,
    /// The plug-in, first, and any others the file lists after it.
    pub effects: Vec<DocEffect>,
    /// Its stream's properties: the field order (0 progressive, 1 upper
    /// first, 2 lower first), the alpha channel (0 none, 1 straight, 2
    /// premultiplied, 3 premultiplied dirty), the rotation (0, 1 = 90°
    /// clockwise, 2 = 180°, 3 = 90° counterclockwise) and whether they are
    /// the media's own (not changed by the user).
    pub field_order: u32,
    pub alpha: u32,
    pub rotation: u32,
    pub automatic: bool,
}

/// The project's settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DocSettings {
    pub width: u32,
    pub height: u32,
    pub frame_rate: f64,
    /// 28: 8-bit full range; 32: 32-bit floating point full range.
    pub pixel_format: u32,
    pub pixel_aspect: f64,
    /// The ruler's timecode: 7 SMPTE 30, 8 drop frame, 9 EBU.
    pub timecode: u32,
    pub beats_per_minute: f64,
    pub beats_per_measure: u16,
    pub beat_note: u16,
    pub sample_rate: u32,
    pub prerender_folder: String,
    pub recorded_folder: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocTake {
    pub media: u32,
    pub stream: u32,
    /// Where it starts in its media.
    pub offset: i64,
    /// How much of its media is left after that.
    pub remain: i64,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocEnvelope {
    pub kind: i32,
    /// Time, value, curve to the next.
    pub points: Vec<(i64, f32, i32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocEvent {
    pub id: u64,
    pub start: i64,
    pub length: i64,
    pub muted: bool,
    pub name: String,
    pub speed: f64,
    pub fade_in: i64,
    pub fade_out: i64,
    /// The fade in's and the fade out's curves, then their reciprocal
    /// curves.
    pub curves: [i32; 4],
    /// Its level (the gain its fades go to and come from): its volume, a
    /// video event's opacity.
    pub level: f32,
    /// Which of `takes` plays (−1: none).
    pub active: i32,
    pub takes: Vec<DocTake>,
    /// Audio: its pitch change, in semitones.
    pub pitch: f64,
    pub envelopes: Vec<DocEnvelope>,
    pub effects: Vec<DocEffect>,
    /// Audio: its effects' chain, and whether a chain could not be read.
    pub audio_fx: Vec<DocAudioFx>,
    pub audio_unread: bool,
    /// Its Pan/Crop, when it has one of its own, and its masks.
    pub pan_crop: Option<crate::pancrop::PanCrop>,
    pub mask: Option<crate::mask::PanCropMask>,
    /// Video: its Pan/Crop's Maintain aspect ratio, kept apart from it.
    pub keep_aspect: bool,
    /// Video: its switches' word.
    pub video_flags: u32,
    /// Video: the share of its frames it shows.
    pub undersample: f64,
    /// Video: the transitions of its fade in and of its fade out, and
    /// their progress envelopes.
    pub transition_in: Option<DocEffect>,
    pub transition_out: Option<DocEffect>,
    pub progress_in: Option<DocEnvelope>,
    pub progress_out: Option<DocEnvelope>,
    /// Audio: the gain Normalize gave it, when it is on.
    pub normalize: Option<f32>,
    /// Audio: how its channels play (1000 as they are, 1005 mixed to one).
    pub channels: u32,
    pub invert_phase: bool,
}

/// A keyframe of a Track Motion: its curve and time (100 ns), and in the
/// file's terms (half the height's and the height's parts, radians) where
/// it turns around, where it is, its size and its rotation.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DocMotionKey {
    pub curve: i32,
    pub at: i64,
    pub pivot: [f64; 3],
    pub position: [f64; 3],
    pub size: [f64; 3],
    pub rotation: [f64; 3],
    pub smoothness: f64,
    /// A shadow's or a glow's: blue, green, red, alpha.
    pub color: [f32; 4],
    pub blur: f64,
    pub intensity: f64,
}

/// A track's motion: its switches' word, the project's shape it was made
/// for (1 in a parent's) and its three lists of keyframes.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DocMotion {
    pub flags: u32,
    pub aspect: f64,
    pub moves: Vec<DocMotionKey>,
    pub shadow: Vec<DocMotionKey>,
    pub glow: Vec<DocMotionKey>,
}

/// An audio bus of the mixer: which (2 the Master, 7 the Preview, 3 on the
/// ones made), its fader's gain, its switches, its name, its effects and
/// its envelopes.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DocBus {
    pub id: u32,
    pub name: String,
    pub gain: f64,
    pub muted: bool,
    pub solo: bool,
    pub effects: Vec<DocAudioFx>,
    pub envelopes: Vec<DocEnvelope>,
}

/// An audio plug-in of a chain: its class (a DirectX plug-in's CLSID), its
/// name and its settings (their format's GUID, then the plug-in's own).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocAudioFx {
    pub class: riff::Guid,
    pub name: String,
    pub settings: Vec<u8>,
    pub bypass: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocTrack {
    pub kind: u32,
    pub name: String,
    pub muted: bool,
    pub volume: f32,
    pub pan: f32,
    /// Video: its compositing mode (4 Source Alpha) and level.
    pub composite_mode: u32,
    pub composite_level: f32,
    /// Audio: its polarity reversed.
    pub invert_phase: bool,
    /// Audio: its send to each bus, as a linear gain (0: none).
    pub sends: Vec<f32>,
    /// Video: how far in it is among compositing children.
    pub composite_depth: u8,
    /// Video: its Track Motion and its Parent Motion.
    pub motion: Option<DocMotion>,
    pub parent_motion: Option<DocMotion>,
    pub events: Vec<DocEvent>,
    pub envelopes: Vec<DocEnvelope>,
    pub effects: Vec<DocEffect>,
    /// Audio: its effects' chain (a new track's has three), and whether
    /// it could not be read.
    pub audio_fx: Vec<DocAudioFx>,
    pub audio_unread: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocMarker {
    pub time: i64,
    pub length: i64,
    pub number: u32,
    pub region: bool,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocCommand {
    pub time: i64,
    pub comment: String,
    pub command: String,
    pub parameter: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocCd {
    pub time: i64,
    pub length: i64,
    pub index: bool,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocBin {
    pub name: String,
    pub parent: Option<usize>,
    pub media: Vec<u32>,
}

/// A project read from a `.veg`, its media still to be found (see
/// [`super::build`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    /// Its format's version (0x160000: VEGAS Pro 23; 0x170000: 2026).
    pub version: u32,
    pub media: Vec<DocMedia>,
    pub(crate) settings: DocSettings,
    pub(crate) tracks: Vec<DocTrack>,
    pub(crate) markers: Vec<DocMarker>,
    pub(crate) commands: Vec<DocCommand>,
    pub(crate) cd: Vec<DocCd>,
    /// Each group's events.
    pub(crate) groups: Vec<Vec<u64>>,
    /// Title, artist, engineer, copyright, comments.
    pub(crate) summary: [String; 5],
    /// The audio busses made besides the Master.
    pub(crate) busses: usize,
    /// The mixer's busses as the file keeps them.
    pub(crate) bus_list: Vec<DocBus>,
    pub(crate) bins: Vec<DocBin>,
    pub(crate) output_fx: Vec<DocEffect>,
    pub(crate) envelopes: Vec<DocEnvelope>,
}

// Little-endian numbers at an offset; 0 past the end.
fn u16_at(b: &[u8], at: usize) -> u16 {
    b.get(at..at + 2)
        .map_or(0, |s| u16::from_le_bytes([s[0], s[1]]))
}

pub(crate) fn u32_at(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4)
        .map_or(0, |s| u32::from_le_bytes(s.try_into().expect("4 bytes")))
}

pub(super) fn i32_at(b: &[u8], at: usize) -> i32 {
    u32_at(b, at) as i32
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    b.get(at..at + 8)
        .map_or(0, |s| u64::from_le_bytes(s.try_into().expect("8 bytes")))
}

pub(super) fn i64_at(b: &[u8], at: usize) -> i64 {
    u64_at(b, at) as i64
}

fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_bits(u32_at(b, at))
}

pub(super) fn f64_at(b: &[u8], at: usize) -> f64 {
    f64::from_bits(u64_at(b, at))
}

fn guid_at(b: &[u8], at: usize) -> Option<riff::Guid> {
    b.get(at..at.checked_add(16)?)
        .map(|s| riff::Guid(s.try_into().expect("16 bytes")))
}

/// The plug-ins of an audio effects chain. After its header — its kind,
/// a version, a plug-in's class and name, two numbers, the sound's format
/// (a `WAVEFORMATEX` and what it adds) and a number — their count and a
/// number, then each its class, a number, its name, its class again, its
/// settings' size twice and its settings. `None` where they do not fit.
pub(super) fn audio_chain(b: &[u8]) -> Option<Vec<DocAudioFx>> {
    let name = u32_at(b, 0x24) as usize;
    if b.len() < 0x28 || name > 1024 {
        return None;
    }
    let format = 0x28 + name + 8;
    let mut at = format + 18 + usize::from(u16_at(b, format + 16)) + 4;
    let count = u32_at(b, at) as usize;
    at += 8;
    let mut out = Vec::new();
    for _ in 0..count.min(256) {
        let class = guid_at(b, at)?;
        let bypass = u32_at(b, at + 16) != 0;
        let len = u32_at(b, at + 20) as usize;
        if len > 1024 {
            return None;
        }
        let name = text_at(b, at + 24, len);
        let p = at + 24 + len;
        if guid_at(b, p)? != class {
            return None;
        }
        let size = u32_at(b, p + 16) as usize;
        let settings = b.get(p + 24..p + 24 + size)?.to_vec();
        out.push(DocAudioFx {
            class,
            name,
            settings,
            bypass,
        });
        at = p + 24 + size;
    }
    Some(out)
}

/// UTF-16 text of `bytes` bytes at `at`, up to its zero.
fn text_at(b: &[u8], at: usize, bytes: usize) -> String {
    let end = at.saturating_add(bytes).min(b.len());
    let units: Vec<u16> = b
        .get(at..end)
        .unwrap_or(&[])
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&c| c != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// Texts one after the other from `at`, each `lens[i]` bytes; where they
/// end.
fn texts_at(b: &[u8], at: usize, lens: &[u32]) -> (Vec<String>, usize) {
    let mut at = at;
    let mut out = Vec::with_capacity(lens.len());
    for &n in lens {
        out.push(text_at(b, at, n as usize));
        at += n as usize;
    }
    (out, at)
}

/// A list of tags (`u32` 8, `u32` n, then each a four-letter code, a
/// `u32` byte count and its text).
fn tags(b: &[u8]) -> Vec<([u8; 4], String)> {
    let n = u32_at(b, 4) as usize;
    let mut at = u32_at(b, 0) as usize;
    let mut out = Vec::new();
    for _ in 0..n.min(4096) {
        if at + 8 > b.len() {
            break;
        }
        let code = [b[at], b[at + 1], b[at + 2], b[at + 3]];
        let len = u32_at(b, at + 4) as usize;
        out.push((code, text_at(b, at + 8, len)));
        at += 8 + len;
    }
    out
}

fn settings(header: &[u8], mixer: Option<&[u8]>) -> DocSettings {
    let cb = u32_at(header, 0) as usize;
    let lens = [0x3C, 0x40, 0x44, 0x6C].map(|o| u32_at(header, o));
    let (_, after) = texts_at(header, cb, &lens);
    let items = u32_at(header, 0x38) as usize;
    let mut s = DocSettings {
        frame_rate: f64_at(header, 0x10),
        timecode: u32_at(header, 0x08),
        beats_per_minute: f64_at(header, 0x18),
        beat_note: u16_at(header, 0x28),
        beats_per_measure: u16_at(header, 0x2A),
        sample_rate: 48_000,
        pixel_format: 28,
        pixel_aspect: 1.0,
        ..Default::default()
    };
    // The first video item is the project's.
    if items > 0 && u32_at(header, after) == 0x358 {
        let item = &header[after..];
        s.width = u32_at(item, 0x04);
        s.height = u32_at(item, 0x08);
        s.pixel_format = u32_at(item, 0x0C);
        let fps = f64_at(item, 0x38);
        if fps > 0.0 {
            s.frame_rate = fps;
        }
        let aspect = f64_at(item, 0x40);
        if aspect > 0.0 {
            s.pixel_aspect = aspect;
        }
    }
    if let Some(m) = mixer
        && m.len() >= 0x6E8
    {
        let rate = u32_at(m, 0x6E4);
        if rate > 0 {
            s.sample_rate = rate;
        }
    }
    s
}

fn media(b: &[u8]) -> DocMedia {
    let cb = u32_at(b, 0) as usize;
    let path = text_at(b, cb, u32_at(b, 0x24) as usize);
    let generated = path.to_ascii_uppercase().starts_with("META:\\");
    // The path, then three other texts.
    let after = cb
        + [0x24, 0x28, 0x2C, 0x40]
            .iter()
            .map(|&at| u32_at(b, at) as usize)
            .sum::<usize>();
    DocMedia {
        id: u32_at(b, 0x3C),
        generator: generated.then(|| generator(b, after)).flatten(),
        generated,
        path,
    }
}

/// A generated media's part from `at`: its generator's header (its length,
/// number and name), its picture's stream (its size and pixel aspect), its
/// effects — the generator's first — and its stream's properties.
fn generator(b: &[u8], at: usize) -> Option<DocGenerator> {
    if u32_at(b, at) != 0x224 {
        return None;
    }
    let mut g = DocGenerator {
        duration: i64_at(b, at + 8),
        number: i64_at(b, at + 0x10),
        name: text_at(b, at + 0x1C, 0x208),
        ..Default::default()
    };
    let s = at + 0x224;
    if u32_at(b, s) != 0xB0 {
        return None;
    }
    g.width = i32_at(b, s + 0x0C);
    g.height = i32_at(b, s + 0x10);
    g.asked = (i32_at(b, s + 0x14), i32_at(b, s + 0x18));
    // Its format's pixel aspect (the second format: the media's).
    g.pixel_aspect = f64_at(b, s + 0x5C + 0x18);
    g.stream_id = i64_at(b, s + 0xA4);
    g.changes = u32_at(b, s + 0xAC);
    let mut e = s + 0xB0 + 4;
    for _ in 0..u32_at(b, s + 0xB0).min(64) {
        let Some((fx, end)) = effect_at(b, e) else {
            break;
        };
        g.effects.push(fx);
        e = end;
    }
    // The stream's properties.
    if u32_at(b, e) == 0x98 {
        g.automatic = u32_at(b, e + 0x0C) != 0;
        g.field_order = u32_at(b, e + 0x18);
        g.alpha = u32_at(b, e + 0x1C);
        g.rotation = u32_at(b, e + 0x40);
    }
    Some(g)
}

/// An envelope (its structure, then its points) and where it ends.
fn envelope(b: &[u8]) -> (DocEnvelope, usize) {
    let cb = u32_at(b, 0) as usize;
    let n = u32_at(b, 0x18) as usize;
    let size = match u32_at(b, 0x34) as usize {
        0 => 0x38,
        s => s.max(0x10),
    };
    let points: Vec<_> = (0..n.min(1 << 20))
        .map(|k| cb + k * size)
        .take_while(|&at| at + 0x10 <= b.len())
        .map(|at| (i64_at(b, at), f32_at(b, at + 8), i32_at(b, at + 0x0C)))
        .collect();
    let end = cb + points.len() * size;
    (
        DocEnvelope {
            kind: i32_at(b, 8),
            points,
        },
        end,
    )
}

/// The envelopes of a list (its header, then one chunk each).
fn envelopes(list: Option<&Chunk<'_>>) -> Vec<DocEnvelope> {
    list.map_or_else(Vec::new, |l| {
        l.blocks(ENVELOPE).map(|c| envelope(c.data).0).collect()
    })
}

/// An effect's body from `at` (a filter's chunk, a transition inside its
/// event's) and where it ends: an OpenFX plug-in's, or one of the file's own
/// kind, known by its class and its preset's name.
fn effect_at(b: &[u8], at: usize) -> Option<(DocEffect, usize)> {
    let cb = u32_at(b, at) as usize;
    if cb == 0x38 {
        let name = u32_at(b, at + 0x34) as usize;
        let size = u32_at(b, at + 0x14) as usize;
        let keys = u32_at(b, at + 0x18) as usize;
        let mut end = at + 0x38 + name + size;
        let mut native_keys = Vec::new();
        for _ in 0..keys.min(1 << 16) {
            if u32_at(b, end) != 0x1C {
                break;
            }
            let bytes = u32_at(b, end + 0x18) as usize;
            native_keys.push(super::NativeKey {
                time: i64_at(b, end + 4),
                curve: u32_at(b, end + 0x0C),
                flags: u32_at(b, end + 0x14),
                data: b
                    .get(end + 0x1C..end + 0x1C + bytes)
                    .unwrap_or(&[])
                    .to_vec(),
            });
            end += 0x1C + bytes;
        }
        return Some((
            DocEffect {
                native: guid_at(b, at + 4).map(|g| format!("{g:?}")),
                native_data: b
                    .get(at + 0x38 + name..at + 0x38 + name + size)
                    .unwrap_or(&[])
                    .to_vec(),
                native_keys,
                preset: text_at(b, at + 0x38, name),
                ..Default::default()
            },
            end.min(b.len()),
        ));
    }
    if !(0x30..=0x100).contains(&cb) || b.get(at + 4..at + 20) != Some(&OFX_HOST.0[..]) {
        return None;
    }
    let (t, after) = texts_at(b, at + cb, &[u32_at(b, at + 0x28), u32_at(b, at + 0x2C)]);
    // `{Svfx:com.vegascreativesoftware:sepia}`: the id inside.
    let plugin = t[0]
        .trim_start_matches('{')
        .trim_end_matches('}')
        .split_once(':')
        .map_or_else(|| t[0].clone(), |(_, id)| id.to_string());
    let flags = u32_at(b, at + 0x20);
    let mut fx = DocEffect {
        plugin,
        preset: t[1].clone(),
        bypass: flags & 1 != 0,
        before_pan_crop: flags & 0x80 != 0,
        transition: u32_at(b, at + 0x14) == 5,
        generator: u32_at(b, at + 0x14) == 11,
        ..Default::default()
    };
    let mut end = after;
    if after + 8 <= b.len() {
        end = after + 8 + u32_at(b, after) as usize;
        let n = u32_at(b, after + 4) as usize;
        let (params, keys) = params_with_keys(b, after + 8, n);
        fx.params = params;
        fx.keys = keys;
    }
    Some((fx, end.min(b.len())))
}

/// An effect's chunk; `None` for one this reader cannot place.
fn effect(b: &[u8]) -> Option<DocEffect> {
    effect_at(b, 0).map(|(fx, _)| fx)
}

/// How a parameter's value and keyframes are kept: the value's bytes, a
/// keyframe's bytes, and how many numbers have handles.
const PARAM_SHAPES: [(usize, usize, usize); 6] = [
    (4, 16, 0),
    (8, 52, 1),
    (16, 92, 2),
    (24, 132, 3),
    (24, 36, 0),
    (32, 44, 0),
];

/// A value of `bytes` bytes: a whole number in 4, else numbers of 8.
fn numbers_at(b: &[u8], at: usize, bytes: usize) -> Vec<f64> {
    if bytes == 4 {
        return vec![f64::from(i32_at(b, at))];
    }
    (0..bytes / 8).map(|k| f64_at(b, at + 8 * k)).collect()
}

/// A parameter's body of `size` bytes: its value (`None` if it is not one
/// this reader knows) and its keyframes.
fn param_body(b: &[u8], at: usize, size: usize) -> (Option<ParamValue>, Vec<DocKey>) {
    if at + size > b.len() || size < 4 {
        return (None, Vec::new());
    }
    // A text: its bytes, the text with its end, no keyframes.
    let len = u32_at(b, at) as usize;
    if len >= 2
        && len.is_multiple_of(2)
        && len + 8 == size
        && b[at + 4 + len - 1] == 0
        && b[at + 4 + len - 2] == 0
        && u32_at(b, at + size - 4) == 0
    {
        return (Some(ParamValue::Text(text_at(b, at + 4, len))), Vec::new());
    }
    let mut keys = Vec::new();
    let mut value_bytes = None;
    for (value, key, handled) in PARAM_SHAPES {
        if value + 4 > size {
            continue;
        }
        let n = u32_at(b, at + value) as usize;
        if n == 0 || value + 4 + n * key != size {
            continue;
        }
        value_bytes = Some(value);
        for k in 0..n {
            let p = at + value + 4 + k * key;
            let values = p + 12;
            let anchors = values + value;
            keys.push(DocKey {
                frame: f64_at(b, p),
                interpolation: i32_at(b, p + 8),
                value: numbers_at(b, values, value),
                handles: (0..handled)
                    .map(|c| {
                        let one = |group: usize| f64_at(b, anchors + group * value + c * 8);
                        [one(0), one(1), one(2), one(3)]
                    })
                    .collect(),
            });
        }
        break;
    }
    if value_bytes.is_none() && u32_at(b, at + size - 4) == 0 {
        value_bytes = Some(size - 4);
    }
    let value = value_bytes.and_then(|bytes| match bytes {
        4 => Some(ParamValue::Integer(i64::from(i32_at(b, at)))),
        8 => Some(ParamValue::Number(f64_at(b, at))),
        n if n > 8 && n.is_multiple_of(8) => Some(ParamValue::Numbers(numbers_at(b, at, n))),
        _ => None,
    });
    (value, keys)
}

/// An effect's parameters, by their names, and their keyframes.
type ParamsAndKeys = (Vec<(String, ParamValue)>, Vec<(String, Vec<DocKey>)>);

/// The `n` parameters of an effect from `at`, and the keyframes of those
/// that have them.
fn params_with_keys(b: &[u8], mut at: usize, n: usize) -> ParamsAndKeys {
    let mut params = Vec::new();
    let mut keys = Vec::new();
    for _ in 0..n.min(4096) {
        let size = u32_at(b, at) as usize;
        let len = u32_at(b, at + 4) as usize;
        let value_at = at + 8 + len;
        if len == 0
            || !len.is_multiple_of(2)
            || len > 512
            || size > 1 << 24
            || value_at + size > b.len()
        {
            break;
        }
        let name = text_at(b, at + 8, len);
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
            break;
        }
        let (value, its_keys) = param_body(b, value_at, size);
        if !its_keys.is_empty() {
            keys.push((name.clone(), its_keys));
        }
        if let Some(v) = value {
            params.push((name, v));
        }
        at = value_at + size;
    }
    (params, keys)
}

/// The `n` parameters of an effect from `at`.
#[cfg(test)]
pub(super) fn params_at(b: &[u8], at: usize, n: usize) -> Vec<(String, ParamValue)> {
    params_with_keys(b, at, n).0
}

fn effects(list: Option<&Chunk<'_>>) -> Vec<DocEffect> {
    list.map_or_else(Vec::new, |l| {
        l.blocks(EFFECT).filter_map(|c| effect(c.data)).collect()
    })
}

fn take(b: &[u8]) -> DocTake {
    let cb = u32_at(b, 0) as usize;
    DocTake {
        media: u32_at(b, 0x24),
        stream: u32_at(b, 0x08),
        offset: i64_at(b, 0x10),
        remain: i64_at(b, 0x18),
        name: text_at(b, cb, u32_at(b, 0x20) as usize),
    }
}

fn event(list: &Chunk<'_>) -> Option<DocEvent> {
    let h = list.block(EVENT_HEADER)?.data;
    let cb = u32_at(h, 0) as usize;
    let (name, after) = texts_at(h, cb, &[u32_at(h, 0x24)]);
    let mut e = DocEvent {
        id: u64_at(h, 0x40),
        start: i64_at(h, 0x10),
        length: i64_at(h, 0x18),
        muted: u32_at(h, 0x04) & 2 != 0,
        name: name[0].clone(),
        speed: f64_at(h, 0x28),
        fade_in: 0,
        fade_out: 0,
        curves: [1, 1, 1, 1],
        level: 1.0,
        active: i32_at(h, 0x20),
        takes: list.blocks(TAKE).map(|c| take(c.data)).collect(),
        pitch: 0.0,
        envelopes: envelopes(list.list(EVENT_ENVELOPES)),
        effects: effects(list.list(EVENT_FX)),
        audio_fx: Vec::new(),
        audio_unread: false,
        pan_crop: None,
        mask: None,
        keep_aspect: true,
        video_flags: 0x201,
        undersample: 1.0,
        transition_in: None,
        transition_out: None,
        progress_in: None,
        progress_out: None,
        normalize: None,
        channels: 1000,
        invert_phase: false,
    };
    if let Some((box_, mask)) = list
        .block(PAN_CROP)
        .and_then(|c| super::pancrop::read(c.data))
    {
        e.pan_crop = Some(box_);
        e.mask = mask;
    }
    if let Some(l) = list.list(AUDIO_EVENT_FX) {
        match l.block(AUDIO_EVENT_FX).and_then(|c| audio_chain(c.data)) {
            Some(chain) => e.audio_fx = chain,
            None => e.audio_unread = true,
        }
    }
    if !(e.speed.is_finite() && e.speed > 0.0) {
        e.speed = 1.0;
    }
    if u32_at(h, after) == 0x48 {
        let level = f32_at(h, after + 0x0C);
        if level.is_finite() && level >= 0.0 {
            e.level = level;
        }
        if u32_at(h, 0x04) & 0x200 != 0 {
            e.normalize = Some(f32_at(h, after + 0x10)).filter(|g| g.is_finite() && *g > 0.0);
        }
        e.curves = [0x14, 0x18, 0x1C, 0x20].map(|at| i32_at(h, after + at));
        e.fade_in = i64_at(h, after + 0x28);
        e.fade_out = i64_at(h, after + 0x30);
    }
    if let Some(a) = list.list(AUDIO_EVENT).and_then(|l| l.block(AUDIO_EVENT)) {
        let pitch = f64_at(a.data, 0x14);
        if pitch.is_finite() {
            e.pitch = pitch;
        }
        e.channels = u32_at(a.data, 0x04);
        e.invert_phase = u32_at(a.data, 0x08) & 1 != 0;
    }
    if let Some(v) = list.list(VIDEO_EVENT_LIST) {
        if let Some(h) = v.block(VIDEO_EVENT) {
            let b = h.data;
            e.video_flags = u32_at(b, 0x04);
            e.keep_aspect = e.video_flags & 1 != 0;
            let rate = f64_at(b, 0x78);
            if rate.is_finite() && rate > 0.0 && rate <= 1.0 {
                e.undersample = rate;
            }
            // The transitions after its structure: the fade in's, the fade out's.
            let mut at = u32_at(b, 0) as usize;
            for (n, place) in [
                (u32_at(b, 0x70), &mut e.transition_in),
                (u32_at(b, 0x74), &mut e.transition_out),
            ] {
                for k in 0..n.min(16) {
                    let Some((fx, end)) = effect_at(b, at) else {
                        break;
                    };
                    at = end;
                    if k == 0 {
                        *place = Some(fx);
                    }
                }
            }
        }
        // Their progress envelopes: the fade in's and the fade out's.
        e.progress_in = envelopes(v.list(VIDEO_EVENT_ENVELOPES_1))
            .into_iter()
            .find(|x| x.kind == TRANSITION_PROGRESS);
        e.progress_out = envelopes(v.list(VIDEO_EVENT_ENVELOPES_2))
            .into_iter()
            .find(|x| x.kind == TRANSITION_PROGRESS);
    }
    Some(e)
}

fn track(list: &Chunk<'_>) -> Option<DocTrack> {
    let h = list.block(TRACK_HEADER)?.data;
    let cb = u32_at(h, 0) as usize;
    let name = text_at(h, cb, u32_at(h, 0x10) as usize);
    let mut t = DocTrack {
        kind: u32_at(h, 0x0C),
        name,
        muted: u32_at(h, 0x04) & 1 != 0,
        volume: 1.0,
        pan: 0.0,
        composite_mode: 4,
        composite_level: 1.0,
        invert_phase: false,
        sends: Vec::new(),
        composite_depth: 0,
        motion: list
            .list(TRACK_MOTION_LIST)
            .and_then(|l| l.block(TRACK_MOTION))
            .map(|c| motion(c.data)),
        parent_motion: list
            .list(PARENT_MOTION_LIST)
            .and_then(|l| l.block(TRACK_MOTION))
            .map(|c| motion(c.data)),
        events: list
            .list(TRACK_HEADER)
            .map_or_else(Vec::new, |l| l.lists(EVENT).filter_map(event).collect()),
        envelopes: envelopes(list.list(TRACK_ENVELOPES)),
        effects: effects(list.list(TRACK_FX)),
        audio_fx: Vec::new(),
        audio_unread: false,
    };
    if let Some(l) = list.list(AUDIO_TRACK_FX) {
        match l.block(AUDIO_TRACK_FX).and_then(|c| audio_chain(c.data)) {
            Some(chain) => t.audio_fx = chain,
            None => t.audio_unread = true,
        }
    }
    t.effects.extend(effects(list.list(TRACK_FX_2)));
    if let Some(a) = list
        .list(AUDIO_TRACK_LIST)
        .and_then(|l| l.block(AUDIO_TRACK))
    {
        let (v, p) = (f32_at(a.data, 0x0C), f32_at(a.data, 0x10));
        if v.is_finite() && v >= 0.0 {
            t.volume = v;
        }
        if p.is_finite() {
            t.pan = p.clamp(-1.0, 1.0);
        }
        t.invert_phase = u32_at(a.data, 0x1FC) != 0;
        t.sends = (0..26).map(|k| f32_at(a.data, 0x124 + 8 * k)).collect();
    }
    if let Some(v) = list
        .list(VIDEO_TRACK_LIST)
        .and_then(|l| l.block(VIDEO_TRACK))
    {
        t.composite_mode = u32_at(v.data, 0x64);
        t.composite_depth = u32_at(v.data, 0xAC).min(16) as u8;
        let level = f32_at(v.data, 0x68);
        if level.is_finite() {
            t.composite_level = level.clamp(0.0, 1.0);
        }
    }
    Some(t)
}

/// A track's motion chunk: its structure, then three lists of keyframes.
pub(super) fn motion(b: &[u8]) -> DocMotion {
    let mut m = DocMotion {
        flags: u32_at(b, 4),
        aspect: f64_at(b, 0x14),
        ..Default::default()
    };
    let lists = (u32_at(b, 0x1C) as usize).min(3);
    let mut at = u32_at(b, 0) as usize;
    for l in 0..lists {
        let n = u32_at(b, at) as usize;
        at += 4;
        let mut keys = Vec::new();
        for _ in 0..n.min(1 << 16) {
            let size = u32_at(b, at) as usize;
            if size < 0xB8 || at + size > b.len() {
                break;
            }
            let three = |o: usize| {
                [
                    f64_at(b, at + o),
                    f64_at(b, at + o + 8),
                    f64_at(b, at + o + 16),
                ]
            };
            let mut k = DocMotionKey {
                curve: i32_at(b, at + 4),
                at: i64_at(b, at + 8),
                pivot: three(0x10),
                position: three(0x30),
                size: three(0x50),
                rotation: three(0x70),
                ..Default::default()
            };
            if size >= 0xDC {
                k.smoothness = f64_at(b, at + 0xB0);
            } else {
                k.color = [0, 4, 8, 12].map(|o| f32_at(b, at + 0xB0 + o));
                k.blur = f64_at(b, at + 0xC0);
                k.intensity = f64_at(b, at + 0xC8);
                k.smoothness = f64_at(b, at + 0xD0);
            }
            keys.push(k);
            at += size;
        }
        match l {
            0 => m.moves = keys,
            1 => m.shadow = keys,
            _ => m.glow = keys,
        }
    }
    m
}

/// A bus's chunk: a number, its structure of 200 bytes (which, its fader, its
/// switches, how long its texts are, how many items its list has), the
/// bytes of the rest, its texts, then blocks with their sizes: two, the
/// list (its last item its envelopes), its effects' chain and one more.
fn bus(b: &[u8]) -> Option<DocBus> {
    if u32_at(b, 0) != 200 || b.len() < 212 {
        return None;
    }
    let s = 4usize;
    let (t, mut at) = texts_at(
        b,
        s + 200 + 4,
        &[0x7C, 0x80, 0x84, 0x9C].map(|o| u32_at(b, s + o)),
    );
    let items = u32_at(b, s + 0x88) as usize;
    let id = u32_at(b, s + 8);
    let block = |at: &mut usize, numbered: bool| -> Option<(usize, usize)> {
        if numbered {
            *at += 4;
        }
        let n = usize::try_from(u64_at(b, *at)).ok()?;
        *at += 8;
        let from = *at;
        *at = at.checked_add(n)?;
        (*at <= b.len()).then_some((from, n))
    };
    let mut d = DocBus {
        id,
        name: t.get(1).cloned().unwrap_or_default(),
        gain: f64_at(b, s + 0x14),
        muted: u32_at(b, 0x5C) != 0,
        solo: u32_at(b, 0x60) != 0,
        ..Default::default()
    };
    block(&mut at, true)?;
    block(&mut at, true)?;
    let mut last = None;
    for _ in 0..items.min(1 << 16) {
        last = Some(block(&mut at, false)?);
    }
    if id != 5 {
        let (from, n) = block(&mut at, true)?;
        if n >= 40 {
            d.effects = audio_chain(&b[from..from + n]).unwrap_or_default();
        }
    }
    // The last item: a number, its properties, the envelopes' header, how
    // many and the envelopes.
    if let Some((from, n)) = last
        && n >= 4 + 0x23C + 0x4C
    {
        let item = &b[from..from + n];
        let mut p = 4 + 0x23C + 0x4C;
        for _ in 0..u32_at(item, 4 + 0x23C + 0x48).min(64) {
            let Some(rest) = item.get(p..) else {
                break;
            };
            if rest.len() < 0x4C {
                break;
            }
            let (e, end) = envelope(rest);
            d.envelopes.push(e);
            p += end;
        }
    }
    Some(d)
}

/// A marker's chunk: time, length, number, flags, its two texts and its
/// four-letter type (0 without).
fn marker(b: &[u8]) -> (i64, i64, u32, u32, String, String, u32) {
    let cb = u32_at(b, 0) as usize;
    let (t, _) = texts_at(b, cb, &[u32_at(b, 0x20), u32_at(b, 0x24)]);
    let kind = if cb >= 0x30 { u32_at(b, 0x28) } else { 0 };
    (
        i64_at(b, 0x08),
        i64_at(b, 0x10),
        u32_at(b, 0x18),
        u32_at(b, 0x1C),
        t[0].clone(),
        t[1].clone(),
        kind,
    )
}

/// The bins under `list`, their parent `parent`, into `out`.
fn bins(list: &Chunk<'_>, parent: Option<usize>, out: &mut Vec<DocBin>, depth: u32) {
    if depth > 32 {
        return;
    }
    for item in list.lists(TREE_ITEM) {
        let Some(d) = item.block(TREE_DATA) else {
            continue;
        };
        let b = d.data;
        let cb = u32_at(b, 0) as usize;
        match u32_at(b, 0x14) {
            // A bin, its items under it.
            1 => {
                out.push(DocBin {
                    name: text_at(b, cb, u32_at(b, 0x04) as usize),
                    parent,
                    media: Vec::new(),
                });
                let me = out.len() - 1;
                if let Some(kids) = item.list(BINS) {
                    bins(kids, Some(me), out, depth + 1);
                }
            }
            // A media in its bin.
            2 => {
                if let Some(p) = parent {
                    out[p].media.push(u32_at(b, 0x0C));
                }
            }
            _ => {}
        }
    }
}

/// Reads a project file.
pub fn read(file: &[u8]) -> Result<Document, VegError> {
    let root = riff::parse(file)?;
    if root.guid != PROJECT {
        return Err(VegError::NotProject);
    }
    let header = root
        .block(HEADER)
        .ok_or(VegError::Damaged("no header"))?
        .data;
    let mixer = root.list(MIXER);
    let mixer_settings = mixer.and_then(|m| m.block(MIXER_SETTINGS));
    let bus_list = mixer.map_or_else(Vec::new, |m| {
        m.blocks(BUS).filter_map(|c| bus(c.data)).collect()
    });
    let mut doc = Document {
        version: u32_at(header, 0x04),
        settings: settings(header, mixer_settings.map(|c| c.data)),
        media: root.list(MEDIA_LIST).map_or_else(Vec::new, |l| {
            l.blocks(MEDIA).map(|c| media(c.data)).collect()
        }),
        tracks: root
            .list(TRACKS)
            .map_or_else(Vec::new, |l| l.lists(TRACK).filter_map(track).collect()),
        output_fx: effects(root.list(OUTPUT_FX)),
        envelopes: envelopes(root.list(PROJECT_ENVELOPES)),
        busses: mixer_settings.map_or(0, |c| u32_at(c.data, 0x228) as usize),
        bus_list,
        ..Default::default()
    };
    // The project's folders (File › Properties).
    if let Some(f) = root.list(FOLDERS).and_then(|l| l.block(FOLDERS)) {
        for (code, value) in tags(f.data) {
            match &code {
                b"ppre" => doc.settings.prerender_folder = value,
                b"prec" => doc.settings.recorded_folder = value,
                _ => {}
            }
        }
    }
    if let Some(s) = root.list(SUMMARY_LIST).and_then(|l| l.block(SUMMARY)) {
        for (code, value) in tags(s.data) {
            let i = match &code {
                b"INAM" => 0,
                b"IART" => 1,
                b"IENG" => 2,
                b"ICOP" => 3,
                b"ICMT" => 4,
                _ => continue,
            };
            doc.summary[i] = value;
        }
    }
    if let Some(l) = root.list(MARKERS) {
        for c in l.blocks(MARKER) {
            let (time, length, number, flags, label, _, _) = marker(c.data);
            doc.markers.push(DocMarker {
                time,
                length,
                number,
                region: flags & 1 != 0,
                label,
            });
        }
    }
    if let Some(l) = root.list(COMMANDS) {
        for c in l.blocks(MARKER) {
            let (time, _, _, _, comment, what, _) = marker(c.data);
            let (command, parameter) = what
                .split_once(':')
                .map_or((what.clone(), String::new()), |(a, b)| {
                    (a.to_string(), b.to_string())
                });
            doc.commands.push(DocCommand {
                time,
                comment,
                command,
                parameter,
            });
        }
    }
    if let Some(l) = root.list(CD_LAYOUT) {
        for c in l.blocks(MARKER) {
            let (time, length, _, flags, name, _, kind) = marker(c.data);
            doc.cd.push(DocCd {
                time,
                length,
                index: kind == u32::from_le_bytes(*b"cdng") || flags & 1 == 0,
                name,
            });
        }
    }
    if let Some(l) = root.list(GROUPS) {
        for c in l.blocks(GROUP) {
            let n = u32_at(c.data, 0x08) as usize;
            let ids = (0..n.min(1 << 16))
                .map(|k| 0x10 + k * 8)
                .take_while(|&at| at + 8 <= c.data.len())
                .map(|at| u64_at(c.data, at))
                .collect();
            doc.groups.push(ids);
        }
    }
    if let Some(l) = root.list(BINS) {
        bins(l, None, &mut doc.bins, 0);
    }
    Ok(doc)
}
