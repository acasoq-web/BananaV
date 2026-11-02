//! FLAC files decoded by ffmpeg must give back exactly the samples that
//! went in, and their MD5 must be the one in STREAMINFO.
//! Run with `cargo test -p bv-flac --test oraculo -- --ignored --nocapture`.

use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;

use bv_flac::FlacWriter;

fn out(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated");
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

/// Pseudo-random numbers in [-1, 1).
fn noise(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0
}

fn signal(kind: &str, channels: usize, frames: usize) -> Vec<Vec<f32>> {
    let mut seed = 7;
    (0..channels)
        .map(|c| {
            (0..frames)
                .map(|i| {
                    let t = i as f32 / 48_000.0;
                    match kind {
                        "silence" => 0.0,
                        "noise" => noise(&mut seed) * 0.8,
                        // Music-like: chords, a sweep and a little noise.
                        _ => {
                            0.3 * (2.0 * std::f32::consts::PI * (220.0 + 40.0 * c as f32) * t).sin()
                                + 0.2 * (2.0 * std::f32::consts::PI * (330.0 + 500.0 * t) * t).sin()
                                + 0.01 * noise(&mut seed)
                        }
                    }
                })
                .collect()
        })
        .collect()
}

#[test]
#[ignore = "needs ffmpeg"]
fn ffmpeg_decodes_the_same_samples() {
    let cases = [
        ("music", 2usize, 16u16, 100_000usize),
        ("music", 2, 24, 50_000),
        ("noise", 2, 16, 20_000),
        ("music", 1, 16, 9_000),
        ("music", 6, 24, 12_345),
        ("silence", 2, 16, 4096),
        ("music", 2, 16, 1000),
    ];
    for (kind, channels, bits, frames) in cases {
        let input = signal(kind, channels, frames);
        let mut w = FlacWriter::new(
            Cursor::new(Vec::new()),
            48_000,
            channels as u16,
            bits,
            &[("TITLE", "BananaV test")],
        )
        .unwrap();
        // In uneven pieces, as a render feeds it.
        let mut at = 0;
        while at < frames {
            let n = (frames - at).min(1601);
            let piece: Vec<Vec<f32>> = input.iter().map(|c| c[at..at + n].to_vec()).collect();
            w.write_planar(&piece).unwrap();
            at += n;
        }
        let data = w.finish().unwrap().into_inner();
        let name = format!("t_{kind}_{channels}ch_{bits}.flac");
        let path = out(&name);
        std::fs::write(&path, &data).unwrap();

        let fmt = if bits == 16 { "s16le" } else { "s32le" };
        let decoded = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&path)
            .args(["-f", fmt, "-"])
            .output()
            .expect("ffmpeg");
        assert!(
            decoded.stderr.is_empty(),
            "{name}: {}",
            String::from_utf8_lossy(&decoded.stderr)
        );
        let got: Vec<i64> = if bits == 16 {
            decoded
                .stdout
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| i16::from_le_bytes(*b) as i64)
                .collect()
        } else {
            // 24-bit samples come out in the high bits of 32.
            decoded
                .stdout
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| (i32::from_le_bytes(*b) >> 8) as i64)
                .collect()
        };
        let max = ((1i64 << (bits - 1)) - 1) as f32;
        let mut want = Vec::with_capacity(frames * channels);
        for i in 0..frames {
            for ch in &input {
                want.push((ch[i].clamp(-1.0, 1.0) * max).round() as i64);
            }
        }
        assert_eq!(got.len(), want.len(), "{name}: length");
        let wrong = got.iter().zip(&want).filter(|(a, b)| a != b).count();
        eprintln!(
            "{name}: {} bytes ({:.1}% of PCM), {wrong} samples differ",
            data.len(),
            100.0 * data.len() as f64 / (frames * channels * bits as usize / 8) as f64
        );
        assert_eq!(wrong, 0, "{name}");
        // STREAMINFO's MD5 is the one of the decoded samples.
        let codec = if bits == 16 { "pcm_s16le" } else { "pcm_s24le" };
        let md5 = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&path)
            .args(["-c:a", codec, "-f", "md5", "-"])
            .output()
            .expect("ffmpeg");
        let ours: String = data[26..42].iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            String::from_utf8_lossy(&md5.stdout).trim(),
            format!("MD5={ours}"),
            "{name}: MD5"
        );
        let tags = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format_tags=title",
                "-of",
                "default=nw=1",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&tags.stdout).contains("BananaV test"));
    }
}
