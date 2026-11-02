//! ProRes frames in a MOV, decoded by ffmpeg, against what went in: every
//! profile, 4:2:2 and 4:4:4 with alpha, odd sizes.
//! Run with `cargo test -p bv-prores --test oraculo -- --ignored --nocapture`.

use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;

use bv_mp4::{Container, VideoFormat, VideoTrack, Writer};
use bv_prores::{Encoder, Frame, Profile};

fn out(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated");
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

/// A 10-bit test picture: soft gradients, a hard edge, fine stripes.
fn plane(w: usize, h: usize, n: usize, kind: usize) -> Vec<u16> {
    let mut v = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let base = match kind {
                0 => 64 + (x * 600 / w) as i32 + (y * 200 / h) as i32,
                1 => 512 + ((x as f32 / w as f32 - 0.5) * 300.0) as i32,
                _ => 512 + ((y as f32 / h as f32 - 0.5) * 300.0) as i32,
            };
            let edge = if kind == 0 && (x + n * 3) % 97 < 20 {
                120
            } else {
                0
            };
            let stripes = if kind == 0 && y > h / 2 && x % 4 < 2 {
                40
            } else {
                0
            };
            v.push((base + edge + stripes).clamp(64, 940) as u16);
        }
    }
    v
}

fn psnr(a: &[u16], b: &[u16], max: f64) -> f64 {
    let mse = a
        .iter()
        .zip(b)
        .map(|(x, y)| (*x as f64 - *y as f64).powi(2))
        .sum::<f64>()
        / a.len().max(1) as f64;
    if mse == 0.0 {
        99.0
    } else {
        10.0 * (max * max / mse).log10()
    }
}

#[test]
#[ignore = "needs ffmpeg"]
fn ffmpeg_decodes_every_profile() {
    let (w, h) = (360usize, 200usize);
    let frames = 3;
    let mut failures = Vec::new();
    for (profile, min_psnr) in [
        (Profile::Proxy, 34.0),
        (Profile::Lt, 38.0),
        (Profile::Standard, 40.0),
        (Profile::Hq, 44.0),
        (Profile::P4444, 46.0),
        (Profile::P4444Xq, 48.0),
    ] {
        let alpha = profile.is_444();
        let enc = Encoder::new(profile, w as u32, h as u32, 30.0, alpha);
        let cw = if profile.is_444() { w } else { w / 2 };
        let fourcc = match profile {
            Profile::Proxy => *b"apco",
            Profile::Lt => *b"apcs",
            Profile::Standard => *b"apcn",
            Profile::Hq => *b"apch",
            Profile::P4444 => *b"ap4h",
            Profile::P4444Xq => *b"ap4x",
        };
        let mut mov = Writer::new(
            Cursor::new(Vec::new()),
            Container::Mov,
            Some(VideoTrack {
                format: VideoFormat::ProRes { fourcc },
                width: w as u32,
                height: h as u32,
                timescale: 30,
                bit_depth: 10,
                color: (1, 1, 1, false),
                alpha,
                pixel_aspect: (1, 1),
                spherical: false,
            }),
            None,
            0,
        )
        .unwrap();
        let mut sources = Vec::new();
        let mut bytes = 0;
        for n in 0..frames {
            let y = plane(w, h, n, 0);
            let cb = plane(cw, h, n, 1);
            let cr = plane(cw, h, n, 2);
            let a: Vec<u16> = (0..w * h)
                .map(|i| {
                    let x = i % w;
                    if x < w / 3 {
                        65535
                    } else if x < 2 * w / 3 {
                        (x * 200) as u16
                    } else {
                        0
                    }
                })
                .collect();
            let frame = enc.encode(&Frame {
                y: &y,
                cb: &cb,
                cr: &cr,
                alpha: alpha.then_some(&a[..]),
            });
            bytes += frame.len();
            mov.write_video(&frame, n as i64, None, 1, true).unwrap();
            sources.push((y, cb, cr, a));
        }
        let data = mov.finish().unwrap().into_inner();
        let name = format!("prores_{profile:?}.mov");
        let path = out(&name);
        std::fs::write(&path, &data).unwrap();
        // 4444 decodes at 12 bits in ffmpeg: compare there, without a
        // conversion back to 10.
        let pix = if alpha { "yuva444p12le" } else { "yuv422p10le" };
        let decoded = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&path)
            .args(["-f", "rawvideo", "-pix_fmt", pix, "-"])
            .output()
            .expect("ffmpeg");
        let err = String::from_utf8_lossy(&decoded.stderr).to_string();
        let samples: Vec<u16> = decoded
            .stdout
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            // 12-bit samples back to the 10-bit scale (alpha stays 12-bit).
            .collect();
        let per_frame = w * h + 2 * cw * h + if alpha { w * h } else { 0 };
        let got = samples.len() / per_frame;
        let mut worst = 99.0f64;
        let mut alpha_off = 0usize;
        for (n, (y, cb, cr, a)) in sources.iter().enumerate().take(got) {
            let f = &samples[n * per_frame..(n + 1) * per_frame];
            let (dy, rest) = f.split_at(w * h);
            let (dcb, rest) = rest.split_at(cw * h);
            let (dcr, da) = rest.split_at(cw * h);
            let ten = |v: &[u16]| -> Vec<u16> {
                if alpha {
                    v.iter().map(|x| (x + 2) >> 2).collect()
                } else {
                    v.to_vec()
                }
            };
            let (dy, dcb, dcr) = (&ten(dy)[..], &ten(dcb)[..], &ten(dcr)[..]);
            if n == 0 {
                eprintln!(
                    "  planes: Y {:.1} Cb {:.1} Cr {:.1}; first Y {:?} vs {:?}",
                    psnr(y, dy, 1023.0),
                    psnr(cb, dcb, 1023.0),
                    psnr(cr, dcr, 1023.0),
                    &y[..6],
                    &dy[..6]
                );
                if alpha {
                    eprintln!(
                        "  alpha row 0: {:?} vs {:?}",
                        a.iter()
                            .step_by(40)
                            .take(9)
                            .map(|v| v >> 4)
                            .collect::<Vec<_>>(),
                        da.iter().step_by(40).take(9).collect::<Vec<_>>()
                    );
                }
            }
            worst = worst
                .min(psnr(y, dy, 1023.0))
                .min(psnr(cb, dcb, 1023.0))
                .min(psnr(cr, dcr, 1023.0));
            if alpha {
                // ffmpeg gives alpha in 12 bits.
                alpha_off += a.iter().zip(da).filter(|(s, d)| (**s >> 4) != **d).count();
            }
        }
        eprintln!(
            "{profile:?}: {} KB/frame, {got} frames, worst PSNR {worst:.1} dB, {alpha_off} alpha values off {err}",
            bytes / frames / 1024
        );
        if got != frames || worst < min_psnr || alpha_off > 0 || !err.is_empty() {
            failures.push(format!(
                "{profile:?}: {got} frames, {worst:.1} dB, alpha {alpha_off}"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
