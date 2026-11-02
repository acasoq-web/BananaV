use std::collections::HashMap;

use bv_core::{FrameRate, SampleRate, Tick, TimeRange};
use serde_json::Value;

use super::*;
use crate::fx::{Direction, Effect, EffectKind, Transition};
use crate::model::{
    AudioStreamInfo, ClipId, Generator, GroupId, ImageSequence, MediaId, MediaInfo, MediaMarker,
    MediaSource, Project, ProjectSettings, TrackKind, VideoStreamInfo,
};

fn s(seconds: f64) -> Tick {
    Tick::from_seconds_f64(seconds)
}

fn file(path: &str, seconds: f64, video: Option<FrameRate>, audio: bool) -> MediaInfo {
    let mut m = MediaInfo::generated(Generator::ColorBars, &ProjectSettings::default());
    m.source = MediaSource::File { path: path.into() };
    m.name = path.rsplit('/').next().unwrap().to_string();
    m.duration = s(seconds);
    m.video = video.map(|frame_rate| VideoStreamInfo {
        width: 1920,
        height: 1080,
        frame_rate,
        pixel_aspect: Default::default(),
        field_order: Default::default(),
        spherical: false,
        full_range: false,
        reading: Default::default(),
    });
    m.audio = audio.then_some(AudioStreamInfo {
        sample_rate: SampleRate::HZ_48000,
        channels: 2,
    });
    m
}

/// The media as a caller finds them: files as BananaV saved them (probed
/// the same), generators as nothing to find.
fn found_as_saved(doc: &Document) -> Vec<Option<MediaInfo>> {
    doc.media
        .iter()
        .map(|r| match r.source {
            MediaRefSource::File(_) | MediaRefSource::Sequence { .. } => r.saved.clone(),
            _ => None,
        })
        .collect()
}

/// The project with its ids counted in order (media by name, groups and
/// markers as met), so two projects that say the same compare equal.
fn canonical(p: &Project) -> Value {
    let mut p = p.clone();
    let mut names: Vec<(String, MediaId)> =
        p.media.iter().map(|m| (m.name.clone(), m.id)).collect();
    names.sort();
    let media: HashMap<MediaId, MediaId> = names
        .iter()
        .enumerate()
        .map(|(i, (_, id))| (*id, MediaId(i as u64 + 1)))
        .collect();
    for m in &mut p.media {
        m.id = media[&m.id];
    }
    p.media.sort_by_key(|m| m.id);
    let mut groups: HashMap<GroupId, GroupId> = HashMap::new();
    let mut n = 0;
    for (ti, t) in p.tracks.iter_mut().enumerate() {
        t.id = crate::model::TrackId(ti as u64 + 1);
        for c in &mut t.clips {
            n += 1;
            c.id = ClipId(n);
            c.media = media[&c.media];
            c.group = c.group.map(|g| {
                let next = GroupId(groups.len() as u64 + 1);
                *groups.entry(g).or_insert(next)
            });
        }
    }
    for (i, m) in p.markers.iter_mut().enumerate() {
        m.id = crate::model::MarkerId(i as u64 + 1);
    }
    for (i, r) in p.regions.iter_mut().enumerate() {
        r.id = crate::model::MarkerId(i as u64 + 1);
    }
    for b in &mut p.bins {
        for m in &mut b.media {
            *m = media[m];
        }
    }
    p.next_id = 0;
    serde_json::to_value(&p).unwrap()
}

/// A project with what OpenTimelineIO has to carry: crossfades of grouped
/// video and sound, fades (butted ones too), a wipe, a speed, a frozen
/// frame, an event inside another, a muted event and track, a generator,
/// an image sequence, media markers, markers, regions, the ruler's offset.
fn sample() -> Project {
    let mut p = Project::new(ProjectSettings {
        frame_rate: FrameRate::FPS_25,
        ..ProjectSettings::default()
    });
    let (v1, a1) = (p.tracks[0].id, p.tracks[1].id);
    let v2 = p.add_track(TrackKind::Video);
    let a2 = p.add_track(TrackKind::Audio);
    let mut cam = file("C:/media/cam one.mov", 60.0, Some(FrameRate::FPS_25), true);
    cam.markers = vec![
        MediaMarker {
            time: s(6.0),
            length: Tick::ZERO,
            name: "Clap".into(),
        },
        MediaMarker {
            time: s(7.0),
            length: s(1.0),
            name: "Take".into(),
        },
    ];
    let cam = p.add_media(cam);
    let music = p.add_media(file("C:/media/music.wav", 120.0, None, true));
    let bars = p.add_media(MediaInfo::generated(Generator::ColorBars, &p.settings));
    let mut seq = file(
        "C:/renders/shot_0001.png",
        2.0,
        Some(FrameRate::FPS_25),
        false,
    );
    seq.sequence = Some(ImageSequence { count: 50 });
    let seq = p.add_media(seq);
    p.add_media(file("C:/media/unused.wav", 3.0, None, true));

    let first = p
        .insert_media_range(cam, Tick::ZERO, s(5.0), s(10.0), Some(v1), Some(a1))
        .unwrap();
    let second = p
        .insert_media_range(cam, s(8.0), s(30.0), s(10.0), Some(v1), Some(a1))
        .unwrap();
    {
        let c = p.clip_mut(second[0]).unwrap();
        c.transition = Transition::Wipe(Direction::Left);
        // Hidden by the crossfade, kept.
        c.fade_in = s(0.4);
    }
    let bar = p
        .insert_media_range(bars, s(20.0), Tick::ZERO, s(5.0), Some(v1), None)
        .unwrap()[0];
    {
        let c = p.clip_mut(bar).unwrap();
        c.fade_in = s(1.0);
        c.fade_out = s(2.0);
        c.level = 0.5;
        c.effects.push(Effect::new(EffectKind::GaussianBlur));
    }
    let shots = p
        .insert_media_range(seq, s(25.0), Tick::ZERO, s(2.0), Some(v1), None)
        .unwrap()[0];
    p.clip_mut(shots).unwrap().fade_in = s(0.5);

    let fast = p
        .insert_media_range(cam, s(2.0), Tick::ZERO, s(4.0), Some(v2), None)
        .unwrap()[0];
    p.clip_mut(fast).unwrap().rate = 2.0;
    let held = p
        .insert_media_range(cam, s(6.0), s(1.0), s(1.0), Some(v2), None)
        .unwrap()[0];
    p.clip_mut(held).unwrap().freeze = Some(s(12.0));
    p.insert_media_range(cam, s(10.0), s(40.0), s(10.0), Some(v2), None)
        .unwrap();
    // Inside the one before: a lane of its own.
    p.insert_media_range(bars, s(12.0), Tick::ZERO, s(2.0), Some(v2), None)
        .unwrap();
    let quiet = p
        .insert_media_range(cam, s(30.0), Tick::ZERO, s(1.0), Some(v2), None)
        .unwrap()[0];
    p.clip_mut(quiet).unwrap().muted = true;
    {
        let t = p.track_by_id_mut(v2).unwrap();
        t.name = "B-roll".into();
        t.level = 0.8;
    }

    let song = p
        .insert_media_range(music, Tick::ZERO, s(10.0), s(30.0), None, Some(a2))
        .unwrap()[0];
    {
        let c = p.clip_mut(song).unwrap();
        c.fade_in = s(2.0);
        c.fade_out = s(3.0);
        c.preserve_pitch = true;
    }
    p.track_by_id_mut(a2).unwrap().muted = true;

    p.add_marker(s(3.0));
    let m = p.markers[0].id;
    p.rename_marker(m, "Start".into());
    let r = p.add_region(TimeRange::new(s(10.0), s(2.0)));
    p.rename_marker(r, "Chorus".into());
    p.ruler_offset = s(3600.0);
    let note = p.add_note(s(2.0), "Mix", 2, (1_667_827_201, 0));
    p.note_mut(note).unwrap().text = "Notes".into();
    p.loop_region = Some(TimeRange::new(s(1.0), s(4.0)));
    p.check_invariants().unwrap();
    assert!(!first.is_empty());
    p
}

/// Checks a document's tracks as OpenTimelineIO requires them: a
/// transition between two items, never by another, within both; an item's
/// two transitions apart.
fn check_structure(text: &str) {
    let v: Value = serde_json::from_str(text).unwrap();
    for t in v["tracks"]["children"].as_array().unwrap() {
        let items = t["children"].as_array().unwrap();
        let length = |i: &Value| read_range(&i["source_range"]).unwrap().1;
        let offset = |i: &Value, k: &str| read_time(&i[k]).unwrap().0;
        let is_transition = |i: &Value| i["OTIO_SCHEMA"] == "Transition.1";
        for (k, item) in items.iter().enumerate() {
            if is_transition(item) {
                assert!(k > 0 && k + 1 < items.len(), "a transition at an end");
                let (before, after) = (&items[k - 1], &items[k + 1]);
                assert!(
                    !is_transition(before) && !is_transition(after),
                    "two transitions in a row"
                );
                assert!(offset(item, "in_offset") <= length(before));
                assert!(offset(item, "out_offset") <= length(after));
            } else {
                let head = if k > 0 && is_transition(&items[k - 1]) {
                    offset(&items[k - 1], "out_offset")
                } else {
                    Tick::ZERO
                };
                let tail = if k + 1 < items.len() && is_transition(&items[k + 1]) {
                    offset(&items[k + 1], "in_offset")
                } else {
                    Tick::ZERO
                };
                assert!(head + tail <= length(item), "{}", item["name"]);
            }
        }
    }
}

#[test]
fn a_project_comes_back_as_it_went() {
    let p = sample();
    let text = export(&p, "Sample");
    check_structure(&text);
    let doc = read(&text).unwrap();
    assert_eq!(doc.name, "Sample");
    assert_eq!(doc.skipped, 0);
    // The cam's file once, however many events use it.
    let files: Vec<&MediaRefSource> = doc.media.iter().map(|m| &m.source).collect();
    assert!(files.contains(&&MediaRefSource::File("C:/media/cam one.mov".into())));
    assert!(files.contains(&&MediaRefSource::Sequence {
        first: "C:/renders/shot_0001.png".into(),
        count: 50,
        rate: Some(FrameRate::FPS_25),
    }));
    assert_eq!(doc.media.len(), 4, "{files:?}");
    let back = build(&doc, &found_as_saved(&doc), &ProjectSettings::default()).unwrap();
    assert!(back.missing.is_empty());
    assert_eq!(back.skipped, 0);
    assert_eq!(canonical(&back.project), canonical(&p));
}

/// Busses, assignable effects chains, the Master's effects, the tracks'
/// outputs and sends, and the busses' sends come back as they went.
#[test]
fn busses_and_sends_come_back() {
    use crate::busses::{Output, Send, SendTo};
    let mut p = sample();
    let a1 = p
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Audio)
        .unwrap()
        .id;
    let bus = p.add_bus().unwrap();
    let reverb = crate::fx::Effect::new(crate::fx::EffectKind::Reverb);
    let fx = p.add_fx_chain(vec![reverb]).unwrap();
    assert!(p.set_track_output(a1, Output::Bus(bus)));
    let mut send = Send::new(SendTo::Fx(fx));
    send.gain_db = -9.0;
    send.link_pan = true;
    assert!(p.set_send(a1, send));
    let mut bus_send = Send::new(SendTo::Fx(fx));
    bus_send.gain_db = -3.0;
    assert!(p.set_bus_send(bus, bus_send));
    p.master_fx
        .push(crate::fx::Effect::new(crate::fx::EffectKind::Limiter));
    p.check_invariants().unwrap();
    let doc = read(&export(&p, "Busses")).unwrap();
    let back = build(&doc, &found_as_saved(&doc), &ProjectSettings::default()).unwrap();
    assert_eq!(back.project.busses, p.busses);
    assert_eq!(back.project.fx_chains, p.fx_chains);
    assert_eq!(canonical(&back.project), canonical(&p));
}

/// Events' velocity, transition progress and reversal: other programs see
/// a reversed event's speed below zero; BananaV's come back as they went.
#[test]
fn velocity_progress_and_reversed_events_come_back() {
    use crate::envelope::EnvelopeKind;
    let mut p = sample();
    let video: Vec<_> = p
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Video)
        .map(|t| t.id)
        .collect();
    let (v1, v2) = (video[0], video[1]);
    let at = |p: &Project, track, start| {
        p.track(track)
            .unwrap()
            .clips
            .iter()
            .find(|c| c.start == start)
            .unwrap()
            .id
    };
    let (second, bar, fast) = (at(&p, v1, s(8.0)), at(&p, v1, s(20.0)), at(&p, v2, s(2.0)));
    p.toggle_event_envelope(second, EnvelopeKind::Velocity)
        .unwrap();
    p.set_envelope_point(second, EnvelopeKind::Velocity, s(3.0), 0.5)
        .unwrap();
    p.toggle_event_envelope(bar, EnvelopeKind::TransitionProgress)
        .unwrap();
    p.set_envelope_point(bar, EnvelopeKind::TransitionProgress, s(0.5), 1.0)
        .unwrap();
    p.reverse_event(fast).unwrap();
    let text = export(&p, "Sample");
    check_structure(&text);
    let v: Value = serde_json::from_str(&text).unwrap();
    let broll = v["tracks"]["children"][0]["children"].as_array().unwrap();
    assert_eq!(broll[1]["effects"][0]["time_scalar"], -2.0);
    let doc = read(&text).unwrap();
    let back = build(&doc, &found_as_saved(&doc), &ProjectSettings::default()).unwrap();
    assert_eq!(canonical(&back.project), canonical(&p));
}

/// A media file a bundle left out (not found when it was packed): a
/// `MissingReference`, offline again where BananaV saved it was.
#[test]
fn media_left_out_keep_where_they_were() {
    let p = sample();
    let url = |path: &str| (!path.contains("music")).then(|| crate::interchange::file_url(path));
    let text = export_with(&p, "Sample", &url);
    let doc = read(&text).unwrap();
    let music = doc.media.iter().find(|m| m.name == "music.wav").unwrap();
    assert_eq!(music.source, MediaRefSource::Missing);
    let back = build(&doc, &found_as_saved(&doc), &ProjectSettings::default()).unwrap();
    assert_eq!(back.missing, ["music.wav"]);
    let offline = back
        .project
        .media
        .iter()
        .find(|m| m.name == "music.wav")
        .unwrap();
    assert_eq!(
        offline.source,
        MediaSource::File {
            path: "C:/media/music.wav".into()
        }
    );
    assert_eq!(canonical(&back.project), canonical(&p));
}

#[test]
fn what_other_programs_read() {
    let text = export(&sample(), "Sample");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["OTIO_SCHEMA"], "Timeline.1");
    assert_eq!(
        v["global_start_time"]["value"], 90000.0,
        "an hour at 25 fps"
    );
    let tracks = v["tracks"]["children"].as_array().unwrap();
    let names: Vec<&str> = tracks.iter().map(|t| t["name"].as_str().unwrap()).collect();
    // From the bottom up; the event inside another over its track.
    assert_eq!(
        names,
        ["B-roll", "B-roll (2)", "Video 1", "Audio 1", "Audio 2"]
    );
    let kinds: Vec<&str> = tracks.iter().map(|t| t["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["Video", "Video", "Video", "Audio", "Audio"]);
    assert_eq!(tracks[4]["enabled"], false);
    let v1 = tracks[2]["children"].as_array().unwrap();
    let schemas: Vec<&str> = v1
        .iter()
        .map(|i| i["OTIO_SCHEMA"].as_str().unwrap())
        .collect();
    assert_eq!(
        schemas,
        [
            "Clip.2",
            "Transition.1",
            "Clip.2",
            "Gap.1",
            "Transition.1",
            "Clip.2",
            "Transition.1",
            "Gap.1",
            "Transition.1",
            "Clip.2",
        ]
    );
    // The crossfade cut in the middle of its two seconds: a second each
    // way, the wipe named.
    assert_eq!(v1[1]["in_offset"]["value"], 25.0);
    assert_eq!(v1[1]["out_offset"]["value"], 25.0);
    assert_eq!(v1[1]["transition_type"], "Custom_Transition");
    assert_eq!(v1[1]["name"], "Wipe left");
    assert_eq!(v1[0]["source_range"]["start_time"]["value"], 125.0);
    assert_eq!(v1[0]["source_range"]["duration"]["value"], 225.0);
    assert_eq!(v1[2]["source_range"]["start_time"]["value"], 775.0);
    let reference = &v1[0]["media_references"]["DEFAULT_MEDIA"];
    assert_eq!(reference["OTIO_SCHEMA"], "ExternalReference.1");
    assert_eq!(reference["target_url"], "file:///C:/media/cam%20one.mov");
    assert_eq!(v1[0]["markers"].as_array().unwrap().len(), 2);
    // The bars' fades: from a gap, to the gap of no length before the
    // pictures butted to them.
    assert_eq!(v1[4]["in_offset"]["value"], 0.0);
    assert_eq!(v1[4]["out_offset"]["value"], 25.0);
    assert_eq!(v1[6]["in_offset"]["value"], 50.0);
    assert_eq!(v1[7]["source_range"]["duration"]["value"], 0.0);
    let bars = &v1[5]["media_references"]["DEFAULT_MEDIA"];
    assert_eq!(bars["OTIO_SCHEMA"], "GeneratorReference.1");
    assert_eq!(bars["generator_kind"], "SMPTEBars");
    let pictures = &v1[9]["media_references"]["DEFAULT_MEDIA"];
    assert_eq!(pictures["OTIO_SCHEMA"], "ImageSequenceReference.1");
    assert_eq!(pictures["target_url_base"], "file:///C:/renders/");
    assert_eq!(pictures["name_prefix"], "shot_");
    assert_eq!(pictures["frame_zero_padding"], 4);
    assert_eq!(pictures["start_frame"], 1);
    // B-roll: a speed and a frozen frame.
    let broll = tracks[0]["children"].as_array().unwrap();
    assert_eq!(broll[1]["effects"][0]["OTIO_SCHEMA"], "LinearTimeWarp.1");
    assert_eq!(broll[1]["effects"][0]["time_scalar"], 2.0);
    assert_eq!(broll[2]["effects"][0]["OTIO_SCHEMA"], "FreezeFrame.1");
    assert_eq!(broll[2]["source_range"]["start_time"]["value"], 300.0);
    // The stack's markers.
    let markers = v["tracks"]["markers"].as_array().unwrap();
    assert_eq!(markers.len(), 2);
    assert_eq!(markers[0]["color"], "ORANGE");
    assert_eq!(markers[1]["color"], "GREEN");
    assert_eq!(markers[1]["marked_range"]["duration"]["value"], 50.0);
}

/// A timeline as another editor writes it: an hour's timecode, media with
/// their own, a fade from black, a dissolve, a slowed clip, a generator,
/// a nested clip, older schemas, a non-finite number.
const FOREIGN: &str = r#"{
    "OTIO_SCHEMA": "Timeline.1",
    "metadata": {"other": {"gain": NaN, "peak": -Infinity, "text": "NaN stays"}},
    "name": "Edit 1",
    "global_start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 86400.0},
    "tracks": {
        "OTIO_SCHEMA": "Stack.1",
        "name": "tracks",
        "children": [
            {
                "OTIO_SCHEMA": "Track.1",
                "kind": "Video",
                "name": "V1",
                "children": [
                    {"OTIO_SCHEMA": "Gap.1", "source_range": {"OTIO_SCHEMA": "TimeRange.1",
                        "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 0.0},
                        "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 24.0}}},
                    {"OTIO_SCHEMA": "Transition.1", "transition_type": "SMPTE_Dissolve",
                        "in_offset": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 0.0},
                        "out_offset": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 12.0}},
                    {"OTIO_SCHEMA": "Clip.2", "name": "A001.mov",
                        "source_range": {"OTIO_SCHEMA": "TimeRange.1",
                            "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 86424.0},
                            "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 48.0}},
                        "media_references": {"DEFAULT_MEDIA": {"OTIO_SCHEMA": "ExternalReference.1",
                            "target_url": "file:///Volumes/Media/A001.mov",
                            "available_range": {"OTIO_SCHEMA": "TimeRange.1",
                                "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 86400.0},
                                "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 240.0}}}},
                        "active_media_reference_key": "DEFAULT_MEDIA",
                        "markers": [{"OTIO_SCHEMA": "Marker.2", "name": "focus", "color": "RED",
                            "marked_range": {"OTIO_SCHEMA": "TimeRange.1",
                                "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 86430.0},
                                "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 0.0}}}]},
                    {"OTIO_SCHEMA": "Transition.1", "transition_type": "SMPTE_Dissolve", "name": "Cross Dissolve",
                        "in_offset": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 6.0},
                        "out_offset": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 6.0}},
                    {"OTIO_SCHEMA": "Clip.1", "name": "B002.mov",
                        "source_range": {"OTIO_SCHEMA": "TimeRange.1",
                            "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 10.0},
                            "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 24.0}},
                        "media_reference": {"OTIO_SCHEMA": "ExternalReference.1", "target_url": "media/B%20002.mov",
                            "available_range": {"OTIO_SCHEMA": "TimeRange.1",
                                "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 0.0},
                                "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 480.0}}},
                        "effects": [{"OTIO_SCHEMA": "LinearTimeWarp.1", "effect_name": "LinearTimeWarp", "time_scalar": 0.5}]},
                    {"OTIO_SCHEMA": "Gap.1", "source_range": {"OTIO_SCHEMA": "TimeRange.1",
                        "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 0.0},
                        "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 24.0}}},
                    {"OTIO_SCHEMA": "Clip.2", "name": "Bars",
                        "source_range": {"OTIO_SCHEMA": "TimeRange.1",
                            "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 0.0},
                            "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 24.0}},
                        "media_references": {"DEFAULT_MEDIA": {"OTIO_SCHEMA": "GeneratorReference.1",
                            "generator_kind": "SMPTEBars", "parameters": {}}},
                        "active_media_reference_key": "DEFAULT_MEDIA"},
                    {"OTIO_SCHEMA": "Stack.1", "name": "Compound",
                        "source_range": {"OTIO_SCHEMA": "TimeRange.1",
                            "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 12.0},
                            "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 24.0}},
                        "children": [
                            {"OTIO_SCHEMA": "Track.1", "kind": "Video", "children": [
                                {"OTIO_SCHEMA": "Clip.2", "name": "A001.mov",
                                    "source_range": {"OTIO_SCHEMA": "TimeRange.1",
                                        "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 86400.0},
                                        "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 48.0}},
                                    "media_references": {"DEFAULT_MEDIA": {"OTIO_SCHEMA": "ExternalReference.1",
                                        "target_url": "file:///Volumes/Media/A001.mov",
                                        "available_range": {"OTIO_SCHEMA": "TimeRange.1",
                                            "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 86400.0},
                                            "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 240.0}}}},
                                    "active_media_reference_key": "DEFAULT_MEDIA"}]},
                            {"OTIO_SCHEMA": "Track.1", "kind": "Audio", "children": [
                                {"OTIO_SCHEMA": "Gap.1", "source_range": {"OTIO_SCHEMA": "TimeRange.1",
                                    "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 0.0},
                                    "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 48.0}}}]}
                        ]}
                ]
            },
            {
                "OTIO_SCHEMA": "Track.1",
                "kind": "Audio",
                "name": "A1",
                "children": [
                    {"OTIO_SCHEMA": "Gap.1", "source_range": {"OTIO_SCHEMA": "TimeRange.1",
                        "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 0.0},
                        "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 24.0}}},
                    {"OTIO_SCHEMA": "Clip.2", "name": "A001.mov",
                        "source_range": {"OTIO_SCHEMA": "TimeRange.1",
                            "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 86424.0},
                            "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 54.0}},
                        "media_references": {"DEFAULT_MEDIA": {"OTIO_SCHEMA": "ExternalReference.1",
                            "target_url": "file:///Volumes/Media/A001.mov",
                            "available_range": {"OTIO_SCHEMA": "TimeRange.1",
                                "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 86400.0},
                                "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 240.0}}}},
                        "active_media_reference_key": "DEFAULT_MEDIA"}
                ]
            },
            {"OTIO_SCHEMA": "Track.1", "kind": "Subtitle", "name": "Subtitles", "children": []}
        ],
        "markers": [
            {"OTIO_SCHEMA": "Marker.1", "name": "Act 2", "color": "RED",
                "marked_range": {"OTIO_SCHEMA": "TimeRange.1",
                    "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 48.0},
                    "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 0.0}}},
            {"OTIO_SCHEMA": "Marker.2", "name": "Credits", "color": "GREEN", "comment": "",
                "marked_range": {"OTIO_SCHEMA": "TimeRange.1",
                    "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 72.0},
                    "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24.0, "value": 24.0}}}
        ]
    }
}"#;

#[test]
fn another_programs_timeline() {
    let doc = read(FOREIGN).unwrap();
    assert_eq!(doc.name, "Edit 1");
    // The subtitles' track, and the nested clip's sound track.
    assert_eq!(doc.skipped, 2);
    let sources: Vec<&MediaRefSource> = doc.media.iter().map(|m| &m.source).collect();
    assert_eq!(
        sources,
        [
            &MediaRefSource::File("/Volumes/Media/A001.mov".into()),
            &MediaRefSource::File("media/B 002.mov".into()),
            &MediaRefSource::Generated(Generator::ColorBars),
        ]
    );
    let f = |n: i64| FrameRate::FPS_24.frame_start(n);
    let mut a001 = file(
        "/Volumes/Media/A001.mov",
        10.0,
        Some(FrameRate::FPS_24),
        true,
    );
    a001.video.as_mut().unwrap().width = 3840;
    a001.video.as_mut().unwrap().height = 2160;
    let b002 = file("media/B 002.mov", 20.0, Some(FrameRate::FPS_24), false);
    let found = vec![Some(a001), Some(b002), None];
    let imported = build(&doc, &found, &ProjectSettings::default()).unwrap();
    assert!(imported.missing.is_empty());
    let p = imported.project;
    assert_eq!(p.settings.frame_rate, FrameRate::FPS_24);
    assert_eq!(
        (p.settings.width, p.settings.height),
        (3840, 2160),
        "the first video's frame"
    );
    assert_eq!(p.ruler_offset, s(3600.0));
    assert_eq!(p.tracks.len(), 2);
    let (v, a) = (&p.tracks[0], &p.tracks[1]);
    assert_eq!((v.name.as_str(), a.name.as_str()), ("V1", "A1"));
    let spans: Vec<(Tick, Tick, Tick)> = v
        .clips
        .iter()
        .map(|c| (c.start, c.duration, c.source_in))
        .collect();
    assert_eq!(
        spans,
        [
            // Faded in from black, half a second; through the dissolve.
            (f(24), f(54), f(24)),
            // The dissolve's start, at half speed: its handles too.
            (f(66), f(30), FrameRate::FPS_24.frame_start(7)),
            (f(120), f(24), Tick::ZERO),
            // The nested clip: its 12th to 36th frames.
            (f(144), f(24), f(12)),
        ]
    );
    assert_eq!(v.clips[0].fade_in, f(12));
    assert_eq!(v.clips[1].rate, 0.5);
    let bars = p.media(v.clips[2].media).unwrap();
    assert_eq!(bars.source, MediaSource::Generated(Generator::ColorBars));
    // The sound, grouped with its picture.
    assert_eq!(a.clips.len(), 1);
    assert_eq!((a.clips[0].start, a.clips[0].duration), (f(24), f(54)));
    assert!(a.clips[0].group.is_some());
    assert_eq!(a.clips[0].group, v.clips[0].group);
    assert_eq!(v.clips[3].group, None);
    // The clip's marker, the media's now (in its own time).
    let cam = p.media(v.clips[0].media).unwrap();
    assert_eq!(cam.markers.len(), 1);
    assert_eq!(
        (cam.markers[0].time, cam.markers[0].name.as_str()),
        (f(30), "focus")
    );
    assert_eq!(p.markers.len(), 1);
    assert_eq!(
        (p.markers[0].time, p.markers[0].name.as_str()),
        (s(2.0), "Act 2")
    );
    assert_eq!(p.regions.len(), 1);
    assert_eq!(p.regions[0].range, TimeRange::new(s(3.0), s(1.0)));
}

#[test]
fn media_not_found_stay_offline() {
    let doc = read(FOREIGN).unwrap();
    let imported = build(&doc, &[None, None, None], &ProjectSettings::default()).unwrap();
    assert_eq!(imported.missing, ["A001.mov", "B002.mov"]);
    let p = &imported.project;
    let a001 = p.media(p.tracks[0].clips[0].media).unwrap();
    assert_eq!(
        a001.source,
        MediaSource::File {
            path: "/Volumes/Media/A001.mov".into()
        }
    );
    assert!(
        a001.video.is_some() && a001.audio.is_some(),
        "the streams its events use"
    );
    assert_eq!(a001.duration, s(10.0), "the length the file gave it");
    let b002 = p.media(p.tracks[0].clips[1].media).unwrap();
    assert!(b002.video.is_some() && b002.audio.is_none());
}

#[test]
fn rates_and_times() {
    for rate in [
        FrameRate::FPS_23_976,
        FrameRate::FPS_24,
        FrameRate::FPS_25,
        FrameRate::FPS_29_97,
        FrameRate::FPS_30,
        FrameRate::FPS_59_94,
        FrameRate::FPS_60,
    ] {
        assert_eq!(frame_rate(rate.as_f64()), Some(rate));
        for n in [0, 1, 1001, 107_892, 5_000_000] {
            let t = rate.frame_start(n);
            let v = rational(t, rate);
            assert_eq!(v["value"], n as f64);
            assert_eq!(read_time(&v).unwrap().0, t, "{rate} {n}");
        }
    }
    // Written rounded, NTSC's rates are still theirs.
    assert_eq!(frame_rate(29.97), Some(FrameRate::FPS_29_97));
    assert_eq!(frame_rate(23.98), Some(FrameRate::FPS_23_976));
    assert_eq!(frame_rate(12.5), Some(FrameRate::new(25, 2)));
    assert_eq!(ticks(48_000.0, 48_000.0), Tick::SECOND);
    assert_eq!(frame_rate(0.0), None);
}

#[test]
fn addresses() {
    assert_eq!(
        url_path("file:///C:/a%20b/c.mov").as_deref(),
        Some("C:/a b/c.mov")
    );
    assert_eq!(
        url_path("file:///home/u/x.mov").as_deref(),
        Some("/home/u/x.mov")
    );
    assert_eq!(
        url_path("file://localhost/x.mov").as_deref(),
        Some("/x.mov")
    );
    assert_eq!(
        url_path("file://server/share/x.mov").as_deref(),
        Some("//server/share/x.mov")
    );
    assert_eq!(url_path("file:/C:/x.mov").as_deref(), Some("C:/x.mov"));
    assert_eq!(
        url_path("media/a%20b.mov").as_deref(),
        Some("media/a b.mov")
    );
    assert_eq!(
        url_path(r"C:\clips\100%.mov").as_deref(),
        Some(r"C:\clips\100%.mov")
    );
    assert_eq!(url_path("https://example.com/a.mov"), None);
    assert_eq!(url_path(""), None);
    assert_eq!(relative_url("media/a b+é.mov"), "media/a%20b%2B%C3%A9.mov");
    assert_eq!(
        url_path(&relative_url("media/a b+é.mov")).as_deref(),
        Some("media/a b+é.mov")
    );
}

#[test]
fn lanes_for_what_transitions_cannot_say() {
    // Three at once, crossfades off, a muted event's overlap.
    let mut p = Project::new(ProjectSettings {
        frame_rate: FrameRate::FPS_25,
        ..ProjectSettings::default()
    });
    let v = p.tracks[0].id;
    let cam = p.add_media(file("C:/m/cam.mov", 100.0, Some(FrameRate::FPS_25), false));
    for (at, len) in [
        (0.0, 10.0),
        (8.0, 10.0),
        (9.0, 10.0),
        (30.0, 5.0),
        (33.0, 5.0),
    ] {
        p.insert_media_range(cam, s(at), s(at), s(len), Some(v), None)
            .unwrap();
    }
    let last = p.tracks[0].clips[4].id;
    p.clip_mut(last).unwrap().muted = true;
    for crossfades in [true, false] {
        p.auto_crossfade = crossfades;
        let text = export(&p, "Lanes");
        check_structure(&text);
        let v: Value = serde_json::from_str(&text).unwrap();
        // With crossfades, one lane for the third event at once and the
        // muted one; without, every overlap its own.
        let lanes = v["tracks"]["children"].as_array().unwrap().len() - 1;
        assert_eq!(lanes, if crossfades { 2 } else { 3 });
        let doc = read(&text).unwrap();
        let back = build(&doc, &found_as_saved(&doc), &ProjectSettings::default()).unwrap();
        assert_eq!(canonical(&back.project), canonical(&p));
    }
}
