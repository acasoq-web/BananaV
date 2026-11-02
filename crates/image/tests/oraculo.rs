//! Every writer against ffmpeg's readers: a test pattern written in each
//! format is decoded by ffmpeg and compared with what went in.
//! Run with `cargo test -p bv-image --test oraculo -- --ignored --nocapture`.

use std::path::PathBuf;
use std::process::Command;

use bv_image::{ExrCompression, Image, Pixels};

const W: u32 = 67;
const H: u32 = 45;

fn out(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated");
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

/// RGBA 16-bit: gradients, and alpha falling from left to right.
fn pattern() -> Vec<u16> {
    let mut v = Vec::new();
    for y in 0..H {
        for x in 0..W {
            v.push((x * 65535 / (W - 1)) as u16);
            v.push((y * 65535 / (H - 1)) as u16);
            v.push((((x + y) % 32) * 2047) as u16);
            v.push((65535 - x * 40000 / (W - 1)) as u16);
        }
    }
    v
}

/// ffmpeg's decode of `path` as RGBA, 16-bit (`deep`) or 8-bit (scaled to
/// 16 like the reference).
fn decode(path: &PathBuf, deep: bool) -> Vec<u16> {
    let o = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "rawvideo", "-pix_fmt"])
        .arg(if deep { "rgba64le" } else { "rgba" })
        .arg("-")
        .output()
        .expect("ffmpeg");
    if deep {
        o.stdout
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect()
    } else {
        o.stdout.iter().map(|b| *b as u16 * 257).collect()
    }
}

/// PSNR of the color channels, in dB.
fn psnr(a: &[u16], b: &[u16]) -> f64 {
    if a.len() != b.len() {
        return 0.0;
    }
    let (mut sum, mut n) = (0f64, 0f64);
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        if i % 4 == 3 {
            continue;
        }
        let d = (*x as f64 - *y as f64) / 257.0;
        sum += d * d;
        n += 1.0;
    }
    10.0 * (255.0f64 * 255.0 / (sum / n).max(1e-9)).log10()
}

/// Largest difference over the color channels (and alpha when kept), in
/// 8-bit steps.
fn worst(a: &[u16], b: &[u16], alpha: bool) -> f64 {
    if a.len() != b.len() {
        return f64::INFINITY;
    }
    let mut w = 0f64;
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        if i % 4 == 3 && !alpha {
            continue;
        }
        w = w.max((*x as f64 - *y as f64).abs() / 257.0);
    }
    w
}

/// (file, bytes, reference, 16-bit, alpha compared, tolerance).
type Case<'a> = (&'a str, Vec<u8>, &'a [u16], bool, bool, f64);

#[test]
#[ignore = "needs ffmpeg"]
fn ffmpeg_reads_what_was_written() {
    let p16 = pattern();
    let p8: Vec<u8> = p16.iter().map(|v| (v >> 8) as u8).collect();
    let back8: Vec<u16> = p8.iter().map(|v| *v as u16 * 257).collect();
    let img8 = |alpha| Image {
        width: W,
        height: H,
        pixels: Pixels::Rgba8(&p8),
        alpha,
    };
    let img16 = |alpha| Image {
        width: W,
        height: H,
        pixels: Pixels::Rgba16(&p16),
        alpha,
    };
    // (file, bytes, reference, 16-bit, alpha compared, tolerance in 8-bit
    // steps)
    let cases: Vec<Case<'_>> = vec![
        (
            "o.bmp",
            bv_image::bmp(&img8(false)),
            &back8,
            false,
            false,
            0.0,
        ),
        (
            "o_alpha.bmp",
            bv_image::bmp(&img8(true)),
            &back8,
            false,
            true,
            0.0,
        ),
        (
            "o.tif",
            bv_image::tiff(&img8(false), 8, false),
            &back8,
            false,
            false,
            0.0,
        ),
        (
            "o_zip16.tif",
            bv_image::tiff(&img16(true), 16, true),
            &p16,
            true,
            true,
            0.01,
        ),
        (
            "o_zip8.tif",
            bv_image::tiff(&img8(true), 8, true),
            &back8,
            false,
            true,
            0.0,
        ),
        (
            "o.dpx",
            bv_image::dpx(&img16(false), "test"),
            &p16,
            true,
            false,
            0.5,
        ),
        (
            "o.dds",
            bv_image::dds(&img8(true)),
            &back8,
            false,
            true,
            0.0,
        ),
    ];
    let mut failures = Vec::new();
    for (name, bytes, reference, deep, alpha, tolerance) in cases {
        let path = out(name);
        std::fs::write(&path, &bytes).unwrap();
        let got = decode(&path, deep);
        let w = worst(reference, &got, alpha);
        eprintln!("{name}: {} bytes, worst {w:.2}", bytes.len());
        if w > tolerance {
            failures.push(format!("{name}: worst {w:.2}"));
        }
    }
    // JPEG: lossy, judged by PSNR. The blue sawtooth of the pattern does
    // not survive 4:2:0 (below quality 90): ffmpeg's own 4:2:0 encode at
    // `-q:v 4` scores 25.9 dB on it.
    for (name, quality, min) in [("o90.jpg", 90, 38.0), ("o75.jpg", 75, 21.0)] {
        let bytes = bv_image::jpeg(&img8(false), quality);
        let path = out(name);
        std::fs::write(&path, &bytes).unwrap();
        let p = psnr(&back8, &decode(&path, false));
        eprintln!("{name}: {} bytes, PSNR {p:.1} dB", bytes.len());
        if p < min {
            failures.push(format!("{name}: PSNR {p:.1} dB"));
        }
    }
    // EXR: linear floats, compared after ffmpeg's conversion (it reads
    // EXR as linear and writes it straight to 16 bits).
    let linear: Vec<[f32; 4]> = p16
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p.map(|v| v as f32 / 65535.0))
        .collect();
    for (name, half, comp) in [
        ("o_half_zip.exr", true, ExrCompression::Zip),
        ("o_float.exr", false, ExrCompression::None),
    ] {
        let path = out(name);
        std::fs::write(&path, bv_image::exr(W, H, &linear, true, half, comp)).unwrap();
        let got = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&path)
            .args(["-f", "rawvideo", "-pix_fmt", "gbrapf32le", "-"])
            .output()
            .expect("ffmpeg")
            .stdout;
        let f: Vec<f32> = got
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        let n = (W * H) as usize;
        let mut w = 0f32;
        if f.len() == 4 * n {
            // Planar G, B, R, A.
            for i in 0..n {
                let px = linear[i];
                let back = [f[2 * n + i], f[i], f[n + i], f[3 * n + i]];
                for c in 0..4 {
                    w = w.max((px[c] - back[c]).abs());
                }
            }
        } else {
            w = f32::INFINITY;
        }
        eprintln!("{name}: worst {w}");
        if w > 1e-3 {
            failures.push(format!("{name}: worst {w}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
