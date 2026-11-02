//! Off-screen captures of the main window's docked windows (the Explorer,
//! Project Media, Media Generator, Transitions, Video FX, Video Preview and
//! Master Bus), in English, for visual review. Run with:
//! `cargo test -p bv-ui-egui --test telas_inicio -- --ignored --nocapture`
//! The images go to `target/telas/inicio_*.png`.

use std::path::PathBuf;
use std::time::Duration;

use bv_ui_egui::{BananaApp, Language, apply_theme, egui, set_language};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn settle(h: &mut Harness<'_, BananaApp>) {
    // The preview, thumbnails and waveforms arrive from other threads.
    for _ in 0..30 {
        h.step();
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn snap(h: &mut Harness<'_, BananaApp>, name: &str) {
    settle(h);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
    std::fs::create_dir_all(&dir).expect("output folder");
    let img = h.render().expect("render");
    img.save(dir.join(format!("inicio_{name}.png")))
        .expect("save");
}

fn harness(app: BananaApp) -> Harness<'static, BananaApp> {
    Harness::builder()
        .with_size(egui::vec2(1920.0, 1035.0))
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

/// A click at `at`.
fn click_at(h: &mut Harness<'_, BananaApp>, at: egui::Pos2) {
    h.event(egui::Event::PointerMoved(at));
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

/// The pointer on the widget named `label` (a preset: only it plays).
fn hover(h: &mut Harness<'_, BananaApp>, label: &str) {
    let at = h.get_by_label(label).rect().center();
    h.event(egui::Event::PointerMoved(at));
    h.step();
}

#[test]
#[ignore = "renders images for visual review"]
fn captura_area_inicial() {
    // A new, empty project, as VEGAS starts.
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::English);
    app.welcome_for_review("closed");
    let mut h = harness(app);
    snap(&mut h, "vazio");

    // The demo project, each window of the left group in turn (the View ›
    // Window commands bring them to the front).
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::English);
    app.load_demo();
    app.welcome_for_review("closed");
    let mut h = harness(app);
    snap(&mut h, "explorer");
    // Project Media with a bin of its own, the first media chosen.
    h.state_mut().bins_for_review();
    for (window, name) in [
        ("Project Media", "midia"),
        ("Media Generators", "geradores"),
        ("Transitions", "transicoes"),
        ("Video FX", "efeitos"),
    ] {
        h.state_mut().show_tab_named(window);
        snap(&mut h, name);
    }
    // The pointer on a preset: it plays (the others stay still).
    h.state_mut().show_tab_named("Transitions");
    settle(&mut h);
    h.get_by_label("Linear Wipe").click();
    settle(&mut h);
    hover(&mut h, "Left-Right, Soft Edge");
    snap(&mut h, "transicoes_sob_o_ponteiro");
    // GL Transition: the open collection's transitions as its presets,
    // the pointer on Cube.
    h.get_by_label("GL Transition").click();
    settle(&mut h);
    hover(&mut h, "Cube");
    snap(&mut h, "transicoes_gl");
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Add Noise").click();
    settle(&mut h);
    hover(&mut h, "(Default)");
    for _ in 0..12 {
        h.step();
        std::thread::sleep(Duration::from_millis(40));
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
    let img = h.render().expect("render");
    img.save(dir.join("inicio_efeitos_sob_o_ponteiro.png"))
        .expect("save");
    // Generators BananaV's plug-ins make: Credit Roll's presets (the one
    // under the pointer scrolling) and Noise Texture's.
    h.state_mut().show_tab_named("Media Generators");
    settle(&mut h);
    h.get_by_label("Credit Roll").click();
    settle(&mut h);
    hover(&mut h, "Over Black");
    snap(&mut h, "geradores_credit_roll");
    h.get_by_label("Noise Texture").click();
    settle(&mut h);
    snap(&mut h, "geradores_noise_texture");

    // The Master Bus's meters lit: −6 dB on the left, a clip on the right.
    h.state_mut().meters_for_review(vec![0.5, 1.05]);
    h.step();
    let img = h.render().expect("render");
    img.save(dir.join("inicio_master_aceso.png")).expect("save");

    // The Video Preview's menus: the quality (Best's sizes open), the zoom,
    // the overlays and Split-Screen View. Nothing is chosen in them (the
    // quality is kept with the user's settings); a click on the menu bar's
    // empty end closes each.
    let away = egui::pos2(1500.0, 19.0);
    h.get_by_label("Preview Quality").click();
    settle(&mut h);
    // The submenu's label ends in its arrow.
    let best = h.get_by_label_contains("Best").rect().center();
    h.event(egui::Event::PointerMoved(best));
    h.step();
    snap(&mut h, "previa_qualidade");
    click_at(&mut h, away);
    let overlays = h.get_by_label("Overlays").rect();
    click_at(
        &mut h,
        egui::pos2(overlays.left() - 8.0, overlays.center().y),
    );
    snap(&mut h, "previa_zoom");
    click_at(&mut h, away);
    click_at(
        &mut h,
        egui::pos2(overlays.right() + 8.0, overlays.center().y),
    );
    snap(&mut h, "previa_overlays");
    click_at(&mut h, away);
    let split = h.get_by_label("Split-Screen View").rect();
    click_at(&mut h, egui::pos2(split.right() + 8.0, split.center().y));
    snap(&mut h, "previa_split_menu");
    click_at(&mut h, away);
    // Split-Screen View on: the right half without the event's Sepia.
    h.state_mut().split_screen_for_review();
    snap(&mut h, "previa_split");
    // The Video Media Generators window of the demo's gradient.
    h.state_mut().generated_media_for_review();
    snap(&mut h, "gerador_janela");
}
