//! The effects that change the sound's length, as VEGAS's: Time Stretch
//! (and its Express version), Pitch Shift, Gapper/Snipper and Vibrato.
//! They work on the whole sound at once (Non-Real-Time Event FX); in a
//! playing chain they let the sound through.

use bv_timeline::audio_fx::{
    gapper_snipper as gs, pitch_shift as ps, time_stretch as ts, vibrato as vb,
};
use bv_timeline::{Effect, EffectKind};

use super::graphs::{along, points};

/// The sound through `e` when it is one of these effects: a buffer a
/// channel, the result's length its own.
pub fn render(e: &Effect, rate: u32, channels: Vec<Vec<f32>>) -> Option<Vec<Vec<f32>>> {
    let params = super::params_of(e);
    let fs = rate.max(1) as f64;
    Some(match e.kind {
        EffectKind::TimeStretch | EffectKind::ExpressTimeStretch => {
            let p = |i: usize| value(&params, ts::PARAMS, i);
            let window = ts::WINDOWS[p(ts::MODE).round().clamp(0.0, 18.0) as usize];
            channels_map(channels, |x| {
                stretch(&x, p(ts::LENGTH) as f64 / 100.0, window, fs)
            })
        }
        EffectKind::GapperSnipper => {
            let p = |i: usize| value(&params, gs::PARAMS, i);
            gap_snip(
                channels,
                p(gs::FREQUENCY) as f64,
                p(gs::LENGTH) as f64,
                p(gs::FADE) as f64,
                p(gs::EFFECT) >= 0.5,
                fs,
            )
        }
        EffectKind::PitchShift => {
            let p = |i: usize| value(&params, ps::PARAMS, i);
            let ratio = 2f64.powf((p(ps::SEMITONES) as f64 + p(ps::CENTS) as f64 / 100.0) / 12.0);
            let points_used = [2, 9, 15][p(ps::ACCURACY).round().clamp(1.0, 3.0) as usize - 1];
            let anti_alias = p(ps::ANTI_ALIAS) >= 0.5 && ratio > 1.1;
            let keep = p(ps::PRESERVE) >= 0.5;
            let window = ts::WINDOWS[p(ps::MODE).round().clamp(0.0, 18.0) as usize];
            channels_map(channels, |x| {
                let x = if anti_alias {
                    low_pass(&x, 0.5 / ratio * 0.95)
                } else {
                    x
                };
                let shifted = resample(&x, ratio, points_used);
                if keep {
                    stretch(&shifted, ratio, window, fs)
                } else {
                    shifted
                }
            })
        }
        EffectKind::Vibrato => {
            let p = |i: usize| value(&params, vb::PARAMS, i);
            let mut wave = points(&params, vb::PARAMS, vb::GRAPH);
            if p(vb::BLEND) >= 0.5 && wave.len() >= 2 {
                let ends = (wave[0].1 + wave[wave.len() - 1].1) / 2.0;
                let (near, far) = (along(&wave, 50.0), along(&wave, 950.0));
                let mut blended = vec![(0.0, ends), (50.0, near)];
                blended.extend(wave.iter().copied().filter(|q| q.0 > 50.0 && q.0 < 950.0));
                blended.extend([(950.0, far), (1000.0, ends)]);
                wave = blended;
            }
            let semitones = p(vb::SEMITONES) as f64;
            let speeds: Vec<f64> = (0..1000)
                .map(|t| {
                    2f64.powf(
                        along(&wave, t as f32).clamp(-100.0, 100.0) as f64 / 100.0 * semitones
                            / 12.0,
                    )
                })
                .collect();
            let cycle = (fs / p(vb::FREQUENCY) as f64).trunc().max(1.0);
            let gain = if p(vb::OUTPUT) <= -80.0 {
                0.0
            } else {
                10f64.powf(p(vb::OUTPUT) as f64 / 20.0)
            };
            channels_map(channels, |x| vary(&x, &speeds, cycle, gain))
        }
        _ => return None,
    })
}

fn value(params: &[f32], specs: &[bv_timeline::ParamSpec], i: usize) -> f32 {
    let s = &specs[i];
    params
        .get(i)
        .copied()
        .unwrap_or(s.default)
        .clamp(s.min, s.max)
}

fn channels_map(channels: Vec<Vec<f32>>, f: impl Fn(Vec<f32>) -> Vec<f32>) -> Vec<Vec<f32>> {
    channels.into_iter().map(f).collect()
}

/// `x` lengthened `factor` times (or shortened below 1) without a change
/// of pitch: windows of `window_ms` laid down with half of them
/// overlapping, each taken where it best continues what was laid before
/// (within a third of a window), joined by straight crossfades.
pub(crate) fn stretch(x: &[f32], factor: f64, window_ms: f64, fs: f64) -> Vec<f32> {
    let w = ((window_ms / 1000.0 * fs) as usize).max(16);
    let hop_out = w / 2;
    let hop_in = hop_out as f64 / factor;
    let search = w / 3;
    let len = (x.len() as f64 * factor).round() as usize;
    let mut out = vec![0f32; len + w];
    let mut weight = vec![0f32; len + w];
    let at = |i: isize| -> f32 {
        if i < 0 || i as usize >= x.len() {
            0.0
        } else {
            x[i as usize]
        }
    };
    let mut k = 0usize;
    let mut prev_end: Option<isize> = None;
    while k * hop_out < len {
        let nominal = (k as f64 * hop_in) as isize;
        // The start that best follows on from the last window's tail.
        let start = match prev_end {
            None => nominal,
            Some(tail) => {
                let mut best = nominal;
                let mut best_score = f32::MIN;
                let span = hop_out.min(256);
                let step = (search / 64).max(1);
                let mut d = -(search as isize);
                while d <= search as isize {
                    let s = nominal + d;
                    let score: f32 = (0..span as isize)
                        .step_by(2)
                        .map(|j| at(s + j) * at(tail + j))
                        .sum();
                    if score > best_score {
                        best_score = score;
                        best = s;
                    }
                    d += step as isize;
                }
                best
            }
        };
        let o = k * hop_out;
        for j in 0..w {
            // A triangle: straight fades in and out over the overlap.
            let g = if j < hop_out {
                j as f32 / hop_out as f32
            } else {
                (w - j) as f32 / (w - hop_out) as f32
            };
            out[o + j] += g * at(start + j as isize);
            weight[o + j] += g;
        }
        prev_end = Some(start + hop_out as isize);
        k += 1;
    }
    out.truncate(len);
    for (o, wt) in out.iter_mut().zip(weight) {
        if wt > 1e-6 {
            *o /= wt;
        }
    }
    out
}

/// `x` read `ratio` times as fast (higher and shorter above 1), between
/// its samples by `taps` points (2: straight lines; more: a windowed sinc).
pub(crate) fn resample(x: &[f32], ratio: f64, taps: usize) -> Vec<f32> {
    let len = (x.len() as f64 / ratio).floor() as usize;
    let half = (taps / 2) as isize;
    (0..len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let base = pos.floor() as isize;
            let frac = pos - base as f64;
            if taps <= 2 {
                let a = x.get(base as usize).copied().unwrap_or(0.0);
                let b = x.get(base as usize + 1).copied().unwrap_or(0.0);
                return a + (b - a) * frac as f32;
            }
            let mut sum = 0.0;
            for k in (1 - half)..=half {
                let t = k as f64 - frac;
                let s = x.get((base + k) as usize).copied().unwrap_or(0.0) as f64;
                let sinc = if t.abs() < 1e-9 {
                    1.0
                } else {
                    (std::f64::consts::PI * t).sin() / (std::f64::consts::PI * t)
                };
                let win = 0.5 + 0.5 * (std::f64::consts::PI * t / (half as f64 + 1.0)).cos();
                sum += s * sinc * win;
            }
            sum as f32
        })
        .collect()
}

/// A low-pass at `cutoff` (a part of the rate), eight second-order
/// sections in a row, run forward.
fn low_pass(x: &[f32], cutoff: f64) -> Vec<f32> {
    let w = std::f64::consts::TAU * cutoff.clamp(0.001, 0.49);
    let (s, c) = w.sin_cos();
    let alpha = s / std::f64::consts::SQRT_2;
    let a0 = 1.0 + alpha;
    let (b0, b1, a1, a2) = (
        (1.0 - c) / 2.0 / a0,
        (1.0 - c) / a0,
        -2.0 * c / a0,
        (1.0 - alpha) / a0,
    );
    let mut y: Vec<f64> = x.iter().map(|v| *v as f64).collect();
    for _ in 0..8 {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        for v in y.iter_mut() {
            let out = b0 * *v + b1 * x1 + b0 * x2 - a1 * y1 - a2 * y2;
            x2 = x1;
            x1 = *v;
            y2 = y1;
            y1 = out;
            *v = out;
        }
    }
    y.into_iter().map(|v| v as f32).collect()
}

/// Gapper/Snipper: each period of the sound (faded in and out over its
/// edges, a share of the period) is followed by an event's length of
/// silence (the gapper) or loses the event's length that follows (the
/// snipper).
pub(crate) fn gap_snip(
    channels: Vec<Vec<f32>>,
    freq: f64,
    length: f64,
    fade: f64,
    snip: bool,
    fs: f64,
) -> Vec<Vec<f32>> {
    let period = ((fs / freq.max(1e-4)).trunc() as usize).max(1);
    let event = ((fs * length).trunc() as usize).max(1);
    let edge = ((fade * period as f64 / 100.0).trunc() as usize).max(1);
    channels
        .into_iter()
        .map(|x| {
            let mut out = Vec::with_capacity(x.len());
            let mut i = 0;
            while i < x.len() {
                let take = (period + 1).min(x.len() - i);
                for j in 0..take {
                    let g = if j < edge {
                        j as f32 / edge as f32
                    } else if j + edge > period {
                        (period + 1 - j) as f32 / edge as f32
                    } else {
                        1.0
                    };
                    out.push(x[i + j] * g.min(1.0));
                }
                i += take;
                if snip {
                    i += event;
                } else if i < x.len() {
                    out.extend(std::iter::repeat_n(0.0, event));
                }
            }
            out
        })
        .collect()
}

/// Vibrato: the sound read at a speed that follows its wave (`speeds`, a
/// round of 1000 steps lasting `cycle` samples of the result), between
/// its samples by straight lines, at `gain`; it ends when the sound does.
pub(crate) fn vary(x: &[f32], speeds: &[f64], cycle: f64, gain: f64) -> Vec<f32> {
    let mut out = Vec::with_capacity(x.len());
    let mut pos = 0.0f64;
    let mut phase = 0.0f64;
    let step = 1000.0 / cycle;
    while (pos as usize) + 1 < x.len() {
        let i = pos as usize;
        let frac = (pos - i as f64) as f32;
        out.push(((x[i] + (x[i + 1] - x[i]) * frac) as f64 * gain) as f32);
        pos += speeds[(phase as usize).min(speeds.len() - 1)];
        phase += step;
        if phase >= 1000.0 {
            phase -= 1000.0;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f64, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (std::f64::consts::TAU * hz * i as f64 / 48_000.0).sin() as f32)
            .collect()
    }

    /// The frequency of `x` from its zero crossings.
    fn pitch(x: &[f32]) -> f64 {
        let crossings = x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        crossings as f64 * 48_000.0 / x.len() as f64
    }

    #[test]
    fn stretching_changes_the_length_and_keeps_the_pitch() {
        let x = sine(440.0, 48_000);
        let y = stretch(&x, 1.5, 100.0, 48_000.0);
        assert_eq!(y.len(), 72_000);
        let p = pitch(&y[4_800..67_200]);
        assert!((p - 440.0).abs() < 8.0, "{p}");
    }

    #[test]
    fn resampling_raises_the_pitch_and_shortens() {
        let x = sine(440.0, 48_000);
        let y = resample(&x, 2.0, 9);
        assert_eq!(y.len(), 24_000);
        let p = pitch(&y[1_000..23_000]);
        assert!((p - 880.0).abs() < 8.0, "{p}");
    }

    #[test]
    fn the_gapper_inserts_and_the_snipper_cuts() {
        let x = vec![vec![1.0f32; 48_000]];
        let g = gap_snip(x.clone(), 10.0, 0.05, 10.0, false, 48_000.0);
        // Ten periods of 4801 samples, each with 2400 of silence after it
        // (but the last).
        assert!(g[0].len() > 48_000 + 9 * 2_400 - 10, "{}", g[0].len());
        let s = gap_snip(x, 10.0, 0.05, 10.0, true, 48_000.0);
        assert!(s[0].len() < 48_000 * 2 / 3 + 4_801, "{}", s[0].len());
    }

    #[test]
    fn a_flat_vibrato_an_octave_up_halves_the_length() {
        let x = sine(440.0, 48_000);
        let speeds = vec![2.0; 1000];
        let y = vary(&x, &speeds, 48_000.0, 1.0);
        assert!((y.len() as i32 - 24_000).abs() <= 1, "{}", y.len());
        let p = pitch(&y[1_000..23_000]);
        assert!((p - 880.0).abs() < 8.0, "{p}");
    }
}
