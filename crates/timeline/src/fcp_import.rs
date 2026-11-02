//! FCPXML (Final Cut Pro X's) read into a timeline document — the same
//! one OpenTimelineIO's reading makes, so [`crate::otio::build`] turns it
//! into a project once its media are found (File › Import's "Final Cut Pro
//! X (*.fcpxml)"). Clips of files come in, at their constant speeds;
//! crossfades as overlaps; titles as BananaV's Titles & Text saying their
//! text; compound clips and multicam clips (their active angles) laid out
//! in their place, its roles naming the tracks. What a clip of BananaV
//! cannot be (other generators, effects, speeds that change or reverse) is
//! counted as left out, for the report. Final Cut Pro 7's XML is
//! [`crate::xmeml`]'s.

use std::collections::{BTreeMap, HashMap};

use bv_core::{FrameRate, TICKS_PER_SECOND, Tick};

use crate::fx::{PluginFx, PluginParam, PluginValue, Transition};
use crate::model::{Generator, TrackKind};
use crate::otio::{DocEvent, DocTrack, Document, MediaRef, MediaRefSource, url_path};
use crate::xml::{self, Element};

/// An event of a file, placed on the timeline.
pub(crate) fn event(
    media: usize,
    start: Tick,
    duration: Tick,
    source: Tick,
    enabled: bool,
) -> DocEvent {
    DocEvent::new(media, start, duration, source, enabled)
}

pub(crate) fn track(kind: TrackKind, events: Vec<DocEvent>) -> DocTrack {
    DocTrack {
        kind,
        name: String::new(),
        enabled: true,
        events,
        effects: Vec::new(),
        key: None,
        saved: None,
    }
}

/// BananaV's Titles & Text plug-in (`bv_effects::TITLES_ID`).
pub const TITLES_PLUGIN: &str = "org.bananav.TitlesText";

/// A title saying `text`: BananaV's Titles & Text, its other settings at
/// their defaults.
pub(crate) fn title(text: &str) -> MediaRefSource {
    let mut params = BTreeMap::new();
    params.insert(
        "text".to_string(),
        PluginParam {
            value: PluginValue::Text(text.trim().to_string()),
            keys: Vec::new(),
        },
    );
    MediaRefSource::Generated(Generator::Plugin(Box::new(PluginFx {
        id: TITLES_PLUGIN.to_string(),
        version: [1, 0],
        name: "Titles & Text".to_string(),
        params,
    })))
}

/// Media time `d` of the timeline at `speed`.
pub(crate) fn scaled(d: Tick, speed: f64) -> Tick {
    if speed == 1.0 {
        d
    } else {
        Tick((d.0 as f64 * speed).round() as i64)
    }
}

/// The first element named `name` in `e` or below it.
pub(crate) fn first_named<'a>(e: &'a Element, name: &str) -> Option<&'a Element> {
    if e.name == name {
        return Some(e);
    }
    e.elements().find_map(|c| first_named(c, name))
}

/// Events overlapping their neighbour on a track: the later one enters
/// through a crossfade.
fn mark_crossfades(events: &mut [DocEvent]) {
    events.sort_by_key(|e| e.start);
    for i in 1..events.len() {
        let before = &events[i - 1];
        if events[i].start < before.start + before.duration {
            events[i].transition = Some(Transition::Dissolve);
        }
    }
}

/// A document of `tracks` (bottom up), its media and what was left out.
fn document(
    name: String,
    rate: Option<FrameRate>,
    media: Vec<MediaRef>,
    tracks: Vec<DocTrack>,
    skipped: usize,
) -> Document {
    Document {
        name,
        media,
        skipped,
        rate,
        start: Tick::ZERO,
        tracks,
        markers: Vec::new(),
        output_fx: None,
        saved: None,
        frame: None,
        sample_rate: None,
    }
}

// ---- FCPXML ---------------------------------------------------------------------

/// An FCPXML time: `"num/dens"`, `"ns"`.
fn fcp_time(text: Option<&str>) -> Tick {
    let Some(t) = text.map(str::trim).and_then(|t| t.strip_suffix('s')) else {
        return Tick::ZERO;
    };
    let (num, den) = match t.split_once('/') {
        Some((n, d)) => (
            n.parse::<i128>().unwrap_or(0),
            d.parse::<i128>().unwrap_or(1),
        ),
        None => {
            // Whole or decimal seconds.
            let secs: f64 = t.parse().unwrap_or(0.0);
            return Tick::from_seconds_f64(secs);
        }
    };
    if den == 0 {
        return Tick::ZERO;
    }
    Tick((num * TICKS_PER_SECOND as i128 / den) as i64)
}

/// A frame duration's rate ("1001/30000s": 30000⁄1001).
fn fcp_rate(frame: Option<&str>) -> Option<FrameRate> {
    let t = frame?.trim().strip_suffix('s')?;
    let (n, d) = t.split_once('/').unwrap_or((t, "1"));
    let (n, d): (u32, u32) = (n.parse().ok()?, d.parse().ok()?);
    if n == 0 || d == 0 {
        return None;
    }
    // In lowest terms ("100/2500s": 25 a second).
    let mut g = (n, d);
    while g.1 != 0 {
        g = (g.1, g.0 % g.1);
    }
    Some(FrameRate::new(d / g.0, n / g.0))
}

/// An asset of the resources: its media and its streams.
struct Asset {
    media: usize,
    video: bool,
    audio: bool,
}

/// What the spine's walk gathers.
struct Walk<'a> {
    assets: &'a HashMap<String, Asset>,
    /// The resources' `media` (compound clips' sequences, multicam
    /// clips' angles), by id.
    resources: &'a HashMap<String, &'a Element>,
    /// The document's media: titles add theirs.
    media: &'a mut Vec<MediaRef>,
    /// Events by lane: video and audio.
    video: BTreeMap<i64, Vec<DocEvent>>,
    audio: BTreeMap<i64, Vec<DocEvent>>,
    /// Each lane's role, video or audio (`None` once its events have
    /// several).
    roles: BTreeMap<(bool, i64), Option<String>>,
    /// Transitions of the primary storyline: where they start, how long.
    transitions: Vec<(Tick, Tick)>,
    skipped: usize,
    /// The streams taken ("all", "video" or "audio": a multicam angle's).
    only: &'a str,
    /// How deep in compound and multicam clips.
    depth: usize,
}

/// A clip's retiming (its `timeMap`): the media time at its local time
/// `start`, its speed, whether it holds a frame, and whether it was more
/// than BananaV keeps (a speed that changes or reverses).
fn retime(item: &Element, start: Tick) -> (Tick, f64, bool, bool) {
    let points: Vec<(Tick, Tick)> = item
        .child("timeMap")
        .map(|m| {
            m.children_named("timept")
                .map(|p| (fcp_time(p.attr("time")), fcp_time(p.attr("value"))))
                .collect()
        })
        .unwrap_or_default();
    let (Some(&(t0, v0)), Some(&(t1, v1))) = (points.first(), points.last()) else {
        return (start, 1.0, false, false);
    };
    let dt = (t1 - t0).0 as f64;
    if points.len() < 2 || dt <= 0.0 {
        return (start, 1.0, false, false);
    }
    let speed = (v1 - v0).0 as f64 / dt;
    let straight = points.windows(2).all(|w| {
        let (a, b) = (w[0], w[1]);
        let want = (b.0 - a.0).0 as f64 * speed;
        ((b.1 - a.1).0 as f64 - want).abs() <= 1e-6 * dt.max(1.0) + 1.0
    });
    let source = v0 + scaled(start - t0, speed);
    if speed.abs() < 1e-9 {
        return (v0, 1.0, true, !straight);
    }
    (
        source.max(Tick::ZERO),
        speed.abs().max(0.01),
        false,
        speed < 0.0 || !straight,
    )
}

/// All the text below `e`, its pieces one after another as they are.
fn all_text(e: &Element) -> String {
    let mut out = String::new();
    for n in &e.children {
        match n {
            xml::Node::Text(t) => out.push_str(t),
            xml::Node::Element(c) => out.push_str(&all_text(c)),
        }
    }
    out
}

impl Walk<'_> {
    /// Puts media `a` on lane `lane` (its streams as `src`, "all",
    /// "video" or "audio", and the walk let), noting its roles.
    fn place(
        &mut self,
        a: (usize, bool, bool),
        src: &str,
        lane: i64,
        e: DocEvent,
        roles: [Option<&str>; 2],
    ) {
        let (_, video, audio) = a;
        if video && src != "audio" && self.only != "audio" {
            self.role(true, lane, roles[0]);
            self.video.entry(lane).or_default().push(e.clone());
        }
        if audio && src != "video" && self.only != "video" {
            self.role(false, lane, roles[1]);
            self.audio.entry(lane).or_default().push(e);
        }
    }

    /// Lane `lane`'s role (its main role: "dialogue" of
    /// "dialogue.dialogue-1"), unless its events have several.
    fn role(&mut self, video: bool, lane: i64, role: Option<&str>) {
        let role = role.map(|r| r.split('.').next().unwrap_or(r).to_string());
        match self.roles.get(&(video, lane)) {
            None => {
                self.roles.insert((video, lane), role);
            }
            Some(r) if *r != role => {
                self.roles.insert((video, lane), None);
            }
            Some(_) => {}
        }
    }

    /// The storyline `items` (a compound clip's spine, a multicam angle)
    /// laid out here: its time `start` at `offset`, cut to `duration`, its
    /// lanes from `lane`, only the streams `only` lets.
    fn nest(&mut self, items: &Element, at: (Tick, Tick, Tick), lane: i64, only: &str) {
        let (offset, start, duration) = at;
        if self.depth >= 8 || only == "none" {
            self.skipped += 1;
            return;
        }
        let only = match (self.only, only) {
            ("all", o) => o,
            (o, "all") => o,
            (a, b) if a == b => a,
            _ => return,
        };
        let only: &str = match only {
            "video" => "video",
            "audio" => "audio",
            _ => "all",
        };
        let mut sub = Walk {
            assets: self.assets,
            resources: self.resources,
            media: &mut *self.media,
            video: BTreeMap::new(),
            audio: BTreeMap::new(),
            roles: BTreeMap::new(),
            transitions: Vec::new(),
            skipped: 0,
            only,
            depth: self.depth + 1,
        };
        for item in items.elements() {
            sub.item(item, &|t| t, false);
        }
        sub.cross_transitions();
        let Walk {
            video,
            audio,
            roles,
            skipped,
            ..
        } = sub;
        self.skipped += skipped;
        let (from, to) = (start, start + duration);
        for (is_video, lanes) in [(true, video), (false, audio)] {
            for (l, events) in lanes {
                for mut e in events {
                    // Cut to the clip's part of the storyline.
                    let (a, b) = (e.start.max(from), (e.start + e.duration).min(to));
                    if b <= a {
                        continue;
                    }
                    if !e.frozen {
                        e.source += scaled(a - e.start, e.speed);
                    }
                    if a > e.start {
                        e.transition = None;
                    }
                    e.start = offset + (a - from);
                    e.duration = b - a;
                    let lanes = if is_video {
                        &mut self.video
                    } else {
                        &mut self.audio
                    };
                    lanes.entry(lane + l).or_default().push(e);
                }
                let role = roles.get(&(is_video, l)).cloned().flatten();
                self.role(is_video, lane + l, role.as_deref());
            }
        }
    }

    /// An item of a storyline (`lane`: its lane when connected, else the
    /// primary storyline's 0), its offset placed by `place`; then what is
    /// connected to it.
    fn item(&mut self, item: &Element, place: &dyn Fn(Tick) -> Tick, connected: bool) {
        let offset = place(fcp_time(item.attr("offset")));
        let start = fcp_time(item.attr("start"));
        let duration = fcp_time(item.attr("duration"));
        let lane = if connected {
            item.attr("lane").and_then(|l| l.parse().ok()).unwrap_or(0)
        } else {
            0
        };
        let enabled = item.attr("enabled") != Some("0");
        let asset = |r: Option<&str>| {
            r.and_then(|r| self.assets.get(r))
                .map(|a| (a.media, a.video, a.audio))
        };
        let roles = [item.attr("role"), item.attr("audioRole")];
        // At its speed (a constant one; a frozen frame held).
        let timed = |media: usize| {
            let (source, speed, frozen, more) = retime(item, start);
            let mut e = event(media, offset, duration, source, enabled);
            e.speed = speed;
            e.frozen = frozen;
            (e, more)
        };
        match item.name.as_str() {
            "asset-clip" => match asset(item.attr("ref")) {
                Some(a) => {
                    let src = item.attr("srcEnable").unwrap_or("all");
                    let (e, more) = timed(a.0);
                    self.skipped += usize::from(more);
                    self.place(a, src, lane, e, roles);
                }
                None => self.skipped += 1,
            },
            "clip" => {
                // A clip wraps its media: its video's or audio's ref.
                let inner = item
                    .elements()
                    .find(|e| matches!(e.name.as_str(), "video" | "audio" | "asset-clip"))
                    .and_then(|e| asset(e.attr("ref")));
                match inner {
                    Some(a) => {
                        let (e, more) = timed(a.0);
                        self.skipped += usize::from(more);
                        self.place(a, "all", lane, e, roles);
                    }
                    None => self.skipped += 1,
                }
            }
            // A title: Titles & Text saying its text.
            "title" => {
                let text = item.child("text").map(all_text).unwrap_or_default();
                if self.only == "audio" || text.trim().is_empty() {
                    self.skipped += 1;
                    return;
                }
                self.media.push(MediaRef {
                    name: item.attr("name").unwrap_or("Title").to_string(),
                    source: title(&text),
                    saved: None,
                    available: None,
                });
                let e = event(self.media.len() - 1, offset, duration, Tick::ZERO, enabled);
                self.role(true, lane, item.attr("role").or(Some("titles")));
                self.video.entry(lane).or_default().push(e);
            }
            // A compound clip: its sequence's storyline in its place.
            "ref-clip" => {
                let spine = item
                    .attr("ref")
                    .and_then(|r| self.resources.get(r))
                    .and_then(|m| m.child("sequence"))
                    .and_then(|q| q.child("spine"));
                match spine {
                    Some(spine) => {
                        let src = item.attr("srcEnable").unwrap_or("all");
                        self.nest(spine, (offset, start, duration), lane, src);
                    }
                    None => self.skipped += 1,
                }
            }
            // A multicam clip: its active angles' storylines, each for the
            // streams it gives.
            "mc-clip" => {
                let multicam = item
                    .attr("ref")
                    .and_then(|r| self.resources.get(r))
                    .and_then(|m| m.child("multicam"));
                let Some(multicam) = multicam else {
                    self.skipped += 1;
                    return;
                };
                let mut any = false;
                for source in item.children_named("mc-source") {
                    let angle = multicam
                        .children_named("mc-angle")
                        .find(|a| a.attr("angleID") == source.attr("angleID"));
                    if let Some(angle) = angle {
                        let src = source.attr("srcEnable").unwrap_or("all");
                        self.nest(angle, (offset, start, duration), lane, src);
                        any = true;
                    }
                }
                if !any {
                    self.skipped += 1;
                }
            }
            "gap" => {}
            "transition" => {
                if !connected {
                    self.transitions.push((offset, duration));
                }
                return;
            }
            "video" | "audio" | "sync-clip" => {
                self.skipped += 1;
                return;
            }
            _ => return,
        }
        // What is connected to it: offsets in its own time.
        let connected_place = move |t: Tick| offset + (t - start);
        for child in item.elements().filter(|e| e.attr("lane").is_some()) {
            self.item(child, &connected_place, true);
        }
    }

    /// Transitions on the primary storyline: crossfades of the clips at
    /// their middle, each clip reaching half of it into the other.
    fn cross_transitions(&mut self) {
        let transitions = std::mem::take(&mut self.transitions);
        let primary = self.video.entry(0).or_default();
        for &(at, length) in &transitions {
            let cut = at + Tick(length.0 / 2);
            let half = Tick(length.0 / 2);
            if let Some(e) = primary.iter_mut().find(|e| e.start + e.duration == cut) {
                e.duration += half;
            }
            if let Some(e) = primary.iter_mut().find(|e| e.start == cut) {
                let back = half.min(e.source);
                e.start -= back;
                e.source -= back;
                e.duration += back;
            }
        }
    }
}

/// A format's frame rate and frame size.
type Format = (Option<FrameRate>, Option<(u32, u32)>);

/// Reads an FCPXML document's first sequence.
pub fn read_fcpxml(text: &str) -> Result<Document, String> {
    let root = xml::parse(text)?;
    if root.name != "fcpxml" {
        return Err("not an FCPXML file".into());
    }
    let resources = root.child("resources");
    let mut formats: HashMap<String, Format> = HashMap::new();
    let mut media: Vec<MediaRef> = Vec::new();
    let mut assets: HashMap<String, Asset> = HashMap::new();
    let mut skipped = 0;
    let mut compounds: HashMap<String, &Element> = HashMap::new();
    if let Some(res) = resources {
        for m in res.children_named("media") {
            compounds.insert(m.attr("id").unwrap_or_default().to_string(), m);
        }
        for f in res.children_named("format") {
            let size = f
                .attr("width")
                .zip(f.attr("height"))
                .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)));
            formats.insert(
                f.attr("id").unwrap_or_default().to_string(),
                (fcp_rate(f.attr("frameDuration")), size),
            );
        }
        for a in res.children_named("asset") {
            // The file: its `src`, or its media-rep's.
            let src = a
                .attr("src")
                .or_else(|| a.child("media-rep").and_then(|r| r.attr("src")));
            let Some(path) = src.and_then(url_path) else {
                skipped += 1;
                continue;
            };
            media.push(MediaRef {
                name: a.attr("name").unwrap_or_default().to_string(),
                source: MediaRefSource::File(path),
                saved: None,
                available: a.attr("duration").map(|d| fcp_time(Some(d))),
            });
            assets.insert(
                a.attr("id").unwrap_or_default().to_string(),
                Asset {
                    media: media.len() - 1,
                    video: a.attr("hasVideo") != Some("0"),
                    audio: a.attr("hasAudio") == Some("1"),
                },
            );
        }
    }
    // The library's (the resources' compound clips have theirs).
    let seq = root
        .child("library")
        .and_then(|l| first_named(l, "sequence"))
        .or_else(|| first_named(&root, "sequence"))
        .ok_or("the file has no sequence")?;
    let format = seq.attr("format").and_then(|f| formats.get(f).copied());
    let rate = format.and_then(|f| f.0);
    let name = first_named(&root, "project")
        .and_then(|p| p.attr("name"))
        .unwrap_or_default()
        .to_string();
    let spine = seq.child("spine").ok_or("the sequence has no spine")?;
    let mut walk = Walk {
        assets: &assets,
        resources: &compounds,
        media: &mut media,
        video: BTreeMap::new(),
        audio: BTreeMap::new(),
        roles: BTreeMap::new(),
        transitions: Vec::new(),
        skipped: 0,
        only: "all",
        depth: 0,
    };
    // The spine: its items one after another on the primary storyline;
    // a gap's or a clip's connected items in its own time.
    for item in spine.elements() {
        walk.item(item, &|t| t, false);
    }
    walk.cross_transitions();
    let Walk {
        video,
        audio,
        roles,
        skipped: walked,
        ..
    } = walk;
    // A lane's role names its track ("dialogue": Dialogue).
    let named = |kind: TrackKind, lane: i64, events: Vec<DocEvent>| {
        let mut t = track(kind, events);
        if let Some(Some(role)) = roles.get(&(kind == TrackKind::Video, lane)) {
            let mut c = role.chars();
            t.name = c
                .next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default();
        }
        t
    };
    let mut tracks = Vec::new();
    // Video lanes from the lowest up; audio lanes: the primary's, those
    // above, then those below.
    for (lane, mut events) in video {
        if !events.is_empty() {
            mark_crossfades(&mut events);
            tracks.push(named(TrackKind::Video, lane, events));
        }
    }
    let mut audio: Vec<(i64, Vec<DocEvent>)> = audio.into_iter().collect();
    audio.sort_by_key(|(lane, _)| if *lane >= 0 { (0, *lane) } else { (1, -*lane) });
    for (lane, mut events) in audio {
        if !events.is_empty() {
            mark_crossfades(&mut events);
            tracks.push(named(TrackKind::Audio, lane, events));
        }
    }
    let mut doc = document(name, rate, media, tracks, skipped + walked);
    doc.frame = format.and_then(|f| f.1);
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interchange::export_fcpxml;
    use crate::model::{
        AudioStreamInfo, MediaId, MediaInfo, MediaSource, Project, ProjectSettings, VideoStreamInfo,
    };
    use bv_core::SampleRate;

    fn s(x: i64) -> Tick {
        Tick::from_seconds(x)
    }

    /// A file's clip on V1 and A1 (3–8 s from its second 1), another's
    /// after it on V1.
    fn project() -> Project {
        let mut p = Project::new(ProjectSettings::default());
        let (v, a) = (p.tracks[0].id, p.tracks[1].id);
        let file = |p: &mut Project, path: &str| {
            p.add_media(MediaInfo {
                id: MediaId(0),
                source: MediaSource::File { path: path.into() },
                name: path.rsplit('\\').next().unwrap().into(),
                duration: s(20),
                video: Some(VideoStreamInfo {
                    width: 1920,
                    height: 1080,
                    frame_rate: p.settings.frame_rate,
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
            })
        };
        let m = file(&mut p, r"C:\Media\one clip.mp4");
        let ids = p.insert_media(m, s(2), Some(v), Some(a)).unwrap();
        p.trim_start(ids[0], s(3)).unwrap();
        p.trim_end(ids[0], s(8)).unwrap();
        let n = file(&mut p, r"C:\Media\two.mov");
        let ids = p.insert_media(n, s(8), Some(v), None).unwrap();
        p.trim_end(ids[0], s(10)).unwrap();
        p
    }

    /// The events of `doc`'s tracks of `kind`: start, length, media time,
    /// media name.
    fn events(doc: &Document, kind: TrackKind) -> Vec<(Tick, Tick, Tick, String)> {
        let rate = doc.rate.unwrap_or(FrameRate::FPS_30);
        let near = |t: Tick| rate.snap_nearest(t);
        doc.tracks
            .iter()
            .filter(|t| t.kind == kind)
            .flat_map(|t| &t.events)
            .map(|e| {
                (
                    near(e.start),
                    near(e.duration),
                    near(e.source),
                    doc.media[e.media].name.clone(),
                )
            })
            .collect()
    }

    #[test]
    fn fcpxml_comes_back_as_it_went() {
        let p = project();
        let (text, _) = export_fcpxml(&p, "Edit");
        let doc = read_fcpxml(&text).unwrap();
        assert_eq!(doc.name, "Edit");
        assert_eq!(doc.rate, Some(p.settings.frame_rate));
        let video = events(&doc, TrackKind::Video);
        let f = |x: i64| p.settings.frame_rate.snap_nearest(s(x));
        assert_eq!(video[0], (f(3), f(5), f(1), "one clip.mp4".into()));
        assert_eq!(video[1].0, f(8));
        assert_eq!(events(&doc, TrackKind::Audio).len(), 1);
    }

    #[test]
    fn an_fcpxml_spine_lays_its_clips_one_after_another() {
        let text = r#"<?xml version="1.0"?><fcpxml version="1.10"><resources>
            <format id="r1" frameDuration="100/2500s"/>
            <asset id="r2" name="a" start="0s" duration="20s" hasVideo="1" hasAudio="1" format="r1">
              <media-rep kind="original-media" src="file:///C:/A.mp4"/></asset>
            <asset id="r3" name="b" src="file:///C:/B.mp4" start="0s" duration="20s" hasVideo="1"/>
            </resources><library><event><project name="P"><sequence format="r1"><spine>
              <asset-clip ref="r2" offset="0s" start="2s" duration="4s">
                <asset-clip ref="r3" lane="1" offset="3s" start="0s" duration="1s"/>
              </asset-clip>
              <transition offset="3s" duration="2s"/>
              <asset-clip ref="r3" offset="4s" start="5s" duration="3s"/>
              <title offset="7s" duration="1s" name="End"><text><text-style ref="ts1">The </text-style><text-style ref="ts2">End</text-style></text></title>
            </spine></sequence></project></event></library></fcpxml>"#;
        let doc = read_fcpxml(text).unwrap();
        assert_eq!(doc.rate, Some(FrameRate::new(25, 1)));
        assert_eq!(doc.name, "P");
        let video = events(&doc, TrackKind::Video);
        // The primary storyline, the transition making them overlap a
        // second each way; the connected clip above, in the parent's time
        // (offset 3 s there: 1 s in).
        assert_eq!(video[0], (s(0), s(5), s(2), "a".into()));
        assert_eq!(video[1], (s(3), s(4), s(4), "b".into()));
        assert_eq!(video[3], (s(1), s(1), s(0), "b".into()));
        assert_eq!(events(&doc, TrackKind::Audio).len(), 1, "a has sound");
        // The title: Titles & Text saying it, on the primary storyline.
        assert_eq!(video[2], (s(7), s(1), s(0), "End".into()));
        let MediaRefSource::Generated(Generator::Plugin(fx)) = &doc.media[2].source else {
            panic!("a title")
        };
        assert_eq!(fx.id, TITLES_PLUGIN);
        assert_eq!(fx.params["text"].value, PluginValue::Text("The End".into()));
        assert_eq!(doc.skipped, 0);
    }

    /// FCPXML's retimed clips (a constant speed, a frozen frame), a
    /// compound clip and a multicam clip laid out in their place, and the
    /// roles naming the tracks.
    #[test]
    fn fcpxml_speeds_compounds_multicams_and_roles() {
        let text = r#"<?xml version="1.0"?><fcpxml version="1.10"><resources>
            <format id="r1" frameDuration="100/2500s"/>
            <asset id="a1" name="a" src="file:///C:/A.mp4" start="0s" duration="60s" hasVideo="1" hasAudio="1"/>
            <asset id="a2" name="b" src="file:///C:/B.mp4" start="0s" duration="60s" hasVideo="1" hasAudio="1"/>
            <media id="c1" name="Compound"><sequence format="r1"><spine>
              <asset-clip ref="a1" offset="0s" start="10s" duration="5s"/>
              <asset-clip ref="a2" offset="5s" start="20s" duration="5s"/>
            </spine></sequence></media>
            <media id="m1" name="Multi"><multicam format="r1">
              <mc-angle name="Cam A" angleID="x"><asset-clip ref="a1" offset="0s" start="0s" duration="60s"/></mc-angle>
              <mc-angle name="Cam B" angleID="y"><asset-clip ref="a2" offset="0s" start="0s" duration="60s"/></mc-angle>
            </multicam></media>
            </resources><library><event><project name="P"><sequence format="r1"><spine>
              <asset-clip ref="a1" offset="0s" start="2s" duration="4s" audioRole="dialogue.dialogue-1">
                <timeMap><timept time="0s" value="0s"/><timept time="10s" value="20s"/></timeMap>
                <asset-clip ref="a2" lane="-1" offset="2s" start="0s" duration="1s" audioRole="music.music-1" srcEnable="audio"/>
              </asset-clip>
              <asset-clip ref="a2" offset="4s" start="3s" duration="1s" audioRole="dialogue">
                <timeMap><timept time="0s" value="7s"/><timept time="10s" value="7s"/></timeMap>
              </asset-clip>
              <ref-clip ref="c1" offset="5s" start="3s" duration="4s"/>
              <mc-clip ref="m1" offset="9s" start="30s" duration="2s">
                <mc-source angleID="y" srcEnable="video"/><mc-source angleID="x" srcEnable="audio"/>
              </mc-clip>
            </spine></sequence></project></event></library></fcpxml>"#;
        let doc = read_fcpxml(text).unwrap();
        let v = &doc.tracks[0].events;
        let name = |e: &DocEvent| doc.media[e.media].name.clone();
        // Twice as fast: 2 s of its time is 4 s of the media.
        assert_eq!((v[0].start, v[0].source, v[0].speed), (s(0), s(4), 2.0));
        // A frozen frame: 7 s held.
        assert!(v[1].frozen && v[1].source == s(7));
        // The compound clip's 3 s to 7 s: a's 13–15 s, then b's 20–22 s.
        assert_eq!(
            (v[2].start, v[2].duration, v[2].source, name(&v[2])),
            (s(5), s(2), s(13), "a".into())
        );
        assert_eq!(
            (v[3].start, v[3].duration, v[3].source, name(&v[3])),
            (s(7), s(2), s(20), "b".into())
        );
        // The multicam clip: angle y's picture, angle x's sound, from 30 s.
        assert_eq!(
            (v[4].start, v[4].source, name(&v[4])),
            (s(9), s(30), "b".into())
        );
        let a = &doc
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .unwrap();
        let last = a.events.last().unwrap();
        assert_eq!(
            (last.start, last.source, name(last)),
            (s(9), s(30), "a".into())
        );
        assert_eq!(doc.skipped, 0);
        // The primary lane's sound is dialogue but the compound's and the
        // multicam's, which have no role: no name; the music below it is
        // Music.
        assert_eq!(a.name, "");
        let names: Vec<&str> = doc.tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names.last(), Some(&"Music"), "{names:?}");
    }

    #[test]
    fn other_files_are_refused() {
        assert!(read_fcpxml("<xmeml/>").is_err());
        assert!(read_fcpxml("not xml").is_err());
    }
}
