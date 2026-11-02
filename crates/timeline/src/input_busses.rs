//! Input busses and input monitoring, as VEGAS's Help describes them
//! ("Using input busses", "Recording audio"): an input bus is an input of
//! the computer (a device's channels) through its effects (always before
//! its fader), volume and pan, heard through the Master unless its output
//! is off, and a recording input tracks can take — the signal recorded
//! with its effects —; its sends go to assignable effects chains (after
//! its volume and mute, or before them). Up to 26 (A to Z). A track's input monitoring plays
//! what it records through its own effects while it is armed: On or Auto
//! (Auto: when playback is stopped and while recording, and in a
//! punch-in only over the selected events).

use serde::{Deserialize, Serialize};

use crate::fx::Effect;
use crate::model::{Project, RecordInput};

/// How many input busses a project may have (A to Z).
pub const MAX_INPUT_BUSSES: usize = 26;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct InputBusId(pub u32);

/// An input bus.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputBus {
    pub id: InputBusId,
    /// The user's name; empty: the default ("Input Bus A"…).
    #[serde(default)]
    pub name: String,
    /// Its Input Source (never an input bus itself).
    #[serde(default)]
    pub input: RecordInput,
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default)]
    pub pan: f32,
    #[serde(default)]
    pub muted: bool,
    /// Its Output: the Master (heard), else off — a recording input only,
    /// heard through the track that records it.
    #[serde(default = "yes")]
    pub to_master: bool,
    /// Its sends to assignable effects chains (those at no level left
    /// out).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sends: Vec<crate::busses::Send>,
}

fn yes() -> bool {
    true
}

/// A track's input monitoring (its Record Input menu).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputMonitor {
    #[default]
    Off,
    /// While armed: when playback is stopped and while recording, the
    /// whole recording.
    On,
    /// As On, but in a punch-in only over the selected events.
    Auto,
}

impl InputMonitor {
    pub fn is_off(&self) -> bool {
        *self == InputMonitor::Off
    }
}

impl Project {
    /// A new input bus taking `input`, the next letter; `None` past 26.
    pub fn add_input_bus(&mut self, input: RecordInput) -> Option<InputBusId> {
        if self.input_busses.len() >= MAX_INPUT_BUSSES {
            return None;
        }
        let id = InputBusId(
            self.input_busses
                .iter()
                .map(|b| b.id.0 + 1)
                .max()
                .unwrap_or(1),
        );
        self.input_busses.push(InputBus {
            id,
            name: String::new(),
            input: RecordInput { bus: None, ..input },
            effects: Vec::new(),
            gain_db: 0.0,
            pan: 0.0,
            muted: false,
            to_master: true,
            sends: Vec::new(),
        });
        Some(id)
    }

    /// Input bus `id`'s send to an assignable effects chain (VEGAS's Help:
    /// an input bus sends to chains, or to busses that play to hardware
    /// outputs, which BananaV has not); one at no level, from after the
    /// volume, is taken away.
    pub fn set_input_bus_send(&mut self, id: InputBusId, send: crate::busses::Send) -> bool {
        let crate::busses::SendTo::Fx(chain) = send.to else {
            return false;
        };
        if self.fx_chain(chain).is_none() {
            return false;
        }
        let Some(b) = self.input_bus_mut(id) else {
            return false;
        };
        b.sends.retain(|s| s.to != send.to);
        if send.gain_db > -96.0 || send.pre_volume {
            b.sends.push(send);
        }
        true
    }

    /// Deletes input bus `id`: tracks that recorded from it record from its
    /// input instead.
    pub fn remove_input_bus(&mut self, id: InputBusId) -> bool {
        let Some(i) = self.input_busses.iter().position(|b| b.id == id) else {
            return false;
        };
        let bus = self.input_busses.remove(i);
        for t in &mut self.tracks {
            if let Some(r) = &mut t.record_input
                && r.bus == Some(id)
            {
                *r = RecordInput {
                    bus: None,
                    ..bus.input.clone()
                };
            }
        }
        true
    }

    pub fn input_bus(&self, id: InputBusId) -> Option<&InputBus> {
        self.input_busses.iter().find(|b| b.id == id)
    }

    pub fn input_bus_mut(&mut self, id: InputBusId) -> Option<&mut InputBus> {
        self.input_busses.iter_mut().find(|b| b.id == id)
    }

    /// Its letter's place (0: A).
    pub fn input_bus_index(&self, id: InputBusId) -> Option<usize> {
        self.input_busses.iter().position(|b| b.id == id)
    }

    /// What a track recording from `input` takes: its device and channels
    /// (an input bus's), and the effects the signal goes through first (an
    /// input bus's).
    pub fn resolved_input(&self, input: &RecordInput) -> (RecordInput, Vec<Effect>) {
        match input.bus.and_then(|b| self.input_bus(b)) {
            Some(bus) => (
                RecordInput {
                    bus: None,
                    ..bus.input.clone()
                },
                bus.effects.clone(),
            ),
            None => (
                RecordInput {
                    bus: None,
                    ..input.clone()
                },
                Vec::new(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ProjectSettings, TrackKind};

    /// Input busses get the next letters up to Z; a track recording from
    /// one takes its input and effects, and its device when it goes.
    #[test]
    fn input_busses_come_and_go() {
        let mut p = Project::new(ProjectSettings::default());
        let mic = RecordInput {
            device: "Mic".into(),
            first: 1,
            channels: 1,
            bus: None,
        };
        let a = p.add_input_bus(mic.clone()).unwrap();
        for _ in 1..MAX_INPUT_BUSSES {
            p.add_input_bus(RecordInput::default()).unwrap();
        }
        assert!(
            p.add_input_bus(RecordInput::default()).is_none(),
            "26 at most"
        );
        p.input_bus_mut(a)
            .unwrap()
            .effects
            .push(Effect::new(crate::fx::EffectKind::Gain));
        let t = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .unwrap()
            .id;
        let via = RecordInput {
            bus: Some(a),
            ..RecordInput::default()
        };
        p.track_by_id_mut(t).unwrap().record_input = Some(via.clone());
        let (input, effects) = p.resolved_input(&via);
        assert_eq!(
            (input.device.as_str(), input.first, input.channels),
            ("Mic", 1, 1)
        );
        assert_eq!(effects.len(), 1);
        assert!(p.remove_input_bus(a));
        let now = p.track(t).unwrap().record_input.clone().unwrap();
        assert_eq!((now.device.as_str(), now.bus), ("Mic", None));
    }

    /// An input bus sends to chains only; its sends go with their chain.
    #[test]
    fn input_busses_send_to_chains() {
        use crate::busses::{BusId, Send, SendTo};
        let mut p = Project::new(ProjectSettings::default());
        let a = p.add_input_bus(RecordInput::default()).unwrap();
        let fx = p.add_fx_chain(Vec::new()).unwrap();
        let bus = p.add_bus().unwrap();
        let mut send = Send::new(SendTo::Fx(fx));
        send.gain_db = -6.0;
        assert!(p.set_input_bus_send(a, send));
        assert!(!p.set_input_bus_send(a, Send::new(SendTo::Bus(bus))));
        assert!(!p.set_input_bus_send(a, Send::new(SendTo::Bus(BusId(99)))));
        assert_eq!(p.input_bus(a).unwrap().sends, vec![send]);
        // Back at no level: gone.
        assert!(p.set_input_bus_send(a, Send::new(SendTo::Fx(fx))));
        assert!(p.input_bus(a).unwrap().sends.is_empty());
        assert!(p.set_input_bus_send(a, send));
        assert!(p.remove_fx_chain(fx));
        assert!(p.input_bus(a).unwrap().sends.is_empty());
    }
}
