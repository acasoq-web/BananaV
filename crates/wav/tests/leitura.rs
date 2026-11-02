//! Every sound file reader against ffmpeg's: files of each container and
//! codec made by ffmpeg are read by BananaV and by ffmpeg, and compared
//! sample by sample.
//! Run with `cargo test -p bv-wav --test leitura -- --ignored --nocapture`.

#![allow(clippy::print_stdout)]

use std::path::PathBuf;
use std::process::Command;

fn make(name: &str, args: &[&str]) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated/som");
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join(name);
    if !p.exists() {
        let st = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg("aevalsrc=0.6*sin(2*PI*440*t)|0.4*sin(2*PI*660*t+1):s=44100:d=1.3")
            .args(args)
            .arg(&p)
            .status()
            .expect("ffmpeg on the PATH");
        assert!(st.success(), "ffmpeg made {name}");
    }
    p
}

/// ffmpeg's decode, interleaved floats.
fn reference(p: &PathBuf) -> Vec<f32> {
    let o = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(p)
        .args(["-f", "f32le", "-"])
        .output()
        .unwrap();
    o.stdout
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect()
}

#[test]
#[ignore = "needs ffmpeg"]
fn containers_and_codecs() {
    let cases: &[(&str, &[&str])] = &[
        ("u8.wav", &["-c:a", "pcm_u8"]),
        ("s16.wav", &["-c:a", "pcm_s16le"]),
        ("s24.wav", &["-c:a", "pcm_s24le"]),
        ("s32.wav", &["-c:a", "pcm_s32le"]),
        ("f32.wav", &["-c:a", "pcm_f32le"]),
        ("f64.wav", &["-c:a", "pcm_f64le"]),
        ("mulaw.wav", &["-c:a", "pcm_mulaw"]),
        ("alaw.wav", &["-c:a", "pcm_alaw"]),
        ("ima.wav", &["-c:a", "adpcm_ima_wav"]),
        ("msadpcm.wav", &["-c:a", "adpcm_ms"]),
        ("rf64.wav", &["-c:a", "pcm_s16le", "-rf64", "always"]),
        ("s24.w64", &["-c:a", "pcm_s24le"]),
        ("f32.w64", &["-c:a", "pcm_f32le"]),
        ("s16.aiff", &["-c:a", "pcm_s16be"]),
        ("s24.aiff", &["-c:a", "pcm_s24be"]),
        ("s8.aiff", &["-c:a", "pcm_s8"]),
        ("sowt.aifc", &["-c:a", "pcm_s16le", "-f", "aiff"]),
        ("fl32.aifc", &["-c:a", "pcm_f32be", "-f", "aiff"]),
        ("ulaw.aifc", &["-c:a", "pcm_mulaw", "-f", "aiff"]),
        ("s16.au", &["-c:a", "pcm_s16be"]),
        ("mulaw.au", &["-c:a", "pcm_mulaw"]),
        ("f32.au", &["-c:a", "pcm_f32be"]),
        ("s16.caf", &["-c:a", "pcm_s16le"]),
        ("f32be.caf", &["-c:a", "pcm_f32be"]),
        ("alaw.caf", &["-c:a", "pcm_alaw"]),
    ];
    for (name, args) in cases {
        let p = make(name, args);
        let file = std::io::BufReader::new(std::fs::File::open(&p).unwrap());
        let mut r = bv_wav::WavReader::new(file).unwrap_or_else(|e| panic!("{name}: {e}"));
        let ch = r.info.channels as usize;
        let want = reference(&p);
        let frames = r.info.frames as usize;
        assert_eq!(frames, want.len() / ch, "{name}: frames");
        // Read in odd pieces (random access across blocks).
        let mut got = Vec::with_capacity(want.len());
        let mut at = 0usize;
        while at < frames {
            let n = 777.min(frames - at);
            let planes = r.read_planar(at as i64, n).unwrap();
            for i in 0..n {
                for p in &planes {
                    got.push(p[i]);
                }
            }
            at += n;
        }
        let worst = got
            .iter()
            .zip(&want)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        println!(
            "{name:14} {:?} {} Hz {ch} ch {frames} frames, worst {worst:e}",
            r.info.container, r.info.sample_rate
        );
        // IMA's steps are summed bit by bit as its reference does; ffmpeg
        // multiplies (a rounding apart).
        let tolerance = if name.starts_with("ima") { 2e-3 } else { 1e-6 };
        assert!(worst <= tolerance, "{name}: off by {worst}");
    }
}
