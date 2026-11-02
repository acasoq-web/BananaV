//! Render's "Save project as path reference in rendered file": the
//! project's path kept in the file BananaV writes, read back for Edit
//! Source Project.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bv_core::{Tick, TimeRange};
use bv_engine::{AudioParams, Engine, FileFormat, RenderOptions, RenderSettings};
use bv_timeline::{Generator, MediaInfo, Project, ProjectSettings, TrackKind};

/// A project of a second of tone.
fn project() -> Project {
    let mut p = Project::new(ProjectSettings::default());
    let a = p
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Audio)
        .map(|t| t.id);
    let m = p.add_media(MediaInfo::generated(
        Generator::TestTone {
            frequency_hz: 440.0,
            level_db: -12.0,
        },
        &p.settings,
    ));
    let clip = p.insert_media(m, Tick::ZERO, None, a).unwrap()[0];
    p.set_clip_duration(clip, Tick::from_seconds(1)).unwrap();
    p
}

/// Renders the project to a WAV file at `path`, the option `keep` on.
fn render(engine: &mut Engine, path: &Path, keep: bool) {
    engine.start_render(RenderSettings {
        format: FileFormat::Wav,
        path: path.to_path_buf(),
        range: TimeRange::new(Tick::ZERO, Tick::from_seconds(1)),
        video: None,
        audio: Some(AudioParams::default()),
        options: RenderOptions {
            path_reference: keep,
            ..RenderOptions::default()
        },
        project_file: Some(PathBuf::from("C:/Projects/Tone.bvproj")),
    });
    let start = Instant::now();
    while !engine.render_progress().is_some_and(|p| p.finished) {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "rendered in time"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let done = engine.render_progress().unwrap();
    assert!(done.error.is_none(), "{:?}", done.error);
}

#[test]
fn a_rendered_file_keeps_its_project_s_path_when_asked() {
    let dir = std::env::temp_dir().join(format!("bv-project-reference-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut engine = Engine::new(Arc::new(|| {}));
    engine.set_project(&project());
    let kept = dir.join("kept.wav");
    render(&mut engine, &kept, true);
    assert_eq!(
        Engine::project_reference(&kept),
        Some(PathBuf::from("C:/Projects/Tone.bvproj"))
    );
    // The option off: no path kept.
    let plain = dir.join("plain.wav");
    render(&mut engine, &plain, false);
    assert_eq!(Engine::project_reference(&plain), None);
    let _ = std::fs::remove_dir_all(&dir);
}
