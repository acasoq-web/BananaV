//! Banana Split's effects through the engine: as OpenFX plug-ins on the
//! processor, by their steps on the card.

#![allow(clippy::print_stdout)]

use super::tests::{StaysOnCard, card, own_fx};
use super::*;
use crate::accel::{Accel, Painter};
use bv_banana::fx::Kind;
use bv_timeline::{Generator, Project, ProjectSettings};

fn id(fx: &bv_banana::fx::Fx) -> String {
    format!("{}{}", bv_banana::ID_PREFIX, fx.id)
}

/// Moving clouds.
fn clouds(p: &mut Project, preset: Option<&str>) -> bv_timeline::MediaId {
    let fx = own_fx("org.bananav.bananasplit.Clouds", preset, &[])
        .plugin
        .expect("a plug-in");
    let media = MediaInfo::generated(Generator::Plugin(fx), &p.settings.clone());
    p.add_media(media)
}

/// A project that shows `fx`, and when.
fn project(fx: &bv_banana::fx::Fx, float: bool, preset: Option<&str>) -> (Project, Tick) {
    let mut p = Project::new(ProjectSettings::default());
    if float {
        p.settings.pixel_format = bv_timeline::PixelFormat::Float32Full;
    }
    let s = Tick::from_seconds;
    let v = p.tracks[0].id;
    match fx.kind {
        Kind::Filter => {
            let m = clouds(&mut p, None);
            let c = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
            p.clip_mut(c).unwrap().effects = vec![own_fx(&id(fx), preset, &[])];
        }
        Kind::Generator => {
            let plugin = own_fx(&id(fx), preset, &[]).plugin.expect("a plug-in");
            let media = MediaInfo::generated(Generator::Plugin(plugin), &p.settings.clone());
            let m = p.add_media(media);
            p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        }
        Kind::Transition => {
            let a = clouds(&mut p, None);
            let b = clouds(&mut p, Some("Storm"));
            p.insert_media(a, Tick::ZERO, Some(v), None).unwrap();
            let c = p.insert_media(b, s(4), Some(v), None).unwrap()[0];
            p.clip_mut(c).unwrap().transition_fx = own_fx(&id(fx), preset, &[]).plugin;
        }
    }
    let at = match fx.kind {
        Kind::Transition => Tick::from_seconds_f64(4.43),
        _ => Tick::from_seconds_f64(1.7),
    };
    (p, at)
}

/// How far apart two frames are: pixels off and the rest's mean.
fn apart(a: &ImageRgbaF32, b: &ImageRgbaF32) -> (usize, f64) {
    let (mut off, mut sum) = (0usize, 0f64);
    for (x, y) in a.data.iter().zip(&b.data) {
        let d = (0..4).map(|k| (x[k] - y[k]).abs()).fold(0f32, f32::max);
        if d.is_nan() || d > 0.03 {
            off += 1;
        } else {
            sum += f64::from(d);
        }
    }
    (off, sum / (a.data.len() - off).max(1) as f64)
}

/// Every effect of the pack in a project's frame, in 8 or in 32 bits: the
/// processor's plug-in and the card's steps draw alike.
#[test]
fn the_pack_draws_alike_in_a_projects_frames() {
    register_own();
    let accel = Accel::new(Arc::new(|| {}));
    let mut painter = Painter::new(accel.clone(), Purpose::File);
    let black = [0.0, 0.0, 0.0, 1.0];
    let size = (256, 144);
    let mut runs = Vec::new();
    for (n, fx) in bv_banana::all().iter().enumerate() {
        let float = n % 2 == 1;
        let (p, at) = project(fx, float, None);
        let c = painter.linear(&p, at, size, &mut VideoPool::new(), black);
        assert!(
            c.data.iter().all(|x| x.iter().all(|v| v.is_finite())),
            "{}",
            fx.id
        );
        let bits = if float { 32 } else { 8 };
        runs.push((format!("{} ({bits} bits)", fx.id), p, at, c));
    }
    if !card(&accel) {
        return;
    }
    let _card = StaysOnCard(accel.clone());
    let _ = painter.linear(&runs[0].1, runs[0].2, size, &mut VideoPool::new(), black);
    let mut differing = Vec::new();
    for (name, p, at, c) in &runs {
        let g = painter.linear(p, *at, size, &mut VideoPool::new(), black);
        let (off, mean) = apart(c, &g);
        if off * 100 > c.data.len() || mean > 1e-3 {
            differing.push(format!("{name}: {off} pixels, the rest by {mean:.1e}"));
        }
    }
    assert!(differing.is_empty(), "{differing:#?}");
}

/// An effect that reads other frames draws the same with them kept from
/// the frames before as worked out anew, and follows what they are of.
#[test]
fn kept_frames_are_the_ones_worked_out_anew() {
    register_own();
    let accel = Accel::new(Arc::new(|| {}));
    let black = [0.0, 0.0, 0.0, 1.0];
    let size = (192, 108);
    let fx = bv_banana::all()
        .iter()
        .find(|f| f.is_temporal())
        .expect("one that reads other frames");
    let (mut p, _) = project(fx, false, None);
    let rate = p.settings.frame_rate.as_f64();
    let times: Vec<Tick> = (0..6)
        .map(|n| Tick::from_seconds_f64(1.0 + f64::from(n) / rate))
        .collect();
    let mut kept = Painter::new(accel.clone(), Purpose::File);
    let mut pool = VideoPool::new();
    for &t in &times {
        let a = kept.linear(&p, t, size, &mut pool, black);
        let mut anew = Painter::new(accel.clone(), Purpose::File);
        let b = anew.linear(&p, t, size, &mut VideoPool::new(), black);
        assert!(a.data == b.data, "{t:?}");
    }
    // Another picture under the effect: never the ones kept.
    let before = kept.linear(&p, times[5], size, &mut pool, black);
    let m = clouds(&mut p, Some("Storm"));
    let c = p.tracks[0].clips[0].id;
    p.clip_mut(c).unwrap().media = m;
    let after = kept.linear(&p, times[5], size, &mut VideoPool::new(), black);
    let mut anew = Painter::new(accel, Purpose::File);
    let b = anew.linear(&p, times[5], size, &mut VideoPool::new(), black);
    assert!(after.data == b.data && after.data != before.data);
}

/// How long each effect takes: a frame on the card and on the processor
/// (`BV_BANANA_CARD_ONLY`: only there), `BV_BANANA_SIZE` big (1920x1080),
/// the slowest first. Filters get moving clouds, or the video file
/// `BV_BANANA_VIDEO`; `BV_BANANA_ONLY` names the effects measured.
/// `cargo test --release -p bv-engine pack_tests -- --ignored --nocapture`
#[test]
#[ignore = "measures"]
fn the_packs_times() {
    register_own();
    let size = std::env::var("BV_BANANA_SIZE").unwrap_or_else(|_| "1920x1080".into());
    let (w, h) = size.split_once('x').expect("a size");
    let size = (w.parse().unwrap(), h.parse().unwrap());
    let accel = Accel::new(Arc::new(|| {}));
    let mut painter = Painter::new(accel.clone(), Purpose::File);
    let black = [0.0, 0.0, 0.0, 1.0];
    let video = std::env::var("BV_BANANA_VIDEO").ok();
    let only = std::env::var("BV_BANANA_ONLY").ok();
    // Enough for an effect of many frames to have them.
    let frames = if video.is_some() { 40 } else { 5 };
    let measure = |painter: &mut Painter, p: &Project, at: Tick| {
        let mut pool = VideoPool::new();
        let rate = p.settings.frame_rate.as_f64();
        let mut best = f64::MAX;
        // Frames in order, as they play.
        for n in 0..frames {
            let start = Instant::now();
            let t = at + Tick::from_seconds_f64(f64::from(n) / rate);
            let _ = painter.linear(p, t, size, &mut pool, black);
            if n > frames / 2 {
                best = best.min(start.elapsed().as_secs_f64() * 1e3);
            }
        }
        best
    };
    let on_processor = std::env::var_os("BV_BANANA_CARD_ONLY").is_none();
    let mut rows: Vec<(String, f64, f64)> = Vec::new();
    let mut base = Project::new(ProjectSettings::default());
    let m = clouds(&mut base, None);
    let v = base.tracks[0].id;
    base.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
    let mut runs = vec![(
        "(the clouds alone)".to_string(),
        base,
        Tick::from_seconds(1),
    )];
    let wanted = |id: &str| {
        only.as_ref()
            .is_none_or(|o| o.split(',').any(|w| w.trim() == id))
    };
    runs.extend(bv_banana::all().iter().filter(|f| wanted(f.id)).map(|fx| {
        let (p, at) = project(fx, false, None);
        (fx.id.to_string(), p, at)
    }));
    if let Some(file) = &video {
        let info = bv_media::probe_file(file).expect("a video");
        for (_, p, _) in &mut runs {
            let m = p.add_media(info.clone());
            for c in p.tracks[0].clips.iter().map(|c| c.id).collect::<Vec<_>>() {
                let generated = p.clip(c).is_some_and(|c| c.transition_fx.is_none());
                if generated && p.tracks[0].clips.len() == 1 {
                    p.clip_mut(c).unwrap().media = m;
                }
            }
        }
    }
    for (name, p, at) in &runs {
        let t = if on_processor {
            measure(&mut painter, p, *at)
        } else {
            0.0
        };
        rows.push((name.clone(), 0.0, t));
    }
    if card(&accel) {
        for (row, (_, p, at)) in rows.iter_mut().zip(&runs) {
            row.1 = measure(&mut painter, p, *at);
        }
    }
    let alone = (rows[0].1, rows[0].2);
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    println!(
        "clouds alone: {:.1} ms card, {:.1} ms processor",
        alone.0, alone.1
    );
    for (name, c, t) in rows {
        println!("{name:24} {c:7.1} ms card {t:8.1} ms processor");
    }
}

/// Knowing the effects there are — BananaV's own and the pack's — takes no
/// time the program's start would show.
#[test]
#[ignore = "measures"]
fn the_packs_start() {
    let start = Instant::now();
    register_own();
    println!("registered in {:?}", start.elapsed());
    let known = bv_ofx::registry().known();
    let took = start.elapsed();
    let pack = known.iter().filter(|i| is_banana_split(i)).count();
    println!("{} plug-ins, {pack} of the pack, in {took:?}", known.len());
}
