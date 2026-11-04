//! Track EQ and Paragraphic EQ, as VEGAS's.
//!
//! Track EQ: four bands in a row, each a low shelf, a peak/notch or a
//! high shelf. A shelf is one to four first-order sections at its
//! frequency (6 to 24 dB an octave), the gain shared among them; a band
//! is a second-order peak as wide as its bandwidth in octaves. Each
//! channel can be left out (the others go through untouched).
//!
//! Paragraphic EQ: four bands in a row, each adding its band-pass, raised
//! or lowered by its gain, to the sound; then optional low and high
//! shelves (a second-order low- or high-pass added the same way); the
//! result at the wet level with the untouched sound at the dry level.

use bv_timeline::audio_fx::{paragraphic_eq as pq, track_eq as te};

use super::{Own, db_gain};

/// Added and taken away again on filter memories, so a fading signal ends
/// at 0 instead of lingering as a denormal number.
const FLUSH: f32 = 1e-18;

/// A filter section of up to the second order:
/// `y = b0·x + b1·x₁ + b2·x₂ − a1·y₁ − a2·y₂`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Section {
    b: [f32; 3],
    a: [f32; 2],
}

impl Section {
    /// `(n0 + n1·z⁻¹ + n2·z⁻²) / (d0 + d1·z⁻¹ + d2·z⁻²)`.
    pub(crate) fn new(n: [f64; 3], d: [f64; 3]) -> Section {
        Section {
            b: [
                (n[0] / d[0]) as f32,
                (n[1] / d[0]) as f32,
                (n[2] / d[0]) as f32,
            ],
            a: [(d[1] / d[0]) as f32, (d[2] / d[0]) as f32],
        }
    }

    /// A first-order shelf at `hz`, `db` at the lows (`high`: at the highs).
    pub(crate) fn shelf(hz: f64, db: f64, high: bool, fs: f64) -> Section {
        let k = (std::f64::consts::PI * (hz / fs).min(0.49)).tan();
        let v = 10f64.powf(db / 20.0);
        // Its analog form on a frequency of 1 (the corner):
        // low (s + A)/(s + B), high (A·s + 1)/(B·s + 1); A over B the gain.
        let (a, b) = if v >= 1.0 { (v, 1.0) } else { (1.0, 1.0 / v) };
        if high {
            Section::new([a + k, k - a, 0.0], [b + k, k - b, 0.0])
        } else {
            Section::new(
                [1.0 + a * k, a * k - 1.0, 0.0],
                [1.0 + b * k, b * k - 1.0, 0.0],
            )
        }
    }

    /// A second-order peak at `hz`, `db` at its top, `octaves` wide.
    pub(crate) fn peak(hz: f64, db: f64, octaves: f64, fs: f64) -> Section {
        let w = std::f64::consts::TAU * (hz / fs).min(0.49);
        let (s, c) = w.sin_cos();
        let alpha = s * (std::f64::consts::LN_2 / 2.0 * octaves * w / s).sinh();
        let a = 10f64.powf(db / 40.0);
        Section::new(
            [1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a],
            [1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a],
        )
    }

    /// A second-order band-pass at `hz` (0 dB at its top), `octaves` wide.
    pub(crate) fn band(hz: f64, octaves: f64, fs: f64) -> Section {
        let w = std::f64::consts::TAU * (hz / fs).min(0.49);
        let (s, c) = w.sin_cos();
        let alpha = s * (std::f64::consts::LN_2 / 2.0 * octaves * w / s).sinh();
        Section::new([alpha, 0.0, -alpha], [1.0 + alpha, -2.0 * c, 1.0 - alpha])
    }

    /// A second-order low-pass (or high-pass) at `hz`, Butterworth.
    pub(crate) fn pass(hz: f64, high: bool, fs: f64) -> Section {
        let w = std::f64::consts::TAU * (hz / fs).min(0.49);
        let (s, c) = w.sin_cos();
        let alpha = s / std::f64::consts::SQRT_2;
        let d = [1.0 + alpha, -2.0 * c, 1.0 - alpha];
        if high {
            Section::new([(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0], d)
        } else {
            Section::new([(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0], d)
        }
    }

    /// `x` through the section with memory `m` (`x₁, x₂, y₁, y₂`).
    #[inline]
    pub(crate) fn run(&self, x: f32, m: &mut [f32; 4]) -> f32 {
        let y = self.b[0] * x + self.b[1] * m[0] + self.b[2] * m[1]
            - self.a[0] * m[2]
            - self.a[1] * m[3];
        let y = (y + FLUSH) - FLUSH;
        *m = [x, m[0], y, m[2]];
        y
    }
}

#[derive(Debug)]
pub(crate) struct TrackEq {
    rate: f64,
    /// The sections in their order, all channels alike.
    sections: Vec<Section>,
    /// Which channels it works on.
    channels: [bool; 6],
    /// Each channel's sections' memories.
    memory: Vec<Vec<[f32; 4]>>,
}

impl TrackEq {
    pub(crate) fn new(rate: u32) -> TrackEq {
        TrackEq {
            rate: rate.max(1) as f64,
            sections: Vec::new(),
            channels: [true; 6],
            memory: Vec::new(),
        }
    }
}

impl Own for TrackEq {
    fn set(&mut self, params: &[f32]) {
        let p = |i: usize| {
            let s = &te::PARAMS[i];
            params
                .get(i)
                .copied()
                .unwrap_or(s.default)
                .clamp(s.min, s.max) as f64
        };
        let mut sections = Vec::new();
        for band in 0..4 {
            let at = band * te::PER_BAND;
            if p(at + te::ENABLE) < 0.5 {
                continue;
            }
            let (hz, db) = (p(at + te::FREQUENCY), p(at + te::GAIN));
            match p(at + te::MODE).round() as i32 {
                1 => sections.push(Section::peak(hz, db, p(at + te::BANDWIDTH), self.rate)),
                mode => {
                    let count = p(at + te::ROLLOFF).round() as usize + 1;
                    for _ in 0..count {
                        let share = db / count as f64;
                        sections.push(Section::shelf(hz, share, mode == 2, self.rate));
                    }
                }
            }
        }
        if sections != self.sections {
            self.memory.clear();
        }
        self.sections = sections;
        for (k, on) in self.channels.iter_mut().enumerate() {
            *on = p(te::CHANNELS + k) >= 0.5;
        }
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let count = channels.len();
        self.memory
            .resize_with(count, || vec![[0.0; 4]; self.sections.len()]);
        for (c, ch) in channels.iter_mut().enumerate() {
            // BananaV's 5.1 order is L R C LFE Ls Rs; the switches' is L R C
            // Ls Rs LFE.
            let switch = [0, 1, 2, 5, 3, 4].get(c).copied().unwrap_or(c % 2);
            if !self.channels[switch] {
                continue;
            }
            for (s, m) in self.sections.iter().zip(self.memory[c].iter_mut()) {
                for x in ch.iter_mut() {
                    *x = s.run(*x, m);
                }
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct ParagraphicEq {
    rate: f64,
    dry: f32,
    wet: f32,
    /// The bands (band-pass, how much of it is added) then the shelves
    /// (low- or high-pass, likewise).
    parts: Vec<(Section, f32)>,
    memory: Vec<Vec<[f32; 4]>>,
    /// One block's untouched sound, a channel.
    dry_copy: Vec<f32>,
}

impl ParagraphicEq {
    pub(crate) fn new(rate: u32) -> ParagraphicEq {
        ParagraphicEq {
            rate: rate.max(1) as f64,
            dry: 0.0,
            wet: 1.0,
            parts: Vec::new(),
            memory: Vec::new(),
            dry_copy: Vec::new(),
        }
    }
}

impl Own for ParagraphicEq {
    fn set(&mut self, params: &[f32]) {
        let p = |i: usize| {
            let s = &pq::PARAMS[i];
            params
                .get(i)
                .copied()
                .unwrap_or(s.default)
                .clamp(s.min, s.max) as f64
        };
        self.dry = db_gain(p(pq::DRY) as f32, -25.0);
        self.wet = db_gain(p(pq::WET) as f32, -99.0);
        let mut parts = Vec::new();
        for band in 0..4 {
            let at = pq::BANDS + band * pq::PER_BAND;
            let more = 10f64.powf(p(at + pq::GAIN) / 20.0) - 1.0;
            let section = Section::band(p(at + pq::CENTER), p(at + pq::WIDTH), self.rate);
            parts.push((section, more as f32));
        }
        for (enable, high) in [(pq::LOW_SHELF, false), (pq::HIGH_SHELF, true)] {
            if p(enable) >= 0.5 {
                let more = 10f64.powf(p(enable + 2) / 20.0) - 1.0;
                parts.push((Section::pass(p(enable + 1), high, self.rate), more as f32));
            }
        }
        if parts.len() != self.parts.len() {
            self.memory.clear();
        }
        self.parts = parts;
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let count = channels.len();
        let parts = self.parts.len();
        self.memory.resize_with(count, || vec![[0.0; 4]; parts]);
        for (ch, memory) in channels.iter_mut().zip(self.memory.iter_mut()) {
            self.dry_copy.clear();
            self.dry_copy.extend_from_slice(ch);
            for ((s, more), m) in self.parts.iter().zip(memory.iter_mut()) {
                for x in ch.iter_mut() {
                    *x += more * s.run(*x, m);
                }
            }
            let (dry, wet) = (self.dry, self.wet);
            for (x, d) in ch.iter_mut().zip(self.dry_copy.iter()) {
                *x = dry * d + wet * *x;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin())
            .collect()
    }

    /// The level (dB) a sine at `hz` comes out at, once settled.
    fn level(fx: &mut dyn Own, hz: f32) -> f32 {
        let mut ch = vec![sine(hz, 48_000)];
        fx.process(&mut ch);
        let tail = &ch[0][24_000..];
        let power = tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32;
        10.0 * (2.0 * power).log10()
    }

    fn track_eq(values: &[(usize, f32)]) -> TrackEq {
        let mut e = TrackEq::new(48_000);
        let mut p: Vec<f32> = te::PARAMS.iter().map(|s| s.default).collect();
        for &(i, v) in values {
            p[i] = v;
        }
        e.set(&p);
        e
    }

    #[test]
    fn track_eq_at_0_db_leaves_the_sound_alone() {
        let mut e = track_eq(&[]);
        for hz in [50.0, 1000.0, 12_000.0] {
            assert!(
                level(&mut e, hz).abs() < 0.05,
                "{hz}: {}",
                level(&mut e, hz)
            );
        }
    }

    #[test]
    fn track_eq_peaks_and_shelves() {
        // Band 3 (a peak at 1 kHz) at −15 dB.
        let at = 2 * te::PER_BAND;
        let mut e = track_eq(&[(at + te::GAIN, -15.0)]);
        assert!(
            (level(&mut e, 1000.0) + 15.0).abs() < 0.3,
            "{}",
            level(&mut e, 1000.0)
        );
        assert!(level(&mut e, 100.0).abs() < 0.5);
        // Band 4 (a high shelf at 5 kHz, 12 dB an octave) at +6 dB.
        let at = 3 * te::PER_BAND;
        let mut e = track_eq(&[(at + te::GAIN, 6.0)]);
        assert!(
            (level(&mut e, 18_000.0) - 6.0).abs() < 0.5,
            "{}",
            level(&mut e, 18_000.0)
        );
        assert!(level(&mut e, 200.0).abs() < 0.3);
        // Band 1 (a low shelf at 100 Hz) at −12 dB, 24 dB an octave.
        let mut e = track_eq(&[(te::GAIN, -12.0), (te::ROLLOFF, 3.0)]);
        assert!(
            (level(&mut e, 20.0) + 12.0).abs() < 1.0,
            "{}",
            level(&mut e, 20.0)
        );
        assert!(level(&mut e, 2000.0).abs() < 0.3);
    }

    #[test]
    fn track_eq_leaves_out_the_channels_switched_off() {
        let at = 2 * te::PER_BAND;
        let mut e = track_eq(&[(at + te::GAIN, -15.0), (te::CHANNELS + 1, 0.0)]);
        let x = sine(1000.0, 4800);
        let mut ch = vec![x.clone(), x.clone()];
        e.process(&mut ch);
        assert_eq!(ch[1], x);
        assert_ne!(ch[0], x);
    }

    fn paragraphic(values: &[(usize, f32)]) -> ParagraphicEq {
        let mut e = ParagraphicEq::new(48_000);
        let mut p: Vec<f32> = pq::PARAMS.iter().map(|s| s.default).collect();
        for &(i, v) in values {
            p[i] = v;
        }
        e.set(&p);
        e
    }

    #[test]
    fn paragraphic_bands_and_shelves() {
        // Its defaults: no change.
        let mut e = paragraphic(&[]);
        assert!(level(&mut e, 1000.0).abs() < 0.05);
        // Band 3 at −15 dB: 1 kHz down 15 dB.
        let at = pq::BANDS + 2 * pq::PER_BAND;
        let mut e = paragraphic(&[(at + pq::GAIN, -15.0)]);
        assert!(
            (level(&mut e, 1000.0) + 15.0).abs() < 0.3,
            "{}",
            level(&mut e, 1000.0)
        );
        assert!(level(&mut e, 100.0).abs() < 0.5);
        // A low shelf at 250 Hz, +4 dB.
        let mut e = paragraphic(&[
            (pq::LOW_SHELF, 1.0),
            (pq::LOW_SHELF + 1, 250.0),
            (pq::LOW_SHELF + 2, 4.0),
        ]);
        assert!(
            (level(&mut e, 30.0) - 4.0).abs() < 0.3,
            "{}",
            level(&mut e, 30.0)
        );
        assert!(level(&mut e, 5000.0).abs() < 0.3);
        // The dry sound mixed back: wet off, dry at 0 dB.
        let mut e = paragraphic(&[(pq::DRY, 0.0), (pq::WET, -25.0), (at + pq::GAIN, -15.0)]);
        let wet = 10f32.powf(-25.0 / 20.0);
        let expect = 20.0 * (1.0 + wet * 10f32.powf(-15.0 / 20.0)).log10();
        assert!(
            (level(&mut e, 1000.0) - expect).abs() < 0.3,
            "{}",
            level(&mut e, 1000.0)
        );
    }
}
