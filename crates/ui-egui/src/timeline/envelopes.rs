//! Envelopes on the timeline: tracks' (drawn over the track; an audio
//! track's sends' too), video events' own — velocity and transition
//! progress, drawn over the event — and busses' and chains' (on their bus
//! tracks): drawing, hit-testing, the text of a value, a line's segment
//! (what dragging the line moves) and Set To…, a point's value typed in a
//! box at the point, as VEGAS's.

use crate::i18n::{tr, trc};
use bv_core::Tick;
use bv_timeline::{
    Clip, ClipId, Envelope, EnvelopeKind, EnvelopeOwner, KeyCurve, Project, TrackId,
};

use super::{Layout, Row, TimelineView};
use crate::app::BananaApp;
use crate::egui::{self, Color32, FontId, Key, Pos2, Rect, Shape, Stroke, pos2, vec2};

/// What the pointer is over: a point of an envelope or its line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EnvHit {
    pub track: TrackId,
    /// The event whose envelope it is (`None`: the track's).
    pub clip: Option<ClipId>,
    pub kind: EnvelopeKind,
    pub point: Option<usize>,
    /// Time (on the timeline) and value on the line under the pointer.
    pub at: Tick,
    pub value: f32,
}

impl EnvHit {
    /// Whose envelope it is.
    pub fn owner(&self) -> EnvelopeOwner {
        owner(self.track, self.clip)
    }
}

/// The track's envelope, or its event `clip`'s.
pub(crate) fn owner(track: TrackId, clip: Option<ClipId>) -> EnvelopeOwner {
    match clip {
        Some(c) => EnvelopeOwner::Event(c),
        None => EnvelopeOwner::Track(track),
    }
}

/// `at` on the timeline as `owner`'s envelope counts it: an event's from
/// its start.
pub(crate) fn local(project: &Project, owner: EnvelopeOwner, at: Tick) -> Tick {
    match owner {
        EnvelopeOwner::Event(c) => at - project.clip(c).map_or(Tick::ZERO, |c| c.start),
        EnvelopeOwner::Track(_) | EnvelopeOwner::Strip(_) | EnvelopeOwner::VideoBus => at,
    }
}

/// `owner`'s `kind` envelope.
pub(crate) fn envelope_of(
    project: &Project,
    owner: EnvelopeOwner,
    kind: EnvelopeKind,
) -> Option<&Envelope> {
    match owner {
        EnvelopeOwner::Track(t) => project.track(t)?.envelopes.iter().find(|e| e.kind == kind),
        EnvelopeOwner::Event(c) => project.clip(c)?.envelope(kind),
        EnvelopeOwner::Strip(s) => project
            .strip_channel(s)?
            .envelopes
            .iter()
            .find(|e| e.kind == kind),
        EnvelopeOwner::VideoBus => project.video_bus.envelopes.iter().find(|e| e.kind == kind),
    }
}

/// The envelope's color (Options › Preferences › Display › Envelope
/// Colors).
pub(crate) fn color(prefs: &crate::settings::Prefs, kind: EnvelopeKind) -> Color32 {
    prefs.envelope_color(crate::settings::EnvelopeColor::of(kind))
}

pub(crate) fn visible(view: &TimelineView, kind: EnvelopeKind) -> bool {
    view.show_envelopes
        && match kind {
            EnvelopeKind::Volume | EnvelopeKind::Send(_) => view.show_env_volume,
            EnvelopeKind::Pan => view.show_env_pan,
            EnvelopeKind::Mute => view.show_env_mute,
            EnvelopeKind::CompositeLevel => view.show_env_composite,
            EnvelopeKind::FadeToColor => view.show_env_fade,
            EnvelopeKind::Velocity => view.show_env_velocity,
            EnvelopeKind::TransitionProgress => view.show_env_progress,
            EnvelopeKind::MotionBlur => view.show_env_motion_blur,
            EnvelopeKind::Supersampling => view.show_env_supersampling,
            EnvelopeKind::FxParam { .. } | EnvelopeKind::FxBypass(_) => true,
        }
}

/// An envelope's name: a send's is its bus's or chain's, an FX
/// automation's its effect's and parameter's ("3-Band EQ: Low (dB)").
pub(crate) fn label(project: &Project, owner: EnvelopeOwner, kind: EnvelopeKind) -> String {
    match kind {
        EnvelopeKind::Send(bv_timeline::SendTo::Bus(b)) => crate::i18n::bus_name(project, b),
        EnvelopeKind::Send(bv_timeline::SendTo::Fx(c)) => crate::i18n::chain_name(project, c),
        EnvelopeKind::FxParam { .. } | EnvelopeKind::FxBypass(_) => {
            match fx_target(project, owner, kind) {
                Some((e, Some(spec))) => format!("{}: {}", crate::ofx::fx_name(&e), tr(spec.name)),
                // A VST 3 effect's parameter: as the plug-in titles it.
                Some((e, None)) => match (kind, e.vst.as_deref()) {
                    (EnvelopeKind::FxParam { param, .. }, Some(v)) => {
                        let name = v
                            .param_names
                            .get(&param)
                            .cloned()
                            .unwrap_or_else(|| trf!("Parameter {}", param));
                        format!("{}: {}", crate::ofx::fx_name(&e), name)
                    }
                    _ => format!("{}: {}", crate::ofx::fx_name(&e), tr("Bypass")),
                },
                None => kind_label(kind).to_string(),
            }
        }
        _ => kind_label(kind).to_string(),
    }
}

/// The effect an FX automation envelope of `owner` works on, and its
/// parameter (`None`: its bypass).
pub(crate) fn fx_target(
    project: &Project,
    owner: EnvelopeOwner,
    kind: EnvelopeKind,
) -> Option<(bv_timeline::Effect, Option<bv_timeline::fx::ParamSpec>)> {
    let (effects, _) = project.fx_chain_and_envelopes(owner)?;
    let (i, what) = bv_timeline::fx_automation::target(effects, kind)?;
    let e = effects.get(i)?.clone();
    let spec = what.and_then(|p| e.kind.params().get(p as usize).copied());
    Some((e, spec))
}

/// [`value_text`], an FX parameter's in the parameter's own units.
pub(crate) fn value_text_of(
    project: &Project,
    owner: EnvelopeOwner,
    kind: EnvelopeKind,
    v: f32,
) -> String {
    if let EnvelopeKind::FxParam { .. } = kind
        && let Some((_, Some(spec))) = fx_target(project, owner, kind)
    {
        return crate::fx_automation_ui::param_text(&spec, spec.from_unit(v));
    }
    value_text(kind, v)
}

/// The name of an envelope kind ("Sends" for a send's).
pub(crate) fn kind_label(kind: EnvelopeKind) -> &'static str {
    match kind {
        EnvelopeKind::Send(_) => tr("Sends"),
        EnvelopeKind::Volume => tr("Volume"),
        EnvelopeKind::Pan => tr("Pan"),
        EnvelopeKind::Mute => tr("Mute"),
        EnvelopeKind::CompositeLevel => tr("Composite Level"),
        EnvelopeKind::FadeToColor => tr("Fade to Color"),
        EnvelopeKind::Velocity => tr("Velocity"),
        EnvelopeKind::TransitionProgress => tr("Transition Progress"),
        EnvelopeKind::MotionBlur => tr("Motion Blur Amount"),
        EnvelopeKind::Supersampling => tr("Video Supersampling"),
        EnvelopeKind::FxParam { .. } | EnvelopeKind::FxBypass(_) => tr("FX Automation"),
    }
}

/// Human-readable value: "−3.0 dB", "25 % L", "Active".
pub(crate) fn value_text(kind: EnvelopeKind, v: f32) -> String {
    match kind {
        EnvelopeKind::Volume | EnvelopeKind::Send(_) if v <= 1e-4 => "−∞ dB".into(),
        EnvelopeKind::Volume | EnvelopeKind::Send(_) => {
            format!("{} dB", crate::fmt::signed(20.0 * v.log10() as f64, 1))
        }
        EnvelopeKind::Pan if v.abs() < 0.005 => tr("Center").into(),
        EnvelopeKind::Pan => format!(
            "{} % {}",
            crate::fmt::num((v.abs() * 100.0) as f64, 0),
            if v < 0.0 {
                trc("pan", "L")
            } else {
                trc("pan", "R")
            }
        ),
        EnvelopeKind::Mute if v >= 0.5 => tr("Active").into(),
        EnvelopeKind::Mute => tr("Muted").into(),
        EnvelopeKind::Velocity if v < -0.005 => {
            format!("−{} %", crate::fmt::num((-v * 100.0).round() as f64, 0))
        }
        EnvelopeKind::CompositeLevel
        | EnvelopeKind::Velocity
        | EnvelopeKind::TransitionProgress => {
            format!("{} %", crate::fmt::num((v * 100.0).round() as f64, 0))
        }
        EnvelopeKind::FadeToColor if v.abs() < 0.005 => tr("No fade").into(),
        EnvelopeKind::FadeToColor if v > 0.0 => trf!(
            "{} % to the top color",
            crate::fmt::num((v * 100.0) as f64, 0)
        ),
        EnvelopeKind::FadeToColor => trf!(
            "{} % to the bottom color",
            crate::fmt::num((-v * 100.0) as f64, 0)
        ),
        EnvelopeKind::MotionBlur => format!("{} s", crate::fmt::num(v as f64, 3)),
        EnvelopeKind::Supersampling => crate::fmt::num(v.round() as f64, 0),
        EnvelopeKind::FxParam { .. } => {
            format!("{} %", crate::fmt::num((v * 100.0).round() as f64, 0))
        }
        EnvelopeKind::FxBypass(_) if v >= 0.5 => tr("On").into(),
        EnvelopeKind::FxBypass(_) => tr("Bypassed").into(),
    }
}

fn lane(row: &Row, l: &Layout) -> Rect {
    row.body(l.lanes.left(), l.lanes.right())
        .shrink2(vec2(0.0, 4.0))
}

/// Where an event's envelopes are drawn: its body, below its title bar.
pub(crate) fn event_lane(view: &TimelineView, l: &Layout, row: &Row, clip: &Clip) -> Rect {
    let r = super::lanes::clip_rect(view, l, row, clip.start, clip.end());
    let title = if view.show_clip_headers {
        super::lanes::TITLE_H
    } else {
        0.0
    };
    Rect::from_min_max(pos2(r.left(), r.top() + title), r.max).shrink2(vec2(0.0, 3.0))
}

/// How far along the event an envelope of its is drawn: a transition
/// progress envelope over the transition, a velocity envelope over the
/// whole event.
fn event_span(project: &Project, clip: &Clip, kind: EnvelopeKind) -> Tick {
    if kind != EnvelopeKind::TransitionProgress {
        return clip.duration;
    }
    project
        .clip_track(clip.id)
        .map_or(clip.duration, |t| {
            t.effective_fades(clip, project.auto_crossfade).0
        })
        .clamp(Tick::ZERO, clip.duration)
}

pub(crate) fn y_of(r: Rect, kind: EnvelopeKind, value: f32) -> f32 {
    r.bottom() - kind.to_unit(value) * r.height()
}

pub(crate) fn value_at_y(r: Rect, kind: EnvelopeKind, y: f32) -> f32 {
    kind.from_unit((r.bottom() - y) / r.height().max(1.0))
}

/// The lane rect where `track`'s envelopes are drawn.
pub(crate) fn lane_of(l: &Layout, track: TrackId) -> Option<Rect> {
    l.row_of(track).map(|row| lane(row, l))
}

/// Where `track`'s envelopes are drawn, or its event `clip`'s.
pub(crate) fn lane_for(
    view: &TimelineView,
    l: &Layout,
    project: &Project,
    track: TrackId,
    clip: Option<ClipId>,
) -> Option<Rect> {
    let row = l.row_of(track)?;
    match clip {
        None => Some(lane(row, l)),
        Some(c) => Some(event_lane(view, l, row, project.clip(c)?)),
    }
}

/// What of a bus track's envelopes is under the pointer: a point of one,
/// or a place on its line (its time and value).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StripHit {
    pub kind: EnvelopeKind,
    pub point: Option<usize>,
    pub at: Tick,
    pub value: f32,
}

/// A bus track's envelope under `pos` in its lane `r`: a point within 6
/// pixels, else the line within 4.
pub(crate) fn strip_hit(
    view: &TimelineView,
    l: &Layout,
    r: Rect,
    envelopes: &[Envelope],
    pos: Pos2,
) -> Option<StripHit> {
    if !r.expand(6.0).contains(pos) {
        return None;
    }
    let mut best: Option<(f32, StripHit)> = None;
    let keep = |best: &mut Option<(f32, StripHit)>, d: f32, h: StripHit| {
        if best.is_none_or(|(bd, _)| d < bd) {
            *best = Some((d, h));
        }
    };
    for env in envelopes.iter().filter(|e| visible(view, e.kind)) {
        for (i, p) in env.points.iter().enumerate() {
            let d = (pos2(view.x_of(l, p.at), y_of(r, env.kind, p.value)) - pos).length();
            if d <= 6.0 {
                keep(
                    &mut best,
                    d,
                    StripHit {
                        kind: env.kind,
                        point: Some(i),
                        at: p.at,
                        value: p.value,
                    },
                );
            }
        }
    }
    if best.is_none() {
        let at = view.time_at(l, pos.x).max(Tick::ZERO);
        for env in envelopes.iter().filter(|e| visible(view, e.kind)) {
            let v = env.value_at(at);
            let d = (y_of(r, env.kind, v) - pos.y).abs();
            if d <= 4.0 {
                keep(
                    &mut best,
                    d + 10.0,
                    StripHit {
                        kind: env.kind,
                        point: None,
                        at,
                        value: v,
                    },
                );
            }
        }
    }
    best.map(|b| b.1)
}

/// The nearer of two hits.
fn nearer(best: &mut Option<(f32, EnvHit)>, d: f32, h: EnvHit) {
    if best.is_none_or(|(bd, _)| d < bd) {
        *best = Some((d, h));
    }
}

/// An envelope of the event under `pos`: a point within 6 pixels, else
/// the line within 4.
fn event_hit(
    view: &TimelineView,
    l: &Layout,
    project: &Project,
    row: &Row,
    pos: Pos2,
) -> Option<EnvHit> {
    let track = project.track(row.track)?;
    let mut best: Option<(f32, EnvHit)> = None;
    for clip in &track.clips {
        let r = event_lane(view, l, row, clip);
        if !r.expand(6.0).contains(pos) {
            continue;
        }
        for env in clip.envelopes.iter().filter(|e| visible(view, e.kind)) {
            let span = event_span(project, clip, env.kind);
            for (i, p) in env.points.iter().enumerate() {
                let at = pos2(view.x_of(l, clip.start + p.at), y_of(r, env.kind, p.value));
                let d = (at - pos).length();
                if d <= 6.0 {
                    nearer(
                        &mut best,
                        d,
                        EnvHit {
                            track: track.id,
                            clip: Some(clip.id),
                            kind: env.kind,
                            point: Some(i),
                            at: clip.start + p.at,
                            value: p.value,
                        },
                    );
                }
            }
            if best.is_some() {
                continue;
            }
            let at = view.time_at(l, pos.x);
            if at < clip.start || at > clip.start + span {
                continue;
            }
            let v = env.value_at(at - clip.start);
            let d = (y_of(r, env.kind, v) - pos.y).abs();
            if d <= 4.0 {
                nearer(
                    &mut best,
                    d + 10.0,
                    EnvHit {
                        track: track.id,
                        clip: Some(clip.id),
                        kind: env.kind,
                        point: None,
                        at,
                        value: v,
                    },
                );
            }
        }
    }
    best.map(|b| b.1)
}

pub(crate) fn hit(view: &TimelineView, l: &Layout, project: &Project, pos: Pos2) -> Option<EnvHit> {
    if !l.lanes.contains(pos) {
        return None;
    }
    let row = l.row_at(pos.y)?;
    // An event's envelopes over the track's.
    if let Some(h) = event_hit(view, l, project, row, pos) {
        return Some(h);
    }
    let track = project.track(row.track)?;
    let r = lane(row, l);
    let mut best: Option<(f32, EnvHit)> = None;
    for env in track.envelopes.iter().filter(|e| visible(view, e.kind)) {
        for (i, p) in env.points.iter().enumerate() {
            let d = (pos2(view.x_of(l, p.at), y_of(r, env.kind, p.value)) - pos).length();
            if d <= 6.0 {
                nearer(
                    &mut best,
                    d,
                    EnvHit {
                        track: track.id,
                        clip: None,
                        kind: env.kind,
                        point: Some(i),
                        at: p.at,
                        value: p.value,
                    },
                );
            }
        }
    }
    if best.is_some() {
        return best.map(|b| b.1);
    }
    let at = view.time_at(l, pos.x).max(Tick::ZERO);
    for env in track.envelopes.iter().filter(|e| visible(view, e.kind)) {
        let v = env.value_at(at);
        let d = (y_of(r, env.kind, v) - pos.y).abs();
        if d <= 4.0 {
            nearer(
                &mut best,
                d,
                EnvHit {
                    track: track.id,
                    clip: None,
                    kind: env.kind,
                    point: None,
                    at,
                    value: v,
                },
            );
        }
    }
    best.map(|b| b.1)
}

/// The envelope's line in `r`: its points `origin` after the timeline's
/// start, the first and last values held out to `from` and `to`.
fn polyline(
    view: &TimelineView,
    l: &Layout,
    r: Rect,
    env: &Envelope,
    origin: Tick,
    (from, to): (f32, f32),
) -> Vec<Pos2> {
    let mut pts = Vec::new();
    let first = env.points[0];
    pts.push(pos2(from, y_of(r, env.kind, first.value)));
    for (i, p) in env.points.iter().enumerate() {
        let x = view.x_of(l, origin + p.at);
        let y = y_of(r, env.kind, p.value);
        if let Some(next) = env.points.get(i + 1) {
            let curve = if env.kind.is_held() {
                KeyCurve::Hold
            } else {
                p.curve
            };
            let x1 = view.x_of(l, origin + next.at);
            if x1 < l.lanes.left() || x > l.lanes.right() {
                pts.push(pos2(x, y));
                continue;
            }
            match curve {
                KeyCurve::Linear => pts.push(pos2(x, y)),
                KeyCurve::Hold => {
                    pts.push(pos2(x, y));
                    pts.push(pos2(x1, y));
                }
                KeyCurve::Smooth
                | KeyCurve::Fast
                | KeyCurve::Slow
                | KeyCurve::Sharp
                | KeyCurve::Bezier
                | KeyCurve::BezierSplit => {
                    for k in 0..16 {
                        let f = k as f32 / 16.0;
                        let v = p.value + (next.value - p.value) * curve.eval(f);
                        pts.push(pos2(x + (x1 - x) * f, y_of(r, env.kind, v)));
                    }
                }
            }
        } else {
            pts.push(pos2(x, y));
            pts.push(pos2(to, y));
        }
    }
    pts
}

/// Where an envelope is drawn: its lane, how far its times are from the
/// timeline's start (an event's start) and the span its line is held out
/// to.
struct Place {
    r: Rect,
    origin: Tick,
    span: (f32, f32),
}

/// An envelope's line and points, as VEGAS draws them (measured on the
/// user's prints): the line two points wide, a little darker either side;
/// the points diamonds of its color edged dark.
fn paint_one(
    p: &egui::Painter,
    view: &TimelineView,
    l: &Layout,
    place: Place,
    env: &Envelope,
    c: Color32,
) {
    let Place { r, origin, span } = place;
    let line = polyline(view, l, r, env, origin, span);
    p.add(Shape::line(
        line.clone(),
        Stroke::new(4.0, Color32::from_black_alpha(64)),
    ));
    p.add(Shape::line(line, Stroke::new(2.0, c)));
    for pt in &env.points {
        let at = pos2(view.x_of(l, origin + pt.at), y_of(r, env.kind, pt.value));
        p.add(Shape::convex_polygon(
            diamond(at, 5.0),
            Color32::from_gray(30),
            Stroke::NONE,
        ));
        p.add(Shape::convex_polygon(diamond(at, 4.0), c, Stroke::NONE));
    }
}

/// A diamond around `at`, `r` from its middle to its corners.
fn diamond(at: Pos2, r: f32) -> Vec<Pos2> {
    vec![
        pos2(at.x, at.y - r),
        pos2(at.x + r, at.y),
        pos2(at.x, at.y + r),
        pos2(at.x - r, at.y),
    ]
}

/// A bus's or a chain's envelopes in its bus track's lane `r`.
pub(crate) fn paint_strip(
    view: &TimelineView,
    painter: &egui::Painter,
    l: &Layout,
    r: Rect,
    envelopes: &[Envelope],
    prefs: &crate::settings::Prefs,
) {
    for env in envelopes.iter().filter(|e| visible(view, e.kind)) {
        let place = Place {
            r,
            origin: Tick::ZERO,
            span: (l.lanes.left(), l.lanes.right()),
        };
        paint_one(painter, view, l, place, env, color(prefs, env.kind));
    }
}

pub(crate) fn paint(
    view: &TimelineView,
    painter: &egui::Painter,
    l: &Layout,
    project: &Project,
    prefs: &crate::settings::Prefs,
) {
    for row in &l.rows {
        let Some(track) = project.track(row.track) else {
            continue;
        };
        let r = lane(row, l);
        if !r.intersects(l.lanes) {
            continue;
        }
        let p = painter.with_clip_rect(r.expand2(vec2(0.0, 4.0)).intersect(l.lanes));
        for env in track.envelopes.iter().filter(|e| visible(view, e.kind)) {
            let place = Place {
                r,
                origin: Tick::ZERO,
                span: (l.lanes.left(), l.lanes.right()),
            };
            paint_one(&p, view, l, place, env, color(prefs, env.kind));
        }
        // What Auto Ducking does to a listener, dotted.
        if track.duck_listener && project.has_ducking() {
            paint_ducking(
                &p,
                view,
                l,
                r,
                project,
                track,
                color(prefs, EnvelopeKind::Volume),
            );
        }
        // The events' own, over each event (a transition's progress over
        // its transition).
        for clip in track.clips.iter().filter(|c| !c.envelopes.is_empty()) {
            let er = event_lane(view, l, row, clip);
            if er.right() < l.lanes.left() || er.left() > l.lanes.right() {
                continue;
            }
            for env in clip.envelopes.iter().filter(|e| visible(view, e.kind)) {
                let end = clip.start + event_span(project, clip, env.kind);
                let area = Rect::from_min_max(er.min, pos2(view.x_of(l, end), er.max.y));
                let p = painter.with_clip_rect(area.expand(4.0).intersect(l.lanes));
                let place = Place {
                    r: er,
                    origin: clip.start,
                    span: (area.left(), area.right()),
                };
                paint_one(&p, view, l, place, env, color(prefs, env.kind));
            }
        }
    }
}

/// Auto Ducking's gain on listener `track` across lane `r`, as a dotted
/// line where its volume envelope would be.
fn paint_ducking(
    p: &egui::Painter,
    view: &TimelineView,
    l: &Layout,
    r: Rect,
    project: &Project,
    track: &bv_timeline::Track,
    c: Color32,
) {
    let (x0, x1) = (r.left().max(l.lanes.left()), r.right().min(l.lanes.right()));
    let mut points = Vec::new();
    let mut x = x0;
    while x <= x1 {
        let g = project.ducking_gain(track, view.time_at(l, x));
        points.push(pos2(x, y_of(r, EnvelopeKind::Volume, g)));
        x += 3.0;
    }
    let stroke = egui::Stroke::new(1.5, c);
    for (i, w) in points.windows(2).enumerate() {
        if i % 2 == 0 {
            p.line_segment([w[0], w[1]], stroke);
        }
    }
}

/// The tooltip of an envelope under the pointer `at`, as VEGAS's: its
/// name, the timeline's time (`time`, as the ruler shows it) and its value
/// there — "Composite Level at 00:01:45:14 is 42 %".
pub(crate) fn paint_value(
    painter: &egui::Painter,
    project: &Project,
    at: Pos2,
    owner: EnvelopeOwner,
    kind: EnvelopeKind,
    time: &str,
    value: f32,
) {
    let text = trf!(
        "{} at {} is {}",
        label(project, owner, kind),
        time,
        value_text_of(project, owner, kind, value)
    );
    paint_tooltip(painter, at, text);
}

/// VEGAS's tooltip under the pointer `at` (measured on the user's print):
/// pale, edged a lighter grey, grey text, a soft shadow to its right and
/// below; kept on the screen.
fn paint_tooltip(painter: &egui::Painter, at: Pos2, text: String) {
    const FILL: Color32 = Color32::from_gray(249);
    const EDGE: Color32 = Color32::from_gray(229);
    const INK: Color32 = Color32::from_gray(87);
    let galley = painter.layout_no_wrap(text, FontId::proportional(12.0), INK);
    let size = vec2((galley.size().x + 14.0).round(), 18.0);
    let screen = painter.clip_rect();
    let mut min = pos2(at.x.round(), at.y.round() + 18.0);
    min.x = min.x.min(screen.right() - size.x - 4.0).max(screen.left());
    if min.y + size.y + 4.0 > screen.bottom() {
        // No room under the pointer: above it.
        min.y = at.y.round() - 6.0 - size.y;
    }
    let r = Rect::from_min_size(min, size);
    // The shadow, darker nearer the box.
    for (d, alpha) in [(4.0, 13), (3.0, 34), (2.0, 46), (1.0, 28)] {
        painter.rect_filled(
            Rect::from_min_max(r.min + vec2(2.0, 2.0), r.max + vec2(d, d)),
            0.0,
            Color32::from_black_alpha(alpha),
        );
    }
    painter.rect(
        r,
        0.0,
        FILL,
        Stroke::new(1.0, EDGE),
        egui::StrokeKind::Inside,
    );
    let y = r.center().y - galley.size().y / 2.0;
    painter.galley(pos2(r.left() + 7.0, y.round()), galley, INK);
}

/// The envelope point under the pointer (or dragged), as VEGAS shows it: a
/// white diamond edged grey and dark.
pub(crate) fn paint_hot_point(painter: &egui::Painter, at: Pos2) {
    for (r, gray) in [(6.0, 30), (5.0, 176), (4.0, 240)] {
        painter.add(Shape::convex_polygon(
            diamond(at, r),
            Color32::from_gray(gray),
            Stroke::NONE,
        ));
    }
}

/// `at` of `owner`'s envelope (an event's from its start) on the timeline.
pub(crate) fn timeline_time(project: &Project, owner: EnvelopeOwner, at: Tick) -> Tick {
    match owner {
        EnvelopeOwner::Event(c) => at + project.clip(c).map_or(Tick::ZERO, |c| c.start),
        EnvelopeOwner::Track(_) | EnvelopeOwner::Strip(_) | EnvelopeOwner::VideoBus => at,
    }
}

/// The segment of `owner`'s `kind` envelope at `at` (its own time), what
/// dragging its line moves, as VEGAS's: the points before and after it
/// (a Hold point's line its own alone; before the first point, the
/// first), each with its time and value.
pub(crate) fn segment(
    project: &Project,
    owner: EnvelopeOwner,
    kind: EnvelopeKind,
    at: Tick,
) -> Vec<(usize, Tick, f32)> {
    let Some(e) = envelope_of(project, owner, kind) else {
        return Vec::new();
    };
    let i = e.points.iter().rposition(|p| p.at <= at).unwrap_or(0);
    let mut picked = vec![i];
    if let Some(p) = e.points.get(i)
        && p.at <= at
        && p.curve != KeyCurve::Hold
        && i + 1 < e.points.len()
    {
        picked.push(i + 1);
    }
    picked
        .into_iter()
        .filter_map(|i| e.points.get(i).map(|p| (i, p.at, p.value)))
        .collect()
}

/// Set To…: a point's value being typed in a box at the point.
#[derive(Clone, Debug)]
pub(crate) struct SetTo {
    pub track: TrackId,
    /// An event's envelope (`None`: the track's).
    pub clip: Option<ClipId>,
    pub kind: EnvelopeKind,
    pub index: usize,
    pub text: String,
    /// Just opened: the box takes the keys, its text picked whole.
    pub fresh: bool,
}

/// A point's value as Set To… shows it to be typed over ("21 %",
/// "−6.0 dB", "25 % L"; a fade to color's signed).
pub(crate) fn set_to_text(kind: EnvelopeKind, v: f32) -> String {
    match kind {
        EnvelopeKind::FadeToColor => format!("{} %", crate::fmt::num((v * 100.0) as f64, 0)),
        _ => value_text(kind, v),
    }
}

/// A value typed for `kind` (Set To…): a percentage ("21 %", "21"), dB
/// ("−6", "-inf"), a pan ("25 L", "center"), seconds or a count; within
/// the envelope's range. `None` when it is not one.
pub(crate) fn parse_value(kind: EnvelopeKind, text: &str) -> Option<f32> {
    // The minus sign the values are shown with, or a hyphen.
    let text = &text.replace('\u{2212}', "-");
    let number = |s: &str| crate::fmt::parse(s.trim().trim_end_matches('%').trim());
    let v = match kind {
        EnvelopeKind::Volume | EnvelopeKind::Send(_) => super::headers::parse_volume(text)?,
        EnvelopeKind::Pan => super::headers::parse_pan(text)?,
        EnvelopeKind::CompositeLevel
        | EnvelopeKind::FadeToColor
        | EnvelopeKind::Velocity
        | EnvelopeKind::TransitionProgress
        | EnvelopeKind::FxParam { .. } => number(text)? as f32 / 100.0,
        EnvelopeKind::MotionBlur => number(text.trim().trim_end_matches('s'))? as f32,
        EnvelopeKind::Supersampling => number(text)?.round() as f32,
        EnvelopeKind::Mute | EnvelopeKind::FxBypass(_) => return None,
    };
    Some(kind.from_unit(kind.to_unit(v)))
}

/// The Set To… box over its point, as VEGAS's: white, the value picked;
/// Enter or a click elsewhere sets what was typed, Escape leaves it.
pub(crate) fn set_to_ui(
    view: &mut TimelineView,
    ui: &egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
    project: &Project,
) {
    let Some((track, clip, kind, index)) = view
        .env_set_to
        .as_ref()
        .map(|s| (s.track, s.clip, s.kind, s.index))
    else {
        return;
    };
    let owner = owner(track, clip);
    let point = envelope_of(project, owner, kind).and_then(|e| e.points.get(index).copied());
    let (Some(point), Some(r)) = (point, lane_for(view, l, project, track, clip)) else {
        view.env_set_to = None;
        return;
    };
    let start = match owner {
        EnvelopeOwner::Event(c) => project.clip(c).map_or(Tick::ZERO, |c| c.start),
        _ => Tick::ZERO,
    };
    let at = pos2(
        view.x_of(l, point.at + start) + 2.0,
        y_of(r, kind, point.value) - 8.0,
    );
    let Some(s) = view.env_set_to.as_mut() else {
        return;
    };
    let id = egui::Id::new("envelope-set-to");
    let (mut commit, mut cancel) = (false, false);
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .show(ui.ctx(), |ui| {
            // VEGAS's box: the text picked in blue, white over it.
            let v = ui.visuals_mut();
            v.selection.bg_fill = Color32::from_rgb(0x00, 0x78, 0xD7);
            v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
            v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_gray(0x80));
            let field = egui::TextEdit::singleline(&mut s.text)
                .id(id.with("field"))
                .desired_width(56.0)
                .font(FontId::proportional(12.0))
                .text_color(Color32::BLACK)
                .background_color(Color32::WHITE)
                .margin(vec2(3.0, 1.0));
            let resp = ui.add(field);
            if std::mem::take(&mut s.fresh) {
                resp.request_focus();
                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id.with("field")) {
                    let all = egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(s.text.chars().count()),
                    );
                    state.cursor.set_char_range(Some(all));
                    state.store(ui.ctx(), id.with("field"));
                }
            } else if ui.input(|i| i.key_pressed(Key::Escape)) {
                cancel = true;
            } else if resp.lost_focus() {
                commit = true;
            }
        });
    if commit && let Some(v) = parse_value(kind, &s.text) {
        app.edit(|p| p.move_envelope_point(owner, kind, index, point.at, v));
    }
    if commit || cancel {
        view.env_set_to = None;
    }
}

impl BananaApp {
    /// An adjustment track on top, its Composite envelope as the user's
    /// VEGAS print shows it: 100 % up to 2 s, a smooth fade down to 21 % at
    /// 5 s (screen tests).
    pub fn adjustment_envelope_for_review(&mut self) {
        self.insert_adjustment_track(None);
        let Some(t) = self.project().tracks.first().map(|t| t.id) else {
            return;
        };
        let (owner, kind) = (EnvelopeOwner::Track(t), EnvelopeKind::CompositeLevel);
        let s = Tick::from_seconds;
        self.modify(|p| {
            let _ = p.set_envelope_point(owner, kind, s(2), 1.0);
            let _ = p.set_envelope_point(owner, kind, s(5), 0.21);
            let _ = p.set_envelope_curve(owner, kind, 1, KeyCurve::Smooth);
        });
    }

    /// Two audio tracks more, all the audio tracks picked and V pressed
    /// (their volume envelopes, as the user's VEGAS print shows them), a
    /// point at 3 s on the first (screen tests).
    pub fn volume_envelopes_for_review(&mut self) {
        self.insert_track(bv_timeline::TrackKind::Audio);
        self.insert_track(bv_timeline::TrackKind::Audio);
        let audio: Vec<TrackId> = self
            .project()
            .tracks
            .iter()
            .filter(|t| t.kind == bv_timeline::TrackKind::Audio)
            .map(|t| t.id)
            .collect();
        for (i, &t) in audio.iter().enumerate() {
            self.pick_track(t, false, i > 0);
        }
        crate::commands::Cmd::ShowEnvelopeType(EnvelopeKind::Volume).run(self);
        if let Some(&first) = audio.first() {
            self.modify(|p| {
                let owner = EnvelopeOwner::Track(first);
                let _ =
                    p.set_envelope_point(owner, EnvelopeKind::Volume, Tick::from_seconds(3), 1.0);
            });
        }
    }

    /// The adjustment track on top with an event of its own (the
    /// checkerboard from half a second, at half its level) and Invert,
    /// which works on it and on the tracks below (screen tests).
    pub fn adjustment_video_for_review(&mut self) {
        let Some(t) = self
            .project()
            .tracks
            .first()
            .filter(|t| t.adjustment)
            .map(|t| t.id)
        else {
            return;
        };
        let Some(checker) = self.project().media.get(2).map(|m| m.id) else {
            return;
        };
        self.place_media(checker, Tick::from_seconds_f64(0.5), Some(t));
        self.modify(|p| {
            if let Some(t) = p.track_by_id_mut(t) {
                t.effects
                    .push(bv_timeline::Effect::new(bv_timeline::EffectKind::Invert));
                if let Some(c) = t.clips.first_mut() {
                    c.level = 0.5;
                }
            }
        });
    }

    /// The first track's Composite envelope's points' values (screen
    /// tests).
    pub fn envelope_values_for_review(&self) -> Vec<f32> {
        let project = self.project();
        project
            .tracks
            .first()
            .and_then(|t| {
                envelope_of(
                    project,
                    EnvelopeOwner::Track(t.id),
                    EnvelopeKind::CompositeLevel,
                )
            })
            .map_or_else(Vec::new, |e| e.points.iter().map(|p| p.value).collect())
    }

    /// Where the first track's Composite envelope is at `seconds` (its
    /// point `index`'s place when given), the timeline as last drawn
    /// (screen tests).
    pub fn envelope_spot_for_review(&self, seconds: f64, index: Option<usize>) -> Option<Pos2> {
        let view = &self.timeline;
        let project = self.project();
        let l = view.layout(view.last_rect, project);
        let t = project.tracks.first()?.id;
        let kind = EnvelopeKind::CompositeLevel;
        let e = envelope_of(project, EnvelopeOwner::Track(t), kind)?;
        let (at, value) = match index {
            Some(i) => e.points.get(i).map(|p| (p.at, p.value))?,
            None => {
                let at = Tick::from_seconds_f64(seconds);
                (at, e.value_at(at))
            }
        };
        let r = lane_of(&l, t)?;
        Some(pos2(view.x_of(&l, at), y_of(r, kind, value)))
    }

    /// A video effect (BananaV's Brightness and Contrast) being dragged
    /// from the Video FX window (screen tests).
    pub fn effect_drag_for_review(&mut self) {
        self.media_drag = Some(crate::app::MediaDrag::Effect(bv_timeline::Effect::plugin(
            bv_timeline::PluginFx {
                id: "org.bananav.BrightnessContrast".into(),
                version: [1, 0],
                name: "Brightness and Contrast".into(),
                params: Default::default(),
            },
        )));
    }

    /// How many effects the first track has, and whether its Video Track
    /// FX window is open (screen tests).
    pub fn first_track_effects_for_review(&self) -> (usize, bool) {
        let t = self.project().tracks.first();
        (
            t.map_or(0, |t| t.effects.len()),
            t.is_some_and(|t| {
                self.clip_windows.track_fx == Some(t.id)
                    || self.clip_windows.fx_target_shown()
                        == Some(crate::fx_window::FxTarget::Track(t.id))
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Set To… reads what it shows and what is typed: percentages, dB,
    /// pans; nothing for a mute's.
    #[test]
    fn typed_values_are_read() {
        // The values as shown, to their last digit (−6.0 dB is 0.501).
        let near = |a: Option<f32>, b: f32| a.is_some_and(|a| (a - b).abs() < 5e-3);
        let c = EnvelopeKind::CompositeLevel;
        assert!(near(parse_value(c, &set_to_text(c, 0.21)), 0.21));
        assert!(near(parse_value(c, "50"), 0.5));
        assert!(near(parse_value(c, "150 %"), 1.0));
        let v = EnvelopeKind::Volume;
        assert!(near(parse_value(v, &set_to_text(v, 0.5)), 0.5));
        assert!(near(parse_value(v, "-inf"), 0.0));
        assert!(near(parse_value(EnvelopeKind::Pan, "25 L"), -0.25));
        let f = EnvelopeKind::FadeToColor;
        assert!(near(parse_value(f, &set_to_text(f, -0.4)), -0.4));
        assert_eq!(parse_value(EnvelopeKind::Mute, "1"), None);
        assert_eq!(parse_value(c, "abc"), None);
    }

    /// A line's segment: the points around it; a Hold point's own; before
    /// the first point, the first.
    #[test]
    fn a_line_drags_the_points_around_it() {
        let mut p = Project::default();
        let t = p.tracks[0].id;
        let owner = EnvelopeOwner::Track(t);
        let kind = EnvelopeKind::CompositeLevel;
        p.toggle_envelope(t, kind).unwrap();
        let s = Tick::from_seconds;
        p.set_envelope_point(owner, kind, s(2), 0.8).unwrap();
        p.set_envelope_point(owner, kind, s(4), 0.2).unwrap();
        let idx = |p: &Project, at| -> Vec<usize> {
            segment(p, owner, kind, at).iter().map(|x| x.0).collect()
        };
        assert_eq!(idx(&p, s(1)), [0, 1]);
        assert_eq!(idx(&p, s(3)), [1, 2]);
        assert_eq!(idx(&p, s(5)), [2]);
        p.set_envelope_curve(owner, kind, 1, KeyCurve::Hold)
            .unwrap();
        assert_eq!(idx(&p, s(3)), [1]);
    }

    #[test]
    fn value_texts() {
        assert_eq!(value_text(EnvelopeKind::Volume, 1.0), "+0.0 dB");
        assert_eq!(value_text(EnvelopeKind::Pan, -0.25), "25 % L");
        assert_eq!(value_text(EnvelopeKind::Mute, 0.0), "Muted");
        assert_eq!(value_text(EnvelopeKind::Velocity, -1.0), "−100 %");
        assert_eq!(value_text(EnvelopeKind::Velocity, 2.5), "250 %");
    }
}
