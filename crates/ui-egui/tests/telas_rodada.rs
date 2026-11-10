//! Off-screen captures of the loop region (its bar of its own color, the
//! strip between yellow handles, the lines down the timeline, the tracks
//! tinted), the Envelope Appearance window (VEGAS's look, then wide lines
//! with square points) and the update window. Run with:
//! `cargo test -p bv-ui-egui --test telas_rodada -- --ignored --nocapture`
//! The images go to `target/telas/rodada_*.png`. Nothing is saved: no OK
//! is pressed.

use std::path::PathBuf;
use std::time::Duration;

use bv_ui_egui::{BananaApp, Language, apply_theme, egui, set_language};
use egui_kittest::Harness;

fn settle(h: &mut Harness<'_, BananaApp>) {
    for _ in 0..20 {
        h.step();
        std::thread::sleep(Duration::from_millis(40));
    }
}

fn snap(h: &mut Harness<'_, BananaApp>, name: &str) {
    settle(h);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
    std::fs::create_dir_all(&dir).expect("output folder");
    let img = h.render().expect("render");
    img.save(dir.join(format!("rodada_{name}.png")))
        .expect("save");
}

fn harness(app: BananaApp) -> Harness<'static, BananaApp> {
    Harness::builder()
        .with_size(egui::vec2(1600.0, 960.0))
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

#[test]
#[ignore = "renders images for visual review"]
fn captura_loop_envelopes_atualizacao() {
    set_language(Language::English);
    let mut app = BananaApp::new(egui::Context::default());
    app.welcome_for_review("closed");
    app.load_demo();
    let mut h = harness(app);
    snap(&mut h, "loop");
    // The Envelope Appearance window: VEGAS's look, then wide lines with
    // big square points and a wide reach.
    h.state_mut()
        .envelope_look_for_review((2.0, 4.0, 4.0, false));
    snap(&mut h, "envelopes_vegas");
    h.state_mut()
        .envelope_look_for_review((4.0, 12.0, 7.0, true));
    snap(&mut h, "envelopes_largos");
    // Transform mode: the event at the cursor at 60 %, moved right, its
    // box over the picture and the Transform window.
    let mut app = BananaApp::new(egui::Context::default());
    app.welcome_for_review("closed");
    app.load_demo();
    app.transform_for_review(0.6);
    let mut h = harness(app);
    snap(&mut h, "transformar");
    // Docking: the Trimmer's tab dragged over the Explorer's left side, the
    // room it would take lit up (not dropped).
    let mut app = BananaApp::new(egui::Context::default());
    app.welcome_for_review("closed");
    app.load_demo();
    let mut h = harness(app);
    settle(&mut h);
    // The Trimmer's tab, beside the Video Preview's (the default layout).
    let from = egui::pos2(950.0, 597.0);
    h.event(egui::Event::PointerMoved(from));
    h.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
    for i in 1..=30 {
        let t = i as f32 / 30.0;
        let at = from + (egui::pos2(90.0, 320.0) - from) * t;
        h.event(egui::Event::PointerMoved(at));
        h.step();
    }
    snap(&mut h, "encaixe_arrastando");
    // The Trimmer floating in a window of its own.
    let mut app = BananaApp::new(egui::Context::default());
    app.welcome_for_review("closed");
    app.load_demo();
    app.float_for_review("Trimmer");
    let mut h = harness(app);
    snap(&mut h, "flutuante");
    // The update window, as a newer BananaV shows.
    let mut app = BananaApp::new(egui::Context::default());
    app.welcome_for_review("closed");
    app.load_demo();
    app.update_for_review("0.3.0");
    let mut h = harness(app);
    snap(&mut h, "atualizacao");
}

/// The loop region with its selection shown (the lines down the timeline,
/// the tracks tinted), then let go of by a click elsewhere (its bar alone),
/// and the Transform window docked beside the browsers.
#[test]
#[ignore = "renders images for visual review"]
fn captura_loop_e_transform() {
    set_language(Language::English);
    let mut app = BananaApp::new(egui::Context::default());
    app.welcome_for_review("closed");
    app.load_demo();
    let mut h = harness(app);
    snap(&mut h, "loop_shown");
    h.state_mut().hide_selection_for_review();
    snap(&mut h, "loop_hidden");
    h.state_mut().transform_window_for_review();
    snap(&mut h, "transform");
}
