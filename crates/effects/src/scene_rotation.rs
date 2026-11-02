//! Scene Rotation, as VEGAS's: a 360° equirectangular picture's field of
//! view turned around its three axes — the angles in degrees, the one
//! about y the other way, as VEGAS reads them, the turns about x, then y,
//! then z (`(Rx · Ry) · Rz`) —, each output pixel's direction on the sphere
//! turned and read back where it lands; with Stereoscopy each eye's half
//! (top and bottom, or side by side) turned in its own picture. On the
//! card `Plan::SphereTurn`, as 360° Video Stabilization.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::{Plan, Values};
use crate::kit::{Buf, Grid};
use crate::ofx::{Fx, Props, is, suites};
use crate::stabilization_360::{Turn, turned};

pub(crate) const ID: &CStr = c"org.bananav.SceneRotation";

const ANGLES: [&CStr; 3] = [c"xAngle", c"yAngle", c"zAngle"];
const STEREO: &CStr = c"stereo";
const STEREOS: [&CStr; 3] = [c"None", c"Top-and-Bottom", c"Side-by-Side"];

/// The turn of angles `[x, y, z]` (degrees): `(Rx · Ry) · Rz`, y's angle
/// the other way, as VEGAS builds it.
pub(crate) fn turn_of([x, y, z]: [f64; 3]) -> Turn {
    let (a0, a1, a2) = (x.to_radians(), (-y).to_radians(), z.to_radians());
    let (s0, c0) = a0.sin_cos();
    let (s1, c1) = a1.sin_cos();
    let (s2, c2) = a2.sin_cos();
    let rx = [[1.0, 0.0, 0.0], [0.0, c0, -s0], [0.0, s0, c0]];
    let ry = [[c1, 0.0, s1], [0.0, 1.0, 0.0], [-s1, 0.0, c1]];
    let rz = [[c2, s2, 0.0], [-s2, c2, 0.0], [0.0, 0.0, 1.0]];
    let mul = |a: [[f64; 3]; 3], b: [[f64; 3]; 3]| {
        std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
    };
    Turn(mul(mul(rx, ry), rz))
}

/// The settings a frame is drawn with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    angles: [f64; 3],
    stereo: c_int,
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            angles: ANGLES.map(|a| fx.param(a).f64_at(time)),
            stereo: fx.param(STEREO).int_at(time),
        }
    }

    pub(crate) fn of_values(v: Values<'_>) -> Settings {
        Settings {
            angles: ANGLES.map(|a| v.f64(a, 0.0)),
            stereo: v.int(STEREO, 0),
        }
    }

    fn changes_nothing(&self) -> bool {
        self.angles.iter().all(|&a| a == 0.0)
    }

    /// What the card draws.
    pub(crate) fn plan(&self) -> Plan {
        if self.changes_nothing() {
            return Plan::Same;
        }
        Plan::SphereTurn {
            matrix: turn_of(self.angles).card(),
            stereo: self.stereo.clamp(0, 2) as u32,
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
    p.set_str(kOfxPropLabel, c"Scene Rotation");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/360");
    p.set_str(
        kOfxPropPluginDescription,
        c"Turns a 360° picture's field of view around its three axes.",
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

fn describe_in_context(fx: Fx) {
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);
    for (name, label, hint) in [
        (
            ANGLES[0],
            c"Turn around x-axis",
            c"Turn the field of view around x-axis.",
        ),
        (
            ANGLES[1],
            c"Turn around y-axis",
            c"Turn the field of view around y-axis.",
        ),
        (
            ANGLES[2],
            c"Turn around z-axis",
            c"Turn the field of view around z-axis.",
        ),
    ] {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_str(kOfxParamPropDoubleType, kOfxParamDoubleTypeAngle);
        p.set_dbl(kOfxParamPropDefault, 0.0);
        p.set_dbl(kOfxParamPropMin, -180.0);
        p.set_dbl(kOfxParamPropMax, 180.0);
        p.set_dbl(kOfxParamPropDisplayMin, -180.0);
        p.set_dbl(kOfxParamPropDisplayMax, 180.0);
        p.set_str(kOfxParamPropHint, hint);
    }
    let p = fx.define_param(kOfxParamTypeChoice, STEREO, c"Stereoscopy");
    p.set_strs(kOfxParamPropChoiceOption, &STEREOS);
    p.set_int(kOfxParamPropDefault, 0);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(kOfxParamPropHint, c"Select the target 3D frame format.");
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
    let s = Settings::at(fx, time);
    let result = if s.changes_nothing() {
        source
    } else {
        turned(&source, &grid, &turn_of(s.angles), s.stereo)
    };
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_turns_are_built_as_vegas_builds_them() {
        // About y alone: VEGAS's angle the other way.
        let t = turn_of([0.0, 90.0, 0.0]);
        let ahead = t.apply([0.0, 0.0, 1.0]);
        assert!((ahead[0] + 1.0).abs() < 1e-12, "{ahead:?}");
        // The order: x after y after z.
        let (a, b, c) = (
            turn_of([30.0, 0.0, 0.0]),
            turn_of([0.0, 40.0, 0.0]),
            turn_of([0.0, 0.0, 50.0]),
        );
        let all = turn_of([30.0, 40.0, 50.0]);
        let v = [0.3, -0.5, 0.81];
        let step = a.apply(b.apply(c.apply(v)));
        let once = all.apply(v);
        for k in 0..3 {
            assert!((step[k] - once[k]).abs() < 1e-12);
        }
        assert!(
            Settings {
                angles: [0.0; 3],
                stereo: 0
            }
            .changes_nothing()
        );
    }
}
