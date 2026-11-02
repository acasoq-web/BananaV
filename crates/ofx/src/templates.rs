//! The properties each object starts with, and their defaults, as the
//! specification (and its reference host) gives them.

use std::ffi::CStr;

use bv_ofx_sys::extensions::{
    kBananaVParamPropColorModel, kBananaVParamPropInteract, kBananaVParamPropInteractOpen,
    kBananaVParamPropLogarithmic,
};
use bv_ofx_sys::names::*;

use crate::props::{PropSet, SetKind};
use crate::value::ParamType;

/// The contexts BananaV runs plug-ins in.
pub const CONTEXTS: [&CStr; 4] = [
    kOfxImageEffectContextFilter,
    kOfxImageEffectContextGeneral,
    kOfxImageEffectContextTransition,
    kOfxImageEffectContextGenerator,
];

/// The host's own properties: what BananaV offers plug-ins.
pub fn host() -> PropSet {
    let s = PropSet::new(SetKind::Host);
    s.define_int(kOfxPropAPIVersion, None, &[1, 4]);
    s.define_str(kOfxPropType, Some(1), &[kOfxTypeImageEffectHost]);
    s.define_str(kOfxPropName, Some(1), &[c"org.bananav.BananaV"]);
    s.define_str(kOfxPropLabel, Some(1), &[c"BananaV"]);
    s.define_int(kOfxPropVersion, None, &[0, 1, 0]);
    s.define_str(kOfxPropVersionLabel, Some(1), &[c"0.1.0"]);
    s.define_int(kOfxImageEffectHostPropIsBackground, Some(1), &[0]);
    s.define_int(kOfxImageEffectPropSupportsOverlays, Some(1), &[1]);
    s.define_int(kOfxImageEffectPropSupportsMultiResolution, Some(1), &[1]);
    // Whole images, always.
    s.define_int(kOfxImageEffectPropSupportsTiles, Some(1), &[0]);
    s.define_int(kOfxImageEffectPropTemporalClipAccess, Some(1), &[1]);
    s.define_str(
        kOfxImageEffectPropSupportedComponents,
        None,
        &[
            kOfxImageComponentRGBA,
            kOfxImageComponentRGB,
            kOfxImageComponentAlpha,
        ],
    );
    s.define_str(
        kOfxImageEffectPropSupportedPixelDepths,
        None,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    s.define_str(kOfxImageEffectPropSupportedContexts, None, &CONTEXTS);
    s.define_int(kOfxImageEffectPropSupportsMultipleClipDepths, Some(1), &[0]);
    s.define_int(kOfxImageEffectPropSupportsMultipleClipPARs, Some(1), &[0]);
    s.define_int(kOfxImageEffectPropSetableFrameRate, Some(1), &[0]);
    s.define_int(kOfxImageEffectPropSetableFielding, Some(1), &[0]);
    s.define_int(kOfxParamHostPropSupportsCustomInteract, Some(1), &[0]);
    s.define_int(kOfxParamHostPropSupportsStringAnimation, Some(1), &[0]);
    s.define_int(kOfxParamHostPropSupportsChoiceAnimation, Some(1), &[1]);
    s.define_int(kOfxParamHostPropSupportsBooleanAnimation, Some(1), &[1]);
    s.define_int(kOfxParamHostPropSupportsCustomAnimation, Some(1), &[0]);
    s.define_int(kOfxParamHostPropSupportsStrChoice, Some(1), &[1]);
    s.define_int(kOfxParamHostPropSupportsStrChoiceAnimation, Some(1), &[1]);
    s.define_int(kOfxParamHostPropSupportsParametricAnimation, Some(1), &[0]);
    s.define_int(kOfxParamHostPropMaxParameters, Some(1), &[-1]);
    s.define_int(kOfxParamHostPropMaxPages, Some(1), &[0]);
    s.define_int(kOfxParamHostPropPageRowColumnCount, Some(2), &[0, 0]);
    s.define_ptr(kOfxPropHostOSHandle, Some(1));
    // Renders in order while rendering a file, not while previewing.
    s.define_int(kOfxImageEffectInstancePropSequentialRender, Some(1), &[2]);
    s.define_str(
        kOfxImageEffectPropOpenGLRenderSupported,
        Some(1),
        &[c"false"],
    );
    s.define_str(kOfxImageEffectPropCudaRenderSupported, Some(1), &[c"false"]);
    s.define_str(kOfxImageEffectPropCudaStreamSupported, Some(1), &[c"false"]);
    s.define_str(
        kOfxImageEffectPropMetalRenderSupported,
        Some(1),
        &[c"false"],
    );
    s.define_str(
        kOfxImageEffectPropOpenCLRenderSupported,
        Some(1),
        &[c"false"],
    );
    s.define_str(kOfxImageEffectPropCPURenderSupported, Some(1), &[c"true"]);
    s.define_int(kOfxImageEffectPropRenderQualityDraft, Some(1), &[1]);
    s.define_str(
        kOfxImageEffectHostPropNativeOrigin,
        Some(1),
        &[kOfxHostNativeOriginBottomLeft],
    );
    s.define_str(
        kOfxImageEffectPropColourManagementStyle,
        Some(1),
        &[kOfxImageEffectColourManagementNone],
    );
    s
}

/// A plug-in's descriptor before it describes itself.
pub fn effect_descriptor(kind: SetKind, file: &CStr) -> PropSet {
    let s = PropSet::new(kind);
    s.define_str(kOfxPropType, Some(1), &[kOfxTypeImageEffect]);
    s.define_str(kOfxPropLabel, Some(1), &[c""]);
    s.define_str(kOfxPropShortLabel, Some(1), &[c""]);
    s.define_str(kOfxPropLongLabel, Some(1), &[c""]);
    s.define_int(kOfxPropVersion, None, &[]);
    s.define_str(kOfxPropVersionLabel, Some(1), &[c""]);
    s.define_str(kOfxPropPluginDescription, Some(1), &[c""]);
    s.define_str(kOfxImageEffectPropSupportedContexts, None, &[]);
    s.define_str(kOfxImageEffectPluginPropGrouping, Some(1), &[c""]);
    s.define_int(kOfxImageEffectPluginPropSingleInstance, Some(1), &[0]);
    s.define_str(
        kOfxImageEffectPluginRenderThreadSafety,
        Some(1),
        &[kOfxImageEffectRenderInstanceSafe],
    );
    s.define_int(kOfxImageEffectPluginPropHostFrameThreading, Some(1), &[1]);
    s.define_ptr(kOfxImageEffectPluginPropOverlayInteractV1, Some(1));
    s.define_ptr(kOfxImageEffectPluginPropOverlayInteractV2, Some(1));
    s.define_int(kOfxImageEffectPropSupportsMultiResolution, Some(1), &[1]);
    s.define_int(kOfxImageEffectPropSupportsTiles, Some(1), &[1]);
    s.define_int(kOfxImageEffectPropTemporalClipAccess, Some(1), &[0]);
    s.define_str(kOfxImageEffectPropSupportedPixelDepths, None, &[]);
    s.define_int(
        kOfxImageEffectPluginPropFieldRenderTwiceAlways,
        Some(1),
        &[1],
    );
    s.define_int(kOfxImageEffectPropSupportsMultipleClipDepths, Some(1), &[0]);
    s.define_int(kOfxImageEffectPropSupportsMultipleClipPARs, Some(1), &[0]);
    s.define_str(kOfxImageEffectPropClipPreferencesSlaveParam, None, &[]);
    s.define_int(kOfxImageEffectInstancePropSequentialRender, Some(1), &[0]);
    s.define_str(kOfxPluginPropFilePath, Some(1), &[file]);
    s.define_str(
        kOfxImageEffectPropOpenGLRenderSupported,
        Some(1),
        &[c"false"],
    );
    s.define_str(kOfxImageEffectPropCudaRenderSupported, Some(1), &[c"false"]);
    s.define_str(kOfxImageEffectPropCudaStreamSupported, Some(1), &[c"false"]);
    s.define_str(
        kOfxImageEffectPropMetalRenderSupported,
        Some(1),
        &[c"false"],
    );
    s.define_str(
        kOfxImageEffectPropOpenCLRenderSupported,
        Some(1),
        &[c"false"],
    );
    s.define_str(kOfxImageEffectPropCPURenderSupported, Some(1), &[c"true"]);
    s.define_str(
        kOfxImageEffectPropColourManagementStyle,
        Some(1),
        &[kOfxImageEffectColourManagementNone],
    );
    s.define_int(kOfxImageEffectPropNoSpatialAwareness, Some(1), &[0]);
    s
}

/// The properties an instance adds to its context's descriptor.
pub fn effect_instance(s: &PropSet, context: &CStr) {
    s.define_str(kOfxPropType, Some(1), &[kOfxTypeImageEffectInstance]);
    s.define_str(kOfxImageEffectPropContext, Some(1), &[context]);
    s.define_ptr(kOfxPropInstanceData, Some(1));
    s.define_ptr(kOfxImageEffectPropPluginHandle, Some(1));
    s.define_dbl(kOfxImageEffectPropProjectSize, Some(2), &[0.0, 0.0]);
    s.define_dbl(kOfxImageEffectPropProjectOffset, Some(2), &[0.0, 0.0]);
    s.define_dbl(kOfxImageEffectPropProjectExtent, Some(2), &[0.0, 0.0]);
    s.define_dbl(kOfxImageEffectPropProjectPixelAspectRatio, Some(1), &[1.0]);
    s.define_dbl(kOfxImageEffectInstancePropEffectDuration, Some(1), &[0.0]);
    s.define_int(kOfxImageEffectInstancePropSequentialRender, Some(1), &[0]);
    s.define_dbl(kOfxImageEffectPropFrameRate, Some(1), &[25.0]);
    s.define_int(kOfxPropIsInteractive, Some(1), &[0]);
    s.define_int(kOfxImageEffectPropInAnalysis, Some(1), &[0]);
    s.define_int(kOfxImageEffectPropSupportsTiles, Some(1), &[0]);
    s.define_str(
        kOfxImageEffectPropOpenGLRenderSupported,
        Some(1),
        &[c"false"],
    );
}

/// A clip as its plug-in defines it.
pub fn clip_descriptor(name: &CStr) -> PropSet {
    let s = PropSet::new(SetKind::ClipDescriptor);
    s.define_str(kOfxPropType, Some(1), &[kOfxTypeClip]);
    s.define_str(kOfxPropName, Some(1), &[name]);
    s.define_str(kOfxPropLabel, Some(1), &[name]);
    s.define_str(kOfxPropShortLabel, Some(1), &[name]);
    s.define_str(kOfxPropLongLabel, Some(1), &[name]);
    s.define_str(kOfxImageEffectPropSupportedComponents, None, &[]);
    s.define_int(kOfxImageEffectPropTemporalClipAccess, Some(1), &[0]);
    s.define_int(kOfxImageClipPropOptional, Some(1), &[0]);
    s.define_int(kOfxImageClipPropIsMask, Some(1), &[0]);
    s.define_str(
        kOfxImageClipPropFieldExtraction,
        Some(1),
        &[kOfxImageFieldDoubled],
    );
    s.define_int(kOfxImageEffectPropSupportsTiles, Some(1), &[1]);
    s
}

/// The properties an instance's clip adds to its descriptor's.
pub fn clip_instance(s: &PropSet) {
    s.define_str(kOfxImageEffectPropPixelDepth, Some(1), &[kOfxBitDepthNone]);
    s.define_str(
        kOfxImageEffectPropComponents,
        Some(1),
        &[kOfxImageComponentNone],
    );
    s.define_str(
        kOfxImageClipPropUnmappedPixelDepth,
        Some(1),
        &[kOfxBitDepthNone],
    );
    s.define_str(
        kOfxImageClipPropUnmappedComponents,
        Some(1),
        &[kOfxImageComponentNone],
    );
    s.define_str(
        kOfxImageEffectPropPreMultiplication,
        Some(1),
        &[kOfxImagePreMultiplied],
    );
    s.define_dbl(kOfxImagePropPixelAspectRatio, Some(1), &[1.0]);
    s.define_dbl(kOfxImageEffectPropFrameRate, Some(1), &[25.0]);
    s.define_dbl(kOfxImageEffectPropFrameRange, Some(2), &[0.0, 0.0]);
    s.define_str(kOfxImageClipPropFieldOrder, Some(1), &[kOfxImageFieldNone]);
    s.define_int(kOfxImageClipPropConnected, Some(1), &[0]);
    s.define_dbl(kOfxImageEffectPropUnmappedFrameRange, Some(2), &[0.0, 0.0]);
    s.define_dbl(kOfxImageEffectPropUnmappedFrameRate, Some(1), &[25.0]);
    s.define_int(kOfxImageClipPropContinuousSamples, Some(1), &[0]);
}

/// An image's properties (their values come with the image).
pub fn image() -> PropSet {
    let s = PropSet::new(SetKind::Image);
    s.define_str(kOfxPropType, Some(1), &[kOfxTypeImage]);
    s.define_str(kOfxImageEffectPropPixelDepth, Some(1), &[kOfxBitDepthNone]);
    s.define_str(
        kOfxImageEffectPropComponents,
        Some(1),
        &[kOfxImageComponentNone],
    );
    s.define_str(
        kOfxImageEffectPropPreMultiplication,
        Some(1),
        &[kOfxImagePreMultiplied],
    );
    s.define_dbl(kOfxImageEffectPropRenderScale, Some(2), &[1.0, 1.0]);
    s.define_dbl(kOfxImagePropPixelAspectRatio, Some(1), &[1.0]);
    s.define_ptr(kOfxImagePropData, Some(1));
    s.define_int(kOfxImagePropBounds, Some(4), &[0, 0, 0, 0]);
    s.define_int(kOfxImagePropRegionOfDefinition, Some(4), &[0, 0, 0, 0]);
    s.define_int(kOfxImagePropRowBytes, Some(1), &[0]);
    s.define_str(kOfxImagePropField, Some(1), &[kOfxImageFieldNone]);
    s.define_str(kOfxImagePropUniqueIdentifier, Some(1), &[c""]);
    s
}

/// A parameter as its plug-in defines it, with its type's defaults.
pub fn param_descriptor(ty: ParamType, name: &CStr) -> PropSet {
    let s = PropSet::new(SetKind::ParamDescriptor);
    s.define_str(kOfxPropType, Some(1), &[kOfxTypeParameter]);
    s.define_int(kOfxParamPropSecret, Some(1), &[0]);
    s.define_str(kOfxParamPropHint, Some(1), &[c""]);
    s.define_str(kOfxParamPropScriptName, Some(1), &[name]);
    s.define_str(kOfxParamPropParent, Some(1), &[c""]);
    s.define_int(kOfxParamPropEnabled, Some(1), &[1]);
    s.define_ptr(kOfxParamPropDataPtr, Some(1));
    s.define_str(kOfxParamPropType, Some(1), &[ty.ofx_name()]);
    s.define_str(kOfxPropName, Some(1), &[name]);
    s.define_str(kOfxPropLabel, Some(1), &[name]);
    s.define_str(kOfxPropShortLabel, Some(1), &[name]);
    s.define_str(kOfxPropLongLabel, Some(1), &[name]);
    s.define_str(kOfxPropIcon, Some(2), &[c"", c""]);

    if !matches!(ty, ParamType::Group | ParamType::Page) {
        // No handle of the host's own on the preview for it.
        s.define_int(kOfxParamPropHasHostOverlayHandle, Some(1), &[0]);
        s.define_ptr(kOfxParamPropInteractV1, Some(1));
        // Its own interface drawn with the draw suite (BananaV's extension).
        s.define_ptr(kBananaVParamPropInteract, Some(1));
        s.define_int(kBananaVParamPropInteractOpen, Some(1), &[1]);
        // The model its color picker shows at first (BananaV's extension).
        s.define_str(kBananaVParamPropColorModel, Some(1), &[c""]);
        // Its slider logarithmic (BananaV's extension).
        s.define_int(kBananaVParamPropLogarithmic, Some(1), &[0]);
        s.define_dbl(kOfxParamPropInteractSize, Some(2), &[0.0, 0.0]);
        s.define_dbl(kOfxParamPropInteractSizeAspect, Some(1), &[1.0]);
        s.define_int(kOfxParamPropInteractMinimumSize, Some(2), &[10, 10]);
        s.define_int(kOfxParamPropInteractPreferedSize, Some(2), &[10, 10]);
    }
    match ty {
        ParamType::Group => {
            s.define_int(kOfxParamPropGroupOpen, Some(1), &[1]);
            return s;
        }
        ParamType::Page => {
            s.define_str(kOfxParamPropPageChild, None, &[]);
            return s;
        }
        ParamType::PushButton => return s,
        _ => {}
    }
    // Every parameter with a value.
    s.define_int(
        kOfxParamPropAnimates,
        Some(1),
        &[ty.animates_by_default() as i32],
    );
    s.define_int(kOfxParamPropIsAnimating, Some(1), &[0]);
    s.define_int(kOfxParamPropIsAutoKeying, Some(1), &[0]);
    s.define_int(kOfxParamPropPersistant, Some(1), &[1]);
    s.define_int(kOfxParamPropEvaluateOnChange, Some(1), &[1]);
    s.define_int(kOfxParamPropPluginMayWrite, Some(1), &[0]);
    s.define_int(kOfxParamPropCanUndo, Some(1), &[1]);
    s.define_str(
        kOfxParamPropCacheInvalidation,
        Some(1),
        &[kOfxParamInvalidateValueChange],
    );

    let n = ty.dimension();
    match ty {
        t if t.is_text() => {
            s.define_str(kOfxParamPropDefault, Some(1), &[c""]);
            if t == ParamType::String {
                s.define_str(
                    kOfxParamPropStringMode,
                    Some(1),
                    &[kOfxParamStringIsSingleLine],
                );
                s.define_int(kOfxParamPropStringFilePathExists, Some(1), &[1]);
            }
            if t == ParamType::StrChoice {
                s.define_str(kOfxParamPropChoiceOption, None, &[]);
                s.define_str(kOfxParamPropChoiceEnum, None, &[]);
            }
            if t == ParamType::Custom {
                s.define_ptr(kOfxParamPropCustomInterpCallbackV1, Some(1));
            }
        }
        ParamType::Bytes => s.define_ptr(kOfxParamPropDefault, Some(1)),
        ParamType::Parametric => {
            s.define_int(kOfxParamPropParametricDimension, Some(1), &[1]);
            s.define_dbl(kOfxParamPropParametricUIColour, None, &[]);
            s.define_ptr(kOfxParamPropParametricInteractBackground, Some(1));
            s.define_dbl(kOfxParamPropParametricRange, Some(2), &[0.0, 1.0]);
        }
        t if t.is_int() => {
            s.define_int(kOfxParamPropDefault, Some(n), &vec![0; n]);
            if t == ParamType::Choice {
                s.define_str(kOfxParamPropChoiceOption, None, &[]);
                s.define_int(kOfxParamPropChoiceOrder, None, &[]);
            }
            if t != ParamType::Boolean && t != ParamType::Choice {
                s.define_int(kOfxParamPropMin, Some(n), &vec![i32::MIN; n]);
                s.define_int(kOfxParamPropMax, Some(n), &vec![i32::MAX; n]);
                s.define_int(kOfxParamPropDisplayMin, Some(n), &vec![i32::MIN; n]);
                s.define_int(kOfxParamPropDisplayMax, Some(n), &vec![i32::MAX; n]);
            }
            if n > 1 {
                let labels: [&CStr; 3] = [c"x", c"y", c"z"];
                s.define_str(kOfxParamPropDimensionLabel, Some(n), &labels[..n]);
            }
        }
        t => {
            // Doubles: one, a point, a color.
            let colour = matches!(t, ParamType::Rgb | ParamType::Rgba);
            s.define_dbl(kOfxParamPropDefault, Some(n), &vec![0.0; n]);
            let (lo, hi) = (-f64::MAX, f64::MAX);
            s.define_dbl(kOfxParamPropMin, Some(n), &vec![lo; n]);
            s.define_dbl(kOfxParamPropMax, Some(n), &vec![hi; n]);
            let (dlo, dhi) = if colour { (0.0, 1.0) } else { (lo, hi) };
            s.define_dbl(kOfxParamPropDisplayMin, Some(n), &vec![dlo; n]);
            s.define_dbl(kOfxParamPropDisplayMax, Some(n), &vec![dhi; n]);
            s.define_dbl(kOfxParamPropIncrement, Some(1), &[1.0]);
            s.define_int(kOfxParamPropDigits, Some(1), &[2]);
            if !colour {
                s.define_str(
                    kOfxParamPropDoubleType,
                    Some(1),
                    &[kOfxParamDoubleTypePlain],
                );
                s.define_str(
                    kOfxParamPropDefaultCoordinateSystem,
                    Some(1),
                    &[kOfxParamCoordinatesCanonical],
                );
            }
            if t == ParamType::Double {
                s.define_int(kOfxParamPropShowTimeMarker, Some(1), &[0]);
            }
            if n > 1 {
                let labels: &[&CStr] = if colour {
                    &[c"r", c"g", c"b", c"a"]
                } else {
                    &[c"x", c"y", c"z"]
                };
                s.define_str(kOfxParamPropDimensionLabel, Some(n), &labels[..n]);
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameters_get_their_types_defaults() {
        let d = param_descriptor(ParamType::Double, c"amount");
        assert_eq!(d.flag(kOfxParamPropAnimates), Some(true));
        assert_eq!(d.str(kOfxPropLabel, 0).as_deref(), Some("amount"));
        assert_eq!(d.dbls(kOfxParamPropDefault), vec![0.0]);
        let c = param_descriptor(ParamType::Choice, c"mode");
        assert_eq!(c.flag(kOfxParamPropAnimates), Some(false));
        assert!(c.has(kOfxParamPropChoiceOption));
        let rgba = param_descriptor(ParamType::Rgba, c"tint");
        assert_eq!(rgba.dbls(kOfxParamPropDisplayMax), vec![1.0; 4]);
        assert_eq!(
            rgba.strs(kOfxParamPropDimensionLabel),
            vec!["r", "g", "b", "a"]
        );
        let g = param_descriptor(ParamType::Group, c"section");
        assert!(!g.has(kOfxParamPropDefault));
    }

    #[test]
    fn the_host_offers_what_it_runs() {
        let h = host();
        assert_eq!(h.strs(kOfxImageEffectPropSupportedContexts).len(), 4);
        assert_eq!(h.str(kOfxPropLabel, 0).as_deref(), Some("BananaV"));
    }
}
