//! Vignette: VEGAS's. The picture's edges covered with a color around a
//! clear middle — or, inverted, the middle covered.
//!
//! VEGAS's math, measured on the calibration card through each of its
//! presets: the shape is an ellipse of the frame's pixels (a circle at
//! Aspect 1), `s = (1 + Aspect) ⁄ 2` its widening — across divided by `s`,
//! up and down times it, so its area stays — around Center (each pixel by
//! its corner), turned by Rotation; the distance counts the frame's half
//! diagonal as 1 (the wider way of non-square pixels shrunk by their
//! aspect), Outer radius times it reaching the full color and Inner radius
//! a part of Outer radius where the color starts (a pixel's span when they
//! meet); between them the color's share rises as `t^(100 ⁄ (Feather +
//! 0.001)²)` (`t` 0–1 from the inner to the outer edge), times the color's
//! alpha, and Invert takes the other share. The color goes over the
//! premultiplied colors, the alpha kept, every channel held within 0–1.
//! Each preset within two levels of VEGAS's, 89 to 99 % of the pixels
//! exact.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::kit::{Buf, Grid};
use crate::ofx::{Fx, Props, is, suites};
use crate::spatial::TIE;
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.Vignette";

/// The parameters (BananaV's earlier Vignette's names, so its projects
/// open).
const OUTER: &CStr = c"outerRadius";
const INNER: &CStr = c"innerRadius";
const ASPECT: &CStr = c"aspect";
const CENTER: &CStr = c"center";
const ROTATION: &CStr = c"rotation";
const FEATHER: &CStr = c"feather";
const COLOR: &CStr = c"color";
const INVERT: &CStr = c"invert";

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub outer: f64,
    pub inner: f64,
    pub aspect: f64,
    /// 0–1 of the frame from its lower left.
    pub center: [f64; 2],
    /// Degrees.
    pub rotation: f64,
    pub feather: f64,
    /// Straight, with its alpha.
    pub color: [f64; 4],
    pub invert: bool,
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        let v = |name| fx.param(name).f64_at(time);
        Settings {
            outer: v(OUTER),
            inner: v(INNER),
            aspect: v(ASPECT),
            center: fx.param(CENTER).xy_at(time),
            rotation: v(ROTATION),
            feather: v(FEATHER),
            color: fx.param(COLOR).rgba_at(time),
            invert: fx.param(INVERT).int_at(time) != 0,
        }
    }

    /// The settings in `values` (the frame's, as the card draws it).
    pub(crate) fn of(values: Values<'_>) -> Settings {
        Settings {
            outer: values.f64(OUTER, 1.0),
            inner: values.f64(INNER, 0.0),
            aspect: values.f64(ASPECT, 1.0),
            center: values.array(CENTER, [0.5, 0.5]),
            rotation: values.f64(ROTATION, 0.0),
            feather: values.f64(FEATHER, 5.0),
            color: values.array(COLOR, [0.0, 0.0, 0.0, 1.0]),
            invert: values.int(INVERT, 0) != 0,
        }
    }

    pub(crate) fn changes_nothing(&self) -> bool {
        self.color[3] <= 0.0
    }

    /// How much of the color covers a point `d` from Center (pixels, y up,
    /// times the pixels' scale) on a frame whose diagonal is `diagonal` in
    /// the same units: VEGAS's.
    pub(crate) fn share(&self, d: [f64; 2], diagonal: f64) -> f64 {
        let (sn, cs) = self.rotation.to_radians().sin_cos();
        let widening = (self.aspect - 1.0) * 0.5 + 1.0;
        let q = (d[0] * sn + d[1] * cs) * widening;
        let r = q.hypot((d[0] * cs - d[1] * sn) / widening);
        let outer = diagonal * self.outer * 0.5;
        let inner = outer * self.inner;
        let band = if outer - inner < 1e-6 {
            1.0
        } else {
            outer - inner
        };
        let m = if r <= inner {
            0.0
        } else {
            let t = ((r - inner) / band).clamp(0.0, 1.0);
            t.powf(self.steepness()).min(1.0)
        };
        let m = if self.invert { 1.0 - m } else { m };
        m * self.color[3]
    }

    /// The power the share rises by: VEGAS's `100 ⁄ (Feather + 0.001)²`.
    fn steepness(&self) -> f64 {
        100.0 / ((self.feather + 0.001) * (self.feather + 0.001))
    }

    /// Its numbers for the card's `kit_vignette` (the pixels' scale after
    /// them, [`pixel_scale`]): Center (0–1), the turn's sine and cosine,
    /// the widening, Outer and Inner radius, the steepness, the color.
    pub(crate) fn numbers(&self) -> Vec<f32> {
        let (sn, cs) = self.rotation.to_radians().sin_cos();
        [
            self.center[0],
            self.center[1],
            sn,
            cs,
            (self.aspect - 1.0) * 0.5 + 1.0,
            self.outer,
            self.inner,
            self.steepness(),
        ]
        .into_iter()
        .chain(self.color)
        .map(|v| v as f32)
        .collect()
    }

    /// A premultiplied pixel covered by the color at `share`: the color
    /// over its colors, the alpha kept, each channel held within 0–1.
    pub(crate) fn cover(&self, p: [f32; 4], share: f64) -> [f32; 4] {
        let k = share as f32;
        std::array::from_fn(|i| {
            let v = if i == 3 || k <= 0.0 {
                p[i]
            } else {
                p[i] * (1.0 - k) + self.color[i] as f32 * k - TIE
            };
            v.clamp(0.0, 1.0)
        })
    }

    /// The picture vignetted, on the frame `g` places.
    pub(crate) fn process(&self, g: &Grid, src: &Buf) -> Buf {
        // The frame in the picture's pixels, Center from its first pixel.
        let (w, h) = (g.width_px(), g.height_px());
        let [sx, sy] = pixel_scale(g.par);
        let diagonal = (w * sx).hypot(h * sy);
        let center = [
            g.origin[0] * g.scale[0] / g.par + w * self.center[0],
            g.origin[1] * g.scale[1] + h * self.center[1],
        ];
        src.map(|x, y| {
            let d = [
                (f64::from(x) - center[0]) * sx,
                (f64::from(y) - center[1]) * sy,
            ];
            self.cover(src.at(x, y), self.share(d, diagonal))
        })
    }
}

/// How VEGAS scales the pixels of aspect `par` across and up: the wider
/// way shrunk by the aspect.
pub(crate) fn pixel_scale(par: f64) -> [f64; 2] {
    if par >= 1.0 {
        [1.0, 1.0 / par]
    } else {
        [par, 1.0]
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
        let s = Settings::at(fx, args.dbl(kOfxPropTime, 0));
        crate::spatial::render(fx, args, &|g, src| s.process(g, src))
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        if Settings::at(fx, args.dbl(kOfxPropTime, 0)).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
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
    p.set_str(kOfxPropLabel, c"Vignette");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Creative");
    p.set_str(
        kOfxPropPluginDescription,
        c"The picture's edges softened into a color around a clear middle.",
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

/// A number with VEGAS's three decimals over `min`–`max`.
fn number(fx: Fx, name: &CStr, label: &CStr, (default, min, max): (f64, f64, f64), hint: &CStr) {
    let p = fx.define_param(kOfxParamTypeDouble, name, label);
    p.set_dbl(kOfxParamPropDefault, default);
    p.set_dbl(kOfxParamPropMin, min);
    p.set_dbl(kOfxParamPropMax, max);
    p.set_dbl(kOfxParamPropDisplayMin, min);
    p.set_dbl(kOfxParamPropDisplayMax, max);
    p.set_dbl(kOfxParamPropIncrement, 0.001);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(kOfxParamPropHint, hint);
}

fn describe_in_context(fx: Fx) {
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);
    number(
        fx,
        OUTER,
        c"Outer radius",
        (1.0, 0.0, 3.0),
        c"Where the color is full: 1 reaches the frame's corners.",
    );
    number(
        fx,
        INNER,
        c"Inner radius",
        (0.0, 0.0, 1.0),
        c"Where the color starts, a part of the outer radius: the clear middle.",
    );
    number(
        fx,
        ASPECT,
        c"Aspect",
        (1.0, 0.25, 4.0),
        c"The vignette's width over its height: 1 a circle, more a wider ellipse.",
    );
    let p = fx.define_param(kOfxParamTypeDouble2D, CENTER, c"Center");
    p.set_str(
        kOfxParamPropDoubleType,
        kOfxParamDoubleTypeNormalisedXYAbsolute,
    );
    p.set_dbls(kOfxParamPropDefault, &[0.5, 0.5]);
    // VEGAS's three decimals ("0.500, 0.500").
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"The vignette's middle: 0, 0 is the lower left corner.",
    );
    let p = fx.define_param(kOfxParamTypeDouble, ROTATION, c"Rotation");
    p.set_str(kOfxParamPropDoubleType, kOfxParamDoubleTypeAngle);
    p.set_dbl(kOfxParamPropDefault, 0.0);
    p.set_dbl(kOfxParamPropMin, 0.0);
    p.set_dbl(kOfxParamPropMax, 360.0);
    p.set_dbl(kOfxParamPropDisplayMin, 0.0);
    p.set_dbl(kOfxParamPropDisplayMax, 360.0);
    p.set_dbl(kOfxParamPropIncrement, 1.0);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"The vignette's ellipse turned, in degrees.",
    );
    number(
        fx,
        FEATHER,
        c"Feather",
        (5.0, 0.0, 10.0),
        c"How gently the color comes in between the inner and the outer radius: 0 a hard edge.",
    );
    let p = fx.define_param(kOfxParamTypeRGBA, COLOR, c"Color");
    p.set_dbls(kOfxParamPropDefault, &[0.0, 0.0, 0.0, 1.0]);
    p.set_str(
        kOfxParamPropHint,
        c"The vignette's color; its alpha says how much it covers.",
    );
    // VEGAS's picker opens in RGB here.
    p.set_str(bv_ofx_sys::extensions::kBananaVParamPropColorModel, c"RGB");
    let p = fx.define_param(kOfxParamTypeBoolean, INVERT, c"Invert");
    p.set_int(kOfxParamPropDefault, 0);
    // It animates, as VEGAS's does (with its clock).
    p.set_int(kOfxParamPropAnimates, 1);
    p.set_str(
        kOfxParamPropHint,
        c"The middle covered with the color and the outside clear.",
    );
}

/// The factory presets: VEGAS's, in its list's order.
pub(crate) fn presets() -> Vec<Preset> {
    struct V {
        outer: f64,
        inner: f64,
        aspect: f64,
        center: [f64; 2],
        feather: f64,
        color: [f64; 4],
        invert: bool,
    }
    let black = [0.0, 0.0, 0.0, 1.0];
    let p = |name: &'static str, v: V| Preset {
        name,
        values: vec![
            ("outerRadius", PresetValue::Num(vec![v.outer])),
            ("innerRadius", PresetValue::Num(vec![v.inner])),
            ("aspect", PresetValue::Num(vec![v.aspect])),
            ("center", PresetValue::Num(v.center.to_vec())),
            ("rotation", PresetValue::Num(vec![0.0])),
            ("feather", PresetValue::Num(vec![v.feather])),
            ("color", PresetValue::Num(v.color.to_vec())),
            (
                "invert",
                PresetValue::Num(vec![f64::from(u8::from(v.invert))]),
            ),
        ],
    };
    let plain = |outer, aspect, feather, color| V {
        outer,
        inner: 0.0,
        aspect,
        center: [0.5, 0.5],
        feather,
        color,
        invert: false,
    };
    vec![
        p("Edges", plain(1.5, 0.25, 3.6, black)),
        p(
            "Highlight",
            V {
                outer: 0.25,
                inner: 0.5,
                aspect: 2.0,
                center: [0.5, 0.25],
                feather: 5.0,
                color: [1.0, 1.0, 0.0, 0.4],
                invert: true,
            },
        ),
        p("Spotlight", plain(0.3, 1.0, 3.0, black)),
        p("Subtle", plain(1.2, 1.0, 5.0, black)),
        p("White", plain(1.0, 1.6, 6.0, [1.0, 1.0, 1.0, 1.0])),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Settings {
        Settings {
            outer: 1.0,
            inner: 0.0,
            aspect: 1.0,
            center: [0.5, 0.5],
            rotation: 0.0,
            feather: 5.0,
            color: [0.0, 0.0, 0.0, 1.0],
            invert: false,
        }
    }

    #[test]
    fn the_share_grows_to_the_outer_radius() {
        let s = defaults();
        let diagonal = 1920f64.hypot(1080.0);
        let half = diagonal / 2.0;
        // The middle clear, the corners full; halfway (t 0.5) to the power
        // 100 ⁄ 5.001².
        assert_eq!(s.share([0.0, 0.0], diagonal), 0.0);
        assert!((s.share([960.0, 540.0], diagonal) - 1.0).abs() < 1e-9);
        let k = 100.0 / (5.001f64 * 5.001);
        assert!((s.share([half / 2.0, 0.0], diagonal) - 0.5f64.powf(k)).abs() < 1e-9);
        // Aspect 2: wider by 1.5 across, narrower by it up and down.
        let wide = Settings { aspect: 2.0, ..s };
        let a = wide.share([300.0 * 1.5, 0.0], diagonal);
        let b = wide.share([0.0, 300.0 / 1.5], diagonal);
        assert!((a - b).abs() < 1e-9 && a > 0.0);
        // Inverted, Highlight's: the middle covered by the alpha.
        let inv = Settings {
            invert: true,
            color: [1.0, 1.0, 0.0, 0.4],
            ..s
        };
        assert!((inv.share([0.0, 0.0], diagonal) - 0.4).abs() < 1e-9);
        // Rotation turns the ellipse as VEGAS's: a quarter turn swaps its
        // axes; an eighth turns its wide way clockwise, toward the lower
        // right and the upper left.
        let turned = Settings {
            rotation: 90.0,
            ..wide
        };
        let a = turned.share([0.0, 300.0 * 1.5], diagonal);
        assert!((a - wide.share([300.0 * 1.5, 0.0], diagonal)).abs() < 1e-9);
        let eighth = Settings {
            rotation: 45.0,
            ..wide
        };
        let up_right = eighth.share([300.0, 300.0], diagonal);
        let up_left = eighth.share([-300.0, 300.0], diagonal);
        assert!(up_left < up_right, "{up_left} {up_right}");
        // Inner radius at Outer radius: a pixel's span between them.
        let edge = Settings { inner: 1.0, ..s };
        assert_eq!(edge.share([half - 0.5, 0.0], diagonal), 0.0);
        assert!(edge.share([half + 0.5, 0.0], diagonal) > 0.0);
        // Wide pixels: the frame's way up shrunk by their aspect.
        assert_eq!(pixel_scale(2.0), [1.0, 0.5]);
        assert_eq!(pixel_scale(0.9), [0.9, 1.0]);
    }

    #[test]
    fn the_color_covers_the_picture() {
        let s = defaults();
        // VEGAS's Default on the card: a gray of 128 at a share of 0.25.
        let out = s.cover([128.0 / 255.0, 0.5, 0.5, 1.0], 0.25);
        assert_eq!((out[0] * 255.0).round(), 96.0);
        assert_eq!(out[3], 1.0);
        assert_eq!(s.cover([0.3, 0.2, 0.1, 1.0], 0.0), [0.3, 0.2, 0.1, 1.0]);
        // The alpha kept, each channel held within 0–1.
        let white = Settings {
            color: [1.0, 1.0, 1.0, 1.0],
            ..s
        };
        let out = white.cover([0.1, 0.1, 0.1, 0.25], 1.0);
        assert!(
            (out[0] - (1.0 - TIE)).abs() < 1e-7 && out[3] == 0.25,
            "{out:?}"
        );
        assert_eq!(s.cover([1.5, 0.2, -0.1, 1.0], 0.0), [1.0, 0.2, 0.0, 1.0]);
        assert!(
            Settings {
                color: [0.0; 4],
                ..s
            }
            .changes_nothing()
        );
    }

    #[test]
    fn the_presets_are_vegass() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            ["Edges", "Highlight", "Spotlight", "Subtle", "White"]
        );
    }
}
