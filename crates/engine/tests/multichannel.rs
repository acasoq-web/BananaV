//! Render's multichannel mapping: the busses the project's channel mapping
//! picks, each in channels of its own in a Wave file.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bv_core::{Tick, TimeRange};
use bv_engine::{AudioParams, Engine, FileFormat, RenderOptions, RenderSettings};
use bv_timeline::{Generator, MappedStrip, MediaInfo, Output, Project, ProjectSettings, TrackKind};

/// A second of tone on a track playing into bus A; bus B gets nothing.
/// The mapping: bus A, then bus B mixed down to one channel.
fn project() -> Project {
    let mut p = Project::new(ProjectSettings::default());
    let a = p.add_bus().unwrap();
    let b = p.add_bus().unwrap();
    let track = p
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Audio)
        .map(|t| t.id)
        .unwrap();
    let m = p.add_media(MediaInfo::generated(
        Generator::TestTone {
            frequency_hz: 440.0,
            level_db: -12.0,
        },
        &p.settings,
    ));
    let clip = p.insert_media(m, Tick::ZERO, None, Some(track)).unwrap()[0];
    p.set_clip_duration(clip, Tick::from_seconds(1)).unwrap();
    p.track_by_id_mut(track).unwrap().output = Output::Bus(a);
    p.channel_mapping = vec![
        MappedStrip {
            bus: Some(a),
            on: true,
            mono: false,
        },
        MappedStrip {
            bus: Some(b),
            on: true,
            mono: true,
        },
    ];
    p
}

/// Renders the project to a Wave file at `path`, the option `on`.
fn render(engine: &mut Engine, path: &Path, on: bool) {
    engine.start_render(RenderSettings {
        format: FileFormat::Wav,
        path: path.to_path_buf(),
        range: TimeRange::new(Tick::ZERO, Tick::from_seconds(1)),
        video: None,
        audio: Some(AudioParams::default()),
        options: RenderOptions {
            multichannel: on,
            ..RenderOptions::default()
        },
        project_file: None,
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

fn loudest(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

#[test]
fn mapped_busses_get_channels_of_their_own() {
    let dir = std::env::temp_dir().join(format!("bv-multichannel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut engine = Engine::new(Arc::new(|| {}));
    engine.set_project(&project());
    // Bus A's two channels, then bus B in one: three in the file (the
    // template's stereo is fewer).
    let mapped = dir.join("mapped.wav");
    render(&mut engine, &mapped, true);
    let mut r = bv_wav::WavReader::new(std::fs::File::open(&mapped).unwrap()).unwrap();
    let samples = r.read_planar(0, 48_000).unwrap();
    assert_eq!(samples.len(), 3);
    assert!(
        loudest(&samples[0]) > 0.1 && loudest(&samples[1]) > 0.1,
        "bus A"
    );
    assert!(loudest(&samples[2]) < 1e-6, "bus B is silent");
    // The option off: the Master's mix in the template's two channels.
    let plain = dir.join("plain.wav");
    render(&mut engine, &plain, false);
    let mut r = bv_wav::WavReader::new(std::fs::File::open(&plain).unwrap()).unwrap();
    let samples = r.read_planar(0, 48_000).unwrap();
    assert_eq!(samples.len(), 2);
    assert!(loudest(&samples[0]) > 0.1);
    let _ = std::fs::remove_dir_all(&dir);
}
