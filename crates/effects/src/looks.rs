//! The color, light, film and texture effects of [`crate::filters`]'
//! table: LAB Adjust, Soft Contrast, Gradient Map, Fill Light, Lens Flare,
//! Starburst, Glint, Light Rays, Film Effects, Film Grain, Newsprint, TV
//! Simulator, Bump Map, Denoise and Looks (after VEGAS's AutoLooks, with
//! looks of BananaV's own) — their settings, as VEGAS's Help describes
//! them, their presets and their pictures, which follow the Help's words
//! until VEGAS's pictures of them are measured. Where VEGAS's control is a
//! list of its own (Gradient Map's control points, Film Effects' particle
//! types with an amount each), a few fixed settings stand in for it.

use std::f64::consts::FRAC_PI_4;
use std::ffi::{CStr, c_int};

use crate::filters::{Kind, premultiplied, straight};
use crate::kit::{Buf, Grid, Spec, Vals, hash01, luma, smooth, step8};
use crate::{Preset, PresetValue};

pub(crate) const LAB_ADJUST: &[Spec] = &[
    Spec::Number(
        c"lightness",
        c"Lightness",
        1.0,
        0.0,
        2.0,
        c"The lightness times this: below 1 darker, above 1 lighter.",
    ),
    Spec::Number(
        c"channelA",
        c"Channel a",
        0.0,
        -1.0,
        1.0,
        c"Greens and magentas toward green (left) or magenta (right).",
    ),
    Spec::Number(
        c"channelB",
        c"Channel b",
        0.0,
        -1.0,
        1.0,
        c"Blues and yellows toward blue (left) or yellow (right).",
    ),
    Spec::Toggle(
        c"invertLuminance",
        c"Invert Luminance",
        false,
        c"The lightness turned around, the colors kept: a negative.",
    ),
    Spec::Number(
        c"desaturate",
        c"Desaturate",
        0.0,
        0.0,
        1.0,
        c"Toward gray: 1 takes all the color away.",
    ),
];
pub(crate) const SOFT_CONTRAST: &[Spec] = &[
    Spec::Number(
        c"stretchRange",
        c"Stretch range",
        17.8,
        0.0,
        100.0,
        c"The bright end stretched up, the dark parts not lifted.",
    ),
    Spec::Number(
        c"contrast",
        c"Contrast",
        66.53,
        0.0,
        100.0,
        c"How much softer, finer contrast; the settings below follow it.",
    ),
    Spec::Number(
        c"diffusion",
        c"Diffusion",
        15.25,
        0.0,
        100.0,
        c"A glow screened in by the highlights' brightness: the highlights soften.",
    ),
    Spec::Number(
        c"lowTrim",
        c"Low trim",
        0.0,
        0.0,
        100.0,
        c"The shadows lifted.",
    ),
    Spec::Number(
        c"highTrim",
        c"High trim",
        0.0,
        0.0,
        100.0,
        c"The highlights held down.",
    ),
    Spec::Number(
        c"tint",
        c"Tint",
        0.0,
        0.0,
        100.0,
        c"How much the brighter parts take Correction hue's color.",
    ),
    Spec::Number(
        c"correctionHue",
        c"Correction hue",
        0.0,
        0.0,
        360.0,
        c"The tint's hue, in degrees.",
    ),
    Spec::Group(c"vignette", c"Vignette"),
    Spec::Choice(
        c"exteriorEffect",
        c"Exterior effect",
        &[c"None", c"Blur", c"Black", c"White", c"Transparent"],
        0,
        c"What is outside the vignette: blurred, black, white or transparent.",
    ),
    Spec::Number(
        c"strength",
        c"Strength",
        100.0,
        0.0,
        100.0,
        c"How much of the outside effect shows, in percent.",
    ),
    Spec::Choice(
        c"shape",
        c"Shape",
        &[c"Rectangle", c"Ellipse"],
        0,
        c"The vignette's shape.",
    ),
    Spec::Number(
        c"softness",
        c"Softness",
        20.0,
        0.0,
        100.0,
        c"How softly the vignette's edge goes over, in percent.",
    ),
    Spec::Number(
        c"width",
        c"Width",
        80.0,
        0.0,
        100.0,
        c"The vignette's width, in percent of the frame's.",
    ),
    Spec::Number(
        c"height",
        c"Height",
        80.0,
        0.0,
        100.0,
        c"The vignette's height, in percent of the frame's.",
    ),
    Spec::Number(
        c"horizontalBlur",
        c"Horizontal blur",
        0.0,
        0.0,
        100.0,
        c"How much Blur blurs across, in percent.",
    ),
    Spec::Number(
        c"verticalBlur",
        c"Vertical blur",
        0.0,
        0.0,
        100.0,
        c"How much Blur blurs up and down, in percent.",
    ),
    Spec::Number(
        c"cornerRadius",
        c"Corner radius",
        0.0,
        0.0,
        100.0,
        c"The rectangle's corners rounded, in percent.",
    ),
    Spec::Point(
        c"position",
        c"Position",
        [0.5, 0.5],
        c"The vignette's middle.",
    ),
    Spec::GroupEnd,
];
pub(crate) const FILL_LIGHT: &[Spec] = &[
    Spec::Number(
        c"hue",
        c"Hue",
        33.42,
        0.0,
        360.0,
        c"The hue of the light's color, which Tint brings in.",
    ),
    Spec::Number(
        c"strength",
        c"Strength",
        52.13,
        0.0,
        100.0,
        c"How much light is added.",
    ),
    Spec::Number(
        c"bias",
        c"Bias",
        0.0,
        -100.0,
        100.0,
        c"Which tones get the most light: below 0 darker ones, above 0 lighter ones.",
    ),
    Spec::Number(
        c"tint",
        c"Tint",
        0.0,
        0.0,
        100.0,
        c"How much of the hue's color the light takes.",
    ),
    Spec::Number(
        c"softness",
        c"Softness",
        3.33,
        0.0,
        100.0,
        c"How far the light spreads around the dark parts.",
    ),
];
pub(crate) const LENS_FLARE: &[Spec] = &[
    Spec::Rgb(
        c"lightColor",
        c"Light color",
        [1.0, 1.0, 1.0],
        c"The light's color, which Tint gives the flare.",
    ),
    Spec::Pair(
        c"lightPosition",
        c"Light position",
        [0.5, -0.5],
        [-1.0, 1.0],
        [-1.0, 1.0],
        c"Where the light is: the frame's middle 0, 0, its edges -1 and 1, up positive.",
    ),
    Spec::Choice(
        c"lensType",
        c"Lens type",
        &[
            c"35mm (9 elements)",
            c"105mm (23 elements)",
            c"70-300mm (16 elements)",
            c"Standard (13 elements)",
            c"Sunburst (8 elements)",
        ],
        0,
        c"The lens whose reflections the flare shows.",
    ),
    Spec::Number(
        c"tint",
        c"Tint",
        0.0,
        0.0,
        1.0,
        c"How much the light's color tints the flare's elements.",
    ),
    Spec::Number(
        c"intensity",
        c"Intensity",
        0.0,
        0.0,
        2.0,
        c"How bright the flare is.",
    ),
    Spec::Number(
        c"blend",
        c"Blend",
        1.0,
        0.0,
        1.0,
        c"How much of the flare is added to the picture.",
    ),
    Spec::Number(
        c"size",
        c"Size",
        1.0,
        0.0,
        1.0,
        c"How big the flare's elements are.",
    ),
    Spec::Number(
        c"perspective",
        c"Perspective",
        0.0,
        -1.0,
        1.0,
        c"The elements growing or shrinking away from the light.",
    ),
];
pub(crate) const STARBURST: &[Spec] = &[
    Spec::Number(
        c"threshold",
        c"Threshold",
        0.0,
        0.0,
        100.0,
        c"How bright a part must be to burst, in percent: each channel under it is left out.",
    ),
    Spec::Number(
        c"brilliance",
        c"Brilliance",
        878.39,
        100.0,
        2000.0,
        c"How bright the brightest lights shine into their rays.",
    ),
    Spec::Number(
        c"strength",
        c"Strength",
        0.0,
        0.0,
        100.0,
        c"How much of the rays is added to the picture, in percent.",
    ),
    Spec::Number(
        c"radius",
        c"Radius",
        16.63,
        2.0,
        30.0,
        c"How long the rays are, in percent of the picture's longer side.",
    ),
    Spec::Int(
        c"points",
        c"Points",
        4,
        2,
        6,
        c"How many rays leave each light.",
    ),
    Spec::Number(
        c"centerBurst",
        c"Center burst",
        0.0,
        0.0,
        100.0,
        c"Short rays of many lengths around each light, in percent.",
    ),
    Spec::Number(
        c"aspect",
        c"Aspect",
        1.0,
        0.5,
        2.0,
        c"The rays stretched across (above 1) or squeezed.",
    ),
    Spec::Number(
        c"orientation",
        c"Orientation",
        45.0,
        0.0,
        360.0,
        c"The rays turned, in degrees.",
    ),
    Spec::Number(
        c"softenEdges",
        c"Soften edges",
        0.0,
        0.0,
        100.0,
        c"Lights near the frame's edges burst less, in percent.",
    ),
    Spec::Toggle(
        c"reduceFlicker",
        c"Reduce flicker",
        false,
        c"The picture smoothed a little first, against flickering.",
    ),
    Spec::Toggle(
        c"effectOnly",
        c"Effect only",
        false,
        c"Only the rays, on nothing, without the picture.",
    ),
];
pub(crate) const RAYS: &[Spec] = crate::effect_mask::with_mask!(
    [
        Spec::Toggle(
            c"keyAlphaChannel",
            c"Key alpha channel",
            false,
            c"The rays start from the picture's alpha, not its brightness.",
        ),
        Spec::Number(
            c"threshold",
            c"Threshold",
            38.56,
            0.0,
            100.0,
            c"How bright a part must be to send rays, in percent.",
        ),
        Spec::Number(
            c"amount",
            c"Amount",
            38.23,
            0.0,
            100.0,
            c"How long the rays are, in percent of the way to the light.",
        ),
        Spec::Point(
            c"position",
            c"Position",
            [0.5, 0.5],
            c"Where the light is: the rays go out from it.",
        ),
        Spec::Number(
            c"aspect",
            c"Aspect",
            1.0,
            0.25,
            4.0,
            c"The rays stretched across (above 1) or squeezed.",
        ),
        Spec::Number(
            c"boost",
            c"Boost",
            -33.81,
            -100.0,
            100.0,
            c"How bright the rays are: above 0 brighter, below dimmer.",
        ),
        Spec::Number(
            c"peak",
            c"Peak",
            100.0,
            0.0,
            100.0,
            c"How bright the rays may get at most, in percent.",
        ),
        Spec::Number(
            c"noise",
            c"Noise",
            0.0,
            0.0,
            100.0,
            c"Rays of uneven strength around the light, in percent.",
        ),
        Spec::Number(
            c"hue",
            c"Hue",
            211.56,
            0.0,
            360.0,
            c"The rays' color at the light, in degrees.",
        ),
        Spec::Number(
            c"hueSweep",
            c"Hue sweep",
            48.81,
            -240.0,
            240.0,
            c"How far the color turns along the rays, in degrees.",
        ),
        Spec::Number(
            c"saturation",
            c"Saturation",
            39.41,
            0.0,
            100.0,
            c"How colored the rays are, in percent.",
        ),
        Spec::Number(
            c"quality",
            c"Quality",
            0.5,
            0.0,
            1.0,
            c"How many readings along each ray: smoother, slower.",
        ),
        Spec::Toggle(
            c"reduceFlicker",
            c"Reduce flicker",
            false,
            c"The picture smoothed a little first, against flickering.",
        ),
        Spec::Toggle(
            c"effectOnly",
            c"Effect only",
            false,
            c"Only the rays, on nothing, without the picture.",
        ),
    ],
    &[c"Wipe", c"Rectangle", c"Bar", c"Ellipse"]
);
pub(crate) const GLINT: &[Spec] = crate::effect_mask::with_mask!(
    [
        Spec::Number(
            c"threshold",
            c"Threshold",
            67.04,
            0.0,
            100.0,
            c"How bright a part must be to glint, in percent.",
        ),
        Spec::Number(
            c"boost",
            c"Boost",
            44.92,
            0.0,
            100.0,
            c"How bright the glints are, in percent.",
        ),
        Spec::Number(
            c"horizontalRadius",
            c"Horizontal radius",
            62.04,
            0.0,
            100.0,
            c"How far the streaks reach across, in percent.",
        ),
        Spec::Number(
            c"verticalRadius",
            c"Vertical radius",
            62.04,
            0.0,
            100.0,
            c"How far the streaks reach up and down, in percent.",
        ),
        Spec::Number(
            c"hue",
            c"Hue",
            116.78,
            0.0,
            360.0,
            c"The streaks' color, in degrees.",
        ),
        Spec::Number(
            c"hueSweep",
            c"Hue sweep",
            240.0,
            -240.0,
            240.0,
            c"How far the color turns along the streaks, in degrees.",
        ),
        Spec::Number(
            c"saturation",
            c"Saturation",
            79.49,
            0.0,
            100.0,
            c"How colored the streaks are, in percent.",
        ),
        Spec::Number(
            c"orientation",
            c"Orientation",
            0.0,
            0.0,
            360.0,
            c"The streaks turned clockwise, in degrees.",
        ),
        Spec::Int(
            c"streaks",
            c"Streaks",
            3,
            1,
            8,
            c"How many lines of light cross each glint.",
        ),
        Spec::Toggle(
            c"reduceFlicker",
            c"Reduce flicker",
            false,
            c"The picture smoothed a little first, against flickering.",
        ),
        Spec::Toggle(
            c"effectOnly",
            c"Effect only",
            false,
            c"Only the glints, on nothing, without the picture.",
        ),
    ],
    &[c"Rectangle", c"Bar", c"Wipe", c"Ellipse"]
);
pub(crate) const LIGHT_RAYS: &[Spec] = &[
    Spec::Point(
        c"lightSource",
        c"Light source",
        [0.5, 0.5],
        c"Where the light is: the rays go out from it.",
    ),
    Spec::Rgb(
        c"tintColor",
        c"Tint color",
        [1.0, 1.0, 1.0],
        c"Every color of the picture times this one's.",
    ),
    Spec::Group(c"group0", c"Group0"),
    Spec::Number(
        c"sensitivity",
        c"Sensitivity",
        0.6,
        0.0,
        1.0,
        c"How little a ray darker than the picture under it counts.",
    ),
    Spec::Number(
        c"strength",
        c"Strength",
        0.25,
        0.0,
        1.0,
        c"How far the rays reach.",
    ),
    Spec::GroupEnd,
    Spec::Group(c"group1", c"Group1"),
    Spec::Toggle(
        c"boundRadius",
        c"Bound radius",
        false,
        c"The picture kept within an ellipse around the light.",
    ),
    Spec::Number(
        c"radiusX",
        c"Radius X",
        0.5,
        0.0,
        1.0,
        c"The ellipse's half width, a part of the frame's width.",
    ),
    Spec::Number(
        c"radiusY",
        c"Radius Y",
        0.5,
        0.0,
        1.0,
        c"The ellipse's half height, a part of the frame's height.",
    ),
    Spec::Number(
        c"feather",
        c"Feather",
        0.5,
        0.0,
        1.0,
        c"How much of the ellipse fades out toward its edge.",
    ),
    Spec::GroupEnd,
    Spec::Group(c"group2", c"Group2"),
    Spec::Number(
        c"blend",
        c"Blend",
        0.0,
        0.0,
        1.0,
        c"How much of the source is mixed back in.",
    ),
    Spec::Number(
        c"noise",
        c"Noise",
        0.0,
        0.0,
        1.0,
        c"Grain over the picture.",
    ),
    Spec::GroupEnd,
];
pub(crate) const FILM_EFFECTS: &[Spec] = &[
    Spec::Number(
        c"grain",
        c"Grain",
        0.15,
        0.0,
        1.0,
        c"How grainy the film is.",
    ),
    Spec::Number(
        c"tint",
        c"Tint",
        0.35,
        0.0,
        1.0,
        c"How far the picture's colors go toward Color.",
    ),
    Spec::Rgb(
        c"color",
        c"Color",
        [0.352941, 0.717647, 0.749020],
        c"The film's tone.",
    ),
    Spec::Toggle(
        c"convertToGrayscale",
        c"Convert to grayscale",
        true,
        c"Black and white film: each pixel the average of its colors.",
    ),
    Spec::Group(c"enableParticles", c"Enable particles"),
    Spec::Toggle(
        c"enable",
        c"Enable",
        false,
        c"Marks of old film over the picture.",
    ),
    Spec::Choice(
        c"type",
        c"Type",
        &[c"Dust", c"Flicker", c"Hair", c"Jitter", c"Scratches"],
        0,
        c"Specks of dust, the light flickering, hairs, the picture shaking or scratches.",
    ),
    Spec::Number(
        c"amount",
        c"Amount",
        0.0,
        0.0,
        1.0,
        c"How many marks, or how strong.",
    ),
    Spec::GroupEnd,
];
pub(crate) const FILM_GRAIN: &[Spec] = &[
    Spec::Number(
        c"amount",
        c"Amount",
        0.312,
        0.0,
        1.0,
        c"How strong the grain is.",
    ),
    Spec::Number(
        c"granularity",
        c"Granularity",
        0.14,
        0.0,
        1.0,
        c"How big the grains are: at 0, one a 240th of the frame's height.",
    ),
    Spec::Number(
        c"chromaticity",
        c"Chromaticity",
        0.0,
        0.0,
        1.0,
        c"0: the same grain for every color; 1: each color its own.",
    ),
];
pub(crate) const NEWSPRINT: &[Spec] = &[
    Spec::Number(
        c"dotSize",
        c"Dot size",
        0.0,
        0.0,
        1.0,
        c"How big the printing dots are; under 0.15 the picture shows through.",
    ),
    Spec::Choice(
        c"type",
        c"Type",
        &[c"Color", c"Monochrome", c"Offset color"],
        0,
        c"Dots of each cell's color, black dots, or three inks a little apart, as an offset press prints.",
    ),
];
pub(crate) const TV_SIMULATOR: &[Spec] = &[
    Spec::Number(
        c"detailZoom",
        c"Detail zoom",
        0.0,
        0.0,
        1.0,
        c"How large the screen's stripes and lines are.",
    ),
    Spec::Number(
        c"apertureGrill",
        c"Aperture grill",
        0.32,
        0.0,
        1.0,
        c"How visible the screen's stripes of red, green and blue are.",
    ),
    Spec::Number(
        c"interlacing",
        c"Interlacing",
        0.66,
        0.0,
        1.0,
        c"How visible the horizontal line bands are.",
    ),
    Spec::Number(
        c"lineSync",
        c"Line sync",
        1.0,
        0.0,
        1.0,
        c"Lower, the lines are torn across, as with a bad line sync.",
    ),
    Spec::Number(
        c"verticalSync",
        c"Vertical sync",
        1.0,
        0.0,
        1.0,
        c"Lower, the picture rolls, as with a misadjusted hold.",
    ),
    Spec::Number(
        c"scanPhasing",
        c"Scan phasing",
        0.15,
        0.0,
        1.0,
        c"A band of light rolling over the screen.",
    ),
    Spec::Number(
        c"phosphorescence",
        c"Phosphorescence",
        0.9,
        0.0,
        1.0,
        c"The phosphors' glow: blue lifted most, red least.",
    ),
    Spec::Number(
        c"static",
        c"Static",
        0.0,
        0.0,
        1.0,
        c"Snow, as with poor reception.",
    ),
];
pub(crate) const BUMP_MAP: &[Spec] = &[
    Spec::Group(c"location", c"Location"),
    Spec::Triple(
        c"source",
        c"Source",
        [-1.0, 1.0, 3.0],
        [-3.0, -3.0, 0.0],
        [3.0, 3.0, 6.0],
        c"Where the light is: across and up from the frame's middle (1 is half its longer side), and how high.",
    ),
    Spec::Pair(
        c"destination",
        c"Destination",
        [0.0, 0.0],
        [-3.0, 3.0],
        [-3.0, 3.0],
        c"Where a spotlight points, as Source counts it.",
    ),
    Spec::GroupEnd,
    Spec::Choice(
        c"lightType",
        c"Light type",
        &[c"Directional", c"Spotlight", c"Omnidirectional"],
        0,
        c"The light: from one direction, a spot, or all around it.",
    ),
    Spec::Number(
        c"intensity",
        c"Intensity",
        0.25,
        0.0,
        1.0,
        c"How strong the light is.",
    ),
    Spec::Number(
        c"focus",
        c"Focus",
        0.5,
        0.0,
        1.0,
        c"A spotlight's beam: lower, tighter; higher, wider.",
    ),
    Spec::Number(
        c"ambience",
        c"Ambience",
        0.1,
        0.0,
        1.0,
        c"How much light comes from all around.",
    ),
    Spec::Number(
        c"shininess",
        c"Shininess",
        0.5,
        0.0,
        1.0,
        c"How reflective the surface looks.",
    ),
    Spec::Number(
        c"bumpHeight",
        c"Bump height",
        0.5,
        0.0,
        1.0,
        c"How high the bumps are at most.",
    ),
    Spec::Choice(
        c"bumpChannel",
        c"Bump channel",
        &[c"Red", c"Green", c"Blue", c"Alpha"],
        1,
        c"The channel the bumps come from.",
    ),
    Spec::Toggle(
        c"whiteIsHigh",
        c"White is high",
        true,
        c"White parts high and black ones low; off, the other way around.",
    ),
    Spec::Toggle(
        c"flip",
        c"Flip input surfaces",
        false,
        c"As a compositor: swaps the picture and the map.",
    ),
];
pub(crate) const DENOISE: &[Spec] = &[
    Spec::Group(c"noiseReduction", c"Noise Reduction"),
    Spec::Ranged(
        c"luminance",
        c"Luminance",
        3.0,
        [0.0, 100.0],
        [0.0, 20.0],
        c"Reduces luminance noise.",
    ),
    Spec::Ranged(
        c"color",
        c"Chrominance",
        3.0,
        [0.0, 100.0],
        [0.0, 20.0],
        c"Reduces color noise.",
    ),
    Spec::GroupEnd,
    Spec::Group(c"sharpening", c"Sharpening"),
    Spec::Number(
        c"amount",
        c"Amount",
        0.0,
        0.0,
        1.0,
        c"How much the picture is sharpened after its noise is reduced.",
    ),
    Spec::GroupEnd,
];
/// Looks' list, as VEGAS's AutoLooks' starts: none, then the looks —
/// BananaV's own.
const LOOK_NAMES: &[&CStr] = &[
    c"none",
    c"Restored",
    c"Bleach Bypass",
    c"Cross Process",
    c"Warm Summer",
    c"Cold Night",
    c"Faded Film",
    c"High Contrast",
    c"Noir",
    c"Two-Strip Color",
    c"Neon",
];
pub(crate) const LOOKS: &[Spec] = &[
    Spec::Choice(
        c"look",
        c"Look",
        LOOK_NAMES,
        0,
        c"The look: a film's, a genre's or a style's; none leaves the picture as it is.",
    ),
    Spec::Number(
        c"gain",
        c"Strength",
        1.0,
        0.0,
        1.0,
        c"How strong the look is.",
    ),
];

/// The picture through effect `kind` (one of this module's) at settings
/// `s`, at `time` (frames: the noise and the television's rolling move
/// with it).
pub(crate) fn process(kind: Kind, s: &Vals, g: &Grid, src: &Buf, time: f64) -> Buf {
    match kind {
        Kind::LabAdjust => lab_adjust(s, src),
        Kind::SoftContrast => soft_contrast(s, g, src),
        Kind::FillLight => fill_light(s, g, src),
        Kind::LensFlare => lens_flare(s, g, src),
        Kind::Starburst => starburst(s, g, src),
        Kind::Glint => glint(s, g, src),
        Kind::Rays => rays(s, g, src),
        Kind::LightRays => light_rays(s, g, src, time),
        Kind::FilmEffects => film_effects(s, g, src, time),
        Kind::FilmGrain => film_grain(s, g, src, time),
        Kind::Newsprint => newsprint(s, g, src),
        Kind::TvSimulator => tv_simulator(s, g, src, time),
        Kind::BumpMap => bump_map(s, g, src),
        Kind::Denoise => denoise(s, src),
        Kind::Looks => looks(s, src),
        _ => src.clone(),
    }
}

// Small sums.

/// The frame's width in this picture's pixels.
fn width_px(g: &Grid) -> f64 {
    g.frame[0] * g.scale[0] / g.par
}

/// A seed of its own for each time.
fn seed_of(time: f64) -> u32 {
    let bits = time.to_bits();
    (bits ^ (bits >> 32)) as u32
}

/// A fully saturated color of hue `deg` (red 0°, green 120°, blue 240°).
fn hue_color(deg: f64) -> [f32; 3] {
    let h = deg.rem_euclid(360.0) / 60.0;
    let x = 1.0 - ((h % 2.0) - 1.0).abs();
    let (r, g, b) = match h as i32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    [r as f32, g as f32, b as f32]
}

/// A color's BT.709 luma.
fn luma3(c: [f32; 3]) -> f32 {
    luma([c[0], c[1], c[2], 1.0])
}

/// A picture's values (video's curve, as sRGB's) to light, and back.
fn to_light(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn to_value(v: f64) -> f64 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// D65's white in CIE XYZ.
const WHITE: [f64; 3] = [0.950_47, 1.0, 1.088_83];

/// CIE L*a*b* (D65) of a color's values.
pub(crate) fn lab(c: [f32; 3]) -> [f64; 3] {
    let [r, g, b] = c.map(|v| to_light(f64::from(v)));
    let x = (0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b) / WHITE[0];
    let y = (0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b) / WHITE[1];
    let z = (0.019_333_9 * r + 0.119_192 * g + 0.950_304_1 * b) / WHITE[2];
    let f = |t: f64| {
        if t > 216.0 / 24_389.0 {
            t.cbrt()
        } else {
            (24_389.0 / 27.0 * t + 16.0) / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// A color's values from CIE L*a*b* (D65).
pub(crate) fn from_lab([l, a, b]: [f64; 3]) -> [f32; 3] {
    let fy = (l + 16.0) / 116.0;
    let (fx, fz) = (fy + a / 500.0, fy - b / 200.0);
    let g = |t: f64| {
        if t > 6.0 / 29.0 {
            t * t * t
        } else {
            (116.0 * t - 16.0) * 27.0 / 24_389.0
        }
    };
    let (x, y, z) = (g(fx) * WHITE[0], g(fy) * WHITE[1], g(fz) * WHITE[2]);
    let r = 3.240_454_2 * x - 1.537_138_5 * y - 0.498_531_4 * z;
    let gr = -0.969_266 * x + 1.876_010_8 * y + 0.041_556 * z;
    let bl = 0.055_643_4 * x - 0.204_025_9 * y + 1.057_225_2 * z;
    [r, gr, bl].map(|v| to_value(v) as f32)
}

// The effects.

/// VEGAS's LAB Adjust: in L*a*b*, the lightness turned around (white's
/// less it) when asked, then scaled by Lightness; a and b moved by 100
/// times Channel a and b, then scaled toward gray by Desaturate. A pixel
/// of no alpha comes out empty, alpha past 1 as 1.
fn lab_adjust(s: &Vals, src: &Buf) -> Buf {
    let [lightness, da, db, keep] = lab_settings(s);
    let invert = s.b(c"invertLuminance");
    if lab_unchanged(s) {
        return src.clone();
    }
    // White's lightness.
    const WHITE_L: f64 = 100.0;
    src.map(|x, y| {
        let c = src.at(x, y);
        if c[3] <= 1e-6 {
            return [0.0; 4];
        }
        let alpha = c[3].min(1.0);
        let [l, a, b] = lab([c[0] / alpha, c[1] / alpha, c[2] / alpha]);
        let l = if invert { WHITE_L - l } else { l } * lightness;
        let rgb = from_lab([l, (a + da) * keep, (b + db) * keep]);
        premultiplied([rgb[0], rgb[1], rgb[2], alpha])
    })
}

/// LAB Adjust's Lightness, a and b moves (100 times Channel a and b)
/// and what Desaturate keeps of a and b.
fn lab_settings(s: &Vals) -> [f64; 4] {
    [
        s.f(c"lightness").clamp(0.0, 2.0),
        s.f(c"channelA").clamp(-1.0, 1.0) * 100.0,
        s.f(c"channelB").clamp(-1.0, 1.0) * 100.0,
        1.0 - s.f(c"desaturate").clamp(0.0, 1.0),
    ]
}

/// Whether LAB Adjust leaves the picture as it is.
fn lab_unchanged(s: &Vals) -> bool {
    lab_settings(s) == [1.0, 0.0, 0.0, 1.0] && !s.b(c"invertLuminance")
}

/// LAB Adjust on the card: the same steps in the `kit_lab_adjust` kernel.
pub(crate) fn lab_plan(s: &Vals) -> crate::accel::Plan {
    if lab_unchanged(s) {
        return crate::accel::Plan::Same;
    }
    crate::accel::Plan::Pixel {
        kernel: crate::accel::PixelKernel::LabAdjust,
        f: lab_settings(s).map(|v| v as f32).to_vec(),
        j: [s.b(c"invertLuminance") as u32, 0, 0, 0],
    }
}

/// Soft Contrast at settings `s`: the luma's curve (Contrast, Stretch
/// range, the trims), the tint's cast and strength, Diffusion and its
/// glow's radius (a part of the frame's width), and the vignette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SoftContrast {
    contrast: f32,
    stretch: f32,
    low: f32,
    high: f32,
    tint: f32,
    cast: [f32; 3],
    diffusion: f32,
    glow: f64,
    vignette: Option<Vignette>,
}

/// Soft Contrast's vignette: what is outside it (1 Blur, 2 Black, 3
/// White, 4 Transparent), Strength, Softness, its size and middle (0–1 of
/// the frame, rows from the top), rectangle or ellipse, its corners'
/// radius and the blur's radii across and down (parts of the width).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Vignette {
    exterior: u32,
    level: f32,
    softness: f32,
    size: [f32; 2],
    middle: [f32; 2],
    rect: bool,
    corner: f32,
    blur: [f64; 2],
}

impl SoftContrast {
    pub(crate) fn of(s: &Vals) -> SoftContrast {
        let pc = |name: &CStr| (s.f(name).clamp(0.0, 100.0) * 0.01) as f32;
        let contrast = pc(c"contrast");
        let hue = hue_color(s.f(c"correctionHue"));
        let exterior = s.i(c"exteriorEffect").clamp(0, 4) as u32;
        let vignette = (exterior > 0).then(|| {
            let [cx, cy] = s.point(c"position");
            Vignette {
                exterior,
                level: pc(c"strength"),
                softness: pc(c"softness"),
                size: [pc(c"width").max(0.01), pc(c"height").max(0.01)],
                middle: [cx as f32, cy as f32],
                rect: s.i(c"shape") == 0,
                corner: pc(c"cornerRadius").min(0.999),
                blur: [
                    f64::from(pc(c"horizontalBlur")) * 0.03,
                    f64::from(pc(c"verticalBlur")) * 0.03,
                ],
            }
        });
        SoftContrast {
            contrast,
            stretch: pc(c"stretchRange"),
            // The settings after Contrast follow it.
            low: pc(c"lowTrim") * contrast,
            high: pc(c"highTrim") * contrast,
            tint: pc(c"tint") * contrast,
            // The cast: the hue's color less its luma, so the brightness
            // stays.
            cast: hue.map(|v| v - luma3(hue)),
            diffusion: pc(c"diffusion") * contrast,
            glow: 0.015,
            vignette,
        }
    }

    /// The luma's curve: an S of Contrast, the top stretched, the trims
    /// lifting the shadows and holding the highlights; black and white
    /// stay where they are.
    fn curve(&self, l: f32) -> f32 {
        let t = l.clamp(0.0, 1.0);
        l + self.contrast * 0.5 * (t * t * (3.0 - 2.0 * t) - t)
            + self.stretch * 0.4 * t * t * t
            + self.low * 1.5 * t * (1.0 - t) * (1.0 - t)
            - self.high * 1.5 * t * t * (1.0 - t)
    }

    /// A premultiplied pixel through the curve and the cast.
    fn toned(&self, px: [f32; 4]) -> [f32; 4] {
        let c = straight(px);
        let l = luma(c);
        let d = self.curve(l) - l;
        let k = self.tint * 0.3 * l.clamp(0.0, 1.0);
        premultiplied([
            c[0] + d + self.cast[0] * k,
            c[1] + d + self.cast[1] * k,
            c[2] + d + self.cast[2] * k,
            c[3],
        ])
    }

    /// The card's numbers for the tone: Contrast, Stretch range, the trims,
    /// the tint's strength and cast.
    fn tone_numbers(&self) -> Vec<f32> {
        vec![
            self.contrast,
            self.stretch,
            self.low,
            self.high,
            self.tint,
            self.cast[0],
            self.cast[1],
            self.cast[2],
        ]
    }
}

/// Diffusion: the picture screened by its glow `b`, as much as the glow's
/// brightness times Diffusion — the highlights soften.
fn diffused(c: [f32; 4], b: [f32; 4], diffusion: f32) -> [f32; 4] {
    let w = luma(b).clamp(0.0, 1.0) * diffusion;
    [
        c[0] + (b[0] - c[0] * b[0]).max(0.0) * w,
        c[1] + (b[1] - c[1] * b[1]).max(0.0) * w,
        c[2] + (b[2] - c[2] * b[2]).max(0.0) * w,
        c[3],
    ]
}

impl Vignette {
    /// How much of what is outside shows at `u`, `v` (0–1, rows from the
    /// top): 0 inside, Strength from its edge out over Softness.
    fn share(&self, u: f64, v: f64) -> f32 {
        let (dx, dy) = (
            ((u - f64::from(self.middle[0])) / (f64::from(self.size[0]) / 2.0)).abs(),
            ((v - f64::from(self.middle[1])) / (f64::from(self.size[1]) / 2.0)).abs(),
        );
        let r = f64::from(self.corner);
        // How far out, 1 at the vignette's edge.
        let d = if self.rect {
            let (qx, qy) = (dx - (1.0 - r), dy - (1.0 - r));
            if qx > 0.0 && qy > 0.0 {
                (1.0 - r) + qx.hypot(qy)
            } else {
                dx.max(dy)
            }
        } else {
            dx.hypot(dy)
        };
        smooth(1.0 - f64::from(self.softness) * 0.5, 1.0 + 1e-9, d) as f32 * self.level
    }

    /// Pixel `c` with what is outside (`b`: the blurred picture) by `k`.
    fn outside(&self, c: [f32; 4], b: [f32; 4], k: f32) -> [f32; 4] {
        match self.exterior {
            1 => std::array::from_fn(|i| c[i] + (b[i] - c[i]) * k),
            2 => [c[0] * (1.0 - k), c[1] * (1.0 - k), c[2] * (1.0 - k), c[3]],
            3 => [
                c[0] + (c[3] - c[0]) * k,
                c[1] + (c[3] - c[1]) * k,
                c[2] + (c[3] - c[2]) * k,
                c[3],
            ],
            _ => c.map(|v| v * (1.0 - k)),
        }
    }

    /// The card's numbers: what is outside, Strength, Softness, the size,
    /// the middle, the corners' radius, rectangle or ellipse.
    fn numbers(&self) -> Vec<f32> {
        vec![
            self.level,
            self.softness,
            self.size[0],
            self.size[1],
            self.middle[0],
            self.middle[1],
            self.corner,
            f32::from(u8::from(self.rect)),
            self.exterior as f32,
        ]
    }
}

/// VEGAS's Soft Contrast: the luma's curve and the tint (`SoftContrast`),
/// Diffusion's glow screened in — the glow blurred as VEGAS blurs
/// (`recursive_gauss`) —, then the vignette's outside: blurred (across
/// and down), black, white or transparent.
fn soft_contrast(s: &Vals, g: &Grid, src: &Buf) -> Buf {
    let sc = SoftContrast::of(s);
    let wpx = width_px(g);
    let toned = src.map(|x, y| sc.toned(src.at(x, y)));
    let diffused_picture = if sc.diffusion > 0.0 {
        let mut glow = toned.clone();
        crate::recursive_gauss::blur(&mut glow, [sc.glow * wpx; 2]);
        toned.map(|x, y| diffused(toned.at(x, y), glow.at(x, y), sc.diffusion))
    } else {
        toned
    };
    let Some(v) = sc.vignette else {
        return diffused_picture;
    };
    let blurred = (v.exterior == 1).then(|| {
        let mut b = diffused_picture.clone();
        crate::recursive_gauss::blur(&mut b, v.blur.map(|r| r * wpx));
        b
    });
    diffused_picture.map(|x, y| {
        let (u, w) = g.uv(x, y);
        let c = diffused_picture.at(x, y);
        let b = blurred.as_ref().map_or(c, |b| b.at(x, y));
        v.outside(c, b, v.share(u, w))
    })
}

/// Soft Contrast on the card: the tone (`kit_soft_tone`), the glow
/// (VEGAS's Gaussian) screened in (`kit_soft_diffuse`), the vignette
/// (`kit_soft_vignette`, over the picture blurred for Blur); the blurs'
/// radii parts of the picture's width.
pub(crate) fn soft_contrast_plan(s: &Vals) -> crate::accel::Plan {
    let sc = SoftContrast::of(s);
    crate::accel::Plan::SoftContrast {
        tone: sc.tone_numbers(),
        diffusion: (sc.diffusion > 0.0).then_some((sc.diffusion, sc.glow)),
        vignette: sc
            .vignette
            .map(|v| (v.numbers(), (v.exterior == 1).then_some(v.blur))),
    }
}

/// VEGAS's Rays (BananaV's math: VEGAS draws them on the card in code of
/// its own): the lights — the picture's brightness, or its alpha, past
/// Threshold — streaming away from Position for Amount of the way, as
/// bright as Boost says up to Peak, uneven by Noise, colored by Hue along
/// Hue sweep at Saturation; Quality readings along each ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Beams {
    key_alpha: bool,
    threshold: f32,
    amount: f32,
    light: [f32; 2],
    aspect: f32,
    gain: f32,
    peak: f32,
    noise: f32,
    hue: f32,
    sweep: f32,
    saturation: f32,
    samples: u32,
    pub(crate) smooth: bool,
    effect_only: bool,
    pub(crate) mask: crate::effect_mask::Mask,
}

impl Beams {
    pub(crate) fn of(s: &Vals) -> Beams {
        let pc = |name: &CStr| (s.f(name).clamp(0.0, 100.0) * 0.01) as f32;
        let [lx, ly] = s.point(c"position");
        Beams {
            key_alpha: s.b(c"keyAlphaChannel"),
            threshold: pc(c"threshold"),
            amount: pc(c"amount"),
            light: [lx as f32, ly as f32],
            aspect: s.f(c"aspect").clamp(0.25, 4.0) as f32,
            gain: (s.f(c"boost").clamp(-100.0, 100.0) / 25.0).exp2() as f32,
            peak: pc(c"peak"),
            noise: pc(c"noise"),
            hue: s.f(c"hue") as f32,
            sweep: s.f(c"hueSweep").clamp(-240.0, 240.0) as f32,
            saturation: pc(c"saturation"),
            samples: 4 + (s.f(c"quality").clamp(0.0, 1.0) * 60.0).round() as u32,
            smooth: s.b(c"reduceFlicker"),
            effect_only: s.b(c"effectOnly"),
            // VEGAS's list: Wipe, Rectangle, Bar, Ellipse.
            mask: crate::effect_mask::Mask::of(s, [2, 0, 1, 3]),
        }
    }

    /// A premultiplied pixel's light: its color by how far past Threshold
    /// its brightness (or alpha) is.
    fn light_of(&self, c: [f32; 4]) -> [f32; 4] {
        let level = if self.key_alpha {
            c[3]
        } else {
            luma(straight(c))
        };
        let k = ((level - self.threshold) / (1.0 - self.threshold).max(1e-4)).clamp(0.0, 1.0);
        c.map(|v| v * k)
    }

    /// The card's numbers: the light's place, Amount, Aspect, the gain,
    /// Peak, Noise, Hue, Hue sweep, Saturation, Threshold, then the mask's
    /// numbers.
    pub(crate) fn numbers(&self) -> Vec<f32> {
        let mut f = vec![
            self.light[0],
            self.light[1],
            self.amount,
            self.aspect,
            self.gain,
            self.peak,
            self.noise,
            self.hue,
            self.sweep,
            self.saturation,
            self.threshold,
        ];
        f.extend(self.mask.numbers);
        f
    }

    /// The card's flags: the readings, Key alpha channel and Reduce
    /// flicker (j.y's bits), Effect only and the mask's mode.
    pub(crate) fn flags(&self) -> [u32; 4] {
        [
            self.samples,
            u32::from(self.key_alpha) | (u32::from(self.smooth) << 1),
            u32::from(self.effect_only),
            self.mask.mode,
        ]
    }
}

/// A ray's strength around the light at `angle` (radians): Noise's
/// uneven rays, 128 of them around, hashed.
fn beam_noise(angle: f32, noise: f32) -> f32 {
    if noise <= 0.0 {
        return 1.0;
    }
    let a = angle.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * 128.0;
    let k = a.floor();
    let f = a - k;
    let k = k as u32 % 128;
    let at = |k: u32| (film_hash(k, 0x7a1d, 0x3c55) & 0xffff) as f32 / 65535.0;
    let r = at(k) * (1.0 - f) + at((k + 1) % 128) * f;
    1.0 - noise * r
}

/// A picture's pixel between pixels (rows from the top), as the card reads
/// it: the four around, its edges held.
fn beam_sample(px: &[[f32; 4]], w: usize, h: usize, x: f32, y: f32) -> [f32; 4] {
    let (fx, fy) = (x - 0.5, y - 0.5);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - x0, fy - y0);
    let at = |i: f32, j: f32| {
        let i = (i as i32).clamp(0, w as i32 - 1) as usize;
        let j = (j as i32).clamp(0, h as i32 - 1) as usize;
        px[j * w + i]
    };
    let (a, b, c, d) = (
        at(x0, y0),
        at(x0 + 1.0, y0),
        at(x0, y0 + 1.0),
        at(x0 + 1.0, y0 + 1.0),
    );
    std::array::from_fn(|k| {
        let top = a[k] + (b[k] - a[k]) * tx;
        let bottom = c[k] + (d[k] - c[k]) * tx;
        top + (bottom - top) * ty
    })
}

/// The rays at pixel `x`, `y` (its middle, rows from the top) of a
/// picture `w` × `h` of `lights`: Quality's readings toward the light over
/// Amount of the way — farther across with Aspect —, their mean
/// brightened by Boost, colored and held under Peak.
fn beams_at(b: &Beams, lights: &[[f32; 4]], w: usize, h: usize, x: f32, y: f32) -> [f32; 4] {
    let (wf, hf) = (w as f32, h as f32);
    let (sx, sy) = (b.light[0] * wf, b.light[1] * hf);
    let (dx, dy) = (sx - x, sy - y);
    let angle = dy.atan2(dx);
    // Farther across with Aspect: the way's part by the direction.
    let along =
        b.amount * ((b.aspect * angle.cos()).hypot(angle.sin()) / b.aspect.max(1.0)).min(1.0);
    let n = b.samples.max(1);
    let mut sum = [0.0f32; 4];
    for k in 0..n {
        let t = along * k as f32 / n as f32;
        let c = beam_sample(lights, w, h, x + dx * t, y + dy * t);
        for (s, v) in sum.iter_mut().zip(c) {
            *s += v;
        }
    }
    let reach = dx.hypot(dy) / wf.hypot(hf);
    let hue = hue_color(f64::from(b.hue + b.sweep * reach));
    let tint = [0, 1, 2].map(|k| 1.0 + (hue[k] - 1.0) * b.saturation);
    let strength = b.gain * beam_noise(angle, b.noise) / n as f32;
    [
        (sum[0] * strength * tint[0]).min(b.peak),
        (sum[1] * strength * tint[1]).min(b.peak),
        (sum[2] * strength * tint[2]).min(b.peak),
        (sum[3] * strength).min(b.peak),
    ]
}

/// VEGAS's Rays (`Beams`): the lights (with the mask on Source), the rays
/// from them added to the picture — or alone with Effect only —, through
/// the mask on Effect, or the mask alone; held within 0–1.
fn rays(s: &Vals, g: &Grid, src: &Buf) -> Buf {
    let b = Beams::of(s);
    let (w, h) = (src.width().max(0) as usize, src.height().max(0) as usize);
    if w == 0 || h == 0 {
        return src.clone();
    }
    let base = if b.smooth {
        crate::filters::binomial(src)
    } else {
        src.clone()
    };
    let aspect = (width_px(g) / g.height_px()) as f32;
    let [x0, y1] = [src.bounds[0], src.bounds[3]];
    // The lights, rows from the top as the card's.
    let mut lights = vec![[0.0f32; 4]; w * h];
    for (j, row) in lights.chunks_mut(w).enumerate() {
        for (i, v) in row.iter_mut().enumerate() {
            let c = b.light_of(base.at(x0 + i as c_int, y1 - 1 - j as c_int));
            let m = if b.mask.mode == 2 {
                b.mask.at(
                    (i as f32 + 0.5) / w as f32,
                    (j as f32 + 0.5) / h as f32,
                    aspect,
                )
            } else {
                1.0
            };
            *v = c.map(|x| x * m);
        }
    }
    src.map(|x, y| {
        let (i, j) = ((x - x0) as f32 + 0.5, (y1 - 1 - y) as f32 + 0.5);
        let m = b.mask.at(i / w as f32, j / h as f32, aspect);
        if b.mask.mode == 3 {
            return [m, m, m, 1.0];
        }
        let r = beams_at(&b, &lights, w, h, i, j);
        let r = if b.mask.mode == 1 {
            r.map(|v| v * m)
        } else {
            r
        };
        let under = if b.effect_only {
            [0.0; 4]
        } else {
            src.at(x, y)
        };
        std::array::from_fn(|k| (under[k] + r[k]).clamp(0.0, 1.0))
    })
}

/// Fill Light, as VEGAS's: light added to the darker tones. Each pixel's
/// BT.601 luma in 8-bit steps picks the light's color from a table — a
/// bump 0.3 high at Bias's tone, nothing at black and from 85 % up, times
/// the light's color —, the light is blurred by Softness (VEGAS's
/// Gaussian, `recursive_gauss`), and half of Strength of it is added, the
/// colors held under white.
fn fill_light(s: &Vals, g: &Grid, src: &Buf) -> Buf {
    let Some(f) = FillLight::of(s) else {
        return src.clone();
    };
    let mut light = src.map(|x, y| f.table[fill_index(src.at(x, y))]);
    if let Some(radii) = f.radii([g.width_px(), g.height_px()], g.pixel_shape()) {
        crate::recursive_gauss::blur(&mut light, radii);
    }
    src.map(|x, y| fill_add(src.at(x, y), light.at(x, y), f.amount))
}

/// Fill Light's numbers, the processor's and the card's: the light for
/// each 8-bit luma (in VEGAS's 8-bit steps), the blur's radius (a part of
/// the frame's longer side; none: 0) and how much of the light is added.
pub(crate) struct FillLight {
    pub(crate) table: Vec<[f32; 4]>,
    pub(crate) reach: f64,
    pub(crate) amount: f32,
}

impl FillLight {
    /// The light settings `s` add; `None` when they add none.
    pub(crate) fn of(s: &Vals) -> Option<FillLight> {
        let strength = s.f(c"strength") * 0.01;
        if strength <= 0.0001 {
            return None;
        }
        let knee = s.f(c"bias") * 0.01 * 0.08 + 0.25;
        let color = fill_color(s.f(c"hue"), s.f(c"tint") * 0.001);
        // The light at 128 tones, then 256 steps straight between them.
        let tones: Vec<[f32; 3]> = (0..128)
            .map(|i| {
                let v = fill_bump(f64::from(i) / 127.0, knee) as f32;
                color.map(|c| c * v)
            })
            .collect();
        let step = |c: f64| ((c * 255.0 + 0.5).floor().clamp(0.0, 255.0) / 255.0) as f32;
        let table = (0..256)
            .map(|j| {
                let x = f64::from(j) * 127.0 / 255.0;
                let i = (x.floor() as usize).min(126);
                let t = x - i as f64;
                let (a, b) = (tones[i], tones[i + 1]);
                let c: [f64; 3] = std::array::from_fn(|k| {
                    f64::from(a[k]) + (f64::from(b[k]) - f64::from(a[k])) * t
                });
                [step(c[0]), step(c[1]), step(c[2]), 0.0]
            })
            .collect();
        // Softness a part of the longer side.
        let reach = s.f(c"softness") * 0.002;
        Some(FillLight {
            table,
            reach: if reach > 0.0001 { reach } else { 0.0 },
            amount: (strength * 0.5) as f32,
        })
    }

    /// The light's blur radii across and down, in pixels, on a picture
    /// `size` pixels whose pixels show `shape` times as wide as tall (none
    /// without Softness): VEGAS's, its reach times the longer side as the
    /// pixels show it, the larger pixels' way shrunk by their shape.
    pub(crate) fn radii(&self, size: [f64; 2], shape: f64) -> Option<[f64; 2]> {
        if self.reach <= 0.0 {
            return None;
        }
        let (a, b) = if shape >= 1.0 {
            (1.0, 1.0 / shape)
        } else {
            (shape, 1.0)
        };
        let r = self.reach * (size[0] * a).max(size[1] * b);
        Some([r * a, r * b])
    }
}

/// Fill Light's bump at tone `x` (0–1): rising from nothing at black to
/// 0.3 at `knee`, back to nothing at 0.85 (cubic Bézier pieces through 0,
/// 0, 0.3, 0.3 and back).
fn fill_bump(x: f64, knee: f64) -> f64 {
    if x < knee {
        let u = x / knee;
        0.3 * u * u * (3.0 - 2.0 * u)
    } else if x <= 0.85 {
        let u = (x - knee) / (0.85 - knee);
        let w = 1.0 - u;
        0.3 * w * w * (1.0 + 2.0 * u)
    } else {
        0.0
    }
}

/// Fill Light's color: gray turned toward red turned by `hue` degrees
/// about the gray axis (its BT.601 luma kept) by `t`, its luma 1.
fn fill_color(hue: f64, t: f64) -> [f32; 3] {
    let (sin, cos) = hue.to_radians().sin_cos();
    // Red's part across the gray axis, turned about it, then lifted along
    // it to red's luma again.
    let r3 = 3f64.sqrt();
    let across = [
        2.0 / 3.0 * cos,
        -cos / 3.0 + sin / r3,
        -cos / 3.0 - sin / r3,
    ];
    let luma = |c: [f64; 3]| 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
    let lift = 0.299 - luma(across);
    std::array::from_fn(|i| ((1.0 - t) + t * (across[i] + lift) / 0.299) as f32)
}

/// A pixel's place in Fill Light's table: its straight color in 8-bit
/// steps, their BT.601 luma in whole numbers.
pub(crate) fn fill_index(p: [f32; 4]) -> usize {
    let c = straight(p);
    let b = crate::kit::step8;
    ((19595 * b(c[0]) + 38469 * b(c[1]) + 7472 * b(c[2]) + 32768) >> 16) as usize
}

/// Pixel `p` with `amount` of `light` added, held under white.
fn fill_add(p: [f32; 4], light: [f32; 4], amount: f32) -> [f32; 4] {
    let c = straight(p);
    premultiplied([
        (c[0] + amount * light[0]).min(1.0),
        (c[1] + amount * light[1]).min(1.0),
        (c[2] + amount * light[2]).min(1.0),
        c[3],
    ])
}

/// A lens's elements: shape, intensity, size, place along
/// the flare's axis (0 the light, 1 the frame's middle, 2 the light
/// mirrored) and color.
type FlareElement = (u32, f32, f32, f32, [f32; 3]);

const LENS_35MM: &[FlareElement] = &[
    (4, 2.0, 0.2, 0.0, [1.0, 1.0, 1.0]),
    (5, 2.0, 0.3, 0.0, [0.67, 0.69, 0.93]),
    (2, 1.2, 0.14, 0.0, [0.16, 0.18, 0.55]),
    (0, 1.0, 0.05, 0.66, [0.48, 0.2, 0.24]),
    (1, 1.0, 0.075, 0.72, [0.48, 0.2, 0.24]),
    (0, 1.2, 0.02, 1.53, [0.44, 0.6, 0.6]),
    (4, 1.0, 0.06, 1.7, [0.42, 0.18, 0.5]),
    (1, 1.0, 0.3, 1.9, [0.28, 0.36, 0.12]),
    (2, 0.6, 0.22, 1.91, [0.11, 0.05, 0.13]),
];
const LENS_105MM: &[FlareElement] = &[
    (1, 1.0, 0.09, -0.35, [0.46, 0.52, 0.5]),
    (1, 1.0, 0.022, -0.95, [0.2, 0.52, 0.34]),
    (1, 1.0, 0.012, -0.97, [0.2, 0.52, 0.34]),
    (4, 0.8, 0.01, -1.01, [0.86, 0.92, 0.44]),
    (4, 1.6, 0.24, 0.0, [1.0, 1.0, 1.0]),
    (5, 2.0, 0.36, 0.0, [0.87, 0.74, 0.7]),
    (2, 1.2, 0.12, 0.0, [0.32, 0.27, 0.27]),
    (1, 0.6, 0.07, 0.29, [0.43, 0.48, 0.54]),
    (4, 1.6, 0.03, 0.66, [0.74, 0.69, 0.47]),
    (0, 1.4, 0.01, 0.92, [0.4, 0.54, 0.56]),
    (1, 0.8, 0.06, 1.46, [0.12, 0.16, 0.2]),
    (0, 1.2, 0.21, 1.46, [0.12, 0.16, 0.2]),
    (1, 1.2, 0.3, 1.46, [0.09, 0.12, 0.15]),
    (0, 1.4, 0.08, 1.59, [0.18, 0.24, 0.3]),
    (1, 1.0, 0.04, 2.13, [0.22, 0.43, 0.51]),
    (0, 1.0, 0.03, 2.17, [0.7, 0.78, 0.82]),
    (1, 1.0, 0.09, 2.19, [0.06, 0.1, 0.12]),
    (1, 1.0, 0.02, 2.33, [0.66, 0.34, 0.16]),
    (0, 1.0, 0.02, 2.36, [0.66, 0.34, 0.16]),
    (1, 1.0, 0.02, 2.4, [0.66, 0.34, 0.16]),
    (1, 1.2, 0.15, 2.84, [0.04, 0.1, 0.06]),
    (1, 1.2, 0.12, 2.86, [0.04, 0.1, 0.06]),
    (0, 1.2, 0.1, 2.99, [0.04, 0.1, 0.06]),
];
const LENS_70_300MM: &[FlareElement] = &[
    (1, 1.0, 0.1, -0.29, [0.19, 0.24, 0.34]),
    (4, 1.6, 0.16, 0.0, [1.0, 1.0, 1.0]),
    (5, 2.0, 0.3, 0.0, [0.67, 0.69, 0.93]),
    (2, 1.2, 0.14, 0.0, [0.16, 0.18, 0.55]),
    (0, 1.2, 0.06, 0.48, [0.39, 0.3, 0.3]),
    (1, 1.0, 0.14, 0.56, [0.26, 0.2, 0.2]),
    (1, 1.0, 0.07, 0.59, [0.26, 0.2, 0.2]),
    (0, 1.2, 0.05, 0.8, [0.14, 0.26, 0.48]),
    (4, 1.2, 0.03, 1.26, [0.46, 0.6, 0.37]),
    (1, 1.0, 0.08, 1.41, [0.01, 0.14, 0.26]),
    (0, 1.2, 0.04, 1.48, [0.01, 0.14, 0.26]),
    (1, 1.0, 0.14, 1.48, [0.01, 0.14, 0.26]),
    (0, 1.0, 0.07, 1.65, [0.18, 0.24, 0.08]),
    (4, 1.2, 0.04, 1.67, [0.41, 0.16, 0.05]),
    (1, 1.0, 0.2, 2.0, [0.01, 0.2, 0.24]),
    (1, 1.2, 0.39, 2.34, [0.08, 0.12, 0.1]),
];
const LENS_STANDARD: &[FlareElement] = &[
    (1, 1.0, 0.08, -0.4, [0.0, 0.15, 0.35]),
    (1, 1.0, 0.1, -0.2, [0.0, 0.15, 0.35]),
    (0, 1.0, 0.1, 0.0, [0.5, 0.5, 0.5]),
    (4, 1.0, 0.3, 0.0, [1.0, 1.0, 1.0]),
    (5, 1.0, 0.4, 0.2, [1.0, 1.0, 1.0]),
    (1, 1.0, 0.2, 0.3, [0.0, 0.15, 0.35]),
    (1, 1.0, 0.09, 0.7, [0.0, 0.35, 0.15]),
    (1, 1.0, 0.05, 1.05, [0.0, 0.15, 0.35]),
    (1, 1.0, 0.07, 1.24, [0.0, 0.15, 0.35]),
    (2, 1.0, 0.13, 1.26, [0.0, 0.15, 0.35]),
    (1, 1.0, 0.07, 1.34, [0.29, 0.2167, 0.1067]),
    (1, 1.0, 0.18, 1.55, [0.29, 0.2167, 0.1067]),
    (3, 1.0, 0.14, 1.9, [0.0, 0.17, 0.07]),
    (1, 1.0, 0.07, 2.25, [0.29, 0.2167, 0.1067]),
];
const LENS_SUNBURST: &[FlareElement] = &[
    (0, 1.0, 0.1, 0.0, [0.5, 0.5, 0.5]),
    (4, 1.0, 0.3, 0.0, [1.0, 1.0, 1.0]),
    (5, 1.0, 0.8, 0.2, [1.0, 1.0, 1.0]),
    (1, 1.0, 0.08, 0.38, [0.4, 0.45, 0.45]),
    (1, 1.0, 0.02, 0.8, [0.4, 0.45, 0.45]),
    (1, 1.0, 0.16, 1.2, [0.4, 0.45, 0.45]),
    (1, 0.8, 0.1, 1.6, [0.4, 0.45, 0.45]),
    (1, 1.0, 0.24, 2.0, [0.4, 0.45, 0.45]),
];

/// A flare's element on the frame: its middle (pixels, rows from the top),
/// its half width and height, its shape and its color (times its
/// intensity and Intensity).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FlareSpot {
    pub at: [f32; 2],
    pub half: [f32; 2],
    pub shape: u32,
    pub color: [f32; 3],
}

/// Lens Flare's settings in the order [`flare_spots`] takes them: Lens
/// type, Light position, Light color, Tint, Intensity, Blend, Size,
/// Perspective.
pub(crate) fn flare_numbers(s: &Vals) -> [f64; 11] {
    let [lx, ly] = s.pair(c"lightPosition");
    let [r, g, b] = s.get_rgb(c"lightColor");
    [
        f64::from(s.i(c"lensType")),
        lx,
        ly,
        r,
        g,
        b,
        s.f(c"tint"),
        s.f(c"intensity"),
        s.f(c"blend"),
        s.f(c"size"),
        s.f(c"perspective"),
    ]
}

/// Lens Flare's elements at a frame `w` × `h` of pixels `par` wide, as
/// VEGAS lays them out: each at the light's place times (1 − its place
/// along the axis) — the frame's middle 0, its edges ±1, up positive —,
/// 2^((its place − 1) × sin(−Perspective × 90°)) × Size × its size × the
/// shorter side big, its color the lens's toward Light color by Tint.
pub(crate) fn flare_spots(v: [f64; 11], w: usize, h: usize, par: f64) -> Vec<FlareSpot> {
    let [kind, lx, ly, r, g, b, tint, intensity, _, size, perspective] = v;
    let lens = match kind as i32 {
        1 => LENS_105MM,
        2 => LENS_70_300MM,
        3 => LENS_STANDARD,
        4 => LENS_SUNBURST,
        _ => LENS_35MM,
    };
    let (lx, ly) = (lx as f32, ly as f32);
    let light = [r, g, b].map(|v| v as f32);
    let tint = tint.clamp(0.0, 1.0) as f32;
    let intensity = intensity as f32;
    let size = size as f32;
    let lean = (-perspective * std::f64::consts::FRAC_PI_2).sin() as f32;
    let (wf, hf) = (w as f32, h as f32);
    let shorter = wf.min(hf);
    lens.iter()
        .map(|&(shape, power, big, t, rgb)| {
            let x = (lx * (1.0 - t)) * wf * 0.5 + wf * 0.5;
            let y = (ly * (1.0 - t)) * hf * 0.5 + hf * 0.5;
            let r = 2f32.powf((t - 1.0) * lean) * size * big * shorter;
            FlareSpot {
                at: [x, hf - y],
                half: [r / par as f32, r],
                shape,
                color: std::array::from_fn(|k| {
                    ((1.0 - tint) * rgb[k] + tint * light[k]) * power * intensity
                }),
            }
        })
        .collect()
}

/// The star rays' lengths around a turn (BananaV's 64 values: VEGAS's
/// table is not known yet).
fn ray_length(k: u32) -> f32 {
    0.35 + 0.65 * (film_hash(k, 0x5ca7, 0x1e75) & 0xffff) as f32 / 65535.0
}

/// How strong a shape is at `u`, `v` (−1 to 1 across its box), `d` from
/// its middle: VEGAS's shapes — a disc, a ring, a halo, the diaphragm's
/// ring, its glow, the star's rays — their profiles BananaV's where
/// VEGAS's are not known yet.
pub(crate) fn flare_shape(shape: u32, u: f32, v: f32, d: f32) -> f32 {
    if d >= 1.0 {
        return 0.0;
    }
    // The diaphragm: a hexagon's distance.
    let hexagon = || {
        let (au, av) = (u.abs(), v.abs());
        (au * 0.866_025_4 + av * 0.5).max(av)
    };
    match shape {
        0 => 1.0 - ((d - 0.8) / 0.2).clamp(0.0, 1.0),
        1 => (1.0 - (d - 0.85).abs() / 0.15).max(0.0),
        2 => (1.0 - d) * (1.0 - d),
        3 => {
            let p = hexagon();
            if p >= 1.0 {
                0.0
            } else if p < 0.2 {
                p / 0.2
            } else {
                (1.0 - p) / 0.8
            }
        }
        4 => {
            let p = hexagon().min(1.0);
            (1.0 - p) * (1.0 - p)
        }
        _ => {
            let a = v.atan2(u).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * 64.0;
            let k = a.floor();
            let f = a - k;
            let k = k as u32 % 64;
            let ray = ray_length(k) * (1.0 - f) + ray_length((k + 1) % 64) * f;
            // Round in the middle 30 %, the rays' lengths beyond.
            let core = (d / 0.3).min(1.0);
            let length = (1.0 - core) + core * ray;
            let r = d / length;
            if r < 1.0 {
                (1.0 - r) * (1.0 - r) * (1.0 - r)
            } else {
                0.0
            }
        }
    }
}

/// The flare's light at pixel `x`, `y` (rows from the top), summed over
/// its elements; `None` where no element reaches.
pub(crate) fn flare_light(spots: &[FlareSpot], x: f32, y: f32) -> Option<[f32; 3]> {
    let mut sum = [0.0f32; 3];
    let mut any = false;
    for s in spots {
        let (u, v) = ((s.at[0] - x) / s.half[0], (s.at[1] - y) / s.half[1]);
        if u.abs() > 1.0 || v.abs() > 1.0 {
            continue;
        }
        any = true;
        let k = flare_shape(s.shape, u, v, (u * u + v * v).sqrt());
        for (total, color) in sum.iter_mut().zip(s.color) {
            *total += color * k;
        }
    }
    any.then_some(sum)
}

/// Lens Flare as VEGAS's: the elements' light added to the picture by
/// Blend, on the colors of what is opaque (transparent pixels under an
/// element go to nothing, as VEGAS's do).
fn lens_flare(s: &Vals, g: &Grid, src: &Buf) -> Buf {
    let (left, bottom, w, h) = frame_pixels(g);
    let numbers = flare_numbers(s);
    let spots = flare_spots(numbers, w, h, g.par);
    let blend = numbers[8].clamp(0.0, 1.0) as f32;
    src.map(|x, y| {
        let c = src.at(x, y);
        let (fx, fy) = (x - left, y - bottom);
        let row = h as c_int - 1 - fy;
        let Some(add) = flare_light(&spots, fx as f32, row as f32) else {
            return c;
        };
        flare_mix(c, add, blend)
    })
}

/// A pixel with a flare's light added by `blend`, as VEGAS adds it.
pub(crate) fn flare_mix(c: [f32; 4], add: [f32; 3], blend: f32) -> [f32; 4] {
    let a = c[3];
    if a <= 1e-6 {
        [0.0; 4]
    } else if a >= 1.0 {
        let k = |i: usize| (c[i] + add[i]) * blend + c[i] * (1.0 - blend);
        [k(0), k(1), k(2), 1.0]
    } else {
        let k = |i: usize| ((c[i] / a + add[i]) * blend + (c[i] / a) * (1.0 - blend)) * a;
        [k(0), k(1), k(2), a]
    }
}

/// Starburst's settings in the order [`Starburst::of`] takes them:
/// Threshold, Brilliance, Strength, Radius, Points, Center burst, Aspect,
/// Orientation, Soften edges, Reduce flicker, Effect only.
pub(crate) fn starburst_numbers(s: &Vals) -> [f64; 11] {
    [
        s.f(c"threshold"),
        s.f(c"brilliance"),
        s.f(c"strength"),
        s.f(c"radius"),
        f64::from(s.i(c"points")),
        s.f(c"centerBurst"),
        s.f(c"aspect"),
        s.f(c"orientation"),
        s.f(c"softenEdges"),
        f64::from(u8::from(s.b(c"reduceFlicker"))),
        f64::from(u8::from(s.b(c"effectOnly"))),
    ]
}

/// The 27 short rays' lengths of VEGAS's center burst (−1 to 1).
const BURST_RAYS: [f32; 27] = [
    0.0,
    -0.866_506_1,
    -0.859_680_95,
    0.549_530_27,
    0.022_278_53,
    0.594_981_13,
    0.654_616_6,
    -0.668_082_4,
    0.472_261_25,
    -0.479_567_26,
    0.012_009_7,
    -0.539_425_55,
    -0.225_719_11,
    -0.568_038_76,
    -0.958_378_26,
    -0.730_332_8,
    0.481_663_47,
    -0.609_817_86,
    0.774_079_14,
    0.722_201_2,
    -0.282_269_87,
    -0.792_595_4,
    0.059_166_1,
    -0.282_489_96,
    0.415_010_96,
    0.160_949_66,
    0.241_096_65,
];

/// The eight long rays' lengths of VEGAS's center burst.
const BURST_SPIKES: [f64; 8] = [
    1.0,
    0.797_071_34,
    1.224_178_2,
    1.063_910_8,
    0.932_929_4,
    0.729_688_17,
    0.798_234_5,
    0.778_814_3,
];

/// VEGAS's Starburst on a picture: the lights' threshold (0–1), the
/// bloom's Brilliance (1–20), Strength (0–1), the rays' radius in pixels
/// (Radius percent of the longer side, the width times the pixels'
/// shape), Points (2–6), Center burst (0–1), Aspect, Orientation
/// (degrees), Soften edges (0–1), Reduce flicker, Effect only, and the
/// picture's size and pixels' shape.
pub(crate) struct Starburst {
    threshold: f64,
    brilliance: f64,
    pub(crate) strength: f64,
    radius: f64,
    points: usize,
    burst: f64,
    aspect: f64,
    orientation: f64,
    soften: f64,
    pub(crate) smooth: bool,
    pub(crate) effect_only: bool,
    size: [f64; 2],
    par: f64,
}

/// A polygon around the middle, its corners in turn (each direction from
/// the middle crossing its outline once): how far out `p` is, 0 in the
/// middle and 1 on the outline (VEGAS's gradients go along it).
fn outward(corners: &[[f64; 2]], p: [f64; 2]) -> f64 {
    let n = corners.len();
    let mut out = f64::INFINITY;
    for k in 0..n {
        let (a, b) = (corners[k], corners[(k + 1) % n]);
        // The direction of `p` between the two corners' (the triangle with
        // the middle holding it): how far along toward their edge.
        let cross = |u: [f64; 2], v: [f64; 2]| u[0] * v[1] - u[1] * v[0];
        if cross(a, p) >= 0.0 && cross(p, b) >= 0.0 {
            let normal = [b[1] - a[1], a[0] - b[0]];
            let reach = normal[0] * a[0] + normal[1] * a[1];
            if reach.abs() > 1e-12 {
                out = out.min((normal[0] * p[0] + normal[1] * p[1]) / reach);
            }
        }
    }
    if p == [0.0, 0.0] { 0.0 } else { out }
}

impl Starburst {
    /// Settings `v` (`starburst_numbers`) on a picture `wpx` × `hpx` pixels
    /// `par` wide.
    pub(crate) fn of(v: [f64; 11], wpx: f64, hpx: f64, par: f64) -> Starburst {
        let [
            threshold,
            brilliance,
            strength,
            radius,
            points,
            burst,
            aspect,
            orientation,
            soften,
            smooth,
            effect_only,
        ] = v;
        let par = if par > 0.0 { par } else { 1.0 };
        Starburst {
            threshold: threshold.clamp(0.0, 100.0) * 0.01,
            brilliance: brilliance.clamp(100.0, 2000.0) * 0.01,
            strength: strength.clamp(0.0, 100.0) * 0.01,
            radius: (wpx * par).max(hpx) * radius.clamp(2.0, 30.0) * 0.01,
            points: (points.round() as usize).clamp(2, 6),
            burst: burst.clamp(0.0, 100.0) * 0.01,
            aspect: aspect.clamp(0.5, 2.0),
            orientation,
            soften: soften.clamp(0.0, 100.0) * 0.01,
            smooth: smooth != 0.0,
            effect_only: effect_only != 0.0,
            size: [wpx, hpx],
            par,
        }
    }

    /// Whether the effect shows at all.
    pub(crate) fn shows(&self) -> bool {
        self.strength > 0.0 || self.effect_only
    }

    /// The lights' bloom: as Defocus's, Brilliance its strength.
    pub(crate) fn bloom(&self) -> Option<[f32; 2]> {
        let t = f64::from(self.threshold as f32);
        (self.threshold < 254.0 / 255.0)
            .then(|| [t as f32, ((self.brilliance - t) / (1.0 - t)) as f32])
    }

    /// The lights' threshold: each channel under it goes.
    pub(crate) fn threshold(&self) -> f32 {
        self.threshold as f32
    }

    /// Soften edges: the lights fade near the frame's edges — inside a
    /// rectangle `1 − Soften edges / 10` of the frame, each way along a
    /// ramp twice the blur's reach wide. Its middle, half sizes and ramp's
    /// half width (pixels); none at 0.
    pub(crate) fn edges(&self) -> Option<[f32; 5]> {
        if self.soften <= 0.0 {
            return None;
        }
        let [w, h] = self.size;
        let keep = (1.0 - self.soften * 0.1) * 0.5;
        let blur = (self.soften * 0.625 * (w * self.par).max(h))
            .sqrt()
            .max(0.5);
        Some(
            [
                (w - 1.0) * 0.5,
                (h - 1.0) * 0.5,
                (w - 1.0) * keep,
                (h - 1.0) * keep,
                blur,
            ]
            .map(|v| v as f32),
        )
    }

    /// The star as VEGAS draws it (none under a pixel): `Points` rays
    /// `2 × trunc(0.586 × radius)` pixels long, the first at Orientation
    /// plus 270°, each a sliver a pixel and a half wide at the middle, its
    /// light 64 of 255 there fading to the tip; with Center
    /// burst, eight rays of set lengths a fifth as long (white in the
    /// middle) mixed in by a quarter of it, then 27 short rays of set
    /// lengths by a fifth of it; Aspect stretches it across. Its side and
    /// the weights (rows up).
    pub(crate) fn star(&self) -> Option<(usize, Vec<f32>)> {
        use std::f64::consts::{PI, TAU};
        let reach = 2.0 * (self.radius * 0.586).trunc();
        if reach < 1.0 {
            return None;
        }
        let r = reach as usize;
        let size = 2 * r + 1;
        let start = (self.orientation + 270.0).rem_euclid(360.0).to_radians();
        let stretch = |p: [f64; 2]| [p[0] * self.aspect, p[1]];
        let at = |a: f64, len: f64| stretch([a.cos() * len, a.sin() * len]);
        let n = self.points;
        let rays: Vec<[f64; 2]> = (0..2 * n)
            .map(|k| {
                let a = start + (k / 2) as f64 * TAU / n as f64;
                if k % 2 == 0 {
                    at(a, reach)
                } else {
                    at(a + PI / n as f64, 0.75)
                }
            })
            .collect();
        let spikes: Vec<[f64; 2]> = (0..16)
            .map(|k| {
                let a = start + (k / 2) as f64 * PI / 4.0;
                if k % 2 == 0 {
                    at(a, reach * 0.2 * BURST_SPIKES[k / 2])
                } else {
                    at(a + PI / 8.0, 7.5)
                }
            })
            .collect();
        let short: Vec<[f64; 2]> = (0..54)
            .map(|k| {
                let a = start + (k / 2) as f64 * TAU / 27.0;
                if k % 2 == 0 {
                    at(a, (f64::from(BURST_RAYS[k / 2]) * 0.75 + 1.0) * reach * 0.4)
                } else {
                    at(a + PI / 27.0, 3.75)
                }
            })
            .collect();
        // A polygon's gradient from `middle` (of 255) to nothing on its
        // outline, as VEGAS's 8-bit drawing keeps it.
        let layer = |corners: &[[f64; 2]], middle: f64, p: [f64; 2]| {
            let out = outward(corners, p);
            if out <= 1.0 {
                (middle * (1.0 - out)).round() / 255.0
            } else {
                0.0
            }
        };
        let (a, b) = (self.burst * 0.25, self.burst * 0.2);
        let mut weights = vec![0.0f32; size * size];
        for (j, row) in weights.chunks_mut(size).enumerate() {
            for (i, w) in row.iter_mut().enumerate() {
                let p = [i as f64 - reach, j as f64 - reach];
                let mut v = layer(&rays, 64.0, p);
                if self.burst > 0.0 {
                    v = a * layer(&spikes, 255.0, p) + (1.0 - a) * v;
                    v = b * layer(&short, 255.0, p) + (1.0 - b) * v;
                }
                *w = v as f32;
            }
        }
        Some((size, weights))
    }
}

/// The lights Starburst bursts, as VEGAS takes them: the picture, faded
/// near the edges with Soften edges, smoothed by the binomial 3 × 3 with
/// Reduce flicker, each channel under Threshold gone.
fn starburst_lights(st: &Starburst, src: &Buf) -> Buf {
    let mut lights = match st.edges() {
        Some(e) => {
            let [x0, y0] = [src.bounds[0], src.bounds[1]];
            src.map(|x, y| {
                let m = edge_mask(e, f64::from(x - x0), f64::from(y - y0));
                src.at(x, y).map(|v| v * m)
            })
        }
        None => src.clone(),
    };
    if st.smooth {
        lights = crate::filters::binomial(&lights);
    }
    let t = st.threshold();
    for c in &mut lights.data {
        for v in c.iter_mut() {
            if *v < t {
                *v = 0.0;
            }
        }
    }
    lights
}

/// Soften edges' mask at pixel `x`, `y` (from the picture's corner): each
/// way a ramp from 1 inside the rectangle to 0 outside it.
pub(crate) fn edge_mask([cx, cy, ax, ay, b]: [f32; 5], x: f64, y: f64) -> f32 {
    let ramp =
        |d: f64, a: f32| ((f64::from(a) - d.abs()) / (2.0 * f64::from(b)) + 0.5).clamp(0.0, 1.0);
    (ramp(x - f64::from(cx), ax) * ramp(y - f64::from(cy), ay)) as f32
}

/// VEGAS's Starburst: the lights (`starburst_lights`), bloomed past the
/// threshold by Brilliance, convolved with the star through the frequency
/// domain (`fft`), Strength of them added to the picture — or alone, on
/// nothing, with Effect only —, held within 0–1.
fn starburst(s: &Vals, g: &Grid, src: &Buf) -> Buf {
    let st = Starburst::of(starburst_numbers(s), width_px(g), g.height_px(), g.par);
    let Some((size, star)) = st.star().filter(|_| st.shows()) else {
        return src.clone();
    };
    let lights = starburst_lights(&st, src);
    let bloom = st.bloom();
    let strength = st.strength as f32;
    let only = st.effect_only;
    crate::fft::convolve(
        &lights,
        &star,
        size,
        &|c| match bloom {
            Some([t, k]) => {
                let up = |v: f32| if v > t { v + (v - t) * k } else { v };
                [up(c[0]), up(c[1]), up(c[2]), c[3]]
            }
            None => c,
        },
        &|x, y, c| {
            let base = if only { [0.0; 4] } else { src.at(x, y) };
            std::array::from_fn(|k| (base[k] + c[k] * strength).clamp(0.0, 1.0))
        },
    )
}

/// VEGAS's Glint (BananaV's math: VEGAS draws it on the card in code of
/// its own): the highlights past Threshold, brightened by Boost, streaked
/// along Streaks lines through each — their reach Horizontal and Vertical
/// radius, turned by Orientation —, the streaks' color from gray toward
/// Hue at Saturation, turning by Hue sweep along them; with VEGAS's Mask
/// group.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glint {
    threshold: f64,
    gain: f32,
    radii: [f64; 2],
    hue: f64,
    sweep: f64,
    saturation: f32,
    orientation: f64,
    streaks: u32,
    pub(crate) smooth: bool,
    pub(crate) effect_only: bool,
    pub(crate) mask: crate::effect_mask::Mask,
}

impl Glint {
    pub(crate) fn of(s: &Vals) -> Glint {
        let pc = |name: &CStr| s.f(name).clamp(0.0, 100.0) * 0.01;
        Glint {
            threshold: pc(c"threshold"),
            gain: (1.0 + pc(c"boost") * 3.0) as f32,
            radii: [pc(c"horizontalRadius"), pc(c"verticalRadius")],
            hue: s.f(c"hue"),
            sweep: s.f(c"hueSweep").clamp(-240.0, 240.0),
            saturation: pc(c"saturation") as f32,
            orientation: s.f(c"orientation"),
            streaks: s.i(c"streaks").clamp(1, 8) as u32,
            smooth: s.b(c"reduceFlicker"),
            effect_only: s.b(c"effectOnly"),
            // VEGAS's list: Rectangle, Bar, Wipe, Ellipse.
            mask: crate::effect_mask::Mask::of(s, [0, 1, 2, 3]),
        }
    }

    /// The light a premultiplied pixel sends along the streaks: its luma
    /// by how far past Threshold it is, times Boost's gain.
    pub(crate) fn light_of(&self, c: [f32; 4]) -> f32 {
        let k = smooth(self.threshold, 1.0, f64::from(luma(straight(c)))) as f32;
        luma(c) * k * self.gain
    }

    /// The card's numbers for the lights: Threshold, the gain, then the
    /// mask's numbers.
    pub(crate) fn light_numbers(&self) -> Vec<f32> {
        let mut f = vec![self.threshold as f32, self.gain];
        f.extend(self.mask.numbers);
        f
    }

    /// The streaks as a kernel (none without reach) on a picture `w` × `h`
    /// pixels: `Streaks` lines through the middle — two rays each, the
    /// first along Orientation (clockwise) —, each ray reaching a quarter of
    /// the frame by the radii, a pixel wide, fading as (1 − t)², its color
    /// at `t` of the way from gray toward the hue Hue sweep turns to, at
    /// Saturation; each ray summing to about one. Its side and the cells:
    /// red and green, blue and gray (rows up).
    pub(crate) fn kernel(&self, w: f64, h: f64) -> Option<(usize, Vec<[f32; 4]>)> {
        use std::f64::consts::PI;
        let rays: Vec<([f64; 2], f64)> = (0..2 * self.streaks)
            .map(|k| {
                let a =
                    -self.orientation.to_radians() + f64::from(k) * PI / f64::from(self.streaks);
                let reach =
                    (a.cos() * self.radii[0] * 0.25 * w).hypot(a.sin() * self.radii[1] * 0.25 * h);
                ([a.cos(), a.sin()], reach)
            })
            .collect();
        let longest = rays.iter().map(|r| r.1).fold(0.0, f64::max);
        if longest < 1.0 {
            return None;
        }
        let r = longest.ceil() as usize;
        let size = 2 * r + 1;
        let mut cells = vec![[0.0f32; 4]; size * size];
        for (j, row) in cells.chunks_mut(size).enumerate() {
            for (i, cell) in row.iter_mut().enumerate() {
                let p = [i as f64 - r as f64, j as f64 - r as f64];
                if p == [0.0, 0.0] {
                    *cell = [1.0; 4];
                    continue;
                }
                for &([ux, uy], reach) in &rays {
                    if reach < 1.0 {
                        continue;
                    }
                    let along = p[0] * ux + p[1] * uy;
                    let t = along / reach;
                    let across = (p[0] * uy - p[1] * ux).abs();
                    if t <= 0.0 || t > 1.0 || across >= 1.0 {
                        continue;
                    }
                    let fall = ((1.0 - across) * (1.0 - t) * (1.0 - t) * 3.0 / reach) as f32;
                    let hue = hue_color(self.hue + self.sweep * t);
                    let l = luma3(hue);
                    let tone = hue.map(|v| 1.0 + (v - l) * self.saturation * 1.5);
                    cell[0] += tone[0] * fall;
                    cell[1] += tone[1] * fall;
                    cell[2] += tone[2] * fall;
                    cell[3] += fall;
                }
            }
        }
        Some((size, cells))
    }
}

/// VEGAS's Glint (`Glint`): the lights (with the mask on Source), their
/// streaks added to the picture — or alone with Effect only —, through the
/// mask on Effect, or the mask alone; held within 0–1.
fn glint(s: &Vals, g: &Grid, src: &Buf) -> Buf {
    let gl = Glint::of(s);
    let (wpx, hpx) = (width_px(g), g.height_px());
    let aspect = (wpx / hpx) as f32;
    let [x0, y1] = [src.bounds[0], src.bounds[3]];
    let (w, h) = (src.width().max(1) as f32, src.height().max(1) as f32);
    let uv = |x: c_int, y: c_int| ((x - x0) as f32 + 0.5, (y1 - 1 - y) as f32 + 0.5);
    let mask_at = |x: c_int, y: c_int| {
        let (i, j) = uv(x, y);
        gl.mask.at(i / w, j / h, aspect)
    };
    if gl.mask.mode == 3 {
        return src.map(|x, y| {
            let m = mask_at(x, y);
            [m, m, m, 1.0]
        });
    }
    let Some((size, kernel)) = gl.kernel(wpx, hpx) else {
        return src.clone();
    };
    let base = if gl.smooth {
        crate::filters::binomial(src)
    } else {
        src.clone()
    };
    let lights = base.map(|x, y| {
        let m = if gl.mask.mode == 2 {
            mask_at(x, y)
        } else {
            1.0
        };
        let l = gl.light_of(base.at(x, y)) * m;
        [l, 0.0, l, 0.0]
    });
    crate::fft::convolve_lanes(&lights, &kernel, size, &|c| c, &|x, y, c| {
        let m = if gl.mask.mode == 1 {
            mask_at(x, y)
        } else {
            1.0
        };
        let under = if gl.effect_only {
            [0.0; 4]
        } else {
            src.at(x, y)
        };
        std::array::from_fn(|k| (under[k] + c[k] * m).clamp(0.0, 1.0))
    })
}

/// VEGAS's Light Rays' numbers for a frame `w` × `h`, as its 8-bit math
/// takes them: the light's column and row (from the top); the rays' weight
/// in 65,536ths — Strength to the power of 75 over the shorter side's
/// pixels, so the rays reach as far at any size —; how much a ray darker
/// than its pixel still counts (256ths, Sensitivity's complement); the
/// bound's ellipse (the reciprocals of its half axes in pixels, where its
/// feather starts and that feather's reciprocal); the noise (255ths), the
/// source blended back (65,536ths) and the tint (256ths); the noise's
/// first number.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Rays {
    pub light: [usize; 2],
    pub weight: i32,
    pub threshold: i32,
    pub bound: bool,
    pub per: [f64; 2],
    pub inner: f64,
    pub feather: f64,
    pub noise: i32,
    pub blend: u32,
    pub tint: [u32; 3],
    pub seed: u32,
}

/// Light Rays' settings in the order [`Rays::new`] takes them.
pub(crate) fn light_rays_numbers(s: &Vals) -> [f64; 13] {
    let [lx, ly] = s.point(c"lightSource");
    let [r, g, b] = s.get_rgb(c"tintColor");
    [
        lx,
        ly,
        r,
        g,
        b,
        s.f(c"sensitivity"),
        s.f(c"strength"),
        f64::from(u8::from(s.b(c"boundRadius"))),
        s.f(c"radiusX"),
        s.f(c"radiusY"),
        s.f(c"feather"),
        s.f(c"blend"),
        s.f(c"noise"),
    ]
}

impl Rays {
    /// The numbers of settings `v` ([`light_rays_numbers`]) for a frame
    /// `w` × `h` at `time`.
    pub(crate) fn new(v: [f64; 13], w: usize, h: usize, time: f64) -> Rays {
        let [
            lx,
            ly,
            r,
            g,
            b,
            sensitivity,
            strength,
            bound,
            rx,
            ry,
            feather,
            blend,
            noise,
        ] = v;
        let unit = |v: f64| v.clamp(0.0, 1.0);
        // The light's pixel; VEGAS's rows go down from the top.
        let place = |v: f64, n: usize| ((n as f64 * unit(v)) as usize).min(n.max(1) - 1);
        let shorter = w.min(h).max(1) as f64;
        let weight = unit(strength).powf(1.0 / (shorter / 300.0)).powf(0.25);
        let inner = 1.0 - unit(feather);
        Rays {
            light: [place(lx, w), place(1.0 - ly, h)],
            weight: (weight * 65536.0) as i64 as i32,
            threshold: ((1.0 - unit(sensitivity)) * 256.0) as i64 as i32,
            bound: bound != 0.0,
            per: [
                1.0 / (w as f64 * unit(rx)).max(1.0),
                1.0 / (h as f64 * unit(ry)).max(1.0),
            ],
            inner,
            feather: if inner == 1.0 {
                0.0
            } else {
                1.0 / (1.0 - inner)
            },
            noise: (unit(noise) * 255.0) as i64 as i32,
            blend: (unit(blend) * 65536.0) as i64 as u32,
            tint: [r, g, b].map(|c| (unit(c) * 256.0) as i64 as u32),
            seed: seed_of(time) & 0xffff,
        }
    }

    /// How far pixel |dx|, |dy| from the light is across the bound's
    /// ellipse (1 its edge), as VEGAS works it out.
    fn reach(&self, ax: usize, ay: usize) -> f64 {
        let (ax, ay) = (ax as f64, ay as f64);
        (ay * self.per[1] * ay * self.per[1] + ax * self.per[0] * ax * self.per[0]).sqrt()
    }

    /// How much of pixel |dx|, |dy| the bound leaves, in 65,536ths.
    pub(crate) fn fade(&self, ax: usize, ay: usize) -> u32 {
        let e = self.reach(ax, ay);
        if e <= self.inner {
            65536
        } else if e < 1.0 {
            (65536 - ((e - self.inner) * self.feather * 65536.0) as i64) as u32
        } else {
            0
        }
    }

    /// For each |dy| below `rows`: the last |dx| the bound leaves whole
    /// (−1: none) and the first it hides (`w`: none) — the card works out
    /// only the pixels between in floats.
    pub(crate) fn bound_rows(&self, w: usize, rows: usize) -> Vec<[i32; 2]> {
        // The first |dx| from which `past` holds (it holds from there on;
        // the reach is never NaN).
        let first = |ay: usize, past: &dyn Fn(f64) -> bool| {
            let (mut lo, mut hi) = (0, w);
            while lo < hi {
                let mid = (lo + hi) / 2;
                if past(self.reach(mid, ay)) {
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
            lo as i32
        };
        (0..rows)
            .map(|ay| {
                let whole = first(ay, &|e| e > self.inner) - 1;
                let hidden = first(ay, &|e| e > self.inner && e >= 1.0);
                [whole, hidden]
            })
            .collect()
    }
}

/// What VEGAS's Light Rays reads past the last column or row (never
/// weighed).
const PAST: [u32; 4] = [0, 0, 255, 0];

/// VEGAS's Light Rays on an 8-bit picture `w` × `h` (premultiplied, rows
/// from the top): from the light out, quarter by quarter — rows outward,
/// each row's pixels outward — each pixel mixed, by the rays' weight (less
/// where the ray is darker than the pixel), with the average of up to three
/// readings a pixel apart toward the light of what is drawn already, the
/// pixel itself first faded by the bound and grained by the noise; then
/// every pixel tinted and the source blended back.
pub(crate) fn rays_drawn(r: &Rays, w: usize, h: usize, src: &[[u8; 4]]) -> Vec<[u8; 4]> {
    let mut dst = src.to_vec();
    let [lx, ly] = r.light;
    let mut noise = r.seed;
    for q in 0..4 {
        let (right, below) = (q % 2 == 1, q >= 2);
        let columns = if right { w - lx - 1 } else { lx + 1 };
        let rows = if below { h - ly - 1 } else { ly + 1 };
        if columns == 0 || rows == 0 {
            continue;
        }
        // The light's column on the quarter's first row starts again as
        // the source, as VEGAS's does.
        let first = if below { ly + 1 } else { ly };
        dst[first * w + lx] = src[first * w + lx];
        for j in 0..rows {
            let y = if below { ly + 1 + j } else { ly - j };
            for i in 0..columns {
                let x = if right { lx + 1 + i } else { lx - i };
                dst[y * w + x] = rays_pixel(r, [w, h], src, &dst, [x, y], q, &mut noise);
            }
        }
    }
    if r.blend != 0 || r.tint.iter().any(|&t| t != 0) {
        for (d, s) in dst.iter_mut().zip(src) {
            let b = u32::from(s[3]) * r.blend / 255;
            let keep = 65536 - b;
            for c in 0..3 {
                let t = ((u32::from(d[c]) * r.tint[c]) >> 8) & 0xff;
                d[c] = ((t * keep + u32::from(s[c]) * b) >> 16) as u8;
            }
            d[3] = ((u32::from(s[3]) * b + u32::from(d[3]) * keep) >> 16) as u8;
        }
    }
    dst
}

/// Pixel `at` of quarter `q` (as VEGAS's renders show): its ray, read in what is
/// drawn, and the pixel mixed with it.
fn rays_pixel(
    r: &Rays,
    [w, h]: [usize; 2],
    src: &[[u8; 4]],
    dst: &[[u8; 4]],
    [x, y]: [usize; 2],
    q: usize,
    noise: &mut u32,
) -> [u8; 4] {
    let [lx, ly] = r.light;
    let (dx, dy) = (lx as i32 - x as i32, ly as i32 - y as i32);
    let own = src[y * w + x].map(u32::from);
    let mine = if r.bound {
        let k = r.fade(dx.unsigned_abs() as usize, dy.unsigned_abs() as usize);
        [
            (own[0] * k) >> 16,
            (own[1] * k) >> 16,
            (own[2] * k) >> 16,
            own[3],
        ]
    } else {
        own
    };
    // Up to three readings toward the light, each a pixel (at most 0.99
    // of one each way) further; fewer next to it.
    let dist = f64::from(dx * dx + dy * dy).sqrt();
    let inv = if dist == 0.0 { 0.0 } else { 1.0 / dist };
    let (mut k, mut part) = (3, 1.0 / 3.0);
    if dx.abs() < 3 && dy.abs() < 3 {
        let d = if dy.abs() < dx.abs() { dx } else { dy };
        k = d.abs();
        part = if k == 0 { 0.0 } else { 1.0 / f64::from(k) };
    }
    let sx = (f64::from(dx) * inv).clamp(-0.99, 0.99);
    let sy = (f64::from(dy) * inv).clamp(-0.99, 0.99);
    let ray = if k == 0 {
        own
    } else {
        let (x0, y0) = (x as f64, y as f64);
        let (mut px, mut py) = (x0, y0);
        let mut sum = [0f64; 4];
        for _ in 0..k {
            px += sx;
            py += sy;
            let (ix, iy) = (px as usize, py as usize);
            let fx = ((px - ix as f64) * 256.0) as u32;
            let fy = ((py - iy as f64) * 256.0) as u32;
            let o = iy * w + ix;
            let (last_x, last_y) = (ix + 1 == w, iy + 1 == h);
            let read = |i: usize| dst[i].map(u32::from);
            let mut c = [
                read(o),
                if last_x { PAST } else { read(o + 1) },
                if last_y { PAST } else { read(o + w) },
                if last_x || last_y {
                    PAST
                } else {
                    read(o + w + 1)
                },
            ];
            // Within a pixel of itself, the pixel's corner is its source.
            if (px - x0).abs() < 1.0 && (py - y0).abs() < 1.0 {
                if px - ix as f64 == 0.0 && py - iy as f64 == 0.0 {
                    c[0] = mine;
                } else {
                    c[q] = mine;
                }
            }
            for (ch, s) in sum.iter_mut().enumerate() {
                let v = ((c[2][ch] * (256 - fx) + c[3][ch] * fx) * fy
                    + (c[1][ch] * fx + c[0][ch] * (256 - fx)) * (256 - fy))
                    >> 16
                    & 0xff;
                *s += f64::from(v) * part;
            }
        }
        sum.map(|v| v as u32)
    };
    // A ray darker than the pixel counts less, by Sensitivity.
    let lum_ray = (ray[0] + ray[1] + ray[2]) / 3;
    let lum = (mine[0] + mine[1] + mine[2]) / 3;
    let mut weight = r.weight;
    if lum_ray < lum {
        weight =
            (lum_ray as i32 - lum as i32 + 256) * ((r.threshold * r.weight) as u32 >> 16) as i32;
    }
    let mut c = mine;
    if r.noise != 0 {
        *noise = (*noise * 25173 + 13849) & 0xffff;
        if lum > 1 {
            let n = ((((*noise * lum * 2) >> 16) as i32 - lum as i32) * r.noise) / 256;
            for v in &mut c[..3] {
                let t = *v as i32;
                *v = if n > 0 {
                    t + (255 - t) * n / 256
                } else {
                    t + n * t / 256
                } as u32;
            }
        }
    }
    let (keep, weight) = ((65536 - weight) as u32, weight as u32);
    [
        ((c[0] * keep + ray[0] * weight) >> 16) as u8,
        ((c[1] * keep + ray[1] * weight) >> 16) as u8,
        ((c[2] * keep + ray[2] * weight) >> 16) as u8,
        ((own[3] * keep + ray[3] * weight) >> 16) as u8,
    ]
}

/// Light Rays as VEGAS's: the frame as its 8-bit picture through
/// [`rays_drawn`].
fn light_rays(s: &Vals, g: &Grid, src: &Buf, time: f64) -> Buf {
    let (left, bottom, w, h) = frame_pixels(g);
    let rays = Rays::new(light_rays_numbers(s), w, h, time);
    // The frame's pixels as 8-bit values, rows from the top.
    let bytes: Vec<[u8; 4]> = (0..w * h)
        .map(|i| {
            let (x, t) = ((i % w) as c_int, (i / w) as c_int);
            let c = src.inside(left + x, bottom + (h as c_int - 1 - t));
            c.map(|v| step8(v) as u8)
        })
        .collect();
    let drawn = rays_drawn(&rays, w, h, &bytes);
    src.map(|x, y| {
        let (fx, fy) = (x - left, y - bottom);
        if fx < 0 || fy < 0 || fx >= w as c_int || fy >= h as c_int {
            return src.at(x, y);
        }
        drawn[(h - 1 - fy as usize) * w + fx as usize].map(|v| f32::from(v) / 255.0)
    })
}

/// A mark of old film over the picture (BananaV's, VEGAS's particles
/// not being known yet): a speck of dust, a
/// hair (an arc) or a scratch (a vertical line), in the frame's pixels
/// (rows from the top), its gray and how opaque.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mark {
    Speck {
        at: [f32; 2],
        r: f32,
        gray: f32,
        opacity: f32,
    },
    Hair {
        at: [f32; 2],
        r: f32,
        from: f32,
        sweep: f32,
        gray: f32,
        opacity: f32,
    },
    Scratch {
        x: f32,
        rows: [f32; 2],
        width: f32,
        gray: f32,
        opacity: f32,
    },
}

impl Mark {
    /// How much of pixel x, y (its middle) the mark covers, times its
    /// opacity.
    fn cover(&self, x: f32, y: f32) -> f32 {
        match *self {
            Mark::Speck { at, r, opacity, .. } => {
                let d = (x - at[0]).hypot(y - at[1]);
                (r + 0.5 - d).clamp(0.0, 1.0) * opacity
            }
            Mark::Hair {
                at,
                r,
                from,
                sweep,
                opacity,
                ..
            } => {
                let (dx, dy) = (x - at[0], y - at[1]);
                let a = (dy.atan2(dx) - from).rem_euclid(std::f32::consts::TAU);
                if a > sweep {
                    return 0.0;
                }
                (0.75 - (dx.hypot(dy) - r).abs()).clamp(0.0, 1.0) * opacity
            }
            Mark::Scratch {
                x: at,
                rows,
                width,
                opacity,
                ..
            } => {
                if y < rows[0] || y > rows[1] {
                    return 0.0;
                }
                (width * 0.5 + 0.5 - (x - at).abs()).clamp(0.0, 1.0) * opacity
            }
        }
    }

    fn gray(&self) -> f32 {
        match *self {
            Mark::Speck { gray, .. } | Mark::Hair { gray, .. } | Mark::Scratch { gray, .. } => gray,
        }
    }

    /// The mark as the card reads it: two rows, its kind and numbers.
    fn rows(&self) -> [[f32; 4]; 2] {
        match *self {
            Mark::Speck {
                at,
                r,
                gray,
                opacity,
            } => [[0.0, at[0], at[1], r], [0.0, 0.0, gray, opacity]],
            Mark::Hair {
                at,
                r,
                from,
                sweep,
                gray,
                opacity,
            } => [[1.0, at[0], at[1], r], [from, sweep, gray, opacity]],
            Mark::Scratch {
                x,
                rows,
                width,
                gray,
                opacity,
            } => [[2.0, x, rows[0], width], [rows[1], 0.0, gray, opacity]],
        }
    }
}

/// Film Effects' numbers at a frame `w` × `h`: VEGAS's grain, tint and
/// grayscale, and the frame's particles (BananaV's): the picture's shake
/// in whole pixels, the light's flicker and the marks.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Film {
    pub grain: f32,
    pub tint: f32,
    pub color: [f32; 3],
    pub gray: bool,
    pub seed: u32,
    pub shake: [i32; 2],
    pub flicker: f32,
    pub marks: Vec<Mark>,
}

/// Film Effects' settings in the order [`Film::new`] takes them.
pub(crate) fn film_numbers(s: &Vals) -> [f64; 8] {
    let [r, g, b] = s.get_rgb(c"color");
    [
        s.f(c"grain"),
        s.f(c"tint"),
        r,
        g,
        b,
        f64::from(u8::from(s.b(c"convertToGrayscale"))),
        if s.b(c"enable") {
            f64::from(s.i(c"type").clamp(0, 4))
        } else {
            -1.0
        },
        s.f(c"amount"),
    ]
}

impl Film {
    /// The numbers of settings `v` ([`film_numbers`]) for a frame `w` ×
    /// `h` at `time` (frames).
    pub(crate) fn new(v: [f64; 8], w: usize, h: usize, time: f64) -> Film {
        let unit = |v: f64| v.clamp(0.0, 1.0);
        let [grain, tint, r, g, b, gray, kind, amount] = v;
        let seed = seed_of(time);
        let amount = unit(amount);
        let (wf, hf) = (w as f64, h as f64);
        // A frame's random numbers, the same for every render of it.
        let mut rng = crate::r250::R250::new(seed);
        let mut u = || f64::from(rng.next() & 0xffff) / 65535.0;
        let s = hf / 480.0;
        let mut shake = [0, 0];
        let mut flicker = 1.0;
        let mut marks = Vec::new();
        if amount > 0.0 {
            match kind as i32 {
                0 => {
                    let n = (amount * 80.0 * wf * hf / (640.0 * 480.0)).round() as usize;
                    for _ in 0..n.min(MARKS) {
                        let light = u() < 0.2;
                        marks.push(Mark::Speck {
                            at: [(u() * wf) as f32, (u() * hf) as f32],
                            r: ((0.6 + 2.4 * u().powi(3)) * s) as f32,
                            gray: if light {
                                0.85 + 0.15 * u() as f32
                            } else {
                                0.05 + 0.2 * u() as f32
                            },
                            opacity: 0.5 + 0.45 * u() as f32,
                        });
                    }
                }
                1 => flicker = (1.0 + amount * 0.3 * (2.0 * u() - 1.0)) as f32,
                2 => {
                    let n = (amount * 4.0 * u()).round() as usize;
                    for _ in 0..n {
                        marks.push(Mark::Hair {
                            at: [(u() * wf) as f32, (u() * hf) as f32],
                            r: ((20.0 + 60.0 * u()) * s) as f32,
                            from: (u() * std::f64::consts::TAU) as f32,
                            sweep: (0.5 + 1.5 * u()) as f32,
                            gray: 0.08,
                            opacity: 0.85,
                        });
                    }
                }
                3 => {
                    shake = [
                        (amount * 2.0 * s * (2.0 * u() - 1.0)).round() as i32,
                        (amount * 6.0 * s * (2.0 * u() - 1.0)).round() as i32,
                    ];
                }
                _ => {
                    // Scratches keep their places for a dozen frames,
                    // wavering a little from frame to frame.
                    let n = (amount * 6.0).round() as u32;
                    let span = (time / 12.0).floor() as u32;
                    for k in 0..n {
                        let place = hash01(k, span, 0x5c7a) * wf;
                        let first = hash01(k, span, 0x5c7b) * hf * 0.5;
                        let length = hf * (0.5 + 0.5 * hash01(k, span, 0x5c7c));
                        let light = hash01(k, span, 0x5c7d) < 0.6;
                        marks.push(Mark::Scratch {
                            x: (place + (2.0 * u() - 1.0) * s) as f32,
                            rows: [first as f32, (first + length) as f32],
                            width: (s.max(1.0)) as f32,
                            gray: if light { 0.9 } else { 0.1 },
                            opacity: (0.35 + 0.35 * u()) as f32,
                        });
                    }
                }
            }
        }
        Film {
            grain: unit(grain) as f32,
            tint: unit(tint) as f32,
            color: [r, g, b].map(|c| unit(c) as f32),
            gray: gray != 0.0,
            seed,
            shake,
            flicker,
            marks,
        }
    }

    /// The card's table of marks (two rows each).
    pub(crate) fn table(&self) -> Vec<[f32; 4]> {
        let mut t: Vec<[f32; 4]> = self.marks.iter().flat_map(|m| m.rows()).collect();
        if t.is_empty() {
            t.push([0.0; 4]);
        }
        t
    }

    /// Pixel `c` (premultiplied) at column `x`, row `y` (from the top) of
    /// the shaken picture: grayed, grained and tinted as VEGAS's renders
    /// show it, the light flickered and the marks laid over.
    pub(crate) fn pixel(&self, c: [f32; 4], x: u32, y: u32) -> [f32; 4] {
        let mut p = c;
        if self.gray {
            let v = (p[1] + p[0] + p[2]) / 3.0;
            p = [v, v, v, p[3]];
        }
        if self.grain > 0.0 {
            let h = film_hash(x, y, self.seed);
            let n = ((h & 0xffff) as i32 - 0x8000) as f32 * 3.051_757_8e-5 * 0.5 * self.grain;
            let a = p[3];
            p = if a <= 1e-6 {
                [0.0; 4]
            } else if a >= 1.0 {
                [p[0] + n, p[1] + n, p[2] + n, 1.0]
            } else {
                [
                    (p[0] / a + n) * a,
                    (p[1] / a + n) * a,
                    (p[2] / a + n) * a,
                    a,
                ]
            };
        }
        if self.tint > 0.0 {
            for (v, c) in p[..3].iter_mut().zip(self.color) {
                *v = (c - *v) * self.tint + *v;
            }
        }
        if self.flicker != 1.0 {
            for v in &mut p[..3] {
                *v *= self.flicker;
            }
        }
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        for m in &self.marks {
            let t = m.cover(fx, fy);
            if t > 0.0 {
                let g = m.gray() * p[3];
                for v in &mut p[..3] {
                    *v += (g - *v) * t;
                }
            }
        }
        p
    }
}

/// The most marks a frame has.
const MARKS: usize = 512;

/// Film Effects' grain's random number at a pixel of a frame (whole-number
/// math the card repeats).
pub(crate) fn film_hash(x: u32, y: u32, seed: u32) -> u32 {
    let mut h =
        x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^ (h >> 16)
}

/// Film Effects: VEGAS's grain, tint and grayscale over the frame, with
/// BananaV's particles.
fn film_effects(s: &Vals, g: &Grid, src: &Buf, time: f64) -> Buf {
    let (left, bottom, w, h) = frame_pixels(g);
    let film = Film::new(film_numbers(s), w, h, time);
    let [dx, dy] = film.shake;
    src.map(|x, y| {
        let (fx, fy) = (x - left, y - bottom);
        if fx < 0 || fy < 0 || fx >= w as c_int || fy >= h as c_int {
            return src.at(x, y);
        }
        let row = h as c_int - 1 - fy;
        // The shaken picture: what is under it moved by the shake, nothing
        // past its edges.
        let c = src.inside(x - dx, y + dy);
        film.pixel(c, fx as u32, row as u32)
    })
}

/// VEGAS's film grain at a frame: three tables of 64 × 64 normal numbers
/// spread over 0–1 (±5.28 deviations), stretched over the frame — a value
/// every (2 × Granularity + 1) 240ths of its height, as wide in the
/// pixels' shape —, read between values; the walk across each row and
/// down the rows as VEGAS steps it.
pub(crate) struct Grain {
    /// The tables, 64 × 64 each, rows first.
    pub table: Vec<f64>,
    /// Each column's and row's two table places and how far between them.
    pub columns: Vec<(usize, usize, f64)>,
    pub rows: Vec<(usize, usize, f64)>,
    pub amount: f32,
    pub chroma: f64,
}

impl Grain {
    /// The grain of a picture `w` × `h` (rows from the top) of pixels `par`
    /// wide, at frame `frame`.
    pub(crate) fn of(s: &Vals, w: usize, h: usize, par: f64, frame: f64) -> Grain {
        let numbers = [s.f(c"amount"), s.f(c"granularity"), s.f(c"chromaticity")];
        Grain::new(numbers, w, h, par, frame)
    }

    /// The grain of Amount, Granularity and Chromaticity `numbers`.
    pub(crate) fn new(numbers: [f64; 3], w: usize, h: usize, par: f64, frame: f64) -> Grain {
        let [amount, granularity, chroma] = numbers;
        let size = ((granularity * 2.0 + 1.0) * h as f64 / 240.0).max(1.0);
        let seed = ((size * 1000.0) as i32).wrapping_add(frame.round() as i32);
        let mut rng = crate::r250::R250::new(seed as u32);
        let mut table = Vec::with_capacity(3 * 4096);
        for _ in 0..3 * 4096 {
            let (s2, v) = loop {
                let u = f64::from(rng.next() & 0xffff) / 65535.0;
                let v = f64::from(rng.next() & 0xffff) / 65535.0;
                let (u, v) = (u + u - 1.0, v + v - 1.0);
                let s2 = v * v + u * u;
                if s2 < 1.0 && s2 != 0.0 {
                    break (s2, v);
                }
            };
            let n = ((s2.ln() * -2.0) / s2).sqrt();
            table.push((n * v + 5.28) / 10.56);
        }
        let walk = |count: usize, step: f64| {
            let (mut i0, mut i1, mut f) = (0usize, 1usize, 0.0f64);
            (0..count)
                .map(|_| {
                    let at = (i0, i1, f);
                    f += step;
                    if f >= 1.0 {
                        f -= 1.0;
                        i0 = i1;
                        i1 = (i1 + 1) & 63;
                    }
                    at
                })
                .collect()
        };
        Grain {
            table,
            columns: walk(w, 1.0 / (size / par)),
            rows: walk(h, 1.0 / size),
            amount: amount as f32,
            chroma,
        }
    }

    /// Table `t` between its places for column `x`, row `y`.
    fn read(&self, t: usize, x: usize, y: usize) -> f64 {
        let (i0, i1, fx) = self.columns[x];
        let (j0, j1, fy) = self.rows[y];
        let at = |j: usize, i: usize| self.table[t * 4096 + j * 64 + i];
        (fx * at(j0, i1) + (1.0 - fx) * at(j0, i0)) * (1.0 - fy)
            + (fx * at(j1, i1) + (1.0 - fx) * at(j1, i0)) * fy
    }

    /// The grain of column `x`, row `y` (from the top) for red, green and
    /// blue.
    pub(crate) fn at(&self, x: usize, y: usize) -> [f64; 3] {
        let n = self.read(0, x, y);
        if self.chroma <= 0.0 {
            return [n; 3];
        }
        let k = self.chroma;
        let rest = (1.0 - k) * n;
        [
            n,
            k * self.read(1, x, y) + rest,
            k * self.read(2, x, y) + rest,
        ]
    }
}

/// The picture's frame in its pixels: its left column, bottom row, width
/// and height.
fn frame_pixels(g: &Grid) -> (c_int, c_int, usize, usize) {
    let left = (g.origin[0] * g.scale[0] / g.par).round() as c_int;
    let bottom = (g.origin[1] * g.scale[1]).round() as c_int;
    let w = (g.frame[0] * g.scale[0] / g.par).round().max(1.0) as usize;
    let h = (g.frame[1] * g.scale[1]).round().max(1.0) as usize;
    (left, bottom, w, h)
}

/// Film Grain as VEGAS's: each color times 1.3 its grain, Amount of the
/// way (alpha kept).
fn film_grain(s: &Vals, g: &Grid, src: &Buf, time: f64) -> Buf {
    let (left, bottom, w, h) = frame_pixels(g);
    let grain = Grain::of(s, w, h, g.par, time);
    let a = grain.amount;
    src.map(|x, y| {
        let c = src.at(x, y);
        let (fx, fy) = (x - left, y - bottom);
        if fx < 0 || fy < 0 || fx >= w as c_int || fy >= h as c_int {
            return c;
        }
        let n = grain.at(fx as usize, h - 1 - fy as usize);
        let k = |i: usize| ((n[i] as f32) * 1.3 * c[i] - c[i]) * a + c[i];
        [k(0), k(1), k(2), c[3]]
    })
}

/// How much of a pixel a printed dot's rim covers, by how far the pixel's
/// middle is from the rim — `i` ⁄ 256 − 0.5 of a pixel, inside below 128 —
/// in 255ths: a disc a pixel wide cut by the rim.
pub(crate) fn dot_edges() -> [u8; 256] {
    let quarter = 0.785_398_163_397_447_5;
    let mut t = [0u8; 256];
    for (i, v) in t.iter_mut().enumerate() {
        if i == 128 {
            *v = 128;
            continue;
        }
        let d = i as f64 * 0.003_906_25 - 0.5;
        let a = if i < 128 {
            let d = d.abs();
            let angle = (d + d).acos();
            let s = angle.sin();
            quarter - ((angle / std::f64::consts::PI) * quarter - s * 0.5 * d * s * 0.5)
        } else {
            let angle = (d + d).acos();
            let s = angle.sin();
            (angle / std::f64::consts::PI) * quarter - s * 0.5 * d * s * 0.5
        };
        *v = ((a / quarter) * 255.0).clamp(0.0, 255.0) as i32 as u8;
    }
    t
}

/// Newsprint's numbers for a frame `w` × `h` of pixels `par` wide (VEGAS's
/// halftone): the cells' size in pixels — (the longer side × Dot size) ⁄
/// 20 + 3, across divided by the pixels' shape —, how many the averaged
/// picture has, the grid's shift (in cells), which rows sit half a cell
/// over, the dots' kind (0 black, 1 the cell's color, 2
/// three offset inks), how much of the source comes back (below Dot size
/// 0.15) and the rims' table.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Newsprint {
    pub cell: [f64; 2],
    pub cells: [usize; 2],
    pub span: [f64; 2],
    pub shift: [f64; 2],
    pub parity: i32,
    pub kind: u32,
    pub blend: f32,
    pub edges: [u8; 256],
}

impl Newsprint {
    pub(crate) fn new(dot: f64, kind: i32, w: usize, h: usize, par: f64) -> Newsprint {
        let dot = dot.clamp(0.0, 1.0);
        let (wf, hf) = (w as f64, h as f64);
        let longer = wf.max(hf);
        let ch = longer * dot / 20.0 + 3.0;
        let cw = ch / (par as f32 as f64);
        let span = [wf / cw, hf / ch];
        let cells = [
            ((span[0] + 0.499_999_985).round_ties_even().max(1.0)) as usize,
            ((hf / ch + 0.499_999_985).round_ties_even().max(1.0)) as usize,
        ];
        let half = (hf - span[1]) * 0.5;
        Newsprint {
            cell: [cw, ch],
            cells,
            span,
            shift: [
                -((wf - span[0]) * 0.5).rem_euclid(1.0),
                -half.rem_euclid(1.0),
            ],
            parity: !(half as i32),
            kind: match kind {
                1 => 0,
                2 => 2,
                _ => 1,
            },
            blend: if dot < 0.15 {
                (1.0 - dot / 0.15) as f32
            } else {
                0.0
            },
            edges: dot_edges(),
        }
    }

    /// The averaged picture: each cell the mean of the frame's pixels it
    /// covers (by how much of each), premultiplied, rows from the top.
    pub(crate) fn averaged(
        &self,
        pixel: impl Fn(usize, usize) -> [f32; 4],
        w: usize,
        h: usize,
    ) -> Vec<[f32; 4]> {
        let [cols, rows] = self.cells;
        let (sx, sy) = (w as f64 / cols as f64, h as f64 / rows as f64);
        let mut out = Vec::with_capacity(cols * rows);
        for j in 0..rows {
            let (y0, y1) = (j as f64 * sy, (j + 1) as f64 * sy);
            for i in 0..cols {
                let (x0, x1) = (i as f64 * sx, (i + 1) as f64 * sx);
                let mut sum = [0f64; 4];
                for y in (y0.floor() as usize)..(y1.ceil() as usize).min(h) {
                    let wy = (y1.min(y as f64 + 1.0) - y0.max(y as f64)).max(0.0);
                    for x in (x0.floor() as usize)..(x1.ceil() as usize).min(w) {
                        let wx = (x1.min(x as f64 + 1.0) - x0.max(x as f64)).max(0.0);
                        let c = pixel(x, y);
                        for k in 0..4 {
                            sum[k] += f64::from(c[k]) * wx * wy;
                        }
                    }
                }
                let area = (x1 - x0) * (y1 - y0);
                out.push(sum.map(|v| (v / area) as f32));
            }
        }
        out
    }

    /// The averaged picture between its cells at `x`, `y` (cells), held a
    /// cell and a half from its far edges as VEGAS's reading is.
    fn sample(&self, cells: &[[f32; 4]], x: f64, y: f64) -> [f32; 4] {
        let [cols, rows] = self.cells;
        let hold = |v: f64, n: f64| (v * 16384.0).clamp(0.0, ((n - 1.5) * 16384.0).max(0.0)) as i64;
        let (fx, fy) = (hold(x, self.span[0]), hold(y, self.span[1]));
        let (ix, iy) = ((fx >> 14) as usize, (fy >> 14) as usize);
        let (tx, ty) = (
            (fx & 0x3fff) as f32 / 16384.0,
            (fy & 0x3fff) as f32 / 16384.0,
        );
        let at = |i: usize, j: usize| cells[j.min(rows - 1) * cols + i.min(cols - 1)];
        let (a, b, c, d) = (
            at(ix, iy),
            at(ix + 1, iy),
            at(ix, iy + 1),
            at(ix + 1, iy + 1),
        );
        std::array::from_fn(|k| {
            let top = a[k] + (b[k] - a[k]) * tx;
            let bottom = c[k] + (d[k] - c[k]) * tx;
            top + (bottom - top) * ty
        })
    }

    /// The dots of the grid, as VEGAS lays them out: for each, its middle
    /// in pixels (rows from the top), its radii and its ink's color.
    pub(crate) fn dots(&self, cells: &[[f32; 4]]) -> Vec<([f64; 2], [f64; 2], [f32; 3])> {
        let [cw, ch] = self.cell;
        let mut dots = Vec::new();
        for j in -1..(self.span[1] as i32 + 3) {
            let y = f64::from(j) + self.shift[1];
            let offset = if (j.wrapping_add(self.parity)) & 1 != 0 {
                0.5
            } else {
                0.0
            };
            for i in -1..(self.span[0] as i32 + 3) {
                let x = f64::from(i) + offset + self.shift[0];
                let c = self.sample(cells, x, y);
                let dark = 1.0 - f64::from(c[1] * 0.587 + c[2] * 0.114 + c[0] * 0.299);
                let (rx, ry) = (cw * 0.5 * dark * 1.25, ch * 0.5 * dark * 1.25);
                if rx <= 1.0 / 255.0 || ry <= 1.0 / 255.0 {
                    continue;
                }
                let at = [x * cw, y * ch];
                match self.kind {
                    0 => dots.push((at, [rx, ry], [0.0, 0.0, 0.0])),
                    1 => dots.push((at, [rx, ry], [c[0], c[1], c[2]])),
                    _ => {
                        let (qx, qy) = (rx * 0.25, ry * 0.25);
                        dots.push(([at[0] - qx, at[1] - qy], [rx, ry], [1.0, 1.0, c[2]]));
                        dots.push(([at[0] + qx, at[1] - qy], [rx, ry], [1.0, c[1], 1.0]));
                        dots.push(([at[0], at[1] + qy], [rx, ry], [c[0], 1.0, 1.0]));
                    }
                }
            }
        }
        dots
    }

    /// How much of pixel `x`, `y` (rows from the top) a dot covers, as
    /// VEGAS's drawing measures it (0 outside the rows and columns it
    /// draws).
    pub(crate) fn cover(&self, at: [f64; 2], r: [f64; 2], x: usize, y: usize) -> f32 {
        let lo = |v: f64| (v - 0.499_999_985).round_ties_even();
        let hi = |v: f64| (v + 0.499_999_985).round_ties_even();
        let (xf, yf) = (x as f64, y as f64);
        if xf < lo(at[0] - r[0])
            || xf >= hi(at[0] + r[0])
            || yf < lo(at[1] - r[1])
            || yf >= hi(at[1] + r[1])
        {
            return 0.0;
        }
        let inner = (r[1] - 0.7).max(0.0);
        let dy = (yf + 0.0 - at[1]) + 0.5;
        let dx = ((xf + 0.5) - at[0]) * (r[1] / r[0]);
        let d2 = dx * dx + dy * dy;
        if d2 < inner * inner {
            return 1.0;
        }
        let d = d2.sqrt();
        if d > r[1] + 0.5 {
            0.0
        } else if d < r[1] - 0.5 {
            1.0
        } else {
            let k = ((d - r[1]) * 255.0).round_ties_even() as i64 + 128;
            f32::from(self.edges[k.clamp(0, 255) as usize]) / 255.0
        }
    }
}

/// Newsprint as VEGAS's: white paper, each dot's ink taken from it by how
/// much of each pixel the dot covers; below Dot size 0.15 the source (gray
/// for the black dots) mixed back.
fn newsprint(s: &Vals, g: &Grid, src: &Buf) -> Buf {
    let (left, bottom, w, h) = frame_pixels(g);
    let n = Newsprint::new(s.f(c"dotSize"), s.i(c"type"), w, h, g.par);
    let pixel = |x: usize, y: usize| src.inside(left + x as c_int, bottom + (h - 1 - y) as c_int);
    let cells = n.averaged(pixel, w, h);
    let mut paper = vec![[1.0f32; 4]; w * h];
    for (at, r, ink) in n.dots(&cells) {
        let rows = ((at[1] - r[1] - 1.0).floor().max(0.0) as usize)
            ..((at[1] + r[1] + 2.0).ceil().max(0.0) as usize).min(h);
        for y in rows {
            let cols = ((at[0] - r[0] - 1.0).floor().max(0.0) as usize)
                ..((at[0] + r[0] + 2.0).ceil().max(0.0) as usize).min(w);
            for x in cols {
                let k = n.cover(at, r, x, y);
                if k > 0.0 {
                    let p = &mut paper[y * w + x];
                    for c in 0..3 {
                        p[c] -= (1.0 - ink[c]) * k;
                    }
                }
            }
        }
    }
    src.map(|x, y| {
        let (fx, fy) = (x - left, y - bottom);
        if fx < 0 || fy < 0 || fx >= w as c_int || fy >= h as c_int {
            return src.at(x, y);
        }
        let (fx, row) = (fx as usize, h - 1 - fy as usize);
        let mut p = paper[row * w + fx];
        if n.blend > 0.0 {
            let mut c = pixel(fx, row);
            if n.kind == 0 {
                let l = c[1] * 0.587 + c[2] * 0.114 + c[0] * 0.299;
                c = [l, l, l, c[3]];
            }
            for k in 0..3 {
                p[k] = (c[k] - p[k]) * n.blend + p[k];
            }
        }
        p
    })
}

/// The scanlines' profile over one cycle (BananaV's: VEGAS's ten values
/// are not known yet).
const SCANLINE: [f32; 10] = [
    1.0, 0.905, 0.655, 0.345, 0.095, 0.0, 0.095, 0.345, 0.655, 0.905,
];
/// The scan phasing band's profile, top to bottom (BananaV's).
const BAND: [f32; 5] = [1.0, 1.2, 1.4, 1.2, 1.0];
/// The aperture grill's red, green and blue stripes and the gap after
/// them, the first again to close the cycle (BananaV's).
const GRILL: [[f32; 3]; 5] = [
    [1.0, 0.24, 0.24],
    [0.24, 1.0, 0.24],
    [0.24, 0.24, 1.0],
    [0.08, 0.08, 0.08],
    [1.0, 0.24, 0.24],
];

/// TV Simulator's numbers at a frame `w` × `h`, as VEGAS's layers: the
/// picture rolled down (Vertical sync), its rows torn across (Line sync),
/// the aperture grill's stripes and the scanlines — Detail zoom their size
/// — mixed in, the scan phasing band rolling down, the static over the
/// stripes and the phosphors' curves. Their profiles are BananaV's
/// ([`SCANLINE`], [`BAND`], [`GRILL`]).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Tv {
    /// How many rows the picture rolls down.
    pub roll: u32,
    /// Each row's move across, in pixels.
    pub shifts: Vec<f32>,
    /// How much of the stripes: 0.75 × the larger of Aperture grill and
    /// Static.
    pub grill: f32,
    pub interlace: f32,
    /// The band's first row and its height (0: none).
    pub band: [f32; 2],
    /// The phosphors' powers for red, green and blue (1: as it is).
    pub phosphor: [f32; 3],
    pub snow: f32,
    pub seed: u32,
    /// The grill's cycle across and the scanlines' down, in pixels.
    pub pitch: f32,
    pub lines: f32,
}

/// TV Simulator's settings in the order [`Tv::new`] takes them.
pub(crate) fn tv_numbers(s: &Vals) -> [f64; 8] {
    [
        c"detailZoom",
        c"apertureGrill",
        c"interlacing",
        c"lineSync",
        c"verticalSync",
        c"scanPhasing",
        c"phosphorescence",
        c"static",
    ]
    .map(|n| s.f(n))
}

impl Tv {
    /// The numbers of settings `v` ([`tv_numbers`]) for a frame `w` × `h`
    /// of pixels `par` wide, at `time` (frames, 30 a second: the effect
    /// is not told the rate).
    pub(crate) fn new(v: [f64; 8], w: usize, h: usize, par: f64, time: f64) -> Tv {
        let unit = |v: f64| v.clamp(0.0, 1.0);
        let [
            detail,
            grill,
            interlace,
            line,
            vertical,
            scan,
            phosphor,
            snow,
        ] = v.map(unit);
        let (wf, hf) = (w as f64, h.max(1) as f64);
        let secs = time / 30.0;
        let zoom = 1.0 + 3.0 * detail;
        let seed = seed_of(time);
        let mut rng = crate::r250::R250::new(seed);
        let mut u = || f64::from(rng.next() & 0xffff) / 65536.0;
        // VEGAS's roll: (1 ⁄ Vertical sync − 1) × the zoom frame heights a
        // second.
        let speed = 1.0 / vertical.max(0.001) - 1.0;
        let roll = ((speed * zoom * hf * secs).floor().rem_euclid(hf)) as u32;
        // Rows torn by five waves of halving strength (VEGAS's 4.567 ⁄
        // zoom cycles down the frame the first).
        let tear = 1.0 - line;
        let phases: [f64; 5] = std::array::from_fn(|_| u());
        let shifts = (0..h)
            .map(|y| {
                if tear <= 0.0 {
                    return 0.0;
                }
                let along = y as f64 / hf * 4.567 / zoom;
                let wave: f64 = (0..5)
                    .map(|k| {
                        let f = f64::from(1u32 << k);
                        (std::f64::consts::TAU * (along * f + phases[k])).sin() / f
                    })
                    .sum();
                (tear * 0.03 * wf / par * wave) as f32
            })
            .collect();
        // The band: 0.35 of the frame times the zoom high, rolling down
        // three times a second times Scan phasing.
        let band = if scan > 1e-4 {
            let height = hf * 0.35 * zoom;
            let at = (3.0 * scan * secs).rem_euclid(1.0) * hf * zoom + (hf - zoom * hf) * 0.5
                - height * 0.5;
            [at as f32, height as f32]
        } else {
            [0.0, 0.0]
        };
        // The phosphors' curves (VEGAS's): powers under 1 lift blue most,
        // red least, by a little noise more each frame.
        let n = (u() * 1024.0).floor() / 1024.0 * 0.05 * phosphor;
        let power = |k: f64| {
            if phosphor > 0.0 {
                (1.0 - phosphor * k + n).min(1.0)
            } else {
                1.0
            }
        };
        Tv {
            roll,
            shifts,
            grill: (0.75 * grill.max(snow)) as f32,
            interlace: interlace as f32,
            band,
            phosphor: [power(0.15) as f32, power(0.5) as f32, power(0.6) as f32],
            snow: snow as f32,
            seed,
            pitch: (wf * 0.006_676_25 * zoom / par).max(1.0) as f32,
            lines: (hf * 0.022_254_166 * zoom).max(1.0) as f32,
        }
    }

    /// The card's table: each row's move.
    pub(crate) fn table(&self) -> Vec<[f32; 4]> {
        let mut t: Vec<[f32; 4]> = self.shifts.iter().map(|&s| [s, 0.0, 0.0, 0.0]).collect();
        if t.is_empty() {
            t.push([0.0; 4]);
        }
        t
    }

    /// How bright row `y` is: the scanlines by Interlacing, the band.
    fn row_weight(&self, y: u32) -> f32 {
        let pos = y as f32 / self.lines * 10.0;
        let i = pos.floor();
        let f = pos - i;
        let i = (i as i64).rem_euclid(10) as usize;
        let line = SCANLINE[i] * (1.0 - f) + SCANLINE[(i + 1) % 10] * f;
        let mut weight = line * self.interlace + (1.0 - self.interlace);
        let [at, height] = self.band;
        let yf = y as f32;
        if height > 0.0 && yf >= at && yf <= at + height {
            let k = (yf - at) / height * 4.0;
            let j = (k.floor() as usize).min(3);
            let g = k - j as f32;
            weight *= BAND[j] * (1.0 - g) + BAND[j + 1] * g;
        }
        weight
    }

    /// Pixel `c` (premultiplied, already rolled and torn) at column `x`,
    /// row `y` (from the top): the stripes and static mixed in, the row's
    /// brightness, held within 0 and its alpha, then the phosphors.
    pub(crate) fn pixel(&self, c: [f32; 4], x: u32, y: u32) -> [f32; 4] {
        let q = x as f32 / self.pitch * 4.0;
        let qi = q.floor();
        let f = q - qi;
        let i = (qi as i64).rem_euclid(4) as usize;
        let static_noise = if self.snow > 0.0 {
            let h = film_hash(qi as i64 as u32, y, self.seed ^ 0x7e1e);
            (h & 0xffff) as f32 / 65536.0 * 2.0 * self.snow
        } else {
            0.0
        };
        let weight = self.row_weight(y);
        let a = c[3];
        let mut out = [0.0f32, 0.0, 0.0, a];
        for k in 0..3 {
            let w = GRILL[i][k] * (1.0 - f) + GRILL[i + 1][k] * f + static_noise;
            let t = w * 0.25 + 1.992_187_5 * c[k] * w;
            let v = ((t * self.grill + c[k] * (1.0 - self.grill)) * weight).clamp(0.0, a.max(0.0));
            out[k] = if self.phosphor[k] < 1.0 {
                v.powf(self.phosphor[k]).min(a)
            } else {
                v
            };
        }
        out
    }
}

/// TV Simulator: the frame through [`Tv`].
fn tv_simulator(s: &Vals, g: &Grid, src: &Buf, time: f64) -> Buf {
    let (left, bottom, w, h) = frame_pixels(g);
    let tv = Tv::new(tv_numbers(s), w, h, g.par, time);
    src.map(|x, y| {
        let (fx, fy) = (x - left, y - bottom);
        if fx < 0 || fy < 0 || fx >= w as c_int || fy >= h as c_int {
            return src.at(x, y);
        }
        let row = (h as c_int - 1 - fy) as u32;
        // The rolled row, torn across: read between its pixels.
        let from = (row + tv.roll) % h as u32;
        let up = bottom + (h as c_int - 1 - from as c_int);
        let sx = fx as f32 - tv.shifts[row as usize];
        let x0 = sx.floor();
        let t = sx - x0;
        let read = |k: f32| src.inside(left + k as c_int, up);
        let (a, b) = (read(x0), read(x0 + 1.0));
        let c: [f32; 4] = std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t);
        tv.pixel(c, fx as u32, row)
    })
}

/// A 3-vector made 1 long.
fn unit((a, b, c): (f64, f64, f64)) -> (f64, f64, f64) {
    let n = (a * a + b * b + c * c).sqrt().max(1e-9);
    (a / n, b / n, c / n)
}

fn dot(a: (f64, f64, f64), b: (f64, f64, f64)) -> f64 {
    a.0 * b.0 + a.1 * b.1 + a.2 * b.2
}

fn bump_map(s: &Vals, g: &Grid, src: &Buf) -> Buf {
    bump_map_with(s, g, src, src)
}

/// Bump Map: `src` lit by the bumps of `map` (itself, as a video effect;
/// as a compositor, the track's picture).
/// Bump Map's light for a picture `w` × `h` pixels of aspect `par`.
struct BumpLight {
    source: (f64, f64, f64),
    /// Directional (0), Spotlight (1) or Omnidirectional (2).
    kind: i32,
    intensity: f64,
    shininess: f64,
    ambience: f64,
    channel: usize,
    /// The surface's rise per step of the bump channel.
    lift: f64,
    axis: (f64, f64, f64),
    cone: f64,
    spread: f64,
    step: (f64, f64),
}

impl BumpLight {
    fn of(s: &Vals, (w, h): (f64, f64), par: f64) -> BumpLight {
        let [sx, sy, sz] = s.triple(c"source");
        let [dx, dy] = s.pair(c"destination");
        let kind = s.i(c"lightType");
        let height = s.f(c"bumpHeight");
        let sign = if s.b(c"whiteIsHigh") { 1.0 } else { -1.0 };
        let (ax, ay) = if kind == 1 { (dx, dy) } else { (0.0, 0.0) };
        let axis = {
            let v = (sx - ax, sy - ay, sz);
            let n = dot(v, v).sqrt();
            if n <= 1e-7 {
                (0.0, 0.0, 1.0)
            } else {
                (v.0 / n, v.1 / n, v.2 / n)
            }
        };
        let (kx, ky) = if par >= 1.0 {
            (1.0, 1.0 / par)
        } else {
            (par, 1.0)
        };
        let longer = (w * kx).max(h * ky);
        let cone = (s.f(c"focus") * FRAC_PI_4).cos();
        let step = 2.0 / longer;
        BumpLight {
            source: (sx, sy, sz),
            kind,
            intensity: s.f(c"intensity"),
            shininess: s.f(c"shininess"),
            ambience: s.f(c"ambience"),
            channel: s.i(c"bumpChannel").clamp(0, 3) as usize,
            lift: longer * 0.001_562_5 * height * height * 0.0625 * sign * 255.0,
            axis,
            cone,
            spread: (1.0 - cone).max(1e-5),
            step: if par >= 1.0 {
                (step, step / par)
            } else {
                (step * par, step)
            },
        }
    }
}

/// Bump Map on the card (the filter: the picture its own map), its light
/// worked out for the frame's pixels.
pub(crate) fn bump_plan(s: &Vals, frame: [f64; 2]) -> crate::accel::Plan {
    let (f, light) = bump_numbers(s, frame);
    crate::accel::Plan::Remap {
        kernel: crate::accel::RemapKernel::BumpMap,
        f,
        j: [light, 0, 0, 0],
    }
}

/// Bump Map's numbers on the card (the shaders' `bump_light`, the filter's
/// and the compositor's): its light worked out for the frame's pixels and
/// the bump channel's weights; and the light's type.
pub(crate) fn bump_numbers(s: &Vals, frame: [f64; 2]) -> (Vec<f32>, u32) {
    let b = BumpLight::of(s, (frame[0], frame[1]), 1.0);
    let mut mask = [0.0f32; 4];
    mask[b.channel] = 1.0;
    let mut f: Vec<f32> = [
        b.source.0,
        b.source.1,
        b.source.2,
        b.axis.0,
        b.axis.1,
        b.axis.2,
        b.lift / frame[0].max(frame[1]),
        b.cone,
        b.spread,
        b.intensity,
        b.shininess,
        b.ambience,
    ]
    .map(|v| v as f32)
    .to_vec();
    f.extend_from_slice(&mask);
    (f, b.kind.clamp(0, 2) as u32)
}

pub(crate) fn bump_map_with(s: &Vals, g: &Grid, src: &Buf, map: &Buf) -> Buf {
    // VEGAS's: each pixel's surface leans by the bump channel's 3 × 3
    // differences; the light is Source less the pixel's place (Directional:
    // Source alone), in units of half the frame's longer side, up and out
    // of the picture; a spotlight's beam from Destination to Source is
    // Focus × 45° wide, fading over the edge; omni and spot lights fall
    // off as 2 ⁄ distance. The picture times (specular and diffuse light)
    // × Intensity × 8 plus twice Ambience; its alpha kept.
    let (w, h) = (f64::from(src.width()), f64::from(src.height()));
    let b = BumpLight::of(s, (w, h), g.par);
    let (sx, sy, sz) = b.source;
    let (kind, intensity, shininess, ambience) = (b.kind, b.intensity, b.shininess, b.ambience);
    let (channel, lift, axis, cone, spread) = (b.channel, b.lift, b.axis, b.cone, b.spread);
    let (step_x, step_y) = b.step;
    let [x1, y1, ..] = src.bounds;
    src.map(|x, y| {
        let at = |i: c_int, j: c_int| f64::from(map.at(x + i, y + j)[channel]);
        // Rows go up here: +1 is the row above.
        let gx = (-1..=1).map(|j| at(-1, j) - at(1, j)).sum::<f64>() * lift;
        let gy = (-1..=1).map(|i| at(i, -1) - at(i, 1)).sum::<f64>() * lift;
        let n = unit((gx, gy, 1.0));
        let (l, reach) = if kind == 0 {
            (axis, 1.0)
        } else {
            let px = (f64::from(x - x1) - w * 0.5) * step_x;
            let py = (f64::from(y - y1) + 1.0 - h * 0.5) * step_y;
            let v = (sx - px, sy - py, sz);
            let far = dot(v, v).sqrt();
            let (l, near) = if far <= 0.0 {
                ((0.0, 0.0, 1.0), 100.0)
            } else {
                ((v.0 / far, v.1 / far, v.2 / far), 1.0 / far)
            };
            let beam = if kind == 1 {
                let c = ((dot(axis, l) - cone) / spread).clamp(0.0, 1.0);
                c * 2.0 * c
            } else {
                1.0
            };
            (l, beam * 2.0 * near)
        };
        let ndl = dot(n, l);
        let spec = (2.0 * n.2 * ndl - l.2)
            .max(0.01)
            .powf(shininess * 8.0 + 4.0);
        let k = ((spec * shininess * 0.5 + ndl * (0.6 - shininess * 0.2)) * intensity * 8.0 * reach
            + 2.0 * ambience)
            .max(0.0) as f32;
        let c = src.at(x, y);
        [c[0] * k, c[1] * k, c[2] * k, c[3]]
    })
}

/// Denoise's strengths below which it leaves a plane (or Sharpening) out:
/// VEGAS's single-precision epsilon.
const NOTHING: f64 = f32::EPSILON as f64;

/// Denoise's settings: Luminance's and Chrominance's strengths and
/// Sharpening's kernel center (none: no sharpening).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Denoise {
    pub(crate) luma: f64,
    pub(crate) chroma: f64,
    pub(crate) center: Option<i32>,
}

impl Denoise {
    pub(crate) fn of(s: &Vals) -> Denoise {
        let amount = s.f(c"amount").clamp(0.0, 1.0);
        Denoise {
            luma: s.f(c"luminance").clamp(0.0, 100.0),
            chroma: s.f(c"color").clamp(0.0, 100.0),
            center: (amount > NOTHING).then(|| sharpen_center(amount)),
        }
    }

    /// Whether it leaves the picture as it is (VEGAS's identity).
    pub(crate) fn changes_nothing(&self) -> bool {
        self.luma < NOTHING && self.chroma < NOTHING && self.center.is_none()
    }

    /// Whether the planes are cleaned at all.
    pub(crate) fn cleans(&self) -> bool {
        self.luma > NOTHING || self.chroma > NOTHING
    }
}

/// Sharpening's kernel center for Amount `a`: a table, 100 at the
/// least, 13 at the most (its divisor, the center less 12, then 1).
pub(crate) fn sharpen_center(a: f64) -> i32 {
    if a < 0.25 {
        if a < 0.125 {
            if a < f64::from(0.063f32) {
                (100.451613 - a * 451.612903) as i32
            } else {
                (88.961538 - a * 269.230769) as i32
            }
        } else {
            (63.0 - a * 152.0) as i32
        }
    } else {
        29 - (a * 16.0) as i32
    }
}

/// A color's BT.601 Y, U and V in 8-bit steps (VEGAS's 8-bit planes), its
/// alpha kept.
pub(crate) fn yuv_of(c: [f32; 4]) -> [f32; 4] {
    let [r, g, b] = [c[0] * 255.0, c[1] * 255.0, c[2] * 255.0];
    [
        r * 0.299 + g * 0.587 + b * 0.114,
        (g * -0.28886 - r * 0.14713) + b * 0.436,
        (r * 0.615 - g * 0.51499) - b * 0.10001,
        c[3],
    ]
}

/// The color of Y, U and V in 8-bit steps: each channel within 0–1, the
/// alpha too.
pub(crate) fn rgb_of(p: [f32; 4]) -> [f32; 4] {
    let [y, u, v, a] = p;
    let r = u * 0.0 + y + v * 1.13983;
    let g = (y - u * 0.39465) - v * 0.5806;
    let b = u * 2.03211 + y + v * 0.0;
    let ch = |c: f32| c.clamp(0.0, 255.0) / 255.0;
    [ch(r), ch(g), ch(b), a.clamp(0.0, 1.0)]
}

/// How far around a pixel non-local means looks, and how far around the
/// two pixels it compares (VEGAS's: 21 × 21, 7 × 7).
pub(crate) const SEARCH: usize = 10;
pub(crate) const PATCH: usize = 3;
/// How far a plane is reflected past its edges for them.
pub(crate) const REACH: usize = SEARCH + PATCH;

/// The index `i` of a row or column `n` long reflected past its ends, its
/// end pixels not repeated (VEGAS's border).
pub(crate) fn reflected(i: isize, n: usize) -> usize {
    let n = n as isize;
    let i = if i < 0 {
        -i
    } else if i >= n {
        2 * (n - 1) - i
    } else {
        i
    };
    i.clamp(0, n - 1) as usize
}

/// A patch difference's weight (`d`: the mean absolute difference,
/// rounded) at strength `h`: VEGAS's exp(−max(d − 2h², 0)/h²), nothing
/// below 0.001.
pub(crate) fn patch_weight(d: f64, h: f64) -> f64 {
    let w = (-(d - 2.0 * h * h).max(0.0) / (h * h)).exp();
    if w < 0.001 { 0.0 } else { w }
}

/// VEGAS's non-local means of a plane `w` × `h` (rows of `plane`) at
/// strength `h`: each pixel the mean of the pixels within 10 of it, each
/// weighted by how alike the 7 × 7 patches around the two are
/// ([`patch_weight`]), the plane reflected past its edges. Rows apart on
/// the host's threads, each move's differences summed across, then down.
pub(crate) fn non_local_means(plane: &[f32], (w, h): (usize, usize), strength: f64) -> Vec<f32> {
    let (pw, ph) = (w + 2 * REACH, h + 2 * REACH);
    let padded: Vec<f32> = (0..pw * ph)
        .map(|k| {
            let (x, y) = (
                (k % pw) as isize - REACH as isize,
                (k / pw) as isize - REACH as isize,
            );
            plane[reflected(y, h) * w + reflected(x, w)]
        })
        .collect();
    let side = 2 * SEARCH + 1;
    let size = 2 * PATCH + 1;
    let dw = w + size - 1;
    let mut out = vec![0.0f32; w * h];
    const ROWS: usize = 16;
    crate::kit::for_chunks(&mut out, w * ROWS, |chunk, rows| {
        let y0 = chunk * ROWS;
        let n = rows.len() / w.max(1);
        let mut acc = vec![0.0f64; n * w];
        let mut total = vec![0.0f64; n * w];
        // A move's differences, and their sums across a patch, over the
        // rows' patches.
        let mut diff = vec![0.0f64; (n + size - 1) * dw];
        let mut across = vec![0.0f64; (n + size - 1) * w];
        for dy in 0..side {
            for dx in 0..side {
                for r in 0..n + size - 1 {
                    let here = &padded[(y0 + r + SEARCH) * pw + SEARCH..][..dw];
                    let there = &padded[(y0 + r + dy) * pw + dx..][..dw];
                    let row = &mut diff[r * dw..][..dw];
                    for ((d, a), b) in row.iter_mut().zip(here).zip(there) {
                        *d = f64::from((a - b).abs());
                    }
                    let mut s: f64 = row[..size].iter().sum();
                    across[r * w] = s;
                    for x in 1..w {
                        s += row[x + size - 1] - row[x - 1];
                        across[r * w + x] = s;
                    }
                }
                for x in 0..w {
                    let mut s: f64 = (0..size).map(|r| across[r * w + x]).sum();
                    for y in 0..n {
                        if y > 0 {
                            s += across[(y + size - 1) * w + x] - across[(y - 1) * w + x];
                        }
                        let d = (s * (1.0 / 49.0)).round();
                        let weight = patch_weight(d, strength);
                        let p = padded[(y0 + y + dy + PATCH) * pw + x + dx + PATCH];
                        acc[y * w + x] += weight * f64::from(p);
                        total[y * w + x] += weight;
                    }
                }
            }
        }
        for ((o, a), t) in rows.iter_mut().zip(&acc).zip(&total) {
            *o = (a / t) as f32;
        }
    });
    out
}

/// Sharpening's 3 × 3 kernel −1 −2 −1, −2 `center` −2, −1 −2 −1 over the
/// center less 12, on every channel of `src`, its edges held, each within
/// 0–1; the colors then at most the alpha (VEGAS's premultiplied patch).
pub(crate) fn sharpened(src: &Buf, center: i32) -> Buf {
    let [x1, y1, x2, y2] = src.bounds;
    let (c, d) = (center as f32, (center - 12) as f32);
    src.map(|x, y| {
        let at =
            |dx: c_int, dy: c_int| src.at((x + dx).clamp(x1, x2 - 1), (y + dy).clamp(y1, y2 - 1));
        let (up, right, down, left) = (at(0, 1), at(1, 0), at(0, -1), at(-1, 0));
        let corners = [at(-1, 1), at(1, 1), at(-1, -1), at(1, -1)];
        let me = at(0, 0);
        let mut v: [f32; 4] = std::array::from_fn(|k| {
            let sum = c * me[k]
                - (corners[0][k] + corners[1][k] + corners[2][k] + corners[3][k])
                - 2.0 * (up[k] + left[k] + right[k] + down[k]);
            (sum / d).clamp(0.0, 1.0)
        });
        for k in 0..3 {
            v[k] = v[k].min(v[3]);
        }
        v
    })
}

/// VEGAS's Denoise as its 8-bit processor works: the picture in BT.601 Y,
/// U and V planes (in 8-bit steps), Y cleaned by non-local means at
/// Luminance's strength, U and V at Chrominance's, back to RGB within 0–1;
/// then Sharpening's kernel.
fn denoise(s: &Vals, src: &Buf) -> Buf {
    let d = Denoise::of(s);
    let cleaned = if d.cleans() {
        let (w, h) = (src.width().max(0) as usize, src.height().max(0) as usize);
        let planes: Vec<[f32; 4]> = src.data.iter().map(|&c| yuv_of(c)).collect();
        let plane = |k: usize, strength: f64| {
            let p: Vec<f32> = planes.iter().map(|c| c[k]).collect();
            if strength > NOTHING {
                non_local_means(&p, (w, h), strength)
            } else {
                p
            }
        };
        let (y, u, v) = (plane(0, d.luma), plane(1, d.chroma), plane(2, d.chroma));
        Buf {
            bounds: src.bounds,
            data: (0..planes.len())
                .map(|i| rgb_of([y[i], u[i], v[i], planes[i][3]]))
                .collect(),
        }
    } else {
        src.clone()
    };
    match d.center {
        Some(c) => sharpened(&cleaned, c),
        None => cleaned,
    }
}

/// Look `i`'s picture of a color (BananaV's own looks).
fn look(i: i32, c: [f32; 3]) -> [f32; 3] {
    let l = luma3(c);
    let saturated = |c: [f32; 3], k: f32| c.map(|v| l + (v - l) * k);
    // An S curve of strength `k` (black and white kept).
    let curve = |v: f32, k: f32| {
        let t = v.clamp(0.0, 1.0);
        v + k * (t * t * (3.0 - 2.0 * t) - t)
    };
    match i {
        // Richer, cleaner colors.
        0 => saturated(c.map(|v| curve(v, 0.3)), 1.25),
        // Silvery, contrasty, little color.
        1 => saturated(c.map(|v| curve(v, 0.7)), 0.4),
        // Greenish yellow highlights, blue shadows.
        2 => [
            curve(c[0], 0.5) * 1.05,
            c[1] * 1.1 + 0.03,
            c[2] * 0.75 + 0.12,
        ],
        3 => [c[0] * 1.12 + 0.03, c[1] * 1.03 + 0.01, c[2] * 0.85],
        4 => saturated([c[0] * 0.8, c[1] * 0.92, c[2] * 1.15 + 0.04], 0.8),
        // Lifted blacks, softer colors.
        5 => saturated(c.map(|v| 0.08 + v * 0.86), 0.75),
        6 => c.map(|v| curve(v, 1.0)),
        // Black and white, contrasty.
        7 => [curve(l, 0.8); 3],
        // Reds and cyans only.
        8 => {
            let cyan = (c[1] + c[2]) / 2.0;
            [c[0], cyan * 0.95, cyan]
        }
        // Saturated, toward magenta and cyan.
        _ => saturated([c[0] * 1.1 + 0.02, c[1] * 0.9, c[2] * 1.15 + 0.02], 1.6),
    }
}

/// How many points each way a look's table has (VEGAS's AutoLooks'
/// tables are 16 × 16 × 16).
const LOOK_POINTS: usize = 16;

/// Look `i`'s table (`None`: none, or past the list): its colors worked
/// out once at [`LOOK_POINTS`]³ places over 0–1, as VEGAS keeps each look.
pub(crate) fn look_table(i: i32) -> Option<std::sync::Arc<bv_lut::Lut>> {
    static TABLES: std::sync::OnceLock<Vec<std::sync::Arc<bv_lut::Lut>>> =
        std::sync::OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        (0..LOOK_NAMES.len() as i32 - 1)
            .map(|k| {
                let n = LOOK_POINTS;
                let step = 1.0 / (n - 1) as f32;
                let entries = (0..n * n * n)
                    .map(|e| {
                        // Red changing fastest, then green, then blue.
                        let c = [e % n, e / n % n, e / (n * n)].map(|v| v as f32 * step);
                        look(k, c)
                    })
                    .collect();
                std::sync::Arc::new(bv_lut::Lut {
                    title: String::new(),
                    shaper: None,
                    cube: Some(bv_lut::Table3 {
                        min: [0.0; 3],
                        max: [1.0; 3],
                        size: n,
                        entries,
                    }),
                })
            })
            .collect()
    });
    usize::try_from(i - 1)
        .ok()
        .and_then(|k| tables.get(k))
        .cloned()
}

/// VEGAS's AutoLooks: each straight color through the look's table,
/// trilinearly, then the picture moved toward it by Strength — on the
/// premultiplied colors `c + (look × alpha − c) × Strength`; transparent
/// pixels stay so. With none the picture stays as it is.
fn looks(s: &Vals, src: &Buf) -> Buf {
    let gain = s.f(c"gain").clamp(0.0, 1.0) as f32;
    let Some(lut) = look_table(s.i(c"look")).filter(|_| gain > 0.0) else {
        return src.clone();
    };
    src.map(|x, y| {
        let c = straight(src.at(x, y));
        if c[3] <= 0.0 {
            return [0.0; 4];
        }
        let l = lut.apply([c[0], c[1], c[2]], bv_lut::Interpolation::Linear);
        let out: [f32; 3] = std::array::from_fn(|k| c[k] + (l[k] - c[k]) * gain);
        premultiplied([out[0], out[1], out[2], c[3]])
    })
}

/// The factory presets of effect `kind` (one of this module's).
pub(crate) fn presets(kind: Kind) -> Vec<Preset> {
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    let p = |name: &'static str, values: Vec<(&'static str, PresetValue)>| Preset { name, values };
    let one = |name: &'static str, param: &'static str, v: f64| p(name, vec![(param, num(&[v]))]);
    match kind {
        Kind::LabAdjust => {
            let lab = |name: &'static str, l: f64, a: f64, b: f64, invert: bool, desat: f64| {
                p(
                    name,
                    vec![
                        ("lightness", num(&[l])),
                        ("channelA", num(&[a])),
                        ("channelB", num(&[b])),
                        ("invertLuminance", num(&[f64::from(u8::from(invert))])),
                        ("desaturate", num(&[desat])),
                    ],
                )
            };
            vec![
                lab("Lighter", 1.1, 0.0, 0.0, false, 0.0),
                lab("Darker", 0.9, 0.0, 0.0, false, 0.0),
                lab("More Red", 1.0, 0.2, 0.0, false, 0.0),
                lab("More Green", 1.0, -0.2, 0.0, false, 0.0),
                lab("More Yellow", 1.0, 0.0, 0.2, false, 0.0),
                lab("More Blue", 1.0, 0.0, -0.2, false, 0.0),
                lab("Invert Luminosity", 1.0, 0.0, 0.0, true, 0.0),
                lab("Half Saturation", 1.0, 0.0, 0.0, false, 0.5),
                lab("Desaturate", 1.0, 0.0, 0.0, false, 1.0),
            ]
        }
        Kind::SoftContrast => {
            // VEGAS's three.
            let soft = |name,
                        [stretch, contrast, diffusion, low, tint, hue]: [f64; 6],
                        [exterior, softness, width, corner]: [f64; 4]| {
                p(
                    name,
                    vec![
                        ("stretchRange", num(&[stretch])),
                        ("contrast", num(&[contrast])),
                        ("diffusion", num(&[diffusion])),
                        ("lowTrim", num(&[low])),
                        ("highTrim", num(&[0.0])),
                        ("tint", num(&[tint])),
                        ("correctionHue", num(&[hue])),
                        ("exteriorEffect", num(&[exterior])),
                        ("strength", num(&[100.0])),
                        ("shape", num(&[0.0])),
                        ("softness", num(&[softness])),
                        ("width", num(&[width])),
                        ("height", num(&[80.0])),
                        ("horizontalBlur", num(&[0.0])),
                        ("verticalBlur", num(&[0.0])),
                        ("cornerRadius", num(&[corner])),
                        ("position", num(&[0.5, 0.5])),
                    ],
                )
            };
            vec![
                soft(
                    "Soft Moderate Contrast",
                    [17.8, 66.53, 15.25, 0.0, 0.0, 0.0],
                    [0.0, 20.0, 80.0, 0.0],
                ),
                soft(
                    "Heavy Contrast",
                    [32.63, 100.0, 0.0, 5.51, 0.0, 0.0],
                    [0.0, 20.0, 80.0, 0.0],
                ),
                soft(
                    "Warm Vignette",
                    [17.8, 66.53, 15.25, 0.0, 32.63, 42.71],
                    [2.0, 50.0, 84.98, 15.0],
                ),
            ]
        }
        Kind::FillLight => {
            let m = |name: &'static str, strength: f64, bias: f64, tint: f64| {
                p(
                    name,
                    vec![
                        ("hue", num(&[33.42])),
                        ("strength", num(&[strength])),
                        ("bias", num(&[bias])),
                        ("tint", num(&[tint])),
                        ("softness", num(&[3.39])),
                    ],
                )
            };
            vec![
                m("Moderate", 52.13, 0.0, 0.0),
                m("Strong, Warm", 75.86, -66.31, 100.0),
            ]
        }
        Kind::LensFlare => {
            // VEGAS's presets: a lens each, Intensity 0.5.
            let lens = |name, kind: f64| {
                p(
                    name,
                    vec![
                        ("lightColor", num(&[1.0, 1.0, 1.0])),
                        ("lightPosition", num(&[0.5, -0.5])),
                        ("lensType", num(&[kind])),
                        ("tint", num(&[0.0])),
                        ("intensity", num(&[0.5])),
                        ("blend", num(&[1.0])),
                        ("size", num(&[1.0])),
                        ("perspective", num(&[0.0])),
                    ],
                )
            };
            vec![
                lens("35mm Lens", 0.0),
                lens("105mm Prime Lens", 1.0),
                lens("70-300mm Zoom Lens", 2.0),
                lens("Standard Lens", 3.0),
                lens("Sunburst Effect", 4.0),
            ]
        }
        Kind::Starburst => {
            // VEGAS's two.
            let star = |name, brilliance, burst| {
                p(
                    name,
                    vec![
                        ("threshold", num(&[72.51])),
                        ("brilliance", num(&[brilliance])),
                        ("strength", num(&[100.0])),
                        ("radius", num(&[16.63])),
                        ("points", num(&[4.0])),
                        ("centerBurst", num(&[burst])),
                        ("aspect", num(&[1.0])),
                        ("orientation", num(&[45.0])),
                        ("softenEdges", num(&[0.0])),
                    ],
                )
            };
            vec![
                star("Simple 4 Point", 878.39, 0.0),
                star("Bright Center", 1796.1, 87.29),
            ]
        }
        Kind::Rays => {
            // VEGAS's two.
            let r = |name,
                     alpha: bool,
                     [
                amount,
                aspect,
                boost,
                noise,
                hue,
                sweep,
                saturation,
                quality,
            ]: [f64; 8],
                     [kind, progress, strength]: [f64; 3]| {
                p(
                    name,
                    vec![
                        ("keyAlphaChannel", num(&[f64::from(u8::from(alpha))])),
                        ("threshold", num(&[38.56])),
                        ("amount", num(&[amount])),
                        ("position", num(&[0.5, 0.5])),
                        ("aspect", num(&[aspect])),
                        ("boost", num(&[boost])),
                        ("peak", num(&[100.0])),
                        ("noise", num(&[noise])),
                        ("hue", num(&[hue])),
                        ("hueSweep", num(&[sweep])),
                        ("saturation", num(&[saturation])),
                        ("quality", num(&[quality])),
                        ("maskMode", num(&[0.0])),
                        ("maskType", num(&[kind])),
                        ("maskProgress", num(&[progress])),
                        ("maskStrength", num(&[strength])),
                    ],
                )
            };
            vec![
                r(
                    "Luminance",
                    false,
                    [38.23, 1.0, -33.81, 0.0, 211.56, 48.81, 39.41, 0.5],
                    [0.0, 50.0, 100.0],
                ),
                r(
                    "Alpha",
                    true,
                    [25.0, 1.39, -100.0, 29.24, 41.74, 180.0, 60.0, 1.0],
                    [2.0, 65.64, 0.0],
                ),
            ]
        }
        Kind::Glint => {
            // VEGAS's two.
            let gl = |name,
                      [threshold, boost, rh, rv, hue, sweep, saturation]: [f64; 7],
                      [mode, kind]: [f64; 2]| {
                p(
                    name,
                    vec![
                        ("threshold", num(&[threshold])),
                        ("boost", num(&[boost])),
                        ("horizontalRadius", num(&[rh])),
                        ("verticalRadius", num(&[rv])),
                        ("hue", num(&[hue])),
                        ("hueSweep", num(&[sweep])),
                        ("saturation", num(&[saturation])),
                        ("orientation", num(&[0.0])),
                        ("streaks", num(&[3.0])),
                        ("maskMode", num(&[mode])),
                        ("maskType", num(&[kind])),
                    ],
                )
            };
            vec![
                gl(
                    "Sparkle",
                    [67.04, 44.92, 62.04, 62.04, 116.78, 240.0, 79.49],
                    [0.0, 0.0],
                ),
                gl(
                    "Center Mask, Glint",
                    [81.95, 88.8, 57.74, 73.88, 98.0, 17.7, 60.0],
                    [1.0, 3.0],
                ),
            ]
        }
        Kind::LightRays => {
            // VEGAS's presets: Tint color, Sensitivity and Strength, Bound
            // radius, Radius X and Y and Feather, Blend and Noise; the
            // light in the middle.
            type Row = (&'static str, [f64; 3], [f64; 2], bool, [f64; 3], [f64; 2]);
            let rows: [Row; 11] = [
                (
                    "Weak Light Rays",
                    [1.0; 3],
                    [0.6, 0.25],
                    false,
                    [0.5, 0.5, 0.5],
                    [0.0, 0.0],
                ),
                (
                    "Moderate Light Rays",
                    [1.0; 3],
                    [0.6, 0.6],
                    false,
                    [0.5, 0.5, 0.5],
                    [0.0, 0.0],
                ),
                (
                    "Intense Light Rays",
                    [1.0; 3],
                    [0.6, 0.95],
                    false,
                    [0.5, 0.5, 0.5],
                    [0.0, 0.0],
                ),
                (
                    "High Sensitivity",
                    [1.0; 3],
                    [1.0, 0.5],
                    false,
                    [0.5, 0.5, 0.5],
                    [0.0, 0.0],
                ),
                (
                    "Low Sensitivity",
                    [1.0; 3],
                    [0.0, 0.5],
                    false,
                    [0.5, 0.5, 0.5],
                    [0.0, 0.0],
                ),
                (
                    "Spotlight with Feather",
                    [1.0; 3],
                    [0.6, 0.6],
                    false,
                    [0.2, 0.2, 0.5],
                    [0.0, 0.0],
                ),
                (
                    "Moderate Bounded Spotlight",
                    [1.0; 3],
                    [0.6, 0.6],
                    true,
                    [0.5, 0.5, 0.0],
                    [0.0, 0.0],
                ),
                (
                    "Cyan Spotlight",
                    [0.0, 1.0, 0.983333],
                    [0.8, 0.6],
                    true,
                    [0.12, 0.12, 0.5],
                    [0.0, 0.0],
                ),
                (
                    "Ambient Red Spotlight",
                    [1.0, 0.2, 0.0],
                    [0.8, 0.6],
                    true,
                    [0.12, 0.12, 0.5],
                    [0.16, 0.0],
                ),
                (
                    "Ambient Bright Yellow Spotlight",
                    [1.0, 1.0, 0.0],
                    [0.8, 0.6],
                    true,
                    [0.12, 0.12, 0.3],
                    [0.08, 0.0],
                ),
                (
                    "Sparkling Light Rays",
                    [1.0; 3],
                    [0.2, 0.8],
                    false,
                    [0.5, 0.5, 0.0],
                    [0.0, 0.2],
                ),
            ];
            rows.into_iter()
                .map(
                    |(
                        name,
                        tint,
                        [sensitivity, strength],
                        bound,
                        [rx, ry, feather],
                        [blend, noise],
                    )| {
                        p(
                            name,
                            vec![
                                ("lightSource", num(&[0.5, 0.5])),
                                ("tintColor", num(&tint)),
                                ("sensitivity", num(&[sensitivity])),
                                ("strength", num(&[strength])),
                                ("boundRadius", num(&[f64::from(u8::from(bound))])),
                                ("radiusX", num(&[rx])),
                                ("radiusY", num(&[ry])),
                                ("feather", num(&[feather])),
                                ("blend", num(&[blend])),
                                ("noise", num(&[noise])),
                            ],
                        )
                    },
                )
                .collect()
        }
        Kind::FilmEffects => {
            // VEGAS's presets: Grain, Tint, Color, Convert to grayscale,
            // and Dust's Amount.
            let f = |name, grain: f64, tint: f64, color: [f64; 3], gray: bool, dust: f64| {
                p(
                    name,
                    vec![
                        ("grain", num(&[grain])),
                        ("tint", num(&[tint])),
                        ("color", num(&color)),
                        ("convertToGrayscale", num(&[f64::from(u8::from(gray))])),
                        ("enable", num(&[1.0])),
                        ("type", num(&[0.0])),
                        ("amount", num(&[dust])),
                    ],
                )
            };
            vec![
                f(
                    "Very Old Film",
                    0.15,
                    0.25,
                    [0.75, 0.716875, 0.3525],
                    true,
                    0.5,
                ),
                f(
                    "Low Quality Color Film",
                    0.15,
                    0.35,
                    [0.352941, 0.717647, 0.749020],
                    false,
                    0.2,
                ),
                f(
                    "Circa 1908",
                    0.25,
                    0.35,
                    [0.749020, 0.717647, 0.352941],
                    true,
                    0.5,
                ),
                f(
                    "Circa 1980",
                    0.1,
                    0.2,
                    [0.352941, 0.717647, 0.749020],
                    false,
                    0.1,
                ),
            ]
        }
        Kind::FilmGrain => {
            let w = |name, a: f64, g: f64, c: f64| {
                p(
                    name,
                    vec![
                        ("amount", num(&[a])),
                        ("granularity", num(&[g])),
                        ("chromaticity", num(&[c])),
                    ],
                )
            };
            vec![
                w("Subtle", 0.312, 0.14, 0.0),
                w("Extreme", 0.74, 0.384, 0.0),
                w("Color", 0.74, 0.652, 1.0),
            ]
        }
        Kind::Newsprint => {
            // VEGAS's presets: Dot size and Type.
            let n = |name, dot: f64, ty: f64| {
                p(name, vec![("dotSize", num(&[dot])), ("type", num(&[ty]))])
            };
            vec![
                n("Newspaper", 0.12, 1.0),
                n("Color Print", 0.12, 0.0),
                n("Offset Print", 0.32, 2.0),
                n("Newspaper Zoomed", 0.62, 1.0),
                n("Color Print Zoomed", 0.62, 0.0),
                n("Offset Print Zoomed", 0.82, 2.0),
            ]
        }
        Kind::TvSimulator => {
            // VEGAS's presets: Detail zoom, Aperture grill, Interlacing,
            // Line sync, Vertical sync, Scan phasing, Phosphorescence,
            // Static.
            let names = [
                "detailZoom",
                "apertureGrill",
                "interlacing",
                "lineSync",
                "verticalSync",
                "scanPhasing",
                "phosphorescence",
                "static",
            ];
            let tv = |name, v: [f64; 8]| {
                p(
                    name,
                    names.iter().zip(v).map(|(&n, x)| (n, num(&[x]))).collect(),
                )
            };
            vec![
                tv("TV Look", [0.0, 0.32, 0.66, 1.0, 1.0, 0.15, 0.9, 0.0]),
                tv("Bad Sync", [0.0, 0.32, 0.78, 0.32, 0.25, 0.2, 0.5, 0.0]),
            ]
        }
        Kind::BumpMap => {
            let light =
                |name: &'static str,
                 source: [f64; 3],
                 destination: [f64; 2],
                 kind: f64,
                 [intensity, focus, ambience, shininess, height]: [f64; 5]| {
                    p(
                        name,
                        vec![
                            ("source", num(&source)),
                            ("destination", num(&destination)),
                            ("lightType", num(&[kind])),
                            ("intensity", num(&[intensity])),
                            ("focus", num(&[focus])),
                            ("ambience", num(&[ambience])),
                            ("shininess", num(&[shininess])),
                            ("bumpHeight", num(&[height])),
                            ("bumpChannel", num(&[1.0])),
                            ("whiteIsHigh", num(&[1.0])),
                            ("flip", num(&[0.0])),
                        ],
                    )
                };
            vec![
                light(
                    "Spotlight Up (flat)",
                    [0.0, -1.7, 0.55],
                    [0.0, 0.0],
                    1.0,
                    [0.4, 0.4, 0.1, 0.0, 0.0],
                ),
                light(
                    "Lower-Left Spotlight",
                    [-1.8, -1.1, 0.75],
                    [0.25, 0.2],
                    1.0,
                    [0.55, 0.4, 0.1, 0.5, 0.3],
                ),
                light(
                    "Upper-Right Glow",
                    [0.65, 0.4, 0.3],
                    [0.0, 0.0],
                    2.0,
                    [0.15, 0.5, 0.1, 1.0, 0.5],
                ),
            ]
        }
        // A preset a look, as VEGAS's (none has none).
        Kind::Looks => LOOK_NAMES
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, n)| one(n.to_str().unwrap_or(""), "look", i as f64))
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Grid {
        Grid {
            origin: [0.0, 0.0],
            frame: [32.0, 18.0],
            scale: [1.0, 1.0],
            par: 1.0,
        }
    }

    /// `kind`'s defaults with `values` over them.
    fn vals(kind: Kind, values: &[(&str, &[f64])]) -> Vals {
        Vals::of(kind.specs(), &|spec| {
            let key = spec.name().to_str().unwrap_or("");
            values
                .iter()
                .find(|(n, _)| *n == key)
                .map_or_else(|| spec.default(), |(_, v)| v.to_vec())
        })
    }

    fn flat(c: [f32; 4]) -> Buf {
        Buf::solid([0, 0, 32, 18], c)
    }

    #[test]
    fn lab_goes_there_and_back() {
        for c in [
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.2, 0.5, 0.8],
            [0.9, 0.1, 0.3],
            [1.5, 0.7, -0.1],
        ] {
            let back = from_lab(lab(c));
            for i in 0..3 {
                assert!((back[i] - c[i]).abs() < 1e-5, "{c:?} {back:?}");
            }
        }
        // White is L 100, gray has no color.
        let w = lab([1.0, 1.0, 1.0]);
        assert!((w[0] - 100.0).abs() < 1e-3 && w[1].abs() < 1e-3 && w[2].abs() < 1e-3);
    }

    #[test]
    fn lab_adjust_desaturates_and_inverts_the_lightness() {
        let g = grid();
        let src = flat([0.9, 0.3, 0.2, 1.0]);
        let gray = process(
            Kind::LabAdjust,
            &vals(Kind::LabAdjust, &[("desaturate", &[1.0])]),
            &g,
            &src,
            0.0,
        );
        let c = gray.at(5, 5);
        assert!(
            (c[0] - c[1]).abs() < 1e-4 && (c[1] - c[2]).abs() < 1e-4,
            "{c:?}"
        );
        // Black turns white.
        let black = flat([0.0, 0.0, 0.0, 1.0]);
        let inverted = process(
            Kind::LabAdjust,
            &vals(Kind::LabAdjust, &[("invertLuminance", &[1.0])]),
            &g,
            &black,
            0.0,
        );
        assert!(
            inverted.at(1, 1)[..3]
                .iter()
                .all(|v| (v - 1.0).abs() < 1e-4),
            "{:?}",
            inverted.at(1, 1)
        );
        // VEGAS's Lightness scales L; Channel a and b move a and b by 100
        // times theirs; half transparent pixels keep their alpha.
        let half = flat([0.25, 0.15, 0.1, 0.5]);
        let s = vals(
            Kind::LabAdjust,
            &[
                ("lightness", &[0.5]),
                ("channelA", &[0.1]),
                ("channelB", &[-0.2]),
            ],
        );
        let out = process(Kind::LabAdjust, &s, &g, &half, 0.0).at(2, 2);
        let [l, a, b] = lab([0.5, 0.3, 0.2]);
        let want = from_lab([l * 0.5, a + 10.0, b - 20.0]);
        assert!((out[3] - 0.5).abs() < 1e-6, "{out:?}");
        for k in 0..3 {
            assert!((out[k] - want[k] * 0.5).abs() < 1e-5, "{out:?} {want:?}");
        }
        // Nothing where there is no alpha.
        let clear = flat([0.0, 0.0, 0.0, 0.0]);
        let out = process(Kind::LabAdjust, &s, &g, &clear, 0.0).at(0, 0);
        assert_eq!(out, [0.0; 4]);
        assert_eq!(presets(Kind::LabAdjust).len(), 9, "VEGAS's nine");
    }

    #[test]
    fn fill_light_adds_its_light_to_the_darker_tones_as_vegas_does() {
        // The bump: nothing at black and from 85 % up, 0.3 at the knee.
        assert_eq!(fill_bump(0.0, 0.25), 0.0);
        assert!((fill_bump(0.25, 0.25) - 0.3).abs() < 1e-12);
        assert_eq!(fill_bump(0.9, 0.25), 0.0);
        // The light's color: gray without Tint; red turned a third of the
        // way round toward yellow keeps its luma, so at 33.42° it warms.
        assert_eq!(fill_color(33.42, 0.0), [1.0; 3]);
        let warm = fill_color(33.42, 0.1);
        assert!(warm[0] > warm[1] && warm[1] > warm[2], "{warm:?}");
        let luma = 0.299 * warm[0] + 0.587 * warm[1] + 0.114 * warm[2];
        assert!((luma - 1.0).abs() < 1e-5, "{luma}");
        // Moderate: a dark gray gets 0.5 × 52.13 % of its table's light
        // (no softness to spread it on a flat picture), white stays white.
        assert!(
            presets(Kind::FillLight)
                .iter()
                .any(|p| p.name == "Moderate")
        );
        let s = vals(
            Kind::FillLight,
            &[
                ("hue", &[33.42]),
                ("strength", &[52.13]),
                ("bias", &[0.0]),
                ("tint", &[0.0]),
                ("softness", &[3.39]),
            ],
        );
        let f = FillLight::of(&s).expect("light");
        // Its blur: 0.002 × Softness of the longer side.
        let r = f.radii([1920.0, 1080.0], 1.0).expect("blur");
        assert!((r[0] - 0.002 * 3.39 * 1920.0).abs() < 1e-9 && r[0] == r[1]);
        let wide = f.radii([960.0, 1080.0], 2.0).expect("blur");
        assert!((wide[0] - 0.002 * 3.39 * 960.0).abs() < 1e-9);
        assert!((wide[1] - wide[0] / 2.0).abs() < 1e-9);
        let dark = [0.2, 0.2, 0.2, 1.0];
        let lit = process(Kind::FillLight, &s, &grid(), &flat(dark), 0.0).at(5, 5);
        // VEGAS's filter keeps a flat picture within a few ten-thousandths.
        let want = 0.2 + f.amount * f.table[fill_index(dark)][0];
        assert!(
            (lit[0] - want).abs() < 5e-4 && lit[0] > 0.23,
            "{lit:?} {want}"
        );
        let white = process(Kind::FillLight, &s, &grid(), &flat([1.0; 4]), 0.0).at(5, 5);
        assert_eq!(white, [1.0; 4]);
    }

    #[test]
    fn neutral_settings_leave_the_picture_alone() {
        let g = grid();
        let src = flat([0.3, 0.6, 0.2, 1.0]);
        let same = |kind: Kind, values: &[(&str, &[f64])]| {
            let out = process(kind, &vals(kind, values), &g, &src, 0.0);
            out.data
                .iter()
                .zip(&src.data)
                .all(|(a, b)| a.iter().zip(b).all(|(p, q)| (p - q).abs() < 1e-4))
        };
        assert!(same(Kind::LabAdjust, &[]));
        assert!(same(Kind::Looks, &[("level", &[0.0])]));
        assert!(same(Kind::FillLight, &[("strength", &[0.0])]));
        assert!(same(
            Kind::Denoise,
            &[("luminance", &[0.0]), ("color", &[0.0])]
        ));
        // A flat picture has no bumps: lit evenly by a directional light.
        let lit = process(
            Kind::BumpMap,
            &vals(Kind::BumpMap, &[("lightType", &[0.0])]),
            &g,
            &src,
            0.0,
        );
        let (a, b) = (lit.at(2, 2), lit.at(29, 15));
        assert!((a[1] - b[1]).abs() < 1e-5, "{a:?} {b:?}");
    }

    /// VEGAS's Bump Map on a flat picture: the light alone, as its sum
    /// gives it (Directional, Source -1, 1, 3).
    #[test]
    fn bump_map_lights_a_flat_picture_as_vegas_does() {
        let g = grid();
        let src = flat([0.5, 0.5, 0.5, 1.0]);
        let out = process(Kind::BumpMap, &vals(Kind::BumpMap, &[]), &g, &src, 0.0).at(5, 5);
        let l = unit((-1.0, 1.0, 3.0));
        let spec = l.2.powf(8.0);
        let k = (spec * 0.25 + l.2 * 0.5) * 0.25 * 8.0 + 0.2;
        assert!((f64::from(out[0]) - 0.5 * k).abs() < 1e-5, "{out:?} {k}");
        assert_eq!(out[3], 1.0);
        assert_eq!(presets(Kind::BumpMap).len(), 3, "VEGAS's three");
    }

    #[test]
    fn newsprint_keeps_paper_white_and_ink_black() {
        let g = grid();
        for ty in [0.0, 1.0, 2.0] {
            let s = vals(Kind::Newsprint, &[("type", &[ty])]);
            let white = process(Kind::Newsprint, &s, &g, &flat([1.0; 4]), 0.0);
            assert!(
                white.data.iter().all(|c| c[0] > 0.99 && c[2] > 0.99),
                "{ty}"
            );
        }
        let s = vals(Kind::Newsprint, &[("type", &[0.0])]);
        let black = process(Kind::Newsprint, &s, &g, &flat([0.0, 0.0, 0.0, 1.0]), 0.0);
        let mean = black.data.iter().map(|c| c[0]).sum::<f32>() / black.data.len() as f32;
        assert!(mean < 0.05, "{mean}");
    }

    #[test]
    fn denoise_smooths_noise_away() {
        let g = grid();
        let mut src = flat([0.5, 0.5, 0.5, 1.0]);
        for (k, c) in src.data.iter_mut().enumerate() {
            let n = (hash01(k as u32, 7, 3) as f32 - 0.5) * 0.1;
            *c = [0.5 + n, 0.5 + n, 0.5 + n, 1.0];
        }
        let spread = |b: &Buf| {
            let m = b.data.iter().map(|c| c[0]).sum::<f32>() / b.data.len() as f32;
            b.data.iter().map(|c| (c[0] - m).powi(2)).sum::<f32>() / b.data.len() as f32
        };
        // At its defaults: patches this alike weigh all the same.
        let out = process(Kind::Denoise, &vals(Kind::Denoise, &[]), &g, &src, 0.0);
        assert!(
            spread(&out) < spread(&src) * 0.05,
            "{} {}",
            spread(&out),
            spread(&src)
        );
        // Much weaker: the pixel's own weight, others' far below.
        let out = process(
            Kind::Denoise,
            &vals(Kind::Denoise, &[("luminance", &[0.5]), ("color", &[0.5])]),
            &g,
            &src,
            0.0,
        );
        assert!(spread(&out) > spread(&src) * 0.7, "{}", spread(&out));
    }

    #[test]
    fn denoise_is_vegass() {
        // Sharpening's centers: 100 at the least, 13 at the most, the jump
        // at an eighth.
        assert_eq!(sharpen_center(0.0), 100);
        assert_eq!(sharpen_center(0.1), 62);
        assert_eq!(sharpen_center(0.124), 55);
        assert_eq!(sharpen_center(0.125), 44);
        assert_eq!(sharpen_center(0.25), 25);
        assert_eq!(sharpen_center(1.0), 13);
        // The weights: whole up to 2h², then falling, none under 0.001.
        assert_eq!(patch_weight(18.0, 3.0), 1.0);
        assert!((patch_weight(27.0, 3.0) - (-1.0f64).exp()).abs() < 1e-12);
        assert_eq!(patch_weight(90.0, 3.0), 0.0);
        // The planes and back, near enough.
        let c = [0.3, 0.6, 0.2, 1.0];
        let back = rgb_of(yuv_of(c));
        assert!(
            c.iter().zip(back).all(|(a, b)| (a - b).abs() < 2e-4),
            "{back:?}"
        );
        assert_eq!(reflected(-2, 10), 2);
        assert_eq!(reflected(11, 10), 7);
        // An edge stays but at its next pixels: there a move of one weighs
        // exp(−(22 − 18)/9) (7 columns of 153 levels over 49), of two
        // 0.056, of three 0.005 — the side across a fifth of the mix.
        let mut edge = flat([0.2, 0.2, 0.2, 1.0]);
        let w = edge.width();
        for (k, c) in edge.data.iter_mut().enumerate() {
            if k as c_int % w >= w / 2 {
                *c = [0.8, 0.8, 0.8, 1.0];
            }
        }
        let g = grid();
        let out = process(Kind::Denoise, &vals(Kind::Denoise, &[]), &g, &edge, 0.0);
        let row = edge.height() / 2;
        let next = out.at(w / 2 - 1, row)[0];
        assert!(next > 0.36 && next < 0.39, "{next}");
        assert!(
            (out.at(w / 2 - 4, row)[0] - 0.2).abs() < 2e-3,
            "{:?}",
            out.at(w / 2 - 4, row)
        );
        assert!((out.at(w / 2 + 3, row)[0] - 0.8).abs() < 2e-3);
        // Sharpening raises the edge's step: at the most, 12 times the
        // difference from the neighbours' weighted mean.
        let out = process(
            Kind::Denoise,
            &vals(
                Kind::Denoise,
                &[("luminance", &[0.0]), ("color", &[0.0]), ("amount", &[1.0])],
            ),
            &g,
            &edge,
            0.0,
        );
        assert_eq!(out.at(w / 2 - 1, row)[0], 0.0);
        assert_eq!(out.at(w / 2, row)[0], 1.0);
    }

    #[test]
    fn grain_and_static_move_with_time() {
        let g = grid();
        let src = flat([0.5, 0.5, 0.5, 1.0]);
        for kind in [Kind::FilmGrain, Kind::TvSimulator, Kind::FilmEffects] {
            let s = vals(kind, &[]);
            let (a, b) = (
                process(kind, &s, &g, &src, 0.0),
                process(kind, &s, &g, &src, 1.0),
            );
            assert!(a.data != b.data, "{kind:?}");
            // The same time, the same picture.
            assert!(process(kind, &s, &g, &src, 1.0).data == b.data, "{kind:?}");
        }
    }

    /// Glint: a light streaked along its lines, through the mask on
    /// Effect.
    #[test]
    fn glint_streaks_a_light() {
        let g = grid();
        let mut src = flat([0.0, 0.0, 0.0, 1.0]);
        src.data[9 * 32 + 16] = [1.0; 4];
        let s = vals(
            Kind::Glint,
            &[
                ("threshold", &[50.0]),
                ("streaks", &[1.0]),
                ("horizontalRadius", &[100.0]),
                ("saturation", &[0.0]),
            ],
        );
        let out = process(Kind::Glint, &s, &g, &src, 0.0);
        // Along the line across, not above.
        assert!(out.at(19, 9)[0] > 0.01, "{:?}", out.at(19, 9));
        assert!(out.at(16, 12)[0] < 1e-4, "{:?}", out.at(16, 12));
    }

    /// Rays: the lights stream away from Position; Show mask only shows
    /// the mask.
    #[test]
    fn rays_stream_from_the_light() {
        let g = grid();
        let mut src = flat([0.0, 0.0, 0.0, 1.0]);
        src.data[9 * 32 + 20] = [1.0; 4];
        let s = vals(
            Kind::Rays,
            &[
                ("threshold", &[50.0]),
                ("amount", &[100.0]),
                ("boost", &[100.0]),
                ("saturation", &[0.0]),
                ("position", &[0.25, 0.5]),
            ],
        );
        let out = process(Kind::Rays, &s, &g, &src, 0.0);
        // Away from the light (at the left), past the bright pixel.
        assert!(out.at(24, 9)[0] > 0.01, "{:?}", out.at(24, 9));
        assert!(out.at(16, 9)[0] < out.at(24, 9)[0], "{:?}", out.at(16, 9));
        let mask = vals(Kind::Rays, &[("maskMode", &[3.0]), ("maskType", &[1.0])]);
        let shown = process(Kind::Rays, &mask, &g, &src, 0.0);
        assert_eq!(shown.at(16, 9), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(shown.at(0, 0), [0.0, 0.0, 0.0, 1.0]);
    }

    /// VEGAS's Starburst: a light's rays along its points, none between.
    #[test]
    fn starburst_rays_leave_each_light() {
        let g = grid();
        let mut src = flat([0.0, 0.0, 0.0, 1.0]);
        src.data[9 * 32 + 16] = [1.0; 4];
        // Four rays, the first straight up (Orientation 180: 450°).
        let s = vals(
            Kind::Starburst,
            &[
                ("threshold", &[50.0]),
                ("strength", &[100.0]),
                ("radius", &[30.0]),
                ("orientation", &[180.0]),
            ],
        );
        let out = process(Kind::Starburst, &s, &g, &src, 0.0);
        assert!(out.at(16, 12)[0] > 0.01, "{:?}", out.at(16, 12));
        assert!(out.at(19, 9)[0] > 0.01, "{:?}", out.at(19, 9));
        assert!(out.at(19, 12)[0] < 1e-4, "{:?}", out.at(19, 12));
        // Strength 0 leaves the picture.
        let none = vals(Kind::Starburst, &[("threshold", &[50.0])]);
        assert_eq!(
            process(Kind::Starburst, &none, &g, &src, 0.0).data,
            src.data
        );
    }
}
