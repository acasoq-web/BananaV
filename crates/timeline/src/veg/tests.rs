//! A project through a `.veg` and back.

use bv_core::{Tick, TimeRange};

use super::*;
use crate::envelope::{Envelope, EnvelopeKind, EnvelopePoint};
use crate::fx::{FadeCurve, KeyCurve};
use crate::model::{
    AudioStreamInfo, Clip, ClipId, CompositeMode, GroupId, MediaInfo, MediaSource, Project,
    ProjectSettings, Summary, TrackKind, VideoStreamInfo,
};

fn ms(n: i64) -> Tick {
    Tick::from_timescale(n, 1000)
}

fn file(path: &str, video: bool, audio: bool, seconds: i64) -> MediaInfo {
    let s = ProjectSettings::default();
    let mut m = MediaInfo::generated(crate::model::Generator::ColorBars, &s);
    m.source = MediaSource::File {
        path: path.to_string(),
    };
    m.name = path.rsplit('\\').next().unwrap().to_string();
    m.duration = Tick::from_seconds(seconds);
    m.still = false;
    m.video = video.then_some(VideoStreamInfo {
        width: 1280,
        height: 720,
        frame_rate: s.frame_rate,
        pixel_aspect: Default::default(),
        field_order: Default::default(),
        spherical: false,
        full_range: false,
        reading: Default::default(),
    });
    m.audio = audio.then_some(AudioStreamInfo {
        sample_rate: s.sample_rate,
        channels: 2,
    });
    m
}

/// A project with most of what a `.veg` holds.
fn project() -> Project {
    let mut p = Project::new(ProjectSettings::default());
    p.tracks.clear();
    let clip = p.add_media(file("C:\\media\\clip.mp4", true, true, 20));
    let tone = p.add_media(file("C:\\media\\tone.wav", false, true, 10));
    let take2 = p.add_media(file("C:\\media\\take two.wav", false, true, 10));
    let v = p.add_track(TrackKind::Video);
    let a = p.add_track(TrackKind::Audio);
    let group = GroupId(p.alloc_id());
    {
        let t = p.track_mut(v).unwrap();
        t.name = "Picture".into();
        t.level = 0.5;
        t.composite_mode = CompositeMode::Add;
        let mut c = Clip::new(
            ClipId(9001),
            clip,
            Tick::from_seconds(1),
            Tick::from_seconds(2),
        );
        c.source_in = ms(250);
        c.fade_in = ms(500);
        c.fade_in_curve = FadeCurve::Smooth;
        c.level = 0.75;
        c.group = Some(group);
        t.clips.push(c);
    }
    {
        let t = p.track_mut(a).unwrap();
        t.name = "Sound".into();
        t.muted = true;
        t.level = 0.5;
        t.pan = -0.5;
        let mut c = Clip::new(
            ClipId(9002),
            tone,
            Tick::from_seconds(1),
            Tick::from_seconds(2),
        );
        c.fade_out = ms(750);
        c.fade_out_curve = FadeCurve::Slow;
        c.fade_in_curve = FadeCurve::Fast;
        c.rate = 2.0;
        c.pitch = 3.0;
        c.level = 0.25;
        c.muted = true;
        c.group = Some(group);
        c.takes = vec![
            crate::takes::Take {
                media: tone,
                offset: Tick::ZERO,
                name: Some("First".into()),
                none: false,
            },
            crate::takes::Take {
                media: take2,
                offset: Tick::from_seconds(1),
                name: None,
                none: false,
            },
        ];
        t.clips.push(c);
        let mut env = Envelope::new(EnvelopeKind::Volume);
        env.points = vec![
            EnvelopePoint {
                at: Tick::ZERO,
                value: 1.0,
                curve: KeyCurve::Smooth,
            },
            EnvelopePoint {
                at: Tick::from_seconds(2),
                value: 0.5,
                curve: KeyCurve::Fast,
            },
        ];
        t.envelopes.push(env);
    }
    let m = p.add_marker(ms(1500));
    p.markers.iter_mut().find(|k| k.id == m).unwrap().name = "Mark".into();
    let r = p.add_region(TimeRange::new(Tick::from_seconds(2), Tick::from_seconds(1)));
    p.regions.iter_mut().find(|k| k.id == r).unwrap().name = "Region".into();
    let id = crate::model::MarkerId(p.alloc_id());
    p.commands.push(crate::commands::CommandMarker {
        id,
        time: Tick::from_seconds(3),
        command: "Text".into(),
        parameter: "Hello".into(),
        comment: "A note".into(),
    });
    p.summary = Summary {
        title: "Title".into(),
        artist: "Artist".into(),
        engineer: "Engineer".into(),
        copyright: "Copyright".into(),
        comments: "Comments".into(),
    };
    p.add_bus();
    let bin = crate::bins::BinId(p.alloc_id());
    p.bins.push(crate::bins::MediaBin {
        id: bin,
        name: "Bin".into(),
        parent: None,
        media: vec![tone],
    });
    p
}

fn options() -> WriteOptions<'static> {
    WriteOptions {
        path: "C:\\projects\\test.veg".into(),
        documents: "C:\\Users\\someone\\Documents".into(),
        effect: &|_| None,
    }
}

#[test]
fn a_project_comes_back_from_a_veg() {
    let p = project();
    let bytes = write(&p, &options());
    let doc = read(&bytes).expect("it reads");
    assert_eq!(doc.version, VERSION);
    assert_eq!(doc.media.len(), 3);
    assert_eq!(doc.media[0].path, "C:\\media\\clip.mp4");
    let found: Vec<Option<MediaInfo>> = p.media.iter().cloned().map(Some).collect();
    let back = build(&doc, &found, &ProjectSettings::default(), &|_, _| None).project;
    assert_eq!(back.tracks.len(), 2);
    let (v, a) = (&back.tracks[0], &back.tracks[1]);
    assert_eq!((v.kind, v.name.as_str()), (TrackKind::Video, "Picture"));
    assert_eq!((v.level, v.composite_mode), (0.5, CompositeMode::Add));
    assert_eq!(
        (a.kind, a.name.as_str(), a.muted),
        (TrackKind::Audio, "Sound", true)
    );
    assert_eq!((a.level, a.pan), (0.5, -0.5));
    let c = &v.clips[0];
    assert_eq!(
        (c.start, c.duration),
        (Tick::from_seconds(1), Tick::from_seconds(2))
    );
    assert_eq!(c.source_in, ms(250));
    assert_eq!((c.fade_in, c.fade_in_curve), (ms(500), FadeCurve::Smooth));
    // The events' levels: a picture's opacity, a sound's volume.
    assert_eq!((c.level, a.clips[0].level), (0.75, 0.25));
    let d = &a.clips[0];
    assert!(d.muted);
    assert_eq!(d.rate, 2.0);
    assert_eq!(d.pitch, 3.0);
    assert_eq!((d.fade_out, d.fade_out_curve), (ms(750), FadeCurve::Slow));
    assert_eq!(d.fade_in_curve, FadeCurve::Fast);
    // Its takes: both, the first active, its name kept.
    assert_eq!(d.takes.len(), 2);
    assert_eq!(d.takes[0].name.as_deref(), Some("First"));
    assert_eq!(d.takes[1].offset, Tick::from_seconds(1));
    // The two events grouped again.
    assert!(c.group.is_some() && c.group == d.group);
    // The volume envelope's points and curves.
    let env = &a.envelopes[0];
    assert_eq!(env.kind, EnvelopeKind::Volume);
    assert_eq!(env.points.len(), 2);
    assert_eq!(
        (env.points[1].at, env.points[1].value),
        (Tick::from_seconds(2), 0.5)
    );
    assert_eq!(env.points[1].curve, KeyCurve::Fast);
    // Markers, regions, commands, summary, busses, bins.
    assert_eq!(back.markers[0].name, "Mark");
    assert_eq!(back.markers[0].time, ms(1500));
    assert_eq!(back.regions[0].name, "Region");
    assert_eq!(back.regions[0].range.duration, Tick::from_seconds(1));
    // The file spells Text as TEXT; BananaV's list as Text.
    assert_eq!(doc.commands[0].command, "TEXT");
    assert_eq!(back.commands[0].command, "Text");
    assert_eq!(back.commands[0].parameter, "Hello");
    assert_eq!(back.commands[0].comment, "A note");
    assert_eq!(back.summary, p.summary);
    assert_eq!(back.busses.len(), 1);
    assert_eq!(back.bins[0].name, "Bin");
    assert_eq!(back.bins[0].media.len(), 1);
    back.check_invariants().expect("a sound project");
}

#[test]
fn effects_keep_parameters_of_every_size() {
    // Records as the files have them: the value's size, the name's, the
    // name, the value and 4 bytes.
    let record = |size: u32, name: &str, value: &[u8]| {
        let n: Vec<u8> = name
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        let mut r = size.to_le_bytes().to_vec();
        r.extend_from_slice(&(n.len() as u32).to_le_bytes());
        r.extend_from_slice(&n);
        r.extend_from_slice(value);
        r
    };
    let numbers = |v: &[f64]| -> Vec<u8> {
        v.iter()
            .flat_map(|x| x.to_le_bytes())
            .chain([0; 4])
            .collect()
    };
    let mut b = Vec::new();
    b.extend(record(0x14, "Mid", &numbers(&[103.5, 0.5])));
    // A text: passed over.
    b.extend(record(6, "Label", b"abc\0\0\0"));
    b.extend(record(
        8,
        "Mode",
        &[&(-2i32).to_le_bytes()[..], &[0; 4]].concat(),
    ));
    b.extend(record(12, "Gain", &numbers(&[1.5])));
    b.extend(record(0x24, "Tint", &numbers(&[0.1, 0.2, 0.3, 1.0])));
    // Not a name: the end.
    b.extend(record(12, "\u{1}", &numbers(&[9.0])));
    let p = read::params_at(&b, 0, 6);
    assert_eq!(
        p,
        [
            ("Mid".to_string(), ParamValue::Numbers(vec![103.5, 0.5])),
            ("Mode".to_string(), ParamValue::Integer(-2)),
            ("Gain".to_string(), ParamValue::Number(1.5)),
            (
                "Tint".to_string(),
                ParamValue::Numbers(vec![0.1, 0.2, 0.3, 1.0])
            ),
        ]
    );
    // And written as they were read.
    let mut p2 = project();
    let e = crate::fx::Effect::plugin(crate::fx::PluginFx {
        id: "com.vegascreativesoftware:colorcorrector".into(),
        version: [1, 0],
        name: String::new(),
        params: Default::default(),
    });
    p2.tracks[0].effects.push(e);
    let doc_fx = DocEffect {
        plugin: "com.vegascreativesoftware:colorcorrector".into(),
        preset: String::new(),
        params: p.clone(),
        ..Default::default()
    };
    let o = WriteOptions {
        effect: &|_| Some(doc_fx.clone()),
        ..options()
    };
    let back = read(&write(&p2, &o)).expect("it reads");
    assert_eq!(back.tracks[0].effects[0].params, p);
}

#[test]
fn times_go_to_hundreds_of_nanoseconds_and_back() {
    for t in [Tick::ZERO, ms(250), Tick::from_seconds(3600)] {
        assert_eq!(ticks(units(t)), t);
    }
    assert_eq!(units(Tick::from_seconds(1)), UNITS);
}

#[test]
fn what_is_not_a_project_is_refused() {
    assert_eq!(read(b"RIFF....WAVEfmt "), Err(VegError::NotRiff));
    let mut w = riff::Writer::new(riff::Guid::parse("00000000-0000-0000-0000-000000000001"));
    w.block(HEADER, &[0; 8]);
    assert_eq!(read(&w.finish()), Err(VegError::NotProject));
}

/// The chunks of a file as paths of their GUIDs (lists by their types).
fn paths(file: &[u8]) -> Vec<String> {
    fn walk(c: &riff::Chunk<'_>, path: &str, out: &mut Vec<String>) {
        for k in &c.kids {
            let name = format!("{}{:?}", if k.list { "L:" } else { "" }, k.guid);
            let p = format!("{path}/{}", &name[..name.len().min(10)]);
            out.push(p.clone());
            walk(k, &p, out);
        }
    }
    let root = riff::parse(file).expect("a riff");
    let mut out = Vec::new();
    walk(&root, "", &mut out);
    out
}

/// VEGAS Pro 23's own sample projects (not in the repository:
/// `BV_VEG_SAMPLES`, the folder of `00_vazio.veg` … `39_dois_takes.veg`,
/// each a new project with one thing changed): each read as the sample
/// says, made a project, and written back with the same chunks as VEGAS's
/// but the audio tracks' effect chains, which BananaV leaves out.
#[test]
#[ignore = "needs VEGAS's sample projects"]
fn vegas_samples_read_and_write_back() {
    let dir = std::path::PathBuf::from(std::env::var("BV_VEG_SAMPLES").expect("BV_VEG_SAMPLES"));
    let open = |name: &str| {
        let bytes = std::fs::read(dir.join(format!("{name}.veg"))).expect(name);
        let doc = read(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        (bytes, doc)
    };
    let s = |t: f64| (t * UNITS as f64).round() as i64;
    let (_, d) = open("02_video_1280x720");
    assert_eq!((d.settings.width, d.settings.height), (1280, 720));
    let (_, d) = open("03_video_25fps");
    assert_eq!((d.settings.frame_rate, d.settings.timecode), (25.0, 9));
    assert_eq!(open("04_video_32bits").1.settings.pixel_format, 32);
    assert_eq!(open("05_audio_44100").1.settings.sample_rate, 44_100);
    let (_, d) = open("06_resumo");
    assert!(d.summary.iter().all(|t| !t.is_empty()), "{:?}", d.summary);
    let (_, d) = open("07_regua");
    assert_eq!(
        (d.settings.beats_per_minute, d.settings.beats_per_measure),
        (90.0, 3)
    );
    let (_, d) = open("10_duas_trilhas");
    let names: Vec<_> = d.tracks.iter().map(|t| (t.kind, t.name.as_str())).collect();
    assert_eq!(
        names,
        [(VIDEO, "TRILHA_VIDEO_1"), (AUDIO, "TRILHA_AUDIO_1")]
    );
    let (_, d) = open("13_evento_video");
    let e = &d.tracks[0].events[0];
    assert_eq!((e.start, e.length), (s(1.0), s(2.0)));
    assert_eq!(e.curves, [4, 4]);
    let (_, d) = open("17_take_offset");
    assert_eq!(d.tracks[0].events[0].takes[0].offset, s(0.25));
    let (_, d) = open("18_nomes");
    let e = &d.tracks[0].events[0];
    assert_eq!(
        (e.name.as_str(), e.takes[0].name.as_str()),
        ("EVENTO_XYZ", "TAKE_XYZ")
    );
    let (_, d) = open("19_fades");
    let e = &d.tracks[0].events[0];
    assert_eq!(
        (e.fade_in, e.fade_out, e.curves),
        (s(0.5), s(0.75), [2, -2])
    );
    assert!(open("20_evento_mudo").1.tracks[0].events[0].muted);
    assert_eq!(open("21_velocidade").1.tracks[0].events[0].speed, 2.0);
    // A new audio track's three effects, each BananaV's own.
    let (_, d) = open("09_trilha_audio");
    let chain = &d.tracks[0].audio_fx;
    assert_eq!(chain.len(), 3);
    assert!(
        chain
            .iter()
            .all(|x| audio::effect(x.class, &x.settings).is_some())
    );
    let (_, d) = open("22_volume_pan");
    assert_eq!((d.tracks[0].volume, d.tracks[0].pan), (0.5, -0.5));
    assert!(open("23_mute_solo").1.tracks[0].muted);
    let (_, d) = open("24_marcador");
    assert_eq!(
        (d.markers[0].time, d.markers[0].label.as_str()),
        (s(1.5), "MARCADOR_1")
    );
    let (_, d) = open("25_regiao");
    let r = &d.markers[0];
    assert!(r.region && r.time == s(2.0) && r.length == s(1.0) && r.label == "REGIAO_1");
    let (_, d) = open("26_comando");
    let c = &d.commands[0];
    assert_eq!(
        (c.command.as_str(), c.parameter.as_str(), c.comment.as_str()),
        ("TEXT", "PARAMETRO_CMD", "COMENTARIO_CMD")
    );
    let (_, d) = open("27_envelope_volume");
    let env = &d.tracks[0].envelopes[0];
    assert_eq!(env.kind, 0);
    assert_eq!(env.points, [(0, 1.0, 4), (s(2.0), 0.5, 4)]);
    let (_, d) = open("28_fx_trilha_video");
    assert_eq!(
        d.tracks[0].effects[0].plugin,
        "com.vegascreativesoftware:sepia"
    );
    assert_eq!(d.tracks[0].effects[0].params.len(), 3);
    let (_, d) = open("29_fx_evento_video");
    let fx = &d.tracks[0].events[0].effects[0];
    assert_eq!(fx.plugin, "com.vegascreativesoftware:brightnessandcontrast");
    assert_eq!(
        fx.params[0],
        ("Brightness".to_string(), ParamValue::Number(0.25))
    );
    assert_eq!(open("30_barramento").1.busses, 1);
    let (_, d) = open("31_bin");
    assert_eq!(
        (d.bins[0].name.as_str(), d.bins[0].media.len()),
        ("BIN_XYZ", 1)
    );
    let (_, d) = open("33_composicao");
    assert_eq!(
        (d.tracks[0].composite_mode, d.tracks[0].composite_level),
        (1, 0.5)
    );
    let (_, d) = open("36_grupo");
    assert_eq!(
        d.groups,
        [vec![0x3000_0000_0000_0001, 0x2000_0000_0000_0002]]
    );
    assert_eq!(open("37_tom").1.tracks[0].events[0].pitch, 3.0);
    let (_, d) = open("38_faixa_cd");
    assert_eq!(d.cd.len(), 2);
    assert!(!d.cd[0].index && d.cd[1].index);
    let (_, d) = open("39_dois_takes");
    let e = &d.tracks[0].events[0];
    assert_eq!(
        (e.active, e.takes.len(), e.takes[1].name.as_str()),
        (1, 2, "TAKE_2")
    );

    // Every sample made a project and written back: VEGAS's chunks, but
    // what BananaV leaves out.
    let left_out = [
        "L:F79298F4",
        "L:B66A95CE",
        "884BACFF",
        "L:B5707E20",
        // The progress of a transition at an event's end.
        "L:EF72046C",
    ];
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .expect("the samples")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".veg"))
        .collect();
    names.sort();
    // Effects kept as the plug-ins they are, both ways.
    let keep_fx = |e: &DocEffect, _: bv_core::FrameRate| {
        let params = e
            .params
            .iter()
            .map(|(n, v)| {
                // Whole numbers ride as text, to come back as they went.
                let value = match v {
                    ParamValue::Number(x) => crate::fx::PluginValue::Num(vec![*x]),
                    ParamValue::Integer(x) => crate::fx::PluginValue::Text(x.to_string()),
                    ParamValue::Numbers(v) => crate::fx::PluginValue::Num(v.clone()),
                    ParamValue::Text(t) => crate::fx::PluginValue::Text(t.clone()),
                };
                (
                    n.clone(),
                    crate::fx::PluginParam {
                        value,
                        keys: Vec::new(),
                    },
                )
            })
            .collect();
        Some(crate::fx::Effect::plugin(crate::fx::PluginFx {
            id: e.plugin.clone(),
            version: [1, 0],
            name: e.preset.clone(),
            params,
        }))
    };
    let back_fx = |e: &crate::fx::Effect| {
        let fx = e.plugin.as_ref()?;
        let params = fx
            .params
            .iter()
            .filter_map(|(n, p)| {
                let value = match &p.value {
                    crate::fx::PluginValue::Text(x) => match x.parse() {
                        Ok(whole) => ParamValue::Integer(whole),
                        Err(_) => ParamValue::Text(x.clone()),
                    },
                    crate::fx::PluginValue::Num(v) => match v.as_slice() {
                        [x] => ParamValue::Number(*x),
                        v => ParamValue::Numbers(v.to_vec()),
                    },
                    _ => return None,
                };
                Some((n.clone(), value))
            })
            .collect();
        Some(DocEffect {
            plugin: fx.id.clone(),
            preset: fx.name.clone(),
            params,
            ..Default::default()
        })
    };
    let mut report = Vec::new();
    for name in &names {
        let (bytes, doc) = open(name.trim_end_matches(".veg"));
        let found = vec![None; doc.media.len()];
        let made = build(&doc, &found, &ProjectSettings::default(), &keep_fx);
        made.project
            .check_invariants()
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let options = WriteOptions {
            effect: &back_fx,
            ..options()
        };
        let ours = write(&made.project, &options);
        // The structures BananaV writes as VEGAS does, byte for byte.
        let blocks = |file: &[u8], guid: riff::Guid| -> Vec<Vec<u8>> {
            fn all<'a>(c: &riff::Chunk<'a>, g: riff::Guid, out: &mut Vec<&'a [u8]>) {
                for k in &c.kids {
                    if !k.list && k.guid == g {
                        out.push(k.data);
                    }
                    all(k, g, out);
                }
            }
            let root = riff::parse(file).expect("a riff");
            let mut out = Vec::new();
            all(&root, guid, &mut out);
            out.into_iter().map(<[u8]>::to_vec).collect()
        };
        for (what, g) in [
            ("mixer", MIXER_SETTINGS),
            ("audio track", AUDIO_TRACK),
            ("video track", VIDEO_TRACK),
            ("video event", VIDEO_EVENT),
            ("audio event", AUDIO_EVENT),
            ("four numbers", FOUR),
            ("summary", SUMMARY),
            ("envelopes' header", ENVELOPES_HEADER),
            ("envelope", ENVELOPE),
            ("group", GROUP),
            ("marker", MARKER),
        ] {
            let (a, b) = (blocks(&bytes, g), blocks(&ours, g));
            for (k, (x, y)) in a.iter().zip(&b).enumerate() {
                // A CD index's name: BananaV's indices have none.
                let cd_index = g == MARKER && x.first() == Some(&0x30);
                // What is not written: the chains' place in their owners,
                // the transitions at events' ends (and what the plug-ins'
                // settings are in another order), a track's pan up and down.
                // A point keeps only its start: after it, what its memory had.
                let ignored = |i: usize| match what {
                    "audio track" => (0x304..0x308).contains(&i) || (0x14..0x1C).contains(&i),
                    "audio event" => (0x24..0x28).contains(&i),
                    "video event" => i >= 0x90 || (0x74..0x78).contains(&i),
                    "envelope" => i >= 0x4C && (i - 0x4C) % 0x38 >= 0x10,
                    _ => false,
                };
                if x != y && !cd_index {
                    let at: Vec<String> = (0..x.len().max(y.len()))
                        .filter(|&i| x.get(i) != y.get(i) && !ignored(i))
                        .take(12)
                        .map(|i| format!("{i:#x}"))
                        .collect();
                    if at.is_empty() {
                        continue;
                    }
                    report.push(format!("{name}: {what} #{k} differs at {}", at.join(", ")));
                }
            }
        }
        // Track Motion: BananaV keeps its numbers shorter, without a 3D
        // box's depth.
        let (a, b) = (blocks(&bytes, TRACK_MOTION), blocks(&ours, TRACK_MOTION));
        assert_eq!(a.len(), b.len(), "{name}: track motions");
        for (x, y) in a.iter().zip(&b) {
            let (x, y) = (read::motion(x), read::motion(y));
            assert_eq!(x.flags, y.flags, "{name}: track motion's switches");
            for (l, m) in [
                (&x.moves, &y.moves),
                (&x.shadow, &y.shadow),
                (&x.glow, &y.glow),
            ] {
                assert_eq!(l.len(), m.len(), "{name}: track motion's keys");
                for (p, q) in l.iter().zip(m.iter()) {
                    assert_eq!((p.curve, p.at), (q.curve, q.at), "{name}: track motion");
                    let close = |u: f64, v: f64| (u - v).abs() < 1e-5;
                    let all = [
                        (p.position[0], q.position[0]),
                        (p.position[1], q.position[1]),
                        (p.position[2], q.position[2]),
                        (p.size[0], q.size[0]),
                        (p.size[1], q.size[1]),
                        (p.rotation[0], q.rotation[0]),
                        (p.rotation[1], q.rotation[1]),
                        (p.rotation[2], q.rotation[2]),
                        (p.smoothness, q.smoothness),
                        (p.blur, q.blur),
                        (p.intensity, q.intensity),
                    ];
                    assert!(
                        all.iter().all(|&(u, v)| close(u, v)),
                        "{name}: track motion {p:?} {q:?}"
                    );
                    assert_eq!(p.color, q.color, "{name}: a halo's color");
                }
            }
        }
        if std::env::var_os("BV_VEG_SHOW").is_some() {
            for (what, g) in [
                ("header", HEADER),
                ("media", MEDIA),
                ("track header", TRACK_HEADER),
                ("event header", EVENT_HEADER),
                ("take", TAKE),
                ("bus", BUS),
                ("tree data", TREE_DATA),
            ] {
                let (a, b) = (blocks(&bytes, g), blocks(&ours, g));
                for (k, (x, y)) in a.iter().zip(&b).enumerate() {
                    let at: Vec<String> = (0..x.len().max(y.len()))
                        .filter(|&i| x.get(i) != y.get(i))
                        .take(16)
                        .map(|i| format!("{i:#x}"))
                        .collect();
                    if !at.is_empty() {
                        eprintln!(
                            "  {name}: {what} #{k} ({} vs {} B): {}",
                            x.len(),
                            y.len(),
                            at.join(", ")
                        );
                    }
                }
            }
        }
        let read_back = read(&ours).unwrap_or_else(|e| panic!("{name} written: {e}"));
        // The effects come back as they went (their parameters by name).
        let fx = |d: &Document| -> Vec<(String, Vec<(String, ParamValue)>)> {
            d.tracks
                .iter()
                .flat_map(|t| {
                    t.effects
                        .iter()
                        .chain(t.events.iter().flat_map(|e| &e.effects))
                })
                .map(|e| {
                    let mut p = e.params.clone();
                    p.sort_by(|a, b| a.0.cmp(&b.0));
                    (e.plugin.clone(), p)
                })
                .collect()
        };
        assert_eq!(fx(&read_back), fx(&doc), "{name}");
        assert_eq!(read_back.tracks.len(), doc.tracks.len(), "{name}");
        let keep = |p: &String| !left_out.iter().any(|l| p.contains(l));
        let theirs: Vec<String> = paths(&bytes).into_iter().filter(keep).collect();
        let mine: Vec<String> = paths(&ours).into_iter().filter(keep).collect();
        if theirs != mine {
            let only_theirs: Vec<_> = theirs.iter().filter(|p| !mine.contains(p)).collect();
            let only_mine: Vec<_> = mine.iter().filter(|p| !theirs.contains(p)).collect();
            report.push(format!(
                "{name}: VEGAS's only {only_theirs:?}; ours only {only_mine:?}"
            ));
        }
    }
    eprintln!("{}", report.join("\n"));
    assert!(report.is_empty(), "{} samples differ", report.len());
}

/// An audio effects chain as the files hold one: its header, then each
/// plug-in's class, name and settings (a format's GUID, then `fields`).
fn chain(fx: &[(&str, &str, Vec<u8>)]) -> Vec<u8> {
    let text = |s: &str| -> Vec<u8> {
        s.encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    };
    let guid = |s: &str| riff::Guid::parse(s).0.to_vec();
    let u32s = |v: &[u32]| -> Vec<u8> { v.iter().flat_map(|x| x.to_le_bytes()).collect() };
    let mut b = guid("B4128601-702E-11D1-8253-00009290DD44");
    b.extend(u32s(&[3]));
    b.extend(guid("3873D131-2D0F-11D3-87C0-00C04F8EF35F"));
    let first = text("Track Compressor");
    b.extend(u32s(&[first.len() as u32]));
    b.extend(first);
    b.extend(u32s(&[1_000_000, 0]));
    // The sound's format: float, stereo, 48 kHz.
    b.extend([3, 0, 2, 0]);
    b.extend(u32s(&[48_000, 384_000]));
    b.extend([8, 0, 32, 0, 0, 0]);
    b.extend(u32s(&[0, fx.len() as u32, 32]));
    for (class, name, fields) in fx {
        let n = text(name);
        b.extend(guid(class));
        b.extend(u32s(&[0, n.len() as u32]));
        b.extend(n);
        b.extend(guid(class));
        let size = 16 + fields.len() as u32;
        b.extend(u32s(&[size, size]));
        b.extend(guid("C400F99B-0C6E-444C-8F20-98DB216BE56E"));
        b.extend(fields);
    }
    b.extend([0; 6]);
    b
}

#[test]
fn audio_chains_bring_their_effects() {
    use crate::fx::EffectKind;
    let ints = |v: &[i32]| -> Vec<u8> { v.iter().flat_map(|x| x.to_le_bytes()).collect() };
    const GATE: &str = "869419DD-501F-11D3-8CDC-00C04F6B8E4C";
    const EQ: &str = "8CB69A0A-10E8-11D2-9B89-00104B8D13C2";
    const COMPRESSOR: &str = "23C9F225-40EC-11D2-9D36-00C04F8EDC1E";
    // A new track's three, the EQ's third band down 6 dB, the compressor
    // at 4:1 from -12 dB, and a plug-in BananaV does not have.
    let mut eq = vec![
        0, 0, -60, 0, 10, 10, 10, 10, 100, 300, 1000, 5000, 1, 2, 2, 3,
    ];
    eq.extend([2, 1, 1, 2, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, -1]);
    let mut compressor: Vec<u8> = [0.0f32, 0.0, -12.0, 4.0, 15.0, 250.0]
        .iter()
        .flat_map(|x| x.to_le_bytes())
        .collect();
    compressor.extend(ints(&[1, 0, 0, 0, -1]));
    let b = chain(&[
        (GATE, "Track Noise Gate", ints(&[3, 100, -800, 0, -1])),
        (EQ, "Track EQ", ints(&eq)),
        (COMPRESSOR, "Track Compressor", compressor),
        (
            "00000000-1111-2222-3333-444444444444",
            "Elsewhere",
            ints(&[1]),
        ),
    ]);
    let fx = read::audio_chain(&b).expect("it reads");
    assert_eq!(fx.len(), 4);
    assert_eq!(fx[1].name, "Track EQ");
    let effect = |k: usize| audio::effect(fx[k].class, &fx[k].settings);
    let gate = effect(0).expect("the gate");
    assert_eq!(gate.kind, EffectKind::TrackNoiseGate);
    assert_eq!(gate.params, [-80.0, 3.0, 100.0]);
    assert!(gate.enabled);
    let eq = effect(1).expect("the EQ");
    assert_eq!(eq.kind, EffectKind::TrackEq);
    use crate::audio_fx::track_eq::{BANDWIDTH, FREQUENCY, GAIN, MODE, PER_BAND, ROLLOFF};
    let band = |b: usize, p: usize| eq.params[b * PER_BAND + p];
    assert_eq!(
        [0, 1, 2, 3].map(|b| band(b, FREQUENCY)),
        [100.0, 300.0, 1000.0, 5000.0]
    );
    // Low shelf, two bands, high shelf; 12 dB/oct. shelves.
    assert_eq!([0, 1, 2, 3].map(|b| band(b, MODE)), [0.0, 1.0, 1.0, 2.0]);
    assert_eq!((band(0, ROLLOFF), band(3, ROLLOFF)), (1.0, 1.0));
    assert_eq!((band(2, GAIN), band(1, BANDWIDTH)), (-6.0, 1.0));
    let c = effect(2).expect("the compressor");
    use crate::audio_fx::track_compressor::{AUTO_GAIN, RATIO, SMOOTH, THRESHOLD};
    assert_eq!(
        (
            c.params[THRESHOLD],
            c.params[RATIO],
            c.params[AUTO_GAIN],
            c.params[SMOOTH]
        ),
        (-12.0, 4.0, 1.0, 0.0)
    );
    assert!(effect(3).is_none(), "not BananaV's");
    // A bypassed gate comes in off; a chain cut short does not read.
    let off = chain(&[(GATE, "Track Noise Gate", ints(&[3, 100, -800, 1, -1]))]);
    let fx = read::audio_chain(&off).unwrap();
    assert!(!audio::effect(fx[0].class, &fx[0].settings).unwrap().enabled);
    assert!(read::audio_chain(&b[..b.len() / 2]).is_none());
}

/// The plug-ins of the audio samples (`BV_VEG_SAMPLES2`, the folder of
/// `a01_…veg` … `a51_…veg` and their `log.txt`, each a track with one
/// plug-in at its last preset): each read as BananaV's factory preset of the
/// name the log gives, or its default when the log gives none.
#[test]
#[ignore = "needs VEGAS's sample projects"]
fn audio_samples_read_as_their_presets() {
    use crate::fx::{Effect, EffectKind};
    let dir = std::path::PathBuf::from(std::env::var("BV_VEG_SAMPLES2").expect("BV_VEG_SAMPLES2"));
    let log = std::fs::read(dir.join("log.txt")).expect("log");
    let log = String::from_utf8_lossy(&log).into_owned();
    let chosen = |case: &str| -> String {
        let at = log.find(&format!("{case}: ")).expect(case);
        log[at..]
            .lines()
            .find_map(|l| l.trim().strip_prefix("escolhido: "))
            .unwrap_or("-")
            .trim_start_matches("[Sys] ")
            .to_string()
    };
    // A graph's count and first point: the points past the count are not kept.
    let graphs = |k: EffectKind| -> &[(usize, usize)] {
        match k {
            EffectKind::Vibrato | EffectKind::AmplitudeModulation => &[(3, 32)],
            EffectKind::GraphicDynamics => &[(0, 16)],
            EffectKind::Distortion => &[(2, 32), (67, 32)],
            EffectKind::GraphicEq => &[(0, 32)],
            _ => &[],
        }
    };
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .expect("folder")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with('a') && n.ends_with(".veg"))
        .collect();
    names.sort();
    let (mut checked, mut wrong, mut no_row) = (0, Vec::new(), Vec::new());
    for name in &names {
        let case = name.trim_end_matches(".veg");
        let bytes = std::fs::read(dir.join(name)).expect(name);
        let doc = read(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let x = doc
            .tracks
            .iter()
            .rev()
            .find_map(|t| t.audio_fx.last())
            .unwrap_or_else(|| panic!("{name}: no chain"));
        let e = super::audio::effect(x.class, &x.settings)
            .unwrap_or_else(|| panic!("{name}: {} left out", x.name));
        let preset = chosen(case);
        let row = crate::audio_fx::presets(e.kind)
            .unwrap_or(&[])
            .iter()
            .find(|(n, _)| *n == preset || (preset == "-" && *n == "Default"))
            .map(|(_, r)| r.to_vec())
            .or_else(|| (preset == "-").then(|| Effect::new(e.kind).params.clone()));
        let Some(row) = row else {
            no_row.push(format!("{case}: {preset}: {:?}", e.params));
            continue;
        };
        let mut left_out = std::collections::BTreeSet::new();
        for &(at, room) in graphs(e.kind) {
            let n = row[at].round() as usize;
            left_out.extend((at + 1 + 2 * n)..(at + 1 + 2 * room));
        }
        checked += 1;
        for (i, (&a, &b)) in e.params.iter().zip(&row).enumerate() {
            if left_out.contains(&i) {
                continue;
            }
            // The plug-in itself goes no lower than −15 dB of the preset's −25.
            if e.kind == EffectKind::TrackEq && i == 3 && a == -15.0 {
                continue;
            }
            // A band the preset leaves off has no settings of its own.
            if e.kind == EffectKind::MultiBandDynamics
                && i >= 2
                && row[2 + (i - 2) / 10 * 10] == 1.0
            {
                continue;
            }
            if (a - b).abs() > 0.02 + 0.005 * b.abs() {
                wrong.push(format!(
                    "{case}: {preset}: {i} {}: read {a}, the preset {b}",
                    e.kind.params()[i].name
                ));
            }
        }
    }
    eprintln!("{checked} of {} checked", names.len());
    for line in &no_row {
        eprintln!("no row: {line}");
    }
    for line in &wrong {
        eprintln!("WRONG {line}");
    }
    assert!(wrong.is_empty(), "{} wrong", wrong.len());
}

/// What a project's editing leaves in the file, read from the samples
/// (`BV_VEG_SAMPLES`, the folder of `58_transicao_in.veg` … and of
/// `master_6db.veg` above it), each with one thing changed.
#[test]
#[ignore = "needs VEGAS's sample projects"]
fn edits_read_as_the_samples_say() {
    use crate::envelope::EnvelopeKind as K;
    let dir = std::path::PathBuf::from(std::env::var("BV_VEG_SAMPLES").expect("BV_VEG_SAMPLES"));
    let kept = |e: &DocEffect, _: bv_core::FrameRate| {
        let mut params = std::collections::BTreeMap::new();
        for (n, v) in &e.params {
            let value = match v {
                ParamValue::Number(x) => crate::fx::PluginValue::Num(vec![*x]),
                ParamValue::Integer(x) => crate::fx::PluginValue::Num(vec![*x as f64]),
                ParamValue::Numbers(v) => crate::fx::PluginValue::Num(v.clone()),
                ParamValue::Text(t) => crate::fx::PluginValue::Text(t.clone()),
            };
            params.insert(
                n.clone(),
                crate::fx::PluginParam {
                    value,
                    keys: Vec::new(),
                },
            );
        }
        Some(crate::fx::Effect::plugin(crate::fx::PluginFx {
            id: e.native.clone().unwrap_or_else(|| e.plugin.clone()),
            version: [1, 0],
            name: e.preset.clone(),
            params,
        }))
    };
    let doc = |name: &str| {
        let bytes = std::fs::read(dir.join(format!("{name}.veg"))).expect(name);
        read(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"))
    };
    let open = |name: &str| {
        let d = doc(name);
        let found = vec![None; d.media.len()];
        build(&d, &found, &ProjectSettings::default(), &kept)
    };
    let second = |t: f64| ticks((t * UNITS as f64).round() as i64);
    let near = |a: f32, b: f32, what: &str| assert!((a - b).abs() < 1e-3, "{what}: {a} is not {b}");

    // Transitions: at the fade in, its plug-in and its settings.
    let m = open("58_transicao_in");
    let c = m.project.clips().next().unwrap().1;
    assert!(c.transition_fx.is_some(), "58");
    assert_eq!(m.transitions_left, 0, "58");
    assert_eq!(open("59_transicao_out").transitions_left, 1, "59");
    let d = doc("62_transicao_parametro");
    let t = d
        .tracks
        .iter()
        .flat_map(|t| &t.events)
        .find_map(|e| e.transition_out.as_ref())
        .expect("62");
    assert!(t.transition, "62");
    let feather = t
        .params
        .iter()
        .find(|(n, _)| n == "Transition")
        .expect("Transition");
    assert_eq!(feather.1, ParamValue::Number(0.3), "62");
    // The crossfade: on the second event.
    let m = open("60_crossfade");
    let clips: Vec<_> = m.project.clips().map(|(_, c)| c).collect();
    assert_eq!(clips.len(), 2, "60");
    assert!(
        clips[0].transition_fx.is_none() && clips[1].transition_fx.is_some(),
        "60"
    );
    assert_eq!(clips[0].fade_out, second(1.0), "60");
    assert_eq!(clips[1].fade_in, second(1.0), "60");
    // Its progress: 0, then 0.25 half way, then 1, over the fade.
    let m = open("61_transicao_envelope");
    let c = m.project.clips().next().unwrap().1;
    let env = c.envelope(K::TransitionProgress).expect("61");
    assert_eq!(env.points.len(), 3, "61");
    near(env.points[1].value, 0.25, "61");
    assert_eq!(env.points[1].at, Tick(c.fade_in.0 / 2), "61");

    // Audio chains: in the event, in the track, on or off, the master's.
    let m = open("63_fx_evento_audio");
    let c = m.project.clips().next().unwrap().1;
    assert_eq!(c.effects.len(), 1, "63");
    assert_eq!(c.effects[0].kind, crate::fx::EffectKind::ReverbFx, "63");
    near(c.effects[0].params[2], -35.0, "63");
    let m = open("64_fx_trilha_audio");
    let kinds: Vec<_> = m.project.tracks[0].effects.iter().map(|e| e.kind).collect();
    assert_eq!(kinds.len(), 4, "64");
    assert_eq!(kinds[0], crate::fx::EffectKind::TrackNoiseGate, "64");
    assert_eq!(kinds[3], crate::fx::EffectKind::ReverbFx, "64");
    let m = open("66_fx_bypass");
    let on: Vec<_> = m.project.tracks[0]
        .effects
        .iter()
        .map(|e| e.enabled)
        .collect();
    assert_eq!(on, [false, true, true], "66");
    let m = open("68_fx_mestre");
    assert_eq!(m.project.master_fx.len(), 1, "68");
    assert_eq!(
        m.project.master_fx[0].kind,
        crate::fx::EffectKind::ReverbFx,
        "68"
    );
    assert_eq!(m.audio_fx_left, 0, "68");

    // An audio event's switches.
    let c = open("70_evento_normalize")
        .project
        .clips()
        .next()
        .unwrap()
        .1
        .clone();
    assert_eq!(c.normalize, Some(0.5), "70");
    assert!(
        open("71_evento_fase")
            .project
            .clips()
            .next()
            .unwrap()
            .1
            .invert_phase,
        "71"
    );
    assert_eq!(
        open("72_evento_canais")
            .project
            .clips()
            .next()
            .unwrap()
            .1
            .channels,
        crate::model::ChannelMap::Combine,
        "72"
    );
    near(
        open("56_fade_ganho_audio")
            .project
            .clips()
            .next()
            .unwrap()
            .1
            .level,
        0.75,
        "56",
    );

    // A track's phase, its envelopes by their kinds, their curves.
    assert!(open("73_trilha_fase").project.tracks[0].invert_phase, "73");
    let m = open("74_trilha_pan_env");
    let envs = &m.project.tracks[0].envelopes;
    assert_eq!(envs.len(), 1, "74");
    assert_eq!(envs[0].kind, K::Pan, "74");
    near(envs[0].points[1].value, -0.5, "74");
    let m = open("27_envelope_volume");
    assert_eq!(m.project.tracks[0].envelopes[0].kind, K::Volume, "27");
    let m = open("75_env_curvas");
    let curves: Vec<_> = m.project.tracks[0].envelopes[0]
        .points
        .iter()
        .map(|p| p.curve)
        .collect();
    assert_eq!(curves.len(), 7, "75");
    let m = open("93_env_composicao");
    assert_eq!(
        m.project.tracks[0].envelopes[0].kind,
        K::CompositeLevel,
        "93"
    );
    let m = open("77_send_bus");
    assert!(
        matches!(m.project.tracks[0].envelopes[0].kind, K::Send(_)),
        "77"
    );
    assert_eq!(m.envelopes_left, 0, "77");

    // An event's velocity.
    let m = open("92_env_velocidade");
    let c = m.project.clips().next().unwrap().1;
    let env = c.envelope(K::Velocity).expect("92");
    assert_eq!(env.points.len(), 2, "92");
    near(env.points[1].value, 2.0, "92");
    assert_eq!(env.points[1].at, second(1.0), "92");

    // The busses: faders, switches.
    let m = open("../master_6db");
    near(m.project.master.gain_db, -6.0, "master");
    let m = open("97_bus_so_solo");
    assert_eq!(m.project.busses.len(), 1, "97");
    assert!(
        m.project.busses[0].channel.solo && !m.project.busses[0].channel.muted,
        "97"
    );
    let m = open("98_bus_so_mudo");
    assert!(
        m.project.busses[0].channel.muted && !m.project.busses[0].channel.solo,
        "98"
    );

    // A video event's switches.
    assert!(
        open("50_evento_interlace")
            .project
            .clips()
            .next()
            .unwrap()
            .1
            .reduce_flicker,
        "50"
    );
    assert_eq!(
        open("51_evento_resample")
            .project
            .clips()
            .next()
            .unwrap()
            .1
            .resample,
        Some(crate::model::ResampleMode::Disable),
        "51"
    );
    near(
        open("52_evento_undersample")
            .project
            .clips()
            .next()
            .unwrap()
            .1
            .undersample,
        0.5,
        "52",
    );

    // An effect's keyframes: frames, interpolations and values.
    let d = doc("80_ofx_kf_double");
    let fx = d
        .tracks
        .iter()
        .flat_map(|t| &t.events)
        .flat_map(|e| &e.effects)
        .next()
        .expect("80");
    let (_, keys) = fx.keys.iter().find(|(n, _)| n == "Brightness").expect("80");
    let got: Vec<_> = keys
        .iter()
        .map(|k| (k.frame, k.interpolation, k.value[0]))
        .collect();
    assert_eq!(
        got,
        [
            (0.0, 1, 0.1),
            (15.0, 2, 0.2),
            (30.0, 3, 0.3),
            (45.0, 4, 0.4),
            (60.0, 6, 0.5)
        ],
        "80"
    );
    // Colors, points and choices too.
    let d = doc("81_ofx_kf_border");
    let fx = d
        .tracks
        .iter()
        .flat_map(|t| &t.events)
        .flat_map(|e| &e.effects)
        .next()
        .expect("81");
    let of = |name: &str| &fx.keys.iter().find(|(n, _)| n == name).expect(name).1;
    assert_eq!(of("color")[0].value.len(), 4, "81");
    assert_eq!(of("color_rgb")[0].value.len(), 3, "81");
    assert_eq!(of("Type")[0].value.len(), 1, "81");
    let d = doc("82_ofx_kf_bumpmap");
    let fx = d
        .tracks
        .iter()
        .flat_map(|t| &t.events)
        .flat_map(|e| &e.effects)
        .next()
        .expect("82");
    let of = |name: &str| &fx.keys.iter().find(|(n, _)| n == name).expect(name).1;
    assert_eq!(of("LocationSrc")[0].handles.len(), 3, "82");
    assert_eq!(of("LocationDst")[0].handles.len(), 2, "82");
    // The chain's order and switches.
    let d = doc("90_fx_antes_pancrop");
    let fx = d
        .tracks
        .iter()
        .flat_map(|t| &t.events)
        .flat_map(|e| &e.effects)
        .next()
        .expect("90");
    assert!(fx.before_pan_crop, "90");
    let c = open("90_fx_antes_pancrop")
        .project
        .clips()
        .next()
        .unwrap()
        .1
        .clone();
    assert_eq!(c.pan_crop_at, 1, "90");
    let c = open("91_fx_video_bypass")
        .project
        .clips()
        .next()
        .unwrap()
        .1
        .clone();
    assert!(!c.effects[0].enabled, "91");
    // A text.
    let d = doc("95_ofx_texto");
    let fx = d
        .tracks
        .iter()
        .flat_map(|t| &t.events)
        .flat_map(|e| &e.effects)
        .next()
        .expect("95");
    assert!(
        fx.params
            .iter()
            .any(|(_, v)| matches!(v, ParamValue::Text(_))),
        "95"
    );

    // Track Motion: where, how big, turned, over time.
    let m = open("83_tm_kf");
    let tm = m.project.tracks[0].motion.as_ref().expect("83");
    assert_eq!(tm.keys.len(), 2, "83");
    let k = &tm.keys[1];
    assert_eq!(k.at, second(1.0), "83");
    near(k.value.x, 10.0 / 640.0, "83");
    near(k.value.y, 20.0 / 360.0, "83");
    near(k.value.scale, 0.5, "83");
    near(k.value.stretch, 1.0, "83");
    near(k.value.rotation_deg, 0.5, "83");
    let m = open("85_tm_glow");
    let tm = m.project.tracks[0].motion.as_ref().expect("85");
    assert!(tm.glow.on && !tm.shadow.on, "85");
    assert_eq!(tm.glow.keys.len(), 2, "85");
    near(tm.glow.keys[1].value.intensity, 0.0075, "85");
    near(tm.glow.keys[1].value.color[0], 10.0 / 255.0, "85");
    let m = open("86_tm_sombra");
    let tm = m.project.tracks[0].motion.as_ref().expect("86");
    assert!(tm.shadow.on && !tm.glow.on, "86");
    let m = open("87_tm_3d");
    let t = &m.project.tracks[0];
    assert_eq!(t.composite_mode, CompositeMode::ThreeDSourceAlpha, "87");
    let tm = t.motion.as_ref().expect("87");
    assert_eq!(tm.space_keys.len(), 2, "87");
    near(tm.space_keys[1].value.z, 100.0 / 640.0, "87");
    near(tm.space_keys[1].value.rotation[1], 0.5, "87");
    let m = open("88_tm_pai");
    assert!(m.project.tracks[0].parent_motion.is_some(), "88");
}

/// The Pan/Crops of another program's projects (`BV_PANCROP_SAMPLES`, the
/// folder of `mascaras.veg` and `vegas/geometria.veg`): each written and read
/// again as it was, and as long as the file's.
#[test]
#[ignore = "needs VEGAS's sample projects"]
fn pan_crop_chunks_read_and_write_back() {
    let dir = std::path::PathBuf::from(std::env::var("BV_PANCROP_SAMPLES").expect("the folder"));
    let mut written = 0;
    for file in ["mascaras.veg", "vegas/geometria.veg"] {
        let bytes = std::fs::read(dir.join(file)).expect(file);
        let doc = read(&bytes).unwrap_or_else(|e| panic!("{file}: {e}"));
        let events = doc.tracks.iter().flat_map(|t| &t.events);
        for e in events {
            let Some(box_) = &e.pan_crop else {
                continue;
            };
            let chunk = pancrop::write(box_, e.mask.as_ref(), box_.size, &[])
                .unwrap_or_else(|| panic!("{file} {}: its chunk", e.start));
            let (again, mask) = pancrop::read(&chunk).expect("it reads");
            assert_eq!(&again, box_, "{file} {}", e.start);
            assert_eq!(mask, e.mask, "{file} {}", e.start);
            written += 1;
        }
    }
    eprintln!("{written} chunks");
    assert!(written >= 40, "{written} chunks");
}
