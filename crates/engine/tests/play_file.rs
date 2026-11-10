//! How fast the preview plays a file of the user's: its video and audio on
//! the timeline of a project (1080p 59.94 by default), played for a while
//! once the graphics card has compiled its kernels, as the Video Preview
//! asks for frames. Prints the different frames shown a second and the
//! longest wait between them. Run with a file:
//! `BV_PLAY_FILE=<path> cargo test --release -p bv-engine --test play_file -- --ignored --nocapture`.
//! `BV_PLAY_PROJECT=3840x2160@59.94`, `BV_PLAY_SIZE=1920x1080` (the size
//! frames are drawn at: Auto's room, or the project's at Full),
//! `BV_PLAY_ROOM=1280x720` (the screen's pixels the frames show in, which
//! frames drawn larger come back reduced to), `BV_PLAY_FROM=10` (seconds),
//! `BV_PLAY_SECONDS=8`, `BV_PLAY_WARM=10` (seconds the graphics card's
//! kernels get to compile first), `BV_GPU=off`, `BV_DECODING=off` change
//! it.
//! Heavier frames: `BV_PLAY_EFFECTS=org.bananav.GaussianBlur,org.bananav.Glow`
//! stacks those effects on the events, `BV_PLAY_TRANSITION=dissolve` (or a
//! transition's id, `org.bananav.Push`) plays a transition between two
//! events of the file all along, and `BV_PLAY_ANIMATE=1` zooms and turns
//! the picture with a Pan/Crop animation.

#![allow(clippy::print_stdout)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use bv_core::{FrameRate, Tick};
use bv_timeline::{Project, ProjectSettings};

fn env_pair(name: &str, default: (u32, u32)) -> (u32, u32) {
    std::env::var(name)
        .ok()
        .and_then(|v| {
            let v = v.split('@').next()?.to_string();
            let (w, h) = v.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or(default)
}

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[test]
#[ignore = "needs a file: BV_PLAY_FILE"]
fn playing_a_file() {
    let Ok(path) = std::env::var("BV_PLAY_FILE") else {
        return;
    };
    let info = bv_media::probe_file(&path).expect("probes");
    let mut settings = ProjectSettings::default();
    let (w, h) = env_pair("BV_PLAY_PROJECT", (1920, 1080));
    let rate = std::env::var("BV_PLAY_PROJECT")
        .ok()
        .and_then(|v| v.split_once('@').and_then(|(_, r)| r.parse::<f64>().ok()))
        .unwrap_or(59.94);
    settings.width = w;
    settings.height = h;
    settings.frame_rate = if (rate - 59.94).abs() < 0.01 {
        FrameRate::FPS_59_94
    } else if (rate - 29.97).abs() < 0.01 {
        FrameRate::FPS_29_97
    } else {
        FrameRate::new(rate.round() as u32, 1)
    };
    let mut p = Project::new(settings);
    let video = p.tracks[0].id;
    let audio = p
        .tracks
        .iter()
        .find(|t| t.kind == bv_timeline::TrackKind::Audio)
        .map(|t| t.id);
    let length = info.duration;
    let media_size = info.video.map(|v| (v.width, v.height));
    let id = p.add_media(info);
    let from_s = env_f64("BV_PLAY_FROM", 10.0);
    let seconds_s = env_f64("BV_PLAY_SECONDS", 8.0);
    let transition = std::env::var("BV_PLAY_TRANSITION").ok();
    if transition.is_some() {
        // Two events of the file overlapping from a second before playback
        // to a second after it: a transition all along.
        let s = Tick::from_seconds_f64;
        let end = s(from_s + seconds_s + 1.0).min(length);
        let second = s((from_s - 1.0).max(0.0));
        p.insert_media_range(id, Tick::ZERO, Tick::ZERO, end, Some(video), audio)
            .expect("placed");
        p.insert_media_range(id, second, second, length - second, Some(video), audio)
            .expect("placed");
    } else {
        p.insert_media_range(id, Tick::ZERO, Tick::ZERO, length, Some(video), audio)
            .expect("placed");
    }
    let fx = |id: &str| bv_timeline::PluginFx {
        id: id.into(),
        version: [1, 0],
        name: id.rsplit('.').next().unwrap_or(id).into(),
        params: Default::default(),
    };
    let effects: Vec<String> = std::env::var("BV_PLAY_EFFECTS")
        .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();
    let animate = std::env::var("BV_PLAY_ANIMATE").is_ok_and(|v| v == "1");
    let clips: Vec<bv_timeline::ClipId> = p.tracks[0].clips.iter().map(|c| c.id).collect();
    for (k, &c) in clips.iter().enumerate() {
        let clip = p.clip_mut(c).expect("an event");
        clip.effects = effects
            .iter()
            .map(|e| bv_timeline::Effect::plugin(fx(e)))
            .collect();
        if k == 1
            && let Some(t) = transition.as_deref().filter(|t| *t != "dissolve")
        {
            clip.transition_fx = Some(Box::new(fx(t)));
        }
        // From the whole picture to a turned third of it over the part
        // played.
        if animate && let Some((mw, mh)) = media_size {
            let (mw, mh) = (f64::from(mw), f64::from(mh));
            let at = |s: f64| Tick::from_seconds_f64(s) - clip.start;
            let key = |t: Tick, k: f64, angle: f64| bv_timeline::pancrop::PanCropKey {
                at: t,
                curve: Default::default(),
                width: mw * k,
                height: mh * k,
                center: [mw / 2.0, mh / 2.0],
                angle,
                pivot: [mw / 2.0, mh / 2.0],
                smoothness: 0.0,
                crop: [0.0; 4],
            };
            clip.pan_crop.size = [mw as u32, mh as u32];
            clip.pan_crop.keys = vec![
                key(at(from_s).max(Tick::ZERO), 1.0, 0.0),
                key(at(from_s + seconds_s), 0.35, 0.6),
            ];
        }
    }
    let gpu = if std::env::var("BV_GPU").is_ok_and(|v| v == "off") {
        bv_engine::GpuChoice::Off
    } else {
        bv_engine::GpuChoice::Auto
    };
    let decoding = if std::env::var("BV_DECODING").is_ok_and(|v| v == "off") {
        bv_engine::DecodeChoice::Off
    } else {
        bv_engine::DecodeChoice::Auto
    };
    let mut engine = bv_engine::Engine::new(Arc::new(|| {}));
    engine.set_hardware_decoding(decoding.clone());
    engine.set_gpu(gpu.clone());
    // The card open and its kernels compiled, as after a minute of use.
    let start = Instant::now();
    while engine.gpu_starting() && start.elapsed() < Duration::from_secs(60) {
        std::thread::sleep(Duration::from_millis(20));
    }
    // Every kernel compiled meanwhile (`BV_PLAY_WARM` seconds), as in a
    // BananaV open for a while: frames using one still compiling wait.
    if gpu != bv_engine::GpuChoice::Off {
        std::thread::sleep(Duration::from_secs_f64(env_f64("BV_PLAY_WARM", 10.0)));
    }
    engine.set_project(&p);
    let (pw, ph) = env_pair("BV_PLAY_SIZE", (1920, 1080));
    let room = env_pair("BV_PLAY_ROOM", (0, 0));
    let from = Tick::from_seconds_f64(from_s);
    let seconds = seconds_s;
    // The frame where playback starts drawn first, as a still would be.
    engine.request_preview_in(from, pw, ph, room);
    let wait = Instant::now();
    while engine.preview_frame().is_none_or(|f| !f.exact)
        && wait.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    engine.play(from, bv_engine::PlayOptions::default());
    let mut shown: Vec<(Tick, Instant)> = Vec::new();
    let start = Instant::now();
    while start.elapsed().as_secs_f64() < seconds {
        if let Some(t) = engine.position() {
            engine.request_preview_in(t, pw, ph, room);
        }
        if let Some(f) = engine.preview_frame()
            && shown.last().is_none_or(|s| s.0 != f.time)
        {
            shown.push((f.time, Instant::now()));
        }
        // A 60 Hz display's paints.
        std::thread::sleep(Duration::from_micros(16_600));
    }
    engine.stop();
    // The first second left out: readers open, plug-ins get ready.
    let settled: Vec<&(Tick, Instant)> = shown
        .iter()
        .filter(|s| s.1.duration_since(start).as_secs_f64() >= 1.0)
        .collect();
    let span = seconds - 1.0;
    let gaps: Vec<f64> = settled
        .windows(2)
        .map(|w| w[1].1.duration_since(w[0].1).as_secs_f64() * 1e3)
        .collect();
    let longest = gaps.iter().copied().fold(0.0, f64::max);
    println!(
        "{path}: project {w}x{h} at {:.3}, preview {pw}x{ph} in {room:?}, effects {effects:?}, transition {transition:?}, animated {animate}, {gpu:?} / decoding {decoding:?} ({:?}): {:.1} different frames a second, longest wait {longest:.0} ms",
        p.settings.frame_rate.as_f64(),
        engine.gpu_status(),
        settled.len() as f64 / span,
    );
}
