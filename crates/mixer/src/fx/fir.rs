//! The EQs applied as FIR filters through FFTs, as VEGAS's: Parametric EQ
//! and Express Graphic EQ. Each draws a frequency response bin by bin —
//! the Parametric EQ a shelf, a band-pass or a notch/boost with straight
//! ramps at its edges; the Express Graphic EQ ten bands an octave apart,
//! joined by straight lines in dB along the octaves — times the output
//! gain. [`Fir`] makes it a linear-phase FIR filter half the FFT long
//! (windowed) and applies it through FFTs, half an FFT of sound at a time
//! (overlap-add). Two channels share each transform, one as its real part
//! and the other as its imaginary, the filter being the same on both. A
//! new response takes over at the next half FFT.

use bv_timeline::audio_fx::{express_graphic_eq as geq, graphic_eq as ge, parametric_eq as pe};

use crate::fft::{Complex, Plan};

use super::Own;

/// How far past a band's edge its ramp goes (a ratio of frequencies).
const EDGE: f64 = 1.2;

/// A linear-phase FIR filter applied through FFTs.
#[derive(Debug)]
pub(crate) struct Fir {
    rate: f64,
    size: usize,
    plan: Plan,
    /// The filter's spectrum, `size` bins.
    filter: Vec<Complex>,
    /// Each channel's sound gathered in this half, its sound due out in
    /// this half, and what the last transform left for the next.
    input: Vec<Vec<f32>>,
    output: Vec<Vec<f32>>,
    tail: Vec<Vec<f32>>,
    /// Where in the half the next sample goes.
    at: usize,
    work: Vec<Complex>,
}

impl Fir {
    pub(crate) fn new(rate: u32) -> Fir {
        Fir {
            rate: rate.max(1) as f64,
            size: 0,
            plan: Plan::default(),
            filter: Vec::new(),
            input: Vec::new(),
            output: Vec::new(),
            tail: Vec::new(),
            at: 0,
            work: Vec::new(),
        }
    }

    /// An FFT `size` long: another length starts the sound in flight again.
    fn resize(&mut self, size: usize) {
        if size != self.size {
            self.size = size;
            self.plan = Plan::new(size);
            for v in [&mut self.input, &mut self.output, &mut self.tail] {
                v.clear();
            }
            self.at = 0;
        }
    }

    /// The filter for `response`, a linear gain at each frequency (Hz).
    fn design(&mut self, response: impl Fn(f64) -> f64) {
        let n = self.size;
        let taps = n / 2;
        // The zero-phase response (real and even), back to time.
        let mut w: Vec<Complex> = (0..n)
            .map(|k| {
                let bin = k.min(n - k) as f64;
                Complex::new(response(bin * self.rate / n as f64), 0.0)
            })
            .collect();
        self.plan.run(&mut w, true);
        // Half an FFT of taps centred on the middle one, through a
        // Blackman window: linear phase, `taps / 2` samples late.
        let mut h = vec![Complex::ZERO; n];
        for (m, t) in h.iter_mut().take(taps).enumerate() {
            let x = (m as f64 - (taps / 2) as f64) / (taps / 2) as f64;
            let window = 0.42
                + 0.5 * (std::f64::consts::PI * x).cos()
                + 0.08 * (std::f64::consts::TAU * x).cos();
            t.re = w[(m + n - taps / 2) % n].re * window;
        }
        self.plan.run(&mut h, false);
        self.filter = h;
    }

    /// The gathered half through the filter, two channels at a time.
    fn convolve(&mut self) {
        let n = self.size;
        let half = n / 2;
        let count = self.input.len();
        for a in (0..count).step_by(2) {
            let b = (a + 1 < count).then_some(a + 1);
            self.work.clear();
            self.work.resize(n, Complex::ZERO);
            for (k, w) in self.work.iter_mut().take(half).enumerate() {
                w.re = self.input[a][k] as f64;
                w.im = b.map_or(0.0, |b| self.input[b][k] as f64);
            }
            self.plan.run(&mut self.work, false);
            for (w, h) in self.work.iter_mut().zip(self.filter.iter()) {
                *w = *w * *h;
            }
            self.plan.run(&mut self.work, true);
            let (now, next) = self.work.split_at(half);
            for k in 0..half {
                self.output[a][k] = now[k].re as f32 + self.tail[a][k];
                self.tail[a][k] = next[k].re as f32;
            }
            if let Some(b) = b {
                for k in 0..half {
                    self.output[b][k] = now[k].im as f32 + self.tail[b][k];
                    self.tail[b][k] = next[k].im as f32;
                }
            }
        }
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let half = self.size / 2;
        for v in [&mut self.input, &mut self.output, &mut self.tail] {
            v.resize_with(channels.len(), || vec![0.0; half]);
        }
        let mut i = 0;
        while i < n {
            let m = (half - self.at).min(n - i);
            let span = self.at..self.at + m;
            for (c, ch) in channels.iter_mut().enumerate() {
                let samples = &mut ch[i..i + m];
                self.input[c][span.clone()].copy_from_slice(samples);
                samples.copy_from_slice(&self.output[c][span.clone()]);
            }
            self.at += m;
            i += m;
            if self.at == half {
                self.convolve();
                self.at = 0;
            }
        }
    }

    fn latency(&self) -> usize {
        // Half an FFT gathered, then the filter's own half of its taps.
        self.size / 2 + self.size / 4
    }
}

#[derive(Debug)]
pub(crate) struct ParametricEq {
    fir: Fir,
}

impl ParametricEq {
    pub(crate) fn new(rate: u32) -> ParametricEq {
        ParametricEq {
            fir: Fir::new(rate),
        }
    }
}

/// Parametric EQ's response with values `p` at sampling rate `fs`: its
/// linear gain at `hz` (before the output gain).
fn parametric(p: &impl Fn(usize) -> f32, fs: f64, hz: f64) -> f64 {
    let amount = 10f64.powf(p(pe::AMOUNT) as f64 / 20.0);
    let f = (p(pe::FREQUENCY) as f64).clamp(1.0, 0.47 * fs);
    let r = 2f64.powf(p(pe::WIDTH) as f64);
    let ramp = |x: f64, (x0, g0): (f64, f64), (x1, g1): (f64, f64)| {
        g0 + (g1 - g0) * ((x - x0) / (x1 - x0)).clamp(0.0, 1.0)
    };
    match p(pe::STYLE).round() as i32 {
        // A low shelf: the amount below `f / r`, nothing above `f · r`.
        0 => ramp(hz, (f / r, amount), (f * r, 1.0)),
        // A high shelf: the other way round.
        1 => ramp(hz, (f / r, 1.0), (f * r, amount)),
        // The bands: half the width each side of `f`.
        style => {
            let s = r.sqrt();
            let (lo, hi) = (f / s, f * s);
            let (inside, outside) = if style == 2 {
                (1.0, amount)
            } else {
                (amount, 1.0)
            };
            if hz < lo {
                ramp(hz, (lo / EDGE, outside), (lo, inside))
            } else {
                ramp(hz, (hi, inside), (hi * EDGE, outside))
            }
        }
    }
}

impl Own for ParametricEq {
    fn set(&mut self, params: &[f32]) {
        let p = |i: usize| {
            let s = &pe::PARAMS[i];
            params
                .get(i)
                .copied()
                .unwrap_or(s.default)
                .clamp(s.min, s.max)
        };
        let accuracy = p(pe::ACCURACY).round().clamp(0.0, 2.0) as usize;
        self.fir.resize(pe::FFT[accuracy]);
        let out = 10f64.powf(p(pe::OUTPUT) as f64 / 20.0);
        let fs = self.fir.rate;
        self.fir.design(|hz| parametric(&p, fs, hz) * out);
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        self.fir.process(channels);
    }

    fn latency(&self) -> usize {
        self.fir.latency()
    }
}

/// Express Graphic EQ: each band's gain at its centre, straight lines in
/// dB between them along the octaves, flat past the ends; a band at its
/// lowest (−60 dB) is silence.
#[derive(Debug)]
pub(crate) struct ExpressGraphicEq {
    fir: Fir,
}

impl ExpressGraphicEq {
    pub(crate) fn new(rate: u32) -> ExpressGraphicEq {
        ExpressGraphicEq {
            fir: Fir::new(rate),
        }
    }
}

impl Own for ExpressGraphicEq {
    fn set(&mut self, params: &[f32]) {
        let p = |i: usize| {
            let s = &geq::PARAMS[i];
            params
                .get(i)
                .copied()
                .unwrap_or(s.default)
                .clamp(s.min, s.max)
        };
        let accuracy = p(geq::ACCURACY).round().clamp(0.0, 2.0) as usize;
        self.fir.resize(pe::FFT[accuracy]);
        let gains: Vec<(f64, f64)> = geq::CENTERS
            .iter()
            .enumerate()
            .map(|(k, hz)| ((*hz as f64).log2(), p(geq::BANDS + k) as f64))
            .collect();
        let out = p(geq::OUTPUT) as f64;
        self.fir.design(|hz| {
            let x = hz.max(1.0).log2();
            let at = gains.partition_point(|g| g.0 <= x);
            let db = if at == 0 {
                gains[0].1
            } else if at == gains.len() {
                gains[gains.len() - 1].1
            } else {
                let ((x0, g0), (x1, g1)) = (gains[at - 1], gains[at]);
                g0 + (g1 - g0) * (x - x0) / (x1 - x0)
            };
            if db <= -60.0 {
                0.0
            } else {
                10f64.powf((db + out) / 20.0)
            }
        });
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        self.fir.process(channels);
    }

    fn latency(&self) -> usize {
        self.fir.latency()
    }
}

/// Graphic EQ: the drawn response, straight lines in dB along the octaves
/// (flat past its ends), its bottom (−60 dB) silence, times the output
/// gain.
#[derive(Debug)]
pub(crate) struct GraphicEq {
    fir: Fir,
}

impl GraphicEq {
    pub(crate) fn new(rate: u32) -> GraphicEq {
        GraphicEq {
            fir: Fir::new(rate),
        }
    }
}

impl Own for GraphicEq {
    fn set(&mut self, params: &[f32]) {
        let p = |i: usize| {
            let s = &ge::PARAMS[i];
            params
                .get(i)
                .copied()
                .unwrap_or(s.default)
                .clamp(s.min, s.max)
        };
        let accuracy = p(ge::ACCURACY).round().clamp(0.0, 2.0) as usize;
        self.fir.resize(pe::FFT[accuracy]);
        let curve: Vec<(f32, f32)> = super::graphs::points(params, ge::PARAMS, ge::GRAPH)
            .into_iter()
            .map(|(hz, db)| (hz.max(1.0).log2(), db))
            .collect();
        let out = p(ge::OUTPUT) as f64;
        self.fir.design(|hz| {
            let db = super::graphs::along(&curve, (hz.max(1.0) as f32).log2()) as f64;
            if db <= -60.0 {
                0.0
            } else {
                10f64.powf((db + out) / 20.0)
            }
        });
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        self.fir.process(channels);
    }

    fn latency(&self) -> usize {
        self.fir.latency()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eq(values: [f32; 6]) -> ParametricEq {
        let mut e = ParametricEq::new(48_000);
        e.set(&values);
        e
    }

    /// The level (dB) a sine at `hz` comes out at, once through.
    fn level(e: &mut ParametricEq, hz: f32) -> f32 {
        let n = 48_000;
        let x: Vec<f32> = (0..n)
            .map(|i| (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin())
            .collect();
        let mut ch = [x.clone(), x];
        for block in 0..(n / 1000) {
            let mut b: Vec<Vec<f32>> = ch
                .iter()
                .map(|c| c[block * 1000..(block + 1) * 1000].to_vec())
                .collect();
            e.process(&mut b);
            for (c, part) in ch.iter_mut().zip(b) {
                c[block * 1000..(block + 1) * 1000].copy_from_slice(&part);
            }
        }
        // Its amplitude from its power (a sampled sine's peak can miss it).
        let tail = &ch[1][24_000..];
        let power = tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32;
        10.0 * (2.0 * power).log10()
    }

    #[test]
    fn its_latency_is_three_quarters_of_the_fft() {
        let mut e = eq([0.0, 0.0, 3.0, 1000.0, 1.0, 1.0]);
        assert_eq!(e.latency(), 768);
        // A click comes out that late, whole (an amount of 0 dB).
        let mut x = vec![vec![0.0f32; 4096]];
        x[0][0] = 1.0;
        e.process(&mut x);
        let (at, peak) = x[0].iter().enumerate().fold((0, 0f32), |(i, m), (k, s)| {
            if s.abs() > m { (k, s.abs()) } else { (i, m) }
        });
        assert_eq!(at, 768);
        assert!((peak - 1.0).abs() < 0.01, "{peak}");
    }

    #[test]
    fn a_notch_cuts_its_band_and_leaves_the_rest() {
        // "Remove frequencies around 1500 Hz by 25 dB".
        let mut e = eq([0.0, -25.0, 3.0, 1500.0, 1.0, 2.0]);
        assert!(
            (level(&mut e, 1500.0) + 25.0).abs() < 1.0,
            "{}",
            level(&mut e, 1500.0)
        );
        assert!(level(&mut e, 200.0).abs() < 0.5);
        assert!(level(&mut e, 8000.0).abs() < 0.5);
    }

    #[test]
    fn a_band_pass_keeps_its_band() {
        // "Phone line effect": 400 to 1600 Hz kept, the rest 25 dB down.
        let mut e = eq([0.0, -25.0, 2.0, 800.0, 2.0, 1.0]);
        assert!(level(&mut e, 800.0).abs() < 0.5);
        assert!((level(&mut e, 5000.0) + 25.0).abs() < 1.5);
        assert!(
            (level(&mut e, 100.0) + 25.0).abs() < 3.0,
            "{}",
            level(&mut e, 100.0)
        );
    }

    #[test]
    fn shelves_and_output_gain() {
        // "Hiss removal": above 6 kHz · √2 down 40 dB, the lows alone.
        let mut e = eq([0.0, -40.0, 1.0, 6000.0, 0.5, 2.0]);
        assert!((level(&mut e, 12_000.0) + 40.0).abs() < 2.0);
        assert!(level(&mut e, 1000.0).abs() < 0.5);
        // A low shelf, and −6 dB of output on everything.
        let mut e = eq([-6.0, -15.0, 0.0, 400.0, 0.5, 2.0]);
        assert!(
            (level(&mut e, 100.0) + 21.0).abs() < 1.0,
            "{}",
            level(&mut e, 100.0)
        );
        assert!((level(&mut e, 5000.0) + 6.0).abs() < 0.5);
    }
}
