//! Renders a project to every format and checks the files with
//! ffprobe/ffmpeg: codec, size, frame count, duration, audio sync, the
//! picture itself, metadata and markers.
//! Run with `cargo test -p bv-engine --test exportar -- --ignored --nocapture`.

#![cfg(any(windows, all(target_os = "linux", not(bv_no_ffmpeg))))]
#![allow(clippy::print_stdout)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use bv_core::{FrameRate, Tick, TimeRange};
use bv_engine::{
    AudioParams, Encoder, Engine, FileFormat, ProResProfile, RateMode, RenderOptions,
    RenderSettings, VideoParams,
};
use bv_timeline::{Generator, MediaInfo, Project};

fn out(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated/render");
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

/// Two seconds: color bars then red, a 1 kHz tone with a click at 1.5 s,
/// a marker at 0.5 s and a title.
fn project() -> Project {
    let mut p = Project::default();
    p.settings.frame_rate = FrameRate::FPS_30;
    p.settings.width = 640;
    p.settings.height = 360;
    p.summary.title = "BananaV render".into();
    let (v, a) = (p.tracks[0].id, p.tracks[1].id);
    let s = p.settings;
    let bars = p.add_media(MediaInfo::generated(Generator::ColorBars, &s));
    let red = p.add_media(MediaInfo::generated(
        Generator::SolidColor {
            rgba: [1.0, 0.0, 0.0, 1.0],
        },
        &s,
    ));
    let tone = p.add_media(MediaInfo::generated(
        Generator::TestTone {
            frequency_hz: 1000.0,
            level_db: -12.0,
        },
        &s,
    ));
    let b = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
    let _ = p.trim_end(b, Tick::from_seconds(1));
    let r = p
        .insert_media(red, Tick::from_seconds(1), Some(v), None)
        .unwrap()[0];
    let _ = p.trim_end(r, Tick::from_seconds(2));
    let t = p.insert_media(tone, Tick::ZERO, None, Some(a)).unwrap()[0];
    let _ = p.trim_end(t, Tick::from_seconds(2));
    let m = p.add_marker(Tick::from_seconds_f64(0.5));
    p.rename_marker(m, "Halfway".into());
    p
}

fn render(p: &Project, s: RenderSettings) -> Result<Vec<PathBuf>, String> {
    let mut engine = Engine::new(Arc::new(|| {}));
    engine.set_project(p);
    engine.start_render(s);
    loop {
        let pr = engine.render_progress().unwrap();
        if pr.finished {
            return match pr.error {
                Some(e) => Err(e),
                None => Ok(pr.files),
            };
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn probe(path: &Path, entries: &str) -> String {
    let o = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-count_frames",
            "-show_entries",
            entries,
            "-of",
            "default=nw=1",
        ])
        .arg(path)
        .output()
        .expect("ffprobe");
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn value(text: &str, key: &str) -> Option<f64> {
    text.lines()
        .find_map(|l| l.strip_prefix(&format!("{key}=")))
        .and_then(|v| v.parse().ok())
}

/// The mean RGB of the frame at `seconds`.
fn color_at(path: &Path, seconds: f64) -> Vec<u8> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error"]);
    // (A seek in a still JPEG gives nothing.)
    if seconds > 0.0 {
        cmd.args(["-ss", &seconds.to_string()]);
    }
    cmd.arg("-i")
        .arg(path)
        .args([
            "-frames:v",
            "1",
            "-vf",
            "scale=1:1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("ffmpeg")
        .stdout
}

fn video(format: FileFormat, encoder: Encoder) -> VideoParams {
    VideoParams {
        width: 640,
        height: 360,
        frame_rate: FrameRate::FPS_30,
        bit_depth: 8,
        encoder,
        rate_mode: RateMode::Vbr,
        bitrate: 4_000_000,
        max_bitrate: 6_000_000,
        prores: if format == FileFormat::ProRes {
            ProResProfile::Hq
        } else {
            ProResProfile::Standard
        },
        ..VideoParams::default()
    }
}

fn settings(format: FileFormat, path: PathBuf, video: Option<VideoParams>) -> RenderSettings {
    RenderSettings {
        format,
        path,
        range: TimeRange::new(Tick::ZERO, Tick::from_seconds(2)),
        video,
        audio: Some(AudioParams {
            sample_rate: 48_000,
            channels: 2,
            bit_depth: 16,
            float: false,
            bitrate: 192_000,
        }),
        options: RenderOptions {
            use_rotation: true,
            save_markers: true,
            loudness_log: true,
            ..RenderOptions::default()
        },
        project_file: None,
    }
}

#[test]
#[ignore = "needs ffmpeg and the system's encoders"]
fn every_video_format_and_encoder() {
    let p = project();
    let mut failures = Vec::new();
    let mut cases: Vec<(FileFormat, Encoder)> = Vec::new();
    for f in [FileFormat::AvcMp4, FileFormat::HevcMp4, FileFormat::Av1Mp4] {
        for e in f.video_encoders() {
            cases.push((f, e));
        }
    }
    cases.push((FileFormat::ProRes, Encoder::Software));
    // Windows Media is Windows'.
    if cfg!(windows) {
        cases.push((FileFormat::Wmv, Encoder::Software));
    }
    for (f, e) in cases {
        let name = format!("v_{f:?}_{e:?}.{}", f.extension());
        let path = out(&name);
        let _ = std::fs::remove_file(&path);
        if let Err(err) = render(&p, settings(f, path.clone(), Some(video(f, e)))) {
            failures.push(format!("{name}: {err}"));
            continue;
        }
        let info = probe(
            &path,
            "stream=codec_name,width,height,nb_read_frames,sample_rate:format=duration:format_tags=title",
        );
        let red = color_at(&path, 1.5);
        println!("{name}:\n{info}red frame {red:?}\n");
        for want in [
            "width=640",
            "height=360",
            "nb_read_frames=60",
            "sample_rate=48000",
        ] {
            if !info.contains(want) {
                failures.push(format!("{name}: no {want}"));
            }
        }
        if f != FileFormat::Wmv && !info.contains("TAG:title=BananaV render") {
            failures.push(format!("{name}: no title"));
        }
        let d = value(&info, "duration").unwrap_or(0.0);
        if (d - 2.0).abs() > 0.1 {
            failures.push(format!("{name}: duration {d}"));
        }
        if !(red.len() == 3 && red[0] > 200 && red[1] < 50 && red[2] < 50) {
            failures.push(format!("{name}: frame at 1.5 s is {red:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
#[ignore = "needs ffmpeg"]
fn prores_4444_keeps_alpha_and_hits_its_rate() {
    let mut p = project();
    // Nothing on the video track for the first second: transparent there.
    let first = p.tracks[0].clips[0].id;
    p.delete_clips(&[first], false).unwrap();
    let path = out("v_prores_4444.mov");
    let mut params = video(FileFormat::ProRes, Encoder::Software);
    params.prores = ProResProfile::P4444;
    params.alpha = true;
    render(&p, settings(FileFormat::ProRes, path.clone(), Some(params))).unwrap();
    let info = probe(&path, "stream=codec_name,profile,pix_fmt,bit_rate");
    println!("{info}");
    assert!(info.contains("codec_name=prores"), "{info}");
    assert!(info.contains("pix_fmt=yuva444p"), "{info}");
    // The alpha of the first second is 0, of the second 1.
    let alpha = |t: f64| {
        Command::new("ffmpeg")
            .args(["-v", "error", "-ss", &t.to_string(), "-i"])
            .arg(&path)
            .args([
                "-frames:v",
                "1",
                "-vf",
                "alphaextract,scale=1:1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "gray",
                "-",
            ])
            .output()
            .unwrap()
            .stdout
    };
    assert_eq!(alpha(0.5), vec![0]);
    assert_eq!(alpha(1.5), vec![255]);
}

#[test]
#[ignore = "needs ffmpeg"]
fn every_image_sequence() {
    let p = project();
    let mut failures = Vec::new();
    for f in [
        FileFormat::Png,
        FileFormat::Bmp,
        FileFormat::Tiff,
        FileFormat::Dpx,
        FileFormat::Exr,
        FileFormat::Dds,
        FileFormat::Jpeg,
        FileFormat::Heic,
        FileFormat::WmPhoto,
    ] {
        // HEIC and JPEG XR are written by Windows' Imaging Component.
        if !cfg!(windows) && matches!(f, FileFormat::Heic | FileFormat::WmPhoto) {
            continue;
        }
        let dir = out(&format!("seq_{f:?}"));
        let _ = std::fs::remove_dir_all(&dir);
        let mut v = video(f, Encoder::Software);
        v.frame_rate = FrameRate::new(5, 1);
        let mut s = settings(f, dir.join(format!("frame.{}", f.extension())), Some(v));
        s.audio = None;
        let files = match render(&p, s) {
            Ok(files) => files,
            Err(e) => {
                failures.push(format!("{f:?}: {e}"));
                continue;
            }
        };
        let last = files.last().cloned().unwrap_or_default();
        println!("{f:?}: {} files, last {}", files.len(), last.display());
        if files.len() != 10 {
            failures.push(format!("{f:?}: {} files", files.len()));
        }
        // ffmpeg reads all of them but JPEG XR.
        if f != FileFormat::WmPhoto {
            let red = color_at(&last, 0.0);
            if !(red.len() == 3 && red[0] > 200 && red[1] < 50 && red[2] < 50) {
                failures.push(format!("{f:?}: last frame is {red:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
#[ignore = "needs ffmpeg and the system's encoders"]
fn every_audio_format() {
    let p = project();
    let mut failures = Vec::new();
    for (f, codec) in [
        (FileFormat::Wav, "pcm_s16le"),
        (FileFormat::Wave64, "pcm_s16le"),
        (FileFormat::Aiff, "pcm_s16be"),
        (FileFormat::Flac, "flac"),
        (FileFormat::Mp3, "mp3"),
        (FileFormat::Aac, "aac"),
        (FileFormat::Wma, "wmav2"),
    ] {
        if !cfg!(windows) && f == FileFormat::Wma {
            continue;
        }
        let path = out(&format!("a_{f:?}.{}", f.extension()));
        let _ = std::fs::remove_file(&path);
        if let Err(e) = render(&p, settings(f, path.clone(), None)) {
            failures.push(format!("{f:?}: {e}"));
            continue;
        }
        let info = probe(
            &path,
            "stream=codec_name,sample_rate,channels:format=duration",
        );
        println!("{f:?}:\n{info}");
        if !info.contains(&format!("codec_name={codec}")) {
            failures.push(format!("{f:?}: not {codec}: {info}"));
        }
        let d = value(&info, "duration").unwrap_or(0.0);
        if (d - 2.0).abs() > 0.1 {
            failures.push(format!("{f:?}: duration {d}"));
        }
        let log = path.with_file_name(format!(
            "{}_loud.txt",
            path.file_stem().unwrap().to_string_lossy()
        ));
        if !log.exists() {
            failures.push(format!("{f:?}: no loudness log"));
        }
    }
    // The marker is a cue point of the WAV file.
    let wav = std::fs::read(out("a_Wav.wav")).unwrap_or_default();
    if !wav.windows(4).any(|w| w == b"cue ") || !wav.windows(7).any(|w| w == b"Halfway") {
        failures.push("WAV: no marker".into());
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
#[ignore = "needs ffmpeg and Media Foundation"]
fn detects_scene_cuts() {
    // Three scenes: red (1 s), bars (1 s), blue (1 s).
    let path = out("tres_cenas.mp4");
    if !path.exists() {
        let st = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=red:s=320x180:r=30:d=1",
                "-f",
                "lavfi",
                "-i",
                "smptebars=s=320x180:r=30:d=1",
                "-f",
                "lavfi",
                "-i",
                "color=blue:s=320x180:r=30:d=1",
                "-filter_complex",
                "[0][1][2]concat=n=3:v=1:a=0",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&path)
            .status()
            .unwrap();
        assert!(st.success());
    }
    let mut p = Project::default();
    p.settings.frame_rate = FrameRate::FPS_30;
    let info = bv_media::probe_file(path.to_str().unwrap()).unwrap();
    let m = p.add_media(info);
    let v = p.tracks[0].id;
    let c = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
    let mut engine = Engine::new(Arc::new(|| {}));
    engine.set_project(&p);
    assert!(engine.start_scene_detection(c, 0.5));
    let pr = loop {
        let pr = engine.scene_progress().unwrap();
        if pr.finished {
            break pr;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(pr.error.is_none(), "{:?}", pr.error);
    let secs: Vec<f64> = pr.cuts.iter().map(|t| t.as_seconds_f64()).collect();
    assert_eq!(secs.len(), 2, "{secs:?}");
    assert!(
        (secs[0] - 1.0).abs() < 0.05 && (secs[1] - 2.0).abs() < 0.05,
        "{secs:?}"
    );
    // The same file looked through as a media (Project Media's Detect
    // Scenes): from 0.5 s, its cuts in its own time.
    engine.cancel_scene_detection();
    let info = p.media(m).unwrap().clone();
    let part = bv_core::TimeRange::new(Tick::from_seconds_f64(0.5), Tick::from_seconds(2));
    assert!(engine.start_media_scene_detection(info, part, 0.5));
    let pr = loop {
        let pr = engine.scene_progress().unwrap();
        if pr.finished {
            break pr;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!((pr.media, pr.clip), (Some(m), None));
    let secs: Vec<f64> = pr.cuts.iter().map(|t| t.as_seconds_f64()).collect();
    assert_eq!(secs.len(), 2, "{secs:?}");
    assert!(
        (secs[0] - 1.0).abs() < 0.05 && (secs[1] - 2.0).abs() < 0.05,
        "{secs:?}"
    );
}

#[test]
#[ignore = "needs ffmpeg and Media Foundation"]
fn mp4_from_a_96_khz_project_has_48_khz_aac() {
    let mut p = project();
    p.settings.sample_rate = bv_core::SampleRate(96_000);
    let path = out("v_96k.mp4");
    let mut s = settings(
        FileFormat::AvcMp4,
        path.clone(),
        Some(video(FileFormat::AvcMp4, Encoder::Software)),
    );
    if let Some(a) = &mut s.audio {
        a.sample_rate = 96_000;
    }
    render(&p, s).unwrap();
    let text = probe(&path, "stream=codec_name,sample_rate");
    assert!(
        text.contains("codec_name=aac") && text.contains("sample_rate=48000"),
        "{text}"
    );
}
