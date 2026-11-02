//! The decoder against ffmpeg's: AC-3 files of every channel layout, sample
//! rate and a range of bit rates (low ones use coupling and rematrixing)
//! are decoded by both and compared. Both dither the silent mantissas with
//! their own noise, so they match to a signal-to-noise ratio, not exactly.
//! Run with `cargo test -p bv-ac3 --test oraculo -- --ignored --nocapture`.

#![allow(clippy::print_stdout)]

use std::path::PathBuf;
use std::process::Command;

fn make(name: &str, layout: &str, channels: usize, rate: u32, bitrate: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated/ac3");
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join(name);
    if !p.exists() {
        // Each channel its own gliding tone; a click every half second.
        let exprs: Vec<String> = (0..channels)
            .map(|i| {
                format!(
                    r"0.4*sin(2*PI*({}+40*t)*t)+0.3*lt(mod(t\,0.5)\,0.002)",
                    180 + 90 * i
                )
            })
            .collect();
        let src = format!("aevalsrc={}:c={layout}:s={rate}:d=2.5", exprs.join("|"));
        let st = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", &src])
            .args(["-c:a", "ac3", "-b:a", bitrate])
            .arg(&p)
            .status()
            .expect("ffmpeg on the PATH");
        assert!(st.success(), "ffmpeg made {name}");
    }
    p
}

fn reference(p: &PathBuf) -> (Vec<f32>, usize) {
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=channels",
            "-of",
            "csv=p=0",
        ])
        .arg(p)
        .output()
        .unwrap();
    let ch: usize = String::from_utf8_lossy(&probe.stdout)
        .trim()
        .parse()
        .unwrap();
    let o = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(p)
        .args(["-f", "f32le", "-"])
        .output()
        .unwrap();
    (
        o.stdout
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect(),
        ch,
    )
}

#[test]
#[ignore = "needs ffmpeg"]
fn layouts_rates_and_bitrates() {
    let cases = [
        ("mono.ac3", "mono", 1, 48_000, "96k"),
        ("stereo.ac3", "stereo", 2, 48_000, "192k"),
        ("stereo_low.ac3", "stereo", 2, 48_000, "96k"),
        ("stereo_441.ac3", "stereo", 2, 44_100, "160k"),
        ("stereo_32.ac3", "stereo", 2, 32_000, "128k"),
        ("2.1.ac3", "2.1", 3, 48_000, "224k"),
        ("3.0.ac3", "3.0", 3, 48_000, "256k"),
        ("quad.ac3", "quad", 4, 48_000, "256k"),
        ("4.0.ac3", "4.0", 4, 48_000, "256k"),
        ("5.0.ac3", "5.0", 5, 48_000, "384k"),
        ("5.1.ac3", "5.1", 6, 48_000, "448k"),
        ("5.1_low.ac3", "5.1", 6, 48_000, "192k"),
        ("5.1_high.ac3", "5.1", 6, 48_000, "640k"),
    ];
    for (name, layout, channels, rate, bitrate) in cases {
        let p = make(name, layout, channels, rate, bitrate);
        let data = std::fs::read(&p).unwrap();
        let mut dec = bv_ac3::Decoder::new();
        let mut got = Vec::new();
        // In odd pieces, as a demuxer may hand them over.
        for piece in data.chunks(1000) {
            dec.decode(piece, &mut got).unwrap();
        }
        let (want, ch) = reference(&p);
        assert_eq!(dec.channels(), ch, "{name}: channels");
        assert_eq!(dec.sample_rate(), rate, "{name}: rate");
        assert_eq!(got.len(), want.len(), "{name}: samples");
        let (mut signal, mut noise) = (0f64, 0f64);
        for (a, b) in got.iter().zip(&want) {
            signal += (*b as f64).powi(2);
            noise += (*a as f64 - *b as f64).powi(2);
        }
        let snr = 10.0 * (signal / noise.max(1e-30)).log10();
        println!("{name:16} {ch} ch {rate} Hz {bitrate:>5}: {snr:.1} dB");
        assert!(snr > 40.0, "{name}: {snr:.1} dB");
    }
}
