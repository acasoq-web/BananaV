//! Lens Correction, as VEGAS's: each pixel reads the picture where a lens
//! model sends it — the PanoTools polynomial `a·ρ³ + b·ρ² + c·ρ + d` of
//! the distance from Distortion Center (ρ 1 at half the picture's shorter
//! side, `d` keeping that radius where it is), over Scale; or, with the
//! hidden Model on polynomial, the radial `1 + k1·r² + k2·r⁴ + k3·r⁶` of
//! the picture's parts — between the four pixels around it, weighed in
//! 1024ths, nothing past the picture's edges. Lens Model's Search for
//! lists the lenses whose names have what it says (Found Lenses), and
//! picking one gives k1, k2 and k3 its coefficients. Show Distortion leaves
//! the picture as it is and draws over it the grid the coefficients bend,
//! to match the curved lines of what the lens filmed.
//!
//! The lenses are an adaptation of the lensfun project's database (see
//! `lens_correction/lenses.rs`), the math the reference's, in single
//! precision as it works it out; the card's kernel
//! (`kit_lens_correction`) does the same sums.
#![allow(unsafe_code)]

mod lenses;

use std::ffi::{CStr, CString, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::{Plan, Values};
use crate::kit::{Buf, Spec, define};
use crate::ofx::{Draw, Fx, Props, interact_props, is, near_handle, redraw, suites};

pub(crate) const ID: &CStr = c"org.bananav.LensCorrection";

const GROUP: &CStr = c"lensModel";
const QUERY: &CStr = c"searchFor";
const FOUND: &CStr = c"foundLenses";
const MODEL: &CStr = c"model";
const CENTER: &CStr = c"distortionCenter";
const K: [&CStr; 3] = [c"k1", c"k2", c"k3"];
const SCALE: &CStr = c"scale";
const SHOW: &CStr = c"showDistortion";

/// What Found Lenses says before any search, first in a search's list,
/// and when a search finds nothing.
const UNSEARCHED: &CStr = c"Select Lens";
const PICK: &CStr = c"Select lens";
const NOTHING: &CStr = c"Nothing found";

/// What may be typed (the reference's has no limits).
const ANY: [f64; 2] = [-1.0e9, 1.0e9];

const SPECS: &[Spec] = &[
    Spec::Choice(MODEL, c"Model", &[c"polynomial", c"ptLens"], 1, c""),
    Spec::Pair(
        CENTER,
        c"Distortion Center",
        [0.5, 0.5],
        ANY,
        [0.0, 1.0],
        c"Adjust the center in case your lens is off-centered.",
    ),
    Spec::Ranged(
        K[0],
        c"Radial Distortion k1",
        0.0,
        ANY,
        [-1.0, 1.0],
        c"This is the parameter a of ptlens model.",
    ),
    Spec::Ranged(
        K[1],
        c"Radial Distortion k2",
        0.0,
        ANY,
        [-1.0, 1.0],
        c"This is the parameter b of ptlens model.",
    ),
    Spec::Ranged(
        K[2],
        c"Radial Distortion k3",
        0.0,
        ANY,
        [-1.0, 1.0],
        c"This is the parameter c of ptlens model.",
    ),
    Spec::Ranged(SCALE, c"Scale", 1.0, [0.1, 10.0], [0.5, 2.0], c""),
    Spec::Toggle(
        SHOW,
        c"Show Distortion",
        false,
        c"Show the distortion which is corrected by the specified parameters.",
    ),
];

/// The settings a frame is drawn with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    /// k1, k2 and k3, in single precision as the model takes them.
    pub k: [f32; 3],
    /// Distortion Center: across and up, in parts of the picture.
    pub center: [f64; 2],
    pub scale: f64,
    /// Model: the PanoTools polynomial (else the radial one).
    pub ptlens: bool,
    pub show: bool,
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            k: K.map(|k| fx.param(k).f64_at(time) as f32),
            center: fx.param(CENTER).xy_at(time),
            scale: fx.param(SCALE).f64_at(time),
            ptlens: fx.param(MODEL).int_at(time) == 1,
            show: fx.param(SHOW).int_at(time) != 0,
        }
    }

    /// The settings in an instance's values (the card's).
    pub(crate) fn of_values(v: Values<'_>) -> Settings {
        Settings {
            k: K.map(|k| v.f64(k, 0.0) as f32),
            center: v.array(CENTER, [0.5, 0.5]),
            scale: v.f64(SCALE, 1.0),
            ptlens: v.int(MODEL, 1) == 1,
            show: v.int(SHOW, 0) != 0,
        }
    }

    /// Whether the picture stays as it is: Show Distortion on, or no
    /// distortion at Scale 1 (in single precision, as the reference
    /// tests them).
    pub(crate) fn changes_nothing(&self) -> bool {
        self.show || (self.k.iter().all(|&k| k == 0.0) && self.scale as f32 == 1.0)
    }

    /// The model on a `w` × `h` picture: its center the pixel
    /// Distortion Center falls in (single precision, truncated), the
    /// PanoTools polynomial over Scale.
    fn model(&self, w: u32, h: u32) -> Model {
        let pixel = |v: f64, n: u32| truncated(n as f32 * v as f32);
        Model::new(
            self.ptlens,
            self.k,
            (1.0 / self.scale) as f32,
            [pixel(self.center[0], w), pixel(self.center[1], h)],
            [w, h],
        )
    }

    /// What the card draws: nothing, or the model's settings — k1, k2 and
    /// k3, 1 / Scale and Distortion Center — for [`card_numbers`] to lay
    /// on the picture.
    pub(crate) fn plan(&self) -> Plan {
        if self.changes_nothing() {
            return Plan::Same;
        }
        let [k1, k2, k3] = self.k;
        Plan::LensCorrection {
            settings: [
                k1,
                k2,
                k3,
                (1.0 / self.scale) as f32,
                self.center[0] as f32,
                self.center[1] as f32,
            ],
            ptlens: self.ptlens,
        }
    }
}

/// The card's numbers for the model of `settings` (as [`Settings::plan`]
/// gives them) on a `w` × `h` picture, worked out as the processor's —
/// so that the card divides nothing: k1, k2 and k3, what the PanoTools
/// polynomial is multiplied by, the center in parts of the picture and in
/// its pixels, `2 / min(w, h)`, `1 / w`, `1 / h` and the polynomial's
/// constant term.
pub(crate) fn card_numbers(settings: &[f32; 6], ptlens: bool, w: u32, h: u32) -> [f32; 12] {
    let [k1, k2, k3, times, cx, cy] = *settings;
    let m = Model::new(
        ptlens,
        [k1, k2, k3],
        times,
        [truncated(w as f32 * cx), truncated(h as f32 * cy)],
        [w, h],
    );
    [
        k1,
        k2,
        k3,
        times,
        m.center[0],
        m.center[1],
        m.pixel[0],
        m.pixel[1],
        m.s,
        m.per[0],
        m.per[1],
        m.d,
    ]
}

/// A single-precision number made a whole one as the processor's
/// conversion does: toward zero, and out of range (or not a number) the
/// smallest.
fn truncated(v: f32) -> i32 {
    if v.is_nan() || !(-2_147_483_648.0..2_147_483_648.0).contains(&v) {
        i32::MIN
    } else {
        v as i32
    }
}

/// [`truncated`] of a double.
fn truncated_double(v: f64) -> i32 {
    if v.is_nan() || !(-2_147_483_648.0..2_147_483_648.0).contains(&v) {
        i32::MIN
    } else {
        v as i32
    }
}

/// The lens model on a picture: where each of its pixels reads.
#[derive(Clone, Copy, Debug)]
struct Model {
    ptlens: bool,
    k: [f32; 3],
    /// What the PanoTools polynomial is multiplied by.
    times: f32,
    /// The center, in parts of the picture and in its pixels.
    center: [f32; 2],
    pixel: [f32; 2],
    /// `2 / min(w, h)`: ρ's unit, half the shorter side.
    s: f32,
    /// `1 / w` and `1 / h`.
    per: [f32; 2],
    /// The PanoTools polynomial's constant term, `1 − k3 − k2 − k1`.
    d: f32,
}

impl Model {
    /// The model on a picture `size` big about the pixel `pixel` — left
    /// of the picture, about its middle.
    fn new(ptlens: bool, k: [f32; 3], times: f32, pixel: [i32; 2], size: [u32; 2]) -> Model {
        let (w, h) = (size[0] as f32, size[1] as f32);
        let center = if pixel[0] < 0 {
            [0.5, 0.5]
        } else {
            [pixel[0] as f32 / w, pixel[1] as f32 / h]
        };
        let [k1, k2, k3] = k;
        Model {
            ptlens,
            k,
            times,
            center,
            pixel: [w * center[0], h * center[1]],
            s: 2.0 / size[0].min(size[1]) as f32,
            per: [1.0 / w, 1.0 / h],
            d: ((1.0 - k3) - k2) - k1,
        }
    }

    /// Where pixel (x, y) (rows up) reads, in parts of the picture.
    fn at(&self, x: u32, y: u32) -> [f32; 2] {
        let [k1, k2, k3] = self.k;
        if self.ptlens {
            let (s, [cx, cy]) = (self.s, self.pixel);
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            let r = (dx * dx + dy * dy).sqrt();
            let f = (((r * s * s * s * k1 + s * s * k2) * r + k3 * s) * r + self.d) * self.times;
            [(f * dx + cx) * self.per[0], (f * dy + cy) * self.per[1]]
        } else {
            let [cx, cy] = self.center;
            let v = y as f32 * self.per[1] - cy;
            let u = x as f32 * self.per[0] - cx;
            let r2 = u * u + v * v;
            let f = ((r2 * k3 + k2) * r2 + k1) * r2 + 1.0;
            [f * u + cx, f * v + cy]
        }
    }
}

/// The picture `src` (`w` × `h` pixels, rows up, row after row) at `at`
/// (parts of the picture, as the scale of `w − 1` and `h − 1` pixels
/// counts them): between the four pixels around, each weighed in 1024ths
/// (truncated) — right of the last column the next row's first pixel,
/// past the last pixel the one before held —; nothing from 0.4 of a pixel
/// before the first column or row and 0.6 before the end on.
pub(crate) fn read(src: &[[f32; 4]], [w, h]: [u32; 2], at: [f32; 2]) -> [f32; 4] {
    let sx = (w - 1) as f32 * at[0];
    let sy = (h - 1) as f32 * at[1];
    // Not a number (an infinite polynomial times 0 at its center): out.
    let inside = sx > -0.4 && sx < w as f32 - 0.6 && sy > -0.4 && sy < h as f32 - 0.6;
    if !inside {
        return [0.0; 4];
    }
    let (ix, iy) = (sx as u32, sy as u32);
    let fx = (sx - ix as f32).max(0.0);
    let fy = (sy - iy as f32).max(0.0);
    let n = w as usize * h as usize;
    let i = iy as usize * w as usize + ix as usize;
    let p00 = src[i];
    let p10 = if i + 1 < n { src[i + 1] } else { p00 };
    let up = i + w as usize;
    let p01 = if up < n { src[up] } else { p00 };
    let p11 = if up + 1 < n { src[up + 1] } else { p01 };
    let weight = |v: f32| (v * 1024.0) as i32 as f32;
    let w00 = weight((1.0 - fx) * (1.0 - fy));
    let w10 = weight((1.0 - fy) * fx);
    let w01 = weight((1.0 - fx) * fy);
    let w11 = weight(fy * fx);
    std::array::from_fn(|c| {
        (p10[c] * w10 + w00 * p00[c] + p01[c] * w01 + p11[c] * w11) * 0.000_976_562_5
    })
}

/// `src` corrected as `s` says (rows up; every pixel of its box read
/// where the model sends it).
pub(crate) fn corrected(s: &Settings, src: &Buf) -> Buf {
    let (w, h) = (src.width().max(0) as u32, src.height().max(0) as u32);
    if w == 0 || h == 0 || s.changes_nothing() {
        return src.clone();
    }
    let model = s.model(w, h);
    let [x1, y1, ..] = src.bounds;
    src.map(|x, y| {
        read(
            &src.data,
            [w, h],
            model.at((x - x1) as u32, (y - y1) as u32),
        )
    })
}

/// The grid Show Distortion draws: 9 rows of 16 points, in parts of the
/// frame (across, up) — the places 16 × 9 pixels spread over a 192 × 108
/// picture read through the PanoTools model times Scale, whatever the
/// model (the reference's grid).
fn grid(s: &Settings) -> [[[f32; 2]; 16]; 9] {
    const SIZE: [u32; 2] = [192, 108];
    let pixel = |v: f64, n: u32| truncated_double(v * f64::from(n));
    let model = Model::new(
        true,
        s.k,
        s.scale as f32,
        [pixel(s.center[0], SIZE[0]), pixel(s.center[1], SIZE[1])],
        SIZE,
    );
    // The pixels: spread over the picture, the last held inside it.
    let spread = |i: usize, n: u32, steps: f32| {
        let t = i as f32 * (n as f32 / steps);
        if t <= (n - 1) as f32 { t as u32 } else { n - 1 }
    };
    std::array::from_fn(|r| {
        let y = spread(r, SIZE[1], 8.0);
        std::array::from_fn(|c| model.at(spread(c, SIZE[0], 15.0), y))
    })
}

/// A smooth line through `points` as separate lines (pairs of points),
/// `steps` to a stretch: the cardinal spline of tension 0.5 — each
/// stretch a cubic whose handles reach a sixth of the chord between the
/// points around their ends (at the line's ends, a sixth of the stretch).
fn curve(points: &[[f64; 2]], steps: usize) -> Vec<[f64; 2]> {
    let n = points.len();
    let mut lines = Vec::new();
    if n < 2 {
        return lines;
    }
    const T: f64 = 0.5 / 3.0;
    let p = |i: usize| points[i];
    let toward = |a: [f64; 2], d: [f64; 2], t: f64| [a[0] + d[0] * t, a[1] + d[1] * t];
    let minus = |a: [f64; 2], b: [f64; 2]| [a[0] - b[0], a[1] - b[1]];
    // The handles after and before point `i`.
    let after = |i: usize| {
        if i == 0 {
            toward(p(0), minus(p(1), p(0)), T)
        } else {
            toward(p(i), minus(p(i + 1), p(i - 1)), T)
        }
    };
    let before = |i: usize| {
        if i == n - 1 {
            toward(p(n - 1), minus(p(n - 2), p(n - 1)), T)
        } else {
            toward(p(i), minus(p(i + 1), p(i - 1)), -T)
        }
    };
    for i in 0..n - 1 {
        let (a, b, c, d) = (p(i), after(i), before(i + 1), p(i + 1));
        let mut last = a;
        for step in 1..=steps {
            let t = step as f64 / steps as f64;
            let u = 1.0 - t;
            let at: [f64; 2] = std::array::from_fn(|k| {
                u * u * u * a[k]
                    + 3.0 * u * u * t * b[k]
                    + 3.0 * u * t * t * c[k]
                    + t * t * t * d[k]
            });
            lines.push(last);
            lines.push(at);
            last = at;
        }
    }
    lines
}

/// The lenses whose names have `query` (letters of either case alike),
/// by their places in [`lenses::LENSES`].
fn search(query: &str) -> Vec<usize> {
    let query = query.to_ascii_lowercase();
    lenses::LENSES
        .iter()
        .enumerate()
        .filter(|(_, (name, _))| name.to_ascii_lowercase().contains(&query))
        .map(|(i, _)| i)
        .collect()
}

/// Gives Found Lenses the options `first` and the lenses `found` after
/// it.
fn list(fx: Fx, first: &CStr, found: impl Iterator<Item = usize>) {
    let names: Vec<CString> = found
        .filter_map(|i| CString::new(lenses::LENSES[i].0).ok())
        .collect();
    let mut options = vec![first];
    options.extend(names.iter().map(CString::as_c_str));
    let p = fx.param(FOUND).props();
    p.reset(kOfxParamPropChoiceOption);
    p.set_strs(kOfxParamPropChoiceOption, &options);
}

/// VEGAS's rules when `name` changes: Search for lists the lenses its
/// words find (all of them when empty; Nothing found when none, unless
/// the list says no more than one thing), Found Lenses back on its
/// first option; picking a lens gives k1, k2 and k3 its coefficients and
/// Distortion Center (0.5, 0) — the reference's.
fn changed(fx: Fx, time: f64, name: &str) {
    let is = |n: &CStr| n.to_str() == Ok(name);
    if is(QUERY) {
        fx.param(FOUND).set_int(0);
        let query = fx.param(QUERY).text_at(time);
        if query.is_empty() {
            list(fx, PICK, 0..lenses::LENSES.len());
        } else {
            let found = search(&query);
            if !found.is_empty() {
                list(fx, PICK, found.into_iter());
            } else if fx.param(FOUND).props().count(kOfxParamPropChoiceOption) > 1 {
                list(fx, NOTHING, std::iter::empty());
            }
        }
    } else if is(FOUND) {
        let Ok(pick) = usize::try_from(fx.param(FOUND).int_at(time)) else {
            return;
        };
        if pick == 0 {
            return;
        }
        let options = fx.param(FOUND).props().count(kOfxParamPropChoiceOption);
        let lens = if usize::try_from(options - 1) == Ok(lenses::LENSES.len()) {
            lenses::LENSES.get(pick - 1)
        } else {
            search(&fx.param(QUERY).text_at(time))
                .get(pick - 1)
                .map(|&i| &lenses::LENSES[i])
        };
        if let Some((_, coefficients)) = lens {
            for (k, &v) in K.iter().zip(coefficients) {
                fx.param(k).set_f64(v);
            }
            fx.param(CENTER).set_xy([0.5, 0.0]);
        }
    }
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
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        if Settings::at(fx, args.dbl(kOfxPropTime, 0)).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // Any pixel may read from anywhere in the picture.
        let time = args.dbl(kOfxPropTime, 0);
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        let roi = fx
            .clip(kOfxImageEffectSimpleSourceClipName)
            .rod(time)
            .unwrap_or(roi);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &roi);
        kOfxStatOK
    } else if is(action, kOfxActionInstanceChanged) {
        if args.is(kOfxPropType, kOfxTypeParameter) {
            changed(fx, args.dbl(kOfxPropTime, 0), &args.text(kOfxPropName, 0));
        }
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
    p.set_str(kOfxPropLabel, c"Lens Correction");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Utility");
    p.set_str(kOfxPropPluginDescription, c"Corrects Lens Distortions");
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextFilter, kOfxImageEffectContextGeneral],
    );
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    // The model spans the whole picture.
    p.set_int(kOfxImageEffectPropSupportsTiles, 0);
    p.set_int(kOfxImageEffectPropSupportsMultiResolution, 1);
    p.set_int(kOfxImageEffectPluginPropHostFrameThreading, 0);
    p.set_str(
        kOfxImageEffectPluginRenderThreadSafety,
        kOfxImageEffectRenderFullySafe,
    );
    p.set_ptr(
        kOfxImageEffectPluginPropOverlayInteractV2,
        overlay as *mut c_void,
    );
}

fn describe_in_context(fx: Fx) {
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);
    let g = fx.define_param(kOfxParamTypeGroup, GROUP, c"Lens Model");
    g.set_str(kOfxParamPropHint, c"Contains a set of lens models.");
    g.set_int(kOfxParamPropGroupOpen, 1);
    // Searching and picking are not kept: they give the coefficients.
    let p = fx.define_param(kOfxParamTypeString, QUERY, c"Search for");
    p.set_str(kOfxParamPropHint, c"Enter a lens model name to search for.");
    p.set_str(kOfxParamPropDefault, c"");
    let p2 = fx.define_param(kOfxParamTypeChoice, FOUND, c"Found Lenses");
    p2.set_str(kOfxParamPropHint, c"Enumerates all lenses fulling query.");
    p2.set_strs(kOfxParamPropChoiceOption, &[UNSEARCHED]);
    p2.set_int(kOfxParamPropDefault, 0);
    for p in [p, p2] {
        p.set_str(kOfxParamPropParent, GROUP);
        p.set_int(kOfxParamPropAnimates, 0);
        p.set_int(kOfxParamPropPersistant, 0);
    }
    for spec in SPECS {
        let Some(p) = define(fx, *spec) else { continue };
        let name = spec.name();
        if name == MODEL {
            p.set_int(kOfxParamPropSecret, 1);
        }
        if name == MODEL || name == SHOW {
            p.set_int(kOfxParamPropAnimates, 0);
        }
        if name != MODEL && name != SHOW {
            p.set_dbl(kOfxParamPropIncrement, 0.1);
        }
        if name == CENTER {
            p.set_int(kOfxParamPropDigits, 3);
        }
        if name == SCALE {
            p.set_int(kOfxParamPropDigits, 2);
        }
    }
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let window: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let Some(src) = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time) else {
        return kOfxStatFailed;
    };
    if src.depth != out.depth {
        return kOfxStatErrImageFormat;
    }
    let cur = Buf::read(Some(&src));
    drop(src);
    let result = corrected(&Settings::at(fx, time), &cur);
    let [x1, y1, x2, y2] = window;
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

/// The color of Show Distortion's grid.
const GRID: [f32; 4] = [64.0 / 255.0, 64.0 / 255.0, 64.0 / 255.0, 1.0];

/// What the overlay remembers: the center taken (where the pen took it,
/// from the center), and whether the pen is on it.
#[derive(Debug, Default)]
struct Overlay {
    drag: Option<[f64; 2]>,
    hover: bool,
}

/// The overlay: VEGAS's grid while Show Distortion is on, and the
/// Distortion Center's handle, dragged on the preview.
unsafe extern "C" fn overlay(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if is(action, kOfxActionDescribe) {
        return kOfxStatOK;
    }
    let Some(props) = interact_props(handle) else {
        return kOfxStatReplyDefault;
    };
    if is(action, kOfxActionCreateInstance) {
        let state = Box::into_raw(Box::new(Overlay::default()));
        props.set_ptr(kOfxPropInstanceData, state as *mut c_void);
        return kOfxStatOK;
    }
    let state = props.ptr(kOfxPropInstanceData) as *mut Overlay;
    if is(action, kOfxActionDestroyInstance) {
        if !state.is_null() {
            // SAFETY: made above, freed once.
            drop(unsafe { Box::from_raw(state) });
            props.set_ptr(kOfxPropInstanceData, std::ptr::null_mut());
        }
        return kOfxStatOK;
    }
    if state.is_null() {
        return kOfxStatReplyDefault;
    }
    // SAFETY: this overlay's state; hosts run one of its actions at a time.
    let state = unsafe { &mut *state };
    let args = Props(in_args);
    let fx = Fx(args.ptr(kOfxPropEffectInstance) as OfxImageEffectHandle);
    if fx.0.is_null() {
        return kOfxStatReplyDefault;
    }
    let time = args.dbl(kOfxPropTime, 0);
    let ps: [f64; 2] = args.dbls(kOfxInteractPropPixelScale);
    let ps = ps.map(|v| if v.abs() > 1e-12 { v.abs() } else { 1.0 });
    let (origin, size) = fx.frame();
    let s = Settings::at(fx, time);
    let center = [
        origin[0] + s.center[0] * size[0],
        origin[1] + s.center[1] * size[1],
    ];
    if is(action, kOfxInteractActionDraw) {
        let Some(d) = Draw::of(args) else {
            return kOfxStatReplyDefault;
        };
        if s.show {
            let g = grid(&s);
            let at = |p: [f32; 2]| {
                [
                    origin[0] + f64::from(p[0]) * size[0],
                    origin[1] + f64::from(p[1]) * size[1],
                ]
            };
            let mut lines = Vec::new();
            for row in &g {
                let points: Vec<[f64; 2]> = row.iter().map(|&p| at(p)).collect();
                lines.extend(curve(&points, 8));
            }
            for c in 0..g[0].len() {
                let points: Vec<[f64; 2]> = g.iter().map(|row| at(row[c])).collect();
                lines.extend(curve(&points, 8));
            }
            d.colour(GRID);
            d.width(1.0);
            d.lines(&lines);
        }
        d.handle(center, ps, state.hover || state.drag.is_some());
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    let near = near_handle(pen, center, ps);
    if is(action, kOfxInteractActionPenDown) {
        if !near {
            return kOfxStatReplyDefault;
        }
        state.drag = Some([pen[0] - center[0], pen[1] - center[1]]);
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        match state.drag {
            Some(grip) => {
                let to = [
                    (pen[0] - grip[0] - origin[0]) / size[0],
                    (pen[1] - grip[1] - origin[1]) / size[1],
                ];
                fx.param(CENTER).set_xy(to);
                kOfxStatOK
            }
            None => {
                if near != state.hover {
                    state.hover = near;
                    redraw(handle);
                }
                kOfxStatReplyDefault
            }
        }
    } else if is(action, kOfxInteractActionPenUp) {
        if state.drag.take().is_some() {
            redraw(handle);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else {
        kOfxStatReplyDefault
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(k: [f32; 3], center: [f64; 2], scale: f64, ptlens: bool) -> Settings {
        Settings {
            k,
            center,
            scale,
            ptlens,
            show: false,
        }
    }

    #[test]
    fn the_lenses_are_the_references_list_in_its_order() {
        let names: Vec<&str> = lenses::LENSES.iter().map(|(n, _)| *n).collect();
        // The reference's 639, without its three own action cameras.
        assert_eq!(names.len(), 636);
        assert!(names.windows(2).all(|w| w[0].as_bytes() < w[1].as_bytes()));
        assert_eq!(names[0], "1 Nikkor 10mm f/2.8");
        assert!(names.contains(&"GoPro Hero3 black & compatibles"));
    }

    #[test]
    fn a_search_finds_names_with_its_letters_of_either_case() {
        let found = search("NIKKOR 10mm");
        let names: Vec<&str> = found.iter().map(|&i| lenses::LENSES[i].0).collect();
        assert_eq!(names, ["1 Nikkor 10mm f/2.8"]);
        assert_eq!(search("").len(), lenses::LENSES.len());
        assert!(search("no such lens").is_empty());
    }

    #[test]
    fn without_distortion_each_pixel_reads_its_own_place() {
        let m = settings([0.0; 3], [0.5, 0.5], 1.0, true).model(8, 6);
        assert_eq!(m.at(3, 2), [3.0 / 8.0, 2.0 / 6.0]);
        let m = settings([0.0; 3], [0.5, 0.5], 1.0, false).model(8, 6);
        assert_eq!(m.at(3, 2), [3.0 / 8.0, 2.0 / 6.0]);
    }

    #[test]
    fn the_panotools_model_bends_about_the_center_pixel() {
        // On 100 × 50 about pixel (50, 25), ρ = r / 25: (75, 25) is ρ 1,
        // which d keeps in place; (60, 25) is ρ 0.4.
        let s = settings([0.1, -0.05, 0.02], [0.5, 0.5], 1.0, true);
        let m = s.model(100, 50);
        let kept = m.at(75, 25);
        assert!((kept[0] - 0.75).abs() < 1e-6 && (kept[1] - 0.5).abs() < 1e-6);
        let rho: f64 = 0.4;
        let f = 0.1 * rho.powi(3) - 0.05 * rho * rho + 0.02 * rho + (1.0 - 0.1 + 0.05 - 0.02);
        let p = m.at(60, 25);
        assert!((f64::from(p[0]) - (50.0 + 10.0 * f) / 100.0).abs() < 1e-6);
        // Over Scale.
        let m = settings([0.1, -0.05, 0.02], [0.5, 0.5], 2.0, true).model(100, 50);
        let p = m.at(60, 25);
        assert!((f64::from(p[0]) - (50.0 + 5.0 * f) / 100.0).abs() < 1e-6);
    }

    #[test]
    fn the_polynomial_model_bends_the_pictures_parts() {
        let m = settings([0.2, 0.0, 0.0], [0.5, 0.5], 3.0, false).model(10, 10);
        // u = 0.8 − 0.5 = 0.3, v = 0: f = 1 + 0.2 × 0.09; Scale unused.
        let p = m.at(8, 5);
        assert!((p[0] - (0.5 + 0.3 * 1.018)).abs() < 1e-6);
        assert!((p[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_center_left_of_the_picture_is_its_middle() {
        let m = settings([0.1, 0.0, 0.0], [-0.5, 0.9], 1.0, true).model(10, 10);
        assert_eq!(m.center, [0.5, 0.5]);
        // A part of a pixel left still counts as its first column.
        let m = settings([0.1, 0.0, 0.0], [-0.05, 0.9], 1.0, true).model(10, 10);
        assert_eq!(m.center, [0.0, 0.9]);
    }

    #[test]
    fn reading_weighs_four_pixels_in_1024ths() {
        let src = [
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
        ];
        // Halfway between the four (the scale's w − 1 is one pixel): 256
        // each.
        let c = read(&src, [2, 2], [0.5, 0.5]);
        assert_eq!(c, [0.25, 0.25, 0.25, 1.0]);
        // A quarter of the way each way: 576, 192, 192 and 64.
        let c = read(&src, [2, 2], [0.25, 0.25]);
        assert_eq!(c, [0.1875, 0.1875, 0.0625, 1.0]);
        // A third of the way: weights truncated, a little lost.
        let c = read(&src, [2, 2], [1.0 / 3.0, 0.0]);
        assert_eq!(c[0], 341.0 / 1024.0);
        assert_eq!(c[3], (682.0 + 341.0) / 1024.0);
    }

    #[test]
    fn reading_stops_short_of_the_edges_and_wraps_past_the_last_column() {
        let src = [
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
        ];
        assert_eq!(read(&src, [2, 2], [-0.4, 0.0]), [0.0; 4]);
        assert_eq!(read(&src, [2, 2], [1.4, 0.0]), [0.0; 4]);
        assert_eq!(read(&src, [2, 2], [f32::NAN, 0.0]), [0.0; 4]);
        // Just before the first column: its pixel.
        assert_eq!(read(&src, [2, 2], [-0.35, 0.0]), src[0]);
        // A fifth past the last column of the bottom row reads into the
        // next row's first pixel.
        let c = read(&src, [2, 2], [1.2, 0.0]);
        let right = (0.2f32 * 1024.0) as i32 as f32;
        let own = (0.8f32 * 1024.0) as i32 as f32;
        assert_eq!(c[0], own / 1024.0);
        assert_eq!(c[1], right / 1024.0);
    }

    #[test]
    fn nothing_changes_without_distortion_or_while_showing_it() {
        assert!(settings([0.0; 3], [0.2, 0.7], 1.0, true).changes_nothing());
        assert!(!settings([0.0; 3], [0.5, 0.5], 1.5, true).changes_nothing());
        assert!(!settings([0.1, 0.0, 0.0], [0.5, 0.5], 1.0, true).changes_nothing());
        let mut s = settings([0.1, 0.0, 0.0], [0.5, 0.5], 1.0, true);
        s.show = true;
        assert!(s.changes_nothing());
        let src = Buf::solid([0, 0, 4, 4], [0.5, 0.25, 0.0, 1.0]);
        assert_eq!(corrected(&s, &src).data, src.data);
    }

    #[test]
    fn the_grid_spreads_16_by_9_pixels_of_a_192_by_108_picture() {
        let g = grid(&settings([0.0; 3], [0.5, 0.5], 1.0, true));
        let near =
            |p: [f32; 2], q: [f32; 2]| (p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6;
        assert!(near(g[0][0], [0.0, 0.0]));
        assert!(near(g[1][1], [12.0 / 192.0, 13.0 / 108.0]));
        assert!(near(g[8][15], [191.0 / 192.0, 107.0 / 108.0]));
        // Its PanoTools model times Scale: everything twice as far from
        // the center.
        let g = grid(&settings([0.0; 3], [0.5, 0.5], 2.0, false));
        assert!((g[4][0][0] - (96.0 - 2.0 * 96.0) / 192.0).abs() < 1e-6);
    }

    #[test]
    fn the_curve_runs_through_its_points() {
        let points = [[0.0, 0.0], [10.0, 5.0], [20.0, 0.0], [30.0, 5.0]];
        let lines = curve(&points, 8);
        assert_eq!(lines.len(), 3 * 8 * 2);
        assert_eq!(lines[0], points[0]);
        for (i, p) in points.iter().enumerate().skip(1) {
            let end = lines[(i * 8 - 1) * 2 + 1];
            assert!((end[0] - p[0]).abs() < 1e-9 && (end[1] - p[1]).abs() < 1e-9);
        }
        // Between them it bends: the first stretch rises above its chord's
        // middle.
        let mid = lines[7];
        assert!(mid[1] > 2.5);
    }

    #[test]
    fn the_cards_plan_carries_the_settings() {
        let s = settings([0.1, -0.2, 0.3], [0.25, 0.75], 2.0, true);
        match s.plan() {
            Plan::LensCorrection { settings, ptlens } => {
                assert_eq!(settings, [0.1, -0.2, 0.3, 0.5, 0.25, 0.75]);
                assert!(ptlens);
                // On 8 × 4: the center pixel (2, 3), ρ's unit 2 / 4.
                let n = card_numbers(&settings, ptlens, 8, 4);
                assert_eq!(n[4..9], [0.25, 0.75, 2.0, 3.0, 0.5]);
                assert_eq!(n[9..], [0.125, 0.25, ((1.0f32 - 0.3) + 0.2) - 0.1]);
            }
            p => panic!("{p:?}"),
        }
        assert!(matches!(
            settings([0.0; 3], [0.5, 0.5], 1.0, true).plan(),
            Plan::Same
        ));
    }
}
