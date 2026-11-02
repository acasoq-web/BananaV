//! VEGAS's media generators as OpenFX generators written as tables of
//! settings (as VEGAS's Help describes them, on [`crate::kit`]):
//! Checkerboard, Color Gradient, Noise Texture and Test Pattern. Each
//! works out every pixel from its place in the frame (0–1 across, 0–1
//! down) and its settings ([`Kind::painter`]); colors are video's values,
//! as Titles & Text's. Where VEGAS's control is a list of its own (Color
//! Gradient's control points), a few fixed settings stand in for it;
//! Noise Texture's styles and Test Pattern's patterns are BananaV's own.
#![allow(unsafe_code)]

use std::f64::consts::PI;
use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::kit::{Grid, Spec, Vals, define_all};
use crate::ofx::{Fx, Props, for_rows, is, suites};
use crate::{Preset, PresetValue};

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
        c"tileWidth",
        c"Width",
        0.1,
        0.001,
        1.0,
        c"A tile's width, a part of the frame's width.",
    ),
    Spec::Number(
        c"tileHeight",
        c"Height",
        0.1,
        0.001,
        1.0,
        c"A tile's height, a part of the frame's height (unless Square).",
    ),
    Spec::Toggle(
        c"square",
        c"Square",
        true,
        c"The tiles square: the height follows the width.",
    ),
    Spec::GroupEnd,
    Spec::Group(c"edgeBlending", c"Edge blending"),
    Spec::Number(
        c"blendHorizontal",
        c"Horizontal",
        0.0,
        0.0,
        1.0,
        c"The tiles' left and right edges blended: 1 across the whole tile.",
    ),
    Spec::Number(
        c"blendVertical",
        c"Vertical",
        0.0,
        0.0,
        1.0,
        c"The tiles' top and bottom edges blended (unless Proportional).",
    ),
    Spec::Toggle(
        c"proportional",
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
        c"offsetX",
        c"X",
        0.0,
        0.0,
        1.0,
        c"The grid moved across: 1 is the frame's width.",
    ),
    Spec::Number(
        c"offsetY",
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
        c"type",
        c"Type",
        &[c"Linear", c"Elliptical", c"Rectangular", c"Angular"],
        0,
        c"The gradient's shape.",
    ),
    Spec::Point(
        c"center",
        c"Center",
        [0.5, 0.5],
        c"The gradient's center: 0, 0 is the lower left corner.",
    ),
    Spec::Angle(
        c"angle",
        c"Angle",
        0.0,
        c"The angle of the line the control points lie on.",
    ),
    Spec::Int(
        c"points",
        c"Control Points",
        2,
        2,
        4,
        c"How many control points the gradient has.",
    ),
    Spec::Number(
        c"distance1",
        c"Point 1 Distance",
        -0.8,
        -1.0,
        1.0,
        c"How far the point is from the center (1: half the frame's width).",
    ),
    Spec::Color(
        c"color1",
        c"Point 1 Color",
        [0.13, 0.2, 0.55, 1.0],
        c"The point's color.",
    ),
    Spec::Number(
        c"distance2",
        c"Point 2 Distance",
        0.8,
        -1.0,
        1.0,
        c"How far the point is from the center (1: half the frame's width).",
    ),
    Spec::Color(
        c"color2",
        c"Point 2 Color",
        [0.96, 0.62, 0.25, 1.0],
        c"The point's color.",
    ),
    Spec::Number(
        c"distance3",
        c"Point 3 Distance",
        0.0,
        -1.0,
        1.0,
        c"How far the point is from the center (1: half the frame's width).",
    ),
    Spec::Color(
        c"color3",
        c"Point 3 Color",
        [0.85, 0.3, 0.4, 1.0],
        c"The point's color.",
    ),
    Spec::Number(
        c"distance4",
        c"Point 4 Distance",
        0.4,
        -1.0,
        1.0,
        c"How far the point is from the center (1: half the frame's width).",
    ),
    Spec::Color(
        c"color4",
        c"Point 4 Color",
        [1.0, 1.0, 1.0, 1.0],
        c"The point's color.",
    ),
];
const NOISE_TEXTURE: &[Spec] = &[
    Spec::Choice(
        c"style",
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
        c"layers",
        c"Number of layers",
        1.0,
        1.0,
        5.0,
        c"How many layers of finer noise, each twice as fine; a part of one fades it in.",
    ),
    Spec::Toggle(
        c"swapColors",
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
        c"progress",
        c"Progress (in degrees)",
        0.0,
        0.0,
        10.0,
        c"The pattern moved through its depth: animate it and the noise flows.",
    ),
    Spec::GroupEnd,
    Spec::Group(c"noiseParameters", c"Noise parameters"),
    Spec::Number(
        c"min",
        c"Min",
        0.0,
        0.0,
        1.0,
        c"The noise's value that takes Color 1.",
    ),
    Spec::Number(
        c"max",
        c"Max",
        1.0,
        0.0,
        1.0,
        c"The noise's value that takes Color 2.",
    ),
    Spec::Number(
        c"bias",
        c"Bias",
        0.0,
        -1.0,
        1.0,
        c"Added to the noise: below 0 toward Color 1, above toward Color 2.",
    ),
    Spec::Number(
        c"amplitude",
        c"Amplitude",
        1.0,
        0.0,
        10.0,
        c"The noise multiplied: more contrast between the colors.",
    ),
    Spec::Number(
        c"grain",
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
    c"pattern",
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

    /// What colors each place of the frame (u across, v down, 0–1) at
    /// settings `s`: premultiplied video values.
    /// What the card draws for this generator with settings `s` on the
    /// picture `g` places; `None` where the processor still draws it.
    pub(crate) fn card_plan(self, s: &Vals, g: &Grid) -> Option<crate::accel::Plan> {
        use crate::accel::{GenerateKernel, Plan};
        let size = [width_px(g) as f32, g.height_px() as f32];
        match self {
            Kind::Checkerboard => Some(Plan::Generate {
                kernel: GenerateKernel::Checkerboard,
                f: Checker::of(s, g).numbers(),
                j: [0; 4],
            }),
            Kind::NoiseTexture => {
                let (mut f, j) = Noise::of(s).numbers();
                f.splice(0..0, size);
                Some(Plan::Generate {
                    kernel: GenerateKernel::NoiseTexture,
                    f,
                    j,
                })
            }
            Kind::SolidColor => Some(Plan::Generate {
                kernel: GenerateKernel::SolidColor,
                f: pre(s.rgba(c"color")).to_vec(),
                j: [0; 4],
            }),
            Kind::TestPattern => Some(Plan::Generate {
                kernel: GenerateKernel::TestPattern,
                f: size.to_vec(),
                j: [s.i(c"pattern").clamp(0, 8) as u32, 0, 0, 0],
            }),
            _ => None,
        }
    }

    pub(crate) fn painter(self, s: &Vals, g: &Grid) -> Box<dyn Fn(f64, f64) -> [f32; 4] + Sync> {
        match self {
            Kind::Checkerboard => checkerboard(s, g),
            Kind::ColorGradient => color_gradient(s, g),
            Kind::NoiseTexture => noise_texture(s),
            Kind::SolidColor => {
                // VEGAS's: the color premultiplied, everywhere.
                let c = pre(s.rgba(c"color"));
                Box::new(move |_, _| c)
            }
            Kind::TestPattern => test_pattern(s, g),
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
        // The groups too, as sections (Checkerboard's, Noise Texture's).
        define_all(fx, kind.specs());
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
    let paint = kind.painter(&vals, &grid);
    let straight = out.channels == 4 && out.unpremultiplied;
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(row) = out.row(y) else { return };
        for x in columns.clone() {
            let (u, v) = grid.uv(x, y);
            let mut c = paint(u, v);
            if straight && c[3] > 0.0 {
                c = [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]];
            }
            // SAFETY: a row the output has, a column inside its bounds.
            unsafe { crate::colors::put_pixel(&out, row, x, c) };
        }
    };
    for_rows(y1.max(out.bounds[1])..y2.min(out.bounds[3]), &fill);
    drop(out);
    kOfxStatOK
}

// The pictures.

/// The frame's width in this picture's pixels.
fn width_px(g: &Grid) -> f64 {
    (g.frame[0] * g.scale[0] / g.par).max(1.0)
}

/// A straight color premultiplied.
fn pre(c: [f32; 4]) -> [f32; 4] {
    [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
}

fn mix(a: [f32; 4], b: [f32; 4], k: f64) -> [f32; 4] {
    let k = k as f32;
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k)
}

/// VEGAS's Checkerboard, in the frame's pixels from its top left: tiles
/// Width and Height of the frame (square on screen with Square), at
/// least a pixel; each edge a straight blend from one color to the other
/// over the Edge blending part of the tile on either side (half a pixel
/// at least), the colors half and half on it; the grid moved by Grid
/// offset.
fn checkerboard(s: &Vals, g: &Grid) -> Box<dyn Fn(f64, f64) -> [f32; 4] + Sync> {
    let c = Checker::of(s, g);
    Box::new(move |u, v| c.paint(u, v))
}

/// Checkerboard's tiles on the picture `g` places: the frame's size in its
/// pixels, the tiles' width and height, the blends' half widths, the
/// grid's move and the two colors (premultiplied).
#[derive(Clone, Copy)]
pub(crate) struct Checker {
    size: [f64; 2],
    tile: [f64; 2],
    blend: [f64; 2],
    offset: [f64; 2],
    colors: [[f32; 4]; 2],
}

impl Checker {
    pub(crate) fn of(s: &Vals, g: &Grid) -> Checker {
        let (w, h) = (width_px(g), g.height_px());
        let par = g.par;
        let mut tw = s.f(c"tileWidth").clamp(0.001, 1.0) * w;
        let mut th = if s.b(c"square") {
            tw * par
        } else {
            s.f(c"tileHeight").clamp(0.001, 1.0) * h
        };
        if tw < 1.0 / par {
            th = th / par / tw;
            tw = 1.0 / par;
        }
        if th < 1.0 {
            tw /= th;
            th = 1.0;
        }
        let bh = s.f(c"blendHorizontal").clamp(0.0, 1.0);
        let bv = if s.b(c"proportional") {
            bh
        } else {
            s.f(c"blendVertical").clamp(0.0, 1.0)
        };
        let (bx, by) = ((bh * tw * 0.5).max(0.5), (bv * th * 0.5).max(0.5));
        let (c1, c2) = (pre(s.rgba(c"color1")), pre(s.rgba(c"color2")));
        let (ox, oy) = (s.f(c"offsetX") * w, s.f(c"offsetY") * h);
        Checker {
            size: [w, h],
            tile: [tw, th],
            blend: [bx, by],
            offset: [ox, oy],
            colors: [c1, c2],
        }
    }

    /// The color at the frame's u, v (v from the top).
    fn paint(&self, u: f64, v: f64) -> [f32; 4] {
        let ([w, h], [tw, th], [bx, by], [ox, oy]) =
            (self.size, self.tile, self.blend, self.offset);
        let [c1, c2] = self.colors;
        let (x, y) = (u * w - ox, v * h - oy);
        let (ix, iy) = ((x / tw).floor(), (y / th).floor());
        let (own, other) = if (ix + iy).rem_euclid(2.0) >= 1.0 {
            (c2, c1)
        } else {
            (c1, c2)
        };
        // How far into the other color: 1 at a blend's far end, 0.5 on
        // the edge, 0 inside.
        let reach = |t: f64, size: f64, b: f64| {
            let from_low = t - (t / size).floor() * size;
            let near = from_low.min(size - from_low);
            ((b - near) * (0.5 / b)).clamp(0.0, 0.5)
        };
        let (kx, ky) = (reach(x, tw, bx), reach(y, th, by));
        // Across the rows' blend the two colors mixed first, as VEGAS's.
        let row_own = mix(own, other, ky);
        let row_other = mix(other, own, ky);
        mix(row_own, row_other, kx)
    }

    /// Its numbers for the card's `kit_checkerboard`.
    pub(crate) fn numbers(&self) -> Vec<f32> {
        let mut f: Vec<f32> = [
            self.size[0],
            self.size[1],
            self.tile[0],
            self.tile[1],
            self.blend[0],
            self.blend[1],
            self.offset[0],
            self.offset[1],
        ]
        .map(|v| v as f32)
        .to_vec();
        f.extend(self.colors[0]);
        f.extend(self.colors[1]);
        f
    }
}

fn color_gradient(s: &Vals, g: &Grid) -> Box<dyn Fn(f64, f64) -> [f32; 4] + Sync> {
    let aspect = g.aspect();
    let ty = s.i(c"type");
    let c = s.point(c"center");
    let (sn, cs) = s.f(c"angle").to_radians().sin_cos();
    let n = s.i(c"points").clamp(2, 4) as usize;
    let names = [
        (c"distance1", c"color1"),
        (c"distance2", c"color2"),
        (c"distance3", c"color3"),
        (c"distance4", c"color4"),
    ];
    let mut points: Vec<(f64, [f32; 4])> = names[..n]
        .iter()
        .map(|&(d, col)| (s.f(d), s.rgba(col)))
        .collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    Box::new(move |u, v| {
        // Across and up in the frame's heights, turned by the angle.
        let (dx, dy) = ((u - c[0]) * aspect, c[1] - v);
        let (px, py) = (dx * cs + dy * sn, -dx * sn + dy * cs);
        let half = aspect / 2.0;
        let t = match ty {
            1 => (px / half).hypot(py / 0.5),
            2 => (px / half).abs().max((py / 0.5).abs()),
            3 => py.atan2(px) / PI,
            _ => px / half,
        };
        let first = points[0];
        let last = points[points.len() - 1];
        let color = if t <= first.0 {
            first.1
        } else if t >= last.0 {
            last.1
        } else {
            let i = points
                .iter()
                .position(|p| p.0 > t)
                .unwrap_or(points.len() - 1);
            let (a, b) = (points[i - 1], points[i]);
            let k = (t - a.0) / (b.0 - a.0).max(1e-9);
            mix(a.1, b.1, k)
        };
        pre(color)
    })
}

/// Noise Texture's styles: the kind — 0 fractal, 1 veins,
/// 2 rings —, whether the layers add up their sizes (turbulent), and the
/// veins' five numbers (the rings' repeats first).
const NOISE_STYLES: [(u32, bool, [f64; 5]); 7] = [
    (0, false, [0.0; 5]),
    (0, true, [0.0; 5]),
    (1, false, [0.9, 0.15, 0.4, 1.2, 0.1]),
    (1, false, [0.05, 0.9, 1.3, 0.0, 0.0]),
    (1, false, [0.6, 0.0, 0.2, 0.5, 0.1]),
    (1, false, [1.0, 0.0, 1.5, 0.5, 0.05]),
    (2, false, [4.0, 0.0, 0.0, 0.0, 0.0]),
];

/// Noise Texture's numbers, as VEGAS works them out: the style, each
/// layer's weight (Grain to the layer's power, the last a part by Number of
/// layers, those over 0.01), the noise's place (Frequency, Offset,
/// Progress), the colors' line (Amplitude ⁄ (Max − Min), the offset by Bias
/// and Min) and the two colors, premultiplied (Swap colors swaps them).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Noise {
    pub style: usize,
    pub weights: Vec<f64>,
    pub place: [f64; 5],
    pub scale: f64,
    pub offset: f64,
    pub amplitude: f64,
    pub colors: [[f32; 4]; 2],
}

impl Noise {
    pub(crate) fn of(s: &Vals) -> Noise {
        let layers = s.f(c"layers").clamp(1.0, 5.0);
        let grain = s.f(c"grain").clamp(0.0, 1.0);
        let whole = (layers - 0.499_999_985).round_ties_even() as usize;
        let mut weights = vec![1.0f64; whole + 1];
        for k in 1..=whole {
            weights[k] = grain * weights[k - 1];
        }
        weights[whole] *= layers - whole as f64;
        let used = weights.iter().take_while(|&&w| w > 0.01).count();
        weights.truncate(used.max(1));
        let (min, max) = (s.f(c"min"), s.f(c"max"));
        let (amplitude, bias) = (s.f(c"amplitude"), s.f(c"bias"));
        let (scale, offset) = if min < max {
            let k = amplitude / (max - min);
            (k, bias - min * k)
        } else {
            (0.0, (min + max) * 0.5 * amplitude + bias)
        };
        let pre = |c: [f32; 4]| [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]];
        let (a, b) = (pre(s.rgba(c"color1")), pre(s.rgba(c"color2")));
        Noise {
            style: s.i(c"style").clamp(0, 6) as usize,
            weights,
            place: [
                s.f(c"frequencyX"),
                s.f(c"frequencyY"),
                s.f(c"offsetX"),
                s.f(c"offsetY"),
                s.f(c"progress"),
            ],
            scale,
            offset,
            amplitude,
            colors: if s.b(c"swapColors") { [b, a] } else { [a, b] },
        }
    }

    /// The noise's value at `u`, `v` (the frame's 0–1, from the top left).
    fn value(&self, u: f64, v: f64) -> f64 {
        let (kind, turbulent, p) = NOISE_STYLES[self.style];
        let [fx, fy, ox, oy, z] = self.place;
        let (x, y) = (u * fx + ox, v * fy + oy);
        let (mut sum, mut total) = (0.0, 0.0);
        for (k, &w) in self.weights.iter().enumerate() {
            let f = f64::from(1u32 << k);
            let n = gradient_noise(x * f, y * f, z * f) * w * 1.515_151_515_151_515;
            sum += if turbulent { n.abs() } else { n };
            total += w;
        }
        let n = sum / total;
        match kind {
            0 if turbulent => n,
            0 => n * 0.5 + 0.5,
            1 => {
                let mut s = p[2] * n + p[0] * fx * u - p[1] * fy * v;
                if p[3] > 0.0 {
                    let t = p[3] * fy * v;
                    let t = t - t.floor();
                    s += p[4] * 4.0 * t * (1.0 - t);
                }
                let t = s - s.floor();
                1.0 - (2.0 * t - 1.0) * (2.0 * t - 1.0)
            }
            _ => {
                let s = (n + 2.0) * p[0] * self.amplitude;
                let t = s - s.floor();
                if t >= 0.5 {
                    (4.0 * t - 3.0) * (4.0 * t - 3.0)
                } else {
                    2.0 - (4.0 * t - 1.0) * (4.0 * t - 1.0)
                }
            }
        }
    }

    /// The texture's color at `u`, `v`: the value along the colors' line,
    /// in 65,536ths as VEGAS's, held within the two colors.
    pub(crate) fn at(&self, u: f64, v: f64) -> [f32; 4] {
        let t = ((self.value(u, v) * self.scale + self.offset) * 65536.0 + 0.5 + 1.5e-8)
            .round_ties_even()
            .clamp(0.0, 65536.0) as f32
            * 1.525_878_9e-5;
        let [a, b] = self.colors;
        std::array::from_fn(|k| b[k] * t + a[k] * (1.0 - t))
    }

    /// The card's numbers (`kit_noise_texture`): the place, five weights,
    /// the line, Amplitude and the two colors; the style and how many
    /// layers.
    pub(crate) fn numbers(&self) -> (Vec<f32>, [u32; 4]) {
        let mut f: Vec<f32> = self.place.iter().map(|&v| v as f32).collect();
        f.extend((0..5).map(|k| self.weights.get(k).copied().unwrap_or(0.0) as f32));
        f.extend([self.scale as f32, self.offset as f32, self.amplitude as f32]);
        f.extend(self.colors.iter().flatten().copied());
        (f, [self.style as u32, self.weights.len() as u32, 0, 0])
    }
}

/// The lattice's gradient at a corner: one of Perlin's twelve directions,
/// picked by a hash of the corner (whole-number math the card repeats).
fn lattice(x: i32, y: i32, z: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ (z as u32).wrapping_mul(0xcb1a_b31f);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    (h ^ (h >> 16)) & 15
}

fn gradient(h: u32, x: f64, y: f64, z: f64) -> f64 {
    let u = if h < 8 { x } else { y };
    let v = if h < 4 {
        y
    } else if h == 12 || h == 14 {
        x
    } else {
        z
    };
    (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
}

/// Perlin's improved noise (2002) at `x`, `y`, `z`: the quintic fade, the
/// corners' gradients of unit length.
fn gradient_noise(x: f64, y: f64, z: f64) -> f64 {
    let (xi, yi, zi) = (x.floor(), y.floor(), z.floor());
    let (xf, yf, zf) = (x - xi, y - yi, z - zi);
    let (xi, yi, zi) = (xi as i32, yi as i32, zi as i32);
    let fade = |t: f64| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (u, v, w) = (fade(xf), fade(yf), fade(zf));
    let g = |i: i32, j: i32, k: i32| {
        let h = lattice(xi + i, yi + j, zi + k);
        gradient(h, xf - f64::from(i), yf - f64::from(j), zf - f64::from(k))
    };
    let lerp = |t: f64, a: f64, b: f64| a + (b - a) * t;
    let near = lerp(
        v,
        lerp(u, g(0, 0, 0), g(1, 0, 0)),
        lerp(u, g(0, 1, 0), g(1, 1, 0)),
    );
    let far = lerp(
        v,
        lerp(u, g(0, 0, 1), g(1, 0, 1)),
        lerp(u, g(0, 1, 1), g(1, 1, 1)),
    );
    lerp(w, near, far) * std::f64::consts::FRAC_1_SQRT_2
}

fn noise_texture(s: &Vals) -> Box<dyn Fn(f64, f64) -> [f32; 4] + Sync> {
    let n = Noise::of(s);
    Box::new(move |u, v| n.at(u, v))
}

/// A video color (straight, opaque).
const fn opaque(r: f32, g: f32, b: f32) -> [f32; 4] {
    [r, g, b, 1.0]
}

/// VEGAS's Test Pattern: NTSC's bars all at 75 %, PAL's (EBU's) with a
/// 100 % white; VEGAS's gradient (red to green across, blue toward the
/// bottom) and gray ramp (black around it, a tenth of the frame); SMPTE's
/// bars and the black and white references of their bottom row.
fn test_pattern(s: &Vals, _g: &Grid) -> Box<dyn Fn(f64, f64) -> [f32; 4] + Sync> {
    let pattern = s.i(c"pattern");
    Box::new(move |u, v| match pattern {
        1 => color_bars(u, 1.0),
        2 => opaque((1.0 - u) as f32, u as f32, v as f32),
        3 => pluge_row(u, None),
        4 => {
            let inside = (0.1..=0.9).contains(&u) && (0.1..=0.9).contains(&v);
            let t = if inside { u as f32 } else { 0.0 };
            opaque(t, t, t)
        }
        5 => smpte_bars(u, v, 0.75, 0.75),
        6 => smpte_bars(u, v, 1.0, 0.75),
        7 => pluge_row(u, Some(opaque(1.0, 1.0, 1.0))),
        8 => pluge_row(u, Some(opaque(0.75, 0.75, 0.0))),
        _ => color_bars(u, 0.75),
    })
}

/// Seven bars — white, yellow, cyan, green, magenta, red, blue — the
/// colors at 75 %, the white at `white`; EBU's eighth, black, when the
/// white is whole.
fn color_bars(u: f64, white: f32) -> [f32; 4] {
    let k = 0.75;
    let bars = [
        opaque(white, white, white),
        opaque(k, k, 0.0),
        opaque(0.0, k, k),
        opaque(0.0, k, 0.0),
        opaque(k, 0.0, k),
        opaque(k, 0.0, 0.0),
        opaque(0.0, 0.0, k),
        opaque(0.0, 0.0, 0.0),
    ];
    let n = if white >= 1.0 { 8.0 } else { 7.0 };
    bars[((u * n) as usize).min(n as usize - 1)]
}

/// SMPTE's color bars (as its EG 1 lays them out): the 75 % colors with a
/// `white` bar on top, the reversed blue bars, and the bottom row.
fn smpte_bars(u: f64, v: f64, white: f32, k: f32) -> [f32; 4] {
    let top = [
        opaque(white, white, white),
        opaque(k, k, 0.0),
        opaque(0.0, k, k),
        opaque(0.0, k, 0.0),
        opaque(k, 0.0, k),
        opaque(k, 0.0, 0.0),
        opaque(0.0, 0.0, k),
    ];
    let black = opaque(0.0, 0.0, 0.0);
    let bar = ((u * 7.0) as usize).min(6);
    if v < 0.67 {
        return top[bar];
    }
    if v < 0.75 {
        return [top[6], black, top[4], black, top[2], black, top[0]][bar];
    }
    pluge_row(u, None)
}

/// SMPTE's bottom row: −I, white, +Q, black and the PLUGE (black, black,
/// +4 %); with `porch` only that color in the white's place, the rest
/// black but the PLUGE.
fn pluge_row(u: f64, porch: Option<[f32; 4]>) -> [f32; 4] {
    let black = opaque(0.0, 0.0, 0.0);
    let (minus_i, plus_q) = if porch.is_some() {
        (black, black)
    } else {
        (opaque(0.0, 0.129, 0.290), opaque(0.196, 0.0, 0.416))
    };
    let bottom: [(f64, [f32; 4]); 7] = [
        (1.25, minus_i),
        (1.25, porch.unwrap_or(opaque(1.0, 1.0, 1.0))),
        (1.25, plus_q),
        (1.25, black),
        (1.0 / 3.0, black),
        (1.0 / 3.0, opaque(0.04, 0.04, 0.04)),
        (1.0 + 1.0 / 3.0, black),
    ];
    let at = u * 7.0;
    let mut edge = 0.0;
    for (w, c) in bottom {
        edge += w;
        if at < edge {
            return c;
        }
    }
    black
}

/// The factory presets of generator `kind`.
pub(crate) fn presets(kind: Kind) -> Vec<Preset> {
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    let p = |name: &'static str, values: Vec<(&'static str, PresetValue)>| Preset { name, values };
    let one = |name: &'static str, param: &'static str, v: f64| p(name, vec![(param, num(&[v]))]);
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
                        ("tileWidth", num(&[size[0]])),
                        ("tileHeight", num(&[size[1]])),
                        ("square", flag(square)),
                        ("blendHorizontal", num(&[blend[0]])),
                        ("blendVertical", num(&[blend[1]])),
                        ("proportional", flag(proportional)),
                        ("color1", num(&colors[0])),
                        ("color2", num(&colors[1])),
                        ("offsetX", num(&[offset[0]])),
                        ("offsetY", num(&[offset[1]])),
                    ],
                )
            };
            const BLACK: [f64; 4] = [0.0, 0.0, 0.0, 1.0];
            const WHITE: [f64; 4] = [1.0, 1.0, 1.0, 1.0];
            const CLEAR: [f64; 4] = [0.0, 0.0, 0.0, 0.0];
            vec![
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
        Kind::ColorGradient => vec![
            p("Sunset", vec![]),
            p(
                "Ocean",
                vec![
                    ("angle", num(&[90.0])),
                    ("color1", num(&[0.03, 0.1, 0.3, 1.0])),
                    ("color2", num(&[0.1, 0.7, 0.75, 1.0])),
                ],
            ),
            p(
                "Spotlight",
                vec![
                    ("type", num(&[1.0])),
                    ("distance1", num(&[0.0])),
                    ("color1", num(&[1.0, 1.0, 0.9, 1.0])),
                    ("distance2", num(&[1.0])),
                    ("color2", num(&[0.0, 0.0, 0.0, 1.0])),
                ],
            ),
            p(
                "Rainbow",
                vec![
                    ("points", num(&[4.0])),
                    ("distance1", num(&[-1.0])),
                    ("color1", num(&[0.9, 0.1, 0.1, 1.0])),
                    ("distance2", num(&[1.0])),
                    ("color2", num(&[0.4, 0.1, 0.9, 1.0])),
                    ("distance3", num(&[-0.33])),
                    ("color3", num(&[1.0, 0.85, 0.1, 1.0])),
                    ("distance4", num(&[0.33])),
                    ("color4", num(&[0.1, 0.75, 0.3, 1.0])),
                ],
            ),
            p(
                "Frame",
                vec![
                    ("type", num(&[2.0])),
                    ("distance1", num(&[0.7])),
                    ("color1", num(&[0.0, 0.0, 0.0, 0.0])),
                    ("distance2", num(&[1.0])),
                    ("color2", num(&[0.0, 0.0, 0.0, 1.0])),
                ],
            ),
            p(
                "Color Wheel",
                vec![
                    ("type", num(&[3.0])),
                    ("points", num(&[4.0])),
                    ("distance1", num(&[-1.0])),
                    ("color1", num(&[0.0, 0.6, 1.0, 1.0])),
                    ("distance2", num(&[1.0])),
                    ("color2", num(&[0.0, 0.6, 1.0, 1.0])),
                    ("distance3", num(&[-0.33])),
                    ("color3", num(&[1.0, 0.2, 0.6, 1.0])),
                    ("distance4", num(&[0.33])),
                    ("color4", num(&[1.0, 0.9, 0.2, 1.0])),
                ],
            ),
        ],
        Kind::NoiseTexture => {
            // VEGAS's presets: Noise style, Number of layers, Frequency X
            // and Y, Offset X and Y, Progress, Min, Max, Bias, Amplitude,
            // Grain; then Color 1 and Color 2.
            let names = [
                "style",
                "layers",
                "frequencyX",
                "frequencyY",
                "offsetX",
                "offsetY",
                "progress",
                "min",
                "max",
                "bias",
                "amplitude",
                "grain",
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
            one("SMPTE Bars (NTSC)", "pattern", 5.0),
            one("SMPTE Bars (PAL)", "pattern", 6.0),
            one("Color Bars (NTSC)", "pattern", 0.0),
            one("Color Bars (PAL)", "pattern", 1.0),
            one("White Porches", "pattern", 7.0),
            one("Yellow Porches", "pattern", 8.0),
            one("Pluge and Porches", "pattern", 3.0),
            one("Ramp", "pattern", 4.0),
            one("Gradient", "pattern", 2.0),
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
                let paint = kind.painter(&vals(kind, p), &g);
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
        let p = presets(Kind::Checkerboard).remove(0);
        let paint = Kind::Checkerboard.painter(&vals(Kind::Checkerboard, &p), &g);
        // Tiles a tenth of the width, square (6.4 pixels): neighbours
        // across differ, diagonal ones match (tiles' middles).
        let (a, b) = (paint(0.55, 16.0 / 36.0), paint(0.45, 16.0 / 36.0));
        assert!((a[0] - b[0]).abs() > 0.9, "{a:?} {b:?}");
        let c = paint(0.45, 9.6 / 36.0);
        assert!((a[0] - c[0]).abs() < 1e-6, "{a:?} {c:?}");
        // VEGAS's twelve presets; Large Tiles is black and white.
        assert_eq!(presets(Kind::Checkerboard).len(), 12);
        assert_eq!(paint(0.05, 0.05)[..3], [0.0, 0.0, 0.0]);
        // On an edge the colors are half and half, with the blending
        // going straight across Edge blending's part of the tile.
        let s = Vals::of(
            Kind::Checkerboard.specs(),
            &|spec| match spec.name().to_bytes() {
                b"blendHorizontal" => vec![1.0],
                _ => spec.default(),
            },
        );
        let paint = Kind::Checkerboard.painter(&s, &g);
        let at = |x: f64| paint(x / 64.0, 16.0 / 36.0)[0];
        assert!((at(6.4) - 0.5).abs() < 1e-5, "{}", at(6.4));
        assert!(
            (at(4.8) - 0.75).abs() < 1e-5 || (at(4.8) - 0.25).abs() < 1e-5,
            "{}",
            at(4.8)
        );
    }

    #[test]
    fn the_gradient_goes_from_point_to_point() {
        let g = grid();
        let p = presets(Kind::ColorGradient).remove(0);
        let s = vals(Kind::ColorGradient, &p);
        let paint = Kind::ColorGradient.painter(&s, &g);
        // Left of the first point its color, right of the last the other's.
        let (l, r) = (paint(0.0, 0.5), paint(1.0, 0.5));
        let (c1, c2) = (s.rgba(c"color1"), s.rgba(c"color2"));
        assert!((l[2] - c1[2]).abs() < 1e-4 && (r[0] - c2[0]).abs() < 1e-4);
        // The middle halfway.
        let m = paint(0.5, 0.5);
        assert!((m[0] - (c1[0] + c2[0]) / 2.0).abs() < 1e-3, "{m:?}");
    }

    #[test]
    fn noise_stays_between_its_colors_and_flows_with_progress() {
        let g = grid();
        for style in 0..7 {
            let s = Vals::of(NOISE_TEXTURE, &|spec| {
                if spec.name() == c"style" {
                    vec![f64::from(style)]
                } else {
                    spec.default()
                }
            });
            let paint = Kind::NoiseTexture.painter(&s, &g);
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
                if spec.name() == c"progress" {
                    vec![progress]
                } else {
                    spec.default()
                }
            });
            Kind::NoiseTexture.painter(&s, &g)(0.37, 0.61)
        };
        assert!((at(0.0)[0] - at(0.5)[0]).abs() > 1e-4);
    }

    #[test]
    fn smpte_bars_are_where_eg_1_puts_them() {
        assert_eq!(smpte_bars(0.05, 0.3, 0.75, 0.75), opaque(0.75, 0.75, 0.75));
        assert_eq!(smpte_bars(0.95, 0.3, 0.75, 0.75), opaque(0.0, 0.0, 0.75));
        assert_eq!(smpte_bars(0.95, 0.7, 0.75, 0.75), opaque(0.75, 0.75, 0.75));
        // White under the second bar.
        assert_eq!(smpte_bars(0.25, 0.9, 0.75, 0.75), opaque(1.0, 1.0, 1.0));
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
        let paint = Kind::SolidColor.painter(&vals(Kind::SolidColor, gray), &g);
        assert_eq!(paint(0.3, 0.7), [0.501961, 0.501961, 0.501961, 1.0]);
        let s = Vals::of(Kind::SolidColor.specs(), &|_| vec![1.0, 0.5, 0.0, 0.5]);
        assert_eq!(
            Kind::SolidColor.painter(&s, &g)(0.9, 0.1),
            [0.5, 0.25, 0.0, 0.5]
        );
        let s = Vals::of(Kind::SolidColor.specs(), &|spec| spec.default());
        assert_eq!(Kind::SolidColor.painter(&s, &g)(0.5, 0.5), [0.0; 4]);
    }

    /// VEGAS's list of patterns, its gradient and its ramp.
    #[test]
    fn the_test_patterns_are_vegass() {
        let g = grid();
        let paint = |pattern: f64| {
            let s = Vals::of(Kind::TestPattern.specs(), &|_| vec![pattern]);
            Kind::TestPattern.painter(&s, &g)
        };
        assert_eq!(presets(Kind::TestPattern).len(), 9);
        // Color bars: NTSC's seven at 75 %, PAL's eight with 100 % white.
        assert_eq!(paint(0.0)(0.05, 0.5), opaque(0.75, 0.75, 0.75));
        assert_eq!(paint(1.0)(0.05, 0.5), opaque(1.0, 1.0, 1.0));
        assert_eq!(paint(1.0)(0.95, 0.5), opaque(0.0, 0.0, 0.0));
        // Gradient: red to green across, blue toward the bottom.
        assert_eq!(paint(2.0)(0.25, 0.75), opaque(0.75, 0.25, 0.75));
        // Ramp: gray across, black within a tenth of the edges.
        assert_eq!(paint(4.0)(0.5, 0.5), opaque(0.5, 0.5, 0.5));
        assert_eq!(paint(4.0)(0.5, 0.05), opaque(0.0, 0.0, 0.0));
    }
}
