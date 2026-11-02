//! VST 3 effects in the mixer's chains (`bv-vst3`): an instance of the
//! plug-in for each chain, given the effect's state when made, then
//! following its state and its parameters as they change (no instance made
//! again: plug-ins take long to start); a plug-in that cannot load lets
//! the sound through.

use std::collections::BTreeMap;

use bv_timeline::VstFx;

/// The longest block a plug-in is handed (longer ones go in pieces).
const BLOCK: usize = 4096;

/// A VST 3 effect running.
#[derive(Debug)]
pub(crate) struct Run {
    instance: bv_vst3::Instance,
    /// The state it was given and the parameters' values it has
    /// (normalized).
    state: Vec<u8>,
    applied: BTreeMap<u32, f64>,
}

impl Run {
    /// The effect `v` set up for `channels` channels at `rate`; `None`
    /// when its plug-in cannot load or refuses.
    pub(crate) fn new(v: &VstFx, rate: u32, channels: usize) -> Option<Run> {
        let module = bv_vst3::load(&v.path).ok()?;
        let class = module.effect(&v.class_id)?.clone();
        let mut instance = bv_vst3::Instance::new(&module, &class, false).ok()?;
        if !v.state.is_empty() {
            instance.set_state(&v.state, &[]);
        }
        instance.setup(f64::from(rate), BLOCK, channels).ok()?;
        for (&id, &value) in &v.params {
            instance.queue(id, value);
        }
        Some(Run {
            instance,
            state: v.state.clone(),
            applied: v.params.clone(),
        })
    }

    /// Takes `v`'s state when it changed, and the values of its parameters
    /// that changed (the next block gets them).
    pub(crate) fn follow(&mut self, v: &VstFx) {
        if v.state != self.state {
            if !v.state.is_empty() {
                self.instance.set_state(&v.state, &[]);
            }
            self.state = v.state.clone();
            self.applied.clear();
        }
        for (&id, &value) in &v.params {
            if self.applied.get(&id) != Some(&value) {
                self.instance.queue(id, value);
                self.applied.insert(id, value);
            }
        }
    }

    pub(crate) fn process(&mut self, channels: &mut [Vec<f32>]) {
        self.instance.process(channels);
    }

    /// How many samples late its output comes (the plug-in's latency).
    pub(crate) fn latency(&self) -> usize {
        self.instance.latency() as usize
    }
}
