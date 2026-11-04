//! Color Grading: what the Color Grading window works, as VEGAS's Color
//! Grading effect holds it — the same parameters, all of them hidden (the
//! window edits them): each wheel's red, green, blue and Y (and where its
//! point stands, U and V, for the window to draw it again), the RL
//! wheels and their two thresholds, the ten curves as text
//! (`N:x:y:ix:iy:ox:oy:…`), Saturation, Exposure (two: one not on the
//! panel), Brightness, Contrast and its center, Temperature and Tint,
//! Levels, Hue, Sat and Val, and the Camera and Look LUTs (on, file,
//! interpolation, strength). The work is [`crate::grading`]'s.
//!
//! The wheels' points map to their colors as VEGAS's do: three lines
//! through the middle, at 5° (red), −40° (green) and 65° (blue); a
//! channel goes toward its top the farther the point is from its line on
//! its side (all of it 51 of the wheel's 85 pixels away), toward its
//! bottom on the other ([`wheel_values`]); typed values put the point
//! back about where it gives them ([`wheel_point`]).
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_void};

use bv_core::curve::CurvePoint;
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::grading::{self, Grade, HslCurve, Knot, LutStep, LutTable, Params, Wheel};
use crate::ofx::{Fx, Props, is, suites};

pub(crate) const ID: &CStr = c"org.bananav.ColorGrading";

/// The Color Wheels' and the RL Color Wheels' names (each wheel's
/// parameters are its name and R, G, B, Y, U or V).
pub const WHEELS: [&str; 4] = ["lift", "gamma", "gain", "offset"];
pub const RL_WHEELS: [&str; 4] = [
    "logWheelShadows",
    "logWheelMidtones",
    "logWheelHighlights",
    "logWheelOffset",
];
/// A wheel's parts: its color, its Y, its point (U across, V up: pixels
/// of an 85-pixel wheel).
pub const PARTS: [&str; 6] = ["R", "G", "B", "Y", "U", "V"];
/// The panel's range of each wheel's color and Y, and where it rests:
/// Lift, Gamma, Gain, Offset; every RL wheel −1 to 1 about 0.
pub const WHEEL_RANGES: [(f64, f64, f64); 4] = [
    (-1.0, 1.0, 0.0),
    (0.0, 5.0, 1.0),
    (0.0, 2.0, 1.0),
    (-1.0, 1.0, 0.0),
];
pub const RL_RANGE: (f64, f64, f64) = (-1.0, 1.0, 0.0);

/// The curves: red, green, blue, Y; then the HSL Curves.
pub const CURVES: [&str; 10] = [
    "curveR", "curveG", "curveB", "curveY", "curveHvH", "curveHvS", "curveHvL", "curveSvS",
    "curveSvL", "curveLvS",
];
pub const HIGH_THRESHOLD: &str = "logWheelHighThreshold";
pub const LOW_THRESHOLD: &str = "logWheelLowThreshold";
pub const SATURATION: &str = "saturationAdjust";
pub const EXPOSURE: &str = "exposureAdjust";
pub const EXPOSURE_LN: &str = "exposureLnAdjust";
pub const BRIGHTNESS: &str = "brightnessAdjust";
pub const CONTRAST: &str = "contrastAdjust";
pub const CONTRAST_CENTER: &str = "contrastCtrAdjust";
pub const TEMPERATURE: &str = "temperatureAdjust";
pub const TINT: &str = "tintAdjust";
pub const IN_MIN: &str = "inMinAdjust";
pub const IN_MAX: &str = "inMaxAdjust";
pub const OUT_MIN: &str = "outMinAdjust";
pub const OUT_MAX: &str = "outMaxAdjust";
pub const HUE: &str = "hueAdjust";
pub const SAT: &str = "satAdjust";
pub const VAL: &str = "valAdjust";
/// The LUTs' rows: 0 the Camera LUT, 1 the Look LUT.
pub const LUT_ENABLE: [&str; 2] = ["lutEnable0", "lutEnable1"];
pub const LUT_FILE: [&str; 2] = ["lutFilename0", "lutFilename1"];
pub const LUT_INTERPOLATION: [&str; 2] = ["lutInterpolation0", "lutInterpolation1"];
pub const LUT_GAIN: [&str; 2] = ["lutGain0", "lutGain1"];

/// The Camera LUT's curves BananaV knows (the LUT list's own entries):
/// Apple Log decoded to Rec. 709's linear light or to its video.
pub const APPLE_LOG_TO_LINEAR: &str = "builtin:apple-log-to-linear";
pub const APPLE_LOG_TO_REC709: &str = "builtin:apple-log-to-rec709";

/// A wheel's parameter.
pub fn wheel_param(wheel: &str, part: &str) -> String {
    format!("{wheel}{part}")
}

fn c(name: &str) -> CString {
    CString::new(name).unwrap_or_default()
}

// The wheels' points.

/// How far a point is from a channel's line, as a part of the way to its
/// end (51 pixels of the 85).
const REACH: f64 = 85.0 * 0.6;
/// Where each channel's top lies from the middle (degrees, y up): above
/// its line for red, toward the bottom left for green, the bottom right
/// for blue.
const TOWARD: [f64; 3] = [95.0, 230.0, 335.0];

/// The color a wheel's point (`u` across, `v` up, pixels of an 85-pixel
/// wheel) gives, the wheel's range `(low, high, rest)`.
pub fn wheel_values(u: f64, v: f64, (low, high, rest): (f64, f64, f64)) -> [f64; 3] {
    TOWARD.map(|a| {
        let (s, c) = a.to_radians().sin_cos();
        let d = ((u * c + v * s) / REACH).clamp(-1.0, 1.0);
        if d >= 0.0 {
            rest + (high - rest) * d
        } else {
            rest + (low - rest) * -d
        }
    })
}

/// About where a wheel's point gives `values` (typed, or set by a
/// picker): the channels' changes added along 185°, −40° and 65°, the
/// sum's length scaled to the wheel and its angle turned about 88.5°
/// (94.79° on Gamma). `gamma` says the wheel is Gamma's.
pub fn wheel_point(values: [f64; 3], (low, high, rest): (f64, f64, f64), gamma: bool) -> [f64; 2] {
    let (mut x, mut y) = (0.0, 0.0);
    for (v, a) in values.iter().zip([185.0f64, -40.0, 65.0]) {
        let (s, c) = a.to_radians().sin_cos();
        x += (v - rest) * c;
        y += (v - rest) * s;
    }
    let reach = (rest - low).abs().max((high - rest).abs()).max(1e-9);
    let scale = if gamma {
        85.0 / 3f64.sqrt()
    } else {
        85.0 / (3.0 * reach * reach).sqrt()
    };
    let r = (x.hypot(y) * scale).min(85.0);
    let turn = if gamma { 94.79 } else { 88.5 };
    let a = (turn - y.atan2(x).to_degrees()).to_radians();
    // The angle counts down the screen: up is the other way.
    [r * a.cos(), -r * a.sin()]
}

// The curves' text as the window writes it.

impl From<Knot> for CurvePoint {
    fn from(k: Knot) -> CurvePoint {
        CurvePoint::with(
            f64::from(k.x),
            f64::from(k.y),
            [f64::from(k.ix), f64::from(k.iy)],
            [f64::from(k.ox), f64::from(k.oy)],
        )
    }
}

fn knot(p: &CurvePoint) -> Knot {
    let (before, after) = p
        .handles
        .map_or(([0.0; 2], [0.0; 2]), |h| (h.before, h.after));
    Knot::new(
        p.x as f32,
        p.y as f32,
        before[0] as f32,
        before[1] as f32,
        after[0] as f32,
        after[1] as f32,
    )
}

/// A Color Curve's text from its points: points at x = 0 and x = 1 added
/// when the curve does not reach them, flat (their handles 0.05 long).
pub fn color_curve_text(points: &[CurvePoint]) -> String {
    let mut k: Vec<Knot> = points.iter().map(knot).collect();
    k.sort_by(|a, b| a.x.total_cmp(&b.x));
    if let Some(first) = k.first().copied()
        && first.x > 0.0
    {
        k.insert(0, Knot::new(0.0, first.y, 0.0, 0.0, 0.05, 0.0));
    }
    if let Some(last) = k.last().copied()
        && last.x < 1.0
    {
        k.push(Knot::new(1.0, last.y, -0.05, 0.0, 0.0, 0.0));
    }
    grading::curve_text(&k)
}

/// A hue curve's text from its points (0–1): the last one copied a turn
/// before the first, the first a turn after the last, so that the curve
/// goes round.
pub fn hue_curve_text(points: &[CurvePoint]) -> String {
    let mut k: Vec<Knot> = points.iter().map(knot).collect();
    k.sort_by(|a, b| a.x.total_cmp(&b.x));
    if let (Some(first), Some(last)) = (k.first().copied(), k.last().copied()) {
        k.insert(
            0,
            Knot {
                x: last.x - 1.0,
                ..last
            },
        );
        k.push(Knot {
            x: first.x + 1.0,
            ..first
        });
    }
    grading::curve_text(&k)
}

/// A curve's points as the window edits them: a hue curve's own (the
/// turn's copies, its first and last, left out), the others' all.
pub fn curve_points(text: &str, hue: bool) -> Vec<CurvePoint> {
    if grading::untouched(text) {
        return Vec::new();
    }
    let mut k = grading::parse_curve(text);
    if hue {
        if k.len() >= 3 {
            k = k[1..k.len() - 1].to_vec();
        } else {
            k.retain(|p| (0.0..1.0).contains(&p.x));
        }
    }
    k.into_iter().map(CurvePoint::from).collect()
}

/// A curve's values as the effect works them out (its table: 511
/// entries over 0–1), for the window to draw; `None` when untouched.
pub fn curve_table(text: &str) -> Option<Vec<f32>> {
    if grading::untouched(text) {
        return None;
    }
    let t = grading::table(&grading::parse_curve(text), 511, 0.0);
    (!t.is_empty()).then_some(t)
}

// The settings.

/// A LUT row: its file (or a built-in curve), strength, interpolation and
/// whether it applies.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LutRow {
    pub file: String,
    pub gain: f64,
    pub tetrahedral: bool,
    pub on: bool,
}

impl LutRow {
    fn step(&self) -> Option<LutStep> {
        if !self.on || self.gain <= 0.001 {
            return None;
        }
        let table = match self.file.trim() {
            APPLE_LOG_TO_LINEAR => LutTable::AppleLog { video: false },
            APPLE_LOG_TO_REC709 => LutTable::AppleLog { video: true },
            file => LutTable::Cube {
                lut: crate::lut_filter::table(file)?,
                tetrahedral: self.tetrahedral,
            },
        };
        Some(LutStep {
            table,
            gain: self.gain,
        })
    }
}

/// The settings at a time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    pub params: Params,
    pub luts: [LutRow; 2],
}

impl Settings {
    /// The settings from their values by name (`num`, `text`; `None` for
    /// one not given: its default).
    fn read(num: impl Fn(&str) -> Option<f64>, text: impl Fn(&str) -> Option<String>) -> Settings {
        let n = |name: &str, default: f64| num(name).unwrap_or(default);
        let wheel = |name: &str, rest: f64| Wheel {
            rgb: std::array::from_fn(|i| n(&wheel_param(name, PARTS[i]), rest)),
            y: n(&wheel_param(name, "Y"), rest),
        };
        let p = Params {
            wheels: std::array::from_fn(|i| wheel(WHEELS[i], WHEEL_RANGES[i].2)),
            rl: std::array::from_fn(|i| wheel(RL_WHEELS[i], 0.0)),
            thresholds: [n(LOW_THRESHOLD, 0.25), n(HIGH_THRESHOLD, 0.5)],
            curves: std::array::from_fn(|i| {
                text(CURVES[i]).unwrap_or_else(|| grading::DEFAULT_CURVE.to_string())
            }),
            saturation: n(SATURATION, 1.0),
            exposure: n(EXPOSURE, 0.0),
            exposure_ln: n(EXPOSURE_LN, 0.0),
            brightness: n(BRIGHTNESS, 0.0),
            contrast: n(CONTRAST, 0.0),
            contrast_center: n(CONTRAST_CENTER, 0.5),
            temperature: n(TEMPERATURE, 0.0),
            tint: n(TINT, 0.0),
            levels: [
                n(IN_MIN, 0.0),
                n(IN_MAX, 1.0),
                n(OUT_MIN, 0.0),
                n(OUT_MAX, 1.0),
            ],
            hsl: [n(HUE, 0.0), n(SAT, 1.0), n(VAL, 1.0)],
        };
        let row = |k: usize| LutRow {
            file: text(LUT_FILE[k]).unwrap_or_default(),
            gain: n(LUT_GAIN[k], 1.0).clamp(0.0, 1.0),
            tetrahedral: n(LUT_INTERPOLATION[k], 0.0).round() != 0.0,
            on: n(LUT_ENABLE[k], 0.0) >= 0.5,
        };
        Settings {
            params: p,
            luts: [row(0), row(1)],
        }
    }

    /// The settings in `v`, a time's values as the host gives them.
    pub(crate) fn of(v: Values<'_>) -> Settings {
        Settings::read(
            |name| v.nums(&c(name)).and_then(|n| n.first().copied()),
            |name| v.text(&c(name)).map(str::to_string),
        )
    }

    fn at(fx: Fx, time: f64) -> Settings {
        Settings::read(
            |name| {
                let p = fx.param(&c(name));
                Some(
                    if name.starts_with("lutEnable") || name.starts_with("lutInterpolation") {
                        f64::from(p.int_at(time))
                    } else {
                        p.f64_at(time)
                    },
                )
            },
            |name| Some(fx.param(&c(name)).text_at(time)),
        )
    }

    /// Ready to color pixels.
    pub(crate) fn grade(&self) -> Grade {
        Grade::new(&self.params, self.luts[0].step(), self.luts[1].step())
    }
}

// The window's pickers.

/// The Color Grading window's pickers: what a color picked on the preview
/// (the effect's input there) sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picker {
    /// Lift's eyedropper.
    BlackPoint,
    /// Gain's eyedropper.
    WhitePoint,
    /// White Balance's Select Neutral Tone: the color becomes a grey.
    NeutralTone,
    /// The HSL Curves' Color Selector on curve `n` (Hue vs Hue … Luminance
    /// vs Saturation): a point where a color clicked stands, or three for a
    /// region's colors — their two ends and their mean.
    Selector(usize),
}

/// What `picker` sets, from the colors picked (straight RGB, the effect's
/// input: a click's few pixels, or a region's when `region`) and the
/// effect's `values`: the parameters' new values by name. Nothing when
/// the colors say nothing (greys have no hue; black takes no white
/// balance).
pub fn pick(
    values: &[(String, crate::accel::Value)],
    picker: Picker,
    colors: &[[f32; 3]],
    region: bool,
) -> Vec<(String, crate::accel::Value)> {
    use crate::accel::Value;
    if colors.is_empty() {
        return Vec::new();
    }
    let settings = Settings::of(Values(values));
    let n = colors.len() as f64;
    let mean: [f64; 3] =
        std::array::from_fn(|i| colors.iter().map(|c| f64::from(c[i])).sum::<f64>() / n);
    let num = |v: f64| Value::Num(vec![v]);
    match picker {
        // VEGAS's eyedroppers: each channel of the color between the
        // wheel's ends — Lift's turned over, Gain's as it is.
        Picker::BlackPoint | Picker::WhitePoint => {
            let w = if picker == Picker::BlackPoint { 0 } else { 2 };
            let range = WHEEL_RANGES[w];
            let (low, high, _) = range;
            let v = mean.map(|x| {
                let x = x.clamp(0.0, 1.0);
                let x = if w == 0 { 1.0 - x } else { x };
                ((low + x * (high - low)) * 1000.0).round() / 1000.0
            });
            let [u, vv] = wheel_point(v, range, false);
            let mut out: Vec<(String, Value)> = (0..3)
                .map(|i| (wheel_param(WHEELS[w], PARTS[i]), num(v[i])))
                .collect();
            out.push((wheel_param(WHEELS[w], "U"), num(u)));
            out.push((wheel_param(WHEELS[w], "V"), num(vv)));
            out
        }
        // The temperature and tint that leave the color a grey.
        Picker::NeutralTone => {
            let before = before_balance(&settings, mean);
            if before.iter().all(|v| *v < 1e-4) {
                return Vec::new();
            }
            let (t, k) = neutral_balance(before);
            vec![
                (TEMPERATURE.to_string(), num(t)),
                (TINT.to_string(), num(k)),
            ]
        }
        Picker::Selector(curve) => {
            let curve = curve.min(5);
            let kind = HslCurve::ALL[curve];
            let keys: Vec<f64> = colors
                .iter()
                .map(|c| selector_key(kind, c.map(f64::from)))
                .collect();
            let xs = match kind {
                HslCurve::HueHue | HslCurve::HueSat | HslCurve::HueLum => {
                    hue_points(&keys, colors, region)
                }
                _ => spread_points(&keys, region),
            };
            if xs.is_empty() {
                return Vec::new();
            }
            let hue = curve < 3;
            let text = &settings.params.curves[4 + curve];
            let mut points = curve_points(text, hue);
            let table = curve_table(text);
            for x in xs {
                if points.iter().any(|p| (p.x - x).abs() < 0.004) {
                    continue;
                }
                // Where the curve is already: it keeps its shape.
                let y = table
                    .as_ref()
                    .map_or(0.5, |t| grading::lookup(t, x).clamp(0.0, 1.0));
                points.push(CurvePoint::new(x, y));
                points.sort_by(|a, b| a.x.total_cmp(&b.x));
            }
            let points = flat_handles(&points);
            let text = if hue {
                hue_curve_text(&points)
            } else {
                color_curve_text(&points)
            };
            vec![(CURVES[4 + curve].to_string(), Value::Text(text))]
        }
    }
}

/// Points given flat handles, each half the way to its neighbour (at most
/// 0.125): the window's flattened points.
pub fn flat_handles(points: &[CurvePoint]) -> Vec<CurvePoint> {
    (0..points.len())
        .map(|i| {
            let p = points[i];
            if p.handles.is_some() {
                return p;
            }
            let before = i
                .checked_sub(1)
                .map_or(0.125, |j| ((p.x - points[j].x) / 2.0).min(0.125));
            let after = points
                .get(i + 1)
                .map_or(0.125, |q| ((q.x - p.x) / 2.0).min(0.125));
            CurvePoint::with(p.x, p.y, [-before, 0.0], [after, 0.0])
        })
        .collect()
}

/// A color's key on an HSL curve: its hue, saturation or (Luminance vs
/// Saturation) lightness.
fn selector_key(kind: HslCurve, c: [f64; 3]) -> f64 {
    let [h, s, l] = hsl_of(c);
    match kind {
        HslCurve::HueHue | HslCurve::HueSat | HslCurve::HueLum => h,
        HslCurve::SatSat | HslCurve::SatLum => s,
        HslCurve::LumSat => l,
    }
}

fn hsl_of([r, g, b]: [f64; 3]) -> [f64; 3] {
    let lo = r.min(g).min(b);
    let hi = r.max(g).max(b);
    let l = (lo + hi) / 2.0;
    let d = hi - lo;
    if d <= 1e-9 {
        return [0.0, 0.0, l];
    }
    let s = if l > 0.5 {
        d / (2.0 - hi - lo)
    } else {
        d / (hi + lo)
    };
    let h = if hi == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if hi == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h / 6.0, s, l]
}

/// The hues a Color Selector puts points at: their mean (the saturated
/// weighing more, greys none) and for a region its two ends round it.
fn hue_points(hues: &[f64], colors: &[[f32; 3]], region: bool) -> Vec<f64> {
    use std::f64::consts::TAU;
    let (mut x, mut y) = (0.0f64, 0.0f64);
    let mut kept = Vec::new();
    for (&h, c) in hues.iter().zip(colors) {
        let s = hsl_of(c.map(f64::from))[1];
        if s < 0.02 {
            continue;
        }
        let (sin, cos) = (h * TAU).sin_cos();
        x += s * cos;
        y += s * sin;
        kept.push(h);
    }
    if kept.is_empty() || x.hypot(y) < 1e-9 {
        return Vec::new();
    }
    let mean = (y.atan2(x) / TAU).rem_euclid(1.0);
    if !region {
        return vec![mean];
    }
    let off = |h: f64| (h - mean + 0.5).rem_euclid(1.0) - 0.5;
    let lo = kept.iter().map(|&h| off(h)).fold(0.0, f64::min);
    let hi = kept.iter().map(|&h| off(h)).fold(0.0, f64::max);
    if hi - lo < 0.01 {
        return vec![mean];
    }
    vec![
        (mean + lo).rem_euclid(1.0),
        mean,
        (mean + hi).rem_euclid(1.0),
    ]
}

/// The saturations or lightnesses a Color Selector puts points at: their
/// mean, and for a region the lowest and the highest.
fn spread_points(values: &[f64], region: bool) -> Vec<f64> {
    let v: Vec<f64> = values.iter().map(|v| v.clamp(0.0, 1.0)).collect();
    if v.is_empty() {
        return Vec::new();
    }
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    let lo = v.iter().copied().fold(1.0, f64::min);
    let hi = v.iter().copied().fold(0.0, f64::max);
    if !region || hi - lo < 0.01 {
        return vec![mean];
    }
    vec![lo, mean, hi]
}

/// A color where White Balance takes it: through the Camera LUT and the
/// HSL Curves.
fn before_balance(settings: &Settings, c: [f64; 3]) -> [f64; 3] {
    let mut only = settings.clone();
    only.luts[1].on = false;
    let mut p = Params::default();
    p.curves[4..].clone_from_slice(&settings.params.curves[4..]);
    only.params = p;
    let g = only.grade();
    g.apply(c.map(|v| v as f32)).map(f64::from)
}

/// The temperature and tint (within their ranges) that leave `c` a grey,
/// found by halving steps from the middle.
fn neutral_balance(c: [f64; 3]) -> (f64, f64) {
    let off = |t: f64, k: f64| {
        let m = grading::balance_matrix(t, k);
        let lin = c.map(|v| v.max(0.0).powf(2.4));
        let o: [f64; 3] = std::array::from_fn(|i| {
            (m[i][0] * lin[0] + m[i][1] * lin[1] + m[i][2] * lin[2])
                .max(0.0)
                .powf(1.0 / 2.4)
        });
        let mean = (o[0] + o[1] + o[2]) / 3.0;
        o.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
    };
    let (mut t, mut k) = (0.0f64, 0.0f64);
    let (mut st, mut sk) = (2000.0f64, 50.0f64);
    while st > 0.5 {
        let mut best = (off(t, k), t, k);
        for dt in [-st, 0.0, st] {
            for dk in [-sk, 0.0, sk] {
                let (nt, nk) = (
                    (t + dt).clamp(-4000.0, 4000.0),
                    (k + dk).clamp(-100.0, 100.0),
                );
                let e = off(nt, nk);
                if e < best.0 {
                    best = (e, nt, nk);
                }
            }
        }
        if best.1 == t && best.2 == k {
            st /= 2.0;
            sk /= 2.0;
        }
        (t, k) = (best.1, best.2);
    }
    ((t * 10.0).round() / 10.0, (k * 1000.0).round() / 1000.0)
}

/// The Color Grading's grade with `values` (the parameters set; the
/// others at their defaults) as a `.cube` 3D table `size` points a side:
/// Export LUT.
pub fn cube(values: &[(String, crate::accel::Value)], size: usize) -> String {
    let size = size.clamp(2, 129);
    let grade = Settings::of(Values(values)).grade();
    let mut out = String::with_capacity(size * size * size * 30 + 100);
    out.push_str("TITLE \"BananaV Color Grading\"\n");
    out.push_str(&format!("LUT_3D_SIZE {size}\n"));
    out.push_str("DOMAIN_MIN 0.0 0.0 0.0\nDOMAIN_MAX 1.0 1.0 1.0\n");
    let step = 1.0 / (size - 1) as f32;
    // Red changes fastest, as the format has it.
    for b in 0..size {
        for g in 0..size {
            for r in 0..size {
                let c = grade.apply([r as f32 * step, g as f32 * step, b as f32 * step]);
                out.push_str(&format!("{:.6} {:.6} {:.6}\n", c[0], c[1], c[2]));
            }
        }
    }
    out
}

/// Apple Log (Apple's published curve, Rec. 2020's primaries) decoded:
/// Rec. 709's linear light, or its video (BT.709's curve, clipped at
/// white).
pub(crate) fn apple_log(c: [f32; 3], rec709: bool) -> [f32; 3] {
    const R0: f64 = -0.05641088;
    const RT: f64 = 0.01;
    const C: f64 = 47.28711236;
    const B: f64 = 0.00964052;
    const G: f64 = 0.08550479;
    const D: f64 = 0.69336945;
    let linear = |x: f32| {
        let x = f64::from(x);
        let pt = C * (RT - R0).powi(2);
        if x >= pt {
            2f64.powf((x - D) / G) - B
        } else if x >= 0.0 {
            (x / C).sqrt() + R0
        } else {
            R0
        }
    };
    let l = c.map(linear);
    // Rec. 2020 to Rec. 709 (both D65).
    const M: [[f64; 3]; 3] = [
        [1.660_491, -0.587_641, -0.072_850],
        [-0.124_551, 1.132_900, -0.008_349],
        [-0.018_151, -0.100_579, 1.118_730],
    ];
    let out: [f64; 3] = std::array::from_fn(|i| M[i][0] * l[0] + M[i][1] * l[1] + M[i][2] * l[2]);
    if !rec709 {
        return out.map(|v| v as f32);
    }
    out.map(|v| {
        let v = v.clamp(0.0, 1.0);
        let e = if v < 0.018 {
            4.5 * v
        } else {
            1.099 * v.powf(0.45) - 0.099
        };
        e as f32
    })
}

// The card.

impl Grade {
    /// What the card draws (`accel::GradePlan`): `steps` holds the
    /// settings, a row each, then the tables, an entry a row (see
    /// `shaders/own_pair.wgsl`'s `grade`).
    pub(crate) fn plan(&self) -> crate::accel::GradePlan {
        use crate::accel::GradeLut;
        let lut = |s: &Option<LutStep>| {
            s.as_ref().map(|s| match &s.table {
                LutTable::Cube { lut, tetrahedral } => GradeLut::Cube {
                    lut: lut.clone(),
                    tetrahedral: *tetrahedral,
                    strength: s.gain as f32,
                },
                LutTable::AppleLog { video } => GradeLut::AppleLog {
                    video: *video,
                    strength: s.gain as f32,
                },
            })
        };
        let f = |v: f64| v as f32;
        let on = |b: bool| if b { 1.0 } else { 0.0 };
        let mut rows = vec![[0.0f32; 4]; HEAD];
        let mut tables: Vec<f32> = Vec::new();
        // A table's place (after the head) and length.
        let mut put = |t: &[f32]| -> [f32; 2] {
            let at = tables.len();
            tables.extend_from_slice(t);
            [(HEAD + at) as f32, t.len() as f32]
        };
        rows[0] = [
            on(self.balance.is_some()),
            on(self.levels.is_some()),
            on(self.rl_rgb.is_some()),
            on(self.rl_y.is_some()),
        ];
        rows[1] = [
            on(self.curves_rgb.is_some()),
            on(self.curve_y.is_some()),
            on(self.lift.is_some()),
            self.hsl_curves.len() as f32,
        ];
        rows[2] = [
            self.saturation.map_or(1.0, f),
            self.exposure.map_or(1.0, f),
            self.brightness.map_or(0.0, f),
            on(self.exposure_ln.is_some()),
        ];
        let (k, center) = self.contrast.unwrap_or((0.0, 0.5));
        rows[3] = [
            f(k),
            f(center),
            self.exposure_ln.as_ref().map_or(1.0, |e| f(e.pre)),
            f(self.hsl[0]),
        ];
        rows[4] = [
            f(self.hsl[1]),
            f(self.hsl[2]),
            on(self.cdl[0].is_some()),
            on(self.cdl[1].is_some()),
        ];
        if let Some(m) = &self.balance {
            for (i, row) in m.iter().enumerate() {
                rows[5 + i] = [f(row[0]), f(row[1]), f(row[2]), 0.0];
            }
        }
        if let Some(l) = self.levels {
            rows[8] = l.map(f);
        }
        if let Some(l) = self.lift {
            rows[9] = [f(l[0]), f(l[1]), f(l[2]), 0.0];
        }
        for (i, cdl) in self.cdl.iter().enumerate() {
            if let Some(c) = cdl {
                rows[10 + 3 * i] = [f(c.slope[0]), f(c.slope[1]), f(c.slope[2]), 0.0];
                rows[11 + 3 * i] = [f(c.offset[0]), f(c.offset[1]), f(c.offset[2]), 0.0];
                rows[12 + 3 * i] = [f(c.power[0]), f(c.power[1]), f(c.power[2]), 0.0];
            }
        }
        for (i, (kind, t)) in self.hsl_curves.iter().enumerate() {
            let [at, len] = put(t);
            let k = HslCurve::ALL.iter().position(|c| c == kind).unwrap_or(0);
            rows[16 + i] = [k as f32, at, len, 0.0];
        }
        let none: &[f32] = &[];
        let rl = self.rl_rgb.as_ref();
        let curves = self.curves_rgb.as_ref();
        let listed: [&[f32]; 9] = [
            rl.map_or(none, |t| &t[0]),
            rl.map_or(none, |t| &t[1]),
            rl.map_or(none, |t| &t[2]),
            self.rl_y.as_deref().unwrap_or(none),
            curves.map_or(none, |t| &t[0]),
            curves.map_or(none, |t| &t[1]),
            curves.map_or(none, |t| &t[2]),
            self.curve_y.as_deref().unwrap_or(none),
            self.exposure_ln.as_ref().map_or(none, |e| &e.table),
        ];
        for (i, t) in listed.iter().enumerate() {
            let [at, len] = put(t);
            rows[22 + i] = [at, len, 0.0, 0.0];
        }
        rows.extend(tables.into_iter().map(|v| [v, 0.0, 0.0, 0.0]));
        crate::accel::GradePlan {
            camera: lut(&self.camera),
            steps: rows,
            entries: HEAD as u32,
            look: lut(&self.look),
        }
    }
}

/// The card's settings' rows before the tables.
const HEAD: usize = 32;

// The plug-in.

/// The plug-in's entry point.
pub(crate) unsafe extern "C" fn main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if suites().is_none() {
        return kOfxStatErrMissingHostFeature;
    }
    let fx = Fx(handle as OfxImageEffectHandle);
    let (args, out) = (Props(in_args), Props(out_args));
    if is(action, kOfxActionDescribe) {
        describe(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        describe_in_context(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        let grade = Settings::at(fx, args.dbl(kOfxPropTime, 0)).grade();
        crate::colors::render(fx, args, &|c| grade.apply(c))
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        if Settings::at(fx, args.dbl(kOfxPropTime, 0))
            .grade()
            .changes_nothing()
        {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &roi);
        kOfxStatOK
    } else if is(action, kOfxActionLoad)
        || is(action, kOfxActionUnload)
        || is(action, kOfxActionCreateInstance)
        || is(action, kOfxActionDestroyInstance)
    {
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Color Grading");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"The Color Grading window's work: LUTs, color wheels, utilities and curves.",
    );
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextFilter, kOfxImageEffectContextGeneral],
    );
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    p.set_int(kOfxImageEffectPropSupportsTiles, 1);
    p.set_int(kOfxImageEffectPropSupportsMultiResolution, 1);
    p.set_int(kOfxImageEffectPluginPropHostFrameThreading, 0);
    p.set_str(
        kOfxImageEffectPluginRenderThreadSafety,
        kOfxImageEffectRenderFullySafe,
    );
}

/// A parameter's label: its name with its words apart ("liftR": "Lift
/// R").
fn label(name: &str) -> CString {
    let mut s = String::new();
    for (i, ch) in name.chars().enumerate() {
        if i == 0 {
            s.extend(ch.to_uppercase());
        } else {
            if ch.is_ascii_uppercase() || ch.is_ascii_digit() {
                s.push(' ');
            }
            s.push(ch);
        }
    }
    c(&s)
}

fn describe_in_context(fx: Fx) {
    let comps = [
        kOfxImageComponentRGBA,
        kOfxImageComponentRGB,
        kOfxImageComponentAlpha,
    ];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);
    // Every parameter hidden: the Color Grading window edits them.
    let hidden = |p: Props| {
        p.set_int(kOfxParamPropSecret, 1);
        p
    };
    // A number: its range, the panel's (shown), its default and its
    // decimals.
    let number =
        |name: &str, [min, max]: [f64; 2], [shown_min, shown_max]: [f64; 2], default: f64| {
            let p = hidden(fx.define_param(kOfxParamTypeDouble, &c(name), &label(name)));
            p.set_dbl(kOfxParamPropDefault, default);
            p.set_dbl(kOfxParamPropMin, min);
            p.set_dbl(kOfxParamPropMax, max);
            p.set_dbl(kOfxParamPropDisplayMin, shown_min);
            p.set_dbl(kOfxParamPropDisplayMax, shown_max);
            p.set_dbl(kOfxParamPropIncrement, 0.001);
            p.set_int(kOfxParamPropDigits, 3);
        };
    let text = |name: &str, default: &str| {
        let p = hidden(fx.define_param(kOfxParamTypeString, &c(name), &label(name)));
        p.set_str(kOfxParamPropDefault, &c(default));
        p.set_int(kOfxParamPropAnimates, 1);
        p
    };
    for name in CURVES {
        text(name, grading::DEFAULT_CURVE);
    }
    // The wheels: Lift, Gamma, Gain and Offset's own ranges; the RL
    // wheels −1 to 1.
    let limits = [[-1.0, 1.0], [-5.0, 5.0], [0.0, 1000.0], [-1000.0, 1000.0]];
    for (i, wheel) in WHEELS.into_iter().enumerate() {
        let (lo, hi, rest) = WHEEL_RANGES[i];
        for part in ["R", "G", "B", "Y"] {
            number(&wheel_param(wheel, part), limits[i], [lo, hi], rest);
        }
        for part in ["U", "V"] {
            number(&wheel_param(wheel, part), [-85.0, 85.0], [-85.0, 85.0], 0.0);
        }
    }
    for wheel in RL_WHEELS {
        for part in ["R", "G", "B", "Y"] {
            number(&wheel_param(wheel, part), [-1.0, 1.0], [-1.0, 1.0], 0.0);
        }
        for part in ["U", "V"] {
            number(&wheel_param(wheel, part), [-85.0, 85.0], [-85.0, 85.0], 0.0);
        }
    }
    number(HIGH_THRESHOLD, [0.0, 1.0], [0.0, 1.0], 0.5);
    number(LOW_THRESHOLD, [0.0, 1.0], [0.0, 1.0], 0.25);
    number(SATURATION, [0.0, 2.0], [0.0, 2.0], 1.0);
    number(EXPOSURE, [-5.0, 5.0], [-3.0, 3.0], 0.0);
    number(EXPOSURE_LN, [-5.0, 5.0], [-5.0, 5.0], 0.0);
    number(BRIGHTNESS, [-1.0, 1.0], [-1.0, 1.0], 0.0);
    number(CONTRAST, [-1.0, 1.0], [-1.0, 1.0], 0.0);
    number(CONTRAST_CENTER, [0.0, 1.0], [0.0, 1.0], 0.5);
    number(TEMPERATURE, [-4000.0, 4000.0], [-4000.0, 4000.0], 0.0);
    number(TINT, [-100.0, 100.0], [-100.0, 100.0], 0.0);
    number(IN_MIN, [0.0, 1.0], [0.0, 1.0], 0.0);
    number(IN_MAX, [0.0, 1.0], [0.0, 1.0], 1.0);
    number(OUT_MIN, [0.0, 1.0], [0.0, 1.0], 0.0);
    number(OUT_MAX, [0.0, 1.0], [0.0, 1.0], 1.0);
    number(HUE, [0.0, 1.0], [0.0, 1.0], 0.0);
    number(SAT, [0.0, 5.0], [0.0, 2.0], 1.0);
    number(VAL, [0.0, 5.0], [0.0, 2.0], 1.0);
    for k in 0..2 {
        let p = hidden(fx.define_param(
            kOfxParamTypeBoolean,
            &c(LUT_ENABLE[k]),
            &label(LUT_ENABLE[k]),
        ));
        p.set_int(kOfxParamPropDefault, 0);
        let p = text(LUT_FILE[k], "");
        p.set_str(kOfxParamPropStringMode, kOfxParamStringIsFilePath);
        p.set_int(kOfxParamPropAnimates, 0);
        let p = hidden(fx.define_param(
            kOfxParamTypeChoice,
            &c(LUT_INTERPOLATION[k]),
            &label(LUT_INTERPOLATION[k]),
        ));
        p.set_strs(
            kOfxParamPropChoiceOption,
            &[c"Linear (good)", c"Tetrahedral (best)"],
        );
        p.set_int(kOfxParamPropDefault, 0);
        p.set_int(kOfxParamPropAnimates, 0);
        number(LUT_GAIN[k], [0.0, 1.0], [0.0, 1.0], 1.0);
    }
    // Kept as VEGAS's effect keeps them; BananaV's color handling is the
    // project's.
    let p = hidden(fx.define_param(kOfxParamTypeInteger, c"autoAdjust", c"Auto Adjust"));
    p.set_int(kOfxParamPropDefault, 0);
    let p = hidden(fx.define_param(
        kOfxParamTypeInteger,
        c"cspaceVersion",
        c"Color Space Version",
    ));
    p.set_int(kOfxParamPropDefault, -2);
    text("cspaceName", "").set_int(kOfxParamPropAnimates, 0);
    text("outputTransformCspaceName", "").set_int(kOfxParamPropAnimates, 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accel::Value;

    fn values(pairs: &[(&str, Value)]) -> Vec<(String, Value)> {
        pairs
            .iter()
            .map(|(n, v)| (n.to_string(), v.clone()))
            .collect()
    }

    fn num(v: f64) -> Value {
        Value::Num(vec![v])
    }

    #[test]
    fn the_wheels_points_give_their_colors_as_vegas() {
        let lift = WHEEL_RANGES[0];
        // Up: red, toward its top.
        let v = wheel_values(0.0, 51.0, lift);
        assert!(v[0] > 0.99 && v[1] < 0.0 && v[2] < 0.0, "{v:?}");
        // The middle: the rest.
        assert_eq!(wheel_values(0.0, 0.0, lift), [0.0; 3]);
        // Down at the left: green; down at the right: blue.
        let a = 230f64.to_radians();
        let v = wheel_values(51.0 * a.cos(), 51.0 * a.sin(), lift);
        assert!(v[1] > 0.99, "{v:?}");
        let a = 335f64.to_radians();
        let v = wheel_values(51.0 * a.cos(), 51.0 * a.sin(), lift);
        assert!(v[2] > 0.99, "{v:?}");
        // Gamma's top is 5, its bottom 0.
        let v = wheel_values(0.0, 51.0, WHEEL_RANGES[1]);
        assert!((v[0] - 5.0).abs() < 0.05 && v[1] < 1.0, "{v:?}");
        // Typed values put the point about where it gives them.
        let [u, w] = wheel_point([0.3, 0.0, 0.0], lift, false);
        assert!(w > 0.0 && u.abs() < w, "{u} {w}");
        let back = wheel_values(u, w, lift);
        assert!((back[0] - 0.3).abs() < 0.05, "{back:?}");
    }

    #[test]
    fn curves_text_round_trips_with_their_ends_and_turns() {
        let points = vec![CurvePoint::new(0.25, 0.4), CurvePoint::new(0.75, 0.6)];
        let text = color_curve_text(&flat_handles(&points));
        let k = grading::parse_curve(&text);
        assert_eq!(k.len(), 4);
        assert_eq!((k[0].x, k[0].y), (0.0, 0.4));
        assert_eq!((k[3].x, k[3].y), (1.0, 0.6));
        // A hue curve keeps its turn: a copy of its last point before 0.
        let text = hue_curve_text(&flat_handles(&points));
        let k = grading::parse_curve(&text);
        assert_eq!(k.len(), 4);
        assert!((k[0].x + 0.25).abs() < 1e-5 && (k[3].x - 1.25).abs() < 1e-5);
        assert_eq!(curve_points(&text, true).len(), 2);
    }

    #[test]
    fn the_settings_read_their_values_and_default_to_nothing() {
        let s = Settings::of(Values(&[]));
        assert!(s.grade().changes_nothing());
        let v = values(&[
            ("liftR", num(0.3)),
            ("gammaY", num(1.2)),
            ("temperatureAdjust", num(2000.0)),
            (
                "curveR",
                Value::Text(
                    "2:0.0:0.08:-0.25:-0.25:0.25:0.25:1.0:1.0:-0.25:-0.25:0.25:0.25:".into(),
                ),
            ),
        ]);
        let s = Settings::of(Values(&v));
        assert_eq!(s.params.wheels[0].rgb[0], 0.3);
        assert_eq!(s.params.wheels[1].y, 1.2);
        assert_eq!(s.params.temperature, 2000.0);
        let g = s.grade();
        assert!(!g.changes_nothing());
        // The red curve lifts black.
        let out = g.apply([0.0; 3]);
        assert!(out[0] > 0.05, "{out:?}");
    }

    #[test]
    fn pickers_set_what_vegas_eyedroppers_set() {
        // Lift: each channel between −1 and 1, the color turned over.
        let v = pick(&[], Picker::BlackPoint, &[[0.25, 0.5, 1.0]], false);
        let get = |n: &str| {
            v.iter()
                .find(|(k, _)| k == n)
                .map(|(_, v)| match v {
                    Value::Num(x) => x[0],
                    _ => f64::NAN,
                })
                .unwrap()
        };
        assert!((get("liftR") - 0.5).abs() < 1e-9);
        assert!(get("liftG").abs() < 1e-9);
        assert!((get("liftB") + 1.0).abs() < 1e-9);
        // A grey a white balance tinted made neutral again (warming acts
        // more than cooling: a slight warmth).
        let tinted = values(&[
            ("temperatureAdjust", num(-500.0)),
            ("tintAdjust", num(15.0)),
        ]);
        let warm = Settings::of(Values(&tinted)).grade().apply([0.5; 3]);
        let v = pick(&[], Picker::NeutralTone, &[warm], false);
        let out = Settings::of(Values(&v)).grade().apply(warm);
        assert!(
            (out[0] - out[1]).abs() < 0.01 && (out[1] - out[2]).abs() < 0.01,
            "{out:?} {v:?}"
        );
        // The Color Selector: a red's hue on Hue vs Hue, at the curve's
        // rest (0.5); greys say nothing.
        let v = pick(&[], Picker::Selector(0), &[[0.8, 0.1, 0.1]], false);
        let Some((name, Value::Text(t))) = v.first() else {
            panic!("{v:?}");
        };
        assert_eq!(name, "curveHvH");
        let p = curve_points(t, true);
        assert_eq!(p.len(), 1);
        assert!((p[0].y - 0.5).abs() < 1e-5);
        assert!(pick(&[], Picker::Selector(0), &[[0.5; 3]], false).is_empty());
    }

    #[test]
    fn apple_log_decodes_to_video() {
        let near = |a: [f32; 3], b: f32, e: f32| a.iter().all(|v| (v - b).abs() < e);
        // Apple Log's 18 % grey back to 0.18 light, and to Rec. 709's
        // video of it.
        let grey = 0.08550479 * (0.18f64 + 0.00964052).log2() + 0.69336945;
        let lin = apple_log([grey as f32; 3], false);
        assert!(near(lin, 0.18, 1e-4), "{lin:?}");
        let video = apple_log([grey as f32; 3], true);
        let expect = 1.099 * 0.18f32.powf(0.45) - 0.099;
        assert!(near(video, expect, 1e-4), "{video:?}");
        // As the Camera LUT at half strength: half way there.
        let v = values(&[
            ("lutEnable0", num(1.0)),
            ("lutFilename0", Value::Text(APPLE_LOG_TO_REC709.into())),
            ("lutGain0", num(0.5)),
        ]);
        let out = Settings::of(Values(&v)).grade().apply([grey as f32; 3]);
        assert!(near(out, (grey as f32 + expect) / 2.0, 1e-3), "{out:?}");
    }

    /// The cases VEGAS rendered, in their order: each one's settings by
    /// BananaV's names (LUT files from `BV_GRADING_LUTS`).
    fn rendered_cases(luts: &str) -> Vec<(&'static str, Vec<(String, Value)>)> {
        let t = |s: &str| Value::Text(s.to_string());
        let lut = |file: &str| Value::Text(format!("{luts}/{file}"));
        let case = |name: &'static str, pairs: Vec<(&str, Value)>| {
            let v = pairs.into_iter().map(|(n, v)| (n.to_string(), v)).collect();
            (name, v)
        };
        vec![
            case("none", vec![]),
            case("default", vec![]),
            case(
                "lift",
                vec![
                    ("liftR", num(0.3)),
                    ("liftG", num(-0.2)),
                    ("liftB", num(0.1)),
                    ("liftY", num(0.05)),
                ],
            ),
            case(
                "gamma",
                vec![
                    ("gammaR", num(1.5)),
                    ("gammaG", num(0.7)),
                    ("gammaB", num(2.5)),
                    ("gammaY", num(1.2)),
                ],
            ),
            case(
                "gain",
                vec![
                    ("gainR", num(1.4)),
                    ("gainG", num(0.8)),
                    ("gainB", num(1.1)),
                    ("gainY", num(0.9)),
                ],
            ),
            case(
                "offset",
                vec![
                    ("offsetR", num(0.1)),
                    ("offsetG", num(-0.05)),
                    ("offsetB", num(0.03)),
                    ("offsetY", num(0.02)),
                ],
            ),
            case("rl shadows y", vec![("logWheelShadowsY", num(0.5))]),
            case(
                "rl mixed",
                vec![
                    ("logWheelShadowsR", num(-0.3)),
                    ("logWheelMidtonesG", num(0.4)),
                    ("logWheelHighlightsB", num(-0.5)),
                    ("logWheelOffsetY", num(0.1)),
                    ("logWheelLowThreshold", num(0.2)),
                    ("logWheelHighThreshold", num(0.6)),
                ],
            ),
            case(
                "levels",
                vec![
                    ("inMinAdjust", num(0.1)),
                    ("inMaxAdjust", num(0.9)),
                    ("outMinAdjust", num(0.05)),
                    ("outMaxAdjust", num(0.95)),
                ],
            ),
            case(
                "tone",
                vec![
                    ("exposureLnAdjust", num(1.2)),
                    ("brightnessAdjust", num(0.05)),
                    ("contrastAdjust", num(0.3)),
                    ("contrastCtrAdjust", num(0.4)),
                    ("saturationAdjust", num(1.4)),
                ],
            ),
            case(
                "tone down",
                vec![
                    ("exposureLnAdjust", num(-2.0)),
                    ("contrastAdjust", num(-0.4)),
                    ("saturationAdjust", num(0.5)),
                ],
            ),
            case("linear exposure", vec![("exposureAdjust", num(1.0))]),
            case(
                "white balance up",
                vec![
                    ("temperatureAdjust", num(2000.0)),
                    ("tintAdjust", num(30.0)),
                ],
            ),
            case(
                "white balance down",
                vec![
                    ("temperatureAdjust", num(-3000.0)),
                    ("tintAdjust", num(-50.0)),
                ],
            ),
            case(
                "hsl",
                vec![
                    ("hueAdjust", num(0.1)),
                    ("satAdjust", num(1.3)),
                    ("valAdjust", num(0.9)),
                ],
            ),
            case(
                "color curves",
                vec![
                    (
                        "curveY",
                        t(
                            "3:0.00000:0.00000:-0.25000:-0.25000:0.15000:0.05000:0.50000:0.50000:-0.15000:-0.20000:0.15000:0.20000:1.00000:1.00000:-0.15000:-0.05000:0.25000:0.25000:",
                        ),
                    ),
                    (
                        "curveR",
                        t(
                            "2:0.00000:0.08000:-0.25000:-0.25000:0.25000:0.25000:1.00000:1.00000:-0.25000:-0.25000:0.25000:0.25000:",
                        ),
                    ),
                    (
                        "curveB",
                        t(
                            "2:0.00000:0.00000:-0.25000:-0.25000:0.25000:0.25000:1.00000:0.85000:-0.25000:-0.20000:0.25000:0.25000:",
                        ),
                    ),
                ],
            ),
            case(
                "hsl curves 1",
                vec![
                    (
                        "curveHvH",
                        t(
                            "5:-0.16667:0.50000:-0.12500:0.00000:0.08333:0.00000:0.00000:0.60000:-0.08333:0.00000:0.08333:0.00000:0.16667:0.50000:-0.08333:0.00000:0.12500:0.00000:0.83333:0.50000:-0.12500:0.00000:0.08333:0.00000:1.00000:0.60000:-0.08333:0.00000:0.08333:0.00000",
                        ),
                    ),
                    (
                        "curveHvS",
                        t(
                            "5:-0.50000:0.50000:-0.08333:0.00000:0.12500:0.00000:0.16667:0.50000:-0.12500:0.00000:0.08333:0.00000:0.33333:0.15000:-0.08333:0.00000:0.08333:0.00000:0.50000:0.50000:-0.08333:0.00000:0.12500:0.00000:1.16667:0.50000:-0.12500:0.00000:0.08333:0.00000",
                        ),
                    ),
                ],
            ),
            case(
                "hsl curves 2",
                vec![
                    (
                        "curveHvL",
                        t(
                            "5:-0.16667:0.50000:-0.08333:0.00000:0.12500:0.00000:0.50000:0.50000:-0.12500:0.00000:0.08333:0.00000:0.66667:0.30000:-0.08333:0.00000:0.08333:0.00000:0.83333:0.50000:-0.08333:0.00000:0.12500:0.00000:1.50000:0.50000:-0.12500:0.00000:0.08333:0.00000",
                        ),
                    ),
                    (
                        "curveSvL",
                        t(
                            "3:0.00000:0.50000:0.00000:0.00000:0.12500:0.00000:0.50000:0.60000:-0.12500:0.00000:0.12500:0.00000:1.00000:0.50000:-0.12500:0.00000:0.00000:0.00000:",
                        ),
                    ),
                    (
                        "curveLvS",
                        t(
                            "3:0.00000:0.50000:0.00000:0.00000:0.12500:0.00000:0.80000:0.20000:-0.12500:0.00000:0.10000:0.00000:1.00000:0.50000:-0.10000:0.00000:0.00000:0.00000:",
                        ),
                    ),
                ],
            ),
            case(
                "look lut",
                vec![
                    ("lutEnable1", num(1.0)),
                    ("lutFilename1", lut("Look/24.cube")),
                    ("lutGain1", num(0.7)),
                    ("lutInterpolation1", num(1.0)),
                ],
            ),
            case(
                "camera lut",
                vec![
                    ("lutEnable0", num(1.0)),
                    ("lutFilename0", lut("Camera/S-Log3_to_REC.709.cube")),
                    ("lutGain0", num(1.0)),
                    ("lutInterpolation0", num(0.0)),
                ],
            ),
        ]
    }

    /// The grade against VEGAS's own renders of the same cases, an 8-bit
    /// project with the graphics card on (not in the repository):
    /// `BV_GRADING_INPUT` the picture they started from, `BV_GRADING_RENDERS`
    /// a folder of the frames, three a case in the cases' order (the
    /// middle one measured), `BV_GRADING_LUTS` the LUTs' folder. Prints
    /// each case's largest and mean difference in 8-bit levels, with
    /// VEGAS's 8-bit steps and as BananaV renders (in floating point).
    #[test]
    #[ignore = "needs VEGAS's renders"]
    fn matches_vegas_renders() {
        let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name}"));
        let read = |path: &std::path::Path| {
            let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            bv_png::decode(&bytes).expect("a picture").to_rgba8()
        };
        let input = read(std::path::Path::new(&var("BV_GRADING_INPUT")));
        let mut frames: Vec<_> = std::fs::read_dir(var("BV_GRADING_RENDERS"))
            .expect("the renders")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("png")))
            .collect();
        frames.sort();
        let cases = rendered_cases(&var("BV_GRADING_LUTS"));
        assert!(frames.len() >= cases.len() * 3, "{} frames", frames.len());
        let pixels = input.len() / 4;
        // The largest and the mean difference of `grade` from `rendered`.
        let measure = |grade: Option<&Grade>, rendered: &[u8]| {
            let (mut max, mut sum) = (0u8, 0u64);
            for p in 0..pixels {
                let c = [input[p * 4], input[p * 4 + 1], input[p * 4 + 2]];
                let ours = grade.map_or(c, |g| {
                    g.apply(c.map(|v| f32::from(v) / 255.0))
                        .map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)
                });
                let d = (0..3)
                    .map(|i| ours[i].abs_diff(rendered[p * 4 + i]))
                    .max()
                    .unwrap_or(0);
                max = max.max(d);
                sum += u64::from(d);
            }
            (max, sum as f64 / pixels as f64)
        };
        let (mut steps, mut floats) = (0, 0);
        for (k, (name, values)) in cases.iter().enumerate() {
            let rendered = read(&frames[3 * k + 1]);
            let mut grade = Settings::of(Values(values)).grade();
            let none = *name == "none";
            let (a, am) = measure((!none).then_some(&grade), &rendered);
            grade.quantize = true;
            let (b, bm) = measure((!none).then_some(&grade), &rendered);
            eprintln!(
                "{name:20} 8-bit steps: max {b} mean {bm:.3}   floating point: max {a} mean {am:.3}"
            );
            steps = steps.max(b);
            floats = floats.max(a);
        }
        assert!(steps <= 1 && floats <= 2, "{steps} {floats}");
    }
}
