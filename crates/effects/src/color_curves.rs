//! Color Curves: the red, green and blue channels each through a curve,
//! as VEGAS's Color Curves effect does. The curves are an OpenFX
//! parametric parameter — one curve per channel, from 0 to 1 in and out —
//! that the host draws and edits. Auto Modes and Calculate set them from
//! the picture at the cursor: Color Balance (the channels' averages
//! made grey), Color Levels (each channel stretched from its darkest to
//! its brightest), Contrast (the same for the picture's light) or
//! Histogram Equalization (the light spread evenly).
//!
//! The curves work on the values the host hands over: in BananaV's 8-bit
//! projects, as VEGAS's, the encoded ones; colors are taken unpremultiplied
//! and clamped from 0 to 1.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_core::curve::CurvePoint as P;
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.ColorCurves";

/// The parameters.
pub(crate) const CURVES: &CStr = c"curves";
const AUTO_MODE: &CStr = c"autoMode";
const CALCULATE: &CStr = c"calculate";

/// The curves: one per channel, in this order.
const CHANNELS: [&CStr; 3] = [c"Red", c"Green", c"Blue"];

/// How each curve is drawn, red, green and blue from 0 to 1.
const COLOURS: [[f64; 3]; 3] = [[0.80, 0.55, 0.55], [0.50, 0.75, 0.50], [0.56, 0.56, 0.85]];

/// What Calculate does, in the order of the parameter's options.
const MODES: [&CStr; 4] = [
    c"Color Balance",
    c"Color Levels",
    c"Contrast",
    c"Histogram Equalization",
];

/// Color Levels, VEGAS's default.
const DEFAULT_MODE: c_int = 1;

/// The entries of each channel's table, read from its curve per frame.
pub(crate) const TABLE: usize = 1024;

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
        render(fx, args)
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        is_identity(fx, args, out)
    } else if is(action, kOfxActionInstanceChanged) {
        if args.is(kOfxPropName, CALCULATE) {
            calculate(fx, args.dbl(kOfxPropTime, 0));
        }
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // Each output pixel needs the source's pixel there only.
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
    p.set_str(kOfxPropLabel, c"Color Curves");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"Changes the red, green and blue channels through curves.",
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

    let p = fx.define_param(kOfxParamTypeParametric, CURVES, c"Curves");
    p.set_int(kOfxParamPropParametricDimension, CHANNELS.len() as c_int);
    p.set_strs(kOfxParamPropDimensionLabel, &CHANNELS);
    let colours: Vec<f64> = COLOURS.iter().flatten().copied().collect();
    p.set_dbls(kOfxParamPropParametricUIColour, &colours);
    p.set_dbls(kOfxParamPropParametricRange, &[0.0, 1.0]);
    p.set_int(kOfxParamPropAnimates, 1);
    p.set_str(
        kOfxParamPropHint,
        c"The red, green and blue channels each go through their curve.",
    );
    // Each curve starts straight, changing nothing.
    let curves = fx.param(CURVES);
    for c in 0..CHANNELS.len() {
        curves.set_curve(c as c_int, 0.0, &[[0.0, 0.0], [1.0, 1.0]]);
    }

    let p = fx.define_param(kOfxParamTypeChoice, AUTO_MODE, c"Auto Modes");
    p.set_strs(kOfxParamPropChoiceOption, &MODES);
    p.set_int(kOfxParamPropDefault, DEFAULT_MODE);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(
        kOfxParamPropHint,
        c"How Calculate sets the curves from the picture at the cursor.",
    );

    let p = fx.define_param(kOfxParamTypePushButton, CALCULATE, c"Calculate");
    p.set_str(
        kOfxParamPropHint,
        c"Sets the curves from the picture at the cursor, as Auto Modes says.",
    );
}

/// Each channel's curve as a table over 0–1.
struct Tables([Vec<f32>; 3]);

impl Tables {
    fn of(fx: Fx, time: f64) -> Tables {
        let curves = fx.param(CURVES);
        Tables(std::array::from_fn(|c| {
            (0..TABLE)
                .map(|i| {
                    let x = i as f64 / (TABLE - 1) as f64;
                    curves.curve_at(c as c_int, time, x).clamp(0.0, 1.0) as f32
                })
                .collect()
        }))
    }

    /// Channel `c`'s value for `x`.
    fn at(&self, c: usize, x: f32) -> f32 {
        let t = &self.0[c];
        let f = x.clamp(0.0, 1.0) * (TABLE - 1) as f32;
        let i = (f as usize).min(TABLE - 2);
        let k = f - i as f32;
        t[i] + (t[i + 1] - t[i]) * k
    }
}

/// Each channel's table from `v`, a time's values as the host gives them
/// (what the graphics card draws with): the host's curves, as
/// [`Tables::of`] reads them.
pub(crate) fn tables_of(v: Values<'_>) -> [Vec<f32>; 3] {
    let curves = v.curves(CURVES).unwrap_or_default();
    std::array::from_fn(|c| {
        let points = curves.get(c).map_or(&[][..], |p| &p[..]);
        (0..TABLE)
            .map(|i| {
                let x = i as f64 / (TABLE - 1) as f64;
                bv_core::curve::curve_at(points, x).clamp(0.0, 1.0) as f32
            })
            .collect()
    })
}

/// Whether every curve leaves its channel as it is (read at a few
/// places: the handles may bend a curve between its points).
fn is_identity(fx: Fx, args: Props, out: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let curves = fx.param(CURVES);
    let straight = (0..CHANNELS.len()).all(|c| {
        (0..=16).all(|i| {
            let x = i as f64 / 16.0;
            (curves.curve_at(c as c_int, time, x) - x).abs() < 1e-6
        })
    });
    if straight {
        out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

/// Renders: each channel of the source through its curve.
fn render(fx: Fx, args: Props) -> OfxStatus {
    let tables = Tables::of(fx, args.dbl(kOfxPropTime, 0));
    crate::colors::render(fx, args, &|c| {
        [tables.at(0, c[0]), tables.at(1, c[1]), tables.at(2, c[2])]
    })
}

// Calculate.

/// How many bins the picture's histograms have.
const BINS: usize = 256;

/// The picture's histograms: each channel's, then its light's (Rec. 709
/// weights), of its straight colors; and each channel's sum.
#[derive(Debug)]
pub(crate) struct Histograms {
    pub bins: [[u64; BINS]; 4],
    pub sums: [f64; 3],
    pub count: u64,
}

impl Histograms {
    fn new() -> Histograms {
        Histograms {
            bins: [[0; BINS]; 4],
            sums: [0.0; 3],
            count: 0,
        }
    }

    pub(crate) fn add(&mut self, rgb: [f32; 3]) {
        let bin = |v: f32| ((v.clamp(0.0, 1.0) * (BINS - 1) as f32).round() as usize).min(BINS - 1);
        let light = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
        for (c, &v) in rgb.iter().enumerate() {
            self.bins[c][bin(v)] += 1;
            self.sums[c] += v.clamp(0.0, 1.0) as f64;
        }
        self.bins[3][bin(light)] += 1;
        self.count += 1;
    }

    /// Where `part` of histogram `h`'s values lie below (0–1).
    fn percentile(&self, h: usize, part: f64) -> f64 {
        let goal = (part * self.count as f64).ceil() as u64;
        let mut seen = 0;
        for (i, &n) in self.bins[h].iter().enumerate() {
            seen += n;
            if seen >= goal.max(1) {
                return i as f64 / (BINS - 1) as f64;
            }
        }
        1.0
    }

    /// The part of histogram `h`'s values at or below `x`.
    fn below(&self, h: usize, x: f64) -> f64 {
        let last = ((x.clamp(0.0, 1.0) * (BINS - 1) as f64).round() as usize).min(BINS - 1);
        let n: u64 = self.bins[h][..=last].iter().sum();
        n as f64 / self.count.max(1) as f64
    }
}

/// The curves Calculate makes of a picture's histograms in `mode`.
pub(crate) fn auto_curves(h: &Histograms, mode: c_int) -> [Vec<[f64; 2]>; 3] {
    let straight = || vec![[0.0, 0.0], [1.0, 1.0]];
    if h.count == 0 {
        return std::array::from_fn(|_| straight());
    }
    // From its darkest to its brightest (a little of each left out).
    let stretch = |lo: f64, hi: f64| {
        if hi - lo < 0.02 {
            return straight();
        }
        let mut v = Vec::new();
        if lo > 0.002 {
            v.push([0.0, 0.0]);
        }
        let third = (hi - lo) / 3.0;
        v.extend([
            [lo, 0.0],
            [lo + third, 1.0 / 3.0],
            [hi - third, 2.0 / 3.0],
            [hi, 1.0],
        ]);
        if hi < 0.998 {
            v.push([1.0, 1.0]);
        }
        v
    };
    match mode {
        // Color Balance: each channel's average to the three's.
        0 => {
            let n = h.count as f64;
            let means = h.sums.map(|s| s / n);
            let grey = means.iter().sum::<f64>() / 3.0;
            means.map(|m| {
                if (0.02..=0.98).contains(&m) && (0.02..=0.98).contains(&grey) {
                    vec![[0.0, 0.0], [m, grey], [1.0, 1.0]]
                } else {
                    straight()
                }
            })
        }
        // Color Levels: each channel stretched on its own.
        1 => std::array::from_fn(|c| stretch(h.percentile(c, 0.005), h.percentile(c, 0.995))),
        // Contrast: the light stretched, every channel alike.
        2 => {
            let curve = stretch(h.percentile(3, 0.005), h.percentile(3, 0.995));
            std::array::from_fn(|_| curve.clone())
        }
        // Histogram Equalization: the light's spread made even.
        _ => {
            let curve: Vec<[f64; 2]> = (0..=8)
                .map(|k| {
                    let x = k as f64 / 8.0;
                    let y = match k {
                        0 => 0.0,
                        8 => 1.0,
                        _ => h.below(3, x),
                    };
                    [x, y]
                })
                .collect();
            std::array::from_fn(|_| curve.clone())
        }
    }
}

/// Calculate: the picture at `time` read, the curves made of it.
fn calculate(fx: Fx, time: f64) {
    let Some(img) = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time) else {
        if crate::ofx::tracing() {
            eprintln!("bananav color curves: no picture to calculate from at {time}");
        }
        return;
    };
    if img.channels < 3 {
        return;
    }
    let premultiplied = img.channels == 4 && !img.unpremultiplied;
    let mut h = Histograms::new();
    let [x1, y1, x2, y2] = img.bounds;
    // At most about 250 000 pixels, evenly taken.
    let pixels = (x2 - x1).max(0) as i64 * (y2 - y1).max(0) as i64;
    let step = ((pixels as f64 / 250_000.0).sqrt().ceil() as usize).max(1);
    for y in (y1..y2).step_by(step) {
        for x in (x1..x2).step_by(step) {
            if let Some(c) = crate::colors::pixel(&img, x, y)
                .and_then(|px| crate::colors::straight(px, premultiplied))
            {
                h.add(c);
            }
        }
    }
    drop(img);
    let mode = fx.param(AUTO_MODE).int_at(time);
    let curves = fx.param(CURVES);
    for (c, points) in auto_curves(&h, mode).iter().enumerate() {
        curves.set_curve(c as c_int, time, points);
    }
}

// Presets.

/// A curve straight from (0, `y0`) to (1, `y1`).
const fn line(y0: f64, y1: f64) -> [P; 2] {
    [P::new(0.0, y0), P::new(1.0, y1)]
}

const IDENTITY: [P; 2] = line(0.0, 1.0);

/// Bent up toward the top, flat at its end (Warm Colors' red, Cool
/// Colors' blue).
const RAISED: [P; 2] = [
    P::with(0.0, 0.0, [0.0, 0.0], [0.1952, 0.2858]),
    P::with(1.0, 1.0, [-0.157, 0.0], [0.0, 0.0]),
];

/// 8-bit levels, as VEGAS's presets count them.
const fn level(n: f64) -> f64 {
    n / 255.0
}

/// The factory presets, VEGAS's: their names and each channel's curve.
pub(crate) fn presets() -> Vec<Preset> {
    let same = |c: &[P]| [c.to_vec(), c.to_vec(), c.to_vec()];
    let rgb = |r: &[P], g: &[P], b: &[P]| [r.to_vec(), g.to_vec(), b.to_vec()];
    let warm_line = line(0.0, level(240.0));
    let curves: [(&'static str, [Vec<P>; 3]); 11] = [
        ("Warm Colors", rgb(&RAISED, &warm_line, &warm_line)),
        ("Cool Colors", rgb(&warm_line, &warm_line, &RAISED)),
        ("Reduce Contrast", same(&line(level(40.0), level(210.0)))),
        (
            "Increase Contrast",
            same(&[
                P::with(0.0, 0.0, [0.0, 0.0], [0.0667, 0.0]),
                P::split(0.2, 0.0, [-0.0667, 0.0], [0.2, 1.0 / 3.0]),
                P::split(0.8, 1.0, [-0.2, -1.0 / 3.0], [0.0667, 0.0]),
                P::with(1.0, 1.0, [-0.0667, 0.0], [0.0, 0.0]),
            ]),
        ),
        (
            "Brighten Brights",
            same(&[
                P::with(0.0, 0.0, [0.0, 0.0], [1.0 / 6.0, 1.0 / 6.0]),
                P::split(0.5, 0.5, [-1.0 / 6.0, -1.0 / 6.0], [0.0643, 0.1863]),
                P::with(1.0, 1.0, [-0.35, -0.0695], [0.0, 0.0]),
            ]),
        ),
        (
            "Highlight Shadows",
            same(&[
                P::with(0.0, 0.0, [0.0, 0.0], [0.04, 0.178]),
                P::split(0.35, 0.36, [-0.146, -0.036], [0.2167, 0.2133]),
                P::with(1.0, 1.0, [-0.2167, -0.2133], [0.0, 0.0]),
            ]),
        ),
        (
            "Yellow Tint",
            rgb(
                &IDENTITY,
                &IDENTITY,
                &[
                    P::with(0.0, 0.0, [0.0, 0.0], [0.1, 0.025]),
                    P::with(1.0, level(197.0), [-0.6363, -0.5965], [0.0, 0.0]),
                ],
            ),
        ),
        (
            "Night",
            rgb(
                &[
                    P::with(0.0, 0.0, [0.0, 0.0], [0.2429, 0.0]),
                    P::with(1.0, level(96.0), [-0.2003, -0.2249], [0.0, 0.0]),
                ],
                &[
                    P::with(0.0, 0.0, [0.0, 0.0], [0.2533, 0.0]),
                    P::with(1.0, level(124.0), [-0.1425, -0.2378], [0.0, 0.0]),
                ],
                &[
                    P::with(0.0, 0.0, [0.0, 0.0], [0.2613, 0.0]),
                    P::with(1.0, level(170.0), [-0.1343, -0.2785], [0.0, 0.0]),
                ],
            ),
        ),
        (
            "Infrared",
            rgb(
                &[
                    P::with(0.0, 0.0, [0.0, 0.0], [0.0172, 0.0283]),
                    P::with(0.172, 0.17, [-0.0748, 0.0], [0.0682, 0.0]),
                    P::split(0.361, 0.018, [-0.0785, 0.0], [0.2338, 0.0667]),
                    P::split(0.96, 1.0, [-0.0449, 0.0733], [0.0133, 0.0]),
                    P::with(1.0, 1.0, [-0.0133, 0.0], [0.0, 0.0]),
                ],
                &[
                    P::with(0.0, 0.0, [0.0, 0.0], [0.0165, 0.0]),
                    P::split(0.165, 0.0, [-0.096, 0.0114], [0.017, -0.1218]),
                    P::split(0.315, 0.678, [-0.1215, -0.0099], [0.0277, 0.0]),
                    P::with(0.42, 0.631, [-0.0551, 0.0], [0.119, 0.0]),
                    P::split(0.675, 0.875, [-0.066, 0.0], [0.1802, -0.101]),
                    P::split(0.881, 0.302, [-0.1407, 0.0699], [0.0322, 0.0]),
                    P::with(0.978, 1.0, [-0.0132, 0.0], [0.0073, 0.0]),
                    P::with(1.0, 1.0, [-0.0073, 0.0], [0.0, 0.0]),
                ],
                &[
                    P::with(0.0, 0.0, [0.0, 0.0], [0.0, 0.2941]),
                    P::split(0.205, 0.839, [-0.083, 0.0295], [0.1761, 0.0]),
                    P::with(0.609, 0.081, [-0.0824, 0.0], [0.0229, 0.0]),
                    P::with(0.722, 0.19, [-0.0314, 0.0], [0.0241, 0.0]),
                    P::split(0.861, 0.028, [-0.0756, 0.0], [0.139, 0.0054]),
                    P::with(1.0, 1.0, [0.0, -0.321], [0.0, 0.0]),
                ],
            ),
        ),
        (
            "Negative Shadows",
            same(&[
                P::with(0.0, 1.0, [0.0, 0.0], [0.0263, -0.0693]),
                P::split(level(45.0), level(67.0), [-0.1235, 0.0321], [0.116, -0.012]),
                P::with(1.0, 1.0, [-0.1, -0.097], [0.0, 0.0]),
            ]),
        ),
        (
            "Blues and Yellows",
            rgb(&IDENTITY, &IDENTITY, &line(1.0, 0.0)),
        ),
    ];
    curves
        .into_iter()
        .map(|(name, curves)| Preset {
            name,
            values: vec![(
                CURVES.to_str().unwrap_or("curves"),
                PresetValue::Curves(curves.to_vec()),
            )],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_core::curve::curve_at;

    #[test]
    fn calculate_stretches_balances_and_spreads() {
        // A dull reddish picture: red from 0.3 to 0.7, green and blue from
        // 0.2 to 0.5.
        let mut h = Histograms::new();
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            h.add([0.3 + 0.4 * t, 0.2 + 0.3 * t, 0.2 + 0.3 * t]);
        }
        let at = |c: &Vec<[f64; 2]>, x: f64| {
            let p: Vec<P> = c.iter().map(|q| P::new(q[0], q[1])).collect();
            curve_at(&p, x)
        };
        // Color Levels: each channel reaches black and white.
        let levels = auto_curves(&h, 1);
        assert!(at(&levels[0], 0.3) < 0.02 && at(&levels[0], 0.7) > 0.98);
        assert!(at(&levels[1], 0.2) < 0.02 && at(&levels[1], 0.5) > 0.98);
        // Contrast: one curve for the three.
        let contrast = auto_curves(&h, 2);
        assert_eq!(contrast[0], contrast[2]);
        // Color Balance: red's average comes down to the grey, blue's up.
        let balance = auto_curves(&h, 0);
        assert!(at(&balance[0], 0.5) < 0.5 && at(&balance[2], 0.35) > 0.35);
        // Histogram Equalization: rising, from 0 to 1.
        let eq = auto_curves(&h, 3);
        assert!(eq[0].windows(2).all(|w| w[1][1] >= w[0][1]));
        assert_eq!((eq[0][0][1], eq[0][8][1]), (0.0, 1.0));
        // Nothing to read: the curves stay straight.
        assert_eq!(
            auto_curves(&Histograms::new(), 1)[0],
            vec![[0.0, 0.0], [1.0, 1.0]]
        );
    }

    #[test]
    fn the_presets_look_like_vegass() {
        let all = presets();
        assert_eq!(all.len(), 11);
        let curves = |name: &str| -> Vec<Vec<P>> {
            let p = all.iter().find(|p| p.name == name).expect(name);
            match &p.values[0].1 {
                PresetValue::Curves(c) => c.clone(),
                other => panic!("curves: {other:?}"),
            }
        };
        let near = |a: f64, b: f64| (a - b).abs() < 0.01;
        // Read off VEGAS's own drawings.
        let warm = curves("Warm Colors");
        assert!(near(curve_at(&warm[0], 0.5), 0.593));
        assert!(near(curve_at(&warm[1], 1.0), 0.941));
        let reduce = curves("Reduce Contrast");
        assert!(near(curve_at(&reduce[0], 0.5), 0.490));
        let increase = curves("Increase Contrast");
        assert!(near(curve_at(&increase[0], 0.1), 0.0) && near(curve_at(&increase[0], 0.5), 0.5));
        let bright = curves("Brighten Brights");
        assert!(near(curve_at(&bright[0], 0.25), 0.25) && near(curve_at(&bright[0], 0.75), 0.893));
        let shadows = curves("Highlight Shadows");
        assert!(near(curve_at(&shadows[0], 0.1), 0.192));
        let night = curves("Night");
        assert!(near(curve_at(&night[2], 0.5), 0.198));
        let infrared = curves("Infrared");
        assert!(near(curve_at(&infrared[2], 0.205), 0.839));
        assert!(near(curve_at(&infrared[1], 0.42), 0.631));
        let negative = curves("Negative Shadows");
        assert!(
            near(curve_at(&negative[0], 0.05), 0.628) && near(curve_at(&negative[0], 0.5), 0.5)
        );
        let blues = curves("Blues and Yellows");
        assert!(near(curve_at(&blues[2], 0.25), 0.75));
    }
}
