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
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
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
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
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

/// Converte o deslocamento RGB numa posição no plano de croma (raio 1 = 0,5).
pub(crate) fn offset_to_point(o: [f32; 3]) -> (f32, f32) {
    let m = (o[0] + o[1] + o[2]) / 3.0;
    let c = [o[0] - m, o[1] - m, o[2] - m];
    let x = c[0] - 0.5 * (c[1] + c[2]);
    let y = 0.866_025_4 * (c[1] - c[2]);
    (x / 0.5, y / 0.5)
}

pub(crate) fn point_to_offset(x: f32, y: f32, master: f32) -> [f32; 3] {
    // Inversa exata de `offset_to_point` (com a restrição de média zero).
    let r = (x * x + y * y).sqrt();
    let (x, y) = if r > 1.0 { (x / r, y / r) } else { (x, y) };
    let c0 = x / 1.5;
    let d = y / 0.866_025_4;
    let c = [c0, (-c0 + d) / 2.0, (-c0 - d) / 2.0];
    c.map(|v| v * 0.5 + master)
}

/// Where a wheel's point (`offset_to_point`'s plane) shows on the screen,
/// from its middle in radii (y down): the plane turned a quarter, red up,
/// as VEGAS's wheels and the vectorscope have it.
fn wheel_screen((x, y): (f32, f32)) -> egui::Vec2 {
    vec2(-y, -x)
}

/// The point of the plane at a screen offset from a wheel's middle, in
/// radii.
fn wheel_plane(d: egui::Vec2) -> (f32, f32) {
    (-d.y, -d.x)
}

/// A wheel as VEGAS 23 draws its own: a dark ball — lighter in the middle
/// — in a ring of hues, red at the top; its crosshair, the ball's meridian
/// through the point and its equator; the point, and a line to it in its
/// color. Dragging moves the color (its mean kept), a double-click brings
/// it back to the middle. `size` is the ring's outer diameter.
pub(crate) fn grading_wheel(ui: &mut Ui, offset: &mut [f32; 3], size: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let center = rect.center();
    let radius = size / 2.0 - 5.0;
    let master = (offset[0] + offset[1] + offset[2]) / 3.0;
    if resp.dragged()
        && let Some(p) = resp.interact_pointer_pos()
    {
        // Ctrl: finer.
        let fine = ui.input(|i| i.modifiers.ctrl);
        let (x, y) = if fine {
            let (x0, y0) = offset_to_point(*offset);
            let (dx, dy) = wheel_plane(resp.drag_delta() / radius * 0.25);
            (x0 + dx, y0 + dy)
        } else {
            wheel_plane((p - center) / radius)
        };
        *offset = point_to_offset(x, y, master);
        resp.mark_changed();
    }
    if resp.double_clicked() {
        *offset = [master; 3];
        resp.mark_changed();
    }
    // The joystick moves the point of the wheel under the pointer (its y
    // up, the screen's down).
    if let Some(v) = joystick_push(ui, &resp) {
        let (x, y) = offset_to_point(*offset);
        let (dx, dy) = wheel_plane(vec2(v.x, -v.y) * 0.8);
        *offset = point_to_offset(x + dx, y + dy, master);
        resp.mark_changed();
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
    // The point, within the ring.
    let (x, y) = offset_to_point(*offset);
    let r = (x * x + y * y).sqrt();
    let (x, y) = if r > 1.0 { (x / r, y / r) } else { (x, y) };
    let r = r.min(1.0);
    let point = center + wheel_screen((x, y)) * radius;
    let c = hue_rgb(y.atan2(x));
    let hue = Color32::from_rgb(
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
    );
    if r > 0.01 {
        // The ball turned toward the point: its equator, and the meridian
        // through the point.
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
        // The line to the point, fading toward the middle.
        let steps = 10;
        for k in 0..steps {
            let (t0, t1) = (k as f32 / steps as f32, (k + 1) as f32 / steps as f32);
            let a = 0.15 + 0.85 * t1;
            painter.line_segment(
                [
                    center + (point - center) * t0,
                    center + (point - center) * t1,
                ],
                Stroke::new(2.5, hue.gamma_multiply(a)),
            );
        }
    }
    // The ring of hues: each hue where its point would be.
    for i in 0..n {
        let a0 = i as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
        let mid = (a0 + a1) / 2.0;
        let (px, py) = wheel_plane(vec2(mid.cos(), -mid.sin()));
        let c = hue_rgb(py.atan2(px));
        let color = Color32::from_rgb(
            (c[0] * 255.0) as u8,
            (c[1] * 255.0) as u8,
            (c[2] * 255.0) as u8,
        );
        let at = |a: f32| center + vec2(a.cos(), -a.sin()) * (radius + 2.5);
        painter.line_segment([at(a0), at(a1)], Stroke::new(5.0, color));
    }
    painter.circle_filled(point, 5.0, gray(246));
    painter.circle_stroke(point, 5.0, Stroke::new(1.0, gray(90)));
    resp.on_hover_text(tr(
        "Drag to shift the color (Ctrl: finer) · double-click resets",
    ))
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
        let mut offset = [0.0f32; 3];
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new(JOYSTICK), vec2(1.0, 0.0)));
            grading_wheel(ui, &mut offset, 100.0);
        });
        // Not under the pointer: nothing moves.
        harness.run_steps(3);
        drop(harness);
        assert_eq!(offset, [0.0; 3]);
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new(JOYSTICK), vec2(1.0, 0.0)));
            grading_wheel(ui, &mut offset, 100.0);
        });
        harness.hover_at(pos2(50.0, 50.0));
        harness.run_steps(5);
        drop(harness);
        // Right on the screen, red up: the plane's y down.
        let d = wheel_screen(offset_to_point(offset));
        assert!(d.x > 0.0 && d.y.abs() < 1e-3, "pushed right: {d:?}");
    }

    #[test]
    fn the_wheel_shows_red_at_the_top() {
        // Red's point is up; the screen and the plane go both ways.
        let red = offset_to_point([0.5, 0.0, 0.0]);
        let d = wheel_screen(red);
        assert!(d.y < 0.0 && d.x.abs() < 1e-6, "{d:?}");
        let (x, y) = wheel_plane(d);
        assert!((x - red.0).abs() < 1e-6 && (y - red.1).abs() < 1e-6);
    }

    #[test]
    fn roda_de_cor_ida_e_volta() {
        for (x, y) in [(0.5, 0.0), (0.0, -0.8), (-0.3, 0.4), (0.0, 0.0)] {
            let o = point_to_offset(x, y, 0.1);
            let (bx, by) = offset_to_point(o);
            assert!(
                (bx - x).abs() < 1e-3 && (by - y).abs() < 1e-3,
                "({x},{y}) → ({bx},{by})"
            );
            let m = (o[0] + o[1] + o[2]) / 3.0;
            assert!((m - 0.1).abs() < 1e-5, "o brilho médio fica no master");
        }
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
}
