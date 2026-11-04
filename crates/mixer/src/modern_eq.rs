//! The Modern Equalizer, as VEGAS's: up to eight bands — a high or low
//! pass of 6, 12, 24 or 36 dB an octave, a low or high shelf of 6 or 12, a
//! peak, a band pass of 6, 12 or 18 or a band stop —, each on both
//! channels or on the left, the right, the middle or the sides alone; then
//! Mono Below (the sides high-passed, for a mono low end), Stereo Width
//! (the sides scaled) and Output Gain. Global Scale scales every band's
//! gain (below 0 a boost becomes a cut).
//!
//! A band is minimum phase — the cookbook's second-order sections, the
//! steeper slopes two or three alike in a row (all with the band's Q), the
//! 6 dB passes and shelves first order — or linear phase: the curves of a
//! routing's linear-phase bands multiplied into one response with no
//! phase, cut to an FIR filter as long as Linear Phase Quality says and
//! applied through FFTs ([`Linear`]). Frequency, gain and Q glide to new
//! values in 50 ms, the sections redesigned sample by sample meanwhile; a
//! band fades in or out as long when turned on or off (or reshaped: out
//! as it was, in as it is), and a new linear-phase filter crossfades from
//! the old. Oversampled, the bands run at 2, 4 or 8 times the rate between
//! half-band FIR filters ([`Oversampler`]). [`band_curve`] and
//! [`total_curve`] give the curves the interface draws.

use std::collections::VecDeque;
use std::f64::consts::{PI, TAU};
use std::sync::Mutex;
use std::time::Instant;

use bv_timeline::Effect;
/// How many bands it has; its global parameters (Global Scale, Mono
/// Below, Stereo Width, Output Gain) and each band's (in use, on, type,
/// frequency, gain, Q, slope, routing), as the model lays them out; after
/// them Oversampling, each band's phase and Linear Phase Quality.
pub use bv_timeline::fx::modern_eq::{BANDS, GLOBALS, OVERSAMPLING, PER_BAND, QUALITY, model};

use crate::fft::{Complex, Plan};
use crate::fx::Own;

/// Mono Below is off at its slider's start.
pub const MONO_BELOW_OFF: f32 = 20.0;

/// How long values glide to new ones and a band fades in or out (s).
const GLIDE: f64 = 0.05;

/// Added and taken away again: what is smaller than its last digit goes,
/// before it slows the filters' memory down.
const FLUSH: f64 = 1e-30;

/// A band's shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandType {
    HighPass,
    LowPass,
    LowShelf,
    HighShelf,
    BandPass,
    Peaking,
    BandStop,
}

impl BandType {
    /// In the order of the Filter Type buttons.
    pub const ALL: [BandType; 7] = [
        BandType::HighPass,
        BandType::LowPass,
        BandType::LowShelf,
        BandType::HighShelf,
        BandType::BandPass,
        BandType::Peaking,
        BandType::BandStop,
    ];

    fn of(v: f32) -> BandType {
        BandType::ALL[(v.round().max(0.0) as usize).min(BandType::ALL.len() - 1)]
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
            BandType::BandStop => "Band Stop",
        }
    }

    /// The slopes it comes in (dB an octave).
    pub fn slopes(self) -> &'static [u8] {
        match self {
            BandType::HighPass | BandType::LowPass => &[6, 12, 24, 36],
            BandType::LowShelf | BandType::HighShelf => &[6, 12],
            BandType::BandPass => &[6, 12, 18],
            BandType::Peaking => &[12],
            BandType::BandStop => &[6],
        }
    }

    /// Whether it has a gain (the passes and the stop have none).
    pub fn has_gain(self) -> bool {
        matches!(
            self,
            BandType::LowShelf | BandType::HighShelf | BandType::Peaking
        )
    }

    /// Whether its slope can be picked.
    pub fn has_slope(self) -> bool {
        self.slopes().len() > 1
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
    /// dB an octave as last picked (a multiple of 6): its type keeps the
    /// nearest it comes in ([`Band::steepness`]).
    pub slope: u8,
    pub route: Route,
    /// Linear phase (an FIR filter shared with the other linear-phase
    /// bands), or minimum phase (its own sections).
    pub linear: bool,
}

impl Band {
    /// Whether it changes the sound.
    pub fn active(&self) -> bool {
        self.used && self.on
    }

    /// Its slope (dB an octave): the one picked if its type comes in it,
    /// else the nearest that it does, the steeper on a tie.
    pub fn steepness(&self) -> u8 {
        let picked = i32::from(self.slope);
        self.kind
            .slopes()
            .iter()
            .copied()
            .min_by_key(|&s| ((i32::from(s) - picked).abs(), -i32::from(s)))
            .unwrap_or(12)
    }

    /// The type and slope it filters with: linear phase has no 6 dB
    /// shelves (they are 12 dB ones there).
    fn shape(&self) -> (BandType, u8) {
        let slope = self.steepness();
        match self.kind {
            BandType::LowShelf | BandType::HighShelf if self.linear => (self.kind, 12),
            kind => (kind, slope),
        }
    }

    /// Whether its Q shapes it (the first-order passes and shelves have
    /// none).
    pub fn has_q(&self) -> bool {
        let (kind, slope) = self.shape();
        !(slope == 6
            && matches!(
                kind,
                BandType::HighPass | BandType::LowPass | BandType::LowShelf | BandType::HighShelf
            ))
    }
}

/// The equalizer's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Global Scale: every band's gain times this (−2 to 2).
    pub scale: f32,
    /// Mono Below: the sides high-passed here (Hz, 20 to 200; off at
    /// [`MONO_BELOW_OFF`]).
    pub mono_below: f32,
    /// Stereo Width: the sides times this (0 to 1).
    pub width: f32,
    pub output_db: f32,
    pub bands: [Band; BANDS],
    /// Oversampling: the bands run at this many times the rate (1, 2, 4
    /// or 8), so their curves keep their shape near the top.
    pub oversampling: u8,
    /// Linear Phase Quality (0 to 4): how long the linear-phase filter is
    /// ([`Linear::taps`]).
    pub quality: u8,
}

/// Where band `b`'s parameters start.
pub fn band_at(b: usize) -> usize {
    bv_timeline::fx::modern_eq::at(b, 0)
}

impl Settings {
    pub fn of(e: &Effect) -> Settings {
        Settings::read(|i| e.param(i))
    }

    /// The settings that parameter values `p` (by index) make.
    fn read(p: impl Fn(usize) -> f32) -> Settings {
        Settings {
            scale: (p(0) / 100.0).clamp(-2.0, 2.0),
            mono_below: p(1).clamp(MONO_BELOW_OFF, 200.0),
            width: (p(2) / 100.0).clamp(0.0, 1.0),
            output_db: p(3).clamp(-24.0, 24.0),
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
                    linear: p(model(b)) >= 0.5,
                }
            }),
            oversampling: 1 << (p(OVERSAMPLING).round().clamp(0.0, 3.0) as u8),
            quality: p(QUALITY).round().clamp(0.0, 4.0) as u8,
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
            e.set_param(model(b), f32::from(u8::from(band.linear)));
        }
        e.set_param(
            OVERSAMPLING,
            f32::from(self.oversampling.max(1).trailing_zeros() as u8),
        );
        e.set_param(QUALITY, f32::from(self.quality.min(4)));
    }

    /// Whether Mono Below filters the sides.
    pub fn mono_below_on(&self) -> bool {
        self.mono_below > MONO_BELOW_OFF + 0.01
    }
}

/// A second-order section's coefficients (`a0` being 1).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Coeffs {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Coeffs {
    /// Its power gain at the angle whose cosine is `c1` (`c2` twice its).
    fn power(&self, c1: f64, c2: f64) -> f64 {
        let num = self.b0 * self.b0
            + self.b1 * self.b1
            + self.b2 * self.b2
            + 2.0 * (self.b0 * self.b1 + self.b1 * self.b2) * c1
            + 2.0 * self.b0 * self.b2 * c2;
        let den = 1.0
            + self.a1 * self.a1
            + self.a2 * self.a2
            + 2.0 * (self.a1 + self.a1 * self.a2) * c1
            + 2.0 * self.a2 * c2;
        num / den.max(1e-300)
    }
}

/// A band's filter of `kind` and `slope` at `fs` — `freq`, `gain_db` (the
/// scale applied) and `q` —: a section and how many alike in a row.
fn design(kind: BandType, slope: u8, fs: f64, freq: f64, gain_db: f64, q: f64) -> (Coeffs, usize) {
    let f = freq.clamp(1.0, 0.5 * fs);
    let q = q.max(0.01);
    let k = (PI * f / fs).tan();
    let a = 10f64.powf(gain_db / 40.0);
    // The cookbook's angle (2 Hz at least) and its bandwidth term.
    let (sw, cw) = (TAU * f.max(2.0) / fs).sin_cos();
    let alpha = sw / (2.0 * q);
    // The resonant passes' and the stop's denominator.
    let d = k * k * q + k + q;
    let (a1, a2) = (2.0 * q * (k * k - 1.0) / d, (k * k * q - k + q) / d);
    let steps = |per: u8| usize::from(slope / per).clamp(1, 3);
    match kind {
        BandType::LowPass | BandType::HighPass if slope <= 6 => {
            let n = 1.0 / (k + 1.0);
            let (b0, b1) = if kind == BandType::LowPass {
                (k * n, k * n)
            } else {
                (n, -n)
            };
            let c = Coeffs {
                b0,
                b1,
                b2: 0.0,
                a1: (k - 1.0) * n,
                a2: 0.0,
            };
            (c, 1)
        }
        BandType::LowPass => {
            let b0 = k * k * q / d;
            (
                Coeffs {
                    b0,
                    b1: 2.0 * b0,
                    b2: b0,
                    a1,
                    a2,
                },
                steps(12),
            )
        }
        BandType::HighPass => {
            let b0 = q / d;
            (
                Coeffs {
                    b0,
                    b1: -2.0 * b0,
                    b2: b0,
                    a1,
                    a2,
                },
                steps(12),
            )
        }
        BandType::BandStop => {
            let b0 = (k * k + 1.0) * q / d;
            (
                Coeffs {
                    b0,
                    b1: a1,
                    b2: b0,
                    a1,
                    a2,
                },
                1,
            )
        }
        // Its gain at the middle is Q (Q² and Q³ two and three alike).
        BandType::BandPass => {
            let n = 1.0 / (1.0 + alpha);
            let b0 = 0.5 * sw * n;
            (
                Coeffs {
                    b0,
                    b1: 0.0,
                    b2: -b0,
                    a1: -2.0 * cw * n,
                    a2: (1.0 - alpha) * n,
                },
                steps(6),
            )
        }
        BandType::Peaking => {
            let n = 1.0 / (1.0 + alpha / a);
            (
                Coeffs {
                    b0: (1.0 + alpha * a) * n,
                    b1: -2.0 * cw * n,
                    b2: (1.0 - alpha * a) * n,
                    a1: -2.0 * cw * n,
                    a2: (1.0 - alpha / a) * n,
                },
                1,
            )
        }
        // First order: the whole gain below (or above) the corner, the
        // corner moved by the gain's square root so that the middle of the
        // change (in dB) stays at the band's frequency; the tangent's
        // series in place of the tangent.
        BandType::LowShelf | BandType::HighShelf if slope <= 6 => {
            let low = kind == BandType::LowShelf;
            let corner = if low { f / a } else { f * a }.clamp(fs / 24_576.0, fs / 2.125);
            let w = PI * corner / fs;
            let t = w + w.powi(3) / 3.0 + 2.0 * w.powi(5) / 15.0;
            let a1 = (t - 1.0) / (t + 1.0);
            let (b0, b1) = if low {
                let m = (a * a - 1.0) * t / (t + 1.0);
                (1.0 + m, a1 + m)
            } else {
                let m = (a * a - 1.0) / (t + 1.0);
                (1.0 + m, a1 - m)
            };
            (
                Coeffs {
                    b0,
                    b1,
                    b2: 0.0,
                    a1,
                    a2: 0.0,
                },
                1,
            )
        }
        BandType::LowShelf | BandType::HighShelf => {
            let beta = sw * a.sqrt() / q;
            let (p, m) = (a + 1.0, a - 1.0);
            let c = if kind == BandType::LowShelf {
                let n = 1.0 / (p + m * cw + beta);
                Coeffs {
                    b0: a * (p - m * cw + beta) * n,
                    b1: 2.0 * a * (m - p * cw) * n,
                    b2: a * (p - m * cw - beta) * n,
                    a1: -2.0 * (m + p * cw) * n,
                    a2: (p + m * cw - beta) * n,
                }
            } else {
                let n = 1.0 / (p - m * cw + beta);
                Coeffs {
                    b0: a * (p + m * cw + beta) * n,
                    b1: -2.0 * a * (m + p * cw) * n,
                    b2: a * (p + m * cw - beta) * n,
                    a1: 2.0 * (m - p * cw) * n,
                    a2: (p - m * cw - beta) * n,
                }
            };
            (c, 1)
        }
    }
}

/// Band `b`'s filter at `fs` with Global Scale's `scale`.
fn band_filter(b: &Band, scale: f32, fs: f64) -> (Coeffs, usize) {
    let (kind, slope) = b.shape();
    design(
        kind,
        slope,
        fs,
        f64::from(b.freq),
        f64::from(b.gain_db * scale),
        f64::from(b.q),
    )
}

/// The power gain at each of `freqs` (Hz) of `count` sections `c` in a
/// row, at `fs`.
fn powers(c: &Coeffs, count: usize, fs: f64, freqs: &[f32]) -> Vec<f64> {
    freqs
        .iter()
        .map(|&f| {
            let w = TAU * f64::from(f) / fs;
            c.power(w.cos(), (2.0 * w).cos()).powi(count as i32)
        })
        .collect()
}

fn to_db(power: f64) -> f32 {
    (10.0 * power.max(1e-24).log10()) as f32
}

/// Band `b`'s own curve: its gain (dB) at each of `freqs`, Global Scale's
/// `scale` applied; 0 when it does nothing.
pub fn band_curve(b: &Band, scale: f32, rate: f32, freqs: &[f32]) -> Vec<f32> {
    if !b.active() {
        return vec![0.0; freqs.len()];
    }
    let fs = f64::from(rate);
    let (c, count) = band_filter(b, scale, fs);
    powers(&c, count, fs, freqs)
        .into_iter()
        .map(to_db)
        .collect()
}

/// [`band_curve`] at one frequency.
pub fn band_db(b: &Band, scale: f32, rate: f32, f: f32) -> f32 {
    band_curve(b, scale, rate, &[f])[0]
}

/// Mono Below's filter on the sides: a second-order Butterworth high pass.
fn mono_below_filter(fs: f64, freq: f64) -> Coeffs {
    design(
        BandType::HighPass,
        12,
        fs,
        freq,
        0.0,
        std::f64::consts::FRAC_1_SQRT_2,
    )
    .0
}

/// Mono Below's curve on the sides (dB at each of `freqs`); `None` when it
/// is off.
pub fn mono_below_curve(s: &Settings, rate: f32, freqs: &[f32]) -> Option<Vec<f32>> {
    s.mono_below_on().then(|| {
        let fs = f64::from(rate);
        let c = mono_below_filter(fs, f64::from(s.mono_below));
        powers(&c, 1, fs, freqs).into_iter().map(to_db).collect()
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

/// A value gliding in a straight line to the last one it was sent.
#[derive(Clone, Copy, Debug)]
struct Glide {
    now: f64,
    target: f64,
    step: f64,
    left: u32,
}

impl Glide {
    fn at(v: f64) -> Glide {
        Glide {
            now: v,
            target: v,
            step: 0.0,
            left: 0,
        }
    }

    /// Toward `v` in `steps` samples, the whole way again from where it is.
    fn to(&mut self, v: f64, steps: u32) {
        if v == self.target {
            return;
        }
        self.target = v;
        if steps == 0 {
            self.now = v;
            self.left = 0;
        } else {
            self.left = steps;
            self.step = (v - self.now) / f64::from(steps);
        }
    }

    fn moving(&self) -> bool {
        self.left > 0
    }

    /// The next sample's value.
    fn next(&mut self) -> f64 {
        if self.left > 0 {
            self.left -= 1;
            self.now = if self.left == 0 {
                self.target
            } else {
                self.now + self.step
            };
        }
        self.now
    }
}

/// One sample through a section (`m` its memory), the second direct form.
#[inline]
fn tick(c: &Coeffs, m: &mut [f64; 2], x: f64) -> f64 {
    let w = x - c.a1 * m[0] - c.a2 * m[1] + FLUSH - FLUSH;
    let y = c.b0 * w + c.b1 * m[0] + c.b2 * m[1];
    m[1] = m[0];
    m[0] = w;
    y
}

/// What a minimum-phase band filters with while it lasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shape {
    kind: BandType,
    slope: u8,
    route: Route,
}

impl Shape {
    fn of(b: &Band) -> Shape {
        let (kind, slope) = b.shape();
        Shape {
            kind,
            slope,
            route: b.route,
        }
    }
}

/// A minimum-phase band at work.
#[derive(Debug)]
struct Run {
    shape: Shape,
    /// Its next shape, taken once it has faded out.
    next: Option<Shape>,
    /// Whether it is to be heard (on, in use, minimum phase).
    heard: bool,
    freq: Glide,
    gain: Glide,
    q: Glide,
    /// How much of it is heard (0 to 1).
    wet: Glide,
    coeffs: Coeffs,
    count: usize,
    /// Each slot's sections' memory.
    memory: Vec<[[f64; 2]; 3]>,
}

impl Run {
    fn new(b: &Band, scale: f32, fs: f64) -> Run {
        let heard = b.active() && !b.linear;
        let mut run = Run {
            shape: Shape::of(b),
            next: None,
            heard,
            freq: Glide::at(f64::from(b.freq)),
            gain: Glide::at(f64::from(b.gain_db * scale)),
            q: Glide::at(f64::from(b.q)),
            wet: Glide::at(if heard { 1.0 } else { 0.0 }),
            coeffs: band_filter(b, scale, fs).0,
            count: 1,
            memory: Vec::new(),
        };
        run.redesign(fs);
        run
    }

    fn redesign(&mut self, fs: f64) {
        (self.coeffs, self.count) = design(
            self.shape.kind,
            self.shape.slope,
            fs,
            self.freq.now,
            self.gain.now,
            self.q.now,
        );
    }

    /// Band `b`'s new values: frequency, gain and Q glide there in `steps`
    /// samples; a new shape waits for the band to fade out.
    fn follow(&mut self, b: &Band, scale: f32, fs: f64, steps: u32) {
        self.heard = b.active() && !b.linear;
        let shape = Shape::of(b);
        if shape != self.shape {
            if self.silent() {
                self.shape = shape;
                self.next = None;
                self.redesign(fs);
            } else {
                self.next = Some(shape);
            }
        } else {
            self.next = None;
        }
        self.freq.to(f64::from(b.freq), steps);
        self.gain.to(f64::from(b.gain_db * scale), steps);
        self.q.to(f64::from(b.q), steps);
        if !self.freq.moving() && !self.gain.moving() && !self.q.moving() {
            self.redesign(fs);
        }
        let wet = if self.heard && self.next.is_none() {
            1.0
        } else {
            0.0
        };
        self.wet.to(wet, steps);
    }

    /// Faded out, with nothing waiting: it changes nothing.
    fn silent(&self) -> bool {
        self.wet.now == 0.0 && !self.wet.moving()
    }

    /// `slots` (as long each) through the band.
    fn process(&mut self, slots: &mut [&mut [f64]], fs: f64, steps: u32) {
        if self.silent() && self.next.is_none() {
            return;
        }
        if self.memory.len() < slots.len() {
            self.memory.resize(slots.len(), [[0.0; 2]; 3]);
        }
        let n = slots.first().map_or(0, |s| s.len());
        let gliding = self.freq.moving() || self.gain.moving() || self.q.moving();
        if !gliding && !self.wet.moving() && self.wet.now == 1.0 {
            // At rest: the sections, a slot at a time.
            let count = self.count;
            for (slot, memory) in slots.iter_mut().zip(&mut self.memory) {
                for m in memory.iter_mut().take(count) {
                    for v in slot.iter_mut() {
                        *v = tick(&self.coeffs, m, *v);
                    }
                }
            }
            return;
        }
        for i in 0..n {
            if self.freq.moving() || self.gain.moving() || self.q.moving() {
                self.freq.next();
                self.gain.next();
                self.q.next();
                self.redesign(fs);
            }
            let wet = self.wet.next();
            for (slot, memory) in slots.iter_mut().zip(&mut self.memory) {
                let x = slot[i];
                let mut y = x;
                for m in memory.iter_mut().take(self.count) {
                    y = tick(&self.coeffs, m, y);
                }
                slot[i] = x + wet * (y - x);
            }
            if wet == 0.0 && !self.wet.moving() {
                // Faded out: its memory goes, a waiting shape comes in.
                self.memory.fill([[0.0; 2]; 3]);
                if let Some(shape) = self.next.take() {
                    self.shape = shape;
                    self.redesign(fs);
                    if self.heard {
                        self.wet.to(1.0, steps);
                    }
                } else {
                    return;
                }
            }
        }
    }
}

/// A half-band low pass of `4n + 3` taps: those an odd number of samples
/// from its middle (`h[2i − m]` for `i` in `0..=m`), the middle one a half
/// and the others nothing.
#[derive(Debug)]
struct HalfBand {
    taps: Vec<f64>,
    m: usize,
}

fn bessel_i0(x: f64) -> f64 {
    let half = x / 2.0;
    let (mut sum, mut term) = (1.0, 1.0);
    for k in 1..300 {
        term *= (half / k as f64).powi(2);
        sum += term;
        if term < 1e-17 * sum {
            break;
        }
    }
    sum
}

impl HalfBand {
    /// Through Kaiser's window: from `0.5 − tw` of the higher rate's half
    /// to `0.5 + tw` it falls `atten` dB.
    fn new(tw: f64, atten: f64) -> HalfBand {
        let beta = if atten > 50.0 {
            0.1102 * (atten - 8.7)
        } else if atten >= 21.0 {
            0.5842 * (atten - 21.0).powf(0.4) + 0.07886 * (atten - 21.0)
        } else {
            0.0
        };
        let estimate = ((atten - 7.95) / (2.285 * 2.0 * tw * PI)).ceil() as usize + 1;
        let n = estimate.saturating_sub(3).div_ceil(4);
        let m = 2 * n + 1;
        let i0 = bessel_i0(beta);
        let mut taps: Vec<f64> = (0..=m)
            .map(|i| {
                let k = 2.0 * i as f64 - m as f64;
                let r = k / m as f64;
                let window = bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0;
                (PI * k / 2.0).sin() / (PI * k) * window
            })
            .collect();
        // They add up to a half: nothing changes at 0 Hz.
        let sum: f64 = taps.iter().sum();
        for t in &mut taps {
            *t *= 0.5 / sum;
        }
        HalfBand { taps, m }
    }

    /// The taps' sum over the samples `at − 2·i·stride` (`i` in `0..=m`),
    /// paired from both ends.
    #[inline]
    fn dot(&self, v: &[f64], at: usize, stride: usize) -> f64 {
        let m = self.m;
        (0..m.div_ceil(2))
            .map(|i| self.taps[i] * (v[at - i * stride] + v[at - (m - i) * stride]))
            .sum()
    }

    /// `x` at twice the rate into `out`; `history` the last `m` samples
    /// before it.
    fn up(&self, x: &[f64], history: &mut Vec<f64>, out: &mut Vec<f64>) {
        let m = self.m;
        history.resize(m, 0.0);
        let mut v = std::mem::take(history);
        v.extend_from_slice(x);
        out.clear();
        for j in 0..x.len() {
            let at = m + j;
            out.push(2.0 * self.dot(&v, at, 1));
            out.push(v[at - (m - 1) / 2]);
        }
        history.extend_from_slice(&v[v.len() - m..]);
    }

    /// `x` (an even count) at half the rate into `out`; `history` the last
    /// `2m` samples before it.
    fn down(&self, x: &[f64], history: &mut Vec<f64>, out: &mut Vec<f64>) {
        let m = self.m;
        history.resize(2 * m, 0.0);
        let mut v = std::mem::take(history);
        v.extend_from_slice(x);
        out.clear();
        for j in 0..x.len() / 2 {
            let at = 2 * m + 2 * j;
            out.push(0.5 * v[at - m] + self.dot(&v, at, 2));
        }
        history.extend_from_slice(&v[v.len() - 2 * m..]);
    }
}

/// The bands' rate doubled stage by stage and halved back, each stage two
/// half-band filters: going up, a transition a tenth of the half wide and
/// 80, 70, 60 dB down (the first 5 % wide and 90 dB down); coming down,
/// 12 % and 65, 55, 45 dB (the first 6 % and 75 dB).
#[derive(Debug)]
struct Oversampler {
    stages: Vec<(HalfBand, HalfBand)>,
    /// Each stage's and channel's history, going up and coming down.
    up: Vec<Vec<Vec<f64>>>,
    down: Vec<Vec<Vec<f64>>>,
}

impl Oversampler {
    fn new(factor: usize) -> Oversampler {
        let stages: Vec<(HalfBand, HalfBand)> = (0..factor.max(1).trailing_zeros())
            .map(|i| {
                let first = i == 0;
                let i = f64::from(i);
                (
                    HalfBand::new(if first { 0.05 } else { 0.10 }, 90.0 - 10.0 * i),
                    HalfBand::new(if first { 0.06 } else { 0.12 }, 75.0 - 10.0 * i),
                )
            })
            .collect();
        let count = stages.len();
        Oversampler {
            stages,
            up: vec![Vec::new(); count],
            down: vec![Vec::new(); count],
        }
    }

    /// How late its output comes, in samples of the top rate.
    fn delay(&self) -> usize {
        let count = self.stages.len();
        self.stages
            .iter()
            .enumerate()
            .map(|(i, (up, down))| (up.m + down.m) << (count - 1 - i))
            .sum()
    }

    /// Channel `c`'s `x` at the top rate, into `out`.
    fn raise(&mut self, c: usize, x: &[f32], out: &mut Vec<f64>) {
        out.clear();
        out.extend(x.iter().map(|&v| f64::from(v)));
        let mut next = Vec::new();
        for (s, (up, _)) in self.stages.iter().enumerate() {
            if self.up[s].len() <= c {
                self.up[s].resize(c + 1, Vec::new());
            }
            up.up(out, &mut self.up[s][c], &mut next);
            std::mem::swap(out, &mut next);
        }
    }

    /// Channel `c`'s `x` from the top rate back down, into `out`.
    fn lower(&mut self, c: usize, x: &mut Vec<f64>, out: &mut [f32]) {
        let mut next = Vec::new();
        for (s, (_, down)) in self.stages.iter().enumerate().rev() {
            if self.down[s].len() <= c {
                self.down[s].resize(c + 1, Vec::new());
            }
            down.down(x, &mut self.down[s][c], &mut next);
            std::mem::swap(x, &mut next);
        }
        for (o, &v) in out.iter_mut().zip(x.iter()) {
            *o = v as f32;
        }
    }
}

/// A linear-phase band as its filter is made: its shape, routing and
/// values (its gain scaled).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Key {
    shape: Shape,
    freq: f64,
    gain: f64,
    q: f64,
}

/// One set of linear-phase filters' spectra: for the left and the right,
/// what each hears of itself and of the other (the middle's and sides'
/// bands make each heard in the other), and the other channels' filter;
/// with one channel, `ll` is its. With what the last block left for the
/// next, each channel.
#[derive(Clone, Debug)]
struct Filters {
    ll: Vec<Complex>,
    lr: Option<Vec<Complex>>,
    rl: Option<Vec<Complex>>,
    rr: Vec<Complex>,
    rest: Vec<Complex>,
    tails: Vec<Vec<f64>>,
}

/// The linear-phase bands at work: their curves multiplied (a routing's
/// alike into one), sampled at about twice the rate's count of points,
/// back in time centred and cut to [`Linear::taps`] taps, and applied
/// through FFTs half an FFT of sound at a time; a new filter, made when a
/// block ends and no fade is running, fades in from the old over its
/// quality's time.
#[derive(Debug)]
struct Linear {
    fs: f64,
    quality: u8,
    channels: usize,
    taps: usize,
    half: usize,
    plan: Plan,
    /// The transform the curves go back to time through.
    design: Plan,
    now: Filters,
    old: Option<Filters>,
    fade: usize,
    fade_len: usize,
    made: Vec<Key>,
    wanted: Vec<Key>,
    input: Vec<Vec<f64>>,
    output: Vec<Vec<f64>>,
    /// The last block's spectra, each channel: a new filter starts with
    /// what it would have left.
    last: Vec<Vec<Complex>>,
    at: usize,
}

/// The power of two nearest to `x`.
fn nearest_power_of_two(x: f64) -> usize {
    let below = 1usize << (x.max(1.0).log2().floor() as u32);
    if x - below as f64 <= (2 * below) as f64 - x {
        below
    } else {
        2 * below
    }
}

impl Linear {
    /// How long its filter is at `fs` for Linear Phase Quality `quality`:
    /// 1024, 2048, 4096 or 8192 taps at 44.1 kHz (an odd count), or the
    /// longest the curve's sampling allows.
    fn taps(fs: f64, quality: u8) -> usize {
        if quality >= 4 {
            return nearest_power_of_two(2.0 * fs) / 2 - 1;
        }
        let taps = (f64::from(1024u32 << quality) * fs / 44_100.0) as usize;
        (taps - usize::from(taps.is_multiple_of(2))).max(3)
    }

    /// How long a new filter takes to fade in (s), by quality.
    const FADES: [f64; 5] = [0.05, 0.075, 0.15, 0.25, 0.5];

    fn new(fs: f64, quality: u8, channels: usize, keys: Vec<Key>) -> Linear {
        let quality = quality.min(4);
        let taps = Linear::taps(fs, quality);
        let half = taps.next_power_of_two();
        let points = if quality >= 4 {
            nearest_power_of_two(2.0 * fs)
        } else {
            nearest_power_of_two(2.0 * fs).min((16 * taps).next_power_of_two().max(1024))
        };
        let channels = channels.max(1);
        let mut l = Linear {
            fs,
            quality,
            channels,
            taps,
            half,
            plan: Plan::new(2 * half),
            design: Plan::new(points),
            now: Filters {
                ll: Vec::new(),
                lr: None,
                rl: None,
                rr: Vec::new(),
                rest: Vec::new(),
                tails: vec![vec![0.0; half]; channels],
            },
            old: None,
            fade: 0,
            fade_len: (Linear::FADES[usize::from(quality)] * fs) as usize,
            made: Vec::new(),
            wanted: keys,
            input: vec![vec![0.0; half]; channels],
            output: vec![vec![0.0; half]; channels],
            last: vec![vec![Complex::ZERO; 2 * half]; channels],
            at: 0,
        };
        l.now = l.filters(&l.wanted);
        l.made = l.wanted.clone();
        l
    }

    /// How late its output comes (samples of its rate).
    fn delay(&self) -> usize {
        self.half + (self.taps - 1) / 2
    }

    /// The filter for `response` (a gain at each of the design's first
    /// half of points and the middle one), as a spectrum.
    fn kernel(&self, response: &[f64]) -> Vec<Complex> {
        let n = self.design.len();
        let mut w: Vec<Complex> = (0..n)
            .map(|k| Complex::new(response[k.min(n - k)], 0.0))
            .collect();
        self.design.run(&mut w, true);
        let middle = (self.taps - 1) / 2;
        let mut h = vec![Complex::ZERO; 2 * self.half];
        for (j, t) in h.iter_mut().take(self.taps.min(n)).enumerate() {
            t.re = w[(j + n - middle) % n].re;
        }
        self.plan.run(&mut h, false);
        h
    }

    /// The filters `keys` make.
    fn filters(&self, keys: &[Key]) -> Filters {
        let n = self.design.len();
        let bins = n / 2 + 1;
        let cos: Vec<(f64, f64)> = (0..bins)
            .map(|k| {
                let w = TAU * k as f64 / n as f64;
                (w.cos(), (2.0 * w).cos())
            })
            .collect();
        // Each routing's curve, in the order of `Route::ALL`.
        let mut curves = vec![vec![1.0f64; bins]; Route::ALL.len()];
        for key in keys {
            let (c, count) = design(
                key.shape.kind,
                key.shape.slope,
                self.fs,
                key.freq,
                key.gain,
                key.q,
            );
            let r = key.shape.route.index() as usize;
            for (g, &(c1, c2)) in curves[r].iter_mut().zip(&cos) {
                *g *= c.power(c1, c2).powi(count as i32).sqrt();
            }
        }
        let [stereo, left, right, mid, side] = [0, 1, 2, 3, 4].map(|r| &curves[r]);
        let of = |f: &dyn Fn(usize) -> f64| -> Vec<f64> { (0..bins).map(f).collect() };
        let tails = vec![vec![0.0; self.half]; self.channels];
        if self.channels == 1 {
            let ll = self.kernel(&of(&|k| stereo[k] * left[k] * right[k] * mid[k]));
            return Filters {
                rr: ll.clone(),
                rest: ll.clone(),
                ll,
                lr: None,
                rl: None,
                tails,
            };
        }
        // Left and right from the middle and the sides: each hears the
        // two's mean of itself and their half difference of the other.
        let same = |k: usize| 0.5 * (mid[k] + side[k]);
        let cross = |k: usize| 0.5 * (mid[k] - side[k]);
        let crossed = (0..bins).any(|k| cross(k).abs() > 1e-12);
        Filters {
            ll: self.kernel(&of(&|k| same(k) * left[k] * stereo[k])),
            rr: self.kernel(&of(&|k| same(k) * right[k] * stereo[k])),
            lr: crossed.then(|| self.kernel(&of(&|k| cross(k) * right[k] * stereo[k]))),
            rl: crossed.then(|| self.kernel(&of(&|k| cross(k) * left[k] * stereo[k]))),
            rest: if self.channels > 2 {
                self.kernel(stereo)
            } else {
                Vec::new()
            },
            tails,
        }
    }

    /// The block whose spectra (each channel's) are `x` through `f`: each
    /// channel's half of sound due out; `f`'s tails kept for the next.
    fn apply(plan: &Plan, half: usize, f: &mut Filters, x: &[Vec<Complex>]) -> Vec<Vec<f64>> {
        let times = |k: &[Complex], v: &[Complex]| -> Vec<Complex> {
            k.iter().zip(v).map(|(&a, &b)| a * b).collect()
        };
        let add = |mut a: Vec<Complex>, k: &Option<Vec<Complex>>, v: &[Complex]| {
            if let Some(k) = k {
                for ((y, &a2), &b) in a.iter_mut().zip(k.iter()).zip(v) {
                    *y = *y + a2 * b;
                }
            }
            a
        };
        let count = x.len();
        (0..count)
            .map(|c| {
                let mut y = match (count, c) {
                    (1, _) => times(&f.ll, &x[0]),
                    (_, 0) => add(times(&f.ll, &x[0]), &f.lr, &x[1]),
                    (_, 1) => add(times(&f.rr, &x[1]), &f.rl, &x[0]),
                    _ => times(&f.rest, &x[c]),
                };
                plan.run(&mut y, true);
                let tail = &mut f.tails[c];
                let out = (0..half).map(|k| y[k].re + tail[k]).collect();
                for (t, v) in tail.iter_mut().zip(&y[half..]) {
                    *t = v.re;
                }
                out
            })
            .collect()
    }

    /// The gathered half through the filters.
    fn block(&mut self) {
        let half = self.half;
        let spectra: Vec<Vec<Complex>> = self
            .input
            .iter()
            .map(|x| {
                let mut v = vec![Complex::ZERO; 2 * half];
                for (s, &x) in v.iter_mut().zip(x) {
                    s.re = x;
                }
                self.plan.run(&mut v, false);
                v
            })
            .collect();
        if self.old.is_none() && self.made != self.wanted {
            // The new filter, primed with what the last block would have
            // left it; the old one fades out.
            let mut new = self.filters(&self.wanted);
            Linear::apply(&self.plan, half, &mut new, &self.last);
            self.old = Some(std::mem::replace(&mut self.now, new));
            self.made = self.wanted.clone();
            self.fade = self.fade_len;
        }
        let mut out = Linear::apply(&self.plan, half, &mut self.now, &spectra);
        if let Some(old) = &mut self.old {
            let before = Linear::apply(&self.plan, half, old, &spectra);
            let len = self.fade_len.max(1) as f64;
            let mut fade = self.fade;
            for k in 0..half {
                let t = 1.0 - fade as f64 / len;
                for (o, b) in out.iter_mut().zip(&before) {
                    o[k] = b[k] + t * (o[k] - b[k]);
                }
                fade = fade.saturating_sub(1);
            }
            self.fade = fade;
            if fade == 0 {
                self.old = None;
            }
        }
        self.output = out;
        self.last = spectra;
    }

    /// `channels` at its rate through it, its delay late.
    fn process(&mut self, channels: &mut [Vec<f64>]) {
        let n = channels.first().map_or(0, Vec::len);
        let mut i = 0;
        while i < n {
            let m = (self.half - self.at).min(n - i);
            let span = self.at..self.at + m;
            for (c, ch) in channels.iter_mut().enumerate().take(self.channels) {
                let samples = &mut ch[i..i + m];
                self.input[c][span.clone()].copy_from_slice(samples);
                samples.copy_from_slice(&self.output[c][span.clone()]);
            }
            self.at += m;
            i += m;
            if self.at == self.half {
                self.block();
                self.at = 0;
            }
        }
    }
}

/// Mono Below and Stereo Width at work on the left and the right: the
/// sides high-passed (faded in and out as it is turned on and off) and
/// scaled.
#[derive(Debug)]
struct Sides {
    on: Glide,
    freq: Glide,
    width: Glide,
    coeffs: Coeffs,
    memory: [f64; 2],
}

impl Sides {
    fn new(s: &Settings, fs: f64) -> Sides {
        let freq = f64::from(s.mono_below);
        Sides {
            on: Glide::at(if s.mono_below_on() { 1.0 } else { 0.0 }),
            freq: Glide::at(freq),
            width: Glide::at(f64::from(s.width)),
            coeffs: mono_below_filter(fs, freq),
            memory: [0.0; 2],
        }
    }

    fn follow(&mut self, s: &Settings, fs: f64, steps: u32) {
        self.on.to(if s.mono_below_on() { 1.0 } else { 0.0 }, steps);
        self.freq.to(f64::from(s.mono_below), steps);
        self.width.to(f64::from(s.width), steps);
        if !self.freq.moving() {
            self.coeffs = mono_below_filter(fs, self.freq.now);
        }
    }

    fn process(&mut self, l: &mut [f64], r: &mut [f64], fs: f64) {
        let idle = |g: &Glide, v: f64| g.now == v && !g.moving();
        if idle(&self.on, 0.0) && idle(&self.width, 1.0) {
            self.memory = [0.0; 2];
            return;
        }
        let k = std::f64::consts::FRAC_1_SQRT_2;
        for (l, r) in l.iter_mut().zip(r.iter_mut()) {
            if self.freq.moving() {
                self.coeffs = mono_below_filter(fs, self.freq.next());
            }
            let on = self.on.next();
            let width = self.width.next();
            let m = (*l + *r) * k;
            let mut s = (*l - *r) * k;
            if on > 0.0 {
                let high = tick(&self.coeffs, &mut self.memory, s);
                s += on * (high - s);
            } else {
                self.memory = [0.0; 2];
            }
            s *= width;
            *l = (m + s) * k;
            *r = (m - s) * k;
        }
    }
}

/// The equalizer running: oversampled, its bands, its linear-phase
/// filters, a delay that makes its lateness whole samples of the rate,
/// Mono Below and Stereo Width at the top rate; the output gain after.
#[derive(Debug)]
pub(crate) struct Eq {
    rate: u32,
    channels: usize,
    settings: Option<Settings>,
    factor: usize,
    over: Oversampler,
    runs: Vec<Run>,
    linear: Option<Linear>,
    sides: Sides,
    gain: Glide,
    /// The delay's lines, each channel, and how long.
    lines: Vec<VecDeque<f64>>,
    pad: usize,
    high: Vec<Vec<f64>>,
}

impl Eq {
    pub(crate) fn new(rate: u32, channels: usize) -> Eq {
        let s = Settings::read(|i| {
            bv_timeline::EffectKind::ModernEq
                .params()
                .get(i)
                .map_or(0.0, |p| p.default)
        });
        let fs = f64::from(rate.max(1));
        Eq {
            rate: rate.max(1),
            channels: channels.max(1),
            settings: None,
            factor: 1,
            over: Oversampler::new(1),
            runs: Vec::new(),
            linear: None,
            sides: Sides::new(&s, fs),
            gain: Glide::at(1.0),
            lines: Vec::new(),
            pad: 0,
            high: Vec::new(),
        }
    }

    /// The bands' rate.
    fn fs(&self) -> f64 {
        f64::from(self.rate) * self.factor as f64
    }

    /// The linear-phase bands of `s` that are heard, as their filters are
    /// made.
    fn keys(s: &Settings) -> Vec<Key> {
        s.bands
            .iter()
            .filter(|b| b.active() && b.linear)
            .map(|b| Key {
                shape: Shape::of(b),
                freq: f64::from(b.freq),
                gain: f64::from(b.gain_db * s.scale),
                q: f64::from(b.q),
            })
            .collect()
    }

    /// The linear-phase filters `s` wants: none when no band in use is
    /// linear phase.
    fn want_linear(&mut self, s: &Settings) {
        if !s.bands.iter().any(|b| b.used && b.linear) {
            self.linear = None;
        } else {
            let keys = Eq::keys(s);
            match &mut self.linear {
                Some(l) if l.quality == s.quality && l.channels == self.channels => l.wanted = keys,
                _ => self.linear = Some(Linear::new(self.fs(), s.quality, self.channels, keys)),
            }
        }
        // Late by whole samples of the rate.
        let late = self.over.delay() + self.linear.as_ref().map_or(0, Linear::delay);
        let pad = (self.factor - late % self.factor) % self.factor;
        if pad != self.pad || self.lines.len() != self.channels {
            self.pad = pad;
            self.lines = vec![VecDeque::from(vec![0.0; pad]); self.channels];
        }
    }

    /// Everything anew for `s`: its rate, its bands where they are.
    fn rebuild(&mut self, s: &Settings) {
        self.factor = usize::from(s.oversampling.max(1));
        let fs = self.fs();
        self.over = Oversampler::new(self.factor);
        self.runs = s.bands.iter().map(|b| Run::new(b, s.scale, fs)).collect();
        self.sides = Sides::new(s, fs);
        self.gain = Glide::at(10f64.powf(f64::from(s.output_db) / 20.0));
        self.linear = None;
        self.lines.clear();
        self.want_linear(s);
    }
}

impl Own for Eq {
    fn set(&mut self, params: &[f32]) {
        let s = Settings::read(|i| params.get(i).copied().unwrap_or(0.0));
        let fresh = self
            .settings
            .is_none_or(|old| old.oversampling != s.oversampling);
        if fresh {
            self.rebuild(&s);
        } else {
            let fs = self.fs();
            let steps = (GLIDE * fs) as u32;
            for (run, b) in self.runs.iter_mut().zip(&s.bands) {
                run.follow(b, s.scale, fs, steps);
            }
            self.sides.follow(&s, fs, steps);
            self.gain.to(
                10f64.powf(f64::from(s.output_db) / 20.0),
                (GLIDE * f64::from(self.rate)) as u32,
            );
            self.want_linear(&s);
        }
        self.settings = Some(s);
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let Some(s) = self.settings else {
            return;
        };
        let n = channels.first().map_or(0, Vec::len);
        if n == 0 {
            return;
        }
        if channels.len() != self.channels {
            self.channels = channels.len();
            self.rebuild(&s);
        }
        let fs = self.fs();
        let steps = (GLIDE * fs) as u32;
        let mut high = std::mem::take(&mut self.high);
        high.resize_with(channels.len(), Vec::new);
        for (c, ch) in channels.iter().enumerate() {
            self.over.raise(c, ch, &mut high[c]);
        }
        // The minimum-phase bands, in order.
        let stereo = high.len() >= 2;
        let k = std::f64::consts::FRAC_1_SQRT_2;
        let mid_side = |high: &mut [Vec<f64>]| {
            let (l, r) = high.split_at_mut(1);
            for (l, r) in l[0].iter_mut().zip(r[0].iter_mut()) {
                (*l, *r) = ((*l + *r) * k, (*l - *r) * k);
            }
        };
        for run in &mut self.runs {
            if run.silent() && run.next.is_none() {
                continue;
            }
            match (run.shape.route, stereo) {
                (Route::Stereo, _) => {
                    let mut slots: Vec<&mut [f64]> =
                        high.iter_mut().map(Vec::as_mut_slice).collect();
                    run.process(&mut slots, fs, steps);
                }
                (Route::Right, true) => run.process(&mut [&mut high[1][..]], fs, steps),
                (Route::Mid | Route::Side, true) => {
                    mid_side(&mut high);
                    let at = usize::from(run.shape.route == Route::Side);
                    run.process(&mut [&mut high[at][..]], fs, steps);
                    mid_side(&mut high);
                }
                // One channel has no sides.
                (Route::Side, false) => {}
                _ => run.process(&mut [&mut high[0][..]], fs, steps),
            }
        }
        if let Some(l) = &mut self.linear {
            l.process(&mut high);
        }
        if self.pad > 0 {
            for (ch, line) in high.iter_mut().zip(&mut self.lines) {
                for v in ch.iter_mut() {
                    line.push_back(*v);
                    *v = line.pop_front().unwrap_or(0.0);
                }
            }
        }
        if stereo {
            let (l, r) = high.split_at_mut(1);
            self.sides.process(&mut l[0], &mut r[0], fs);
        }
        for (c, ch) in channels.iter_mut().enumerate() {
            self.over.lower(c, &mut high[c], ch);
        }
        self.high = high;
        if self.gain.moving() || self.gain.now != 1.0 {
            for i in 0..n {
                let g = self.gain.next() as f32;
                for ch in channels.iter_mut() {
                    ch[i] *= g;
                }
            }
        }
    }

    fn latency(&self) -> usize {
        (self.over.delay() + self.linear.as_ref().map_or(0, Linear::delay) + self.pad)
            / self.factor.max(1)
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
        s.oversampling = 1;
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
            q: std::f32::consts::FRAC_1_SQRT_2,
            slope: 12,
            route: Route::Stereo,
            linear: false,
        }
    }

    fn running(e: &Effect, channels: usize) -> Eq {
        let mut q = Eq::new(RATE, channels);
        q.set(&crate::fx::params_of(e));
        q
    }

    /// The change (dB) `e` makes on a tone of `f` on each side, the tone
    /// on the left and the right times `sides`.
    fn gain_at(e: &Effect, f: f32, sides: [f32; 2]) -> [f32; 2] {
        let x = sine(f, RATE as usize);
        let mut ch = vec![
            x.iter().map(|v| v * sides[0]).collect::<Vec<_>>(),
            x.iter().map(|v| v * sides[1]).collect(),
        ];
        let before = [level_db(&ch[0]), level_db(&ch[1])];
        running(e, 2).process(&mut ch);
        [level_db(&ch[0]) - before[0], level_db(&ch[1]) - before[1]]
    }

    #[test]
    fn settings_keep_in_the_parameters() {
        let e = eq(|s| {
            s.bands[2] = Band {
                route: Route::Side,
                slope: 24,
                linear: true,
                ..band(BandType::HighPass, 120.0, 0.0)
            };
            s.bands[5] = band(BandType::BandStop, 3000.0, 0.0);
            s.width = 0.5;
            s.quality = 3;
            s.mono_below = 90.0;
        });
        let s = Settings::of(&e);
        assert_eq!(s.bands[2].kind, BandType::HighPass);
        assert_eq!(s.bands[2].route, Route::Side);
        assert_eq!(s.bands[2].steepness(), 24);
        assert!(s.bands[2].linear && !s.bands[1].linear);
        assert_eq!(s.bands[5].kind, BandType::BandStop);
        assert_eq!((s.quality, s.mono_below), (3, 90.0));
        assert!(!s.bands[0].used && s.bands[0].on);
        assert!((s.width - 0.5).abs() < 1e-6);
        // A new one changes nothing, oversampled twice.
        let flat = Settings::of(&Effect::new(EffectKind::ModernEq));
        assert!(flat.bands.iter().all(|b| !b.active()));
        assert_eq!(flat.oversampling, 2);
        assert!(!flat.mono_below_on());
        assert_eq!(total_db(&flat, RATE as f32, 1000.0), 0.0);
    }

    #[test]
    fn slopes_snap_to_those_the_type_comes_in() {
        let with = |kind, slope| {
            Band {
                slope,
                ..band(kind, 1000.0, 0.0)
            }
            .steepness()
        };
        assert_eq!(with(BandType::HighPass, 18), 24);
        assert_eq!(with(BandType::LowPass, 30), 36);
        assert_eq!(with(BandType::BandPass, 36), 18);
        assert_eq!(with(BandType::LowShelf, 24), 12);
        assert_eq!(with(BandType::HighShelf, 6), 6);
        assert_eq!(with(BandType::Peaking, 6), 12);
        assert_eq!(with(BandType::BandStop, 24), 6);
        // Linear phase has no 6 dB shelves; the 6 dB passes have no Q.
        let shelf = Band {
            slope: 6,
            linear: true,
            ..band(BandType::LowShelf, 1000.0, 6.0)
        };
        assert_eq!(shelf.shape(), (BandType::LowShelf, 12));
        assert!(shelf.has_q());
        assert!(
            !Band {
                slope: 6,
                ..band(BandType::HighPass, 1000.0, 0.0)
            }
            .has_q()
        );
    }

    #[test]
    fn the_curves_are_the_cookbooks() {
        let fs = RATE as f32;
        let at = |b: Band, f: f32| band_db(&b, 1.0, fs, f);
        // Passes with Q 0.707: 3 dB down at their frequency at 12 dB an
        // octave, 6 and 9 dB two and three alike; an octave below, about
        // their slope down.
        for (slope, down) in [(12u8, 3.0f32), (24, 6.0), (36, 9.0)] {
            let b = Band {
                slope,
                ..band(BandType::HighPass, 1000.0, 0.0)
            };
            assert!(
                (at(b, 1000.0) + down).abs() < 0.1,
                "{slope}: {}",
                at(b, 1000.0)
            );
            let octave = at(b, 250.0) - at(b, 500.0);
            assert!((octave + f32::from(slope)).abs() < 1.5, "{slope}: {octave}");
        }
        // First order: 3 dB at the frequency, 6 dB an octave.
        let b = Band {
            slope: 6,
            ..band(BandType::LowPass, 1000.0, 0.0)
        };
        assert!((at(b, 1000.0) + 3.01).abs() < 0.05);
        assert!((at(b, 2000.0) - at(b, 4000.0) - 6.0).abs() < 1.0);
        // A resonant pass: Q 2 stands 6 dB up at its frequency.
        let b = Band {
            q: 2.0,
            ..band(BandType::LowPass, 1000.0, 0.0)
        };
        assert!((at(b, 1000.0) - 6.02).abs() < 0.05);
        // A band pass's middle is Q (Q² and Q³ for two and three alike).
        for (slope, power) in [(6u8, 1), (12, 2), (18, 3)] {
            let b = Band {
                slope,
                q: 2.0,
                ..band(BandType::BandPass, 1000.0, 0.0)
            };
            let want = 20.0 * 2f32.log10() * power as f32;
            assert!(
                (at(b, 1000.0) - want).abs() < 0.05,
                "{slope}: {}",
                at(b, 1000.0)
            );
        }
        // A peak: its gain at its frequency.
        let b = Band {
            q: 1.0,
            ..band(BandType::Peaking, 1000.0, 6.0)
        };
        assert!((at(b, 1000.0) - 6.0).abs() < 0.01);
        assert!(at(b, 50.0).abs() < 0.1);
        // A band stop: nothing left at its frequency.
        let b = band(BandType::BandStop, 1000.0, 0.0);
        assert!(at(b, 1000.0) < -60.0 && at(b, 100.0).abs() < 0.3);
        // Shelves: the whole gain at the far end; a 12 dB one half (in
        // dB) at its frequency, a 6 dB one too, its corner moved so.
        for slope in [12u8, 6] {
            let b = Band {
                slope,
                ..band(BandType::LowShelf, 1000.0, 12.0)
            };
            assert!((at(b, 20.0) - 12.0).abs() < 0.3, "{slope}: {}", at(b, 20.0));
            assert!(
                (at(b, 1000.0) - 6.0).abs() < 0.3,
                "{slope}: {}",
                at(b, 1000.0)
            );
            assert!(at(b, 18_000.0).abs() < 0.5, "{slope}: {}", at(b, 18_000.0));
            let b = Band {
                slope,
                ..band(BandType::HighShelf, 1000.0, -12.0)
            };
            assert!(
                (at(b, 20_000.0) + 12.0).abs() < 0.6,
                "{slope}: {}",
                at(b, 20_000.0)
            );
            assert!(at(b, 30.0).abs() < 0.3);
        }
        // Global Scale scales the gain; below 0 a boost becomes a cut.
        let b = Band {
            q: 1.0,
            ..band(BandType::Peaking, 1000.0, 6.0)
        };
        assert!((band_db(&b, -1.0, fs, 1000.0) + 6.0).abs() < 0.01);
    }

    #[test]
    fn it_sounds_as_its_curves_say() {
        let e = eq(|s| {
            s.bands[0] = Band {
                q: 1.0,
                ..band(BandType::Peaking, 1000.0, 6.0)
            }
        });
        assert!((gain_at(&e, 1000.0, [1.0, 1.0])[0] - 6.0).abs() < 0.2);
        assert!(gain_at(&e, 100.0, [1.0, 1.0])[0].abs() < 0.3);
        // A bypassed band does nothing.
        let off = eq(|s| {
            s.bands[0] = Band {
                on: false,
                ..band(BandType::Peaking, 1000.0, 6.0)
            }
        });
        assert!(gain_at(&off, 1000.0, [1.0, 1.0])[0].abs() < 0.01);
        // Oversampled, a peak near the top keeps its shape and the sound
        // follows it.
        for over in [2u8, 4, 8] {
            let e = eq(|s| {
                s.bands[0] = Band {
                    q: 1.0,
                    ..band(BandType::Peaking, 18_000.0, 6.0)
                };
                s.oversampling = over;
            });
            let [l, _] = gain_at(&e, 18_000.0, [1.0, 1.0]);
            assert!((l - 6.0).abs() < 0.6, "{over}: {l}");
            let [l, _] = gain_at(&e, 1_000.0, [1.0, 1.0]);
            assert!(l.abs() < 0.2, "{over}: {l}");
        }
    }

    #[test]
    fn oversampling_is_transparent_and_as_late_as_it_says() {
        for over in [1u8, 2, 4, 8] {
            let e = eq(|s| s.oversampling = over);
            let mut q = running(&e, 1);
            let late = q.latency();
            let mut x = vec![0.0f32; 4096];
            x[100] = 1.0;
            let mut ch = vec![x];
            q.process(&mut ch);
            let peak = ch[0]
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                .map(|(i, _)| i);
            assert_eq!(peak, Some(100 + late), "{over}");
            // A tone well inside the band comes through whole.
            let g = gain_at(&e, 5_000.0, [1.0, 1.0])[0];
            assert!(g.abs() < 0.02, "{over}: {g}");
        }
        assert_eq!(running(&eq(|s| s.oversampling = 2), 1).latency(), 49);
    }

    #[test]
    fn values_glide_without_a_click() {
        // A peak's gain jumps from 0 to 12 dB as a tone plays: the sound
        // gets there in 50 ms, smoothly.
        let e0 = eq(|s| {
            s.bands[0] = Band {
                q: 1.0,
                ..band(BandType::Peaking, 1000.0, 0.0)
            }
        });
        let e1 = eq(|s| {
            s.bands[0] = Band {
                q: 1.0,
                ..band(BandType::Peaking, 1000.0, 12.0)
            }
        });
        let mut q = running(&e0, 1);
        let x = sine(1000.0, 24_000);
        let mut a = vec![x[..12_000].to_vec()];
        q.process(&mut a);
        q.set(&crate::fx::params_of(&e1));
        let mut b = vec![x[12_000..].to_vec()];
        q.process(&mut b);
        let out: Vec<f32> = a[0].iter().chain(&b[0]).copied().collect();
        let jump = out
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0f32, f32::max);
        // A 1 kHz sine of 0.25 · 4 moves at most about 0.13 a sample.
        assert!(jump < 0.15, "{jump}");
        let late = level_db(&out[12_000 + 4_800..]) - level_db(&x[..12_000]);
        assert!((late - 12.0).abs() < 0.3, "{late}");
    }

    #[test]
    fn linear_phase_bands_sound_alike_and_ring_both_ways() {
        let e = eq(|s| {
            s.bands[0] = Band {
                linear: true,
                q: 1.0,
                ..band(BandType::Peaking, 1000.0, 6.0)
            };
            s.quality = 0;
        });
        let g = gain_at(&e, 1000.0, [1.0, 1.0]);
        assert!((g[0] - 6.0).abs() < 0.3, "{g:?}");
        assert!(gain_at(&e, 100.0, [1.0, 1.0])[0].abs() < 0.3);
        // Its impulse response is even about its middle: linear phase.
        let mut q = running(&e, 1);
        let late = q.latency();
        let mut x = vec![0.0f32; 16_384];
        x[1000] = 1.0;
        let mut ch = vec![x];
        q.process(&mut ch);
        let mid = 1000 + late;
        for k in 1..200 {
            let (a, b) = (ch[0][mid - k], ch[0][mid + k]);
            assert!((a - b).abs() < 1e-4, "{k}: {a} {b}");
        }
        // A side band of linear phase leaves a centered tone alone.
        let e = eq(|s| {
            s.bands[0] = Band {
                linear: true,
                route: Route::Side,
                q: 1.0,
                ..band(BandType::Peaking, 1000.0, -24.0)
            };
            s.quality = 0;
        });
        let centered = gain_at(&e, 1000.0, [1.0, 1.0]);
        assert!(centered[0].abs() < 0.05, "{centered:?}");
        assert!(gain_at(&e, 1000.0, [1.0, 0.0])[0] < -3.0);
    }

    #[test]
    fn routing_mid_side_width_and_scale() {
        let e = eq(|s| {
            s.bands[0] = Band {
                route: Route::Side,
                q: 1.0,
                ..band(BandType::Peaking, 1000.0, -24.0)
            }
        });
        let centered = gain_at(&e, 1000.0, [1.0, 1.0]);
        assert!(
            centered[0].abs() < 0.05 && centered[1].abs() < 0.05,
            "{centered:?}"
        );
        assert!(gain_at(&e, 1000.0, [1.0, 0.0])[0] < -3.0);
        let e = eq(|s| {
            s.bands[0] = Band {
                route: Route::Left,
                q: 1.0,
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
                s.bands[0] = Band {
                    q: 1.0,
                    ..band(BandType::Peaking, 1000.0, 6.0)
                };
                s.scale = scale;
            })
        };
        assert!((gain_at(&scaled(-1.0), 1000.0, [1.0, 1.0])[0] + 6.0).abs() < 0.2);
        assert!((gain_at(&scaled(2.0), 1000.0, [1.0, 1.0])[0] - 12.0).abs() < 0.3);
        // Mono Below: low sides go, the middle stays.
        let e = eq(|s| s.mono_below = 200.0);
        assert!(gain_at(&e, 30.0, [1.0, -1.0])[0] < -15.0);
        assert!(gain_at(&e, 60.0, [1.0, 1.0])[0].abs() < 0.05);
        assert!(gain_at(&e, 4000.0, [1.0, -1.0])[0].abs() < 0.1);
        let s = Settings::of(&e);
        let curve = mono_below_curve(&s, RATE as f32, &[30.0, 4000.0]).unwrap();
        assert!(curve[0] < -15.0 && curve[1].abs() < 0.1, "{curve:?}");
        // Output gain.
        let e = eq(|s| s.output_db = -6.0);
        assert!((gain_at(&e, 1000.0, [1.0, 1.0])[0] + 6.0).abs() < 0.01);
    }

    #[test]
    fn one_channel_and_surround() {
        let mut ch = vec![sine(1000.0, 24_000)];
        let before = level_db(&ch[0]);
        let e = eq(|s| {
            s.bands[0] = Band {
                route: Route::Side,
                q: 1.0,
                ..band(BandType::Peaking, 1000.0, -12.0)
            };
            s.bands[1] = Band {
                route: Route::Mid,
                q: 1.0,
                ..band(BandType::Peaking, 1000.0, 6.0)
            };
        });
        running(&e, 1).process(&mut ch);
        assert!((level_db(&ch[0]) - before - 6.0).abs() < 0.2);
        let x = sine(1000.0, 24_000);
        let mut six = vec![x.clone(); 6];
        let e = eq(|s| {
            s.bands[0] = Band {
                q: 1.0,
                ..band(BandType::Peaking, 1000.0, 6.0)
            }
        });
        running(&e, 6).process(&mut six);
        for c in &six {
            assert!((level_db(c) - level_db(&x) - 6.0).abs() < 0.2);
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
}
