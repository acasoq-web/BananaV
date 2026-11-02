//! Options › Metronome: a click on each beat of the project's tempo (File ›
//! Properties › Ruler) while it plays and records; renders never have it.
//! The first beat of each measure is accented.

use bv_core::{AudioBuffer, SampleRate, Tick};
use bv_timeline::Tempo;

/// A click: mono samples at their own rate.
#[derive(Clone, Debug, PartialEq)]
pub struct Click {
    pub samples: Vec<f32>,
    pub rate: u32,
}

impl Click {
    /// A tone of `hz` fading out over `seconds`, peaking at `level`.
    fn tone(hz: f32, seconds: f32, level: f32, rate: u32) -> Click {
        let n = (seconds * rate as f32) as usize;
        let samples = (0..n)
            .map(|i| {
                let t = i as f32 / rate as f32;
                // A quick attack, then an exponential fade.
                let attack = (t / 0.001).min(1.0);
                let decay = (-t / (seconds / 4.0)).exp();
                (std::f32::consts::TAU * hz * t).sin() * level * attack * decay
            })
            .collect();
        Click { samples, rate }
    }

    /// A sound file's first two seconds, mixed to mono.
    fn from_file(path: &str) -> Option<Click> {
        let info = bv_media::probe_file(path).ok()?;
        let mut reader = bv_media::open_audio(&info).ok()?;
        let rate = reader.sample_rate().0;
        let frames =
            ((info.duration.as_seconds_f64() * rate as f64) as usize).min(rate as usize * 2);
        let buf = reader.read(0, frames).ok()?;
        let channels = buf.channels.len().max(1) as f32;
        let samples = (0..frames)
            .map(|i| {
                buf.channels
                    .iter()
                    .map(|c| c.get(i).copied().unwrap_or(0.0))
                    .sum::<f32>()
                    / channels
            })
            .collect();
        Some(Click { samples, rate })
    }

    /// The click at `rate` (linear interpolation).
    fn at(&self, rate: u32) -> Vec<f32> {
        if rate == self.rate || self.samples.is_empty() {
            return self.samples.clone();
        }
        let step = self.rate as f64 / rate as f64;
        let n = (self.samples.len() as f64 / step) as usize;
        (0..n)
            .map(|i| {
                let x = i as f64 * step;
                let k = x as usize;
                let f = (x - k as f64) as f32;
                let a = self.samples[k];
                let b = self.samples.get(k + 1).copied().unwrap_or(a);
                a + (b - a) * f
            })
            .collect()
    }
}

/// The metronome's sounds: a beat, and the accented first beat of a
/// measure.
#[derive(Clone, Debug, PartialEq)]
pub struct Clicks {
    pub normal: Click,
    pub accent: Click,
}

impl Clicks {
    /// BananaV's own clicks.
    pub fn standard() -> Clicks {
        Clicks {
            normal: Click::tone(1000.0, 0.04, 0.35, 48_000),
            accent: Click::tone(1600.0, 0.05, 0.5, 48_000),
        }
    }

    /// Clicks from sound files (Options › Preferences › Audio: "Use custom
    /// metronome sounds"); BananaV's own for a file missing or unreadable.
    pub fn from_files(normal: Option<&str>, accent: Option<&str>) -> Clicks {
        let standard = Clicks::standard();
        Clicks {
            normal: normal.and_then(Click::from_file).unwrap_or(standard.normal),
            accent: accent.and_then(Click::from_file).unwrap_or(standard.accent),
        }
    }

    /// The clicks at `rate`.
    pub(crate) fn at(&self, rate: u32) -> (Vec<f32>, Vec<f32>) {
        (self.normal.at(rate), self.accent.at(rate))
    }
}

/// Adds to `out` (a block of the project's mix from sample `start`) the
/// clicks of the beats of `tempo`, counted from the ruler's start `offset`.
pub(crate) fn add_clicks(
    out: &mut AudioBuffer,
    start: i64,
    rate: SampleRate,
    tempo: &Tempo,
    offset: Tick,
    (normal, accent): (&[f32], &[f32]),
) {
    let frames = out.channels.first().map_or(0, Vec::len) as i64;
    if frames == 0 {
        return;
    }
    let beat = tempo.beat().0.max(1);
    let longest = normal.len().max(accent.len()) as i64;
    let from = rate.sample_start(start - longest).0 + offset.0;
    let to = rate.sample_start(start + frames).0 + offset.0;
    let per_measure = tempo.beats_per_measure.max(1) as i64;
    for k in from.div_euclid(beat)..=to.div_euclid(beat) {
        let at = Tick(k * beat) - offset;
        let s = rate.sample_at(at);
        let s = if rate.sample_start(s) < at { s + 1 } else { s };
        let click = if k.rem_euclid(per_measure) == 0 {
            accent
        } else {
            normal
        };
        for ch in &mut out.channels {
            for (i, v) in click.iter().enumerate() {
                let j = s + i as i64 - start;
                if (0..frames).contains(&j) {
                    ch[j as usize] += v;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_core::ChannelLayout;

    #[test]
    fn a_click_on_each_beat_and_an_accent_on_each_measure() {
        let rate = SampleRate(48_000);
        let tempo = Tempo {
            beats_per_minute: 120.0,
            beats_per_measure: 4,
            beat_note: 4,
        };
        let (normal, accent) = Clicks::standard().at(48_000);
        // Two seconds: four beats at 0, 0.5, 1 and 1.5 s.
        let mut out = AudioBuffer::silent(rate, ChannelLayout::Stereo, 96_000);
        add_clicks(&mut out, 0, rate, &tempo, Tick::ZERO, (&normal, &accent));
        let loud = |s: usize| {
            out.channels[0][s..s + 200]
                .iter()
                .fold(0.0f32, |m, v| m.max(v.abs()))
        };
        for beat in [0, 24_000, 48_000, 72_000] {
            assert!(loud(beat) > 0.1, "a click at {beat}");
        }
        assert!(loud(12_000) < 1e-6, "silence between beats");
        assert!(
            loud(0) > loud(24_000),
            "the measure's first beat is accented"
        );
        // A block starting in the middle of a click finishes it.
        let mut part = AudioBuffer::silent(rate, ChannelLayout::Stereo, 1000);
        add_clicks(&mut part, 100, rate, &tempo, Tick::ZERO, (&normal, &accent));
        assert_eq!(part.channels[0][0], out.channels[0][100]);
        // Resampled clicks keep their length in time.
        assert_eq!(Clicks::standard().normal.at(96_000).len(), normal.len() * 2);
    }
}
