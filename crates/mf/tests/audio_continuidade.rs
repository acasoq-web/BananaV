//! The AAC decoder against ffmpeg on a long, OBS-like recording (1080p60,
//! keyframes every 2 s, AAC 48 kHz): sequential reads in the mixer's block
//! size and random one-second reads must give the same samples as ffmpeg,
//! with no gaps (silence) or skipped samples — those are heard as clicks and
//! seen as holes in the waveform.
//! Needs `ffmpeg` on the PATH; run with
//! `cargo test -p bv-mf --test audio_continuidade -- --ignored --nocapture`.

#![cfg(any(
    windows,
    all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg))
))]

use std::path::PathBuf;
use std::process::Command;

fn media_dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated");
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 60 s of 1080p60 with an AAC stream of tones and sweeps, so a gained or
/// lost sample shows.
fn obs_like() -> PathBuf {
    let p = media_dir().join("obs_like_60s_tonal.mp4");
    if !p.exists() {
        let st = Command::new("ffmpeg")
            .args(["-v", "error", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc2=s=1920x1080:r=60:d=60"])
            // Tones and a slow sweep: no noise, which AAC may code as "noise
            // here" (PNS) that each decoder fills with its own random values.
            .args([
                "-f",
                "lavfi",
                "-i",
                "aevalsrc=0.25*sin(2*PI*440*t)+0.2*sin(2*PI*(150+40*t)*t)+0.1*sin(2*PI*3000*t)|0.25*sin(2*PI*660*t)+0.2*sin(2*PI*(300+30*t)*t):s=48000:d=60",
            ])
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-g", "120"])
            .args([
                "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "160k", "-ac", "2",
            ])
            .arg(&p)
            .status()
            .expect("ffmpeg on the PATH");
        assert!(st.success());
    }
    p
}

/// ffmpeg's decode of the left channel, as f32.
fn reference(path: &PathBuf) -> Vec<f32> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-af", "pan=mono|c0=c0", "-f", "f32le", "-"])
        .output()
        .expect("ffmpeg");
    out.stdout
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect()
}

/// The left channel of a planar block.
fn mono(channels: &[Vec<f32>]) -> Vec<f32> {
    channels[0].clone()
}

/// The offset (in samples, −2048 to 2048) at which `got` best matches the
/// reference around `start`, and the largest difference there.
fn best_match(reference: &[f32], got: &[f32], start: i64) -> (i64, f32) {
    let mut best = (0i64, f32::MAX);
    for off in -2048i64..=2048 {
        let mut worst = 0.0f32;
        for (i, g) in got.iter().enumerate().step_by(7) {
            let r = start + off + i as i64;
            let Some(&want) = usize::try_from(r).ok().and_then(|r| reference.get(r)) else {
                continue;
            };
            worst = worst.max((g - want).abs());
            if worst > best.1 {
                break;
            }
        }
        if worst < best.1 {
            best = (off, worst);
        }
    }
    best
}

#[test]
#[ignore = "needs ffmpeg and Media Foundation"]
fn sequential_and_random_reads_match_ffmpeg() {
    let path = obs_like();
    let reference = reference(&path);
    let p = path.to_str().unwrap();

    // Sequential, like playback: blocks of 1024 from 10 s on.
    let mut dec = bv_mf::AudioDecoder::open(p).unwrap();
    let mut seq = Vec::new();
    let mut pos = 480_000i64;
    for _ in 0..200 {
        let b = dec.read(pos, 1024).unwrap();
        seq.extend(mono(&b.channels));
        pos += 1024;
    }
    // One offset for every window: no sample gained or lost on the way.
    let offsets: Vec<i64> = (0..24)
        .map(|w| {
            best_match(
                &reference,
                &seq[w * 512..w * 512 + 512],
                480_000 + w as i64 * 512,
            )
            .0
        })
        .collect();
    eprintln!("window offsets: {offsets:?}");
    let (off, worst) = best_match(&reference, &seq[..4096], 480_000);
    eprintln!("sequential: offset {off}, worst {worst}");
    // The whole run must follow the reference at that one offset: a gap or
    // a skipped sample moves the rest.
    let mut bad = 0;
    for (i, g) in seq.iter().enumerate() {
        let r = (480_000 + off + i as i64) as usize;
        if (g - reference[r]).abs() > 1e-3 {
            bad += 1;
        }
    }
    eprintln!("sequential: {bad} of {} samples off", seq.len());

    // Random one-second reads, like the waveform's.
    let mut dec = bv_mf::AudioDecoder::open(p).unwrap();
    let mut random_bad = Vec::new();
    for second in [37i64, 5, 52, 20, 21, 44, 3, 30] {
        let start = second * 48_000;
        let b = dec.read(start, 48_000).unwrap();
        let m = mono(&b.channels);
        let zeros = m.iter().take(4800).filter(|v| v.abs() < 1e-6).count();
        let (o, w) = best_match(&reference, &m[24_000..28_096], start + 24_000);
        let mut bad = 0;
        for (i, g) in m.iter().enumerate() {
            let r = (start + o + i as i64) as usize;
            if (g - reference[r]).abs() > 1e-3 {
                bad += 1;
            }
        }
        eprintln!(
            "second {second}: offset {o}, worst {w}, {bad} off, {zeros} zeros in the first 0.1 s"
        );
        if bad > 0 || o != off {
            let errs: Vec<usize> = (0..12)
                .map(|w| {
                    (w * 4000..w * 4000 + 4000)
                        .filter(|&i| {
                            let r = (start + o + i as i64) as usize;
                            (m[i] - reference[r]).abs() > 1e-3
                        })
                        .count()
                })
                .collect();
            eprintln!("   errors per 4000 samples: {errs:?}");
            random_bad.push(second);
        }
    }
    assert_eq!(bad, 0, "sequential reads must follow ffmpeg");
    // The AAC priming is skipped (the edit list): no delay against ffmpeg.
    assert_eq!(off, 0, "the audio starts where ffmpeg's does");
    assert!(random_bad.is_empty(), "random reads off: {random_bad:?}");
}
