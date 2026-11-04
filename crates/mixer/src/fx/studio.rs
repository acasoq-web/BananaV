//! BananaV's versions of the studio set (Gate, DeEsser, Limiter, Tremolo
//! Pan, Tube Stage, Vocal Strip, …), built from what their controls do.
//! Every one takes the sound at its input gain and gives it back at its
//! output gain.

use bv_timeline::audio_fx::{
    efx_deesser as ed, efx_gate as eg, efx_limiter as el, efx_tremolo_pan as et,
    efx_tube_stage as tu, efx_vocal_strip as ev,
};

use super::Own;
use super::eq::Section;

/// Added and taken away again on fed-back state, so a fading signal ends
/// at 0 instead of lingering as a denormal number.
const FLUSH: f32 = 1e-18;

/// A one-pole follower's coefficient for a time in milliseconds.
fn follow(ms: f32, fs: f32) -> f32 {
    let n = ms / 1000.0 * fs;
    if n < 1.0 { 0.0 } else { (-1.0 / n).exp() }
}

fn db(v: f32) -> f32 {
    10f32.powf(v / 20.0)
}

/// Gate: the level (of the side chain: the sound itself, filtered when
/// asked) followed up by the attack and down by the release; below the
/// threshold the sound comes down by the range (with the soft knee over
/// 12 dB), held open a while after the level falls, the gain itself
/// gliding through two more followers; the sound a little late, so the
/// gate is open when a note starts.
#[derive(Debug)]
pub(crate) struct Gate {
    rate: f32,
    params: Vec<f32>,
    side: Option<Section>,
    high_pass: Option<Section>,
    level: f32,
    held: usize,
    gains: [f32; 2],
    /// Each channel's filters' memories: side chain, input high-pass.
    memory: Vec<[[f32; 4]; 2]>,
    lines: Vec<Vec<f32>>,
    at: usize,
}

impl Gate {
    pub(crate) fn new(rate: u32) -> Gate {
        Gate {
            rate: rate.max(1) as f32,
            params: Vec::new(),
            side: None,
            high_pass: None,
            level: 0.0,
            held: 0,
            gains: [1.0; 2],
            memory: Vec::new(),
            lines: Vec::new(),
            at: 0,
        }
    }

    fn p(&self, i: usize) -> f32 {
        let s = &eg::PARAMS[i];
        self.params
            .get(i)
            .copied()
            .unwrap_or(s.default)
            .clamp(s.min, s.max)
    }

    fn ahead(&self) -> usize {
        (self.p(eg::LOOK_AHEAD) / 1000.0 * self.rate).round() as usize
    }
}

impl Own for Gate {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
        let fs = self.rate as f64;
        let hz = self.p(eg::FILTER) as f64;
        self.side = match self.p(eg::FILTER_TYPE).round() as i32 {
            1 => Some(Section::pass(hz, true, fs)),
            2 => Some(Section::band(hz, 1.0, fs)),
            3 => Some(Section::pass(hz, false, fs)),
            _ => None,
        };
        let hp = self.p(eg::INPUT_HIGH_PASS) as f64;
        self.high_pass = (hp >= 1.0).then(|| Section::pass(hp, true, fs));
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let count = channels.len();
        let fs = self.rate;
        let input = db(self.p(eg::INPUT));
        let output = db(self.p(eg::OUTPUT));
        let threshold = self.p(eg::THRESHOLD);
        let range = self.p(eg::RANGE);
        let floor = if range >= 90.0 { 0.0 } else { db(-range) };
        let soft = self.p(eg::SOFT_KNEE) >= 0.5;
        let up = follow(self.p(eg::ATTACK), fs);
        let down = follow(self.p(eg::RELEASE), fs);
        let hold = ([10.0, 50.0, 150.0][self.p(eg::HOLD).round().clamp(0.0, 2.0) as usize] / 1000.0
            * fs) as usize;
        let listen = self.p(eg::LISTEN) >= 0.5;
        let ahead = self.ahead();
        let room = ahead + 1;
        if self.lines.len() != count || self.lines.first().map_or(0, Vec::len) != room {
            self.lines = vec![vec![0.0; room]; count];
            self.memory = vec![[[0.0; 4]; 2]; count];
            self.at = 0;
        }
        for i in 0..n {
            let mut peak = 0f32;
            for (c, ch) in channels.iter_mut().enumerate() {
                let mut x = ch[i] * input;
                if let Some(hp) = &self.high_pass {
                    x = hp.run(x, &mut self.memory[c][1]);
                }
                ch[i] = x;
                let s = match &self.side {
                    Some(f) => f.run(x, &mut self.memory[c][0]),
                    None => x,
                };
                if listen {
                    ch[i] = s;
                }
                peak = peak.max(s.abs());
            }
            let c = if peak > self.level { up } else { down };
            self.level = (peak + (self.level - peak) * c + FLUSH) - FLUSH;
            let l = 20.0 * self.level.max(1e-9).log10();
            let mut target = if l >= threshold {
                self.held = hold;
                1.0
            } else if self.held > 0 {
                self.held -= 1;
                1.0
            } else if soft && l > threshold - 12.0 {
                db(-range * (threshold - l) / 12.0).max(floor)
            } else {
                floor
            };
            if listen {
                target = 1.0;
            }
            for g in self.gains.iter_mut() {
                let c = if target > *g { up } else { down };
                *g = target + (*g - target) * c;
                target = *g;
            }
            let gain = self.gains[1] * output;
            let at = self.at;
            for (c, ch) in channels.iter_mut().enumerate() {
                let line = &mut self.lines[c];
                line[at] = ch[i];
                ch[i] = line[(at + 1) % room] * gain;
            }
            self.at = (at + 1) % room;
        }
    }

    fn latency(&self) -> usize {
        self.ahead()
    }
}

/// DeEsser: no threshold to set — the level of a band around its
/// frequency (Q its width) is compared with the whole sound's; where the
/// band stands out, that band is turned down, up to the reduction (the
/// share, doubled and rounded off, of how much it stands out). Listen
/// plays the band alone.
#[derive(Debug)]
pub(crate) struct DeEsser {
    rate: f32,
    params: Vec<f32>,
    band: Section,
    memory: Vec<[f32; 4]>,
    /// The band's and the whole sound's followed levels.
    levels: [f32; 2],
}

impl DeEsser {
    pub(crate) fn new(rate: u32) -> DeEsser {
        DeEsser {
            rate: rate.max(1) as f32,
            params: Vec::new(),
            band: Section::default(),
            memory: Vec::new(),
            levels: [0.0; 2],
        }
    }

    fn p(&self, i: usize) -> f32 {
        let s = &ed::PARAMS[i];
        self.params
            .get(i)
            .copied()
            .unwrap_or(s.default)
            .clamp(s.min, s.max)
    }
}

impl Own for DeEsser {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
        // Q to octaves: a narrow Q (2) is about two thirds of an octave.
        let q = self.p(ed::Q) as f64;
        let octaves = (2.0 / std::f64::consts::LN_2) * (1.0 / (2.0 * q)).asinh();
        self.band = Section::band(self.p(ed::FREQUENCY) as f64, octaves, self.rate as f64);
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        self.memory.resize(channels.len(), [0.0; 4]);
        let input = db(self.p(ed::INPUT));
        let output = db(self.p(ed::OUTPUT));
        let reduction = self.p(ed::REDUCTION);
        let listen = self.p(ed::LISTEN) >= 0.5;
        let up = follow(1.0, self.rate);
        let down = follow(60.0, self.rate);
        for i in 0..n {
            let (mut band, mut whole) = (0f32, 0f32);
            let mut bands = [0f32; 8];
            for (c, ch) in channels.iter_mut().enumerate() {
                let x = ch[i] * input;
                ch[i] = x;
                let b = self.band.run(x, &mut self.memory[c]);
                if c < 8 {
                    bands[c] = b;
                }
                band = band.max(b.abs());
                whole = whole.max(x.abs());
            }
            for (level, now) in self.levels.iter_mut().zip([band, whole]) {
                let c = if now > *level { up } else { down };
                *level = (now + (*level - now) * c + FLUSH) - FLUSH;
            }
            // How much the band stands out: its share of the whole.
            let share = (self.levels[0] / self.levels[1].max(1e-9)).min(1.0);
            let m = (2.0 * share).min(1.0);
            let m = (2.0 - m) * m;
            let cut = db(-m * reduction) - 1.0;
            for (c, ch) in channels.iter_mut().enumerate() {
                let b = if c < 8 { bands[c] } else { 0.0 };
                ch[i] = output * if listen { b } else { ch[i] + cut * b };
            }
        }
    }
}

/// Limiter: the sound (its lows cut below the input low cut) held under
/// the threshold — 0.99 of it — by a gain that drops at once on what it
/// sees ahead (the attack's look-ahead) and comes back by the release,
/// slower the longer and harder it has been working when adaptive; the
/// level read between the peak and the power by Peak vs. RMS, the side
/// chain's lows cut first. Auto makeup brings the threshold back to full
/// scale; then the clip gain pushes it into the clipper (shape 0 a cubic
/// curve, 1 straight up to the limit) and the output gain sets the top.
#[derive(Debug)]
pub(crate) struct Limiter {
    rate: f32,
    params: Vec<f32>,
    input_cut: Option<Section>,
    side_cut: Option<Section>,
    memory: Vec<[[f32; 4]; 2]>,
    lines: Vec<Vec<f32>>,
    at: usize,
    gain: f32,
    power: f32,
    /// How long it has been holding the level down (samples).
    working: f32,
    /// The gains the sound in the line asks for: the gain never rises
    /// above the least of them, so nothing in the line gets past.
    targets: Vec<f32>,
}

impl Limiter {
    pub(crate) fn new(rate: u32) -> Limiter {
        Limiter {
            rate: rate.max(1) as f32,
            params: Vec::new(),
            input_cut: None,
            side_cut: None,
            memory: Vec::new(),
            lines: Vec::new(),
            at: 0,
            gain: 1.0,
            power: 0.0,
            working: 0.0,
            targets: Vec::new(),
        }
    }

    fn p(&self, i: usize) -> f32 {
        let s = &el::PARAMS[i];
        self.params
            .get(i)
            .copied()
            .unwrap_or(s.default)
            .clamp(s.min, s.max)
    }

    fn ahead(&self) -> usize {
        (self.p(el::ATTACK) / 1000.0 * self.rate).round() as usize
    }
}

/// The clipper: `v` past the limit of 1 held there; `shape` 0 a cubic
/// curve 1.5·(v − v³/3), 1 straight.
fn clip(v: f32, shape: f32) -> f32 {
    let v = v.clamp(-1.0, 1.0);
    (v - v * v * v / 3.0) * 1.5 * (1.0 - shape) + v * shape
}

impl Own for Limiter {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
        let fs = self.rate as f64;
        let cut = |hz: f32| (hz >= 1.0).then(|| Section::pass(hz as f64, true, fs));
        self.input_cut = cut(self.p(el::INPUT_LOW_CUT));
        self.side_cut = cut(self.p(el::SIDE_LOW_CUT));
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let count = channels.len();
        let ahead = self.ahead();
        let room = ahead + 1;
        if self.lines.len() != count || self.lines.first().map_or(0, Vec::len) != room {
            self.lines = vec![vec![0.0; room]; count];
            self.memory = vec![[[0.0; 4]; 2]; count];
            self.targets = vec![1.0; room];
            self.at = 0;
        }
        let input = db(self.p(el::INPUT));
        let output = db(self.p(el::OUTPUT));
        let ceiling = 0.99 * db(self.p(el::THRESHOLD));
        let makeup = if self.p(el::MAKEUP) >= 0.5 {
            1.0 / db(self.p(el::THRESHOLD))
        } else {
            1.0
        };
        let push = db(self.p(el::CLIP_GAIN));
        let shape = self.p(el::CLIP_SHAPE);
        let mix = self.p(el::PEAK_RMS);
        let adaptive = self.p(el::ADAPTIVE);
        let release_ms = self.p(el::RELEASE);
        let rms = follow(10.0, self.rate);
        for i in 0..n {
            let mut peak = 0f32;
            let mut power = 0f32;
            for (c, ch) in channels.iter_mut().enumerate() {
                let mut x = ch[i] * input;
                if let Some(f) = &self.input_cut {
                    x = f.run(x, &mut self.memory[c][0]);
                }
                ch[i] = x;
                let s = match &self.side_cut {
                    Some(f) => f.run(x, &mut self.memory[c][1]),
                    None => x,
                };
                peak = peak.max(s.abs());
                power += s * s;
            }
            self.power =
                power / count.max(1) as f32 + (self.power - power / count.max(1) as f32) * rms;
            let level = peak * (1.0 - mix) + self.power.sqrt() * std::f32::consts::SQRT_2 * mix;
            self.targets[self.at] = if level > ceiling {
                ceiling / level
            } else {
                1.0
            };
            let target = self.targets.iter().fold(1f32, |m, t| m.min(*t));
            if target < self.gain {
                self.gain = target;
                self.working += 1.0;
            } else {
                // Adaptive: up to four times slower after a long, hard stretch.
                let busy = (self.working / self.rate).min(1.0) * adaptive;
                let back = follow(release_ms * (1.0 + 3.0 * busy), self.rate);
                self.gain = target + (self.gain - target) * back;
                self.working = (self.working - 4.0).max(0.0);
            }
            let g = self.gain * makeup * push;
            let at = self.at;
            for (c, ch) in channels.iter_mut().enumerate() {
                let line = &mut self.lines[c];
                line[at] = ch[i];
                ch[i] = clip(line[(at + 1) % room] * g, shape) * output;
            }
            self.at = (at + 1) % room;
        }
    }

    fn latency(&self) -> usize {
        self.ahead()
    }
}

/// Tremolo Pan: an oscillator a side (the right one moved on by the phase,
/// half a round at 100 %), its corners softened by a one-pole follower,
/// shaped from a sine toward a square by a rational curve and rounded by
/// a gentle saturation; each side's level 1 − depth + depth·the oscillator
/// (at full depth the trough turns the sound over).
#[derive(Debug)]
pub(crate) struct TremoloPan {
    rate: f32,
    params: Vec<f32>,
    tempo: bv_timeline::Tempo,
    phase: f64,
    smooth: [f32; 2],
}

impl TremoloPan {
    pub(crate) fn new(rate: u32) -> TremoloPan {
        TremoloPan {
            rate: rate.max(1) as f32,
            params: Vec::new(),
            tempo: bv_timeline::Tempo::default(),
            phase: 0.0,
            smooth: [0.0; 2],
        }
    }

    fn p(&self, i: usize) -> f32 {
        let s = &et::PARAMS[i];
        self.params
            .get(i)
            .copied()
            .unwrap_or(s.default)
            .clamp(s.min, s.max)
    }
}

impl Own for TremoloPan {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
    }

    fn keep_time(&mut self, tempo: &bv_timeline::Tempo) {
        self.tempo = *tempo;
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let input = db(self.p(et::INPUT));
        let output = db(self.p(et::OUTPUT));
        let hz = if self.p(et::SYNC) >= 0.5 {
            1.0 / bv_timeline::audio_fx::tempo::seconds(self.p(et::NOTE), &self.tempo)
        } else {
            self.p(et::RATE) as f64
        };
        let step = hz / self.rate as f64;
        let offset = 0.5 * self.p(et::PHASE) as f64;
        let kappa = 1.0 - 1.0 / (0.49 * self.p(et::SHAPE) + 0.5);
        let depth = self.p(et::DEPTH);
        // The corners softened over about a fiftieth of a round.
        let soften = 1.0
            - (-std::f32::consts::TAU * (hz as f32 * 50.0).min(self.rate / 4.0) / self.rate).exp();
        let start = self.phase;
        for (c, ch) in channels.iter_mut().enumerate() {
            let side = c.min(1);
            let mut phase = start + if side == 1 { offset } else { 0.0 };
            let s = &mut self.smooth[side];
            for x in ch.iter_mut() {
                let sine = (std::f64::consts::TAU * phase).sin() as f32;
                *s += (sine - *s) * soften;
                let a = s.abs();
                let g = s.signum() * a / ((a - 1.0) * kappa + a).max(1e-9);
                let g = (0.87 * g).clamp(-1.0, 1.0);
                let g = ((2.0 - g.abs()) * g).clamp(-1.0, 1.0);
                *x *= input * (1.0 - depth + depth * g) * output;
                phase += step;
            }
        }
        self.phase = (start + step * n as f64).fract();
    }
}

/// Tube Stage: the pre EQ (a tilt around 80 Hz), then one or two triode
/// stages — the grid resting halfway up the plate's curve, less a little
/// of its own low-passed output (the cathode), the curve cut off below 0
/// and saturated past 1, the coupling's DC taken away — driven by the
/// gain (class AB a little straighter), then the post EQ (a tilt around
/// 350 Hz); twice the rate inside when oversampling.
#[derive(Debug)]
pub(crate) struct TubeStage {
    rate: f32,
    params: Vec<f32>,
    /// Each channel's: two stages' cathode and coupling memories, the
    /// EQs' memories, and the last input (for oversampling).
    state: Vec<([f32; 4], [[f32; 4]; 2], f32)>,
    pre: Section,
    post: Section,
}

impl TubeStage {
    pub(crate) fn new(rate: u32) -> TubeStage {
        TubeStage {
            rate: rate.max(1) as f32,
            params: Vec::new(),
            state: Vec::new(),
            pre: Section::default(),
            post: Section::default(),
        }
    }

    fn p(&self, i: usize) -> f32 {
        let s = &tu::PARAMS[i];
        self.params
            .get(i)
            .copied()
            .unwrap_or(s.default)
            .clamp(s.min, s.max)
    }
}

/// The plate's current at grid voltage `v`: none below 0, `v³·(2 − v³)`
/// up to 1, all of it past 1.
fn plate(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    let c = v * v * v;
    c * (2.0 - c)
}

/// Where the grid rests (its bias) on the plate's curve.
const REST: f32 = 0.5;

/// One triode stage at sampling rate `fs`: `x` in, its cathode and
/// coupling memories in `m`; small sounds come out as loud as they went
/// in, louder ones bend on the curve.
fn triode(x: f32, drive: f32, soft: f32, m: &mut [f32], fs: f32) -> f32 {
    let cathode = 1.0 - (-std::f32::consts::TAU * 20.0 / fs).exp();
    let coupling = 1.0 - (-std::f32::consts::TAU * 10.0 / fs).exp();
    let curve = |v: f32| plate(v) * (1.0 - soft) + v.clamp(0.0, 1.0) * soft;
    // The curve's slope at rest: 3v²·(2 − 2v³), and 1 where straight.
    let slope = 3.0 * REST * REST * (2.0 - 2.0 * REST.powi(3)) * (1.0 - soft) + soft;
    let grid = REST + 0.5 * (x * drive - m[0]);
    let y = (curve(grid) - curve(REST)) / (0.5 * slope);
    m[0] += (y * 0.05 - m[0]) * cathode;
    m[1] += (y - m[1]) * coupling;
    y - m[1]
}

impl Own for TubeStage {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
        let fs = self.rate as f64;
        // The EQs: −1 to 1 a tilt of ±6 dB around their frequencies.
        self.pre = Section::shelf(80.0, -6.0 * self.p(tu::PRE_EQ) as f64, false, fs);
        self.post = Section::shelf(350.0, 6.0 * self.p(tu::POST_EQ) as f64, true, fs);
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        self.state
            .resize(channels.len(), ([0.0; 4], [[0.0; 4]; 2], 0.0));
        let input = db(self.p(tu::INPUT));
        let output = db(self.p(tu::OUTPUT));
        let drive = db(self.p(tu::DRIVE));
        let stages = self.p(tu::STAGES).round().clamp(1.0, 2.0) as usize;
        let soft = if self.p(tu::CLASS) >= 0.5 { 0.18 } else { 0.0 };
        let over = self.p(tu::OVERSAMPLE) >= 0.5;
        let fs = if over { 2.0 * self.rate } else { self.rate };
        for (ch, (tubes, eqs, last)) in channels.iter_mut().zip(self.state.iter_mut()) {
            for x in ch.iter_mut() {
                let v = self.pre.run(*x * input, &mut eqs[0]);
                let mut run = |v: f32| {
                    let mut y = v;
                    for s in 0..stages {
                        y = triode(
                            y,
                            if s == 0 { drive } else { 1.0 },
                            soft,
                            &mut tubes[2 * s..2 * s + 2],
                            fs,
                        );
                    }
                    y
                };
                let y = if over {
                    // Twice the rate: halfway from the last input, then the
                    // input, averaged back down.
                    let half = run((*last + v) / 2.0);
                    let full = run(v);
                    (half + full) / 2.0
                } else {
                    run(v)
                };
                *last = v;
                *x = self.post.run(y, &mut eqs[1]) * output;
            }
        }
    }
}

/// Vocal Strip: a voice's chain — a high-pass at its frequency; a gate
/// that turns the sound well down (24 dB) below its threshold (off at its
/// lowest); the de-esser's band at its frequency, turned down by up to
/// its amount where it stands out; a compressor whose amount sets both
/// how far it squeezes (the threshold comes down by half the amount, at
/// 4:1) and, with auto makeup, how much it gives back, its release slower
/// as the adaptive setting rises; and a tilt of ±6 dB around 1 kHz.
#[derive(Debug)]
pub(crate) struct VocalStrip {
    rate: f32,
    params: Vec<f32>,
    high_pass: Section,
    band: Section,
    tilt: [Section; 2],
    /// Each channel's filters' memories: high-pass, band, tilt low, tilt
    /// high.
    memory: Vec<[[f32; 4]; 4]>,
    gate: f32,
    gate_gain: f32,
    levels: [f32; 2],
    comp: f32,
    /// One sample's de-esser band on each channel.
    bands: Vec<f32>,
}

impl VocalStrip {
    pub(crate) fn new(rate: u32) -> VocalStrip {
        VocalStrip {
            rate: rate.max(1) as f32,
            params: Vec::new(),
            high_pass: Section::default(),
            band: Section::default(),
            tilt: [Section::default(); 2],
            memory: Vec::new(),
            gate: 0.0,
            gate_gain: 1.0,
            levels: [0.0; 2],
            comp: 0.0,
            bands: Vec::new(),
        }
    }

    fn p(&self, i: usize) -> f32 {
        let s = &ev::PARAMS[i];
        self.params
            .get(i)
            .copied()
            .unwrap_or(s.default)
            .clamp(s.min, s.max)
    }
}

impl Own for VocalStrip {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
        let fs = self.rate as f64;
        self.high_pass = Section::pass(self.p(ev::HIGH_PASS) as f64, true, fs);
        self.band = Section::band(self.p(ev::DEESS_FREQUENCY) as f64, 1.5, fs);
        let tone = 6.0 * self.p(ev::TONE) as f64;
        self.tilt = [
            Section::shelf(1000.0, -tone / 2.0, false, fs),
            Section::shelf(1000.0, tone / 2.0, true, fs),
        ];
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        self.memory.resize(channels.len(), [[0.0; 4]; 4]);
        let fs = self.rate;
        let input = db(self.p(ev::INPUT));
        let output = db(self.p(ev::OUTPUT));
        let gate_at = self.p(ev::GATE);
        let gate_on = gate_at > -90.0;
        let gate_at = db(gate_at);
        let deess = self.p(ev::DEESSER);
        let amount = self.p(ev::COMPRESSION);
        let threshold = db(-amount / 2.0);
        let makeup = if self.p(ev::MAKEUP) >= 0.5 {
            db(amount / 2.0 * 0.75 * 0.7)
        } else {
            1.0
        };
        let attack = follow(self.p(ev::ATTACK), fs);
        let release = follow(80.0 * (1.0 + self.p(ev::ADAPTIVE) / 2.0), fs);
        let (gate_up, gate_down) = (follow(1.0, fs), follow(100.0, fs));
        let (band_up, band_down) = (follow(1.0, fs), follow(60.0, fs));
        self.bands.resize(channels.len(), 0.0);
        for i in 0..n {
            let (mut peak, mut band_peak) = (0f32, 0f32);
            for (c, ch) in channels.iter_mut().enumerate() {
                let m = &mut self.memory[c];
                let x = self.high_pass.run(ch[i] * input, &mut m[0]);
                let b = self.band.run(x, &mut m[1]);
                self.bands[c] = b;
                ch[i] = x;
                peak = peak.max(x.abs());
                band_peak = band_peak.max(b.abs());
            }
            // The gate.
            let c = if peak > self.gate { gate_up } else { gate_down };
            self.gate = peak + (self.gate - peak) * c;
            let gate_target = if !gate_on || self.gate >= gate_at {
                1.0
            } else {
                db(-24.0)
            };
            let c = if gate_target > self.gate_gain {
                gate_up
            } else {
                gate_down
            };
            self.gate_gain = gate_target + (self.gate_gain - gate_target) * c;
            // The de-esser.
            for (level, now) in self.levels.iter_mut().zip([band_peak, peak]) {
                let c = if now > *level { band_up } else { band_down };
                *level = (now + (*level - now) * c + FLUSH) - FLUSH;
            }
            let share = (self.levels[0] / self.levels[1].max(1e-9)).min(1.0);
            let m = (2.0 * share).min(1.0);
            let cut = db(-(2.0 - m) * m * deess) - 1.0;
            // The compressor, on the gated, de-essed level.
            let level = peak * self.gate_gain;
            let c = if level > self.comp { attack } else { release };
            self.comp = (level + (self.comp - level) * c + FLUSH) - FLUSH;
            let squeeze = if amount > 0.0 && self.comp > threshold {
                (self.comp / threshold).powf(0.25 - 1.0)
            } else {
                1.0
            };
            let g = self.gate_gain * squeeze * makeup * output;
            for (c, ch) in channels.iter_mut().enumerate() {
                let m = &mut self.memory[c];
                let x = (ch[i] + cut * self.bands[c]) * g;
                let low = self.tilt[0].run(x, &mut m[2]);
                ch[i] = self.tilt[1].run(low, &mut m[3]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(specs: &[bv_timeline::ParamSpec], values: &[(usize, f32)]) -> Vec<f32> {
        let mut p: Vec<f32> = specs.iter().map(|s| s.default).collect();
        for &(i, v) in values {
            p[i] = v;
        }
        p
    }

    fn sine(hz: f32, level: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| level * (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin())
            .collect()
    }

    /// The level (dB) of the second half of `fx`'s output for a sine.
    fn level(fx: &mut dyn Own, hz: f32, amp: f32) -> f32 {
        let mut ch = vec![sine(hz, amp, 48_000), sine(hz, amp, 48_000)];
        fx.process(&mut ch);
        let tail = &ch[0][24_000..];
        10.0 * (2.0 * tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32).log10()
    }

    #[test]
    fn the_gate_lowers_quiet_sound_by_its_range() {
        let mut g = Gate::new(48_000);
        g.set(&with(
            eg::PARAMS,
            &[
                (eg::THRESHOLD, -30.0),
                (eg::RANGE, 20.0),
                (eg::RELEASE, 10.0),
            ],
        ));
        let quiet = level(&mut g, 1000.0, 0.01);
        assert!((quiet - (-40.0 - 20.0)).abs() < 1.0, "{quiet}");
        let mut g = Gate::new(48_000);
        g.set(&with(
            eg::PARAMS,
            &[(eg::THRESHOLD, -30.0), (eg::RANGE, 20.0)],
        ));
        let loud = level(&mut g, 1000.0, 0.5);
        assert!((loud - 20.0 * 0.5f32.log10()).abs() < 0.5, "{loud}");
    }

    #[test]
    fn the_deesser_turns_down_its_band_only() {
        let mut d = DeEsser::new(48_000);
        d.set(&with(ed::PARAMS, &[(ed::REDUCTION, 12.0)]));
        let hiss = level(&mut d, 7000.0, 0.5);
        assert!(hiss < 20.0 * 0.5f32.log10() - 6.0, "{hiss}");
        let mut d = DeEsser::new(48_000);
        d.set(&with(ed::PARAMS, &[(ed::REDUCTION, 12.0)]));
        let low = level(&mut d, 200.0, 0.5);
        assert!((low - 20.0 * 0.5f32.log10()).abs() < 1.0, "{low}");
    }

    #[test]
    fn the_limiter_holds_its_threshold() {
        let mut l = Limiter::new(48_000);
        l.set(&with(
            el::PARAMS,
            &[(el::THRESHOLD, -12.0), (el::MAKEUP, 0.0), (el::OUTPUT, 0.0)],
        ));
        let mut ch = vec![sine(200.0, 1.0, 48_000)];
        l.process(&mut ch);
        let peak = ch[0][24_000..].iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(peak <= 0.99 * 10f32.powf(-12.0 / 20.0) + 1e-3, "{peak}");
        assert!(peak > 0.2, "{peak}");
    }

    #[test]
    fn tremolo_moves_the_level_and_tube_stage_starts_clean() {
        let mut t = TremoloPan::new(48_000);
        t.set(&with(et::PARAMS, &[(et::RATE, 2.0), (et::DEPTH, 0.5)]));
        let mut ch = vec![vec![1.0f32; 48_000]];
        t.process(&mut ch);
        let (lo, hi) = ch[0]
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), s| (a.min(*s), b.max(*s)));
        assert!(lo < 0.2 && hi > 0.8, "{lo} {hi}");
        // Tube Stage at its defaults: a quiet tone comes out about as loud.
        let mut s = TubeStage::new(48_000);
        s.set(&with(tu::PARAMS, &[]));
        let out = level(&mut s, 1000.0, 0.01);
        assert!((out + 40.0).abs() < 1.5, "{out}");
    }

    #[test]
    fn vocal_strip_at_its_defaults_leaves_a_voice_alone() {
        let mut v = VocalStrip::new(48_000);
        v.set(&with(ev::PARAMS, &[]));
        let out = level(&mut v, 300.0, 0.3);
        assert!((out - 20.0 * 0.3f32.log10()).abs() < 0.5, "{out}");
    }
}
