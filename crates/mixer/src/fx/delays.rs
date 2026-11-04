//! Simple Delay and Multi-Tap Delay, as VEGAS's.
//!
//! Simple Delay: a line a channel as long as the delay. What
//! leaves the line is the echo; with Multiple delays it also goes back in,
//! scaled so the echoes fall 30 dB in the decay time (a comb filter).
//! Changing the delay's length starts the lines again, silent.

use bv_timeline::Tempo;
use bv_timeline::audio_fx::{multi_tap_delay as mt, simple_delay as sd, tempo};

use super::{Own, Ramp, db_gain};

/// Added and taken away again where a fed-back signal fades, so it ends
/// at 0 instead of lingering as a denormal number (slow on processors).
const FLUSH: f32 = 1e-18;

#[derive(Debug)]
pub(crate) struct SimpleDelay {
    rate: f64,
    params: Vec<f32>,
    tempo: Tempo,
    dry: Ramp,
    wet: Ramp,
    /// What goes back into the line each time round (0: one echo).
    feedback: f32,
    /// A line a channel, each `len` long.
    lines: Vec<Vec<f32>>,
    len: usize,
    /// Where the lines are read and written.
    at: usize,
}

impl SimpleDelay {
    pub(crate) fn new(rate: u32) -> SimpleDelay {
        SimpleDelay {
            rate: rate.max(1) as f64,
            params: Vec::new(),
            tempo: Tempo::default(),
            dry: Ramp::new(0.5),
            wet: Ramp::new(0.5),
            feedback: 0.0,
            lines: Vec::new(),
            len: 2,
            at: 0,
        }
    }

    fn param(&self, i: usize) -> f32 {
        self.params
            .get(i)
            .copied()
            .unwrap_or_else(|| sd::PARAMS[i].default)
    }

    /// A time in seconds, or a note of the tempo times a multiplier.
    fn seconds(&self, time: usize, sync: usize, note: usize, multiplier: usize) -> f64 {
        let spec = &sd::PARAMS;
        if self.param(sync) >= 0.5 {
            let m = &spec[multiplier];
            tempo::seconds(self.param(note), &self.tempo)
                * self.param(multiplier).clamp(m.min, m.max) as f64
        } else {
            let t = &spec[time];
            self.param(time).clamp(t.min, t.max) as f64
        }
    }

    fn update(&mut self) {
        let delay = self.seconds(sd::TIME, sd::SYNC, sd::NOTE, sd::MULTIPLIER);
        let len = ((delay * self.rate).round() as usize).max(2);
        let decay = self.seconds(
            sd::DECAY,
            sd::DECAY_SYNC,
            sd::DECAY_NOTE,
            sd::DECAY_MULTIPLIER,
        );
        // How many times round the line the decay time lasts.
        let rounds = decay * self.rate / len as f64;
        self.feedback = if self.param(sd::MULTIPLE) >= 0.5 && rounds >= 0.1 {
            // −30 dB after the decay time.
            10f64.powf(-1.5 / rounds) as f32
        } else {
            0.0
        };
        if len != self.len {
            self.len = len;
            for line in &mut self.lines {
                line.clear();
                line.resize(len, 0.0);
            }
            self.at = 0;
        }
        self.dry.set(db_gain(self.param(sd::DRY), -80.0));
        self.wet.set(db_gain(self.param(sd::WET), -80.0));
    }
}

impl Own for SimpleDelay {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
        self.update();
    }

    fn keep_time(&mut self, tempo: &Tempo) {
        self.tempo = *tempo;
        self.update();
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let (dry, dry_step) = self.dry.block(n);
        let (wet, wet_step) = self.wet.block(n);
        let len = self.len;
        if self.lines.len() != channels.len() {
            self.lines.resize_with(channels.len(), || vec![0.0; len]);
        }
        let g = self.feedback;
        let mut end = self.at;
        for (ch, line) in channels.iter_mut().zip(self.lines.iter_mut()) {
            let mut at = self.at;
            let mut i = 0;
            while i < n {
                // Up to the line's end: nothing read here was written in
                // this stretch, so its samples go together (vectorized).
                let m = (len - at).min(n - i);
                let base = i as f32;
                let samples = ch[i..i + m].iter_mut().zip(line[at..at + m].iter_mut());
                for (k, (x, d)) in samples.enumerate() {
                    let t = base + k as f32;
                    let echo = *d;
                    *d = (*x + g * echo + FLUSH) - FLUSH;
                    *x = (dry + dry_step * t) * *x + (wet + wet_step * t) * echo;
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

/// Each tap's fixed nudge (ms) and share of the modulation's swing.
const TAP_NUDGE: [f32; 8] = [0.123, 0.377, -0.411, 0.095, -0.301, -0.253, 0.457, 0.179];
const TAP_SWING: [f32; 8] = [0.7, 0.9, 0.4, 0.6, 0.8, 0.9, 0.1, 0.9];

/// The line's room: the longest tap and its nudge, a little more.
const MULTI_TAP_SECONDS: f64 = 2.6;

/// Multi-Tap Delay, as VEGAS's: the channels at the input gain (darkened
/// by the low-pass when on) go, averaged, into one line, with the taps'
/// sums fed back (the feedback over the taps' total level when that is
/// more than 1, so the loop never runs away). Each tap reads it at its
/// time, a little nudged, shortened by a sine swing (one for all taps,
/// each its own share of it), and plays at its gain on its side of the
/// stereo picture; the taps' sums go out through the low-pass too.
#[derive(Debug)]
pub(crate) struct MultiTapDelay {
    rate: f32,
    params: Vec<f32>,
    line: Vec<f32>,
    mask: usize,
    at: usize,
    phase: f64,
    /// The input's and the two sums' low-pass memories.
    lows: [f32; 4],
}

impl MultiTapDelay {
    pub(crate) fn new(rate: u32) -> MultiTapDelay {
        let fs = rate.max(1) as f64;
        let n = ((MULTI_TAP_SECONDS * fs) as usize).next_power_of_two();
        MultiTapDelay {
            rate: fs as f32,
            params: Vec::new(),
            line: vec![0.0; n],
            mask: n - 1,
            at: 0,
            phase: 0.0,
            lows: [0.0; 4],
        }
    }

    fn p(&self, i: usize) -> f32 {
        let s = &mt::PARAMS[i];
        self.params
            .get(i)
            .copied()
            .unwrap_or(s.default)
            .clamp(s.min, s.max)
    }

    /// The line `d` samples back (1 the newest), between samples.
    fn read(&self, d: f32) -> f32 {
        let d = d.max(1.0);
        let whole = d.floor();
        let part = d - whole;
        let i = whole as usize;
        let a = self.line[(self.at + self.line.len() - (i - 1)) & self.mask];
        let b = self.line[(self.at + self.line.len() - i) & self.mask];
        a + (b - a) * part
    }
}

impl Own for MultiTapDelay {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let fs = self.rate;
        let input = db_gain(self.p(mt::INPUT), -80.0);
        let dry = db_gain(self.p(mt::DRY), -80.0);
        let wet = db_gain(self.p(mt::WET), -80.0);
        let p = if self.p(mt::LOW_PASS) >= 0.5 {
            (-std::f32::consts::TAU * self.p(mt::LOW_FREQ) / fs).exp()
        } else {
            0.0
        };
        let count = self.p(mt::TAPS).round() as usize;
        let depth = self.p(mt::DEPTH) * 3e-5 * fs;
        let step = std::f64::consts::TAU * self.p(mt::RATE) as f64 / fs as f64;
        // Each tap: its delay (samples), swing, and levels to the left and
        // right (its pan, −1 to 1, is positive to the left).
        let taps: Vec<(f32, f32, f32, f32)> = (0..count)
            .map(|k| {
                let at = mt::TAP + k * mt::PER_TAP;
                let g = self.p(at + mt::GAIN);
                let pan = -self.p(at + mt::PAN);
                let (left, right) = if pan <= 0.0 {
                    (g, (pan + 1.0) * g)
                } else {
                    ((1.0 - pan) * g, g)
                };
                let delay = (self.p(at + mt::DELAY) + TAP_NUDGE[k]) * fs / 1000.0;
                (delay, depth * TAP_SWING[k], left, right)
            })
            .collect();
        let total: f32 = taps.iter().map(|t| t.2.abs().max(t.3.abs())).sum();
        let feedback = self.p(mt::FEEDBACK) / total.max(1.0);
        let stereo = channels.len() >= 2;
        let [mut low_l, mut low_r, mut out_l, mut out_r] = self.lows;
        for i in 0..n {
            let swing = (self.phase.sin() + 1.0) as f32;
            self.phase = (self.phase + step) % std::f64::consts::TAU;
            let (mut sum_l, mut sum_r) = (0.0, 0.0);
            for &(delay, depth, left, right) in &taps {
                let tap = self.read(delay - swing * depth);
                sum_l += left * tap;
                sum_r += right * tap;
            }
            let x_l = channels[0][i];
            let x_r = if stereo { channels[1][i] } else { x_l };
            low_l = (1.0 - p) * input * x_l + p * low_l;
            low_r = (1.0 - p) * input * x_r + p * low_r;
            let into = if stereo {
                ((sum_l + sum_r) * feedback + low_l + low_r) / 2.0
            } else {
                (sum_l + sum_r) / 2.0 * feedback + low_l
            };
            self.at = (self.at + 1) & self.mask;
            self.line[self.at] = ((into + FLUSH) - FLUSH).clamp(-16_777_216.0, 16_777_216.0);
            out_l = (1.0 - p) * sum_l + p * out_l;
            out_r = (1.0 - p) * sum_r + p * out_r;
            if stereo {
                channels[0][i] = dry * input * x_l + wet * out_l;
                channels[1][i] = dry * input * x_r + wet * out_r;
                for ch in channels.iter_mut().skip(2) {
                    ch[i] *= dry * input;
                }
            } else {
                channels[0][i] = dry * input * x_l + wet * (out_l + out_r) / 2.0;
            }
        }
        self.lows = [low_l, low_r, out_l, out_r];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delay(values: &[(usize, f32)]) -> SimpleDelay {
        let mut d = SimpleDelay::new(48_000);
        let mut p: Vec<f32> = sd::PARAMS.iter().map(|s| s.default).collect();
        for &(i, v) in values {
            p[i] = v;
        }
        d.set(&p);
        d
    }

    fn click(n: usize) -> Vec<f32> {
        let mut v = vec![0.0; n];
        v[0] = 1.0;
        v
    }

    #[test]
    fn one_echo_after_the_delay() {
        let mut d = delay(&[(sd::DRY, 0.0), (sd::WET, -6.0), (sd::TIME, 0.25)]);
        let mut ch = vec![click(30_000)];
        d.process(&mut ch);
        let out = &ch[0];
        assert!((out[0] - 1.0).abs() < 1e-6);
        // 250 ms at 48 kHz, −6 dB; nothing more.
        assert!((out[12_000] - 0.501).abs() < 1e-3, "{}", out[12_000]);
        assert!(out[1..12_000].iter().all(|s| *s == 0.0));
        assert!(out[12_001..].iter().all(|s| *s == 0.0));
    }

    #[test]
    fn feedback_falls_30_db_in_the_decay_time() {
        // 250 ms delay, 1 s decay: each echo 0.0316^0.25 of the one before
        // (about 7.5 dB down).
        let mut d = delay(&[
            (sd::DRY, -80.0),
            (sd::WET, 0.0),
            (sd::TIME, 0.25),
            (sd::MULTIPLE, 1.0),
            (sd::DECAY, 1.0),
        ]);
        let mut ch = [click(61_000)];
        // In blocks of an odd length, across the line's end.
        for block in ch[0].chunks_mut(1_000) {
            let mut b = vec![block.to_vec()];
            d.process(&mut b);
            block.copy_from_slice(&b[0]);
        }
        let out = &ch[0];
        assert_eq!(out[0], 0.0);
        let ratio = out[24_000] / out[12_000];
        assert!((ratio - 0.031_622_8f32.powf(0.25)).abs() < 1e-4, "{ratio}");
        // −30 dB after the decay time (four echoes on).
        let db = 20.0 * (out[60_000] / out[12_000]).log10();
        assert!((db + 30.0).abs() < 0.1, "{db}");
    }

    #[test]
    fn tempo_sync_takes_a_note_of_the_tempo() {
        // An eighth note at 120 BPM: 250 ms.
        let mut d = delay(&[
            (sd::DRY, -80.0),
            (sd::WET, 0.0),
            (sd::SYNC, 1.0),
            (sd::NOTE, 10.0),
        ]);
        let mut ch = vec![click(20_000)];
        d.process(&mut ch);
        assert!((ch[0][12_000] - 1.0).abs() < 1e-6);
        // At 60 BPM, twice as late.
        d.keep_time(&Tempo {
            beats_per_minute: 60.0,
            ..Tempo::default()
        });
        let mut ch = vec![click(30_000)];
        d.process(&mut ch);
        assert!((ch[0][24_000] - 1.0).abs() < 1e-6);
    }

    fn multi_tap(values: &[(usize, f32)]) -> MultiTapDelay {
        let mut d = MultiTapDelay::new(48_000);
        let mut p: Vec<f32> = mt::PARAMS.iter().map(|s| s.default).collect();
        for &(i, v) in values {
            p[i] = v;
        }
        d.set(&p);
        d
    }

    #[test]
    fn a_tap_comes_at_its_time_on_its_side() {
        // One tap, 100 ms (and its nudge, 0.123 ms), all the way left.
        let mut d = multi_tap(&[(mt::DRY, -80.0), (mt::WET, 0.0), (mt::TAP + mt::PAN, 1.0)]);
        let mut x = vec![click(9_600), click(9_600)];
        d.process(&mut x);
        let at = (100.123f32 * 48.0).round() as usize;
        let left: f32 = x[0][at - 2..at + 3].iter().sum();
        let right: f32 = x[1][at - 2..at + 3].iter().sum();
        assert!((left - 1.0).abs() < 0.01, "{left}");
        assert!(right.abs() < 1e-6, "{right}");
        assert!(x[0][..at - 2].iter().all(|s| s.abs() < 1e-6));
    }

    #[test]
    fn multi_tap_feedback_is_shared_by_the_taps() {
        // Two taps at full gain, 150 % feedback: 75 % each way round.
        let mut d = multi_tap(&[
            (mt::DRY, -80.0),
            (mt::WET, 0.0),
            (mt::FEEDBACK, 1.5),
            (mt::TAPS, 2.0),
            (mt::TAP + mt::DELAY, 10.0),
            (mt::TAP + mt::PER_TAP + mt::DELAY, 500.0),
        ]);
        let mut x = vec![click(4_000)];
        d.process(&mut x);
        // The first tap's echo, then its echo of itself 10 ms later.
        let one = (10.123f32 * 48.0).round() as usize;
        let first: f32 = x[0][one - 2..one + 3].iter().sum();
        let again: f32 = x[0][2 * one - 3..2 * one + 4].iter().sum();
        assert!((again / first - 0.75).abs() < 0.03, "{first} {again}");
    }

    #[test]
    fn dry_and_delay_out_at_their_lowest_are_silent() {
        let mut d = delay(&[(sd::DRY, -80.0), (sd::WET, -80.0)]);
        let mut ch = vec![vec![0.5f32; 10_000]];
        d.process(&mut ch);
        assert!(ch[0].iter().all(|s| *s == 0.0));
    }
}
