//! 360° Video Stabilization, as VEGAS's: its panel — Stereoscopy, Analyze
//! Motion Data, Clear Motion Data, Exclude Views and Smoothing (a window
//! in frames) — and its work: Analyze Motion Data has the host find each
//! frame's turn to the next (BananaV's `kBananaVParamPropAnalysis`
//! "Rotation": a rotation vector a line, radians), over the picture of one
//! eye when Stereoscopy says the frame holds two, leaving out the poles
//! Exclude Views names (`kBananaVParamPropAnalysisExclude` reading the
//! hidden Excluded the plug-in keeps in the host's order); the turns are
//! smoothed by Video Stabilization's Blackman window Smoothing frames long
//! (its ends mirrored), and each
//! frame's sphere turned back by its turns less the smoothed ones. The
//! sphere's turn (`turned`, Scene Rotation's too) reads each output
//! pixel's direction where the turn sends it, bilinearly, the edges held;
//! on the card `Plan::SphereTurn`, `kit_sphere_turn`.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::{Plan, Values};
use crate::kit::{Buf, Grid};
use crate::ofx::{Fx, Props, is, suites};

pub(crate) const ID: &CStr = c"org.bananav.VideoStabilization360";

const STEREO: &CStr = c"stereo";
const ANALYZE: &CStr = c"analyze";
const CLEAR: &CStr = c"clear";
const EXCLUDE: &CStr = c"excludeBottom";
/// Exclude Views in the host's order: none, the north, the south, both.
const EXCLUDED: &CStr = c"excluded";
const SMOOTHNESS: &CStr = c"smoothness";
/// The turns the host found (hidden).
const MOTION: &CStr = c"analyzedData";

const STEREOS: [&CStr; 3] = [c"None", c"Top-Bottom", c"Side-by-Side"];
const EXCLUDES: [&CStr; 3] = [c"North and South Pole", c"South Pole", c"None"];

/// The rotations' text's first word.
const MAGIC: &str = "bvrotation";

/// The turns as the hidden parameter keeps them: a rotation vector a
/// frame (x across, y up, z ahead; radians).
pub fn rotation_text(turns: &[[f64; 3]]) -> String {
    let mut s = format!("{MAGIC} 1 {}\n", turns.len());
    for t in turns {
        s.push_str(&format!("{:.8} {:.8} {:.8}\n", t[0], t[1], t[2]));
    }
    s
}

fn parse(text: &str) -> Option<Vec<[f64; 3]>> {
    let mut lines = text.lines();
    if lines.next()?.split_whitespace().next() != Some(MAGIC) {
        return None;
    }
    Some(
        lines
            .filter_map(|l| {
                let v: Vec<f64> = l
                    .split_whitespace()
                    .filter_map(|w| w.parse().ok())
                    .collect();
                (v.len() == 3).then(|| [v[0], v[1], v[2]])
            })
            .collect(),
    )
}

/// The rotation by vector `w` (its length the angle) of `v` (Rodrigues).
pub fn rotate(w: [f64; 3], v: [f64; 3]) -> [f64; 3] {
    Turn::of_vector(w).apply(v)
}

/// A turn of the sphere: a 3 × 3 matrix by rows (x across, y up, z
/// ahead).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Turn(pub [[f64; 3]; 3]);

impl Turn {
    /// The turn by rotation vector `w` (its length the angle; Rodrigues).
    pub(crate) fn of_vector(w: [f64; 3]) -> Turn {
        let angle = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
        if angle < 1e-12 {
            return Turn([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        }
        let k = w.map(|c| c / angle);
        let (s, c) = angle.sin_cos();
        let t = 1.0 - c;
        Turn([
            [
                c + k[0] * k[0] * t,
                k[0] * k[1] * t - k[2] * s,
                k[0] * k[2] * t + k[1] * s,
            ],
            [
                k[1] * k[0] * t + k[2] * s,
                c + k[1] * k[1] * t,
                k[1] * k[2] * t - k[0] * s,
            ],
            [
                k[2] * k[0] * t - k[1] * s,
                k[2] * k[1] * t + k[0] * s,
                c + k[2] * k[2] * t,
            ],
        ])
    }

    pub(crate) fn apply(&self, v: [f64; 3]) -> [f64; 3] {
        let m = &self.0;
        std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
    }

    /// The card's numbers: the matrix by rows.
    pub(crate) fn card(&self) -> [f32; 9] {
        let m = &self.0;
        std::array::from_fn(|i| m[i / 3][i % 3] as f32)
    }
}

/// A direction of the sphere as the equirectangular picture's u (0 left)
/// and v (0 top), and back.
fn direction(u: f64, v: f64) -> [f64; 3] {
    let lon = (u - 0.5) * std::f64::consts::TAU;
    let lat = (0.5 - v) * std::f64::consts::PI;
    [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()]
}

fn place(d: [f64; 3]) -> (f64, f64) {
    let lon = d[0].atan2(d[2]);
    let lat = d[1].atan2(d[0].hypot(d[2]));
    (
        (lon / std::f64::consts::TAU + 0.5).rem_euclid(1.0),
        0.5 - lat / std::f64::consts::PI,
    )
}

/// Where an eye's picture starts and how big it is along u (`across`) or
/// v, for a pixel at `t` (0–1) of the frame.
fn eye(stereo: c_int, across: bool, t: f64) -> (f64, f64) {
    match (stereo, across) {
        (1, false) | (2, true) if t < 0.5 => (0.0, 0.5),
        (1, false) | (2, true) => (0.5, 0.5),
        _ => (0.0, 1.0),
    }
}

/// The card's table for a picture `w` × `h` (`stereo` as in [`turned`]):
/// each column's longitude's sine and cosine with where its eye's picture
/// starts and how wide it is, then each row's (from the top) latitude's
/// with its eye's start and height.
pub(crate) fn card_table(stereo: c_int, w: u32, h: u32) -> Vec<[f32; 4]> {
    let column = |k: u32| {
        let u = (f64::from(k) + 0.5) / f64::from(w);
        let (o, s) = eye(stereo, true, u);
        let (sin, cos) = (((u - o) / s - 0.5) * std::f64::consts::TAU).sin_cos();
        [sin as f32, cos as f32, o as f32, s as f32]
    };
    let row = |j: u32| {
        let v = (f64::from(j) + 0.5) / f64::from(h);
        let (o, s) = eye(stereo, false, v);
        let (sin, cos) = ((0.5 - (v - o) / s) * std::f64::consts::PI).sin_cos();
        [sin as f32, cos as f32, o as f32, s as f32]
    };
    (0..w).map(column).chain((0..h).map(row)).collect()
}

/// The picture with each output pixel's direction read where `turn` sends
/// it, each eye's half (`stereo` 1 top and bottom, 2 side by side) in its
/// own picture.
pub(crate) fn turned(source: &Buf, grid: &Grid, turn: &Turn, stereo: c_int) -> Buf {
    source.map(|x, y| {
        let (u, v) = grid.uv(x, y);
        let ((ou, su), (ov, sv)) = (eye(stereo, true, u), eye(stereo, false, v));
        let d = direction((u - ou) / su, (v - ov) / sv);
        let (pu, pv) = place(turn.apply(d));
        source.sample_held(grid, ou + pu * su, ov + pv * sv)
    })
}

/// Each frame's correction (a rotation vector): its turns less the
/// smoothed ones, summed from the first frame — the turns' components
/// smoothed by Video Stabilization's Blackman window `window` frames long.
fn corrections(turns: &[[f64; 3]], window: c_int) -> Vec<[f64; 3]> {
    let smooth: Vec<[f64; 3]> = if window < 3 {
        turns.to_vec()
    } else {
        let len = if window % 2 == 0 { window + 1 } else { window } as usize;
        let columns: Vec<Vec<f32>> = (0..3)
            .map(|k| {
                let c: Vec<f32> = turns.iter().map(|t| t[k] as f32).collect();
                crate::stabilization::smoothed(&c, len)
            })
            .collect();
        (0..turns.len())
            .map(|i| std::array::from_fn(|k| f64::from(columns[k][i])))
            .collect()
    };
    let mut c = vec![[0.0; 3]];
    for (t, s) in turns.iter().zip(&smooth) {
        let p = *c.last().unwrap_or(&[0.0; 3]);
        c.push([p[0] + s[0] - t[0], p[1] + s[1] - t[1], p[2] + s[2] - t[2]]);
    }
    c
}

/// The settings a frame is drawn with.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    motion: String,
    smoothness: c_int,
    stereo: c_int,
}

impl Settings {
    fn at(fx: Fx) -> Settings {
        Settings {
            motion: fx.param(MOTION).text_at(0.0),
            smoothness: fx.param(SMOOTHNESS).int_at(0.0),
            stereo: fx.param(STEREO).int_at(0.0),
        }
    }

    pub(crate) fn of_values(v: Values<'_>) -> Settings {
        Settings {
            motion: v.text(MOTION).unwrap_or("").to_string(),
            smoothness: v.int(SMOOTHNESS, 30),
            stereo: v.int(STEREO, 0),
        }
    }

    /// Frame `frame`'s turn back (none: nothing analyzed).
    fn turn(&self, frame: f64) -> Option<Turn> {
        let turns = parse(&self.motion)?;
        if turns.is_empty() {
            return None;
        }
        let all = corrections(&turns, self.smoothness);
        let i = (frame.round().max(0.0) as usize).min(all.len() - 1);
        Some(Turn::of_vector(all[i].map(|c| -c)))
    }

    /// What the card draws.
    pub(crate) fn plan(&self, frame: f64) -> Plan {
        match self.turn(frame) {
            Some(t) => Plan::SphereTurn {
                matrix: t.card(),
                stereo: self.stereo.clamp(0, 2) as u32,
            },
            None => Plan::Same,
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
        let time = args.dbl(kOfxPropTime, 0);
        if Settings::at(fx).turn(time).is_none() {
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
        if args.is(kOfxPropType, kOfxTypeParameter) && args.is(kOfxPropName, CLEAR) {
            fx.param(MOTION).set_text("");
        }
        excluded(fx);
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) || is(action, kOfxActionBeginInstanceEdit) {
        excluded(fx);
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

/// Exclude Views in the host's order (none, the north, the south, both).
fn excluded(fx: Fx) {
    let host = match fx.param(EXCLUDE).int_at(0.0) {
        0 => 3,
        1 => 2,
        _ => 0,
    };
    if fx.param(EXCLUDED).int_at(0.0) != host {
        fx.param(EXCLUDED).set_int(host);
    }
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"360° Video Stabilization");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/360");
    p.set_str(
        kOfxPropPluginDescription,
        c"Smooths out a shaky 360° camera: its turns are analyzed, and the sphere turned back against what was not intended.",
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

fn choice(fx: Fx, name: &CStr, label: &CStr, options: &[&CStr], default: c_int, hint: &CStr) {
    let p = fx.define_param(kOfxParamTypeChoice, name, label);
    p.set_strs(kOfxParamPropChoiceOption, options);
    p.set_int(kOfxParamPropDefault, default);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(kOfxParamPropHint, hint);
}

fn describe_in_context(fx: Fx) {
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);
    choice(
        fx,
        STEREO,
        c"Stereoscopy",
        &STEREOS,
        0,
        c"You can select a preset for a stereoscopic model.",
    );
    use bv_ofx_sys::extensions as x;
    let p = fx.define_param(kOfxParamTypePushButton, ANALYZE, c"Analyze Motion Data");
    p.set_str(
        kOfxParamPropHint,
        c"The analysis is performed to determine the motion data.",
    );
    p.set_str(x::kBananaVParamPropAnalysis, c"Rotation");
    p.set_str(x::kBananaVParamPropAnalysisParam, MOTION);
    p.set_str(x::kBananaVParamPropAnalysisExclude, EXCLUDED);
    p.set_str(x::kBananaVParamPropAnalysisLayout, STEREO);
    let p = fx.define_param(kOfxParamTypePushButton, CLEAR, c"Clear Motion Data");
    p.set_str(
        kOfxParamPropHint,
        c"All results of the analysis are discarded.",
    );
    choice(
        fx,
        EXCLUDE,
        c"Exclude Views",
        &EXCLUDES,
        0,
        c"Views that are not included in the analysis.",
    );
    let p = fx.define_param(kOfxParamTypeInteger, SMOOTHNESS, c"Smoothing");
    p.set_int(kOfxParamPropDefault, 30);
    p.set_int(kOfxParamPropMin, 0);
    p.set_int(kOfxParamPropMax, 120);
    p.set_int(kOfxParamPropDisplayMin, 0);
    p.set_int(kOfxParamPropDisplayMax, 120);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(
        kOfxParamPropHint,
        c"The Smoothing parameter is the parameter that helps to define how much \"unsteadiness\" should be kept in a scene, the higher that factor is, the more the movements are smoothed.",
    );
    let p = fx.define_param(kOfxParamTypeString, MOTION, c"");
    p.set_str(kOfxParamPropDefault, c"");
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_int(kOfxParamPropSecret, 1);
    let p = fx.define_param(kOfxParamTypeInteger, EXCLUDED, c"");
    p.set_int(kOfxParamPropDefault, 3);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_int(kOfxParamPropSecret, 1);
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
    let (origin, frame) = fx.frame();
    let grid = Grid {
        origin,
        frame,
        scale,
        par: out.par,
    };
    let source = Buf::read(src.as_ref());
    drop(src);
    let s = Settings::at(fx);
    let result = match s.turn(time) {
        Some(t) => turned(&source, &grid, &t, s.stereo),
        None => source,
    };
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_keep_as_text_and_rotate_back() {
        let turns = vec![[0.0, 0.01, 0.0], [0.0, -0.01, 0.0]];
        assert_eq!(parse(&rotation_text(&turns)).unwrap().len(), 2);
        // A quarter turn about up: ahead goes to the right... and back.
        let w = [0.0, std::f64::consts::FRAC_PI_2, 0.0];
        let r = Turn::of_vector(w).apply([0.0, 0.0, 1.0]);
        assert!((r[0] - 1.0).abs() < 1e-9, "{r:?}");
        let b = Turn::of_vector(w.map(|c| -c)).apply(r);
        assert!((b[2] - 1.0).abs() < 1e-9);
        // Directions and places go round.
        let (u, v) = place(direction(0.3, 0.6));
        assert!((u - 0.3).abs() < 1e-9 && (v - 0.6).abs() < 1e-9);
    }

    #[test]
    fn a_yaw_wobble_is_smoothed_away() {
        let turns: Vec<[f64; 3]> = (0..40)
            .map(|i| [0.0, if i % 2 == 0 { 0.02 } else { -0.02 }, 0.0])
            .collect();
        // Smoothed over 30 frames the wobble's mean is near nothing: frame
        // 1, turned 0.02 about up, comes back about as far.
        let c = corrections(&turns, 30);
        assert!((c[1][1] + 0.02).abs() < 0.003, "{:?}", c[1]);
        assert!(c[2][1].abs() < 0.003, "{:?}", c[2]);
        // No smoothing: as filmed.
        assert!(corrections(&turns, 0).iter().all(|c| c[1].abs() < 1e-12));
    }
}
