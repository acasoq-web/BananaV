//! Loudness measurement after ITU-R BS.1770-4 / EBU R 128: K-weighting,
//! momentary (400 ms) and short-term (3 s) loudness, gated integrated
//! loudness and loudness range (EBU Tech 3342).

use std::f64::consts::PI;

#[derive(Clone, Copy, Debug)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// The two K-weighting stages for `rate` (high shelf, then RLB high-pass).
fn k_weighting(rate: f64) -> [Biquad; 2] {
    // Stage 1: high shelf modelling the head.
    let (f0, gain_db, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
    let k = (PI * f0 / rate).tan();
    let vh = 10f64.powf(gain_db / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad {
        b: [
            (vh + vb * k / q + k * k) / a0,
            2.0 * (k * k - vh) / a0,
            (vh - vb * k / q + k * k) / a0,
        ],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        z: [0.0; 2],
    };
    // Stage 2: revised low-frequency B-curve high-pass.
    let (f0, q) = (38.13547087602444, 0.5003270373238773);
    let k = (PI * f0 / rate).tan();
    let a0 = 1.0 + k / q + k * k;
    let high_pass = Biquad {
        b: [1.0, -2.0, 1.0],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        z: [0.0; 2],
    };
    [shelf, high_pass]
}

/// Loudness of a mean square value, in LUFS.
fn lufs(mean_square: f64) -> f64 {
    if mean_square <= 1e-20 {
        -200.0
    } else {
        -0.691 + 10.0 * mean_square.log10()
    }
}

/// Summary of a whole measurement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoudnessReport {
    /// Gated integrated loudness (LUFS).
    pub integrated: f64,
    /// Loudness range (LU).
    pub range: f64,
    pub max_momentary: f64,
    pub max_short_term: f64,
    /// Highest sample magnitude (dBFS).
    pub sample_peak: f64,
    /// Seconds measured.
    pub duration: f64,
}

/// Streaming meter: feed planar blocks with [`LoudnessMeter::push`].
#[derive(Clone, Debug)]
pub struct LoudnessMeter {
    rate: u32,
    filters: Vec<[Biquad; 2]>,
    /// Samples per 100 ms sub-block.
    sub_len: usize,
    acc: f64,
    acc_n: usize,
    /// Channel-summed mean square of each finished 100 ms sub-block.
    subs: Vec<f64>,
    peak: f32,
    samples: u64,
}

impl LoudnessMeter {
    pub fn new(rate: u32, channels: usize) -> LoudnessMeter {
        LoudnessMeter {
            rate,
            filters: (0..channels.max(1))
                .map(|_| k_weighting(rate as f64))
                .collect(),
            sub_len: (rate as usize / 10).max(1),
            acc: 0.0,
            acc_n: 0,
            subs: Vec::new(),
            peak: 0.0,
            samples: 0,
        }
    }

    /// Adds a block of planar samples (one `Vec` per channel; L, R, C have
    /// weight 1, surround channels 1.41).
    pub fn push(&mut self, channels: &[Vec<f32>]) {
        let n = channels.iter().map(Vec::len).min().unwrap_or(0);
        let count = channels.len().min(self.filters.len());
        for i in 0..n {
            let mut sum = 0.0;
            for (c, ch) in channels.iter().enumerate().take(count) {
                let x = ch[i];
                self.peak = self.peak.max(x.abs());
                let [s1, s2] = &mut self.filters[c];
                let y = s2.process(s1.process(x as f64));
                // 5.1 is L R C LFE Ls Rs (LFE not measured); quad is L R Ls Rs.
                let weight = match (count, c) {
                    (6, 3) => continue,
                    (6, 4 | 5) | (4, 2 | 3) => 1.41,
                    _ => 1.0,
                };
                sum += weight * y * y;
            }
            self.acc += sum;
            self.acc_n += 1;
            if self.acc_n == self.sub_len {
                self.subs.push(self.acc / self.sub_len as f64);
                self.acc = 0.0;
                self.acc_n = 0;
            }
        }
        self.samples += n as u64;
    }

    fn window(&self, end: usize, len: usize) -> f64 {
        let start = end + 1 - len;
        self.subs[start..=end].iter().sum::<f64>() / len as f64
    }

    /// Loudness of the last 400 ms.
    pub fn momentary(&self) -> f64 {
        if self.subs.len() < 4 {
            return -200.0;
        }
        lufs(self.window(self.subs.len() - 1, 4))
    }

    /// Loudness of the last 3 s.
    pub fn short_term(&self) -> f64 {
        if self.subs.len() < 30 {
            return -200.0;
        }
        lufs(self.window(self.subs.len() - 1, 30))
    }

    /// Gated mean of windows of `len` sub-blocks: absolute gate at −70 LUFS
    /// and relative gate `relative` LU below the absolute-gated mean.
    fn gated(&self, len: usize, relative: f64) -> Vec<(f64, f64)> {
        if self.subs.len() < len {
            return Vec::new();
        }
        let blocks: Vec<(f64, f64)> = (len - 1..self.subs.len())
            .map(|j| {
                let z = self.window(j, len);
                (z, lufs(z))
            })
            .filter(|&(_, l)| l > -70.0)
            .collect();
        if blocks.is_empty() {
            return blocks;
        }
        let mean = blocks.iter().map(|b| b.0).sum::<f64>() / blocks.len() as f64;
        let gate = lufs(mean) - relative;
        blocks.into_iter().filter(|&(_, l)| l > gate).collect()
    }

    pub fn report(&self) -> LoudnessReport {
        let integrated = {
            let kept = self.gated(4, 10.0);
            if kept.is_empty() {
                -200.0
            } else {
                lufs(kept.iter().map(|b| b.0).sum::<f64>() / kept.len() as f64)
            }
        };
        let range = {
            let mut l: Vec<f64> = self.gated(30, 20.0).into_iter().map(|b| b.1).collect();
            if l.len() < 2 {
                0.0
            } else {
                l.sort_by(f64::total_cmp);
                let at = |p: f64| l[((l.len() - 1) as f64 * p).round() as usize];
                at(0.95) - at(0.10)
            }
        };
        let max_of = |len: usize| {
            if self.subs.len() < len {
                -200.0
            } else {
                (len - 1..self.subs.len())
                    .map(|j| lufs(self.window(j, len)))
                    .fold(-200.0, f64::max)
            }
        };
        LoudnessReport {
            integrated,
            range,
            max_momentary: max_of(4),
            max_short_term: max_of(30),
            sample_peak: if self.peak <= 0.0 {
                -200.0
            } else {
                20.0 * (self.peak as f64).log10()
            },
            duration: self.samples as f64 / self.rate as f64,
        }
    }
}

/// True peaks (ITU-R BS.1770-4, Annex 2): each channel four times
/// oversampled through a 48-tap windowed-sinc interpolator (12 taps a
/// phase), the highest magnitude of the samples between the samples. With
/// the blocking filter (the Loudness Meters' True Peak Blocking Filter), a
/// DC-blocking high-pass runs alongside and the higher of the two counts:
/// asymmetric signals or an offset cannot hide a peak.
#[derive(Clone, Debug)]
pub struct TruePeak {
    taps: [[f32; 12]; 4],
    hist: Vec<[f32; 12]>,
    blocked: Vec<[f32; 12]>,
    dc: Vec<(f32, f32)>,
    blocking: bool,
    /// Since the last reset, and per channel since the last reading.
    max: f32,
    recent: Vec<f32>,
}

impl TruePeak {
    pub fn new(channels: usize, blocking: bool) -> TruePeak {
        let n = 48usize;
        let c = (n as f64 - 1.0) / 2.0;
        let mut taps = [[0.0f32; 12]; 4];
        for (p, phase) in taps.iter_mut().enumerate() {
            let mut sum = 0.0f64;
            let mut h = [0.0f64; 12];
            for (k, v) in h.iter_mut().enumerate() {
                let i = 4 * k + p;
                let x = (i as f64 - c) / 4.0;
                let sinc = if x.abs() < 1e-12 {
                    1.0
                } else {
                    (PI * x).sin() / (PI * x)
                };
                let w = 0.5 - 0.5 * (2.0 * PI * (i as f64 + 0.5) / n as f64).cos();
                *v = sinc * w;
                sum += *v;
            }
            for (t, v) in phase.iter_mut().zip(h) {
                *t = (v / sum) as f32;
            }
        }
        TruePeak {
            taps,
            hist: vec![[0.0; 12]; channels],
            blocked: vec![[0.0; 12]; channels],
            dc: vec![(0.0, 0.0); channels],
            blocking,
            max: 0.0,
            recent: vec![0.0; channels],
        }
    }

    /// The highest of the four phases' interpolated values over `h` (the
    /// newest sample first).
    fn over(&self, h: &[f32; 12]) -> f32 {
        let mut m = 0.0f32;
        for phase in &self.taps {
            let y: f32 = phase.iter().zip(h).map(|(t, x)| t * x).sum();
            m = m.max(y.abs());
        }
        m
    }

    pub fn push(&mut self, channels: &[Vec<f32>]) {
        let n = channels.iter().map(Vec::len).min().unwrap_or(0);
        for (c, ch) in channels.iter().enumerate().take(self.hist.len()) {
            for &x in &ch[..n] {
                let h = &mut self.hist[c];
                h.copy_within(0..11, 1);
                h[0] = x;
                let mut peak = self.over(&self.hist[c]).max(x.abs());
                if self.blocking {
                    // y = x − x₁ + R·y₁: a high-pass of a few hertz.
                    let (x1, y1) = self.dc[c];
                    let y = x - x1 + 0.9995 * y1;
                    self.dc[c] = (x, y);
                    let b = &mut self.blocked[c];
                    b.copy_within(0..11, 1);
                    b[0] = y;
                    peak = peak.max(self.over(&self.blocked[c]));
                }
                self.recent[c] = self.recent[c].max(peak);
                self.max = self.max.max(peak);
            }
        }
    }

    /// The highest true peak since the last reset (linear).
    pub fn max(&self) -> f32 {
        self.max
    }

    /// Each channel's highest true peak since the last call (linear).
    pub fn take_recent(&mut self) -> Vec<f32> {
        let out = self.recent.clone();
        self.recent.iter_mut().for_each(|v| *v = 0.0);
        out
    }

    /// Reset Clip: the over-the-limit mark starts again.
    pub fn reset_max(&mut self) {
        self.max = 0.0;
    }
}

/// What the Loudness Meters show (LUFS, LU and dBTP; −200 for nothing yet).
#[derive(Clone, Debug, PartialEq)]
pub struct LiveLoudness {
    pub momentary: f64,
    pub short_term: f64,
    pub integrated: f64,
    pub range: f64,
    pub max_momentary: f64,
    pub max_short_term: f64,
    /// The lowest above silence (−70 LUFS), for the meters' valleys.
    pub min_momentary: f64,
    pub min_short_term: f64,
    /// The highest true peak since the reset, and each channel's since the
    /// last reading.
    pub true_peak: f64,
    pub true_peaks: Vec<f64>,
}

/// The Loudness Meters' engine: loudness (BS.1770, EBU R 128) and true
/// peaks of what plays, from the start of playback or the last reset.
#[derive(Clone, Debug)]
pub struct LiveMeter {
    meter: LoudnessMeter,
    peaks: TruePeak,
    subs_seen: usize,
    min_momentary: f64,
    min_short_term: f64,
}

/// dB of a linear magnitude (−200 for silence).
fn db(v: f32) -> f64 {
    if v <= 0.0 {
        -200.0
    } else {
        20.0 * (v as f64).log10()
    }
}

impl LiveMeter {
    pub fn new(rate: u32, channels: usize, blocking: bool) -> LiveMeter {
        LiveMeter {
            meter: LoudnessMeter::new(rate, channels),
            peaks: TruePeak::new(channels, blocking),
            subs_seen: 0,
            min_momentary: -200.0,
            min_short_term: -200.0,
        }
    }

    pub fn push(&mut self, channels: &[Vec<f32>]) {
        self.meter.push(channels);
        self.peaks.push(channels);
        // The valleys: each new 100 ms step's loudness above silence.
        let low = |v: f64, now: f64| {
            if now <= -70.0 {
                v
            } else if v <= -200.0 {
                now
            } else {
                v.min(now)
            }
        };
        while self.subs_seen < self.meter.subs.len() {
            self.subs_seen += 1;
            let end = self.subs_seen - 1;
            if self.subs_seen >= 4 {
                let m = lufs(self.meter.window(end, 4));
                self.min_momentary = low(self.min_momentary, m);
            }
            if self.subs_seen >= 30 {
                let s = lufs(self.meter.window(end, 30));
                self.min_short_term = low(self.min_short_term, s);
            }
        }
    }

    /// The readings now (each channel's true peak since the last reading).
    pub fn read(&mut self) -> LiveLoudness {
        let r = self.meter.report();
        LiveLoudness {
            momentary: self.meter.momentary(),
            short_term: self.meter.short_term(),
            integrated: r.integrated,
            range: r.range,
            max_momentary: r.max_momentary,
            max_short_term: r.max_short_term,
            min_momentary: self.min_momentary,
            min_short_term: self.min_short_term,
            true_peak: db(self.peaks.max()),
            true_peaks: self.peaks.take_recent().into_iter().map(db).collect(),
        }
    }

    /// Reset Clip.
    pub fn reset_clip(&mut self) {
        self.peaks.reset_max();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn true_peaks_find_what_falls_between_samples() {
        // A sine at a quarter of the rate, 45° off the samples: they read
        // 0.707, the wave reaches 1.
        let rate = 48_000.0;
        let s: Vec<f32> = (0..4800)
            .map(|i| (2.0 * PI * 12_000.0 * i as f64 / rate + PI / 4.0).sin() as f32)
            .collect();
        let sample_peak = s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((sample_peak - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3);
        let mut tp = TruePeak::new(1, false);
        tp.push(&[s]);
        assert!(db(tp.max()).abs() < 0.6, "{} dBTP", db(tp.max()));
        // The live meter: −23 LUFS of a 1 kHz sine, its true peak at −23.
        let tone: Vec<f32> = (0..48_000 * 4)
            .map(|i| {
                (10f64.powf(-23.0 / 20.0) * (2.0 * PI * 1000.0 * i as f64 / rate).sin()) as f32
            })
            .collect();
        let mut live = LiveMeter::new(48_000, 2, true);
        live.push(&[tone.clone(), tone]);
        let r = live.read();
        assert!((r.momentary + 23.0).abs() < 0.1 && (r.short_term + 23.0).abs() < 0.1);
        assert!((r.true_peak + 23.0).abs() < 0.2, "{}", r.true_peak);
        assert!((r.min_momentary + 23.0).abs() < 0.3, "{}", r.min_momentary);
        assert_eq!(r.true_peaks.len(), 2);
        live.reset_clip();
        assert!(live.read().true_peak <= -200.0);
    }

    fn sine(rate: u32, seconds: f64, hz: f64, dbfs: f64) -> Vec<f32> {
        let a = 10f64.powf(dbfs / 20.0);
        (0..(rate as f64 * seconds) as usize)
            .map(|i| (a * (2.0 * PI * hz * i as f64 / rate as f64).sin()) as f32)
            .collect()
    }

    #[test]
    fn ebu_3341_case_1_reads_minus_23() {
        // Stereo 1 kHz sine at −23 dBFS: M, S and I are −23 LUFS.
        for rate in [48_000, 44_100] {
            let s = sine(rate, 20.0, 1000.0, -23.0);
            let mut m = LoudnessMeter::new(rate, 2);
            for chunk in s.chunks(1024) {
                m.push(&[chunk.to_vec(), chunk.to_vec()]);
            }
            let r = m.report();
            assert!((r.integrated + 23.0).abs() < 0.1, "{rate} Hz: {r:?}");
            assert!((m.short_term() + 23.0).abs() < 0.1);
            assert!((m.momentary() + 23.0).abs() < 0.1);
            assert!(r.range < 0.1);
            assert!((r.sample_peak + 23.0).abs() < 0.01);
        }
    }

    #[test]
    fn silence_is_gated_out() {
        let rate = 48_000;
        let mut tone = sine(rate, 10.0, 1000.0, -23.0);
        tone.extend(std::iter::repeat_n(0.0, rate as usize * 10));
        let mut m = LoudnessMeter::new(rate, 2);
        m.push(&[tone.clone(), tone]);
        assert!(
            (m.report().integrated + 23.0).abs() < 0.2,
            "{:?}",
            m.report()
        );
    }

    #[test]
    fn loudness_range_of_two_levels() {
        // 20 s at −20 then 20 s at −30 dBFS: LRA close to 10 LU (EBU 3342 case 1).
        let rate = 48_000;
        let mut s = sine(rate, 20.0, 1000.0, -20.0);
        s.extend(sine(rate, 20.0, 1000.0, -30.0));
        let mut m = LoudnessMeter::new(rate, 2);
        m.push(&[s.clone(), s]);
        let lra = m.report().range;
        assert!((lra - 10.0).abs() < 1.0, "{lra}");
    }
}
