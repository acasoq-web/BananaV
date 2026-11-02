//! Beats and tempo of a sound, as VEGAS's Beat Detection and Tempo
//! Detection find them: the energy its low and low-middle frequencies gain
//! from moment to moment — a spectral flux under 500 Hz, a hundred times a
//! second, on the sound brought down to 4 kHz —; the flux's peaks are the
//! beats, each with how far it stands out from the flux around it, so a
//! sensitivity picks among them without reading the sound again; the tempo
//! is the period the flux repeats at best (its autocorrelation, weighed
//! toward the 120 beats a minute people tap at, as Ellis's beat tracker
//! weighs it, and refined on a multiple of the period).

use crate::fft::spectrum;

/// The flux's values a second.
pub const HOPS_PER_SECOND: f64 = 100.0;

/// The rate the sound is brought down to (Hz).
const LOW_RATE: f64 = 4000.0;
/// The spectrum's window, in samples at `LOW_RATE` (64 ms).
const WINDOW: usize = 256;
/// The highest frequency the flux counts (Hz).
const TOP_HZ: f64 = 500.0;
/// Where the sound is cut before it is brought down (Hz): far enough under
/// `LOW_RATE` that nothing folds back into the frequencies counted.
const CUT_HZ: f64 = 800.0;

/// A second-order low-pass (the RBJ cookbook's, Q of a Butterworth's).
#[derive(Clone, Copy, Debug)]
struct LowPass {
    b: [f64; 3],
    a: [f64; 2],
    x: [f64; 2],
    y: [f64; 2],
}

impl LowPass {
    fn new(cut: f64, rate: f64) -> LowPass {
        let w = std::f64::consts::TAU * (cut / rate).min(0.45);
        let alpha = w.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
        let c = w.cos();
        let a0 = 1.0 + alpha;
        LowPass {
            b: [(1.0 - c) / 2.0 / a0, (1.0 - c) / a0, (1.0 - c) / 2.0 / a0],
            a: [-2.0 * c / a0, (1.0 - alpha) / a0],
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }

    fn run(&mut self, v: f64) -> f64 {
        let y = self.b[0] * v + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [v, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// The low end's flux of a sound that comes in blocks (its channels mixed):
/// [`LowFlux::push`] each block, then [`LowFlux::finish`].
#[derive(Clone, Debug)]
pub struct LowFlux {
    filter: LowPass,
    /// The rate the sound is kept at.
    low_rate: f64,
    /// Samples of the sound for each one kept, and when the next is kept.
    step: f64,
    next: f64,
    taken: f64,
    sum: f64,
    count: u32,
    low: Vec<f32>,
}

impl LowFlux {
    /// For a sound of `rate` samples a second.
    pub fn new(rate: u32) -> LowFlux {
        let rate = f64::from(rate.max(1));
        let step = (rate / LOW_RATE).max(1.0);
        LowFlux {
            filter: LowPass::new(CUT_HZ, rate),
            low_rate: rate / step,
            step,
            next: step,
            taken: 0.0,
            sum: 0.0,
            count: 0,
            low: Vec::new(),
        }
    }

    /// The next samples of the sound.
    pub fn push(&mut self, samples: &[f32]) {
        for &s in samples {
            self.sum += self.filter.run(f64::from(s));
            self.count += 1;
            self.taken += 1.0;
            if self.taken >= self.next {
                self.low.push((self.sum / f64::from(self.count)) as f32);
                self.sum = 0.0;
                self.count = 0;
                self.next += self.step;
            }
        }
    }

    /// What the sound was found to hold: its flux, and the low end kept to
    /// place its beats.
    pub fn finish(self) -> Analysis {
        Analysis {
            flux: flux(&self.low, self.low_rate),
            low: self.low,
            low_rate: self.low_rate,
        }
    }
}

/// What the flux of a sound found.
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    /// The flux, [`HOPS_PER_SECOND`] values a second: value `i` how much
    /// the low end's spectrum grew around `i ⁄ HOPS_PER_SECOND` seconds.
    pub flux: Vec<f32>,
    /// The sound's low end, `low_rate` samples a second.
    low: Vec<f32>,
    low_rate: f64,
}

impl Analysis {
    /// The beats: the flux's peaks ([`peaks`]), each placed where the low
    /// end's energy rises fastest near it (the flux, on 64 ms windows,
    /// peaks as an attack enters its window: early).
    pub fn beats(&self) -> Vec<Beat> {
        peaks(&self.flux)
            .into_iter()
            .map(|b| Beat {
                time: self.onset(b.time),
                ..b
            })
            .collect()
    }

    /// The tempo ([`tempo`]).
    pub fn tempo(&self) -> Option<f64> {
        tempo(&self.flux)
    }

    /// The moment near `t` (seconds) the low end's energy rises fastest:
    /// its energy over 5 ms windows every 2.5 ms, each against the one
    /// before, from 30 ms before `t` to 50 ms after.
    fn onset(&self, t: f64) -> f64 {
        let r = self.low_rate;
        if self.low.is_empty() || r <= 0.0 {
            return t;
        }
        let step = (r * 0.0025).round().max(1.0) as i64;
        let width = 2 * step;
        let energy = |start: i64| -> f64 {
            let sum: f64 = (start..start + width)
                .map(|k| {
                    let v = usize::try_from(k)
                        .ok()
                        .and_then(|j| self.low.get(j))
                        .copied()
                        .map_or(0.0, f64::from);
                    v * v
                })
                .sum();
            sum / width as f64 + 1e-12
        };
        let (from, to) = (
            ((t - 0.03) * r).round() as i64,
            ((t + 0.05) * r).round() as i64,
        );
        let mut best = (f64::NEG_INFINITY, t);
        let mut s = from;
        while s <= to {
            let rise = energy(s).ln() - energy(s - width).ln();
            if rise > best.0 {
                best = (rise, s as f64 / r);
            }
            s += step;
        }
        best.1.max(0.0)
    }
}

/// The flux of `low`, a sound at `rate` samples a second (about
/// `LOW_RATE`).
fn flux(low: &[f32], rate: f64) -> Vec<f32> {
    let hop = rate / HOPS_PER_SECOND;
    let frames = (low.len() as f64 / hop).floor() as usize;
    let window: Vec<f64> = (0..WINDOW)
        .map(|k| {
            let t = (k as f64 + 0.5) / WINDOW as f64;
            0.5 - 0.5 * (std::f64::consts::TAU * t).cos()
        })
        .collect();
    let bins = ((TOP_HZ / (rate / WINDOW as f64)).round() as usize).clamp(1, WINDOW / 2);
    let mut prev = vec![0.0f64; bins + 1];
    let mut out = Vec::with_capacity(frames);
    let mut seg = vec![0.0f64; WINDOW];
    for i in 0..frames {
        // The window's middle at the frame's time.
        let start = (i as f64 * hop).round() as i64 - (WINDOW / 2) as i64;
        for (k, s) in seg.iter_mut().enumerate() {
            let at = start + k as i64;
            let v = usize::try_from(at)
                .ok()
                .and_then(|j| low.get(j))
                .copied()
                .unwrap_or(0.0);
            *s = f64::from(v) * window[k];
        }
        let spec = spectrum(&seg, WINDOW);
        let mut f = 0.0;
        for b in 1..=bins {
            let m = (spec[b].re.hypot(spec[b].im) * 10.0).ln_1p();
            if i > 0 {
                f += (m - prev[b]).max(0.0);
            }
            prev[b] = m;
        }
        out.push(f as f32);
    }
    out
}

/// A beat: when (seconds) and how far it stands out from the flux around
/// it (in that flux's standard deviations).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beat {
    pub time: f64,
    pub strength: f32,
}

/// How far around a peak the flux it stands out from reaches (hops).
const AROUND: usize = 75;
/// How far apart two beats are at least (hops: a tenth of a second).
const APART: usize = 10;

/// The flux's peaks, in order: its highest values within a tenth of a
/// second, each with how far it stands out from the flux within
/// [`AROUND`] hops.
pub fn peaks(flux: &[f32]) -> Vec<Beat> {
    let n = flux.len();
    if n < 3 {
        return Vec::new();
    }
    let mut sums = vec![0.0f64; n + 1];
    let mut squares = vec![0.0f64; n + 1];
    for (i, &v) in flux.iter().enumerate() {
        sums[i + 1] = sums[i] + f64::from(v);
        squares[i + 1] = squares[i] + f64::from(v) * f64::from(v);
    }
    let mut out: Vec<Beat> = Vec::new();
    for i in 0..n {
        let v = flux[i];
        let (a, b) = (i.saturating_sub(APART / 2), (i + APART / 2 + 1).min(n));
        // The first of equal highest values.
        let highest = (a..b).all(|j| flux[j] < v || (flux[j] == v && j >= i));
        if !highest || v <= 0.0 {
            continue;
        }
        let (a, b) = (i.saturating_sub(AROUND), (i + AROUND + 1).min(n));
        let k = (b - a) as f64;
        let mean = (sums[b] - sums[a]) / k;
        let spread = ((squares[b] - squares[a]) / k - mean * mean)
            .max(0.0)
            .sqrt();
        let strength = ((f64::from(v) - mean) / spread.max(1e-9)) as f32;
        if strength <= 0.0 {
            continue;
        }
        let beat = Beat {
            time: i as f64 / HOPS_PER_SECOND,
            strength,
        };
        // Too near the one before: the stronger stays.
        match out.last_mut() {
            Some(last) if (beat.time - last.time) * HOPS_PER_SECOND < APART as f64 => {
                if beat.strength > last.strength {
                    *last = beat;
                }
            }
            _ => out.push(beat),
        }
    }
    out
}

/// How far a beat stands out at least for `sensitivity` (0–100, VEGAS's
/// slider: the higher, the fewer markers).
pub fn threshold(sensitivity: f64) -> f32 {
    (0.5 + 0.045 * sensitivity.clamp(0.0, 100.0)) as f32
}

/// The beats `sensitivity` keeps.
pub fn kept(beats: &[Beat], sensitivity: f64) -> Vec<Beat> {
    let t = threshold(sensitivity);
    beats.iter().copied().filter(|b| b.strength >= t).collect()
}

/// The slowest and fastest tempos looked for (beats a minute).
const SLOWEST: f64 = 60.0;
const FASTEST: f64 = 200.0;
/// The tempo people tap at, and how far (octaves) the weight spreads.
const TAPPED: f64 = 120.0;
const SPREAD: f64 = 0.9;
/// The autocorrelation a tempo needs to be believed (of its value at 0).
const MIN_REPEAT: f64 = 0.05;

/// The tempo the flux repeats at (beats a minute), `None` without a steady
/// one or with less than eight seconds of sound.
pub fn tempo(flux: &[f32]) -> Option<f64> {
    let n = flux.len();
    if (n as f64) < 8.0 * HOPS_PER_SECOND {
        return None;
    }
    // The flux less its mean over half a second each way: the onsets' shape.
    let half = (HOPS_PER_SECOND / 2.0) as usize;
    let mut sums = vec![0.0f64; n + 1];
    for (i, &v) in flux.iter().enumerate() {
        sums[i + 1] = sums[i] + f64::from(v);
    }
    let x: Vec<f64> = (0..n)
        .map(|i| {
            let (a, b) = (i.saturating_sub(half), (i + half + 1).min(n));
            (f64::from(flux[i]) - (sums[b] - sums[a]) / (b - a) as f64).max(0.0)
        })
        .collect();
    let r = |lag: usize| -> f64 {
        if lag >= n {
            return 0.0;
        }
        let s: f64 = x[..n - lag].iter().zip(&x[lag..]).map(|(a, b)| a * b).sum();
        s / (n - lag) as f64
    };
    let r0 = r(0);
    if r0 <= 0.0 {
        return None;
    }
    let bpm = |lag: f64| 60.0 * HOPS_PER_SECOND / lag;
    let weight = |lag: f64| {
        let o = (bpm(lag) / TAPPED).log2() / SPREAD;
        (-0.5 * o * o).exp()
    };
    let lo = (60.0 * HOPS_PER_SECOND / FASTEST).floor() as usize;
    let hi = (60.0 * HOPS_PER_SECOND / SLOWEST).ceil() as usize;
    let (best, _) = (lo..=hi)
        .map(|lag| {
            let score = weight(lag as f64) * (r(lag) + 0.5 * r(2 * lag)) / r0;
            (lag, score)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    if r(best) / r0 < MIN_REPEAT {
        return None;
    }
    // The peak between the hops, then on its largest multiple that fits a
    // quarter of the flux: four times as fine.
    let peak = |lag: usize| -> f64 {
        let (a, b, c) = (r(lag - 1), r(lag), r(lag + 1));
        let d = a - 2.0 * b + c;
        if d.abs() < 1e-12 {
            lag as f64
        } else {
            lag as f64 + (0.5 * (a - c) / d).clamp(-0.5, 0.5)
        }
    };
    let rough = peak(best);
    let times = ((n / 4) as f64 / rough).floor().clamp(1.0, 8.0) as usize;
    let lag = if times > 1 {
        let around = (rough * times as f64).round() as usize;
        let top = (around.saturating_sub(3).max(1)..=around + 3)
            .max_by(|&a, &b| r(a).total_cmp(&r(b)))
            .unwrap_or(around);
        peak(top) / times as f64
    } else {
        rough
    };
    Some(bpm(lag))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    /// A beat of `bpm` for `seconds`: kicks (a low thud dying away), a
    /// weaker one every other beat when `accents`, a hi-hat halfway between
    /// beats (high noise the flux leaves out) and a little noise all along.
    fn track(bpm: f64, seconds: f64, accents: bool) -> Vec<f32> {
        let period = 60.0 / bpm;
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut noise = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
        };
        let n = (seconds * f64::from(RATE)) as usize;
        let mut hat_prev = 0.0;
        (0..n)
            .map(|i| {
                let t = i as f64 / f64::from(RATE);
                let beat = (t / period).floor();
                let since = t - beat * period;
                let loud = if accents && (beat as i64) % 2 == 1 {
                    0.35
                } else {
                    1.0
                };
                let kick = loud
                    * 0.8
                    * (std::f64::consts::TAU * 60.0 * since).sin()
                    * (-since / 0.08).exp();
                let off = (t - period / 2.0).rem_euclid(period);
                let n = noise();
                // White noise first-differenced: mostly high frequencies.
                let hat = (n - hat_prev) * 0.15 * (-off / 0.03).exp();
                hat_prev = n;
                (kick + hat + 0.01 * noise()) as f32
            })
            .collect()
    }

    fn analyse(samples: &[f32]) -> Analysis {
        let mut f = LowFlux::new(RATE);
        for block in samples.chunks(4096) {
            f.push(block);
        }
        f.finish()
    }

    #[test]
    fn the_tempo_of_a_beat() {
        for bpm in [128.0, 95.0, 140.0, 70.0] {
            let t = analyse(&track(bpm, 30.0, false)).tempo().expect("a tempo");
            assert!((t - bpm).abs() < 0.1, "{bpm}: {t}");
        }
        // Silence has none.
        assert_eq!(analyse(&vec![0.0; RATE as usize * 10]).tempo(), None);
    }

    #[test]
    fn beats_fall_on_the_kicks() {
        let found = kept(&analyse(&track(120.0, 12.0, false)).beats(), 50.0);
        // A kick every half second; the first maybe missed at the start.
        assert!(found.len() >= 22 && found.len() <= 24, "{}", found.len());
        for b in &found {
            let off = (b.time + 0.25).rem_euclid(0.5) - 0.25;
            assert!(off.abs() < 0.004, "{b:?}");
        }
    }

    #[test]
    fn more_sensitivity_fewer_markers() {
        let all = analyse(&track(120.0, 12.0, true)).beats();
        let (many, few) = (kept(&all, 20.0).len(), kept(&all, 95.0).len());
        // All the kicks, then only the accented ones.
        assert!(many >= 22, "{many}");
        assert!((10..=12).contains(&few), "{few}");
    }
}
