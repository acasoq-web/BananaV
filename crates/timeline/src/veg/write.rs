//! A project written as a `.veg`: the chunks in the order VEGAS Pro 23
//! writes them, each structure with VEGAS's own defaults where BananaV has
//! nothing to say (the values a new project's have), so that it opens it.
//! What this does not write of BananaV's (its audio effects' chains, its
//! older built-in generators…) is left out.

use std::collections::HashMap;

use bv_core::Tick;

use super::build::composite_number;
use super::riff::{Guid, Writer};
use super::*;
use crate::envelope::EnvelopeKind;
use crate::fx::Effect;
use crate::model::{
    ChannelMap, Clip, MediaId, MediaInfo, MediaSource, PixelFormat, Project, ResampleMode,
    RulerFormat, TrackKind,
};

/// What the file is written with besides the project.
pub struct WriteOptions<'a> {
    /// Where it goes: the path its header holds.
    pub path: String,
    /// The user's documents folder (the header's).
    pub documents: String,
    /// Each of BananaV's effects as another program's plug-in (`None`: it
    /// has none, and the effect is left out).
    pub effect: &'a dyn Fn(&Effect) -> Option<DocEffect>,
    /// Each of BananaV's generators as another program's (an OpenFX
    /// plug-in's settings, or a record of the file's own kind); `None`:
    /// its media is left out.
    pub generator: &'a dyn Fn(&crate::fx::PluginFx) -> Option<DocEffect>,
}

impl std::fmt::Debug for WriteOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriteOptions")
            .field("path", &self.path)
            .field("documents", &self.documents)
            .finish_non_exhaustive()
    }
}

/// A structure being filled: zeros, then the values at their places.
struct Data(Vec<u8>);

impl Data {
    fn new(size: usize) -> Data {
        Data(vec![0; size])
    }

    fn u16(&mut self, at: usize, v: u16) -> &mut Data {
        self.0[at..at + 2].copy_from_slice(&v.to_le_bytes());
        self
    }

    fn u32(&mut self, at: usize, v: u32) -> &mut Data {
        self.0[at..at + 4].copy_from_slice(&v.to_le_bytes());
        self
    }

    fn i32(&mut self, at: usize, v: i32) -> &mut Data {
        self.u32(at, v as u32)
    }

    fn u64(&mut self, at: usize, v: u64) -> &mut Data {
        self.0[at..at + 8].copy_from_slice(&v.to_le_bytes());
        self
    }

    fn i64(&mut self, at: usize, v: i64) -> &mut Data {
        self.u64(at, v as u64)
    }

    fn f32(&mut self, at: usize, v: f32) -> &mut Data {
        self.u32(at, v.to_bits())
    }

    fn f64(&mut self, at: usize, v: f64) -> &mut Data {
        self.u64(at, v.to_bits())
    }

    /// `v` every `step` bytes from `from` to `to` (included).
    fn run(&mut self, from: usize, to: usize, step: usize, v: u32) -> &mut Data {
        for at in (from..=to).step_by(step) {
            self.u32(at, v);
        }
        self
    }

    fn bytes(&mut self, at: usize, b: &[u8]) -> &mut Data {
        self.0[at..at + b.len()].copy_from_slice(b);
        self
    }

    /// Appends `s` as UTF-16 with its zero (nothing for an empty `s` when
    /// `none_if_empty`); its bytes.
    fn text(&mut self, s: &str, none_if_empty: bool) -> u32 {
        if s.is_empty() && none_if_empty {
            return 0;
        }
        let before = self.0.len();
        for u in s.encode_utf16().chain(std::iter::once(0)) {
            self.0.extend_from_slice(&u.to_le_bytes());
        }
        (self.0.len() - before) as u32
    }

    fn push(&mut self, b: &[u8]) {
        self.0.extend_from_slice(b);
    }
}

/// UTF-16 bytes of `s` with its zero.
fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// A list of tags: `u32` 8, `u32` n, each its code, its byte count and
/// its text (with its zero).
fn tags(items: &[(&[u8; 4], &str)]) -> Vec<u8> {
    let mut d = Data::new(8);
    d.u32(0, 8).u32(4, items.len() as u32);
    for (code, value) in items {
        let t = utf16(value);
        d.push(&code[..]);
        d.push(&(t.len() as u32).to_le_bytes());
        d.push(&t);
    }
    d.0
}

/// The project's header: its settings, its path, two folders and its
/// three video items (the project's, then two VEGAS keeps at HD).
fn header(p: &Project, o: &WriteOptions<'_>) -> Vec<u8> {
    let s = &p.settings;
    let fps = s.frame_rate.as_f64();
    let timecode = match s.ruler_format {
        Some(RulerFormat::SmpteDrop) => 8,
        Some(RulerFormat::SmpteEbu) => 9,
        _ if (fps - 25.0).abs() < 1e-6 => 9,
        _ if (fps - 29.97).abs() < 1e-2 || (fps - 59.94).abs() < 1e-2 => 8,
        _ => 7,
    };
    let mut d = Data::new(0xB8);
    d.u32(0x00, 0xB8)
        .u32(0x04, VERSION)
        .u32(0x08, timecode)
        .u32(0x0C, 48_000)
        .f64(0x10, fps)
        .f64(0x18, s.tempo.beats_per_minute)
        .u16(0x28, s.tempo.beat_note as u16)
        .u16(0x2A, s.tempo.beats_per_measure as u16)
        .u32(0x2C, 0x40)
        .u32(0x38, 3)
        .u32(0x60, 1)
        .u32(0x70, 1)
        .u32(0x74, 1)
        .u32(0x78, 0x78)
        .u32(0x7C, 1)
        .u32(0xB0, 1);
    let path = d.text(
        if o.path.is_empty() {
            "Untitled"
        } else {
            &o.path
        },
        false,
    );
    let documents = d.text(&o.documents, false);
    let data = d.text(&p.prerender_folder, false);
    let fourth = d.text("", false);
    d.u32(0x3C, path)
        .u32(0x40, documents)
        .u32(0x44, data)
        .u32(0x6C, fourth);
    let float = matches!(
        s.pixel_format,
        PixelFormat::Float32Full | PixelFormat::Float32VideoLevels
    );
    let pixel_aspect = s.pixel_aspect.num as f64 / s.pixel_aspect.den.max(1) as f64;
    for k in 0..3 {
        let (w, h, f, fl) = if k == 0 {
            (s.width, s.height, fps, float)
        } else {
            (1920, 1080, 30_000.0 / 1001.0, false)
        };
        let mut item = Data::new(0x358);
        item.u32(0x00, 0x358)
            .u32(0x04, w)
            .u32(0x08, h)
            .u32(0x0C, if fl { 32 } else { 28 })
            .run(0x10, 0x20, 4, 3)
            .u32(0x28, 0x40)
            .u32(0x2C, u32::MAX)
            .u32(0x30, u32::from(k > 0))
            .f64(0x38, f)
            .f64(0x40, if k == 0 { pixel_aspect } else { 1.0 })
            .f64(0x48, if k > 0 || fl { 1.0 } else { 0.0 })
            .run(0xA0, 0xAC, 4, 1f32.to_bits())
            .f32(0xBC, 1.0)
            .f32(0xC8, 1.0)
            .f32(0xCC, 1.0)
            .f64(0xD8, 30_000.0 / 1001.0)
            .u32(0xE0, 1)
            .f64(0xE8, 0.1)
            .u32(0xF4, 1)
            .u32(0x104, 0x44)
            .f32(0x118, 1.0)
            .u32(0x120, 2)
            .u32(0x134, 1)
            .run(0x180, 0x1B0, 0x0C, 1)
            .bytes(0x2BC, b"ACES")
            .bytes(0x2FC, b"sRGB")
            .u32(0x33C, 2)
            .u32(0x340, 1)
            .u32(0x350, u32::from(k > 0 || fl));
        d.push(&item.0);
    }
    d.0
}

/// A media's entry: its path, its streams and, with sound, its sound's
/// channels.
fn media_entry(m: &MediaInfo, id: u32) -> Option<Vec<u8>> {
    let MediaSource::File { path } = &m.source else {
        return None;
    };
    let audio = m.audio.iter().chain(&m.extra_audio).count();
    let video = usize::from(m.video.is_some());
    let mut d = Data::new(0x80);
    d.u32(0x00, 0x80)
        .u32(0x18, 1)
        .u32(0x20, if video > 0 { 3 } else { 1 })
        .u32(0x30, u32::MAX)
        .u32(0x38, (audio + video) as u32)
        .u32(0x3C, id)
        .u32(0x48, u32::from(audio > 0))
        .f64(0x68, 1.0)
        .u32(0x70, u32::MAX)
        .u64(0x78, u64::MAX);
    let bytes = d.text(path, false);
    d.u32(0x24, bytes);
    for k in 0..video + audio {
        let mut s = Data::new(0x98);
        s.u32(0x00, 0x98).u32(0x0C, 1);
        if k < video {
            s.u32(0x04, VIDEO).f64(0x38, 1.0).u64(0x8C, u64::MAX);
        } else {
            s.u32(0x04, AUDIO);
        }
        d.push(&s.0);
    }
    if let Some(a) = m.audio {
        let mut props = Data::new(0x1C + 0x14);
        props
            .u32(0x00, 0x1C)
            .u32(0x04, 100)
            .bytes(0x08, &AUDIO_MEDIA_PROPERTIES.0)
            .u32(0x18, 0x14)
            .u32(0x1C, u32::from(a.channels))
            .u32(0x20, 1)
            .u32(0x28, 8);
        d.push(&props.0);
    }
    Some(d.0)
}

/// An effect's chunk, as an OpenFX plug-in's.
/// A parameter's value, how many keyframes it has, and its keyframes: a
/// frame, an interpolation, the value and, for the numbers that have them,
/// their handles.
fn param_body(value: &ParamValue, keys: &[DocKey]) -> Vec<u8> {
    let mut b = Vec::new();
    match value {
        ParamValue::Number(v) => b.extend_from_slice(&v.to_le_bytes()),
        ParamValue::Integer(v) => b.extend_from_slice(&(*v as i32).to_le_bytes()),
        ParamValue::Numbers(v) => v.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes())),
        ParamValue::Text(t) => {
            let units: Vec<u8> = t
                .encode_utf16()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect();
            b.extend_from_slice(&(units.len() as u32).to_le_bytes());
            b.extend_from_slice(&units);
        }
    }
    b.extend_from_slice(&(keys.len() as u32).to_le_bytes());
    for k in keys {
        b.extend_from_slice(&k.frame.to_le_bytes());
        b.extend_from_slice(&k.interpolation.to_le_bytes());
        match value {
            ParamValue::Integer(_) => {
                let whole = k.value.first().copied().unwrap_or(0.0).round() as i32;
                b.extend_from_slice(&whole.to_le_bytes());
            }
            _ => k
                .value
                .iter()
                .for_each(|x| b.extend_from_slice(&x.to_le_bytes())),
        }
        // Each of the four, for every number.
        for group in 0..4 {
            for h in &k.handles {
                b.extend_from_slice(&h[group].to_le_bytes());
            }
        }
    }
    b
}

fn effect_chunk(e: &DocEffect) -> Vec<u8> {
    if e.native.is_some() {
        return native_chunk(e);
    }
    let mut d = Data::new(0x30);
    d.u32(0x00, 0x30)
        .bytes(0x04, &OFX_HOST.0)
        .u32(
            0x14,
            if e.transition {
                5
            } else if e.generator {
                11
            } else {
                2
            },
        )
        .u32(0x18, 1)
        .u32(
            0x20,
            0x400 | u32::from(e.bypass) | if e.before_pan_crop { 0x80 } else { 0 },
        );
    let id = d.text(&format!("{{Svfx:{}}}", e.plugin), false);
    let preset = d.text(
        if e.preset.is_empty() {
            "(Default)"
        } else {
            &e.preset
        },
        false,
    );
    d.u32(0x28, id).u32(0x2C, preset);
    let mut params = Vec::new();
    for (name, value) in &e.params {
        let n = utf16(name);
        let keys = e
            .keys
            .iter()
            .find(|(k, _)| k == name)
            .map_or(&[][..], |(_, k)| &k[..]);
        let body = param_body(value, keys);
        params.extend_from_slice(&(body.len() as u32).to_le_bytes());
        params.extend_from_slice(&(n.len() as u32).to_le_bytes());
        params.extend_from_slice(&n);
        params.extend_from_slice(&body);
    }
    d.push(&(params.len() as u32).to_le_bytes());
    d.push(&(e.params.len() as u32).to_le_bytes());
    d.push(&params);
    d.0
}

/// A plug-in of the file's own kind (a generator's): its class, its
/// preset's name, its record and its keyframes (each the whole record).
fn native_chunk(e: &DocEffect) -> Vec<u8> {
    let class = e.native.as_deref().unwrap_or_default();
    // A class as GUIDs are written (hex digits, dashes at their places).
    let well_formed = class.len() == 36
        && class.bytes().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        });
    let guid = if well_formed {
        Guid::parse(class)
    } else {
        Guid([0; 16])
    };
    let preset = if e.preset.is_empty() {
        "(Default)"
    } else {
        &e.preset
    };
    let mut d = Data::new(0x38);
    let last = e.native_keys.last().map_or(0, |k| k.time);
    d.u32(0x00, 0x38)
        .bytes(0x04, &guid.0)
        .u32(0x14, e.native_data.len() as u32)
        .u32(0x18, e.native_keys.len() as u32)
        .i64(0x20, last)
        .u32(0x28, 11)
        .u32(0x2C, native_kind(class));
    let bytes = d.text(preset, false);
    d.u32(0x34, bytes);
    d.push(&e.native_data);
    for k in &e.native_keys {
        let mut h = Data::new(0x1C);
        h.u32(0x00, 0x1C)
            .i64(0x04, k.time)
            .u32(0x0C, k.curve)
            .u32(0x14, k.flags)
            .u32(0x18, k.data.len() as u32);
        d.push(&h.0);
        d.push(&k.data);
    }
    d.0
}

/// The number a record of the file's own kind carries for its class (as
/// the files have it).
fn native_kind(class: &str) -> u32 {
    if class.eq_ignore_ascii_case("02FD3B60-986B-11D4-9F6F-00485481BD31") {
        0x11A
    } else {
        0x10E
    }
}

/// A generated media's entry: the media's structure and its path (the
/// generator's), the generator's header (its length, number and name), its
/// stream (its frame's size, at a thousand frames a second), the generator
/// itself and the stream's structure (its fields, alpha, pixels' shape and
/// rotation).
fn generated_entry(m: &MediaInfo, id: u32, o: &WriteOptions<'_>) -> Option<Vec<u8>> {
    use crate::model::{AlphaChannel, Generator, MediaRotation};
    let MediaSource::Generated(Generator::Plugin(fx)) = &m.source else {
        return None;
    };
    let v = m.video?;
    let mut e = (o.generator)(fx)?;
    e.generator = true;
    let mut d = Data::new(0x80);
    d.u32(0x00, 0x80)
        .u32(0x18, 1)
        .u32(0x20, 3)
        .u32(0x30, u32::MAX)
        .u32(0x38, 1)
        .u32(0x3C, id)
        .f64(0x68, 1.0)
        .u32(0x70, u32::MAX)
        .u64(0x78, u64::MAX);
    let n = id.saturating_sub(1);
    let unique = match &e.native {
        Some(class) => format!("{{{class}}}"),
        None => format!("{{Svfx:{}}}", e.plugin),
    };
    let bytes = d.text(&format!("META:\\Video Generator\\{unique} ({n})[0]"), false);
    d.u32(0x24, bytes);
    // The header: its length, number and name.
    let mut h = Data::new(0x224);
    h.u64(0x00, 0x224)
        .i64(0x08, units(m.duration))
        .i64(0x10, i64::from(n))
        .u32(0x18, 1);
    let name: Vec<u16> = fx.name.encode_utf16().take(259).collect();
    for (k, u) in name.iter().enumerate() {
        h.u16(0x1C + 2 * k, *u);
    }
    d.push(&h.0);
    // The stream: the generator's own frame (before its rotation), the
    // size asked (in square pixels), a thousand frames a second.
    let turned = v.reading.rotation.quarters() % 2 == 1;
    let (w, ht) = if turned {
        (v.height, v.width)
    } else {
        (v.width, v.height)
    };
    let aspect = f64::from(v.pixel_aspect.num) / f64::from(v.pixel_aspect.den.max(1));
    let asked = ((f64::from(w) / aspect).round() as i32).max(1);
    let format = |par: f64| {
        let mut f = Data::new(0x40);
        f.u32(0x00, 0x40)
            .i32(0x04, -1)
            .f64(0x10, 1000.0)
            .f64(0x18, par)
            .u32(0x28, 2);
        f.0
    };
    let mut s = Data::new(0xB0);
    s.u64(0x00, 0xB0)
        .u32(0x08, 2)
        .i32(0x0C, w as i32)
        .i32(0x10, ht as i32)
        .i32(0x14, asked)
        .i32(0x18, ht as i32)
        .bytes(0x1C, &format(1.0))
        .bytes(0x5C, &format(aspect))
        .u64(
            0x9C,
            (units(m.duration) as f64 * 1000.0 / 10_000_000.0).round() as u64,
        )
        .i64(0xA4, i64::from(n))
        .u32(0xAC, 2);
    d.push(&s.0);
    // The generator.
    d.push(&1u32.to_le_bytes());
    d.push(&effect_chunk(&e));
    // The stream's structure.
    let mut st = Data::new(0x98);
    st.u32(0x00, 0x98)
        .u32(0x04, VIDEO)
        .u32(0x0C, 1)
        .u32(
            0x18,
            match v.field_order {
                bv_core::FieldOrder::UpperFirst => 1,
                bv_core::FieldOrder::LowerFirst => 2,
                _ => 0,
            },
        )
        .u32(
            0x1C,
            match v.reading.alpha {
                AlphaChannel::None => 0,
                AlphaChannel::Straight => 1,
                AlphaChannel::PremultipliedDirty => 3,
                _ => 2,
            },
        )
        .f64(0x38, aspect)
        .u32(
            0x40,
            match v.reading.rotation {
                MediaRotation::Clockwise => 1,
                MediaRotation::Inverted => 2,
                MediaRotation::Counterclockwise => 3,
                _ => 0,
            },
        )
        .u64(0x8C, u64::MAX);
    d.push(&st.0);
    Some(d.0)
}

/// A list of effects: those switched off in it too, the first `before`
/// working before the event's Pan/Crop.
fn effects(w: &mut Writer, list: Guid, fx: &[Effect], before: usize, o: &WriteOptions<'_>) {
    w.begin_list(list);
    for (i, e) in fx.iter().enumerate() {
        if let Some(mut doc) = (o.effect)(e) {
            doc.bypass = !e.enabled;
            doc.before_pan_crop = i < before;
            w.block(EFFECT, &effect_chunk(&doc));
        }
    }
    w.end();
}

/// An envelope to write: its number, its range's bottom, top and rest, and
/// its points.
type EnvelopeOut = (i32, (f32, f32, f32), Vec<(i64, f32, i32)>);

/// A list of an event's envelopes: its header (the event's start and how
/// far its envelopes' places go), then each.
fn event_envelopes(w: &mut Writer, list: Guid, (start, length): (i64, i64), items: &[EnvelopeOut]) {
    w.begin_list(list);
    let mut head = Data::new(0x48);
    head.u32(0x00, 0x48)
        .i64(0x08, start)
        .i64(0x10, length)
        .u32(0x40, 1);
    w.block(ENVELOPES_HEADER, &head.0);
    for (number, range, points) in items {
        w.block(ENVELOPE, &envelope_chunk(*number, *range, points));
    }
    w.end();
}

/// A keyframe of a Track Motion's list: its curve and time, the picture's
/// where it turns around, where it is, its size and its rotation, each as
/// x, y, z and 1.
fn motion_key(
    size: u32,
    curve: u32,
    at: i64,
    [pivot, position, extent, rotation]: [[f64; 3]; 4],
) -> Data {
    let mut d = Data::new(size as usize);
    d.u32(0x00, size).u32(0x04, curve).i64(0x08, at);
    for (from, v) in [
        (0x10, pivot),
        (0x30, position),
        (0x50, extent),
        (0x70, rotation),
    ] {
        for (k, x) in v.into_iter().chain([1.0]).enumerate() {
            d.f64(from + 8 * k, x);
        }
    }
    d.f64(0x90, 1.0);
    d
}

/// A move's keyframe.
fn move_key(curve: u32, at: i64, parts: [[f64; 3]; 4], smoothness: f64) -> Vec<u8> {
    let mut d = motion_key(0xDC, curve, at, parts);
    d.f64(0xB0, smoothness).f64(0xCC, -1.0).f64(0xD4, 1.0);
    d.0
}

/// A shadow's or a glow's keyframe.
fn halo_key(
    curve: u32,
    at: i64,
    parts: [[f64; 3]; 4],
    color: [f32; 4],
    blur: f64,
    intensity: f64,
) -> Vec<u8> {
    let mut d = motion_key(0xD8, curve, at, parts);
    let [red, green, blue, alpha] = color;
    for (k, x) in [blue, green, red, alpha].into_iter().enumerate() {
        d.f32(0xB0 + 4 * k, x);
    }
    d.f64(0xC0, blur).f64(0xC8, intensity);
    d.0
}

/// A track's motion chunk: its switches, the project's shape it is made for,
/// and its three lists: the file measures from half the height (where) and
/// from the height (how big), and in radians.
fn motion_chunk(
    m: &crate::motion::TrackMotion,
    aspect: f64,
    three_d: bool,
    parent: bool,
) -> Vec<u8> {
    use crate::fx::{KeyCurve, Transform};
    use crate::motion::{Halo, HaloLook};
    let number = |c: KeyCurve| super::pancrop::curve_number(c);
    let framing = |t: &Transform| -> [[f64; 3]; 4] {
        let (w, h) = (
            f64::from(t.scale) * aspect,
            f64::from(t.scale) * f64::from(t.stretch),
        );
        [
            [
                f64::from(t.pivot[0]) * 2.0 * w.abs(),
                f64::from(t.pivot[1]) * 2.0 * h.abs(),
                0.0,
            ],
            [f64::from(t.x) * 2.0 * aspect, f64::from(t.y) * 2.0, 0.0],
            [w, h, if parent { 1.0 } else { 0.05 }],
            [0.0, 0.0, f64::from(t.rotation_deg).to_radians()],
        ]
    };
    let spatial = |b: &crate::space::Box3d| -> [[f64; 3]; 4] {
        [
            [0.0; 3],
            [
                f64::from(b.x) * 2.0 * aspect,
                f64::from(b.y) * 2.0,
                f64::from(b.z) * 2.0 * aspect,
            ],
            [f64::from(b.width) * aspect, f64::from(b.height), 0.05],
            b.rotation.map(|r| f64::from(r).to_radians()),
        ]
    };
    let mut moves: Vec<Vec<u8>> = Vec::new();
    if three_d && !m.space_keys.is_empty() {
        for k in &m.space_keys {
            moves.push(move_key(
                number(k.curve),
                units(k.at),
                spatial(&k.value),
                0.0,
            ));
        }
    } else if three_d {
        moves.push(move_key(0, 0, spatial(&m.space), 0.0));
    } else if m.keys.is_empty() {
        moves.push(move_key(0, 0, framing(&m.transform), 0.0));
    } else {
        for k in &m.keys {
            moves.push(move_key(
                number(k.curve),
                units(k.at),
                framing(&k.value),
                f64::from(k.smoothness) / 100.0,
            ));
        }
    }
    let look_parts = |l: &HaloLook, rest_z: f64| -> [[f64; 3]; 4] {
        [
            [0.0; 3],
            [f64::from(l.x) * 2.0 * aspect, f64::from(l.y) * 2.0, 0.0],
            [f64::from(l.size), f64::from(l.size), rest_z],
            [0.0; 3],
        ]
    };
    // Off, it stands as a new track's has it.
    let halo = |h: &Halo, unused: Vec<u8>| -> Vec<Vec<u8>> {
        if !h.on {
            return vec![unused];
        }
        let one = |l: &HaloLook, curve: u32, at: i64| {
            halo_key(
                curve,
                at,
                look_parts(l, 1.0),
                l.color,
                f64::from(l.blur),
                f64::from(l.intensity),
            )
        };
        if h.keys.is_empty() {
            vec![one(&h.look, 0, 0)]
        } else {
            h.keys
                .iter()
                .map(|k| one(&k.value, number(k.curve), units(k.at)))
                .collect()
        }
    };
    let shadow = halo(
        &m.shadow,
        halo_key(
            0,
            0,
            [[0.0; 3], [0.05, -0.05, 0.0], [1.0, 1.0, 1.0], [0.0; 3]],
            [0.0, 0.0, 0.0, 1.0],
            0.05,
            0.0,
        ),
    );
    let glow = halo(
        &m.glow,
        halo_key(
            0,
            0,
            [[0.0; 3], [0.0; 3], [1.05, 1.05, 1.0], [0.0; 3]],
            [1.0, 1.0, 0.0, 1.0],
            0.05,
            0.0,
        ),
    );
    let flags = 1u32
        | if parent { 4 } else { 0 }
        | if m.shadow.on { 8 } else { 0 }
        | if m.glow.on { 0x10 } else { 0 };
    let mut d = Data::new(0x24);
    d.u32(0x00, 0x24)
        .u32(0x04, flags)
        .u32(0x08, 1)
        .f64(0x14, aspect)
        .u32(0x1C, 3);
    for list in [moves, shadow, glow] {
        d.push(&(list.len() as u32).to_le_bytes());
        for k in list {
            d.push(&k);
        }
    }
    d.0
}

/// A file's envelope number for BananaV's kind on a track, and its
/// range's top and its default.
fn envelope_number(
    kind: EnvelopeKind,
    busses: &[crate::busses::BusId],
) -> Option<(i32, f32, f32, f32)> {
    use crate::busses::SendTo;
    match kind {
        EnvelopeKind::Volume => Some((0, 0.0, 2.0, 1.0)),
        EnvelopeKind::Pan => Some((1, -1.0, 1.0, 0.0)),
        EnvelopeKind::Send(SendTo::Bus(b)) => {
            let i = busses.iter().position(|&x| x == b)?;
            (i < 26).then_some((3 + i as i32, 0.0, 2.0, 1.0))
        }
        EnvelopeKind::CompositeLevel => Some((29, 0.0, 1.0, 1.0)),
        EnvelopeKind::FadeToColor => Some((30, -1.0, 1.0, 0.0)),
        _ => None,
    }
}

fn envelope_chunk(number: i32, range: (f32, f32, f32), points: &[(i64, f32, i32)]) -> Vec<u8> {
    let mut d = Data::new(0x4C);
    d.u32(0x00, 0x4C)
        .i32(0x08, number)
        .f32(0x0C, range.0)
        .f32(0x10, range.1)
        .f32(0x14, range.2)
        .u32(0x18, points.len() as u32)
        .u32(0x20, u32::MAX)
        .u32(0x34, 0x38)
        .run(0x38, 0x40, 4, u32::MAX)
        .u32(0x44, 1);
    for &(at, value, curve) in points {
        let mut pt = Data::new(0x38);
        pt.i64(0x00, at).f32(0x08, value).i32(0x0C, curve);
        d.push(&pt.0);
    }
    d.0
}

/// A track's or an event's property set: its size, 100, its GUID, its
/// items' size and one item.
fn properties(guid: Guid, item: &[u8]) -> Vec<u8> {
    let mut d = Data::new(0x1C);
    d.u32(0x00, 0x1C)
        .u32(0x04, 100)
        .bytes(0x08, &guid.0)
        .u32(0x18, item.len() as u32);
    d.push(item);
    d.0
}

/// Rounds a CD's time to its frames (588 samples at 44.1 kHz), as VEGAS
/// writes the CD layout's.
fn cd_time(t: Tick) -> i64 {
    let u = units(t).max(0);
    let samples = (u as f64 * 1e-7 * 44_100.0 + 0.5).floor() as i64;
    let frames = (samples + 294) / 588;
    (frames * 588 * UNITS + 22_050) / 44_100
}

/// A marker's chunk: time, length, number, flags, its two texts (and its
/// type with the 0x30 layout).
fn marker_chunk(
    time: i64,
    length: i64,
    number: u32,
    flags: u32,
    a: &str,
    b: &str,
    kind: Option<&[u8; 4]>,
) -> Vec<u8> {
    let size = if kind.is_some() { 0x30 } else { 0x28 };
    let mut d = Data::new(size);
    d.u32(0x00, size as u32)
        .i64(0x08, time)
        .i64(0x10, length)
        .u32(0x18, number)
        .u32(0x1C, flags);
    if let Some(k) = kind {
        d.bytes(0x28, &k[..]);
    }
    let first = d.text(a, true);
    let second = d.text(b, true);
    d.u32(0x20, first).u32(0x24, second);
    d.0
}

/// The name a take has without one of its own: its media file's, without
/// its extension.
fn stem(m: &MediaInfo) -> String {
    let name = match &m.source {
        MediaSource::File { path } => path.rsplit(['\\', '/']).next().unwrap_or(path),
        _ => &m.name,
    };
    name.rsplit_once('.').map_or(name, |(a, _)| a).to_string()
}

struct Ids {
    media: HashMap<MediaId, u32>,
    event: u64,
    track: u32,
}

#[allow(clippy::too_many_arguments)]
fn event(
    w: &mut Writer,
    p: &Project,
    c: &Clip,
    kind: TrackKind,
    ids: &mut Ids,
    group: u32,
    o: &WriteOptions<'_>,
) {
    let media_of = |m: MediaId| p.media.iter().find(|x| x.id == m);
    let takes = c.take_list();
    if takes.iter().all(|t| !ids.media.contains_key(&t.media)) {
        return;
    }
    let file_kind = if kind == TrackKind::Video {
        VIDEO
    } else {
        AUDIO
    };
    ids.event += 1;
    let id = (u64::from(file_kind) << 60) | ids.event;
    w.begin_list(EVENT);
    let mut h = Data::new(0x60);
    h.u32(0x00, 0x60)
        .u32(
            0x04,
            0x3001_0100
                | if c.muted { 2 } else { 0 }
                | if c.normalize.is_some() { 0x200 } else { 0 },
        )
        .u32(0x08, file_kind)
        .i64(0x10, units(c.start))
        .i64(0x18, units(c.duration))
        .i32(0x20, c.active_take() as i32)
        .f64(0x28, c.rate)
        .u32(0x3C, u32::MAX)
        .u64(0x40, id)
        .u32(0x48, group)
        .u32(0x4C, 2)
        .u32(0x50, 1);
    // The fades: their curves, their reciprocal curves (the other fade's
    // when the event has none of its own), lengths.
    let (curve_in, curve_out) = (
        fade_curve_number(c.fade_in_curve),
        fade_curve_number(c.fade_out_curve),
    );
    let (reciprocal_in, reciprocal_out) = (
        fade_curve_number(c.fade_in_reciprocal.unwrap_or(c.fade_out_curve)),
        fade_curve_number(c.fade_out_reciprocal.unwrap_or(c.fade_in_curve)),
    );
    let mut fades = Data::new(0x48);
    fades
        .u32(0x00, 0x48)
        .f32(0x0C, c.level)
        .f32(0x10, c.normalize.unwrap_or(1.0))
        .i32(0x14, curve_in)
        .i32(0x18, curve_out)
        .i32(0x1C, reciprocal_in)
        .i32(0x20, reciprocal_out)
        .i64(0x28, units(c.fade_in))
        .i64(0x30, units(c.fade_out));
    h.push(&fades.0);
    // The list of ids after the fades (empty), and the property set.
    let mut ids_list = Data::new(0x10);
    ids_list.u32(0x00, 0x10);
    h.push(&ids_list.0);
    h.push(&properties(EVENT_PROPERTIES, &[0; 4]));
    w.block(EVENT_HEADER, &h.0);
    for t in &takes {
        let Some(&mid) = ids.media.get(&t.media) else {
            continue;
        };
        let Some(m) = media_of(t.media) else {
            continue;
        };
        let at = c.source_in + t.offset;
        // A picture lasts as long as VEGAS gives one (5 s), or the event.
        let length = if m.still {
            (at + c.duration).max(Tick::from_seconds(5))
        } else {
            m.duration
        };
        let mut d = Data::new(0x2A);
        d.u32(0x00, 0x2A)
            .u32(0x04, 7)
            .u32(
                0x08,
                if kind == TrackKind::Audio {
                    c.audio_stream as u32
                } else {
                    0
                },
            )
            .i64(0x10, units(at))
            .i64(0x18, units(length - at).max(0))
            .u32(0x24, mid);
        let name = t.name.clone().unwrap_or_else(|| stem(m));
        let bytes = d.text(&name, false);
        d.u32(0x20, bytes);
        w.block(TAKE, &d.0);
    }
    if let Some(env) = c
        .envelope(EnvelopeKind::Velocity)
        .filter(|_| kind == TrackKind::Video)
    {
        let points = env
            .points
            .iter()
            .map(|p| (units(p.at), p.value, key_curve_number(p.curve)))
            .collect();
        event_envelopes(
            w,
            EVENT_ENVELOPES,
            (units(c.start), units(c.duration)),
            &[(VELOCITY, (-1.0, 10.0, 1.0), points)],
        );
    }
    match kind {
        TrackKind::Audio => {
            let mut a = Data::new(0x28);
            a.u32(0x00, 0x28)
                .u32(
                    0x04,
                    match c.channels {
                        ChannelMap::Both => 1000,
                        ChannelMap::RightOnly => 1001,
                        ChannelMap::LeftOnly => 1002,
                        ChannelMap::Combine => 1005,
                        ChannelMap::Swap => 1006,
                    },
                )
                .u32(0x08, 8 | u32::from(c.invert_phase))
                .u32(0x0C, 2)
                .u32(0x10, 1)
                .f64(0x14, f64::from(c.pitch));
            w.begin_list(AUDIO_EVENT);
            w.block(AUDIO_EVENT, &a.0);
            w.end();
        }
        TrackKind::Video => {
            let resample = match c.resample {
                None => 0x200,
                Some(ResampleMode::Disable) => 0,
                Some(ResampleMode::FrameBlend) => 0x80,
                Some(ResampleMode::OpticalFlow) => 0x800,
            };
            // Its transitions: the one it comes in with, the one it goes out
            // with (the same chunk; the file keeps each in its fade's list).
            let chunk = |fx: &crate::fx::PluginFx, bypass: bool| {
                let mut doc = (o.effect)(&Effect::plugin(fx.clone()))?;
                doc.transition = true;
                doc.bypass = bypass;
                // Where it is along the transition, first as the file has it.
                doc.params.retain(|(n, _)| n != "Transition");
                doc.params
                    .insert(0, ("Transition".to_string(), ParamValue::Number(0.0)));
                Some(effect_chunk(&doc))
            };
            let transition = c
                .transition_fx
                .as_ref()
                .and_then(|fx| chunk(fx, c.transition_bypass));
            let exit = c
                .out_transition
                .as_deref()
                .and_then(|out| chunk(out.fx.as_deref()?, out.bypass));
            let mut v = Data::new(0x90);
            v.u32(0x00, 0x90)
                .u32(
                    0x04,
                    u32::from(c.pan_crop.keep_aspect)
                        | resample
                        | if c.reduce_flicker { 0x100 } else { 0 },
                )
                .f64(0x08, 1.0)
                .f64(0x10, 1.0)
                .u32(0x70, u32::from(transition.is_some()))
                .u32(0x74, u32::from(exit.is_some()))
                .f64(0x78, f64::from(c.undersample))
                .f64(0x88, 1.0);
            for t in [&transition, &exit].into_iter().flatten() {
                v.push(t);
            }
            w.begin_list(VIDEO_EVENT_LIST);
            w.block(VIDEO_EVENT, &v.0);
            // Its transition progress envelope: the fade in's part and the
            // fade out's, each in its own list as its fade's parts of a
            // million (where a fade has none, VEGAS's: 0 to 1 coming in, 1
            // to 0 going out, smooth).
            if let Some(env) = c.envelope(EnvelopeKind::TransitionProgress) {
                let smooth = key_curve_number(crate::fx::KeyCurve::Smooth);
                for (list, from, span, ends) in [
                    (VIDEO_EVENT_ENVELOPES_1, Tick::ZERO, c.fade_in, (0.0, 1.0)),
                    (
                        VIDEO_EVENT_ENVELOPES_2,
                        c.duration - c.fade_out,
                        c.fade_out,
                        (1.0, 0.0),
                    ),
                ] {
                    let mut points: Vec<_> = env
                        .points
                        .iter()
                        .filter(|p| span > Tick::ZERO && p.at >= from && p.at <= from + span)
                        .map(|p| {
                            let share = (p.at - from).0 as f64 / span.0 as f64;
                            let place = (share * 1_000_000.0).round() as i64;
                            (place, p.value, key_curve_number(p.curve))
                        })
                        .collect();
                    if points.is_empty() {
                        points = vec![(0, ends.0, smooth), (1_000_000, ends.1, smooth)];
                    }
                    event_envelopes(
                        w,
                        list,
                        (units(c.start), 1_000_000),
                        &[(TRANSITION_PROGRESS, (0.0, 1.0, 0.5), points)],
                    );
                }
            }
            w.end();
            if c.effects.iter().any(|e| (o.effect)(e).is_some()) {
                effects(w, EVENT_FX, &c.effects, c.pan_crop_at, o);
            }
            // Its Pan/Crop, when it is not a new event's.
            let size = media_of(c.media)
                .and_then(|m| m.video.as_ref())
                .map_or(c.pan_crop.size, |v| [v.width, v.height]);
            let whole = [crate::pancrop::PanCropKey::whole([
                f64::from(size[0]),
                f64::from(size[1]),
            ])];
            if let Some(b) = super::pancrop::write(&c.pan_crop, c.mask.as_ref(), size, &whole) {
                w.block(PAN_CROP, &b);
            }
        }
    }
    w.end();
}

fn track(
    w: &mut Writer,
    p: &Project,
    index: usize,
    ids: &mut Ids,
    groups: &HashMap<crate::model::GroupId, u32>,
    o: &WriteOptions<'_>,
) {
    let t = &p.tracks[index];
    let file_kind = if t.kind == TrackKind::Video {
        VIDEO
    } else {
        AUDIO
    };
    ids.track += 1;
    w.begin_list(TRACK);
    let mut h = Data::new(0x38);
    h.u32(0x00, 0x38)
        .u32(0x04, 0x0020_0000 | u32::from(t.muted))
        .u32(0x08, 0x44)
        .u32(0x0C, file_kind)
        .u32(0x14, index as u32)
        .u32(0x1C, ids.track)
        .u32(0x20, if t.kind == TrackKind::Video { 2 } else { 1 })
        .u32(0x24, 2)
        .u32(0x30, 1);
    let name = h.text(&t.name, true);
    h.u32(0x10, name);
    let mut item = Data::new(0x11C);
    item.u32(0x118, 0x7F00);
    h.push(&properties(TRACK_PROPERTIES, &item.0));
    w.block(TRACK_HEADER, &h.0);
    if !t.clips.is_empty() {
        w.begin_list(TRACK_HEADER);
        for c in &t.clips {
            let group = c.group.and_then(|g| groups.get(&g).copied()).unwrap_or(0);
            event(w, p, c, t.kind, ids, group, o);
        }
        w.end();
    }
    let busses: Vec<_> = p.busses.iter().map(|b| b.id).collect();
    let envelopes: Vec<_> = t
        .envelopes
        .iter()
        .filter_map(|e| Some((envelope_number(e.kind, &busses)?, e)))
        .collect();
    if !envelopes.is_empty() {
        w.begin_list(TRACK_ENVELOPES);
        let mut head = Data::new(0x48);
        head.u32(0x00, 0x48).u32(0x14, 0x2000_0000).u32(0x40, 1);
        w.block(ENVELOPES_HEADER, &head.0);
        for ((number, lo, hi, default), e) in envelopes {
            let points: Vec<(i64, f32, i32)> = e
                .points
                .iter()
                .map(|p| (units(p.at), p.value, key_curve_number(p.curve)))
                .collect();
            w.block(
                ENVELOPE,
                &envelope_chunk(number, (lo, hi, default), &points),
            );
        }
        w.end();
    }
    match t.kind {
        TrackKind::Audio => {
            let mut a = Data::new(0x310);
            let sends = |k: usize| {
                p.busses.get(k).and_then(|b| {
                    t.sends
                        .iter()
                        .find(|s| s.to == crate::busses::SendTo::Bus(b.id))
                })
            };
            a.u32(0x00, 0x310)
                .u32(0x04, u32::MAX)
                .f32(0x0C, t.level)
                .f32(0x10, t.pan)
                .f32(0x18, 1.0)
                .u32(0x1FC, u32::from(t.invert_phase))
                .run(0x20, 0x118, 8, 6)
                .run(0x120, 0x1E8, 8, 1)
                .u32(0x200, 0x37)
                .f32(0x204, 1.0)
                .run(0x210, 0x2FC, 4, 1f32.to_bits())
                .u32(0x304, 3)
                .u32(0x308, 8);
            for k in 0..p.busses.len().min(26) {
                if let Some(send) = sends(k) {
                    let gain = 10f32.powf(send.gain_db / 20.0);
                    a.f32(0x124 + 8 * k, gain);
                }
            }
            w.begin_list(AUDIO_TRACK_LIST);
            w.block(AUDIO_TRACK, &a.0);
            w.end();
        }
        TrackKind::Video => {
            let has_fx = t.effects.iter().any(|e| (o.effect)(e).is_some());
            // Over its children, among a parent's.
            let depth = t.composite_depth;
            let parent = p
                .tracks
                .get(index + 1)
                .is_some_and(|n| n.kind == TrackKind::Video && n.composite_depth > depth);
            let mut v = Data::new(0xB4);
            v.u32(0x00, 0xB4)
                .u32(
                    0x0C,
                    if parent { 0x40 } else { 0 } | if depth > 0 { 0x20 } else { 0 },
                )
                .u32(0xAC, u32::from(depth))
                .run(0x30, 0x3C, 4, 1f32.to_bits())
                .f32(0x4C, 1.0)
                .u32(0x64, composite_number(t.composite_mode))
                .f32(0x68, t.level)
                .u32(0x80, 4)
                .f32(0x84, 1.0)
                .u32(0xA0, u32::from(has_fx))
                .f32(0xA4, 1.0);
            w.begin_list(VIDEO_TRACK_LIST);
            w.block(VIDEO_TRACK, &v.0);
            w.end();
            let three_d = t.composite_mode == crate::model::CompositeMode::ThreeDSourceAlpha;
            let aspect = p.settings.width as f64 * f64::from(p.settings.pixel_aspect.ratio())
                / p.settings.height.max(1) as f64;
            if let Some(m) = &t.motion {
                w.begin_list(TRACK_MOTION_LIST);
                w.block(TRACK_MOTION, &motion_chunk(m, aspect, three_d, false));
                w.end();
            }
            if let Some(m) = &t.parent_motion {
                w.begin_list(PARENT_MOTION_LIST);
                w.block(TRACK_MOTION, &motion_chunk(m, 1.0, false, true));
                w.end();
            }
            effects(w, TRACK_FX, &t.effects, 0, o);
            w.begin_list(TRACK_LIST_B5707E20);
            w.end();
            w.begin_list(TRACK_FX_2);
            w.end();
        }
    }
    w.end();
}

/// The Preview bus's, the Master's and a made bus's records: a structure
/// of 0xC8 bytes, then (from 0xC8) a 0 and the rest's size, the device's
/// and the bus's names and VEGAS's defaults of its settings.
fn bus(kind: u32, name: &str, letter_bus: bool) -> Vec<u8> {
    let mut d = Data::new(0xC8);
    d.u32(0x00, 0xC8)
        .u32(0x04, 0x7800_0005)
        .u32(0x08, 0xC8)
        .u32(0x0C, kind)
        .run(0x1C, 0x54, 8, 0x3FF0_0000);
    let (device, label) = (utf16(""), utf16(name));
    d.u32(0x80, device.len() as u32)
        .u32(0x84, label.len() as u32)
        .u32(0x88, 2)
        .u32(0xA0, label.len() as u32);
    match kind {
        // The Preview bus.
        7 => {
            d.u32(0x14, u32::MAX)
                .u32(0x94, u32::MAX)
                .u32(0x98, 0xFFFF_FFFE);
        }
        // The Master.
        2 => {
            d.u32(0x8C, 1).u32(0x90, 1).u32(0x94, 1);
        }
        _ => {
            d.u32(0x10, 0x8000_0000)
                .u32(0x14, 1)
                .f32(0x6C, 1.0)
                .f32(0x74, 1.0)
                .u32(0x78, 0x37)
                .u32(0x8C, 1)
                .u32(0x98, 0xFFFF_FFFE)
                .u32(0xA4, 0x8000_0000);
        }
    }
    let mut rest = Vec::new();
    rest.extend_from_slice(&device);
    rest.extend_from_slice(&label);
    rest.extend_from_slice(&[0, 0]);
    rest.extend_from_slice(&label);
    if kind == 7 {
        rest.extend_from_slice(&[0; 48]);
    } else {
        let mut tail = Data::new(784);
        tail.u32(0x18, 0x2D8)
            .u32(0x20, 3)
            .u32(0x24, 0x23C)
            .u32(0x28, 4)
            .u32(0x30, 0x44)
            .f32(0x34, 1.0)
            .f32(0x44, 1.0)
            .f32(0x48, 1.0)
            .u32(0x4C, 2)
            .u32(0x260, 0x48)
            .u32(0x2AC, 0x48);
        if letter_bus {
            tail.run(0xA0, 0xFC, 4, 1f32.to_bits())
                .run(0x160, 0x1DC, 4, 1f32.to_bits());
        }
        rest.extend_from_slice(&tail.0);
    }
    d.push(&0u32.to_le_bytes());
    d.push(&(rest.len() as u32).to_le_bytes());
    d.push(&rest);
    d.0
}

/// A bus's fader (twice, as a linear gain) and switches, at their places.
fn bus_settings(b: &mut [u8], gain_db: f32, muted: bool, solo: bool) {
    let gain = 10f64.powf(f64::from(gain_db) / 20.0);
    b[0x18..0x20].copy_from_slice(&gain.to_le_bytes());
    b[0x20..0x28].copy_from_slice(&gain.to_le_bytes());
    b[0x5C..0x60].copy_from_slice(&u32::from(muted).to_le_bytes());
    b[0x60..0x64].copy_from_slice(&u32::from(solo).to_le_bytes());
}

fn mixer(w: &mut Writer, p: &Project) {
    w.begin_list(MIXER);
    let mut m = Data::new(0x7F8);
    m.u32(0x00, 4)
        .u32(0x04, 0x7EC)
        .u32(0x08, 0x7EC)
        .u32(0x0C, 1)
        .u32(0x10, 2)
        .u32(0x220, 1)
        .u32(0x228, p.busses.len().min(26) as u32)
        .run(0x25C, 0x2D8, 4, 0x8000_0000)
        .u32(0x6E0, 1)
        .u32(0x6E4, p.settings.sample_rate.0)
        .u32(0x6E8, 0x10);
    w.block(MIXER_SETTINGS, &m.0);
    w.block(BUS, &bus(7, "Preview", false));
    let mut master = bus(2, "Master", false);
    bus_settings(&mut master, p.master.gain_db, p.master.muted, false);
    w.block(BUS, &master);
    for (k, b) in p.busses.iter().take(26).enumerate() {
        let letter = (b'A' + k as u8) as char;
        let name = if b.channel.name.is_empty() {
            format!("Bus {letter}")
        } else {
            b.channel.name.clone()
        };
        let mut made = bus(3, &name, true);
        bus_settings(
            &mut made,
            b.channel.gain_db,
            b.channel.muted,
            b.channel.solo,
        );
        w.block(BUS, &made);
    }
    w.end();
}

/// A tree item's data: its name, its id, what it is (1: a folder, 2: a
/// media, `media` its id).
fn tree_data(name: &str, id: u32, kind: u32, media: u32) -> Vec<u8> {
    let mut d = Data::new(0x38);
    d.u32(0x00, 0x38).u32(0x14, kind).u32(0x2C, u32::MAX);
    if kind == 2 {
        d.u32(0x08, 2).u32(0x0C, media).f64(0x24, 1.0);
    } else {
        d.u32(0x08, id)
            .u32(0x18, u32::from(name != "Main Timeline"));
    }
    let bytes = d.text(name, true);
    d.u32(0x04, bytes);
    d.0
}

fn bins(
    w: &mut Writer,
    p: &Project,
    parent: Option<crate::bins::BinId>,
    ids: &Ids,
    next: &mut u32,
) {
    for b in p.bins.iter().filter(|b| b.parent == parent) {
        *next += 1;
        w.begin_list(TREE_ITEM);
        w.block(TREE_DATA, &tree_data(&b.name, *next, 1, 0));
        let kids = p.bins.iter().any(|k| k.parent == Some(b.id));
        let media: Vec<u32> = b
            .media
            .iter()
            .filter_map(|m| ids.media.get(m).copied())
            .collect();
        if kids || !media.is_empty() {
            w.begin_list(BINS);
            bins(w, p, Some(b.id), ids, next);
            for m in media {
                w.begin_list(TREE_ITEM);
                w.block(TREE_DATA, &tree_data("", 0, 2, m));
                w.end();
            }
            w.end();
        }
        w.end();
    }
}

/// Writes `p` as a `.veg`.
pub fn write(p: &Project, o: &WriteOptions<'_>) -> Vec<u8> {
    let mut w = Writer::new(PROJECT);
    w.block(HEADER, &header(p, o));
    let mut four = Data::new(0x24);
    four.u32(0x00, 0x24)
        .f64(0x04, 50f64.to_radians())
        .f64(0x0C, 1.0);
    w.begin_list(FOUR_LIST);
    w.block(FOUR, &four.0);
    w.end();
    if p.video_output_fx.iter().any(|e| (o.effect)(e).is_some()) {
        effects(&mut w, OUTPUT_FX, &p.video_output_fx, 0, o);
    }
    let mut ids = Ids {
        media: HashMap::new(),
        event: 0,
        track: 0,
    };
    let entries: Vec<(u32, Vec<u8>)> = p
        .media
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            let id = 20 + i as u32;
            let e = media_entry(m, id).or_else(|| generated_entry(m, id, o))?;
            ids.media.insert(m.id, id);
            Some((id, e))
        })
        .collect();
    if !entries.is_empty() {
        w.begin_list(MEDIA_LIST);
        for (_, e) in &entries {
            w.block(MEDIA, e);
        }
        w.end();
    }
    // The groups, numbered from 1, of the events written.
    let mut groups: HashMap<crate::model::GroupId, u32> = HashMap::new();
    let mut members: Vec<Vec<u64>> = Vec::new();
    {
        let mut n = 0u64;
        for t in &p.tracks {
            let kind = if t.kind == TrackKind::Video {
                VIDEO
            } else {
                AUDIO
            };
            for c in &t.clips {
                if c.take_list()
                    .iter()
                    .all(|k| !ids.media.contains_key(&k.media))
                {
                    continue;
                }
                n += 1;
                if let Some(g) = c.group {
                    let k = *groups.entry(g).or_insert_with(|| {
                        members.push(Vec::new());
                        members.len() as u32
                    });
                    members[k as usize - 1].push((u64::from(kind) << 60) | n);
                }
            }
        }
    }
    if members.iter().any(|m| m.len() > 1) {
        w.begin_list(GROUPS);
        for (k, m) in members.iter().enumerate() {
            let mut g = Data::new(0x10);
            g.u32(0x00, 0x10)
                .u32(0x04, k as u32 + 1)
                .u32(0x08, m.len() as u32);
            for id in m {
                g.push(&id.to_le_bytes());
            }
            w.block(GROUP, &g.0);
        }
        w.end();
    }
    if !p.tracks.is_empty() {
        w.begin_list(TRACKS);
        for i in 0..p.tracks.len() {
            track(&mut w, p, i, &mut ids, &groups, o);
        }
        w.end();
    }
    let s = &p.summary;
    w.begin_list(SUMMARY_LIST);
    w.block(
        SUMMARY,
        &tags(&[
            (b"IENG", &s.engineer),
            (b"INAM", &s.title),
            (b"IART", &s.artist),
            (b"ICOP", &s.copyright),
            (b"ICMT", &s.comments),
        ]),
    );
    w.end();
    // Markers and regions share their numbers, in time order.
    let mut marks: Vec<(i64, i64, bool, &str)> = p
        .markers
        .iter()
        .map(|m| (units(m.time), 0, false, m.name.as_str()))
        .chain(p.regions.iter().map(|r| {
            (
                units(r.range.start),
                units(r.range.duration),
                true,
                r.name.as_str(),
            )
        }))
        .collect();
    marks.sort_by_key(|m| (m.0, m.2));
    if !marks.is_empty() {
        w.begin_list(MARKERS);
        for (n, (time, length, region, name)) in marks.iter().enumerate() {
            let chunk = marker_chunk(
                *time,
                *length,
                n as u32 + 1,
                u32::from(*region),
                name,
                "",
                None,
            );
            w.block(MARKER, &chunk);
        }
        w.end();
    }
    if !p.commands.is_empty() {
        w.begin_list(COMMANDS);
        for c in &p.commands {
            let what = format!("{}:{}", command_out(&c.command), c.parameter);
            w.block(
                MARKER,
                &marker_chunk(units(c.time), 0, 0, 0, &c.comment, &what, None),
            );
        }
        w.end();
    }
    if !p.cd_tracks.is_empty() || !p.cd_indices.is_empty() {
        w.begin_list(CD_LAYOUT);
        for t in &p.cd_tracks {
            let start = cd_time(t.range.start);
            let end = cd_time(t.range.start + t.range.duration);
            w.block(
                MARKER,
                &marker_chunk(start, end - start, 0, 1, &t.name, "", None),
            );
        }
        for i in &p.cd_indices {
            let mut chunk = marker_chunk(cd_time(i.time), 0, 0, 0, "", "", Some(b"cdng"));
            let mut extra = Data::new(20);
            extra.u32(0, 1);
            chunk.extend_from_slice(&extra.0);
            w.block(MARKER, &chunk);
        }
        w.end();
    }
    mixer(&mut w, p);
    let mut next = 0x100;
    if !p.bins.is_empty() {
        w.begin_list(BINS);
        bins(&mut w, p, None, &ids, &mut next);
        w.end();
    }
    w.begin_list(LIST_1F0900B5);
    w.end();
    w.begin_list(TIMELINES);
    w.begin_list(TREE_ITEM);
    w.block(TREE_DATA, &tree_data("Main Timeline", next + 1, 1, 0));
    w.end();
    w.end();
    let folders: Vec<(&[u8; 4], &str)> = [
        b"pmed", b"pgmu", b"prmd", b"prnd", b"ppre", b"prec", b"pcap", b"pavc", b"pvob", b"pcdx",
        b"pgtm", b"ppix", b"pmix", b"ptak", b"pblu", b"pvcd", b"pppt", b"ppsp", b"pmul", b"pdao",
        b"fmed", b"fgmu", b"frmd", b"frnd", b"fpre", b"frec", b"fdim", b"fblu", b"nblu", b"pmng",
    ]
    .into_iter()
    .map(|code| {
        let value = match code {
            b"ppre" => p.prerender_folder.as_str(),
            b"prec" => p.recorded_folder.as_str(),
            b"pmng" => "0",
            _ => "",
        };
        (code, value)
    })
    .collect();
    w.begin_list(FOLDERS);
    w.block(FOLDERS, &tags(&folders));
    w.end();
    w.finish()
}
