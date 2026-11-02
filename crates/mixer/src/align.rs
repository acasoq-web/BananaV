//! Where two recordings of one sound line up (Tools › Multicamera › Sync
//! Mixed Audio to Align Events). Each recording becomes an envelope of
//! its onsets — the rises of its loudness, `HOPS_PER_SECOND` a second,
//! which speech and music keep whatever microphone heard them — and the
//! envelopes are compared at every offset at once through the FFT
//! ([`Matcher`]); the best offset is then refined on the sound itself, to
//! a fraction of a sample at `FINE_RATE` ([`refine`]).

use crate::fft::{self, Complex};

/// The envelopes' steps a second.
pub const HOPS_PER_SECOND: u32 = 200;

/// The rate the sound itself is compared at, in the last step.
pub const FINE_RATE: u32 = 8000;

/// When a fit is believed. The best offset must stand above the others
/// (by `MIN_PROMINENCE` standard deviations of the correlation over all
/// of them) and either beat the next best one — a fifth of a second or
/// more away — by `MIN_MARGIN` times, or match almost exactly
/// (`SURE_SCORE`: a steady beat fits every beat nearly as well, but only
/// its own exactly). Measured on BananaV's test sounds: one sound heard by
/// two microphones through noise beats the next offset by 2 to 9 times,
/// a steady beat matches its own place to 0.97; sounds that are not the
/// same beat it by at most 1.6 and match to at most 0.6 (two seconds of
/// one in another).
pub const MIN_PROMINENCE: f64 = 6.0;
pub const MIN_MARGIN: f64 = 1.8;
pub const SURE_SCORE: f64 = 0.8;

/// The first difference that takes a recording's rumble away before its
/// power is measured: `y = x − PRE_EMPHASIS · previous x`.
pub const PRE_EMPHASIS: f32 = 0.97;

/// Onset strengths from each hop's mean power: the rises of the power's
/// logarithm (floored 30 dB under the mean power, so the noise of silence
/// does not count), less their mean.
pub fn onsets(power: &[f32]) -> Vec<f64> {
    if power.is_empty() {
        return Vec::new();
    }
    let mean = power.iter().map(|&p| p as f64).sum::<f64>() / power.len() as f64;
    let floor = (mean * 1e-3).max(1e-12);
    let mut out = Vec::with_capacity(power.len());
    let mut prev = (power[0] as f64 + floor).ln();
    out.push(0.0);
    for &p in &power[1..] {
        let e = (p as f64 + floor).ln();
        out.push((e - prev).max(0.0));
        prev = e;
    }
    let m = out.iter().sum::<f64>() / out.len() as f64;
    for v in &mut out {
        *v -= m;
    }
    out
}

/// Where a recording's envelope fits the target's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    /// In hops (with a fraction): the recording's hop `i` matches the
    /// target's hop `i + at`.
    pub at: f64,
    /// How far the match stands above the other offsets, in standard
    /// deviations.
    pub prominence: f64,
    /// How alike the two envelopes are where they overlap there: their
    /// correlation, up to 1.
    pub score: f64,
    /// How many times the next best offset's score this one is.
    pub margin: f64,
}

/// Compares envelopes with the target's, its spectrum worked out once.
#[derive(Debug)]
pub struct Matcher {
    n: usize,
    target: Vec<Complex>,
    len: usize,
    /// The sums of the target's squares up to each hop.
    energy: Vec<f64>,
}

impl Matcher {
    /// For the target's envelope and others up to `longest` hops long.
    pub fn new(target: &[f64], longest: usize) -> Matcher {
        let n = (target.len() + longest).next_power_of_two().max(2);
        Matcher {
            n,
            target: fft::spectrum(target, n),
            len: target.len(),
            energy: squares(target),
        }
    }

    /// Where `other` fits the target best: the offset where the two
    /// envelopes correlate best over their overlap — at least
    /// `min_overlap` hops and a quarter of the shorter, so that a short
    /// stretch that happens to look alike does not win —, which tells a
    /// steady beat's beats apart where the correlation's sum alone would
    /// not. `None` when no offset stands out (`MIN_PROMINENCE`,
    /// `MIN_MARGIN`, `SURE_SCORE`).
    pub fn fit(&self, other: &[f64], min_overlap: usize) -> Option<Fit> {
        if other.is_empty() || self.len == 0 || self.len + other.len() > self.n {
            return None;
        }
        let r = fft::correlate(&self.target, fft::spectrum(other, self.n));
        let (na, nb, n) = (self.len as i64, other.len() as i64, self.n as i64);
        let theirs = squares(other);
        let shorter = na.min(nb);
        let min = (min_overlap as i64).max(shorter / 4).min(shorter).max(1);
        // The correlation at offset `k` over the overlap (`None`: too
        // little overlap).
        let score = |k: i64| -> Option<f64> {
            let (b0, b1) = (0.max(-k), nb.min(na - k));
            if b1 - b0 < min {
                return None;
            }
            let ea = self.energy[(b1 + k) as usize] - self.energy[(b0 + k) as usize];
            let eb = theirs[b1 as usize] - theirs[b0 as usize];
            let e = ea * eb;
            Some(if e > 0.0 {
                r[k.rem_euclid(n) as usize] / e.sqrt()
            } else {
                0.0
            })
        };
        let (mut sum, mut sum2, mut count) = (0.0, 0.0, 0usize);
        let mut best: Option<(i64, f64)> = None;
        for k in -(nb - 1)..na {
            let Some(v) = score(k) else {
                continue;
            };
            sum += v;
            sum2 += v * v;
            count += 1;
            if best.is_none_or(|(_, b)| v > b) {
                best = Some((k, v));
            }
        }
        let (k, peak) = best?;
        if count < 3 {
            return None;
        }
        let mean = sum / count as f64;
        let sd = (sum2 / count as f64 - mean * mean).max(0.0).sqrt();
        if sd <= 0.0 {
            return None;
        }
        let prominence = (peak - mean) / sd;
        let second = (-(nb - 1)..na)
            .filter(|&j| (j - k).abs() > HOPS_PER_SECOND as i64 / 5)
            .filter_map(score)
            .fold(f64::MIN, f64::max);
        let margin = if second > 0.0 {
            peak / second
        } else {
            f64::INFINITY
        };
        if prominence < MIN_PROMINENCE || (margin < MIN_MARGIN && peak < SURE_SCORE) {
            return None;
        }
        let frac = match (score(k - 1), score(k + 1)) {
            (Some(l), Some(r)) => vertex(l, peak, r),
            _ => 0.0,
        };
        Some(Fit {
            at: k as f64 + frac,
            prominence,
            score: peak,
            margin,
        })
    }
}

/// The sums of `x`'s squares up to each element: element `i` sums the
/// first `i`.
fn squares(x: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(x.len() + 1);
    let mut acc = 0.0;
    out.push(0.0);
    for &v in x {
        acc += v * v;
        out.push(acc);
    }
    out
}

/// Where between −½ and ½ the parabola through three samples around a
/// peak has its top.
fn vertex(left: f64, mid: f64, right: f64) -> f64 {
    let den = left - 2.0 * mid + right;
    if den < 0.0 {
        (0.5 * (left - right) / den).clamp(-0.5, 0.5)
    } else {
        0.0
    }
}

/// The fine step. `x` is the target's sound and `y` the recording's, both
/// at `FINE_RATE`, `y` placed by the coarse fit and `reach` samples longer
/// on each side. Returns how many samples (with a fraction) the recording
/// must move by and how well the two then match (a correlation, up to 1);
/// `None` when the best match is at the edge of the reach (the coarse fit
/// was off) or either is silent.
pub fn refine(x: &[f32], y: &[f32], reach: usize) -> Option<(f64, f64)> {
    let w = x.len();
    if w == 0 || y.len() < w + 2 * reach {
        return None;
    }
    let ex: f64 = x.iter().map(|&v| (v as f64) * (v as f64)).sum();
    if ex <= 0.0 {
        return None;
    }
    let scores: Vec<f64> = (0..=2 * reach)
        .map(|d| {
            let seg = &y[d..d + w];
            let (mut dot, mut ey) = (0.0, 0.0);
            for (&a, &b) in x.iter().zip(seg) {
                dot += a as f64 * b as f64;
                ey += b as f64 * b as f64;
            }
            if ey > 0.0 {
                dot / (ex * ey).sqrt()
            } else {
                0.0
            }
        })
        .collect();
    let (i, best) = scores
        .iter()
        .copied()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    if i == 0 || i == scores.len() - 1 || best <= 0.0 {
        return None;
    }
    let lag = i as f64 + vertex(scores[i - 1], best, scores[i + 1]) - reach as f64;
    // The recording's sound `lag` samples later matches the target's here:
    // it moves back by as much.
    Some((-lag, best))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sound like speech: bursts of noise of varying loudness and length
    /// over a little hiss, `seconds` long at `rate` (repeatable: `seed`).
    fn speech(seconds: f64, rate: u32, seed: u64) -> Vec<f32> {
        let mut state = seed | 1;
        let mut rnd = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f64 / (1u64 << 53) as f64
        };
        let n = (seconds * rate as f64) as usize;
        let mut out = Vec::with_capacity(n);
        let mut level = 0.0;
        let mut left = 0usize;
        for _ in 0..n {
            if left == 0 {
                level = if rnd() < 0.35 { 0.0 } else { 0.1 + rnd() * 0.8 };
                left = (rate as f64 * (0.05 + rnd() * 0.3)) as usize;
            }
            left -= 1;
            out.push(((rnd() - 0.5) * 2.0 * level + (rnd() - 0.5) * 0.01) as f32);
        }
        out
    }

    /// Mean power of each hop of `x` (at `rate`), after the first
    /// difference — what the engine's reading does.
    fn powers(x: &[f32], rate: u32) -> Vec<f32> {
        let hop = (rate / HOPS_PER_SECOND) as usize;
        let mut prev = 0.0;
        x.chunks(hop)
            .map(|c| {
                let mut acc = 0.0f64;
                for &s in c {
                    let y = s - PRE_EMPHASIS * prev;
                    prev = s;
                    acc += (y * y) as f64;
                }
                (acc / c.len() as f64) as f32
            })
            .collect()
    }

    #[test]
    fn a_later_recording_of_the_same_sound_is_found() {
        let rate = 8000;
        let sound = speech(60.0, rate, 7);
        // The second microphone started 12.34 s later, hears it quieter
        // and with noise of its own.
        let start = (12.34 * rate as f64) as usize;
        let mut other: Vec<f32> = sound[start..start + 30 * rate as usize]
            .iter()
            .map(|&s| s * 0.3)
            .collect();
        let noise = speech(30.0, rate, 99);
        for (o, n) in other.iter_mut().zip(&noise) {
            *o += n * 0.05;
        }
        let a = onsets(&powers(&sound, rate));
        let b = onsets(&powers(&other, rate));
        let fit = Matcher::new(&a, b.len())
            .fit(&b, 200)
            .expect("the same sound fits");
        let seconds = fit.at / HOPS_PER_SECOND as f64;
        assert!((seconds - 12.34).abs() < 0.006, "{seconds}");
        assert!(fit.margin > 2.0 && fit.score > 0.5, "{fit:?}");
        // The fine step gets the rest: the coarse fit off by 3 ms.
        let from = 20 * rate as usize;
        let w = 4 * rate as usize;
        let reach = 100;
        let x = &sound[from..from + w];
        let placed = from - start + 24 - reach;
        let y = &other[placed..placed + w + 2 * reach];
        let (shift, score) = refine(x, y, reach).expect("a fine match");
        assert!((shift - 24.0).abs() < 0.5, "{shift}");
        assert!(score > 0.9, "{score}");
    }

    #[test]
    fn a_steady_beat_fits_where_it_was() {
        let rate = 8000;
        // Notes struck every half second, each as loud as the music has it
        // there, with a little noise: which beat is which only shows in
        // their loudness.
        let mut rnd = 1u64;
        let mut next = move || {
            rnd = rnd
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (rnd >> 40) as f64 / (1u64 << 24) as f64
        };
        let accents: Vec<f64> = (0..240).map(|_| 0.3 + 0.7 * next()).collect();
        let beat: Vec<f32> = (0..120 * rate as usize)
            .map(|i| {
                let t = i as f64 / rate as f64;
                let env = (-(t % 0.5) / 0.5 * 12.0).exp() * accents[(t / 0.5) as usize];
                let tone = (t * 440.0 * std::f64::consts::TAU).sin();
                ((tone * 0.3 + (next() - 0.5) * 0.4) * env) as f32
            })
            .collect();
        let start = 13 * rate as usize + 1234;
        let other: Vec<f32> = beat[start..start + 60 * rate as usize]
            .iter()
            .map(|&s| s * 0.5)
            .collect();
        let a = onsets(&powers(&beat, rate));
        let b = onsets(&powers(&other, rate));
        let fit = Matcher::new(&a, b.len()).fit(&b, 200).expect("it fits");
        let seconds = fit.at / HOPS_PER_SECOND as f64;
        assert!(
            (seconds - start as f64 / rate as f64).abs() < 0.002,
            "{seconds}"
        );
    }

    #[test]
    fn unrelated_sounds_do_not_fit() {
        let rate = 8000;
        let a = onsets(&powers(&speech(40.0, rate, 3), rate));
        let b = onsets(&powers(&speech(20.0, rate, 4), rate));
        assert_eq!(Matcher::new(&a, b.len()).fit(&b, 200), None);
    }
}
