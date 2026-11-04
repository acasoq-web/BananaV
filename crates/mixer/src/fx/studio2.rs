//! BananaV's versions of the rest of the studio set: Compressor, Phaser,
//! Chorus/Flanger, Studio Reverb and Stereo Delay, built from what their
//! controls do.

use bv_timeline::Tempo;
use bv_timeline::audio_fx::{
    efx_chorus_flanger as ec, efx_compressor as ek, efx_phaser as ep, efx_reverb as er,
    efx_stereo_delay as es, tempo,
};

use super::Own;
use super::eq::Section;

/// Added and taken away again on fed-back state, so a fading signal ends
/// at 0 instead of lingering as a denormal number.
const FLUSH: f32 = 1e-18;

fn db(v: f32) -> f32 {
    10f32.powf(v / 20.0)
}

/// A one-pole follower's coefficient for a time in milliseconds.
fn follow(ms: f32, fs: f32) -> f32 {
    let n = ms / 1000.0 * fs;
    if n < 1.0 { 0.0 } else { (-1.0 / n).exp() }
}

/// A one-pole low-pass's coefficient for a frequency.
fn one_pole(hz: f32, fs: f32) -> f32 {
    1.0 - (-std::f32::consts::TAU * hz.min(fs * 0.49) / fs).exp()
}

/// `(2 − |v|)·v` held within ±1: a gentle parabolic saturation.
fn soft(v: f32) -> f32 {
    let v = v.clamp(-1.0, 1.0);
    (2.0 - v.abs()) * v
}

/// The oscillator's shape: a sine with a straighter middle.
fn lfo_shape(s: f32) -> f32 {
    0.33 * s.powi(7) + 0.67 * s
}

/// A value of `params` in range, its default when missing.
fn value(params: &[f32], specs: &[bv_timeline::ParamSpec], i: usize) -> f32 {
    let s = &specs[i];
    params
        .get(i)
        .copied()
        .unwrap_or(s.default)
        .clamp(s.min, s.max)
}

/// A delay line read between its samples.
#[derive(Clone, Debug)]
struct Line {
    buf: Vec<f32>,
    mask: usize,
    at: usize,
}

impl Line {
    fn new(len: usize) -> Line {
        let n = len.max(4).next_power_of_two();
        Line {
            buf: vec![0.0; n],
            mask: n - 1,
            at: 0,
        }
    }

    /// The sound `d` samples back (1 the newest), between samples.
    fn read(&self, d: f32) -> f32 {
        let d = d.clamp(1.0, (self.buf.len() - 2) as f32);
        let whole = d.floor();
        let part = d - whole;
        let i = whole as usize;
        let a = self.buf[(self.at + self.buf.len() - (i - 1)) & self.mask];
        let b = self.buf[(self.at + self.buf.len() - i) & self.mask];
        a + (b - a) * part
    }

    fn write(&mut self, x: f32) {
        self.at = (self.at + 1) & self.mask;
        self.buf[self.at] = x;
    }
}

/// Compressor: the side chain (the sound, filtered when asked) read
/// after the gain already applied — a feedback compressor, as the classic
/// FET designs — its level followed by the attack and three releases (the
/// adaptive setting spreading them: a short burst lets go fast, a long
/// passage slowly); above the threshold the gain comes down by the ratio
/// over a 6 dB knee; auto makeup gives back half of what it takes at full
/// scale; the mix blends in the untouched sound (parallel compression);
/// the soft clip rounds off what goes past full scale.
#[derive(Debug)]
pub(crate) struct Compressor {
    rate: f32,
    params: Vec<f32>,
    side: Option<Section>,
    memory: Vec<[f32; 4]>,
    /// The attack follower and the three release followers.
    levels: [f32; 3],
    gain: f32,
}

impl Compressor {
    pub(crate) fn new(rate: u32) -> Compressor {
        Compressor {
            rate: rate.max(1) as f32,
            params: Vec::new(),
            side: None,
            memory: Vec::new(),
            levels: [0.0; 3],
            gain: 1.0,
        }
    }

    fn p(&self, i: usize) -> f32 {
        value(&self.params, ek::PARAMS, i)
    }
}

impl Own for Compressor {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
        let fs = self.rate as f64;
        let hz = self.p(ek::FILTER) as f64;
        self.side = match self.p(ek::FILTER_TYPE).round() as i32 {
            1 => Some(Section::pass(hz, true, fs)),
            2 => Some(Section::band(hz, 1.0, fs)),
            3 => Some(Section::pass(hz, false, fs)),
            _ => None,
        };
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        self.memory.resize(channels.len(), [0.0; 4]);
        let fs = self.rate;
        let input = self.p(ek::INPUT);
        let output = db(self.p(ek::OUTPUT));
        let threshold = self.p(ek::THRESHOLD);
        let slope = 1.0 / self.p(ek::RATIO) - 1.0;
        let makeup = if self.p(ek::MAKEUP) >= 0.5 {
            db(-threshold * -slope / 2.0)
        } else {
            1.0
        };
        let attack = follow(self.p(ek::ATTACK), fs);
        let release = self.p(ek::RELEASE);
        let spread = 1.0 + self.p(ek::ADAPTIVE);
        let releases = [
            follow(release / spread, fs),
            follow(release, fs),
            follow(release * spread, fs),
        ];
        let mix = self.p(ek::MIX);
        let clip = self.p(ek::SOFT_CLIP) >= 0.5;
        let listen = self.p(ek::LISTEN) >= 0.5;
        for i in 0..n {
            let mut peak = 0f32;
            for (c, ch) in channels.iter_mut().enumerate() {
                let x = ch[i] * input;
                ch[i] = x;
                let s = match &self.side {
                    Some(f) => f.run(x, &mut self.memory[c]),
                    None => x,
                };
                if listen {
                    ch[i] = s;
                }
                peak = peak.max(s.abs());
            }
            // Read after the gain: the feedback.
            let seen = peak * self.gain;
            for (level, rel) in self.levels.iter_mut().zip(releases) {
                let c = if seen > *level { attack } else { rel };
                *level = (seen + (*level - seen) * c + FLUSH) - FLUSH;
            }
            let e = self.levels.iter().fold(0f32, |m, l| m.max(*l));
            let over = 20.0 * e.max(1e-9).log10() - threshold;
            let cut = if over <= -3.0 {
                0.0
            } else if over < 3.0 {
                slope * (over + 3.0).powi(2) / 12.0
            } else {
                slope * over
            };
            // The gain read off the level after it (the feedback): it
            // settles where that level sits on the curve.
            self.gain = db(cut).min(1.0);
            for ch in channels.iter_mut() {
                if listen {
                    continue;
                }
                let x = ch[i];
                let mut y = mix * x * self.gain * makeup + (1.0 - mix) * x;
                if clip {
                    let v = y.clamp(-1.0, 1.0);
                    y = 1.5 * (v - v * v * v / 3.0) * 0.98855;
                }
                ch[i] = y * output;
            }
        }
    }
}

/// Phaser: an oscillator a side (the right one moved on by the phase
/// offset), shaped straighter in its middle, sweeps 4, 8 or 12 first-order
/// all-pass stages up and down from the centre frequency (by up to eleven
/// times each way at full depth); what comes out of them, its lows and
/// highs trimmed, goes back in by the feedback; the stages' sound (its DC
/// taken away) is mixed with the sound itself, and the notches appear
/// where the two cancel.
#[derive(Debug)]
pub(crate) struct Phaser {
    rate: f32,
    params: Vec<f32>,
    phase: f64,
    /// Each channel: the stages' memories (input, output), the feedback's
    /// two low-passes, the DC blocker, the last chain output.
    state: Vec<(Vec<[f32; 2]>, [f32; 3], f32)>,
}

impl Phaser {
    pub(crate) fn new(rate: u32) -> Phaser {
        Phaser {
            rate: rate.max(1) as f32,
            params: Vec::new(),
            phase: 0.0,
            state: Vec::new(),
        }
    }

    fn p(&self, i: usize) -> f32 {
        value(&self.params, ep::PARAMS, i)
    }
}

impl Own for Phaser {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let fs = self.rate;
        let stages = [4, 8, 12][self.p(ep::MODE).round().clamp(0.0, 2.0) as usize];
        if self.state.len() != channels.len()
            || self.state.first().map_or(0, |s| s.0.len()) != stages
        {
            self.state = vec![(vec![[0.0; 2]; stages], [0.0; 3], 0.0); channels.len()];
        }
        let input = self.p(ep::INPUT);
        let output = self.p(ep::OUTPUT);
        let step = self.p(ep::RATE) as f64 / fs as f64;
        let depth = self.p(ep::DEPTH);
        let offset = self.p(ep::PHASE) as f64 * 0.5;
        let feedback = self.p(ep::FEEDBACK);
        let centre = self.p(ep::CENTER);
        let mix = self.p(ep::MIX);
        let (lp1, lp2, dc) = (one_pole(5000.0, fs), one_pole(50.0, fs), one_pole(20.0, fs));
        let start = self.phase;
        for (c, ch) in channels.iter_mut().enumerate() {
            let side = if c % 2 == 1 { offset } else { 0.0 };
            let (chain, filters, last) = &mut self.state[c];
            let mut a = 0.0;
            for (i, x) in ch.iter_mut().enumerate() {
                // The sweep, worked out every 16 samples.
                if i % 16 == 0 {
                    let t = start + side + step * i as f64;
                    let s = (std::f64::consts::TAU * t).sin() as f32;
                    let v = lfo_shape(s);
                    let e = 10.0 * depth * v.abs();
                    let f = if v >= 0.0 {
                        centre * (1.0 + e)
                    } else {
                        centre / (1.0 + e)
                    };
                    let t = (std::f32::consts::PI * f.clamp(0.1, fs * 0.49) / fs).tan();
                    a = (t - 1.0) / (t + 1.0);
                }
                let dry = *x * input;
                filters[0] += (*last - filters[0]) * lp1;
                filters[1] += (filters[0] - filters[1]) * lp2;
                let back = filters[0] - filters[1];
                let mut u = soft((feedback * back + 0.25 * dry) / 2.0);
                for st in chain.iter_mut() {
                    let y = a * u + st[0] - a * st[1];
                    st[0] = u;
                    st[1] = (y + FLUSH) - FLUSH;
                    u = st[1];
                }
                *last = u;
                filters[2] += (u - filters[2]) * dc;
                let wet = 4.0 * (u - filters[2]);
                *x = (mix * wet + (1.0 - mix) * dry) * output;
            }
        }
        self.phase = (start + step * n as f64).fract();
    }
}

/// Chorus/Flanger: three voices a side, each a delay line read at a time
/// its own oscillator swings (the voices a third of a round apart, the
/// right side moved on by the phase offset); a chorus sits around 7 ms, a
/// flanger near 0.5 ms, an ensemble wider around 10 ms. Each voice feeds
/// back on itself (rounded off), the voices' sum has its lows cut, and
/// the mix blends it with the sound.
#[derive(Debug)]
pub(crate) struct ChorusFlanger {
    rate: f32,
    params: Vec<f32>,
    phase: f64,
    /// Each channel's three lines, their smoothed delays, and the low cut's
    /// memory.
    voices: Vec<([Line; 3], [f32; 3], f32)>,
}

impl ChorusFlanger {
    pub(crate) fn new(rate: u32) -> ChorusFlanger {
        ChorusFlanger {
            rate: rate.max(1) as f32,
            params: Vec::new(),
            phase: 0.0,
            voices: Vec::new(),
        }
    }

    fn p(&self, i: usize) -> f32 {
        value(&self.params, ec::PARAMS, i)
    }
}

impl Own for ChorusFlanger {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let fs = self.rate;
        let room = (0.05 * fs) as usize;
        if self.voices.len() != channels.len() {
            let line = Line::new(room);
            self.voices = vec![([line.clone(), line.clone(), line], [1.0; 3], 0.0); channels.len()];
        }
        // Each mode's middle and swing, in milliseconds.
        let (base, swing) = [(7.0, 5.0), (0.5, 4.0), (10.0, 8.0)]
            [self.p(ec::MODE).round().clamp(0.0, 2.0) as usize];
        let input = self.p(ec::INPUT);
        let output = self.p(ec::OUTPUT);
        let step = self.p(ec::RATE) as f64 / fs as f64;
        let depth = self.p(ec::DEPTH);
        let offset = self.p(ec::PHASE) as f64 * 0.5;
        let feedback = self.p(ec::FEEDBACK);
        let cut = one_pole(self.p(ec::LOW_CUT), fs);
        let mix = self.p(ec::MIX);
        let (wet_gain, dry_gain) = (
            (mix * std::f32::consts::FRAC_PI_2).sin(),
            (mix * std::f32::consts::FRAC_PI_2).cos(),
        );
        let glide = one_pole(30.0, fs);
        let start = self.phase;
        for (c, ch) in channels.iter_mut().enumerate() {
            let side = if c % 2 == 1 { offset } else { 0.0 };
            let (lines, delays, low) = &mut self.voices[c];
            for (i, x) in ch.iter_mut().enumerate() {
                let dry = *x * input;
                let mut sum = 0.0;
                for (k, (line, d)) in lines.iter_mut().zip(delays.iter_mut()).enumerate() {
                    let t = start + side + k as f64 / 3.0 + step * i as f64;
                    let v = (lfo_shape((std::f64::consts::TAU * t).sin() as f32) + 1.0) / 2.0;
                    // Toward the top of the swing more slowly than a sine.
                    let r = v / (v - 2.0303 * (v - 1.0));
                    let target = ((r * depth * swing + base) * fs / 1000.0).max(1.0);
                    *d += (target - *d) * glide;
                    let late = line.read(*d);
                    line.write((soft((dry + feedback * late) / 2.0) * 2.0 + FLUSH) - FLUSH);
                    sum += late;
                }
                let wet = sum / 3.0;
                *low += (wet - *low) * cut;
                *x = (wet_gain * (wet - *low) + dry_gain * dry) * output;
            }
        }
        self.phase = (start + step * n as f64).fract();
    }
}

/// Studio Reverb: the sound (its lows cut, after the pre-delay) through
/// four diffusers, then a tank of two loops — each a modulated diffuser,
/// a delay, a damping low-pass and a decaying diffuser, feeding the other
/// — tapped at several points for the left and the right; the mode sets
/// the tank's size and brightness (plate, room, hall, vintage plate), the
/// time how long it rings, the modulation how much the loops drift.
#[derive(Debug)]
pub(crate) struct StudioReverb {
    rate: f32,
    params: Vec<f32>,
    pre: Line,
    diffusers: Vec<Line>,
    tank: Vec<Line>,
    low: f32,
    damp: [f32; 2],
    phase: f64,
    scale: f32,
}

/// The diffusers' and the tank's lengths at 29.8 kHz (ms × 29.8): the
/// published plate topology's proportions, scaled by the rate and size.
const DIFFUSE: [f32; 4] = [142.0, 107.0, 379.0, 277.0];
const TANK: [f32; 8] = [672.0, 4453.0, 1800.0, 3720.0, 908.0, 4217.0, 2656.0, 3163.0];

impl StudioReverb {
    pub(crate) fn new(rate: u32) -> StudioReverb {
        StudioReverb {
            rate: rate.max(1) as f32,
            params: Vec::new(),
            pre: Line::new(1),
            diffusers: Vec::new(),
            tank: Vec::new(),
            low: 0.0,
            damp: [0.0; 2],
            phase: 0.0,
            scale: 0.0,
        }
    }

    fn p(&self, i: usize) -> f32 {
        value(&self.params, er::PARAMS, i)
    }
}

/// A diffuser (an all-pass on a line): `x` in with gain `g`, `d` samples.
fn diffuse(line: &mut Line, x: f32, g: f32, d: f32) -> f32 {
    let late = line.read(d);
    let v = x - g * late;
    line.write((v + FLUSH) - FLUSH);
    late + g * v
}

impl Own for StudioReverb {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
        let mode = self.p(er::MODE).round() as usize;
        let room = [1.0, 0.6, 1.6, 1.2][mode.min(3)];
        let scale = self.rate / 29_800.0 * room * (0.3 + 1.2 * self.p(er::SIZE));
        if (scale - self.scale).abs() > 1e-6 {
            self.scale = scale;
            self.diffusers = DIFFUSE
                .iter()
                .map(|l| Line::new((l * scale) as usize + 8))
                .collect();
            self.tank = TANK
                .iter()
                .map(|l| Line::new((l * scale) as usize + 100))
                .collect();
            self.pre = Line::new((0.26 * self.rate) as usize);
        }
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let count = channels.len();
        if count == 0 {
            return;
        }
        let fs = self.rate;
        let s = self.scale;
        let input = self.p(er::INPUT);
        let output = self.p(er::OUTPUT);
        let mix = self.p(er::MIX);
        let decay = 0.2 + 0.78 * self.p(er::TIME);
        let u = (1.0 - self.p(er::DAMPING)).clamp(0.0001, 0.99999);
        let fc = 20_000.0 * u / (u - 1.2222 * (u - 1.0)) + 500.0;
        let damp = one_pole(fc, fs);
        let diffusion = self.p(er::DIFFUSION);
        let (g1, g2) = (0.75 * diffusion, 0.625 * diffusion);
        let swing = 16.0 * self.p(er::MODULATION) * s;
        let pre = (self.p(er::PREDELAY) / 1000.0 * fs).max(1.0);
        let cut = one_pole(self.p(er::LOW_CUT), fs);
        let step = 1.0 / fs as f64;
        for i in 0..n {
            let x = channels.iter().map(|c| c[i]).sum::<f32>() / count as f32 * input;
            self.pre.write(x);
            let mut v = self.pre.read(pre);
            self.low += (v - self.low) * cut;
            v -= self.low;
            for (k, line) in self.diffusers.iter_mut().enumerate() {
                v = diffuse(line, v, if k < 2 { g1 } else { g2 }, DIFFUSE[k] * s);
            }
            let wobble = (std::f64::consts::TAU * self.phase).sin() as f32 * swing;
            self.phase = (self.phase + step).fract();
            // The two loops, each fed by the other's end.
            for side in 0..2 {
                let other = self.tank[if side == 0 { 7 } else { 3 }]
                    .read(TANK[if side == 0 { 7 } else { 3 }] * s);
                let into = v + decay * other;
                let base = 4 * side;
                let a = diffuse(&mut self.tank[base], into, -0.7, TANK[base] * s + wobble);
                self.tank[base + 1].write(a);
                let b = self.tank[base + 1].read(TANK[base + 1] * s);
                self.damp[side] += (b - self.damp[side]) * damp;
                let c = diffuse(
                    &mut self.tank[base + 2],
                    self.damp[side] * decay,
                    0.5,
                    TANK[base + 2] * s,
                );
                self.tank[base + 3].write(c);
            }
            // Taps across both loops for each side.
            let t = |k: usize, f: f32| self.tank[k].read(TANK[k] * s * f);
            let left = 0.6
                * (t(5, 0.06) + t(5, 0.68) - t(6, 0.48) + t(7, 0.47)
                    - t(1, 0.47)
                    - t(2, 0.04)
                    - t(3, 0.27));
            let right = 0.6
                * (t(1, 0.08) + t(1, 0.61) - t(2, 0.66) + t(3, 0.89)
                    - t(5, 0.47)
                    - t(6, 0.07)
                    - t(7, 0.08));
            for (c, ch) in channels.iter_mut().enumerate() {
                let wet = if count == 1 {
                    (left + right) / 2.0
                } else if c % 2 == 0 {
                    left
                } else {
                    right
                };
                ch[i] = (mix * wet + (1.0 - mix) * ch[i] * input) * output;
            }
        }
    }
}

/// Stereo Delay: each side's delay in milliseconds or a note of the
/// tempo, its time gliding when changed; the echo fed back through the
/// damping (a low-pass from 18 kHz down to 1 kHz) and the low cut. Analog
/// darkens the line and rounds it off; the bucket brigade narrows its band
/// by its stages, squeezes before the line and opens after it (the
/// compander), lets its clock drift and adds its noise. The mix blends
/// the echoes with the sound.
#[derive(Debug)]
pub(crate) struct StereoDelay {
    rate: f32,
    params: Vec<f32>,
    tempo: Tempo,
    lines: Vec<Line>,
    /// Each side's gliding delay, its loop's low-pass and low cut, and
    /// its band limit's memory.
    state: Vec<[f32; 4]>,
    drift: f64,
    noise: u32,
}

impl StereoDelay {
    pub(crate) fn new(rate: u32) -> StereoDelay {
        StereoDelay {
            rate: rate.max(1) as f32,
            params: Vec::new(),
            tempo: Tempo::default(),
            lines: Vec::new(),
            state: Vec::new(),
            drift: 0.0,
            noise: 0x1234_5678,
        }
    }

    fn p(&self, i: usize) -> f32 {
        value(&self.params, es::PARAMS, i)
    }
}

impl Own for StereoDelay {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
    }

    fn keep_time(&mut self, tempo: &Tempo) {
        self.tempo = *tempo;
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let fs = self.rate;
        if self.lines.len() != channels.len() {
            self.lines = vec![Line::new((4.2 * fs) as usize); channels.len()];
            self.state = vec![[0.0; 4]; channels.len()];
        }
        let mode = self.p(es::MODE).round() as i32;
        let input = self.p(es::INPUT);
        let output = self.p(es::OUTPUT);
        let synced = self.p(es::SYNC) >= 0.5;
        let times: [f32; 2] =
            [(es::LEFT, es::LEFT_NOTE), (es::RIGHT, es::RIGHT_NOTE)].map(|(ms, note)| {
                let seconds = if synced {
                    tempo::seconds(self.p(note), &self.tempo) as f32
                } else {
                    self.p(ms) / 1000.0
                };
                (seconds * fs).clamp(1.0, 4.1 * fs)
            });
        let feedback = self.p(es::FEEDBACK);
        let damp = one_pole(18_000.0 * 0.0556f32.powf(self.p(es::DAMPING)), fs);
        let cut = one_pole(self.p(es::LOW_CUT), fs);
        let mix = self.p(es::MIX);
        let stages = self.p(es::STAGES);
        let band = one_pole(
            if mode == 2 {
                12_000.0 / stages
            } else {
                9_000.0
            },
            fs,
        );
        let squeeze = 1.0 + self.p(es::COMPANDER) / 5.0;
        let drift = self.p(es::DRIFT);
        let noise = if mode == 2 {
            db(self.p(es::NOISE))
        } else {
            0.0
        };
        let glide = one_pole(5.0, fs);
        let start = self.drift;
        for (c, ch) in channels.iter_mut().enumerate() {
            let line = &mut self.lines[c];
            let st = &mut self.state[c];
            let target = times[c % 2];
            if st[0] < 1.0 {
                st[0] = target;
            }
            let mut phase = start;
            for x in ch.iter_mut() {
                let dry = *x * input;
                st[0] += (target - st[0]) * glide;
                let wobble = if mode == 2 {
                    1.0 + 0.01 * drift * (std::f64::consts::TAU * phase).sin() as f32
                } else {
                    1.0
                };
                phase = (phase + 0.7 / fs as f64).fract();
                let mut echo = line.read(st[0] * wobble);
                if mode >= 1 {
                    st[3] += (echo - st[3]) * band;
                    echo = st[3];
                }
                if mode == 2 {
                    // The compander: what was squeezed going in opens out
                    // (and the line's noise with it, pushed down).
                    echo = echo.signum() * echo.abs().min(1.0).powf(squeeze);
                }
                st[1] += (echo - st[1]) * damp;
                st[2] += (st[1] - st[2]) * cut;
                let back = st[1] - st[2];
                let mut into = dry + feedback * back;
                if mode >= 1 {
                    into = soft(into * 0.53) / 0.53;
                }
                if mode == 2 {
                    self.noise ^= self.noise << 13;
                    self.noise ^= self.noise >> 17;
                    self.noise ^= self.noise << 5;
                    into = into.signum() * into.abs().min(1.0).powf(squeeze.recip())
                        + noise * (self.noise as f32 / u32::MAX as f32 - 0.5);
                }
                line.write((into + FLUSH) - FLUSH);
                *x = (mix * echo + (1.0 - mix) * dry) * output;
            }
        }
        self.drift = (start + 0.7 * n as f64 / fs as f64).fract();
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

    fn peak(x: &[f32]) -> f32 {
        x.iter().fold(0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn the_compressor_squeezes_loud_sound() {
        let mut c = Compressor::new(48_000);
        c.set(&with(
            ek::PARAMS,
            &[(ek::THRESHOLD, -20.0), (ek::RATIO, 10.0)],
        ));
        let mut ch = vec![sine(200.0, 0.9, 48_000)];
        c.process(&mut ch);
        let out = 20.0 * peak(&ch[0][24_000..]).log10();
        assert!(out < -6.0 && out > -20.0, "{out}");
        // Mixed all dry, nothing changes.
        let mut c = Compressor::new(48_000);
        c.set(&with(ek::PARAMS, &[(ek::THRESHOLD, -20.0), (ek::MIX, 0.0)]));
        let x = sine(200.0, 0.5, 4_800);
        let mut ch = vec![x.clone()];
        c.process(&mut ch);
        assert!(ch[0].iter().zip(&x).all(|(a, b)| (a - b).abs() < 1e-5));
    }

    #[test]
    fn the_modulation_effects_stay_bounded_and_change_the_sound() {
        let x = sine(440.0, 0.5, 24_000);
        let mut fx: Vec<Box<dyn Own>> = vec![
            Box::new(Phaser::new(48_000)),
            Box::new(ChorusFlanger::new(48_000)),
            Box::new(StudioReverb::new(48_000)),
            Box::new(StereoDelay::new(48_000)),
        ];
        let specs = [ep::PARAMS, ec::PARAMS, er::PARAMS, es::PARAMS];
        for (f, s) in fx.iter_mut().zip(specs) {
            f.set(&with(s, &[]));
            let mut ch = vec![x.clone(), x.clone()];
            f.process(&mut ch);
            assert!(ch.iter().flatten().all(|v| v.is_finite() && v.abs() < 4.0));
            assert!(ch[0].iter().zip(&x).any(|(a, b)| (a - b).abs() > 1e-3));
        }
    }

    #[test]
    fn the_stereo_delay_echoes_at_its_time() {
        let mut d = StereoDelay::new(48_000);
        d.set(&with(
            es::PARAMS,
            &[
                (es::SYNC, 0.0),
                (es::LEFT, 100.0),
                (es::RIGHT, 100.0),
                (es::MIX, 1.0),
                (es::FEEDBACK, 0.0),
            ],
        ));
        let mut x = vec![vec![0.0f32; 9_600], vec![0.0f32; 9_600]];
        x[0][0] = 1.0;
        d.process(&mut x);
        let at = x[0]
            .iter()
            .enumerate()
            .fold(
                (0, 0f32),
                |(i, m), (k, s)| if s.abs() > m { (k, s.abs()) } else { (i, m) },
            )
            .0;
        assert!((at as i32 - 4_800).abs() <= 2, "{at}");
    }
}
