//! How fast the preview draws a 1080p60 recording: one event, and a
//! crossfade between two events of the same file (each needs a decoder of
//! its own, or every frame seeks twice), on the processor and on the
//! graphics card. Needs ffmpeg (and a graphics card for that part):
//! `cargo test --release -p bv-engine --test desempenho -- --ignored --nocapture`.

#![allow(clippy::print_stdout)]

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use bv_core::{Tick, TransferCharacteristic};
use bv_gpu::{Compositor, Gpu, Output, Rendered};
use bv_render::VideoPool;
use bv_timeline::{Project, ProjectSettings};

fn recording() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated/perf");
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join("h264_1080p60_50s_aac.mp4");
    if !p.exists() {
        // Keyframes 250 frames apart, as screen recorders often make them.
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

/// Frames a second drawing 90 frames from `start` at `w` × `h`.
fn fps(p: &Project, start: Tick, (w, h): (u32, u32), card: Option<&mut Compositor>) -> f64 {
    let rate = p.settings.frame_rate;
    let first = rate.snap_floor(start);
    let mut pool = VideoPool::new();
    let mut card = card;
    let mut frame = |t: Tick, pool: &mut VideoPool| -> Vec<u8> {
        match card.as_deref_mut() {
            Some(c) => {
                let scene = bv_timeline::scene::frame_scene(p, t, w, h);
                let out = Output::Display(TransferCharacteristic::Bt709);
                match c.render(&scene, p.settings.rotation, pool, [0.0, 0.0, 0.0, 1.0], out) {
                    Ok(Rendered::Display { rgba, .. }) => rgba,
                    other => panic!("the card failed: {other:?}"),
                }
            }
            None => bv_color::linear_to_display_rgba8(
                &bv_render::render_output(p, t, w, h, pool),
                TransferCharacteristic::Bt709,
            ),
        }
    };
    frame(first, &mut pool);
    let n = 90;
    let at = Instant::now();
    for k in 1..=n {
        std::hint::black_box(frame(first + rate.frame_start(k), &mut pool));
    }
    n as f64 / at.elapsed().as_secs_f64()
}

/// Two events of the same recording, video and audio: the second shows the
/// file 5 s ahead of the first and they overlap from 20 to 40 s.
fn crossfade_project() -> Project {
    let path = recording();
    let info = bv_media::probe_file(path.to_str().unwrap()).unwrap();
    let v = info.video.unwrap();
    let mut settings = ProjectSettings::default();
    (settings.width, settings.height, settings.frame_rate) = (v.width, v.height, v.frame_rate);
    let mut p = Project::new(settings);
    let video = p.tracks[0].id;
    let audio = p
        .tracks
        .iter()
        .find(|t| t.kind == bv_timeline::TrackKind::Audio)
        .map(|t| t.id);
    let id = p.add_media(info);
    let s = Tick::from_seconds;
    p.insert_media_range(id, s(0), s(0), s(40), Some(video), audio)
        .unwrap();
    p.insert_media_range(id, s(20), s(25), s(20), Some(video), audio)
        .unwrap();
    p
}

#[test]
#[ignore = "needs ffmpeg"]
fn audio_of_a_crossfade_of_one_recording() {
    let p = crossfade_project();
    let rate = p.settings.sample_rate;
    let mut pool = bv_mixer::AudioPool::new();
    // Ten seconds mixed as playback does, 1024 samples at a time.
    let speed = |from: i64, pool: &mut bv_mixer::AudioPool| {
        let n = rate.0 as i64 * 10 / 1024;
        let at = Instant::now();
        for k in 0..n {
            std::hint::black_box(bv_mixer::mix(&p, from + k * 1024, 1024, pool));
        }
        10.0 / at.elapsed().as_secs_f64()
    };
    let single = speed(rate.0 as i64 * 5, &mut pool);
    let crossfade = speed(rate.0 as i64 * 25, &mut pool);
    println!("audio mixed {single:.0}× real time with one event, {crossfade:.0}× in the crossfade");
    assert!(
        crossfade * 3.0 > single,
        "the crossfade's events must not share a reader"
    );
}

#[test]
#[ignore = "needs ffmpeg"]
fn crossfade_of_one_recording() {
    let p = crossfade_project();
    let s = Tick::from_seconds;
    let mut card = Gpu::open(None).ok().map(|g| {
        g.warm_up().unwrap();
        println!("graphics card: {}", g.adapter().name);
        Compositor::new(g)
    });
    for size in [(720, 405), (1920, 1080)] {
        let single = fps(&p, s(5), size, None);
        let crossfade = fps(&p, s(25), size, None);
        println!(
            "{}x{} processor: one event {single:.0} fps, crossfade {crossfade:.0} fps",
            size.0, size.1
        );
        assert!(
            crossfade * 3.0 > single,
            "the crossfade's events must not share a decoder"
        );
        if let Some(c) = card.as_mut() {
            let single = fps(&p, s(5), size, Some(c));
            let crossfade = fps(&p, s(25), size, Some(c));
            println!(
                "{}x{} graphics card: one event {single:.0} fps, crossfade {crossfade:.0} fps",
                size.0, size.1
            );
            assert!(crossfade * 3.0 > single);
        }
    }
}

/// Playback as the interface sees it: the engine plays the crossfade (the
/// audio clock leads) while the screen asks for the frame at the playing
/// position 60 times a second; how many different frames come a second.
#[test]
#[ignore = "needs ffmpeg"]
fn playing_the_crossfade() {
    use std::sync::Arc;
    use std::time::Duration;

    let p = crossfade_project();
    for (gpu, decoding) in [
        (bv_engine::GpuChoice::Off, bv_engine::DecodeChoice::Off),
        (bv_engine::GpuChoice::Auto, bv_engine::DecodeChoice::Off),
        (bv_engine::GpuChoice::Off, bv_engine::DecodeChoice::Auto),
        (bv_engine::GpuChoice::Auto, bv_engine::DecodeChoice::Auto),
    ] {
        let mut engine = bv_engine::Engine::new(Arc::new(|| {}));
        engine.set_hardware_decoding(decoding.clone());
        engine.set_gpu(gpu.clone());
        let start = Instant::now();
        while matches!(engine.gpu_status(), bv_engine::GpuStatus::Starting)
            && start.elapsed() < Duration::from_secs(20)
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        engine.set_project(&p);
        engine.play(Tick::from_seconds(22), bv_engine::PlayOptions::default());
        let mut shown = Vec::new();
        let mut asked = Vec::new();
        let start = Instant::now();
        let seconds = 6.0;
        let mut iterations = 0;
        while start.elapsed().as_secs_f64() < seconds {
            iterations += 1;
            if let Some(t) = engine.position() {
                engine.request_preview(t, 720, 405);
                let t = p.settings.frame_rate.snap_floor(t);
                if asked.last() != Some(&t) {
                    asked.push(t);
                }
            }
            if let Some(f) = engine.preview_frame()
                && shown.last() != Some(&f.time)
            {
                shown.push(f.time);
            }
            if std::env::var_os("BV_PREVIEW_TRACE").is_some() {
                println!(
                    "poll {:.0} ms: asked {:?}, showing {:?}",
                    start.elapsed().as_secs_f64() * 1e3,
                    asked.last().map(|t| t.as_seconds_f64()),
                    shown.last().map(|t| t.as_seconds_f64())
                );
            }
            std::thread::sleep(Duration::from_millis(16));
        }
        let played = engine.stop().unwrap_or(Tick::ZERO) - Tick::from_seconds(22);
        // The first second opens the readers.
        let fps = shown.len() as f64 / seconds;
        println!(
            "{gpu:?} / decoding {decoding:?} ({:?}): {fps:.0} different frames a second ({:.0} asked for, {:.0} screen refreshes), {:.2} s played in {seconds} s",
            engine.gpu_status(),
            asked.len() as f64 / seconds,
            iterations as f64 / seconds,
            played.as_seconds_f64(),
        );
        // On the graphics card nearly every frame asked for shows.
        if gpu == bv_engine::GpuChoice::Auto
            && matches!(engine.gpu_status(), bv_engine::GpuStatus::Ready(_))
        {
            assert!(
                shown.len() as f64 >= asked.len() as f64 * 0.8,
                "playback keeps up"
            );
        }
    }
}

/// Playback through the start of a crossfade: the second event's decoder
/// must be ready when it starts (its first frame is 3 s into a 4-second
/// group of pictures, so it needs a seek). The longest wait between new
/// frames on the screen.
#[test]
#[ignore = "needs ffmpeg"]
fn playing_into_a_crossfade() {
    use std::sync::Arc;
    use std::time::Duration;

    let path = recording();
    let info = bv_media::probe_file(path.to_str().unwrap()).unwrap();
    let v = info.video.unwrap();
    let mut settings = ProjectSettings::default();
    (settings.width, settings.height, settings.frame_rate) = (v.width, v.height, v.frame_rate);
    let mut p = Project::new(settings);
    let video = p.tracks[0].id;
    let audio = p
        .tracks
        .iter()
        .find(|t| t.kind == bv_timeline::TrackKind::Audio)
        .map(|t| t.id);
    let id = p.add_media(info);
    let s = Tick::from_seconds;
    p.insert_media_range(id, s(0), s(0), s(30), Some(video), audio)
        .unwrap();
    p.insert_media_range(id, s(20), s(3), s(20), Some(video), audio)
        .unwrap();
    for (gpu, decoding) in [
        (bv_engine::GpuChoice::Off, bv_engine::DecodeChoice::Off),
        (bv_engine::GpuChoice::Auto, bv_engine::DecodeChoice::Auto),
    ] {
        let mut engine = bv_engine::Engine::new(Arc::new(|| {}));
        engine.set_hardware_decoding(decoding.clone());
        engine.set_gpu(gpu.clone());
        let start = Instant::now();
        while matches!(engine.gpu_status(), bv_engine::GpuStatus::Starting)
            && start.elapsed() < Duration::from_secs(20)
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        engine.set_project(&p);
        engine.play(s(17), bv_engine::PlayOptions::default());
        let mut last_new: Option<(Instant, Tick)> = None;
        let mut worst = (Duration::ZERO, Tick::ZERO);
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if let Some(t) = engine.position() {
                engine.request_preview(t, 720, 405);
            }
            if let Some(f) = engine.preview_frame()
                && last_new.is_none_or(|(_, t)| t != f.time)
            {
                let now = Instant::now();
                // After the first second (opening the first decoder).
                if let Some((at, _)) = last_new
                    && start.elapsed() > Duration::from_secs(1)
                    && now - at > worst.0
                {
                    worst = (now - at, f.time);
                }
                last_new = Some((now, f.time));
            }
            std::thread::sleep(Duration::from_millis(4));
        }
        engine.stop();
        println!(
            "{gpu:?} / decoding {decoding:?}: the longest wait for a new frame was {:.0} ms (at {:.2} s)",
            worst.0.as_secs_f64() * 1000.0,
            worst.1.as_seconds_f64()
        );
    }
}

/// Playback through a one-second transition between two events of the
/// 1080p60 recording (the second starts 3 s into its file: its decoder
/// must seek), as the interface sees it at 1280 × 720: the longest wait for
/// a new frame on the screen and the frames a second before, at the
/// start, in the middle and at the end of it, and after. A plain
/// crossfade, BananaV's own Linear Wipe (on the card), a GL Transition (on
/// the processor) and, where it is installed, Sapphire's S_DissolveBlur.
/// `BV_TRANSITION_SIZE` (`1920x1080`) changes the preview's size;
/// `BV_PREVIEW_TRACE=1` prints each frame drawn, and each one shown.
#[test]
#[ignore = "needs ffmpeg"]
fn playing_through_a_transition() {
    use std::sync::Arc;
    use std::time::Duration;

    let path = recording();
    let info = bv_media::probe_file(path.to_str().unwrap()).unwrap();
    let v = info.video.unwrap();
    let mut settings = ProjectSettings::default();
    (settings.width, settings.height, settings.frame_rate) = (v.width, v.height, v.frame_rate);
    let mut base = Project::new(settings);
    let video = base.tracks[0].id;
    let audio = base
        .tracks
        .iter()
        .find(|t| t.kind == bv_timeline::TrackKind::Audio)
        .map(|t| t.id);
    let id = base.add_media(info);
    let s = Tick::from_seconds;
    base.insert_media_range(id, s(0), s(0), s(21), Some(video), audio)
        .unwrap();
    let second = base
        .insert_media_range(id, s(20), s(3), s(10), Some(video), audio)
        .unwrap()[0];
    let sapphire = std::path::Path::new(
        r"C:\Program Files\Common Files\OFX\Plugins\Sapphire.ofx.bundle\Contents\Win64\Sapphire.ofx",
    );
    let third_party = bv_ofx::Binary::load(sapphire)
        .ok()
        .and_then(|b| bv_ofx::registry().add_built_in(b).ok())
        .is_some();
    let mut kinds = vec![
        ("crossfade", None),
        ("Linear Wipe", Some(("org.bananav.LinearWipe", [1, 0]))),
        ("GL Transition", Some(("org.bananav.GLTransition", [2, 0]))),
    ];
    if third_party {
        kinds.push((
            "S_DissolveBlur",
            Some(("com.genarts.sapphire.Transitions.S_DissolveBlur", [0, 0])),
        ));
    }
    let (w, h) = std::env::var("BV_TRANSITION_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((1280, 720));
    let shown_trace = std::env::var_os("BV_PREVIEW_TRACE").is_some();
    let mut engine = bv_engine::Engine::new(Arc::new(|| {}));
    engine.set_hardware_decoding(bv_engine::DecodeChoice::Auto);
    engine.set_gpu(bv_engine::GpuChoice::Auto);
    let start = Instant::now();
    while matches!(engine.gpu_status(), bv_engine::GpuStatus::Starting)
        && start.elapsed() < Duration::from_secs(20)
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    println!("{:?}, {w}x{h}", engine.gpu_status());
    // `BV_TRANSITION_KINDS` (names, commas) plays only some of them.
    let only = std::env::var("BV_TRANSITION_KINDS").ok();
    kinds.retain(|(name, _)| {
        only.as_ref()
            .is_none_or(|o| o.split(',').any(|k| k == *name))
    });
    for (name, fx) in kinds {
        let mut p = base.clone();
        p.clip_mut(second).unwrap().transition_fx = fx.map(|(id, version)| {
            let version = bv_ofx::registry().info(id).map_or(version, |i| i.version);
            Box::new(bv_timeline::PluginFx {
                id: id.into(),
                version,
                name: name.into(),
                params: Default::default(),
            })
        });
        engine.set_project(&p);
        // The first frame, then playback from 17.5 s.
        engine.request_preview(Tick::from_seconds_f64(17.5), w, h);
        let first = Instant::now();
        while engine.preview_frame().is_none() && first.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(5));
        }
        engine.play(
            Tick::from_seconds_f64(17.5),
            bv_engine::PlayOptions::default(),
        );
        // Before, the start, the middle, the end, after.
        let parts = [19.9, 20.25, 20.75, 21.25, f64::MAX];
        let mut worst = [Duration::ZERO; 5];
        let mut frames = [0usize; 5];
        let mut last: Option<(Instant, Tick)> = None;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs_f64(5.5) {
            let position = engine.position();
            if let Some(t) = position {
                engine.request_preview(t, w, h);
            }
            if let Some(f) = engine.preview_frame()
                && last.is_none_or(|(_, t)| t != f.time)
            {
                let now = Instant::now();
                if shown_trace {
                    println!(
                        "shown {:.0} ms: {:.3} s (playing at {:.3} s)",
                        start.elapsed().as_secs_f64() * 1e3,
                        f.time.as_seconds_f64(),
                        position.unwrap_or(Tick::ZERO).as_seconds_f64()
                    );
                }
                let part = parts
                    .iter()
                    .position(|&end| f.time.as_seconds_f64() < end)
                    .unwrap_or(4);
                frames[part] += 1;
                if let Some((at, _)) = last
                    && start.elapsed() > Duration::from_millis(500)
                {
                    worst[part] = worst[part].max(now - at);
                }
                last = Some((now, f.time));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        engine.stop();
        let lengths = [2.4, 0.35, 0.5, 0.5, 1.75];
        let report: Vec<String> = ["before", "start", "middle", "end", "after"]
            .iter()
            .enumerate()
            .map(|(i, part)| {
                format!(
                    "{part} {:.0} fps (longest wait {:.0} ms)",
                    frames[i] as f64 / lengths[i],
                    worst[i].as_secs_f64() * 1000.0
                )
            })
            .collect();
        println!("{name}: {}", report.join(", "));
    }
    drop(engine);
    bv_ofx::unload_all(Duration::from_secs(5));
}

/// Scrubbing: the needle dragged across a recording with a keyframe every
/// 4 seconds (every new position needs a seek), as the interface asks: 60
/// times a second. How often the preview changes while dragging, how far
/// behind the needle it is, and how long the exact frame takes once the
/// needle stops.
#[test]
#[ignore = "needs ffmpeg"]
fn scrubbing_a_recording() {
    use std::sync::Arc;
    use std::time::Duration;

    let p = crossfade_project();
    let rate = p.settings.frame_rate;
    for (gpu, decoding) in [
        (bv_engine::GpuChoice::Off, bv_engine::DecodeChoice::Off),
        (bv_engine::GpuChoice::Auto, bv_engine::DecodeChoice::Auto),
    ] {
        let mut engine = bv_engine::Engine::new(Arc::new(|| {}));
        engine.set_hardware_decoding(decoding.clone());
        engine.set_gpu(gpu.clone());
        let start = Instant::now();
        while matches!(engine.gpu_status(), bv_engine::GpuStatus::Starting)
            && start.elapsed() < Duration::from_secs(20)
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        engine.set_project(&p);
        for (name, from, step) in [
            ("forward", 2.0, 0.05),
            ("backward", 18.0, -0.05),
            ("fast forward", 1.0, 0.3),
        ] {
            let mut shown = 0;
            let mut last: Option<Tick> = None;
            let mut behind = 0.0;
            let mut samples = 0;
            let mut t = Tick::from_seconds_f64(from);
            let drag = Instant::now();
            while drag.elapsed() < Duration::from_millis(1500) {
                t = rate.snap_floor(Tick((t.0 as f64 + step * Tick::SECOND.0 as f64) as i64));
                engine.request_preview(t, 720, 405);
                if let Some(f) = engine.preview_frame() {
                    if last != Some(f.time) {
                        shown += 1;
                        last = Some(f.time);
                    }
                    behind += (t - f.time).as_seconds_f64().abs();
                    samples += 1;
                }
                std::thread::sleep(Duration::from_millis(16));
            }
            // The needle stops: the exact frame.
            let stop = Instant::now();
            while engine
                .preview_frame()
                .is_none_or(|f| f.time != t || !f.exact)
                && stop.elapsed() < Duration::from_secs(5)
            {
                engine.request_preview(t, 720, 405);
                std::thread::sleep(Duration::from_millis(2));
            }
            println!(
                "{gpu:?}/{decoding:?} {name:12}: {:.0} new pictures a second, {:.2} s behind the needle on average, exact frame {:.0} ms after stopping",
                shown as f64 / 1.5,
                behind / samples.max(1) as f64,
                stop.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
}

/// The audio output kept open between playbacks: how long Play takes to
/// make the position move, with the output opened for it or held already,
/// and that playback on the held output keeps time.
#[test]
#[ignore = "needs an audio output"]
fn playing_on_the_held_output() {
    use std::sync::Arc;
    use std::time::Duration;

    let mut p = bv_timeline::Project::default();
    let tone = p.add_media(bv_timeline::MediaInfo::generated(
        bv_timeline::Generator::TestTone {
            frequency_hz: 440.0,
            level_db: -18.0,
        },
        &p.settings,
    ));
    let a = p.add_track(bv_timeline::TrackKind::Audio);
    p.insert_media(tone, Tick::ZERO, None, Some(a)).unwrap();
    for held in [false, true] {
        let mut engine = bv_engine::Engine::new(Arc::new(|| {}));
        engine.set_project(&p);
        engine.hold_audio(held);
        let mut moved = Vec::new();
        for _ in 0..3 {
            let start = Instant::now();
            engine.play(Tick::ZERO, bv_engine::PlayOptions::default());
            while engine.position().is_none_or(|t| t == Tick::ZERO)
                && start.elapsed() < Duration::from_secs(2)
            {
                std::thread::sleep(Duration::from_millis(1));
            }
            moved.push(start.elapsed());
            std::thread::sleep(Duration::from_millis(1000));
            let played = engine.stop().unwrap_or(Tick::ZERO).as_seconds_f64();
            assert!(
                (0.8..1.3).contains(&played),
                "held {held}: {played:.2} s played in a second"
            );
        }
        println!(
            "held output {held}: the position moved {:?} after Play ({:?})",
            moved,
            engine.audio_status()
        );
    }
}
