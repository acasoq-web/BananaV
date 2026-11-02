//! Every video encoder of this computer (processor and graphics cards)
//! against ffmpeg's decoders: 60 frames of a moving pattern are encoded,
//! decoded back by ffmpeg and compared (frame count and PSNR).
//! Needs `ffmpeg` on the PATH; run with
//! `cargo test -p bv-mf --test encoders -- --ignored --nocapture`.

#![cfg(any(windows, all(target_os = "linux", not(bv_no_ffmpeg))))]

use std::path::PathBuf;
use std::process::Command;

use bv_core::{ColorInfo, FrameRate, PixelFormat, PlaneData, VideoFrame};
use bv_mf::{Codec, RateControl, Vendor, VideoEncoder, VideoEncoderSettings};

const W: u32 = 640;
const H: u32 = 360;
const FRAMES: usize = 60;

fn out(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated");
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

/// Frame `n`: diagonal bands moving right, over a vertical gradient.
fn frame(n: usize, ten_bit: bool) -> VideoFrame {
    let format = if ten_bit {
        PixelFormat::Yuv420P10
    } else {
        PixelFormat::Yuv420P8
    };
    let mut f = VideoFrame::black(W, H, format, ColorInfo::default());
    let scale = if ten_bit { 4 } else { 1 };
    for (p, plane) in f.planes.iter_mut().enumerate() {
        let (pw, ph) = (plane.width, plane.height);
        for y in 0..ph {
            for x in 0..pw {
                let v: u32 = match p {
                    0 => 16 + ((x + y + n * 4) % 64) as u32 * 2 + (y as u32 * 64 / ph as u32),
                    1 => 128 + ((x as u32 * 60) / pw as u32) - 30,
                    _ => 128 + ((y as u32 * 60) / ph as u32) - 30,
                };
                let i = y * plane.stride + x;
                match &mut plane.data {
                    PlaneData::U8(d) => d[i] = v as u8,
                    PlaneData::U16(d) => d[i] = (v * scale) as u16,
                }
            }
        }
    }
    f
}

/// The 8-bit planes of a frame, concatenated like ffmpeg's yuv420p.
fn raw(f: &VideoFrame) -> Vec<u8> {
    let mut v = Vec::new();
    for plane in &f.planes {
        for y in 0..plane.height {
            for x in 0..plane.width {
                let s = match &plane.data {
                    PlaneData::U8(d) => d[y * plane.stride + x],
                    PlaneData::U16(d) => (d[y * plane.stride + x] >> 2) as u8,
                };
                v.push(s);
            }
        }
    }
    v
}

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let mse = a
        .iter()
        .zip(b)
        .map(|(x, y)| (*x as f64 - *y as f64).powi(2))
        .sum::<f64>()
        / a.len() as f64;
    if mse == 0.0 {
        99.0
    } else {
        10.0 * (255.0f64 * 255.0 / mse).log10()
    }
}

fn run(codec: Codec, vendor: Vendor, ten_bit: bool) -> Result<(String, f64, usize), String> {
    let s = VideoEncoderSettings {
        codec,
        vendor,
        width: W,
        height: H,
        frame_rate: FrameRate::new(30, 1),
        bitrate: 4_000_000,
        max_bitrate: 6_000_000,
        rate_control: RateControl::Vbr,
        gop: 30,
        ten_bit,
        field_order: bv_core::FieldOrder::Progressive,
    };
    let mut enc = VideoEncoder::new(s).map_err(|e| e.to_string())?;
    let name = enc.name().to_string();
    // AV1 encoders repeat the sequence header in the keyframes.
    let mut stream = if codec == Codec::Av1 {
        Vec::new()
    } else {
        enc.sequence_header().unwrap_or_default()
    };
    let mut packets = Vec::new();
    for n in 0..FRAMES {
        let t = n as i64 * 10_000_000 / 30;
        packets.extend(
            enc.encode(&frame(n, ten_bit), t, 10_000_000 / 30)
                .map_err(|e| e.to_string())?,
        );
    }
    packets.extend(enc.finish().map_err(|e| e.to_string())?);
    let keys = packets.iter().filter(|p| p.keyframe).count();
    for p in &packets {
        stream.extend_from_slice(&p.data);
    }
    let (ext, demuxer) = match codec {
        Codec::H264 => ("h264", "h264"),
        Codec::Hevc => ("hevc", "hevc"),
        _ => ("obu", "obu"),
    };
    let path = out(&format!("enc_{codec:?}_{vendor:?}_{ten_bit}.{ext}"));
    std::fs::write(&path, &stream).unwrap();
    let decoded = Command::new("ffmpeg")
        .args(["-v", "error", "-f", demuxer, "-i"])
        .arg(&path)
        .args(["-f", "rawvideo", "-pix_fmt", "yuv420p", "-"])
        .output()
        .map_err(|e| e.to_string())?;
    let size = (W * H * 3 / 2) as usize;
    let got = decoded.stdout.len() / size;
    let mut worst = 99.0f64;
    for n in 0..got.min(FRAMES) {
        let p = psnr(
            &raw(&frame(n, ten_bit)),
            &decoded.stdout[n * size..(n + 1) * size],
        );
        worst = worst.min(p);
    }
    eprintln!(
        "{codec:?} {vendor:?} 10-bit {ten_bit}: {name}: {} packets ({keys} key), {got} frames decoded, worst PSNR {worst:.1} dB",
        packets.len()
    );
    Ok((name, worst, got))
}

#[test]
#[ignore = "needs ffmpeg and the system's encoders"]
fn every_encoder_round_trips() {
    let mut failures = Vec::new();
    for e in bv_mf::encoders() {
        if !matches!(e.codec, Codec::H264 | Codec::Hevc | Codec::Av1) {
            continue;
        }
        // Windows' own HEVC encoder has no Main 10 (the render does not ask
        // it for 10 bits).
        let tens: &[bool] = if e.codec == Codec::H264 || (e.codec == Codec::Hevc && !e.hardware) {
            &[false]
        } else {
            &[false, true]
        };
        for &ten in tens {
            match run(e.codec, e.vendor, ten) {
                Ok((_, worst, got)) if got == FRAMES && worst > 30.0 => {}
                Ok((name, worst, got)) => {
                    failures.push(format!("{name} 10-bit {ten}: {got} frames, {worst:.1} dB"))
                }
                Err(err) => failures.push(format!("{} 10-bit {ten}: {err}", e.name)),
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
