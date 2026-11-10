//! Options › Preferences › Display in the main window: the interface
//! type, the scale, the docking area hidden until wanted, and the system's
//! statistics in the status bar.

use std::time::{Duration, Instant};

use super::BananaApp;
use crate::egui;
use crate::i18n::tr;
use crate::theme;

impl BananaApp {
    /// Options › Preferences › Display set as `look` says, on settings that
    /// are never saved (for screen tests): an interface type ("Dark",
    /// "Medium", "Light", "White"), and any of "tabs at top", "statistics"
    /// and "hide dock".
    pub fn display_for_review(&mut self, look: &[&str]) {
        self.settings = crate::settings::UiSettings::in_memory();
        let p = &mut self.settings.prefs;
        for &what in look {
            match what {
                "Dark" => p.interface = theme::Interface::Dark,
                "Medium" => p.interface = theme::Interface::Medium,
                "Light" => p.interface = theme::Interface::Light,
                "White" => p.interface = theme::Interface::White,
                "tabs at top" => p.tabs_at_top = true,
                "statistics" => p.system_stats = true,
                "hide dock" => p.auto_hide_dock = true,
                _ => panic!("unknown look: {what}"),
            }
        }
    }

    /// Options › Preferences › Display: the interface type, the active
    /// buttons' and the icons' colors, and the scale (with "Use High DPI
    /// scaling" off, one screen pixel per point, as on a 100 % display).
    pub(super) fn apply_look(&mut self, ctx: &egui::Context) {
        let p = &self.settings.prefs;
        crate::envelope_look::set(p.envelope_look);
        theme::set_button_highlight(p.button_highlight);
        theme::set_icon_strength(p.icon_strength);
        // The brightness the Preferences window shows while it is open (its
        // slider's), else the one kept.
        let brightness = self
            .look_preview
            .unwrap_or(p.interface_brightness)
            .clamp(-40, 40);
        let look = (p.interface, brightness);
        if self.look_applied != Some(look)
            || theme::interface() != p.interface
            || theme::brightness() != brightness
        {
            theme::set_interface(p.interface);
            theme::set_brightness(brightness);
            theme::apply_kind(ctx, p.interface);
            self.look_applied = Some(look);
        }
        let native = ctx
            .input(|i| i.viewport().native_pixels_per_point)
            .unwrap_or(1.0)
            .max(0.25);
        let zoom = if p.high_dpi { 1.0 } else { 1.0 / native };
        if self.zoom_applied != Some(zoom) {
            ctx.set_zoom_factor(zoom);
            self.zoom_applied = Some(zoom);
        }
    }

    /// "Automatically hide docking area", hidden: a thin strip on its side
    /// brings it back, pointed at when it is at the bottom of the window,
    /// double-clicked when it is at the top (by the menus), as in VEGAS.
    pub(super) fn dock_strip(&mut self, ui: &mut egui::Ui) {
        let above = !self.timeline_top;
        let panel = if above {
            egui::Panel::top("dock-strip")
        } else {
            egui::Panel::bottom("dock-strip")
        };
        panel
            .exact_size(6.0)
            .frame(egui::Frame::new().fill(theme::panel_dark()))
            .show(ui, |ui| {
                let rect = ui.max_rect();
                let resp = ui.interact(rect, egui::Id::new("dock-strip"), egui::Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(rect, 0.0, theme::banana_dim());
                }
                let show = if above {
                    resp.double_clicked()
                } else {
                    resp.hovered()
                };
                if show {
                    self.dock_peek = true;
                    self.dock_away_since = None;
                }
                let tip = if above {
                    tr("Double-click to show the docking area")
                } else {
                    tr("Point here to show the docking area")
                };
                // Named by its tip for screen readers (and the tests).
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
                resp.on_hover_text(tip);
            });
    }

    /// Once shown, the hidden docking area hides again when the pointer
    /// has been away from it for a moment (not while a button is held or a
    /// menu is open: a drag from it may be on its way).
    /// `area` holds the docking area and the timeline's window
    /// (`timeline`), which is not the docking area.
    pub(super) fn follow_dock_peek(
        &mut self,
        ctx: &egui::Context,
        area: egui::Rect,
        timeline: Option<egui::Rect>,
    ) {
        if !self.dock_peek {
            return;
        }
        let now = ctx.input(|i| i.time);
        let (pointer, held) = ctx.input(|i| (i.pointer.latest_pos(), i.pointer.any_down()));
        let inside = pointer.is_some_and(|p| {
            area.expand(4.0).contains(p) && !timeline.is_some_and(|t| t.shrink(4.0).contains(p))
        });
        let busy = held || self.media_drag.is_some() || egui::Popup::is_any_open(ctx);
        if inside || busy {
            self.dock_away_since = None;
            return;
        }
        let since = *self.dock_away_since.get_or_insert(now);
        if now - since > 0.8 {
            self.dock_peek = false;
            self.dock_away_since = None;
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    /// The status bar's "Record Time (2 channels): 3:16:31:15": how long a
    /// stereo recording can go on in the recordings' folder, at the
    /// project's sample rate and depth (hours:minutes:seconds:frames).
    /// Looked up every few seconds.
    pub(super) fn record_time_text(&mut self) -> Option<String> {
        let fresh = self
            .record_time
            .as_ref()
            .is_some_and(|(at, _)| at.elapsed() < Duration::from_secs(5));
        if !fresh {
            let s = self.project().settings;
            let free = bv_platform::free_space(&self.recorded_folder());
            let text = free.map(|bytes| {
                let per_second = s.sample_rate.0 as u64 * 2 * (s.bit_depth.max(8) as u64 / 8);
                let seconds = bytes / per_second.max(1);
                let rate = s.frame_rate.as_f64().round().max(1.0) as u64;
                let frames = ((bytes % per_second.max(1)) * rate) / per_second.max(1);
                trf!(
                    "Record Time (2 channels): {}:{:02}:{:02}:{:02}",
                    seconds / 3600,
                    seconds / 60 % 60,
                    seconds % 60,
                    frames
                )
            });
            self.record_time = Some((Instant::now(), text));
        }
        self.record_time.as_ref().and_then(|(_, t)| t.clone())
    }

    /// "Show system statistics in the status bar": measured once a second.
    pub(super) fn update_stats(&mut self, ctx: &egui::Context) {
        if !self.settings.prefs.system_stats {
            self.stats = None;
            return;
        }
        let s = self.stats.get_or_insert_with(|| SystemStats {
            usage: bv_platform::Usage::new(),
            text: String::new(),
            at: None,
        });
        if s.at.is_none_or(|at| at.elapsed() >= Duration::from_secs(1)) {
            let (cpu, gpu, memory) = (s.usage.cpu(), s.usage.gpu(), s.usage.memory());
            s.text = stats_text(cpu, gpu, memory);
            s.at = Some(Instant::now());
        }
        ctx.request_repaint_after(Duration::from_secs(1));
    }
}

/// Options › Preferences › Display › "Show system statistics in the status
/// bar": what it measures, and its last text.
#[derive(Debug)]
pub(super) struct SystemStats {
    usage: bv_platform::Usage,
    pub(super) text: String,
    at: Option<Instant>,
}

/// "CPU: 12 % · GPU: 3 % · Memory: 1.25 GB" (a dash for what is not known
/// yet).
fn stats_text(cpu: Option<f64>, gpu: Option<f64>, memory: Option<u64>) -> String {
    let share = |v: Option<f64>| v.map_or("—".to_string(), |v| crate::fmt::num(v, 0));
    let memory = memory.map_or("—".to_string(), |b| crate::fmt::bytes(b as f64));
    format!(
        "{} · {} · {}",
        trf!("CPU: {} %", share(cpu)),
        trf!("GPU: {} %", share(gpu)),
        trf!("Memory: {}", memory)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn harness(app: BananaApp) -> Harness<'static, BananaApp> {
        Harness::builder()
            .with_size(egui::vec2(1280.0, 800.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            )
    }

    #[test]
    fn statistics_read_as_the_status_bar_shows_them() {
        crate::i18n::set_language(crate::i18n::Language::English);
        assert_eq!(
            stats_text(Some(12.4), None, Some(3 << 29)),
            "CPU: 12 % · GPU: — % · Memory: 1.50 GB"
        );
    }

    #[test]
    fn the_hidden_docking_area_shows_on_a_double_click_and_hides_when_left() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.settings.prefs.auto_hide_dock = true;
        let mut h = harness(app);
        h.run_steps(4);
        // Hidden: no docked window, the strip in its place.
        assert!(h.query_by_label("Up One Folder").is_none());
        let strip = h.get_by_label("Double-click to show the docking area");
        let at = strip.rect().center();
        for _ in 0..2 {
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
        h.run_steps(3);
        assert!(h.state().dock_peek, "shown");
        assert!(h.query_by_label("Up One Folder").is_some());
        // The pointer goes down to the timeline: a moment later it hides.
        h.event(egui::Event::PointerMoved(egui::pos2(640.0, 760.0)));
        h.run_steps(70);
        assert!(!h.state().dock_peek, "hidden again");
        assert!(h.query_by_label("Up One Folder").is_none());
    }

    #[test]
    fn the_interface_type_and_the_scale_reach_egui() {
        let mut app = BananaApp::new(egui::Context::default());
        app.settings.prefs.interface = theme::Interface::White;
        app.settings.prefs.high_dpi = false;
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 800.0))
            .with_pixels_per_point(1.5)
            .build_ui_state(|ui, app: &mut BananaApp| app.frame(ui), app);
        h.run_steps(2);
        let ctx = h.ctx.clone();
        let fill = ctx.global_style().visuals.panel_fill;
        assert_eq!(fill, theme::Interface::White.palette().panel);
        // One screen pixel per point, as on a 100 % display.
        assert!(
            (ctx.pixels_per_point() - 1.0).abs() < 1e-3,
            "{}",
            ctx.pixels_per_point()
        );
        h.state_mut().settings.prefs.interface = theme::Interface::Dark;
        h.state_mut().settings.prefs.high_dpi = true;
        h.run_steps(2);
        assert_eq!(
            ctx.global_style().visuals.panel_fill,
            theme::Interface::Dark.palette().panel
        );
        assert!((ctx.pixels_per_point() - 1.5).abs() < 1e-3);
    }
}
