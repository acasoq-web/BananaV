//! A color parameter as VEGAS draws an OFX effect's: its swatch (checkered
//! where it lets what is under it show) and its numbers — in the color
//! model picked, HSV at first ("0, 1.0, 1.0" is red), its alpha last when
//! it has one — and, opened with its triangle or the swatch, the picker
//! under them: the model (HSV, HSL, RGB, Lab), the plane and the slider
//! beside it — (H×S) V, (S×V) H, (H×V) S, or the three numbers apart —,
//! the alpha's slider, and an eyedropper that takes a color from anywhere
//! on the screen.

use crate::cursors::{self, Cursor};
use crate::egui::{self, Color32, Mesh, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use crate::theme;

/// How a color's three numbers are counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    Hsv,
    Hsl,
    Rgb,
    Lab,
}

impl Model {
    pub const ALL: [Model; 4] = [Model::Hsv, Model::Hsl, Model::Rgb, Model::Lab];

    pub fn name(self) -> &'static str {
        match self {
            Model::Hsv => "HSV",
            Model::Hsl => "HSL",
            Model::Rgb => "RGB",
            Model::Lab => "Lab",
        }
    }

    /// Its numbers' letters.
    fn letters(self) -> [&'static str; 3] {
        match self {
            Model::Hsv => ["H", "S", "V"],
            Model::Hsl => ["H", "S", "L"],
            Model::Rgb => ["R", "G", "B"],
            Model::Lab => ["L", "a", "b"],
        }
    }

    /// Each number's range.
    fn ranges(self) -> [(f64, f64); 3] {
        match self {
            Model::Hsv | Model::Hsl => [(0.0, 360.0), (0.0, 1.0), (0.0, 1.0)],
            Model::Rgb => [(0.0, 1.0); 3],
            Model::Lab => [(0.0, 100.0), (-128.0, 127.0), (-128.0, 127.0)],
        }
    }

    /// Each number's decimals, as VEGAS writes them: as few as it needs,
    /// within these ("0, 1.0, 1.0", "56, 0.31, 1.0", "1.0, 0.5, 0.25";
    /// Sepia's 166, 166, 0 is "60, 1.0, 0.65", its 255, 211, 26 "48, 0.9,
    /// 1.0": two decimals at most).
    fn decimals(self) -> [(usize, usize); 3] {
        match self {
            Model::Hsv | Model::Hsl => [(0, 0), (2, 1), (2, 1)],
            Model::Rgb => [(2, 1); 3],
            Model::Lab => [(2, 1); 3],
        }
    }

    /// A color's numbers in this model (red, green and blue 0–1).
    pub fn numbers_of(self, rgb: [f64; 3]) -> [f64; 3] {
        match self {
            Model::Hsv => rgb_to_hsv(rgb),
            Model::Hsl => {
                let [h, s, l] = crate::color_picker::rgb_to_hsl(rgb.map(|v| v as f32));
                [h as f64 * 360.0, s as f64, l as f64]
            }
            Model::Rgb => rgb,
            Model::Lab => rgb_to_lab(rgb),
        }
    }

    /// The color of numbers in this model.
    pub fn colour_of(self, v: [f64; 3]) -> [f64; 3] {
        match self {
            Model::Hsv => hsv_to_rgb(v),
            Model::Hsl => {
                crate::color_picker::hsl_to_rgb([(v[0] / 360.0) as f32, v[1] as f32, v[2] as f32])
                    .map(f64::from)
            }
            Model::Rgb => v,
            Model::Lab => lab_to_rgb(v),
        }
        .map(|c| c.clamp(0.0, 1.0))
    }
}

/// Which two numbers the plane shows and which the slider beside it, or
/// the three apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plane {
    /// (first × second) third: (H×S) V.
    First,
    /// (second × third) first: (S×V) H.
    Second,
    /// (first × third) second: (H×V) S.
    Third,
    Separate,
}

impl Plane {
    pub const ALL: [Plane; 4] = [Plane::First, Plane::Second, Plane::Third, Plane::Separate];

    /// The numbers across and up the plane, and the slider's.
    fn axes(self) -> Option<[usize; 3]> {
        match self {
            Plane::First => Some([0, 1, 2]),
            Plane::Second => Some([1, 2, 0]),
            Plane::Third => Some([0, 2, 1]),
            Plane::Separate => None,
        }
    }

    /// Its name in `model`'s letters: "(SxV) H".
    pub fn name(self, model: Model) -> String {
        let l = model.letters();
        match self.axes() {
            Some([x, y, s]) => format!("({}x{}) {}", l[x], l[y], l[s]),
            None => crate::i18n::tr("Separate").to_string(),
        }
    }
}

/// A color parameter's picker state.
#[derive(Clone, Debug)]
pub struct ColorView {
    pub model: Model,
    pub plane: Plane,
    /// The numbers last set and the color they gave: a grey keeps the hue
    /// it was given.
    numbers: Option<([f64; 3], [f64; 3])>,
    /// The eyedropper waits for a click: whether the mouse's button was
    /// down the frame before.
    picking: Option<bool>,
}

impl Default for ColorView {
    fn default() -> ColorView {
        ColorView {
            model: Model::Hsv,
            plane: Plane::Second,
            numbers: None,
            picking: None,
        }
    }
}

impl ColorView {
    /// A picker showing model `name` at first ("HSV", "HSL", "RGB" or
    /// "Lab"; others: HSV), as a plug-in asks.
    pub fn starting_in(name: &str) -> ColorView {
        ColorView {
            model: Model::ALL
                .into_iter()
                .find(|m| m.name().eq_ignore_ascii_case(name))
                .unwrap_or(Model::Hsv),
            ..ColorView::default()
        }
    }

    /// The numbers `rgb` shows as: those last set when they gave it.
    fn numbers(&self, rgb: [f64; 3]) -> [f64; 3] {
        match self.numbers {
            Some((n, c)) if same(c, rgb) => n,
            _ => self.model.numbers_of(rgb),
        }
    }

    /// Sets numbers: the color they give.
    fn set(&mut self, n: [f64; 3]) -> [f64; 3] {
        let rgb = self.model.colour_of(n);
        self.numbers = Some((n, rgb));
        rgb
    }
}

fn same(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-4)
}

/// The numbers' text for `rgb`, in the view's model, and its `alpha` last
/// when it has one.
pub fn text(view: &ColorView, rgb: [f64; 3], alpha: Option<f64>) -> String {
    let n = view.numbers(rgb);
    let d = view.model.decimals();
    let sep = if crate::i18n::language().decimal_separator() == ',' {
        "; "
    } else {
        ", "
    };
    (0..3)
        .map(|i| number(n[i], d[i].0, d[i].1))
        .chain(alpha.map(|a| number(a, 3, 1)))
        .collect::<Vec<_>>()
        .join(sep)
}

/// A number as VEGAS writes it: up to `most` decimals, as few as it needs
/// but `least` ("53.39", "0.0", "-0.01", "1.0").
fn number(v: f64, most: usize, least: usize) -> String {
    let v = if v.abs() < 0.5 * 10f64.powi(-(most as i32)) {
        0.0
    } else {
        v
    };
    let mut s = crate::fmt::num(v, most);
    let point = crate::i18n::language().decimal_separator();
    if let Some(at) = s.rfind(point) {
        while s.len() > at + 1 + least && s.ends_with('0') {
            s.pop();
        }
    }
    s
}

/// The color typed as numbers in the view's model, with its alpha when
/// `alpha` (the alpha typed, if it was).
pub fn parse(view: &mut ColorView, text: &str, alpha: bool) -> Option<([f64; 3], Option<f64>)> {
    let typed = if alpha {
        crate::ofx::parse_numbers(text, 4)
    } else {
        None
    };
    let v = typed.or_else(|| crate::ofx::parse_numbers(text, 3))?;
    let r = view.model.ranges();
    let n = [0, 1, 2].map(|i| v[i].clamp(r[i].0, r[i].1));
    Some((view.set(n), v.get(3).map(|a| a.clamp(0.0, 1.0))))
}

fn color32(rgb: [f64; 3]) -> Color32 {
    let b = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(b(rgb[0]), b(rgb[1]), b(rgb[2]))
}

/// The swatch: the color, framed as VEGAS frames it, over a checkerboard
/// where its `alpha` lets it show. A click on it opens or closes the
/// picker; a double click puts the color back (the caller's part).
pub fn swatch(ui: &mut Ui, rgb: [f64; 3], alpha: f64, width: f32, label: &str) -> egui::Response {
    let h = ui.spacing().interact_size.y;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, h), Sense::click());
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::ColorButton, ui.is_enabled(), label)
    });
    let p = ui.painter();
    let frame = if ui.is_enabled() {
        Color32::from_gray(170)
    } else {
        Color32::from_gray(96)
    };
    p.rect_filled(rect, 0.0, frame);
    p.rect_filled(rect.shrink(2.0), 0.0, Color32::from_gray(32));
    let inner = rect.shrink(3.0);
    if alpha < 1.0 {
        checkers(p, inner, 4.0);
    }
    let mut c = color32(rgb).gamma_multiply(alpha.clamp(0.0, 1.0) as f32);
    if !ui.is_enabled() {
        c = c.gamma_multiply(0.6);
    }
    p.rect_filled(inner, 0.0, c);
    resp
}

/// A checkerboard of `size` squares over `rect`: what shows through a
/// see-through color.
fn checkers(p: &egui::Painter, rect: Rect, size: f32) {
    p.rect_filled(rect, 0.0, Color32::from_gray(200));
    let (nx, ny) = (
        (rect.width() / size).ceil() as usize,
        (rect.height() / size).ceil() as usize,
    );
    for j in 0..ny {
        for i in (j % 2..nx).step_by(2) {
            let min = rect.min + vec2(i as f32 * size, j as f32 * size);
            let cell = Rect::from_min_size(min, vec2(size, size)).intersect(rect);
            p.rect_filled(cell, 0.0, Color32::from_gray(120));
        }
    }
}

/// The picker's sizes: the lists' row, the plane and its slider.
const ROW: f32 = 22.0;
const PLANE: Vec2 = vec2(96.0, 100.0);
const SLIDER: Vec2 = vec2(20.0, 100.0);
const GAP: f32 = 20.0;
/// The alpha's slider, beside the other.
const ALPHA_GAP: f32 = 12.0;
/// Its height.
pub const HEIGHT: f32 = ROW + 8.0 + PLANE.y + 6.0;

/// The picker for `rgb` (and its `alpha`, when it has one), in a box
/// `width` wide from where `ui` is: the model's and the plane's lists and
/// the eyedropper, then the plane, its slider and the alpha's. The color
/// picked, when it changed, and its alpha.
pub fn picker(
    ui: &mut Ui,
    view: &mut ColorView,
    id: egui::Id,
    rgb: [f64; 3],
    alpha: Option<f64>,
    width: f32,
) -> Option<([f64; 3], Option<f64>)> {
    let mut out = None;
    let mut new_alpha = None;
    let top = ui.cursor().min;
    // The lists and the eyedropper, centred over the plane and its sliders.
    let block = PLANE.x + GAP + SLIDER.x + alpha.map_or(0.0, |_| ALPHA_GAP + SLIDER.x);
    let left = top.x + ((width - block) / 2.0).max(0.0) - 6.0;
    let row = Rect::from_min_size(pos2(left, top.y), vec2(block + 40.0, ROW));
    ui.scope_builder(egui::UiBuilder::new().max_rect(row), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            egui::ComboBox::from_id_salt(id.with("model"))
                .width(52.0)
                .selected_text(view.model.name())
                .show_ui(ui, |ui| {
                    for m in Model::ALL {
                        if ui.selectable_label(view.model == m, m.name()).clicked()
                            && view.model != m
                        {
                            view.model = m;
                            view.numbers = None;
                        }
                    }
                });
            egui::ComboBox::from_id_salt(id.with("plane"))
                .width(70.0)
                .selected_text(view.plane.name(view.model))
                .show_ui(ui, |ui| {
                    for p in Plane::ALL {
                        let name = p.name(view.model);
                        if ui.selectable_label(view.plane == p, name).clicked() {
                            view.plane = p;
                        }
                    }
                });
            if eyedropper_button(ui, view.picking.is_some()).clicked() {
                if view.picking.take().is_some() {
                    bv_platform::release_mouse();
                } else if bv_platform::SYSTEM_COLOR_SAMPLER {
                    // The system's sampler (macOS): its magnifier takes the
                    // click and the color.
                    if bv_platform::start_color_sampler() {
                        view.picking = Some(false);
                    }
                } else {
                    // Clicks anywhere come to this window until the pick.
                    if let Some(title) = ui.ctx().input(|i| i.viewport().title.clone()) {
                        bv_platform::capture_mouse(&title);
                    }
                    view.picking = Some(bv_platform::main_button_down());
                }
            }
        });
    });
    if let Some(c) = pick(ui, view, id) {
        out = Some(c);
    }
    let n = view.numbers(rgb);
    let body = Rect::from_min_size(pos2(left + 6.0, top.y + ROW + 8.0), vec2(block, PLANE.y));
    let enabled = ui.is_enabled();
    match view.plane.axes() {
        Some(axes) => {
            let plane = Rect::from_min_size(body.min, PLANE);
            let slider = Rect::from_min_size(pos2(plane.right() + GAP, body.top()), SLIDER);
            if let Some(m) = plane_ui(
                ui,
                id.with("plane-area"),
                plane,
                view.model,
                axes,
                n,
                enabled,
            ) {
                out = Some(view.set(m));
            }
            if let Some(m) = slider_ui(
                ui,
                id.with("slider"),
                slider,
                view.model,
                axes[2],
                n,
                enabled,
            ) {
                out = Some(view.set(m));
            }
            if let Some(a) = alpha {
                let rect =
                    Rect::from_min_size(pos2(slider.right() + ALPHA_GAP, body.top()), SLIDER);
                let shown = out.unwrap_or(rgb);
                new_alpha = alpha_ui(ui, id.with("alpha"), rect, shown, a, enabled);
            }
        }
        None => {
            if let Some(m) = bars_ui(ui, id.with("bars"), body, view.model, n, enabled) {
                out = Some(view.set(m));
            }
            if let Some(a) = alpha {
                // The alpha's bar under the others.
                let rect = Rect::from_min_size(
                    pos2(body.left() + 16.0, body.top() + 6.0 + 3.0 * 30.0),
                    vec2(body.width() - 16.0, 14.0),
                );
                ui.painter().text(
                    pos2(body.left(), rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    "A",
                    egui::FontId::proportional(12.0),
                    theme::text(),
                );
                new_alpha =
                    alpha_bar_ui(ui, id.with("alpha"), rect, out.unwrap_or(rgb), a, enabled);
            }
        }
    }
    ui.allocate_rect(
        Rect::from_min_size(top, vec2(width, HEIGHT)),
        Sense::hover(),
    );
    match (out, new_alpha) {
        (None, None) => None,
        (c, a) => Some((c.unwrap_or(rgb), a.or(alpha))),
    }
}

/// The alpha's slider: the color over a checkerboard, from see-through at
/// the bottom to solid at the top.
fn alpha_ui(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    rgb: [f64; 3],
    alpha: f64,
    enabled: bool,
) -> Option<f64> {
    let resp = ui.interact(
        rect.expand2(vec2(4.0, 6.0)),
        id,
        if enabled {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        },
    );
    let mut out = None;
    if let Some(p) = resp.interact_pointer_pos()
        && (resp.is_pointer_button_down_on() || resp.clicked())
    {
        out = Some((((rect.bottom() - p.y) / rect.height()) as f64).clamp(0.0, 1.0));
    }
    let shown = out.unwrap_or(alpha);
    let painter = ui.painter_at(rect.expand(8.0));
    checkers(&painter, rect, 5.0);
    let c = color32(rgb);
    painter.add(egui::Shape::mesh(gradient_mesh(rect, 16, |_, fy| {
        c.gamma_multiply(fy as f32)
    })));
    if !enabled {
        painter.rect_filled(rect, 0.0, Color32::from_black_alpha(110));
    }
    let y = rect.bottom() - shown as f32 * rect.height();
    let knob = Rect::from_center_size(pos2(rect.center().x, y), vec2(rect.width() + 6.0, 8.0));
    painter.rect_filled(knob, 3.0, Color32::from_gray(190));
    painter.rect_stroke(
        knob,
        3.0,
        Stroke::new(1.0, Color32::from_gray(80)),
        egui::StrokeKind::Inside,
    );
    out
}

/// The alpha's bar, with the numbers apart: see-through at the left.
fn alpha_bar_ui(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    rgb: [f64; 3],
    alpha: f64,
    enabled: bool,
) -> Option<f64> {
    let resp = ui.interact(
        rect.expand2(vec2(4.0, 4.0)),
        id,
        if enabled {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        },
    );
    let mut out = None;
    if let Some(p) = resp.interact_pointer_pos()
        && (resp.is_pointer_button_down_on() || resp.clicked())
    {
        out = Some((((p.x - rect.left()) / rect.width()) as f64).clamp(0.0, 1.0));
    }
    let shown = out.unwrap_or(alpha);
    let painter = ui.painter_at(rect.expand(8.0));
    checkers(&painter, rect, 5.0);
    let c = color32(rgb);
    painter.add(egui::Shape::mesh(gradient_mesh(rect, 16, |fx, _| {
        c.gamma_multiply(fx as f32)
    })));
    let x = rect.left() + shown as f32 * rect.width();
    let knob = Rect::from_center_size(pos2(x, rect.center().y), vec2(8.0, rect.height() + 6.0));
    painter.rect_filled(knob, 3.0, Color32::from_gray(190));
    painter.rect_stroke(
        knob,
        3.0,
        Stroke::new(1.0, Color32::from_gray(80)),
        egui::StrokeKind::Inside,
    );
    out
}

/// The eyedropper's wait for a click anywhere on the screen: the color
/// there, when the click came.
fn pick(ui: &mut Ui, view: &mut ColorView, id: egui::Id) -> Option<[f64; 3]> {
    let was_down = view.picking?;
    if bv_platform::SYSTEM_COLOR_SAMPLER {
        let Some(picked) = bv_platform::take_sampled_color() else {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
            return None;
        };
        view.picking = None;
        let rgb = picked?.map(|v| v as f64 / 255.0);
        view.numbers = None;
        return Some(rgb);
    }
    // VEGAS's color picker's dropper.
    cursors::set(ui.ctx(), Cursor::Dropper);
    let _ = id;
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        view.picking = None;
        bv_platform::release_mouse();
        return None;
    }
    let down = bv_platform::main_button_down();
    if down && !was_down {
        view.picking = None;
        bv_platform::release_mouse();
        let (x, y) = bv_platform::cursor_screen_position()?;
        let c = bv_platform::screen_color(x, y)?;
        let rgb = c.map(|v| v as f64 / 255.0);
        view.numbers = None;
        return Some(rgb);
    }
    view.picking = Some(down);
    ui.ctx().request_repaint();
    None
}

/// The eyedropper's button: a pipette, boxed while it waits.
fn eyedropper_button(ui: &mut Ui, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
    let tip = crate::i18n::tr("Pick a color from the screen");
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), on, tip)
    });
    let p = ui.painter();
    if on || resp.hovered() {
        p.rect_filled(rect, 2.0, theme::hover());
    }
    if on {
        p.rect_stroke(
            rect,
            2.0,
            Stroke::new(1.0, theme::selection()),
            egui::StrokeKind::Inside,
        );
    }
    let c = rect.center();
    let ink = if ui.is_enabled() {
        Color32::from_gray(220)
    } else {
        Color32::from_gray(110)
    };
    p.line_segment(
        [c + vec2(-6.0, 6.0), c + vec2(2.0, -2.0)],
        Stroke::new(2.5, ink),
    );
    p.line_segment(
        [c + vec2(-7.5, 7.5), c + vec2(-6.0, 6.0)],
        Stroke::new(1.0, ink),
    );
    p.circle_filled(c + vec2(4.0, -4.0), 3.0, ink);
    resp.on_hover_text(tip)
}

/// A mesh of `n`×`n` cells over `rect`, each corner's color given by its
/// place (0–1 across, 0–1 up).
fn gradient_mesh(rect: Rect, n: usize, color: impl Fn(f64, f64) -> Color32) -> Mesh {
    let mut mesh = Mesh::default();
    for j in 0..=n {
        for i in 0..=n {
            let (fx, fy) = (i as f64 / n as f64, j as f64 / n as f64);
            let at = pos2(
                rect.left() + fx as f32 * rect.width(),
                rect.bottom() - fy as f32 * rect.height(),
            );
            mesh.colored_vertex(at, color(fx, fy));
        }
    }
    let w = (n + 1) as u32;
    for j in 0..n as u32 {
        for i in 0..n as u32 {
            let a = j * w + i;
            mesh.add_triangle(a, a + 1, a + w);
            mesh.add_triangle(a + 1, a + w + 1, a + w);
        }
    }
    mesh
}

/// A number's place in its range (0–1), and back.
fn part(model: Model, k: usize, v: f64) -> f64 {
    let (lo, hi) = model.ranges()[k];
    ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
}

fn value(model: Model, k: usize, f: f64) -> f64 {
    let (lo, hi) = model.ranges()[k];
    lo + f.clamp(0.0, 1.0) * (hi - lo)
}

/// The plane: numbers `axes[0]` across and `axes[1]` up, the third as it
/// is; a click or a drag picks there.
fn plane_ui(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    model: Model,
    axes: [usize; 3],
    n: [f64; 3],
    enabled: bool,
) -> Option<[f64; 3]> {
    let [ax, ay, _] = axes;
    let resp = ui.interact(
        rect,
        id,
        if enabled {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        },
    );
    let mut out = None;
    if let Some(p) = resp.interact_pointer_pos()
        && (resp.is_pointer_button_down_on() || resp.clicked())
    {
        let mut m = n;
        m[ax] = value(model, ax, ((p.x - rect.left()) / rect.width()) as f64);
        m[ay] = value(model, ay, ((rect.bottom() - p.y) / rect.height()) as f64);
        out = Some(m);
    }
    let shown = out.unwrap_or(n);
    let mesh = gradient_mesh(rect, 24, |fx, fy| {
        let mut m = shown;
        m[ax] = value(model, ax, fx);
        m[ay] = value(model, ay, fy);
        color32(model.colour_of(m))
    });
    let painter = ui.painter_at(rect.expand(8.0));
    painter.add(egui::Shape::mesh(mesh));
    if !enabled {
        painter.rect_filled(rect, 0.0, Color32::from_black_alpha(110));
    }
    // The point: a cross, light over dark.
    let at = pos2(
        rect.left() + part(model, ax, shown[ax]) as f32 * rect.width(),
        rect.bottom() - part(model, ay, shown[ay]) as f32 * rect.height(),
    );
    cross(&painter, at);
    out
}

fn cross(p: &egui::Painter, at: Pos2) {
    for (w, c) in [(3.0, Color32::BLACK), (1.2, Color32::WHITE)] {
        p.line_segment(
            [at + vec2(-7.0, 0.0), at + vec2(7.0, 0.0)],
            Stroke::new(w, c),
        );
        p.line_segment(
            [at + vec2(0.0, -7.0), at + vec2(0.0, 7.0)],
            Stroke::new(w, c),
        );
    }
}

/// The slider of number `k`, up from its least, the others as they are.
fn slider_ui(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    model: Model,
    k: usize,
    n: [f64; 3],
    enabled: bool,
) -> Option<[f64; 3]> {
    let resp = ui.interact(
        rect.expand2(vec2(4.0, 6.0)),
        id,
        if enabled {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        },
    );
    let mut out = None;
    if let Some(p) = resp.interact_pointer_pos()
        && (resp.is_pointer_button_down_on() || resp.clicked())
    {
        let mut m = n;
        m[k] = value(model, k, ((rect.bottom() - p.y) / rect.height()) as f64);
        out = Some(m);
    }
    let shown = out.unwrap_or(n);
    let mesh = gradient_mesh(rect, 32, |_, fy| {
        let mut m = shown;
        m[k] = value(model, k, fy);
        color32(model.colour_of(m))
    });
    let painter = ui.painter_at(rect.expand(8.0));
    painter.add(egui::Shape::mesh(mesh));
    if !enabled {
        painter.rect_filled(rect, 0.0, Color32::from_black_alpha(110));
    }
    let y = rect.bottom() - part(model, k, shown[k]) as f32 * rect.height();
    let knob = Rect::from_center_size(pos2(rect.center().x, y), vec2(rect.width() + 6.0, 8.0));
    painter.rect_filled(knob, 3.0, Color32::from_gray(190));
    painter.rect_stroke(
        knob,
        3.0,
        Stroke::new(1.0, Color32::from_gray(80)),
        egui::StrokeKind::Inside,
    );
    out
}

/// The three numbers apart: a bar each, its letter beside it.
fn bars_ui(
    ui: &mut Ui,
    id: egui::Id,
    body: Rect,
    model: Model,
    n: [f64; 3],
    enabled: bool,
) -> Option<[f64; 3]> {
    let mut out = None;
    let letters = model.letters();
    for k in 0..3 {
        let y = body.top() + 6.0 + k as f32 * 30.0;
        let rect =
            Rect::from_min_size(pos2(body.left() + 16.0, y), vec2(body.width() - 16.0, 14.0));
        ui.painter().text(
            pos2(body.left(), rect.center().y),
            egui::Align2::LEFT_CENTER,
            letters[k],
            egui::FontId::proportional(12.0),
            theme::text(),
        );
        let resp = ui.interact(
            rect.expand2(vec2(4.0, 4.0)),
            id.with(k),
            if enabled {
                Sense::click_and_drag()
            } else {
                Sense::hover()
            },
        );
        let current = out.unwrap_or(n);
        if let Some(p) = resp.interact_pointer_pos()
            && (resp.is_pointer_button_down_on() || resp.clicked())
        {
            let mut m = current;
            m[k] = value(model, k, ((p.x - rect.left()) / rect.width()) as f64);
            out = Some(m);
        }
        let shown = out.unwrap_or(n);
        let mesh = gradient_mesh(rect, 32, |fx, _| {
            let mut m = shown;
            m[k] = value(model, k, fx);
            color32(model.colour_of(m))
        });
        let painter = ui.painter_at(rect.expand(8.0));
        painter.add(egui::Shape::mesh(mesh));
        let x = rect.left() + part(model, k, shown[k]) as f32 * rect.width();
        let knob = Rect::from_center_size(pos2(x, rect.center().y), vec2(8.0, rect.height() + 6.0));
        painter.rect_filled(knob, 3.0, Color32::from_gray(190));
        painter.rect_stroke(
            knob,
            3.0,
            Stroke::new(1.0, Color32::from_gray(80)),
            egui::StrokeKind::Inside,
        );
    }
    out
}

// Color models.

fn rgb_to_hsv([r, g, b]: [f64; 3]) -> [f64; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max > 0.0 { d / max } else { 0.0 };
    [h, s, max]
}

fn hsv_to_rgb([h, s, v]: [f64; 3]) -> [f64; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

/// sRGB's encoding, undone and done.
fn linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn encoded(c: f64) -> f64 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.max(0.0).powf(1.0 / 2.4) - 0.055
    }
}

/// D65's white.
const WHITE: [f64; 3] = [0.950_47, 1.0, 1.088_83];

fn rgb_to_lab(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(linear);
    let xyz = [
        0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b,
        0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b,
        0.019_333_9 * r + 0.119_192 * g + 0.950_304_1 * b,
    ];
    let f = |t: f64| {
        if t > 216.0 / 24389.0 {
            t.cbrt()
        } else {
            (24389.0 / 27.0 * t + 16.0) / 116.0
        }
    };
    let [fx, fy, fz] = [0, 1, 2].map(|i| f(xyz[i] / WHITE[i]));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn lab_to_rgb([l, a, b]: [f64; 3]) -> [f64; 3] {
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let inv = |f: f64| {
        let t = f * f * f;
        if t > 216.0 / 24389.0 {
            t
        } else {
            (116.0 * f - 16.0) * 27.0 / 24389.0
        }
    };
    let [x, y, z] = [inv(fx) * WHITE[0], inv(fy) * WHITE[1], inv(fz) * WHITE[2]];
    let r = 3.240_454_2 * x - 1.537_138_5 * y - 0.498_531_4 * z;
    let g = -0.969_266 * x + 1.876_010_8 * y + 0.041_556 * z;
    let bl = 0.055_643_4 * x - 0.204_025_9 * y + 1.057_225_2 * z;
    [r, g, bl].map(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_models_go_there_and_back() {
        for rgb in [
            [1.0, 0.0, 0.0],
            [0.2, 0.6, 0.9],
            [0.5, 0.5, 0.5],
            [0.0, 0.0, 0.0],
        ] {
            for m in Model::ALL {
                let back = m.colour_of(m.numbers_of(rgb));
                assert!(
                    same(back, rgb) || back.iter().zip(&rgb).all(|(a, b)| (a - b).abs() < 1e-3),
                    "{m:?} {rgb:?} {back:?}"
                );
            }
        }
        // VEGAS's numbers: red in HSV is "0, 1.0, 1.0", green's hue 120.
        assert_eq!(Model::Hsv.numbers_of([1.0, 0.0, 0.0]), [0.0, 1.0, 1.0]);
        assert_eq!(Model::Hsv.numbers_of([0.0, 1.0, 0.0])[0], 120.0);
        assert_eq!(Plane::Second.name(Model::Hsv), "(SxV) H");
        assert_eq!(Plane::First.name(Model::Rgb), "(RxG) B");
    }

    #[test]
    fn a_grey_keeps_the_hue_it_was_given() {
        let mut view = ColorView::default();
        // Black with the hue at 120°.
        let rgb = view.set([120.0, 1.0, 0.0]);
        assert_eq!(rgb, [0.0, 0.0, 0.0]);
        assert_eq!(view.numbers(rgb)[0], 120.0);
        assert_eq!(text(&view, [1.0, 1.0, 1.0], None), "0, 0.0, 1.0");
    }

    #[test]
    fn numbers_read_as_vegass() {
        // VEGAS's Pyramid's color in HSV, the Translucent Blue Border's in
        // RGB with its alpha, White Balance's neutral in Lab.
        let hsv = ColorView::starting_in("HSV");
        let pale = Model::Hsv.colour_of([56.0, 0.31, 1.0]);
        assert_eq!(text(&hsv, pale, None), "56, 0.31, 1.0");
        // VEGAS's Sepia presets' 8-bit colors, as its picker writes them.
        for (rgb, shown) in [
            ([166.0, 166.0, 0.0], "60, 1.0, 0.65"),
            ([203.0, 114.0, 0.0], "34, 1.0, 0.8"),
            ([98.0, 140.0, 98.0], "120, 0.3, 0.55"),
            ([166.0, 63.0, 0.0], "23, 1.0, 0.65"),
            ([255.0, 211.0, 26.0], "48, 0.9, 1.0"),
        ] {
            let rgb: [f64; 3] = rgb.map(|v| v / 255.0);
            assert_eq!(text(&hsv, rgb, None), shown);
        }
        let rgb = ColorView::starting_in("RGB");
        assert_eq!(text(&rgb, [0.0, 0.0, 1.0], Some(0.5)), "0.0, 0.0, 1.0, 0.5");
        assert_eq!(text(&rgb, [1.0, 1.0, 1.0], Some(1.0)), "1.0, 1.0, 1.0, 1.0");
        assert_eq!(number(53.391, 2, 1), "53.39");
        assert_eq!(number(-0.004, 2, 1), "0.0");
        assert_eq!(number(-0.01, 2, 1), "-0.01");
        // Typed: four numbers give the alpha, three keep it.
        let mut v = ColorView::starting_in("RGB");
        assert_eq!(
            parse(&mut v, "0, 0, 1, 0.5", true),
            Some(([0.0, 0.0, 1.0], Some(0.5)))
        );
        assert_eq!(
            parse(&mut v, "1, 0, 0", true),
            Some(([1.0, 0.0, 0.0], None))
        );
    }
}
