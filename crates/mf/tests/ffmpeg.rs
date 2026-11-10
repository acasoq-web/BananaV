//! FFmpeg on Linux, with no tool around: a movie written with the system's
//! encoders (H.264 and AAC) and BananaV's MP4 writer reads back through
//! bv-mf — the probe tells its streams, each frame decodes where it was
//! (played on and sought), the sound lands on its sample and the video's
//! packets come one per frame. A picture in an icon reads as a still.

#![cfg(all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg)))]

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use bv_core::{ColorInfo, FrameRate, PixelFormat, Plane, PlaneData, Tick, VideoFrame};
use bv_mf::{
    AudioDecoder, AudioEncoder, AudioEncoderSettings, Codec, PacketReader, RateControl, Vendor,
    VideoDecoder, VideoEncoder, VideoEncoderSettings,
};
use bv_mp4::{AudioFormat, AudioTrack, Container, VideoFormat, VideoTrack, Writer};

const W: u32 = 320;
const H: u32 = 180;
const FRAMES: usize = 64;
const RATE: u32 = 48_000;
/// The click of the audio, in samples.
const CLICK: usize = 24_000;

fn out(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bv-ffmpeg-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

/// Frame `n`: its luma tells its number (16 + 3 n), on a faint pattern.
fn frame(n: usize) -> VideoFrame {
    let mut f = VideoFrame::black(W, H, PixelFormat::Yuv420P8, ColorInfo::default());
    if let PlaneData::U8(d) = &mut f.planes[0].data {
        for (i, v) in d.iter_mut().enumerate() {
            *v = (16 + 3 * n + (i % 7) / 3) as u8;
        }
    }
    f
}

/// The number a decoded frame tells (from its mean luma).
fn number(f: &VideoFrame) -> f64 {
    let p: &Plane = &f.planes[0];
    let sum: f64 = match &p.data {
        PlaneData::U8(d) => d.iter().map(|&v| v as f64).sum(),
        PlaneData::U16(d) => d.iter().map(|&v| v as f64 / 4.0).sum(),
    };
    (sum / (p.width * p.height) as f64 - 16.0 - 5.0 / 7.0) / 3.0
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

/// The movie (H.264 of the processor, AAC), made once.
fn movie() -> PathBuf {
    let path = out("movie.mp4");
    let mut venc = VideoEncoder::new(VideoEncoderSettings {
        codec: Codec::H264,
        vendor: Vendor::Software,
        width: W,
        height: H,
        frame_rate: FrameRate::new(30, 1),
        bitrate: 2_000_000,
        max_bitrate: 4_000_000,
        rate_control: RateControl::Vbr,
        gop: 30,
        ten_bit: false,
        field_order: bv_core::FieldOrder::Progressive,
    })
    .expect("an H.264 encoder");
    let mut aenc = AudioEncoder::new(AudioEncoderSettings {
        codec: Codec::Aac,
        sample_rate: RATE,
        channels: 2,
        bitrate: 192_000,
        bits: 16,
    })
    .expect("an AAC encoder");
    let mut w = Writer::new(
        BufWriter::new(File::create(&path).unwrap()),
        Container::Mp4,
        Some(VideoTrack {
            format: VideoFormat::H264,
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
        Writer::<File>::reserve_for(FRAMES as u64, 200),
    )
    .unwrap();
    if let Some(h) = venc.sequence_header() {
        w.add_parameter_sets(&h);
    }
    let pcm = audio();
    let per_frame = 2 * RATE as usize / FRAMES;
    let hns = |n: usize| n as i64 * 10_000_000 / 30;
    let put_video = |w: &mut Writer<_>, packets: Vec<bv_mf::Packet>| {
        for p in packets {
            let pts = (p.time * 30 + 5_000_000) / 10_000_000;
            w.write_video(&p.data, pts, None, 1, p.keyframe).unwrap();
        }
    };
    for n in 0..FRAMES {
        put_video(&mut w, venc.encode(&frame(n), hns(n), hns(1)).unwrap());
        let block = &pcm[n * per_frame * 2..(n + 1) * per_frame * 2];
        for p in aenc.encode(block).unwrap() {
            w.write_audio(&p.data, 1024).unwrap();
        }
    }
    put_video(&mut w, venc.finish().unwrap());
    for p in aenc.finish().unwrap() {
        w.write_audio(&p.data, 1024).unwrap();
    }
    w.set_audio_length(2 * RATE as u64);
    w.finish().unwrap();
    path
}

#[test]
fn a_movie_reads_back_as_it_was_written() {
    let names: Vec<_> = bv_mf::encoders().iter().map(|e| e.name.clone()).collect();
    assert!(
        bv_mf::encoders()
            .iter()
            .any(|e| e.codec == Codec::H264 && e.vendor == Vendor::Software),
        "{names:?}"
    );
    let path = movie();
    let file = path.to_str().unwrap();

    let p = bv_mf::probe(file).unwrap();
    let rate = FrameRate::new(30, 1);
    let frames = (p.duration.as_seconds_f64() * 30.0).round() as usize;
    assert_eq!(frames, FRAMES, "{:?}", p.duration);
    let v = p.video.unwrap();
    assert_eq!((v.width, v.height, v.frame_rate), (W, H, rate));
    assert!(v.decodable);
    assert_eq!(v.codec, bv_mf::codec::H264);
    let a = p.audio.unwrap();
    assert_eq!((a.sample_rate.0, a.channels), (RATE, 2));
    assert!(a.decodable);

    // Played on, then sought back and forth.
    let mut d = VideoDecoder::open(file).unwrap();
    assert!(!d.on_graphics_card());
    let at = |n: usize| rate.frame_start(n as i64) + Tick(rate.frame_duration().0 / 3);
    for n in 0..FRAMES {
        let f = d.frame_at(at(n)).unwrap();
        assert!(
            (number(&f) - n as f64).abs() < 0.5,
            "frame {n}: {}",
            number(&f)
        );
        assert_eq!(f.pts, rate.frame_start(n as i64));
    }
    for n in [50, 10, 33, 0, 63, 31, 32] {
        let f = d.frame_at(at(n)).unwrap();
        assert!(
            (number(&f) - n as f64).abs() < 0.5,
            "sought {n}: {}",
            number(&f)
        );
    }
    // After the end, the last frame stays.
    let f = d.frame_at(Tick::from_seconds(10)).unwrap();
    assert!((number(&f) - (FRAMES - 1) as f64).abs() < 0.5);

    // The click on its sample, read through and read at random.
    let mut ad = AudioDecoder::open(file).unwrap();
    let all = ad.read(0, 2 * RATE as usize).unwrap();
    let click = all.channels[0].iter().position(|&s| s > 0.5);
    assert!(
        click.is_some_and(|c| c.abs_diff(CLICK) <= 2),
        "click at {click:?}"
    );
    let part = ad.read(CLICK as i64 - 4_000, 8_000).unwrap();
    let click = part.channels[1].iter().position(|&s| s > 0.5);
    assert!(
        click.is_some_and(|c| c.abs_diff(4_000) <= 2),
        "click at {click:?}"
    );
    let start = ad.read(0, 1_000).unwrap();
    assert!(start.channels[0].iter().all(|s| s.abs() < 0.2), "the tone");

    // One packet a frame, a keyframe every second.
    let mut r = PacketReader::video(file).unwrap();
    let mut packets = Vec::new();
    while let Some(p) = r.next_packet().unwrap() {
        packets.push(p);
    }
    assert_eq!(packets.len(), FRAMES);
    assert!(packets[0].keyframe && packets[30].keyframe && !packets[1].keyframe);
    assert_eq!(packets[10].time, 3_333_333);
    r.seek(rate.frame_start(40)).unwrap();
    assert_eq!(r.next_packet().unwrap().map(|p| p.time), Some(10_000_000));
}

#[test]
fn a_picture_in_an_icon_is_a_still() {
    // A 4×2 picture, red on the left, half-transparent blue on the right.
    let mut rgba = Vec::new();
    for _ in 0..2 {
        rgba.extend_from_slice(&[
            255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 128, 0, 0, 255, 128,
        ]);
    }
    let png = bv_png::encode_rgba8(4, 2, &rgba, 6);
    let mut ico = vec![0, 0, 1, 0, 1, 0, 4, 2, 0, 0, 1, 0, 32, 0];
    ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());
    ico.extend_from_slice(&png);
    let path = out("picture.ico");
    std::fs::write(&path, ico).unwrap();
    let file = path.to_str().unwrap();
    assert_eq!(bv_mf::still_size(file).unwrap(), (4, 2));
    let s = bv_mf::read_still(file).unwrap();
    assert_eq!((s.width, s.height), (4, 2));
    assert_eq!(&s.rgba[..4], &[65535, 0, 0, 65535]);
    let right = &s.rgba[12..16];
    assert_eq!(right[2], 65535);
    assert!(right[3].abs_diff(128 * 257) < 300, "{right:?}");
}
