//! Dual Fish Eye Stitching, as VEGAS's: its panel — Field of view, the
//! Left and Right fisheye's orientations, the Image Vignetting group
//! (Remove vignetting, Intensity, Range) and the Refinement group (Center,
//! Radius, Rotation) —, its five cameras' presets and its math, in single
//! precision as VEGAS works it out. Each pixel of the equirectangular
//! picture is a direction on the sphere, the frame's middle along the
//! front lens's axis; where only one lens sees it, its equidistant
//! fish-eye point — the angle off the lens's axis over half the field of
//! view, less its Center part, times Radius, turned by Rotation (in
//! hundredths of a degree) and the lens's orientation; the back lens's
//! circle on the left half, the front's on the right — read between four
//! pixels, the edges held; where both see it (fields of view past 180°)
//! both, mixed along the overlap in Y′UV. Remove vignetting raises each
//! channel to a power that falls from 1 by Intensity over the outer Range
//! of each circle. The frame read is at most half as high as wide, its
//! top; the picture comes out opaque. On the card `Plan::Stitch`,
//! `kit_stitch`, with each column's and row's cosine and sine from
//! `card_table`.
#![allow(unsafe_code)]

use std::f32::consts::{FRAC_PI_2, PI};
use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::{Plan, Values};
use crate::kit::Buf;
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.DualFishEyeStitching";

const FOV: &CStr = c"fov";
const LEFT: &CStr = c"orientLeft";
const RIGHT: &CStr = c"orientRight";
const VIGNETTING: &CStr = c"imageVignetting";
const FALLOFF: &CStr = c"falloffUse";
const FACTOR: &CStr = c"falloffFactor";
const RANGE: &CStr = c"falloffRange";
const REFINEMENT: &CStr = c"refinement";
const CENTER: &CStr = c"center";
const NARROWING: &CStr = c"narrowing";
const ROTATION: &CStr = c"rotation";

const ORIENTATIONS: [&CStr; 4] = [c"0 Degree", c"-90 Degrees", c"180 Degrees", c"90 Degrees"];

/// What a frame is stitched with, as VEGAS hands it to its stitcher.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    /// The field of view, in radians.
    fov: f32,
    /// How far along the lenses' axis (−1 to 1) both see a direction:
    /// within it either way they overlap.
    reach: f32,
    intensity: f32,
    range: f32,
    /// Center and Radius, as parts.
    center: f32,
    narrowing: f32,
    /// Each lens's turn, Rotation and its orientation, as its cosine and
    /// sine: the back's (Left fisheye, the left half), the front's.
    turns: [[f32; 2]; 2],
}

impl Settings {
    fn make(n: [f64; 7], left: c_int, right: c_int) -> Settings {
        let [fov, falloff, factor, range, center, narrowing, rotation] = n;
        let on = falloff != 0.0;
        let fov = (fov as f32 * PI) / 180.0;
        let rotation = ((rotation as f32 / 100.0) * PI) / 180.0;
        // VEGAS's -90 Degrees turns the lens's points a quarter clockwise.
        let turn = |orientation: c_int| {
            let a = match orientation {
                1 => rotation - FRAC_PI_2,
                2 => rotation + PI,
                3 => rotation + FRAC_PI_2,
                _ => rotation,
            };
            [a.cos(), a.sin()]
        };
        Settings {
            fov,
            reach: ((fov - PI) * 0.5).sin().abs(),
            intensity: if on { factor as f32 / 100.0 } else { 0.0 },
            range: if on { range as f32 / 100.0 } else { 0.0 },
            center: center as f32 / 100.0,
            narrowing: narrowing as f32 / 100.0,
            turns: [turn(left), turn(right)],
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        let p = |n: &CStr| fx.param(n).f64_at(time);
        Settings::make(
            [
                p(FOV),
                f64::from(fx.param(FALLOFF).int_at(time)),
                p(FACTOR),
                p(RANGE),
                p(CENTER),
                p(NARROWING),
                p(ROTATION),
            ],
            fx.param(LEFT).int_at(time),
            fx.param(RIGHT).int_at(time),
        )
    }

    pub(crate) fn of_values(v: Values<'_>) -> Settings {
        Settings::make(
            [
                v.f64(FOV, 180.0),
                v.f64(FALLOFF, 1.0),
                v.f64(FACTOR, 20.0),
                v.f64(RANGE, 40.0),
                v.f64(CENTER, 0.0),
                v.f64(NARROWING, 100.0),
                v.f64(ROTATION, 0.0),
            ],
            v.int(LEFT, 0),
            v.int(RIGHT, 0),
        )
    }

    /// The card's numbers: the field of view, the overlap's reach, the
    /// vignetting's intensity and range, Center and Radius, then each
    /// lens's turn (the back's, the front's).
    pub(crate) fn plan(&self) -> Plan {
        let [[cb, sb], [cf, sf]] = self.turns;
        Plan::Stitch {
            numbers: [
                self.fov,
                self.reach,
                self.intensity,
                self.range,
                self.center,
                self.narrowing,
                cb,
                sb,
                cf,
                sf,
            ],
        }
    }
}

/// Each of a `w` × `h` frame's columns' longitude's cosine and sine, then
/// each of its rows' latitude's from the top, as VEGAS works them out:
/// the card reads them too.
pub(crate) fn card_table(w: u32, h: u32) -> Vec<[f32; 4]> {
    let angle = |k: u32, n: u32, half: f32| {
        let t = k as f32 / n.saturating_sub(1).max(1) as f32;
        let a = ((t + t) - 1.0) * half;
        [a.cos(), a.sin(), 0.0, 0.0]
    };
    let columns = (0..w).map(|k| angle(k, w, PI));
    let rows = (0..h).map(|j| angle(h - 1 - j, h, FRAC_PI_2));
    columns.chain(rows).collect()
}

/// A lens's point for direction `d` (x along the front lens, y up;
/// `front` the right half's lens): where to read the frame (each axis −1
/// to 1, up second) and how far out on the lens's circle it is.
fn project(s: &Settings, d: [f32; 3], front: bool) -> ([f32; 2], f32) {
    let rho = d[2].hypot(d[1]);
    let theta = rho.atan2(d[0]);
    let r = (theta + theta) / s.fov;
    let shift = r * s.center;
    let rr = (r - shift) * s.narrowing;
    // Around the axis VEGAS's angle is atan2(y, z): its cosine and sine
    // are the sides over ρ (none along the axis).
    let (px, py) = if rho > 0.0 {
        (d[2] / rho * rr, d[1] / rho * rr)
    } else {
        (rr, 0.0)
    };
    let [ca, sa] = s.turns[usize::from(front)];
    let x = ca * px - sa * py;
    let u = if front {
        x + 1.0 + shift
    } else {
        (x - 1.0) - shift
    };
    ([u * 0.5, ca * py + sa * px], rr.abs())
}

/// A channel raised to the vignetting's power `e`, kept within 0–1.
fn power(v: f32, e: f32) -> f32 {
    if v > 0.0 {
        v.powf(e).min(1.0)
    } else if e == 0.0 {
        1.0
    } else {
        0.0
    }
}

/// The frame's part read (`w` × `h`, rows up from row `base` of `src`) at
/// `at` (each axis −1 to 1), between its four pixels, the edges held, the
/// vignetting taken out `radius` out on the lens's circle.
fn read(
    src: &[[f32; 4]],
    [w, h, base]: [usize; 3],
    at: [f32; 2],
    radius: f32,
    s: &Settings,
) -> [f32; 3] {
    let u = at[0].clamp(-1.0, 1.0);
    let v = at[1].clamp(-1.0, 1.0);
    let x = (w - 1) as f32 * (u + 1.0) * 0.5;
    let top = (h - 1) as f32;
    let y = top - (top - (v + 1.0) * 0.5 * top);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (gx, gy) = (1.0 - fx, 1.0 - fy);
    let (i, j) = (x0 as usize, y0 as usize);
    let at = |i: usize, j: usize| src[(base + j.min(h - 1)) * w + i.min(w - 1)];
    let (p00, p10, p01, p11) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
    let mut c: [f32; 3] = std::array::from_fn(|k| {
        p00[k] * gx * gy + p10[k] * fx * gy + p01[k] * gx * fy + p11[k] * fx * fy
    });
    if s.intensity > 0.0 && s.range != 0.0 && 1.0 - s.range <= radius {
        let f = (radius - (1.0 - s.range)) / s.range;
        let e = 1.0 - f * s.intensity * f;
        c = c.map(|v| power(v, e));
    }
    c
}

/// Two colors mixed, `a` weighing `wa` and `b` `wb`, in Y′UV as VEGAS
/// mixes them, each channel kept within 0–1.
fn mixed(a: [f32; 3], b: [f32; 3], wa: f32, wb: f32) -> [f32; 3] {
    let ya = a[1] * 0.587 + a[0] * 0.299 + a[2] * 0.114;
    let yb = b[1] * 0.587 + b[0] * 0.299 + b[2] * 0.114;
    let y = yb * wb + ya * wa;
    let vv = ((b[0] - yb) / 0.701) * 0.615 * wb + ((a[0] - ya) / 0.701) * 0.615 * wa;
    let uu = ((b[2] - yb) / 0.886) * 0.436 * wb + ((a[2] - ya) / 0.886) * 0.436 * wa;
    [
        (vv * 1.14 + y).clamp(0.0, 1.0),
        ((y - uu * 0.395) - vv * 0.581).clamp(0.0, 1.0),
        (uu * 2.033 + y).clamp(0.0, 1.0),
    ]
}

/// The equirectangular pixel whose column's longitude and row's latitude
/// have the cosines and sines `col` and `row`.
fn stitched(
    src: &[[f32; 4]],
    size: [usize; 3],
    s: &Settings,
    col: [f32; 4],
    row: [f32; 4],
) -> [f32; 4] {
    let d = [col[0] * row[0], row[1], col[1] * row[0]];
    let t = s.reach;
    let c = if t <= d[0].abs() {
        let front = d[0] > -t;
        let d = if front { d } else { [d[0].abs(), d[1], -d[2]] };
        let (at, r) = project(s, d, front);
        read(src, size, at, r, s)
    } else {
        let (af, rf) = project(s, d, true);
        let (ab, rb) = project(s, [-d[0], d[1], -d[2]], false);
        let w = if d[0] > 0.0 {
            (d[0].abs() / t + 1.0) * 0.5
        } else if d[0] < 0.0 {
            (1.0 - d[0].abs() / t) * 0.5
        } else {
            0.5
        };
        mixed(
            read(src, size, af, rf, s),
            read(src, size, ab, rb, s),
            w,
            1.0 - w,
        )
    };
    [c[0], c[1], c[2], 1.0]
}

/// VEGAS's five cameras.
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: f64| PresetValue::Num(vec![v]);
    let p = |name: &'static str, v: [f64; 9]| Preset {
        name,
        values: vec![
            ("fov", num(v[0])),
            ("orientLeft", num(v[1])),
            ("orientRight", num(v[2])),
            ("falloffUse", num(v[3])),
            ("falloffFactor", num(v[4])),
            ("falloffRange", num(v[5])),
            ("center", num(v[6])),
            ("narrowing", num(v[7])),
            ("rotation", num(v[8])),
        ],
    };
    vec![
        p(
            "Samsumg Gear 360°",
            [192.46, 0.0, 0.0, 1.0, 34.0, 41.5, 0.0, 100.0, 0.0],
        ),
        // VEGAS's leaves Intensity and Range as they were: the defaults.
        p(
            "LG 360° Cam",
            [192.4, 0.0, 0.0, 0.0, 20.0, 40.0, 0.0, 100.0, 0.0],
        ),
        p(
            "Ricoh Theta V",
            [191.0, 3.0, 1.0, 1.0, 20.0, 30.0, 0.0, 100.0, 0.0],
        ),
        p(
            "Ricoh Theta S",
            [194.0, 3.0, 1.0, 1.0, 12.0, 30.0, 2.25, 96.0, 0.0],
        ),
        p(
            "Ricoh Theta",
            [191.0, 3.0, 3.0, 1.0, 35.0, 44.0, 0.0, 97.0, 0.0],
        ),
    ]
}

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
        render(fx, args)
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        let time = args.dbl(kOfxPropTime, 0);
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        let all = fx
            .clip(kOfxImageEffectSimpleSourceClipName)
            .rod(time)
            .unwrap_or(roi);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &all);
        kOfxStatOK
    } else if is(action, kOfxActionLoad)
        || is(action, kOfxActionUnload)
        || is(action, kOfxActionCreateInstance)
        || is(action, kOfxActionDestroyInstance)
        || is(action, kOfxActionInstanceChanged)
    {
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Dual Fish Eye Stitching");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/360");
    p.set_str(
        kOfxPropPluginDescription,
        c"Stitches the two fish-eye circles of a 360° camera into one equirectangular picture.",
    );
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextFilter],
    );
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    p.set_int(kOfxImageEffectPropSupportsTiles, 0);
    p.set_int(kOfxImageEffectPropSupportsMultiResolution, 1);
    p.set_int(kOfxImageEffectPluginPropHostFrameThreading, 0);
    p.set_str(
        kOfxImageEffectPluginRenderThreadSafety,
        kOfxImageEffectRenderFullySafe,
    );
}

fn number(
    fx: Fx,
    (name, label): (&CStr, &CStr),
    (default, min, max): (f64, f64, f64),
    parent: Option<&CStr>,
    hint: &CStr,
) {
    let p = fx.define_param(kOfxParamTypeDouble, name, label);
    p.set_dbl(kOfxParamPropDefault, default);
    p.set_dbl(kOfxParamPropMin, min);
    p.set_dbl(kOfxParamPropMax, max);
    p.set_dbl(kOfxParamPropDisplayMin, min);
    p.set_dbl(kOfxParamPropDisplayMax, max);
    p.set_int(kOfxParamPropDigits, 2);
    p.set_str(kOfxParamPropHint, hint);
    if let Some(g) = parent {
        p.set_str(kOfxParamPropParent, g);
    }
}

fn describe_in_context(fx: Fx) {
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);
    number(
        fx,
        (FOV, c"Field of view"),
        (180.0, 180.0, 220.0),
        None,
        c"The field of view is the extent of the observable world that is seen at any given moment.",
    );
    for (name, label, hint) in [
        (
            LEFT,
            c"Left fisheye",
            c"Select the orientation of the left fisheye in your raw footage.",
        ),
        (
            RIGHT,
            c"Right fisheye",
            c"Select the orientation of the right fisheye in your raw footage.",
        ),
    ] {
        let p = fx.define_param(kOfxParamTypeChoice, name, label);
        p.set_strs(kOfxParamPropChoiceOption, &ORIENTATIONS);
        p.set_int(kOfxParamPropDefault, 0);
        p.set_str(kOfxParamPropHint, hint);
    }
    let vignetting = c"Vignetting is a reduction of an image's brightness or saturation toward the periphery compared to the image center.";
    let g = fx.define_param(kOfxParamTypeGroup, VIGNETTING, c"Image Vignetting");
    g.set_str(kOfxParamPropHint, vignetting);
    let p = fx.define_param(kOfxParamTypeBoolean, FALLOFF, c"Remove vignetting");
    p.set_int(kOfxParamPropDefault, 1);
    p.set_str(kOfxParamPropParent, VIGNETTING);
    p.set_str(kOfxParamPropHint, vignetting);
    number(
        fx,
        (FACTOR, c"Intensity"),
        (20.0, 0.0, 100.0),
        Some(VIGNETTING),
        c"The maximum intensity to increase toward the periphery.",
    );
    number(
        fx,
        (RANGE, c"Range"),
        (40.0, 0.0, 100.0),
        Some(VIGNETTING),
        c"The radial distance from the periphery to treat.",
    );
    let g = fx.define_param(kOfxParamTypeGroup, REFINEMENT, c"Refinement");
    g.set_str(kOfxParamPropHint, c"Refinement");
    number(
        fx,
        (CENTER, c"Center"),
        (0.0, 0.0, 10.0),
        Some(REFINEMENT),
        c"The offset of the center of each fisheye.",
    );
    number(
        fx,
        (NARROWING, c"Radius"),
        (100.0, 70.0, 100.0),
        Some(REFINEMENT),
        c"The radial distance to use in all computations.",
    );
    number(
        fx,
        (ROTATION, c"Rotation"),
        (0.0, -100.0, 100.0),
        Some(REFINEMENT),
        c"The rotation between both fisheyes.",
    );
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    if let Some(s) = &src
        && (s.depth != out.depth || s.channels != out.channels)
    {
        return kOfxStatErrImageFormat;
    }
    let source = Buf::read(src.as_ref());
    drop(src);
    let s = Settings::at(fx, time);
    let result = stitch(&source, &s);
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

/// The picture stitched (as it is when smaller than 2 × 2 pixels).
fn stitch(source: &Buf, s: &Settings) -> Buf {
    let (w, h) = (
        source.width().max(0) as usize,
        source.height().max(0) as usize,
    );
    if w < 2 || h < 2 {
        return source.clone();
    }
    // The frame read is at most half as high as wide: its top.
    let used = h.min(w / 2);
    let size = [w, used, h - used];
    let table = card_table(w as u32, h as u32);
    let [bx, by] = [source.bounds[0], source.bounds[1]];
    source.map(|x, y| {
        let (i, j) = ((x - bx) as usize, (y - by) as usize);
        stitched(&source.data, size, s, table[i], table[w + h - 1 - j])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(left: c_int, right: c_int) -> Settings {
        Settings::make([180.0, 0.0, 20.0, 40.0, 0.0, 100.0, 0.0], left, right)
    }

    #[test]
    fn ahead_is_the_front_circles_middle_and_behind_the_backs() {
        let s = settings(0, 0);
        // Straight ahead: the right half's circle's middle.
        let (at, r) = project(&s, [1.0, 0.0, 0.0], true);
        assert!(
            (at[0] - 0.5).abs() < 1e-6 && at[1].abs() < 1e-6 && r < 1e-6,
            "{at:?}"
        );
        // Behind (mirrored): the left half's.
        let (at, _) = project(&s, [1.0, 0.0, 0.0], false);
        assert!((at[0] + 0.5).abs() < 1e-6, "{at:?}");
        // At 90° off the axis: the circle's rim, up its top.
        let (at, r) = project(&s, [0.0, 1.0, 0.0], true);
        assert!(
            (r - 1.0).abs() < 1e-5 && (at[1] - 1.0).abs() < 1e-5,
            "{at:?} {r}"
        );
    }

    #[test]
    fn orientations_turn_each_lens() {
        // The right fisheye at 90 Degrees: its rim's top read on its left.
        let (at, _) = project(&settings(0, 3), [0.0, 1.0, 0.0], true);
        assert!(at[0].abs() < 1e-5 && at[1].abs() < 1e-5, "{at:?}");
        // At -90 Degrees on its right.
        let (at, _) = project(&settings(0, 1), [0.0, 1.0, 0.0], true);
        assert!((at[0] - 1.0).abs() < 1e-5 && at[1].abs() < 1e-5, "{at:?}");
        // The left fisheye at 180 Degrees: upside down.
        let (at, _) = project(&settings(2, 0), [1.0, 1.0, 0.0], false);
        assert!(at[1] < -0.4, "{at:?}");
    }

    #[test]
    fn the_overlap_mixes_in_yuv() {
        let a = [1.0, 0.0, 0.0];
        let b = [0.0, 0.0, 1.0];
        let half = mixed(a, b, 0.5, 0.5);
        for (k, v) in half.iter().enumerate() {
            assert!((v - (a[k] + b[k]) / 2.0).abs() < 2e-3, "{half:?}");
        }
        assert_eq!(
            mixed(a, b, 1.0, 0.0).map(|v| (v * 1000.0).round()),
            [1000.0, 0.0, 0.0]
        );
    }

    #[test]
    fn vignetting_lightens_the_rim() {
        let src = vec![[0.25f32; 4]; 4 * 2];
        let on = Settings::make([180.0, 1.0, 50.0, 40.0, 0.0, 100.0, 0.0], 0, 0);
        let rim = read(&src, [4, 2, 0], [0.0, 0.0], 1.0, &on)[0];
        let middle = read(&src, [4, 2, 0], [0.0, 0.0], 0.2, &on)[0];
        // At the rim the power is 1 − Intensity.
        assert!((middle - 0.25).abs() < 1e-6, "{middle}");
        assert!((rim - 0.25f32.powf(0.5)).abs() < 1e-6, "{rim}");
        // Off, nothing changes.
        let off = Settings::make([180.0, 0.0, 50.0, 40.0, 0.0, 100.0, 0.0], 0, 0);
        assert_eq!(read(&src, [4, 2, 0], [0.0, 0.0], 1.0, &off)[0], 0.25);
    }

    #[test]
    fn the_lenses_meet_at_the_sides_and_the_top_of_a_wide_frame_is_read() {
        // Left half red (the back lens), right half green (the front), a
        // blue strip under the half-as-high part read.
        let (w, h) = (16usize, 12usize);
        let mut data = vec![[0.0, 0.0, 1.0, 1.0]; w * h];
        for j in h - w / 2..h {
            for i in 0..w {
                data[j * w + i] = if i < w / 2 {
                    [1.0, 0.0, 0.0, 1.0]
                } else {
                    [0.0, 1.0, 0.0, 1.0]
                };
            }
        }
        let source = Buf {
            bounds: [0, 0, w as c_int, h as c_int],
            data,
        };
        let out = stitch(&source, &settings(0, 0));
        let near = |i: usize, j: usize, c: [f32; 4]| {
            let p = out.data[j * w + i];
            p.iter().zip(c).all(|(a, b)| (a - b).abs() < 1e-6)
        };
        // The middle is ahead (green), the sides behind (red), no blue.
        assert!(near(w / 2, h / 2, [0.0, 1.0, 0.0, 1.0]));
        assert!(near(0, h / 2, [1.0, 0.0, 0.0, 1.0]));
        assert!(out.data.iter().all(|c| c[2] == 0.0 && c[3] == 1.0));
    }
}
