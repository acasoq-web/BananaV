//! Color Grading: what the Color Grading window works, as VEGAS's Help
//! lays its sections out, in a colorist's order: the Camera LUT (a log
//! recording's decoding: a `.cube` file, or Apple Log's published curve to
//! linear or to Rec. 709), the Color Wheels (Lift, Gamma, Gain and Offset,
//! each a color and a master Y), the RL Color Wheels (Shadows, Midtones
//! and Highlights within two luma thresholds, and an Offset), the
//! Utilities (Levels, Tone, White Balance, HSL), the Curves (Brightness,
//! Red, Green, Blue) and the HSL Curves (Hue vs Hue, Hue vs Saturation,
//! Hue vs Luminance, Saturation vs Saturation, Saturation vs Luminance,
//! Luminance vs Saturation), then the Look LUT; each LUT at its strength.
//! Its parameters are what the window edits.
//!
//! The math is BananaV's (the Help describes what each control does, not
//! how): Lift raises the dark end toward its color (x + lift·(1 − x)),
//! Gain scales, Gamma bends (x^(1/gamma)) and Offset adds, each channel by
//! its number with the master Y; the RL wheels add their colors weighed
//! by where the luma stands against the thresholds (soft edges 0.05 wide);
//! Levels remaps the input's black and white points to the output's;
//! Tone adds brightness, scales contrast about its center and saturation
//! about the luma (BT.709), and moves log footage 0.1 a stop; White
//! Balance scales red and blue (temperature) and green (tint); HSL turns
//! the chroma, scales it and adds to the luma; the curves map the luma
//! (Brightness, the color scaled along) and each channel; the HSL curves
//! move the hue, scale the saturation and the value by curves of the
//! hue (wrapping round red), of the saturation and of the value.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::Arc;

use bv_core::curve::CurvePoint;
use bv_lut::{Interpolation, Lut};
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Fx, Props, is, suites};

pub(crate) const ID: &CStr = c"org.bananav.ColorGrading";

/// The parameters.
pub(crate) const CAMERA_LUT: &CStr = c"cameraLut";
pub(crate) const CAMERA_STRENGTH: &CStr = c"cameraStrength";
pub(crate) const CAMERA_TETRAHEDRAL: &CStr = c"cameraTetrahedral";
pub(crate) const CAMERA_ON: &CStr = c"cameraOn";
pub(crate) const LOOK_LUT: &CStr = c"lookLut";
pub(crate) const LOOK_STRENGTH: &CStr = c"lookStrength";
pub(crate) const LOOK_TETRAHEDRAL: &CStr = c"lookTetrahedral";
pub(crate) const LOOK_ON: &CStr = c"lookOn";
/// The Color Wheels and the RL Color Wheels: a color and a master Y each.
pub(crate) const WHEELS: [(&CStr, &CStr); 4] = [
    (c"lift", c"liftY"),
    (c"gamma", c"gammaY"),
    (c"gain", c"gainY"),
    (c"offset", c"offsetY"),
];
pub(crate) const RL_WHEELS: [(&CStr, &CStr); 4] = [
    (c"shadows", c"shadowsY"),
    (c"midtones", c"midtonesY"),
    (c"highlights", c"highlightsY"),
    (c"rlOffset", c"rlOffsetY"),
];
pub(crate) const LOWER: &CStr = c"lowerThreshold";
pub(crate) const UPPER: &CStr = c"upperThreshold";
pub(crate) const IN_BLACK: &CStr = c"inputBlack";
pub(crate) const IN_WHITE: &CStr = c"inputWhite";
pub(crate) const OUT_MIN: &CStr = c"outputMin";
pub(crate) const OUT_MAX: &CStr = c"outputMax";
pub(crate) const BRIGHTNESS: &CStr = c"brightness";
pub(crate) const CONTRAST: &CStr = c"contrast";
pub(crate) const CONTRAST_CENTER: &CStr = c"contrastCenter";
pub(crate) const SATURATION: &CStr = c"saturation";
pub(crate) const LOG_EXPOSURE: &CStr = c"logExposure";
pub(crate) const TEMPERATURE: &CStr = c"temperature";
pub(crate) const TINT: &CStr = c"tint";
pub(crate) const HUE: &CStr = c"hue";
pub(crate) const HSL_SATURATION: &CStr = c"hslSaturation";
pub(crate) const LUMINANCE: &CStr = c"luminance";
pub(crate) const CURVES: &CStr = c"curves";
pub(crate) const HSL_CURVES: &CStr = c"hslCurves";

/// The Curves, in their parameter's order.
pub(crate) const CURVE_NAMES: [&CStr; 4] = [c"Brightness", c"Red", c"Green", c"Blue"];
const CURVE_COLOURS: [[f64; 3]; 4] = [
    [0.85, 0.85, 0.85],
    [0.80, 0.55, 0.55],
    [0.50, 0.75, 0.50],
    [0.56, 0.56, 0.85],
];
/// The HSL Curves, in their parameter's order: each maps its input (x,
/// 0–1) to a change (y: 0 none; a hue's −1 to 1 is half a turn either
/// way, a saturation's or a value's −1 to 1 scales it from 0 to 2).
pub(crate) const HSL_CURVE_NAMES: [&CStr; 6] = [
    c"Hue vs Hue",
    c"Hue vs Saturation",
    c"Hue vs Luminance",
    c"Saturation vs Saturation",
    c"Saturation vs Luminance",
    c"Luminance vs Saturation",
];

/// The Camera LUT's curves BananaV knows (the LUT list's own entries):
/// Apple Log decoded to Rec. 709's linear light or to its video.
pub const APPLE_LOG_TO_LINEAR: &str = "builtin:apple-log-to-linear";
pub const APPLE_LOG_TO_REC709: &str = "builtin:apple-log-to-rec709";

/// How far a stop moves log footage (Log. Exposure).
const LOG_STOP: f32 = 0.1;
/// The RL thresholds' soft edges (half their width).
const SOFT: f32 = 0.05;
/// How far White Balance's temperature and tint scale their channels.
const BALANCE: f32 = 0.25;
/// The entries of each curve's table.
const TABLE: usize = 1024;

/// A LUT row: its file (or a built-in curve), strength, interpolation and
/// whether it applies (LUT Selection).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LutRow {
    pub file: String,
    pub strength: f64,
    pub tetrahedral: bool,
    pub on: bool,
}

impl LutRow {
    fn active(&self) -> bool {
        self.on && self.strength > 0.0 && !self.file.trim().is_empty()
    }
}

/// The settings at a time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    pub camera: LutRow,
    pub look: LutRow,
    /// Lift, Gamma, Gain, Offset: red, green, blue and the master Y.
    pub wheels: [[f64; 4]; 4],
    /// Shadows, Midtones, Highlights, Offset.
    pub rl: [[f64; 4]; 4],
    pub thresholds: [f64; 2],
    /// Input black and white, output minimum and maximum.
    pub levels: [f64; 4],
    /// Brightness, contrast, contrast center, saturation, log exposure.
    pub tone: [f64; 5],
    /// Temperature, tint.
    pub balance: [f64; 2],
    /// Hue (degrees), saturation, luminance.
    pub hsl: [f64; 3],
    pub curves: Vec<Vec<CurvePoint>>,
    pub hsl_curves: Vec<Vec<CurvePoint>>,
}

/// The wheels' neutral values: Gamma's and Gain's multiply.
const WHEEL_NEUTRAL: [f64; 4] = [0.0, 1.0, 1.0, 0.0];

impl Settings {
    /// The settings in `v`, a time's values as the host gives them.
    pub(crate) fn of(v: Values<'_>) -> Settings {
        let row = |file, strength, tetra, on| LutRow {
            file: v.text(file).unwrap_or_default().to_string(),
            strength: v.f64(strength, 1.0).clamp(0.0, 1.0),
            tetrahedral: v.int(tetra, 1) != 0,
            on: v.int(on, 1) != 0,
        };
        let wheel = |(rgb, y): (&CStr, &CStr), n: f64| {
            let [r, g, b] = v.array(rgb, [n; 3]);
            [r, g, b, v.f64(y, n)]
        };
        Settings {
            camera: row(CAMERA_LUT, CAMERA_STRENGTH, CAMERA_TETRAHEDRAL, CAMERA_ON),
            look: row(LOOK_LUT, LOOK_STRENGTH, LOOK_TETRAHEDRAL, LOOK_ON),
            wheels: std::array::from_fn(|i| wheel(WHEELS[i], WHEEL_NEUTRAL[i])),
            rl: std::array::from_fn(|i| wheel(RL_WHEELS[i], 0.0)),
            thresholds: [v.f64(LOWER, 0.25), v.f64(UPPER, 0.75)],
            levels: [
                v.f64(IN_BLACK, 0.0),
                v.f64(IN_WHITE, 1.0),
                v.f64(OUT_MIN, 0.0),
                v.f64(OUT_MAX, 1.0),
            ],
            tone: [
                v.f64(BRIGHTNESS, 0.0),
                v.f64(CONTRAST, 0.0),
                v.f64(CONTRAST_CENTER, 0.5),
                v.f64(SATURATION, 1.0),
                v.f64(LOG_EXPOSURE, 0.0),
            ],
            balance: [v.f64(TEMPERATURE, 0.0), v.f64(TINT, 0.0)],
            hsl: [
                v.f64(HUE, 0.0),
                v.f64(HSL_SATURATION, 1.0),
                v.f64(LUMINANCE, 0.0),
            ],
            curves: v.curves(CURVES).map(<[_]>::to_vec).unwrap_or_default(),
            hsl_curves: v.curves(HSL_CURVES).map(<[_]>::to_vec).unwrap_or_default(),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        let row = |file, strength, tetra, on| LutRow {
            file: fx.param(file).text_at(time),
            strength: fx.param(strength).f64_at(time).clamp(0.0, 1.0),
            tetrahedral: fx.param(tetra).int_at(time) != 0,
            on: fx.param(on).int_at(time) != 0,
        };
        let wheel = |(rgb, y): (&CStr, &CStr)| {
            let [r, g, b] = fx.param(rgb).xyz_at(time);
            [r, g, b, fx.param(y).f64_at(time)]
        };
        let f = |name| fx.param(name).f64_at(time);
        // The host's curves, sampled where their points are (their
        // handles kept): what `curve` evaluates back.
        let sampled = |name, n: usize| -> Vec<Vec<CurvePoint>> {
            let p = fx.param(name);
            (0..n)
                .map(|c| {
                    (0..=64)
                        .map(|i| {
                            let x = i as f64 / 64.0;
                            CurvePoint {
                                x,
                                y: p.curve_at(c as c_int, time, x),
                                handles: None,
                            }
                        })
                        .collect()
                })
                .collect()
        };
        Settings {
            camera: row(CAMERA_LUT, CAMERA_STRENGTH, CAMERA_TETRAHEDRAL, CAMERA_ON),
            look: row(LOOK_LUT, LOOK_STRENGTH, LOOK_TETRAHEDRAL, LOOK_ON),
            wheels: std::array::from_fn(|i| wheel(WHEELS[i])),
            rl: std::array::from_fn(|i| wheel(RL_WHEELS[i])),
            thresholds: [f(LOWER), f(UPPER)],
            levels: [f(IN_BLACK), f(IN_WHITE), f(OUT_MIN), f(OUT_MAX)],
            tone: [
                f(BRIGHTNESS),
                f(CONTRAST),
                f(CONTRAST_CENTER),
                f(SATURATION),
                f(LOG_EXPOSURE),
            ],
            balance: [f(TEMPERATURE), f(TINT)],
            hsl: [f(HUE), f(HSL_SATURATION), f(LUMINANCE)],
            curves: sampled(CURVES, CURVE_NAMES.len()),
            hsl_curves: sampled(HSL_CURVES, HSL_CURVE_NAMES.len()),
        }
    }

    /// The curves as the host hands them to the render (`Settings::at`):
    /// each sampled where its points are by the host's own evaluation, so
    /// that the graphics card, which reads the values, draws what the
    /// render draws.
    pub(crate) fn sampled(mut self) -> Settings {
        let sample = |curves: &[Vec<CurvePoint>], n: usize| -> Vec<Vec<CurvePoint>> {
            (0..n)
                .map(|c| {
                    let points = curves.get(c).map_or(&[][..], Vec::as_slice);
                    (0..=64)
                        .map(|i| {
                            let x = i as f64 / 64.0;
                            CurvePoint {
                                x,
                                y: bv_core::curve::curve_at(points, x),
                                handles: None,
                            }
                        })
                        .collect()
                })
                .collect()
        };
        self.curves = sample(&self.curves, CURVE_NAMES.len());
        self.hsl_curves = sample(&self.hsl_curves, HSL_CURVE_NAMES.len());
        self
    }

    /// The default settings: nothing changes.
    pub(crate) fn neutral() -> Settings {
        Settings::of(Values(&[]))
    }

    /// Whether the settings change nothing.
    pub(crate) fn changes_nothing(&self) -> bool {
        let n = Settings::neutral();
        !self.camera.active()
            && !self.look.active()
            && self.wheels == n.wheels
            && self.rl == n.rl
            && self.levels == n.levels
            && self.tone[0] == 0.0
            && self.tone[1] == 0.0
            && self.tone[3] == 1.0
            && self.tone[4] == 0.0
            && self.balance == n.balance
            && self.hsl == n.hsl
            && self
                .curves
                .iter()
                .enumerate()
                .all(|(c, p)| curve_is_neutral(p, c, false))
            && self.hsl_curves.iter().all(|p| curve_is_neutral(p, 0, true))
    }

    /// Ready to color pixels: the LUTs read, the curves as tables.
    pub(crate) fn grade(&self) -> Grade {
        let table = |points: Option<&Vec<CurvePoint>>, neutral: fn(f64) -> f64, wrap: bool| {
            let points = points.filter(|p| !p.is_empty());
            (0..TABLE)
                .map(|i| {
                    let x = i as f64 / (TABLE - 1) as f64;
                    match points {
                        Some(p) if wrap => wrapped_at(p, x) as f32,
                        Some(p) => bv_core::curve::curve_at(p, x) as f32,
                        None => neutral(x) as f32,
                    }
                })
                .collect::<Vec<f32>>()
        };
        let straight = |x: f64| x;
        let flat = |_: f64| 0.0;
        Grade {
            settings: self.clone(),
            camera: lut_of(&self.camera),
            look: lut_of(&self.look),
            curves: std::array::from_fn(|c| table(self.curves.get(c), straight, false)),
            hsl_curves: std::array::from_fn(|c| table(self.hsl_curves.get(c), flat, c < 3)),
        }
    }
}

/// Whether curve `c`'s points change nothing: none, or straight (the
/// Curves) or flat at 0 (the HSL Curves).
fn curve_is_neutral(points: &[CurvePoint], _c: usize, flat: bool) -> bool {
    if points.is_empty() {
        return true;
    }
    (0..=16).all(|i| {
        let x = i as f64 / 16.0;
        let y = bv_core::curve::curve_at(points, x);
        if flat {
            y.abs() < 1e-6
        } else {
            (y - x).abs() < 1e-6
        }
    })
}

/// A hue curve at `x`: its points repeated a turn before and after, so it
/// wraps round red.
fn wrapped_at(points: &[CurvePoint], x: f64) -> f64 {
    let mut all = Vec::with_capacity(points.len() * 3);
    for shift in [-1.0, 0.0, 1.0] {
        all.extend(points.iter().map(|p| CurvePoint {
            x: p.x + shift,
            ..*p
        }));
    }
    // A point at 1 is the one at 0 a turn on: once.
    all.sort_by(|a, b| a.x.total_cmp(&b.x));
    all.dedup_by(|a, b| (a.x - b.x).abs() < 1e-9);
    bv_core::curve::curve_at(&all, x)
}

/// A LUT row's table or curve.
#[derive(Clone)]
pub(crate) enum Table {
    Cube(Arc<Lut>, Interpolation),
    AppleLog { rec709: bool },
}

impl Table {
    fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        match self {
            Table::Cube(lut, how) => lut.apply(c, *how),
            Table::AppleLog { rec709 } => apple_log(c, *rec709),
        }
    }
}

fn lut_of(row: &LutRow) -> Option<(Table, f32)> {
    if !row.active() {
        return None;
    }
    let table = match row.file.trim() {
        APPLE_LOG_TO_LINEAR => Table::AppleLog { rec709: false },
        APPLE_LOG_TO_REC709 => Table::AppleLog { rec709: true },
        file => {
            let how = if row.tetrahedral {
                Interpolation::Tetrahedral
            } else {
                Interpolation::Linear
            };
            Table::Cube(crate::lut_filter::table(file)?, how)
        }
    };
    Some((table, row.strength as f32))
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
        let x = x as f64;
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

/// The settings ready to color pixels.
#[derive(Clone)]
pub(crate) struct Grade {
    settings: Settings,
    camera: Option<(Table, f32)>,
    look: Option<(Table, f32)>,
    curves: [Vec<f32>; 4],
    hsl_curves: [Vec<f32>; 6],
}

/// BT.709's luma.
fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A table's value at `x` (0–1), between its entries; past its ends
/// along their slope (what passes white or black keeps going: a straight
/// curve changes nothing there).
fn at(t: &[f32], x: f32) -> f32 {
    let n = t.len() - 1;
    if x < 0.0 {
        return t[0] + x * (t[1] - t[0]) * n as f32;
    }
    if x > 1.0 {
        return t[n] + (x - 1.0) * (t[n] - t[n - 1]) * n as f32;
    }
    within(t, x)
}

/// A table's value at `x`, held within 0–1.
fn within(t: &[f32], x: f32) -> f32 {
    let f = x.clamp(0.0, 1.0) * (t.len() - 1) as f32;
    let i = (f as usize).min(t.len() - 2);
    let k = f - i as f32;
    t[i] + (t[i + 1] - t[i]) * k
}

/// HSV's hue (0–1), saturation and value.
fn hsv([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    if d <= 1e-7 || max <= 0.0 {
        return [0.0, 0.0, max];
    }
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h / 6.0, d / max, max]
}

fn from_hsv([h, s, v]: [f32; 3]) -> [f32; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = v - c;
    let [r, g, b] = match h as u32 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    [r + m, g + m, b + m]
}

/// The grade's steps, in their order: a picker reads a color where its
/// step begins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Step {
    CameraLut,
    Wheels,
    RlWheels,
    LevelsAndTone,
    Balance,
    Hsl,
    Curves,
    HslCurves,
    LookLut,
}

impl Step {
    const ALL: [Step; 9] = [
        Step::CameraLut,
        Step::Wheels,
        Step::RlWheels,
        Step::LevelsAndTone,
        Step::Balance,
        Step::Hsl,
        Step::Curves,
        Step::HslCurves,
        Step::LookLut,
    ];
}

impl Grade {
    /// What the card draws (`accel::GradePlan`).
    pub(crate) fn plan(&self) -> crate::accel::GradePlan {
        use crate::accel::GradeLut;
        let s = &self.settings;
        let f = |v: f64| v as f32;
        let on = |b: bool| f32::from(u8::from(b));
        let lut = |l: &Option<(Table, f32)>| {
            l.as_ref().map(|(t, k)| match t {
                Table::Cube(lut, how) => GradeLut::Cube {
                    lut: lut.clone(),
                    tetrahedral: *how == Interpolation::Tetrahedral,
                    strength: *k,
                },
                Table::AppleLog { rec709 } => GradeLut::AppleLog {
                    video: *rec709,
                    strength: *k,
                },
            })
        };
        let levels = s.levels.map(f);
        let mut steps = vec![[0.0f32; 4]; 16];
        for (i, w) in s.wheels.iter().enumerate() {
            steps[i] = w.map(f);
        }
        for (i, w) in s.rl.iter().enumerate() {
            steps[4 + i] = w.map(f);
        }
        steps[8] = [
            f(s.thresholds[0]),
            f(s.thresholds[1]),
            f(s.balance[0]),
            f(s.balance[1]),
        ];
        steps[9] = levels;
        steps[10] = [f(s.tone[0]), f(s.tone[1]), f(s.tone[2]), f(s.tone[3])];
        steps[11] = [f(s.tone[4]), f(s.hsl[0]), f(s.hsl[1]), f(s.hsl[2])];
        steps[12] = [
            on(s.rl.iter().any(|w| w.iter().any(|v| *v != 0.0))),
            on(self.hsl_curves.iter().any(|t| t.iter().any(|v| *v != 0.0))),
            on(levels != [0.0, 1.0, 0.0, 1.0]),
            0.0,
        ];
        let (c, h) = (&self.curves, &self.hsl_curves);
        steps.extend((0..TABLE).map(|k| [c[0][k], c[1][k], c[2][k], c[3][k]]));
        steps.extend((0..TABLE).map(|k| [h[0][k], h[1][k], h[2][k], h[3][k]]));
        steps.extend((0..TABLE).map(|k| [h[4][k], h[5][k], 0.0, 0.0]));
        crate::accel::GradePlan {
            camera: lut(&self.camera),
            steps,
            entries: TABLE as u32,
            look: lut(&self.look),
        }
    }

    /// A color through the grade.
    pub(crate) fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        Step::ALL.into_iter().fold(c, |c, step| self.step(step, c))
    }

    /// A color through the steps before `end`.
    fn before(&self, end: Step, c: [f32; 3]) -> [f32; 3] {
        Step::ALL
            .into_iter()
            .take_while(|step| *step < end)
            .fold(c, |c, step| self.step(step, c))
    }

    /// A color through one step.
    fn step(&self, step: Step, mut c: [f32; 3]) -> [f32; 3] {
        let s = &self.settings;
        match step {
            Step::CameraLut | Step::LookLut => {
                let lut = if step == Step::CameraLut {
                    &self.camera
                } else {
                    &self.look
                };
                if let Some((t, k)) = lut {
                    let out = t.apply(c);
                    c = std::array::from_fn(|i| c[i] + (out[i] - c[i]) * k);
                }
            }
            // Color Wheels: Lift, Gain, Gamma, Offset.
            Step::Wheels => {
                let w = s.wheels.map(|w| w.map(|v| v as f32));
                for (i, x) in c.iter_mut().enumerate() {
                    let lift = w[0][i] + w[0][3];
                    let gamma = (w[1][i] * w[1][3]).max(0.01);
                    let gain = w[2][i] * w[2][3];
                    let offset = w[3][i] + w[3][3];
                    let mut v = *x + lift * (1.0 - *x);
                    v *= gain;
                    if v > 0.0 && gamma != 1.0 {
                        v = v.powf(1.0 / gamma);
                    }
                    *x = v + offset;
                }
            }
            // RL Color Wheels: by where the luma stands.
            Step::RlWheels => {
                if s.rl.iter().any(|w| w.iter().any(|v| *v != 0.0)) {
                    let [lo, hi] = s.thresholds.map(|t| t as f32);
                    let y = luma(c);
                    let shadows = 1.0 - smoothstep(lo - SOFT, lo + SOFT, y);
                    let highlights = smoothstep(hi - SOFT, hi + SOFT, y);
                    let midtones = (1.0 - shadows - highlights).max(0.0);
                    let r = s.rl.map(|w| w.map(|v| v as f32));
                    for (i, x) in c.iter_mut().enumerate() {
                        *x += shadows * (r[0][i] + r[0][3])
                            + midtones * (r[1][i] + r[1][3])
                            + highlights * (r[2][i] + r[2][3])
                            + r[3][i]
                            + r[3][3];
                    }
                }
            }
            Step::LevelsAndTone => {
                let [in_black, in_white, out_min, out_max] = s.levels.map(|v| v as f32);
                if [in_black, in_white, out_min, out_max] != [0.0, 1.0, 0.0, 1.0] {
                    let span = (in_white - in_black).max(1e-4);
                    c = c.map(|x| out_min + (x - in_black) / span * (out_max - out_min));
                }
                let [brightness, contrast, center, saturation, log_exposure] =
                    s.tone.map(|v| v as f32);
                c = c.map(|x| {
                    (x + brightness + log_exposure * LOG_STOP - center) * (1.0 + contrast) + center
                });
                if saturation != 1.0 {
                    let y = luma(c);
                    c = c.map(|x| y + (x - y) * saturation);
                }
            }
            // White Balance: warmer to the left, greener to the right.
            Step::Balance => {
                let [temperature, tint] = s.balance.map(|v| v as f32);
                if temperature != 0.0 || tint != 0.0 {
                    c[0] *= 1.0 - BALANCE * temperature;
                    c[2] *= 1.0 + BALANCE * temperature;
                    c[1] *= 1.0 + BALANCE * tint;
                }
            }
            // HSL: the chroma turned and scaled about the luma, then the
            // luma moved.
            Step::Hsl => {
                let [hue, hsl_saturation, luminance] = s.hsl.map(|v| v as f32);
                if hue != 0.0 || hsl_saturation != 1.0 || luminance != 0.0 {
                    let y = luma(c);
                    let (cb, cr) = (c[2] - y, c[0] - y);
                    let (sin, cos) = hue.to_radians().sin_cos();
                    let (cb, cr) = (
                        (cb * cos - cr * sin) * hsl_saturation,
                        (cb * sin + cr * cos) * hsl_saturation,
                    );
                    let y = y + luminance;
                    let r = y + cr;
                    let b = y + cb;
                    let g = (y - 0.2126 * r - 0.0722 * b) / 0.7152;
                    c = [r, g, b];
                }
            }
            // Curves: the luma through Brightness (the color scaled along),
            // then each channel.
            Step::Curves => {
                let y = luma(c);
                let mapped = at(&self.curves[0], y);
                if y > 1e-5 {
                    let k = mapped / y;
                    c = c.map(|x| x * k);
                } else {
                    c = c.map(|x| x + (mapped - y));
                }
                for (i, x) in c.iter_mut().enumerate() {
                    *x = at(&self.curves[i + 1], *x);
                }
            }
            Step::HslCurves => {
                if self.hsl_curves.iter().any(|t| t.iter().any(|v| *v != 0.0)) {
                    let [h, sat, v] = hsv(c.map(|x| x.max(0.0)));
                    let t = &self.hsl_curves;
                    let h2 = h + 0.5 * within(&t[0], h);
                    let sat2 = sat
                        * (1.0 + within(&t[1], h))
                        * (1.0 + within(&t[3], sat))
                        * (1.0 + within(&t[5], v));
                    let v2 = v * (1.0 + within(&t[2], h) + within(&t[4], sat));
                    c = from_hsv([h2, sat2.max(0.0), v2.max(0.0)]);
                }
            }
        }
        c
    }
}

// The window's pickers.

/// The Color Grading window's pickers: what a color picked on the preview
/// (the effect's input there) sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picker {
    /// Lift's Specify a Black Point: the color becomes black.
    BlackPoint,
    /// Gain's Specify a White Point: the color becomes white.
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
/// effect's `values` (those not given at their defaults): the parameters'
/// new values by name. Nothing when the colors say nothing (greys have no
/// hue; black takes no white balance).
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
    let grade = settings.grade();
    let n = colors.len() as f64;
    let mean: [f32; 3] =
        std::array::from_fn(|i| (colors.iter().map(|c| c[i] as f64).sum::<f64>() / n) as f32);
    let named = |name: &CStr| name.to_string_lossy().into_owned();
    match picker {
        // Lift takes the color, where the wheels begin, down to 0.
        Picker::BlackPoint => {
            let c = grade.before(Step::Wheels, mean);
            let master = settings.wheels[0][3];
            let lift = c.map(|x| {
                let x = x as f64;
                let whole = if x < 0.999 { -x / (1.0 - x) } else { -1.0 };
                (whole - master).clamp(-1.0, 1.0)
            });
            vec![(named(WHEELS[0].0), Value::Num(lift.to_vec()))]
        }
        // Gain takes it, lifted, up to 1.
        Picker::WhitePoint => {
            let c = grade.before(Step::Wheels, mean);
            let [lift, _, gain, _] = settings.wheels;
            let gain: [f64; 3] = std::array::from_fn(|i| {
                let x = c[i] as f64;
                let v = (x + (lift[i] + lift[3]) * (1.0 - x)) * gain[3];
                (1.0 / v.max(0.25)).min(4.0)
            });
            vec![(named(WHEELS[2].0), Value::Num(gain.to_vec()))]
        }
        // The temperature evens red and blue out, the tint green with them.
        Picker::NeutralTone => {
            let [r, g, b] = grade.before(Step::Balance, mean).map(|x| x.max(0.0) as f64);
            if r + b < 1e-4 || g < 1e-4 {
                return Vec::new();
            }
            let k = BALANCE as f64;
            let temperature = ((r - b) / (k * (r + b))).clamp(-1.0, 1.0);
            let grey = r * (1.0 - k * temperature);
            let tint = ((grey / g - 1.0) / k).clamp(-1.0, 1.0);
            vec![
                (named(TEMPERATURE), Value::Num(vec![temperature])),
                (named(TINT), Value::Num(vec![tint])),
            ]
        }
        Picker::Selector(n) => {
            let n = n.min(HSL_CURVE_NAMES.len() - 1);
            let at: Vec<[f32; 3]> = colors
                .iter()
                .map(|&c| hsv(grade.before(Step::HslCurves, c).map(|x| x.max(0.0))))
                .collect();
            let xs = match n {
                0..=2 => hue_points(&at, region),
                3 | 4 => spread_points(at.iter().map(|c| c[1]), region),
                _ => spread_points(at.iter().map(|c| c[2]), region),
            };
            if xs.is_empty() {
                return Vec::new();
            }
            let mut curves = settings.hsl_curves.clone();
            curves.resize(HSL_CURVE_NAMES.len(), Vec::new());
            let curve = &mut curves[n];
            for x in xs {
                if curve.iter().any(|p| (p.x - x).abs() < 0.004) {
                    continue;
                }
                // Where the curve is already: it keeps its shape.
                let y = if n < 3 {
                    wrapped_at(curve, x)
                } else {
                    bv_core::curve::curve_at(curve, x)
                };
                curve.push(CurvePoint {
                    x,
                    y,
                    handles: None,
                });
                curve.sort_by(|a, b| a.x.total_cmp(&b.x));
            }
            vec![(named(HSL_CURVES), Value::Curves(curves))]
        }
    }
}

/// The hues (a turn's 0–1) a Color Selector puts points at, from colors'
/// HSV: their mean hue (the saturated weighing more, greys none) and for a
/// region its two ends round it.
fn hue_points(colors: &[[f32; 3]], region: bool) -> Vec<f64> {
    use std::f64::consts::TAU;
    let (mut x, mut y) = (0.0f64, 0.0f64);
    let mut hues = Vec::new();
    for &[h, s, _] in colors {
        if s < 0.02 {
            continue;
        }
        let (sin, cos) = (h as f64 * TAU).sin_cos();
        x += s as f64 * cos;
        y += s as f64 * sin;
        hues.push(h as f64);
    }
    if hues.is_empty() || x.hypot(y) < 1e-9 {
        return Vec::new();
    }
    let mean = (y.atan2(x) / TAU).rem_euclid(1.0);
    if !region {
        return vec![mean];
    }
    let off = |h: f64| (h - mean + 0.5).rem_euclid(1.0) - 0.5;
    let lo = hues.iter().map(|&h| off(h)).fold(0.0, f64::min);
    let hi = hues.iter().map(|&h| off(h)).fold(0.0, f64::max);
    if hi - lo < 0.01 {
        return vec![mean];
    }
    vec![
        (mean + lo).rem_euclid(1.0),
        mean,
        (mean + hi).rem_euclid(1.0),
    ]
}

/// The saturations or values a Color Selector puts points at: their mean,
/// and for a region the lowest and the highest.
fn spread_points(values: impl Iterator<Item = f32>, region: bool) -> Vec<f64> {
    let v: Vec<f64> = values.map(|v| (v as f64).clamp(0.0, 1.0)).collect();
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

/// The Color Grading's grade with `values` (the parameters set; the
/// others at their defaults) as a `.cube` 3D table `size` points a side:
/// Export LUT.
pub fn cube(values: &[(String, crate::accel::Value)], size: usize) -> String {
    let size = size.clamp(2, 129);
    let grade = Settings::of(Values(values)).grade();
    let mut out = String::with_capacity(size * size * size * 30 + 100);
    out.push_str(
        "TITLE \"BananaV Color Grading\"
",
    );
    out.push_str(&format!(
        "LUT_3D_SIZE {size}
"
    ));
    out.push_str(
        "DOMAIN_MIN 0.0 0.0 0.0
DOMAIN_MAX 1.0 1.0 1.0
",
    );
    let step = 1.0 / (size - 1) as f32;
    // Red changes fastest, as the format has it.
    for b in 0..size {
        for g in 0..size {
            for r in 0..size {
                let c = grade.apply([r as f32 * step, g as f32 * step, b as f32 * step]);
                out.push_str(&format!(
                    "{:.6} {:.6} {:.6}
",
                    c[0], c[1], c[2]
                ));
            }
        }
    }
    out
}

/// The plug-in's entry point.
pub(crate) unsafe extern "C" fn main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    let Some(s) = suites() else {
        return kOfxStatErrMissingHostFeature;
    };
    let fx = Fx(handle as OfxImageEffectHandle);
    let (args, out) = (Props(in_args), Props(out_args));
    if is(action, kOfxActionDescribe) {
        if s.parametric.is_none() {
            return kOfxStatErrMissingHostFeature;
        }
        describe(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        describe_in_context(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        let grade = Settings::at(fx, args.dbl(kOfxPropTime, 0)).grade();
        crate::colors::render(fx, args, &|c| grade.apply(c))
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        if Settings::at(fx, args.dbl(kOfxPropTime, 0)).changes_nothing() {
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

fn describe_in_context(fx: Fx) {
    let comps = [
        kOfxImageComponentRGBA,
        kOfxImageComponentRGB,
        kOfxImageComponentAlpha,
    ];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);

    let number = |name: &CStr, label: &CStr, [min, max, default]: [f64; 3]| {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, default);
        p.set_dbl(kOfxParamPropMin, min);
        p.set_dbl(kOfxParamPropMax, max);
        p.set_dbl(kOfxParamPropDisplayMin, min);
        p.set_dbl(kOfxParamPropDisplayMax, max);
        p.set_dbl(kOfxParamPropIncrement, 0.001);
        p.set_int(kOfxParamPropDigits, 3);
    };
    let boolean = |name: &CStr, label: &CStr| {
        let p = fx.define_param(kOfxParamTypeBoolean, name, label);
        p.set_int(kOfxParamPropDefault, 1);
    };
    let color3 = |name: &CStr, label: &CStr, [min, max, default]: [f64; 3]| {
        let p = fx.define_param(kOfxParamTypeDouble3D, name, label);
        p.set_dbls(kOfxParamPropDefault, &[default; 3]);
        p.set_dbls(kOfxParamPropMin, &[min; 3]);
        p.set_dbls(kOfxParamPropMax, &[max; 3]);
        p.set_dbls(kOfxParamPropDisplayMin, &[min; 3]);
        p.set_dbls(kOfxParamPropDisplayMax, &[max; 3]);
        p.set_strs(kOfxParamPropDimensionLabel, &[c"R", c"G", c"B"]);
    };
    // The LUTs.
    for (file, strength, tetra, on, label) in [
        (
            CAMERA_LUT,
            CAMERA_STRENGTH,
            CAMERA_TETRAHEDRAL,
            CAMERA_ON,
            c"Camera LUT",
        ),
        (
            LOOK_LUT,
            LOOK_STRENGTH,
            LOOK_TETRAHEDRAL,
            LOOK_ON,
            c"Look LUT",
        ),
    ] {
        let p = fx.define_param(kOfxParamTypeString, file, label);
        p.set_str(kOfxParamPropStringMode, kOfxParamStringIsFilePath);
        p.set_str(kOfxParamPropDefault, c"");
        number(strength, c"Strength", [0.0, 1.0, 1.0]);
        boolean(tetra, c"Tetrahedral Interpolation");
        boolean(on, c"LUT Selection");
    }
    // The wheels: Lift and Offset add, Gamma and Gain multiply.
    let wheel_labels = [c"Lift", c"Gamma", c"Gain", c"Offset"];
    let wheel_ranges = [
        [-1.0, 1.0, 0.0],
        [0.1, 4.0, 1.0],
        [0.0, 4.0, 1.0],
        [-1.0, 1.0, 0.0],
    ];
    for (i, (rgb, y)) in WHEELS.into_iter().enumerate() {
        color3(rgb, wheel_labels[i], wheel_ranges[i]);
        number(y, c"Y", wheel_ranges[i]);
    }
    let rl_labels = [c"Shadows", c"Midtones", c"Highlights", c"RL Offset"];
    for (i, (rgb, y)) in RL_WHEELS.into_iter().enumerate() {
        color3(rgb, rl_labels[i], [-1.0, 1.0, 0.0]);
        number(y, c"Y", [-1.0, 1.0, 0.0]);
    }
    number(LOWER, c"Lower Threshold", [0.0, 1.0, 0.25]);
    number(UPPER, c"Upper Threshold", [0.0, 1.0, 0.75]);
    // The Utilities.
    number(IN_BLACK, c"Input Black", [0.0, 1.0, 0.0]);
    number(IN_WHITE, c"Input White", [0.0, 1.0, 1.0]);
    number(OUT_MIN, c"Output Min", [0.0, 1.0, 0.0]);
    number(OUT_MAX, c"Output Max", [0.0, 1.0, 1.0]);
    number(BRIGHTNESS, c"Brightness", [-1.0, 1.0, 0.0]);
    number(CONTRAST, c"Contrast", [-1.0, 1.0, 0.0]);
    number(CONTRAST_CENTER, c"Contrast Center", [0.0, 1.0, 0.5]);
    number(SATURATION, c"Saturation", [0.0, 2.0, 1.0]);
    number(LOG_EXPOSURE, c"Log. Exposure", [-4.0, 4.0, 0.0]);
    number(TEMPERATURE, c"Temperature", [-1.0, 1.0, 0.0]);
    number(TINT, c"Tint", [-1.0, 1.0, 0.0]);
    number(HUE, c"Hue", [-180.0, 180.0, 0.0]);
    number(HSL_SATURATION, c"Sat", [0.0, 2.0, 1.0]);
    number(LUMINANCE, c"Luminance", [-1.0, 1.0, 0.0]);
    // The Curves: straight; the HSL Curves: flat at 0.
    let p = fx.define_param(kOfxParamTypeParametric, CURVES, c"Color Curves");
    p.set_int(kOfxParamPropParametricDimension, CURVE_NAMES.len() as c_int);
    p.set_strs(kOfxParamPropDimensionLabel, &CURVE_NAMES);
    let colours: Vec<f64> = CURVE_COLOURS.iter().flatten().copied().collect();
    p.set_dbls(kOfxParamPropParametricUIColour, &colours);
    p.set_dbls(kOfxParamPropParametricRange, &[0.0, 1.0]);
    p.set_int(kOfxParamPropAnimates, 1);
    let curves = fx.param(CURVES);
    for c in 0..CURVE_NAMES.len() {
        curves.set_curve(c as c_int, 0.0, &[[0.0, 0.0], [1.0, 1.0]]);
    }
    let p = fx.define_param(kOfxParamTypeParametric, HSL_CURVES, c"HSL Curves");
    p.set_int(
        kOfxParamPropParametricDimension,
        HSL_CURVE_NAMES.len() as c_int,
    );
    p.set_strs(kOfxParamPropDimensionLabel, &HSL_CURVE_NAMES);
    p.set_dbls(kOfxParamPropParametricRange, &[0.0, 1.0]);
    p.set_int(kOfxParamPropAnimates, 1);
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

    fn near(a: [f32; 3], b: [f32; 3], within: f32) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() <= within)
    }

    #[test]
    fn exported_luts_hold_the_grade() {
        let num = |v: &[f64]| Value::Num(v.to_vec());
        let v = values(&[("gain", num(&[0.5, 1.0, 1.0]))]);
        let text = cube(&v, 2);
        let lut = Lut::parse(&text).unwrap();
        let out = lut.apply([1.0, 1.0, 1.0], Interpolation::Linear);
        assert!(near(out, [0.5, 1.0, 1.0], 1e-4), "{out:?}");
        assert!(text.starts_with("TITLE"));
    }

    #[test]
    fn the_default_changes_nothing() {
        let s = Settings::neutral();
        assert!(s.changes_nothing());
        let g = s.grade();
        for c in [
            [0.1, 0.5, 0.9],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.3, 0.3, 0.3],
        ] {
            assert!(near(g.apply(c), c, 1e-3), "{c:?}: {:?}", g.apply(c));
        }
    }

    #[test]
    fn wheels_lift_gain_gamma_and_offset() {
        let num = |v: &[f64]| Value::Num(v.to_vec());
        // Lift: black rises to it, white stays.
        let v = values(&[("lift", num(&[0.2, 0.2, 0.2]))]);
        let g = Settings::of(Values(&v)).grade();
        assert!(near(g.apply([0.0; 3]), [0.2; 3], 1e-3));
        assert!(near(g.apply([1.0; 3]), [1.0; 3], 1e-3));
        // Gain: scales; Gamma: bends the middle (white and black stay).
        let v = values(&[("gain", num(&[0.5, 1.0, 1.0]))]);
        let g = Settings::of(Values(&v)).grade();
        assert!(near(g.apply([0.8; 3]), [0.4, 0.8, 0.8], 1e-3));
        let v = values(&[("gammaY", num(&[2.0]))]);
        let g = Settings::of(Values(&v)).grade();
        assert!(near(g.apply([0.25; 3]), [0.5; 3], 1e-3));
        assert!(near(g.apply([1.0; 3]), [1.0; 3], 1e-3));
        // Offset adds everywhere.
        let v = values(&[("offsetY", num(&[0.1]))]);
        let g = Settings::of(Values(&v)).grade();
        assert!(near(g.apply([0.3; 3]), [0.4; 3], 1e-3));
    }

    #[test]
    fn rl_wheels_keep_to_their_range() {
        let num = |v: &[f64]| Value::Num(v.to_vec());
        let v = values(&[("shadows", num(&[0.0, 0.0, 0.2]))]);
        let g = Settings::of(Values(&v)).grade();
        // Dark grays turn blue, bright ones stay.
        assert!((g.apply([0.05; 3])[2] - 0.25).abs() < 1e-3);
        assert!(near(g.apply([0.9; 3]), [0.9; 3], 1e-3));
        let v = values(&[("highlightsY", num(&[-0.1]))]);
        let g = Settings::of(Values(&v)).grade();
        assert!(near(g.apply([0.9; 3]), [0.8; 3], 1e-3));
        assert!(near(g.apply([0.1; 3]), [0.1; 3], 1e-3));
    }

    #[test]
    fn utilities_levels_tone_balance_and_hsl() {
        let num = |v: &[f64]| Value::Num(v.to_vec());
        // Levels: the input's 0.1–0.9 stretched to the whole range.
        let v = values(&[("inputBlack", num(&[0.1])), ("inputWhite", num(&[0.9]))]);
        let g = Settings::of(Values(&v)).grade();
        assert!(near(g.apply([0.1; 3]), [0.0; 3], 1e-3));
        assert!(near(g.apply([0.9; 3]), [1.0; 3], 1e-3));
        // Contrast about its center: the center stays.
        let v = values(&[("contrast", num(&[0.5]))]);
        let g = Settings::of(Values(&v)).grade();
        assert!(near(g.apply([0.5; 3]), [0.5; 3], 1e-3));
        assert!(near(g.apply([0.7; 3]), [0.8; 3], 1e-3));
        // Saturation 0: the luma's gray.
        let v = values(&[("saturation", num(&[0.0]))]);
        let g = Settings::of(Values(&v)).grade();
        let out = g.apply([0.9, 0.2, 0.1]);
        assert!((out[0] - out[1]).abs() < 1e-4 && (out[1] - out[2]).abs() < 1e-4);
        // Warmer to the left.
        let v = values(&[("temperature", num(&[-0.4]))]);
        let g = Settings::of(Values(&v)).grade();
        let out = g.apply([0.5; 3]);
        assert!(out[0] > 0.5 && out[2] < 0.5);
        // Hue: a half turn takes red toward cyan, the luma kept.
        let v = values(&[("hue", num(&[180.0]))]);
        let g = Settings::of(Values(&v)).grade();
        let red = [0.6, 0.2, 0.2];
        let out = g.apply(red);
        assert!(out[0] < out[1] && out[0] < out[2], "{out:?}");
        assert!((luma(out) - luma(red)).abs() < 1e-4);
    }

    #[test]
    fn curves_and_hsl_curves() {
        let p = |x: f64, y: f64| CurvePoint {
            x,
            y,
            handles: None,
        };
        // The red curve lifted to half at its start.
        let curves = vec![
            vec![p(0.0, 0.0), p(1.0, 1.0)],
            vec![p(0.0, 0.5), p(1.0, 1.0)],
            vec![p(0.0, 0.0), p(1.0, 1.0)],
            vec![p(0.0, 0.0), p(1.0, 1.0)],
        ];
        let v = values(&[("curves", Value::Curves(curves))]);
        let g = Settings::of(Values(&v)).grade();
        assert!(near(g.apply([0.0; 3]), [0.5, 0.0, 0.0], 1e-3));
        // Hue vs Saturation: the reds desaturated (wrapping round red),
        // the blues kept.
        let mut hsl = vec![Vec::new(); 6];
        hsl[1] = vec![p(0.0, -1.0), p(0.2, 0.0), p(0.8, 0.0)];
        let v = values(&[("hslCurves", Value::Curves(hsl))]);
        let s = Settings::of(Values(&v));
        assert!(!s.changes_nothing());
        let g = s.grade();
        let red = g.apply([0.8, 0.1, 0.1]);
        assert!((red[0] - red[1]).abs() < 0.05, "{red:?}");
        assert!(near(g.apply([0.1, 0.1, 0.8]), [0.1, 0.1, 0.8], 1e-3));
    }

    #[test]
    fn pickers_take_the_colors_where_they_should_go() {
        let grade = |v: &[(String, Value)]| Settings::of(Values(v)).grade();
        let set = |v: &mut Vec<(String, Value)>, new: Vec<(String, Value)>| {
            for (name, value) in new {
                v.retain(|(n, _)| *n != name);
                v.push((name, value));
            }
        };
        // A black point: the dark blue-grey picked becomes black.
        let dark = [0.1, 0.08, 0.12];
        let mut v = Vec::new();
        let new = pick(&v, Picker::BlackPoint, &[dark], false);
        set(&mut v, new);
        assert!(near(grade(&v).apply(dark), [0.0; 3], 1e-4));
        // Then a white point, past the lift: the light one becomes white.
        let light = [0.8, 0.85, 0.9];
        let new = pick(&v, Picker::WhitePoint, &[light], false);
        set(&mut v, new);
        let g = grade(&v);
        assert!(near(g.apply(light), [1.0; 3], 1e-3), "{:?}", g.apply(light));
        assert!(near(g.apply(dark), [0.0; 3], 1e-4));
        // A neutral tone: the warm grey becomes a grey.
        let warm = [0.6, 0.5, 0.42];
        let v = pick(&[], Picker::NeutralTone, &[warm], false);
        let out = grade(&v).apply(warm);
        assert!(
            (out[0] - out[1]).abs() < 1e-4 && (out[1] - out[2]).abs() < 1e-4,
            "{out:?}"
        );
        // The Color Selector: a red's hue on Hue vs Hue, flat (the curve
        // keeps its shape); greys say nothing.
        let v = pick(&[], Picker::Selector(0), &[[0.8, 0.1, 0.1]], false);
        let Some((_, Value::Curves(c))) = v.first() else {
            panic!("{v:?}");
        };
        assert_eq!(c[0].len(), 1);
        assert!(c[0][0].x.abs() < 1e-3 || (c[0][0].x - 1.0).abs() < 1e-3);
        assert_eq!(c[0][0].y, 0.0);
        assert!(pick(&[], Picker::Selector(0), &[[0.5; 3]], false).is_empty());
        // A region of oranges to yellows: its two ends and its mean.
        let region: Vec<[f32; 3]> = (0..=10)
            .map(|i| {
                let h = 0.1 + 0.06 * i as f32 / 10.0;
                from_hsv([h, 0.8, 0.9])
            })
            .collect();
        let v = pick(&[], Picker::Selector(1), &region, true);
        let Some((_, Value::Curves(c))) = v.first() else {
            panic!("{v:?}");
        };
        let xs: Vec<f64> = c[1].iter().map(|p| p.x).collect();
        assert_eq!(xs.len(), 3, "{xs:?}");
        assert!(
            (xs[0] - 0.1).abs() < 0.01 && (xs[2] - 0.16).abs() < 0.01,
            "{xs:?}"
        );
        // Added to a curve with a point: on it, where it already is.
        let curves = vec![vec![], vec![CurvePoint::new(0.5, 0.3)]];
        let v = pick(
            &values(&[("hslCurves", Value::Curves(curves))]),
            Picker::Selector(1),
            &[from_hsv([0.2, 0.8, 0.9])],
            false,
        );
        let Some((_, Value::Curves(c))) = v.first() else {
            panic!("{v:?}");
        };
        assert_eq!(c[1].len(), 2);
        assert!(c[1].iter().all(|p| (p.y - 0.3).abs() < 1e-9));
    }

    #[test]
    fn apple_log_decodes_to_video() {
        // Apple Log's 18 % grey (its curve at 0.18) back to 0.18 light,
        // and to Rec. 709's video of it.
        let grey = 0.08550479 * (0.18f64 + 0.00964052).log2() + 0.69336945;
        let lin = apple_log([grey as f32; 3], false);
        assert!(near(lin, [0.18; 3], 1e-4), "{lin:?}");
        let video = apple_log([grey as f32; 3], true);
        let expect = 1.099 * 0.18f32.powf(0.45) - 0.099;
        assert!(near(video, [expect; 3], 1e-4), "{video:?}");
        // As a Camera LUT at half strength: half way there.
        let v = values(&[
            ("cameraLut", Value::Text(APPLE_LOG_TO_REC709.into())),
            ("cameraStrength", Value::Num(vec![0.5])),
        ]);
        let g = Settings::of(Values(&v)).grade();
        let out = g.apply([grey as f32; 3]);
        assert!(
            near(out, [(grey as f32 + expect) / 2.0; 3], 1e-3),
            "{out:?}"
        );
    }
}
