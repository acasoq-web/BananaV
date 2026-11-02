//! Keyboard-only actions of the keyboard map (they have no menu item).

use std::collections::HashSet;

use bv_core::Tick;
use bv_timeline::{ClipId, EditError, TrackId, TrackKind};

use super::BananaApp;
use crate::dock::Tab;
use crate::i18n::tr;
use crate::keymap::{Extra, TrackMove};

impl BananaApp {
    /// Ctrl+F11: the track headers folded to their color bars, or back to
    /// their width (`Some(fold)`: that way, whatever they are).
    pub(crate) fn fold_track_headers(&mut self, fold: Option<bool>) {
        use crate::timeline::{HEADER_W, MAX_HEADER_W, MIN_HEADER_W};
        let now = self.settings.track_list_width;
        let folded = now <= MIN_HEADER_W + 30.0;
        if fold.unwrap_or(!folded) == folded {
            return;
        }
        self.settings.track_list_width = if folded {
            self.timeline.full_header_w.max(HEADER_W.min(MAX_HEADER_W))
        } else {
            self.timeline.full_header_w = now;
            MIN_HEADER_W
        };
    }

    /// = and − (Ctrl: a cent, Shift: an octave; Ctrl+Shift: back to none):
    /// the selected audio events' pitch changed (one undo), within four
    /// octaves either way.
    pub(crate) fn shift_pitch(&mut self, cents: Option<i16>) {
        let ids: Vec<ClipId> = self.selection.iter().copied().collect();
        let p = self.project();
        let audio: Vec<ClipId> = ids
            .into_iter()
            .filter(|&id| p.clip_track(id).is_some_and(|t| t.kind == TrackKind::Audio))
            .collect();
        if audio.is_empty() {
            return;
        }
        self.modify(|p| {
            for id in audio {
                if let Some(c) = p.clip_mut(id) {
                    c.pitch = match cents {
                        Some(d) => {
                            let v = (f32::from(d) / 100.0 + c.pitch).clamp(-48.0, 48.0);
                            (v * 100.0).round() / 100.0
                        }
                        None => 0.0,
                    };
                }
            }
        });
    }

    /// The track keyboard track commands act on: the selected track, else
    /// the selected event's track, else the first one.
    pub(crate) fn focused_track(&self) -> Option<TrackId> {
        let p = self.project();
        self.selected_track
            .and_then(|id| p.track(id))
            .or_else(|| self.primary_clip().and_then(|id| p.clip_track(id)))
            .or_else(|| p.tracks.first())
            .map(|t| t.id)
    }

    /// The selected track, when the track list has the keys.
    pub(crate) fn track_list_selection(&self) -> Option<TrackId> {
        (self.key_focus == crate::keymap::Context::TrackList)
            .then_some(self.selected_track)
            .flatten()
            .filter(|&id| self.project().track(id).is_some())
    }

    /// Removes tracks (one undo) and selects the track where the first of
    /// them was.
    pub(crate) fn delete_tracks(&mut self, ids: &[TrackId]) {
        let p = self.project();
        let Some(i) = ids.iter().filter_map(|&id| p.track_index(id)).min() else {
            return;
        };
        let removed = self.edit(|p| -> Result<(), bv_timeline::EditError> {
            for &id in ids {
                p.remove_track(id)?;
            }
            Ok(())
        });
        if removed.is_none() {
            return;
        }
        self.prune_selection();
        let p = self.project();
        self.selected_track = p
            .tracks
            .get(i.min(p.tracks.len().saturating_sub(1)))
            .map(|t| t.id);
        self.track_picks = self.selected_track.into_iter().collect();
    }

    /// Moves the track selection like VEGAS's track list cursor and scrolls
    /// the timeline to show the track.
    fn move_track_cursor(&mut self, to: TrackMove) {
        let p = self.project();
        let n = p.tracks.len();
        if n == 0 {
            return;
        }
        let current = self
            .selected_track
            .and_then(|id| p.track_index(id))
            .or_else(|| self.focused_track().and_then(|id| p.track_index(id)));
        let page = self.timeline.tracks_per_page().max(1);
        let last = n - 1;
        let target = match (to, current) {
            (TrackMove::Up, Some(i)) => i.saturating_sub(1),
            (TrackMove::Down, Some(i)) => (i + 1).min(last),
            (TrackMove::PageUp, Some(i)) => i.saturating_sub(page),
            (TrackMove::PageDown, Some(i)) => (i + page).min(last),
            (TrackMove::Up | TrackMove::PageUp | TrackMove::First, _) => 0,
            (TrackMove::Down | TrackMove::PageDown | TrackMove::Last, _) => last,
            // 1–9 are tracks 1–9; 0 is track 10.
            (TrackMove::Number(k), _) => {
                let i = if k == 0 { 9 } else { k as usize - 1 };
                if i > last {
                    return;
                }
                i
            }
        };
        let id = p.tracks[target].id;
        self.selected_track = Some(id);
        let project = self.project().clone();
        self.timeline.reveal_track(&project, id);
    }

    pub(crate) fn run_extra(&mut self, e: Extra) {
        match e {
            Extra::GoToMarker(n) => {
                let name = n.to_string();
                let t = self
                    .project()
                    .markers
                    .iter()
                    .find(|m| m.name == name)
                    .map(|m| m.time);
                match t {
                    Some(t) => self.seek_playback(t),
                    None => self.notify(trf!("There is no marker {}.", n)),
                }
            }
            Extra::CdStep { indices, dir } => self.cd_step(indices, dir),
            Extra::Details(op) => {
                let pasted = self.pasted_text.take();
                crate::tabs::details::run(self, op, pasted);
            }
            Extra::SelectNone => self.selection.clear(),
            Extra::OpenInTrimmer => {
                if let Some(m) = self
                    .primary_clip()
                    .and_then(|id| self.project().clip(id))
                    .map(|c| c.media)
                {
                    self.trimmer.open(m);
                    self.show_tab(Tab::Trimmer, false);
                }
            }
            Extra::SelectInMediaPool => {
                if let Some(m) = self
                    .primary_clip()
                    .and_then(|id| self.project().clip(id))
                    .map(|c| c.media)
                {
                    self.selected_media = Some(m);
                    self.show_tab(Tab::ProjectMedia, false);
                }
            }
            Extra::Nudge { frames, dir } => self.nudge(frames, dir),
            Extra::Jog(dir) => {
                let step = Tick(self.frame_rate().frame_duration().0 * dir as i64);
                if self.timeline.edge_trim.is_some() {
                    self.trim_edit_point(step);
                } else {
                    let rate = self.frame_rate();
                    let t = rate.snap_floor((self.cursor + step).max(Tick::ZERO));
                    self.seek(t);
                }
            }
            Extra::Refresh => self.engine.refresh_thumbnails(),
            Extra::TrackMute { exclusive } => self.toggle_track_flag(exclusive, false),
            Extra::TrackSolo { exclusive } => self.toggle_track_flag(exclusive, true),
            Extra::TrackVolume(step) => {
                if let Some(t) = self.focused_track() {
                    let k = 10f32.powf(step as f32 / 20.0);
                    self.modify(|p| {
                        if let Some(tr) = p.track_by_id_mut(t) {
                            tr.level = match tr.kind {
                                // Video: composite level in 1 % steps.
                                TrackKind::Video => (tr.level + step as f32 * 0.01).clamp(0.0, 1.0),
                                // Audio: 1 dB steps, up to +12 dB.
                                TrackKind::Audio => (tr.level.max(0.001) * k).min(4.0),
                            };
                        }
                    });
                }
            }
            Extra::TrackPan(dir) => self.set_track_pan(|pan| pan + dir as f32 * 0.02),
            Extra::Pitch(cents) => self.shift_pitch(cents),
            Extra::TrackPanPosition(n) => {
                let to = match (n.max(1) - 1) % 3 {
                    0 => -1.0,
                    1 => 0.0,
                    _ => 1.0,
                };
                self.set_track_pan(|_| to);
            }
            Extra::TrackCursor(to) => self.move_track_cursor(to),
            Extra::Trimmer(op) => crate::tabs::trimmer::run(self, op),
            Extra::Console(op) => crate::tabs::console::run(self, op),
            // The Track Motion window takes them when it draws.
            Extra::TrackMotion(k) => self.track_motion_keys.push(k),
            Extra::DetectScenes => {
                if let Some(id) = self.primary_clip() {
                    self.clip_windows.scenes = Some((id, 0.5, false));
                }
            }
            Extra::DeleteTrack => {
                if let Some(id) = self.selected_track {
                    self.request_delete_track(id);
                }
            }
            Extra::SaveLayout(slot) => {
                let name = self
                    .settings
                    .layouts
                    .get(slot)
                    .and_then(|l| l.as_ref())
                    .map_or_else(|| format!("Layout {slot}"), |l| l.name.clone());
                self.save_layout(slot, name);
            }
            Extra::RenameTrack => {
                if let Some(id) = self.focused_track() {
                    self.selected_track = Some(id);
                    let name = self
                        .project()
                        .track(id)
                        .map(crate::timeline::scribble_name)
                        .unwrap_or_default();
                    self.timeline.renaming = Some((id, name));
                }
            }
            Extra::ToggleMinimizeTracks => self.timeline.compact_tracks ^= true,
            Extra::DefaultTrackHeights => self.timeline.default_track_heights(),
            Extra::Play => {
                if !self.is_playing() {
                    self.toggle_play();
                }
            }
            // F12 does what Space does (Options › Preferences › General).
            Extra::PlayToggle => self.space_bar(),
            Extra::Pause => self.toggle_pause(),
            Extra::Stop => self.stop(),
            Extra::PlayFromStart => self.play_from_start(),
            Extra::StepForward => self.step_frames(1),
            Extra::StepBack => self.step_frames(-1),
            // L plays; pressing it again doubles the speed (up to 4×).
            // L: plays, then faster at each press; how fast, Options ›
            // Preferences › Editing: "JKL / shuttle speed".
            Extra::ScrubRight => {
                // From shuttling backwards: playing on.
                self.scrub_rate = 0.0;
                let speed = self.settings.prefs.jkl_speed;
                if self.is_playing() {
                    self.play_rate = speed.next_rate(Some(self.play_rate));
                    self.restart_if_playing();
                } else {
                    self.play_rate = speed.next_rate(None);
                    self.toggle_play();
                }
            }
            // K pauses and resets the speed.
            Extra::ScrubPause => {
                if self.is_playing() {
                    self.toggle_pause();
                }
                self.play_rate = 1.0;
                self.scrub_rate = 0.0;
            }
            // J: backwards, faster at each press.
            Extra::ScrubLeft => {
                if self.is_playing() {
                    self.stop();
                }
                let speed = self.settings.prefs.jkl_speed;
                let now = (self.scrub_rate < 0.0).then_some(-self.scrub_rate);
                self.scrub_rate = -speed.next_rate(now);
            }
            Extra::PlayReverse => {
                if self.is_playing() {
                    self.stop();
                }
                self.scrub_rate = -1.0;
            }
            Extra::Rewind | Extra::FastForward => {
                if self.is_playing() {
                    self.stop();
                }
                let dir = if e == Extra::Rewind { -1.0 } else { 1.0 };
                self.scrub_rate = dir * 4.0;
            }
            Extra::TypeSelection(field) => {
                let rate = self.frame_rate();
                let r = self.project().loop_region;
                let t = match field {
                    crate::menu_dialogs::GoToField::SelectionLength => {
                        r.map_or(Tick::ZERO, |r| r.duration)
                    }
                    _ => r.map_or(self.cursor, |r| r.end()),
                };
                let text = bv_core::timecode::format(t, rate);
                self.menu_dialogs.open_go_to_field(text, field);
            }
            Extra::SwitchFocus => {
                use crate::keymap::Context;
                self.key_focus = match self.key_focus {
                    Context::TrackList => Context::TrackView,
                    _ => Context::TrackList,
                };
                if self.key_focus == Context::TrackList && self.selected_track.is_none() {
                    self.selected_track = self.focused_track();
                }
            }
            Extra::MoveToTrack(dir) => self.move_to_track(dir),
            Extra::DuplicateTrack => {
                if let Some(t) = self.focused_track() {
                    self.duplicate_track(t);
                }
            }
            Extra::DockArea(area) => {
                self.dock_request = Some(match area {
                    Some(a) => crate::dock::DockRequest::Area(a),
                    None => crate::dock::DockRequest::All,
                });
            }
            Extra::FoldTrackHeaders => self.fold_track_headers(None),
            Extra::MaximizeTimeline => {
                let on = !self.timeline_maximized;
                self.timeline_maximized = on;
                // Every area hidden (or back), and the headers with them.
                if on != self.docks_hidden.is_some() {
                    self.dock_request = Some(crate::dock::DockRequest::All);
                }
                self.fold_track_headers(Some(on));
            }
            Extra::AllTrackHeights(dir) => {
                let k = if dir > 0 { 1.25 } else { 0.8 };
                // The bus area in focus: its tracks.
                if self.timeline.bus_focus && self.timeline.show_bus_tracks {
                    let project = self.project().clone();
                    self.timeline.scale_bus_heights(&project, k);
                } else {
                    self.scale_track_heights(|h| h * k);
                }
            }
            Extra::TrackHeightBy1(dir) => self.scale_track_heights(|h| h + dir as f32),
            Extra::TrackHeightMax => {
                if let Some(t) = self.focused_track() {
                    let project = self.project().clone();
                    self.timeline.maximize_track(&project, t);
                }
            }
            Extra::SwapCursor => {
                if let Some(r) = self.project().loop_region {
                    let to = if self.cursor == r.start {
                        r.end()
                    } else {
                        r.start
                    };
                    self.seek(to);
                }
            }
            Extra::CutToTransition => self.cut_to_transition(),
            Extra::TransitionToCut => self.transition_to_cut(),
            Extra::PreviewZoom(dir) => self.preview.zoom_by(5 * dir as i32),
            Extra::PreviewZoomDefault => self.preview.zoom_by(0),
            Extra::PreviewBypassZoom => self.preview.toggle_bypass_zoom(),
        }
    }

    /// Every track's height changed by `f` (within the tracks' limits).
    fn scale_track_heights(&mut self, f: impl Fn(f32) -> f32) {
        use crate::timeline::{MAX_TRACK_H, MIN_TRACK_H};
        let tracks: Vec<_> = self.project().tracks.clone();
        for t in &tracks {
            let h = f(self.timeline.height_of(t)).clamp(MIN_TRACK_H, MAX_TRACK_H);
            self.timeline.track_heights.insert(t.id, h);
        }
    }

    /// Num 8/2: each selected event to the nearest track of its kind above
    /// (−1) or below (+1), where it stays at its time.
    fn move_to_track(&mut self, dir: i8) {
        let ids = self.selected();
        self.edit(|p| -> Result<(), EditError> {
            for id in ids {
                let (Some(track), Some(start)) = (
                    p.clip_track(id).map(|t| (t.id, t.kind)),
                    p.clip(id).map(|c| c.start),
                ) else {
                    continue;
                };
                let Some(i) = p.track_index(track.0) else {
                    continue;
                };
                let target = if dir < 0 {
                    p.tracks[..i].iter().rev().find(|t| t.kind == track.1)
                } else {
                    p.tracks[i + 1..].iter().find(|t| t.kind == track.1)
                }
                .map(|t| t.id);
                if let Some(t) = target {
                    p.move_clip(id, start, Some(t))?;
                }
            }
            Ok(())
        });
    }

    /// The event of the focused track starting at the cut nearest the cursor
    /// (within two frames), with the one before it ending there.
    fn cut_at_cursor(&self) -> Option<(ClipId, ClipId)> {
        let p = self.project();
        let track = p.track(self.focused_track()?)?;
        let reach = Tick(self.frame_rate().frame_duration().0 * 2);
        let after = track
            .clips
            .iter()
            .filter(|c| (c.start - self.cursor).0.abs() <= reach.0)
            .min_by_key(|c| (c.start - self.cursor).0.abs())?;
        let before = track
            .clips
            .iter()
            .filter(|c| c.id != after.id && c.start < after.start && c.end() >= after.start)
            .max_by_key(|c| c.start)?;
        Some((before.id, after.id))
    }

    /// Num / * −: the cut at the cursor made a crossfade (as long as
    /// Options › Preferences › Editing's cut-to-overlap conversion says).
    fn cut_to_transition(&mut self) {
        match self.cut_at_cursor() {
            Some((_, after)) => {
                crate::tabs::transitions::apply(self, &[after], bv_timeline::Transition::default())
            }
            None => self.notify(tr("No cut at the cursor on the selected track.")),
        }
    }

    /// Ctrl+Num /: the crossfade at the cursor made a cut, at its middle.
    fn transition_to_cut(&mut self) {
        let p = self.project();
        let Some(track) = self.focused_track().and_then(|t| p.track(t)) else {
            return;
        };
        // The event the cursor is in that overlaps the one before it.
        let pair = track.clips.iter().find_map(|after| {
            let before = track
                .clips
                .iter()
                .filter(|c| c.id != after.id && c.start < after.start && c.end() > after.start)
                .max_by_key(|c| c.start)?;
            let overlap = (after.start, before.end());
            (self.cursor >= overlap.0 && self.cursor <= overlap.1)
                .then_some((before.id, after.id, overlap))
        });
        let Some((before, after, (a, b))) = pair else {
            self.notify(tr("No crossfade at the cursor on the selected track."));
            return;
        };
        let mid = self.frame_rate().snap_nearest(Tick((a.0 + b.0) / 2));
        self.edit(|p| -> Result<(), EditError> {
            p.trim_end(before, mid)?;
            p.trim_start(after, mid)?;
            Ok(())
        });
    }

    /// Sets the pan of the focused audio track (video tracks have none).
    fn set_track_pan(&mut self, f: impl Fn(f32) -> f32) {
        let Some(id) = self.focused_track() else {
            return;
        };
        if self
            .project()
            .track(id)
            .is_none_or(|t| t.kind != TrackKind::Audio)
        {
            return;
        }
        self.modify(|p| {
            if let Some(t) = p.track_by_id_mut(id) {
                t.pan = f(t.pan).clamp(-1.0, 1.0);
            }
        });
    }

    /// Toggles mute (or solo) of the focused track; `exclusive` turns it off
    /// on every other track.
    fn toggle_track_flag(&mut self, exclusive: bool, solo: bool) {
        let Some(id) = self.focused_track() else {
            return;
        };
        self.modify(|p| {
            let on = !p
                .track(id)
                .is_some_and(|t| if solo { t.solo } else { t.muted });
            for t in &mut p.tracks {
                let flag = if solo { &mut t.solo } else { &mut t.muted };
                if t.id == id {
                    *flag = on;
                } else if exclusive {
                    *flag = false;
                }
            }
        });
    }

    /// Moves the selected events (each group once) by a frame or a pixel.
    fn nudge(&mut self, frames: bool, dir: i8) {
        let step = if frames {
            self.frame_rate().frame_duration()
        } else {
            Tick(self.timeline.ticks_per_px.round().max(1.0) as i64)
        };
        let delta = Tick(step.0 * dir as i64);
        // Trimming an edge: the keys trim it.
        if self.timeline.edge_trim.is_some() {
            self.trim_edit_point(delta);
            return;
        }
        let ids = self.selected();
        self.edit(|p| -> Result<(), EditError> {
            let mut done: HashSet<ClipId> = HashSet::new();
            for id in ids {
                if done.contains(&id) {
                    continue;
                }
                done.extend(p.group_members(id));
                if let Some(start) = p.clip(id).map(|c| c.start) {
                    p.move_clip(id, start + delta, None)?;
                }
            }
            Ok(())
        });
    }
}
