//! A long render looked at frame by frame: pieces of a video file, cut
//! anywhere, rendered for as long as asked, then every frame of the result
//! compared with the one of the file it should be. What a decoder, a card
//! or an encoder breaks now and then in a long render shows as a frame a
//! part of which is far from what it should be. Run with a file:
//! `BV_LONG_FILE=<path> cargo test --release -p bv-engine --test long_render -- --ignored --nocapture`;
//! `BV_LONG_MINUTES=30`, `BV_LONG_ENCODER=nvidia|amd|intel|software`,
//! `BV_LONG_OUT=<folder>`, `BV_LONG_EFFECTS=<ids>` (drawn and not compared
//! but for finding frames that jump) change it.
#![cfg(windows)]
#![allow(clippy::print_stdout)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use bv_core::{PlaneData, Tick, TimeRange, VideoFrame};
use bv_engine::{Encoder, Engine, FileFormat, RenderOptions, RenderSettings, VideoParams};
use bv_timeline::{Project, ProjectSettings};

/// A frame's light, each of its numbers the mean of 4 × 4 pixels.
fn small(f: &VideoFrame) -> Vec<u8> {
    let p = &f.planes[0];
    let (w, h) = (f.width as usize / 4, f.height as usize / 4);
    let PlaneData::U8(data) = &p.data else {
        panic!("8-bit video")
    };
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let sum: u32 = (0..4)
                .flat_map(|j| (0..4).map(move |i| (i, j)))
                .map(|(i, j)| u32::from(data[(y * 4 + j) * p.stride + x * 4 + i]))
                .sum();
            out.push((sum / 16) as u8);
        }
    }
    out
}

/// How far apart two frames are where they are furthest: the mean of a
/// block of 16 × 16 of their numbers.
fn furthest(a: &[u8], b: &[u8], w: usize) -> f64 {
    let h = a.len() / w;
    let mut most = 0.0f64;
    for by in (0..h).step_by(16) {
        for bx in (0..w).step_by(16) {
            let (mut sum, mut n) = (0u32, 0u32);
            for y in by..(by + 16).min(h) {
                for x in bx..(bx + 16).min(w) {
                    sum += u32::from(a[y * w + x].abs_diff(b[y * w + x]));
                    n += 1;
                }
            }
            most = most.max(f64::from(sum) / f64::from(n));
        }
    }
    most
}

#[test]
#[ignore = "needs a file: BV_LONG_FILE"]
fn a_long_render_is_whole() {
    let Ok(path) = std::env::var("BV_LONG_FILE") else {
        return;
    };
    let minutes: f64 = std::env::var("BV_LONG_MINUTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30.0);
    let encoder = match std::env::var("BV_LONG_ENCODER").as_deref() {
        Ok("amd") => Encoder::Amd,
        Ok("intel") => Encoder::Intel,
        Ok("software") => Encoder::Software,
        _ => Encoder::Nvidia,
    };
    let dir = std::env::var("BV_LONG_OUT").map_or_else(|_| std::env::temp_dir(), Into::into);
    let info = Engine::probe_file(&path).expect("probes");
    let v = info.video.expect("a video");
    let rate = v.frame_rate;
    let frames = rate.frame_at(info.duration) - 2;
    let mut p = Project::new(ProjectSettings {
        width: v.width,
        height: v.height,
        frame_rate: rate,
        ..ProjectSettings::default()
    });
    let track = p.tracks[0].id;
    let media = p.add_media(info);
    // Pieces of one to nine seconds, from anywhere in the file.
    let total = (minutes * 60.0 * rate.as_f64()) as i64;
    let mut seed = 7u32;
    let mut next = |n: i64| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        i64::from(seed >> 8) % n.max(1)
    };
    let mut wanted: Vec<i64> = Vec::with_capacity(total as usize);
    while (wanted.len() as i64) < total {
        let second = rate.as_f64().round() as i64;
        let len = (second + next(8 * second)).min(total - wanted.len() as i64);
        let len = len.min(frames - 1).max(1);
        let from = next(frames - len);
        let at = rate.frame_start(wanted.len() as i64);
        p.insert_media_range(
            media,
            at,
            rate.frame_start(from),
            rate.frame_start(from + len) - rate.frame_start(from),
            Some(track),
            None,
        )
        .expect("placed");
        wanted.extend(from..from + len);
    }
    if let Ok(list) = std::env::var("BV_LONG_EFFECTS") {
        for t in &mut p.tracks {
            for c in &mut t.clips {
                c.effects = list
                    .split(',')
                    .map(|id| {
                        bv_timeline::Effect::plugin(bv_timeline::PluginFx {
                            id: id.trim().into(),
                            version: [1, 0],
                            name: id.trim().into(),
                            params: Default::default(),
                        })
                    })
                    .collect();
            }
        }
    }
    let out = dir.join(format!("bv-long-render-{}.mp4", std::process::id()));
    let mut engine = Engine::new(Arc::new(|| {}));
    engine.set_hardware_decoding(bv_engine::DecodeChoice::Auto);
    engine.set_gpu(bv_engine::GpuChoice::Auto);
    engine.set_project(&p);
    let start = Instant::now();
    while engine.gpu_starting() && start.elapsed() < Duration::from_secs(60) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let start = Instant::now();
    engine.start_render(RenderSettings {
        format: FileFormat::AvcMp4,
        path: out.clone(),
        range: TimeRange::new(Tick::ZERO, rate.frame_start(wanted.len() as i64)),
        video: Some(VideoParams {
            width: v.width,
            height: v.height,
            frame_rate: rate,
            encoder,
            bitrate: 24_000_000,
            max_bitrate: 40_000_000,
            ..VideoParams::default()
        }),
        audio: None,
        options: RenderOptions::default(),
        project_file: None,
    });
    while !engine.render_progress().is_some_and(|p| p.finished) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let done = engine.render_progress().expect("the render");
    assert!(done.error.is_none(), "{:?}", done.error);
    println!(
        "{} frames of {} events rendered by {} in {:.0} s",
        wanted.len(),
        p.tracks[0].clips.len(),
        done.encoder,
        start.elapsed().as_secs_f64()
    );
    // What they should be, and what they are, both decoded on the
    // processor, one frame after another.
    bv_mf::set_hardware_decoding(bv_mf::HardwareDecoding::Off);
    let mut d = bv_mf::VideoDecoder::open(&path).expect("opens");
    let source: Vec<Vec<u8>> = (0..frames)
        .map(|n| small(&d.frame_at(rate.frame_start(n)).expect("a frame")))
        .collect();
    let w = v.width as usize / 4;
    let mut d = bv_mf::VideoDecoder::open(&out.to_string_lossy()).expect("opens");
    let mut off: Vec<(f64, usize)> = Vec::new();
    for (n, from) in wanted.iter().enumerate() {
        let f = d.frame_at(rate.frame_start(n as i64)).expect("a frame");
        off.push((furthest(&small(&f), &source[*from as usize], w), n));
    }
    let mean = off.iter().map(|o| o.0).sum::<f64>() / off.len() as f64;
    off.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("furthest block of each frame: {mean:.2} of 255 on average; the most:");
    for (o, n) in off.iter().take(12) {
        println!("  {o:.1} at frame {n} ({:.2} s)", *n as f64 / rate.as_f64());
    }
    let broken = off.iter().filter(|o| o.0 > 30.0).count();
    println!("{broken} frames with a block more than 30 off");
    if std::env::var_os("BV_LONG_KEEP").is_none() {
        let _ = std::fs::remove_file(&out);
    }
    if std::env::var_os("BV_LONG_EFFECTS").is_none() {
        assert_eq!(broken, 0);
    }
}
