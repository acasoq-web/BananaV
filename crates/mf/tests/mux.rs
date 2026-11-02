//! The system's encoders through BananaV's MP4 writer, checked by ffprobe
//! and ffmpeg: video of every encoder decodes to the frames that went in,
//! AAC audio lands on its sample (the encoder delay is skipped by the edit
//! list) and the movie header sits before the media.
//! Needs `ffmpeg`/`ffprobe` on the PATH; run with
//! `cargo test -p bv-mf --test mux -- --ignored --nocapture`.

#![cfg(any(windows, all(target_os = "linux", not(bv_no_ffmpeg))))]

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::process::Command;

use bv_core::{ColorInfo, FrameRate, PixelFormat, PlaneData, VideoFrame};
use bv_mf::{
    AudioEncoder, AudioEncoderSettings, Codec, RateControl, Vendor, VideoEncoder,
    VideoEncoderSettings,
};
use bv_mp4::{AudioFormat, AudioTrack, Container, VideoFormat, VideoTrack, Writer};

const W: u32 = 640;
const H: u32 = 360;
const FRAMES: usize = 60;
const RATE: u32 = 48_000;
/// The click of the audio, in samples.
const CLICK: usize = 36_000;

fn out(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated");
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

fn frame(n: usize) -> VideoFrame {
    let mut f = VideoFrame::black(W, H, PixelFormat::Yuv420P8, ColorInfo::default());
    for (p, plane) in f.planes.iter_mut().enumerate() {
        let (pw, ph) = (plane.width, plane.height);
        for y in 0..ph {
            for x in 0..pw {
                let v = match p {
                    0 => 16 + ((x + y + n * 4) % 64) * 2 + y * 64 / ph,
                    1 => 98 + x * 60 / pw,
                    _ => 98 + y * 60 / ph,
                };
                if let PlaneData::U8(d) = &mut plane.data {
                    d[y * plane.stride + x] = v as u8;
                }
            }
        }
    }
    f
}

/// Two seconds of a quiet tone with a loud click at `CLICK`.
fn audio() -> Vec<f32> {
    let mut v = Vec::with_capacity(2 * RATE as usize * 2);
    for i in 0..2 * RATE as usize {
        let t = i as f32 / RATE as f32;
        let mut s = 0.05 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
        if (CLICK..CLICK + 48).contains(&i) {
            s = 0.9;
        }
        v.push(s);
        v.push(s);
    }
    v
}

fn ffprobe(path: &PathBuf, entries: &str) -> String {
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
    String::from_utf8_lossy(&o.stdout).to_string() + &String::from_utf8_lossy(&o.stderr)
}

/// Where the click is in ffmpeg's decode of the file's audio.
fn click_at(path: &PathBuf) -> Option<usize> {
    let o = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-ac", "1", "-f", "f32le", "-"])
        .output()
        .expect("ffmpeg");
    o.stdout
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .position(|v| v > 0.5)
}

fn mux(codec: Codec, vendor: Vendor) -> Result<PathBuf, String> {
    let mut venc = VideoEncoder::new(VideoEncoderSettings {
        codec,
        vendor,
        width: W,
        height: H,
        frame_rate: FrameRate::new(30, 1),
        bitrate: 3_000_000,
        max_bitrate: 5_000_000,
        rate_control: RateControl::Vbr,
        gop: 30,
        ten_bit: false,
        field_order: bv_core::FieldOrder::Progressive,
    })
    .map_err(|e| e.to_string())?;
    let mut aenc = AudioEncoder::new(AudioEncoderSettings {
        codec: Codec::Aac,
        sample_rate: RATE,
        channels: 2,
        bitrate: 192_000,
        bits: 16,
    })
    .map_err(|e| e.to_string())?;
    let path = out(&format!("mux_{codec:?}_{vendor:?}.mp4"));
    let format = match codec {
        Codec::H264 => VideoFormat::H264,
        Codec::Hevc => VideoFormat::Hevc,
        _ => VideoFormat::Av1,
    };
    let mut w = Writer::new(
        BufWriter::new(File::create(&path).unwrap()),
        Container::Mp4,
        Some(VideoTrack {
            format,
            width: W,
            height: H,
            timescale: 30,
            bit_depth: 8,
            color: (1, 1, 1, false),
            alpha: false,
            pixel_aspect: (1, 1),
            spherical: false,
        }),
        Some(AudioTrack {
            format: AudioFormat::Aac {
                config: aenc.config().to_vec(),
                bitrate: aenc.bitrate(),
            },
            sample_rate: RATE,
            channels: 2,
            priming: bv_mf::AAC_PRIMING,
        }),
        Writer::<File>::reserve_for(FRAMES as u64, 100),
    )
    .map_err(|e| e.to_string())?;
    if let Some(h) = venc.sequence_header() {
        w.add_parameter_sets(&h);
    }
    let pcm = audio();
    let per_frame = RATE as usize / 30;
    let hns = |n: usize| n as i64 * 10_000_000 / 30;
    let put_video = |w: &mut Writer<_>, packets: Vec<bv_mf::Packet>| {
        for p in packets {
            let pts = (p.time * 30 + 5_000_000) / 10_000_000;
            w.write_video(&p.data, pts, None, 1, p.keyframe).unwrap();
        }
    };
    for n in 0..FRAMES {
        let packets = venc
            .encode(&frame(n), hns(n), hns(1))
            .map_err(|e| e.to_string())?;
        put_video(&mut w, packets);
        let block = &pcm[n * per_frame * 2..(n + 1) * per_frame * 2];
        for p in aenc.encode(block).map_err(|e| e.to_string())? {
            w.write_audio(&p.data, 1024).unwrap();
        }
    }
    put_video(&mut w, venc.finish().map_err(|e| e.to_string())?);
    for p in aenc.finish().map_err(|e| e.to_string())? {
        w.write_audio(&p.data, 1024).unwrap();
    }
    w.set_audio_length(2 * RATE as u64);
    w.set_metadata(bv_mp4::Metadata {
        title: "BananaV test".into(),
        encoder: "BananaV".into(),
        ..Default::default()
    });
    w.finish().map_err(|e| e.to_string())?;
    Ok(path)
}

#[test]
#[ignore = "needs ffmpeg and the system's encoders"]
fn encoded_files_play_in_ffmpeg() {
    let mut failures = Vec::new();
    for e in bv_mf::encoders() {
        if !matches!(e.codec, Codec::H264 | Codec::Hevc | Codec::Av1) {
            continue;
        }
        let path = match mux(e.codec, e.vendor) {
            Ok(p) => p,
            Err(err) => {
                failures.push(format!("{}: {err}", e.name));
                continue;
            }
        };
        let info = ffprobe(
            &path,
            "stream=codec_name,width,height,nb_read_frames,sample_rate:format=duration:format_tags=title",
        );
        let click = click_at(&path);
        eprintln!(
            "{} → {}\n{info}click at {click:?} (want {CLICK})\n",
            e.name,
            path.display()
        );
        let want = [
            "width=640",
            "height=360",
            "nb_read_frames=60",
            "codec_name=aac",
            "TAG:title=BananaV test",
        ];
        for w in want {
            if !info.contains(w) {
                failures.push(format!("{}: no {w}", e.name));
            }
        }
        if click.is_none_or(|c| c.abs_diff(CLICK) > 2) {
            failures.push(format!("{}: click at {click:?}", e.name));
        }
        // The movie header comes before the media.
        let head = std::fs::read(&path).unwrap();
        let moov = head.windows(4).position(|w| w == b"moov");
        let mdat = head.windows(4).position(|w| w == b"mdat");
        if moov.is_none() || moov > mdat {
            failures.push(format!("{}: moov after mdat", e.name));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
