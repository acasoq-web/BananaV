//! Stretching an audio event without changing its pitch (Options ›
//! Preferences › Editing: "Preserve pitch when stretching audio events"):
//! WSOLA. The media is cut into overlapping grains laid out at the event's
//! speed, each one moved a little in the media to line up with the sound
//! the grain before it would go on with, so that no beating comes out of
//! the overlaps. An event's pitch change (`Clip::pitch`) reads each grain
//! that many times faster (resampled within it): the grains still follow
//! the event's speed, so its length stays.
//!
//! A grain's shift depends only on where the grain is: any block of an
//! event comes out the same however playback got there. The shifts are
//! worked out in runs of [`RUN`] grains, each from [`RUN`] grains before it
//! (shift zero there), and kept for the blocks that follow.

use std::collections::HashMap;

use bv_core::{SampleRate, TICKS_PER_SECOND, Tick};
use bv_media::{AudioReader, MediaError};
use bv_timeline::Clip;

/// Grains whose shifts are worked out together.
const RUN: i64 = 8;
/// Runs kept per event; past this the farthest ones go.
const KEPT_RUNS: usize = 64;

/// The grains' hop at `rate`: about 10.7 ms (512 samples at 48 kHz). A
/// grain is two hops long.
fn hop(rate: SampleRate) -> i64 {
    ((rate.0 as f64 * 0.0107).round() as i64).max(16)
}

/// The shifts of an event's grains, run by run.
#[derive(Debug, Default)]
pub(crate) struct Shifts {
    /// What they were worked out for: the event's place, media and speed,
    /// and the laps of its loop already played.
    key: Option<(Tick, Tick, u64, Tick)>,
    runs: HashMap<i64, Vec<i32>>,
}

impl Shifts {
    fn for_clip(&mut self, clip: &Clip, shift: Tick) {
        let key = (
            clip.start,
            clip.source_in,
            clip.rate.to_bits() ^ u64::from(clip.pitch.to_bits()),
            shift,
        );
        if self.key != Some(key) {
            self.key = Some(key);
            self.runs.clear();
        }
    }
}

/// The media around a stretch: its samples at the project's rate from
/// `first` (`u` positions), all channels and their sum.
struct Media {
    first: i64,
    channels: Vec<Vec<f32>>,
    mono: Vec<f32>,
}

impl Media {
    /// Media samples `[first, first + len)` at the project's `rate` (Catmull-Rom
    /// interpolation from the media's own rate).
    fn read(
        reader: &mut dyn AudioReader,
        first: i64,
        len: usize,
        rate: SampleRate,
    ) -> Result<Media, MediaError> {
        let src_rate = reader.sample_rate();
        let channels = if src_rate == rate {
            reader.read(first, len)?.channels
        } else {
            let step = src_rate.0 as f64 / rate.0 as f64;
            let a = (first as f64 * step).floor() as i64 - 1;
            let b = ((first + len as i64) as f64 * step).ceil() as i64 + 2;
            let src = reader.read(a, (b - a + 1) as usize)?;
            src.channels
                .iter()
                .map(|ch| {
                    let at = |j: i64| ch[j.clamp(0, ch.len() as i64 - 1) as usize];
                    (0..len)
                        .map(|i| {
                            let p = (first + i as i64) as f64 * step;
                            let k = p.floor() as i64 - a;
                            let f = (p - p.floor()) as f32;
                            let (y0, y1, y2, y3) = (at(k - 1), at(k), at(k + 1), at(k + 2));
                            let c1 = 0.5 * (y2 - y0);
                            let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
                            let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
                            ((c3 * f + c2) * f + c1) * f + y1
                        })
                        .collect()
                })
                .collect()
        };
        let mono = (0..len)
            .map(|i| {
                channels
                    .iter()
                    .map(|c| c.get(i).copied().unwrap_or(0.0))
                    .sum()
            })
            .collect();
        Ok(Media {
            first,
            channels,
            mono,
        })
    }

    fn mono_at(&self, u: i64) -> f32 {
        self.mono
            .get((u - self.first) as usize)
            .copied()
            .unwrap_or(0.0)
    }
}

/// Where grain `k` of the event sits: its center in the output (project
/// samples) and in the media (project-rate samples, before its shift).
struct Grid<'a> {
    clip: &'a Clip,
    rate: SampleRate,
    /// The event's first sample.
    first: i64,
    hop: i64,
    shift: Tick,
}

impl Grid<'_> {
    fn center(&self, k: i64) -> i64 {
        self.first + k * self.hop
    }

    fn media(&self, k: i64) -> i64 {
        let t = self
            .clip
            .source_time(self.rate.sample_start(self.center(k)))
            - self.shift;
        (t.0 as f64 * self.rate.0 as f64 / TICKS_PER_SECOND as f64).round() as i64
    }
}

/// The shift (within ±hop/2) that best lines grain `k` (media center
/// `center`) up with the sound following the grain before it, which was
/// played from `prev`: correlation over the overlap, coarse then fine;
/// each read `pitch` times faster (a pitch change).
fn best_shift(m: &Media, hop: i64, center: i64, prev: i64, pitch: f64) -> i32 {
    let reach = hop / 2;
    let at = |base: i64, j: i64| base + (j as f64 * pitch).round() as i64;
    let score = |d: i64, stride: usize| -> f32 {
        let (mut dot, mut energy) = (0.0f32, 0.0f32);
        for j in (0..hop).step_by(stride) {
            let a = m.mono_at(at(center + d, j - hop));
            dot += a * m.mono_at(at(prev, j));
            energy += a * a;
        }
        if energy > 1e-12 {
            dot / energy.sqrt()
        } else {
            0.0
        }
    };
    let pick = |candidates: &mut dyn Iterator<Item = i64>, stride: usize| {
        let mut best: Option<(f32, i64)> = None;
        for d in candidates {
            let s = score(d, stride);
            // Ties keep the smallest shift: over silence grains stay put.
            let better =
                best.is_none_or(|(bs, bd)| s > bs + 1e-6 || (s > bs - 1e-6 && d.abs() < bd.abs()));
            if better {
                best = Some((s, d));
            }
        }
        best.map_or(0, |b| b.1)
    };
    let coarse = pick(&mut (-reach..=reach).step_by(4), 2);
    let fine = pick(
        &mut (coarse - 3..=coarse + 3).filter(|d| d.abs() <= reach),
        1,
    );
    fine as i32
}

/// Reads `n` samples of a stretched event from project sample `s0`, its
/// pitch kept; `shift`: the laps of its loop already played.
pub(crate) fn read(
    reader: &mut dyn AudioReader,
    shifts: &mut Shifts,
    clip: &Clip,
    rate: SampleRate,
    s0: i64,
    n: usize,
    shift: Tick,
) -> Result<Vec<Vec<f32>>, MediaError> {
    shifts.for_clip(clip, shift);
    let hop = hop(rate);
    // Each grain's samples this many times faster (a pitch change).
    let pitch = clip.pitch_factor().clamp(0.05, 16.0);
    let span = (hop as f64 * pitch).ceil() as i64;
    let grid = Grid {
        clip,
        rate,
        first: {
            let s = rate.sample_at(clip.start);
            if rate.sample_start(s) < clip.start {
                s + 1
            } else {
                s
            }
        },
        hop,
        shift,
    };
    let s1 = s0 + n as i64;
    // Grain k sounds over [center - hop, center + hop).
    let k_first = (s0 - grid.first).div_euclid(hop).max(0);
    let k_last = (s1 - grid.first + hop - 1).div_euclid(hop).max(k_first);
    let (r_first, r_last) = (k_first.div_euclid(RUN), k_last.div_euclid(RUN));
    let missing: Vec<i64> = (r_first..=r_last)
        .filter(|r| !shifts.runs.contains_key(r))
        .collect();
    // The media the grains (and the runs to work out) read.
    let from = missing
        .first()
        .map_or(k_first, |&r| ((r - 1) * RUN).max(0))
        .min(k_first);
    let to = missing
        .last()
        .map_or(k_last, |&r| (r + 1) * RUN - 1)
        .max(k_last);
    let reach = hop / 2;
    let (lo, hi) = (grid.media(from), grid.media(to));
    let first = lo.min(hi) - reach - hop.max(span) - 2;
    let len = (lo.max(hi) - first + reach + 2 * hop.max(span) + 4) as usize;
    let media = Media::read(reader, first, len, rate)?;
    for r in missing {
        let start = ((r - 1) * RUN).max(0);
        let mut d = 0i32;
        let mut run = Vec::with_capacity(RUN as usize);
        for k in start..(r + 1) * RUN {
            if k > start {
                let prev = grid.media(k - 1) + d as i64;
                d = best_shift(&media, hop, grid.media(k), prev, pitch);
            }
            if k >= r * RUN {
                run.push(d);
            }
        }
        if shifts.runs.len() >= KEPT_RUNS {
            // The farthest from here go first.
            let mut keys: Vec<i64> = shifts.runs.keys().copied().collect();
            keys.sort_by_key(|&q| std::cmp::Reverse((q - r).abs()));
            for q in keys.into_iter().take(KEPT_RUNS / 4) {
                shifts.runs.remove(&q);
            }
        }
        shifts.runs.insert(r, run);
    }
    // Overlap-add: sin² windows two hops long sum to one at a one-hop step.
    let mut out = vec![vec![0.0f32; n]; media.channels.len()];
    for k in k_first..=k_last {
        let d = shifts
            .runs
            .get(&k.div_euclid(RUN))
            .and_then(|run| run.get(k.rem_euclid(RUN) as usize))
            .copied()
            .unwrap_or(0) as i64;
        let (center, from) = (grid.center(k), grid.media(k) + d);
        for j in 0..2 * hop {
            let o = center - hop + j - s0;
            if o < 0 || o >= n as i64 {
                continue;
            }
            let w = (std::f32::consts::PI * j as f32 / (2 * hop) as f32)
                .sin()
                .powi(2);
            if pitch == 1.0 {
                let u = from - hop + j - media.first;
                if u < 0 {
                    continue;
                }
                for (dst, src) in out.iter_mut().zip(&media.channels) {
                    dst[o as usize] += w * src.get(u as usize).copied().unwrap_or(0.0);
                }
                continue;
            }
            // Its sample between two of the media's, around its center.
            let u = (from - media.first) as f64 + (j - hop) as f64 * pitch;
            if u < 0.0 {
                continue;
            }
            let (i, t) = (u.floor() as usize, (u - u.floor()) as f32);
            for (dst, src) in out.iter_mut().zip(&media.channels) {
                let a = src.get(i).copied().unwrap_or(0.0);
                let b = src.get(i + 1).copied().unwrap_or(0.0);
                dst[o as usize] += w * (a + (b - a) * t);
            }
        }
    }
    Ok(out)
}
