//! The Pan/Crop window's keyframe controller, as VEGAS 23's (the user's
//! prints): a ruler over the event's time and a little past its end — what
//! is past it white —, the Position and Mask rows, VEGAS's needle (a light
//! head on the ruler, an orange line down the rows), its keyframes as
//! VEGAS's diamonds (picked ones white in a grey ring), a scroll bar for
//! the rows at the right and one for the time under them (its thumb's
//! ends zoom; + and −; the wheel zooms round the pointer). Keyframes are
//! picked with a click, Ctrl and a click (one more or one less), Shift and
//! a click (a run in its row), by dragging the needle across them from an
//! empty spot of a row, or with Ctrl+A; dragged together — the first one
//! too — (with Ctrl, copies of them); and cut, copied, pasted at the
//! needle, deleted and given a curve all together, from their menu (Cut,
//! Copy, Paste, Delete and the curves) or the keys (Ctrl+X, C, V, Delete).

use std::collections::BTreeSet;

use bv_core::{FrameRate, Tick, timecode};
use bv_timeline::mask::MaskKey;
use bv_timeline::{Clip, KeyCurve, TransformKey};

use crate::egui::{
    self, Align, Align2, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui,
    pos2, vec2,
};
use crate::i18n::tr;
use crate::icons::Icon;
use crate::ofx_keys::mix;
use crate::theme;

/// A row of the controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum KeyRow {
    /// The framing (the box).
    Position,
    /// VEGAS 2026's masks: their shapes over time.
    Mask,
}

const ROWS: [KeyRow; 2] = [KeyRow::Position, KeyRow::Mask];
/// The ruler's height, a row's (its line under it included), the scroll
/// bars' thickness, the buttons' bar and the rows' names.
const RULER: f32 = 17.0;
const ROW: f32 = 19.0;
const BAR: f32 = 16.0;
const BUTTONS: f32 = 30.0;
const NAMES: f32 = 140.0;
/// How far the view goes past the event's end at first (a part of its
/// length): VEGAS shows that part white.
const PAST_END: f64 = 0.025;
/// How near (pixels) the pointer takes a keyframe.
const GRAB: f32 = 8.0;
/// VEGAS's colors (its dark interface's, the same on every interface):
/// the needle's line and head, what is past the event's end, keyframes.
const NEEDLE: Color32 = Color32::from_rgb(0xF0, 0x8C, 0x28);
const NEEDLE_HEAD: Color32 = Color32::from_gray(240);
const PAST: Color32 = Color32::from_gray(240);
const KEY: Color32 = Color32::from_gray(192);

/// The height the controller takes.
pub(crate) fn height() -> f32 {
    RULER + ROWS.len() as f32 * ROW + 2.0 + BAR + BUTTONS
}

/// A keyframe as its row keeps it: its time and curve.
trait Timed: Clone {
    fn at(&self) -> Tick;
    fn set_at(&mut self, at: Tick);
    fn set_curve(&mut self, curve: KeyCurve);
}

impl Timed for TransformKey {
    fn at(&self) -> Tick {
        self.at
    }
    fn set_at(&mut self, at: Tick) {
        self.at = at;
    }
    fn set_curve(&mut self, curve: KeyCurve) {
        self.curve = curve;
    }
}

impl Timed for MaskKey {
    fn at(&self) -> Tick {
        self.at
    }
    fn set_at(&mut self, at: Tick) {
        self.at = at;
    }
    fn set_curve(&mut self, curve: KeyCurve) {
        self.curve = curve;
    }
}

/// The rows' keyframes as the controller edits them (their times from the
/// event's start).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Keys {
    pub position: Vec<TransformKey>,
    pub mask: Vec<MaskKey>,
}

/// A keyframe Ctrl+C, Ctrl+X or Copy took: its row, its time from the
/// first one taken, its value.
#[derive(Clone, Debug)]
pub(crate) enum Copied {
    Position(TransformKey),
    Mask(MaskKey),
}

impl Copied {
    fn row(&self) -> KeyRow {
        match self {
            Copied::Position(_) => KeyRow::Position,
            Copied::Mask(_) => KeyRow::Mask,
        }
    }
}

impl Keys {
    /// The event's keyframes. A row without any whose value is set (the
    /// framing; masks that have paths) shows that value as one at the
    /// start, VEGAS's first keyframe: it is made when edited.
    pub(crate) fn of(clip: &Clip, curve: KeyCurve) -> Keys {
        let position = if clip.transform_keys.is_empty() {
            vec![TransformKey::new(Tick::ZERO, clip.transform, curve)]
        } else {
            clip.transform_keys.clone()
        };
        let mask = match &clip.mask {
            Some(m) if m.keys.is_empty() && !m.paths.is_empty() => vec![MaskKey {
                at: Tick::ZERO,
                shapes: m.paths.iter().map(|p| p.anchors.clone()).collect(),
                curve: KeyCurve::Smooth,
            }],
            Some(m) => m.keys.clone(),
            None => Vec::new(),
        };
        Keys { position, mask }
    }

    /// The times of `row`'s keyframes, with their curves.
    fn times(&self, row: KeyRow) -> Vec<(Tick, KeyCurve)> {
        match row {
            KeyRow::Position => self.position.iter().map(|k| (k.at, k.curve)).collect(),
            KeyRow::Mask => self.mask.iter().map(|k| (k.at, k.curve)).collect(),
        }
    }

    fn all(&self) -> BTreeSet<(KeyRow, Tick)> {
        ROWS.iter()
            .flat_map(|&r| self.times(r).into_iter().map(move |(t, _)| (r, t)))
            .collect()
    }

    /// The keyframes `picked` moved by `delta` (copies of them, with
    /// `copy`), taking the place of others where they land; and where they
    /// went.
    pub(crate) fn moved(
        &self,
        picked: &BTreeSet<(KeyRow, Tick)>,
        delta: Tick,
        copy: bool,
    ) -> (Keys, BTreeSet<(KeyRow, Tick)>) {
        let of = |r: KeyRow| -> BTreeSet<Tick> {
            picked
                .iter()
                .filter(|(row, _)| *row == r)
                .map(|(_, t)| *t)
                .collect()
        };
        let keys = Keys {
            position: retimed(&self.position, &of(KeyRow::Position), delta, copy),
            mask: retimed(&self.mask, &of(KeyRow::Mask), delta, copy),
        };
        let went = picked.iter().map(|&(r, t)| (r, t + delta)).collect();
        (keys, went)
    }

    /// The keyframes `picked` deleted; a row keeps one at least (its first
    /// left, or its first).
    pub(crate) fn deleted(&self, picked: &BTreeSet<(KeyRow, Tick)>) -> Keys {
        fn gone<K: Timed>(keys: &[K], row: KeyRow, picked: &BTreeSet<(KeyRow, Tick)>) -> Vec<K> {
            let left: Vec<K> = keys
                .iter()
                .filter(|k| !picked.contains(&(row, k.at())))
                .cloned()
                .collect();
            if left.is_empty() {
                keys.iter().take(1).cloned().collect()
            } else {
                left
            }
        }
        Keys {
            position: gone(&self.position, KeyRow::Position, picked),
            mask: gone(&self.mask, KeyRow::Mask, picked),
        }
    }

    /// The keyframes `picked` given `curve` (to the next one).
    pub(crate) fn curved(&self, picked: &BTreeSet<(KeyRow, Tick)>, curve: KeyCurve) -> Keys {
        fn set<K: Timed>(
            keys: &[K],
            row: KeyRow,
            p: &BTreeSet<(KeyRow, Tick)>,
            c: KeyCurve,
        ) -> Vec<K> {
            keys.iter()
                .cloned()
                .map(|mut k| {
                    if p.contains(&(row, k.at())) {
                        k.set_curve(c);
                    }
                    k
                })
                .collect()
        }
        Keys {
            position: set(&self.position, KeyRow::Position, picked, curve),
            mask: set(&self.mask, KeyRow::Mask, picked, curve),
        }
    }

    /// The keyframes `picked`, their times from the first's.
    pub(crate) fn copied(&self, picked: &BTreeSet<(KeyRow, Tick)>) -> Vec<(Tick, Copied)> {
        let first = picked.iter().map(|(_, t)| *t).min().unwrap_or(Tick::ZERO);
        let mut out: Vec<(Tick, Copied)> = Vec::new();
        for k in &self.position {
            if picked.contains(&(KeyRow::Position, k.at)) {
                out.push((k.at - first, Copied::Position(*k)));
            }
        }
        for k in &self.mask {
            if picked.contains(&(KeyRow::Mask, k.at)) {
                out.push((k.at - first, Copied::Mask(k.clone())));
            }
        }
        out
    }

    /// `copied` put back from `at` (no later than `last`), each in its
    /// row, taking the place of keyframes there; and where they went.
    pub(crate) fn pasted(
        &self,
        copied: &[(Tick, Copied)],
        at: Tick,
        last: Tick,
    ) -> (Keys, BTreeSet<(KeyRow, Tick)>) {
        let mut keys = self.clone();
        let mut went = BTreeSet::new();
        for (rel, c) in copied {
            let t = (at + *rel).clamp(Tick::ZERO, last);
            match c {
                Copied::Position(k) => {
                    keys.position.retain(|x| x.at != t);
                    keys.position.push(TransformKey { at: t, ..*k });
                }
                Copied::Mask(k) => {
                    keys.mask.retain(|x| x.at != t);
                    keys.mask.push(MaskKey { at: t, ..k.clone() });
                }
            }
            went.insert((c.row(), t));
        }
        keys.position.sort_by_key(|k| k.at);
        keys.mask.sort_by_key(|k| k.at);
        (keys, went)
    }
}

/// `keys` with those at the times `picked` moved by `delta` (copies of
/// them, with `copy`), in their place where others were.
fn retimed<K: Timed>(keys: &[K], picked: &BTreeSet<Tick>, delta: Tick, copy: bool) -> Vec<K> {
    let movers: Vec<K> = keys
        .iter()
        .filter(|k| picked.contains(&k.at()))
        .cloned()
        .map(|mut k| {
            k.set_at(k.at() + delta);
            k
        })
        .collect();
    let landed: BTreeSet<Tick> = movers.iter().map(|k| k.at()).collect();
    let mut out: Vec<K> = keys
        .iter()
        .filter(|k| (copy || !picked.contains(&k.at())) && !landed.contains(&k.at()))
        .cloned()
        .collect();
    out.extend(movers);
    out.sort_by_key(|k| k.at());
    out
}

/// A drag in the controller.
#[derive(Clone, Debug)]
enum StripDrag {
    /// The keyframes picked moved (copies of them, with `copy`), from the
    /// rows as they were: the one taken and where the pointer took it.
    Keys {
        base: Keys,
        picked: BTreeSet<(KeyRow, Tick)>,
        at: Tick,
        from: f32,
        copy: bool,
        serial: u64,
    },
    /// The needle from an empty spot of `row`: the keyframes it passes
    /// picked (with those `kept`).
    Sweep {
        row: KeyRow,
        from: Tick,
        kept: BTreeSet<(KeyRow, Tick)>,
    },
    /// The needle, from the ruler.
    Ruler,
}

/// The controller's own state.
#[derive(Debug)]
pub(crate) struct KeyStrip {
    /// How much of the view's time shows: 1, all of it (the event and a
    /// little past its end).
    zoom: f64,
    /// The time at the left edge, a part (0–1) of the view's time.
    start: f64,
    /// How far down the rows are scrolled.
    scroll: f32,
    /// The keyframes picked.
    pub picked: BTreeSet<(KeyRow, Tick)>,
    /// The keyframe a Shift and a click runs from.
    anchor: Option<(KeyRow, Tick)>,
    drag: Option<StripDrag>,
    /// What a press on a keyframe will do once let go without moving: pick
    /// it alone, or (with Ctrl) add or take it from those picked.
    press: Option<(KeyRow, Tick, bool)>,
    /// Counts drags: each one undoes as one step.
    drags: u64,
    /// What Ctrl+C, Ctrl+X, Copy or Cut took.
    clipboard: Vec<(Tick, Copied)>,
    /// The V key's press was seen (a Ctrl+V with no text to paste comes
    /// without its paste).
    v_down: bool,
    /// The keyframe the menu is open on.
    menu: Option<(KeyRow, Tick)>,
    /// Where each keyframe and each row were drawn last (screen tests).
    pub(crate) spots: Vec<((KeyRow, Tick), Pos2)>,
    pub(crate) rows_drawn: Vec<(KeyRow, Rect)>,
}

impl Default for KeyStrip {
    fn default() -> KeyStrip {
        KeyStrip {
            zoom: 1.0,
            start: 0.0,
            scroll: 0.0,
            picked: BTreeSet::new(),
            anchor: None,
            drag: None,
            press: None,
            drags: 0,
            clipboard: Vec::new(),
            v_down: false,
            menu: None,
            spots: Vec::new(),
            rows_drawn: Vec::new(),
        }
    }
}

/// What the controller shows and edits this frame.
pub(crate) struct StripCtx<'a> {
    pub clip: &'a Clip,
    pub rate: FrameRate,
    /// The needle, from the event's start.
    pub pos: Tick,
    pub row: KeyRow,
    pub sync: bool,
    /// The curve new keyframes take (the preference's).
    pub curve: KeyCurve,
    /// The FX window has the keys.
    pub keys_on: bool,
}

/// What the user did in the controller.
#[derive(Debug, Default)]
pub(crate) struct StripAnswer {
    /// Where the needle goes (from the event's start).
    pub pos: Option<Tick>,
    pub row: Option<KeyRow>,
    pub sync: Option<bool>,
    /// The Mask row's checkbox clicked.
    pub toggle_mask: bool,
    /// The rows' new keyframes, and the drag whose undo step they join.
    pub keys: Option<(Keys, Option<u64>)>,
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

/// A keyframe as VEGAS draws it: a diamond, grey edged in black; picked,
/// white in a grey ring.
fn paint_key(p: &egui::Painter, c: Pos2, picked: bool, hot: bool) {
    let diamond = |r: f32| {
        vec![
            c + vec2(0.0, -r),
            c + vec2(r, 0.0),
            c + vec2(0.0, r),
            c + vec2(-r, 0.0),
        ]
    };
    let edge = Stroke::new(1.0, Color32::BLACK);
    if picked {
        p.add(Shape::convex_polygon(diamond(8.5), KEY, edge));
        p.add(Shape::convex_polygon(diamond(5.5), Color32::WHITE, edge));
    } else {
        let fill = if hot { Color32::from_gray(222) } else { KEY };
        p.add(Shape::convex_polygon(diamond(8.5), fill, edge));
    }
}

/// VEGAS's needle head on the ruler: a light pentagon pointing down to
/// `x` at the ruler's bottom.
fn paint_needle_head(p: &egui::Painter, x: f32, bottom: f32) {
    let (hw, h) = (4.5, 13.0);
    let points = vec![
        pos2(x - hw, bottom - h),
        pos2(x + hw, bottom - h),
        pos2(x + hw, bottom - 4.5),
        pos2(x, bottom),
        pos2(x - hw, bottom - 4.5),
    ];
    p.add(Shape::convex_polygon(
        points,
        NEEDLE_HEAD,
        Stroke::new(1.0, Color32::from_gray(20)),
    ));
}

impl KeyStrip {
    /// The controller, under the workspace.
    pub(crate) fn show(&mut self, ui: &mut Ui, cx: &StripCtx<'_>) -> StripAnswer {
        let mut answer = StripAnswer::default();
        let clip = cx.clip;
        let rate = cx.rate;
        let dur = clip.duration.max(Tick(1));
        let last = (clip.duration - rate.frame_duration()).max(Tick::ZERO);
        let keys = Keys::of(clip, cx.curve);
        // Keyframes picked that are gone (undone, say) are no longer.
        let all = keys.all();
        self.picked.retain(|k| all.contains(k));

        let (rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), height()), Sense::hover());
        let body_bottom = rect.bottom() - BUTTONS;
        let names = Rect::from_min_max(
            pos2(rect.left(), rect.top() + RULER),
            pos2(rect.left() + NAMES, body_bottom - BAR),
        );
        let vbar = Rect::from_min_max(
            pos2(rect.right() - BAR, rect.top()),
            pos2(rect.right(), body_bottom - BAR),
        );
        let ruler = Rect::from_min_max(
            pos2(names.right(), rect.top()),
            pos2(vbar.left(), rect.top() + RULER),
        );
        let lanes = Rect::from_min_max(
            pos2(names.right(), ruler.bottom()),
            pos2(vbar.left(), body_bottom - BAR),
        );
        let hbar = Rect::from_min_max(
            pos2(names.right(), body_bottom - BAR),
            pos2(rect.right(), body_bottom),
        );
        let buttons = Rect::from_min_max(pos2(rect.left(), body_bottom), rect.max);

        // The time shown: the event and a little past its end.
        let total = dur.0 as f64 * (1.0 + PAST_END);
        let max_zoom = (total / rate.frame_duration().0.max(1) as f64 / 4.0).max(1.0);
        let wheel = ui.input(|i| i.smooth_scroll_delta);
        if (wheel.x != 0.0 || wheel.y != 0.0)
            && let Some(p) = ui.ctx().pointer_hover_pos()
            && (lanes.contains(p) || ruler.contains(p))
        {
            if wheel.x != 0.0 || ui.input(|i| i.modifiers.shift) {
                // Sideways: the view moves.
                let by = (wheel.x + if wheel.x == 0.0 { wheel.y } else { 0.0 }) as f64;
                self.start -= by / lanes.width() as f64 / self.zoom;
            } else {
                // The wheel zooms round the pointer, as the timeline's.
                let anchor = ((p.x - lanes.left()) / lanes.width()) as f64;
                let at = self.start + anchor / self.zoom;
                self.zoom = (self.zoom * (wheel.y as f64 * 0.004).exp()).clamp(1.0, max_zoom);
                self.start = at - anchor / self.zoom;
            }
        }
        self.zoom = self.zoom.clamp(1.0, max_zoom);
        self.start = self.start.clamp(0.0, 1.0 - 1.0 / self.zoom);
        let (zoom, start) = (self.zoom, self.start);
        let x_of = move |t: Tick| {
            lanes.left() + ((t.0 as f64 / total - start) * zoom) as f32 * lanes.width()
        };
        let raw_t = move |x: f32| {
            let f = ((x - lanes.left()) / lanes.width()) as f64 / zoom + start;
            Tick((f * total) as i64)
        };
        // On the timeline's frames (the event may start between two).
        let t_of = move |x: f32| {
            (rate.snap_nearest(clip.start + raw_t(x)) - clip.start).clamp(Tick::ZERO, last)
        };
        let scroll = self.scroll;
        let row_rect = move |i: usize| {
            let top = lanes.top() + i as f32 * ROW - scroll;
            Rect::from_min_max(
                pos2(lanes.left(), top),
                pos2(lanes.right(), top + ROW - 1.0),
            )
        };
        let row_at = |y: f32| -> Option<KeyRow> {
            ROWS.iter()
                .enumerate()
                .find(|(i, _)| row_rect(*i).y_range().contains(y))
                .map(|(_, r)| *r)
        };
        let key_at = |p: Pos2| -> Option<(KeyRow, Tick)> {
            let row = row_at(p.y)?;
            keys.times(row)
                .into_iter()
                .map(|(t, _)| (t, (x_of(t) - p.x).abs()))
                .filter(|(_, d)| *d <= GRAB)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(t, _)| (row, t))
        };

        // The pointer in the rows: keyframes picked, dragged, the needle.
        let lanes_resp = ui.interact(
            lanes,
            ui.id().with("pc-keys-lanes"),
            Sense::click_and_drag(),
        );
        let ruler_resp = ui.interact(
            ruler,
            ui.id().with("pc-keys-ruler"),
            Sense::click_and_drag(),
        );
        let mods = ui.input(|i| i.modifiers);
        let pointer = ui.input(|i| i.pointer.interact_pos());
        if ruler_resp.drag_started() || ruler_resp.clicked() {
            self.drag = ruler_resp.drag_started().then_some(StripDrag::Ruler);
            if let Some(p) = ruler_resp.interact_pointer_pos() {
                answer.pos = Some(t_of(p.x));
            }
        }
        if ruler_resp.dragged()
            && matches!(self.drag, Some(StripDrag::Ruler))
            && let Some(p) = pointer
        {
            answer.pos = Some(t_of(p.x));
        }
        // A press: on a keyframe it may be a click (pick) or a drag (move);
        // on an empty spot, the needle and what it passes picked.
        if lanes_resp.is_pointer_button_down_on()
            && ui.input(|i| i.pointer.primary_pressed())
            && let Some(o) = ui.input(|i| i.pointer.press_origin())
        {
            if let Some(row) = row_at(o.y) {
                answer.row = Some(row);
            }
            self.press = key_at(o).map(|(r, t)| (r, t, mods.command));
        }
        if lanes_resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(o) = ui.input(|i| i.pointer.press_origin())
        {
            match key_at(o) {
                Some((row, at)) => {
                    // A keyframe not picked: it alone (with those picked, by Ctrl).
                    if !self.picked.contains(&(row, at)) {
                        if !mods.command {
                            self.picked.clear();
                        }
                        self.picked.insert((row, at));
                    }
                    self.drags += 1;
                    self.drag = Some(StripDrag::Keys {
                        base: keys.clone(),
                        picked: self.picked.clone(),
                        at,
                        from: o.x,
                        copy: mods.command,
                        serial: self.drags,
                    });
                    self.press = None;
                }
                None => {
                    let row = row_at(o.y).unwrap_or(cx.row);
                    let kept = if mods.command {
                        self.picked.clone()
                    } else {
                        BTreeSet::new()
                    };
                    self.drag = Some(StripDrag::Sweep {
                        row,
                        from: t_of(o.x),
                        kept,
                    });
                }
            }
        }
        if lanes_resp.dragged_by(egui::PointerButton::Primary)
            && let Some(p) = pointer
        {
            match &self.drag {
                Some(StripDrag::Keys {
                    base,
                    picked,
                    at,
                    from,
                    copy,
                    serial,
                }) => {
                    // Together, none of them past the event's start or end.
                    let (lo, hi) = picked
                        .iter()
                        .fold((Tick(i64::MAX), Tick(i64::MIN)), |(lo, hi), (_, t)| {
                            (lo.min(*t), hi.max(*t))
                        });
                    let to = (rate.snap_nearest(clip.start + *at + (raw_t(p.x) - raw_t(*from)))
                        - clip.start)
                        .clamp(*at - lo, *at + (last - hi));
                    let delta = to - *at;
                    let (moved, went) = base.moved(picked, delta, *copy);
                    if moved != keys {
                        answer.keys = Some((moved, Some(*serial)));
                    }
                    self.picked = went;
                    answer.pos = Some(to);
                }
                Some(StripDrag::Sweep { row, from, kept }) => {
                    let to = t_of(p.x);
                    let (a, b) = if to < *from { (to, *from) } else { (*from, to) };
                    let mut picked = kept.clone();
                    for (t, _) in keys.times(*row) {
                        if t >= a && t <= b {
                            picked.insert((*row, t));
                        }
                    }
                    self.picked = picked;
                    answer.pos = Some(to);
                }
                _ => {}
            }
        }
        if lanes_resp.drag_stopped() || ruler_resp.drag_stopped() {
            self.drag = None;
        }
        // A click: on a keyframe it is picked (alone; with Ctrl, one more or
        // one less; with Shift, the run from the last one picked in its row)
        // and the needle goes there; on an empty spot the needle goes there
        // and nothing is picked.
        if lanes_resp.clicked()
            && let Some(p) = lanes_resp.interact_pointer_pos()
        {
            match self
                .press
                .take()
                .or_else(|| key_at(p).map(|(r, t)| (r, t, mods.command)))
            {
                Some((row, at, ctrl)) => {
                    if ctrl {
                        if !self.picked.remove(&(row, at)) {
                            self.picked.insert((row, at));
                        }
                    } else if mods.shift
                        && let Some((arow, from)) = self.anchor
                        && arow == row
                    {
                        let (a, b) = if at < from { (at, from) } else { (from, at) };
                        for (t, _) in keys.times(row) {
                            if t >= a && t <= b {
                                self.picked.insert((row, t));
                            }
                        }
                    } else {
                        self.picked.clear();
                        self.picked.insert((row, at));
                    }
                    if !mods.shift {
                        self.anchor = Some((row, at));
                    }
                    answer.pos = Some(at);
                }
                None => {
                    if !mods.command {
                        self.picked.clear();
                    }
                    answer.pos = Some(t_of(p.x));
                }
            }
        }

        // The menu of a keyframe (it picked first, unless it is): Cut,
        // Copy, Paste, Delete and the curves, all on those picked.
        if lanes_resp.secondary_clicked()
            && let Some(p) = lanes_resp.interact_pointer_pos()
        {
            self.menu = key_at(p);
            if let Some(k) = self.menu
                && !self.picked.contains(&k)
            {
                self.picked.clear();
                self.picked.insert(k);
            }
        }
        let mut menu_action: Option<MenuAction> = None;
        lanes_resp.context_menu(|ui| {
            let on_key = self.menu.is_some();
            let curve_now = self.menu.and_then(|(r, t)| {
                keys.times(r)
                    .into_iter()
                    .find(|(at, _)| *at == t)
                    .map(|(_, c)| c)
            });
            ui.add_enabled_ui(on_key, |ui| {
                if crate::menus::item(ui, tr("Cut")) {
                    menu_action = Some(MenuAction::Cut);
                    ui.close();
                }
                if crate::menus::item(ui, tr("Copy")) {
                    menu_action = Some(MenuAction::Copy);
                    ui.close();
                }
            });
            ui.add_enabled_ui(!self.clipboard.is_empty(), |ui| {
                if crate::menus::item(ui, tr("Paste")) {
                    menu_action = Some(MenuAction::Paste);
                    ui.close();
                }
            });
            ui.separator();
            ui.add_enabled_ui(on_key, |ui| {
                if crate::menus::item(ui, tr("Delete")) {
                    menu_action = Some(MenuAction::Delete);
                    ui.close();
                }
                ui.separator();
                for c in [
                    KeyCurve::Linear,
                    KeyCurve::Fast,
                    KeyCurve::Slow,
                    KeyCurve::Smooth,
                    KeyCurve::Sharp,
                    KeyCurve::Hold,
                ] {
                    if crate::menus::check_item(ui, tr(c.name()), curve_now == Some(c), true, "") {
                        menu_action = Some(MenuAction::Curve(c));
                        ui.close();
                    }
                }
            });
        });

        // The keys, while the FX window has them (and no text is typed):
        // Ctrl+C, Ctrl+X, Ctrl+V, Delete, Ctrl+A, Ctrl+← and Ctrl+→.
        let mut go: Option<bool> = None;
        if cx.keys_on && !ui.ctx().egui_wants_keyboard_input() {
            let v_down = &mut self.v_down;
            ui.input(|i| {
                for e in &i.events {
                    match e {
                        egui::Event::Copy => menu_action = Some(MenuAction::CopyKeys),
                        egui::Event::Cut => menu_action = Some(MenuAction::CutKeys),
                        egui::Event::Paste(_) => {
                            menu_action = Some(MenuAction::Paste);
                            *v_down = true;
                        }
                        egui::Event::Key {
                            key: egui::Key::V,
                            pressed,
                            modifiers,
                            ..
                        } => {
                            if *pressed {
                                *v_down = true;
                            } else if !std::mem::take(v_down) && modifiers.command {
                                menu_action = Some(MenuAction::Paste);
                            }
                        }
                        egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            ..
                        } => match key {
                            egui::Key::Delete if modifiers.is_none() => {
                                menu_action = Some(MenuAction::DeleteKeys);
                            }
                            egui::Key::A if modifiers.command => {
                                menu_action = Some(MenuAction::All)
                            }
                            egui::Key::ArrowLeft if modifiers.command => go = Some(false),
                            egui::Key::ArrowRight if modifiers.command => go = Some(true),
                            _ => {}
                        },
                        _ => {}
                    }
                }
            });
        }
        if let Some(a) = menu_action {
            self.act(a, &keys, cx, last, &mut answer);
        }

        // Drawn.
        let painter = ui.painter_at(rect);
        let panel = theme::panel();
        let line = mix(panel, theme::text(), 0.42);
        painter.rect_filled(rect, 0.0, panel);
        // What is past the event's end and under the rows: white.
        painter.rect_filled(lanes, 0.0, PAST);
        let end_x = x_of(dur).min(lanes.right());
        let lanes_p = painter.with_clip_rect(lanes);
        for (i, r) in ROWS.iter().enumerate() {
            let rr = row_rect(i);
            let picked_row = *r == cx.row;
            let fill = if picked_row {
                mix(panel, theme::text(), 0.06)
            } else {
                mix(panel, theme::panel_dark(), 0.1)
            };
            lanes_p.rect_filled(
                Rect::from_min_max(rr.min, pos2(end_x, rr.bottom())),
                0.0,
                fill,
            );
            lanes_p.hline(
                lanes.left()..=end_x,
                rr.bottom() + 0.5,
                Stroke::new(1.0, line),
            );
        }
        // The ruler: timecodes at its steps, small ticks between.
        painter.rect_filled(ruler, 0.0, panel);
        painter.hline(
            ruler.x_range(),
            ruler.bottom() - 0.5,
            Stroke::new(1.0, line),
        );
        let ruler_p = painter.with_clip_rect(ruler);
        let secs = total / Tick::SECOND.0 as f64;
        let px_per_s = lanes.width() as f64 * zoom / secs.max(1e-9);
        let step = ruler_step(px_per_s, rate);
        let first = (start * secs / step).floor() * step;
        let mut s = first;
        while s <= secs + step {
            let x = x_of(Tick::from_seconds_f64(s));
            if x > ruler.right() {
                break;
            }
            ruler_p.vline(
                x,
                ruler.bottom() - 6.0..=ruler.bottom(),
                Stroke::new(1.0, theme::text_dim()),
            );
            ruler_p.text(
                pos2(x + 2.0, ruler.top() + 1.0),
                Align2::LEFT_TOP,
                timecode::format(Tick::from_seconds_f64(s), rate),
                FontId::monospace(10.0),
                theme::text_dim(),
            );
            for m in 1..4 {
                let xm = x_of(Tick::from_seconds_f64(s + step * m as f64 / 4.0));
                ruler_p.vline(
                    xm,
                    ruler.bottom() - 3.0..=ruler.bottom(),
                    Stroke::new(1.0, line),
                );
            }
            s += step;
        }
        // The keyframes, picked ones as VEGAS shows them.
        let hot = pointer.filter(|p| lanes.contains(*p)).and_then(key_at);
        self.spots.clear();
        self.rows_drawn.clear();
        for (i, r) in ROWS.iter().enumerate() {
            let rr = row_rect(i);
            self.rows_drawn
                .push((*r, Rect::from_min_max(rr.min, pos2(end_x, rr.bottom()))));
            for (t, _) in keys.times(*r) {
                let picked = self.picked.contains(&(*r, t));
                let at = pos2(x_of(t), rr.center().y);
                paint_key(&lanes_p, at, picked, hot == Some((*r, t)));
                self.spots.push(((*r, t), at));
            }
        }
        // The needle where it goes (its head on the ruler, its line down).
        let needle = answer.pos.unwrap_or(cx.pos);
        let nx = x_of(needle).round() + 0.5;
        if ruler.x_range().contains(nx) {
            lanes_p.vline(nx, lanes.top()..=lanes.bottom(), Stroke::new(1.0, NEEDLE));
            paint_needle_head(
                &painter.with_clip_rect(ruler.expand2(vec2(6.0, 0.0))),
                nx,
                ruler.bottom(),
            );
        }

        // The rows' names: a click picks the row; the Mask row's checkbox
        // turns the masks on and off.
        painter.rect_filled(
            Rect::from_min_max(
                pos2(rect.left(), rect.top()),
                pos2(names.right(), ruler.bottom()),
            ),
            0.0,
            panel,
        );
        let names_p = painter.with_clip_rect(names);
        for (i, r) in ROWS.iter().enumerate() {
            let rr = row_rect(i);
            let label = Rect::from_min_max(
                pos2(names.left(), rr.top()),
                pos2(names.right() - 2.0, rr.bottom()),
            );
            let picked_row = *r == cx.row;
            names_p.rect_filled(
                label,
                0.0,
                mix(panel, theme::text(), if picked_row { 0.21 } else { 0.105 }),
            );
            names_p.hline(names.x_range(), rr.bottom() + 0.5, Stroke::new(1.0, panel));
            let resp = ui.interact(
                label,
                ui.id().with(("pc-keys-name", *r as u8)),
                Sense::click(),
            );
            if resp.clicked() {
                answer.row = Some(*r);
            }
            match r {
                KeyRow::Position => {
                    names_p.text(
                        pos2(label.left() + 8.0, label.center().y),
                        Align2::LEFT_CENTER,
                        tr("Position"),
                        FontId::proportional(13.0),
                        theme::text_strong(),
                    );
                }
                KeyRow::Mask => {
                    let on = clip.mask.as_ref().is_some_and(|m| m.enabled);
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(label.shrink2(vec2(6.0, 1.0)))
                            .layout(Layout::left_to_right(Align::Center)),
                    );
                    let mut v = on;
                    let text = RichText::new(tr("Mask")).color(if on {
                        theme::text_strong()
                    } else {
                        theme::text_disabled()
                    });
                    if crate::widgets::checkbox(&mut child, &mut v, text).changed() {
                        answer.toggle_mask = true;
                        answer.row = Some(KeyRow::Mask);
                    }
                }
            }
        }

        self.vertical_bar(ui, vbar, lanes.height());
        self.time_bar(ui, hbar, max_zoom);

        // The buttons, as VEGAS's: Sync Cursor; the first, previous, next
        // and last keyframe of the row picked; one made or deleted at the
        // needle; the needle's time at the right.
        let row = answer.row.unwrap_or(cx.row);
        let times: Vec<Tick> = keys.times(row).into_iter().map(|(t, _)| t).collect();
        let pos = answer.pos.unwrap_or(cx.pos);
        if let Some(forward) = go {
            let to = if forward {
                times.iter().find(|&&a| a > pos).copied()
            } else {
                times.iter().rev().find(|&&a| a < pos).copied()
            };
            if let Some(to) = to {
                answer.pos = Some(to);
            }
        }
        let mut bar = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(buttons.shrink2(vec2(6.0, 3.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        bar.spacing_mut().item_spacing.x = 2.0;
        let button = |ui: &mut Ui, icon, on, enabled, tip: &str| {
            crate::widgets::icon_button_ex(ui, icon, tip, on, enabled).clicked()
        };
        bar.add_space((NAMES - 2.0).max(4.0));
        if button(&mut bar, Icon::Lock, cx.sync, true, tr("Sync Cursor")) {
            answer.sync = Some(!cx.sync);
        }
        bar.add_space(10.0);
        let any = !times.is_empty();
        if button(&mut bar, Icon::GoStart, false, any, tr("First Keyframe")) {
            answer.pos = times.first().copied();
        }
        let before = times.iter().any(|&a| a < pos);
        if button(
            &mut bar,
            Icon::PrevFrame,
            false,
            before,
            tr("Previous Keyframe"),
        ) {
            answer.pos = times.iter().rev().find(|&&a| a < pos).copied();
        }
        let after = times.iter().any(|&a| a > pos);
        if button(&mut bar, Icon::NextFrame, false, after, tr("Next Keyframe")) {
            answer.pos = times.iter().find(|&&a| a > pos).copied();
        }
        if button(&mut bar, Icon::GoEnd, false, any, tr("Last Keyframe")) {
            answer.pos = times.last().copied();
        }
        bar.add_space(10.0);
        let has = times.contains(&pos);
        let can_make =
            row == KeyRow::Position || clip.mask.as_ref().is_some_and(|m| !m.paths.is_empty());
        if button(
            &mut bar,
            Icon::Plus,
            false,
            can_make && !has,
            tr("Create Keyframe at Cursor"),
        ) {
            answer.keys = Some((made_at(&keys, clip, row, pos), None));
        }
        if button(
            &mut bar,
            Icon::Delete,
            false,
            has && times.len() > 1,
            tr("Delete Keyframe at Cursor"),
        ) {
            let one: BTreeSet<(KeyRow, Tick)> = [(row, pos)].into();
            answer.keys = Some((keys.deleted(&one), None));
        }
        bar.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(6.0);
            ui.label(
                RichText::new(timecode::format(pos, rate))
                    .monospace()
                    .color(mix(theme::text(), theme::selection(), 0.35)),
            );
            // VEGAS's needle beside it.
            let (r, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
            paint_needle_head(ui.painter(), r.center().x, r.center().y + 2.0);
            ui.painter().vline(
                r.center().x,
                r.center().y + 2.0..=r.bottom(),
                Stroke::new(1.0, NEEDLE_HEAD),
            );
        });
        answer
    }

    /// Does what the menu or the keys asked.
    fn act(
        &mut self,
        a: MenuAction,
        keys: &Keys,
        cx: &StripCtx<'_>,
        last: Tick,
        answer: &mut StripAnswer,
    ) {
        // The keys' Copy, Cut and Delete with nothing picked: the row's
        // keyframe at the needle (Copy: its value there, keyframe or not).
        let at_needle: BTreeSet<(KeyRow, Tick)> = keys
            .times(cx.row)
            .into_iter()
            .filter(|(t, _)| *t == cx.pos)
            .map(|(t, _)| (cx.row, t))
            .collect();
        let picked = match a {
            MenuAction::CopyKeys | MenuAction::CutKeys | MenuAction::DeleteKeys
                if self.picked.is_empty() =>
            {
                at_needle.clone()
            }
            _ => self.picked.clone(),
        };
        match a {
            MenuAction::Copy | MenuAction::CopyKeys | MenuAction::Cut | MenuAction::CutKeys => {
                self.clipboard = if picked.is_empty() && matches!(a, MenuAction::CopyKeys) {
                    value_at(keys, cx.clip, cx.row, cx.pos)
                } else {
                    keys.copied(&picked)
                };
                if matches!(a, MenuAction::Cut | MenuAction::CutKeys) && !picked.is_empty() {
                    answer.keys = Some((keys.deleted(&picked), None));
                }
            }
            MenuAction::Paste => {
                // Masks' shapes go only where there are masks.
                let copied: Vec<(Tick, Copied)> = self
                    .clipboard
                    .iter()
                    .filter(|(_, c)| c.row() == KeyRow::Position || cx.clip.mask.is_some())
                    .cloned()
                    .collect();
                if !copied.is_empty() {
                    let (pasted, went) = keys.pasted(&copied, cx.pos, last);
                    answer.keys = Some((pasted, None));
                    self.picked = went;
                }
            }
            MenuAction::Delete | MenuAction::DeleteKeys => {
                if !picked.is_empty() {
                    answer.keys = Some((keys.deleted(&picked), None));
                    self.picked.clear();
                }
            }
            MenuAction::Curve(c) => answer.keys = Some((keys.curved(&picked, c), None)),
            MenuAction::All => self.picked = keys.all(),
        }
    }

    /// The scroll bar of the rows, at their right, as VEGAS's.
    fn vertical_bar(&mut self, ui: &mut Ui, bar: Rect, shown: f32) {
        let content = ROWS.len() as f32 * ROW;
        let max_scroll = (content - shown).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max_scroll);
        let p = ui.painter_at(bar);
        p.rect_filled(bar, 0.0, mix(theme::panel(), theme::text(), 0.03));
        let w = bar.width();
        let up = Rect::from_min_size(bar.min, vec2(w, w));
        let down = Rect::from_min_size(pos2(bar.left(), bar.bottom() - w), vec2(w, w));
        let id = ui.id().with("pc-keys-vbar");
        for (r, dir, salt) in [(up, -1.0, "up"), (down, 1.0, "down")] {
            let resp = ui.interact(r, id.with(salt), Sense::click());
            let color = if resp.hovered() && max_scroll > 0.0 {
                theme::text_strong()
            } else {
                theme::text()
            };
            let c = r.center();
            p.add(Shape::convex_polygon(
                vec![
                    c + vec2(-4.0, -2.5 * dir),
                    c + vec2(4.0, -2.5 * dir),
                    c + vec2(0.0, 2.5 * dir),
                ],
                color,
                Stroke::NONE,
            ));
            if resp.clicked() {
                self.scroll = (self.scroll + dir * ROW).clamp(0.0, max_scroll);
            }
        }
        let track =
            Rect::from_min_max(pos2(bar.left(), up.bottom()), pos2(bar.right(), down.top()));
        let frac = (shown / content.max(1.0)).min(1.0);
        let th = (track.height() * frac).max(12.0f32.min(track.height()));
        let room = track.height() - th;
        let at = if max_scroll > 0.0 {
            self.scroll / max_scroll * room
        } else {
            0.0
        };
        let thumb = Rect::from_min_size(
            pos2(track.left() + 1.0, track.top() + at),
            vec2(track.width() - 2.0, th),
        );
        let resp = ui.interact(track, id.with("thumb"), Sense::drag());
        if resp.dragged() && room > 0.0 {
            self.scroll =
                (self.scroll + resp.drag_delta().y / room * max_scroll).clamp(0.0, max_scroll);
        }
        p.rect_filled(thumb, 0.0, mix(theme::panel(), theme::text(), 0.16));
        // Its grip.
        for k in [-2.0, 2.0] {
            p.hline(
                thumb.left() + 3.0..=thumb.right() - 3.0,
                thumb.center().y + k,
                Stroke::new(1.0, mix(theme::panel(), theme::text(), 0.42)),
            );
        }
    }

    /// The bar under the rows, as VEGAS's: ◀, the scroll bar (its thumb's
    /// ends zoom), ▶, + and −.
    fn time_bar(&mut self, ui: &mut Ui, bar: Rect, max_zoom: f64) {
        let p = ui.painter_at(bar);
        p.rect_filled(bar, 0.0, mix(theme::panel(), theme::text(), 0.03));
        let h = bar.height();
        let left = Rect::from_min_size(bar.min, vec2(h, h));
        let minus = Rect::from_min_size(pos2(bar.right() - h, bar.top()), vec2(h, h));
        let plus = minus.translate(vec2(-h - 4.0, 0.0));
        let right = plus.translate(vec2(-h - 4.0, 0.0));
        let track = Rect::from_min_max(
            pos2(left.right(), bar.top()),
            pos2(right.left(), bar.bottom()),
        );
        p.rect_filled(track, 0.0, mix(theme::panel(), theme::text(), 0.16));
        let id = ui.id().with("pc-keys-hbar");
        if let Some((start, len)) = crate::widgets::zoom_scrollbar(
            ui,
            track,
            id.with("thumb"),
            (self.start, 1.0 / self.zoom),
            1.0,
            (1.0 / max_zoom, 1.0),
        ) {
            self.zoom = (1.0 / len).clamp(1.0, max_zoom);
            self.start = start.clamp(0.0, 1.0 - 1.0 / self.zoom);
        }
        let small =
            |ui: &mut Ui, r: Rect, salt: &str, glyph: &dyn Fn(&egui::Painter, Pos2, Color32)| {
                let resp = ui.interact(r, id.with(salt), Sense::click());
                let color = if resp.hovered() {
                    theme::text_strong()
                } else {
                    theme::text()
                };
                glyph(&p, r.center(), color);
                resp.clicked()
            };
        let arrow = |dir: f32| {
            move |p: &egui::Painter, c: Pos2, col: Color32| {
                p.add(Shape::convex_polygon(
                    vec![
                        c + vec2(-2.5 * dir, -4.0),
                        c + vec2(2.5 * dir, 0.0),
                        c + vec2(-2.5 * dir, 4.0),
                    ],
                    col,
                    Stroke::NONE,
                ));
            }
        };
        let page = 0.25 / self.zoom;
        if small(ui, left, "left", &arrow(-1.0)) {
            self.start = (self.start - page).max(0.0);
        }
        if small(ui, right, "right", &arrow(1.0)) {
            self.start = (self.start + page).min(1.0 - 1.0 / self.zoom);
        }
        let sign = |plus: bool| {
            move |p: &egui::Painter, c: Pos2, col: Color32| {
                p.rect_filled(
                    Rect::from_center_size(c, vec2(14.0, 14.0)),
                    0.0,
                    mix(theme::panel(), theme::text(), 0.16),
                );
                let s = Stroke::new(1.6, col);
                p.line_segment([c + vec2(-4.0, 0.0), c + vec2(4.0, 0.0)], s);
                if plus {
                    p.line_segment([c + vec2(0.0, -4.0), c + vec2(0.0, 4.0)], s);
                }
            }
        };
        let zoom_by = |k: f64, s: &mut KeyStrip| {
            let middle = s.start + 0.5 / s.zoom;
            s.zoom = (s.zoom * k).clamp(1.0, max_zoom);
            s.start = (middle - 0.5 / s.zoom).clamp(0.0, 1.0 - 1.0 / s.zoom);
        };
        if small(ui, plus, "zoom in", &sign(true)) {
            zoom_by(1.5, self);
        }
        if small(ui, minus, "zoom out", &sign(false)) {
            zoom_by(1.0 / 1.5, self);
        }
    }
}

/// What the menu and the keys do.
#[derive(Clone, Copy, Debug, PartialEq)]
enum MenuAction {
    Cut,
    Copy,
    Paste,
    Delete,
    Curve(KeyCurve),
    /// Ctrl+C, Ctrl+X and Delete: with nothing picked, the keyframe at the
    /// needle.
    CopyKeys,
    CutKeys,
    DeleteKeys,
    All,
}

/// `row`'s value at `pos` as one keyframe copied (Ctrl+C with nothing
/// picked: the framing or the masks' shapes there).
fn value_at(keys: &Keys, clip: &Clip, row: KeyRow, pos: Tick) -> Vec<(Tick, Copied)> {
    let curve = |times: Vec<(Tick, KeyCurve)>| {
        times
            .into_iter()
            .rev()
            .find(|(t, _)| *t <= pos)
            .map_or(KeyCurve::Smooth, |(_, c)| c)
    };
    match row {
        KeyRow::Position => vec![(
            Tick::ZERO,
            Copied::Position(TransformKey::new(
                Tick::ZERO,
                clip.transform_at(clip.start + pos),
                curve(keys.times(row)),
            )),
        )],
        KeyRow::Mask => clip
            .mask
            .as_ref()
            .map(|m| {
                vec![(
                    Tick::ZERO,
                    Copied::Mask(MaskKey {
                        at: Tick::ZERO,
                        shapes: m.shapes_at(pos),
                        curve: curve(keys.times(row)),
                    }),
                )]
            })
            .unwrap_or_default(),
    }
}

/// The rows with a keyframe made in `row` at `pos`, holding its value
/// there (the curve of the one before).
fn made_at(keys: &Keys, clip: &Clip, row: KeyRow, pos: Tick) -> Keys {
    let made = value_at(keys, clip, row, pos);
    keys.pasted(&made, pos, clip.duration).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::Transform;

    fn keys(times: &[i64]) -> Keys {
        Keys {
            position: times
                .iter()
                .map(|&t| {
                    TransformKey::new(
                        Tick(t),
                        Transform {
                            x: t as f32,
                            ..Transform::default()
                        },
                        KeyCurve::Linear,
                    )
                })
                .collect(),
            mask: Vec::new(),
        }
    }

    fn times(k: &Keys) -> Vec<i64> {
        k.position.iter().map(|k| k.at.0).collect()
    }

    fn pick(times: &[i64]) -> BTreeSet<(KeyRow, Tick)> {
        times.iter().map(|&t| (KeyRow::Position, Tick(t))).collect()
    }

    /// Picked keyframes move together, the first one too, taking the place
    /// of one where they land; with Ctrl, copies move and the others stay.
    #[test]
    fn picked_keyframes_move_together() {
        let k = keys(&[0, 10, 20, 30]);
        let (moved, went) = k.moved(&pick(&[0, 10]), Tick(20), false);
        assert_eq!(times(&moved), [20, 30]);
        assert_eq!(went, pick(&[20, 30]));
        // The one at 20 was the first's: x 0 now.
        assert_eq!(moved.position[0].value.x, 0.0);
        let (copied, _) = k.moved(&pick(&[10]), Tick(5), true);
        assert_eq!(times(&copied), [0, 10, 15, 20, 30]);
    }

    /// Deleting keeps a keyframe in the row; the curves and the clipboard
    /// work on all those picked.
    #[test]
    fn picked_keyframes_are_deleted_curved_copied_and_pasted_together() {
        let k = keys(&[0, 10, 20]);
        assert_eq!(times(&k.deleted(&pick(&[10, 20]))), [0]);
        assert_eq!(times(&k.deleted(&pick(&[0, 10, 20]))), [0]);
        let c = k.curved(&pick(&[0, 20]), KeyCurve::Hold);
        let curves: Vec<KeyCurve> = c.position.iter().map(|k| k.curve).collect();
        assert_eq!(curves, [KeyCurve::Hold, KeyCurve::Linear, KeyCurve::Hold]);
        let copied = k.copied(&pick(&[10, 20]));
        assert_eq!(copied.iter().map(|(t, _)| t.0).collect::<Vec<_>>(), [0, 10]);
        let (pasted, went) = k.pasted(&copied, Tick(15), Tick(100));
        assert_eq!(times(&pasted), [0, 10, 15, 20, 25]);
        assert_eq!(went, pick(&[15, 25]));
        // Past the event's end they stop at its last frame.
        let (late, _) = k.pasted(&copied, Tick(95), Tick(100));
        assert_eq!(times(&late), [0, 10, 20, 95, 100]);
    }
}
