//! Actions behind the File, Edit and View menus that are not part of the
//! basic editing core in `app.rs`.

use crate::i18n::tr;
use std::path::PathBuf;

use bv_core::{Tick, TimeRange};
use bv_timeline::{ChannelMap, ClipId, RippleScope, TrackId, TrackKind};

use super::{BananaApp, Pending, Tool};
use crate::dock::{self, Tab};

/// Built-in window layouts (View › Window Layouts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutPreset {
    Default,
    AudioMixing,
    ColorCorrection,
    SourceWindow,
    TimelineTop,
}

impl LayoutPreset {
    pub const ALL: [LayoutPreset; 5] = [
        LayoutPreset::Default,
        LayoutPreset::AudioMixing,
        LayoutPreset::ColorCorrection,
        LayoutPreset::SourceWindow,
        LayoutPreset::TimelineTop,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LayoutPreset::Default => tr("Default Layout"),
            LayoutPreset::AudioMixing => tr("Audio Mixing"),
            LayoutPreset::ColorCorrection => tr("Color Correction"),
            LayoutPreset::SourceWindow => tr("Source Window Layout"),
            LayoutPreset::TimelineTop => tr("Timeline Top Layout"),
        }
    }
}

impl BananaApp {
    // ---- File ---------------------------------------------------------------

    /// Runs `p` now, or after the "Save changes?" question when the project
    /// has unsaved changes.
    pub(crate) fn request(&mut self, p: Pending) {
        if self.is_dirty() {
            self.confirm = Some(p);
        } else {
            self.perform(p);
        }
    }

    pub(crate) fn perform(&mut self, p: Pending) {
        match p {
            Pending::New => {
                self.new_project();
                self.open_project_settings();
            }
            Pending::Open => self.open_dialog(),
            Pending::OpenPath(path) => self.open_path(path),
            Pending::NewSized {
                width,
                height,
                rate,
                advanced,
            } => {
                self.new_project_sized(width, height, rate);
                if advanced {
                    self.open_project_settings();
                }
            }
            Pending::Close => {
                self.new_project();
                self.notify(tr("Project closed."));
            }
            Pending::Demo => self.load_demo(),
            Pending::Exit => self.exit_now = true,
            Pending::ImportOtio => crate::otio_ui::import_dialog(self),
            Pending::ImportFcp(fcpxml) => crate::otio_ui::import_fcp(self, fcpxml),
        }
    }

    /// File › Save Incrementally: saves as the next numbered copy
    /// (`name_001`, `name_002`, …) and keeps working on it.
    pub(crate) fn save_incremental(&mut self) {
        let Some(path) = self.project_path.clone() else {
            self.save(true);
            return;
        };
        let stem = path.file_stem().map_or(tr("Untitled").to_string(), |s| {
            s.to_string_lossy().to_string()
        });
        let ext = path
            .extension()
            .map_or("bvproj".to_string(), |s| s.to_string_lossy().to_string());
        let (base, mut n) = match stem.rsplit_once('_') {
            Some((b, num)) if num.len() == 3 && num.chars().all(|c| c.is_ascii_digit()) => {
                (b.to_string(), num.parse::<u32>().unwrap_or(0))
            }
            _ => (stem.clone(), 0),
        };
        let dir = path.parent().map(PathBuf::from).unwrap_or_default();
        let next = loop {
            n += 1;
            let candidate = dir.join(format!("{base}_{n:03}.{ext}"));
            if !candidate.exists() {
                break candidate;
            }
        };
        self.project_path = Some(next);
        self.save(false);
    }

    /// File › Export › BananaV project: writes a copy and keeps the current
    /// file open.
    pub(crate) fn export_project_copy(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr("Export BananaV Project"))
            .add_filter(tr("BananaV Project"), &["bvproj"])
            .set_file_name(tr("Copy.bvproj"))
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, self.project().to_json()) {
            Ok(()) => self.notify(trf!("Project copy saved to {}", path.display())),
            Err(e) => self.report_error(trf!("Could not export: {}", e)),
        }
    }

    // ---- Edit -----------------------------------------------------------------

    pub(crate) fn undo_label(&self) -> Option<&'static str> {
        let before = self.history.undo_target()?;
        Some(crate::history_label::describe(before, self.project()))
    }

    pub(crate) fn redo_label(&self) -> Option<&'static str> {
        let after = self.history.redo_target()?;
        Some(crate::history_label::describe(self.project(), after))
    }

    pub(crate) fn undo_all(&mut self) {
        if self.history.undo_all() {
            self.prune_selection();
            self.notify(tr("All edits were undone (Redo brings them back)."));
        }
    }

    pub(crate) fn clear_history(&mut self) {
        self.history.clear_history();
        self.notify(tr("Edit history cleared."));
    }

    /// Tracks of the clipboard clips (as they exist now).
    fn clipboard_tracks(&self) -> Vec<TrackId> {
        let Some(data) = &self.clipboard else {
            return Vec::new();
        };
        data.tracks()
            .into_iter()
            .filter(|t| self.project().track(*t).is_some())
            .collect()
    }

    /// Edit › Paste Insert: opens room at the cursor on the pasted tracks,
    /// then pastes.
    pub(crate) fn paste_insert(&mut self) {
        let Some(data) = self.clipboard.clone() else {
            return;
        };
        let at = self.cursor;
        let span = data.span();
        let tracks = self.clipboard_tracks();
        let mark = self.project().id_counter();
        let ids = self.modify(|p| {
            p.insert_space(at, span, &tracks, tracks.is_empty());
            p.paste_clips(&data, at)
        });
        self.selection = ids.into_iter().collect();
        self.ask_generated_copies(mark);
    }

    /// Edit › Paste Repeat: pastes `times` copies, one after the other
    /// (`every` = start-to-start spacing, `None` = end to end).
    pub(crate) fn paste_repeat(&mut self, times: usize, every: Option<Tick>) {
        let Some(data) = self.clipboard.clone() else {
            return;
        };
        let step = every
            .unwrap_or(data.span())
            .max(self.frame_rate().frame_duration());
        let start = self.cursor;
        let mark = self.project().id_counter();
        let ids = self.modify(|p| {
            let mut out = Vec::new();
            for i in 0..times {
                out.extend(p.paste_clips(&data, start + Tick(step.0 * i as i64)));
            }
            out
        });
        self.selection = ids.into_iter().collect();
        self.ask_generated_copies(mark);
    }

    /// Edit › Paste Event Attributes: copies effects, pan/crop, level, fades,
    /// the video's velocity and transition progress envelopes and the
    /// audio's switches from the clipboard event to the selected ones.
    pub(crate) fn paste_attributes(&mut self) {
        let Some(data) = self.clipboard.clone() else {
            return;
        };
        let targets = self.selected();
        let mut n = 0;
        self.modify(|p| {
            for id in targets {
                let Some(kind) = p.clip_track(id).map(|t| t.kind) else {
                    continue;
                };
                let Some(src) = data.first_of(kind).cloned() else {
                    continue;
                };
                if let Some(c) = p.clip_mut(id) {
                    c.effects = src.effects.clone();
                    c.pan_crop_at = src.pan_crop_at;
                    c.level = src.level;
                    c.fade_in = src.fade_in.min(c.duration);
                    c.fade_out = src.fade_out.min(c.duration - c.fade_in);
                    c.fade_in_curve = src.fade_in_curve;
                    c.fade_out_curve = src.fade_out_curve;
                    if kind == TrackKind::Video {
                        c.pan_crop = src.pan_crop.clone();
                        c.transition = src.transition;
                        // Its velocity and transition progress envelopes.
                        c.envelopes = src.envelopes.clone();
                    } else {
                        c.channels = src.channels;
                        c.invert_phase = src.invert_phase;
                        c.normalize = src.normalize;
                    }
                    n += 1;
                }
            }
        });
        self.notify(trn!(
            n,
            "Attributes pasted into {} event.",
            "Attributes pasted into {} events.",
            n
        ));
    }

    /// Tracks holding the selected events (empty = no selection).
    fn selected_tracks(&self) -> Vec<TrackId> {
        let p = self.project();
        let mut v: Vec<TrackId> = self
            .selection
            .iter()
            .filter_map(|&id| p.clip_track(id).map(|t| t.id))
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// Edit › Smart Split: cuts the time selection out of the selected
    /// events' tracks (or every track) and closes the hole.
    pub(crate) fn smart_split(&mut self) {
        let Some(r) = self
            .project()
            .loop_region
            .filter(|r| r.duration > Tick::ZERO)
        else {
            self.notify(tr("Make a time selection over the part to remove."));
            return;
        };
        let tracks = self.selected_tracks();
        let all = tracks.is_empty();
        // The parts left overlap by Options › Preferences › Editing's
        // "Smart split default length (frames)", a transition between them.
        let overlap = self
            .frame_rate()
            .frame_start(self.settings.prefs.smart_split_frames as i64);
        // VEGAS's Warp Flow joins the video parts (a crossfade where the
        // plug-in did not load).
        let warp = crate::ofx::transitions()
            .into_iter()
            .find(|p| p.identifier == bv_engine::plugins::WARP_FLOW_ID)
            .map(|p| crate::ofx::plugin_fx(&p));
        let removed = self.modify(|p| {
            let n = p.smart_split(r, &tracks, all, overlap, warp.as_ref());
            p.loop_region = None;
            n
        });
        self.prune_selection();
        self.cursor = r.start;
        self.notify(trn!(
            removed,
            "Part removed ({} event cut).",
            "Part removed ({} events cut).",
            removed
        ));
    }

    pub(crate) fn rejoin_selection(&mut self) {
        let ids = self.selected();
        let joined = self.edit(|p| -> Result<bool, bv_timeline::EditError> {
            let mut any = false;
            for id in ids {
                if p.clip(id).is_some() {
                    any |= p.rejoin(id)?;
                }
            }
            Ok(any)
        });
        if joined == Some(false) {
            self.notify(tr(
                "Only neighboring events from the same split can be rejoined.",
            ));
        }
        self.prune_selection();
    }

    pub(crate) fn close_gaps(&mut self) {
        let ids = self.selected();
        let n = self.modify(|p| p.close_gaps(&ids));
        if n == 0 {
            self.notify(tr("There are no gaps between the selected events."));
        }
    }

    pub(crate) fn post_edit_ripple(&mut self, scope: RippleScope) {
        let Some(id) = self.primary_clip() else {
            return;
        };
        self.edit(|p| p.post_edit_ripple(id, scope));
    }

    // ---- Edit › Navigate --------------------------------------------------------

    fn view_range(&self) -> TimeRange {
        let v = &self.timeline;
        TimeRange::new(
            v.view_start,
            Tick((v.lanes_width as f64 * v.ticks_per_px) as i64),
        )
    }

    pub(crate) fn go_selection_edge(&mut self, start: bool) {
        let r = self
            .project()
            .loop_region
            .unwrap_or_else(|| self.view_range());
        self.seek(if start { r.start } else { r.end() });
    }

    pub(crate) fn move_by_grid(&mut self, dir: i64) {
        let rate = self.frame_rate();
        let step = self.timeline.grid_step(rate);
        let grid = rate.frame_at(self.cursor) / step;
        let target = if dir > 0 {
            (grid + 1) * step
        } else if rate.frame_at(self.cursor) % step == 0 {
            (grid - 1) * step
        } else {
            grid * step
        };
        self.seek(rate.frame_start(target.max(0)));
    }

    pub(crate) fn move_by_pixel(&mut self, dir: i64) {
        let px = Tick(self.timeline.ticks_per_px.round() as i64);
        let mut t = self.cursor + Tick(px.0 * dir);
        if self.quantize {
            let rate = self.frame_rate();
            t = rate.snap_nearest(t);
            if t == self.cursor {
                t = rate.frame_start(rate.frame_at(self.cursor) + dir);
            }
        }
        self.seek(t);
    }

    fn marker_points(&self) -> Vec<Tick> {
        let p = self.project();
        let mut v: Vec<Tick> = p
            .markers
            .iter()
            .map(|m| m.time)
            .chain(
                p.regions
                    .iter()
                    .flat_map(|r| [r.range.start, r.range.end()]),
            )
            .collect();
        v.sort();
        v.dedup();
        v
    }

    fn step_to(&mut self, points: Vec<Tick>, dir: i64) {
        let c = self.cursor;
        let t = if dir > 0 {
            points.into_iter().find(|&t| t > c)
        } else {
            points.into_iter().rev().find(|&t| t < c)
        };
        if let Some(t) = t {
            self.seek(t);
        }
    }

    pub(crate) fn move_by_marker(&mut self, dir: i64) {
        let pts = self.marker_points();
        self.step_to(pts, dir);
    }

    /// Selects from the cursor to the next marker or region edge.
    pub(crate) fn select_to_marker(&mut self) {
        let c = self.cursor;
        let end = self
            .marker_points()
            .into_iter()
            .find(|&t| t > c)
            .unwrap_or_else(|| self.project().duration());
        if end > c {
            self.set_loop_region(Some(TimeRange::from_bounds(c, end)));
        }
    }

    pub(crate) fn move_by_edit_point(&mut self, dir: i64) {
        let pts = self.project().snap_points();
        self.step_to(pts, dir);
    }

    pub(crate) fn move_by_selected_event(&mut self, dir: i64) {
        let p = self.project();
        let mut pts: Vec<Tick> = self
            .selection
            .iter()
            .filter_map(|&id| p.clip(id))
            .flat_map(|c| [c.start, c.end()])
            .collect();
        pts.sort();
        pts.dedup();
        self.step_to(pts, dir);
    }

    /// Edit › Navigate › Move Cursor to Playhead: stopping keeps the
    /// current playback position instead of jumping back.
    pub(crate) fn cursor_to_playhead(&mut self) {
        if let Some(pos) = self.playhead {
            self.cursor = self.frame_rate().snap_floor(pos);
        }
    }

    pub(crate) fn center_on_cursor(&mut self) {
        let v = &mut self.timeline;
        let half = Tick((v.lanes_width as f64 * v.ticks_per_px / 2.0) as i64);
        v.view_start = (self.cursor - half).max(Tick::ZERO);
    }

    // ---- Edit › Select ------------------------------------------------------------

    pub(crate) fn select_after_cursor(&mut self) {
        let c = self.cursor;
        self.selection = self
            .project()
            .clips()
            .filter(|(_, clip)| clip.start >= c)
            .map(|(_, clip)| clip.id)
            .collect();
    }

    /// Time selection from the selected event's start to the cursor
    /// (`start`) or from the cursor to its end.
    /// Edit › Select › Event Start / Event End (`[`, `]`): the selected
    /// event's start or end edge to trim (a red bracket, and the cursor
    /// there); while trimming, the track's previous or next edge.
    pub(crate) fn select_event_edge(&mut self, start: bool) {
        let project = self.project().clone();
        let Some(e) = self.timeline.edge_trim else {
            let Some(c) = self.primary_clip().and_then(|id| project.clip(id)) else {
                return;
            };
            self.timeline.trim_edge(&project, c.id, !start, false);
            self.seek(if start { c.start } else { c.end() });
            return;
        };
        let Some(track) = project.track(e.track) else {
            return;
        };
        // The track's edges in time order (an end before a start there).
        let mut edges: Vec<(Tick, bv_timeline::ClipId, bool)> = track
            .clips
            .iter()
            .flat_map(|c| [(c.start, c.id, false), (c.end(), c.id, true)])
            .collect();
        edges.sort_by_key(|&(t, _, end)| (t, !end));
        let Some(i) = edges
            .iter()
            .position(|&(_, id, end)| id == e.clip && end == e.end)
        else {
            return;
        };
        let j = if start {
            i.checked_sub(1)
        } else {
            Some(i + 1).filter(|&j| j < edges.len())
        };
        if let Some((t, id, end)) = j.map(|j| edges[j]) {
            self.timeline.trim_edge(&project, id, end, e.expanded);
            self.select_clip(id, false);
            self.seek(t);
        }
    }

    /// Edit › Select › Enter Expanded Edit Mode: the edge being trimmed
    /// (or the selected event's edge nearest the cursor) shown on the A
    /// and B rolls; again, the timeline as it was.
    pub(crate) fn toggle_expanded_edit(&mut self) {
        let project = self.project().clone();
        match self.timeline.edge_trim {
            Some(e) if e.expanded => self.timeline.edge_trim = None,
            Some(e) => self.timeline.trim_edge(&project, e.clip, e.end, true),
            None => {
                let Some(c) = self.primary_clip().and_then(|id| project.clip(id)) else {
                    return;
                };
                let end = (self.cursor - c.end()).abs() < (self.cursor - c.start).abs();
                self.timeline.trim_edge(&project, c.id, end, true);
            }
        }
    }

    /// Trims the edge being trimmed by `delta`; the cursor follows it.
    pub(crate) fn trim_edit_point(&mut self, delta: Tick) {
        let Some(e) = self.timeline.edge_trim else {
            return;
        };
        let Some(c) = self.project().clip(e.clip).cloned() else {
            return;
        };
        if e.end {
            self.edit(|p| p.trim_end(c.id, c.end() + delta));
        } else {
            self.edit(|p| p.trim_start(c.id, c.start + delta));
        }
        if let Some(c) = self.project().clip(e.clip) {
            let t = if e.end { c.end() } else { c.start };
            self.seek(t);
        }
    }

    pub(crate) fn selection_to_view(&mut self) {
        let r = self.view_range();
        self.set_loop_region(Some(r));
    }

    pub(crate) fn selection_to_project(&mut self) {
        let end = self.project().duration();
        if end > Tick::ZERO {
            self.set_loop_region(Some(TimeRange::new(Tick::ZERO, end)));
        }
    }

    /// Selects the events inside the time selection.
    pub(crate) fn select_in_loop_region(&mut self) {
        let Some(r) = self.project().loop_region else {
            return;
        };
        self.selection = self
            .project()
            .clips()
            .filter(|(_, c)| c.start < r.end() && c.end() > r.start)
            .map(|(_, c)| c.id)
            .collect();
    }

    /// Shift with a key that moved the cursor from `from`: the time
    /// selection runs from its other end (or from `from`) to the cursor,
    /// like VEGAS's.
    pub(crate) fn extend_selection(&mut self, from: Tick) {
        let to = self.cursor;
        let anchor = match self.project().loop_region {
            Some(r) if r.duration > Tick::ZERO && from == r.end() => r.start,
            Some(r) if r.duration > Tick::ZERO && from == r.start => r.end(),
            _ => from,
        };
        if to != anchor {
            let r = TimeRange::from_bounds(anchor.min(to), anchor.max(to));
            let key = crate::egui::Id::new("selecao-de-tempo");
            self.modify_merged(key, |p| p.loop_region = Some(r));
            self.restart_if_playing();
        }
    }

    /// I / O: sets one end of the time selection at the cursor.
    pub(crate) fn set_selection_edge(&mut self, start: bool) {
        let c = self.mark_time();
        let r = self.project().loop_region;
        let (a, b) = match (r, start) {
            (Some(r), true) => (c, r.end().max(c + Tick(1))),
            (Some(r), false) => (r.start.min(c - Tick(1)).max(Tick::ZERO), c),
            (None, true) => (c, self.project().duration().max(c + Tick(1))),
            (None, false) => (Tick::ZERO, c),
        };
        if b > a {
            self.set_loop_region(Some(TimeRange::from_bounds(a, b)));
        }
    }

    /// Selects the previous / next event on the track of the selected one
    /// and makes its span the time selection.
    pub(crate) fn select_neighbour_event(&mut self, dir: i64) {
        let p = self.project();
        let Some(id) = self.primary_clip() else {
            return;
        };
        let (Some(track), Some(c)) = (p.clip_track(id), p.clip(id)) else {
            return;
        };
        let next = if dir > 0 {
            track
                .clips
                .iter()
                .filter(|o| o.start > c.start)
                .min_by_key(|o| o.start)
        } else {
            track
                .clips
                .iter()
                .filter(|o| o.start < c.start)
                .max_by_key(|o| o.start)
        };
        if let Some(o) = next.cloned() {
            self.select_clip(o.id, false);
            self.set_loop_region(Some(o.range()));
        }
    }

    pub(crate) fn track_selection_history(&mut self) {
        let now = self.project().loop_region;
        if now != self.last_selection {
            if !self.restoring_selection {
                self.selection_history.push(self.last_selection);
                if self.selection_history.len() > 50 {
                    self.selection_history.remove(0);
                }
            }
            self.restoring_selection = false;
            self.last_selection = now;
        }
    }

    pub(crate) fn can_restore_selection(&self) -> bool {
        !self.selection_history.is_empty()
    }

    pub(crate) fn restore_selection(&mut self) {
        if let Some(r) = self.selection_history.pop() {
            self.restoring_selection = true;
            self.set_loop_region(r);
        }
    }

    pub(crate) fn toggle_loop_playback(&mut self) {
        self.loop_enabled = !self.loop_enabled;
        self.restart_if_playing();
    }

    // ---- Edit › Switches, Group, Channels -------------------------------------------

    pub(crate) fn selected_of(&self, kind: TrackKind) -> Vec<ClipId> {
        let p = self.project();
        self.selection
            .iter()
            .copied()
            .filter(|&id| p.clip_track(id).is_some_and(|t| t.kind == kind))
            .collect()
    }

    /// `Some(true)` when every selected event has the flag, `None` without
    /// a selection of that kind.
    pub(crate) fn all_selected(
        &self,
        kind: Option<TrackKind>,
        f: impl Fn(&bv_timeline::Clip) -> bool,
    ) -> Option<bool> {
        let p = self.project();
        let ids: Vec<ClipId> = match kind {
            Some(k) => self.selected_of(k),
            None => self.selected(),
        };
        if ids.is_empty() {
            return None;
        }
        Some(ids.iter().all(|&id| p.clip(id).is_some_and(&f)))
    }

    /// Edit › Switches › Loop on the selected events.
    pub(crate) fn toggle_loop(&mut self) {
        let on = !self.all_selected(None, |c| c.looped).unwrap_or(false);
        let ids = self.selected();
        self.modify(|p| {
            for id in ids {
                if let Some(c) = p.clip_mut(id) {
                    c.looped = on;
                }
            }
        });
    }

    /// The event menu's Switches: the selected events loop past their
    /// media's end (`looped`) or, video ones, hold its last frame.
    pub(crate) fn set_event_end(&mut self, looped: bool) {
        let ids = self.selected();
        self.modify(|p| {
            for id in ids {
                let video = p.clip_track(id).is_some_and(|t| t.kind == TrackKind::Video);
                if let Some(c) = p.clip_mut(id) {
                    c.looped = looped;
                    c.all_frames = false;
                    if !looped && video {
                        c.hold_last = true;
                    }
                }
            }
        });
    }

    /// Edit › Switches › Trim Event to Include All Frames on the selected
    /// video events (`on`), or off.
    pub(crate) fn set_all_frames(&mut self, on: bool) {
        let ids = self.selected_of(TrackKind::Video);
        self.modify(|p| {
            for id in ids {
                let _ = p.set_all_frames(id, on);
            }
        });
    }

    /// Edit › Switches › Auto Normalize on the selected audio events: on,
    /// their normalize gain follows their edits (worked out after each).
    pub(crate) fn toggle_auto_normalize(&mut self) {
        let on = !self
            .all_selected(Some(TrackKind::Audio), |c| c.auto_normalize)
            .unwrap_or(false);
        let ids = self.selected_of(TrackKind::Audio);
        self.modify(|p| {
            for id in ids {
                if let Some(c) = p.clip_mut(id) {
                    c.auto_normalize = on;
                }
            }
        });
    }

    /// Edit › Switches' Resample mode on the selected video events
    /// (`None`: the project's).
    pub(crate) fn set_resample(&mut self, mode: Option<bv_timeline::ResampleMode>) {
        let ids = self.selected_of(TrackKind::Video);
        self.modify(|p| {
            for id in ids {
                if let Some(c) = p.clip_mut(id) {
                    c.resample = mode;
                }
            }
        });
    }

    /// Edit › Switches › Reduce Interlace Flicker on the selected video
    /// events.
    pub(crate) fn toggle_reduce_flicker(&mut self) {
        let on = !self
            .all_selected(Some(TrackKind::Video), |c| c.reduce_flicker)
            .unwrap_or(false);
        let ids = self.selected_of(TrackKind::Video);
        self.modify(|p| {
            for id in ids {
                if let Some(c) = p.clip_mut(id) {
                    c.reduce_flicker = on;
                }
            }
        });
    }

    /// Whether a selected event's media has a stream no event of its group
    /// plays (Add Missing Stream to Selected Event).
    pub(crate) fn can_add_missing_stream(&self) -> bool {
        let p = self.project();
        self.selection
            .iter()
            .any(|&id| p.missing_stream(id).is_some())
    }

    /// Insert › Add Missing Stream to Selected Event: each selected event
    /// whose media's other stream no event of its group plays gets it, on a
    /// track of its own beside its track.
    pub(crate) fn add_missing_streams(&mut self) {
        let mut ids: Vec<ClipId> = self.selection.iter().copied().collect();
        ids.sort_by_key(|&id| self.project().clip(id).map(|c| c.start));
        self.modify(|p| {
            for id in ids {
                if p.missing_stream(id).is_some() {
                    let _ = p.add_missing_stream(id);
                }
            }
        });
    }

    /// Keeps the switches that follow edits, as part of the edit that
    /// changed things (no undo of their own): Auto Normalize's gain, worked
    /// out again when an event's part of its media changed, and Trim Event
    /// to Include All Frames' length.
    /// Sync links (VEGAS's Help, "Using sync links"): after each edit, an
    /// event linked to one that moved moves as far, in that edit's undo;
    /// one moved on its own keeps its place, its offset noted; a link to
    /// an event gone goes. Undo and redo find them as they were; while a
    /// drag is under way nothing moves yet.
    pub(crate) fn keep_sync_links(&mut self) {
        if self.timeline.gesture.is_some() {
            return;
        }
        let p = self.project();
        let before = self.history.undo_target();
        let mut fix: Vec<(ClipId, Option<(Tick, Tick)>)> = Vec::new();
        for (_, c) in p.clips() {
            let Some(link) = c.sync_link else {
                continue;
            };
            let Some(m) = p.clip(link.to) else {
                fix.push((c.id, None));
                continue;
            };
            let expected = (m.start + link.offset).max(Tick::ZERO);
            if c.start == expected {
                continue;
            }
            let followed_moved = before
                .and_then(|b| b.clip(m.id))
                .is_none_or(|was| was.start != m.start);
            fix.push(if followed_moved {
                (c.id, Some((expected, link.offset)))
            } else {
                (c.id, Some((c.start, c.start - m.start)))
            });
        }
        if fix.is_empty() {
            return;
        }
        self.history.amend(|p| {
            for (id, f) in fix {
                match f {
                    None => {
                        if let Some(c) = p.clip_mut(id) {
                            c.sync_link = None;
                        }
                    }
                    Some((start, offset)) => {
                        if let Some(l) = p.clip_mut(id).and_then(|c| c.sync_link.as_mut()) {
                            l.offset = offset;
                        }
                        if p.clip(id).is_some_and(|c| c.start != start) {
                            let _ = p.move_clip(id, start, None);
                        }
                    }
                }
            }
        });
    }

    pub(crate) fn keep_switches(&mut self) {
        let p = self.project();
        let target = self.settings.prefs.normalize_peak_db;
        let mut fit = Vec::new();
        let mut stale = Vec::new();
        for (t, c) in p.clips() {
            match t.kind {
                TrackKind::Video if c.all_frames => {
                    let held =
                        p.media(c.media).is_some_and(|m| m.has_frames()) && c.freeze.is_none();
                    let length = p.media(c.media).map(|m| {
                        c.to_timeline((m.duration - c.source_in).max(Tick::ZERO))
                            .max(p.min_clip_duration())
                    });
                    if held && length.is_some_and(|l| l != c.duration) {
                        fit.push(c.id);
                    }
                }
                TrackKind::Audio if c.auto_normalize => {
                    let now = NormalizedFor::of(c, target);
                    if self.auto_normalized.get(&c.id) != Some(&now) {
                        stale.push((c.clone(), now));
                    }
                }
                _ => {}
            }
        }
        let mut gains = Vec::new();
        for (c, now) in stale {
            if let Some(g) = self.normalize_gain(&c) {
                self.auto_normalized.insert(c.id, now);
                if c.normalize != g {
                    gains.push((c.id, g));
                }
            }
        }
        if fit.is_empty() && gains.is_empty() {
            return;
        }
        self.history.amend(|p| {
            for id in fit {
                let _ = p.set_all_frames(id, true);
            }
            for (id, g) in gains {
                if let Some(c) = p.clip_mut(id) {
                    c.normalize = g;
                }
            }
        });
    }

    /// Edit › Switches › Hold Last Frame on the selected video events.
    pub(crate) fn toggle_hold_last(&mut self) {
        let on = !self
            .all_selected(Some(TrackKind::Video), |c| c.hold_last)
            .unwrap_or(false);
        let ids = self.selected_of(TrackKind::Video);
        self.modify(|p| {
            for id in ids {
                if let Some(c) = p.clip_mut(id) {
                    c.hold_last = on;
                }
            }
        });
    }

    pub(crate) fn toggle_mute_selection(&mut self) {
        let on = !self.all_selected(None, |c| c.muted).unwrap_or(false);
        let ids = self.selected();
        self.modify(|p| {
            for id in ids {
                if let Some(c) = p.clip_mut(id) {
                    c.muted = on;
                }
            }
        });
    }

    pub(crate) fn toggle_invert_phase(&mut self) {
        let on = !self
            .all_selected(Some(TrackKind::Audio), |c| c.invert_phase)
            .unwrap_or(false);
        let ids = self.selected_of(TrackKind::Audio);
        self.modify(|p| {
            for id in ids {
                if let Some(c) = p.clip_mut(id) {
                    c.invert_phase = on;
                }
            }
        });
    }

    /// The gain Normalize gives event `c`: its peak (measured on its
    /// waveform) to Options › Preferences › Audio's "Normalize peak level".
    /// `None` while the waveform is being worked out; `Some(None)` for
    /// silence or media that cannot be read.
    pub(crate) fn normalize_gain(&self, c: &bv_timeline::Clip) -> Option<Option<f32>> {
        let p = self.project();
        let Some(info) = p.media(c.media) else {
            return Some(None);
        };
        let wf = self.engine.waveform(info, c.audio_stream)?;
        let (from, to) = (c.source_in, c.source_in + c.to_source(c.duration));
        // A looped event that goes around plays all of its media.
        let (from, to) = if c.looped && (from < Tick::ZERO || to > info.duration) {
            (Tick::ZERO, info.duration)
        } else {
            (from, to)
        };
        let peak = (0..wf.peaks.len())
            .map(|ch| {
                let (lo, hi) = wf.range(ch, from, to);
                lo.abs().max(hi.abs())
            })
            .fold(0.0f32, f32::max);
        let target = bv_mixer_gain(self.settings.prefs.normalize_peak_db);
        Some((peak > 1e-4).then(|| (target / peak).min(1000.0)))
    }

    /// Edit › Switches › Normalize: gain that brings the event's peak to
    /// 0 dBFS (measured on the waveform), or removes it when already on.
    pub(crate) fn toggle_normalize(&mut self) {
        let ids = self.selected_of(TrackKind::Audio);
        if self
            .all_selected(Some(TrackKind::Audio), |c| c.normalize.is_some())
            .unwrap_or(false)
        {
            self.modify(|p| {
                for id in ids {
                    if let Some(c) = p.clip_mut(id) {
                        c.normalize = None;
                    }
                }
            });
            return;
        }
        let mut gains = Vec::new();
        let mut missing = 0;
        for &id in &ids {
            let Some(c) = self.project().clip(id).cloned() else {
                continue;
            };
            match self.normalize_gain(&c) {
                None => missing += 1,
                Some(Some(g)) => gains.push((id, g)),
                Some(None) => {}
            }
        }
        if missing > 0 {
            self.notify(tr("Wait for the waveforms to finish computing."));
        }
        if !gains.is_empty() {
            self.modify(|p| {
                for (id, g) in gains {
                    if let Some(c) = p.clip_mut(id) {
                        c.normalize = Some(g);
                    }
                }
            });
        }
    }

    /// Edit › Switches › Maintain Aspect Ratio: off stretches the media to
    /// fill the output frame.
    pub(crate) fn toggle_maintain_aspect(&mut self) {
        let keep = self
            .all_selected(Some(TrackKind::Video), |c| c.pan_crop.keep_aspect)
            .unwrap_or(true);
        let ids = self.selected_of(TrackKind::Video);
        self.set_maintain_aspect(&ids, !keep);
    }

    /// Maintain Aspect Ratio on video events `ids` (`keep`), or off: their
    /// media stretched to fill the output frame (Pan/Crop's Source).
    pub(crate) fn set_maintain_aspect(&mut self, ids: &[ClipId], keep: bool) {
        let ids = ids.to_vec();
        self.modify(|p| {
            for id in ids {
                if let Some(c) = p.clip_mut(id) {
                    c.pan_crop.keep_aspect = keep;
                }
            }
        });
    }

    pub(crate) fn set_channels(&mut self, map: ChannelMap) {
        let ids = self.selected_of(TrackKind::Audio);
        self.modify(|p| {
            for id in ids {
                if let Some(c) = p.clip_mut(id) {
                    c.channels = map;
                }
            }
        });
    }

    /// Edit › Group › Clear: dissolves the whole groups of the selection.
    pub(crate) fn clear_groups(&mut self) {
        let p = self.project();
        let mut ids: Vec<ClipId> = self
            .selection
            .iter()
            .flat_map(|&id| p.group_members(id))
            .collect();
        ids.sort();
        ids.dedup();
        self.modify(|p| p.ungroup_clips(&ids));
    }

    pub(crate) fn select_groups(&mut self) {
        let p = self.project();
        let ids: Vec<ClipId> = self
            .selection
            .iter()
            .flat_map(|&id| p.group_members(id))
            .collect();
        self.selection.extend(ids);
    }

    pub(crate) fn next_tool(&mut self, step: isize) {
        self.tool = self.tool.next(step);
    }

    // ---- View ------------------------------------------------------------------------

    pub(crate) fn focus_to_timeline(&mut self) {
        self.focus_timeline = true;
    }

    pub(crate) fn toggle_tab(&mut self, tab: Tab) {
        self.show_tab(tab, true);
    }

    pub(crate) fn apply_layout(&mut self, preset: LayoutPreset) {
        self.dock = Some(dock::preset_layout(preset));
        self.timeline_top = preset == LayoutPreset::TimelineTop;
        self.layout_slot = None;
    }

    /// View › Window Layouts › Save Layout, Ctrl+Alt+D and a number: the
    /// windows as they are now at `slot` of the menu, in its file (its
    /// folder, else the layouts' folder).
    pub(crate) fn save_layout(&mut self, slot: usize, name: String) {
        let folder = self
            .settings
            .layouts
            .get(slot)
            .and_then(|l| l.as_ref()?.file.as_ref()?.parent().map(PathBuf::from))
            .or_else(|| self.settings.layouts_folder());
        self.store_layout(slot, name, folder);
    }

    /// Save Layout As: named `name`, at `slot` of the menu, its file in
    /// `folder` (else the layouts' folder, which it becomes).
    pub(crate) fn save_layout_as(&mut self, slot: usize, name: String, folder: Option<PathBuf>) {
        if let Some(f) = &folder
            && Some(f) != self.settings.layouts_folder().as_ref()
        {
            self.settings.layout_folder = Some(f.clone());
        }
        let folder = folder.or_else(|| self.settings.layouts_folder());
        self.store_layout(slot, name, folder);
    }

    fn store_layout(&mut self, slot: usize, name: String, folder: Option<PathBuf>) {
        let Some(dock) = &self.dock else { return };
        let Ok(value) = serde_json::to_value(dock) else {
            self.report_error(tr("Could not store the layout."));
            return;
        };
        let mut layout = crate::settings::SavedLayout {
            name: name.clone(),
            dock: value,
            timeline_top: self.timeline_top,
            file: None,
        };
        if let Some(folder) = folder {
            let path = crate::layouts::path_for(&folder, &name);
            match crate::layouts::write(&path, &layout) {
                Ok(()) => layout.file = Some(path),
                Err(e) => self.report_error(trf!("Could not write {}: {}", path.display(), e)),
            }
        }
        if self.settings.layouts.len() < 10 {
            self.settings.layouts.resize(10, None);
        }
        self.settings.layouts[slot] = Some(layout);
        self.layout_slot = Some(slot);
        match self.settings.save() {
            Ok(()) => self.notify(trf!("Layout \"{}\" saved (Alt+D, {}).", name, slot)),
            Err(e) => self.report_error(trf!("Could not save the layout: {}", e)),
        }
    }

    /// Alt+D and a number: the menu's layout at `slot`.
    pub(crate) fn load_layout(&mut self, slot: usize) {
        let Some(Some(saved)) = self.settings.layouts.get(slot).cloned() else {
            return;
        };
        self.activate_layout(&saved, Some(slot));
    }

    /// Puts a layout's windows in place (`slot`: the menu's number it has,
    /// which becomes the layout in use).
    pub(crate) fn activate_layout(
        &mut self,
        layout: &crate::settings::SavedLayout,
        slot: Option<usize>,
    ) {
        match crate::layouts::dock_from_value(&layout.dock) {
            Ok(dock) => {
                self.dock = Some(dock);
                self.timeline_top = layout.timeline_top;
                self.layout_slot = slot;
            }
            Err(e) => self.report_error(trf!("Invalid layout: {}", e)),
        }
    }

    /// Reload Selected Layout: the layout in use as it was last saved (its
    /// file's, when it has one).
    pub(crate) fn reload_layout(&mut self) {
        let Some(slot) = self.layout_slot else { return };
        let Some(Some(mut saved)) = self.settings.layouts.get(slot).cloned() else {
            return;
        };
        if let Some(path) = saved.file.clone()
            && let Ok(fresh) = crate::layouts::read(&path)
        {
            saved = fresh;
            self.settings.layouts[slot] = Some(saved.clone());
        }
        self.activate_layout(&saved, Some(slot));
    }

    /// Organize Layouts' OK: the menu as it was left, the folder chosen.
    pub(crate) fn organized_layouts(
        &mut self,
        menu: Vec<Option<crate::settings::SavedLayout>>,
        folder: PathBuf,
    ) {
        // The layout in use stays marked where it went.
        let current = self
            .layout_slot
            .and_then(|n| self.settings.layouts.get(n)?.clone());
        self.layout_slot = current.and_then(|c| {
            menu.iter().position(|l| {
                l.as_ref()
                    .is_some_and(|l| l.name == c.name && l.file == c.file)
            })
        });
        self.settings.layouts = menu;
        if !folder.as_os_str().is_empty()
            && Some(&folder) != self.settings.layouts_folder().as_ref()
        {
            self.settings.layout_folder = Some(folder);
        }
        if let Err(e) = self.settings.save() {
            self.report_error(trf!("Could not save the layout: {}", e));
        }
    }

    /// Tools › Build Dynamic RAM Preview: the time selection's frames are
    /// drawn into memory at the preview's size, then played in a loop.
    pub(crate) fn build_ram_preview(&mut self) {
        let Some(range) = self
            .project()
            .loop_region
            .filter(|r| r.duration > bv_core::Tick::ZERO)
        else {
            self.notify(tr(
                "Make a time selection to build its Dynamic RAM Preview.",
            ));
            return;
        };
        if self.settings.prefs.ram_preview_mb == 0 {
            self.notify(tr(
                "The Dynamic RAM Preview has no memory (Options › Preferences › Video).",
            ));
            return;
        }
        self.stop();
        let (w, h) = self.preview.size();
        self.engine.build_ram_preview(range, w, h);
        self.building_ram_preview = true;
    }

    /// While the Dynamic RAM Preview builds: its progress; built, it plays.
    pub(crate) fn follow_ram_preview(&mut self) {
        if !self.building_ram_preview {
            return;
        }
        match self.engine.ram_preview_progress() {
            Some(bv_engine::BuildProgress::Building(done, total)) => {
                self.notify(trf!(
                    "Building Dynamic RAM Preview: {} of {} frames",
                    done,
                    total
                ));
            }
            Some(bv_engine::BuildProgress::Done(done, total)) => {
                self.building_ram_preview = false;
                if done < total {
                    self.notify(trf!(
                        "The Dynamic RAM Preview holds {} of {} frames (Options › Preferences › Video: Dynamic RAM Preview max).",
                        done,
                        total
                    ));
                } else {
                    self.notify(tr("Dynamic RAM Preview built."));
                }
                if let Some(r) = self.project().loop_region {
                    self.loop_enabled = true;
                    self.seek(r.start);
                    self.toggle_play();
                }
            }
            None => self.building_ram_preview = false,
        }
    }

    pub(crate) fn rebuild_peaks(&mut self) {
        for m in &self.project().media {
            if let bv_timeline::MediaSource::File { path } = &m.source {
                bv_engine::Engine::delete_peak_files(path);
            }
        }
        self.engine.refresh_thumbnails();
        self.notify(tr("Recomputing audio peaks and thumbnails."));
    }

    /// Picks the tool and tells the user which one is active.
    pub(crate) fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
    }
}

/// Scales the video events `ids` to fill the project's frame: media of
/// another shape is cropped instead of showing bars beside or above it.
pub(crate) fn fill_frame(p: &mut bv_timeline::Project, ids: &[ClipId]) {
    let s = p.settings;
    for &id in ids {
        if p.clip_track(id).is_none_or(|t| t.kind != TrackKind::Video) {
            continue;
        }
        let Some(v) = p
            .clip(id)
            .and_then(|c| p.media(c.media))
            .and_then(|m| m.video)
        else {
            continue;
        };
        let par = v.pixel_aspect.ratio() / s.pixel_aspect.ratio();
        let sx = s.width as f32 / (v.width as f32 * par).max(1.0);
        let sy = s.height as f32 / (v.height as f32).max(1.0);
        let _ = p.set_clip_framing(
            id,
            &bv_timeline::Transform {
                scale: sx.max(sy) / sx.min(sy).max(1e-6),
                ..bv_timeline::Transform::default()
            },
        );
    }
}

/// The gain of `db` decibels.
fn bv_mixer_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Extra;

    /// BananaV opens as it was left, as VEGAS does: once started, the
    /// session its window keeps as it changes (a second later) brings back
    /// the layout and the bars; an app never started keeps none.
    #[test]
    fn the_window_opens_as_it_was_left() {
        let ctx = crate::egui::Context::default();
        let tabs = |app: &BananaApp| {
            let dock = app.dock.as_ref().unwrap();
            dock.iter_all_tabs().map(|(_, t)| *t).collect::<Vec<_>>()
        };
        let mut app = BananaApp::new(ctx.clone());
        app.apply_layout(LayoutPreset::AudioMixing);
        let before = std::time::Instant::now() - std::time::Duration::from_secs(2);
        app.session_checked = before;
        app.keep_session(&ctx);
        assert_eq!(app.settings.session, None, "not started: nothing kept");
        app.restore_session();
        app.apply_layout(LayoutPreset::SourceWindow);
        app.show_toolbar = false;
        app.show_status_bar = false;
        app.timeline.show_waveforms = false;
        app.timeline.show_video_bus = true;
        // Looked at again only a second after the last time.
        app.session_checked = std::time::Instant::now();
        app.keep_session(&ctx);
        assert_eq!(app.settings.session, None);
        app.session_checked = before;
        app.keep_session(&ctx);
        let session = app.settings.session.clone().expect("the session kept");
        assert!(session.hide_toolbar && session.hide_status_bar);
        let mut next = BananaApp::new(ctx.clone());
        assert!(next.show_toolbar && next.show_status_bar);
        next.settings.session = Some(session);
        next.restore_session();
        assert_eq!(tabs(&next), tabs(&app));
        assert!(
            next.dock
                .as_ref()
                .unwrap()
                .find_tab(&Tab::Trimmer)
                .is_some()
        );
        assert!(!next.show_toolbar && !next.show_status_bar);
        // The View menu's switches of the timeline too.
        assert!(!next.timeline.show_waveforms && next.timeline.show_video_bus);
    }

    /// Sync links: an event linked to another moves when that one moves
    /// (in the same undo), not the other way round; moved alone, it keeps
    /// its place and its new offset.
    #[test]
    fn sync_links_follow_one_way() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.load_demo();
        let p = app.project().clone();
        let last = |kind| {
            p.tracks
                .iter()
                .filter(|t| t.kind == kind)
                .find_map(|t| t.clips.iter().max_by_key(|c| c.start))
                .cloned()
                .unwrap()
        };
        let (a, b) = (last(TrackKind::Video), last(TrackKind::Audio));
        app.modify(|p| {
            if let Some(c) = p.clip_mut(b.id) {
                c.sync_link = Some(bv_timeline::SyncLink {
                    to: a.id,
                    offset: b.start - a.start,
                });
            }
        });
        app.keep_sync_links();
        let (one, two) = (Tick::from_seconds(1), Tick::from_seconds(2));
        // The event followed moves: the linked one too, in one undo.
        let _ = app.edit(|p| p.move_clip(a.id, a.start + one, None));
        app.keep_sync_links();
        assert_eq!(app.project().clip(b.id).unwrap().start, b.start + one);
        app.undo();
        app.keep_sync_links();
        assert_eq!(app.project().clip(a.id).unwrap().start, a.start);
        assert_eq!(app.project().clip(b.id).unwrap().start, b.start);
        // The linked one moved alone: the other stays, the offset follows.
        let _ = app.edit(|p| p.move_clip(b.id, b.start + two, None));
        app.keep_sync_links();
        assert_eq!(app.project().clip(a.id).unwrap().start, a.start);
        let link = app.project().clip(b.id).unwrap().sync_link.unwrap();
        assert_eq!(link.offset, b.start + two - a.start);
        // The event followed gone: the link goes.
        let _ = app.edit(|p| p.delete_clips(&[a.id], false));
        app.keep_sync_links();
        assert!(app.project().clip(b.id).unwrap().sync_link.is_none());
    }

    /// Expand Track Layers: the video track's events alternate between the
    /// A roll (the top third) and the B roll (the bottom third), the track
    /// twice as tall.
    #[test]
    fn expanded_track_layers_alternate_the_events() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.load_demo();
        let p = app.project().clone();
        let track = p
            .tracks
            .iter()
            .find(|t| t.kind == bv_timeline::TrackKind::Video)
            .unwrap();
        let tall = app.timeline.height_of(track);
        app.timeline.toggle_layers(track.id);
        app.timeline.follow_layers(&p);
        let mut starts: Vec<Tick> = track.clips.iter().map(|c| c.start).collect();
        starts.sort();
        let third = 1.0 / 3.0;
        assert_eq!(app.timeline.band(track.id, starts[0]), (0.0, third));
        assert_eq!(app.timeline.band(track.id, starts[1]), (2.0 * third, third));
        assert_eq!(app.timeline.band(track.id, starts[2]), (0.0, third));
        assert_eq!(app.timeline.height_of(track), tall * 2.0);
        // Again: collapsed.
        app.timeline.toggle_layers(track.id);
        app.timeline.follow_layers(&p);
        assert_eq!(app.timeline.band(track.id, starts[1]), (0.0, 1.0));
    }

    #[test]
    fn edges_are_trimmed_with_a_red_bracket_and_expanded_on_two_rolls() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.load_demo();
        let s = Tick::from_seconds;
        // The sunset, from 4 s to 10 s.
        let g = app.primary_clip().expect("selected");
        // ]: its end to trim, the cursor there.
        app.select_event_edge(false);
        let e = app.timeline.edge_trim.expect("trimming");
        assert_eq!((e.clip, e.end, e.expanded), (g, true, false));
        assert_eq!(app.cursor, s(10));
        // The nudge keys trim the edge now.
        let frame = app.frame_rate().frame_duration();
        app.run_extra(Extra::Nudge {
            frames: true,
            dir: -1,
        });
        assert_eq!(app.project().clip(g).unwrap().end(), s(10) - frame);
        assert_eq!(app.cursor, s(10) - frame);
        // F3: one frame more.
        app.run_extra(Extra::Jog(-1));
        assert_eq!(app.project().clip(g).unwrap().end(), s(10) - frame - frame);
        // ]: the track's next edge, the checkerboard's start.
        app.select_event_edge(false);
        let e = app.timeline.edge_trim.unwrap();
        assert!(!e.end && e.clip != g);
        assert_eq!(app.cursor, s(10));
        // [: back to the sunset's end.
        app.select_event_edge(true);
        assert_eq!(
            app.timeline.edge_trim.map(|e| (e.clip, e.end)),
            Some((g, true))
        );

        // Expanded Edit Mode: the events up to the edit point on the A roll,
        // the ones after it on the B roll, the track twice as tall.
        let track = e.track;
        let tall = app.timeline.height_of(app.project().track(track).unwrap());
        app.toggle_expanded_edit();
        assert!(app.timeline.edge_trim.is_some_and(|e| e.expanded));
        assert_eq!(app.timeline.roll(track, Tick::ZERO), Some(false));
        assert_eq!(app.timeline.roll(track, s(4)), Some(false));
        assert_eq!(app.timeline.roll(track, s(10)), Some(true));
        let other = app.project().tracks[1].id;
        assert_eq!(app.timeline.roll(other, s(10)), None);
        let p = app.project().clone();
        assert_eq!(app.timeline.height_of(p.track(track).unwrap()), tall * 2.0);
        // The events on either side.
        let (a, b) = crate::timeline::edit_point_events(&p, app.timeline.edge_trim.unwrap());
        assert_eq!(a, Some(g));
        assert_eq!(b.and_then(|id| p.clip(id)).map(|c| c.start), Some(s(10)));
        // Again: out of it.
        app.toggle_expanded_edit();
        assert!(app.timeline.edge_trim.is_none());
        assert_eq!(app.timeline.height_of(p.track(track).unwrap()), tall);
    }

    /// A project with a 10 s file of video and audio (a 640 × 480 picture)
    /// at 0 s: its two events, grouped.
    fn av_project(app: &mut BananaApp) -> (bv_timeline::MediaId, ClipId, ClipId) {
        use bv_timeline::{AudioStreamInfo, MediaInfo, MediaSource, VideoStreamInfo};
        let mut p = bv_timeline::Project::default();
        let (v, a) = (p.tracks[0].id, p.tracks[1].id);
        let mut info = MediaInfo::generated(bv_timeline::Generator::ColorBars, &p.settings);
        info.source = MediaSource::File {
            path: "camera.mp4".into(),
        };
        info.duration = Tick::from_seconds(10);
        info.video = Some(VideoStreamInfo {
            width: 640,
            height: 480,
            frame_rate: p.settings.frame_rate,
            pixel_aspect: Default::default(),
            field_order: Default::default(),
            spherical: false,
            full_range: false,
            reading: Default::default(),
        });
        info.audio = Some(AudioStreamInfo {
            sample_rate: p.settings.sample_rate,
            channels: 2,
        });
        let m = p.add_media(info);
        let ids = p.insert_media(m, Tick::ZERO, Some(v), Some(a)).unwrap();
        app.history.reset(p);
        (m, ids[0], ids[1])
    }

    #[test]
    fn new_events_follow_the_editing_preferences() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        let (m, ..) = av_project(&mut app);
        let prefs = &mut app.settings.prefs;
        (prefs.loop_events, prefs.preserve_pitch, prefs.crop_video) = (false, true, true);
        prefs.match_first_media = false;
        app.place_media(m, Tick::from_seconds(20), None);
        let p = app.project();
        let placed: Vec<_> = p
            .clips()
            .filter(|(_, c)| c.start == Tick::from_seconds(20))
            .map(|(t, c)| (t.kind, c.clone()))
            .collect();
        assert_eq!(placed.len(), 2);
        for (kind, c) in &placed {
            assert!(!c.looped && c.preserve_pitch, "{kind:?}");
            if *kind == TrackKind::Video {
                // 4:3 in a 16:9 frame: enlarged to fill its width.
                let s = p.settings;
                let fill = (s.width as f32 / 640.0) / (s.height as f32 / 480.0);
                let scale = p.clip_transform_at(c.id, c.start).unwrap().scale;
                assert!((scale - fill).abs() < 1e-4, "{scale}");
            }
        }
        // A still image lasts as long as asked.
        let mut still = p.media(m).unwrap().clone();
        (still.still, still.audio) = (true, None);
        still.duration = bv_timeline::Generator::DURATION;
        app.settings.prefs.still_length = 3.0;
        let id = app.modify(|p| p.add_media(still));
        app.place_media(id, Tick::from_seconds(40), None);
        let c = app
            .project()
            .clips()
            .find(|(_, c)| c.media == id)
            .map(|(_, c)| c.duration);
        assert_eq!(c, Some(Tick::from_seconds(3)));
    }

    #[test]
    fn grouped_events_cut_and_deleted_as_the_preference_says() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        let (_, video, audio) = av_project(&mut app);
        // Only the video selected (its audio taken out with Ctrl+click).
        app.settings.prefs.group_edits = false;
        app.selection = [video].into_iter().collect();
        app.copy();
        assert_eq!(app.clipboard.as_ref().map(|c| c.len()), Some(1));
        app.delete_selection(false);
        assert!(app.project().clip(video).is_none() && app.project().clip(audio).is_some());
        let (_, video, audio) = av_project(&mut app);
        app.settings.prefs.group_edits = true;
        app.selection = [video].into_iter().collect();
        app.delete_selection(false);
        assert!(
            app.project().clip(audio).is_none(),
            "the group goes with it"
        );
    }

    #[test]
    fn smart_split_overlaps_by_its_frames() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        let (_, video, _) = av_project(&mut app);
        app.settings.prefs.smart_split_frames = 10;
        let s = Tick::from_seconds;
        app.set_loop_region(Some(bv_core::TimeRange::new(s(4), s(2))));
        app.smart_split();
        let frame = app.frame_rate().frame_duration();
        let left = app.project().clip(video).unwrap().clone();
        let right = app
            .project()
            .clips()
            .find(|(t, c)| t.kind == TrackKind::Video && c.id != video)
            .map(|(_, c)| c.clone())
            .unwrap();
        assert_eq!(left.end(), s(4));
        assert_eq!(right.start, s(4) - app.frame_rate().frame_start(10));
        assert!(right.start < left.end() && left.end() - right.start >= frame);
        assert!(left.group.is_some() && left.group == right.group);
        // Warp Flow between the video parts, as VEGAS's.
        assert_eq!(
            right.transition_fx.as_ref().map(|f| f.id.as_str()),
            Some(bv_engine::plugins::WARP_FLOW_ID)
        );
    }

    #[test]
    fn a_transition_on_a_cut_overlaps_the_events() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        let (_, video, _) = av_project(&mut app);
        let s = Tick::from_seconds;
        // Cut at 5 s: the part after it from 5 s of the media.
        let right = app
            .edit(|p| p.split(video, s(5)))
            .and_then(|ids| ids.first().copied())
            .unwrap();
        app.settings.prefs.cut_overlap = 1.0;
        app.settings.prefs.cut_overlap_align = crate::settings::CutAlign::AfterCut;
        crate::tabs::transitions::apply(&mut app, &[right], bv_timeline::Transition::Iris);
        let p = app.project();
        assert_eq!(
            p.clip(video).unwrap().end(),
            s(6),
            "the first grew past the cut"
        );
        assert_eq!(p.clip(right).unwrap().start, s(5));
        assert_eq!(
            p.clip(right).unwrap().transition,
            bv_timeline::Transition::Iris
        );
    }

    #[test]
    fn the_cursor_preview_plays_around_the_cursor() {
        let s = Tick::from_seconds;
        let r = crate::app::preview_range(s(5), 2.0);
        assert_eq!((r.start, r.end()), (s(4), s(6)));
        // Near the start it does not go before it.
        let r = crate::app::preview_range(Tick::ZERO, 2.0);
        assert_eq!((r.start, r.end()), (Tick::ZERO, s(1)));
    }
}

/// What an Auto Normalize gain was worked out for: the event's part of its
/// media (and how it plays it) and the target level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NormalizedFor {
    media: bv_timeline::MediaId,
    stream: u16,
    from: Tick,
    length: Tick,
    rate: f64,
    looped: bool,
    target: f32,
}

impl NormalizedFor {
    fn of(c: &bv_timeline::Clip, target: f32) -> NormalizedFor {
        NormalizedFor {
            media: c.media,
            stream: c.audio_stream,
            from: c.source_in,
            length: c.duration,
            rate: c.rate,
            looped: c.looped,
            target,
        }
    }
}
