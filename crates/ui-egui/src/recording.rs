//! Recording audio (the transport's Record, Ctrl+R): into the armed audio
//! tracks, each from its Record Input, while the project plays. When it
//! stops, the Recorded Files window lists the takes (Options › Preferences
//! › General: "Prompt to keep files after recording") and the ones kept
//! become events where the recording started — as VEGAS's Help describes
//! it ("Recording audio"): with Loop Playback each lap round the loop
//! region is a take of an event over it (regions in the file mark them),
//! and selected events on the armed tracks are punched into, a take for
//! each pass the recording made through them, the last one active. A
//! track recording from an input bus records its input through the bus's
//! effects.

use std::path::{Path, PathBuf};
use std::time::Instant;

use bv_core::{Tick, TimeRange};
use bv_engine::{InputDevice, RecordTake};
use bv_timeline::{ClipId, RecordInput, TrackId, TrackKind};

use crate::app::BananaApp;
use crate::egui::{self, RichText};
use crate::i18n::tr;

/// A recording going on: where it started and into which tracks, the
/// loop it goes round (Loop Playback) and the selected events of armed
/// tracks it punches into.
#[derive(Clone, Debug, Default)]
pub(crate) struct Recording {
    pub start: Tick,
    pub tracks: Vec<TrackId>,
    pub looping: Option<TimeRange>,
    pub punch: Vec<(TrackId, ClipId, TimeRange)>,
}

/// When (from a recording's start) playback reached timeline time `at`,
/// within `elapsed` of recording: on its way from `start`, then on each
/// lap round `looping` when `at` is in it.
pub(crate) fn passes(
    start: Tick,
    looping: Option<TimeRange>,
    at: Tick,
    elapsed: Tick,
) -> Vec<Tick> {
    let mut out = Vec::new();
    match looping.filter(|l| l.duration > Tick::ZERO && start < l.end()) {
        Some(l) => {
            if at >= start && at < l.end() {
                out.push(at - start);
            }
            if at >= l.start && at < l.end() {
                let mut e = (l.end() - start) + (at - l.start);
                while e <= elapsed {
                    out.push(e);
                    e += l.duration;
                }
            }
        }
        None if at >= start => out.push(at - start),
        None => {}
    }
    out.retain(|e| *e <= elapsed);
    out
}

/// Live routes' keys of input busses (a track's is its id), and of an
/// input bus's sends (its id, then its chain's).
pub(crate) const INPUT_BUS_KEY: u64 = 1 << 48;
pub(crate) const INPUT_SEND_KEY: u64 = 1 << 49;

/// A take recorded: its track, file and length, and how far from where
/// recording started its first sample was heard (the latency measured).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Take {
    pub track: TrackId,
    pub path: PathBuf,
    pub length: Tick,
    pub offset: Option<Tick>,
}

/// The Recorded Files window: the takes of the last recording.
#[derive(Debug)]
pub(crate) struct RecordedFiles {
    rec: Recording,
    files: Vec<Take>,
    selected: usize,
    /// A new name being typed for the selected file.
    renaming: Option<String>,
}

/// A free file name in `folder` for a take of `track` ("Audio 1 Recording
/// 1.wav", "… 2.wav"…), none of `taken` either.
fn take_path(folder: &Path, track: &str, taken: &[PathBuf]) -> PathBuf {
    let clean: String = track
        .chars()
        .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
        .collect();
    let clean = clean.trim();
    let base = if clean.is_empty() { "Audio" } else { clean };
    (1..)
        .map(|n| folder.join(format!("{base} Recording {n}.wav")))
        .find(|p| !p.exists() && !taken.contains(p))
        .expect("a free name")
}

/// How Record Input names an input: "Stereo", "Mono 1", "Stereo 3/4"…
pub(crate) fn input_label(input: &RecordInput) -> String {
    match (input.channels, input.first) {
        (1, 0) => tr("Mono Left").to_string(),
        (1, 1) => tr("Mono Right").to_string(),
        (1, n) => trf!("Mono {}", n + 1),
        (_, 0) => tr("Stereo").to_string(),
        (_, n) => trf!("Stereo {}/{}", n + 1, n + 2),
    }
}

impl BananaApp {
    /// Where recordings go: File › Properties › Audio's folder, or the
    /// Documents folder.
    pub(crate) fn recorded_folder(&self) -> PathBuf {
        let set = self.project().recorded_folder.trim();
        if set.is_empty() {
            crate::project_properties::default_recorded_folder()
        } else {
            PathBuf::from(set)
        }
    }

    /// The input devices, found again when older than a few seconds.
    pub(crate) fn input_devices(&mut self) -> Vec<InputDevice> {
        let stale = self
            .input_devices
            .as_ref()
            .is_none_or(|(at, _)| at.elapsed().as_secs() >= 5);
        if stale {
            self.input_devices = Some((Instant::now(), bv_engine::input_devices()));
        }
        self.input_devices
            .as_ref()
            .map(|(_, d)| d.clone())
            .unwrap_or_default()
    }

    /// The input tracks record from when they have none of their own
    /// (Options › Preferences › Audio Device: "Default audio recording
    /// device"; the system's default input with "Audio device type" so).
    pub(crate) fn default_record_input(&self) -> RecordInput {
        let p = &self.settings.prefs;
        let mut input = p.record_device.clone();
        if p.audio_device_type == crate::settings::AudioDeviceType::SystemDefault {
            input.device.clear();
        }
        input
    }

    /// No track armed: a new audio track, armed, records.
    pub(crate) fn record_into_new_track(&mut self) {
        let id = self.modify(|p| p.add_track(TrackKind::Audio));
        self.modify_untracked(move |p| {
            if let Some(t) = p.track_by_id_mut(id) {
                t.armed = true;
            }
        });
        self.selected_track = Some(id);
        self.start_recording();
    }

    /// Gives the engine Options › Preferences › Audio Device's settings
    /// (the output device, the buffering) and the metronome's sounds.
    pub(crate) fn apply_audio_prefs(&mut self) {
        let p = &self.settings.prefs;
        let device = match p.audio_device_type {
            crate::settings::AudioDeviceType::SystemDefault => None,
            crate::settings::AudioDeviceType::Windows => p.playback_device.clone(),
        };
        bv_engine::Engine::set_audio_buffering(
            p.playback_buffer,
            p.track_buffering.then_some(p.track_buffer),
        );
        let quick_fade = bv_core::Tick::from_seconds_f64(p.quick_fade_ms as f64 / 1000.0);
        let listen = p.prefader_sends_listen_to_mute;
        self.engine.set_audio_device(device);
        self.set_metronome(self.metronome);
        bv_engine::Engine::set_quick_fade(quick_fade);
        bv_engine::Engine::set_prefader_sends_listen_to_mute(listen);
    }

    /// Options › Metronome on or off: BananaV's clicks, or the sounds of
    /// Options › Preferences › Audio.
    pub(crate) fn set_metronome(&mut self, on: bool) {
        self.metronome = on;
        let p = &self.settings.prefs;
        let clicks = on.then(|| {
            if p.metronome_custom {
                let path =
                    |f: &Option<PathBuf>| f.as_ref().map(|f| f.to_string_lossy().to_string());
                bv_engine::Clicks::from_files(
                    path(&p.metronome_normal).as_deref(),
                    path(&p.metronome_accent).as_deref(),
                )
            } else {
                bv_engine::Clicks::standard()
            }
        });
        bv_engine::Engine::set_metronome(clicks);
    }

    /// Arm for Record on audio track `id`, or off.
    pub(crate) fn toggle_arm(&mut self, id: TrackId) {
        // Nothing arms during a real-time render.
        if self.engine.is_recording() || self.engine.is_real_time_rendering() {
            return;
        }
        self.modify_untracked(move |p| {
            if let Some(t) = p.track_by_id_mut(id)
                && t.kind == TrackKind::Audio
            {
                t.armed = !t.armed;
            }
        });
    }

    /// The track's Record Input.
    pub(crate) fn set_record_input(&mut self, id: TrackId, input: RecordInput) {
        self.modify_untracked(move |p| {
            if let Some(t) = p.track_by_id_mut(id) {
                t.record_input = Some(input.clone());
            }
        });
    }

    /// The transport's Record (Ctrl+R): starts recording into the armed
    /// tracks; while recording, stops.
    pub(crate) fn toggle_recording(&mut self) {
        if self.engine.is_recording() {
            self.finish_recording();
        } else {
            self.start_recording();
        }
    }

    /// Transport › Arm for Record and Set Recording Path (Ctrl+Alt+Shift+R):
    /// the folder recordings go to (File › Properties › Audio), then Record.
    pub(crate) fn set_path_and_record(&mut self) {
        if self.engine.is_recording() {
            self.finish_recording();
            return;
        }
        let picked = rfd::FileDialog::new()
            .set_title(tr("Recorded files folder"))
            .set_directory(self.recorded_folder())
            .pick_folder();
        let Some(folder) = picked else {
            return;
        };
        let folder = folder.to_string_lossy().to_string();
        self.modify(move |p| p.recorded_folder = folder);
        self.start_recording();
    }

    fn start_recording(&mut self) {
        if self.engine.is_real_time_rendering() {
            self.notify(tr("Nothing records during a real-time render."));
            return;
        }
        let default_input = self.default_record_input();
        let p = self.project();
        let armed: Vec<(TrackId, String, RecordInput)> = p
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio && t.armed)
            .map(|t| {
                (
                    t.id,
                    crate::i18n::track_name(t),
                    t.record_input
                        .clone()
                        .unwrap_or_else(|| default_input.clone()),
                )
            })
            .collect();
        if armed.is_empty() {
            // Options › Preferences › Audio: "Prompt user when no track has
            // been armed for record"; without it, a new track records.
            if self.settings.prefs.ask_no_armed_track {
                self.ask_user(crate::ask::Ask {
                    title: tr("Record").to_string(),
                    text: tr(
                        "No track is armed for recording. Add an audio track armed for recording and record into it?",
                    )
                    .to_string(),
                    what: crate::ask::Asked::RecordNewTrack,
                    check: Some((
                        tr("Always ask"),
                        |p: &mut crate::settings::Prefs| &mut p.ask_no_armed_track,
                    )),
                });
            } else {
                self.record_into_new_track();
            }
            return;
        }
        let folder = self.recorded_folder();
        if let Err(e) = std::fs::create_dir_all(&folder) {
            self.report_error(trf!("Could not record into {}: {}", folder.display(), e));
            return;
        }
        let from = self.cursor;
        let broadcast_wave = self.settings.prefs.record_bwf;
        let mut takes: Vec<RecordTake> = Vec::new();
        for (track, name, input) in &armed {
            let taken: Vec<PathBuf> = takes.iter().map(|t| t.path.clone()).collect();
            // An input bus: its input through its effects.
            let (input, effects) = self.project().resolved_input(input);
            takes.push(RecordTake {
                track: *track,
                input,
                path: take_path(&folder, name, &taken),
                start: from,
                broadcast_wave,
                effects,
            });
        }
        let bits = self.project().settings.bit_depth as u16;
        self.stop();
        self.engine.set_project(self.history.current());
        match self
            .engine
            .start_recording(takes, bits, from, self.play_options())
        {
            Ok(()) => {
                self.playhead = Some(from);
                let tracks: Vec<TrackId> = armed.iter().map(|(t, _, _)| *t).collect();
                // The selected events of the armed tracks: punched into.
                let p = self.project();
                let punch = self
                    .selection
                    .iter()
                    .filter_map(|&c| {
                        let t = p.clip_track(c)?;
                        let clip = p.clip(c)?;
                        tracks.contains(&t.id).then_some((t.id, c, clip.range()))
                    })
                    .collect();
                self.recording = Some(Recording {
                    start: from,
                    tracks,
                    looping: self.play_options().looping,
                    punch,
                });
                self.notify(tr("Recording…"));
            }
            Err(e) => self.report_error(trf!("Could not start recording: {}", e)),
        }
    }

    /// Stops recording: the takes are offered in the Recorded Files window,
    /// or (without the preference) placed at once.
    pub(crate) fn finish_recording(&mut self) {
        let recorded = self.engine.stop_recording();
        self.playhead = None;
        let Some(rec) = self.recording.take() else {
            return;
        };
        self.recording_waves.clear();
        let mut files = Vec::new();
        for r in recorded {
            match r.error {
                Some(e) => {
                    self.report_error(trf!("Recording {} failed: {}", r.path.display(), e));
                }
                None if r.length <= Tick::ZERO => {
                    let _ = std::fs::remove_file(&r.path);
                }
                None => files.push(Take {
                    track: r.track,
                    path: r.path,
                    length: r.length,
                    offset: r.offset,
                }),
            }
        }
        if files.is_empty() {
            return;
        }
        if self.settings.prefs.ask_keep_recordings {
            self.recorded_files = Some(RecordedFiles {
                rec,
                files,
                selected: 0,
                renaming: None,
            });
        } else {
            self.place_recordings(rec, files);
        }
    }

    /// Where a take goes: its event's start and the part of the file
    /// skipped, so that what was played and what was recorded line up
    /// (Options › Preferences › Audio Device: the latency measured, or the
    /// user's offset).
    pub(crate) fn take_placement(&self, start: Tick, take: &Take) -> (Tick, Tick) {
        let p = &self.settings.prefs;
        let shift = if p.record_latency_auto {
            take.offset.unwrap_or(Tick::ZERO)
        } else {
            -Tick::from_seconds_f64(p.record_latency_ms.max(0.0) as f64 / 1000.0)
        };
        if shift >= Tick::ZERO {
            (start + shift, Tick::ZERO)
        } else {
            (start, (-shift).min(take.length))
        }
    }

    /// The takes become events on their tracks where recording started, and
    /// media of the project (one undo): takes of the events punched into, a
    /// take a pass; round a loop, an event over it with a take a lap.
    fn place_recordings(&mut self, rec: Recording, files: Vec<Take>) {
        let start = rec.start;
        let mut infos = Vec::new();
        for take in &files {
            match bv_engine::Engine::probe_file(&take.path.to_string_lossy()) {
                Ok(info) => {
                    let (at, skip) = self.take_placement(start, take);
                    // Its file's time `f` was heard `f + shift` after the
                    // start.
                    let shift = (at - start) - skip;
                    let fits = |f: Tick, len: Tick| f >= Tick::ZERO && f + len <= take.length;
                    let elapsed = take.length + shift;
                    // The events punched into on its track: each pass.
                    let mut punched: Vec<(ClipId, Vec<Tick>)> = Vec::new();
                    for (_, clip, range) in rec.punch.iter().filter(|(t, _, _)| *t == take.track) {
                        let heard: Vec<Tick> = passes(start, rec.looping, range.start, elapsed)
                            .into_iter()
                            .map(|e| e - shift)
                            .filter(|&f| fits(f, range.duration))
                            .collect();
                        if !heard.is_empty() {
                            punched.push((*clip, heard));
                        }
                    }
                    // Round the loop (nothing punched): a take a whole lap.
                    let laps: Vec<Tick> = match rec.looping {
                        Some(l) if punched.is_empty() => passes(start, Some(l), l.start, elapsed)
                            .into_iter()
                            .map(|e| e - shift)
                            .filter(|&f| fits(f, l.duration))
                            .collect(),
                        _ => Vec::new(),
                    };
                    let mut info = info;
                    if let Some(l) = rec.looping.filter(|_| laps.len() > 1) {
                        // Regions in the file mark the takes (the Trimmer
                        // shows them).
                        for (k, &f) in laps.iter().enumerate() {
                            info.markers.push(bv_timeline::MediaMarker {
                                time: f,
                                length: l.duration,
                                name: trf!("Take {}", k + 1),
                            });
                        }
                    }
                    infos.push((take.track, info, at, skip, punched, laps));
                }
                Err(e) => self.report_error(trf!(
                    "Could not import {}: {}",
                    take.path.display(),
                    crate::media_files::error_text(&e)
                )),
            }
        }
        if infos.is_empty() {
            return;
        }
        let n = infos.len();
        let new = self.settings.prefs.new_events();
        let looping = rec.looping;
        self.edit(move |p| -> Result<(), bv_timeline::EditError> {
            for (track, info, at, skip, punched, laps) in infos {
                let length = info.duration - skip;
                let id = p.add_media(info);
                if !punched.is_empty() {
                    for (clip, heard) in punched {
                        for f in heard {
                            p.add_take_at(clip, id, f)?;
                        }
                    }
                    continue;
                }
                if let (Some(l), Some((&first, rest))) = (looping, laps.split_first()) {
                    let made = p.insert_media_range_as(
                        id,
                        l.start,
                        first,
                        l.duration,
                        (None, Some(track)),
                        &new,
                    )?;
                    if let Some(&clip) = made.first() {
                        for &f in rest {
                            p.add_take_at(clip, id, f)?;
                        }
                    }
                    continue;
                }
                if length > Tick::ZERO {
                    p.insert_media_range_as(id, at, skip, length, (None, Some(track)), &new)?;
                }
            }
            Ok(())
        });
        self.notify(trn!(n, "{} recording added.", "{} recordings added.", n));
    }

    /// Input monitoring and input busses heard (VEGAS's Help, "Recording
    /// audio", "Using input busses"): what the engine plays live now — each
    /// armed track monitored (On or Auto: when playback is stopped and
    /// while recording; Auto in a punch-in only over its selected events)
    /// through its input bus's effects and its own, at its volume and pan;
    /// each input bus whose output is the Master, and its sends to
    /// assignable effects chains — each through the bus's effects and the
    /// chain's, at the chain's input level, volume and pan (BananaV plays
    /// the chain's effects for it apart from the project's).
    pub(crate) fn follow_live(&mut self) {
        let routes = self.live_routes();
        self.engine.set_live(routes);
    }

    /// The routes [`Self::follow_live`] plays.
    pub(crate) fn live_routes(&self) -> Vec<bv_engine::LiveRoute> {
        use bv_timeline::InputMonitor;
        let p = self.project();
        let default_input = self.default_record_input();
        let pos = self.playhead.unwrap_or(self.cursor);
        let stopped = !self.is_playing();
        let mut routes = Vec::new();
        for t in p.tracks.iter().filter(|t| {
            t.kind == TrackKind::Audio && t.armed && !t.muted && !t.input_monitor.is_off()
        }) {
            let input = t.record_input.as_ref().unwrap_or(&default_input);
            let (input, bus_fx) = p.resolved_input(input);
            let active = match &self.recording {
                Some(rec) => {
                    let mine: Vec<&TimeRange> = rec
                        .punch
                        .iter()
                        .filter(|(k, _, _)| *k == t.id)
                        .map(|(_, _, r)| r)
                        .collect();
                    t.input_monitor == InputMonitor::On
                        || mine.is_empty()
                        || mine.iter().any(|r| pos >= r.start && pos < r.end())
                }
                None => stopped,
            };
            let mut chains = Vec::new();
            if !bus_fx.is_empty() {
                chains.push(bus_fx);
            }
            chains.push(t.effects.clone());
            routes.push(bv_engine::LiveRoute {
                key: t.id.0,
                input,
                chains,
                gain: t.level,
                pan: t.pan,
                active,
            });
        }
        for b in p.input_busses.iter().filter(|b| b.to_master && !b.muted) {
            routes.push(bv_engine::LiveRoute {
                key: INPUT_BUS_KEY | u64::from(b.id.0),
                input: RecordInput {
                    bus: None,
                    ..b.input.clone()
                },
                chains: vec![b.effects.clone()],
                gain: 10f32.powf(b.gain_db / 20.0),
                pan: b.pan,
                active: true,
            });
        }
        let db = |d: f32| 10f32.powf(d / 20.0);
        for b in &p.input_busses {
            for s in &b.sends {
                let bv_timeline::SendTo::Fx(chain) = s.to else {
                    continue;
                };
                let Some(c) = p.fx_chain(chain) else {
                    continue;
                };
                // After the bus's volume and mute, unless before them.
                if s.gain_db <= -96.0 || c.channel.muted || (b.muted && !s.pre_volume) {
                    continue;
                }
                let volume = if s.pre_volume { 1.0 } else { db(b.gain_db) };
                routes.push(bv_engine::LiveRoute {
                    key: INPUT_SEND_KEY | u64::from(b.id.0) << 20 | u64::from(chain.0),
                    input: RecordInput {
                        bus: None,
                        ..b.input.clone()
                    },
                    chains: vec![b.effects.clone(), c.channel.effects.clone()],
                    gain: volume * db(s.gain_db) * db(c.input_db) * db(c.channel.gain_db),
                    pan: c.channel.pan,
                    active: true,
                });
            }
        }
        routes
    }

    /// The waveforms of the tracks recording, as far as they are recorded
    /// (only what is new is fetched).
    pub(crate) fn follow_recording_waves(&mut self) {
        let Some(rec) = &self.recording else {
            return;
        };
        let show = match self.settings.prefs.record_waveforms {
            crate::settings::RecordWaveforms::None => false,
            crate::settings::RecordWaveforms::Single => rec.tracks.len() == 1,
            crate::settings::RecordWaveforms::All => true,
        };
        if !show {
            self.recording_waves.clear();
            return;
        }
        for track in rec.tracks.clone() {
            let have = self
                .recording_waves
                .get(&track)
                .map_or(0, |w| w.peaks.len());
            if let Some(new) = self.engine.recording_wave(track, have) {
                let wave = self
                    .recording_waves
                    .entry(track)
                    .or_insert(bv_engine::RecordingWave {
                        peaks: Vec::new(),
                        frames: new.frames,
                        rate: new.rate,
                    });
                wave.peaks.extend(new.peaks);
            }
        }
    }

    /// The Recorded Files window: Rename, Delete, Delete All, Done.
    pub(crate) fn recorded_files_window(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.recorded_files else {
            return;
        };
        let mut open = true;
        let mut done = false;
        let mut error = None;
        crate::floating::Window::new(tr("Recorded Files"))
            .id(egui::Id::new("recorded-files"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(460.0);
                ui.label(tr("These files were recorded:"));
                ui.add_space(4.0);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_min_height(120.0);
                    ui.set_width(ui.available_width());
                    for (i, Take { path, length, .. }) in dialog.files.iter().enumerate() {
                        let name = path.file_name().map_or_else(
                            || path.display().to_string(),
                            |n| n.to_string_lossy().to_string(),
                        );
                        let text =
                            format!("{name}   ({})", crate::fmt::num(length.as_seconds_f64(), 1));
                        if ui
                            .selectable_label(dialog.selected == i, text)
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            dialog.selected = i;
                            dialog.renaming = None;
                        }
                    }
                });
                if let Some(dir) = dialog.files.first().and_then(|t| t.path.parent()) {
                    ui.label(
                        RichText::new(trf!("Folder: {}", dir.display()))
                            .small()
                            .color(crate::theme::text_dim()),
                    );
                }
                ui.add_space(6.0);
                if dialog.renaming.is_some() {
                    let mut commit = false;
                    ui.horizontal(|ui| {
                        ui.label(tr("New name:"));
                        if let Some(text) = &mut dialog.renaming {
                            let edit =
                                ui.add(egui::TextEdit::singleline(text).desired_width(260.0));
                            let enter =
                                edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            commit = ui.button(tr("OK")).clicked() || enter;
                        }
                    });
                    if commit {
                        let typed = dialog.renaming.take().unwrap_or_default();
                        let name = typed.trim().trim_end_matches(".wav").to_string();
                        if let Some(Take { path, .. }) = dialog.files.get_mut(dialog.selected)
                            && !name.is_empty()
                        {
                            let to = path.with_file_name(format!("{name}.wav"));
                            match std::fs::rename(&*path, &to) {
                                Ok(()) => *path = to,
                                Err(e) => error = Some(trf!("Could not rename the file: {}", e)),
                            }
                        }
                    }
                }
                let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
                ui.allocate_ui_with_layout(
                    row,
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        let some = !dialog.files.is_empty();
                        if ui
                            .add_enabled(some, egui::Button::new(tr("Rename…")))
                            .clicked()
                        {
                            dialog.renaming = dialog.files.get(dialog.selected).map(|t| {
                                t.path
                                    .file_stem()
                                    .map(|s| s.to_string_lossy().to_string())
                                    .unwrap_or_default()
                            });
                        }
                        if ui
                            .add_enabled(some, egui::Button::new(tr("Delete")))
                            .clicked()
                            && dialog.selected < dialog.files.len()
                        {
                            let take = dialog.files.remove(dialog.selected);
                            let _ = std::fs::remove_file(take.path);
                            dialog.selected =
                                dialog.selected.min(dialog.files.len().saturating_sub(1));
                            dialog.renaming = None;
                        }
                        if ui
                            .add_enabled(some, egui::Button::new(tr("Delete All")))
                            .clicked()
                        {
                            for take in dialog.files.drain(..) {
                                let _ = std::fs::remove_file(take.path);
                            }
                            dialog.renaming = None;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(RichText::new(tr("Done")).strong()).clicked() {
                                done = true;
                            }
                        });
                    },
                );
            });
        if let Some(e) = error {
            self.report_error(e);
        }
        if (done || !open)
            && let Some(d) = self.recorded_files.take()
        {
            self.place_recordings(d.rec, d.files);
        }
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    /// An input bus heard, and its send to a chain through both chains'
    /// effects at the levels in turn.
    #[test]
    fn input_bus_sends_play_through_their_chains() {
        use bv_timeline::{Effect, EffectKind, Send, SendTo};
        let mut app = BananaApp::new(egui::Context::default());
        let a = app
            .modify(|p| p.add_input_bus(RecordInput::default()))
            .unwrap();
        let fx = app
            .modify(|p| p.add_fx_chain(vec![Effect::new(EffectKind::Reverb)]))
            .unwrap();
        let mut send = Send::new(SendTo::Fx(fx));
        send.gain_db = -6.0;
        assert!(app.modify(|p| p.set_input_bus_send(a, send)));
        let routes = app.live_routes();
        assert_eq!(routes.len(), 2);
        let r = routes.iter().find(|r| r.key & INPUT_SEND_KEY != 0).unwrap();
        assert_eq!(r.chains.len(), 2);
        assert_eq!(r.chains[1][0].kind, EffectKind::Reverb);
        assert!((r.gain - 10f32.powf(-6.0 / 20.0)).abs() < 1e-4);
        // Muted, the bus sends nothing after its volume; before it, still.
        app.modify(|p| p.input_bus_mut(a).unwrap().muted = true);
        assert!(app.live_routes().is_empty());
        send.pre_volume = true;
        app.modify(|p| p.set_input_bus_send(a, send));
        assert_eq!(app.live_routes().len(), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_line_up_with_what_was_heard() {
        let mut app = BananaApp::new(egui::Context::default());
        let s = Tick::from_seconds;
        let ms = |m: f64| Tick::from_seconds_f64(m / 1000.0);
        let take = |offset| Take {
            track: TrackId(1),
            path: PathBuf::from("take.wav"),
            length: s(10),
            offset,
        };
        // Measured: captured 30 ms after the first sound was heard, the
        // file starts 30 ms later; 20 ms before, its first 20 ms go.
        assert_eq!(
            app.take_placement(s(5), &take(Some(ms(30.0)))),
            (s(5) + ms(30.0), Tick::ZERO)
        );
        assert_eq!(
            app.take_placement(s(5), &take(Some(-ms(20.0)))),
            (s(5), ms(20.0))
        );
        assert_eq!(app.take_placement(s(5), &take(None)), (s(5), Tick::ZERO));
        // The user's offset instead.
        app.settings.prefs.record_latency_auto = false;
        app.settings.prefs.record_latency_ms = 12.0;
        assert_eq!(
            app.take_placement(s(5), &take(Some(ms(30.0)))),
            (s(5), ms(12.0))
        );
    }

    /// Passes through a timeline time: once on the way, then each lap of
    /// the loop it is in.
    #[test]
    fn passes_go_round_the_loop() {
        let s = Tick::from_seconds;
        let l = Some(TimeRange::from_bounds(s(10), s(14)));
        // From 8 s: 11 s reached after 3 s, then each 4 s lap.
        assert_eq!(passes(s(8), l, s(11), s(15)), [s(3), s(7), s(11), s(15)]);
        // The loop's start: after 2 s (the way there), then laps.
        assert_eq!(passes(s(8), l, s(10), s(10)), [s(2), s(6), s(10)]);
        // Outside the loop: once.
        assert_eq!(passes(s(8), l, s(9), s(20)), [s(1)]);
        assert_eq!(passes(s(8), None, s(11), s(20)), [s(3)]);
    }

    /// Recorded round a loop: an event over it, a take a lap (the last
    /// active), regions in the file; punched into a selected event: a take
    /// a pass.
    #[test]
    fn loop_takes_and_punch_ins() {
        let s = Tick::from_seconds;
        let dir = std::env::temp_dir().join(format!("bv-laps-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Laps.wav");
        crate::tabs::trimmer::tests_wav(&path, 10);
        let mut app = BananaApp::new(egui::Context::default());
        app.settings.prefs.record_latency_auto = true;
        let track = app.modify(|p| p.add_track(TrackKind::Audio));
        let take = Take {
            track,
            path: path.clone(),
            length: s(10),
            offset: Some(Tick::ZERO),
        };
        // From 2 s round a 2 s to 5 s loop: laps from 0 s, 3 s and 6 s of
        // the file (9 s on is not a whole lap).
        let looping = Some(TimeRange::from_bounds(s(2), s(5)));
        let rec = Recording {
            start: s(2),
            tracks: vec![track],
            looping,
            punch: Vec::new(),
        };
        app.place_recordings(rec, vec![take.clone()]);
        let clips = app.project().track(track).unwrap().clips.clone();
        assert_eq!(clips.len(), 1);
        let c = &clips[0];
        assert_eq!((c.start, c.duration), (s(2), s(3)));
        assert_eq!(c.take_count(), 3);
        assert_eq!(c.active_take(), 2);
        assert_eq!(c.source_in, s(6), "the last lap plays");
        let media = app.project().media(c.media).unwrap();
        assert_eq!(media.markers.len(), 3);
        // Punched into that event from 1 s, no loop: one pass, 1 s into
        // the file.
        let clip = c.id;
        let rec = Recording {
            start: s(1),
            tracks: vec![track],
            looping: None,
            punch: vec![(track, clip, TimeRange::from_bounds(s(2), s(5)))],
        };
        app.place_recordings(rec, vec![take]);
        let c = app.project().clip(clip).unwrap().clone();
        assert_eq!(c.take_count(), 4);
        assert_eq!(c.source_in, s(1));
        assert_eq!(
            app.project().track(track).unwrap().clips.len(),
            1,
            "no new event"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn with_nothing_armed_a_new_track_records_or_the_user_is_asked() {
        let mut app = BananaApp::new(egui::Context::default());
        app.start_recording();
        assert!(matches!(
            app.ask.as_ref().map(|a| &a.what),
            Some(crate::ask::Asked::RecordNewTrack)
        ));
        // The default input follows the system's choice with that device
        // type, keeping the channels chosen.
        app.settings.prefs.record_device = RecordInput {
            device: "USB Microphone".into(),
            first: 1,
            channels: 1,
            bus: None,
        };
        assert_eq!(app.default_record_input().device, "");
        assert_eq!(app.default_record_input().first, 1);
        app.settings.prefs.audio_device_type = crate::settings::AudioDeviceType::Windows;
        assert_eq!(app.default_record_input().device, "USB Microphone");
    }

    #[test]
    fn takes_are_named_after_their_track() {
        let dir = std::env::temp_dir().join(format!("bv-takes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = take_path(&dir, "Voice: lead", &[]);
        assert_eq!(first.file_name().unwrap(), "Voice_ lead Recording 1.wav");
        // Two tracks of one name: two files.
        let other = take_path(&dir, "Voice: lead", std::slice::from_ref(&first));
        assert_eq!(other.file_name().unwrap(), "Voice_ lead Recording 2.wav");
        std::fs::write(&first, b"").unwrap();
        assert_eq!(
            take_path(&dir, "Voice: lead", &[]).file_name().unwrap(),
            "Voice_ lead Recording 2.wav"
        );
        let _ = std::fs::remove_dir_all(&dir);
        let stereo = RecordInput::default();
        assert_eq!(input_label(&stereo), "Stereo");
        let right = RecordInput {
            first: 1,
            channels: 1,
            ..RecordInput::default()
        };
        assert_eq!(input_label(&right), "Mono Right");
    }

    #[test]
    fn recorded_files_become_events_where_recording_started() {
        let mut app = BananaApp::new(egui::Context::default());
        // A new project has no tracks: one to record on.
        let track = app.modify(|p| p.add_track(TrackKind::Audio));
        // Nothing armed: nothing records.
        app.start_recording();
        assert!(!app.engine.is_recording());
        // A take recorded (made here as a file).
        let dir = std::env::temp_dir().join(format!("bv-recorded-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Audio 2 Recording 1.wav");
        crate::tabs::trimmer::tests_wav(&path, 2);
        let mut kept = dir.join("Other.wav");
        crate::tabs::trimmer::tests_wav(&kept, 1);
        let take = |path: &std::path::Path, seconds: i64| Take {
            track,
            path: path.to_path_buf(),
            length: Tick::from_seconds(seconds),
            offset: None,
        };
        app.recorded_files = Some(RecordedFiles {
            rec: Recording {
                start: Tick::from_seconds(3),
                ..Recording::default()
            },
            files: vec![take(&path, 2), take(&kept, 1)],
            selected: 0,
            renaming: None,
        });
        // Delete: the selected file goes.
        if let Some(d) = &mut app.recorded_files {
            let t = d.files.remove(0);
            std::fs::remove_file(t.path).unwrap();
        }
        assert!(!path.exists());
        // Done: the rest become events at 3 s on their track.
        let d = app.recorded_files.take().unwrap();
        kept = d.files[0].path.clone();
        app.place_recordings(d.rec, d.files);
        let clips: Vec<_> = app.project().track(track).unwrap().clips.clone();
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].start, Tick::from_seconds(3));
        assert_eq!(clips[0].duration, Tick::from_seconds(1));
        assert!(app.project().media.iter().any(|m| matches!(&m.source,
            bv_timeline::MediaSource::File { path } if std::path::Path::new(path) == kept)));
        assert!(app.history.can_undo());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
