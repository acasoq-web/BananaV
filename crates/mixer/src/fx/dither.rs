//! Dither, as VEGAS's: the sound rounded (to the nearest, ties to even) to
//! the steps of a bit depth, a little noise added first so the rounding's
//! error turns into a steady hiss instead of distortion. Noise shaping
//! feeds the rounding errors back so that hiss moves up, where it is
//! heard less: by one sample (a high-pass contour) or along the ear's
//! equal loudness contour (the published E-weighted filter of Lipshitz,
//! Vanderkooy and Wannamaker, five taps). The result stays a float
//! signal, holding only the levels of that depth.
//!
//! The noise comes from a counter run through a hash, so a block's noise
//! is made at once (vectorized); the rounding with shaping goes sample by
//! sample, as its errors feed back.

use bv_timeline::audio_fx::dither;

use super::Own;

/// Equal loudness contour's error feedback, newest error first.
const CONTOUR: [f64; 5] = [2.033, -2.165, 1.959, -1.590, 0.6149];

#[derive(Debug)]
pub(crate) struct Dither {
    /// Steps of the depth in each unit of the signal: 2^(bits − 1).
    steps: f64,
    noise: usize,
    shaping: usize,
    /// Each channel's last rounding errors, newest first.
    errors: Vec<[f64; 5]>,
    /// Each channel's last uniform number (the high-pass triangular noise).
    last: Vec<f64>,
    /// The noise generator's counter.
    count: u32,
    /// One block's noise (kept, not made again each time).
    buf: Vec<f64>,
}

impl Default for Dither {
    fn default() -> Dither {
        Dither {
            steps: 32768.0,
            noise: 1,
            shaping: 0,
            errors: Vec::new(),
            last: Vec::new(),
            count: 0,
            buf: Vec::new(),
        }
    }
}

/// A well-mixed 32-bit hash (a counter's next random number).
fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// A uniform number in (−½, ½) from counter `c` of stream `s`.
fn uniform(c: u32, s: u32) -> f64 {
    hash(c ^ s.wrapping_mul(0x9e37_79b9)) as f64 / 4_294_967_296.0 - 0.5
}

impl Dither {
    /// Channel `ch`'s noise for the next `n` samples, in steps, into `buf`.
    fn noise(&mut self, ch: usize, n: usize) {
        let c0 = self.count;
        let s = ch as u32 * 4;
        self.buf.clear();
        self.buf.resize(n, 0.0);
        let at = |k: usize| c0.wrapping_add(k as u32);
        match self.noise {
            // Half rectangular: ±¼ step.
            1 => {
                for (k, d) in self.buf.iter_mut().enumerate() {
                    *d = 0.5 * uniform(at(k), s);
                }
            }
            // Rectangular: ±½ step.
            2 => {
                for (k, d) in self.buf.iter_mut().enumerate() {
                    *d = uniform(at(k), s);
                }
            }
            // Triangular: two uniforms, ±1 step.
            3 => {
                for (k, d) in self.buf.iter_mut().enumerate() {
                    *d = uniform(at(k), s) + uniform(at(k), s + 1);
                }
            }
            // High-pass triangular: each uniform less the one before.
            4 => {
                let mut last = self.last[ch];
                for (k, d) in self.buf.iter_mut().enumerate() {
                    let u = uniform(at(k), s);
                    *d = u - last;
                    last = u;
                }
                self.last[ch] = last;
            }
            // Gaussian: four uniforms, scaled to a deviation of ½ step.
            5 => {
                for (k, d) in self.buf.iter_mut().enumerate() {
                    let sum = uniform(at(k), s)
                        + uniform(at(k), s + 1)
                        + uniform(at(k), s + 2)
                        + uniform(at(k), s + 3);
                    *d = sum * 0.866_025_4;
                }
            }
            _ => {}
        }
    }
}

impl Own for Dither {
    fn set(&mut self, params: &[f32]) {
        let pick = |i: usize, top: usize| {
            params
                .get(i)
                .copied()
                .unwrap_or(dither::PARAMS[i].default)
                .round()
                .clamp(0.0, top as f32) as usize
        };
        let bits = dither::BITS[pick(dither::DEPTH, dither::BITS.len() - 1)];
        self.steps = 2f64.powi(bits as i32 - 1);
        self.noise = pick(dither::NOISE, 5);
        self.shaping = pick(dither::SHAPING, 2);
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        self.errors.resize(channels.len(), [0.0; 5]);
        self.last.resize(channels.len(), 0.0);
        let q = self.steps;
        for (c, ch) in channels.iter_mut().enumerate() {
            self.noise(c, n);
            let e = &mut self.errors[c];
            match self.shaping {
                0 => {
                    for (x, d) in ch.iter_mut().zip(self.buf.iter()) {
                        *x = ((*x as f64 * q + d).round_ties_even() / q) as f32;
                    }
                }
                1 => {
                    for (x, d) in ch.iter_mut().zip(self.buf.iter()) {
                        let v = *x as f64 * q - e[0];
                        let r = (v + d).round_ties_even();
                        e[0] = r - v;
                        *x = (r / q) as f32;
                    }
                }
                _ => {
                    for (x, d) in ch.iter_mut().zip(self.buf.iter()) {
                        let fed: f64 = CONTOUR.iter().zip(e.iter()).map(|(c, e)| c * e).sum();
                        let v = *x as f64 * q - fed;
                        let r = (v + d).round_ties_even();
                        e.copy_within(0..4, 1);
                        e[0] = r - v;
                        *x = (r / q) as f32;
                    }
                }
            }
        }
        self.count = self.count.wrapping_add(n as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dither(depth: f32, noise: f32, shaping: f32) -> Dither {
        let mut d = Dither::default();
        d.set(&[depth, noise, shaping]);
        d
    }

    fn sine(n: usize, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (i as f32 * std::f32::consts::TAU * 1000.0 / 48_000.0).sin())
            .collect()
    }

    #[test]
    fn every_sample_lands_on_a_step_of_the_depth() {
        for (depth, bits) in [(0.0, 8), (1.0, 12), (2.0, 16)] {
            for noise in 0..6 {
                for shaping in 0..3 {
                    let mut d = dither(depth, noise as f32, shaping as f32);
                    let mut ch = vec![sine(4800, 0.5), sine(4800, 0.25)];
                    d.process(&mut ch);
                    let q = 2f32.powi(bits - 1);
                    for s in ch.iter().flatten() {
                        let v = s * q;
                        assert!((v - v.round()).abs() < 1e-3, "{bits} bits: {s}");
                    }
                }
            }
        }
    }

    #[test]
    fn without_noise_it_rounds_to_the_nearest_step() {
        let mut d = dither(0.0, 0.0, 0.0);
        let mut ch = vec![vec![0.3f32, -0.3, 1.0 / 256.0]];
        d.process(&mut ch);
        let q = 128.0;
        assert_eq!(ch[0][0], (0.3f32 * q).round() / q);
        assert_eq!(ch[0][1], (-0.3f32 * q).round() / q);
        // Ties go to the even step.
        assert_eq!(ch[0][2], 0.0);
    }

    #[test]
    fn noise_has_the_type_s_spread() {
        // On silence at 8 bits, the result is the noise rounded: rectangular
        // (±½ step) rounds to 0 or ±1 step, triangular reaches ±1.
        for (noise, most) in [(2.0, 1.0), (3.0, 1.0), (1.0, 0.0)] {
            let mut d = dither(0.0, noise, 0.0);
            let mut ch = vec![vec![0.0f32; 48_000]];
            d.process(&mut ch);
            let top = ch[0].iter().fold(0f32, |m, s| m.max(s.abs())) * 128.0;
            assert!(top <= most + 1e-6, "{noise}: {top}");
        }
        // Its mean is 0 and its power about a step's twelfth (rectangular).
        let mut d = dither(0.0, 2.0, 0.0);
        let mut ch = vec![vec![0.3f32 / 128.0; 48_000]];
        d.process(&mut ch);
        let mean = ch[0].iter().map(|s| *s as f64).sum::<f64>() / 48_000.0 * 128.0;
        assert!((mean - 0.3).abs() < 0.02, "{mean}");
    }

    /// The power of `x` in the band `lo..hi` (a part of the rate), by a
    /// plain DFT on a few bins.
    fn band_power(x: &[f32], lo: f64, hi: f64) -> f64 {
        let n = x.len();
        let mut sum = 0.0;
        let bins = ((lo * n as f64) as usize)..((hi * n as f64) as usize);
        for k in bins.step_by(7) {
            let (mut re, mut im) = (0.0, 0.0);
            for (i, s) in x.iter().enumerate() {
                let a = std::f64::consts::TAU * (k * i) as f64 / n as f64;
                re += *s as f64 * a.cos();
                im -= *s as f64 * a.sin();
            }
            sum += re * re + im * im;
        }
        sum
    }

    #[test]
    fn shaping_moves_the_noise_up() {
        let error = |shaping: f32| {
            let mut d = dither(0.0, 3.0, shaping);
            let x: Vec<f32> = sine(8192, 0.3);
            let mut ch = vec![x.clone()];
            d.process(&mut ch);
            let e: Vec<f32> = ch[0].iter().zip(&x).map(|(y, x)| y - x).collect();
            (band_power(&e, 0.02, 0.1), band_power(&e, 0.4, 0.5))
        };
        let (low_flat, high_flat) = error(0.0);
        for shaping in [1.0, 2.0] {
            let (low, high) = error(shaping);
            assert!(low < low_flat * 0.5, "{shaping}: {low} vs {low_flat}");
            assert!(high > high_flat * 2.0, "{shaping}: {high} vs {high_flat}");
        }
    }
}
