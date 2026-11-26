//! How fast a render goes: a video file placed whole (and as many times as
//! fill the time asked), rendered to MP4 at a size, then the frames a
//! second. What a slow render on someone's card compares with:
//! `BV_SPEED_FILE=<path> cargo test --release -p bv-engine --test render_speed -- --ignored --nocapture`;
//! `BV_SPEED_SECONDS=60`, `BV_SPEED_SIZE=2560x1440` (the file's own by
//! default), `BV_SPEED_ENCODER=nvidia|amd|intel|software`,
//! `BV_SPEED_QUALITY=best|good|preview|draft`, `BV_SPEED_AUDIO=1` (its
//! sound too), `BV_SPEED_BITRATE=<Mb/s>` change it.
#![cfg(windows)]
#![allow(clippy::print_stdout)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use bv_core::{Tick, TimeRange};
use bv_engine::{
    AudioParams, Encoder, Engine, FileFormat, RenderOptions, RenderSettings, VideoParams,
};
use bv_timeline::{Project, ProjectSettings, RenderQuality};

#[test]
#[ignore = "needs a file: BV_SPEED_FILE"]
fn a_render_goes_this_fast() {
    let Ok(path) = std::env::var("BV_SPEED_FILE") else {
        return;
    };
    let var = |name: &str| std::env::var(name).ok();
    let seconds: f64 = var("BV_SPEED_SECONDS")
        .and_then(|v| v.parse().ok())
        .unwrap_or(60.0);
    let encoder = match var("BV_SPEED_ENCODER").as_deref() {
        Some("amd") => Encoder::Amd,
        Some("intel") => Encoder::Intel,
        Some("software") => Encoder::Software,
        _ => Encoder::Nvidia,
    };
    let quality = match var("BV_SPEED_QUALITY").as_deref() {
        Some("good") => Some(RenderQuality::Good),
        Some("preview") => Some(RenderQuality::Preview),
        Some("draft") => Some(RenderQuality::Draft),
        Some("best") => Some(RenderQuality::Best),
        _ => None,
    };
    let info = Engine::probe_file(&path).expect("probes");
    let v = info.video.expect("a video");
    let (width, height) = var("BV_SPEED_SIZE")
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((v.width, v.height));
    let rate = v.frame_rate;
    let mut p = Project::new(ProjectSettings {
        width,
        height,
        frame_rate: rate,
        ..ProjectSettings::default()
    });
    let video = p.tracks[0].id;
    let audio = p.tracks.get(1).map(|t| t.id);
    let length = info.duration;
    let media = p.add_media(info);
    let end = Tick::from_seconds_f64(seconds);
    let mut at = Tick::ZERO;
    let piece = rate.frame_start(rate.frame_at(length) - 2);
    while at < end {
        let len = piece.min(end - at);
        p.insert_media_range(media, at, Tick::ZERO, len, Some(video), None)
            .expect("placed");
        at += len;
    }
    let _ = audio;
    let range = TimeRange::new(Tick::ZERO, rate.frame_start(rate.frame_at(end)));
    let frames = rate.frame_at(range.duration);
    let out = std::env::temp_dir().join(format!("bv-render-speed-{}.mp4", std::process::id()));
    let mut engine = Engine::new(Arc::new(|| {}));
    engine.set_hardware_decoding(bv_engine::DecodeChoice::Auto);
    engine.set_gpu(bv_engine::GpuChoice::Auto);
    engine.set_project(&p);
    let start = Instant::now();
    while engine.gpu_starting() && start.elapsed() < Duration::from_secs(60) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let mbps: u32 = var("BV_SPEED_BITRATE")
        .and_then(|v| v.parse().ok())
        .unwrap_or(24);
    let start = Instant::now();
    engine.start_render(RenderSettings {
        format: FileFormat::AvcMp4,
        path: out.clone(),
        range,
        video: Some(VideoParams {
            width,
            height,
            frame_rate: rate,
            encoder,
            bitrate: mbps * 1_000_000,
            max_bitrate: mbps * 2_000_000,
            rendering_quality: quality,
            ..VideoParams::default()
        }),
        audio: var("BV_SPEED_AUDIO").map(|_| AudioParams::default()),
        options: RenderOptions::default(),
        project_file: None,
    });
    // Every five seconds: how far, how fast lately (a render that stops
    // shows here).
    let mut last = (Instant::now(), 0.0f64);
    loop {
        std::thread::sleep(Duration::from_millis(250));
        let Some(now) = engine.render_progress() else {
            continue;
        };
        if now.finished {
            break;
        }
        if last.0.elapsed() >= Duration::from_secs(5) {
            let done = now.done as f64;
            let fps = (done - last.1) / last.0.elapsed().as_secs_f64();
            println!(
                "{:5.1} %  {fps:6.1} frames a second lately",
                100.0 * done / now.total.max(1) as f64
            );
            last = (Instant::now(), done);
        }
    }
    let done = engine.render_progress().expect("the render");
    assert!(done.error.is_none(), "{:?}", done.error);
    let s = start.elapsed().as_secs_f64();
    println!(
        "{frames} frames of {width}x{height} at {:.3} by {} in {s:.1} s: {:.1} frames a second",
        rate.as_f64(),
        done.encoder,
        frames as f64 / s
    );
    let _ = std::fs::remove_file(&out);
}
