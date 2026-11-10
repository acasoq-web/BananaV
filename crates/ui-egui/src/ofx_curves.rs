//! A parametric parameter's curves (OpenFX), edited as VEGAS's Color
//! Curves edits its own: on the left the curve picked (Channel: those most
//! of them share, together — "RGB", a curve of its own left as it is — or
//! one), whether the others show, and buttons that make the others like
//! one; on the right the curves over a white box, the picked one's points
//! (blue) and handles (grey), and under it the bar of what the curves make
//! of a ramp from black to white.
//!
//! A double-click on the curve adds a point (the curve keeps its shape).
//! A click picks a point — with Ctrl one more or one less, with Shift the
//! run from the one clicked last — and the points picked are dragged
//! together; handles are dragged one at a time. A point's menu deletes the
//! points picked, a handle's locks or unlocks their handles in line and
//! puts them back level; the box's menu creates a point where it was
//! clicked and makes all the curves like one. Delete deletes. The
//! handles are BananaV's (OpenFX's curves have points only): the plug-in
//! reads the curve through the host, handles included.

use std::collections::BTreeSet;

use bv_core::curve::{self, CurvePoint, Handles};
use bv_ofx::ParamInfo;

use crate::cursors::{self, Cursor};
use crate::egui::{self, Color32, CursorIcon, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use crate::i18n::tr;
use crate::menus;
use crate::theme;

/// How a point of a plug-in's graph — a curve's point or handle, a
/// keyframe, an equalizer's band — is taken by the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Grab {
    /// Under it, to click or drag.
    Over,
    /// Dragged across only (a keyframe along the time).
    Across,
    /// Dragged both ways.
    Free,
}

impl Grab {
    /// Its pointer, as VEGAS's plug-ins' curves: their pointing hand over a
    /// point (as on its keyframe rows); dragged, the system's ↔ across
    /// only and its four arrows both ways.
    pub(crate) fn show(self, ctx: &egui::Context) {
        match self {
            Grab::Over => cursors::set(ctx, Cursor::HandPoint),
            Grab::Across => cursors::system(ctx, CursorIcon::ResizeHorizontal),
            Grab::Free => cursors::system(ctx, CursorIcon::Move),
        }
    }
}

/// The curves' pointer: a point or a handle dragged goes both ways; else
/// the hand over one.
fn curve_grab(dragging: bool, over: bool) -> Option<Grab> {
    if dragging {
        Some(Grab::Free)
    } else {
        over.then_some(Grab::Over)
    }
}

/// The curves' box, the white margin around it, the gap down to the bar
/// and the bar.
const BOX: Vec2 = Vec2::new(300.0, 245.0);
const MARGIN: f32 = 11.0;
const GAP: f32 = 10.0;
const BAR: f32 = 18.0;
/// The left column's width.
const PANEL: f32 = 128.0;
/// A point's square, a handle's, and how near the pointer takes them.
const POINT: f32 = 7.0;
const HANDLE: f32 = 5.0;
const REACH: f32 = 6.0;

const BLUE: Color32 = Color32::from_rgb(0x00, 0x78, 0xD7);
const GREY: Color32 = Color32::from_rgb(0xA0, 0xA0, 0xA0);
const BORDER: Color32 = Color32::from_rgb(0xA0, 0xA0, 0xA0);
const INNER: Color32 = Color32::from_rgb(0xB4, 0xB4, 0xB4);

/// The editor's state, for each parametric parameter shown.
#[derive(Clone, Debug)]
pub struct CurvesView {
    /// The curve edited: 0 all of them together, then each.
    pub channel: usize,
    /// The other curves show too.
    pub show_all: bool,
    /// The points picked (their indexes in the edited curve), and the one
    /// clicked last: Shift picks the run from it.
    picked: BTreeSet<usize>,
    anchor: Option<usize>,
    drag: Option<Drag>,
    /// What the menu open is for.
    menu: Option<MenuFor>,
}

impl Default for CurvesView {
    fn default() -> CurvesView {
        CurvesView {
            channel: 0,
            show_all: true,
            picked: BTreeSet::new(),
            anchor: None,
            drag: None,
            menu: None,
        }
    }
}

/// A point of the edited curve, or one of its handles.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grip {
    Point(usize),
    Before(usize),
    After(usize),
}

/// A drag under way: what it took, where the pointer was then (on the
/// curves' scales) and the curve as it was.
#[derive(Clone, Debug)]
struct Drag {
    grip: Grip,
    from: [f64; 2],
    base: Vec<CurvePoint>,
}

/// What a menu is open for: a point or a handle, or the box away from
/// them (where it was clicked, on the curves' scales).
#[derive(Clone, Copy, Debug, PartialEq)]
enum MenuFor {
    Grip(Grip),
    Box([f64; 2]),
}

/// Picks point `i` as a click with `mods` does: alone; with Ctrl, in or out
/// of those picked; with Shift, the run from the one clicked last (with
/// Ctrl too, added to them).
fn pick(view: &mut CurvesView, i: usize, mods: egui::Modifiers) {
    if mods.shift
        && let Some(a) = view.anchor
    {
        if !mods.command {
            view.picked.clear();
        }
        view.picked.extend(a.min(i)..=a.max(i));
        return;
    }
    if mods.command {
        if !view.picked.remove(&i) {
            view.picked.insert(i);
        }
    } else {
        view.picked = BTreeSet::from([i]);
    }
    view.anchor = Some(i);
}

/// The points an action on point `i` goes to: those picked when it is one
/// of them, or it alone.
fn targets(view: &CurvesView, i: usize) -> BTreeSet<usize> {
    if view.picked.contains(&i) {
        view.picked.clone()
    } else {
        BTreeSet::from([i])
    }
}

/// Picks the point of `c` at `x` (one just added) alone; its index.
fn pick_added(view: &mut CurvesView, c: &[CurvePoint], x: f64) -> Option<usize> {
    let i = c.iter().position(|q| (q.x - x).abs() < 1e-9)?;
    view.picked = BTreeSet::from([i]);
    view.anchor = Some(i);
    Some(i)
}

/// `c` without the points `gone`; `None` when fewer than two would stay.
fn without(c: &[CurvePoint], gone: &BTreeSet<usize>) -> Option<Vec<CurvePoint>> {
    let kept: Vec<CurvePoint> = c
        .iter()
        .enumerate()
        .filter(|(i, _)| !gone.contains(i))
        .map(|(_, p)| *p)
        .collect();
    (kept.len() >= 2 && kept.len() < c.len()).then_some(kept)
}

/// The curve most of `curves` are (two or more of them alike, or the only
/// one), and which are.
fn shared(curves: &[Vec<CurvePoint>]) -> Option<(Vec<usize>, Vec<CurvePoint>)> {
    if curves.len() == 1 {
        return Some((vec![0], curves[0].clone()));
    }
    let mut best: Option<Vec<usize>> = None;
    for c in curves {
        let alike: Vec<usize> = (0..curves.len()).filter(|&j| curves[j] == *c).collect();
        if alike.len() >= 2 && best.as_ref().is_none_or(|b| alike.len() > b.len()) {
            best = Some(alike);
        }
    }
    let alike = best?;
    let c = curves[alike[0]].clone();
    Some((alike, c))
}

/// The curve being edited and the curves an edit goes to: one, or — all
/// of them together — those most share, as VEGAS's: a curve of its own
/// (Warm Colors' red) keeps it.
fn edited(view: &CurvesView, curves: &[Vec<CurvePoint>]) -> Option<(Vec<usize>, Vec<CurvePoint>)> {
    match view.channel {
        0 => shared(curves),
        k => curves.get(k - 1).map(|c| (vec![k - 1], c.clone())),
    }
}

/// The colour a curve is drawn in: the plug-in's, or one of the host's.
fn colour(info: &ParamInfo, k: usize) -> Color32 {
    match info.curve_colours.get(k) {
        Some(c) => Color32::from_rgb(
            (c[0].clamp(0.0, 1.0) * 255.0) as u8,
            (c[1].clamp(0.0, 1.0) * 255.0) as u8,
            (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        ),
        None => [
            Color32::from_rgb(0x50, 0x50, 0x50),
            Color32::from_rgb(0xCC, 0x8C, 0x8C),
            Color32::from_rgb(0x80, 0xC0, 0x80),
            Color32::from_rgb(0x90, 0x90, 0xD8),
        ][k % 4],
    }
}

/// The edited curve's colour. Curves of the red, green and blue channels
/// take VEGAS's: a channel's own dark colour, two of them a dark mix of
/// theirs (green and blue a dark teal), all three a dark grey. Others take
/// their curves' colours mixed, deeper and darker.
fn dark(info: &ParamInfo, of: &[usize]) -> Color32 {
    let channels: Option<Vec<usize>> = of.iter().map(|&k| channel_of(info, k)).collect();
    if let Some(mut ch) = channels {
        ch.sort_unstable();
        ch.dedup();
        if ch.len() == of.len() {
            let c = match ch.as_slice() {
                [0] => Some([0x80, 0x00, 0x00]),
                [1] => Some([0x00, 0x6E, 0x00]),
                [2] => Some([0x00, 0x00, 0x96]),
                [0, 1] => Some([0x4B, 0x37, 0x00]),
                [1, 2] => Some([0x00, 0x4B, 0x37]),
                [0, 2] => Some([0x37, 0x00, 0x4B]),
                [0, 1, 2] => Some([0x28, 0x1F, 0x37]),
                _ => None,
            };
            if let Some([r, g, b]) = c {
                return Color32::from_rgb(r, g, b);
            }
        }
    }
    let n = of.len().max(1) as f32;
    let sum = of.iter().fold([0.0f32; 3], |a, &k| {
        let c = colour(info, k);
        [
            a[0] + c.r() as f32,
            a[1] + c.g() as f32,
            a[2] + c.b() as f32,
        ]
    });
    let mean = sum.map(|v| v / n);
    let grey = (mean[0] + mean[1] + mean[2]) / 3.0;
    let c = mean.map(|v| ((grey + (v - grey) * 2.5) * 0.4).clamp(0.0, 255.0) as u8);
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Which colour channel (0 red, 1 green, 2 blue) a curve stands for, by
/// its colour; `None` when it is not clearly one.
fn channel_of(info: &ParamInfo, k: usize) -> Option<usize> {
    let c = info.curve_colours.get(k)?;
    let (i, &m) = c.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    (c.iter().enumerate().all(|(j, &v)| j == i || v + 0.1 < m)).then_some(i)
}

/// The Channel list's first item: the curves together, named by their
/// initials ("RGB") when those tell them apart, or "All" (Brightness and
/// Blue both begin with a B).
fn all_label(labels: &[String]) -> String {
    let initials: Vec<char> = labels.iter().filter_map(|l| l.chars().next()).collect();
    let apart = initials
        .iter()
        .enumerate()
        .all(|(i, c)| !initials[..i].contains(c));
    if labels.len() > 1 && labels.len() <= 4 && initials.len() == labels.len() && apart {
        initials.into_iter().collect()
    } else {
        tr("All").to_string()
    }
}

/// `base` with the points `moved` gone `by` together: as far as the points
/// beside them that stay allow across (the ends within `range`) and
/// within `values` up — the group keeps its shape.
fn move_points(
    base: &[CurvePoint],
    moved: &BTreeSet<usize>,
    by: [f64; 2],
    range: [f64; 2],
    values: [f64; 2],
) -> Vec<CurvePoint> {
    const EPS: f64 = 1e-3;
    let n = base.len();
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
    let (mut down, mut up) = (f64::NEG_INFINITY, f64::INFINITY);
    for &i in moved.iter().filter(|&&i| i < n) {
        let p = base[i];
        let left = match i {
            0 => range[0],
            _ if moved.contains(&(i - 1)) => f64::NEG_INFINITY,
            _ => base[i - 1].x + EPS,
        };
        let right = if i + 1 == n {
            range[1]
        } else if moved.contains(&(i + 1)) {
            f64::INFINITY
        } else {
            base[i + 1].x - EPS
        };
        lo = lo.max(left - p.x);
        hi = hi.min(right - p.x);
        down = down.max(values[0] - p.y);
        up = up.min(values[1] - p.y);
    }
    let clamp = |v: f64, a: f64, b: f64| if a <= b { v.clamp(a, b) } else { 0.0 };
    let (dx, dy) = (clamp(by[0], lo, hi), clamp(by[1], down, up));
    let mut c = base.to_vec();
    for &i in moved.iter().filter(|&&i| i < n) {
        c[i].x += dx;
        c[i].y += dy;
    }
    c
}

/// Moves a handle of point `i` of `c` to `to` (`after`: the one toward the
/// next point); locked, the other turns to stay in line.
pub(crate) fn move_handle(c: &mut [CurvePoint], i: usize, after: bool, to: [f64; 2]) {
    let mut h = curve::handles(c, i);
    let p = c[i];
    let mut d = [to[0] - p.x, to[1] - p.y];
    if after {
        d[0] = d[0].max(0.0);
    } else {
        d[0] = d[0].min(0.0);
    }
    let len = |v: [f64; 2]| (v[0] * v[0] + v[1] * v[1]).sqrt();
    let (moved, other) = if after {
        (&mut h.after, &mut h.before)
    } else {
        (&mut h.before, &mut h.after)
    };
    *moved = d;
    let (lm, lo) = (len(d), len(*other));
    if !h.split && lm > 1e-9 && lo > 1e-9 {
        *other = [-d[0] / lm * lo, -d[1] / lm * lo];
    }
    c[i].handles = Some(h);
}

/// `c` with a point added at `x` on it, the curve keeping its shape (its
/// segment split there).
pub(crate) fn add_point(c: &[CurvePoint], x: f64) -> Vec<CurvePoint> {
    let mut out = c.to_vec();
    if c.is_empty() {
        return vec![CurvePoint::new(x, x)];
    }
    let n = c.len();
    if x <= c[0].x {
        out.insert(0, CurvePoint::new(x, c[0].y));
        return out;
    }
    if x >= c[n - 1].x {
        out.push(CurvePoint::new(x, c[n - 1].y));
        return out;
    }
    let i = (c.partition_point(|p| p.x <= x) - 1).min(n - 2);
    if (c[i].x - x).abs() < 1e-6 {
        return out;
    }
    let s = curve::segment(c, i);
    let bez = |t: f64, k: usize| {
        let u = 1.0 - t;
        u * u * u * s[0][k]
            + 3.0 * u * u * t * s[1][k]
            + 3.0 * u * t * t * s[2][k]
            + t * t * t * s[3][k]
    };
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..48 {
        let m = 0.5 * (lo + hi);
        if bez(m, 0) < x {
            lo = m;
        } else {
            hi = m;
        }
    }
    let t = 0.5 * (lo + hi);
    let lerp = |a: [f64; 2], b: [f64; 2]| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    let (p01, p12, p23) = (lerp(s[0], s[1]), lerp(s[1], s[2]), lerp(s[2], s[3]));
    let (p012, p123) = (lerp(p01, p12), lerp(p12, p23));
    let mid = lerp(p012, p123);
    let sub = |a: [f64; 2], b: [f64; 2]| [a[0] - b[0], a[1] - b[1]];
    let (left, right) = (curve::handles(c, i), curve::handles(c, i + 1));
    out[i].handles = Some(Handles {
        after: sub(p01, s[0]),
        ..left
    });
    out[i + 1].handles = Some(Handles {
        before: sub(p23, s[3]),
        ..right
    });
    out.insert(
        i + 1,
        CurvePoint::with(mid[0], mid[1], sub(p012, mid), sub(p123, mid)),
    );
    out
}

/// The handles of point `i` put back level (their lengths kept).
fn reset_handles(c: &mut [CurvePoint], i: usize) {
    let h = curve::handles(c, i);
    let len = |v: [f64; 2]| (v[0] * v[0] + v[1] * v[1]).sqrt();
    c[i].handles = Some(Handles {
        before: [-len(h.before), 0.0],
        after: [len(h.after), 0.0],
        split: h.split,
    });
}

/// Locks (in line) or unlocks the handles of point `i`.
pub(crate) fn lock_handles(c: &mut [CurvePoint], i: usize, lock: bool) {
    let mut h = curve::handles(c, i);
    h.split = !lock;
    if lock {
        let len = |v: [f64; 2]| (v[0] * v[0] + v[1] * v[1]).sqrt();
        let (la, lb) = (len(h.after), len(h.before));
        if la > 1e-9 {
            h.before = [-h.after[0] / la * lb, -h.after[1] / la * lb];
        }
    }
    c[i].handles = Some(h);
}

/// The curves with the edited ones (`to`) given `c`.
fn with(curves: &[Vec<CurvePoint>], to: &[usize], c: &[CurvePoint]) -> Vec<Vec<CurvePoint>> {
    let mut out = curves.to_vec();
    for &k in to {
        if let Some(slot) = out.get_mut(k) {
            *slot = c.to_vec();
        }
    }
    out
}

/// Draws the editor in two cells of the settings' grid — the left column,
/// then the box and its bar — for the parametric parameter `info` whose
/// curves are `curves`; `label` and `labels` name it and its curves in the
/// interface's language. The curves changed by the user.
pub fn editor(
    ui: &mut Ui,
    view: &mut CurvesView,
    id: egui::Id,
    info: &ParamInfo,
    curves: &[Vec<CurvePoint>],
    label: &str,
    labels: &[String],
) -> Option<Vec<Vec<CurvePoint>>> {
    let n = curves.len();
    let enabled = info.enabled;
    let height = BOX.y + 2.0 * MARGIN + GAP + BAR;
    let mut out = None;
    view.channel = view.channel.min(n);
    // The left column.
    ui.allocate_ui_with_layout(
        vec2(PANEL, height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_size(vec2(PANEL, height));
            ui.add_enabled_ui(enabled, |ui| {
                // One curve: its name, as the other rows have.
                if n <= 1 {
                    ui.label(format!("{label}:"));
                }
                if n > 1 {
                    ui.horizontal(|ui| {
                        ui.label(tr("Channel:"));
                        let names: Vec<String> = std::iter::once(all_label(labels))
                            .chain(labels.iter().cloned())
                            .collect();
                        egui::ComboBox::from_id_salt(id.with("channel"))
                            .width(72.0)
                            .selected_text(names[view.channel].clone())
                            .show_ui(ui, |ui| {
                                for (k, name) in names.iter().enumerate() {
                                    if ui.selectable_label(view.channel == k, name).clicked() {
                                        view.channel = k;
                                        view.picked.clear();
                                        view.anchor = None;
                                    }
                                }
                            });
                    });
                    crate::widgets::checkbox(ui, &mut view.show_all, tr("Show all channels"));
                }
                // The buttons that make the other curves like one, at the
                // column's foot.
                if n > 1 {
                    let room = ui.available_height() - 30.0;
                    if room > 0.0 {
                        ui.add_space(room);
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (k, c) in curves.iter().enumerate() {
                            let name = labels.get(k).cloned().unwrap_or_default();
                            if match_button(ui, info, k, labels)
                                .on_hover_text(trf!("Set All to {}", name))
                                .clicked()
                            {
                                out = Some(vec![c.clone(); n]);
                            }
                        }
                    });
                }
            });
        },
    );
    // The box and the bar.
    let (rect, _) = ui.allocate_exact_size(vec2(BOX.x + 2.0 * MARGIN, height), Sense::hover());
    let frame = Rect::from_min_size(rect.min, vec2(rect.width(), BOX.y + 2.0 * MARGIN));
    let area = frame.shrink(MARGIN);
    let bar = Rect::from_min_size(
        pos2(rect.left(), frame.bottom() + GAP),
        vec2(rect.width(), BAR),
    );
    let [x0, mut x1] = info.curve_range;
    if x1 <= x0 {
        x1 = x0 + 1.0;
    }
    let (mut y0, mut y1) = (0.0f64, 1.0f64);
    for p in curves.iter().flatten() {
        y0 = y0.min(p.y);
        y1 = y1.max(p.y);
    }
    let to_screen = |p: [f64; 2]| {
        pos2(
            area.left() + ((p[0] - x0) / (x1 - x0)) as f32 * area.width(),
            area.bottom() - ((p[1] - y0) / (y1 - y0)) as f32 * area.height(),
        )
    };
    let to_curve = |s: Pos2| {
        [
            x0 + ((s.x - area.left()) / area.width()) as f64 * (x1 - x0),
            y0 + ((area.bottom() - s.y) / area.height()) as f64 * (y1 - y0),
        ]
    };
    let edit = edited(view, curves);
    // What is under the pointer: a handle (on top) or a point.
    let hit = |at: Pos2| -> Option<Grip> {
        let (_, c) = edit.as_ref()?;
        for (i, p) in c.iter().enumerate() {
            let h = curve::handles(c, i);
            let base = [p.x, p.y];
            if i + 1 < c.len()
                && to_screen([base[0] + h.after[0], base[1] + h.after[1]]).distance(at) <= REACH
            {
                return Some(Grip::After(i));
            }
            if i > 0
                && to_screen([base[0] + h.before[0], base[1] + h.before[1]]).distance(at) <= REACH
            {
                return Some(Grip::Before(i));
            }
        }
        c.iter()
            .position(|p| to_screen([p.x, p.y]).distance(at) <= REACH)
            .map(Grip::Point)
    };
    let sense = if enabled {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let resp = ui.interact(frame, id.with("box"), sense);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, enabled, label));
    let mods = ui.input(|i| i.modifiers);
    // What was picked and is gone (deleted, undone, another curve).
    let points = edit.as_ref().map_or(0, |(_, c)| c.len());
    view.picked.retain(|&i| i < points);
    if view.anchor.is_some_and(|a| a >= points) {
        view.anchor = None;
    }
    if let Some((to, c)) = &edit {
        // A drag moves the points picked together (taking one not picked
        // picks it as a click would), or one handle.
        if resp.drag_started() {
            let at = ui.input(|i| i.pointer.press_origin()).unwrap_or_default();
            let grip = hit(at);
            if let Some(Grip::Point(i)) = grip
                && !view.picked.contains(&i)
            {
                pick(view, i, mods);
            }
            view.drag = grip.map(|grip| Drag {
                grip,
                from: to_curve(at),
                base: c.clone(),
            });
        }
        if resp.dragged()
            && let (Some(d), Some(p)) = (&view.drag, resp.interact_pointer_pos())
        {
            let at = to_curve(p);
            let moved = match d.grip {
                Grip::Point(_) => {
                    let by = [at[0] - d.from[0], at[1] - d.from[1]];
                    move_points(&d.base, &view.picked, by, [x0, x1], [y0, y1])
                }
                Grip::Before(i) | Grip::After(i) => {
                    let mut c = d.base.clone();
                    move_handle(&mut c, i, matches!(d.grip, Grip::After(_)), at);
                    c
                }
            };
            out = Some(with(curves, to, &moved));
        }
        // A click picks a point (Ctrl: one more or one less; Shift: the
        // run from the last one clicked); on the box, none.
        if resp.clicked() {
            resp.request_focus();
            match resp.interact_pointer_pos().and_then(hit) {
                Some(Grip::Point(i)) => pick(view, i, mods),
                Some(_) => {}
                None if !(mods.command || mods.shift) => {
                    view.picked.clear();
                    view.anchor = None;
                }
                None => {}
            }
        }
        if resp.double_clicked()
            && let Some(p) = resp.interact_pointer_pos()
            && !matches!(hit(p), Some(Grip::Point(_)))
        {
            let x = to_curve(p)[0].clamp(x0, x1);
            let added = add_point(c, x);
            pick_added(view, &added, x);
            out = Some(with(curves, to, &added));
        }
        // Delete deletes the points picked.
        if resp.has_focus()
            && !view.picked.is_empty()
            && ui.input_mut(|inp| inp.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
            && let Some(kept) = without(c, &view.picked)
        {
            view.picked.clear();
            view.anchor = None;
            out = Some(with(curves, to, &kept));
        }
    }
    // The menus: a point's, a handle's (for the points picked when it is
    // one of theirs), and the box's.
    if resp.secondary_clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        view.menu = Some(match hit(p) {
            Some(g) => {
                if let Grip::Point(i) = g
                    && !view.picked.contains(&i)
                {
                    view.picked = BTreeSet::from([i]);
                    view.anchor = Some(i);
                }
                MenuFor::Grip(g)
            }
            None => MenuFor::Box(to_curve(p)),
        });
    }
    let menu = view.menu;
    resp.context_menu(|ui| {
        match (menu, &edit) {
            (Some(MenuFor::Grip(Grip::Point(i))), Some((to, c))) => {
                let gone = targets(view, i);
                let text = if gone.len() > 1 {
                    tr("Delete Points")
                } else {
                    tr("Delete Point")
                };
                let kept = without(c, &gone);
                if ui
                    .add_enabled_ui(kept.is_some(), |ui| menus::item(ui, text))
                    .inner
                    && let Some(kept) = kept
                {
                    view.picked.clear();
                    view.anchor = None;
                    out = Some(with(curves, to, &kept));
                    ui.close();
                }
            }
            (Some(MenuFor::Grip(Grip::Before(i) | Grip::After(i))), Some((to, c))) => {
                let these = targets(view, i);
                let locked = !curve::handles(c, i).split;
                if menus::toggle_item(ui, tr("Lock Tangents"), locked) {
                    let mut c = c.clone();
                    let n = c.len();
                    for &j in these.iter().filter(|&&j| j < n) {
                        lock_handles(&mut c, j, !locked);
                    }
                    out = Some(with(curves, to, &c));
                    ui.close();
                }
                if menus::item(ui, tr("Reset Tangents")) {
                    let mut c = c.clone();
                    let n = c.len();
                    for &j in these.iter().filter(|&&j| j < n) {
                        reset_handles(&mut c, j);
                    }
                    out = Some(with(curves, to, &c));
                    ui.close();
                }
            }
            (Some(MenuFor::Box(at)), edit) => {
                // A point where it was clicked, the curve through it.
                if ui
                    .add_enabled_ui(edit.is_some(), |ui| menus::item(ui, tr("Create Point")))
                    .inner
                    && let Some((to, c)) = edit
                {
                    let x = at[0].clamp(x0, x1);
                    let mut added = add_point(c, x);
                    if let Some(i) = pick_added(view, &added, x) {
                        added[i].y = at[1].clamp(y0, y1);
                    }
                    out = Some(with(curves, to, &added));
                    ui.close();
                }
                if n > 1 {
                    ui.separator();
                    for (k, c) in curves.iter().enumerate() {
                        let name = labels.get(k).cloned().unwrap_or_default();
                        if menus::item(ui, &trf!("Set All to {}", name)) {
                            out = Some(vec![c.clone(); n]);
                            ui.close();
                        }
                    }
                }
            }
            _ => ui.close(),
        }
    });
    if !resp.dragged() {
        view.drag = None;
    }
    // A point or a handle dragged (both ways), or under the pointer.
    let over = resp.hover_pos().and_then(hit).is_some();
    if let Some(g) = curve_grab(view.drag.is_some(), over) {
        g.show(ui.ctx());
    }

    // The box: white, framed, the curves in it.
    let painter = ui.painter_at(rect.expand(1.0));
    painter.rect_filled(frame, 0.0, Color32::WHITE);
    painter.rect_stroke(
        frame,
        0.0,
        Stroke::new(1.0, BORDER),
        egui::StrokeKind::Inside,
    );
    painter.rect_stroke(
        area,
        0.0,
        Stroke::new(1.0, INNER),
        egui::StrokeKind::Outside,
    );
    let clip = ui.painter_at(area.expand(1.0));
    let trace = |c: &[CurvePoint]| -> Vec<Pos2> {
        curve::trace(c, x0, x1, area.width() as usize)
            .into_iter()
            .map(to_screen)
            .collect()
    };
    let edited_set: Vec<usize> = edit.as_ref().map_or_else(Vec::new, |(to, _)| to.clone());
    for (k, c) in curves.iter().enumerate() {
        if edited_set.contains(&k) || !(view.show_all || view.channel == 0 && edit.is_none()) {
            continue;
        }
        clip.add(egui::Shape::line(
            trace(c),
            Stroke::new(1.0, colour(info, k)),
        ));
    }
    if let Some((_, c)) = &edit {
        clip.add(egui::Shape::line(
            trace(c),
            Stroke::new(1.5, dark(info, &edited_set)),
        ));
        // Every point's handles, then the points.
        for (i, p) in c.iter().enumerate() {
            let h = curve::handles(c, i);
            let at = to_screen([p.x, p.y]);
            for (end, shown) in [(h.before, i > 0), (h.after, i + 1 < c.len())] {
                if !shown {
                    continue;
                }
                let e = to_screen([p.x + end[0], p.y + end[1]]);
                if e.distance(at) > 1.0 {
                    painter.line_segment([at, e], Stroke::new(1.0, GREY));
                }
                painter.rect_filled(Rect::from_center_size(e, Vec2::splat(HANDLE)), 0.0, GREY);
            }
        }
        // As VEGAS's: the points picked filled, the one clicked last
        // ringed in black.
        for (i, p) in c.iter().enumerate() {
            let r = Rect::from_center_size(to_screen([p.x, p.y]), Vec2::splat(POINT));
            let picked = view.picked.contains(&i);
            painter.rect_filled(r, 0.0, if picked { BLUE } else { Color32::WHITE });
            painter.rect_stroke(r, 0.0, Stroke::new(1.5, BLUE), egui::StrokeKind::Inside);
            if picked && view.anchor == Some(i) {
                painter.rect_stroke(
                    r,
                    0.0,
                    Stroke::new(1.5, Color32::BLACK),
                    egui::StrokeKind::Outside,
                );
            }
        }
    }

    // The bar: a ramp from black to white through the curves.
    let mesh = ramp_mesh(info, curves, bar.shrink(1.0), [x0, x1]);
    painter.add(egui::Shape::mesh(mesh));
    painter.rect_stroke(bar, 0.0, Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
    out
}

/// The colours a grey ramp comes out as through the curves: those
/// coloured red, green and blue change their channel; with none so
/// coloured, the first changes all three.
fn ramp_mesh(
    info: &ParamInfo,
    curves: &[Vec<CurvePoint>],
    r: Rect,
    [x0, x1]: [f64; 2],
) -> egui::Mesh {
    let channels: Vec<Option<usize>> = (0..curves.len()).map(|k| channel_of(info, k)).collect();
    let any = channels.iter().any(Option::is_some);
    let colour_at = |x: f64| -> Color32 {
        let mut c = [x; 3];
        if any {
            for (k, ch) in channels.iter().enumerate() {
                if let Some(ch) = *ch {
                    c[ch] = curve::curve_at(&curves[k], x);
                }
            }
        } else if let Some(first) = curves.first() {
            c = [curve::curve_at(first, x); 3];
        }
        let b = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color32::from_rgb(b(c[0]), b(c[1]), b(c[2]))
    };
    let steps = (r.width() / 2.0).max(1.0) as usize;
    let mut mesh = egui::Mesh::default();
    for i in 0..=steps {
        let f = i as f32 / steps as f32;
        let x = r.left() + f * r.width();
        let c = colour_at(x0 + (x1 - x0) * f as f64);
        mesh.colored_vertex(pos2(x, r.top()), c);
        mesh.colored_vertex(pos2(x, r.bottom()), c);
        if i > 0 {
            let v = (2 * i) as u32;
            mesh.add_triangle(v - 2, v - 1, v);
            mesh.add_triangle(v - 1, v, v + 1);
        }
    }
    mesh
}

/// A button that makes the other curves like curve `k`: as VEGAS draws
/// it, the others' initials over its own, each in its curve's colour.
fn match_button(ui: &mut Ui, info: &ParamInfo, k: usize, labels: &[String]) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    let p = ui.painter();
    let bg = if resp.hovered() {
        theme::hover()
    } else {
        theme::panel()
    };
    p.rect_filled(rect, 2.0, bg);
    p.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, theme::separator()),
        egui::StrokeKind::Inside,
    );
    let initial = |j: usize| labels.get(j).and_then(|l| l.chars().next()).unwrap_or('?');
    let font = egui::FontId::proportional(9.0);
    let box_at = |c: Pos2, j: usize| {
        let r = Rect::from_center_size(c, vec2(9.0, 9.0));
        p.rect_filled(r, 1.0, Color32::from_gray(0xF4));
        p.rect_stroke(
            r,
            1.0,
            Stroke::new(1.0, colour(info, j)),
            egui::StrokeKind::Inside,
        );
        p.text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            initial(j),
            font.clone(),
            Color32::from_gray(0x20),
        );
    };
    let others: Vec<usize> = (0..labels.len().max(1)).filter(|&j| j != k).collect();
    let n = others.len() as f32;
    for (m, &j) in others.iter().enumerate() {
        let dx = (m as f32 - (n - 1.0) / 2.0) * 10.0;
        box_at(rect.center() + vec2(dx, -5.0), j);
    }
    box_at(rect.center() + vec2(0.0, 6.0), k);
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            trf!("Set All to {}", labels.get(k).cloned().unwrap_or_default()),
        )
    });
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_core::curve::{IDENTITY, curve_at};

    #[test]
    fn a_point_added_keeps_the_curve() {
        let c = vec![
            CurvePoint::with(0.0, 0.0, [0.0, 0.0], [0.4, 0.0]),
            CurvePoint::with(1.0, 1.0, [-0.4, 0.0], [0.0, 0.0]),
        ];
        let added = add_point(&c, 0.3);
        assert_eq!(added.len(), 3);
        for i in 0..=20 {
            let x = i as f64 / 20.0;
            assert!(
                (curve_at(&c, x) - curve_at(&added, x)).abs() < 1e-6,
                "{x}: {} {}",
                curve_at(&c, x),
                curve_at(&added, x)
            );
        }
        // On the curve, where the double-click was.
        assert!((added[1].x - 0.3).abs() < 1e-9);
    }

    #[test]
    fn locked_handles_stay_in_line() {
        let mut c = vec![
            CurvePoint::new(0.0, 0.0),
            CurvePoint::new(0.5, 0.5),
            CurvePoint::new(1.0, 1.0),
        ];
        move_handle(&mut c, 1, true, [0.6, 0.9]);
        let h = c[1].handles.expect("handles");
        // The other turned to point the opposite way.
        let cross = h.after[0] * h.before[1] - h.after[1] * h.before[0];
        assert!(cross.abs() < 1e-9 && h.before[0] < 0.0);
        // Unlocked, each moves alone.
        lock_handles(&mut c, 1, false);
        let before = c[1].handles.expect("handles").before;
        move_handle(&mut c, 1, true, [0.6, 0.5]);
        assert_eq!(c[1].handles.expect("handles").before, before);
        // Reset: level again.
        reset_handles(&mut c, 1);
        let h = c[1].handles.expect("handles");
        assert_eq!((h.before[1], h.after[1]), (0.0, 0.0));
    }

    #[test]
    fn points_are_picked_alone_one_more_or_in_a_run() {
        let mut view = CurvesView::default();
        let none = egui::Modifiers::NONE;
        let ctrl = egui::Modifiers::COMMAND;
        let shift = egui::Modifiers::SHIFT;
        pick(&mut view, 1, none);
        pick(&mut view, 3, ctrl);
        assert_eq!(view.picked, BTreeSet::from([1, 3]));
        // Ctrl again: one less.
        pick(&mut view, 1, ctrl);
        assert_eq!(view.picked, BTreeSet::from([3]));
        // Shift: the run from the one clicked last.
        pick(&mut view, 3, none);
        pick(&mut view, 0, shift);
        assert_eq!(view.picked, BTreeSet::from([0, 1, 2, 3]));
        assert_eq!(view.anchor, Some(3));
        pick(&mut view, 2, none);
        assert_eq!(view.picked, BTreeSet::from([2]));
    }

    #[test]
    fn points_picked_move_together_as_far_as_the_others_allow() {
        let c = vec![
            CurvePoint::new(0.0, 0.0),
            CurvePoint::new(0.3, 0.3),
            CurvePoint::new(0.5, 0.5),
            CurvePoint::new(0.8, 0.8),
            CurvePoint::new(1.0, 1.0),
        ];
        let picked = BTreeSet::from([1, 2]);
        let moved = move_points(&c, &picked, [0.1, 0.2], [0.0, 1.0], [0.0, 1.0]);
        assert!((moved[1].x - 0.4).abs() < 1e-9 && (moved[1].y - 0.5).abs() < 1e-9);
        assert!((moved[2].x - 0.6).abs() < 1e-9 && (moved[2].y - 0.7).abs() < 1e-9);
        assert_eq!((moved[0], moved[3]), (c[0], c[3]));
        // The point after them stops them, the group keeping its shape.
        let far = move_points(&c, &picked, [0.5, 0.0], [0.0, 1.0], [0.0, 1.0]);
        assert!(far[2].x < c[3].x && (far[2].x - far[1].x - 0.2).abs() < 1e-9);
        // Deleting: never fewer than two points.
        assert_eq!(without(&c, &picked).map(|k| k.len()), Some(3));
        assert!(without(&c, &BTreeSet::from([0, 1, 2, 3])).is_none());
    }

    #[test]
    fn all_channels_together_edit_the_curve_most_share() {
        let warm = vec![
            vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 0.8)],
            IDENTITY.to_vec(),
            IDENTITY.to_vec(),
        ];
        let view = CurvesView::default();
        let (to, c) = edited(&view, &warm).expect("green and blue are alike");
        // Red keeps its own: the colors stay.
        assert_eq!(to, [1, 2]);
        assert_eq!(c, IDENTITY.to_vec());
        let moved = vec![CurvePoint::new(0.0, 0.1), CurvePoint::new(1.0, 0.9)];
        let after = with(&warm, &to, &moved);
        assert_eq!(after[0], warm[0]);
        assert_eq!((&after[1], &after[2]), (&moved, &moved));
        // All different: nothing to edit together.
        let night = vec![
            vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 0.4)],
            vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 0.5)],
            vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 0.6)],
        ];
        assert!(edited(&view, &night).is_none());
        let green = CurvesView {
            channel: 2,
            ..CurvesView::default()
        };
        assert_eq!(edited(&green, &night).expect("green").0, [1]);
        assert_eq!(
            all_label(&["Red".into(), "Green".into(), "Blue".into()]),
            "RGB"
        );
        let grading = ["Brightness", "Red", "Green", "Blue"].map(String::from);
        assert_eq!(all_label(&grading), tr("All"));
    }

    /// As VEGAS's plug-ins' curves: the hand over a point or a handle, the
    /// four arrows while one is dragged (wherever the pointer went).
    #[test]
    fn a_point_shows_the_hand_and_dragged_the_four_arrows() {
        assert_eq!(curve_grab(false, true), Some(Grab::Over));
        assert_eq!(curve_grab(true, false), Some(Grab::Free));
        assert_eq!(curve_grab(true, true), Some(Grab::Free));
        assert_eq!(curve_grab(false, false), None);
    }
}
