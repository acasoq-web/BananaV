//! Stabilize, as VEGAS's older stabilizer (its media effect): its panel —
//! Pan smoothing, Stabilization amount, Rolling shutter correction and
//! Apply —, its three presets and its rules: Apply asks the host for the
//! picture's motion (BananaV's `kBananaVParamPropAnalysis` "Motion", kept
//! in the hidden M20 data) and the values it was applied with are kept
//! apart, so changing a setting greys nothing out but turns Apply on with
//! VEGAS's message until it is applied again. The math is BananaV's own:
//! the camera's moves smoothed by Video
//! Stabilization's Blackman window over 0.2 to 5 seconds as Pan smoothing
//! goes, each frame's correction held within the zoom Stabilization
//! amount allows (a quarter of the frame at most), the view zoomed that
//! far as needed, and with Rolling shutter correction each frame's rows
//! put back where a sensor read top to bottom over most of a frame left
//! them; drawn as Video Stabilization draws (`Plan::Stabilize` on the
//! card).
#![allow(unsafe_code)]

use std::collections::hash_map::DefaultHasher;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::{Plan, Values};
use crate::kit::Buf;
use crate::ofx::{Fx, Props, is, suites};
use crate::stabilization::{
    Additional, Affine, IDENTITY, Model, Smoothing, compose, corrections, margins, matrix,
    normalized, parse, read, window_of,
};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.Stabilize";

const HAS_M10: &CStr = c"hasM10StabData";
const M10: &CStr = c"m10StabData";
const HAS_M20: &CStr = c"hasM20StabData";
const M20_HEADER: &CStr = c"m20StabDataHeader";
/// The motion the host found (hidden).
const M20: &CStr = c"m20StabData";
const PER_BLOB: &CStr = c"m20DataFramesPerBlob";
const CURRENT_PAN: &CStr = c"currentPanSmoothing";
const CURRENT_AMOUNT: &CStr = c"currentViewScale";
const CURRENT_SHUTTER: &CStr = c"currentRollingShutterCorrection";
const PAN: &CStr = c"panSmoothing";
const AMOUNT: &CStr = c"viewScale";
const SHUTTER: &CStr = c"rollingShutterCorrection";
const APPLY: &CStr = c"analyzeButton";
const CONTROLS: &CStr = c"controls";

/// The message while what is applied differs from the panel.
const MUST_APPLY: &CStr = c"You must click Apply for changes to take effect.";

/// The most a frame is zoomed, Stabilization amount at 1: a quarter of
/// the frame cut away.
const MOST_ZOOM: f64 = 0.25;
/// How much of a frame's time a sensor takes to read it, top to bottom.
const READOUT: f64 = 0.9;

/// The settings a frame is drawn with: what was applied.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    motion: String,
    has: bool,
    pan: f64,
    amount: f64,
    shutter: bool,
}

/// The frames' corrections for the applied settings, worked out once.
#[derive(Debug)]
struct Plan2 {
    /// Each frame's map (frame pixels, rows up).
    maps: Vec<Affine>,
}

type Kept = (u64, Arc<Plan2>);
static PLANS: Mutex<Vec<Kept>> = Mutex::new(Vec::new());

impl Settings {
    fn at(fx: Fx) -> Settings {
        let p = |n: &CStr| fx.param(n);
        Settings {
            motion: p(M20).text_at(0.0),
            has: p(HAS_M20).int_at(0.0) != 0,
            pan: p(CURRENT_PAN).f64_at(0.0),
            amount: p(CURRENT_AMOUNT).f64_at(0.0),
            shutter: p(CURRENT_SHUTTER).int_at(0.0) != 0,
        }
    }

    pub(crate) fn of_values(v: Values<'_>) -> Settings {
        Settings {
            motion: v.text(M20).unwrap_or("").to_string(),
            has: v.int(HAS_M20, 0) != 0,
            pan: v.f64(CURRENT_PAN, 0.0),
            amount: v.f64(CURRENT_AMOUNT, 0.0),
            shutter: v.int(CURRENT_SHUTTER, 0) != 0,
        }
    }

    /// Each frame's map on a frame `size` pixels big at `rate` frames a
    /// second.
    fn maps(&self, size: [f64; 2], rate: f64) -> Option<Arc<Plan2>> {
        if !self.has {
            return None;
        }
        let mut h = DefaultHasher::new();
        self.motion.hash(&mut h);
        [self.pan, self.amount, rate].map(f64::to_bits).hash(&mut h);
        self.shutter.hash(&mut h);
        size.map(f64::to_bits).hash(&mut h);
        let key = h.finish();
        if let Ok(kept) = PLANS.lock()
            && let Some((_, p)) = kept.iter().find(|(k, _)| *k == key)
        {
            return Some(p.clone());
        }
        let steps = parse(&self.motion)?;
        if steps.is_empty() {
            return None;
        }
        let made = Arc::new(stabilized(&steps, self, size, rate));
        if let Ok(mut kept) = PLANS.lock() {
            kept.retain(|(k, _)| *k != key);
            kept.push((key, made.clone()));
            if kept.len() > 4 {
                kept.remove(0);
            }
        }
        Some(made)
    }

    /// What the card draws for frame `frame`.
    pub(crate) fn plan(&self, frame: f64, size: [f64; 2], rate: f64) -> Plan {
        match self.drawing(frame, size, rate) {
            Some(m) if m != IDENTITY => Plan::Stabilize {
                map: normalized(&m, size),
                fill: 0,
                reach: 0.0,
            },
            _ => Plan::Same,
        }
    }

    /// Where frame `frame`'s pixels read the picture.
    fn drawing(&self, frame: f64, size: [f64; 2], rate: f64) -> Option<Affine> {
        let p = self.maps(size, rate)?;
        let k = frame.round().max(0.0) as usize;
        p.maps.get(k).copied()
    }
}

/// The maps of `steps` with settings `s`.
fn stabilized(
    steps: &[crate::stabilization::Step],
    s: &Settings,
    size: [f64; 2],
    rate: f64,
) -> Plan2 {
    let seconds = 0.2 + 4.8 * s.pan.clamp(0.0, 1.0);
    let smoothing = Smoothing {
        model: Model::Similar,
        freeze: false,
        window: window_of(seconds, rate),
        reference: 0,
        locks: [true; 6],
    };
    let c = corrections(steps, smoothing, size);
    let allowed = MOST_ZOOM * s.amount.clamp(0.0, 1.0);
    // Each correction held within what the zoom allowed hides.
    let held: Vec<[f64; 6]> = c
        .numbers
        .iter()
        .map(|n| {
            let m = matrix(n, Model::Similar, Additional::default(), size);
            let [mx, my] = margins(&m, size);
            let need = (2.0 * mx / size[0]).max(2.0 * my / size[1]);
            let t = if need > allowed && need > 0.0 {
                allowed / need
            } else {
                1.0
            };
            [n[0] * t, n[1] * t, n[2] * t, n[3] * t, n[4] * t, n[5] * t]
        })
        .collect();
    let zoom = held
        .iter()
        .map(|n| {
            let m = matrix(n, Model::Similar, Additional::default(), size);
            let [mx, my] = margins(&m, size);
            (2.0 * mx / size[0]).max(2.0 * my / size[1])
        })
        .fold(0.0f64, f64::max)
        .min(allowed);
    let add = Additional {
        zoom,
        ..Additional::default()
    };
    let n = held.len();
    let maps = (0..n)
        .map(|k| {
            let m = matrix(&held[k], Model::Similar, add, size);
            if !s.shutter {
                return m;
            }
            // The camera's speed through the frame (pixels a frame): the
            // moves into and out of it.
            let speed = |i: usize| {
                steps
                    .get(i)
                    .map_or([0.0, 0.0], |st| [st.dx * size[0], -st.dy * size[0]])
            };
            let (a, b) = (speed(k.saturating_sub(1)), speed(k.min(steps.len() - 1)));
            let v = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
            // A row read `t` of a frame after the middle row shows the
            // scene as the camera saw it `t` later: rows up, the top read
            // first.
            let mid = size[1] / 2.0;
            let per_row = READOUT / size[1];
            let rolling: Affine = [
                1.0,
                -v[0] * per_row,
                v[0] * per_row * mid,
                0.0,
                1.0 - v[1] * per_row,
                v[1] * per_row * mid,
            ];
            compose(&rolling, &m)
        })
        .collect();
    Plan2 { maps }
}

/// VEGAS's three presets.
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: f64| PresetValue::Num(vec![v]);
    let p = |name: &'static str, v: f64| Preset {
        name,
        values: vec![
            ("panSmoothing", num(v)),
            ("viewScale", num(v)),
            ("rollingShutterCorrection", num(0.0)),
        ],
    };
    vec![
        p("Light Stabilization", 0.25),
        p("Medium Stabilization", 0.5),
        p("Heavy Stabilization", 0.75),
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
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        let time = args.dbl(kOfxPropTime, 0);
        let rate = fx.props().dbl_or(kOfxImageEffectPropFrameRate, 30.0);
        let s = Settings::at(fx);
        if s.drawing(time, pixels(fx), rate)
            .is_none_or(|m| m == IDENTITY)
        {
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
    } else if is(action, kOfxActionInstanceChanged) {
        if args.is(kOfxPropType, kOfxTypeParameter) && args.is(kOfxPropName, M20) {
            applied(fx);
        }
        show(fx);
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) || is(action, kOfxActionBeginInstanceEdit) {
        show(fx);
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

/// The frame's size in pixels.
fn pixels(fx: Fx) -> [f64; 2] {
    let (_, size) = fx.frame();
    let par = fx
        .props()
        .dbl_or(kOfxImageEffectPropProjectPixelAspectRatio, 1.0);
    let par = if par > 0.0 { par } else { 1.0 };
    [size[0] / par, size[1]]
}

/// An analysis came in: the values it was applied with kept.
fn applied(fx: Fx) {
    let p = |n: &CStr| fx.param(n);
    if parse(&p(M20).text_at(0.0)).is_none_or(|m| m.is_empty()) {
        return;
    }
    p(HAS_M20).set_int(1);
    p(M20_HEADER).set_text("bvstabilize 1");
    p(CURRENT_PAN).set_f64(p(PAN).f64_at(0.0));
    p(CURRENT_AMOUNT).set_f64(p(AMOUNT).f64_at(0.0));
    p(CURRENT_SHUTTER).set_int(p(SHUTTER).int_at(0.0));
}

/// VEGAS's rule: Apply, and its message, while nothing is applied or the
/// panel differs from what was.
fn show(fx: Fx) {
    let p = |n: &CStr| fx.param(n);
    let has = p(HAS_M20).int_at(0.0) != 0;
    let same = has
        && p(PAN).f64_at(0.0) == p(CURRENT_PAN).f64_at(0.0)
        && p(AMOUNT).f64_at(0.0) == p(CURRENT_AMOUNT).f64_at(0.0)
        && p(SHUTTER).int_at(0.0) == p(CURRENT_SHUTTER).int_at(0.0);
    p(APPLY).set_enabled(!same);
    fx.persistent_message_as(kOfxMessageMessage, (!same).then_some(MUST_APPLY));
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Stabilize");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Utility");
    p.set_str(kOfxPropPluginDescription, c"Stabilizes video motion");
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextFilter],
    );
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthByte],
    );
    p.set_int(kOfxImageEffectPropSupportsTiles, 0);
    p.set_int(kOfxImageEffectPropSupportsMultiResolution, 1);
    p.set_int(kOfxImageEffectPluginPropHostFrameThreading, 0);
    p.set_str(
        kOfxImageEffectPluginRenderThreadSafety,
        kOfxImageEffectRenderFullySafe,
    );
}

fn describe_in_context(fx: Fx) {
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);
    let hidden = |ty: &CStr, name: &CStr, animates: bool| {
        let p = fx.define_param(ty, name, c"");
        p.set_int(kOfxParamPropSecret, 1);
        p.set_int(kOfxParamPropAnimates, c_int::from(animates));
        p
    };
    hidden(kOfxParamTypeBoolean, HAS_M10, false).set_int(kOfxParamPropDefault, 0);
    hidden(kOfxParamTypeString, M10, false).set_str(kOfxParamPropDefault, c"");
    hidden(kOfxParamTypeBoolean, HAS_M20, true).set_int(kOfxParamPropDefault, 0);
    hidden(kOfxParamTypeString, M20_HEADER, false).set_str(kOfxParamPropDefault, c"");
    hidden(kOfxParamTypeString, M20, true).set_str(kOfxParamPropDefault, c"");
    let p = hidden(kOfxParamTypeInteger, PER_BLOB, true);
    p.set_int(kOfxParamPropDefault, 0);
    p.set_int(kOfxParamPropMin, c_int::MIN);
    p.set_int(kOfxParamPropMax, c_int::MAX);
    for name in [CURRENT_PAN, CURRENT_AMOUNT] {
        let p = hidden(kOfxParamTypeDouble, name, false);
        p.set_dbl(kOfxParamPropDefault, 0.0);
        p.set_dbl(kOfxParamPropDisplayMin, 0.0);
        p.set_dbl(kOfxParamPropDisplayMax, 1.0);
        p.set_int(kOfxParamPropDigits, 2);
    }
    hidden(kOfxParamTypeBoolean, CURRENT_SHUTTER, false).set_int(kOfxParamPropDefault, 0);
    for (name, label) in [(PAN, c"Pan smoothing"), (AMOUNT, c"Stabilization amount")] {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, 0.5);
        p.set_dbl(kOfxParamPropMin, 0.0);
        p.set_dbl(kOfxParamPropMax, 1.0);
        p.set_dbl(kOfxParamPropDisplayMin, 0.0);
        p.set_dbl(kOfxParamPropDisplayMax, 1.0);
        p.set_dbl(kOfxParamPropIncrement, 0.01);
        p.set_int(kOfxParamPropDigits, 2);
        p.set_int(kOfxParamPropAnimates, 0);
    }
    let p = fx.define_param(kOfxParamTypeBoolean, SHUTTER, c"Rolling shutter correction");
    p.set_int(kOfxParamPropDefault, 0);
    p.set_int(kOfxParamPropAnimates, 0);
    let p = fx.define_param(kOfxParamTypePushButton, APPLY, c"Apply");
    use bv_ofx_sys::extensions as x;
    p.set_str(x::kBananaVParamPropAnalysis, c"Motion");
    p.set_str(x::kBananaVParamPropAnalysisParam, M20);
    let p = fx.define_param(kOfxParamTypePage, CONTROLS, c"");
    p.set_strs(kOfxParamPropPageChild, &[PAN, AMOUNT, SHUTTER, APPLY]);
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|v| if v > 0.0 { v } else { 1.0 });
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    if let Some(s) = &src
        && (s.depth != out.depth || s.channels != out.channels)
    {
        return kOfxStatErrImageFormat;
    }
    let (origin, _) = fx.frame();
    let par = out.par;
    let full = pixels(fx);
    let rate = fx.props().dbl_or(kOfxImageEffectPropFrameRate, 30.0);
    let source = Buf::read(src.as_ref());
    drop(src);
    let s = Settings::at(fx);
    let result = match s.drawing(time, full, rate) {
        Some(m) if m != IDENTITY && !source.data.is_empty() => {
            let render = [full[0] * scale[0], full[1] * scale[1]];
            let n = normalized(&m, full);
            let o = [
                (origin[0] * scale[0] / par).round() as c_int,
                (origin[1] * scale[1]).round() as c_int,
            ];
            let [bx, by] = [source.bounds[0] - o[0], source.bounds[1] - o[1]];
            let m = [
                n[0],
                n[1] * render[0] / render[1],
                n[2] * render[0] - f64::from(bx),
                n[3] * render[1] / render[0],
                n[4],
                n[5] * render[1] - f64::from(by),
            ];
            let size = [
                source.width().max(0) as usize,
                source.height().max(0) as usize,
            ];
            source.map(|x, y| {
                let (fx, fy) = (f64::from(x - o[0]), f64::from(y - o[1]));
                read(&source.data, size, &m, fx, fy, 0)
            })
        }
        _ => source,
    };
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stabilization::{Step, apply, invert, motion_text};

    const SIZE: [f64; 2] = [320.0, 180.0];

    fn settings(steps: &[Step], amount: f64, shutter: bool) -> Settings {
        Settings {
            motion: motion_text(steps),
            has: true,
            pan: 0.5,
            amount,
            shutter,
        }
    }

    #[test]
    fn a_shake_is_held_within_the_zoom_allowed() {
        // Shaking 6 pixels each way: a small amount holds the correction
        // back, a large one takes it out.
        let steps: Vec<Step> = (0..40)
            .map(|i| Step::similar(if i % 2 == 0 { 6.0 } else { -6.0 } / SIZE[0], 0.0, 0.0, 1.0))
            .collect();
        let little = settings(&steps, 0.05, false)
            .drawing(1.0, SIZE, 30.0)
            .unwrap();
        let much = settings(&steps, 1.0, false)
            .drawing(1.0, SIZE, 30.0)
            .unwrap();
        let mid = [160.0, 90.0];
        let (a, b) = (apply(&little, mid)[0] - 160.0, apply(&much, mid)[0] - 160.0);
        assert!(a.abs() < b.abs() && b > 4.0, "{a} {b}");
        // The view zoomed as far as needed, within what is allowed.
        let corner = apply(&much, [0.0, 0.0]);
        assert!(
            corner[0] > 0.0 && corner[0] < MOST_ZOOM * SIZE[0],
            "{corner:?}"
        );
        // Nothing applied yet: as filmed.
        let none = Settings {
            has: false,
            ..settings(&steps, 1.0, false)
        };
        assert!(none.drawing(1.0, SIZE, 30.0).is_none());
        assert!(matches!(none.plan(1.0, SIZE, 30.0), Plan::Same));
        // Applied: the card draws Video Stabilization's map.
        let card = settings(&steps, 1.0, false).plan(1.0, SIZE, 30.0);
        assert!(matches!(card, Plan::Stabilize { fill: 0, .. }), "{card:?}");
    }

    #[test]
    fn rolling_shutter_puts_rows_back() {
        // The picture moving right 10 pixels a frame: the top row, read
        // first, reads further left than the bottom one.
        let steps = vec![Step::similar(10.0 / SIZE[0], 0.0, 0.0, 1.0); 30];
        let plain = settings(&steps, 0.5, false)
            .drawing(10.0, SIZE, 30.0)
            .unwrap();
        let fixed = settings(&steps, 0.5, true)
            .drawing(10.0, SIZE, 30.0)
            .unwrap();
        let top = apply(&fixed, [160.0, 179.0])[0] - apply(&plain, [160.0, 179.0])[0];
        let bottom = apply(&fixed, [160.0, 0.0])[0] - apply(&plain, [160.0, 0.0])[0];
        assert!(top < 0.0 && bottom > 0.0, "{top} {bottom}");
        assert!((top + bottom).abs() < 0.2, "{top} {bottom}");
        assert!(invert(&fixed)[0].is_finite());
    }
}
