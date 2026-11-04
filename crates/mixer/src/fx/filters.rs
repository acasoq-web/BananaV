//! Smooth/Enhance and Resonant Filter, as VEGAS's.
//!
//! Smooth/Enhance: a first-order filter a channel, `y = (1 − p)·x ± p·y₁`
//! with `p = e^(−2π·f)`, `f` a part of the sampling rate; smoothing puts
//! the pole at `+p` (a low pass), enhancing at `−p` with a gain after it
//! (the lows back near 0 dB, the highs raised).
//!
//! Resonant Filter: a state-variable filter (Chamberlin's), its low-,
//! band- and high-pass at once, run three times a sample so that it stays
//! steady near the top of the band; the fourth order is two in a row.

use bv_timeline::audio_fx::{resonant_filter as rf, smooth_enhance};

use super::{Own, Ramp};

/// Each `Operation`'s cutoff (a part of the rate), whether it enhances,
/// and its gain after the filter: −5 to −1 smooth, 1 to 5 enhance.
const STEPS: [(f32, bool, f32); 10] = [
    (0.008, false, 1.0),
    (0.02, false, 1.0),
    (0.06, false, 1.0),
    (0.15, false, 1.0),
    (0.25, false, 1.0),
    (0.2, true, 1.78),
    (0.15, true, 2.5),
    (0.1, true, 3.1),
    (0.08, true, 4.0),
    (0.05, true, 5.6),
];

/// Below this the filter's memory is 0 (no denormal numbers).
const TINY: f32 = 2e-11;

#[derive(Debug, Default)]
pub(crate) struct SmoothEnhance {
    /// The filter's `a`, `b` and gain; none at 0.
    coefs: Option<(f32, f32, f32)>,
    /// Each channel's last output before the gain.
    memory: Vec<f32>,
}

impl Own for SmoothEnhance {
    fn set(&mut self, params: &[f32]) {
        let k = params
            .get(smooth_enhance::OPERATION)
            .copied()
            .unwrap_or(0.0)
            .round()
            .clamp(-5.0, 5.0) as i32;
        self.coefs = match k {
            0 => None,
            k => {
                let (f, enhance, gain) = STEPS[if k < 0 {
                    (k + 5) as usize
                } else {
                    (k + 4) as usize
                }];
                let p = (-std::f32::consts::TAU * f).exp();
                Some((1.0 - p, if enhance { -p } else { p }, gain))
            }
        };
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let Some((a, b, gain)) = self.coefs else {
            return;
        };
        self.memory.resize(channels.len(), 0.0);
        for (ch, y1) in channels.iter_mut().zip(self.memory.iter_mut()) {
            // A recursion along time: each channel on its own.
            let mut y = *y1;
            for s in ch.iter_mut() {
                y = a * *s + b * y;
                if y.abs() < TINY {
                    y = 0.0;
                }
                *s = y * gain;
            }
            *y1 = y;
        }
    }
}

/// Added and taken away again on the filters' memory, so a fading
/// signal ends at 0 instead of lingering as a denormal number.
const FLUSH: f32 = 1e-18;

#[derive(Debug)]
pub(crate) struct ResonantFilter {
    rate: f32,
    /// Its frequency (Hz), damping and wet part, gliding over a block.
    freq: Ramp,
    damping: Ramp,
    wet: Ramp,
    /// 0 low-pass, 1 band-pass, 2 high-pass.
    kind: usize,
    fourth: bool,
    /// Each channel's two sections' low-pass and band-pass memories.
    state: Vec<[f32; 4]>,
}

impl ResonantFilter {
    pub(crate) fn new(rate: u32) -> ResonantFilter {
        ResonantFilter {
            rate: rate.max(1) as f32,
            freq: Ramp::new(20_000.0),
            damping: Ramp::new(std::f32::consts::SQRT_2),
            wet: Ramp::new(1.0),
            kind: 0,
            fourth: false,
            state: Vec::new(),
        }
    }
}

/// The filter's frequency coefficient at `hz` and sampling rate `rate`:
/// three steps a sample.
fn coef(rate: f32, hz: f32) -> f32 {
    2.0 * (std::f32::consts::PI * (hz / (3.0 * rate)).clamp(0.0, 0.1512)).sin()
}

/// One sample through a section (its low-pass and band-pass memories),
/// three steps; its low-, band- and high-pass.
#[inline(always)]
fn section(x: f32, f: f32, q: f32, low: &mut f32, band: &mut f32) -> [f32; 3] {
    let mut high = 0.0;
    for _ in 0..3 {
        *low += f * *band;
        high = x - *low - q * *band;
        *band += f * high;
    }
    *low = (*low + FLUSH) - FLUSH;
    *band = (*band + FLUSH) - FLUSH;
    [*low, *band, high]
}

impl Own for ResonantFilter {
    fn set(&mut self, params: &[f32]) {
        let p = |i: usize| params.get(i).copied().unwrap_or(rf::PARAMS[i].default);
        self.freq.set(p(rf::FREQUENCY).clamp(1.0, 96_000.0));
        // 0 %: a damping of √2 (no peak); 100 %: 0.04 (a peak 28 dB high).
        let res = p(rf::RESONANCE).clamp(0.0, 1.0);
        self.damping
            .set(1.0 / (res * 24.2929 + std::f32::consts::FRAC_1_SQRT_2));
        self.wet.set(p(rf::MIX).clamp(0.0, 1.0));
        self.kind = p(rf::TYPE).round().clamp(0.0, 2.0) as usize;
        self.fourth = p(rf::ORDER) >= 0.5;
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let (f0, f_step) = self.freq.block(n);
        let (q0, q_step) = self.damping.block(n);
        let (w0, w_step) = self.wet.block(n);
        self.state.resize(channels.len(), [0.0; 4]);
        let (rate, kind, fourth) = (self.rate, self.kind, self.fourth);
        let fixed = coef(rate, f0);
        for (ch, st) in channels.iter_mut().zip(self.state.iter_mut()) {
            let [mut l1, mut b1, mut l2, mut b2] = *st;
            for (i, x) in ch.iter_mut().enumerate() {
                let t = i as f32;
                // A frequency moving within the block: its coefficient
                // worked out again at every sample, so a sweep is smooth.
                let f = if f_step == 0.0 {
                    fixed
                } else {
                    coef(rate, f0 + f_step * t)
                };
                let q = q0 + q_step * t;
                let mut y = section(*x, f, q, &mut l1, &mut b1)[kind];
                if fourth {
                    y = section(y, f, q, &mut l2, &mut b2)[kind];
                }
                let w = w0 + w_step * t;
                *x = w * y + (1.0 - w) * *x;
            }
            *st = [l1, b1, l2, b2];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The response at DC and at half the rate, from a steady signal and
    /// one alternating every sample.
    fn ends(k: f32) -> (f32, f32) {
        let mut f = SmoothEnhance::default();
        f.set(&[k]);
        let mut dc = vec![vec![1.0f32; 4000]];
        f.process(&mut dc);
        let mut f = SmoothEnhance::default();
        f.set(&[k]);
        let mut nyq = vec![
            (0..4000)
                .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
                .collect(),
        ];
        f.process(&mut nyq);
        (dc[0][3999], nyq[0][3999].abs())
    }

    #[test]
    fn smoothing_keeps_the_lows_and_cuts_the_highs() {
        let (dc, top) = ends(-5.0);
        assert!((dc - 1.0).abs() < 1e-4, "{dc}");
        let p = (-std::f32::consts::TAU * 0.008).exp();
        assert!((top - (1.0 - p) / (1.0 + p)).abs() < 1e-4, "{top}");
        // About −32 dB at half the rate.
        assert!((20.0 * top.log10() + 32.0).abs() < 1.0, "{top}");
    }

    #[test]
    fn enhancing_raises_the_highs_by_its_gain() {
        // 1: −0.08 dB in the lows, +5 dB at half the rate.
        let (dc, top) = ends(1.0);
        assert!((20.0 * dc.log10() + 0.08).abs() < 0.05, "{dc}");
        assert!((20.0 * top.log10() - 5.0).abs() < 0.05, "{top}");
        // 5: +15 dB.
        let (_, top) = ends(5.0);
        assert!((20.0 * top.log10() - 15.0).abs() < 0.05, "{top}");
    }

    fn sine(hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin())
            .collect()
    }

    /// The level (dB) a sine at `hz` comes out at, once settled.
    fn level(params: &[f32], hz: f32) -> f32 {
        let mut f = ResonantFilter::new(48_000);
        f.set(params);
        let mut ch = vec![sine(hz, 48_000)];
        f.process(&mut ch);
        // Its amplitude from its power (a sampled sine's peak can miss it).
        let tail = &ch[0][24_000..];
        let power = tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32;
        10.0 * (2.0 * power).log10()
    }

    #[test]
    fn low_pass_keeps_the_lows_and_rings_at_its_frequency() {
        // 1 kHz, no resonance: 0 dB well below, −3 dB at it, falling above.
        let flat = [1000.0, 0.0, 1.0, 0.0, 0.0];
        assert!(level(&flat, 100.0).abs() < 0.2);
        assert!(
            (level(&flat, 1000.0) + 3.0).abs() < 0.5,
            "{}",
            level(&flat, 1000.0)
        );
        assert!(level(&flat, 8000.0) < -30.0);
        // Full resonance: a peak of about 28 dB at the frequency.
        let ringing = [1000.0, 1.0, 1.0, 0.0, 0.0];
        assert!(
            (level(&ringing, 1000.0) - 28.0).abs() < 1.0,
            "{}",
            level(&ringing, 1000.0)
        );
        // The fourth order falls about twice as steeply.
        let fourth = [1000.0, 0.0, 1.0, 0.0, 1.0];
        assert!(level(&fourth, 3000.0) < 1.9 * level(&flat, 3000.0));
    }

    #[test]
    fn band_and_high_pass_choose_their_output() {
        let band = [1000.0, 0.0, 1.0, 1.0, 0.0];
        assert!(level(&band, 1000.0) > level(&band, 100.0) + 15.0);
        assert!(level(&band, 1000.0) > level(&band, 10_000.0) + 15.0);
        let high = [1000.0, 0.0, 1.0, 2.0, 0.0];
        assert!(level(&high, 8000.0).abs() < 0.5);
        assert!(level(&high, 100.0) < -30.0);
    }

    #[test]
    fn wet_dry_blends_back_the_sound() {
        // Dry only: the sound as it was.
        let mut f = ResonantFilter::new(48_000);
        f.set(&[200.0, 1.0, 0.0, 0.0, 0.0]);
        let x = sine(5000.0, 4800);
        let mut ch = vec![x.clone()];
        f.process(&mut ch);
        assert!(ch[0].iter().zip(&x).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn zero_leaves_the_sound_alone() {
        let mut f = SmoothEnhance::default();
        f.set(&[0.0]);
        let mut ch = vec![vec![0.3f32, -0.7, 0.1]];
        f.process(&mut ch);
        assert_eq!(ch[0], [0.3, -0.7, 0.1]);
    }
}
