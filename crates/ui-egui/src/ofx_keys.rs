//! The keyframe controller under a plug-in's settings, as VEGAS's: a ruler
//! over its owner's time; a row for the effect (every keyframe of it), one
//! for each animated parameter and, for a parameter of several numbers (a
//! position), one for each number — their keyframes over their values
//! (Lanes) or the values' curves in one graph (Curves). Curves (parametric
//! parameters) keep their keyframes in the effect's row, as VEGAS's Color
//! Curves: with only those, it is the one row and there is no view to
//! pick. Keyframes are
//! picked with the mouse (a click; Ctrl or Shift and a click; a box drawn
//! around them, the cursor following it), dragged (with Ctrl, copies; with
//! Alt, the first or last of a group spaces it out), cut, copied, pasted,
//! given a curve or a value from their menu — a Bézier curve's handles,
//! locked or split, dragged in Curves; the buttons go from keyframe to
//! keyframe, make and delete them and keep the cursor with the
//! timeline's.

use std::collections::BTreeSet;

use bv_core::{FrameRate, Tick};
use bv_engine::plugins::{animation, frames, handles, set_handle};
use bv_ofx::ParamType;
use bv_timeline::{KeyCurve, PluginKey, PluginParam, PluginValue};

use crate::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use crate::i18n::tr;
use crate::icons::Icon;
use crate::menus;
use crate::ofx::{FxPlace, KeyEdit};
use crate::ofx_curves::Grab;
use crate::theme;

const RULER: f32 = 22.0;
const ROW: f32 = 30.0;
const SCROLL: f32 = 16.0;
const BAR: f32 = 32.0;
/// The room between a row's edges and its keyframes (Lanes), and between
/// the graph's and its curves (Curves).
const LANE_PAD: f32 = 5.0;
const GRAPH_PAD: f32 = 16.0;

/// The narrowest the controller is: its buttons in a row.
pub const MIN_WIDTH: f32 = 500.0;
/// Its height when it opens.
pub const HEIGHT: f32 = 250.0;
/// The least it takes: a row and its bars.
pub const MIN_HEIGHT: f32 = RULER + ROW + SCROLL + BAR + 4.0;

/// The controller's own state, for each effect shown.
#[derive(Clone, Debug)]
pub struct KeyView {
    /// Curves (the values' curves in one graph) rather than Lanes.
    pub curves: bool,
    /// How much of the owner's time shows: 1, all of it.
    zoom: f64,
    /// The time at the left edge, a part (0–1) of the owner's time.
    start: f64,
    /// The cursor goes with the timeline's.
    pub sync: bool,
    /// The controller's own cursor (a timeline time) while it is not.
    pub local: Option<Tick>,
    /// The effect's row open: its parameters' rows show.
    pub open: bool,
    /// The parameters of several numbers whose rows are closed.
    closed: BTreeSet<String>,
    /// The row picked (its name in blue; its curve the one edited).
    row: Row,
    /// The keyframes picked: their parameters and times.
    picked: BTreeSet<(String, Tick)>,
    /// How far down the rows are scrolled.
    scroll: f32,
    /// Its height (the bar above it sets it).
    pub height: f32,
    drag: Option<Drag>,
    /// "Set to…" open: the keyframe, its text and where it shows.
    set_to: Option<SetTo>,
    /// The V key's press was seen (Ctrl+V with no text on the clipboard
    /// arrives only as its release).
    v_down: bool,
}

impl Default for KeyView {
    fn default() -> KeyView {
        KeyView {
            curves: false,
            zoom: 1.0,
            start: 0.0,
            sync: true,
            local: None,
            open: true,
            closed: BTreeSet::new(),
            row: Row::Effect,
            picked: BTreeSet::new(),
            scroll: 0.0,
            height: HEIGHT,
            drag: None,
            set_to: None,
            v_down: false,
        }
    }
}

/// A row of the controller.
#[derive(Clone, Debug, Default, PartialEq)]
enum Row {
    /// The effect's.
    #[default]
    Effect,
    /// A parameter's (of several numbers: their group's).
    Param(String),
    /// One number of a parameter of several.
    Part(String, usize),
}

#[derive(Clone, Debug, PartialEq)]
enum Drag {
    /// Keyframes being moved (with Ctrl, copies of them).
    Keys(KeyDrag),
    /// The cursor, from the ruler.
    Cursor,
    /// The box that picks keyframes, from a time (`add`: to those picked).
    Band { from: Tick, add: bool },
    /// A Bézier keyframe's handle (Curves).
    Handle(HandleDrag),
}

/// A Bézier keyframe's handle being dragged: its parameter as it was, the
/// keyframe, the number, the handle (the one after it, or the one before)
/// and how the graph's heights are values.
#[derive(Clone, Debug, PartialEq)]
struct HandleDrag {
    name: String,
    at: Tick,
    part: usize,
    after: bool,
    base: PluginParam,
    range: (f64, f64),
}

#[derive(Clone, Debug, PartialEq)]
struct KeyDrag {
    /// The time of the keyframe taken, and the pointer's offset from it.
    at: Tick,
    grip: egui::Vec2,
    /// The keyframes picked, and their parameters' keyframes, as they were.
    picked: Vec<(String, Tick)>,
    base: Vec<(String, Vec<PluginKey>)>,
    copy: bool,
    /// Alt: the first or last of the group spaces it out evenly.
    space: bool,
    /// Curves: the parameter and number whose value follows the pointer,
    /// and how the graph's heights are values.
    value: Option<(String, usize, Rect, (f64, f64))>,
}

/// The controller's pointer: what is dragged — keyframes along the time
/// (in Curves the one taken goes up and down with its value too), a
/// handle both ways —, else a keyframe or a handle under it (`over`).
fn key_grab(drag: Option<&Drag>, over: bool) -> Option<Grab> {
    match drag {
        Some(Drag::Keys(d)) if d.value.is_some() => Some(Grab::Free),
        Some(Drag::Keys(_)) => Some(Grab::Across),
        Some(Drag::Handle(_)) => Some(Grab::Free),
        Some(Drag::Cursor | Drag::Band { .. }) => None,
        None => over.then_some(Grab::Over),
    }
}

/// "Set to…": a keyframe's value being typed.
#[derive(Clone, Debug, PartialEq)]
struct SetTo {
    name: String,
    at: Tick,
    text: String,
    pos: Pos2,
    focus: bool,
}

/// An animated parameter's row.
#[derive(Debug)]
pub struct KeyRow<'a> {
    pub name: &'a str,
    pub label: String,
    pub param: &'a PluginParam,
    pub ty: ParamType,
    /// The values each number's curve spans (Curves).
    pub ranges: Vec<(f64, f64)>,
    /// What each number is divided by to show it ("Set to…"): a
    /// position's frame size, or 1.
    pub scale: Vec<f64>,
    /// Each number's name in its row (none: X, Y… or R, G…).
    pub parts: Vec<String>,
}

/// The height the controller takes.
pub fn height(view: &KeyView) -> f32 {
    view.height.max(MIN_HEIGHT)
}

/// What the user did in the controller.
#[derive(Debug, Default)]
pub struct Answer {
    pub edits: Vec<KeyEdit>,
    /// Where the cursor goes (the owner's time).
    pub seek: Option<Tick>,
}

/// A good step between the ruler's labels, in seconds.
fn ruler_step(px_per_second: f64, rate: FrameRate) -> f64 {
    let frame = rate.frame_duration().as_seconds_f64();
    [
        frame, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0,
    ]
    .into_iter()
    .find(|s| s * px_per_second >= 96.0)
    .unwrap_or(3600.0)
}

fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// `a` towards `b` by `t`.
pub(crate) fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// The colors of the numbers' curves (Curves), in their rows' order.
fn curve_color(i: usize) -> Color32 {
    [
        0xFF0000, 0x208080, 0x508020, 0x502080, 0xC08020, 0x2060C0, 0xC03090, 0x808020,
    ]
    .map(rgb)[i % 8]
}

/// A keyframe's color, its interpolation curve's as VEGAS paints them:
/// linear grey, fast green, slow gold, smooth lilac, sharp pink, hold red;
/// Bézier ones amber (BananaV's).
pub(crate) fn key_color(c: KeyCurve) -> Color32 {
    rgb(match c {
        KeyCurve::Linear => 0xC0C0C0,
        KeyCurve::Fast => 0xC0FFC0,
        KeyCurve::Slow => 0xC9C98A,
        KeyCurve::Smooth => 0xD2C4F4,
        KeyCurve::Sharp => 0xF4C4DA,
        KeyCurve::Hold => 0xF29090,
        KeyCurve::Bezier => 0xF2D27A,
        KeyCurve::BezierSplit => 0xE6B45C,
    })
}

/// The rows of numbers' shades (Lanes): lighter and darker in turn.
fn shade(i: usize) -> Color32 {
    if i.is_multiple_of(2) {
        mix(theme::panel(), theme::text(), 0.21)
    } else {
        theme::separator()
    }
}

/// The interpolation curves of a keyframe's menu, in VEGAS's order.
const CURVES: [(KeyCurve, &str); 6] = [
    (KeyCurve::Linear, "Linear Fade"),
    (KeyCurve::Smooth, "Smooth Fade"),
    (KeyCurve::Fast, "Fast Fade"),
    (KeyCurve::Slow, "Slow Fade"),
    (KeyCurve::Sharp, "Sharp Fade"),
    (KeyCurve::Hold, "Hold"),
];

/// The Bézier curves, the menu's last items (numbers only).
const BEZIERS: [(KeyCurve, &str); 2] = [
    (KeyCurve::Bezier, "Bézier curve (locked tangents)"),
    (KeyCurve::BezierSplit, "Bézier curve (split tangents)"),
];

/// A number's name in its parameter's rows.
fn part_label(ty: ParamType, d: usize) -> &'static str {
    match ty {
        ParamType::Rgb | ParamType::Rgba => ["R", "G", "B", "A"][d.min(3)],
        _ => ["X", "Y", "Z", "W"][d.min(3)],
    }
}

/// A row as it shows.
#[derive(Debug, PartialEq)]
struct Line {
    row: Row,
    /// 0: the effect's; 1: a parameter's; 2: a number's.
    depth: usize,
    label: String,
    /// Its parameter's index in the rows, and its number.
    param: Option<usize>,
    part: Option<usize>,
    /// A parameter of several numbers (its triangle opens them).
    group: bool,
    /// The row of numbers it is (its shade, its curve's color).
    shade: usize,
}

/// Whether a parameter's keyframes have a row of their own (a parametric
/// parameter's, curves, and a plug-in's own data, such as Mesh Warp's
/// mesh, show in the effect's only).
fn has_lane(r: &KeyRow<'_>) -> bool {
    !matches!(r.ty, ParamType::Parametric | ParamType::Custom)
}

/// The rows that show, in order.
fn lines(view: &KeyView, title: &str, rows: &[KeyRow<'_>]) -> Vec<Line> {
    let mut out = vec![Line {
        row: Row::Effect,
        depth: 0,
        label: title.to_string(),
        param: None,
        part: None,
        group: false,
        shade: 0,
    }];
    let mut shade = 0;
    for (i, r) in rows.iter().enumerate() {
        // Curves have their keyframes in the effect's row alone, as VEGAS's
        // Color Curves.
        if !has_lane(r) {
            continue;
        }
        let n = r.ty.dimension();
        if n <= 1 {
            if view.open {
                out.push(Line {
                    row: Row::Param(r.name.to_string()),
                    depth: 1,
                    label: r.label.clone(),
                    param: Some(i),
                    part: (n == 1).then_some(0),
                    group: false,
                    shade,
                });
            }
            shade += 1;
            continue;
        }
        if view.open {
            out.push(Line {
                row: Row::Param(r.name.to_string()),
                depth: 1,
                label: r.label.clone(),
                param: Some(i),
                part: None,
                group: true,
                shade: 0,
            });
        }
        let open = view.open && !view.closed.contains(r.name);
        for d in 0..n {
            if open {
                out.push(Line {
                    row: Row::Part(r.name.to_string(), d),
                    depth: 2,
                    label: r
                        .parts
                        .get(d)
                        .cloned()
                        .unwrap_or_else(|| part_label(r.ty, d).to_string()),
                    param: Some(i),
                    part: Some(d),
                    group: false,
                    shade,
                });
            }
            shade += 1;
        }
    }
    out
}

/// `keys` with those at the first times of `moves` gone to the second
/// (`copy`: copies of them), any other keyframe there giving way.
fn retimed(keys: &[PluginKey], moves: &[(Tick, Tick)], copy: bool) -> Vec<PluginKey> {
    let moved: Vec<PluginKey> = moves
        .iter()
        .filter_map(|&(from, to)| {
            keys.iter().find(|k| k.at == from).map(|k| PluginKey {
                at: to,
                ..k.clone()
            })
        })
        .collect();
    let mut out: Vec<PluginKey> = keys
        .iter()
        .filter(|k| copy || !moves.iter().any(|(f, _)| *f == k.at))
        .filter(|k| !moved.iter().any(|m| m.at == k.at))
        .cloned()
        .collect();
    out.extend(moved);
    out.sort_by_key(|k| k.at);
    out
}

/// Where the keyframes at `times` (sorted) go when the one at `from` is
/// dragged `by`: all of them with it — or, `space` and `from` the first
/// or the last of three or more, that one alone, the others spaced evenly
/// between it and the other end (on the frames of `rate`).
fn moved_time(t: Tick, times: &[Tick], from: Tick, by: Tick, space: bool, rate: FrameRate) -> Tick {
    let n = times.len();
    let first = times.first().copied();
    let last = times.last().copied();
    if !space || n < 3 || (Some(from) != first && Some(from) != last) {
        return t + by;
    }
    let (Some(first), Some(last)) = (first, last) else {
        return t + by;
    };
    let i = times.iter().position(|&x| x == t).unwrap_or(0);
    let (a, b) = if from == first {
        (first + by, last)
    } else {
        (first, last + by)
    };
    let f = i as f64 / (n - 1) as f64;
    rate.snap_nearest(a + Tick(((b - a).0 as f64 * f).round() as i64))
}

/// A keyframe mark: where it shows, what it stands for.
struct Mark {
    line: usize,
    at: Tick,
    /// The keyframes it stands for: one, or (the effect's row) a time's.
    keys: Vec<(String, Tick)>,
    curve: KeyCurve,
    pos: Pos2,
    shape: Shape,
}

/// A Bézier keyframe's handle in the Curves graph: its keyframe, number
/// and side, where it and its keyframe show, its curve's span and color.
struct HandleMark {
    name: String,
    at: Tick,
    part: usize,
    after: bool,
    key: Pos2,
    pos: Pos2,
    range: (f64, f64),
    color: Color32,
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    /// The effect's row.
    Diamond,
    /// A row's (Lanes).
    Dot,
    /// The edited curve's (Curves).
    Point,
}

fn paint_mark(p: &egui::Painter, m: &Mark, picked: bool, hot: bool) {
    let c = m.pos;
    let outline = if picked {
        Stroke::new(2.0, theme::selection())
    } else {
        Stroke::new(1.0, Color32::BLACK)
    };
    let base = match m.shape {
        Shape::Diamond => rgb(0xC0C0C0),
        Shape::Dot => key_color(m.curve),
        Shape::Point => Color32::WHITE,
    };
    let fill = if picked {
        Color32::WHITE
    } else if hot {
        mix(base, Color32::WHITE, 0.5)
    } else {
        base
    };
    match m.shape {
        Shape::Diamond => {
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + vec2(0.0, -7.0),
                    c + vec2(7.0, 0.0),
                    c + vec2(0.0, 7.0),
                    c + vec2(-7.0, 0.0),
                ],
                fill,
                outline,
            ));
        }
        Shape::Dot | Shape::Point => {
            p.circle(c, 5.0, fill, outline);
        }
    }
}

/// The keyframes of `rows` at times within `t0..=t1`.
fn keys_between(rows: &[KeyRow<'_>], t0: Tick, t1: Tick) -> Vec<(String, Tick)> {
    rows.iter()
        .flat_map(|r| {
            r.param
                .keys
                .iter()
                .filter(move |k| k.at >= t0 && k.at <= t1)
                .map(move |k| (r.name.to_string(), k.at))
        })
        .collect()
}

/// The value of number `d` of the keyframe of `p` at `at`.
fn key_number(p: &PluginParam, at: Tick, d: usize) -> Option<f64> {
    match &p.keys.iter().find(|k| k.at == at)?.value {
        PluginValue::Num(v) => v.get(d).copied(),
        _ => None,
    }
}

/// The lowest and highest values of number `d` of `p`'s keyframes.
fn key_span(p: &PluginParam, d: usize) -> Option<(f64, f64)> {
    let mut span: Option<(f64, f64)> = None;
    for k in &p.keys {
        if let PluginValue::Num(v) = &k.value
            && let Some(&x) = v.get(d)
        {
            span = Some(span.map_or((x, x), |(a, b)| (a.min(x), b.max(x))));
        }
    }
    span
}

/// Draws the controller `height` high (its own, or less where there is
/// not room for it); `title` names the effect's row, `clip` holds the
/// keyframes copied.
#[allow(clippy::too_many_arguments)]
pub fn controller(
    ui: &mut Ui,
    view: &mut KeyView,
    id: egui::Id,
    title: &str,
    rows: &[KeyRow<'_>],
    place: &FxPlace,
    clip: &mut Vec<(String, PluginKey)>,
    height: f32,
) -> Answer {
    let mut out = Answer::default();
    let rate = place.rate;
    let (lo, hi) = place.span;
    let total = (hi - lo).max(Tick(1)).0 as f64;
    let max_zoom = (total / rate.frame_duration().0 as f64).max(1.0);
    view.zoom = view.zoom.clamp(1.0, max_zoom);
    view.start = view.start.clamp(0.0, 1.0 - 1.0 / view.zoom);
    // What was picked and is gone (deleted, undone) is not any more.
    view.picked.retain(|(n, t)| {
        rows.iter()
            .any(|r| r.name == n && r.param.keys.iter().any(|k| k.at == *t))
    });
    let lines = lines(view, title, rows);
    if !lines.iter().any(|l| l.row == view.row) {
        view.row = Row::Effect;
    }
    // Parameters with rows of their own: the effect's row opens on them,
    // and they have the Curves view.
    let nested = rows.iter().any(has_lane);
    if !nested {
        view.curves = false;
    }

    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, height.max(MIN_HEIGHT)), Sense::hover());
    let names_w = (width * 0.2).clamp(96.0, 150.0);
    let vbar_w = 14.0;
    let body_bottom = rect.bottom() - BAR;
    let lanes = Rect::from_min_max(
        pos2(rect.left() + names_w + 2.0, rect.top()),
        pos2(rect.right() - vbar_w, body_bottom - SCROLL),
    );
    let ruler = Rect::from_min_max(lanes.min, pos2(lanes.right(), lanes.top() + RULER));
    let graph = Rect::from_min_max(pos2(lanes.left(), ruler.bottom()), lanes.max);
    let names = Rect::from_min_max(
        pos2(rect.left(), graph.top()),
        pos2(rect.left() + names_w, graph.bottom()),
    );
    let hbar = Rect::from_min_max(
        pos2(lanes.left(), lanes.bottom()),
        pos2(lanes.right(), body_bottom),
    );
    let vbar = Rect::from_min_max(
        pos2(lanes.right(), rect.top()),
        pos2(rect.right(), body_bottom),
    );
    let buttons = Rect::from_min_max(pos2(rect.left(), body_bottom), rect.max);

    // The wheel zooms around the pointer over the lanes and scrolls the
    // rows over their names.
    let content = lines.len() as f32 * ROW;
    let max_scroll = (content - graph.height()).max(0.0);
    let wheel = ui.input(|i| i.smooth_scroll_delta.y);
    if wheel != 0.0
        && let Some(p) = ui.ctx().pointer_hover_pos()
    {
        if ui.rect_contains_pointer(graph) {
            let anchor = (p.x - lanes.left()) as f64 / lanes.width() as f64;
            let at = view.start + anchor / view.zoom;
            view.zoom = (view.zoom * (wheel as f64 * 0.004).exp()).clamp(1.0, max_zoom);
            view.start = (at - anchor / view.zoom).clamp(0.0, 1.0 - 1.0 / view.zoom);
        } else if ui.rect_contains_pointer(names) {
            view.scroll -= wheel;
        }
    }
    view.scroll = view.scroll.clamp(0.0, max_scroll);

    let (zoom, start) = (view.zoom, view.start);
    let x_of = move |t: Tick| {
        let f = ((t - lo).0 as f64 / total - start) * zoom;
        lanes.left() + f as f32 * lanes.width()
    };
    let raw_t = move |x: f32| {
        let f = ((x - lanes.left()) / lanes.width()) as f64 / zoom + start;
        (lo + Tick((f * total) as i64)).clamp(lo, hi)
    };
    // On the timeline's frames (its owner may start between two).
    let origin = place.origin;
    let t_of = move |x: f32| {
        let t = raw_t(x);
        let snapped = match origin {
            Some(o) => rate.snap_nearest(o + t) - o,
            None => rate.snap_nearest(t),
        };
        snapped.clamp(lo, hi)
    };

    // The rows, scrolled up and down.
    let scroll = view.scroll;
    let line_top = |i: usize| graph.top() + i as f32 * ROW - scroll;
    let line_at = |y: f32| -> Option<usize> {
        let i = ((y - graph.top() + scroll) / ROW).floor();
        (i >= 0.0 && (i as usize) < lines.len() && y >= graph.top()).then_some(i as usize)
    };

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::panel());
    let line_color = mix(theme::panel(), theme::text(), 0.42);
    let edge = mix(theme::panel(), theme::text(), 0.21);
    painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, edge));
    painter.hline(rect.x_range(), ruler.bottom() - 0.5, Stroke::new(1.0, edge));
    painter.vline(
        lanes.left() - 1.0,
        rect.top()..=hbar.bottom(),
        Stroke::new(2.0, mix(theme::panel(), theme::text(), 0.3)),
    );

    // The ruler.
    let ruler_p = ui.painter_at(ruler);
    let px_per_s = lanes.width() as f64 * view.zoom / (total / Tick::from_seconds(1).0 as f64);
    let step = ruler_step(px_per_s, rate);
    let first = (raw_t(lanes.left()) - lo).as_seconds_f64().max(0.0);
    let mut s = (first / step).floor() * step;
    while s <= (hi - lo).as_seconds_f64() + 1e-9 {
        let x = x_of(lo + Tick::from_seconds_f64(s));
        if x > lanes.right() {
            break;
        }
        ruler_p.vline(
            x,
            ruler.bottom() - 7.0..=ruler.bottom(),
            Stroke::new(1.0, theme::text_dim()),
        );
        ruler_p.text(
            pos2(x + 3.0, ruler.top() + 3.0),
            Align2::LEFT_TOP,
            bv_core::timecode::format(Tick::from_seconds_f64(s), rate),
            FontId::proportional(10.0),
            theme::text_dim(),
        );
        for m in 1..4 {
            let xm = x_of(lo + Tick::from_seconds_f64(s + step * m as f64 / 4.0));
            ruler_p.vline(
                xm,
                ruler.bottom() - 3.0..=ruler.bottom(),
                Stroke::new(1.0, theme::text_dim()),
            );
        }
        s += step;
    }
    // A click or a drag on the ruler puts the cursor there.
    let on_ruler = ui.interact(ruler, id.with("ruler"), Sense::click_and_drag());
    if on_ruler.drag_started() {
        view.drag = Some(Drag::Cursor);
    }
    if on_ruler.clicked()
        && let Some(p) = on_ruler.interact_pointer_pos()
    {
        out.seek = Some(t_of(p.x));
    }

    // The lanes: a click puts the cursor there (and drops what was
    // picked), a drag draws the box that picks keyframes, the cursor
    // following it, and a double-click makes a keyframe.
    let mods = ui.input(|i| i.modifiers);
    let adding = mods.command || mods.shift;
    let back = ui.interact(graph, id.with("lanes"), Sense::click_and_drag());
    if back.drag_started() {
        let from = ui
            .input(|i| i.pointer.press_origin())
            .map_or(place.time, |p| raw_t(p.x));
        view.drag = Some(Drag::Band { from, add: adding });
    }
    if back.clicked()
        && let Some(p) = back.interact_pointer_pos()
    {
        out.seek = Some(t_of(p.x));
        if !adding {
            view.picked.clear();
        }
        if !view.curves
            && let Some(i) = line_at(p.y)
        {
            view.row = lines[i].row.clone();
        }
    }
    if back.double_clicked()
        && let Some(p) = back.interact_pointer_pos()
    {
        let t = t_of(p.x);
        let row = if view.curves {
            Some(&view.row)
        } else {
            line_at(p.y).map(|i| &lines[i].row)
        };
        out.edits.push(match row {
            Some(Row::Param(n) | Row::Part(n, _)) => KeyEdit::AddTo(n.clone(), t),
            _ => KeyEdit::Add(t),
        });
    }

    // The rows' names, and their lanes (Lanes) or the graph (Curves).
    let names_p = ui.painter_at(names);
    let graph_p = ui.painter_at(graph);
    let mut marks: Vec<Mark> = Vec::new();
    let mut handle_marks: Vec<HandleMark> = Vec::new();
    let all_times: Vec<Tick> = {
        let mut v: Vec<Tick> = rows
            .iter()
            .flat_map(|r| r.param.keys.iter().map(|k| k.at))
            .collect();
        v.sort();
        v.dedup();
        v
    };
    let graph_y = |v: f64, (a, b): (f64, f64)| {
        let f = if b > a { (v - a) / (b - a) } else { 0.5 };
        graph.bottom() - GRAPH_PAD - f as f32 * (graph.height() - 2.0 * GRAPH_PAD)
    };
    for (i, l) in lines.iter().enumerate() {
        let top = line_top(i);
        let row = Rect::from_min_max(pos2(rect.left(), top), pos2(lanes.right(), top + ROW));
        if row.bottom() < graph.top() || row.top() > graph.bottom() {
            continue;
        }
        // The effect's row alone is not picked (VEGAS's shows it plain).
        let selected = view.row == l.row && (nested || l.depth > 0);
        let leaf = l.part.is_some() || (l.param.is_some() && !l.group);
        // Its name: the effect's on the column itself, the others in
        // boxes stepped in by their depth.
        let indent = [0.0, 20.0, 36.0][l.depth.min(2)];
        let cell = Rect::from_min_max(
            pos2(names.left() + indent, row.top()),
            pos2(names.right(), row.bottom()),
        );
        let fill = if selected {
            theme::selection()
        } else if leaf {
            shade(l.shade)
        } else {
            theme::panel()
        };
        names_p.rect_filled(cell, 0.0, fill);
        if l.depth > 0 {
            names_p.rect_stroke(
                cell.shrink(0.5),
                0.0,
                Stroke::new(1.0, theme::hover()),
                egui::StrokeKind::Inside,
            );
        }
        let text_color = if selected {
            Color32::WHITE
        } else {
            theme::text()
        };
        let triangle = (l.depth == 0 && nested) || l.group;
        let tri_x = cell.left() + if l.depth == 0 { 12.0 } else { 10.0 };
        let text_x = if triangle {
            tri_x + 14.0
        } else {
            cell.left() + 8.0
        };
        if triangle {
            let open = if l.depth == 0 {
                view.open
            } else {
                !l.param.is_some_and(|p| view.closed.contains(rows[p].name))
            };
            let c = pos2(tri_x, cell.center().y);
            let pts = if open {
                vec![
                    c + vec2(-6.0, -3.0),
                    c + vec2(6.0, -3.0),
                    c + vec2(0.0, 4.0),
                ]
            } else {
                vec![
                    c + vec2(-3.0, -6.0),
                    c + vec2(4.0, 0.0),
                    c + vec2(-3.0, 6.0),
                ]
            };
            names_p.add(egui::Shape::convex_polygon(pts, text_color, Stroke::NONE));
        }
        let dot = view.curves && leaf && l.part.is_some();
        crate::widgets::fit_text(
            &names_p,
            pos2(text_x, cell.center().y),
            Align2::LEFT_CENTER,
            &l.label,
            FontId::proportional(13.0),
            text_color,
            cell.right() - text_x - if dot { 22.0 } else { 4.0 },
        );
        if dot {
            names_p.circle(
                pos2(cell.right() - 12.0, cell.center().y),
                6.0,
                curve_color(l.shade),
                Stroke::new(1.0, Color32::BLACK),
            );
        }
        // A click picks the row; on a triangle it opens or closes it.
        let resp = ui.interact(cell.intersect(names), id.with(("name", i)), Sense::click());
        resp.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &l.label)
        });
        if resp.clicked() {
            let on_triangle = triangle
                && resp
                    .interact_pointer_pos()
                    .is_some_and(|p| (p.x - tri_x).abs() <= 9.0);
            if on_triangle && l.depth == 0 {
                view.open = !view.open;
            } else if on_triangle && let Some(p) = l.param {
                let name = rows[p].name.to_string();
                if !view.closed.remove(&name) {
                    view.closed.insert(name);
                }
            } else {
                view.row = l.row.clone();
            }
        }

        // Its lane (Lanes).
        if view.curves {
            continue;
        }
        let lane = Rect::from_min_max(
            pos2(graph.left(), row.top()),
            pos2(graph.right(), row.bottom()),
        );
        if leaf {
            graph_p.rect_filled(lane, 0.0, shade(l.shade));
        }
        let center = lane.center().y;
        match (l.param, l.part) {
            // The effect's row: a diamond at every keyframe's time.
            (None, _) => {
                for &t in &all_times {
                    let keys: Vec<(String, Tick)> = rows
                        .iter()
                        .filter(|r| r.param.keys.iter().any(|k| k.at == t))
                        .map(|r| (r.name.to_string(), t))
                        .collect();
                    let curve = rows
                        .iter()
                        .find_map(|r| r.param.keys.iter().find(|k| k.at == t).map(|k| k.curve))
                        .unwrap_or_default();
                    marks.push(Mark {
                        line: i,
                        at: t,
                        keys,
                        curve,
                        pos: pos2(x_of(t), center),
                        shape: Shape::Diamond,
                    });
                }
            }
            // A number's row: its value over time, within the row.
            (Some(p), Some(d)) => {
                let r = &rows[p];
                let span = key_span(r.param, d);
                let y_of = |v: f64| match span {
                    Some((a, b)) if b - a > 1e-12 => {
                        lane.bottom()
                            - LANE_PAD
                            - ((v - a) / (b - a)) as f32 * (ROW - 2.0 * LANE_PAD)
                    }
                    _ => center,
                };
                let anim = animation(r.param, rate);
                let pts: Vec<Pos2> = (0..=(graph.width() / 3.0) as usize)
                    .map(|k| {
                        let x = graph.left() + k as f32 * 3.0;
                        let v = anim.at(frames(raw_t(x), rate), r.ty).num(d);
                        pos2(x, y_of(v))
                    })
                    .collect();
                graph_p.add(egui::Shape::line(pts, Stroke::new(1.0, line_color)));
                for k in &r.param.keys {
                    let v = key_number(r.param, k.at, d).unwrap_or(0.0);
                    marks.push(Mark {
                        line: i,
                        at: k.at,
                        keys: vec![(r.name.to_string(), k.at)],
                        curve: k.curve,
                        pos: pos2(x_of(k.at), y_of(v)),
                        shape: Shape::Dot,
                    });
                }
            }
            // A parameter of several numbers (or of text): its keyframes.
            (Some(p), None) => {
                let r = &rows[p];
                for k in &r.param.keys {
                    marks.push(Mark {
                        line: i,
                        at: k.at,
                        keys: vec![(r.name.to_string(), k.at)],
                        curve: k.curve,
                        pos: pos2(x_of(k.at), center),
                        shape: Shape::Dot,
                    });
                }
            }
        }
    }
    // The empty part under the rows' names.
    let below = Rect::from_min_max(pos2(names.left(), line_top(lines.len())), names.max);
    if below.height() > 0.0 {
        names_p.rect_filled(below, 0.0, theme::panel());
    }

    // Curves: the numbers' values over time in one graph, the edited ones
    // drawn thick with their keyframes.
    if view.curves {
        let picked_row = view.row.clone();
        let emphasized = |l: &Line| -> bool {
            match (&picked_row, &l.row) {
                (Row::Param(a), Row::Param(b)) => a == b && !l.group,
                (Row::Param(a), Row::Part(b, _)) => a == b,
                (Row::Part(a, i), Row::Part(b, j)) => a == b && i == j,
                _ => false,
            }
        };
        for pass in [false, true] {
            for (i, l) in lines.iter().enumerate() {
                let (Some(p), Some(d)) = (l.param, l.part) else {
                    continue;
                };
                if emphasized(l) != pass {
                    continue;
                }
                let r = &rows[p];
                let range = r.ranges.get(d).copied().unwrap_or((0.0, 1.0));
                let anim = animation(r.param, rate);
                let pts: Vec<Pos2> = (0..=(graph.width() / 2.0) as usize)
                    .map(|k| {
                        let x = graph.left() + k as f32 * 2.0;
                        let v = anim.at(frames(raw_t(x), rate), r.ty).num(d);
                        pos2(x, graph_y(v, range))
                    })
                    .collect();
                let thick = if pass { 3.0 } else { 1.0 };
                graph_p.add(egui::Shape::line(
                    pts,
                    Stroke::new(thick, curve_color(l.shade)),
                ));
                if pass {
                    for (n, k) in r.param.keys.iter().enumerate() {
                        let v = key_number(r.param, k.at, d).unwrap_or(0.0);
                        let pos = pos2(x_of(k.at), graph_y(v, range));
                        // A Bézier keyframe's handles.
                        if k.curve.is_bezier()
                            && let Some(h) = handles(r.param, n, rate).get(d)
                        {
                            for (after, (dt, dv)) in [(false, h.before), (true, h.after)] {
                                handle_marks.push(HandleMark {
                                    name: r.name.to_string(),
                                    at: k.at,
                                    part: d,
                                    after,
                                    key: pos,
                                    pos: pos2(x_of(k.at + dt), graph_y(v + dv, range)),
                                    range,
                                    color: curve_color(l.shade),
                                });
                            }
                        }
                        marks.push(Mark {
                            line: i,
                            at: k.at,
                            keys: vec![(r.name.to_string(), k.at)],
                            curve: k.curve,
                            pos,
                            shape: Shape::Point,
                        });
                    }
                }
            }
        }
    }

    // A keyframe or a handle under the pointer (its pointer, `key_grab`).
    let mut over = false;
    // A Bézier keyframe's handles (Curves): lines out to dots, dragged to
    // shape the curve on each side of it.
    for (n, h) in handle_marks.iter().enumerate() {
        graph_p.line_segment(
            [h.key, h.pos],
            Stroke::new(1.0, h.color.gamma_multiply(0.6)),
        );
        let hit = Rect::from_center_size(h.pos, vec2(10.0, 10.0)).intersect(graph);
        let mut hot = false;
        if hit.is_positive() {
            let resp = ui.interact(hit, id.with(("handle", n, h.at.0, h.after)), Sense::drag());
            resp.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Other, true, tr("Bézier handle"))
            });
            if resp.drag_started()
                && let Some(r) = rows.iter().find(|r| r.name == h.name)
            {
                view.drag = Some(Drag::Handle(HandleDrag {
                    name: h.name.clone(),
                    at: h.at,
                    part: h.part,
                    after: h.after,
                    base: r.param.clone(),
                    range: h.range,
                }));
            }
            hot = resp.hovered() || resp.dragged();
            over |= resp.hovered();
        }
        let fill = if hot {
            h.color
        } else {
            mix(h.color, Color32::BLACK, 0.35)
        };
        graph_p.circle_filled(h.pos, if hot { 4.5 } else { 3.5 }, fill);
    }

    // The keyframes: a click picks one (with Ctrl or Shift, one more or
    // one less) and puts the cursor on it, a drag moves the ones picked,
    // and their menu edits them.
    for (n, m) in marks.iter().enumerate() {
        if m.pos.x < graph.left() - 8.0 || m.pos.x > graph.right() + 8.0 {
            continue;
        }
        let hit = Rect::from_center_size(m.pos, vec2(14.0, 16.0)).intersect(graph);
        if !hit.is_positive() {
            continue;
        }
        let line = &lines[m.line];
        let resp = ui.interact(hit, id.with(("key", n, m.at.0)), Sense::click_and_drag());
        resp.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                trf!(
                    "Keyframe of {} at {}",
                    line.label,
                    bv_core::timecode::format(m.at - lo, rate)
                ),
            )
        });
        let picked = !m.keys.is_empty() && m.keys.iter().all(|k| view.picked.contains(k));
        paint_mark(&graph_p, m, picked, resp.hovered());
        over |= resp.hovered();
        if resp.clicked() {
            if adding {
                if picked {
                    for k in &m.keys {
                        view.picked.remove(k);
                    }
                } else {
                    view.picked.extend(m.keys.iter().cloned());
                }
            } else {
                view.picked = m.keys.iter().cloned().collect();
                out.seek = Some(m.at);
            }
            if !view.curves {
                view.row = line.row.clone();
            }
        }
        if resp.secondary_clicked() && !picked {
            view.picked = m.keys.iter().cloned().collect();
        }
        if resp.drag_started() {
            if !picked {
                if !adding {
                    view.picked.clear();
                }
                view.picked.extend(m.keys.iter().cloned());
            }
            let base = rows
                .iter()
                .filter(|r| view.picked.iter().any(|(n, _)| n == r.name))
                .map(|r| (r.name.to_string(), r.param.keys.clone()))
                .collect();
            let value = match (m.shape, line.param, line.part) {
                (Shape::Point, Some(p), Some(d)) => Some((
                    rows[p].name.to_string(),
                    d,
                    graph,
                    rows[p].ranges.get(d).copied().unwrap_or((0.0, 1.0)),
                )),
                _ => None,
            };
            let press = ui.input(|i| i.pointer.press_origin()).unwrap_or(m.pos);
            view.drag = Some(Drag::Keys(KeyDrag {
                at: m.at,
                grip: press - m.pos,
                picked: view.picked.iter().cloned().collect(),
                base,
                copy: mods.command,
                space: mods.alt,
                value,
            }));
            if !view.curves {
                view.row = line.row.clone();
            }
        }
        resp.context_menu(|ui| {
            key_menu(ui, view, &mut out, m, line, rows, place, clip);
        });
    }

    // What is being dragged.
    let (down, released, pointer) = ui.input(|i| {
        (
            i.pointer.primary_down(),
            i.pointer.primary_released(),
            i.pointer.interact_pos(),
        )
    });
    let mut band = None;
    match (view.drag.clone(), pointer) {
        (Some(Drag::Handle(h)), Some(p)) if down => {
            // The handle where the pointer is, from its keyframe in time and
            // value (past the graph's span and range too).
            let f = ((p.x - lanes.left()) / lanes.width()) as f64 / zoom + start;
            let t = lo + Tick((f * total) as i64);
            let (a, b) = h.range;
            let up =
                ((graph.bottom() - GRAPH_PAD - p.y) / (graph.height() - 2.0 * GRAPH_PAD)) as f64;
            let mut param = h.base.clone();
            if let Some(i) = param.keys.iter().position(|k| k.at == h.at)
                && let Some(v) = key_number(&param, h.at, h.part)
            {
                let to = (t - h.at, a + up * (b - a) - v);
                set_handle(&mut param, i, h.part, h.after, to, rate);
                out.edits.push(KeyEdit::Keys(h.name.clone(), param.keys));
            }
        }
        (Some(Drag::Keys(d)), Some(p)) if down => {
            // Where the keyframe taken is (not the pointer: it took it
            // off its middle).
            let p = p - d.grip;
            let to = t_of(p.x);
            let first = d.picked.iter().map(|k| k.1).min().unwrap_or(d.at);
            let last = d.picked.iter().map(|k| k.1).max().unwrap_or(d.at);
            let by = (to - d.at).clamp(lo - first, hi - last);
            let mut times: Vec<Tick> = d.picked.iter().map(|k| k.1).collect();
            times.sort();
            times.dedup();
            let goes = |t: Tick| moved_time(t, &times, d.at, by, d.space, rate);
            for (name, keys) in &d.base {
                let moves: Vec<(Tick, Tick)> = d
                    .picked
                    .iter()
                    .filter(|(n, _)| n == name)
                    .map(|&(_, t)| (t, goes(t)))
                    .collect();
                let mut keys = retimed(keys, &moves, d.copy);
                // Curves: the keyframe taken goes up and down with its value.
                if let Some((vn, part, g, (a, b))) = &d.value
                    && vn == name
                    && let Some(k) = keys.iter_mut().find(|k| k.at == goes(d.at))
                    && let PluginValue::Num(v) = &mut k.value
                    && let Some(x) = v.get_mut(*part)
                {
                    let f = ((g.bottom() - GRAPH_PAD - p.y) / (g.height() - 2.0 * GRAPH_PAD))
                        .clamp(0.0, 1.0) as f64;
                    *x = a + f * (b - a);
                    if let Some(r) = rows.iter().find(|r| r.name == name)
                        && r.ty.is_int()
                    {
                        *x = x.round();
                    }
                }
                out.edits.push(KeyEdit::Keys(name.clone(), keys));
            }
            view.picked = d
                .picked
                .iter()
                .map(|(n, t)| (n.clone(), goes(*t)))
                .collect();
            out.seek = Some(goes(d.at));
        }
        (Some(Drag::Band { from, add }), Some(p)) if down || released => {
            let to = raw_t(p.x);
            out.seek = Some(t_of(p.x));
            band = Some((from, to));
            if released {
                let inside = keys_between(rows, from.min(to), from.max(to));
                if !add {
                    view.picked.clear();
                }
                view.picked.extend(inside);
                view.drag = None;
            }
        }
        (Some(Drag::Cursor), Some(p)) if down => out.seek = Some(t_of(p.x)),
        _ => view.drag = None,
    }
    if let Some(g) = key_grab(view.drag.as_ref(), over) {
        g.show(ui.ctx());
    }
    // The box, over all the rows.
    if let Some((a, b)) = band {
        let (x0, x1) = (x_of(a.min(b)), x_of(a.max(b)));
        graph_p.rect_filled(
            Rect::from_min_max(pos2(x0, graph.top()), pos2(x1, graph.bottom())),
            0.0,
            theme::selection().gamma_multiply(0.25),
        );
    }
    // The cursor.
    if (lo..=hi).contains(&place.time) {
        let x = x_of(place.time);
        if (lanes.left()..=lanes.right()).contains(&x) {
            painter.vline(
                x,
                ruler.top() + 2.0..=graph.bottom(),
                Stroke::new(1.0, theme::cursor()),
            );
            let m = Rect::from_center_size(pos2(x, ruler.top() + 7.0), vec2(7.0, 11.0));
            painter.rect_filled(m, 1.0, theme::cursor());
        }
    }

    // The keyboard: Ctrl+Left and Ctrl+Right go to the keyframe before and
    // after the cursor, Delete deletes the keyframes picked, Ctrl+X, C and
    // V cut, copy and paste them.
    let now = place.time;
    if !ui.ctx().egui_wants_keyboard_input() {
        let ctrl = egui::Modifiers::COMMAND;
        if ui.input_mut(|i| i.consume_key(ctrl, egui::Key::ArrowLeft)) {
            out.seek = all_times
                .iter()
                .rev()
                .find(|&&a| a < now)
                .copied()
                .or(out.seek);
        }
        if ui.input_mut(|i| i.consume_key(ctrl, egui::Key::ArrowRight)) {
            out.seek = all_times.iter().find(|&&a| a > now).copied().or(out.seek);
        }
        if !view.picked.is_empty()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
        {
            out.edits
                .push(KeyEdit::Remove(view.picked.iter().cloned().collect()));
        }
        let (mut copy, mut cut, mut paste) = (false, false, false);
        ui.input(|i| {
            for e in &i.events {
                match e {
                    egui::Event::Copy => copy = true,
                    egui::Event::Cut => cut = true,
                    egui::Event::Paste(_) => {
                        paste = true;
                        view.v_down = true;
                    }
                    egui::Event::Key {
                        key: egui::Key::V,
                        pressed,
                        modifiers,
                        ..
                    } => {
                        if *pressed {
                            view.v_down = true;
                        } else if !std::mem::take(&mut view.v_down) && modifiers.command {
                            paste = true;
                        }
                    }
                    _ => {}
                }
            }
        });
        if (copy || cut) && !view.picked.is_empty() {
            copy_keys(view, rows, clip);
            if cut {
                out.edits
                    .push(KeyEdit::Remove(view.picked.iter().cloned().collect()));
            }
        }
        if paste && !clip.is_empty() {
            out.edits.push(paste_keys(clip, now, hi));
        }
    }

    // "Set to…": a keyframe's value, typed.
    set_to(ui, view, id, rows, &mut out);

    // The bars that scroll the rows up and down and the time across, and
    // zoom in and out.
    vertical_bar(ui, view, id, vbar, content, graph.height(), max_scroll);
    hscroll(ui, view, id, hbar, max_zoom);

    // The buttons: the cursor with the timeline's, going between
    // keyframes, making and deleting them; the views.
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(buttons.shrink2(vec2(0.0, 3.0))),
        |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.add_space((names_w - 60.0).max(4.0));
                let button = |ui: &mut Ui, icon, on, enabled, tip: &str| {
                    crate::widgets::icon_button_ex(ui, icon, tip, on, enabled).clicked()
                };
                if button(ui, Icon::Lock, view.sync, true, tr("Sync Cursor")) {
                    view.sync = !view.sync;
                }
                ui.add_space(10.0);
                if button(
                    ui,
                    Icon::GoStart,
                    false,
                    !all_times.is_empty(),
                    tr("First Keyframe"),
                ) {
                    out.seek = all_times.first().copied();
                }
                if button(
                    ui,
                    Icon::PrevFrame,
                    false,
                    all_times.iter().any(|&a| a < now),
                    tr("Previous Keyframe"),
                ) {
                    out.seek = all_times.iter().rev().find(|&&a| a < now).copied();
                }
                if button(
                    ui,
                    Icon::NextFrame,
                    false,
                    all_times.iter().any(|&a| a > now),
                    tr("Next Keyframe"),
                ) {
                    out.seek = all_times.iter().find(|&&a| a > now).copied();
                }
                if button(
                    ui,
                    Icon::GoEnd,
                    false,
                    !all_times.is_empty(),
                    tr("Last Keyframe"),
                ) {
                    out.seek = all_times.last().copied();
                }
                ui.add_space(10.0);
                let every = rows
                    .iter()
                    .all(|r| r.param.keys.iter().any(|k| k.at == now));
                if button(
                    ui,
                    Icon::Plus,
                    false,
                    !every,
                    tr("Create Keyframe at Cursor"),
                ) {
                    out.edits.push(KeyEdit::Add(now));
                }
                // The keyframes picked, or those at the cursor.
                let doomed: Vec<(String, Tick)> = if view.picked.is_empty() {
                    keys_between(rows, now, now)
                } else {
                    view.picked.iter().cloned().collect()
                };
                if button(
                    ui,
                    Icon::Delete,
                    false,
                    !doomed.is_empty(),
                    tr("Delete Keyframe"),
                ) {
                    out.edits.push(KeyEdit::Remove(doomed));
                }
                ui.add_space(10.0);
                egui::Frame::new()
                    .fill(theme::panel_dark())
                    .inner_margin(egui::Margin::symmetric(8, 2))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(bv_core::timecode::format(now - lo, rate))
                                .monospace()
                                .color(theme::text_strong()),
                        );
                    });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if !nested {
                        // Only the effect's row: no views to pick, the
                        // buttons blank, as VEGAS's.
                        no_views(ui);
                        return;
                    }
                    // As VEGAS's: the view to go to is the one boxed.
                    if view_button(ui, tr("Curves"), !view.curves) {
                        view.curves = true;
                    }
                    if view_button(ui, tr("Lanes"), view.curves) {
                        view.curves = false;
                    }
                });
            });
        },
    );
    out
}

/// A Lanes/Curves button: boxed when it is the view to go to; clicked or
/// not.
fn view_button(ui: &mut Ui, text: &str, boxed: bool) -> bool {
    let button = egui::Button::new(egui::RichText::new(text).color(theme::text()))
        .fill(if boxed {
            theme::separator()
        } else {
            Color32::TRANSPARENT
        })
        .stroke(Stroke::NONE)
        .min_size(vec2(64.0, 24.0));
    ui.add(button).clicked()
}

/// The Lanes/Curves buttons with nothing to pick: one blank box, split.
fn no_views(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(2.0 * 64.0 + 4.0, 24.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, theme::separator());
    p.vline(
        rect.center().x,
        rect.y_range(),
        Stroke::new(1.0, theme::panel()),
    );
}

/// Puts the keyframes picked in `clip`, their times counted from the
/// first's.
fn copy_keys(view: &KeyView, rows: &[KeyRow<'_>], clip: &mut Vec<(String, PluginKey)>) {
    let Some(first) = view.picked.iter().map(|k| k.1).min() else {
        return;
    };
    clip.clear();
    for (name, at) in &view.picked {
        if let Some(k) = rows
            .iter()
            .find(|r| r.name == name)
            .and_then(|r| r.param.keys.iter().find(|k| k.at == *at))
        {
            clip.push((
                name.clone(),
                PluginKey {
                    at: *at - first,
                    ..k.clone()
                },
            ));
        }
    }
}

/// The keyframes of `clip` put in from `at` (none after `end`).
fn paste_keys(clip: &[(String, PluginKey)], at: Tick, end: Tick) -> KeyEdit {
    KeyEdit::Insert(
        clip.iter()
            .filter(|(_, k)| at + k.at <= end)
            .map(|(n, k)| {
                (
                    n.clone(),
                    PluginKey {
                        at: at + k.at,
                        ..k.clone()
                    },
                )
            })
            .collect(),
    )
}

/// A keyframe's menu, as VEGAS's: delete, cut, copy, paste, "Set to…" and
/// the interpolation curves — the effect row's only the first three.
#[allow(clippy::too_many_arguments)]
fn key_menu(
    ui: &mut Ui,
    view: &mut KeyView,
    out: &mut Answer,
    m: &Mark,
    line: &Line,
    rows: &[KeyRow<'_>],
    place: &FxPlace,
    clip: &mut Vec<(String, PluginKey)>,
) {
    let picked: Vec<(String, Tick)> = view.picked.iter().cloned().collect();
    let own = line.param.is_some();
    if menus::item(ui, tr("Delete")) {
        out.edits.push(KeyEdit::Remove(picked.clone()));
        ui.close();
    }
    if menus::item(ui, tr("Cut")) {
        copy_keys(view, rows, clip);
        out.edits.push(KeyEdit::Remove(picked.clone()));
        ui.close();
    }
    if menus::item(ui, tr("Copy")) {
        copy_keys(view, rows, clip);
        ui.close();
    }
    let paste = ui
        .add_enabled_ui(!clip.is_empty(), |ui| menus::item(ui, tr("Paste")))
        .inner;
    if paste {
        out.edits.push(paste_keys(clip, place.time, place.span.1));
        ui.close();
    }
    ui.separator();
    // A value typed: numbers only (not text, nor curves).
    let numeric = line
        .param
        .is_some_and(|p| matches!(rows[p].param.value, PluginValue::Num(_)));
    let set = ui
        .add_enabled_ui(numeric, |ui| menus::item(ui, tr("Set to…")))
        .inner;
    if set
        && let Some(p) = line.param
        && let Some(k) = rows[p].param.keys.iter().find(|k| k.at == m.at)
        && let PluginValue::Num(v) = &k.value
    {
        let scale = &rows[p].scale;
        let shown: Vec<f64> = v
            .iter()
            .enumerate()
            .map(|(i, x)| x / scale.get(i).copied().unwrap_or(1.0))
            .collect();
        view.set_to = Some(SetTo {
            name: rows[p].name.to_string(),
            at: m.at,
            text: crate::ofx::numbers_text(&shown, 3),
            pos: m.pos + vec2(8.0, 8.0),
            focus: true,
        });
        ui.close();
    }
    ui.separator();
    ui.add_enabled_ui(own, |ui| {
        for (curve, label) in CURVES {
            if menus::check_item(ui, tr(label), m.curve == curve, true, "") {
                out.edits.push(KeyEdit::Curve(picked.clone(), curve));
                ui.close();
            }
        }
    });
    ui.separator();
    ui.add_enabled_ui(own && numeric, |ui| {
        for (curve, label) in BEZIERS {
            if menus::check_item(ui, tr(label), m.curve == curve, true, "") {
                out.edits.push(KeyEdit::Curve(picked.clone(), curve));
                ui.close();
            }
        }
    });
}

/// "Set to…": a small box where a keyframe's value is typed (Enter or OK
/// sets it, Escape or Cancel leaves it).
fn set_to(ui: &mut Ui, view: &mut KeyView, id: egui::Id, rows: &[KeyRow<'_>], out: &mut Answer) {
    let Some(s) = &mut view.set_to else {
        return;
    };
    let Some(row) = rows.iter().find(|r| r.name == s.name) else {
        view.set_to = None;
        return;
    };
    let (mut done, mut commit) = (false, false);
    egui::Area::new(id.with("set-to"))
        .order(egui::Order::Foreground)
        .fixed_pos(s.pos)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(format!("{}:", row.label));
                let field = ui.add(egui::TextEdit::singleline(&mut s.text).desired_width(150.0));
                if std::mem::take(&mut s.focus) {
                    field.request_focus();
                }
                if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    commit = true;
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    done = true;
                }
                ui.horizontal(|ui| {
                    if ui.button(tr("OK")).clicked() {
                        commit = true;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        done = true;
                    }
                });
            });
        });
    if commit && let Some(v) = crate::ofx::parse_numbers(&s.text, row.scale.len().max(1)) {
        let v: Vec<f64> = v
            .iter()
            .enumerate()
            .map(|(i, x)| {
                let x = x * row.scale.get(i).copied().unwrap_or(1.0);
                if row.ty.is_int() { x.round() } else { x }
            })
            .collect();
        out.edits.push(KeyEdit::Value(s.name.clone(), s.at, v));
        done = true;
    }
    if done {
        view.set_to = None;
    }
}

/// The bar beside the rows that scrolls them up and down.
fn vertical_bar(
    ui: &mut Ui,
    view: &mut KeyView,
    id: egui::Id,
    bar: Rect,
    content: f32,
    shown: f32,
    max_scroll: f32,
) {
    let p = ui.painter_at(bar);
    p.rect_filled(bar, 0.0, theme::panel_dark());
    let w = bar.width();
    let up = Rect::from_min_size(bar.min, vec2(w, w));
    let down = Rect::from_min_size(pos2(bar.left(), bar.bottom() - w), vec2(w, w));
    let track = Rect::from_min_max(pos2(bar.left(), up.bottom()), pos2(bar.right(), down.top()));
    for (r, dir, salt) in [(up, -1.0, "up"), (down, 1.0, "down")] {
        let resp = ui.interact(r, id.with(salt), Sense::click());
        let color = if resp.hovered() && max_scroll > 0.0 {
            theme::text_strong()
        } else {
            theme::text_dim()
        };
        let c = r.center();
        p.add(egui::Shape::convex_polygon(
            vec![
                c + vec2(-4.0, -3.0 * dir),
                c + vec2(4.0, -3.0 * dir),
                c + vec2(0.0, 3.0 * dir),
            ],
            color,
            Stroke::NONE,
        ));
        if resp.clicked() {
            view.scroll = (view.scroll + dir * ROW).clamp(0.0, max_scroll);
        }
    }
    let frac = if content > 0.0 {
        (shown / content).min(1.0)
    } else {
        1.0
    };
    let th = (track.height() * frac)
        .max(crate::widgets::MIN_THUMB.min(track.height()))
        .min(track.height());
    let room = track.height() - th;
    let at = if max_scroll > 0.0 {
        view.scroll / max_scroll * room
    } else {
        0.0
    };
    let thumb = Rect::from_min_size(
        pos2(track.left() + 2.0, track.top() + at),
        vec2(track.width() - 4.0, th),
    );
    let resp = ui.interact(track, id.with("vscroll"), Sense::drag());
    if resp.dragged() && room > 0.0 {
        view.scroll =
            (view.scroll + resp.drag_delta().y / room * max_scroll).clamp(0.0, max_scroll);
    }
    p.rect_filled(
        thumb,
        3.0,
        if resp.dragged() || resp.hovered() {
            theme::text_dim()
        } else {
            theme::separator()
        },
    );
    // Its grip.
    for k in [-3.0, 0.0, 3.0] {
        let y = thumb.center().y + k;
        p.hline(
            thumb.left() + 3.0..=thumb.right() - 3.0,
            y,
            Stroke::new(1.0, theme::panel_dark()),
        );
    }
}

/// The bar under the lanes: ◀ the scroll bar ▶, + and −. Its thumb's
/// ends zoom, as the timeline's.
fn hscroll(ui: &mut Ui, view: &mut KeyView, id: egui::Id, bar: Rect, max_zoom: f64) {
    let p = ui.painter_at(bar);
    p.rect_filled(bar, 0.0, theme::panel_dark());
    let h = bar.height();
    let left = Rect::from_min_size(bar.min, vec2(h, h));
    let minus = Rect::from_min_size(pos2(bar.right() - h, bar.top()), vec2(h, h));
    let plus = minus.translate(vec2(-h - 2.0, 0.0));
    let right = plus.translate(vec2(-h - 2.0, 0.0));
    let track = Rect::from_min_max(
        pos2(left.right(), bar.top()),
        pos2(right.left(), bar.bottom()),
    );
    if let Some((start, len)) = crate::widgets::zoom_scrollbar(
        ui,
        track,
        id.with("thumb"),
        (view.start, 1.0 / view.zoom),
        1.0,
        (1.0 / max_zoom, 1.0),
    ) {
        view.zoom = (1.0 / len).clamp(1.0, max_zoom);
        view.start = start.clamp(0.0, 1.0 - 1.0 / view.zoom);
    }
    let small =
        |ui: &mut Ui, r: Rect, salt: &str, glyph: &dyn Fn(&egui::Painter, Pos2, Color32)| {
            let resp = ui.interact(r, id.with(salt), Sense::click());
            let color = if resp.hovered() {
                theme::text_strong()
            } else {
                theme::text_dim()
            };
            glyph(&p, r.center(), color);
            resp.clicked()
        };
    let arrow = |dir: f32| {
        move |p: &egui::Painter, c: Pos2, col: Color32| {
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + vec2(-3.0 * dir, -4.0),
                    c + vec2(3.0 * dir, 0.0),
                    c + vec2(-3.0 * dir, 4.0),
                ],
                col,
                Stroke::NONE,
            ));
        }
    };
    let page = 0.25 / view.zoom;
    if small(ui, left, "left", &arrow(-1.0)) {
        view.start = (view.start - page).max(0.0);
    }
    if small(ui, right, "right", &arrow(1.0)) {
        view.start = (view.start + page).min(1.0 - 1.0 / view.zoom);
    }
    let sign = |plus: bool| {
        move |p: &egui::Painter, c: Pos2, col: Color32| {
            let s = Stroke::new(1.6, col);
            p.line_segment([c + vec2(-4.0, 0.0), c + vec2(4.0, 0.0)], s);
            if plus {
                p.line_segment([c + vec2(0.0, -4.0), c + vec2(0.0, 4.0)], s);
            }
        }
    };
    let zoom_by = |k: f64, view: &mut KeyView| {
        let middle = view.start + 0.5 / view.zoom;
        view.zoom = (view.zoom * k).clamp(1.0, max_zoom);
        view.start = (middle - 0.5 / view.zoom).clamp(0.0, 1.0 - 1.0 / view.zoom);
    };
    if small(ui, plus, "zoom in", &sign(true)) {
        zoom_by(1.5, view);
    }
    if small(ui, minus, "zoom out", &sign(false)) {
        zoom_by(1.0 / 1.5, view);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: i64, x: f64) -> PluginKey {
        PluginKey {
            at: Tick::from_seconds(s),
            value: PluginValue::Num(vec![x]),
            curve: KeyCurve::Linear,
            handles: Vec::new(),
        }
    }

    #[test]
    fn keyframes_move_and_copy_over_others() {
        let s = Tick::from_seconds;
        let keys = [key(0, 0.0), key(1, 1.0), key(2, 2.0)];
        // 0 and 1 a second later: 1 goes to 2, over the keyframe there.
        let moved = retimed(&keys, &[(s(0), s(1)), (s(1), s(2))], false);
        assert_eq!(
            moved,
            vec![
                PluginKey {
                    at: s(1),
                    ..key(0, 0.0)
                },
                PluginKey {
                    at: s(2),
                    ..key(1, 1.0)
                }
            ]
        );
        // Copied: the first stays.
        let copied = retimed(&keys, &[(s(0), s(3))], true);
        assert_eq!(copied.len(), 4);
        assert_eq!(copied[3].value, PluginValue::Num(vec![0.0]));
        assert_eq!(copied[0], key(0, 0.0));
    }

    #[test]
    fn alt_spaces_a_group_out_from_its_end() {
        let rate = FrameRate::FPS_30;
        let s = Tick::from_seconds;
        let times = [s(0), s(1), s(5)];
        // The last one dragged to 4 s: the middle one goes halfway.
        let to = |t| moved_time(t, &times, s(5), s(-1), true, rate);
        assert_eq!(times.map(to), [s(0), s(2), s(4)]);
        // Without Alt (or from the middle) they all move.
        let plain = |t| moved_time(t, &times, s(5), s(-1), false, rate);
        assert_eq!(times.map(plain), [s(-1), s(0), s(4)]);
        let middle = |t| moved_time(t, &times, s(1), s(1), true, rate);
        assert_eq!(times.map(middle), [s(1), s(2), s(6)]);
    }

    #[test]
    fn a_position_has_a_row_of_its_own_for_each_number() {
        let one = PluginParam {
            value: PluginValue::Num(vec![1.0]),
            keys: vec![key(0, 1.0)],
        };
        let two = PluginParam {
            value: PluginValue::Num(vec![0.5, 0.5]),
            keys: Vec::new(),
        };
        let row = |name, param, ty: ParamType| KeyRow {
            name,
            label: name.to_string(),
            param,
            ty,
            ranges: vec![(0.0, 1.0); ty.dimension()],
            scale: vec![1.0; ty.dimension()],
            parts: Vec::new(),
        };
        let rows = [
            row("x", &one, ParamType::Double),
            row("at", &two, ParamType::Double2D),
            row("y", &one, ParamType::Double),
        ];
        let mut view = KeyView::default();
        let shown = |view: &KeyView| {
            lines(view, "Crop", &rows)
                .iter()
                .map(|l| (l.label.clone(), l.depth, l.shade))
                .collect::<Vec<_>>()
        };
        let row_of = |l: &str, d, s| (l.to_string(), d, s);
        assert_eq!(
            shown(&view),
            [
                row_of("Crop", 0, 0),
                row_of("x", 1, 0),
                row_of("at", 1, 0),
                row_of("X", 2, 1),
                row_of("Y", 2, 2),
                row_of("y", 1, 3),
            ]
        );
        // Closed, the position's numbers go; the colors stay.
        view.closed.insert("at".into());
        assert_eq!(
            shown(&view),
            [
                row_of("Crop", 0, 0),
                row_of("x", 1, 0),
                row_of("at", 1, 0),
                row_of("y", 1, 3),
            ]
        );
        view.open = false;
        assert_eq!(shown(&view), [row_of("Crop", 0, 0)]);
    }

    /// The hand over a keyframe or a handle; dragged, keyframes along the
    /// time show the system's ↔ (in Curves, where the one taken follows
    /// the pointer up and down too, its four arrows, as a handle's); the
    /// cursor and the picking box none.
    #[test]
    fn the_pointer_says_how_a_keyframe_or_a_handle_moves() {
        let keys = |value| {
            Drag::Keys(KeyDrag {
                at: Tick::ZERO,
                grip: egui::Vec2::ZERO,
                picked: Vec::new(),
                base: Vec::new(),
                copy: false,
                space: false,
                value,
            })
        };
        assert_eq!(key_grab(None, true), Some(Grab::Over));
        assert_eq!(key_grab(None, false), None);
        assert_eq!(key_grab(Some(&keys(None)), false), Some(Grab::Across));
        let graph = Some(("x".to_string(), 0, Rect::ZERO, (0.0, 1.0)));
        assert_eq!(key_grab(Some(&keys(graph)), true), Some(Grab::Free));
        let handle = Drag::Handle(HandleDrag {
            name: "x".into(),
            at: Tick::ZERO,
            part: 0,
            after: true,
            base: PluginParam {
                value: PluginValue::Num(vec![0.0]),
                keys: Vec::new(),
            },
            range: (0.0, 1.0),
        });
        assert_eq!(key_grab(Some(&handle), false), Some(Grab::Free));
        assert_eq!(key_grab(Some(&Drag::Cursor), true), None);
        let band = Drag::Band {
            from: Tick::ZERO,
            add: false,
        };
        assert_eq!(key_grab(Some(&band), true), None);
    }
}
