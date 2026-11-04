//! The compute kernels: their shaders, how their resources are bound, and
//! how one is run on images in buffers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use wgpu::util::DeviceExt;

/// How a kernel's buffers are bound (after the parameters, binding 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Sig {
    /// One image changed in place.
    InPlace,
    /// One image read, another written.
    Pair,
    /// Two images read, a third written.
    Triple,
    /// Packed planes and a table read, an image written.
    Yuv,
    /// An image and a table read, words written.
    Encode,
    /// Two images and a table read, a third image written.
    Quad,
}

impl Sig {
    const ALL: [Sig; 6] = [
        Sig::InPlace,
        Sig::Pair,
        Sig::Triple,
        Sig::Yuv,
        Sig::Encode,
        Sig::Quad,
    ];

    fn source(self) -> &'static str {
        match self {
            Sig::InPlace => include_str!("shaders/inplace.wgsl"),
            Sig::Pair => include_str!("shaders/pair.wgsl"),
            Sig::Triple => include_str!("shaders/transition.wgsl"),
            Sig::Yuv => include_str!("shaders/yuv.wgsl"),
            Sig::Encode => include_str!("shaders/encode.wgsl"),
            Sig::Quad => include_str!("shaders/quad.wgsl"),
        }
    }

    /// The kernels of BananaV's own effects with these bindings, after the
    /// others (`shaders/own_*.wgsl`).
    fn own(self) -> &'static str {
        match self {
            Sig::InPlace => concat!(
                include_str!("shaders/own_inplace.wgsl"),
                include_str!("shaders/color_inplace.wgsl")
            ),
            Sig::Pair => concat!(
                include_str!("shaders/own_pair.wgsl"),
                include_str!("shaders/own_fft.wgsl"),
                include_str!("shaders/color_pair.wgsl")
            ),
            Sig::Triple => concat!(
                include_str!("shaders/own_triple.wgsl"),
                include_str!("shaders/own_transitions.wgsl"),
                include_str!("shaders/own_pages.wgsl")
            ),
            Sig::Quad => concat!(
                include_str!("shaders/own_moves.wgsl"),
                include_str!("shaders/own_blinds.wgsl"),
                include_str!("shaders/own_gradient_wipe.wgsl"),
                include_str!("shaders/own_cards.wgsl"),
                include_str!("shaders/own_flow.wgsl")
            ),
            Sig::Yuv | Sig::Encode => "",
        }
    }

    /// The shader module of the kernels with these bindings: the prelude,
    /// then the kernels.
    fn module(self) -> String {
        let prelude = include_str!("shaders/prelude.wgsl");
        format!("{prelude}\n{}\n{}", self.source(), self.own())
    }

    /// Whether each buffer after the parameters is only read.
    fn read_only(self) -> &'static [bool] {
        match self {
            Sig::InPlace => &[false],
            Sig::Pair => &[true, false],
            Sig::Triple | Sig::Yuv | Sig::Encode => &[true, true, false],
            Sig::Quad => &[true, true, true, false],
        }
    }
}

/// Every kernel, by its entry point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum K {
    Fill,
    Scale,
    ToVideoGamma,
    FromVideoGamma,
    ClipRange,
    BrightnessContrast,
    ColorGrade,
    Levels,
    BlackAndWhite,
    Sepia,
    Invert,
    Vignette,
    ChromaKey,
    GlowBright,
    CopyInto,
    Place,
    PlaceHard,
    PlaceSoft,
    HalfSize,
    Composite,
    Rotate,
    Mirror,
    FlickerFilter,
    InterpolateFields,
    Blend,
    BoxH,
    BoxV,
    BlockAverage,
    BlockFill,
    Sharpen,
    AddGlow,
    Dissolve,
    FadeThroughBlack,
    Wipe,
    PushSlide,
    Iris,
    Clock,
    Zoom,
    YuvToLinear,
    EncodeRgba8,
    EncodeNv12,
    EncodeP010,
    FadeToColor,
    Shade,
    // BananaV's own effects.
    CropWindow,
    ColorCorrector,
    CookieCutter,
    Curves,
    FieldAlpha,
    FieldBoxH,
    FieldBoxV,
    LayerSurface,
    LayerUnder,
    PixelRows,
    PixelCells,
    PixelFill,
    LutFilter,
    ColorMatrix,
    EqualizeRows,
    EqualizeTotal,
    EqualizeApply,
    HslAdjust,
    PictureInPicture,
    SaturationAdjust,
    Border,
    FillCombine,
    UnsharpMask,
    AddNoise,
    Overlay,
    ColorBalance,
    ColorSecondary,
    Grade,
    AppleLog,
    BezierMasking,
    BezierMaskFx,
    KitThreshold,
    KitBlackRestore,
    KitChannelBlend,
    KitMaskGenerator,
    KitBroadcastColors,
    KitLabAdjust,
    KitColorMatch,
    KitSpherize,
    KitSwirl,
    KitPinchPunch,
    KitWave,
    KitFilmGrain,
    KitLightRays,
    KitLightRaysFinish,
    KitGradientMap,
    KitFilmEffects,
    KitTvSimulator,
    KitNewsprintCells,
    KitNewsprint,
    KitLensFlare,
    FftImage,
    FftKernel,
    FftPass,
    FftCrop,
    FftMultiply,
    FftAdd,
    KitSoftTone,
    KitSoftDiffuse,
    KitSoftVignette,
    KitRaysLights,
    KitRays,
    KitGlintLights,
    KitMotionBlurPick,
    KitFlowBetween,
    KitFlickerAdd,
    KitFlickerMatch,
    KitFlickerGather,
    KitFlickerFinish,
    KitDenoisePlanes,
    KitDenoiseNlm,
    KitDenoiseRgb,
    KitDenoiseSharpen,
    KitOffsetWrap,
    KitTinyPlanet,
    KitDeform,
    KitConvolution,
    KitQuickBlur,
    KitBumpMap,
    KitLensCorrection,
    KitMeshWarp,
    KitStabilize,
    KitStabilizeBlur,
    KitSphereTurn,
    KitStitch,
    KitStereoWarp,
    KitLanczosRows,
    KitLanczosColumns,
    KitSmartEnhance,
    KitGuidedLow,
    KitGuidedOp,
    KitGuidedMean,
    KitGuidedCoef,
    KitGuidedApply,
    KitDehazeDark,
    KitDehazeAir,
    KitDehazeOp,
    KitKeyMask,
    KitKeyApply,
    KitChromaUv,
    KitChromaApply,
    KitShear,
    KitUnshear,
    KitPolar,
    KitPolarMeans,
    KitPolarBack,
    KitRadialBlur,
    KitMedian,
    KitConvH,
    KitConvV,
    KitGaussPick,
    KitSharpen,
    KitGlowBright,
    KitGlowApply,
    KitVignette,
    KitMirror,
    KitCheckerboard,
    KitTestPattern,
    KitNoiseTexture,
    KitDisplace,
    KitHeights,
    KitHeightMap,
    KitBumpWith,
    KitFillMap,
    KitFillApply,
    KitBoxRows,
    KitBoxColumns,
    KitGaussRows,
    KitGaussColumns,
    // BananaV's own transitions.
    TrLinearWipe,
    TrClockWipe,
    TrIris,
    TrVenetianBlinds,
    TrStarWipe,
    TrMoves,
    TrGradientWipe,
    TrFlash,
    TrDissolve,
    TrCrossEffect,
    TrPages,
    TrCards,
    TrCellRows,
    TrCells,
    TrCellFill,
    // Color management (ACES).
    ColorEnter,
    ViewBegin,
    ViewEnd,
    ViewMatrix,
    ViewRange,
    ViewPower,
    ViewCurve,
    AcesGamutCompress,
    AcesRrt,
    AcesOdt,
}

impl K {
    pub const ALL: [K; 198] = [
        K::Fill,
        K::Scale,
        K::ToVideoGamma,
        K::FromVideoGamma,
        K::ClipRange,
        K::BrightnessContrast,
        K::ColorGrade,
        K::Levels,
        K::BlackAndWhite,
        K::Sepia,
        K::Invert,
        K::Vignette,
        K::ChromaKey,
        K::GlowBright,
        K::CopyInto,
        K::Place,
        K::PlaceHard,
        K::PlaceSoft,
        K::HalfSize,
        K::Composite,
        K::Rotate,
        K::Mirror,
        K::FlickerFilter,
        K::InterpolateFields,
        K::Blend,
        K::BoxH,
        K::BoxV,
        K::BlockAverage,
        K::BlockFill,
        K::Sharpen,
        K::AddGlow,
        K::Dissolve,
        K::FadeThroughBlack,
        K::Wipe,
        K::PushSlide,
        K::Iris,
        K::Clock,
        K::Zoom,
        K::YuvToLinear,
        K::EncodeRgba8,
        K::EncodeNv12,
        K::EncodeP010,
        K::FadeToColor,
        K::Shade,
        K::CropWindow,
        K::ColorCorrector,
        K::CookieCutter,
        K::Curves,
        K::FieldAlpha,
        K::FieldBoxH,
        K::FieldBoxV,
        K::LayerSurface,
        K::LayerUnder,
        K::PixelRows,
        K::PixelCells,
        K::PixelFill,
        K::LutFilter,
        K::ColorMatrix,
        K::EqualizeRows,
        K::EqualizeTotal,
        K::EqualizeApply,
        K::HslAdjust,
        K::PictureInPicture,
        K::SaturationAdjust,
        K::Border,
        K::FillCombine,
        K::UnsharpMask,
        K::AddNoise,
        K::Overlay,
        K::ColorBalance,
        K::ColorSecondary,
        K::Grade,
        K::AppleLog,
        K::BezierMasking,
        K::BezierMaskFx,
        K::KitThreshold,
        K::KitBlackRestore,
        K::KitChannelBlend,
        K::KitMaskGenerator,
        K::KitBroadcastColors,
        K::KitLabAdjust,
        K::KitColorMatch,
        K::KitSpherize,
        K::KitSwirl,
        K::KitPinchPunch,
        K::KitWave,
        K::KitFilmGrain,
        K::KitLightRays,
        K::KitLightRaysFinish,
        K::KitGradientMap,
        K::KitFilmEffects,
        K::KitTvSimulator,
        K::KitNewsprintCells,
        K::KitNewsprint,
        K::KitLensFlare,
        K::FftImage,
        K::FftKernel,
        K::FftPass,
        K::FftCrop,
        K::FftMultiply,
        K::FftAdd,
        K::KitSoftTone,
        K::KitSoftDiffuse,
        K::KitSoftVignette,
        K::KitRaysLights,
        K::KitRays,
        K::KitGlintLights,
        K::KitMotionBlurPick,
        K::KitFlowBetween,
        K::KitFlickerAdd,
        K::KitFlickerMatch,
        K::KitFlickerGather,
        K::KitFlickerFinish,
        K::KitDenoisePlanes,
        K::KitDenoiseNlm,
        K::KitDenoiseRgb,
        K::KitDenoiseSharpen,
        K::KitOffsetWrap,
        K::KitTinyPlanet,
        K::KitDeform,
        K::KitConvolution,
        K::KitQuickBlur,
        K::KitBumpMap,
        K::KitLensCorrection,
        K::KitMeshWarp,
        K::KitStabilize,
        K::KitStabilizeBlur,
        K::KitSphereTurn,
        K::KitStitch,
        K::KitStereoWarp,
        K::KitLanczosRows,
        K::KitLanczosColumns,
        K::KitSmartEnhance,
        K::KitGuidedLow,
        K::KitGuidedOp,
        K::KitGuidedMean,
        K::KitGuidedCoef,
        K::KitGuidedApply,
        K::KitDehazeDark,
        K::KitDehazeAir,
        K::KitDehazeOp,
        K::KitKeyMask,
        K::KitKeyApply,
        K::KitChromaUv,
        K::KitChromaApply,
        K::KitShear,
        K::KitUnshear,
        K::KitPolar,
        K::KitPolarMeans,
        K::KitPolarBack,
        K::KitRadialBlur,
        K::KitMedian,
        K::KitConvH,
        K::KitConvV,
        K::KitGaussPick,
        K::KitSharpen,
        K::KitGlowBright,
        K::KitGlowApply,
        K::KitVignette,
        K::KitMirror,
        K::KitCheckerboard,
        K::KitTestPattern,
        K::KitNoiseTexture,
        K::KitDisplace,
        K::KitHeights,
        K::KitHeightMap,
        K::KitBumpWith,
        K::KitFillMap,
        K::KitFillApply,
        K::KitBoxRows,
        K::KitBoxColumns,
        K::KitGaussRows,
        K::KitGaussColumns,
        K::TrLinearWipe,
        K::TrClockWipe,
        K::TrIris,
        K::TrVenetianBlinds,
        K::TrStarWipe,
        K::TrMoves,
        K::TrGradientWipe,
        K::TrFlash,
        K::TrDissolve,
        K::TrCrossEffect,
        K::TrPages,
        K::TrCards,
        K::TrCellRows,
        K::TrCells,
        K::TrCellFill,
        K::ColorEnter,
        K::ViewBegin,
        K::ViewEnd,
        K::ViewMatrix,
        K::ViewRange,
        K::ViewPower,
        K::ViewCurve,
        K::AcesGamutCompress,
        K::AcesRrt,
        K::AcesOdt,
    ];

    fn sig(self) -> Sig {
        use K::*;
        match self {
            Fill | Scale | ToVideoGamma | FromVideoGamma | ClipRange | BrightnessContrast
            | ColorGrade | Levels | BlackAndWhite | Sepia | Invert | Vignette | ChromaKey
            | GlowBright | FadeToColor | Shade | CropWindow | ColorCorrector | CookieCutter
            | ColorMatrix | HslAdjust | SaturationAdjust | Border | AddNoise | ColorBalance
            | ColorSecondary | ColorEnter | ViewBegin | ViewEnd | ViewMatrix | ViewRange
            | ViewPower | ViewCurve | AcesGamutCompress | AppleLog | KitThreshold
            | KitBlackRestore | KitChannelBlend | KitMaskGenerator | KitBroadcastColors
            | KitLabAdjust | KitColorMatch | KitSoftTone | KitVignette | KitCheckerboard
            | KitTestPattern | KitNoiseTexture | KitFlickerAdd => Sig::InPlace,
            CopyInto | Place | PlaceHard | PlaceSoft | HalfSize | Composite | Rotate | Mirror
            | FlickerFilter | InterpolateFields | Blend | BoxH | BoxV | BlockAverage
            | BlockFill | Sharpen | AddGlow | Curves | FieldAlpha | FieldBoxH | FieldBoxV
            | PixelRows | PixelCells | PixelFill | LutFilter | EqualizeRows | EqualizeTotal
            | EqualizeApply | PictureInPicture | UnsharpMask | Overlay | AcesRrt | AcesOdt
            | Grade | BezierMasking | KitSpherize | KitOffsetWrap | KitTinyPlanet | KitDeform
            | KitConvolution | KitQuickBlur | KitBumpMap | KitLensCorrection | KitStabilize
            | KitStabilizeBlur | KitSmartEnhance | KitGuidedLow | KitGuidedOp | KitGuidedMean
            | KitDehazeDark | KitKeyMask | KitKeyApply | KitChromaUv | KitChromaApply
            | KitShear | KitUnshear | KitPolar | KitPolarMeans | KitPolarBack | KitRadialBlur
            | KitGaussPick | KitSharpen | KitGlowBright | KitGlowApply | KitMirror
            | KitFillApply | KitBoxRows | KitBoxColumns | KitGaussRows | KitGaussColumns
            | FftImage | FftKernel | FftPass | FftCrop | KitRaysLights | KitGlintLights
            | KitFlickerMatch | KitFlickerFinish | KitDenoisePlanes | KitDenoiseNlm
            | KitDenoiseRgb | KitDenoiseSharpen => Sig::Pair,
            Dissolve | FadeThroughBlack | Wipe | PushSlide | Iris | Clock | Zoom | LayerSurface
            | LayerUnder | FillCombine | BezierMaskFx | KitMedian | KitConvH | KitConvV
            | KitDisplace | KitHeights | KitHeightMap | KitBumpWith | KitFillMap | KitWave
            | KitFilmGrain | KitLightRays | KitLightRaysFinish | KitGradientMap
            | KitFilmEffects | KitTvSimulator | KitNewsprintCells | KitNewsprint | KitLensFlare
            | FftMultiply | FftAdd | KitSoftDiffuse | KitSoftVignette | KitRays | KitSwirl
            | KitPinchPunch | KitMotionBlurPick | KitFlickerGather | KitMeshWarp
            | KitSphereTurn | KitStitch | KitStereoWarp | KitLanczosRows | KitLanczosColumns
            | KitGuidedCoef | KitGuidedApply | KitDehazeAir | KitDehazeOp => Sig::Triple,
            TrLinearWipe | TrClockWipe | TrIris | TrStarWipe | TrFlash | TrDissolve
            | TrCrossEffect | TrPages | TrCellRows | TrCells | TrCellFill => Sig::Triple,
            YuvToLinear => Sig::Yuv,
            EncodeRgba8 | EncodeNv12 | EncodeP010 => Sig::Encode,
            TrMoves | TrVenetianBlinds | TrGradientWipe | TrCards | KitFlowBetween => Sig::Quad,
        }
    }

    pub fn entry(self) -> &'static str {
        use K::*;
        match self {
            Fill => "fill",
            Scale => "scale",
            ToVideoGamma => "to_video_gamma",
            FromVideoGamma => "from_video_gamma",
            ClipRange => "clip_range",
            BrightnessContrast => "brightness_contrast",
            ColorGrade => "color_grade",
            Levels => "levels",
            BlackAndWhite => "black_and_white",
            Sepia => "sepia",
            Invert => "invert",
            Vignette => "vignette",
            ChromaKey => "chroma_key",
            GlowBright => "glow_bright",
            CopyInto => "copy_into",
            Place => "place",
            PlaceHard => "place_hard",
            PlaceSoft => "place_soft",
            HalfSize => "half_size",
            Composite => "composite",
            Rotate => "rotate",
            Mirror => "mirror",
            FlickerFilter => "flicker_filter",
            InterpolateFields => "interpolate_fields",
            Blend => "blend",
            BoxH => "box_h",
            BoxV => "box_v",
            BlockAverage => "block_average",
            BlockFill => "block_fill",
            Sharpen => "sharpen",
            AddGlow => "add_glow",
            Dissolve => "dissolve",
            FadeThroughBlack => "fade_through_black",
            Wipe => "wipe",
            PushSlide => "push_slide",
            Iris => "iris",
            Clock => "clock",
            Zoom => "zoom",
            YuvToLinear => "yuv_to_linear",
            EncodeRgba8 => "encode_rgba8",
            EncodeNv12 => "encode_nv12",
            EncodeP010 => "encode_p010",
            FadeToColor => "fade_to_color",
            Shade => "shade",
            CropWindow => "crop_window",
            ColorCorrector => "color_corrector",
            CookieCutter => "cookie_cutter",
            Curves => "curves",
            FieldAlpha => "field_alpha",
            FieldBoxH => "field_box_h",
            FieldBoxV => "field_box_v",
            LayerSurface => "layer_surface",
            LayerUnder => "layer_under",
            PixelRows => "pixel_rows",
            PixelCells => "pixel_cells",
            PixelFill => "pixel_fill",
            LutFilter => "lut_filter",
            ColorMatrix => "color_matrix",
            EqualizeRows => "equalize_rows",
            EqualizeTotal => "equalize_total",
            EqualizeApply => "equalize_apply",
            HslAdjust => "hsl_adjust",
            PictureInPicture => "picture_in_picture",
            SaturationAdjust => "saturation_adjust",
            Border => "border",
            FillCombine => "fill_combine",
            UnsharpMask => "unsharp_mask",
            AddNoise => "add_noise",
            Overlay => "overlay",
            ColorBalance => "color_balance",
            ColorSecondary => "color_secondary",
            Grade => "grade",
            AppleLog => "apple_log",
            BezierMasking => "bezier_masking",
            BezierMaskFx => "bezier_mask_fx",
            KitThreshold => "kit_threshold",
            KitBlackRestore => "kit_black_restore",
            KitChannelBlend => "kit_channel_blend",
            KitMaskGenerator => "kit_mask_generator",
            KitBroadcastColors => "kit_broadcast_colors",
            KitLabAdjust => "kit_lab_adjust",
            KitColorMatch => "kit_color_match",
            KitSpherize => "kit_spherize",
            KitSwirl => "kit_swirl",
            KitPinchPunch => "kit_pinch_punch",
            KitWave => "kit_wave",
            KitFilmGrain => "kit_film_grain",
            KitLightRays => "kit_light_rays",
            KitLightRaysFinish => "kit_light_rays_finish",
            KitGradientMap => "kit_gradient_map",
            KitFilmEffects => "kit_film_effects",
            KitTvSimulator => "kit_tv_simulator",
            KitNewsprintCells => "kit_newsprint_cells",
            KitNewsprint => "kit_newsprint",
            KitLensFlare => "kit_lens_flare",
            FftImage => "fft_image",
            FftKernel => "fft_kernel",
            FftPass => "fft_pass",
            FftCrop => "fft_crop",
            FftMultiply => "fft_multiply",
            FftAdd => "fft_add",
            KitSoftTone => "kit_soft_tone",
            KitSoftDiffuse => "kit_soft_diffuse",
            KitSoftVignette => "kit_soft_vignette",
            KitRaysLights => "kit_rays_lights",
            KitRays => "kit_rays",
            KitGlintLights => "kit_glint_lights",
            KitMotionBlurPick => "kit_motion_blur_pick",
            KitFlowBetween => "kit_flow_between",
            KitFlickerAdd => "kit_flicker_add",
            KitFlickerMatch => "kit_flicker_match",
            KitFlickerGather => "kit_flicker_gather",
            KitFlickerFinish => "kit_flicker_finish",
            KitDenoisePlanes => "kit_denoise_planes",
            KitDenoiseNlm => "kit_denoise_nlm",
            KitDenoiseRgb => "kit_denoise_rgb",
            KitDenoiseSharpen => "kit_denoise_sharpen",
            KitOffsetWrap => "kit_offset_wrap",
            KitTinyPlanet => "kit_tiny_planet",
            KitDeform => "kit_deform",
            KitConvolution => "kit_convolution",
            KitQuickBlur => "kit_quick_blur",
            KitBumpMap => "kit_bump_map",
            KitLensCorrection => "kit_lens_correction",
            KitMeshWarp => "kit_mesh_warp",
            KitStabilize => "kit_stabilize",
            KitStabilizeBlur => "kit_stabilize_blur",
            KitSphereTurn => "kit_sphere_turn",
            KitStitch => "kit_stitch",
            KitStereoWarp => "kit_stereo_warp",
            KitLanczosRows => "kit_lanczos_rows",
            KitLanczosColumns => "kit_lanczos_columns",
            KitSmartEnhance => "kit_smart_enhance",
            KitGuidedLow => "kit_guided_low",
            KitGuidedOp => "kit_guided_op",
            KitGuidedMean => "kit_guided_mean",
            KitGuidedCoef => "kit_guided_coef",
            KitGuidedApply => "kit_guided_apply",
            KitDehazeDark => "kit_dehaze_dark",
            KitDehazeAir => "kit_dehaze_air",
            KitDehazeOp => "kit_dehaze_op",
            KitKeyMask => "kit_key_mask",
            KitKeyApply => "kit_key_apply",
            KitChromaUv => "kit_chroma_uv",
            KitChromaApply => "kit_chroma_apply",
            KitShear => "kit_shear",
            KitUnshear => "kit_unshear",
            KitPolar => "kit_polar",
            KitPolarMeans => "kit_polar_means",
            KitPolarBack => "kit_polar_back",
            KitRadialBlur => "kit_radial_blur",
            KitMedian => "kit_median",
            KitConvH => "kit_conv_h",
            KitConvV => "kit_conv_v",
            KitGaussPick => "kit_gauss_pick",
            KitSharpen => "kit_sharpen",
            KitGlowBright => "kit_glow_bright",
            KitGlowApply => "kit_glow_apply",
            KitVignette => "kit_vignette",
            KitMirror => "kit_mirror",
            KitCheckerboard => "kit_checkerboard",
            KitTestPattern => "kit_test_pattern",
            KitNoiseTexture => "kit_noise_texture",
            KitDisplace => "kit_displace",
            KitHeights => "kit_heights",
            KitHeightMap => "kit_height_map",
            KitBumpWith => "kit_bump_with",
            KitFillMap => "kit_fill_map",
            KitFillApply => "kit_fill_apply",
            KitBoxRows => "kit_box_rows",
            KitBoxColumns => "kit_box_columns",
            KitGaussRows => "kit_gauss_rows",
            KitGaussColumns => "kit_gauss_columns",
            TrLinearWipe => "tr_linear_wipe",
            TrClockWipe => "tr_clock_wipe",
            TrIris => "tr_iris",
            TrVenetianBlinds => "tr_venetian_blinds",
            TrStarWipe => "tr_star_wipe",
            TrMoves => "tr_moves",
            TrGradientWipe => "tr_gradient_wipe",
            TrFlash => "tr_flash",
            TrDissolve => "tr_dissolve",
            TrCrossEffect => "tr_cross_effect",
            TrPages => "tr_pages",
            TrCards => "tr_cards",
            TrCellRows => "tr_cell_rows",
            TrCells => "tr_cells",
            TrCellFill => "tr_cell_fill",
            ColorEnter => "color_enter",
            ViewBegin => "view_begin",
            ViewEnd => "view_end",
            ViewMatrix => "view_matrix",
            ViewRange => "view_range",
            ViewPower => "view_power",
            ViewCurve => "view_curve",
            AcesGamutCompress => "aces_gamut_compress",
            AcesRrt => "aces_rrt",
            AcesOdt => "aces_odt",
        }
    }
}

/// A kernel's parameters (`Params` in `shaders/prelude.wgsl`).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Params {
    /// The size written.
    pub w: u32,
    pub h: u32,
    /// The size read.
    pub sw: u32,
    pub sh: u32,
    pub i: [i32; 4],
    pub j: [u32; 4],
    pub o: [u32; 4],
    pub f: [f32; 24],
}

impl Params {
    pub fn sized(w: u32, h: u32) -> Params {
        Params {
            w,
            h,
            sw: w,
            sh: h,
            ..Params::default()
        }
    }

    pub fn with_f(mut self, values: &[f32]) -> Params {
        self.f[..values.len()].copy_from_slice(values);
        self
    }

    fn bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(160);
        for x in [self.w, self.h, self.sw, self.sh] {
            v.extend_from_slice(&x.to_le_bytes());
        }
        for x in self.i {
            v.extend_from_slice(&x.to_le_bytes());
        }
        for x in self.j.iter().chain(&self.o) {
            v.extend_from_slice(&x.to_le_bytes());
        }
        for x in self.f {
            v.extend_from_slice(&x.to_le_bytes());
        }
        v
    }
}

/// How many workgroups cover the work.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Grid {
    /// One invocation a pixel of `w` × `h` (16 × 16 a group).
    Pixels(u32, u32),
    /// One invocation each of `n` rows or columns (64 a group).
    Lines(u32),
    /// `n` groups (a row each, say), as big as the kernel says.
    Groups(u32),
}

/// The shaders, compiled once per device; pipelines are made the first
/// time a kernel runs, or by `Gpu::warm_up` before that.
pub(crate) struct Kernels {
    modules: HashMap<Sig, wgpu::ShaderModule>,
    layouts: HashMap<Sig, (wgpu::BindGroupLayout, wgpu::PipelineLayout)>,
    /// Each kernel's own cell: the compiler (a kernel may take seconds)
    /// runs outside the map's lock, so a frame waits for the kernels it
    /// uses and for no other.
    pipelines: Mutex<HashMap<K, Arc<OnceLock<wgpu::ComputePipeline>>>>,
    /// Kernels made at run time, by their source (`None`: not valid WGSL),
    /// the last ones kept.
    made: Mutex<Vec<(String, Option<wgpu::ComputePipeline>)>>,
}

/// How many kernels made at run time are kept.
const MADE: usize = 16;

impl Kernels {
    pub fn new(device: &wgpu::Device) -> Kernels {
        let mut modules = HashMap::new();
        let mut layouts = HashMap::new();
        for sig in Sig::ALL {
            let source = sig.module();
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&format!("bananav {sig:?}")),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            modules.insert(sig, module);
            let mut entries = vec![wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }];
            for (k, &read_only) in sig.read_only().iter().enumerate() {
                entries.push(wgpu::BindGroupLayoutEntry {
                    binding: k as u32 + 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                });
            }
            let group = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(&format!("bananav {sig:?}")),
                entries: &entries,
            });
            let pipeline = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(&format!("bananav {sig:?}")),
                bind_group_layouts: &[Some(&group)],
                immediate_size: 0,
            });
            layouts.insert(sig, (group, pipeline));
        }
        Kernels {
            modules,
            layouts,
            pipelines: Mutex::new(HashMap::new()),
            made: Mutex::new(Vec::new()),
        }
    }

    /// A kernel made at run time: `source` after the prelude, with the
    /// bindings of `sig`'s kernels and entry point `entry` — checked as
    /// WGSL first (`None` when it is not valid, the card never seeing it).
    fn made(
        &self,
        device: &wgpu::Device,
        sig: Sig,
        source: &str,
        entry: &str,
    ) -> Option<wgpu::ComputePipeline> {
        let mut made = self.made.lock().expect("kernels made");
        if let Some((_, p)) = made.iter().find(|(s, _)| s == source) {
            return p.clone();
        }
        let full = format!("{}\n{source}", include_str!("shaders/prelude.wgsl"));
        let valid = naga::front::wgsl::parse_str(&full).ok().is_some_and(|m| {
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::default(),
            )
            .validate(&m)
            .is_ok()
                && m.entry_points.iter().any(|e| e.name == entry)
        });
        let pipeline = valid.then(|| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("bananav made"),
                source: wgpu::ShaderSource::Wgsl(full.into()),
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&self.layouts[&sig].1),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        });
        if made.len() >= MADE {
            made.remove(0);
        }
        made.push((source.to_string(), pipeline.clone()));
        pipeline
    }

    /// Records a kernel made at run time ([`Kernels::made`]: `source`, its
    /// entry point `entry`, `sig`'s bindings) on `buffers`; false when its
    /// source is not valid WGSL (nothing recorded).
    #[allow(clippy::too_many_arguments)]
    pub fn run_made(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        (sig, source, entry): (Sig, &str, &str),
        params: &Params,
        buffers: &[&wgpu::Buffer],
        grid: Grid,
    ) -> bool {
        let Some(pipeline) = self.made(device, sig, source, entry) else {
            return false;
        };
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params"),
            contents: &params.bytes(),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }];
        for (n, b) in buffers.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: n as u32 + 1,
                resource: b.as_entire_binding(),
            });
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(entry),
            layout: &self.layouts[&sig].0,
            entries: &entries,
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(entry),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        match grid {
            Grid::Pixels(w, h) => pass.dispatch_workgroups(w.div_ceil(16), h.div_ceil(16), 1),
            Grid::Lines(n) => pass.dispatch_workgroups(n.div_ceil(64), 1, 1),
            Grid::Groups(n) => pass.dispatch_workgroups(n, 1, 1),
        }
        true
    }

    pub fn pipeline(&self, device: &wgpu::Device, k: K) -> wgpu::ComputePipeline {
        let cell = self
            .pipelines
            .lock()
            .expect("pipelines")
            .entry(k)
            .or_default()
            .clone();
        cell.get_or_init(|| {
            let sig = k.sig();
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(k.entry()),
                layout: Some(&self.layouts[&sig].1),
                module: &self.modules[&sig],
                entry_point: Some(k.entry()),
                // The kernels write their `var<workgroup>` tables before
                // reading them; zeroing them first, an element at a time
                // for FXC, costs tens of seconds to compile.
                compilation_options: wgpu::PipelineCompilationOptions {
                    zero_initialize_workgroup_memory: false,
                    ..Default::default()
                },
                cache: None,
            })
        })
        .clone()
    }

    /// Records kernel `k` on `buffers` (in binding order after the
    /// parameters).
    pub fn run(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        k: K,
        params: &Params,
        buffers: &[&wgpu::Buffer],
        grid: Grid,
    ) {
        let sig = k.sig();
        debug_assert_eq!(buffers.len(), sig.read_only().len(), "{k:?}");
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params"),
            contents: &params.bytes(),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }];
        for (n, b) in buffers.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: n as u32 + 1,
                resource: b.as_entire_binding(),
            });
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(k.entry()),
            layout: &self.layouts[&sig].0,
            entries: &entries,
        });
        let pipeline = self.pipeline(device, k);
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(k.entry()),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        match grid {
            Grid::Pixels(w, h) => pass.dispatch_workgroups(w.div_ceil(16), h.div_ceil(16), 1),
            Grid::Lines(n) => pass.dispatch_workgroups(n.div_ceil(64), 1, 1),
            Grid::Groups(n) => pass.dispatch_workgroups(n, 1, 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shader module is valid WGSL with every kernel's entry point,
    /// checked without a graphics card (where none runs the tests, a
    /// reserved word or a wrong type would go unseen: the card's tests
    /// pass without one).
    #[test]
    fn the_shaders_are_valid_wgsl_with_every_kernel() {
        for sig in Sig::ALL {
            let source = sig.module();
            let module = naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|e| panic!("{sig:?}: {}", e.emit_to_string(&source)));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{sig:?}: {e:?}"));
            for k in K::ALL.into_iter().filter(|k| k.sig() == sig) {
                assert!(
                    module.entry_points.iter().any(|e| e.name == k.entry()),
                    "{k:?}: no entry point {} in {sig:?}",
                    k.entry()
                );
            }
        }
    }
}
