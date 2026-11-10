//! The interface's own time for a frame while the preview plays: the
//! guard against playback slowing down again because of work the
//! interface does every frame. It happened (2026-11-10): the LUT Filter's
//! presets written out at every frame, the preview's frames converted
//! pixel by pixel and video thumbnails decoding during playback took a 4K
//! recording from 60 window frames a second to 40.
//!
//! The demo project plays with each docked window shown in turn beside
//! the preview; every window's median frame must stay within the budget,
//! and its 90th percentile within half as much again.
//! Only optimized builds are measured (a debug build's times mean
//! nothing): `cargo xtask perf`, or
//! `cargo test --release -p bv-ui-egui --test frame_budget -- --ignored --nocapture`.
//! `BV_BUDGET_FILE=<video>` plays a file instead of the demo project,
//! `BV_BUDGET_MS=6` changes the budget.

#![allow(clippy::print_stdout)]

use std::time::{Duration, Instant};

use bv_ui_egui::{BananaApp, Language, apply_theme, egui, set_language};
use egui_kittest::{Harness, TestRenderer};

/// The median time the interface may take for a frame, any window shown
/// (milliseconds, in an optimized build): a 60 Hz screen's frame is 16.7,
/// and painting the frame and the graphics card's share come on top.
const BUDGET_MS: f64 = 6.0;

/// Textures go nowhere, as if the graphics card had taken them (the
/// default renderer keeps every one until it draws).
struct Discard;

impl TestRenderer for Discard {
    fn handle_delta(&mut self, delta: &mut egui::TexturesDelta) {
        *delta = egui::TexturesDelta::default();
    }

    fn render(
        &mut self,
        _ctx: &egui::Context,
        _output: &egui::FullOutput,
    ) -> Result<image::RgbaImage, String> {
        Err("frames are only timed".into())
    }
}

/// The app and how long it took for its frames (milliseconds).
struct Timed {
    app: BananaApp,
    took: Vec<f64>,
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

/// A 60 Hz screen's frames: one step, then the rest of its 16.7 ms.
fn paint(h: &mut Harness<'_, Timed>) {
    let start = Instant::now();
    h.step();
    let rest = Duration::from_micros(16_667).saturating_sub(start.elapsed());
    std::thread::sleep(rest);
}

/// Playing again when the project's end stopped it.
fn keep_playing(h: &mut Harness<'_, Timed>) {
    if !h.state().app.playing_for_review() {
        h.state_mut().app.seek_seconds(0.0);
        h.step();
        h.key_press(egui::Key::Space);
        let start = Instant::now();
        while !h.state().app.playing_for_review() && start.elapsed() < Duration::from_secs(6) {
            paint(h);
        }
    }
}

#[test]
#[ignore = "times the interface: an optimized build (cargo xtask perf)"]
fn the_interface_keeps_within_its_frame_budget() {
    set_language(Language::English);
    let mut app = BananaApp::new(egui::Context::default());
    app.welcome_for_review("closed");
    app.fx_packages_for_review();
    match std::env::var("BV_BUDGET_FILE") {
        Ok(path) => assert!(
            app.import_and_place(std::path::Path::new(&path)),
            "imported {path}"
        ),
        Err(_) => app.load_demo(),
    }
    let budget = std::env::var("BV_BUDGET_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(BUDGET_MS);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1920.0, 1040.0))
        .with_pixels_per_point(1.0)
        .renderer(Discard)
        .build_ui_state(
            |ui, s: &mut Timed| {
                apply_theme(ui.ctx());
                let start = Instant::now();
                s.app.frame(ui);
                s.took.push(start.elapsed().as_secs_f64() * 1e3);
            },
            Timed {
                app,
                took: Vec::new(),
            },
        );
    // The graphics card's kernels and the readers get ready first.
    for _ in 0..120 {
        paint(&mut h);
    }
    let mut rows: Vec<(String, f64, f64, f64, f64)> = Vec::new();
    for n in 0.. {
        // The preview in front (a window sharing its place may have hidden
        // it), then the window.
        h.state_mut().app.show_preview_for_review();
        let Some(title) = h.state_mut().app.show_window_for_review(n) else {
            break;
        };
        keep_playing(&mut h);
        for _ in 0..20 {
            paint(&mut h);
        }
        h.state_mut().took.clear();
        for _ in 0..120 {
            keep_playing(&mut h);
            paint(&mut h);
        }
        let mut took = std::mem::take(&mut h.state_mut().took);
        took.sort_by(f64::total_cmp);
        let rate = h.state().app.preview_rate_for_review();
        rows.push((
            title.to_string(),
            quantile(&took, 0.5),
            quantile(&took, 0.9),
            took.last().copied().unwrap_or(0.0),
            rate,
        ));
    }
    println!("window: median / 90% / slowest frame (ms), preview frames a second");
    for (title, median, p90, max, rate) in &rows {
        println!("  {title:<22} {median:5.2} / {p90:5.2} / {max:6.2}   {rate:5.1}");
    }
    let worst = rows
        .iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .expect("windows shown");
    println!(
        "slowest: {} at {:.2} ms (budget {budget} ms)",
        worst.0, worst.1
    );
    if cfg!(debug_assertions) {
        println!("(a debug build: not judged)");
        return;
    }
    assert!(
        worst.1 <= budget,
        "the interface takes {:.2} ms a frame with {} shown (budget {budget} ms)",
        worst.1,
        worst.0
    );
    // The frames with a new picture of the preview, or other work now and
    // then: within half the budget again.
    let spiky = rows
        .iter()
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .expect("windows shown");
    assert!(
        spiky.2 <= budget * 1.5,
        "a tenth of the interface's frames take over {:.2} ms with {} shown (budget {} ms)",
        spiky.2,
        spiky.0,
        budget * 1.5
    );
}
