//! The decoder against ffmpeg's: ProRes frames of every profile, 4444 with
//! alpha and interlaced ones, from ffmpeg's two encoders, decoded by both.
//! Run with `cargo test -p bv-prores --test leitura -- --ignored --nocapture`.

#![allow(clippy::print_stdout)]

use std::path::PathBuf;
use std::process::Command;

const W: usize = 330;
const H: usize = 190;

/// A MOV made by ffmpeg, and its raw ProRes frames (one after the other).
fn make(name: &str, args: &[&str]) -> (PathBuf, PathBuf) {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated/prores");
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join(format!("{name}.mov"));
    let raw = d.join(name);
    if !raw.exists() {
        let st = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!(
                "testsrc2=size={W}x{H}:rate=25:duration=0.12,format=yuva444p10le,\
                 geq=lum='lum(X,Y)':cb='cb(X,Y)':cr='cr(X,Y)':a='1023*X/W'"
            ))
            .args(args)
            .arg(&p)
            .status()
            .expect("ffmpeg on the PATH");
        assert!(st.success(), "ffmpeg made {name}");
        let st = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-i"])
            .arg(&p)
            .args(["-map", "0:v:0", "-c", "copy", "-f", "data"])
            .arg(&raw)
            .status()
            .unwrap();
        assert!(st.success(), "ffmpeg copied {name}");
    }
    (p, raw)
}

/// ffmpeg's decode, as planes of 16-bit samples.
fn reference(p: &PathBuf, pix_fmt: &str) -> Vec<u16> {
    let o = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(p)
        .args(["-f", "rawvideo", "-pix_fmt", pix_fmt, "-"])
        .output()
        .expect("ffmpeg");
    o.stdout
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b))
        .collect()
}

/// The frames of a raw ProRes stream (each starts with its size).
fn frames(data: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 8 <= data.len() {
        let n = u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as usize;
        if n < 8 || at + n > data.len() {
            break;
        }
        out.push(&data[at..at + n]);
        at += n;
    }
    out
}

fn psnr(a: &[u16], b: &[u16], max: f64) -> f64 {
    let se: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| (*x as f64 - *y as f64).powi(2))
        .sum();
    if se == 0.0 {
        return f64::INFINITY;
    }
    10.0 * (max * max / (se / a.len() as f64)).log10()
}

#[test]
#[ignore = "needs ffmpeg"]
fn profiles_alpha_and_fields() {
    let cases: &[(&str, &[&str])] = &[
        (
            "proxy.prores",
            &[
                "-c:v",
                "prores_ks",
                "-profile:v",
                "0",
                "-pix_fmt",
                "yuv422p10le",
            ],
        ),
        (
            "lt.prores",
            &[
                "-c:v",
                "prores_ks",
                "-profile:v",
                "1",
                "-pix_fmt",
                "yuv422p10le",
            ],
        ),
        (
            "standard.prores",
            &[
                "-c:v",
                "prores_ks",
                "-profile:v",
                "2",
                "-pix_fmt",
                "yuv422p10le",
            ],
        ),
        (
            "hq.prores",
            &[
                "-c:v",
                "prores_ks",
                "-profile:v",
                "3",
                "-pix_fmt",
                "yuv422p10le",
            ],
        ),
        (
            "4444.prores",
            &[
                "-c:v",
                "prores_ks",
                "-profile:v",
                "4",
                "-pix_fmt",
                "yuva444p10le",
            ],
        ),
        (
            "4444xq.prores",
            &[
                "-c:v",
                "prores_ks",
                "-profile:v",
                "5",
                "-pix_fmt",
                "yuva444p10le",
            ],
        ),
        (
            "aw_hq.prores",
            &[
                "-c:v",
                "prores_aw",
                "-profile:v",
                "3",
                "-pix_fmt",
                "yuv422p10le",
            ],
        ),
        (
            "aw_4444.prores",
            &[
                "-c:v",
                "prores_aw",
                "-profile:v",
                "4",
                "-pix_fmt",
                "yuva444p10le",
            ],
        ),
        (
            "interlaced_tff.prores",
            &[
                "-c:v",
                "prores_ks",
                "-profile:v",
                "3",
                "-pix_fmt",
                "yuv422p10le",
                "-flags",
                "+ildct",
                "-top",
                "1",
            ],
        ),
        (
            "interlaced_bff.prores",
            &[
                "-c:v",
                "prores_ks",
                "-profile:v",
                "3",
                "-pix_fmt",
                "yuv422p10le",
                "-flags",
                "+ildct",
                "-top",
                "0",
            ],
        ),
    ];
    for (name, args) in cases {
        let (mov, raw) = make(name, args);
        let data = std::fs::read(&raw).unwrap();
        let fr = frames(&data);
        assert!(!fr.is_empty(), "{name}: frames");
        let alpha = name.contains("4444");
        let pix = if alpha { "yuva444p10le" } else { "yuv422p10le" };
        let want = reference(&mov, pix);
        let cw = if alpha { W } else { W.div_ceil(2) };
        let frame_len = W * H * (1 + usize::from(alpha)) + 2 * cw * H;
        let mut worst = f64::INFINITY;
        let mut worst_alpha = f64::INFINITY;
        for (i, f) in fr.iter().enumerate() {
            let d = bv_prores::decode(f).unwrap_or_else(|e| panic!("{name} frame {i}: {e}"));
            assert_eq!((d.width as usize, d.height as usize), (W, H));
            assert_eq!(d.chroma444, alpha, "{name}: chroma");
            let frame = &want[i * frame_len..(i + 1) * frame_len];
            let (y, rest) = frame.split_at(W * H);
            let (cb, rest) = rest.split_at(cw * H);
            let (cr, a) = rest.split_at(cw * H);
            let db = [
                psnr(&d.y, y, 1023.0),
                psnr(&d.cb, cb, 1023.0),
                psnr(&d.cr, cr, 1023.0),
            ];
            worst = worst.min(db.iter().copied().fold(f64::INFINITY, f64::min));
            if alpha {
                // ffmpeg gives 10-bit alpha.
                let ours: Vec<u16> = d
                    .alpha
                    .as_ref()
                    .expect("alpha")
                    .iter()
                    .map(|v| v >> 6)
                    .collect();
                worst_alpha = worst_alpha.min(psnr(&ours, a, 1023.0));
            }
        }
        println!(
            "{name:24} {} frames: every plane ≥ {worst:.1} dB{}",
            fr.len(),
            if alpha {
                format!(", alpha {worst_alpha:.1} dB")
            } else {
                String::new()
            }
        );
        assert!(worst > 50.0, "{name}: {worst:.1} dB");
        if alpha {
            assert!(worst_alpha > 50.0, "{name}: alpha {worst_alpha:.1} dB");
        }
    }
}
