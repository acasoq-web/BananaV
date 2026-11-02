//! Track automation (a track header's Automation Settings and Activate
//! Automation Controls), and a bus track's alike: the volume, pan, level
//! and send controls either trim or, as automation controls, follow the
//! envelopes and record them — while playing, in Touch until the control
//! is let go, in Latch until playback stops; stopped, they set the
//! envelope's point at the cursor. The Mute button as an automation
//! control edits the mute envelope (with automation off it mutes the
//! whole track; in Read it only shows the envelope). With automation off
//! the envelopes do not play and the automation controls hold values of
//! their own.

use bv_core::Tick;
use bv_timeline::{
    AutomationMode, EnvelopeKind, EnvelopeOwner, Gesture, KeyCurve, Project, Send, SendTo, Strip,
    TrackAutomation, TrackId,
};

use crate::app::BananaApp;
use crate::egui;

/// A track control: what its header's sliders (and a control surface's
/// faders and knobs) set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrackControl {
    /// An audio track's volume (a bus's).
    Volume,
    /// An audio track's pan (a bus's).
    Pan,
    /// A video track's level (its composite level).
    Level,
    /// A video track's Fade slider (fade to color): an automation control
    /// only, shown with Automation Settings.
    Fade,
    /// The Mute button (1 playing, 0 muted).
    Mute,
    /// An audio track's multipurpose slider: its send to a bus or a chain.
    Send(SendTo),
    /// The video bus's Motion Blur slider (seconds) and Supersampling
    /// slider (frames a frame).
    MotionBlur,
    Supersampling,
}

impl TrackControl {
    /// The envelope it automates.
    pub fn envelope(self) -> EnvelopeKind {
        match self {
            TrackControl::Volume => EnvelopeKind::Volume,
            TrackControl::Pan => EnvelopeKind::Pan,
            TrackControl::Level => EnvelopeKind::CompositeLevel,
            TrackControl::Fade => EnvelopeKind::FadeToColor,
            TrackControl::Mute => EnvelopeKind::Mute,
            TrackControl::Send(to) => EnvelopeKind::Send(to),
            TrackControl::MotionBlur => EnvelopeKind::MotionBlur,
            TrackControl::Supersampling => EnvelopeKind::Supersampling,
        }
    }

    pub fn default_value(self) -> f32 {
        match self {
            TrackControl::Pan | TrackControl::Fade | TrackControl::MotionBlur => 0.0,
            TrackControl::Supersampling => 1.0,
            TrackControl::Volume | TrackControl::Level | TrackControl::Mute => 1.0,
            TrackControl::Send(_) => 1.0,
        }
    }
}

/// Whose controls: a track's, or a bus's or chain's (its bus track's and
/// its Mixing Console strip's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoOwner {
    Track(TrackId),
    Strip(Strip),
    /// The video bus track's.
    VideoBus,
}

impl From<AutoOwner> for EnvelopeOwner {
    fn from(o: AutoOwner) -> EnvelopeOwner {
        match o {
            AutoOwner::Track(t) => EnvelopeOwner::Track(t),
            AutoOwner::Strip(s) => EnvelopeOwner::Strip(s),
            AutoOwner::VideoBus => EnvelopeOwner::VideoBus,
        }
    }
}

/// Automation recorded since a control was first moved while playing: one
/// undo for it all.
#[derive(Debug)]
pub(crate) struct AutomationPass {
    gesture: Gesture<Project>,
    writes: Vec<Write>,
}

/// One control's recording.
#[derive(Debug)]
struct Write {
    owner: AutoOwner,
    control: TrackControl,
    samples: Vec<(Tick, f32)>,
    /// Still being held (Touch) or latched (Latch).
    held: bool,
    latch: bool,
    last: f32,
}

/// `owner`'s automation settings.
fn automation_of(p: &Project, owner: AutoOwner) -> Option<TrackAutomation> {
    match owner {
        AutoOwner::Track(t) => p.track(t).map(|t| t.automation),
        AutoOwner::Strip(s) => p.strip_channel(s).map(|c| c.automation),
        AutoOwner::VideoBus => Some(p.video_bus.automation),
    }
}

/// What `control` of `owner` is as a trim (the Mute button: whether it
/// plays).
pub(crate) fn trim_value(p: &Project, owner: AutoOwner, control: TrackControl) -> f32 {
    match owner {
        AutoOwner::Track(id) => {
            let Some(t) = p.track(id) else {
                return control.default_value();
            };
            match control {
                TrackControl::Pan => t.pan,
                TrackControl::Volume | TrackControl::Level => t.level,
                TrackControl::Fade => 0.0,
                TrackControl::Mute => playing(!t.muted),
                TrackControl::Send(to) => t
                    .sends
                    .iter()
                    .find(|s| s.to == to)
                    .map_or(0.0, |s| db_to_gain(s.gain_db)),
                TrackControl::MotionBlur | TrackControl::Supersampling => control.default_value(),
            }
        }
        AutoOwner::Strip(s) => {
            let Some(c) = p.strip_channel(s) else {
                return control.default_value();
            };
            match control {
                TrackControl::Pan => c.pan,
                TrackControl::Volume => db_to_gain(c.gain_db),
                TrackControl::Mute => playing(!c.muted),
                _ => control.default_value(),
            }
        }
        AutoOwner::VideoBus => {
            let b = &p.video_bus;
            match control {
                TrackControl::MotionBlur => b.motion_blur,
                TrackControl::Supersampling => b.supersampling.max(1.0),
                TrackControl::Mute => playing(!b.muted),
                _ => control.default_value(),
            }
        }
    }
}

fn playing(on: bool) -> f32 {
    if on { 1.0 } else { 0.0 }
}

/// A level in dB as a gain (at or under −96 dB, nothing), and back, as
/// the mixer counts them.
fn db_to_gain(db: f32) -> f32 {
    if db <= -96.0 {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

fn gain_to_db(g: f32) -> f32 {
    if g <= 1e-5 { -96.0 } else { 20.0 * g.log10() }
}

/// `control` of `owner` trimmed to `value` (the Mute button: muted below
/// a half).
pub(crate) fn set_trim(p: &mut Project, owner: AutoOwner, control: TrackControl, value: f32) {
    match owner {
        AutoOwner::Track(id) => match control {
            TrackControl::Send(to) => {
                let Some(t) = p.track(id) else {
                    return;
                };
                let send = t
                    .sends
                    .iter()
                    .find(|s| s.to == to)
                    .copied()
                    .unwrap_or_else(|| Send::new(to));
                let gain_db = if value <= 1e-4 {
                    f32::NEG_INFINITY
                } else {
                    20.0 * value.log10()
                };
                p.set_send(id, Send { gain_db, ..send });
            }
            TrackControl::Mute => {
                let _ = p.set_track_muted(id, value < 0.5);
            }
            _ => {
                if let Some(t) = p.track_by_id_mut(id) {
                    match control {
                        TrackControl::Pan => t.pan = value,
                        TrackControl::Volume | TrackControl::Level => t.level = value,
                        _ => {}
                    }
                }
            }
        },
        AutoOwner::Strip(s) => {
            if let Some(c) = p.strip_channel_mut(s) {
                match control {
                    TrackControl::Volume => c.gain_db = gain_to_db(value),
                    TrackControl::Pan => c.pan = value.clamp(-1.0, 1.0),
                    TrackControl::Mute => c.muted = value < 0.5,
                    _ => {}
                }
            }
        }
        AutoOwner::VideoBus => set_bus_value(p, control, value),
    }
}

/// The video bus's slider `control` (or its Mute) set to `value` (its own
/// value: a trim, or what it holds with automation off).
fn set_bus_value(p: &mut Project, control: TrackControl, value: f32) {
    let b = &mut p.video_bus;
    match control {
        TrackControl::MotionBlur => b.motion_blur = value.clamp(0.0, 1.0),
        TrackControl::Supersampling => b.supersampling = value.round().clamp(1.0, 8.0),
        TrackControl::Mute => b.muted = value < 0.5,
        _ => {}
    }
}

/// What `control` of `owner` holds with automation off.
fn set_held(p: &mut Project, owner: AutoOwner, control: TrackControl, value: f32) {
    match owner {
        AutoOwner::Track(id) => {
            if let Some(t) = p.track_by_id_mut(id) {
                match control {
                    TrackControl::Volume => t.automation.volume = value,
                    TrackControl::Pan => t.automation.pan = value,
                    TrackControl::Level => t.automation.level = value,
                    TrackControl::Fade => t.automation.fade = value,
                    TrackControl::Mute => {}
                    TrackControl::Send(to) => {
                        if let Some(s) = t.sends.iter_mut().find(|s| s.to == to) {
                            s.held = value;
                        }
                    }
                    TrackControl::MotionBlur | TrackControl::Supersampling => {}
                }
            }
        }
        AutoOwner::Strip(s) => {
            if let Some(c) = p.strip_channel_mut(s) {
                match control {
                    TrackControl::Volume => c.automation.volume = value,
                    TrackControl::Pan => c.automation.pan = value,
                    _ => {}
                }
            }
        }
        AutoOwner::VideoBus => {
            if control != TrackControl::Mute {
                set_bus_value(p, control, value);
            }
        }
    }
}

impl BananaApp {
    /// What `control` of `track` shows: its trim, or, as an automation
    /// control, the automation at the time shown (the value held with
    /// automation off).
    pub(crate) fn control_value(&self, track: TrackId, control: TrackControl) -> f32 {
        self.owner_value(AutoOwner::Track(track), control)
    }

    /// [`Self::control_value`] of a track's or a bus track's control.
    pub(crate) fn owner_value(&self, owner: AutoOwner, control: TrackControl) -> f32 {
        let p = self.project();
        let Some(auto) = automation_of(p, owner) else {
            return control.default_value();
        };
        // The Mute button with automation off mutes the whole track.
        if !auto.controls || (control == TrackControl::Mute && auto.mode == AutomationMode::Off) {
            return trim_value(p, owner, control);
        }
        let at = self.display_time();
        let value = match owner {
            AutoOwner::Track(id) => p.track(id).and_then(|t| match control {
                TrackControl::Send(to) if t.automation.mode == AutomationMode::Off => {
                    Some(t.sends.iter().find(|s| s.to == to).map_or(1.0, |s| s.held))
                }
                _ => t.automation_at(control.envelope(), at),
            }),
            AutoOwner::Strip(s) => p
                .strip_channel(s)
                .and_then(|c| c.automation_at(control.envelope(), at)),
            AutoOwner::VideoBus => Some(match control {
                TrackControl::Mute => playing(!p.video_bus.muted_at(at)),
                _ => p.video_bus.control_at(control.envelope(), at),
            }),
        };
        value.unwrap_or(control.default_value())
    }

    /// Whether `owner`'s Mute button shows it muted.
    pub(crate) fn owner_muted(&self, owner: AutoOwner) -> bool {
        self.owner_value(owner, TrackControl::Mute) < 0.5
    }

    /// Moves `control` of `track` to `value` (`key`: merges the undo of a
    /// drag): the trim, or as an automation control — with automation off
    /// the value it holds; in Read nothing (it follows the envelope); in
    /// Touch and Latch, recorded while playing, or else the envelope's
    /// point at the cursor (the envelope added if missing).
    pub(crate) fn set_control(
        &mut self,
        track: TrackId,
        control: TrackControl,
        value: f32,
        key: egui::Id,
    ) {
        self.set_owner_control(AutoOwner::Track(track), control, value, key);
    }

    /// [`Self::set_control`] of a track's or a bus track's control. The
    /// Mute button mutes the whole track with automation off; while
    /// playing in Touch or Latch, what it sets holds until playback stops.
    pub(crate) fn set_owner_control(
        &mut self,
        owner: AutoOwner,
        control: TrackControl,
        value: f32,
        key: egui::Id,
    ) {
        let Some(auto) = automation_of(self.project(), owner) else {
            return;
        };
        if !auto.controls || (control == TrackControl::Mute && auto.mode == AutomationMode::Off) {
            // Fade has no trim: it is an automation control only.
            if control == TrackControl::Fade {
                return;
            }
            self.modify_merged(key, |p| set_trim(p, owner, control, value));
            return;
        }
        match auto.mode {
            AutomationMode::Off => self.modify_merged(key, |p| set_held(p, owner, control, value)),
            AutomationMode::Read => {}
            AutomationMode::Touch | AutomationMode::Latch => {
                let latch = auto.mode == AutomationMode::Latch || control == TrackControl::Mute;
                match self.playhead.filter(|_| self.is_playing()) {
                    Some(at) => self.record_control(owner, control, value, at, latch),
                    None => {
                        let at = self.cursor;
                        let kind = control.envelope();
                        let curve = self.settings.prefs.new_envelope_curve(kind);
                        self.modify_merged(key, |p| {
                            let _ = set_point(p, owner, kind, at, value, curve);
                        });
                    }
                }
            }
        }
    }

    /// Moves `control` of `track` to `value` and, when `track` is among
    /// several selected tracks, theirs by as much (VEGAS's Help: "if
    /// multiple tracks are selected, all selected tracks are adjusted"):
    /// the volume and a send by the same dB, the others by the same step.
    pub(crate) fn set_controls(
        &mut self,
        track: TrackId,
        control: TrackControl,
        value: f32,
        key: egui::Id,
    ) {
        let before = self.control_value(track, control);
        let others: Vec<TrackId> = {
            let sel = self.track_selection();
            if sel.len() > 1 && sel.contains(&track) {
                let kind = self.project().track(track).map(|t| t.kind);
                sel.into_iter()
                    .filter(|t| *t != track)
                    .filter(|t| self.project().track(*t).map(|x| x.kind) == kind)
                    .collect()
            } else {
                Vec::new()
            }
        };
        self.set_control(track, control, value, key);
        for t in others {
            let now = self.control_value(t, control);
            let v = match control {
                TrackControl::Volume | TrackControl::Send(_) if before > 1e-6 => {
                    now * value / before
                }
                TrackControl::Volume | TrackControl::Send(_) | TrackControl::Mute => value,
                TrackControl::Pan | TrackControl::Fade => (now + value - before).clamp(-1.0, 1.0),
                TrackControl::Level => (now + value - before).clamp(0.0, 1.0),
                TrackControl::MotionBlur | TrackControl::Supersampling => value,
            };
            let v = match control {
                TrackControl::Volume | TrackControl::Send(_) => {
                    v.clamp(0.0, bv_timeline::envelope::MAX_VOLUME)
                }
                _ => v,
            };
            self.set_control(t, control, v, key);
        }
    }

    /// `controls` of every selected track let go (see `set_controls`).
    pub(crate) fn release_controls(&mut self, track: TrackId, control: TrackControl) {
        for t in self.track_selection() {
            if t != track {
                self.release_control(t, control);
            }
        }
        self.release_control(track, control);
    }

    /// `control` of `track` let go: in Touch its recording ends (and with
    /// it the pass, when nothing else is held).
    pub(crate) fn release_control(&mut self, track: TrackId, control: TrackControl) {
        self.release_owner_control(AutoOwner::Track(track), control);
    }

    /// [`Self::release_control`] of a track's or a bus track's control.
    pub(crate) fn release_owner_control(&mut self, owner: AutoOwner, control: TrackControl) {
        let Some(pass) = &mut self.automation_pass else {
            return;
        };
        for w in pass
            .writes
            .iter_mut()
            .filter(|w| w.owner == owner && w.control == control && !w.latch)
        {
            w.held = false;
        }
        if pass.writes.iter().all(|w| !w.held) {
            self.finish_automation();
        }
    }

    fn record_control(
        &mut self,
        owner: AutoOwner,
        control: TrackControl,
        value: f32,
        at: Tick,
        latch: bool,
    ) {
        if self.automation_pass.is_none() {
            let gesture = self.history.begin_gesture();
            self.automation_pass = Some(AutomationPass {
                gesture,
                writes: Vec::new(),
            });
        }
        let Some(pass) = &mut self.automation_pass else {
            return;
        };
        match pass
            .writes
            .iter_mut()
            .find(|w| w.owner == owner && w.control == control)
        {
            Some(w) => {
                w.samples.push((at, value));
                (w.held, w.last) = (true, value);
            }
            None => pass.writes.push(Write {
                owner,
                control,
                samples: vec![(at, value)],
                held: true,
                latch,
                last: value,
            }),
        }
        self.preview_automation(false);
    }

    /// Each frame while recording: the controls held (or latched) keep
    /// their value as playback goes on; when playback stops, the pass ends.
    pub(crate) fn follow_automation(&mut self) {
        if self.automation_pass.is_none() {
            return;
        }
        let Some(at) = self.playhead.filter(|_| self.is_playing()) else {
            self.finish_automation();
            return;
        };
        let Some(pass) = &mut self.automation_pass else {
            return;
        };
        let mut more = false;
        for w in pass.writes.iter_mut().filter(|w| w.held) {
            if w.samples.last().is_none_or(|s| s.0 < at) {
                w.samples.push((at, w.last));
                more = true;
            }
        }
        if more {
            self.preview_automation(false);
        }
    }

    /// The pass so far, written over the project as it was when it began;
    /// `thin`: with fewer points.
    fn preview_automation(&mut self, thin: bool) {
        let Some(pass) = &self.automation_pass else {
            return;
        };
        let prefs = &self.settings.prefs;
        self.history.preview(&pass.gesture, |p| {
            for w in &pass.writes {
                let kind = w.control.envelope();
                let curve = prefs.new_envelope_curve(kind);
                let _ = p.write_automation_to(w.owner.into(), kind, &w.samples, thin, curve);
            }
        });
    }

    /// The pass ends: thinned if Options › Preferences › External Control
    /// & Automation says so ("Smooth and thin automation data after
    /// recording or drawing"), one undo.
    pub(crate) fn finish_automation(&mut self) {
        let thin = self.settings.prefs.thin_automation;
        self.preview_automation(thin);
        if let Some(pass) = self.automation_pass.take() {
            self.history.commit_gesture(pass.gesture);
        }
    }

    /// Automation Settings: how `track`'s envelopes play and record. Turned
    /// off, its automation controls go back to their defaults, or keep the
    /// values the envelopes have at the cursor (Options › Preferences ›
    /// External Control & Automation: "Set controls to default values when
    /// automation is turned off").
    pub(crate) fn set_automation_mode(&mut self, track: TrackId, mode: AutomationMode) {
        self.set_owner_automation_mode(AutoOwner::Track(track), mode);
    }

    /// [`Self::set_automation_mode`] of a track or a bus track.
    pub(crate) fn set_owner_automation_mode(&mut self, owner: AutoOwner, mode: AutomationMode) {
        let at = self.cursor;
        let defaults = self.settings.prefs.automation_off_defaults;
        self.modify(|p| {
            let (auto, envelopes) = match owner {
                AutoOwner::Track(id) => match p.track_by_id_mut(id) {
                    Some(t) => (&mut t.automation, &t.envelopes),
                    None => return,
                },
                AutoOwner::Strip(s) => match p.strip_channel_mut(s) {
                    Some(c) => (&mut c.automation, &c.envelopes),
                    None => return,
                },
                AutoOwner::VideoBus => {
                    // Turned off, its sliders hold what the envelopes have
                    // at the cursor (or go back to none).
                    let b = &mut p.video_bus;
                    if mode == AutomationMode::Off && b.automation.mode != AutomationMode::Off {
                        let held = |kind: EnvelopeKind, own: f32| {
                            if defaults {
                                0.0
                            } else {
                                b.envelopes
                                    .iter()
                                    .find(|e| e.kind == kind)
                                    .map_or(own, |e| e.value_at(at))
                            }
                        };
                        (b.motion_blur, b.supersampling) = (
                            held(EnvelopeKind::MotionBlur, b.motion_blur),
                            held(EnvelopeKind::Supersampling, b.supersampling),
                        );
                    }
                    b.automation.mode = mode;
                    return;
                }
            };
            if mode == AutomationMode::Off && auto.mode != AutomationMode::Off {
                let held = |kind: EnvelopeKind, default: f32| {
                    if defaults {
                        default
                    } else {
                        envelopes
                            .iter()
                            .find(|e| e.kind == kind)
                            .map_or(default, |e| e.value_at(at))
                    }
                };
                (auto.volume, auto.pan, auto.level) = (
                    held(EnvelopeKind::Volume, 1.0),
                    held(EnvelopeKind::Pan, 0.0),
                    held(EnvelopeKind::CompositeLevel, 1.0),
                );
            }
            auto.mode = mode;
        });
    }

    /// Activate Automation Controls: `track`'s sliders automate instead of
    /// trimming, or the other way round.
    pub(crate) fn toggle_automation_controls(&mut self, track: TrackId) {
        self.toggle_owner_automation_controls(AutoOwner::Track(track));
    }

    /// [`Self::toggle_automation_controls`] of a track or a bus track.
    pub(crate) fn toggle_owner_automation_controls(&mut self, owner: AutoOwner) {
        self.modify(|p| match owner {
            AutoOwner::Track(id) => {
                if let Some(t) = p.track_by_id_mut(id) {
                    t.automation.controls ^= true;
                }
            }
            AutoOwner::Strip(s) => {
                if let Some(c) = p.strip_channel_mut(s) {
                    c.automation.controls ^= true;
                }
            }
            AutoOwner::VideoBus => p.video_bus.automation.controls ^= true,
        });
    }

    /// The Mute button of `owner` clicked: muted or not as a trim, or as
    /// an automation control (in Read nothing: it shows the envelope).
    pub(crate) fn click_mute(&mut self, owner: AutoOwner, key: egui::Id) {
        let muted = self.owner_muted(owner);
        self.set_owner_control(owner, TrackControl::Mute, playing(muted), key);
    }
}

/// The point at `at` of `owner`'s `kind` envelope set to `value`, the
/// envelope added (its points starting with `curve`) when missing.
fn set_point(
    p: &mut Project,
    owner: AutoOwner,
    kind: EnvelopeKind,
    at: Tick,
    value: f32,
    curve: KeyCurve,
) -> Result<usize, bv_timeline::EditError> {
    let has = match owner {
        AutoOwner::Track(t) => p
            .track(t)
            .is_some_and(|t| t.envelopes.iter().any(|e| e.kind == kind)),
        AutoOwner::Strip(s) => p
            .strip_channel(s)
            .is_some_and(|c| c.envelopes.iter().any(|e| e.kind == kind)),
        AutoOwner::VideoBus => p.video_bus.envelopes.iter().any(|e| e.kind == kind),
    };
    if !has {
        match owner {
            AutoOwner::Track(t) => {
                p.toggle_envelope_as(t, kind, curve)?;
            }
            AutoOwner::Strip(s) => {
                p.toggle_strip_envelope(s, kind, curve)?;
            }
            AutoOwner::VideoBus => {
                p.toggle_video_bus_envelope(kind, curve)?;
            }
        }
    }
    p.set_envelope_point(EnvelopeOwner::from(owner), kind, at, value)
}

impl BananaApp {
    /// The bus tracks of [`Self::bus_tracks_for_review`] with bus A's, the
    /// audio track's and the video bus's controls automation controls
    /// (Touch; the audio track's Mute button and multipurpose slider
    /// marked) and a mute envelope on bus A (screen tests).
    pub fn bus_automation_for_review(&mut self) -> bool {
        use bv_timeline::TrackKind;
        if self.project().busses.is_empty() && !self.bus_tracks_for_review() {
            return false;
        }
        let made = self.modify(|p| {
            let a = p.busses.first()?.id;
            let audio = p.tracks.iter().find(|t| t.kind == TrackKind::Audio)?.id;
            let strip = Strip::Bus(a);
            let c = p.strip_channel_mut(strip)?;
            c.automation.controls = true;
            c.automation.mode = AutomationMode::Touch;
            p.toggle_strip_envelope(strip, EnvelopeKind::Mute, KeyCurve::Hold)
                .ok()?;
            let owner = EnvelopeOwner::Strip(strip);
            p.set_envelope_point(owner, EnvelopeKind::Mute, Tick::from_seconds(7), 0.0)
                .ok()?;
            let t = p.track_by_id_mut(audio)?;
            t.automation.controls = true;
            t.automation.mode = AutomationMode::Touch;
            p.video_bus.automation.controls = true;
            p.video_bus.automation.mode = AutomationMode::Touch;
            Some(())
        });
        made.is_some()
    }

    /// The demo's video track with Automation Settings on (its Fade slider
    /// shown, a fade to the top color held with automation off) and a
    /// second audio track, both audio tracks selected (screen tests).
    pub fn header_controls_for_review(&mut self) -> bool {
        use bv_timeline::TrackKind;
        let made = self.modify(|p| {
            let v = p.tracks.iter().find(|t| t.kind == TrackKind::Video)?.id;
            let a = p.tracks.iter().find(|t| t.kind == TrackKind::Audio)?.id;
            let t = p.track_by_id_mut(v)?;
            t.automation.controls = true;
            t.automation.mode = AutomationMode::Off;
            t.automation.fade = 0.35;
            let b = p.add_track(TrackKind::Audio);
            Some((a, b))
        });
        let Some((a, b)) = made else {
            return false;
        };
        self.pick_track(a, false, false);
        self.pick_track(b, true, false);
        self.show_tab(crate::dock::Tab::Timeline, false);
        true
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    use bv_timeline::TrackKind;

    /// The selected tracks move together: the volume by the same dB, the
    /// pan by the same step; a track outside the selection alone. The
    /// Fade control is an automation control only.
    #[test]
    fn selected_tracks_are_adjusted_together() {
        let mut app = BananaApp::new(egui::Context::default());
        let (a, b, c) = app.modify(|p| {
            (
                p.add_track(TrackKind::Audio),
                p.add_track(TrackKind::Audio),
                p.add_track(TrackKind::Audio),
            )
        });
        app.modify(|p| p.track_by_id_mut(b).unwrap().level = 0.5);
        app.pick_track(a, false, false);
        app.pick_track(b, true, false);
        let key = egui::Id::new("test");
        app.set_controls(a, TrackControl::Volume, 2.0, key);
        app.release_controls(a, TrackControl::Volume);
        let p = app.project();
        assert!((p.track(a).unwrap().level - 2.0).abs() < 1e-6);
        assert!((p.track(b).unwrap().level - 1.0).abs() < 1e-6, "+6 dB too");
        assert!(
            (p.track(c).unwrap().level - 1.0).abs() < 1e-6,
            "not selected"
        );
        app.set_controls(b, TrackControl::Pan, -0.25, key);
        let p = app.project();
        assert!((p.track(a).unwrap().pan + 0.25).abs() < 1e-6);
        // Not in the selection: alone.
        app.set_controls(c, TrackControl::Pan, 0.5, key);
        assert!((app.project().track(a).unwrap().pan + 0.25).abs() < 1e-6);
        // Fade: nothing as a trim; with automation off, its held value.
        let v = app.modify(|p| p.add_track(TrackKind::Video));
        app.set_control(v, TrackControl::Fade, 0.4, key);
        assert_eq!(app.control_value(v, TrackControl::Fade), 0.0);
        app.modify(|p| {
            let t = p.track_by_id_mut(v).unwrap();
            t.automation.controls = true;
            t.automation.mode = bv_timeline::AutomationMode::Off;
        });
        app.set_control(v, TrackControl::Fade, 0.4, key);
        let t = app.project().track(v).unwrap();
        assert_eq!(t.automation.fade, 0.4);
        assert_eq!(
            t.automation_at(bv_timeline::EnvelopeKind::FadeToColor, bv_core::Tick::ZERO),
            Some(0.4)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::TrackKind;

    fn app_with_audio() -> (BananaApp, TrackId) {
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.load_demo();
        let t = app
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
            .unwrap();
        (app, t)
    }

    #[test]
    fn trims_or_automation_controls() {
        let (mut app, t) = app_with_audio();
        let key = egui::Id::new("test");
        // A trim: the track's level.
        app.set_control(t, TrackControl::Volume, 0.5, key);
        assert_eq!(app.project().track(t).unwrap().level, 0.5);
        // Automation controls in Touch, stopped: the envelope's point at the
        // cursor, the envelope added.
        app.toggle_automation_controls(t);
        app.set_automation_mode(t, AutomationMode::Touch);
        app.cursor = Tick::from_seconds(2);
        app.set_control(t, TrackControl::Pan, -0.5, key);
        let tr = app.project().track(t).unwrap();
        let pan = tr.envelopes.iter().find(|e| e.kind == EnvelopeKind::Pan);
        assert_eq!(pan.map(|e| e.value_at(Tick::from_seconds(2))), Some(-0.5));
        assert_eq!(tr.level, 0.5, "the trim stays");
        assert_eq!(app.control_value(t, TrackControl::Pan), -0.5);
        // In Read it follows: moving it does nothing.
        app.set_automation_mode(t, AutomationMode::Read);
        app.set_control(t, TrackControl::Pan, 0.9, key);
        assert_eq!(app.control_value(t, TrackControl::Pan), -0.5);
    }

    #[test]
    fn automation_turned_off_holds_defaults_or_the_last_values() {
        let (mut app, t) = app_with_audio();
        let key = egui::Id::new("test");
        app.toggle_automation_controls(t);
        app.set_automation_mode(t, AutomationMode::Latch);
        app.cursor = Tick::ZERO;
        app.set_control(t, TrackControl::Volume, 0.25, key);
        // Off with the preference on: back to 0 dB.
        app.set_automation_mode(t, AutomationMode::Off);
        assert_eq!(app.project().track(t).unwrap().automation.volume, 1.0);
        // With it off: the envelope's value at the cursor.
        app.set_automation_mode(t, AutomationMode::Read);
        app.settings.prefs.automation_off_defaults = false;
        app.set_automation_mode(t, AutomationMode::Off);
        assert_eq!(app.project().track(t).unwrap().automation.volume, 0.25);
        // Moving it now sets the held value (the envelope does not play).
        app.set_control(t, TrackControl::Volume, 0.75, key);
        let tr = app.project().track(t).unwrap();
        assert_eq!(tr.automation.volume, 0.75);
        assert_eq!(
            tr.automation_at(EnvelopeKind::Volume, Tick::ZERO),
            Some(0.75)
        );
    }

    /// The Mute button and the multipurpose slider as trims or automation
    /// controls, and a bus track's controls alike.
    #[test]
    fn mute_sends_and_bus_controls() {
        let (mut app, t) = app_with_audio();
        let key = egui::Id::new("test");
        let owner = AutoOwner::Track(t);
        // A trim: the track muted, and back.
        app.click_mute(owner, key);
        assert!(app.project().track(t).unwrap().muted);
        app.click_mute(owner, key);
        assert!(!app.project().track(t).unwrap().muted);
        // In Touch, stopped: the mute envelope's point at the cursor.
        app.toggle_automation_controls(t);
        app.set_automation_mode(t, AutomationMode::Touch);
        app.cursor = Tick::from_seconds(1);
        app.click_mute(owner, key);
        let tr = app.project().track(t).unwrap();
        assert!(!tr.muted, "the trim stays");
        let mute = tr
            .envelopes
            .iter()
            .find(|e| e.kind == EnvelopeKind::Mute)
            .unwrap();
        assert_eq!(mute.value_at(Tick::from_seconds(1)), 0.0);
        assert!(app.owner_muted(owner));
        // In Read it shows the envelope: a click does nothing.
        app.set_automation_mode(t, AutomationMode::Read);
        let before = app.project().clone();
        app.click_mute(owner, key);
        assert_eq!(*app.project(), before);
        // Off: the whole track.
        app.set_automation_mode(t, AutomationMode::Off);
        app.click_mute(owner, key);
        assert!(app.project().track(t).unwrap().muted);
        // The multipurpose slider: a trim, then the send's envelope, then
        // with automation off the level it holds.
        let bus = app.modify(|p| p.add_bus()).unwrap();
        let to = SendTo::Bus(bus);
        let send = TrackControl::Send(to);
        app.toggle_automation_controls(t);
        app.set_control(t, send, 0.5, key);
        let s = app.project().track(t).unwrap().sends[0];
        assert!((s.gain_db + 6.0206).abs() < 1e-3);
        app.toggle_automation_controls(t);
        app.set_automation_mode(t, AutomationMode::Latch);
        app.set_control(t, send, 0.25, key);
        let tr = app.project().track(t).unwrap();
        let env = tr
            .envelopes
            .iter()
            .find(|e| e.kind == EnvelopeKind::Send(to))
            .unwrap();
        assert!((env.value_at(Tick::from_seconds(1)) - 0.25).abs() < 1e-6);
        app.set_automation_mode(t, AutomationMode::Off);
        app.set_control(t, send, 0.75, key);
        assert_eq!(app.control_value(t, send), 0.75);
        // A bus track: its volume a trim; in Touch while playing its pan
        // recorded; with automation off the volume it holds.
        let strip = Strip::Bus(bus);
        let b = AutoOwner::Strip(strip);
        app.set_owner_control(b, TrackControl::Volume, 0.5, key);
        let c = app.project().strip_channel(strip).unwrap();
        assert!((c.gain_db + 6.0206).abs() < 1e-3);
        app.toggle_owner_automation_controls(b);
        app.set_owner_automation_mode(b, AutomationMode::Touch);
        let s = Tick::from_seconds;
        app.record_control(b, TrackControl::Pan, -1.0, s(1), false);
        app.record_control(b, TrackControl::Pan, -1.0, s(2), false);
        app.release_owner_control(b, TrackControl::Pan);
        assert!(app.automation_pass.is_none(), "the pass ended");
        let c = app.project().strip_channel(strip).unwrap();
        let pan = c
            .envelopes
            .iter()
            .find(|e| e.kind == EnvelopeKind::Pan)
            .unwrap();
        assert!((pan.value_at(Tick::from_seconds_f64(1.5)) + 1.0).abs() < 1e-6);
        app.set_owner_automation_mode(b, AutomationMode::Off);
        app.set_owner_control(b, TrackControl::Volume, 0.25, key);
        assert_eq!(app.owner_value(b, TrackControl::Volume), 0.25);
    }

    #[test]
    fn recorded_while_playing_and_one_undo() {
        let (mut app, t) = app_with_audio();
        app.toggle_automation_controls(t);
        app.set_automation_mode(t, AutomationMode::Touch);
        let undo = app.history.undo_len();
        // Playing (without sound: the playhead is set by hand): a fader
        // moved from 1 s to 2 s, then let go.
        let s = Tick::from_seconds;
        for (i, v) in [0.2f32, 0.4, 0.6, 0.8].into_iter().enumerate() {
            app.record_control(
                AutoOwner::Track(t),
                TrackControl::Volume,
                v,
                s(1) + Tick(s(1).0 / 3 * i as i64),
                false,
            );
        }
        app.release_control(t, TrackControl::Volume);
        assert!(app.automation_pass.is_none(), "the pass ended");
        assert_eq!(app.history.undo_len(), undo + 1, "one undo");
        let tr = app.project().track(t).unwrap();
        let env = tr
            .envelopes
            .iter()
            .find(|e| e.kind == EnvelopeKind::Volume)
            .unwrap();
        assert!((env.value_at(s(1)) - 0.2).abs() < 1e-4);
        assert!((env.value_at(s(2)) - 0.8).abs() < 1e-4);
        // Back to what it was (0 dB) after it, and before it.
        assert_eq!(env.value_at(s(3)), 1.0);
        assert_eq!(env.value_at(Tick(s(1).0 / 2)), 1.0);
        // A straight line thins to its ends.
        assert!(env.points.len() <= 5, "{}", env.points.len());
    }
}
