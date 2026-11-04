//! The effects drawn on a graph, as VEGAS's: Amplitude Modulation (a
//! wave), Distortion (a level curve a half of the wave), Graphic Dynamics
//! (a level curve, a block of 20 ms at a time) — Graphic EQ's response is
//! `fir`'s.

use bv_timeline::Tempo;
use bv_timeline::audio_fx::{
    amplitude_modulation as am, distortion as ds, graphic_dynamics as gd, tempo,
};

use super::{Own, db_gain};

/// Added and taken away again on fed-back state, so a fading signal ends
/// at 0 instead of lingering as a denormal number.
const FLUSH: f32 = 1e-18;

/// A graph's points from an effect's values: the count at `at`, then
/// `(x, y)` pairs, sorted by x.
pub(crate) fn points(
    params: &[f32],
    specs: &[bv_timeline::ParamSpec],
    at: usize,
) -> Vec<(f32, f32)> {
    let value = |i: usize| params.get(i).copied().unwrap_or(specs[i].default);
    let count = value(at).round().clamp(0.0, specs[at].max) as usize;
    let mut p: Vec<(f32, f32)> = (0..count)
        .map(|k| (value(at + 1 + 2 * k), value(at + 2 + 2 * k)))
        .collect();
    p.sort_by(|a, b| a.0.total_cmp(&b.0));
    p
}

/// The straight-line curve through `points` at `x` (flat past its ends).
pub(crate) fn along(points: &[(f32, f32)], x: f32) -> f32 {
    let Some(first) = points.first() else {
        return 0.0;
    };
    let k = points.partition_point(|p| p.0 <= x);
    if k == 0 {
        return first.1;
    }
    if k == points.len() {
        return points[k - 1].1;
    }
    let ((x0, y0), (x1, y1)) = (points[k - 1], points[k]);
    if x1 <= x0 {
        y1
    } else {
        y0 + (y1 - y0) * (x - x0) / (x1 - x0)
    }
}

/// A level curve (input dB to output dB, −84 the bottom) as linear gains
/// every 0.1 dB from −84 to 0: the output less the input; a stretch whose
/// output is −84 dB at both ends is silence.
pub(crate) fn level_gains(points: &[(f32, f32)]) -> Vec<f32> {
    (0..=840)
        .map(|k| {
            let l = -84.0 + k as f32 / 10.0;
            let j = points.partition_point(|p| p.0 <= l);
            let silent =
                j > 0 && j < points.len() && points[j - 1].1 <= -84.0 && points[j].1 <= -84.0;
            if silent || (points.len() == 1 && points[0].1 <= -84.0) {
                0.0
            } else {
                10f32.powf((along(points, l) - l) / 20.0)
            }
        })
        .collect()
}

/// The gain of `gains` (from [`level_gains`]) at sample value `x`.
fn level_gain(gains: &[f32], x: f32) -> f32 {
    let a = x.abs();
    let l = if a < 1.0 / 32768.0 {
        -84.0
    } else {
        (20.0 * a.log10()).clamp(-84.0, 0.0)
    };
    let at = (l + 84.0) * 10.0;
    let k = (at as usize).min(839);
    let part = at - k as f32;
    gains[k] + (gains[k + 1] - gains[k]) * part
}

#[derive(Debug)]
pub(crate) struct AmplitudeModulation {
    rate: f64,
    params: Vec<f32>,
    tempo: Tempo,
    /// The wave's gains over a round, 1001 steps.
    gains: Vec<f32>,
    /// Where in the round it is, 0–1000.
    phase: f64,
    lows: Vec<f32>,
}

impl AmplitudeModulation {
    pub(crate) fn new(rate: u32) -> AmplitudeModulation {
        AmplitudeModulation {
            rate: rate.max(1) as f64,
            params: Vec::new(),
            tempo: Tempo::default(),
            gains: vec![1.0; 1001],
            phase: 0.0,
            lows: Vec::new(),
        }
    }

    fn p(&self, i: usize) -> f32 {
        let s = &am::PARAMS[i];
        self.params
            .get(i)
            .copied()
            .unwrap_or(s.default)
            .clamp(s.min, s.max)
    }
}

impl Own for AmplitudeModulation {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
        let mut wave = points(params, am::PARAMS, am::GRAPH);
        if self.p(am::BLEND) >= 0.5 && wave.len() >= 2 {
            // The seam smoothed: both ends at their average, the curve kept
            // from 5 % into the round and from 5 % before its end.
            let ends = (wave[0].1 + wave[wave.len() - 1].1) / 2.0;
            let (near, far) = (along(&wave, 50.0), along(&wave, 950.0));
            let mut blended = vec![(0.0, ends), (50.0, near)];
            blended.extend(wave.iter().copied().filter(|p| p.0 > 50.0 && p.0 < 950.0));
            blended.extend([(950.0, far), (1000.0, ends)]);
            wave = blended;
        }
        let depth = self.p(am::AMPLITUDE).abs();
        for (t, g) in self.gains.iter_mut().enumerate() {
            let v = along(&wave, t as f32).clamp(-100.0, 0.0);
            // 0 at the top is 0 dB, −100 at the bottom the amplitude.
            *g = 10f32.powf(v / 100.0 * depth / 20.0);
        }
    }

    fn keep_time(&mut self, tempo: &Tempo) {
        self.tempo = *tempo;
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let fs = self.rate;
        let samples = if self.p(am::SYNC) >= 0.5 {
            tempo::seconds(self.p(am::PERIOD), &self.tempo) * self.p(am::MULTIPLIER) as f64 * fs
        } else {
            (fs / self.p(am::FREQUENCY) as f64).trunc()
        };
        let step = 1000.0 / samples.max(1.0);
        let dry = db_gain(self.p(am::DRY), -80.0);
        let wet = db_gain(self.p(am::WET), -80.0);
        let p = if self.p(am::LOW_PASS) >= 0.5 {
            (-std::f32::consts::TAU * self.p(am::LOW_FREQ) / fs as f32).exp()
        } else {
            0.0
        };
        // The right channel behind by the stereo pan (half a round at 100 %).
        let behind = 5.0 * self.p(am::PAN) as f64 * 100.0;
        self.lows.resize(channels.len(), 0.0);
        let stereo = channels.len() == 2;
        for (c, (ch, low)) in channels.iter_mut().zip(self.lows.iter_mut()).enumerate() {
            let mut phase = self.phase + if stereo && c == 1 { behind } else { 0.0 };
            for x in ch.iter_mut() {
                let g = self.gains[(phase.rem_euclid(1000.0) as usize).min(1000)];
                let mut w = (1.0 - p) * wet * g * *x + p * *low;
                if w.abs() < 2e-11 {
                    w = 0.0;
                }
                *low = w;
                *x = w + dry * *x;
                phase += step;
            }
        }
        self.phase = (self.phase + step * n as f64).rem_euclid(1000.0);
    }
}

#[derive(Debug)]
pub(crate) struct Distortion {
    rate: f32,
    /// The positive and negative halves' gains (see [`level_gains`]).
    gains: [Vec<f32>; 2],
    slew: Option<f32>,
    low_pass: f32,
    dry: f32,
    wet: f32,
    /// The slew limiter's last two outputs, one pair for all channels:
    /// the channels' samples go through it in turn, so in stereo the two
    /// sides pull on each other.
    last: [f32; 2],
    lows: Vec<f32>,
}

impl Distortion {
    pub(crate) fn new(rate: u32) -> Distortion {
        Distortion {
            rate: rate.max(1) as f32,
            gains: [vec![1.0; 841], vec![1.0; 841]],
            slew: None,
            low_pass: 0.0,
            dry: 0.0,
            wet: 1.0,
            last: [0.0; 2],
            lows: Vec::new(),
        }
    }
}

impl Own for Distortion {
    fn set(&mut self, params: &[f32]) {
        let p = |i: usize| {
            let s = &ds::PARAMS[i];
            params
                .get(i)
                .copied()
                .unwrap_or(s.default)
                .clamp(s.min, s.max)
        };
        self.gains = [
            level_gains(&points(params, ds::PARAMS, ds::POSITIVE)),
            level_gains(&points(params, ds::PARAMS, ds::NEGATIVE)),
        ];
        self.slew = (p(ds::SLEW) >= 0.5).then(|| 0.0005 * p(ds::SLEW_RATE));
        let fc = p(ds::LOW_FREQ);
        self.low_pass = if p(ds::LOW_PASS) >= 0.5 && fc <= self.rate / 2.0 {
            (-std::f32::consts::TAU * fc / self.rate).exp()
        } else {
            0.0
        };
        self.dry = db_gain(p(ds::DRY), -60.0);
        self.wet = db_gain(p(ds::WET), -60.0);
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        self.lows.resize(channels.len(), 0.0);
        let p = self.low_pass;
        let [mut s1, mut s2] = self.last;
        for i in 0..n {
            for (ch, low) in channels.iter_mut().zip(self.lows.iter_mut()) {
                let x = ch[i];
                let half = if x >= 0.0 { 0 } else { 1 };
                let shaped = x * level_gain(&self.gains[half], x);
                let s = match self.slew {
                    Some(rho) => {
                        // The change of slope held within ±rho.
                        let d = s1 - s2;
                        let s = s1 + (shaped - s1).clamp(d - rho, d + rho);
                        s2 = s1;
                        s1 = s;
                        s
                    }
                    None => shaped,
                };
                *low = (p * *low + (1.0 - p) * s + FLUSH) - FLUSH;
                ch[i] = self.wet * *low + self.dry * x;
            }
        }
        self.last = [s1, s2];
    }
}

/// Graphic Dynamics: a block of 20 ms gathered (one block late), its peak
/// (every channel by the loudest with sync) read on the level curve, the
/// gain moving toward it nine tenths of the way in the attack (going
/// down) or release (going up) time, then laid on the block in a straight
/// line over that time (at most the block).
#[derive(Debug)]
pub(crate) struct GraphicDynamics {
    rate: f32,
    block: usize,
    gains: Vec<f32>,
    attack: (f32, usize),
    release: (f32, usize),
    output: f32,
    sync: bool,
    /// Each channel's smoothed gain and the gain last laid on its sound.
    gain: Vec<f32>,
    applied: Vec<f32>,
    /// Each channel's block being gathered, and the one going out.
    gathering: Vec<Vec<f32>>,
    going: Vec<Vec<f32>>,
    at: usize,
}

impl GraphicDynamics {
    pub(crate) fn new(rate: u32) -> GraphicDynamics {
        let fs = rate.max(1) as f32;
        GraphicDynamics {
            rate: fs,
            block: ((0.02 * fs) as usize).max(1),
            gains: vec![1.0; 841],
            attack: (1.0, 0),
            release: (1.0, 0),
            output: 1.0,
            sync: true,
            gain: Vec::new(),
            applied: Vec::new(),
            gathering: Vec::new(),
            going: Vec::new(),
            at: 0,
        }
    }

    /// A time's share of the way a block (all of it up to a block) and its
    /// length in samples (at most a block).
    fn timing(&self, ms: f32) -> (f32, usize) {
        let t = ms / 1000.0;
        let share = if t <= 0.02 {
            1.0
        } else {
            1.0 - 0.1f32.powf(0.02 / t)
        };
        (share, ((t * self.rate) as usize).min(self.block))
    }

    /// The gathered block's gains worked out and laid on it.
    fn finish_block(&mut self) {
        let peaks: Vec<f32> = self
            .gathering
            .iter()
            .map(|b| b.iter().fold(0f32, |m, s| m.max(s.abs())))
            .collect();
        let loudest = peaks.iter().fold(0f32, |m, p| m.max(*p));
        for (c, own) in peaks.iter().enumerate() {
            let peak = if self.sync { loudest } else { *own };
            let target = level_gain(&self.gains, peak);
            let g = &mut self.gain[c];
            let (share, ramp) = if target < *g {
                self.attack
            } else {
                self.release
            };
            *g = (share * target + (1.0 - share) * *g).min(50.0);
            let mut out = (*g * self.output).min(5000.0);
            if out < 0.0002 {
                out = 0.0;
            }
            let from = self.applied[c];
            for (k, s) in self.gathering[c].iter_mut().enumerate() {
                let t = if ramp == 0 || k >= ramp {
                    1.0
                } else {
                    k as f32 / ramp as f32
                };
                *s *= from + (out - from) * t;
            }
            self.applied[c] = out;
        }
        std::mem::swap(&mut self.gathering, &mut self.going);
    }
}

impl Own for GraphicDynamics {
    fn set(&mut self, params: &[f32]) {
        let p = |i: usize| {
            let s = &gd::PARAMS[i];
            params
                .get(i)
                .copied()
                .unwrap_or(s.default)
                .clamp(s.min, s.max)
        };
        self.gains = level_gains(&points(params, gd::PARAMS, gd::GRAPH));
        self.attack = self.timing(p(gd::ATTACK));
        self.release = self.timing(p(gd::RELEASE));
        let auto = if p(gd::AUTO_GAIN) >= 0.5 { 0.98 } else { 1.0 };
        self.output = 10f32.powf(p(gd::OUTPUT) / 20.0) * auto;
        self.sync = p(gd::SYNC) >= 0.5;
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let count = channels.len();
        let block = self.block;
        if self.gathering.len() != count {
            self.gathering = vec![vec![0.0; block]; count];
            self.going = vec![vec![0.0; block]; count];
            self.gain = vec![1.0; count];
            self.applied = vec![1.0; count];
            self.at = 0;
        }
        let mut i = 0;
        while i < n {
            let m = (block - self.at).min(n - i);
            for (c, ch) in channels.iter_mut().enumerate() {
                let span = self.at..self.at + m;
                self.gathering[c][span.clone()].copy_from_slice(&ch[i..i + m]);
                ch[i..i + m].copy_from_slice(&self.going[c][span]);
            }
            self.at += m;
            i += m;
            if self.at == block {
                self.finish_block();
                self.at = 0;
            }
        }
    }

    fn latency(&self) -> usize {
        self.block
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(specs: &[bv_timeline::ParamSpec], values: &[(usize, f32)]) -> Vec<f32> {
        let mut p: Vec<f32> = specs.iter().map(|s| s.default).collect();
        for &(i, v) in values {
            p[i] = v;
        }
        p
    }

    /// A graph's values: its count and points from `at`.
    fn curve(at: usize, pts: &[(f32, f32)]) -> Vec<(usize, f32)> {
        let mut v = vec![(at, pts.len() as f32)];
        for (k, (x, y)) in pts.iter().enumerate() {
            v.push((at + 1 + 2 * k, *x));
            v.push((at + 2 + 2 * k, *y));
        }
        v
    }

    #[test]
    fn level_curves_give_gains_and_silence() {
        // 2:1 above −18 dB: at 0 dB in, −9 dB out.
        let g = level_gains(&[(-84.0, -84.0), (-18.0, -18.0), (0.0, -9.0)]);
        assert!((level_gain(&g, 1.0) - 10f32.powf(-9.0 / 20.0)).abs() < 1e-4);
        assert!((level_gain(&g, 0.01) - 1.0).abs() < 1e-4);
        // A gate: −84 dB out below −50 dB in.
        let g = level_gains(&[(-84.0, -84.0), (-50.0, -84.0), (-49.9, -49.9), (0.0, 0.0)]);
        assert_eq!(level_gain(&g, 0.001), 0.0);
        assert!((level_gain(&g, 0.5) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn amplitude_modulation_follows_its_wave_down_to_the_amplitude() {
        // A square wave: 0 dB the first half, the amplitude (−48 dB) after.
        let mut values = curve(
            am::GRAPH,
            &[(0.0, 0.0), (500.0, 0.0), (501.0, -100.0), (1000.0, -100.0)],
        );
        values.extend([(am::DRY, -80.0), (am::WET, 0.0), (am::FREQUENCY, 1.0)]);
        let mut m = AmplitudeModulation::new(48_000);
        m.set(&with(am::PARAMS, &values));
        let mut x = vec![vec![1.0f32; 48_000]];
        m.process(&mut x);
        assert!((x[0][10_000] - 1.0).abs() < 1e-4);
        assert!(
            (x[0][40_000] - 10f32.powf(-48.0 / 20.0)).abs() < 1e-4,
            "{}",
            x[0][40_000]
        );
        // Its default: a flat wave at −50, a steady −24 dB.
        let mut m = AmplitudeModulation::new(48_000);
        m.set(&with(am::PARAMS, &[]));
        let mut x = vec![vec![1.0f32; 4_800]];
        m.process(&mut x);
        assert!((x[0][2_000] - 10f32.powf(-24.0 / 20.0)).abs() < 1e-4);
    }

    #[test]
    fn distortion_shapes_each_half_on_its_curve() {
        // The positive half 2:1 above −18 dB, the negative left straight.
        let mut values = curve(ds::POSITIVE, &[(-84.0, -84.0), (-18.0, -18.0), (0.0, -9.0)]);
        values.extend([(ds::DRY, -60.0), (ds::WET, 0.0)]);
        let mut d = Distortion::new(48_000);
        d.set(&with(ds::PARAMS, &values));
        let mut x = vec![vec![1.0f32, -1.0, 0.05]];
        d.process(&mut x);
        assert!(
            (x[0][0] - 10f32.powf(-9.0 / 20.0)).abs() < 1e-4,
            "{}",
            x[0][0]
        );
        assert!((x[0][1] + 1.0).abs() < 1e-4);
        assert!((x[0][2] - 0.05).abs() < 1e-4);
    }

    #[test]
    fn graphic_dynamics_limits_a_block_late() {
        // A limiter at −12 dB, instant.
        let mut values = curve(gd::GRAPH, &[(-84.0, -84.0), (-12.0, -12.0), (0.0, -12.0)]);
        values.extend([(gd::ATTACK, 0.0), (gd::RELEASE, 0.0)]);
        let mut g = GraphicDynamics::new(48_000);
        g.set(&with(gd::PARAMS, &values));
        assert_eq!(g.latency(), 960);
        let mut x = vec![vec![1.0f32; 9_600]];
        g.process(&mut x);
        // The first block out is the silence before the sound; then −12 dB.
        assert!(x[0][..960].iter().all(|s| *s == 0.0));
        let want = 10f32.powf(-12.0 / 20.0);
        assert!(
            x[0][2_000..].iter().all(|s| (s - want).abs() < 1e-3),
            "{}",
            x[0][2_000]
        );
    }
}
