//! Off-screen captures of the whole main window for the website, all at
//! 1920 × 1080 and all of the demo project (nothing of the computer's own
//! files shows: the Explorer stays behind other windows). Run with:
//! `cargo test -p bv-ui-egui --test telas_site -- --ignored --nocapture`
//! The images go to `target/telas/site_*.png`; `site/assets/img/screen-*.png`
//! are made from them.

use std::path::PathBuf;
use std::time::Duration;

use bv_ui_egui::{BananaApp, Language, apply_theme, egui, set_language};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn settle(h: &mut Harness<'_, BananaApp>) {
    // The preview, thumbnails and waveforms arrive from other threads.
    for _ in 0..40 {
        h.step();
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn snap(h: &mut Harness<'_, BananaApp>, name: &str) {
    settle(h);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
    std::fs::create_dir_all(&dir).expect("output folder");
    let img = h.render().expect("render");
    img.save(dir.join(format!("site_{name}.png")))
        .expect("save");
}

/// The demo project in English, the welcome screen closed, never the
/// user's filter packages.
fn demo() -> BananaApp {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::English);
    app.welcome_for_review("closed");
    app.fx_packages_for_review();
    app.load_demo();
    app
}

fn harness(app: BananaApp) -> Harness<'static, BananaApp> {
    Harness::builder()
        .with_size(egui::vec2(1920.0, 1080.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        )
}

/// The pointer on the widget named `label` (a preset: only it plays).
fn hover(h: &mut Harness<'_, BananaApp>, label: &str) {
    let at = h.get_by_label(label).rect().center();
    h.event(egui::Event::PointerMoved(at));
    h.step();
}

/// A chooser's plug-in picked, the pointer on one of its presets.
fn chooser(h: &mut Harness<'_, BananaApp>, window: &str, plugin: &str, preset: &str) {
    h.state_mut().show_tab_named(window);
    settle(h);
    h.get_by_label(plugin).click();
    settle(h);
    hover(h, preset);
}

#[test]
#[ignore = "renders images for the website"]
fn captura_site() {
    let mut h = harness(demo());
    chooser(&mut h, "Video FX", "Add Noise", "(Default)");
    snap(&mut h, "efeitos");
    chooser(&mut h, "Transitions", "GL Transition", "Cube");
    snap(&mut h, "transicoes");
    chooser(&mut h, "Media Generators", "Noise Texture", "Lava");
    snap(&mut h, "geradores");

    // An event's effects window: Swirl after Vignette, its center's handle
    // on the preview.
    let mut app = demo();
    assert!(app.plugin_effect_for_review("org.bananav.Swirl"));
    let mut h = harness(app);
    h.state_mut().show_tab_named("Project Media");
    snap(&mut h, "efeito_janela");

    let mut app = demo();
    assert!(app.color_grading_for_review(1));
    snap(&mut harness(app), "cor");

    let mut app = demo();
    assert!(app.mixing_console_for_review());
    snap(&mut harness(app), "audio");

    let mut app = demo();
    assert!(app.bus_tracks_for_review());
    let mut h = harness(app);
    settle(&mut h);
    assert!(h.state_mut().bus_automation_for_review());
    snap(&mut h, "timeline");
}
