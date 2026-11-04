//! Processamento de áudio (DSP) dos efeitos: filtros biquad (RBJ Audio EQ
//! Cookbook), dinâmica (compressor, limitador, gate), atraso e reverberação
//! (topologia Freeverb: 8 filtros pente + 4 passa-tudo por canal).
//!
//! Cada efeito guarda estado entre blocos; a mixagem recria o estado quando
//! a leitura pula (busca) para não "vazar" som de outro lugar.

use bv_timeline::{Effect, EffectKind, Tempo};

fn db(x: f32) -> f32 {
    10f32.powf(x / 20.0)
}

fn to_db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// Coeficientes de um biquad normalizado (a0 = 1).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Coeffs {
    pub(crate) b0: f32,
    pub(crate) b1: f32,
    pub(crate) b2: f32,
    pub(crate) a1: f32,
    pub(crate) a2: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Shape {
    LowShelf,
    HighShelf,
    Peak,
    HighPass,
    LowPass,
}

pub(crate) fn design(shape: Shape, rate: f32, freq: f32, q: f32, gain_db: f32) -> Coeffs {
    let f = freq.clamp(10.0, rate * 0.49);
    let w0 = std::f32::consts::TAU * f / rate;
    let (sin, cos) = w0.sin_cos();
    let a = 10f32.powf(gain_db / 40.0);
    let alpha = sin / (2.0 * q.max(0.05));
    let (b0, b1, b2, a0, a1, a2) = match shape {
        Shape::Peak => (
            1.0 + alpha * a,
            -2.0 * cos,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos,
            1.0 - alpha / a,
        ),
        Shape::LowShelf => {
            let k = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) - (a - 1.0) * cos + k),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                a * ((a + 1.0) - (a - 1.0) * cos - k),
                (a + 1.0) + (a - 1.0) * cos + k,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                (a + 1.0) + (a - 1.0) * cos - k,
            )
        }
        Shape::HighShelf => {
            let k = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) + (a - 1.0) * cos + k),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                a * ((a + 1.0) + (a - 1.0) * cos - k),
                (a + 1.0) - (a - 1.0) * cos + k,
                2.0 * ((a - 1.0) - (a + 1.0) * cos),
                (a + 1.0) - (a - 1.0) * cos - k,
            )
        }
        Shape::HighPass => (
            (1.0 + cos) / 2.0,
            -(1.0 + cos),
            (1.0 + cos) / 2.0,
            1.0 + alpha,
            -2.0 * cos,
            1.0 - alpha,
        ),
        Shape::LowPass => (
            (1.0 - cos) / 2.0,
            1.0 - cos,
            (1.0 - cos) / 2.0,
            1.0 + alpha,
            -2.0 * cos,
            1.0 - alpha,
        ),
    };
    Coeffs {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
}

/// Biquad em forma direta transposta II, um estado por canal.
#[derive(Clone, Debug)]
pub(crate) struct Biquad {
    c: Coeffs,
    z: Vec<[f32; 2]>,
}

impl Biquad {
    pub(crate) fn new(c: Coeffs, channels: usize) -> Biquad {
        Biquad {
            c,
            z: vec![[0.0; 2]; channels],
        }
    }

    pub(crate) fn process(&mut self, ch: usize, x: f32) -> f32 {
        let c = self.c;
        let z = &mut self.z[ch];
        let y = c.b0 * x + z[0];
        z[0] = c.b1 * x - c.a1 * y + z[1];
        z[1] = c.b2 * x - c.a2 * y;
        y
    }
}

/// Butterworth low-pass of order `2 × stages` built from biquads (12
/// dB/octave per stage), one channel. Used on the LFE channel of 5.1
/// projects (File › Properties › Audio).
#[derive(Clone, Debug)]
pub struct LowPass {
    stages: Vec<Biquad>,
}

impl LowPass {
    pub fn new(rate: f32, cutoff_hz: f32, stages: usize) -> LowPass {
        let n = stages.max(1);
        let stages = (1..=n)
            .map(|k| {
                // Q of each pole pair of a Butterworth filter of order 2n.
                let angle = std::f32::consts::PI * (2 * k - 1) as f32 / (4 * n) as f32;
                let q = 1.0 / (2.0 * angle.cos());
                Biquad::new(design(Shape::LowPass, rate, cutoff_hz, q, 0.0), 1)
            })
            .collect();
        LowPass { stages }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for x in samples {
            for st in &mut self.stages {
                *x = st.process(0, *x);
            }
        }
    }
}

/// Detector de envoltória com ataque/liberação (coeficientes por amostra).
#[derive(Clone, Debug)]
struct Envelope {
    attack: f32,
    release: f32,
    value: f32,
}

impl Envelope {
    fn new(rate: f32, attack_ms: f32, release_ms: f32) -> Envelope {
        let coef = |ms: f32| (-1.0 / (ms.max(0.01) * 0.001 * rate)).exp();
        Envelope {
            attack: coef(attack_ms),
            release: coef(release_ms),
            value: 0.0,
        }
    }

    fn follow(&mut self, x: f32) -> f32 {
        let c = if x > self.value {
            self.attack
        } else {
            self.release
        };
        self.value = x + c * (self.value - x);
        self.value
    }
}

#[derive(Clone, Debug)]
struct Comb {
    buf: Vec<f32>,
    pos: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Comb {
        Comb {
            buf: vec![0.0; len.max(1)],
            pos: 0,
            store: 0.0,
        }
    }

    fn process(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let y = self.buf[self.pos];
        self.store = y * (1.0 - damp) + self.store * damp;
        self.buf[self.pos] = x + self.store * feedback;
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
}

#[derive(Clone, Debug)]
struct Allpass {
    buf: Vec<f32>,
    pos: usize,
}

impl Allpass {
    fn new(len: usize) -> Allpass {
        Allpass {
            buf: vec![0.0; len.max(1)],
            pos: 0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let b = self.buf[self.pos];
        let y = -x + b;
        self.buf[self.pos] = x + b * 0.5;
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
}

#[derive(Debug)]
enum State {
    Gain(f32),
    Filters(Vec<Biquad>),
    Compressor {
        env: Envelope,
        threshold: f32,
        ratio: f32,
        makeup: f32,
    },
    Gate {
        env: Envelope,
        threshold: f32,
        gain: Envelope,
    },
    Delay {
        lines: Vec<Vec<f32>>,
        pos: usize,
        feedback: f32,
        mix: f32,
    },
    Reverb {
        combs: Vec<Vec<Comb>>,
        allpasses: Vec<Vec<Allpass>>,
        feedback: f32,
        damp: f32,
        mix: f32,
    },
    /// One of the effects that follow VEGAS's ([`crate::fx`]).
    Own(Box<dyn crate::fx::Own>),
    #[cfg(not(target_family = "wasm"))]
    Vst(Box<crate::vst::Run>),
}

/// What tells a VST 3 effect from another: its plug-in (its module and
/// its class).
type VstKey = (std::path::PathBuf, String);

fn vst_key(e: &Effect) -> Option<VstKey> {
    e.vst
        .as_deref()
        .map(|v| (v.path.clone(), v.class_id.clone()))
}

#[cfg(test)]
thread_local! {
    /// The tests' stand-in for a plug-in's latency: what a Gain effect
    /// reports on this thread.
    pub(crate) static FAKE_LATENCY: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Um efeito de áudio pronto para processar.
#[derive(Debug)]
pub struct Processor {
    kind: EffectKind,
    params: Vec<f32>,
    vst: Option<VstKey>,
    state: State,
}

impl Processor {
    pub fn new(e: &Effect, rate: u32, channels: usize) -> Processor {
        if let Some(own) = crate::fx::make(e, rate, channels) {
            return Processor {
                kind: e.kind,
                params: e.params.clone(),
                vst: None,
                state: State::Own(own),
            };
        }
        let r = rate as f32;
        let p = |i: usize| e.param(i);
        let state = match e.kind {
            EffectKind::Eq3 => State::Filters(vec![
                Biquad::new(design(Shape::LowShelf, r, p(1), 0.707, p(0)), channels),
                Biquad::new(design(Shape::Peak, r, p(3), p(4), p(2)), channels),
                Biquad::new(design(Shape::HighShelf, r, p(6), 0.707, p(5)), channels),
            ]),
            EffectKind::HighPass => State::Filters(vec![Biquad::new(
                design(Shape::HighPass, r, p(0), p(1), 0.0),
                channels,
            )]),
            EffectKind::LowPass => State::Filters(vec![Biquad::new(
                design(Shape::LowPass, r, p(0), p(1), 0.0),
                channels,
            )]),
            EffectKind::Compressor => State::Compressor {
                env: Envelope::new(r, p(2), p(3)),
                threshold: p(0),
                ratio: p(1).max(1.0),
                makeup: db(p(4)),
            },
            EffectKind::Limiter => State::Compressor {
                env: Envelope::new(r, 0.1, p(1)),
                threshold: p(0),
                ratio: 1000.0,
                makeup: 1.0,
            },
            EffectKind::NoiseGate => State::Gate {
                env: Envelope::new(r, 1.0, p(1)),
                threshold: db(p(0)),
                gain: Envelope::new(r, 1.0, p(1)),
            },
            EffectKind::Delay => {
                let len = ((p(0) * 0.001 * r) as usize).max(1);
                State::Delay {
                    lines: vec![vec![0.0; len]; channels],
                    pos: 0,
                    feedback: p(1),
                    mix: p(2),
                }
            }
            EffectKind::Reverb => {
                // Afinação original do Freeverb a 44,1 kHz, escalada para a taxa.
                const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
                const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
                let scale = r / 44_100.0;
                let spread = |c: usize| c * 23;
                let combs = (0..channels)
                    .map(|c| {
                        COMBS
                            .iter()
                            .map(|&l| Comb::new(((l + spread(c)) as f32 * scale) as usize))
                            .collect()
                    })
                    .collect();
                let allpasses = (0..channels)
                    .map(|c| {
                        ALLPASSES
                            .iter()
                            .map(|&l| Allpass::new(((l + spread(c)) as f32 * scale) as usize))
                            .collect()
                    })
                    .collect();
                State::Reverb {
                    combs,
                    allpasses,
                    feedback: 0.7 + p(0) * 0.28,
                    damp: p(1) * 0.4,
                    mix: p(2),
                }
            }
            // A plug-in that cannot load lets the sound through.
            #[cfg(not(target_family = "wasm"))]
            EffectKind::Vst => match e
                .vst
                .as_deref()
                .and_then(|v| crate::vst::Run::new(v, rate, channels))
            {
                Some(run) => State::Vst(Box::new(run)),
                None => State::Gain(1.0),
            },
            _ => State::Gain(db(if e.kind == EffectKind::Gain {
                p(0)
            } else {
                0.0
            })),
        };
        Processor {
            kind: e.kind,
            params: e.params.clone(),
            vst: vst_key(e),
            state,
        }
    }

    /// O mesmo efeito com os mesmos parâmetros (o estado pode ser
    /// mantido); a VST 3 plug-in's, the same plug-in (it follows its state
    /// and its parameters: [`Processor::follow`]).
    pub fn matches(&self, e: &Effect) -> bool {
        self.kind == e.kind && self.params == e.params && self.vst == vst_key(e)
    }

    /// The project's tempo, for the effects with tempo sync.
    pub fn keep_time(&mut self, tempo: &Tempo) {
        if let State::Own(own) = &mut self.state {
            own.keep_time(tempo);
        }
    }

    /// How many samples late its output comes: a VST 3 plug-in's latency,
    /// or that of one of BananaV's own effects working in blocks (FFTs).
    pub fn latency(&self) -> usize {
        match &self.state {
            #[cfg(not(target_family = "wasm"))]
            State::Vst(run) => run.latency(),
            State::Own(own) => own.latency(),
            #[cfg(test)]
            State::Gain(_) => FAKE_LATENCY.with(std::cell::Cell::get),
            _ => 0,
        }
    }

    /// The same effect, whatever its values (it can follow them:
    /// [`Processor::retune`]).
    pub fn same_effect(&self, e: &Effect) -> bool {
        self.kind == e.kind && self.vst == vst_key(e)
    }

    /// Takes `e`'s values while it runs, keeping what it holds — filters'
    /// memory, envelopes, delay lines and reverb tails — so that values
    /// that change as it plays (FX automation, a slider dragged) do not
    /// click.
    pub fn retune(&mut self, e: &Effect, rate: u32, channels: usize) {
        if let State::Own(own) = &mut self.state
            && self.kind == e.kind
        {
            own.set(&crate::fx::params_of(e));
            self.params = e.params.clone();
            return;
        }
        let fresh = Processor::new(e, rate, channels);
        match (&mut self.state, fresh.state) {
            (State::Gain(g), State::Gain(n)) => *g = n,
            (State::Filters(old), State::Filters(new)) if old.len() == new.len() => {
                for (o, n) in old.iter_mut().zip(new) {
                    o.c = n.c;
                }
            }
            (
                State::Compressor {
                    env,
                    threshold,
                    ratio,
                    makeup,
                },
                State::Compressor {
                    env: ne,
                    threshold: nt,
                    ratio: nr,
                    makeup: nm,
                },
            ) => {
                env.attack = ne.attack;
                env.release = ne.release;
                *threshold = nt;
                *ratio = nr;
                *makeup = nm;
            }
            (
                State::Gate {
                    env,
                    threshold,
                    gain,
                },
                State::Gate {
                    env: ne,
                    threshold: nt,
                    gain: ng,
                },
            ) => {
                env.attack = ne.attack;
                env.release = ne.release;
                gain.attack = ng.attack;
                gain.release = ng.release;
                *threshold = nt;
            }
            (
                State::Delay {
                    lines,
                    pos,
                    feedback,
                    mix,
                },
                State::Delay {
                    lines: nl,
                    feedback: nf,
                    mix: nm,
                    ..
                },
            ) => {
                let len = nl.first().map_or(1, Vec::len).max(1);
                for l in lines.iter_mut() {
                    l.resize(len, 0.0);
                }
                *pos %= len;
                *feedback = nf;
                *mix = nm;
            }
            (
                State::Reverb {
                    feedback,
                    damp,
                    mix,
                    ..
                },
                State::Reverb {
                    feedback: nf,
                    damp: nd,
                    mix: nm,
                    ..
                },
            ) => {
                *feedback = nf;
                *damp = nd;
                *mix = nm;
            }
            (state, new) => *state = new,
        }
        self.params = e.params.clone();
    }

    /// Takes what of `e` it can while it runs: a VST 3 plug-in's state
    /// and parameters.
    pub fn follow(&mut self, e: &Effect) {
        #[cfg(not(target_family = "wasm"))]
        if let (State::Vst(run), Some(v)) = (&mut self.state, e.vst.as_deref()) {
            run.follow(v);
        }
        #[cfg(target_family = "wasm")]
        let _ = e;
    }

    /// Processa os canais (planar) no lugar.
    pub fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        match &mut self.state {
            State::Own(own) => own.process(channels),
            State::Gain(g) => {
                for ch in channels.iter_mut() {
                    for s in ch.iter_mut() {
                        *s *= *g;
                    }
                }
            }
            State::Filters(fs) => {
                for f in fs.iter_mut() {
                    for (c, ch) in channels.iter_mut().enumerate().take(f.z.len()) {
                        for s in ch.iter_mut() {
                            *s = f.process(c, *s);
                        }
                    }
                }
            }
            State::Compressor {
                env,
                threshold,
                ratio,
                makeup,
            } => {
                // Detecção ligada entre os canais (estéreo não "anda").
                for i in 0..n {
                    let peak = channels.iter().fold(0f32, |m, ch| m.max(ch[i].abs()));
                    let level = to_db(env.follow(peak));
                    let over = level - *threshold;
                    let reduction = if over > 0.0 {
                        over - over / *ratio
                    } else {
                        0.0
                    };
                    let g = db(-reduction) * *makeup;
                    for ch in channels.iter_mut() {
                        ch[i] *= g;
                    }
                }
            }
            State::Gate {
                env,
                threshold,
                gain,
            } => {
                for i in 0..n {
                    let peak = channels.iter().fold(0f32, |m, ch| m.max(ch[i].abs()));
                    let open = if env.follow(peak) >= *threshold {
                        1.0
                    } else {
                        0.0
                    };
                    let g = gain.follow(open);
                    for ch in channels.iter_mut() {
                        ch[i] *= g;
                    }
                }
            }
            State::Delay {
                lines,
                pos,
                feedback,
                mix,
            } => {
                let len = lines.first().map_or(1, Vec::len);
                let start = *pos;
                for (ch, line) in channels.iter_mut().zip(lines.iter_mut()) {
                    let mut p = start;
                    for s in ch.iter_mut() {
                        let delayed = line[p];
                        line[p] = *s + delayed * *feedback;
                        *s += delayed * *mix;
                        p = (p + 1) % len;
                    }
                }
                *pos = (start + n) % len;
            }
            State::Reverb {
                combs,
                allpasses,
                feedback,
                damp,
                mix,
            } => {
                let input_gain = 0.015;
                for (c, ch) in channels.iter_mut().enumerate() {
                    let Some(cs) = combs.get_mut(c) else { continue };
                    let aps = &mut allpasses[c];
                    for s in ch.iter_mut() {
                        let x = *s * input_gain;
                        let mut wet = 0.0;
                        for comb in cs.iter_mut() {
                            wet += comb.process(x, *feedback, *damp);
                        }
                        for ap in aps.iter_mut() {
                            wet = ap.process(wet);
                        }
                        *s = *s * (1.0 - *mix) + wet * *mix * 3.0;
                    }
                }
            }
            #[cfg(not(target_family = "wasm"))]
            State::Vst(run) => run.process(channels),
        }
    }
}

/// Cadeia de efeitos com estado. Recria só os efeitos que mudaram.
#[derive(Debug, Default)]
pub struct Chain {
    /// The processors, and whether each is on (bypassed ones run only to
    /// keep their state).
    procs: Vec<(Processor, bool)>,
    /// Each processor's place among the effects it was matched to.
    slots: Vec<usize>,
    rate: u32,
    channels: usize,
    /// The project's tempo, for the effects that keep time with it.
    tempo: Tempo,
}

impl Chain {
    /// How many samples late the chain's output comes: its effects'
    /// latencies (those on) summed.
    pub fn latency(&self) -> usize {
        self.procs
            .iter()
            .filter(|(_, on)| *on)
            .map(|(p, _)| p.latency())
            .sum()
    }

    /// Matches the chain to the list of effects (the audio ones that are on,
    /// and the bypassed ones too with `keep_bypassed`: they go on running,
    /// unheard, so that turning them on again joins without a break).
    pub fn sync(&mut self, effects: &[&Effect], rate: u32, channels: usize, keep_bypassed: bool) {
        let wanted: Vec<&Effect> = effects
            .iter()
            .copied()
            .filter(|e| e.kind.is_audio() && (e.enabled || keep_bypassed))
            .collect();
        self.slots = effects
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind.is_audio() && (e.enabled || keep_bypassed))
            .map(|(i, _)| i)
            .collect();
        // Each effect in its place keeps its processor while it matches
        // (and follows what it can); the others are made again.
        let mut old: Vec<Option<Processor>> = if self.rate == rate && self.channels == channels {
            std::mem::take(&mut self.procs)
                .into_iter()
                .map(|(p, _)| Some(p))
                .collect()
        } else {
            Vec::new()
        };
        self.procs = wanted
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let p = match old.get_mut(i).and_then(Option::take) {
                    Some(mut p) if p.matches(e) => {
                        p.follow(e);
                        p
                    }
                    // Other values (FX automation, a slider dragged): the
                    // same processor follows them.
                    Some(mut p) if p.same_effect(e) => {
                        p.retune(e, rate, channels);
                        p.follow(e);
                        p
                    }
                    _ => {
                        let mut p = Processor::new(e, rate, channels);
                        p.keep_time(&self.tempo);
                        p
                    }
                };
                (p, e.enabled)
            })
            .collect();
        self.rate = rate;
        self.channels = channels;
    }

    pub fn is_empty(&self) -> bool {
        self.procs.is_empty()
    }

    /// The project's tempo (the effects with tempo sync follow it).
    pub fn set_tempo(&mut self, tempo: Tempo) {
        if tempo != self.tempo {
            self.tempo = tempo;
            for (p, _) in &mut self.procs {
                p.keep_time(&tempo);
            }
        }
    }

    /// Zera o estado (depois de uma busca).
    pub fn reset(&mut self) {
        self.procs.clear();
        self.rate = 0;
    }

    pub fn process(&mut self, channels: &mut [Vec<f32>]) {
        self.process_watching(channels, None);
    }

    /// [`Self::process`], what goes in and comes out of the effect at
    /// `watched` (its place among the effects) noted for the Modern
    /// Equalizer's analyzer and meters.
    pub fn process_watching(&mut self, channels: &mut [Vec<f32>], watched: Option<usize>) {
        let rate = self.rate;
        for (k, (p, on)) in self.procs.iter_mut().enumerate() {
            let input = (watched.is_some() && self.slots.get(k).copied() == watched)
                .then(|| channels.to_vec());
            if *on {
                p.process(channels);
            } else {
                // Bypassed, kept running: on a copy nobody hears.
                let mut unheard = channels.to_vec();
                p.process(&mut unheard);
            }
            if let Some(input) = input {
                crate::modern_eq::note(rate, &input, channels);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: f32, n: usize, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / rate).sin())
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    fn run(kind: EffectKind, set: &[(usize, f32)], input: Vec<f32>) -> Vec<f32> {
        let mut e = Effect::new(kind);
        for &(i, v) in set {
            e.set_param(i, v);
        }
        let mut p = Processor::new(&e, 48_000, 1);
        let mut ch = vec![input];
        p.process(&mut ch);
        ch.remove(0)
    }

    /// A VST 3 effect in a chain: its parameters followed while it runs
    /// (the instance kept), its state when made; a missing plug-in lets the
    /// sound through.
    #[test]
    fn a_vst_effect_runs_in_the_chain_and_follows_its_values() {
        use bv_vst3_testplug as plug;
        bv_vst3::linked("testplug", plug::GetPluginFactory).unwrap();
        let fx = bv_timeline::VstFx {
            path: "<linked>/testplug".into(),
            class_id: bv_vst3_sys::uid_to_hex(&plug::GAIN),
            name: "BananaV Test Gain".into(),
            params: [(plug::GAIN_ID, 0.25)].into_iter().collect(),
            ..Default::default()
        };
        let mut e = Effect::vst(fx);
        let x = sine(1000.0, 48_000.0, 4800, 0.5);
        let mut chain = Chain::default();
        let level = |chain: &mut Chain, e: &Effect| {
            chain.sync(&[e], 48_000, 2, false);
            let mut ch = vec![x.clone(), x.clone()];
            chain.process(&mut ch);
            rms(&ch[0]) / rms(&x)
        };
        assert!((level(&mut chain, &e) - 0.5).abs() < 1e-4);
        e.vst.as_mut().unwrap().params.insert(plug::GAIN_ID, 1.0);
        assert!((level(&mut chain, &e) - 2.0).abs() < 1e-4);
        // A state (the gain's: gain and bypass), taken while it runs.
        let mut state = 0.5f64.to_le_bytes().to_vec();
        state.extend_from_slice(&1.0f64.to_le_bytes());
        let v = e.vst.as_mut().unwrap();
        v.state = state;
        v.params.clear();
        assert!((level(&mut chain, &e) - 1.0).abs() < 1e-4);
        // A missing plug-in.
        e.vst.as_mut().unwrap().path = "C:/nowhere/Missing.vst3".into();
        assert!((level(&mut chain, &e) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_modern_equalizer_runs_in_the_chain() {
        use bv_timeline::fx::modern_eq::{FREQUENCY, GAIN, OUTPUT_GAIN, TYPE, USED, at};
        // A peak of +6 dB at 1 kHz and the output 6 dB down: a 1 kHz tone
        // comes out as it went in, a 100 Hz one half as loud.
        let set = [
            (at(0, USED), 1.0),
            (at(0, TYPE), 5.0),
            (at(0, FREQUENCY), 1000.0),
            (at(0, GAIN), 6.0),
            (OUTPUT_GAIN, -6.0),
        ];
        let level = |f: f32| {
            let out = run(EffectKind::ModernEq, &set, sine(f, 48_000.0, 24_000, 0.5));
            rms(&out[12_000..]) / rms(&sine(f, 48_000.0, 12_000, 0.5))
        };
        assert!((level(1000.0) - 1.0).abs() < 0.03, "{}", level(1000.0));
        assert!((level(100.0) - 0.5).abs() < 0.03, "{}", level(100.0));
    }

    #[test]
    fn a_bypassed_effect_kept_running_comes_back_where_it_was() {
        let mut e = Effect::new(EffectKind::Reverb);
        let input = sine(440.0, 48_000.0, 4800, 0.5);
        let silence = vec![0.0f32; 4800];
        // Runs through a burst, is bypassed through silence, then turned on:
        // kept running, its tail is still there; rebuilt, it starts silent.
        let mut tail = |keep: bool| {
            let mut chain = Chain::default();
            e.enabled = true;
            chain.sync(&[&e], 48_000, 1, keep);
            chain.process(&mut [input.clone()]);
            e.enabled = false;
            chain.sync(&[&e], 48_000, 1, keep);
            let mut quiet = vec![silence.clone()];
            chain.process(&mut quiet);
            assert_eq!(quiet[0], silence, "bypassed: not heard");
            e.enabled = true;
            chain.sync(&[&e], 48_000, 1, keep);
            let mut after = vec![vec![0.0f32; 480]];
            chain.process(&mut after);
            rms(&after[0])
        };
        assert!(tail(true) > 0.0, "the tail goes on");
        assert_eq!(tail(false), 0.0, "rebuilt from nothing");
    }

    #[test]
    fn ganho_de_6_db_dobra() {
        let out = run(EffectKind::Gain, &[(0, 6.0206)], vec![0.25; 10]);
        assert!((out[5] - 0.5).abs() < 1e-3);
    }

    #[test]
    fn passa_altas_corta_graves_e_mantem_agudos() {
        let low = run(
            EffectKind::HighPass,
            &[(0, 1000.0)],
            sine(50.0, 48_000.0, 48_000, 0.5),
        );
        let high = run(
            EffectKind::HighPass,
            &[(0, 1000.0)],
            sine(8000.0, 48_000.0, 48_000, 0.5),
        );
        assert!(rms(&low[24_000..]) < 0.01, "{}", rms(&low[24_000..]));
        assert!((rms(&high[24_000..]) - 0.3536).abs() < 0.02);
    }

    #[test]
    fn eq_de_medios_aumenta_so_os_medios() {
        let boost = [(2, 12.0), (3, 1000.0)];
        let mid = run(EffectKind::Eq3, &boost, sine(1000.0, 48_000.0, 48_000, 0.1));
        let low = run(EffectKind::Eq3, &boost, sine(60.0, 48_000.0, 48_000, 0.1));
        let gain_mid = rms(&mid[24_000..]) / 0.0707;
        let gain_low = rms(&low[24_000..]) / 0.0707;
        assert!((to_db(gain_mid) - 12.0).abs() < 0.5, "{}", to_db(gain_mid));
        assert!(to_db(gain_low).abs() < 1.0, "{}", to_db(gain_low));
    }

    #[test]
    fn compressor_reduz_acima_do_limiar() {
        // Senoide a −6 dBFS, limiar −20, razão 4: saída ≈ −20 + 14/4 = −16,5 dBFS de pico.
        let out = run(
            EffectKind::Compressor,
            &[(0, -20.0), (1, 4.0), (2, 1.0), (3, 50.0)],
            sine(1000.0, 48_000.0, 48_000, 0.5),
        );
        let peak = out[24_000..].iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!((to_db(peak) + 16.5).abs() < 1.5, "{}", to_db(peak));
    }

    #[test]
    fn limitador_segura_o_teto() {
        let out = run(
            EffectKind::Limiter,
            &[(0, -6.0)],
            sine(440.0, 48_000.0, 48_000, 1.0),
        );
        let peak = out[4_800..].iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!(to_db(peak) < -5.0, "{}", to_db(peak));
    }

    #[test]
    fn gate_fecha_no_silencio() {
        let mut input = sine(440.0, 48_000.0, 24_000, 0.001);
        input.extend(sine(440.0, 48_000.0, 24_000, 0.5));
        let out = run(EffectKind::NoiseGate, &[(0, -40.0), (1, 20.0)], input);
        assert!(rms(&out[12_000..24_000]) < 1e-4);
        assert!(rms(&out[36_000..]) > 0.3);
    }

    #[test]
    fn atraso_repete_o_impulso() {
        let mut input = vec![0.0f32; 48_000];
        input[0] = 1.0;
        let out = run(EffectKind::Delay, &[(0, 100.0), (1, 0.5), (2, 1.0)], input);
        assert!((out[4_800] - 1.0).abs() < 1e-5, "{}", out[4_800]);
        assert!((out[9_600] - 0.5).abs() < 1e-5, "{}", out[9_600]);
    }

    #[test]
    fn reverb_cria_cauda() {
        let mut input = vec![0.0f32; 48_000];
        input[0] = 1.0;
        let out = run(EffectKind::Reverb, &[(2, 1.0)], input);
        assert!(
            rms(&out[4_800..24_000]) > 1e-4,
            "{}",
            rms(&out[4_800..24_000])
        );
    }

    #[test]
    fn cadeia_mantem_estado_entre_blocos() {
        let e = Effect::new(EffectKind::Delay);
        let mut whole = Chain::default();
        whole.sync(&[&e], 48_000, 1, false);
        let mut input = vec![0.0f32; 48_000];
        input[10] = 1.0;
        let mut a = vec![input.clone()];
        whole.process(&mut a);
        let mut parts = Chain::default();
        parts.sync(&[&e], 48_000, 1, false);
        let mut out = Vec::new();
        for chunk in input.chunks(1000) {
            let mut c = vec![chunk.to_vec()];
            parts.sync(&[&e], 48_000, 1, false);
            parts.process(&mut c);
            out.extend_from_slice(&c[0]);
        }
        assert_eq!(out, a[0]);
    }
}
