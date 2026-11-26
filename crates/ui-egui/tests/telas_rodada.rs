//! Off-screen captures of the loop region (its bar of its own color, the
//! strip between yellow handles, the lines down the timeline, the tracks
//! tinted), the Envelope Appearance window (VEGAS's look, then wide lines
//! with square points), the update window, the Normal Edit Tool's menu
//! of editing tools, envelope points picked and the BananaV Mods window
//! with AI agents on. Run with:
//! `cargo test -p bv-ui-egui --test telas_rodada -- --ignored --nocapture`
//! The images go to `target/telas/rodada_*.png`. Nothing is saved: no OK
//! is pressed.

use std::path::PathBuf;
use std::time::Duration;

use bv_ui_egui::{BananaApp, Language, apply_theme, egui, set_language};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

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

/// A click of the mouse's main button at `at`: moved there, pressed and
/// let go, a frame for each.
fn mouse_click(h: &mut Harness<'_, BananaApp>, at: egui::Pos2) {
    h.event(egui::Event::PointerMoved(at));
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }
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
    app.update_for_review("0.4.1");
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

/// The markers' flags at the timeline's top, their tails ending in the
/// loop region's bar and their lines down the ruler and the tracks; then
/// the keyframe controller a parameter's Animate button brings up in a
/// transition's window and in the Video Media Generators window (Titles &
/// Text's Text color, its row the color's own).
#[test]
#[ignore = "renders images for visual review"]
fn captura_marcadores_e_keyframes() {
    set_language(Language::English);
    let mut app = BananaApp::new(egui::Context::default());
    app.welcome_for_review("closed");
    app.load_demo();
    let mut h = harness(app);
    snap(&mut h, "marcadores");
    // A transition's window: its first parameter animated, with the mouse.
    assert!(h.state_mut().transition_for_review(true));
    settle(&mut h);
    let at = h
        .query_all_by_label("Animate")
        .next()
        .map(|c| c.rect().center())
        .expect("an Animate button");
    mouse_click(&mut h, at);
    settle(&mut h);
    assert!(
        h.query_by_label("Sync Cursor").is_some(),
        "the keyframe controller"
    );
    snap(&mut h, "transicao_animada");
    // The Video Media Generators window on a Titles & Text event: its Text
    // color animated.
    let mut app = BananaApp::new(egui::Context::default());
    app.welcome_for_review("closed");
    app.load_demo();
    app.titles_for_review();
    let mut h = harness(app);
    settle(&mut h);
    snap(&mut h, "gerador_antes");
    let row = h
        .query_all_by_label("Text color:")
        .next()
        .map(|c| c.rect().center().y)
        .expect("Titles & Text's Text color");
    let at = h
        .query_all_by_label("Animate")
        .map(|c| c.rect().center())
        .min_by(|a, b| (a.y - row).abs().total_cmp(&(b.y - row).abs()))
        .expect("an Animate button");
    mouse_click(&mut h, at);
    settle(&mut h);
    assert!(
        h.query_by_label("Sync Cursor").is_some(),
        "the keyframe controller"
    );
    snap(&mut h, "gerador_animado");
}

/// The Normal Edit Tool's menu of editing tools, opened by its arrow in
/// the timeline's toolbar; then the button after the Slip Tool is picked.
#[test]
#[ignore = "renders images for visual review"]
fn captura_ferramentas_de_edicao() {
    let mut app = BananaApp::new(egui::Context::default());
    app.welcome_for_review("closed");
    app.load_demo();
    // The app takes the language of the user's settings: English here.
    set_language(Language::English);
    let mut h = harness(app);
    settle(&mut h);
    let at = h
        .query_all_by_label("Editing Tool")
        .next()
        .map(|c| c.rect().center())
        .expect("the Normal Edit Tool's arrow");
    mouse_click(&mut h, at);
    snap(&mut h, "ferramentas_menu");
    let slip = h
        .query_all_by_label("Slip Tool")
        .next()
        .map(|c| c.rect().center())
        .expect("the Slip Tool's row");
    mouse_click(&mut h, slip);
    snap(&mut h, "ferramentas_slip");
}

/// Envelope points picked with Ctrl and Shift: an adjustment track's
/// Composite envelope, its points at 4 and 6 s picked (white).
#[test]
#[ignore = "renders images for visual review"]
fn captura_pontos_escolhidos() {
    let mut app = BananaApp::new(egui::Context::default());
    app.welcome_for_review("closed");
    app.load_demo();
    app.envelope_picks_for_review();
    set_language(Language::English);
    let mut h = harness(app);
    snap(&mut h, "pontos_escolhidos");
}

/// Options › BananaV Mods with AI agents on: the window listening (its
/// address in a file of the test's own), Copy MCP configuration.
#[test]
#[ignore = "renders images for visual review"]
fn captura_mods_agentes() {
    let mut app = BananaApp::new(egui::Context::default());
    app.welcome_for_review("closed");
    app.load_demo();
    let file = std::env::temp_dir().join(format!("bv-agents-telas-{}.json", std::process::id()));
    app.agents_for_review(file.clone());
    set_language(Language::English);
    let mut h = harness(app);
    snap(&mut h, "mods_agentes");
    drop(h);
    let _ = std::fs::remove_file(file);
}
