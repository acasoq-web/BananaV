//! Gaussian Blur: VEGAS's. The picture blurred across and up and down, in
//! the channels picked.
//!
//! VEGAS's math: its Gaussian (`recursive_gauss`), the radii each range
//! of the frame's longer side in pixels — the vertical one times how many
//! times as wide as tall the pixels show —, the picture's edge pixels held
//! beyond it (none blurs only with both ranges 0); each channel of the
//! values the host hands over (premultiplied; in 8-bit projects the
//! encoded ones, as VEGAS's), those the Channels group turns off left as
//! they were.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::kit::{Buf, Grid};
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.GaussianBlur";

/// The parameters (BananaV's earlier Gaussian Blur's names, so its
/// projects open).
const HORIZONTAL: &CStr = c"horizontalRange";
const VERTICAL: &CStr = c"verticalRange";
const CHANNELS: &CStr = c"channels";
const RED: &CStr = c"red";
const GREEN: &CStr = c"green";
const BLUE: &CStr = c"blue";
const ALPHA: &CStr = c"alpha";

/// The ranges at first: VEGAS's.
const RANGE_AT_FIRST: f64 = 0.063;

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub horizontal: f64,
    pub vertical: f64,
    /// Red, green, blue and alpha: blurred or left.
    pub channels: [bool; 4],
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        let on = |name| fx.param(name).int_at(time) != 0;
        Settings {
            horizontal: fx.param(HORIZONTAL).f64_at(time),
            vertical: fx.param(VERTICAL).f64_at(time),
            channels: [on(RED), on(GREEN), on(BLUE), on(ALPHA)],
        }
    }

    /// The settings in `values` (the frame's, as the card draws it).
    pub(crate) fn of(values: Values<'_>) -> Settings {
        let on = |name| values.int(name, 1) != 0;
        Settings {
            horizontal: values.f64(HORIZONTAL, RANGE_AT_FIRST),
            vertical: values.f64(VERTICAL, RANGE_AT_FIRST),
            channels: [on(RED), on(GREEN), on(BLUE), on(ALPHA)],
        }
    }

    pub(crate) fn changes_nothing(&self) -> bool {
        (self.horizontal <= 0.0 && self.vertical <= 0.0) || self.channels == [false; 4]
    }

    /// The blur's radii across and down, in pixels, on a picture `size`
    /// pixels (the frame at render scale) whose pixels show `shape` times
    /// as wide as tall: VEGAS's, each range of the longer side, the
    /// vertical one times the pixels' shape.
    pub(crate) fn radii(&self, size: [f64; 2], shape: f64) -> [f64; 2] {
        let longer = size[0].max(size[1]);
        [longer * self.horizontal, longer * self.vertical * shape]
    }

    /// The picture blurred, on the frame `g` places.
    pub(crate) fn process(&self, g: &Grid, src: &Buf) -> Buf {
        let mut blurred = src.clone();
        let radii = self.radii([g.width_px(), g.height_px()], g.pixel_shape());
        crate::recursive_gauss::blur(&mut blurred, radii);
        if self.channels != [true; 4] {
            for (b, &p) in blurred.data.iter_mut().zip(&src.data) {
                *b = self.pick(p, *b);
            }
        }
        blurred
    }

    /// A pixel with the channels the Channels group turns on blurred, as
    /// VEGAS's: each the blurred picture's or the picture's, as the
    /// premultiplied values stand.
    fn pick(&self, p: [f32; 4], blurred: [f32; 4]) -> [f32; 4] {
        std::array::from_fn(|i| if self.channels[i] { blurred[i] } else { p[i] })
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
    p.set_str(kOfxPropLabel, c"Gaussian Blur");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Blur");
    p.set_str(
        kOfxPropPluginDescription,
        c"The picture blurred across and up and down, in the channels you choose.",
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
    let range = |name: &CStr, label: &CStr, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, RANGE_AT_FIRST);
        p.set_dbl(kOfxParamPropMin, 0.0);
        p.set_dbl(kOfxParamPropMax, 1.0);
        p.set_dbl(kOfxParamPropDisplayMin, 0.0);
        p.set_dbl(kOfxParamPropDisplayMax, 1.0);
        p.set_dbl(kOfxParamPropIncrement, 0.0001);
        // VEGAS's four decimals.
        p.set_int(kOfxParamPropDigits, 4);
        p.set_str(kOfxParamPropHint, hint);
    };
    range(
        HORIZONTAL,
        c"Horizontal range",
        c"How far across the pixels blend, a part of the frame's longer side.",
    );
    range(
        VERTICAL,
        c"Vertical range",
        c"How far up and down the pixels blend, a part of the frame's longer side as the horizontal range.",
    );
    // Closed at first, as VEGAS's.
    let g = fx.define_param(kOfxParamTypeGroup, CHANNELS, c"Channels");
    g.set_int(kOfxParamPropGroupOpen, 0);
    for (name, label, hint) in [
        (RED, c"Red", c"The red channel blurred."),
        (GREEN, c"Green", c"The green channel blurred."),
        (BLUE, c"Blue", c"The blue channel blurred."),
        (
            ALPHA,
            c"Alpha",
            c"The alpha channel blurred: softer transparency and masks.",
        ),
    ] {
        let p = fx.define_param(kOfxParamTypeBoolean, name, label);
        p.set_int(kOfxParamPropDefault, 1);
        // They animate, as VEGAS's do (each with its clock).
        p.set_int(kOfxParamPropAnimates, 1);
        p.set_str(kOfxParamPropParent, CHANNELS);
        p.set_str(kOfxParamPropHint, hint);
    }
}

/// The factory presets: VEGAS's, in its list's order.
pub(crate) fn presets() -> Vec<Preset> {
    let p = |name: &'static str, range: f64, blue: bool| Preset {
        name,
        values: vec![
            ("horizontalRange", PresetValue::Num(vec![range])),
            ("verticalRange", PresetValue::Num(vec![range])),
            ("red", PresetValue::Num(vec![1.0])),
            ("green", PresetValue::Num(vec![1.0])),
            ("blue", PresetValue::Num(vec![f64::from(u8::from(blue))])),
            ("alpha", PresetValue::Num(vec![1.0])),
        ],
    };
    vec![
        p("Medium Blur", 0.06, true),
        p("Light Blur", 0.01, true),
        p("Extreme Blur", 0.19, true),
        p("Soften", 0.02, true),
        p("Yellow Glow", 0.01, false),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_radii_are_vegass() {
        // 1920 × 1080: the defaults reach 120.96 pixels each way (a sigma
        // of 36.79); a portrait frame counts its height.
        let s = settings([true; 4]);
        let near =
            |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
        assert!(near(s.radii([1920.0, 1080.0], 1.0), [120.96, 120.96]));
        assert!(near(s.radii([1080.0, 1920.0], 1.0), [120.96, 120.96]));
        // Wide pixels: farther up and down, so it looks round.
        assert!(near(s.radii([1440.0, 1080.0], 2.0), [90.72, 181.44]));
        let sigma = crate::recursive_gauss::sigma(120.96);
        assert!((sigma - 36.79).abs() < 0.01, "{sigma}");
    }

    fn settings(channels: [bool; 4]) -> Settings {
        Settings {
            horizontal: 0.063,
            vertical: 0.063,
            channels,
        }
    }

    #[test]
    fn the_channels_turned_off_stay() {
        let p = [0.2, 0.4, 0.6, 1.0];
        let b = [0.5, 0.5, 0.5, 1.0];
        assert_eq!(settings([true; 4]).pick(p, b), b);
        // Yellow Glow: blue left as it was.
        let out = settings([true, true, false, true]).pick(p, b);
        assert_eq!(out, [0.5, 0.5, 0.6, 1.0]);
        // The alpha kept: the premultiplied colors blurred as they stand.
        let out =
            settings([true, true, true, false]).pick([0.1, 0.1, 0.1, 0.5], [0.3, 0.3, 0.3, 0.75]);
        assert_eq!(out, [0.3, 0.3, 0.3, 0.5]);
        assert!(settings([false; 4]).changes_nothing());
        assert!(
            Settings {
                horizontal: 0.0,
                vertical: 0.0,
                channels: [true; 4]
            }
            .changes_nothing()
        );
    }

    #[test]
    fn the_presets_are_vegass() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            [
                "Medium Blur",
                "Light Blur",
                "Extreme Blur",
                "Soften",
                "Yellow Glow"
            ]
        );
    }
}
