//! Auto Ducking, as VEGAS's: a controller track's sound lowers the
//! listener tracks' — by its Floor Reduction as long as it sounds, and by
//! its Peak Reduction more as it gets louder. Each of the controller's
//! events keeps its sound's loudness as it was read ([`DuckLevels`], in its
//! media's time), so the reduction goes wherever the event goes, and the
//! reductions change without reading the sound again.

use bv_core::Tick;
use serde::{Deserialize, Serialize};

use crate::{Project, Track, TrackKind};

/// How many loudness levels a second an analysis keeps.
pub const LEVELS_PER_SECOND: f64 = 25.0;

/// Under this level (dBFS) the controller counts as silent; this many dB
/// above it, as sounding.
const GATE_DB: f32 = -50.0;
const GATE_SPAN_DB: f32 = 10.0;
/// Peak Reduction counts the level above this (dBFS).
const PEAK_FROM_DB: f32 = -30.0;
/// The most a listener goes down (dB).
const DEEPEST_DB: f32 = -60.0;
/// A level the analysis keeps, at least (dBFS).
const QUIETEST_DB: f32 = -100.0;

/// A controller track's reductions (or an event's own).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ducking {
    /// Floor Reduction: how far the listeners go down while it sounds (dB,
    /// 0 or less).
    pub floor_db: f32,
    /// Peak Reduction: how much further they go down with its loudness
    /// (0–1: at 1, a dB more for each dB it is above −30 dBFS).
    pub peak: f32,
}

impl Default for Ducking {
    /// VEGAS's example for narration: −12 dB, no Peak Reduction.
    fn default() -> Ducking {
        Ducking {
            floor_db: -12.0,
            peak: 0.0,
        }
    }
}

impl Ducking {
    /// The reduction (dB, 0 or less) a controller sounding at `level`
    /// dBFS gives its listeners.
    pub fn reduction_db(&self, level: f32) -> f32 {
        let x = ((level - GATE_DB) / GATE_SPAN_DB).clamp(0.0, 1.0);
        let sounding = x * x * (3.0 - 2.0 * x);
        let peak = self.peak.clamp(0.0, 1.0) * (level - PEAK_FROM_DB).max(0.0);
        (self.floor_db.min(0.0) * sounding - peak * sounding).max(DEEPEST_DB)
    }
}

/// An event's loudness as Auto Ducking read it, and its own reductions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DuckLevels {
    /// The media time of the first level.
    pub from: Tick,
    /// The level (dBFS, rounded; quick to rise, slow to fall) every
    /// 1 ⁄ [`LEVELS_PER_SECOND`] of a second of the media.
    pub levels: Vec<i8>,
    /// Its own reductions (`None`: its track's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own: Option<Ducking>,
}

impl DuckLevels {
    /// The level at media time `m` (dBFS): between its values, silence
    /// outside them.
    pub fn level_at(&self, m: Tick) -> f32 {
        let f = (m - self.from).as_seconds_f64() * LEVELS_PER_SECOND;
        if f < 0.0 || self.levels.is_empty() {
            return QUIETEST_DB;
        }
        let i = f.floor() as usize;
        let Some(&a) = self.levels.get(i) else {
            return QUIETEST_DB;
        };
        let b = self.levels.get(i + 1).copied().unwrap_or(a);
        let k = (f - i as f64) as f32;
        f32::from(a) + (f32::from(b) - f32::from(a)) * k
    }
}

/// Reads a sound's loudness as it comes in blocks (its channels mixed):
/// [`LevelMeter::push`] each block, then [`LevelMeter::finish`].
#[derive(Clone, Debug)]
pub struct LevelMeter {
    /// Samples a level, and when the next level is taken.
    hop: f64,
    next: f64,
    taken: f64,
    squares: f64,
    count: u32,
    smooth: f32,
    levels: Vec<i8>,
}

impl LevelMeter {
    /// For a sound of `rate` samples a second.
    pub fn new(rate: u32) -> LevelMeter {
        let hop = (f64::from(rate.max(1)) / LEVELS_PER_SECOND).max(1.0);
        LevelMeter {
            hop,
            next: hop,
            taken: 0.0,
            squares: 0.0,
            count: 0,
            smooth: QUIETEST_DB,
            levels: Vec::new(),
        }
    }

    pub fn push(&mut self, samples: &[f32]) {
        for &s in samples {
            self.squares += f64::from(s) * f64::from(s);
            self.count += 1;
            self.taken += 1.0;
            if self.taken >= self.next {
                let mean = self.squares / f64::from(self.count.max(1));
                let db = (10.0 * (mean + 1e-10).log10()) as f32;
                // Quick to rise (about 10 ms), slow to fall (about 300 ms).
                let k = if db > self.smooth { 0.98 } else { 0.125 };
                self.smooth += (db - self.smooth) * k;
                self.levels
                    .push(self.smooth.clamp(QUIETEST_DB, 0.0).round() as i8);
                self.squares = 0.0;
                self.count = 0;
                self.next += self.hop;
            }
        }
    }

    /// The levels, the first at media time `from`.
    pub fn finish(self, from: Tick) -> DuckLevels {
        DuckLevels {
            from,
            levels: self.levels,
            own: None,
        }
    }
}

impl Project {
    /// The gain (linear) Auto Ducking gives listener track `track` at `t`:
    /// the deepest reduction of the controllers sounding then (1 when it
    /// is no listener, or none sounds).
    pub fn ducking_gain(&self, track: &Track, t: Tick) -> f32 {
        if !track.duck_listener {
            return 1.0;
        }
        let mut db = 0.0f32;
        for c in self
            .tracks
            .iter()
            .filter(|c| c.kind == TrackKind::Audio && c.id != track.id && !c.muted)
        {
            let Some(d) = c.ducking else {
                continue;
            };
            for clip in c.clips_at(t).filter(|k| !k.muted) {
                let Some(l) = &clip.duck_levels else {
                    continue;
                };
                let level = l.level_at(clip.source_time(t));
                db = db.min(l.own.unwrap_or(d).reduction_db(level));
            }
        }
        if db >= 0.0 {
            1.0
        } else {
            10f32.powf(db / 20.0)
        }
    }

    /// Whether some track is a listener with a controller to listen to.
    pub fn has_ducking(&self) -> bool {
        self.tracks.iter().any(|t| t.duck_listener)
            && self
                .tracks
                .iter()
                .any(|t| t.ducking.is_some() && t.kind == TrackKind::Audio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_floor_while_it_sounds_and_more_on_its_peaks() {
        let d = Ducking::default();
        assert_eq!(d.reduction_db(-80.0), 0.0);
        assert!((d.reduction_db(-20.0) + 12.0).abs() < 1e-4);
        let peaky = Ducking {
            floor_db: -6.0,
            peak: 0.5,
        };
        // 20 dB over −30: 10 dB more.
        assert!((peaky.reduction_db(-10.0) + 16.0).abs() < 1e-4);
        assert!((peaky.reduction_db(-35.0) + 6.0).abs() < 1e-4);
        // Never past −60 dB.
        let deep = Ducking {
            floor_db: -50.0,
            peak: 1.0,
        };
        assert_eq!(deep.reduction_db(0.0), DEEPEST_DB);
    }

    #[test]
    fn levels_rise_quickly_and_fall_slowly() {
        let rate = 48_000u32;
        let mut m = LevelMeter::new(rate);
        // A second of silence, a second of a tone at −6 dBFS RMS, a second
        // of silence.
        let tone = |i: usize| 0.707 * (std::f32::consts::TAU * 220.0 * i as f32 / 48_000.0).sin();
        let mut s: Vec<f32> = vec![0.0; rate as usize];
        s.extend((0..rate as usize).map(tone));
        s.extend(vec![0.0; rate as usize]);
        m.push(&s);
        let l = m.finish(Tick::from_seconds(10));
        assert_eq!(l.levels.len(), 75);
        let at = |s: f64| l.level_at(Tick::from_seconds_f64(10.0 + s));
        assert!(at(0.5) < -90.0, "{}", at(0.5));
        // Up within a level.
        assert!((at(1.1) + 6.0).abs() < 1.0, "{}", at(1.1));
        // Once the tone stops, it falls over a few tenths of a second.
        assert!(at(2.06) > -45.0, "{}", at(2.06));
        assert!(at(2.6) < at(2.06) - 15.0, "{}", at(2.6));
        // Outside the levels: silence.
        assert_eq!(at(-1.0), QUIETEST_DB);
        assert_eq!(at(5.0), QUIETEST_DB);
    }
}
