//! Audio busses and assignable effects chains, as VEGAS mixes them: a
//! track plays into its output — the Master bus or an audio bus — and
//! sends to busses and chains from before its volume and mute or after
//! them, panned as the track or from the middle; a bus mixes what reaches
//! it through its effects (before its volume, or after it), pan and volume
//! into its own output — the Master or another bus — and sends to chains
//! from before its pan, volume and mute or after them; a chain takes what
//! is sent to it at its input level through its effects, pan and volume
//! into the Master or a bus. Nothing plays in a circle. Up to 26 busses (A
//! to Z) and 32 chains.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::envelope::EnvelopeKind;
use crate::fx::Effect;
use crate::model::{AutomationMode, Project, TrackAutomation, TrackId};

/// How many busses (A to Z) and chains a project may have.
pub const MAX_BUSSES: usize = 26;
pub const MAX_FX_CHAINS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BusId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FxChainId(pub u32);

/// Where a track, a bus or a chain plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Output {
    #[default]
    Master,
    Bus(BusId),
    /// A pair of the output device's channels (0: its first two), as
    /// VEGAS's hardware outputs: heard while playing, left out of
    /// renders. Only busses play there.
    Hardware(u8),
}

/// What a track or a bus sends to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SendTo {
    Bus(BusId),
    Fx(FxChainId),
}

/// A line of Render's channel mapping (VEGAS's Channel Mapping dialog): the
/// Master bus (`bus` `None`) or an audio bus, whether its sound goes to
/// the rendered file, and whether as one channel (its Mono downmix)
/// rather than its layout's (two for stereo).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MappedStrip {
    pub bus: Option<BusId>,
    pub on: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mono: bool,
}

/// A track's send to a bus or a chain, or a bus's to a chain.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Send {
    pub to: SendTo,
    /// Its level (dB; at or under −96, nothing).
    pub gain_db: f32,
    /// Taken before the volume and the mute (a track's after its effects,
    /// a bus's before its pan); else after them.
    pub pre_volume: bool,
    /// A track's: panned as the track is (its pan envelope too); else from
    /// the middle, with the track's panning mode.
    #[serde(default)]
    pub link_pan: bool,
    /// A track's, with its automation off: the level its automation control
    /// (the multipurpose slider) holds, linear over `gain_db` (1: as it is).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub held: f32,
}

fn one() -> f32 {
    1.0
}

fn is_one(v: &f32) -> bool {
    *v == 1.0
}

impl Send {
    /// A send to `to` at no level, as VEGAS's start: to a bus from before
    /// the volume (a cue mix), to a chain from after it.
    pub fn new(to: SendTo) -> Send {
        Send {
            to,
            gain_db: f32::NEG_INFINITY,
            pre_volume: matches!(to, SendTo::Bus(_)),
            link_pan: false,
            held: 1.0,
        }
    }

    /// A bus's send to `to` at no level: from after its volume and mute,
    /// as VEGAS's start.
    pub fn of_bus(to: SendTo) -> Send {
        Send {
            pre_volume: false,
            ..Send::new(to)
        }
    }
}

/// A bus or a chain: a strip of the Mixing Console, a step of the routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Strip {
    Bus(BusId),
    Chain(FxChainId),
}

/// What a bus and a chain share: their effects, levels and output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    /// The user's name; empty: the default ("Bus A", "FX 1"…).
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default)]
    pub pan: f32,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub solo: bool,
    #[serde(default)]
    pub effects: Vec<Effect>,
    /// Its effects after its volume (Post Fader Insert FX); else before.
    #[serde(default)]
    pub fx_post_fader: bool,
    #[serde(default)]
    pub output: Output,
    /// Its volume, pan and mute over time (its bus track's envelopes).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub envelopes: Vec<crate::envelope::Envelope>,
    /// 5.1 projects: its Surround Panner (`None`, never panned: in the
    /// middle of the room, `surround::MIDDLE`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surround: Option<crate::surround::SurroundPan>,
    /// Its bus track's Automation Settings, as a track's: its volume, pan
    /// and mute controls trims or automation controls, how its envelopes
    /// play and record, and what they hold with automation off.
    #[serde(default)]
    pub automation: TrackAutomation,
}

impl Channel {
    /// Its `kind` automation at `t`: its envelope's, or with automation off
    /// what its automation controls hold (volume, pan).
    pub fn automation_at(&self, kind: EnvelopeKind, t: bv_core::Tick) -> Option<f32> {
        if self.automation.mode == AutomationMode::Off {
            return match kind {
                EnvelopeKind::Volume => Some(self.automation.volume),
                EnvelopeKind::Pan => Some(self.automation.pan),
                _ => None,
            };
        }
        self.envelopes
            .iter()
            .find(|e| e.kind == kind)
            .map(|e| e.value_at(t))
    }
}

impl Output {
    pub fn is_master(&self) -> bool {
        *self == Output::Master
    }

    pub fn is_hardware(&self) -> bool {
        matches!(self, Output::Hardware(_))
    }
}

impl Default for Channel {
    fn default() -> Channel {
        Channel {
            name: String::new(),
            gain_db: 0.0,
            pan: 0.0,
            muted: false,
            solo: false,
            effects: Vec::new(),
            fx_post_fader: false,
            output: Output::Master,
            envelopes: Vec::new(),
            surround: None,
            automation: TrackAutomation::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bus {
    pub id: BusId,
    /// Its sends to assignable effects chains (the Mixing Console's
    /// Sends).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sends: Vec<Send>,
    #[serde(flatten)]
    pub channel: Channel,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FxChain {
    pub id: FxChainId,
    /// Its input level (dB): what is sent to it, before its effects.
    #[serde(default)]
    pub input_db: f32,
    #[serde(flatten)]
    pub channel: Channel,
}

/// A bus's letter by its place (A to Z).
pub fn bus_letter(index: usize) -> char {
    (b'A' + (index.min(25) as u8)) as char
}

// ---- Editing ----------------------------------------------------------------------

impl Project {
    /// Busses, chains, routes and sends as they must be: at most 26
    /// busses and 32 chains, each once; outputs and sends to ones that
    /// exist (a bus's to chains); nothing playing in a circle.
    pub(crate) fn check_busses(&self) -> Result<(), String> {
        if self.busses.len() > MAX_BUSSES || self.fx_chains.len() > MAX_FX_CHAINS {
            return Err("too many busses or chains".into());
        }
        let mut ids = HashSet::new();
        if !self.busses.iter().all(|b| ids.insert(b.id)) {
            return Err("duplicate bus".into());
        }
        let mut chains = HashSet::new();
        if !self.fx_chains.iter().all(|c| chains.insert(c.id)) {
            return Err("duplicate chain".into());
        }
        let exists = |o: &Output| match o {
            Output::Master | Output::Hardware(_) => true,
            Output::Bus(b) => ids.contains(b),
        };
        // Busses send to chains, and to busses on hardware outputs.
        let hardware: HashSet<BusId> = self
            .busses
            .iter()
            .filter(|b| b.channel.output.is_hardware())
            .map(|b| b.id)
            .collect();
        for b in &self.busses {
            if !exists(&b.channel.output) {
                return Err(format!("bus {:?} routed to no bus", b.id));
            }
            if !b.sends.iter().all(|s| match s.to {
                SendTo::Fx(c) => chains.contains(&c),
                SendTo::Bus(to) => to != b.id && hardware.contains(&to),
            }) {
                return Err(format!("bus {:?} sends to what it may not", b.id));
            }
        }
        for c in &self.fx_chains {
            if !exists(&c.channel.output) || c.channel.output.is_hardware() {
                return Err(format!("chain {:?} routed to no bus", c.id));
            }
        }
        for s in self.strips() {
            if self.strip_feeds(s).into_iter().any(|n| self.reaches(n, s)) {
                return Err(format!("{s:?} plays in a circle"));
            }
        }
        for t in &self.tracks {
            if !exists(&t.output) || t.output.is_hardware() {
                return Err(format!("track {:?} routed to no bus", t.id));
            }
            for s in &t.sends {
                let ok = match s.to {
                    SendTo::Bus(b) => ids.contains(&b),
                    SendTo::Fx(c) => chains.contains(&c),
                };
                if !ok {
                    return Err(format!("track {:?} sends to nothing", t.id));
                }
            }
            // A send's envelope rides on its send.
            for e in &t.envelopes {
                if let crate::envelope::EnvelopeKind::Send(to) = e.kind
                    && !t.sends.iter().any(|s| s.to == to)
                {
                    return Err(format!(
                        "track {:?} has a send envelope without its send",
                        t.id
                    ));
                }
            }
        }
        Ok(())
    }

    /// Render's channel mapping as its dialog lists it: the lines kept, in
    /// their order (those of busses deleted since left out), then the
    /// Master and the busses not in it yet, not mapped.
    pub fn channel_mapping_rows(&self) -> Vec<MappedStrip> {
        let mut rows: Vec<MappedStrip> = self
            .channel_mapping
            .iter()
            .copied()
            .filter(|m| m.bus.is_none_or(|id| self.bus(id).is_some()))
            .collect();
        for bus in std::iter::once(None).chain(self.busses.iter().map(|b| Some(b.id))) {
            if !rows.iter().any(|m| m.bus == bus) {
                rows.push(MappedStrip {
                    bus,
                    on: false,
                    mono: false,
                });
            }
        }
        rows
    }

    /// The strips whose sound multichannel renders put in the file's
    /// channels, in order (none: no multichannel file).
    pub fn mapped_strips(&self) -> Vec<MappedStrip> {
        self.channel_mapping_rows()
            .into_iter()
            .filter(|m| m.on)
            .collect()
    }

    /// How many channels the mapping fills: each strip's layout's, or one
    /// mixed down.
    pub fn mapped_channels(&self) -> usize {
        let n = self.settings.channels.channel_count();
        self.mapped_strips()
            .iter()
            .map(|m| if m.mono { 1 } else { n })
            .sum()
    }

    /// Bus `id`'s place among the busses (its letter).
    pub fn bus_index(&self, id: BusId) -> Option<usize> {
        self.busses.iter().position(|b| b.id == id)
    }

    pub fn bus(&self, id: BusId) -> Option<&Bus> {
        self.busses.iter().find(|b| b.id == id)
    }

    pub fn bus_mut(&mut self, id: BusId) -> Option<&mut Bus> {
        self.busses.iter_mut().find(|b| b.id == id)
    }

    pub fn fx_chain(&self, id: FxChainId) -> Option<&FxChain> {
        self.fx_chains.iter().find(|c| c.id == id)
    }

    pub fn fx_chain_mut(&mut self, id: FxChainId) -> Option<&mut FxChain> {
        self.fx_chains.iter_mut().find(|c| c.id == id)
    }

    /// The busses, then the chains.
    pub fn strips(&self) -> Vec<Strip> {
        self.busses
            .iter()
            .map(|b| Strip::Bus(b.id))
            .chain(self.fx_chains.iter().map(|c| Strip::Chain(c.id)))
            .collect()
    }

    /// A bus's or a chain's channel.
    pub fn strip_channel(&self, s: Strip) -> Option<&Channel> {
        match s {
            Strip::Bus(id) => self.bus(id).map(|b| &b.channel),
            Strip::Chain(id) => self.fx_chain(id).map(|c| &c.channel),
        }
    }

    pub fn strip_channel_mut(&mut self, s: Strip) -> Option<&mut Channel> {
        match s {
            Strip::Bus(id) => self.bus_mut(id).map(|b| &mut b.channel),
            Strip::Chain(id) => self.fx_chain_mut(id).map(|c| &mut c.channel),
        }
    }

    /// What `s` plays and sends into besides the Master: a bus's chains
    /// and output bus, a chain's output bus.
    pub fn strip_feeds(&self, s: Strip) -> Vec<Strip> {
        let (channel, sends) = match s {
            Strip::Bus(id) => match self.bus(id) {
                Some(b) => (&b.channel, b.sends.as_slice()),
                None => return Vec::new(),
            },
            Strip::Chain(id) => match self.fx_chain(id) {
                Some(c) => (&c.channel, &[][..]),
                None => return Vec::new(),
            },
        };
        let mut out: Vec<Strip> = sends
            .iter()
            .map(|x| match x.to {
                SendTo::Bus(b) => Strip::Bus(b),
                SendTo::Fx(c) => Strip::Chain(c),
            })
            .collect();
        if let Output::Bus(b) = channel.output {
            out.push(Strip::Bus(b));
        }
        out
    }

    /// Whether `from`'s sound ends up in `to` (itself included).
    pub fn reaches(&self, from: Strip, to: Strip) -> bool {
        let mut seen = HashSet::new();
        let mut stack = vec![from];
        while let Some(s) = stack.pop() {
            if s == to {
                return true;
            }
            if seen.insert(s) {
                stack.extend(self.strip_feeds(s));
            }
        }
        false
    }

    /// The busses and chains in the order they are mixed: each before
    /// those it plays or sends into.
    pub fn strip_order(&self) -> Vec<Strip> {
        let all = self.strips();
        let mut fed: HashMap<Strip, usize> = all.iter().map(|s| (*s, 0)).collect();
        for s in &all {
            for n in self.strip_feeds(*s) {
                if let Some(k) = fed.get_mut(&n) {
                    *k += 1;
                }
            }
        }
        let mut ready: Vec<Strip> = all.iter().rev().filter(|s| fed[*s] == 0).copied().collect();
        let mut order = Vec::with_capacity(all.len());
        while let Some(s) = ready.pop() {
            order.push(s);
            for n in self.strip_feeds(s) {
                if let Some(k) = fed.get_mut(&n) {
                    *k -= 1;
                    if *k == 0 {
                        ready.push(n);
                    }
                }
            }
        }
        // A circle (never in a checked project): the rest as they come.
        for s in all {
            if !order.contains(&s) {
                order.push(s);
            }
        }
        order
    }

    /// A bus's name: the user's, else "Bus A"… by its place (English;
    /// the interface translates "Bus").
    pub fn bus_name(&self, id: BusId) -> String {
        match (self.bus(id), self.bus_index(id)) {
            (Some(b), _) if !b.channel.name.is_empty() => b.channel.name.clone(),
            (_, Some(i)) => format!("Bus {}", bus_letter(i)),
            _ => String::new(),
        }
    }

    /// Adds a bus (Insert › Audio Bus), routed to the Master; `None` past
    /// 26.
    pub fn add_bus(&mut self) -> Option<BusId> {
        if self.busses.len() >= MAX_BUSSES {
            return None;
        }
        let id = BusId(self.busses.iter().map(|b| b.id.0 + 1).max().unwrap_or(1));
        self.busses.push(Bus {
            id,
            sends: Vec::new(),
            channel: Channel::default(),
        });
        Some(id)
    }

    /// Deletes bus `id`: what played into it (tracks, busses, chains)
    /// plays into the bus before it (the Master for the first, and for
    /// what would then play in a circle), and the sends to it go.
    pub fn remove_bus(&mut self, id: BusId) -> bool {
        let Some(i) = self.bus_index(id) else {
            return false;
        };
        let instead = match i {
            0 => Output::Master,
            _ => Output::Bus(self.busses[i - 1].id),
        };
        self.busses.remove(i);
        self.channel_mapping.retain(|m| m.bus != Some(id));
        for b in &mut self.busses {
            b.sends.retain(|s| s.to != SendTo::Bus(id));
        }
        let gone = Output::Bus(id);
        for t in &mut self.tracks {
            if t.output == gone {
                t.output = instead;
            }
            t.sends.retain(|s| s.to != SendTo::Bus(id));
            t.envelopes
                .retain(|e| e.kind != crate::envelope::EnvelopeKind::Send(SendTo::Bus(id)));
        }
        let moved: Vec<Strip> = self
            .strips()
            .into_iter()
            .filter(|s| self.strip_channel(*s).is_some_and(|c| c.output == gone))
            .collect();
        for s in moved {
            let fits = match s {
                Strip::Bus(b) => self.bus_can_route(b, instead),
                Strip::Chain(c) => self.chain_can_route(c, instead),
            };
            if let Some(c) = self.strip_channel_mut(s) {
                c.output = if fits { instead } else { Output::Master };
            }
        }
        true
    }

    /// Whether bus `from` may play into `to` (one that exists, and no
    /// circle: `to` does not end up in `from`; any hardware output).
    pub fn bus_can_route(&self, from: BusId, to: Output) -> bool {
        match to {
            Output::Master | Output::Hardware(_) => true,
            Output::Bus(b) => {
                self.bus(b).is_some() && !self.reaches(Strip::Bus(b), Strip::Bus(from))
            }
        }
    }

    /// Whether chain `from` may play into `to` (never a hardware output:
    /// chains play into the Master or a bus, as VEGAS's).
    pub fn chain_can_route(&self, from: FxChainId, to: Output) -> bool {
        match to {
            Output::Hardware(_) => false,
            Output::Master => true,
            Output::Bus(b) => {
                self.bus(b).is_some() && !self.reaches(Strip::Bus(b), Strip::Chain(from))
            }
        }
    }

    /// Whether bus `from` may send to chain `to` (the chain does not play
    /// back into it).
    pub fn bus_can_send(&self, from: BusId, to: FxChainId) -> bool {
        self.fx_chain(to).is_some() && !self.reaches(Strip::Chain(to), Strip::Bus(from))
    }

    /// Whether bus `from` may send to `to`: a chain that does not play
    /// back into it, or another bus on a hardware output (VEGAS's: a bus
    /// sends to chains and to busses routed to hardware only).
    pub fn bus_can_send_to(&self, from: BusId, to: SendTo) -> bool {
        match to {
            SendTo::Fx(c) => self.bus_can_send(from, c),
            SendTo::Bus(b) => {
                b != from
                    && self.bus(b).is_some_and(|x| x.channel.output.is_hardware())
                    && !self.reaches(Strip::Bus(b), Strip::Bus(from))
            }
        }
    }

    /// Whether a bus plays on a hardware output (the Master's mute then
    /// leaves it heard).
    pub fn plays_to_hardware(&self) -> bool {
        self.busses.iter().any(|b| b.channel.output.is_hardware())
    }

    /// Routes bus `id` into `to`; refused when it would make a circle.
    /// Off a hardware output, the other busses' sends to it go.
    pub fn set_bus_output(&mut self, id: BusId, to: Output) -> bool {
        if !self.bus_can_route(id, to) {
            return false;
        }
        match self.bus_mut(id) {
            Some(b) => b.channel.output = to,
            None => return false,
        }
        if !to.is_hardware() {
            for b in &mut self.busses {
                b.sends.retain(|s| s.to != SendTo::Bus(id));
            }
        }
        true
    }

    /// Routes chain `id` into `to`; refused when it would make a circle.
    pub fn set_chain_output(&mut self, id: FxChainId, to: Output) -> bool {
        if !self.chain_can_route(id, to) {
            return false;
        }
        match self.fx_chain_mut(id) {
            Some(c) => {
                c.channel.output = to;
                true
            }
            None => false,
        }
    }

    /// Sets the number of busses (Project Properties › Audio): new ones
    /// added at the end, the last ones deleted.
    pub fn set_bus_count(&mut self, n: usize) {
        let n = n.min(MAX_BUSSES);
        while self.busses.len() < n {
            self.add_bus();
        }
        while self.busses.len() > n {
            let last = self.busses[self.busses.len() - 1].id;
            self.remove_bus(last);
        }
    }

    /// Adds an assignable effects chain (Insert › Audio Assignable FX),
    /// into the Master; `None` past 32.
    pub fn add_fx_chain(&mut self, effects: Vec<Effect>) -> Option<FxChainId> {
        if self.fx_chains.len() >= MAX_FX_CHAINS {
            return None;
        }
        let id = FxChainId(self.fx_chains.iter().map(|c| c.id.0 + 1).max().unwrap_or(1));
        self.fx_chains.push(FxChain {
            id,
            input_db: 0.0,
            channel: Channel {
                effects,
                ..Channel::default()
            },
        });
        Some(id)
    }

    /// Deletes chain `id` and the sends to it.
    pub fn remove_fx_chain(&mut self, id: FxChainId) -> bool {
        let before = self.fx_chains.len();
        self.fx_chains.retain(|c| c.id != id);
        for t in &mut self.tracks {
            t.sends.retain(|s| s.to != SendTo::Fx(id));
            t.envelopes
                .retain(|e| e.kind != crate::envelope::EnvelopeKind::Send(SendTo::Fx(id)));
        }
        for b in &mut self.busses {
            b.sends.retain(|s| s.to != SendTo::Fx(id));
        }
        for b in &mut self.input_busses {
            b.sends.retain(|s| s.to != SendTo::Fx(id));
        }
        self.fx_chains.len() != before
    }

    /// Moves a bus or a chain to place `index` among its kind (the
    /// Mixing Console's channels dragged by their names); a bus named by
    /// its place takes its new place's letter.
    pub fn move_strip_to(&mut self, s: Strip, index: usize) -> bool {
        fn shift<T>(v: &mut Vec<T>, from: usize, to: usize) {
            let x = v.remove(from);
            let to = to.min(v.len());
            v.insert(to, x);
        }
        match s {
            Strip::Bus(id) => {
                let Some(from) = self.bus_index(id) else {
                    return false;
                };
                shift(&mut self.busses, from, index);
            }
            Strip::Chain(id) => {
                let Some(from) = self.fx_chains.iter().position(|c| c.id == id) else {
                    return false;
                };
                shift(&mut self.fx_chains, from, index);
            }
        }
        true
    }

    /// Deletes a bus or a chain.
    pub fn remove_strip(&mut self, s: Strip) -> bool {
        match s {
            Strip::Bus(id) => self.remove_bus(id),
            Strip::Chain(id) => self.remove_fx_chain(id),
        }
    }

    /// Track `id`'s output.
    pub fn set_track_output(&mut self, id: TrackId, to: Output) -> bool {
        if matches!(to, Output::Bus(b) if self.bus(b).is_none()) || to.is_hardware() {
            return false;
        }
        match self.track_by_id_mut(id) {
            Some(t) => {
                t.output = to;
                true
            }
            None => false,
        }
    }

    /// Track `id`'s send to `send.to` (one a target; a send at no level
    /// that is as it starts goes).
    pub fn set_send(&mut self, id: TrackId, send: Send) -> bool {
        let exists = match send.to {
            SendTo::Bus(b) => self.bus(b).is_some(),
            SendTo::Fx(c) => self.fx_chain(c).is_some(),
        };
        let Some(t) = self.track_by_id_mut(id).filter(|_| exists) else {
            return false;
        };
        t.sends.retain(|s| s.to != send.to);
        // One with an envelope stays: the envelope rides on it.
        let envelope = t
            .envelopes
            .iter()
            .any(|e| e.kind == crate::envelope::EnvelopeKind::Send(send.to));
        if send != Send::new(send.to) || envelope {
            t.sends.push(send);
        }
        true
    }

    /// Bus `id`'s send to a chain or a bus on a hardware output (a send
    /// at no level that is as it starts goes); refused when it would play
    /// back into the bus.
    pub fn set_bus_send(&mut self, id: BusId, send: Send) -> bool {
        if !self.bus_can_send_to(id, send.to) {
            return false;
        }
        let Some(b) = self.bus_mut(id) else {
            return false;
        };
        b.sends.retain(|s| s.to != send.to);
        if send != Send::new(send.to) && send != Send::of_bus(send.to) {
            b.sends.push(send);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ProjectSettings, TrackKind};

    /// A send's envelope makes its send when the track had none, keeps it
    /// at no level, and goes with its bus; busses' and chains' envelopes
    /// are volume, pan and mute.
    #[test]
    fn send_envelopes_ride_on_their_sends() {
        use crate::envelope::{EnvelopeKind, EnvelopeOwner};
        use crate::fx::KeyCurve;
        let (mut p, a) = project();
        let bus = p.add_bus().unwrap();
        let kind = EnvelopeKind::Send(SendTo::Bus(bus));
        assert!(p.toggle_envelope(a, kind).unwrap());
        assert_eq!(p.track(a).unwrap().sends.len(), 1);
        p.check_invariants().unwrap();
        // Brought back to no level, the send stays for its envelope.
        assert!(p.set_send(a, Send::new(SendTo::Bus(bus))));
        assert_eq!(p.track(a).unwrap().sends.len(), 1);
        // Without the envelope it may go.
        assert!(!p.toggle_envelope(a, kind).unwrap());
        p.toggle_envelope(a, kind).unwrap();
        // The bus deleted: the send and its envelope go.
        assert!(p.remove_bus(bus));
        assert!(p.track(a).unwrap().sends.is_empty());
        assert!(p.track(a).unwrap().envelopes.is_empty());
        p.check_invariants().unwrap();
        // A send to nothing: refused.
        assert!(p.toggle_envelope(a, kind).is_err());
        // A chain's volume envelope; a velocity one refused.
        let fx = p.add_fx_chain(Vec::new()).unwrap();
        let strip = Strip::Chain(fx);
        assert!(
            p.toggle_strip_envelope(strip, EnvelopeKind::Volume, KeyCurve::Linear)
                .unwrap()
        );
        p.set_envelope_point(
            EnvelopeOwner::Strip(strip),
            EnvelopeKind::Volume,
            bv_core::Tick::from_seconds(1),
            0.25,
        )
        .unwrap();
        assert_eq!(p.fx_chain(fx).unwrap().channel.envelopes[0].points.len(), 2);
        assert!(
            p.toggle_strip_envelope(strip, EnvelopeKind::Velocity, KeyCurve::Linear)
                .is_err()
        );
        p.check_invariants().unwrap();
    }

    fn project() -> (Project, TrackId) {
        let p = Project::new(ProjectSettings::default());
        let audio = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .unwrap()
            .id;
        (p, audio)
    }

    #[test]
    fn busses_come_from_a_to_z_and_go_to_the_one_before() {
        let (mut p, audio) = project();
        let ids: Vec<BusId> = (0..4).map(|_| p.add_bus().unwrap()).collect();
        assert_eq!(p.bus_name(ids[3]), "Bus D");
        p.bus_mut(ids[1]).unwrap().channel.name = "Drums".into();
        assert_eq!(p.bus_name(ids[1]), "Drums");
        // A track on D, a send to C and a chain into D.
        assert!(p.set_track_output(audio, Output::Bus(ids[3])));
        let mut send = Send::new(SendTo::Bus(ids[2]));
        send.gain_db = -6.0;
        assert!(p.set_send(audio, send));
        let fx = p.add_fx_chain(Vec::new()).unwrap();
        assert!(p.set_chain_output(fx, Output::Bus(ids[3])));
        p.check_invariants().unwrap();
        // D goes: the track and the chain play into C.
        assert!(p.remove_bus(ids[3]));
        assert_eq!(p.track(audio).unwrap().output, Output::Bus(ids[2]));
        assert_eq!(p.fx_chain(fx).unwrap().channel.output, Output::Bus(ids[2]));
        // C goes: the send to it too.
        assert!(p.remove_bus(ids[2]));
        assert!(p.track(audio).unwrap().sends.is_empty());
        assert_eq!(p.track(audio).unwrap().output, Output::Bus(ids[1]));
        p.check_invariants().unwrap();
        // 26 at most.
        p.set_bus_count(30);
        assert_eq!(p.busses.len(), MAX_BUSSES);
        assert!(p.add_bus().is_none());
        assert_eq!(p.bus_name(p.busses[25].id), "Bus Z");
        p.set_bus_count(1);
        assert_eq!(p.busses.len(), 1);
        assert_eq!(p.track(audio).unwrap().output, Output::Bus(ids[0]));
        p.check_invariants().unwrap();
    }

    #[test]
    fn busses_and_chains_move_among_their_kind() {
        let (mut p, audio) = project();
        let ids: Vec<BusId> = (0..3).map(|_| p.add_bus().unwrap()).collect();
        assert!(p.set_track_output(audio, Output::Bus(ids[0])));
        assert!(p.move_strip_to(Strip::Bus(ids[0]), 2));
        let order: Vec<BusId> = p.busses.iter().map(|b| b.id).collect();
        assert_eq!(order, vec![ids[1], ids[2], ids[0]]);
        // Named by its new place; its track still plays into it.
        assert_eq!(p.bus_name(ids[0]), "Bus C");
        assert_eq!(p.track(audio).unwrap().output, Output::Bus(ids[0]));
        let fx: Vec<FxChainId> = (0..2)
            .map(|_| p.add_fx_chain(Vec::new()).unwrap())
            .collect();
        assert!(p.move_strip_to(Strip::Chain(fx[1]), 0));
        assert_eq!(p.fx_chains[0].id, fx[1]);
        assert!(!p.move_strip_to(Strip::Bus(BusId(999)), 0));
        p.check_invariants().unwrap();
    }

    #[test]
    fn busses_never_route_in_a_circle() {
        let (mut p, _) = project();
        let a = p.add_bus().unwrap();
        let b = p.add_bus().unwrap();
        let c = p.add_bus().unwrap();
        assert!(p.set_bus_output(a, Output::Bus(b)));
        assert!(p.set_bus_output(b, Output::Bus(c)));
        // C cannot go to A or B: it would come back to itself.
        assert!(!p.set_bus_output(c, Output::Bus(a)));
        assert!(!p.set_bus_output(c, Output::Bus(b)));
        assert!(!p.set_bus_output(a, Output::Bus(a)));
        assert!(p.set_bus_output(c, Output::Master));
        p.check_invariants().unwrap();
        // B goes: A plays into the bus before it, A itself — into the
        // Master instead.
        assert!(p.remove_bus(b));
        assert_eq!(p.bus(a).unwrap().channel.output, Output::Master);
        p.check_invariants().unwrap();
    }

    #[test]
    fn a_bus_sends_to_chains_that_do_not_play_back_into_it() {
        let (mut p, _) = project();
        let a = p.add_bus().unwrap();
        let b = p.add_bus().unwrap();
        let fx = p.add_fx_chain(Vec::new()).unwrap();
        let mut send = Send::new(SendTo::Fx(fx));
        assert!(!send.pre_volume, "a bus's send starts after its volume");
        send.gain_db = -6.0;
        assert!(p.set_bus_send(a, send));
        // Busses send to chains, and to busses only on hardware outputs.
        let to_bus = Send {
            to: SendTo::Bus(b),
            ..send
        };
        assert!(!p.set_bus_send(a, to_bus));
        assert!(p.set_bus_output(b, Output::Hardware(1)));
        assert!(p.set_bus_send(a, to_bus));
        assert!(!p.set_bus_send(b, Send::of_bus(SendTo::Bus(b))));
        p.check_invariants().unwrap();
        // Back from the hardware: the send to it goes. Chains and tracks
        // never play there.
        assert!(p.set_bus_output(b, Output::Master));
        assert_eq!(p.bus(a).unwrap().sends.len(), 1);
        assert!(!p.set_chain_output(fx, Output::Hardware(0)));
        p.check_invariants().unwrap();
        // The chain cannot play back into A, nor into B once B plays into
        // A.
        assert!(!p.set_chain_output(fx, Output::Bus(a)));
        assert!(p.set_bus_output(b, Output::Bus(a)));
        assert!(!p.set_chain_output(fx, Output::Bus(b)));
        p.check_invariants().unwrap();
        // Mixed in order: B before A (it plays into A), A before the chain.
        let order = p.strip_order();
        let at = |s| order.iter().position(|x| *x == s).unwrap();
        assert!(at(Strip::Bus(b)) < at(Strip::Bus(a)));
        assert!(at(Strip::Bus(a)) < at(Strip::Chain(fx)));
        // A bus of its own for the chain: C, mixed after it.
        let c = p.add_bus().unwrap();
        assert!(p.set_chain_output(fx, Output::Bus(c)));
        let order = p.strip_order();
        let at = |s| order.iter().position(|x| *x == s).unwrap();
        assert!(at(Strip::Chain(fx)) < at(Strip::Bus(c)));
        // C goes: the chain would play into B, which comes back to it
        // through A, so into the Master.
        assert!(p.remove_bus(c));
        assert_eq!(p.fx_chain(fx).unwrap().channel.output, Output::Master);
        assert_eq!(p.bus(b).unwrap().channel.output, Output::Bus(a));
        p.check_invariants().unwrap();
        // The chain goes, and A's send to it.
        assert!(p.remove_strip(Strip::Chain(fx)));
        assert!(p.bus(a).unwrap().sends.is_empty());
        p.check_invariants().unwrap();
    }

    #[test]
    fn chains_and_sends_keep_in_the_project_file() {
        let (mut p, audio) = project();
        let bus = p.add_bus().unwrap();
        let fx = p
            .add_fx_chain(vec![crate::fx::Effect::new(crate::fx::EffectKind::Reverb)])
            .unwrap();
        p.fx_chain_mut(fx).unwrap().input_db = -3.0;
        let mut send = Send::new(SendTo::Fx(fx));
        assert!(!send.pre_volume, "a chain's send starts after the volume");
        assert!(Send::new(SendTo::Bus(bus)).pre_volume);
        send.gain_db = -12.0;
        send.link_pan = true;
        p.set_send(audio, send);
        p.set_track_output(audio, Output::Bus(bus));
        let mut bus_send = Send::new(SendTo::Fx(fx));
        bus_send.gain_db = -3.0;
        bus_send.pre_volume = true;
        assert!(p.set_bus_send(bus, bus_send));
        p.master_fx
            .push(crate::fx::Effect::new(crate::fx::EffectKind::Limiter));
        let text = serde_json::to_string(&p).unwrap();
        let back: Project = serde_json::from_str(&text).unwrap();
        assert_eq!(back.busses, p.busses);
        assert_eq!(back.fx_chains, p.fx_chains);
        assert_eq!(back.master_fx, p.master_fx);
        assert_eq!(back.track(audio), p.track(audio));
        // A send back at its start goes.
        p.set_send(audio, Send::new(SendTo::Fx(fx)));
        assert!(p.track(audio).unwrap().sends.is_empty());
        // The chain goes, and the send to it.
        p.set_send(audio, send);
        assert!(p.remove_fx_chain(fx));
        assert!(p.track(audio).unwrap().sends.is_empty());
        p.check_invariants().unwrap();
    }

    #[test]
    fn the_channel_mapping_lists_the_master_and_every_bus() {
        let mut p = Project::default();
        let a = p.add_bus().unwrap();
        // Nothing mapped yet: the Master and the bus, neither on.
        let rows = p.channel_mapping_rows();
        assert_eq!(
            rows.iter().map(|m| m.bus).collect::<Vec<_>>(),
            [None, Some(a)]
        );
        assert!(p.mapped_strips().is_empty());
        // Bus A first, mixed down, then the Master's two channels.
        p.channel_mapping = vec![
            MappedStrip {
                bus: Some(a),
                on: true,
                mono: true,
            },
            MappedStrip {
                bus: None,
                on: true,
                mono: false,
            },
        ];
        let b = p.add_bus().unwrap();
        let rows = p.channel_mapping_rows();
        assert_eq!(
            rows.iter().map(|m| m.bus).collect::<Vec<_>>(),
            [Some(a), None, Some(b)]
        );
        assert_eq!(p.mapped_channels(), 3);
        // A bus deleted leaves the mapping.
        assert!(p.remove_bus(a));
        assert_eq!(p.mapped_channels(), 2);
        assert!(p.channel_mapping.iter().all(|m| m.bus != Some(a)));
    }
}
