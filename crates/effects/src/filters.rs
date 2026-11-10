//! More of VEGAS's video effects, as OpenFX filters written as tables of
//! settings (VEGAS's, as its Help describes them): Tiny Planet, Defocus,
//! Linear Blur, Quick Blur, Radial Blur, Median, Min and Max,
//! Convolution Kernel, Spherize, Swirl, Threshold, Wave, Pinch/Punch,
//! Offset and Wrap, Black Restore, Broadcast Colors, Channel Blend, Chroma
//! Blur, Mask Generator, Lens Correction, Radial Pixelate and Deform —
//! and, worked out in [`crate::looks`], the color, light and film ones
//! (LAB Adjust, Soft Contrast, Gradient Map, Fill Light, Lens Flare,
//! Starburst, Glint, Light Rays, Film Effects, Film Grain, Newsprint, TV
//! Simulator, Bump Map, Denoise and Looks), and in [`crate::classics`]
//! Chroma Keyer. Each
//! reads its source once ([`Buf`]) and works out the whole picture
//! ([`Kind::process`]): moved pixels are sampled in the frame's 0–1
//! coordinates, neighbourhoods counted in the picture's own pixels (sizes
//! typed in pixels are the frame's at full size). VEGAS's pictures of them
//! wait to be measured; their looks follow the Help's words. On the
//! graphics card they go through the processor until they have kernels.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::{PixelKernel, Plan, RemapKernel};
use crate::kit::{Buf, Grid, Spec, Vals, define_all, gauss_plane};
use crate::ofx::{Draw, Fx, Props, interact_props, is, near_handle, redraw, suites};
use crate::{Preset, PresetValue};

/// One of the effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    TinyPlanet,
    Defocus,
    LinearBlur,
    QuickBlur,
    RadialBlur,
    Median,
    MinMax,
    Convolution,
    Spherize,
    Swirl,
    Threshold,
    Wave,
    PinchPunch,
    OffsetWrap,
    BlackRestore,
    BroadcastColors,
    ChannelBlend,
    ChromaBlur,
    MaskGenerator,
    RadialPixelate,
    Deform,
    LabAdjust,
    SoftContrast,
    FillLight,
    LensFlare,
    Starburst,
    Glint,
    Rays,
    LightRays,
    FilmEffects,
    FilmGrain,
    Newsprint,
    TvSimulator,
    BumpMap,
    Denoise,
    Looks,
    ChromaKeyer,
}

const CENTER: Spec = Spec::Point(
    c"center",
    c"Center",
    [0.5, 0.5],
    c"The effect's center: 0, 0 is the lower left corner.",
);
const PROPORTIONAL: Spec = Spec::Toggle(
    c"proportional",
    c"Proportional",
    true,
    c"The height follows the width.",
);
const HORIZONTAL_SIZE: Spec = Spec::Number(
    c"horizontal",
    c"Horizontal",
    1.0,
    0.0,
    1.0,
    c"How wide the effect is: 1 is the frame's width.",
);
const VERTICAL_SIZE: Spec = Spec::Number(
    c"vertical",
    c"Vertical",
    1.0,
    0.0,
    1.0,
    c"How tall the effect is: 1 is the frame's height (unless proportional).",
);
const CIRCULAR: Spec = Spec::Toggle(
    c"circular",
    c"Circular area processing",
    false,
    c"A round area around each pixel; else a rectangle.",
);
/// Median and Min and Max's reach across (VEGAS's: 1 is a twentieth of the
/// frame's longer side each way).
const fn range_x(default: f64) -> Spec {
    Spec::Number(
        c"horizontalRange",
        c"Horizontal range",
        default,
        0.0,
        1.0,
        c"How far across each pixel's area reaches: 1 is a twentieth of the frame's longer side each way.",
    )
}
const fn range_y(default: f64) -> Spec {
    Spec::Number(
        c"verticalRange",
        c"Vertical range",
        default,
        0.0,
        1.0,
        c"How far up and down each pixel's area reaches: 1 is a twentieth of the frame's longer side each way.",
    )
}
const WRAPS: &[&CStr] = &[c"Repeat", c"Mirror", c"Off"];

const TINY_PLANET: &[Spec] = &[
    Spec::Ranged(
        c"latitude",
        c"Latitude",
        0.0,
        [-18000.0, 18000.0],
        [0.0, 180.0],
        c"The sphere turned up or down, in degrees: 0 looks at the floor, 180 at the sky.",
    ),
    Spec::Ranged(
        c"longitude",
        c"Longitude",
        0.0,
        [-36000.0, 36000.0],
        [0.0, 360.0],
        c"The sphere turned left or right, in degrees.",
    ),
    Spec::Ranged(
        c"z",
        c"Z",
        20.0,
        [-10000.0, 10000.0],
        [0.0, 100.0],
        c"How near the sphere is: higher shows less of it, bigger.",
    ),
];
const DEFOCUS: &[Spec] = &[
    Spec::Number(
        c"radius",
        c"Radius",
        5.6,
        0.0,
        10.0,
        c"How much the picture defocuses: the iris's radius, in percent of the picture's longer side.",
    ),
    Spec::Choice(
        c"irisShape",
        c"Iris shape",
        &[
            c"Circle",
            c"Pentagon",
            c"Hexagon",
            c"Heptagon",
            c"Octagon",
        ],
        0,
        c"The shape the lens's iris gives the blooms of light.",
    ),
    Spec::Number(
        c"orientation",
        c"Orientation",
        0.0,
        0.0,
        360.0,
        c"The iris turned, in degrees.",
    ),
    Spec::Number(
        c"discCurvature",
        c"Disc curvature",
        0.0,
        -100.0,
        100.0,
        c"How the iris's light goes from its middle to its rim: below 0 the rim brighter, above it the middle.",
    ),
    Spec::Number(
        c"bloomThreshold",
        c"Bloom threshold",
        94.97,
        50.0,
        100.0,
        c"How bright a light must be to bloom, in percent.",
    ),
    Spec::Number(
        c"bloomStrength",
        c"Bloom strength",
        10.51,
        1.0,
        50.0,
        c"How bright the brightest lights bloom.",
    ),
    Spec::Toggle(
        c"reduceFlicker",
        c"Reduce flicker",
        false,
        c"The picture smoothed a little first, against flickering.",
    ),
];
const LINEAR_BLUR: &[Spec] = &[
    Spec::Ranged(
        c"angle",
        c"Angle",
        0.0,
        [0.0, 360.0],
        [0.0, 360.0],
        c"The blur's direction, in degrees: 0 across, 90 up and down.",
    ),
    Spec::Number(
        c"amount",
        c"Amount",
        0.0,
        0.0,
        1.0,
        c"How much it blurs: 0 none, 1 the most.",
    ),
];
const RADIAL_BLUR: &[Spec] = &[
    CENTER,
    Spec::Choice(
        c"type",
        c"Type",
        &[c"Proportional", c"Fixed radius", c"Gaussian"],
        0,
        c"Proportional: each step toward the center a part of the way there, so the blur grows away from it. Fixed radius and Gaussian: steps of the same length all over.",
    ),
    Spec::Number(
        c"strength",
        c"Strength",
        0.0,
        0.0,
        1.0,
        c"How much it blurs: 0 none, 1 the most.",
    ),
];
const QUICK_BLUR: &[Spec] = &[Spec::Number(
    c"blendAmount",
    c"Blend amount",
    0.0,
    0.0,
    1.0,
    c"How much of the blurred picture shows: 0 none of it, 1 all of it.",
)];
const MEDIAN: &[Spec] = &[
    range_x(0.1),
    range_y(0.1),
    Spec::Number(
        c"offset",
        c"Offset",
        0.5,
        0.0,
        1.0,
        c"Which of each area's values: 0 its darkest, 0.5 its middle one, 1 its brightest.",
    ),
    CIRCULAR,
];
const MIN_MAX: &[Spec] = &[
    Spec::Choice(
        c"operation",
        c"Operation",
        &[c"Minimum", c"Maximum"],
        1,
        c"Each pixel takes its area's darkest or its brightest value.",
    ),
    range_x(0.08),
    range_y(0.08),
    CIRCULAR,
];
const CONVOLUTION: &[Spec] = &[
    Spec::Group(c"row1", c"Row 1"),
    Spec::Number(
        c"r1c1",
        c"Column 1",
        0.0,
        -100.0,
        100.0,
        c"A weight of the matrix.",
    ),
    Spec::Number(
        c"r1c2",
        c"Column 2",
        0.0,
        -100.0,
        100.0,
        c"A weight of the matrix.",
    ),
    Spec::Number(
        c"r1c3",
        c"Column 3",
        0.0,
        -100.0,
        100.0,
        c"A weight of the matrix.",
    ),
    Spec::GroupEnd,
    Spec::Group(c"row2", c"Row 2"),
    Spec::Number(
        c"r2c1",
        c"Column 1",
        0.0,
        -100.0,
        100.0,
        c"A weight of the matrix.",
    ),
    Spec::Number(
        c"r2c2",
        c"Column 2",
        1.0,
        -100.0,
        100.0,
        c"The pixel's own weight.",
    ),
    Spec::Number(
        c"r2c3",
        c"Column 3",
        0.0,
        -100.0,
        100.0,
        c"A weight of the matrix.",
    ),
    Spec::GroupEnd,
    Spec::Group(c"row3", c"Row 3"),
    Spec::Number(
        c"r3c1",
        c"Column 1",
        0.0,
        -100.0,
        100.0,
        c"A weight of the matrix.",
    ),
    Spec::Number(
        c"r3c2",
        c"Column 2",
        0.0,
        -100.0,
        100.0,
        c"A weight of the matrix.",
    ),
    Spec::Number(
        c"r3c3",
        c"Column 3",
        0.0,
        -100.0,
        100.0,
        c"A weight of the matrix.",
    ),
    Spec::GroupEnd,
    Spec::Ranged(
        c"rotation",
        c"Rotation",
        0.0,
        [-3600.0, 3600.0],
        [-360.0, 360.0],
        c"The outer weights turned round the middle one, in degrees: each 45° moves them one place.",
    ),
    Spec::Number(
        c"scale",
        c"Scale",
        1.0,
        -100.0,
        100.0,
        c"The weights times this (without Auto normalize).",
    ),
    Spec::Number(
        c"offset",
        c"Offset",
        0.0,
        0.0,
        1.0,
        c"Added to each channel: 1 adds white.",
    ),
    Spec::Toggle(
        c"autoNormalize",
        c"Auto normalize",
        true,
        c"The weights scaled to add up to 1.",
    ),
];
const SPHERIZE: &[Spec] = &[
    CENTER,
    Spec::Number(
        c"amount",
        c"Amount",
        1.0,
        -1.0,
        1.0,
        c"How much it bulges: above 0 out, below 0 in.",
    ),
    Spec::Group(c"scaling", c"Scaling"),
    HORIZONTAL_SIZE,
    VERTICAL_SIZE,
    PROPORTIONAL,
    Spec::GroupEnd,
];
const SWIRL: &[Spec] = &[
    CENTER,
    Spec::Number(
        c"amount",
        c"Amount",
        1.0,
        -1.0,
        1.0,
        c"How much it twists: above 0 clockwise, below 0 the other way.",
    ),
    Spec::Group(c"scaling", c"Scaling"),
    HORIZONTAL_SIZE,
    VERTICAL_SIZE,
    PROPORTIONAL,
    Spec::GroupEnd,
];
const THRESHOLD: &[Spec] = &[Spec::Number(
    c"threshold",
    c"Threshold",
    0.5,
    0.0,
    1.0,
    c"Each of red, green and blue this bright or brighter becomes full, darker none.",
)];
const WAVE: &[Spec] = &[
    Spec::Number(
        c"verticalWaves",
        c"Vertical waves",
        5.4,
        0.0,
        100.0,
        c"How many waves move the picture up and down.",
    ),
    Spec::Number(
        c"horizontalWaves",
        c"Horizontal waves",
        5.4,
        0.0,
        100.0,
        c"How many waves move the picture across.",
    ),
    Spec::Number(
        c"verticalAmplitude",
        c"Vertical amplitude",
        0.05,
        0.0,
        1.0,
        c"How far the waves move it up and down: 1 is the frame's height.",
    ),
    Spec::Number(
        c"horizontalAmplitude",
        c"Horizontal amplitude",
        0.05,
        0.0,
        1.0,
        c"How far the waves move it across: 1 is the frame's width.",
    ),
    Spec::Number(
        c"verticalPhase",
        c"Vertical phase",
        0.0,
        0.0,
        1.0,
        c"Where the waves are.",
    ),
    Spec::Number(
        c"horizontalPhase",
        c"Horizontal phase",
        0.0,
        0.0,
        1.0,
        c"Where the waves are.",
    ),
];
const PINCH_PUNCH: &[Spec] = &[
    CENTER,
    Spec::Number(
        c"amount",
        c"Amount",
        1.0,
        -1.0,
        1.0,
        c"Above 0 punches out of the center, below 0 pinches toward it.",
    ),
    Spec::Group(c"scaling", c"Scaling"),
    HORIZONTAL_SIZE,
    VERTICAL_SIZE,
    PROPORTIONAL,
    Spec::GroupEnd,
];
const OFFSET_WRAP: &[Spec] = &[
    Spec::Pair(
        c"offset",
        c"Offset",
        [0.0, 0.0],
        [-1000.0, 1000.0],
        [-1.0, 1.0],
        c"How far the picture moves: 1 is the frame's width (or height).",
    ),
    Spec::Ranged(
        c"count",
        c"Count",
        1.0,
        [0.0, 50.0],
        [0.0, 5.0],
        c"How many times the picture fits across: above 1 it repeats smaller.",
    ),
    Spec::Group(c"wrapMode", c"Wrap Mode"),
    Spec::Choice(
        c"horizontalWrap",
        c"Horizontal",
        WRAPS,
        0,
        c"Past the left and right edges: the picture repeated, mirrored or nothing.",
    ),
    Spec::Choice(
        c"verticalWrap",
        c"Vertical",
        WRAPS,
        0,
        c"Past the top and bottom edges: the picture repeated, mirrored or nothing.",
    ),
    Spec::GroupEnd,
];
const BLACK_RESTORE: &[Spec] = &[Spec::Number(
    c"threshold",
    c"Threshold",
    0.0,
    0.0,
    1.0,
    c"Pixels whose red, green and blue are all at or under it become black.",
)];
const BROADCAST_COLORS: &[Spec] = &[
    Spec::Group(c"luma", c"Luma"),
    Spec::Number(
        c"lumaMin",
        c"Min",
        -8.0,
        -8.0,
        40.0,
        c"The lowest luminance kept, in IRE.",
    ),
    Spec::Number(
        c"lumaMinSmoothness",
        c"Smoothness",
        0.0,
        0.0,
        1.0,
        c"0 clamps at the value; higher bends toward it.",
    ),
    Spec::Number(
        c"lumaMax",
        c"Max",
        110.0,
        60.0,
        110.0,
        c"The highest luminance kept, in IRE.",
    ),
    Spec::Number(
        c"lumaMaxSmoothness",
        c"Smoothness",
        0.0,
        0.0,
        1.0,
        c"0 clamps at the value; higher bends toward it.",
    ),
    Spec::GroupEnd,
    Spec::Group(c"chroma", c"Chroma"),
    Spec::Number(
        c"chromaMax",
        c"Max",
        148.0,
        0.0,
        148.0,
        c"The most chrominance kept, peak to peak in IRE.",
    ),
    Spec::Number(
        c"chromaSmoothness",
        c"Smoothness",
        0.0,
        0.0,
        1.0,
        c"0 clamps at the value; higher bends toward it.",
    ),
    Spec::GroupEnd,
    Spec::Group(c"composite", c"Composite"),
    Spec::Number(
        c"compositeMin",
        c"Min",
        -47.0,
        -47.0,
        -8.0,
        c"The lowest luminance less chrominance kept, in IRE.",
    ),
    Spec::Number(
        c"compositeMinSmoothness",
        c"Smoothness",
        0.0,
        0.0,
        1.0,
        c"0 clamps at the value; higher bends toward it.",
    ),
    Spec::Number(
        c"compositeMax",
        c"Max",
        148.0,
        100.0,
        148.0,
        c"The highest luminance plus chrominance kept, in IRE.",
    ),
    Spec::Number(
        c"compositeMaxSmoothness",
        c"Smoothness",
        0.0,
        0.0,
        1.0,
        c"0 clamps at the value; higher bends toward it.",
    ),
    Spec::GroupEnd,
    Spec::Toggle(
        c"setup",
        c"7.5 IRE setup",
        true,
        c"Black at 7.5 IRE, as NTSC video in the United States.",
    ),
    Spec::Toggle(
        c"studioRgb",
        c"Studio RGB (16 to 235)",
        true,
        c"The colors in 16 (black) to 235 (white).",
    ),
];
const CHROMA_BLUR: &[Spec] = &[
    Spec::Ranged(
        c"horizontalPixels",
        c"Horizontal pixels",
        0.0,
        [0.0, 100.0],
        [0.0, 10.0],
        c"How wide the color's blur is, in pixels.",
    ),
    Spec::Ranged(
        c"verticalPixels",
        c"Vertical pixels",
        0.0,
        [0.0, 100.0],
        [0.0, 10.0],
        c"How tall the color's blur is, in pixels.",
    ),
];
const MASK_GENERATOR: &[Spec] = &[
    Spec::Choice(
        c"type",
        c"Type",
        &[c"Luminance", c"Red", c"Green", c"Blue", c"Alpha"],
        0,
        c"What the mask is made of.",
    ),
    Spec::Toggle(
        c"invert",
        c"Invert",
        false,
        c"The values turned around before Low and High in.",
    ),
    Spec::Number(
        c"lowIn",
        c"Low in",
        0.0,
        0.0,
        1.0,
        c"Values up to this give Low out.",
    ),
    Spec::Number(
        c"highIn",
        c"High in",
        1.0,
        0.0,
        1.0,
        c"Values from this up give High out.",
    ),
    Spec::Number(
        c"lowOut",
        c"Low out",
        0.0,
        0.0,
        1.0,
        c"The mask's value for the lowest values.",
    ),
    Spec::Number(
        c"highOut",
        c"High out",
        1.0,
        0.0,
        1.0,
        c"The mask's value for the highest values.",
    ),
];
const RADIAL_PIXELATE: &[Spec] = &[
    CENTER,
    Spec::Choice(
        c"type",
        c"Type",
        &[c"Radial", c"Circular"],
        0,
        c"Radial: rays from the center. Circular: rings around it.",
    ),
    Spec::Number(
        c"strength",
        c"Strength",
        0.0,
        0.0,
        1.0,
        c"How coarse it is: 0 not at all.",
    ),
];
const DEFORM: &[Spec] = &[
    Spec::Number(
        c"amount",
        c"Amount",
        1.0,
        -1.0,
        1.0,
        c"How much of the deformation, and which way.",
    ),
    Spec::Toggle(
        c"centerImage",
        c"Center image",
        false,
        c"The picture kept in the middle after the deformation.",
    ),
    Spec::Color(
        c"background",
        c"Background",
        [0.0, 0.0, 0.0, 0.0],
        c"What shows where the picture no longer reaches.",
    ),
    Spec::Group(c"squeeze", c"Squeeze"),
    Spec::Number(
        c"squeezeLeft",
        c"Left",
        0.0,
        -1.0,
        1.0,
        c"The left edge squeezed (above 0) or spread.",
    ),
    Spec::Number(
        c"squeezeRight",
        c"Right",
        0.0,
        -1.0,
        1.0,
        c"The right edge squeezed (above 0) or spread.",
    ),
    Spec::Number(
        c"squeezeTop",
        c"Top",
        0.0,
        -1.0,
        1.0,
        c"The top edge squeezed (above 0) or spread.",
    ),
    Spec::Number(
        c"squeezeBottom",
        c"Bottom",
        0.0,
        -1.0,
        1.0,
        c"The bottom edge squeezed (above 0) or spread.",
    ),
    Spec::GroupEnd,
    Spec::Group(c"shear", c"Shear"),
    Spec::Number(
        c"shearHorizontal",
        c"Horizontal",
        0.0,
        -1.0,
        1.0,
        c"The picture slanted across.",
    ),
    Spec::Number(
        c"shearVertical",
        c"Vertical",
        0.0,
        -1.0,
        1.0,
        c"The picture slanted up and down.",
    ),
    Spec::GroupEnd,
    Spec::Group(c"curve", c"Curve"),
    Spec::Number(
        c"curveLeft",
        c"Left",
        0.0,
        -1.0,
        1.0,
        c"The left edge bent.",
    ),
    Spec::Number(
        c"curveRight",
        c"Right",
        0.0,
        -1.0,
        1.0,
        c"The right edge bent.",
    ),
    Spec::Number(c"curveTop", c"Top", 0.0, -1.0, 1.0, c"The top edge bent."),
    Spec::Number(
        c"curveBottom",
        c"Bottom",
        0.0,
        -1.0,
        1.0,
        c"The bottom edge bent.",
    ),
    Spec::GroupEnd,
];

/// Channel Blend's sliders: each output channel's weights and offset.
const BLEND: [&CStr; 20] = [
    c"redRed",
    c"redGreen",
    c"redBlue",
    c"redAlpha",
    c"redOffset",
    c"greenRed",
    c"greenGreen",
    c"greenBlue",
    c"greenAlpha",
    c"greenOffset",
    c"blueRed",
    c"blueGreen",
    c"blueBlue",
    c"blueAlpha",
    c"blueOffset",
    c"alphaRed",
    c"alphaGreen",
    c"alphaBlue",
    c"alphaAlpha",
    c"alphaOffset",
];
/// Each row's labels, as VEGAS's (its group names the output).
const BLEND_LABELS: [&CStr; 5] = [
    c"Red channel weighting",
    c"Green channel weighting",
    c"Blue channel weighting",
    c"Alpha channel weighting",
    c"Offset",
];
/// The rows' groups: their names and labels.
const BLEND_GROUPS: [(&CStr, &CStr); 4] = [
    (c"redOutput", c"Red Output"),
    (c"greenOutput", c"Green Output"),
    (c"blueOutput", c"Blue Output"),
    (c"alphaOutput", c"Alpha Output"),
];

/// Channel Blend's parameters, made once: Normalize, then a group for
/// each output with its weights and offset (VEGAS's: typed within ±255,
/// the sliders ±10).
fn channel_blend_specs() -> &'static [Spec] {
    static SPECS: std::sync::OnceLock<Vec<Spec>> = std::sync::OnceLock::new();
    SPECS.get_or_init(|| {
        let mut v = vec![Spec::Toggle(
            c"normalize",
            c"Normalize",
            false,
            c"Each output's weights and offset scaled to add up to 1.",
        )];
        for (row, (group, label)) in BLEND_GROUPS.into_iter().enumerate() {
            v.push(Spec::Group(group, label));
            for col in 0..5 {
                v.push(Spec::Ranged(
                    BLEND[row * 5 + col],
                    BLEND_LABELS[col],
                    if col == row { 1.0 } else { 0.0 },
                    [-255.0, 255.0],
                    [-10.0, 10.0],
                    if col == 4 {
                        c"Added to the channel."
                    } else {
                        c"How much of that channel goes into this one."
                    },
                ));
            }
            v.push(Spec::GroupEnd);
        }
        v
    })
}

impl Kind {
    pub(crate) const ALL: [Kind; 37] = [
        Kind::TinyPlanet,
        Kind::Defocus,
        Kind::LinearBlur,
        Kind::QuickBlur,
        Kind::RadialBlur,
        Kind::Median,
        Kind::MinMax,
        Kind::Convolution,
        Kind::Spherize,
        Kind::Swirl,
        Kind::Threshold,
        Kind::Wave,
        Kind::PinchPunch,
        Kind::OffsetWrap,
        Kind::BlackRestore,
        Kind::BroadcastColors,
        Kind::ChannelBlend,
        Kind::ChromaBlur,
        Kind::MaskGenerator,
        Kind::RadialPixelate,
        Kind::Deform,
        Kind::LabAdjust,
        Kind::SoftContrast,
        Kind::FillLight,
        Kind::LensFlare,
        Kind::Starburst,
        Kind::Glint,
        Kind::Rays,
        Kind::LightRays,
        Kind::FilmEffects,
        Kind::FilmGrain,
        Kind::Newsprint,
        Kind::TvSimulator,
        Kind::BumpMap,
        Kind::Denoise,
        Kind::Looks,
        Kind::ChromaKeyer,
    ];

    pub(crate) fn id(self) -> &'static CStr {
        match self {
            Kind::TinyPlanet => c"org.bananav.TinyPlanet",
            Kind::Defocus => c"org.bananav.Defocus",
            Kind::LinearBlur => c"org.bananav.LinearBlur",
            Kind::QuickBlur => c"org.bananav.QuickBlur",
            Kind::RadialBlur => c"org.bananav.RadialBlur",
            Kind::Median => c"org.bananav.Median",
            Kind::MinMax => c"org.bananav.MinMax",
            Kind::Convolution => c"org.bananav.ConvolutionKernel",
            Kind::Spherize => c"org.bananav.Spherize",
            Kind::Swirl => c"org.bananav.Swirl",
            Kind::Threshold => c"org.bananav.Threshold",
            Kind::Wave => c"org.bananav.Wave",
            Kind::PinchPunch => c"org.bananav.PinchPunch",
            Kind::OffsetWrap => c"org.bananav.OffsetWrap",
            Kind::BlackRestore => c"org.bananav.BlackRestore",
            Kind::BroadcastColors => c"org.bananav.BroadcastColors",
            Kind::ChannelBlend => c"org.bananav.ChannelBlend",
            Kind::ChromaBlur => c"org.bananav.ChromaBlur",
            Kind::MaskGenerator => c"org.bananav.MaskGenerator",
            Kind::RadialPixelate => c"org.bananav.RadialPixelate",
            Kind::Deform => c"org.bananav.Deform",
            Kind::LabAdjust => c"org.bananav.LabAdjust",
            Kind::SoftContrast => c"org.bananav.SoftContrast",
            Kind::FillLight => c"org.bananav.FillLight",
            Kind::LensFlare => c"org.bananav.LensFlare",
            Kind::Starburst => c"org.bananav.Starburst",
            Kind::Glint => c"org.bananav.Glint",
            Kind::Rays => c"org.bananav.Rays",
            Kind::LightRays => c"org.bananav.LightRays",
            Kind::FilmEffects => c"org.bananav.FilmEffects",
            Kind::FilmGrain => c"org.bananav.FilmGrain",
            Kind::Newsprint => c"org.bananav.Newsprint",
            Kind::TvSimulator => c"org.bananav.TvSimulator",
            Kind::BumpMap => c"org.bananav.BumpMap",
            Kind::Denoise => c"org.bananav.Denoise",
            Kind::Looks => c"org.bananav.Looks",
            Kind::ChromaKeyer => c"org.bananav.ChromaKeyer",
        }
    }

    fn label(self) -> &'static CStr {
        match self {
            Kind::TinyPlanet => c"Tiny Planet",
            Kind::Defocus => c"Defocus",
            Kind::LinearBlur => c"Linear Blur",
            Kind::QuickBlur => c"Quick Blur",
            Kind::RadialBlur => c"Radial Blur",
            Kind::Median => c"Median",
            Kind::MinMax => c"Min and Max",
            Kind::Convolution => c"Convolution Kernel",
            Kind::Spherize => c"Spherize",
            Kind::Swirl => c"Swirl",
            Kind::Threshold => c"Threshold",
            Kind::Wave => c"Wave",
            Kind::PinchPunch => c"Pinch/Punch",
            Kind::OffsetWrap => c"Offset and Wrap",
            Kind::BlackRestore => c"Black Restore",
            Kind::BroadcastColors => c"Broadcast Colors",
            Kind::ChannelBlend => c"Channel Blend",
            Kind::ChromaBlur => c"Chroma Blur",
            Kind::MaskGenerator => c"Mask Generator",
            Kind::RadialPixelate => c"Radial Pixelate",
            Kind::Deform => c"Deform",
            Kind::LabAdjust => c"LAB Adjust",
            Kind::SoftContrast => c"Soft Contrast",
            Kind::FillLight => c"Fill Light",
            Kind::LensFlare => c"Lens Flare",
            Kind::Starburst => c"Starburst",
            Kind::Glint => c"Glint",
            Kind::Rays => c"Rays",
            Kind::LightRays => c"Light Rays",
            Kind::FilmEffects => c"Film Effects",
            Kind::FilmGrain => c"Film Grain",
            Kind::Newsprint => c"Newsprint",
            Kind::TvSimulator => c"TV Simulator",
            Kind::BumpMap => c"Bump Map",
            Kind::Denoise => c"Denoise",
            Kind::Looks => c"Looks",
            Kind::ChromaKeyer => c"Chroma Keyer",
        }
    }

    fn grouping(self) -> &'static CStr {
        match self {
            Kind::Defocus
            | Kind::LinearBlur
            | Kind::QuickBlur
            | Kind::RadialBlur
            | Kind::Median => c"BananaV/Blur",
            Kind::TinyPlanet
            | Kind::MinMax
            | Kind::Convolution
            | Kind::Spherize
            | Kind::Swirl
            | Kind::Threshold
            | Kind::Wave
            | Kind::PinchPunch
            | Kind::OffsetWrap
            | Kind::RadialPixelate
            | Kind::Deform
            | Kind::LensFlare
            | Kind::Starburst
            | Kind::Glint
            | Kind::Rays
            | Kind::LightRays
            | Kind::FilmEffects
            | Kind::FilmGrain
            | Kind::Newsprint
            | Kind::TvSimulator
            | Kind::BumpMap => c"BananaV/Creative",
            Kind::BlackRestore
            | Kind::BroadcastColors
            | Kind::ChannelBlend
            | Kind::ChromaBlur
            | Kind::MaskGenerator
            | Kind::Denoise
            | Kind::ChromaKeyer => c"BananaV/Utility",
            Kind::LabAdjust | Kind::SoftContrast | Kind::FillLight | Kind::Looks => {
                c"BananaV/Color"
            }
        }
    }

    fn description(self) -> &'static CStr {
        match self {
            Kind::TinyPlanet => c"The picture bent into a little planet seen from above: its bottom in the middle, its sides around.",
            Kind::Defocus => c"The picture out of focus, as a lens shows it: bright lights bloom into the iris's shape.",
            Kind::LinearBlur => c"The picture blurred along one direction, as if it moved.",
            Kind::QuickBlur => c"A slight, fast blur.",
            Kind::RadialBlur => c"The picture blurred away from a center, as when zooming fast, or all over.",
            Kind::Median => c"Each pixel the middle value of its surroundings: noise smoothed away, or a painted look.",
            Kind::MinMax => c"The picture in groups of pixels, each group its darkest or brightest pixel.",
            Kind::Convolution => c"Each pixel worked out from its neighbours by a matrix of weights: blurs, embossing, sharpening.",
            Kind::Spherize => c"The picture wrapped around a sphere, bulging out or in.",
            Kind::Swirl => c"The picture twisted, more in the middle than at the edges.",
            Kind::Threshold => c"Each of red, green and blue full where it reaches the threshold, else none.",
            Kind::Wave => c"The picture rippled across, up and down, or both.",
            Kind::PinchPunch => c"The picture squeezed in toward a center, or pushed out of it.",
            Kind::OffsetWrap => c"The picture moved, smaller and repeated or mirrored, wrapping around its edges.",
            Kind::BlackRestore => c"The darkest colors set to black, against noise near black.",
            Kind::BroadcastColors => c"Luminance and chrominance kept within what television shows.",
            Kind::ChannelBlend => c"The red, green, blue and alpha channels mixed into each other.",
            Kind::ChromaBlur => c"The colors blurred, the brightness left sharp: smoother keys and masks.",
            Kind::MaskGenerator => c"Transparency from the alpha, the luminance or a color channel.",
            Kind::RadialPixelate => c"The picture pixelated in rays from a center or rings around it.",
            Kind::Deform => c"The picture squeezed, slanted or bent.",
            Kind::LabAdjust => c"Lightness and colors adjusted apart, in the L*a*b* color space.",
            Kind::SoftContrast => c"Softer, finer contrast, with a vignette around the picture if wanted.",
            Kind::FillLight => c"Light added to the dark parts, as a fill light would.",
            Kind::LensFlare => c"The reflections a camera's lens makes of a bright light.",
            Kind::Starburst => c"Star-shaped streaks from the picture's highlights.",
            Kind::Glint => c"Glinting streaks, colored or not, from the picture's highlights.",
            Kind::Rays => c"Rays of light streaming from the picture's bright parts, away from a light.",
            Kind::LightRays => c"Rays from a light source shining through the picture's bright parts.",
            Kind::FilmEffects => c"The look of old film: grain, a tone, dust, hairs and scratches.",
            Kind::FilmGrain => c"Film's grain, gray or colored.",
            Kind::Newsprint => c"The picture printed in dots, as in a newspaper, a magazine or a photo.",
            Kind::TvSimulator => c"The picture on an old television: its stripes, lines, glow, rolling and snow.",
            Kind::BumpMap => c"Texture and light from the picture's own brightness: light parts high, dark parts low.",
            Kind::Denoise => c"Reduces luminance and color noise.",
            Kind::Looks => c"A film's, a genre's or a style's look, as strong as wanted.",
            Kind::ChromaKeyer => c"A color made transparent, so another picture shows behind (blue or green screens).",
        }
    }

    pub(crate) fn specs(self) -> &'static [Spec] {
        match self {
            Kind::TinyPlanet => TINY_PLANET,
            Kind::Defocus => DEFOCUS,
            Kind::LinearBlur => LINEAR_BLUR,
            Kind::QuickBlur => QUICK_BLUR,
            Kind::RadialBlur => RADIAL_BLUR,
            Kind::Median => MEDIAN,
            Kind::MinMax => MIN_MAX,
            Kind::Convolution => CONVOLUTION,
            Kind::Spherize => SPHERIZE,
            Kind::Swirl => SWIRL,
            Kind::Threshold => THRESHOLD,
            Kind::Wave => WAVE,
            Kind::PinchPunch => PINCH_PUNCH,
            Kind::OffsetWrap => OFFSET_WRAP,
            Kind::BlackRestore => BLACK_RESTORE,
            Kind::BroadcastColors => BROADCAST_COLORS,
            Kind::ChannelBlend => channel_blend_specs(),
            Kind::ChromaBlur => CHROMA_BLUR,
            Kind::MaskGenerator => MASK_GENERATOR,
            Kind::RadialPixelate => RADIAL_PIXELATE,
            Kind::Deform => DEFORM,
            Kind::LabAdjust => crate::looks::LAB_ADJUST,
            Kind::SoftContrast => crate::looks::SOFT_CONTRAST,
            Kind::FillLight => crate::looks::FILL_LIGHT,
            Kind::LensFlare => crate::looks::LENS_FLARE,
            Kind::Starburst => crate::looks::STARBURST,
            Kind::Glint => crate::looks::GLINT,
            Kind::Rays => crate::looks::RAYS,
            Kind::LightRays => crate::looks::LIGHT_RAYS,
            Kind::FilmEffects => crate::looks::FILM_EFFECTS,
            Kind::FilmGrain => crate::looks::FILM_GRAIN,
            Kind::Newsprint => crate::looks::NEWSPRINT,
            Kind::TvSimulator => crate::looks::TV_SIMULATOR,
            Kind::BumpMap => crate::looks::BUMP_MAP,
            Kind::Denoise => crate::looks::DENOISE,
            Kind::Looks => crate::looks::LOOKS,
            Kind::ChromaKeyer => crate::classics::CHROMA_KEYER,
        }
    }

    /// Whether it leaves every picture as it is with `s` (its identity).
    fn changes_nothing(self, s: &Vals) -> bool {
        match self {
            Kind::Denoise => crate::looks::Denoise::of(s).changes_nothing(),
            _ => false,
        }
    }

    /// What the card draws for this kind with settings `s` (the same
    /// math as [`Kind::process`]).
    pub(crate) fn card_plan(self, s: &Vals, frame: [f64; 2], time: f64) -> Option<Plan> {
        let pixel = |kernel, f: Vec<f32>, j: [u32; 4]| Some(Plan::Pixel { kernel, f, j });
        let remap = |kernel, f: Vec<f32>, j: [u32; 4]| Plan::Remap { kernel, f, j };
        match self {
            Kind::Threshold => pixel(
                PixelKernel::Threshold,
                Vec::new(),
                [threshold_step(s), 0, 0, 0],
            ),
            Kind::BlackRestore => pixel(
                PixelKernel::BlackRestore,
                vec![s.f(c"threshold") as f32],
                [0; 4],
            ),
            Kind::ChannelBlend => pixel(
                PixelKernel::ChannelBlend,
                blend_weights(s).map(|v| v as f32).to_vec(),
                [0; 4],
            ),
            Kind::MaskGenerator => pixel(
                PixelKernel::MaskGenerator,
                mask_levels(s).to_vec(),
                [s.i(c"type").clamp(0, 4) as u32, s.b(c"invert") as u32, 0, 0],
            ),
            Kind::BroadcastColors => pixel(
                PixelKernel::BroadcastColors,
                Broadcast::of(s).numbers(),
                [0; 4],
            ),
            Kind::LabAdjust => Some(crate::looks::lab_plan(s)),
            Kind::Glint => Some(Plan::Glint {
                numbers: crate::looks::Glint::of(s),
            }),
            Kind::Denoise => {
                let d = crate::looks::Denoise::of(s);
                if d.changes_nothing() {
                    return Some(Plan::Same);
                }
                let on = |h: f64| {
                    if h > f64::from(f32::EPSILON) {
                        h as f32
                    } else {
                        0.0
                    }
                };
                Some(Plan::Denoise {
                    strengths: [on(d.luma), on(d.chroma)],
                    center: d.center.unwrap_or(0),
                })
            }
            Kind::Rays => {
                let b = crate::looks::Beams::of(s);
                Some(Plan::Rays {
                    f: b.numbers(),
                    j: b.flags(),
                })
            }
            Kind::SoftContrast => Some(crate::looks::soft_contrast_plan(s)),
            Kind::Starburst => Some(Plan::Starburst {
                numbers: crate::looks::starburst_numbers(s),
                canonical: frame,
            }),
            Kind::Defocus => Some(Plan::Defocus {
                numbers: defocus_numbers(s),
                canonical: frame,
            }),
            Kind::Looks => {
                // LUT Filter's kernel with the look's table, trilinearly.
                let gain = s.f(c"gain").clamp(0.0, 1.0) as f32;
                Some(match crate::looks::look_table(s.i(c"look")) {
                    Some(lut) if gain > 0.0 => Plan::Lut {
                        lut: Some(lut),
                        tetrahedral: false,
                        strength: gain,
                    },
                    _ => Plan::Same,
                })
            }
            Kind::Spherize => Some(match sphere(s) {
                Some([amount, h, v]) => {
                    let c = s.point(c"center");
                    remap(
                        RemapKernel::Spherize,
                        [amount, h, v, c[0], c[1]].map(|x| x as f32).to_vec(),
                        [0; 4],
                    )
                }
                None => Plan::Same,
            }),
            Kind::Swirl | Kind::PinchPunch => Some(Plan::Lens {
                swirl: self == Kind::Swirl,
                settings: lens_settings(s),
            }),
            Kind::Wave => Some(Plan::Wave {
                numbers: wave_numbers(s),
            }),
            Kind::OffsetWrap => {
                let [ox, oy] = s.pair(c"offset");
                let [mh, mv] = wrap_modes(s);
                Some(remap(
                    RemapKernel::OffsetWrap,
                    [ox, oy, s.f(c"count").clamp(0.0, 50.0)]
                        .map(|x| x as f32)
                        .to_vec(),
                    [mh as u32, mv as u32, 0, 0],
                ))
            }
            Kind::TinyPlanet => Some(remap(
                RemapKernel::TinyPlanet,
                [
                    (s.f(c"latitude").to_radians() - std::f64::consts::FRAC_PI_2).sin(),
                    (s.f(c"latitude").to_radians() - std::f64::consts::FRAC_PI_2).cos(),
                    s.f(c"longitude").to_radians(),
                    s.f(c"z"),
                ]
                .map(|x| x as f32)
                .to_vec(),
                [0; 4],
            )),
            Kind::Convolution => {
                let (w, alpha_too, offset) = kernel_weights(s);
                let mut f: Vec<f32> = w.iter().flatten().copied().collect();
                f.push(offset);
                Some(remap(
                    RemapKernel::Convolution,
                    f,
                    [alpha_too as u32, 0, 0, 0],
                ))
            }
            Kind::QuickBlur => {
                let amount = s.f(c"blendAmount").clamp(0.0, 1.0) as f32;
                Some(if amount <= 1e-6 {
                    Plan::Same
                } else {
                    remap(RemapKernel::QuickBlur, vec![amount], [0; 4])
                })
            }
            Kind::BumpMap => Some(crate::looks::bump_plan(s, frame)),
            Kind::ChromaKeyer => Some(crate::classics::key_plan(s)),
            Kind::Median | Kind::MinMax => Some(Plan::Ranked {
                range: [s.f(c"horizontalRange"), s.f(c"verticalRange")],
                round: s.b(c"circular"),
                rank: if self == Kind::Median {
                    s.f(c"offset")
                } else {
                    f64::from(s.i(c"operation").clamp(0, 1))
                },
            }),
            Kind::RadialBlur => Some({
                let strength = s.f(c"strength").clamp(0.0, 1.0);
                if strength <= 0.0 {
                    Plan::Same
                } else {
                    Plan::RadialBlur {
                        strength,
                        proportional: s.i(c"type") == 0,
                        center: s.point(c"center"),
                    }
                }
            }),
            Kind::RadialPixelate => Some({
                let strength = s.f(c"strength").clamp(0.0, 1.0);
                if strength <= 1e-6 {
                    Plan::Same
                } else {
                    let c = s.point(c"center");
                    Plan::RadialPixelate {
                        center: [2.0 * c[0] - 1.0, 1.0 - 2.0 * c[1]],
                        strength,
                        radial: s.i(c"type") == 0,
                    }
                }
            }),
            Kind::LinearBlur => Some(Plan::LinearBlur {
                amount: s.f(c"amount"),
                angle: s.f(c"angle"),
            }),
            Kind::ChromaBlur => Some(Plan::ChromaBlur {
                pixels: [s.f(c"horizontalPixels"), s.f(c"verticalPixels")],
            }),
            Kind::FillLight => Some(match crate::looks::FillLight::of(s) {
                Some(f) => Plan::FillLight {
                    table: f.table,
                    reach: f.reach,
                    amount: f.amount,
                },
                None => Plan::Same,
            }),
            Kind::Deform => Some(Plan::Deform {
                settings: deform_settings(s),
            }),
            Kind::FilmGrain => Some(Plan::FilmGrain {
                numbers: [s.f(c"amount"), s.f(c"granularity"), s.f(c"chromaticity")],
                frame: time,
                canonical: frame,
            }),
            Kind::LensFlare => Some(Plan::LensFlare {
                numbers: crate::looks::flare_numbers(s),
                canonical: frame,
            }),
            Kind::Newsprint => Some(Plan::Newsprint {
                dot: s.f(c"dotSize"),
                kind: s.i(c"type"),
                canonical: frame,
            }),
            Kind::TvSimulator => Some(Plan::TvSimulator {
                numbers: crate::looks::tv_numbers(s),
                time,
                canonical: frame,
            }),
            Kind::FilmEffects => Some(Plan::FilmEffects {
                numbers: crate::looks::film_numbers(s),
                time,
            }),
            Kind::LightRays => Some(Plan::LightRays {
                numbers: crate::looks::light_rays_numbers(s),
                time,
            }),
        }
    }

    /// The kind whose identifier is `id`.
    pub(crate) fn of(id: &str) -> Option<Kind> {
        Kind::ALL
            .into_iter()
            .find(|k| k.id().to_str().ok() == Some(id))
    }
}

macro_rules! entry {
    ($($name:ident => $kind:expr;)*) => {
        $(
            /// The plug-in's entry point.
            pub(crate) unsafe extern "C" fn $name(
                action: *const c_char,
                handle: *const c_void,
                in_args: OfxPropertySetHandle,
                out_args: OfxPropertySetHandle,
            ) -> OfxStatus {
                main($kind, action, handle, in_args, out_args)
            }
        )*
    };
}

entry! {
    tiny_planet => Kind::TinyPlanet;
    defocus => Kind::Defocus;
    linear_blur => Kind::LinearBlur;
    quick_blur => Kind::QuickBlur;
    radial_blur => Kind::RadialBlur;
    median => Kind::Median;
    min_max => Kind::MinMax;
    convolution => Kind::Convolution;
    spherize => Kind::Spherize;
    swirl => Kind::Swirl;
    threshold => Kind::Threshold;
    wave => Kind::Wave;
    pinch_punch => Kind::PinchPunch;
    offset_wrap => Kind::OffsetWrap;
    black_restore => Kind::BlackRestore;
    broadcast_colors => Kind::BroadcastColors;
    channel_blend => Kind::ChannelBlend;
    chroma_blur => Kind::ChromaBlur;
    mask_generator => Kind::MaskGenerator;
    radial_pixelate => Kind::RadialPixelate;
    deform => Kind::Deform;
    lab_adjust => Kind::LabAdjust;
    soft_contrast => Kind::SoftContrast;
    fill_light => Kind::FillLight;
    lens_flare => Kind::LensFlare;
    starburst => Kind::Starburst;
    glint => Kind::Glint;
    rays => Kind::Rays;
    light_rays => Kind::LightRays;
    film_effects => Kind::FilmEffects;
    film_grain => Kind::FilmGrain;
    newsprint => Kind::Newsprint;
    tv_simulator => Kind::TvSimulator;
    bump_map => Kind::BumpMap;
    denoise => Kind::Denoise;
    looks => Kind::Looks;
    chroma_keyer => Kind::ChromaKeyer;
}

fn main(
    kind: Kind,
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
        describe(kind, fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        let comps = [kOfxImageComponentRGBA];
        fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
        // Bump Map is a compositor too: in the General context, the
        // picture whose bumps light the source.
        if kind == Kind::BumpMap
            && args.is(kOfxImageEffectPropContext, kOfxImageEffectContextGeneral)
        {
            fx.define_clip(crate::compositors::MAP_CLIP, &comps)
                .set_int(kOfxImageClipPropOptional, 1);
        }
        fx.define_clip(kOfxImageEffectOutputClipName, &comps);
        define_all(fx, kind.specs());
        // Denoise's numbers as VEGAS shows them: two digits, a tenth a step.
        if kind == Kind::Denoise {
            for name in [c"luminance", c"color", c"amount"] {
                let p = fx.param(name).props();
                p.set_dbl(kOfxParamPropIncrement, 0.1);
                p.set_int(kOfxParamPropDigits, 2);
            }
        }
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        render(kind, fx, args)
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // Pixels move and spread: all of the source.
        let (origin, frame) = fx.frame();
        out.set_dbls(
            c"OfxImageClipPropRoI_Source",
            &[
                origin[0],
                origin[1],
                origin[0] + frame[0],
                origin[1] + frame[1],
            ],
        );
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        let time = args.dbl(kOfxPropTime, 0);
        let vals = Vals::of(kind.specs(), &|spec| spec.read(fx, time));
        if kind.changes_nothing(&vals) {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxActionInstanceChanged) {
        show(kind, fx, args.dbl(kOfxPropTime, 0));
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) {
        show(kind, fx, 0.0);
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

/// Greys out what the settings leave unused, as VEGAS does: Light Rays'
/// ellipse while Bound radius is off, Film Effects' particles while they
/// are off.
fn show(kind: Kind, fx: Fx, time: f64) {
    let (switch, names): (&CStr, &[&CStr]) = match kind {
        Kind::LightRays => (c"boundRadius", &[c"radiusX", c"radiusY", c"feather"]),
        Kind::FilmEffects => (c"enable", &[c"type", c"amount"]),
        _ => return,
    };
    let vals = Vals::of(kind.specs(), &|spec| spec.read(fx, time));
    let on = vals.b(switch);
    for &name in names {
        fx.param(name).set_enabled(on);
    }
}

fn describe(kind: Kind, fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, kind.label());
    p.set_str(kOfxImageEffectPluginPropGrouping, kind.grouping());
    p.set_str(kOfxPropPluginDescription, kind.description());
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextFilter, kOfxImageEffectContextGeneral],
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
    // Points counted otherwise than the frame's 0–1: handles of its own.
    if let Some(entry) = overlay_entry(kind) {
        p.set_ptr(kOfxImageEffectPluginPropOverlayInteractV2, entry);
    }
}

// Points on the preview counted otherwise than the frame's 0–1 (a
// position the host shows itself, as VEGAS's standard overlay does).

/// How a kind's point counts the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    /// From the frame's middle, its edges ±1, up positive (Lens Flare's
    /// light).
    Middle,
    /// The picture's middle moved: 1 the frame's width or height, up
    /// positive (Offset and Wrap's offset).
    Shift,
    /// From the middle, 1 half the frame's longer side, up positive (Bump
    /// Map's light, its first two numbers).
    HalfLonger,
}

impl Place {
    /// The point `v` as a part of a `w` × `h` frame (0–1, rows up).
    fn to_frame(self, v: [f64; 2], [w, h]: [f64; 2]) -> [f64; 2] {
        match self {
            Place::Middle => [0.5 + v[0] * 0.5, 0.5 + v[1] * 0.5],
            Place::Shift => [0.5 + v[0], 0.5 + v[1]],
            Place::HalfLonger => {
                let half = w.max(h) * 0.5;
                [0.5 + v[0] * half / w, 0.5 + v[1] * half / h]
            }
        }
    }

    /// The point at `p` of the frame (0–1, rows up) as this place counts it.
    fn of_frame(self, p: [f64; 2], [w, h]: [f64; 2]) -> [f64; 2] {
        match self {
            Place::Middle => [(p[0] - 0.5) * 2.0, (p[1] - 0.5) * 2.0],
            Place::Shift => [p[0] - 0.5, p[1] - 0.5],
            Place::HalfLonger => {
                let half = w.max(h) * 0.5;
                [(p[0] - 0.5) * w / half, (p[1] - 0.5) * h / half]
            }
        }
    }
}

impl Kind {
    /// The points this kind's overlay shows as handles: each parameter,
    /// how it counts the frame, and whether it has a third number (kept).
    fn handles(self) -> &'static [(&'static CStr, Place, bool)] {
        match self {
            Kind::LensFlare => &[(c"lightPosition", Place::Middle, false)],
            Kind::OffsetWrap => &[(c"offset", Place::Shift, false)],
            Kind::BumpMap => &[
                (c"source", Place::HalfLonger, true),
                (c"destination", Place::HalfLonger, false),
            ],
            _ => &[],
        }
    }
}

/// What a kind's overlay remembers: the handle taken (its place in the
/// list, where the pen took it from the point) and the one under the pen.
#[derive(Debug, Default)]
struct Points {
    drag: Option<(usize, [f64; 2])>,
    hover: Option<usize>,
}

macro_rules! overlays {
    ($($name:ident => $kind:path;)*) => {
        $(
            unsafe extern "C" fn $name(
                action: *const c_char,
                handle: *const c_void,
                in_args: OfxPropertySetHandle,
                out_args: OfxPropertySetHandle,
            ) -> OfxStatus {
                overlay($kind, action, handle, in_args, out_args)
            }
        )*

        /// The overlay of a kind with handles of its own.
        fn overlay_entry(kind: Kind) -> Option<*mut c_void> {
            match kind {
                $($kind => Some($name as *mut c_void),)*
                _ => None,
            }
        }
    };
}

overlays! {
    lens_flare_overlay => Kind::LensFlare;
    offset_wrap_overlay => Kind::OffsetWrap;
    bump_map_overlay => Kind::BumpMap;
}

fn overlay(
    kind: Kind,
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if is(action, kOfxActionDescribe) {
        return kOfxStatOK;
    }
    let Some(props) = interact_props(handle) else {
        return kOfxStatReplyDefault;
    };
    if is(action, kOfxActionCreateInstance) {
        let state = Box::into_raw(Box::new(Points::default()));
        props.set_ptr(kOfxPropInstanceData, state as *mut c_void);
        return kOfxStatOK;
    }
    let state = props.ptr(kOfxPropInstanceData) as *mut Points;
    if is(action, kOfxActionDestroyInstance) {
        if !state.is_null() {
            // SAFETY: made above, freed once.
            drop(unsafe { Box::from_raw(state) });
            props.set_ptr(kOfxPropInstanceData, std::ptr::null_mut());
        }
        return kOfxStatOK;
    }
    if state.is_null() {
        return kOfxStatReplyDefault;
    }
    // SAFETY: this overlay's state; hosts run one of its actions at a time.
    let state = unsafe { &mut *state };
    let args = Props(in_args);
    let fx = Fx(args.ptr(kOfxPropEffectInstance) as OfxImageEffectHandle);
    if fx.0.is_null() {
        return kOfxStatReplyDefault;
    }
    let time = args.dbl(kOfxPropTime, 0);
    let ps: [f64; 2] = args.dbls(kOfxInteractPropPixelScale);
    let ps = ps.map(|v| if v.abs() > 1e-12 { v.abs() } else { 1.0 });
    let (origin, size) = fx.frame();
    let handles = kind.handles();
    let points: Vec<[f64; 2]> = handles
        .iter()
        .map(|&(name, place, triple)| {
            let p = fx.param(name);
            let v = if triple {
                let [x, y, _] = p.xyz_at(time);
                [x, y]
            } else {
                p.xy_at(time)
            };
            let n = place.to_frame(v, size);
            [origin[0] + n[0] * size[0], origin[1] + n[1] * size[1]]
        })
        .collect();
    if is(action, kOfxInteractActionDraw) {
        if let Some(d) = Draw::of(args) {
            for (k, &at) in points.iter().enumerate() {
                let hot = state.hover == Some(k) || state.drag.is_some_and(|(i, _)| i == k);
                d.handle(at, ps, hot);
            }
        }
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    let near = points.iter().position(|&at| near_handle(pen, at, ps));
    if is(action, kOfxInteractActionPenDown) {
        let Some(k) = near else {
            return kOfxStatReplyDefault;
        };
        state.drag = Some((k, [pen[0] - points[k][0], pen[1] - points[k][1]]));
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        match state.drag {
            Some((k, grip)) => {
                let (name, place, triple) = handles[k];
                let n = [
                    (pen[0] - grip[0] - origin[0]) / size[0],
                    (pen[1] - grip[1] - origin[1]) / size[1],
                ];
                let v = place.of_frame(n, size);
                let p = fx.param(name);
                if triple {
                    let [_, _, z] = p.xyz_at(time);
                    p.set_xyz([v[0], v[1], z]);
                } else {
                    p.set_xy(v);
                }
                kOfxStatOK
            }
            None => {
                if near != state.hover {
                    state.hover = near;
                    redraw(handle);
                }
                kOfxStatReplyDefault
            }
        }
    } else if is(action, kOfxInteractActionPenUp) {
        if state.drag.take().is_some() {
            redraw(handle);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else {
        kOfxStatReplyDefault
    }
}

fn render(kind: Kind, fx: Fx, args: Props) -> OfxStatus {
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
    let vals = Vals::of(kind.specs(), &|s| s.read(fx, time));
    let source = Buf::read(src.as_ref());
    drop(src);
    // Bump Map as a compositor: the map's bumps light the source.
    let map = (kind == Kind::BumpMap)
        .then(|| fx.clip(crate::compositors::MAP_CLIP).image(time))
        .flatten();
    let result = match map {
        Some(map) if map.depth == out.depth && map.channels == out.channels => {
            let guide = Buf::read(Some(&map));
            drop(map);
            let (source, guide) = if vals.b(c"flip") {
                (&guide, &source)
            } else {
                (&source, &guide)
            };
            crate::looks::bump_map_with(&vals, &grid, source, guide)
        }
        _ => kind.process(&vals, &grid, &source, time),
    };
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let rows = y1.max(out.bounds[1])..y2.min(out.bounds[3]);
    crate::colors::write_out(&out, &result, columns, rows);
    drop(out);
    kOfxStatOK
}

// The pictures.

/// The straight color of a premultiplied one.
pub(crate) fn straight(c: [f32; 4]) -> [f32; 4] {
    if c[3] > 0.0 {
        [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]]
    } else {
        [0.0; 4]
    }
}

/// A straight color premultiplied.
pub(crate) fn premultiplied(c: [f32; 4]) -> [f32; 4] {
    [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
}

/// A box blur `rx` × `ry` pixels each way (edges held), across then
/// down: running sums along the rows, then down bands of them, so that it
/// costs the same whatever the radii.
pub(crate) fn box_blur(src: &Buf, rx: c_int, ry: c_int) -> Buf {
    if src.data.is_empty() || (rx <= 0 && ry <= 0) {
        return src.clone();
    }
    match (rx > 0, ry > 0) {
        (true, true) => blur_columns(&blur_rows(src, rx as usize), ry as usize),
        (true, false) => blur_rows(src, rx as usize),
        _ => blur_columns(src, ry as usize),
    }
}

/// Each row of `src` averaged over `r` pixels each way (edges held), by a
/// running sum.
fn blur_rows(src: &Buf, r: usize) -> Buf {
    let w = src.width().max(0) as usize;
    let mut out = Buf {
        bounds: src.bounds,
        data: vec![[0.0; 4]; src.data.len()],
    };
    if w == 0 {
        return out;
    }
    let n = (2 * r + 1) as f64;
    let y1 = src.bounds[1];
    out.rows(|y, row| {
        let line = &src.data[(y - y1) as usize * w..][..w];
        let at = |i: isize| line[i.clamp(0, w as isize - 1) as usize];
        let mut acc = [0.0f64; 4];
        for k in -(r as isize)..=(r as isize) {
            let c = at(k);
            for i in 0..4 {
                acc[i] += f64::from(c[i]);
            }
        }
        for (x, px) in row.iter_mut().enumerate() {
            *px = acc.map(|v| (v / n) as f32);
            let (add, gone) = (at((x + r + 1) as isize), at(x as isize - r as isize));
            for i in 0..4 {
                acc[i] += f64::from(add[i]) - f64::from(gone[i]);
            }
        }
    });
    out
}

/// Each column of `src` averaged over `r` pixels each way (edges held):
/// a running sum per column down each band of rows.
fn blur_columns(src: &Buf, r: usize) -> Buf {
    let (w, h) = (src.width().max(0) as usize, src.height().max(0) as isize);
    let mut out = Buf {
        bounds: src.bounds,
        data: vec![[0.0; 4]; src.data.len()],
    };
    if w == 0 || h == 0 {
        return out;
    }
    let n = (2 * r + 1) as f64;
    let (y1, r) = (src.bounds[1], r as isize);
    let row = |j: isize| &src.data[j.clamp(0, h - 1) as usize * w..][..w];
    // About two bands a thread.
    let per = (h as usize).div_ceil(32).max(8);
    out.bands(per, |first, band| {
        let top = (first - y1) as isize;
        let mut acc = vec![[0.0f64; 4]; w];
        for k in -r..=r {
            for (a, c) in acc.iter_mut().zip(row(top + k)) {
                for i in 0..4 {
                    a[i] += f64::from(c[i]);
                }
            }
        }
        for (m, line) in band.chunks_mut(w).enumerate() {
            let j = top + m as isize;
            let (add, gone) = (row(j + r + 1), row(j - r));
            for ((px, a), (p, q)) in line.iter_mut().zip(&mut acc).zip(add.iter().zip(gone)) {
                *px = a.map(|v| (v / n) as f32);
                for i in 0..4 {
                    a[i] += f64::from(p[i]) - f64::from(q[i]);
                }
            }
        }
    });
    out
}

/// VEGAS's Linear Blur: the Gaussian (`recursive_gauss`) along the line
/// Angle gives (turning clockwise from across), worked out as VEGAS does:
/// the picture sheared so the line runs along its rows (or columns, nearer
/// upright), blurred along them by Amount × the frame's longer side times
/// how far the line runs along them (and across them by a radius of 0, a
/// little, as VEGAS's), edges held, and sheared back.
fn line_blurred(s: &Vals, g: &Grid, src: &Buf) -> Buf {
    let longer = g.width_px().max(g.height_px());
    let Some(LineShear {
        across,
        slope,
        radius,
        lines: [u1, w1, u2, w2],
    }) = LineShear::of(
        s.f(c"amount"),
        s.f(c"angle"),
        longer,
        g.pixel_shape(),
        src.bounds,
    )
    else {
        return src.clone();
    };
    let mut lines = Buf {
        bounds: [u1, w1, u2, w2],
        data: vec![[0.0; 4]; ((u2 - u1).max(0) * (w2 - w1).max(0)) as usize],
    };
    lines.rows(|w, row| {
        for (i, px) in row.iter_mut().enumerate() {
            let u = f64::from(u1 + i as c_int);
            let v = f64::from(w) + slope * u;
            *px = if across {
                src.bilinear(u, v)
            } else {
                src.bilinear(v, u)
            };
        }
    });
    crate::recursive_gauss::blur(&mut lines, [radius, 0.0]);
    src.map(|x, y| {
        let (u, v) = if across { (x, y) } else { (y, x) };
        lines.bilinear(f64::from(u), f64::from(v) - slope * f64::from(u))
    })
}

/// VEGAS's Radial Blur: from Center outward each pixel becomes `1 − q`
/// of its own color and `q` of the blurred picture a step nearer Center,
/// `q` = Strength^(75 ⁄ the frame's shorter side in pixels): Proportional
/// steps 0.018 × Strength^1.5 × 300 ⁄ that side of the way there, the
/// others Strength² + 1 pixels (straight to Center within that). As
/// VEGAS: each quarter around Center in turn — below on the left, below
/// on the right, above on the left, above on the right — rows and then
/// pixels outward, on the picture as it goes; the pixel over Center goes
/// back to its own color as the last quarter starts.
fn radially_blurred(s: &Vals, g: &Grid, src: &Buf) -> Buf {
    let strength = s.f(c"strength").clamp(0.0, 1.0);
    let (w, h) = (src.width().max(0) as isize, src.height().max(0) as isize);
    if strength <= 0.0 || w == 0 || h == 0 {
        return src.clone();
    }
    let shorter = (g.frame[0] * g.scale[0] / g.par).min(g.height_px());
    let [part, q, fixed] = radial_steps(strength, shorter);
    let q = q as f32;
    let proportional = s.i(c"type") == 0;
    let c = s.point(c"center");
    let (fx, fy) = g.xy(c[0], c[1]);
    let cx = ((fx + 0.5).floor() as isize - src.bounds[0] as isize).clamp(0, w - 1);
    let cy = ((fy + 0.5).floor() as isize - src.bounds[1] as isize).clamp(0, h - 1);
    let mut out = src.data.clone();
    let at = |out: &[[f32; 4]], x: isize, y: isize| {
        out[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize]
    };
    let lefts: Vec<isize> = (0..=cx).rev().collect();
    let rights: Vec<isize> = (cx + 1..w).collect();
    let belows: Vec<isize> = (0..=cy).rev().collect();
    let aboves: Vec<isize> = (cy + 1..h).collect();
    let quarters = [
        (&belows, &lefts),
        (&belows, &rights),
        (&aboves, &lefts),
        (&aboves, &rights),
    ];
    for (n, (rows, columns)) in quarters.into_iter().enumerate() {
        if rows.is_empty() || columns.is_empty() {
            continue;
        }
        if n == 3 {
            let i = ((cy + 1) * w + cx) as usize;
            out[i] = src.data[i];
        }
        for &y in rows {
            for &x in columns {
                let (dx, dy) = ((cx - x) as f64, (cy - y) as f64);
                let (px, py) = if proportional {
                    (x as f64 + dx * part, y as f64 + dy * part)
                } else {
                    let d = dx.hypot(dy);
                    if d > fixed {
                        (x as f64 + dx * fixed / d, y as f64 + dy * fixed / d)
                    } else {
                        (cx as f64, cy as f64)
                    }
                };
                let (ix, iy) = (px.floor() as isize, py.floor() as isize);
                let (tx, ty) = ((px - ix as f64) as f32, (py - iy as f64) as f32);
                let corners = [
                    at(&out, ix, iy),
                    at(&out, ix + 1, iy),
                    at(&out, ix, iy + 1),
                    at(&out, ix + 1, iy + 1),
                ];
                let i = (y * w + x) as usize;
                let own = src.data[i];
                out[i] = std::array::from_fn(|k| {
                    let low = corners[0][k] + (corners[1][k] - corners[0][k]) * tx;
                    let high = corners[2][k] + (corners[3][k] - corners[2][k]) * tx;
                    (1.0 - q) * own[k] + q * (low + (high - low) * ty)
                });
            }
        }
    }
    Buf {
        bounds: src.bounds,
        data: out,
    }
}

/// The picture moved: each pixel sampled where `from` (the frame's 0–1
/// coordinates) says, nothing where it says `None`.
fn remap(src: &Buf, g: &Grid, from: impl Fn(f64, f64) -> Option<(f64, f64)> + Sync) -> Buf {
    src.map(|x, y| {
        let (u, v) = g.uv(x, y);
        match from(u, v) {
            Some((su, sv)) => src.sample(g, su, sv),
            None => [0.0; 4],
        }
    })
}

/// VEGAS's Tiny Planet: the picture, taken as the whole sphere
/// (equirectangular), seen in a stereographic projection about the point
/// Latitude and Longitude give (Latitude 0 the bottom pole), the frame's
/// width `Z` times what reaches the projection's unit; the middle pixel
/// takes the bottom row, as VEGAS's does.
fn planet(s: &Vals, src: &Buf) -> Buf {
    use std::f64::consts::{FRAC_PI_2, PI, TAU};
    let (w, h) = (src.width().max(0), src.height().max(0));
    if w == 0 || h == 0 {
        return src.clone();
    }
    let lat = s.f(c"latitude").to_radians() - FRAC_PI_2;
    let lon = s.f(c"longitude").to_radians();
    let unit = 2.0 * f64::from(w) / s.f(c"z");
    let (sin0, cos0) = lat.sin_cos();
    let [x1, y1, ..] = src.bounds;
    src.map(|x, y| {
        // Rows go up here.
        let (dx, dy) = (f64::from(x - x1 - w / 2), f64::from(y - y1 - h / 2));
        let r = dx.hypot(dy);
        // The angle from the middle, 2 atan(r ⁄ unit), by its sine and
        // cosine.
        let t = r / unit;
        let (sin_c, cos_c) = (2.0 * t / (1.0 + t * t), (1.0 - t * t) / (1.0 + t * t));
        let (row, along) = if r > 0.0 {
            // The point on the sphere — up, east and north of the
            // projection's middle —: its latitude from all three stays
            // exact near the poles.
            let up = sin_c * dy * cos0 / r + cos_c * sin0;
            let east = sin_c * dx / r;
            let north = (cos0 * r * cos_c - sin0 * dy * sin_c) / r;
            let phi = up.atan2(east.hypot(north));
            let lambda = (sin_c * dx).atan2(cos0 * r * cos_c - sin0 * dy * sin_c);
            (f64::from(h - 1) * (phi + FRAC_PI_2) / PI, lambda)
        } else {
            (0.0, 0.0)
        };
        let mut along = along + lon + PI;
        if along < 0.0 {
            along += TAU;
        } else if along > TAU {
            along -= TAU;
        }
        let column = f64::from(w - 1) * along / TAU;
        src.bilinear(
            f64::from(x1) + column.clamp(0.0, f64::from(w - 1)),
            f64::from(y1) + row.clamp(0.0, f64::from(h - 1)),
        )
    })
}

/// VEGAS's Radial Pixelate: the picture laid out by angle (across) and
/// distance from Center (up, to twice the frame's half diagonal), in the
/// frame's own proportions, each pixel the nearest; averaged in blocks —
/// Radial `(1 − Strength) × width ⁄ 2 + 1` of them across, Circular as
/// many up —; and laid back, each pixel again the nearest. Center's 0 to
/// 1 is the frame's.
fn radially_pixelated(s: &Vals, src: &Buf) -> Buf {
    let strength = s.f(c"strength").clamp(0.0, 1.0);
    let (w, h) = (src.width().max(0) as usize, src.height().max(0) as usize);
    if strength <= 1e-6 || w == 0 || h == 0 {
        return src.clone();
    }
    let (wf, hf) = (w as f64, h as f64);
    let c = s.point(c"center");
    // The middle in -1 to 1 of the frame, rows going up.
    let (cx, cy) = (2.0 * c[0] - 1.0, 1.0 - 2.0 * c[1]);
    const REACH: f64 = 2.828_427;
    let nearest = |v: f64, n: usize| (v.max(0.0).floor() as usize).min(n - 1);
    // The picture by angle and distance.
    let polar: Vec<[f32; 4]> = (0..h * w)
        .map(|k| {
            let (i, j) = ((k % w) as f64, (k / w) as f64);
            let a = (2.0 * i / wf - 1.0) * std::f64::consts::PI;
            let r = j / hf * REACH;
            let x = ((a.sin() * r + cx) * 0.5 + 0.5) * wf;
            let y = ((a.cos() * r + cy) * 0.5 + 0.5) * hf;
            src.data[nearest(y, h) * w + nearest(x, w)]
        })
        .collect();
    // Its blocks' means.
    let [bx, by] = pixelate_blocks(strength, s.i(c"type") == 0, [w, h]);
    let mut means = vec![[0.0f32; 4]; bx * by];
    for (b, mean) in means.iter_mut().enumerate() {
        let (i, j) = (b % bx, b / bx);
        let (x0, x1) = (i * w / bx, ((i + 1) * w / bx).max(i * w / bx + 1));
        let (y0, y1) = (j * h / by, ((j + 1) * h / by).max(j * h / by + 1));
        let mut sum = [0.0f64; 4];
        for y in y0..y1.min(h) {
            for px in &polar[y * w + x0..y * w + x1.min(w)] {
                for k in 0..4 {
                    sum[k] += f64::from(px[k]);
                }
            }
        }
        let n = ((x1.min(w) - x0) * (y1.min(h) - y0)).max(1) as f64;
        *mean = sum.map(|v| (v / n) as f32);
    }
    let [x1, y1, ..] = src.bounds;
    src.map(|x, y| {
        let (i, j) = (f64::from(x - x1), f64::from(y - y1));
        let (dx, dy) = (2.0 * i / wf - 1.0 - cx, 2.0 * j / hf - 1.0 - cy);
        let r = dx.hypot(dy) / REACH * hf;
        let a = (dx.atan2(dy) / std::f64::consts::PI * 0.5 + 0.5) * wf;
        let (pi, pj) = (nearest(a, w), nearest(r, h));
        means[(pj * by / h) * bx + pi * bx / w]
    })
}

/// VEGAS's Spherize: within the ellipse Scaling makes of the frame (its
/// width and height at 1) about Center — whose 0 to 1 reaches half a frame
/// past each edge —, each pixel takes the picture `1 − 0.8 × Amount ×
/// cos(r × 90°)` of its way from the middle, `r` its place out to the
/// ellipse (0 to 1): above 0 the middle grows, below 0 it shrinks.
fn spherized(s: &Vals, g: &Grid, src: &Buf) -> Buf {
    let Some([amount, h, v]) = sphere(s) else {
        return src.clone();
    };
    let (w, ht) = (g.frame[0] * g.scale[0] / g.par, g.height_px());
    // The frame's first pixel, and the middle as VEGAS rounds it (rows go
    // up here).
    let (fx, fy) = (g.origin[0] * g.scale[0] / g.par, g.origin[1] * g.scale[1]);
    let c = s.point(c"center");
    let xc = fx + ((2.0 * c[0] - 0.5) * w).trunc();
    let yc = fy + ((2.0 * (1.0 - c[1]) - 0.5) * ht).trunc();
    let (kx, ky) = (2.0 / (h + 1e-4) / w, 2.0 / (v + 1e-4) / ht);
    src.map(|x, y| {
        let (dx, dy) = (f64::from(x) - xc, f64::from(y) - yc);
        let r2 = (dx * kx).powi(2) + (dy * ky).powi(2);
        if r2 >= 1.0 {
            return src.at(x, y);
        }
        let f = 1.0 - (r2.sqrt() * std::f64::consts::FRAC_PI_2).cos() * amount * 0.8;
        src.bilinear(xc + dx * f, yc + dy * f)
    })
}

/// Radial Blur's steps on a frame whose shorter side is `shorter`
/// pixels: how far toward Center each pixel reads (Proportional's part of
/// its distance), how much of what it reads it takes, and Fixed's reach.
pub(crate) fn radial_steps(strength: f64, shorter: f64) -> [f64; 3] {
    let shorter = shorter.max(1.0);
    [
        strength.powf(1.5) * 0.018 / (shorter / 300.0),
        strength.powf(75.0 / shorter),
        strength * strength + 1.0,
    ]
}

/// Radial Pixelate's blocks across and up the picture laid out by angle
/// and distance (`w` × `h`): few angles (`radial`) or few distances.
pub(crate) fn pixelate_blocks(strength: f64, radial: bool, [w, h]: [usize; 2]) -> [usize; 2] {
    let few = |n: usize| ((n as f64 * (1.0 - strength) * 0.5 + 1.0) as usize).clamp(1, n);
    if radial { [few(w), h] } else { [w, few(h)] }
}

/// Linear Blur's shear: whether its line goes across (else up), the
/// line's slope (pixels across the lines per pixel along them), the
/// Gaussian's radius along it (pixels) and the sheared picture's window
/// (`u` along the lines, `w` across them); `None` when it blurs nothing.
pub(crate) struct LineShear {
    pub across: bool,
    pub slope: f64,
    pub radius: f64,
    pub lines: [c_int; 4],
}

impl LineShear {
    /// Amount and Angle (degrees) over a picture of window `bounds`
    /// whose frame's longer side is `longer` pixels, its pixels showing
    /// `shape` times as wide as tall (the render scale the same both ways,
    /// as BananaV's).
    pub(crate) fn of(
        amount: f64,
        angle: f64,
        longer: f64,
        shape: f64,
        bounds: [c_int; 4],
    ) -> Option<LineShear> {
        use std::f64::consts::{FRAC_PI_4, PI};
        let amount = amount.clamp(0.0, 1.0);
        if amount <= 1e-6 {
            return None;
        }
        // VEGAS's: the angle within a half turn, and mirrored (rows going
        // up) to pick the rows below 45° and from 135°, the columns
        // between.
        let half = |d: f64| if d >= 180.0 { d - 180.0 } else { d };
        let turn = half(angle).to_radians();
        let mut mirrored = half(360.0 - angle).to_radians();
        if mirrored.cos().abs() > 1e-6 {
            mirrored = mirrored.sin().atan2(mirrored.cos());
            if mirrored < 0.0 {
                mirrored += 2.0 * PI;
            }
        }
        let reach = amount * longer;
        let across = !(FRAC_PI_4..3.0 * FRAC_PI_4).contains(&mirrored);
        let (slope, radius) = if across {
            (mirrored.tan() * shape, reach * turn.cos().abs())
        } else {
            (
                1.0 / (mirrored.tan() * shape),
                reach * turn.sin().abs() * shape,
            )
        };
        // Along the rows (`u`) or the columns; each line of the sheared
        // picture holds the pixels at `w + slope × u` across them.
        let [x1, y1, x2, y2] = bounds;
        let (u1, u2, v1, v2) = if across {
            (x1, x2, y1, y2)
        } else {
            (y1, y2, x1, x2)
        };
        let ends = [slope * f64::from(u1), slope * f64::from(u2 - 1)];
        let w1 = (f64::from(v1) - ends[0].max(ends[1])).floor() as c_int;
        let w2 = (f64::from(v2 - 1) - ends[0].min(ends[1])).ceil() as c_int + 1;
        Some(LineShear {
            across,
            slope,
            radius,
            lines: [u1, w1, u2, w2],
        })
    }
}

/// Spherize's Amount and its area's width and height (parts of the
/// frame's); `None` when it changes nothing.
fn sphere(s: &Vals) -> Option<[f64; 3]> {
    let amount = s.f(c"amount").clamp(-1.0, 1.0);
    let h = s.f(c"horizontal").clamp(0.0, 1.0);
    let v = if s.b(c"proportional") {
        h
    } else {
        s.f(c"vertical").clamp(0.0, 1.0)
    };
    (amount.abs() > 1e-6 && (h > 1e-6 || v > 1e-6)).then_some([amount, h, v])
}

/// Swirl's and Pinch/Punch's area as VEGAS lays it on a picture `w` × `h`,
/// its rows counted from the top: about the center pixel (Center times the
/// frame, cut toward 0), Horizontal times the frame's half width wide and
/// Vertical (Horizontal when proportional) times its half height tall, a
/// hair more.
pub(crate) struct Lens {
    /// The center's column and row (from the top).
    pub(crate) center: [i32; 2],
    /// The area's half width and half height, in pixels.
    pub(crate) reach: [f64; 2],
    /// A column's offset over the half width per pixel, and the half
    /// height's reciprocal over the frame's.
    per: [f64; 2],
    height: f64,
}

impl Lens {
    pub(crate) fn of(settings: &[f64; 5], w: u32, h: u32) -> Lens {
        let [cx, cy, sx, sy, _] = *settings;
        let (wf, hf) = (f64::from(w), f64::from(h));
        let (kx, ky) = (1.0 / (sx.max(0.0) + 0.0001), 1.0 / (sy.max(0.0) + 0.0001));
        Lens {
            // Center as VEGAS keeps it: from the middle, down.
            center: [
                (((cx - 0.5) + 0.5) * wf) as i32,
                (((0.5 - cy) + 0.5) * hf) as i32,
            ],
            reach: [wf / kx * 0.5, hf / ky * 0.5],
            per: [(kx + kx) / wf, ky],
            height: hf,
        }
    }

    /// Column `x`'s offset from the center over the half width.
    pub(crate) fn across(&self, x: i32) -> f64 {
        f64::from(x - self.center[0]) * self.per[0]
    }

    /// Row `t`'s (from the top) offset from the center over the half
    /// height.
    pub(crate) fn down(&self, t: i32) -> f64 {
        let d = f64::from(t - self.center[1]);
        (d + d) / self.height * self.per[1]
    }

    /// Each column's offset and its square, then each row's, in single
    /// precision: the card's table, and what tells the pixels in the area
    /// (the squares' sum under 1) alike on the processor and the card.
    pub(crate) fn table(&self, w: u32, h: u32) -> Vec<[f32; 4]> {
        let entry = |o: f64| [o as f32, (o * o) as f32, 0.0, 0.0];
        (0..w as i32)
            .map(|x| entry(self.across(x)))
            .chain((0..h as i32).map(|t| entry(self.down(t))))
            .collect()
    }
}

/// Channel Blend's weights, a row of five (red, green, blue, alpha and
/// the offset) for each output; Normalize divides a row by its sum when
/// it is positive.
fn blend_weights(s: &Vals) -> [f64; 20] {
    let mut w: [f64; 20] = std::array::from_fn(|i| s.f(BLEND[i]));
    if s.b(c"normalize") {
        for row in w.chunks_mut(5) {
            let sum: f64 = row.iter().sum();
            if sum > 0.0 {
                row.iter_mut().for_each(|v| *v /= sum);
            }
        }
    }
    w
}

/// Mask Generator's levels: Low and High in, Low and High out, and the
/// scale from the first two to 0–1.
fn mask_levels(s: &Vals) -> [f32; 5] {
    let (li, hi) = (s.f(c"lowIn") as f32, s.f(c"highIn") as f32);
    let (lo, ho) = (s.f(c"lowOut") as f32, s.f(c"highOut") as f32);
    let scale = if hi - li > 1e-7 { 1.0 / (hi - li) } else { 1.0 };
    [li, hi, lo, ho, scale]
}

/// Offset and Wrap's lists (Repeat, Mirror, Off) as [`wrap`]'s modes,
/// across and up.
fn wrap_modes(s: &Vals) -> [i32; 2] {
    let mode = |name: &CStr| match s.i(name) {
        0 => 2,
        1 => 1,
        _ => 0,
    };
    [mode(c"horizontalWrap"), mode(c"verticalWrap")]
}

/// Mirrored or repeated past 0–1 (`mode`: 0 nothing, 1 mirror, 2 repeat).
fn wrap(t: f64, mode: i32) -> Option<f64> {
    match mode {
        1 => {
            let m = t.rem_euclid(2.0);
            Some(if m > 1.0 { 2.0 - m } else { m })
        }
        2 => Some(t.rem_euclid(1.0)),
        _ => (0.0..=1.0).contains(&t).then_some(t),
    }
}

impl Kind {
    /// The whole picture through the effect at settings `s`, at `time`
    /// (frames).
    pub(crate) fn process(self, s: &Vals, g: &Grid, src: &Buf, time: f64) -> Buf {
        if src.data.is_empty() {
            return src.clone();
        }
        match self {
            Kind::TinyPlanet => planet(s, src),
            Kind::Defocus => defocused(s, g, src),
            Kind::LinearBlur => line_blurred(s, g, src),
            // VEGAS's: the picture blurred, mixed in by Blend amount.
            Kind::QuickBlur => {
                let amount = s.f(c"blendAmount").clamp(0.0, 1.0) as f32;
                if amount <= 1e-6 {
                    return src.clone();
                }
                let blurred = box_blur(src, 1, 1);
                src.map(|x, y| {
                    let (a, b) = (src.at(x, y), blurred.at(x, y));
                    std::array::from_fn(|k| a[k] + (b[k] - a[k]) * amount)
                })
            }
            Kind::RadialBlur => radially_blurred(s, g, src),
            // VEGAS's Min and Max is its Median at either end.
            Kind::Median | Kind::MinMax => {
                let rank = if self == Kind::Median {
                    s.f(c"offset")
                } else {
                    f64::from(s.i(c"operation").clamp(0, 1))
                };
                ranked(
                    src,
                    g,
                    [s.f(c"horizontalRange"), s.f(c"verticalRange")],
                    s.b(c"circular"),
                    rank,
                )
            }
            Kind::Convolution => convolved(s, src),
            Kind::Spherize => spherized(s, g, src),
            Kind::Swirl => lensed(true, s, src),
            Kind::PinchPunch => lensed(false, s, src),
            // VEGAS's: each of red, green and blue in 8-bit steps, full
            // (the alpha) from the threshold's step up, else none.
            Kind::Threshold => {
                let t = threshold_step(s);
                src.map(|x, y| {
                    let c = src.at(x, y);
                    let k = |v: f32| if crate::kit::step8(v) >= t { c[3] } else { 0.0 };
                    [k(c[0]), k(c[1]), k(c[2]), c[3]]
                })
            }
            Kind::Wave => {
                let (w, h) = (src.width(), src.height());
                if w < 2 || h < 2 {
                    return src.clone();
                }
                let (rows, columns) = wave_offsets(&wave_numbers(s), w as u32, h as u32);
                let [x1, y1, ..] = src.bounds;
                src.map(|x, y| {
                    // Rows counted from the top, as VEGAS counts them.
                    let (x, t) = (x - x1, h - 1 - (y - y1));
                    let sx = (x * 16_384 + rows[t as usize]).clamp(0, (w - 2) * 16_384);
                    let st = (t * 16_384 + columns[x as usize]).clamp(0, (h - 2) * 16_384);
                    let at = |i: c_int, j: c_int| src.data[((h - 1 - j) * w + i) as usize];
                    let (ix, it) = (sx >> 14, st >> 14);
                    let (fx, ft) = (
                        (sx & 16_383) as f32 / 16_384.0,
                        (st & 16_383) as f32 / 16_384.0,
                    );
                    let (a, b, c, d) = (
                        at(ix, it),
                        at(ix + 1, it),
                        at(ix, it + 1),
                        at(ix + 1, it + 1),
                    );
                    std::array::from_fn(|k| {
                        a[k] * ((1.0 - fx) * (1.0 - ft))
                            + b[k] * (fx * (1.0 - ft))
                            + c[k] * ((1.0 - fx) * ft)
                            + d[k] * (fx * ft)
                    })
                })
            }
            // VEGAS's: the picture scaled down Count times about the
            // frame's middle and moved by Offset (frames; up above 0), its
            // edges repeated, mirrored or left.
            Kind::OffsetWrap => {
                let [ox, oy] = s.pair(c"offset");
                let count = s.f(c"count").clamp(0.0, 50.0);
                let [mh, mv] = wrap_modes(s);
                remap(src, g, |u, v| {
                    let su = 0.5 + (u - 0.5) * count - ox;
                    let sv = 0.5 + (v - 0.5) * count + oy;
                    Some((wrap(su, mh)?, wrap(sv, mv)?))
                })
            }
            // VEGAS's: a pixel stays when any of its red, green and blue
            // is past the threshold; else they go to black, its alpha kept.
            Kind::BlackRestore => {
                let t = s.f(c"threshold") as f32;
                src.map(|x, y| {
                    let c = src.at(x, y);
                    if c[0] > t || c[1] > t || c[2] > t {
                        c
                    } else {
                        [0.0, 0.0, 0.0, c[3]]
                    }
                })
            }
            Kind::BroadcastColors => broadcast(s, src),
            // VEGAS's: each output the pixel's values as they are
            // (premultiplied) weighted, plus its offset — Normalize divides
            // a row, weights and offset, by their sum when it is positive —,
            // the alpha within 0–1 and the colors within 0 and it.
            Kind::ChannelBlend => {
                let w = blend_weights(s);
                src.map(|x, y| {
                    let c = src.at(x, y).map(f64::from);
                    let out = |k: usize| {
                        let r = &w[k * 5..k * 5 + 5];
                        r[0] * c[0] + r[1] * c[1] + r[2] * c[2] + r[3] * c[3] + r[4]
                    };
                    let a = out(3).clamp(0.0, 1.0);
                    [
                        out(0).clamp(0.0, a) as f32,
                        out(1).clamp(0.0, a) as f32,
                        out(2).clamp(0.0, a) as f32,
                        a as f32,
                    ]
                })
            }
            // VEGAS's: the picture's U and V (BT.601's) blurred by three
            // boxes standing for a Gaussian of the whole pixels plus 1.5
            // (so even 0 blurs a little), its Y and alpha as they are,
            // the colors back within 0–1.
            Kind::ChromaBlur => {
                let w = src.width() as usize;
                let sigma = |v: f64, scale: f64| ((v + 1.5).floor() * scale) as f32;
                let (sx, sy) = (
                    sigma(s.f(c"horizontalPixels"), g.scale[0]),
                    sigma(s.f(c"verticalPixels"), g.scale[1]),
                );
                let yuv: Vec<[f32; 3]> = src
                    .data
                    .iter()
                    .map(|c| {
                        [
                            c[0] * 0.299 + c[1] * 0.587 + c[2] * 0.114,
                            c[0] * -0.14713 - c[1] * 0.28886 + c[2] * 0.436,
                            c[0] * 0.615 - c[1] * 0.51499 - c[2] * 0.10001,
                        ]
                    })
                    .collect();
                let mut u: Vec<f32> = yuv.iter().map(|p| p[1]).collect();
                let mut v: Vec<f32> = yuv.iter().map(|p| p[2]).collect();
                gauss_plane(&mut u, w, sx, sy);
                gauss_plane(&mut v, w, sx, sy);
                let mut out = src.clone();
                for (i, c) in out.data.iter_mut().enumerate() {
                    let (y, u, v) = (yuv[i][0], u[i], v[i]);
                    *c = [
                        (y + v * 1.13983).clamp(0.0, 1.0),
                        (y - u * 0.39465 - v * 0.5806).clamp(0.0, 1.0),
                        (y + u * 2.03211).clamp(0.0, 1.0),
                        c[3],
                    ];
                }
                out
            }
            // VEGAS's: the mask from the pixel's values as they are — the
            // plain average of red, green and blue, or one channel —,
            // turned around with Invert, taken from Low–High in to 0–1
            // (past them 0 or 1), then to Low–High out, in every channel.
            Kind::MaskGenerator => {
                let ty = s.i(c"type");
                let invert = s.b(c"invert");
                let [li, hi, lo, ho, scale] = mask_levels(s);
                src.map(|x, y| {
                    let c = src.at(x, y);
                    let mut v = match ty {
                        1 => c[0],
                        2 => c[1],
                        3 => c[2],
                        4 => c[3],
                        _ => (c[0] + c[1] + c[2]) * (1.0 / 3.0),
                    };
                    if invert {
                        v = 1.0 - v;
                    }
                    let t = if v <= li {
                        0.0
                    } else if v < hi {
                        (v - li) * scale
                    } else {
                        1.0
                    };
                    [t * (ho - lo) + lo; 4]
                })
            }
            Kind::RadialPixelate => radially_pixelated(s, src),
            Kind::Deform => deformed(s, src),
            Kind::LabAdjust
            | Kind::SoftContrast
            | Kind::FillLight
            | Kind::LensFlare
            | Kind::Starburst
            | Kind::Glint
            | Kind::Rays
            | Kind::LightRays
            | Kind::FilmEffects
            | Kind::FilmGrain
            | Kind::Newsprint
            | Kind::TvSimulator
            | Kind::BumpMap
            | Kind::Denoise
            | Kind::Looks => crate::looks::process(self, s, g, src, time),
            Kind::ChromaKeyer => crate::classics::process(self, s, g, src),
        }
    }
}

/// One pixel's weight in Median and Min and Max's areas (VEGAS's fixed
/// point).
const WHOLE: i64 = 1 << 14;

/// VEGAS's Median and Min and Max: each channel of each pixel becomes the
/// value `rank` of the way through its area's values (0 the least, 1 the
/// most): the mean of the values over one whole pixel's weight from
/// there, the area's pixels weighed by [`area_weights`] and only those
/// inside the picture counted. `range` is the settings, 1 reaching a
/// twentieth of the frame's longer side each way. VEGAS works these out
/// in 8 bits (the effects take nothing else) and drops the mean's
/// fraction.
/// Median's and Min and Max's area for the card: each weighed pixel
/// around the first of a row (`dx`, `dy` — rows up — and its weight),
/// then each change of weight as the area moves a pixel right (`dy`,
/// `dx`, the change), in `kit_median`'s table; `range` as the settings
/// say, over a frame whose longer side is `longer` pixels.
pub(crate) fn ranked_table(range: [f64; 2], round: bool, longer: f64) -> (Vec<[f32; 4]>, u32) {
    let rx = range[0].clamp(0.0, 1.0) * 0.05 * longer;
    let ry = range[1].clamp(0.0, 1.0) * 0.05 * longer;
    let quarter = area_weights(rx, ry, round);
    let (kh, kw) = (quarter.len() as isize - 1, quarter[0].len() as isize - 1);
    let weight = |dx: isize, dy: isize| {
        quarter
            .get(dy.unsigned_abs())
            .and_then(|r| r.get(dx.unsigned_abs()))
            .copied()
            .unwrap_or(0)
    };
    let mut table = Vec::new();
    for dy in -kh..=kh {
        for dx in -kw..=kw {
            let wgt = weight(dx, dy);
            if wgt != 0 {
                table.push([dx as f32, dy as f32, wgt as f32, 0.0]);
            }
        }
    }
    let fill = table.len() as u32;
    for dy in -kh..=kh {
        for d in -kw..=kw + 1 {
            let delta = weight(d - 1, dy) - weight(d, dy);
            if delta != 0 {
                table.push([dy as f32, d as f32, delta as f32, 0.0]);
            }
        }
    }
    (table, fill)
}

fn ranked(src: &Buf, g: &Grid, range: [f64; 2], round: bool, rank: f64) -> Buf {
    let (w, h) = (src.width().max(0) as usize, src.height().max(0) as usize);
    if w == 0 || h == 0 {
        return src.clone();
    }
    let longer = (g.frame[0] * g.scale[0] / g.par).max(g.height_px());
    let rx = range[0].clamp(0.0, 1.0) * 0.05 * longer / g.par;
    let ry = range[1].clamp(0.0, 1.0) * 0.05 * longer;
    let quarter = area_weights(rx, ry, round);
    let (kh, kw) = (quarter.len() as isize - 1, quarter[0].len() as isize - 1);
    let weight = |dx: isize, dy: isize| {
        quarter
            .get(dy.unsigned_abs())
            .and_then(|r| r.get(dx.unsigned_abs()))
            .copied()
            .unwrap_or(0)
    };
    // As the area moves a pixel right, the pixel `d` from its middle
    // weighs weight(d - 1) instead of weight(d): each row's changes.
    let steps: Vec<Vec<(isize, i64)>> = (-kh..=kh)
        .map(|dy| {
            (-kw..=kw + 1)
                .filter_map(|d| {
                    let delta = weight(d - 1, dy) - weight(d, dy);
                    (delta != 0).then_some((d, delta))
                })
                .collect()
        })
        .collect();
    let levels: Vec<[u8; 4]> = src
        .data
        .iter()
        .map(|c| c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
        .collect();
    let rank = (rank.clamp(0.0, 1.0) * WHOLE as f64) as i64;
    let y1 = src.bounds[1];
    let (wi, hi) = (w as isize, h as isize);
    let mut out = Buf {
        bounds: src.bounds,
        data: vec![[0.0; 4]; src.data.len()],
    };
    out.rows(|y, row| {
        let j = (y - y1) as isize;
        // Each channel's weights by 8-bit value, and their sum.
        let mut hist = [[0i64; 256]; 4];
        let mut total = 0i64;
        let weigh =
            |hist: &mut [[i64; 256]; 4], total: &mut i64, x: isize, line: isize, wgt: i64| {
                if (0..wi).contains(&x) && (0..hi).contains(&line) {
                    let px = levels[line as usize * w + x as usize];
                    for (k, v) in px.iter().enumerate() {
                        hist[k][usize::from(*v)] += wgt;
                    }
                    *total += wgt;
                }
            };
        for dy in -kh..=kh {
            for dx in -kw..=kw {
                let wgt = weight(dx, dy);
                if wgt != 0 {
                    weigh(&mut hist, &mut total, dx, j + dy, wgt);
                }
            }
        }
        for (i, px) in row.iter_mut().enumerate() {
            let lo = (total - WHOLE) * rank / WHOLE;
            *px = std::array::from_fn(|k| f32::from(value_at(&hist[k], lo, lo + WHOLE)) / 255.0);
            if i + 1 < w {
                for (dy, changes) in (-kh..=kh).zip(&steps) {
                    for &(d, delta) in changes {
                        weigh(&mut hist, &mut total, i as isize + d, j + dy, delta);
                    }
                }
            }
        }
    });
    out
}

/// Median and Min and Max's weights about the middle pixel, a quarter of
/// them (the rest mirror it): `w[j][i]` for the pixel `i` across and `j`
/// down. VEGAS's rectangle takes the pixels within the ranges whole and
/// the next ones by the ranges' fractions; its round area is an ellipse
/// half a pixel wider each way, each pixel weighed by how much of it the
/// ellipse covers, the middle one whole.
fn area_weights(rx: f64, ry: f64, round: bool) -> Vec<Vec<i64>> {
    let fixed = |v: f64| (v * WHOLE as f64) as i64;
    if !round {
        let (ix, iy) = (rx as usize, ry as usize);
        let part = |n: usize, whole: usize, r: f64| if n <= whole { 1.0 } else { r.fract() };
        return (0..=iy + 1)
            .map(|j| {
                (0..=ix + 1)
                    .map(|i| fixed(part(i, ix, rx) * part(j, iy, ry)))
                    .collect()
            })
            .collect();
    }
    let (a, b) = (rx + 0.5, ry + 0.5);
    (0..=(b + 1.0) as usize)
        .map(|j| {
            (0..=(a + 1.0) as usize)
                .map(|i| {
                    if i == 0 && j == 0 {
                        WHOLE
                    } else {
                        fixed(covered(a, b, i, j))
                    }
                })
                .collect()
        })
        .collect()
}

/// How much of the pixel `i` across and `j` down from (0, 0) the ellipse
/// of half-axes `a` and `b` about (0, 0) covers.
fn covered(a: f64, b: f64, i: usize, j: usize) -> f64 {
    let inside = |x: f64, y: f64| (x / a).powi(2) + (y / b).powi(2) <= 1.0;
    let (x0, y0) = (i as f64 - 0.5, j as f64 - 0.5);
    if !inside(x0.max(0.0), y0.max(0.0)) {
        return 0.0;
    }
    if inside(x0 + 1.0, y0 + 1.0) {
        return 1.0;
    }
    // The ellipse's height within the pixel, summed across it.
    const STEPS: usize = 64;
    let height = (0..STEPS)
        .map(|k| {
            let x = x0 + (k as f64 + 0.5) / STEPS as f64;
            let half = b * (1.0 - (x / a).powi(2)).max(0.0).sqrt();
            (half.min(y0 + 1.0) - (-half).max(y0)).max(0.0)
        })
        .sum::<f64>();
    height / STEPS as f64
}

/// The mean of a channel's values from `lo` to `hi` of the way through
/// its weights (`hist`: the weights by 8-bit value), its fraction dropped.
fn value_at(hist: &[i64; 256], lo: i64, hi: i64) -> u8 {
    let (mut before, mut sum) = (0i64, 0i64);
    for (v, &n) in hist.iter().enumerate() {
        if n <= 0 {
            continue;
        }
        let after = before + n;
        let overlap = after.min(hi) - before.max(lo);
        if overlap > 0 {
            sum += overlap * v as i64;
        }
        if after >= hi {
            break;
        }
        before = after;
    }
    (sum / WHOLE).clamp(0, 255) as u8
}

/// VEGAS's Convolution Kernel: each pixel the sum of its 3 × 3
/// neighbors' channels by the matrix's weights (Row 1 above it, Column 1
/// on its left; the picture's edges held), plus Offset, times the alpha.
/// Rotation turns the eight outer weights round the middle one, a place
/// each 45° and mixed in between; the weights are scaled by Scale, or to
/// add up to 1 with Auto normalize (by 1 when they add up to nothing).
/// When they add up to 1 or more the alpha is summed too, within 0–1,
/// else the pixel keeps its own.
fn convolved(s: &Vals, src: &Buf) -> Buf {
    let (w, alpha_too, offset) = kernel_weights(s);
    src.map(|x, y| {
        let mut acc = [0.0f32; 4];
        for (r, row) in w.iter().enumerate() {
            for (c, &weight) in row.iter().enumerate() {
                // Rows go up here: Row 1 is the one above.
                let px = src.at(x + c as c_int - 1, y + 1 - r as c_int);
                for (a, v) in acc.iter_mut().zip(px) {
                    *a += weight * v;
                }
            }
        }
        let alpha = if alpha_too {
            acc[3].clamp(0.0, 1.0)
        } else {
            src.at(x, y)[3]
        };
        [
            alpha * (acc[0] + offset),
            alpha * (acc[1] + offset),
            alpha * (acc[2] + offset),
            alpha,
        ]
    })
}

/// Convolution Kernel's weights (Row 1 above the pixel) turned and
/// scaled, whether the alpha is summed too (the weights reach 1) and the
/// offset.
fn kernel_weights(s: &Vals) -> ([[f32; 3]; 3], bool, f32) {
    let names = [
        [c"r1c1", c"r1c2", c"r1c3"],
        [c"r2c1", c"r2c2", c"r2c3"],
        [c"r3c1", c"r3c2", c"r3c3"],
    ];
    let mut k = names.map(|row| row.map(|n| s.f(n)));
    let turn = s.f(c"rotation").rem_euclid(360.0);
    if turn != 0.0 {
        // The ring from the bottom left, along the bottom, up the right,
        // back along the top and down the left.
        const RING: [(usize, usize); 8] = [
            (2, 0),
            (2, 1),
            (2, 2),
            (1, 2),
            (0, 2),
            (0, 1),
            (0, 0),
            (1, 0),
        ];
        let old = RING.map(|(r, c)| k[r][c]);
        let places = (turn / 45.0).floor();
        let t = turn / 45.0 - places;
        let n = places as usize;
        for (i, &(r, c)) in RING.iter().enumerate() {
            k[r][c] = (1.0 - t) * old[(i + n) % 8] + t * old[(i + n + 1) % 8];
        }
    }
    let sum: f64 = k.iter().flatten().sum();
    let scale = if !s.b(c"autoNormalize") {
        s.f(c"scale")
    } else if sum.abs() > 1e-4 {
        (1.0 / sum).clamp(-100.0, 100.0)
    } else {
        1.0
    };
    let w = k.map(|row| row.map(|v| (v * scale) as f32));
    let alpha_too = w.iter().flatten().sum::<f32>() >= 1.0 - 1e-6;
    (w, alpha_too, s.f(c"offset") as f32)
}

/// Defocus's settings in the order [`Defocus::of`] takes them: Radius,
/// Iris shape, Orientation, Disc curvature, Bloom threshold and strength,
/// Reduce flicker.
pub(crate) fn defocus_numbers(s: &Vals) -> [f64; 7] {
    [
        s.f(c"radius"),
        f64::from(s.i(c"irisShape")),
        s.f(c"orientation"),
        s.f(c"discCurvature"),
        s.f(c"bloomThreshold"),
        s.f(c"bloomStrength"),
        f64::from(u8::from(s.b(c"reduceFlicker"))),
    ]
}

/// VEGAS's Defocus on a picture: its iris's radius in pixels — Radius
/// percent of the longer side, the width times the pixels' shape —, its
/// shape, turn (degrees) and curvature (−1 to 1), the bloom's threshold
/// (0–1 of the colors) and strength, whether the picture is smoothed first
/// and the pixels' shape.
pub(crate) struct Defocus {
    radius: f64,
    shape: i32,
    orientation: f64,
    curvature: f64,
    threshold: f64,
    strength: f64,
    pub(crate) smooth: bool,
    par: f64,
}

impl Defocus {
    /// Settings `v` (`defocus_numbers`) on a picture `wpx` × `hpx` pixels
    /// `par` wide.
    pub(crate) fn of(v: [f64; 7], wpx: f64, hpx: f64, par: f64) -> Defocus {
        let [
            radius,
            shape,
            orientation,
            curvature,
            threshold,
            strength,
            smooth,
        ] = v;
        let par = if par > 0.0 { par } else { 1.0 };
        Defocus {
            radius: (wpx * par).max(hpx) * radius.clamp(0.0, 10.0) * 0.01,
            shape: (shape.round() as i32).clamp(0, 4),
            orientation,
            curvature: curvature.clamp(-100.0, 100.0) * 0.01,
            threshold: threshold.clamp(50.0, 100.0) * 0.01,
            strength: strength.clamp(1.0, 50.0),
            smooth: smooth != 0.0,
            par,
        }
    }

    /// The bloom: the colors past its threshold brightened by its slope —
    /// (Bloom strength less the threshold) over what is past it — for each
    /// unit past it; none from 254 of 255 up.
    pub(crate) fn bloom(&self) -> Option<[f32; 2]> {
        let t = f64::from(self.threshold as f32);
        (self.threshold < 254.0 / 255.0)
            .then(|| [t as f32, ((self.strength - t) / (1.0 - t)) as f32])
    }

    /// The iris as VEGAS draws it (none under 1.1 pixels: the picture is
    /// left as it is): a disc or a regular polygon — Iris shape's 5 to 8
    /// corners, the first a side's turn short of the top, turned by
    /// Orientation, its height and width by the pixels' shape — of
    /// `0.95 × radius` whole pixels; its light from the middle (`t` 0) to
    /// the rim (1) VEGAS's gradient of 255 grays, `200 (d (1 − t³) + t³)`
    /// with `d` 1 + 0.25 × Disc curvature (0.4 below 0) — below 0 the rim
    /// brighter, VEGAS's "Visible Outer Ring" —; the weights summing to 1,
    /// rows up. Its side and the weights.
    pub(crate) fn iris(&self) -> Option<(usize, Vec<f32>)> {
        use std::f64::consts::{FRAC_PI_2, TAU};
        if self.radius < 1.1 {
            return None;
        }
        let r = (self.radius * 0.95).trunc();
        let reach = r as usize;
        let size = 2 * reach + 1;
        let d = 1.0 + if self.curvature >= 0.0 { 0.25 } else { 0.4 } * self.curvature;
        let stop = |i: usize| {
            let t = i as f64 / 254.0;
            ((d * (1.0 - t * t * t) + t * t * t) * 200.0)
                .trunc()
                .clamp(0.0, 255.0)
        };
        let gray = |t: f64| {
            let x = t.clamp(0.0, 1.0) * 254.0;
            let i = (x.floor() as usize).min(253);
            (stop(i) + (stop(i + 1) - stop(i)) * (x - i as f64)).round()
        };
        // The polygon's edges: each one's outward normal and how far out.
        let sides = if self.shape == 0 {
            0
        } else {
            (self.shape + 4).min(8) as usize
        };
        let (sx, sy) = if self.par >= 1.0 {
            (1.0, 1.0 / self.par)
        } else {
            (self.par, 1.0)
        };
        let turn = TAU / sides.max(1) as f64;
        let corners: Vec<[f64; 2]> = (0..sides)
            .map(|k| {
                let a = (self.orientation.to_radians() + FRAC_PI_2 - turn + k as f64 * turn)
                    .rem_euclid(TAU);
                [a.cos() * r * sx, a.sin() * r * sy]
            })
            .collect();
        let edges: Vec<([f64; 2], f64)> = (0..sides)
            .map(|k| {
                let (a, b) = (corners[k], corners[(k + 1) % sides]);
                let n = [b[1] - a[1], a[0] - b[0]];
                let len = n[0].hypot(n[1]).max(1e-12);
                let n = [n[0] / len, n[1] / len];
                (n, n[0] * a[0] + n[1] * a[1])
            })
            .collect();
        let mut weights = vec![0.0f32; size * size];
        let mut sum = 0.0f64;
        for (j, row) in weights.chunks_mut(size).enumerate() {
            for (i, w) in row.iter_mut().enumerate() {
                let p = [i as f64 - r, j as f64 - r];
                // How far out the pixel is: 0 in the middle, 1 on the rim.
                let out = if sides == 0 {
                    if r > 0.0 { p[0].hypot(p[1]) / r } else { 0.0 }
                } else {
                    edges
                        .iter()
                        .map(|(n, far)| (n[0] * p[0] + n[1] * p[1]) / far.max(1e-12))
                        .fold(0.0, f64::max)
                };
                if out > 1.0 {
                    continue;
                }
                let v = gray(out) / 255.0;
                *w = v as f32;
                sum += v;
            }
        }
        if sum > 1e-7 {
            for w in &mut weights {
                *w = (f64::from(*w) / sum) as f32;
            }
        }
        Some((size, weights))
    }
}

/// `src` smoothed by the binomial 3 × 3 (1 2 1 each way, over 16), its
/// edges held: VEGAS's Reduce flicker.
pub(crate) fn binomial(src: &Buf) -> Buf {
    let [x0, y0, x1, y1] = src.bounds;
    let at = |x: c_int, y: c_int| src.at(x.clamp(x0, x1 - 1), y.clamp(y0, y1 - 1));
    src.map(|x, y| {
        let mut acc = [0.0f32; 4];
        for (dy, wy) in [(-1, 1.0f32), (0, 2.0), (1, 1.0)] {
            for (dx, wx) in [(-1, 1.0f32), (0, 2.0), (1, 1.0)] {
                let c = at(x + dx, y + dy);
                for (a, v) in acc.iter_mut().zip(c) {
                    *a += v * wx * wy;
                }
            }
        }
        acc.map(|v| v / 16.0)
    })
}

/// VEGAS's Defocus: the picture — smoothed first with Reduce flicker, its
/// colors past Bloom threshold brightened (`Defocus::bloom`) — convolved
/// with the iris through the frequency domain (`fft`), its edges held, the
/// result held within 0–1.
fn defocused(s: &Vals, g: &Grid, src: &Buf) -> Buf {
    let d = Defocus::of(
        defocus_numbers(s),
        g.frame[0] * g.scale[0] / g.par,
        g.height_px(),
        g.par,
    );
    let smoothed;
    let input = if d.smooth {
        smoothed = binomial(src);
        &smoothed
    } else {
        src
    };
    let Some((size, kernel)) = d.iris() else {
        return input.clone();
    };
    let bloom = d.bloom();
    crate::fft::convolve(
        input,
        &kernel,
        size,
        &|c| match bloom {
            Some([t, k]) => {
                let up = |v: f32| if v > t { v + (v - t) * k } else { v };
                [up(c[0]), up(c[1]), up(c[2]), c[3]]
            }
            None => c,
        },
        &|_, _, c| c.map(|v| v.clamp(0.0, 1.0)),
    )
}

/// A cubic knee from `start` to `end` that lands on `limit`: its value at
/// `x` past `end`'s side counted from `end`, flat at `end` (VEGAS's
/// Broadcast Colors' soft limits).
fn knee(start: f64, end: f64, limit: f64, x: f64) -> f64 {
    let (mut c3, mut c2) = (0.0, 0.0);
    let d = start - end;
    if d != 0.0 {
        let a = start - limit;
        c3 = (a - d * 0.5) / (d * d * d * -0.5);
        c2 = (a - d / 3.0) / (d * d / 3.0);
    }
    let t = x - end;
    t * t * t * c3 + t * t * c2 + limit
}

/// Where a limit's knee begins and ends: `limit` approached from `room`
/// (the farthest a signal reaches) over `smooth` of the way, bounded by
/// `inner` (no knee past it) and `outer` (the scale's end); a knee less
/// than a third of its length on `limit`'s near side is made one.
fn knee_span(limit: f64, room: f64, smooth: f64, inner: f64, outer: f64) -> (f64, f64) {
    let up = room > limit;
    let d = (room - limit).abs();
    if d <= 2.0 || smooth <= 0.0 {
        return (limit, limit);
    }
    let reach = (limit - inner).abs();
    let f = if reach < d { reach / d } else { 1.0 };
    let near = if up {
        limit - f * d * smooth
    } else {
        limit + f * d * smooth
    };
    if near == limit {
        return (limit, limit);
    }
    let (near, far) = if up {
        (near.max(inner), (limit + d).min(outer))
    } else {
        (near.min(inner), (limit - d).max(outer))
    };
    let far = if (limit - near).abs() <= (far - near).abs() / 3.0 {
        near + (limit - near) * 3.0
    } else {
        far
    };
    (near, far)
}

/// VEGAS's Broadcast Colors: the picture measured in IRE — black at 7.5
/// IRE with its setup, studio RGB's 16 to 235 as asked —, its luma kept
/// within Min and Max, its chroma's peak to peak within Chroma's Max and
/// the luma plus or minus the chroma within Composite's Min and Max, each
/// limit reached through a cubic knee as wide as its Smoothness says.
fn broadcast(s: &Vals, src: &Buf) -> Buf {
    let b = Broadcast::of(s);
    src.map(|x, y| b.pixel(src.at(x, y)))
}

/// Broadcast Colors' limits worked out once (the card's kernel takes the
/// same numbers).
struct Broadcast {
    /// The setup's IRE, black's code value and the IRE of a code value.
    setup: f64,
    black: f64,
    k: f64,
    ymin: f64,
    ymax: f64,
    /// The luma's knees: each one's inner end (where it starts to bend)
    /// and outer one.
    lo: (f64, f64),
    hi: (f64, f64),
    cmax: f64,
    /// The chroma's knee, peak to peak.
    chroma: (f64, f64),
    /// The composite's knees near Min and near Max.
    low: (f64, f64),
    high: (f64, f64),
    kmin: f64,
    kmax: f64,
}

impl Broadcast {
    fn of(s: &Vals) -> Broadcast {
        let setup = if s.b(c"setup") { 7.5 } else { 0.0 };
        let (black, white) = if s.b(c"studioRgb") {
            (16.0, 235.0)
        } else {
            (0.0, 255.0)
        };
        let k = (100.0 - setup) / (white - black);
        let ire = |code: f64| (code - black) * k + setup;
        // Luma: its knees toward Min from black and toward Max from white.
        let (ymin, ymax) = (s.f(c"lumaMin"), s.f(c"lumaMax"));
        let mid = (ymin + ymax) / 2.0;
        let lo = knee_span(ymin, ire(0.0), s.f(c"lumaMinSmoothness"), mid, -8.0);
        let hi = knee_span(ymax, ire(255.0), s.f(c"lumaMaxSmoothness"), mid, 110.0);
        // Chroma, peak to peak: its knee toward Max from the most any
        // color has (the RGB cube's corners: √25999.2 of U and V).
        let cmax = s.f(c"chromaMax");
        let most = k * 2.0 * 25_999.205_85_f64.sqrt();
        let chroma = if cmax >= 1.0 {
            knee_span(cmax, most, s.f(c"chromaSmoothness"), 1.0, 148.0)
        } else {
            (cmax, cmax)
        };
        // The composite's knees, from 100 % of the room left to 120 %.
        let composite_span = |smooth: f64, rounded: bool| {
            if smooth <= 0.0 {
                return (100.0, 100.0);
            }
            let steps = smooth * 20.0;
            let start = 100.0
                - if rounded {
                    (steps + 1.5e-8).round()
                } else {
                    steps
                };
            if start == 100.0 {
                return (100.0, 100.0);
            }
            let start = start.max(1.0);
            let end = if 100.0 - start <= (120.0 - start) / 3.0 {
                (100.0 - start) * 3.0 + start
            } else {
                120.0
            };
            (start, end)
        };
        Broadcast {
            setup,
            black,
            k,
            ymin,
            ymax,
            lo,
            hi,
            cmax,
            chroma,
            low: composite_span(s.f(c"compositeMinSmoothness"), true),
            high: composite_span(s.f(c"compositeMaxSmoothness"), false),
            kmin: s.f(c"compositeMin"),
            kmax: s.f(c"compositeMax"),
        }
    }

    /// IRE of a code value, and back.
    fn ire(&self, code: f64) -> f64 {
        (code - self.black) * self.k + self.setup
    }

    fn code(&self, y: f64) -> f64 {
        (y - self.setup) / self.k + self.black
    }

    /// The luma in IRE, and whether it changed.
    fn luma(&self, y: f64) -> (f64, bool) {
        let ((lo_in, lo_out), (hi_in, hi_out)) = (self.lo, self.hi);
        if y < lo_out {
            (self.ymin, true)
        } else if y < lo_in {
            (knee(lo_in, lo_out, self.ymin, y), true)
        } else if y < hi_in {
            (y, false)
        } else if y < hi_out {
            (knee(hi_in, hi_out, self.ymax, y), true)
        } else {
            (self.ymax, true)
        }
    }

    fn chroma_scale(&self, pp: f64) -> f64 {
        let (start, end) = self.chroma;
        if pp < start {
            1.0
        } else if pp >= end {
            self.cmax / pp
        } else {
            knee(start, end, self.cmax, pp) / pp
        }
    }

    fn composite_scale((start, end): (f64, f64), x: f64) -> f64 {
        if x < start {
            1.0
        } else if x >= end {
            100.0 / x
        } else {
            knee(start, end, 100.0, x) / x
        }
    }

    fn pixel(&self, c: [f32; 4]) -> [f32; 4] {
        let (k, c_start) = (self.k, self.chroma.0);
        let a = c[3].min(1.0) as f64;
        if a <= 1e-6 {
            return [0.0; 4];
        }
        // As VEGAS's: the luma of the values as they are, the chroma of
        // the straight colors.
        let [r, g, b] = [c[0], c[1], c[2]].map(|v| v as f64 * 255.0);
        let (yl, changed) =
            self.luma(self.ire(r) * 0.299 + self.ire(g) * 0.587 + self.ire(b) * 0.114);
        let s_ = |v: f64| if a < 1.0 { v / a } else { v };
        let (rs, gs, bs) = (s_(r), s_(g), s_(b));
        let small = |v: f64| if v.abs() < 1e-6 { 0.0 } else { v };
        let mut u = small(-0.147 * rs - 0.289 * gs + 0.436 * bs);
        let mut v = small(0.615 * rs - 0.515 * gs - 0.100 * bs);
        let rough = (u.abs() + v.abs()) * k;
        let (up, down) = (self.kmax - yl, yl - self.kmin);
        let kept = [c[0], c[1], c[2], a as f32];
        if !changed && up + 1e-6 > rough && down + 1e-6 > rough && c_start * 0.5 + 1e-6 > rough {
            return kept;
        }
        let near_low = down < up;
        let (room, start) = if near_low {
            (down, self.low.0)
        } else {
            (up, self.high.0)
        };
        let ratio = start / 100.0;
        let mut sq = (u * u + v * v) * k * k;
        let mut moved = changed;
        if sq > (c_start * 0.5) * (c_start * 0.5) {
            let f = self.chroma_scale(2.0 * sq.sqrt());
            moved = true;
            u *= f;
            v *= f;
            sq *= f * f;
        }
        if sq <= room * room * ratio * ratio {
            if !moved {
                return kept;
            }
        } else {
            let mut r = room / sq.sqrt();
            if r > 1.0 / 1.2 {
                let span = if near_low { self.low } else { self.high };
                r = Broadcast::composite_scale(span, 100.0 / r);
            }
            u *= r;
            v *= r;
        }
        let yn = self.code(yl) / 255.0;
        let (u, v) = (u / 255.0, v / 255.0);
        [
            ((v * 1.14 + yn) * a) as f32,
            ((yn - u * 0.394 - v * 0.581) * a) as f32,
            ((u * 2.032 + yn) * a) as f32,
            a as f32,
        ]
    }

    /// Its numbers as the card's `kit_broadcast_colors` takes them.
    fn numbers(&self) -> Vec<f32> {
        [
            self.setup,
            self.black,
            self.k,
            self.ymin,
            self.ymax,
            self.lo.0,
            self.lo.1,
            self.hi.0,
            self.hi.1,
            self.cmax,
            self.chroma.0,
            self.chroma.1,
            self.low.0,
            self.low.1,
            self.high.0,
            self.high.1,
            self.kmin,
            self.kmax,
        ]
        .map(|v| v as f32)
        .to_vec()
    }
}

/// How far before the picture's first column or row a point still counts
/// as in it (Deform's): the processor and the card place points a hair
/// apart, the picture's own edges exactly.
const EDGE: f64 = 1e-3;

/// Deform's settings: Amount, Center image (1 on), the background
/// (premultiplied), then the shears across and up, the squeezes of the
/// top, bottom, left and right edges and their curves in that order.
pub(crate) fn deform_settings(s: &Vals) -> [f64; 16] {
    let mut v = [0.0; 16];
    v[0] = s.f(c"amount");
    v[1] = f64::from(u8::from(s.b(c"centerImage")));
    for (k, c) in premultiplied(s.rgba(c"background")).into_iter().enumerate() {
        v[2 + k] = f64::from(c);
    }
    let names = [
        c"shearHorizontal",
        c"shearVertical",
        c"squeezeTop",
        c"squeezeBottom",
        c"squeezeLeft",
        c"squeezeRight",
        c"curveTop",
        c"curveBottom",
        c"curveLeft",
        c"curveRight",
    ];
    for (k, n) in names.into_iter().enumerate() {
        v[6 + k] = s.f(n);
    }
    v
}

/// Deform's shape as VEGAS lays it on a picture `w` × `h`, rows counted
/// from the top, about the picture's middle: the top's and bottom's curves
/// bend the rows (each edge by Amount × its curve × half the height at the
/// middle, nothing at the corners), the left's and right's the columns,
/// the left's and right's squeezes scale the height from one side to the
/// other, the top's and bottom's the width, then the shears slant it — and
/// back, as VEGAS undoes them (its shears undone from the slanted place).
pub(crate) struct Warp {
    size: [f64; 2],
    half: [f64; 2],
    /// Across per row, up per column.
    shear: Option<[f64; 2]>,
    /// The top's half width and its step per row.
    across: Option<[f64; 2]>,
    /// The left's half height and its step per column.
    up: Option<[f64; 2]>,
    /// The top's and bottom's bends at the middle.
    rows: Option<[f64; 2]>,
    /// The left's and right's.
    columns: Option<[f64; 2]>,
    /// The place the deformed picture's box starts at, before the shape
    /// (the box moved by Center image).
    pub(crate) origin: [f64; 2],
    /// The deformed picture's box in the frame: columns and rows from the
    /// top, first in, last out.
    pub(crate) bounds: [i32; 4],
}

impl Warp {
    pub(crate) fn of(settings: &[f64; 16], w: u32, h: u32) -> Warp {
        let a = settings[0];
        let p = &settings[6..16];
        let (wf, hf) = (f64::from(w), f64::from(h));
        let (hw, hh) = (wf * 0.5, hf * 0.5);
        let on = |c: bool| a != 0.0 && c;
        let (kx, ky) = (wf * p[0] * a, hf * p[1] * a);
        let (st, sb) = ((1.0 - a * p[2]) * hw, hw * (1.0 - a * p[3]));
        let (sl, sr) = ((1.0 - a * p[4]) * hh, hh * (1.0 - a * p[5]));
        let mut warp = Warp {
            size: [wf, hf],
            half: [hw, hh],
            shear: on(kx != 0.0 || ky != 0.0).then_some([kx / (hf - 1.0), ky / (wf - 1.0)]),
            across: on(p[2] != 0.0 || p[3] != 0.0).then_some([st, (sb - st) / hf]),
            up: on(p[5] != 0.0 || p[4] != 0.0).then_some([sl, (sr - sl) / wf]),
            rows: on(p[6] != 0.0 || p[7] != 0.0).then_some([a * p[6] * hh, a * p[7] * hh]),
            columns: on(p[8] != 0.0 || p[9] != 0.0).then_some([-p[8] * a * hw, -p[9] * a * hw]),
            origin: [0.0; 2],
            bounds: [0; 4],
        };
        // The box: the corners', the edges' middles' and the middle's
        // places, a pixel more each way.
        let middle = warp.forward([hw, hh]);
        let (mut lo, mut hi) = (middle, middle);
        let edges = [
            [0.0, 0.0],
            [0.0, hf],
            [wf, hf],
            [wf, 0.0],
            [0.0, hh],
            [wf, hh],
            [hw, 0.0],
            [hw, hf],
        ];
        for q in edges.map(|e| warp.forward(e)) {
            lo = [lo[0].min(q[0]), lo[1].min(q[1])];
            hi = [hi[0].max(q[0]), hi[1].max(q[1])];
        }
        let first = |v: f64| (v - 0.499_999_985).round_ties_even() as i32 - 1;
        let last = |v: f64| (v + 0.499_999_985).round_ties_even() as i32 + 1;
        let mut bounds = [first(lo[0]), first(lo[1]), last(hi[0]), last(hi[1])];
        let mut origin = [f64::from(bounds[0]), f64::from(bounds[1])];
        // Center image: the middle's place moved to the frame's middle,
        // the box by whole pixels, the rest of the way in the origin.
        if settings[1] != 0.0 {
            for (k, size) in [w, h].into_iter().enumerate() {
                let by = f64::from(size) * 0.5 - middle[k];
                let whole = by as i32;
                bounds[k] += whole;
                bounds[k + 2] += whole;
                origin[k] += f64::from(whole) - by;
            }
        }
        // Within the frame.
        for (k, size) in [w as i32, h as i32].into_iter().enumerate() {
            if bounds[k] < 0 {
                origin[k] += f64::from(-bounds[k]);
                bounds[k] = 0;
            }
            bounds[k + 2] = bounds[k + 2].min(size);
        }
        warp.origin = origin;
        warp.bounds = bounds;
        warp
    }

    /// A place of the picture deformed.
    fn forward(&self, [mut x, mut y]: [f64; 2]) -> [f64; 2] {
        let ([w, h], [hw, hh]) = (self.size, self.half);
        if let Some([top, bottom]) = self.rows {
            let a = (x - hw) * (x - hw) / (hw * hw);
            let t = a * top - top;
            y = (((a * bottom - bottom) + h) - t) * (y / h) + t;
        }
        if let Some([left, right]) = self.columns {
            let b = (y - hh) * (y - hh) / (hh * hh);
            let l = b * left - left;
            x = (((b * right - right) + w) - l) * (x / w) + l;
        }
        if let Some([sl, step]) = self.up {
            y = ((x * step + sl) * (y - hh)) / hh + hh;
        }
        if let Some([st, step]) = self.across {
            x = ((y * step + st) * (x - hw)) / hw + hw;
        }
        if let Some([kx, ky]) = self.shear {
            x += y * kx;
            y += x * ky;
        }
        [x, y]
    }

    /// The picture's place a deformed place shows, or none (an edge or
    /// a squeeze to nothing).
    pub(crate) fn back(&self, [mut x, mut y]: [f64; 2]) -> Option<[f64; 2]> {
        let ([w, h], [hw, hh]) = (self.size, self.half);
        if let Some([kx, ky]) = self.shear {
            let t = x * ky;
            x -= y * kx;
            y -= t;
        }
        if let Some([st, step]) = self.across {
            let d = y * step + st;
            if d == 0.0 {
                return None;
            }
            x = ((x - hw) * hw) / d + hw;
        }
        if let Some([sl, step]) = self.up {
            let d = x * step + sl;
            if d == 0.0 {
                return None;
            }
            y = ((y - hh) * hh) / d + hh;
        }
        if let Some([left, right]) = self.columns {
            let b = (y - hh) * (y - hh) / (hh * hh);
            let (l, r) = (b * left - left, (b * right - right) + w);
            if l == r {
                return None;
            }
            x = ((x - l) / (r - l)) * w;
        }
        if let Some([top, bottom]) = self.rows {
            let a = (x - hw) * (x - hw) / (hw * hw);
            let (t, b) = (a * top - top, (a * bottom - bottom) + h);
            if t == b {
                return None;
            }
            y = ((y - t) / (b - t)) * h;
        }
        Some([x, y])
    }

    /// The card's numbers (`kit_deform`): the box's origin, the middle,
    /// the shears, the squeezes across and up, the rows' and columns'
    /// curves; which of them apply (bits 1, 2, 4, 8, 16); the box.
    pub(crate) fn numbers(&self) -> ([f32; 14], u32, [i32; 4]) {
        let two = |v: Option<[f64; 2]>| v.unwrap_or_default();
        let parts = [
            self.origin,
            self.half,
            two(self.shear),
            two(self.across),
            two(self.up),
            two(self.rows),
            two(self.columns),
        ];
        let mut f = [0.0; 14];
        for (k, v) in parts.iter().flatten().enumerate() {
            f[k] = *v as f32;
        }
        let bits = [self.shear, self.across, self.up, self.rows, self.columns]
            .iter()
            .enumerate()
            .fold(0, |b, (k, v)| b | (u32::from(v.is_some()) << k));
        (f, bits, self.bounds)
    }
}

/// VEGAS's Deform (`Warp`), rows counted from the top: in the deformed
/// picture's box each pixel shows the picture where the shape takes it
/// back, read between its four nearest pixels (the middle of the three
/// readings VEGAS picks by render quality), the background past the
/// picture's edges; out of the box, and where the picture no longer
/// reaches, the background.
fn deformed(s: &Vals, src: &Buf) -> Buf {
    let (w, h) = (src.width(), src.height());
    if w < 1 || h < 1 {
        return src.clone();
    }
    let settings = deform_settings(s);
    let warp = Warp::of(&settings, w as u32, h as u32);
    let bg = [settings[2], settings[3], settings[4], settings[5]].map(|v| v as f32);
    let (wf, hf) = (f64::from(w), f64::from(h));
    let [x1, y1, ..] = src.bounds;
    let [bx0, by0, bx1, by1] = warp.bounds;
    // The pixel at column x and row t from the top; the background past
    // the picture.
    let texel = |x: c_int, t: c_int| {
        if x < 0 || t < 0 || x >= w || t >= h {
            bg
        } else {
            src.data[((h - 1 - t) * w + x) as usize]
        }
    };
    src.map(|x, y| {
        let (x, t) = (x - x1, h - 1 - (y - y1));
        if x < bx0 || t < by0 || x >= bx1 || t >= by1 {
            return bg;
        }
        let place = [
            f64::from(x - bx0) + warp.origin[0],
            f64::from(t - by0) + warp.origin[1],
        ];
        let Some([sx, sy]) = warp.back(place) else {
            return bg;
        };
        if !(-EDGE..wf).contains(&sx) || !(-EDGE..hf).contains(&sy) {
            return bg;
        }
        let (ix, it) = (
            (sx - 0.499_999_985).round_ties_even() as c_int,
            (sy - 0.499_999_985).round_ties_even() as c_int,
        );
        let (fx, ft) = (sx - f64::from(ix), sy - f64::from(it));
        let (a, b, c, d) = (
            texel(ix, it),
            texel(ix + 1, it),
            texel(ix, it + 1),
            texel(ix + 1, it + 1),
        );
        std::array::from_fn(|k| {
            let v = |p: [f32; 4]| f64::from(p[k]);
            ((v(c) * (1.0 - fx) + v(d) * fx) * ft + (v(a) * (1.0 - fx) + v(b) * fx) * (1.0 - ft))
                as f32
        })
    })
}

// Presets.

/// The factory presets of effect `kind`.
pub(crate) fn presets(kind: Kind) -> Vec<Preset> {
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    let p = |name: &'static str, values: Vec<(&'static str, PresetValue)>| Preset { name, values };
    let one = |name: &'static str, param: &'static str, v: f64| p(name, vec![(param, num(&[v]))]);
    // Spherize's, Swirl's and Pinch/Punch's: Amount and the scaling.
    let lens = |name: &'static str, amount: f64, scale: f64| {
        p(
            name,
            vec![
                ("center", num(&[0.5, 0.5])),
                ("amount", num(&[amount])),
                ("horizontal", num(&[scale])),
                ("vertical", num(&[scale])),
                ("proportional", num(&[1.0])),
            ],
        )
    };
    match kind {
        Kind::TinyPlanet => {
            let m = |name: &'static str, latitude: f64| {
                p(
                    name,
                    vec![
                        ("latitude", num(&[latitude])),
                        ("longitude", num(&[0.0])),
                        ("z", num(&[20.0])),
                    ],
                )
            };
            vec![
                m("Tiny Planet", 0.0),
                m("Tiny Sky", 180.0),
                m("Flat Horizon", 90.0),
            ]
        }
        Kind::Defocus => {
            // VEGAS's three.
            let d = |name, radius, shape: f64, curvature, threshold, strength| {
                p(
                    name,
                    vec![
                        ("radius", num(&[radius])),
                        ("irisShape", num(&[shape])),
                        ("orientation", num(&[0.0])),
                        ("discCurvature", num(&[curvature])),
                        ("bloomThreshold", num(&[threshold])),
                        ("bloomStrength", num(&[strength])),
                        ("reduceFlicker", num(&[0.0])),
                    ],
                )
            };
            vec![
                d("Circle, Moderate Bloom", 3.48, 0.0, 0.0, 92.64, 6.78),
                d("Hexagon", 5.6, 2.0, 0.0, 94.97, 10.51),
                d("Visible Outer Ring", 3.74, 0.0, -100.0, 92.64, 6.78),
            ]
        }
        Kind::LinearBlur => {
            let m = |name: &'static str, angle: f64, amount: f64| {
                p(
                    name,
                    vec![("angle", num(&[angle])), ("amount", num(&[amount]))],
                )
            };
            vec![
                m("Horizontal Light", 0.0, 0.05),
                m("Vertical Light", 90.0, 0.05),
                m("Horizontal Medium", 0.0, 0.1),
                m("Vertical Extreme", 90.0, 0.2),
                m("45 Degrees", 45.0, 0.1),
                m("30 Degrees Light", 30.0, 0.05),
                m("60 Degrees Extreme", 60.0, 0.2),
            ]
        }
        Kind::QuickBlur => vec![p("Default", vec![])],
        Kind::RadialBlur => {
            let m = |name: &'static str, kind: f64, strength: f64| {
                p(
                    name,
                    vec![
                        ("center", num(&[0.5, 0.5])),
                        ("type", num(&[kind])),
                        ("strength", num(&[strength])),
                    ],
                )
            };
            vec![
                m("Weak Fixed Radius", 1.0, 0.4),
                m("Moderate Fixed Radius", 1.0, 0.6),
                m("Intense Fixed Radius", 1.0, 0.8),
                m("Weak Proportional", 0.0, 0.4),
                m("Moderate Proportional", 0.0, 0.6),
                m("Strong Proportional", 0.0, 0.8),
            ]
        }
        Kind::Median => {
            let m = |name: &'static str, x: f64, y: f64, offset: f64, round: bool| {
                p(
                    name,
                    vec![
                        ("horizontalRange", num(&[x])),
                        ("verticalRange", num(&[y])),
                        ("offset", num(&[offset])),
                        ("circular", num(&[f64::from(u8::from(round))])),
                    ],
                )
            };
            vec![
                m("25%", 0.25, 0.25, 0.5, false),
                m("50%", 0.5, 0.5, 0.5, false),
                m("75%", 0.75, 0.75, 0.5, false),
                m("Light Noise Reduction", 0.02, 0.02, 0.5, false),
                m("Round 50%", 0.5, 0.5, 0.5, true),
                m("Round Noise Reduction", 0.02, 0.02, 0.5, true),
                m("Round Extreme Horizontal", 0.75, 0.1, 0.5, true),
                m("Round Max", 0.5, 0.5, 1.0, true),
            ]
        }
        Kind::MinMax => {
            let m = |name: &'static str, operation: f64| {
                p(
                    name,
                    vec![
                        ("operation", num(&[operation])),
                        ("horizontalRange", num(&[0.5])),
                        ("verticalRange", num(&[0.5])),
                        ("circular", num(&[0.0])),
                    ],
                )
            };
            vec![m("Min 50%", 0.0), m("Max 50%", 1.0)]
        }
        Kind::Convolution => {
            let m = |name: &'static str, w: [f64; 9], scale: f64, offset: f64| {
                let names = [
                    "r1c1", "r1c2", "r1c3", "r2c1", "r2c2", "r2c3", "r3c1", "r3c2", "r3c3",
                ];
                let mut v: Vec<(&'static str, PresetValue)> =
                    names.iter().zip(w).map(|(n, x)| (*n, num(&[x]))).collect();
                v.push(("rotation", num(&[0.0])));
                v.push(("scale", num(&[scale])));
                v.push(("offset", num(&[offset])));
                v.push(("autoNormalize", num(&[1.0])));
                p(name, v)
            };
            vec![
                m(
                    "Blur",
                    [4.0, 4.0, 4.0, 4.0, 5.0, 4.0, 4.0, 4.0, 4.0],
                    0.0,
                    0.0,
                ),
                m(
                    "Emboss",
                    [2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -2.0],
                    1.0,
                    0.5,
                ),
                m(
                    "Bump",
                    [20.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -20.0],
                    1.0,
                    0.0,
                ),
                m(
                    "Find Edges",
                    [1.0, 1.0, 1.0, 1.0, -8.0, 1.0, 1.0, 1.0, 1.0],
                    1.0,
                    0.0,
                ),
                m(
                    "Sharpen Edges",
                    [1.0, 1.0, 1.0, 1.0, -7.0, 1.0, 1.0, 1.0, 1.0],
                    1.0,
                    0.0,
                ),
                m(
                    "Sharpen",
                    [-1.0, -2.0, -1.0, -2.0, 16.0, -2.0, -1.0, -2.0, -1.0],
                    0.0,
                    0.0,
                ),
            ]
        }
        Kind::Spherize => vec![
            lens("Maximum Sphere In", 1.0, 1.0),
            lens("Medium Sphere In", 0.5, 1.0),
            lens("Maximum Sphere Out", -1.0, 1.0),
            lens("Medium Sphere Out", -0.5, 1.0),
            lens("Maximum Sphere In, Half Scale", -0.5, 0.5),
            lens("Maximum Sphere Out, Half Scale", -1.0, 0.5),
        ],
        Kind::Swirl => vec![
            lens("Maximum Swirl, Clockwise", 1.0, 1.0),
            lens("Medium Swirl, Clockwise", 0.5, 1.0),
            lens("Maximum Swirl, Counter Clockwise", -1.0, 1.0),
            lens("Medium Swirl, Counter Clockwise", -0.5, 1.0),
            lens("Maximum Swirl, Clockwise, Half Scale", 1.0, 0.5),
            lens("Maximum Swirl, Counter Clockwise, Half Scale", -1.0, 0.5),
        ],
        Kind::Threshold => vec![
            one("25%", "threshold", 0.25),
            one("40%", "threshold", 0.4),
            one("50%", "threshold", 0.5),
            one("60%", "threshold", 0.6),
            one("75%", "threshold", 0.75),
        ],
        Kind::Wave => {
            let m = |name: &'static str, waves: [f64; 2], amplitude: [f64; 2], phase: [f64; 2]| {
                p(
                    name,
                    vec![
                        ("verticalWaves", num(&[waves[0]])),
                        ("horizontalWaves", num(&[waves[1]])),
                        ("verticalAmplitude", num(&[amplitude[0]])),
                        ("horizontalAmplitude", num(&[amplitude[1]])),
                        ("verticalPhase", num(&[phase[0]])),
                        ("horizontalPhase", num(&[phase[1]])),
                    ],
                )
            };
            vec![
                m("Large", [5.4, 5.4], [0.05, 0.05], [0.0, 0.0]),
                m("Medium", [14.0, 14.0], [0.05, 0.05], [0.0, 0.0]),
                m("Small", [20.0, 20.0], [0.03, 0.03], [0.0, 0.0]),
                m("Vertical Only", [10.0, 0.0], [0.04, 0.0], [0.8, 0.0]),
                m("Horizontal Only", [0.0, 10.0], [0.0, 0.04], [0.0, 0.8]),
            ]
        }
        Kind::PinchPunch => vec![
            lens("Maximum Punch", 1.0, 1.0),
            lens("Medium Punch", 0.5, 1.0),
            lens("Maximum Pinch", -1.0, 1.0),
            lens("Medium Pinch", -0.5, 1.0),
            lens("Maximum Punch, Half Scale", 1.0, 1.0),
            lens("Maximum Pinch, Half Scale", -1.0, 1.0),
        ],
        Kind::OffsetWrap => vec![p("Default", vec![])],
        Kind::BlackRestore => vec![one("Streaming", "threshold", 0.02)],
        Kind::BroadcastColors => {
            // VEGAS's: Lenient, Conservative and Extremely Conservative —
            // the composite's top at 120, 110 and 100 IRE —, each also with
            // the 7.5 IRE setup (black at 7.5).
            let broadcast = |name: &'static str, top: f64, setup: bool| {
                p(
                    name,
                    vec![
                        ("lumaMin", num(&[if setup { 7.5 } else { 0.0 }])),
                        ("lumaMinSmoothness", num(&[0.0])),
                        ("lumaMax", num(&[100.0])),
                        ("lumaMaxSmoothness", num(&[0.0])),
                        ("chromaMax", num(&[100.0])),
                        ("chromaSmoothness", num(&[0.0])),
                        ("compositeMin", num(&[-20.0])),
                        ("compositeMinSmoothness", num(&[0.0])),
                        ("compositeMax", num(&[top])),
                        ("compositeMaxSmoothness", num(&[0.0])),
                        ("setup", num(&[f64::from(u8::from(setup))])),
                        ("studioRgb", num(&[1.0])),
                    ],
                )
            };
            vec![
                broadcast("Lenient", 120.0, false),
                broadcast("Lenient - 7.5 Setup", 120.0, true),
                broadcast("Conservative", 110.0, false),
                broadcast("Conservative - 7.5 Setup", 110.0, true),
                broadcast("Extremely Conservative", 100.0, false),
                broadcast("Extremely Conservative - 7.5 Setup", 100.0, true),
            ]
        }
        Kind::ChannelBlend => {
            // VEGAS's, with Normalize as each says: the rows red, green,
            // blue and alpha, each its weights of red, green, blue and alpha
            // and its offset.
            let m = |name: &'static str, normalize: bool, rows: [[f64; 4]; 4]| {
                let mut values = vec![("normalize", num(&[f64::from(u8::from(normalize))]))];
                for (r, row) in rows.iter().enumerate() {
                    for (c, x) in row.iter().chain(&[0.0]).enumerate() {
                        values.push((BLEND_KEYS[r * 5 + c], num(&[*x])));
                    }
                }
                p(name, values)
            };
            let alpha = [0.0, 0.0, 0.0, 1.0];
            let all = [1.0, 1.0, 1.0, 0.0];
            let none = [0.0; 4];
            let only = [1.0, 1.0, 1.0, 1.0];
            let from = |k: usize| {
                let mut w = [0.0; 4];
                w[k] = 1.0;
                w
            };
            vec![
                m("Grayscale", true, [all, all, all, alpha]),
                m("Distribute Red", false, [from(0), from(0), from(0), alpha]),
                m(
                    "Distribute Green",
                    false,
                    [from(1), from(1), from(1), alpha],
                ),
                m("Distribute Blue", false, [from(2), from(2), from(2), alpha]),
                m("Red channel only", true, [only, none, none, alpha]),
                m("Green channel only", true, [none, only, none, alpha]),
                m("Blue channel only", true, [none, none, only, alpha]),
                m("RGBA -> BGRA", false, [from(2), from(1), from(0), alpha]),
            ]
        }
        Kind::ChromaBlur => {
            let both = |name: &'static str, v: f64| {
                p(
                    name,
                    vec![
                        ("horizontalPixels", num(&[v])),
                        ("verticalPixels", num(&[v])),
                    ],
                )
            };
            vec![
                both("Light Blur", 1.0),
                both("Medium Blur", 2.0),
                both("Maximum Blur", 4.0),
            ]
        }
        Kind::MaskGenerator => [
            "Luminance Mask",
            "Red Channel Mask",
            "Green Channel Mask",
            "Blue Channel Mask",
            "Alpha Mask",
        ]
        .into_iter()
        .zip(0..)
        .map(|(name, ty)| one(name, "type", f64::from(ty)))
        .collect(),
        Kind::RadialPixelate => {
            let m = |name: &'static str, kind: f64, strength: f64| {
                p(
                    name,
                    vec![
                        ("center", num(&[0.5, 0.5])),
                        ("type", num(&[kind])),
                        ("strength", num(&[strength])),
                    ],
                )
            };
            vec![
                m("Weak Radial", 0.0, 0.4),
                m("Moderate Radial", 0.0, 0.6),
                m("Strong Radial", 0.0, 0.8),
                m("Weak Circular", 1.0, 0.4),
                m("Moderate Circular", 1.0, 0.6),
                m("Strong Circular", 1.0, 0.8),
            ]
        }
        Kind::Deform => {
            // Amount, Center image and the settings that differ from 0.
            let m = |name: &'static str, amount: f64, centered: bool, set: &[&'static str]| {
                let mut v = vec![
                    ("amount", num(&[amount])),
                    ("centerImage", num(&[f64::from(u8::from(centered))])),
                    ("background", num(&[0.0, 0.0, 0.0, 0.0])),
                ];
                for name in [
                    "squeezeLeft",
                    "squeezeRight",
                    "squeezeTop",
                    "squeezeBottom",
                    "shearHorizontal",
                    "shearVertical",
                    "curveLeft",
                    "curveRight",
                    "curveTop",
                    "curveBottom",
                ] {
                    v.push((name, num(&[f64::from(u8::from(set.contains(&name)))])));
                }
                p(name, v)
            };
            vec![
                m("Shear Horizontally", 1.0, true, &["shearHorizontal"]),
                m("Shear Vertically", 1.0, true, &["shearVertical"]),
                m("Squeeze Top", 1.0, true, &["squeezeTop"]),
                m("Squeeze Bottom", 1.0, true, &["squeezeBottom"]),
                m("Squeeze Left", 1.0, true, &["squeezeLeft"]),
                m("Squeeze Right", 1.0, true, &["squeezeRight"]),
                m("Curve Top", 1.0, false, &["curveTop"]),
                m("Curve Bottom", 1.0, false, &["curveBottom"]),
                m("Curve Left", 1.0, false, &["curveLeft"]),
                m("Curve Right", 1.0, false, &["curveRight"]),
                m("Bend Horizontally", 1.0, true, &["curveLeft", "curveRight"]),
                m("Bend Vertically", 1.0, true, &["curveTop", "curveBottom"]),
                m(
                    "Compress Horizontally",
                    0.5,
                    true,
                    &["squeezeTop", "squeezeBottom"],
                ),
                m(
                    "Compress Vertically",
                    0.5,
                    true,
                    &["squeezeLeft", "squeezeRight"],
                ),
            ]
        }
        Kind::LabAdjust
        | Kind::SoftContrast
        | Kind::FillLight
        | Kind::LensFlare
        | Kind::Starburst
        | Kind::Glint
        | Kind::Rays
        | Kind::LightRays
        | Kind::FilmEffects
        | Kind::FilmGrain
        | Kind::Newsprint
        | Kind::TvSimulator
        | Kind::BumpMap
        | Kind::Denoise
        | Kind::Looks => crate::looks::presets(kind),
        Kind::ChromaKeyer => crate::classics::presets(kind),
    }
}

/// Threshold's step: VEGAS's 8-bit one, the threshold (single
/// precision) times 256.
pub(crate) fn threshold_step(s: &Vals) -> u32 {
    (f64::from(s.f(c"threshold") as f32) * 256.0).max(0.0) as u32
}

/// Wave's settings in VEGAS's order: the vertical and horizontal waves,
/// amplitudes and phases.
pub(crate) fn wave_numbers(s: &Vals) -> [f64; 6] {
    [
        c"verticalWaves",
        c"horizontalWaves",
        c"verticalAmplitude",
        c"horizontalAmplitude",
        c"verticalPhase",
        c"horizontalPhase",
    ]
    .map(|n| s.f(n))
}

/// π as VEGAS's Wave takes it.
#[allow(clippy::approx_constant)]
const WAVE_PI: f64 = 3.141;

/// VEGAS's Wave on a picture `w` × `h`, in 16,384ths of a pixel, rows
/// counted from the top as VEGAS counts them: each row's move across —
/// the vertical waves' sine at its place down, π taken as 3.141, times the
/// width and the vertical amplitude, cut toward 0 — and each column's move
/// down, the horizontal waves' at its place times the height and the
/// horizontal amplitude, down to the step below (a pixel's place then
/// within the picture less its last row and column).
pub(crate) fn wave_offsets(n: &[f64; 6], w: u32, h: u32) -> (Vec<c_int>, Vec<c_int>) {
    let [vw, hw, va, ha, vp, hp] = *n;
    let (wf, hf) = (f64::from(w), f64::from(h));
    let rows = (0..h)
        .map(|y| ((f64::from(y) / hf * vw * WAVE_PI + vp).sin() * wf * va * 16_384.0) as c_int)
        .collect();
    let columns = (0..w)
        .map(|x| {
            ((f64::from(x) / wf * hw * WAVE_PI + hp).sin() * hf * ha * 16_384.0).floor() as c_int
        })
        .collect();
    (rows, columns)
}

/// Swirl's and Pinch/Punch's settings: Center, Horizontal, Vertical
/// (Horizontal again when proportional) and Amount.
pub(crate) fn lens_settings(s: &Vals) -> [f64; 5] {
    let [cx, cy] = s.point(c"center");
    let h = s.f(c"horizontal");
    let v = if s.b(c"proportional") {
        h
    } else {
        s.f(c"vertical")
    };
    [cx, cy, h, v, s.f(c"amount").clamp(-1.0, 1.0)]
}

/// A place's pixel as VEGAS takes it — the one at or before it, a place
/// within 1.5e-8 of the next one taking that one — and how far past it.
fn place_step(v: f64) -> (c_int, f32) {
    let i = (v - 0.499_999_985).round_ties_even() as c_int;
    (i, (v - f64::from(i)) as f32)
}

/// Swirl or Pinch/Punch as VEGAS's, rows counted from the top: in the area
/// each pixel shows the picture turned about the center by 15 × Amount ×
/// (1 − r)² radians (Swirl, clockwise above 0), or at its offset times 1 −
/// (cos πr + 1) × Amount × 0.4 (Pinch/Punch: the middle grows above 0) —
/// r its distance over the area —, read between its four nearest pixels
/// (Swirl's weighted by their alpha), held at the edges; out of the area
/// the pixel stays.
fn lensed(swirl: bool, s: &Vals, src: &Buf) -> Buf {
    let (w, h) = (src.width(), src.height());
    if w < 1 || h < 1 {
        return src.clone();
    }
    let settings = lens_settings(s);
    let amount = settings[4];
    let lens = Lens::of(&settings, w as u32, h as u32);
    let table = lens.table(w as u32, h as u32);
    let [ci, cj] = lens.center;
    let [x1, y1, ..] = src.bounds;
    // The pixel at column x and row t from the top, held in the picture.
    let texel = |x: c_int, t: c_int| {
        let (x, t) = (x.clamp(0, w - 1), t.clamp(0, h - 1));
        src.data[((h - 1 - t) * w + x) as usize]
    };
    src.map(|x, y| {
        let (x, t) = (x - x1, h - 1 - (y - y1));
        if out_of_lens(&table, w, x, t) {
            return texel(x, t);
        }
        let (u, v) = (lens.across(x), lens.down(t));
        let r = (u * u + v * v).sqrt();
        if swirl {
            let turn = (1.0 - r) * (amount * 15.0) * (1.0 - r);
            let (sin, cos) = turn.sin_cos();
            let (it, ft) = place_step(f64::from(cj) + (cos * v - sin * u) * lens.reach[1]);
            let (ix, fx) = place_step(f64::from(ci) + (cos * u + sin * v) * lens.reach[0]);
            let (a, b, c, d) = (
                texel(ix, it),
                texel(ix + 1, it),
                texel(ix, it + 1),
                texel(ix + 1, it + 1),
            );
            // Each weight times its pixel's alpha.
            let (wa, wb) = ((1.0 - fx) * a[3] * (1.0 - ft), fx * b[3] * (1.0 - ft));
            let (wc, wd) = ((1.0 - fx) * c[3] * ft, fx * d[3] * ft);
            let alpha = wb + wa + wc + wd;
            if alpha <= 0.0 {
                return [0.0; 4];
            }
            let mut o: [f32; 4] =
                std::array::from_fn(|k| wa * a[k] + wb * b[k] + wc * c[k] + wd * d[k]);
            o[3] = alpha;
            o
        } else {
            let k = 1.0 - ((r * PINCH_PI).cos() + 1.0) * amount * 0.4;
            let (it, ft) = place_step(f64::from(t - cj) * k + f64::from(cj));
            let (ix, fx) = place_step(k * f64::from(x - ci) + f64::from(ci));
            let (a, b, c, d) = (
                texel(ix, it),
                texel(ix + 1, it),
                texel(ix, it + 1),
                texel(ix + 1, it + 1),
            );
            std::array::from_fn(|i| {
                ((1.0 - fx) * c[i] + fx * d[i]) * ft + ((1.0 - fx) * a[i] + fx * b[i]) * (1.0 - ft)
            })
        }
    })
}

/// Whether column `x` and row `t` lie out of the lens's area by its
/// `table` (the squares' sum 1 or more).
fn out_of_lens(table: &[[f32; 4]], w: c_int, x: c_int, t: c_int) -> bool {
    table[x as usize][1] + table[(w + t) as usize][1] >= 1.0
}

/// π as VEGAS's Pinch/Punch takes it.
#[allow(clippy::approx_constant)]
const PINCH_PI: f64 = 3.141_592_6;

/// Channel Blend's parameter names as presets name them.
const BLEND_KEYS: [&str; 20] = [
    "redRed",
    "redGreen",
    "redBlue",
    "redAlpha",
    "redOffset",
    "greenRed",
    "greenGreen",
    "greenBlue",
    "greenAlpha",
    "greenOffset",
    "blueRed",
    "blueGreen",
    "blueBlue",
    "blueAlpha",
    "blueOffset",
    "alphaRed",
    "alphaGreen",
    "alphaBlue",
    "alphaAlpha",
    "alphaOffset",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A preset's settings: its values, the others' defaults.
    fn vals(kind: Kind, preset: &Preset) -> Vals {
        Vals::of(kind.specs(), &|spec| {
            let key = spec.name().to_str().unwrap_or("");
            preset
                .values
                .iter()
                .find(|(n, _)| *n == key)
                .and_then(|(_, v)| match v {
                    PresetValue::Num(v) => Some(v.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| spec.default())
        })
    }

    fn grid() -> Grid {
        Grid {
            origin: [0.0, 0.0],
            frame: [32.0, 18.0],
            scale: [1.0, 1.0],
            par: 1.0,
        }
    }

    /// VEGAS's Defocus: a light spread over its iris, as much light as
    /// before; a polygon's corners reach farther than its sides.
    #[test]
    fn defocus_spreads_a_light_over_its_iris() {
        let g = Grid {
            origin: [0.0, 0.0],
            frame: [100.0, 60.0],
            scale: [1.0, 1.0],
            par: 1.0,
        };
        let mut b = Buf::solid([0, 0, 100, 60], [0.0, 0.0, 0.0, 1.0]);
        b.data[30 * 100 + 50] = [1.0, 1.0, 1.0, 1.0];
        // Radius 10 % of 100 pixels: an iris of 9 pixels; no bloom.
        let s = |shape: f64| {
            Vals::of(
                Kind::Defocus.specs(),
                &|spec| match spec.name().to_bytes() {
                    b"radius" => vec![10.0],
                    b"irisShape" => vec![shape],
                    b"bloomThreshold" => vec![100.0],
                    _ => spec.default(),
                },
            )
        };
        let disc = defocused(&s(0.0), &g, &b);
        let total: f32 = disc.data.iter().map(|c| c[0]).sum();
        assert!((total - 1.0).abs() < 1e-3, "{total}");
        assert!(disc.at(50 + 9, 30)[0] > 1e-4);
        assert!(disc.at(50 + 11, 30)[0].abs() < 1e-5);
        // The pentagon: a corner straight up, its sides' middles nearer.
        let pentagon = defocused(&s(1.0), &g, &b);
        assert!(pentagon.at(50, 30 + 8)[0] > 1e-4, "a corner up");
        assert!(pentagon.at(50, 30 - 8)[0].abs() < 1e-5, "a side down");
        // The bloom: white brightened by Bloom strength less the threshold.
        let bright = Defocus::of([10.0, 0.0, 0.0, 0.0, 90.0, 10.0, 0.0], 100.0, 60.0, 1.0);
        let [t, k] = bright.bloom().unwrap();
        assert!((1.0 + (1.0 - t) * k - (11.0 - 0.9)).abs() < 1e-4);
        assert!(
            Defocus::of([10.0, 0.0, 0.0, 0.0, 100.0, 10.0, 0.0], 100.0, 60.0, 1.0)
                .bloom()
                .is_none()
        );
    }

    /// A small picture: a horizontal ramp, a bright spot in the middle.
    fn picture() -> Buf {
        let mut b = Buf::solid([0, 0, 32, 18], [0.0, 0.0, 0.0, 1.0]);
        for y in 0..18 {
            for x in 0..32 {
                let v = x as f32 / 31.0;
                b.data[(y * 32 + x) as usize] = [v, 0.5, 1.0 - v, 1.0];
            }
        }
        b.data[(9 * 32 + 16) as usize] = [1.0; 4];
        b
    }

    /// A frame `w` × `h` whose pixels tell their column (red) and row from
    /// the top (green).
    fn places(w: c_int, h: c_int) -> Buf {
        let mut b = Buf::solid([0, 0, w, h], [0.0; 4]);
        for y in 0..h {
            for x in 0..w {
                b.data[(y * w + x) as usize] =
                    [x as f32 / 64.0, (h - 1 - y) as f32 / 64.0, 0.0, 1.0];
            }
        }
        b
    }

    /// VEGAS's Swirl at Amount 1 ("Maximum Swirl, Clockwise"): right of
    /// the center the picture comes from above (clockwise), at the center
    /// it stays, out of the area it is the same.
    #[test]
    fn swirl_turns_the_picture_clockwise_above_0() {
        let src = places(41, 41);
        let presets = presets(Kind::Swirl);
        assert_eq!(presets.len(), 6, "VEGAS's six");
        let s = vals(Kind::Swirl, &presets[0]);
        assert_eq!(lens_settings(&s)[4], 1.0);
        let out = Kind::Swirl.process(&s, &grid(), &src, 0.0);
        // Center pixel 20, 20 (from the top): it stays.
        assert_eq!(out.at(20, 20), src.at(20, 20));
        // Near the area's right edge (little turned) a pixel shows what
        // was above it: row 16 from the top.
        let right = out.at(38, 20);
        assert!((15.5..16.5).contains(&(right[1] * 64.0)), "{right:?}");
        // A corner is out of the area.
        assert_eq!(out.at(0, 0), src.at(0, 0));
        // Below 0 the other way ("Maximum Swirl, Counter Clockwise"):
        // from below.
        let back = vals(Kind::Swirl, &presets[2]);
        assert_eq!(lens_settings(&back)[4], -1.0);
        let other = Kind::Swirl.process(&back, &grid(), &src, 0.0).at(38, 20);
        assert!((23.5..24.5).contains(&(other[1] * 64.0)), "{other:?}");
    }

    /// VEGAS's Pinch/Punch: a punch (above 0) shows near the center what
    /// lies nearer still (the middle grows), a pinch what lies farther.
    #[test]
    fn punch_grows_the_middle_and_pinch_shrinks_it() {
        let src = places(41, 41);
        let presets = presets(Kind::PinchPunch);
        assert_eq!(presets.len(), 6, "VEGAS's six");
        let punch = vals(Kind::PinchPunch, &presets[0]);
        let out = Kind::PinchPunch.process(&punch, &grid(), &src, 0.0);
        // Column 25 (5 right of the center) shows about column 21.
        let near = out.at(25, 20)[0] * 64.0;
        assert!((20.5..23.0).contains(&near), "{near}");
        let pinch = vals(Kind::PinchPunch, &presets[2]);
        let far = Kind::PinchPunch
            .process(&pinch, &grid(), &src, 0.0)
            .at(25, 20)[0]
            * 64.0;
        assert!(far > 27.0, "{far}");
    }

    /// VEGAS's Deform: "Compress Horizontally" halves the width about the
    /// middle (the background past it), "Squeeze Top" brings the top edge
    /// to a point (rows counted from the top), the bottom staying wide.
    #[test]
    fn deform_compresses_and_squeezes_as_vegas() {
        let src = places(40, 20);
        let presets = presets(Kind::Deform);
        let named = |n: &str| {
            let p = presets.iter().find(|p| p.name == n).expect(n);
            vals(Kind::Deform, p)
        };
        let compress = Kind::Deform.process(&named("Compress Horizontally"), &grid(), &src, 0.0);
        // Column 15 shows column 10; column 5 is past the picture.
        assert!(
            (compress.at(15, 10)[0] * 64.0 - 10.0).abs() < 1e-4,
            "{:?}",
            compress.at(15, 10)
        );
        assert_eq!(compress.at(5, 10), [0.0; 4]);
        let squeeze = Kind::Deform.process(&named("Squeeze Top"), &grid(), &src, 0.0);
        // The top row (19 here, rows go up) is a point: nothing at its
        // left; the bottom row reaches nearly to the edge.
        assert_eq!(squeeze.at(2, 19), [0.0; 4]);
        assert!(squeeze.at(2, 0)[3] > 0.99, "{:?}", squeeze.at(2, 0));
    }

    /// VEGAS's Wave counts rows from the top: with no horizontal waves the
    /// top row stays where it is, the rows below move across.
    #[test]
    fn wave_moves_rows_counted_from_the_top() {
        let src = places(64, 36);
        let s = Vals::of(Kind::Wave.specs(), &|spec| match spec.name().to_bytes() {
            b"verticalWaves" => vec![5.0],
            b"verticalAmplitude" => vec![0.05],
            _ => vec![0.0],
        });
        let out = Kind::Wave.process(&s, &grid(), &src, 0.0);
        // The top row (35 here, rows go up) is unmoved.
        assert_eq!(out.at(10, 35), src.at(10, 35));
        assert_ne!(out.at(10, 30), src.at(10, 30));
    }

    /// VEGAS's Black Restore: a pixel any of whose red, green and blue is
    /// past the threshold stays; else they go to black, its alpha kept.
    #[test]
    fn black_restore_keeps_a_pixel_with_any_channel_past_the_threshold() {
        let presets = presets(Kind::BlackRestore);
        assert_eq!(presets.len(), 1, "VEGAS's Streaming alone");
        let s = vals(Kind::BlackRestore, &presets[0]);
        let mut src = Buf::solid([0, 0, 3, 1], [0.0; 4]);
        src.data[0] = [0.01, 0.015, 0.02, 1.0];
        src.data[1] = [0.0, 0.0, 0.03, 1.0];
        src.data[2] = [0.01, 0.01, 0.01, 0.5];
        let out = Kind::BlackRestore.process(&s, &grid(), &src, 0.0);
        assert_eq!(out.data[0], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(out.data[1], src.data[1]);
        assert_eq!(out.data[2], [0.0, 0.0, 0.0, 0.5]);
    }

    /// VEGAS's Broadcast Colors, its IRE math worked out by hand: white
    /// comes down to 100 IRE (235 in studio RGB), black up to the setup's
    /// 7.5 (16), and red's chroma — 147 IRE peak to peak — down to 100,
    /// then until red less its chroma stays over the composite's −20.
    #[test]
    fn broadcast_colors_holds_luma_chroma_and_composite_as_vegas_does() {
        let presets = presets(Kind::BroadcastColors);
        assert_eq!(presets.len(), 6);
        let run = |preset: usize, c: [f32; 4]| {
            let s = vals(Kind::BroadcastColors, &presets[preset]);
            let src = Buf::solid([0, 0, 1, 1], c);
            Kind::BroadcastColors.process(&s, &grid(), &src, 0.0).data[0]
        };
        let close = |a: [f32; 4], b: [f64; 4]| {
            a.iter()
                .zip(b)
                .all(|(x, y)| (f64::from(*x) - y).abs() < 1e-4)
        };
        // Conservative: white to 235.
        let w = 235.0 / 255.0;
        assert!(
            close(run(2, [1.0; 4]), [w, w, w, 1.0]),
            "{:?}",
            run(2, [1.0; 4])
        );
        // Lenient - 7.5 Setup: black to 16.
        let b = 16.0 / 255.0;
        let black = run(1, [0.0, 0.0, 0.0, 1.0]);
        assert!(close(black, [b, b, b, 1.0]), "{black:?}");
        // Conservative: red.
        let k = 100.0 / 219.0;
        let y = k * (239.0 * 0.299 - 16.0 * 0.701);
        let (u, v): (f64, f64) = (-0.147 * 255.0, 0.615 * 255.0);
        let amp = (u * u + v * v).sqrt() * k;
        let f = 100.0 / (2.0 * amp) * ((y + 20.0) / 50.0);
        let (u, v) = (u * f / 255.0, v * f / 255.0);
        let yn = (y / k + 16.0) / 255.0;
        let red = run(2, [1.0, 0.0, 0.0, 1.0]);
        let expected = [
            v * 1.14 + yn,
            yn - u * 0.394 - v * 0.581,
            u * 2.032 + yn,
            1.0,
        ];
        assert!(close(red, expected), "{red:?} {expected:?}");
        // The defaults (−8 to 110, every limit at its widest) leave colors
        // within them alone.
        let mid = [0.4, 0.5, 0.6, 1.0];
        let s = Vals::of(Kind::BroadcastColors.specs(), &|spec| spec.default());
        let out = Kind::BroadcastColors.process(&s, &grid(), &Buf::solid([0, 0, 1, 1], mid), 0.0);
        assert_eq!(out.data[0], mid);
    }

    /// VEGAS's Channel Blend: each output the pixel's values weighted
    /// plus its offset — normalized, by the row's sum —, the colors kept
    /// within 0 and the alpha.
    #[test]
    fn channel_blend_weighs_the_channels_as_vegas_does() {
        let presets = presets(Kind::ChannelBlend);
        assert_eq!(presets.len(), 8);
        let run = |name: &str, c: [f32; 4]| {
            let p = presets.iter().find(|p| p.name == name).expect(name);
            let src = Buf::solid([0, 0, 1, 1], c);
            Kind::ChannelBlend
                .process(&vals(Kind::ChannelBlend, p), &grid(), &src, 0.0)
                .data[0]
        };
        let close = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
        let c = [0.2, 0.4, 0.6, 1.0];
        assert!(close(run("Grayscale", c), [0.4, 0.4, 0.4, 1.0]));
        assert!(close(run("RGBA -> BGRA", c), [0.6, 0.4, 0.2, 1.0]));
        assert!(close(run("Red channel only", c), [0.55, 0.0, 0.0, 1.0]));
        assert!(close(run("Distribute Green", c), [0.4, 0.4, 0.4, 1.0]));
        // Half transparent: the values as they are, within the alpha.
        let half = [0.1, 0.2, 0.3, 0.5];
        assert!(close(run("Distribute Blue", half), [0.3, 0.3, 0.3, 0.5]));
        let mut s = Vals::of(Kind::ChannelBlend.specs(), &|spec| {
            if spec.name() == c"redOffset" {
                vec![2.0]
            } else {
                spec.default()
            }
        });
        let out = Kind::ChannelBlend.process(&s, &grid(), &Buf::solid([0, 0, 1, 1], half), 0.0);
        assert!(
            close(out.data[0], [0.5, 0.2, 0.3, 0.5]),
            "{:?}",
            out.data[0]
        );
        s = Vals::of(Kind::ChannelBlend.specs(), &|spec| spec.default());
        let out = Kind::ChannelBlend.process(&s, &grid(), &Buf::solid([0, 0, 1, 1], half), 0.0);
        assert_eq!(out.data[0], half, "the defaults change nothing");
    }

    /// VEGAS's Chroma Blur at its defaults still blurs: U and V over three
    /// pixels (a Gaussian of 1.5 cut to 1), each pixel's Y its own.
    #[test]
    fn chroma_blur_spreads_the_color_and_keeps_the_brightness() {
        assert_eq!(crate::kit::gauss_boxes(1.0), [1, 1, 3]);
        let mut src = Buf::solid([0, 0, 5, 1], [0.5, 0.5, 0.5, 1.0]);
        src.data[2] = [1.0, 0.0, 0.0, 1.0];
        let s = Vals::of(Kind::ChromaBlur.specs(), &|spec| spec.default());
        let out = Kind::ChromaBlur.process(&s, &grid(), &src, 0.0);
        let (u, v) = (-0.14713_f32 / 3.0, 0.615_f32 / 3.0);
        let gray = |y: f32| {
            [
                y + v * 1.13983,
                y - u * 0.39465 - v * 0.5806,
                y + u * 2.03211,
            ]
        };
        for x in [1, 3] {
            let want = gray(0.5);
            let got = out.data[x];
            assert!(
                (0..3).all(|k| (got[k] - want[k]).abs() < 1e-5),
                "{x}: {got:?} {want:?}"
            );
        }
        // Too far to reach (VEGAS's Y′UV matrices round trip to within 2e-5).
        assert!(
            out.data[0]
                .iter()
                .all(|v| (v - 0.5).abs() < 2e-5 || *v == 1.0)
        );
        let y = out.data[2][0] * 0.299 + out.data[2][1] * 0.587 + out.data[2][2] * 0.114;
        assert!((y - 0.299).abs() < 1e-4, "red's brightness kept: {y}");
    }

    /// VEGAS's Mask Generator: the plain average or a channel, through
    /// the levels, in every channel.
    #[test]
    fn mask_generator_makes_the_mask_as_vegas_does() {
        let presets = presets(Kind::MaskGenerator);
        assert_eq!(presets.len(), 5);
        let c = [0.2, 0.4, 0.9, 1.0];
        let run = |s: &Vals| {
            Kind::MaskGenerator
                .process(s, &grid(), &Buf::solid([0, 0, 1, 1], c), 0.0)
                .data[0]
        };
        let luma = run(&vals(Kind::MaskGenerator, &presets[0]));
        assert!(luma.iter().all(|v| (v - 0.5).abs() < 1e-6), "{luma:?}");
        assert_eq!(run(&vals(Kind::MaskGenerator, &presets[3])), [0.9; 4]);
        // Levels: 0.25–0.75 in, 0.2–0.6 out, inverted.
        let s = Vals::of(
            Kind::MaskGenerator.specs(),
            &|spec| match spec.name().to_bytes() {
                b"invert" => vec![1.0],
                b"lowIn" => vec![0.25],
                b"highIn" => vec![0.75],
                b"lowOut" => vec![0.2],
                b"highOut" => vec![0.6],
                _ => spec.default(),
            },
        );
        // Inverted, 0.5 stays 0.5: half way, so 0.4.
        let out = run(&s);
        assert!(out.iter().all(|v| (v - 0.4).abs() < 1e-6), "{out:?}");
    }

    /// VEGAS's Median and Min and Max: each channel ranked over its area,
    /// whose next pixels weigh the range's fraction, the mean over one
    /// pixel's weight from the rank, in 8 bits, its fraction dropped.
    #[test]
    fn median_and_min_max_rank_the_area_as_vegas_does() {
        assert_eq!(
            area_weights(1.5, 0.0, false),
            vec![vec![WHOLE, WHOLE, WHOLE / 2], vec![0, 0, 0]]
        );
        // The round area: an ellipse half a pixel wider, by coverage.
        let mirrored = |q: &[Vec<i64>]| -> f64 {
            let twice = |n: usize| if n > 0 { 2 } else { 1 };
            let sum: i64 = q
                .iter()
                .enumerate()
                .flat_map(|(j, r)| {
                    r.iter()
                        .enumerate()
                        .map(move |(i, &w)| w * twice(i) * twice(j))
                })
                .sum();
            sum as f64 / WHOLE as f64
        };
        let area = mirrored(&area_weights(3.0, 3.0, true));
        assert!(
            (area - std::f64::consts::PI * 3.5 * 3.5).abs() < 0.05,
            "{area}"
        );
        assert_eq!(mirrored(&area_weights(0.0, 0.0, true)), 1.0);
        // Grays 0, 0.2, 0.4, 0.6 and 0.8 in a row; 1.5 pixels across (a
        // range of 1 in a frame 30 wide), none down.
        let mut src = Buf::solid([0, 0, 5, 1], [0.0; 4]);
        for (i, px) in src.data.iter_mut().enumerate() {
            let v = i as f32 * 0.2;
            *px = [v, v, v, 1.0];
        }
        let g = Grid {
            origin: [0.0, 0.0],
            frame: [30.0, 1.0],
            scale: [1.0, 1.0],
            par: 1.0,
        };
        let run = |kind: Kind, rank: f64| -> Vec<u8> {
            let s = Vals::of(kind.specs(), &|spec| match spec.name().to_bytes() {
                b"horizontalRange" => vec![1.0],
                b"verticalRange" => vec![0.0],
                b"offset" | b"operation" => vec![rank],
                _ => spec.default(),
            });
            let out = kind.process(&s, &g, &src, 0.0);
            out.data
                .iter()
                .map(|c| (c[0] * 255.0).round() as u8)
                .collect()
        };
        assert_eq!(run(Kind::Median, 0.5), [38, 63, 102, 140, 165]);
        assert_eq!(run(Kind::MinMax, 0.0), [0, 0, 25, 76, 127]);
        assert_eq!(run(Kind::MinMax, 1.0), [76, 127, 178, 204, 204]);
        assert_eq!(presets(Kind::Median).len(), 8, "VEGAS's eight");
        assert_eq!(presets(Kind::MinMax).len(), 2, "VEGAS's two");
    }

    /// VEGAS's Convolution Kernel: Row 1 above the pixel, Column 1 on its
    /// left; Rotation turns the outer weights a place each 45°; Auto
    /// normalize scales them to add up to 1, or leaves them when they add
    /// up to nothing; the alpha is summed only when they reach 1.
    #[test]
    fn convolution_kernel_weighs_the_neighbors_as_vegas_does() {
        // Each pixel its own gray (rows go up), half transparent.
        let mut src = Buf::solid([0, 0, 3, 3], [0.0; 4]);
        for (i, px) in src.data.iter_mut().enumerate() {
            let v = i as f32 / 10.0;
            *px = [v, v, v, 0.5];
        }
        let run = |values: &[(&[u8], f64)]| {
            let s = Vals::of(Kind::Convolution.specs(), &|spec| {
                values
                    .iter()
                    .find(|(n, _)| *n == spec.name().to_bytes())
                    .map_or_else(|| spec.default(), |(_, v)| vec![*v])
            });
            Kind::Convolution.process(&s, &grid(), &src, 0.0).at(1, 1)
        };
        let near = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(p, q)| (p - q).abs() < 1e-5);
        // The pixel above on the left: 0.6.
        let corner = run(&[(b"r1c1", 1.0), (b"r2c2", 0.0)]);
        assert!(near(corner, [0.3, 0.3, 0.3, 0.5]), "{corner:?}");
        // Turned 45°: the one above, 0.7; 22.5°: half of each.
        let turned = run(&[(b"r1c1", 1.0), (b"r2c2", 0.0), (b"rotation", 45.0)]);
        assert!(near(turned, [0.35, 0.35, 0.35, 0.5]), "{turned:?}");
        let half = run(&[(b"r1c1", 1.0), (b"r2c2", 0.0), (b"rotation", -337.5)]);
        assert!(near(half, [0.325, 0.325, 0.325, 0.5]), "{half:?}");
        // Adding up to nothing: as they are, the alpha kept; the one
        // below on the right is 0.2.
        let emboss = run(&[
            (b"r1c1", 2.0),
            (b"r2c2", 0.0),
            (b"r3c3", -2.0),
            (b"offset", 0.5),
        ]);
        assert!(near(emboss, [0.65, 0.65, 0.65, 0.5]), "{emboss:?}");
        // Scale without Auto normalize: the alpha summed, within 0–1.
        let scaled = run(&[(b"autoNormalize", 0.0), (b"scale", 2.0)]);
        assert!(near(scaled, [0.8, 0.8, 0.8, 1.0]), "{scaled:?}");
        assert_eq!(presets(Kind::Convolution).len(), 6, "VEGAS's six");
    }

    /// VEGAS's Linear Blur: a Gaussian along Angle, turning clockwise,
    /// across it only VEGAS's least (a radius of 0).
    #[test]
    fn linear_blur_spreads_along_its_angle_as_vegas_does() {
        let g = Grid {
            origin: [0.0, 0.0],
            frame: [41.0, 41.0],
            scale: [1.0, 1.0],
            par: 1.0,
        };
        let mut src = Buf::solid([0, 0, 41, 41], [0.0, 0.0, 0.0, 1.0]);
        src.data[20 * 41 + 20] = [1.0; 4];
        let run = |angle: f64, amount: f64| {
            let s = Vals::of(
                Kind::LinearBlur.specs(),
                &|spec| match spec.name().to_bytes() {
                    b"angle" => vec![angle],
                    _ => vec![amount],
                },
            );
            Kind::LinearBlur.process(&s, &g, &src, 0.0)
        };
        assert_eq!(run(30.0, 0.0).data, src.data);
        // Across: along the row only.
        let across = run(0.0, 0.1);
        assert!(across.at(24, 20)[0] > 0.005, "{:?}", across.at(24, 20));
        assert!(across.at(20, 24)[0] < 1e-6, "{:?}", across.at(20, 24));
        // 30°: down to the right (rows go up here), not up.
        let turned = run(30.0, 0.3);
        let (down, up) = (turned.at(26, 17)[0], turned.at(26, 23)[0]);
        assert!(down > 10.0 * up.max(1e-6), "{down} {up}");
        assert_eq!(presets(Kind::LinearBlur).len(), 7, "VEGAS's seven");
    }

    /// VEGAS's Radial Blur, worked out by hand on a 4 × 4 frame with
    /// Center at the lower left pixel: Fixed radius steps 1.81 pixels.
    #[test]
    fn radial_blur_mixes_each_pixel_with_the_blur_nearer_center() {
        let g = Grid {
            origin: [0.0, 0.0],
            frame: [4.0, 4.0],
            scale: [1.0, 1.0],
            par: 1.0,
        };
        let mut src = Buf::solid([0, 0, 4, 4], [0.0; 4]);
        for (i, px) in src.data.iter_mut().enumerate() {
            let v = (i % 4) as f32 / 10.0 + (i / 4) as f32 / 100.0;
            *px = [v, v, v, 1.0];
        }
        let s = Vals::of(
            Kind::RadialBlur.specs(),
            &|spec| match spec.name().to_bytes() {
                b"center" => vec![0.0, 0.0],
                b"type" => vec![1.0],
                _ => vec![0.9],
            },
        );
        let out = Kind::RadialBlur.process(&s, &g, &src, 0.0);
        let q = 0.9f32.powf(75.0 / 4.0);
        let at10 = (1.0 - q) * 0.1;
        let at20 = (1.0 - q) * 0.2 + q * (0.19 * at10);
        assert!((out.at(1, 0)[0] - at10).abs() < 1e-5, "{:?}", out.at(1, 0));
        assert!((out.at(2, 0)[0] - at20).abs() < 1e-5, "{:?}", out.at(2, 0));
        // The pixel over Center keeps its own color, as VEGAS's does.
        assert_eq!(out.at(0, 1), src.at(0, 1));
        assert_eq!(out.at(0, 0), src.at(0, 0));
        assert_eq!(presets(Kind::RadialBlur).len(), 6, "VEGAS's six");
    }

    /// VEGAS's Spherize: inside the ellipse each pixel takes the picture
    /// `1 − 0.8 × Amount × cos(r × 90°)` of its way from the middle; its
    /// Center reaches half a frame past each edge.
    #[test]
    fn spherize_pulls_the_picture_as_vegas_does() {
        let g = Grid {
            origin: [0.0, 0.0],
            frame: [40.0, 40.0],
            scale: [1.0, 1.0],
            par: 1.0,
        };
        let mut src = Buf::solid([0, 0, 40, 40], [0.0; 4]);
        for (i, px) in src.data.iter_mut().enumerate() {
            let v = (i % 40) as f32 / 40.0;
            *px = [v, v, v, 1.0];
        }
        let run = |center: [f64; 2]| {
            let s = Vals::of(
                Kind::Spherize.specs(),
                &|spec| match spec.name().to_bytes() {
                    b"center" => center.to_vec(),
                    _ => spec.default(),
                },
            );
            Kind::Spherize.process(&s, &g, &src, 0.0)
        };
        let out = run([0.5, 0.5]);
        let r = 10.0 * 2.0 / 1.0001 / 40.0;
        let f = 1.0 - (r * std::f64::consts::FRAC_PI_2).cos() * 0.8;
        let want = ((20.0 + 10.0 * f) / 40.0) as f32;
        assert!(
            (out.at(30, 20)[0] - want).abs() < 1e-5,
            "{:?} {want}",
            out.at(30, 20)
        );
        assert_eq!(out.at(0, 0), src.at(0, 0));
        // At 0.25 the middle is the left edge: the corner half way up
        // moves, the right edge does not.
        let left = run([0.25, 0.5]);
        assert_ne!(left.at(10, 20), src.at(10, 20));
        assert_eq!(left.at(39, 20), src.at(39, 20));
        assert_eq!(presets(Kind::Spherize).len(), 6, "VEGAS's six");
    }

    /// VEGAS's Offset and Wrap: Offset moves the picture by frames, Count
    /// fits it that many times across, the edges repeated by default.
    #[test]
    fn offset_and_wrap_moves_and_repeats_the_picture_as_vegas_does() {
        let g = grid();
        let mut src = Buf::solid([0, 0, 32, 18], [0.0; 4]);
        for (i, px) in src.data.iter_mut().enumerate() {
            let v = (i % 32) as f32 / 31.0;
            *px = [v, v, v, 1.0];
        }
        let run = |offset: [f64; 2], count: f64| {
            let s = Vals::of(
                Kind::OffsetWrap.specs(),
                &|spec| match spec.name().to_bytes() {
                    b"offset" => offset.to_vec(),
                    b"count" => vec![count],
                    _ => spec.default(),
                },
            );
            Kind::OffsetWrap.process(&s, &g, &src, 0.0)
        };
        // A quarter of the frame to the right: the left quarter is the
        // picture's right end, repeated round.
        let moved = run([0.25, 0.0], 1.0);
        assert_eq!(moved.at(12, 9), src.at(4, 9));
        assert_eq!(moved.at(4, 9), src.at(28, 9));
        // Twice across about the middle: the whole picture in the middle
        // half, its right half repeated on the left.
        let twice = run([0.0, 0.0], 2.0);
        assert!(
            (twice.at(16, 9)[0] - 0.53).abs() < 0.04,
            "{:?}",
            twice.at(16, 9)
        );
        assert!(
            (twice.at(4, 9)[0] - 0.78).abs() < 0.04,
            "{:?}",
            twice.at(4, 9)
        );
    }

    /// VEGAS's Radial Pixelate: at the most, Radial leaves each ring one
    /// color and Circular each ray.
    #[test]
    fn radial_pixelate_makes_rings_or_rays_as_vegas_does() {
        let g = Grid {
            origin: [0.0, 0.0],
            frame: [20.0, 20.0],
            scale: [1.0, 1.0],
            par: 1.0,
        };
        let mut src = Buf::solid([0, 0, 20, 20], [0.0; 4]);
        for (i, px) in src.data.iter_mut().enumerate() {
            let (x, y) = ((i % 20) as f32 / 20.0, (i / 20) as f32 / 20.0);
            *px = [x, y, 0.5, 1.0];
        }
        let run = |kind: f64, strength: f64| {
            let s = Vals::of(
                Kind::RadialPixelate.specs(),
                &|spec| match spec.name().to_bytes() {
                    b"type" => vec![kind],
                    b"strength" => vec![strength],
                    _ => spec.default(),
                },
            );
            Kind::RadialPixelate.process(&s, &g, &src, 0.0)
        };
        assert_eq!(run(0.0, 0.0).data, src.data);
        // Radial: the same distance, the same color, left or right.
        let rings = run(0.0, 0.98);
        assert_eq!(rings.at(5, 10), rings.at(15, 10));
        assert_ne!(rings.at(5, 10), src.at(5, 10));
        // Circular: the same direction, the same color, near or far.
        let rays = run(1.0, 0.98);
        assert_eq!(rays.at(5, 10), rays.at(2, 10));
        assert_eq!(presets(Kind::RadialPixelate).len(), 6, "VEGAS's six");
    }

    /// VEGAS's Tiny Planet: the bottom pole in the middle, the sky round
    /// the edges; the middle pixel is the bottom row's.
    #[test]
    fn tiny_planet_projects_the_sphere_as_vegas_does() {
        let g = Grid {
            origin: [0.0, 0.0],
            frame: [40.0, 20.0],
            scale: [1.0, 1.0],
            par: 1.0,
        };
        // Ground (rows going up: the lower half) green, sky blue.
        let mut src = Buf::solid([0, 0, 40, 20], [0.0; 4]);
        for (i, px) in src.data.iter_mut().enumerate() {
            *px = if i / 40 < 10 {
                [0.0, 1.0, 0.0, 1.0]
            } else {
                [0.0, 0.0, 1.0, 1.0]
            };
        }
        let defaults = Vals::of(Kind::TinyPlanet.specs(), &|spec| spec.default());
        let out = Kind::TinyPlanet.process(&defaults, &g, &src, 0.0);
        assert_eq!(out.at(20, 10), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(out.at(0, 0), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(out.at(39, 19), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(presets(Kind::TinyPlanet).len(), 3, "VEGAS's three");
    }

    /// VEGAS's Quick Blur leaves the picture alone at its default; Blend
    /// amount mixes the blurred one in.
    #[test]
    fn quick_blur_mixes_the_blur_in_by_its_blend_amount() {
        let (g, src) = (grid(), picture());
        let with = |amount: f64| {
            let s = Vals::of(Kind::QuickBlur.specs(), &|_| vec![amount]);
            Kind::QuickBlur.process(&s, &g, &src, 0.0)
        };
        assert_eq!(with(0.0).data, src.data);
        let (half, full) = (with(0.5), with(1.0));
        let (a, b, c) = (src.at(16, 9), half.at(16, 9), full.at(16, 9));
        assert!(c[1] < a[1], "{c:?}");
        for k in 0..4 {
            assert!((b[k] - (a[k] + c[k]) / 2.0).abs() < 1e-6, "{b:?}");
        }
    }

    #[test]
    fn every_effect_and_preset_draws_a_picture() {
        let (g, src) = (grid(), picture());
        for kind in Kind::ALL {
            let mut presets = presets(kind);
            // VEGAS's Denoise has none: its defaults.
            if kind == Kind::Denoise {
                assert!(presets.is_empty());
                presets.push(crate::Preset {
                    name: "(Default)",
                    values: Vec::new(),
                });
            }
            assert!(!presets.is_empty(), "{kind:?}");
            for p in &presets {
                let out = kind.process(&vals(kind, p), &g, &src, 0.0);
                assert_eq!(out.data.len(), src.data.len(), "{kind:?} {}", p.name);
                assert!(
                    out.data.iter().all(|c| c.iter().all(|v| v.is_finite())),
                    "{kind:?} {}",
                    p.name
                );
            }
        }
    }

    #[test]
    fn neutral_settings_leave_the_picture_alone() {
        let (g, src) = (grid(), picture());
        let close = |a: &Buf, b: &Buf| {
            a.data
                .iter()
                .zip(&b.data)
                .all(|(x, y)| x.iter().zip(y).all(|(p, q)| (p - q).abs() < 1e-4))
        };
        // Defaults that change nothing: Offset and Wrap, Channel Blend,
        // Convolution Kernel.
        for kind in [Kind::OffsetWrap, Kind::ChannelBlend, Kind::Convolution] {
            let defaults = Vals::of(kind.specs(), &|spec| spec.default());
            let out = kind.process(&defaults, &g, &src, 0.0);
            assert!(close(&out, &src), "{kind:?}");
        }
        // Channel Blend's swap: red and blue trade places.
        let swap = presets(Kind::ChannelBlend)
            .into_iter()
            .find(|p| p.name == "RGBA -> BGRA")
            .unwrap();
        let out = Kind::ChannelBlend.process(&vals(Kind::ChannelBlend, &swap), &g, &src, 0.0);
        let (a, b) = (src.at(3, 4), out.at(3, 4));
        assert!((a[0] - b[2]).abs() < 1e-5 && (a[2] - b[0]).abs() < 1e-5);
        // Threshold: white where it was bright, black where dark.
        let t = presets(Kind::Threshold).remove(0);
        let out = Kind::Threshold.process(&vals(Kind::Threshold, &t), &g, &src, 0.0);
        assert_eq!(out.at(16, 9), [1.0; 4]);
        // The median takes the lone bright spot away.
        let m = presets(Kind::Median).remove(0);
        let out = Kind::Median.process(&vals(Kind::Median, &m), &g, &src, 0.0);
        assert!(out.at(16, 9)[1] < 0.6, "{:?}", out.at(16, 9));
    }
}
