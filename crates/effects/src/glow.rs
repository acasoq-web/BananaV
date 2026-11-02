//! Glow: VEGAS's. Light spread around the picture's bright parts, in a
//! color.
//!
//! VEGAS's math: the brightness — the plain average of red, green and
//! blue — is blurred by VEGAS's Gaussian (`recursive_gauss`), its radius
//! half of Glow percent × the frame's longer side (measured on the
//! calibration card through each of VEGAS's presets: a sigma of `0.15 ×
//! Glow percent × the frame's width`, this radius's); Suppression is taken
//! from the blurred brightness and what is left, times Intensity (within
//! 0–1), is how far each pixel goes toward Color. So Suppression keeps the
//! glow to where the picture is bright around, Intensity makes it stronger
//! and wider, and a Glow percent of 0 colors the bright parts themselves
//! (the filter still blurs a little, as VEGAS's). The picture's edge pixels
//! are held beyond it, as Gaussian Blur's.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::kit::{Buf, Grid};
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.Glow";

/// The parameters (BananaV's earlier Glow's names, so its projects open).
const GLOW: &CStr = c"glowPercent";
const INTENSITY: &CStr = c"intensity";
const SUPPRESSION: &CStr = c"suppression";
const COLOR: &CStr = c"color";

/// The blur's radius per Glow percent, a part of the frame's longer side.
const REACH: f64 = 0.5;

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub glow: f64,
    pub intensity: f64,
    pub suppression: f64,
    pub color: [f64; 3],
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        let v = |name| fx.param(name).f64_at(time);
        Settings {
            glow: v(GLOW),
            intensity: v(INTENSITY),
            suppression: v(SUPPRESSION),
            color: fx.param(COLOR).rgb_at(time),
        }
    }

    /// The settings in `values` (the frame's, as the card draws it).
    pub(crate) fn of(values: Values<'_>) -> Settings {
        Settings {
            glow: values.f64(GLOW, 0.0),
            intensity: values.f64(INTENSITY, 0.0),
            suppression: values.f64(SUPPRESSION, 0.0),
            color: values.array(COLOR, [0.0; 3]),
        }
    }

    pub(crate) fn changes_nothing(&self) -> bool {
        self.intensity <= 0.0
    }

    /// The blur's radius, in pixels, on a frame whose longer side is
    /// `longer` pixels.
    pub(crate) fn radius(&self, longer: f64) -> f64 {
        self.reach() * longer
    }

    /// The blur's radius, a part of the frame's longer side.
    pub(crate) fn reach(&self) -> f64 {
        REACH * self.glow.clamp(0.0, 1.0)
    }

    /// How far toward Color a pixel goes where the blurred brightness is
    /// `bright`.
    pub(crate) fn weight(&self, bright: f32) -> f32 {
        (self.intensity as f32 * (bright - self.suppression as f32)).clamp(0.0, 1.0)
    }

    /// The picture glowing, on the frame `g` places.
    pub(crate) fn process(&self, g: &Grid, src: &Buf) -> Buf {
        let mut bright = src.map(|x, y| {
            let p = src.at(x, y);
            [(p[0] + p[1] + p[2]) / 3.0, 0.0, 0.0, 0.0]
        });
        let r = self.radius(g.width_px().max(g.height_px()));
        crate::recursive_gauss::blur(&mut bright, [r; 2]);
        let blurred = bright.data.iter().map(|b| b[0]);
        let to = [
            self.color[0] as f32,
            self.color[1] as f32,
            self.color[2] as f32,
            1.0,
        ];
        let data = src
            .data
            .iter()
            .zip(blurred)
            .map(|(&p, b)| {
                let k = self.weight(b);
                if k <= 0.0 {
                    return p;
                }
                // The color painted over the pixel by the weight.
                std::array::from_fn(|i| p[i] + (to[i] - p[i]) * k)
            })
            .collect();
        Buf {
            bounds: src.bounds,
            data,
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
        let s = Settings::at(fx, args.dbl(kOfxPropTime, 0));
        crate::spatial::render(fx, args, &|g, src| s.process(g, src))
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        if Settings::at(fx, args.dbl(kOfxPropTime, 0)).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        crate::spatial::whole_source(fx, args, out)
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
    p.set_str(kOfxPropLabel, c"Glow");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Creative");
    p.set_str(
        kOfxPropPluginDescription,
        c"A halo of light around the bright parts.",
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

/// A number with VEGAS's three decimals, 0 at first, over 0–`max`.
fn number(fx: Fx, name: &CStr, label: &CStr, max: f64, hint: &CStr) {
    let p = fx.define_param(kOfxParamTypeDouble, name, label);
    p.set_dbl(kOfxParamPropDefault, 0.0);
    p.set_dbl(kOfxParamPropMin, 0.0);
    p.set_dbl(kOfxParamPropMax, max);
    p.set_dbl(kOfxParamPropDisplayMin, 0.0);
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
        GLOW,
        c"Glow percent",
        1.0,
        c"How far the light spreads around the bright parts (0: only the bright parts themselves).",
    );
    number(
        fx,
        INTENSITY,
        c"Intensity",
        20.0,
        c"How strong the glow is: higher, it covers more and reaches farther.",
    );
    number(
        fx,
        SUPPRESSION,
        c"Suppression",
        1.0,
        c"How bright the surroundings must be to glow: higher, only the brightest parts glow.",
    );
    // Its picker in HSV, as VEGAS's opens.
    let p = fx.define_param(kOfxParamTypeRGB, COLOR, c"Color");
    p.set_dbls(kOfxParamPropDefault, &[0.0, 0.0, 0.0]);
    p.set_str(kOfxParamPropHint, c"The glow's color.");
}

/// The factory presets: VEGAS's, in its list's order.
pub(crate) fn presets() -> Vec<Preset> {
    let p =
        |name: &'static str, [glow, intensity, suppression]: [f64; 3], color: [f64; 3]| Preset {
            name,
            values: vec![
                ("glowPercent", PresetValue::Num(vec![glow])),
                ("intensity", PresetValue::Num(vec![intensity])),
                ("suppression", PresetValue::Num(vec![suppression])),
                ("color", PresetValue::Num(color.to_vec())),
            ],
        };
    let white = [1.0, 1.0, 1.0];
    vec![
        p("White Soft Glow", [0.22, 3.2, 0.3], white),
        p("White Intense Glow", [0.3, 5.0, 0.2], white),
        p("White Highlights", [0.05, 3.2, 0.4], white),
        p("Red Glow", [0.5, 1.0, 0.2], [1.0, 0.0, 0.0]),
        p("Blue Glow Everywhere", [1.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        p(
            "Green Highlights on Brights",
            [0.1, 3.0, 0.75],
            [0.0, 1.0, 0.0],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(glow: f64, intensity: f64, suppression: f64) -> Settings {
        Settings {
            glow,
            intensity,
            suppression,
            color: [1.0, 1.0, 1.0],
        }
    }

    #[test]
    fn the_spread_and_the_weight_are_vegass() {
        // The card, 1920 wide: Glow percent 1 reaches 960 pixels, a sigma
        // of 288.8 (288 measured), 0.05 48, a sigma of 14.9 (14.4).
        assert_eq!(s(1.0, 1.0, 0.0).radius(1920.0), 960.0);
        assert_eq!(s(0.05, 3.2, 0.4).radius(1920.0), 48.0);
        for (glow, measured) in [(1.0, 288.0), (0.05, 14.4)] {
            let sigma = crate::recursive_gauss::sigma(s(glow, 1.0, 0.0).radius(1920.0));
            assert!((sigma - measured).abs() < 0.9, "{glow}: {sigma}");
        }
        // White Highlights inside the card's middle gray: 3.2 × (128⁄255 −
        // 0.4) of the way to white, VEGAS's 169.
        let w = s(0.05, 3.2, 0.4).weight(128.0 / 255.0);
        assert_eq!((128.0 + (255.0 - 128.0) * w).round(), 169.0);
        // Below Suppression nothing; past it, all the way.
        assert_eq!(s(0.3, 5.0, 0.2).weight(0.19), 0.0);
        assert_eq!(s(0.3, 5.0, 0.2).weight(0.5), 1.0);
        assert!(s(0.0, 0.0, 0.0).changes_nothing());
    }

    #[test]
    fn the_glow_goes_toward_its_color() {
        let g = Grid {
            origin: [0.0, 0.0],
            frame: [16.0, 8.0],
            scale: [1.0, 1.0],
            par: 1.0,
        };
        // A gray picture through Blue Glow Everywhere: everything a share
        // of its brightness toward blue — red down, blue up.
        let src = Buf {
            bounds: [0, 0, 16, 8],
            data: vec![[0.5, 0.5, 0.5, 1.0]; 128],
        };
        let blue = Settings {
            color: [0.0, 0.0, 1.0],
            ..s(1.0, 1.0, 0.0)
        };
        let out = blue.process(&g, &src);
        // VEGAS's filter keeps a flat picture within a few ten-thousandths.
        let p = out.at(8, 4);
        assert!(
            (p[0] - 0.25).abs() < 5e-4 && (p[2] - 0.75).abs() < 5e-4,
            "{p:?}"
        );
        assert_eq!(p[3], 1.0);
    }

    #[test]
    fn the_presets_are_vegass() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            [
                "White Soft Glow",
                "White Intense Glow",
                "White Highlights",
                "Red Glow",
                "Blue Glow Everywhere",
                "Green Highlights on Brights"
            ]
        );
    }
}
