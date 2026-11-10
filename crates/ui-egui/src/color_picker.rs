//! VEGAS's color picker, the dialog a color swatch opens: an alpha bar
//! (which also shows the color), red, green and blue bars (hue, saturation
//! and luminance after the color model button), a hue × luminance spectrum,
//! the values as numbers, and an eyedropper that takes a color from
//! anywhere on the screen.

use crate::cursors::{self, Cursor};
use crate::egui::{
    self, Color32, ColorImage, Mesh, Rect, Sense, Shape, Stroke, TextureHandle, TextureOptions, Ui,
    Vec2, pos2, vec2,
};
use crate::i18n::tr;
use crate::theme;

/// How a picker ended this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Open,
    Cancelled,
    /// OK: the color chosen (8-bit RGBA, not premultiplied).
    Chosen([u8; 4]),
}

pub struct ColorPicker {
    title: String,
    /// Red, green and blue, 0–1.
    rgb: [f32; 3],
    /// Hue, saturation and luminance, 0–1 (the hue and saturation are kept
    /// where gray, black or white have none).
    hsl: [f32; 3],
    alpha: f32,
    /// The bars show hue, saturation and luminance.
    hsl_bars: bool,
    /// The eyedropper waits for a click: whether the mouse button was down
    /// the last frame.
    picking: Option<bool>,
    spectrum: Option<TextureHandle>,
}

/// Width of the bars' and spectrum's area.
const WIDTH: f32 = 360.0;
/// The alpha bar and the buttons below it.
const SIDE: Vec2 = vec2(24.0, 66.0);
const BUTTON: Vec2 = vec2(24.0, 23.0);
const BAR_HEIGHT: f32 = 12.0;
/// A bar and its marker.
const BAR_STEP: f32 = 23.0;
const SPECTRUM_HEIGHT: f32 = 56.0;

impl ColorPicker {
    /// A picker titled `title` (what the color is for) starting at `rgba`.
    pub fn new(title: impl Into<String>, rgba: [u8; 4]) -> ColorPicker {
        let rgb = [rgba[0], rgba[1], rgba[2]].map(|v| v as f32 / 255.0);
        ColorPicker {
            title: title.into(),
            rgb,
            hsl: rgb_to_hsl(rgb),
            alpha: rgba[3] as f32 / 255.0,
            hsl_bars: false,
            picking: None,
            spectrum: None,
        }
    }

    /// The color as 8-bit RGBA.
    pub fn rgba(&self) -> [u8; 4] {
        let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [
            to8(self.rgb[0]),
            to8(self.rgb[1]),
            to8(self.rgb[2]),
            to8(self.alpha),
        ]
    }

    fn set_rgb(&mut self, rgb: [f32; 3]) {
        self.rgb = rgb.map(|v| v.clamp(0.0, 1.0));
        let [h, s, l] = rgb_to_hsl(self.rgb);
        // Gray has no hue, black and white no saturation: the old ones stay.
        let h = if s > 0.0 { h } else { self.hsl[0] };
        let s = if l > 0.0 && l < 1.0 { s } else { self.hsl[1] };
        self.hsl = [h, s, l];
    }

    fn set_hsl(&mut self, hsl: [f32; 3]) {
        self.hsl = hsl.map(|v| v.clamp(0.0, 1.0));
        self.rgb = hsl_to_rgb(self.hsl);
    }

    fn color(&self, alpha: f32) -> Color32 {
        let [r, g, b] = to_u8(self.rgb);
        Color32::from_rgba_unmultiplied(r, g, b, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
    }

    /// Draws the dialog; ends with OK, Cancel or its close button.
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome {
        let mut open = true;
        let mut outcome = Outcome::Open;
        crate::floating::Window::new(self.title.clone())
            .id(egui::Id::new(("color-picker", self.title.as_str())))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| outcome = self.contents(ui));
        if !open {
            outcome = Outcome::Cancelled;
        }
        if outcome != Outcome::Open && self.picking.take().is_some() {
            bv_platform::release_mouse();
        }
        outcome
    }

    fn contents(&mut self, ui: &mut Ui) -> Outcome {
        let id = ui.id().with("color-picker");
        // Room for the alpha marker at the top.
        ui.add_space(6.0);
        let (area, _) =
            ui.allocate_exact_size(vec2(WIDTH, SIDE.y + 8.0 + SPECTRUM_HEIGHT), Sense::hover());
        self.alpha_bar(ui, id, Rect::from_min_size(area.min, SIDE));
        self.bars(ui, id, area);
        let below = area.top() + SIDE.y + 8.0;
        let model = Rect::from_min_size(pos2(area.left(), below), BUTTON);
        let dropper = Rect::from_min_size(pos2(area.left(), below + BUTTON.y + 4.0), BUTTON);
        if self.model_button(ui, id, model).clicked() {
            self.hsl_bars = !self.hsl_bars;
        }
        let tip = tr("Take a color from the screen: click this, then anywhere (Esc cancels)");
        let picking = self.picking.is_some();
        let r = ui.interact(dropper, id.with("dropper"), Sense::click());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
        let r = r.on_hover_text(tip);
        tool_frame(ui, &dropper, &r, picking);
        crate::icons::paint(
            ui.painter(),
            dropper.shrink(5.0),
            crate::icons::Icon::Eyedropper,
            theme::text(),
        );
        if r.clicked() {
            if picking {
                self.picking = None;
                bv_platform::release_mouse();
            } else if bv_platform::SYSTEM_COLOR_SAMPLER {
                // The system's sampler (macOS): its magnifier takes the
                // click and the color.
                if bv_platform::start_color_sampler() {
                    self.picking = Some(false);
                }
            } else {
                // Clicks on the program's other windows come here instead of
                // acting on them.
                bv_platform::capture_mouse(&self.title);
                self.picking = Some(bv_platform::main_button_down());
            }
        }
        let spectrum = Rect::from_min_max(
            pos2(area.left() + SIDE.x + 8.0, below),
            pos2(area.right(), below + SPECTRUM_HEIGHT),
        );
        self.spectrum(ui, id, spectrum);
        ui.add_space(10.0);
        self.numbers(ui);
        ui.add_space(10.0);
        let mut outcome = Outcome::Open;
        let row = vec2(
            ui.available_width().max(WIDTH),
            ui.spacing().interact_size.y,
        );
        let layout = egui::Layout::right_to_left(egui::Align::Center);
        ui.allocate_ui_with_layout(row, layout, |ui| {
            let size = vec2(72.0, 0.0);
            if ui
                .add(egui::Button::new(tr("Cancel")).min_size(size))
                .clicked()
            {
                outcome = Outcome::Cancelled;
            }
            if ui.add(egui::Button::new("OK").min_size(size)).clicked() {
                outcome = Outcome::Chosen(self.rgba());
            }
        });
        self.eyedropper(ui, id);
        let (enter, escape) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if escape && self.picking.take().is_some() {
            bv_platform::release_mouse();
        } else if escape {
            outcome = Outcome::Cancelled;
        } else if enter && !ui.ctx().egui_wants_keyboard_input() {
            outcome = Outcome::Chosen(self.rgba());
        }
        outcome
    }

    /// While the eyedropper waits: the next click, wherever it lands, takes
    /// the color under the mouse. The dialog's own controls do not take
    /// that click.
    fn eyedropper(&mut self, ui: &mut Ui, id: egui::Id) {
        let Some(was_down) = self.picking else {
            return;
        };
        if bv_platform::SYSTEM_COLOR_SAMPLER {
            match bv_platform::take_sampled_color() {
                Some(picked) => {
                    if let Some(c) = picked {
                        self.set_rgb(c.map(|v| v as f32 / 255.0));
                    }
                    self.picking = None;
                }
                None => ui
                    .ctx()
                    .request_repaint_after(std::time::Duration::from_millis(50)),
            }
            return;
        }
        ui.interact(ui.max_rect(), id.with("picking"), Sense::click_and_drag());
        // VEGAS's color picker's dropper.
        cursors::set(ui.ctx(), Cursor::Dropper);
        let down = bv_platform::main_button_down();
        if down && !was_down {
            if let Some((x, y)) = bv_platform::cursor_screen_position()
                && let Some(c) = bv_platform::screen_color(x, y)
            {
                self.set_rgb(c.map(|v| v as f32 / 255.0));
            }
            // The capture ends with the click.
            self.picking = None;
        } else {
            self.picking = Some(down);
            ui.ctx().request_repaint();
        }
    }

    /// The alpha bar: the color over a checkerboard, opaque at the top,
    /// clear at the bottom; the marker at the alpha.
    fn alpha_bar(&mut self, ui: &mut Ui, id: egui::Id, rect: Rect) {
        let r = ui.interact(rect, id.with("alpha"), Sense::click_and_drag());
        if let Some(p) = r.interact_pointer_pos()
            && (r.is_pointer_button_down_on() || r.clicked())
        {
            self.alpha = (1.0 - (p.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        }
        let painter = ui.painter_at(rect.expand(8.0));
        checkerboard(&painter, rect);
        gradient(&painter, rect, &[self.color(1.0), self.color(0.0)], true);
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, theme::separator()),
            egui::StrokeKind::Outside,
        );
        let y = rect.top() + (1.0 - self.alpha) * rect.height();
        painter.hline(
            rect.x_range(),
            y,
            Stroke::new(1.5, Color32::from_rgb(230, 40, 40)),
        );
        marker(
            &painter,
            [
                pos2(rect.right() + 1.0, y),
                pos2(rect.right() + 8.0, y - 5.0),
                pos2(rect.right() + 8.0, y + 5.0),
            ],
        );
    }

    /// The three bars: red, green and blue (or hue, saturation and
    /// luminance), each its channel's range with the others as they are.
    fn bars(&mut self, ui: &mut Ui, id: egui::Id, area: Rect) {
        let left = area.left() + SIDE.x + 22.0;
        let names = if self.hsl_bars {
            ["H", "S", "L"]
        } else {
            ["R", "G", "B"]
        };
        for (k, name) in names.into_iter().enumerate() {
            let top = area.top() + 2.0 + k as f32 * BAR_STEP;
            let bar =
                Rect::from_min_max(pos2(left, top), pos2(area.right() - 6.0, top + BAR_HEIGHT));
            ui.painter().text(
                pos2(left - 8.0, bar.center().y),
                egui::Align2::CENTER_CENTER,
                name,
                egui::FontId::proportional(11.0),
                theme::text_dim(),
            );
            let hit = Rect::from_min_max(bar.min, pos2(bar.right(), bar.bottom() + 8.0));
            let r = ui.interact(hit, id.with(("bar", k)), Sense::click_and_drag());
            if let Some(p) = r.interact_pointer_pos()
                && (r.is_pointer_button_down_on() || r.clicked())
            {
                let v = ((p.x - bar.left()) / bar.width()).clamp(0.0, 1.0);
                if self.hsl_bars {
                    let mut hsl = self.hsl;
                    hsl[k] = v;
                    self.set_hsl(hsl);
                } else {
                    let mut rgb = self.rgb;
                    rgb[k] = v;
                    self.set_rgb(rgb);
                }
            }
            // The bar's colors, at evenly spaced stops.
            let stops: Vec<Color32> = if self.hsl_bars {
                let steps = if k == 1 { 1 } else { 36 };
                (0..=steps)
                    .map(|i| {
                        let mut hsl = self.hsl;
                        hsl[k] = i as f32 / steps as f32;
                        rgb_color(hsl_to_rgb(hsl))
                    })
                    .collect()
            } else {
                [0.0, 1.0]
                    .map(|v| {
                        let mut rgb = self.rgb;
                        rgb[k] = v;
                        rgb_color(rgb)
                    })
                    .to_vec()
            };
            let painter = ui.painter_at(hit.expand(6.0));
            gradient(&painter, bar, &stops, false);
            painter.rect_stroke(
                bar,
                0.0,
                Stroke::new(1.0, theme::separator()),
                egui::StrokeKind::Outside,
            );
            let v = if self.hsl_bars {
                self.hsl[k]
            } else {
                self.rgb[k]
            };
            let x = bar.left() + v * bar.width();
            marker(
                &painter,
                [
                    pos2(x, bar.bottom() + 1.0),
                    pos2(x - 5.0, bar.bottom() + 8.0),
                    pos2(x + 5.0, bar.bottom() + 8.0),
                ],
            );
        }
    }

    /// The color model button: red, green and blue bars or hue, saturation
    /// and luminance ones.
    fn model_button(&mut self, ui: &mut Ui, id: egui::Id, rect: Rect) -> egui::Response {
        let tip = tr("Red, green and blue bars, or hue, saturation and luminance ones");
        let r = ui.interact(rect, id.with("model"), Sense::click());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
        let r = r.on_hover_text(tip);
        tool_frame(ui, &rect, &r, self.hsl_bars);
        // A little spectrum, and the three primaries.
        let inner = rect.shrink2(vec2(6.0, 5.0));
        let half = (inner.width() - 2.0) / 2.0;
        let column = Rect::from_min_size(inner.min, vec2(half, inner.height()));
        let rainbow: Vec<Color32> = (0..=6)
            .map(|i| rgb_color(hsl_to_rgb([i as f32 / 6.0, 1.0, 0.5])))
            .collect();
        gradient(ui.painter(), column, &rainbow, true);
        let square = inner.height() / 3.0;
        for (i, c) in [
            Color32::from_rgb(230, 30, 30),
            Color32::from_rgb(30, 200, 60),
            Color32::from_rgb(40, 80, 230),
        ]
        .into_iter()
        .enumerate()
        {
            let min = pos2(inner.right() - half, inner.top() + i as f32 * square);
            ui.painter()
                .rect_filled(Rect::from_min_size(min, vec2(half, square)), 0.0, c);
        }
        r
    }

    /// The spectrum: hue across, luminance down (white to black); a click
    /// takes the color under it.
    fn spectrum(&mut self, ui: &mut Ui, id: egui::Id, rect: Rect) {
        let r = ui.interact(rect, id.with("spectrum"), Sense::click_and_drag());
        if let Some(p) = r.interact_pointer_pos()
            && (r.is_pointer_button_down_on() || r.clicked())
        {
            let h = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let l = 1.0 - ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            self.set_hsl([h, 1.0, l]);
        }
        let texture = self.spectrum.get_or_insert_with(|| {
            ui.ctx()
                .load_texture("color-spectrum", spectrum_image(), TextureOptions::LINEAR)
        });
        let painter = ui.painter_at(rect);
        painter.image(
            texture.id(),
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        // The marker: four small triangles pointing at the color.
        let c = pos2(
            rect.left() + self.hsl[0] * rect.width(),
            rect.top() + (1.0 - self.hsl[2]) * rect.height(),
        );
        for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let tip = c + vec2(dx * 2.0, dy * 2.0);
            marker(
                &painter,
                [tip, tip + vec2(dx * 6.0, 0.0), tip + vec2(0.0, dy * 6.0)],
            );
        }
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, theme::separator()),
            egui::StrokeKind::Inside,
        );
    }

    /// The values: red, green, blue and alpha 0–255, hue, saturation and
    /// luminance 0–1.
    fn numbers(&mut self, ui: &mut Ui) {
        let field = |ui: &mut Ui, v: &mut f64, max: f64| {
            ui.add_sized(
                vec2(56.0, ui.spacing().interact_size.y),
                egui::DragValue::new(v)
                    .range(0.0..=max)
                    .speed(max / 255.0)
                    .custom_formatter(|v, _| crate::fmt::num(v, 1))
                    .custom_parser(crate::fmt::parse),
            )
            .changed()
        };
        let rgb_names = [tr("Red:"), tr("Green:"), tr("Blue:")];
        let hsl_names = [tr("Hue:"), tr("Saturation:"), tr("Luminance:")];
        egui::Grid::new("color-picker-numbers")
            .num_columns(6)
            .spacing(vec2(8.0, 6.0))
            .show(ui, |ui| {
                for k in 0..3 {
                    ui.label(rgb_names[k]);
                    let mut v = self.rgb[k] as f64 * 255.0;
                    if field(ui, &mut v, 255.0) {
                        let mut rgb = self.rgb;
                        rgb[k] = (v / 255.0) as f32;
                        self.set_rgb(rgb);
                    }
                    ui.label(hsl_names[k]);
                    let mut v = self.hsl[k] as f64;
                    if field(ui, &mut v, 1.0) {
                        let mut hsl = self.hsl;
                        hsl[k] = v as f32;
                        self.set_hsl(hsl);
                    }
                    if k == 0 {
                        ui.label(tr("Alpha:"));
                        let mut v = self.alpha as f64 * 255.0;
                        if field(ui, &mut v, 255.0) {
                            self.alpha = (v / 255.0) as f32;
                        }
                    }
                    ui.end_row();
                }
            });
    }
}

impl std::fmt::Debug for ColorPicker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColorPicker")
            .field("title", &self.title)
            .field("rgba", &self.rgba())
            .finish()
    }
}

impl Drop for ColorPicker {
    fn drop(&mut self) {
        // Closed while the eyedropper waited: the mouse is let go.
        if self.picking.is_some() {
            bv_platform::release_mouse();
        }
    }
}

/// The frame of the picker's small tool buttons (lit while on).
fn tool_frame(ui: &Ui, rect: &Rect, r: &egui::Response, on: bool) {
    let fill = if on || r.is_pointer_button_down_on() || r.hovered() {
        theme::hover()
    } else {
        theme::panel()
    };
    ui.painter().rect(
        *rect,
        3.0,
        fill,
        Stroke::new(
            1.0,
            if on {
                theme::banana()
            } else {
                theme::separator()
            },
        ),
        egui::StrokeKind::Inside,
    );
}

/// A hollow triangle marker, light with a dark edge.
fn marker(painter: &egui::Painter, points: [egui::Pos2; 3]) {
    painter.add(Shape::convex_polygon(
        points.to_vec(),
        Color32::WHITE,
        Stroke::new(1.0, Color32::BLACK),
    ));
}

/// Colors at evenly spaced stops across `rect` (down it when `vertical`).
fn gradient(painter: &egui::Painter, rect: Rect, colors: &[Color32], vertical: bool) {
    let n = colors.len().saturating_sub(1).max(1);
    let mut mesh = Mesh::default();
    for (i, &c) in colors.iter().enumerate() {
        let f = i as f32 / n as f32;
        if vertical {
            let y = rect.top() + f * rect.height();
            mesh.colored_vertex(pos2(rect.left(), y), c);
            mesh.colored_vertex(pos2(rect.right(), y), c);
        } else {
            let x = rect.left() + f * rect.width();
            mesh.colored_vertex(pos2(x, rect.top()), c);
            mesh.colored_vertex(pos2(x, rect.bottom()), c);
        }
    }
    for i in 0..(colors.len().saturating_sub(1)) as u32 {
        let k = i * 2;
        mesh.add_triangle(k, k + 1, k + 2);
        mesh.add_triangle(k + 1, k + 2, k + 3);
    }
    painter.add(Shape::mesh(mesh));
}

/// The gray squares under see-through colors.
pub(crate) fn checkerboard(painter: &egui::Painter, rect: Rect) {
    const CELL: f32 = 6.0;
    painter.rect_filled(rect, 0.0, Color32::from_gray(200));
    let (cols, rows) = (
        (rect.width() / CELL).ceil() as i32,
        (rect.height() / CELL).ceil() as i32,
    );
    let painter = painter.with_clip_rect(rect);
    for row in 0..rows {
        for col in (row % 2..cols).step_by(2) {
            let min = rect.min + vec2(col as f32 * CELL, row as f32 * CELL);
            painter.rect_filled(
                Rect::from_min_size(min, Vec2::splat(CELL)),
                0.0,
                Color32::from_gray(130),
            );
        }
    }
}

/// The hue × luminance field (full saturation): white at the top, the
/// pure colors halfway, black at the bottom.
fn spectrum_image() -> ColorImage {
    const W: usize = 360;
    const H: usize = 64;
    let mut pixels = Vec::with_capacity(W * H);
    for y in 0..H {
        let l = 1.0 - y as f32 / (H - 1) as f32;
        for x in 0..W {
            let h = x as f32 / (W - 1) as f32;
            pixels.push(rgb_color(hsl_to_rgb([h, 1.0, l])));
        }
    }
    ColorImage::new([W, H], pixels)
}

fn to_u8(rgb: [f32; 3]) -> [u8; 3] {
    rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn rgb_color(rgb: [f32; 3]) -> Color32 {
    let [r, g, b] = to_u8(rgb);
    Color32::from_rgb(r, g, b)
}

/// Red, green and blue (0–1) to hue, saturation and luminance (0–1).
pub(crate) fn rgb_to_hsl([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d <= f32::EPSILON {
        return [0.0, 0.0, l];
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h / 6.0, s, l]
}

/// Hue, saturation and luminance (0–1) to red, green and blue (0–1).
pub(crate) fn hsl_to_rgb([h, s, l]: [f32; 3]) -> [f32; 3] {
    if s <= 0.0 {
        return [l, l, l];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let channel = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vegas_take_indicator_in_hsl() {
        // VEGAS shows 64, 255, 255 as hue 0.5, saturation 1, luminance 0.6.
        let [h, s, l] = rgb_to_hsl([64.0 / 255.0, 1.0, 1.0]);
        assert!((h - 0.5).abs() < 1e-6, "{h}");
        assert!((s - 1.0).abs() < 1e-6, "{s}");
        assert!((l - 0.625).abs() < 0.001, "{l}");
    }

    #[test]
    fn hsl_and_back() {
        for rgb in [
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.5, 0.5, 0.5],
            [1.0, 0.0, 0.0],
            [0.2, 0.7, 0.4],
            [0.9, 0.1, 0.8],
            [0.25, 1.0, 1.0],
        ] {
            let back = hsl_to_rgb(rgb_to_hsl(rgb));
            for k in 0..3 {
                assert!((back[k] - rgb[k]).abs() < 1e-5, "{rgb:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn gray_keeps_its_hue() {
        let mut p = ColorPicker::new("test", [64, 255, 255, 255]);
        p.set_rgb([0.5, 0.5, 0.5]);
        assert!((p.hsl[0] - 0.5).abs() < 1e-6, "hue kept");
        p.set_rgb([1.0, 1.0, 1.0]);
        assert_eq!(p.hsl[2], 1.0);
        assert_eq!(p.rgba(), [255, 255, 255, 255]);
    }

    #[test]
    fn the_spectrum_runs_from_white_to_black() {
        let img = spectrum_image();
        let [w, h] = img.size;
        assert_eq!(img.pixels[0], Color32::WHITE);
        assert_eq!(img.pixels[(h - 1) * w + w / 2], Color32::BLACK);
        // Halfway down, the pure colors: red at the left.
        let mid = img.pixels[(h / 2) * w];
        assert!(mid.r() > 240 && mid.g() < 20 && mid.b() < 20, "{mid:?}");
    }
}
