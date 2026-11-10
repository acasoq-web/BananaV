//! VST effects in the mixer's chains — VST 3 (`bv-vst3`) and VST 2
//! (`bv-vst2`, BananaV's own compatibility); 32-bit plug-ins of either
//! kind in BananaV's 32-bit bridge: an instance of the plug-in for each chain, given the effect's
//! state when made, then following its state and its parameters as they
//! change (no instance made again, not even after a seek: plug-ins take
//! long to start; a seek clears their sound instead). Each block tells the
//! plug-in where it lies on the timeline and the project's tempo and meter
//! (tempo-synced delays and LFOs follow them); FX automation reaches it as
//! ramps, sample by sample. A plug-in whose input and output have been
//! silent for longer than its tail (a second at least) sleeps — it is not
//! called — until sound comes again. A plug-in that cannot load, or
//! crashed (the hosts' guards, the bridge's process), lets the sound
//! through.

use std::collections::BTreeMap;

use bv_timeline::{Tempo, VstFormat, VstFx};

/// The longest block a plug-in is handed (longer ones go in pieces).
const BLOCK: usize = 4096;

/// Below this a sample is silence (−140 dB).
const SILENCE: f32 = 1e-7;

/// A plug-in of either kind.
#[derive(Debug)]
enum Plugin {
    V3(Box<bv_vst3::Instance>),
    /// A 32-bit VST 3 module, in the bridge.
    V3Bridged(Box<bv_vst3::Bridged>),
    V2(bv_vst2::Instance),
}

/// Forwards a call to the plug-in, whichever kind.
macro_rules! both {
    ($plugin:expr, $p:ident => $e:expr) => {
        match $plugin {
            Plugin::V3($p) => $e,
            Plugin::V3Bridged($p) => $e,
            Plugin::V2($p) => $e,
        }
    };
}

impl Plugin {
    fn make(v: &VstFx) -> Option<Plugin> {
        match v.format {
            VstFormat::Vst3 if bv_vst3::is_32bit(&v.path) => {
                bv_vst3::Bridged::spawn(&v.path, &v.class_id, false)
                    .ok()
                    .map(|b| Plugin::V3Bridged(Box::new(b)))
            }
            VstFormat::Vst3 => {
                let module = bv_vst3::load(&v.path).ok()?;
                let class = module.effect(&v.class_id)?.clone();
                bv_vst3::Instance::new(&module, &class, false)
                    .ok()
                    .map(|i| Plugin::V3(Box::new(i)))
            }
            VstFormat::Vst2 => bv_vst2::Instance::new(&v.path, bv_vst2::shell_id(&v.class_id))
                .ok()
                .map(Plugin::V2),
        }
    }

    fn set_state(&mut self, state: &[u8]) {
        match self {
            Plugin::V3(p) => {
                p.set_state(state, &[]);
            }
            Plugin::V3Bridged(p) => {
                p.set_state(state, &[]);
            }
            Plugin::V2(p) => {
                p.set_state(state);
            }
        }
    }

    fn setup(&mut self, rate: u32, channels: usize) -> bool {
        both!(self, p => p.setup(f64::from(rate), BLOCK, channels).is_ok())
    }

    fn queue(&mut self, id: u32, value: f64) {
        both!(self, p => p.queue(id, value))
    }

    fn queue_ramp(&mut self, id: u32, points: &[(usize, f64)]) {
        both!(self, p => p.queue_ramp(id, points))
    }

    fn has_queued(&self) -> bool {
        both!(self, p => p.has_queued())
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        both!(self, p => p.process(channels))
    }

    fn locate(&mut self, position: i64, tempo: &Tempo) {
        let (playing, bpm) = (true, tempo.beats_per_minute);
        let numerator = i32::try_from(tempo.beats_per_measure).unwrap_or(4);
        let denominator = i32::try_from(tempo.beat_note).unwrap_or(4);
        let v3 = bv_vst3::Transport {
            playing,
            tempo: bpm,
            numerator,
            denominator,
            cycle: None,
        };
        match self {
            Plugin::V3(p) => {
                p.locate(position);
                p.set_transport(v3);
            }
            Plugin::V3Bridged(p) => {
                p.locate(position);
                p.set_transport(v3);
            }
            Plugin::V2(p) => {
                p.locate(position);
                p.set_transport(bv_vst2::Transport {
                    playing,
                    tempo: bpm,
                    numerator,
                    denominator,
                    cycle: None,
                });
            }
        }
    }

    fn latency(&self) -> u32 {
        both!(self, p => p.latency())
    }

    fn tail(&self) -> Option<u32> {
        both!(self, p => p.tail())
    }

    #[cfg(test)]
    fn state(&self) -> Option<Vec<u8>> {
        both!(self, p => p.state())
    }
}

/// A VST effect running.
#[derive(Debug)]
pub(crate) struct Run {
    plugin: Plugin,
    /// The state it was given and the parameters' values it has
    /// (normalized).
    state: Vec<u8>,
    applied: BTreeMap<u32, f64>,
    rate: u32,
    channels: usize,
    /// How long its sound lasts after its input stops (`None`: for ever).
    tail: Option<u32>,
    /// Samples in a row its input and output were silent.
    quiet: usize,
}

impl Run {
    /// The effect `v` set up for `channels` channels at `rate`; `None`
    /// when its plug-in cannot load or refuses.
    pub(crate) fn new(v: &VstFx, rate: u32, channels: usize) -> Option<Run> {
        let mut plugin = Plugin::make(v)?;
        if !v.state.is_empty() {
            plugin.set_state(&v.state);
        }
        if !plugin.setup(rate, channels) {
            return None;
        }
        for (&id, &value) in &v.params {
            plugin.queue(id, value);
        }
        let tail = plugin.tail();
        Some(Run {
            plugin,
            state: v.state.clone(),
            applied: v.params.clone(),
            rate,
            channels,
            tail,
            quiet: 0,
        })
    }

    /// Takes `v`'s state when it changed, and the values of its parameters
    /// that changed (the next block gets them).
    pub(crate) fn follow(&mut self, v: &VstFx) {
        if v.state != self.state {
            if !v.state.is_empty() {
                self.plugin.set_state(&v.state);
            }
            self.state = v.state.clone();
            self.applied.clear();
        }
        for (&id, &value) in &v.params {
            if self.applied.get(&id) != Some(&value) {
                self.plugin.queue(id, value);
                self.applied.insert(id, value);
            }
        }
    }

    /// Where its next block lies on the timeline (samples) and the
    /// project's tempo and meter: the timeline plays.
    pub(crate) fn locate(&mut self, position: i64, tempo: &Tempo) {
        self.plugin.locate(position, tempo);
    }

    /// Parameter `id`'s values along the next block (sample, normalized
    /// value): FX automation's envelope, sample by sample.
    pub(crate) fn ramp(&mut self, id: u32, points: &[(usize, f64)]) {
        if let Some(&(_, last)) = points.last() {
            self.plugin.queue_ramp(id, points);
            self.applied.insert(id, last);
        }
    }

    pub(crate) fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let silent = |chs: &[Vec<f32>]| chs.iter().flatten().all(|s| s.abs() < SILENCE);
        let quiet_in = silent(channels);
        // Asleep: silence in, silence out, nothing to change.
        let sleep_after = self
            .tail
            .map_or(usize::MAX, |t| (t as usize).max(self.rate as usize));
        if quiet_in && self.quiet >= sleep_after && !self.plugin.has_queued() {
            return;
        }
        self.plugin.process(channels);
        if quiet_in && silent(channels) {
            self.quiet = self.quiet.saturating_add(n);
        } else {
            self.quiet = 0;
        }
    }

    /// After a seek: its sound cleared (its delay lines and reverb tails
    /// left behind), the plug-in kept. Whether it could.
    pub(crate) fn restart(&mut self) -> bool {
        let ok = self.plugin.setup(self.rate, self.channels);
        self.quiet = 0;
        ok
    }

    /// How many samples late its output comes (the plug-in's latency).
    pub(crate) fn latency(&self) -> usize {
        self.plugin.latency() as usize
    }

    /// The plug-in's state (for the tests).
    #[cfg(test)]
    pub(crate) fn plugin_state(&self) -> Option<Vec<u8>> {
        self.plugin.state()
    }
}
