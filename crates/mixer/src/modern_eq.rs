//! The Modern Equalizer, as VEGAS's Help describes it: up to eight bands —
//! high pass, low pass, low and high shelving, band pass and peaking —,
//! the passes 6 to 36 dB an octave (the others a fixed 12), each on both
//! channels or on the left, the right, the middle or the sides alone; then
//! Mono Below (the sides high-passed, for a mono low end), Stereo Width
//! (the sides scaled) and Output Gain; Global Scale scales every band's
//! gain (below 0 a boost becomes a cut). Butterworth sections from the RBJ
//! cookbook's biquads (`dsp.rs`); [`band_db`] and [`total_db`] give the
//! curves the interface draws.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Instant;

use bv_timeline::Effect;
/// How many bands it has; its global parameters (Global Scale, Mono
/// Below, Stereo Width, Output Gain) and each band's (in use, on, type,
/// frequency, gain, Q, slope, routing), as the model lays them out.
pub use bv_timeline::fx::modern_eq::{BANDS, GLOBALS, OVERSAMPLING, PER_BAND};

use crate::dsp::{Biquad, Coeffs, Shape, design};

/// Mono Below's filter is off under this frequency (its slider's start).
pub const MONO_BELOW_OFF: f32 = 10.0;

/// A band's shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandType {
    HighPass,
    LowPass,
    LowShelf,
    HighShelf,
    BandPass,
    Peaking,
}

impl BandType {
    /// In the order of the Filter Type buttons.
    pub const ALL: [BandType; 6] = [
        BandType::HighPass,
        BandType::LowPass,
        BandType::LowShelf,
        BandType::HighShelf,
        BandType::BandPass,
        BandType::Peaking,
    ];

    fn of(v: f32) -> BandType {
        BandType::ALL[(v.round().max(0.0) as usize).min(5)]
    }

    fn index(self) -> f32 {
        BandType::ALL.iter().position(|&t| t == self).unwrap_or(5) as f32
    }

    /// Its name (English; the interface translates it).
    pub fn name(self) -> &'static str {
        match self {
            BandType::HighPass => "High Pass",
            BandType::LowPass => "Low Pass",
            BandType::LowShelf => "Low Shelving",
            BandType::HighShelf => "High Shelving",
            BandType::BandPass => "Band Pass",
            BandType::Peaking => "Peaking",
        }
    }

    /// Whether it has a gain (the passes have none).
    pub fn has_gain(self) -> bool {
        matches!(
            self,
            BandType::LowShelf | BandType::HighShelf | BandType::Peaking
        )
    }

    /// Whether its slope can be picked (the others' is 12 dB an octave).
    pub fn has_slope(self) -> bool {
        matches!(
            self,
            BandType::HighPass | BandType::LowPass | BandType::BandPass
        )
    }

    /// Whether its Q sets its width.
    pub fn has_q(self) -> bool {
        matches!(self, BandType::Peaking | BandType::BandPass)
    }
}

/// Which part of the sound a band works on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Stereo,
    Left,
    Right,
    Mid,
    Side,
}

impl Route {
    pub const ALL: [Route; 5] = [
        Route::Stereo,
        Route::Left,
        Route::Right,
        Route::Mid,
        Route::Side,
    ];

    fn of(v: f32) -> Route {
        Route::ALL[(v.round().max(0.0) as usize).min(4)]
    }

    fn index(self) -> f32 {
        Route::ALL.iter().position(|&r| r == self).unwrap_or(0) as f32
    }

    /// Its name (English; the interface translates it).
    pub fn name(self) -> &'static str {
        match self {
            Route::Stereo => "Stereo",
            Route::Left => "Left",
            Route::Right => "Right",
            Route::Mid => "Mid",
            Route::Side => "Side",
        }
    }
}

/// One band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    /// Added (the + button, a double-click on the graph); deleted bands
    /// keep their place, unused.
    pub used: bool,
    /// On, or bypassed.
    pub on: bool,
    pub kind: BandType,
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
    /// dB an octave: 6, 12, 18, 24, 30 or 36 (the passes').
    pub slope: u8,
    pub route: Route,
}

impl Band {
    /// Whether it changes the sound.
    pub fn active(&self) -> bool {
        self.used && self.on
    }

    /// Its order as it filters: its slope's for the passes, 12 dB an
    /// octave's for the others.
    fn order(&self) -> usize {
        if self.kind.has_slope() {
            usize::from(self.slope / 6).clamp(1, 6)
        } else {
            2
        }
    }
}

/// The equalizer's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Global Scale: every band's gain times this (−2 to 2).
    pub scale: f32,
    /// Mono Below: the sides high-passed here (Hz; off under
    /// [`MONO_BELOW_OFF`]).
    pub mono_below: f32,
    /// Stereo Width: the sides times this (0 to 2).
    pub width: f32,
    pub output_db: f32,
    pub bands: [Band; BANDS],
    /// Oversampling: the bands run at this many times the rate (1, 2, 4
    /// or 8), so their curves keep their shape near the top.
    pub oversampling: u8,
}

/// Where band `b`'s parameters start.
pub fn band_at(b: usize) -> usize {
    bv_timeline::fx::modern_eq::at(b, 0)
}

impl Settings {
    pub fn of(e: &Effect) -> Settings {
        let p = |i: usize| e.param(i);
        Settings {
            scale: (p(0) / 100.0).clamp(-2.0, 2.0),
            mono_below: p(1).max(0.0),
            width: (p(2) / 100.0).clamp(0.0, 2.0),
            output_db: p(3),
            bands: std::array::from_fn(|b| {
                let at = band_at(b);
                Band {
                    used: p(at) >= 0.5,
                    on: p(at + 1) >= 0.5,
                    kind: BandType::of(p(at + 2)),
                    freq: p(at + 3).clamp(10.0, 24_000.0),
                    gain_db: p(at + 4).clamp(-36.0, 36.0),
                    q: p(at + 5).clamp(0.05, 30.0),
                    slope: ((p(at + 6) / 6.0).round().clamp(1.0, 6.0) * 6.0) as u8,
                    route: Route::of(p(at + 7)),
                }
            }),
            oversampling: 1 << (p(OVERSAMPLING).round().clamp(0.0, 3.0) as u8),
        }
    }

    /// These settings into `e`'s parameters.
    pub fn write(&self, e: &mut Effect) {
        e.set_param(0, self.scale * 100.0);
        e.set_param(1, self.mono_below);
        e.set_param(2, self.width * 100.0);
        e.set_param(3, self.output_db);
        for (b, band) in self.bands.iter().enumerate() {
            let at = band_at(b);
            e.set_param(at, f32::from(u8::from(band.used)));
            e.set_param(at + 1, f32::from(u8::from(band.on)));
            e.set_param(at + 2, band.kind.index());
            e.set_param(at + 3, band.freq);
            e.set_param(at + 4, band.gain_db);
            e.set_param(at + 5, band.q);
            e.set_param(at + 6, f32::from(band.slope));
            e.set_param(at + 7, band.route.index());
        }
        e.set_param(
            OVERSAMPLING,
            f32::from(self.oversampling.max(1).trailing_zeros() as u8),
        );
    }

    /// Whether Mono Below filters the sides.
    pub fn mono_below_on(&self) -> bool {
        self.mono_below >= MONO_BELOW_OFF
    }
}

/// The Qs of a Butterworth filter of `order` (1 to 6) as second-order
/// sections, and whether it also has a first-order one (odd orders).
fn butterworth(order: usize) -> (bool, Vec<f32>) {
    let n = order.max(1);
    let odd = n % 2 == 1;
    let qs = (0..n / 2)
        .map(|k| {
            // Each pair of poles' angle from the negative real axis.
            let phi = if odd {
                std::f32::consts::PI * (k + 1) as f32 / n as f32
            } else {
                std::f32::consts::PI * (2 * k + 1) as f32 / (2 * n) as f32
            };
            1.0 / (2.0 * phi.cos())
        })
        .collect();
    (odd, qs)
}

/// A first-order low or high pass (`high`) at `f`.
fn first_order(high: bool, rate: f32, f: f32) -> Coeffs {
    let k = (std::f32::consts::PI * f.clamp(10.0, rate * 0.49) / rate).tan();
    let a1 = (k - 1.0) / (k + 1.0);
    let b0 = if high { 1.0 } else { k } / (1.0 + k);
    Coeffs {
        b0,
        b1: if high { -b0 } else { b0 },
        b2: 0.0,
        a1,
        a2: 0.0,
    }
}

/// A Butterworth pass of `order` at `f`, as sections.
fn pass(high: bool, order: usize, rate: f32, f: f32) -> Vec<Coeffs> {
    let shape = if high {
        Shape::HighPass
    } else {
        Shape::LowPass
    };
    let (odd, qs) = butterworth(order);
    let mut v: Vec<Coeffs> = qs.iter().map(|&q| design(shape, rate, f, q, 0.0)).collect();
    if odd {
        v.push(first_order(high, rate, f));
    }
    v
}

/// The sections band `b` is made of at `rate`, its gain times `scale`.
fn sections(b: &Band, scale: f32, rate: f32) -> Vec<Coeffs> {
    match b.kind {
        BandType::HighPass | BandType::LowPass => {
            pass(b.kind == BandType::HighPass, b.order(), rate, b.freq)
        }
        // A section for each 6 dB an octave of its sides, each wider so
        // that together they keep the band's width (half power where one
        // alone of Q would have it).
        BandType::BandPass => {
            let n = b.order();
            let q = b.q * (2f32.powf(1.0 / n as f32) - 1.0).sqrt();
            vec![design(Shape::BandPass, rate, b.freq, q.max(0.05), 0.0); n]
        }
        BandType::LowShelf => vec![design(
            Shape::LowShelf,
            rate,
            b.freq,
            0.707,
            b.gain_db * scale,
        )],
        BandType::HighShelf => vec![design(
            Shape::HighShelf,
            rate,
            b.freq,
            0.707,
            b.gain_db * scale,
        )],
        BandType::Peaking => vec![design(Shape::Peak, rate, b.freq, b.q, b.gain_db * scale)],
    }
}

/// A section's gain (dB) at `f`.
fn section_db(c: &Coeffs, rate: f32, f: f32) -> f32 {
    let w = std::f64::consts::TAU * f64::from(f) / f64::from(rate);
    let (s1, c1) = w.sin_cos();
    let (s2, c2) = (2.0 * w).sin_cos();
    let [b0, b1, b2, a1, a2] = [c.b0, c.b1, c.b2, c.a1, c.a2].map(f64::from);
    let nr = b0 + b1 * c1 + b2 * c2;
    let ni = -(b1 * s1 + b2 * s2);
    let dr = 1.0 + a1 * c1 + a2 * c2;
    let di = -(a1 * s1 + a2 * s2);
    let power = (nr * nr + ni * ni) / (dr * dr + di * di).max(1e-30);
    (10.0 * power.max(1e-24).log10()) as f32
}

/// The gains (dB) at `freqs` of sections in a row.
fn curve(secs: &[Coeffs], rate: f32, freqs: &[f32]) -> Vec<f32> {
    freqs
        .iter()
        .map(|&f| secs.iter().map(|c| section_db(c, rate, f)).sum())
        .collect()
}

/// Band `b`'s own curve: its gain (dB) at each of `freqs`, Global Scale's
/// `scale` applied; 0 when it does nothing.
pub fn band_curve(b: &Band, scale: f32, rate: f32, freqs: &[f32]) -> Vec<f32> {
    if !b.active() {
        return vec![0.0; freqs.len()];
    }
    curve(&sections(b, scale, rate), rate, freqs)
}

/// [`band_curve`] at one frequency.
pub fn band_db(b: &Band, scale: f32, rate: f32, f: f32) -> f32 {
    band_curve(b, scale, rate, &[f])[0]
}

/// Mono Below's slope: 24 dB an octave.
const MONO_BELOW_ORDER: usize = 4;

/// Mono Below's curve on the sides (dB at each of `freqs`); `None` when it
/// is off.
pub fn mono_below_curve(s: &Settings, rate: f32, freqs: &[f32]) -> Option<Vec<f32>> {
    s.mono_below_on().then(|| {
        curve(
            &pass(true, MONO_BELOW_ORDER, rate, s.mono_below),
            rate,
            freqs,
        )
    })
}

/// The whole curve at each of `freqs` (dB): every band's, as the middle
/// of the sound hears them (left, right and side bands count half), and
/// the output gain.
pub fn total_curve(s: &Settings, rate: f32, freqs: &[f32]) -> Vec<f32> {
    let mut out = vec![s.output_db; freqs.len()];
    for b in s.bands.iter().filter(|b| b.active()) {
        let weight = match b.route {
            Route::Stereo | Route::Mid => 1.0,
            Route::Left | Route::Right | Route::Side => 0.5,
        };
        for (o, db) in out.iter_mut().zip(band_curve(b, s.scale, rate, freqs)) {
            *o += db * weight;
        }
    }
    out
}

/// [`total_curve`] at one frequency.
pub fn total_db(s: &Settings, rate: f32, f: f32) -> f32 {
    total_curve(s, rate, &[f])[0]
}

/// The equalizer running: each band's sections with their state, Mono
/// Below's filter, the width and the output gain; oversampled, the filters
/// that take the sound up to the higher rate and back.
#[derive(Clone, Debug)]
pub(crate) struct Eq {
    bands: Vec<(Route, Vec<Biquad>)>,
    mono_below: Vec<Biquad>,
    width: f32,
    gain: f32,
    factor: usize,
    up: Vec<Biquad>,
    down: Vec<Biquad>,
}

/// The order of the filters around oversampling.
const OVERSAMPLE_ORDER: usize = 8;

impl Eq {
    pub(crate) fn new(e: &Effect, rate: u32, channels: usize) -> Eq {
        let s = Settings::of(e);
        let factor = usize::from(s.oversampling.max(1));
        let r = (rate as usize * factor) as f32;
        let n = channels.max(2);
        // Around oversampling: what is above the sound's own top cut away
        // going up and coming back down.
        let guard = || -> Vec<Biquad> {
            if factor == 1 {
                return Vec::new();
            }
            pass(false, OVERSAMPLE_ORDER, r, rate as f32 * 0.45)
                .into_iter()
                .map(|c| Biquad::new(c, n))
                .collect()
        };
        Eq {
            bands: s
                .bands
                .iter()
                .filter(|b| b.active())
                .map(|b| {
                    let secs = sections(b, s.scale, r)
                        .into_iter()
                        .map(|c| Biquad::new(c, n))
                        .collect();
                    (b.route, secs)
                })
                .collect(),
            mono_below: if s.mono_below_on() {
                pass(true, MONO_BELOW_ORDER, r, s.mono_below)
                    .into_iter()
                    .map(|c| Biquad::new(c, 1))
                    .collect()
            } else {
                Vec::new()
            },
            width: s.width,
            gain: 10f32.powf(s.output_db / 20.0),
            factor,
            up: guard(),
            down: guard(),
        }
    }

    /// Processes the channels (planar) in place: the first two are the
    /// left and the right; oversampled, at the higher rate in between.
    pub(crate) fn process(&mut self, channels: &mut [Vec<f32>]) {
        if self.factor <= 1 {
            self.process_at_rate(channels);
            return;
        }
        let k = self.factor;
        let mut high: Vec<Vec<f32>> = channels
            .iter()
            .enumerate()
            .map(|(c, ch)| {
                let mut v = Vec::with_capacity(ch.len() * k);
                for &x in ch {
                    for j in 0..k {
                        let s = if j == 0 { x * k as f32 } else { 0.0 };
                        v.push(self.up.iter_mut().fold(s, |y, f| f.process(c, y)));
                    }
                }
                v
            })
            .collect();
        self.process_at_rate(&mut high);
        for (c, (ch, v)) in channels.iter_mut().zip(&high).enumerate() {
            for (i, out) in ch.iter_mut().enumerate() {
                let mut y = 0.0;
                for j in 0..k {
                    y = self
                        .down
                        .iter_mut()
                        .fold(v[i * k + j], |y, f| f.process(c, y));
                }
                *out = y;
            }
        }
    }

    fn process_at_rate(&mut self, channels: &mut [Vec<f32>]) {
        fn run(secs: &mut [Biquad], slot: usize, x: f32) -> f32 {
            secs.iter_mut().fold(x, |v, s| s.process(slot, v))
        }
        let n = channels.first().map_or(0, Vec::len);
        let stereo = channels.len() >= 2;
        let sides = stereo && (!self.mono_below.is_empty() || (self.width - 1.0).abs() > 1e-6);
        for i in 0..n {
            for (route, secs) in &mut self.bands {
                match (*route, stereo) {
                    (Route::Stereo, _) => {
                        for (c, ch) in channels.iter_mut().enumerate() {
                            ch[i] = run(secs, c, ch[i]);
                        }
                    }
                    (Route::Left, true) => channels[0][i] = run(secs, 0, channels[0][i]),
                    (Route::Right, true) => channels[1][i] = run(secs, 1, channels[1][i]),
                    (Route::Mid | Route::Side, true) => {
                        let (l, r) = (channels[0][i], channels[1][i]);
                        let (mut m, mut s) = ((l + r) * 0.5, (l - r) * 0.5);
                        if *route == Route::Mid {
                            m = run(secs, 0, m);
                        } else {
                            s = run(secs, 1, s);
                        }
                        channels[0][i] = m + s;
                        channels[1][i] = m - s;
                    }
                    // One channel is the left, the right and the middle; it
                    // has no sides.
                    (Route::Side, false) => {}
                    (_, false) => channels[0][i] = run(secs, 0, channels[0][i]),
                }
            }
            if sides {
                let (l, r) = (channels[0][i], channels[1][i]);
                let m = (l + r) * 0.5;
                let s = run(&mut self.mono_below, 0, (l - r) * 0.5) * self.width;
                channels[0][i] = m + s;
                channels[1][i] = m - s;
            }
        }
        if (self.gain - 1.0).abs() > 1e-7 {
            for ch in channels.iter_mut() {
                for v in ch.iter_mut() {
                    *v *= self.gain;
                }
            }
        }
    }
}

// The editor's watch: its FFT analyzer and peak meters.

/// Whose chain an effect is in, as the editor names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapOwner {
    Clip(bv_timeline::ClipId),
    Track(bv_timeline::TrackId),
    Bus(bv_timeline::BusId),
    Chain(bv_timeline::FxChainId),
    Master,
}

/// How many of the output's samples the analyzer looks at (its FFT).
const TAP_SAMPLES: usize = 4096;

/// How long a watch lasts without the editor asking again (seconds).
const TAP_LIFE: f32 = 1.0;

/// What the editor watches and what the mixer measured there.
#[derive(Debug)]
struct Tap {
    owner: TapOwner,
    index: usize,
    asked: Instant,
    rate: u32,
    /// The input's and the output's peaks since last taken (linear).
    peaks: [f32; 2],
    samples: VecDeque<f32>,
    heard: bool,
}

static TAP: Mutex<Option<Tap>> = Mutex::new(None);

/// The effect the editor shows — its chain's owner and its place in the
/// chain — measured by the mixer while it plays (`None`: none); asked
/// again each frame, or the watch lapses.
pub fn watch(at: Option<(TapOwner, usize)>) {
    let Ok(mut t) = TAP.lock() else {
        return;
    };
    match at {
        None => *t = None,
        Some((owner, index)) => match t.as_mut() {
            Some(x) if x.owner == owner && x.index == index => x.asked = Instant::now(),
            _ => {
                *t = Some(Tap {
                    owner,
                    index,
                    asked: Instant::now(),
                    rate: 48_000,
                    peaks: [0.0; 2],
                    samples: VecDeque::with_capacity(TAP_SAMPLES),
                    heard: false,
                });
            }
        },
    }
}

/// The place in `owner`'s chain the editor watches, if it does.
pub(crate) fn watched_in(owner: TapOwner) -> Option<usize> {
    let t = TAP.lock().ok()?;
    let t = t.as_ref()?;
    (t.owner == owner && t.asked.elapsed().as_secs_f32() < TAP_LIFE).then_some(t.index)
}

/// What the watched effect heard (`input`) and made (`output`), planar,
/// at `rate`.
pub(crate) fn note(rate: u32, input: &[Vec<f32>], output: &[Vec<f32>]) {
    let Ok(mut t) = TAP.lock() else {
        return;
    };
    let Some(t) = t.as_mut() else {
        return;
    };
    let peak = |ch: &[Vec<f32>]| {
        ch.iter()
            .flat_map(|c| c.iter())
            .fold(0f32, |m, v| m.max(v.abs()))
    };
    t.peaks[0] = t.peaks[0].max(peak(input));
    t.peaks[1] = t.peaks[1].max(peak(output));
    t.rate = rate;
    let n = output.first().map_or(0, Vec::len);
    let k = output.len().max(1) as f32;
    for i in 0..n {
        if t.samples.len() == TAP_SAMPLES {
            t.samples.pop_front();
        }
        t.samples.push_back(
            output
                .iter()
                .map(|c| c.get(i).copied().unwrap_or(0.0))
                .sum::<f32>()
                / k,
        );
    }
    t.heard = true;
}

/// What the watched effect did since last asked: its input's and
/// output's peaks (linear), and its output's spectrum at `freqs` (dB of a
/// full-scale sine).
#[derive(Clone, Debug, PartialEq)]
pub struct Analysis {
    pub peaks: [f32; 2],
    pub spectrum: Vec<f32>,
}

/// The analysis since last asked (`None`: nothing played through it).
pub fn take_analysis(freqs: &[f32]) -> Option<Analysis> {
    let (peaks, samples, rate) = {
        let mut g = TAP.lock().ok()?;
        let t = g.as_mut()?;
        if !t.heard {
            return None;
        }
        t.heard = false;
        let peaks = std::mem::take(&mut t.peaks);
        let samples: Vec<f32> = t.samples.iter().copied().collect();
        (peaks, samples, t.rate)
    };
    Some(Analysis {
        peaks,
        spectrum: spectrum_db(&samples, rate, freqs),
    })
}

/// `x`'s spectrum at `freqs` (dB of a full-scale sine): a Hann-windowed
/// FFT, each frequency the loudest bin within a twelfth of an octave.
pub fn spectrum_db(x: &[f32], rate: u32, freqs: &[f32]) -> Vec<f32> {
    let n = TAP_SAMPLES;
    if x.is_empty() {
        return vec![-120.0; freqs.len()];
    }
    let len = x.len().min(n);
    let w: Vec<f64> = x[x.len() - len..]
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let h = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (len.max(2) - 1) as f64).cos();
            f64::from(v) * h
        })
        .collect();
    let s = crate::fft::spectrum(&w, n);
    // A full-scale sine's bin: half the samples times the window's mean.
    let full = len as f64 * 0.25;
    let bin = |f: f32| f64::from(f) * n as f64 / f64::from(rate.max(1));
    freqs
        .iter()
        .map(|&f| {
            let (a, b) = (bin(f * 0.971), bin(f * 1.03));
            let lo = (a.floor() as usize).clamp(1, n / 2 - 1);
            let hi = (b.ceil() as usize).clamp(lo, n / 2 - 1);
            let m = (lo..=hi)
                .map(|k| (s[k].re * s[k].re + s[k].im * s[k].im).sqrt())
                .fold(0.0, f64::max);
            (20.0 * (m / full).max(1e-9).log10()) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::EffectKind;

    const RATE: u32 = 48_000;

    fn sine(f: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.25 * (std::f32::consts::TAU * f * i as f32 / RATE as f32).sin())
            .collect()
    }

    fn level_db(x: &[f32]) -> f32 {
        let tail = &x[x.len() / 2..];
        let rms = (tail.iter().map(|v| v * v).sum::<f32>() / tail.len() as f32).sqrt();
        20.0 * rms.max(1e-9).log10()
    }

    fn eq(set: impl FnOnce(&mut Settings)) -> Effect {
        let mut e = Effect::new(EffectKind::ModernEq);
        let mut s = Settings::of(&e);
        set(&mut s);
        s.write(&mut e);
        e
    }

    fn band(kind: BandType, freq: f32, gain_db: f32) -> Band {
        Band {
            used: true,
            on: true,
            kind,
            freq,
            gain_db,
            q: 1.0,
            slope: 12,
            route: Route::Stereo,
        }
    }

    #[test]
    fn oversampled_it_sounds_as_its_curves_say() {
        // A peak at 18 kHz: oversampled, its curve near the top keeps its
        // shape, and the sound follows it (a tone at the peak boosted by
        // about its gain), Off as before.
        for over in [1u8, 4] {
            let e = eq(|s| {
                s.bands[0] = band(BandType::Peaking, 18_000.0, 6.0);
                s.oversampling = over;
            });
            assert_eq!(Settings::of(&e).oversampling, over);
            let [l, _] = gain_at(&e, 18_000.0, [1.0, 1.0]);
            assert!((l - 6.0).abs() < 0.6, "{over}: {l}");
            // Lower down, nothing.
            let [l, _] = gain_at(&e, 1_000.0, [1.0, 1.0]);
            assert!(l.abs() < 0.2, "{over}: {l}");
        }
    }

    #[test]
    fn the_analyzer_finds_a_tone_and_its_peaks() {
        let tone = sine(1_000.0, 8192);
        let freqs = [500.0, 1_000.0, 4_000.0];
        let db = spectrum_db(&tone, RATE, &freqs);
        // A 0.25 sine: about −12 dB at 1 kHz, far less elsewhere.
        assert!((db[1] + 12.0).abs() < 1.5, "{db:?}");
        assert!(db[0] < db[1] - 30.0 && db[2] < db[1] - 30.0, "{db:?}");
        // The watch: what the mixer notes comes back once.
        // (A chain no other test has.)
        let owner = TapOwner::Chain(bv_timeline::FxChainId(987_654));
        watch(Some((owner, 2)));
        assert_eq!(watched_in(owner), Some(2));
        assert_eq!(watched_in(TapOwner::Master), None);
        let input = vec![tone.clone(), tone.clone()];
        let output = vec![tone.iter().map(|v| v * 2.0).collect::<Vec<_>>(); 2];
        note(RATE, &input, &output);
        let a = take_analysis(&freqs).expect("heard");
        assert!((a.peaks[0] - 0.25).abs() < 1e-3 && (a.peaks[1] - 0.5).abs() < 1e-3);
        assert!(take_analysis(&freqs).is_none());
        watch(None);
        assert_eq!(watched_in(owner), None);
    }

    /// The change (dB) `e` makes on a tone of `f` on each side, the tone
    /// on the left and the right times `sides`.
    fn gain_at(e: &Effect, f: f32, sides: [f32; 2]) -> [f32; 2] {
        let x = sine(f, RATE as usize / 2);
        let mut ch = vec![
            x.iter().map(|v| v * sides[0]).collect::<Vec<_>>(),
            x.iter().map(|v| v * sides[1]).collect(),
        ];
        let before = [level_db(&ch[0]), level_db(&ch[1])];
        let mut q = Eq::new(e, RATE, 2);
        q.process(&mut ch);
        [level_db(&ch[0]) - before[0], level_db(&ch[1]) - before[1]]
    }

    #[test]
    fn settings_keep_in_the_parameters() {
        let e = eq(|s| {
            s.bands[2] = Band {
                route: Route::Side,
                slope: 24,
                ..band(BandType::HighPass, 120.0, 0.0)
            };
            s.width = 1.5;
        });
        let s = Settings::of(&e);
        assert_eq!(s.bands[2].kind, BandType::HighPass);
        assert_eq!(s.bands[2].route, Route::Side);
        assert_eq!(s.bands[2].slope, 24);
        assert!(!s.bands[0].used && s.bands[0].on);
        assert!((s.width - 1.5).abs() < 1e-6);
        // A new one changes nothing.
        let flat = Settings::of(&Effect::new(EffectKind::ModernEq));
        assert!(flat.bands.iter().all(|b| !b.active()));
        assert_eq!(total_db(&flat, RATE as f32, 1000.0), 0.0);
    }

    #[test]
    fn butterworth_poles() {
        let (odd, qs) = butterworth(3);
        assert!(odd && (qs[0] - 1.0).abs() < 1e-5, "{qs:?}");
        let (odd, qs) = butterworth(4);
        assert!(!odd && (qs[0] - 0.5412).abs() < 1e-3 && (qs[1] - 1.3066).abs() < 1e-3);
        let (_, qs) = butterworth(6);
        assert!((qs[2] - 1.9319).abs() < 1e-3, "{qs:?}");
    }

    #[test]
    fn a_peak_boosts_its_frequency_and_the_passes_cut_by_their_slopes() {
        let e = eq(|s| s.bands[0] = band(BandType::Peaking, 1000.0, 6.0));
        let g = gain_at(&e, 1000.0, [1.0, 1.0]);
        assert!((g[0] - 6.0).abs() < 0.2, "{g:?}");
        assert!(gain_at(&e, 100.0, [1.0, 1.0])[0].abs() < 0.3);
        // A bypassed band does nothing.
        let off = eq(|s| {
            s.bands[0] = Band {
                on: false,
                ..band(BandType::Peaking, 1000.0, 6.0)
            }
        });
        assert!(gain_at(&off, 1000.0, [1.0, 1.0])[0].abs() < 0.01);
        // A high pass at 1 kHz: an octave below, its slope down.
        for slope in [6u8, 12, 18, 24, 36] {
            let e = eq(|s| {
                s.bands[0] = Band {
                    slope,
                    ..band(BandType::HighPass, 1000.0, 0.0)
                }
            });
            let down = gain_at(&e, 500.0, [1.0, 1.0])[0];
            assert!(
                (down + f32::from(slope)).abs() < 2.5,
                "{slope} dB/oct: {down}"
            );
            // At its frequency 3 dB down; well above it, nothing.
            let at = gain_at(&e, 1000.0, [1.0, 1.0])[0];
            assert!((at + 3.0).abs() < 0.5, "{slope}: {at}");
            assert!(gain_at(&e, 8000.0, [1.0, 1.0])[0].abs() < 0.3);
        }
        // A band pass: nothing taken at its frequency, its sides steeper
        // with its slope, its width kept.
        let bp = |slope| {
            eq(|s| {
                s.bands[0] = Band {
                    slope,
                    q: 2.0,
                    ..band(BandType::BandPass, 1000.0, 0.0)
                }
            })
        };
        for slope in [6u8, 24] {
            let s = Settings::of(&bp(slope));
            let at = total_db(&s, RATE as f32, 1000.0);
            assert!(at.abs() < 0.05, "{slope}: {at}");
            // Half power about a third of an octave away (Q 2).
            let edge = total_db(&s, RATE as f32, 1000.0 * 2f32.powf(0.36));
            assert!((edge + 3.0).abs() < 0.6, "{slope}: {edge}");
        }
        let far = |slope| total_db(&Settings::of(&bp(slope)), RATE as f32, 125.0);
        assert!(far(24) < far(6) - 30.0, "{} {}", far(6), far(24));
        // The curve the interface draws agrees with what is heard.
        let s = Settings::of(&eq(|s| s.bands[0] = band(BandType::Peaking, 1000.0, 6.0)));
        assert!((total_db(&s, RATE as f32, 1000.0) - 6.0).abs() < 0.05);
    }

    #[test]
    fn routing_mid_side_width_and_scale() {
        // A cut of the sides leaves a centered tone alone; a tone on the
        // left only is half sides and changes.
        let e = eq(|s| {
            s.bands[0] = Band {
                route: Route::Side,
                ..band(BandType::Peaking, 1000.0, -24.0)
            }
        });
        let centered = gain_at(&e, 1000.0, [1.0, 1.0]);
        assert!(
            centered[0].abs() < 0.05 && centered[1].abs() < 0.05,
            "{centered:?}"
        );
        assert!(gain_at(&e, 1000.0, [1.0, 0.0])[0] < -3.0);
        // A left band leaves the right channel alone.
        let e = eq(|s| {
            s.bands[0] = Band {
                route: Route::Left,
                ..band(BandType::Peaking, 1000.0, 6.0)
            }
        });
        let g = gain_at(&e, 1000.0, [1.0, 1.0]);
        assert!((g[0] - 6.0).abs() < 0.2 && g[1].abs() < 0.05, "{g:?}");
        // Width 0: mono; a tone on the left alone comes out on both, half.
        let e = eq(|s| s.width = 0.0);
        let g = gain_at(&e, 1000.0, [1.0, 0.0]);
        assert!((g[0] + 6.02).abs() < 0.1, "{g:?}");
        // Global Scale −100 %: the boost becomes a cut; 200 % doubles it.
        let scaled = |scale| {
            eq(|s| {
                s.bands[0] = band(BandType::Peaking, 1000.0, 6.0);
                s.scale = scale;
            })
        };
        assert!((gain_at(&scaled(-1.0), 1000.0, [1.0, 1.0])[0] + 6.0).abs() < 0.2);
        assert!((gain_at(&scaled(2.0), 1000.0, [1.0, 1.0])[0] - 12.0).abs() < 0.3);
        // Mono Below: low sides go, the middle stays.
        let e = eq(|s| s.mono_below = 200.0);
        assert!(gain_at(&e, 60.0, [1.0, -1.0])[0] < -30.0);
        assert!(gain_at(&e, 60.0, [1.0, 1.0])[0].abs() < 0.05);
        assert!(gain_at(&e, 2000.0, [1.0, -1.0])[0].abs() < 0.1);
        let s = Settings::of(&e);
        let curve = mono_below_curve(&s, RATE as f32, &[60.0, 2000.0]).unwrap();
        assert!(curve[0] < -30.0 && curve[1].abs() < 0.1, "{curve:?}");
        // Output gain.
        let e = eq(|s| s.output_db = -6.0);
        assert!((gain_at(&e, 1000.0, [1.0, 1.0])[0] + 6.0).abs() < 0.01);
    }

    #[test]
    fn one_channel_and_surround() {
        // Mono: a side band does nothing, the others apply.
        let mut ch = vec![sine(1000.0, 24_000)];
        let before = level_db(&ch[0]);
        let e = eq(|s| {
            s.bands[0] = Band {
                route: Route::Side,
                ..band(BandType::Peaking, 1000.0, -12.0)
            };
            s.bands[1] = Band {
                route: Route::Mid,
                ..band(BandType::Peaking, 1000.0, 6.0)
            };
        });
        Eq::new(&e, RATE, 1).process(&mut ch);
        assert!((level_db(&ch[0]) - before - 6.0).abs() < 0.2);
        // 5.1: a stereo band on every channel, each with its own state.
        let x = sine(1000.0, 24_000);
        let mut six = vec![x.clone(); 6];
        let e = eq(|s| s.bands[0] = band(BandType::Peaking, 1000.0, 6.0));
        Eq::new(&e, RATE, 6).process(&mut six);
        for c in &six {
            assert!((level_db(c) - level_db(&x) - 6.0).abs() < 0.2);
        }
    }
}
