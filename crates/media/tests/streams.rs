//! Files with several audio streams: each one is found and read on its own.
//! Needs `ffmpeg` on the PATH; run with
//! `cargo test -p bv-media --test streams -- --ignored`.

#![cfg(windows)]

use std::path::PathBuf;
use std::process::Command;

fn media_dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated");
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// MP4 of 2 s with video and two audio streams: 440 Hz, then 1000 Hz (a
/// recording with the voice and the game apart, say).
fn two_streams() -> PathBuf {
    let p = media_dir().join("dois_streams_de_audio.mp4");
    if !p.exists() {
        let st = Command::new("ffmpeg")
            .args(["-v", "error", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc2=s=320x180:r=30:d=2"])
            .args([
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000:d=2",
            ])
            .args([
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:sample_rate=48000:d=2",
            ])
            .args(["-map", "0:v", "-map", "1:a", "-map", "2:a"])
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"])
            .arg(&p)
            .status()
            .expect("ffmpeg on the PATH");
        assert!(st.success());
    }
    p
}

/// Rising zero crossings per second of the first channel.
fn frequency(samples: &[f32], rate: f64) -> f64 {
    let n = samples
        .windows(2)
        .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
        .count();
    n as f64 * rate / samples.len() as f64
}

/// Three streams (300, 1000, 2500 Hz) in `ext`: Media Foundation lists an
/// MP4's tracks backwards, the streams must still come in the file's order.
fn three_streams(ext: &str) -> PathBuf {
    let p = media_dir().join(format!("tres_streams.{ext}"));
    if !p.exists() {
        let st = Command::new("ffmpeg")
            .args(["-v", "error", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc2=s=320x180:r=30:d=2"])
            .args([
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=300:sample_rate=48000:d=2",
            ])
            .args([
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:sample_rate=48000:d=2",
            ])
            .args([
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=2500:sample_rate=48000:d=2",
            ])
            .args(["-map", "0:v", "-map", "1:a", "-map", "2:a", "-map", "3:a"])
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"])
            .arg(&p)
            .status()
            .expect("ffmpeg on the PATH");
        assert!(st.success());
    }
    p
}

#[test]
#[ignore = "needs ffmpeg and Media Foundation"]
fn streams_keep_the_files_order() {
    for ext in ["mp4", "mov", "mkv"] {
        let path = three_streams(ext);
        let info = bv_media::probe_file(path.to_str().unwrap()).unwrap();
        assert_eq!(info.audio_stream_count(), 3, "{ext}");
        for (stream, hz) in [(0u16, 300.0), (1, 1000.0), (2, 2500.0)] {
            let mut r = bv_media::open_audio_stream(&info, stream).unwrap();
            let rate = r.sample_rate().0 as f64;
            let buf = r.read(4800, 48_000).unwrap();
            let f = frequency(&buf.channels[0], rate);
            assert!((f - hz).abs() < hz * 0.05, "{ext} stream {stream}: {f} Hz");
        }
    }
}

#[test]
#[ignore = "needs ffmpeg and Media Foundation"]
fn every_audio_stream_is_found_and_read_apart() {
    let path = two_streams();
    let info = bv_media::probe_file(path.to_str().unwrap()).unwrap();
    assert!(info.video.is_some());
    assert_eq!(info.audio_stream_count(), 2, "{info:?}");
    for (stream, hz) in [(0u16, 440.0), (1, 1000.0)] {
        let mut r = bv_media::open_audio_stream(&info, stream).unwrap();
        let rate = r.sample_rate().0 as f64;
        let buf = r.read(4800, 48_000).unwrap();
        let f = frequency(&buf.channels[0], rate);
        // AAC noise adds a few crossings: within 5 %.
        assert!((f - hz).abs() < hz * 0.05, "stream {stream}: {f} Hz");
    }
    // On the timeline each stream gets its own audio track.
    let mut p = bv_timeline::Project::default();
    let (v, a) = (p.tracks[0].id, p.tracks[1].id);
    let m = p.add_media(info);
    let ids = p
        .insert_media(m, bv_core::Tick::ZERO, Some(v), Some(a))
        .unwrap();
    assert_eq!(ids.len(), 3);
    let audio = p
        .tracks
        .iter()
        .filter(|t| t.kind == bv_timeline::TrackKind::Audio)
        .count();
    assert_eq!(audio, 2);
}
