//! A DVD-Video folder rendered from a project: its video objects and
//! information files, a chapter at the marker, a 16:9 project letterboxed
//! on a 4:3 disc, and the folder's disc image (skipped where the system
//! has no MPEG-2 encoder).

use std::sync::Arc;
use std::time::{Duration, Instant};

use bv_core::{FrameRate, Tick, TimeRange};
use bv_engine::{
    AudioParams, Availability, Engine, FileFormat, RenderOptions, RenderSettings, VideoParams,
};
use bv_timeline::{Generator, MediaInfo, Project, ProjectSettings, TrackKind};

fn get32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
}

#[test]
fn a_project_renders_into_a_dvd_video_folder() {
    if FileFormat::Mpeg2.availability() != Availability::Ready {
        return;
    }
    let rate = FrameRate::new(30_000, 1001);
    let mut p = Project::new(ProjectSettings {
        width: 1280,
        height: 720,
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
        p.set_clip_duration(c, Tick::from_seconds(3)).unwrap();
    }
    p.add_marker(Tick::from_seconds(1));
    let dir = std::env::temp_dir().join(format!("bv-dvd-render-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut engine = Engine::new(Arc::new(|| {}));
    engine.set_project(&p);
    engine.start_render(RenderSettings {
        format: FileFormat::Mpeg2,
        path: dir.clone(),
        range: TimeRange::new(Tick::ZERO, Tick::from_seconds(3)),
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
            bitrate: 0,
        }),
        options: RenderOptions {
            dvd: true,
            // A 16:9 project on a 4:3 disc: letterboxed.
            dvd_wide: Some(false),
            ..RenderOptions::default()
        },
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
    let video_ts = dir.join("VIDEO_TS");
    assert!(dir.join("AUDIO_TS").is_dir());
    let vob = std::fs::read(video_ts.join("VTS_01_1.VOB")).unwrap();
    assert_eq!(vob.len() % bv_dvd::SECTOR, 0);
    let vts = std::fs::read(video_ts.join("VTS_01_0.IFO")).unwrap();
    assert_eq!(&vts[..12], b"DVDVIDEO-VTS");
    // The title's VOBs fill what the IFO says.
    let ifo_sectors = vts.len() / bv_dvd::SECTOR;
    let last = get32(&vts, 0x0C) as usize;
    assert_eq!(last + 1, 2 * ifo_sectors + vob.len() / bv_dvd::SECTOR);
    // Two chapters: the start and the marker.
    let vmg = std::fs::read(video_ts.join("VIDEO_TS.IFO")).unwrap();
    assert_eq!(&vmg[..12], b"DVDVIDEO-VMG");
    let chapters = u16::from_be_bytes([vmg[bv_dvd::SECTOR + 10], vmg[bv_dvd::SECTOR + 11]]);
    assert_eq!(chapters, 2);
    // Every VOBU in the map opens with a navigation pack.
    let admap = get32(&vts, 0xE4) as usize * bv_dvd::SECTOR;
    let count = (get32(&vts, admap) as usize + 1 - 4) / 4;
    assert!(count >= 3, "{count}");
    for k in 0..count {
        let s = get32(&vts, admap + 4 + 4 * k) as usize * bv_dvd::SECTOR;
        assert_eq!(&vob[s + 14..s + 18], [0, 0, 1, 0xBB], "VOBU {k}");
    }
    // The system's MPEG-2 decoder reads the title's video.
    let info = Engine::probe_file(&video_ts.join("VTS_01_1.VOB").to_string_lossy()).unwrap();
    let video = info.video.expect("its video");
    assert_eq!((video.width, video.height), (720, 480));
    let mut reader = bv_media::open_video(&info).unwrap();
    let decoded = reader.frame_at(Tick::from_seconds_f64(1.5)).unwrap();
    assert_eq!((decoded.width, decoded.height), (720, 480));
    assert!(info.audio.is_some(), "its linear PCM sound");
    let secs = info.duration.as_seconds_f64();
    assert!((2.5..=3.5).contains(&secs), "{secs}");
    // 4:3 in the sequence header, the bars between black bands.
    let seq = vob
        .windows(4)
        .position(|w| w == [0, 0, 1, 0xB3])
        .expect("a sequence header");
    assert_eq!(vob[seq + 7] >> 4, 2, "4:3");
    let row = |y: u32| {
        (0..720)
            .map(|x| {
                decoded.data[(y * 720 + x) as usize][..3]
                    .iter()
                    .sum::<f32>()
            })
            .sum::<f32>()
            / (720.0 * 3.0)
    };
    assert!(row(20) < 0.05, "band: {}", row(20));
    assert!(row(240) > 0.1, "picture: {}", row(240));
    // The folder as a disc: UDF's anchor at sector 256, the title found by
    // ISO 9660 too.
    let mut image = Vec::new();
    bv_engine::write_dvd_image(&dir, "Test", &mut image).unwrap();
    assert_eq!(image[256 * bv_dvd::SECTOR..][..2], [2, 0]);
    assert!(image.windows(14).any(|w| w == b"VTS_01_1.VOB;1"));
    let _ = std::fs::remove_dir_all(&dir);
}
