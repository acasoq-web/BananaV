//! The optical flow effects, as VEGAS 2026's Help describes them: Slow
//! Motion (the frames between the media's made along the optical flow,
//! Playback Rate animatable, Blend or Nearest Frame, Show Optical Flow
//! Field, and Adjust Event Length, which asks the host through BananaV's
//! `kBananaVParamPropEventLength`), Motion Blur (VEGAS's panel and math,
//! `motion_blur.rs`: the moving areas within Motion Threshold's range
//! blurred along the frame's mean motion, with its visualizations) and
//! the Warp Flow transition (the
//! first picture flowing into the second). The flow is `bv-track`'s dense
//! Lucas–Kanade, worked out on the frames made small (Quality).
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::{Arc, Mutex};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;
use bv_track::Gray;
use bv_track::flow::{Flow, FlowQuality, dense_flow};

use crate::kit::Buf;
use crate::ofx::{Fx, Props, is, suites};

pub(crate) const SLOW_ID: &CStr = c"org.bananav.SlowMotion";
pub(crate) const BLUR_ID: &CStr = c"org.bananav.MotionBlur";
pub(crate) const WARP_ID: &CStr = c"org.bananav.WarpFlow";
const _: () = assert!(WARP_ID.to_bytes().len() == crate::WARP_FLOW_ID.len());

const QUALITY: &CStr = c"quality";
const PARAMETERS: &CStr = c"parameters";
const METHOD: &CStr = c"method";
const BLENDING: &CStr = c"blending";
const RATE: &CStr = c"playbackRate";
const SHOW_FLOW: &CStr = c"showFlow";
const ADJUST: &CStr = c"adjustLength";
const MODE: &CStr = c"blurMode";
const MIN: &CStr = c"thresholdMin";
const MAX: &CStr = c"thresholdMax";
const AMOUNT: &CStr = c"blurAmount";
const VISUALIZATION: &CStr = c"visualization";
const SHOW: &CStr = c"showVisualization";

const METHODS: [&CStr; 2] = [c"Morph", c"Optical Flow"];
const BLENDINGS: [&CStr; 2] = [c"Blend", c"Nearest Frame"];
const SHOWS: [&CStr; 4] = [
    c"Off",
    c"Thresholded Optical Flow Field",
    c"Blur Mask",
    c"Highlight Motion Blur Area",
];

fn quality(i: c_int) -> FlowQuality {
    match i {
        0 => FlowQuality::Low,
        2 => FlowQuality::High,
        _ => FlowQuality::Medium,
    }
}

/// A picture made small for the flow: its luma, rows as the buffer's, and
/// how many of the buffer's pixels each of its pixels is.
fn small(b: &Buf, width: usize) -> (Gray, f32) {
    let (bw, bh) = (b.width().max(1) as usize, b.height().max(1) as usize);
    let k = (bw as f32 / width.max(16) as f32).max(1.0);
    let (w, h) = (
        ((bw as f32) / k).round().max(2.0) as usize,
        ((bh as f32) / k).round().max(2.0) as usize,
    );
    let mut data = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let c = b.bilinear(
                b.bounds[0] as f64 + (x as f64 + 0.5) * k as f64 - 0.5,
                b.bounds[1] as f64 + (y as f64 + 0.5) * k as f64 - 0.5,
            );
            data.push(crate::kit::luma(c));
        }
    }
    (
        Gray {
            width: w,
            height: h,
            data,
        },
        k,
    )
}

/// A flow and the scale of the frames it was worked out on.
type Scaled = Arc<(Flow, f32)>;

/// The last flow worked out (consecutive frames often ask for the same).
static FLOW: Mutex<Option<(u64, Scaled)>> = Mutex::new(None);

/// The flow from `a` to `b` (the same size), worked out at `q`; `key`
/// tells the pairs apart.
fn flow_of(a: &Buf, b: &Buf, q: FlowQuality, key: u64) -> Scaled {
    if let Ok(f) = FLOW.lock()
        && let Some((k, flow)) = &*f
        && *k == key
    {
        return flow.clone();
    }
    let (ga, k) = small(a, q.width());
    let (gb, _) = small(b, q.width());
    let made = Arc::new((dense_flow(&ga, &gb, q), k));
    if let Ok(mut f) = FLOW.lock() {
        *f = Some((key, made.clone()));
    }
    made
}

/// A key for a pair of frames of an instance.
fn key(fx: Fx, a: f64, b: f64, q: FlowQuality, size: (c_int, c_int)) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (fx.0 as usize, a.to_bits(), b.to_bits(), q as u8, size).hash(&mut h);
    h.finish()
}

/// The move at buffer pixel (`x`, `y`), in buffer pixels.
fn move_at(f: &(Flow, f32), b: &Buf, x: f64, y: f64) -> [f64; 2] {
    let k = f.1 as f64;
    let m = f.0.at(
        ((x - b.bounds[0] as f64 + 0.5) / k - 0.5) as f32,
        ((y - b.bounds[1] as f64 + 0.5) / k - 0.5) as f32,
    );
    [m[0] as f64 * k, m[1] as f64 * k]
}

/// VEGAS's frame `t` of the way from `a` to `b`, `f` the motion back from
/// `b` to `a`: `a` read `t` of the move along, `b` the rest of it back,
/// mixed (Blend) or the nearer one (Nearest Frame); where one is
/// transparent, the other alone.
fn made_between(a: &Buf, b: &Buf, f: &(Flow, f32), t: f64, nearest: bool) -> Buf {
    a.map(|x, y| {
        let (px, py) = (x as f64, y as f64);
        let m = move_at(f, b, px, py);
        let ca = a.bilinear(px + t * m[0], py + t * m[1]);
        let cb = b.bilinear(px - (1.0 - t) * m[0], py - (1.0 - t) * m[1]);
        if ca[3] <= 1e-6 {
            cb
        } else if cb[3] <= 1e-6 {
            ca
        } else if nearest {
            if t <= 0.5 { ca } else { cb }
        } else {
            crate::kit::mix(ca, cb, t)
        }
    })
}

/// The picture `t` of the way from `a` to `b` along the flow: each
/// pixel's color from `a` back along its move, from `b` forward, mixed.
#[cfg(test)]
fn between(a: &Buf, b: &Buf, f: &(Flow, f32), t: f64) -> Buf {
    a.map(|x, y| {
        let (px, py) = (x as f64, y as f64);
        let m = move_at(f, a, px, py);
        let ca = a.bilinear(px - t * m[0], py - t * m[1]);
        let cb = b.bilinear(px + (1.0 - t) * m[0], py + (1.0 - t) * m[1]);
        crate::kit::mix(ca, cb, t)
    })
}

/// A move drawn as a color: its direction the hue (up red, right a
/// greenish yellow, down cyan, left violet), its length the brightness
/// (`full` pixels bright).
fn flow_color(m: [f64; 2], full: f64) -> [f32; 4] {
    let len = m[0].hypot(m[1]);
    let v = (len / full.max(1e-6)).min(1.0);
    // 0° up, going clockwise (the buffer's rows go up).
    let angle = m[0].atan2(m[1]).to_degrees().rem_euclid(360.0);
    let h = angle / 60.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as i32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    [(r * v) as f32, (g * v) as f32, (b * v) as f32, 1.0]
}

/// Where Slow Motion's output frame `t` is in its source: the playback
/// rate summed frame by frame.
fn source_time(fx: Fx, t: f64) -> f64 {
    let rate = fx.param(RATE);
    summed_rate(&|k| rate.f64_at(k), t)
}

/// [`source_time`] with the playback rate at a frame `rate(frame)`.
pub(crate) fn summed_rate(rate: &dyn Fn(f64) -> f64, t: f64) -> f64 {
    let whole = t.floor().max(0.0) as i64;
    let mut s = 0.0;
    for k in 0..whole.min(100_000) {
        s += rate(k as f64).max(0.0);
    }
    s + (t - whole as f64) * rate(whole as f64).max(0.0)
}

/// The flow back from `second` to `first` at quality `q` as the card
/// reads it (`kit_flow_between`): its cells across and up, each cell's
/// move — in the flow's pixels, rows up — as the bits of two floats, and
/// how many of the pictures' pixels a cell is.
pub(crate) fn flow_table(
    first: &Buf,
    second: &Buf,
    q: c_int,
    key: u64,
) -> ([u32; 2], Vec<[i32; 4]>, f32) {
    let f = flow_of(second, first, quality(q), key);
    let table =
        f.0.dx
            .iter()
            .zip(&f.0.dy)
            .map(|(&dx, &dy)| [dx.to_bits() as i32, dy.to_bits() as i32, 0, 0])
            .collect();
    ([f.0.width as u32, f.0.height as u32], table, f.1)
}

/// Slow Motion's settings in `values` (a time's, as the card draws it):
/// its quality, Nearest Frame and Show Optical Flow Field.
pub(crate) fn slow_settings_of(values: crate::accel::Values<'_>) -> (c_int, bool, bool) {
    (
        values.int(QUALITY, 1),
        values.int(BLENDING, 0) == 1,
        values.int(SHOW_FLOW, 0) != 0,
    )
}

/// Warp Flow's Nearest Frame in `values`.
pub(crate) fn warp_nearest_of(values: crate::accel::Values<'_>) -> bool {
    values.int(BLENDING, 0) == 1
}

/// Slow Motion's playback rate's name.
pub(crate) const RATE_NAME: &str = "playbackRate";

fn describe(fx: Fx, label: &CStr, description: &CStr, transition: bool) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, label);
    p.set_str(
        kOfxImageEffectPluginPropGrouping,
        if transition {
            c"BananaV/Flow"
        } else {
            c"BananaV/Time"
        },
    );
    p.set_str(kOfxPropPluginDescription, description);
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        if transition {
            &[kOfxImageEffectContextTransition][..]
        } else {
            &[kOfxImageEffectContextFilter, kOfxImageEffectContextGeneral][..]
        },
    );
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    p.set_int(kOfxImageEffectPropSupportsTiles, 0);
    p.set_int(kOfxImageEffectPropSupportsMultiResolution, 1);
    p.set_int(
        kOfxImageEffectPropTemporalClipAccess,
        i32::from(!transition),
    );
    p.set_int(kOfxImageEffectPluginPropHostFrameThreading, 0);
    p.set_str(
        kOfxImageEffectPluginRenderThreadSafety,
        kOfxImageEffectRenderFullySafe,
    );
}

fn choice(fx: Fx, name: &CStr, label: &CStr, options: &[&CStr], default: c_int, hint: &CStr) {
    let p = fx.define_param(kOfxParamTypeChoice, name, label);
    p.set_strs(kOfxParamPropChoiceOption, options);
    p.set_int(kOfxParamPropDefault, default);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(kOfxParamPropHint, hint);
}

/// The whole of the source, wherever a pixel may come from.
fn whole_source(fx: Fx, args: Props, out: Props, clips: &[&CStr]) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
    for c in clips {
        let all = fx.clip(c).rod(time).unwrap_or(roi);
        let name = format!("OfxImageClipPropRoI_{}", c.to_string_lossy());
        if let Ok(name) = std::ffi::CString::new(name) {
            out.set_dbls(&name, &all);
        }
    }
    kOfxStatOK
}

// ---- Slow Motion -------------------------------------------------------------------------------

/// Slow Motion's entry point.
pub(crate) unsafe extern "C" fn slow_main(
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
        describe(
            fx,
            c"Slow Motion",
            c"Slows the video down (or speeds it up) smoothly: the frames between the media's are made along the optical flow.",
            false,
        );
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        slow_params(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        slow_render(fx, args)
    } else if is(action, kOfxImageEffectActionGetFramesNeeded) {
        let s = source_time(fx, args.dbl(kOfxPropTime, 0));
        out.set_dbls(
            c"OfxImageClipPropFrameRange_Source",
            &[s.floor(), s.floor() + 1.0],
        );
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        whole_source(fx, args, out, &[kOfxImageEffectSimpleSourceClipName])
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

fn slow_params(fx: Fx) {
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps)
        .set_int(kOfxImageEffectPropTemporalClipAccess, 1);
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);
    // VEGAS's panel in its real-time mode: the Parameter group, then
    // Visualization and Adjust Event Length.
    let g = fx.define_param(kOfxParamTypeGroup, PARAMETERS, c"Parameter");
    g.set_int(kOfxParamPropGroupOpen, 1);
    g.set_str(kOfxParamPropHint, c"Groups the individual parameters.");
    let p = fx.define_param(kOfxParamTypeChoice, QUALITY, c"Quality");
    p.set_strs(kOfxParamPropChoiceOption, &BLUR_QUALITIES);
    p.set_int(kOfxParamPropDefault, 1);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(kOfxParamPropParent, PARAMETERS);
    p.set_str(
        kOfxParamPropHint,
        c"How finely the frames between are made: finer is more precise and slower.",
    );
    let p = fx.define_param(kOfxParamTypeChoice, BLENDING, c"Blending");
    p.set_strs(kOfxParamPropChoiceOption, &BLENDINGS);
    p.set_int(kOfxParamPropDefault, 0);
    p.set_str(kOfxParamPropParent, PARAMETERS);
    p.set_str(
        kOfxParamPropHint,
        c"Blend mixes the two frames around each one made; Nearest Frame takes the nearer one, moved along the motion.",
    );
    let p = fx.define_param(kOfxParamTypeDouble, RATE, c"Playback Rate");
    p.set_dbl(kOfxParamPropDefault, 1.0);
    p.set_dbl(kOfxParamPropMin, 0.0);
    p.set_dbl(kOfxParamPropMax, 100.0);
    p.set_dbl(kOfxParamPropDisplayMin, 0.0);
    p.set_dbl(kOfxParamPropDisplayMax, 10.0);
    p.set_dbl(kOfxParamPropIncrement, 0.1);
    p.set_int(kOfxParamPropDigits, 2);
    p.set_str(kOfxParamPropParent, PARAMETERS);
    p.set_str(
        kOfxParamPropHint,
        c"How fast the clip plays: under 1 slower, over 1 faster. It may be animated.",
    );
    let g = fx.define_param(kOfxParamTypeGroup, VISUALIZATION, c"Visualization");
    g.set_int(kOfxParamPropGroupOpen, 0);
    g.set_str(kOfxParamPropHint, c"Groups the visualization parameters.");
    let p = fx.define_param(kOfxParamTypeBoolean, SHOW_FLOW, c"Show Optical Flow Field");
    p.set_int(kOfxParamPropDefault, 0);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(kOfxParamPropParent, VISUALIZATION);
    p.set_str(
        kOfxParamPropHint,
        c"Shows the motion found instead of the picture: its direction as the color, its speed as the brightness.",
    );
    let p = fx.define_param(kOfxParamTypePushButton, ADJUST, c"Adjust Event Length");
    p.set_str(
        kOfxParamPropHint,
        c"Changes the event's length to make up for the Playback Rate's change.",
    );
    p.set_str(bv_ofx_sys::extensions::kBananaVParamPropEventLength, RATE);
}

fn slow_render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let clip = fx.clip(kOfxImageEffectSimpleSourceClipName);
    let s = source_time(fx, time);
    let (s0, frac) = (s.floor(), s - s.floor());
    let nearest = fx.param(BLENDING).int_at(time) == 1;
    let show = fx.param(SHOW_FLOW).int_at(time) != 0;
    let read = |t: f64| clip.image(t).map(|i| Buf::read(Some(&i)));
    // VEGAS's: a whole source frame is shown as it is.
    let result = if frac < 1e-9 && !show {
        read(s0)
    } else {
        let a = read(s0);
        let b = read(s0 + 1.0);
        match (a, b) {
            (Some(a), Some(b)) if b.bounds == a.bounds => {
                let q = quality(fx.param(QUALITY).int_at(time));
                // The motion back from the later frame to the earlier.
                let f = flow_of(&b, &a, q, key(fx, s0 + 1.0, s0, q, (a.width(), a.height())));
                if show {
                    let full = a.width() as f64 * 0.05;
                    Some(b.map(|x, y| flow_color(move_at(&f, &b, x as f64, y as f64), full)))
                } else {
                    Some(made_between(&a, &b, &f, frac, nearest))
                }
            }
            (a, _) => a,
        }
    };
    let Some(result) = result else {
        return kOfxStatFailed;
    };
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

// ---- Motion Blur -------------------------------------------------------------------------------

/// Motion Blur's parameters beyond the others' (VEGAS's panel).
const MAGNIFIER: &CStr = c"blurMagnifier";
const MOST: &CStr = c"blurMax";

/// Motion Blur's Quality and Blur Mode, VEGAS's lists.
const BLUR_QUALITIES: [&CStr; 3] = [c"Coarse", c"Medium", c"Fine"];
const BLUR_MODES: [&CStr; 2] = [c"Fixed Blur", c"Motion-adaptive Blur"];

/// Motion Blur's entry point.
pub(crate) unsafe extern "C" fn blur_main(
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
        describe(
            fx,
            c"Motion Blur",
            c"Puts motion blur on the moving areas, along the frame's motion.",
            false,
        );
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        blur_params(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        blur_render(fx, args)
    } else if is(action, kOfxImageEffectActionGetFramesNeeded) {
        let t = args.dbl(kOfxPropTime, 0);
        out.set_dbls(c"OfxImageClipPropFrameRange_Source", &[t - 1.0, t + 1.0]);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        whole_source(fx, args, out, &[kOfxImageEffectSimpleSourceClipName])
    } else if is(action, kOfxActionInstanceChanged) {
        show_blur_amounts(fx, args.dbl(kOfxPropTime, 0));
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) || is(action, kOfxActionBeginInstanceEdit) {
        show_blur_amounts(fx, 0.0);
        kOfxStatOK
    } else if is(action, kOfxActionLoad)
        || is(action, kOfxActionUnload)
        || is(action, kOfxActionDestroyInstance)
    {
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

/// Blur Mode's rule, VEGAS's: Fixed Blur shows its Blur Amount,
/// Motion-adaptive Blur its own and the maximum.
fn show_blur_amounts(fx: Fx, time: f64) {
    let adaptive = fx.param(MODE).int_at(time) == 1;
    fx.param(AMOUNT).set_secret(adaptive);
    fx.param(MAGNIFIER).set_secret(!adaptive);
    fx.param(MOST).set_secret(!adaptive);
}

fn blur_params(fx: Fx) {
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps)
        .set_int(kOfxImageEffectPropTemporalClipAccess, 1);
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);
    choice(
        fx,
        QUALITY,
        c"Quality",
        &BLUR_QUALITIES,
        1,
        c"How finely the motion is worked out: finer is more precise and slower.",
    );
    let p = fx.define_param(kOfxParamTypeChoice, MODE, c"Blur Mode");
    p.set_strs(kOfxParamPropChoiceOption, &BLUR_MODES);
    p.set_int(kOfxParamPropDefault, 0);
    p.set_str(
        kOfxParamPropHint,
        c"How long the blur is: fixed, or by how far the moving areas move.",
    );
    let amount = |name: &CStr,
                  label: &CStr,
                  (default, min, max): (f64, f64, f64),
                  [lo, hi]: [f64; 2],
                  hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, default);
        p.set_dbl(kOfxParamPropMin, min);
        p.set_dbl(kOfxParamPropMax, max);
        p.set_dbl(kOfxParamPropDisplayMin, lo);
        p.set_dbl(kOfxParamPropDisplayMax, hi);
        p.set_dbl(kOfxParamPropIncrement, 0.1);
        p.set_int(kOfxParamPropDigits, 2);
        p.set_str(kOfxParamPropHint, hint);
        p
    };
    amount(
        MIN,
        c"Motion Threshold (min)",
        (0.5, 0.0, 100.0),
        [0.0, 10.0],
        c"An area must move more than this (a percentage of the frame's width a frame) to be blurred.",
    );
    amount(
        MAX,
        c"Motion Threshold (max)",
        (100.0, 0.1, 100.0),
        [0.1, 100.0],
        c"An area must move less than this (a percentage of the frame's width a frame) to be blurred.",
    );
    amount(
        AMOUNT,
        c"Blur Amount",
        (2.0, 1.0, 100.0),
        [1.0, 20.0],
        c"How long the blur is on the moving areas, in thousandths of the frame's width.",
    );
    amount(
        MAGNIFIER,
        c"Blur Amount",
        (1.0, 0.1, 50.0),
        [0.1, 4.0],
        c"How long the blur is, times how far the moving areas move.",
    )
    .set_int(kOfxParamPropSecret, 1);
    amount(
        MOST,
        c"Maximum Blur Amount",
        (10.0, 1.0, 100.0),
        [1.0, 20.0],
        c"The longest the blur gets, in thousandths of the frame's width.",
    )
    .set_int(kOfxParamPropSecret, 1);
    let g = fx.define_param(kOfxParamTypeGroup, VISUALIZATION, c"Visualization");
    g.set_int(kOfxParamPropGroupOpen, 0);
    let p = fx.define_param(kOfxParamTypeChoice, SHOW, c"Show Visualization");
    p.set_strs(kOfxParamPropChoiceOption, &SHOWS);
    p.set_int(kOfxParamPropDefault, 0);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(kOfxParamPropParent, VISUALIZATION);
    p.set_str(
        kOfxParamPropHint,
        c"Helps set the thresholds: the motion as colors, where the blur goes, or the rest darkened. Turn it off before rendering.",
    );
}

/// Motion Blur's settings at a time.
fn blur_settings(fx: Fx, time: f64) -> crate::motion_blur::Settings {
    let v = |name| fx.param(name).f64_at(time);
    crate::motion_blur::Settings {
        adaptive: fx.param(MODE).int_at(time) == 1,
        range: [v(MIN), v(MAX)],
        strength: v(AMOUNT),
        magnifier: v(MAGNIFIER),
        most: v(MOST),
    }
}

/// Motion Blur's settings in `values` (a time's, as the card draws it):
/// its quality and visualization too.
pub(crate) fn blur_settings_of(
    values: crate::accel::Values<'_>,
) -> (crate::motion_blur::Settings, c_int, c_int) {
    let f = |name, d| values.f64(name, d);
    (
        crate::motion_blur::Settings {
            adaptive: values.int(MODE, 0) == 1,
            range: [f(MIN, 0.5), f(MAX, 100.0)],
            strength: f(AMOUNT, 2.0),
            magnifier: f(MAGNIFIER, 1.0),
            most: f(MOST, 10.0),
        },
        values.int(QUALITY, 1),
        values.int(SHOW, 0),
    )
}

/// The motion of `first` to `second` (the frame and the next, or the one
/// before and the frame) at quality `q`, as the blur reads it: each
/// pixel's (from the lower left), in pixels.
pub(crate) fn blur_motion(
    first: &Buf,
    second: &Buf,
    q: c_int,
    key: u64,
) -> impl Fn(usize, usize) -> [f64; 2] + Sync {
    let f = flow_of(first, second, quality(q), key);
    let (x0, y0) = (first.bounds[0] as f64, first.bounds[1] as f64);
    let first = first.clone();
    move |x, y| move_at(&f, &first, x0 + x as f64, y0 + y as f64)
}

fn blur_render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let clip = fx.clip(kOfxImageEffectSimpleSourceClipName);
    let read = |t: f64| clip.image(t).map(|i| Buf::read(Some(&i)));
    let Some(cur) = read(time) else {
        return kOfxStatFailed;
    };
    let q = fx.param(QUALITY).int_at(time);
    let size = (cur.width(), cur.height());
    // VEGAS's: the frame to the next, else the one before to the frame.
    let pair = match read(time + 1.0).filter(|b| b.bounds == cur.bounds) {
        Some(next) => Some((cur.clone(), next, time, time + 1.0)),
        None => read(time - 1.0)
            .filter(|b| b.bounds == cur.bounds)
            .map(|prev| (prev, cur.clone(), time - 1.0, time)),
    };
    let result = match pair {
        None => cur,
        Some((first, second, a, b)) => {
            let motion = blur_motion(&first, &second, q, key(fx, a, b, quality(q), size));
            let s = blur_settings(fx, time);
            let (w, h) = (size.0.max(0) as usize, size.1.max(0) as usize);
            let show = fx.param(SHOW).int_at(time);
            if show == 0 {
                match s.blur(w, h, &motion) {
                    Some(blur) => crate::motion_blur::apply(&cur, &blur),
                    None => cur,
                }
            } else {
                blur_shown(&cur, &s, show, &motion)
            }
        }
    };
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

/// Motion Blur's visualizations: the motion within the thresholds as
/// colors (black elsewhere), the mask spread by the kernel in white, or
/// what is not blurred darkened.
fn blur_shown(
    cur: &Buf,
    s: &crate::motion_blur::Settings,
    show: c_int,
    motion: &(dyn Fn(usize, usize) -> [f64; 2] + Sync),
) -> Buf {
    let (w, h) = (cur.width().max(0) as usize, cur.height().max(0) as usize);
    let blur = s.blur(w, h, motion);
    let (x0, y0) = (cur.bounds[0], cur.bounds[1]);
    let width = w.max(1) as f64;
    let reach = width * s.range[0].max(s.range[1]).max(0.1) / 100.0;
    let spread = blur.as_ref().map(|b| crate::motion_blur::spread(cur, b));
    cur.map(|x, y| {
        let (i, j) = ((x - x0) as usize, (y - y0) as usize);
        let masked = blur.as_ref().is_some_and(|b| b.mask[j * w + i]);
        let blurred = spread.as_ref().is_some_and(|m| m[j * w + i]);
        match show {
            1 if masked => flow_color(motion(i, j), reach),
            1 => [0.0, 0.0, 0.0, 1.0],
            2 => {
                let v = if blurred { 1.0 } else { 0.0 };
                [v, v, v, 1.0]
            }
            _ => {
                let c = cur.at(x, y);
                if blurred {
                    c
                } else {
                    [c[0] * 0.35, c[1] * 0.35, c[2] * 0.35, c[3]]
                }
            }
        }
    })
}

// ---- Warp Flow ---------------------------------------------------------------------------------

/// Warp Flow's entry point.
pub(crate) unsafe extern "C" fn warp_main(
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
        describe(
            fx,
            c"Warp Flow",
            c"The first picture flows into the second: the points they share are found, and the frames between morph one into the other.",
            true,
        );
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        let rgba = [kOfxImageComponentRGBA];
        fx.define_clip(kOfxImageEffectTransitionSourceFromClipName, &rgba);
        fx.define_clip(kOfxImageEffectTransitionSourceToClipName, &rgba);
        fx.define_clip(kOfxImageEffectOutputClipName, &rgba);
        let p = fx.define_param(
            kOfxParamTypeDouble,
            kOfxImageEffectTransitionParamName,
            c"Progress",
        );
        p.set_dbl(kOfxParamPropDefault, 0.0);
        p.set_dbl(kOfxParamPropMin, 0.0);
        p.set_dbl(kOfxParamPropMax, 1.0);
        // VEGAS's panel: Method, then Blending.
        choice(
            fx,
            METHOD,
            c"Method",
            &METHODS,
            0,
            c"How the frames between are made: from the points both pictures share, or from the motion at every pixel.",
        );
        let p = fx.define_param(kOfxParamTypeChoice, BLENDING, c"Blending");
        p.set_strs(kOfxParamPropChoiceOption, &BLENDINGS);
        p.set_int(kOfxParamPropDefault, 0);
        p.set_str(
            kOfxParamPropHint,
            c"Blend mixes the two pictures, both moved to the same shape; Nearest Frame takes the nearer one, moved.",
        );
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        warp_render(fx, args)
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        whole_source(
            fx,
            args,
            out,
            &[
                kOfxImageEffectTransitionSourceFromClipName,
                kOfxImageEffectTransitionSourceToClipName,
            ],
        )
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

fn warp_render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let read = |c: &CStr| fx.clip(c).image(time).map(|i| Buf::read(Some(&i)));
    let (Some(a), Some(b)) = (
        read(kOfxImageEffectTransitionSourceFromClipName),
        read(kOfxImageEffectTransitionSourceToClipName),
    ) else {
        return kOfxStatFailed;
    };
    let t = fx
        .param(kOfxImageEffectTransitionParamName)
        .f64_at(time)
        .clamp(0.0, 1.0);
    let nearest = fx.param(BLENDING).int_at(time) == 1;
    let result = if b.bounds != a.bounds {
        a
    } else {
        // The pictures change as the transition goes: the flow of each
        // pair is its own, back from the second to the first.
        let q = FlowQuality::Medium;
        let f = flow_of(&b, &a, q, key(fx, time, t, q, (a.width(), a.height())));
        made_between(&a, &b, &f, t, nearest)
    };
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

#[cfg(test)]
mod tests {
    use super::*;

    fn textured(shift: f64) -> Buf {
        let (w, h) = (96, 64);
        let mut data = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let u = x as f64 - shift;
                let v = y as f64;
                let g = (0.5
                    + 0.2 * (u * 0.21).sin() * (v * 0.17).cos()
                    + 0.15 * ((u + v) * 0.13).sin()) as f32;
                data.push([g, g, g, 1.0]);
            }
        }
        Buf {
            bounds: [0, 0, w as c_int, h as c_int],
            data,
        }
    }

    #[test]
    fn the_frame_between_is_halfway_along() {
        // Two frames four pixels apart: the one between is two along.
        let (a, b) = (textured(0.0), textured(4.0));
        let f = flow_of(&a, &b, FlowQuality::High, 1);
        let mid = between(&a, &b, &f, 0.5);
        let want = textured(2.0);
        let mut worst = 0.0f32;
        for y in 12..52 {
            for x in 12..84 {
                let (m, w) = (mid.at(x, y), want.at(x, y));
                worst = worst.max((m[0] - w[0]).abs());
            }
        }
        assert!(worst < 0.04, "{worst}");
    }

    #[test]
    fn vegass_frame_between_is_halfway_along_too() {
        // VEGAS's way: the motion back from the second frame, the first
        // read along it, the second back; Nearest Frame the nearer, moved.
        let (a, b) = (textured(0.0), textured(4.0));
        let f = flow_of(&b, &a, FlowQuality::High, 2);
        let want = textured(2.0);
        for nearest in [false, true] {
            let mid = made_between(&a, &b, &f, 0.5, nearest);
            let mut worst = 0.0f32;
            for y in 12..52 {
                for x in 12..84 {
                    worst = worst.max((mid.at(x, y)[0] - want.at(x, y)[0]).abs());
                }
            }
            assert!(worst < 0.05, "{nearest}: {worst}");
        }
    }

    #[test]
    fn motion_shows_as_colors_by_direction() {
        let up = flow_color([0.0, 1.0], 1.0);
        assert!(up[0] > 0.9 && up[1] < 0.1, "{up:?}");
        let still = flow_color([0.0, 0.0], 1.0);
        assert!(still[..3].iter().all(|&c| c == 0.0));
    }
}
