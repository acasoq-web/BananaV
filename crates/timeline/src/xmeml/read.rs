//! A Final Cut Pro 7 XML document read into a timeline document.

use std::collections::{HashMap, HashSet};

use bv_core::{FrameRate, Tick};

use super::rate_of;
use crate::fcp_import::{first_named, scaled, title};
use crate::fx::{Direction, FadeCurve, Transition};
use crate::model::{Generator, TrackKind};
use crate::otio::{
    DocChannels, DocEvent, DocMarker, DocTrack, Document, MediaRef, MediaRefSource, url_path,
};
use crate::xml::{self, Element};

/// How deep sequences nest in one another, at most.
const DEEPEST: usize = 8;

/// A number of child `name` (written whole or not).
fn number(e: &Element, name: &str) -> Option<i64> {
    let v: f64 = e.child_text(name)?.trim().parse().ok()?;
    v.is_finite().then(|| v.round() as i64)
}

/// An effect's parameter `id`.
fn parameter<'a>(effect: &'a Element, id: &str) -> Option<&'a Element> {
    effect
        .children_named("parameter")
        .find(|p| p.child_text("parameterid").as_deref() == Some(id))
}

/// An effect's id and name together, in lower case: what it is.
fn effect_of(effect: &Element) -> String {
    let id = effect.child_text("effectid").unwrap_or_default();
    let name = effect.child_text("name").unwrap_or_default();
    format!("{id} {name}").to_lowercase()
}

/// A clip item's Time Remap effect: its constant speed (1 for 100 %), and
/// whether it was more than BananaV keeps (a speed that changes or
/// reverses). `None` without one.
fn speed_of(item: &Element) -> Option<(f64, bool)> {
    let effect = item
        .children_named("filter")
        .filter_map(|f| f.child("effect"))
        .find(|e| e.child_text("effectid").as_deref() == Some("timeremap"))?;
    let speed = parameter(effect, "speed")?;
    let value: f64 = speed.child_text("value")?.trim().parse().ok()?;
    let reverse = parameter(effect, "reverse")
        .and_then(|p| p.child_text("value"))
        .is_some_and(|v| v.eq_ignore_ascii_case("true"));
    let varies = speed.child("keyframe").is_some()
        || parameter(effect, "graphdict").is_some_and(|g| g.children_named("keyframe").count() > 2);
    Some((
        (value / 100.0).abs().max(0.01),
        reverse || value < 0.0 || varies,
    ))
}

/// A generator item's picture: a title's text (Text, Outline Text, Lower
/// 3rd, Scrolling Text…: its `str` parameters), a Color's color, a Slug's
/// black; `None` for others.
fn generated(item: &Element) -> Option<MediaRefSource> {
    let effect = item.child("effect")?;
    let id = effect
        .child_text("effectid")
        .or_else(|| effect.child_text("name"))
        .unwrap_or_default();
    let solid = |rgba: [f32; 4]| Some(MediaRefSource::Generated(Generator::SolidColor { rgba }));
    match id.as_str() {
        "Slug" => solid([0.0, 0.0, 0.0, 1.0]),
        "Color" => {
            let value = parameter(effect, "color")?.child("value")?;
            let c = |name: &str, default: f32| {
                value
                    .child_text(name)
                    .and_then(|t| t.trim().parse::<f32>().ok())
                    .unwrap_or(default)
                    / 255.0
            };
            solid([
                c("red", 0.0),
                c("green", 0.0),
                c("blue", 0.0),
                c("alpha", 255.0),
            ])
        }
        _ => {
            let lines: Vec<String> = ["str", "str1", "str2"]
                .iter()
                .filter_map(|id| parameter(effect, id)?.child_text("value"))
                .filter(|t| !t.trim().is_empty())
                .collect();
            (!lines.is_empty()).then(|| title(&lines.join("\n")))
        }
    }
}

/// Where a transition stands on its cut (its `alignment`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Align {
    Center,
    Start,
    End,
    /// From black: the clip after it fades in.
    StartBlack,
    /// To black: the clip before it fades out.
    EndBlack,
}

impl Align {
    fn black(self) -> bool {
        matches!(self, Align::StartBlack | Align::EndBlack)
    }
}

/// A transition item: from and to the sequence's frames, what it is.
#[derive(Clone, Debug)]
struct Trans {
    start: i64,
    end: i64,
    align: Align,
    /// Its effect (`effect_of`).
    effect: String,
}

impl Trans {
    fn read(e: &Element) -> Option<Trans> {
        let (start, end) = (number(e, "start")?, number(e, "end")?);
        let align = match e
            .child_text("alignment")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "start" => Align::Start,
            "end" => Align::End,
            "start-black" => Align::StartBlack,
            "end-black" => Align::EndBlack,
            _ => Align::Center,
        };
        let effect = e.child("effect").map(effect_of).unwrap_or_default();
        (end > start).then_some(Trans {
            start,
            end,
            align,
            effect,
        })
    }

    /// Whether a clip edge written at `at` (sequence frames) is its own:
    /// within it, its ends included.
    fn touches(&self, at: i64) -> bool {
        at >= self.start - 1 && at <= self.end + 1
    }

    /// Its cut: where it passes from the clip before it to the one after.
    fn cut(&self) -> i64 {
        match self.align {
            Align::Start | Align::StartBlack => self.start,
            Align::End | Align::EndBlack => self.end,
            Align::Center => self.start + (self.end - self.start) / 2,
        }
    }

    /// The transition BananaV draws for it, between pictures.
    fn picture(&self) -> Transition {
        let e = &self.effect;
        if ["dip", "fade in fade out", "black", "white"]
            .iter()
            .any(|w| e.contains(w))
        {
            Transition::FadeThroughBlack
        } else if e.contains("iris") {
            Transition::Iris
        } else if e.contains("clock") {
            Transition::Clock
        } else if e.contains("zoom") {
            Transition::Zoom
        } else if e.contains("push") {
            Transition::Push(Direction::Left)
        } else if e.contains("slide") {
            Transition::Slide(Direction::Left)
        } else if e.contains("wipe") {
            Transition::Wipe(Direction::Left)
        } else {
            Transition::Dissolve
        }
    }

    /// Between sounds: the curve of its crossfade or fade (Final Cut Pro
    /// 7's 0 dB, of constant gain, and +3 dB, of constant power;
    /// Premiere's Constant Gain and Constant Power).
    fn curve(&self) -> Option<FadeCurve> {
        let e = &self.effect;
        if e.contains("0db") || e.contains("constant gain") {
            Some(FadeCurve::Linear)
        } else if e.contains("3db") || e.contains("constant power") {
            Some(FadeCurve::Smooth)
        } else {
            None
        }
    }
}

/// An opacity's or a gain's keyframes as far as BananaV keeps them: the
/// level, and fades at the clip's ends (its media's frames).
#[derive(Clone, Copy, Debug, Default)]
struct Level {
    value: Option<f32>,
    fade_in: i64,
    fade_out: i64,
}

/// The level of parameter `param` (`unit`: its whole, 100 for opacity's
/// percent; `span`: the clip's media frames): its value, or its keyframes'
/// — a rise from nothing at the clip's start and a fall to nothing at its
/// end are fades.
fn level(param: &Element, unit: f32, span: Option<(i64, i64)>) -> Level {
    let value = |e: &Element| {
        let v: f32 = e.child_text("value")?.trim().parse().ok()?;
        v.is_finite().then_some(v / unit)
    };
    let mut keys: Vec<(i64, f32)> = param
        .children_named("keyframe")
        .filter_map(|k| Some((number(k, "when")?, value(k)?)))
        .collect();
    keys.sort_by_key(|k| k.0);
    if keys.len() < 2 {
        return Level {
            value: keys.first().map(|k| k.1).or_else(|| value(param)),
            ..Level::default()
        };
    }
    let peak = keys.iter().map(|k| k.1).fold(0.0, f32::max);
    let low = |v: f32| v <= peak * 0.02;
    let high = |v: f32| v >= peak * 0.98;
    // `when` counts the clip's media frames (its `in` first), or from the
    // clip's start.
    let (first, last) = (keys[0], keys[keys.len() - 1]);
    let (from, to) = match span {
        Some((a, b)) if first.0 >= a - 1 && last.0 <= b + 1 => (a, b),
        Some((a, b)) => (0, b - a),
        None => (first.0, last.0),
    };
    let mut l = Level {
        value: Some(peak),
        ..Level::default()
    };
    if low(first.1) && first.0 <= from + 1 && high(keys[1].1) {
        l.fade_in = keys[1].0 - first.0;
    }
    let before = keys[keys.len() - 2];
    if low(last.1) && last.0 >= to - 1 && high(before.1) {
        l.fade_out = last.0 - before.0;
    }
    if l.fade_in == 0 && l.fade_out == 0 {
        // Some other change of level: the one it starts with.
        l.value = Some(first.1);
    }
    l
}

/// What a clip item shows.
#[derive(Clone, Copy, Debug)]
enum Shows<'a> {
    /// A media of the document: a file, or a generator's picture.
    Media(usize),
    /// A sequence nested in it.
    Nest(&'a Element),
}

impl Shows<'_> {
    fn same(&self, other: &Shows<'_>) -> bool {
        match (self, other) {
            (Shows::Media(a), Shows::Media(b)) => a == b,
            (Shows::Nest(a), Shows::Nest(b)) => std::ptr::eq(*a, *b),
            _ => false,
        }
    }
}

/// A clip item or a generator item of a track, as the document writes it.
struct Item<'a> {
    id: String,
    shows: Shows<'a>,
    /// The sequence's frames; −1 where a transition places it.
    start: i64,
    end: i64,
    /// Its media's frames at `rate` (`None`: a generator without them).
    span: Option<(i64, i64)>,
    rate: FrameRate,
    speed: f64,
    enabled: bool,
    level: Level,
    pan: Option<f32>,
    /// Audio: the file's channel it plays (1-based, `sourcetrack`'s
    /// `trackindex`).
    channel: Option<u16>,
    links: Vec<String>,
    /// Its media's markers it shows: media time, length, name.
    markers: Vec<(Tick, Tick, String)>,
}

/// What a track lists.
enum Entry<'a> {
    Clip(Box<Item<'a>>),
    Trans(Trans),
    /// Something BananaV leaves out (another generator, a multiclip).
    Other,
}

/// A clip item placed: its event and what pairs and links it to others.
struct Placed<'a> {
    id: String,
    shows: Shows<'a>,
    event: DocEvent,
    channel: Option<u16>,
    pan: Option<f32>,
    links: Vec<String>,
}

/// A track of the document: its name, whether it plays, its clips placed.
struct Track<'a> {
    name: String,
    enabled: bool,
    placed: Vec<Placed<'a>>,
}

/// The markers listed in `e`: their times at `rate` (`in`, and `out`
/// for a length), names.
fn markers(e: &Element, rate: FrameRate) -> Vec<(Tick, Tick, String)> {
    e.children_named("marker")
        .filter_map(|m| {
            let a = number(m, "in")?;
            let b = number(m, "out").unwrap_or(-1);
            let name = m
                .child_text("name")
                .filter(|n| !n.is_empty())
                .or_else(|| m.child_text("comment"))
                .unwrap_or_default();
            let length = if b > a {
                rate.frame_start(b) - rate.frame_start(a)
            } else {
                Tick::ZERO
            };
            Some((rate.frame_start(a.max(0)), length, name))
        })
        .collect()
}

/// Whether VEGAS wrote the sequence: its clip items are named after their
/// master clips and VEGAS's numbers of the events (a video event's from
/// 0x30000000, an audio one's from 0x20000000; a right channel's with a 1
/// after it).
fn by_vegas(seq: &Element) -> bool {
    let Some(media) = seq.child("media") else {
        return false;
    };
    let mut any = false;
    for item in media
        .elements()
        .flat_map(|kind| kind.children_named("track"))
        .flat_map(|t| t.children_named("clipitem"))
    {
        let (Some(id), Some(master)) = (item.attr("id"), item.child_text("masterclipid")) else {
            return false;
        };
        let Some(n) = id.strip_prefix(master.as_str()) else {
            return false;
        };
        if n.is_empty() || n.len() > 18 || !n.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        if n.parse::<u64>().unwrap_or(0) < 0x2000_0000 {
            return false;
        }
        any = true;
    }
    any
}

/// The sequence a document is about: the first at its top or in its
/// project's and bins' lists (not one nested in a clip), else the first.
fn main_sequence(root: &Element) -> Option<&Element> {
    fn listed(e: &Element) -> Option<&Element> {
        e.elements().find_map(|c| match c.name.as_str() {
            "sequence" if c.child("media").is_some() => Some(c),
            "project" | "children" | "bin" => listed(c),
            _ => None,
        })
    }
    listed(root).or_else(|| first_named(root, "sequence"))
}

/// The files and sequences of a document described somewhere in it, by
/// their ids: the first description of each.
fn index<'a>(
    e: &'a Element,
    files: &mut HashMap<&'a str, &'a Element>,
    sequences: &mut HashMap<&'a str, &'a Element>,
) {
    match e.name.as_str() {
        "file" if e.elements().next().is_some() => {
            if let Some(id) = e.attr("id") {
                files.entry(id).or_insert(e);
            }
        }
        "sequence" if e.child("media").is_some() => {
            if let Some(id) = e.attr("id") {
                sequences.entry(id).or_insert(e);
            }
        }
        _ => {}
    }
    for c in e.elements() {
        index(c, files, sequences);
    }
}

struct Reader<'a> {
    files: HashMap<&'a str, &'a Element>,
    sequences: HashMap<&'a str, &'a Element>,
    media: Vec<MediaRef>,
    /// Media by file (its id, else its path or name).
    by_file: HashMap<String, usize>,
    skipped: usize,
    depth: usize,
    vegas: bool,
}

impl<'a> Reader<'a> {
    /// The media of a clip item's `file`: described here or where it
    /// first appears in the document. A file without a path is looked
    /// for by its name (beside the document).
    fn file(&mut self, file: &'a Element, rate: FrameRate) -> Option<usize> {
        let id = file.attr("id").filter(|i| !i.is_empty());
        let described = id.and_then(|i| self.files.get(i).copied()).unwrap_or(file);
        let path = described
            .child_text("pathurl")
            .and_then(|u| url_path(&u))
            .filter(|p| !p.is_empty());
        let name = described.child_text("name").filter(|n| !n.is_empty());
        let key = match (id, &path, &name) {
            (Some(i), ..) => format!("id {i}"),
            (None, Some(p), _) => format!("path {p}"),
            (None, None, Some(n)) => format!("name {n}"),
            _ => return None,
        };
        if let Some(&i) = self.by_file.get(&key) {
            return Some(i);
        }
        let source = path.clone().or_else(|| name.clone())?;
        let own = described.child("rate").and_then(rate_of).unwrap_or(rate);
        let available = number(described, "duration")
            .filter(|d| *d > 0)
            .map(|d| own.frame_start(d));
        let shown = name.unwrap_or_else(|| {
            source
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(&source)
                .to_string()
        });
        self.media.push(MediaRef {
            name: shown,
            source: MediaRefSource::File(source),
            saved: None,
            available,
        });
        self.by_file.insert(key, self.media.len() - 1);
        Some(self.media.len() - 1)
    }

    /// A track's clip item or generator item (`rate`: the sequence's).
    fn item(&mut self, e: &'a Element, kind: TrackKind, rate: FrameRate) -> Entry<'a> {
        let own = e.child("rate").and_then(rate_of).unwrap_or(rate);
        let shows = if e.name == "generatoritem" {
            match generated(e) {
                Some(source) if kind == TrackKind::Video => {
                    self.media.push(MediaRef {
                        name: e.child_text("name").unwrap_or_default(),
                        source,
                        saved: None,
                        available: None,
                    });
                    Shows::Media(self.media.len() - 1)
                }
                _ => return Entry::Other,
            }
        } else if let Some(s) = e.child("sequence") {
            let nested = match s.attr("id").and_then(|i| self.sequences.get(i)) {
                Some(&n) if s.child("media").is_none() => n,
                _ => s,
            };
            Shows::Nest(nested)
        } else if let Some(file) = e.child("file") {
            match self.file(file, own) {
                Some(m) => Shows::Media(m),
                None => return Entry::Other,
            }
        } else {
            return Entry::Other;
        };
        let (speed, more) = speed_of(e).unwrap_or((1.0, false));
        self.skipped += usize::from(more);
        let span = match (number(e, "in"), number(e, "out")) {
            (Some(a), Some(b)) if a >= 0 && b > a => Some((a, b)),
            _ => None,
        };
        let mut item = Item {
            id: e.attr("id").unwrap_or_default().to_string(),
            shows,
            start: number(e, "start").unwrap_or(-1),
            end: number(e, "end").unwrap_or(-1),
            span,
            rate: own,
            speed,
            enabled: e
                .child_text("enabled")
                .is_none_or(|t| !t.eq_ignore_ascii_case("false")),
            level: Level::default(),
            pan: None,
            channel: e
                .find("sourcetrack/trackindex")
                .and_then(|t| t.text().parse().ok())
                .filter(|&c: &u16| c > 0),
            links: e
                .children_named("link")
                .filter_map(|l| l.child_text("linkclipref"))
                .collect(),
            markers: markers(e, own),
        };
        // Its opacity or gain and its pan; other effects stay behind.
        let mut others = false;
        for effect in e.children_named("filter").filter_map(|f| f.child("effect")) {
            let id = effect
                .child_text("effectid")
                .unwrap_or_default()
                .to_lowercase();
            match id.as_str() {
                "opacity" if kind == TrackKind::Video => {
                    if let Some(p) = parameter(effect, "opacity") {
                        item.level = level(p, 100.0, span);
                    }
                }
                "audiolevels" if kind == TrackKind::Audio => {
                    if let Some(p) = parameter(effect, "level") {
                        item.level = level(p, 1.0, span);
                    }
                }
                "audiopan" if kind == TrackKind::Audio => {
                    item.pan = parameter(effect, "pan")
                        .and_then(|p| p.child_text("value"))
                        .and_then(|v| v.trim().parse().ok());
                }
                "timeremap" => {}
                _ => others = true,
            }
        }
        self.skipped += usize::from(others);
        Entry::Clip(Box::new(item))
    }

    /// Track `t`'s items placed in time (`rate`: the sequence's): next to a
    /// transition each reaches over it — a crossfade with the clip on its
    /// other side, else a fade from or to black.
    fn track(&mut self, t: &'a Element, kind: TrackKind, rate: FrameRate) -> Vec<Placed<'a>> {
        let entries: Vec<Entry<'_>> = t
            .elements()
            .filter_map(|e| match e.name.as_str() {
                "clipitem" | "generatoritem" => Some(self.item(e, kind, rate)),
                "transitionitem" => Some(Trans::read(e).map_or(Entry::Other, Entry::Trans)),
                _ => None,
            })
            .collect();
        self.skipped += entries.iter().filter(|e| matches!(e, Entry::Other)).count();
        let at = |n: i64| rate.frame_start(n);
        let half = Tick(rate.frame_duration().0 / 2);
        let mut out: Vec<Placed<'_>> = Vec::new();
        // The event placed for each entry (to give the one before a
        // crossfade its curve).
        let mut placed: Vec<Option<usize>> = Vec::with_capacity(entries.len());
        let mut last_end = None;
        let trans = |i: Option<usize>| match i.and_then(|i| entries.get(i)) {
            Some(Entry::Trans(t)) => Some(t),
            _ => None,
        };
        for (i, entry) in entries.iter().enumerate() {
            placed.push(None);
            let Entry::Clip(c) = entry else {
                continue;
            };
            let head = trans(i.checked_sub(1)).filter(|h| c.start < 0 || h.touches(c.start));
            let tail = trans(Some(i + 1)).filter(|t| c.end < 0 || t.touches(c.end));
            // A clip on a transition's other side, placed: a crossfade.
            let joined_before = head.is_some_and(|h| {
                !h.align.black() && i >= 2 && placed.get(i - 2).copied().flatten().is_some()
            });
            let joined_after = tail.is_some_and(|t| {
                !t.align.black() && matches!(entries.get(i + 2), Some(Entry::Clip(_)))
            });
            let (span, mut source) = match c.span {
                Some((a, b)) => (
                    c.rate.frame_start(b) - c.rate.frame_start(a),
                    c.rate.frame_start(a),
                ),
                None if c.start >= 0 && c.end > c.start => (at(c.end) - at(c.start), Tick::ZERO),
                None => {
                    self.skipped += 1;
                    continue;
                }
            };
            let len = scaled(span, 1.0 / c.speed);
            let mut start = if c.start >= 0 {
                at(c.start)
            } else if let Some(h) = head {
                if h.align == Align::EndBlack {
                    at(h.end)
                } else {
                    // Over the transition from its start, its `in` its
                    // media's frame there — or, written at the cut, that
                    // much later in it.
                    let from_end = (c.end >= 0).then(|| at(c.end) - len);
                    if let Some(f) = from_end
                        && f > at(h.start) + half
                        && (f - at(h.cut())).abs() <= half
                    {
                        source -= scaled(f - at(h.start), c.speed);
                    }
                    at(h.start)
                }
            } else if c.end >= 0 {
                at(c.end) - len
            } else if let Some(e) = last_end {
                e
            } else {
                self.skipped += 1;
                continue;
            };
            // Written at the cut: it reaches back over the transition.
            if c.start >= 0
                && let Some(h) = head.filter(|h| h.align != Align::EndBlack)
                && at(h.start) < start
            {
                source -= scaled(start - at(h.start), c.speed);
                start = at(h.start);
            }
            let mut end = if c.end >= 0 { at(c.end) } else { start + len };
            if let Some(t) = tail {
                if t.align == Align::StartBlack {
                    // The next clip fades in: this one stops where it starts.
                    if c.end < 0 {
                        end = at(t.start);
                    }
                } else {
                    end = end.max(at(t.end));
                }
            }
            if end <= start {
                self.skipped += 1;
                continue;
            }
            let Shows::Media(media) = c.shows else {
                // A nested sequence's events come in its place later.
                out.push(Placed {
                    id: c.id.clone(),
                    shows: c.shows,
                    event: DocEvent::new(0, start, end - start, source, c.enabled),
                    channel: c.channel,
                    pan: c.pan,
                    links: c.links.clone(),
                });
                placed[i] = Some(out.len() - 1);
                last_end = Some(end);
                continue;
            };
            let mut e = DocEvent::new(media, start, end - start, source, c.enabled);
            e.speed = c.speed;
            e.level = c.level.value;
            e.markers = c.markers.clone();
            let sound = kind == TrackKind::Audio;
            if let Some(h) = head {
                if joined_before {
                    if sound {
                        e.curves[0] = h.curve();
                        // The event before fades out the same way.
                        if let Some(b) = placed[i - 2] {
                            out[b].event.curves[1] = h.curve();
                        }
                    } else {
                        e.transition = Some(h.picture());
                    }
                } else if h.align != Align::EndBlack {
                    e.fade_in = at(h.end) - at(h.start);
                    if sound {
                        e.curves[0] = h.curve();
                    }
                }
            }
            if let Some(t) = tail
                && !joined_after
                && t.align != Align::StartBlack
            {
                e.fade_out = at(t.end) - at(t.start);
                if sound {
                    e.curves[1] = t.curve();
                }
            }
            // Fades its opacity's or gain's keyframes make.
            let frames = |n: i64| scaled(c.rate.frame_start(n), 1.0 / c.speed);
            if e.fade_in == Tick::ZERO && c.level.fade_in > 0 {
                e.fade_in = frames(c.level.fade_in);
            }
            if e.fade_out == Tick::ZERO && c.level.fade_out > 0 {
                e.fade_out = frames(c.level.fade_out);
            }
            out.push(Placed {
                id: c.id.clone(),
                shows: c.shows,
                event: e,
                channel: c.channel,
                pan: c.pan,
                links: c.links.clone(),
            });
            placed[i] = Some(out.len() - 1);
            last_end = Some(end);
        }
        out
    }

    /// A sequence's tracks of `kind` (`top`: the document's own, whose
    /// links are kept): video from the bottom up, audio from the top down,
    /// stereo pairs one event each.
    fn tracks(
        &mut self,
        seq: &'a Element,
        kind: TrackKind,
        merged: &mut HashMap<String, String>,
    ) -> Vec<Track<'a>> {
        let rate = seq
            .child("rate")
            .and_then(rate_of)
            .unwrap_or(FrameRate::FPS_30);
        let part = match kind {
            TrackKind::Video => "video",
            TrackKind::Audio => "audio",
        };
        let Some(part) = seq.child("media").and_then(|m| m.child(part)) else {
            return Vec::new();
        };
        let mut tracks: Vec<Track<'_>> = part
            .children_named("track")
            .map(|t| Track {
                name: t.attr("MZ.TrackName").unwrap_or_default().to_string(),
                enabled: t
                    .child_text("enabled")
                    .is_none_or(|e| !e.eq_ignore_ascii_case("false")),
                placed: self.track(t, kind, rate),
            })
            .collect();
        if kind == TrackKind::Audio {
            pair(&mut tracks, half_frame(rate), merged);
            // VEGAS lists its audio tracks from the bottom up.
            if self.vegas {
                tracks.reverse();
            }
        }
        tracks
    }

    /// A nested sequence's tracks of `kind` laid out where clip `p` shows
    /// it: their events from its media time on, cut to its length.
    fn nest(&mut self, seq: &'a Element, kind: TrackKind, p: &DocEvent) -> Vec<Vec<DocEvent>> {
        if self.depth >= DEEPEST {
            self.skipped += 1;
            return Vec::new();
        }
        self.depth += 1;
        let mut merged = HashMap::new();
        let tracks = self.tracks(seq, kind, &mut merged);
        let lanes: Vec<Vec<DocEvent>> = tracks
            .into_iter()
            .map(|t| self.events(t.placed, kind, &HashMap::new()))
            .collect();
        self.depth -= 1;
        let (from, to) = (p.source, p.source + p.duration);
        lanes
            .into_iter()
            .map(|events| {
                events
                    .into_iter()
                    .filter_map(|mut e| {
                        let (a, b) = (e.start.max(from), (e.start + e.duration).min(to));
                        if b <= a {
                            return None;
                        }
                        if !e.frozen {
                            e.source += scaled(a - e.start, e.speed);
                        }
                        if a > e.start {
                            e.transition = None;
                            e.fade_in = Tick::ZERO;
                        }
                        if b < e.start + e.duration {
                            e.fade_out = Tick::ZERO;
                        }
                        e.start = p.start + (a - from);
                        e.duration = b - a;
                        e.enabled &= p.enabled;
                        e.link = None;
                        Some(e)
                    })
                    .collect()
            })
            .collect()
    }

    /// A track's placed clips as events — a nested sequence's in its place,
    /// lanes of their own past its first track — with their links' groups
    /// (`groups`: each clip's).
    fn events(
        &mut self,
        placed: Vec<Placed<'a>>,
        kind: TrackKind,
        groups: &HashMap<String, usize>,
    ) -> Vec<DocEvent> {
        self.lanes(placed, kind, groups)
            .into_iter()
            .flatten()
            .collect()
    }

    /// A track's placed clips in lanes: its own first, then a nested
    /// sequence's other tracks.
    fn lanes(
        &mut self,
        placed: Vec<Placed<'a>>,
        kind: TrackKind,
        groups: &HashMap<String, usize>,
    ) -> Vec<Vec<DocEvent>> {
        let mut lanes: Vec<Vec<DocEvent>> = vec![Vec::new()];
        for p in placed {
            match p.shows {
                Shows::Media(_) => {
                    let mut e = p.event;
                    e.link = groups.get(&p.id).copied();
                    lanes[0].push(e);
                }
                Shows::Nest(seq) => {
                    if p.event.speed != 1.0 {
                        self.skipped += 1;
                    }
                    for (n, events) in self.nest(seq, kind, &p.event).into_iter().enumerate() {
                        if lanes.len() <= n {
                            lanes.resize_with(n + 1, Vec::new);
                        }
                        lanes[n].extend(events);
                    }
                }
            }
        }
        for lane in &mut lanes {
            lane.sort_by_key(|e| e.start);
        }
        lanes
    }
}

fn half_frame(rate: FrameRate) -> Tick {
    Tick(rate.frame_duration().0 / 2)
}

/// Stereo pairs: a clip and one on the next track showing the same at the
/// same place, linked to it or to the same clip (or neither linked to
/// anything), are the two channels of one event, which the first keeps (`merged`: the second's
/// id to the first's). Each sound event gets the channels it plays.
fn pair(tracks: &mut [Track<'_>], near: Tick, merged: &mut HashMap<String, String>) {
    let close = |a: Tick, b: Tick| (a - b).0.abs() <= near.0;
    let mut gone: Vec<HashSet<usize>> = vec![HashSet::new(); tracks.len()];
    for t in 0..tracks.len().saturating_sub(1) {
        for i in 0..tracks[t].placed.len() {
            if gone[t].contains(&i) {
                continue;
            }
            let x = &tracks[t].placed[i];
            let mate = tracks[t + 1].placed.iter().enumerate().find(|(j, y)| {
                !gone[t + 1].contains(j)
                    && x.shows.same(&y.shows)
                    && close(x.event.start, y.event.start)
                    && close(x.event.duration, y.event.duration)
                    && close(x.event.source, y.event.source)
                    && (x.links.contains(&y.id)
                        || y.links.contains(&x.id)
                        || x.links.iter().any(|l| y.links.contains(l))
                        || (x.links.is_empty() && y.links.is_empty()))
            });
            let Some((j, y)) = mate else {
                continue;
            };
            let (id, channel, pan) = (y.id.clone(), y.channel, y.pan);
            gone[t + 1].insert(j);
            merged.insert(id, x.id.clone());
            let x = &mut tracks[t].placed[i];
            let left = x.channel.unwrap_or(1);
            let middle = |p: Option<f32>| p.is_some_and(|p| p.abs() < 0.01);
            x.event.channels = Some(DocChannels {
                left,
                right: channel.unwrap_or(left + 1),
                centered: middle(x.pan) && middle(pan),
            });
        }
    }
    for (t, track) in tracks.iter_mut().enumerate() {
        let mut i = 0;
        track.placed.retain(|_| {
            i += 1;
            !gone[t].contains(&(i - 1))
        });
        // A clip alone: its channel in the middle.
        for p in &mut track.placed {
            if p.event.channels.is_none()
                && let Some(c) = p.channel
            {
                p.event.channels = Some(DocChannels {
                    left: c,
                    right: c,
                    centered: true,
                });
            }
        }
    }
}

/// The groups the clips' links make (each clip's group, by its id): only
/// those of more than one event.
fn link_groups(tracks: &[&Track<'_>], merged: &HashMap<String, String>) -> HashMap<String, usize> {
    let ids: Vec<&str> = tracks
        .iter()
        .flat_map(|t| &t.placed)
        .map(|p| p.id.as_str())
        .filter(|id| !id.is_empty())
        .collect();
    let index: HashMap<&str, usize> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut parent: Vec<usize> = (0..ids.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let find = |id: &str| {
        let id = merged.get(id).map_or(id, String::as_str);
        index.get(id).copied()
    };
    for p in tracks.iter().flat_map(|t| &t.placed) {
        let Some(a) = find(&p.id) else {
            continue;
        };
        for link in &p.links {
            if let Some(b) = find(link) {
                let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                parent[ra] = rb;
            }
        }
    }
    let roots: Vec<usize> = (0..ids.len()).map(|i| root(&mut parent, i)).collect();
    let mut size: HashMap<usize, usize> = HashMap::new();
    for &r in &roots {
        *size.entry(r).or_default() += 1;
    }
    ids.iter()
        .zip(&roots)
        .filter(|(_, r)| size[r] > 1)
        .map(|(&id, &r)| (id.to_string(), r))
        .collect()
}

/// Reads a Final Cut Pro 7 XML (`xmeml`) document's sequence: the one at
/// its top or in its project's bins.
pub fn read(text: &str) -> Result<Document, String> {
    let root = xml::parse(text)?;
    if root.name != "xmeml" {
        return Err("not a Final Cut Pro 7 XML file".into());
    }
    let seq = main_sequence(&root).ok_or("the file has no sequence")?;
    let (mut files, mut sequences) = (HashMap::new(), HashMap::new());
    index(&root, &mut files, &mut sequences);
    let mut r = Reader {
        files,
        sequences,
        media: Vec::new(),
        by_file: HashMap::new(),
        skipped: 0,
        depth: 0,
        vegas: by_vegas(seq),
    };
    let rate = seq.child("rate").and_then(rate_of);
    let mut merged = HashMap::new();
    let video = r.tracks(seq, TrackKind::Video, &mut merged);
    let audio = r.tracks(seq, TrackKind::Audio, &mut merged);
    let groups = link_groups(&video.iter().chain(&audio).collect::<Vec<_>>(), &merged);
    let mut tracks = Vec::new();
    for (kind, list) in [(TrackKind::Video, video), (TrackKind::Audio, audio)] {
        for t in list {
            let lanes = r.lanes(t.placed, kind, &groups);
            for (n, events) in lanes.into_iter().enumerate() {
                if events.is_empty() {
                    continue;
                }
                let name = match n {
                    0 => t.name.clone(),
                    n if !t.name.is_empty() => format!("{} ({})", t.name, n + 1),
                    _ => String::new(),
                };
                tracks.push(DocTrack {
                    kind,
                    name,
                    enabled: t.enabled,
                    events,
                    effects: Vec::new(),
                    key: None,
                    saved: None,
                });
            }
        }
    }
    let fr = rate.unwrap_or(FrameRate::FPS_30);
    let markers = markers(seq, fr)
        .into_iter()
        .map(|(start, duration, name)| DocMarker {
            name,
            start,
            duration,
        })
        .collect();
    let format = seq.find("media/video/format/samplecharacteristics");
    let frame = format.and_then(|f| {
        let (w, h) = (number(f, "width")?, number(f, "height")?);
        (w > 0 && h > 0).then_some((w as u32, h as u32))
    });
    let sample_rate = seq
        .find("media/audio/format/samplecharacteristics/samplerate")
        .and_then(|s| s.text().parse::<f64>().ok())
        .map(|s| s.round() as u32)
        .filter(|&s| s > 0);
    Ok(Document {
        name: seq.child_text("name").unwrap_or_default(),
        media: r.media,
        skipped: r.skipped,
        rate,
        start: Tick::ZERO,
        tracks,
        markers,
        output_fx: None,
        saved: None,
        frame,
        sample_rate,
    })
}
