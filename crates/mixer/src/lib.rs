//! Mixagem de áudio: soma todas as trilhas audíveis num bloco de amostras na
//! taxa e no layout de canais do projeto.
//!
//! As bordas dos clipes caem na amostra exata: a amostra `s` do projeto toca
//! se o instante `sample_start(s)` estiver dentro do clipe.

pub mod align;
pub mod beats;
pub mod dsp;
pub mod fft;
mod fx;

/// `channels` (a buffer a channel, all of the sound) through `e` when it
/// is one of the effects that change the sound's length
/// ([`bv_timeline::EffectKind::works_offline`]): the result, its length its
/// own.
pub fn render_offline(
    e: &bv_timeline::Effect,
    rate: u32,
    channels: Vec<Vec<f32>>,
) -> Option<Vec<Vec<f32>>> {
    fx::offline::render(e, rate, channels)
}
pub mod loudness;
pub mod meters;
pub mod modern_eq;
mod prerender;
mod stretch;
#[cfg(not(target_family = "wasm"))]
mod vst;

use std::collections::HashMap;

use bv_core::ChannelLayout;
use bv_core::{AudioBuffer, SampleRate, TICKS_PER_SECOND, Tick};
use bv_media::{AudioReader, MediaError};
use bv_timeline::{
    AudioQuality, BusId, Channel, ChannelMap, Clip, ClipId, Effect, FxChainId, MediaId, MediaInfo,
    Output, Project, SendTo, Strip, TrackId,
};

/// De quem é uma cadeia de efeitos com estado.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum FxKey {
    Clip(ClipId),
    Track(TrackId),
    Bus(BusId),
    Chain(FxChainId),
    Master,
}

/// A block's sound as the tracks leave it: for the Master bus, for each
/// audio bus and for each assignable effects chain (each in the project's
/// layout). [`finish_mix`] routes the busses and chains into the Master.
#[derive(Debug)]
pub struct Sums {
    pub master: AudioBuffer,
    busses: Vec<(BusId, AudioBuffer)>,
    chains: Vec<(FxChainId, AudioBuffer)>,
    /// What busses play on the output device's pairs of channels (stereo).
    hardware: Vec<(u8, AudioBuffer)>,
}

impl Sums {
    /// Silence for `project`'s busses and chains, `frames` long.
    pub fn silent(project: &Project, frames: usize) -> Sums {
        let rate = project.settings.sample_rate;
        let layout = project.settings.channels;
        let silent = || AudioBuffer::silent(rate, layout, frames);
        Sums {
            master: silent(),
            busses: project.busses.iter().map(|b| (b.id, silent())).collect(),
            chains: project.fx_chains.iter().map(|c| (c.id, silent())).collect(),
            hardware: Vec::new(),
        }
    }

    /// Adds a bus's sound to the device's pair `pair` (its first two
    /// channels; a mono one on both).
    fn add_hardware(&mut self, pair: u8, buf: &AudioBuffer) {
        let i = match self.hardware.iter().position(|(p, _)| *p == pair) {
            Some(i) => i,
            None => {
                let frames = buf.channels.first().map_or(0, Vec::len);
                let stereo = AudioBuffer::silent(buf.sample_rate, ChannelLayout::Stereo, frames);
                self.hardware.push((pair, stereo));
                self.hardware.len() - 1
            }
        };
        for (k, dst) in self.hardware[i].1.channels.iter_mut().enumerate() {
            if let Some(src) = buf.channels.get(k).or(buf.channels.first()) {
                for (o, s) in dst.iter_mut().zip(src) {
                    *o += s;
                }
            }
        }
    }

    /// Where `to` plays (the Master for a bus that is no more).
    fn output(&mut self, to: Output) -> &mut AudioBuffer {
        let i = match to {
            Output::Bus(id) => self.busses.iter().position(|(b, _)| *b == id),
            Output::Master | Output::Hardware(_) => None,
        };
        match i {
            Some(i) => &mut self.busses[i].1,
            None => &mut self.master,
        }
    }

    /// What `to` receives (none for one that is no more).
    fn send(&mut self, to: SendTo) -> Option<&mut AudioBuffer> {
        match to {
            SendTo::Bus(id) => self
                .busses
                .iter_mut()
                .find(|(b, _)| *b == id)
                .map(|(_, buf)| buf),
            SendTo::Fx(id) => self
                .chains
                .iter_mut()
                .find(|(c, _)| *c == id)
                .map(|(_, buf)| buf),
        }
    }

    /// Takes what reached bus or chain `s` (none for one that is no
    /// more).
    fn take(&mut self, s: Strip) -> Option<AudioBuffer> {
        let buf = match s {
            Strip::Bus(id) => self
                .busses
                .iter_mut()
                .find(|(b, _)| *b == id)
                .map(|(_, buf)| buf),
            Strip::Chain(id) => self
                .chains
                .iter_mut()
                .find(|(c, _)| *c == id)
                .map(|(_, buf)| buf),
        }?;
        let empty = AudioBuffer::silent(buf.sample_rate, buf.layout, 0);
        Some(std::mem::replace(buf, empty))
    }

    /// Adds `other`'s sound (a track's, mixed apart) to these.
    pub fn add(&mut self, other: &Sums) {
        add_into(&mut self.master, &other.master);
        for (id, buf) in &other.busses {
            if let Some((_, dst)) = self.busses.iter_mut().find(|(b, _)| b == id) {
                add_into(dst, buf);
            }
        }
        for (id, buf) in &other.chains {
            if let Some((_, dst)) = self.chains.iter_mut().find(|(c, _)| c == id) {
                add_into(dst, buf);
            }
        }
    }
}

/// `src` added into `dst`.
fn add_into(dst: &mut AudioBuffer, src: &AudioBuffer) {
    for (o, s) in dst.channels.iter_mut().zip(&src.channels) {
        for (d, v) in o.iter_mut().zip(s) {
            *d += v;
        }
    }
}

/// `src` times `gain` added into `dst`.
fn add_scaled(dst: &mut AudioBuffer, src: &AudioBuffer, gain: f32) {
    for (o, s) in dst.channels.iter_mut().zip(&src.channels) {
        for (d, v) in o.iter_mut().zip(s) {
            *d += v * gain;
        }
    }
}

/// Every sample of `buf` times `gain`.
fn scale(buf: &mut AudioBuffer, gain: f32) {
    if (gain - 1.0).abs() > 1e-7 {
        for ch in &mut buf.channels {
            for s in ch.iter_mut() {
                *s *= gain;
            }
        }
    }
}

/// A bus's or a chain's pan: its sides' balance (to the right, the left
/// side goes down; to the left, the right side).
fn balance(buf: &mut AudioBuffer, pan: f32) {
    if buf.channels.len() < 2 || pan.abs() < 1e-6 {
        return;
    }
    let (side, keep) = balance_of(pan);
    for s in buf.channels[side].iter_mut() {
        *s *= keep;
    }
}

/// The side a balance turns down, and how much of it stays.
fn balance_of(pan: f32) -> (usize, f32) {
    if pan > 0.0 {
        (0, 1.0 - pan.min(1.0))
    } else {
        (1, 1.0 + pan.max(-1.0))
    }
}

/// A bus's or a chain's volume, pan and mute with their envelopes (its bus
/// track's), sample by sample from sample `s0`: the envelope's volume
/// times the strip's, its pan added to the strip's — or, in 5.1 with its
/// Surround Panner, its pair placed among the six channels.
fn follow_envelopes(
    ch: &Channel,
    rate: bv_core::SampleRate,
    s0: i64,
    buf: &mut AudioBuffer,
    surround: Option<&bv_timeline::SurroundPan>,
) {
    use bv_timeline::{AutomationMode, EnvelopeKind};
    // With automation off the envelopes do not play: the automation
    // controls' volume and pan do, as a second trim.
    let off = ch.automation.mode == AutomationMode::Off;
    let env = |k: EnvelopeKind| {
        (!off)
            .then(|| ch.envelopes.iter().find(|e| e.kind == k))
            .flatten()
    };
    let (volume, pan, mute) = (
        env(EnvelopeKind::Volume),
        env(EnvelopeKind::Pan),
        env(EnvelopeKind::Mute),
    );
    let (held_volume, held_pan) = if off {
        (ch.automation.volume, ch.automation.pan)
    } else {
        (1.0, 0.0)
    };
    let gain = db_to_gain(ch.gain_db) * held_volume;
    let frames = buf.channels.first().map_or(0, Vec::len);
    let stereo = buf.channels.len() >= 2;
    let mut placed = Placement::default();
    for i in 0..frames {
        let t = rate.sample_start(s0 + i as i64);
        let mut g = gain * volume.map_or(1.0, |e| e.value_at(t));
        if mute.is_some_and(|m| m.value_at(t) < 0.5) {
            g = 0.0;
        }
        if let Some(sp) = surround.filter(|_| buf.channels.len() >= 6) {
            let gains = placed.at(sp, bv_timeline::PanMode::default(), i, (!off).then_some(t));
            let (l, r) = (buf.channels[0][i], buf.channels[1][i]);
            for (c, samples) in buf.channels.iter_mut().enumerate().take(6) {
                let own = if c >= 2 { samples[i] } else { 0.0 };
                samples[i] = (l * gains[c][0] + r * gains[c][1] + own) * g;
            }
            continue;
        }
        let p = (ch.pan + pan.map_or(held_pan, |e| e.value_at(t))).clamp(-1.0, 1.0);
        let (side, keep) = balance_of(p);
        for (c, samples) in buf.channels.iter_mut().enumerate() {
            let k = if stereo && c == side && p.abs() >= 1e-6 {
                keep
            } else {
                1.0
            };
            samples[i] *= g * k;
        }
    }
}

/// Open audio readers and the effects' state.
///
/// Each event reads with a reader of its own: two events of one file (a
/// crossfade between two parts of a recording) would otherwise make a
/// single reader seek on every block.
#[derive(Default)]
pub struct AudioPool {
    readers: Vec<AudioSlot>,
    errors: HashMap<(MediaId, u16), MediaError>,
    /// Counts blocks mixed: a reader used in this block is not given to
    /// another event.
    block: u64,
    /// Cadeia de efeitos e a próxima amostra esperada (se a leitura pular,
    /// o estado é zerado).
    fx: HashMap<FxKey, (dsp::Chain, i64)>,
    /// LFE low-pass of 5.1 projects: its settings (rate, cutoff, quality),
    /// the filter and the next sample expected.
    lfe: Option<((u32, u32, AudioQuality), dsp::LowPass, i64)>,
    /// Peak per channel of each track's output in the last block mixed
    /// (the track meters).
    track_peaks: Vec<(TrackId, Vec<f32>)>,
    /// Peak per channel of each bus's and chain's output in the last block
    /// finished (their meters in the Mixing Console).
    strip_peaks: Vec<(Strip, Vec<f32>)>,
    /// The VU/PPM meters' needles and the last block's readings.
    meters: meters::Meters,
    /// The grains' shifts of events stretched with their pitch kept.
    stretch: HashMap<ClipId, stretch::Shifts>,
    /// Multichannel renders: each bus's sound as it leaves the bus (after
    /// its effects, pan and volume), kept by [`finish_mix`] while asked
    /// for ([`AudioPool::keep_bus_outputs`]).
    bus_outputs: Option<Vec<(BusId, AudioBuffer)>>,
    /// What the busses on hardware outputs played in the last block
    /// [`finish_mix`] finished: heard while playing, never rendered.
    hardware: Vec<(u8, AudioBuffer)>,
    /// Plug-in delay compensation's delay lines (see [`Delays`]).
    lines: HashMap<DelayKey, DelayLine>,
    /// The project's tempo, for the effects with tempo sync.
    tempo: bv_timeline::Tempo,
    /// The sounds of events whose effects change the sound's length, made
    /// through their chains ([`prerender`]).
    prerenders: prerender::Prerenders,
}

/// How far ahead of the frame's time each bus's and chain's input plays
/// (plug-in delay compensation along the routing): its own effects'
/// latency and that of what it plays into. A track is read as much
/// earlier as the latest of the paths it plays and sends into needs, the
/// other paths getting it through delay lines; tracks whose paths answer
/// in time play as they are.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Delays(HashMap<Strip, i64>);

impl Delays {
    /// How far ahead what plays into `to` must be.
    pub fn output(&self, to: Output) -> i64 {
        match to {
            Output::Bus(b) => self.0.get(&Strip::Bus(b)).copied().unwrap_or(0),
            Output::Master | Output::Hardware(_) => 0,
        }
    }

    /// How far ahead what is sent into `to` must be.
    pub fn send(&self, to: SendTo) -> i64 {
        let s = match to {
            SendTo::Bus(b) => Strip::Bus(b),
            SendTo::Fx(c) => Strip::Chain(c),
        };
        self.0.get(&s).copied().unwrap_or(0)
    }
}

/// `project`'s [`Delays`], its busses' and chains' effects' latencies
/// asked of `pool` (where [`finish_mix`] runs them).
pub fn delays(project: &Project, pool: &mut AudioPool) -> Delays {
    pool.tempo = project.settings.tempo;
    let rate = project.settings.sample_rate;
    let channels = project.settings.channels.channel_count();
    let mut total = HashMap::new();
    // Each after what it plays into.
    for s in project.strip_order().into_iter().rev() {
        let Some(ch) = project.strip_channel(s) else {
            continue;
        };
        let automated =
            bv_timeline::fx_automation::automated(&ch.effects, &ch.envelopes, rate.sample_start(0));
        let effects: Vec<&Effect> = if project.bypass_audio_fx {
            Vec::new()
        } else {
            automated.iter().collect()
        };
        let key = match s {
            Strip::Bus(id) => FxKey::Bus(id),
            Strip::Chain(id) => FxKey::Chain(id),
        };
        let own = if effects.is_empty() {
            0
        } else {
            pool.chain_latency(key, &effects, rate.0, channels) as i64
        };
        let after = match ch.output {
            Output::Bus(b) => total.get(&Strip::Bus(b)).copied().unwrap_or(0),
            Output::Master | Output::Hardware(_) => 0,
        };
        total.insert(s, own + after);
    }
    Delays(total)
}

/// A path's delay line, by what it carries: a track's output, a track's
/// send, a bus's send.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum DelayKey {
    Output(TrackId),
    Send(TrackId, SendTo),
    BusSend(BusId, SendTo),
}

/// What went into a path `delay` samples before, and the sample the next
/// block must start at (another time: a seek, which starts it again).
struct DelayLine {
    delay: usize,
    next: i64,
    ring: Vec<std::collections::VecDeque<f32>>,
}

/// A reader of a media's audio stream, and the event using it.
struct AudioSlot {
    key: (MediaId, u16),
    owner: ClipId,
    /// The block it was last used in.
    used: u64,
    reader: Box<dyn AudioReader>,
}

/// Readers of one media stream at most (more overlapping events share).
const READERS_PER_STREAM: usize = 6;
/// Blocks a reader may go unused before it is closed (some 20 seconds).
const IDLE_BLOCKS: u64 = 1000;

impl std::fmt::Debug for AudioPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioPool")
            .field("abertos", &self.readers.len())
            .field("erros", &self.errors)
            .finish()
    }
}

impl AudioPool {
    pub fn new() -> AudioPool {
        AudioPool::default()
    }

    /// Mixes while playing: the sounds of events whose effects change the
    /// sound's length are made on threads of their own (the last one made
    /// plays meanwhile) instead of at once.
    pub fn set_live(&mut self, live: bool) {
        // The web build has no threads of its own: there, at once.
        self.prerenders.live = live && !cfg!(target_family = "wasm");
    }

    /// The reader event `owner` reads `info`'s audio stream `stream` with:
    /// its own; else one no event used in this block (the latest used: at
    /// a cut the next event goes on where the last one stopped); else a new
    /// one.
    fn reader(&mut self, info: &MediaInfo, stream: u16, owner: ClipId) -> Option<usize> {
        let key = (info.id, stream);
        if self.errors.contains_key(&key) {
            return None;
        }
        let block = self.block;
        let mine = self
            .readers
            .iter()
            .position(|s| s.key == key && s.owner == owner);
        let i = match mine {
            Some(i) => i,
            None => {
                let free = self
                    .readers
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.key == key && s.used < block)
                    .max_by_key(|(_, s)| s.used)
                    .map(|(i, _)| i);
                let count = self.readers.iter().filter(|s| s.key == key).count();
                match free {
                    Some(i) => i,
                    None if count < READERS_PER_STREAM => {
                        match bv_media::open_audio_stream(info, stream) {
                            Ok(reader) => {
                                self.readers.push(AudioSlot {
                                    key,
                                    owner,
                                    used: block,
                                    reader,
                                });
                                self.readers.len() - 1
                            }
                            Err(e) => {
                                self.errors.insert(key, e);
                                return None;
                            }
                        }
                    }
                    None => self
                        .readers
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| s.key == key)
                        .min_by_key(|(_, s)| s.used)
                        .map(|(i, _)| i)?,
                }
            }
        };
        let slot = &mut self.readers[i];
        slot.owner = owner;
        slot.used = block;
        Some(i)
    }

    /// Starts a block: readers unused for long close (and the stretch
    /// shifts of events no reader reads any more).
    fn begin_block(&mut self) {
        self.block += 1;
        let block = self.block;
        self.readers.retain(|s| block - s.used <= IDLE_BLOCKS);
        self.prerenders.forget_old(block);
        if !self.stretch.is_empty() {
            let readers = &self.readers;
            self.stretch
                .retain(|id, _| readers.iter().any(|s| s.owner == *id));
        }
    }

    pub fn clear(&mut self) {
        self.readers.clear();
        self.errors.clear();
        self.fx.clear();
        self.lfe = None;
        self.track_peaks.clear();
        self.strip_peaks.clear();
        self.meters.clear();
        self.stretch.clear();
        self.lines.clear();
        self.prerenders.clear();
    }

    /// Adds `src` (the block from sample `at`) to `dst` `delay` samples
    /// late, through `key`'s delay line: a jump in time or another delay
    /// starts the line again, silent.
    fn delay_into(
        &mut self,
        key: DelayKey,
        delay: usize,
        at: i64,
        src: &AudioBuffer,
        dst: &mut AudioBuffer,
    ) {
        let n = src.channels.len();
        let frames = src.channels.first().map_or(0, Vec::len);
        let line = self.lines.entry(key).or_insert_with(|| DelayLine {
            delay,
            next: i64::MIN,
            ring: Vec::new(),
        });
        if line.delay != delay || line.next != at || line.ring.len() != n {
            *line = DelayLine {
                delay,
                next: at,
                ring: (0..n)
                    .map(|_| std::iter::repeat_n(0.0, delay).collect())
                    .collect(),
            };
        }
        for ((ring, s), d) in line
            .ring
            .iter_mut()
            .zip(&src.channels)
            .zip(&mut dst.channels)
        {
            for (x, o) in s.iter().zip(d.iter_mut()) {
                ring.push_back(*x);
                *o += ring.pop_front().unwrap_or(0.0);
            }
        }
        line.next = at + frames as i64;
    }

    /// Keeps each bus's output of the blocks finished from now on (or no
    /// more), for [`AudioPool::take_bus_outputs`].
    pub fn keep_bus_outputs(&mut self, keep: bool) {
        self.bus_outputs = keep.then(Vec::new);
    }

    /// Each bus's output in the last block finished (none of a block the
    /// muted Master left silent).
    pub fn take_bus_outputs(&mut self) -> Vec<(BusId, AudioBuffer)> {
        self.bus_outputs
            .as_mut()
            .map_or_else(Vec::new, std::mem::take)
    }

    /// What the busses on hardware outputs played in the last block
    /// [`finish_mix`] finished, by the device's pair of channels (0: its
    /// first two); the playback adds them there.
    pub fn take_hardware(&mut self) -> Vec<(u8, AudioBuffer)> {
        std::mem::take(&mut self.hardware)
    }

    /// Peak per channel of each audible track's output in the last block
    /// [`mix`] made (after volume, pan and track FX).
    pub fn take_track_peaks(&mut self) -> Vec<(TrackId, Vec<f32>)> {
        std::mem::take(&mut self.track_peaks)
    }

    /// Peak per channel of each bus's and chain's output in the last block
    /// [`finish_mix`] finished.
    pub fn take_strip_peaks(&mut self) -> Vec<(Strip, Vec<f32>)> {
        std::mem::take(&mut self.strip_peaks)
    }

    /// The VU/PPM readings of the last block mixed (a sine's amplitude per
    /// channel), while [`meters::set_vu_meters`] asks for them.
    pub fn take_vu_levels(&mut self) -> Vec<(meters::MeterOf, Vec<f32>)> {
        std::mem::take(&mut self.meters.readings)
    }

    /// A bus's or a chain's channel on its sound `buf` (`[s0, s1)`): its
    /// effects before or after its pan and volume (and their envelopes,
    /// with its mute's, sample by sample); nothing when muted, or when
    /// others are soloed and it is not (`solo`). With `tap`, the sound its
    /// pre-volume sends take: before its pan, volume and mute (after its
    /// effects when they come before its volume).
    #[allow(clippy::too_many_arguments)]
    fn channel(
        &mut self,
        project: &Project,
        key: FxKey,
        ch: &Channel,
        solo: bool,
        s0: i64,
        s1: i64,
        buf: &mut AudioBuffer,
        tap: bool,
    ) -> Option<AudioBuffer> {
        let silent = ch.muted || (solo && !ch.solo);
        if silent && !tap {
            scale(buf, 0.0);
            return None;
        }
        // FX automation: the effects as their envelopes (its bus track's)
        // have them now.
        let automated = bv_timeline::fx_automation::automated(
            &ch.effects,
            &ch.envelopes,
            project.settings.sample_rate.sample_start(s0),
        );
        let fx: Vec<&Effect> = if project.bypass_audio_fx {
            Vec::new()
        } else {
            automated.iter().collect()
        };
        let rate = project.settings.sample_rate.0;
        if !ch.fx_post_fader {
            self.run_fx(key, &fx, &ch.envelopes, rate, s0, s1, &mut buf.channels);
        }
        let pre = tap.then(|| buf.clone());
        if silent {
            scale(buf, 0.0);
            return pre;
        }
        let surround = surround_of(project, ch.surround.as_ref(), ch.output);
        // With automation off its envelopes do not play: what its
        // automation controls hold does, as a second trim.
        let off = ch.automation.mode == bv_timeline::AutomationMode::Off;
        if (ch.envelopes.is_empty() || off) && surround.is_none() {
            let (volume, pan) = if off {
                (ch.automation.volume, ch.automation.pan)
            } else {
                (1.0, 0.0)
            };
            balance(buf, (ch.pan + pan).clamp(-1.0, 1.0));
            scale(buf, db_to_gain(ch.gain_db) * volume);
        } else {
            follow_envelopes(ch, project.settings.sample_rate, s0, buf, surround);
        }
        if ch.fx_post_fader {
            self.run_fx(key, &fx, &ch.envelopes, rate, s0, s1, &mut buf.channels);
        }
        pre
    }

    /// Filters the LFE channel of a 5.1 mix of `[s0, s1)` (File › Properties
    /// › Audio › Enable low-pass filter on LFE).
    fn lfe_lowpass(&mut self, project: &Project, s0: i64, s1: i64, out: &mut AudioBuffer) {
        let s = &project.settings;
        if s.channels != ChannelLayout::Surround51 || !s.lfe_lowpass || out.channel_count() < 4 {
            self.lfe = None;
            return;
        }
        let key = (s.sample_rate.0, s.lfe_cutoff_hz, s.lfe_quality);
        let stages = match s.lfe_quality {
            AudioQuality::Preview => 1,
            AudioQuality::Good => 2,
            AudioQuality::Best => 4,
        };
        let fresh = || dsp::LowPass::new(key.0 as f32, key.1 as f32, stages);
        match &mut self.lfe {
            Some((k, f, next)) if *k == key => {
                if *next != s0 {
                    // A jump: start over instead of carrying sound from elsewhere.
                    *f = fresh();
                }
            }
            slot => *slot = Some((key, fresh(), s0)),
        }
        if let Some((_, f, next)) = &mut self.lfe {
            // 5.1 is L R C LFE Ls Rs.
            f.process(&mut out.channels[3]);
            *next = s1;
        }
    }

    /// Applies the effect chain of `key` to the samples `[s0, s1)`.
    /// How late chain `key` plays `effects` on `channels` channels at
    /// `rate`: matched to them first (as [`Self::run_fx`] does), its
    /// plug-ins' latencies summed.
    fn chain_latency(
        &mut self,
        key: FxKey,
        effects: &[&Effect],
        rate: u32,
        channels: usize,
    ) -> usize {
        let keep = KEEP_BYPASSED.load(std::sync::atomic::Ordering::Relaxed);
        if !effects
            .iter()
            .any(|e| e.kind.is_audio() && (e.enabled || keep))
        {
            return 0;
        }
        let entry = self
            .fx
            .entry(key)
            .or_insert_with(|| (dsp::Chain::default(), i64::MIN));
        entry.0.set_tempo(self.tempo);
        entry.0.sync(effects, rate, channels, keep);
        entry.0.latency()
    }

    /// Runs chain `key` (`effects`, FX automation's `envelopes`) on
    /// `channels`, the timeline's samples `[s0, s1)` at `rate`.
    #[allow(clippy::too_many_arguments)]
    fn run_fx(
        &mut self,
        key: FxKey,
        effects: &[&Effect],
        envelopes: &[bv_timeline::Envelope],
        rate: u32,
        s0: i64,
        s1: i64,
        channels: &mut [Vec<f32>],
    ) {
        let keep = KEEP_BYPASSED.load(std::sync::atomic::Ordering::Relaxed);
        if !effects
            .iter()
            .any(|e| e.kind.is_audio() && (e.enabled || keep))
        {
            self.fx.remove(&key);
            return;
        }
        let entry = self
            .fx
            .entry(key)
            .or_insert_with(|| (dsp::Chain::default(), s0));
        if entry.1 != s0 {
            entry.0.reset();
        }
        entry.0.set_tempo(self.tempo);
        entry.0.sync(effects, rate, channels.len(), keep);
        // The VST 3 plug-ins: where the block lies, and FX automation
        // sample by sample.
        entry.0.locate(s0);
        let frames = channels.first().map_or(0, Vec::len);
        entry.0.automate(effects, envelopes, s0, frames, rate);
        let owner = match key {
            FxKey::Clip(c) => modern_eq::TapOwner::Clip(c),
            FxKey::Track(t) => modern_eq::TapOwner::Track(t),
            FxKey::Bus(b) => modern_eq::TapOwner::Bus(b),
            FxKey::Chain(c) => modern_eq::TapOwner::Chain(c),
            FxKey::Master => modern_eq::TapOwner::Master,
        };
        entry
            .0
            .process_watching(channels, modern_eq::watched_in(owner));
        entry.1 = s1;
    }
}

/// Bypassed audio effects keep running, unheard, so that turning them on
/// again joins without a break (Options › Preferences › General: "Keep
/// bypassed FX running").
static KEEP_BYPASSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether bypassed audio effects keep running.
pub fn set_keep_bypassed(keep: bool) {
    KEEP_BYPASSED.store(keep, std::sync::atomic::Ordering::Relaxed);
}

/// Tracks' pre-volume sends go silent with the track's mute (Options ›
/// Preferences › Audio: "Track prefader sends listen to mute"); else they
/// play on (a cue mix).
static PREFADER_SENDS_LISTEN_TO_MUTE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Whether tracks' pre-volume sends go silent with the track's mute.
pub fn set_prefader_sends_listen_to_mute(listen: bool) {
    PREFADER_SENDS_LISTEN_TO_MUTE.store(listen, std::sync::atomic::Ordering::Relaxed);
}

fn prefader_sends_listen_to_mute() -> bool {
    PREFADER_SENDS_LISTEN_TO_MUTE.load(std::sync::atomic::Ordering::Relaxed)
}

/// The tracks the mixer works on: those heard
/// ([`Project::audible_tracks`]) and the muted ones (or not soloed) whose
/// pre-volume sends play on.
pub fn mixed_tracks(project: &Project) -> Vec<&bv_timeline::Track> {
    let listen = prefader_sends_listen_to_mute();
    project
        .tracks
        .iter()
        .filter(|t| {
            project.is_audible(t)
                || (t.kind == bv_timeline::TrackKind::Audio
                    && !listen
                    && t.sends
                        .iter()
                        .any(|s| s.pre_volume && db_to_gain(s.gain_db) > 0.0))
        })
        .collect()
}

/// The length of the quick fades at audio event edges, in ticks (Options ›
/// Preferences › Editing: "Quick fade length for audio events").
static QUICK_FADE: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(TICKS_PER_SECOND / 100);

/// Sets the length of the quick fades at audio event edges (Options ›
/// Quickfade Audio Edits turns them on).
pub fn set_quick_fade(length: Tick) {
    QUICK_FADE.store(length.0.max(0), std::sync::atomic::Ordering::Relaxed);
}

fn quick_fade() -> Tick {
    Tick(QUICK_FADE.load(std::sync::atomic::Ordering::Relaxed))
}

/// Primeira amostra cujo início é ≥ `t`.
/// `src` (event `clip`'s sound from project sample `s0`) through its fades
/// in, out and with its neighbours (and the quick fades at its edges).
fn fade_clip(
    project: &Project,
    track: &bv_timeline::Track,
    clip: &Clip,
    rate: SampleRate,
    s0: i64,
    src: &mut [Vec<f32>],
) {
    let n = src.first().map_or(0, Vec::len);
    let (fade_in, fade_out) = track.effective_fades(clip, project.auto_crossfade);
    let q_len = quick_fade();
    let quick = project.quick_fades && q_len > Tick::ZERO && (fade_in < q_len || fade_out < q_len);
    if fade_in > Tick::ZERO || fade_out > Tick::ZERO || quick {
        let gains: Vec<f32> = (0..n)
            .map(|i| {
                let t = rate.sample_start(s0 + i as i64);
                let q = if quick {
                    quick_fade_gain(clip, fade_in, fade_out, t, q_len)
                } else {
                    1.0
                };
                fade_gain(clip, fade_in, fade_out, t) * q
            })
            .collect();
        for ch in src.iter_mut() {
            for (s, g) in ch.iter_mut().zip(&gains) {
                *s *= g;
            }
        }
    }
}

fn first_sample_at_or_after(rate: SampleRate, t: Tick) -> i64 {
    let s = rate.sample_at(t);
    if rate.sample_start(s) < t { s + 1 } else { s }
}

/// Mistura `frames` amostras a partir da amostra `start` do projeto.
///
/// Por trilha: cada clipe recebe seu nível e seus fades (incluindo o
/// crossfade automático com clipes sobrepostos, de potência constante com a
/// curva padrão); a soma da trilha recebe o nível e o pan dela. No fim, o
/// barramento mestre aplica o ganho geral.
pub fn mix(project: &Project, start: i64, frames: usize, pool: &mut AudioPool) -> AudioBuffer {
    let mut sums = Sums::silent(project, frames);
    // A muted Master leaves the block silent, but for the busses on
    // hardware outputs.
    if project.master.muted && !project.plays_to_hardware() {
        return sums.master;
    }
    pool.track_peaks.clear();
    pool.meters.readings.clear();
    pool.begin_block();
    let delays = delays(project, pool);
    for track in mixed_tracks(project) {
        if let Some(peaks) = add_track(project, track, start, frames, &delays, pool, &mut sums) {
            pool.track_peaks.push((track.id, peaks));
        }
    }
    finish_mix(project, start, pool, sums)
}

/// One track's sound in the block of `frames` samples from `start`, with
/// a pool of its own (see [`mix`]): its events, effects, level, pan and
/// envelopes in the project's layout, into its output and its sends, and
/// its peaks. `None` when it is not heard or adds nothing. Summing the
/// tracks of [`mixed_tracks`] ([`Sums::add`]) and then
/// [`finish_mix`] gives what [`mix`] gives, each track on a thread of its
/// own if need be — `delays` the project's [`Delays`], worked out with the
/// pool the busses mix in.
pub fn mix_track(
    project: &Project,
    track: TrackId,
    start: i64,
    frames: usize,
    delays: &Delays,
    pool: &mut AudioPool,
) -> Option<(Sums, Vec<f32>)> {
    if project.master.muted && !project.plays_to_hardware() {
        return None;
    }
    let track = mixed_tracks(project).into_iter().find(|t| t.id == track)?;
    pool.meters.readings.clear();
    pool.begin_block();
    let mut sums = Sums::silent(project, frames);
    let peaks = add_track(project, track, start, frames, delays, pool, &mut sums)?;
    Some((sums, peaks))
}

/// The busses and chains on what the tracks left them, into their outputs
/// and a bus's sends — each before those it plays or sends into
/// ([`Project::strip_order`]) — and the Master bus on the whole: each
/// through its effects (before or after its volume), pan and volume, muted
/// or soloed, a chain at its input level; the Master's effects and volume,
/// and in 5.1 the LFE low-pass (with `pool`'s effects and filter states).
pub fn finish_mix(
    project: &Project,
    start: i64,
    pool: &mut AudioPool,
    mut sums: Sums,
) -> AudioBuffer {
    let frames = sums.master.channels.first().map_or(0, Vec::len);
    let end = start + frames as i64;
    if let Some(kept) = &mut pool.bus_outputs {
        kept.clear();
    }
    let delays = delays(project, pool);
    // A soloed bus mutes the busses not soloed; a soloed chain, the chains
    // and the busses not soloed.
    let chain_solo = project.fx_chains.iter().any(|c| c.channel.solo);
    let bus_solo = chain_solo || project.busses.iter().any(|b| b.channel.solo);
    pool.strip_peaks.clear();
    // The tracks' readings stay (mixed in this pool before).
    pool.meters
        .readings
        .retain(|(of, _)| matches!(of, meters::MeterOf::Track(_)));
    let peaks = |buf: &AudioBuffer| -> Vec<f32> {
        buf.channels
            .iter()
            .map(|c| c.iter().fold(0f32, |m, s| m.max(s.abs())))
            .collect()
    };
    for strip in project.strip_order() {
        let Some(mut buf) = sums.take(strip) else {
            continue;
        };
        match strip {
            Strip::Chain(id) => {
                let Some(chain) = project.fx_chain(id) else {
                    continue;
                };
                scale(&mut buf, db_to_gain(chain.input_db));
                let key = FxKey::Chain(id);
                pool.channel(
                    project,
                    key,
                    &chain.channel,
                    chain_solo,
                    start,
                    end,
                    &mut buf,
                    false,
                );
                pool.strip_peaks.push((strip, peaks(&buf)));
                pool.meters
                    .measure(meters::MeterOf::Strip(strip), start, &buf);
                add_into(sums.output(chain.channel.output), &buf);
            }
            Strip::Bus(id) => {
                let Some(bus) = project.bus(id) else {
                    continue;
                };
                let sends: Vec<_> = bus
                    .sends
                    .iter()
                    .filter(|s| db_to_gain(s.gain_db) > 0.0)
                    .collect();
                let tap = sends.iter().any(|s| s.pre_volume);
                let key = FxKey::Bus(id);
                let pre = pool.channel(
                    project,
                    key,
                    &bus.channel,
                    bus_solo,
                    start,
                    end,
                    &mut buf,
                    tap,
                );
                // Its sends to chains: before its pan, volume and mute, or
                // after them.
                for send in sends {
                    let from = if send.pre_volume {
                        pre.as_ref()
                    } else {
                        Some(&buf)
                    };
                    // What it sends goes as late as the bus's own output
                    // when the chain answers earlier.
                    let lag = delays.output(bus.channel.output) - delays.send(send.to);
                    if let (Some(from), Some(dst)) = (from, sums.send(send.to)) {
                        let gain = db_to_gain(send.gain_db);
                        if lag > 0 {
                            let mut sent =
                                AudioBuffer::silent(from.sample_rate, from.layout, frames);
                            add_scaled(&mut sent, from, gain);
                            let key = DelayKey::BusSend(id, send.to);
                            pool.delay_into(key, lag as usize, start, &sent, dst);
                        } else {
                            add_scaled(dst, from, gain);
                        }
                    }
                }
                pool.strip_peaks.push((strip, peaks(&buf)));
                pool.meters
                    .measure(meters::MeterOf::Strip(strip), start, &buf);
                if let Some(kept) = &mut pool.bus_outputs {
                    kept.push((id, buf.clone()));
                }
                match bus.channel.output {
                    Output::Hardware(pair) => sums.add_hardware(pair, &buf),
                    to => add_into(sums.output(to), &buf),
                }
            }
        }
    }
    pool.hardware = std::mem::take(&mut sums.hardware);
    if project.master.muted {
        let rate = project.settings.sample_rate;
        return AudioBuffer::silent(rate, project.settings.channels, frames);
    }
    let mut out = sums.master;
    let fx: Vec<&Effect> = if project.bypass_audio_fx {
        Vec::new()
    } else {
        project.master_fx.iter().collect()
    };
    let rate = project.settings.sample_rate.0;
    if !project.master_fx_post_fader {
        pool.run_fx(FxKey::Master, &fx, &[], rate, start, end, &mut out.channels);
    }
    scale(&mut out, db_to_gain(project.master.gain_db));
    // A 5.1 master's channels, each by its own fader.
    if project.settings.channels == ChannelLayout::Surround51 {
        for (c, &db) in out.channels.iter_mut().zip(&project.master.channel_db) {
            if db != 0.0 {
                let g = db_to_gain(db);
                c.iter_mut().for_each(|s| *s *= g);
            }
        }
    }
    if project.master_fx_post_fader {
        pool.run_fx(FxKey::Master, &fx, &[], rate, start, end, &mut out.channels);
    }
    pool.lfe_lowpass(project, start, end, &mut out);
    pool.meters.measure(meters::MeterOf::Master, start, &out);
    out
}

/// Adds `track`'s sound in the block to its output and its sends; its
/// peaks (`None` when it adds nothing). A track not heard (muted, or
/// others soloed) still plays into its pre-volume sends unless they
/// listen to its mute.
fn add_track(
    project: &Project,
    track: &bv_timeline::Track,
    start: i64,
    frames: usize,
    delays: &Delays,
    pool: &mut AudioPool,
    sums: &mut Sums,
) -> Option<Vec<f32>> {
    pool.tempo = project.settings.tempo;
    let rate = project.settings.sample_rate;
    let layout = project.settings.channels;
    // FX automation: the effects as their envelopes have them now.
    let automated = bv_timeline::fx_automation::automated(
        &track.effects,
        &track.envelopes,
        rate.sample_start(start),
    );
    let track_fx: Vec<&Effect> = if project.bypass_audio_fx {
        Vec::new()
    } else {
        automated.iter().collect()
    };
    // Plug-in delay compensation: effects that answer late (a VST 3
    // plug-in's latency) have the track's sound read that much earlier,
    // so that it lines up with the other tracks'.
    let late = if track_fx.is_empty() {
        0
    } else {
        pool.chain_latency(
            FxKey::Track(track.id),
            &track_fx,
            rate.0,
            layout.channel_count(),
        )
    };
    // Along its routing: as early as the latest path it plays or sends
    // into needs, the others through delay lines.
    let main_ahead = delays.output(track.output);
    let ahead = track
        .sends
        .iter()
        .filter(|s| db_to_gain(s.gain_db) > 0.0)
        .map(|s| delays.send(s.to))
        .fold(main_ahead, i64::max);
    let block_start = start;
    let start = start + late as i64 + ahead;
    let block_end = start + frames as i64;
    let block =
        bv_core::TimeRange::from_bounds(rate.sample_start(start), rate.sample_start(block_end));
    {
        let heard = project.is_audible(track);
        let pre_heard = heard || !prefader_sends_listen_to_mute();
        let sends: Vec<_> = track
            .sends
            .iter()
            .filter(|s| db_to_gain(s.gain_db) > 0.0)
            .filter(|s| if s.pre_volume { pre_heard } else { heard })
            .collect();
        let main = heard && track.level > 0.0;
        if !main && sends.iter().all(|s| !s.pre_volume) {
            return None;
        }
        let mut bus = AudioBuffer::silent(rate, layout, frames);
        let mut any = false;
        for clip in track.clips_overlapping(block) {
            // Quadro congelado é coisa do vídeo: o áudio do clipe fica mudo.
            if clip.level <= 0.0 || clip.muted || clip.freeze.is_some() {
                continue;
            }
            let Some(info) = project.media(clip.media) else {
                continue;
            };
            let s0 = first_sample_at_or_after(rate, clip.start).max(start);
            let s1 = first_sample_at_or_after(rate, clip.end()).min(block_end);
            if s1 <= s0 {
                continue;
            }
            let quality = project.settings.resample_quality;
            // Its media's effects, then its own (before fades and level);
            // effects that answer late (a plug-in's latency) have it read
            // that much ahead, up to its end — plug-in delay compensation.
            let effects: Vec<&Effect> = if project.bypass_audio_fx {
                Vec::new()
            } else {
                info.effects.iter().chain(&clip.effects).collect()
            };
            let channels = info.audio.map_or(0, |a| usize::from(a.channels));
            // Effects that change its length: it plays its sound made
            // through its whole chain (see `prerender`), once made.
            let made = if effects.iter().any(|e| e.enabled && e.kind.works_offline()) {
                let block = pool.block;
                pool.prerenders
                    .sound(info, clip, &effects, rate, quality, pool.tempo, block)
            } else {
                None
            };
            if let Some(made) = made {
                let from = (s0 - first_sample_at_or_after(rate, clip.start)).max(0) as usize;
                let n = (s1 - s0) as usize;
                let mut src: Vec<Vec<f32>> = made
                    .iter()
                    .map(|m| {
                        let mut v = vec![0.0f32; n];
                        if from < m.len() {
                            let k = (m.len() - from).min(n);
                            v[..k].copy_from_slice(&m[from..from + k]);
                        }
                        v
                    })
                    .collect();
                fade_clip(project, track, clip, rate, s0, &mut src);
                add_mapped(&mut bus, &src, (s0 - start) as usize, clip.level);
                any = true;
                continue;
            }
            let late = if effects.is_empty() || channels == 0 {
                0
            } else {
                pool.chain_latency(FxKey::Clip(clip.id), &effects, rate.0, channels) as i64
            };
            let end = first_sample_at_or_after(rate, clip.end());
            let (r0, r1) = (s0 + late, (s1 + late).min(end));
            let Some(slot) = pool.reader(info, clip.audio_stream, clip.id) else {
                continue;
            };
            let reader = pool.readers[slot].reader.as_mut();
            // Stretched with its pitch kept (Options › Preferences › Editing:
            // "Preserve pitch when stretching audio events"), or its pitch
            // changed: grains (`stretch.rs`).
            let keep_pitch = ((clip.preserve_pitch && (clip.rate - 1.0).abs() > 1e-9)
                || clip.pitch != 0.0)
                && clip.reverse.is_none()
                && clip.velocity().is_none();
            let n = (s1 - s0) as usize;
            // With Loop the media starts over: each lap is read on its own.
            let mut src: Option<Vec<Vec<f32>>> = None;
            let read = if r1 > r0 {
                loop_segments(clip, rate, r0, r1, info.duration)
            } else {
                Vec::new()
            };
            for (a, b, shift) in read {
                let len = (b - a) as usize;
                let part = if keep_pitch {
                    let shifts = pool.stretch.entry(clip.id).or_default();
                    stretch::read(reader, shifts, clip, rate, a, len, shift)
                } else {
                    read_clip(reader, clip, rate, a, len, quality, shift)
                };
                let Ok(part) = part else {
                    continue;
                };
                let dst = src.get_or_insert_with(|| vec![vec![0.0f32; n]; part.len()]);
                for (d, p) in dst.iter_mut().zip(&part) {
                    for (o, v) in d[(a - r0) as usize..].iter_mut().zip(p) {
                        *o = *v;
                    }
                }
            }
            // Read past its end: what its late effects still hold comes out.
            let src = match src {
                Some(s) => Some(s),
                None if late > 0 => Some(vec![vec![0.0f32; n]; channels]),
                None => None,
            };
            let Some(mut src) = src else {
                continue;
            };
            apply_switches(clip, &mut src);
            if !project.bypass_audio_fx {
                pool.run_fx(
                    FxKey::Clip(clip.id),
                    &effects,
                    &[],
                    rate.0,
                    s0 + late,
                    s1 + late,
                    &mut src,
                );
            }
            fade_clip(project, track, clip, rate, s0, &mut src);
            add_mapped(&mut bus, &src, (s0 - start) as usize, clip.level);
            any = true;
        }
        if any || !track_fx.is_empty() {
            pool.run_fx(
                FxKey::Track(track.id),
                &track_fx,
                &track.envelopes,
                rate.0,
                start,
                block_end,
                &mut bus.channels,
            );
            if track.invert_phase {
                for ch in &mut bus.channels {
                    for s in ch.iter_mut() {
                        *s = -*s;
                    }
                }
            }
            // Auto Ducking: a listener goes down while its controllers
            // sound.
            if track.duck_listener && project.has_ducking() {
                duck(project, track, start, &mut bus);
            }
            // Its sends: from before its volume and mute (and their
            // envelopes), or after them; panned as the track, or from the
            // middle with its panning mode.
            // With automation off, the level each send's automation
            // control holds as a second trim.
            let off = track.automation.mode == bv_timeline::AutomationMode::Off;
            for send in sends {
                let gain = db_to_gain(send.gain_db) * if off { send.held } else { 1.0 };
                if sums.send(send.to).is_none() {
                    continue;
                }
                // A path that answers earlier than the latest: through its
                // delay line.
                let lag = ahead - delays.send(send.to);
                let mut held = (lag > 0).then(|| AudioBuffer::silent(rate, layout, frames));
                let dst = match held.as_mut() {
                    Some(h) => h,
                    None => sums.send(send.to).expect("a send's target"),
                };
                // Its envelope (VEGAS's bus envelope) over its level.
                let envelope = track
                    .envelopes
                    .iter()
                    .find(|e| e.kind == bv_timeline::EnvelopeKind::Send(send.to));
                let tap = Tap {
                    level: if send.pre_volume {
                        gain
                    } else {
                        gain * track.level
                    },
                    volume: !send.pre_volume,
                    mute: !send.pre_volume || prefader_sends_listen_to_mute(),
                    panned: send.link_pan,
                };
                if track.has_automation() {
                    mix_with_envelopes(track, rate, start, &bus, dst, tap, envelope, None);
                } else {
                    let pan = if send.link_pan { track.pan } else { 0.0 };
                    add_panned(track.pan_mode, pan, tap.level, &bus, dst);
                }
                if let Some(h) = held
                    && let Some(dst) = sums.send(send.to)
                {
                    let key = DelayKey::Send(track.id, send.to);
                    pool.delay_into(key, lag as usize, block_start, &h, dst);
                }
            }
            if !main {
                // The VU/PPM meters fall: what it plays is nothing.
                let silent = AudioBuffer::silent(rate, layout, frames);
                pool.meters
                    .measure(meters::MeterOf::Track(track.id), start, &silent);
                return Some(vec![0.0; layout.channel_count()]);
            }
            // With VU/PPM meters, its output apart first, to be measured;
            // apart too when it waits for a later path (a delay line).
            let lag = ahead - main_ahead;
            let mut own = (meters::vu_meters().is_some() || lag > 0)
                .then(|| AudioBuffer::silent(rate, layout, frames));
            let out = match own.as_mut() {
                Some(own) => own,
                None => sums.output(track.output),
            };
            let surround = surround_of(project, track.surround.as_ref(), track.output);
            let peaks = if !track.has_automation() && surround.is_none() {
                add_panned(track.pan_mode, track.pan, track.level, &bus, out)
            } else {
                let tap = Tap {
                    level: track.level,
                    volume: true,
                    mute: true,
                    panned: true,
                };
                mix_with_envelopes(track, rate, start, &bus, out, tap, None, surround)
            };
            if let Some(own) = own {
                if lag > 0 {
                    let key = DelayKey::Output(track.id);
                    pool.delay_into(
                        key,
                        lag as usize,
                        block_start,
                        &own,
                        sums.output(track.output),
                    );
                } else {
                    add_into(sums.output(track.output), &own);
                }
                if meters::vu_meters().is_some() {
                    pool.meters
                        .measure(meters::MeterOf::Track(track.id), start, &own);
                }
            }
            return Some(peaks);
        }
    }
    None
}

/// Lowers listener `track`'s sound in `bus` (from sample `start`) as Auto
/// Ducking says (`Project::ducking_gain`): the gain worked out every few
/// samples, ramped between.
fn duck(project: &Project, track: &bv_timeline::Track, start: i64, bus: &mut AudioBuffer) {
    const STEP: usize = 32;
    let rate = project.settings.sample_rate;
    let frames = bus.channels.first().map_or(0, Vec::len);
    let gain = |i: usize| project.ducking_gain(track, rate.sample_start(start + i as i64));
    let mut a = gain(0);
    let mut i = 0;
    while i < frames {
        let n = STEP.min(frames - i);
        let b = gain(i + n);
        for k in 0..n {
            let g = a + (b - a) * k as f32 / n as f32;
            for ch in &mut bus.channels {
                ch[i + k] *= g;
            }
        }
        a = b;
        i += n;
    }
}

pub fn db_to_gain(db: f32) -> f32 {
    if db <= -96.0 {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

pub fn gain_to_db(g: f32) -> f32 {
    if g <= 1e-5 { -96.0 } else { 20.0 * g.log10() }
}

/// Adds `bus` times `level` to `out`, its first two channels placed by
/// the track's pan (`mode` at `pan`), the others as they are. Returns the
/// peak of each output channel.
fn add_panned(
    mode: bv_timeline::PanMode,
    pan: f32,
    level: f32,
    bus: &AudioBuffer,
    out: &mut AudioBuffer,
) -> Vec<f32> {
    let mut peaks = vec![0.0f32; out.channel_count()];
    let n = bus.channels.len().min(out.channels.len());
    let paired = n >= 2;
    if paired {
        let m = mode.matrix(pan);
        let (l, r) = (&bus.channels[0], &bus.channels[1]);
        for (c, o) in out.channels.iter_mut().enumerate().take(2) {
            let (a, b) = (m[c][0] * level, m[c][1] * level);
            for ((d, x), y) in o.iter_mut().zip(l).zip(r) {
                let v = x * a + y * b;
                *d += v;
                peaks[c] = peaks[c].max(v.abs());
            }
        }
    }
    let first = if paired { 2 } else { 0 };
    let rest = out
        .channels
        .iter_mut()
        .zip(&bus.channels)
        .zip(peaks.iter_mut());
    for ((o, b), peak) in rest.take(n).skip(first) {
        for (d, s) in o.iter_mut().zip(b) {
            let v = s * level;
            *d += v;
            *peak = peak.max(v.abs());
        }
    }
    peaks
}

/// The Master Bus's Downmix Output (playback only, VEGAS's monitoring
/// switch): what a listener with fewer speakers hears.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Downmix {
    #[default]
    Off,
    /// 5.1 to stereo.
    Stereo,
    /// To mono (5.1 through stereo).
    Mono,
}

/// Folds `channels` (laid out as `layout`) down as `to` says: 5.1's
/// center and surrounds into the front pair 3 dB down (the LFE left out,
/// as ITU-R BS.775 downmixes), then the pair to its mean on both sides.
pub fn downmix(channels: &mut [Vec<f32>], layout: bv_core::ChannelLayout, to: Downmix) {
    if to == Downmix::Off || channels.len() < 2 {
        return;
    }
    let k = std::f32::consts::FRAC_1_SQRT_2;
    let n = channels[0].len();
    if layout == bv_core::ChannelLayout::Surround51 && channels.len() >= 6 {
        for i in 0..n {
            let (c, ls, rs) = (channels[2][i], channels[4][i], channels[5][i]);
            channels[0][i] += k * (c + ls);
            channels[1][i] += k * (c + rs);
            for ch in &mut channels[2..6] {
                ch[i] = 0.0;
            }
        }
    }
    if to == Downmix::Mono {
        let (l, r) = channels.split_at_mut(1);
        for (a, b) in l[0].iter_mut().zip(r[0].iter_mut()) {
            let m = 0.5 * (*a + *b);
            *a = m;
            *b = m;
        }
    }
}

/// Ganho do fade no instante `t` do clipe (entrada × saída).
fn fade_gain(clip: &Clip, fade_in: Tick, fade_out: Tick, t: Tick) -> f32 {
    let mut g = 1.0;
    if fade_in > Tick::ZERO && t < clip.start + fade_in {
        let p = (t - clip.start).0 as f64 / fade_in.0 as f64;
        g *= clip.fade_in_curve.eval(p as f32);
    }
    if fade_out > Tick::ZERO && t >= clip.end() - fade_out {
        let p = (clip.end() - t).0 as f64 / fade_out.0 as f64;
        g *= clip.fade_out_curve.eval_out(p as f32);
    }
    g
}

/// Half the taps of the windowed-sinc interpolation (Best quality).
const SINC_HALF: i64 = 16;

/// Reads `n` samples of the clip from project sample `s0`, already at the
/// project rate. When the rates differ or the clip is stretched, the media
/// is interpolated with the project's resample quality: linear (Preview),
/// cubic (Good) or windowed sinc that also filters out what the lower rate
/// cannot hold (Best).
fn read_clip(
    reader: &mut dyn AudioReader,
    clip: &Clip,
    rate: SampleRate,
    s0: i64,
    n: usize,
    quality: AudioQuality,
    shift: Tick,
) -> Result<Vec<Vec<f32>>, MediaError> {
    let src_rate = reader.sample_rate();
    // `shift`: the laps of a looped event already played.
    let t0 = clip.source_time(rate.sample_start(s0)) - shift;
    let straight = clip.reverse.is_none() && clip.velocity().is_none();
    if src_rate == rate && clip.rate == 1.0 && straight {
        let first = src_rate.sample_at(t0);
        return Ok(reader.read(first, n)?.channels);
    }
    // Fractional media position of each output sample.
    let pos = |i: usize| -> f64 {
        let t = clip.source_time(rate.sample_start(s0 + i as i64)) - shift;
        t.0 as f64 * src_rate.0 as f64 / TICKS_PER_SECOND as f64
    };
    // Media samples needed around each position.
    let (before, after) = match quality {
        AudioQuality::Preview => (0, 1),
        AudioQuality::Good => (1, 2),
        AudioQuality::Best => (SINC_HALF - 1, SINC_HALF),
    };
    let (a, b) = (pos(0), pos(n.saturating_sub(1)));
    let p_first = a.min(b).floor() as i64 - before;
    let p_last = a.max(b).floor() as i64 + after;
    let src = reader.read(p_first, (p_last - p_first + 1) as usize)?;
    // Media samples per output sample: above 1 the sinc is widened to
    // filter out what the project rate cannot hold.
    let step = if n > 1 {
        ((b - a) / (n - 1) as f64).abs()
    } else {
        1.0
    };
    let cutoff = (1.0 / step.max(1.0)) as f32;
    let mut out = vec![vec![0.0f32; n]; src.channel_count()];
    for i in 0..n {
        let p = pos(i);
        let k = p.floor() as i64 - p_first;
        let f = (p - p.floor()) as f32;
        for (ch, o) in src.channels.iter().zip(out.iter_mut()) {
            // Clamped to the samples read (the edges repeat).
            let at = |j: i64| ch[(k + j).clamp(0, ch.len() as i64 - 1) as usize];
            o[i] = match quality {
                AudioQuality::Preview => at(0) + (at(1) - at(0)) * f,
                AudioQuality::Good => {
                    // Catmull-Rom cubic through the 4 nearest samples.
                    let (y0, y1, y2, y3) = (at(-1), at(0), at(1), at(2));
                    let c1 = 0.5 * (y2 - y0);
                    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
                    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
                    ((c3 * f + c2) * f + c1) * f + y1
                }
                AudioQuality::Best => {
                    let (mut acc, mut sum) = (0.0f32, 0.0f32);
                    for j in -(SINC_HALF - 1)..=SINC_HALF {
                        let x = j as f32 - f;
                        let w = sinc(x * cutoff) * sinc(x / SINC_HALF as f32);
                        acc += at(j) * w;
                        sum += w;
                    }
                    if sum.abs() > 1e-6 { acc / sum } else { at(0) }
                }
            };
        }
    }
    Ok(out)
}

/// The project samples `[s0, s1)` of a clip split where its media starts
/// over (Loop): (first, end, media time to take off). One piece without
/// Loop.
fn loop_segments(
    clip: &Clip,
    rate: SampleRate,
    s0: i64,
    s1: i64,
    media_len: Tick,
) -> Vec<(i64, i64, Tick)> {
    if !clip.looped
        || clip.freeze.is_some()
        || clip.reverse.is_some()
        || clip.velocity().is_some()
        || media_len <= Tick::ZERO
    {
        return vec![(s0, s1, Tick::ZERO)];
    }
    let len = media_len.0;
    let lap = |s: i64| clip.source_time(rate.sample_start(s)).0.div_euclid(len);
    let mut out = Vec::new();
    let mut a = s0;
    while a < s1 && out.len() < 10_000 {
        let k = lap(a);
        let next = clip.start + clip.to_timeline(Tick((k + 1) * len) - clip.source_in);
        let mut b = first_sample_at_or_after(rate, next).clamp(a + 1, s1);
        // Rounding: the piece ends where the lap really changes.
        while b < s1 && lap(b) == k {
            b += 1;
        }
        while b - 1 > a && lap(b - 1) != k {
            b -= 1;
        }
        out.push((a, b, Tick(k * len)));
        a = b;
    }
    out
}

/// sin(πx)/(πx).
fn sinc(x: f32) -> f32 {
    if x.abs() < 1e-6 {
        1.0
    } else {
        let px = std::f32::consts::PI * x;
        px.sin() / px
    }
}

/// A ramp `len` long at the edges that have no fade of their own, so that
/// cuts do not click.
fn quick_fade_gain(clip: &Clip, fade_in: Tick, fade_out: Tick, t: Tick, len: Tick) -> f32 {
    let mut g = 1.0;
    if fade_in == Tick::ZERO && t < clip.start + len {
        g *= ((t - clip.start).0 as f64 / len.0 as f64).clamp(0.0, 1.0) as f32;
    }
    if fade_out == Tick::ZERO && t > clip.end() - len {
        g *= ((clip.end() - t).0 as f64 / len.0 as f64).clamp(0.0, 1.0) as f32;
    }
    g
}

/// Where a track's sound is taken on its way out: at a level, through its
/// volume (its envelope, or the automation controls' volume) or not,
/// through its mute (its envelope) or not, panned as the track or from the
/// middle.
#[derive(Clone, Copy)]
struct Tap {
    level: f32,
    volume: bool,
    mute: bool,
    panned: bool,
}

/// Adds a track bus to the output following its volume, pan and mute
/// envelopes (sample by sample) as `tap` says — a send's own envelope,
/// `send`, too; in 5.1 with its Surround Panner (`surround`), its pair
/// placed among the six channels. Returns the peak per output channel.
#[allow(clippy::too_many_arguments)]
fn mix_with_envelopes(
    track: &bv_timeline::Track,
    rate: bv_core::SampleRate,
    start: i64,
    bus: &AudioBuffer,
    out: &mut AudioBuffer,
    tap: Tap,
    send: Option<&bv_timeline::Envelope>,
    surround: Option<&bv_timeline::SurroundPan>,
) -> Vec<f32> {
    let mut peaks = vec![0.0f32; out.channel_count()];
    use bv_timeline::{AutomationMode, EnvelopeKind};
    // With automation off the envelopes do not play: the automation
    // controls' volume and pan do, as a second trim.
    let off = track.automation.mode == AutomationMode::Off;
    let env = |k: EnvelopeKind| {
        (!off)
            .then(|| track.envelopes.iter().find(|e| e.kind == k))
            .flatten()
    };
    let (volume, pan, mute) = (
        env(EnvelopeKind::Volume),
        env(EnvelopeKind::Pan),
        env(EnvelopeKind::Mute),
    );
    let (held_volume, held_pan) = if off {
        (track.automation.volume, track.automation.pan)
    } else {
        (1.0, 0.0)
    };
    let (held_volume, volume, mute) = if tap.volume {
        (held_volume, volume, mute.filter(|_| tap.mute))
    } else {
        (1.0, None, mute.filter(|_| tap.mute))
    };
    let send = send.filter(|_| !off);
    let channels = bus.channel_count();
    let frames = bus.channels.first().map_or(0, Vec::len);
    let mut placed = Placement::default();
    for i in 0..frames {
        let t = rate.sample_start(start + i as i64);
        let mut g = tap.level * held_volume;
        if let Some(v) = volume {
            g *= v.value_at(t);
        }
        if let Some(e) = send {
            g *= e.value_at(t);
        }
        if let Some(m) = mute
            && m.value_at(t) < 0.5
        {
            g = 0.0;
        }
        if g == 0.0 {
            continue;
        }
        // The Surround Panner: the pair among 5.1's six channels.
        if let Some(sp) = surround.filter(|_| channels >= 6 && out.channel_count() >= 6) {
            let gains = placed.at(sp, track.pan_mode, i, (!off).then_some(t));
            let (l, r) = (bus.channels[0][i], bus.channels[1][i]);
            for (c, peak) in peaks.iter_mut().enumerate().take(6) {
                let own = if c >= 2 { bus.channels[c][i] } else { 0.0 };
                let v = (l * gains[c][0] + r * gains[c][1] + own) * g;
                out.channels[c][i] += v;
                *peak = peak.max(v.abs());
            }
            continue;
        }
        let p = pan.map_or(held_pan, |e| e.value_at(t));
        let p = if tap.panned {
            (track.pan + p).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        // The pan envelope places the sound as the track's pan type does.
        let m = (channels >= 2).then(|| track.pan_mode.matrix(p));
        for (c, peak) in peaks.iter_mut().enumerate().take(channels) {
            let v = match m {
                Some(m) if c < 2 => {
                    (bus.channels[0][i] * m[c][0] + bus.channels[1][i] * m[c][1]) * g
                }
                _ => bus.channels[c][i] * g,
            };
            out.channels[c][i] += v;
            *peak = peak.max(v.abs());
        }
    }
    peaks
}

/// A track's or a strip's Surround Panner, in a 5.1 project (never
/// panned: the middle of the room): only the Master (VEGAS's Surround
/// Master) takes 5.1, so what plays into a bus goes there stereo, and the
/// bus pans.
fn surround_of<'a>(
    project: &Project,
    sp: Option<&'a bv_timeline::SurroundPan>,
    output: Output,
) -> Option<&'a bv_timeline::SurroundPan> {
    (project.settings.channels == ChannelLayout::Surround51 && output == Output::Master)
        .then(|| sp.unwrap_or(&bv_timeline::surround::MIDDLE))
}

/// A Surround Panner's gains worked out again every 64 samples (the point
/// moves slowly).
#[derive(Default)]
struct Placement {
    gains: [[f32; 2]; 6],
    until: usize,
}

impl Placement {
    /// At sample `i`, time `t` (`None`: its keyframes do not play, its
    /// trim alone places it — the track's automation off).
    fn at(
        &mut self,
        sp: &bv_timeline::SurroundPan,
        mode: bv_timeline::PanMode,
        i: usize,
        t: Option<Tick>,
    ) -> [[f32; 2]; 6] {
        if i >= self.until {
            let point = t.map_or_else(|| sp.trim_point(), |t| sp.point_at(t));
            self.gains = sp.gains(mode, point);
            self.until = i + 64;
        }
        self.gains
    }
}

/// Applies the event switches (Edit › Switches / Channels): channel
/// routing, polarity inversion and the normalize gain.
fn apply_switches(clip: &Clip, src: &mut [Vec<f32>]) {
    if src.len() >= 2 {
        let (l, r) = src.split_at_mut(1);
        let (l, r) = (&mut l[0], &mut r[0]);
        match clip.channels {
            ChannelMap::Both => {}
            ChannelMap::LeftOnly => r.copy_from_slice(l),
            ChannelMap::RightOnly => l.copy_from_slice(r),
            ChannelMap::Combine => {
                for (a, b) in l.iter_mut().zip(r.iter_mut()) {
                    let m = (*a + *b) * 0.5;
                    *a = m;
                    *b = m;
                }
            }
            ChannelMap::Swap => std::mem::swap(l, r),
        }
    }
    let gain = clip.normalize.unwrap_or(1.0) * if clip.invert_phase { -1.0 } else { 1.0 };
    if gain != 1.0 {
        for ch in src.iter_mut() {
            for s in ch.iter_mut() {
                *s *= gain;
            }
        }
    }
}

/// Soma `src` em `dst` a partir de `offset`, adaptando o número de canais.
fn add_mapped(dst: &mut AudioBuffer, src: &[Vec<f32>], offset: usize, gain: f32) {
    let out_ch = dst.channel_count();
    let in_ch = src.len();
    if in_ch == 0 {
        return;
    }
    for (o, out) in dst.channels.iter_mut().enumerate() {
        let target = &mut out[offset..];
        if in_ch == 1 {
            // Mono vai igual para todos os canais.
            for (d, s) in target.iter_mut().zip(&src[0]) {
                *d += s * gain;
            }
        } else if out_ch == 1 {
            // Para mono: média de todos os canais.
            let k = gain / in_ch as f32;
            for ch in src {
                for (d, s) in target.iter_mut().zip(ch) {
                    *d += s * k;
                }
            }
        } else if o < in_ch {
            for (d, s) in target.iter_mut().zip(&src[o]) {
                *d += s * gain;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::{Generator, ProjectSettings, TrackKind};

    fn tone(p: &mut Project, hz: f32, db: f32) -> MediaId {
        let info = MediaInfo::generated(
            Generator::TestTone {
                frequency_hz: hz,
                level_db: db,
            },
            &p.settings,
        );
        p.add_media(info)
    }

    /// Auto Ducking: music on a listener track goes down by the
    /// controller's Floor Reduction while its narration sounds, and back up
    /// after.
    #[test]
    fn a_listener_goes_down_while_its_controller_sounds() {
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let music = p.tracks[1].id;
        let voice = p.add_track(TrackKind::Audio);
        let m = tone(&mut p, 440.0, -6.0);
        let v = tone(&mut p, 1000.0, -12.0);
        p.insert_media(m, Tick::ZERO, None, Some(music)).unwrap();
        let c = p.insert_media(v, Tick::ZERO, None, Some(voice)).unwrap()[0];
        // The narration's loudness: −15 dBFS for a second, then silence.
        let mut levels = vec![-15i8; 25];
        levels.extend(vec![-100i8; 50]);
        p.clip_mut(c).unwrap().duck_levels = Some(bv_timeline::DuckLevels {
            from: Tick::ZERO,
            levels,
            own: None,
        });
        // Only the music is heard here.
        p.track_by_id_mut(voice).unwrap().level = 0.0;
        let rate = p.settings.sample_rate;
        let peak = |p: &Project, at: f64| {
            let start = rate.sample_at(Tick::from_seconds_f64(at));
            let b = mix(p, start, 4800, &mut AudioPool::new());
            b.channels[0].iter().fold(0f32, |m, s| m.max(s.abs()))
        };
        let plain = peak(&p, 0.5);
        // A controller, a listener: down by 12 dB while it sounds.
        p.track_by_id_mut(voice).unwrap().ducking = Some(bv_timeline::Ducking::default());
        p.track_by_id_mut(music).unwrap().duck_listener = true;
        let ducked = peak(&p, 0.5);
        assert!((ducked / plain - 0.251).abs() < 0.01, "{plain} {ducked}");
        // Back up once it is silent.
        assert!((peak(&p, 1.8) - plain).abs() < 0.01);
        // The event's own reductions win over its track's.
        p.clip_mut(c).unwrap().duck_levels.as_mut().unwrap().own = Some(bv_timeline::Ducking {
            floor_db: -6.0,
            peak: 0.0,
        });
        assert!((peak(&p, 0.5) / plain - 0.501).abs() < 0.01);
    }

    /// Plug-in delay compensation: a track whose effects answer 300
    /// samples late plays its sound 300 samples early (the plug-in's
    /// latency then puts it back in place).
    #[test]
    fn a_late_chain_plays_its_track_early() {
        use bv_timeline::{Effect, EffectKind};
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let t = p.tracks[1].id;
        let m = tone(&mut p, 440.0, -6.0);
        p.insert_media(m, Tick::from_seconds(1), None, Some(t))
            .unwrap();
        p.track_by_id_mut(t)
            .unwrap()
            .effects
            .push(Effect::new(EffectKind::Gain));
        let rate = p.settings.sample_rate;
        let at = rate.sample_at(Tick::from_seconds(1));
        let first = |p: &Project| {
            let b = mix(p, at - 1000, 2000, &mut AudioPool::new());
            b.channels[0].iter().position(|s| s.abs() > 1e-6)
        };
        let base = first(&p).unwrap();
        assert!((1000..=1002).contains(&base), "{base}");
        dsp::FAKE_LATENCY.with(|l| l.set(300));
        let early = first(&p);
        dsp::FAKE_LATENCY.with(|l| l.set(0));
        assert_eq!(early, Some(base - 300));
    }

    /// Plug-in delay compensation along the routing: a bus whose effects
    /// answer 300 samples late has its tracks read 300 samples earlier; a
    /// track playing into the Master and sending into that bus waits for
    /// it there through a delay line (the line's first 300 samples
    /// silent).
    #[test]
    fn a_late_bus_has_its_tracks_read_ahead() {
        use bv_timeline::{Effect, EffectKind, Output, Send, SendTo};
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let t = p.tracks[1].id;
        let m = tone(&mut p, 440.0, -6.0);
        p.insert_media(m, Tick::from_seconds(1), None, Some(t))
            .unwrap();
        let bus = p.add_bus().unwrap();
        p.bus_mut(bus)
            .unwrap()
            .channel
            .effects
            .push(Effect::new(EffectKind::Gain));
        p.set_track_output(t, Output::Bus(bus));
        let rate = p.settings.sample_rate;
        let at = rate.sample_at(Tick::from_seconds(1));
        let blocks = |p: &Project| {
            let mut pool = AudioPool::new();
            let mut out = Vec::new();
            for k in 0..3 {
                let b = mix(p, at - 1000 + k * 1000, 1000, &mut pool);
                out.extend(b.channels[0].iter().copied());
            }
            out
        };
        let late = |p: &Project| {
            dsp::FAKE_LATENCY.with(|l| l.set(300));
            let out = blocks(p);
            dsp::FAKE_LATENCY.with(|l| l.set(0));
            out
        };
        let plain = blocks(&p);
        let ahead = late(&p);
        for i in 1000..2500 {
            assert!((ahead[i] - plain[i + 300]).abs() < 1e-5, "{i}");
        }
        // Into the Master, and sending into the late bus (muted): what it
        // plays into the Master is in time once the line has filled.
        p.set_track_output(t, Output::Master);
        let mut send = Send::new(SendTo::Bus(bus));
        send.gain_db = 0.0;
        p.set_send(t, send);
        p.bus_mut(bus).unwrap().channel.muted = true;
        let plain = blocks(&p);
        let waited = late(&p);
        assert!(plain[1500].abs() > 0.1);
        for i in 300..3000 {
            assert!((waited[i] - plain[i]).abs() < 1e-5, "{i}");
        }
        assert!(waited[..300].iter().all(|s| s.abs() < 1e-6));
    }

    /// Plug-in delay compensation on an event: its effects answering 300
    /// samples late, it is read 300 samples ahead.
    #[test]
    fn a_late_event_chain_reads_its_event_ahead() {
        use bv_timeline::{Effect, EffectKind};
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let t = p.tracks[1].id;
        let m = tone(&mut p, 440.0, -6.0);
        let c = p
            .insert_media(m, Tick::from_seconds(1), None, Some(t))
            .unwrap()[0];
        p.clip_mut(c)
            .unwrap()
            .effects
            .push(Effect::new(EffectKind::Gain));
        let rate = p.settings.sample_rate;
        let at = rate.sample_at(Tick::from_seconds(1));
        let block =
            |p: &Project| mix(p, at - 1000, 3000, &mut AudioPool::new()).channels[0].clone();
        let plain = block(&p);
        dsp::FAKE_LATENCY.with(|l| l.set(300));
        let ahead = block(&p);
        dsp::FAKE_LATENCY.with(|l| l.set(0));
        for i in 1000..1500 {
            assert!((ahead[i] - plain[i + 300]).abs() < 1e-5, "{i}");
        }
    }

    /// A bus on a hardware output plays there and not into the Master
    /// (so never into renders), the Master muted or not; another bus
    /// sends to it from after its volume.
    #[test]
    fn a_bus_on_a_hardware_output_leaves_the_master() {
        use bv_timeline::{Output, Send, SendTo};
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, -12.0);
        p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
        let plain = peak_of(&p);
        let peak = |c: &[f32]| c.iter().fold(0f32, |m, s| m.max(s.abs()));
        let hw = p.add_bus().unwrap();
        p.set_track_output(a, Output::Bus(hw));
        assert!(p.set_bus_output(hw, Output::Hardware(1)));
        assert!(!p.set_track_output(a, Output::Hardware(1)));
        p.check_invariants().unwrap();
        let mut pool = AudioPool::new();
        let master = mix(&p, 4800, 4800, &mut pool);
        assert!(peak(&master.channels[0]) < 1e-6);
        let out = pool.take_hardware();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, 1);
        assert!((peak(&out[0].1.channels[1]) / plain - 1.0).abs() < 1e-3);
        // The Master muted: the hardware output still plays.
        p.master.muted = true;
        let mut pool = AudioPool::new();
        mix(&p, 4800, 4800, &mut pool);
        assert_eq!(pool.take_hardware().len(), 1);
        p.master.muted = false;
        // Bus B into the Master sends to it at −6 dB, after its volume.
        let b = p.add_bus().unwrap();
        p.set_track_output(a, Output::Bus(b));
        let mut send = Send::of_bus(SendTo::Bus(hw));
        send.gain_db = -6.0206;
        assert!(p.set_bus_send(b, send));
        p.bus_mut(b).unwrap().channel.gain_db = -6.0206;
        let mut pool = AudioPool::new();
        let master = mix(&p, 4800, 4800, &mut pool);
        assert!((peak(&master.channels[0]) / plain - 0.5).abs() < 1e-3);
        let out = pool.take_hardware();
        assert!((peak(&out[0].1.channels[0]) / plain - 0.25).abs() < 1e-3);
    }

    /// FX automation: a track's Gain effect follows its envelope (−24 dB
    /// to +24 dB over two seconds), its Bypass envelope turns it off, and
    /// Bypass FX Automation brings back its own value.
    #[test]
    fn automated_effects_follow_their_envelopes() {
        use bv_timeline::{Effect, EffectKind, EnvelopeKind, EnvelopeOwner, KeyCurve};
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let t = p.tracks[1].id;
        let m = tone(&mut p, 440.0, -12.0);
        p.insert_media(m, Tick::ZERO, None, Some(t)).unwrap();
        p.track_by_id_mut(t)
            .unwrap()
            .effects
            .push(Effect::new(EffectKind::Gain));
        let owner = EnvelopeOwner::Track(t);
        assert!(p.set_fx_automation(owner, 0, &[Some(0), None], KeyCurve::Linear));
        let rate = p.settings.sample_rate;
        let peak = |p: &Project, at: f64| {
            let start = rate.sample_at(Tick::from_seconds_f64(at));
            let b = mix(p, start, 480, &mut AudioPool::new());
            b.channels[0].iter().fold(0f32, |m, s| m.max(s.abs()))
        };
        let plain = peak(&p, 0.5);
        {
            let track = p.track_by_id_mut(t).unwrap();
            let env = track
                .envelopes
                .iter_mut()
                .find(|e| e.kind == EnvelopeKind::FxParam { fx: 1, param: 0 })
                .unwrap();
            env.points[0].value = 0.0;
            env.points.push(bv_timeline::EnvelopePoint {
                at: Tick::from_seconds(2),
                value: 1.0,
                curve: KeyCurve::Linear,
            });
        }
        // −24 dB at the start, 0 dB halfway, +24 dB at the end.
        let db = |v: f32| 20.0 * (v / plain).log10();
        assert!(
            (db(peak(&p, 0.0)) + 24.0).abs() < 0.5,
            "{}",
            db(peak(&p, 0.0))
        );
        assert!(db(peak(&p, 1.0)).abs() < 0.5, "{}", db(peak(&p, 1.0)));
        // Bypass FX Automation: its own 0 dB again.
        p.track_by_id_mut(t).unwrap().effects[0].automation_bypassed = true;
        assert!(db(peak(&p, 0.0)).abs() < 0.1);
        p.track_by_id_mut(t).unwrap().effects[0].automation_bypassed = false;
        // The Bypass envelope off: the effect bypassed, 0 dB at the start.
        let track = p.track_by_id_mut(t).unwrap();
        let bypass = track
            .envelopes
            .iter_mut()
            .find(|e| e.kind == EnvelopeKind::FxBypass(1))
            .unwrap();
        bypass.points[0].value = 0.0;
        assert!(db(peak(&p, 0.0)).abs() < 0.1);
    }

    /// The peak of channel 0 of the mix of `p` around 0.1 s.
    fn peak_of(p: &Project) -> f32 {
        let b = mix(p, 4800, 4800, &mut AudioPool::new());
        b.channels[0].iter().fold(0f32, |m, s| m.max(s.abs()))
    }

    /// Busses and assignable effects chains, as VEGAS mixes them: a track
    /// into a bus, a bus into another, sends from before and after the
    /// track's volume, a chain's input level and effects, solo, and the
    /// Master's effects.
    #[test]
    fn busses_and_chains_route_the_sound() {
        use bv_timeline::{EffectKind, Output, Send, SendTo};
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, -12.0);
        p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
        let plain = peak_of(&p);
        // The track into bus A at −6 dB.
        let bus_a = p.add_bus().unwrap();
        p.bus_mut(bus_a).unwrap().channel.gain_db = -6.0206;
        p.set_track_output(a, Output::Bus(bus_a));
        assert!((peak_of(&p) / plain - 0.5).abs() < 1e-3);
        // Bus A into bus B at −6 dB: a quarter.
        let bus_b = p.add_bus().unwrap();
        p.bus_mut(bus_b).unwrap().channel.gain_db = -6.0206;
        assert!(p.set_bus_output(bus_a, Output::Bus(bus_b)));
        assert!((peak_of(&p) / plain - 0.25).abs() < 1e-3);
        // A muted bus: nothing reaches the Master.
        p.bus_mut(bus_b).unwrap().channel.muted = true;
        assert!(peak_of(&p) < 1e-6);
        p.bus_mut(bus_b).unwrap().channel.muted = false;
        // Back to the Master, the track's volume at half: a send to bus A
        // from before its volume hears it whole.
        p.set_track_output(a, Output::Master);
        p.set_bus_output(bus_a, Output::Master);
        p.bus_mut(bus_a).unwrap().channel.gain_db = 0.0;
        p.track_by_id_mut(a).unwrap().level = 0.5;
        let mut send = Send::new(SendTo::Bus(bus_a));
        send.gain_db = 0.0;
        p.set_send(a, send);
        assert!(
            (peak_of(&p) / plain - 1.5).abs() < 1e-3,
            "{}",
            peak_of(&p) / plain
        );
        // From after it: half.
        send.pre_volume = false;
        p.set_send(a, send);
        assert!((peak_of(&p) / plain - 1.0).abs() < 1e-3);
        p.set_send(a, Send::new(SendTo::Bus(bus_a)));
        // A chain: its input at −6 dB, a Gain of +12 dB, sent from after
        // the volume at 0 dB: half × half × 4.
        let mut gain = bv_timeline::Effect::new(EffectKind::Gain);
        gain.set_param(0, 12.0412);
        let fx = p.add_fx_chain(vec![gain]).unwrap();
        p.fx_chain_mut(fx).unwrap().input_db = -6.0206;
        let mut send = Send::new(SendTo::Fx(fx));
        send.gain_db = 0.0;
        p.set_send(a, send);
        assert!(
            (peak_of(&p) / plain - 1.5).abs() < 1e-3,
            "{}",
            peak_of(&p) / plain
        );
        // Soloed, the chain alone of the busses and chains plays (the
        // track itself still does).
        p.fx_chain_mut(fx).unwrap().channel.solo = true;
        p.bus_mut(bus_b).unwrap().channel.gain_db = 0.0;
        p.set_track_output(a, Output::Bus(bus_b));
        assert!(
            (peak_of(&p) / plain - 1.0).abs() < 1e-3,
            "{}",
            peak_of(&p) / plain
        );
        p.fx_chain_mut(fx).unwrap().channel.solo = false;
        // A soloed bus mutes the other busses, not the chains: bus A
        // soloed, B (the track's output) silent, the chain playing.
        p.bus_mut(bus_a).unwrap().channel.solo = true;
        assert!(
            (peak_of(&p) / plain - 1.0).abs() < 1e-3,
            "{}",
            peak_of(&p) / plain
        );
        p.bus_mut(bus_a).unwrap().channel.solo = false;
        p.set_track_output(a, Output::Master);
        // The Master's effects: the Gain again.
        let mut twice = bv_timeline::Effect::new(EffectKind::Gain);
        twice.set_param(0, 6.0206);
        p.master_fx.push(twice);
        assert!(
            (peak_of(&p) / plain - 3.0).abs() < 2e-3,
            "{}",
            peak_of(&p) / plain
        );
    }

    /// A bus's envelopes (its bus track's): volume and mute over time, pan
    /// added to its own; and a track's send envelope (VEGAS's bus
    /// envelope) over the send's level.
    #[test]
    fn bus_tracks_and_sends_follow_their_envelopes() {
        use bv_timeline::{EnvelopeKind, EnvelopeOwner, KeyCurve, Output, SendTo, Strip};
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, -12.0);
        p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
        let rate = p.settings.sample_rate;
        let peak = |p: &Project, at: f64, channel: usize| {
            let start = rate.sample_at(Tick::from_seconds_f64(at));
            let b = mix(p, start, 960, &mut AudioPool::new());
            b.channels[channel].iter().fold(0f32, |m, s| m.max(s.abs()))
        };
        let plain = peak(&p, 0.5, 0);
        let bus = p.add_bus().unwrap();
        p.set_track_output(a, Output::Bus(bus));
        let strip = Strip::Bus(bus);
        // Volume: whole at 0 s, half at 1 s (a straight line in gain).
        assert!(
            p.toggle_strip_envelope(strip, EnvelopeKind::Volume, KeyCurve::Linear)
                .unwrap()
        );
        p.set_envelope_point(
            EnvelopeOwner::Strip(strip),
            EnvelopeKind::Volume,
            Tick::from_seconds(1),
            0.5,
        )
        .unwrap();
        assert!((peak(&p, 0.0, 0) / plain - 1.0).abs() < 0.01);
        assert!((peak(&p, 1.5, 0) / plain - 0.5).abs() < 0.01);
        // Mute from 2 s.
        p.toggle_strip_envelope(strip, EnvelopeKind::Mute, KeyCurve::Hold)
            .unwrap();
        p.set_envelope_point(
            EnvelopeOwner::Strip(strip),
            EnvelopeKind::Mute,
            Tick::from_seconds(2),
            0.0,
        )
        .unwrap();
        assert!(peak(&p, 2.5, 0) < 1e-6);
        assert!(peak(&p, 1.5, 0) > 0.0);
        // Pan to the right: the left side goes.
        p.toggle_strip_envelope(strip, EnvelopeKind::Mute, KeyCurve::Hold)
            .unwrap();
        p.toggle_strip_envelope(strip, EnvelopeKind::Volume, KeyCurve::Linear)
            .unwrap();
        p.toggle_strip_envelope(strip, EnvelopeKind::Pan, KeyCurve::Linear)
            .unwrap();
        p.set_envelope_point(
            EnvelopeOwner::Strip(strip),
            EnvelopeKind::Pan,
            Tick::ZERO,
            1.0,
        )
        .unwrap();
        assert!(peak(&p, 0.5, 0) < 1e-6);
        assert!((peak(&p, 0.5, 1) / plain - 1.0).abs() < 0.01);
        // The send's envelope: the track back to the Master, a send to the
        // bus made at 0 dB with it, then halved by it.
        p.toggle_strip_envelope(strip, EnvelopeKind::Pan, KeyCurve::Linear)
            .unwrap();
        p.set_track_output(a, Output::Master);
        let kind = EnvelopeKind::Send(SendTo::Bus(bus));
        assert!(p.toggle_envelope(a, kind).unwrap());
        assert!(p.track(a).unwrap().sends.iter().any(|s| s.gain_db == 0.0));
        assert!((peak(&p, 0.5, 0) / plain - 2.0).abs() < 0.02);
        p.set_envelope_point(a, kind, Tick::ZERO, 0.5).unwrap();
        assert!((peak(&p, 0.5, 0) / plain - 1.5).abs() < 0.02);
    }

    /// With a bus track's automation off its envelopes do not play: the
    /// volume its automation control holds does, over its trim; a track's
    /// send with the track's automation off takes the level its control
    /// holds.
    #[test]
    fn automation_off_holds_bus_and_send_levels() {
        use bv_timeline::{AutomationMode, EnvelopeKind, EnvelopeOwner, KeyCurve, Output};
        use bv_timeline::{SendTo, Strip};
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, -12.0);
        p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
        let rate = p.settings.sample_rate;
        let peak = |p: &Project| {
            let start = rate.sample_at(Tick::from_seconds_f64(0.5));
            let b = mix(p, start, 960, &mut AudioPool::new());
            b.channels[0].iter().fold(0f32, |m, s| m.max(s.abs()))
        };
        let plain = peak(&p);
        let bus = p.add_bus().unwrap();
        p.set_track_output(a, Output::Bus(bus));
        let strip = Strip::Bus(bus);
        // A mute envelope silences the bus...
        p.toggle_strip_envelope(strip, EnvelopeKind::Mute, KeyCurve::Hold)
            .unwrap();
        let owner = EnvelopeOwner::Strip(strip);
        p.set_envelope_point(owner, EnvelopeKind::Mute, Tick::ZERO, 0.0)
            .unwrap();
        assert!(peak(&p) < 1e-6);
        // ...but not with its automation off: the volume held plays.
        let c = p.strip_channel_mut(strip).unwrap();
        c.automation.mode = AutomationMode::Off;
        c.automation.volume = 0.5;
        assert!((peak(&p) / plain - 0.5).abs() < 0.01);
        // The track back to the Master, a send to the bus at 0 dB: twice;
        // the track's automation off and the send's control at half.
        p.set_track_output(a, Output::Master);
        p.strip_channel_mut(strip).unwrap().automation.volume = 1.0;
        p.toggle_envelope(a, EnvelopeKind::Send(SendTo::Bus(bus)))
            .unwrap();
        assert!((peak(&p) / plain - 2.0).abs() < 0.02);
        let t = p.track_by_id_mut(a).unwrap();
        t.automation.mode = AutomationMode::Off;
        t.sends[0].held = 0.5;
        assert!((peak(&p) / plain - 1.5).abs() < 0.02);
    }

    /// In 5.1 a track's Surround Panner places its pair among the six
    /// channels: never panned, in the middle of the room — every speaker
    /// at −6 dB —; at the back only the surrounds. Into a bus a track goes
    /// stereo, and the bus's panner places it.
    #[test]
    fn the_surround_panner_places_tracks_in_the_room() {
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let a = p.tracks[1].id;
        // A stereo tone (made before the project goes 5.1).
        let m = tone(&mut p, 1000.0, -12.0);
        p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
        let peaks = |p: &Project| -> Vec<f32> {
            let b = mix(p, 4800, 4800, &mut AudioPool::new());
            b.channels
                .iter()
                .map(|c| c.iter().fold(0f32, |m, s| m.max(s.abs())))
                .collect()
        };
        let plain = peaks(&p)[0];
        assert!(plain > 0.1);
        p.settings.channels = ChannelLayout::Surround51;
        let middle = peaks(&p);
        for c in [0, 1, 2, 4, 5] {
            assert!(
                (middle[c] / plain - 0.5).abs() < 0.01,
                "{c}: {}",
                middle[c] / plain
            );
        }
        assert_eq!(middle[3], 0.0);
        p.track_by_id_mut(a).unwrap().surround = Some(bv_timeline::SurroundPan {
            y: -1.0,
            ..Default::default()
        });
        let back = peaks(&p);
        assert!(back[0] < 1e-6 && back[2] < 1e-6 && (back[4] / plain - 1.0).abs() < 0.01);
        // Into a bus: stereo there (the track's own panner left aside);
        // the bus, never panned, in the middle; then panned to the back
        // right.
        let bus = p.add_bus().unwrap();
        p.set_track_output(a, Output::Bus(bus));
        let through = peaks(&p);
        assert!((through[0] / plain - 0.5).abs() < 0.01);
        assert!((through[4] / plain - 0.5).abs() < 0.01);
        let b = p.bus_mut(bus).unwrap();
        b.channel.surround = Some(bv_timeline::SurroundPan {
            x: 1.0,
            y: -1.0,
            ..Default::default()
        });
        let panned = peaks(&p);
        assert!(
            panned[0] < 1e-6 && panned[4] < 1e-6 && panned[5] > 0.1,
            "{panned:?}"
        );
    }

    /// A bus's effects before its volume, or after it: a limiter at −6 dB
    /// on a bus at −6 dB.
    #[test]
    fn a_bus_runs_its_effects_before_or_after_its_volume() {
        use bv_timeline::{EffectKind, Output};
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, 0.0);
        p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
        let bus = p.add_bus().unwrap();
        p.set_track_output(a, Output::Bus(bus));
        let mut limiter = bv_timeline::Effect::new(EffectKind::Limiter);
        limiter.set_param(0, -6.0);
        let b = p.bus_mut(bus).unwrap();
        b.channel.effects.push(limiter);
        b.channel.gain_db = -6.0206;
        // Once the limiter has taken hold (its first peak gets through).
        let settled = |p: &Project| {
            let b = mix(p, 4800, 4800, &mut AudioPool::new());
            b.channels[0][2400..]
                .iter()
                .fold(0f32, |m, s| m.max(s.abs()))
        };
        // Before: limited to −6 dB, then −6 dB: a quarter.
        let before = settled(&p);
        assert!((before - 0.25).abs() < 0.02, "{before}");
        // After: −6 dB, then not limited: a half.
        p.bus_mut(bus).unwrap().channel.fx_post_fader = true;
        let after = settled(&p);
        assert!((after - 0.5).abs() < 0.02, "{after}");
    }

    /// A track's pre-volume sends play on while it is muted (a cue mix),
    /// unless they listen to its mute; its post-volume sends go silent
    /// with it.
    #[test]
    fn pre_volume_sends_play_past_the_tracks_mute() {
        use bv_timeline::{Send, SendTo};
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, -12.0);
        p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
        let plain = peak_of(&p);
        let bus = p.add_bus().unwrap();
        let mut send = Send::new(SendTo::Bus(bus));
        send.gain_db = -6.0206;
        p.set_send(a, send);
        p.track_by_id_mut(a).unwrap().muted = true;
        // Muted: the send alone, at half; its volume down too.
        assert!(
            (peak_of(&p) / plain - 0.5).abs() < 1e-3,
            "{}",
            peak_of(&p) / plain
        );
        p.track_by_id_mut(a).unwrap().level = 0.0;
        assert!((peak_of(&p) / plain - 0.5).abs() < 1e-3);
        // Another track soloed: still.
        let b = p.add_track(TrackKind::Audio);
        p.track_by_id_mut(b).unwrap().solo = true;
        assert!((peak_of(&p) / plain - 0.5).abs() < 1e-3);
        p.track_by_id_mut(b).unwrap().solo = false;
        // Listening to the mute: silence.
        set_prefader_sends_listen_to_mute(true);
        let listening = peak_of(&p);
        set_prefader_sends_listen_to_mute(false);
        assert!(listening < 1e-6, "{listening}");
        // From after the volume: silence.
        send.pre_volume = false;
        p.set_send(a, send);
        assert!(peak_of(&p) < 1e-6);
    }

    /// A bus sends to a chain from before its volume and mute, or after
    /// them; the chain mixed before the bus it plays into.
    #[test]
    fn a_bus_sends_to_a_chain_before_or_after_its_volume() {
        use bv_timeline::{Output, Send, SendTo};
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, -12.0);
        p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
        let plain = peak_of(&p);
        let bus = p.add_bus().unwrap();
        p.set_track_output(a, Output::Bus(bus));
        p.bus_mut(bus).unwrap().channel.gain_db = -6.0206;
        let fx = p.add_fx_chain(Vec::new()).unwrap();
        let mut send = Send::new(SendTo::Fx(fx));
        send.gain_db = 0.0;
        assert!(p.set_bus_send(bus, send));
        // After its volume: half, and half again through the chain.
        assert!(
            (peak_of(&p) / plain - 1.0).abs() < 1e-3,
            "{}",
            peak_of(&p) / plain
        );
        // Before it: half and whole.
        send.pre_volume = true;
        assert!(p.set_bus_send(bus, send));
        assert!(
            (peak_of(&p) / plain - 1.5).abs() < 1e-3,
            "{}",
            peak_of(&p) / plain
        );
        // Muted, the bus still sends from before its mute.
        p.bus_mut(bus).unwrap().channel.muted = true;
        assert!(
            (peak_of(&p) / plain - 1.0).abs() < 1e-3,
            "{}",
            peak_of(&p) / plain
        );
        // The chain into another bus at −6 dB, mixed after the chain.
        let c = p.add_bus().unwrap();
        p.bus_mut(c).unwrap().channel.gain_db = -6.0206;
        assert!(p.set_chain_output(fx, Output::Bus(c)));
        assert!(
            (peak_of(&p) / plain - 0.5).abs() < 1e-3,
            "{}",
            peak_of(&p) / plain
        );
    }

    /// With VU/PPM meters asked for, each block reads each track, bus and
    /// the Master: a PPM settles on the tone's amplitude where it plays.
    #[test]
    fn vu_meters_read_tracks_busses_and_the_master() {
        use bv_timeline::Output;
        use meters::{MeterKind, MeterOf};
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, -12.0);
        p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
        let bus = p.add_bus().unwrap();
        p.set_track_output(a, Output::Bus(bus));
        meters::set_vu_meters(Some(MeterKind::EbuPpm), false);
        assert_eq!(meters::vu_meters(), Some((MeterKind::EbuPpm, false)));
        let mut pool = AudioPool::new();
        let mut last = Vec::new();
        for k in 0..10 {
            let out = mix(&p, k * 4800, 4800, &mut pool);
            assert!(!out.channels[0].is_empty());
            last = pool.take_vu_levels();
        }
        meters::set_vu_meters(None, false);
        let level = |of| {
            last.iter()
                .find(|(o, _)| *o == of)
                .map_or(0.0, |(_, l): &(MeterOf, Vec<f32>)| l[0])
        };
        let (track, on_bus, master) = (
            level(MeterOf::Track(a)),
            level(MeterOf::Strip(bv_timeline::Strip::Bus(bus))),
            level(MeterOf::Master),
        );
        assert!(track > 0.05, "{track}");
        assert!((on_bus / track - 1.0).abs() < 1e-3, "{on_bus} {track}");
        assert!((master / track - 1.0).abs() < 1e-3, "{master} {track}");
        // Off, none.
        mix(&p, 0, 4800, &mut pool);
        assert!(pool.take_vu_levels().is_empty());
    }

    /// A 5.1 master's channel faders trim each channel alone.
    #[test]
    fn surround_master_channels_are_trimmed_apart() {
        let mut p = Project::new(ProjectSettings::default());
        p.settings.channels = ChannelLayout::Surround51;
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, -12.0);
        p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
        let peaks = |p: &Project| -> Vec<f32> {
            let b = mix(p, 4800, 4800, &mut AudioPool::new());
            b.channels
                .iter()
                .map(|c| c.iter().fold(0f32, |m, s| m.max(s.abs())))
                .collect()
        };
        let before = peaks(&p);
        p.master.channel_db[0] = -6.0206;
        p.master.channel_db[4] = -96.0;
        let after = peaks(&p);
        assert!((after[0] / before[0] - 0.5).abs() < 1e-3);
        assert!(after[4] < 1e-6);
        assert_eq!(after[1], before[1]);
    }

    #[test]
    fn tracks_mixed_apart_sound_as_the_whole_mix() {
        let mut p = Project::new(ProjectSettings::default());
        let a = p.tracks[1].id;
        let b = p.add_track(TrackKind::Audio);
        let t1 = tone(&mut p, 440.0, -12.0);
        let t2 = tone(&mut p, 660.0, -6.0);
        let c = p.insert_media(t1, Tick::ZERO, None, Some(a)).unwrap()[0];
        p.insert_media(t2, Tick::from_seconds(1), None, Some(b))
            .unwrap();
        p.clip_mut(c).unwrap().fade_out = Tick::from_seconds(1);
        p.track_by_id_mut(b).unwrap().pan = 0.4;
        p.master.gain_db = -3.0;
        let rate = p.settings.sample_rate;
        let start = rate.sample_at(Tick::from_seconds_f64(0.9));
        let whole = mix(&p, start, 9600, &mut AudioPool::new());
        let mut pools: Vec<(TrackId, AudioPool)> = p
            .audible_tracks()
            .map(|t| (t.id, AudioPool::new()))
            .collect();
        let mut sums = Sums::silent(&p, 9600);
        for (id, pool) in &mut pools {
            let delays = delays(&p, &mut AudioPool::new());
            if let Some((own, _)) = mix_track(&p, *id, start, 9600, &delays, pool) {
                sums.add(&own);
            }
        }
        let sum = finish_mix(&p, start, &mut AudioPool::new(), sums);
        assert_eq!(sum.channels, whole.channels);
        assert!(whole.channels[0].iter().any(|s| s.abs() > 0.1));
    }

    #[test]
    fn clipe_comeca_na_amostra_exata() {
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, 0.0);
        // Começa em 10 amostras de 48 kHz.
        let start = SampleRate::HZ_48000.sample_start(10);
        p.insert_media(m, start, None, Some(a)).unwrap();
        let mut pool = AudioPool::new();
        let buf = mix(&p, 0, 64, &mut pool);
        assert!(buf.channels[0][..10].iter().all(|&s| s == 0.0));
        // A amostra 10 do projeto é a amostra 0 do tom: sen(0) = 0; a 11 não.
        assert_eq!(buf.channels[0][10], 0.0);
        assert!(buf.channels[0][11] > 0.1);
        // Lido em dois blocos dá o mesmo resultado.
        let a1 = mix(&p, 0, 30, &mut pool);
        let a2 = mix(&p, 30, 34, &mut pool);
        let joined: Vec<f32> = a1.channels[1]
            .iter()
            .chain(&a2.channels[1])
            .copied()
            .collect();
        assert_eq!(joined, buf.channels[1]);
    }

    #[test]
    fn reversed_events_play_backward() {
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let a = p.tracks[1].id;
        let m = tone(&mut p, 440.0, -6.0);
        let c = p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap()[0];
        p.trim_end(c, Tick::from_seconds(1)).unwrap();
        let n = 48_000;
        let plain = mix(&p, 0, n, &mut AudioPool::new());
        p.reverse_event(c).unwrap();
        let back = mix(&p, 0, n, &mut AudioPool::new());
        for i in (1..n).step_by(997) {
            let (b, f) = (back.channels[0][i], plain.channels[0][n - i]);
            assert!((b - f).abs() < 1e-4, "{i}: {b} for {f}");
        }
    }

    #[test]
    fn switches_invert_route_and_normalize() {
        let mut p = Project::new(ProjectSettings::default());
        p.quick_fades = false;
        let a = p.tracks[1].id;
        let m = tone(&mut p, 440.0, -6.0);
        let c = p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap()[0];
        let mut pool = AudioPool::new();
        let plain = mix(&p, 0, 128, &mut pool);
        let mut q = p.clone();
        {
            let clip = q.clip_mut(c).unwrap();
            clip.invert_phase = true;
            clip.normalize = Some(2.0);
            clip.channels = ChannelMap::LeftOnly;
        }
        let mut pool = AudioPool::new();
        let switched = mix(&q, 0, 128, &mut pool);
        for i in 0..128 {
            let expected = -2.0 * plain.channels[0][i];
            assert!((switched.channels[0][i] - expected).abs() < 1e-5);
            assert!((switched.channels[1][i] - expected).abs() < 1e-5);
        }
    }

    #[test]
    fn envelopes_and_quick_fades_shape_the_track() {
        use bv_timeline::EnvelopeKind;
        let mut p = Project::new(ProjectSettings::default());
        let a = p.tracks[1].id;
        let m = tone(&mut p, 440.0, -6.0);
        p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
        let mut pool = AudioPool::new();
        let plain = {
            let mut q = p.clone();
            q.quick_fades = false;
            mix(&q, 0, 4800, &mut pool)
        };
        // Quick fade: silent at the very first sample, half way at 5 ms.
        let faded = mix(&p, 0, 4800, &mut pool);
        assert_eq!(faded.channels[0][0], 0.0);
        let k = faded.channels[0][240] / plain.channels[0][240];
        assert!((k - 0.5).abs() < 0.01, "{k}");
        assert_eq!(faded.channels[0][1000], plain.channels[0][1000]);
        // A mute envelope switched off at 50 ms silences the rest.
        p.quick_fades = false;
        p.toggle_envelope(a, EnvelopeKind::Mute).unwrap();
        let off = SampleRate::HZ_48000.sample_start(2400);
        p.set_envelope_point(a, EnvelopeKind::Mute, off, 0.0)
            .unwrap();
        let muted = mix(&p, 0, 4800, &mut pool);
        assert_eq!(muted.channels[0][1000], plain.channels[0][1000]);
        assert!(muted.channels[0][2400..].iter().all(|&s| s == 0.0));
        // Volume at 0.5 halves the signal; pan hard right empties the left.
        p.toggle_envelope(a, EnvelopeKind::Mute).unwrap();
        p.toggle_envelope(a, EnvelopeKind::Volume).unwrap();
        p.set_envelope_point(a, EnvelopeKind::Volume, Tick::ZERO, 0.5)
            .unwrap();
        p.toggle_envelope(a, EnvelopeKind::Pan).unwrap();
        p.set_envelope_point(a, EnvelopeKind::Pan, Tick::ZERO, 1.0)
            .unwrap();
        let shaped = mix(&p, 0, 4800, &mut pool);
        assert!(shaped.channels[0].iter().all(|&s| s.abs() < 1e-6));
        let r = shaped.channels[1][1000] / plain.channels[1][1000];
        assert!((r - 0.5).abs() < 1e-4, "{r}");
    }

    #[test]
    fn trilhas_somam_e_mute_silencia() {
        let mut p = Project::new(ProjectSettings::default());
        let a1 = p.tracks[1].id;
        let a2 = p.add_track(TrackKind::Audio);
        let m = tone(&mut p, 440.0, -6.0);
        p.insert_media(m, Tick::ZERO, None, Some(a1)).unwrap();
        p.insert_media(m, Tick::ZERO, None, Some(a2)).unwrap();
        let mut pool = AudioPool::new();
        let one = {
            let mut q = p.clone();
            q.set_track_muted(a2, true).unwrap();
            mix(&q, 0, 256, &mut pool)
        };
        let both = mix(&p, 0, 256, &mut pool);
        for i in 0..256 {
            assert!((both.channels[0][i] - 2.0 * one.channels[0][i]).abs() < 1e-6);
        }
    }

    #[test]
    fn crossfade_de_potencia_constante() {
        let mut p = Project::new(ProjectSettings::default());
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, 0.0);
        let c1 = p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap()[0];
        let c2 = p
            .insert_media(m, Tick::from_seconds(3), None, Some(a))
            .unwrap()[0];
        let track = p.track(a).unwrap();
        let s = Tick::from_seconds;
        assert_eq!(
            track.effective_fades(p.clip(c1).unwrap(), true),
            (Tick::ZERO, s(2))
        );
        assert_eq!(
            track.effective_fades(p.clip(c2).unwrap(), true),
            (s(2), Tick::ZERO)
        );
        // No meio do crossfade, os ganhos são sen(45°) e cos(45°).
        let c = p.clip(c2).unwrap();
        let g_in = fade_gain(c, s(2), Tick::ZERO, s(4));
        let c = p.clip(c1).unwrap();
        let g_out = fade_gain(c, Tick::ZERO, s(2), s(4));
        assert!((g_in * g_in + g_out * g_out - 1.0).abs() < 1e-5);
    }

    #[test]
    fn own_fades_give_way_to_a_crossfade() {
        let mut p = Project::new(ProjectSettings::default());
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, 0.0);
        let s = Tick::from_seconds;
        let c1 = p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap()[0];
        let c2 = p.insert_media(m, s(3), None, Some(a)).unwrap()[0];
        // Fades longer than the overlap (2 s) at the overlapped edges, and
        // one at a free edge.
        p.clip_mut(c1).unwrap().fade_in = s(1);
        p.clip_mut(c1).unwrap().fade_out = s(4);
        p.clip_mut(c2).unwrap().fade_in = s(4);
        let track = p.track(a).unwrap();
        assert_eq!(
            track.effective_fades(p.clip(c1).unwrap(), true),
            (s(1), s(2)),
            "the crossfade, not the event's own fade-out"
        );
        assert_eq!(
            track.effective_fades(p.clip(c2).unwrap(), true),
            (s(2), Tick::ZERO)
        );
        // Without automatic crossfades the events keep their own.
        assert_eq!(
            track.effective_fades(p.clip(c1).unwrap(), false),
            (s(1), s(4))
        );
    }

    #[test]
    fn pan_types_and_their_envelope() {
        let mut p = Project::new(ProjectSettings::default());
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, 0.0);
        p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
        let peak = |p: &Project, c: usize| {
            mix(p, 0, 256, &mut AudioPool::new()).channels[c]
                .iter()
                .fold(0.0f32, |m, s| m.max(s.abs()))
        };
        let center = peak(&p, 0);
        // Constant power: 3 dB down at the center.
        p.track_by_id_mut(a).unwrap().pan_mode = bv_timeline::PanMode::ConstantPower;
        assert!((peak(&p, 0) / center - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3);
        // Add Channels fully left: both channels on the left.
        p.track_by_id_mut(a).unwrap().pan_mode = bv_timeline::PanMode::AddChannels;
        p.track_by_id_mut(a).unwrap().pan = -1.0;
        assert!((peak(&p, 0) / center - 2.0).abs() < 1e-3);
        assert_eq!(peak(&p, 1), 0.0);
        // The pan envelope follows the pan type too.
        p.track_by_id_mut(a).unwrap().pan = 0.0;
        p.track_by_id_mut(a).unwrap().pan_mode = bv_timeline::PanMode::Balance6;
        p.toggle_envelope(a, bv_timeline::EnvelopeKind::Pan)
            .unwrap();
        assert!((peak(&p, 1) / center - 0.501_187).abs() < 1e-3);
    }

    #[test]
    fn downmix_folds_surround_and_stereo() {
        use bv_core::ChannelLayout;
        let mut five: Vec<Vec<f32>> = (0..6).map(|c| vec![c as f32 * 0.1; 4]).collect();
        downmix(&mut five, ChannelLayout::Surround51, Downmix::Stereo);
        let k = std::f32::consts::FRAC_1_SQRT_2;
        assert!((five[0][0] - (0.0 + k * (0.2 + 0.4))).abs() < 1e-6);
        assert!((five[1][0] - (0.1 + k * (0.2 + 0.5))).abs() < 1e-6);
        assert!(five[2..].iter().all(|c| c.iter().all(|&s| s == 0.0)));
        let mut two = vec![vec![1.0f32; 3], vec![0.0f32; 3]];
        downmix(&mut two, ChannelLayout::Stereo, Downmix::Mono);
        assert_eq!(two, vec![vec![0.5; 3], vec![0.5; 3]]);
        let mut same = vec![vec![1.0f32; 3], vec![0.0f32; 3]];
        downmix(&mut same, ChannelLayout::Stereo, Downmix::Off);
        assert_eq!(same[0][0], 1.0);
    }

    #[test]
    fn pan_e_mestre() {
        let mut p = Project::new(ProjectSettings::default());
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, 0.0);
        p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
        p.track_by_id_mut(a).unwrap().pan = 1.0;
        let buf = mix(&p, 0, 64, &mut AudioPool::new());
        assert!(buf.channels[0].iter().all(|&s| s == 0.0));
        assert!(buf.channels[1].iter().any(|&s| s != 0.0));
        p.master.gain_db = -6.0206;
        let half = mix(&p, 0, 64, &mut AudioPool::new());
        for (x, y) in half.channels[1].iter().zip(&buf.channels[1]) {
            assert!((x - y * 0.5).abs() < 1e-4);
        }
        p.master.muted = true;
        assert!(
            mix(&p, 0, 64, &mut AudioPool::new()).channels[1]
                .iter()
                .all(|&s| s == 0.0)
        );
    }

    #[test]
    fn fade_in_linear() {
        let mut p = Project::new(ProjectSettings::default());
        let a = p.tracks[1].id;
        let m = tone(&mut p, 12000.0, 0.0);
        let c = p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap()[0];
        p.set_fades(c, Some(Tick::from_seconds(1)), None).unwrap();
        p.clip_mut(c).unwrap().fade_in_curve = bv_timeline::FadeCurve::Linear;
        let mut pool = AudioPool::new();
        let early = mix(&p, 4800, 480, &mut pool);
        let late = mix(&p, 48000 * 2, 480, &mut pool);
        let pk = |b: &AudioBuffer| b.channels[0].iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(
            pk(&early) < 0.12 && pk(&late) > 0.95,
            "{} {}",
            pk(&early),
            pk(&late)
        );
    }

    #[test]
    fn velocidade_dobra_a_frequencia() {
        let mut p = Project::new(ProjectSettings::default());
        let a = p.tracks[1].id;
        let m = tone(&mut p, 500.0, 0.0);
        let c = p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap()[0];
        p.set_clip_rate(c, 2.0).unwrap();
        let buf = mix(&p, 0, 48_000, &mut AudioPool::new());
        let ch = &buf.channels[0];
        let crossings = ch.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        assert!((999..=1001).contains(&crossings), "{crossings}");
    }

    #[test]
    fn efeito_do_clipe_e_da_trilha() {
        let mut p = Project::new(ProjectSettings::default());
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, -12.0);
        let c = p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap()[0];
        let base = mix(&p, 0, 4800, &mut AudioPool::new());
        let mut gain = bv_timeline::Effect::new(bv_timeline::EffectKind::Gain);
        gain.set_param(0, 6.0206);
        p.clip_mut(c).unwrap().effects.push(gain.clone());
        let clip_fx = mix(&p, 0, 4800, &mut AudioPool::new());
        p.track_by_id_mut(a).unwrap().effects.push(gain);
        let both = mix(&p, 0, 4800, &mut AudioPool::new());
        for i in 0..4800 {
            assert!((clip_fx.channels[0][i] - 2.0 * base.channels[0][i]).abs() < 1e-4);
            assert!((both.channels[0][i] - 4.0 * base.channels[0][i]).abs() < 1e-4);
        }
    }

    /// An event with Pitch Shift — an effect that changes the sound's
    /// length — plays its sound made through its chain: a 500 Hz tone an
    /// octave up comes out at 1 kHz, the event as long as it was, silent
    /// once the shortened sound ends.
    #[test]
    fn length_changing_effects_play_from_the_events_made_sound() {
        use bv_timeline::audio_fx::pitch_shift as ps;
        let mut p = Project::new(ProjectSettings::default());
        let a = p.tracks[1].id;
        let m = tone(&mut p, 500.0, -6.0);
        p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
        let id = p.tracks[1].clips[0].id;
        let length = p.tracks[1].clips[0].duration;
        let mut e = Effect::new(bv_timeline::EffectKind::PitchShift);
        e.set_param(ps::SEMITONES, 12.0);
        p.clip_mut(id).unwrap().effects.push(e);
        let mut pool = AudioPool::new();
        let buf = mix(&p, 0, 48_000, &mut pool);
        let ch = &buf.channels[0];
        let crossings = ch.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        assert!((995..=1005).contains(&crossings), "{crossings}");
        // Past half its length the shortened sound has ended.
        let late = first_sample_at_or_after(SampleRate::HZ_48000, length) * 3 / 4;
        let quiet = mix(&p, late, 4800, &mut pool);
        let peak = quiet.channels[0].iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(peak < 1e-4, "{peak}");
    }

    #[test]
    fn reamostragem_mantem_a_frequencia() {
        for quality in AudioQuality::ALL {
            let mut p = Project::new(ProjectSettings::default());
            p.settings.resample_quality = quality;
            let a = p.tracks[1].id;
            let mut info = MediaInfo::generated(
                Generator::TestTone {
                    frequency_hz: 1000.0,
                    level_db: 0.0,
                },
                &p.settings,
            );
            info.audio.as_mut().unwrap().sample_rate = SampleRate::HZ_44100;
            let m = p.add_media(info);
            p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
            let buf = mix(&p, 0, 48_000, &mut AudioPool::new());
            // Conta cruzamentos ascendentes por zero: 1000 em 1 s.
            let ch = &buf.channels[0];
            let crossings = ch.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
            assert!(
                (999..=1001).contains(&crossings),
                "{quality:?}: {crossings}"
            );
            // The level stays (no gain from the interpolation).
            let peak = ch[1000..47_000].iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!((peak - 1.0).abs() < 0.02, "{quality:?}: peak {peak}");
        }
    }

    #[test]
    fn better_resampling_is_closer_to_the_tone() {
        // A 5 kHz tone at 44.1 kHz into a 48 kHz project: the error against
        // the ideal tone shrinks from Preview to Good to Best.
        let error = |quality| {
            let mut p = Project::new(ProjectSettings::default());
            p.settings.resample_quality = quality;
            let a = p.tracks[1].id;
            let mut info = MediaInfo::generated(
                Generator::TestTone {
                    frequency_hz: 5000.0,
                    level_db: 0.0,
                },
                &p.settings,
            );
            info.audio.as_mut().unwrap().sample_rate = SampleRate::HZ_44100;
            let m = p.add_media(info);
            p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
            let buf = mix(&p, 0, 4800, &mut AudioPool::new());
            let ch = &buf.channels[0];
            // Best fit of a 5 kHz sine (amplitude and phase) by projection.
            let w = std::f64::consts::TAU * 5000.0 / 48_000.0;
            // After the quick fade-in at the clip start.
            let range = 1000..4700;
            let (mut sc, mut cc) = (0.0f64, 0.0f64);
            for i in range.clone() {
                sc += ch[i] as f64 * (w * i as f64).sin();
                cc += ch[i] as f64 * (w * i as f64).cos();
            }
            let k = 2.0 / range.len() as f64;
            let (sc, cc) = (sc * k, cc * k);
            range
                .map(|i| {
                    let ideal = sc * (w * i as f64).sin() + cc * (w * i as f64).cos();
                    (ch[i] as f64 - ideal).powi(2)
                })
                .sum::<f64>()
        };
        let (preview, good, best) = (
            error(AudioQuality::Preview),
            error(AudioQuality::Good),
            error(AudioQuality::Best),
        );
        assert!(good < preview && best < good, "{preview} {good} {best}");
    }

    #[test]
    fn looped_audio_repeats_the_media() {
        // A tone media cut to 0.5 s and looped over 2 s plays the same sample
        // at 0.1 s and at 0.6 s. At 441 Hz the tone itself would be half a
        // cycle off there (220.5 cycles in 0.5 s): only the loop matches.
        let mut p = Project::new(ProjectSettings::default());
        let a = p.tracks[1].id;
        let mut info = MediaInfo::generated(
            Generator::TestTone {
                frequency_hz: 441.0,
                level_db: 0.0,
            },
            &p.settings,
        );
        info.duration = Tick::from_seconds_f64(0.5);
        let m = p.add_media(info);
        let c = p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap()[0];
        p.trim_end(c, Tick::from_seconds(2)).unwrap();
        p.quick_fades = false;
        let buf = mix(&p, 0, 96_000, &mut AudioPool::new());
        let ch = &buf.channels[0];
        // 0.5 s = 24,000 samples at 48 kHz.
        for i in [4_800usize, 10_000, 20_000] {
            assert!((ch[i] - ch[i + 24_000]).abs() < 1e-4, "sample {i}");
            assert!((ch[i] - ch[i + 48_000]).abs() < 1e-4, "sample {i}");
        }
        let segs = loop_segments(
            p.clip(c).unwrap(),
            p.settings.sample_rate,
            0,
            96_000,
            Tick::from_seconds_f64(0.5),
        );
        assert_eq!(segs.len(), 4);
        assert_eq!(segs[1].0, 24_000);
    }

    #[test]
    fn track_phase_and_track_peaks() {
        let mut p = Project::new(ProjectSettings::default());
        let a = p.tracks[1].id;
        let tone = p.add_media(MediaInfo::generated(
            Generator::TestTone {
                frequency_hz: 1000.0,
                level_db: -6.0,
            },
            &p.settings,
        ));
        p.insert_media(tone, Tick::ZERO, None, Some(a)).unwrap();
        let mut pool = AudioPool::new();
        let base = mix(&p, 4800, 960, &mut pool);
        let peaks = pool.take_track_peaks();
        assert_eq!(peaks.len(), 1);
        assert_eq!(peaks[0].0, a);
        let half = 10f32.powf(-6.0 / 20.0);
        assert!((peaks[0].1[0] - half).abs() < 0.02, "{:?}", peaks[0].1);
        p.track_by_id_mut(a).unwrap().invert_phase = true;
        let inverted = mix(&p, 4800, 960, &mut AudioPool::new());
        for i in 0..960 {
            assert!((inverted.channels[0][i] + base.channels[0][i]).abs() < 1e-5);
        }
    }

    #[test]
    fn lfe_low_pass_in_surround_projects() {
        use bv_core::ChannelLayout;
        // A 6-channel buffer with the same tone in L and LFE: the filter
        // leaves L alone and takes the tone out of the LFE when it is above
        // the cutoff.
        let level = |hz: f32, enabled: bool| {
            let mut p = Project::new(ProjectSettings::default());
            p.settings.channels = ChannelLayout::Surround51;
            p.settings.lfe_lowpass = enabled;
            let mut pool = AudioPool::new();
            let mut out = AudioBuffer::silent(p.settings.sample_rate, p.settings.channels, 9600);
            let tone: Vec<f32> = (0..9600)
                .map(|i| (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin())
                .collect();
            out.channels[0] = tone.clone();
            out.channels[3] = tone;
            pool.lfe_lowpass(&p, 0, 9600, &mut out);
            let peak = |c: &[f32]| c[4800..].iter().fold(0.0f32, |m, s| m.max(s.abs()));
            (peak(&out.channels[0]), peak(&out.channels[3]))
        };
        let (l, lfe) = level(1000.0, true);
        assert!(l > 0.99 && lfe < 0.01, "1 kHz: L {l}, LFE {lfe}");
        let (_, lfe) = level(40.0, true);
        assert!(lfe > 0.9, "40 Hz passes: {lfe}");
        let (_, lfe) = level(1000.0, false);
        assert!(lfe > 0.99, "filter off: {lfe}");
    }

    /// A pitch change (an octave up, a fifth down) moves the tone, the
    /// event's length and level as they were; mixed in blocks, the same.
    #[test]
    fn pitch_changes_move_the_tone() {
        for (semitones, want) in [(12.0f32, 1000usize), (-7.0, 334)] {
            let mut p = Project::new(ProjectSettings::default());
            let a = p.tracks[1].id;
            let m = tone(&mut p, 500.0, -6.0);
            let c = p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap()[0];
            p.clip_mut(c).unwrap().pitch = semitones;
            let whole = mix(&p, 0, 48_000, &mut AudioPool::new());
            let ch = &whole.channels[0];
            let crossings = ch.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
            assert!(crossings.abs_diff(want) <= 6, "{semitones}: {crossings}");
            let peak = ch[4800..43_200].iter().fold(0f32, |m, s| m.max(s.abs()));
            assert!((peak - 0.5).abs() < 0.08, "{semitones}: {peak}");
            let mut pool = AudioPool::new();
            let mut parts = Vec::new();
            for b in 0..100 {
                parts.extend_from_slice(&mix(&p, b * 480, 480, &mut pool).channels[0]);
            }
            let worst = parts
                .iter()
                .zip(ch)
                .map(|(x, y)| (x - y).abs())
                .fold(0f32, f32::max);
            assert!(worst < 1e-5, "{semitones}: {worst}");
        }
    }

    #[test]
    fn stretched_events_keep_their_pitch() {
        for rate in [2.0, 0.5] {
            let mut p = Project::new(ProjectSettings::default());
            let a = p.tracks[1].id;
            let m = tone(&mut p, 500.0, -6.0);
            let c = p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap()[0];
            p.clip_mut(c).unwrap().preserve_pitch = true;
            p.set_clip_rate(c, rate).unwrap();
            let whole = mix(&p, 0, 48_000, &mut AudioPool::new());
            let ch = &whole.channels[0];
            let crossings = ch.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
            assert!((495..=505).contains(&crossings), "{rate}: {crossings}");
            let peak = ch[4800..43_200].iter().fold(0f32, |m, s| m.max(s.abs()));
            assert!(
                (peak - 0.5).abs() < 0.05,
                "{rate}: the level stays ({peak})"
            );
            // Mixed in blocks, as playback does: the very same samples.
            let mut pool = AudioPool::new();
            let mut parts = Vec::new();
            for b in 0..100 {
                parts.extend_from_slice(&mix(&p, b * 480, 480, &mut pool).channels[0]);
            }
            let worst = parts
                .iter()
                .zip(ch)
                .map(|(x, y)| (x - y).abs())
                .fold(0f32, f32::max);
            assert!(worst < 1e-5, "{rate}: {worst}");
        }
    }

    #[test]
    fn quick_fades_as_long_as_asked() {
        let clip = Clip::new(ClipId(1), MediaId(1), Tick::ZERO, Tick::from_seconds(1));
        let ms = |x: i64| Tick(TICKS_PER_SECOND * x / 1000);
        let none = Tick::ZERO;
        // Half way up a 10 ms ramp; an eighth of a 40 ms one.
        let g = quick_fade_gain(&clip, none, none, ms(5), ms(10));
        assert!((g - 0.5).abs() < 1e-6, "{g}");
        let g = quick_fade_gain(&clip, none, none, ms(5), ms(40));
        assert!((g - 0.125).abs() < 1e-6, "{g}");
        // An edge with a fade of its own has none.
        assert_eq!(quick_fade_gain(&clip, ms(100), none, ms(5), ms(10)), 1.0);
    }

    #[test]
    fn automation_off_plays_the_held_values_not_the_envelopes() {
        let mut p = Project::new(ProjectSettings::default());
        let a = p.tracks[1].id;
        let m = tone(&mut p, 1000.0, 0.0);
        p.insert_media(m, Tick::ZERO, None, Some(a)).unwrap();
        let peak = |p: &Project| {
            let b = mix(p, 4800, 4800, &mut AudioPool::new());
            b.channels[0].iter().fold(0f32, |m, s| m.max(s.abs()))
        };
        let full = peak(&p);
        // A volume envelope at half.
        p.toggle_envelope(a, bv_timeline::EnvelopeKind::Volume)
            .unwrap();
        p.set_envelope_point(a, bv_timeline::EnvelopeKind::Volume, Tick::ZERO, 0.5)
            .unwrap();
        assert!((peak(&p) - full * 0.5).abs() < 1e-3);
        // Automation off: the envelope does not play; the held value does.
        let t = p.track_by_id_mut(a).unwrap();
        t.automation.mode = bv_timeline::AutomationMode::Off;
        t.automation.volume = 0.25;
        assert!((peak(&p) - full * 0.25).abs() < 1e-3);
    }
}
