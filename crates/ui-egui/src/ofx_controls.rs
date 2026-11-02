//! The controls of a plug-in's settings, as VEGAS draws an OFX effect's:
//! a slider with its number in a box beside it, the clock that animates a
//! parameter, and the box where a position is picked.

use crate::egui::{self, Color32, Rect, Response, Sense, Stroke, Ui, pos2, vec2};
use crate::theme;

/// The slider's, the number box's and the lists' widths.
pub const SLIDER_W: f32 = 145.0;
pub const FIELD_W: f32 = 145.0;
pub const CHOICE_W: f32 = 160.0;
/// The position box's side.
pub const PAD: f32 = 92.0;

/// A horizontal slider as VEGAS 23's: a dark groove, lit from zero (or the
/// end nearest it) to the value, and a white knob. A click or a drag puts
/// the value there (within `range`), a double click back to `reset`. With
/// `log` (and both of `range`'s ends above 0) it runs logarithmically: its
/// middle is their geometric mean. A value typed past its ends shows as
/// VEGAS shows it: a small dot at that end, no knob.
pub fn slider(
    ui: &mut Ui,
    value: &mut f64,
    range: (f64, f64),
    reset: f64,
    int: bool,
    label: &str,
    log: bool,
) -> Response {
    let h = ui.spacing().interact_size.y;
    let (rect, mut resp) = ui.allocate_exact_size(vec2(SLIDER_W, h), Sense::click_and_drag());
    let (lo, hi) = range;
    let log = log && lo > 0.0 && hi > lo;
    // A value's place along the slider (0–1), and back.
    let place = |v: f64| {
        if log {
            (v.max(lo) / lo).ln() / (hi / lo).ln()
        } else if hi > lo {
            (v - lo) / (hi - lo)
        } else {
            0.0
        }
    };
    let value_at = |f: f64| {
        if log {
            lo * (hi / lo).powf(f)
        } else {
            lo + f * (hi - lo)
        }
    };
    let knob_w = 12.0;
    let track = rect.shrink2(vec2(knob_w / 2.0, 0.0));
    let before = *value;
    if resp.double_clicked() {
        *value = reset;
    } else if (resp.dragged() || resp.clicked() || resp.drag_started())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let f = ((p.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
        let v = value_at(f);
        *value = if int { v.round() } else { v };
    }
    if *value != before {
        resp.mark_changed();
    }
    let shown = *value;
    resp.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), shown, label));
    let painter = ui.painter();
    let y = rect.center().y;
    let x = |f: f64| track.left() + f.clamp(0.0, 1.0) as f32 * track.width();
    // The groove, 6 points high, its inner top and left edges a little
    // lighter; the part from zero to the value lit (VEGAS's 20, 30 and 80
    // on its 40).
    let groove = Rect::from_min_max(pos2(track.left(), y - 3.0), pos2(track.right(), y + 3.0));
    let inner = groove.shrink(1.0);
    let edge = theme::panel_dark().lerp_to_gamma(theme::panel(), 0.5);
    let lit = theme::panel_dark().lerp_to_gamma(theme::text_strong(), 0.255);
    painter.rect_filled(groove, 1.0, theme::panel_dark());
    painter.hline(inner.x_range(), inner.top() + 0.5, Stroke::new(1.0, edge));
    painter.vline(inner.left() + 0.5, inner.y_range(), Stroke::new(1.0, edge));
    let zero = x(place(0.0_f64.clamp(lo.min(hi), hi.max(lo))));
    let at = pos2(x(place(*value)), y);
    let (a, b) = (
        zero.min(at.x).max(inner.left()),
        zero.max(at.x).min(inner.right()),
    );
    if b > a {
        painter.rect_filled(
            Rect::from_min_max(pos2(a, inner.top()), pos2(b, inner.bottom())),
            0.0,
            lit,
        );
        let lit_edge = Stroke::new(1.0, lit.lerp_to_gamma(edge, 0.5));
        painter.hline(a..=b, inner.top() + 0.5, lit_edge);
        if a <= inner.left() {
            painter.vline(inner.left() + 0.5, inner.y_range(), lit_edge);
        }
    }
    if *value < lo.min(hi) || *value > hi.max(lo) {
        painter.circle_filled(at, 3.5, Color32::from_gray(104));
        painter.circle_stroke(at, 3.5, Stroke::new(1.0, Color32::from_gray(64)));
    } else {
        // VEGAS's knob: white, outlined dark; greyed with its parameter.
        let knob = Rect::from_center_size(at, vec2(knob_w, h));
        let face = if ui.is_enabled() {
            theme::text_strong()
        } else {
            theme::text_disabled()
        };
        painter.rect_filled(knob, 0.0, face);
        painter.rect_stroke(
            knob,
            0.0,
            Stroke::new(1.0, theme::panel_dark()),
            egui::StrokeKind::Inside,
        );
    }
    resp
}

/// A box showing `text`: what is typed in it is given back when Enter is
/// pressed or the box is left (Escape drops it).
pub fn field(ui: &mut Ui, id: egui::Id, text: &str, width: f32, label: &str) -> Option<String> {
    let h = ui.spacing().interact_size.y;
    let editing: Option<String> = ui.data(|d| d.get_temp(id));
    let mut buf = editing.unwrap_or_else(|| text.to_string());
    let resp = ui.add_sized(
        vec2(width, h),
        egui::TextEdit::singleline(&mut buf)
            .id(id)
            .horizontal_align(egui::Align::LEFT),
    );
    let mut info = egui::WidgetInfo::text_edit(true, "", &buf, "");
    info.label = Some(label.to_string());
    resp.widget_info(|| info.clone());
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, buf));
        return None;
    }
    ui.data_mut(|d| d.remove::<String>(id));
    let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
    (resp.lost_focus() && !escaped && buf != text).then_some(buf)
}

/// VEGAS's Animate button: a clock with a + (click to animate) or a red ×
/// (animated: click to stop).
pub fn animate(ui: &mut Ui, on: bool, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 18.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, tip));
    let painter = ui.painter();
    let c = rect.center() + vec2(-2.0, -1.5);
    let r = 6.0;
    let enabled = ui.is_enabled();
    let face = if !enabled {
        Color32::from_gray(150)
    } else if resp.hovered() {
        theme::text_strong()
    } else {
        Color32::from_gray(214)
    };
    painter.circle_filled(c, r, face);
    let hand = Stroke::new(1.3, Color32::from_gray(48));
    painter.line_segment([c, c + vec2(0.0, -r * 0.7)], hand);
    painter.line_segment([c, c + vec2(r * 0.55, 0.0)], hand);
    // The mark at its foot: VEGAS's "no" sign while its parameter is
    // disabled.
    let m = rect.right_bottom() + vec2(-5.0, -4.5);
    let s = 3.0;
    if !enabled {
        let no = Stroke::new(1.4, face);
        painter.circle_stroke(m, s, no);
        painter.line_segment([m + vec2(-2.1, 2.1), m + vec2(2.1, -2.1)], no);
    } else if on {
        let red = Stroke::new(1.8, Color32::from_rgb(232, 64, 56));
        painter.line_segment([m + vec2(-s, -s), m + vec2(s, s)], red);
        painter.line_segment([m + vec2(-s, s), m + vec2(s, -s)], red);
    } else {
        let white = Stroke::new(1.6, face);
        painter.line_segment([m + vec2(-s, 0.0), m + vec2(s, 0.0)], white);
        painter.line_segment([m + vec2(0.0, -s), m + vec2(0.0, s)], white);
    }
    resp
}

/// The box where a position is picked: the frame, 0–1 across and up (its
/// middle at 0.5, 0.5), the position a crosshair. A click or a drag puts
/// it there, a double click back at `reset`.
pub fn pad(ui: &mut Ui, v: &mut [f64; 2], reset: [f64; 2], label: &str) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(PAD, PAD), Sense::click_and_drag());
    if resp.double_clicked() {
        if *v != reset {
            *v = reset;
            resp.mark_changed();
        }
    } else if (resp.dragged() || resp.clicked() || resp.drag_started())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let x = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
        let y = ((rect.bottom() - p.y) / rect.height()).clamp(0.0, 1.0) as f64;
        if [x, y] != *v {
            *v = [x, y];
            resp.mark_changed();
        }
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, label));
    let painter = ui.painter_at(rect.expand(1.0));
    painter.rect_filled(rect, 0.0, theme::panel_dark());
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, theme::separator()),
        egui::StrokeKind::Inside,
    );
    let c = pos2(
        rect.left() + v[0].clamp(0.0, 1.0) as f32 * rect.width(),
        rect.bottom() - v[1].clamp(0.0, 1.0) as f32 * rect.height(),
    );
    let ink = Stroke::new(1.5, Color32::from_gray(235));
    for d in [
        vec2(1.0, 0.0),
        vec2(-1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(0.0, -1.0),
    ] {
        painter.line_segment([c + d * 2.5, c + d * 9.0], ink);
    }
    resp
}
