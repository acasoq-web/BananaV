//! How fast BananaV renders 1080p60: ten seconds of a recording, and of a
//! crossfade of two parts of it, to H.264 in MP4 with AAC, on each encoder
//! of this computer (NVIDIA NVENC, AMD, the processor), the graphics card
//! compositing. Needs ffmpeg:
//! `cargo test --release -p bv-engine --test render_desempenho -- --ignored --nocapture`.

#![allow(clippy::print_stdout)]

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bv_core::{Tick, TimeRange};
use bv_engine::{
    AudioParams, DecodeChoice, Encoder, Engine, FileFormat, GpuChoice, GpuStatus, RateMode,
    RenderOptions, RenderSettings, VideoParams,
};
use bv_timeline::{Project, ProjectSettings, TrackKind};

fn recording() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated/perf");
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join("h264_1080p60_50s_aac.mp4");
    if !p.exists() {
        let st = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi"])
            .args(["-i", "testsrc2=s=1920x1080:r=60000/1001:d=50"])
            .args(["-f", "lavfi", "-i", "sine=f=440:d=50:sample_rate=48000"])
            .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "20"])
            .args([
                "-g", "250", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "160k",
            ])
            .arg(&p)
            .status()
            .expect("ffmpeg on the PATH");
        assert!(st.success());
    }
    p
}

/// The recording from its start; with `crossfade`, a second event of it
/// overlapping the first from 0 to 10 s.
fn project(crossfade: bool) -> Project {
    let info = bv_media::probe_file(recording().to_str().unwrap()).unwrap();
    let v = info.video.unwrap();
    let mut settings = ProjectSettings::default();
    (settings.width, settings.height, settings.frame_rate) = (v.width, v.height, v.frame_rate);
    let mut p = Project::new(settings);
    let video = p.tracks[0].id;
    let audio = p
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Audio)
        .map(|t| t.id);
    let id = p.add_media(info);
    let s = Tick::from_seconds;
    if crossfade {
        p.insert_media_range(id, s(0), s(0), s(15), Some(video), audio)
            .unwrap();
        p.insert_media_range(id, s(0), s(20), s(15), Some(video), audio)
            .unwrap();
    } else {
        p.insert_media_range(id, s(0), s(0), s(15), Some(video), audio)
            .unwrap();
    }
    p
}

/// Renders ten seconds of `p` with `encoder`: frames a second, or why not.
fn render(engine: &mut Engine, p: &Project, encoder: Encoder) -> Result<f64, String> {
    let rate = p.settings.frame_rate;
    let path = std::env::temp_dir().join(format!("bv_render_speed_{encoder:?}.mp4"));
    let range = TimeRange::new(Tick::ZERO, Tick::from_seconds(10));
    engine.set_project(p);
    engine.start_render(RenderSettings {
        format: FileFormat::AvcMp4,
        path: path.clone(),
        range,
        video: Some(VideoParams {
            width: 1920,
            height: 1080,
            frame_rate: rate,
            encoder,
            rate_mode: RateMode::Vbr,
            bitrate: 16_000_000,
            max_bitrate: 24_000_000,
            ..VideoParams::default()
        }),
        audio: Some(AudioParams::default()),
        options: RenderOptions::default(),
        project_file: None,
    });
    let at = Instant::now();
    loop {
        let pr = engine.render_progress().unwrap();
        if pr.finished {
            let _ = std::fs::remove_file(&path);
            if let Some(e) = pr.error {
                return Err(e);
            }
            let frames = (range.duration.as_seconds_f64() * rate.as_f64()).round();
            return Ok(frames / at.elapsed().as_secs_f64());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore = "needs ffmpeg and takes a while"]
fn render_speed_1080p60() {
    let mut engine = Engine::new(Arc::new(|| {}));
    engine.set_hardware_decoding(DecodeChoice::Auto);
    engine.set_gpu(GpuChoice::Auto);
    let start = Instant::now();
    while engine.gpu_status() == GpuStatus::Starting && start.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("compositing: {:?}", engine.gpu_status());
    for crossfade in [false, true] {
        let p = project(crossfade);
        for encoder in [Encoder::Nvidia, Encoder::Amd, Encoder::Software] {
            match render(&mut engine, &p, encoder) {
                Ok(fps) => println!(
                    "{}: {:?} {fps:.1} fps ({:.2}× real time)",
                    if crossfade { "crossfade" } else { "one event" },
                    encoder,
                    fps / p.settings.frame_rate.as_f64()
                ),
                Err(e) => println!("{encoder:?}: {e}"),
            }
        }
    }
}

/// How fast BananaV renders 1080p60 through another maker's plug-in: ten
/// seconds of `BV_SPEED_MEDIA` (the generated recording by default) with
/// `BV_SPEED_PLUGIN` on it (Sapphire's S_ZapFrom at its defaults), to H.264
/// on NVIDIA's encoder; `BV_RENDER_PAINTERS` sets how many painters draw at
/// once. Skipped without the plug-ins (Sapphire's binary by default,
/// `BV_SPEED_BINARY`). Its output goes to a file, never a pipe: Sapphire's
/// preset browser keeps it open.
/// `cargo test --release -p bv-engine --test render_desempenho
/// render_speed_with_a_plugin -- --ignored --nocapture > out.txt`.
#[test]
#[ignore = "needs ffmpeg, another maker's plug-ins and takes a while"]
fn render_speed_with_a_plugin() {
    let binary = std::env::var("BV_SPEED_BINARY").unwrap_or_else(|_| {
        r"C:\Program Files\Common Files\OFX\Plugins\Sapphire.ofx.bundle\Contents\Win64\Sapphire.ofx".into()
    });
    let Ok(loaded) = bv_ofx::Binary::load(std::path::Path::new(&binary)) else {
        println!("no plug-ins at {binary}");
        return;
    };
    bv_ofx::registry().add_built_in(loaded).expect("described");
    let id = std::env::var("BV_SPEED_PLUGIN")
        .unwrap_or_else(|_| "com.genarts.sapphire.Render.S_ZapFrom".into());
    let Some(plugin) = bv_ofx::registry().info(&id) else {
        println!("no {id}");
        return;
    };
    let media = std::env::var("BV_SPEED_MEDIA").map_or_else(|_| recording(), PathBuf::from);
    let info = bv_media::probe_file(media.to_str().unwrap()).unwrap();
    let v = info.video.unwrap();
    let mut settings = ProjectSettings::default();
    (settings.width, settings.height, settings.frame_rate) = (v.width, v.height, v.frame_rate);
    let mut p = Project::new(settings);
    let video = p.tracks[0].id;
    let audio = p
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Audio)
        .map(|t| t.id);
    let m = p.add_media(info);
    let s = Tick::from_seconds;
    let clip = p
        .insert_media_range(m, s(0), s(0), s(12), Some(video), audio)
        .unwrap()[0];
    p.clip_mut(clip).unwrap().effects = vec![bv_timeline::Effect::plugin(bv_timeline::PluginFx {
        id: id.clone(),
        version: plugin.version,
        name: plugin.name().into(),
        params: Default::default(),
    })];
    let mut engine = Engine::new(Arc::new(|| {}));
    engine.set_hardware_decoding(DecodeChoice::Auto);
    engine.set_gpu(GpuChoice::Auto);
    let start = Instant::now();
    while engine.gpu_status() == GpuStatus::Starting && start.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(50));
    }
    println!(
        "compositing: {:?}; painters: {}",
        engine.gpu_status(),
        std::env::var("BV_RENDER_PAINTERS").unwrap_or_else(|_| "default".into())
    );
    match render(&mut engine, &p, Encoder::Nvidia) {
        Ok(fps) => println!(
            "{} with {}: {fps:.1} fps ({:.2}× real time)",
            media.file_name().unwrap().to_string_lossy(),
            plugin.name(),
            fps / p.settings.frame_rate.as_f64()
        ),
        Err(e) => println!("{e}"),
    }
    bv_ofx::unload_all(Duration::from_secs(5));
}
