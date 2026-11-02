//! Off-screen captures of the shortcut menus: the timeline's (a video
//! track's empty part, a video event, events let go after a drag with the
//! right mouse button) and the Video Preview's, and the preview zoomed
//! with the mouse wheel, in English, for visual review. Run with:
//! `cargo test -p bv-ui-egui --test telas_menus -- --ignored --nocapture`
//! The images go to `target/telas/menus_*.png`. Nothing is chosen in the
//! menus (the right-drag's is cancelled), so no setting changes.

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
    img.save(dir.join(format!("menus_{name}.png")))
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

/// A press and release of `button` at `at`.
fn click_with(h: &mut Harness<'_, BananaApp>, at: egui::Pos2, button: egui::PointerButton) {
    h.event(egui::Event::PointerMoved(at));
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }
}

/// The pointer on the item whose label holds `label` (the last one: a
/// menu's, over the window's own button of that name); a submenu opens.
fn hover(h: &mut Harness<'_, BananaApp>, label: &str) {
    let at = h
        .get_all_by_label_contains(label)
        .last()
        .expect(label)
        .rect()
        .center();
    h.event(egui::Event::PointerMoved(at));
    for _ in 0..4 {
        h.step();
    }
}

#[test]
#[ignore = "renders images for visual review"]
fn captura_menus() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::English);
    app.load_demo();
    app.welcome_for_review("closed");
    let mut h = harness(app);
    settle(&mut h);
    let right = egui::PointerButton::Secondary;
    // The menu bar's empty end: a click there closes a menu.
    let away = egui::pos2(1500.0, 19.0);

    // A video track's empty part, then its envelopes.
    let empty = h.state().timeline_spot_for_review(0, 20.0).expect("track");
    click_with(&mut h, empty, right);
    snap(&mut h, "trilha_video");
    hover(&mut h, "Insert/Remove Envelope");
    snap(&mut h, "trilha_video_envelopes");
    click_with(&mut h, away, egui::PointerButton::Primary);

    // A video event (the demo's gradient), then its switches.
    let event = h.state().timeline_spot_for_review(0, 7.0).expect("event");
    click_with(&mut h, event, right);
    // VEGAS 23's items and no others: Motion Tracking once, no beats.
    assert_eq!(h.query_all_by_label_contains("Motion Tracking").count(), 1);
    for label in ["Beat Detection", "Remove Sync Link from Selected Events"] {
        assert!(
            h.query_all_by_label_contains(label).next().is_none(),
            "{label}"
        );
    }
    snap(&mut h, "evento_video");
    hover(&mut h, "Switches");
    snap(&mut h, "evento_video_switches");
    click_with(&mut h, away, egui::PointerButton::Primary);

    // An audio event (the demo's 440 Hz tone), as VEGAS's: its beats, no
    // Media FX, then its channels.
    let tone = h.state().timeline_spot_for_review(1, 7.0).expect("tone");
    click_with(&mut h, tone, right);
    for label in [
        "Open in Audio Editor",
        "Beat Detection",
        "Apply Non-Real-Time Event FX…",
    ] {
        assert!(
            h.query_all_by_label_contains(label).next().is_some(),
            "no {label}"
        );
    }
    assert!(h.query_all_by_label_contains("Media FX").next().is_none());
    snap(&mut h, "evento_audio");
    hover(&mut h, "Channels");
    snap(&mut h, "evento_audio_canais");
    click_with(&mut h, away, egui::PointerButton::Primary);

    // Below the tracks: Open, the tracks to insert and Preferences (just
    // under the last track, whatever room the layout gives the timeline).
    let below = h.state().timeline_spot_for_review(1, 20.0).expect("track") + egui::vec2(0.0, 60.0);
    click_with(&mut h, below, right);
    snap(&mut h, "abaixo_das_trilhas");
    for label in [
        "Open…",
        "Insert Audio Track",
        "Insert Video Track",
        "Preferences…",
    ] {
        assert!(
            h.query_all_by_label_contains(label).next().is_some(),
            "no {label}"
        );
    }
    click_with(&mut h, away, egui::PointerButton::Primary);

    // The event dragged with the right button and let go: Move Here, Copy
    // Here or Cancel (cancelled).
    let to = h.state().timeline_spot_for_review(0, 9.0).expect("there");
    h.event(egui::Event::PointerMoved(event));
    h.step();
    h.event(egui::Event::PointerButton {
        pos: event,
        button: right,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
    for i in 1..=8 {
        let p = event + (to - event) * (i as f32 / 8.0);
        h.event(egui::Event::PointerMoved(p));
        h.step();
    }
    h.event(egui::Event::PointerButton {
        pos: to,
        button: right,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
    snap(&mut h, "arrasto_direito");
    h.get_by_label("Cancel").click();
    settle(&mut h);

    // Ctrl+drag of the ends of the gradient (video) and of the 440 Hz tone
    // (audio), from 10 to 12 seconds: slower, a wavy line and the rate.
    for track in [0, 1] {
        let from = h
            .state()
            .timeline_spot_for_review(track, 9.98)
            .expect("edge");
        let to = h
            .state()
            .timeline_spot_for_review(track, 12.0)
            .expect("there");
        h.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
        h.event(egui::Event::PointerMoved(from));
        h.step();
        h.event(egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::COMMAND,
        });
        h.step();
        for i in 1..=8 {
            h.event(egui::Event::PointerMoved(
                from + (to - from) * (i as f32 / 8.0),
            ));
            h.step();
        }
        h.event(egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::COMMAND,
        });
        h.step();
        h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        h.step();
    }
    snap(&mut h, "esticado");
    // The sound of an event moved away from its picture: by how much.
    h.state_mut().sync_offset_for_review();
    snap(&mut h, "fora_de_sincronia");

    // The Video Preview's menu, then its quality's.
    let frame = h.state().preview_spot_for_review();
    click_with(&mut h, frame, right);
    snap(&mut h, "previa");
    hover(&mut h, "Preview Quality");
    snap(&mut h, "previa_qualidade");
    click_with(&mut h, away, egui::PointerButton::Primary);

    // The audio track's meter lit: −6 dB on the left, a clip on the right.
    h.state_mut().track_meters_for_review(vec![0.5, 1.05]);
    h.step();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
    let img = h.render().expect("render");
    img.save(dir.join("menus_medidor_trilha.png"))
        .expect("save");

    // Three notches of the mouse wheel toward the frame's upper left.
    let at = frame - egui::vec2(120.0, 60.0);
    h.event(egui::Event::PointerMoved(at));
    for _ in 0..3 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, 1.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }
    snap(&mut h, "previa_zoom_roda");

    // A video track's header, then an audio track's: VEGAS's menus.
    let headers: [(usize, &str, &[&str]); 2] = [
        (
            0,
            "cabecalho_video",
            &[
                "More",
                "Insert Video Track",
                "Expand Track Layers",
                "Track Display Color",
            ],
        ),
        (
            1,
            "cabecalho_audio",
            &[
                "Arm for Record",
                "Pan Type",
                "FX Automation Envelopes",
                "Show Output Meters",
            ],
        ),
    ];
    for (track, name, items) in headers {
        let at = h
            .state()
            .track_header_spot_for_review(track)
            .expect("header");
        click_with(&mut h, at, right);
        for label in items {
            assert!(
                h.query_all_by_label_contains(label).next().is_some(),
                "no {label}"
            );
        }
        snap(&mut h, name);
        click_with(&mut h, away, egui::PointerButton::Primary);
    }

    // The checkerboard (video, from 10 seconds) dragged below the tracks:
    // its ghost there meanwhile, a new video track for it when let go, as
    // VEGAS's.
    assert_eq!(h.state().tracks_for_review().len(), 2);
    let from = h.state().timeline_spot_for_review(0, 12.0).expect("event");
    let to = h.state().timeline_spot_for_review(1, 12.0).expect("track") + egui::vec2(0.0, 150.0);
    h.event(egui::Event::PointerMoved(from));
    h.step();
    h.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    for i in 1..=12 {
        h.event(egui::Event::PointerMoved(
            from + (to - from) * (i as f32 / 12.0),
        ));
        h.step();
    }
    assert_eq!(
        h.state().tracks_for_review().len(),
        2,
        "not before it is let go"
    );
    snap(&mut h, "nova_trilha_arrastando");
    h.event(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    settle(&mut h);
    let tracks = h.state().tracks_for_review();
    assert_eq!(tracks.len(), 3, "{tracks:?}");
    assert_eq!(tracks[2], (true, 1), "{tracks:?}");
    snap(&mut h, "nova_trilha");
}

#[test]
#[ignore = "renders images for visual review"]
fn captura_selecao_e_medidores() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::English);
    app.load_demo();
    app.welcome_for_review("closed");
    let mut h = harness(app);
    settle(&mut h);
    // Events picked (yellow) and one selected for being in a group (blue).
    h.state_mut().grouped_selection_for_review();
    snap(&mut h, "selecao_grupo");
    // The mouse wheel over an audio track's output meter: the closer
    // range, then the two wider ones.
    let meter = h.state_mut().track_meter_wheel_for_review().expect("meter");
    h.event(egui::Event::PointerMoved(meter));
    h.step();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
    for (notches, name) in [(1.0, "fino"), (-2.0, "largo"), (-1.0, "mais_largo")] {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, notches),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        h.state_mut().track_meters_for_review(vec![0.5, 1.05]);
        h.step();
        let img = h.render().expect("render");
        img.save(dir.join(format!("menus_medidor_{name}.png")))
            .expect("save");
    }
    // Use Vertical Meters off: the meters across the headers.
    h.state_mut().track_meters_across_for_review();
    h.step();
    h.state_mut().track_meters_for_review(vec![0.5, 1.05]);
    h.step();
    let img = h.render().expect("render");
    img.save(dir.join("menus_medidor_horizontal.png"))
        .expect("save");
}

#[test]
#[ignore = "renders images for visual review"]
fn captura_boas_vindas() {
    // The welcome screen's pages; nothing is created, opened or saved (the
    // check box and the languages are left alone).
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::English);
    app.welcome_for_review("new");
    let mut h = harness(app);
    snap(&mut h, "boas_vindas_novo");
    // The resolution's and the frame rate's lists: the welcome screen's,
    // top to bottom (the Explorer has one too, higher up).
    let combo = |h: &Harness<'_, BananaApp>, i: usize| {
        let mut lists: Vec<egui::Rect> = h
            .get_all_by_role(egui::accesskit::Role::ComboBox)
            .map(|n| n.rect())
            .filter(|r| r.top() > 400.0)
            .collect();
        lists.sort_by(|a, b| {
            a.top()
                .total_cmp(&b.top())
                .then(a.left().total_cmp(&b.left()))
        });
        lists[i].center()
    };
    let at = combo(&h, 0);
    click_with(&mut h, at, egui::PointerButton::Primary);
    snap(&mut h, "boas_vindas_resolucao");
    h.key_press(egui::Key::Escape);
    h.get_by_label("Portrait (9:16)").click();
    settle(&mut h);
    let at = combo(&h, 1);
    click_with(&mut h, at, egui::PointerButton::Primary);
    snap(&mut h, "boas_vindas_taxa");
    h.key_press(egui::Key::Escape);
    h.get_by_label("Open Project").click();
    snap(&mut h, "boas_vindas_abrir");
    // Getting Started: BananaV's website first, then GitHub, Discord and
    // donations, each card with its address (nothing is opened).
    h.get_by_label("Getting Started").click();
    snap(&mut h, "boas_vindas_comecar");
    for (tab, file) in [
        ("GitHub", "boas_vindas_github"),
        ("Discord", "boas_vindas_discord"),
        ("Donate", "boas_vindas_doacao"),
    ] {
        h.get_by_label(tab).click();
        snap(&mut h, file);
    }
    h.get_by_label("Language").click();
    snap(&mut h, "boas_vindas_idioma");
}

/// Fades as VEGAS draws them, up to the event's level line (the
/// checkerboard at 56 %, the 440 Hz tone at half its gain), and the popup
/// of curves a right-click on a fade opens (a video fade-in, an audio
/// fade-out). Nothing is picked.
#[test]
#[ignore = "renders images for visual review"]
fn captura_curvas_de_fade() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::English);
    app.load_demo();
    app.welcome_for_review("closed");
    app.fades_for_review(0, 2, 2.0, 2.0, 0.56);
    app.fades_for_review(1, 1, 1.5, 2.0, 0.5);
    let mut h = harness(app);
    settle(&mut h);
    snap(&mut h, "fades_ate_o_nivel");
    let right = egui::PointerButton::Secondary;
    let away = egui::pos2(1500.0, 19.0);
    let fade_in = h
        .state()
        .timeline_spot_for_review(0, 10.5)
        .expect("fade-in");
    click_with(&mut h, fade_in, right);
    assert!(h.query_all_by_label("Sharp").next().is_some(), "the popup");
    snap(&mut h, "fade_curvas_video");
    click_with(&mut h, away, egui::PointerButton::Primary);
    let fade_out = h
        .state()
        .timeline_spot_for_review(1, 9.5)
        .expect("fade-out");
    click_with(&mut h, fade_out, right);
    assert!(h.query_all_by_label("Sharp").next().is_some(), "the popup");
    snap(&mut h, "fade_curvas_audio");
    click_with(&mut h, away, egui::PointerButton::Primary);
    // A crossfade (the bars into the sunset): VEGAS's 25 pairs.
    let crossfade = h
        .state()
        .timeline_spot_for_review(0, 4.5)
        .expect("crossfade");
    click_with(&mut h, crossfade, right);
    assert!(
        h.query_all_by_label("Out: Fast, in: Slow").next().is_some(),
        "the popup"
    );
    snap(&mut h, "crossfade_curvas");
    click_with(&mut h, away, egui::PointerButton::Primary);
    // The checkerboard picked: its fades' handles on its level line.
    let checkerboard = h.state().timeline_spot_for_review(0, 13.0).expect("event");
    click_with(&mut h, checkerboard, egui::PointerButton::Primary);
    snap(&mut h, "fade_ancoras");
}

/// Media let go with the right mouse button: Add Across Time, Add Across
/// Tracks, Add as Takes, Video Only and Audio Only (open), Cancel.
#[test]
#[ignore = "renders images for visual review"]
fn captura_menu_de_soltar_midia() {
    let mut app = BananaApp::new(egui::Context::default());
    set_language(Language::English);
    app.welcome_for_review("closed");
    app.load_demo();
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
    h.state_mut().media_drop_menu_for_review(9.0);
    settle(&mut h);
    hover(&mut h, "Video Only");
    snap(&mut h, "soltar_midia_direito");
}
