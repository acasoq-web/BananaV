//! The Color Grading panel's Color Curves, as VEGAS 2026's Help describes
//! them ("Color Curves") and VEGAS 23 lays them out: Brightness, Red,
//! Green and Blue on one graph filling the column — only Brightness at
//! first, Select Curves (on the column's title row) turning the others
//! on —, or, the column wide enough (the pinned curves' left border
//! dragged), side by side, each on its own graph. A graph zooms (its
//! buttons in its corner, the mouse wheel, Num + and Num −; Num 5 resets
//! it) and pans when zoomed (Num 8, 6, 4 and 2). Every point has its
//! handles, as VEGAS's: a curve starts straight, its two ends' handles a
//! quarter along it; a double-click adds a point where it is, its handles
//! along the line between its neighbours and theirs turned toward it
//! ([`add_dot`]); a double-click on a point makes its handles level
//! ([`flatten`]). Points drag (the ends: the shadows and the highlights);
//! Page Up and Page Down pick the next or the previous point, the arrows
//! move it, Shift with them its left tangent and Ctrl+Shift its right
//! one; the menu locks or unlocks a point's tangents, resets it to the
//! diagonal, smooths the curve, shows the picked point's tangents or
//! all, locks the points' axis (up and down only) or to the grid, and
//! deletes a point; the yellow handles shape the curve without moving
//! it.

use bv_core::curve::{self, CurvePoint, Handles};

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
    /// The menu open, and the point it is for.
    menu: Option<Option<(usize, usize)>>,
    /// Every point's tangents shown, else the picked point's.
    pub all_tangents: bool,
    /// Points move up and down only (Lock axis), or onto the grid's lines
    /// near them (Lock to grid).
    pub lock_axis: bool,
    pub lock_grid: bool,
}

impl Default for GradingCurves {
    fn default() -> GradingCurves {
        GradingCurves {
            active: [true, false, false, false],
            picked: None,
            views: [View::default(); 5],
            drag: None,
            menu: None,
            all_tangents: false,
            lock_axis: false,
            lock_grid: false,
        }
    }
}

/// Curve `c` as edited: no points stand for the straight line VEGAS's
/// curves start as — its ends' handles a quarter along it.
fn points(c: &[CurvePoint]) -> Vec<CurvePoint> {
    if c.is_empty() {
        let h = ([-0.25, -0.25], [0.25, 0.25]);
        vec![
            CurvePoint::with(0.0, 0.0, h.0, h.1),
            CurvePoint::with(1.0, 1.0, h.0, h.1),
        ]
    } else {
        c.to_vec()
    }
}

/// Point `i`'s neighbours' x (`wrap`: a hue curve's, its ends' a turn
/// round); `None` past an end.
fn neighbours(c: &[CurvePoint], i: usize, wrap: bool) -> (Option<f64>, Option<f64>) {
    let n = c.len();
    let before = if i > 0 {
        Some(c[i - 1].x)
    } else if wrap {
        Some(c[n - 1].x - 1.0)
    } else {
        None
    };
    let after = if i + 1 < n {
        Some(c[i + 1].x)
    } else if wrap {
        Some(c[0].x + 1.0)
    } else {
        None
    };
    (before, after)
}

/// Point `i`'s handles made level, as VEGAS's double-click on a point
/// makes them: each half the way to its neighbour, at most 0.125 (`wrap`:
/// a hue curve's, its ends' neighbours a turn round).
pub(crate) fn flatten(c: &mut [CurvePoint], i: usize, wrap: bool) {
    let x = c[i].x;
    let (before, after) = neighbours(c, i, wrap);
    let half = |gap: Option<f64>| gap.map_or(0.125, |g| g.clamp(0.0, 0.25) / 2.0);
    c[i].handles = Some(Handles {
        before: [-half(before.map(|b| x - b)), 0.0],
        after: [half(after.map(|a| a - x)), 0.0],
        split: false,
    });
}

/// `c` with a point added at `at`, as VEGAS's double-click adds one: its
/// handles along the line from the point before it to the one after —
/// in, half the way to the next; out, half the way from the one before —
/// and theirs toward it, half the way. Where it went.
pub(crate) fn add_dot(c: &[CurvePoint], at: [f64; 2], wrap: bool) -> (Vec<CurvePoint>, usize) {
    let mut out = c.to_vec();
    let n = c.len();
    let i = c.partition_point(|p| p.x < at[0]);
    let prev = if i > 0 {
        Some((i - 1, [c[i - 1].x, c[i - 1].y]))
    } else if wrap && n > 0 {
        Some((n - 1, [c[n - 1].x - 1.0, c[n - 1].y]))
    } else {
        None
    };
    let next = if i < n {
        Some((i, [c[i].x, c[i].y]))
    } else if wrap && n > 0 {
        Some((0, [c[0].x + 1.0, c[0].y]))
    } else {
        None
    };
    let mut p = CurvePoint::new(at[0], at[1]);
    if let (Some((_, a)), Some((_, b))) = (prev, next)
        && b[0] - a[0] > 1e-9
    {
        let slope = (b[1] - a[1]) / (b[0] - a[0]);
        let (din, dout) = ((at[0] - b[0]) / 2.0, (at[0] - a[0]) / 2.0);
        p.handles = Some(Handles {
            before: [din, din * slope],
            after: [dout, dout * slope],
            split: false,
        });
    }
    out.insert(i, p);
    let toward = |from: [f64; 2]| [(at[0] - from[0]) / 2.0, (at[1] - from[1]) / 2.0];
    // The neighbours' places in `out`, past the point added.
    let shifted = |k: usize| if k >= i { k + 1 } else { k };
    if let Some((k, a)) = prev {
        let k = shifted(k);
        let mut h = curve::handles(&out, k);
        h.after = toward(a);
        out[k].handles = Some(h);
    }
    if let Some((k, b)) = next {
        let k = shifted(k);
        let mut h = curve::handles(&out, k);
        h.before = toward(b);
        out[k].handles = Some(h);
    }
    if out[i].handles.is_none() {
        flatten(&mut out, i, wrap);
    }
    (out, i)
}

/// Every point given smooth handles: along the line from the point
/// before it to the one after, a third of the way to each (an end's
/// level).
pub(crate) fn smooth(c: &mut [CurvePoint], wrap: bool) {
    let old = c.to_vec();
    let n = old.len() as isize;
    let y_at = |k: isize| old[k.rem_euclid(n) as usize].y;
    for i in 0..old.len() {
        let x = old[i].x;
        let (before, after) = neighbours(&old, i, wrap);
        let slope = match (before, after) {
            (Some(b), Some(a)) if a - b > 1e-9 => {
                (y_at(i as isize + 1) - y_at(i as isize - 1)) / (a - b)
            }
            _ => 0.0,
        };
        let third = |gap: f64| gap.max(0.0) / 3.0;
        let din = before.map_or(0.0, |b| third(x - b));
        let dout = after.map_or(0.0, |a| third(a - x));
        c[i].handles = Some(Handles {
            before: [-din, -din * slope],
            after: [dout, dout * slope],
            split: false,
        });
    }
}

/// `p` near one of the grid's lines (its quarters) put on it.
pub(crate) fn to_grid(p: [f64; 2]) -> [f64; 2] {
    p.map(|v| {
        let q = (v * 4.0).round() / 4.0;
        if (v - q).abs() < 0.03 { q } else { v }
    })
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
        // The picked point's handles, or every point's (an end has none
        // toward the side).
        let tangents: Vec<usize> = if view.all_tangents {
            (0..c.len()).collect()
        } else {
            picked
                .filter(|&(pk, i)| pk == k && i < c.len())
                .map(|(_, i)| i)
                .into_iter()
                .collect()
        };
        for i in tangents {
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
        let mut at = to_curve(p);
        let mut c = base.clone();
        match *grip {
            Grip::Point(i) => {
                if view.lock_grid {
                    at = to_grid(at);
                }
                if view.lock_axis {
                    at[0] = c[i].x;
                }
                move_point(&mut c, i, at)
            }
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
    // A double-click on a point makes its handles level; elsewhere it adds
    // a point there to the curve nearest it (the picked one's first).
    if resp.double_clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        if let Some((k, Grip::Point(i))) = hit(p) {
            let mut c = points(curves.get(k).map_or(&[][..], Vec::as_slice));
            flatten(&mut c, i, false);
            view.picked = Some((k, i));
            out = Some(with(curves, k, c));
        } else {
            let at = to_curve(p);
            let at = [at[0].clamp(0.0, 1.0), at[1].clamp(0.0, 1.0)];
            let nearest = order.iter().rev().copied().min_by(|&a, &b| {
                let d = |k: usize| {
                    let c = points(curves.get(k).map_or(&[][..], Vec::as_slice));
                    (curve::curve_at(&c, at[0]) - at[1]).abs()
                };
                d(a).total_cmp(&d(b))
            });
            if let Some(k) = nearest {
                let c = points(curves.get(k).map_or(&[][..], Vec::as_slice));
                let (added, i) = add_dot(&c, at, false);
                view.picked = Some((k, i));
                out = Some(with(curves, k, added));
            }
        }
    }
    // The menu: a point's (Lock Tangents, Reset, Delete) when one is under
    // the pointer; the curve's and the graph's.
    if resp.secondary_clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let point = match hit(p) {
            Some((k, Grip::Point(i))) => Some((k, i)),
            _ => view.picked,
        };
        if let Some(pk) = point {
            view.picked = Some(pk);
        }
        view.menu = Some(point);
    }
    if let Some(point) = view.menu {
        resp.context_menu(|ui| {
            let curve_of = |k: usize| points(curves.get(k).map_or(&[][..], Vec::as_slice));
            if let Some((k, i)) = point
                && i < curve_of(k).len()
            {
                let c = curve_of(k);
                let locked = !curve::handles(&c, i).split;
                if crate::menus::toggle_item(ui, tr("Lock Tangents"), locked) {
                    let mut c = c.clone();
                    crate::ofx_curves::lock_handles(&mut c, i, !locked);
                    out = Some(with(curves, k, c));
                }
                // Back on the diagonal, its handles level.
                if crate::menus::item(ui, tr("Reset")) {
                    let mut c = c.clone();
                    let x = c[i].x;
                    move_point(&mut c, i, [x, x]);
                    flatten(&mut c, i, false);
                    out = Some(with(curves, k, c));
                    ui.close();
                }
                if crate::menus::item(ui, tr("Smooth curves")) {
                    let mut c = c.clone();
                    smooth(&mut c, false);
                    out = Some(with(curves, k, c));
                    ui.close();
                }
                ui.separator();
            }
            if crate::menus::check_item(ui, tr("All tangents"), view.all_tangents, true, "") {
                view.all_tangents = true;
                ui.close();
            }
            if crate::menus::check_item(ui, tr("Selected tangents"), !view.all_tangents, true, "") {
                view.all_tangents = false;
                ui.close();
            }
            if crate::menus::toggle_item(ui, tr("Lock axis"), view.lock_axis) {
                view.lock_axis = !view.lock_axis;
                ui.close();
            }
            if crate::menus::toggle_item(ui, tr("Lock to grid"), view.lock_grid) {
                view.lock_grid = !view.lock_grid;
                ui.close();
            }
            if let Some((k, i)) = point
                && i < curve_of(k).len()
            {
                let c = curve_of(k);
                ui.separator();
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
    fn points_are_added_and_levelled_as_vegass() {
        // The straight line: a point added at (0.5, 0.7) takes handles
        // along the line between its neighbours, theirs turned to it.
        let c = points(&[]);
        let (added, i) = add_dot(&c, [0.5, 0.7], false);
        assert_eq!(i, 1);
        let h = curve::handles(&added, 1);
        assert!((h.before[0] + 0.25).abs() < 1e-9 && (h.before[1] + 0.25).abs() < 1e-9);
        assert!((h.after[0] - 0.25).abs() < 1e-9 && (h.after[1] - 0.25).abs() < 1e-9);
        let first = curve::handles(&added, 0).after;
        assert!((first[0] - 0.25).abs() < 1e-9 && (first[1] - 0.35).abs() < 1e-9);
        let last = curve::handles(&added, 2).before;
        assert!((last[0] + 0.25).abs() < 1e-9 && (last[1] + 0.15).abs() < 1e-9);
        // Level: half the way to each neighbour, at most 0.125.
        let mut c = added;
        flatten(&mut c, 1, false);
        let h = curve::handles(&c, 1);
        assert_eq!((h.before, h.after), ([-0.125, 0.0], [0.125, 0.0]));
        // A hue curve's single point: its neighbours a turn round.
        let mut one = vec![CurvePoint::new(0.3, 0.5)];
        flatten(&mut one, 0, true);
        assert_eq!(curve::handles(&one, 0).after, [0.125, 0.0]);
        assert_eq!(to_grid([0.26, 0.6]), [0.25, 0.6]);
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
