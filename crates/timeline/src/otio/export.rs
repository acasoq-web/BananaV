//! A project as an OpenTimelineIO timeline.

use std::collections::HashSet;
use std::fmt::Write as _;

use bv_core::{FrameRate, Tick};
use serde::Serialize;
use serde_json::{Value, json};

use super::{DEFAULT_MEDIA, KEY, METADATA_VERSION, rational, time_range};
use crate::fx::{Effect, Transition};
use crate::interchange::file_url;
use crate::model::{
    Clip, Generator, ImageSequence, MediaId, MediaInfo, MediaSource, Project, Track, TrackKind,
};

/// The project as an OpenTimelineIO document named `name`, its media files
/// at their `file://` URLs.
pub fn export(p: &Project, name: &str) -> String {
    export_with(p, name, &|path: &str| Some(file_url(path)))
}

/// The project as an OpenTimelineIO document named `name`, `url` giving
/// the address written for each media file's path (a bundle's
/// `media/clip.mov`; for an image sequence, its first picture's: the
/// others are beside it), or `None` for a file left out (a bundle's media
/// not found: a `MissingReference`, as OpenTimelineIO's bundles have
/// them, BananaV's metadata keeping where it was).
pub fn export_with(p: &Project, name: &str, url: &dyn Fn(&str) -> Option<String>) -> String {
    let writer = Writer {
        p,
        rate: p.settings.frame_rate,
        url,
    };
    let timeline = writer.timeline(name);
    // Four spaces, as OpenTimelineIO indents.
    let mut out = Vec::new();
    let format = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut json = serde_json::Serializer::with_formatter(&mut out, format);
    timeline
        .serialize(&mut json)
        .expect("JSON values always serialize");
    let mut text = String::from_utf8(out).expect("serde_json writes UTF-8");
    text.push('\n');
    text
}

/// A relative path as a URL's path, as OpenTimelineIO's bundles write
/// their media's (`media/clip%20a.mov`).
pub fn relative_url(path: &str) -> String {
    let mut out = String::new();
    for b in path.replace('\\', "/").bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// A track's events in lanes, each a run OpenTimelineIO's items and
/// transitions can say: events one after another, an event overlapping
/// only the one before it — starting and ending after it, clear of the
/// crossfade before, with crossfades on and neither muted — a crossfade.
/// Each event goes to the first lane it fits; lanes past the first are
/// tracks over it (a later event covers an earlier one).
pub(crate) fn lanes(t: &Track, crossfades: bool) -> Vec<Vec<&Clip>> {
    let mut lanes: Vec<Vec<&Clip>> = Vec::new();
    for c in &t.clips {
        let fits = |lane: &Vec<&Clip>| {
            let Some(&last) = lane.last() else {
                return true;
            };
            if c.start >= last.end() {
                return true;
            }
            let before = lane.len().checked_sub(2).map(|i| lane[i]);
            crossfades
                && !c.muted
                && !last.muted
                && c.start > last.start
                && c.end() > last.end()
                && before.is_none_or(|b| b.end() < c.start)
        };
        match lanes.iter_mut().find(|lane| fits(lane)) {
            Some(lane) => lane.push(c),
            None => lanes.push(vec![c]),
        }
    }
    if lanes.is_empty() {
        lanes.push(Vec::new());
    }
    lanes
}

/// The kind OpenTimelineIO names a track's.
fn kind_name(kind: TrackKind) -> &'static str {
    match kind {
        TrackKind::Video => "Video",
        TrackKind::Audio => "Audio",
    }
}

/// An effect as an `Effect.1`, BananaV's whole effect in its metadata.
fn effect(e: &Effect) -> Value {
    json!({
        "OTIO_SCHEMA": "Effect.1",
        "effect_name": e.name(),
        "enabled": e.enabled,
        "metadata": {KEY: {"effect": e}},
        "name": e.name(),
    })
}

/// A generator's kind and parameters, as a `GeneratorReference` names
/// them (`SMPTEBars` as OpenTimelineIO's own examples).
fn generator(g: &Generator) -> (String, Value) {
    match g {
        Generator::SolidColor { rgba } => ("SolidColor".into(), json!({"color": rgba})),
        Generator::ColorBars => ("SMPTEBars".into(), json!({})),
        Generator::Adjustment => ("Adjustment".into(), json!({})),
        Generator::Checkerboard { size } => ("Checkerboard".into(), json!({"size": size})),
        Generator::Gradient {
            from,
            to,
            angle_deg,
        } => (
            "Gradient".into(),
            json!({"from": from, "to": to, "angle": angle_deg}),
        ),
        Generator::Noise { amount, color } => {
            ("Noise".into(), json!({"amount": amount, "color": color}))
        }
        Generator::TestTone {
            frequency_hz,
            level_db,
        } => (
            "Tone".into(),
            json!({"frequency": frequency_hz, "level": level_db}),
        ),
        Generator::Plugin(fx) => (fx.id.clone(), json!({})),
    }
}

/// A numbered picture's name in parts: before the number, the number's
/// digits and value, after it.
fn numbered(path: &str) -> Option<(String, usize, u64, String)> {
    let (start, end, value) = ImageSequence::number_of(path)?;
    let name = path.rfind(['/', '\\']).map_or(0, |i| i + 1);
    Some((
        path[name..start].to_string(),
        end - start,
        value,
        path[end..].to_string(),
    ))
}

struct Writer<'a> {
    p: &'a Project,
    rate: FrameRate,
    url: &'a dyn Fn(&str) -> Option<String>,
}

impl Writer<'_> {
    fn time(&self, t: Tick) -> Value {
        rational(t, self.rate)
    }

    fn range(&self, start: Tick, duration: Tick) -> Value {
        time_range(start, duration, self.rate)
    }

    fn timeline(&self, name: &str) -> Value {
        let p = self.p;
        // The project without what the tracks, clips and markers say (the
        // media no event uses stay: Project Media lists them).
        let used: HashSet<MediaId> = p.clips().map(|(_, c)| c.media).collect();
        let mut rest = p.clone();
        rest.tracks.clear();
        rest.media.retain(|m| !used.contains(&m.id));
        rest.markers.clear();
        rest.regions.clear();
        let video = p.tracks.iter().filter(|t| t.kind == TrackKind::Video);
        let audio = p.tracks.iter().filter(|t| t.kind == TrackKind::Audio);
        let mut tracks = Vec::new();
        // A stack's tracks go from the bottom up.
        for t in video.rev().chain(audio) {
            for (n, lane) in lanes(t, p.auto_crossfade).iter().enumerate() {
                tracks.push(self.track(t, n, lane));
            }
        }
        json!({
            "OTIO_SCHEMA": "Timeline.1",
            "global_start_time": self.time(p.ruler_offset),
            "metadata": {KEY: {"version": METADATA_VERSION, "project": rest}},
            "name": name,
            "tracks": {
                "OTIO_SCHEMA": "Stack.1",
                "children": tracks,
                "effects": p.video_output_fx.iter().map(effect).collect::<Vec<_>>(),
                "enabled": true,
                "markers": self.markers(),
                "metadata": {},
                "name": "tracks",
                "source_range": null,
            },
        })
    }

    /// The timeline's markers and regions, on the stack.
    fn markers(&self) -> Vec<Value> {
        let p = self.p;
        let markers = p
            .markers
            .iter()
            .map(|m| self.marker(&m.name, m.time, Tick::ZERO));
        let regions = p
            .regions
            .iter()
            .map(|r| self.marker(&r.name, r.range.start, r.range.duration));
        markers.chain(regions).collect()
    }

    /// A `Marker.2`: orange for a marker, green for a region, as BananaV
    /// draws them.
    fn marker(&self, name: &str, at: Tick, length: Tick) -> Value {
        json!({
            "OTIO_SCHEMA": "Marker.2",
            "color": if length > Tick::ZERO { "GREEN" } else { "ORANGE" },
            "comment": "",
            "marked_range": self.range(at, length),
            "metadata": {},
            "name": name,
        })
    }

    /// Lane `lane` of track `t`: the track itself with its settings (in
    /// BananaV's metadata), or one over it with the events that did not
    /// fit below.
    fn track(&self, t: &Track, lane: usize, clips: &[&Clip]) -> Value {
        let mut ours = json!({"key": t.id.0, "lane": lane});
        let mut effects = Vec::new();
        let name = if lane == 0 {
            let mut settings = t.clone();
            settings.clips.clear();
            ours["track"] = serde_json::to_value(&settings).unwrap_or_default();
            effects = t.effects.iter().map(effect).collect();
            t.name.clone()
        } else {
            format!("{} ({})", t.name, lane + 1)
        };
        json!({
            "OTIO_SCHEMA": "Track.1",
            "children": self.items(t, clips),
            "effects": effects,
            "enabled": !t.muted,
            "kind": kind_name(t.kind),
            "markers": [],
            "metadata": {KEY: ours},
            "name": name,
            "source_range": null,
        })
    }

    /// Where the crossfade of `a` into `b` is cut: the frame nearest the
    /// middle of their overlap.
    fn cut(&self, a: &Clip, b: &Clip) -> Tick {
        let (lo, hi) = (b.start, a.end());
        let middle = lo + Tick((hi - lo).0 / 2);
        self.rate.snap_nearest(middle).clamp(lo, hi)
    }

    /// A lane's items: its events as clips, the time between them as gaps,
    /// crossfades as transitions centred on cuts, fades as transitions to
    /// and from gaps (of no length where nothing is between).
    fn items(&self, t: &Track, lane: &[&Clip]) -> Vec<Value> {
        let mut out = Vec::new();
        let mut at = Tick::ZERO;
        // A fade-out's transition waits for the gap after it.
        let mut open = false;
        for (i, &c) in lane.iter().enumerate() {
            let from = i
                .checked_sub(1)
                .map(|j| lane[j])
                .filter(|p| p.end() > c.start);
            let into = lane.get(i + 1).copied().filter(|n| n.start < c.end());
            // What the event plays: where another event overlaps it
            // (another lane's too), the crossfade instead of its own fade.
            let (fade_in, fade_out) = t.effective_fades(c, self.p.auto_crossfade);
            let room = into.map_or(c.end(), |n| n.start) - c.start;
            let fade_in = fade_in.clamp(Tick::ZERO, room.max(Tick::ZERO));
            let (a, head) = match from {
                Some(p) => {
                    let cut = self.cut(p, c);
                    out.push(self.transition(t.kind, Some(c), cut - c.start, p.end() - cut));
                    (cut, p.end())
                }
                None => {
                    if c.start > at || open || fade_in > Tick::ZERO {
                        out.push(self.gap(c.start - at));
                        open = false;
                    }
                    if fade_in > Tick::ZERO {
                        out.push(self.transition(t.kind, Some(c), Tick::ZERO, fade_in));
                    }
                    (c.start, c.start + fade_in)
                }
            };
            let b = into.map_or(c.end(), |n| self.cut(c, n));
            out.push(self.clip(c, a, b));
            at = b;
            let fade_out = fade_out.clamp(Tick::ZERO, (c.end() - head).max(Tick::ZERO));
            if into.is_none() && fade_out > Tick::ZERO {
                out.push(self.transition(t.kind, None, fade_out, Tick::ZERO));
                open = true;
            }
        }
        if open {
            out.push(self.gap(Tick::ZERO));
        }
        out
    }

    fn gap(&self, duration: Tick) -> Value {
        json!({
            "OTIO_SCHEMA": "Gap.1",
            "effects": [],
            "enabled": true,
            "markers": [],
            "metadata": {},
            "name": "",
            "source_range": self.range(Tick::ZERO, duration),
        })
    }

    /// A transition `in_offset` before its cut and `out_offset` after it:
    /// how `entering` enters (`None`: a fade-out). A dissolve is SMPTE's;
    /// BananaV's others are custom ones, named.
    fn transition(
        &self,
        kind: TrackKind,
        entering: Option<&Clip>,
        in_offset: Tick,
        out_offset: Tick,
    ) -> Value {
        let shape = entering.map_or(Transition::Dissolve, |c| c.transition);
        let plugin = entering.and_then(|c| c.transition_fx.as_deref());
        let (otio_type, name) = match (kind, plugin, shape) {
            (TrackKind::Audio, ..) => ("SMPTE_Dissolve", "Crossfade".to_string()),
            (_, Some(fx), _) => ("Custom_Transition", fx.name.clone()),
            (_, None, Transition::Dissolve) => ("SMPTE_Dissolve", shape.name().to_string()),
            (_, None, other) => ("Custom_Transition", other.name().to_string()),
        };
        json!({
            "OTIO_SCHEMA": "Transition.1",
            "in_offset": self.time(in_offset),
            "metadata": {KEY: {"transition": shape}},
            "name": name,
            "out_offset": self.time(out_offset),
            "transition_type": otio_type,
        })
    }

    /// Event `c` as a clip from `a` to `b` on its track (between its cuts):
    /// the media it shows there, its speed or frozen frame, its effects,
    /// the media's markers it shows.
    fn clip(&self, c: &Clip, a: Tick, b: Tick) -> Value {
        let media = self.p.media(c.media);
        // Reversed, the part shows its media from its end down: the range
        // starts at the lowest time it shows, the speed below zero.
        let reversed = c.reverse.is_some();
        let start = c.freeze.unwrap_or_else(|| {
            if reversed {
                c.source_time(b)
            } else {
                c.source_time(a)
            }
        });
        let mut effects = Vec::new();
        if c.freeze.is_some() {
            effects.push(json!({
                "OTIO_SCHEMA": "FreezeFrame.1",
                "effect_name": "FreezeFrame",
                "metadata": {},
                "name": "",
                "time_scalar": 0.0,
            }));
        } else if c.rate != 1.0 || reversed {
            effects.push(json!({
                "OTIO_SCHEMA": "LinearTimeWarp.1",
                "effect_name": "LinearTimeWarp",
                "metadata": {},
                "name": "",
                "time_scalar": if reversed { -c.rate } else { c.rate },
            }));
        }
        effects.extend(c.effects.iter().map(effect));
        let shown = if c.freeze.is_some() {
            start..start
        } else {
            start..start + c.to_source(b - a)
        };
        let markers: Vec<Value> = media
            .into_iter()
            .flat_map(|m| &m.markers)
            .filter(|m| shown.contains(&m.time))
            .map(|m| self.marker(&m.name, m.time, m.length))
            .collect();
        json!({
            "OTIO_SCHEMA": "Clip.2",
            "active_media_reference_key": DEFAULT_MEDIA,
            "effects": effects,
            "enabled": !c.muted,
            "markers": markers,
            "media_references": {DEFAULT_MEDIA: self.reference(media)},
            "metadata": {KEY: {"event": c}},
            "name": media.map_or("", |m| m.name.as_str()),
            "source_range": self.range(start, b - a),
        })
    }

    /// A media as a reference: a file's address, an image sequence's
    /// pictures, a generator; BananaV's whole media in its metadata.
    fn reference(&self, m: Option<&MediaInfo>) -> Value {
        let missing = |name: &str, metadata: Value| {
            json!({
                "OTIO_SCHEMA": "MissingReference.1",
                "available_image_bounds": null,
                "available_range": null,
                "metadata": metadata,
                "name": name,
            })
        };
        let Some(m) = m else {
            return missing("", json!({}));
        };
        let available = self.range(Tick::ZERO, m.duration);
        let metadata = json!({KEY: {"media": m}});
        match &m.source {
            MediaSource::File { path } => {
                match ((self.url)(path), m.sequence.and(numbered(path))) {
                    (None, _) => missing(&m.name, metadata),
                    (Some(first), Some((prefix, digits, number, suffix))) => {
                        let base = first
                            .rsplit_once('/')
                            .map_or(String::new(), |(base, _)| format!("{base}/"));
                        let rate = m.video.map_or(self.rate, |v| v.frame_rate);
                        json!({
                            "OTIO_SCHEMA": "ImageSequenceReference.1",
                            "available_image_bounds": null,
                            "available_range": available,
                            "frame_step": 1,
                            "frame_zero_padding": digits,
                            "metadata": metadata,
                            "missing_frame_policy": "error",
                            "name": m.name,
                            "name_prefix": prefix,
                            "name_suffix": suffix,
                            "rate": rate.as_f64(),
                            "start_frame": number,
                            "target_url_base": base,
                        })
                    }
                    (Some(url), None) => json!({
                        "OTIO_SCHEMA": "ExternalReference.1",
                        "available_image_bounds": null,
                        "available_range": available,
                        "metadata": metadata,
                        "name": m.name,
                        "target_url": url,
                    }),
                }
            }
            MediaSource::Generated(g) => {
                let (kind, parameters) = generator(g);
                json!({
                    "OTIO_SCHEMA": "GeneratorReference.1",
                    "available_image_bounds": null,
                    "available_range": available,
                    "generator_kind": kind,
                    "metadata": metadata,
                    "name": m.name,
                    "parameters": parameters,
                })
            }
        }
    }
}
