//! The Color Grading panel's Color Curves, as VEGAS 2026's Help describes
//! them ("Color Curves") and VEGAS 23 lays them out: Brightness, Red,
//! Green and Blue on one graph filling the column — only Brightness at
//! first, Select Curves (on the column's title row) turning the others
//! on —, or, the column wide enough (the pinned curves' left border
//! dragged), side by side, each on its own graph. A graph zooms (its
//! buttons in its corner, the mouse wheel, Num + and Num −; Num 5 resets
//! it) and pans when zoomed (Num 8, 6, 4 and 2). A double-click on a curve adds a point; points drag (the
//! ends: the shadows and the highlights); Page Up and Page Down pick the
//! next or the previous point, the arrows move it, Shift with them its
//! left tangent and Ctrl+Shift its right one; a point's menu locks or
//! unlocks its tangents and deletes it; the yellow handles of the point
//! picked shape the curve without moving it.

use bv_core::curve::{self, CurvePoint};

use crate::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use crate::i18n::tr;
use crate::icons::Icon;
use crate::theme;

/// A graph's least width and height, the gap between graphs side by side
/// and how wide the column must be for the four of them.
const GRAPH_MIN: f32 = 140.0;
const GRAPH_LOW: f32 = 100.0;
const GAP: f32 = 8.0;
pub(crate) const SIDE_BY_SIDE: f32 = 4.0 * GRAPH_MIN + 3.0 * GAP;
/// How near the pointer takes a point or a handle.
const REACH: f32 = 6.0;
/// The deepest zoom, and a zoom button's (or a wheel notch's) step.
const ZOOM_MAX: f64 = 16.0;
const ZOOM_STEP: f64 = 1.25;

/// The curves' colors: Brightness, Red, Green, Blue.
const COLORS: [Color32; 4] = [
    Color32::from_rgb(0xE6, 0xE6, 0xE6),
    Color32::from_rgb(0xE8, 0x4A, 0x4A),
    Color32::from_rgb(0x4C, 0xC8, 0x6A),
    Color32::from_rgb(0x4A, 0x8C, 0xF0),
];
const HANDLE: Color32 = Color32::from_rgb(0xF0, 0xC8, 0x30);

/// A graph's view: how far it is zoomed in and the middle it shows (on
/// the curves' scales).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct View {
    pub zoom: f64,
    pub center: [f64; 2],
}

impl Default for View {
    fn default() -> View {
        View {
            zoom: 1.0,
            center: [0.5, 0.5],
        }
    }
}

impl View {
    /// The span shown across and up.
    fn span(&self) -> f64 {
        1.0 / self.zoom
    }

    /// Kept over the curves' square (the middle at least half a span
    /// from its sides).
    fn clamped(mut self) -> View {
        self.zoom = self.zoom.clamp(1.0, ZOOM_MAX);
        let h = self.span() / 2.0;
        for c in &mut self.center {
            *c = c.clamp(h, 1.0 - h);
        }
        self
    }

    /// Zoomed by `factor` about `at`, which stays where it is.
    pub(crate) fn zoomed(self, factor: f64, at: [f64; 2]) -> View {
        let old = self.span();
        let zoom = (self.zoom * factor).clamp(1.0, ZOOM_MAX);
        let k = (1.0 / zoom) / old;
        View {
            zoom,
            center: [
                at[0] + (self.center[0] - at[0]) * k,
                at[1] + (self.center[1] - at[1]) * k,
            ],
        }
        .clamped()
    }

    /// Panned by a tenth of its span, `dx` right and `dy` up.
    pub(crate) fn panned(self, dx: f64, dy: f64) -> View {
        let d = self.span() / 10.0;
        View {
            center: [self.center[0] + dx * d, self.center[1] + dy * d],
            ..self
        }
        .clamped()
    }
}

/// A point of a curve, or one of its handles.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grip {
    Point(usize),
    Before(usize),
    After(usize),
}

/// The editor's state.
#[derive(Clone, Debug)]
pub struct GradingCurves {
    /// The curves on the shared graph (Select Curves): Brightness at
    /// first.
    pub active: [bool; 4],
    /// The curve and the point picked.
    pub picked: Option<(usize, usize)>,
    /// The shared graph's view, then each graph's side by side.
    pub(crate) views: [View; 5],
    /// A drag under way: its curve, what it took and the curve as it was.
    drag: Option<(usize, Grip, Vec<CurvePoint>)>,
    /// The point a menu is open for.
    menu: Option<(usize, usize)>,
}

impl Default for GradingCurves {
    fn default() -> GradingCurves {
        GradingCurves {
            active: [true, false, false, false],
            picked: None,
            views: [View::default(); 5],
            drag: None,
            menu: None,
        }
    }
}

/// Curve `c` as edited: no points stand for the straight line.
fn points(c: &[CurvePoint]) -> Vec<CurvePoint> {
    if c.is_empty() {
        vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 1.0)]
    } else {
        c.to_vec()
    }
}

/// The curves with curve `k` given `c`.
fn with(curves: &[Vec<CurvePoint>], k: usize, c: Vec<CurvePoint>) -> Vec<Vec<CurvePoint>> {
    let mut out = curves.to_vec();
    if out.len() <= k {
        out.resize(k + 1, Vec::new());
    }
    out[k] = c;
    out
}

/// Point `i` of `c` moved to `to`: between its neighbours, the ends at
/// the curves' sides' reach, its value 0–1.
pub(crate) fn move_point(c: &mut [CurvePoint], i: usize, to: [f64; 2]) {
    let n = c.len();
    let lo = if i > 0 { c[i - 1].x + 1e-3 } else { 0.0 };
    let hi = if i + 1 < n { c[i + 1].x - 1e-3 } else { 1.0 };
    c[i].x = to[0].clamp(lo, hi.max(lo));
    c[i].y = to[1].clamp(0.0, 1.0);
}

/// The color curves editor: Select Curves on the title row `bar` (from
/// its right), then, under it, one shared graph or, wide enough, four
/// side by side, filling what is left. The curves changed.
pub fn editor(
    ui: &mut Ui,
    view: &mut GradingCurves,
    id: egui::Id,
    curves: &[Vec<CurvePoint>],
    labels: &[String],
    bar: &mut Ui,
) -> Option<Vec<Vec<CurvePoint>>> {
    let n = curves.len().clamp(1, 4);
    let side = n == 4 && ui.available_width() >= SIDE_BY_SIDE;
    if let Some((k, _)) = view.picked
        && (k >= n || (!side && !view.active[k]))
    {
        view.picked = None;
    }
    if !side {
        let (r, b) = bar.allocate_exact_size(vec2(30.0, 22.0), Sense::click());
        if b.hovered() {
            bar.painter().rect_filled(r, 3.0, theme::hover());
        }
        let c = theme::text();
        crate::icons::paint(
            bar.painter(),
            Rect::from_center_size(r.center() - vec2(5.0, 0.0), vec2(14.0, 14.0)),
            Icon::Grid,
            c,
        );
        let m = pos2(r.right() - 6.0, r.center().y);
        bar.painter().add(egui::Shape::convex_polygon(
            vec![
                m + vec2(-3.0, -1.5),
                m + vec2(3.0, -1.5),
                m + vec2(0.0, 2.0),
            ],
            c,
            Stroke::NONE,
        ));
        let b = b.on_hover_text(tr("Select Curves"));
        egui::Popup::menu(&b)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                for k in 0..n {
                    let name = labels.get(k).cloned().unwrap_or_default();
                    crate::widgets::checkbox(ui, &mut view.active[k], &name);
                }
            });
    }
    let h = (ui.available_height() - 4.0).max(GRAPH_LOW);
    let mut out = None;
    if side {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            let w = ((ui.available_width() - 3.0 * GAP) / 4.0).max(GRAPH_MIN);
            for k in 0..4 {
                let name = labels.get(k).cloned().unwrap_or_default();
                let size = vec2(w, h);
                if let Some(c) = graph(ui, view, id.with(k), 1 + k, &[k], curves, &name, size) {
                    out = Some(c);
                }
            }
        });
    } else {
        let shown: Vec<usize> = (0..n).filter(|&k| view.active[k]).collect();
        let size = vec2(ui.available_width().max(GRAPH_MIN), h);
        out = graph(ui, view, id, 0, &shown, curves, "", size);
    }
    out
}

/// A graph's zoom buttons, in its lower right corner as VEGAS's: Zoom In,
/// Reset View and Zoom Out, one over the other.
fn zoom_buttons(ui: &mut Ui, view: &mut GradingCurves, id: egui::Id, g: usize, rect: Rect) {
    let v = view.views[g];
    let mid = v.center;
    let tips = [
        tr("Zoom In (Num +)"),
        tr("Reset View (Num 5)"),
        tr("Zoom Out (Num −)"),
    ];
    for (k, tip) in tips.into_iter().enumerate() {
        let r = Rect::from_center_size(
            pos2(
                rect.right() - 14.0,
                rect.bottom() - 14.0 - (2 - k) as f32 * 24.0,
            ),
            vec2(20.0, 20.0),
        );
        let b = ui.interact(r, id.with(("zoom", k)), Sense::click());
        let c = if b.hovered() {
            theme::text_strong()
        } else {
            theme::text_dim()
        };
        let p = ui.painter();
        if b.hovered() {
            p.rect_filled(r, 3.0, theme::hover());
        }
        let m = r.center();
        if k == 1 {
            // A dotted frame: the whole curves again.
            for (a, d) in [
                (vec2(-5.0, -5.0), vec2(3.0, 0.0)),
                (vec2(-5.0, -5.0), vec2(0.0, 3.0)),
                (vec2(5.0, -5.0), vec2(-3.0, 0.0)),
                (vec2(5.0, -5.0), vec2(0.0, 3.0)),
                (vec2(-5.0, 5.0), vec2(3.0, 0.0)),
                (vec2(-5.0, 5.0), vec2(0.0, -3.0)),
                (vec2(5.0, 5.0), vec2(-3.0, 0.0)),
                (vec2(5.0, 5.0), vec2(0.0, -3.0)),
            ] {
                p.line_segment([m + a, m + a + d], Stroke::new(1.0, c));
            }
        } else {
            // A magnifier with + or −.
            let lens = m + vec2(-1.5, -1.5);
            p.circle_stroke(lens, 5.0, Stroke::new(1.2, c));
            p.line_segment(
                [lens + vec2(3.6, 3.6), m + vec2(6.0, 6.0)],
                Stroke::new(1.6, c),
            );
            p.line_segment(
                [lens - vec2(2.5, 0.0), lens + vec2(2.5, 0.0)],
                Stroke::new(1.0, c),
            );
            if k == 0 {
                p.line_segment(
                    [lens - vec2(0.0, 2.5), lens + vec2(0.0, 2.5)],
                    Stroke::new(1.0, c),
                );
            }
        }
        if b.on_hover_text(tip).clicked() {
            view.views[g] = match k {
                0 => v.zoomed(ZOOM_STEP, mid),
                1 => View::default(),
                _ => v.zoomed(1.0 / ZOOM_STEP, mid),
            };
        }
    }
}

/// A graph `size` big (`g`: its view) of curves `shown`, `name` in its
/// corner; the curves changed.
#[allow(clippy::too_many_arguments)]
fn graph(
    ui: &mut Ui,
    view: &mut GradingCurves,
    id: egui::Id,
    g: usize,
    shown: &[usize],
    curves: &[Vec<CurvePoint>],
    name: &str,
    size: egui::Vec2,
) -> Option<Vec<Vec<CurvePoint>>> {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let resp = ui.interact(rect, id.with("graph"), Sense::click_and_drag());
    let area = rect.shrink(8.0);
    let v = view.views[g];
    let span = v.span();
    let (x0, y0) = (v.center[0] - span / 2.0, v.center[1] - span / 2.0);
    let to_screen = |p: [f64; 2]| {
        pos2(
            area.left() + ((p[0] - x0) / span) as f32 * area.width(),
            area.bottom() - ((p[1] - y0) / span) as f32 * area.height(),
        )
    };
    let to_curve = |s: Pos2| {
        [
            x0 + f64::from((s.x - area.left()) / area.width()) * span,
            y0 + f64::from((area.bottom() - s.y) / area.height()) * span,
        ]
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, theme::panel_dark());
    // The grid: quarters of the curves' square, and its diagonal.
    let grid = Stroke::new(1.0, theme::text_dim().gamma_multiply(0.25));
    for q in 0..=4 {
        let t = f64::from(q) / 4.0;
        painter.line_segment([to_screen([t, 0.0]), to_screen([t, 1.0])], grid);
        painter.line_segment([to_screen([0.0, t]), to_screen([1.0, t])], grid);
    }
    painter.line_segment([to_screen([0.0, 0.0]), to_screen([1.0, 1.0])], grid);
    if !name.is_empty() {
        painter.text(
            rect.left_top() + vec2(6.0, 4.0),
            egui::Align2::LEFT_TOP,
            name,
            egui::FontId::proportional(11.0),
            theme::text_dim(),
        );
    }
    let picked = view.picked.filter(|(k, _)| shown.contains(k));
    // The curves, the picked one's last (on top).
    let mut order: Vec<usize> = shown.to_vec();
    if let Some((k, _)) = picked {
        order.retain(|&x| x != k);
        order.push(k);
    }
    let (vx0, vx1) = (x0.max(0.0), (x0 + span).min(1.0));
    for &k in &order {
        let c = points(curves.get(k).map_or(&[][..], Vec::as_slice));
        let color = COLORS[k.min(3)];
        let line: Vec<Pos2> = curve::trace(&c, vx0, vx1, 160)
            .into_iter()
            .map(|p| to_screen([p[0], p[1].clamp(0.0, 1.0)]))
            .collect();
        painter.add(egui::Shape::line(line, Stroke::new(1.6, color)));
        for (i, p) in c.iter().enumerate() {
            let at = to_screen([p.x, p.y]);
            let on = picked == Some((k, i));
            let r = Rect::from_center_size(at, vec2(7.0, 7.0));
            painter.rect_filled(r, 1.0, if on { color } else { theme::panel_dark() });
            painter.rect_stroke(r, 1.0, Stroke::new(1.2, color), egui::StrokeKind::Inside);
        }
        // The picked point's handles (an end has none toward the side).
        if let Some((pk, i)) = picked
            && pk == k
            && i < c.len()
        {
            let h = curve::handles(&c, i);
            let base = [c[i].x, c[i].y];
            let at = to_screen(base);
            for (has, d) in [(i > 0, h.before), (i + 1 < c.len(), h.after)] {
                if has {
                    let end = to_screen([base[0] + d[0], base[1] + d[1]]);
                    painter.line_segment([at, end], Stroke::new(1.0, HANDLE));
                    painter.circle_filled(end, 3.5, HANDLE);
                }
            }
        }
    }
    // What is under the pointer: the picked point's handles, then the
    // points (the picked curve's first).
    let hit = |at: Pos2| -> Option<(usize, Grip)> {
        if let Some((k, i)) = picked {
            let c = points(curves.get(k).map_or(&[][..], Vec::as_slice));
            if i < c.len() {
                let h = curve::handles(&c, i);
                let base = [c[i].x, c[i].y];
                if i + 1 < c.len()
                    && to_screen([base[0] + h.after[0], base[1] + h.after[1]]).distance(at) <= REACH
                {
                    return Some((k, Grip::After(i)));
                }
                if i > 0
                    && to_screen([base[0] + h.before[0], base[1] + h.before[1]]).distance(at)
                        <= REACH
                {
                    return Some((k, Grip::Before(i)));
                }
            }
        }
        order.iter().rev().find_map(|&k| {
            let c = points(curves.get(k).map_or(&[][..], Vec::as_slice));
            c.iter()
                .position(|p| to_screen([p.x, p.y]).distance(at) <= REACH)
                .map(|i| (k, Grip::Point(i)))
        })
    };
    let mut out = None;
    if resp.clicked() || resp.drag_started() {
        resp.request_focus();
    }
    // Dragging a point or a handle.
    if resp.drag_started() {
        let at = ui.input(|i| i.pointer.press_origin()).unwrap_or_default();
        if let Some((k, grip)) = hit(at) {
            if let Grip::Point(i) = grip {
                view.picked = Some((k, i));
            }
            let c = points(curves.get(k).map_or(&[][..], Vec::as_slice));
            view.drag = Some((k, grip, c));
        }
    }
    if resp.dragged()
        && let (Some((k, grip, base)), Some(p)) = (&view.drag, resp.interact_pointer_pos())
    {
        let at = to_curve(p);
        let mut c = base.clone();
        match *grip {
            Grip::Point(i) => move_point(&mut c, i, at),
            Grip::Before(i) => crate::ofx_curves::move_handle(&mut c, i, false, at),
            Grip::After(i) => crate::ofx_curves::move_handle(&mut c, i, true, at),
        }
        out = Some(with(curves, *k, c));
    }
    if resp.drag_stopped() {
        view.drag = None;
    }
    // A click picks a point.
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        view.picked = match hit(p) {
            Some((k, Grip::Point(i))) => Some((k, i)),
            Some(_) => view.picked,
            None => None,
        };
    }
    // A double-click adds a point to the curve nearest it (the picked
    // one's first).
    if resp.double_clicked()
        && let Some(p) = resp.interact_pointer_pos()
        && !matches!(hit(p), Some((_, Grip::Point(_))))
    {
        let at = to_curve(p);
        let x = at[0].clamp(0.0, 1.0);
        let nearest = order.iter().rev().copied().min_by(|&a, &b| {
            let d = |k: usize| {
                let c = points(curves.get(k).map_or(&[][..], Vec::as_slice));
                (curve::curve_at(&c, x) - at[1]).abs()
            };
            d(a).total_cmp(&d(b))
        });
        if let Some(k) = nearest {
            let c = points(curves.get(k).map_or(&[][..], Vec::as_slice));
            let added = crate::ofx_curves::add_point(&c, x);
            view.picked = added
                .iter()
                .position(|q| (q.x - x).abs() < 1e-9)
                .map(|i| (k, i));
            out = Some(with(curves, k, added));
        }
    }
    // A point's menu: Lock Tangents, Delete.
    if resp.secondary_clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        view.menu = match hit(p) {
            Some((k, Grip::Point(i))) => {
                view.picked = Some((k, i));
                Some((k, i))
            }
            _ => None,
        };
    }
    if let Some((k, i)) = view.menu {
        let c = points(curves.get(k).map_or(&[][..], Vec::as_slice));
        resp.context_menu(|ui| {
            if i >= c.len() {
                ui.close();
                return;
            }
            let locked = !curve::handles(&c, i).split;
            if crate::menus::toggle_item(ui, tr("Lock Tangents"), locked) {
                let mut c = c.clone();
                crate::ofx_curves::lock_handles(&mut c, i, !locked);
                out = Some(with(curves, k, c));
            }
            let end = i == 0 || i + 1 == c.len();
            if ui
                .add_enabled(!end, egui::Button::new(tr("Delete")))
                .clicked()
            {
                let mut c = c.clone();
                c.remove(i);
                view.picked = None;
                out = Some(with(curves, k, c));
                ui.close();
            }
        });
    }
    // The mouse wheel zooms about the pointer.
    if resp.hovered()
        && let Some(p) = resp.hover_pos()
    {
        let (notches, _) = ui.input(|i| crate::widgets::wheel_notches(&i.events));
        if notches != 0.0 {
            view.views[g] = v.zoomed(ZOOM_STEP.powf(f64::from(notches)), to_curve(p));
            ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
        }
    }
    // The keys, while the graph has the focus.
    if resp.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                resp.id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default()
                },
            );
        });
        if let Some(c) = keys(ui, view, g, curves) {
            out = Some(c);
        }
    }
    zoom_buttons(ui, view, id, g, rect);
    out
}

/// The keys of a graph with the focus: Num + and −, Num 5 and the pan
/// keys; Page Up and Down; the arrows (Shift: the left tangent,
/// Ctrl+Shift: the right one; Ctrl alone a fifth as far); Delete.
fn keys(
    ui: &Ui,
    view: &mut GradingCurves,
    g: usize,
    curves: &[Vec<CurvePoint>],
) -> Option<Vec<Vec<CurvePoint>>> {
    use egui::Key;
    let (pressed, m) = ui.input(|i| {
        let k = |key| i.key_pressed(key);
        (
            [
                k(Key::Plus) || k(Key::Equals),
                k(Key::Minus),
                k(Key::Num5),
                k(Key::Num8),
                k(Key::Num2),
                k(Key::Num4),
                k(Key::Num6),
                k(Key::PageUp),
                k(Key::PageDown),
                k(Key::ArrowLeft),
                k(Key::ArrowRight),
                k(Key::ArrowUp),
                k(Key::ArrowDown),
                k(Key::Delete),
            ],
            i.modifiers,
        )
    });
    let [
        plus,
        minus,
        reset,
        up8,
        down2,
        left4,
        right6,
        page_up,
        page_down,
        al,
        ar,
        au,
        ad,
        del,
    ] = pressed;
    let v = view.views[g];
    if plus {
        view.views[g] = v.zoomed(ZOOM_STEP, v.center);
    }
    if minus {
        view.views[g] = v.zoomed(1.0 / ZOOM_STEP, v.center);
    }
    if reset {
        view.views[g] = View::default();
    }
    let pan = |a: bool, b: bool| f64::from(u8::from(b)) - f64::from(u8::from(a));
    let (px, py) = (pan(left4, right6), pan(down2, up8));
    if px != 0.0 || py != 0.0 {
        view.views[g] = view.views[g].panned(px, py);
    }
    let (k, i) = view.picked?;
    let c = points(curves.get(k).map_or(&[][..], Vec::as_slice));
    if i >= c.len() {
        return None;
    }
    // The next or the previous point.
    if page_down || page_up {
        let next = if page_down {
            (i + 1).min(c.len() - 1)
        } else {
            i.saturating_sub(1)
        };
        view.picked = Some((k, next));
        return None;
    }
    if del && i > 0 && i + 1 < c.len() {
        let mut c = c;
        c.remove(i);
        view.picked = None;
        return Some(with(curves, k, c));
    }
    let (dx, dy) = (pan(al, ar), pan(ad, au));
    if dx == 0.0 && dy == 0.0 {
        return None;
    }
    let fine = if m.command && !m.shift { 0.2 } else { 1.0 };
    let mut c = c;
    if m.shift {
        // A tangent: up and down tilt it, left and right lengthen or
        // shorten it.
        let after = m.command;
        if (after && i + 1 >= c.len()) || (!after && i == 0) {
            return None;
        }
        let h = curve::handles(&c, i);
        let d = if after { h.after } else { h.before };
        let grow = 1.0 + 0.1 * dx * if after { 1.0 } else { -1.0 };
        let to = [c[i].x + d[0] * grow, c[i].y + d[1] * grow + 0.01 * dy];
        crate::ofx_curves::move_handle(&mut c, i, after, to);
    } else {
        let to = [c[i].x + 0.01 * fine * dx, c[i].y + 0.01 * fine * dy];
        move_point(&mut c, i, to);
    }
    Some(with(curves, k, c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_view_zooms_about_a_point_and_pans_inside_the_square() {
        let v = View::default();
        let z = v.zoomed(2.0, [0.25, 0.25]);
        assert_eq!(z.zoom, 2.0);
        // The point zoomed about stays where it was on the graph.
        let on = |v: View, p: f64| (p - (v.center[0] - v.span() / 2.0)) / v.span();
        assert!((on(v, 0.25) - on(z, 0.25)).abs() < 1e-9);
        // Out to the whole square, never past it.
        let back = z.zoomed(0.1, [0.9, 0.9]);
        assert_eq!(back, View::default());
        let far = z.panned(100.0, -100.0);
        assert!((far.center[0] - 0.75).abs() < 1e-9 && (far.center[1] - 0.25).abs() < 1e-9);
        assert_eq!(View::default().zoomed(1000.0, [0.5, 0.5]).zoom, ZOOM_MAX);
    }

    #[test]
    fn points_stay_between_their_neighbours() {
        let mut c = points(&[]);
        assert_eq!(c.len(), 2);
        c.insert(1, CurvePoint::new(0.5, 0.5));
        move_point(&mut c, 1, [2.0, -1.0]);
        assert!(c[1].x < 1.0 && c[1].y == 0.0);
        move_point(&mut c, 0, [0.3, 0.2]);
        assert!(c[0].x <= c[1].x && c[0].y == 0.2);
        let all = with(&[], 2, c.clone());
        assert_eq!(all.len(), 3);
        assert_eq!(all[2], c);
    }
}
