//! Tools › Selectively Prerender Video: the parts that need it drawn into
//! files (BananaV's own ProRes encoder), and the preview playing them from
//! those files until an edit changes what they show.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bv_core::{FrameRate, Tick, TimeRange};
use bv_engine::{
    Engine, FileFormat, PrerenderFormat, PrerenderOptions, RenderOptions, RenderSettings,
    VideoParams,
};
use bv_timeline::{Effect, EffectKind, Generator, MediaInfo, Project, ProjectSettings};

fn settings() -> ProjectSettings {
    ProjectSettings {
        width: 256,
        height: 144,
        frame_rate: FrameRate::new(30, 1),
        ..ProjectSettings::default()
    }
}

/// A project of one event of `generator` from the start.
fn project(generator: Generator) -> Project {
    let mut p = Project::new(settings());
    let v = p.tracks[0].id;
    let m = p.add_media(MediaInfo::generated(generator, &p.settings));
    p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
    p
}

/// The preview's frame of `t`, drawn for the project the engine has now.
fn preview(engine: &Engine, t: Tick) -> [u8; 4] {
    engine.request_preview(t, 256, 144);
    let start = Instant::now();
    loop {
        if let Some(f) = engine.preview_frame()
            && f.time == t
            && f.exact
        {
            let i = ((f.height / 2 * f.width + f.width / 2) * 4) as usize;
            return [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2], f.rgba[i + 3]];
        }
        assert!(start.elapsed() < Duration::from_secs(30), "drawn in time");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn red(c: [u8; 4]) -> bool {
    c[0] > 200 && c[1] < 60 && c[2] < 60
}

#[test]
fn the_preview_plays_prerendered_parts_until_they_are_edited() {
    let folder =
        std::env::temp_dir().join(format!("bananav-prerender-engine-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut p = project(Generator::ColorBars);
    let clip = p.tracks[0].clips[0].id;
    let all = TimeRange::new(Tick::ZERO, Tick::from_seconds(2));
    let mut engine = Engine::new(Arc::new(|| {}));
    engine.set_project(&p);
    assert_eq!(
        engine.prerender(all, &folder, PrerenderOptions::default()),
        0,
        "plays as it is"
    );

    p.clip_mut(clip)
        .unwrap()
        .effects
        .push(Effect::new(EffectKind::Invert));
    engine.set_project(&p);
    let options = PrerenderOptions {
        format: PrerenderFormat::ProRes,
        ..PrerenderOptions::default()
    };
    let second = TimeRange::new(Tick::ZERO, Tick::from_seconds(1));
    assert_eq!(engine.prerender(second, &folder, options), 1);
    let start = Instant::now();
    let made = loop {
        let (progress, made) = engine.prerender_progress().expect("going on");
        if progress.finished {
            assert_eq!(progress.error, None);
            break made;
        }
        assert!(start.elapsed() < Duration::from_secs(60), "done in time");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(made.len(), 1);
    assert_eq!(made[0].range, second);
    let file = PathBuf::from(&made[0].file);
    let info = Engine::probe_file(&made[0].file).expect("a video");
    assert_eq!(info.video.map(|v| (v.width, v.height)), Some((256, 144)));
    assert_eq!(settings().frame_rate.frame_at(info.duration), 30);

    // To tell the file from the project, it shows red instead.
    let mut other = Engine::new(Arc::new(|| {}));
    other.set_project(&project(Generator::SolidColor {
        rgba: [1.0, 0.0, 0.0, 1.0],
    }));
    other.start_render(RenderSettings {
        format: FileFormat::ProRes,
        path: file.clone(),
        range: second,
        video: Some(VideoParams {
            width: 256,
            height: 144,
            frame_rate: settings().frame_rate,
            ..VideoParams::default()
        }),
        audio: None,
        options: RenderOptions::default(),
        project_file: None,
    });
    while !other.render_progress().is_some_and(|p| p.finished) {
        assert!(start.elapsed() < Duration::from_secs(90), "done in time");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(other.render_progress().and_then(|p| p.error), None);

    p.prerenders = made;
    engine.set_project(&p);
    assert_eq!(engine.prerendered(), &[second]);
    let t = |frame| settings().frame_rate.frame_start(frame);
    assert!(red(preview(&engine, t(15))), "played from the file");
    assert!(!red(preview(&engine, t(40))), "after the part: the project");

    // An edit there: the project again.
    p.clip_mut(clip).unwrap().level = 0.9;
    engine.set_project(&p);
    assert!(engine.prerendered().is_empty());
    assert!(!red(preview(&engine, t(16))), "no longer the file");
    let _ = std::fs::remove_dir_all(&folder);
}

/// Under ACES a prerendered part holds the frames as they left the view:
/// played from the file it shows what the project does, not the view
/// twice.
#[test]
fn prerendered_parts_under_aces_show_what_the_project_does() {
    use bv_timeline::{AcesDisplay, AcesView, PixelFormat, ViewTransform};
    let folder =
        std::env::temp_dir().join(format!("bananav-prerender-aces-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut p = project(Generator::SolidColor {
        rgba: [0.2, 0.5, 0.8, 1.0],
    });
    p.settings.pixel_format = PixelFormat::Float32Full;
    p.settings.color.view = Some(ViewTransform {
        display: AcesDisplay::Srgb,
        view: AcesView::Aces,
    });
    // An effect, so the part needs prerendering.
    let clip = p.tracks[0].clips[0].id;
    p.clip_mut(clip)
        .unwrap()
        .effects
        .push(Effect::new(EffectKind::Invert));
    let mut engine = Engine::new(Arc::new(|| {}));
    engine.set_project(&p);
    let t = |frame| settings().frame_rate.frame_start(frame);
    // Another frame than the one compared later (the same color): the
    // preview keeps the last frame it drew.
    let live = preview(&engine, t(14));
    let second = TimeRange::new(Tick::ZERO, Tick::from_seconds(1));
    let options = PrerenderOptions {
        format: PrerenderFormat::ProRes,
        ..PrerenderOptions::default()
    };
    assert_eq!(engine.prerender(second, &folder, options), 1);
    let start = Instant::now();
    let made = loop {
        let (progress, made) = engine.prerender_progress().expect("going on");
        if progress.finished {
            assert_eq!(progress.error, None);
            break made;
        }
        assert!(start.elapsed() < Duration::from_secs(60), "done in time");
        std::thread::sleep(Duration::from_millis(10));
    };
    p.prerenders = made;
    engine.set_project(&p);
    assert_eq!(engine.prerendered(), &[second]);
    let played = preview(&engine, t(15));
    let off = (0..3)
        .map(|c| (i32::from(live[c]) - i32::from(played[c])).abs())
        .max()
        .unwrap_or(0);
    assert!(off <= 3, "live {live:?}, from the file {played:?}");
    let _ = std::fs::remove_dir_all(&folder);
}
