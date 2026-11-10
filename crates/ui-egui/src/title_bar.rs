//! The main window's title bar, laid out as VEGAS 23's: the program's
//! icon, the menus beside it, the window's title in the middle and the
//! window's buttons at the right. The system's title bar is off (`lib.rs`'
//! `run`), so this bar moves the window when dragged, maximizes or
//! restores it when double-clicked and has the window's menu on a right
//! click; `resize_edges` sizes the window from its edges. On macOS the
//! system's window buttons (close, minimize, zoom) stay at the bar's left,
//! as Mac programs have them, over BananaV's bar (`lib.rs`' `run`: the
//! content under a clear title bar); the system sizes the window and has
//! no window menu.

use crate::app::BananaApp;
use crate::egui::{
    self, Align2, Color32, CursorIcon, FontId, Rect, Sense, Shape, Stroke, StrokeKind, UiBuilder,
    pos2, vec2,
};
use crate::i18n::tr;
use crate::theme;

/// The bar's height, VEGAS's.
pub(crate) const HEIGHT: f32 = 31.0;
/// A window button's width, as the system's.
const BUTTON_W: f32 = 47.0;
/// How far in from the window's edges the pointer sizes it.
const EDGE: f32 = 5.0;
/// The room the Mac's window buttons take at the bar's left.
const MAC_BUTTONS: f32 = 78.0;
/// The system's own window buttons and frame (macOS).
const SYSTEM_FRAME: bool = cfg!(target_os = "macos");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Button {
    Minimize,
    Maximize,
    Close,
}

/// The bar, in `ui` (a panel as tall as `HEIGHT` across the window).
pub(crate) fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    let ctx = ui.ctx().clone();
    let bar = ui.max_rect();
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    // The bar itself, under the menus and buttons drawn after it.
    let resp = ui.interact(bar, ui.id().with("title-bar"), Sense::click_and_drag());
    if resp.drag_started_by(egui::PointerButton::Primary) {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if resp.double_clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    if !SYSTEM_FRAME {
        resp.context_menu(|ui| window_menu(ui, maximized));
    }

    // The program's icon, where VEGAS has its own (on a Mac the window's
    // buttons are there; in full screen they hide).
    let icon = if SYSTEM_FRAME {
        let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        let room = if full { 4.0 } else { MAC_BUTTONS };
        Rect::from_min_size(
            pos2(bar.left() + room - 9.0, bar.center().y),
            vec2(0.0, 0.0),
        )
    } else {
        let icon = Rect::from_min_size(
            pos2(bar.left() + 5.0, bar.center().y - 10.0),
            vec2(20.0, 20.0),
        );
        if let Some(texture) = program_icon(&ctx) {
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            ui.painter().image(texture.id(), icon, uv, Color32::WHITE);
        }
        icon
    };

    let buttons_left = if SYSTEM_FRAME {
        bar.right()
    } else {
        bar.right() - 3.0 * BUTTON_W
    };
    let row = ui.spacing().interact_size.y;
    let menus = Rect::from_min_max(
        pos2(icon.right() + 9.0, bar.center().y - row / 2.0),
        pos2(buttons_left, bar.center().y + row / 2.0),
    );
    let menus_end = ui
        .scope_builder(UiBuilder::new().max_rect(menus), |ui| {
            crate::menus::menu_bar(ui, app)
        })
        .inner;

    // The window's title in the middle, or as near it as the menus let it.
    let (start, end) = (menus_end + 16.0, buttons_left - 16.0);
    if end - start > 40.0 {
        let font = FontId::proportional(12.0);
        let title = app.window_title().to_string();
        let width = ui
            .painter()
            .layout_no_wrap(title.clone(), font.clone(), theme::text_dim())
            .size()
            .x;
        let left = (bar.center().x - width / 2.0).clamp(start, (end - width).max(start));
        crate::widgets::fit_text(
            ui.painter(),
            pos2(left, bar.center().y),
            Align2::LEFT_CENTER,
            title,
            font,
            theme::text_dim(),
            end - left,
        );
    }

    let buttons: &[Button] = if SYSTEM_FRAME {
        &[]
    } else {
        &[Button::Minimize, Button::Maximize, Button::Close]
    };
    for (i, &button) in buttons.iter().enumerate() {
        let r = Rect::from_min_max(
            pos2(buttons_left + i as f32 * BUTTON_W, bar.top()),
            pos2(buttons_left + (i + 1) as f32 * BUTTON_W, bar.bottom()),
        );
        if window_button(ui, r, button, maximized).clicked() {
            ctx.send_viewport_cmd(match button {
                Button::Minimize => egui::ViewportCommand::Minimized(true),
                Button::Maximize => egui::ViewportCommand::Maximized(!maximized),
                Button::Close => egui::ViewportCommand::Close,
            });
        }
    }
}

/// Minimize, Maximize (Restore Down while maximized) or Close: the
/// system's glyphs, lit under the pointer — Close in red.
fn window_button(ui: &egui::Ui, r: Rect, button: Button, maximized: bool) -> egui::Response {
    let resp = ui.interact(r, ui.id().with(("window-button", button)), Sense::click());
    let hot = resp.hovered() || resp.is_pointer_button_down_on();
    let painter = ui.painter();
    let close = button == Button::Close;
    if hot {
        let fill = if close {
            Color32::from_rgb(0xC4, 0x2B, 0x1C)
        } else {
            theme::hover()
        };
        painter.rect_filled(r, 0.0, fill);
    }
    let color = match (hot, close) {
        (true, true) => Color32::WHITE,
        (true, false) => theme::text(),
        _ => theme::text_dim(),
    };
    // On the pixels' centers, so the lines stay sharp.
    let ppp = ui.ctx().pixels_per_point();
    let px = |v: f32| ((v * ppp).floor() + 0.5) / ppp;
    let c = r.center();
    let (x0, x1, y0, y1) = (px(c.x - 5.0), px(c.x + 5.0), px(c.y - 5.0), px(c.y + 5.0));
    let stroke = Stroke::new(1.0, color);
    match button {
        Button::Minimize => {
            painter.line_segment([pos2(x0, px(c.y)), pos2(x1, px(c.y))], stroke);
        }
        Button::Maximize if maximized => {
            let front = Rect::from_min_max(pos2(x0, y0 + 2.0), pos2(x1 - 2.0, y1));
            painter.rect_stroke(front, 0.0, stroke, StrokeKind::Middle);
            painter.add(Shape::line(
                vec![
                    pos2(x0 + 2.0, y0 + 2.0),
                    pos2(x0 + 2.0, y0),
                    pos2(x1, y0),
                    pos2(x1, y1 - 2.0),
                    pos2(x1 - 2.0, y1 - 2.0),
                ],
                stroke,
            ));
        }
        Button::Maximize => {
            let square = Rect::from_min_max(pos2(x0, y0), pos2(x1, y1));
            painter.rect_stroke(square, 0.0, stroke, StrokeKind::Middle);
        }
        Button::Close => {
            painter.line_segment([pos2(x0, y0), pos2(x1, y1)], stroke);
            painter.line_segment([pos2(x1, y0), pos2(x0, y1)], stroke);
        }
    }
    resp.on_hover_text(match button {
        Button::Minimize => tr("Minimize"),
        Button::Maximize if maximized => tr("Restore Down"),
        Button::Maximize => tr("Maximize"),
        Button::Close => tr("Close"),
    })
}

/// The window's menu, as the system's title bar has it.
fn window_menu(ui: &mut egui::Ui, maximized: bool) {
    let ctx = ui.ctx().clone();
    if ui
        .add_enabled(maximized, egui::Button::new(tr("Restore")))
        .clicked()
    {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
        ui.close();
    }
    if ui.button(tr("Minimize")).clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        ui.close();
    }
    if ui
        .add_enabled(!maximized, egui::Button::new(tr("Maximize")))
        .clicked()
    {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        ui.close();
    }
    ui.separator();
    if ui
        .add(egui::Button::new(tr("Close")).shortcut_text("Alt+F4"))
        .clicked()
    {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        ui.close();
    }
}

/// The program's icon (the window's, `crate::window_icon`) as a texture,
/// made once.
fn program_icon(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let id = egui::Id::new("program-icon");
    if let Some(texture) = ctx.data_mut(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(texture);
    }
    let icon = crate::window_icon()?;
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [icon.width as usize, icon.height as usize],
        &icon.rgba,
    );
    // Drawn far smaller than its 256 pixels: through its mipmaps.
    let options = egui::TextureOptions {
        mipmap_mode: Some(egui::TextureFilter::Linear),
        ..egui::TextureOptions::LINEAR
    };
    let texture = ctx.load_texture("program-icon", image, options);
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    Some(texture)
}

/// The window's edges and corners (`window`, the whole window) size it,
/// while it is neither maximized nor full screen: the system's frame,
/// which would, is off.
pub(crate) fn resize_edges(ui: &egui::Ui, window: Rect) {
    use egui::viewport::ResizeDirection as Dir;
    if SYSTEM_FRAME {
        return;
    }
    let ctx = ui.ctx();
    let framed = ctx.input(|i| {
        let v = i.viewport();
        !v.maximized.unwrap_or(false) && !v.fullscreen.unwrap_or(false)
    });
    if !framed {
        return;
    }
    let (l, t, r, b) = (window.left(), window.top(), window.right(), window.bottom());
    let c = EDGE * 2.0;
    let zones = [
        (
            Dir::NorthWest,
            (l, t, l + c, t + c),
            CursorIcon::ResizeNorthWest,
        ),
        (
            Dir::NorthEast,
            (r - c, t, r, t + c),
            CursorIcon::ResizeNorthEast,
        ),
        (
            Dir::SouthWest,
            (l, b - c, l + c, b),
            CursorIcon::ResizeSouthWest,
        ),
        (
            Dir::SouthEast,
            (r - c, b - c, r, b),
            CursorIcon::ResizeSouthEast,
        ),
        (
            Dir::North,
            (l + c, t, r - c, t + EDGE),
            CursorIcon::ResizeNorth,
        ),
        (
            Dir::South,
            (l + c, b - EDGE, r - c, b),
            CursorIcon::ResizeSouth,
        ),
        (
            Dir::West,
            (l, t + c, l + EDGE, b - c),
            CursorIcon::ResizeWest,
        ),
        (
            Dir::East,
            (r - EDGE, t + c, r, b - c),
            CursorIcon::ResizeEast,
        ),
    ];
    for (i, (dir, (x0, y0, x1, y1), cursor)) in zones.into_iter().enumerate() {
        let zone = Rect::from_min_max(pos2(x0, y0), pos2(x1, y1));
        let resp = ui.interact(zone, egui::Id::new(("window-edge", i)), Sense::drag());
        if resp.hovered() || resp.dragged() {
            crate::cursors::system(ctx, cursor);
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }
}
