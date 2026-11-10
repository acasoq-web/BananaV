//! VEGAS's media generators as OpenFX generators written as tables of
//! settings ([`crate::kit`], their parameters named as VEGAS's, so a
//! `.veg`'s meet them): Checkerboard, Color Gradient, Noise Texture, Solid
//! Color and Test Pattern. Each works its picture out as VEGAS's pictures
//! have it — in an 8-bit project its bytes as VEGAS's 8-bit renders show
//! them ([`checkerboard`], [`color_gradient`], [`noise_texture`],
//! [`test_pattern`]) —, a pixel or a row at a time ([`Kind::painter`],
//! [`Kind::row_painter`]), and the card draws it from the same tables
//! ([`Kind::card_plan`]).
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::kit::{Grid, Spec, Vals, define_all, define_generator};
use crate::ofx::{Fx, Props, for_rows, is, suites};
use crate::{Preset, PresetValue};
use crate::{checkerboard, color_gradient, gradient_page, noise_texture, test_pattern};

/// One of the generators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Checkerboard,
    ColorGradient,
    NoiseTexture,
    SolidColor,
    TestPattern,
}

const CHECKERBOARD: &[Spec] = &[
    Spec::Group(c"tileDimensions", c"Tile dimensions"),
    Spec::Number(
        c"tileDimensionsWidth",
        c"Width",
        0.1,
        0.001,
        1.0,
        c"A tile's width, a part of the frame's width.",
    ),
    Spec::Number(
        c"tileDimensionsHeight",
        c"Height",
        0.1,
        0.001,
        1.0,
        c"A tile's height, a part of the frame's height (unless Square).",
    ),
    Spec::Toggle(
        c"tileDimensionsSquare",
        c"Square",
        true,
        c"The tiles square: the height follows the width.",
    ),
    Spec::GroupEnd,
    Spec::Group(c"edgeBlending", c"Edge blending"),
    Spec::Number(
        c"edgeBlendingHorizontal",
        c"Horizontal",
        0.0,
        0.0,
        1.0,
        c"The tiles' left and right edges blended: 1 across the whole tile.",
    ),
    Spec::Number(
        c"edgeBlendingVertical",
        c"Vertical",
        0.0,
        0.0,
        1.0,
        c"The tiles' top and bottom edges blended (unless Proportional).",
    ),
    Spec::Toggle(
        c"edgeBlendingProportional",
        c"Proportional",
        true,
        c"The vertical blending follows the horizontal.",
    ),
    Spec::GroupEnd,
    Spec::Color(
        c"color1",
        c"Color 1",
        [0.0, 0.0, 0.0, 1.0],
        c"The first tiles' color.",
    ),
    Spec::Color(
        c"color2",
        c"Color 2",
        [1.0, 1.0, 1.0, 1.0],
        c"The other tiles' color.",
    ),
    Spec::Group(c"gridOffset", c"Grid offset"),
    Spec::Number(
        c"gridOffsetX",
        c"X",
        0.0,
        0.0,
        1.0,
        c"The grid moved across: 1 is the frame's width.",
    ),
    Spec::Number(
        c"gridOffsetY",
        c"Y",
        0.0,
        0.0,
        1.0,
        c"The grid moved down: 1 is the frame's height.",
    ),
    Spec::GroupEnd,
];
const COLOR_GRADIENT: &[Spec] = &[
    Spec::Choice(
        c"gradientType",
        c"Gradient Type",
        &[c"Linear", c"Elliptical", c"Rectangular"],
        0,
        c"The gradient's shape.",
    ),
    Spec::Angle(
        c"aspectRatioAngle",
        c"Aspect Ratio Angle",
        0.0,
        c"The angle of the line the control points lie on.",
    ),
    Spec::Point(
        c"center",
        c"Center",
        [0.5, 0.5],
        c"The gradient's center: 0, 0 is the lower left corner.",
    ),
    Spec::Int(
        c"controlPoints",
        c"Control Points",
        2,
        1,
        9,
        c"How many control points the gradient has.",
    ),
    Spec::Number(
        c"distance1",
        c"Distance",
        -0.5,
        -1.0,
        1.0,
        c"How far the point is from the center along that line: 1 is the frame's size.",
    ),
    Spec::Color(
        c"color1",
        c"Color",
        [1.0, 1.0, 1.0, 1.0],
        c"The point's color.",
    ),
    Spec::Number(
        c"distance2",
        c"Distance",
        0.5,
        -1.0,
        1.0,
        c"How far the point is from the center along that line: 1 is the frame's size.",
    ),
    Spec::Color(
        c"color2",
        c"Color",
        [0.0, 0.0, 0.0, 1.0],
        c"The point's color.",
    ),
    Spec::Number(
        c"distance3",
        c"Distance",
        0.0,
        -1.0,
        1.0,
        c"How far the point is from the center along that line: 1 is the frame's size.",
    ),
    Spec::Color(
        c"color3",
        c"Color",
        [1.0, 1.0, 1.0, 1.0],
        c"The point's color.",
    ),
    Spec::Number(
        c"distance4",
        c"Distance",
        0.0,
        -1.0,
        1.0,
        c"How far the point is from the center along that line: 1 is the frame's size.",
    ),
    Spec::Color(
        c"color4",
        c"Color",
        [1.0, 1.0, 1.0, 1.0],
        c"The point's color.",
    ),
    Spec::Number(
        c"distance5",
        c"Distance",
        0.0,
        -1.0,
        1.0,
        c"How far the point is from the center along that line: 1 is the frame's size.",
    ),
    Spec::Color(
        c"color5",
        c"Color",
        [1.0, 1.0, 1.0, 1.0],
        c"The point's color.",
    ),
    Spec::Number(
        c"distance6",
        c"Distance",
        0.0,
        -1.0,
        1.0,
        c"How far the point is from the center along that line: 1 is the frame's size.",
    ),
    Spec::Color(
        c"color6",
        c"Color",
        [1.0, 1.0, 1.0, 1.0],
        c"The point's color.",
    ),
    Spec::Number(
        c"distance7",
        c"Distance",
        0.0,
        -1.0,
        1.0,
        c"How far the point is from the center along that line: 1 is the frame's size.",
    ),
    Spec::Color(
        c"color7",
        c"Color",
        [1.0, 1.0, 1.0, 1.0],
        c"The point's color.",
    ),
    Spec::Number(
        c"distance8",
        c"Distance",
        0.0,
        -1.0,
        1.0,
        c"How far the point is from the center along that line: 1 is the frame's size.",
    ),
    Spec::Color(
        c"color8",
        c"Color",
        [1.0, 1.0, 1.0, 1.0],
        c"The point's color.",
    ),
    Spec::Number(
        c"distance9",
        c"Distance",
        0.0,
        -1.0,
        1.0,
        c"How far the point is from the center along that line: 1 is the frame's size.",
    ),
    Spec::Color(
        c"color9",
        c"Color",
        [1.0, 1.0, 1.0, 1.0],
        c"The point's color.",
    ),
];
const NOISE_TEXTURE: &[Spec] = &[
    Spec::Choice(
        c"noiseStyle",
        c"Noise style",
        &[
            c"Fractal - smooth",
            c"Fractal - turbulent",
            c"Marble - light grain",
            c"Marble - heavy grain",
            c"Jagged lines",
            c"Contours",
            c"Rings",
        ],
        0,
        c"How the noise is shaped: clouds, smoke, veins of marble, lines, contours or rings.",
    ),
    Spec::Number(
        c"numberOfLayers",
        c"Number of layers",
        1.0,
        1.0,
        5.0,
        c"How many layers of finer noise, each twice as fine; a part of one fades it in.",
    ),
    Spec::Toggle(
        c"swapcolors",
        c"Swap colors",
        false,
        c"Color 1 and Color 2 change places.",
    ),
    Spec::Color(
        c"color1",
        c"Color 1",
        [0.0, 0.0, 0.0, 1.0],
        c"The color of the noise's low values.",
    ),
    Spec::Color(
        c"color2",
        c"Color 2",
        [1.0, 1.0, 1.0, 1.0],
        c"The color of the noise's high values.",
    ),
    Spec::Group(c"frequency", c"Frequency"),
    Spec::Number(
        c"frequencyX",
        c"X",
        10.0,
        0.001,
        30.0,
        c"How many ripples across.",
    ),
    Spec::Number(
        c"frequencyY",
        c"Y",
        10.0,
        0.001,
        30.0,
        c"How many ripples up and down.",
    ),
    Spec::GroupEnd,
    Spec::Group(c"offset", c"Offset"),
    Spec::Number(
        c"offsetX",
        c"X",
        0.0,
        -5.0,
        5.0,
        c"The pattern moved across.",
    ),
    Spec::Number(
        c"offsetY",
        c"Y",
        0.0,
        -5.0,
        5.0,
        c"The pattern moved up and down.",
    ),
    Spec::Number(
        c"offsetprogressinDegrees",
        c"Progress (in degrees)",
        0.0,
        0.0,
        10.0,
        c"The pattern moved through its depth: animate it and the noise flows.",
    ),
    Spec::GroupEnd,
    Spec::Group(c"noiseParameters", c"Noise parameters"),
    Spec::Number(
        c"noiseParametersMin",
        c"Min",
        0.0,
        0.0,
        1.0,
        c"The noise's value that takes Color 1.",
    ),
    Spec::Number(
        c"noiseParametersMax",
        c"Max",
        1.0,
        0.0,
        1.0,
        c"The noise's value that takes Color 2.",
    ),
    Spec::Number(
        c"noiseParametersBias",
        c"Bias",
        0.0,
        -1.0,
        1.0,
        c"Added to the noise: below 0 toward Color 1, above toward Color 2.",
    ),
    Spec::Number(
        c"noiseParametersAmplitude",
        c"Amplitude",
        1.0,
        0.0,
        10.0,
        c"The noise multiplied: more contrast between the colors.",
    ),
    Spec::Number(
        c"noiseParametersGrainAmplitudeFalloff",
        c"Grain (amplitude falloff)",
        0.5,
        0.0,
        1.0,
        c"How strong each finer layer is next to the one before.",
    ),
    Spec::GroupEnd,
];
const SOLID_COLOR: &[Spec] = &[Spec::Color(
    c"color",
    c"Color",
    [0.0, 0.0, 0.0, 0.0],
    c"The frame's color; its alpha how opaque it is.",
)];
const TEST_PATTERN: &[Spec] = &[Spec::Choice(
    c"testPatternType",
    c"Test pattern type",
    &[
        c"Color bars (NTSC)",
        c"Color bars (PAL)",
        c"Gradient",
        c"Pluge and porches",
        c"Ramp",
        c"SMPTE bars (NTSC)",
        c"SMPTE bars (PAL)",
        c"White porches",
        c"Yellow porches",
    ],
    0,
    c"The pattern: color bars, a gradient, a gray ramp, or the black and white references.",
)];

/// How a generator's picture is drawn: in an 8-bit project's bytes (as
/// VEGAS's 8-bit pictures have them) or in floats, and the shape of the
/// generated media's pixels (width over height).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Look {
    pub eight_bit: bool,
    pub pixel_aspect: f64,
}

impl Look {
    /// Floats, square pixels.
    #[cfg(test)]
    pub(crate) const FLOAT: Look = Look {
        eight_bit: false,
        pixel_aspect: 1.0,
    };
}

impl Kind {
    pub(crate) const ALL: [Kind; 5] = [
        Kind::Checkerboard,
        Kind::ColorGradient,
        Kind::NoiseTexture,
        Kind::SolidColor,
        Kind::TestPattern,
    ];

    pub(crate) fn id(self) -> &'static CStr {
        match self {
            Kind::Checkerboard => c"org.bananav.Checkerboard",
            Kind::ColorGradient => c"org.bananav.ColorGradient",
            Kind::NoiseTexture => c"org.bananav.NoiseTexture",
            Kind::SolidColor => c"org.bananav.SolidColor",
            Kind::TestPattern => c"org.bananav.TestPattern",
        }
    }

    fn label(self) -> &'static CStr {
        match self {
            Kind::Checkerboard => c"Checkerboard",
            Kind::ColorGradient => c"Color Gradient",
            Kind::NoiseTexture => c"Noise Texture",
            Kind::SolidColor => c"Solid Color",
            Kind::TestPattern => c"Test Pattern",
        }
    }

    fn description(self) -> &'static CStr {
        match self {
            Kind::Checkerboard => c"Checked and striped patterns: tiles of two colors, their edges sharp or blended.",
            Kind::ColorGradient => c"Colors blending into each other from control points: straight across, around a center or in rectangles.",
            Kind::NoiseTexture => c"Textures from noise: clouds, lava, metal, wood, marble.",
            Kind::SolidColor => c"The whole frame in one color.",
            Kind::TestPattern => c"Color bars and other patterns for checking the picture.",
        }
    }

    pub(crate) fn specs(self) -> &'static [Spec] {
        match self {
            Kind::Checkerboard => CHECKERBOARD,
            Kind::ColorGradient => COLOR_GRADIENT,
            Kind::NoiseTexture => NOISE_TEXTURE,
            Kind::SolidColor => SOLID_COLOR,
            Kind::TestPattern => TEST_PATTERN,
        }
    }

    /// The kind whose identifier is `id`.
    pub(crate) fn of(id: &str) -> Option<Kind> {
        Kind::ALL
            .into_iter()
            .find(|k| k.id().to_str().ok() == Some(id))
    }

    /// What the card draws for this generator with settings `s` on the
    /// picture `g` places, drawn as `look` says; `None` where the
    /// processor still draws it.
    pub(crate) fn card_plan(self, s: &Vals, g: &Grid, look: Look) -> Option<crate::accel::Plan> {
        use crate::accel::{GenerateKernel, Plan};
        match self {
            Kind::Checkerboard => {
                let (w, h) = picture_size(g);
                let board = checkerboard::Board::new(
                    &checker_settings(s),
                    w,
                    h,
                    look.pixel_aspect,
                    look.eight_bit,
                );
                Some(Plan::Generate {
                    kernel: GenerateKernel::Checkerboard,
                    f: Vec::new(),
                    j: [u32::from(look.eight_bit), 0, 0, 0],
                    table: board.table(),
                })
            }
            Kind::NoiseTexture => {
                let (w, h) = picture_size(g);
                let (f, j, table) =
                    noise_texture::Noise::new(&noise_settings(s), w, h, look.eight_bit).plan();
                Some(Plan::Generate {
                    kernel: GenerateKernel::NoiseTexture,
                    f,
                    j,
                    table,
                })
            }
            Kind::SolidColor => Some(Plan::Generate {
                kernel: GenerateKernel::SolidColor,
                f: solid(s.rgba64(c"color"), look.eight_bit).to_vec(),
                j: [0; 4],
                table: Vec::new(),
            }),
            Kind::ColorGradient => {
                let (w, h) = picture_size(g);
                let (f, j, table) =
                    color_gradient::Gradient::new(&gradient_settings(s), w, h, look.eight_bit)
                        .plan();
                Some(Plan::Generate {
                    kernel: GenerateKernel::ColorGradient,
                    f,
                    j,
                    table,
                })
            }
            Kind::TestPattern => {
                let (w, h) = picture_size(g);
                Some(Plan::Generate {
                    kernel: GenerateKernel::TestPattern,
                    f: Vec::new(),
                    j: [0; 4],
                    table: test_pattern::Pattern::new(pattern_kind(s), w, h).table(),
                })
            }
        }
    }

    /// The rows this generator works out whole (Noise Texture's and Color
    /// Gradient's, whose pixels go on from one another): the picture's
    /// size and a row's pixels by its place from the top.
    #[allow(clippy::type_complexity)]
    pub(crate) fn row_painter(
        self,
        s: &Vals,
        g: &Grid,
        look: Look,
    ) -> Option<(usize, usize, Box<dyn Fn(usize) -> Vec<[f32; 4]> + Sync>)> {
        match self {
            Kind::NoiseTexture => {
                let (w, h) = picture_size(g);
                let noise = noise_texture::Noise::new(&noise_settings(s), w, h, look.eight_bit);
                Some((w, h, Box::new(move |y| noise.row(y))))
            }
            Kind::ColorGradient => {
                let (w, h) = picture_size(g);
                let gradient =
                    color_gradient::Gradient::new(&gradient_settings(s), w, h, look.eight_bit);
                Some((w, h, Box::new(move |y| gradient.row(y))))
            }
            _ => None,
        }
    }

    /// What colors each place of the frame (u across, v down, 0–1) at
    /// settings `s` on the picture `g` places, drawn as `look` says:
    /// premultiplied values of the project's (in an 8-bit one, bytes'
    /// values as VEGAS's 8-bit pictures have them).
    pub(crate) fn painter(
        self,
        s: &Vals,
        g: &Grid,
        look: Look,
    ) -> Box<dyn Fn(f64, f64) -> [f32; 4] + Sync> {
        match self {
            Kind::Checkerboard => {
                let (w, h) = picture_size(g);
                let board = checkerboard::Board::new(
                    &checker_settings(s),
                    w,
                    h,
                    look.pixel_aspect,
                    look.eight_bit,
                );
                Box::new(move |u, v| board.at(pixel_at(u, w), pixel_at(v, h)))
            }
            Kind::ColorGradient => {
                let (w, h) = picture_size(g);
                let gradient =
                    color_gradient::Gradient::new(&gradient_settings(s), w, h, look.eight_bit);
                Box::new(move |u, v| gradient.row(pixel_at(v, h))[pixel_at(u, w)])
            }
            Kind::NoiseTexture => {
                let (w, h) = picture_size(g);
                let noise = noise_texture::Noise::new(&noise_settings(s), w, h, look.eight_bit);
                Box::new(move |u, v| noise.row(pixel_at(v, h))[pixel_at(u, w)])
            }
            Kind::SolidColor => {
                let c = solid(s.rgba64(c"color"), look.eight_bit);
                Box::new(move |_, _| c)
            }
            Kind::TestPattern => {
                let (w, h) = picture_size(g);
                let pattern = test_pattern::Pattern::new(pattern_kind(s), w, h);
                Box::new(move |u, v| pattern.at(pixel_at(u, w), pixel_at(v, h)))
            }
        }
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
    checkerboard_entry => Kind::Checkerboard;
    color_gradient_entry => Kind::ColorGradient;
    noise_texture_entry => Kind::NoiseTexture;
    solid_color_entry => Kind::SolidColor;
    test_pattern_entry => Kind::TestPattern;
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
        fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);
        if kind == Kind::ColorGradient {
            // Its type and angle, then VEGAS's Control Points page.
            define_all(fx, &COLOR_GRADIENT[..2]);
            gradient_page::define_page(fx, &COLOR_GRADIENT[2..]);
        } else {
            // The groups too, as sections (Checkerboard's, Noise Texture's),
            // closed at first as VEGAS's.
            define_generator(fx, kind.specs());
        }
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        render(kind, fx, args)
    } else if is(action, kOfxImageEffectActionGetRegionOfDefinition) {
        let (origin, size) = fx.frame();
        out.set_dbls(
            kOfxImageEffectPropRegionOfDefinition,
            &[
                origin[0],
                origin[1],
                origin[0] + size[0],
                origin[1] + size[1],
            ],
        );
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionGetClipPreferences) {
        out.set_int(kOfxImageEffectFrameVarying, 1);
        out.set_str(kOfxImageClipPropFieldOrder, kOfxImageFieldNone);
        out.set_str(kOfxImageEffectPropPreMultiplication, kOfxImagePreMultiplied);
        kOfxStatOK
    } else if is(action, kOfxActionInstanceChanged) {
        if kind == Kind::Checkerboard {
            let by_hand = args.is(kOfxPropType, kOfxTypeParameter)
                && args.is(kOfxPropChangeReason, kOfxChangeUserEdited);
            let changed = |n: &CStr| args.is(kOfxPropName, n);
            let changed: Option<&dyn Fn(&CStr) -> bool> =
                if by_hand { Some(&changed) } else { None };
            checker_links(fx, args.dbl(kOfxPropTime, 0), changed);
        }
        if kind == Kind::ColorGradient {
            let time = args.dbl(kOfxPropTime, 0);
            // A button of the page's bar, or anything else.
            let button = args.is(kOfxPropType, kOfxTypeParameter)
                && gradient_page::BAR_NAMES
                    .iter()
                    .find(|n| args.is(kOfxPropName, n))
                    .is_some_and(|n| gradient_page::pressed(fx, time, n));
            if !button {
                gradient_page::shown(fx, time);
            }
        }
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) || is(action, kOfxActionBeginInstanceEdit) {
        if kind == Kind::Checkerboard {
            checker_links(fx, 0.0, None);
        }
        if kind == Kind::ColorGradient {
            gradient_page::created(fx);
            gradient_page::shown(fx, 0.0);
        }
        kOfxStatOK
    } else if is(action, kOfxActionDestroyInstance) {
        if kind == Kind::ColorGradient {
            gradient_page::destroyed(fx);
        }
        kOfxStatOK
    } else if is(action, kOfxActionLoad) || is(action, kOfxActionUnload) {
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

fn describe(kind: Kind, fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, kind.label());
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Generators");
    p.set_str(kOfxPropPluginDescription, kind.description());
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[
            kOfxImageEffectContextGenerator,
            kOfxImageEffectContextGeneral,
        ],
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

fn render(kind: Kind, fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|v| if v > 0.0 { v } else { 1.0 });
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let (origin, frame) = fx.frame();
    let grid = Grid {
        origin,
        frame,
        scale,
        par: if out.par > 0.0 { out.par } else { 1.0 },
    };
    let vals = Vals::of(kind.specs(), &|s| s.read(fx, time));
    let look = Look {
        eight_bit: args.int(bv_ofx_sys::extensions::kBananaVImageEffectPropEightBit, 0) != 0,
        pixel_aspect: args.dbl_or(
            bv_ofx_sys::extensions::kBananaVImageEffectPropMediaPixelAspect,
            1.0,
        ),
    };
    let straight = out.channels == 4 && out.unpremultiplied;
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let put = |row: *mut u8, x: c_int, mut c: [f32; 4]| {
        if straight && c[3] > 0.0 {
            c = [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]];
        }
        // SAFETY: a row the output has, a column inside its bounds.
        unsafe { crate::colors::put_pixel(&out, row, x, c) };
    };
    let rows = y1.max(out.bounds[1])..y2.min(out.bounds[3]);
    if let Some((w, h, paint_row)) = kind.row_painter(&vals, &grid, look) {
        // A row of the picture at a time.
        let fill = |y: c_int| {
            let Some(row) = out.row(y) else { return };
            let line = paint_row(pixel_at(grid.uv(x1, y).1, h));
            for x in columns.clone() {
                put(row, x, line[pixel_at(grid.uv(x, y).0, w)]);
            }
        };
        for_rows(rows, &fill);
    } else {
        let paint = kind.painter(&vals, &grid, look);
        let fill = |y: c_int| {
            let Some(row) = out.row(y) else { return };
            for x in columns.clone() {
                let (u, v) = grid.uv(x, y);
                put(row, x, paint(u, v));
            }
        };
        for_rows(rows, &fill);
    }
    drop(out);
    kOfxStatOK
}

// The pictures.

/// The frame's width in this picture's pixels.
fn width_px(g: &Grid) -> f64 {
    (g.frame[0] * g.scale[0] / g.par).max(1.0)
}

/// VEGAS's Solid Color: the color premultiplied, everywhere — in an 8-bit
/// project each byte cut as VEGAS's 8-bit pictures have it, (c·255)·a and
/// a·255 in doubles.
fn solid(c: [f64; 4], eight_bit: bool) -> [f32; 4] {
    let a = c[3];
    if eight_bit {
        let byte = |v: f64| (v.floor().clamp(0.0, 255.0) / 255.0) as f32;
        [
            byte(c[0] * 255.0 * a),
            byte(c[1] * 255.0 * a),
            byte(c[2] * 255.0 * a),
            byte(a * 255.0),
        ]
    } else {
        [
            (c[0] * a) as f32,
            (c[1] * a) as f32,
            (c[2] * a) as f32,
            a as f32,
        ]
    }
}

/// The picture's size in its pixels.
fn picture_size(g: &Grid) -> (usize, usize) {
    (
        (width_px(g).round() as usize).max(1),
        (g.height_px().round() as usize).max(1),
    )
}

/// Of the `n` pixels across (or down), the one whose middle is at `t`
/// (0–1).
fn pixel_at(t: f64, n: usize) -> usize {
    ((t * n as f64).floor().max(0.0) as usize).min(n.max(1) - 1)
}

/// Test Pattern's pattern: its place in the list.
fn pattern_kind(s: &Vals) -> u32 {
    s.i(c"testPatternType").clamp(0, 8) as u32
}

/// Color Gradient's settings ([`color_gradient`]): the center from the
/// top, the points VEGAS's way.
fn gradient_settings(s: &Vals) -> color_gradient::Settings {
    let n = s.i(c"controlPoints").clamp(1, 9) as usize;
    let center = s.point(c"center");
    color_gradient::Settings {
        kind: s.i(c"gradientType").clamp(0, 2) as u32,
        angle: s.f(c"aspectRatioAngle").to_radians(),
        center: [center[0], 1.0 - center[1]],
        points: gradient_page::POINTS[..n]
            .iter()
            .map(|(d, c)| (s.f(d), s.rgba64(c).map(|v| v as f32)))
            .collect(),
    }
}

/// Noise Texture's settings ([`noise_texture`]).
fn noise_settings(s: &Vals) -> noise_texture::Settings {
    noise_texture::Settings {
        style: s.i(c"noiseStyle").clamp(0, 6) as usize,
        layers: s.f(c"numberOfLayers").clamp(1.0, 5.0),
        swap: s.b(c"swapcolors"),
        colors: [s.rgba64(c"color1"), s.rgba64(c"color2")],
        frequency: [s.f(c"frequencyX"), s.f(c"frequencyY")],
        offset: [s.f(c"offsetX"), s.f(c"offsetY")],
        progress: s.f(c"offsetprogressinDegrees"),
        min: s.f(c"noiseParametersMin"),
        max: s.f(c"noiseParametersMax"),
        bias: s.f(c"noiseParametersBias"),
        amplitude: s.f(c"noiseParametersAmplitude"),
        falloff: s.f(c"noiseParametersGrainAmplitudeFalloff"),
    }
}

/// Checkerboard's settings ([`checkerboard`]).
fn checker_settings(s: &Vals) -> checkerboard::Settings {
    checkerboard::Settings {
        tile: [
            s.f(c"tileDimensionsWidth").clamp(0.001, 1.0),
            s.f(c"tileDimensionsHeight").clamp(0.001, 1.0),
        ],
        square: s.b(c"tileDimensionsSquare"),
        blend: [
            s.f(c"edgeBlendingHorizontal").clamp(0.0, 1.0),
            s.f(c"edgeBlendingVertical").clamp(0.0, 1.0),
        ],
        proportional: s.b(c"edgeBlendingProportional"),
        colors: [s.rgba64(c"color1"), s.rgba64(c"color2")],
        offset: [s.f(c"gridOffsetX"), s.f(c"gridOffsetY")],
    }
}

/// Checkerboard's links, VEGAS's: with Square the height is the width's,
/// with Proportional the vertical blending the horizontal's — greyed
/// out, and set when the user changes either (`changed`; none: only
/// greyed).
fn checker_links(fx: Fx, time: f64, changed: Option<&dyn Fn(&CStr) -> bool>) {
    for (flag, from, to) in [
        (
            c"tileDimensionsSquare",
            c"tileDimensionsWidth",
            c"tileDimensionsHeight",
        ),
        (
            c"edgeBlendingProportional",
            c"edgeBlendingHorizontal",
            c"edgeBlendingVertical",
        ),
    ] {
        let linked = fx.param(flag).int_at(time) != 0;
        if let Some(changed) = changed
            && linked
            && (changed(flag) || changed(from))
        {
            let v = fx.param(from).f64_at(time);
            if fx.param(to).f64_at(time) != v {
                fx.param(to).set_f64(v);
            }
        }
        fx.param(to).set_enabled(!linked);
    }
}

/// The factory presets of generator `kind`.
pub(crate) fn presets(kind: Kind) -> Vec<Preset> {
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    let p = |name: &'static str, values: Vec<(&'static str, PresetValue)>| Preset { name, values };
    let one = |name: &'static str, param: &'static str, v: f64| p(name, vec![(param, num(&[v]))]);
    // At its defaults first, as VEGAS lists a generator none of whose
    // presets is its default (Checkerboard, Test Pattern).
    let defaults = || p("(Default)", vec![]);
    match kind {
        Kind::Checkerboard => {
            let m = |name: &'static str,
                     size: [f64; 2],
                     square: bool,
                     blend: [f64; 2],
                     proportional: bool,
                     colors: [[f64; 4]; 2],
                     offset: [f64; 2]| {
                let flag = |b: bool| num(&[f64::from(u8::from(b))]);
                p(
                    name,
                    vec![
                        ("tileDimensionsWidth", num(&[size[0]])),
                        ("tileDimensionsHeight", num(&[size[1]])),
                        ("tileDimensionsSquare", flag(square)),
                        ("edgeBlendingHorizontal", num(&[blend[0]])),
                        ("edgeBlendingVertical", num(&[blend[1]])),
                        ("edgeBlendingProportional", flag(proportional)),
                        ("color1", num(&colors[0])),
                        ("color2", num(&colors[1])),
                        ("gridOffsetX", num(&[offset[0]])),
                        ("gridOffsetY", num(&[offset[1]])),
                    ],
                )
            };
            const BLACK: [f64; 4] = [0.0, 0.0, 0.0, 1.0];
            const WHITE: [f64; 4] = [1.0, 1.0, 1.0, 1.0];
            const CLEAR: [f64; 4] = [0.0, 0.0, 0.0, 0.0];
            vec![
                defaults(),
                m(
                    "Large Tiles",
                    [0.1, 0.1],
                    true,
                    [0.0, 0.0],
                    true,
                    [BLACK, WHITE],
                    [0.0, 0.0],
                ),
                m(
                    "Small Tiles",
                    [0.03, 0.03],
                    true,
                    [0.0, 0.0],
                    true,
                    [BLACK, WHITE],
                    [0.0, 0.0],
                ),
                m(
                    "Horizontal Blinds",
                    [1.0, 0.07],
                    false,
                    [0.0, 0.5],
                    false,
                    [CLEAR, [1.0, 0.701961, 0.6, 1.0]],
                    [0.0, 0.0],
                ),
                m(
                    "Vertical Blinds",
                    [0.07, 1.0],
                    false,
                    [0.3, 0.0],
                    false,
                    [CLEAR, [1.0, 1.0, 0.8, 1.0]],
                    [0.47, 0.0],
                ),
                m(
                    "Grate",
                    [0.1, 0.04],
                    false,
                    [0.3, 0.7],
                    false,
                    [[0.0, 0.0, 0.0, 0.4], [0.2, 0.2, 0.2, 1.0]],
                    [0.0, 0.0],
                ),
                m(
                    "Fence",
                    [0.13, 0.13],
                    true,
                    [1.0, 1.0],
                    true,
                    [[0.501961, 0.501961, 0.501961, 1.0], CLEAR],
                    [0.0, 0.0],
                ),
                m(
                    "Ridges",
                    [1.0, 0.05],
                    false,
                    [1.0, 0.5],
                    false,
                    [[1.0, 1.0, 1.0, 0.5], [0.0, 0.0, 0.0, 0.5]],
                    [0.0, 0.0],
                ),
                m(
                    "Bumps",
                    [0.04, 0.03],
                    false,
                    [1.0, 1.0],
                    false,
                    [
                        [0.0, 0.101961, 0.301961, 1.0],
                        [0.701961, 0.701961, 0.701961, 1.0],
                    ],
                    [0.0, 0.0],
                ),
                m(
                    "Plaid",
                    [0.07, 0.07],
                    true,
                    [0.4, 0.4],
                    true,
                    [[1.0, 0.0, 0.0, 1.0], [0.45098, 0.25098, 0.0, 1.0]],
                    [0.0, 0.0],
                ),
                m(
                    "Letterhead",
                    [1.0, 1.0],
                    false,
                    [0.0, 0.0],
                    false,
                    [WHITE, [0.0, 0.0, 1.0, 1.0]],
                    [0.15, 0.25],
                ),
                m(
                    "Split Screen",
                    [0.5, 1.0],
                    false,
                    [0.0, 0.0],
                    true,
                    [WHITE, BLACK],
                    [0.0, 0.0],
                ),
                m(
                    "Horizon",
                    [1.0, 1.0],
                    false,
                    [0.0, 0.1],
                    false,
                    [[0.2, 0.8, 0.0, 1.0], [0.67, 1.0, 1.0, 1.0]],
                    [0.0, 0.5],
                ),
            ]
        }
        Kind::ColorGradient => {
            // VEGAS's eighteen: the type, the angle (degrees) and each
            // point's distance along it and color.
            let g = |name: &'static str, kind: u32, angle: f64, points: &[(f64, [f64; 4])]| {
                let mut values = vec![
                    ("gradientType", num(&[f64::from(kind)])),
                    ("aspectRatioAngle", num(&[angle])),
                    ("center", num(&[0.5, 0.5])),
                    ("controlPoints", num(&[points.len() as f64])),
                ];
                for (&(d, c), &(r, color)) in gradient_page::POINTS.iter().zip(points) {
                    let (Ok(d), Ok(c)) = (d.to_str(), c.to_str()) else {
                        continue;
                    };
                    values.push((d, num(&[r])));
                    values.push((c, num(&color)));
                }
                p(name, values)
            };
            vec![
                g(
                    "Linear White to Black",
                    0,
                    0.0,
                    &[(-0.5, [1.0, 1.0, 1.0, 1.0]), (0.5, [0.0, 0.0, 0.0, 1.0])],
                ),
                g(
                    "Linear Black to Transparent",
                    0,
                    0.0,
                    &[(-0.5, [0.0, 0.0, 0.0, 1.0]), (0.5, [0.0, 0.0, 0.0, 0.0])],
                ),
                g(
                    "Linear Red, Green and Blue",
                    0,
                    44.998672835087035,
                    &[
                        (-0.49497474683058323, [0.0, 0.0, 1.0, 1.0]),
                        (0.0, [0.0, 1.0, 0.0, 1.0]),
                        (0.49497474683058323, [1.0, 0.0, 0.0, 1.0]),
                    ],
                ),
                g(
                    "Elliptical White to Black",
                    1,
                    44.998672835087035,
                    &[
                        (0.0, [1.0, 1.0, 1.0, 1.0]),
                        (0.49497474683058323, [0.0, 0.0, 0.0, 1.0]),
                    ],
                ),
                g(
                    "Elliptical Transparent to Black",
                    1,
                    44.998672835087035,
                    &[
                        (0.28284271247461895, [0.0, 0.0, 0.0, 0.0]),
                        (0.49497474683058323, [0.0, 0.0, 0.0, 1.0]),
                    ],
                ),
                g(
                    "Elliptical Ring",
                    1,
                    44.998672835087035,
                    &[
                        (0.3535533905932738, [0.0, 0.0, 0.0, 0.0]),
                        (0.42426406871192857, [1.0, 1.0, 0.0, 1.0]),
                        (0.49497474683058323, [0.0, 0.0, 0.0, 0.0]),
                    ],
                ),
                g(
                    "Rectangular Transparent to Black",
                    2,
                    44.998672835087035,
                    &[
                        (0.28284271247461895, [0.0, 0.0, 0.0, 0.0]),
                        (0.49497474683058323, [0.0, 0.0, 0.0, 1.0]),
                    ],
                ),
                g(
                    "Linear Red to Yellow",
                    0,
                    89.99734567017407,
                    &[(-0.5, [1.0, 0.0, 0.0, 1.0]), (0.5, [1.0, 1.0, 0.0, 1.0])],
                ),
                g(
                    "Linear, Blue-Green Title Bar",
                    0,
                    89.99734567017407,
                    &[
                        (0.23099999999999998, [0.10000000149011612, 0.0, 0.5, 1.0]),
                        (0.266, [0.0, 1.0, 0.0, 1.0]),
                        (0.5, [0.0, 0.25, 0.0, 1.0]),
                    ],
                ),
                g(
                    "Skyscape",
                    0,
                    -269.99203701052215,
                    &[
                        (-0.5, [0.11699999868869781, 0.1899999976158142, 1.0, 1.0]),
                        (0.5, [0.0, 1.0, 0.0, 1.0]),
                        (
                            -0.03799998760223389,
                            [0.9399999976158142, 0.9900000095367432, 1.0, 1.0],
                        ),
                        (
                            -0.025999993085861206,
                            [0.8799999952316284, 0.5550000071525574, 0.0, 1.0],
                        ),
                    ],
                ),
                g(
                    "Desert",
                    0,
                    -269.99203701052215,
                    &[
                        (-0.5, [0.0, 0.5640000104904175, 1.0, 1.0]),
                        (0.5, [1.0, 1.0, 1.0, 1.0]),
                        (
                            -0.03799998760223389,
                            [0.9399999976158142, 0.9900000095367432, 1.0, 1.0],
                        ),
                        (
                            -0.025999993085861206,
                            [0.5899999737739563, 0.4269999861717224, 0.0, 1.0],
                        ),
                        (
                            0.10799998044967651,
                            [0.8799999952316284, 0.6399999856948853, 0.0, 1.0],
                        ),
                    ],
                ),
                g(
                    "Soft-Green Backdrop",
                    0,
                    89.99734567017407,
                    &[
                        (0.5, [0.0, 0.6470000147819519, 0.46000000834465027, 1.0]),
                        (-0.5, [0.9399999976158142, 0.9900000095367432, 1.0, 1.0]),
                    ],
                ),
                g(
                    "Soft-Blue Backdrop",
                    0,
                    89.99734567017407,
                    &[
                        (0.5, [0.0, 0.5049999952316284, 0.8199999928474426, 1.0]),
                        (-0.5, [0.9399999976158142, 0.9900000095367432, 1.0, 1.0]),
                    ],
                ),
                g(
                    "Soft-Red Backdrop",
                    0,
                    89.99734567017407,
                    &[
                        (0.5, [0.8799999952316284, 0.17000000178813934, 0.0, 1.0]),
                        (-0.5, [0.9399999976158142, 0.9900000095367432, 1.0, 1.0]),
                    ],
                ),
                g(
                    "Sunburst",
                    1,
                    45.26366581533504,
                    &[
                        (0.022627397779014773, [1.0, 1.0, 0.0, 1.0]),
                        (
                            0.5472368798397161,
                            [1.0, 0.3779999911785126, 0.05999999865889549, 1.0],
                        ),
                    ],
                ),
                g(
                    "Blue Movie Border",
                    2,
                    44.23234178409955,
                    &[
                        (0.6781777063502288, [0.0, 0.0, 1.0, 1.0]),
                        (0.5820008560310374, [0.0, 0.0, 0.0, 0.0]),
                    ],
                ),
                g(
                    "Eclipse",
                    1,
                    50.76406064859094,
                    &[
                        (0.5046206495972989, [0.0, 0.0, 0.0, 1.0]),
                        (
                            0.29251495688254986,
                            [1.0, 0.8859999775886536, 0.11800000071525574, 1.0],
                        ),
                        (0.27503636123247416, [0.0, 0.0, 0.0, 1.0]),
                    ],
                ),
                g(
                    "Fancy Wooden Board",
                    0,
                    89.99734567017407,
                    &[
                        (-0.20400000000000001, [0.0, 0.0, 0.0, 0.0]),
                        (0.2933, [0.0, 0.0, 0.0, 0.0]),
                        (
                            -0.08500000000000002,
                            [0.5879999995231628, 0.2280000001192093, 0.0, 1.0],
                        ),
                        (
                            -0.16299999999999998,
                            [1.0, 0.5600000023841858, 0.05900000035762787, 1.0],
                        ),
                        (0.246, [0.5289999842643738, 0.17900000512599945, 0.0, 1.0]),
                        (0.16080000000000005, [1.0, 0.5809999704360962, 0.0, 1.0]),
                    ],
                ),
            ]
        }
        Kind::NoiseTexture => {
            // VEGAS's presets: Noise style, Number of layers, Frequency X
            // and Y, Offset X and Y, Progress, Min, Max, Bias, Amplitude,
            // Grain; then Color 1 and Color 2.
            let names = [
                "noiseStyle",
                "numberOfLayers",
                "frequencyX",
                "frequencyY",
                "offsetX",
                "offsetY",
                "offsetprogressinDegrees",
                "noiseParametersMin",
                "noiseParametersMax",
                "noiseParametersBias",
                "noiseParametersAmplitude",
                "noiseParametersGrainAmplitudeFalloff",
            ];
            let n = |name, v: [f64; 12], c1: [f64; 4], c2: [f64; 4]| {
                let mut values: Vec<(&'static str, PresetValue)> =
                    names.iter().zip(v).map(|(&k, x)| (k, num(&[x]))).collect();
                values.push(("color1", num(&c1)));
                values.push(("color2", num(&c2)));
                p(name, values)
            };
            vec![
                n(
                    "Standard Noise",
                    [0.0, 1.0, 10.0, 10.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.5],
                    [0.0, 0.0, 0.0, 1.0],
                    [1.0, 1.0, 1.0, 1.0],
                ),
                n(
                    "Standard Turbulence",
                    [1.0, 1.0, 10.0, 10.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.5],
                    [0.0, 0.0, 0.0, 1.0],
                    [1.0, 1.0, 1.0, 1.0],
                ),
                n(
                    "Soft Clouds",
                    [0.0, 4.0, 2.0, 6.0, 0.0, 2.0, 1.0, 0.0, 1.0, -1.0, 3.7, 0.5],
                    [1.0, 0.894118, 1.0, 1.0],
                    [0.31, 0.31, 1.0, 1.0],
                ),
                n(
                    "Puffy Clouds",
                    [1.0, 3.5, 1.5, 3.0, 1.5, 0.0, 0.0, 0.0, 1.0, -0.3, 2.0, 0.5],
                    [0.411765, 0.34902, 1.0, 1.0],
                    [1.0, 1.0, 1.0, 1.0],
                ),
                n(
                    "Cloud Cover",
                    [0.0, 3.0, 2.0, 8.0, 0.0, 0.0, 0.0, 0.0, 1.0, -1.0, 4.0, 0.7],
                    [0.54902, 0.54902, 0.54902, 1.0],
                    [0.0, 0.0, 0.0, 0.0],
                ),
                n(
                    "Lava",
                    [0.0, 2.0, 15.0, 5.0, 0.1, 0.0, 0.0, 0.1, 1.0, 0.0, 1.5, 0.5],
                    [1.0, 0.760784, 0.0, 1.0],
                    [0.87451, 0.133333, 0.0, 1.0],
                ),
                n(
                    "Plasma",
                    [0.0, 4.0, 5.0, 5.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.5],
                    [1.0, 0.0, 1.0, 1.0],
                    [0.0, 1.0, 1.0, 1.0],
                ),
                n(
                    "Blood Cells",
                    [1.0, 2.0, 10.0, 10.0, 0.0, 0.0, 0.0, 0.0, 0.8, 0.0, 1.5, 0.5],
                    [0.509804, 0.0, 0.090196, 1.0],
                    [1.0, 0.8, 0.886667, 1.0],
                ),
                n(
                    "Camouflage",
                    [
                        0.0, 2.0, 15.0, 15.0, 0.0, 0.0, 1.0, 0.48, 0.5, -0.8, 4.0, 0.3,
                    ],
                    [0.254902, 0.705882, 0.0, 1.0],
                    [0.415686, 0.223529, 0.0, 1.0],
                ),
                n(
                    "Gravel",
                    [0.0, 2.0, 20.0, 20.0, 0.0, 0.0, 0.0, 0.2, 1.0, 0.0, 1.2, 1.0],
                    [0.0, 0.0, 0.0, 1.0],
                    [1.0, 1.0, 1.0, 1.0],
                ),
                n(
                    "Marble 1",
                    [3.0, 4.0, 2.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.5],
                    [0.392157, 0.392157, 0.501961, 1.0],
                    [0.784314, 0.784314, 0.705882, 1.0],
                ),
                n(
                    "Marble 2",
                    [2.0, 4.0, 1.5, 3.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.5],
                    [0.501961, 0.352941, 0.431373, 1.0],
                    [0.768627, 0.768627, 0.768627, 1.0],
                ),
                n(
                    "Moss",
                    [
                        1.0, 4.0, 8.0, 8.0, 0.0, 0.0, 0.0, 0.0, 0.55, -0.16, 1.5, 1.0,
                    ],
                    [0.313726, 0.54902, 0.0, 1.0],
                    [0.0, 0.0, 0.0, 1.0],
                ),
                n(
                    "Wood",
                    [6.0, 3.0, 0.5, 5.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.5],
                    [0.627451, 0.313726, 0.0, 1.0],
                    [0.470588, 0.137255, 0.0, 1.0],
                ),
                n(
                    "Microscopic Threads 1",
                    [6.0, 4.0, 3.0, 3.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.7],
                    [0.176471, 0.501961, 1.0, 1.0],
                    [0.0, 0.392157, 0.070588, 1.0],
                ),
                n(
                    "Microscopic Threads 2",
                    [1.0, 3.0, 4.0, 4.0, 0.0, 0.0, 0.0, 0.15, 1.0, 0.4, 2.0, 1.0],
                    [1.0, 0.294118, 0.784314, 1.0],
                    [0.235294, 0.0, 0.0, 1.0],
                ),
                n(
                    "Lightning",
                    [4.0, 5.0, 1.3, 4.0, 0.55, 0.0, 0.0, 0.0, 1.0, 0.0, 7.0, 0.48],
                    [1.0, 1.0, 0.686275, 1.0],
                    [0.0, 0.0, 0.0, 1.0],
                ),
                n(
                    "Starry Sky",
                    [3.0, 5.0, 2.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.2, 0.0, 2.0, 1.0],
                    [1.0, 1.0, 0.69, 1.0],
                    [0.0, 0.0, 0.0, 1.0],
                ),
                n(
                    "Charcoal Scribbles",
                    [5.0, 4.0, 4.0, 4.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.5],
                    [0.098039, 0.098039, 0.098039, 1.0],
                    [1.0, 0.921569, 0.862745, 1.0],
                ),
                n(
                    "Chalk Scribbles",
                    [2.0, 3.0, 3.0, 5.0, 0.0, 0.0, 0.0, 0.0, 0.4, -0.8, 1.5, 1.0],
                    [1.0, 0.588235, 0.588235, 1.0],
                    [0.0, 0.0, 0.0, 1.0],
                ),
            ]
        }
        Kind::SolidColor => {
            // VEGAS's ten, gray and orange at 128 of 255.
            let solid =
                |name, rgb: [f64; 3]| p(name, vec![("color", num(&[rgb[0], rgb[1], rgb[2], 1.0]))]);
            const HALF: f64 = 0.501961;
            vec![
                solid("White", [1.0, 1.0, 1.0]),
                solid("Black", [0.0, 0.0, 0.0]),
                solid("Red", [1.0, 0.0, 0.0]),
                solid("Green", [0.0, 1.0, 0.0]),
                solid("Blue", [0.0, 0.0, 1.0]),
                solid("Yellow", [1.0, 1.0, 0.0]),
                solid("Gray", [HALF, HALF, HALF]),
                solid("Cyan", [0.0, 1.0, 1.0]),
                solid("Magenta", [1.0, 0.0, 1.0]),
                solid("Orange", [1.0, HALF, 0.0]),
            ]
        }
        Kind::TestPattern => vec![
            defaults(),
            one("SMPTE Bars (NTSC)", "testPatternType", 5.0),
            one("SMPTE Bars (PAL)", "testPatternType", 6.0),
            one("Color Bars (NTSC)", "testPatternType", 0.0),
            one("Color Bars (PAL)", "testPatternType", 1.0),
            one("White Porches", "testPatternType", 7.0),
            one("Yellow Porches", "testPatternType", 8.0),
            one("Pluge and Porches", "testPatternType", 3.0),
            one("Ramp", "testPatternType", 4.0),
            one("Gradient", "testPatternType", 2.0),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Grid {
        Grid {
            origin: [0.0, 0.0],
            frame: [64.0, 36.0],
            scale: [1.0, 1.0],
            par: 1.0,
        }
    }

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

    #[test]
    fn every_generator_and_preset_paints_the_frame() {
        let g = grid();
        for kind in Kind::ALL {
            let presets = presets(kind);
            assert!(!presets.is_empty(), "{kind:?}");
            for p in &presets {
                let paint = kind.painter(&vals(kind, p), &g, Look::FLOAT);
                for (u, v) in [(0.0, 0.0), (0.3, 0.7), (0.5, 0.5), (0.999, 0.999)] {
                    let c = paint(u, v);
                    assert!(c.iter().all(|x| x.is_finite()), "{kind:?} {}", p.name);
                    assert!(c[3] >= 0.0 && c[3] <= 1.0, "{kind:?} {}", p.name);
                }
            }
        }
    }

    #[test]
    fn the_checkerboard_alternates_its_colors() {
        let g = grid();
        // Large Tiles (after the defaults).
        let p = presets(Kind::Checkerboard).remove(1);
        let paint = Kind::Checkerboard.painter(&vals(Kind::Checkerboard, &p), &g, Look::FLOAT);
        // Tiles a tenth of the width, square (6.4 pixels): neighbours
        // across differ, diagonal ones match (tiles' middles).
        let (a, b) = (paint(0.55, 16.0 / 36.0), paint(0.45, 16.0 / 36.0));
        assert!((a[0] - b[0]).abs() > 0.9, "{a:?} {b:?}");
        let c = paint(0.45, 9.6 / 36.0);
        assert!((a[0] - c[0]).abs() < 1e-6, "{a:?} {c:?}");
        // The defaults and VEGAS's twelve presets; Large Tiles is black
        // and white.
        assert_eq!(presets(Kind::Checkerboard).len(), 13);
        assert_eq!(presets(Kind::Checkerboard)[0].name, "(Default)");
        assert_eq!(paint(0.05, 0.05)[..3], [0.0, 0.0, 0.0]);
        // A tile's edge blended across the whole tile: between the colors.
        let s = Vals::of(
            Kind::Checkerboard.specs(),
            &|spec| match spec.name().to_bytes() {
                b"edgeBlendingHorizontal" => vec![1.0],
                _ => spec.default(),
            },
        );
        let paint = Kind::Checkerboard.painter(&s, &g, Look::FLOAT);
        let edge = paint(6.5 / 64.0, 16.5 / 36.0)[0];
        assert!(edge > 0.2 && edge < 0.8, "{edge}");
    }

    #[test]
    fn square_tiles_are_square_on_screen() {
        // Pixels twice as wide as tall: a square tile half as many pixels
        // wide as tall.
        let g = Grid {
            frame: [100.0, 100.0],
            ..grid()
        };
        let p = presets(Kind::Checkerboard).remove(0);
        let wide = Look {
            pixel_aspect: 2.0,
            ..Look::FLOAT
        };
        let paint = Kind::Checkerboard.painter(&vals(Kind::Checkerboard, &p), &g, wide);
        let at = |x: f64, y: f64| paint((x + 0.5) / 100.0, (y + 0.5) / 100.0)[0];
        // 10 pixels wide, 20 tall.
        assert_eq!((at(9.0, 0.0), at(10.0, 0.0)), (0.0, 1.0));
        assert_eq!((at(0.0, 19.0), at(0.0, 20.0)), (0.0, 1.0));
    }

    #[test]
    fn the_gradient_goes_from_point_to_point() {
        let g = grid();
        let p = presets(Kind::ColorGradient).remove(0);
        let s = vals(Kind::ColorGradient, &p);
        let paint = Kind::ColorGradient.painter(&s, &g, Look::FLOAT);
        // The first column the first point's color, the last a step from
        // the other's (the pixels measured from their left edges).
        let (l, r) = (paint(0.0, 0.5), paint(0.999, 0.5));
        let (c1, c2) = (s.rgba(c"color1"), s.rgba(c"color2"));
        assert!((l[2] - c1[2]).abs() < 1e-4, "{l:?}");
        assert!((r[0] - c2[0]).abs() < 0.02, "{r:?}");
        // The middle halfway.
        let m = paint(0.5, 0.5);
        assert!((m[0] - (c1[0] + c2[0]) / 2.0).abs() < 1e-3, "{m:?}");
    }

    #[test]
    fn noise_stays_between_its_colors_and_flows_with_progress() {
        let g = grid();
        for style in 0..7 {
            let s = Vals::of(NOISE_TEXTURE, &|spec| {
                if spec.name() == c"noiseStyle" {
                    vec![f64::from(style)]
                } else {
                    spec.default()
                }
            });
            let paint = Kind::NoiseTexture.painter(&s, &g, Look::FLOAT);
            let values: Vec<f32> = (0..200)
                .map(|i| paint(f64::from(i % 20) / 20.0, f64::from(i / 20) / 10.0)[0])
                .collect();
            assert!(values.iter().all(|v| (0.0..=1.0).contains(v)), "{style}");
            let (lo, hi) = values
                .iter()
                .fold((1.0f32, 0.0f32), |(a, b), &v| (a.min(v), b.max(v)));
            assert!(hi - lo > 0.2, "style {style}: {lo}–{hi}");
        }
        let at = |progress: f64| {
            let s = Vals::of(NOISE_TEXTURE, &|spec| {
                if spec.name() == c"offsetprogressinDegrees" {
                    vec![progress]
                } else {
                    spec.default()
                }
            });
            Kind::NoiseTexture.painter(&s, &g, Look::FLOAT)(0.37, 0.61)
        };
        assert!((at(0.0)[0] - at(0.5)[0]).abs() > 1e-4);
    }

    /// VEGAS's Solid Color: its ten presets, the color premultiplied,
    /// transparent at first.
    #[test]
    fn the_solid_color_is_vegass() {
        let g = grid();
        let presets = presets(Kind::SolidColor);
        assert_eq!(presets.len(), 10);
        let gray = &presets[6];
        assert_eq!(gray.name, "Gray");
        let paint = Kind::SolidColor.painter(&vals(Kind::SolidColor, gray), &g, Look::FLOAT);
        assert_eq!(paint(0.3, 0.7), [0.501961, 0.501961, 0.501961, 1.0]);
        let s = Vals::of(Kind::SolidColor.specs(), &|_| vec![1.0, 0.5, 0.0, 0.5]);
        assert_eq!(
            Kind::SolidColor.painter(&s, &g, Look::FLOAT)(0.9, 0.1),
            [0.5, 0.25, 0.0, 0.5]
        );
        let s = Vals::of(Kind::SolidColor.specs(), &|spec| spec.default());
        assert_eq!(
            Kind::SolidColor.painter(&s, &g, Look::FLOAT)(0.5, 0.5),
            [0.0; 4]
        );
    }

    /// VEGAS's list of patterns, its gradient and its ramp.
    #[test]
    fn the_test_patterns_are_vegass() {
        let g = Grid {
            frame: [640.0, 360.0],
            ..grid()
        };
        let paint = |pattern: f64| {
            let s = Vals::of(Kind::TestPattern.specs(), &|_| vec![pattern]);
            Kind::TestPattern.painter(&s, &g, Look::FLOAT)
        };
        let byte = |c: [f32; 4]| c.map(|v| (v * 255.0).round() as u8);
        assert_eq!(presets(Kind::TestPattern).len(), 10);
        // Color bars: NTSC's at 75 %, PAL's with a white of 235.
        assert_eq!(byte(paint(0.0)(0.05, 0.5)), [180, 180, 180, 255]);
        assert_eq!(byte(paint(1.0)(0.05, 0.5)), [235, 235, 235, 255]);
        assert_eq!(byte(paint(1.0)(0.95, 0.5)), [16, 16, 180, 255]);
        // Ramp: gray across, the border's within a tenth of the edges.
        assert_eq!(byte(paint(4.0)(0.5, 0.5)), [127, 127, 127, 255]);
        assert_eq!(byte(paint(4.0)(0.5, 0.05)), [7, 7, 7, 255]);
    }
}
