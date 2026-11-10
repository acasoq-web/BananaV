//! Playback started at a spot of a long-GOP recording (a 4K trailer with a
//! keyframe every 8 s): the sound waits for the preview's first frame, so
//! picture and sound start together, and how long that takes is printed.
//! Needs a file and plays a moment of it:
//! `BV_SEEK_FILE=<path> cargo test -p bv-ui-egui --test playback_start -- --ignored --nocapture`

#![allow(clippy::print_stdout)]

use std::time::{Duration, Instant};

use bv_ui_egui::{BananaApp, Language, apply_theme, egui, set_language};
use egui_kittest::Harness;

#[test]
#[ignore = "needs a file: BV_SEEK_FILE"]
fn the_sound_waits_for_the_first_frame() {
    let Ok(path) = std::env::var("BV_SEEK_FILE") else {
        return;
    };
    set_language(Language::English);
    let mut app = BananaApp::new(egui::Context::default());
    app.welcome_for_review("closed");
    assert!(
        app.import_and_place(std::path::Path::new(&path)),
        "imported"
    );
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 960.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    let steps = |h: &mut Harness<'_, BananaApp>, n: usize| {
        for _ in 0..n {
            h.step();
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    steps(&mut h, 30);
    for spot in [45.0, 12.0, 70.0] {
        // A click on the ruler there, then Space at once (as the user did).
        h.state_mut().seek_seconds(spot);
        steps(&mut h, 2);
        let start = Instant::now();
        h.key_press(egui::Key::Space);
        while !h.state().playing_for_review() {
            assert!(start.elapsed() < Duration::from_secs(6), "playback started");
            steps(&mut h, 1);
        }
        let waited = start.elapsed();
        let (at, exact) = h.state().preview_shown_for_review().expect("a frame");
        println!(
            "from {spot} s: the sound started after {} ms, the preview at {at:.2} s (exact: {exact})",
            waited.as_millis()
        );
        assert!(exact && (at - spot).abs() < 0.1, "the first frame was in");
        // Played a moment, then stopped.
        steps(&mut h, 20);
        h.key_press(egui::Key::Space);
        steps(&mut h, 5);
    }
}
