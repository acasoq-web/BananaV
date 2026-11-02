//! Smart Zoom, as VEGAS's: its panel — Cutout (where the window is and
//! how far it zooms, 1 to 4), Noise sensitivity, Enable enhancement, and
//! the hidden Alpha mod enabled, External alpha and Enhancement level —
//! and its zoom: a window of 1 ⁄ zoom of the frame around the Cutout's
//! point (rows up, as the frame's 0–1) brought to the frame's size by a
//! separable Lanczos of four lobes, eight taps a pixel — each output
//! pixel's middle read at `(i + 0.5) ⁄ zoom` from the window's start, its
//! first tap ⌊c − 3.5⌋, each weighed by the kernel at the distance of its
//! middle, the weights summing to 1, edges held —, rows first. VEGAS's
//! enhancement is a learned filter on the luma whose tables are not
//! readable; BananaV's lifts the luma's detail (its difference from the
//! binomial 3 × 3 around it) by the Enhancement level (and External alpha)
//! where it stands out of the noise Noise sensitivity allows, the color
//! kept. The Cutout's point is dragged on the preview, its window drawn.
//! On the card `Plan::SmartZoom`: `kit_lanczos_rows`, `kit_lanczos_columns`
//! reading `card_table`'s taps, then `kit_smart_enhance`.
#![allow(unsafe_code)]

use std::f64::consts::PI;
use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::{Plan, Values};
use crate::kit::Buf;
use crate::ofx::{Draw, Fx, Props, interact_props, is, near_handle, redraw, suites};

pub(crate) const ID: &CStr = c"org.bananav.SmartZoom";

const CUTOUT: &CStr = c"LocationAndZoom";
const NOISE: &CStr = c"Noise";
const ALPHA_MOD: &CStr = c"AlphaModEnabled";
const EXTERNAL_ALPHA: &CStr = c"ExternalAlpha";
const LEVEL: &CStr = c"CoeffIndexChoice";
const ENHANCE: &CStr = c"EnableEnhancement";

const LEVELS: [&CStr; 4] = [c"Very Strong", c"Strong", c"Medium", c"Weak"];

/// How strongly each Enhancement level lifts the detail.
const STRENGTHS: [f32; 4] = [1.0, 0.75, 0.5, 0.25];

/// What a frame is zoomed with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    /// The window's middle (0–1 of the frame, rows up) and the zoom.
    cutout: [f64; 3],
    /// The detail lifted (0: none) and the noise it stands out of.
    strength: f32,
    threshold: f32,
}

impl Settings {
    fn make(
        cutout: [f64; 3],
        noise: f64,
        enhance: bool,
        alpha_mod: bool,
        alpha: f64,
        level: c_int,
    ) -> Settings {
        let level = STRENGTHS[level.clamp(0, 3) as usize];
        let alpha = if alpha_mod {
            alpha.clamp(0.0, 1.0) as f32
        } else {
            1.0
        };
        Settings {
            cutout: [cutout[0], cutout[1], cutout[2].clamp(1.0, 4.0)],
            strength: if enhance { level * alpha } else { 0.0 },
            threshold: (noise.clamp(0.0, 2000.0) / 2000.0 * 0.05) as f32,
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        let p = |n: &CStr| fx.param(n);
        Settings::make(
            p(CUTOUT).xyz_at(time),
            p(NOISE).f64_at(time),
            p(ENHANCE).int_at(time) != 0,
            p(ALPHA_MOD).int_at(time) != 0,
            p(EXTERNAL_ALPHA).f64_at(time),
            p(LEVEL).int_at(time),
        )
    }

    pub(crate) fn of_values(v: Values<'_>) -> Settings {
        Settings::make(
            v.array(CUTOUT, [0.5, 0.5, 1.5]),
            v.f64(NOISE, 200.0),
            v.int(ENHANCE, 1) != 0,
            v.int(ALPHA_MOD, 1) != 0,
            v.f64(EXTERNAL_ALPHA, 1.0),
            v.int(LEVEL, 0),
        )
    }

    /// Whether the frame comes out as it went in: the whole frame at zoom 1,
    /// nothing lifted.
    fn changes_nothing(&self) -> bool {
        self.cutout == [0.5, 0.5, 1.0] && self.strength == 0.0
    }

    pub(crate) fn plan(&self) -> Plan {
        if self.changes_nothing() {
            return Plan::Same;
        }
        let [x, y, z] = self.cutout.map(|v| v as f32);
        Plan::SmartZoom {
            numbers: [x, y, z, self.strength, self.threshold],
        }
    }
}

/// Lanczos's kernel of four lobes.
fn lanczos(x: f64) -> f64 {
    const A: f64 = 4.0;
    if x == 0.0 {
        1.0
    } else if x.abs() < A {
        let p = PI * x;
        A * p.sin() * (p / A).sin() / (p * p)
    } else {
        0.0
    }
}

/// An output pixel's first tap and its eight taps' weights.
type Taps = (i32, [f32; 8]);

/// For each of `n` output pixels reading a source from `start` (its
/// pixels) at `scale`: its taps.
fn taps(n: usize, scale: f64, start: f64) -> Vec<Taps> {
    (0..n)
        .map(|i| {
            let c = (i as f64 + 0.5) / scale + start;
            let first = (c - 4.0 + 0.5).floor();
            let w: [f64; 8] = std::array::from_fn(|k| lanczos(first + k as f64 + 0.5 - c));
            let sum: f64 = w.iter().sum();
            (first as i32, w.map(|v| (v / sum) as f32))
        })
        .collect()
}

/// The columns' and the rows' taps (rows up) of a `w` × `h` frame.
fn window(cutout: [f64; 3], [w, h]: [usize; 2]) -> (Vec<Taps>, Vec<Taps>) {
    let [x, y, z] = cutout;
    let (fw, fh) = (w as f64, h as f64);
    (
        taps(w, z, x * fw - fw / (2.0 * z)),
        taps(h, z, y * fh - fh / (2.0 * z)),
    )
}

/// The taps as the card reads them: each column's first tap and weights
/// (three entries), then each row's from the top.
pub(crate) fn card_table(numbers: &[f32; 5], w: u32, h: u32) -> Vec<[f32; 4]> {
    let cutout = [0, 1, 2].map(|i| f64::from(numbers[i]));
    let (columns, rows) = window(cutout, [w as usize, h as usize]);
    let entry = |(first, wt): &Taps| {
        [
            [*first as f32, 0.0, 0.0, 0.0],
            [wt[0], wt[1], wt[2], wt[3]],
            [wt[4], wt[5], wt[6], wt[7]],
        ]
    };
    columns
        .iter()
        .flat_map(entry)
        .chain(rows.iter().rev().flat_map(entry))
        .collect()
}

/// The luma's detail lifted where it stands out of the noise.
fn enhanced(src: &Buf, strength: f32, threshold: f32) -> Buf {
    let (w, h) = (src.width().max(0) as usize, src.height().max(0) as usize);
    let luma: Vec<f32> = src
        .data
        .iter()
        .map(|p| p[0] * 0.299 + p[1] * 0.587 + p[2] * 0.114)
        .collect();
    let at = |x: isize, y: isize| {
        luma[y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize]
    };
    let [bx, by] = [src.bounds[0], src.bounds[1]];
    src.map(|x, y| {
        let (i, j) = ((x - bx) as isize, (y - by) as isize);
        let row = |j: isize| at(i - 1, j) + at(i, j) * 2.0 + at(i + 1, j);
        let blur = (row(j - 1) + row(j) * 2.0 + row(j + 1)) * 0.0625;
        let d = at(i, j) - blur;
        let t = ((d.abs() - threshold) / threshold.max(1e-6)).clamp(0.0, 1.0);
        let lift = d * strength * (t * t * (3.0 - 2.0 * t));
        let c = src.data[j as usize * w + i as usize];
        [c[0] + lift, c[1] + lift, c[2] + lift, c[3]]
    })
}

/// The frame zoomed into its window.
fn zoomed(source: &Buf, s: &Settings) -> Buf {
    let (w, h) = (
        source.width().max(0) as usize,
        source.height().max(0) as usize,
    );
    if w == 0 || h == 0 || s.changes_nothing() {
        return source.clone();
    }
    let (columns, rows) = window(s.cutout, [w, h]);
    let held = |v: i32, n: usize| v.clamp(0, n as i32 - 1) as usize;
    let mut across = source.clone();
    across.rows(|y, row| {
        let j = (y - source.bounds[1]) as usize;
        let line = &source.data[j * w..(j + 1) * w];
        for (px, (first, wt)) in row.iter_mut().zip(&columns) {
            let mut acc = [0.0f32; 4];
            for (k, &wk) in wt.iter().enumerate() {
                let p = line[held(first + k as i32, w)];
                for c in 0..4 {
                    acc[c] += wk * p[c];
                }
            }
            *px = acc;
        }
    });
    let mut out = across.clone();
    out.rows(|y, row| {
        let (first, wt) = rows[(y - source.bounds[1]) as usize];
        for (x, px) in row.iter_mut().enumerate() {
            let mut acc = [0.0f32; 4];
            for (k, &wk) in wt.iter().enumerate() {
                let p = across.data[held(first + k as i32, h) * w + x];
                for c in 0..4 {
                    acc[c] += wk * p[c];
                }
            }
            *px = acc;
        }
    });
    if s.strength > 0.0 {
        enhanced(&out, s.strength, s.threshold)
    } else {
        out
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
        let time = args.dbl(kOfxPropTime, 0);
        if Settings::at(fx, time).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
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
    p.set_str(kOfxPropLabel, c"Smart Zoom");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Utility");
    p.set_str(kOfxPropPluginDescription, c"High quality zoom.");
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
    // The cutout's point and window on the preview.
    p.set_ptr(
        kOfxImageEffectPluginPropOverlayInteractV2,
        overlay as *mut c_void,
    );
}

fn describe_in_context(fx: Fx) {
    let comps = [kOfxImageComponentRGBA];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);
    let p = fx.define_param(kOfxParamTypeDouble3D, CUTOUT, c"Cutout");
    p.set_dbls(kOfxParamPropDefault, &[0.5, 0.5, 1.5]);
    p.set_dbls(kOfxParamPropMin, &[0.0, 0.0, 1.0]);
    p.set_dbls(kOfxParamPropMax, &[1.0, 1.0, 4.0]);
    p.set_dbls(kOfxParamPropDisplayMin, &[0.0, 0.0, 1.0]);
    p.set_dbls(kOfxParamPropDisplayMax, &[1.0, 1.0, 4.0]);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(kOfxParamPropHint, c"The location and zoom of the cut out.");
    let p = fx.define_param(kOfxParamTypeDouble, NOISE, c"Noise sensitivity");
    p.set_dbl(kOfxParamPropDefault, 200.0);
    p.set_dbl(kOfxParamPropMin, 0.0);
    p.set_dbl(kOfxParamPropMax, 2000.0);
    p.set_dbl(kOfxParamPropDisplayMin, 0.0);
    p.set_dbl(kOfxParamPropDisplayMax, 2000.0);
    p.set_str(kOfxParamPropHint, c"The noise level of the image.");
    let p = fx.define_param(kOfxParamTypeBoolean, ALPHA_MOD, c"Alpha mod enabled");
    p.set_int(kOfxParamPropDefault, 1);
    p.set_int(kOfxParamPropSecret, 1);
    p.set_str(
        kOfxParamPropHint,
        c"Select to enable SPICA 'Aplha Mod' processing.",
    );
    let p = fx.define_param(kOfxParamTypeDouble, EXTERNAL_ALPHA, c"External alpha");
    p.set_dbl(kOfxParamPropDefault, 1.0);
    p.set_dbl(kOfxParamPropMin, 0.0);
    p.set_dbl(kOfxParamPropMax, 1.0);
    p.set_int(kOfxParamPropSecret, 1);
    p.set_str(kOfxParamPropHint, c"Sets the overall 'Alpha Mod' level.");
    let p = fx.define_param(kOfxParamTypeChoice, LEVEL, c"Enhancement level");
    p.set_strs(kOfxParamPropChoiceOption, &LEVELS);
    p.set_int(kOfxParamPropDefault, 0);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_int(kOfxParamPropSecret, 1);
    p.set_str(
        kOfxParamPropHint,
        c"Choose which set of coefficients to use with the SPICA algorithm",
    );
    let p = fx.define_param(kOfxParamTypeBoolean, ENHANCE, c"Enable enhancement");
    p.set_int(kOfxParamPropDefault, 1);
    p.set_str(
        kOfxParamPropHint,
        c"Enable or disable the smart enhancement.",
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
    let result = zoomed(&source, &Settings::at(fx, time));
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

// The cutout on the preview.

/// What the overlay remembers: the point taken (where the pen took it,
/// from the point) or a corner of the window, and what is under the pen.
#[derive(Debug, Default)]
struct Overlay {
    drag: Option<Grab>,
    hover: bool,
}

#[derive(Clone, Copy, Debug)]
enum Grab {
    Point([f64; 2]),
    Corner,
}

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
    let (origin, frame) = fx.frame();
    let [x, y, z] = fx.param(CUTOUT).xyz_at(time);
    let z = z.clamp(1.0, 4.0);
    let at = [origin[0] + x * frame[0], origin[1] + y * frame[1]];
    let half = [frame[0] / (2.0 * z), frame[1] / (2.0 * z)];
    let corners = [
        [at[0] - half[0], at[1] - half[1]],
        [at[0] + half[0], at[1] - half[1]],
        [at[0] + half[0], at[1] + half[1]],
        [at[0] - half[0], at[1] + half[1]],
    ];
    if is(action, kOfxInteractActionDraw) {
        if let Some(d) = Draw::of(args) {
            let hot = state.hover || state.drag.is_some();
            for (colour, width) in [([0.0, 0.0, 0.0, 0.8], 3.0), ([1.0, 1.0, 1.0, 1.0], 1.4)] {
                d.colour(colour);
                d.width(width);
                d.outline(&corners);
            }
            d.handle(at, ps, hot);
        }
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    let near = near_handle(pen, at, ps);
    let corner = corners.iter().any(|&c| near_handle(pen, c, ps));
    if is(action, kOfxInteractActionPenDown) {
        state.drag = if near {
            Some(Grab::Point([pen[0] - at[0], pen[1] - at[1]]))
        } else if corner {
            Some(Grab::Corner)
        } else {
            return kOfxStatReplyDefault;
        };
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        match state.drag {
            Some(Grab::Point(grip)) => {
                let to = [
                    ((pen[0] - grip[0] - origin[0]) / frame[0]).clamp(0.0, 1.0),
                    ((pen[1] - grip[1] - origin[1]) / frame[1]).clamp(0.0, 1.0),
                ];
                fx.param(CUTOUT).set_xyz([to[0], to[1], z]);
                kOfxStatOK
            }
            Some(Grab::Corner) => {
                // The window's corner under the pen: its zoom.
                let reach = ((pen[0] - at[0]).abs() / (frame[0] / 2.0))
                    .max((pen[1] - at[1]).abs() / (frame[1] / 2.0))
                    .max(0.25);
                fx.param(CUTOUT)
                    .set_xyz([x, y, (1.0 / reach).clamp(1.0, 4.0)]);
                kOfxStatOK
            }
            None => {
                let over = near || corner;
                if over != state.hover {
                    state.hover = over;
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

    fn buf(w: usize, h: usize, f: impl Fn(usize, usize) -> f32) -> Buf {
        Buf {
            bounds: [0, 0, w as c_int, h as c_int],
            data: (0..w * h)
                .map(|i| [f(i % w, i / w), 0.0, 0.0, 1.0])
                .collect(),
        }
    }

    #[test]
    fn the_weights_sum_to_one_and_zoom_one_reads_each_pixel() {
        for (first, w) in taps(7, 1.0, 0.0) {
            let sum: f32 = w.iter().sum();
            assert!((sum - 1.0).abs() < 1e-6);
            assert!((w[3] - 1.0).abs() < 1e-6 && first >= -3, "{w:?}");
        }
        let s = Settings::make([0.5, 0.5, 1.0], 200.0, false, true, 1.0, 0);
        assert!(s.changes_nothing());
    }

    #[test]
    fn a_ramp_stays_a_ramp_zoomed_about_its_middle() {
        let src = buf(32, 8, |x, _| x as f32);
        let s = Settings::make([0.5, 0.5, 2.0], 200.0, false, true, 1.0, 0);
        let out = zoomed(&src, &s);
        // Twice as steep about the middle: pixel 16's middle reads 16.25 - 0.5
        // (Lanczos's lobes keep a ramp within a few hundredths).
        let at = |x: usize| out.data[3 * 32 + x][0];
        assert!((at(16) - 15.75).abs() < 0.02, "{}", at(16));
        assert!((at(17) - at(16) - 0.5).abs() < 0.04);
    }

    #[test]
    fn the_enhancement_lifts_edges_not_noise() {
        let edge = buf(16, 16, |x, _| if x < 8 { 0.2 } else { 0.8 });
        let out = enhanced(&edge, 1.0, 0.01);
        assert!(
            out.data[7][0] < 0.2 && out.data[8][0] > 0.8,
            "{:?}",
            &out.data[6..10]
        );
        let faint = buf(16, 16, |x, _| if x < 8 { 0.5 } else { 0.505 });
        let out = enhanced(&faint, 1.0, 0.01);
        assert!(
            (out.data[7][0] - 0.5).abs() < 1e-6,
            "{:?}",
            &out.data[6..10]
        );
    }
}
