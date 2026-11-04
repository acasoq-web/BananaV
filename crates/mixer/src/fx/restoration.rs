//! Express Audio Restoration, as VEGAS's: clicks taken out and the
//! steady hiss turned down, for old recordings.
//!
//! Clicks: where the sound's curvature (its second difference) jumps far
//! above its usual size — the more removal, the lower the bar — and is
//! back to it a millisecond later (a note starting stays high), the few
//! milliseconds around it are drawn again in a straight line between the
//! good sound either side.
//!
//! Hiss: the sound in overlapping windows (2048 samples, a quarter apart)
//! through FFTs; each band's level, smoothed over a few windows, has its
//! quietest of the last second and a half or so taken, twice that being
//! its noise (about its mean); the band is lowered where it
//! barely rises above that noise (a spectral subtraction, one and a half
//! times the noise taken away), down to the reduction at most; bands
//! below the noise floor count as noise, bands under "affect frequencies
//! above" are left alone; the gains glide from window to window by the
//! attack and release speeds.

use bv_timeline::audio_fx::express_audio_restoration as ar;

use crate::fft::{Complex, Plan};

use super::Own;

const SIZE: usize = 2048;
const HOP: usize = SIZE / 4;
/// The quietest level is kept for stretches of this many windows, the
/// last [`STRETCHES`] of them.
const STRETCH: usize = 16;
const STRETCHES: usize = 8;

#[derive(Debug)]
pub(crate) struct Restoration {
    rate: f32,
    params: Vec<f32>,
    plan: Plan,
    window: Vec<f32>,
    /// Each channel: its input gathered, its output going out (overlap-
    /// added), each band's noise and gain, the click detector's state.
    input: Vec<Vec<f32>>,
    output: Vec<Vec<f32>>,
    level: Vec<Vec<f32>>,
    /// Each band's quietest level in the stretch under way and in the
    /// last ones (each channel), how many windows the stretch has had,
    /// and where the next past one goes.
    quietest: Vec<Vec<f32>>,
    past: Vec<Vec<[f32; STRETCHES]>>,
    windows: usize,
    stretch: usize,
    /// How many windows have gone by since the start (the first ones,
    /// partly before the sound, are not looked at for the noise).
    seen: usize,
    gains: Vec<Vec<f32>>,
    curvature: Vec<f32>,
    at: usize,
    /// Each channel's newest sound, held while clicks are looked for.
    pre: Vec<Vec<f32>>,
    pre_at: usize,
    frame: Vec<Complex>,
}

impl Restoration {
    pub(crate) fn new(rate: u32) -> Restoration {
        let window = (0..SIZE)
            .map(|i| {
                // A Hann window's square root, on both sides: their product
                // a Hann, which sums to 2 at a quarter's overlap.
                let h = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / SIZE as f32).cos();
                h.sqrt()
            })
            .collect();
        Restoration {
            rate: rate.max(1) as f32,
            params: Vec::new(),
            plan: Plan::new(SIZE),
            window,
            input: Vec::new(),
            output: Vec::new(),
            level: Vec::new(),
            quietest: Vec::new(),
            past: Vec::new(),
            windows: 0,
            stretch: 0,
            seen: 0,
            gains: Vec::new(),
            curvature: Vec::new(),
            at: 0,
            pre: Vec::new(),
            pre_at: 0,
            frame: Vec::new(),
        }
    }

    fn p(&self, i: usize) -> f32 {
        let s = &ar::PARAMS[i];
        self.params
            .get(i)
            .copied()
            .unwrap_or(s.default)
            .clamp(s.min, s.max)
    }

    /// One window of channel `c` (its last SIZE input samples) through the
    /// noise reduction, overlap-added into its output.
    fn window_out(&mut self, c: usize) {
        let reduce = self.p(ar::REDUCTION);
        let floor_gain = if reduce >= 100.0 {
            0.0
        } else {
            10f32.powf(-reduce / 20.0)
        };
        let floor_db = self.p(ar::FLOOR);
        let floor = if floor_db <= -96.0 {
            0.0
        } else {
            (0.25 * 10f32.powf(floor_db / 20.0) * SIZE as f32 / 4.0).powi(2)
        };
        let from = (self.p(ar::ABOVE) / self.rate * SIZE as f32) as usize;
        let attack = (100.0 - self.p(ar::ATTACK)) / 100.0;
        let release = (100.0 - self.p(ar::RELEASE)) / 100.0;
        let start = self.at;
        let input = &self.input[c];
        self.frame.clear();
        self.frame.extend(
            (0..SIZE)
                .map(|i| Complex::new((input[(start + i) % SIZE] * self.window[i]) as f64, 0.0)),
        );
        self.plan.run(&mut self.frame, false);
        let level = &mut self.level[c];
        let quietest = &mut self.quietest[c];
        let past = &self.past[c];
        let gains = &mut self.gains[c];
        for k in 0..=SIZE / 2 {
            let power = (self.frame[k].re.powi(2) + self.frame[k].im.powi(2)) as f32;
            let l = &mut level[k];
            *l = if *l == 0.0 {
                power
            } else {
                0.8 * *l + 0.2 * power
            };
            if self.seen >= SIZE / HOP {
                quietest[k] = quietest[k].min(*l);
            }
            let noise = 2.0 * past[k].iter().fold(quietest[k], |m, &v| m.min(v));
            // Nothing known of the noise yet: the band is left alone.
            let target = if k < from || reduce <= 0.0 || !noise.is_finite() {
                1.0
            } else if power <= floor {
                floor_gain
            } else {
                (1.0 - 1.5 * noise / power.max(1e-20))
                    .max(0.0)
                    .sqrt()
                    .max(floor_gain)
            };
            let keep = if target < gains[k] { attack } else { release };
            gains[k] = target + (gains[k] - target) * keep;
            let g = gains[k] as f64;
            self.frame[k].re *= g;
            self.frame[k].im *= g;
            if k > 0 && k < SIZE / 2 {
                self.frame[SIZE - k].re *= g;
                self.frame[SIZE - k].im *= g;
            }
        }
        self.plan.run(&mut self.frame, true);
        let out = &mut self.output[c];
        for i in 0..SIZE {
            out[(start + i) % SIZE] += self.frame[i].re as f32 * self.window[i] * 0.5;
        }
    }
}

impl Own for Restoration {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let count = channels.len();
        let span = ((0.002 * self.rate) as usize).max(2);
        let held = 4 * span + 4;
        if self.input.len() != count || self.pre.first().map_or(0, Vec::len) != held {
            self.input = vec![vec![0.0; SIZE]; count];
            self.output = vec![vec![0.0; SIZE]; count];
            self.level = vec![vec![0.0; SIZE / 2 + 1]; count];
            self.quietest = vec![vec![f32::MAX; SIZE / 2 + 1]; count];
            self.past = vec![vec![[f32::MAX; STRETCHES]; SIZE / 2 + 1]; count];
            self.windows = 0;
            self.stretch = 0;
            self.seen = 0;
            self.gains = vec![vec![1.0; SIZE / 2 + 1]; count];
            self.curvature = vec![0.0; count];
            self.pre = vec![vec![0.0; held]; count];
            self.at = 0;
            self.pre_at = 0;
        }
        let n = channels.first().map_or(0, Vec::len);
        let amount = self.p(ar::CLICKS);
        // The click bar: how far above its usual size the curvature must go.
        let bar = 10f32.powf((22.0 - amount) / 18.0);
        let fade = 1.0 - 1.0 / (0.05 * self.rate);
        for i in 0..n {
            for (c, ch) in channels.iter_mut().enumerate() {
                // The newest sound waits a little, so a click is mended
                // whole before it goes on.
                let pre = &mut self.pre[c];
                pre[self.pre_at] = ch[i];
                let back = |pre: &[f32], k: usize| pre[(self.pre_at + held - k) % held];
                let bend = |pre: &[f32], k: usize| {
                    (back(pre, k) - 2.0 * back(pre, k + 1) + back(pre, k + 2)).abs()
                };
                let curve = bend(pre, span);
                let usual = self.curvature[c];
                let limit = bar * usual * 4.0;
                // A click is short: the sound just after it (the newest
                // half span) bends as usual again; a note starting does not.
                let after = (0..span / 2).map(|k| bend(pre, k)).sum::<f32>() / (span / 2) as f32;
                let click = usual > 0.0 && curve > limit && after < limit * 0.25;
                if click {
                    // Mended: a straight line across the samples around it.
                    let (a, b) = (back(pre, 2 * span + 2), back(pre, 0));
                    for k in 1..=(2 * span + 1) {
                        let t = k as f32 / (2 * span + 2) as f32;
                        let pos = (self.pre_at + held - (2 * span + 2) + k) % held;
                        pre[pos] = a + (b - a) * t;
                    }
                } else {
                    self.curvature[c] = curve + (usual - curve) * fade;
                }
                let leaving = pre[(self.pre_at + 1) % held];
                self.input[c][self.at] = leaving;
                let out = &mut self.output[c];
                ch[i] = out[self.at];
                out[self.at] = 0.0;
            }
            self.pre_at = (self.pre_at + 1) % held;
            self.at = (self.at + 1) % SIZE;
            if self.at.is_multiple_of(HOP) {
                for c in 0..count {
                    self.window_out(c);
                }
                // A stretch ends: its quietest levels join the past ones.
                self.seen += 1;
                self.windows += 1;
                if self.windows == STRETCH {
                    self.windows = 0;
                    for (now, past) in self.quietest.iter_mut().zip(&mut self.past) {
                        for (q, p) in now.iter_mut().zip(past.iter_mut()) {
                            p[self.stretch] = *q;
                            *q = f32::MAX;
                        }
                    }
                    self.stretch = (self.stretch + 1) % STRETCHES;
                }
            }
        }
    }

    fn latency(&self) -> usize {
        let span = ((0.002 * self.rate) as usize).max(2);
        SIZE + 4 * span + 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn restore(values: &[(usize, f32)]) -> Restoration {
        let mut r = Restoration::new(48_000);
        let mut p: Vec<f32> = ar::PARAMS.iter().map(|s| s.default).collect();
        for &(i, v) in values {
            p[i] = v;
        }
        r.set(&p);
        r
    }

    #[test]
    fn with_nothing_to_do_the_sound_comes_through_late() {
        let mut r = restore(&[(ar::REDUCTION, 0.0), (ar::CLICKS, 1.0)]);
        let x: Vec<f32> = (0..24_000)
            .map(|i| 0.5 * (std::f32::consts::TAU * 440.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut ch = vec![x.clone()];
        r.process(&mut ch);
        let lat = r.latency();
        for i in 6_000..20_000 {
            assert!(
                (ch[0][i] - x[i - lat]).abs() < 1e-3,
                "{i}: {} {}",
                ch[0][i],
                x[i - lat]
            );
        }
    }

    #[test]
    fn hiss_comes_down_and_a_click_goes() {
        // Quiet noise alone: turned well down.
        let mut r = restore(&[(ar::REDUCTION, 30.0), (ar::ABOVE, 100.0)]);
        let mut seed = 1u32;
        let noise: Vec<f32> = (0..96_000)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed as f32 / u32::MAX as f32 - 0.5) * 0.02
            })
            .collect();
        let mut ch = vec![noise.clone()];
        r.process(&mut ch);
        let power = |x: &[f32]| x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32;
        let before = power(&noise[72_000..]);
        let after = power(&ch[0][72_000..]);
        assert!(after < before / 4.0, "{before} {after}");
        // A click on a quiet tone: mended.
        let mut r = restore(&[(ar::REDUCTION, 0.0), (ar::CLICKS, 20.0)]);
        let mut x: Vec<f32> = (0..24_000)
            .map(|i| 0.1 * (std::f32::consts::TAU * 200.0 * i as f32 / 48_000.0).sin())
            .collect();
        x[12_000] += 0.9;
        let mut ch = vec![x];
        r.process(&mut ch);
        let lat = r.latency();
        let worst = ch[0][12_000 + lat - 200..12_000 + lat + 200]
            .iter()
            .fold(0f32, |m, s| m.max(s.abs()));
        assert!(worst < 0.3, "{worst}");
    }
}
