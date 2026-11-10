//! Tools › Video › Track Motion (and a video track's Track Motion, a
//! compositing parent's Parent Motion), as VEGAS's 2D Track Motion window:
//! where the track's picture goes in the frame — its box's position, size
//! and angle, dragged on the workspace or typed —, its 2D shadow and 2D
//! glow, each a row of the keyframe controller below; edits at the
//! timeline's cursor (keyframes there once it is past the start, as
//! VEGAS's). Its Preset row saves the row being edited at the cursor
//! under a name and sets it back (2D and 3D presets apart,
//! `UiSettings::track_motion_presets`); Edit in Object Space moves the
//! box along its own axes; the TrackMotion keyboard context (the 3D
//! layouts' 1 to 6, the toolbar's toggles) works while the window has
//! the keys.

use bv_core::Tick;
use bv_timeline::{CompositeMode, HaloLook, KeyCurve, Project, TrackId, TrackKind, Transform};
use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::cursors::{self, Cursor};
use crate::egui::{
    self, Align, Color32, CursorIcon, Layout, PointerButton, Pos2, Rect, Sense, Shape, Stroke,
    StrokeKind, Vec2, pos2, vec2,
};
use crate::fx_window::{self as fxw, Glyph, Hit, Space};
use crate::i18n::{tr, trc};
use crate::pancrop::Viewport;
use crate::theme;

/// The keyframe controller's rows: what the window edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Row {
    Position,
    Shadow,
    Glow,
}

#[derive(Clone, Copy, Debug)]
enum Drag {
    Move { vp: Viewport, from: Pos2 },
    Resize { vp: Viewport, sx: f32, sy: f32 },
    Rotate { vp: Viewport, from: f32 },
}

impl Drag {
    /// The pointer while it goes on: the one its press showed (the box
    /// mirrored `flips`), as Pan/Crop's.
    fn cursor(&self, flips: (bool, bool)) -> Cursor {
        match *self {
            Drag::Move { .. } => Cursor::MoveFrame,
            Drag::Resize { vp, sx, sy } => fxw::box_handle_cursor(sx, sy, vp.angle_deg, flips),
            Drag::Rotate { .. } => Cursor::Rotate,
        }
    }
}

/// What the window changes this frame.
#[derive(Clone, Copy, Debug)]
enum Edit {
    Box(Transform),
    Halo(HaloLook),
    HaloOn(bool, bool),
    Mode(CompositeMode),
    Curve(Tick, KeyCurve),
    Smoothness(Tick, f32),
    DeleteKey(Row, Tick),
    Restore,
    /// The 3D box at the cursor.
    Space(bv_timeline::space::Box3d),
    SpaceCurve(Tick, KeyCurve),
    DeleteSpaceKey(Tick),
}

pub(crate) struct TrackMotionWindow {
    pub track: TrackId,
    /// Parent Motion: the track's compositing group moves.
    pub parent: bool,
    pub row: Row,
    rotation: bool,
    snap: bool,
    lock_aspect: bool,
    from_center: bool,
    prevent_move: [bool; 2],
    prevent_scale: [bool; 2],
    /// 3D: Prevent Movement (Z).
    prevent_z: bool,
    /// Edit in Object Space: moves along the box's own axes.
    object_space: bool,
    /// The Preset box's name.
    preset: String,
    /// 3D: the workspace's panes and what each shows.
    layout: PaneLayout,
    views: [View; 4],
    /// The grid's spacing, in the frame's pixels.
    grid: f32,
    sections: [bool; 5],
    drag: Option<Drag>,
    /// The compositing mode is 3D Source Alpha: the box is in space.
    three_d: bool,
    /// The 3D box when a drag began, and the pane and zone dragged.
    drag_3d: Option<bv_timeline::space::Box3d>,
    zone_3d: Option<(usize, Zone)>,
    /// Each drag is one undo step.
    gesture: u64,
}

impl TrackMotionWindow {
    /// A key of the TrackMotion context: a 3D layout, or a toolbar's
    /// toggle.
    fn key(&mut self, k: TrackMotionKey) {
        match k {
            TrackMotionKey::Layout(n) => {
                if let Some(&l) = PaneLayout::ALL.get(usize::from(n.saturating_sub(1))) {
                    self.layout = l;
                }
            }
            TrackMotionKey::Aspect => self.lock_aspect ^= true,
            TrackMotionKey::Center => self.from_center ^= true,
            TrackMotionKey::ScaleX => self.prevent_scale[0] ^= true,
            TrackMotionKey::ScaleY => self.prevent_scale[1] ^= true,
            // The boxes are flat: no depth to scale.
            TrackMotionKey::ScaleZ => {}
            TrackMotionKey::MoveX => self.prevent_move[0] ^= true,
            TrackMotionKey::MoveY => self.prevent_move[1] ^= true,
            TrackMotionKey::MoveZ => self.prevent_z ^= true,
            TrackMotionKey::Rotation => self.rotation ^= true,
            TrackMotionKey::Snap => self.snap ^= true,
            TrackMotionKey::ObjectSpace => self.object_space ^= true,
        }
    }

    /// The 3D workspace's layout, by its place in the list (screen tests).
    pub(crate) fn layout_for_review(&mut self, layout: usize) {
        self.layout = PaneLayout::ALL[layout.min(PaneLayout::ALL.len() - 1)];
    }

    pub(crate) fn new(track: TrackId, parent: bool) -> TrackMotionWindow {
        TrackMotionWindow {
            track,
            parent,
            row: Row::Position,
            rotation: true,
            snap: false,
            lock_aspect: true,
            from_center: true,
            prevent_move: [false; 2],
            prevent_scale: [false; 2],
            prevent_z: false,
            object_space: false,
            preset: String::new(),
            layout: PaneLayout::Single,
            views: [View::CameraFront, View::Top, View::Left, View::Perspective],
            grid: 32.0,
            sections: [true, true, false, true, true],
            drag: None,
            three_d: false,
            drag_3d: None,
            zone_3d: None,
            gesture: 0,
        }
    }
}

/// The box (where the picture goes) in the frame's pixels, y down.
pub(crate) fn viewport(tf: &Transform, out: (f32, f32)) -> Viewport {
    Viewport {
        cx: out.0 / 2.0 + tf.x * out.0,
        cy: out.1 / 2.0 - tf.y * out.1,
        w: tf.scale * out.0,
        h: tf.scale * tf.stretch * out.1,
        angle_deg: tf.rotation_deg,
    }
}

/// The framing that puts the picture in box `vp` (flips from `base`).
pub(crate) fn transform(vp: &Viewport, base: &Transform, out: (f32, f32)) -> Transform {
    let scale = (vp.w / out.0).max(1e-3);
    Transform {
        x: (vp.cx - out.0 / 2.0) / out.0,
        y: -(vp.cy - out.1 / 2.0) / out.1,
        scale,
        stretch: (vp.h / out.1).max(1e-3) / scale,
        rotation_deg: fxw::wrap180(vp.angle_deg),
        ..*base
    }
}

/// A 2D shadow's or glow's box: the picture's, moved and scaled.
fn halo_viewport(b: &Viewport, look: &HaloLook, out: (f32, f32)) -> Viewport {
    Viewport {
        cx: b.cx + look.x * out.0,
        cy: b.cy - look.y * out.1,
        w: b.w * look.size,
        h: b.h * look.size,
        angle_deg: b.angle_deg,
    }
}

impl BananaApp {
    /// Tools › Video › Track Motion, a track's Track Motion and Parent
    /// Motion: the window for `track`.
    pub(crate) fn open_track_motion(&mut self, track: TrackId, parent: bool) {
        let ok = self
            .project()
            .track(track)
            .is_some_and(|t| t.kind == TrackKind::Video && !t.adjustment);
        if ok {
            self.track_motion = Some(TrackMotionWindow::new(track, parent));
        }
    }

    pub(crate) fn track_motion_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.track_motion.take() else {
            return;
        };
        if w.show(ctx, self) && self.track_motion.is_none() {
            self.track_motion = Some(w);
        }
    }
}

impl TrackMotionWindow {
    /// Shows the window; whether it stays open.
    fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let p = app.project().clone();
        let Some(track) = p.track(self.track) else {
            return false;
        };
        let s = p.settings;
        let out = (s.width as f32, s.height as f32);
        let t = s.frame_rate.snap_floor(app.cursor).max(Tick::ZERO);
        let motion = p
            .track_motion(self.track, self.parent)
            .cloned()
            .unwrap_or_default();
        let tf = motion.transform_at(t);
        let name = crate::i18n::track_name(track);
        let title = if self.parent {
            trf!("Parent Motion: {}", name)
        } else {
            trf!("Track Motion: {}", name)
        };
        let mode = if self.parent {
            track.parent_mode
        } else {
            track.composite_mode
        };
        self.three_d = mode.is_three_d();
        // What a preset keeps: the row being edited, at the cursor.
        let current = match (self.row, self.three_d) {
            (Row::Position, true) => PresetValue::Box3d(motion.space_at(t)),
            (Row::Position, false) => PresetValue::Box(tf),
            (Row::Shadow, _) => PresetValue::Shadow(motion.shadow.at(t)),
            (Row::Glow, _) => PresetValue::Glow(motion.glow.at(t)),
        };
        let mut open = true;
        let mut edits: Vec<Edit> = Vec::new();
        let mut seek: Option<Tick> = None;
        crate::floating::Window::new(title)
            .id(egui::Id::new("track-motion-window"))
            .collapsible(false)
            .resizable(true)
            .default_size([940.0, 640.0])
            .open(&mut open)
            .show(ctx, |ui| {
                // The TrackMotion keyboard context, while the window (its
                // own OS window) has the keys.
                if !ui.ctx().embed_viewports()
                    && ui.input(|i| i.focused)
                    && !ui.ctx().egui_wants_keyboard_input()
                {
                    let before = app.key_focus;
                    app.key_focus = crate::keymap::Context::TrackMotion;
                    crate::keymap::dispatch(app, ui.ctx());
                    app.key_focus = before;
                }
                for k in std::mem::take(&mut app.track_motion_keys) {
                    self.key(k);
                }
                self.presets_row(ui, app, &current, &mut edits);
                self.toolbar(ui, mode, &mut edits);
                ui.add_space(4.0);
                let strip_h = 3.0 * ROW_H + RULER_H + 8.0;
                let body_h = (ui.available_height() - strip_h - 8.0).max(260.0);
                ui.horizontal(|ui| {
                    ui.set_height(body_h);
                    ui.allocate_ui_with_layout(
                        vec2(290.0, body_h),
                        Layout::top_down(Align::LEFT),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("track-motion-props")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    if self.three_d && self.row == Row::Position {
                                        self.properties_3d(ui, &motion, t, out, &mut edits);
                                    } else {
                                        self.properties(ui, &motion, &tf, t, out, &mut edits);
                                    }
                                });
                        },
                    );
                    let size = vec2(ui.available_width(), body_h);
                    if self.three_d {
                        self.workspace_3d(ui, &motion, t, out, size, &mut edits);
                    } else {
                        self.workspace(ui, &motion, &tf, t, out, size, &mut edits);
                    }
                });
                ui.add_space(4.0);
                self.keyframes(ui, &p, &motion, t, &mut edits, &mut seek);
            });
        if let Some(at) = seek {
            app.seek(at);
        }
        if !edits.is_empty() {
            self.apply(app, &edits, t);
        }
        open
    }

    fn apply(&mut self, app: &mut BananaApp, edits: &[Edit], t: Tick) {
        let (track, parent, row) = (self.track, self.parent, self.row);
        let new = app.settings.prefs.new_track_motion_keys();
        let key = egui::Id::new(("track-motion", track, parent, self.gesture));
        app.modify_merged(key, |p| {
            for e in edits {
                let _ = edit(p, track, parent, row, t, *e, new);
            }
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, mode: CompositeMode, edits: &mut Vec<Edit>) {
        ui.horizontal(|ui| {
            fxw::toggle(ui, &mut self.rotation, Glyph::Rotate, tr("Enable Rotation"));
            fxw::toggle(
                ui,
                &mut self.snap,
                Glyph::Icon(crate::icons::Icon::Grid),
                tr("Enable Snapping to Grid"),
            );
            fxw::toggle(
                ui,
                &mut self.object_space,
                Glyph::Axes,
                tr("Edit in Object Space"),
            );
            ui.add_space(6.0);
            fxw::toggle(
                ui,
                &mut self.prevent_move[0],
                Glyph::NoMoveX,
                tr("Prevent Movement (X)"),
            );
            fxw::toggle(
                ui,
                &mut self.prevent_move[1],
                Glyph::NoMoveY,
                tr("Prevent Movement (Y)"),
            );
            if self.three_d {
                fxw::toggle(
                    ui,
                    &mut self.prevent_z,
                    Glyph::NoMoveZ,
                    tr("Prevent Movement (Z)"),
                );
            }
            ui.add_space(6.0);
            fxw::toggle(
                ui,
                &mut self.lock_aspect,
                Glyph::Icon(crate::icons::Icon::Lock),
                tr("Lock Aspect Ratio"),
            );
            fxw::toggle(
                ui,
                &mut self.from_center,
                Glyph::FromCenter,
                tr("Scale About Center"),
            );
            fxw::toggle(
                ui,
                &mut self.prevent_scale[0],
                Glyph::NoScaleX,
                tr("Prevent Scaling (X)"),
            );
            fxw::toggle(
                ui,
                &mut self.prevent_scale[1],
                Glyph::NoScaleY,
                tr("Prevent Scaling (Y)"),
            );
            if self.three_d {
                // BananaV's boxes are flat: no depth to scale yet.
                fxw::glyph_button(ui, Glyph::NoScaleZ, false, false, tr("Prevent Scaling (Z)"));
            }
            ui.add_space(14.0);
            ui.label(if self.parent {
                tr("Parent Composite Mode:")
            } else {
                tr("Composite Mode:")
            });
            let mut m = mode;
            egui::ComboBox::from_id_salt("track-motion-mode")
                .selected_text(tr(m.name()))
                .width(170.0)
                .show_ui(ui, |ui| {
                    for c in CompositeMode::MENU {
                        ui.selectable_value(&mut m, c, tr(c.name()));
                    }
                });
            if m != mode {
                edits.push(Edit::Mode(m));
            }
            if self.three_d {
                egui::ComboBox::from_id_salt("track-motion-layout")
                    .selected_text(trf!("Layout ({})", self.layout.name()))
                    .width(190.0)
                    .show_ui(ui, |ui| {
                        for l in PaneLayout::ALL {
                            ui.selectable_value(&mut self.layout, l, l.name());
                        }
                    });
            }
        });
    }

    fn properties(
        &mut self,
        ui: &mut egui::Ui,
        motion: &bv_timeline::TrackMotion,
        tf: &Transform,
        t: Tick,
        out: (f32, f32),
        edits: &mut Vec<Edit>,
    ) {
        match self.row {
            Row::Position => {
                let vp0 = viewport(tf, out);
                let mut vp = vp0;
                // VEGAS's numbers: the center from the frame's (y up),
                // the box's size, its angle counterclockwise.
                let (mut x, mut y) = (vp.cx - out.0 / 2.0, out.1 / 2.0 - vp.cy);
                let mut angle = -vp.angle_deg;
                let lock = self.lock_aspect;
                fxw::section(ui, &mut self.sections[0], tr("Position"), |ui| {
                    fxw::prop_grid(ui, "tm-position", |ui| {
                        fxw::prop(ui, "X", crate::fmt::drag(&mut x, 1, "").speed(1.0));
                        fxw::prop(ui, "Y", crate::fmt::drag(&mut y, 1, "").speed(1.0));
                        let (w0, h0) = (vp.w, vp.h);
                        fxw::prop(
                            ui,
                            tr("Width"),
                            crate::fmt::drag(&mut vp.w, 1, "")
                                .speed(1.0)
                                .range(1.0..=out.0 * 20.0),
                        );
                        fxw::prop(
                            ui,
                            tr("Height"),
                            crate::fmt::drag(&mut vp.h, 1, "")
                                .speed(1.0)
                                .range(1.0..=out.1 * 20.0),
                        );
                        if lock {
                            if vp.w != w0 {
                                vp.h = vp.w * h0 / w0;
                            } else if vp.h != h0 {
                                vp.w = vp.h * w0 / h0;
                            }
                        }
                    });
                });
                fxw::section(ui, &mut self.sections[1], tr("Orientation"), |ui| {
                    fxw::prop_grid(ui, "tm-orientation", |ui| {
                        fxw::prop(
                            ui,
                            tr("Angle"),
                            crate::fmt::drag(&mut angle, 1, "°")
                                .speed(0.5)
                                .range(-180.0..=180.0),
                        );
                    });
                });
                vp.cx = out.0 / 2.0 + x;
                vp.cy = out.1 / 2.0 - y;
                vp.angle_deg = -angle;
                if vp != vp0 {
                    edits.push(Edit::Box(transform(&vp, tf, out)));
                }
                let keys = &motion.keys;
                let at = keys.iter().rposition(|k| k.at <= t);
                fxw::section(
                    ui,
                    &mut self.sections[2],
                    tr("Keyframe Interpolation"),
                    |ui| match at {
                        Some(i) => fxw::prop_grid(ui, "tm-interpolation", |ui| {
                            ui.label(tr("Curve to next"));
                            let mut cv = keys[i].curve;
                            egui::ComboBox::from_id_salt("tm-curve")
                                .selected_text(tr(cv.name()))
                                .show_ui(ui, |ui| {
                                    for c in KeyCurve::ALL {
                                        ui.selectable_value(&mut cv, c, tr(c.name()));
                                    }
                                });
                            ui.end_row();
                            if cv != keys[i].curve {
                                edits.push(Edit::Curve(keys[i].at, cv));
                            }
                            let mut smooth = keys[i].smoothness;
                            let r = fxw::prop(
                                ui,
                                tr("Smoothness"),
                                crate::fmt::drag(&mut smooth, 1, "")
                                    .speed(0.5)
                                    .range(0.0..=100.0),
                            );
                            if r.changed() {
                                edits.push(Edit::Smoothness(keys[i].at, smooth));
                            }
                        }),
                        None => crate::tabs::hint(
                            ui,
                            tr(
                                "No animation: move the cursor and change the box to add keyframes.",
                            ),
                        ),
                    },
                );
            }
            Row::Shadow | Row::Glow => {
                let shadow = self.row == Row::Shadow;
                let halo = if shadow { &motion.shadow } else { &motion.glow };
                let mut on = halo.on;
                let label = if shadow {
                    tr("2D Shadow")
                } else {
                    tr("2D Glow")
                };
                if crate::widgets::checkbox(ui, &mut on, label).changed() {
                    edits.push(Edit::HaloOn(shadow, on));
                }
                let look0 = halo.at(t);
                let mut look = look0;
                fxw::section(ui, &mut self.sections[3], label, |ui| {
                    fxw::prop_grid(ui, "tm-halo", |ui| {
                        let mut blur = look.blur * 100.0;
                        fxw::prop(
                            ui,
                            tr("Blur %"),
                            crate::fmt::drag(&mut blur, 1, "")
                                .speed(0.5)
                                .range(0.0..=100.0),
                        );
                        look.blur = blur / 100.0;
                        let mut k = look.intensity * 100.0;
                        fxw::prop(
                            ui,
                            tr("Intensity"),
                            crate::fmt::drag(&mut k, 1, "")
                                .speed(0.5)
                                .range(0.0..=100.0),
                        );
                        look.intensity = k / 100.0;
                        ui.label(tr("Color"));
                        ui.color_edit_button_rgba_unmultiplied(&mut look.color);
                        ui.end_row();
                        let (mut x, mut y) = (look.x * out.0, look.y * out.1);
                        fxw::prop(ui, "X", crate::fmt::drag(&mut x, 1, "").speed(1.0));
                        fxw::prop(ui, "Y", crate::fmt::drag(&mut y, 1, "").speed(1.0));
                        look.x = x / out.0;
                        look.y = y / out.1;
                        let mut size = look.size * 100.0;
                        fxw::prop(
                            ui,
                            tr("Size"),
                            crate::fmt::drag(&mut size, 1, " %")
                                .speed(0.5)
                                .range(1.0..=1000.0),
                        );
                        look.size = size / 100.0;
                    });
                });
                if look != look0 {
                    edits.push(Edit::Halo(look));
                    if !on {
                        edits.push(Edit::HaloOn(shadow, true));
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn workspace(
        &mut self,
        ui: &mut egui::Ui,
        motion: &bv_timeline::TrackMotion,
        tf: &Transform,
        t: Tick,
        out: (f32, f32),
        size: egui::Vec2,
        edits: &mut Vec<Edit>,
    ) {
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let k = (rect.width() * 0.7 / out.0).min(rect.height() * 0.7 / out.1);
        let sp = Space {
            center: rect.center(),
            k,
            src: out,
        };
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let shift = ui.input(|i| i.modifiers.shift);
        let picture = viewport(tf, out);
        let looks = [
            (Row::Shadow, motion.shadow.on, motion.shadow.at(t)),
            (Row::Glow, motion.glow.on, motion.glow.at(t)),
        ];
        let halo_look = looks.iter().find(|l| l.0 == self.row).map(|l| l.2);
        let vp0 = match halo_look {
            Some(look) => halo_viewport(&picture, &look, out),
            None => picture,
        };
        // Halos neither turn nor change shape: they follow the picture.
        let rotation = self.rotation && halo_look.is_none();
        let lock = self.lock_aspect || halo_look.is_some();
        let mut vp = vp0;
        if resp.drag_started_by(PointerButton::Primary)
            && let Some(o) = ui.input(|i| i.pointer.press_origin())
        {
            self.gesture += 1;
            self.drag = match fxw::hit(&sp, &vp, o, rotation) {
                Hit::Handle(sx, sy) => Some(Drag::Resize { vp, sx, sy }),
                Hit::Rotate => Some(Drag::Rotate {
                    vp,
                    from: fxw::angle_of(&sp, &vp, o),
                }),
                Hit::Inside => Some(Drag::Move { vp, from: o }),
                Hit::Outside => None,
            };
        }
        if resp.dragged()
            && let (Some(d), Some(p)) = (self.drag, pointer)
        {
            let grid = self.grid.max(1.0);
            match d {
                Drag::Move { vp: v0, from } => {
                    let (mut dx, mut dy) = ((p.x - from.x) / sp.k, (p.y - from.y) / sp.k);
                    if shift {
                        if dx.abs() > dy.abs() {
                            dy = 0.0;
                        } else {
                            dx = 0.0;
                        }
                    }
                    // Edit in Object Space: the axes held are the box's.
                    let angle = if self.object_space { v0.angle_deg } else { 0.0 };
                    let (dx, dy) = held_move((dx, dy), angle, self.prevent_move);
                    vp.cx = v0.cx + dx;
                    vp.cy = v0.cy + dy;
                    if self.snap {
                        vp.cx = (vp.cx / grid).round() * grid;
                        vp.cy = (vp.cy / grid).round() * grid;
                    }
                }
                Drag::Resize { vp: v0, sx, sy } => {
                    let l = fxw::local(&sp, &v0, p);
                    let sx = if self.prevent_scale[0] { 0.0 } else { sx };
                    let sy = if self.prevent_scale[1] { 0.0 } else { sy };
                    if sx != 0.0 || sy != 0.0 {
                        vp = fxw::resize(&v0, l, sx, sy, lock, self.from_center);
                        if self.snap {
                            let step = (grid, grid);
                            vp = fxw::snap_resize(&v0, &vp, sx, sy, lock, self.from_center, step);
                        }
                    }
                }
                Drag::Rotate { vp: v0, from } => {
                    let mut a = v0.angle_deg + (fxw::angle_of(&sp, &v0, p) - from).to_degrees();
                    if shift {
                        a = (a / 15.0).round() * 15.0;
                    }
                    vp.angle_deg = fxw::wrap180(a);
                }
            }
        }
        if resp.drag_stopped() {
            self.drag = None;
        }
        // The pointer, as Pan/Crop's (the same box, VEGAS's choices): a
        // drag's own while it goes on, else the zone's under it; far from
        // the box the system's arrow (this view does not move).
        let flips = if halo_look.is_some() {
            (false, false)
        } else {
            (tf.flip_h, tf.flip_v)
        };
        let pointer = match &self.drag {
            Some(d) => Some(d.cursor(flips)),
            None => resp.hover_pos().and_then(|p| {
                fxw::box_cursor(fxw::hit(&sp, &vp, p, rotation), false, vp.angle_deg, flips)
            }),
        };
        match pointer {
            Some(c) => cursors::set(ui.ctx(), c),
            None if resp.hovered() => cursors::system(ui.ctx(), CursorIcon::Default),
            None => {}
        }
        // The workspace's menu: restore, center, flip, aspect.
        let mut action: Option<&str> = None;
        resp.context_menu(|ui| {
            for (key, label) in [
                ("restore", tr("Restore")),
                ("center", trc("menu", "Center")),
                ("fliph", tr("Flip Horizontal")),
                ("flipv", tr("Flip Vertical")),
                ("output", tr("Match Output Aspect")),
                ("square", tr("Square Aspect")),
            ] {
                if ui.button(label).clicked() {
                    action = Some(key);
                    ui.close();
                }
            }
        });
        match (action, halo_look) {
            (Some("restore"), _) => edits.push(Edit::Restore),
            (Some("center"), Some(look)) => edits.push(Edit::Halo(HaloLook {
                x: 0.0,
                y: 0.0,
                ..look
            })),
            (Some("center"), None) => {
                vp.cx = out.0 / 2.0;
                vp.cy = out.1 / 2.0;
            }
            (Some("fliph"), None) => edits.push(Edit::Box(Transform {
                flip_h: !tf.flip_h,
                ..*tf
            })),
            (Some("flipv"), None) => edits.push(Edit::Box(Transform {
                flip_v: !tf.flip_v,
                ..*tf
            })),
            (Some("output"), None) => vp.h = vp.w * out.1 / out.0,
            (Some("square"), None) => vp.h = vp.w,
            _ => {}
        }
        if vp != vp0 {
            match halo_look {
                Some(look) => edits.push(Edit::Halo(HaloLook {
                    x: (vp.cx - picture.cx) / out.0,
                    y: -(vp.cy - picture.cy) / out.1,
                    size: (vp.w / picture.w.max(1e-3)).max(0.01),
                    ..look
                })),
                None => edits.push(Edit::Box(transform(&vp, tf, out))),
            }
        }

        // The workspace: the frame, the boxes, the grid.
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, fxw::WS_BG);
        let frame = Rect::from_min_max(sp.screen((0.0, 0.0)), sp.screen(out));
        painter.rect_filled(frame, 0.0, Color32::from_gray(150));
        painter.rect_stroke(
            frame,
            0.0,
            Stroke::new(1.0, Color32::from_gray(90)),
            StrokeKind::Outside,
        );
        if self.snap {
            let g = self.grid * sp.k;
            if g >= 6.0 {
                let stroke = Stroke::new(1.0, Color32::from_black_alpha(36));
                let mut x = frame.left();
                while x <= frame.right() {
                    painter.vline(x, frame.y_range(), stroke);
                    x += g;
                }
                let mut y = frame.top();
                while y <= frame.bottom() {
                    painter.hline(frame.x_range(), y, stroke);
                    y += g;
                }
            }
        }
        // The shadow and glow, faint, under the picture's box.
        let picture = match halo_look {
            None => vp,
            Some(_) => picture,
        };
        for (row, on, look) in looks {
            if !on && row != self.row {
                continue;
            }
            let hv = if row == self.row {
                vp
            } else {
                halo_viewport(&picture, &look, out)
            };
            let corners: Vec<Pos2> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .into_iter()
                .map(|(sx, sy)| fxw::box_point(&sp, &hv, sx, sy))
                .collect();
            let c = look.color;
            let fill = Color32::from_rgba_unmultiplied(
                (c[0] * 255.0) as u8,
                (c[1] * 255.0) as u8,
                (c[2] * 255.0) as u8,
                70,
            );
            painter.add(Shape::convex_polygon(corners, fill, Stroke::NONE));
        }
        let active = if halo_look.is_some() { picture } else { vp };
        fxw::paint_f_box(
            &painter,
            &sp,
            &active,
            (tf.flip_h, tf.flip_v),
            (active.cx, active.cy),
            rotation,
        );
        if halo_look.is_some() {
            // The halo's own box, dashed, its handles.
            fxw::paint_f_box(&painter, &sp, &vp, (false, false), (vp.cx, vp.cy), false);
        }
    }

    fn keyframes(
        &mut self,
        ui: &mut egui::Ui,
        p: &Project,
        motion: &bv_timeline::TrackMotion,
        t: Tick,
        edits: &mut Vec<Edit>,
        seek: &mut Option<Tick>,
    ) {
        let three_d = self.three_d;
        let length = p.duration().max(Tick::from_seconds(10));
        let (rect, _) = ui.allocate_exact_size(
            vec2(ui.available_width(), RULER_H + 3.0 * ROW_H),
            Sense::hover(),
        );
        let label_w = 110.0;
        let strip = Rect::from_min_max(pos2(rect.left() + label_w, rect.top()), rect.max);
        let x_of = |at: Tick| {
            strip.left() + (at.0 as f64 / length.0 as f64) as f32 * (strip.width() - 8.0) + 4.0
        };
        let at_of = |x: f32| {
            let f = ((x - strip.left() - 4.0) / (strip.width() - 8.0)).clamp(0.0, 1.0) as f64;
            p.settings
                .frame_rate
                .snap_nearest(Tick((f * length.0 as f64) as i64))
        };
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::panel_dark());
        let ruler = Rect::from_min_max(strip.min, pos2(strip.right(), strip.top() + RULER_H));
        painter.rect_filled(ruler, 0.0, theme::bg());
        let rows = [
            (Row::Position, tr("Position"), None),
            (Row::Shadow, tr("2D Shadow"), Some(motion.shadow.on)),
            (Row::Glow, tr("2D Glow"), Some(motion.glow.on)),
        ];
        let keys: [Vec<(Tick, KeyCurve)>; 3] = [
            if three_d {
                motion.space_keys.iter().map(|k| (k.at, k.curve)).collect()
            } else {
                motion.keys.iter().map(|k| (k.at, k.curve)).collect()
            },
            motion.shadow.keys.iter().map(|k| (k.at, k.curve)).collect(),
            motion.glow.keys.iter().map(|k| (k.at, k.curve)).collect(),
        ];
        for (i, (row, label, on)) in rows.into_iter().enumerate() {
            let top = ruler.bottom() + i as f32 * ROW_H;
            let lane =
                Rect::from_min_max(pos2(strip.left(), top), pos2(strip.right(), top + ROW_H));
            let head = Rect::from_min_max(pos2(rect.left(), top), pos2(strip.left(), top + ROW_H));
            let fill = if row == self.row {
                theme::track_video()
            } else {
                theme::bg()
            };
            painter.rect_filled(lane.shrink2(vec2(0.0, 1.0)), 0.0, fill);
            // The row's name (a halo's box checks it), a click picks it.
            let r = ui.interact(head, ui.id().with(("tm-row", i)), Sense::click());
            let mut x = head.left() + 6.0;
            if let Some(mut on_now) = on {
                let b = Rect::from_min_size(pos2(x, head.center().y - 6.0), vec2(12.0, 12.0));
                let cb = ui.interact(b, ui.id().with(("tm-on", i)), Sense::click());
                crate::widgets::paint_check_box(ui, b, on_now, false, true);
                if cb.clicked() {
                    on_now = !on_now;
                    edits.push(Edit::HaloOn(row == Row::Shadow, on_now));
                }
                x += 16.0;
            }
            painter.text(
                pos2(x, head.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(11.5),
                if row == self.row {
                    theme::text()
                } else {
                    theme::text_dim()
                },
            );
            if r.clicked() {
                self.row = row;
            }
            let lr = ui.interact(lane, ui.id().with(("tm-lane", i)), Sense::click());
            let mut on_key = None;
            for &(at, curve) in &keys[i] {
                let c = pos2(x_of(at), lane.center().y);
                fxw::paint_key(&painter, c, curve, theme::banana());
                if lr.hover_pos().is_some_and(|h| (h - c).length() <= 6.0) {
                    on_key = Some(at);
                }
            }
            // A keyframe to click, as VEGAS's keyframe rows: the hand.
            if on_key.is_some() {
                cursors::set(ui.ctx(), Cursor::HandPoint);
            }
            if lr.clicked()
                && let Some(h) = lr.interact_pointer_pos()
            {
                self.row = row;
                *seek = Some(on_key.unwrap_or_else(|| at_of(h.x)));
            }
            if let Some(at) = on_key {
                lr.context_menu(|ui| {
                    if ui.button(tr("Delete Keyframe")).clicked() {
                        edits.push(if three_d && row == Row::Position {
                            Edit::DeleteSpaceKey(at)
                        } else {
                            Edit::DeleteKey(row, at)
                        });
                        ui.close();
                    }
                });
            }
        }
        // The cursor.
        let cx = x_of(t);
        painter.vline(
            cx,
            ruler.top()..=rect.bottom(),
            Stroke::new(1.0, theme::cursor()),
        );
        let rr = ui.interact(ruler, ui.id().with("tm-ruler"), Sense::click_and_drag());
        if (rr.clicked() || rr.dragged())
            && let Some(h) = rr.interact_pointer_pos()
        {
            *seek = Some(at_of(h.x));
        }
    }
}

/// The keyframe controller's ruler and rows.
const RULER_H: f32 = 16.0;
const ROW_H: f32 = 20.0;

/// One of the window's changes on the project.
fn edit(
    p: &mut Project,
    track: TrackId,
    parent: bool,
    row: Row,
    t: Tick,
    e: Edit,
    new: (KeyCurve, f32),
) -> Result<(), bv_timeline::EditError> {
    match e {
        Edit::Box(tf) => p.set_track_motion_at(track, parent, t, tf, new),
        Edit::Space(b) => {
            p.track_motion_mut(track, parent)?.set_space_at(t, b, new.0);
            Ok(())
        }
        Edit::SpaceCurve(at, c) => {
            let m = p.track_motion_mut(track, parent)?;
            if let Some(k) = m.space_keys.iter_mut().find(|k| k.at == at) {
                k.curve = c;
            }
            Ok(())
        }
        Edit::DeleteSpaceKey(at) => {
            p.track_motion_mut(track, parent)?.remove_space_key(at);
            Ok(())
        }
        Edit::Halo(look) => p.set_track_halo_at(track, parent, row == Row::Shadow, t, look),
        Edit::HaloOn(shadow, on) => p.set_track_halo_on(track, parent, shadow, on),
        Edit::Mode(m) => {
            let tr = p
                .track_by_id_mut(track)
                .ok_or(bv_timeline::EditError::UnknownTrack(track))?;
            if parent {
                tr.parent_mode = m;
            } else {
                tr.composite_mode = m;
            }
            Ok(())
        }
        Edit::Curve(at, c) => {
            let m = p.track_motion_mut(track, parent)?;
            if let Some(k) = m.keys.iter_mut().find(|k| k.at == at) {
                k.curve = c;
            }
            Ok(())
        }
        Edit::Smoothness(at, v) => {
            let m = p.track_motion_mut(track, parent)?;
            if let Some(k) = m.keys.iter_mut().find(|k| k.at == at) {
                k.smoothness = v;
            }
            Ok(())
        }
        Edit::DeleteKey(r, at) => {
            let m = p.track_motion_mut(track, parent)?;
            match r {
                Row::Position => m.remove_key(at),
                Row::Shadow => m.shadow.remove_key(at),
                Row::Glow => m.glow.remove_key(at),
            }
            Ok(())
        }
        Edit::Restore => match row {
            Row::Position => p.set_track_motion_at(track, parent, t, Transform::default(), new),
            Row::Shadow => p.set_track_halo_at(track, parent, true, t, HaloLook::SHADOW),
            Row::Glow => p.set_track_halo_at(track, parent, false, t, HaloLook::GLOW),
        },
    }
}

/// 3D Track Motion (the 3D Source Alpha compositing mode's): the box in
/// space — its position, size, orientation and rotation typed, or dragged
/// on the workspace, whose Layout (VEGAS's "Layout (Single Pane)") splits
/// it into panes, each showing the box from a view its menu picks: in
/// Camera Front the trackball VEGAS's Help draws (the box's corners bring
/// it nearer or farther, inside moves it — with Ctrl nearer or farther —,
/// the circle's top and bottom turn it about x, its sides about y, the
/// rest of its rim about z, the guide in bold), in the flat views the box
/// moved along their axes — and its keyframes' curves.
impl TrackMotionWindow {
    fn properties_3d(
        &mut self,
        ui: &mut egui::Ui,
        motion: &bv_timeline::TrackMotion,
        t: Tick,
        out: (f32, f32),
        edits: &mut Vec<Edit>,
    ) {
        let b0 = motion.space_at(t);
        let mut b = b0;
        let lock = self.lock_aspect;
        fxw::section(ui, &mut self.sections[0], tr("Position"), |ui| {
            fxw::prop_grid(ui, "tm-position-3d", |ui| {
                let (mut x, mut y, mut z) = (b.x * out.0, b.y * out.1, b.z * out.0);
                fxw::prop(ui, "X", crate::fmt::drag(&mut x, 1, "").speed(1.0));
                fxw::prop(ui, "Y", crate::fmt::drag(&mut y, 1, "").speed(1.0));
                fxw::prop(ui, "Z", crate::fmt::drag(&mut z, 1, "").speed(1.0));
                let (mut w, mut h) = (b.width * out.0, b.height * out.1);
                let (w0, h0) = (w, h);
                fxw::prop(
                    ui,
                    tr("Width"),
                    crate::fmt::drag(&mut w, 1, "")
                        .speed(1.0)
                        .range(1.0..=out.0 * 20.0),
                );
                fxw::prop(
                    ui,
                    tr("Height"),
                    crate::fmt::drag(&mut h, 1, "")
                        .speed(1.0)
                        .range(1.0..=out.1 * 20.0),
                );
                if lock {
                    if w != w0 {
                        h = w * h0 / w0;
                    } else if h != h0 {
                        w = h * w0 / h0;
                    }
                }
                b.x = x / out.0;
                b.y = y / out.1;
                b.z = z / out.0;
                b.width = w / out.0;
                b.height = h / out.1;
            });
        });
        let sections = [
            (1, tr("Orientation"), "tm-orientation-3d"),
            (4, tr("Rotation"), "tm-rotation-3d"),
        ];
        for (i, title, id) in sections {
            let angles = if i == 1 {
                &mut b.orientation
            } else {
                &mut b.rotation
            };
            fxw::section(ui, &mut self.sections[i], title, |ui| {
                fxw::prop_grid(ui, id, |ui| {
                    for (k, axis) in ["X", "Y", "Z"].into_iter().enumerate() {
                        fxw::prop(
                            ui,
                            axis,
                            crate::fmt::drag(&mut angles[k], 1, "°")
                                .speed(0.5)
                                .range(-360.0..=360.0),
                        );
                    }
                });
            });
        }
        if b != b0 {
            edits.push(Edit::Space(b));
        }
        let keys = &motion.space_keys;
        let at = keys.iter().rposition(|k| k.at <= t);
        fxw::section(
            ui,
            &mut self.sections[2],
            tr("Keyframe Interpolation"),
            |ui| match at {
                Some(i) => fxw::prop_grid(ui, "tm-interpolation-3d", |ui| {
                    ui.label(tr("Curve to next"));
                    let mut cv = keys[i].curve;
                    egui::ComboBox::from_id_salt("tm-curve-3d")
                        .selected_text(tr(cv.name()))
                        .show_ui(ui, |ui| {
                            for c in KeyCurve::ALL {
                                ui.selectable_value(&mut cv, c, tr(c.name()));
                            }
                        });
                    ui.end_row();
                    if cv != keys[i].curve {
                        edits.push(Edit::SpaceCurve(keys[i].at, cv));
                    }
                }),
                None => crate::tabs::hint(
                    ui,
                    tr("No animation: move the cursor and change the box to add keyframes."),
                ),
            },
        );
    }

    /// The 3D workspace: its panes, as the Layout says, each showing the
    /// box from its view; edits in the Camera Front panes (the trackball's
    /// zones) and the flat views (the box moved along their two axes).
    #[allow(clippy::too_many_arguments)]
    fn workspace_3d(
        &mut self,
        ui: &mut egui::Ui,
        motion: &bv_timeline::TrackMotion,
        t: Tick,
        out: (f32, f32),
        size: egui::Vec2,
        edits: &mut Vec<Edit>,
    ) {
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let b0 = motion.space_at(t);
        let mut b = b0;
        for (i, pane) in self.layout.panes(rect).into_iter().enumerate() {
            let view = self.views[i];
            let resp = ui.interact(
                pane,
                ui.id().with(("track-motion-pane", i)),
                Sense::click_and_drag(),
            );
            let k = view.scale(pane, out);
            let screen = |p: [f32; 3]| {
                view.show(p, out.0)
                    .map(|[x, y]| pane.center() + vec2(x * k, y * k))
            };
            let corners: Option<Vec<Pos2>> = b0
                .plane(out.0, out.1)
                .corners()
                .iter()
                .map(|&c| screen(c))
                .collect();
            let center = screen(b0.plane(out.0, out.1).center);
            // Where the pointer is: the zone it would drag.
            let ball = match (&corners, center) {
                (Some(c), Some(center)) if view == View::CameraFront => {
                    Some(Trackball::around(center, c))
                }
                _ => None,
            };
            // The camera's other views only show.
            let editable = view == View::CameraFront || view.flat();
            let hover =
                resp.hover_pos()
                    .filter(|_| editable)
                    .and_then(|p| match (&ball, &corners) {
                        (Some(ball), Some(c)) => ball.zone(p, c, self.rotation),
                        (None, Some(c)) => near_segments(p, c).then_some(Zone::Move),
                        _ => None,
                    });
            if resp.drag_started_by(PointerButton::Primary) && hover.is_some() {
                self.gesture += 1;
                self.drag_3d = Some(b0);
                self.zone_3d = hover.map(|z| (i, z));
            }
            let active = self
                .zone_3d
                .filter(|(pane_at, _)| *pane_at == i)
                .map(|z| z.1);
            if resp.dragged()
                && let (Some(start), Some(zone)) = (self.drag_3d, active)
                && let Some(o) = ui.input(|i| i.pointer.press_origin())
                && let Some(p) = resp.interact_pointer_pos()
            {
                let ctrl = ui.input(|i| i.modifiers.command);
                b = self.dragged_3d(start, zone, view, o, p, k, ctrl, ball.as_ref(), out);
            }
            if resp.drag_stopped() {
                self.drag_3d = None;
                self.zone_3d = None;
            }
            // The zone's pointer; dragged, the one its press showed.
            if let Some(zone) = active.or(hover) {
                let at = if active.is_some() {
                    ui.input(|i| i.pointer.press_origin())
                } else {
                    resp.hover_pos()
                };
                let corner = match (&corners, center, at) {
                    (Some(c), Some(center), Some(at)) => c
                        .iter()
                        .min_by(|a, b| a.distance(at).total_cmp(&b.distance(at)))
                        .map_or(Vec2::ZERO, |&k| k - center),
                    _ => Vec2::ZERO,
                };
                cursors::set(ui.ctx(), zone.cursor(corner));
            }
            resp.context_menu(|ui| {
                if ui.button(tr("Restore")).clicked() {
                    edits.push(Edit::Space(bv_timeline::space::Box3d::default()));
                    ui.close();
                }
                ui.separator();
                for v in View::ALL {
                    if ui.radio(self.views[i] == v, v.name()).clicked() {
                        self.views[i] = v;
                        ui.close();
                    }
                }
            });
            self.paint_pane(ui, pane, view, k, out, corners, ball, active.or(hover));
        }
        if b != b0 {
            edits.push(Edit::Space(b));
        }
    }

    /// The box dragged from `start` by the pointer, from `o` to `p`, in a
    /// pane of `view` at `k` screen pixels a frame pixel.
    #[allow(clippy::too_many_arguments)]
    fn dragged_3d(
        &self,
        start: bv_timeline::space::Box3d,
        zone: Zone,
        view: View,
        o: Pos2,
        p: Pos2,
        k: f32,
        ctrl: bool,
        ball: Option<&Trackball>,
        out: (f32, f32),
    ) -> bv_timeline::space::Box3d {
        let mut b = start;
        let (dx, dy) = ((p.x - o.x) / k, (p.y - o.y) / k);
        let [no_x, no_y] = self.prevent_move;
        let no_z = self.prevent_z;
        match zone {
            Zone::Move if ctrl && view == View::CameraFront => {
                // Ctrl: nearer or farther (down: farther).
                if !no_z {
                    b.z = start.z + dy / out.0;
                }
            }
            Zone::Move if self.object_space && view == View::CameraFront => {
                // Edit in Object Space: the axes held are the box's own.
                let plane = start.plane(out.0, out.1);
                let unit = |a: [f32; 3]| {
                    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-6);
                    a.map(|c| c / l)
                };
                let (u, v) = (unit(plane.u), unit(plane.v));
                let n = unit([
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ]);
                let d = [dx, -dy, 0.0];
                let mut m = [0.0f32; 3];
                for (axis, held) in [(u, no_x), (v, no_y), (n, no_z)] {
                    if !held {
                        let along = d[0] * axis[0] + d[1] * axis[1] + d[2] * axis[2];
                        for (mi, ai) in m.iter_mut().zip(axis) {
                            *mi += along * ai;
                        }
                    }
                }
                b.x = start.x + m[0] / out.0;
                b.y = start.y + m[1] / out.1;
                b.z = start.z + m[2] / out.0;
            }
            Zone::Move => {
                // The view's two axes: across it, and up it (screen y
                // down).
                let ((across, sa), (up, su)) = view.axes();
                for (axis, amount) in [(across, sa * dx), (up, -su * dy)] {
                    match axis {
                        Axis::X if !no_x => b.x = start.x + amount / out.0,
                        Axis::Y if !no_y => b.y = start.y + amount / out.1,
                        Axis::Z if !no_z => b.z = start.z + amount / out.0,
                        _ => {}
                    }
                }
            }
            Zone::InOut => {
                if let Some(ball) = ball
                    && !no_z
                {
                    // The corner's distance from the middle grows as the box
                    // comes nearer: the size the camera shows it at.
                    let (r0, r1) = (o.distance(ball.center), p.distance(ball.center));
                    if r0 > 1.0 && r1 > 1.0 {
                        let d = bv_timeline::space::CAMERA_DISTANCE;
                        b.z = ((d + start.z) * r0 / r1 - d).clamp(-0.9 * d, 50.0);
                    }
                }
            }
            Zone::RotateX => b.rotation[0] = fxw::wrap180(start.rotation[0] + (p.y - o.y) * 0.5),
            Zone::RotateY => b.rotation[1] = fxw::wrap180(start.rotation[1] + (p.x - o.x) * 0.5),
            Zone::RotateZ => {
                if let Some(ball) = ball {
                    let angle = |q: Pos2| (-(q.y - ball.center.y)).atan2(q.x - ball.center.x);
                    let turn = (angle(p) - angle(o)).to_degrees();
                    b.rotation[2] = fxw::wrap180(start.rotation[2] + turn);
                }
            }
        }
        b
    }

    /// A pane: the frame (its edge, in the flat views, with the camera),
    /// the box as the view shows it, and in Camera Front the trackball,
    /// the zone under the pointer (or dragged) in bold.
    #[allow(clippy::too_many_arguments)]
    fn paint_pane(
        &self,
        ui: &egui::Ui,
        pane: Rect,
        view: View,
        k: f32,
        out: (f32, f32),
        corners: Option<Vec<Pos2>>,
        ball: Option<Trackball>,
        zone: Option<Zone>,
    ) {
        let painter = ui.painter_at(pane);
        painter.rect_filled(pane, 0.0, fxw::WS_BG);
        painter.rect_stroke(
            pane,
            0.0,
            Stroke::new(1.0, Color32::from_gray(70)),
            StrokeKind::Inside,
        );
        let screen = |p: [f32; 3]| {
            view.show(p, out.0)
                .map(|[x, y]| pane.center() + vec2(x * k, y * k))
        };
        let (hw, hh) = (out.0 / 2.0, out.1 / 2.0);
        let frame: Option<Vec<Pos2>> = [
            [-hw, hh, 0.0],
            [hw, hh, 0.0],
            [hw, -hh, 0.0],
            [-hw, -hh, 0.0],
        ]
        .iter()
        .map(|&c| screen(c))
        .collect();
        if let Some(frame) = frame {
            if view.flat() {
                painter.add(Shape::closed_line(
                    frame,
                    Stroke::new(2.0, Color32::from_gray(150)),
                ));
            } else {
                painter.add(Shape::convex_polygon(
                    frame,
                    Color32::from_gray(150),
                    Stroke::new(1.0, Color32::from_gray(90)),
                ));
            }
        }
        if view.flat() {
            // The camera, a frame and a half in front.
            let d = bv_timeline::space::CAMERA_DISTANCE * out.0;
            if let Some(c) = screen([0.0, 0.0, -d]) {
                painter.circle_filled(c, 4.0, theme::text_dim());
            }
        }
        painter.text(
            pane.left_top() + vec2(6.0, 4.0),
            egui::Align2::LEFT_TOP,
            view.name(),
            egui::FontId::proportional(11.0),
            theme::text_dim(),
        );
        let Some(points) = corners else {
            return;
        };
        let bold = zone == Some(Zone::InOut) || (zone == Some(Zone::Move) && ball.is_none());
        let stroke = Stroke::new(if bold { 3.0 } else { 1.5 }, theme::banana());
        painter.add(Shape::convex_polygon(
            points.clone(),
            Color32::from_rgba_unmultiplied(255, 220, 90, 36),
            Stroke::NONE,
        ));
        painter.add(Shape::closed_line(points.clone(), stroke));
        painter.line_segment([points[0], points[1]], Stroke::new(3.0, theme::banana()));
        if let Some(ball) = ball {
            ball.paint(&painter, zone);
        }
    }
}

/// A move of the box by `d` (frame pixels, y down) with the axes
/// `prevent` holds — the frame's, or at `angle` the box's own (Edit in
/// Object Space).
fn held_move(d: (f32, f32), angle: f32, prevent: [bool; 2]) -> (f32, f32) {
    let (mut a, mut b) = crate::pancrop::rot(d, -angle);
    if prevent[0] {
        a = 0.0;
    }
    if prevent[1] {
        b = 0.0;
    }
    crate::pancrop::rot((a, b), angle)
}

/// The TrackMotion keyboard context's commands (VEGAS's): the 3D
/// layouts and the toolbar's toggles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackMotionKey {
    Layout(u8),
    Aspect,
    Center,
    ScaleX,
    ScaleY,
    ScaleZ,
    MoveX,
    MoveY,
    MoveZ,
    Rotation,
    Snap,
    ObjectSpace,
}

/// A Track Motion keyframe preset: a name and a row's settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct MotionPreset {
    pub name: String,
    pub value: PresetValue,
}

/// What a preset keeps: the box (2D or 3D, saved apart, as VEGAS's), or
/// a 2D shadow's or glow's look.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum PresetValue {
    Box(Transform),
    Box3d(bv_timeline::space::Box3d),
    Shadow(HaloLook),
    Glow(HaloLook),
}

impl PresetValue {
    /// Presets of the same kind go to the same row.
    fn same_kind(&self, other: &PresetValue) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// The edit that sets it at the cursor.
    fn edit(self) -> Edit {
        match self {
            PresetValue::Box(tf) => Edit::Box(tf),
            PresetValue::Box3d(b) => Edit::Space(b),
            PresetValue::Shadow(look) | PresetValue::Glow(look) => Edit::Halo(look),
        }
    }
}

impl TrackMotionWindow {
    /// VEGAS's Preset row: the name typed, or picked from the presets of
    /// the row being edited (2D and 3D apart) — which sets the row at the
    /// cursor —, Save Preset and Delete Preset.
    fn presets_row(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut BananaApp,
        current: &PresetValue,
        edits: &mut Vec<Edit>,
    ) {
        let presets = &app.settings.track_motion_presets;
        let names: Vec<(String, PresetValue)> = presets
            .iter()
            .filter(|p| p.value.same_kind(current))
            .map(|p| (p.name.clone(), p.value))
            .collect();
        let name = self.preset.trim().to_string();
        let saved = names.iter().any(|(n, _)| *n == name);
        let (mut save, mut delete) = (false, false);
        ui.horizontal(|ui| {
            ui.label(tr("Preset:"));
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add(
                egui::TextEdit::singleline(&mut self.preset)
                    .desired_width(260.0)
                    .hint_text(tr("(Untitled)")),
            );
            let list = crate::generated_media::list_button(ui);
            egui::Popup::menu(&list).show(|ui| {
                for (n, value) in &names {
                    if crate::menus::check_item(ui, n, *n == name, true, "") {
                        edits.push(value.edit());
                        self.preset = n.clone();
                    }
                }
            });
            let named = !name.is_empty();
            save = crate::widgets::icon_button_ex(
                ui,
                crate::icons::Icon::Save,
                tr("Save Preset"),
                false,
                named,
            )
            .clicked();
            delete = crate::widgets::icon_button_ex(
                ui,
                crate::icons::Icon::Delete,
                tr("Delete Preset"),
                false,
                saved,
            )
            .clicked();
        });
        if save || delete {
            let list = &mut app.settings.track_motion_presets;
            list.retain(|p| !(p.name == name && p.value.same_kind(current)));
            if save {
                list.push(MotionPreset {
                    name,
                    value: *current,
                });
            }
            let _ = app.settings.save();
        }
    }
}

/// 3D Track Motion's Layout list: the workspace's panes (VEGAS's six,
/// its keyboard's 3DLayout1 to 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaneLayout {
    Single,
    SideBySide,
    TopAndBottom,
    /// One large pane on the left, two on the right.
    LargeLeft,
    /// One large pane on top, two below.
    LargeTop,
    Four,
}

impl PaneLayout {
    pub(crate) const ALL: [PaneLayout; 6] = [
        PaneLayout::Single,
        PaneLayout::SideBySide,
        PaneLayout::TopAndBottom,
        PaneLayout::LargeLeft,
        PaneLayout::LargeTop,
        PaneLayout::Four,
    ];

    fn name(self) -> &'static str {
        match self {
            PaneLayout::Single => tr("Single Pane"),
            PaneLayout::SideBySide => tr("Side by Side"),
            PaneLayout::TopAndBottom => tr("Top and Bottom"),
            PaneLayout::LargeLeft => tr("Three Panes, Large Left"),
            PaneLayout::LargeTop => tr("Three Panes, Large Top"),
            PaneLayout::Four => tr("Four Panes"),
        }
    }

    /// Its panes in `rect`, in reading order.
    fn panes(self, rect: Rect) -> Vec<Rect> {
        let (w, h) = (rect.width(), rect.height());
        let at = |x: f32, y: f32, pw: f32, ph: f32| {
            Rect::from_min_size(rect.min + vec2(x * w, y * h), vec2(pw * w, ph * h)).shrink(1.0)
        };
        match self {
            PaneLayout::Single => vec![rect],
            PaneLayout::SideBySide => vec![at(0.0, 0.0, 0.5, 1.0), at(0.5, 0.0, 0.5, 1.0)],
            PaneLayout::TopAndBottom => vec![at(0.0, 0.0, 1.0, 0.5), at(0.0, 0.5, 1.0, 0.5)],
            PaneLayout::LargeLeft => vec![
                at(0.0, 0.0, 0.62, 1.0),
                at(0.62, 0.0, 0.38, 0.5),
                at(0.62, 0.5, 0.38, 0.5),
            ],
            PaneLayout::LargeTop => vec![
                at(0.0, 0.0, 1.0, 0.62),
                at(0.0, 0.62, 0.5, 0.38),
                at(0.5, 0.62, 0.5, 0.38),
            ],
            PaneLayout::Four => vec![
                at(0.0, 0.0, 0.5, 0.5),
                at(0.5, 0.0, 0.5, 0.5),
                at(0.0, 0.5, 0.5, 0.5),
                at(0.5, 0.5, 0.5, 0.5),
            ],
        }
    }
}

/// What a pane shows the box from: the camera (in front of the frame, or
/// behind it), a perspective from above and aside, or straight along an
/// axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum View {
    CameraFront,
    CameraRear,
    Perspective,
    Top,
    Bottom,
    Left,
    Right,
}

/// A frame space's axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    X,
    Y,
    Z,
}

impl View {
    const ALL: [View; 7] = [
        View::CameraFront,
        View::CameraRear,
        View::Perspective,
        View::Top,
        View::Bottom,
        View::Left,
        View::Right,
    ];

    fn name(self) -> &'static str {
        match self {
            View::CameraFront => tr("Camera Front"),
            View::CameraRear => tr("Camera Rear"),
            View::Perspective => tr("Perspective"),
            View::Top => tr("Top"),
            View::Bottom => tr("Bottom"),
            View::Left => tr("Left"),
            View::Right => tr("Right"),
        }
    }

    /// Straight along an axis, no perspective.
    fn flat(self) -> bool {
        matches!(self, View::Top | View::Bottom | View::Left | View::Right)
    }

    /// The frame space's axes across the pane (to the right) and up it,
    /// each with the way it runs (1 or -1).
    fn axes(self) -> ((Axis, f32), (Axis, f32)) {
        match self {
            View::Top => ((Axis::X, 1.0), (Axis::Z, 1.0)),
            View::Bottom => ((Axis::X, 1.0), (Axis::Z, -1.0)),
            View::Left => ((Axis::Z, 1.0), (Axis::Y, 1.0)),
            View::Right => ((Axis::Z, -1.0), (Axis::Y, 1.0)),
            _ => ((Axis::X, 1.0), (Axis::Y, 1.0)),
        }
    }

    /// Where it shows the point `p` (a `w`-wide frame's pixels from its
    /// center, y up, z away from the camera), from the pane's middle in
    /// frame pixels (y down); `None` behind a camera.
    fn show(self, p: [f32; 3], w: f32) -> Option<[f32; 2]> {
        use bv_timeline::space::project;
        let [x, y, z] = p;
        match self {
            View::CameraFront => project(p, w).map(|[a, b]| [a, -b]),
            // A camera as far behind the frame, looking back.
            View::CameraRear => project([-x, y, -z], w).map(|[a, b]| [a, -b]),
            View::Perspective => {
                // Turned 30° about y and 20° about x, seen from twice as far.
                let (sy, cy) = 30f32.to_radians().sin_cos();
                let (sx, cx) = 20f32.to_radians().sin_cos();
                let (x1, z1) = (x * cy - z * sy, x * sy + z * cy);
                let (y2, z2) = (y * cx - z1 * sx, y * sx + z1 * cx);
                let d = 2.0 * bv_timeline::space::CAMERA_DISTANCE * w;
                let depth = d + z2;
                (depth > 1e-3).then(|| [x1 * d / depth, -y2 * d / depth])
            }
            // From above: farther up the pane; from below: down it.
            View::Top => Some([x, -z]),
            View::Bottom => Some([x, z]),
            // From the left: farther to the right; from the right: left.
            View::Left => Some([z, -y]),
            View::Right => Some([-z, -y]),
        }
    }

    /// Screen pixels a frame pixel in `pane`: the frame (and, in the flat
    /// views, the camera before it) fits with room around.
    fn scale(self, pane: Rect, out: (f32, f32)) -> f32 {
        let depth = 2.0 * bv_timeline::space::CAMERA_DISTANCE * out.0 + out.0 / 2.0;
        let (across, up) = match self {
            View::Top | View::Bottom => (out.0, depth),
            View::Left | View::Right => (depth, out.1),
            _ => (out.0, out.1),
        };
        (pane.width() * 0.62 / across).min(pane.height() * 0.62 / up)
    }
}

/// What a drag on the workspace does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Zone {
    /// Across the view (inside the box).
    Move,
    /// Nearer or farther (a corner).
    InOut,
    /// Turns about x (the trackball's top or bottom), y (its sides) or z
    /// (the rest of its rim).
    RotateX,
    RotateY,
    RotateZ,
}

impl Zone {
    /// The pointer over the zone and while it is dragged, as the 2D box's
    /// (VEGAS's Pan/Crop's): inside, the box moved; a corner (`corner`, its
    /// way from the box's middle on the screen), nearer or farther, the
    /// arrows along its diagonal; the trackball's top and bottom ↕, its
    /// sides ↔, the rest of its rim the turning one.
    fn cursor(self, corner: Vec2) -> Cursor {
        match self {
            Zone::Move => Cursor::MoveFrame,
            Zone::InOut if corner.x * corner.y < 0.0 => Cursor::ResizeNesw,
            Zone::InOut => Cursor::ResizeNwse,
            Zone::RotateX => Cursor::ResizeNs,
            Zone::RotateY => Cursor::ResizeEw,
            Zone::RotateZ => Cursor::Rotate,
        }
    }
}

/// The trackball in the box's middle (VEGAS's): a circle with the x and y
/// axes, the zones that turn the box on its rim.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Trackball {
    center: Pos2,
    radius: f32,
}

/// How near (screen pixels) a zone is grabbed.
const GRAB: f32 = 7.0;

impl Trackball {
    /// The trackball of a box whose corners show at `corners`.
    fn around(center: Pos2, corners: &[Pos2]) -> Trackball {
        let short = corners
            .iter()
            .map(|c| c.distance(center))
            .fold(f32::MAX, f32::min);
        Trackball {
            center,
            radius: (short * 0.45).clamp(18.0, 110.0),
        }
    }

    /// The zone at `p`: a corner, the rim (its top and bottom turn about
    /// x, its sides about y, the rest about z; only with Enable Rotation),
    /// inside the box.
    fn zone(&self, p: Pos2, corners: &[Pos2], rotation: bool) -> Option<Zone> {
        if corners.iter().any(|c| c.distance(p) <= GRAB + 3.0) {
            return Some(Zone::InOut);
        }
        let (dx, dy) = (p.x - self.center.x, p.y - self.center.y);
        let r = dx.hypot(dy);
        if rotation && (r - self.radius).abs() <= GRAB {
            let (c, s) = (dx / r, dy / r);
            return Some(if s.abs() > 0.92 {
                Zone::RotateX
            } else if c.abs() > 0.92 {
                Zone::RotateY
            } else {
                Zone::RotateZ
            });
        }
        inside(p, corners).then_some(Zone::Move)
    }

    /// The circle, its axes and labels; the zone's guide in bold.
    fn paint(&self, painter: &egui::Painter, zone: Option<Zone>) {
        let (c, r) = (self.center, self.radius);
        let thin = Stroke::new(1.0, Color32::from_gray(60));
        let bold = Stroke::new(2.5, Color32::from_gray(20));
        let pick = |z: Zone| if zone == Some(z) { bold } else { thin };
        painter.circle_stroke(c, r, pick(Zone::RotateZ));
        // A turn about y shows the circle's meridian, about x its equator.
        let ellipse = |rx: f32, ry: f32, st: Stroke| {
            let points: Vec<Pos2> = (0..48)
                .map(|i| {
                    let a = i as f32 / 48.0 * std::f32::consts::TAU;
                    c + vec2(rx * a.cos(), ry * a.sin())
                })
                .collect();
            painter.add(Shape::closed_line(points, st));
        };
        if zone == Some(Zone::RotateY) {
            ellipse(r * 0.35, r, bold);
        }
        if zone == Some(Zone::RotateX) {
            ellipse(r, r * 0.35, bold);
        }
        let axes_bold = matches!(zone, Some(Zone::Move));
        let x_st = if axes_bold || zone == Some(Zone::RotateX) {
            bold
        } else {
            thin
        };
        let y_st = if axes_bold || zone == Some(Zone::RotateY) {
            bold
        } else {
            thin
        };
        painter.line_segment([c - vec2(r, 0.0), c + vec2(r, 0.0)], x_st);
        painter.line_segment([c + vec2(0.0, r), c - vec2(0.0, r)], y_st);
        let font = egui::FontId::proportional(11.0);
        let ink = Color32::from_gray(25);
        painter.text(
            c + vec2(r + 4.0, 0.0),
            egui::Align2::LEFT_CENTER,
            "X",
            font.clone(),
            ink,
        );
        painter.text(
            c - vec2(0.0, r + 3.0),
            egui::Align2::CENTER_BOTTOM,
            "Y",
            font.clone(),
            ink,
        );
        painter.text(c + vec2(3.0, 3.0), egui::Align2::LEFT_TOP, "Z", font, ink);
    }
}

/// Whether `p` is inside the convex polygon `corners`.
fn inside(p: Pos2, corners: &[Pos2]) -> bool {
    let n = corners.len();
    let side = |i: usize| {
        let (a, b) = (corners[i], corners[(i + 1) % n]);
        (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
    };
    let first = side(0);
    n > 2 && (1..n).all(|i| side(i) * first >= 0.0)
}

/// Whether `p` is on the box a flat view shows (a line when seen edge on,
/// a shape when turned).
fn near_segments(p: Pos2, corners: &[Pos2]) -> bool {
    if inside(p, corners) {
        return true;
    }
    let n = corners.len();
    (0..n).any(|i| {
        let (a, b) = (corners[i], corners[(i + 1) % n]);
        let ab = b - a;
        let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
        (a + ab * t).distance(p) <= GRAB
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_space_holds_the_box_s_own_axes() {
        let near =
            |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3;
        // The frame's axes: X held, the move goes up and down only.
        assert!(near(held_move((10.0, 5.0), 0.0, [true, false]), (0.0, 5.0)));
        // A box turned a quarter: its own axes swap with the frame's.
        let along = held_move((10.0, 0.0), 90.0, [true, false]);
        assert!(near(along, (10.0, 0.0)), "{along:?}");
        let held = held_move((10.0, 0.0), 90.0, [false, true]);
        assert!(near(held, (0.0, 0.0)), "{held:?}");
    }

    #[test]
    fn the_track_motion_keys_switch_layouts_and_toggles() {
        let mut w = TrackMotionWindow::new(TrackId(1), false);
        w.key(TrackMotionKey::Layout(6));
        assert_eq!(w.layout, PaneLayout::Four);
        w.key(TrackMotionKey::Layout(9));
        assert_eq!(w.layout, PaneLayout::Four, "no such layout");
        for (k, before) in [
            (TrackMotionKey::MoveZ, w.prevent_z),
            (TrackMotionKey::ObjectSpace, w.object_space),
            (TrackMotionKey::Snap, w.snap),
        ] {
            w.key(k);
            let after = match k {
                TrackMotionKey::MoveZ => w.prevent_z,
                TrackMotionKey::ObjectSpace => w.object_space,
                _ => w.snap,
            };
            assert_ne!(after, before, "{k:?}");
        }
    }

    #[test]
    fn presets_keep_their_kind() {
        let two = PresetValue::Box(Transform::default());
        let three = PresetValue::Box3d(bv_timeline::space::Box3d::default());
        assert!(!two.same_kind(&three));
        assert!(two.same_kind(&PresetValue::Box(Transform {
            x: 0.5,
            ..Transform::default()
        })));
        assert!(matches!(three.edit(), Edit::Space(_)));
    }

    #[test]
    fn views_show_the_frame_space_their_way() {
        let w = 1920.0;
        let p = [100.0, 50.0, 200.0];
        // Camera Front: the camera's projection, y down.
        let [x, y] = View::CameraFront.show(p, w).unwrap();
        let [px, py] = bv_timeline::space::project(p, w).unwrap();
        assert_eq!((x, y), (px, -py));
        // From above farther is up; from the left, to the right.
        assert_eq!(View::Top.show(p, w), Some([100.0, -200.0]));
        assert_eq!(View::Left.show(p, w), Some([200.0, -50.0]));
        // Dragging up a flat view goes the way it shows.
        for v in [View::Top, View::Bottom, View::Left, View::Right] {
            let ((across, sa), (up, su)) = v.axes();
            let unit = |axis: Axis, s: f32| match axis {
                Axis::X => [s, 0.0, 0.0],
                Axis::Y => [0.0, s, 0.0],
                Axis::Z => [0.0, 0.0, s],
            };
            let [ax, _] = v.show(unit(across, sa), w).unwrap();
            let [_, uy] = v.show(unit(up, su), w).unwrap();
            assert!(ax > 0.0 && uy < 0.0, "{v:?}");
        }
        // Behind the rear camera: nothing.
        assert!(View::CameraRear.show([0.0, 0.0, 3000.0], w).is_none());
    }

    #[test]
    fn the_trackball_s_zones_are_where_vegas_draws_them() {
        let corners = [
            pos2(0.0, 0.0),
            pos2(200.0, 0.0),
            pos2(200.0, 100.0),
            pos2(0.0, 100.0),
        ];
        let ball = Trackball::around(pos2(100.0, 50.0), &corners);
        let (c, r) = (ball.center, ball.radius);
        assert_eq!(ball.zone(pos2(1.0, 1.0), &corners, true), Some(Zone::InOut));
        assert_eq!(
            ball.zone(c + vec2(0.0, -r), &corners, true),
            Some(Zone::RotateX)
        );
        assert_eq!(
            ball.zone(c + vec2(-r, 0.0), &corners, true),
            Some(Zone::RotateY)
        );
        let diagonal = vec2(r, -r) / 2f32.sqrt();
        assert_eq!(ball.zone(c + diagonal, &corners, true), Some(Zone::RotateZ));
        assert_eq!(
            ball.zone(c + vec2(5.0, 5.0), &corners, true),
            Some(Zone::Move)
        );
        // Without Enable Rotation the rim moves the box.
        assert_eq!(ball.zone(c + diagonal, &corners, false), Some(Zone::Move));
        assert_eq!(ball.zone(pos2(300.0, 300.0), &corners, true), None);
    }

    /// The 3D zones' pointers, as the 2D box's: a corner's arrows along its
    /// own diagonal on the screen, the trackball's ↕ and ↔ where a drag up
    /// and down or across turns the box, the turning one on the rest of
    /// its rim.
    #[test]
    fn the_3d_zones_show_the_2d_box_s_pointers() {
        assert_eq!(Zone::Move.cursor(Vec2::ZERO), Cursor::MoveFrame);
        for (way, c) in [
            (vec2(-1.0, -1.0), Cursor::ResizeNwse),
            (vec2(1.0, 1.0), Cursor::ResizeNwse),
            (vec2(1.0, -1.0), Cursor::ResizeNesw),
            (vec2(-1.0, 1.0), Cursor::ResizeNesw),
        ] {
            assert_eq!(Zone::InOut.cursor(way), c, "{way:?}");
        }
        assert_eq!(Zone::RotateX.cursor(Vec2::ZERO), Cursor::ResizeNs);
        assert_eq!(Zone::RotateY.cursor(Vec2::ZERO), Cursor::ResizeEw);
        assert_eq!(Zone::RotateZ.cursor(Vec2::ZERO), Cursor::Rotate);
    }

    #[test]
    fn boxes_and_framings_round_trip() {
        let out = (1920.0, 1080.0);
        let tf = Transform {
            x: 0.2,
            y: -0.1,
            scale: 0.5,
            stretch: 0.8,
            rotation_deg: 30.0,
            ..Transform::default()
        };
        let vp = viewport(&tf, out);
        assert!((vp.cx - (960.0 + 384.0)).abs() < 1e-3 && (vp.cy - (540.0 + 108.0)).abs() < 1e-3);
        assert!((vp.w - 960.0).abs() < 1e-3 && (vp.h - 432.0).abs() < 1e-3);
        let back = transform(&vp, &Transform::default(), out);
        for (a, b) in [
            (back.x, tf.x),
            (back.y, tf.y),
            (back.scale, tf.scale),
            (back.stretch, tf.stretch),
            (back.rotation_deg, tf.rotation_deg),
        ] {
            assert!((a - b).abs() < 1e-5, "{a} {b}");
        }
        // The whole frame: no motion.
        let whole = transform(
            &viewport(&Transform::default(), out),
            &Transform::default(),
            out,
        );
        assert!(whole.is_identity(), "{whole:?}");
    }
}
