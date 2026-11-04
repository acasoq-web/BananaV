//! A project written as Final Cut Pro 7 XML, as its readers expect it
//! (Final Cut Pro 7's, DaVinci Resolve's, Premiere's and VEGAS's files):
//! one sequence at the top, each file described at its first clip item,
//! crossfades and fades as transitions next to clip items whose `start` or
//! `end` is −1, sound as stereo pairs of mono clip items on two tracks.

use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Write as _};

use bv_core::{FieldOrder, FrameRate, Tick, timecode};

use super::{CROSSFADE_0DB, CROSSFADE_3DB, timebase};
use crate::fcp_import::TITLES_PLUGIN;
use crate::fx::{FadeCurve, PluginValue, Transition};
use crate::model::{
    ChannelMap, Clip, ClipId, Generator, GroupId, MediaId, MediaInfo, MediaSource, Project, Track,
    TrackKind,
};
use crate::otio::lanes;

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A number as short as it can be written (`100`, `0.5`).
fn num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// XML text, an element a line, indented by tabs.
#[derive(Default)]
struct Xml {
    s: String,
    depth: usize,
}

impl Xml {
    fn line(&mut self, text: &str) {
        for _ in 0..self.depth {
            self.s.push('\t');
        }
        self.s.push_str(text);
        self.s.push('\n');
    }

    fn tag(name: &str, attrs: &[(&str, &str)], end: &str) -> String {
        let mut t = format!("<{name}");
        for (k, v) in attrs {
            let _ = write!(t, " {k}=\"{}\"", escape(v));
        }
        t.push_str(end);
        t
    }

    fn open(&mut self, name: &str) {
        self.open_with(name, &[]);
    }

    fn open_with(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.line(&Xml::tag(name, attrs, ">"));
        self.depth += 1;
    }

    fn close(&mut self, name: &str) {
        self.depth -= 1;
        self.line(&format!("</{name}>"));
    }

    fn empty_with(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.line(&Xml::tag(name, attrs, "/>"));
    }

    fn leaf(&mut self, name: &str, text: impl Display) {
        let text = escape(&text.to_string());
        self.line(&format!("<{name}>{text}</{name}>"));
    }

    fn flag(&mut self, name: &str, on: bool) {
        self.leaf(name, if on { "TRUE" } else { "FALSE" });
    }

    fn rate(&mut self, rate: FrameRate) {
        let (base, ntsc) = timebase(rate);
        self.open("rate");
        self.leaf("timebase", base);
        self.flag("ntsc", ntsc);
        self.close("rate");
    }

    /// A parameter of an effect: its id, name, range and value.
    fn parameter(&mut self, id: &str, name: &str, range: Option<(f64, f64)>, value: impl Display) {
        self.open("parameter");
        self.leaf("parameterid", id);
        self.leaf("name", name);
        if let Some((lo, hi)) = range {
            self.leaf("valuemin", num(lo));
            self.leaf("valuemax", num(hi));
        }
        self.leaf("value", value);
        self.close("parameter");
    }

    /// An effect's head: name, id, category, type and media.
    fn effect(&mut self, name: &str, id: &str, category: &str, kind: &str, media: &str) {
        self.open("effect");
        self.leaf("name", name);
        self.leaf("effectid", id);
        self.leaf("effectcategory", category);
        self.leaf("effecttype", kind);
        self.leaf("mediatype", media);
    }
}

/// A path as Final Cut Pro 7's `pathurl` (`file://localhost/C:/…`, a
/// share's `file://server/…`): what is not a letter, a digit or `-._~/:`
/// percent-encoded, as UTF-8.
fn path_url(path: &str) -> String {
    let p = path.replace('\\', "/");
    let mut out = String::from(if p.starts_with("//") {
        "file:"
    } else if p.starts_with('/') {
        "file://localhost"
    } else {
        "file://localhost/"
    });
    for b in p.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(b as char)
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Whole frames at `rate` closest to `t`.
fn frames(t: Tick, rate: FrameRate) -> i64 {
    rate.frame_at(rate.snap_nearest(t))
}

/// A track's name as its user gave it: none for BananaV's own ("Video 2").
fn given_name(t: &Track) -> &str {
    let own = match t.kind {
        TrackKind::Video => "Video ",
        TrackKind::Audio => "Audio ",
    };
    let name = t.name.trim();
    if name
        .strip_prefix(own)
        .is_some_and(|n| n.parse::<u32>().is_ok())
    {
        ""
    } else {
        name
    }
}

fn field_dominance(f: FieldOrder) -> &'static str {
    match f {
        FieldOrder::Progressive => "none",
        FieldOrder::UpperFirst => "upper",
        FieldOrder::LowerFirst => "lower",
    }
}

/// What a clip item shows.
enum Shows<'a> {
    /// A file: the media and its path.
    File(&'a MediaInfo, &'a str),
    /// A title saying its text (Final Cut Pro 7's Text generator).
    Text(String),
    /// A solid color (its Color generator), 8-bit RGBA.
    Color([u8; 4]),
}

/// A clip item to write: an event, or one side of its sound.
struct Item<'a> {
    id: String,
    clip: &'a Clip,
    shows: Shows<'a>,
    kind: TrackKind,
    /// Sound: the file's channel it plays (1-based, across its streams)
    /// and its pan.
    channel: u16,
    pan: f64,
    /// The sequence's frames; −1 where a transition places it.
    start: i64,
    end: i64,
    /// Its media's frames shown, transitions' handles included.
    src: (i64, i64),
    /// Its track's number among its kind's, and its own among the track's
    /// items (1-based).
    track: usize,
    index: usize,
}

/// A transition item.
struct Trans {
    start: i64,
    end: i64,
    align: &'static str,
    /// Its effect's name and id.
    effect: (&'static str, &'static str),
    kind: TrackKind,
}

enum Piece {
    Item(usize),
    Trans(Trans),
}

/// A track to write: a lane of a BananaV track, a side of its sound.
struct XTrack {
    kind: TrackKind,
    name: String,
    enabled: bool,
    /// Sound: the side of the stereo pair (1: left, 2: right).
    side: u8,
    pieces: Vec<Piece>,
}

struct Writer<'a> {
    p: &'a Project,
    rate: FrameRate,
    items: Vec<Item<'a>>,
    tracks: Vec<XTrack>,
    /// Each event's clip items.
    of_event: HashMap<ClipId, Vec<usize>>,
    /// Each file's number, in the order first used.
    files: HashMap<MediaId, usize>,
    /// Events written only in part, or not at all.
    partial: HashSet<ClipId>,
}

impl<'a> Writer<'a> {
    fn f(&self, t: Tick) -> i64 {
        frames(t, self.rate)
    }

    /// What event `c` shows, as a clip item can say it (`None`: nothing
    /// another program could open).
    fn shows(&self, c: &Clip, kind: TrackKind) -> Option<Shows<'a>> {
        let m = self.p.media(c.media)?;
        match &m.source {
            MediaSource::File { path } if !path.to_lowercase().ends_with(".bvproj") => {
                Some(Shows::File(m, path))
            }
            MediaSource::Generated(Generator::Plugin(fx))
                if kind == TrackKind::Video && fx.id == TITLES_PLUGIN =>
            {
                match fx.params.get("text").map(|p| &p.value) {
                    Some(PluginValue::Text(t)) if !t.trim().is_empty() => {
                        Some(Shows::Text(t.clone()))
                    }
                    _ => None,
                }
            }
            MediaSource::Generated(Generator::SolidColor { rgba }) if kind == TrackKind::Video => {
                let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                Some(Shows::Color([
                    b(rgba[0]),
                    b(rgba[1]),
                    b(rgba[2]),
                    b(rgba[3]),
                ]))
            }
            _ => None,
        }
    }

    /// Whether the format says all event `c` does.
    fn whole(&self, c: &Clip) -> bool {
        c.effects.is_empty()
            && c.pan_crop.is_default()
            && c.mask.is_none()
            && c.freeze.is_none()
            && c.reverse.is_none()
            && c.velocity().is_none()
            && c.undersample == 1.0
            && c.pitch == 0.0
            && c.transition_fx.is_none()
            && matches!(
                c.transition,
                Transition::Dissolve | Transition::FadeThroughBlack
            )
    }

    /// Sound: the file's channel (1-based, across its streams) side `side`
    /// of event `c` plays (1: left, 2: right), and its pan.
    fn channel(&self, c: &Clip, side: u8) -> (u16, f64) {
        let streams: Vec<u16> = self
            .p
            .media(c.media)
            .map(|m| {
                m.audio
                    .iter()
                    .chain(&m.extra_audio)
                    .map(|a| a.channels.max(1))
                    .collect()
            })
            .unwrap_or_default();
        let s = usize::from(c.audio_stream).min(streams.len().saturating_sub(1));
        let base = 1 + streams.iter().take(s).sum::<u16>();
        let second = if streams.get(s).copied().unwrap_or(2) >= 2 {
            base + 1
        } else {
            base
        };
        let (left, right, pans) = match c.channels {
            ChannelMap::Both => (base, second, (-1.0, 1.0)),
            ChannelMap::LeftOnly => (base, base, (-1.0, 1.0)),
            ChannelMap::RightOnly => (second, second, (-1.0, 1.0)),
            ChannelMap::Swap => (second, base, (-1.0, 1.0)),
            ChannelMap::Combine => (base, second, (0.0, 0.0)),
        };
        if side == 1 {
            (left, pans.0)
        } else {
            (right, pans.1)
        }
    }

    /// A crossfade's or a fade's transition: a picture's, or a sound's by
    /// its curves (constant gain where they are linear, else power).
    fn transition(
        kind: TrackKind,
        entering: Option<&Clip>,
        curves: &[FadeCurve],
    ) -> (&'static str, &'static str) {
        match kind {
            TrackKind::Video => match entering.map(|c| c.transition) {
                Some(Transition::FadeThroughBlack) => {
                    ("Dip to Color Dissolve", "Dip to Color Dissolve")
                }
                _ => ("Cross Dissolve", "Cross Dissolve"),
            },
            TrackKind::Audio if curves.iter().all(|&c| c == FadeCurve::Linear) => CROSSFADE_0DB,
            TrackKind::Audio => CROSSFADE_3DB,
        }
    }

    /// Lane `lane` of track `t` as a track (`side`: sound's left or right):
    /// its events as clip items, crossfades and fades as transitions.
    fn track(&mut self, t: &'a Track, lane: &[&'a Clip], side: u8) {
        let p = self.p;
        let kind = t.kind;
        let number = 1 + self.tracks.iter().filter(|x| x.kind == kind).count();
        // What another program can open; the rest left out.
        let lane: Vec<&Clip> = lane
            .iter()
            .copied()
            .filter(|c| {
                let ok = self.shows(c, kind).is_some();
                if !ok {
                    self.partial.insert(c.id);
                }
                ok
            })
            .collect();
        let mut pieces = Vec::new();
        for (i, &c) in lane.iter().enumerate() {
            let from = i
                .checked_sub(1)
                .map(|j| lane[j])
                .filter(|q| q.end() > c.start);
            let into = lane.get(i + 1).copied().filter(|n| n.start < c.end());
            let (fade_in, fade_out) = t.effective_fades(c, p.auto_crossfade);
            let room = into.map_or(c.end(), |n| n.start) - c.start;
            let fade_in = match from {
                Some(_) => Tick::ZERO,
                None => fade_in.clamp(Tick::ZERO, room.max(Tick::ZERO)),
            };
            let head_end = from.map_or(c.start + fade_in, |q| q.end());
            let fade_out = match into {
                Some(_) => Tick::ZERO,
                None => fade_out.clamp(Tick::ZERO, (c.end() - head_end).max(Tick::ZERO)),
            };
            if fade_in > Tick::ZERO {
                pieces.push(Piece::Trans(Trans {
                    start: self.f(c.start),
                    end: self.f(c.start + fade_in),
                    align: "start-black",
                    effect: Writer::transition(kind, Some(c), &[c.fade_in_curve]),
                    kind,
                }));
            }
            if !self.whole(c) {
                self.partial.insert(c.id);
            }
            let shown = match c.freeze {
                Some(_) => c.duration,
                None => c.to_source(c.duration),
            };
            let src_in = c.freeze.unwrap_or(c.source_in);
            let (channel, pan) = match kind {
                TrackKind::Audio => self.channel(c, side),
                TrackKind::Video => (0, 0.0),
            };
            let index = 1 + pieces
                .iter()
                .filter(|p| matches!(p, Piece::Item(_)))
                .count();
            let item = Item {
                id: format!("clipitem-{}", self.items.len() + 1),
                clip: c,
                shows: self.shows(c, kind).expect("filtered above"),
                kind,
                channel,
                pan,
                start: if from.is_some() || fade_in > Tick::ZERO {
                    -1
                } else {
                    self.f(c.start)
                },
                end: if into.is_some() || fade_out > Tick::ZERO {
                    -1
                } else {
                    self.f(c.end())
                },
                src: (self.f(src_in), self.f(src_in + shown)),
                track: number,
                index,
            };
            if let Shows::File(m, _) = item.shows {
                let next = self.files.len() + 1;
                self.files.entry(m.id).or_insert(next);
            }
            self.of_event
                .entry(c.id)
                .or_default()
                .push(self.items.len());
            pieces.push(Piece::Item(self.items.len()));
            self.items.push(item);
            if let Some(n) = into {
                pieces.push(Piece::Trans(Trans {
                    start: self.f(n.start),
                    end: self.f(c.end()),
                    align: "center",
                    effect: Writer::transition(kind, Some(n), &[c.fade_out_curve, n.fade_in_curve]),
                    kind,
                }));
            } else if fade_out > Tick::ZERO {
                pieces.push(Piece::Trans(Trans {
                    start: self.f(c.end() - fade_out),
                    end: self.f(c.end()),
                    align: "end-black",
                    effect: Writer::transition(kind, None, &[c.fade_out_curve]),
                    kind,
                }));
            }
        }
        self.tracks.push(XTrack {
            kind,
            name: given_name(t).to_string(),
            enabled: !t.muted,
            side,
            pieces,
        });
    }

    /// The project's tracks: video from the bottom up (V1 is the lowest),
    /// then sound from the top down, each of its tracks a pair (left,
    /// right); a track's events that overlap more than a crossfade does on
    /// tracks of their own after it.
    fn plan(&mut self) {
        let p = self.p;
        let crossfades = p.auto_crossfade;
        let video: Vec<&Track> = p
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Video)
            .collect();
        for &t in video.iter().rev() {
            for lane in lanes(t, crossfades) {
                self.track(t, &lane, 0);
            }
        }
        for t in p.tracks.iter().filter(|t| t.kind == TrackKind::Audio) {
            for lane in lanes(t, crossfades) {
                self.track(t, &lane, 1);
                self.track(t, &lane, 2);
            }
        }
    }

    /// How far into its media the document reaches: its length, or a
    /// still's (any length) as far as its events show it.
    fn length(&self, m: &MediaInfo) -> Tick {
        let used = self
            .items
            .iter()
            .filter(|i| i.clip.media == m.id)
            .map(|i| self.rate.frame_start(i.src.1))
            .max()
            .unwrap_or(Tick::ZERO);
        if m.still { used } else { m.duration.max(used) }
    }

    /// A media's file: described where it first appears, named after.
    fn file(&self, x: &mut Xml, m: &MediaInfo, path: &str, written: &mut HashSet<MediaId>) {
        let id = format!("file-{}", self.files.get(&m.id).copied().unwrap_or(0));
        if !written.insert(m.id) {
            x.empty_with("file", &[("id", &id)]);
            return;
        }
        x.open_with("file", &[("id", &id)]);
        x.leaf("name", file_name(path));
        x.leaf("pathurl", path_url(path));
        let own = m.video.map_or(self.rate, |v| v.frame_rate);
        x.rate(own);
        x.leaf("duration", frames(self.length(m), own));
        let tc = m.reel.timecode.unwrap_or(Tick::ZERO);
        x.open("timecode");
        x.rate(own);
        x.leaf("string", timecode::format(tc, own));
        x.leaf("frame", frames(tc, own));
        x.leaf(
            "displayformat",
            if own.is_drop_frame() { "DF" } else { "NDF" },
        );
        if !m.reel.tape_name.is_empty() {
            x.leaf("reel", &m.reel.tape_name);
        }
        x.close("timecode");
        x.open("media");
        if let Some(v) = m.video {
            x.open("video");
            x.open("samplecharacteristics");
            x.rate(v.frame_rate);
            x.leaf("width", v.width);
            x.leaf("height", v.height);
            x.flag("anamorphic", false);
            x.leaf("pixelaspectratio", "square");
            x.leaf("fielddominance", field_dominance(v.field_order));
            x.close("samplecharacteristics");
            x.close("video");
        }
        for a in m.audio.iter().chain(&m.extra_audio) {
            x.open("audio");
            x.open("samplecharacteristics");
            x.leaf("depth", 16);
            x.leaf("samplerate", a.sample_rate.0);
            x.close("samplecharacteristics");
            x.leaf("channelcount", a.channels.max(1));
            x.close("audio");
        }
        x.close("media");
        x.close("file");
    }

    /// The clip items linked to item `k`: its event's group's (its own
    /// other side for sound).
    fn linked(&self, k: &Item<'_>, groups: &HashMap<GroupId, Vec<usize>>) -> Vec<usize> {
        match k.clip.group.and_then(|g| groups.get(&g)) {
            Some(members) => members.clone(),
            None => self.of_event.get(&k.clip.id).cloned().unwrap_or_default(),
        }
    }

    fn filter_level(x: &mut Xml, kind: TrackKind, level: f32) {
        x.open("filter");
        match kind {
            TrackKind::Video => {
                x.effect("Opacity", "opacity", "motion", "motion", "video");
                x.parameter(
                    "opacity",
                    "opacity",
                    Some((0.0, 100.0)),
                    num(f64::from(level) * 100.0),
                );
            }
            TrackKind::Audio => {
                x.effect(
                    "Audio Levels",
                    "audiolevels",
                    "audiolevels",
                    "audiolevels",
                    "audio",
                );
                x.parameter(
                    "level",
                    "Level",
                    Some((0.0, 3.98109)),
                    num(f64::from(level)),
                );
            }
        }
        x.close("effect");
        x.close("filter");
    }

    fn filter_pan(x: &mut Xml, pan: f64) {
        x.open("filter");
        x.effect("Audio Pan", "audiopan", "audiopan", "audiopan", "audio");
        x.parameter("pan", "Pan", Some((-1.0, 1.0)), num(pan));
        x.close("effect");
        x.close("filter");
    }

    /// Final Cut Pro 7's Time Remap at a constant speed.
    fn filter_speed(x: &mut Xml, kind: TrackKind, rate: f64) {
        let media = match kind {
            TrackKind::Video => "video",
            TrackKind::Audio => "audio",
        };
        x.open("filter");
        x.effect("Time Remap", "timeremap", "motion", "motion", media);
        x.parameter("variablespeed", "variablespeed", Some((0.0, 1.0)), 0);
        x.parameter(
            "speed",
            "speed",
            Some((-100000.0, 100000.0)),
            num(rate * 100.0),
        );
        x.parameter("reverse", "reverse", None, "FALSE");
        x.parameter("frameblending", "frameblending", None, "FALSE");
        x.close("effect");
        x.close("filter");
    }

    fn item(
        &self,
        x: &mut Xml,
        k: &Item<'_>,
        groups: &HashMap<GroupId, Vec<usize>>,
        written: &mut HashSet<MediaId>,
    ) {
        let c = k.clip;
        let sound = k.kind == TrackKind::Audio;
        let length = |n: i64| n.max(1);
        match &k.shows {
            Shows::File(m, path) => {
                let mut attrs = vec![("id", k.id.as_str())];
                if sound {
                    attrs.push(("premiereChannelType", "stereo"));
                }
                x.open_with("clipitem", &attrs);
                let master = format!("masterclip-{}", self.files.get(&m.id).copied().unwrap_or(0));
                x.leaf("masterclipid", &master);
                x.leaf("name", &m.name);
                x.flag("enabled", !c.muted);
                x.leaf("duration", length(self.f(self.length(m))));
                x.rate(self.rate);
                x.leaf("start", k.start);
                x.leaf("end", k.end);
                x.leaf("in", k.src.0);
                x.leaf("out", k.src.1);
                if !sound {
                    x.leaf("alphatype", "none");
                    x.leaf("pixelaspectratio", "square");
                    x.flag("anamorphic", false);
                }
                self.file(x, m, path, written);
                Writer::filter_level(x, k.kind, c.level);
                if sound {
                    Writer::filter_pan(x, k.pan);
                }
                if c.rate != 1.0 && c.freeze.is_none() {
                    Writer::filter_speed(x, k.kind, c.rate);
                }
                x.open("sourcetrack");
                x.leaf("mediatype", if sound { "audio" } else { "video" });
                x.leaf("trackindex", if sound { k.channel } else { 1 });
                x.close("sourcetrack");
                let linked = self.linked(k, groups);
                if linked.len() > 1 {
                    for i in linked {
                        let o = &self.items[i];
                        let audio = o.kind == TrackKind::Audio;
                        x.open("link");
                        x.leaf("linkclipref", &o.id);
                        x.leaf("mediatype", if audio { "audio" } else { "video" });
                        x.leaf("trackindex", o.track);
                        x.leaf("clipindex", o.index);
                        if audio {
                            x.leaf("groupindex", 1);
                        }
                        x.close("link");
                    }
                }
                // The media's markers it shows.
                let (from, to) = (
                    self.rate.frame_start(k.src.0),
                    self.rate.frame_start(k.src.1),
                );
                for mark in m
                    .markers
                    .iter()
                    .filter(|mk| mk.time >= from && mk.time < to)
                {
                    x.open("marker");
                    x.leaf("name", &mark.name);
                    x.leaf("comment", "");
                    x.leaf("in", self.f(mark.time));
                    x.leaf(
                        "out",
                        match mark.length > Tick::ZERO {
                            true => self.f(mark.time + mark.length),
                            false => -1,
                        },
                    );
                    x.close("marker");
                }
                x.close("clipitem");
            }
            Shows::Text(text) => {
                x.open_with("generatoritem", &[("id", k.id.as_str())]);
                x.leaf("name", "Text");
                x.flag("enabled", !c.muted);
                let len = self.f(c.end()) - self.f(c.start);
                x.leaf("duration", length(len));
                x.rate(self.rate);
                x.leaf("start", k.start);
                x.leaf("end", k.end);
                x.leaf("in", 0);
                x.leaf("out", len);
                x.effect("Text", "Text", "Text", "generator", "video");
                x.parameter("str", "Text", None, text);
                x.close("effect");
                Writer::filter_level(x, k.kind, c.level);
                x.close("generatoritem");
            }
            Shows::Color(rgba) => {
                x.open_with("generatoritem", &[("id", k.id.as_str())]);
                x.leaf("name", "Color");
                x.flag("enabled", !c.muted);
                let len = self.f(c.end()) - self.f(c.start);
                x.leaf("duration", length(len));
                x.rate(self.rate);
                x.leaf("start", k.start);
                x.leaf("end", k.end);
                x.leaf("in", 0);
                x.leaf("out", len);
                x.effect("Color", "Color", "Matte", "generator", "video");
                x.open("parameter");
                x.leaf("parameterid", "color");
                x.leaf("name", "Color");
                x.open("value");
                x.leaf("alpha", rgba[3]);
                x.leaf("red", rgba[0]);
                x.leaf("green", rgba[1]);
                x.leaf("blue", rgba[2]);
                x.close("value");
                x.close("parameter");
                x.close("effect");
                Writer::filter_level(x, k.kind, c.level);
                x.close("generatoritem");
            }
        }
    }

    fn write(&self, name: &str) -> String {
        let p = self.p;
        let s = &p.settings;
        let rate = self.rate;
        let mut groups: HashMap<GroupId, Vec<usize>> = HashMap::new();
        for (i, item) in self.items.iter().enumerate() {
            if let Some(g) = item.clip.group {
                groups.entry(g).or_default().push(i);
            }
        }
        let mut x = Xml::default();
        x.s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE xmeml>\n");
        x.open_with("xmeml", &[("version", "5")]);
        x.open_with("sequence", &[("id", "sequence-1")]);
        x.leaf("name", name);
        x.leaf("duration", self.f(p.duration()));
        x.rate(rate);
        x.open("timecode");
        x.rate(rate);
        x.leaf("string", timecode::format(p.ruler_offset, rate));
        x.leaf("frame", self.f(p.ruler_offset));
        x.leaf(
            "displayformat",
            if rate.is_drop_frame() { "DF" } else { "NDF" },
        );
        x.close("timecode");
        x.leaf("in", -1);
        x.leaf("out", -1);
        x.open("media");
        let mut written = HashSet::new();
        for kind in [TrackKind::Video, TrackKind::Audio] {
            let sound = kind == TrackKind::Audio;
            x.open(if sound { "audio" } else { "video" });
            if sound {
                x.leaf("numOutputChannels", 2);
                x.open("format");
                x.open("samplecharacteristics");
                x.leaf("depth", 16);
                x.leaf("samplerate", s.sample_rate.0);
                x.close("samplecharacteristics");
                x.close("format");
                x.open("outputs");
                for side in 1..=2 {
                    x.open("group");
                    x.leaf("index", side);
                    x.leaf("numchannels", 1);
                    x.leaf("downmix", 0);
                    x.open("channel");
                    x.leaf("index", side);
                    x.close("channel");
                    x.close("group");
                }
                x.close("outputs");
            } else {
                let (aspect, anamorphic) = pixel_aspect(s.pixel_aspect.ratio());
                x.open("format");
                x.open("samplecharacteristics");
                x.rate(rate);
                x.leaf("width", s.width);
                x.leaf("height", s.height);
                x.flag("anamorphic", anamorphic);
                x.leaf("pixelaspectratio", aspect);
                x.leaf("fielddominance", field_dominance(s.field_order));
                x.leaf("colordepth", 24);
                x.close("samplecharacteristics");
                x.close("format");
            }
            for t in self.tracks.iter().filter(|t| t.kind == kind) {
                let side = (t.side.max(1) - 1).to_string();
                let mut attrs: Vec<(&str, &str)> = Vec::new();
                if sound {
                    attrs.push(("currentExplodedTrackIndex", &side));
                    attrs.push(("totalExplodedTrackCount", "2"));
                    attrs.push(("premiereTrackType", "Stereo"));
                }
                if !t.name.is_empty() {
                    attrs.push(("MZ.TrackName", &t.name));
                }
                x.open_with("track", &attrs);
                for piece in &t.pieces {
                    match piece {
                        Piece::Item(i) => self.item(&mut x, &self.items[*i], &groups, &mut written),
                        Piece::Trans(tr) => {
                            x.open("transitionitem");
                            x.rate(rate);
                            x.leaf("start", tr.start);
                            x.leaf("end", tr.end);
                            x.leaf("alignment", tr.align);
                            x.open("effect");
                            x.leaf("name", tr.effect.0);
                            x.leaf("effectid", tr.effect.1);
                            if tr.kind == TrackKind::Video {
                                x.leaf("effectcategory", "Dissolve");
                                x.leaf("effecttype", "transition");
                                x.leaf("mediatype", "video");
                                x.leaf("wipecode", 0);
                                x.leaf("wipeaccuracy", 100);
                                x.leaf("startratio", 0);
                                x.leaf("endratio", 1);
                                x.flag("reverse", false);
                            } else {
                                x.leaf("effecttype", "transition");
                                x.leaf("mediatype", "audio");
                            }
                            x.close("effect");
                            x.close("transitionitem");
                        }
                    }
                }
                x.flag("enabled", t.enabled);
                x.flag("locked", false);
                if sound {
                    x.leaf("outputchannelindex", t.side);
                }
                x.close("track");
            }
            x.close(if sound { "audio" } else { "video" });
        }
        x.close("media");
        let markers = p.markers.iter().map(|m| (&m.name, m.time, Tick::ZERO));
        let regions = p
            .regions
            .iter()
            .map(|r| (&r.name, r.range.start, r.range.duration));
        for (name, at, length) in markers.chain(regions) {
            x.open("marker");
            x.leaf("name", name);
            x.leaf("comment", "");
            x.leaf("in", self.f(at));
            x.leaf(
                "out",
                if length > Tick::ZERO {
                    self.f(at + length)
                } else {
                    -1
                },
            );
            x.close("marker");
        }
        x.close("sequence");
        x.close("xmeml");
        x.s
    }
}

/// Final Cut Pro 7's name of a pixel shape, and whether the frame is
/// anamorphic (wide standard definition).
fn pixel_aspect(ratio: f32) -> (&'static str, bool) {
    let near = |r: f32| (ratio - r).abs() < 0.01;
    if near(1.0) {
        ("square", false)
    } else if near(0.9091) || near(0.9) {
        ("NTSC-601", false)
    } else if near(1.2121) || near(1.2) {
        ("NTSC-601", true)
    } else if near(1.0940) || near(1.0667) {
        ("PAL-601", false)
    } else if near(1.4587) || near(1.4222) {
        ("PAL-601", true)
    } else if near(1.3333) {
        ("HD-(1440x1080)", false)
    } else if near(1.5) {
        ("HD-(960x720)", false)
    } else {
        ("square", false)
    }
}

/// The project as Final Cut Pro 7 XML (`xmeml` version 5), its sequence
/// named `name`; also how many events it says only in part (effects,
/// Pan/Crop, frozen or reversed frames, other transitions) or not at all
/// (generated media but titles and colors, nested projects).
pub fn export(p: &Project, name: &str) -> (String, usize) {
    let mut w = Writer {
        p,
        rate: p.settings.frame_rate,
        items: Vec::new(),
        tracks: Vec::new(),
        of_event: HashMap::new(),
        files: HashMap::new(),
        partial: HashSet::new(),
    };
    w.plan();
    (w.write(name), w.partial.len())
}
