//! The Pan/Crop window's masks, as VEGAS 2026's Help describes them
//! ("Creating masks using the Pan/Crop tool") and the user's VEGAS shows
//! them: with the keyframe controller's Mask row picked, the tools draw
//! and edit shapes on the media (the Normal Edit tool, Anchor Creation,
//! Anchor Deletion, Split Tangent, Rectangle and Oval), the properties
//! show the mask's and the picked path's settings, and the workspace
//! darkens what the masks cut. Every change comes back as the event's new
//! masks, for the window to store as one step of the undo history per
//! gesture.

use std::collections::{BTreeSet, HashMap};

use crate::egui::{
    self, Align, Color32, ColorImage, CursorIcon, Layout, Painter, PointerButton, Pos2, Rect,
    Response, Shape, Stroke, StrokeKind, TextureHandle, TextureOptions, UiBuilder, pos2, vec2,
};
use bv_core::Tick;
use bv_timeline::mask::{self, MaskAnchor, MaskFeather, MaskMode, MaskPath, PanCropMask};

use crate::fx_window::{
    Glyph, Space, glyph_button, prop, prop_choice, prop_grid, prop_name, prop_value, prop_yes_no,
    section,
};
use crate::i18n::tr;

/// The tools of the Mask row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MaskTool {
    Normal,
    AddAnchor,
    DeleteAnchor,
    SplitTangent,
    Rectangle,
    Oval,
}

/// An anchor: its path and its place in it.
type AnchorRef = (usize, usize);

/// What a drag on the workspace does.
#[derive(Clone, Debug)]
enum MaskDrag {
    /// The picked path moved (all of its anchors).
    Path {
        path: usize,
        from: (f32, f32),
        shapes: Vec<Vec<MaskAnchor>>,
    },
    /// The anchors picked moved together, `grabbed` (the one under the
    /// pointer) snapping.
    Anchors {
        grabbed: AnchorRef,
        from: (f32, f32),
        shapes: Vec<Vec<MaskAnchor>>,
    },
    /// An anchor's tangent's end: the one after it, or before; `alone`,
    /// the other one stays where it is (the tangents split).
    Tangent {
        path: usize,
        anchor: usize,
        after: bool,
        alone: bool,
        shapes: Vec<Vec<MaskAnchor>>,
    },
    /// The picked path turned about its box's middle (the handle above
    /// the box), from where the drag began.
    Rotate {
        path: usize,
        center: (f32, f32),
        from: (f32, f32),
        shapes: Vec<Vec<MaskAnchor>>,
    },
    /// The picked path scaled about the corner of its box opposite the
    /// one dragged.
    Scale {
        path: usize,
        fixed: (f32, f32),
        corner: (f32, f32),
        shapes: Vec<Vec<MaskAnchor>>,
    },
    /// A rectangle or an oval being drawn from where the drag began.
    Shape { tool: MaskTool, from: (f32, f32) },
    /// The picked path's segment from its anchor `segment` bent from the
    /// point `t` of the way along it: that point follows the pointer, the
    /// anchors' tangents round it (curved anchors stay curved).
    Bend {
        path: usize,
        segment: usize,
        t: f32,
        from: (f32, f32),
        shapes: Vec<Vec<MaskAnchor>>,
    },
    /// A rectangle picking the anchors inside it, from `from` (the
    /// screen's); with Ctrl or Shift, after those picked when it began.
    Marquee {
        from: Pos2,
        base: BTreeSet<AnchorRef>,
    },
}

/// A press on the workspace: where (the screen's and the media's), and
/// how — the start of a drag or a click, the second of a double-click, the
/// keys held.
struct Press {
    o: Pos2,
    at: (f32, f32),
    primary: bool,
    double: bool,
    mods: egui::Modifiers,
}

/// The masks' side of the Pan/Crop editor.
pub(crate) struct MaskEditor {
    pub tool: MaskTool,
    /// The path picked (its settings in the properties).
    pub selected: Option<usize>,
    /// The anchors picked: their tangents show, a drag of one moves them
    /// all, Delete takes them away. A click picks one, Ctrl and a click
    /// adds or takes one, Shift and a click adds one, a rectangle dragged
    /// off the paths picks those inside it.
    pub anchors: BTreeSet<AnchorRef>,
    /// The picked path's box shows (as VEGAS's: a double-click on its line
    /// or its anchors with the Normal Edit tool; a click off the path hides
    /// it): its corners scale the path, the handle above it turns it, the
    /// small toolbar under it sets its mode and feather.
    boxed: bool,
    /// The path the Anchor Creation tool is drawing: a click on its first
    /// anchor closes it.
    drawing: Option<usize>,
    drag: Option<MaskDrag>,
    /// The rectangle picking anchors, while it is dragged.
    marquee: Option<Rect>,
    /// The tangents a click of the Anchor Creation or Split Tangent tool
    /// took from an anchor (as it was): the next click on it, still there,
    /// brings them back.
    hidden: HashMap<AnchorRef, MaskAnchor>,
    /// The properties' sections open: Mask, Position, Path.
    pub sections: [bool; 3],
    /// What the masks cut, darkened, as last drawn (and what it was drawn
    /// from).
    shade: Option<(u64, TextureHandle)>,
}

impl std::fmt::Debug for MaskEditor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MaskEditor")
            .field("tool", &self.tool)
            .field("selected", &self.selected)
            .field("anchors", &self.anchors)
            .field("boxed", &self.boxed)
            .finish_non_exhaustive()
    }
}

impl Default for MaskEditor {
    fn default() -> MaskEditor {
        MaskEditor {
            tool: MaskTool::Normal,
            selected: None,
            anchors: BTreeSet::new(),
            boxed: false,
            drawing: None,
            drag: None,
            marquee: None,
            hidden: HashMap::new(),
            sections: [true, true, true],
            shade: None,
        }
    }
}

/// A click on the picked path's small toolbar.
#[derive(Clone, Copy, Debug)]
enum PathEdit {
    Mode(MaskMode),
    Feather(MaskFeather),
}

/// How near (screen pixels) the pointer must be to grab an anchor or a
/// handle.
const GRAB: f32 = 7.0;

/// How far above the picked path's box its rotation handle is (screen
/// pixels).
const ROTATE_ARM: f32 = 22.0;

/// The event's masks to edit: its own, or new ones, on.
fn editable(mask: Option<&PanCropMask>) -> PanCropMask {
    mask.cloned().unwrap_or(PanCropMask {
        enabled: true,
        ..PanCropMask::default()
    })
}

impl MaskEditor {
    /// The Mask row's tools (none lit while the zoom tool is, `zoom`):
    /// whether one was picked.
    pub(crate) fn toolbar(&mut self, ui: &mut egui::Ui, zoom: bool) -> bool {
        let mut picked = false;
        for (tool, g, tip) in [
            (MaskTool::Normal, Glyph::Pointer, tr("Normal Edit Tool")),
            (MaskTool::AddAnchor, Glyph::Pen, tr("Anchor Creation Tool")),
            (
                MaskTool::DeleteAnchor,
                Glyph::PenMinus,
                tr("Anchor Deletion Tool"),
            ),
            (
                MaskTool::SplitTangent,
                Glyph::SplitTangent,
                tr("Split Tangent Tool"),
            ),
            (
                MaskTool::Rectangle,
                Glyph::MaskRect,
                tr("Rectangle Mask Tool"),
            ),
            (MaskTool::Oval, Glyph::MaskOval, tr("Oval Mask Tool")),
        ] {
            if glyph_button(ui, g, self.tool == tool && !zoom, true, tip).clicked() {
                self.tool = tool;
                self.drawing = None;
                self.boxed = false;
                picked = true;
            }
        }
        picked
    }

    /// Whether the picked path's box shows (interface tests).
    #[cfg(test)]
    pub(crate) fn box_shows(&self) -> bool {
        self.boxed
    }

    /// Picks path `path` and its anchor `anchor`; `boxed`, its box shows
    /// (screen and interface tests).
    pub(crate) fn pick_for_review(
        &mut self,
        path: Option<usize>,
        anchor: Option<usize>,
        boxed: bool,
    ) {
        self.selected = path;
        self.anchors = path.zip(anchor).into_iter().collect();
        self.boxed = boxed && path.is_some();
    }

    /// The properties of the Mask row, as VEGAS's grid lists them: the
    /// masks' Apply to FX, then the picked path's Position and its Path
    /// settings. Returns the event's new masks when something changed.
    pub(crate) fn properties(
        &mut self,
        ui: &mut egui::Ui,
        mask: Option<&PanCropMask>,
        local: Tick,
    ) -> Option<Option<PanCropMask>> {
        let mut m = editable(mask);
        let before = m.clone();
        if self.selected.is_some_and(|i| i >= m.paths.len()) {
            self.selected = None;
        }
        section(ui, &mut self.sections[0], tr("Mask"), |ui| {
            prop_grid(ui, "pcm-mascara", |ui| {
                prop_yes_no(ui, tr("Apply to FX"), "pcm-fx", &mut m.apply_to_fx);
            });
        });
        let Some(i) = self.selected else {
            crate::tabs::hint(
                ui,
                tr(
                    "Pick a mask on the workspace, or draw one with the Rectangle, Oval or Anchor Creation tool.",
                ),
            );
            return (m != before).then_some(Some(m));
        };
        let shape = m.shapes_at(local).swap_remove(i);
        if let Some((lo, hi)) = mask::bounds(&shape) {
            let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
            let mut to = c;
            section(ui, &mut self.sections[1], tr("Position"), |ui| {
                prop_grid(ui, "pcm-posicao", |ui| {
                    prop(ui, "X", crate::fmt::drag_grouped(&mut to[0], 1).speed(1.0));
                    prop(ui, "Y", crate::fmt::drag_grouped(&mut to[1], 1).speed(1.0));
                });
            });
            if to != c {
                m.map_path_at(local, i, |p| [p[0] + to[0] - c[0], p[1] + to[1] - c[1]]);
            }
        }
        let p = &mut m.paths[i];
        section(ui, &mut self.sections[2], tr("Path"), |ui| {
            prop_grid(ui, "pcm-caminho", |ui| {
                let modes: Vec<(MaskMode, &str)> =
                    MaskMode::ALL.iter().map(|&v| (v, tr(v.name()))).collect();
                prop_choice(ui, tr("Mode"), "pcm-modo", &mut p.mode, &modes);
                prop_yes_no(ui, tr("Anti alias"), "pcm-aa", &mut p.anti_alias);
                let mut pct = p.opacity * 100.0;
                if prop(
                    ui,
                    tr("Opacity (%)"),
                    crate::fmt::drag_grouped(&mut pct, 1)
                        .speed(0.5)
                        .range(0.0..=100.0),
                )
                .changed()
                {
                    p.opacity = pct / 100.0;
                }
                let feathers: Vec<(MaskFeather, &str)> = MaskFeather::ALL
                    .iter()
                    .map(|&v| (v, tr(v.name())))
                    .collect();
                prop_choice(
                    ui,
                    tr("Feather type"),
                    "pcm-pena",
                    &mut p.feather,
                    &feathers,
                );
                // Greyed with no feather, as VEGAS's.
                let on = p.feather != MaskFeather::None;
                prop_name(ui, tr("Feather (%)"), on);
                prop_value(ui, |ui| {
                    ui.add_enabled(
                        on,
                        crate::fmt::drag_grouped(&mut p.feather_pct, 1)
                            .speed(0.1)
                            .range(0.0..=100.0),
                    )
                });
                ui.end_row();
            });
        });
        (m != before).then_some(Some(m))
    }

    /// The workspace's mask side: the drags and clicks of the tools on
    /// `resp` (the workspace's), the keys, the menu, and the masks drawn —
    /// what they cut darkened, the paths, their anchors, the picked ones'
    /// tangents, the picked path's box and small toolbar when it shows.
    /// Returns the event's new masks when they changed.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn workspace(
        &mut self,
        ui: &mut egui::Ui,
        resp: &Response,
        painter: &Painter,
        sp: &Space,
        mask: Option<&PanCropMask>,
        local: Tick,
        grid: Option<[f32; 2]>,
        interactive: bool,
    ) -> Option<Option<PanCropMask>> {
        let mut m = editable(mask);
        let before = m.clone();
        let shapes = m.shapes_at(local);
        self.forget_gone(&shapes);
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let mods = ui.input(|i| i.modifiers);
        let snap = |p: (f32, f32)| match grid {
            Some([gx, gy]) if gx > 0.0 && gy > 0.0 => {
                ((p.0 / gx).round() * gx, (p.1 / gy).round() * gy)
            }
            _ => p,
        };
        let primary = interactive && resp.drag_started_by(PointerButton::Primary);
        let clicked = interactive && resp.clicked();
        // Where it was pressed: a click's is where it was let go (the press's
        // is gone by then; a click does not move).
        let press = if primary {
            ui.input(|i| i.pointer.press_origin())
        } else if clicked {
            resp.interact_pointer_pos()
        } else {
            None
        };

        // A press: what it grabs, by tool.
        if let Some(o) = press {
            let h = Press {
                o,
                at: sp.media(o),
                primary,
                double: interactive && resp.double_clicked(),
                mods,
            };
            match self.tool {
                MaskTool::Normal => self.press_normal(&h, sp, &shapes, &mut m, local),
                MaskTool::SplitTangent => self.press_split(&h, sp, &shapes, &mut m, local),
                MaskTool::AddAnchor => self.press_pen(&h, sp, &shapes, &mut m, local, snap(h.at)),
                MaskTool::DeleteAnchor => {
                    if let Some((path, anchor)) = self.anchor_at(sp, &shapes, o) {
                        m.remove_anchor(path, anchor);
                        self.anchors.retain(|&(p, _)| p != path);
                        if path >= m.paths.len() {
                            self.selected = None;
                            self.boxed = false;
                        }
                    }
                }
                MaskTool::Rectangle | MaskTool::Oval => {
                    if primary {
                        self.drag = Some(MaskDrag::Shape {
                            tool: self.tool,
                            from: snap(h.at),
                        });
                    }
                }
            }
        }

        // The drag, from the shapes as they were when it began.
        let mut drawn = None;
        self.marquee = None;
        if interactive
            && resp.dragged()
            && let (Some(d), Some(p)) = (self.drag.clone(), pointer)
        {
            let at = sp.media(p);
            match d {
                MaskDrag::Path {
                    path,
                    from,
                    mut shapes,
                } => {
                    let (mut dx, mut dy) = axis((at.0 - from.0, at.1 - from.1), mods);
                    if let Some(first) = shapes[path].first() {
                        let to = snap((first.at[0] + dx, first.at[1] + dy));
                        (dx, dy) = (to.0 - first.at[0], to.1 - first.at[1]);
                    }
                    for a in &mut shapes[path] {
                        a.at = [a.at[0] + dx, a.at[1] + dy];
                    }
                    m.set_shapes_at(local, shapes);
                }
                MaskDrag::Anchors {
                    grabbed,
                    from,
                    mut shapes,
                } => {
                    let (mut dx, mut dy) = axis((at.0 - from.0, at.1 - from.1), mods);
                    if let Some(g) = shapes.get(grabbed.0).and_then(|s| s.get(grabbed.1)) {
                        let to = snap((g.at[0] + dx, g.at[1] + dy));
                        (dx, dy) = (to.0 - g.at[0], to.1 - g.at[1]);
                    }
                    for &(path, anchor) in &self.anchors {
                        if let Some(a) = shapes.get_mut(path).and_then(|s| s.get_mut(anchor)) {
                            a.at = [a.at[0] + dx, a.at[1] + dy];
                        }
                    }
                    m.set_shapes_at(local, shapes);
                }
                MaskDrag::Tangent {
                    path,
                    anchor,
                    after,
                    alone,
                    mut shapes,
                } => {
                    let a = &mut shapes[path][anchor];
                    let t = [at.0 - a.at[0], at.1 - a.at[1]];
                    let other = if after { a.before } else { a.after };
                    let len = (other[0].hypot(other[1])).max(if alone { 0.0 } else { 1e-3 });
                    let tl = t[0].hypot(t[1]).max(1e-6);
                    // Locked tangents stay in line, the other keeping its
                    // length (both as long when it had none).
                    let mirrored = if other == [0.0; 2] {
                        [-t[0], -t[1]]
                    } else {
                        [-t[0] / tl * len, -t[1] / tl * len]
                    };
                    if after {
                        a.after = t;
                        if !alone {
                            a.before = mirrored;
                        }
                    } else {
                        a.before = t;
                        if !alone {
                            a.after = mirrored;
                        }
                    }
                    a.split = alone;
                    m.set_shapes_at(local, shapes);
                }
                MaskDrag::Rotate {
                    path,
                    center,
                    from,
                    mut shapes,
                } => {
                    // Shift: in steps of 15°.
                    let angle = |q: (f32, f32)| (q.1 - center.1).atan2(q.0 - center.0);
                    let mut a = angle(at) - angle(from);
                    if mods.shift {
                        let step = 15f32.to_radians();
                        a = (a / step).round() * step;
                    }
                    let (s, c) = a.sin_cos();
                    let turn = |v: [f32; 2]| [v[0] * c - v[1] * s, v[0] * s + v[1] * c];
                    for x in &mut shapes[path] {
                        let r = turn([x.at[0] - center.0, x.at[1] - center.1]);
                        x.at = [center.0 + r[0], center.1 + r[1]];
                        x.before = turn(x.before);
                        x.after = turn(x.after);
                    }
                    m.set_shapes_at(local, shapes);
                }
                MaskDrag::Scale {
                    path,
                    fixed,
                    corner,
                    mut shapes,
                } => {
                    let span = |a: f32, b: f32| if (b - a).abs() < 1e-3 { 1e-3 } else { b - a };
                    let mut sx = (at.0 - fixed.0) / span(fixed.0, corner.0);
                    let mut sy = (at.1 - fixed.1) / span(fixed.1, corner.1);
                    if mods.shift {
                        let s = sx.abs().max(sy.abs());
                        (sx, sy) = (s * sx.signum(), s * sy.signum());
                    }
                    for a in &mut shapes[path] {
                        a.at = [
                            fixed.0 + (a.at[0] - fixed.0) * sx,
                            fixed.1 + (a.at[1] - fixed.1) * sy,
                        ];
                        a.before = [a.before[0] * sx, a.before[1] * sy];
                        a.after = [a.after[0] * sx, a.after[1] * sy];
                    }
                    m.set_shapes_at(local, shapes);
                }
                MaskDrag::Shape { tool, from } => {
                    drawn = Some(drawn_shape(tool, from, snap(at), mods));
                }
                MaskDrag::Bend {
                    path,
                    segment,
                    t,
                    from,
                    mut shapes,
                } => {
                    bend(
                        &mut shapes[path],
                        segment,
                        t,
                        [at.0 - from.0, at.1 - from.1],
                    );
                    m.set_shapes_at(local, shapes);
                }
                MaskDrag::Marquee { from, base } => {
                    // The anchors inside it picked as it goes.
                    let r = Rect::from_two_pos(from, p);
                    let mut picked = base;
                    for (i, s) in shapes.iter().enumerate() {
                        for (k, a) in s.iter().enumerate() {
                            if r.contains(sp.screen((a.at[0], a.at[1]))) {
                                picked.insert((i, k));
                            }
                        }
                    }
                    if let Some(&(path, _)) = picked.iter().next()
                        && self
                            .selected
                            .is_none_or(|s| !picked.iter().any(|&(p, _)| p == s))
                    {
                        self.selected = Some(path);
                    }
                    self.anchors = picked;
                    self.marquee = Some(r);
                }
            }
        }
        if resp.drag_stopped() {
            if let (Some(MaskDrag::Shape { tool, from }), Some(p)) = (self.drag.take(), pointer) {
                let path = drawn_shape(tool, from, snap(sp.media(p)), mods);
                if path
                    .bounds()
                    .is_some_and(|(lo, hi)| hi[0] - lo[0] >= 2.0 && hi[1] - lo[1] >= 2.0)
                {
                    m.add_path(path);
                    self.pick_path(m.paths.len() - 1);
                    self.anchors.clear();
                }
            }
            self.drag = None;
        }

        // The keyboard: Delete takes the picked anchors away, or the picked
        // path; Escape ends drawing.
        if interactive && (resp.hovered() || resp.has_focus()) {
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.drawing = None;
            }
            // Taken here, not by the keyframes under the workspace.
            let something = !self.anchors.is_empty() || self.selected.is_some();
            if something
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
            {
                if !self.anchors.is_empty() {
                    self.delete_anchors(&mut m);
                } else if let Some(i) = self.selected {
                    m.remove_path(i);
                    self.selected = None;
                    self.boxed = false;
                }
            }
        }

        // The menu.
        if interactive {
            self.menu(resp, &mut m, local);
        }

        // The cursor.
        if interactive
            && self.drag.is_none()
            && let Some(p) = resp.hover_pos()
        {
            let shapes = m.shapes_at(local);
            let on_line = || self.segment_at(sp, &shapes, p).is_some();
            let icon = match self.tool {
                MaskTool::Rectangle | MaskTool::Oval => CursorIcon::Crosshair,
                MaskTool::AddAnchor => CursorIcon::Cell,
                MaskTool::DeleteAnchor => CursorIcon::NotAllowed,
                _ if self.anchor_at(sp, &shapes, p).is_some()
                    || self.tangent_at(sp, &shapes, p).is_some() =>
                {
                    CursorIcon::PointingHand
                }
                // The turning handle: the turning cursor, drawn.
                _ if self.rotate_at(sp, &shapes, p).is_some() => CursorIcon::None,
                _ if self.corner_at(sp, &shapes, p).is_some() => CursorIcon::ResizeNwSe,
                MaskTool::SplitTangent if on_line() => CursorIcon::Cell,
                MaskTool::Normal if on_line() => CursorIcon::Grab,
                _ if path_at(&shapes, sp.media(p)).is_some() => CursorIcon::Move,
                _ => CursorIcon::Default,
            };
            if icon == CursorIcon::None {
                crate::fx_window::rotate_cursor(ui.ctx(), p);
            } else {
                ui.ctx().set_cursor_icon(icon);
            }
        }
        if matches!(self.drag, Some(MaskDrag::Rotate { .. }))
            && let Some(p) = ui.input(|i| i.pointer.latest_pos())
        {
            crate::fx_window::rotate_cursor(ui.ctx(), p);
        }

        // Drawn with what it is now.
        self.paint(ui, painter, sp, &m, local, drawn.as_ref());
        if self.boxed
            && let Some(i) = self.selected
            && let Some(edit) = self.path_bar(ui, sp, &m, local, i)
        {
            let p = &mut m.paths[i];
            match edit {
                PathEdit::Mode(mode) => p.mode = mode,
                // A second click on the feather in use: none.
                PathEdit::Feather(f) if p.feather == f => p.feather = MaskFeather::None,
                PathEdit::Feather(f) => {
                    p.feather = f;
                    if p.feather_pct <= 0.0 {
                        p.feather_pct = 5.0;
                    }
                }
            }
        }
        (m != before).then(|| (!m.paths.is_empty() || m.enabled).then_some(m))
    }

    /// The Normal Edit tool's press: the box's handles (while it shows),
    /// a picked anchor's tangent, an anchor (picked; dragged, the picked
    /// ones with it), a line (bent from where it was taken, its anchors'
    /// tangents showing as they follow), the inside of a path (moved; with
    /// Ctrl a copy), or nothing (a rectangle picking anchors, or a click
    /// picking nothing). A double-click on a path's line or anchors shows
    /// its box.
    fn press_normal(
        &mut self,
        h: &Press,
        sp: &Space,
        shapes: &[Vec<MaskAnchor>],
        m: &mut PanCropMask,
        local: Tick,
    ) {
        let adding = h.mods.ctrl || h.mods.shift;
        if let Some(center) = self.rotate_at(sp, shapes, h.o) {
            if h.primary {
                self.drag = Some(MaskDrag::Rotate {
                    path: self.selected.unwrap_or(0),
                    center,
                    from: h.at,
                    shapes: shapes.to_vec(),
                });
            }
            return;
        }
        if let Some((fixed, corner)) = self.corner_at(sp, shapes, h.o) {
            if h.primary {
                self.drag = Some(MaskDrag::Scale {
                    path: self.selected.unwrap_or(0),
                    fixed,
                    corner,
                    shapes: shapes.to_vec(),
                });
            }
            return;
        }
        if let Some((after, path, anchor)) = self.tangent_at(sp, shapes, h.o) {
            if h.primary {
                // Locked tangents turn together; with Alt they split.
                self.drag = Some(MaskDrag::Tangent {
                    path,
                    anchor,
                    after,
                    alone: shapes[path][anchor].split || h.mods.alt,
                    shapes: shapes.to_vec(),
                });
            }
            return;
        }
        if let Some((path, anchor)) = self.anchor_at(sp, shapes, h.o) {
            let key = (path, anchor);
            self.pick_path(path);
            if h.primary {
                if !adding && !self.anchors.contains(&key) {
                    self.anchors.clear();
                }
                self.anchors.insert(key);
                self.drag = Some(MaskDrag::Anchors {
                    grabbed: key,
                    from: h.at,
                    shapes: shapes.to_vec(),
                });
            } else if h.mods.ctrl {
                if !self.anchors.remove(&key) {
                    self.anchors.insert(key);
                }
            } else if h.mods.shift {
                self.anchors.insert(key);
            } else {
                self.anchors = BTreeSet::from([key]);
            }
            self.boxed |= h.double;
            return;
        }
        if let Some((path, segment, t)) = self.segment_at(sp, shapes, h.o) {
            self.pick_path(path);
            if h.primary {
                let n = shapes[path].len();
                self.anchors = BTreeSet::from([(path, segment), (path, (segment + 1) % n)]);
                self.drag = Some(MaskDrag::Bend {
                    path,
                    segment,
                    t,
                    from: h.at,
                    shapes: shapes.to_vec(),
                });
            } else if !adding {
                self.anchors.clear();
            }
            self.boxed |= h.double;
            return;
        }
        if let Some(path) = path_at(shapes, h.at) {
            self.pick_path(path);
            if h.primary {
                // Ctrl: a copy dragged, the original left.
                let (path, shapes) = match h.mods.ctrl.then(|| m.duplicate_path(path, [0.0; 2])) {
                    Some(Some(copy)) => {
                        self.pick_path(copy);
                        self.anchors.clear();
                        (copy, m.shapes_at(local))
                    }
                    _ => (path, shapes.to_vec()),
                };
                self.drag = Some(MaskDrag::Path {
                    path,
                    from: h.at,
                    shapes,
                });
            }
            return;
        }
        // Off the paths: a rectangle picks anchors; a click, nothing.
        self.boxed = false;
        if h.primary {
            let base = if adding {
                self.anchors.clone()
            } else {
                BTreeSet::new()
            };
            self.drag = Some(MaskDrag::Marquee { from: h.o, base });
        } else if !adding {
            self.selected = None;
            self.anchors.clear();
        }
    }

    /// The Split Tangent tool's press: a picked anchor's tangent dragged
    /// alone (split from the other); an anchor clicked turned a corner, or
    /// curved again, dragged its tangents pulled out; a line given an
    /// anchor exactly where it was clicked, the curve kept.
    fn press_split(
        &mut self,
        h: &Press,
        sp: &Space,
        shapes: &[Vec<MaskAnchor>],
        m: &mut PanCropMask,
        local: Tick,
    ) {
        if let Some((after, path, anchor)) = self.tangent_at(sp, shapes, h.o) {
            if h.primary {
                self.drag = Some(MaskDrag::Tangent {
                    path,
                    anchor,
                    after,
                    alone: true,
                    shapes: shapes.to_vec(),
                });
            }
            return;
        }
        if let Some((path, anchor)) = self.anchor_at(sp, shapes, h.o) {
            self.pick_path(path);
            self.anchors = BTreeSet::from([(path, anchor)]);
            if h.primary {
                self.drag = Some(MaskDrag::Tangent {
                    path,
                    anchor,
                    after: true,
                    alone: shapes[path][anchor].split,
                    shapes: shapes.to_vec(),
                });
            } else {
                let mut s = shapes.to_vec();
                self.toggle(&mut s[path], path, anchor);
                m.set_shapes_at(local, s);
            }
            return;
        }
        if let Some((path, segment, t)) = self.segment_at(sp, shapes, h.o) {
            self.split_at(m, local, path, segment, t, h.primary);
            return;
        }
        if !h.primary && !(h.mods.ctrl || h.mods.shift) {
            self.selected = None;
            self.anchors.clear();
            self.boxed = false;
        }
    }

    /// The Anchor Creation tool's press, as the user's VEGAS: on the first
    /// anchor of the path being drawn it closes it; on another anchor a
    /// click makes a curved one a corner and a corner curved again, a drag
    /// pulls its tangents out; drawing, a click adds a corner (the line to
    /// it straight) and a drag a curve; on a path's line an anchor goes
    /// exactly there, the curve kept; elsewhere a new path begins
    /// (`corner` the anchor snapped).
    fn press_pen(
        &mut self,
        h: &Press,
        sp: &Space,
        shapes: &[Vec<MaskAnchor>],
        m: &mut PanCropMask,
        local: Tick,
        corner: (f32, f32),
    ) {
        let corner = MaskAnchor::corner([corner.0, corner.1]);
        if let Some(d) = self.drawing
            && self.near_first(sp, shapes, d, h.o)
        {
            self.drawing = None;
            return;
        }
        if let Some((path, anchor)) = self.anchor_at(sp, shapes, h.o) {
            self.pick_path(path);
            self.anchors = BTreeSet::from([(path, anchor)]);
            if h.primary {
                self.pull_tangents(true, path, anchor, m, local);
            } else {
                let mut s = shapes.to_vec();
                self.toggle(&mut s[path], path, anchor);
                m.set_shapes_at(local, s);
            }
            return;
        }
        if let Some(d) = self.drawing {
            let n = shapes[d].len();
            m.insert_anchor(d, n, local, corner);
            self.anchors = BTreeSet::from([(d, n)]);
            if h.primary {
                // Dragged: a curve, its tangents pulled out.
                self.pull_tangents(true, d, n, m, local);
            } else {
                // A click: a straight line to it.
                let mut s = m.shapes_at(local);
                if let Some(prev) = s[d].get_mut(n.wrapping_sub(1))
                    && prev.after != [0.0; 2]
                {
                    prev.split = prev.before != [0.0; 2];
                    prev.after = [0.0; 2];
                }
                m.set_shapes_at(local, s);
            }
            return;
        }
        if let Some((path, segment, t)) = self.segment_at(sp, shapes, h.o) {
            self.split_at(m, local, path, segment, t, h.primary);
            return;
        }
        m.add_path(MaskPath::new(vec![corner]));
        let i = m.paths.len() - 1;
        self.drawing = Some(i);
        self.pick_path(i);
        self.anchors = BTreeSet::from([(i, 0)]);
        self.pull_tangents(h.primary, i, 0, m, local);
    }

    /// An anchor put on path `path`'s segment from its anchor `segment`,
    /// `t` of the way along it (the curve kept) and picked; dragged
    /// (`primary`), its tangents pulled out.
    fn split_at(
        &mut self,
        m: &mut PanCropMask,
        local: Tick,
        path: usize,
        segment: usize,
        t: f32,
        primary: bool,
    ) {
        m.split_segment(path, segment, t);
        self.pick_path(path);
        self.anchors = BTreeSet::from([(path, segment + 1)]);
        self.pull_tangents(primary, path, segment + 1, m, local);
    }

    /// Path `path` picked: the box goes when it was another's.
    fn pick_path(&mut self, path: usize) {
        if self.selected != Some(path) {
            self.boxed = false;
        }
        self.selected = Some(path);
    }

    /// Anchor `i` of path `path` (in `shape`) clicked by the Anchor Creation
    /// or Split Tangent tool: curved, it becomes a corner (its tangents kept
    /// aside); a corner, its tangents come back — those a click took, else
    /// along the line through its neighbors.
    fn toggle(&mut self, shape: &mut [MaskAnchor], path: usize, i: usize) {
        let a = shape[i];
        if a.before != [0.0; 2] || a.after != [0.0; 2] {
            self.hidden.insert((path, i), a);
            shape[i] = MaskAnchor::corner(a.at);
        } else if let Some(was) = self.hidden.remove(&(path, i)).filter(|w| w.at == a.at) {
            shape[i] = was;
        } else {
            toggle_corner(shape, i);
        }
    }

    /// The picked anchors taken away (a path left with fewer than three
    /// goes).
    fn delete_anchors(&mut self, m: &mut PanCropMask) {
        let picked = std::mem::take(&mut self.anchors);
        let paths: BTreeSet<usize> = picked.iter().map(|&(p, _)| p).collect();
        for &path in paths.iter().rev() {
            for &(_, anchor) in picked.iter().rev().filter(|&&(p, _)| p == path) {
                let n = m.paths.get(path).map_or(0, |p| p.anchors.len());
                m.remove_anchor(path, anchor);
                if n <= 3 {
                    // The path went with it.
                    break;
                }
            }
        }
        if self.selected.is_some_and(|i| i >= m.paths.len()) {
            self.selected = None;
            self.boxed = false;
        }
    }

    /// What no longer exists forgotten: paths and anchors picked past the
    /// shapes, the path being drawn.
    fn forget_gone(&mut self, shapes: &[Vec<MaskAnchor>]) {
        if self.selected.is_some_and(|i| i >= shapes.len()) {
            self.selected = None;
        }
        if self.selected.is_none() {
            self.boxed = false;
        }
        self.anchors
            .retain(|&(p, a)| shapes.get(p).is_some_and(|s| a < s.len()));
        if self.drawing.is_some_and(|i| i >= shapes.len()) {
            self.drawing = None;
        }
    }

    /// With the Anchor Creation tool, a press that drags pulls the new
    /// anchor's tangents out.
    fn pull_tangents(
        &mut self,
        primary: bool,
        path: usize,
        anchor: usize,
        m: &PanCropMask,
        local: Tick,
    ) {
        if primary {
            self.drag = Some(MaskDrag::Tangent {
                path,
                anchor,
                after: true,
                alone: false,
                shapes: m.shapes_at(local),
            });
        }
    }

    /// The anchor under `p` (screen): the picked path's first.
    fn anchor_at(&self, sp: &Space, shapes: &[Vec<MaskAnchor>], p: Pos2) -> Option<AnchorRef> {
        let near = |path: usize| {
            shapes[path]
                .iter()
                .position(|a| sp.screen((a.at[0], a.at[1])).distance(p) <= GRAB)
                .map(|i| (path, i))
        };
        if let Some(found) = self.selected.and_then(near) {
            return Some(found);
        }
        (0..shapes.len()).rev().find_map(near)
    }

    /// The end of a tangent of a picked anchor under `p`: whether it is
    /// the one after it, and the anchor.
    fn tangent_at(
        &self,
        sp: &Space,
        shapes: &[Vec<MaskAnchor>],
        p: Pos2,
    ) -> Option<(bool, usize, usize)> {
        self.anchors.iter().find_map(|&(path, i)| {
            let a = shapes.get(path)?.get(i)?;
            [(true, a.after), (false, a.before)]
                .into_iter()
                .find(|(_, t)| {
                    *t != [0.0; 2]
                        && sp.screen((a.at[0] + t[0], a.at[1] + t[1])).distance(p) <= GRAB
                })
                .map(|(after, _)| (after, path, i))
        })
    }

    /// The line under `p` (screen) — the picked path's first; not the
    /// closing one of the path being drawn —: its path, the anchor its
    /// segment starts at, and how far along it the point nearest `p` is.
    fn segment_at(
        &self,
        sp: &Space,
        shapes: &[Vec<MaskAnchor>],
        p: Pos2,
    ) -> Option<(usize, usize, f32)> {
        let on = |path: usize| {
            let open = self.drawing == Some(path);
            segment_hit(sp, &shapes[path], p, open).map(|(s, t)| (path, s, t))
        };
        if let Some(found) = self.selected.filter(|&i| i < shapes.len()).and_then(on) {
            return Some(found);
        }
        (0..shapes.len()).rev().find_map(on)
    }

    /// The picked path's rotation handle under `p`, while its box shows:
    /// its box's middle (in the media), what it turns about.
    fn rotate_at(&self, sp: &Space, shapes: &[Vec<MaskAnchor>], p: Pos2) -> Option<(f32, f32)> {
        if !self.boxed {
            return None;
        }
        let (lo, hi) = mask::bounds(shapes.get(self.selected?)?)?;
        let b = Rect::from_min_max(sp.screen((lo[0], lo[1])), sp.screen((hi[0], hi[1])));
        let knob = pos2(b.center().x, b.top()) - vec2(0.0, ROTATE_ARM);
        (knob.distance(p) <= GRAB).then_some(((lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0))
    }

    /// A corner of the picked path's box under `p`, while it shows: the
    /// corner opposite (which stays) and it.
    fn corner_at(
        &self,
        sp: &Space,
        shapes: &[Vec<MaskAnchor>],
        p: Pos2,
    ) -> Option<((f32, f32), (f32, f32))> {
        if !self.boxed {
            return None;
        }
        let (lo, hi) = mask::bounds(shapes.get(self.selected?)?)?;
        for (c, o) in [
            ((lo[0], lo[1]), (hi[0], hi[1])),
            ((hi[0], lo[1]), (lo[0], hi[1])),
            ((hi[0], hi[1]), (lo[0], lo[1])),
            ((lo[0], hi[1]), (hi[0], lo[1])),
        ] {
            if sp.screen(c).distance(p) <= GRAB {
                return Some((o, c));
            }
        }
        None
    }

    /// Whether `p` is on the first anchor of path `d` (closing it).
    fn near_first(&self, sp: &Space, shapes: &[Vec<MaskAnchor>], d: usize, p: Pos2) -> bool {
        shapes[d].len() >= 3
            && shapes[d]
                .first()
                .is_some_and(|a| sp.screen((a.at[0], a.at[1])).distance(p) <= GRAB)
    }

    /// The workspace's menu in the Mask row. Its tangents' items work on
    /// the picked path's picked anchors, or on all of its anchors.
    fn menu(&mut self, resp: &Response, m: &mut PanCropMask, local: Tick) {
        let picked = self.selected.filter(|&i| i < m.paths.len());
        let some: BTreeSet<usize> = picked
            .map(|i| {
                self.anchors
                    .iter()
                    .filter(|&&(p, _)| p == i)
                    .map(|&(_, a)| a)
                    .collect()
            })
            .unwrap_or_default();
        resp.context_menu(|ui| {
            let item = |ui: &mut egui::Ui, label: &str, on: bool| {
                let r = ui.add_enabled(on, egui::Button::new(label));
                if r.clicked() {
                    ui.close();
                }
                r.clicked()
            };
            if item(ui, tr("Reset Mask"), !m.paths.is_empty()) {
                *m = PanCropMask {
                    enabled: m.enabled,
                    apply_to_fx: m.apply_to_fx,
                    ..PanCropMask::default()
                };
                self.selected = None;
                self.anchors.clear();
                self.boxed = false;
            }
            ui.separator();
            if item(ui, tr("Deselect"), picked.is_some()) {
                self.selected = None;
                self.anchors.clear();
                self.boxed = false;
            }
            if item(ui, tr("Delete"), picked.is_some())
                && let Some(i) = picked
            {
                m.remove_path(i);
                self.selected = None;
                self.anchors.clear();
                self.boxed = false;
            }
            if item(ui, tr("Duplicate"), picked.is_some())
                && let Some(i) = picked
            {
                self.selected = m.duplicate_path(i, [16.0, 16.0]);
                self.anchors.clear();
                self.boxed = false;
            }
            ui.separator();
            let tangents = |m: &mut PanCropMask, f: &dyn Fn(&[MaskAnchor], usize) -> MaskAnchor| {
                if let Some(i) = picked {
                    let mut shapes = m.shapes_at(local);
                    let s = shapes[i].clone();
                    shapes[i] = (0..s.len())
                        .map(|k| {
                            if some.is_empty() || some.contains(&k) {
                                f(&s, k)
                            } else {
                                s[k]
                            }
                        })
                        .collect();
                    m.set_shapes_at(local, shapes);
                }
            };
            if item(ui, tr("Initialize Tangents"), picked.is_some()) {
                tangents(m, &|s, k| {
                    let n = s.len();
                    let (a, b) = (s[(k + n - 1) % n].at, s[(k + 1) % n].at);
                    let t = [(b[0] - a[0]) / 6.0, (b[1] - a[1]) / 6.0];
                    MaskAnchor {
                        at: s[k].at,
                        before: [-t[0], -t[1]],
                        after: t,
                        split: false,
                    }
                });
            }
            if item(ui, tr("Hide Tangents"), picked.is_some()) {
                tangents(m, &|s, k| MaskAnchor::corner(s[k].at));
            }
            if item(ui, tr("Split Tangents"), picked.is_some()) {
                tangents(m, &|s, k| MaskAnchor {
                    split: true,
                    ..s[k]
                });
            }
            ui.separator();
            for (label, h) in [(tr("Flip Horizontal"), true), (tr("Flip Vertical"), false)] {
                if item(ui, label, picked.is_some())
                    && let Some(i) = picked
                    && let Some((lo, hi)) = mask::bounds(&m.shapes_at(local)[i])
                {
                    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
                    m.map_path_at(local, i, |p| {
                        if h {
                            [2.0 * c[0] - p[0], p[1]]
                        } else {
                            [p[0], 2.0 * c[1] - p[1]]
                        }
                    });
                }
            }
        });
    }

    /// The masks on the workspace: what they cut darkened (when they
    /// mask), each path dashed with its anchors (the picked ones filled),
    /// the picked anchors' tangents, the picked path's box while it shows,
    /// the shape being drawn and the rectangle picking anchors.
    fn paint(
        &mut self,
        ui: &egui::Ui,
        painter: &Painter,
        sp: &Space,
        m: &PanCropMask,
        local: Tick,
        drawn: Option<&MaskPath>,
    ) {
        let media = Rect::from_min_max(sp.screen((0.0, 0.0)), sp.screen(sp.src));
        if m.active() {
            let tex = self.shade(ui.ctx(), m, local, sp.src);
            painter.image(
                tex,
                media,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let shapes = m.shapes_at(local);
        let yellow = Color32::from_rgb(250, 210, 40);
        for (i, (path, s)) in m.paths.iter().zip(&shapes).enumerate() {
            let picked = self.selected == Some(i);
            let dim = path.mode == MaskMode::Disabled;
            dashed(painter, sp, s, dim, self.drawing == Some(i));
            // Its feather's reach: dotted where the edge's softening ends
            // (In: inside it, Out: outside, Both: half each way).
            if path.feather != MaskFeather::None && path.feather_pct > 0.0 && s.len() >= 2 {
                let short = sp.src.0.min(sp.src.1) as f64;
                let d = path.feather_pct as f64 / 100.0 * short;
                let poly = mask::polygon(s);
                let reach: &[f64] = match path.feather {
                    MaskFeather::In => &[-d],
                    MaskFeather::Out => &[d],
                    _ => &[-d / 2.0, d / 2.0],
                };
                for &k in reach {
                    let mut pts: Vec<Pos2> = mask::offset_polygon(&poly, k)
                        .into_iter()
                        .map(|q| sp.screen((q[0] as f32, q[1] as f32)))
                        .collect();
                    if let Some(&first) = pts.first() {
                        pts.push(first);
                    }
                    let alpha = if dim { 70 } else { 150 };
                    painter.extend(Shape::dashed_line(
                        &pts,
                        Stroke::new(1.0, Color32::from_white_alpha(alpha)),
                        2.0,
                        3.0,
                    ));
                }
            }
            if picked
                && self.boxed
                && let Some((lo, hi)) = mask::bounds(s)
            {
                let b = Rect::from_min_max(sp.screen((lo[0], lo[1])), sp.screen((hi[0], hi[1])));
                // The rotation handle above the box.
                let top = pos2(b.center().x, b.top());
                let knob = top - vec2(0.0, ROTATE_ARM);
                painter.line_segment(
                    [top, knob],
                    Stroke::new(1.0, Color32::from_white_alpha(140)),
                );
                painter.circle(knob, 4.5, yellow, Stroke::new(1.0, Color32::BLACK));
                painter.rect_stroke(
                    b,
                    0.0,
                    Stroke::new(1.0, Color32::from_white_alpha(140)),
                    StrokeKind::Middle,
                );
                for c in [
                    b.left_top(),
                    b.right_top(),
                    b.right_bottom(),
                    b.left_bottom(),
                ] {
                    let r = Rect::from_center_size(c, vec2(8.0, 8.0));
                    painter.rect_filled(r, 0.0, yellow);
                    painter.rect_stroke(
                        r,
                        0.0,
                        Stroke::new(1.0, Color32::BLACK),
                        StrokeKind::Inside,
                    );
                }
            }
            // The picked anchors' tangents, under the anchors.
            for (k, a) in s.iter().enumerate() {
                if !self.anchors.contains(&(i, k)) {
                    continue;
                }
                let c = sp.screen((a.at[0], a.at[1]));
                for t in [a.before, a.after] {
                    if t != [0.0; 2] {
                        let e = sp.screen((a.at[0] + t[0], a.at[1] + t[1]));
                        painter.line_segment([c, e], Stroke::new(1.0, yellow));
                        painter.circle(e, 3.5, Color32::WHITE, Stroke::new(1.0, Color32::BLACK));
                    }
                }
            }
            for (k, a) in s.iter().enumerate() {
                let c = sp.screen((a.at[0], a.at[1]));
                let r = Rect::from_center_size(c, vec2(6.0, 6.0));
                let (fill, edge) = if self.anchors.contains(&(i, k)) {
                    (crate::theme::banana(), Color32::WHITE)
                } else if picked {
                    (Color32::BLACK, Color32::WHITE)
                } else {
                    (Color32::WHITE, Color32::BLACK)
                };
                painter.rect_filled(r, 0.0, fill);
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, edge), StrokeKind::Outside);
            }
        }
        if let Some(path) = drawn {
            dashed(painter, sp, &path.anchors, false, false);
        }
        if let Some(r) = self.marquee {
            painter.rect_filled(r, 0.0, Color32::from_white_alpha(24));
            painter.rect_stroke(
                r,
                0.0,
                Stroke::new(1.0, Color32::from_white_alpha(200)),
                StrokeKind::Inside,
            );
        }
    }

    /// The small toolbar under the picked path's box while it shows
    /// (Positive, Negative; Feather In, Both, Out): the change to its path,
    /// if one was clicked.
    fn path_bar(
        &self,
        ui: &mut egui::Ui,
        sp: &Space,
        m: &PanCropMask,
        local: Tick,
        i: usize,
    ) -> Option<PathEdit> {
        let (lo, hi) = mask::bounds(m.shapes_at(local).get(i)?)?;
        let path = m.paths.get(i)?;
        let b = Rect::from_min_max(sp.screen((lo[0], lo[1])), sp.screen((hi[0], hi[1])));
        let bar = Rect::from_min_size(
            pos2(b.center().x - 82.0, b.bottom() + 10.0),
            vec2(164.0, 26.0),
        );
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(bar)
                .layout(Layout::left_to_right(Align::Center)),
        );
        child
            .painter()
            .rect_filled(bar, 4.0, Color32::from_black_alpha(170));
        child.spacing_mut().item_spacing.x = 1.0;
        child.add_space(3.0);
        let mut out = None;
        for (mode, g, tip) in [
            (MaskMode::Positive, Glyph::MaskPositive, tr("Positive")),
            (MaskMode::Negative, Glyph::MaskNegative, tr("Negative")),
        ] {
            if glyph_button(&mut child, g, path.mode == mode, true, tip).clicked() {
                out = Some(PathEdit::Mode(mode));
            }
        }
        child.add_space(8.0);
        for (feather, g, tip) in [
            (MaskFeather::In, Glyph::FeatherIn, tr("Feather Inside")),
            (MaskFeather::Both, Glyph::FeatherBoth, tr("Feather Both")),
            (MaskFeather::Out, Glyph::FeatherOut, tr("Feather Outside")),
        ] {
            if glyph_button(&mut child, g, path.feather == feather, true, tip).clicked() {
                out = Some(PathEdit::Feather(feather));
            }
        }
        out
    }

    /// What the masks cut, as a see-through dark picture over the media
    /// (worked out at a small size, again only when they change).
    fn shade(
        &mut self,
        ctx: &egui::Context,
        m: &PanCropMask,
        local: Tick,
        src: (f32, f32),
    ) -> egui::TextureId {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        format!("{m:?}{local:?}{src:?}").hash(&mut h);
        let key = h.finish();
        if let Some((k, tex)) = &self.shade
            && *k == key
        {
            return tex.id();
        }
        let k = (320.0 / src.0.max(1.0)).min(1.0);
        let (w, h) = (
            ((src.0 * k).round() as usize).max(1),
            ((src.1 * k).round() as usize).max(1),
        );
        let short = src.0.min(src.1) as f64;
        let keep = m.coverage(local, w, h, [k as f64, k as f64], short);
        let pixels: Vec<Color32> = keep
            .iter()
            .map(|c| Color32::from_black_alpha(((1.0 - c.clamp(0.0, 1.0)) * 150.0) as u8))
            .collect();
        let img = ColorImage::new([w, h], pixels);
        match &mut self.shade {
            Some((k, tex)) => {
                tex.set(img, TextureOptions::LINEAR);
                *k = key;
                tex.id()
            }
            None => {
                let tex = ctx.load_texture("pancrop-mask-shade", img, TextureOptions::LINEAR);
                let id = tex.id();
                self.shade = Some((key, tex));
                id
            }
        }
    }
}

impl crate::app::BananaApp {
    /// The picked event with Pan/Crop masks — a feathered oval, positive,
    /// and a rectangle cut out of it at 60 % — and its Pan/Crop window on
    /// the Mask row, the oval and its first anchor picked, its box showing
    /// (screen tests).
    pub fn pan_crop_masks_for_review(&mut self) -> bool {
        let Some(id) = self.primary_clip() else {
            return false;
        };
        let size = self
            .project()
            .clip(id)
            .and_then(|c| self.project().media(c.media))
            .and_then(|m| m.video)
            .map(|v| (v.width as f32, v.height as f32));
        let Some((w, h)) = size else {
            return false;
        };
        let mut oval = MaskPath::oval([0.18 * w, 0.12 * h], [0.72 * w, 0.88 * h]);
        oval.feather = MaskFeather::Both;
        oval.feather_pct = 4.0;
        let mut hole = MaskPath::rectangle([0.38 * w, 0.42 * h], [0.52 * w, 0.62 * h]);
        hole.mode = MaskMode::Negative;
        hole.opacity = 0.6;
        let mut m = PanCropMask {
            enabled: true,
            ..PanCropMask::default()
        };
        m.add_path(oval);
        m.add_path(hole);
        self.modify(|p| {
            if let Some(c) = p.clip_mut(id) {
                c.mask = Some(m);
            }
        });
        self.open_clip_window("pancrop");
        self.clip_windows.mask_row = Some((Some(0), Some(0), true));
        true
    }
}

/// The segment of `shape` under `p` (the screen's, within reach): the
/// anchor it starts at and how far along it (0–1) its point nearest `p`
/// is — found among samples, then narrowed down between the samples next
/// to it. `open`: the path is being drawn, its closing segment not there.
fn segment_hit(sp: &Space, shape: &[MaskAnchor], p: Pos2, open: bool) -> Option<(usize, f32)> {
    const SAMPLES: usize = 64;
    let n = shape.len();
    if n < 2 {
        return None;
    }
    let dist = |i: usize, t: f32| {
        let q = mask::segment_point(shape, i, t);
        sp.screen((q[0], q[1])).distance(p)
    };
    let mut best: Option<(usize, f32, f32)> = None;
    for i in 0..if open { n - 1 } else { n } {
        for k in 0..=SAMPLES {
            let t = k as f32 / SAMPLES as f32;
            let d = dist(i, t);
            if best.is_none_or(|(_, _, bd)| d < bd) {
                best = Some((i, t, d));
            }
        }
    }
    let (i, t, _) = best?;
    let step = 1.0 / SAMPLES as f32;
    let (mut lo, mut hi) = ((t - step).max(0.0), (t + step).min(1.0));
    for _ in 0..40 {
        let (a, b) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
        if dist(i, a) <= dist(i, b) {
            hi = b;
        } else {
            lo = a;
        }
    }
    let t = (lo + hi) / 2.0;
    (dist(i, t) <= GRAB).then_some((i, t.clamp(0.005, 0.995)))
}

/// An anchor clicked by the Anchor Creation or Split Tangent tool, with no
/// tangents of its own kept aside: curved, it becomes a corner (its
/// tangents gone); a corner, it is curved again — its tangents along the
/// line through its neighbors, a third of the way to each.
fn toggle_corner(shape: &mut [MaskAnchor], i: usize) {
    let n = shape.len();
    let a = shape[i];
    if a.before != [0.0; 2] || a.after != [0.0; 2] {
        shape[i] = MaskAnchor::corner(a.at);
        return;
    }
    if n < 3 {
        return;
    }
    let (prev, next) = (shape[(i + n - 1) % n].at, shape[(i + 1) % n].at);
    let d = [next[0] - prev[0], next[1] - prev[1]];
    let len = d[0].hypot(d[1]).max(1e-6);
    let u = [d[0] / len, d[1] / len];
    let to_prev = (prev[0] - a.at[0]).hypot(prev[1] - a.at[1]) / 3.0;
    let to_next = (next[0] - a.at[0]).hypot(next[1] - a.at[1]) / 3.0;
    shape[i].before = [-u[0] * to_prev, -u[1] * to_prev];
    shape[i].after = [u[0] * to_next, u[1] * to_next];
    shape[i].split = false;
}

/// The segment of `shape` from anchor `segment` bent: its point `t` of the
/// way along moved by `by` (media pixels), the tangents at its ends
/// stretched by how much each pulls that point (the cubic's own weights);
/// at a curved anchor the tangent on the other side turns along, keeping
/// its length, so the curve stays smooth there.
fn bend(shape: &mut [MaskAnchor], segment: usize, t: f32, by: [f32; 2]) {
    let n = shape.len();
    if n < 2 {
        return;
    }
    let j = (segment + 1) % n;
    let t = t.clamp(0.08, 0.92);
    let u = 1.0 - t;
    let k = 1.0 / (3.0 * t * u * (u * u + t * t));
    let (w1, w2) = (k * u, k * t);
    let (mut a, mut b) = (shape[segment], shape[j]);
    a.after = [a.after[0] + w1 * by[0], a.after[1] + w1 * by[1]];
    b.before = [b.before[0] + w2 * by[0], b.before[1] + w2 * by[1]];
    let turn = |other: [f32; 2], this: [f32; 2]| {
        let (l, tl) = (other[0].hypot(other[1]), this[0].hypot(this[1]));
        if l == 0.0 || tl == 0.0 {
            other
        } else {
            [-this[0] / tl * l, -this[1] / tl * l]
        }
    };
    if !a.split {
        a.before = turn(a.before, a.after);
    }
    if !b.split {
        b.after = turn(b.after, b.before);
    }
    shape[segment] = a;
    shape[j] = b;
}

/// A move `d` (media pixels), held along its longer axis with Shift.
fn axis(d: (f32, f32), mods: egui::Modifiers) -> (f32, f32) {
    match mods.shift {
        true if d.0.abs() > d.1.abs() => (d.0, 0.0),
        true => (0.0, d.1),
        false => d,
    }
}

/// The topmost path whose inside has `p` (media pixels).
fn path_at(shapes: &[Vec<MaskAnchor>], p: (f32, f32)) -> Option<usize> {
    (0..shapes.len()).rev().find(|&i| {
        let poly = mask::polygon(&shapes[i]);
        let n = poly.len();
        if n < 3 {
            return false;
        }
        let (x, y) = (p.0 as f64, p.1 as f64);
        let mut inside = false;
        for k in 0..n {
            let (a, b) = (poly[k], poly[(k + 1) % n]);
            if (a[1] > y) != (b[1] > y) && x < a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
                inside = !inside;
            }
        }
        inside
    })
}

/// The shape a Rectangle or Oval drag draws from `a` to `b` (media
/// pixels): Shift a square or a circle, Ctrl from its middle, Alt a
/// rectangle's corners rounded or half an oval.
fn drawn_shape(tool: MaskTool, a: (f32, f32), b: (f32, f32), mods: egui::Modifiers) -> MaskPath {
    let (mut dx, mut dy) = (b.0 - a.0, b.1 - a.1);
    if mods.shift {
        let s = dx.abs().max(dy.abs());
        (dx, dy) = (s * dx.signum(), s * dy.signum());
    }
    let (p0, p1) = if mods.ctrl {
        ([a.0 - dx, a.1 - dy], [a.0 + dx, a.1 + dy])
    } else {
        ([a.0, a.1], [a.0 + dx, a.1 + dy])
    };
    match (tool, mods.alt) {
        (MaskTool::Oval, false) => MaskPath::oval(p0, p1),
        (MaskTool::Oval, true) => MaskPath::half_oval(p0, p1),
        (_, true) => MaskPath::rounded(p0, p1, 0.2),
        _ => MaskPath::rectangle(p0, p1),
    }
}

/// A path drawn dashed, white over black (dimmer when it does not mask;
/// open while it is being drawn).
fn dashed(painter: &Painter, sp: &Space, anchors: &[MaskAnchor], dim: bool, open: bool) {
    if anchors.len() < 2 {
        return;
    }
    let mut pts: Vec<Pos2> = mask::polygon(anchors)
        .into_iter()
        .map(|q| sp.screen((q[0] as f32, q[1] as f32)))
        .collect();
    if open {
        // Its last segment, back to the first anchor, not drawn yet: the
        // polygon's points from the last anchor on go, but that one.
        let n = anchors.len();
        let straight = anchors[n - 1].after == [0.0; 2] && anchors[0].before == [0.0; 2];
        let last = if straight { 1 } else { 24 };
        pts.truncate((pts.len() + 1).saturating_sub(last));
    } else if let Some(&first) = pts.first() {
        pts.push(first);
    }
    let alpha = if dim { 110 } else { 255 };
    painter.add(Shape::line(
        pts.clone(),
        Stroke::new(1.0, Color32::from_black_alpha(alpha)),
    ));
    painter.extend(Shape::dashed_line(
        &pts,
        Stroke::new(1.0, Color32::from_white_alpha(alpha)),
        4.0,
        4.0,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A click of the Anchor Creation tool on an anchor: curved, it becomes
    /// a corner; a corner, curved again along the line through its
    /// neighbors, a third of the way to each.
    #[test]
    fn a_click_on_an_anchor_makes_it_a_corner_and_curved_again() {
        let mut s = vec![
            MaskAnchor::corner([0.0, 0.0]),
            MaskAnchor::corner([60.0, 30.0]),
            MaskAnchor::corner([120.0, 0.0]),
        ];
        toggle_corner(&mut s, 1);
        let third = 60f32.hypot(30.0) / 3.0;
        let near =
            |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3;
        assert!(
            near(s[1].before, [-third, 0.0]) && near(s[1].after, [third, 0.0]),
            "{:?}",
            s[1]
        );
        toggle_corner(&mut s, 1);
        assert_eq!(s[1], MaskAnchor::corner([60.0, 30.0]));
    }

    /// A segment bent from one of its points: that point goes where it is
    /// pulled; a curved anchor's other tangent turns along (the curve stays
    /// smooth there), a corner's other side stays straight.
    #[test]
    fn a_bent_segment_goes_through_where_it_is_pulled() {
        let mut s = vec![
            MaskAnchor {
                at: [0.0, 0.0],
                before: [-10.0, 0.0],
                after: [10.0, 0.0],
                split: false,
            },
            MaskAnchor::corner([100.0, 0.0]),
            MaskAnchor::corner([50.0, 80.0]),
        ];
        let was = mask::segment_point(&s, 0, 0.5);
        bend(&mut s, 0, 0.5, [0.0, -30.0]);
        let now = mask::segment_point(&s, 0, 0.5);
        assert!(
            (now[0] - was[0]).abs() < 1e-3 && (now[1] - (was[1] - 30.0)).abs() < 1e-3,
            "{was:?} {now:?}"
        );
        let (b, a) = (s[0].before, s[0].after);
        assert!((b[0] * a[1] - b[1] * a[0]).abs() < 1e-3 && b[0] * a[0] + b[1] * a[1] < 0.0);
        assert_eq!(s[1].after, [0.0; 2]);
    }
}
