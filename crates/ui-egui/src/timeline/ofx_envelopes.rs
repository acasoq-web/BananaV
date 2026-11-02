//! VEGAS 2026's timeline-based OFX keyframe animation ("Animate OFX
//! keyframes on the timeline", a beta there too): with View › Show OFX
//! Effect Envelopes (Beta) on, the animated parameters of plug-in effects
//! show as envelopes on the timeline — a video track's in its track
//! keyframes area (the track header's Expand Track Keyframes: a line for
//! each of its effects and for its Track Motion with their keyframes,
//! whose times can be dragged there without touching their values; the
//! effect picked there shows its envelopes), an event's over the event. An
//! envelope is one number of a parameter (a position's X and Y apart) over
//! its display range, its points the parameter's keyframes: dragged (time
//! and value), added by a double-click on the line, a Bézier point's
//! handles dragged. A point's menu types its value (Set to…) and deletes
//! it; a point's and a line's give a curve (a line's to its point before
//! the pointer), add a point at the cursor, and flip, thin or reset the
//! envelope; the keyframes area's adds a point to every envelope it shows
//! and shows or hides each.

use std::collections::HashSet;

use bv_core::{FrameRate, Tick};
use bv_engine::plugins::{animation, frames, handles, plugin_value, set_handle};
use bv_ofx::{ParamInfo, ParamType, Value};
use bv_timeline::{
    ClipId, Effect, KeyCurve, PluginFx, PluginKey, PluginParam, PluginValue, Project, TrackId,
    TrackKind,
};

use super::{Layout, TimelineView};
use crate::app::BananaApp;
use crate::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, pos2, vec2};
use crate::i18n::tr;
use crate::theme;

/// A line of the track keyframes area: an effect's, or Track Motion's.
const LINE_H: f32 = 14.0;
/// The envelopes' room under the lines.
const ENVELOPES_H: f32 = 64.0;

/// The curves of a point's menu, as VEGAS's timeline lists them.
const CURVES: [(KeyCurve, &str); 8] = [
    (KeyCurve::Linear, "Linear Fade"),
    (KeyCurve::Fast, "Fast Fade"),
    (KeyCurve::Slow, "Slow Fade"),
    (KeyCurve::Smooth, "Smooth Fade"),
    (KeyCurve::Sharp, "Sharp Fade"),
    (KeyCurve::Hold, "Hold"),
    (KeyCurve::Bezier, "Bézier curve (locked tangents)"),
    (KeyCurve::BezierSplit, "Bézier curve (split tangents)"),
];

/// Whose effects: a track's or an event's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Owner {
    Track(TrackId),
    Clip(ClipId),
}

/// An envelope as a keyframes area hides it: its effect's owner and place
/// in the chain, its parameter and number.
pub(crate) type EnvelopeKey = (Owner, usize, String, usize);

/// An envelope: one number of a parameter of an effect.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Line {
    owner: Owner,
    effect: usize,
    param: String,
    component: usize,
    ty: ParamType,
    lo: f64,
    hi: f64,
    /// The timeline's time of the owner's 0.
    origin: Tick,
    /// Where it is drawn.
    area: Rect,
    color: Color32,
    label: String,
}

impl Line {
    fn key(&self) -> EnvelopeKey {
        (self.owner, self.effect, self.param.clone(), self.component)
    }
}

/// What the pointer is on: a point (a keyframe; `handle`, one of a Bézier
/// point's handles — the one after it, or the one before) or a line, at a
/// time, and where.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OfxHit {
    line: Line,
    key: Option<usize>,
    handle: Option<bool>,
    /// The owner's time there.
    at: Tick,
    value: f64,
    pos: Pos2,
}

/// What is being dragged, with its undo step: a point (or one of its
/// handles), or the keyframes of a keyframes area's line at a time.
#[derive(Clone, Debug)]
pub(crate) enum OfxDrag {
    Point {
        line: Line,
        key: usize,
        handle: Option<bool>,
        undo: egui::Id,
    },
    Times {
        track: TrackId,
        row: AreaRow,
        from: Tick,
        undo: egui::Id,
    },
}

/// A line of a track's keyframes area: an effect's (its place in the
/// chain) or Track Motion's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AreaRow {
    Effect(usize),
    Motion,
}

/// "Set to…": a point's value being typed, and where.
#[derive(Clone, Debug)]
pub(crate) struct OfxSetTo {
    line: Line,
    key: usize,
    text: String,
    pos: Pos2,
    focus: bool,
}

/// How tall `track`'s keyframes area is (none while it is collapsed or
/// the envelopes are hidden).
pub(crate) fn height(view: &TimelineView, track: &bv_timeline::Track) -> f32 {
    if !view.show_ofx_envelopes
        || !view.show_track_keys
        || track.kind != TrackKind::Video
        || !view.track_keys.contains_key(&track.id)
    {
        return 0.0;
    }
    let effects = track.effects.iter().filter(|e| e.plugin.is_some()).count();
    LINE_H * (effects + 1) as f32 + ENVELOPES_H
}

fn chain(project: &Project, owner: Owner) -> Option<&[Effect]> {
    match owner {
        Owner::Track(t) => project.track(t).map(|t| &t.effects[..]),
        Owner::Clip(c) => project.clip(c).map(|c| &c.effects[..]),
    }
}

fn param_mut<'a>(p: &'a mut Project, line: &Line) -> Option<&'a mut PluginParam> {
    let effects = match line.owner {
        Owner::Track(t) => &mut p.track_by_id_mut(t)?.effects,
        Owner::Clip(c) => &mut p.clip_mut(c)?.effects,
    };
    effects
        .get_mut(line.effect)?
        .plugin
        .as_mut()?
        .params
        .get_mut(&line.param)
}

/// A plug-in's parameter as it describes it.
fn param_info(fx: &PluginFx, name: &str) -> Option<ParamInfo> {
    let info = bv_ofx::registry().info(&fx.id)?;
    let d = info.described.first()?;
    d.params.iter().find(|p| p.name == name).cloned()
}

/// Numbers a parameter keeps that an envelope draws.
fn numeric(ty: ParamType) -> bool {
    matches!(
        ty,
        ParamType::Integer
            | ParamType::Double
            | ParamType::Rgba
            | ParamType::Rgb
            | ParamType::Double2D
            | ParamType::Integer2D
            | ParamType::Double3D
            | ParamType::Integer3D
    )
}

fn is_int(ty: ParamType) -> bool {
    matches!(
        ty,
        ParamType::Integer | ParamType::Integer2D | ParamType::Integer3D
    )
}

/// Number `d` of a value.
fn num(v: &PluginValue, d: usize) -> Option<f64> {
    match v {
        PluginValue::Num(v) => v.get(d).copied(),
        _ => None,
    }
}

/// The envelopes of `fx`'s animated numbers, owned by `owner` (effect
/// `effect`), drawn in `area`, but for those `hidden` hides.
#[allow(clippy::too_many_arguments)]
fn effect_lines(
    fx: &PluginFx,
    owner: Owner,
    effect: usize,
    origin: Tick,
    area: Rect,
    prefs: &crate::settings::Prefs,
    hidden: Option<&HashSet<EnvelopeKey>>,
    out: &mut Vec<Line>,
) {
    // Each envelope's color by its place among the effect's.
    let mut k = 0;
    for (name, p) in &fx.params {
        if p.keys.is_empty() {
            continue;
        }
        let Some(info) = param_info(fx, name) else {
            continue;
        };
        if !numeric(info.ty) || info.secret {
            continue;
        }
        let n = match &p.value {
            PluginValue::Num(v) => v.len(),
            _ => 0,
        };
        for c in 0..n {
            let keys: Vec<f64> = p.keys.iter().filter_map(|k| num(&k.value, c)).collect();
            let pick = |list: &[f64], fallback: f64| {
                list.get(c)
                    .copied()
                    .filter(|v| v.is_finite())
                    .unwrap_or(fallback)
            };
            let seen_lo = keys.iter().copied().fold(f64::INFINITY, f64::min);
            let seen_hi = keys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let mut lo = pick(&info.display_min, pick(&info.min, seen_lo));
            let mut hi = pick(&info.display_max, pick(&info.max, seen_hi));
            // A range far wider than what the keyframes use shows them flat:
            // theirs, then, with room.
            if !(lo.is_finite() && hi.is_finite()) || hi - lo > 1e6 {
                lo = seen_lo;
                hi = seen_hi;
            }
            if hi - lo < 1e-9 {
                lo -= 1.0;
                hi += 1.0;
            }
            let color = prefs.envelope_color(crate::settings::EnvelopeColor::FxParameter(
                (k % 10) as u8 + 1,
            ));
            k += 1;
            if hidden.is_some_and(|h| h.contains(&(owner, effect, name.clone(), c))) {
                continue;
            }
            let mut label = format!("{}: {}", fx.name, tr(&info.label));
            if n > 1 {
                let dim = info
                    .dimension_labels
                    .get(c)
                    .cloned()
                    .unwrap_or_else(|| ["X", "Y", "Z", "A"][c.min(3)].to_string());
                label.push_str(&format!(" {dim}"));
            }
            out.push(Line {
                owner,
                effect,
                param: name.clone(),
                component: c,
                ty: info.ty,
                lo,
                hi,
                origin,
                area,
                color,
                label,
            });
        }
    }
}

/// Track `t`'s effect picked in its keyframes area: its place in the
/// chain and the plug-in.
fn picked_effect<'a>(
    view: &TimelineView,
    t: &'a bv_timeline::Track,
) -> Option<(usize, &'a PluginFx)> {
    let picked = *view.track_keys.get(&t.id)?;
    t.effects
        .iter()
        .enumerate()
        .filter_map(|(i, e)| e.plugin.as_deref().map(|f| (i, f)))
        .nth(picked)
}

/// Where a track's keyframes area (`keys`) draws its envelopes: under its
/// lines.
fn envelopes_rect(keys: Rect, track: &bv_timeline::Track) -> Rect {
    let effects = track.effects.iter().filter(|e| e.plugin.is_some()).count();
    Rect::from_min_max(
        pos2(
            keys.left(),
            keys.top() + LINE_H * (effects + 1) as f32 + 3.0,
        ),
        pos2(keys.right(), keys.bottom() - 3.0),
    )
}

/// Every envelope drawn now.
fn lines(
    view: &TimelineView,
    l: &Layout,
    project: &Project,
    prefs: &crate::settings::Prefs,
) -> Vec<Line> {
    let mut out = Vec::new();
    if !view.show_ofx_envelopes {
        return out;
    }
    for row in &l.rows {
        let Some(track) = project
            .track(row.track)
            .filter(|t| t.kind == TrackKind::Video)
        else {
            continue;
        };
        // The track's effect picked, in its keyframes area.
        if row.keys > 0.0
            && let Some((effect, fx)) = picked_effect(view, track)
        {
            let area = envelopes_rect(row.keys_rect(l.lanes.left(), l.lanes.right()), track);
            effect_lines(
                fx,
                Owner::Track(track.id),
                effect,
                Tick::ZERO,
                area,
                prefs,
                Some(&view.ofx_hidden),
                &mut out,
            );
        }
        // The events' effects, over the events.
        for clip in &track.clips {
            let r = super::lanes::clip_rect(view, l, row, clip.start, clip.end());
            if r.right() < l.lanes.left() || r.left() > l.lanes.right() {
                continue;
            }
            let title = if view.show_clip_headers {
                super::lanes::TITLE_H
            } else {
                0.0
            };
            let area = Rect::from_min_max(
                pos2(r.left(), r.top() + title + 3.0),
                pos2(r.right(), r.bottom() - 3.0),
            );
            for (i, e) in clip.effects.iter().enumerate() {
                if let Some(fx) = e.plugin.as_deref() {
                    effect_lines(
                        fx,
                        Owner::Clip(clip.id),
                        i,
                        clip.start,
                        area,
                        prefs,
                        Some(&view.ofx_hidden),
                        &mut out,
                    );
                }
            }
        }
    }
    out
}

/// The envelopes of track `track`'s keyframes area, the hidden ones too.
fn area_envelopes(
    view: &TimelineView,
    project: &Project,
    prefs: &crate::settings::Prefs,
    track: TrackId,
) -> Vec<Line> {
    let mut out = Vec::new();
    if let Some(t) = project.track(track)
        && let Some((effect, fx)) = picked_effect(view, t)
    {
        effect_lines(
            fx,
            Owner::Track(track),
            effect,
            Tick::ZERO,
            Rect::NOTHING,
            prefs,
            None,
            &mut out,
        );
    }
    out
}

/// The lines of track `t`'s keyframes area — each plug-in effect's, then
/// Track Motion's — with their keyframes' times.
fn area_rows(t: &bv_timeline::Track) -> Vec<(AreaRow, Vec<Tick>)> {
    let mut rows: Vec<(AreaRow, Vec<Tick>)> = t
        .effects
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let fx = e.plugin.as_deref()?;
            let mut times: Vec<Tick> = fx
                .params
                .values()
                .flat_map(|p| p.keys.iter().map(|k| k.at))
                .collect();
            times.sort();
            times.dedup();
            Some((AreaRow::Effect(i), times))
        })
        .collect();
    let motion = t
        .motion
        .as_ref()
        .map_or_else(Vec::new, |m| m.keys.iter().map(|k| k.at).collect());
    rows.push((AreaRow::Motion, motion));
    rows
}

/// The video track whose keyframes area is under `p`.
pub(crate) fn area_at(
    view: &TimelineView,
    l: &Layout,
    project: &Project,
    p: Pos2,
) -> Option<TrackId> {
    if !view.show_ofx_envelopes {
        return None;
    }
    l.rows
        .iter()
        .filter(|r| r.keys > 0.0)
        .filter(|r| {
            project
                .track(r.track)
                .is_some_and(|t| t.kind == TrackKind::Video)
        })
        .find(|r| r.keys_rect(l.lanes.left(), l.lanes.right()).contains(p))
        .map(|r| r.track)
}

/// The keyframe of a keyframes area's line under `p`: its track, its line
/// and its time.
fn area_key_at(
    view: &TimelineView,
    l: &Layout,
    project: &Project,
    p: Pos2,
) -> Option<(TrackId, AreaRow, Tick)> {
    let track = area_at(view, l, project, p)?;
    let row = l.rows.iter().find(|r| r.track == track)?;
    let area = row.keys_rect(l.lanes.left(), l.lanes.right());
    let n = ((p.y - area.top()) / LINE_H).floor();
    if n < 0.0 {
        return None;
    }
    let rows = area_rows(project.track(track)?);
    let (r, times) = rows.get(n as usize)?;
    let at = times
        .iter()
        .copied()
        .find(|&t| (view.x_of(l, t) - p.x).abs() <= 4.0)?;
    Some((track, *r, at))
}

fn param_of<'a>(project: &'a Project, line: &Line) -> Option<&'a PluginParam> {
    chain(project, line.owner)?
        .get(line.effect)?
        .plugin
        .as_deref()?
        .params
        .get(&line.param)
}

/// The envelope's value at the owner's time `t`.
fn value_at(project: &Project, line: &Line, t: Tick) -> Option<f64> {
    let rate = project.settings.frame_rate;
    let p = param_of(project, line)?;
    match animation(p, rate).at(frames(t, rate), line.ty) {
        Value::Num(v) => v.get(line.component).copied(),
        _ => None,
    }
}

fn y_of(line: &Line, v: f64) -> f32 {
    let u = ((v - line.lo) / (line.hi - line.lo)).clamp(0.0, 1.0) as f32;
    line.area.bottom() - u * line.area.height()
}

fn v_of(line: &Line, y: f32) -> f64 {
    let u = ((line.area.bottom() - y) / line.area.height().max(1.0)).clamp(0.0, 1.0) as f64;
    let v = line.lo + u * (line.hi - line.lo);
    if is_int(line.ty) { v.round() } else { v }
}

/// `y_of` and `v_of` past the envelope's range: a handle's.
fn y_free(line: &Line, v: f64) -> f32 {
    let u = ((v - line.lo) / (line.hi - line.lo)) as f32;
    line.area.bottom() - u * line.area.height()
}

fn v_free(line: &Line, y: f32) -> f64 {
    let u = ((line.area.bottom() - y) / line.area.height().max(1.0)) as f64;
    line.lo + u * (line.hi - line.lo)
}

/// The frame's start nearest `t`.
fn on_frame(rate: FrameRate, t: Tick) -> Tick {
    let f = (t.as_seconds_f64() * rate.as_f64()).round() as i64;
    rate.frame_start(f.max(0))
}

/// Point `k`'s handles on the screen (the one before it, the one after
/// it), when it is a Bézier point.
fn handle_pos(
    view: &TimelineView,
    l: &Layout,
    project: &Project,
    line: &Line,
    k: usize,
) -> Option<(Pos2, Pos2)> {
    let p = param_of(project, line)?;
    let key = p.keys.get(k).filter(|k| k.curve.is_bezier())?;
    let v = num(&key.value, line.component)?;
    let h = *handles(p, k, project.settings.frame_rate).get(line.component)?;
    let at = |(dt, dv): (Tick, f64)| {
        pos2(
            view.x_of(l, line.origin + key.at + dt),
            y_free(line, v + dv),
        )
    };
    Some((at(h.before), at(h.after)))
}

/// The envelope point, Bézier handle or line under `p`.
pub(crate) fn hit(
    view: &TimelineView,
    l: &Layout,
    project: &Project,
    prefs: &crate::settings::Prefs,
    p: Pos2,
) -> Option<OfxHit> {
    let all = lines(view, l, project, prefs);
    let near = |a: Pos2| (a.x - p.x).abs() <= 5.0 && (a.y - p.y).abs() <= 5.0;
    for line in all.iter().filter(|line| line.area.expand(4.0).contains(p)) {
        let keys = param_of(project, line).map_or(&[][..], |p| &p.keys[..]);
        for (i, k) in keys.iter().enumerate() {
            if let Some(v) = num(&k.value, line.component)
                && near(pos2(view.x_of(l, line.origin + k.at), y_of(line, v)))
            {
                return Some(OfxHit {
                    line: line.clone(),
                    key: Some(i),
                    handle: None,
                    at: k.at,
                    value: v,
                    pos: p,
                });
            }
        }
    }
    for line in &all {
        let keys = param_of(project, line).map_or(&[][..], |p| &p.keys[..]);
        for (i, k) in keys.iter().enumerate() {
            let Some((before, after)) = handle_pos(view, l, project, line, i) else {
                continue;
            };
            for (side, at) in [(false, before), (true, after)] {
                if near(at) && l.lanes.contains(at) {
                    return Some(OfxHit {
                        line: line.clone(),
                        key: Some(i),
                        handle: Some(side),
                        at: k.at,
                        value: num(&k.value, line.component).unwrap_or(0.0),
                        pos: p,
                    });
                }
            }
        }
    }
    for line in all.iter().filter(|line| line.area.contains(p)) {
        let at = view.time_at(l, p.x) - line.origin;
        if let Some(v) = value_at(project, line, at)
            && (y_of(line, v) - p.y).abs() <= 4.0
        {
            return Some(OfxHit {
                line: line.clone(),
                key: None,
                handle: None,
                at,
                value: v,
                pos: p,
            });
        }
    }
    None
}

/// Drags points, their handles and the keyframes areas' keyframes, adds
/// points with a double-click on a line, shows "Set to…", and keeps the
/// rest of the timeline's mouse off what it takes.
pub(crate) fn interact(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
) {
    if !view.show_ofx_envelopes {
        view.ofx_drag = None;
        view.ofx_set_to = None;
        return;
    }
    set_to_ui(view, ui, app);
    let project = app.project().clone();
    let rate = project.settings.frame_rate;
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let (pressed, down, double) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer
                .button_double_clicked(egui::PointerButton::Primary),
        )
    });
    if let Some(d) = view.ofx_drag.clone() {
        match pointer.filter(|_| down) {
            Some(p) => drag_to(view, ui, app, l, d, p),
            None => view.ofx_drag = None,
        }
        return;
    }
    let Some(p) = pointer.filter(|p| l.lanes.contains(*p)) else {
        return;
    };
    // A keyframes area line's keyframe: its time dragged, its values kept.
    if let Some((track, row, at)) = area_key_at(view, l, &project, p) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        if pressed {
            view.ignore_press = true;
            view.ofx_drags += 1;
            view.ofx_drag = Some(OfxDrag::Times {
                track,
                row,
                from: at,
                undo: egui::Id::new(("ofx-envelope", view.ofx_drags)),
            });
        }
        return;
    }
    let prefs = app.settings.prefs.clone();
    let Some(h) = hit(view, l, &project, &prefs, p) else {
        return;
    };
    ui.ctx().set_cursor_icon(if h.key.is_some() {
        egui::CursorIcon::Grab
    } else {
        egui::CursorIcon::PointingHand
    });
    // The value under the pointer, unless a menu covers it.
    if !egui::Popup::is_any_open(ui.ctx()) {
        view.ofx_hover = Some((h.line.label.clone(), param_text(&h.line, h.value), p));
    }
    if double && h.key.is_none() {
        let at = on_frame(rate, h.line.origin + h.at) - h.line.origin;
        let value = v_of(&h.line, p.y);
        let line = h.line.clone();
        app.modify(|pr| add_key(pr, &line, at.max(Tick::ZERO), value));
        view.ignore_press = true;
    } else if pressed {
        view.ignore_press = true;
        if let Some(k) = h.key {
            view.ofx_drags += 1;
            view.ofx_drag = Some(OfxDrag::Point {
                line: h.line,
                key: k,
                handle: h.handle,
                undo: egui::Id::new(("ofx-envelope", view.ofx_drags)),
            });
        }
    }
}

/// What is dragged follows the pointer, at `p`.
fn drag_to(
    view: &mut TimelineView,
    ui: &egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
    d: OfxDrag,
    p: Pos2,
) {
    let rate = app.project().settings.frame_rate;
    // At the tracks' sides the view runs on, as VEGAS's.
    let p = pos2(view.edge_scroll_x(ui, l, p.x), p.y);
    match d {
        OfxDrag::Point {
            line,
            key,
            handle: None,
            undo,
        } => {
            let at = (on_frame(rate, view.time_at(l, p.x)) - line.origin).max(Tick::ZERO);
            let value = v_of(&line, p.y);
            app.modify_merged(undo, |pr| move_key(pr, &line, key, at, value));
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        OfxDrag::Point {
            line,
            key,
            handle: Some(after),
            undo,
        } => {
            let at = view.time_at(l, p.x) - line.origin;
            let value = v_free(&line, p.y);
            app.modify_merged(undo, |pr| move_handle(pr, &line, key, after, at, value));
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        OfxDrag::Times {
            track,
            row,
            from,
            undo,
        } => {
            let to = on_frame(rate, view.time_at(l, p.x));
            let now = app.modify_merged(undo, |pr| move_times(pr, track, row, from, to));
            if let Some(OfxDrag::Times { from, .. }) = &mut view.ofx_drag {
                *from = now;
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
    }
}

fn param_text(line: &Line, v: f64) -> String {
    crate::fmt::num(v, if is_int(line.ty) { 0 } else { 3 })
}

/// "Set to…": a small box where a point's value is typed (Enter or OK
/// sets it, Escape or Cancel leaves it).
fn set_to_ui(view: &mut TimelineView, ui: &egui::Ui, app: &mut BananaApp) {
    let Some(s) = &mut view.ofx_set_to else {
        return;
    };
    let (mut done, mut commit) = (false, false);
    egui::Area::new(egui::Id::new("ofx-envelope-set-to"))
        .order(egui::Order::Foreground)
        .fixed_pos(s.pos)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(format!("{}:", s.line.label));
                let field = ui.add(egui::TextEdit::singleline(&mut s.text).desired_width(120.0));
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
    if commit
        && let Some(v) = crate::ofx::parse_numbers(&s.text, 1).and_then(|v| v.first().copied())
    {
        let (line, key) = (s.line.clone(), s.key);
        let v = if is_int(line.ty) { v.round() } else { v };
        app.modify(|p| set_value(p, &line, key, v));
        done = true;
    }
    if done {
        view.ofx_set_to = None;
    }
}

/// The track keyframes areas and the envelopes, over the events.
pub(crate) fn paint(
    view: &mut TimelineView,
    ui: &egui::Ui,
    app: &BananaApp,
    l: &Layout,
    project: &Project,
) {
    let hover = view.ofx_hover.take();
    view.ofx_painted.clear();
    view.ofx_areas.clear();
    if !view.show_ofx_envelopes {
        return;
    }
    let painter = ui.painter().with_clip_rect(l.lanes);
    let font = FontId::proportional(10.0);
    // The track keyframes areas: a line for each effect and Track Motion,
    // their keyframes as diamonds.
    for row in &l.rows {
        if row.keys <= 0.0 {
            continue;
        }
        let Some(track) = project
            .track(row.track)
            .filter(|t| t.kind == TrackKind::Video)
        else {
            continue;
        };
        let area = row.keys_rect(l.lanes.left(), l.lanes.right());
        painter.rect_filled(area, 0.0, theme::panel_dark());
        view.ofx_areas.push(envelopes_rect(area, track));
        let picked = picked_effect(view, track).map(|(i, _)| i);
        let mut y = area.top();
        for (r, times) in area_rows(track) {
            let on = matches!(r, AreaRow::Effect(i) if Some(i) == picked);
            let mid = y + LINE_H / 2.0;
            painter.hline(
                area.x_range(),
                mid,
                Stroke::new(1.0, Color32::from_white_alpha(if on { 60 } else { 25 })),
            );
            for t in times {
                let x = view.x_of(l, t);
                let d = 3.5;
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        pos2(x, mid - d),
                        pos2(x + d, mid),
                        pos2(x, mid + d),
                        pos2(x - d, mid),
                    ],
                    if on {
                        theme::banana()
                    } else {
                        theme::text_dim()
                    },
                    Stroke::NONE,
                ));
            }
            y += LINE_H;
        }
    }
    // The envelopes, their points and a Bézier point's handles.
    let prefs = &app.settings.prefs;
    for line in lines(view, l, project, prefs) {
        let x0 = line.area.left().max(l.lanes.left());
        let x1 = line.area.right().min(l.lanes.right());
        if x1 <= x0 {
            continue;
        }
        let mut pts = Vec::new();
        let mut x = x0;
        while x <= x1 {
            let t = view.time_at(l, x) - line.origin;
            if let Some(v) = value_at(project, &line, t.max(Tick::ZERO)) {
                pts.push(pos2(x, y_of(&line, v)));
            }
            x += 3.0;
        }
        painter.add(egui::Shape::line(pts, Stroke::new(1.5, line.color)));
        let Some(p) = param_of(project, &line) else {
            continue;
        };
        for (i, k) in p.keys.iter().enumerate() {
            let Some(v) = num(&k.value, line.component) else {
                continue;
            };
            let c = pos2(view.x_of(l, line.origin + k.at), y_of(&line, v));
            if let Some((before, after)) = handle_pos(view, l, project, &line, i) {
                let dim = line.color.gamma_multiply(0.6);
                for h in [before, after] {
                    painter.line_segment([c, h], Stroke::new(1.0, dim));
                    painter.circle_filled(h, 3.0, dim);
                }
            }
            let r = Rect::from_center_size(c, vec2(6.0, 6.0));
            painter.rect_filled(r, 0.0, line.color);
            painter.rect_stroke(
                r,
                0.0,
                Stroke::new(1.0, Color32::BLACK),
                egui::StrokeKind::Outside,
            );
            view.ofx_painted.push((i, c));
        }
    }
    // The value under the pointer.
    if let Some((label, value, at)) = hover {
        let tip = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("valor-ofx"),
        ));
        let galley = tip.layout_no_wrap(format!("{label}: {value}"), font, theme::text());
        let r = Rect::from_min_size(at + vec2(12.0, -18.0), galley.size()).expand(3.0);
        tip.rect_filled(r, 3.0, theme::panel_dark());
        tip.galley(r.min + vec2(3.0, 3.0), galley, theme::text());
    }
}

/// The keyframes areas' names in the track headers: each effect's (a
/// click shows its envelopes) and Track Motion's.
pub(crate) fn headers_ui(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    l: &Layout,
    project: &Project,
) {
    if !view.show_ofx_envelopes {
        return;
    }
    let font = FontId::proportional(10.0);
    for row in &l.rows {
        if row.keys <= 0.0 {
            continue;
        }
        let Some(track) = project
            .track(row.track)
            .filter(|t| t.kind == TrackKind::Video)
        else {
            continue;
        };
        let head = row.keys_rect(l.headers.left(), l.headers.right());
        ui.painter().rect_filled(head, 0.0, theme::panel_dark());
        let picked = view.track_keys.get(&track.id).copied();
        let mut y = head.top();
        let names: Vec<String> = track
            .effects
            .iter()
            .filter_map(|e| e.plugin.as_deref())
            .map(|fx| fx.name.clone())
            .collect();
        for (n, name) in names.iter().enumerate() {
            let r = Rect::from_min_size(pos2(head.left(), y), vec2(head.width(), LINE_H));
            let resp = ui.interact(r, ui.id().with(("ofx-keys", track.id, n)), Sense::click());
            let on = picked == Some(n);
            let color = if on { theme::banana() } else { theme::text() };
            ui.painter().text(
                pos2(r.left() + 18.0, r.center().y),
                Align2::LEFT_CENTER,
                name,
                font.clone(),
                color,
            );
            // Expand Keyframes: this effect's envelopes show.
            let dot = pos2(r.left() + 9.0, r.center().y);
            ui.painter().circle(
                dot,
                3.5,
                if on {
                    theme::banana()
                } else {
                    Color32::TRANSPARENT
                },
                Stroke::new(1.0, color),
            );
            if resp
                .on_hover_text(tr("Expand Keyframes: show this effect's envelopes"))
                .clicked()
            {
                view.track_keys.insert(track.id, n);
            }
            y += LINE_H;
        }
        ui.painter().text(
            pos2(head.left() + 18.0, y + LINE_H / 2.0),
            Align2::LEFT_CENTER,
            tr("Track Motion"),
            font.clone(),
            theme::text_dim(),
        );
    }
}

/// A point's or a line's menu, as VEGAS's: Add Point (at the cursor),
/// Set to… (a point's value), the curves (a point's, or a line's point
/// before the pointer), Flip, Thin, Reset and Delete (a point).
pub(crate) fn menu(ui: &mut egui::Ui, view: &mut TimelineView, app: &mut BananaApp, h: &OfxHit) {
    ui.label(egui::RichText::new(&h.line.label).color(h.line.color));
    ui.separator();
    let line = h.line.clone();
    let keys = param_of(app.project(), &line).map_or_else(Vec::new, |p| p.keys.clone());
    let point = h.key.or_else(|| keys.iter().rposition(|k| k.at <= h.at));
    if ui
        .button(egui::RichText::new(tr("Add Point")).strong())
        .clicked()
    {
        let rate = app.project().settings.frame_rate;
        let at = (on_frame(rate, app.cursor) - line.origin).max(Tick::ZERO);
        let value = value_at(app.project(), &line, at).unwrap_or(h.value);
        app.modify(|p| add_key(p, &line, at, value));
        ui.close();
    }
    if ui
        .add_enabled(h.key.is_some(), egui::Button::new(tr("Set to…")))
        .clicked()
        && let Some(k) = h.key
    {
        view.ofx_set_to = Some(OfxSetTo {
            line: line.clone(),
            key: k,
            text: param_text(&line, h.value),
            pos: h.pos + vec2(8.0, 8.0),
            focus: true,
        });
        ui.close();
    }
    ui.separator();
    let curve = point.and_then(|k| keys.get(k)).map(|k| k.curve);
    ui.add_enabled_ui(point.is_some(), |ui| {
        for (c, label) in CURVES {
            if crate::widgets::radio(ui, curve == Some(c), tr(label)).clicked()
                && let Some(k) = point
            {
                app.modify(|p| set_curve(p, &line, k, c));
                ui.close();
            }
        }
    });
    ui.separator();
    if ui.button(tr("Flip")).clicked() {
        app.modify(|p| flip(p, &line));
        ui.close();
    }
    if ui.button(tr("Thin")).clicked() {
        app.modify(|p| thin(p, &line));
        ui.close();
    }
    if ui.button(tr("Reset")).clicked() {
        let at = (app.cursor - line.origin).max(Tick::ZERO);
        app.modify(|p| reset(p, &line, at));
        ui.close();
    }
    if ui
        .add_enabled(h.key.is_some(), egui::Button::new(tr("Delete")))
        .clicked()
        && let Some(k) = h.key
    {
        app.modify(|p| delete_key(p, &line, k));
        ui.close();
    }
}

/// A keyframes area's menu, as VEGAS's: a point at the cursor on every
/// envelope it shows, and each of its envelopes shown or hidden.
pub(crate) fn area_menu(
    ui: &mut egui::Ui,
    view: &mut TimelineView,
    app: &mut BananaApp,
    track: TrackId,
) {
    let prefs = app.settings.prefs.clone();
    let all = area_envelopes(view, app.project(), &prefs, track);
    let shown: Vec<Line> = all
        .iter()
        .filter(|l| !view.ofx_hidden.contains(&l.key()))
        .cloned()
        .collect();
    if ui
        .add_enabled(
            !shown.is_empty(),
            egui::Button::new(egui::RichText::new(tr("Add Point")).strong()),
        )
        .clicked()
    {
        let cursor = app.cursor;
        app.modify(|p| add_points(p, &shown, cursor));
        ui.close();
    }
    if !all.is_empty() {
        ui.separator();
    }
    for line in &all {
        let key = line.key();
        let mut on = !view.ofx_hidden.contains(&key);
        if ui
            .checkbox(&mut on, egui::RichText::new(&line.label).color(line.color))
            .clicked()
        {
            if on {
                view.ofx_hidden.remove(&key);
            } else {
                view.ofx_hidden.insert(key);
            }
        }
    }
}

/// Point `k` of `line` to the owner's time `at` and `value` (the time
/// shared by the parameter's numbers; not past its neighbours).
fn move_key(p: &mut Project, line: &Line, k: usize, at: Tick, value: f64) {
    let Some(param) = param_mut(p, line) else {
        return;
    };
    let before = k
        .checked_sub(1)
        .and_then(|b| param.keys.get(b))
        .map(|b| b.at);
    let after = param.keys.get(k + 1).map(|a| a.at);
    let at = match (before, after) {
        (Some(b), _) if at <= b => param.keys[k].at,
        (_, Some(a)) if at >= a => param.keys[k].at,
        _ => at,
    };
    if let Some(key) = param.keys.get_mut(k) {
        key.at = at;
        if let PluginValue::Num(v) = &mut key.value
            && let Some(x) = v.get_mut(line.component)
        {
            *x = value;
        }
    }
}

/// Point `k`'s handle (the one after it, or the one before) to the
/// owner's time `at` and `value`.
fn move_handle(p: &mut Project, line: &Line, k: usize, after: bool, at: Tick, value: f64) {
    let rate = p.settings.frame_rate;
    let Some(param) = param_mut(p, line) else {
        return;
    };
    let Some((t, v)) = param
        .keys
        .get(k)
        .and_then(|key| Some((key.at, num(&key.value, line.component)?)))
    else {
        return;
    };
    set_handle(param, k, line.component, after, (at - t, value - v), rate);
}

/// The keyframes of a keyframes area's line at `from` to `to`, kept a
/// frame from the line's keyframes before and after them, their values
/// as they were; where they are now.
fn move_times(p: &mut Project, track: TrackId, row: AreaRow, from: Tick, to: Tick) -> Tick {
    let frame = p.settings.frame_rate.frame_duration();
    let Some(t) = p.track_by_id_mut(track) else {
        return from;
    };
    let times = area_rows(t)
        .into_iter()
        .find(|(r, _)| *r == row)
        .map_or_else(Vec::new, |(_, v)| v);
    let Some(i) = times.iter().position(|&x| x == from) else {
        return from;
    };
    let lo = i.checked_sub(1).map_or(Tick::ZERO, |j| times[j] + frame);
    let to = match times.get(i + 1).map(|&n| n - frame) {
        Some(hi) if hi < lo => return from,
        Some(hi) => to.clamp(lo, hi),
        None => to.max(lo),
    };
    match row {
        AreaRow::Effect(e) => {
            if let Some(fx) = t.effects.get_mut(e).and_then(|e| e.plugin.as_mut()) {
                for param in fx.params.values_mut() {
                    for k in param.keys.iter_mut().filter(|k| k.at == from) {
                        k.at = to;
                    }
                }
            }
        }
        AreaRow::Motion => {
            if let Some(m) = &mut t.motion {
                for k in m.keys.iter_mut().filter(|k| k.at == from) {
                    k.at = to;
                }
            }
        }
    }
    to
}

/// A point at the owner's time `at`: the parameter's numbers there, with
/// `line`'s at `value`.
fn add_key(p: &mut Project, line: &Line, at: Tick, value: f64) {
    let rate = p.settings.frame_rate;
    let Some(param) = param_mut(p, line) else {
        return;
    };
    if param.keys.iter().any(|k| k.at == at) {
        return;
    }
    let Value::Num(mut v) = animation(param, rate).at(frames(at, rate), line.ty) else {
        return;
    };
    if let Some(x) = v.get_mut(line.component) {
        *x = value;
    }
    let Some(value) = plugin_value(&Value::Num(v)) else {
        return;
    };
    let i = param.keys.partition_point(|k| k.at < at);
    let curve = i
        .checked_sub(1)
        .and_then(|b| param.keys.get(b))
        .map_or(KeyCurve::Linear, |k| k.curve);
    param.keys.insert(i, PluginKey::new(at, value, curve));
}

/// A point at the cursor on each of `lines`, keeping their values there.
fn add_points(p: &mut Project, lines: &[Line], cursor: Tick) {
    let rate = p.settings.frame_rate;
    for line in lines {
        let at = (on_frame(rate, cursor) - line.origin).max(Tick::ZERO);
        if let Some(v) = value_at(p, line, at) {
            add_key(p, line, at, v);
        }
    }
}

fn delete_key(p: &mut Project, line: &Line, k: usize) {
    let Some(param) = param_mut(p, line) else {
        return;
    };
    if k >= param.keys.len() {
        return;
    }
    let gone = param.keys.remove(k);
    // The last one gone: its value holds.
    if param.keys.is_empty() {
        param.value = gone.value;
    }
}

/// Point `k`'s curve (a Bézier one gets handles that keep the envelope's
/// shape).
fn set_curve(p: &mut Project, line: &Line, k: usize, c: KeyCurve) {
    let rate = p.settings.frame_rate;
    if let Some(param) = param_mut(p, line) {
        bv_engine::plugins::set_curve(param, k, c, rate);
    }
}

/// Set to…: point `k`'s value on `line`.
fn set_value(p: &mut Project, line: &Line, k: usize, value: f64) {
    if let Some(PluginValue::Num(v)) = param_mut(p, line)
        .and_then(|param| param.keys.get_mut(k))
        .map(|key| &mut key.value)
        && let Some(x) = v.get_mut(line.component)
    {
        *x = value;
    }
}

/// Flip: each point's value mirrored about the middle of the range, a
/// Bézier point's handles with it.
fn flip(p: &mut Project, line: &Line) {
    let Some(param) = param_mut(p, line) else {
        return;
    };
    for key in &mut param.keys {
        if let PluginValue::Num(v) = &mut key.value
            && let Some(x) = v.get_mut(line.component)
        {
            *x = line.lo + line.hi - *x;
        }
        if let Some(h) = key.handles.get_mut(line.component) {
            h.before.1 = -h.before.1;
            h.after.1 = -h.after.1;
        }
    }
}

/// Thin: the points left out that the shape — every number of the
/// parameter — keeps without, within a two-hundredth of each number's
/// range (this envelope's display range; the others', their keyframes').
fn thin(p: &mut Project, line: &Line) {
    let rate = p.settings.frame_rate;
    let Some(param) = param_mut(p, line) else {
        return;
    };
    let n = match &param.value {
        PluginValue::Num(v) => v.len(),
        _ => return,
    };
    let tolerance: Vec<f64> = (0..n)
        .map(|d| {
            let span = if d == line.component {
                line.hi - line.lo
            } else {
                let values = param.keys.iter().filter_map(|k| num(&k.value, d));
                let (lo, hi) = values.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
                    (a.min(v), b.max(v))
                });
                hi - lo
            };
            (span / 200.0).max(1e-9)
        })
        .collect();
    let ty = ParamType::Double;
    let mut i = 1;
    while i + 1 < param.keys.len() {
        let mut without = param.clone();
        without.keys.remove(i);
        let (a, b) = (animation(param, rate), animation(&without, rate));
        let t0 = frames(param.keys[i - 1].at, rate);
        let t1 = frames(param.keys[i + 1].at, rate);
        let same = (0..=32)
            .map(|s| t0 + (t1 - t0) * s as f64 / 32.0)
            .chain([frames(param.keys[i].at, rate)])
            .all(|t| {
                let (x, y) = (a.at(t, ty), b.at(t, ty));
                (0..n).all(|d| (x.num(d) - y.num(d)).abs() <= tolerance[d])
            });
        if same {
            *param = without;
        } else {
            i += 1;
        }
    }
}

/// Reset: no keyframes, the parameter's value at the owner's time `at`.
fn reset(p: &mut Project, line: &Line, at: Tick) {
    let rate = p.settings.frame_rate;
    let Some(param) = param_mut(p, line) else {
        return;
    };
    if let Some(v) = plugin_value(&animation(param, rate).at(frames(at, rate), line.ty)) {
        param.value = v;
    }
    param.keys.clear();
}

impl BananaApp {
    /// The demo's video track with an animated plug-in effect, its track
    /// keyframes area open and View › Show OFX Effect Envelopes (Beta) on
    /// (screen tests).
    pub fn ofx_envelopes_for_review(&mut self) -> bool {
        let id = "org.bananav.BrightnessContrast".to_string();
        let made = self.modify(|p| {
            let t = p.tracks.iter().find(|t| t.kind == TrackKind::Video)?.id;
            let rate = p.settings.frame_rate;
            let mut fx = PluginFx {
                id,
                version: [1, 0],
                name: "Brightness and Contrast".into(),
                params: Default::default(),
            };
            let num = |v: f64| PluginValue::Num(vec![v]);
            fx.params.insert(
                "brightness".into(),
                PluginParam {
                    value: num(0.0),
                    keys: vec![
                        PluginKey::new(Tick::ZERO, num(-0.5), KeyCurve::Smooth),
                        PluginKey::new(rate.frame_start(150), num(0.6), KeyCurve::Linear),
                        PluginKey::new(rate.frame_start(420), num(0.1), KeyCurve::Linear),
                    ],
                },
            );
            p.track_by_id_mut(t)?.effects.push(Effect::plugin(fx));
            Some(t)
        });
        let Some(t) = made else {
            return false;
        };
        self.timeline.show_ofx_envelopes = true;
        self.timeline.track_keys.insert(t, 0);
        true
    }

    /// The review envelope's middle point made a locked Bézier one, its
    /// handle after it pulled up (screen tests).
    pub fn ofx_bezier_for_review(&mut self) -> bool {
        let done = self.modify(|p| {
            let rate = p.settings.frame_rate;
            let param = p
                .tracks
                .iter_mut()
                .filter(|t| t.kind == TrackKind::Video)
                .flat_map(|t| t.effects.iter_mut())
                .filter_map(|e| e.plugin.as_mut())
                .find_map(|fx| fx.params.get_mut("brightness"))?;
            bv_engine::plugins::set_curve(param, 1, KeyCurve::Bezier, rate);
            set_handle(param, 1, 0, true, (rate.frame_start(60), 0.5), rate);
            Some(())
        });
        done.is_some()
    }

    /// Where the timeline drew the envelopes' point `k` (the first
    /// envelope's), and the first keyframes area's envelopes (screen
    /// tests).
    pub fn ofx_point_for_review(&self, k: usize) -> Option<egui::Pos2> {
        self.timeline
            .ofx_painted
            .iter()
            .find(|(i, _)| *i == k)
            .map(|(_, p)| *p)
    }

    pub fn ofx_area_for_review(&self) -> Option<egui::Rect> {
        self.timeline.ofx_areas.first().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(p: &Project, line: &Line, k: usize) -> (Tick, f64) {
        let key = &param_of(p, line).unwrap().keys[k];
        (key.at, num(&key.value, 0).unwrap())
    }

    /// The review envelope's project and line.
    fn review() -> (Project, Line) {
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.load_demo();
        assert!(app.ofx_envelopes_for_review());
        let p = app.project().clone();
        let track = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .unwrap();
        let effect = track
            .effects
            .iter()
            .position(|e| e.plugin.is_some())
            .unwrap();
        let line = Line {
            owner: Owner::Track(track.id),
            effect,
            param: "brightness".into(),
            component: 0,
            ty: ParamType::Double,
            lo: -1.0,
            hi: 1.0,
            origin: Tick::ZERO,
            area: Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0)),
            color: Color32::WHITE,
            label: String::new(),
        };
        (p, line)
    }

    /// A point added, moved (never past its neighbours), flipped, deleted,
    /// and the envelope reset to the value at a time.
    #[test]
    fn points_are_added_moved_flipped_and_reset() {
        let (mut p, line) = review();
        let rate = p.settings.frame_rate;
        // Between the second and the third keyframes.
        add_key(&mut p, &line, rate.frame_start(300), 0.25);
        assert_eq!(value(&p, &line, 2), (rate.frame_start(300), 0.25));
        // Moved: to frame 160 and 0.4; not past frame 300's neighbour.
        move_key(&mut p, &line, 1, rate.frame_start(160), 0.4);
        assert_eq!(value(&p, &line, 1), (rate.frame_start(160), 0.4));
        move_key(&mut p, &line, 1, rate.frame_start(500), 0.4);
        assert_eq!(value(&p, &line, 1).0, rate.frame_start(160));
        // Flipped about the range's middle.
        flip(&mut p, &line);
        assert!((value(&p, &line, 1).1 + 0.4).abs() < 1e-9);
        set_curve(&mut p, &line, 1, KeyCurve::Hold);
        assert_eq!(param_of(&p, &line).unwrap().keys[1].curve, KeyCurve::Hold);
        delete_key(&mut p, &line, 2);
        assert_eq!(param_of(&p, &line).unwrap().keys.len(), 3);
        // Set to: a value typed.
        set_value(&mut p, &line, 1, 0.75);
        assert_eq!(value(&p, &line, 1).1, 0.75);
        // Reset: no keyframes, the value at the start (flipped −0.5: 0.5).
        reset(&mut p, &line, Tick::ZERO);
        let param = param_of(&p, &line).unwrap();
        assert!(param.keys.is_empty());
        assert_eq!(param.value, PluginValue::Num(vec![0.5]));
    }

    /// A Bézier point: its handles move (a locked one's together) and flip
    /// with it; Thin leaves out the points a line passes anyway; a
    /// keyframes area's keyframes move in time between their neighbours,
    /// values kept; Add Point on every envelope keeps their values.
    #[test]
    fn bezier_points_thin_and_times() {
        let (mut p, line) = review();
        let rate = p.settings.frame_rate;
        set_curve(&mut p, &line, 1, KeyCurve::Bezier);
        let key = &param_of(&p, &line).unwrap().keys[1];
        assert_eq!((key.curve, key.handles.len()), (KeyCurve::Bezier, 1));
        let at = key.at;
        move_handle(&mut p, &line, 1, true, at + rate.frame_start(30), 0.8);
        let h = param_of(&p, &line).unwrap().keys[1].handles[0];
        assert_eq!(h.after.0, rate.frame_start(30));
        assert!((h.after.1 - 0.2).abs() < 1e-9);
        assert_eq!(h.before.0, Tick(-rate.frame_start(30).0));
        assert!((h.before.1 + 0.2).abs() < 1e-9);
        flip(&mut p, &line);
        let h = param_of(&p, &line).unwrap().keys[1].handles[0];
        assert!((h.after.1 + 0.2).abs() < 1e-9);
        // Thin: a point on the straight line between its neighbours goes,
        // one off it stays.
        set_curve(&mut p, &line, 1, KeyCurve::Linear);
        let (a, b) = (value(&p, &line, 1), value(&p, &line, 2));
        let mid = Tick((a.0.0 + b.0.0) / 2);
        add_key(&mut p, &line, mid, (a.1 + b.1) / 2.0);
        let before = param_of(&p, &line).unwrap().keys.len();
        thin(&mut p, &line);
        assert_eq!(param_of(&p, &line).unwrap().keys.len(), before - 1);
        thin(&mut p, &line);
        assert_eq!(param_of(&p, &line).unwrap().keys.len(), before - 1);
        // The effect's line in the keyframes area: its middle keyframe
        // dragged, not past its neighbours, its value kept.
        let Owner::Track(track) = line.owner else {
            unreachable!()
        };
        let row = AreaRow::Effect(line.effect);
        let now = move_times(&mut p, track, row, a.0, rate.frame_start(200));
        assert_eq!(now, rate.frame_start(200));
        assert_eq!(value(&p, &line, 1), (rate.frame_start(200), a.1));
        let now = move_times(&mut p, track, row, now, rate.frame_start(9000));
        assert_eq!(now, b.0 - rate.frame_duration());
        // Add Point on every envelope shown: the value there kept.
        let cursor = rate.frame_start(100);
        let there = value_at(&p, &line, cursor).unwrap();
        add_points(&mut p, std::slice::from_ref(&line), cursor);
        let param = param_of(&p, &line).unwrap();
        let k = param.keys.iter().position(|k| k.at == cursor).unwrap();
        assert!((value(&p, &line, k).1 - there).abs() < 1e-9);
    }
}
