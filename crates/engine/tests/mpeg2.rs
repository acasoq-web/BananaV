//! MPEG-2 rendering: a DVD video stream (BananaV's MPEG-2 video and the
//! system's MPEG audio in BananaV's program stream) read back; and a Video
//! CD's MPEG-1 stream (skipped where the system has no MPEG audio
//! encoder).

use std::sync::Arc;
use std::time::{Duration, Instant};

use bv_core::{FrameRate, Tick, TimeRange};
use bv_engine::{
    AudioParams, Availability, Engine, FileFormat, RenderOptions, RenderSettings, VideoParams,
};
use bv_timeline::{Generator, MediaInfo, Project, ProjectSettings, TrackKind};

/// The mean difference of two frames' colors.
fn difference(a: &bv_core::ImageRgbaF32, b: &bv_core::ImageRgbaF32) -> f32 {
    a.data
        .iter()
        .zip(&b.data)
        .map(|(a, b)| (0..3).map(|c| (a[c] - b[c]).abs()).sum::<f32>() / 3.0)
        .sum::<f32>()
        / b.data.len() as f32
}

#[test]
fn a_video_cd_stream_renders_and_reads_back() {
    if FileFormat::Mpeg1.availability() != Availability::Ready {
        return;
    }
    let rate = FrameRate::new(30_000, 1001);
    // The Video CD's frame (a 4:3 project would be stretched to it).
    let mut p = Project::new(ProjectSettings {
        width: 352,
        height: 240,
        frame_rate: rate,
        ..ProjectSettings::default()
    });
    let track = |p: &Project, kind| p.tracks.iter().find(|t| t.kind == kind).map(|t| t.id);
    let (v, a) = (track(&p, TrackKind::Video), track(&p, TrackKind::Audio));
    let bars = p.add_media(MediaInfo::generated(Generator::ColorBars, &p.settings));
    let tone = p.add_media(MediaInfo::generated(
        Generator::TestTone {
            frequency_hz: 440.0,
            level_db: -12.0,
        },
        &p.settings,
    ));
    for (m, video, audio) in [(bars, v, None), (tone, None, a)] {
        let c = p.insert_media(m, Tick::ZERO, video, audio).unwrap()[0];
        p.set_clip_duration(c, Tick::from_seconds(2)).unwrap();
    }
    let dir = std::env::temp_dir().join(format!("bv-mpeg1-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("vcd.mpg");
    let mut engine = Engine::new(Arc::new(|| {}));
    engine.set_project(&p);
    engine.start_render(RenderSettings {
        format: FileFormat::Mpeg1,
        path: path.clone(),
        range: TimeRange::new(Tick::ZERO, Tick::from_seconds(2)),
        video: Some(VideoParams {
            width: 352,
            height: 240,
            frame_rate: rate,
            bitrate: 1_150_000,
            max_bitrate: 1_150_000,
            ..VideoParams::default()
        }),
        audio: Some(AudioParams {
            sample_rate: 44_100,
            channels: 2,
            bit_depth: 16,
            float: false,
            bitrate: 224_000,
        }),
        options: RenderOptions::default(),
        project_file: None,
    });
    let start = Instant::now();
    while !engine.render_progress().is_some_and(|p| p.finished) {
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "rendered in time"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let done = engine.render_progress().unwrap();
    assert!(done.error.is_none(), "{:?}", done.error);
    let bytes = std::fs::read(&path).unwrap();
    // Video CD's packs, MPEG-1's headers, no MPEG-2 extensions.
    assert_eq!((bytes.len() - 4) % 2324, 0, "whole packs");
    assert_eq!(bytes[4] & 0xF1, 0x21, "an MPEG-1 pack header");
    assert!(!bytes.windows(4).any(|w| w == [0, 0, 1, 0xB5]));
    let info = Engine::probe_file(&path.to_string_lossy()).unwrap();
    let video = info.video.expect("its video");
    assert_eq!((video.width, video.height), (352, 240));
    assert!(info.audio.is_some(), "its sound");
    // The system decodes the bars as the project shows them.
    let mut reader = bv_media::open_video(&info).unwrap();
    let mut pool = bv_render::VideoPool::new();
    let t = Tick::from_seconds(1);
    let decoded = reader.frame_at(t).unwrap();
    assert_eq!((decoded.width, decoded.height), (352, 240));
    let frame = rate.frame_at(t);
    let best = (frame - 1..=frame + 1)
        .map(|f| {
            let source = bv_render::render_output(&p, rate.frame_start(f), 352, 240, &mut pool);
            difference(&decoded, &source)
        })
        .fold(f32::MAX, f32::min);
    // BT.601 colours, as MPEG-1 decoders take them (BT.709 would be 0.03).
    assert!(best < 0.025, "mean difference {best}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_dvd_stream_renders_and_reads_back() {
    if FileFormat::Mpeg2.availability() != Availability::Ready {
        return;
    }
    let rate = FrameRate::new(30_000, 1001);
    let mut p = Project::new(ProjectSettings {
        width: 720,
        height: 480,
        frame_rate: rate,
        ..ProjectSettings::default()
    });
    let track = |p: &Project, kind| p.tracks.iter().find(|t| t.kind == kind).map(|t| t.id);
    let (v, a) = (track(&p, TrackKind::Video), track(&p, TrackKind::Audio));
    let bars = p.add_media(MediaInfo::generated(Generator::ColorBars, &p.settings));
    let tone = p.add_media(MediaInfo::generated(
        Generator::TestTone {
            frequency_hz: 440.0,
            level_db: -12.0,
        },
        &p.settings,
    ));
    for (m, video, audio) in [(bars, v, None), (tone, None, a)] {
        let c = p.insert_media(m, Tick::ZERO, video, audio).unwrap()[0];
        p.set_clip_duration(c, Tick::from_seconds(2)).unwrap();
        if video.is_some() {
            // The bars slide across: P-pictures carry motion.
            for (at, x) in [(0, 0.0), (2, 0.2)] {
                let tf = bv_timeline::Transform {
                    x,
                    ..bv_timeline::Transform::default()
                };
                p.set_clip_transform_at(c, Tick::from_seconds(at), tf)
                    .unwrap();
            }
        }
    }
    let dir = std::env::temp_dir().join(format!("bv-mpeg2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("dvd.mpg");
    let mut engine = Engine::new(Arc::new(|| {}));
    engine.set_project(&p);
    engine.start_render(RenderSettings {
        format: FileFormat::Mpeg2,
        path: path.clone(),
        range: TimeRange::new(Tick::ZERO, Tick::from_seconds(2)),
        video: Some(VideoParams {
            width: 720,
            height: 480,
            frame_rate: rate,
            bitrate: 6_000_000,
            max_bitrate: 8_000_000,
            ..VideoParams::default()
        }),
        audio: Some(AudioParams {
            sample_rate: 48_000,
            channels: 2,
            bit_depth: 16,
            float: false,
            bitrate: 192_000,
        }),
        options: RenderOptions::default(),
        project_file: None,
    });
    let start = Instant::now();
    while !engine.render_progress().is_some_and(|p| p.finished) {
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "rendered in time"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let done = engine.render_progress().unwrap();
    assert!(done.error.is_none(), "{:?}", done.error);
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..4], [0, 0, 1, 0xBA], "a program stream");
    assert_eq!((bytes.len() - 4) % bv_mp4::ps::PACK, 0, "whole packs");
    // B-pictures among them (their headers' picture_coding_type 3).
    assert!(
        bytes
            .windows(6)
            .any(|w| w[..4] == [0, 0, 1, 0] && (w[5] >> 3) & 7 == 3),
        "B-pictures"
    );
    let info = Engine::probe_file(&path.to_string_lossy()).unwrap();
    let video = info.video.expect("its video");
    assert_eq!((video.width, video.height), (720, 480));
    assert!(info.audio.is_some(), "its sound");
    // The system reckons the length from the mux rate (an estimate).
    let secs = info.duration.as_seconds_f64();
    assert!((1.4..=2.3).contains(&secs), "{secs}");
    // Frames decoded by the system look like the project's: an I-picture
    // (1 s) and P-pictures of moving bars between.
    let mut reader = bv_media::open_video(&info).unwrap();
    let mut pool = bv_render::VideoPool::new();
    for ms in [1000, 1200, 1450] {
        let t = Tick::from_seconds_f64(ms as f64 / 1000.0);
        let decoded = reader.frame_at(t).unwrap();
        assert_eq!((decoded.width, decoded.height), (720, 480));
        // Against the project's frame there (or the one beside it: the
        // file's times and the project's may round apart).
        let frame = rate.frame_at(t);
        let diff = (frame - 1..=frame + 1)
            .map(|f| {
                let source = bv_render::render_output(&p, rate.frame_start(f), 720, 480, &mut pool);
                decoded
                    .data
                    .iter()
                    .zip(&source.data)
                    .map(|(a, b)| (0..3).map(|c| (a[c] - b[c]).abs()).sum::<f32>() / 3.0)
                    .sum::<f32>()
                    / source.data.len() as f32
            })
            .collect::<Vec<f32>>();
        let best = diff.iter().copied().fold(f32::MAX, f32::min);
        assert!(best < 0.03, "{ms} ms: mean differences {diff:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
