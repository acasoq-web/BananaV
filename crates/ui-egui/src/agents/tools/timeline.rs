//! The timeline's tools: tracks, events, markers and regions, envelopes,
//! the cursor, the selection, the loop region and playback.

use bv_core::{Tick, TimeRange};
use bv_timeline::{
    ClipId, EnvelopeKind, EnvelopeOwner, FadeCurve, KeyCurve, MarkerId, Project, TrackKind,
};
use serde_json::{Value, json};

use super::{
    Args, Kind, Tool, arg, choice, clip_of, event_json, media_of, secs, track_at, track_json,
};
use crate::agents::{Outcome, Output};
use crate::app::BananaApp;
use crate::i18n::tr;

fn done(v: Value) -> Outcome {
    Ok(Output::value(v))
}

/// `t` on the project's frames (where events and edits land).
fn on_frames(p: &Project, t: Tick) -> Tick {
    p.settings.frame_rate.snap_nearest(t)
}

/// The track kind named `name`.
fn track_kind(name: &str) -> TrackKind {
    if name == "audio" {
        TrackKind::Audio
    } else {
        TrackKind::Video
    }
}

/// Where event `id` is: its track's index and the event.
fn find_event(p: &Project, id: u64) -> Result<(usize, &bv_timeline::Clip), String> {
    let c = clip_of(p, id)?;
    let (t, _) = p
        .find_clip(c.id)
        .ok_or_else(|| trf!("There is no event {}.", id))?;
    Ok((t, c))
}

fn event_out(app: &BananaApp, id: ClipId) -> Value {
    let p = app.project();
    match find_event(p, id.0) {
        Ok((t, c)) => event_json(p, t, c),
        Err(_) => Value::Null,
    }
}

const CURVES: [&str; 5] = ["linear", "fast", "slow", "smooth", "sharp"];

fn fade_curve(name: &str) -> FadeCurve {
    match name {
        "linear" => FadeCurve::Linear,
        "fast" => FadeCurve::Fast,
        "slow" => FadeCurve::Slow,
        "sharp" => FadeCurve::Sharp,
        _ => FadeCurve::Smooth,
    }
}

const KEY_CURVES: [&str; 6] = ["linear", "fast", "slow", "smooth", "sharp", "hold"];

pub(super) fn key_curve(name: &str) -> KeyCurve {
    match name {
        "linear" => KeyCurve::Linear,
        "fast" => KeyCurve::Fast,
        "slow" => KeyCurve::Slow,
        "sharp" => KeyCurve::Sharp,
        "hold" => KeyCurve::Hold,
        _ => KeyCurve::Smooth,
    }
}

pub(super) fn key_curve_name(c: KeyCurve) -> &'static str {
    match c {
        KeyCurve::Linear => "linear",
        KeyCurve::Fast => "fast",
        KeyCurve::Slow => "slow",
        KeyCurve::Sharp => "sharp",
        KeyCurve::Hold => "hold",
        _ => "smooth",
    }
}

/// The envelopes an agent can set, by name.
const ENVELOPES: [&str; 7] = [
    "volume",
    "pan",
    "mute",
    "composite_level",
    "fade_to_color",
    "velocity",
    "transition_progress",
];

fn envelope_kind(name: &str) -> Option<EnvelopeKind> {
    Some(match name {
        "volume" => EnvelopeKind::Volume,
        "pan" => EnvelopeKind::Pan,
        "mute" => EnvelopeKind::Mute,
        "composite_level" => EnvelopeKind::CompositeLevel,
        "fade_to_color" => EnvelopeKind::FadeToColor,
        "velocity" => EnvelopeKind::Velocity,
        "transition_progress" => EnvelopeKind::TransitionProgress,
        _ => return None,
    })
}

fn envelope_name(k: &EnvelopeKind) -> String {
    match k {
        EnvelopeKind::Volume => "volume".into(),
        EnvelopeKind::Pan => "pan".into(),
        EnvelopeKind::Mute => "mute".into(),
        EnvelopeKind::CompositeLevel => "composite_level".into(),
        EnvelopeKind::FadeToColor => "fade_to_color".into(),
        EnvelopeKind::Velocity => "velocity".into(),
        EnvelopeKind::TransitionProgress => "transition_progress".into(),
        other => format!("{other:?}").to_lowercase(),
    }
}

fn envelopes_json(list: &[bv_timeline::Envelope], from: Tick) -> Value {
    Value::Array(
        list.iter()
            .map(|e| {
                json!({
                    "kind": envelope_name(&e.kind),
                    "points": e.points.iter().map(|pt| json!({
                        "at": secs(pt.at + from),
                        "value": pt.value,
                        "curve": key_curve_name(pt.curve),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

fn list_tracks(app: &mut BananaApp, _: &Args<'_>) -> Outcome {
    let p = app.project();
    done(json!({
        "tracks": p.tracks.iter().enumerate().map(|(i, t)| track_json(i, t)).collect::<Vec<_>>(),
    }))
}

fn add_track(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let kind = track_kind(a.opt_str("kind")?.unwrap_or("video"));
    let index = a.opt_u64("index")?;
    let name = a.opt_str("name")?.map(str::to_string);
    let count = app.project().tracks.len();
    if index.is_some_and(|i| i as usize > count) {
        return Err(trf!(
            "The index {} is past the tracks (the project has {}).",
            index.unwrap_or(0),
            count
        ));
    }
    let id = app.agent_modify(|p| {
        let id = match index {
            Some(i) => p.insert_track_at(kind, i as usize),
            None => p.add_track(kind),
        };
        if let (Some(n), Some(t)) = (name, p.track_by_id_mut(id)) {
            t.name = n;
        }
        id
    });
    app.selected_track = Some(id);
    let p = app.project();
    let i = p.track_index(id).unwrap_or(0);
    done(track_json(i, &p.tracks[i]))
}

fn delete_track(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let id = track_at(app.project(), a.u64("track")?)?.id;
    app.agent_edit(|p| p.remove_track(id))?;
    app.prune_selection();
    done(json!({"deleted": true, "tracks": app.project().tracks.len()}))
}

fn set_track(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let id = track_at(app.project(), a.u64("track")?)?.id;
    let name = a.opt_str("name")?.map(str::to_string);
    let muted = a.opt_bool("muted")?;
    let solo = a.opt_bool("solo")?;
    let level = a.opt_f64("level")?;
    let pan = a.opt_f64("pan")?;
    let to = a.opt_u64("move_to")?;
    if let Some(i) = to
        && i as usize >= app.project().tracks.len()
    {
        return Err(trf!(
            "There is no track {} (the project has {}).",
            i,
            app.project().tracks.len()
        ));
    }
    app.agent_edit(|p| -> Result<(), bv_timeline::EditError> {
        if let Some(m) = muted {
            p.set_track_muted(id, m)?;
        }
        if let Some(s) = solo {
            p.set_track_solo(id, s)?;
        }
        if let Some(t) = p.track_by_id_mut(id) {
            if let Some(n) = name {
                t.name = n;
            }
            if let Some(l) = level {
                let top = if t.kind == TrackKind::Audio {
                    bv_timeline::envelope::MAX_VOLUME
                } else {
                    1.0
                };
                t.level = (l as f32).clamp(0.0, top);
            }
            if let Some(x) = pan {
                t.pan = (x as f32).clamp(-1.0, 1.0);
            }
        }
        if let Some(i) = to {
            p.move_track_to(id, i as usize)?;
        }
        Ok(())
    })?;
    let p = app.project();
    let i = p.track_index(id).unwrap_or(0);
    done(track_json(i, &p.tracks[i]))
}

fn list_events(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let p = app.project();
    let only = a.opt_u64("track")?;
    if let Some(t) = only {
        track_at(p, t)?;
    }
    let from = a.opt_time("from")?;
    let to = a.opt_time("to")?;
    let mut out = Vec::new();
    for (i, t) in p.tracks.iter().enumerate() {
        if only.is_some_and(|o| o as usize != i) {
            continue;
        }
        for c in &t.clips {
            if from.is_some_and(|f| c.end() <= f) || to.is_some_and(|e| c.start >= e) {
                continue;
            }
            out.push(event_json(p, i, c));
        }
    }
    done(json!({"events": out}))
}

fn add_event(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let media = media_of(app.project(), a.u64("media")?)?.id;
    let track = match a.opt_u64("track")? {
        Some(t) => Some(track_at(app.project(), t)?.id),
        None => None,
    };
    let at = match a.opt_time("at")? {
        Some(t) => on_frames(app.project(), t),
        None => app.cursor,
    };
    let range = match (a.opt_time("media_offset")?, a.opt_time("length")?) {
        (None, None) => None,
        (from, length) => {
            let m = media_of(app.project(), media.0)?;
            let from = from.unwrap_or(Tick::ZERO);
            let length = length.unwrap_or_else(|| m.duration.max(from) - from);
            Some(TimeRange::new(from, length))
        }
    };
    let (video, audio) = (a.bool_or("video", true)?, a.bool_or("audio", true)?);
    let mark = std::time::Instant::now();
    let before = app.history.revision();
    app.place_media_streams(media, at, range, track, video, audio);
    if let Some(e) = app.status_error_since(mark) {
        return Err(e);
    }
    if app.history.revision() == before {
        return Err(tr(
            "Nothing was placed: the media has no picture or sound for the tracks asked.",
        )
        .into());
    }
    let ids: Vec<ClipId> = app.selection.iter().copied().collect();
    done(json!({"events": ids.iter().map(|&id| event_out(app, id)).collect::<Vec<_>>()}))
}

fn move_event(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let (_, c) = find_event(app.project(), a.u64("event")?)?;
    let (id, start) = (c.id, c.start);
    let to = match a.opt_time("start")? {
        Some(t) => on_frames(app.project(), t),
        None => start,
    };
    let track = match a.opt_u64("track")? {
        Some(t) => Some(track_at(app.project(), t)?.id),
        None => None,
    };
    app.agent_edit(|p| p.move_clip(id, to, track))?;
    done(event_out(app, id))
}

fn trim_event(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let (_, c) = find_event(app.project(), a.u64("event")?)?;
    let id = c.id;
    let start = a.opt_time("start")?.map(|t| on_frames(app.project(), t));
    let end = a.opt_time("end")?.map(|t| on_frames(app.project(), t));
    if start.is_none() && end.is_none() {
        return Err(tr("Give \"start\", \"end\" or both.").into());
    }
    if let (Some(s), Some(e)) = (start, end)
        && e <= s
    {
        return Err(tr("The end must come after the start.").into());
    }
    app.agent_edit(|p| -> Result<(), bv_timeline::EditError> {
        if let Some(s) = start {
            p.trim_start(id, s)?;
        }
        if let Some(e) = end {
            p.trim_end(id, e)?;
        }
        Ok(())
    })?;
    done(event_out(app, id))
}

fn split_event(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let at = on_frames(app.project(), a.time("at")?);
    let made = match a.opt_u64("event")? {
        Some(e) => {
            let id = clip_of(app.project(), e)?.id;
            app.agent_edit(|p| p.split(id, at))?
        }
        None => app.agent_modify(|p| p.split_all_at(at)),
    };
    if made.is_empty() {
        return Err(tr("No event goes on past that time.").into());
    }
    done(json!({"new_events": made.iter().map(|&id| event_out(app, id)).collect::<Vec<_>>()}))
}

fn delete_events(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let mut ids = Vec::new();
    for e in a.ids("events")? {
        ids.push(clip_of(app.project(), e)?.id);
    }
    let ripple = a.bool_or("ripple", false)?;
    let grouped = app.settings.prefs.group_edits;
    app.agent_edit(|p| p.delete_events(&ids, ripple, grouped))?;
    app.prune_selection();
    done(json!({"deleted": ids.len()}))
}

fn set_event(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let (_, c) = find_event(app.project(), a.u64("event")?)?;
    let id = c.id;
    let take = c.active_take();
    let level = a.opt_f64("level")?;
    let muted = a.opt_bool("muted")?;
    let locked = a.opt_bool("locked")?;
    let rate = a.opt_f64("rate")?;
    let fade_in = a.opt_time("fade_in")?;
    let fade_out = a.opt_time("fade_out")?;
    let in_curve = a.opt_str("fade_in_curve")?.map(fade_curve);
    let out_curve = a.opt_str("fade_out_curve")?.map(fade_curve);
    let length = a.opt_time("length")?.map(|t| on_frames(app.project(), t));
    let offset = a.opt_time("media_offset")?;
    let name = a.opt_str("name")?.map(str::to_string);
    if rate.is_some_and(|r| r <= 0.0) {
        return Err(tr("The rate must be more than 0.").into());
    }
    app.agent_edit(|p| -> Result<(), bv_timeline::EditError> {
        if let Some(r) = rate {
            p.set_clip_rate(id, r)?;
        }
        if let Some(l) = length {
            p.set_clip_duration(id, l)?;
        }
        if fade_in.is_some() || fade_out.is_some() {
            p.set_fades(id, fade_in, fade_out)?;
        }
        if let Some(c) = in_curve {
            p.set_fade_curve(id, false, c)?;
        }
        if let Some(c) = out_curve {
            p.set_fade_curve(id, true, c)?;
        }
        if let Some(l) = level {
            p.set_clip_level(id, l as f32)?;
        }
        if let Some(n) = &name {
            p.rename_take(id, take, n)?;
        }
        if let Some(c) = p.clip_mut(id) {
            if let Some(m) = muted {
                c.muted = m;
            }
            if let Some(l) = locked {
                c.locked = l;
            }
            if let Some(o) = offset {
                c.source_in = o;
            }
        }
        Ok(())
    })?;
    done(event_out(app, id))
}

fn group_events(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let mut ids = Vec::new();
    for e in a.ids("events")? {
        ids.push(clip_of(app.project(), e)?.id);
    }
    let together = a.bool_or("group", true)?;
    if together && ids.len() < 2 {
        return Err(tr("Grouping needs two events or more.").into());
    }
    app.agent_modify(|p| {
        if together {
            p.group_clips(&ids);
        } else {
            p.ungroup_clips(&ids);
        }
    });
    done(json!({"events": ids.iter().map(|&id| event_out(app, id)).collect::<Vec<_>>()}))
}

fn list_markers(app: &mut BananaApp, _: &Args<'_>) -> Outcome {
    let p = app.project();
    done(json!({
        "markers": p.markers.iter().map(|m| json!({"id": m.id.0, "at": secs(m.time), "name": m.name})).collect::<Vec<_>>(),
        "regions": p.regions.iter().map(|r| json!({
            "id": r.id.0,
            "start": secs(r.range.start),
            "end": secs(r.range.end()),
            "name": r.name,
        })).collect::<Vec<_>>(),
    }))
}

fn add_marker(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let at = on_frames(app.project(), a.time("at")?);
    let name = a.opt_str("name")?.map(str::to_string);
    let id = app.agent_modify(|p| {
        let id = p.add_marker(at);
        if let Some(n) = name {
            p.rename_marker(id, n);
        }
        id
    });
    done(json!({"id": id.0, "at": secs(at)}))
}

fn add_region(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let start = on_frames(app.project(), a.time("start")?);
    let end = on_frames(app.project(), a.time("end")?);
    if end <= start {
        return Err(tr("The end must come after the start.").into());
    }
    let name = a.opt_str("name")?.map(str::to_string);
    let id = app.agent_modify(|p| {
        let id = p.add_region(TimeRange::from_bounds(start, end));
        if let Some(n) = name {
            p.rename_marker(id, n);
        }
        id
    });
    done(json!({"id": id.0, "start": secs(start), "end": secs(end)}))
}

fn delete_marker(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let id = MarkerId(a.u64("marker")?);
    let p = app.project();
    if !p.markers.iter().any(|m| m.id == id) && !p.regions.iter().any(|r| r.id == id) {
        return Err(trf!("There is no marker or region {}.", id.0));
    }
    app.agent_modify(|p| p.remove_marker(id));
    done(json!({"deleted": true}))
}

/// The owner a tool's `track` or `event` argument names.
fn owner(app: &BananaApp, a: &Args<'_>) -> Result<(EnvelopeOwner, Tick), String> {
    let p = app.project();
    match (a.opt_u64("track")?, a.opt_u64("event")?) {
        (Some(t), None) => Ok((EnvelopeOwner::Track(track_at(p, t)?.id), Tick::ZERO)),
        (None, Some(e)) => {
            let c = clip_of(p, e)?;
            Ok((EnvelopeOwner::Event(c.id), c.start))
        }
        _ => Err(tr("Give \"track\" or \"event\", one of them.").into()),
    }
}

fn list_envelopes(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let (owner, from) = owner(app, a)?;
    let p = app.project();
    let list = match owner {
        EnvelopeOwner::Track(t) => p.track(t).map(|t| t.envelopes.as_slice()),
        EnvelopeOwner::Event(c) => p.clip(c).map(|c| c.envelopes.as_slice()),
        _ => None,
    };
    done(json!({"envelopes": envelopes_json(list.unwrap_or_default(), from)}))
}

fn set_envelope(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let (owner, from) = owner(app, a)?;
    let name = a.str("kind")?;
    let kind = envelope_kind(name).ok_or_else(|| trf!("Unknown envelope: {}", name))?;
    let mut points = Vec::new();
    if let Some(list) = a.get("points") {
        let Some(list) = list.as_array() else {
            return Err(tr("\"points\" is a list of {\"at\": seconds, \"value\": number}.").into());
        };
        for pt in list {
            let at = pt.get("at").and_then(Value::as_f64).filter(|s| *s >= 0.0);
            let value = pt.get("value").and_then(Value::as_f64);
            let (Some(at), Some(value)) = (at, value) else {
                return Err(
                    tr("\"points\" is a list of {\"at\": seconds, \"value\": number}.").into(),
                );
            };
            let curve = pt.get("curve").and_then(Value::as_str).map(key_curve);
            points.push((Tick::from_seconds_f64(at), value as f32, curve));
        }
    }
    let clear = a.bool_or("clear", false)?;
    let remove = a.bool_or("remove", false)?;
    let has = |p: &Project| match owner {
        EnvelopeOwner::Track(t) => p
            .track(t)
            .is_some_and(|t| t.envelopes.iter().any(|e| e.kind == kind)),
        EnvelopeOwner::Event(c) => p
            .clip(c)
            .is_some_and(|c| c.envelopes.iter().any(|e| e.kind == kind)),
        _ => false,
    };
    app.agent_edit(|p| -> Result<(), bv_timeline::EditError> {
        let toggle = |p: &mut Project| match owner {
            EnvelopeOwner::Event(c) => p.toggle_event_envelope(c, kind),
            EnvelopeOwner::Track(t) => p.toggle_envelope(t, kind),
            _ => Ok(false),
        };
        if has(p) && (clear || remove) {
            toggle(p)?;
        }
        if remove {
            return Ok(());
        }
        if !has(p) {
            toggle(p)?;
        }
        for (at, value, curve) in &points {
            // An event's points count from its start.
            let at = (*at - from).max(Tick::ZERO);
            let i = p.set_envelope_point(owner, kind, at, *value)?;
            if let Some(c) = curve {
                p.set_envelope_curve(owner, kind, i, *c)?;
            }
        }
        Ok(())
    })?;
    list_envelopes(app, a)
}

fn set_cursor(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let at = a.time("at")?;
    app.seek(at);
    done(json!({"cursor": secs(app.cursor)}))
}

fn select_events(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let add = a.bool_or("add", false)?;
    let mut ids = Vec::new();
    for e in a.ids("events")? {
        ids.push(clip_of(app.project(), e)?.id);
    }
    if !add {
        app.selection.clear();
    }
    for id in ids {
        app.select_clip(id, true);
    }
    done(json!({"selection": app.selection.iter().map(|c| c.0).collect::<Vec<_>>()}))
}

fn set_loop_region(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    if a.bool_or("clear", false)? {
        app.set_loop_region(None);
        return done(json!({"loop_region": null}));
    }
    let start = on_frames(app.project(), a.time("start")?);
    let end = on_frames(app.project(), a.time("end")?);
    if end <= start {
        return Err(tr("The end must come after the start.").into());
    }
    app.set_loop_region(Some(TimeRange::from_bounds(start, end)));
    done(json!({"loop_region": {"start": secs(start), "end": secs(end)}}))
}

fn play(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let from = a.opt_time("from")?.unwrap_or(app.cursor);
    app.start_playback(from);
    done(json!({"playing": true, "from": secs(from)}))
}

fn stop(app: &mut BananaApp, _: &Args<'_>) -> Outcome {
    app.stop();
    done(json!({"playing": false, "cursor": secs(app.cursor)}))
}

pub(super) fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "list_tracks",
            title: "List tracks",
            description: "The project's tracks from the top (index 0): kind, name, mute, solo, level, pan, composite mode, how many events, their effects.",
            properties: || json!({}),
            required: &[],
            kind: Kind::Read,
            run: list_tracks,
        },
        Tool {
            name: "add_track",
            title: "Add a track",
            description: "Adds a video or audio track: at `index` (0 = top), else a video track below the other video tracks and an audio track at the end.",
            properties: || {
                json!({
                    "kind": choice(&["video", "audio"], "The track's kind (video if not given)."),
                    "index": arg("integer", "Where it goes, 0 at the top."),
                    "name": arg("string", "Its name."),
                })
            },
            required: &[],
            kind: Kind::Edit,
            run: add_track,
        },
        Tool {
            name: "delete_track",
            title: "Delete a track",
            description: "Deletes a track and its events.",
            properties: || json!({"track": arg("integer", "The track's index (0 = top).")}),
            required: &["track"],
            kind: Kind::Remove,
            run: delete_track,
        },
        Tool {
            name: "set_track",
            title: "Change a track",
            description: "Changes a track's name, mute, solo, level (video: opacity 0-1; audio: gain, 1 = 0 dB, up to 3.98 = +12 dB) or pan (-1 left to 1 right), or moves it to another index.",
            properties: || {
                json!({
                    "track": arg("integer", "The track's index (0 = top)."),
                    "name": arg("string", "Its new name."),
                    "muted": arg("boolean", "Muted or not."),
                    "solo": arg("boolean", "Solo or not."),
                    "level": arg("number", "Video: opacity 0-1. Audio: linear gain, 1 = 0 dB."),
                    "pan": arg("number", "-1 (left) to 1 (right)."),
                    "move_to": arg("integer", "The index it moves to."),
                })
            },
            required: &["track"],
            kind: Kind::Edit,
            run: set_track,
        },
        Tool {
            name: "list_events",
            title: "List events",
            description: "The events (clips) on the timeline, or on one track, or between two times: id, track, media, start, end, length, media offset, rate, level, fades, effects.",
            properties: || {
                json!({
                    "track": arg("integer", "Only this track's (index)."),
                    "from": arg("number", "Only those ending after this time (seconds)."),
                    "to": arg("number", "Only those starting before this time (seconds)."),
                })
            },
            required: &[],
            kind: Kind::Read,
            run: list_events,
        },
        Tool {
            name: "add_event",
            title: "Place media on the timeline",
            description: "Places a media (from import_media or list_media) on the timeline as events: its picture on a video track and its sound on an audio track (grouped), at `at` (the cursor if not given). Without `track`, the first track of each kind (made when there is none). `media_offset` and `length` place a part of it.",
            properties: || {
                json!({
                    "media": arg("integer", "The media's id."),
                    "at": arg("number", "Where it starts on the timeline (seconds)."),
                    "track": arg("integer", "The track (index) for the stream of its kind."),
                    "media_offset": arg("number", "Where in the media the event starts (seconds)."),
                    "length": arg("number", "How long the event is (seconds)."),
                    "video": arg("boolean", "Place its picture (default true)."),
                    "audio": arg("boolean", "Place its sound (default true)."),
                })
            },
            required: &["media"],
            kind: Kind::Edit,
            run: add_event,
        },
        Tool {
            name: "move_event",
            title: "Move an event",
            description: "Moves an event (its group with it) to a start time and/or onto another track of its kind.",
            properties: || {
                json!({
                    "event": arg("integer", "The event's id."),
                    "start": arg("number", "Its new start (seconds)."),
                    "track": arg("integer", "The track it goes to (index)."),
                })
            },
            required: &["event"],
            kind: Kind::Edit,
            run: move_event,
        },
        Tool {
            name: "trim_event",
            title: "Trim an event",
            description: "Moves an event's start and/or end edge (its group's with it), the media staying in place.",
            properties: || {
                json!({
                    "event": arg("integer", "The event's id."),
                    "start": arg("number", "Its new start (seconds)."),
                    "end": arg("number", "Its new end (seconds)."),
                })
            },
            required: &["event"],
            kind: Kind::Edit,
            run: trim_event,
        },
        Tool {
            name: "split_event",
            title: "Split events",
            description: "Splits an event at a time (or, without `event`, every event there). Returns the new events (the parts after the time).",
            properties: || {
                json!({
                    "at": arg("number", "The time (seconds)."),
                    "event": arg("integer", "Only this event (id)."),
                })
            },
            required: &["at"],
            kind: Kind::Edit,
            run: split_event,
        },
        Tool {
            name: "delete_events",
            title: "Delete events",
            description: "Deletes events (with their groups, as the user's preferences say). `ripple` closes the gaps they leave.",
            properties: || {
                json!({
                    "events": arg("array", "The events' ids."),
                    "ripple": arg("boolean", "Close the gaps (default false)."),
                })
            },
            required: &["events"],
            kind: Kind::Remove,
            run: delete_events,
        },
        Tool {
            name: "set_event",
            title: "Change an event",
            description: "Changes an event: level (video opacity 0-1, audio gain 1 = 0 dB), mute, lock, playback rate (1 = normal; changes its length), length, media offset, fade-in and fade-out lengths and curves, name (its take's).",
            properties: || {
                json!({
                    "event": arg("integer", "The event's id."),
                    "level": arg("number", "Video: opacity 0-1. Audio: linear gain, 1 = 0 dB."),
                    "muted": arg("boolean", "Muted or not."),
                    "locked": arg("boolean", "Locked or not."),
                    "rate": arg("number", "Playback rate, 1 = normal (0.05 to 20)."),
                    "length": arg("number", "Its length (seconds)."),
                    "media_offset": arg("number", "The media's time at its start (seconds)."),
                    "fade_in": arg("number", "The fade-in's length (seconds)."),
                    "fade_out": arg("number", "The fade-out's length (seconds)."),
                    "fade_in_curve": choice(&CURVES, "The fade-in's curve."),
                    "fade_out_curve": choice(&CURVES, "The fade-out's curve."),
                    "name": arg("string", "Its name (the active take's)."),
                })
            },
            required: &["event"],
            kind: Kind::Edit,
            run: set_event,
        },
        Tool {
            name: "group_events",
            title: "Group or ungroup events",
            description: "Groups events (they move and trim together) or, with `group: false`, takes them out of their groups.",
            properties: || {
                json!({
                    "events": arg("array", "The events' ids."),
                    "group": arg("boolean", "true groups them (default), false ungroups."),
                })
            },
            required: &["events"],
            kind: Kind::Edit,
            run: group_events,
        },
        Tool {
            name: "list_markers",
            title: "List markers and regions",
            description: "The project's markers (id, time, name) and regions (id, start, end, name).",
            properties: || json!({}),
            required: &[],
            kind: Kind::Read,
            run: list_markers,
        },
        Tool {
            name: "add_marker",
            title: "Add a marker",
            description: "Adds a marker at a time, with a name.",
            properties: || {
                json!({
                    "at": arg("number", "Its time (seconds)."),
                    "name": arg("string", "Its name."),
                })
            },
            required: &["at"],
            kind: Kind::Edit,
            run: add_marker,
        },
        Tool {
            name: "add_region",
            title: "Add a region",
            description: "Adds a region from a start to an end, with a name.",
            properties: || {
                json!({
                    "start": arg("number", "Its start (seconds)."),
                    "end": arg("number", "Its end (seconds)."),
                    "name": arg("string", "Its name."),
                })
            },
            required: &["start", "end"],
            kind: Kind::Edit,
            run: add_region,
        },
        Tool {
            name: "delete_marker",
            title: "Delete a marker or region",
            description: "Deletes a marker or a region by its id.",
            properties: || json!({"marker": arg("integer", "Its id.")}),
            required: &["marker"],
            kind: Kind::Remove,
            run: delete_marker,
        },
        Tool {
            name: "list_envelopes",
            title: "List envelopes",
            description: "A track's or an event's envelopes and their points (times in seconds from the project's start).",
            properties: || {
                json!({
                    "track": arg("integer", "The track (index)."),
                    "event": arg("integer", "The event (id)."),
                })
            },
            required: &[],
            kind: Kind::Read,
            run: list_envelopes,
        },
        Tool {
            name: "set_envelope",
            title: "Set an envelope's points",
            description: "Adds a track's or an event's envelope when it has none, and sets points on it (a point at a time already there is changed). Tracks: volume (gain, 1 = 0 dB, up to 3.98), pan (-1 to 1), mute (0 or 1) on audio; composite_level (0-1), fade_to_color (-1 to 1), mute on video. Video events: velocity (1 = 100 %, -1 to 3), transition_progress (0-1). `clear` starts it anew, `remove` takes it away.",
            properties: || {
                json!({
                    "track": arg("integer", "The track (index)."),
                    "event": arg("integer", "The event (id)."),
                    "kind": choice(&ENVELOPES, "Which envelope."),
                    "points": {
                        "type": "array",
                        "description": "Points: {\"at\": seconds from the project's start, \"value\": number, \"curve\": linear|fast|slow|smooth|sharp|hold}.",
                        "items": {"type": "object"},
                    },
                    "clear": arg("boolean", "Start from an envelope with no points of its own."),
                    "remove": arg("boolean", "Take the envelope away."),
                })
            },
            required: &["kind"],
            kind: Kind::Edit,
            run: set_envelope,
        },
        Tool {
            name: "set_cursor",
            title: "Move the cursor",
            description: "Moves the edit cursor (where add_event and the window's commands work).",
            properties: || json!({"at": arg("number", "The time (seconds).")}),
            required: &["at"],
            kind: Kind::Edit,
            run: set_cursor,
        },
        Tool {
            name: "select_events",
            title: "Select events",
            description: "Selects events (their groups with them), for the window's commands (run_command) that act on the selection.",
            properties: || {
                json!({
                    "events": arg("array", "The events' ids."),
                    "add": arg("boolean", "Add to the selection instead of replacing it."),
                })
            },
            required: &["events"],
            kind: Kind::Edit,
            run: select_events,
        },
        Tool {
            name: "set_loop_region",
            title: "Set the loop region",
            description: "Sets the time selection (loop region) from a start to an end, or clears it.",
            properties: || {
                json!({
                    "start": arg("number", "Its start (seconds)."),
                    "end": arg("number", "Its end (seconds)."),
                    "clear": arg("boolean", "Clear it instead."),
                })
            },
            required: &[],
            kind: Kind::Edit,
            run: set_loop_region,
        },
        Tool {
            name: "play",
            title: "Play",
            description: "Plays the project in the window, from a time (the cursor if not given).",
            properties: || json!({"from": arg("number", "Where to start (seconds).")}),
            required: &[],
            kind: Kind::Window,
            run: play,
        },
        Tool {
            name: "stop",
            title: "Stop",
            description: "Stops playback in the window.",
            properties: || json!({}),
            required: &[],
            kind: Kind::Window,
            run: stop,
        },
    ]
}

/// The key curve's names, for the effects' keyframes.
pub(super) const KEYFRAME_CURVES: [&str; 6] = KEY_CURVES;
