//! Reverb, as VEGAS's: the channels' average, darkened and thinned as
//! asked, feeds eight to ten comb filters in parallel (each mode's own
//! delays), each fed back through the same low-pass and falling 60 dB in
//! the decay time; the first two have twins, a little shorter and longer,
//! that make the right side, so the room opens in stereo. Three early
//! reflections (each style's times, levels and places) come from the
//! sound itself; the combs' sound comes after the pre-delay.

use bv_timeline::audio_fx::reverb as rv;

use super::{Own, db_gain};

/// Each mode's combs: how many, and their delays in milliseconds.
const MODES: [(usize, [f32; 10]); 19] = [
    (
        10,
        [23.6, 47.1, 25.5, 27.5, 29.7, 34.5, 32.1, 37.3, 40.4, 19.9],
    ),
    (
        8,
        [23.2, 18.5, 20.6, 16.0, 26.5, 29.3, 13.0, 33.5, 0.0, 0.0],
    ),
    (
        10,
        [53.7, 67.6, 41.1, 72.9, 84.3, 49.6, 45.9, 57.8, 62.5, 78.7],
    ),
    (
        10,
        [13.5, 25.0, 28.4, 12.0, 15.8, 17.8, 10.7, 19.7, 9.4, 22.0],
    ),
    (
        10,
        [54.2, 28.7, 21.0, 49.3, 44.4, 35.9, 12.3, 66.5, 40.3, 60.1],
    ),
    (
        10,
        [70.5, 46.5, 52.8, 39.7, 76.9, 58.5, 64.3, 31.6, 90.8, 83.5],
    ),
    (
        10,
        [46.5, 89.9, 53.0, 58.8, 83.1, 70.5, 3901.7, 76.4, 64.7, 31.6],
    ),
    (
        9,
        [25.0, 34.8, 29.5, 37.5, 32.1, 22.7, 40.5, 43.8, 47.1, 0.0],
    ),
    (
        10,
        [33.0, 35.3, 44.2, 38.2, 62.8, 30.4, 41.5, 47.5, 51.6, 56.5],
    ),
    (
        10,
        [13.4, 24.4, 11.9, 10.3, 4.5, 15.2, 20.3, 8.5, 17.2, 6.5],
    ),
    (10, [31.7, 21.9, 5.0, 27.3, 11.4, 16.0, 6.6, 9.7, 8.2, 13.3]),
    (10, [13.3, 20.3, 12.0, 10.6, 4.7, 14.8, 17.0, 9.1, 6.6, 8.0]),
    (
        10,
        [21.1, 66.4, 28.7, 49.3, 44.4, 36.0, 12.3, 40.3, 54.3, 60.0],
    ),
    (
        10,
        [41.0, 45.8, 49.6, 53.8, 57.8, 62.5, 67.6, 72.9, 78.7, 84.2],
    ),
    (
        10,
        [37.1, 45.9, 62.8, 53.7, 57.8, 17.3, 67.6, 73.0, 78.7, 84.2],
    ),
    (
        9,
        [50.1, 62.8, 53.9, 25.8, 17.2, 67.8, 73.6, 93.9, 84.2, 0.0],
    ),
    (9, [31.7, 15.0, 11.5, 36.0, 19.1, 5.4, 9.4, 26.4, 7.4, 0.0]),
    (10, [3.1, 18.4, 12.9, 11.1, 5.0, 15.5, 6.5, 9.5, 21.7, 7.9]),
    (
        10,
        [29.8, 12.8, 49.7, 20.8, 25.3, 43.0, 16.0, 4.6, 9.1, 36.5],
    ),
];

/// The twins of the first two combs, parts of their delays.
const TWINS: [f32; 2] = [0.93, 1.06];

/// Each style's three reflections: time (ms), level and place (−1 left
/// to 1 right).
const STYLES: [[(f32, f32, f32); 3]; 10] = [
    [(6.1, 0.46, -0.72), (17.0, -0.60, 0.71), (29.2, 0.21, -0.06)],
    [(8.2, 0.56, 0.70), (20.0, -0.60, -0.70), (53.2, 0.18, 0.16)],
    [(11.1, -0.47, -0.76), (17.0, 0.60, 1.00), (29.5, 0.28, 0.35)],
    [
        (16.0, 0.79, 0.64),
        (30.1, -0.80, -1.00),
        (55.0, 0.40, -0.26),
    ],
    [
        (20.0, 1.00, 0.25),
        (29.0, -0.50, -0.29),
        (49.0, 0.40, -0.25),
    ],
    [
        (36.5, 1.00, 0.25),
        (47.1, -0.50, -0.29),
        (62.0, 0.40, -0.25),
    ],
    [(8.2, 0.56, 0.50), (20.0, -0.60, 0.70), (53.2, 0.18, 0.26)],
    [
        (8.2, 0.56, -0.50),
        (20.0, -0.60, -0.70),
        (53.2, 0.18, -0.26),
    ],
    [(16.0, 0.79, 0.80), (30.1, -0.80, 0.90), (55.0, 0.40, 0.25)],
    [
        (16.0, 0.79, -0.80),
        (30.1, -0.80, -0.90),
        (55.0, 0.40, -0.25),
    ],
];

/// What each reflection's time gains, in milliseconds.
const NUDGE: [f32; 3] = [0.051, 0.151, 0.078];

/// Added and taken away again on fed-back state, so a fading signal ends
/// at 0 instead of lingering as a denormal number.
const FLUSH: f32 = 1e-18;

/// A comb filter: a line, the low-pass in its loop, its gain per round.
#[derive(Clone, Debug)]
struct Comb {
    line: Vec<f32>,
    at: usize,
    low: f32,
    gain: f32,
    ms: f32,
}

impl Comb {
    fn new(ms: f32, fs: f32) -> Comb {
        Comb {
            line: vec![0.0; ((ms * fs / 1000.0).round() as usize).max(1)],
            at: 0,
            low: 0.0,
            gain: 0.0,
            ms,
        }
    }

    /// The block `input` through the comb, added to `out`.
    fn run(&mut self, input: &[f32], out: &mut [f32], p: f32) {
        let len = self.line.len();
        let (g, mut low, mut at) = (self.gain, self.low, self.at);
        for (x, o) in input.iter().zip(out.iter_mut()) {
            let y = self.line[at];
            low = (p * low + (1.0 - p) * y + FLUSH) - FLUSH;
            self.line[at] = g * (*x + low);
            at += 1;
            if at == len {
                at = 0;
            }
            *o += y;
        }
        self.low = low;
        self.at = at;
    }
}

/// A plain delay line, `len` long.
#[derive(Clone, Debug, Default)]
struct Delay {
    line: Vec<f32>,
    at: usize,
}

impl Delay {
    fn new(len: usize) -> Delay {
        Delay {
            line: vec![0.0; len.max(1)],
            at: 0,
        }
    }

    /// Writes `x`; what was written `back` samples before it (1 the last).
    fn push(&mut self, x: f32) {
        self.at = (self.at + 1) % self.line.len();
        self.line[self.at] = x;
    }

    fn back(&self, back: usize) -> f32 {
        let n = self.line.len();
        self.line[(self.at + n - back.min(n - 1)) % n]
    }
}

#[derive(Debug)]
pub(crate) struct ReverbFx {
    rate: f32,
    params: Vec<f32>,
    mode: usize,
    /// The mode's combs, then the two twins.
    combs: Vec<Comb>,
    /// The averaged sound, as the combs take it after the pre-delay.
    pre: Delay,
    /// The averaged sound, for the early reflections.
    early: Delay,
    /// The input's low-pass memory and its bass filter's state.
    low: f32,
    bass: [f32; 4],
    /// One block's averaged sound, its pre-delayed copy, and the left,
    /// right and shared combs' sums.
    mono: Vec<f32>,
    late: Vec<f32>,
    sums: [Vec<f32>; 3],
}

impl ReverbFx {
    pub(crate) fn new(rate: u32) -> ReverbFx {
        let fs = rate.max(1) as f32;
        ReverbFx {
            rate: fs,
            params: Vec::new(),
            mode: usize::MAX,
            combs: Vec::new(),
            pre: Delay::new((0.2 * fs) as usize + 2),
            early: Delay::new((0.07 * fs) as usize + 2),
            low: 0.0,
            bass: [0.0; 4],
            mono: Vec::new(),
            late: Vec::new(),
            sums: Default::default(),
        }
    }

    fn p(&self, i: usize) -> f32 {
        self.params
            .get(i)
            .copied()
            .unwrap_or(rv::PARAMS[i].default)
            .clamp(rv::PARAMS[i].min, rv::PARAMS[i].max)
    }
}

impl Own for ReverbFx {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
        let mode = self.p(rv::MODE).round() as usize;
        if mode != self.mode {
            // Another room: its combs start silent.
            self.mode = mode;
            let (n, delays) = MODES[mode.min(MODES.len() - 1)];
            self.combs = delays[..n]
                .iter()
                .map(|&ms| Comb::new(ms, self.rate))
                .collect();
            for (k, twin) in TWINS.iter().enumerate() {
                self.combs.push(Comb::new(delays[k] * twin, self.rate));
            }
        }
        // −60 dB after the decay time: each round loses its share.
        let decay = self.p(rv::DECAY);
        for c in &mut self.combs {
            c.gain = 10f32.powf(-3.0 * c.ms / 1000.0 / decay);
        }
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let count = channels.len();
        if n == 0 || count == 0 {
            return;
        }
        let fs = self.rate;
        let dry = db_gain(self.p(rv::DRY), -80.0);
        let wet = db_gain(self.p(rv::WET), -80.0);
        let early = db_gain(self.p(rv::EARLY), -80.0);
        // The high cut (one pole), on the input and in the combs' loops.
        let p = if self.p(rv::HIGH) >= 0.5 {
            (-2.0 * std::f32::consts::TAU * self.p(rv::HIGH_FREQ) / fs).exp()
        } else {
            0.0
        };
        // The bass cut: a second-order low-pass whose output is taken away.
        let bass = (self.p(rv::BASS) >= 0.5).then(|| {
            let w = std::f32::consts::TAU * self.p(rv::BASS_FREQ).min(0.45 * fs) / fs;
            let (s, c) = w.sin_cos();
            let alpha = s / std::f32::consts::SQRT_2;
            let a0 = 1.0 + alpha;
            let b = (1.0 - c) / 2.0 / a0;
            [b, 2.0 * b, b, -2.0 * c / a0, (1.0 - alpha) / a0]
        });
        let pre = (self.p(rv::PREDELAY) / 1000.0 * fs).round() as usize;
        self.mono.clear();
        self.late.clear();
        let mut low = self.low;
        for i in 0..n {
            let m = channels.iter().map(|c| c[i]).sum::<f32>() / count as f32;
            low = (p * low + (1.0 - p) * m + FLUSH) - FLUSH;
            let mut x = low;
            if let Some([b0, b1, b2, a1, a2]) = bass {
                let [x1, x2, y1, y2] = self.bass;
                let y = (b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2 + FLUSH) - FLUSH;
                self.bass = [x, x1, y, y1];
                x -= y;
            }
            self.mono.push(x);
            self.pre.push(x);
            self.late
                .push(if pre == 0 { x } else { self.pre.back(pre) });
        }
        self.low = low;
        // The combs: the first two (left), their twins (right), the rest
        // (both).
        let shared = self.combs.len() - 2;
        for s in &mut self.sums {
            s.clear();
            s.resize(n, 0.0);
        }
        for (k, comb) in self.combs.iter_mut().enumerate() {
            let side = if k < 2 {
                0
            } else if k >= shared {
                1
            } else {
                2
            };
            comb.run(&self.late, &mut self.sums[side], p);
        }
        let style = STYLES[(self.p(rv::STYLE).round() as usize).min(STYLES.len() - 1)];
        let taps: Vec<usize> = style
            .iter()
            .zip(NUDGE)
            .map(|((ms, _, _), nudge)| ((ms + nudge) / 1000.0 * fs).round() as usize)
            .collect();
        for i in 0..n {
            self.early.push(self.mono[i]);
            let (mut left, mut right) = (0.0, 0.0);
            for ((_, level, place), back) in style.iter().zip(&taps) {
                let tap = level * self.early.back(*back);
                let to_right = (place + 1.0) / 2.0;
                left += tap * (1.0 - to_right);
                right += tap * to_right;
            }
            for (c, ch) in channels.iter_mut().enumerate() {
                let (combs, reflections) = if count == 1 {
                    (self.sums[0][i] + self.sums[2][i], left + right)
                } else if c % 2 == 0 {
                    (self.sums[0][i] + self.sums[2][i], left)
                } else {
                    (self.sums[1][i] + self.sums[2][i], right)
                };
                ch[i] = dry * ch[i] + wet * combs + early * reflections;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reverb(values: &[(usize, f32)]) -> ReverbFx {
        let mut r = ReverbFx::new(48_000);
        let mut p: Vec<f32> = rv::PARAMS.iter().map(|s| s.default).collect();
        for &(i, v) in values {
            p[i] = v;
        }
        r.set(&p);
        r
    }

    fn energy(x: &[f32]) -> f32 {
        x.iter().map(|s| s * s).sum()
    }

    #[test]
    fn its_tail_falls_60_db_in_the_decay_time() {
        // Mono, combs only, no cuts: a click's tail, 1 s apart, 60 dB down.
        let mut r = reverb(&[
            (rv::DRY, -80.0),
            (rv::WET, 0.0),
            (rv::EARLY, -80.0),
            (rv::DECAY, 1.0),
            (rv::PREDELAY, 0.0),
            (rv::BASS, 0.0),
            (rv::HIGH, 0.0),
        ]);
        let mut x = [vec![0.0f32; 144_000]];
        x[0][0] = 1.0;
        for block in x[0].chunks_mut(4_800) {
            let mut b = vec![block.to_vec()];
            r.process(&mut b);
            block.copy_from_slice(&b[0]);
        }
        let first = energy(&x[0][24_000..48_000]);
        let later = energy(&x[0][72_000..96_000]);
        let db = 10.0 * (later / first).log10();
        assert!((db + 60.0).abs() < 4.0, "{db}");
    }

    #[test]
    fn the_pre_delay_holds_the_tail_back() {
        let mut r = reverb(&[
            (rv::DRY, -80.0),
            (rv::WET, 0.0),
            (rv::EARLY, -80.0),
            (rv::PREDELAY, 100.0),
            (rv::BASS, 0.0),
            (rv::HIGH, 0.0),
        ]);
        let mut x = vec![vec![0.0f32; 9_600]];
        x[0][0] = 1.0;
        r.process(&mut x);
        // Nothing before 100 ms plus the shortest comb.
        assert!(x[0][..4_800].iter().all(|s| *s == 0.0));
        assert!(energy(&x[0][4_800..]) > 0.0);
    }

    #[test]
    fn the_early_reflections_and_the_stereo_sides() {
        // Early only, style 1: 6.151 ms (295 samples), 46 %, 72 % left.
        let mut r = reverb(&[
            (rv::DRY, -80.0),
            (rv::WET, -80.0),
            (rv::EARLY, 0.0),
            (rv::STYLE, 0.0),
            (rv::BASS, 0.0),
            (rv::HIGH, 0.0),
        ]);
        let mut x = vec![vec![0.0f32; 4_800], vec![0.0f32; 4_800]];
        x[0][0] = 1.0;
        x[1][0] = 1.0;
        r.process(&mut x);
        let at = 295;
        assert!((x[0][at] - 0.46 * 0.86).abs() < 1e-4, "{}", x[0][at]);
        assert!((x[1][at] - 0.46 * 0.14).abs() < 1e-4, "{}", x[1][at]);
        // The combs: the left and the right differ.
        let mut r = reverb(&[(rv::DRY, -80.0), (rv::WET, 0.0), (rv::EARLY, -80.0)]);
        let mut x = vec![vec![0.0f32; 9_600], vec![0.0f32; 9_600]];
        x[0][0] = 1.0;
        x[1][0] = 1.0;
        r.process(&mut x);
        assert!(x[0].iter().zip(&x[1]).any(|(a, b)| (a - b).abs() > 1e-3));
    }

    #[test]
    fn dry_only_leaves_the_sound_alone() {
        let mut r = reverb(&[(rv::DRY, 0.0), (rv::WET, -80.0), (rv::EARLY, -80.0)]);
        let x: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.01).sin()).collect();
        let mut ch = vec![x.clone()];
        r.process(&mut ch);
        assert_eq!(ch[0], x);
    }
}
