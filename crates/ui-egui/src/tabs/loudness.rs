//! Loudness Meters (View › Window › Loudness Meters), as VEGAS's Help
//! describes them: the momentary (M, 400 ms), short-term (S, 3 s) and
//! integrated (I) loudness, the loudness range (LRA) and the true peaks of
//! what plays, against EBU R 128's target (−23 LUFS, −1 dBTP) or ATSC
//! A/85's (−24 LUFS, −2 dBTP); the statistics on the left, red where the
//! integrated loudness or the true peaks go over; the scale EBU +9 or
//! EBU +18, in LU or in LUFS. The options are the window's shortcut menu
//! and Options › Loudness Meters. The engine measures (`bv_engine`'s live
//! meter); the readings start again with each playback.

use bv_engine::Engine;
use bv_engine::LiveLoudness;

use crate::app::BananaApp;
use crate::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, pos2, vec2};
use crate::i18n::tr;
use crate::theme;

/// No reading (below every meter).
const NOTHING: f64 = -200.0;
/// The true peak meters' range (dBTP).
const PEAK_FLOOR: f64 = -60.0;

/// The readings shown: the last ones are kept between playbacks; the true
/// peak bars fall back slowly.
#[derive(Clone, Debug, Default)]
pub struct LoudnessView {
    last: Option<LiveLoudness>,
    peaks: Vec<f64>,
}

impl LoudnessView {
    /// Shows `r` as if it had been measured (the screen tests).
    pub(crate) fn show(&mut self, r: LiveLoudness) {
        self.peaks = r.true_peaks.clone();
        self.last = Some(r);
    }
}

fn color_of(lu: f64) -> Color32 {
    if lu > 0.0 {
        Color32::from_rgb(0xD9, 0x48, 0x3B)
    } else if lu > -2.0 {
        Color32::from_rgb(0xE6, 0xC2, 0x29)
    } else {
        Color32::from_rgb(0x3C, 0xB4, 0x4B)
    }
}

/// A value as the scale shows it: LU against the target, or LUFS.
fn text_of(v: f64, target: f64, absolute: bool) -> String {
    if v <= -70.0 {
        "—".into()
    } else if absolute {
        format!("{} LUFS", crate::fmt::num(v, 1))
    } else {
        format!("{} LU", crate::fmt::signed(v - target, 1))
    }
}

pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    let prefs = app.settings.loudness;
    let (target, limit) = prefs.targets();
    let (lo, hi) = prefs.range();
    let mut shown = std::mem::take(&mut app.loudness_view);
    if let Some(r) = Engine::loudness() {
        // Each channel's true peak: up at once, down 20 dB a second.
        let dt = ui.input(|i| i.stable_dt).min(0.1) as f64;
        shown.peaks.resize(r.true_peaks.len(), NOTHING);
        for (s, &now) in shown.peaks.iter_mut().zip(&r.true_peaks) {
            *s = if now >= *s {
                now
            } else {
                (*s - 20.0 * dt).max(now).max(NOTHING)
            };
        }
        shown.last = Some(r);
    }
    let r = shown.last.clone();
    let area = ui.available_rect_before_wrap();
    let resp = ui.allocate_rect(area, Sense::click());
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 0.0, theme::panel_dark());

    // The statistics.
    let stats_w = 210.0f32.min(area.width() * 0.45);
    let font = FontId::proportional(13.0);
    let value =
        |v: Option<f64>| v.map_or("—".to_string(), |v| text_of(v, target, prefs.absolute));
    let rows: [(&str, String, bool); 5] = [
        (
            tr("Momentary"),
            value(r.as_ref().map(|r| r.momentary)),
            false,
        ),
        (tr("Short"), value(r.as_ref().map(|r| r.short_term)), false),
        (
            tr("Integrated"),
            value(r.as_ref().map(|r| r.integrated)),
            r.as_ref()
                .is_some_and(|r| r.integrated > -70.0 && r.integrated > target),
        ),
        (
            tr("Loudness Range"),
            r.as_ref()
                .filter(|r| r.integrated > -70.0)
                .map_or("—".into(), |r| {
                    format!("{} LU", crate::fmt::num(r.range, 1))
                }),
            false,
        ),
        (
            tr("True Peaks"),
            r.as_ref()
                .filter(|r| r.true_peak > NOTHING)
                .map_or("—".into(), |r| {
                    format!("{} dBTP", crate::fmt::num(r.true_peak, 1))
                }),
            r.as_ref().is_some_and(|r| r.true_peak > limit),
        ),
    ];
    let mut y = area.top() + 10.0;
    for (label, text, over) in rows {
        painter.text(
            pos2(area.left() + 10.0, y),
            Align2::LEFT_TOP,
            label,
            font.clone(),
            theme::text_dim(),
        );
        let box_rect = Rect::from_min_size(
            pos2(area.left() + 10.0, y + 16.0),
            vec2(stats_w - 20.0, 22.0),
        );
        painter.rect_filled(
            box_rect,
            2.0,
            if over {
                Color32::from_rgb(0x8C, 0x22, 0x1A)
            } else {
                Color32::from_gray(0x14)
            },
        );
        painter.text(
            box_rect.right_center() - vec2(6.0, 0.0),
            Align2::RIGHT_CENTER,
            text,
            FontId::monospace(13.0),
            theme::text(),
        );
        y += 46.0;
    }
    let mode = if prefs.atsc {
        tr("ATSC A/85")
    } else {
        tr("EBU R 128")
    };
    painter.text(
        pos2(area.left() + 10.0, y + 2.0),
        Align2::LEFT_TOP,
        format!("{mode} · {}", text_of(target, target, true)),
        FontId::proportional(11.5),
        theme::text_dim(),
    );

    // The meters: M, S, I, LRA and the true peaks.
    let left = area.left() + stats_w + 34.0;
    let top = area.top() + 12.0;
    let bottom = area.bottom() - if prefs.labels { 26.0 } else { 10.0 };
    if bottom - top < 40.0 || area.right() - left < 60.0 {
        app.loudness_view = shown;
        resp.context_menu(|ui| options_items(ui, app));
        return;
    }
    let height = bottom - top;
    let y_of = |lu: f64| bottom - (((lu - lo) / (hi - lo)).clamp(0.0, 1.0) as f32) * height;
    // The scale, every 3 LU (6 on the wide one).
    let step = if prefs.wide { 6 } else { 3 };
    let mut lu = lo as i32;
    while lu <= hi as i32 {
        let yy = y_of(lu as f64);
        painter.hline(
            left - 6.0..=left - 2.0,
            yy,
            Stroke::new(1.0, theme::text_dim()),
        );
        let label = if prefs.absolute {
            crate::fmt::num(target + lu as f64, 0)
        } else if lu == 0 {
            "0".into()
        } else {
            crate::fmt::signed(lu as f64, 0)
        };
        painter.text(
            pos2(left - 10.0, yy),
            Align2::RIGHT_CENTER,
            label,
            FontId::proportional(10.0),
            theme::text_dim(),
        );
        lu += step;
    }
    let bar_w = 22.0;
    let gap = 10.0;
    let mut x = left;
    let mut bar = |label: &str, v: Option<f64>, hold: (Option<f64>, Option<f64>)| {
        let rect = Rect::from_min_max(pos2(x, top), pos2(x + bar_w, bottom));
        painter.rect_filled(rect, 0.0, Color32::from_gray(0x14));
        // The target's line.
        painter.hline(
            rect.x_range(),
            y_of(0.0),
            Stroke::new(1.0, Color32::from_gray(0x60)),
        );
        if let Some(v) = v.filter(|v| *v > -70.0) {
            let lu = v - target;
            let fill = Rect::from_min_max(
                pos2(rect.left() + 2.0, y_of(lu)),
                pos2(rect.right() - 2.0, bottom),
            );
            painter.rect_filled(fill, 0.0, color_of(lu));
        }
        let (peak, valley) = hold;
        for h in [peak, valley].into_iter().flatten().filter(|h| *h > -70.0) {
            painter.hline(
                rect.x_range(),
                y_of(h - target),
                Stroke::new(1.5, Color32::from_gray(0xE6)),
            );
        }
        if prefs.labels {
            painter.text(
                pos2(rect.center().x, bottom + 6.0),
                Align2::CENTER_TOP,
                label,
                FontId::proportional(11.0),
                theme::text(),
            );
        }
        x += bar_w + gap;
    };
    let holds = |peak: Option<f64>, valley: Option<f64>| {
        (
            peak.filter(|_| prefs.hold_peaks),
            valley.filter(|_| prefs.hold_valleys),
        )
    };
    bar(
        "M",
        r.as_ref().map(|r| r.momentary),
        holds(
            r.as_ref().map(|r| r.max_momentary),
            r.as_ref().map(|r| r.min_momentary),
        ),
    );
    bar(
        "S",
        r.as_ref().map(|r| r.short_term),
        holds(
            r.as_ref().map(|r| r.max_short_term),
            r.as_ref().map(|r| r.min_short_term),
        ),
    );
    bar("I", r.as_ref().map(|r| r.integrated), (None, None));
    // The loudness range: a band between the scale's bottom and its width.
    {
        let rect = Rect::from_min_max(pos2(x, top), pos2(x + bar_w, bottom));
        painter.rect_filled(rect, 0.0, Color32::from_gray(0x14));
        if let Some(range) = r.as_ref().filter(|r| r.integrated > -70.0).map(|r| r.range) {
            let h = ((range / (hi - lo)).clamp(0.0, 1.0) as f32) * height;
            let fill = Rect::from_min_max(
                pos2(rect.left() + 2.0, bottom - h),
                pos2(rect.right() - 2.0, bottom),
            );
            painter.rect_filled(fill, 0.0, Color32::from_rgb(0x4A, 0x90, 0xD9));
        }
        if prefs.labels {
            painter.text(
                pos2(rect.center().x, bottom + 6.0),
                Align2::CENTER_TOP,
                "LRA",
                FontId::proportional(11.0),
                theme::text(),
            );
        }
        x += bar_w + gap * 2.0;
    }
    // The true peaks, a meter a channel (−60 to 0 dBTP), the limit's line.
    if prefs.true_peak {
        let peak_y =
            |db: f64| bottom - (((db - PEAK_FLOOR) / -PEAK_FLOOR).clamp(0.0, 1.0) as f32) * height;
        let channels = shown.peaks.len().max(2);
        for c in 0..channels {
            let rect = Rect::from_min_max(pos2(x, top), pos2(x + bar_w * 0.7, bottom));
            painter.rect_filled(rect, 0.0, Color32::from_gray(0x14));
            painter.hline(
                rect.x_range(),
                peak_y(limit),
                Stroke::new(1.0, Color32::from_rgb(0xD9, 0x48, 0x3B)),
            );
            if let Some(&db) = shown.peaks.get(c).filter(|d| **d > PEAK_FLOOR) {
                let color = if db > limit {
                    Color32::from_rgb(0xD9, 0x48, 0x3B)
                } else {
                    Color32::from_rgb(0x3C, 0xB4, 0x4B)
                };
                let fill = Rect::from_min_max(
                    pos2(rect.left() + 1.0, peak_y(db)),
                    pos2(rect.right() - 1.0, bottom),
                );
                painter.rect_filled(fill, 0.0, color);
            }
            if prefs.labels {
                let name = match (channels, c) {
                    (2, 0) => "L".to_string(),
                    (2, 1) => "R".to_string(),
                    _ => (c + 1).to_string(),
                };
                painter.text(
                    pos2(rect.center().x, bottom + 6.0),
                    Align2::CENTER_TOP,
                    name,
                    FontId::proportional(11.0),
                    theme::text(),
                );
            }
            x += bar_w * 0.7 + 4.0;
        }
        if prefs.labels {
            painter.text(
                pos2(x - 2.0, top),
                Align2::LEFT_TOP,
                "TP",
                FontId::proportional(11.0),
                theme::text_dim(),
            );
        }
    }
    app.loudness_view = shown;
    resp.context_menu(|ui| options_items(ui, app));
}

/// Options › Loudness Meters, and the window's shortcut menu.
pub(crate) fn options_items(ui: &mut egui::Ui, app: &mut BananaApp) {
    if crate::menus::item(ui, tr("Reset Clip")) {
        Engine::reset_true_peak_clip();
        ui.close();
    }
    if crate::menus::item(ui, tr("Reset Meter Measurement")) {
        Engine::reset_loudness();
        ui.close();
    }
    ui.separator();
    let p = app.settings.loudness;
    let mut new = p;
    if crate::widgets::radio(ui, !p.atsc, tr("EBU R 128 Mode")).clicked() {
        new.atsc = false;
    }
    if crate::widgets::radio(ui, p.atsc, tr("ATSC A 85 Mode")).clicked() {
        new.atsc = true;
    }
    ui.separator();
    if crate::menus::toggle_item(ui, tr("True Peak Blocking Filter"), p.blocking) {
        new.blocking = !p.blocking;
    }
    crate::menus::sub(ui, tr("True Peak Range"), false, |_| {});
    crate::menus::sub(ui, tr("Loudness Scale"), true, |ui| {
        if crate::widgets::radio(ui, !p.wide, "EBU +9").clicked() {
            new.wide = false;
        }
        if crate::widgets::radio(ui, p.wide, "EBU +18").clicked() {
            new.wide = true;
        }
        ui.separator();
        if crate::menus::toggle_item(ui, tr("Absolute (-23 LUFS)"), p.absolute) {
            new.absolute = !p.absolute;
        }
    });
    ui.separator();
    if crate::menus::toggle_item(ui, tr("Show True Peak Meter"), p.true_peak) {
        new.true_peak = !p.true_peak;
    }
    if crate::menus::toggle_item(ui, tr("Show Labels"), p.labels) {
        new.labels = !p.labels;
    }
    if crate::menus::toggle_item(ui, tr("Hold Peaks"), p.hold_peaks) {
        new.hold_peaks = !p.hold_peaks;
    }
    if crate::menus::toggle_item(ui, tr("Hold Valleys"), p.hold_valleys) {
        new.hold_valleys = !p.hold_valleys;
    }
    if new != p {
        if new.blocking != p.blocking {
            Engine::set_true_peak_blocking(new.blocking);
        }
        app.settings.loudness = new;
        let _ = app.settings.save();
        ui.close();
    }
}
