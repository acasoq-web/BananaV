//! Off-screen captures of Options › Preferences › Display's looks: the
//! interface types other than Dark (Medium, Light, White), the tabs at the
//! top of docked windows with the system's statistics in the status bar,
//! and the docking area hidden. Run with:
//! `cargo test -p bv-ui-egui --test telas_temas -- --ignored --nocapture`
//! The images go to `target/telas/`. Its own test binary: the interface
//! type is global, so it must not reach the other captures.

use std::path::PathBuf;
use std::time::Duration;

use bv_ui_egui::{BananaApp, Language, apply_theme, egui, set_language};
use egui_kittest::Harness;
use egui_kittest::kittest::{self, Queryable};

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
    img.save(dir.join(format!("{name}.png"))).expect("save");
}

#[test]
#[ignore = "renders images for visual review"]
fn captura_tipos_de_interface() {
    let looks: [(&[&str], &str, bool); 5] = [
        (&["Medium"], "tema_medio", true),
        (&["Light"], "tema_claro", true),
        (&["White"], "tema_branco", true),
        (
            &["Dark", "tabs at top", "statistics"],
            "exibicao_abas_no_topo",
            false,
        ),
        (&["Dark", "hide dock"], "exibicao_area_oculta", false),
    ];
    for (look, name, preferences) in looks {
        let mut app = BananaApp::new(egui::Context::default());
        set_language(Language::English);
        app.load_demo();
        app.welcome_for_review("closed");
        app.display_for_review(look);
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
        snap(&mut h, &format!("{name}_principal"));
        if preferences {
            // The Display page in that look. Nothing is applied: OK/Apply
            // would write the settings (never saved here anyway).
            h.get_by_label("Options").click();
            settle(&mut h);
            h.get(
                kittest::by()
                    .predicate(|n| n.label().is_some_and(|l| l.starts_with("Preferences…"))),
            )
            .click();
            settle(&mut h);
            h.get_by_label("Display").click();
            snap(&mut h, &format!("{name}_preferencias"));
            h.get_by_label("Cancel").click();
        }
    }
}
