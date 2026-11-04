//! The Express effects, as VEGAS's: small versions of the full ones, most
//! mixed with the untouched sound by a "… / Original" control (see
//! [`Mixed`]).
//!
//! Express Delay: a comb, each echo the decay's share of the one before.
//! Express Distortion: past a knee the sound follows the slew's slope
//! (soft, or harder with hard clipping), the faintest sound is thinned
//! out, a short comb (half the distortion's tenths of a millisecond back)
//! colours it, and it may move only so fast from sample to sample.
//! Express Equalization: a low shelf at 300 Hz, a band at 400 Hz 2.5
//! octaves wide and a high shelf at 500 Hz (−25 dB takes each out).
//! Express Stutter: the sound on for its length and off for the silence's,
//! each sounding stretch faded in and out over an eighth of it; with Add
//! space the second channel runs half a round on. Express Amplitude
//! Modulation: the level follows a smooth wave, its bottom at 1 − depth.

use bv_timeline::audio_fx::{
    express_amplitude_modulation as xam, express_delay as xd, express_distortion as xdi,
    express_equalization as xeq, express_stutter as xs,
};

use super::eq::Section;
use super::{Own, Ramp};

/// Added and taken away again on fed-back state, so a fading signal ends
/// at 0 instead of lingering as a denormal number.
const FLUSH: f32 = 1e-18;

/// An effect mixed with the sound it was given: `mix` of what it makes,
/// the rest of the sound as it came. Its own values come from the
/// effect's through `map`.
#[derive(Debug)]
pub(crate) struct Mixed<T> {
    inner: T,
    map: fn(&[f32]) -> Vec<f32>,
    mix_at: usize,
    mix: Ramp,
    dry: Vec<Vec<f32>>,
}

impl<T: Own> Mixed<T> {
    pub(crate) fn new(inner: T, map: fn(&[f32]) -> Vec<f32>, mix_at: usize) -> Mixed<T> {
        Mixed {
            inner,
            map,
            mix_at,
            mix: Ramp::new(1.0),
            dry: Vec::new(),
        }
    }
}

impl<T: Own> Own for Mixed<T> {
    fn set(&mut self, params: &[f32]) {
        self.mix.set(
            params
                .get(self.mix_at)
                .copied()
                .unwrap_or(1.0)
                .clamp(0.0, 1.0),
        );
        self.inner.set(&(self.map)(params));
    }

    fn keep_time(&mut self, tempo: &bv_timeline::Tempo) {
        self.inner.keep_time(tempo);
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        self.dry.resize_with(channels.len(), Vec::new);
        for (d, ch) in self.dry.iter_mut().zip(channels.iter()) {
            d.clear();
            d.extend_from_slice(ch);
        }
        self.inner.process(channels);
        let (start, step) = self.mix.block(n);
        for (ch, dry) in channels.iter_mut().zip(self.dry.iter()) {
            for (i, (x, d)) in ch.iter_mut().zip(dry.iter()).enumerate() {
                let m = start + step * i as f32;
                *x = m * *x + (1.0 - m) * d;
            }
        }
    }
}

/// The values as they are (for an inner effect that reads the same ones).
pub(crate) fn same(params: &[f32]) -> Vec<f32> {
    params.to_vec()
}

/// A value of `params`, its default when missing, in its range.
fn value(params: &[f32], specs: &[bv_timeline::ParamSpec], i: usize) -> f32 {
    let s = &specs[i];
    params
        .get(i)
        .copied()
        .unwrap_or(s.default)
        .clamp(s.min, s.max)
}

/// Express Delay's echo alone.
#[derive(Debug)]
pub(crate) struct ExpressDelay {
    rate: f32,
    lines: Vec<Vec<f32>>,
    len: usize,
    at: usize,
    feedback: f32,
}

impl ExpressDelay {
    pub(crate) fn new(rate: u32) -> ExpressDelay {
        ExpressDelay {
            rate: rate.max(1) as f32,
            lines: Vec::new(),
            len: 1,
            at: 0,
            feedback: 0.0,
        }
    }
}

impl Own for ExpressDelay {
    fn set(&mut self, params: &[f32]) {
        let ms = value(params, xd::PARAMS, xd::DELAY);
        let len = ((ms * self.rate / 1000.0).round() as usize).max(1);
        if len != self.len {
            self.len = len;
            for l in &mut self.lines {
                l.clear();
                l.resize(len, 0.0);
            }
            self.at = 0;
        }
        self.feedback = value(params, xd::PARAMS, xd::DECAY);
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let len = self.len;
        self.lines.resize_with(channels.len(), || vec![0.0; len]);
        let g = self.feedback;
        let mut end = self.at;
        for (ch, line) in channels.iter_mut().zip(self.lines.iter_mut()) {
            let mut at = self.at;
            let mut i = 0;
            while i < n {
                // Up to the line's end, nothing read was written here.
                let m = (len - at).min(n - i);
                for (x, d) in ch[i..i + m].iter_mut().zip(line[at..at + m].iter_mut()) {
                    let echo = *d;
                    *d = (*x + g * echo + FLUSH) - FLUSH;
                    *x = echo;
                }
                i += m;
                at += m;
                if at == len {
                    at = 0;
                }
            }
            end = at;
        }
        self.at = end;
    }
}

/// Express Distortion's distorted sound alone.
#[derive(Debug, Default)]
pub(crate) struct ExpressDistortion {
    slope: f32,
    knee: f32,
    gain: f32,
    quiet: f32,
    back: usize,
    step: f32,
    /// Each channel's last 500 samples and its last output.
    lines: Vec<Vec<f32>>,
    at: usize,
    last: Vec<f32>,
}

impl Own for ExpressDistortion {
    fn set(&mut self, params: &[f32]) {
        let p = |i| value(params, xdi::PARAMS, i);
        let d = p(xdi::DISTORTION).min(0.999);
        let slew = p(xdi::SLEW);
        if p(xdi::HARD) >= 0.5 {
            self.slope = 1.0 - 0.95 * slew;
            self.knee = 0.5 * self.slope + 0.5;
            self.gain = 1.0;
        } else {
            // The slew's thousandths, 1 to 1000, on a log scale: 1 to 0.02.
            self.slope = 1.0 - 0.98 * (slew * 1000.0).max(1.0).log10() / 3.0;
            self.knee = self.slope;
            self.gain = 1.0 / ((1.0 - self.slope) * self.slope + self.slope);
        }
        self.quiet = (1.0 - d) * 0.01;
        self.back = ((p(xdi::DISTORTION) * 500.0).round() as usize).min(499);
        self.step = 1.0 - 0.9 * d;
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let count = channels.len();
        self.lines.resize_with(count, || vec![0.0; 500]);
        self.last.resize(count, 0.0);
        let at0 = self.at;
        for (c, ch) in channels.iter_mut().enumerate() {
            let line = &mut self.lines[c];
            let mut last = self.last[c];
            let mut at = at0;
            for x in ch.iter_mut() {
                let v = *x;
                let t = self.knee;
                let mut y = if v > t {
                    t + (v - t) * self.slope
                } else if v < -t {
                    -t + (v + t) * self.slope
                } else {
                    v
                };
                if v.abs() < self.quiet {
                    y *= v.abs() / 0.01;
                }
                y *= self.gain;
                line[at] = y;
                if self.back > 0 {
                    y = (y + line[(at + 500 - self.back) % 500]) / 2.0;
                }
                at = (at + 1) % 500;
                y = y.clamp(last - self.step, last + self.step);
                last = y;
                *x = y;
            }
            self.last[c] = last;
            if c + 1 == count {
                self.at = at;
            }
        }
    }
}

/// Express Equalization's equalized sound alone.
#[derive(Debug)]
pub(crate) struct ExpressEq {
    rate: f64,
    /// Each part (a filter and how much of it is added).
    parts: [(Section, f32); 3],
    memory: Vec<[[f32; 4]; 3]>,
}

impl ExpressEq {
    pub(crate) fn new(rate: u32) -> ExpressEq {
        let fs = rate.max(1) as f64;
        ExpressEq {
            rate: fs,
            parts: [
                (Section::pass(300.0, false, fs), 0.0),
                (Section::band(400.0, 2.5, fs), 0.0),
                (Section::pass(500.0, true, fs), 0.0),
            ],
            memory: Vec::new(),
        }
    }
}

impl Own for ExpressEq {
    fn set(&mut self, params: &[f32]) {
        let more = |i| {
            let db = value(params, xeq::PARAMS, i);
            if db <= -25.0 {
                -1.0
            } else {
                10f32.powf(db / 20.0) - 1.0
            }
        };
        let _ = self.rate;
        self.parts[0].1 = more(xeq::BASS);
        self.parts[1].1 = more(xeq::MID);
        self.parts[2].1 = more(xeq::TREBLE);
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        self.memory.resize(channels.len(), [[0.0; 4]; 3]);
        for (ch, memory) in channels.iter_mut().zip(self.memory.iter_mut()) {
            for ((s, more), m) in self.parts.iter().zip(memory.iter_mut()) {
                for x in ch.iter_mut() {
                    *x += more * s.run(*x, m);
                }
            }
        }
    }
}

/// Express Stutter's gated sound alone.
#[derive(Debug)]
pub(crate) struct ExpressStutter {
    rate: f32,
    sound: usize,
    silence: usize,
    space: bool,
    /// Where in the round the first channel is.
    at: usize,
}

impl ExpressStutter {
    pub(crate) fn new(rate: u32) -> ExpressStutter {
        ExpressStutter {
            rate: rate.max(1) as f32,
            sound: 1,
            silence: 1,
            space: false,
            at: 0,
        }
    }

    /// The gain `at` samples into a round: the sound faded in and out
    /// over an eighth of it, then silence.
    fn gain(&self, at: usize) -> f32 {
        if at >= self.sound {
            return 0.0;
        }
        let edge = (self.sound as f32 * 0.125).max(1.0);
        let from_end = (self.sound - at) as f32;
        (at as f32 / edge).min(from_end / edge).min(1.0)
    }
}

impl Own for ExpressStutter {
    fn set(&mut self, params: &[f32]) {
        let ms = |i| value(params, xs::PARAMS, i) * self.rate / 1000.0;
        self.sound = (ms(xs::SOUND).round() as usize).max(1);
        self.silence = (ms(xs::SILENCE).round() as usize).max(1);
        self.space = value(params, xs::PARAMS, xs::SPACE) >= 0.5;
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let round = self.sound + self.silence;
        let start = self.at % round;
        for (c, ch) in channels.iter_mut().enumerate() {
            let shift = if self.space && c % 2 == 1 {
                round / 2
            } else {
                0
            };
            for (i, x) in ch.iter_mut().enumerate() {
                *x *= self.gain((start + shift + i) % round);
            }
        }
        self.at = (start + n) % round;
    }
}

/// Express Amplitude Modulation's wave: a smooth one, as its presets
/// draw it (time 0–1000, level 0 at the top to −100 at the bottom).
const AM_WAVE: [(f32, f32); 12] = [
    (0.0, -50.0),
    (125.0, -12.0),
    (188.0, -3.0),
    (250.0, 0.0),
    (313.0, -3.0),
    (375.0, -12.0),
    (625.0, -88.0),
    (688.0, -97.0),
    (750.0, -100.0),
    (813.0, -97.0),
    (875.0, -88.0),
    (1000.0, -50.0),
];

/// Express Amplitude Modulation's modulated sound alone.
#[derive(Debug)]
pub(crate) struct ExpressAm {
    rate: f32,
    /// The wave's level over a round, 1001 steps, as gains.
    gains: Vec<f32>,
    step: f32,
    phase: f32,
}

impl ExpressAm {
    pub(crate) fn new(rate: u32) -> ExpressAm {
        ExpressAm {
            rate: rate.max(1) as f32,
            gains: vec![1.0; 1001],
            step: 0.0,
            phase: 0.0,
        }
    }
}

impl Own for ExpressAm {
    fn set(&mut self, params: &[f32]) {
        let depth = value(params, xam::PARAMS, xam::DEPTH);
        let hz = value(params, xam::PARAMS, xam::RATE);
        self.step = 1000.0 / (self.rate / hz).max(1.0);
        for (t, g) in self.gains.iter_mut().enumerate() {
            let t = t as f32;
            let k = AM_WAVE
                .partition_point(|p| p.0 <= t)
                .clamp(1, AM_WAVE.len() - 1);
            let ((t0, v0), (t1, v1)) = (AM_WAVE[k - 1], AM_WAVE[k]);
            let v = v0 + (v1 - v0) * ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
            // A linear depth: the bottom of the wave at 1 − depth.
            *g = (1.0 - depth).max(0.0).powf(-v / 100.0);
        }
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let start = self.phase;
        for ch in channels.iter_mut() {
            let mut phase = start;
            for x in ch.iter_mut() {
                *x *= self.gains[(phase as usize).min(1000)];
                phase += self.step;
                if phase >= 1000.0 {
                    phase -= 1000.0;
                }
            }
        }
        self.phase = (start + self.step * n as f32) % 1000.0;
    }
}

/// An effect that reads its values through `map` (the Express versions
/// of the full effects, their other values fixed).
#[derive(Debug)]
pub(crate) struct Mapped<T> {
    inner: T,
    map: fn(&[f32]) -> Vec<f32>,
}

impl<T: Own> Mapped<T> {
    pub(crate) fn new(inner: T, map: fn(&[f32]) -> Vec<f32>) -> Mapped<T> {
        Mapped { inner, map }
    }
}

impl<T: Own> Own for Mapped<T> {
    fn set(&mut self, params: &[f32]) {
        self.inner.set(&(self.map)(params));
    }

    fn keep_time(&mut self, tempo: &bv_timeline::Tempo) {
        self.inner.keep_time(tempo);
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        self.inner.process(channels);
    }

    fn latency(&self) -> usize {
        self.inner.latency()
    }
}

/// Express Chorus as the full Chorus: 0 dB in, dry and chorus, no
/// feedback nor cut, a 20 ms delay.
pub(crate) fn chorus_values(params: &[f32]) -> Vec<f32> {
    use bv_timeline::audio_fx::{chorus as co, express_chorus as xc};
    let p = |i| value(params, xc::PARAMS, i);
    let mut v: Vec<f32> = co::PARAMS.iter().map(|s| s.default).collect();
    v[co::INPUT] = 0.0;
    v[co::DRY] = 0.0;
    v[co::WET] = 0.0;
    v[co::INVERT] = 0.0;
    v[co::INVERT_FEEDBACK] = 0.0;
    v[co::SIZE] = p(xc::SIZE);
    v[co::RATE] = p(xc::RATE);
    v[co::DEPTH] = p(xc::DEPTH);
    v[co::FEEDBACK] = 0.0;
    v[co::DELAY] = 20.0;
    v[co::ATTENUATE] = 0.0;
    v[co::SYNC] = 0.0;
    v
}

/// Express Flange/Wah-Wah as the full one: wet only (the mix does the
/// rest), the wah's centre at 500 Hz and its resonance at 75 %.
pub(crate) fn flange_values(params: &[f32]) -> Vec<f32> {
    use bv_timeline::audio_fx::{express_flange_wah_wah as xf, flange_wah_wah as fw};
    let p = |i| value(params, xf::PARAMS, i);
    let mut v: Vec<f32> = fw::PARAMS.iter().map(|s| s.default).collect();
    v[fw::EFFECT] = if p(xf::EFFECT) >= 0.5 { 2.0 } else { 0.0 };
    v[fw::DRY] = -80.0;
    v[fw::WET] = 0.0;
    v[fw::RATE] = p(xf::RATE);
    v[fw::DEPTH] = p(xf::DEPTH);
    v[fw::CENTER] = 500.0;
    v[fw::RESONANCE] = 0.75;
    v[fw::SYNC] = 0.0;
    v
}

/// Express Dynamics as the Track Compressor: no gains, its top ratio a
/// limiter.
pub(crate) fn dynamics_values(params: &[f32]) -> Vec<f32> {
    use bv_timeline::audio_fx::{express_dynamics as xy, track_compressor as tc};
    let p = |i| value(params, xy::PARAMS, i);
    let mut v: Vec<f32> = tc::PARAMS.iter().map(|s| s.default).collect();
    v[tc::INPUT] = 0.0;
    v[tc::OUTPUT] = 0.0;
    v[tc::THRESHOLD] = p(xy::THRESHOLD);
    let ratio = p(xy::RATIO);
    v[tc::RATIO] = if ratio >= 50.0 { 500.0 } else { ratio };
    v[tc::ATTACK] = p(xy::ATTACK);
    v[tc::RELEASE] = p(xy::RELEASE);
    v[tc::AUTO_GAIN] = p(xy::AUTO_GAIN);
    v[tc::SMOOTH] = 0.0;
    v
}

/// Express Reverb as the full Reverb: its room, the size setting the
/// decay (0.5 to 5 s) and the liveliness the pre-delay (0 to 200 ms),
/// the reverb at −24 dB and the early reflections at −16 dB.
pub(crate) fn reverb_values(params: &[f32]) -> Vec<f32> {
    use bv_timeline::audio_fx::{express_reverb as xr, reverb as rv};
    let p = |i| value(params, xr::PARAMS, i);
    let mut v: Vec<f32> = rv::PARAMS.iter().map(|s| s.default).collect();
    v[rv::MODE] = p(xr::ROOM);
    v[rv::DRY] = -80.0;
    v[rv::WET] = -24.0;
    v[rv::EARLY] = -16.0;
    v[rv::STYLE] = 2.0;
    v[rv::DECAY] = 0.5 + 4.5 * p(xr::SIZE);
    v[rv::PREDELAY] = 200.0 * p(xr::LIVELINESS);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::audio_fx::express_noise_gate as xg;

    fn with(specs: &[bv_timeline::ParamSpec], values: &[(usize, f32)]) -> Vec<f32> {
        let mut p: Vec<f32> = specs.iter().map(|s| s.default).collect();
        for &(i, v) in values {
            p[i] = v;
        }
        p
    }

    #[test]
    fn the_mix_shares_the_sound_and_the_echo() {
        let mut d = Mixed::new(ExpressDelay::new(48_000), same, xd::MIX);
        d.set(&with(xd::PARAMS, &[(xd::DELAY, 10.0), (xd::MIX, 0.5)]));
        let mut x = vec![vec![0.0f32; 2_000]];
        x[0][0] = 1.0;
        d.process(&mut x);
        assert!((x[0][0] - 0.5).abs() < 1e-6);
        assert!((x[0][480] - 0.5).abs() < 1e-6, "{}", x[0][480]);
        // Fed back by the decay: each echo that share of the one before.
        let mut d = Mixed::new(ExpressDelay::new(48_000), same, xd::MIX);
        d.set(&with(
            xd::PARAMS,
            &[(xd::DELAY, 10.0), (xd::DECAY, 0.5), (xd::MIX, 1.0)],
        ));
        let mut x = vec![vec![0.0f32; 2_000]];
        x[0][0] = 1.0;
        d.process(&mut x);
        assert!((x[0][960] / x[0][480] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_stutter_switches_the_sound_off_and_on() {
        let mut s = Mixed::new(ExpressStutter::new(48_000), same, xs::MIX);
        s.set(&with(
            xs::PARAMS,
            &[(xs::SOUND, 10.0), (xs::SILENCE, 10.0), (xs::MIX, 1.0)],
        ));
        let mut x = vec![vec![1.0f32; 1_920]];
        s.process(&mut x);
        // Sounding in the middle of the first 10 ms, silent in the next.
        assert!((x[0][240] - 1.0).abs() < 1e-6);
        assert!(x[0][480..960].iter().all(|v| *v == 0.0));
        assert!((x[0][960 + 240] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn amplitude_modulation_follows_its_wave() {
        let mut m = Mixed::new(ExpressAm::new(48_000), same, xam::MIX);
        m.set(&with(
            xam::PARAMS,
            &[(xam::RATE, 1.0), (xam::DEPTH, 0.5), (xam::MIX, 1.0)],
        ));
        let mut x = vec![vec![1.0f32; 48_000]];
        m.process(&mut x);
        // The top of the wave a quarter round in, its bottom (1 − depth)
        // three quarters in.
        assert!((x[0][12_000] - 1.0).abs() < 1e-3, "{}", x[0][12_000]);
        assert!((x[0][36_000] - 0.5).abs() < 1e-3, "{}", x[0][36_000]);
    }

    #[test]
    fn express_eq_at_0_db_and_cutting_the_bass() {
        let level = |values: &[(usize, f32)], hz: f32| {
            let mut e = Mixed::new(ExpressEq::new(48_000), same, xeq::MIX);
            e.set(&with(xeq::PARAMS, values));
            let mut x = vec![
                (0..48_000)
                    .map(|i| (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin())
                    .collect::<Vec<f32>>(),
            ];
            e.process(&mut x);
            let tail = &x[0][24_000..];
            10.0 * (2.0 * tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32).log10()
        };
        assert!(level(&[], 1000.0).abs() < 0.05);
        // Taking the low-pass away leaves a gentle slope: well down by 30 Hz.
        assert!(level(&[(xeq::BASS, -25.0)], 30.0) < -12.0);
        assert!(level(&[(xeq::BASS, -25.0)], 8000.0).abs() < 0.5);
    }

    #[test]
    fn express_noise_gate_reads_the_noise_gate_s_values() {
        assert_eq!(xg::PARAMS.len(), 3);
        let mut g = super::super::dynamics::Gate::new(48_000);
        g.set(&with(xg::PARAMS, &[]));
        let mut x = vec![vec![0.01f32; 4_800]];
        g.process(&mut x);
        assert!(x[0].iter().all(|v| v.abs() < 1e-6));
    }
}
