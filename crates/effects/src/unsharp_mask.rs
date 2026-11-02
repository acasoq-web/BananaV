//! Unsharp Mask: VEGAS's. The picture sharpened: each pixel moved away
//! from the picture blurred around it, so edges and detail stand out.
//!
//! VEGAS's math: the picture blurred by VEGAS's Gaussian
//! (`recursive_gauss`) — its radius Radius × the frame's longer side, up
//! and down that over the pixel aspect —, the picture's edge pixels held
//! beyond it; each channel, alpha too, of the values the host hands over
//! (premultiplied; in BananaV's 8-bit projects, as VEGAS's, the encoded
//! ones) moves away from the blurred one by Amount × their difference,
//! unless the result is within Threshold of the blurred one, which keeps
//! the pixel's; nothing is clipped (in 32-bit projects what passes white
//! or black stays, as VEGAS's float effects keep it; 8-bit projects hold
//! each effect's result within 0–1, as VEGAS's 8-bit pictures). Amount
//! goes past its slider's 0–1 as VEGAS's does: its presets go up to 4
//! (Extreme) and down to −3 (Outline Edges); far below 0 the picture turns
//! to its negative around the blurred one (Negative).
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::Mutex;

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::kit::Buf;
use crate::ofx::{Fx, Image, Props, for_rows, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.UnsharpMask";

const AMOUNT: &CStr = c"amount";
const RADIUS: &CStr = c"radius";
const THRESHOLD: &CStr = c"threshold";

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub amount: f64,
    pub radius: f64,
    pub threshold: f64,
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        Settings {
            amount: v.f64(AMOUNT, 0.0),
            radius: v.f64(RADIUS, 0.0),
            threshold: v.f64(THRESHOLD, 0.0),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        let v = |name| fx.param(name).f64_at(time);
        Settings {
            amount: v(AMOUNT),
            radius: v(RADIUS),
            threshold: v(THRESHOLD),
        }
    }

    /// Nothing changes: no Amount, no Radius, or a Threshold of a whole
    /// 8-bit range (VEGAS's: Threshold × 255 rounding to 255).
    pub(crate) fn changes_nothing(&self) -> bool {
        self.amount == 0.0
            || self.radius.abs() <= 1e-6
            || (self.threshold * 255.0 + 0.5) as i64 == 255
    }

    /// The blur's radii across and down, in pixels, on a picture `size`
    /// pixels (the frame at render scale) of pixel aspect `par`: VEGAS's,
    /// Radius (0–1) × the longer side, over the pixel aspect up and down.
    pub(crate) fn radii(&self, size: [f64; 2], par: f64) -> [f64; 2] {
        let across = self.radius.clamp(0.0, 1.0) * size[0].max(size[1]);
        [across, across / par]
    }

    /// A pixel sharpened against the blurred picture's there, as VEGAS's:
    /// each channel, alpha too, as the premultiplied values stand, moved
    /// away from the blurred one by Amount × their difference — unless the
    /// result is within Threshold of the blurred one, which keeps the
    /// pixel's; unclipped.
    pub(crate) fn sharpen(&self, px: [f32; 4], blurred: [f32; 4]) -> [f32; 4] {
        std::array::from_fn(|i| {
            let (o, d) = (px[i], blurred[i]);
            let s = (f64::from(o - d) * self.amount + f64::from(o)) as f32;
            if f64::from((s - d).abs()) <= self.threshold {
                o
            } else {
                s
            }
        })
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
        // The blur reaches across the picture, its edges held.
        let time = args.dbl(kOfxPropTime, 0);
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        let roi = fx
            .clip(kOfxImageEffectSimpleSourceClipName)
            .rod(time)
            .unwrap_or(roi);
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
    p.set_str(kOfxPropLabel, c"Unsharp Mask");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Blur");
    p.set_str(
        kOfxPropPluginDescription,
        c"Sharpens the picture: each pixel moves away from the picture blurred around it, so edges and detail stand out.",
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
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);

    let number = |name: &CStr, label: &CStr, [min, max]: [f64; 2], hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, 0.0);
        p.set_dbl(kOfxParamPropMin, min);
        p.set_dbl(kOfxParamPropMax, max);
        // The sliders run over 0–1, as VEGAS's.
        p.set_dbl(kOfxParamPropDisplayMin, 0.0);
        p.set_dbl(kOfxParamPropDisplayMax, 1.0);
        p.set_dbl(kOfxParamPropIncrement, 0.01);
        p.set_int(kOfxParamPropDigits, 3);
        p.set_str(kOfxParamPropHint, hint);
    };
    number(
        AMOUNT,
        c"Amount",
        [-100_000.0, 100_000.0],
        c"How strongly edges and detail are sharpened (0: not at all). Below 0 the picture softens, and far below it turns to its negative.",
    );
    number(
        RADIUS,
        c"Radius",
        [0.0, 1.0],
        c"How far around each pixel the sharpening looks, a part of the frame's longer side: small for fine detail, large for broad contrast.",
    );
    number(
        THRESHOLD,
        c"Threshold",
        [0.0, 1.0],
        c"Pixels whose sharpened value stays this close to the blurred picture are left as they are, so flat areas and grain stay (0 sharpens every pixel).",
    );
}

/// The source's pixel at `x`, `y`, premultiplied (nothing where it has
/// none).
fn premultiplied_at(src: Option<&Image>, premultiplied: bool, x: c_int, y: c_int) -> [f32; 4] {
    src.and_then(|i| crate::colors::pixel(i, x, y))
        .map_or([0.0; 4], |c| {
            if premultiplied {
                c
            } else {
                [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
            }
        })
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|s| if s > 0.0 { s } else { 1.0 });
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    if let Some(s) = &src
        && (s.depth != out.depth || s.channels != out.channels)
    {
        return kOfxStatErrImageFormat;
    }
    let s = Settings::at(fx, time);
    let (origin, frame) = fx.frame();
    let par = out.par;
    // The frame's pixels at render scale: its first and how many.
    let first = [
        (origin[0] * scale[0] / par).round() as c_int,
        (origin[1] * scale[1]).round() as c_int,
    ];
    let w = ((frame[0] * scale[0] / par).round() as usize).max(1);
    let h = ((frame[1] * scale[1]).round() as usize).max(1);
    let premultiplied = src
        .as_ref()
        .is_some_and(|i| i.channels < 4 || !i.unpremultiplied);
    // The picture over the frame, row by row on the host's threads, and
    // the picture blurred.
    let rows: Vec<Mutex<Vec<[f32; 4]>>> = (0..h).map(|_| Mutex::new(Vec::new())).collect();
    for_rows(0..h as c_int, &|y| {
        let row: Vec<[f32; 4]> = (0..w)
            .map(|x| {
                premultiplied_at(
                    src.as_ref(),
                    premultiplied,
                    first[0] + x as c_int,
                    first[1] + y,
                )
            })
            .collect();
        if let Ok(mut r) = rows[y as usize].lock() {
            *r = row;
        }
    });
    let picture: Vec<[f32; 4]> = rows
        .into_iter()
        .flat_map(|r| r.into_inner().unwrap_or_default())
        .collect();
    let mut blurred = Buf {
        bounds: [0, 0, w as c_int, h as c_int],
        data: picture.clone(),
    };
    crate::recursive_gauss::blur(&mut blurred, s.radii([w as f64, h as f64], par));
    let blurred = blurred.data;
    let out_straight = out.channels == 4 && out.unpremultiplied;
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(row) = out.row(y) else { return };
        for x in columns.clone() {
            let (gx, gy) = (x - first[0], y - first[1]);
            let inside = gx >= 0 && gy >= 0 && (gx as usize) < w && (gy as usize) < h;
            let mut v = if inside {
                let i = gy as usize * w + gx as usize;
                s.sharpen(picture[i], blurred[i])
            } else {
                // Past the frame: the source's own pixel.
                premultiplied_at(src.as_ref(), premultiplied, x, y)
            };
            if out_straight && v[3] > 0.0 {
                v = [v[0] / v[3], v[1] / v[3], v[2] / v[3], v[3]];
            }
            // SAFETY: a row the output has, a column inside its bounds.
            unsafe { crate::colors::put_pixel(&out, row, x, v) };
        }
    };
    for_rows(y1.max(out.bounds[1])..y2.min(out.bounds[3]), &fill);
    drop(src);
    drop(out);
    kOfxStatOK
}

// Presets.

/// The factory presets: VEGAS's.
pub(crate) fn presets() -> Vec<Preset> {
    let preset = |name: &'static str, [amount, radius, threshold]: [f64; 3]| Preset {
        name,
        values: vec![
            ("amount", PresetValue::Num(vec![amount])),
            ("radius", PresetValue::Num(vec![radius])),
            ("threshold", PresetValue::Num(vec![threshold])),
        ],
    };
    vec![
        preset("Light", [0.2, 0.02, 0.0]),
        preset("Medium", [0.5, 0.02, 0.0]),
        preset("Strong", [0.8, 0.025, 0.0]),
        preset("Extreme", [4.0, 0.025, 0.1]),
        preset("Negative", [-2.0, 1.0, 0.0]),
        preset("Outline Edges", [-3.0, 0.01, 0.0]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(amount: f64, radius: f64, threshold: f64) -> Settings {
        Settings {
            amount,
            radius,
            threshold,
        }
    }

    #[test]
    fn the_blur_reaches_radius_times_the_longer_side() {
        // 1920 × 1080: Medium's Radius 0.02 reaches 38.4 pixels (a sigma
        // of 11.99); a portrait frame counts its height; up and down over
        // the pixel aspect; Radius held within 0–1.
        let medium = s(0.5, 0.02, 0.0);
        let near =
            |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
        assert!(near(medium.radii([1920.0, 1080.0], 1.0), [38.4, 38.4]));
        assert!(near(medium.radii([1080.0, 1920.0], 1.0), [38.4, 38.4]));
        assert!(near(medium.radii([1440.0, 1080.0], 2.0), [28.8, 14.4]));
        assert!(near(
            s(1.0, 3.0, 0.0).radii([800.0, 600.0], 1.0),
            [800.0, 800.0]
        ));
        let sigma = crate::recursive_gauss::sigma(38.4);
        assert!((sigma - 11.99).abs() < 0.01, "{sigma}");
    }

    #[test]
    fn each_channel_moves_away_from_the_blurred_one() {
        let medium = s(0.5, 0.02, 0.0);
        let out = medium.sharpen([0.6, 0.4, 0.5, 1.0], [0.5, 0.5, 0.5, 1.0]);
        for (o, want) in out.iter().zip([0.65, 0.35, 0.5, 1.0]) {
            assert!((o - want).abs() < 1e-6, "{out:?}");
        }
        // Not clipped (8-bit projects clip what each effect leaves).
        let extreme = s(4.0, 0.025, 0.0);
        let out = extreme.sharpen([0.9, 0.1, 0.5, 1.0], [0.6, 0.4, 0.5, 1.0]);
        for (o, want) in out.iter().zip([2.1, -1.1, 0.5, 1.0]) {
            assert!((o - want).abs() < 1e-5, "{out:?}");
        }
        // Negative: twice the blurred one less the picture.
        let negative = s(-2.0, 1.0, 0.0);
        let out = negative.sharpen([0.1, 0.3, 0.5, 1.0], [0.3, 0.3, 0.3, 1.0]);
        for (o, want) in out.iter().zip([0.5, 0.3, 0.1, 1.0]) {
            assert!((o - want).abs() < 1e-6, "{out:?}");
        }
        // The alpha too, as the premultiplied values stand.
        let out = medium.sharpen([0.3, 0.2, 0.25, 0.5], [0.25, 0.25, 0.25, 0.25]);
        for (o, want) in out.iter().zip([0.325, 0.175, 0.25, 0.625]) {
            assert!((o - want).abs() < 1e-6, "{out:?}");
        }
    }

    #[test]
    fn results_near_the_blurred_one_keep_the_pixel() {
        // VEGAS's Extreme (Amount 4, Threshold 0.1): the result, five
        // times the difference from the blurred picture, within 0.1 of it
        // keeps the pixel.
        let extreme = s(4.0, 0.025, 0.1);
        let out = extreme.sharpen([0.52, 0.53, 0.5, 1.0], [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(out[0], 0.52);
        assert!((out[1] - 0.65).abs() < 1e-6, "{out:?}");
        assert_eq!(out[2], 0.5);
    }

    #[test]
    fn the_default_changes_nothing() {
        assert!(s(0.0, 0.0, 0.0).changes_nothing());
        assert!(s(0.0, 0.5, 0.0).changes_nothing());
        assert!(s(1.0, 0.0, 0.0).changes_nothing());
        assert!(!s(0.2, 0.02, 0.0).changes_nothing());
        // A Threshold of the whole range keeps every pixel.
        assert!(s(1.0, 0.02, 1.0).changes_nothing());
        assert!(!s(1.0, 0.02, 0.99).changes_nothing());
    }

    #[test]
    fn the_presets_are_vegass() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            [
                "Light",
                "Medium",
                "Strong",
                "Extreme",
                "Negative",
                "Outline Edges"
            ]
        );
    }
}
