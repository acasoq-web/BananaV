//! Off-screen captures of the interface in the partial languages: the main
//! window, its File menu and the render dialog, each language with the
//! computer's fonts for its letters; and the welcome screen's Language
//! page, all in one test (the language is the program's). Run with:
//! `cargo test -p bv-ui-egui --test telas_idiomas -- --ignored --nocapture`
//! The images go to `target/telas/idioma_<tag>_*.png`. Its own test
//! binary: the language switch does not reach the other captures. Nothing
//! is rendered, saved or picked.

use std::path::PathBuf;
use std::time::Duration;

use bv_ui_egui::{BananaApp, Language, apply_theme, egui, install_system_fonts, set_language, tr};
use egui_kittest::Harness;
use egui_kittest::kittest::{self, Queryable};

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
    img.save(dir.join(format!("idioma_{name}.png")))
        .expect("save");
}

fn harness(app: BananaApp, lang: Language) -> Harness<'static, BananaApp> {
    let h = Harness::builder()
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
    install_system_fonts(&h.ctx, lang);
    h
}

const PARTIAL: [Language; 18] = [
    Language::ChineseSimplified,
    Language::Russian,
    Language::SpanishSpain,
    Language::German,
    Language::Japanese,
    Language::French,
    Language::Polish,
    Language::Korean,
    Language::Turkish,
    Language::ChineseTraditional,
    Language::SpanishLatinAmerica,
    Language::Thai,
    Language::Ukrainian,
    Language::Italian,
    Language::Czech,
    Language::Hungarian,
    Language::PortuguesePortugal,
    Language::Swedish,
];

#[test]
#[ignore = "renders images for visual review"]
fn captura_idiomas_parciais() {
    for lang in PARTIAL {
        let tag = lang.code().to_lowercase();
        let mut app = BananaApp::new(egui::Context::default());
        set_language(lang);
        app.load_demo();
        app.welcome_for_review("closed");
        let mut h = harness(app, lang);
        settle(&mut h);
        snap(&mut h, &format!("{tag}_janela"));
        // The menu bar's File, at the window's top, then its Render….
        let file = tr("File");
        let at = h
            .get_all_by_label(file)
            .map(|n| n.rect().center())
            .find(|at| at.y < 40.0)
            .expect("File");
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
        snap(&mut h, &format!("{tag}_arquivo"));
        let render = tr("Render…");
        h.get(kittest::by().predicate(move |n| n.label().is_some_and(|l| l.starts_with(render))))
            .click();
        snap(&mut h, &format!("{tag}_render"));
    }
    // The welcome screen's Language page, in English (in this test: the
    // language is one setting for the whole program, and tests run side
    // by side).
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::English);
    app.welcome_for_review("new");
    let mut h = harness(app, Language::English);
    settle(&mut h);
    h.get_by_label("Language").click();
    snap(&mut h, "boas_vindas");
}
