//! Dialog windows as real OS windows (egui viewports), like VEGAS's: they
//! can be dragged out of the main window, even to another monitor, and are
//! owned by it (they stay in front of it, minimize with it and have no
//! taskbar button of their own).
//!
//! [`Window`] has the builder of `egui::Window` that the dialogs use. Where
//! the backend cannot open more windows (the headless test harness) it falls
//! back to an `egui::Window` inside the main one, with the same options.

use std::sync::Mutex;

use crate::egui::{
    self, Align2, Context, Id, Pos2, Ui, Vec2, ViewportBuilder, ViewportId, pos2, vec2,
};

/// Title of the main window, the owner of the dialogs.
static MAIN_TITLE: Mutex<String> = Mutex::new(String::new());

/// Where the context keeps the dialogs taking input while one is modal.
fn modal_key() -> Id {
    Id::new("floating-modal")
}

/// Makes `ids` the only dialogs that take input (none: every window
/// does). The main window and the other dialogs take none, as under
/// VEGAS's modal windows (its render's progress).
pub(crate) fn set_modal(ctx: &Context, ids: &[Id]) {
    ctx.data_mut(|d| {
        if ids.is_empty() {
            d.remove::<Vec<Id>>(modal_key());
        } else {
            d.insert_temp(modal_key(), ids.to_vec());
        }
    });
}

/// A modal dialog is up: the rest of the program takes no input.
pub(crate) fn modal(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<Vec<Id>>(modal_key()))
        .is_some_and(|m| !m.is_empty())
}

/// The dialog `id` takes no input now (another one is modal).
fn blocked(ctx: &Context, id: Id) -> bool {
    ctx.data(|d| d.get_temp::<Vec<Id>>(modal_key()))
        .is_some_and(|m| !m.is_empty() && !m.contains(&id))
}

/// Covers the viewport being drawn with a widget that takes every click
/// and drag, above what `order` passes (a blocked window's contents): what
/// is under it takes none. `own_window`: the whole viewport is blocked, its
/// keyboard focus too (an embedded dialog may be the modal one).
pub(crate) fn shield(ctx: &Context, order: egui::Order, own_window: bool) {
    let rect = ctx.viewport_rect();
    egui::Area::new(Id::new("modal-shield"))
        .order(order)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            ui.allocate_rect(rect, egui::Sense::click_and_drag());
        });
    if own_window {
        ctx.memory_mut(|m| {
            if let Some(f) = m.focused() {
                m.surrender_focus(f);
            }
        });
    }
}

/// The app tells the dialogs which window is theirs when its title changes.
pub fn set_main_title(title: &str) {
    if let Ok(mut t) = MAIN_TITLE.lock() {
        title.clone_into(&mut t);
    }
}

pub(crate) fn main_title() -> String {
    MAIN_TITLE
        .lock()
        .map(|t| t.clone())
        .ok()
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "BananaV".into())
}

/// Margin between the native window's border and the dialog's content.
const MARGIN: f32 = 10.0;

/// What a native dialog remembers between frames (and between openings:
/// a dialog opened again starts at the size it had).
#[derive(Clone, Copy, Debug, Default)]
struct State {
    /// Content size measured last frame (auto-sized dialogs).
    size: Option<Vec2>,
    /// Screen position, chosen when it opens: centered on the main window.
    pos: Option<Pos2>,
    /// The size the position was centered for.
    centered_for: Option<Vec2>,
    /// Times it was centered since it opened (at the guess, then at the
    /// measured size; later size changes keep the user's position).
    centerings: u8,
    /// Frame of the last time it was drawn, to tell a new opening.
    last_frame: u64,
    /// Owned by the main window already.
    owned: bool,
}

/// Sets the size of the content of the dialog with the id `id` (its OS
/// window follows; a dialog inside the main window takes it and keeps it).
pub fn resize(ctx: &Context, id: Id, size: Vec2) {
    if ctx.embed_viewports() {
        ctx.data_mut(|d| d.insert_temp(id.with("embedded-size"), size));
        return;
    }
    ctx.send_viewport_cmd_to(
        ViewportId::from_hash_of(id),
        egui::ViewportCommand::InnerSize(size + Vec2::splat(2.0 * MARGIN)),
    );
}

pub struct Window<'open> {
    title: String,
    id: Option<Id>,
    open: Option<&'open mut bool>,
    resizable: bool,
    collapsible: bool,
    default_size: Option<Vec2>,
    min_size: Option<Vec2>,
    default_pos: Option<Pos2>,
    pivot: Option<Align2>,
    anchor: Option<(Align2, Vec2)>,
}

impl<'open> Window<'open> {
    pub fn new(title: impl Into<String>) -> Window<'open> {
        Window {
            title: title.into(),
            id: None,
            open: None,
            resizable: true,
            collapsible: true,
            default_size: None,
            min_size: None,
            default_pos: None,
            pivot: None,
            anchor: None,
        }
    }

    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    /// The close button sets `open` to `false`.
    pub fn open(mut self, open: &'open mut bool) -> Self {
        self.open = Some(open);
        self
    }

    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Only for the embedded fallback (OS windows do not collapse).
    pub fn collapsible(mut self, collapsible: bool) -> Self {
        self.collapsible = collapsible;
        self
    }

    pub fn default_size(mut self, size: impl Into<Vec2>) -> Self {
        self.default_size = Some(size.into());
        self
    }

    pub fn min_size(mut self, size: impl Into<Vec2>) -> Self {
        self.min_size = Some(size.into());
        self
    }

    /// Only for the embedded fallback: OS dialogs open centered on the
    /// main window.
    pub fn default_pos(mut self, pos: impl Into<Pos2>) -> Self {
        self.default_pos = Some(pos.into());
        self
    }

    pub fn pivot(mut self, pivot: Align2) -> Self {
        self.pivot = Some(pivot);
        self
    }

    pub fn anchor(mut self, align: Align2, offset: impl Into<Vec2>) -> Self {
        self.anchor = Some((align, offset.into()));
        self
    }

    pub fn show<R>(self, ctx: &Context, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        if self.open.as_deref() == Some(&false) {
            return None;
        }
        if ctx.embed_viewports() {
            return self.embedded(ctx, add);
        }
        let id = self.id.unwrap_or_else(|| Id::new(&self.title));
        let viewport = ViewportId::from_hash_of(id);
        let state_id = id.with("floating-window");
        let mut st: State = ctx.data(|d| d.get_temp(state_id)).unwrap_or_default();
        let frame = ctx.cumulative_frame_nr();
        if frame > st.last_frame + 1 {
            // Opened (again): a new window, centered and owned anew.
            st.pos = None;
            st.centered_for = None;
            st.centerings = 0;
            st.owned = false;
        }
        st.last_frame = frame;
        let auto = !self.resizable;
        // The OS window must be visible from its first frame (drawing into a
        // hidden one stalls), so auto-sized dialogs open at a guess and take
        // their measured size on the next frame.
        let size = match (auto, st.size) {
            (true, Some(s)) => s,
            (true, None) => vec2(420.0, 300.0),
            (false, _) => self.default_size.unwrap_or(vec2(560.0, 420.0)),
        };
        // Fit on the monitor.
        let size = match ctx.input(|i| i.viewport().monitor_size) {
            Some(m) => size.min(m - vec2(40.0, 80.0)),
            None => size,
        };
        // Centered on the main window when it opens, and once more when the
        // first measure changes the size.
        if st.centered_for != Some(size)
            && st.centerings < 2
            && let Some(main) = ctx.input(|i| i.viewport().outer_rect)
        {
            // Centered, but not above or left of the main window.
            st.pos = Some((main.center() - size / 2.0).max(main.min));
            st.centered_for = Some(size);
            st.centerings += 1;
        }
        let mut builder = ViewportBuilder::default()
            .with_title(self.title.clone())
            .with_inner_size(size)
            .with_resizable(self.resizable)
            .with_minimize_button(false)
            .with_maximize_button(self.resizable)
            .with_close_button(self.open.is_some())
            .with_taskbar(false)
            .with_active(true);
        if !auto {
            let min = self
                .min_size
                .map_or(vec2(240.0, 160.0), |m| m + Vec2::splat(2.0 * MARGIN));
            builder = builder.with_min_inner_size(min);
        }
        if let Some(p) = st.pos {
            builder = builder.with_position(p);
        }
        let mut add = Some(add);
        let mut measured = None;
        let mut close = false;
        let out = ctx.show_viewport_immediate(viewport, builder, |ui, _class| {
            let add = add.take()?;
            let fill = ui.visuals().window_fill;
            let out = if auto {
                // An untitled egui window at the corner sizes itself to its
                // content like before; the OS window follows its size.
                ui.painter().rect_filled(ui.max_rect(), 0.0, fill);
                let w = egui::Window::new(self.title.clone())
                    .id(id.with("content"))
                    .title_bar(false)
                    .movable(false)
                    .resizable(false)
                    .constrain(false)
                    .fixed_pos(pos2(0.0, 0.0))
                    .frame(egui::Frame::new().fill(fill).inner_margin(MARGIN));
                let r = w.show(ui.ctx(), add);
                if let Some(r) = &r {
                    measured = Some(r.response.rect.size());
                }
                r.and_then(|r| r.inner)
            } else {
                Some(
                    egui::Frame::new()
                        .fill(fill)
                        .inner_margin(MARGIN)
                        .show(ui, |ui| {
                            ui.set_min_size(ui.available_size());
                            add(ui)
                        })
                        .inner,
                )
            };
            close = ui.ctx().input(|i| i.viewport().close_requested());
            if blocked(ui.ctx(), id) {
                shield(ui.ctx(), egui::Order::Foreground, true);
                close = false;
            }
            out
        });
        if let Some(m) = measured {
            let m = vec2(m.x.ceil(), m.y.ceil());
            if st.size.is_none_or(|s| (s - m).length() > 0.5) {
                st.size = Some(m);
                ctx.request_repaint();
            }
        }
        if !st.owned {
            // The OS creates the window during the frame: retry until it is
            // there.
            st.owned = bv_platform::set_window_owner(&self.title, &main_title());
            if !st.owned {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
        }
        ctx.data_mut(|d| d.insert_temp(state_id, st));
        if close && let Some(open) = self.open {
            *open = false;
        }
        out
    }

    /// The dialog inside the main window (backends without more windows).
    fn embedded<R>(self, ctx: &Context, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        let mut w = egui::Window::new(self.title.clone())
            .resizable(self.resizable)
            .collapsible(self.collapsible)
            // Clicks go through to the main window's shield.
            .interactable(!blocked(
                ctx,
                self.id.unwrap_or_else(|| Id::new(&self.title)),
            ));
        if let Some(id) = self.id {
            w = w.id(id);
        }
        if let Some(open) = self.open {
            w = w.open(open);
        }
        let fitted: Option<Vec2> = self
            .id
            .and_then(|id| ctx.data(|d| d.get_temp(id.with("embedded-size"))));
        // egui sizes a window with its title bar and frame: what they take
        // (measured last frame) goes on top of the content's size.
        let extra_id = self.id.map(|id| id.with("embedded-extra"));
        let extra: Vec2 = extra_id
            .and_then(|id| ctx.data(|d| d.get_temp(id)))
            .unwrap_or(vec2(16.0, 46.0));
        match fitted {
            // The size it was given (`resize`).
            Some(s) => w = w.fixed_size(s + extra),
            None => {
                if let Some(s) = self.default_size {
                    w = w.default_size(s);
                }
                if let Some(s) = self.min_size {
                    w = w.min_size(s);
                }
            }
        }
        if let Some(p) = self.default_pos {
            w = w.default_pos(p);
        }
        if let Some(p) = self.pivot {
            w = w.pivot(p);
        }
        if let Some((a, o)) = self.anchor {
            w = w.anchor(a, o);
        }
        let mut content = None;
        let r = w.show(ctx, |ui| {
            content = Some(ui.max_rect().min);
            add(ui)
        });
        // The title bar and the frame, from where the content starts (its
        // size may run past the window's): the margin around it is the
        // same on every side.
        if let (Some(r), Some(c), Some(id)) = (&r, content, extra_id) {
            let outer = r.response.rect.min;
            let side = c.x - outer.x;
            let e = vec2(2.0 * side, c.y - outer.y + side);
            if ctx.data(|d| d.get_temp::<Vec2>(id)) != Some(e) {
                ctx.data_mut(|d| d.insert_temp(id, e));
                ctx.request_repaint();
            }
        }
        r.and_then(|r| r.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[derive(Default)]
    struct Clicks {
        modal_up: bool,
        main: u32,
        other: u32,
        modal: u32,
    }

    fn click_each(h: &mut Harness<'_, Clicks>) -> (u32, u32, u32) {
        for label in ["Main", "In other", "In modal"] {
            h.get_by_label(label).click();
            h.run();
        }
        let c = h.state();
        (c.main, c.other, c.modal)
    }

    /// While a dialog is modal only it takes clicks: the main window and
    /// the other dialogs take none, and all of them do again after.
    #[test]
    fn a_modal_dialog_alone_takes_input() {
        let mut h = Harness::builder()
            .with_size(vec2(900.0, 600.0))
            .build_ui_state(
                |ui, c: &mut Clicks| {
                    let ctx = ui.ctx().clone();
                    let modal_ids = [Id::new("modal")];
                    set_modal(&ctx, if c.modal_up { &modal_ids } else { &[] });
                    if ui.button("Main").clicked() {
                        c.main += 1;
                    }
                    Window::new("Other")
                        .id(Id::new("other"))
                        .default_pos(pos2(60.0, 200.0))
                        .default_size(vec2(160.0, 80.0))
                        .show(&ctx, |ui| {
                            if ui.button("In other").clicked() {
                                c.other += 1;
                            }
                        });
                    Window::new("Modal")
                        .id(Id::new("modal"))
                        .default_pos(pos2(500.0, 200.0))
                        .default_size(vec2(160.0, 80.0))
                        .show(&ctx, |ui| {
                            if ui.button("In modal").clicked() {
                                c.modal += 1;
                            }
                        });
                    if modal(&ctx) {
                        shield(&ctx, egui::Order::Background, false);
                    }
                },
                Clicks::default(),
            );
        h.run();
        assert_eq!(click_each(&mut h), (1, 1, 1), "nothing modal");
        h.state_mut().modal_up = true;
        h.run();
        assert_eq!(click_each(&mut h), (1, 1, 2), "only the modal one");
        h.state_mut().modal_up = false;
        h.run();
        assert_eq!(click_each(&mut h), (2, 2, 3), "all of them again");
    }
}
