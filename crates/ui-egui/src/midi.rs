//! The MIDI ports BananaV has open (Options › Preferences › MIDI), each
//! once, shared by MIDI Thru (an input played out on an output at once),
//! the timecode and clock it sends, following timecode and the control
//! surfaces. They open as the preferences and the Options menu ask, and
//! close while another program is active when Options › Preferences ›
//! General says so.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bv_core::Tick;

use crate::app::BananaApp;
use crate::settings::{LocateMessages, MidiRoute, MtcRate};
use crate::sync::{ChaseAction, ChaseSettings, ClockOut, Generator, MtcDecoder, MtcOut, Transport};

/// Short messages from the control surfaces' inputs: (port, bytes).
pub(crate) type Incoming = Arc<Mutex<Vec<(String, [u8; 3])>>>;

/// Messages kept for the control surfaces between frames, at most.
const QUEUE: usize = 4096;

#[derive(Debug, Default)]
pub(crate) struct MidiPorts {
    /// What the ports are open for.
    plan: Option<Plan>,
    pub outputs: HashMap<String, Arc<bv_platform::MidiOut>>,
    inputs: Vec<bv_platform::MidiIn>,
    pub incoming: Incoming,
    /// What the port Configure Generic Control learns from sends.
    pub learned: Incoming,
    /// Timecode coming in (Options › Timecode › Trigger from MIDI
    /// Timecode).
    pub mtc_in: Arc<Mutex<MtcDecoder>>,
    /// Sending MIDI timecode and clock.
    pub generator: Option<Generator>,
    /// Playback as last told to the generator: playing, where, when; and
    /// how many times it started, jumped or (stopped) the cursor moved.
    told: Option<(bool, Tick, Instant)>,
    serial: u64,
}

/// What the ports are open for.
#[derive(Clone, Debug, Default, PartialEq)]
struct Plan {
    thru: Vec<MidiRoute>,
    mtc_out: Option<(String, MtcRate, LocateMessages)>,
    clock_out: Option<(String, bool, LocateMessages)>,
    mtc_in: Option<String>,
    surfaces: Vec<(Option<String>, Option<String>)>,
    /// The input Configure Generic Control's Learn listens to.
    learn: Option<String>,
}

impl MidiPorts {
    /// Closes everything and opens what `plan` needs; what did not open.
    fn reopen(&mut self, plan: Plan) -> Vec<String> {
        // Inputs first: they send on outputs.
        self.inputs.clear();
        self.generator = None;
        self.outputs.clear();
        let mut errors = Vec::new();
        let mut outputs: Vec<&String> = plan.thru.iter().map(|r| &r.output).collect();
        outputs.extend(plan.mtc_out.iter().map(|m| &m.0));
        outputs.extend(plan.clock_out.iter().map(|c| &c.0));
        outputs.extend(plan.surfaces.iter().filter_map(|s| s.1.as_ref()));
        outputs.sort();
        outputs.dedup();
        for name in outputs {
            match bv_platform::MidiOut::open(name) {
                Ok(o) => {
                    self.outputs.insert(name.clone(), Arc::new(o));
                }
                Err(e) => errors.push(e),
            }
        }
        let mut inputs: Vec<&String> = plan.thru.iter().map(|r| &r.input).collect();
        inputs.extend(plan.mtc_in.iter());
        inputs.extend(plan.surfaces.iter().filter_map(|s| s.0.as_ref()));
        inputs.extend(plan.learn.iter());
        inputs.sort();
        inputs.dedup();
        for name in inputs {
            let thru = plan
                .thru
                .iter()
                .find(|r| &r.input == name)
                .and_then(|r| self.outputs.get(&r.output).cloned());
            let mtc = (plan.mtc_in.as_ref() == Some(name)).then(|| self.mtc_in.clone());
            let surface = plan
                .surfaces
                .iter()
                .any(|s| s.0.as_ref() == Some(name))
                .then(|| self.incoming.clone());
            let learn = (plan.learn.as_ref() == Some(name)).then(|| self.learned.clone());
            let port = name.clone();
            let handler: bv_platform::MidiHandler = Box::new(move |bytes| {
                if let Some(out) = &thru {
                    out.send(&bytes);
                }
                if let Some(m) = &mtc
                    && bytes[0] == 0xF1
                    && let Ok(mut d) = m.lock()
                {
                    d.quarter_frame(bytes[1], Instant::now());
                }
                for q in [&surface, &learn].into_iter().flatten() {
                    if let Ok(mut q) = q.lock()
                        && q.len() < QUEUE
                    {
                        q.push((port.clone(), bytes));
                    }
                }
            });
            match bv_platform::MidiIn::open(name, handler) {
                Ok(i) => self.inputs.push(i),
                Err(e) => errors.push(e),
            }
        }
        let mtc = plan.mtc_out.as_ref().and_then(|(name, rate, full)| {
            Some(MtcOut {
                port: self.outputs.get(name)?.clone(),
                rate: *rate,
                full_frames: *full,
            })
        });
        let clock = plan.clock_out.as_ref().and_then(|(name, start, spp)| {
            Some(ClockOut {
                port: self.outputs.get(name)?.clone(),
                send_start: *start,
                song_position: *spp,
            })
        });
        if mtc.is_some() || clock.is_some() {
            self.generator = Some(Generator::start(mtc, clock));
        }
        self.told = None;
        self.plan = Some(plan);
        errors
    }
}

impl BananaApp {
    /// Opens the MIDI ports the preferences and the Options menu need,
    /// closing the others; `active`: BananaV is the active program. A port
    /// that does not open is told once, and tried again when what is needed
    /// changes.
    pub(crate) fn sync_midi(&mut self, active: bool) {
        // Asked for again each frame while Learn is on.
        let learn = self.midi_learn.take();
        let p = &self.settings.prefs;
        let open = active || !p.release_audio_inactive;
        let output = |n: &Option<String>| n.clone().filter(|n| p.midi_outputs.contains(n));
        let input = |n: &Option<String>| n.clone().filter(|n| p.midi_inputs.contains(n));
        let plan = if open {
            Plan {
                thru: p
                    .midi_thru
                    .iter()
                    .filter(|r| p.midi_inputs.contains(&r.input))
                    .filter(|r| p.midi_outputs.contains(&r.output))
                    .cloned()
                    .collect(),
                mtc_out: self
                    .generate_mtc
                    .then(|| output(&p.mtc_output))
                    .flatten()
                    .map(|n| (n, p.mtc_output_rate, p.mtc_full_frames)),
                clock_out: self
                    .generate_clock
                    .then(|| output(&p.clock_output))
                    .flatten()
                    .map(|n| (n, p.clock_send_start, p.clock_song_position)),
                mtc_in: self.trigger_mtc.then(|| input(&p.mtc_input)).flatten(),
                surfaces: if p.external_control {
                    p.control_devices
                        .iter()
                        .map(|d| (input(&d.input), output(&d.output)))
                        .collect()
                } else {
                    Vec::new()
                },
                learn,
            }
        } else {
            Plan::default()
        };
        if self.midi.plan.as_ref() == Some(&plan) {
            return;
        }
        for e in self.midi.reopen(plan) {
            self.notify(trf!("MIDI: {}", e));
        }
    }

    /// Each frame: tells the timecode and clock generator where playback
    /// is.
    pub(crate) fn publish_transport(&mut self) {
        if self.midi.generator.is_none() {
            return;
        }
        let playing = self.is_playing();
        let position = if playing {
            self.playhead.unwrap_or(self.cursor)
        } else {
            self.cursor
        };
        let now = Instant::now();
        // Started, stopped, jumped, or (stopped) the cursor moved.
        let changed = match self.midi.told {
            Some((true, at, when)) if playing => {
                let expected =
                    at + Tick::from_seconds_f64(when.elapsed().as_secs_f64() * self.play_rate);
                (position - expected).abs() > Tick::from_seconds_f64(0.25)
            }
            Some((false, at, _)) if !playing => at != position,
            _ => true,
        };
        if changed {
            self.midi.serial += 1;
        }
        self.midi.told = Some((playing, position, now));
        let p = self.project();
        let t = Transport {
            playing,
            position,
            at: now,
            rate: self.play_rate,
            offset: p.ruler_offset,
            beats_per_minute: p.settings.tempo.beats_per_minute,
            serial: self.midi.serial,
        };
        if let Some(g) = &self.midi.generator {
            g.publish(t);
        }
    }

    /// Each frame: Options › Timecode › Trigger from MIDI Timecode — playback
    /// cued, started and kept in step by the timecode coming in, stopped
    /// when it is gone.
    pub(crate) fn follow_timecode(&mut self) {
        if !self.trigger_mtc {
            self.chase = Default::default();
            return;
        }
        let latest = self.midi.mtc_in.lock().ok().and_then(|d| d.latest);
        let p = &self.settings.prefs;
        let settings = ChaseSettings {
            rate: p.mtc_input_rate,
            freewheel: p.mtc_freewheel,
            slack_frames: p.mtc_slack_frames,
            freewheel_seconds: p.mtc_freewheel_seconds,
            sync_delay: p.mtc_sync_delay,
            offset_quarters: p.mtc_offset_quarters,
        };
        let playhead = self.playhead.filter(|_| self.is_playing());
        let offset = self.project().ruler_offset;
        match self
            .chase
            .step(latest, Instant::now(), playhead, offset, &settings)
        {
            ChaseAction::Nothing => {}
            ChaseAction::Cue(t) => self.seek(t),
            ChaseAction::Play(t) => {
                if self.is_playing() {
                    self.seek_playback(t);
                } else {
                    self.cursor = t;
                    self.start_playback(t);
                }
            }
            ChaseAction::Stop => self.stop(),
        }
    }
}
