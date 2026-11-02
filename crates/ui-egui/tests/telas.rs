//! Off-screen (wgpu) captures of the interface, in English, for visual
//! review. Run with:
//! `cargo test -p bv-ui-egui --test telas -- --ignored --nocapture`
//! The images go to `target/telas/`. `telas_pt_br.rs` captures the
//! Portuguese interface.

use std::path::PathBuf;
use std::time::Duration;

use bv_ui_egui::{BananaApp, Language, apply_theme, egui, set_language};
use egui_kittest::Harness;
use egui_kittest::kittest::{self, NodeT, Queryable};

fn out_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
    std::fs::create_dir_all(&dir).expect("output folder");
    dir
}

/// A new app shown in English, whatever language the user picked, with the
/// welcome screen closed even when the user's settings show it.
fn new_app() -> BananaApp {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::English);
    app.welcome_for_review("closed");
    // Never the user's filter packages.
    app.fx_packages_for_review();
    app
}

fn settle(h: &mut Harness<'_, BananaApp>) {
    // The preview, thumbnails and waveforms arrive from other threads.
    for _ in 0..30 {
        h.step();
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn snap(h: &mut Harness<'_, BananaApp>, name: &str) {
    settle(h);
    let img = h.render().expect("render");
    img.save(out_dir().join(format!("{name}.png")))
        .expect("save");
}

#[test]
#[ignore = "renders images for visual review"]
fn capturas_da_interface() {
    let mut app = new_app();
    app.load_demo();
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
    // Zoom on the crossfade at 4–5 s (the cursor is at 4.5 s).
    for _ in 0..4 {
        h.key_press(egui::Key::ArrowUp);
    }
    snap(&mut h, "crossfade");
    // Playing, then clicking further on the timeline: the edit cursor
    // (yellow) moves, the playback position (white) keeps going.
    h.key_press(egui::Key::Space);
    let click = egui::pos2(1250.0, 690.0);
    h.event(egui::Event::PointerMoved(click));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: click,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
        h.step();
    }
    snap(&mut h, "duas_agulhas");
    h.key_press(egui::Key::Space);
    settle(&mut h);
    for (tab, file) in [
        ("Transitions", "transicoes"),
        ("Video FX", "efeitos"),
        ("Color Grading", "cor"),
        ("Media Generators", "geradores"),
        ("Explorer", "explorador"),
    ] {
        h.state_mut().show_tab_named(tab);
        snap(&mut h, file);
    }
    for (tab, file) in [
        ("Properties", "propriedades"),
        ("Trimmer", "trimmer"),
        ("Video Scopes", "alcances"),
        ("Edit Details", "detalhes"),
        ("Mixing Console", "console"),
        ("Plug-In Manager", "plugins"),
    ] {
        h.state_mut().show_tab_named(tab);
        snap(&mut h, file);
    }
    // Track envelopes (Insert › Audio Envelopes: Shift+V, Shift+P).
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::V);
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::P);
    snap(&mut h, "envelopes");
    // Menus, opened with simulated clicks; submenus by hovering.
    for (menu, file, submenus) in [
        (
            "File",
            "menu_arquivo",
            &[
                ("Import", "menu_arquivo_importar"),
                ("Export", "menu_arquivo_exportar"),
            ][..],
        ),
        (
            "Edit",
            "menu_editar",
            &[
                ("Navigate", "menu_editar_navegar"),
                ("Switches", "menu_editar_interruptores"),
            ][..],
        ),
        (
            "View",
            "menu_visualizar",
            &[
                ("Window", "menu_visualizar_janela"),
                ("Window Layouts", "menu_visualizar_layouts"),
            ][..],
        ),
        (
            "Insert",
            "menu_inserir",
            &[("Audio Envelopes", "menu_inserir_envelopes")][..],
        ),
        (
            "Tools",
            "menu_ferramentas",
            &[("Scripting", "menu_ferramentas_scripting")][..],
        ),
        (
            "Options",
            "menu_opcoes",
            &[
                ("Ruler Time Format", "menu_opcoes_regua"),
                ("Ripple", "menu_opcoes_ondulacao"),
            ][..],
        ),
        ("Help", "menu_ajuda", &[][..]),
    ] {
        h.get_by_label(menu).click();
        snap(&mut h, file);
        for (sub, file) in submenus {
            // The submenu whose label is `sub` plus its arrow: "Window" is
            // also part of "Window Layouts".
            h.get(kittest::by().predicate(move |n| {
                n.label().is_some_and(|l| {
                    l.strip_prefix(sub)
                        .is_some_and(|rest| !rest.chars().any(char::is_alphanumeric))
                })
            }))
            .hover();
            snap(&mut h, file);
        }
        h.get_by_label(menu).click();
        settle(&mut h);
    }
    // Options › Customize Keyboard.
    h.get_by_label("Options").click();
    settle(&mut h);
    h.get_by_label_contains("Customize Keyboard").click();
    snap(&mut h, "personalizar_teclado");
    // The TrackList tab: the same commands, with the track-header keys.
    h.get_by_label("TrackList").click();
    snap(&mut h, "personalizar_teclado_tracklist");
    // The Explorer tab: VEGAS's menu commands, none with a default key.
    h.get_by_label("Explorer").click();
    snap(&mut h, "personalizar_teclado_explorer");
    h.get_by_label("Trimmer").click();
    snap(&mut h, "personalizar_teclado_trimmer");
    for (tab, file) in [
        ("Mixer", "personalizar_teclado_mixer"),
        ("EditDetails", "personalizar_teclado_editdetails"),
        ("TrackMotion", "personalizar_teclado_trackmotion"),
        ("Mixing Console", "personalizar_teclado_console"),
        ("Global", "personalizar_teclado_global"),
    ] {
        // The Master Bus window has a Mixing Console button too: the
        // dialog's tab is the one nearest TrackMotion's.
        let near = h.get_by_label("TrackMotion").rect().center();
        let at = h
            .get_all_by_label(tab)
            .map(|n| n.rect().center())
            .min_by(|a, b| a.distance(near).total_cmp(&b.distance(near)))
            .expect("tab");
        click_at(&mut h, at);
        snap(&mut h, file);
    }
    h.get_by_label("TrackList").click();
    settle(&mut h);
    let filter = h.get_by_role(egui::accesskit::Role::TextInput);
    filter.click();
    filter.type_text("TrackView.Track.");
    snap(&mut h, "personalizar_teclado_tracklist_trilhas");
    h.get_by_label("Cancel").click();
    settle(&mut h);
    // Options › Preferences, with BananaV's own Language tab. Nothing is
    // applied: OK/Apply would write the user's settings file.
    h.get_by_label("Options").click();
    settle(&mut h);
    h.get(kittest::by().predicate(|n| n.label().is_some_and(|l| l.starts_with("Preferences…"))))
        .click();
    snap(&mut h, "preferencias");
    h.get_by_label("Language").click();
    snap(&mut h, "preferencias_idioma");
    // Video: the graphics card that composites; File I/O: decoding on it.
    h.get_by_label("Video").click();
    snap(&mut h, "preferencias_video");
    // A color swatch opens VEGAS's color picker; its model button turns the
    // bars to hue, saturation and luminance. Its Cancel changes nothing.
    h.get_by_label("Active take indicator").click();
    snap(&mut h, "seletor_de_cor");
    h.get_by_label("Red, green and blue bars, or hue, saturation and luminance ones")
        .click();
    snap(&mut h, "seletor_de_cor_hsl");
    h.get_all_by_label("Cancel")
        .last()
        .expect("the picker's Cancel")
        .click();
    h.get_by_label("File I/O").click();
    snap(&mut h, "preferencias_es_arquivo");
    // Preview Device (the external monitor), Audio and Audio Device.
    h.get_by_label("Preview Device").click();
    snap(&mut h, "preferencias_dispositivo_previa");
    h.get_by_label("Audio").click();
    snap(&mut h, "preferencias_audio");
    h.get_by_label("Audio Device").click();
    snap(&mut h, "preferencias_dispositivo_audio");
    h.get_by_label("MIDI").click();
    snap(&mut h, "preferencias_midi");
    // The VST folders are searched in the background: the list once found.
    h.get_by_label("VST Effects").click();
    for _ in 0..40 {
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    snap(&mut h, "preferencias_vst");
    h.get_by_label("Editing").click();
    snap(&mut h, "preferencias_edicao");
    h.get_by_label("CD Settings").click();
    snap(&mut h, "preferencias_cd");
    h.get_by_label("Sync").click();
    snap(&mut h, "preferencias_sincronia");
    h.get_by_label("External Control & Automation").click();
    snap(&mut h, "preferencias_controle_externo");
    h.get_by_label("Deprecated Features").click();
    snap(&mut h, "preferencias_obsoletos");
    h.get_by_label("Display").click();
    snap(&mut h, "preferencias_exibicao");
    h.get_by_label("Cancel").click();
    settle(&mut h);
    // A click on a track header selects the track and gives the keys to the
    // track list (its header turns blue); Down moves to the next track.
    let header = egui::pos2(120.0, 640.0);
    h.event(egui::Event::PointerMoved(header));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: header,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
        h.step();
    }
    h.key_press(egui::Key::ArrowDown);
    snap(&mut h, "trilha_selecionada");
    // Options › Customize Timeline Toolbar (nothing is changed: that would
    // write the user's settings).
    h.get_by_label("Options").click();
    settle(&mut h);
    h.get_by_label_contains("Customize Timeline Toolbar")
        .click();
    snap(&mut h, "personalizar_barra_timeline");
    // Scrolled to the long names: they end in "…" inside the list.
    wheel(&mut h, egui::pos2(900.0, 500.0), -420.0);
    snap(&mut h, "personalizar_barra_timeline_rolada");
    h.get_by_label("Close").click();
    settle(&mut h);
    // Options › Customize Toolbar (the main toolbar under the menus).
    h.get_by_label("Options").click();
    settle(&mut h);
    h.get_by_label_contains("Customize Toolbar…").click();
    snap(&mut h, "personalizar_barra_principal");
    wheel(&mut h, egui::pos2(620.0, 500.0), -760.0);
    snap(&mut h, "personalizar_barra_principal_rolada");
    h.get_by_label("Close").click();
    settle(&mut h);
    // File › Properties, Video tab (closed with Cancel: nothing changes).
    h.get_by_label("File").click();
    settle(&mut h);
    h.get(kittest::by().predicate(|n| n.label().is_some_and(|l| l.starts_with("Properties…"))))
        .click();
    snap(&mut h, "propriedades_projeto");
    // The pixel formats: the video-levels ones are greyed out ("Coming soon").
    click_at(&mut h, egui::pos2(340.0, 254.0));
    snap(&mut h, "propriedades_projeto_formato");
    h.key_press(egui::Key::Escape);
    settle(&mut h);
    h.get_by_label("Audio").click();
    snap(&mut h, "propriedades_projeto_audio");
    // 5.1 Surround turns the LFE filter on (only in the dialog: Cancel).
    click_at(&mut h, egui::pos2(388.0, 107.0));
    settle(&mut h);
    h.get_by_label("5.1 Surround").click();
    snap(&mut h, "propriedades_projeto_audio_51");
    h.get_by_label("Ruler").click();
    snap(&mut h, "propriedades_projeto_regua");
    h.get_by_label("Summary").click();
    snap(&mut h, "propriedades_projeto_resumo");
    h.get_by_label("Audio CD").click();
    snap(&mut h, "propriedades_projeto_cd");
    h.get_by_label("Advanced").click();
    snap(&mut h, "propriedades_projeto_avancado");
    h.get_by_label("Cancel").click();
    settle(&mut h);
    // Options › Ruler Time Format › Measures & Beats: the ruler counts
    // measures and beats at the project tempo.
    h.get_by_label("Options").click();
    settle(&mut h);
    h.get(kittest::by().predicate(|n| {
        n.label().is_some_and(|l| {
            l.strip_prefix("Ruler Time Format")
                .is_some_and(|rest| !rest.chars().any(char::is_alphanumeric))
        })
    }))
    .hover();
    settle(&mut h);
    h.get_by_label_contains("Measures & Beats").click();
    snap(&mut h, "regua_compassos");
    // Tools › Selectively Prerender Video, closed with Cancel.
    h.get_by_label("Tools").click();
    settle(&mut h);
    h.get_by_label_contains("Selectively Prerender Video")
        .click();
    snap(&mut h, "prerender_video");
    h.get_by_label("Cancel").click();
    settle(&mut h);
    // Expanded Edit Mode at the sunset's start: the color bars on the A
    // roll, the sunset on the B roll, a red bracket on the edge, and both
    // sides of the edit point in the Video Preview.
    h.state_mut().show_tab_named("Video Preview");
    h.state_mut().expanded_edit_for_review(4.0);
    snap(&mut h, "edicao_expandida");
    h.key_press(egui::Key::Escape);
    settle(&mut h);
    // Video Preview on External Monitor (here a window in the main one; in
    // BananaV, full screen on the display of Options › Preferences ›
    // Preview Device).
    h.get_by_label("Video Preview on External Monitor (Alt+Shift+4)")
        .click();
    settle(&mut h);
    snap(&mut h, "monitor_externo");
    h.get_by_label("Video Preview on External Monitor (Alt+Shift+4)")
        .click();
    settle(&mut h);
}

/// A video with two audio streams: each gets its own audio track. Stretched
/// past its 2 s, the events loop (notches where the media starts over); a
/// double-click on "Level" opens the box to type it.
#[test]
#[ignore = "renders images for visual review"]
fn captura_streams_e_loop() {
    let video = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-media/generated/dois_streams_de_audio.mp4");
    assert!(
        video.exists(),
        "generate it with: cargo test -p bv-media --test streams -- --ignored"
    );
    let mut app = new_app();
    assert!(app.import_and_place(&video), "import failed");
    app.stretch_selection_to(6.5);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 960.0))
        .with_pixels_per_point(1.0)
        // Real-time steps: a double-click needs its clicks 0.3 s apart at most.
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    h.state_mut().seek_seconds(3.0);
    snap(&mut h, "streams_e_loop");
    // Dragging the end edge near where the media starts over (6 s): it
    // sticks there, with the snap line, like VEGAS's.
    let from = egui::pos2(1066.0, 600.0);
    let to = egui::pos2(1015.0, 600.0);
    h.event(egui::Event::PointerMoved(from));
    h.step();
    h.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    for i in 1..=10 {
        h.event(egui::Event::PointerMoved(
            from + (to - from) * (i as f32 / 10.0),
        ));
        h.step();
    }
    snap(&mut h, "snap_fim_da_midia");
    h.event(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    settle(&mut h);
    // Track 1 taller, by its header's bottom edge.
    drag(
        &mut h,
        egui::pos2(120.0, 649.0),
        egui::pos2(120.0, 700.0),
        None,
    );
    snap(&mut h, "trilha_redimensionada");
    // Track 3 above track 2, by its header: the line shows where it lands.
    drag(
        &mut h,
        egui::pos2(60.0, 780.0),
        egui::pos2(60.0, 712.0),
        Some("trilha_arrastando"),
    );
    snap(&mut h, "trilha_reordenada");
    // Zoomed far in: the waveform of the samples themselves (a click in the
    // timeline gives it the keys back from the track list).
    click_at(&mut h, egui::pos2(1300.0, 850.0));
    h.state_mut().seek_seconds(0.5);
    for _ in 0..24 {
        h.key_press(egui::Key::ArrowUp);
    }
    snap(&mut h, "onda_detalhada");
    let level = egui::pos2(60.0, 608.0);
    click_at(&mut h, level);
    click_at(&mut h, level);
    snap(&mut h, "nivel_digitado");
}

/// A speech-like recording (bursts and pauses, AAC, 60 fps): its waveform
/// at about VEGAS's zoom in the user's screenshot, continuous in the pauses
/// too.
#[test]
#[ignore = "renders images for visual review"]
fn captura_onda_fala() {
    let video = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-media/generated/fala_simulada.mp4");
    if !video.exists() {
        let st = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc2=s=640x360:r=60:d=20"])
            .args([
                "-f",
                "lavfi",
                "-i",
                r"aevalsrc=0.4*(random(0)-0.5)*gt(sin(2*PI*0.7*t)+sin(2*PI*1.9*t)\,0.6)*abs(sin(2*PI*4*t))+0.003*(random(1)-0.5):s=48000:d=20",
            ])
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"])
            .args(["-c:a", "aac", "-ac", "2"])
            .arg(&video)
            .status()
            .expect("ffmpeg on the PATH");
        assert!(st.success());
    }
    let mut app = new_app();
    assert!(app.import_and_place(&video), "import failed");
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
    h.state_mut().seek_seconds(3.0);
    // About 140 pixels per second, like the screenshot.
    for _ in 0..3 {
        h.key_press(egui::Key::ArrowUp);
    }
    snap(&mut h, "onda_fala");
    // The audio event's level line under the pointer: the blue handle and
    // "Gain is 0.0 dB".
    h.event(egui::Event::PointerMoved(egui::pos2(800.0, 654.0)));
    snap(&mut h, "ganho_hover");
    // The video event's opacity dragged down to about half.
    drag(
        &mut h,
        egui::pos2(800.0, 576.0),
        egui::pos2(800.0, 609.0),
        Some("opacidade_arrastando"),
    );
}

/// A drag from `from` to `to` in small steps; `mid` names a capture taken
/// before the button is released.
fn drag(h: &mut Harness<'_, BananaApp>, from: egui::Pos2, to: egui::Pos2, mid: Option<&str>) {
    h.event(egui::Event::PointerMoved(from));
    h.step();
    h.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    for i in 1..=12 {
        h.event(egui::Event::PointerMoved(
            from + (to - from) * (i as f32 / 12.0),
        ));
        h.step();
    }
    if let Some(name) = mid {
        snap(h, name);
    }
    h.event(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    settle(h);
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

/// Mouse wheel over `at` (negative `dy` scrolls down).
fn wheel(h: &mut Harness<'_, BananaApp>, at: egui::Pos2, dy: f32) {
    h.event(egui::Event::PointerMoved(at));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, dy),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::default(),
    });
}

#[test]
#[ignore = "renders images for visual review; needs test-media/generated/teste_h264_aac.mp4"]
fn captura_com_video_importado() {
    let video = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-media/generated/teste_h264_aac.mp4");
    assert!(
        video.exists(),
        "generate it with: cargo test -p bv-mf --test oraculo -- --ignored"
    );
    let mut app = new_app();
    assert!(app.import_and_place(&video), "import failed");
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
    h.state_mut().seek_seconds(1.5);
    snap(&mut h, "video_importado");
    // The first video offers its settings to the project: kept as they are.
    h.get_by_label("No").click();
    settle(&mut h);
    // Dragging the media from Project Media onto the timeline: a preview
    // shows where and how long the events will be.
    let start = egui::pos2(60.0, 150.0);
    let end = egui::pos2(1000.0, 610.0);
    h.event(egui::Event::PointerMoved(start));
    h.event(egui::Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    for i in 1..=12 {
        let f = i as f32 / 12.0;
        h.event(egui::Event::PointerMoved(start + (end - start) * f));
        h.step();
    }
    snap(&mut h, "arrasto_preview");
    let away = egui::pos2(1000.0, 300.0);
    h.event(egui::Event::PointerMoved(away));
    h.event(egui::Event::PointerButton {
        pos: away,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    settle(&mut h);
    // The media's markers and regions: in the Trimmer's strip and in the
    // events (View › Event Media Markers, with labels).
    h.state_mut().media_markers_for_review(&[
        (0.5, 0.0, "Start"),
        (1.0, 1.2, "Chorus"),
        (2.6, 0.0, ""),
    ]);
    h.state_mut().show_tab_named("Trimmer");
    snap(&mut h, "video_trimmer");
    h.state_mut().open_clip_window("pancrop");
    snap(&mut h, "clipe_pancrop");
    // Clipe sem efeitos: o botão FX abre o seletor de plug-ins.
    h.state_mut().open_clip_window("fx");
    snap(&mut h, "clipe_seletor");
    for label in ["Vignette", "Add", "OK"] {
        h.get_by_label(label).click();
        settle(&mut h);
    }
    snap(&mut h, "clipe_fx_vinheta");
    h.state_mut().open_clip_window("fx-audio");
    snap(&mut h, "clipe_fx_audio");
    h.state_mut().open_clip_window("extras");
    snap(&mut h, "clipe_extras");
}

/// File › Render As: VEGAS 23's Render window. Nothing is pressed that
/// would save the user's settings (no Render, Cancel or favorites).
#[test]
#[ignore = "renders images for visual review"]
fn captura_render() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    // File › Render… (Ctrl+R records, as in VEGAS).
    h.get_by_label("File").click();
    settle(&mut h);
    h.get(kittest::by().predicate(|n| n.label().is_some_and(|l| l.starts_with("Render…"))))
        .click();
    settle(&mut h);
    h.get_by_label("Use Case").click();
    snap(&mut h, "render_use_case");
    h.get_by_label("YouTube").click();
    snap(&mut h, "render_youtube");
    tree_row(&mut h, "Video");
    snap(&mut h, "render_video");
    h.get_by_label("Image Sequence").click();
    snap(&mut h, "render_imagens");
    tree_row(&mut h, "Audio");
    snap(&mut h, "render_audio");
    h.get_by_label("Filter the templates").click();
    snap(&mut h, "render_filtro");
    h.key_press(egui::Key::Escape);
    settle(&mut h);
    h.get_by_label("Render options").click();
    snap(&mut h, "render_opcoes");
    h.key_press(egui::Key::Escape);
    settle(&mut h);
    tree_row(&mut h, "Video");
    settle(&mut h);
    h.get_by_label("Template view").click();
    settle(&mut h);
    h.get_by_label("Details").click();
    snap(&mut h, "render_detalhes");
    h.get_by_label("Template view").click();
    settle(&mut h);
    h.get_by_label("List").click();
    settle(&mut h);
    h.get_by_label_contains("Customize").click();
    snap(&mut h, "render_personalizar");
    // Its Project tab, as VEGAS's (under ACES, the color space list).
    h.state_mut()
        .aces_for_review(Some(bv_timeline::ViewTransform {
            display: bv_timeline::AcesDisplay::Srgb,
            view: bv_timeline::AcesView::Aces,
        }));
    h.get_by_label("Project").click();
    snap(&mut h, "render_personalizar_projeto");
}

/// Project Media's Properties for a media file, as VEGAS's Media tab, under
/// ACES (its Color space list).
#[test]
#[ignore = "renders images for visual review"]
fn captura_propriedades_da_midia() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut()
        .aces_for_review(Some(bv_timeline::ViewTransform {
            display: bv_timeline::AcesDisplay::Srgb,
            view: bv_timeline::AcesView::Aces,
        }));
    h.state_mut().media_properties_for_review();
    settle(&mut h);
    snap(&mut h, "propriedades_da_midia");
}

/// The progress window of a render (of the demo project): running, with
/// its frames in the preview; "Cancel Warning" (answered No); complete.
/// Then a render canceled with Yes leaves no file behind.
#[test]
#[ignore = "renders images for visual review"]
fn captura_render_progresso() {
    let mut app = new_app();
    app.load_demo();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated");
    std::fs::create_dir_all(&dir).expect("output folder");
    let file = dir.join("progresso.mp4");
    // Long enough to be caught running: renders go several times faster
    // than real time.
    app.render_for_review(file.clone(), 90.0, (1920, 1080));
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
    for _ in 0..25 {
        h.step();
        std::thread::sleep(Duration::from_millis(40));
    }
    let img = h.render().expect("render");
    img.save(out_dir().join("render_progresso.png"))
        .expect("save");
    // The window's Cancel asks first (the status bar's, the lowest one,
    // takes no clicks while the window is up).
    let cancel = window_cancel(&h, "Cancel");
    click_at(&mut h, cancel.center());
    for _ in 0..3 {
        h.step();
    }
    let img = h.render().expect("render");
    img.save(out_dir().join("render_cancelar_aviso.png"))
        .expect("save");
    h.get_by_label("No").click();
    // Until it ends (the window closes by itself when the user's settings
    // say so: "Close this dialog when render is complete").
    let done = |h: &Harness<'_, BananaApp>| {
        h.query_by_label("Close").is_some()
            || h.query_by_label_contains("Rendered progresso.mp4")
                .is_some()
    };
    for _ in 0..1200 {
        h.step();
        std::thread::sleep(Duration::from_millis(50));
        if done(&h) {
            break;
        }
    }
    snap(&mut h, "render_concluido");
    assert!(file.exists(), "the render made its file");
    if h.query_by_label("Close").is_some() {
        h.get_by_label("Close").click();
    }
    settle(&mut h);

    // Canceled with Yes: nothing is left.
    let gone = dir.join("cancelado.mp4");
    h.state_mut()
        .render_for_review(gone.clone(), 90.0, (1920, 1080));
    for _ in 0..10 {
        h.step();
        std::thread::sleep(Duration::from_millis(40));
    }
    let cancel = window_cancel(&h, "Cancel");
    click_at(&mut h, cancel.center());
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Yes").click();
    for _ in 0..100 {
        h.step();
        std::thread::sleep(Duration::from_millis(50));
        if h.query_by_label("Estimated Properties").is_none() {
            break;
        }
    }
    snap(&mut h, "render_cancelado");
    assert!(!gone.exists(), "a canceled render leaves no file");
}

/// The render window's Cancel: the highest one (the status bar has one
/// too, at the bottom).
fn window_cancel(h: &Harness<'_, BananaApp>, label: &str) -> egui::Rect {
    h.get_all_by_label(label)
        .map(|n| n.rect())
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("Cancel")
}

/// While a render's window is up the rest of the program takes no input,
/// as VEGAS's (its render window is modal): a click on the ruler leaves
/// the cursor where it was, Ctrl+End does not move it and the status
/// bar's Cancel asks nothing; the window's own Cancel works, and once the
/// window closes the ruler takes clicks again.
#[test]
#[ignore = "renders a file"]
fn render_bloqueia_o_programa() {
    let mut app = new_app();
    app.load_demo();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated");
    std::fs::create_dir_all(&dir).expect("output folder");
    let file = dir.join("bloqueio.mp4");
    app.render_for_review(file.clone(), 90.0, (1920, 1080));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 960.0))
        .with_pixels_per_point(1.0)
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    for _ in 0..10 {
        h.step();
        std::thread::sleep(Duration::from_millis(40));
    }
    let ruler = h.state().ruler_spot_for_review(6.7);
    let before = h.state().cursor_seconds();
    click_at(&mut h, ruler);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::End);
    h.step();
    assert_eq!(
        h.state().cursor_seconds(),
        before,
        "neither the ruler nor the keyboard moved the cursor"
    );
    let lowest = h
        .get_all_by_label("Cancel")
        .map(|n| n.rect())
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("the status bar's Cancel");
    click_at(&mut h, lowest.center());
    h.step();
    assert!(
        h.query_by_label("Yes").is_none(),
        "the status bar's Cancel asked nothing"
    );
    let cancel = window_cancel(&h, "Cancel");
    click_at(&mut h, cancel.center());
    h.step();
    h.get_by_label("Yes").click();
    for _ in 0..100 {
        h.step();
        std::thread::sleep(Duration::from_millis(50));
        if h.query_by_label("Estimated Properties").is_none() {
            break;
        }
    }
    assert!(
        h.query_by_label("Estimated Properties").is_none(),
        "the window closed"
    );
    click_at(&mut h, ruler);
    h.step();
    assert!(
        (h.state().cursor_seconds() - before).abs() > 0.5,
        "the ruler takes clicks again"
    );
    assert!(!file.exists(), "a canceled render leaves no file");
}

/// A folder that takes no files: Windows' Controlled folder access
/// explained when Render is clicked, and a render that failed.
#[test]
#[ignore = "renders images for visual review"]
fn captura_render_pasta_bloqueada() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    snap(&mut h, "render_pasta_bloqueada");
    assert!(h.query_by_label("Open Windows Security").is_some());
    h.get_by_label("OK").click();
    settle(&mut h);
    assert!(h.query_by_label("Open Windows Security").is_none());

    // A render into a "folder" that is a file fails, and says why.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated");
    std::fs::create_dir_all(&dir).expect("output folder");
    let not_a_folder = dir.join("nao_e_pasta.txt");
    std::fs::write(&not_a_folder, b"x").expect("file");
    h.state_mut()
        .render_for_review(not_a_folder.join("falha.mp4"), 2.0, (640, 360));
    for _ in 0..200 {
        h.step();
        std::thread::sleep(Duration::from_millis(20));
        if h.query_by_label("Close").is_some() {
            break;
        }
    }
    settle(&mut h);
    snap(&mut h, "render_falhou");
    h.get_by_label("Close").click();
    settle(&mut h);
}

/// File › Import › Image Sequence: the first of 24 numbered pictures; OK
/// puts one event of 24 frames on the timeline.
#[test]
#[ignore = "renders images for visual review"]
fn captura_sequencia_de_imagens() {
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
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    snap(&mut h, "sequencia_de_imagens");
    assert!(h.query_by_label("shot_0001.png").is_some());
    let before = h.state().media_for_review().len();
    h.get_by_label("OK").click();
    settle(&mut h);
    let media = h.state().media_for_review();
    assert_eq!(media.len(), before + 1);
    assert_eq!(
        media.last().cloned(),
        Some(("shot_0001–0024.png".to_string(), Some(24)))
    );
    snap(&mut h, "sequencia_importada");
}

#[test]
#[ignore = "renders images for visual review"]
fn captura_automacao_e_dispositivos() {
    // A track's automation controls (in Touch, its volume envelope as
    // recorded), View › Window › Device Explorer, and the time display
    // listening for MIDI timecode.
    let mut app = new_app();
    app.load_demo();
    app.automation_for_review();
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
    settle(&mut h);
    snap(&mut h, "automacao_dispositivos");
}

#[test]
#[ignore = "renders images for visual review"]
fn captura_miniaturas() {
    // Options › Preferences › Video: which frames video events show
    // (Head; Head, Tail; Head, Center, Tail; All) and the box with each
    // one's source frame number, time or timecode.
    let mut last = None;
    for (thumbnails, numbers, name) in [
        (1, 1, "miniaturas_inicio_quadros"),
        (2, 2, "miniaturas_inicio_fim_tempo"),
        (3, 3, "miniaturas_inicio_meio_fim_timecode"),
        (4, 3, "miniaturas_todas_timecode"),
    ] {
        let mut app = new_app();
        app.load_demo();
        app.thumbnails_for_review(thumbnails, numbers);
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
        settle(&mut h);
        h.key_press(egui::Key::ArrowUp);
        snap(&mut h, name);
        last = Some(h);
    }
    // The Video Preview's overlays as the same page sets them: a grid of
    // 10 × 10 divisions, and the 10% action and 20% title safe areas.
    let mut h = last.expect("captured");
    // Overlays on (the grid is on by default), then the safe areas from
    // the button's menu (its arrow, right of it).
    let overlays = h.get_by_label("Overlays").rect();
    h.get_by_label("Overlays").click();
    settle(&mut h);
    click_at(
        &mut h,
        egui::pos2(overlays.right() + 6.0, overlays.center().y),
    );
    settle(&mut h);
    // A menu item's label ends in its keys' column.
    h.get_by_label_contains("Safe Areas").click();
    snap(&mut h, "previa_grade_e_areas_seguras");
}

/// Clicks the Render window's tree row `label` (other widgets may have the
/// same label: the row is the one in the tree's column, below "Use Case").
fn tree_row(h: &mut Harness<'_, BananaApp>, label: &str) {
    let use_case = h.get_by_label("Use Case").rect();
    let row = h
        .get_all_by_label(label)
        .map(|n| n.rect())
        .find(|r| r.top() > use_case.top() && (r.left() - use_case.left()).abs() < 2.0)
        .expect("tree row");
    click_at(h, row.center());
    settle(h);
}

/// The test OFX plug-ins, as if a folder had them (they are built into the
/// tests; BananaV finds the others in the OFX folders).
fn ofx_test_plugins() {
    static ADDED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ADDED.get_or_init(|| {
        let binary = bv_ofx::Binary::built_in(
            "bv-ofx-testplug",
            bv_ofx_testplug::OfxGetNumberOfPlugins,
            bv_ofx_testplug::OfxGetPlugin,
            None,
        );
        bv_ofx::registry()
            .add_built_in(binary)
            .expect("the test plug-ins");
    });
}

/// The value of a parameter of the selected event's OFX effect.
fn ofx_value(h: &Harness<'_, BananaApp>, name: &str) -> Option<bv_timeline::PluginValue> {
    let clip = h.state().clip_for_review()?;
    clip.effects
        .iter()
        .find_map(|e| e.plugin.as_ref()?.params.get(name).map(|p| p.value.clone()))
}

/// Hovers preset `label` of a chooser window until it plays, and clicks
/// its + (adds it).
fn add_preset(h: &mut Harness<'_, BananaApp>, label: &str) {
    let cell = h.get_by_label(label).rect();
    h.event(egui::Event::PointerMoved(cell.center()));
    settle(h);
    click_at(h, egui::pos2(cell.right() - 12.0, cell.top() + 12.0));
    settle(h);
}

/// The value of a parameter of the selected event's first OFX effect.
fn crop_value(h: &Harness<'_, BananaApp>, name: &str) -> Vec<f64> {
    match ofx_value(h, name) {
        Some(bv_timeline::PluginValue::Num(v)) => v,
        other => panic!("{name}: {other:?}"),
    }
}

/// BananaV's own Crop, an OFX plug-in built into BananaV, as VEGAS's: in
/// Video FX's Utility category; added to the selected event by its
/// preset's +, its settings in the FX window as VEGAS draws them (the
/// position's box open); its handles on the preview (the left edge dragged
/// in, the window moved, turned); its parameters animated, with the
/// keyframe controller under them in both its views.
#[test]
#[ignore = "renders images for visual review"]
fn captura_crop() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Utility").click();
    settle(&mut h);
    h.get_all_by_label("Crop").next().expect("Crop").click();
    settle(&mut h);
    snap(&mut h, "crop_video_fx");
    // Its preset's + adds it to the selected event and shows it in the
    // event's FX window.
    add_preset(&mut h, "(Default)");
    snap(&mut h, "crop_fx_janela");
    // Stretched by hand, the window keeps the user's size: what its page
    // takes does not depend on the window's own size (an OS window gets a
    // new size frames later: a measure that did made it fit itself again
    // and again).
    let fitted = h
        .state()
        .fx_page_size_for_review()
        .expect("the page's size");
    let ctx = h.ctx.clone();
    h.state_mut()
        .fx_window_size_for_review(&ctx, fitted + egui::vec2(160.0, 200.0));
    settle(&mut h);
    assert_eq!(h.state().fx_page_size_for_review(), Some(fitted));
    snap(&mut h, "crop_janela_esticada");

    for label in [
        "Anchor Point:",
        "X Crop:",
        "Y Crop:",
        "Location:",
        "Angle:",
        "Opacity:",
        "About",
        "?",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    // The position's box, opened by the triangle before "Location:".
    let location = h.get_by_label("Location:").rect();
    click_at(
        &mut h,
        egui::pos2(location.left() - 9.0, location.center().y),
    );
    settle(&mut h);
    snap(&mut h, "crop_posicao");
    // On the preview (its left part is under the FX window): the right
    // edge's handle dragged in a quarter of the frame makes the left edge
    // the anchor and keeps three quarters of the width.
    let f = h.state().preview_frame_for_review();
    assert!(f.width() > 10.0, "{f:?}");
    let y = f.center().y;
    drag(
        &mut h,
        egui::pos2(f.right() - 1.0, y),
        egui::pos2(f.right() - f.width() * 0.25, y),
        Some("crop_alca_arrastada"),
    );
    let x = crop_value(&h, "xCrop")[0];
    assert!((x - 0.75).abs() < 0.02, "X Crop {x}");
    assert_eq!(crop_value(&h, "anchor"), [1.0], "anchored left");
    // Dragged inside, the window moves (right and up).
    let before = crop_value(&h, "location");
    drag(
        &mut h,
        f.center() + egui::vec2(60.0, 40.0),
        f.center() + egui::vec2(100.0, 20.0),
        None,
    );
    let after = crop_value(&h, "location");
    assert!(
        after[0] > before[0] && after[1] > before[1],
        "{before:?} → {after:?}"
    );
    snap(&mut h, "crop_janela_movida");
    // A double click in the position's box puts it back in the middle.
    let text_input = |n: &kittest::AccessKitNode<'_>| n.role() == egui::accesskit::Role::TextInput;
    let pad = h
        .query_all_by_label("Location")
        .find(|n| !text_input(&n.accesskit_node()) && n.rect().width() > 50.0)
        .expect("the position's box")
        .rect();
    let at = pad.center() + egui::vec2(20.0, -15.0);
    h.event(egui::Event::PointerMoved(at));
    h.step();
    // Both clicks in one frame (queued events would take a frame, a
    // quarter of a second, each: too slow for a double click).
    for _ in 0..2 {
        for pressed in [true, false] {
            h.input_mut().events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
    h.step();
    settle(&mut h);
    assert!(
        h.query_all_by_value("0.500, 0.500").next().is_some(),
        "the window back in the middle: {:?} (box {pad:?})",
        crop_value(&h, "location")
    );
    // X Crop never goes past 1, as VEGAS's.
    h.query_all_by_label("X Crop")
        .find(|n| text_input(&n.accesskit_node()))
        .expect("X Crop's number")
        .focus();
    settle(&mut h);
    press(&mut h, egui::Key::A, egui::Modifiers::COMMAND);
    h.event(egui::Event::Text("1.5".into()));
    settle(&mut h);
    press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(crop_value(&h, "xCrop"), [1.0]);
    snap(&mut h, "crop_recentrado");
    // The window fits the page, as VEGAS's.
    let page = h
        .state()
        .fx_page_size_for_review()
        .expect("the page's size");
    // X Crop animated (the first clock: Anchor Point has none): a keyframe
    // at the event's start; in its middle a new value typed makes a second
    // one. The keyframe controller opens under the settings: the window
    // grows to take it.
    h.get_all_by_label("Animate")
        .next()
        .expect("X Crop's clock")
        .click();
    settle(&mut h);
    let grown = h
        .state()
        .fx_page_size_for_review()
        .expect("the page's size");
    assert!(
        grown.y > page.y + 100.0 && grown.x >= page.x,
        "{page:?} → {grown:?}"
    );
    let event = h.state().clip_for_review().expect("the event");
    let middle = event.start + bv_core::Tick(event.duration.0 / 2);
    h.state_mut().seek_seconds(middle.as_seconds_f64());
    settle(&mut h);
    h.query_all_by_label("X Crop")
        .find(|n| text_input(&n.accesskit_node()))
        .expect("X Crop's number")
        .focus();
    settle(&mut h);
    press(&mut h, egui::Key::A, egui::Modifiers::COMMAND);
    h.event(egui::Event::Text("0.4".into()));
    settle(&mut h);
    press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    let keys = h
        .state()
        .clip_for_review()
        .and_then(|c| {
            let fx = c.effects.iter().find_map(|e| e.plugin.clone())?;
            Some(fx.params.get("xCrop")?.keys.len())
        })
        .unwrap_or(0);
    assert_eq!(keys, 2, "two keyframes of X Crop");
    snap(&mut h, "crop_quadros_chave");
    keyframe_controller(&mut h);
}

/// The Crop's presets, VEGAS's: in Video FX, Crop Left Half added (the
/// right half kept, its middle at 0.75 as VEGAS gives it) and Crop Top
/// Half picked from the FX window's list.
#[test]
#[ignore = "renders images for visual review"]
fn captura_crop_predefinicoes() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Utility").click();
    settle(&mut h);
    h.get_all_by_label("Crop").next().expect("Crop").click();
    settle(&mut h);
    let presets = [
        "Crop Left Half, No Angle, Opaque",
        "Crop Right Half, No Angle, Opaque",
        "Crop Top Half, No Angle, Opaque",
        "Crop Bottom Half, No Angle, Opaque",
    ];
    for preset in presets {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "crop_predefinicoes");
    add_preset(&mut h, presets[0]);
    assert_eq!(crop_value(&h, "anchor"), [2.0]);
    assert_eq!(crop_value(&h, "xCrop"), [0.5]);
    assert_eq!(crop_value(&h, "location"), [0.75, 0.5]);
    assert!(
        h.query_all_by_value("0.750, 0.500").next().is_some(),
        "Location as VEGAS shows it"
    );
    snap(&mut h, "crop_metade_esquerda");
    pick_in_list(&mut h, presets[0], presets[2]);
    assert_eq!(crop_value(&h, "anchor"), [3.0]);
    assert_eq!(crop_value(&h, "location"), [0.5, 0.25]);
    snap(&mut h, "crop_metade_de_cima");
}

/// BananaV's own LUT Filter, as VEGAS's: in Video FX's Color with
/// BananaV's looks as presets, its window (Name, File with its Browse…,
/// Interpolation, Strength), a table's path typed in naming it after its
/// title, Tetrahedral picked.
#[test]
#[ignore = "renders images for visual review"]
fn captura_lut_filter() {
    let dir = std::env::temp_dir().join(format!("bananav-telas-lut-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // A warm look: red up, blue down.
    let mut cube = String::from("TITLE \"Warm Look\"\nLUT_3D_SIZE 5\n");
    for b in 0..5 {
        for g in 0..5 {
            for r in 0..5 {
                let [r, g, b] = [r, g, b].map(|v| v as f32 / 4.0);
                cube += &format!("{} {} {}\n", (r * 1.15).min(1.0), g, b * 0.8);
            }
        }
    }
    let file = dir.join("warm.cube");
    std::fs::write(&file, cube).unwrap();

    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Color").click();
    settle(&mut h);
    h.get_all_by_label("LUT Filter")
        .next()
        .expect("LUT Filter")
        .click();
    settle(&mut h);
    // VEGAS's has none: BananaV's looks.
    for preset in [
        "Warm",
        "Cool",
        "Teal and Orange",
        "Faded Film",
        "High Contrast",
        "Black and White",
        "Bleach Bypass",
        "Vintage",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "lut_video_fx");
    add_preset(&mut h, "(Default)");
    for label in ["Name:", "File:", "Browse…", "Interpolation:", "Strength:"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "lut_janela");
    // The table's path typed into File: its title names it.
    let text_input = |n: &kittest::AccessKitNode<'_>| n.role() == egui::accesskit::Role::TextInput;
    h.query_all_by_label("File")
        .find(|n| text_input(&n.accesskit_node()))
        .expect("File's text")
        .focus();
    settle(&mut h);
    h.event(egui::Event::Text(file.display().to_string()));
    settle(&mut h);
    press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    let name = match ofx_value(&h, "name") {
        Some(bv_timeline::PluginValue::Text(t)) => t,
        other => panic!("name: {other:?}"),
    };
    assert_eq!(name, "Warm Look");
    pick_in_list(&mut h, "Linear (good)", "Tetrahedral (best)");
    assert_eq!(corrector_value(&h, "interpolation"), [1.0]);
    snap(&mut h, "lut_aplicado");
    let _ = std::fs::remove_dir_all(&dir);
}

/// BananaV's own White Balance, as VEGAS's: in Video FX's Color with
/// BananaV's lights as presets (VEGAS's has none), its window (the neutral
/// color in Lab, 5999 K and 10, Equalize Histogram off), a light to take
/// away from the presets' list at the amount that takes it to gray.
#[test]
#[ignore = "renders images for visual review"]
fn captura_white_balance() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Color").click();
    settle(&mut h);
    h.get_all_by_label("White Balance")
        .next()
        .expect("White Balance")
        .click();
    settle(&mut h);
    for preset in [
        "Tungsten Light Corrected",
        "Fluorescent Light Corrected",
        "Shade Corrected",
        "Warm Light",
        "Golden Hour",
        "Cool Light",
        "Moonlight",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "wb_video_fx");
    add_preset(&mut h, "(Default)");
    for label in [
        "Select neutral color:",
        "Temperature:",
        "Tint:",
        "Amount of correction:",
        "Brightness:",
        "Equalize Histogram:",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    // VEGAS's numbers: mid gray in Lab, 5999 K and 10.
    for value in ["53.39, 0.0, 0.0", "5999", "10"] {
        assert!(h.query_all_by_value(value).next().is_some(), "no {value}");
    }
    snap(&mut h, "wb_janela");
    pick_in_list(&mut h, "(Default)", "Tungsten Light Corrected");
    // The amount that takes the light's color to gray.
    assert!((corrector_value(&h, "amount")[0] - 0.539).abs() < 0.001);
    assert!((corrector_value(&h, "temperature")[0] - 3200.0).abs() < 1.0);
    snap(&mut h, "wb_tungstenio");
}

/// BananaV's own HSL Adjust, as VEGAS's: in Video FX's Color with VEGAS's
/// presets, its window (Add to hue, Saturation, Luminance) and a preset
/// from the list.
#[test]
#[ignore = "renders images for visual review"]
fn captura_hsl_adjust() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Color").click();
    settle(&mut h);
    h.get_all_by_label("HSL Adjust")
        .next()
        .expect("HSL Adjust")
        .click();
    settle(&mut h);
    for preset in [
        "Desaturate",
        "Invert Color",
        "Max Saturation",
        "Half Saturation",
        "Saturation and Luminance Boost",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "hsl_video_fx");
    add_preset(&mut h, "(Default)");
    for label in ["Add to hue:", "Saturation:", "Luminance:"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "hsl_janela");
    pick_in_list(&mut h, "(Default)", "Invert Color");
    assert_eq!(corrector_value(&h, "hue"), [0.5]);
    assert_eq!(corrector_value(&h, "saturation"), [1.0]);
    snap(&mut h, "hsl_inverter");
}

/// BananaV's own Picture In Picture, as VEGAS's: in Video FX's Utility
/// with VEGAS's presets, its window (Location, Angle, the scales, Mode,
/// Shear, Opacity, the corner points) and a preset putting the picture in
/// the upper left, its handles on the preview.
#[test]
#[ignore = "renders images for visual review"]
fn captura_picture_in_picture() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Utility").click();
    settle(&mut h);
    // Below the list's visible part (the category has grown): picked
    // through the accessibility tree, which the pointer cannot reach.
    h.get_all_by_label("Picture In Picture")
        .next()
        .expect("Picture In Picture")
        .click_accesskit();
    settle(&mut h);
    for preset in [
        "Upper Left, Scaled, No Angle, Opaque",
        "Upper Right, Scaled, No Angle, Opaque",
        "Lower Left, Scaled, No Angle, Opaque",
        "Lower Right, Scaled, No Angle, Opaque",
        "Middle Left, Scaled, Angled, Translucent",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "pip_video_fx");
    add_preset(&mut h, "(Default)");
    for label in [
        "Location:",
        "Angle:",
        "Scale in X:",
        "Scale in Y:",
        "Mode:",
        "Shear:",
        "Opacity:",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "pip_janela");
    pick_in_list(&mut h, "(Default)", "Upper Left, Scaled, No Angle, Opaque");
    assert_eq!(corrector_value(&h, "location"), [0.25, 0.75]);
    assert_eq!(corrector_value(&h, "scaleX"), [0.333]);
    snap(&mut h, "pip_superior_esquerdo");
    // VEGAS's fifth: turned, bigger, see-through; its corners where they
    // start, as VEGAS shows them.
    pick_in_list(
        &mut h,
        "Upper Left, Scaled, No Angle, Opaque",
        "Middle Left, Scaled, Angled, Translucent",
    );
    assert_eq!(corrector_value(&h, "location"), [0.25, 0.6]);
    assert_eq!(corrector_value(&h, "angle"), [-35.0]);
    assert_eq!(corrector_value(&h, "scaleX"), [0.433]);
    assert_eq!(corrector_value(&h, "opacity"), [0.75]);
    assert_eq!(corrector_value(&h, "topLeft"), [0.25, 0.75]);
    snap(&mut h, "pip_meio_esquerda");
    // Dragged by its middle past the frame's bottom edge, into the
    // preview's dark margin: the picture's controls go on there, and are
    // still grabbed there.
    let frame = h.state().preview_frame_for_review();
    let area = h.state().preview_area_for_review();
    let room = area.bottom() - frame.bottom();
    assert!(
        room > 10.0,
        "a margin under the frame: {frame:?} in {area:?}"
    );
    let middle = egui::pos2(
        frame.left() + 0.25 * frame.width(),
        frame.top() + 0.4 * frame.height(),
    );
    let outside = egui::pos2(middle.x, frame.bottom() + room / 2.0);
    drag(&mut h, middle, outside, None);
    let moved = corrector_value(&h, "location");
    assert!(moved[1] < 0.0, "past the bottom edge: {moved:?}");
    snap(&mut h, "pip_fora_do_quadro");
    let back = egui::pos2(middle.x, frame.top() + 0.6 * frame.height());
    drag(&mut h, outside, back, None);
    let again = corrector_value(&h, "location");
    assert!(again[1] > moved[1], "grabbed outside: {again:?}");
}

/// Mesh Warp's mesh (its text; empty: the grid at rest).
fn mesh_text(h: &Harness<'_, BananaApp>) -> String {
    match ofx_value(h, "mesh") {
        Some(bv_timeline::PluginValue::Text(t)) => t,
        None => String::new(),
        other => panic!("mesh: {other:?}"),
    }
}

/// Mesh Warp, as VEGAS's: in Video FX's Creative; its window (the grid's
/// sizes, the Mesh Behavior group's Elasticity and Fixed Border Points,
/// Reset Mesh) and its grid on the preview; a point dragged, the picture
/// following, and put back with a double click.
#[test]
#[ignore = "renders images for visual review"]
fn captura_mesh_warp() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Creative").click();
    settle(&mut h);
    h.get_all_by_label("Mesh Warp")
        .next()
        .expect("Mesh Warp")
        .click();
    settle(&mut h);
    snap(&mut h, "mesh_warp_video_fx");
    add_preset(&mut h, "(Default)");
    for label in [
        "Grid size in X:",
        "Grid size in Y:",
        "Lock Grid size in X and Y:",
        "Mesh Behavior",
        "Elasticity:",
        "Fixed Border Points:",
        "Reset Mesh",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "mesh_warp_janela");
    // Past the crossfade, the gradient alone.
    h.state_mut().seek_seconds(7.0);
    settle(&mut h);
    // A point of the grid (three quarters across, half way up) dragged on
    // the preview: the picture follows it.
    let f = h.state().preview_frame_for_review();
    assert!(f.width() > 10.0, "{f:?}");
    let from = egui::pos2(f.left() + f.width() * 0.75, f.center().y);
    let to = from + egui::vec2(f.width() * 0.08, -f.height() * 0.1);
    drag(&mut h, from, to, Some("mesh_warp_ponto_arrastado"));
    let moved = mesh_text(&h);
    assert!(moved.starts_with("8 8 "), "{moved}");
    snap(&mut h, "mesh_warp_ponto_solto");
    // A double click on it puts it back (two presses in two frames, as the
    // preview tells the overlay of each).
    click_at(&mut h, to);
    click_at(&mut h, to);
    settle(&mut h);
    // Every point where it was at rest.
    let back: Vec<f32> = mesh_text(&h)
        .split_ascii_whitespace()
        .skip(2)
        .map(|v| v.parse().unwrap())
        .collect();
    let rest = (0..81).flat_map(|n| [(n % 9) as f32 / 8.0, (n / 9) as f32 / 8.0]);
    assert!(
        back.iter().zip(rest).all(|(a, b)| (a - b).abs() < 1e-6),
        "put back: {back:?}"
    );
}

/// Lens Correction, as VEGAS's: its window (Lens Model's Search for and
/// Found Lenses, Distortion Center, k1 to k3, Scale, Show Distortion); a
/// search lists the lenses whose names have its words, a lens picked
/// gives k1, k2 and k3 its coefficients and Distortion Center (0.5, 0),
/// and Show Distortion leaves the picture as it is under its grid.
#[test]
#[ignore = "renders images for visual review"]
fn captura_lens_correction() {
    use egui::accesskit::Role;
    let mut h = video_fx_harness("Utility", "Lens Correction");
    add_preset(&mut h, "(Default)");
    // Past the crossfade, the gradient alone.
    h.state_mut().seek_seconds(7.0);
    settle(&mut h);
    snap(&mut h, "lens_correction_janela");
    let search = h.get_by_role_and_label(Role::TextInput, "Search for");
    search.click();
    search.type_text("Nikkor 35mm");
    h.key_press(egui::Key::Enter);
    settle(&mut h);
    // Found Lenses: the list on its row.
    let on_row = |h: &Harness<'_, BananaApp>, label: &str, role: Role| {
        let row = h.get_by_label(label).rect().center().y;
        h.get_all_by_role(role)
            .map(|n| n.rect().center())
            .min_by(|a, b| (a.y - row).abs().total_cmp(&(b.y - row).abs()))
            .expect(label)
    };
    let list = on_row(&h, "Found Lenses:", Role::ComboBox);
    click_at(&mut h, list);
    settle(&mut h);
    snap(&mut h, "lens_correction_lentes_encontradas");
    for lens in [
        "Nikon AF Nikkor 35mm f/2.0D",
        "Nikon AF-S DX Nikkor 35mm f/1.8G",
        "Nikon AI-S Nikkor 35mm f/2.0",
    ] {
        assert!(h.query_all_by_label(lens).next().is_some(), "no {lens}");
    }
    h.get_by_label("Nikon AI-S Nikkor 35mm f/2.0").click();
    settle(&mut h);
    let num = |h: &Harness<'_, BananaApp>, name: &str| match ofx_value(h, name) {
        Some(bv_timeline::PluginValue::Num(v)) => v,
        v => panic!("{name}: {v:?}"),
    };
    assert_eq!(num(&h, "k1"), [0.01447]);
    assert_eq!(num(&h, "k2"), [-0.03949]);
    assert_eq!(num(&h, "k3"), [0.0153]);
    assert_eq!(num(&h, "distortionCenter"), [0.5, 0.0]);
    snap(&mut h, "lens_correction_lente_escolhida");
    // Show Distortion: the box on its row.
    let at = on_row(&h, "Show Distortion:", Role::CheckBox);
    click_at(&mut h, at);
    settle(&mut h);
    assert_eq!(num(&h, "showDistortion"), [1.0]);
    snap(&mut h, "lens_correction_grade");
}

/// Bézier Masking, as VEGAS's Help describes it: in Video FX's Utility with
/// its presets; its window (Mask 1's settings, the tracker's greyed out,
/// the General Options) and the mask on the preview with its handles —
/// white corners, yellow sides, the ring —; a corner dragged scales it.
#[test]
#[ignore = "renders images for visual review"]
fn captura_bezier_masking() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Utility").click();
    settle(&mut h);
    h.get_all_by_label("Bézier Masking")
        .next()
        .expect("Bézier Masking")
        .click();
    settle(&mut h);
    for preset in ["Oval", "Diamond", "Soft Oval", "Inverted Oval"] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "bezier_masking_video_fx");
    add_preset(&mut h, "(Default)");
    for label in ["Mask 1", "General Options", "Invert:", "Mask FX:", "Blend:"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    // Past the crossfade, the gradient alone.
    h.state_mut().seek_seconds(7.0);
    settle(&mut h);
    snap(&mut h, "bezier_masking_janela");
    // The lower right corner (three quarters across, a quarter up) dragged
    // out: the mask grows, its aspect kept.
    let f = h.state().preview_frame_for_review();
    assert!(f.width() > 10.0, "{f:?}");
    let from = egui::pos2(f.left() + f.width() * 0.75, f.top() + f.height() * 0.75);
    let to = from + egui::vec2(f.width() * 0.1, f.height() * 0.1);
    drag(&mut h, from, to, Some("bezier_masking_canto_arrastado"));
    let width = match ofx_value(&h, "width1") {
        Some(bv_timeline::PluginValue::Num(v)) => v[0],
        other => panic!("width1: {other:?}"),
    };
    assert!(width > 0.55, "{width}");
    snap(&mut h, "bezier_masking_canto_solto");
}

/// Beat Detection and Tempo Detection, as VEGAS's: a beat of 128 a
/// minute (kicks, hi-hats between them) in a file; its kicks shown in the
/// Trimmer while Sensitivity is set; then its tempo found.
#[test]
#[ignore = "renders images for visual review"]
fn captura_batidas_e_andamento() {
    let dir = std::env::temp_dir().join(format!("bananav-telas-beats-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let rate = 44_100u32;
    let period = 60.0 / 128.0;
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut noise = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    };
    let samples: Vec<f32> = (0..16 * rate as usize)
        .map(|i| {
            let t = i as f64 / rate as f64;
            let since = t.rem_euclid(period);
            let kick = 0.8 * (std::f64::consts::TAU * 60.0 * since).sin() * (-since / 0.08).exp();
            let off = (t - period / 2.0).rem_euclid(period);
            let hat = 0.12 * noise() * (-off / 0.03).exp();
            (kick + hat) as f32
        })
        .collect();
    let path = dir.join("beat 128.wav");
    let file = std::fs::File::create(&path).unwrap();
    let mut w = bv_wav::WavWriter::new(file, rate, 1, bv_wav::SampleFormat::Int(16)).unwrap();
    w.write_planar(&[samples]).unwrap();
    w.finish().unwrap();
    let mut app = new_app();
    app.load_demo();
    app.beats_for_review(&path.to_string_lossy(), false);
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
    settle(&mut h);
    assert!(h.query_all_by_label("Sensitivity:").next().is_some());
    snap(&mut h, "batidas");
    h.state_mut().close_beat_dialogs_for_review();
    h.state_mut()
        .beats_for_review(&path.to_string_lossy(), true);
    settle(&mut h);
    assert!(h.query_all_by_label("Detected tempo:").next().is_some());
    snap(&mut h, "andamento");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Auto Ducking: a controller's dialog (Floor and Peak Reduction) and, on
/// its listener track, the dotted line of what it gets.
#[test]
#[ignore = "renders images for visual review"]
fn captura_auto_ducking() {
    let mut app = new_app();
    app.load_demo();
    app.ducking_for_review();
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
    settle(&mut h);
    for label in ["Floor Reduction:", "Peak Reduction:"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "auto_ducking");
}

/// Tools › Audio › Apply Non-Real-Time Event FX: the dialog for an event
/// with a Gain and a bypassed Reverb.
#[test]
#[ignore = "renders images for visual review"]
fn captura_nao_tempo_real() {
    let mut app = new_app();
    app.load_demo();
    app.nonrealtime_for_review();
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
    settle(&mut h);
    for label in ["Effects:", "File:", "Format:", "Render"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "nao_tempo_real");
}

/// Builds BananaV's VST 3 test plug-ins and puts them in a folder of
/// their own as a bundle, as a vendor ships one; the bundle's path.
fn vst_test_bundle() -> PathBuf {
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
    let built = target.join("debug").join(if cfg!(windows) {
        "bv_vst3_testplug.dll"
    } else {
        "libbv_vst3_testplug.so"
    });
    let folder = std::env::temp_dir().join(format!("bananav-telas-vst3-{}", std::process::id()));
    let bundle = folder.join("BananaV Tests.vst3");
    std::fs::create_dir_all(&bundle).unwrap();
    let library = bv_vst3::library_of(&bundle);
    std::fs::create_dir_all(library.parent().unwrap()).unwrap();
    std::fs::copy(&built, &library).unwrap();
    bundle
}

/// The selected event's VST 3 effect.
fn vst_fx(h: &Harness<'_, BananaApp>) -> bv_timeline::VstFx {
    let clip = h.state().clip_for_review().expect("the event");
    clip.effects
        .iter()
        .find_map(|e| e.vst.as_deref().cloned())
        .expect("a VST 3 effect")
}

/// VST 3 plug-ins: the Plug-In Chooser's VST3 folder (the plug-ins found
/// in Preferences' folders), one added to an audio event, and its page in
/// the FX window — BananaV's generic editor: the plug-in's parameters as
/// rows with its own texts (its hidden meter not), a switch changed going
/// back into the effect's values and state.
#[test]
#[ignore = "renders images for visual review"]
fn captura_vst3() {
    let bundle = vst_test_bundle();
    let mut app = new_app();
    app.load_demo();
    assert!(app.vst_chooser_for_review(&bundle));
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
    settle(&mut h);
    h.get_by_label("VST3").click();
    settle(&mut h);
    snap(&mut h, "vst3_seletor");
    let item = h.get_by_label("BananaV Tests").rect().center();
    double_click_at(&mut h, item);
    h.get_by_label("OK").click();
    settle(&mut h);
    snap(&mut h, "vst3_editor_generico");
    for label in ["Gain", "Bypass", "0.0 dB", "Show Plug-In Window"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    assert!(h.query_all_by_label("Meter").next().is_none());
    let fx = vst_fx(&h);
    assert_eq!(fx.name, "BananaV Tests");
    // The bypass switched on: the effect's value and its state follow.
    let bypass = h.get_by_label("Bypass").rect();
    click_at(&mut h, egui::pos2(bypass.right() + 24.0, bypass.center().y));
    settle(&mut h);
    let fx = vst_fx(&h);
    assert_eq!(fx.params.get(&1), Some(&1.0));
    assert_eq!(fx.state.len(), 16);
    assert_eq!(f64::from_le_bytes(fx.state[8..].try_into().unwrap()), 1.0);
    assert!(!h.state().vst_window_open_for_review());
    snap(&mut h, "vst3_bypass");
    let _ = std::fs::remove_dir_all(bundle.parent().unwrap());
}

/// The settings of the selected event's Modern Equalizer.
fn eq_settings(h: &Harness<'_, BananaApp>) -> bv_engine::modern_eq::Settings {
    let clip = h.state().clip_for_review().expect("the event");
    let e = clip
        .effects
        .iter()
        .find(|e| e.kind == bv_timeline::EffectKind::ModernEq)
        .expect("the equalizer");
    bv_engine::modern_eq::Settings::of(e)
}

/// The Modern Equalizer, as VEGAS's Help describes it: its graph (each
/// band's area in its routing's color, the whole curve, Mono Below's green
/// line, the bands' circles, a bypassed band dashed), the bands' toolbar,
/// the picked band's controls and the global ones. A double-click on the
/// graph adds a band (a peak in the middle), a circle dragged moves its
/// band, a right-click opens Cut, Copy and Replace, a double-click on a
/// circle deletes its band, and A/B keeps two settings apart.
/// Busses and assignable FX: the Mixing Console's strips (a track's
/// output and sends, a chain's input level, a bus's effects after its
/// volume and its send) and the track header's bus button and
/// multipurpose slider.
#[test]
#[ignore = "renders images for visual review"]
fn captura_barramentos() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    for label in ["Bus A", "Drums", "Reverb"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "barramentos");
}

/// The audio bus tracks (View › Audio Bus Tracks): a track for each bus
/// and the chain below the splitter, bus A's volume and pan envelopes, the
/// chain's volume, and the audio track's send envelope to Drums; then bus
/// A's and the audio track's controls as automation controls (Automation
/// Settings) with a mute envelope on bus A.
#[test]
#[ignore = "renders images for visual review"]
fn captura_trilhas_de_barramento() {
    let mut app = new_app();
    app.load_demo();
    assert!(app.bus_tracks_for_review());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1300.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    settle(&mut h);
    snap(&mut h, "trilhas_de_barramento");
    assert!(h.state_mut().bus_automation_for_review());
    settle(&mut h);
    snap(&mut h, "trilhas_de_barramento_automacao");
}

/// Project Notes as VEGAS 2026's: three notes (untitled, labelled, one
/// resolved), the default color and label at the bottom.
#[test]
#[ignore = "renders images for visual review"]
fn captura_project_notes() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    snap(&mut h, "project_notes");
}

/// The track headers' buttons as the user's VEGAS 23 prints show them:
/// the More (⋯) menu with VEGAS's buttons (Mute and Solo shown, every
/// other command in it), Edit Visible Button Set opened from it (Mute and
/// Solo ticked), Show All, and the video header with every button at the
/// prints' widths — the buttons going from the left as the list narrows,
/// More then having the commands of those that went.
#[test]
#[ignore = "renders images for visual review"]
fn captura_botoes_visiveis() {
    let mut app = new_app();
    app.load_demo();
    app.track_list_width_for_review(215.0);
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
    settle(&mut h);
    let count = |h: &Harness<'_, BananaApp>, label: &str| h.query_all_by_label(label).count();
    // More, as VEGAS's: the commands of the buttons not shown.
    h.get_all_by_label("More").next().expect("More").click();
    settle(&mut h);
    assert_eq!(count(&h, "Track FX"), 1, "More lists Track FX");
    assert_eq!(count(&h, "Bypass Motion Blur"), 1);
    assert_eq!(count(&h, "Mute"), 0, "Mute is shown, not in More");
    snap(&mut h, "botoes_menu_mais");
    h.get_by_label("Edit Visible Button Set…").click();
    settle(&mut h);
    snap(&mut h, "botoes_visiveis_dialogo");
    h.get_by_label("Show All").click();
    settle(&mut h);
    h.get_by_label("OK").click();
    settle(&mut h);
    for width in [1098.0, 215.0, 123.0] {
        h.state_mut().track_list_width_for_review(width);
        snap(&mut h, &format!("botoes_todos_{}", width as u32));
    }
    // At 215 pixels Maximize and Minimize went: More has them, and not the
    // buttons the header shows.
    h.state_mut().track_list_width_for_review(215.0);
    settle(&mut h);
    let fx = count(&h, "Track FX");
    h.get_all_by_label("More").next().expect("More").click();
    settle(&mut h);
    assert_eq!(count(&h, "Minimize"), 1, "More lists Minimize");
    assert_eq!(count(&h, "Maximize"), 1, "More lists Maximize");
    assert_eq!(count(&h, "Track FX"), fx, "the header shows Track FX");
    snap(&mut h, "botoes_menu_mais_estreito");
}

/// Fades and crossfades as the user's VEGAS prints show them: an audio
/// event's waveform thinning along its fade-out, the sounds of a crossfade
/// merged, dark lines where a crossfade begins and ends, the earlier event's
/// outline over the later one while it is the one picked, VEGAS's blue
/// handles; then the pointer over a fade's handle.
#[test]
#[ignore = "renders images for visual review"]
fn captura_fades_como_o_vegas() {
    let mut app = new_app();
    app.load_demo();
    // The second audio event: a fade-out of 3 s.
    app.fades_for_review(1, 1, 0.0, 3.0, 1.0);
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
    settle(&mut h);
    // The first video event picked: its outline over the crossfade.
    let spot = h
        .state()
        .timeline_spot_for_review(0, 1.0)
        .expect("the event");
    click_at(&mut h, spot);
    settle(&mut h);
    snap(&mut h, "fades_vegas");
    let handle = h
        .state()
        .fade_handle_spot_for_review(1, 1, true)
        .expect("the fade-out's handle");
    h.event(egui::Event::PointerMoved(handle));
    settle(&mut h);
    snap(&mut h, "fades_ponteiro");
}

/// The adjustment track as the user's VEGAS prints show it: its
/// Composite envelope's point menu (Set to 100% and 0% Composite Strength,
/// Set To…, Delete, the fades, Flip, Thin, Reset All) and Set To…'s box at
/// the point, its value picked, a value typed there; a click on the line
/// adds no point, a drag takes it down, a double-click adds one; a video
/// effect let go on the track becomes its own.
#[test]
#[ignore = "renders images for visual review"]
fn captura_trilha_de_ajuste() {
    let mut app = new_app();
    app.load_demo();
    app.adjustment_envelope_for_review();
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
    settle(&mut h);
    let point = h
        .state()
        .envelope_spot_for_review(0.0, Some(2))
        .expect("the point");
    // VEGAS's items, each one more than before the menu opened (the
    // toolbar has a Delete too).
    let labels = [
        "Set to 100% Composite Strength",
        "Set to 0% Composite Strength",
        "Set To…",
        "Delete",
        "Linear Fade",
        "Fast Fade",
        "Slow Fade",
        "Smooth Fade",
        "Sharp Fade",
        "Hold",
        "Flip All Points",
        "Thin All Points",
        "Reset All",
    ];
    let count = |h: &Harness<'_, BananaApp>, label: &str| h.query_all_by_label(label).count();
    let before: Vec<usize> = labels.iter().map(|l| count(&h, l)).collect();
    right_click_at(&mut h, point);
    settle(&mut h);
    for (label, n) in labels.iter().zip(before) {
        assert_eq!(count(&h, label), n + 1, "the menu's {label}");
    }
    snap(&mut h, "ajuste_menu_do_ponto");
    h.get_by_label("Set To…").click();
    settle(&mut h);
    snap(&mut h, "ajuste_set_to");
    h.event(egui::Event::Text("50".into()));
    h.step();
    press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    settle(&mut h);
    let values = h.state().envelope_values_for_review();
    assert_eq!(values.len(), 3);
    assert!((values[2] - 0.5).abs() < 1e-3, "typed: {values:?}");
    // A click on the line adds nothing; a drag takes it down.
    let line = h
        .state()
        .envelope_spot_for_review(8.0, None)
        .expect("the line");
    click_at(&mut h, line);
    settle(&mut h);
    assert_eq!(
        h.state().envelope_values_for_review().len(),
        3,
        "a click adds no point"
    );
    drag(&mut h, line, line + egui::vec2(0.0, 12.0), None);
    let values = h.state().envelope_values_for_review();
    assert_eq!(values.len(), 3, "a drag adds no point");
    assert!(values[2] < 0.45, "the line went down: {values:?}");
    snap(&mut h, "ajuste_linha_arrastada");
    let line = h
        .state()
        .envelope_spot_for_review(8.0, None)
        .expect("the line");
    double_click_at(&mut h, line);
    assert_eq!(
        h.state().envelope_values_for_review().len(),
        4,
        "a double-click adds one"
    );
    // A video effect let go on the track: its own, its window shown.
    let lane = h
        .state()
        .envelope_spot_for_review(10.0, None)
        .expect("the lane")
        + egui::vec2(0.0, 8.0);
    h.state_mut().effect_drag_for_review();
    h.event(egui::Event::PointerMoved(lane));
    h.step();
    h.event(egui::Event::PointerButton {
        pos: lane,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    settle(&mut h);
    assert_eq!(h.state().first_track_effects_for_review(), (1, true));
    snap(&mut h, "ajuste_efeito_solto");
}

/// V pressed on the audio tracks picked, as the user's VEGAS print shows:
/// each one's volume envelope, 0 dB in the middle of the track, the line
/// VEGAS's purple edged darker, a point a diamond.
#[test]
#[ignore = "renders images for visual review"]
fn captura_envelopes_de_volume() {
    let mut app = new_app();
    app.load_demo();
    app.welcome_for_review("closed");
    app.volume_envelopes_for_review();
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
    settle(&mut h);
    snap(&mut h, "envelopes_de_volume");
}

/// An envelope's point under the pointer as the user's VEGAS print shows
/// it: a white diamond, the tooltip "Composite Level at <time> is <value>";
/// a video event on the adjustment track (the checkerboard at half its
/// level), the track's Invert working on it and on the tracks below.
#[test]
#[ignore = "renders images for visual review"]
fn captura_ponto_do_envelope_e_video_na_trilha_de_ajuste() {
    let mut app = new_app();
    app.load_demo();
    app.welcome_for_review("closed");
    app.adjustment_envelope_for_review();
    app.adjustment_video_for_review();
    app.seek_seconds(1.0);
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
    settle(&mut h);
    let point = h
        .state()
        .envelope_spot_for_review(0.0, Some(2))
        .expect("the point");
    // A little up and to the right: the harness's arrow off the diamond.
    h.event(egui::Event::PointerMoved(point + egui::vec2(4.0, -4.0)));
    settle(&mut h);
    snap(&mut h, "envelope_ponto_sob_o_ponteiro");
}

/// Crossfades as VEGAS draws them: the audio's overlap darker; a
/// transition's label (3D Fly In/Out) with its Transition Properties icon,
/// whose window shows its presets and settings.
#[test]
#[ignore = "renders images for visual review"]
fn captura_transicao_na_timeline() {
    let mut app = new_app();
    app.load_demo();
    app.welcome_for_review("closed");
    assert!(app.transition_for_review(false));
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
    settle(&mut h);
    snap(&mut h, "transicao_rotulo");
    assert!(h.state_mut().transition_for_review(true));
    settle(&mut h);
    snap(&mut h, "transicao_janela");
}

/// BananaV's official logo where the banana was: the toolbar's corner and
/// the Explorer's project files, in Thumbnails and in Details.
#[test]
#[ignore = "renders images for visual review"]
fn captura_logo_oficial() {
    let dir = out_dir().join("logo_explorador");
    std::fs::create_dir_all(&dir).expect("folder");
    let project = dir.join("My Project.bvproj");
    std::fs::write(&project, b"").expect("project");
    let mut app = new_app();
    app.load_demo();
    app.explorer_for_review(&dir, &project);
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
    snap(&mut h, "logo_miniaturas");
    h.state_mut().explorer_pick_all_for_review();
    snap(&mut h, "logo_detalhes");
}

/// The whole window at 1920×1080, as the user's prints of VEGAS 23: the
/// menus in the title bar, the track list down to the window's bottom (the
/// Rate there), the transport beside it with the cursor's time and the
/// selection's length at its right, the status bar under them. And a
/// crossfade with a transition, one of its events picked: its length under
/// the transition's label, zoomed in and out.
#[test]
#[ignore = "renders images for visual review"]
fn captura_enquadramento() {
    let mut app = new_app();
    app.load_demo();
    assert!(app.transition_for_review(false));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1920.0, 1080.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    settle(&mut h);
    let at = h
        .state()
        .timeline_spot_for_review(0, 4.5)
        .expect("the crossfade");
    click_at(&mut h, at);
    snap(&mut h, "enquadramento");
    for _ in 0..4 {
        h.key_press(egui::Key::ArrowUp);
    }
    snap(&mut h, "enquadramento_crossfade_perto");
    for _ in 0..9 {
        h.key_press(egui::Key::ArrowDown);
    }
    snap(&mut h, "enquadramento_crossfade_longe");
    // The cursor out of the time selection: where it is, the end's icon
    // alone and the selection's length dimmed.
    let ruler = h.state().ruler_spot_for_review(8.0);
    click_at(&mut h, ruler);
    snap(&mut h, "enquadramento_cursor_fora");
    // The File menu opens down from the title bar.
    h.get_by_label("File").click();
    snap(&mut h, "enquadramento_menu_arquivo");
}

/// How long the main window's frames take with this computer's plug-ins
/// known (BananaV's cache of them: hundreds of Sapphire's say), without
/// an effect of theirs and with one in the FX window. Prints the times.
#[test]
#[ignore = "reads this computer's plug-in cache; prints timings"]
fn frames_with_this_computers_plug_ins() {
    let Some(path) = std::env::var_os("APPDATA")
        .map(|a| PathBuf::from(a).join("BananaV").join("ofx_cache.json"))
        .filter(|p| p.exists())
    else {
        eprintln!("no plug-in cache here");
        return;
    };
    let cache = bv_ofx::scan::Cache::load(&path);
    let infos: Vec<bv_ofx::PluginInfo> = cache.plugins().into_iter().cloned().collect();
    let glow = infos
        .iter()
        .find(|p| p.identifier.ends_with(".S_Glow"))
        .map(|p| p.identifier.clone());
    bv_ofx::registry().set_scanned(infos);
    let timed = |with: Option<&str>| {
        let mut app = new_app();
        app.load_demo();
        app.welcome_for_review("closed");
        if let Some(id) = with {
            assert!(app.plugin_effect_for_review(id), "{id}");
        }
        let mut h = Harness::builder()
            .with_size(egui::vec2(1600.0, 1000.0))
            .with_pixels_per_point(1.0)
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        settle(&mut h);
        let start = std::time::Instant::now();
        for _ in 0..30 {
            h.step();
        }
        start.elapsed().as_secs_f64() * 1000.0 / 30.0
    };
    eprintln!("frame: {:.1} ms (no plug-in effect)", timed(None));
    if let Some(id) = glow {
        eprintln!(
            "frame: {:.1} ms (S_Glow in the FX window)",
            timed(Some(&id))
        );
        bv_ofx::unload_all(Duration::from_secs(5));
    }
}

/// Another maker's plug-ins in BananaV: a Sapphire effect on an event, its
/// settings in the FX window and its picture in the preview. Needs
/// Sapphire OFX installed (skipped otherwise); its binary is loaded here,
/// and told it is done with at the end (its preset browser waits in the
/// background until then).
#[test]
#[ignore = "renders images for visual review; needs Sapphire OFX"]
fn captura_sapphire() {
    let binary = std::path::Path::new(
        r"C:\Program Files\Common Files\OFX\Plugins\Sapphire.ofx.bundle\Contents\Win64\Sapphire.ofx",
    );
    if !binary.exists() {
        eprintln!("no Sapphire OFX here");
        return;
    }
    let loaded = bv_ofx::Binary::load(binary).expect("Sapphire loads");
    bv_ofx::registry()
        .add_built_in(loaded)
        .expect("Sapphire describes itself");
    for (id, tag) in [
        ("com.genarts.sapphire.Lighting.S_Glow", "sapphire_glow"),
        (
            "com.genarts.sapphire.Lighting.S_LensFlare",
            "sapphire_lens_flare",
        ),
    ] {
        let mut app = new_app();
        app.load_demo();
        app.welcome_for_review("closed");
        assert!(app.plugin_effect_for_review(id), "{id}");
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
        settle(&mut h);
        snap(&mut h, tag);
    }
    bv_ofx::unload_all(Duration::from_secs(5));
}

/// The track list at the widths of the user's VEGAS prints, from only the
/// color bars to a list wider than the tracks: what each header keeps.
#[test]
#[ignore = "renders images for visual review"]
fn captura_lista_de_trilhas_larguras() {
    let mut app = new_app();
    app.load_demo();
    app.welcome_for_review("closed");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1900.0, 1080.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    for width in [26.0, 105.0, 130.0, 245.0, 490.0, 1500.0] {
        h.state_mut().track_list_width_for_review(width);
        settle(&mut h);
        snap(&mut h, &format!("lista_de_trilhas_{}", width as u32));
    }
}

/// The track headers' controls: the video track's Fade slider (Automation
/// Settings on), two audio tracks selected together.
#[test]
#[ignore = "renders images for visual review"]
fn captura_controles_do_cabecalho() {
    let mut app = new_app();
    app.load_demo();
    assert!(app.header_controls_for_review());
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
    settle(&mut h);
    snap(&mut h, "controles_do_cabecalho");
}

/// Set Default Track Properties on the demo's audio track (Volume and
/// Track FX ticked; video's items greyed, Input monitor still to come).
#[test]
#[ignore = "renders images for visual review"]
fn captura_propriedades_padrao_de_trilha() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    snap(&mut h, "propriedades_padrao_de_trilha");
}

/// Track groups: the demo's video tracks collapsed in "Picture" (the bars
/// of their events on its header's lane), its audio tracks open in
/// "Dialogue" (soloed, selected), a third audio track outside them.
#[test]
#[ignore = "renders images for visual review"]
fn captura_grupos_de_trilhas() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    snap(&mut h, "grupos_de_trilhas");
}

/// The Surround Panner (a 5.1 demo): the first audio track panned in the
/// room with three keyframes — their path, Smoothness, the center 3 dB
/// down —, its keyframes row under it, the small panners in the track
/// headers and the bus track; then the Mixing Console's.
#[test]
#[ignore = "renders images for visual review"]
fn captura_surround_panner() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    snap(&mut h, "surround_panner");
    h.state_mut().show_tab_named("Mixing Console");
    snap(&mut h, "surround_console");
    h.state_mut().show_tab_named("Master Bus");
    snap(&mut h, "surround_master");
}

/// The Color Grading panel in the Color Correction layout, as VEGAS 23's,
/// a little grade on the demo's first video event: each group — LUTs,
/// Color Wheels, RL Color Wheels, Utilities, Curves — its items side by
/// side, the HSL Curves alone, an item alone with its group open (Lift,
/// White Balance).
#[test]
#[ignore = "renders images for visual review"]
fn captura_color_grading() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    for (section, file) in [
        (0, "color_grading_luts"),
        (1, "color_grading_wheels"),
        (2, "color_grading_rl"),
        (3, "color_grading_utilities"),
        (4, "color_grading_curves"),
        (5, "color_grading_hsl"),
    ] {
        h.state_mut().color_grading_for_review(section);
        snap(&mut h, file);
    }
    assert!(h.state_mut().color_grading_item_for_review(1, 0));
    snap(&mut h, "color_grading_lift");
    assert!(h.state_mut().color_grading_item_for_review(3, 2));
    snap(&mut h, "color_grading_balance");
    h.state_mut().color_grading_for_review(5);
    // The HSL Curves' tangents on every point (All tangents).
    h.state_mut().hsl_tangents_for_review();
    snap(&mut h, "color_grading_hsl_tangentes");
    // The pickers: Lift's black point waiting, the color under the pointer
    // read out; the RL thresholds on every scope (Show Limits on Scopes).
    h.state_mut().color_grading_for_review(1);
    h.state_mut().color_picker_for_review(0);
    h.state_mut().scopes_for_review(7);
    snap(&mut h, "color_grading_picker");
    h.state_mut().color_grading_for_review(3);
    h.state_mut().color_picker_for_review(2);
    snap(&mut h, "color_grading_neutral");
    h.state_mut().color_grading_for_review(5);
    h.state_mut().color_picker_for_review(3);
    snap(&mut h, "color_grading_selector");
    // The Color Curves: three on the shared graph, zoomed; side by side
    // once the pinned curves are wide.
    assert!(h.state_mut().grading_curves_for_review(false));
    snap(&mut h, "color_grading_curves_zoom");
    assert!(h.state_mut().grading_curves_for_review(true));
    snap(&mut h, "color_grading_curves_lado");
}

/// The Mixing Console in the Audio Mixing layout: the Channel List and
/// View Controls, every control region labelled — Insert FX, Sends, I/O,
/// VU meters, peak meters, faders — on tracks, a chain, busses and the
/// Master.
#[test]
#[ignore = "renders images for visual review"]
fn captura_mixing_console() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    for label in [
        "Meters",
        "Faders",
        "Show All",
        "Drums",
        "Compressor",
        "Insert Input Bus",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "mixing_console");
    // An input bus's strip (its output off: no device opens).
    h.state_mut().input_bus_for_review();
    settle(&mut h);
    assert!(h.query_all_by_label("Guitar").next().is_some());
    snap(&mut h, "mixing_console_entrada");
}

/// The Mixing Console as a project shows it at first, laid out as VEGAS
/// 23's: the I/O and Faders regions of eight audio tracks — a header with
/// the track's number, its input and output, Touch's row, the pan, the
/// button column beside the fader and its meter —, the third track picked,
/// and the Master.
#[test]
#[ignore = "renders images for visual review"]
fn captura_mixing_console_padrao() {
    let mut app = new_app();
    app.load_demo();
    app.mixing_console_default_for_review();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1920.0, 1080.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    settle(&mut h);
    for label in ["Show All", "Meters", "Faders", "Insert Audio Track"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "mixing_console_padrao");
}

/// The Mixing Console after Show All, as VEGAS 23's prints: the Preview
/// channel first, a meter for each channel in the Meters region, eight
/// tracks in the palette's colors — the first three's automation on in
/// Touch (the button blue; the pan's mark, the mute and the fader's knob
/// red), then Read, Latch and Off —, three busses with their letters, the
/// Master picked; then Latch's menu of the automation modes open.
#[test]
#[ignore = "renders images for visual review"]
fn captura_mixing_console_show_all() {
    let mut app = new_app();
    app.load_demo();
    app.mixing_console_show_all_for_review();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1920.0, 1080.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    settle(&mut h);
    snap(&mut h, "mixing_console_show_all");
    h.get_by_label("Automation Write (Latch)").click();
    settle(&mut h);
    assert!(h.query_all_by_label("Automation Off").next().is_some());
    snap(&mut h, "mixing_console_menu_automacao");
}

#[test]
#[ignore = "renders images for visual review"]
fn captura_modern_eq() {
    use bv_engine::modern_eq::BandType;
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    for label in [
        "Filter Graph",
        "Filter Type:",
        "Channel Routing:",
        "Global",
        "Band 1",
        "Mono Below",
        "Stereo Width",
        "Output Gain",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "modern_eq");
    let graph = h.get_by_label("Filter Graph").rect();
    let x = |f: f32| graph.left() + graph.width() * (f / 20.0).ln() / 1000f32.ln();
    let y = |db: f32| graph.center().y - db / 12.0 * (graph.height() / 2.0 - 8.0);
    // A double-click at 1.2 kHz and +6 dB adds a peak there, in the first
    // free slot.
    double_click_at(&mut h, egui::pos2(x(1200.0), y(6.0)));
    let added = eq_settings(&h).bands[4];
    assert!(added.used && added.kind == BandType::Peaking, "{added:?}");
    assert!(
        (added.freq / 1200.0 - 1.0).abs() < 0.03 && (added.gain_db - 6.0).abs() < 0.3,
        "{added:?}"
    );
    snap(&mut h, "modern_eq_banda_nova");
    // Band 2's circle dragged to 900 Hz and −3 dB.
    drag(
        &mut h,
        egui::pos2(x(450.0), y(4.5)),
        egui::pos2(x(900.0), y(-3.0)),
        None,
    );
    let moved = eq_settings(&h).bands[1];
    assert!(
        (moved.freq / 900.0 - 1.0).abs() < 0.03 && (moved.gain_db + 3.0).abs() < 0.3,
        "{moved:?}"
    );
    // Band 3's menu.
    right_click_at(&mut h, egui::pos2(x(7000.0), y(-5.0)));
    settle(&mut h);
    for label in ["Cut", "Copy", "Replace", "Delete Band"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "modern_eq_menu");
    // Copied, it replaces band 5 (its number kept).
    h.get_by_label("Copy").click();
    settle(&mut h);
    let five = graph.left() + graph.width() * (1200f32 / 20.0).ln() / 1000f32.ln();
    right_click_at(&mut h, egui::pos2(five, y(6.0)));
    settle(&mut h);
    h.get_by_label("Replace").click();
    settle(&mut h);
    let s = eq_settings(&h);
    assert!(
        s.bands[4].kind == BandType::HighShelf && (s.bands[4].freq - 7000.0).abs() < 1.0,
        "{:?}",
        s.bands[4]
    );
    // A/B: B starts as a copy of A; band 1 deleted there (a double-click
    // on its circle) is back on A.
    let ab = |h: &Harness<'_, BananaApp>, name: &str| {
        h.get_all_by_label(name)
            .find(|n| n.rect().bottom() < graph.top() && n.rect().left() > graph.left() - 60.0)
            .map(|n| n.rect().center())
            .expect(name)
    };
    let b = ab(&h, "B");
    click_at(&mut h, b);
    settle(&mut h);
    double_click_at(&mut h, egui::pos2(x(70.0), y(0.0)));
    assert!(!eq_settings(&h).bands[0].used);
    snap(&mut h, "modern_eq_b");
    let a = ab(&h, "A");
    click_at(&mut h, a);
    settle(&mut h);
    let s = eq_settings(&h);
    assert!(s.bands[0].used && s.bands[0].kind == BandType::HighPass);
}

/// A text parameter of the selected event's OFX effects (empty: none).
fn ofx_text(h: &Harness<'_, BananaApp>, name: &str) -> String {
    match ofx_value(h, name) {
        Some(bv_timeline::PluginValue::Text(t)) => t,
        None => String::new(),
        other => panic!("{name}: {other:?}"),
    }
}

/// Color Match, as VEGAS's Help describes it: in Video FX's Color; its
/// window (Match Brightness, the source and target images with their
/// buttons on a line each, Strength); the source image taken from the
/// Video Preview's frame (the color bars, at 1 s), the target from the
/// event's own picture (the gradient, at 7 s), a mosaic of each under its
/// row; the gradient then takes the bars' colors.
#[test]
#[ignore = "renders images for visual review"]
fn captura_color_match() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_all_by_label("Color").next().expect("Color").click();
    settle(&mut h);
    h.get_all_by_label("Color Match")
        .next()
        .expect("Color Match")
        .click();
    settle(&mut h);
    snap(&mut h, "color_match_video_fx");
    add_preset(&mut h, "(Default)");
    for label in [
        "Match Brightness:",
        "Source Image:",
        "Target Image:",
        "Strength:",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "color_match_janela");
    // The FX window's Preview buttons, the source's first.
    let previews = |h: &Harness<'_, BananaApp>| {
        let mut v: Vec<egui::Rect> = h
            .get_all_by_label("Preview")
            .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::Button)
            .map(|n| n.rect())
            .filter(|r| r.left() < 560.0)
            .collect();
        v.sort_by(|a, b| a.top().total_cmp(&b.top()));
        v
    };
    h.state_mut().seek_seconds(1.0);
    settle(&mut h);
    let p = previews(&h);
    assert_eq!(p.len(), 2, "{p:?}");
    click_at(&mut h, p[0].center());
    settle(&mut h);
    assert!(ofx_text(&h, "sourceLook").starts_with("1920 1080 "));
    assert_eq!(ofx_text(&h, "source"), "1920 × 1080");
    h.state_mut().seek_seconds(7.0);
    settle(&mut h);
    let p = previews(&h);
    click_at(&mut h, p[1].center());
    settle(&mut h);
    assert!(!ofx_text(&h, "targetLook").is_empty(), "the target taken");
    snap(&mut h, "color_match_igualada");
}

/// Flicker Control, as VEGAS's Help describes it: in Video FX's Utility
/// with its presets; its window in Reduction (Temporal blend, then
/// Histogram smoothing with Calculate greyed) and in Addition (the
/// frequency).
#[test]
#[ignore = "renders images for visual review"]
fn captura_flicker_control() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Utility").click();
    settle(&mut h);
    h.get_all_by_label("Flicker Control")
        .next()
        .expect("Flicker Control")
        .click();
    settle(&mut h);
    for preset in [
        "Gentle",
        "Strong",
        "Time-Lapse",
        "Shadows Only",
        "Old Film",
        "Strobe",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "flicker_video_fx");
    add_preset(&mut h, "(Default)");
    for label in [
        "Mode:",
        "Reduction Mode:",
        "Analysis Window:",
        "Sensitivity:",
        "Type:",
        "Use motion compensation:",
        "Show mask only:",
        "Reset",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    assert!(h.query_all_by_label("Flicker Frequency:").next().is_none());
    snap(&mut h, "flicker_janela");
    pick_in_list(&mut h, "Temporal blend", "Histogram smoothing");
    assert!(h.query_all_by_label("Calculate").next().is_some());
    snap(&mut h, "flicker_histograma");
    pick_in_list(&mut h, "Reduction", "Addition");
    assert!(h.query_all_by_label("Flicker Frequency:").next().is_some());
    assert!(h.query_all_by_label("Analysis Window:").next().is_none());
    snap(&mut h, "flicker_adicao");
}

/// BananaV's own Saturation Adjust, as VEGAS's: in Video FX's Color with
/// VEGAS's presets, its window (Amount, Center, Spread, Low, High) and a
/// preset from the list.
#[test]
#[ignore = "renders images for visual review"]
fn captura_saturation_adjust() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_all_by_label("Color")
        .find(|n| n.accesskit_node().toggled().is_some())
        .expect("Color")
        .click();
    settle(&mut h);
    h.get_all_by_label("Saturation Adjust")
        .next()
        .expect("Saturation Adjust")
        .click();
    settle(&mut h);
    for preset in [
        "Boost Midtones",
        "Reduce Minor Color Noise",
        "Reduce Oversaturated Colors",
        "Selectively Desaturate",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "saturation_video_fx");
    add_preset(&mut h, "(Default)");
    for label in ["Amount:", "Center:", "Spread:", "Low:", "High:"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "saturation_janela");
    pick_in_list(&mut h, "(Default)", "Selectively Desaturate");
    assert_eq!(corrector_value(&h, "amount"), [-1.0]);
    assert_eq!(corrector_value(&h, "spread"), [0.16]);
    snap(&mut h, "saturation_seletiva");
}

/// The harness on the demo project with Video FX's `category` open and
/// `effect` picked in it.
fn video_fx_harness(category: &str, effect: &str) -> Harness<'static, BananaApp> {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    // The chooser's category tab (a toggle: another window may have a
    // button of the same name).
    h.get_all_by_label(category)
        .find(|n| n.accesskit_node().toggled().is_some())
        .expect(category)
        .click();
    settle(&mut h);
    // The list's item, a toggle (another effect may have a preset of its
    // name: Gradient Map's Sepia), scrolled into view first (the last ones
    // reach under the docked windows' tabs).
    h.get_all_by_label(effect)
        .find(|n| n.accesskit_node().toggled().is_some())
        .expect(effect)
        .scroll_to_me();
    settle(&mut h);
    h.get_all_by_label(effect)
        .find(|n| n.accesskit_node().toggled().is_some())
        .expect(effect)
        .click();
    settle(&mut h);
    h
}

/// BananaV's own Border, as VEGAS's: in Video FX's Creative with VEGAS's
/// presets; its window shows what the type uses (Blurred: the size; a
/// bevel: its angle and color; Solid: the color with its alpha, and the
/// picker's alpha slider).
#[test]
#[ignore = "renders images for visual review"]
fn captura_border() {
    let mut h = video_fx_harness("Creative", "Border");
    for preset in [
        "Soft Edge",
        "Bevel",
        "Solid White Border",
        "Translucent Blue Border",
        "Pyramid",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "border_video_fx");
    add_preset(&mut h, "(Default)");
    for label in ["Type:", "Size:"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    assert!(
        h.query_all_by_label("Angle:").next().is_none(),
        "Angle shows"
    );
    snap(&mut h, "border_janela");
    pick_in_list(&mut h, "(Default)", "Bevel");
    assert_eq!(corrector_value(&h, "type"), [1.0]);
    assert_eq!(corrector_value(&h, "angle"), [325.0]);
    assert!(h.query_all_by_label("Angle:").next().is_some(), "no Angle");
    snap(&mut h, "border_chanfro");
    pick_in_list(&mut h, "Bevel", "Translucent Blue Border");
    assert!(
        h.query_all_by_label("Angle:").next().is_none(),
        "Angle shows"
    );
    // The color as VEGAS writes it in RGB, its alpha last.
    assert!(
        h.query_all_by_value("0.0, 0.0, 1.0, 0.5").next().is_some(),
        "the color's numbers"
    );
    let swatch = h
        .get_all_by_label("Color")
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::ColorWell)
        .expect("the swatch")
        .rect();
    click_at(&mut h, swatch.center());
    settle(&mut h);
    assert!(h.query_all_by_value("RGB").next().is_some(), "no RGB");
    snap(&mut h, "border_azul");
}

/// BananaV's own Black Bar Fill, as VEGAS's: in Video FX's Utility with
/// VEGAS's presets; its window (Foreground and Background, each with its
/// Location, Scale and blur) and a preset moving the picture left.
#[test]
#[ignore = "renders images for visual review"]
fn captura_black_bar_fill() {
    let mut h = video_fx_harness("Utility", "Black Bar Fill");
    for preset in [
        "Foreground Scaled",
        "Foreground Left, No Shadow",
        "Foreground Right, No Shadow",
        "Foreground No Shadow",
        "Foreground Maximum Shadow",
        "Background No Blur",
        "Background Maximum Blur",
        "Background Maximum Scale",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "bbf_video_fx");
    add_preset(&mut h, "(Default)");
    for label in ["Foreground", "Background", "Shadow Blur:", "Blur:"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "bbf_janela");
    pick_in_list(&mut h, "(Default)", "Foreground Left, No Shadow");
    assert_eq!(corrector_value(&h, "fgLocation"), [0.158, 0.5]);
    assert_eq!(corrector_value(&h, "shadowBlur"), [0.0]);
    snap(&mut h, "bbf_esquerda");
}

/// BananaV's own Unsharp Mask, as VEGAS's: in Video FX's Blur with VEGAS's
/// presets; its window (Amount, Radius, Threshold) and Extreme, its Amount
/// past the slider's end (a dot there instead of the knob, as VEGAS's).
#[test]
#[ignore = "renders images for visual review"]
fn captura_unsharp_mask() {
    let mut h = video_fx_harness("Blur", "Unsharp Mask");
    for preset in [
        "Light",
        "Medium",
        "Strong",
        "Extreme",
        "Negative",
        "Outline Edges",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "unsharp_video_fx");
    add_preset(&mut h, "(Default)");
    for label in ["Amount:", "Radius:", "Threshold:"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "unsharp_janela");
    pick_in_list(&mut h, "(Default)", "Extreme");
    assert_eq!(corrector_value(&h, "amount"), [4.0]);
    assert_eq!(corrector_value(&h, "threshold"), [0.1]);
    snap(&mut h, "unsharp_extreme");
}

/// BananaV's own Add Noise, as VEGAS's: in Video FX's Creative with VEGAS's
/// presets; its window (Noise level and three checkboxes) and Color.
#[test]
#[ignore = "renders images for visual review"]
fn captura_add_noise() {
    let mut h = video_fx_harness("Creative", "Add Noise");
    for preset in ["Medium", "Grainy", "Extreme", "Color", "Screen"] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "noise_video_fx");
    add_preset(&mut h, "(Default)");
    for label in [
        "Noise level:",
        "Monochromatic:",
        "Gaussian noise:",
        "Animate:",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "noise_janela");
    pick_in_list(&mut h, "(Default)", "Color");
    assert_eq!(corrector_value(&h, "level"), [1.0]);
    assert_eq!(corrector_value(&h, "monochromatic"), [0.0]);
    snap(&mut h, "noise_cor");
}

/// BananaV's own Timecode, as VEGAS's: in Video FX's Utility with VEGAS's
/// presets; its window (the format, the alignments, Width and Height) and
/// Seconds; the time written in the preview's lower right.
#[test]
#[ignore = "renders images for visual review"]
fn captura_timecode() {
    let mut h = video_fx_harness("Utility", "Timecode");
    for preset in [
        "SMPTE Drop (29.97 fps)",
        "SMPTE Drop (29.97 fps), Small",
        "SMPTE Non-Drop (29.97 fps)",
        "SMPTE Film IVTC (23.976 fps)",
        "SMPTE Film (24 fps)",
        "Seconds",
        "Absolute Frames",
        "Time & Frames",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "timecode_video_fx");
    add_preset(&mut h, "(Default)");
    for label in [
        "Timecode format:",
        "Horizontal alignment:",
        "Vertical alignment:",
        "Width:",
        "Height:",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "timecode_janela");
    pick_in_list(&mut h, "(Default)", "Seconds");
    assert_eq!(corrector_value(&h, "format"), [1.0]);
    snap(&mut h, "timecode_segundos");
}

/// BananaV's own Color Balance, as VEGAS's: in Video FX's Color with
/// VEGAS's presets, its window (Red, Green, Blue, Type, Preserve
/// luminosity) and a preset from the list.
#[test]
#[ignore = "renders images for visual review"]
fn captura_color_balance() {
    let mut h = video_fx_harness("Color", "Color Balance");
    for preset in [
        "Red Midtones",
        "Green Midtones",
        "Blue Midtones",
        "Red Highlight",
        "Green Highlight",
        "Blue Highlight",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "balance_video_fx");
    add_preset(&mut h, "(Default)");
    for label in ["Red:", "Green:", "Blue:", "Type:", "Preserve luminosity:"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "balance_janela");
    pick_in_list(&mut h, "(Default)", "Blue Highlight");
    assert_eq!(corrector_value(&h, "blue"), [0.1]);
    assert_eq!(corrector_value(&h, "type"), [0.0]);
    snap(&mut h, "balance_azul_realces");
}

/// BananaV's own Brightness and Contrast, as VEGAS 23's: in Video FX's
/// Color with VEGAS's presets; its window (Brightness, Contrast, Contrast
/// center, each with its clock, Automatic Contrast closed); Automatic
/// Contrast opened (Input's Histogram Cut-Off, Output's Minimum and
/// Maximum Brightness, Analysis Mode, Calculate Brightness & Contrast —
/// no clocks, as VEGAS's); a preset from the list.
#[test]
#[ignore = "renders images for visual review"]
fn captura_brightness_contrast() {
    let mut h = video_fx_harness("Color", "Brightness and Contrast");
    for preset in [
        "Brighter",
        "Very Bright",
        "Darker",
        "Very Dark",
        "Brighter, More Contrast",
        "Brighter, Less Contrast",
        "Darker, More Contrast",
        "Darker, Less Contrast",
        "Flat",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "brilho_contraste_video_fx");
    add_preset(&mut h, "(Default)");
    for label in ["Brightness:", "Contrast:", "Contrast center:"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    assert_eq!(h.query_all_by_label("Animate").count(), 3);
    snap(&mut h, "brilho_contraste_janela");
    h.get_by_label("Automatic Contrast").click();
    settle(&mut h);
    for label in [
        "Histogram Cut-Off:",
        "Minimum Brightness:",
        "Maximum Brightness:",
        "Analysis Mode:",
        "Calculate Brightness & Contrast",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    // Automatic Contrast's settings have no keyframes.
    assert_eq!(h.query_all_by_label("Animate").count(), 3);
    snap(&mut h, "brilho_contraste_automatico");
    pick_in_list(&mut h, "(Default)", "Brighter, Less Contrast");
    assert_eq!(corrector_value(&h, "brightness"), [0.15]);
    assert_eq!(corrector_value(&h, "contrast"), [-0.15]);
    snap(&mut h, "brilho_contraste_preset");
}

/// BananaV's own Levels, as VEGAS 23's: in Video FX's Color with VEGAS's
/// presets; its window (Channel, Input start and end, Output start and
/// end, Gamma — each with its clock, Channel's too); a preset from the
/// list.
#[test]
#[ignore = "renders images for visual review"]
fn captura_levels() {
    let mut h = video_fx_harness("Color", "Levels");
    for preset in [
        "Brighten",
        "Darken",
        "Increase Gamma",
        "Decrease Gamma",
        "Studio RGB to Computer RGB",
        "Computer RGB to Studio RGB",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "levels_video_fx");
    add_preset(&mut h, "(Default)");
    for label in [
        "Channel:",
        "Input start:",
        "Input end:",
        "Output start:",
        "Output end:",
        "Gamma:",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    assert_eq!(h.query_all_by_label("Animate").count(), 6);
    snap(&mut h, "levels_janela");
    pick_in_list(&mut h, "(Default)", "Studio RGB to Computer RGB");
    assert_eq!(corrector_value(&h, "inputStart"), [0.063]);
    assert_eq!(corrector_value(&h, "inputEnd"), [0.92]);
    snap(&mut h, "levels_preset");
}

/// An effect's screens: its name, VEGAS's presets, its window's labels,
/// the preset picked and the pictures' names.
struct EffectScreens {
    category: &'static str,
    effect: &'static str,
    presets: &'static [&'static str],
    labels: &'static [&'static str],
    pick: &'static str,
    file: &'static str,
}

/// BananaV's own Black and White, Invert and Sepia, as VEGAS 23's: each in
/// Video FX's Color with VEGAS's presets, its window and a preset picked.
#[test]
#[ignore = "renders images for visual review"]
fn captura_preto_e_branco_inverter_e_sepia() {
    let cases = [
        EffectScreens {
            category: "Color",
            effect: "Black and White",
            presets: &[
                "25% Black and White",
                "50% Black and White",
                "75% Black and White",
                "100% Black and White",
            ],
            labels: &["Blend amount:"],
            pick: "50% Black and White",
            file: "preto_e_branco",
        },
        EffectScreens {
            category: "Color",
            effect: "Invert",
            presets: &["60% Inversion", "75% Inversion", "100% Inversion"],
            labels: &["Blend amount:"],
            pick: "60% Inversion",
            file: "inverter",
        },
        EffectScreens {
            category: "Color",
            effect: "Sepia",
            presets: &[
                "Neutral",
                "Warm",
                "Cold",
                "Old Photographs",
                "Old Newspaper",
                "Currency",
            ],
            labels: &["Color:", "Blending strength:", "Blending falloff:"],
            pick: "Old Photographs",
            file: "sepia",
        },
    ];
    effect_screens(&cases);
}

/// Each case's effect in Video FX with its presets, its window and a
/// preset picked.
fn effect_screens(cases: &[EffectScreens]) {
    for &EffectScreens {
        category,
        effect,
        presets,
        labels,
        pick,
        file,
    } in cases
    {
        let mut h = video_fx_harness(category, effect);
        snap(&mut h, &format!("{file}_video_fx"));
        for preset in presets {
            assert!(
                h.query_all_by_label(preset).next().is_some(),
                "{effect}: no {preset}"
            );
        }
        add_preset(&mut h, "(Default)");
        for label in labels {
            assert!(
                h.query_all_by_label(label).next().is_some(),
                "{effect}: no {label}"
            );
        }
        assert_eq!(h.query_all_by_label("Animate").count(), labels.len());
        snap(&mut h, &format!("{file}_janela"));
        pick_in_list(&mut h, "(Default)", pick);
        snap(&mut h, &format!("{file}_preset"));
    }
}

/// BananaV's own Gaussian Blur, Sharpen, Vignette, Mirror and Glow, as
/// VEGAS 23's: each in its Video FX category with VEGAS's presets, its
/// window and a preset picked.
#[test]
#[ignore = "renders images for visual review"]
fn captura_desfoque_nitidez_vinheta_espelho_e_brilho() {
    effect_screens(&[
        EffectScreens {
            category: "Blur",
            effect: "Gaussian Blur",
            presets: &[
                "Medium Blur",
                "Light Blur",
                "Extreme Blur",
                "Soften",
                "Yellow Glow",
            ],
            labels: &["Horizontal range:", "Vertical range:"],
            pick: "Yellow Glow",
            file: "desfoque_gaussiano",
        },
        EffectScreens {
            category: "Blur",
            effect: "Sharpen",
            presets: &["Medium", "Light", "Heavy"],
            labels: &["Amount:"],
            pick: "Heavy",
            file: "nitidez",
        },
        EffectScreens {
            category: "Creative",
            effect: "Vignette",
            presets: &["Edges", "Highlight", "Spotlight", "Subtle", "White"],
            labels: &[
                "Outer radius:",
                "Inner radius:",
                "Aspect:",
                "Center:",
                "Rotation:",
                "Feather:",
                "Color:",
                "Invert:",
            ],
            pick: "Highlight",
            file: "vinheta",
        },
        EffectScreens {
            category: "Creative",
            effect: "Mirror",
            presets: &[
                "Reflect Top",
                "Reflect Bottom",
                "Reflect Left",
                "Reflect Right",
                "Reflect to Upper Left",
                "Reflect to Upper Right",
                "Reflect to Lower Left",
                "Reflect to Lower Right",
            ],
            labels: &["Angle:", "Center:", "Background:"],
            pick: "Reflect to Upper Left",
            file: "espelho",
        },
        EffectScreens {
            category: "Creative",
            effect: "Glow",
            presets: &[
                "White Soft Glow",
                "White Intense Glow",
                "White Highlights",
                "Red Glow",
                "Blue Glow Everywhere",
                "Green Highlights on Brights",
            ],
            labels: &["Glow percent:", "Intensity:", "Suppression:", "Color:"],
            pick: "White Intense Glow",
            file: "brilho",
        },
    ]);
}

/// The Video Preview in a narrow pane (the main window narrow, the default
/// layout), as VEGAS's: the toolbar cut off at its edge, the frame fitting
/// its width, never cut, the transport's other buttons in More.
#[test]
#[ignore = "renders images for visual review"]
fn preview_estreito() {
    for (width, name) in [
        (1000.0, "preview_estreito_1000"),
        (640.0, "preview_estreito_640"),
    ] {
        let mut app = new_app();
        app.load_demo();
        let mut h = Harness::builder()
            .with_size(egui::vec2(width, 760.0))
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
        let frame = h.state().preview_frame_for_review();
        let pane = h.state().preview_area_for_review();
        assert!(
            frame.width() > 50.0 && pane.expand(1.0).contains_rect(frame),
            "{name}: the frame {frame:?} inside the pane {pane:?}"
        );
        // The preview's More besides the demo's two tracks' (⋯).
        assert!(h.query_all_by_label("More").count() > 2, "{name}: More");
    }
}

/// BananaV's Pan/Crop as VEGAS 23's: the workspace alone at first (Show
/// Properties off), VEGAS's grid, the box in dark and light dashes with
/// the rotation circle round it; the shortcut menu VEGAS's six; a drag
/// round the box turns it; Show Properties shows them.
#[test]
#[ignore = "renders images for visual review"]
fn pancrop_como_o_vegas() {
    let mut app = new_app();
    app.load_demo();
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
    // Off at first, as VEGAS's (whatever the user's settings say).
    h.state_mut().pan_crop_properties_for_review(false);
    h.state_mut().open_clip_window("pancrop");
    snap(&mut h, "pancrop_vegas");
    let event = h.state().clip_for_review().expect("the event").id;
    let spot = |h: &Harness<'_, BananaApp>, sx: f32, sy: f32| {
        h.state()
            .pan_crop_spot_for_review(sx, sy)
            .expect("the box drawn")
    };
    let inside = spot(&h, 0.3, 0.3);
    right_click_at(&mut h, inside);
    settle(&mut h);
    snap(&mut h, "pancrop_menu");
    for label in [
        "Restore",
        "Center",
        "Flip Horizontal",
        "Flip Vertical",
        "Match Output Aspect",
        "Match Source Aspect",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    assert!(h.query_all_by_label("Fit to Width").next().is_none());
    // A click past the circle closes the menu and does nothing else.
    let (middle, corner) = (spot(&h, 0.0, 0.0), spot(&h, -1.0, -1.0));
    click_at(&mut h, middle + (corner - middle) * 1.6);
    settle(&mut h);
    assert!(h.query_all_by_label("Match Source Aspect").next().is_none());
    // Just past the top right corner, inside the circle: it turns, the
    // pointer a turning arrow there.
    let corner = spot(&h, 1.0, -1.0);
    let from = corner + egui::vec2(14.0, -10.0);
    h.event(egui::Event::PointerMoved(from));
    settle(&mut h);
    snap(&mut h, "pancrop_cursor_girar");
    drag(&mut h, from, from + egui::vec2(30.0, 60.0), None);
    // At the cursor, inside the event: a keyframe there.
    let c = h.state().clip_by_id_for_review(event).expect("the event");
    let angle = c
        .transform_keys
        .iter()
        .map(|k| k.value.rotation_deg)
        .chain([c.transform.rotation_deg])
        .fold(0.0f32, |m, a| if a.abs() > m.abs() { a } else { m });
    assert!(angle.abs() > 1.0, "turned {angle}");
    snap(&mut h, "pancrop_girado");
    h.get_by_label("Show Properties").click();
    settle(&mut h);
    snap(&mut h, "pancrop_propriedades");
    // Off again: what the next window opens with.
    h.get_by_label("Show Properties").click();
    settle(&mut h);
}

/// The Pan/Crop window's properties and keyframe controller as VEGAS 23's
/// (the user's prints): the properties' sections in VEGAS's grid;
/// keyframes picked with a click, with Ctrl and a click, and by dragging
/// the needle across them from an empty spot; their menu (Cut, Copy,
/// Paste, Delete and the curves) on all of those picked; the first one
/// dragged later; what is past the event's end white; then the Mask
/// row's properties as VEGAS's.
#[test]
#[ignore = "renders images for visual review"]
fn pancrop_quadros_chave_como_o_vegas() {
    use bv_timeline::KeyCurve;
    let mut app = new_app();
    app.load_demo();
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
    assert!(h.state_mut().pan_crop_keys_for_review());
    settle(&mut h);
    snap(&mut h, "pancrop_propriedades_vegas");
    for label in [
        "Width:",
        "Height:",
        "X Center:",
        "Angle:",
        "Smoothness:",
        "Maintain aspect ratio:",
        "Stretch to fill frame:",
        "Zoom (%):",
        "X Offset:",
        "Grid spacing:",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    let event = h.state().clip_for_review().expect("the event").id;
    let key = |h: &Harness<'_, BananaApp>, i: usize| {
        h.state()
            .pan_crop_key_spot_for_review(false, i)
            .expect("the keyframe drawn")
    };
    let lane = |h: &Harness<'_, BananaApp>, f: f32| {
        h.state()
            .pan_crop_lane_spot_for_review(false, f)
            .expect("the row drawn")
    };
    let curves = |h: &Harness<'_, BananaApp>| -> Vec<KeyCurve> {
        let c = h.state().clip_by_id_for_review(event).expect("the event");
        c.transform_keys.iter().map(|k| k.curve).collect()
    };
    // A click picks one, Ctrl and a click another; their menu's curve is
    // both's.
    let (k1, k3) = (key(&h, 1), key(&h, 3));
    click_at(&mut h, k1);
    click_with(&mut h, k3, egui::Modifiers::COMMAND);
    settle(&mut h);
    let k3 = key(&h, 3);
    right_click_at(&mut h, k3);
    settle(&mut h);
    snap(&mut h, "pancrop_quadros_chave_menu");
    for label in [
        "Cut", "Copy", "Paste", "Delete", "Linear", "Fast", "Slow", "Smooth", "Sharp", "Hold",
    ] {
        assert!(
            h.query_all_by_label_contains(label).next().is_some(),
            "no {label}"
        );
    }
    h.get(kittest::by().predicate(|n| n.label().is_some_and(|l| l.starts_with("Hold"))))
        .click();
    settle(&mut h);
    use KeyCurve::{Hold, Linear};
    assert_eq!(curves(&h), [Linear, Hold, Linear, Hold, Linear]);
    // The needle dragged from an empty spot across the last three picks
    // them; Delete in their menu deletes them all.
    let (from, to) = (lane(&h, 0.25), lane(&h, 0.75));
    drag(&mut h, from, to, Some("pancrop_quadros_chave_varridos"));
    let k3 = key(&h, 3);
    right_click_at(&mut h, k3);
    settle(&mut h);
    // The menu's, by the keyframe (the toolbars have a Delete too).
    let item = h
        .query_all_by_label("Delete")
        .map(|n| n.rect().center())
        .filter(|c| (c.x - k3.x).abs() < 160.0 && c.y > k3.y)
        .min_by(|a, b| a.y.total_cmp(&b.y))
        .expect("the menu's Delete");
    click_at(&mut h, item);
    settle(&mut h);
    assert_eq!(curves(&h).len(), 2, "{:?}", curves(&h));
    // The first keyframe dragged later, as VEGAS lets it.
    let first = key(&h, 0);
    drag(&mut h, first, first + egui::vec2(60.0, 0.0), None);
    let c = h.state().clip_by_id_for_review(event).expect("the event");
    assert!(
        c.transform_keys[0].at > bv_core::Tick::ZERO,
        "{:?}",
        c.transform_keys
    );
    snap(&mut h, "pancrop_quadros_chave_primeiro");
    // The Mask row's properties.
    assert!(h.state_mut().pan_crop_masks_for_review());
    settle(&mut h);
    snap(&mut h, "pancrop_mascara_propriedades");
    for label in [
        "Apply to FX:",
        "Mode:",
        "Anti alias:",
        "Opacity (%):",
        "Feather type:",
        "Feather (%):",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
}

/// A double click puts an effect's setting back at its default: on a
/// slider, as VEGAS's; on a color's swatch; and on any setting's name — a
/// position's box is closed at first.
#[test]
#[ignore = "renders images for visual review"]
fn duplo_clique_volta_ao_padrao() {
    use egui::accesskit::Role;
    let mut h = video_fx_harness("Creative", "Mirror");
    add_preset(&mut h, "Reflect to Upper Left");
    assert_eq!(crop_value(&h, "angle"), [-135.0]);
    assert_eq!(crop_value(&h, "center"), [0.4, 0.6]);
    let slider = h
        .query_all_by_label("Angle")
        .find(|n| n.accesskit_node().role() == Role::Slider)
        .expect("Angle's slider")
        .rect();
    double_click_at(&mut h, slider.center() + egui::vec2(30.0, 0.0));
    assert_eq!(crop_value(&h, "angle"), [0.0]);
    let name = h.get_by_label("Center:").rect();
    double_click_at(&mut h, name.center());
    assert_eq!(crop_value(&h, "center"), [0.5, 0.5]);
    snap(&mut h, "duplo_clique_espelho");

    let mut h = video_fx_harness("Creative", "Glow");
    add_preset(&mut h, "Red Glow");
    assert_eq!(crop_value(&h, "color"), [1.0, 0.0, 0.0]);
    let swatch = h
        .query_all_by_label("Color")
        .find(|n| n.accesskit_node().role() == Role::ColorWell)
        .expect("Color's swatch")
        .rect();
    double_click_at(&mut h, swatch.center());
    assert_eq!(crop_value(&h, "color"), [0.0, 0.0, 0.0]);
    // The name brings back what a slider holds too.
    assert_eq!(crop_value(&h, "intensity"), [1.0]);
    let name = h.get_by_label("Intensity:").rect();
    double_click_at(&mut h, name.center());
    assert_eq!(crop_value(&h, "intensity"), [0.0]);
    snap(&mut h, "duplo_clique_brilho");
}

/// ACES (File › Properties › Video): the demo without it and through
/// sRGB (ACES) and Rec.709 (ACES); Project Properties with it on (the
/// compositing gamma fixed at linear, the look enabled) and the view
/// transform's list. The project is changed only in memory and the window
/// is closed with Cancel.
#[test]
#[ignore = "renders images for visual review"]
fn captura_aces() {
    use bv_timeline::{AcesDisplay, AcesView, ViewTransform};
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().aces_for_review(None);
    snap(&mut h, "aces_previa_desligado");
    let view = |display| {
        Some(ViewTransform {
            display,
            view: AcesView::Aces,
        })
    };
    h.state_mut().aces_for_review(view(AcesDisplay::Rec709));
    snap(&mut h, "aces_previa_rec709");
    h.state_mut().aces_for_review(view(AcesDisplay::Srgb));
    snap(&mut h, "aces_previa_srgb");
    h.get_by_label("File").click();
    settle(&mut h);
    h.get(kittest::by().predicate(|n| n.label().is_some_and(|l| l.starts_with("Properties…"))))
        .click();
    settle(&mut h);
    for label in [
        "ACES version:",
        "ACES color space:",
        "View transform:",
        "Look modification transform:",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "aces_propriedades");
    // The view transform's list, the color spaces', motion blur's and
    // deinterlacing's.
    for (y, name) in [
        (422.0, "lista_desfoque_de_movimento"),
        (350.0, "aces_lista_exibicao"),
        (326.0, "aces_lista_espacos"),
        (446.0, "lista_desentrelacamento"),
    ] {
        click_at(&mut h, egui::pos2(340.0, y));
        snap(&mut h, name);
        h.key_press(egui::Key::Escape);
        settle(&mut h);
    }
    h.get_by_label("Cancel").click();
    settle(&mut h);
    // With the 2.222 gamma: the look greyed out, as VEGAS's.
    h.state_mut()
        .compositing_gamma_for_review(bv_timeline::CompositingGamma::Video);
    h.get_by_label("File").click();
    settle(&mut h);
    h.get(kittest::by().predicate(|n| n.label().is_some_and(|l| l.starts_with("Properties…"))))
        .click();
    settle(&mut h);
    snap(&mut h, "aces_propriedades_gama_video");
    h.get_by_label("Cancel").click();
    settle(&mut h);
    // ACES 1.3's view transforms, as VEGAS lists them.
    h.state_mut()
        .compositing_gamma_for_review(bv_timeline::CompositingGamma::Linear);
    h.state_mut()
        .aces_version_for_review(bv_timeline::AcesVersion::V1_3);
    h.get_by_label("File").click();
    settle(&mut h);
    h.get(kittest::by().predicate(|n| n.label().is_some_and(|l| l.starts_with("Properties…"))))
        .click();
    settle(&mut h);
    click_at(&mut h, egui::pos2(340.0, 350.0));
    snap(&mut h, "aces_lista_exibicao_1_3");
    h.key_press(egui::Key::Escape);
    settle(&mut h);
    h.get_by_label("Cancel").click();
    settle(&mut h);
}

/// BananaV's own Color Corrector (Secondary), as VEGAS's: in Video FX's
/// Color with VEGAS's presets; its window (Chrominance's wheel under its
/// triangle, the corrections, Select effect range, the masks' boxes and
/// the limits' groups, opened); a preset from the list.
#[test]
#[ignore = "renders images for visual review"]
fn captura_color_corrector_secondary() {
    let mut h = video_fx_harness("Color", "Color Corrector (Secondary)");
    for preset in [
        "Invert Color",
        "Desaturate All but Reds",
        "Desaturate Highs",
        "Boost Mids",
        "Darken Mids",
        "Saturate Greens",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "secundario_video_fx");
    add_preset(&mut h, "(Default)");
    for label in [
        "Chrominance:",
        "Rotate hue:",
        "Saturation:",
        "Gamma:",
        "Gain:",
        "Offset:",
        "Alpha:",
        "Select effect range",
        "Show mask:",
        "Invert mask:",
        "Limit luminance",
        "Limit saturation",
        "Limit hue",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "secundario_janela");
    for group in ["Limit luminance", "Limit saturation", "Limit hue"] {
        h.get_by_label(group).click();
        settle(&mut h);
    }
    assert!(
        h.query_all_by_label("Center:").next().is_some(),
        "no Center"
    );
    snap(&mut h, "secundario_limites");
    pick_in_list(&mut h, "(Default)", "Desaturate All but Reds");
    assert_eq!(corrector_value(&h, "saturation"), [0.0]);
    assert_eq!(corrector_value(&h, "hueCenter"), [283.5]);
    snap(&mut h, "secundario_menos_vermelhos");
}

/// The keyframes of the Crop's parameter `label` in its keyframe
/// controller, left to right.
fn crop_keys(h: &Harness<'_, BananaApp>, label: &str) -> Vec<egui::Rect> {
    let mut v: Vec<egui::Rect> = h
        .query_all_by_label_contains(&format!("Keyframe of {label} at "))
        .map(|n| n.rect())
        .collect();
    v.sort_by(|a, b| a.left().total_cmp(&b.left()));
    v
}

/// The keyframe controller, as VEGAS's: Location animated too shows its
/// numbers' rows; a keyframe's menu and the effect row's; the box that
/// picks keyframes (the cursor following it); Curves; the bar over it
/// dragged; its scroll bar's end zooming; the Columns buttons.
fn keyframe_controller(h: &mut Harness<'_, BananaApp>) {
    // Location's clock (X Crop's says it animates: Y Crop's, then
    // Location's).
    h.get_all_by_label("Animate")
        .nth(1)
        .expect("Location's clock")
        .click();
    settle(h);
    assert_eq!(crop_keys(h, "Location").len(), 1, "Location's row");
    assert_eq!(crop_keys(h, "X").len(), 1, "Location's X row");
    snap(h, "crop_faixas");
    // A keyframe's menu (X Crop's middle one).
    let middle = crop_keys(h, "X Crop")[1];
    right_click_at(h, middle.center());
    snap(h, "crop_menu_quadro");
    for item in [
        "Delete",
        "Cut",
        "Copy",
        "Paste",
        "Set to…",
        "Linear Fade",
        "Hold",
    ] {
        assert!(
            h.query_all_by_label_contains(item).next().is_some(),
            "no {item}"
        );
    }
    close_menu(h);
    // The effect row's (its diamond at the start).
    let diamond = crop_keys(h, "Crop")[0];
    right_click_at(h, diamond.center());
    settle(h);
    snap(h, "crop_menu_efeito");
    close_menu(h);
    // The box: from a spot between the keyframes past the middle one; the
    // keyframes inside are picked, and the cursor stops where it ends.
    let start = crop_keys(h, "X Crop")[0];
    let y = middle.center().y;
    let from = egui::pos2((start.center().x + middle.center().x) / 2.0, y + 8.0);
    let to = egui::pos2(middle.center().x + 40.0, y + 8.0);
    let before = h.state().cursor_seconds();
    drag(h, from, to, Some("crop_caixa_selecao"));
    assert!(h.state().cursor_seconds() > before, "the cursor followed");
    // The picked keyframe's menu: Delete deletes it (and Location's).
    right_click_at(h, crop_keys(h, "X Crop")[1].center());
    settle(h);
    h.get_by_label("Delete").click();
    settle(h);
    assert_eq!(crop_keys(h, "X Crop").len(), 1, "deleted");
    snap(h, "crop_apagado");
    // Curves, X Crop's curve edited.
    h.get_by_label("Curves").click();
    settle(h);
    let button = egui::accesskit::Role::Button;
    let x_crop = h.get_by_role_and_label(button, "X Crop").rect();
    h.get_by_role_and_label(button, "X Crop").click();
    settle(h);
    assert_eq!(crop_keys(h, "X Crop").len(), 1, "X Crop's curve edited");
    snap(h, "crop_curvas");
    // Its keyframe a locked Bézier one: its handles.
    right_click_at(h, crop_keys(h, "X Crop")[0].center());
    settle(h);
    h.query_all_by_label_contains("Bézier curve (locked tangents)")
        .next()
        .expect("the Bézier curve")
        .click();
    settle(h);
    assert!(
        h.query_all_by_label("Bézier handle").next().is_some(),
        "its handles"
    );
    snap(h, "crop_curvas_bezier");
    // The bar over the controller (over its ruler, over the effect's
    // row) dragged up: it grows.
    let bar = egui::pos2(x_crop.center().x, x_crop.top() - 30.0 - 22.0 - 3.0);
    drag(h, bar, bar - egui::vec2(0.0, 60.0), None);
    assert!(
        h.get_by_role_and_label(button, "X Crop").rect().top() < x_crop.top() - 40.0,
        "the controller grew"
    );
    snap(h, "crop_bandeja_maior");
    let sync = h.get_by_label("Sync Cursor").rect();
    // Its scroll bar's right end dragged left: it zooms in.
    let track = h
        .query_all_by_label("Scroll bar")
        .map(|n| n.rect())
        .find(|r| r.top() > sync.top() - 40.0 && r.top() < sync.top())
        .expect("the controller's scroll bar");
    let end = egui::pos2(track.right() - 2.0, track.center().y);
    let keys_before = crop_keys(h, "X Crop");
    drag(h, end, end - egui::vec2(track.width() / 2.0, 0.0), None);
    let keys_after = crop_keys(h, "X Crop");
    assert!(
        keys_after.is_empty() || keys_after[0].left() >= keys_before[0].left() - 1.0,
        "zoomed in"
    );
    snap(h, "crop_zoom_barra");
    // Columns.
    h.get_by_label("2 Columns").click();
    settle(h);
    snap(h, "crop_duas_colunas");
    h.get_by_label("1 Column").click();
    settle(h);
}

/// Closes a menu with a click on an empty spot of the FX window (the
/// main window takes Escape when the dialogs are embedded).
fn close_menu(h: &mut Harness<'_, BananaApp>) {
    let angle = h.get_by_label("Angle:").rect();
    click_at(h, egui::pos2(angle.left() + 230.0, angle.top() - 40.0));
    settle(h);
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

/// The curves of the selected event's Color Curves.
fn curves_value(h: &Harness<'_, BananaApp>) -> Vec<Vec<bv_core::curve::CurvePoint>> {
    match ofx_value(h, "curves") {
        Some(bv_timeline::PluginValue::Curves(c)) => c,
        None => vec![bv_core::curve::IDENTITY.to_vec(); 3],
        other => panic!("curves: {other:?}"),
    }
}

/// Picks `item` in the drop-down list showing `shown` (the item nearest
/// the list: others may have its name).
fn pick_in_list(h: &mut Harness<'_, BananaApp>, shown: &str, item: &str) {
    let list = h.get_by_value(shown).rect();
    h.get_by_value(shown).click();
    settle(h);
    let at = h
        .get_all_by_label(item)
        .map(|n| n.rect())
        .min_by(|a, b| {
            let d = |r: &egui::Rect| r.center().distance(list.center());
            d(a).total_cmp(&d(b))
        })
        .expect(item);
    click_at(h, at.center());
    settle(h);
}

/// Both clicks of a double-click in one frame.
fn double_click_at(h: &mut Harness<'_, BananaApp>, at: egui::Pos2) {
    h.event(egui::Event::PointerMoved(at));
    h.step();
    for _ in 0..2 {
        for pressed in [true, false] {
            h.input_mut().events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
    h.step();
    settle(h);
}

/// BananaV's own Color Curves, an OFX plug-in, as VEGAS's: in Video FX's
/// Color category with VEGAS's presets; its curves' editor in the FX
/// window (the channel, the other channels shown, the bar of what the
/// curves make of a ramp); a point added with a double-click, dragged and
/// deleted from its menu; the red channel's curve alone; Calculate
/// reading the picture.
#[test]
#[ignore = "renders images for visual review"]
fn captura_color_curves() {
    use bv_core::curve::curve_at;
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Color").click();
    settle(&mut h);
    h.get_all_by_label("Color Curves")
        .next()
        .expect("Color Curves")
        .click();
    settle(&mut h);
    for preset in [
        "Warm Colors",
        "Cool Colors",
        "Reduce Contrast",
        "Increase Contrast",
        "Brighten Brights",
        "Highlight Shadows",
        "Yellow Tint",
        "Night",
        "Infrared",
        "Negative Shadows",
        "Blues and Yellows",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "curvas_video_fx");
    // Warm Colors on the event: red raised, green and blue a little down.
    add_preset(&mut h, "Warm Colors");
    let warm = curves_value(&h);
    assert!(curve_at(&warm[0], 0.5) > 0.55 && curve_at(&warm[2], 1.0) < 0.95);
    snap(&mut h, "curvas_quentes");
    pick_in_list(&mut h, "Warm Colors", "Infrared");
    snap(&mut h, "curvas_infravermelho");
    pick_in_list(&mut h, "Infrared", "(Default)");
    // The box: a point added on the curve (all the channels together),
    // dragged up, then deleted from its menu.
    let frame = h.get_by_label("Curves").rect();
    let area = frame.shrink(11.0);
    let at = |x: f32, y: f32| {
        egui::pos2(
            area.left() + x * area.width(),
            area.bottom() - y * area.height(),
        )
    };
    eprintln!("frame {frame:?}");
    double_click_at(&mut h, at(0.5, 0.5));
    snap(&mut h, "debug_duplo");
    assert!(
        curves_value(&h).iter().all(|c| c.len() == 3),
        "a point added: {:?}",
        curves_value(&h)
    );
    drag(
        &mut h,
        at(0.5, 0.5),
        at(0.5, 0.75),
        Some("curvas_ponto_arrastado"),
    );
    let up = curves_value(&h);
    assert!(
        up.iter().all(|c| (curve_at(c, 0.5) - 0.75).abs() < 0.03),
        "{up:?}"
    );
    right_click_at(&mut h, at(0.5, 0.75));
    settle(&mut h);
    snap(&mut h, "curvas_menu_ponto");
    h.get_by_label("Delete Point").click();
    settle(&mut h);
    assert!(curves_value(&h).iter().all(|c| c.len() == 2), "deleted");
    // Red alone: its end brought down; the others stay.
    pick_in_list(&mut h, "RGB", "Red");
    drag(&mut h, at(1.0, 1.0), at(1.0, 0.6), None);
    let red = curves_value(&h);
    assert!(
        curve_at(&red[0], 1.0) < 0.7 && curve_at(&red[1], 1.0) > 0.99,
        "{red:?}"
    );
    snap(&mut h, "curvas_vermelho");
    // Calculate (Color Levels) from the picture at the cursor.
    let before = curves_value(&h);
    h.get_by_label("Calculate").click();
    settle(&mut h);
    let after = curves_value(&h);
    assert_ne!(before, after, "Calculate read the picture");
    snap(&mut h, "curvas_calculadas");
}

/// A left click at `at` with `modifiers` held (Ctrl, Shift).
fn click_with(h: &mut Harness<'_, BananaApp>, at: egui::Pos2, modifiers: egui::Modifiers) {
    h.event(egui::Event::ModifiersChanged(modifiers));
    h.event(egui::Event::PointerMoved(at));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        });
        h.step();
    }
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    settle(h);
}

/// The control labelled `label` nearest `to` (a menu's item, not a button
/// of the same name).
fn nearest(h: &Harness<'_, BananaApp>, label: &str, to: egui::Pos2) -> egui::Rect {
    h.get_all_by_label(label)
        .map(|n| n.rect())
        .min_by(|a, b| a.center().distance(to).total_cmp(&b.center().distance(to)))
        .unwrap_or_else(|| panic!("no {label}"))
}

/// Color Curves as VEGAS's, more: in RGB the curve green and blue share is
/// the one edited and red keeps its own (Warm Colors stays warm); points
/// picked with a click and Ctrl, or a run with Shift, move and go
/// together; the box's menu creates a point and makes the curves like one;
/// the window's Animate opens the keyframe controller with the effect's
/// row alone.
#[test]
#[ignore = "renders images for visual review"]
fn captura_color_curves_rgb_menus_animate() {
    use bv_core::curve::curve_at;
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Color").click();
    settle(&mut h);
    h.get_all_by_label("Color Curves")
        .next()
        .expect("Color Curves")
        .click();
    settle(&mut h);
    add_preset(&mut h, "Warm Colors");
    // No clock of its own: the window's Animate, as VEGAS's.
    assert!(
        h.query_all_by_label("Animate").next().is_some(),
        "the Animate button"
    );
    snap(&mut h, "curvas_animate");
    let frame = h.get_by_label("Curves").rect();
    let area = frame.shrink(11.0);
    let at = |x: f64, y: f64| {
        egui::pos2(
            area.left() + x as f32 * area.width(),
            area.bottom() - y as f32 * area.height(),
        )
    };
    // RGB: green and blue's end dragged down; red keeps its own.
    let warm = curves_value(&h);
    let end = warm[1].last().expect("an end").y;
    drag(&mut h, at(1.0, end), at(1.0, 0.8), None);
    let c = curves_value(&h);
    assert_eq!(c[0], warm[0], "red kept");
    assert!(
        (curve_at(&c[1], 1.0) - 0.8).abs() < 0.02 && c[1] == c[2],
        "{c:?}"
    );
    snap(&mut h, "curvas_rgb_quentes");
    // Two points added on their curve; a click and a Ctrl+click pick both,
    // and a drag takes both up.
    double_click_at(&mut h, at(0.3, 0.1));
    double_click_at(&mut h, at(0.7, 0.1));
    let g = curves_value(&h)[1].clone();
    assert_eq!(g.len(), 4, "{g:?}");
    let (p1, p2) = (at(g[1].x, g[1].y), at(g[2].x, g[2].y));
    click_at(&mut h, p1);
    settle(&mut h);
    click_with(&mut h, p2, egui::Modifiers::COMMAND);
    drag(
        &mut h,
        p1,
        p1 - egui::vec2(0.0, 25.0),
        Some("curvas_dois_pontos"),
    );
    let up = curves_value(&h)[1].clone();
    assert!(
        up[1].y > g[1].y + 0.05 && up[2].y > g[2].y + 0.05,
        "{g:?} {up:?}"
    );
    assert_eq!(curves_value(&h)[0], warm[0], "red still kept");
    // A run picked with Shift, deleted with Delete.
    let (q1, q2) = (at(up[1].x, up[1].y), at(up[2].x, up[2].y));
    click_at(&mut h, q2);
    settle(&mut h);
    click_with(&mut h, q1, egui::Modifiers::SHIFT);
    snap(&mut h, "curvas_corrente");
    press(&mut h, egui::Key::Delete, egui::Modifiers::NONE);
    assert_eq!(curves_value(&h)[1].len(), 2, "the run deleted");
    // The box's menu: Create Point and Set All to….
    let spot = at(0.15, 0.85);
    right_click_at(&mut h, spot);
    settle(&mut h);
    for item in [
        "Create Point",
        "Set All to Red",
        "Set All to Green",
        "Set All to Blue",
    ] {
        assert!(h.query_all_by_label(item).next().is_some(), "no {item}");
    }
    snap(&mut h, "curvas_menu_caixa");
    let item = nearest(&h, "Set All to Red", spot).center();
    click_at(&mut h, item);
    settle(&mut h);
    let c = curves_value(&h);
    assert!(c[1] == c[0] && c[2] == c[0], "{c:?}");
    // Create Point: where it was clicked, the curve through it.
    let spot = at(0.25, 0.75);
    right_click_at(&mut h, spot);
    settle(&mut h);
    let item = nearest(&h, "Create Point", spot).center();
    click_at(&mut h, item);
    settle(&mut h);
    let c = curves_value(&h);
    assert!(
        c.iter().all(|c| (curve_at(c, 0.25) - 0.75).abs() < 0.02),
        "{c:?}"
    );
    snap(&mut h, "curvas_ponto_criado");
    // Animate: a keyframe at the event's start, the controller with the
    // effect's row alone.
    h.get_all_by_label("Animate")
        .last()
        .expect("the Animate button")
        .click();
    settle(&mut h);
    let keys = |h: &Harness<'_, BananaApp>| {
        h.state()
            .clip_for_review()
            .and_then(|c| {
                let fx = c.effects.iter().find_map(|e| e.plugin.clone())?;
                Some(fx.params.get("curves")?.keys.len())
            })
            .unwrap_or(0)
    };
    assert_eq!(keys(&h), 1, "a keyframe at the start");
    assert!(
        h.query_all_by_label("Sync Cursor").next().is_some(),
        "the keyframe controller"
    );
    // A second later in the event, the curves changed: a second keyframe
    // there.
    let start = h
        .state()
        .clip_for_review()
        .map_or(0.0, |c| c.start.as_seconds_f64());
    h.state_mut().seek_seconds(start + 1.0);
    settle(&mut h);
    let now = curves_value(&h);
    let last = now[0].last().expect("an end").y;
    drag(&mut h, at(1.0, last), at(1.0, 0.7), None);
    assert_eq!(keys(&h), 2, "a keyframe at the cursor");
    snap(&mut h, "curvas_animadas");
}

/// The value of a parameter of the selected event's Color Corrector.
fn corrector_value(h: &Harness<'_, BananaApp>, name: &str) -> Vec<f64> {
    match ofx_value(h, name) {
        Some(bv_timeline::PluginValue::Num(v)) => v,
        None => vec![0.0, 0.0],
        other => panic!("{name}: {other:?}"),
    }
}

/// BananaV's own Color Corrector, as VEGAS's: in Video FX's Color category
/// with VEGAS's presets; in the FX window its three wheels (drawn by the
/// plug-in), Saturation, Gamma, Gain and Offset; a preset's wheel; a click
/// on a wheel sets it; an eyedropper takes a color on the preview.
#[test]
#[ignore = "renders images for visual review"]
fn captura_color_corrector() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Color").click();
    settle(&mut h);
    h.get_all_by_label("Color Corrector")
        .next()
        .expect("Color Corrector")
        .click();
    settle(&mut h);
    for preset in [
        "Red Midtones",
        "Green Midtones",
        "Blue Midtones",
        "Red Highlight",
        "Green Highlight",
        "Blue Highlight",
        "Remove Yellow Cast",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "corretor_video_fx");
    add_preset(&mut h, "Red Midtones");
    assert_eq!(corrector_value(&h, "mid"), [103.5, 0.5]);
    for label in [
        "Low:",
        "Mid:",
        "High:",
        "Saturation:",
        "Gamma:",
        "Gain:",
        "Offset:",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    // The wheel's numbers as VEGAS writes them.
    assert!(
        h.query_all_by_value("103.5, 0.500").next().is_some(),
        "Mid's numbers"
    );
    snap(&mut h, "corretor_vermelho");
    pick_in_list(&mut h, "Red Midtones", "Remove Yellow Cast");
    assert_eq!(corrector_value(&h, "high"), [347.1, 0.3]);
    snap(&mut h, "corretor_amarelado");
    // A click on Low's wheel, straight up from its middle and half way
    // out: 90°, half the magnitude.
    // The wheel (its numbers' box has its name too).
    let wheel = h
        .get_all_by_label("Low")
        .find(|n| n.accesskit_node().role() != egui::accesskit::Role::TextInput)
        .expect("Low's wheel")
        .rect();
    let centre = egui::pos2(wheel.left() + 165.0, wheel.bottom() - 68.0);
    click_at(&mut h, centre - egui::vec2(0.0, 30.0));
    settle(&mut h);
    let low = corrector_value(&h, "low");
    assert!(
        (low[0] - 90.0).abs() < 1.0 && (low[1] - 0.5).abs() < 0.02,
        "{low:?}"
    );
    snap(&mut h, "corretor_roda_clicada");
    // Its complement's eyedropper (below the wheel's right), then a click
    // on the preview: Low takes the complement of the color there.
    click_at(
        &mut h,
        egui::pos2(wheel.left() + 247.0, wheel.bottom() - 23.0),
    );
    settle(&mut h);
    snap(&mut h, "corretor_conta_gotas");
    let spot = h.state().preview_spot_for_review();
    click_at(&mut h, spot);
    settle(&mut h);
    let picked = corrector_value(&h, "low");
    assert_ne!(picked, low, "the eyedropper picked a color");
    snap(&mut h, "corretor_conta_gotas_usado");
    // Two clicks near its middle: the first moves it there, the second
    // (a double click) puts it back in the middle, as VEGAS's.
    let near_middle = centre + egui::vec2(10.0, 0.0);
    click_at(&mut h, near_middle);
    let once = corrector_value(&h, "low");
    assert!(once[1] > 0.1, "{once:?}");
    click_at(&mut h, near_middle);
    settle(&mut h);
    assert_eq!(corrector_value(&h, "low"), [0.0, 0.0]);
    snap(&mut h, "corretor_roda_centrada");
}

/// BananaV's own Cookie Cutter and Layer Dimensionality, as VEGAS's: in
/// Video FX with VEGAS's presets; Cookie Cutter's color as VEGAS draws an
/// OFX color (its swatch, its HSV numbers and, open, the picker), its
/// shapes and its center's handle on the preview; Layer Dimensionality's
/// sections closed, then open, the numbers its types turn off greyed out,
/// its light's box.
#[test]
#[ignore = "renders images for visual review"]
fn captura_cookie_cutter_e_layer_dimensionality() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Utility").click();
    settle(&mut h);
    h.get_all_by_label("Cookie Cutter")
        .next()
        .expect("Cookie Cutter")
        .click();
    settle(&mut h);
    for preset in [
        "Circle, Center",
        "Square, Center, White Border",
        "Diamond, Center, Blurred",
        "Circle Cut Out, Center, Blurred",
        "Two Rectangles",
        "Triangle Cut Out, Blue Border",
        "Square, Top Left, Border",
        "Diamond Cut Out, Bottom Right",
        "Top Left Portrait",
        "Yellow Glowing Portrait",
        "Pop-up, Bottom Right",
        "Picture-In-Picture",
        "Grate",
        "TicTacToe",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "cookie_video_fx");
    add_preset(&mut h, "Square, Center, White Border");
    assert_eq!(corrector_value(&h, "border"), [0.1]);
    // The color as VEGAS writes it: white is 0, 0.0, 1.0 in HSV.
    assert!(
        h.query_all_by_value("0, 0.0, 1.0").next().is_some(),
        "the color's numbers"
    );
    snap(&mut h, "cookie_quadrado");
    // Its picker, opened from the swatch.
    let swatch = h
        .get_all_by_label("Color")
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::ColorWell)
        .expect("the swatch")
        .rect();
    click_at(&mut h, swatch.center());
    settle(&mut h);
    for item in ["HSV", "(SxV) H"] {
        assert!(h.query_all_by_value(item).next().is_some(), "no {item}");
    }
    snap(&mut h, "cookie_cor");
    // The plane's middle: half saturated, half bright.
    let plane = egui::Rect::from_min_size(
        egui::pos2(
            swatch.left() + ((145.0 + 10.0 + 145.0 - 136.0) / 2.0 - 6.0) + 6.0,
            swatch.bottom() + 6.0 + 22.0 + 8.0,
        ),
        egui::vec2(96.0, 100.0),
    );
    click_at(&mut h, plane.center());
    settle(&mut h);
    let c = corrector_value(&h, "color");
    assert!(
        (c[0] - 0.5).abs() < 0.05 && (c[1] - 0.25).abs() < 0.05,
        "{c:?}"
    );
    snap(&mut h, "cookie_cor_escolhida");
    // The shapes' list: a triangle.
    pick_in_list(&mut h, "Square", "Triangle");
    assert_eq!(corrector_value(&h, "shape"), [6.0]);
    snap(&mut h, "cookie_triangulo");
    // Its center's handle, dragged on the preview.
    let middle = h.state().preview_spot_for_review();
    let before = corrector_value(&h, "center");
    drag(
        &mut h,
        middle,
        middle + egui::vec2(60.0, -30.0),
        Some("cookie_centro"),
    );
    let after = corrector_value(&h, "center");
    assert!(
        after[0] > before[0] && after[1] > before[1],
        "{before:?} {after:?}"
    );

    // Layer Dimensionality after it, its sections closed.
    h.get_by_label("Close window").click();
    settle(&mut h);
    h.get_by_label("Creative").click();
    settle(&mut h);
    h.get_all_by_label("Layer Dimensionality")
        .next()
        .expect("Layer Dimensionality")
        .click();
    settle(&mut h);
    add_preset(&mut h, "(Default)");
    for section in ["Shadow", "Glow", "Emboss"] {
        assert!(
            h.query_all_by_label(section).next().is_some(),
            "no {section}"
        );
    }
    assert!(
        h.query_all_by_label("Height:").next().is_none(),
        "the sections start closed"
    );
    snap(&mut h, "layer_fechado");
    // Shadow open: Type None, its numbers greyed out.
    h.get_by_label("Shadow").click();
    settle(&mut h);
    snap(&mut h, "layer_sombra_desligada");
    pick_in_list(&mut h, "None", "Drop");
    assert_eq!(corrector_value(&h, "shadowType"), [1.0]);
    snap(&mut h, "layer_sombra");
    // Everything open, the light's box among it (Video FX has a Glow of
    // its own: the section is the one under Shadow).
    let shadow = h.get_by_label("Shadow").rect().center() + egui::vec2(0.0, 120.0);
    let glow = nearest(&h, "Glow", shadow).center();
    click_at(&mut h, glow);
    settle(&mut h);
    let emboss = nearest(&h, "Emboss", shadow).center();
    click_at(&mut h, emboss);
    settle(&mut h);
    // The light's triangle, left of its name.
    let light = nearest(&h, "Light Location:", shadow);
    click_at(&mut h, egui::pos2(light.left() - 10.0, light.center().y));
    settle(&mut h);
    assert!(
        h.query_all_by_label("Amount:").next().is_some(),
        "Glow open"
    );
    snap(&mut h, "layer_aberto");
    // An inner shadow and a glow hugging the edge (VEGAS's types).
    pick_in_list(&mut h, "Drop", "Inner");
    assert_eq!(corrector_value(&h, "shadowType"), [2.0]);
    pick_in_list(&mut h, "None", "Edge");
    assert_eq!(corrector_value(&h, "glowType"), [3.0]);
    snap(&mut h, "layer_interna_borda");
    // One of VEGAS's presets from the list: its light's cross on the
    // preview, up and left.
    pick_in_list(&mut h, "(Custom)", "Inner Shadow Top Left Light");
    assert_eq!(corrector_value(&h, "lightLocation"), [-1.636, 1.989, 3.0]);
    snap(&mut h, "layer_predefinicao");
}

/// BananaV's own Pixelate, as VEGAS's: in Video FX's Creative with
/// VEGAS's presets, then Large added (its two numbers and the preview in
/// blocks) and Medium Horizontal picked from the FX window's list.
#[test]
#[ignore = "renders images for visual review"]
fn captura_pixelate() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Creative").click();
    settle(&mut h);
    assert_eq!(
        h.query_all_by_label("Pixelate").count(),
        1,
        "one Pixelate: BananaV's plug-in"
    );
    h.get_by_label("Pixelate").click();
    settle(&mut h);
    for preset in [
        "Small",
        "Medium",
        "Large",
        "Medium Horizontal",
        "Medium Vertical",
    ] {
        assert!(h.query_all_by_label(preset).next().is_some(), "no {preset}");
    }
    snap(&mut h, "pixelate_video_fx");
    add_preset(&mut h, "Large");
    assert_eq!(corrector_value(&h, "horizontal"), [0.75]);
    assert_eq!(corrector_value(&h, "vertical"), [0.75]);
    for label in ["Horizontal pixelization:", "Vertical pixelization:"] {
        assert!(h.query_all_by_label(label).next().is_some(), "no {label}");
    }
    snap(&mut h, "pixelate_large");
    pick_in_list(&mut h, "Large", "Medium Horizontal");
    assert_eq!(corrector_value(&h, "horizontal"), [0.5]);
    assert_eq!(corrector_value(&h, "vertical"), [0.0]);
    snap(&mut h, "pixelate_medio_horizontal");
}

/// Presses and lets go of a key where the keyboard is.
fn press(h: &mut Harness<'_, BananaApp>, key: egui::Key, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        h.event(egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        });
    }
    settle(h);
}

/// Track Motion: the demo's video track smaller and turned, with a 2D
/// shadow and glow, the window open on the shadow's row.
#[test]
#[ignore = "renders images for visual review"]
fn captura_track_motion() {
    let mut app = new_app();
    app.load_demo();
    app.track_motion_for_review();
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
    settle(&mut h);
    snap(&mut h, "track_motion");
}

/// 3D Track Motion: a box turned in space, in one pane and in four.
#[test]
#[ignore = "renders images for visual review"]
fn captura_track_motion_3d() {
    for (layout, name) in [(0, "track_motion_3d"), (5, "track_motion_3d_quatro")] {
        let mut app = new_app();
        app.load_demo();
        app.track_motion_3d_for_review(layout);
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
        settle(&mut h);
        snap(&mut h, name);
    }
}

/// Custom compositing: a ramp on a parent track moving its child's bars
/// through a Displacement Map, the Compositors window and the compositor's
/// settings.
#[test]
#[ignore = "renders images for visual review"]
fn captura_compositores() {
    let mut app = new_app();
    app.load_demo();
    app.compositors_for_review();
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
    settle(&mut h);
    snap(&mut h, "compositores");
    // The Compositors window alone, Displacement Map's presets.
    h.state_mut().close_compositor_window_for_review();
    settle(&mut h);
    h.get_by_label("Displacement Map").click();
    settle(&mut h);
    snap(&mut h, "compositores_janela");
}

/// Multicamera editing: the demo's video and a second camera made into a
/// multicamera track, the Video Preview's tiles (the active take outlined).
#[test]
#[ignore = "renders images for visual review"]
fn captura_multicamera() {
    let mut app = new_app();
    app.load_demo();
    app.multicam_for_review();
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
    settle(&mut h);
    snap(&mut h, "multicamera");
}

/// Takes: the first event with a second take (the active one, named), its
/// take information on the timeline and the Take Chooser.
#[test]
#[ignore = "renders images for visual review"]
fn captura_takes() {
    let mut app = new_app();
    app.load_demo();
    app.takes_for_review();
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
    settle(&mut h);
    snap(&mut h, "takes");
}

/// Events' own envelopes: the gradient's velocity (half speed, backward,
/// frozen at 8 s) and its crossfade's progress; the checkerboard reversed,
/// its arrow on it.
#[test]
#[ignore = "renders images for visual review"]
fn captura_envelopes_de_evento() {
    let mut app = new_app();
    app.load_demo();
    app.event_envelopes_for_review();
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
    settle(&mut h);
    snap(&mut h, "envelopes_de_evento");
}

/// The Loudness Meters: empty, then a program a little loud for EBU R 128
/// (its true peaks over −1 dBTP), then ATSC A/85 on the EBU +18 scale.
#[test]
#[ignore = "renders images for visual review"]
fn captura_loudness() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    h.state_mut().show_tab_named("Loudness Meters");
    settle(&mut h);
    snap(&mut h, "loudness_vazio");
    h.state_mut()
        .loudness_for_review([-21.4, -22.1, -22.6, 7.3, -0.4], false, false);
    settle(&mut h);
    snap(&mut h, "loudness_ebu");
    h.state_mut()
        .loudness_for_review([-25.0, -24.6, -24.2, 11.0, -3.1], true, true);
    settle(&mut h);
    snap(&mut h, "loudness_atsc");
}

/// Titles & Text, BananaV's own text generator: Media Generator's Titles
/// and Text category with its presets; Insert › Text Media puts one at the
/// cursor with its Video Media Generators window open (the text, the font,
/// its size…), and the preview shows it with its box and anchor.
#[test]
#[ignore = "renders images for visual review"]
fn captura_titulos() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    h.state_mut().show_tab_named("Media Generators");
    settle(&mut h);
    h.get_by_label("Titles and Text").click();
    settle(&mut h);
    h.get_all_by_label("Titles & Text")
        .next()
        .expect("Titles & Text in the list")
        .click();
    settle(&mut h);
    for preset in [
        "Default",
        "Lower Third",
        "Large Title",
        "Outlined",
        "Soft Shadow",
    ] {
        assert!(
            h.query_all_by_label(preset).next().is_some(),
            "no preset {preset}"
        );
    }
    let cell = h.get_by_label("Lower Third").rect();
    h.event(egui::Event::PointerMoved(cell.center()));
    snap(&mut h, "titulos_geradores");
    // A new video track (Insert › Video Track: at the top, selected), then
    // Insert › Text Media… on it at the cursor.
    h.get_by_label("Insert").click();
    settle(&mut h);
    h.get_by_label_contains("Video Track").click();
    settle(&mut h);
    h.get_by_label("Insert").click();
    settle(&mut h);
    h.get_by_label_contains("Text Media…").click();
    settle(&mut h);
    assert!(
        h.state()
            .media_for_review()
            .iter()
            .any(|(name, _)| name == "Titles & Text"),
        "the text media"
    );
    for label in [
        "Text:",
        "Font:",
        "Size:",
        "Bold:",
        "Italic:",
        "Justification:",
        "Text Color:",
        "Scale:",
        "Location:",
        "Anchor Point:",
        "Advanced",
        "Outline",
        "Shadow",
        "Transfer Text Properties",
    ] {
        assert!(
            h.query_all_by_label(label).next().is_some(),
            "no control for {label}"
        );
    }
    snap(&mut h, "titulos_janela");
    // Two seconds into it: the text over the picture, its box and anchor.
    h.state_mut().seek_seconds(6.5);
    h.state_mut().show_tab_named("Video Preview");
    snap(&mut h, "titulos_previa");
}

/// OFX plug-ins: an effect picked in the Plug-In Chooser (in its own
/// folder), its settings in the FX window as the plug-in describes them,
/// edits that reach the project and a button the plug-in answers; Video
/// FX's Third Party category and the Plug-In Manager; a transition from
/// the Transitions window, its settings in Properties and its picture in
/// the preview; media a generator plug-in draws, from Media Generator,
/// with its settings in the Video Media Generators window.
#[test]
#[ignore = "renders images for visual review"]
fn captura_plugin_ofx() {
    use bv_timeline::PluginValue;
    ofx_test_plugins();
    let mut app = new_app();
    app.load_demo();
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
    // The demo's selected event has a Vignette: its FX window opens.
    h.state_mut().open_clip_window("fx");
    settle(&mut h);
    h.get_by_label("Plug-In Chain…").click();
    settle(&mut h);
    h.get_by_label("BananaV Tests").click();
    settle(&mut h);
    snap(&mut h, "ofx_seletor");
    for label in ["Invert (test)", "Add", "OK"] {
        h.get_by_label(label).click();
        settle(&mut h);
    }
    snap(&mut h, "ofx_fx_janela");
    for label in [
        "Amount:",
        "Channels:",
        "Tint:",
        "Center:",
        "Steps:",
        "Note:",
        "Advanced",
        "Invert Alpha:",
        "Reset",
        // The curve's editor.
        "Curve",
        "About",
        "?",
    ] {
        assert!(
            h.query_all_by_label(label).next().is_some(),
            "no control for {label}"
        );
    }
    // The plug-in's on-screen control: a cross at its center, in the
    // middle of the preview; dragged, it moves the center (right and up:
    // canonical coordinates count up from the bottom).
    let middle = h.state().preview_spot_for_review();
    drag(
        &mut h,
        middle,
        middle + egui::vec2(60.0, -30.0),
        Some("ofx_overlay"),
    );
    let Some(PluginValue::Num(c)) = ofx_value(&h, "center") else {
        panic!("the center moved");
    };
    assert!(c[0] > 1000.0 && c[1] > 560.0, "{c:?}");
    snap(&mut h, "ofx_overlay_arrastado");
    // Center animated (its third clock: Amount, Tint, Center): a keyframe
    // at the event's start, as VEGAS makes it; at 6 s the cross dragged
    // again makes a second one (the plug-in sets the value, the host keys
    // it), and the keyframe controller shows both.
    h.get_all_by_label("Animate")
        .nth(2)
        .expect("Center's toggle")
        .click();
    settle(&mut h);
    h.state_mut().seek_seconds(6.0);
    settle(&mut h);
    let cross = middle + egui::vec2(60.0, -30.0);
    drag(&mut h, cross, middle + egui::vec2(-40.0, 20.0), None);
    let keys = h
        .state()
        .clip_for_review()
        .and_then(|c| {
            let fx = c.effects.iter().find_map(|e| e.plugin.clone())?;
            Some(fx.params.get("center")?.keys.len())
        })
        .unwrap_or(0);
    assert_eq!(keys, 2, "two keyframes of the center");
    snap(&mut h, "ofx_quadros_chave");
    // Typing in a text parameter changes the event's effect.
    let text_input = |n: &kittest::AccessKitNode<'_>| n.role() == egui::accesskit::Role::TextInput;
    let note = h
        .query_all_by_value("hello")
        .find(|n| text_input(&n.accesskit_node()))
        .expect("the note");
    note.focus();
    note.type_text("!");
    settle(&mut h);
    assert_eq!(
        ofx_value(&h, "note"),
        Some(PluginValue::Text("hello!".into()))
    );
    // The plug-in's own button: the plug-in sets Amount and Note in turn.
    h.get_by_label("Reset").click();
    settle(&mut h);
    assert_eq!(
        ofx_value(&h, "note"),
        Some(PluginValue::Text("reset".into()))
    );
    assert_eq!(ofx_value(&h, "amount"), Some(PluginValue::Num(vec![1.0])));
    assert!(
        h.query_all_by_value("reset")
            .any(|n| text_input(&n.accesskit_node())),
        "the note shown"
    );
    snap(&mut h, "ofx_fx_reset");
    h.get_by_label("Close window").click();
    settle(&mut h);

    // Video FX: the plug-in in the Third Party category, its preset drawn
    // by the plug-in itself.
    h.state_mut().show_tab_named("Video FX");
    settle(&mut h);
    h.get_by_label("Third Party").click();
    settle(&mut h);
    h.get_by_label("Invert (test)").click();
    settle(&mut h);
    let at = h.get_by_label("(Default)").rect().center();
    h.event(egui::Event::PointerMoved(at));
    snap(&mut h, "ofx_video_fx");

    // The Plug-In Manager: the plug-ins found and the folders searched.
    h.state_mut().show_tab_named("Plug-In Manager");
    settle(&mut h);
    h.get_by_label("BananaV Tests").click();
    snap(&mut h, "ofx_gerenciador");

    // Transitions: the plug-in's, its sample drawn by the plug-in; its +
    // gives it to the selected event (the gradient, over the color bars
    // from 4 to 5 s).
    h.state_mut().show_tab_named("Transitions");
    settle(&mut h);
    h.get_by_label("Third Party").click();
    settle(&mut h);
    h.get_by_label("Wipe (test)").click();
    settle(&mut h);
    let cell = h.get_by_label("(Default)").rect();
    h.event(egui::Event::PointerMoved(cell.center()));
    snap(&mut h, "ofx_transicoes");
    add_preset(&mut h, "(Default)");
    let clip = h.state().clip_for_review().expect("the selected event");
    assert_eq!(
        clip.transition_fx.map(|fx| fx.id).as_deref(),
        Some("org.bananav.test.Wipe")
    );
    // Its settings in Properties (its progress is the host's: not shown).
    h.state_mut().show_tab_named("Properties");
    settle(&mut h);
    assert!(h.query_all_by_label("Softness").next().is_some());
    assert!(h.query_all_by_label("Progress").next().is_none());
    snap(&mut h, "ofx_transicao_propriedades");
    // Halfway through the crossfade (the cursor at 4.5 s): half and half.
    h.state_mut().show_tab_named("Video Preview");
    snap(&mut h, "ofx_transicao_previa");

    // Media Generator: the plug-in's media, added at the cursor and its
    // window open.
    h.state_mut().show_tab_named("Media Generators");
    settle(&mut h);
    h.get_by_label("Third Party").click();
    settle(&mut h);
    h.get_by_label("Gradient (test)").click();
    settle(&mut h);
    let cell = h.get_by_label("(Default)").rect();
    h.event(egui::Event::PointerMoved(cell.center()));
    snap(&mut h, "ofx_geradores");
    add_preset(&mut h, "(Default)");
    assert!(
        h.state()
            .media_for_review()
            .iter()
            .any(|(name, _)| name == "Gradient (test)"),
        "the generated media"
    );
    for label in ["Top", "Bottom"] {
        assert!(
            h.query_all_by_label(label).next().is_some(),
            "no control for {label}"
        );
    }
    snap(&mut h, "ofx_midia_gerada");
    // At 6 s the plug-in's picture comes in over the gradient (the event
    // starts at the cursor, 4.5 s, after the frame shown there).
    h.state_mut().seek_seconds(6.0);
    snap(&mut h, "ofx_midia_gerada_previa");
}

/// Filter packages: an event's chain saved from its FX window (the Save
/// Plug-In Package dialog), Video FX's Filter Packages category with the
/// package's preset, the Plug-In Chooser's Filter Packages folder (Save
/// The Trimmer with Show Video Frames (the media's frames along its time
/// bar, a part selected) and the Mixing Console with the Preview Bus's
/// channel.
#[test]
#[ignore = "renders images for visual review"]
fn captura_trimmer_quadros_e_canal_preview() {
    let mut app = new_app();
    app.load_demo();
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
    assert!(h.state_mut().trimmer_frames_for_review());
    for _ in 0..30 {
        settle(&mut h);
    }
    snap(&mut h, "trimmer_quadros");
    h.state_mut().show_tab_named("Mixing Console");
    settle(&mut h);
    snap(&mut h, "console_canal_preview");
}

/// Pan/Crop masks (VEGAS 2026's): the demo event with a feathered oval and
/// a rectangle cut out of it, its Pan/Crop window on the Mask row — the
/// masks' tools, the oval picked with its box (as after a double-click on
/// it), its first anchor's tangents and the small toolbar, what the masks
/// cut darkened — and the Video Preview showing the event masked.
#[test]
#[ignore = "renders images for visual review"]
fn captura_mascaras_pan_crop() {
    let mut app = new_app();
    app.load_demo();
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
    assert!(h.state_mut().pan_crop_masks_for_review());
    settle(&mut h);
    snap(&mut h, "pancrop_mascaras");
}

/// An event's Properties (its shortcut menu's Properties…), as VEGAS's:
/// a video file's event on its Video Event, Media and General tabs.
#[test]
#[ignore = "renders images for visual review"]
fn captura_propriedades_do_evento() {
    let video = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-media/generated/dois_streams_de_audio.mp4");
    assert!(
        video.exists(),
        "generate it with: cargo test -p bv-media --test streams -- --ignored"
    );
    let mut app = new_app();
    assert!(app.import_and_place(&video), "import failed");
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
    for (page, name) in [
        ("event", "propriedades_evento_video"),
        ("media", "propriedades_evento_midia"),
        ("general", "propriedades_evento_geral"),
    ] {
        assert!(h.state_mut().event_properties_for_review(page));
        settle(&mut h);
        snap(&mut h, name);
    }
}

/// Pan/Crop masks picked as VEGAS's: the oval's box shown (a double-click
/// on it), hidden by a click off the paths; a rectangle dragged there picks
/// the anchors inside it (their tangents shown).
#[test]
#[ignore = "renders images for visual review"]
fn captura_mascaras_selecao() {
    let mut app = new_app();
    app.load_demo();
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
    assert!(h.state_mut().pan_crop_masks_for_review());
    settle(&mut h);
    let spot = |h: &Harness<'_, BananaApp>, x: f32, y: f32| {
        h.state()
            .pan_crop_media_spot_for_review(x, y)
            .expect("the workspace drawn")
    };
    let off = spot(&h, 0.04, 0.04);
    click_at(&mut h, off);
    settle(&mut h);
    snap(&mut h, "pancrop_mascaras_sem_caixa");
    let (from, to) = (spot(&h, 0.04, 0.04), spot(&h, 0.47, 0.55));
    drag(&mut h, from, to, Some("pancrop_mascaras_retangulo"));
    settle(&mut h);
    snap(&mut h, "pancrop_mascaras_pontos_escolhidos");
}

/// The Video Event FX window's chain with the Pan/Crop after the demo
/// event's Vignette (moved by its button's menu; it is dragged too): the
/// Vignette works on the media's own picture, before the framing.
#[test]
#[ignore = "renders images for visual review"]
fn captura_pan_crop_na_cadeia() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().open_clip_window("fx");
    settle(&mut h);
    h.get_by_label("Pan/Crop").click();
    settle(&mut h);
    h.get_by_label("Pan/Crop").click_secondary();
    settle(&mut h);
    snap(&mut h, "cadeia_pancrop_menu");
    h.get_by_label("Move Right").click();
    settle(&mut h);
    snap(&mut h, "cadeia_pancrop_depois");
    // A video track's chain: its Composite button between a Sepia (on the
    // track's own picture) and an Invert (on its compositing group's).
    use bv_timeline::{Effect, EffectKind};
    h.state_mut().track_chain_for_review(
        vec![
            Effect::new(EffectKind::Sepia),
            Effect::new(EffectKind::Invert),
        ],
        Some(1),
    );
    settle(&mut h);
    h.get_by_label("Composite").hover();
    settle(&mut h);
    snap(&mut h, "cadeia_composite");
}

/// A Video Event FX window with a long chain: sized to its page, the
/// window takes the whole chain; made narrower, the chain's buttons stop
/// before the buttons at the row's end and scroll between two scroll
/// buttons (VEGAS 23's): at the start, a button on, and at the end.
#[test]
#[ignore = "renders images for visual review"]
fn captura_cadeia_longa() {
    use bv_timeline::{Effect, PluginFx};
    let mut app = new_app();
    app.load_demo();
    let own = |id: &str, name: &str| {
        Effect::plugin(PluginFx {
            id: format!("org.bananav.{id}"),
            version: [1, 0],
            name: name.into(),
            params: Default::default(),
        })
    };
    app.clip_chain_for_review(vec![
        own("BrightnessContrast", "Brightness and Contrast"),
        own("BlackAndWhite", "Black and White"),
        own("ColorCorrectorSecondary", "Color Corrector (Secondary)"),
        own("SaturationAdjust", "Saturation Adjust"),
        own("WhiteBalance", "White Balance"),
        own("UnsharpMask", "Unsharp Mask"),
        own("AddNoise", "Add Noise"),
    ]);
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
    snap(&mut h, "cadeia_longa_inteira");
    let ctx = h.ctx.clone();
    h.state_mut()
        .fx_window_size_for_review(&ctx, egui::vec2(620.0, 460.0));
    settle(&mut h);
    snap(&mut h, "cadeia_longa");
    let scroll = |h: &mut Harness<'_, BananaApp>| {
        h.get_by_label("Scroll Right").click();
        settle(h);
    };
    scroll(&mut h);
    snap(&mut h, "cadeia_longa_rolada");
    let at_end = |h: &Harness<'_, BananaApp>| {
        h.get_by_label("Scroll Right")
            .accesskit_node()
            .is_disabled()
    };
    for _ in 0..20 {
        if at_end(&h) {
            break;
        }
        scroll(&mut h);
    }
    assert!(at_end(&h), "the chain's end never came");
    snap(&mut h, "cadeia_longa_fim");
}

/// The Video Preview's transport bar with Go to Start and the frame steps
/// shown too, and its Edit Visible Button Set open.
#[test]
#[ignore = "renders images for visual review"]
fn captura_botoes_do_transporte() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().transport_buttons_for_review();
    settle(&mut h);
    snap(&mut h, "transporte_botoes");
}

/// Closed captions (CEA-608): a pop-on caption on CC1 and one painted on
/// at the top on CC3 drawn over the Video Preview (its Overlays' CC1 and
/// CC3), and Edit Details showing the caption commands.
#[test]
#[ignore = "renders images for visual review"]
fn captura_legendas() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().captions_for_review();
    settle(&mut h);
    h.state_mut().show_tab_named("Edit Details");
    settle(&mut h);
    snap(&mut h, "legendas");
}

/// A 360° project (2:1, 360 Output on) of a checkerboard seen through the
/// Video Preview's 360° View, looking up and to the right; then the flat
/// frame again (the 360° View's button).
#[test]
#[ignore = "renders images for visual review"]
fn captura_vista_360() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 960.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            new_app(),
        );
    h.state_mut().view_360_for_review();
    settle(&mut h);
    snap(&mut h, "vista_360");
    h.get_by_label("360° View").click();
    settle(&mut h);
    snap(&mut h, "vista_360_plana");
}

/// The Render window's Channel Mapping (multichannel mapping on) over a
/// project of two busses, bus A mapped.
#[test]
#[ignore = "renders images for visual review"]
fn captura_mapeamento_de_canais() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 960.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            new_app(),
        );
    h.state_mut().channel_mapping_for_review();
    settle(&mut h);
    snap(&mut h, "mapeamento_de_canais");
}

/// Options › Preferences › External Control & Automation with a Generic
/// Control, its Configure Generic Control open on Audio Channels (the
/// first channel's fader and pan mapped).
#[test]
#[ignore = "renders images for visual review"]
fn captura_controle_generico() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 960.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            new_app(),
        );
    h.state_mut().generic_control_for_review();
    settle(&mut h);
    snap(&mut h, "controle_generico");
}

/// Dual Fish Eye Stitching on the demo's first video event (the color bars
/// read as two fish-eyes), in the Video Event FX window.
#[test]
#[ignore = "renders images for visual review"]
fn captura_costura_360() {
    let mut app = new_app();
    app.load_demo();
    assert!(app.stitching_for_review());
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
    settle(&mut h);
    snap(&mut h, "costura_360");
}

/// Video Stabilization on the demo's first video event: before an
/// analysis at its Basic and Expert views (the region of interest on the
/// preview), then its motion analyzed in the Video Event FX window at its
/// Professional and Expert views.
#[test]
#[ignore = "renders images for visual review"]
fn captura_video_stabilization() {
    for (view, name) in [
        (0.0, "estabilizacao_basica"),
        (2.0, "estabilizacao_expert_regiao"),
    ] {
        let mut app = new_app();
        app.load_demo();
        assert!(app.video_stabilization_plain_for_review(view));
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
        settle(&mut h);
        snap(&mut h, name);
    }
    let mut app = new_app();
    app.load_demo();
    assert!(app.video_stabilization_for_review());
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
    settle(&mut h);
    snap(&mut h, "estabilizacao_de_video");
    // Expert: the analysis's work, each component's smoothing, Show
    // Points on the preview.
    assert!(h.state_mut().video_stabilization_expert_for_review());
    settle(&mut h);
    snap(&mut h, "estabilizacao_expert");
}

/// Tools › Video › Motion Tracking on the demo's first video event: a
/// region tracked over its first thirty frames, drawn on the Video Preview
/// with its motion vectors, the window's Motion tracked bar green there.
#[test]
#[ignore = "renders images for visual review"]
fn captura_motion_tracking() {
    let mut app = new_app();
    app.load_demo();
    assert!(app.motion_tracking_for_review());
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
    settle(&mut h);
    snap(&mut h, "rastreamento_de_movimento");
}

/// Options › Preferences › External Control & Automation with a Mackie
/// Control and an Extender, Configure Mackie Control open on the first
/// (Option+F1 mapped, footswitch A cleared).
#[test]
#[ignore = "renders images for visual review"]
fn captura_mackie_control() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 960.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            new_app(),
        );
    h.state_mut().mackie_control_for_review();
    settle(&mut h);
    snap(&mut h, "configurar_mackie");
}

/// Options › Preferences › Display's "Reverse Event Background color"
/// (events in their track's color, waveforms in the darker one) and
/// "Display timeline at bottom of main window" off (the timeline above the
/// docking area).
#[test]
#[ignore = "renders images for visual review"]
fn captura_exibicao_2026() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().display_prefs_for_review();
    settle(&mut h);
    snap(&mut h, "eventos_invertidos_timeline_em_cima");
}

/// FX automation: the demo's audio track with a 3-band EQ whose Bypass,
/// Low and Mid gains have envelopes on the timeline, and the FX Automation
/// Chooser open on it.
#[test]
#[ignore = "renders images for visual review"]
fn captura_automacao_fx() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().fx_automation_for_review();
    settle(&mut h);
    snap(&mut h, "automacao_fx");
}

/// A video track's Expand Track Layers: the demo's events alternating
/// between the A and the B roll, their transitions in the roll between.
#[test]
#[ignore = "renders images for visual review"]
fn captura_camadas_da_trilha() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().track_layers_for_review();
    settle(&mut h);
    snap(&mut h, "camadas_da_trilha");
}

/// VEGAS 2026's timeline OFX envelopes: the demo's video track with an
/// animated Brightness and Contrast, its track keyframes area open (the
/// effect's and Track Motion's lines, the brightness envelope); its middle
/// point a locked Bézier one with its handles, the point's menu, Set to…'s
/// box and the keyframes area's menu.
#[test]
#[ignore = "renders images for visual review"]
fn captura_envelopes_ofx() {
    let mut app = new_app();
    app.load_demo();
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
    assert!(h.state_mut().ofx_envelopes_for_review());
    settle(&mut h);
    snap(&mut h, "envelopes_ofx");
    assert!(h.state_mut().ofx_bezier_for_review());
    settle(&mut h);
    snap(&mut h, "envelopes_ofx_bezier");
    let point = h.state().ofx_point_for_review(1).expect("the middle point");
    right_click_at(&mut h, point);
    settle(&mut h);
    for item in [
        "Add Point",
        "Set to…",
        "Linear Fade",
        "Bézier curve (locked tangents)",
        "Bézier curve (split tangents)",
        "Flip",
        "Thin",
        "Reset",
        "Delete",
    ] {
        assert!(
            h.query_all_by_label_contains(item).next().is_some(),
            "no {item}"
        );
    }
    snap(&mut h, "envelopes_ofx_menu");
    h.get_by_label("Set to…").click();
    settle(&mut h);
    snap(&mut h, "envelopes_ofx_definir");
    h.get_by_label("Cancel").click();
    settle(&mut h);
    // The keyframes area's menu, from a spot under the envelope.
    let area = h.state().ofx_area_for_review().expect("the keyframes area");
    right_click_at(&mut h, area.left_bottom() + egui::vec2(40.0, -3.0));
    settle(&mut h);
    assert!(
        h.query_all_by_label_contains("Brightness").next().is_some(),
        "the envelopes to show or hide"
    );
    snap(&mut h, "envelopes_ofx_area");
}

/// As…, Save LUT…), and the Third Party category's Filter.
#[test]
#[ignore = "renders images for visual review"]
fn captura_pacotes_de_filtros() {
    ofx_test_plugins();
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().open_clip_window("fx");
    settle(&mut h);
    h.get_by_label("Save Plug-In Chain As Filter Package")
        .click();
    settle(&mut h);
    h.event(egui::Event::Text("Warm look".into()));
    snap(&mut h, "pacote_salvar");
    h.get_by_label("OK").click();
    settle(&mut h);
    h.get_by_label("Close window").click();
    settle(&mut h);
    // Video FX: the package and its preset, the sample through the chain.
    h.state_mut().show_tab_named("Video FX");
    h.state_mut()
        .video_fx_category_for_review("Filter Packages");
    settle(&mut h);
    let cell = h
        .get_all_by_label("Warm look")
        .find(|n| n.rect().height() > 60.0)
        .expect("the package's preset")
        .rect();
    h.event(egui::Event::PointerMoved(cell.center()));
    snap(&mut h, "pacotes_video_fx");
    // The Third Party category: its Filter, open.
    h.state_mut().video_fx_category_for_review("Third Party");
    settle(&mut h);
    snap(&mut h, "terceiros_filtro");
    // The Filter's box, right of its label.
    let label = h.get_by_label("Filter:").rect();
    click_at(&mut h, egui::pos2(label.right() + 60.0, label.center().y));
    snap(&mut h, "terceiros_filtro_aberto");
    // One maker's plug-ins.
    h.get_by_label("bv-ofx-testplug").click();
    settle(&mut h);
    // The Plug-In Chooser: the package in its folder.
    h.state_mut().show_tab_named("Project Media");
    h.state_mut().open_clip_window("fx");
    settle(&mut h);
    h.get_by_label("Plug-In Chain…").click();
    settle(&mut h);
    h.get_by_label("Filter Packages").click();
    settle(&mut h);
    h.get_by_label("Warm look").click();
    snap(&mut h, "seletor_pacotes");
}

/// Project Media's tags and searches: the Media Tags panel (the picked
/// media's tags, one on only some of them, the Add Tags box, the Quick
/// Tags), Tagged Media's bins, the Search Media Bins dialog and its
/// results.
#[test]
#[ignore = "renders images for visual review"]
fn captura_tags_e_busca() {
    let mut app = new_app();
    app.load_demo();
    app.media_tags_for_review();
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
    h.state_mut().show_tab_named("Project Media");
    snap(&mut h, "media_tags");
    // Details: the media's tags in their column.
    h.state_mut().media_details_for_review();
    snap(&mut h, "media_detalhes_tags");
    h.state_mut().media_search_for_review();
    snap(&mut h, "media_busca");
    h.get_by_label("Search").click();
    snap(&mut h, "media_busca_resultados");
}

/// Storyboards: one of the project's (a placeholder among its cells), the
/// Main Timeline's, and the Main Timeline's red after an edit it has not
/// followed.
#[test]
#[ignore = "renders images for visual review"]
fn captura_storyboards() {
    let mut app = new_app();
    app.load_demo();
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
    h.state_mut().show_tab_named("Project Media");
    h.state_mut().storyboards_for_review("own");
    snap(&mut h, "storyboard_proprio");
    h.state_mut().storyboards_for_review("main");
    snap(&mut h, "storyboard_principal");
    h.state_mut().storyboards_for_review("stale");
    snap(&mut h, "storyboard_fora_de_sincronia");
}

/// File › Export › Project Archive: its dialog's two pages (nothing is
/// written: Finish is not pressed).
#[test]
#[ignore = "renders images for visual review"]
fn captura_arquivo_do_projeto() {
    let mut app = new_app();
    app.load_demo();
    app.archive_dialog_for_review();
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
    snap(&mut h, "arquivo_projeto");
    h.get_by_label("Next >").click();
    snap(&mut h, "arquivo_projeto_resumo");
    // Save As with Copy Media with Project: how the media are copied.
    h.state_mut().copy_media_dialog_for_review();
    snap(&mut h, "copiar_midia");
}

/// How long each frame takes while the needle is dragged along the ruler
/// (the demo project; printed, for finding what slows the needle down).
#[test]
#[ignore = "measures the interface"]
#[allow(clippy::print_stdout)]
fn medir_arrasto_da_agulha() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    let y = 540.0;
    let start = egui::pos2(600.0, y);
    h.event(egui::Event::PointerMoved(start));
    h.event(egui::Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    h.step();
    let mut times = Vec::new();
    for k in 1..=80 {
        let at = egui::pos2(600.0 + 6.0 * k as f32, y);
        h.event(egui::Event::PointerMoved(at));
        let t0 = std::time::Instant::now();
        h.step();
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    h.event(egui::Event::PointerButton {
        pos: egui::pos2(1080.0, y),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    h.step();
    times.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!(
        "needle drag: frame mean {mean:.2} ms, median {:.2}, p90 {:.2}, max {:.2}",
        times[times.len() / 2],
        times[times.len() * 9 / 10],
        times[times.len() - 1]
    );
    println!("cursor at {:.3} s", h.state().cursor_seconds());
}

/// The Video Preview of the demo project in 8-bit and in 32-bit floating
/// point (full range) with the 1.000 (Linear) gamma: the same where a
/// picture shows alone, unblended; brighter where pictures mix (a
/// crossfade, an event at half opacity over black), as VEGAS's; and
/// through ACES's sRGB view, VEGAS's 32-bit default: ACES's look.
#[test]
#[ignore = "renders images for visual review"]
fn comparar_8_e_32_bits() {
    use bv_timeline::{CompositingGamma, PixelFormat};
    let mut app = new_app();
    app.load_demo();
    // The sunset (4–10 s) at half opacity over black.
    app.fades_for_review(0, 1, 0.0, 0.0, 0.5);
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
    for (name, format) in [
        ("8bit", PixelFormat::Full8Bit),
        ("32bit", PixelFormat::Float32Full),
    ] {
        h.state_mut().pixel_format_for_review(format);
        h.state_mut()
            .compositing_gamma_for_review(CompositingGamma::Linear);
        for (at, seconds) in [("sozinha", 2.0), ("crossfade", 4.5), ("opacidade", 7.0)] {
            h.state_mut().seek_seconds(seconds);
            snap(&mut h, &format!("comparar_{name}_{at}"));
        }
    }
    // Through ACES's sRGB view, as VEGAS's 32-bit projects show by
    // default: the picture alone changes too (ACES's look).
    h.state_mut()
        .aces_for_review(Some(bv_timeline::ViewTransform {
            display: bv_timeline::AcesDisplay::Srgb,
            view: bv_timeline::AcesView::Aces,
        }));
    for (at, seconds) in [("sozinha", 2.0), ("opacidade", 7.0)] {
        h.state_mut().seek_seconds(seconds);
        snap(&mut h, &format!("comparar_32bit_aces_{at}"));
    }
}

/// View › Window Layouts' five layouts (the demo project), as VEGAS 23's:
/// Default, Audio Mixing, Color Correction, Source Window, Timeline Top.
#[test]
#[ignore = "renders images for visual review"]
fn captura_layouts() {
    let mut app = new_app();
    app.load_demo();
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
    for (i, name) in [
        "default",
        "audio_mixing",
        "color_correction",
        "source_window",
        "timeline_top",
    ]
    .into_iter()
    .enumerate()
    {
        h.state_mut().layout_for_review(i);
        snap(&mut h, &format!("layout_{name}"));
    }
}

/// View › Window Layouts (VEGAS's submenu), Save Layout As (name,
/// shortcut, folder) and Organize Layouts (the folder's layouts and the
/// menu's), each closed with Cancel: nothing is saved.
#[test]
#[ignore = "renders images for visual review"]
fn captura_menu_e_janelas_de_layout() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    let open_submenu = |h: &mut Harness<'_, BananaApp>| {
        h.get_by_label("View").click();
        for _ in 0..4 {
            h.step();
        }
        let at = h
            .get_all_by_label_contains("Window Layouts")
            .last()
            .expect("Window Layouts")
            .rect()
            .center();
        h.event(egui::Event::PointerMoved(at));
        for _ in 0..6 {
            h.step();
        }
    };
    open_submenu(&mut h);
    snap(&mut h, "layouts_menu");
    h.get_by_label_contains("Save Layout As").click();
    snap(&mut h, "layouts_salvar_como");
    h.get_all_by_label("Cancel").last().expect("Cancel").click();
    settle(&mut h);
    open_submenu(&mut h);
    h.get_by_label_contains("Organize Layouts").click();
    snap(&mut h, "layouts_organizar");
    h.get_all_by_label("Cancel").last().expect("Cancel").click();
    settle(&mut h);
}

/// Video Scopes in the Color Correction layout, as VEGAS 23's screenshots
/// show them: each of the eight views without video (the scales alone)
/// and with the demo's frame, the scopes' other modes and Video Scope
/// Settings (closed with Cancel); on settings never saved.
#[test]
#[ignore = "renders images for visual review"]
fn captura_alcances() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1920.0, 1040.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            new_app(),
        );
    h.state_mut().layout_for_review(2);
    let views = [
        "vetorscopio",
        "forma_de_onda",
        "histograma",
        "parada_rgb",
        "vetorscopio_forma_de_onda",
        "vetorscopio_forma_de_onda_histograma",
        "vetorscopio_parada_rgb",
        "todos",
    ];
    for (i, name) in views.iter().enumerate() {
        h.state_mut().scopes_for_review(i);
        snap(&mut h, &format!("alcances_sem_video_{name}"));
    }
    h.state_mut().load_demo();
    for (i, name) in views.iter().enumerate() {
        h.state_mut().scopes_for_review(i);
        snap(&mut h, &format!("alcances_{name}"));
    }
    // Composite, Luminance/R/G/B and the parade combined without green.
    h.state_mut().scopes_for_review(7);
    h.state_mut()
        .scope_modes_for_review(true, 5, Some([true, false, true]));
    snap(&mut h, "alcances_todos_outros_modos");
    h.state_mut().scopes_for_review(2);
    h.state_mut().scope_modes_for_review(false, 6, None);
    snap(&mut h, "alcances_histograma_lab");
    h.state_mut().scope_settings_for_review();
    snap(&mut h, "alcances_configuracoes");
    h.get_all_by_label("Cancel").last().expect("Cancel").click();
    settle(&mut h);
}

/// The calibration card (`tools/calibration_card.py`; `BV_CARD` names
/// another copy) through the Video Scopes, as the user's VEGAS screenshots
/// show it: the Color Correction layout, the preview at Best (Full), each
/// scope alone; on settings never saved.
#[test]
#[ignore = "renders images for visual review"]
fn alcances_da_cartela() {
    let card = std::env::var_os("BV_CARD")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../calibracao/cartela_1920x1080.png")
        });
    if !card.exists() {
        eprintln!("no calibration card at {}", card.display());
        return;
    }
    let mut app = new_app();
    assert!(app.import_and_place(&card), "the card imports");
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
    h.state_mut().preview_best_full_for_review();
    h.state_mut().seek_seconds(1.5);
    for (i, name) in ["vetorscopio", "forma_de_onda", "histograma", "parada_rgb"]
        .into_iter()
        .enumerate()
    {
        h.state_mut().scopes_for_review(i);
        h.state_mut().preview_best_full_for_review();
        snap(&mut h, &format!("cartela_{name}"));
    }
}

/// Edit Details as VEGAS 2026's: Show, the Preset box with Save and
/// Delete, the table with its row numbers and corner box, a few cells
/// picked; `show` names the list ("events", "cd", "commands"). The
/// timeline above shows the CD layout bar and the command bar.
fn edit_details(show: &str, name: &str) {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    snap(&mut h, name);
}

#[test]
#[ignore = "renders images for visual review"]
fn captura_edit_details_eventos() {
    edit_details("events", "edit_details_eventos");
}

#[test]
#[ignore = "renders images for visual review"]
fn captura_edit_details_cd() {
    edit_details("cd", "edit_details_cd");
}

#[test]
#[ignore = "renders images for visual review"]
fn captura_edit_details_comandos() {
    edit_details("commands", "edit_details_comandos");
}

/// Insert › Command…'s Command Properties: a template, the command, its
/// parameter, a comment and the position.
#[test]
#[ignore = "renders images for visual review"]
fn captura_command_properties() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    snap(&mut h, "command_properties");
}

/// The Marker Tool on (its button lit at the timeline's top right), a
/// marker and a region picked (outlined).
#[test]
#[ignore = "renders images for visual review"]
fn captura_marker_tool() {
    let mut app = new_app();
    app.load_demo();
    app.marker_tool_for_review();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    settle(&mut h);
    snap(&mut h, "marker_tool");
}

/// The Trimmer's editing: the history's buttons, the adds, Fit to Fill
/// and Overwrite (on), In, Out and Length in boxes, the time bar zoomed;
/// a picture (the color bars) and a sound (a tone: its waveform).
#[test]
#[ignore = "renders images for visual review"]
fn captura_trimmer_edicao() {
    for (index, name) in [(0, "trimmer_imagem"), (3, "trimmer_som")] {
        let mut app = new_app();
        app.load_demo();
        app.trimmer_for_review(index);
        let mut h = Harness::builder()
            .with_size(egui::vec2(1600.0, 900.0))
            .with_pixels_per_point(1.0)
            .wgpu()
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        settle(&mut h);
        snap(&mut h, name);
    }
}

/// Subclips: one in Project Media after its media, Create Subclip open.
#[test]
#[ignore = "renders images for visual review"]
fn captura_subclipes() {
    let mut app = new_app();
    app.load_demo();
    app.subclips_for_review();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    settle(&mut h);
    snap(&mut h, "subclipes");
}

/// Tools › Burn Disc › DVD: the Burn DVD dialog (a drive of the tests'),
/// then the job's window burning.
#[test]
#[ignore = "renders images for visual review"]
fn captura_gravar_dvd() {
    let mut app = new_app();
    app.load_demo();
    app.burn_dvd_for_review();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    settle(&mut h);
    snap(&mut h, "gravar_dvd");
    h.state_mut().burn_job_for_review();
    settle(&mut h);
    snap(&mut h, "gravar_dvd_progresso");
}

/// Tools › Burn Disc's audio CD dialogs (a drive of the tests'): the
/// Track-at-Once one, then the Disc-at-Once one, then a job's window.
#[test]
#[ignore = "renders images for visual review"]
fn captura_gravar_cd() {
    let mut app = new_app();
    app.load_demo();
    app.burn_cd_for_review(false);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    settle(&mut h);
    snap(&mut h, "gravar_cd_tao");
    h.state_mut().burn_cd_for_review(true);
    settle(&mut h);
    snap(&mut h, "gravar_cd_dao");
    h.state_mut().burn_cd_job_for_review();
    settle(&mut h);
    snap(&mut h, "gravar_cd_progresso");
}

/// View › Mixer Preview Fader: the Master Bus window with the Preview
/// strip left of the Master's; Project Media's Start, Stop and Auto
/// Preview (on).
#[test]
#[ignore = "renders images for visual review"]
fn captura_preview_fader() {
    let mut app = new_app();
    app.load_demo();
    app.preview_fader_for_review();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    settle(&mut h);
    snap(&mut h, "preview_fader");
}

/// The Explorer in Thumbnails (a folder of test files), Views' Regions
/// and Summary at the bottom for the selected sound.
#[test]
#[ignore = "renders images for visual review"]
fn captura_explorer_miniaturas() {
    let dir = std::env::temp_dir().join("bv-explorer-review");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("Narration.wav");
    bananav_test_wav(&wav);
    for (name, rgb) in [
        ("Sunset.png", [230u8, 120, 40]),
        ("Forest.png", [40, 140, 60]),
    ] {
        let img = image::RgbImage::from_fn(320, 180, |x, y| {
            let k = (x + y) as f32 / 500.0;
            image::Rgb([
                (rgb[0] as f32 * (0.5 + k)).min(255.0) as u8,
                (rgb[1] as f32 * (0.5 + k)).min(255.0) as u8,
                (rgb[2] as f32 * (0.5 + k)).min(255.0) as u8,
            ])
        });
        img.save(dir.join(name)).unwrap();
    }
    let mut app = new_app();
    app.load_demo();
    app.explorer_for_review(&dir, &wav);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut BananaApp| {
                apply_theme(ui.ctx());
                app.frame(ui);
            },
            app,
        );
    settle(&mut h);
    snap(&mut h, "explorer_miniaturas");
    // Every file picked (Ctrl and Shift), in Details.
    h.state_mut().explorer_pick_all_for_review();
    settle(&mut h);
    snap(&mut h, "explorer_varios");
}

/// A part of a video picked on its Project Media thumbnail (lighter
/// between two lines), and a storyboard's cell of that part.
#[test]
#[ignore = "renders images for visual review; needs test-media/generated/teste_h264_aac.mp4"]
fn captura_trechos_nas_miniaturas() {
    let video = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-media/generated/teste_h264_aac.mp4");
    assert!(
        video.exists(),
        "generate it with: cargo test -p bv-mf --test oraculo -- --ignored"
    );
    let mut app = new_app();
    app.load_demo();
    app.media_parts_for_review(&video).expect("imported");
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
    h.state_mut().show_tab_named("Project Media");
    settle(&mut h);
    snap(&mut h, "media_trecho_miniatura");
    h.state_mut().storyboard_for_review("Cut");
    settle(&mut h);
    snap(&mut h, "storyboard_trecho");
}

/// A 4-second tone with a region ("Chorus", 1 to 3 s), for the Explorer.
fn bananav_test_wav(path: &std::path::Path) {
    // 16-bit mono at 48 kHz, a 440 Hz tone.
    let frames = 48_000u32 * 4;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + frames * 2).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&48_000u32.to_le_bytes());
    b.extend_from_slice(&96_000u32.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(frames * 2).to_le_bytes());
    for i in 0..frames {
        let v = (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.3;
        b.extend_from_slice(&((v * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, b).unwrap();
    let s = bv_core::Tick::from_seconds;
    bv_engine::save_markers(
        &path.to_string_lossy(),
        &[bv_timeline::MediaMarker {
            time: s(1),
            length: s(2),
            name: "Chorus".into(),
        }],
    )
    .unwrap();
}

/// The Video Media Generators window's Media Properties and Replace
/// Plug-In (its list; Checkerboard picked in the gradient's place), after a
/// new adjustment track with its Composite envelope along its top.
#[test]
#[ignore = "renders images for visual review"]
fn captura_midia_gerada_e_trilha_de_ajuste() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    // Insert › Adjustment Track: the blue line along its top.
    h.get_by_label("Insert").click();
    settle(&mut h);
    // The menu's item ends in its keys.
    h.get_by_label_contains("Adjustment Track").click();
    settle(&mut h);
    snap(&mut h, "trilha_de_ajuste_envelope");
    h.state_mut().generated_media_for_review();
    settle(&mut h);
    assert!(h.query_by_label("Media Properties").is_some());
    let before = h.state().media_for_review();
    h.get_by_label("Replace Plug-In").click();
    settle(&mut h);
    snap(&mut h, "midia_gerada_substituir");
    // The menu's item (the demo has a Checkerboard event too).
    h.get_all_by_label("Checkerboard")
        .last()
        .expect("Checkerboard in the menu")
        .click();
    settle(&mut h);
    let after = h.state().media_for_review();
    assert_ne!(before, after, "the gradient replaced");
    snap(&mut h, "midia_gerada_substituida");
    h.get_by_label("Media Properties").click();
    settle(&mut h);
    snap(&mut h, "midia_gerada_propriedades");
}

/// Credit Roll's settings in the Video Media Generators window: the Import
/// and Export files (Browse… opens a dialog to save for the exports) and
/// the Position group, its box first — the frame, the credits' part of it
/// and the yellow handles of the margins.
#[test]
#[ignore = "renders images for visual review"]
fn captura_credit_roll() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    h.state_mut().show_tab_named("Media Generators");
    settle(&mut h);
    h.get_by_label("Credit Roll").click();
    settle(&mut h);
    add_preset(&mut h, "Scrolling Credits");
    h.get_by_label("Import and Export").click();
    settle(&mut h);
    for label in [
        "Position",
        "Left:",
        "Right:",
        "Top:",
        "Import Text:",
        "Export Properties:",
    ] {
        assert!(
            h.query_all_by_label(label).next().is_some(),
            "no control for {label}"
        );
    }
    snap(&mut h, "credit_roll_janela");
}

/// GL Transition: the code box with a new transition's crossfade, then a
/// preset's (Bounce) named controls — its code hidden —, and the preview
/// halfway through.
#[test]
#[ignore = "renders images for visual review"]
fn captura_gl_transition_custom() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    let id = "org.bananav.GLTransition";
    assert!(h.state_mut().transition_plugin_for_review(id, None, true));
    settle(&mut h);
    assert!(
        h.query_all_by_label("Enter your GL transition code:")
            .next()
            .is_some(),
        "no code box"
    );
    snap(&mut h, "gl_transition_custom");
    assert!(
        h.state_mut()
            .transition_plugin_for_review(id, Some("Bounce"), true)
    );
    settle(&mut h);
    for label in ["Shadow Height:", "Bounces:", "Shadow Color:"] {
        assert!(
            h.query_all_by_label(label).next().is_some(),
            "no control for {label}"
        );
    }
    assert!(
        h.query_all_by_label("Enter your GL transition code:")
            .next()
            .is_none()
    );
    snap(&mut h, "gl_transition_bounce");
    h.state_mut().show_tab_named("Video Preview");
    snap(&mut h, "gl_transition_custom_previa");
}

/// A generated media event copied and pasted (the demo's selected
/// gradient, Ctrl+C and Ctrl+V): VEGAS's question — a new copy of the
/// source media or a reference to the original —, OK making the copy.
#[test]
#[ignore = "renders images for visual review"]
fn captura_copia_de_midia_gerada() {
    let mut app = new_app();
    app.load_demo();
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
    settle(&mut h);
    let before = h.state().media_for_review().len();
    // Ctrl+C and Ctrl+V: the selected event (the gradient) at the cursor.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::C);
    settle(&mut h);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::V);
    settle(&mut h);
    snap(&mut h, "midia_gerada_copia");
    assert!(
        h.query_by_label("Create a new copy of the source media")
            .is_some()
    );
    h.get_by_label("OK").click();
    settle(&mut h);
    assert_eq!(
        h.state().media_for_review().len(),
        before + 1,
        "the pasted event's own copy"
    );
}

/// The Trimmer as VEGAS 23's, as the user's prints show it: a video with a
/// marker ("toreja") and its loop region, the cursor out of it; its menus
/// (the picture's, the loop region's, the bar's), More; a sound (ruler,
/// waveform, scroll bar) and its menus (the waveform's, the ruler's time
/// formats). Then Project Media's menu on a video file, a sound file and
/// generated media.
#[test]
#[ignore = "renders images for visual review; needs test-media/generated/teste_h264_aac.mp4"]
fn captura_trimmer_como_o_vegas() {
    let video = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-media/generated/teste_h264_aac.mp4");
    assert!(
        video.exists(),
        "generate it with: cargo test -p bv-mf --test oraculo -- --ignored"
    );
    let dir = std::env::temp_dir().join(format!("bv-trimmer-vegas-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sound = dir.join("teste som.wav");
    bananav_test_wav(&sound);
    let mut app = new_app();
    app.load_demo();
    assert!(app.trimmer_vegas_for_review(&video), "the video imported");
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
    settle(&mut h);
    snap(&mut h, "trimmer_vegas_video");
    let [media, strip, bar, _, more] = h.state().trimmer_rects_for_review();
    let (media, strip, bar, more) = (
        media.expect("picture"),
        strip.expect("loop bar"),
        bar.expect("time bar"),
        more.expect("More"),
    );
    // Esc, and a click on the status bar for a menu Esc leaves open.
    let close = |h: &mut Harness<'_, BananaApp>| {
        h.key_press(egui::Key::Escape);
        settle(h);
        click_at(h, egui::pos2(300.0, 950.0));
        settle(h);
    };
    right_click_at(&mut h, media.center());
    snap(&mut h, "trimmer_menu_imagem");
    close(&mut h);
    // The loop region lies from 30 % to 60 % of the bar.
    let band = egui::pos2(strip.left() + strip.width() * 0.45, strip.bottom() - 4.0);
    right_click_at(&mut h, band);
    snap(&mut h, "trimmer_menu_loop");
    close(&mut h);
    right_click_at(
        &mut h,
        egui::pos2(bar.left() + bar.width() * 0.85, bar.center().y),
    );
    snap(&mut h, "trimmer_menu_barra");
    close(&mut h);
    click_at(&mut h, more.center());
    snap(&mut h, "trimmer_mais");
    close(&mut h);
    // The sound.
    assert!(h.state_mut().trimmer_vegas_for_review(&sound));
    settle(&mut h);
    snap(&mut h, "trimmer_vegas_audio");
    let [media, _, _, ruler, _] = h.state().trimmer_rects_for_review();
    let (wave, ruler) = (media.expect("waveform"), ruler.expect("ruler"));
    right_click_at(&mut h, wave.center());
    snap(&mut h, "trimmer_menu_onda");
    close(&mut h);
    right_click_at(&mut h, egui::pos2(ruler.left() + 120.0, ruler.center().y));
    snap(&mut h, "trimmer_menu_regua");
    close(&mut h);
    // Project Media's menu on each kind.
    h.state_mut().media_thumbnails_for_review();
    h.state_mut().show_tab_named("Project Media");
    settle(&mut h);
    for (name, file) in [
        ("midia_menu_video", Some(video.as_path())),
        ("midia_menu_audio", Some(sound.as_path())),
        ("midia_menu_gerador", None),
    ] {
        let at = h
            .state()
            .media_thumb_rect_for_review(file)
            .expect("its thumbnail");
        right_click_at(&mut h, at.center());
        snap(&mut h, name);
        close(&mut h);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The settings of BananaV's own effects in the FX window, a picture each
/// for review: `BV_PANELS` names them as "Category/Effect", parted by
/// semicolons ("Utility/Bézier Masking"); each added at its (Default)
/// preset — or the one a third part names ("Utility/Flicker Control/Add
/// 50Hz Flicker") — to the demo's selected event,
/// `target/telas/painel_<effect>[_<preset>].png`.
#[test]
#[ignore = "renders images for visual review"]
fn captura_paineis_de_efeitos() {
    let Ok(list) = std::env::var("BV_PANELS") else {
        eprintln!("BV_PANELS is not set: nothing to capture");
        return;
    };
    for item in list.split(';').filter(|s| !s.trim().is_empty()) {
        // "|" parts them where a category has a slash ("AI/ML|AI Sharpen").
        let mark = if item.contains('|') { '|' } else { '/' };
        let mut parts = item.trim().splitn(3, mark);
        let (category, effect) = (
            parts.next().expect("Category/Effect"),
            parts.next().expect("Category/Effect"),
        );
        let preset = parts.next();
        let mut h = video_fx_harness(category, effect);
        add_preset(&mut h, preset.unwrap_or("(Default)"));
        let name = match preset {
            Some(p) => format!("painel_{}_{}", file_name(effect), file_name(p)),
            None => format!("painel_{}", file_name(effect)),
        };
        snap(&mut h, &name);
    }
}

/// The settings of BananaV's own generators `BV_GENERATORS` names with a
/// preset each ("Checkerboard/Large Tiles;Test Pattern/Ramp"): the preset
/// added from Media Generator, its Video Media Generators window open.
#[test]
#[ignore = "renders images for visual review"]
fn captura_paineis_de_geradores() {
    let Ok(list) = std::env::var("BV_GENERATORS") else {
        eprintln!("BV_GENERATORS is not set: nothing to capture");
        return;
    };
    for item in list.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (generator, preset) = item.split_once('/').expect("Generator/Preset");
        let mut app = new_app();
        app.load_demo();
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
        settle(&mut h);
        h.state_mut().show_tab_named("Media Generators");
        settle(&mut h);
        h.get_all_by_label(generator)
            .find(|n| n.accesskit_node().toggled().is_some())
            .expect(generator)
            .click();
        settle(&mut h);
        add_preset(&mut h, preset);
        snap(&mut h, &format!("painel_{}", file_name(generator)));
    }
}

/// The settings windows of the compositors `BV_COMPOSITORS` names with a
/// preset each ("org.bananav.HeightMap/Deep Water;…"): a ramp guiding the
/// demo's video through each, by a track's Custom compositing.
#[test]
#[ignore = "renders images for visual review"]
fn captura_paineis_de_compositores() {
    let Ok(list) = std::env::var("BV_COMPOSITORS") else {
        eprintln!("BV_COMPOSITORS is not set: nothing to capture");
        return;
    };
    for item in list.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (id, preset) = item.split_once('/').expect("id/Preset");
        let mut app = new_app();
        app.load_demo();
        app.compositor_for_review(id, preset);
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
        settle(&mut h);
        let name = id.rsplit('.').next().unwrap_or(id).to_ascii_lowercase();
        snap(&mut h, &format!("painel_{name}"));
    }
}

/// The settings of BananaV's own transitions `BV_TRANSITIONS` names, with a
/// preset each when given ("org.bananav.Iris/Circle, Out, White Border;
/// org.bananav.Dissolve"): the demo's first crossfade given it, its FX
/// window open, `target/telas/painel_<transition>.png`.
#[test]
#[ignore = "renders images for visual review"]
fn captura_paineis_de_transicoes() {
    let Ok(list) = std::env::var("BV_TRANSITIONS") else {
        eprintln!("BV_TRANSITIONS is not set: nothing to capture");
        return;
    };
    for item in list.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (id, preset) = match item.split_once('/') {
            Some((id, preset)) => (id, Some(preset)),
            None => (item, None),
        };
        let mut app = new_app();
        app.load_demo();
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
        settle(&mut h);
        assert!(
            h.state_mut().transition_plugin_for_review(id, preset, true),
            "{id}"
        );
        settle(&mut h);
        let name = id.rsplit('.').next().unwrap_or(id).to_ascii_lowercase();
        snap(&mut h, &format!("painel_{name}"));
    }
}

/// A capture's name for an effect or generator: "Pinch/Punch" as
/// "pinch_punch".
fn file_name(name: &str) -> String {
    name.chars()
        .filter_map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' => Some(c.to_ascii_lowercase()),
            ' ' | '/' | '-' => Some('_'),
            'é' => Some('e'),
            _ => None,
        })
        .collect()
}

/// The standard handles on the preview: Spherize, Swirl and Radial Blur
/// have no overlay of their own, their Center a handle as VEGAS's.
#[test]
#[ignore]
fn captura_alcas_padrao() {
    for (id, name) in [
        ("org.bananav.Spherize", "alca_spherize"),
        ("org.bananav.Swirl", "alca_swirl"),
        ("org.bananav.RadialBlur", "alca_radial_blur"),
        ("org.bananav.LensFlare", "alca_lens_flare"),
        ("org.bananav.OffsetWrap", "alca_offset_wrap"),
        ("org.bananav.BumpMap", "alca_bump_map"),
        ("org.bananav.LensCorrection", "alca_lens_correction"),
    ] {
        let mut app = new_app();
        app.load_demo();
        assert!(app.plugin_effect_for_review(id), "{id}");
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
        settle(&mut h);
        snap(&mut h, name);
        if id == "org.bananav.Spherize" {
            // The handle in the preview's middle, dragged a quarter of the
            // way right and up.
            drag(
                &mut h,
                egui::pos2(1129.0, 315.0),
                egui::pos2(1129.0 + 167.0, 315.0 - 94.0),
                None,
            );
            settle(&mut h);
            let c = corrector_value(&h, "center");
            assert!(
                (c[0] - 0.75).abs() < 0.02 && (c[1] - 0.75).abs() < 0.02,
                "{c:?}"
            );
            snap(&mut h, "alca_spherize_arrastada");
        }
    }
}
