//! Chorus and Flange/Wah-wah, as VEGAS's.
//!
//! Chorus: one to three stages in a row, each a delay line read at a
//! sine-modulated time (its own rate: 1, 0.8117 and 1.1323 times the
//! period), optionally darkened by a one-pole low-pass, fed back, and
//! added to what goes through. The first stage takes the sound at the
//! input gain and keeps it at the dry level; in stereo its copy comes at
//! 0.81 (left) and 0.855 (right) of the chorus level squared, the later
//! stages' at 0.9 and 0.95 of it; in mono 0.9, 0.81 and 0.729 of it.
//!
//! Flange: a delay swinging around 1 ms by the depth, fed back by half,
//! its copy at 0.9 of the wet level (on two channels or more, of its
//! square, times each channel's own factor). Phaser and wah-wah: a
//! second-order all-pass whose frequency sweeps up and down in a straight
//! line (in Hz) over the depth's range of octaves, its resonance a Q; the
//! phaser averages it with the sound, the wah-wah takes it from the sound
//! (a band-pass).

use bv_timeline::Tempo;
use bv_timeline::audio_fx::{chorus as co, flange_wah_wah as fw, tempo};

use super::{Own, db_gain};

/// Added and taken away again on fed-back state, so a fading signal ends
/// at 0 instead of lingering as a denormal number.
const FLUSH: f32 = 1e-18;

/// A delay line read between its samples (by straight lines).
#[derive(Clone, Debug)]
struct Line {
    buf: Vec<f32>,
    mask: usize,
    /// Where the newest sample is.
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

    /// The sound `d` samples back (1 is the newest), between samples.
    #[inline]
    fn read(&self, d: f32) -> f32 {
        let whole = d.floor();
        let part = d - whole;
        let i = whole as usize;
        let a = self.buf[self.at.wrapping_sub(i - 1) & self.mask];
        let b = self.buf[self.at.wrapping_sub(i) & self.mask];
        a + (b - a) * part
    }

    #[inline]
    fn write(&mut self, x: f32) {
        self.at = (self.at + 1) & self.mask;
        self.buf[self.at] = x;
    }
}

/// Each chorus stage's period, a part of the modulation's.
const STAGE_PERIODS: [f64; 3] = [1.0, 0.8117, 1.1323];

/// A value of `params`, its default when missing.
fn param(params: &[f32], specs: &[bv_timeline::ParamSpec], i: usize) -> f32 {
    params
        .get(i)
        .copied()
        .unwrap_or(specs[i].default)
        .clamp(specs[i].min, specs[i].max)
}

/// A modulation's period in samples: `1 / rate`, or with tempo sync its
/// note times the multiplier.
fn period(rate: f64, sync: bool, note: f32, multiplier: f32, tempo: &Tempo, fs: f64) -> f64 {
    let seconds = if sync {
        tempo::seconds(note, tempo) * multiplier as f64
    } else {
        1.0 / rate.max(1e-6)
    };
    (seconds * fs).max(1.0)
}

#[derive(Debug)]
pub(crate) struct Chorus {
    rate: f64,
    params: Vec<f32>,
    tempo: Tempo,
    /// Each stage's line a channel.
    lines: Vec<Vec<Line>>,
    /// Each stage's low-pass memory a channel.
    lows: Vec<Vec<f32>>,
    /// Each stage's place in its modulation, 0–1.
    phases: [f64; 3],
    /// How many stages ran last.
    used: usize,
    /// One block's delays (samples), a stage.
    delays: Vec<f32>,
}

impl Chorus {
    pub(crate) fn new(rate: u32) -> Chorus {
        Chorus {
            rate: rate.max(1) as f64,
            params: Vec::new(),
            tempo: Tempo::default(),
            lines: Vec::new(),
            lows: Vec::new(),
            phases: [0.0; 3],
            used: 0,
            delays: Vec::new(),
        }
    }

    fn p(&self, i: usize) -> f32 {
        param(&self.params, co::PARAMS, i)
    }
}

impl Own for Chorus {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
    }

    fn keep_time(&mut self, tempo: &Tempo) {
        self.tempo = *tempo;
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let fs = self.rate;
        let count = channels.len();
        // Up to twice the longest delay, with room to read between samples.
        let room = (0.2 * fs) as usize + 4;
        if self.lines.len() != 3 || self.lines[0].len() != count {
            self.lines = vec![vec![Line::new(room); count]; 3];
            self.lows = vec![vec![0.0; count]; 3];
        }
        let stages = self.p(co::SIZE).round().clamp(1.0, 3.0) as usize;
        let input = db_gain(self.p(co::INPUT), -60.0);
        let dry = db_gain(self.p(co::DRY), -60.0);
        let mut wet = db_gain(self.p(co::WET), -60.0);
        if self.p(co::INVERT) >= 0.5 {
            wet = -wet;
        }
        let mut feedback = self.p(co::FEEDBACK);
        if self.p(co::INVERT_FEEDBACK) >= 0.5 {
            feedback = -feedback;
        }
        let base = self.p(co::DELAY) as f64 * fs / 1000.0;
        let swing = self.p(co::DEPTH) as f64 * base;
        let cutoff = self.p(co::CUTOFF) as f64;
        let low_pass = (self.p(co::ATTENUATE) >= 0.5 && cutoff < fs / 2.0)
            .then(|| (-std::f64::consts::TAU * cutoff / fs).exp() as f32);
        let samples = period(
            self.p(co::RATE) as f64,
            self.p(co::SYNC) >= 0.5,
            self.p(co::PERIOD),
            self.p(co::MULTIPLIER),
            &self.tempo,
            fs,
        );
        for (k, factor) in STAGE_PERIODS.iter().enumerate().take(stages) {
            // This stage's delays over the block, the same on every channel.
            let step = 1.0 / (samples * factor);
            let mut phase = self.phases[k];
            self.delays.clear();
            for _ in 0..n {
                let d = base + swing * (std::f64::consts::TAU * phase).sin();
                self.delays.push(d.max(1.0) as f32);
                phase = (phase + step).fract();
            }
            self.phases[k] = phase;
            for (c, ch) in channels.iter_mut().enumerate() {
                let level = match (count, k) {
                    (1, k) => 0.9f32.powi(k as i32 + 1) * wet,
                    (_, 0) => [0.81, 0.855][c % 2] * wet * wet,
                    _ => [0.9, 0.95][c % 2] * wet,
                };
                let line = &mut self.lines[k][c];
                let low = &mut self.lows[k][c];
                for (x, d) in ch.iter_mut().zip(self.delays.iter()) {
                    let into = if k == 0 { *x * input } else { *x };
                    let late = line.read(*d);
                    let y = match low_pass {
                        Some(p) => {
                            *low = (1.0 - p) * late + p * *low;
                            *low
                        }
                        None => late,
                    };
                    line.write((into + feedback * y + FLUSH) - FLUSH);
                    *x = if k == 0 { dry * into } else { into } + level * y;
                }
            }
        }
        // Stages left out start again silent when they come back.
        for k in stages..self.used {
            self.lines[k] = vec![Line::new(room); count];
            self.lows[k] = vec![0.0; count];
        }
        self.used = stages;
    }
}

/// The flange's wet level on each channel, two channels or more.
const FLANGE_CHANNELS: [f32; 16] = [
    0.9, 0.95, 0.7, 0.9, 0.8, 1.0, 1.0, 0.7, 0.7, 1.0, 0.8, 1.0, 1.0, 0.8, 0.9, 0.7,
];

#[derive(Debug)]
pub(crate) struct FlangeWahWah {
    rate: f64,
    params: Vec<f32>,
    tempo: Tempo,
    lines: Vec<Line>,
    /// Each channel's all-pass memories.
    state: Vec<[f32; 2]>,
    /// The place in the modulation (or the sweep's round trip), 0–1.
    phase: f64,
    /// One block's delays, or the all-pass's two coefficients.
    a: Vec<f32>,
    b: Vec<f32>,
    /// Express Flange/Wah-Wah's flange levels: 0.9 and 0.95 of the wet
    /// level on the left and right (not of its square).
    express: bool,
}

impl FlangeWahWah {
    pub(crate) fn new(rate: u32) -> FlangeWahWah {
        FlangeWahWah {
            rate: rate.max(1) as f64,
            params: Vec::new(),
            tempo: Tempo::default(),
            lines: Vec::new(),
            state: Vec::new(),
            phase: 0.0,
            a: Vec::new(),
            b: Vec::new(),
            express: false,
        }
    }

    pub(crate) fn express(rate: u32) -> FlangeWahWah {
        FlangeWahWah {
            express: true,
            ..FlangeWahWah::new(rate)
        }
    }

    fn p(&self, i: usize) -> f32 {
        param(&self.params, fw::PARAMS, i)
    }
}

impl Own for FlangeWahWah {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
    }

    fn keep_time(&mut self, tempo: &Tempo) {
        self.tempo = *tempo;
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let fs = self.rate;
        let count = channels.len();
        let dry = db_gain(self.p(fw::DRY), -80.0);
        let wet = db_gain(self.p(fw::WET), -80.0);
        let depth = self.p(fw::DEPTH) as f64;
        let step = 1.0
            / period(
                self.p(fw::RATE) as f64,
                self.p(fw::SYNC) >= 0.5,
                self.p(fw::PERIOD),
                self.p(fw::MULTIPLIER),
                &self.tempo,
                fs,
            );
        let mut phase = self.phase;
        self.a.clear();
        self.b.clear();
        let effect = self.p(fw::EFFECT).round() as i32;
        if effect == 0 {
            // Flange: about 1 ms, swinging by the depth.
            if self.lines.len() != count {
                self.lines = vec![Line::new((0.005 * fs) as usize + 4); count];
            }
            let base = fs / 1000.0;
            for _ in 0..n {
                let d = base + depth * base * (std::f64::consts::TAU * phase).sin();
                self.a.push(d.max(1.0) as f32);
                phase = (phase + step).fract();
            }
            for (c, ch) in channels.iter_mut().enumerate() {
                let level = if count == 1 {
                    0.9 * wet
                } else if self.express {
                    [0.9, 0.95][c % 2] * wet
                } else {
                    0.9 * wet * wet * FLANGE_CHANNELS[c % 16]
                };
                let line = &mut self.lines[c];
                for (x, d) in ch.iter_mut().zip(self.a.iter()) {
                    let late = line.read(*d);
                    line.write((*x + 0.5 * late + FLUSH) - FLUSH);
                    *x = dry * *x + level * late;
                }
            }
        } else {
            // Phaser and wah-wah: the sweep's range, `k` either side of the
            // centre (an octave at 50 %), and its Q.
            let pct = depth * 100.0;
            let mut spread = 1.0 + 0.002 * (pct - 50.0).powi(2);
            if pct > 50.0 {
                spread = 1.0 / spread;
            }
            let k = 2f64.powf(1.0 / spread);
            let res = self.p(fw::RESONANCE) as f64 * 100.0;
            let mut q = 1.0 + 0.008 * (res - 50.0).powi(2);
            if res < 50.0 {
                q = 1.0 / q;
            }
            let centre = self.p(fw::CENTER) as f64 / fs;
            let (lo, hi) = (centre / k, (centre * k).min(0.5));
            for _ in 0..n {
                // Up and back in a straight line, once a period.
                let there = 1.0 - (2.0 * phase - 1.0).abs();
                let f = lo + (hi - lo) * there;
                let r = (-std::f64::consts::PI * f / q).exp();
                self.a
                    .push((2.0 * r * (std::f64::consts::TAU * f).cos()) as f32);
                self.b.push((r * r) as f32);
                phase = (phase + step).fract();
            }
            self.state.resize(count, [0.0; 2]);
            let phaser = effect == 1;
            for (ch, st) in channels.iter_mut().zip(self.state.iter_mut()) {
                let [mut v1, mut v2] = *st;
                for ((x, a1), a2) in ch.iter_mut().zip(self.a.iter()).zip(self.b.iter()) {
                    let mut v = *x + a1 * v1 - a2 * v2;
                    if v.abs() < 2e-11 {
                        v = 0.0;
                    }
                    let all = a2 * v - a1 * v1 + v2;
                    v2 = v1;
                    v1 = v;
                    let e = if phaser { (*x + all) * 0.5 } else { *x - all };
                    *x = dry * *x + wet * e;
                }
                *st = [v1, v2];
            }
        }
        self.phase = phase;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chorus(values: &[(usize, f32)]) -> Chorus {
        let mut c = Chorus::new(48_000);
        let mut p: Vec<f32> = co::PARAMS.iter().map(|s| s.default).collect();
        for &(i, v) in values {
            p[i] = v;
        }
        c.set(&p);
        c
    }

    fn flange(values: &[(usize, f32)]) -> FlangeWahWah {
        let mut f = FlangeWahWah::new(48_000);
        let mut p: Vec<f32> = fw::PARAMS.iter().map(|s| s.default).collect();
        for &(i, v) in values {
            p[i] = v;
        }
        f.set(&p);
        f
    }

    fn sine(hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin())
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn a_chorus_click_comes_back_after_its_delay() {
        // One stage, no depth to speak of, 10 ms, mono: the dry click, then
        // its copy 480 samples on at 0.9 of the chorus level.
        let mut c = chorus(&[
            (co::SIZE, 1.0),
            (co::DEPTH, 0.01),
            (co::DELAY, 10.0),
            (co::DRY, 0.0),
            (co::WET, 0.0),
        ]);
        let mut x = vec![vec![0.0f32; 2000]];
        x[0][0] = 1.0;
        c.process(&mut x);
        assert!((x[0][0] - 1.0).abs() < 1e-6);
        let copy: f32 = x[0][470..490].iter().sum();
        assert!((copy - 0.9).abs() < 0.02, "{copy}");
        assert!(x[0][1..460].iter().all(|s| s.abs() < 1e-6));
    }

    #[test]
    fn stereo_first_stage_takes_the_chorus_level_squared() {
        let mut c = chorus(&[
            (co::SIZE, 1.0),
            (co::DEPTH, 0.01),
            (co::DELAY, 10.0),
            (co::DRY, -60.0),
            (co::WET, -6.0),
        ]);
        let mut x = vec![vec![0.0f32; 2000], vec![0.0f32; 2000]];
        x[0][0] = 1.0;
        x[1][0] = 1.0;
        c.process(&mut x);
        let w = 10f32.powf(-6.0 / 20.0);
        let left: f32 = x[0][470..490].iter().sum();
        let right: f32 = x[1][470..490].iter().sum();
        assert!((left - 0.81 * w * w).abs() < 0.01, "{left}");
        assert!((right - 0.855 * w * w).abs() < 0.01, "{right}");
    }

    #[test]
    fn the_flange_s_dry_and_wet_levels() {
        // Dry only: the sound as it was.
        let mut f = flange(&[(fw::DRY, 0.0), (fw::WET, -80.0)]);
        let x = sine(440.0, 4800);
        let mut ch = vec![x.clone()];
        f.process(&mut ch);
        assert!(ch[0].iter().zip(&x).all(|(a, b)| (a - b).abs() < 1e-6));
        // Wet only, a low tone: the 1 ms copy fed back by half, at 0.9 —
        // about 0.9 / (1 − 0.5) = 1.8 times the tone.
        let mut f = flange(&[(fw::DRY, -80.0), (fw::WET, 0.0), (fw::DEPTH, 0.01)]);
        let mut ch = vec![sine(20.0, 48_000)];
        f.process(&mut ch);
        let level = rms(&ch[0][24_000..]) / rms(&sine(20.0, 24_000));
        assert!((level - 1.8).abs() < 0.05, "{level}");
    }

    #[test]
    fn the_wah_wah_is_a_band_pass_on_its_sweep() {
        // Depth 1 %: a sweep of almost nothing around 1 kHz, Q 1.
        let wah = |hz: f32| {
            let mut f = flange(&[
                (fw::EFFECT, 2.0),
                (fw::DRY, -80.0),
                (fw::WET, 0.0),
                (fw::DEPTH, 0.01),
                (fw::CENTER, 1000.0),
                (fw::RESONANCE, 0.5),
                (fw::RATE, 0.001),
            ]);
            let mut ch = vec![sine(hz, 48_000)];
            f.process(&mut ch);
            rms(&ch[0][24_000..]) / rms(&sine(hz, 24_000))
        };
        let (at, low, high) = (wah(1000.0), wah(100.0), wah(10_000.0));
        assert!(at > 1.5, "{at}");
        assert!(low < 0.4 && high < 0.4, "{low} {high}");
    }

    #[test]
    fn the_phaser_notches_where_the_all_pass_turns_half_round() {
        // A still sweep at 1 kHz: its all-pass turns 180° there, so the
        // phaser's average cancels.
        let phaser = |hz: f32| {
            let mut f = flange(&[
                (fw::EFFECT, 1.0),
                (fw::DRY, -80.0),
                (fw::WET, 0.0),
                (fw::DEPTH, 0.01),
                (fw::CENTER, 1000.0),
                (fw::RESONANCE, 0.5),
                (fw::RATE, 0.001),
            ]);
            let mut ch = vec![sine(hz, 48_000)];
            f.process(&mut ch);
            rms(&ch[0][24_000..]) / rms(&sine(hz, 24_000))
        };
        assert!(phaser(1000.0) < 0.2, "{}", phaser(1000.0));
        assert!(phaser(50.0) > 0.9);
    }
}
