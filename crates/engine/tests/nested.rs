//! Nested projects: a project file brought into another project as media,
//! its pictures drawn and its sound mixed as the outer project reads them.

use std::sync::Arc;

use bv_core::Tick;
use bv_engine::Engine;
use bv_timeline::{Generator, MediaInfo, Project, ProjectSettings, TrackKind};

/// Two seconds of red over a tone.
fn inner() -> Project {
    let mut p = Project::new(ProjectSettings::default());
    let track = |p: &Project, kind| p.tracks.iter().find(|t| t.kind == kind).map(|t| t.id);
    let (v, a) = (track(&p, TrackKind::Video), track(&p, TrackKind::Audio));
    let red = p.add_media(MediaInfo::generated(
        Generator::SolidColor {
            rgba: [1.0, 0.0, 0.0, 1.0],
        },
        &p.settings,
    ));
    let tone = p.add_media(MediaInfo::generated(
        Generator::TestTone {
            frequency_hz: 440.0,
            level_db: -6.0,
        },
        &p.settings,
    ));
    for (m, video, audio) in [(red, v, None), (tone, None, a)] {
        let clip = p.insert_media(m, Tick::ZERO, video, audio).unwrap()[0];
        p.set_clip_duration(clip, Tick::from_seconds(2)).unwrap();
    }
    p
}

#[test]
fn a_nested_project_is_drawn_and_mixed_as_media() {
    let _engine = Engine::new(Arc::new(|| {}));
    let dir = std::env::temp_dir().join(format!("bv-nested-engine-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Red.bvproj");
    std::fs::write(&path, inner().to_json()).unwrap();
    let info = Engine::probe_file(&path.to_string_lossy()).unwrap();
    assert_eq!(info.duration, Tick::from_seconds(2));
    assert_eq!(info.audio.map(|a| a.channels), Some(2));

    // Its pictures: red while its event lasts, transparent after.
    let mut video = bv_media::open_video(&info).unwrap();
    let frame = video.frame_at(Tick::from_seconds(1)).unwrap();
    let px = frame.data[frame.data.len() / 2];
    assert!(px[0] > 0.9 && px[1] < 0.05 && px[3] > 0.99, "{px:?}");
    let small = video
        .frame_at_scaled(Tick::from_seconds(1), 480, 270)
        .unwrap();
    assert_eq!((small.width, small.height), (480, 270));
    let after = video.frame_at(Tick::from_seconds(3)).unwrap();
    assert!(
        after.data.iter().all(|p| p[3] == 0.0),
        "nothing past its end"
    );

    // Its sound: the tone, in stereo.
    let mut audio = bv_media::open_audio(&info).unwrap();
    let rate = audio.sample_rate();
    let buf = audio.read(rate.0 as i64 / 2, 4800).unwrap();
    assert_eq!(buf.channels.len(), 2);
    let peak = buf.channels[0].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(peak > 0.2, "{peak}");

    // In another project, it is one more event.
    let mut outer = Project::new(ProjectSettings::default());
    let v = outer
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Video)
        .map(|t| t.id);
    let m = outer.add_media(info);
    outer.insert_media(m, Tick::ZERO, v, None).unwrap();
    let (w, h) = outer.settings.output_size();
    let mut pool = bv_render::VideoPool::new();
    let img = bv_render::render_output(&outer, Tick::from_seconds(1), w / 4, h / 4, &mut pool);
    let px = img.data[img.data.len() / 2];
    assert!(px[0] > 0.9 && px[1] < 0.05, "{px:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
