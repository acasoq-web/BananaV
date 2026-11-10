//! Controles próprios: botões de ícone, fader vertical, medidor de nível e
//! roda de cor.

use crate::egui::{self, Color32, Rect, Response, Sense, Stroke, Ui, pos2, vec2};
use crate::i18n::tr;
use crate::icons::{self, Icon};
use crate::theme;

pub(crate) const BUTTON: egui::Vec2 = egui::Vec2::new(26.0, 24.0);

/// Where the app keeps the joystick's push in the context's data (see
/// `BananaApp::follow_joystick`).
pub const JOYSTICK: &str = "bv-joystick";

/// Options › Preferences › General: "Enable joystick support": how far the
/// joystick moves the control under the mouse pointer this frame (x to the
/// right, y up; a full push moves one unit a second).
pub fn joystick_push(ui: &Ui, resp: &Response) -> Option<egui::Vec2> {
    if !(resp.hovered() || resp.dragged()) {
        return None;
    }
    let v: egui::Vec2 = ui.ctx().data(|d| d.get_temp(egui::Id::new(JOYSTICK)))?;
    if v == egui::Vec2::ZERO {
        return None;
    }
    let dt = ui.input(|i| i.stable_dt).clamp(0.0, 0.1);
    Some(v * dt)
}

/// A joystick at rest is seldom exactly in the middle: a dead zone there,
/// and the rest of the way scaled to reach 1 again.
pub fn joystick_dead_zone(v: egui::Vec2) -> egui::Vec2 {
    const DEAD: f32 = 0.15;
    let len = v.length();
    if len <= DEAD {
        egui::Vec2::ZERO
    } else {
        v * (((len - DEAD) / (1.0 - DEAD)).min(1.0) / len)
    }
}

/// Styles check boxes and radio buttons so their box stands out: a fill
/// darker than the panels, a border that reads on them and a thick mark.
/// Plain egui ones almost vanish on dark panels (a dark grey square
/// without border).
fn check_style(ui: &mut Ui) {
    let w = &mut ui.visuals_mut().widgets;
    w.inactive.bg_fill = theme::panel_dark();
    w.inactive.bg_stroke = Stroke::new(1.0, theme::check_border());
    w.inactive.fg_stroke = Stroke::new(2.0, theme::text());
    for v in [&mut w.hovered, &mut w.active] {
        v.bg_fill = theme::hover();
        v.bg_stroke = Stroke::new(1.0, theme::banana());
        v.fg_stroke = Stroke::new(2.0, theme::text_strong());
    }
}

/// A double-click on `resp`, clicks counted in pairs: a click soon after
/// another near it, unless that one already ended a pair. egui counts a
/// quick third click as a triple-click, so a second double-click soon after
/// a first one elsewhere (two envelope points in a row) was lost; here it
/// is a double-click of its own. Asked again in the same frame (another
/// widget looking at the same click), the same answer.
pub(crate) fn double_click(ui: &Ui, resp: &Response) -> bool {
    if !resp.clicked() {
        return false;
    }
    resp.interact_pointer_pos()
        .is_some_and(|at| click_pairs(ui, at))
}

/// [`double_click`] for a click of the main button anywhere (what reads
/// the mouse itself rather than a widget's response).
pub(crate) fn double_click_here(ui: &Ui) -> bool {
    let (clicked, at) = ui.input(|i| {
        (
            i.pointer.button_clicked(egui::PointerButton::Primary),
            i.pointer.interact_pos(),
        )
    });
    clicked && at.is_some_and(|at| click_pairs(ui, at))
}

/// Notes a click at `at`: whether it ends a pair.
fn click_pairs(ui: &Ui, at: egui::Pos2) -> bool {
    let now = ui.input(|i| i.time);
    let pass = ui.ctx().cumulative_pass_nr();
    let (delay, near) = ui.ctx().options(|o| {
        (
            o.input_options.max_double_click_delay,
            o.input_options.max_click_dist,
        )
    });
    let id = egui::Id::new("bv-double-click");
    // The last click: when, where, whether it ended a pair, and in which
    // frame.
    let last: Option<(f64, egui::Pos2, bool, u64)> = ui.ctx().data(|d| d.get_temp(id));
    if let Some((t, _, double, at_pass)) = last
        && at_pass == pass
        && t == now
    {
        return double;
    }
    let double = pairs(last.map(|(t, p, d, _)| (t, p, d)), (now, at), delay, near);
    ui.ctx()
        .data_mut(|d| d.insert_temp(id, (now, at, double, pass)));
    double
}

/// Whether a click at `now` (time, place) ends a pair with the click before
/// (time, place, whether it ended one itself).
fn pairs(
    before: Option<(f64, egui::Pos2, bool)>,
    now: (f64, egui::Pos2),
    delay: f64,
    near: f32,
) -> bool {
    before.is_some_and(|(t, p, ended)| !ended && now.0 - t <= delay && p.distance(now.1) <= near)
}

/// Whether `slider`'s bar was double-clicked this frame, to put its
/// default back as VEGAS's sliders do: egui's slider senses drags only, so
/// its response never says it was clicked. `value_shown`: its number box
/// is part of the response, after the bar.
pub fn bar_double_clicked(ui: &Ui, slider: &Response, value_shown: bool) -> bool {
    let mut bar = slider.rect;
    if value_shown {
        bar.set_width(ui.spacing().slider_width.min(bar.width()));
    }
    ui.is_enabled()
        && slider.contains_pointer()
        && ui.input(|i| {
            let primary = egui::PointerButton::Primary;
            // A third click soon after counts too (egui counts it apart).
            (i.pointer.button_double_clicked(primary) || i.pointer.button_triple_clicked(primary))
                && i.pointer.interact_pos().is_some_and(|p| bar.contains(p))
        })
}

/// A check box with a clearly visible box.
pub fn checkbox(ui: &mut Ui, checked: &mut bool, text: impl Into<egui::WidgetText>) -> Response {
    ui.scope(|ui| {
        check_style(ui);
        ui.checkbox(checked, text)
    })
    .inner
}

/// A radio button with a clearly visible circle.
pub fn radio(ui: &mut Ui, selected: bool, text: impl Into<egui::WidgetText>) -> Response {
    ui.scope(|ui| {
        check_style(ui);
        ui.radio(selected, text)
    })
    .inner
}

/// Radio button that sets `current` to `alternative` when clicked.
pub fn radio_value<V: PartialEq>(
    ui: &mut Ui,
    current: &mut V,
    alternative: V,
    text: impl Into<egui::WidgetText>,
) -> Response {
    ui.scope(|ui| {
        check_style(ui);
        ui.radio_value(current, alternative, text)
    })
    .inner
}

/// Paints a check box (or radio circle) inside `rect`, for custom-drawn
/// items such as menu entries.
pub fn paint_check_box(ui: &Ui, rect: Rect, on: bool, radio: bool, enabled: bool) {
    let painter = ui.painter();
    let border = if enabled {
        theme::check_border()
    } else {
        theme::text_disabled()
    };
    let mark = if enabled {
        theme::text()
    } else {
        theme::text_disabled()
    };
    let c = rect.center();
    if radio {
        painter.circle(c, 5.5, theme::panel_dark(), Stroke::new(1.0, border));
        if on {
            painter.circle_filled(c, 3.0, mark);
        }
    } else {
        let b = Rect::from_center_size(c, vec2(12.0, 12.0));
        painter.rect(
            b,
            2.0,
            theme::panel_dark(),
            Stroke::new(1.0, border),
            egui::StrokeKind::Inside,
        );
        if on {
            painter.add(egui::Shape::line(
                vec![
                    pos2(b.left() + 2.5, c.y),
                    pos2(c.x - 0.5, b.bottom() - 3.0),
                    pos2(b.right() - 2.5, b.top() + 2.5),
                ],
                Stroke::new(2.0, mark),
            ));
        }
    }
}

fn draw_button(ui: &Ui, rect: Rect, resp: &Response, icon: Icon, on: bool, enabled: bool) {
    let painter = ui.painter();
    if on {
        // Options › Preferences › Display › "Button Highlight Color".
        let fill = theme::button_highlight();
        painter.rect_filled(rect, 3.0, fill);
        painter.rect_stroke(
            rect,
            3.0,
            Stroke::new(1.0, fill.lerp_to_gamma(theme::banana(), 0.35)),
            egui::StrokeKind::Inside,
        );
    } else if resp.hovered() && enabled {
        painter.rect_filled(rect, 3.0, theme::hover());
    }
    let color = if !enabled {
        theme::text_disabled()
    } else if on {
        theme::banana()
    } else {
        theme::text()
    };
    icons::paint(painter, rect.shrink(5.0), icon, color);
}

/// Botão com ícone e dica.
pub fn icon_button(ui: &mut Ui, icon: Icon, tip: &str) -> Response {
    icon_button_ex(ui, icon, tip, false, true)
}

/// Botão de ícone que liga/desliga (fica destacado quando ligado).
pub fn icon_toggle(ui: &mut Ui, icon: Icon, on: bool, tip: &str) -> Response {
    icon_button_ex(ui, icon, tip, on, true)
}

pub fn icon_button_ex(ui: &mut Ui, icon: Icon, tip: &str, on: bool, enabled: bool) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(BUTTON, sense);
    if ui.is_rect_visible(rect) {
        draw_button(ui, rect, &resp, icon, on, enabled);
    }
    // Named by its tip for screen readers (and the screen tests).
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));
    resp.on_hover_text(tip)
}

/// The width of a pane left of a splitter that can be dragged (VEGAS's
/// windows split into a tree and a list): `default` until it is dragged,
/// kept by `id` meanwhile, within `min` and what leaves `room` of `area`
/// on its right. Draws and handles the splitter (4 points wide) at the
/// pane's right edge.
pub fn pane_width(ui: &Ui, id: egui::Id, area: Rect, default: f32, min: f32, room: f32) -> f32 {
    let max = (area.width() - room).max(min);
    let mut w = ui
        .data(|d| d.get_temp::<f32>(id))
        .unwrap_or(default)
        .clamp(min, max);
    let bar = Rect::from_min_max(
        pos2(area.left() + w, area.top()),
        pos2(area.left() + w + 4.0, area.bottom()),
    );
    let resp = ui.interact(bar, id.with("splitter"), Sense::drag());
    if resp.hovered() || resp.dragged() {
        // A split bar, as VEGAS's.
        crate::cursors::set(ui.ctx(), crate::cursors::Cursor::SplitterEw);
    }
    if resp.dragged() {
        w = (w + resp.drag_delta().x).clamp(min, max);
        ui.data_mut(|d| d.insert_temp(id, w));
    }
    let color = if resp.hovered() || resp.dragged() {
        theme::banana_dim()
    } else {
        theme::panel_dark()
    };
    ui.painter().rect_filled(
        Rect::from_min_max(
            pos2(area.left() + w, area.top()),
            pos2(area.left() + w + 4.0, area.bottom()),
        ),
        0.0,
        color,
    );
    w
}

/// The narrowest a zoom scroll bar's thumb gets: however long what it
/// scrolls, it never disappears.
pub(crate) const MIN_THUMB: f32 = 24.0;
/// How near a thumb's end the pointer takes that end (to zoom).
const THUMB_END: f32 = 5.0;

/// The part of a zoom scroll bar's thumb the pointer took.
#[derive(Clone, Copy, Debug, PartialEq)]
enum BarPart {
    Start,
    Middle,
    End,
}

/// A zoom scroll bar's grip: the part taken, where, and the view then.
#[derive(Clone, Copy, Debug)]
struct BarGrab {
    part: BarPart,
    x: f32,
    start: f64,
    len: f64,
    total: f64,
    /// The thumb's width and the room it moves in, in points.
    width: f32,
    travel: f32,
}

/// Where a zoom scroll bar's thumb is along its `track` points when the
/// part `start..start + len` of `0..total` shows: its offset and width.
fn thumb_span(track: f32, start: f64, len: f64, total: f64) -> (f32, f32) {
    let total = total.max(len).max(f64::MIN_POSITIVE);
    let width = ((len / total) as f32 * track).clamp(MIN_THUMB.min(track), track.max(0.0));
    let range = total - len;
    let travel = track - width;
    let at = if range > 0.0 {
        (start / range).clamp(0.0, 1.0) as f32 * travel
    } else {
        0.0
    };
    (at, width)
}

/// The view (start, length) a zoom scroll bar's `grab` makes `dx` points
/// along: the middle scrolls it; an end zooms it, following the pointer
/// while the other end stays (lengths within `lens`).
fn bar_drag(g: &BarGrab, dx: f32, lens: (f64, f64)) -> (f64, f64) {
    let zoomed = |w: f32| (g.len * (w / g.width) as f64).clamp(lens.0, lens.1);
    match g.part {
        BarPart::Middle => {
            let range = (g.total - g.len).max(0.0);
            let start = if g.travel > 0.0 {
                g.start + (dx / g.travel) as f64 * range
            } else {
                g.start
            };
            (start.clamp(0.0, range), g.len)
        }
        BarPart::End => (g.start, zoomed(g.width + dx)),
        BarPart::Start => {
            let end = g.start + g.len;
            let start = (end - zoomed(g.width - dx)).max(0.0);
            (start, (end - start).clamp(lens.0, lens.1))
        }
    }
}

/// A horizontal scroll bar as VEGAS's timelines have: its thumb is the
/// part `start..start + len` of `0..total` that shows, never thinner than
/// [`MIN_THUMB`]. Dragged by the middle it scrolls; by an end it zooms,
/// that end following the pointer while the other stays (`lens`: the
/// shortest and the longest part that can show); a click beside it brings
/// the view there. Returns the new start and length when they change.
pub(crate) fn zoom_scrollbar(
    ui: &mut Ui,
    rect: Rect,
    id: egui::Id,
    (start, len): (f64, f64),
    total: f64,
    lens: (f64, f64),
) -> Option<(f64, f64)> {
    let (at, width) = thumb_span(rect.width(), start, len, total);
    let thumb = Rect::from_min_size(
        pos2(rect.left() + at, rect.top()),
        vec2(width, rect.height()),
    );
    let part_at = |x: f32| {
        if (x - thumb.left()).abs() <= THUMB_END {
            Some(BarPart::Start)
        } else if (x - thumb.right()).abs() <= THUMB_END {
            Some(BarPart::End)
        } else if thumb.x_range().contains(x) {
            Some(BarPart::Middle)
        } else {
            None
        }
    };
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, tr("Scroll bar"))
    });
    if resp.drag_started() {
        let x = ui
            .input(|i| i.pointer.press_origin())
            .map_or(thumb.center().x, |p| p.x);
        if let Some(part) = part_at(x) {
            let grab = BarGrab {
                part,
                x,
                start,
                len,
                total,
                width,
                travel: rect.width() - width,
            };
            ui.data_mut(|d| d.insert_temp(id, grab));
        }
    }
    let grab = ui
        .data(|d| d.get_temp::<BarGrab>(id))
        .filter(|_| resp.dragged());
    let mut out = None;
    if let (Some(g), Some(p)) = (grab, resp.interact_pointer_pos()) {
        let view = bar_drag(&g, p.x - g.x, lens);
        out = (view != (start, len)).then_some(view);
    }
    if resp.drag_stopped() {
        ui.data_mut(|d| d.remove::<BarGrab>(id));
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
        && part_at(p.x).is_none()
    {
        // Beside the thumb: its middle goes to the click.
        let travel = rect.width() - width;
        let range = (total - len).max(0.0);
        if travel > 0.0 {
            let s = ((p.x - rect.left() - width / 2.0) / travel) as f64 * range;
            out = Some((s.clamp(0.0, range), len));
        }
    }
    let part = grab
        .map(|g| g.part)
        .or_else(|| resp.hover_pos().and_then(|p| part_at(p.x)));
    if matches!(part, Some(BarPart::Start | BarPart::End)) {
        // The thumb's ends, as VEGAS's scroll bars': ↔.
        crate::cursors::set(ui.ctx(), crate::cursors::Cursor::ResizeEw);
    }
    let painter = ui.painter_at(rect);
    let body = thumb.shrink2(vec2(0.0, 2.0));
    let hot = resp.hovered() || resp.dragged();
    painter.rect_filled(
        body,
        3.0,
        if hot {
            theme::text_dim()
        } else {
            theme::separator()
        },
    );
    // Its ends, which zoom.
    for (x, end) in [
        (body.left() + 2.0, BarPart::Start),
        (body.right() - 2.0, BarPart::End),
    ] {
        let color = if part == Some(end) {
            theme::text_strong()
        } else {
            theme::panel_dark()
        };
        painter.vline(
            x,
            body.top() + 2.0..=body.bottom() - 2.0,
            Stroke::new(2.0, color),
        );
    }
    out
}

/// Separador vertical fino para barras de ferramentas.
pub fn toolbar_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 20.0), Sense::hover());
    ui.painter().line_segment(
        [rect.center_top(), rect.center_bottom()],
        Stroke::new(1.0, theme::separator()),
    );
}

/// Converte dB numa posição 0–1 do fader (escala que dá mais espaço perto de 0 dB).
pub(crate) fn db_to_pos(db: f32, min: f32, max: f32) -> f32 {
    let db = db.clamp(min, max);
    ((db - min) / (max - min)).powf(2.0)
}

pub(crate) fn pos_to_db(p: f32, min: f32, max: f32) -> f32 {
    min + p.clamp(0.0, 1.0).sqrt() * (max - min)
}

/// Fader vertical em dB. Duplo clique volta a 0 dB.
pub fn vertical_fader(ui: &mut Ui, db: &mut f32, min: f32, max: f32, height: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, height), Sense::click_and_drag());
    fader_body(ui, rect, resp, db, min, max, 2, None)
}

/// [`vertical_fader`] in `rect`, its knob of `squares` light squares (one
/// for a channel's, the Mixing Console's strips; two for a stereo pair's,
/// the Master's) or of `tint` (VEGAS's red for an automation control).
#[allow(clippy::too_many_arguments)]
pub fn vertical_fader_in(
    ui: &mut Ui,
    rect: Rect,
    id: egui::Id,
    db: &mut f32,
    min: f32,
    max: f32,
    squares: u8,
    tint: Option<Color32>,
) -> Response {
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    fader_body(ui, rect, resp, db, min, max, squares, tint)
}

#[allow(clippy::too_many_arguments)]
fn fader_body(
    ui: &mut Ui,
    rect: Rect,
    mut resp: Response,
    db: &mut f32,
    min: f32,
    max: f32,
    squares: u8,
    tint: Option<Color32>,
) -> Response {
    let track = Rect::from_center_size(rect.center(), vec2(4.0, rect.height() - 16.0));
    if resp.dragged()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let t = 1.0 - (p.y - track.top()) / track.height();
        *db = pos_to_db(t, min, max);
        resp.mark_changed();
    }
    if resp.double_clicked() {
        *db = 0.0;
        resp.mark_changed();
    }
    let painter = ui.painter();
    // As VEGAS's: a thin track, dark above the knob and grey below it, and
    // a light knob of two squares, each with a line across.
    let y = track.bottom() - db_to_pos(*db, min, max) * track.height();
    let x = rect.center().x;
    let line = |top: f32, bottom: f32, color: Color32| {
        painter.rect_filled(
            Rect::from_min_max(pos2(x - 1.0, top), pos2(x + 1.0, bottom)),
            0.0,
            color,
        );
    };
    line(track.top(), y, theme::panel_dark());
    line(y, track.bottom(), Color32::from_gray(0x78));
    let hot = resp.hovered() || resp.dragged();
    let fill = match (tint, hot) {
        (Some(t), false) => t,
        (Some(t), true) => t.lerp_to_gamma(Color32::WHITE, 0.25),
        (None, true) => Color32::WHITE,
        (None, false) => Color32::from_gray(0xEE),
    };
    let edge = Stroke::new(1.0, Color32::from_gray(0x22));
    if squares < 2 {
        // One square with two lines across, as VEGAS's channel strips.
        let knob = Rect::from_center_size(pos2(x, y.round() + 0.5), vec2(14.0, 13.0));
        painter.rect(knob, 0.0, fill, edge, egui::StrokeKind::Inside);
        for dy in [-2.0, 2.0] {
            painter.hline(
                knob.left() + 3.0..=knob.right() - 3.0,
                knob.center().y + dy,
                Stroke::new(1.5, Color32::from_gray(0x22)),
            );
        }
        return resp.on_hover_text(trf!(
            "{} dB (double-click: 0 dB)",
            crate::fmt::signed(*db as f64, 1)
        ));
    }
    let knob = Rect::from_center_size(pos2(x, y), vec2(24.0, 12.0));
    for half in [
        Rect::from_min_max(knob.min, pos2(x, knob.bottom())),
        Rect::from_min_max(pos2(x, knob.top()), knob.max),
    ] {
        painter.rect(half, 0.0, fill, edge, egui::StrokeKind::Inside);
        painter.hline(
            half.left() + 3.0..=half.right() - 3.0,
            half.center().y,
            edge,
        );
    }
    resp.on_hover_text(trf!(
        "{} dB (double-click: 0 dB)",
        crate::fmt::signed(*db as f64, 1)
    ))
}

pub(crate) fn hue_rgb(angle: f32) -> [f32; 3] {
    // HSV com S = V = 1; ângulo em radianos (0 = vermelho, 2π/3 = verde).
    let h = (angle.rem_euclid(std::f32::consts::TAU)) / std::f32::consts::TAU * 6.0;
    let x = 1.0 - ((h % 2.0) - 1.0).abs();
    match h as u32 {
        0 => [1.0, x, 0.0],
        1 => [x, 1.0, 0.0],
        2 => [0.0, 1.0, x],
        3 => [0.0, x, 1.0],
        4 => [x, 0.0, 1.0],
        _ => [1.0, 0.0, x],
    }
}

/// A Color Grading wheel's radius in its points' units: VEGAS's wheels
/// are 170 pixels across, and their points are kept in those pixels.
pub(crate) const WHEEL_RADIUS: f64 = 85.0;

/// What happened to a Color Grading wheel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WheelChange {
    /// Its point moved (across, up: pixels of an 85-pixel wheel).
    Moved([f64; 2]),
    /// A double-click: the point back in the middle, the color at its
    /// default.
    Reset,
}

/// `p` kept within the wheel: past its edge, on it, the same way round.
fn on_wheel(p: [f64; 2]) -> [f64; 2] {
    let r = p[0].hypot(p[1]);
    if r > WHEEL_RADIUS {
        [p[0] / r * WHEEL_RADIUS, p[1] / r * WHEEL_RADIUS]
    } else {
        p
    }
}

/// The mouse wheel's notches over a widget this frame, and the keys held
/// with them (Ctrl, Alt, Shift).
fn notches_with_keys(events: &[egui::Event]) -> (f32, egui::Modifiers) {
    let mut notches = 0.0;
    let mut keys = egui::Modifiers::NONE;
    for e in events {
        if let egui::Event::MouseWheel {
            unit,
            delta,
            modifiers,
            ..
        } = e
        {
            // Shift may turn the wheel sideways: either way counts.
            let d = if delta.y != 0.0 { delta.y } else { delta.x };
            notches += match unit {
                egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => d,
                egui::MouseWheelUnit::Point => d / 40.0,
            };
            keys |= *modifiers;
        }
    }
    (notches, keys)
}

/// A Color Grading wheel as VEGAS's, `size` across: a dark ball — lighter
/// in the middle — in a ring of the colors its point gives (`color_at`: a
/// point's color), its crosshair, the ball's meridian through the point
/// and its equator; the point (`dot`: across, up, pixels of an 85-pixel
/// wheel), and a line to it in its color. As VEGAS's: dragging the wheel
/// moves the point a twentieth of the way the pointer goes, dragging the
/// point itself a thirty-second (the pointer hidden meanwhile), Ctrl ten
/// times finer; a double-click puts it back in the middle, a right
/// double-click where the pointer is; the mouse wheel moves it a pixel a
/// notch (Ctrl: a tenth) — up and down, Alt across five times as far,
/// Shift out and in, Shift and Alt round five degrees —, and the arrow
/// keys a pixel (Ctrl: a tenth), with Shift out and in and round a degree.
pub(crate) fn grading_wheel(
    ui: &mut Ui,
    size: f32,
    dot: [f64; 2],
    color_at: &dyn Fn([f64; 2]) -> Color32,
) -> (Response, Option<WheelChange>) {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let center = rect.center();
    let radius = size / 2.0 - 5.0;
    let scale = radius / WHEEL_RADIUS as f32;
    let screen = |p: [f64; 2]| center + vec2(p[0] as f32, -(p[1] as f32)) * scale;
    let mut change = None;
    let mut to = dot;
    let id = resp.id;
    if resp.clicked() || resp.drag_started() {
        resp.request_focus();
    }
    // Dragging: the point itself or the wheel.
    if resp.drag_started() {
        let from = ui.input(|i| i.pointer.press_origin()).unwrap_or(center);
        let on_dot = from.distance(screen(dot)) <= 7.0;
        ui.data_mut(|d| d.insert_temp(id, on_dot));
    }
    if resp.dragged() {
        let on_dot = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        let mut k = if on_dot { 1.0 / 32.0 } else { 1.0 / 20.0 };
        if ui.input(|i| i.modifiers.command) {
            k *= 0.1;
        }
        let d = resp.drag_delta();
        if d != egui::Vec2::ZERO {
            to = on_wheel([to[0] + f64::from(d.x) * k, to[1] - f64::from(d.y) * k]);
        }
        if on_dot {
            crate::cursors::system(ui.ctx(), egui::CursorIcon::None);
        }
    }
    if resp.double_clicked() {
        change = Some(WheelChange::Reset);
    }
    if resp.double_clicked_by(egui::PointerButton::Secondary)
        && let Some(p) = resp.interact_pointer_pos()
    {
        let d = (p - center) / scale;
        to = on_wheel([f64::from(d.x), -f64::from(d.y)]);
    }
    // The mouse wheel over it.
    if resp.hovered() {
        let (n, keys) = ui.input(|i| notches_with_keys(&i.events));
        if n != 0.0 {
            let n = f64::from(n);
            let step = if keys.ctrl || keys.command { 0.1 } else { 1.0 };
            let (r, a) = (to[0].hypot(to[1]), to[1].atan2(to[0]));
            to = on_wheel(match (keys.shift, keys.alt) {
                (true, true) => {
                    let a = a + (5.0 * n).to_radians();
                    [r * a.cos(), r * a.sin()]
                }
                (true, false) => {
                    let r = (r + n * step).max(0.0);
                    [r * a.cos(), r * a.sin()]
                }
                (false, true) => [to[0] + n * step * 5.0, to[1]],
                // Rolled up, the point goes down, as VEGAS's.
                (false, false) => [to[0], to[1] - n * step],
            });
            ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
        }
    }
    // The arrow keys, while it has the focus.
    if resp.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default()
                },
            );
        });
        let (dx, dy, keys) = ui.input(|i| {
            let k = |key| f64::from(u8::from(i.key_pressed(key)));
            (
                k(egui::Key::ArrowRight) - k(egui::Key::ArrowLeft),
                k(egui::Key::ArrowUp) - k(egui::Key::ArrowDown),
                i.modifiers,
            )
        });
        if dx != 0.0 || dy != 0.0 {
            let step = if keys.ctrl || keys.command { 0.1 } else { 1.0 };
            to = on_wheel(if keys.shift {
                let r = (to[0].hypot(to[1]) + dy * step).max(0.0);
                let a = to[1].atan2(to[0]) - dx.to_radians();
                [r * a.cos(), r * a.sin()]
            } else {
                [to[0] + dx * step, to[1] + dy * step]
            });
        }
    }
    // The joystick moves the point of the wheel under the pointer.
    if let Some(v) = joystick_push(ui, &resp) {
        let k = 0.8 * WHEEL_RADIUS;
        to = on_wheel([to[0] + f64::from(v.x) * k, to[1] - f64::from(v.y) * k]);
    }
    if change.is_none() && to != dot {
        change = Some(WheelChange::Moved(to));
    }
    let painter = ui.painter();
    let gray = Color32::from_gray;
    // The ball.
    let n = 96;
    let mut ball = egui::Mesh::default();
    ball.colored_vertex(center, gray(60));
    for i in 0..=n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        ball.colored_vertex(center + vec2(a.cos(), a.sin()) * radius, gray(32));
    }
    for i in 0..n {
        ball.add_triangle(0, 1 + i, 2 + i);
    }
    painter.add(ball);
    let dark = Stroke::new(1.0, gray(22));
    painter.line_segment(
        [center - vec2(radius, 0.0), center + vec2(radius, 0.0)],
        dark,
    );
    painter.line_segment(
        [center - vec2(0.0, radius), center + vec2(0.0, radius)],
        dark,
    );
    // The point, and the ball turned toward it.
    let shown = match change {
        Some(WheelChange::Moved(p)) => p,
        Some(WheelChange::Reset) => [0.0; 2],
        None => dot,
    };
    let point = screen(shown);
    let r = (shown[0].hypot(shown[1]) / WHEEL_RADIUS).min(1.0) as f32;
    if r > 0.01 {
        let u = (point - center) / (r * radius);
        let v = vec2(-u.y, u.x);
        let ellipse = |minor: f32| -> Vec<egui::Pos2> {
            (0..=72)
                .map(|i| {
                    let t = i as f32 / 72.0 * std::f32::consts::TAU;
                    center + v * (t.cos() * radius) + u * (t.sin() * minor * radius)
                })
                .collect()
        };
        painter.add(egui::Shape::line(
            ellipse((1.0 - r * r).max(0.0).sqrt()),
            Stroke::new(1.0, gray(20)),
        ));
        painter.add(egui::Shape::line(ellipse(r), Stroke::new(1.0, gray(150))));
        // The line to the point in its color, fading toward the middle.
        let color = color_at(shown);
        let steps = 10;
        for k in 0..steps {
            let (t0, t1) = (k as f32 / steps as f32, (k + 1) as f32 / steps as f32);
            let a = 0.15 + 0.85 * t1;
            painter.line_segment(
                [
                    center + (point - center) * t0,
                    center + (point - center) * t1,
                ],
                Stroke::new(2.5, color.gamma_multiply(a)),
            );
        }
    }
    // The ring: each color where the point gives it.
    for i in 0..n {
        let a0 = i as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
        let mid = f64::from((a0 + a1) / 2.0);
        let color = color_at([mid.cos() * WHEEL_RADIUS, mid.sin() * WHEEL_RADIUS]);
        let at = |a: f32| center + vec2(a.cos(), -a.sin()) * (radius + 2.5);
        painter.line_segment([at(a0), at(a1)], Stroke::new(5.0, color));
    }
    painter.circle_filled(point, 5.0, gray(246));
    painter.circle_stroke(point, 5.0, Stroke::new(1.0, gray(90)));
    let resp = resp.on_hover_text(tr(
        "Drag to move the color (Ctrl: finer) · double-click: reset · right double-click: jump there · mouse wheel: move it (Alt: across, Shift: out and in, Shift+Alt: round)",
    ));
    (resp, change)
}

/// Icon button painted in its own color (e.g. a red "delete" cross).
pub fn icon_button_tinted(
    ui: &mut Ui,
    icon: Icon,
    tip: &str,
    tint: Color32,
    enabled: bool,
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(BUTTON, sense);
    if ui.is_rect_visible(rect) {
        if resp.hovered() && enabled {
            ui.painter().rect_filled(rect, 3.0, theme::hover());
        }
        let color = if enabled {
            tint
        } else {
            theme::text_disabled()
        };
        icons::paint(ui.painter(), rect.shrink(5.0), icon, color);
    }
    resp.on_hover_text(tip)
}

/// A tab of a tabbed window (Preferences, Project Properties). A tab
/// BananaV does not have yet is greyed out with the "Coming soon" tip.
pub fn tab_button(ui: &mut Ui, text: &str, on: bool, enabled: bool, min_width: f32) -> Response {
    let color = if enabled {
        theme::text()
    } else {
        theme::text_disabled()
    };
    let button = egui::Button::new(egui::RichText::new(text).color(color))
        .fill(if on { theme::tab_on() } else { theme::panel() })
        .stroke(Stroke::new(
            1.0,
            if on {
                theme::snap()
            } else {
                theme::separator()
            },
        ))
        .min_size(vec2(min_width, 24.0))
        .selected(on);
    ui.add_enabled(enabled, button)
        .on_disabled_hover_text(tr("Coming soon to BananaV"))
}

/// An item of a drop-down list that BananaV cannot do yet: greyed out,
/// with the "Coming soon" tip.
pub fn soon_item(ui: &mut Ui, text: &str) {
    ui.add_enabled(false, egui::Button::selectable(false, text))
        .on_disabled_hover_text(tr("Coming soon to BananaV"));
}

/// A text box with a drop-down list, like a Windows editable combo box:
/// the text can be typed or picked from the list. `list` draws the items
/// and returns the one clicked. Returns the text box's response and the
/// picked item.
pub fn editable_combo<T>(
    ui: &mut Ui,
    id: egui::Id,
    text: &mut String,
    width: f32,
    list: impl FnOnce(&mut Ui) -> Option<T>,
) -> (Response, Option<T>) {
    const ARROW: f32 = 18.0;
    let h = ui.spacing().interact_size.y;
    // Always text then arrow, also inside right-to-left rows.
    let layout = egui::Layout::left_to_right(egui::Align::Center);
    ui.allocate_ui_with_layout(vec2(width, h), layout, |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let edit = ui.add_sized(
            vec2(width - ARROW, h),
            egui::TextEdit::singleline(text).id(id.with("text")),
        );
        let (rect, arrow) = ui.allocate_exact_size(vec2(ARROW, h), Sense::click());
        let visuals = ui.style().interact(&arrow);
        ui.painter().rect(
            rect,
            2.0,
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        let c = rect.center();
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                c + vec2(-4.0, -2.0),
                c + vec2(4.0, -2.0),
                c + vec2(0.0, 3.0),
            ],
            visuals.fg_stroke.color,
            Stroke::NONE,
        ));
        let full = edit.rect.union(rect);
        let picked = egui::Popup::menu(&arrow)
            .id(id.with("list"))
            .align(egui::RectAlign::BOTTOM_END)
            .width(full.width())
            .show(|ui| {
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, list)
                    .inner
            })
            .and_then(|r| r.inner);
        (edit, picked)
    })
    .inner
}

/// A number box with up/down arrows, like a Windows spin box: the value can
/// be typed, dragged or stepped by `step` with the arrows. `decimals` places
/// are shown with the language's decimal separator.
pub fn spin_box(
    ui: &mut Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    step: f64,
    decimals: usize,
    width: f32,
) -> Response {
    const ARROWS: f32 = 12.0;
    let h = ui.spacing().interact_size.y;
    let layout = egui::Layout::left_to_right(egui::Align::Center);
    ui.allocate_ui_with_layout(vec2(width, h), layout, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let mut r = ui.add_sized(
            vec2(width - ARROWS - 2.0, h),
            egui::DragValue::new(value)
                .range(range.clone())
                .speed(step)
                .custom_formatter(move |v, _| crate::fmt::num(v, decimals))
                .custom_parser(crate::fmt::parse),
        );
        let (rect, _) = ui.allocate_exact_size(vec2(ARROWS, h), Sense::hover());
        let halves = [
            (
                Rect::from_min_max(rect.min, pos2(rect.max.x, rect.center().y)),
                1.0,
            ),
            (
                Rect::from_min_max(pos2(rect.min.x, rect.center().y), rect.max),
                -1.0,
            ),
        ];
        for (half, dir) in halves {
            let resp = ui.interact(half, r.id.with(dir as i32), Sense::click());
            let color = if !ui.is_enabled() {
                theme::text_disabled()
            } else if resp.hovered() {
                theme::banana()
            } else {
                theme::text_dim()
            };
            let c = half.center();
            let tip = c + vec2(0.0, -3.0 * dir);
            ui.painter().add(egui::Shape::convex_polygon(
                vec![tip, c + vec2(3.5, 2.0 * dir), c + vec2(-3.5, 2.0 * dir)],
                color,
                Stroke::NONE,
            ));
            if resp.clicked() {
                *value = (*value + step * dir as f64).clamp(*range.start(), *range.end());
                r.mark_changed();
            }
        }
        r
    })
    .inner
}

/// The knob of VEGAS's horizontal sliders (track level, volume, scrub): a
/// light block with two grip lines.
pub fn paint_knob(painter: &egui::Painter, rect: Rect, hot: bool) {
    let fill = if hot {
        theme::text_strong()
    } else {
        theme::knob()
    };
    painter.rect_filled(rect, 2.0, fill);
    let c = rect.center();
    for dx in [-1.5, 1.5] {
        painter.line_segment(
            [
                pos2(c.x + dx, rect.top() + 3.0),
                pos2(c.x + dx, rect.bottom() - 3.0),
            ],
            Stroke::new(1.0, theme::panel_dark()),
        );
    }
}

/// Paints one line of text that fits in `max_width`: longer text ends in
/// "…", like VEGAS's lists, instead of running into its neighbours.
pub fn fit_text(
    painter: &egui::Painter,
    pos: egui::Pos2,
    anchor: egui::Align2,
    text: impl ToString,
    font: egui::FontId,
    color: Color32,
    max_width: f32,
) -> egui::Rect {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(1.0));
    let galley = painter.layout_job(job);
    let rect = anchor.anchor_size(pos, galley.size());
    painter.galley(rect.min, galley, color);
    rect
}

/// The mouse wheel's notches this frame (rolled away from the user:
/// positive) and whether Ctrl was held.
pub(crate) fn wheel_notches(events: &[egui::Event]) -> (f32, bool) {
    let mut notches = 0.0;
    let mut ctrl = false;
    for e in events {
        if let egui::Event::MouseWheel {
            unit,
            delta,
            modifiers,
            ..
        } = e
        {
            notches += match unit {
                egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => delta.y,
                // Touchpads: a notch every 40 points, as egui scrolls.
                egui::MouseWheelUnit::Point => delta.y / 40.0,
            };
            ctrl |= modifiers.ctrl || modifiers.command;
        }
    }
    (notches, ctrl)
}

/// A button that is an automation control (Automation Settings): marked
/// as the sliders' thumbs are, a dot in its corner.
pub(crate) fn mark_automation(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter().circle_filled(
        rect.right_bottom() - egui::vec2(3.5, 3.5),
        2.0,
        crate::theme::banana(),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn wheel_notches_count_lines_and_points() {
        let wheel = |unit, y: f32, ctrl: bool| egui::Event::MouseWheel {
            unit,
            delta: vec2(0.0, y),
            phase: egui::TouchPhase::Move,
            modifiers: if ctrl {
                egui::Modifiers::CTRL
            } else {
                egui::Modifiers::NONE
            },
        };
        let events = [
            wheel(egui::MouseWheelUnit::Line, 1.0, false),
            wheel(egui::MouseWheelUnit::Point, -20.0, false),
        ];
        assert_eq!(wheel_notches(&events), (0.5, false));
        let events = [wheel(egui::MouseWheelUnit::Line, -2.0, true)];
        assert_eq!(wheel_notches(&events), (-2.0, true));
    }

    use super::*;

    #[test]
    fn the_joystick_moves_the_wheel_under_the_pointer() {
        assert_eq!(joystick_dead_zone(vec2(0.1, -0.05)), egui::Vec2::ZERO);
        let full = joystick_dead_zone(vec2(1.0, 0.0));
        assert!((full.x - 1.0).abs() < 1e-6 && full.y == 0.0);
        let dot = std::cell::Cell::new([0.0f64; 2]);
        let wheel = |ui: &mut Ui| {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new(JOYSTICK), vec2(1.0, 0.0)));
            let grey = |_: [f64; 2]| Color32::GRAY;
            if let (_, Some(WheelChange::Moved(p))) = grading_wheel(ui, 100.0, dot.get(), &grey) {
                dot.set(p);
            }
        };
        let mut harness = egui_kittest::Harness::new_ui(wheel);
        // Not under the pointer: nothing moves.
        harness.run_steps(3);
        drop(harness);
        assert_eq!(dot.get(), [0.0; 2]);
        let mut harness = egui_kittest::Harness::new_ui(wheel);
        harness.hover_at(pos2(50.0, 50.0));
        harness.run_steps(5);
        drop(harness);
        // Right on the screen; kept on the wheel.
        let [x, y] = dot.get();
        assert!(x > 0.0 && y.abs() < 1e-6, "pushed right: {x} {y}");
        assert!(x.hypot(y) <= WHEEL_RADIUS + 1e-9);
    }

    #[test]
    fn a_wheels_point_stays_on_it() {
        assert_eq!(on_wheel([30.0, -40.0]), [30.0, -40.0]);
        let [x, y] = on_wheel([300.0, 400.0]);
        assert!((x - 51.0).abs() < 1e-9 && (y - 68.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_scroll_bars_scroll_by_the_middle_and_zoom_by_the_ends() {
        // A tenth of 1000 shows at 300: a thumb 40 wide, 300/900 along.
        let (at, w) = thumb_span(400.0, 300.0, 100.0, 1000.0);
        assert_eq!(w, 40.0);
        assert!((at - 360.0 / 3.0).abs() < 1e-3, "{at}");
        // However little shows, the thumb stays MIN_THUMB wide.
        let (at, w) = thumb_span(400.0, 0.0, 1.0, 1e6);
        assert_eq!((at, w), (0.0, MIN_THUMB));
        let grab = |part| BarGrab {
            part,
            x: 0.0,
            start: 300.0,
            len: 100.0,
            total: 1000.0,
            width: 40.0,
            travel: 360.0,
        };
        let lens = (10.0, 1000.0);
        // The middle scrolls: 36 points are a tenth of the travel.
        let (s, l) = bar_drag(&grab(BarPart::Middle), 36.0, lens);
        assert!((s - 390.0).abs() < 1e-4 && l == 100.0, "{s} {l}");
        // The right end zooms out, the left staying.
        assert_eq!(bar_drag(&grab(BarPart::End), 40.0, lens), (300.0, 200.0));
        // The left end zooms in, the right staying.
        assert_eq!(bar_drag(&grab(BarPart::Start), 20.0, lens), (350.0, 50.0));
        // Within the lengths allowed, and never before the start.
        assert_eq!(bar_drag(&grab(BarPart::End), -100.0, lens), (300.0, 10.0));
        assert_eq!(bar_drag(&grab(BarPart::Start), -400.0, lens), (0.0, 400.0));
    }

    #[test]
    fn fader_ida_e_volta() {
        for db in [-60.0, -12.0, 0.0, 6.0] {
            let p = db_to_pos(db, -60.0, 12.0);
            assert!((pos_to_db(p, -60.0, 12.0) - db).abs() < 1e-3);
        }
    }

    /// Two double-clicks in a row at two spots are two (egui calls the
    /// fourth click a triple one); slow or far clicks make no pair.
    #[test]
    fn clicks_pair_up_two_by_two() {
        use egui::pos2;
        let (d, n) = (0.5, 6.0);
        let a = pos2(10.0, 10.0);
        let b = pos2(80.0, 10.0);
        assert!(!pairs(None, (0.0, a), d, n));
        assert!(pairs(Some((0.0, a, false)), (0.2, a), d, n));
        // The next click starts a pair of its own, even quickly after.
        assert!(!pairs(Some((0.2, a, true)), (0.3, b), d, n));
        assert!(pairs(Some((0.3, b, false)), (0.45, pos2(81.0, 11.0)), d, n));
        // Too late, too far.
        assert!(!pairs(Some((0.0, a, false)), (0.6, a), d, n));
        assert!(!pairs(Some((0.0, a, false)), (0.1, pos2(20.0, 10.0)), d, n));
    }
}
