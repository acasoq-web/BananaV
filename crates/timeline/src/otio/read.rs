//! An OpenTimelineIO document read: its tracks' events placed in time, the
//! media they use.

use std::borrow::Cow;
use std::collections::HashMap;

use bv_core::{FrameRate, Tick};
use serde_json::Value;

use super::{DEFAULT_MEDIA, frame_rate, ours, read_range, read_time, schema};
use crate::fx::{Effect, FadeCurve, Transition};
use crate::model::{Generator, MediaInfo, MediaSource, TrackKind};

/// Where a media's pictures and sound come from, as the document says.
#[derive(Clone, Debug, PartialEq)]
pub enum MediaRefSource {
    /// A file: its path (a relative one from the document's folder).
    File(String),
    /// Numbered pictures: `count` of them from `first`.
    Sequence {
        first: String,
        count: u32,
        rate: Option<FrameRate>,
    },
    /// A generator BananaV draws.
    Generated(Generator),
    /// Nothing BananaV can open: a reference left missing, a web address,
    /// another program's generator.
    Missing,
}

/// A media of the document.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaRef {
    /// Its name (the clip's, when the reference has none).
    pub name: String,
    pub source: MediaRefSource,
    /// The media as BananaV saved it, when BananaV wrote the file.
    pub saved: Option<MediaInfo>,
    /// The length the document gives it.
    pub available: Option<Tick>,
}

/// An event of a track, placed in the timeline's time.
#[derive(Clone, Debug)]
pub(crate) struct DocEvent {
    /// Its media, in [`Document::media`].
    pub media: usize,
    pub start: Tick,
    pub duration: Tick,
    /// The media's time at `start` (frozen, the frame it holds).
    pub source: Tick,
    /// The media's speed (`LinearTimeWarp`'s scalar).
    pub speed: f64,
    pub frozen: bool,
    pub fade_in: Tick,
    pub fade_out: Tick,
    /// How it enters: its fade-in's or crossfade's transition.
    pub transition: Option<Transition>,
    pub enabled: bool,
    pub effects: Vec<Effect>,
    /// The media's markers it shows: media time, length, name.
    pub markers: Vec<(Tick, Tick, String)>,
    /// What BananaV saved of it.
    pub saved: Option<Value>,
    /// Its opacity (video) or gain (audio), when the document says.
    pub level: Option<f32>,
    /// Its fade-in's and its fade-out's curves (a crossfade's: the later
    /// event's fade-in, the earlier one's fade-out), when the document
    /// says.
    pub curves: [Option<FadeCurve>; 2],
    /// Audio: what it plays of its media, when the document says.
    pub channels: Option<DocChannels>,
    /// The document's group of events that move together (another
    /// program's links), when it is in one.
    pub link: Option<usize>,
}

impl DocEvent {
    /// An event of media `media` from `start` for `duration`, its media's
    /// time `source` at its start, nothing else said.
    pub fn new(media: usize, start: Tick, duration: Tick, source: Tick, enabled: bool) -> DocEvent {
        DocEvent {
            media,
            start,
            duration,
            source,
            speed: 1.0,
            frozen: false,
            fade_in: Tick::ZERO,
            fade_out: Tick::ZERO,
            transition: None,
            enabled,
            effects: Vec::new(),
            markers: Vec::new(),
            saved: None,
            level: None,
            curves: [None; 2],
            channels: None,
            link: None,
        }
    }
}

/// Audio: the channels of its media an event plays, as another program's
/// document says them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DocChannels {
    /// The media's channels it plays on the left and on the right
    /// (1-based, counted across the media's streams).
    pub left: u16,
    pub right: u16,
    /// Both in the middle (each panned to the center): the two mixed.
    pub centered: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct DocTrack {
    pub kind: TrackKind,
    pub name: String,
    pub enabled: bool,
    pub events: Vec<DocEvent>,
    /// BananaV's effects on it.
    pub effects: Vec<Effect>,
    /// The BananaV track it is a lane of (that track's id then).
    pub key: Option<u64>,
    /// BananaV's settings of the track.
    pub saved: Option<Value>,
}

#[derive(Clone, Debug)]
pub(crate) struct DocMarker {
    pub name: String,
    pub start: Tick,
    pub duration: Tick,
}

/// A timeline read from an OpenTimelineIO document, its media still to be
/// found (see [`super::build`]).
#[derive(Clone, Debug)]
pub struct Document {
    pub name: String,
    /// The media its clips use.
    pub media: Vec<MediaRef>,
    /// Items left out: a nested timeline's tracks past the one taken, a
    /// track of another kind, a clip without a length.
    pub skipped: usize,
    /// The rate it counts time in.
    pub(crate) rate: Option<FrameRate>,
    /// The time shown at its start.
    pub(crate) start: Tick,
    /// From the bottom of the stack up.
    pub(crate) tracks: Vec<DocTrack>,
    pub(crate) markers: Vec<DocMarker>,
    /// The effects on the whole picture, when BananaV wrote them.
    pub(crate) output_fx: Option<Vec<Effect>>,
    /// The project BananaV saved, without its tracks, media and markers.
    pub(crate) saved: Option<Value>,
    /// The frame's size, when the document says.
    pub(crate) frame: Option<(u32, u32)>,
    /// The sound's sample rate, when the document says.
    pub(crate) sample_rate: Option<u32>,
}

/// The text with OpenTimelineIO's non-finite numbers (`NaN`, `Inf`,
/// `Infinity`, negative or not), which JSON lacks, as `null`.
fn finite(text: &str) -> Cow<'_, str> {
    if !text.contains("NaN") && !text.contains("Inf") {
        return Cow::Borrowed(text);
    }
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let (mut in_string, mut escaped) = (false, false);
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            out.push(c);
            i += 1;
            continue;
        }
        if c == b'"' {
            in_string = true;
        }
        let rest = &b[i..];
        let token = ["-Infinity", "Infinity", "-Inf", "Inf", "-NaN", "NaN"]
            .into_iter()
            .find(|t| rest.starts_with(t.as_bytes()));
        match token {
            Some(t) => {
                out.extend_from_slice(b"null");
                i += t.len();
            }
            None => {
                out.push(c);
                i += 1;
            }
        }
    }
    // Only ASCII was replaced.
    String::from_utf8(out).map_or(Cow::Borrowed(text), Cow::Owned)
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |c: u8| (c as char).to_digit(16);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let (Some(h), Some(l)) = (
                b.get(i + 1).copied().and_then(hex),
                b.get(i + 2).copied().and_then(hex),
            )
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The path a media reference's URL names: a `file:` URL's (Windows's
/// `file:///C:/…` as `C:/…`, another host's as a share, `//host/…`), a
/// path written plainly or relative (a bundle's `media/clip%20a.mov`) as
/// it is, decoded; `None` for other schemes (web addresses).
pub fn url_path(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    if url.len() >= 5 && url[..5].eq_ignore_ascii_case("file:") {
        let rest = &url[5..];
        let rest = match rest.strip_prefix("//") {
            Some(r) => match r.find('/') {
                Some(0) | None => r.to_string(),
                Some(i) if r[..i].eq_ignore_ascii_case("localhost") => r[i..].to_string(),
                Some(_) => format!("//{r}"),
            },
            None => rest.to_string(),
        };
        let path = decode(&rest);
        let b = path.as_bytes();
        if b.len() >= 3
            && b[0] == b'/'
            && b[1].is_ascii_alphabetic()
            && (b[2] == b':' || b[2] == b'|')
        {
            return Some(format!("{}:{}", &path[1..2], &path[3..]));
        }
        return Some(path);
    }
    // Another scheme ("https:", "s3:"); a drive letter ("C:") is one letter.
    if let Some(i) = url.find(':') {
        let scheme = &url[..i];
        if i > 1
            && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        {
            return None;
        }
    }
    Some(decode(url))
}

fn children(v: &Value) -> &[Value] {
    v.get("children")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn enabled(v: &Value) -> bool {
    v.get("enabled").and_then(Value::as_bool).unwrap_or(true)
}

fn track_kind(v: &Value) -> Option<TrackKind> {
    match v.get("kind")?.as_str()? {
        "Video" => Some(TrackKind::Video),
        "Audio" => Some(TrackKind::Audio),
        _ => None,
    }
}

/// BananaV's effects among an item's (other programs' are left out).
fn our_effects(v: &Value) -> Vec<Effect> {
    let all = v.get("effects").and_then(Value::as_array);
    all.into_iter()
        .flatten()
        .filter_map(|fx| {
            let mut e: Effect = serde_json::from_value(ours(fx)?.get("effect")?.clone()).ok()?;
            if let Some(on) = fx.get("enabled").and_then(Value::as_bool) {
                e.enabled = on;
            }
            Some(e)
        })
        .collect()
}

/// A clip's media reference: `Clip.2`'s active one, `Clip.1`'s only one.
fn media_reference(v: &Value) -> Option<&Value> {
    if let Some(refs) = v.get("media_references").and_then(Value::as_object) {
        let key = v
            .get("active_media_reference_key")
            .and_then(Value::as_str)
            .unwrap_or(DEFAULT_MEDIA);
        return refs.get(key).or_else(|| refs.values().next());
    }
    v.get("media_reference").filter(|r| !r.is_null())
}

/// An item's length: its source range's, a clip's media's, a
/// composition's children's (a track's in a row, a stack's longest).
fn length_of(v: &Value, depth: usize) -> Tick {
    if let Some((_, d)) = v.get("source_range").and_then(read_range) {
        return d;
    }
    if depth > 32 {
        return Tick::ZERO;
    }
    match schema(v).map(|s| s.0) {
        Some("Clip") => media_reference(v)
            .and_then(|r| r.get("available_range"))
            .and_then(read_range)
            .map_or(Tick::ZERO, |(_, d)| d),
        Some("Track") => children(v)
            .iter()
            .filter(|c| schema(c).is_some_and(|s| s.0 != "Transition"))
            .map(|c| length_of(c, depth + 1))
            .fold(Tick::ZERO, |a, b| a + b),
        Some("Stack") => children(v)
            .iter()
            .map(|c| length_of(c, depth + 1))
            .max()
            .unwrap_or(Tick::ZERO),
        _ => Tick::ZERO,
    }
}

/// `d` of the timeline as the media's time at `speed`.
fn scaled(d: Tick, speed: f64) -> Tick {
    if speed == 1.0 {
        d
    } else {
        Tick((d.0 as f64 * speed).round() as i64)
    }
}

/// The part of `e` between `lo` and `hi`, or `None` when nothing is left.
fn trimmed(mut e: DocEvent, lo: Tick, hi: Tick) -> Option<DocEvent> {
    let end = e.start + e.duration;
    if e.start < lo {
        let cut = lo - e.start;
        e.start = lo;
        e.duration -= cut;
        if !e.frozen {
            e.source += scaled(cut, e.speed);
        }
        e.fade_in = (e.fade_in - cut).max(Tick::ZERO);
    }
    if end > hi {
        let cut = end - hi;
        e.duration -= cut;
        e.fade_out = (e.fade_out - cut).max(Tick::ZERO);
    }
    (e.duration > Tick::ZERO).then_some(e)
}

/// A color as OpenTimelineIO's writers give one: `[r, g, b(, a)]` or
/// `{r, g, b(, a)}`.
fn color(v: &Value) -> Option<[f32; 4]> {
    let part =
        |c: Option<&Value>, default: f32| c.and_then(Value::as_f64).map_or(default, |x| x as f32);
    match v {
        Value::Array(a) if a.len() >= 3 => Some([
            part(a.first(), 0.0),
            part(a.get(1), 0.0),
            part(a.get(2), 0.0),
            part(a.get(3), 1.0),
        ]),
        Value::Object(o) => Some([
            part(o.get("r").or(o.get("red")), 0.0),
            part(o.get("g").or(o.get("green")), 0.0),
            part(o.get("b").or(o.get("blue")), 0.0),
            part(o.get("a").or(o.get("alpha")), 1.0),
        ]),
        _ => None,
    }
}

/// A `GeneratorReference` BananaV can draw.
fn generator(r: &Value) -> Option<Generator> {
    let kind = r.get("generator_kind")?.as_str()?.to_ascii_lowercase();
    let params = r.get("parameters").cloned().unwrap_or_default();
    let num = |k: &str, default: f64| params.get(k).and_then(Value::as_f64).unwrap_or(default);
    let rgba = |k: &str, default: [f32; 4]| params.get(k).and_then(color).unwrap_or(default);
    Some(match kind.as_str() {
        "solidcolor" | "solid_color" | "solid" | "color" => Generator::SolidColor {
            rgba: rgba("color", [0.0, 0.0, 0.0, 1.0]),
        },
        "black" => Generator::SolidColor {
            rgba: [0.0, 0.0, 0.0, 1.0],
        },
        "smptebars" | "smpte_bars" | "colorbars" | "color_bars" | "bars" => Generator::ColorBars,
        "checkerboard" => Generator::Checkerboard {
            size: num("size", 60.0).clamp(1.0, 4096.0) as u32,
        },
        "gradient" => Generator::Gradient {
            from: rgba("from", [0.0, 0.0, 0.0, 1.0]),
            to: rgba("to", [1.0, 1.0, 1.0, 1.0]),
            angle_deg: num("angle", 0.0) as f32,
        },
        "noise" => Generator::Noise {
            amount: num("amount", 1.0) as f32,
            color: params
                .get("color")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        "tone" | "testtone" | "test_tone" => Generator::TestTone {
            frequency_hz: num("frequency", 1000.0) as f32,
            level_db: num("level", -20.0) as f32,
        },
        _ => return None,
    })
}

/// A transition: its offsets (never negative) and how it draws.
#[derive(Clone, Copy, Debug)]
struct Cut {
    in_offset: Tick,
    out_offset: Tick,
    shape: Transition,
}

fn transition(v: &Value) -> Cut {
    let offset = |k: &str| {
        v.get(k)
            .and_then(read_time)
            .map_or(Tick::ZERO, |(t, _)| t.max(Tick::ZERO))
    };
    let named = |name: &str| Transition::ALL.into_iter().find(|t| t.name() == name);
    let shape = ours(v)
        .and_then(|o| o.get("transition"))
        .and_then(|t| serde_json::from_value(t.clone()).ok())
        .or_else(|| named(v.get("name")?.as_str()?))
        .unwrap_or_default();
    Cut {
        in_offset: offset("in_offset"),
        out_offset: offset("out_offset"),
        shape,
    }
}

/// A track's items, placed one after another.
enum Seg {
    Clip(DocEvent),
    /// A gap, or an item of a kind BananaV does not know.
    Gap,
    /// A nested stack's or track's events.
    Nested(Vec<DocEvent>),
    Transition(Cut),
}

struct Reader {
    media: Vec<MediaRef>,
    /// Each media's index in `media`, by what it is.
    keys: HashMap<String, usize>,
    skipped: usize,
    /// The first rate a clip's range was counted in.
    rate: Option<f64>,
    depth: usize,
}

impl Reader {
    /// The media a clip's reference names, added the first time.
    fn media_of(&mut self, r: Option<&Value>, clip_name: &str) -> usize {
        let saved: Option<MediaInfo> = r
            .and_then(ours)
            .and_then(|o| o.get("media"))
            .and_then(|m| serde_json::from_value(m.clone()).ok());
        let name = r
            .map(|r| text(r, "name"))
            .filter(|n| !n.is_empty())
            .or_else(|| saved.as_ref().map(|s| s.name.clone()))
            .unwrap_or_else(|| clip_name.to_string());
        let available = r
            .and_then(|r| r.get("available_range"))
            .and_then(read_range)
            .map(|(_, d)| d);
        let mut source = match r.and_then(schema).map(|s| s.0) {
            Some("ExternalReference") => r
                .and_then(|r| r.get("target_url"))
                .and_then(Value::as_str)
                .and_then(url_path)
                .map_or(MediaRefSource::Missing, MediaRefSource::File),
            Some("ImageSequenceReference") => {
                r.map_or(MediaRefSource::Missing, |r| sequence(r, available))
            }
            Some("GeneratorReference") => r
                .and_then(generator)
                .map_or(MediaRefSource::Missing, MediaRefSource::Generated),
            _ => MediaRefSource::Missing,
        };
        // BananaV's generators as it saved them.
        if let Some(MediaSource::Generated(g)) = saved.as_ref().map(|s| &s.source)
            && !matches!(source, MediaRefSource::File(_))
        {
            source = MediaRefSource::Generated(g.clone());
        }
        // One media per file, unless BananaV kept two of the same file.
        let id = saved.as_ref().map_or(0, |s| s.id.0);
        let key = match &source {
            MediaRefSource::File(p) => format!("file:{id}:{p}"),
            MediaRefSource::Sequence { first, .. } => format!("sequence:{id}:{first}"),
            MediaRefSource::Generated(g) => format!(
                "generated:{id}:{}",
                serde_json::to_string(g).unwrap_or_default()
            ),
            MediaRefSource::Missing => format!("missing:{id}:{name}"),
        };
        if let Some(&i) = self.keys.get(&key) {
            return i;
        }
        self.media.push(MediaRef {
            name,
            source,
            saved,
            available,
        });
        self.keys.insert(key, self.media.len() - 1);
        self.media.len() - 1
    }

    /// Clip `v` as an event at `at`, `None` when it has no length.
    fn clip(&mut self, v: &Value, at: Tick) -> Option<DocEvent> {
        let reference = media_reference(v);
        let available = reference
            .and_then(|r| r.get("available_range"))
            .and_then(read_range);
        let range = v.get("source_range").filter(|r| !r.is_null());
        if self.rate.is_none() {
            self.rate = range
                .and_then(|r| r.get("start_time"))
                .and_then(read_time)
                .map(|(_, rate)| rate);
        }
        let (start, duration) = range.and_then(read_range).or(available)?;
        if duration <= Tick::ZERO {
            return None;
        }
        // Media time counts from the reference's start (a file's timecode).
        let origin = available.map_or(Tick::ZERO, |(s, _)| s);
        let (mut speed, mut frozen) = (1.0, false);
        let fx = v.get("effects").and_then(Value::as_array);
        for e in fx.into_iter().flatten() {
            match schema(e).map(|s| s.0) {
                Some("FreezeFrame") => frozen = true,
                Some("LinearTimeWarp") => {
                    let k = e.get("time_scalar").and_then(Value::as_f64).unwrap_or(1.0);
                    if k == 0.0 {
                        frozen = true;
                    } else if k.is_finite() {
                        speed *= k.abs();
                    }
                }
                _ => {}
            }
        }
        let name = text(v, "name");
        let media = self.media_of(reference, &name);
        let markers = v
            .get("markers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|m| {
                let (s, d) = read_range(m.get("marked_range")?)?;
                Some((s - origin, d.max(Tick::ZERO), text(m, "name")))
            })
            .collect();
        Some(DocEvent {
            speed,
            frozen,
            effects: our_effects(v),
            markers,
            saved: ours(v).and_then(|o| o.get("event")).cloned(),
            ..DocEvent::new(media, at, duration, start - origin, enabled(v))
        })
    }

    /// A track's events in its own time, and its length. A transition
    /// between two clips is a crossfade (each event overlaps it, by its
    /// handles); between a clip and a gap, a fade.
    fn events(&mut self, track: &Value, kind: TrackKind) -> (Vec<DocEvent>, Tick) {
        if self.depth > 16 {
            self.skipped += 1;
            return (Vec::new(), length_of(track, 0));
        }
        self.depth += 1;
        let mut segs = Vec::new();
        let mut at = Tick::ZERO;
        for child in children(track) {
            match schema(child).map(|s| s.0) {
                Some("Transition") => segs.push(Seg::Transition(transition(child))),
                Some("Clip") => match self.clip(child, at) {
                    Some(e) => {
                        at += e.duration;
                        segs.push(Seg::Clip(e));
                    }
                    None => self.skipped += 1,
                },
                Some("Stack" | "Track") => {
                    let (events, d) = self.nested(child, kind, at);
                    at += d;
                    segs.push(Seg::Nested(events));
                }
                Some(other) => {
                    // A gap, or an item of a schema BananaV does not know:
                    // its time.
                    if other != "Gap" {
                        self.skipped += 1;
                    }
                    at += length_of(child, 0).max(Tick::ZERO);
                    segs.push(Seg::Gap);
                }
                None => {}
            }
        }
        let is_clip = |j: Option<usize>| matches!(j.and_then(|j| segs.get(j)), Some(Seg::Clip(_)));
        let mut events = Vec::new();
        for (i, seg) in segs.iter().enumerate() {
            let mut e = match seg {
                Seg::Clip(e) => e.clone(),
                Seg::Nested(inner) => {
                    events.extend(inner.iter().cloned());
                    continue;
                }
                _ => continue,
            };
            if let Some(Seg::Transition(t)) = i.checked_sub(1).map(|j| &segs[j]) {
                e.start -= t.in_offset;
                e.duration += t.in_offset;
                if !e.frozen {
                    e.source -= scaled(t.in_offset, e.speed);
                }
                e.transition = Some(t.shape);
                if !is_clip(i.checked_sub(2)) {
                    e.fade_in = t.in_offset + t.out_offset;
                }
            }
            if let Some(Seg::Transition(t)) = segs.get(i + 1) {
                e.duration += t.out_offset;
                if !is_clip(Some(i + 2)) {
                    e.fade_out = t.in_offset + t.out_offset;
                }
            }
            events.push(e);
        }
        self.depth -= 1;
        (events, at)
    }

    /// A nested stack's or track's events, placed at `at`: a track of
    /// `kind`, or a stack's first one (its others are left out), trimmed to
    /// the item's source range.
    fn nested(&mut self, v: &Value, kind: TrackKind, at: Tick) -> (Vec<DocEvent>, Tick) {
        let tracks: Vec<&Value> = if schema(v).is_some_and(|s| s.0 == "Track") {
            vec![v]
        } else {
            children(v)
                .iter()
                .filter(|t| schema(t).is_some_and(|s| s.0 == "Track"))
                .collect()
        };
        let mut inner = None;
        for t in tracks {
            if inner.is_none() && track_kind(t) == Some(kind) {
                inner = Some(self.events(t, kind).0);
            } else if !children(t).is_empty() {
                self.skipped += 1;
            }
        }
        let length = length_of(v, 0);
        let (s, d) = v
            .get("source_range")
            .and_then(read_range)
            .unwrap_or((Tick::ZERO, length));
        let events = inner
            .unwrap_or_default()
            .into_iter()
            .filter_map(|e| trimmed(e, s, s + d))
            .map(|mut e| {
                e.start += at - s;
                e
            })
            .collect();
        (events, d.max(Tick::ZERO))
    }
}

/// An image sequence's first picture and how many there are.
fn sequence(r: &Value, available: Option<Tick>) -> MediaRefSource {
    let base = text(r, "target_url_base");
    let base = if base.is_empty() || base.ends_with('/') {
        base
    } else {
        format!("{base}/")
    };
    let start = r
        .get("start_frame")
        .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
        .unwrap_or(1);
    let padding = r
        .get("frame_zero_padding")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(32) as usize;
    let url = format!(
        "{base}{}{start:0padding$}{}",
        text(r, "name_prefix"),
        text(r, "name_suffix")
    );
    let Some(first) = url_path(&url) else {
        return MediaRefSource::Missing;
    };
    let rate = r.get("rate").and_then(Value::as_f64);
    let step = r
        .get("frame_step")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1) as f64;
    let count = match (available, rate) {
        (Some(d), Some(rate)) => (d.as_seconds_f64() * rate / step).round().max(1.0) as u32,
        _ => 1,
    };
    MediaRefSource::Sequence {
        first,
        count,
        rate: rate.and_then(frame_rate),
    }
}

/// The timeline of a document: itself, a collection's first, or a bare
/// stack or track taken as one.
fn timeline_in(v: &Value, depth: usize) -> Option<&Value> {
    match schema(v)?.0 {
        "Timeline" | "Stack" | "Track" => Some(v),
        "SerializableCollection" if depth < 8 => {
            children(v).iter().find_map(|c| timeline_in(c, depth + 1))
        }
        _ => None,
    }
}

fn markers_of(v: &Value) -> Vec<DocMarker> {
    let all = v.get("markers").and_then(Value::as_array);
    all.into_iter()
        .flatten()
        .filter_map(|m| {
            let (start, duration) = read_range(m.get("marked_range")?)?;
            Some(DocMarker {
                name: text(m, "name"),
                start,
                duration: duration.max(Tick::ZERO),
            })
        })
        .collect()
}

/// Reads an OpenTimelineIO document (`.otio`'s JSON).
pub fn read(text_: &str) -> Result<Document, String> {
    let v: Value = serde_json::from_str(&finite(text_))
        .map_err(|e| format!("not an OpenTimelineIO file ({e})"))?;
    let top = timeline_in(&v, 0).ok_or_else(|| "no timeline in the file".to_string())?;
    let (timeline, stack) = match schema(top).map(|s| s.0) {
        Some("Timeline") => (Some(top), top.get("tracks").filter(|s| !s.is_null())),
        _ => (None, Some(top)),
    };
    let tracks: Vec<&Value> = match stack {
        Some(s) if schema(s).is_some_and(|s| s.0 == "Track") => vec![s],
        Some(s) => children(s).iter().collect(),
        None => Vec::new(),
    };
    let mut r = Reader {
        media: Vec::new(),
        keys: HashMap::new(),
        skipped: 0,
        rate: None,
        depth: 0,
    };
    let mut doc_tracks = Vec::new();
    let mut markers = stack.map(markers_of).unwrap_or_default();
    for t in tracks {
        let kind = match schema(t).map(|s| s.0) {
            Some("Track") => track_kind(t),
            _ => None,
        };
        let Some(kind) = kind else {
            r.skipped += 1;
            continue;
        };
        let (events, _) = r.events(t, kind);
        for m in markers_of(t) {
            let known = markers
                .iter()
                .any(|k| k.start == m.start && k.name == m.name);
            if !known {
                markers.push(m);
            }
        }
        let meta = ours(t);
        doc_tracks.push(DocTrack {
            kind,
            name: text(t, "name"),
            enabled: enabled(t),
            events,
            effects: our_effects(t),
            key: meta.and_then(|m| m.get("key")).and_then(Value::as_u64),
            saved: meta.and_then(|m| m.get("track")).cloned(),
        });
    }
    let start = timeline
        .and_then(|t| t.get("global_start_time"))
        .and_then(read_time);
    let saved = timeline
        .and_then(ours)
        .and_then(|o| o.get("project"))
        .cloned();
    let output_fx = stack
        .map(our_effects)
        .filter(|fx| !fx.is_empty() || saved.is_some());
    let rate = start.map(|(_, rate)| rate).or(r.rate).and_then(frame_rate);
    Ok(Document {
        name: top
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        media: r.media,
        skipped: r.skipped,
        rate,
        start: start.map_or(Tick::ZERO, |(t, _)| t),
        tracks: doc_tracks,
        markers,
        output_fx,
        saved,
        frame: None,
        sample_rate: None,
    })
}
