//! 3D compositing through the whole renderer: 3D tracks cross one
//! another; a 2D track between them, even an empty one, is a barrier.

use bv_core::Tick;
use bv_render::VideoPool;
use bv_timeline::space::Box3d;
use bv_timeline::{CompositeMode, Generator, MediaInfo, Project, ProjectSettings, TrackKind};

/// Red on top turned one way, blue below turned the other, both 3D; with
/// `barrier`, an empty 2D track between them.
fn project(barrier: bool) -> Project {
    let mut p = Project::new(ProjectSettings {
        width: 64,
        height: 36,
        ..ProjectSettings::default()
    });
    let top = p
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Video)
        .unwrap()
        .id;
    let below = p.insert_track_at(TrackKind::Video, 1);
    if barrier {
        p.insert_track_at(TrackKind::Video, 1);
    }
    for (track, rgba, deg) in [
        (top, [1.0, 0.0, 0.0, 1.0], 30.0),
        (below, [0.0, 0.0, 1.0, 1.0], -30.0),
    ] {
        let m = p.add_media(MediaInfo::generated(
            Generator::SolidColor { rgba },
            &p.settings,
        ));
        p.insert_media(m, Tick::ZERO, Some(track), None).unwrap();
        let t = p.tracks.iter_mut().find(|t| t.id == track).unwrap();
        t.composite_mode = CompositeMode::ThreeDSourceAlpha;
        p.track_motion_mut(track, false).unwrap().space = Box3d {
            orientation: [0.0, deg, 0.0],
            ..Box3d::default()
        };
    }
    p
}

/// Whether the pixels a quarter and three quarters across are red.
fn sides(p: &Project) -> (bool, bool) {
    let img = bv_render::render_output(p, Tick::from_seconds(1), 64, 36, &mut VideoPool::new());
    let red = |x: usize| img.data[18 * 64 + x][0] > 0.5;
    (red(16), red(48))
}

#[test]
fn three_d_tracks_intersect() {
    let (left, right) = sides(&project(false));
    assert_ne!(left, right, "each in front on one side");
}

#[test]
fn a_two_d_track_between_them_is_a_barrier() {
    assert_eq!(sides(&project(true)), (true, true), "the top one over all");
}
