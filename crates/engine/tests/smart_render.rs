//! Smart rendering ("Enable no-recompress rendering"): ProRes frames the
//! project leaves as their media had them pass into the file as they are.
#![cfg(windows)]

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bv_core::{FrameRate, Tick, TimeRange};
use bv_engine::{
    Engine, FileFormat, ProResProfile, RenderOptions, RenderProgress, RenderSettings, VideoParams,
};
use bv_timeline::{Generator, MediaInfo, Project, ProjectSettings, TrackKind};

fn settings() -> ProjectSettings {
    ProjectSettings {
        width: 160,
        height: 96,
        frame_rate: FrameRate::new(30, 1),
        ..ProjectSettings::default()
    }
}

/// A second of `media` on the first video track of a project.
fn project_of(media: MediaInfo) -> Project {
    let mut p = Project::new(settings());
    let v = p
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Video)
        .map(|t| t.id);
    let m = p.add_media(media);
    let clip = p.insert_media(m, Tick::ZERO, v, None).unwrap()[0];
    p.set_clip_duration(clip, Tick::from_seconds(1)).unwrap();
    p
}

/// Renders a second of `project` to a ProRes 422 file at `path`.
fn render(engine: &mut Engine, project: &Project, path: &Path, smart: bool) -> RenderProgress {
    engine.set_project(project);
    engine.start_render(RenderSettings {
        format: FileFormat::ProRes,
        path: path.to_path_buf(),
        range: TimeRange::new(Tick::ZERO, Tick::from_seconds(1)),
        video: Some(VideoParams {
            width: 160,
            height: 96,
            frame_rate: FrameRate::new(30, 1),
            prores: ProResProfile::Standard,
            alpha: false,
            ..VideoParams::default()
        }),
        audio: None,
        options: RenderOptions {
            no_recompress: smart,
            ..RenderOptions::default()
        },
        project_file: None,
    });
    let start = Instant::now();
    while !engine.render_progress().is_some_and(|p| p.finished) {
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "rendered in time"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let done = engine.render_progress().unwrap();
    assert!(done.error.is_none(), "{:?}", done.error);
    done
}

/// The compressed frames of a file.
fn packets(path: &Path) -> Vec<Vec<u8>> {
    let mut r = bv_mf::PacketReader::video(&path.to_string_lossy()).unwrap();
    let mut out = Vec::new();
    while let Some(p) = r.next_packet().unwrap() {
        out.push(p.data);
    }
    out
}

#[test]
fn unedited_prores_frames_pass_into_the_file_as_they_are() {
    let dir = std::env::temp_dir().join(format!("bv-smart-render-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut engine = Engine::new(Arc::new(|| {}));
    // A ProRes file of color bars.
    let source = dir.join("bars.mov");
    let bars = project_of(MediaInfo::generated(Generator::ColorBars, &settings()));
    assert_eq!(render(&mut engine, &bars, &source, true).passed, 0);
    // That file alone, as it is: every frame passes.
    let info = Engine::probe_file(&source.to_string_lossy()).unwrap();
    let mut plain = project_of(info);
    let out = dir.join("plain.mov");
    assert_eq!(render(&mut engine, &plain, &out, true).passed, 30);
    assert_eq!(packets(&out), packets(&source));
    // The option off: encoded again.
    let again = dir.join("again.mov");
    assert_eq!(render(&mut engine, &plain, &again, false).passed, 0);
    // Faded to half: nothing passes.
    let video = plain.tracks.iter_mut().find(|t| t.kind == TrackKind::Video);
    video.unwrap().clips[0].level = 0.5;
    let faded = dir.join("faded.mov");
    assert_eq!(render(&mut engine, &plain, &faded, true).passed, 0);
    let _ = std::fs::remove_dir_all(&dir);
}
