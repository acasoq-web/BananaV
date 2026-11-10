//! BananaV's own video effects, written as OpenFX image effect plug-ins.
//! BananaV hosts them as plug-ins built into the program
//! ([`number_of_plugins`] and [`get_plugin`] are the binary's entry
//! points), and `apps/ofx-bundle` builds the same code as `BananaV.ofx`
//! for any other OFX host. They only use what the OpenFX specification
//! defines: what BananaV shows of them comes from how they describe
//! themselves.
//!
//! - `org.bananav.Crop` ([`crop`]): keeps a window of the picture, moved,
//!   turned and faded, with handles on the preview.
//! - `org.bananav.ColorCurves` ([`color_curves`]): the red, green and blue
//!   channels each through a curve (a parametric parameter), set by hand
//!   or from the picture.
//! - `org.bananav.ColorGrading` ([`color_grading`]): the Color Grading
//!   window's work — a camera and a look LUT, color wheels, range-limited
//!   wheels, levels, tone, white balance, HSL, curves and HSL curves.
//! - `org.bananav.CookieCutter` ([`cookie_cutter`]): the picture kept in a
//!   shape, or the shape cut out of it, with a border and a soft edge.
//! - `org.bananav.LayerDimensionality` ([`layer_dimensionality`]): a
//!   shadow dropped behind or cast inside, a glow and an embossed surface
//!   for a picture with transparency, the light's cross on the preview.
//! - `org.bananav.ColorCorrector` ([`color_corrector`]): colors added to
//!   the dark, middle and light parts (wheels drawn with the draw suite,
//!   eyedroppers on the preview), saturation, gamma, gain and offset.
//! - `org.bananav.Pixelate` ([`pixelate`]): the picture as blocks of one
//!   color, as wide and as tall as its two settings say.
//! - `org.bananav.LutFilter` ([`lut_filter`]): the colors through a
//!   look-up table read from a `.cube` file (`bv-lut`).
//! - `org.bananav.WhiteBalance` ([`white_balance`]): a light's cast taken
//!   away: the color that should be gray, or its temperature and tint.
//! - `org.bananav.HslAdjust` ([`hsl_adjust`]): the colors' hue turned,
//!   their saturation and lightness scaled.
//! - `org.bananav.PictureInPicture` ([`picture_in_picture`]): the picture
//!   moved, turned, scaled and sheared over a transparent frame, or pinned
//!   by its corners, with handles on the preview.
//! - `org.bananav.SaturationAdjust` ([`saturation_adjust`]): the colors'
//!   saturation changed by how saturated they are.
//! - `org.bananav.Border` ([`border`]): a border around the frame, blurred,
//!   beveled or solid.
//! - `org.bananav.BlackBarFill` ([`black_bar_fill`]): the bars around a
//!   picture of other proportions filled with the picture itself,
//!   enlarged and blurred, with crosshairs on the preview.
//! - `org.bananav.UnsharpMask` ([`unsharp_mask`]): the picture sharpened
//!   against itself blurred.
//! - `org.bananav.AddNoise` ([`add_noise`]): random noise added, gray or
//!   colored, still or moving.
//! - `org.bananav.Timecode` ([`timecode`]): the frame's time as the ruler
//!   shows it, written over the picture (Arimo Bold's glyphs, [`arimo`],
//!   drawn by [`text`]).
//! - `org.bananav.ColorBalance` ([`color_balance`]): the colors pushed
//!   toward red, green or blue in the light parts, the middle tones or the
//!   dark parts.
//! - `org.bananav.ColorCorrectorSecondary` ([`color_corrector_secondary`]):
//!   the Color Corrector's corrections on the colors its limits pick (a
//!   range of luminance, saturation and hue), set from the preview too.
//! - `org.bananav.TitlesText` ([`titles`]): a generator of text in the
//!   computer's fonts ([`fonts`]), with an outline, a shadow and a
//!   background, its box moved and scaled on the preview.
//! - VEGAS's everyday ones, each measured on its pictures: Brightness and
//!   Contrast ([`brightness_contrast`]), Levels ([`levels`]), Black and
//!   White ([`black_and_white`]), Sepia ([`sepia`]), Invert ([`invert`]),
//!   and on what the effects working a whole picture out share
//!   ([`spatial`]) Gaussian Blur ([`gaussian_blur`]), Sharpen
//!   ([`sharpen`]), Vignette ([`vignette`]), Mirror ([`mirror`]) and Glow
//!   ([`glow`]).
//! - More of VEGAS's video effects ([`filters`]): Tiny Planet, Defocus,
//!   Linear Blur, Quick Blur, Radial Blur, Median, Min and Max,
//!   Convolution Kernel, Spherize, Swirl, Threshold, Wave, Pinch/Punch,
//!   Offset and Wrap, Black Restore, Broadcast Colors, Channel Blend, Chroma
//!   Blur, Mask Generator, Lens Correction, Radial Pixelate and Deform, on
//!   what they share with the transitions ([`kit`]); and the color, light
//!   and film ones ([`looks`]): LAB Adjust, Soft Contrast, Gradient Map,
//!   Fill Light, Lens Flare, Starburst, Glint, Light Rays, Film Effects,
//!   Film Grain, Newsprint, TV Simulator, Bump Map, Denoise and Looks;
//!   and Chroma Keyer ([`classics`]).
//! - VEGAS's media generators ([`generators`]): Checkerboard, Color
//!   Gradient, Noise Texture and Test Pattern (`org.bananav.Checkerboard`
//!   and so on).
//! - `org.bananav.CreditRoll` ([`credit_roll`]): credits scrolling over
//!   the event or shown a few lines at a time, drawn by Titles & Text.
//! - The transitions ([`transitions`]): VEGAS's Linear Wipe, Clock Wipe,
//!   Iris, Barn Door, Venetian Blinds, Star Wipe, Push, Slide, Squeeze,
//!   Split, Swap, Zoom, Spiral, Flash, Dissolve, Cross Effect and Portals,
//!   3D Blinds, 3D Cascade, 3D Fly In/Out, 3D Shuffle, Page Loop, Page
//!   Peel and Page Roll, with their settings (`org.bananav.LinearWipe` and
//!   so on).
//!
//! Their factory presets ([`presets`]) are for BananaV, which lists them
//! with its own (OpenFX has no presets).

use std::ffi::c_int;
use std::ptr::null_mut;

use bv_ofx_sys::names::kOfxImageEffectPluginApi;
use bv_ofx_sys::{OfxPlugin, OfxPluginEntryPoint};

pub mod accel;
mod add_noise;
mod ai_guided;
mod arimo;
mod bezier_masking;
mod black_and_white;
mod black_bar_fill;
mod border;
pub mod brightness_contrast;
mod checkerboard;
mod classics;
mod color_balance;
mod color_corrector;
mod color_corrector_secondary;
mod color_curves;
mod color_gradient;
pub mod color_grading;
mod color_match;
mod colors;
mod compositors;
mod cookie_cutter;
mod credit_roll;
mod crop;
mod effect_mask;
mod fft;
mod filters;
pub mod flicker_control;
mod fonts;
mod gaussian_blur;
mod generators;
mod glow;
mod glsl;
mod gradient_map;
mod gradient_page;
mod gradient_wipe;
pub mod grading;
mod hsl_adjust;
mod invert;
mod kerning;
mod kit;
mod layer_dimensionality;
mod lens_correction;
mod levels;
mod looks;
mod lut_filter;
mod mesh_warp;
mod mirror;
mod motion_blur;
mod noise_tables;
mod noise_texture;
mod ofx;
mod optical_flow;
mod picture_in_picture;
mod pixelate;
mod r250;
mod recursive_gauss;
mod saturation_adjust;
mod scene_rotation;
mod sepia;
mod sharpen;
mod smart_zoom;
mod spatial;
mod spline;
pub mod stabilization;
pub mod stabilization_360;
mod stabilize;
mod stereo_adjust;
mod stitching;
mod test_pattern;
mod text;
mod timecode;
mod title_animations;
mod titles;
mod transitions;
mod unsharp_mask;
mod vignette;
mod white_balance;

use bv_core::curve::CurvePoint;

/// The name BananaV gives the binary its effects are built into.
pub const BINARY: &str = "BananaV";

/// The identifiers of BananaV's own plug-ins start with it.
pub const ID_PREFIX: &str = "org.bananav.";

/// Titles & Text's identifier (the interface offers it as VEGAS's
/// Titles & Text generator and Insert › Text Media).
pub const TITLES_ID: &str = "org.bananav.TitlesText";

/// What Motion Tracking imports from an effect (VEGAS's import menu: a
/// region following the effect's): the corners of Picture In Picture's
/// picture, or of Bézier Masking's mask `mask` (1–5; the box around a
/// curve), with `values` (the plug-in's at a time) on a frame `aspect`
/// wide for 1 high — 0–1 of the frame from its top left, round it from the
/// top left corner. `None` for other plug-ins or a mask turned off.
pub fn tracked_region(
    id: &str,
    values: accel::Values<'_>,
    aspect: f64,
    mask: usize,
) -> Option<[[f64; 2]; 4]> {
    let is = |c: &std::ffi::CStr| c.to_str().ok() == Some(id);
    // From 0–1 up to 0–1 down.
    let down = |q: [[f64; 2]; 4]| q.map(|p| [p[0], 1.0 - p[1]]);
    if is(picture_in_picture::ID) {
        let [tl, tr, bl, br] = picture_in_picture::Settings::of(values).quad(aspect);
        Some(down([tl, tr, br, bl]))
    } else if is(bezier_masking::ID) {
        bezier_masking::mask_box(values, mask, aspect).map(down)
    } else {
        None
    }
}

/// Warp Flow's identifier (the transition Smart Split puts between the
/// parts it leaves, as VEGAS's).
pub const WARP_FLOW_ID: &str = "org.bananav.WarpFlow";

#[repr(transparent)]
struct Shared(OfxPlugin);

// SAFETY: the structures are constant; hosts only read them.
#[allow(unsafe_code)]
unsafe impl Sync for Shared {}

const fn plugin(
    id: &'static std::ffi::CStr,
    major: u32,
    minor: u32,
    entry: OfxPluginEntryPoint,
) -> Shared {
    Shared(OfxPlugin {
        plugin_api: kOfxImageEffectPluginApi.as_ptr(),
        api_version: 1,
        plugin_identifier: id.as_ptr(),
        plugin_version_major: major,
        plugin_version_minor: minor,
        set_host: Some(ofx::set_host),
        main_entry: Some(entry),
    })
}

static PLUGINS: [Shared; 120] = [
    // 2.0: Location counts the frame 0–1, VEGAS's way.
    plugin(crop::ID, 2, 0, crop::main),
    plugin(color_curves::ID, 1, 0, color_curves::main),
    plugin(color_grading::ID, 2, 0, color_grading::main),
    // 1.1: Offset counts 8-bit steps, VEGAS's way.
    plugin(color_corrector::ID, 1, 1, color_corrector::main),
    plugin(cookie_cutter::ID, 1, 0, cookie_cutter::main),
    plugin(layer_dimensionality::ID, 1, 0, layer_dimensionality::main),
    plugin(pixelate::ID, 1, 0, pixelate::main),
    plugin(lut_filter::ID, 1, 0, lut_filter::main),
    plugin(white_balance::ID, 1, 0, white_balance::main),
    plugin(hsl_adjust::ID, 1, 0, hsl_adjust::main),
    plugin(picture_in_picture::ID, 1, 0, picture_in_picture::main),
    plugin(saturation_adjust::ID, 1, 0, saturation_adjust::main),
    plugin(border::ID, 1, 0, border::main),
    plugin(black_bar_fill::ID, 1, 0, black_bar_fill::main),
    plugin(unsharp_mask::ID, 1, 0, unsharp_mask::main),
    plugin(add_noise::ID, 1, 0, add_noise::main),
    plugin(timecode::ID, 1, 0, timecode::main),
    plugin(color_balance::ID, 1, 0, color_balance::main),
    // 1.1: Offset counts 8-bit steps, as the Color Corrector's.
    plugin(
        color_corrector_secondary::ID,
        1,
        1,
        color_corrector_secondary::main,
    ),
    // 2.0: VEGAS's units and defaults (`PluginFx::upgrade` converts).
    plugin(titles::ID, 2, 0, titles::main),
    // 2.0: VEGAS's panels, presets and math (the transitions below too).
    plugin(c"org.bananav.LinearWipe", 2, 0, transitions::linear_wipe),
    plugin(c"org.bananav.ClockWipe", 2, 0, transitions::clock_wipe),
    plugin(c"org.bananav.Iris", 2, 0, transitions::iris),
    plugin(c"org.bananav.BarnDoor", 2, 0, transitions::barn_door),
    plugin(
        c"org.bananav.VenetianBlinds",
        2,
        0,
        transitions::venetian_blinds,
    ),
    plugin(c"org.bananav.StarWipe", 2, 0, transitions::star_wipe),
    plugin(c"org.bananav.Push", 2, 0, transitions::push),
    plugin(c"org.bananav.Slide", 2, 0, transitions::slide),
    plugin(c"org.bananav.Squeeze", 2, 0, transitions::squeeze),
    plugin(c"org.bananav.Split", 2, 0, transitions::split),
    plugin(c"org.bananav.Swap", 2, 0, transitions::swap),
    plugin(c"org.bananav.Zoom", 2, 0, transitions::zoom),
    plugin(c"org.bananav.Spiral", 2, 0, transitions::spiral),
    plugin(c"org.bananav.Flash", 2, 0, transitions::flash),
    plugin(c"org.bananav.Dissolve", 2, 0, transitions::dissolve),
    plugin(c"org.bananav.CrossEffect", 2, 0, transitions::cross_effect),
    plugin(c"org.bananav.Portals", 2, 0, transitions::portals),
    // 2.0: VEGAS's settings (Latitude 0 the bottom pole, Z in frames).
    plugin(c"org.bananav.TinyPlanet", 2, 0, filters::tiny_planet),
    // 2.0: VEGAS's panel (Radius in percent, its iris shapes, Orientation,
    // Disc curvature and the bloom in VEGAS's units).
    plugin(c"org.bananav.Defocus", 2, 0, filters::defocus),
    // 2.0: VEGAS's settings (Amount reaches the frame's longer side).
    plugin(c"org.bananav.LinearBlur", 2, 0, filters::linear_blur),
    // 2.0: VEGAS's Blend amount (it blurred all the way before).
    plugin(c"org.bananav.QuickBlur", 2, 0, filters::quick_blur),
    // 2.0: VEGAS's settings (Strength for Level).
    plugin(c"org.bananav.RadialBlur", 2, 0, filters::radial_blur),
    plugin(sharpen::ID, 1, 1, sharpen::main),
    // 2.0: VEGAS's settings, the ranges parts of the frame (they were
    // pixels).
    plugin(c"org.bananav.Median", 2, 0, filters::median),
    plugin(c"org.bananav.MinMax", 2, 0, filters::min_max),
    // 2.0: VEGAS's settings (Scale multiplies; Rotation turns the ring).
    plugin(c"org.bananav.ConvolutionKernel", 2, 0, filters::convolution),
    // 2.0: VEGAS's settings (Amount for Ratio; Swirl's turns clockwise).
    plugin(c"org.bananav.Spherize", 2, 0, filters::spherize),
    plugin(c"org.bananav.Swirl", 2, 0, filters::swirl),
    plugin(c"org.bananav.Threshold", 1, 0, filters::threshold),
    // 2.0: VEGAS's settings (amplitudes in frames, its order).
    plugin(c"org.bananav.Wave", 2, 0, filters::wave),
    // 2.0: VEGAS's settings (Pinch/Punch punches above 0; Offset and
    // Wrap's offset a move, not a place).
    plugin(c"org.bananav.PinchPunch", 2, 0, filters::pinch_punch),
    plugin(c"org.bananav.OffsetWrap", 2, 0, filters::offset_wrap),
    plugin(c"org.bananav.BlackRestore", 1, 0, filters::black_restore),
    // 2.0: VEGAS's settings, in IRE (they were 0–1 of the picture).
    plugin(
        c"org.bananav.BroadcastColors",
        2,
        0,
        filters::broadcast_colors,
    ),
    plugin(c"org.bananav.ChannelBlend", 1, 0, filters::channel_blend),
    plugin(c"org.bananav.ChromaBlur", 1, 0, filters::chroma_blur),
    // 2.0: VEGAS's settings (Type's list in its order).
    plugin(c"org.bananav.MaskGenerator", 2, 0, filters::mask_generator),
    // 2.0: VEGAS's panel (its lenses, the PanoTools model), rules and
    // math.
    plugin(lens_correction::ID, 2, 0, lens_correction::main),
    // 2.0: VEGAS's settings (Strength for Level).
    plugin(
        c"org.bananav.RadialPixelate",
        2,
        0,
        filters::radial_pixelate,
    ),
    // 2.0: VEGAS's settings (a curve for each edge).
    plugin(c"org.bananav.Deform", 2, 0, filters::deform),
    // 2.0: VEGAS's settings (Lightness scales; Brightness added).
    plugin(c"org.bananav.LabAdjust", 2, 0, filters::lab_adjust),
    // 2.0: VEGAS's panel (its units, the Vignette group).
    plugin(c"org.bananav.SoftContrast", 2, 0, filters::soft_contrast),
    plugin(gradient_map::ID, 2, 0, gradient_map::main),
    // 2.0: VEGAS's settings (Strength, Bias; Tint and Softness in percent).
    plugin(c"org.bananav.FillLight", 2, 0, filters::fill_light),
    plugin(c"org.bananav.LensFlare", 2, 0, filters::lens_flare),
    // 2.0: VEGAS's panel (its units, Points 2 to 6) and engine.
    plugin(c"org.bananav.Starburst", 2, 0, filters::starburst),
    // 2.0: VEGAS's panel (its units, Streaks, the Mask group).
    plugin(c"org.bananav.Glint", 2, 0, filters::glint),
    plugin(c"org.bananav.Rays", 1, 0, filters::rays),
    plugin(c"org.bananav.LightRays", 2, 0, filters::light_rays),
    plugin(c"org.bananav.FilmEffects", 2, 0, filters::film_effects),
    plugin(c"org.bananav.FilmGrain", 2, 0, filters::film_grain),
    plugin(c"org.bananav.Newsprint", 2, 0, filters::newsprint),
    plugin(c"org.bananav.TvSimulator", 2, 0, filters::tv_simulator),
    // 2.0: VEGAS's settings (Source in 3D, its light types' order).
    plugin(c"org.bananav.BumpMap", 2, 0, filters::bump_map),
    plugin(c"org.bananav.Denoise", 2, 0, filters::denoise),
    // 2.0: VEGAS's AutoLooks panel (None first, Strength).
    plugin(c"org.bananav.Looks", 2, 0, filters::looks),
    // 2.0: VEGAS's settings (its groups, Grid offset in frames); 2.1:
    // their names VEGAS's (`PluginFx::upgrade` renames them).
    plugin(
        c"org.bananav.Checkerboard",
        2,
        1,
        generators::checkerboard_entry,
    ),
    // 2.0: VEGAS's types, angle and up to nine color points on its axis.
    plugin(
        c"org.bananav.ColorGradient",
        2,
        0,
        generators::color_gradient_entry,
    ),
    // 2.0: VEGAS's panel, styles and presets; 2.1: its parameters' names
    // VEGAS's (`PluginFx::upgrade` renames them).
    plugin(
        c"org.bananav.NoiseTexture",
        2,
        1,
        generators::noise_texture_entry,
    ),
    plugin(
        c"org.bananav.SolidColor",
        1,
        0,
        generators::solid_color_entry,
    ),
    // 2.0: VEGAS's list of patterns; 2.1: its parameter's name VEGAS's
    // (`PluginFx::upgrade` renames it).
    plugin(
        c"org.bananav.TestPattern",
        2,
        1,
        generators::test_pattern_entry,
    ),
    // 2.0: VEGAS's settings (its ways, displays, sizes and room).
    plugin(c"org.bananav.CreditRoll", 2, 0, credit_roll::main),
    plugin(c"org.bananav.Blinds3D", 2, 0, transitions::blinds_3d),
    plugin(c"org.bananav.Cascade3D", 2, 0, transitions::cascade_3d),
    plugin(c"org.bananav.FlyInOut3D", 2, 0, transitions::fly_in_out_3d),
    plugin(c"org.bananav.Shuffle3D", 2, 0, transitions::shuffle_3d),
    plugin(c"org.bananav.PageLoop", 2, 0, transitions::page_loop),
    plugin(c"org.bananav.PagePeel", 2, 0, transitions::page_peel),
    plugin(c"org.bananav.PageRoll", 2, 0, transitions::page_roll),
    plugin(brightness_contrast::ID, 1, 1, brightness_contrast::main),
    plugin(levels::ID, 1, 1, levels::main),
    plugin(black_and_white::ID, 1, 1, black_and_white::main),
    plugin(sepia::ID, 1, 1, sepia::main),
    plugin(invert::ID, 1, 1, invert::main),
    plugin(gaussian_blur::ID, 1, 1, gaussian_blur::main),
    plugin(vignette::ID, 1, 1, vignette::main),
    plugin(mirror::ID, 1, 1, mirror::main),
    plugin(glow::ID, 1, 1, glow::main),
    // 2.0: VEGAS's settings (an RGB key, its thresholds' meaning).
    plugin(c"org.bananav.ChromaKeyer", 2, 0, filters::chroma_keyer),
    plugin(c"org.bananav.GLTransition", 2, 0, transitions::gl::main),
    // 2.0: VEGAS's settings (its groups, a channel and an invert each way).
    plugin(
        c"org.bananav.DisplacementMap",
        2,
        0,
        compositors::displacement_map,
    ),
    // 2.0: VEGAS's settings (its groups, Smoothness in steps).
    plugin(c"org.bananav.HeightMap", 2, 0, compositors::height_map),
    plugin(flicker_control::ID, 2, 0, flicker_control::main),
    // 2.0: VEGAS's panel, rules and math (the mesh's points where they
    // are, not offsets).
    plugin(mesh_warp::ID, 2, 0, mesh_warp::main),
    // 2.0: VEGAS's panel (the oval first, Feather in percent, Blend's
    // meaning); older settings converted.
    plugin(bezier_masking::ID, 2, 0, bezier_masking::main),
    // 2.0: VEGAS's measure (L*a*b*), panel and presets.
    plugin(color_match::ID, 2, 0, color_match::main),
    // 2.0: VEGAS's panel, rules and math (its parameters' names).
    plugin(stabilization::ID, 2, 0, stabilization::main),
    plugin(stabilize::ID, 1, 0, stabilize::main),
    // 2.0: VEGAS's panel (Smoothing a window in frames).
    plugin(stabilization_360::ID, 2, 0, stabilization_360::main),
    plugin(scene_rotation::ID, 1, 0, scene_rotation::main),
    plugin(stitching::ID, 2, 0, stitching::main),
    plugin(stereo_adjust::ID, 1, 0, stereo_adjust::main),
    plugin(smart_zoom::ID, 1, 0, smart_zoom::main),
    plugin(ai_guided::SMOOTHEN_ID, 1, 0, ai_guided::smoothen_main),
    plugin(ai_guided::SHARPEN_ID, 1, 0, ai_guided::sharpen_main),
    plugin(ai_guided::DEHAZE_ID, 1, 0, ai_guided::dehaze_main),
    plugin(optical_flow::SLOW_ID, 2, 0, optical_flow::slow_main),
    plugin(optical_flow::BLUR_ID, 2, 0, optical_flow::blur_main),
    plugin(optical_flow::WARP_ID, 2, 0, optical_flow::warp_main),
    plugin(gradient_wipe::ID, 2, 0, gradient_wipe::main),
];

/// BananaV's compositors (a track's Custom compositing mode runs them: the
/// Compositors window lists them); Bump Map is a video effect too.
pub const COMPOSITORS: [&str; 3] = [
    "org.bananav.DisplacementMap",
    "org.bananav.HeightMap",
    "org.bananav.BumpMap",
];

/// Whether `id` is one of BananaV's compositors.
pub fn is_compositor(id: &str) -> bool {
    COMPOSITORS.contains(&id)
}

/// Whether `id` is a compositor only (not a video effect as well).
pub fn compositor_only(id: &str) -> bool {
    compositors::Kind::of(id).is_some()
}

/// Whether `id` is one of BananaV's transitions.
pub fn is_transition(id: &str) -> bool {
    transitions::Kind::of(id).is_some() || Some(id) == transitions::gl::ID.to_str().ok()
}

/// A factory preset of one of BananaV's effects: its name (in English, as
/// the interface translates it) and the values it gives parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    pub values: Vec<(&'static str, PresetValue)>,
}

/// A parameter's value in a preset.
#[derive(Clone, Debug, PartialEq)]
pub enum PresetValue {
    Num(Vec<f64>),
    /// A string's (a file of BananaV's looks starts with [`LOOKS`]: see
    /// [`resolve_looks`]).
    Text(String),
    /// A parametric parameter's curves.
    Curves(Vec<Vec<CurvePoint>>),
}

pub use lut_filter::LOOKS;

/// Writes BananaV's looks (LUT Filter's presets' tables) to `dir`, each
/// that is missing or differs.
pub fn write_looks(dir: &std::path::Path) -> std::io::Result<()> {
    lut_filter::write_looks(dir)
}

/// A preset's string with BananaV's looks in `dir`: their files' paths.
pub fn resolve_looks(text: &str, dir: &std::path::Path) -> String {
    lut_filter::resolve(text, dir)
}

/// The computer's font face Titles & Text draws `family` with for `bold`
/// and `italic` (another family where it is missing): its file and its
/// place in the file's collection.
pub fn font_face(family: &str, bold: bool, italic: bool) -> Option<(std::path::PathBuf, u32)> {
    fonts::find(family, bold, italic).map(|(face, _, _)| (face.path.clone(), face.index))
}

pub use fonts::Cjk;

/// The computer's faces the interface draws characters its own font lacks
/// with (Chinese, Japanese and Korean, `first`'s first; one with many other
/// scripts): their files and places in them.
pub fn interface_fallbacks(first: Cjk) -> Vec<(std::path::PathBuf, u32)> {
    fonts::interface_fallbacks(first)
}

/// The factory presets of the effect `id` (none for others').
pub fn presets(id: &str) -> Vec<Preset> {
    if Some(id) == crop::ID.to_str().ok() {
        crop::presets()
    } else if Some(id) == color_curves::ID.to_str().ok() {
        color_curves::presets()
    } else if Some(id) == color_corrector::ID.to_str().ok() {
        color_corrector::presets()
    } else if Some(id) == cookie_cutter::ID.to_str().ok() {
        cookie_cutter::presets()
    } else if Some(id) == layer_dimensionality::ID.to_str().ok() {
        layer_dimensionality::presets()
    } else if Some(id) == pixelate::ID.to_str().ok() {
        pixelate::presets()
    } else if Some(id) == lut_filter::ID.to_str().ok() {
        lut_filter::presets()
    } else if Some(id) == white_balance::ID.to_str().ok() {
        white_balance::presets()
    } else if Some(id) == hsl_adjust::ID.to_str().ok() {
        hsl_adjust::presets()
    } else if Some(id) == picture_in_picture::ID.to_str().ok() {
        picture_in_picture::presets()
    } else if Some(id) == saturation_adjust::ID.to_str().ok() {
        saturation_adjust::presets()
    } else if Some(id) == border::ID.to_str().ok() {
        border::presets()
    } else if Some(id) == black_bar_fill::ID.to_str().ok() {
        black_bar_fill::presets()
    } else if Some(id) == unsharp_mask::ID.to_str().ok() {
        unsharp_mask::presets()
    } else if Some(id) == add_noise::ID.to_str().ok() {
        add_noise::presets()
    } else if Some(id) == timecode::ID.to_str().ok() {
        timecode::presets()
    } else if Some(id) == color_balance::ID.to_str().ok() {
        color_balance::presets()
    } else if Some(id) == brightness_contrast::ID.to_str().ok() {
        brightness_contrast::presets()
    } else if Some(id) == levels::ID.to_str().ok() {
        levels::presets()
    } else if Some(id) == black_and_white::ID.to_str().ok() {
        black_and_white::presets()
    } else if Some(id) == sepia::ID.to_str().ok() {
        sepia::presets()
    } else if Some(id) == invert::ID.to_str().ok() {
        invert::presets()
    } else if Some(id) == gradient_map::ID.to_str().ok() {
        gradient_map::presets()
    } else if Some(id) == gaussian_blur::ID.to_str().ok() {
        gaussian_blur::presets()
    } else if Some(id) == sharpen::ID.to_str().ok() {
        sharpen::presets()
    } else if Some(id) == vignette::ID.to_str().ok() {
        vignette::presets()
    } else if Some(id) == mirror::ID.to_str().ok() {
        mirror::presets()
    } else if Some(id) == glow::ID.to_str().ok() {
        glow::presets()
    } else if Some(id) == color_corrector_secondary::ID.to_str().ok() {
        color_corrector_secondary::presets()
    } else if Some(id) == titles::ID.to_str().ok() {
        titles::presets()
    } else if Some(id) == credit_roll::ID.to_str().ok() {
        credit_roll::presets()
    } else if Some(id) == gradient_wipe::ID.to_str().ok() {
        gradient_wipe::presets()
    } else if Some(id) == transitions::gl::ID.to_str().ok() {
        transitions::gl::presets()
    } else if Some(id) == flicker_control::ID.to_str().ok() {
        flicker_control::presets()
    } else if Some(id) == stabilize::ID.to_str().ok() {
        stabilize::presets()
    } else if Some(id) == bezier_masking::ID.to_str().ok() {
        bezier_masking::presets()
    } else if Some(id) == stitching::ID.to_str().ok() {
        stitching::presets()
    } else if Some(id) == color_match::ID.to_str().ok() {
        color_match::presets()
    } else if let Some(kind) = transitions::Kind::of(id) {
        transitions::presets(kind)
    } else if let Some(kind) = filters::Kind::of(id) {
        filters::presets(kind)
    } else if let Some(kind) = generators::Kind::of(id) {
        generators::presets(kind)
    } else if let Some(kind) = compositors::Kind::of(id) {
        compositors::presets(kind)
    } else {
        Vec::new()
    }
}

/// `OfxGetNumberOfPlugins` of BananaV's effects.
///
/// # Safety
/// Called by hosts, as the specification says.
#[allow(unsafe_code)]
pub unsafe extern "C" fn number_of_plugins() -> c_int {
    PLUGINS.len() as c_int
}

/// `OfxGetPlugin` of BananaV's effects.
///
/// # Safety
/// Called by hosts, as the specification says.
#[allow(unsafe_code)]
pub unsafe extern "C" fn get_plugin(nth: c_int) -> *mut OfxPlugin {
    usize::try_from(nth)
        .ok()
        .and_then(|i| PLUGINS.get(i))
        .map_or(null_mut(), |p| &p.0 as *const OfxPlugin as *mut OfxPlugin)
}
