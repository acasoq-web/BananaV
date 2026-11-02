//! Multicamera editing (Tools › Multicamera), as VEGAS's: the cameras'
//! events lined up — by their sound (`Engine::start_audio_sync`, in the
//! background), their media's timecode or the dates they were recorded on
//! (`bv_engine::media_stamps`), the event picked first staying —, the
//! camera tracks made into a multicamera track
//! (`Project::create_multicamera_track`) and back, and its editing mode —
//! the Video Preview showing every take of the multicamera event at the
//! cursor in tiles (the active one outlined in Preferences' "Active take
//! indicator" color, the takes' numbers and names as the preferences say;
//! on the external monitor too, unless Show Full Frame on External
//! Monitor), a click on one or its number key cutting to it there, playing
//! or not, or over the time selection while stopped — with Ctrl a
//! crossfade, as Preferences › Editing's cut-to-overlap conversion says —
//! (Edit Multicamera Audio with Video: the grouped audio too).

use std::collections::BTreeSet;

use bv_core::Tick;
use bv_engine::{MediaStamps, SyncOutcome};
use bv_timeline::{Clip, ClipId, Project, TrackId, TrackKind, Transform, Transition};

use crate::app::BananaApp;
use crate::egui::{self, Color32, Pos2, Rect, Stroke, pos2};
use crate::i18n::tr;

/// One tile of the multicamera view: where it is in the frame (0–1), the
/// take it shows and its name.
#[derive(Clone, Debug)]
pub(crate) struct Tile {
    pub area: Rect,
    pub take: usize,
    pub name: String,
    pub none: bool,
}

/// How the tiles' outlines and labels are drawn.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TileLook {
    /// The take that plays.
    pub active: usize,
    /// Preferences › Video's "Active take indicator".
    pub indicator: Color32,
    pub names: bool,
    pub numbers: bool,
}

/// The tiles of event `clip`'s takes: a project that draws all of them at
/// once (each take an event of its own on a track of its own, framed in its
/// tile), and where each is.
pub(crate) fn tiles(p: &Project, clip: ClipId) -> Option<(Project, Vec<Tile>)> {
    let c = p.clip(clip)?;
    let list = c.take_list();
    let n = list.len();
    if n < 2 {
        return None;
    }
    let cols = (n as f32).sqrt().ceil() as usize;
    let rows = n.div_ceil(cols);
    let scale = 1.0 / cols.max(rows) as f32;
    let mut out = Project::new(p.settings);
    let mut tiles = Vec::new();
    for (k, take) in list.iter().enumerate() {
        let (col, row) = (k % cols, k / cols);
        let area = Rect::from_min_size(
            pos2(col as f32 / cols as f32, row as f32 / rows as f32),
            egui::vec2(1.0 / cols as f32, 1.0 / rows as f32),
        );
        let name = p.take_name(clip, k).unwrap_or_default();
        tiles.push(Tile {
            area,
            take: k,
            name,
            none: take.none,
        });
        if take.none {
            continue;
        }
        let Some(info) = p.media(take.media) else {
            continue;
        };
        let media = out.add_media(info.clone());
        let track = if k == 0 {
            out.tracks[0].id
        } else {
            out.add_track(TrackKind::Video)
        };
        let Ok(ids) = out.insert_media_range(
            media,
            c.start,
            c.source_in + take.offset,
            c.duration,
            Some(track),
            None,
        ) else {
            continue;
        };
        for id in ids {
            if let Some(e) = out.clip_mut(id) {
                e.rate = c.rate;
                e.looped = c.looped;
                e.hold_last = c.hold_last;
                e.transform = Transform {
                    scale,
                    x: area.center().x - 0.5,
                    y: area.center().y - 0.5,
                    ..Transform::default()
                };
            }
        }
    }
    // Only video tracks draw.
    out.tracks.retain(|t| t.kind == TrackKind::Video);
    Some((out, tiles))
}

/// A synchronization by sound under way: the events whose sound is read,
/// the target first, with their starts and media times then — its result
/// lands where the target is now.
#[derive(Clone, Debug)]
pub(crate) struct PendingSync {
    events: Vec<(ClipId, Tick, Tick)>,
}

/// What Sync … to Align Events lines events up by, besides their sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncBy {
    /// The time of day of their media's timecode.
    Timecode,
    /// The date and time they were recorded on.
    Date,
}

/// How events moved by a synchronization fared.
#[derive(Debug, Default)]
struct Tally {
    moved: usize,
    /// They would have had to start before the timeline.
    too_early: usize,
}

/// Where each event goes against the target, from what its media's stamp
/// says (`stamps`: the event, its media's stamp in seconds, its media time
/// at its start; the target first): the target's start plus the time
/// between their starts' moments. A time of day (`wrap_day`) wraps to the
/// nearest: a recording past midnight. `None` without the target's stamp;
/// events without one are left out.
fn stamp_offsets(
    stamps: &[(ClipId, Option<f64>, Tick)],
    wrap_day: bool,
) -> Option<Vec<(ClipId, Tick)>> {
    let ((_, target, from), others) = stamps.split_first()?;
    let base = (*target)? + from.as_seconds_f64();
    Some(
        others
            .iter()
            .filter_map(|&(id, stamp, from)| {
                let mut d = stamp? + from.as_seconds_f64() - base;
                if wrap_day {
                    const DAY: f64 = 86_400.0;
                    d -= DAY * (d / DAY).round();
                }
                Some((id, Tick::from_seconds_f64(d)))
            })
            .collect(),
    )
}

impl BananaApp {
    /// The tracks of the selected events: the cameras Create Multicamera
    /// Track makes one track of (Ctrl+A: all of them).
    fn multicam_tracks(&self) -> Vec<TrackId> {
        let p = self.project();
        let mut v: Vec<TrackId> = Vec::new();
        for id in self.selected() {
            if let Some(t) = p.clip_track(id)
                && !v.contains(&t.id)
            {
                v.push(t.id);
            }
        }
        v
    }

    pub(crate) fn can_create_multicam(&self) -> bool {
        let p = self.project();
        self.multicam_tracks()
            .iter()
            .filter(|&&t| p.track(t).is_some_and(|t| t.kind == TrackKind::Video))
            .count()
            >= 2
    }

    /// Create Multicamera Track.
    pub(crate) fn create_multicam(&mut self) {
        if !self.can_create_multicam() {
            return;
        }
        let tracks = self.multicam_tracks();
        self.modify(|p| {
            let _ = p.create_multicamera_track(&tracks);
        });
        self.selection.clear();
    }

    /// The multicamera track Expand to Multiple Tracks works on: the
    /// selected one, or the selected event's, when its events have takes.
    pub(crate) fn multicam_track(&self) -> Option<TrackId> {
        let p = self.project();
        let candidate = self.selected_track.or_else(|| {
            self.primary_clip()
                .and_then(|c| p.clip_track(c).map(|t| t.id))
        })?;
        p.track(candidate)
            .filter(|t| t.clips.iter().any(|c: &Clip| c.take_count() > 1))
            .map(|t| t.id)
    }

    /// Expand to Multiple Tracks…: asks whether to keep the unused takes
    /// as muted events.
    pub(crate) fn ask_expand_multicam(&mut self) {
        let Some(track) = self.multicam_track() else {
            return;
        };
        self.ask_user(crate::ask::Ask {
            title: tr("Expand to Multiple Tracks").to_string(),
            text: tr(
                "Keep the takes not in use as muted events? (Yes keeps them, so the multicamera track can be made again; No deletes them.)",
            )
            .to_string(),
            what: crate::ask::Asked::ExpandMulticam(track),
            check: None,
        });
    }

    pub(crate) fn expand_multicam(&mut self, track: TrackId, keep: bool) {
        self.modify(|p| {
            let _ = p.expand_multicamera_track(track, keep);
        });
    }

    /// The multicamera event the view shows: at the frame of the playing
    /// position or the cursor (where it starts: what the preview draws).
    pub(crate) fn multicam_event(&self) -> Option<ClipId> {
        self.project()
            .multicamera_event_at(self.multicam_frame_time())
    }

    /// The start of the frame at the playing position or the cursor.
    fn multicam_frame_time(&self) -> Tick {
        self.project()
            .settings
            .frame_rate
            .snap_floor(self.display_time())
            .max(Tick::ZERO)
    }

    /// The multicamera view now: the tiles of the multicamera event at the
    /// cursor and how to draw them (none without multicamera editing or
    /// such an event).
    pub(crate) fn multicam_view(&self) -> Option<(Project, Vec<Tile>, TileLook)> {
        if !self.multicam {
            return None;
        }
        let clip = self.multicam_event()?;
        let (project, tiles) = tiles(self.project(), clip)?;
        let [r, g, b, a] = self.settings.prefs.take_indicator;
        let look = TileLook {
            active: self.project().clip(clip).map_or(0, Clip::active_take),
            indicator: Color32::from_rgba_unmultiplied(r, g, b, a),
            names: self.settings.prefs.take_names,
            numbers: self.settings.prefs.take_numbers,
        };
        Some((project, tiles, look))
    }

    /// Cuts the multicamera event to take `k` at the playing position (or
    /// the cursor), on a frame — while stopped with the cursor in a time
    /// selection, over all of it —; with `crossfade` (Ctrl) the cuts made
    /// crossfades.
    pub(crate) fn multicam_switch(&mut self, k: usize, crossfade: bool) {
        let Some(clip) = self.multicam_event() else {
            return;
        };
        let p = self.project();
        let Some(track) = p.clip_track(clip).map(|t| t.id) else {
            return;
        };
        let at = self.multicam_frame_time();
        let now = self.display_time();
        let selection = p
            .loop_region
            .filter(|r| !r.is_empty() && !self.is_playing() && r.start <= now && now <= r.end());
        let with_group = !self.settings.multicam_audio_apart;
        let overlap = crossfade.then(|| self.settings.prefs.cut_overlap_split());
        self.modify(|p| {
            let cuts = match selection {
                Some(r) => p
                    .recut_takes(track, r, k, with_group)
                    .map(|_| vec![r.start, r.end()]),
                None => p.switch_take_at(clip, at, k, with_group).map(|_| vec![at]),
            };
            if let (Ok(cuts), Some(overlap)) = (cuts, overlap) {
                for t in cuts {
                    crossfade_at(p, track, t, overlap);
                }
            }
        });
    }

    /// In multicamera editing, the number keys (1–9; with Ctrl, a
    /// crossfade) choose the takes.
    pub(crate) fn multicam_keys(&mut self, ctx: &egui::Context) {
        if !self.multicam || ctx.egui_wants_keyboard_input() {
            return;
        }
        use egui::Key;
        const DIGITS: [Key; 9] = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
            Key::Num9,
        ];
        let pressed = ctx.input_mut(|i| {
            [false, true].into_iter().find_map(|ctrl| {
                let m = if ctrl {
                    egui::Modifiers::COMMAND
                } else {
                    egui::Modifiers::NONE
                };
                DIGITS
                    .iter()
                    .position(|&k| i.consume_key(m, k))
                    .map(|k| (k, ctrl))
            })
        });
        if let Some((k, ctrl)) = pressed {
            self.multicam_switch(k, ctrl);
        }
    }

    /// The events Sync … to Align Events lines up: one of each group among
    /// the picked ones — an audio one where the group has one, as its sound
    /// is what counts —, the target first: the event picked first (VEGAS:
    /// click the target, then Ctrl+click the others), else the topmost.
    fn sync_events(&self) -> Vec<ClipId> {
        let p = self.project();
        let picked = self.picked();
        let place = |id: &ClipId| {
            let track = p
                .clip_track(*id)
                .and_then(|t| p.track_index(t.id))
                .unwrap_or(usize::MAX);
            (track, p.clip(*id).map_or(Tick::ZERO, |c| c.start))
        };
        let mut order: Vec<ClipId> = picked.iter().copied().collect();
        order.sort_by_key(place);
        let audio = |c: &ClipId| p.clip_track(*c).is_some_and(|t| t.kind == TrackKind::Audio);
        let mut seen = BTreeSet::new();
        let mut units: Vec<(ClipId, Vec<ClipId>)> = Vec::new();
        for id in order {
            if seen.contains(&id) {
                continue;
            }
            let group = p.group_members(id);
            seen.extend(group.iter().copied());
            let sound = group
                .iter()
                .copied()
                .find(|c| audio(c) && picked.contains(c))
                .or_else(|| group.iter().copied().find(audio))
                .unwrap_or(id);
            units.push((sound, group));
        }
        if let Some(first) = self.first_pick.filter(|f| picked.contains(f))
            && let Some(i) = units.iter().position(|(_, g)| g.contains(&first))
        {
            let target = units.remove(i);
            units.insert(0, target);
        }
        units.into_iter().map(|(sound, _)| sound).collect()
    }

    pub(crate) fn can_sync_events(&self) -> bool {
        self.sync_events().len() >= 2
    }

    /// Sync Mixed Audio to Align Events: the events' sound read and
    /// compared in the background (the status bar shows how far), then
    /// each moved (`poll_audio_sync`).
    pub(crate) fn sync_audio(&mut self) {
        let events = self.sync_events();
        if events.len() < 2 || self.audio_sync.is_some() {
            return;
        }
        self.engine.set_project(self.history.current());
        if !self.engine.start_audio_sync(&events) {
            self.notify(tr(
                "The target event and at least one other need sound to synchronize by.",
            ));
            return;
        }
        let p = self.project();
        self.audio_sync = Some(PendingSync {
            events: events
                .iter()
                .filter_map(|&id| p.clip(id).map(|c| (id, c.start, c.source_in)))
                .collect(),
        });
    }

    /// The synchronization by sound, each frame: once it is done, its
    /// moves (one edit) and what became of each event.
    pub(crate) fn poll_audio_sync(&mut self) {
        if self.audio_sync.is_none() {
            return;
        }
        let Some(progress) = self.engine.audio_sync_progress() else {
            self.audio_sync = None;
            return;
        };
        if !progress.finished {
            return;
        }
        let Some(pending) = self.audio_sync.take() else {
            return;
        };
        self.engine.cancel_audio_sync();
        if let Some(e) = progress.error {
            self.report_error(trf!("Could not read the target event's sound: {}", e));
            return;
        }
        let Some(&(target, target_start, target_in)) = pending.events.first() else {
            return;
        };
        let then = |id: ClipId| pending.events.iter().find(|e| e.0 == id).copied();
        let unchanged = |p: &Project, id: ClipId, source_in: Tick| {
            p.clip(id).is_some_and(|c| c.source_in == source_in)
        };
        let (mut no_match, mut unreadable, mut changed) = (0, 0, 0);
        let mut moves = Vec::new();
        for (id, outcome) in progress.outcomes {
            match outcome {
                SyncOutcome::Move(shift) => match then(id) {
                    Some((_, start, source_in))
                        if unchanged(self.project(), id, source_in)
                            && unchanged(self.project(), target, target_in) =>
                    {
                        moves.push((id, start + shift - target_start));
                    }
                    _ => changed += 1,
                },
                SyncOutcome::NoMatch => no_match += 1,
                SyncOutcome::Unreadable(_) => unreadable += 1,
            }
        }
        let tally = self.align_to(target, moves);
        let mut said = vec![trn!(
            tally.moved,
            "{} event lined up with the target.",
            "{} events lined up with the target.",
            tally.moved
        )];
        if no_match > 0 {
            said.push(trn!(
                no_match,
                "{} event's sound does not match the target's.",
                "{} events' sound does not match the target's.",
                no_match
            ));
        }
        if unreadable > 0 {
            said.push(trn!(
                unreadable,
                "{} event's sound could not be read.",
                "{} events' sound could not be read.",
                unreadable
            ));
        }
        if changed > 0 {
            said.push(trn!(
                changed,
                "{} event was trimmed while its sound was read and stays where it is.",
                "{} events were trimmed while their sound was read and stay where they are.",
                changed
            ));
        }
        self.tell_sync(tally, said);
    }

    /// Sync Media Timecode / Sync Date/Timestamp to Align Events: each
    /// event moved so that the moment its media recorded falls where the
    /// target's does. Dates are what the files say when all of them say
    /// one, else when each file was last written less its length.
    pub(crate) fn sync_by(&mut self, by: SyncBy) {
        let events = self.sync_events();
        if events.len() < 2 {
            return;
        }
        let p = self.project();
        let stamps: Vec<(ClipId, MediaStamps, Tick)> = events
            .iter()
            .filter_map(|&id| {
                let c = p.clip(id)?;
                Some((id, bv_engine::media_stamps(p.media(c.media)?), c.source_in))
            })
            .collect();
        let said_by_files = stamps.iter().all(|(_, s, _)| s.recorded.is_some());
        let value = |s: &MediaStamps| match by {
            SyncBy::Timecode => s.timecode,
            SyncBy::Date if said_by_files => s.recorded,
            SyncBy::Date => s.written,
        };
        let values: Vec<(ClipId, Option<f64>, Tick)> = stamps
            .iter()
            .map(|(id, s, from)| (*id, value(s), *from))
            .collect();
        let Some(offsets) = stamp_offsets(&values, by == SyncBy::Timecode) else {
            self.notify(match by {
                SyncBy::Timecode => tr("The target event's media has no timecode."),
                SyncBy::Date => tr("The target event's media has no date or time stamp."),
            });
            return;
        };
        let missing = values.len() - 1 - offsets.len();
        let tally = self.align_to(events[0], offsets);
        let mut said = vec![trn!(
            tally.moved,
            "{} event lined up with the target.",
            "{} events lined up with the target.",
            tally.moved
        )];
        if missing > 0 {
            said.push(match by {
                SyncBy::Timecode => trn!(
                    missing,
                    "{} event's media has no timecode.",
                    "{} events' media have no timecode.",
                    missing
                ),
                SyncBy::Date => trn!(
                    missing,
                    "{} event's media has no date or time stamp.",
                    "{} events' media have no date or time stamp.",
                    missing
                ),
            });
        }
        self.tell_sync(tally, said);
    }

    /// Moves each event (with its group) to the target's start plus its
    /// offset — on a frame with Quantize to Frames —, in one edit; those
    /// that would start before the timeline stay (VEGAS's: leave room
    /// before them).
    fn align_to(&mut self, target: ClipId, offsets: Vec<(ClipId, Tick)>) -> Tally {
        let Some(base) = self.project().clip(target).map(|c| c.start) else {
            return Tally::default();
        };
        let rate = self.project().settings.frame_rate;
        let quantize = self.quantize;
        let mut tally = Tally::default();
        self.modify(|p| {
            for (id, offset) in offsets {
                let Some(start) = p.clip(id).map(|c| c.start) else {
                    continue;
                };
                let mut to = base + offset;
                if quantize {
                    to = rate.snap_nearest(to);
                }
                let earliest = p
                    .group_members(id)
                    .iter()
                    .filter_map(|&m| p.clip(m).map(|c| c.start))
                    .min()
                    .unwrap_or(start);
                if earliest + (to - start) < Tick::ZERO {
                    tally.too_early += 1;
                } else if p.move_clip(id, to, None).is_ok() {
                    tally.moved += 1;
                }
            }
        });
        tally
    }

    /// Says how a synchronization went.
    fn tell_sync(&mut self, tally: Tally, mut said: Vec<String>) {
        if tally.too_early > 0 {
            said.push(trn!(
                tally.too_early,
                "{} event would have to start before the timeline does: leave room before the events.",
                "{} events would have to start before the timeline does: leave room before the events.",
                tally.too_early
            ));
        }
        self.notify(said.join(" "));
    }
}

/// The cut at `at` on `track` made a crossfade (Ctrl with a take): the
/// event that starts there and the one that ends there grow into each
/// other as Preferences › Editing's cut-to-overlap conversion says
/// (`Project::overlap_cut`), the default transition between them.
fn crossfade_at(p: &mut Project, track: TrackId, at: Tick, (before, after): (Tick, Tick)) {
    let Some(id) = p
        .track(track)
        .and_then(|t| t.clips.iter().find(|c| c.start == at).map(|c| c.id))
    else {
        return;
    };
    let made = p.overlap_cut(id, before, after).unwrap_or(Tick::ZERO);
    if made <= Tick::ZERO {
        return;
    }
    let auto = p.auto_crossfade;
    if let Some(c) = p.clip_mut(id) {
        c.transition = Transition::default();
        c.transition_fx = None;
        if !auto {
            // Without Automatic Crossfades the event fades in over the
            // overlap by itself.
            c.fade_in = c.fade_in.max(made);
        }
    }
}

/// Where tile `t` is drawn over the view in `frame`.
fn tile_rect(frame: Rect, t: &Tile) -> Rect {
    let at = |p: Pos2| {
        pos2(
            frame.left() + p.x * frame.width(),
            frame.top() + p.y * frame.height(),
        )
    };
    Rect::from_min_max(at(t.area.min), at(t.area.max)).shrink(1.0)
}

/// Draws the tiles' outlines and labels over the multicamera view in
/// `frame` (the active take's in the indicator color).
pub(crate) fn paint(painter: &egui::Painter, frame: Rect, tiles: &[Tile], look: &TileLook) {
    let painter = painter.with_clip_rect(frame);
    for t in tiles {
        let rect = tile_rect(frame, t);
        if t.none {
            painter.rect_filled(rect, 0.0, Color32::from_gray(18));
        }
        let stroke = if t.take == look.active {
            Stroke::new(3.0, look.indicator)
        } else {
            Stroke::new(1.0, Color32::from_gray(90))
        };
        painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
        let label = match (look.numbers, look.names) {
            _ if t.none => tr("(no camera)").to_string(),
            (true, true) => format!("{}  {}", t.take + 1, t.name),
            (true, false) => (t.take + 1).to_string(),
            (false, true) => t.name.clone(),
            (false, false) => String::new(),
        };
        if !label.is_empty() {
            let font = egui::FontId::proportional(13.0);
            let galley = painter.layout_no_wrap(label, font, Color32::WHITE);
            let pos = rect.min + egui::vec2(6.0, 5.0);
            painter.rect_filled(
                Rect::from_min_size(
                    pos - egui::vec2(3.0, 2.0),
                    galley.size() + egui::vec2(6.0, 4.0),
                ),
                2.0,
                Color32::from_black_alpha(160),
            );
            painter.galley(pos, galley, Color32::WHITE);
        }
    }
}

/// The tiles over the Video Preview: drawn, and a click on one cuts to it
/// (with Ctrl, a crossfade).
pub(crate) fn overlay(
    ui: &egui::Ui,
    app: &mut BananaApp,
    frame: Rect,
    tiles: &[Tile],
    look: &TileLook,
    resp: &egui::Response,
) {
    paint(ui.painter(), frame, tiles, look);
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
        && let Some(t) = tiles
            .iter()
            .find(|t| !t.none && tile_rect(frame, t).contains(p))
    {
        let crossfade = ui.input(|i| i.modifiers.command);
        app.multicam_switch(t.take, crossfade);
    }
}

/// The synchronization by sound's progress in the status bar, with Cancel.
pub(crate) fn status(app: &mut BananaApp, ui: &mut egui::Ui) {
    if app.audio_sync.is_none() {
        return;
    }
    let Some(p) = app.engine.audio_sync_progress().filter(|p| !p.finished) else {
        return;
    };
    if ui.button(tr("Cancel")).clicked() {
        app.engine.cancel_audio_sync();
        app.audio_sync = None;
        return;
    }
    ui.add(
        egui::ProgressBar::new(p.done as f32 / p.total.max(1) as f32)
            .desired_width(200.0)
            .show_percentage(),
    );
    ui.label(tr("Synchronizing audio…"));
    ui.separator();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_place_events_against_the_target() {
        let s = Tick::from_seconds;
        // The target's media started at 10:00:00 and it shows it from 5 s;
        // the second's at 10:00:20, shown from its start; the third has
        // none.
        let stamps = [
            (ClipId(1), Some(36_000.0), s(5)),
            (ClipId(2), Some(36_020.0), Tick::ZERO),
            (ClipId(3), None, Tick::ZERO),
        ];
        assert_eq!(stamp_offsets(&stamps, true), Some(vec![(ClipId(2), s(15))]));
        // Past midnight: 23:59:50 and 00:00:10 are 20 s apart.
        let late = [
            (ClipId(1), Some(86_390.0), Tick::ZERO),
            (ClipId(2), Some(10.0), Tick::ZERO),
        ];
        assert_eq!(stamp_offsets(&late, true), Some(vec![(ClipId(2), s(20))]));
        // Without the target's, nothing.
        assert_eq!(stamp_offsets(&stamps[2..], true), None);
    }
}
