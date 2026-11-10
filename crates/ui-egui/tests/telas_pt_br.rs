//! Off-screen captures of the interface in Brazilian Portuguese, the
//! secondary language. Run with:
//! `cargo test -p bv-ui-egui --test telas_pt_br -- --ignored --nocapture`
//! The images go to `target/telas/pt_br_*.png`. Its own test binary, so the
//! language switch does not reach the English captures.

use std::path::PathBuf;
use std::time::Duration;

use bv_ui_egui::{BananaApp, Language, apply_theme, egui, set_language};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn snap(h: &mut Harness<'_, BananaApp>, name: &str) {
    for _ in 0..30 {
        h.step();
        std::thread::sleep(Duration::from_millis(50));
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
    std::fs::create_dir_all(&dir).expect("output folder");
    let img = h.render().expect("render");
    img.save(dir.join(format!("pt_br_{name}.png")))
        .expect("save");
}

/// A left click at `at`.
fn click_at(h: &mut Harness<'_, BananaApp>, at: egui::Pos2) {
    h.event(egui::Event::PointerMoved(at));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
        h.step();
    }
}

#[test]
#[ignore = "renders images for visual review"]
fn janelas_da_area_inicial() {
    // The left group's windows in Portuguese (their longer names), at the
    // width of a 1366-pixel screen.
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1366.0, 768.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    snap(&mut h, "inicio_explorer");
    for (window, name) in [
        ("Mídia do projeto", "inicio_midia"),
        ("Geradores de mídia", "inicio_geradores"),
        ("Transições", "inicio_transicoes"),
        ("Efeitos de vídeo", "inicio_efeitos"),
    ] {
        h.state_mut().show_tab_named(window);
        snap(&mut h, name);
    }
    // The Video Preview's quality menu (nothing chosen: the quality is kept
    // with the user's settings) and the Video Media Generators window.
    h.get_by_label("Qualidade da prévia").click();
    for _ in 0..10 {
        h.step();
    }
    let best = h.get_by_label_contains("Melhor").rect().center();
    h.event(egui::Event::PointerMoved(best));
    snap(&mut h, "inicio_previa_qualidade");
    h.key_press(egui::Key::Escape);
    h.state_mut().generated_media_for_review();
    snap(&mut h, "inicio_gerador_janela");
}

#[test]
#[ignore = "renders images for visual review"]
fn captures_in_portuguese() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
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
    snap(&mut h, "principal");
    // The track headers' More (⋯) menus: video, then audio.
    for (y, file) in [(578.0, "menu_trilha_video"), (656.0, "menu_trilha_audio")] {
        click_at(&mut h, egui::pos2(19.0, y));
        snap(&mut h, file);
        // A click elsewhere closes it.
        click_at(&mut h, egui::pos2(800.0, 300.0));
        for _ in 0..5 {
            h.step();
        }
    }
    for (menu, file) in [("Arquivo", "menu_arquivo"), ("Editar", "menu_editar")] {
        h.get_by_label(menu).click();
        snap(&mut h, file);
        h.get_by_label(menu).click();
    }
    h.get_by_label("Opções").click();
    snap(&mut h, "menu_opcoes");
    h.get_by_label_contains("Preferências…").click();
    // A new window is laid out invisibly on its first frame.
    for _ in 0..5 {
        h.step();
    }
    snap(&mut h, "preferencias");
    h.get_by_label("Idioma").click();
    snap(&mut h, "preferencias_idioma");
    h.get_by_label("Vídeo").click();
    snap(&mut h, "preferencias_video");
    h.get_by_label("Indicador da tomada ativa").click();
    snap(&mut h, "seletor_de_cor");
    h.get_all_by_label("Cancelar")
        .last()
        .expect("the picker's Cancel")
        .click();
    h.get_by_label("E/S de arquivo").click();
    snap(&mut h, "preferencias_es_arquivo");
    h.get_by_label("Dispositivo de prévia").click();
    snap(&mut h, "preferencias_dispositivo_previa");
    h.get_by_label("Áudio").click();
    snap(&mut h, "preferencias_audio");
    h.get_by_label("Dispositivo de áudio").click();
    snap(&mut h, "preferencias_dispositivo_audio");
    h.get_by_label("MIDI").click();
    snap(&mut h, "preferencias_midi");
    // The VST folders are searched in the background: the list once found.
    h.get_by_label("Efeitos VST").click();
    for _ in 0..40 {
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    snap(&mut h, "preferencias_vst");
    h.get_by_label("Edição").click();
    snap(&mut h, "preferencias_edicao");
    h.get_by_label("Configurações de CD").click();
    snap(&mut h, "preferencias_cd");
    h.get_by_label("Sincronização").click();
    snap(&mut h, "preferencias_sincronia");
    h.get_by_label("Controle externo e automação").click();
    snap(&mut h, "preferencias_controle_externo");
    h.get_by_label("Recursos obsoletos").click();
    snap(&mut h, "preferencias_obsoletos");
    h.get_by_label("Exibição").click();
    snap(&mut h, "preferencias_exibicao");
    h.get_by_label("Cancelar").click();
    for _ in 0..5 {
        h.step();
    }
    // Arquivo › Propriedades, aba Vídeo.
    h.get_by_label("Arquivo").click();
    h.step();
    h.get_by_label_contains("Propriedades…").click();
    for _ in 0..5 {
        h.step();
    }
    snap(&mut h, "propriedades_projeto");
    h.get_by_label("Áudio").click();
    snap(&mut h, "propriedades_projeto_audio");
    h.get_by_label("Régua").click();
    snap(&mut h, "propriedades_projeto_regua");
    h.get_by_label("Resumo").click();
    snap(&mut h, "propriedades_projeto_resumo");
    h.get_by_label("CD de áudio").click();
    snap(&mut h, "propriedades_projeto_cd");
    h.get_by_label("Avançado").click();
    snap(&mut h, "propriedades_projeto_avancado");
    h.get_by_label("Cancelar").click();
    for _ in 0..3 {
        h.step();
    }
    // Ferramentas › Seletivamente, fazer a pré-renderização do vídeo.
    h.get_by_label("Ferramentas").click();
    h.step();
    h.get_by_label_contains("pré-renderização do vídeo").click();
    for _ in 0..5 {
        h.step();
    }
    snap(&mut h, "prerender_video");
}

/// The Render window in Portuguese (nothing pressed that would save the
/// user's settings).
#[test]
#[ignore = "renders images for visual review"]
fn render_in_portuguese() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
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
    for _ in 0..10 {
        h.step();
    }
    // Arquivo › Renderizar… (Ctrl+R grava).
    h.get_by_label("Arquivo").click();
    h.step();
    h.get_by_label_contains("Renderizar…").click();
    for _ in 0..10 {
        h.step();
    }
    h.get_by_label("Caso de uso").click();
    snap(&mut h, "render_caso_de_uso");
    h.get_by_label("Sequência de imagens").click();
    snap(&mut h, "render_imagens");
    h.get_by_label("Filtrar os modelos").click();
    snap(&mut h, "render_filtro");
    h.key_press(egui::Key::Escape);
    for _ in 0..5 {
        h.step();
    }
    h.get_by_label("Opções de renderização").click();
    snap(&mut h, "render_opcoes");
    h.key_press(egui::Key::Escape);
    for _ in 0..5 {
        h.step();
    }
    h.get_by_label_contains("Personalizar").click();
    snap(&mut h, "render_personalizar");
}

/// The render progress window and "Cancel Warning" in Portuguese (the
/// render is canceled at the end: it leaves no file).
#[test]
#[ignore = "renders images for visual review"]
fn render_progress_in_portuguese() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated");
    std::fs::create_dir_all(&dir).expect("output folder");
    app.render_for_review(dir.join("progresso_pt.mp4"), 10.0, (1920, 1080));
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
    for _ in 0..20 {
        h.step();
        std::thread::sleep(Duration::from_millis(40));
    }
    // The window's (the status bar's, the lowest, takes no clicks while
    // the window is up).
    let cancel = h
        .get_all_by_label("Cancelar")
        .map(|n| n.rect())
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("Cancelar");
    click_at(&mut h, cancel.center());
    for _ in 0..3 {
        h.step();
    }
    let dir_out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
    let img = h.render().expect("render");
    img.save(dir_out.join("pt_br_render_progresso.png"))
        .expect("save");
    h.get_by_label("Sim").click();
    for _ in 0..40 {
        h.step();
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "renders images for visual review"]
fn folder_blocked_in_portuguese() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    app.folder_blocked_for_review(std::path::Path::new(r"C:\Users\User\Videos"));
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
    for _ in 0..6 {
        h.step();
    }
    assert!(h.query_by_label("Abrir a Segurança do Windows").is_some());
    let dir_out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
    let img = h.render().expect("render");
    img.save(dir_out.join("pt_br_render_pasta_bloqueada.png"))
        .expect("save");
}

#[test]
#[ignore = "renders images for visual review"]
fn image_sequence_in_portuguese() {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated/sequencia");
    std::fs::create_dir_all(&dir).expect("folder");
    let first = dir.join("shot_0001.png");
    if !first.exists() {
        let st = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg("testsrc2=size=320x180:rate=24:duration=1")
            .arg(dir.join("shot_%04d.png"))
            .status()
            .expect("ffmpeg on the PATH");
        assert!(st.success());
    }
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    app.image_sequence_for_review(first);
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
    for _ in 0..6 {
        h.step();
    }
    assert!(h.query_by_label("Última imagem:").is_some());
    let dir_out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
    let img = h.render().expect("render");
    img.save(dir_out.join("pt_br_sequencia_de_imagens.png"))
        .expect("save");
}

/// A right click at `at`.
fn right_click_at(h: &mut Harness<'_, BananaApp>, at: egui::Pos2) {
    h.event(egui::Event::PointerMoved(at));
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
        h.step();
    }
}

#[test]
#[ignore = "renders images for visual review"]
fn menus_de_atalho() {
    // The shortcut menus of a video event and of the Video Preview, in
    // Portuguese (nothing is chosen in them).
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1920.0, 1035.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    snap(&mut h, "menus_inicio");
    let away = egui::pos2(1500.0, 19.0);
    let track = h.state().timeline_spot_for_review(0, 20.0).expect("track");
    right_click_at(&mut h, track);
    snap(&mut h, "menu_trilha");
    click_at(&mut h, away);
    let event = h.state().timeline_spot_for_review(0, 7.0).expect("event");
    right_click_at(&mut h, event);
    snap(&mut h, "menu_evento");
    click_at(&mut h, away);
    let frame = h.state().preview_spot_for_review();
    right_click_at(&mut h, frame);
    snap(&mut h, "menu_previa");
    click_at(&mut h, away);
}

#[test]
#[ignore = "renders images for visual review"]
fn boas_vindas_em_portugues() {
    // The welcome screen in Portuguese, on a 1366-pixel screen; only its
    // pages are changed (nothing created, opened or saved).
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.welcome_for_review("new");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1366.0, 768.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    snap(&mut h, "boas_vindas_novo");
    for (page, name) in [
        ("Abrir projeto", "boas_vindas_abrir"),
        ("Começando", "boas_vindas_comecar"),
        ("Idioma", "boas_vindas_idioma"),
    ] {
        h.get_by_label(page).click();
        snap(&mut h, name);
    }
}

/// A VST 3 plug-in's page in Portuguese: BananaV's generic editor for
/// its test plug-ins (built and put in a bundle as a vendor ships one).
#[test]
#[ignore = "renders images for visual review"]
fn vst3_em_portugues() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = root.join("target").join("vst3-testplug");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = std::process::Command::new(cargo)
        .current_dir(&root)
        .args(["build", "-q", "-p", "bv-vst3-testplug", "--target-dir"])
        .arg(&target)
        .status()
        .expect("cargo runs");
    assert!(status.success());
    let built = target.join("debug").join(format!(
        "{}bv_vst3_testplug{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    let folder = std::env::temp_dir().join(format!("bananav-pt-vst3-{}", std::process::id()));
    let bundle = folder.join("BananaV Tests.vst3");
    std::fs::create_dir_all(&bundle).unwrap();
    mac_info_plist(&bundle);
    // On a Mac, Core Foundation is not asked before the executable is in:
    // it keeps a bundle as it first saw it for as long as the process runs.
    let library = if cfg!(target_os = "macos") {
        bundle.join("Contents").join("MacOS").join("BananaV Tests")
    } else {
        bv_vst3::library_of(&bundle)
    };
    std::fs::create_dir_all(library.parent().unwrap()).unwrap();
    std::fs::copy(&built, &library).unwrap();

    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    assert!(app.vst_effect_for_review(&bundle));
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
    snap(&mut h, "vst3_editor_generico");
    assert!(
        h.query_all_by_label("Mostrar a janela do plug-in")
            .next()
            .is_some()
    );
    let _ = std::fs::remove_dir_all(&folder);
}

/// The Modern Equalizer in Portuguese: its window's longer texts.
#[test]
#[ignore = "renders images for visual review"]
fn equalizador_moderno_em_portugues() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    assert!(app.modern_eq_for_review());
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
    snap(&mut h, "equalizador_moderno");
}

/// Busses and assignable FX in Portuguese: the Mixing Console's strips and
/// the track header's bus button and multipurpose slider.
#[test]
#[ignore = "renders images for visual review"]
fn barramentos_em_portugues() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    assert!(app.busses_for_review());
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
    snap(&mut h, "barramentos");
}

/// The Color Grading panel in Portuguese: its Color Wheels and HSL Curves.
#[test]
#[ignore = "renders images for visual review"]
fn matizacao_de_cores_em_portugues() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    assert!(app.color_grading_for_review(1));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1400.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    snap(&mut h, "matizacao_rodas");
    h.state_mut().color_grading_for_review(5);
    snap(&mut h, "matizacao_hsl");
    // The pickers: Lift's black point and the Color Selector waiting.
    h.state_mut().color_grading_for_review(1);
    h.state_mut().color_picker_for_review(0);
    snap(&mut h, "matizacao_ponto_preto");
    h.state_mut().color_grading_for_review(3);
    h.state_mut().color_picker_for_review(2);
    snap(&mut h, "matizacao_tom_neutro");
}

/// Project Notes in Portuguese (the date in Portuguese, 24 hours).
#[test]
#[ignore = "renders images for visual review"]
fn project_notes_em_portugues() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    app.project_notes_for_review();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    snap(&mut h, "project_notes");
}

/// Edit Visible Button Set in Portuguese.
#[test]
#[ignore = "renders images for visual review"]
fn botoes_visiveis_em_portugues() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    assert!(app.header_buttons_for_review());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    snap(&mut h, "botoes_visiveis");
}

/// Set Default Track Properties in Portuguese.
#[test]
#[ignore = "renders images for visual review"]
fn propriedades_padrao_de_trilha_em_portugues() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    assert!(app.track_defaults_for_review());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    snap(&mut h, "propriedades_padrao_de_trilha");
}

/// Track groups in Portuguese.
#[test]
#[ignore = "renders images for visual review"]
fn grupos_de_trilhas_em_portugues() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    assert!(app.track_groups_for_review());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1100.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    snap(&mut h, "grupos_de_trilhas");
}

/// The Surround Panner in Portuguese (a 5.1 demo): the window, the
/// keyframes rows and the small panners.
#[test]
#[ignore = "renders images for visual review"]
fn panner_surround_em_portugues() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    assert!(app.surround_for_review());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1400.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    snap(&mut h, "panner_surround");
}

/// The Mixing Console in Portuguese, every control region labelled.
#[test]
#[ignore = "renders images for visual review"]
fn console_de_mixagem_em_portugues() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    assert!(app.mixing_console_for_review());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1500.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    snap(&mut h, "console_de_mixagem");
}

/// Video Scopes in Portuguese (their longer names) in the Color Correction
/// layout: two views with the demo's frame and Video Scope Settings
/// (closed with Cancel); on settings never saved.
#[test]
#[ignore = "renders images for visual review"]
fn alcances_em_portugues() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1920.0, 1040.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    h.state_mut().layout_for_review(2);
    h.state_mut().scopes_for_review(5);
    snap(&mut h, "alcances_vetorscopio_forma_de_onda_histograma");
    h.state_mut().scopes_for_review(7);
    h.state_mut()
        .scope_modes_for_review(true, 6, Some([true, true, false]));
    snap(&mut h, "alcances_todos");
    h.state_mut().scope_settings_for_review();
    snap(&mut h, "alcances_configuracoes");
    h.get_all_by_label("Cancelar")
        .last()
        .expect("Cancelar")
        .click();
    for _ in 0..4 {
        h.step();
    }
}

/// Edit Details' Audio CD Track List and Command Properties in
/// Portuguese.
#[test]
#[ignore = "renders images for visual review"]
fn edit_details_em_portugues() {
    for (show, name) in [
        ("cd", "edit_details_cd"),
        ("events", "edit_details_eventos"),
    ] {
        let mut app = BananaApp::new(egui::Context::default());
        set_language(Language::PortugueseBrazil);
        app.load_demo();
        app.welcome_for_review("closed");
        app.details_for_review(show);
        let mut h = Harness::builder()
            .with_size(egui::vec2(1600.0, 1000.0))
            .with_pixels_per_point(1.0)
            .wgpu()
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        snap(&mut h, name);
    }
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::PortugueseBrazil);
    app.load_demo();
    app.welcome_for_review("closed");
    app.command_properties_for_review();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    snap(&mut h, "command_properties");
}

/// A Mac bundle's Info.plist naming its executable, as vendors ship one
/// (Core Foundation finds the executable through it); nothing elsewhere.
fn mac_info_plist(bundle: &std::path::Path) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let contents = bundle.join("Contents");
    std::fs::create_dir_all(&contents).unwrap();
    let name = bundle.file_stem().unwrap().to_string_lossy();
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>{name}</string>
    <key>CFBundleIdentifier</key>
    <string>org.bananav.tests.vst3</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
</dict>
</plist>
"#
    );
    std::fs::write(contents.join("Info.plist"), plist).unwrap();
}
