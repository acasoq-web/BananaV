//! BananaV's own effects on the graphics card: what each needs to draw a
//! frame, worked out by the plug-ins' own code from their parameters'
//! values at that time. OpenFX gives a plug-in pictures in the processor's
//! memory only, so BananaV draws its own effects on the card itself:
//! `bv-gpu`'s kernels do the same math as the plug-ins' renders (its
//! tests compare them), from what [`plan`] gives.

use std::ffi::{CStr, c_int};
use std::sync::Arc;

use bv_core::curve::CurvePoint;

use crate::{
    add_noise, ai_guided, bezier_masking, black_and_white, black_bar_fill, border,
    brightness_contrast, color_balance, color_corrector, color_corrector_secondary, color_curves,
    color_grading, color_match, cookie_cutter, crop, flicker_control, gaussian_blur, glow,
    gradient_map, hsl_adjust, invert, layer_dimensionality, lens_correction, levels, lut_filter,
    mesh_warp, mirror, picture_in_picture, pixelate, saturation_adjust, scene_rotation, sepia,
    sharpen, smart_zoom, stabilization, stabilization_360, stabilize, stereo_adjust, stitching,
    timecode, unsharp_mask, vignette, white_balance,
};

/// A parameter's value at a time, as the host gives it to the plug-in.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Numbers: one, a point's, a color's (whole numbers for choices).
    Num(Vec<f64>),
    /// A parametric parameter's curves.
    Curves(Vec<Vec<CurvePoint>>),
    /// A string's.
    Text(String),
}

/// An instance's parameters' values at a time, by name.
#[derive(Clone, Copy, Debug)]
pub struct Values<'a>(pub &'a [(String, Value)]);

impl Values<'_> {
    fn get(&self, name: &CStr) -> Option<&Value> {
        let name = name.to_str().ok()?;
        self.0.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// A parameter's numbers.
    pub(crate) fn nums(&self, name: &CStr) -> Option<&[f64]> {
        match self.get(name)? {
            Value::Num(v) => Some(v),
            Value::Curves(_) | Value::Text(_) => None,
        }
    }

    /// A string.
    pub(crate) fn text(&self, name: &CStr) -> Option<&str> {
        match self.get(name)? {
            Value::Text(t) => Some(t),
            Value::Num(_) | Value::Curves(_) => None,
        }
    }

    pub(crate) fn f64(&self, name: &CStr, default: f64) -> f64 {
        self.nums(name)
            .and_then(|v| v.first().copied())
            .unwrap_or(default)
    }

    /// An integer or a choice.
    pub(crate) fn int(&self, name: &CStr, default: c_int) -> c_int {
        self.nums(name)
            .and_then(|v| v.first())
            .map_or(default, |v| v.round() as c_int)
    }

    /// A point's or a color's numbers.
    pub(crate) fn array<const N: usize>(&self, name: &CStr, default: [f64; N]) -> [f64; N] {
        match self.nums(name) {
            Some(v) if v.len() >= N => std::array::from_fn(|i| v[i]),
            _ => default,
        }
    }

    /// A parametric parameter's curves.
    pub(crate) fn curves(&self, name: &CStr) -> Option<&[Vec<CurvePoint>]> {
        match self.get(name)? {
            Value::Curves(c) => Some(c),
            Value::Num(_) | Value::Text(_) => None,
        }
    }
}

/// How many values a Color Curves table has, over 0–1.
pub const TABLE: usize = color_curves::TABLE;

/// When a frame is: its time as the effect's owner counts it (frames, as
/// OpenFX counts them), the project's frames a second, and its place on
/// the ruler (seconds; the ruler's offset added).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moment {
    pub frames: f64,
    pub rate: f64,
    pub ruler: f64,
    /// The effect's length in frames: its clip's frames run from 0 to it.
    pub length: f64,
}

/// A picture laid over the frame (BananaV's writing): `w` × `h` pixels,
/// premultiplied, its top left at pixel `x`, `y` of the frame (rows from
/// the top).
#[derive(Clone, Debug, PartialEq)]
pub struct Overlay {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<[f32; 4]>,
}

impl Overlay {
    /// `px` (premultiplied) with the overlay's pixel at the frame's `x`,
    /// `y` over it.
    pub fn over(&self, x: i32, y: i32, px: [f32; 4]) -> [f32; 4] {
        let (i, j) = (x - self.x, y - self.y);
        if i < 0 || j < 0 || i >= self.w as i32 || j >= self.h as i32 {
            return px;
        }
        let o = self.rgba[(j as u32 * self.w + i as u32) as usize];
        let k = 1.0 - o[3];
        [
            o[0] + px[0] * k,
            o[1] + px[1] * k,
            o[2] + px[2] * k,
            o[3] + px[3] * k,
        ]
    }
}

/// Timecode's writing: the time as text, and where and how big it and its
/// box are.
#[derive(Clone, Debug, PartialEq)]
pub struct Caption {
    pub text: String,
    settings: timecode::Settings,
}

impl Caption {
    /// The text and its box over a frame `w` × `h` pixels (square ones),
    /// drawn by the processor; `None` when none of it is in the frame.
    pub fn overlay(&self, w: u32, h: u32) -> Option<Overlay> {
        self.settings
            .overlay(&self.text, w as usize, h as usize, 1.0)
    }
}

/// One of BananaV's transitions on the card: its kernel (an entry point of
/// `bv-gpu`'s `shaders/own_transitions.wgsl`, as the plug-in's `Kind`), its
/// settings laid out as the kernels read them (`f`, `i`, `j`: each
/// kernel's comment says how) and the work around the kernel (`steps`).
#[derive(Clone, Debug, PartialEq)]
pub struct TransitionPlan {
    pub kernel: TransitionKernel,
    pub f: [f32; 24],
    pub i: [i32; 4],
    pub j: [u32; 4],
    pub steps: TransitionSteps,
}

/// What a transition's pictures go through around its kernel.
#[derive(Clone, Debug, PartialEq)]
pub enum TransitionSteps {
    /// Nothing: the kernel reads the two pictures.
    None,
    /// Flash: the kernel reads the picture lighting (the second if
    /// `second`) and, beside it, the same picture blurred as VEGAS blurs
    /// (`blur`: the filter's numbers across and down,
    /// `recursive_gauss::sections`) or as it is.
    Light {
        second: bool,
        blur: Option<[[f32; 16]; 2]>,
    },
    /// Cross Effect's pixelation: the pictures `pictures` names (1 the
    /// first, 2 the second, 3 both) averaged in `counts` cells across and
    /// down before the kernel mixes them — `table`: each cell column's
    /// first pixel and width, each cell row's (rows from the bottom), each
    /// pixel's column of cells, each row's (from the bottom).
    Cells {
        pictures: u32,
        counts: [u32; 2],
        table: Vec<[f32; 4]>,
    },
    /// Cross Effect's blur, as VEGAS blurs: the pictures `pictures` names
    /// (1 the first, 2 the second) before the kernel mixes them, or (4) the
    /// mix after it.
    Blur { pictures: u32, taps: [[f32; 16]; 2] },
    /// A table the kernel reads beside the two pictures: the frame's steps
    /// of the transitions that move the pictures (`tr_moves`,
    /// `Drawing::table`), Venetian Blinds' openness and offsets
    /// (`tr_venetian_blinds`).
    Table { table: Vec<[i32; 4]> },
    /// The 3D transitions' cards (`tr_cards`): what is under them
    /// (`under`: 0 nothing, 1 the first picture, 2 the second) put in
    /// place, then the cards drawn from the table over it.
    Cards { under: u32, table: Vec<[i32; 4]> },
}

/// BananaV's transitions, each a kernel of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionKernel {
    LinearWipe,
    ClockWipe,
    Iris,
    VenetianBlinds,
    StarWipe,
    /// Barn Door, Push, Slide, Squeeze, Split, Swap, Zoom, Spiral and
    /// Portals.
    Moves,
    GradientWipe,
    Flash,
    Dissolve,
    CrossEffect,
    /// Page Peel, Page Roll and Page Loop.
    Pages,
    /// 3D Blinds, 3D Cascade, 3D Fly In/Out and 3D Shuffle.
    Cards,
}

/// What the card draws for BananaV's own transition `id` at `progress`
/// from its parameters' values, in a frame `frame` canonical units big and
/// `size` pixels (`None`: not one of BananaV's).
pub fn transition_plan(
    id: &str,
    values: Values<'_>,
    frame: [f64; 2],
    size: [f64; 2],
    progress: f64,
) -> Option<TransitionPlan> {
    crate::transitions::plan(id, values, frame, size, progress)
}

/// Color Grading on the card: its Camera LUT, its own steps — `steps`, as
/// `shaders/own_pair.wgsl`'s `grade` reads them: the settings, a row each
/// (`entries` rows), then its curves' tables, an entry a row — and its
/// Look LUT.
#[derive(Clone, Debug, PartialEq)]
pub struct GradePlan {
    pub camera: Option<GradeLut>,
    pub steps: Vec<[f32; 4]>,
    pub entries: u32,
    pub look: Option<GradeLut>,
}

/// One of Color Grading's LUTs: a table (as LUT Filter's), or Apple Log's
/// published curve decoded to Rec. 709's light or video; mixed in by
/// `strength`.
#[derive(Clone, Debug, PartialEq)]
pub enum GradeLut {
    Cube {
        lut: Arc<bv_lut::Lut>,
        tetrahedral: bool,
        strength: f32,
    },
    AppleLog {
        video: bool,
        strength: f32,
    },
}

/// What the card draws for one of BananaV's effects.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    /// Color Grading.
    Grade(Box<GradePlan>),
    /// Crop: the picture times how much of each pixel the window covers,
    /// times the opacity. The window's sides are from its pivot, unturned
    /// (`lo` to `hi`), `turn` its angle's cosine and sine (canonical units,
    /// y up).
    Crop {
        pivot: [f32; 2],
        lo: [f32; 2],
        hi: [f32; 2],
        turn: [f32; 2],
        opacity: f32,
    },
    /// Color Curves: each straight channel through its table ([`TABLE`]
    /// values over 0–1, straight lines between them).
    Curves([Vec<f32>; 3]),
    /// Color Corrector: what each wheel adds to U and V where it has all
    /// the weight, then the saturation, Gamma's power (in BT.709's
    /// encoding of the luma: `color_corrector::gamma_curve`), gain and
    /// offset; unclipped.
    Corrector {
        shifts: [[f32; 2]; 3],
        saturation: f32,
        power: f32,
        gain: f32,
        offset: f32,
    },
    Cookie(Cookie),
    Layer(Layer),
    /// Pixelate: how many cells the frame holds across and down (as many
    /// as its pixels at full size: none that way), whole cells from edge
    /// to edge, each pixel in the cell its middle is in, each cell the
    /// average of its pixels.
    Pixelate {
        cells: [u32; 2],
    },
    /// LUT Filter: the straight colors through `lut` (none: its file could
    /// not be read, nothing changes), tetrahedrally or trilinearly, mixed
    /// with the picture by `strength`.
    Lut {
        lut: Option<Arc<bv_lut::Lut>>,
        tetrahedral: bool,
        strength: f32,
    },
    /// Picture In Picture: each pixel of a new picture the frame's sampled
    /// where `inverse` (rows) maps it, both 0–1 across and up: `samples` ×
    /// `samples` of them averaged, each from the four nearest pixels,
    /// times `opacity`.
    Warp {
        inverse: [f32; 9],
        samples: u32,
        opacity: f32,
    },
    /// HSL Adjust: each straight color's hue turned by `hue` (a whole turn
    /// is 1), its saturation and lightness scaled.
    Hsl {
        hue: f32,
        saturation: f32,
        luminance: f32,
    },
    /// Saturation Adjust: each straight color scaled about its BT.601
    /// luma by 1 + the adjustment for its saturation (its distance from
    /// that gray, a part of pure green's): `amount` at `center`, fading
    /// smoothly into `low` below and `high` above `spread` ⁄ 2 away; the
    /// result unclipped.
    Saturation {
        amount: f32,
        center: f32,
        spread: f32,
        low: f32,
        high: f32,
    },
    /// Color Corrector (Secondary): in BT.601 Y′UV, the mask — luminance's
    /// and saturation's limits (`limits` 0, 1: low, high, smooth, 0–1;
    /// smoothsteps of how far beyond) and the hue's (2: center, half the
    /// width, smooth, degrees; straight), those `flags` turns on (bits
    /// 0–2), their product squared, inverted by bit 3 — mixes in U and V
    /// turned by `turn` (cosine, sine), moved by `shift` and scaled by
    /// `saturation`, the luma through Gamma's curve at `power` times
    /// `gain` plus `offset`;
    /// unclipped, the alpha scaled toward `alpha`; bit 4 shows the mask.
    Secondary {
        limits: [[f32; 3]; 3],
        flags: u32,
        turn: [f32; 2],
        shift: [f32; 2],
        saturation: f32,
        power: f32,
        gain: f32,
        offset: f32,
        alpha: f32,
    },
    /// Color Balance: each straight color moved by `shifts` (red, green,
    /// blue) times the weight of its intensity, the channels' mean, for
    /// `range` (0 Highlights, 1 Midtones, 2 Shadow); unclipped.
    Balance {
        shifts: [f32; 3],
        range: u32,
    },
    /// Border on a frame `frame` big (canonical units): Blurred (`kind` 0:
    /// the picture times a rectangle `width` in, blurred by `sigma`),
    /// Beveled (1: the facets `depth` deep across and up, times `shades`
    /// — top, right, bottom, left —, clipped) or Solid (2: `color`,
    /// straight with its alpha, over the band `width` wide).
    Border {
        kind: u32,
        frame: [f32; 2],
        width: f32,
        sigma: f32,
        depth: [f32; 2],
        shades: [[f32; 3]; 4],
        color: [f32; 4],
    },
    /// Black Bar Fill: two layers of the picture, each where its `inverse`
    /// (rows; the frame and the picture 0–1 across and up) maps the frame
    /// back to the picture, `samples` × `samples` a pixel: the background
    /// blurred by VEGAS's Gaussian, `background_blur` of the frame's
    /// longer side ([`fill_background_sections`]; its edges held), the
    /// foreground over it, with the shadow of its alpha blurred by
    /// `shadow_sigma`, black at `shadow` (0: none).
    Fill {
        background: [f32; 9],
        background_samples: u32,
        background_blur: f64,
        foreground: [f32; 9],
        foreground_samples: u32,
        shadow_sigma: f64,
        shadow: f32,
    },
    /// White Balance: the straight colors through `matrix` (rows) plus
    /// `offset`, negatives clipped; with `equalize`, clipped to 0–1, each
    /// channel stretched over the picture's own range and red and green
    /// held under blue.
    Affine {
        matrix: [[f32; 3]; 3],
        offset: [f32; 3],
        equalize: bool,
    },
    /// Unsharp Mask: the picture blurred by VEGAS's Gaussian for Radius
    /// `radius` ([`unsharp_sections`]; none: nothing changes), then each
    /// channel, alpha too, moved away from the blurred one by `amount` ×
    /// their difference, unless the result is within `threshold` of the
    /// blurred one; unclipped.
    Unsharp {
        radius: f64,
        amount: f32,
        threshold: f32,
    },
    /// Add Noise: each straight color, clipped to 0–1, plus `width` × the
    /// noise of its pixel (from the frame's lower left, at render scale)
    /// for `seed` — one for the three channels when `monochromatic`,
    /// Gaussian or even —, folded back into 0–1.
    Noise {
        width: f32,
        monochromatic: bool,
        gaussian: bool,
        seed: u32,
    },
    /// Timecode: the caption's [`Overlay`], drawn by the processor at the
    /// picture's size, over the picture.
    Timecode(Caption),
    /// One of the table effects that change each pixel where it is: its
    /// kernel and the numbers it takes (`f` in its slots, `j` its
    /// switches).
    Pixel {
        kernel: PixelKernel,
        f: Vec<f32>,
        j: [u32; 4],
    },
    /// Mesh Warp: the mesh's points (`cells` and one each way, rows up,
    /// parts of the picture) and whether they are where the grid at rest
    /// goes (Target) or where it reads (Source), laid on the picture at its
    /// size (`mesh_warp_table`).
    MeshWarp {
        points: Arc<Vec<[f32; 2]>>,
        cells: [u32; 2],
        target: bool,
    },
    /// Lens Correction: k1, k2, k3, 1 / Scale and Distortion Center
    /// (across, up) and its model (`ptlens`, else the radial one), laid on
    /// the picture at its size (`lens_correction_numbers`).
    LensCorrection {
        settings: [f32; 6],
        ptlens: bool,
    },
    /// Video Stabilization: where each pixel reads the picture, as parts
    /// of the frame (rows up: `u′ = m0 u + m1 v + m2`, `v′ = m3 u + m4 v +
    /// m5`, laid on the picture at its size by `stabilize_numbers`), the
    /// edge filling (transparent, mirrored, repeated, held) and the motion
    /// blur's reach each way as a part of the frame's width.
    Stabilize {
        map: [f64; 6],
        fill: u32,
        reach: f32,
    },
    /// The 360° effects: each pixel's direction on the sphere turned by
    /// the matrix (by rows; x across, y up, z ahead), each eye apart as
    /// `stereo` says (1 top and bottom, 2 side by side).
    SphereTurn {
        matrix: [f32; 9],
        stereo: u32,
    },
    /// Dual Fish Eye Stitching: the field of view, how far along the
    /// lenses' axis both see, the vignetting's intensity and range, Center
    /// and Radius, each lens's turn's cosine and sine (the back's, the
    /// front's); each column's and row's direction from `stitch_table`.
    Stitch {
        numbers: [f32; 10],
    },
    /// Stereoscopic 3D Adjust: Horizontal offset, Vertical offset, Zoom,
    /// Keystone, Rotation, Crop, Automatic crop (1 on) and the floating
    /// windows' widths, and the flips; the eye's reads from `stereo_card`.
    Stereo {
        numbers: [f32; 9],
        flips: [u32; 2],
    },
    /// Smart Zoom: the window's middle (0–1, rows up) and zoom, how
    /// strongly the detail is lifted and the noise it stands out of; the
    /// Lanczos taps from `smart_zoom_table`.
    SmartZoom {
        numbers: [f32; 5],
    },
    /// AI Smoothen and AI Sharpen: Border, Gain, Offset and Amount; the
    /// small copy's columns and rows and what is worked out on it (1
    /// Sharpen); the margin from `guided_layout`.
    Guided {
        numbers: [f32; 4],
        low: [u32; 3],
    },
    /// One of the table effects that draw each pixel from somewhere else
    /// in the picture (warps): its kernel and its numbers.
    Remap {
        kernel: RemapKernel,
        f: Vec<f32>,
        j: [u32; 4],
    },
    /// Chroma Keyer: the key's numbers for `kit_key_mask` and
    /// `kit_key_apply` (`f`, `j`), and Blur amount.
    ChromaKey {
        f: Vec<f32>,
        j: [u32; 4],
        blur: f64,
    },
    /// Linear Blur's Amount and Angle (degrees): the card shears the
    /// picture as [`line_shear`] says, blurs its rows and shears it back.
    LinearBlur {
        amount: f64,
        angle: f64,
    },
    /// Median and Min and Max: the ranges across and up, Circular area
    /// processing and the rank (0 the lowest, 1 the highest).
    Ranked {
        range: [f64; 2],
        round: bool,
        rank: f64,
    },
    /// Radial Blur: Strength, Proportional (else Fixed) and Center (the
    /// frame's u, v from the top).
    RadialBlur {
        strength: f64,
        proportional: bool,
        center: [f64; 2],
    },
    /// Radial Pixelate: its middle (-1 to 1 of the frame, y up), Strength
    /// and whether it pixelates by angle (else by distance).
    RadialPixelate {
        center: [f64; 2],
        strength: f64,
        radial: bool,
    },
    /// Chroma Blur: its blur's pixels across and up (at render scale 1).
    ChromaBlur {
        pixels: [f64; 2],
    },
    /// Gaussian Blur: its ranges across and up ([`gaussian_sections`])
    /// and the channels it blurs (red, green, blue, alpha).
    Gaussian {
        ranges: [f64; 2],
        channels: [bool; 4],
    },
    /// Sharpen: how far each channel moves from its neighbours' average.
    Sharpen {
        gain: f32,
    },
    /// Glow: its blur's radius (a part of the frame's longer side),
    /// Intensity, Suppression and Color.
    Glow {
        reach: f64,
        intensity: f32,
        suppression: f32,
        color: [f32; 3],
    },
    /// Vignette: `kit_vignette`'s numbers (the render scale after them)
    /// and Invert.
    Vignette {
        numbers: Vec<f32>,
        invert: bool,
    },
    /// A frame of the effect's input at another time shown as it is
    /// (Slow Motion on a whole source frame).
    Frame {
        picture: Arc<bv_core::video::ImageRgbaF32>,
    },
    /// Denoise (`looks::denoise`): the picture's BT.601 planes in 8-bit
    /// steps (`looks::yuv_of`), reflected past its edges by
    /// `denoise_reach`; Y cleaned by non-local means at the first strength,
    /// U and V at the second (0: left), back to RGB (`looks::rgb_of`);
    /// then Sharpening's kernel with this center (0: none).
    Denoise {
        strengths: [f32; 2],
        center: i32,
    },
    /// Flicker Control's Addition (`flicker_control::add_pixel`): the light
    /// added, Type, and Show mask only.
    FlickerAdd {
        amount: f32,
        tones: i32,
        mask: bool,
    },
    /// Flicker Control's Histogram smoothing (`flicker_control::match_pixel`):
    /// each channel's levels' matches, a channel after another (`None`:
    /// nothing counted, the picture as it is), the levels Type maps, and
    /// Show mask only.
    FlickerMatch {
        table: Option<Arc<Vec<f32>>>,
        range: [i32; 2],
        mask: bool,
    },
    /// Flicker Control's Temporal blend (`flicker_control::blend`): the
    /// window's frames the clip has, the one that keeps its alpha, Type,
    /// and Show mask only.
    FlickerBlend {
        frames: Vec<FlickerFrame>,
        center: usize,
        tones: i32,
        mask: bool,
    },
    /// Slow Motion's frame between two of its input, `first` and `second`
    /// ([`FlowBetween`]).
    FlowBetween {
        first: Arc<bv_core::video::ImageRgbaF32>,
        second: Arc<bv_core::video::ImageRgbaF32>,
        between: FlowBetween,
    },
    /// Motion Blur: the moving areas' mask (the picture's pixels, rows
    /// from the top: 255 where they move) and the kernel (its side and
    /// weights, rows up), from the frame and its neighbour
    /// ([`temporal_plan`]): the picture convolved with it, its edges
    /// reflected, where the mask convolved with it reaches.
    MotionBlur {
        mask: Arc<Vec<[f32; 4]>>,
        kernel: Arc<Vec<f32>>,
        size: u32,
    },
    /// Mirror: its settings ([`mirror_numbers`]) and what shows past the
    /// picture (premultiplied, no alpha).
    Mirror {
        angle: f64,
        center: [f64; 2],
        beyond: [f32; 4],
    },
    /// One of the generators: its kernel and numbers (`f` from the frame's
    /// size in the picture's pixels on).
    Generate {
        kernel: GenerateKernel,
        f: Vec<f32>,
        j: [u32; 4],
    },
    /// Fill Light: each pixel's light from `table` by its 8-bit luma
    /// (`looks::fill_index`), blurred by VEGAS's Gaussian reaching `reach`
    /// of the frame's longer side ([`fill_light_sections`]; none: 0),
    /// `amount` of it added.
    FillLight {
        table: Vec<[f32; 4]>,
        reach: f64,
        amount: f32,
    },
    /// Film Grain: Amount, Granularity and Chromaticity, the frame its
    /// random numbers start from and the frame's size in canonical units
    /// (the pixels' shape); its tables worked out at the picture's size
    /// (`film_grain_table`).
    FilmGrain {
        numbers: [f64; 3],
        frame: f64,
        canonical: [f64; 2],
    },
    /// Gradient Map: its gradient's table (`gradient_map::Gradient::table`),
    /// Amount and whether it adds.
    GradientMap {
        table: Vec<[f32; 4]>,
        amount: f32,
        add: bool,
    },
    /// Glint: its settings; the streaks' kernel worked out at the
    /// picture's size (`glint_card`).
    Glint {
        numbers: crate::looks::Glint,
    },
    /// Rays: the lights (`kit_rays_lights`), then the rays from them over
    /// the picture (`kit_rays`): `looks::Beams`' numbers and flags.
    Rays {
        f: Vec<f32>,
        j: [u32; 4],
    },
    /// Soft Contrast: the tone's numbers (`kit_soft_tone`), Diffusion and
    /// its glow's radius, and the vignette's numbers (`kit_soft_vignette`)
    /// with Blur's radii across and down — radii parts of the picture's
    /// width, blurred as VEGAS blurs (`recursive_gauss`).
    SoftContrast {
        tone: Vec<f32>,
        diffusion: Option<(f32, f64)>,
        vignette: Option<(Vec<f32>, Option<[f64; 2]>)>,
    },
    /// Starburst: its settings (`looks::starburst_numbers`) and the
    /// frame's size in canonical units (the pixels' shape); the star and
    /// the lights' steps worked out at the picture's size
    /// (`starburst_card`).
    Starburst {
        numbers: [f64; 11],
        canonical: [f64; 2],
    },
    /// Defocus: its settings (`filters::defocus_numbers`) and the frame's
    /// size in canonical units (the pixels' shape); the iris worked out at
    /// the picture's size (`defocus_card`).
    Defocus {
        numbers: [f64; 7],
        canonical: [f64; 2],
    },
    /// Lens Flare: its settings (`looks::flare_numbers`) and the frame's
    /// size in canonical units (the pixels' shape); its elements worked
    /// out at the picture's size (`lens_flare_card`).
    LensFlare {
        numbers: [f64; 11],
        canonical: [f64; 2],
    },
    /// Newsprint: Dot size, Type and the frame's size in canonical units
    /// (the pixels' shape); its numbers worked out at the picture's size
    /// (`newsprint_card`).
    Newsprint {
        dot: f64,
        kind: i32,
        canonical: [f64; 2],
    },
    /// TV Simulator: its settings (`looks::tv_numbers`), the frame's time
    /// and its size in canonical units (the pixels' shape); its numbers
    /// worked out at the picture's size (`tv_simulator_card`).
    TvSimulator {
        numbers: [f64; 8],
        time: f64,
        canonical: [f64; 2],
    },
    /// Film Effects: its settings (`looks::film_numbers`) and the frame's
    /// time; its marks and numbers worked out at the picture's size
    /// (`film_effects_card`).
    FilmEffects {
        numbers: [f64; 8],
        time: f64,
    },
    /// Light Rays: its settings (`looks::light_rays_numbers`) and the
    /// frame's time; its numbers worked out at the picture's size
    /// (`light_rays_card`).
    LightRays {
        numbers: [f64; 13],
        time: f64,
    },
    /// Wave: its numbers in VEGAS's order (`filters::wave_numbers`), the
    /// moves per row and column worked out at the picture's size
    /// (`wave_offsets`).
    Wave {
        numbers: [f64; 6],
    },
    /// Swirl (`swirl`) or Pinch/Punch: Center, Horizontal, Vertical and
    /// Amount (`filters::lens_settings`), the area laid on the picture at
    /// its size (`lens`).
    Lens {
        swirl: bool,
        settings: [f64; 5],
    },
    /// Deform: Amount, Center image, the background and the shears,
    /// squeezes and curves (`filters::deform_settings`), laid on the
    /// picture at its size (`deform`).
    Deform {
        settings: [f64; 16],
    },
    /// The picture as it is (the effect's settings change nothing).
    Same,
    /// Bézier Masking: the masks that show (`any`: whether any is on at
    /// all), Invert, Mask FX — the effects before it over the chain's
    /// input — and Blend.
    Masks {
        shapes: Vec<MaskShape>,
        any: bool,
        invert: bool,
        mask_fx: bool,
        blend: f32,
    },
}

/// The kernels of [`Plan::Generate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerateKernel {
    Checkerboard,
    TestPattern,
    NoiseTexture,
    /// Solid Color: the card's fill with f's color.
    SolidColor,
}

/// What the card draws for generator `id` with `values`: a picture
/// `size` pixels big of a frame `full` pixels big (its pixels square), as
/// the plug-in draws it at that render scale; `None` for a generator the
/// processor draws.
pub fn generator_plan(
    id: &str,
    values: Values<'_>,
    full: (u32, u32),
    size: (u32, u32),
) -> Option<Plan> {
    let kind = crate::generators::Kind::of(id)?;
    let vals = crate::kit::Vals::of(kind.specs(), &|spec| {
        values
            .nums(spec.name())
            .map_or_else(|| spec.default(), <[f64]>::to_vec)
    });
    let k = f64::from(size.0) / f64::from(full.0.max(1));
    let grid = crate::kit::Grid {
        origin: [0.0, 0.0],
        frame: [f64::from(full.0), f64::from(full.1)],
        scale: [k, k],
        par: 1.0,
    };
    kind.card_plan(&vals, &grid)
}

/// A compositor on the card (a track's Custom compositing mode): what
/// `kernel` reads (`f`, `j`: Bump Map's light; Displacement Map's scales,
/// negative when inverted, its channels, Alpha channel scales RGB
/// channels and Wrap pixels around; Height Map's Amplitude, Elevation and
/// Height scale, its channel, Invert data source and the same two),
/// whether the picture and the map swap (`flip`) and Height Map's
/// Smoothness.
#[derive(Clone, Debug, PartialEq)]
pub struct CompositorPlan {
    pub kernel: CompositorKernel,
    pub f: Vec<f32>,
    pub j: [u32; 4],
    pub flip: bool,
    pub smoothness: i32,
}

/// Height Map's numbers for a picture of a size (`height_numbers`): its
/// smoothing box's radius in pixels, the bits of its heights' fixed point
/// below the 765ths of the full height, the brightest parts' height in
/// pixels and a fixed-point unit's, Height scale and Elevation's depth in
/// pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeightNumbers {
    pub radius: u32,
    pub bits: u32,
    pub height: f32,
    pub unit: f32,
    pub scale: f32,
    pub depth: f32,
}

/// Height Map's numbers for `plan` on a picture `w` × `h`, the
/// processor's; `None` where it draws nothing.
pub fn height_numbers(plan: &CompositorPlan, w: u32, h: u32) -> Option<HeightNumbers> {
    crate::compositors::height_numbers(plan, w, h)
}

/// Film Grain's tables for a picture `w` × `h` (`Plan::FilmGrain`): the
/// three grain tables (64 × 64 values each), then each column's and each
/// row's two table places and how far between them (as the processor's
/// `looks::Grain`); and Amount and Chromaticity.
pub fn film_grain_table(
    numbers: [f64; 3],
    frame: f64,
    canonical: [f64; 2],
    w: u32,
    h: u32,
) -> (Vec<[f32; 4]>, [f32; 2]) {
    let par = if w > 0 && h > 0 && canonical[1] > 0.0 {
        (canonical[0] / f64::from(w)) / (canonical[1] / f64::from(h))
    } else {
        1.0
    };
    let g = crate::looks::Grain::new(numbers, w as usize, h as usize, par, frame);
    let mut t: Vec<[f32; 4]> = g.table.iter().map(|&v| [v as f32, 0.0, 0.0, 0.0]).collect();
    let place = |&(a, b, f): &(usize, usize, f64)| [a as f32, b as f32, f as f32, 0.0];
    t.extend(g.columns.iter().map(place));
    t.extend(g.rows.iter().map(place));
    (t, [g.amount, g.chroma as f32])
}

/// The frequency domain's grid for a picture `w` × `h` convolved with a
/// kernel reaching `reach` pixels each way (the processor's).
pub fn fft_grid(w: usize, h: usize, reach: usize) -> (usize, usize) {
    crate::fft::grid_size(w, h, reach)
}

/// The card's numbers for VEGAS's Gaussian of radius `radius` pixels
/// (`recursive_gauss::sections`).
pub fn blur_sections(radius: f64) -> [f32; 16] {
    crate::recursive_gauss::sections(crate::recursive_gauss::sigma(radius))
}

/// Vignette's pixels' scale on the card at a picture `w` × `h` of a frame
/// `canonical` big (`vignette::pixel_scale`).
pub fn vignette_scale(canonical: [f64; 2], w: u32, h: u32) -> [f32; 2] {
    vignette::pixel_scale(pixel_shape(canonical, w, h)).map(|v| v as f32)
}

/// A frame Flicker Control's Temporal blend takes: its picture (`None`:
/// the frame itself, the picture the card has), its weight and the
/// homography taking the frame's pixels to it (rows up), with motion
/// compensation.
#[derive(Clone, Debug, PartialEq)]
pub struct FlickerFrame {
    pub picture: Option<Arc<bv_core::video::ImageRgbaF32>>,
    pub weight: f32,
    pub map: Option<[f32; 9]>,
}

/// How far Denoise's planes are reflected past the picture: its search's
/// and its patches' reach.
pub fn denoise_reach() -> u32 {
    crate::looks::REACH as u32
}

/// Whether effect `id`'s plan with `values` reads other frames
/// ([`temporal_plan`]).
pub fn reads_other_frames(id: &str, values: Values<'_>) -> bool {
    id == BLUR_ID
        || id == SLOW_ID
        || (flicker_control::ID.to_str().ok() == Some(id) && {
            let s = flicker_control::Settings::of_values(values);
            !s.addition && !s.histogram
        })
}

/// GL Transition's identifier.
pub const GL_TRANSITION_ID: &str = "org.bananav.GLTransition";

/// GL Transition's kernel on the card with `values` at `progress` over
/// pictures `w` × `h`: its WGSL (after WGSL's prelude; entry point
/// `glt_main`, the bindings of the transitions' kernels) and its numbers
/// (`P.f`); `None` when its code cannot be one (the processor draws it).
pub fn gl_transition_card(
    values: Values<'_>,
    progress: f64,
    w: u32,
    h: u32,
) -> Option<(Arc<String>, [f32; 24])> {
    crate::transitions::gl::card(values, progress, w, h)
}

/// Motion Blur's, Slow Motion's and Warp Flow's identifiers.
const BLUR_ID: &str = "org.bananav.MotionBlur";
const SLOW_ID: &str = "org.bananav.SlowMotion";
pub const WARP_FLOW_ID: &str = "org.bananav.WarpFlow";

/// A frame made between two pictures along the motion back from the
/// second to the first (`optical_flow`'s `made_between`): the flow's
/// cells across and up, each cell's move as the bits of two floats (rows
/// up), how many of the pictures' pixels a cell is, how far between the
/// frame is, and Nearest Frame.
#[derive(Clone, Debug, PartialEq)]
pub struct FlowBetween {
    pub cells: [u32; 2],
    pub table: Arc<Vec<[i32; 4]>>,
    pub k: f32,
    pub t: f32,
    pub nearest: bool,
}

/// A key for a pair of frames the flow is worked out between.
fn flow_key(what: &str, a: f64, b: f64, quality: c_int, bounds: [c_int; 4]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (what, a.to_bits(), b.to_bits(), quality, bounds).hash(&mut h);
    h.finish()
}

/// Warp Flow on the card: the flow between `from` and `to` (as the card
/// has them) for `progress` of the way.
pub fn warp_flow_card(
    values: Values<'_>,
    from: &bv_core::video::ImageRgbaF32,
    to: &bv_core::video::ImageRgbaF32,
    progress: f64,
) -> FlowBetween {
    use crate::kit::Buf;
    let (a, b) = (Buf::of_image(from), Buf::of_image(to));
    let key = flow_key("warp", progress, 0.0, 1, a.bounds);
    let (cells, table, k) = crate::optical_flow::flow_table(&a, &b, 1, key);
    FlowBetween {
        cells,
        table: Arc::new(table),
        k,
        t: progress.clamp(0.0, 1.0) as f32,
        nearest: crate::optical_flow::warp_nearest_of(values),
    }
}

/// The plan of an effect that reads other frames, `current` its input at
/// the frame (`at`), `input` giving it at another time (frames, as its
/// plug-in counts them) and `number` a parameter's value then: Motion
/// Blur's mask and kernel from the frame and the next (the last frame: the
/// one before and it), Slow Motion's source frames and the flow between
/// them, as the plug-ins work them out; `None` when the processor draws it
/// (their visualizations).
pub fn temporal_plan(
    id: &str,
    values: Values<'_>,
    at: Moment,
    current: &bv_core::video::ImageRgbaF32,
    input: &dyn Fn(f64) -> Option<Arc<bv_core::video::ImageRgbaF32>>,
    numbers: &dyn Fn(&str, f64) -> Option<Vec<f64>>,
) -> Option<Plan> {
    if id == SLOW_ID {
        return slow_motion_plan(values, at, input, numbers);
    }
    if flicker_control::ID.to_str().ok() == Some(id) {
        return flicker_blend_plan(values, at, current, input, numbers);
    }
    if id != BLUR_ID {
        return None;
    }
    let (s, quality, show) = crate::optical_flow::blur_settings_of(values);
    if show != 0 {
        return None;
    }
    let now = at.frames;
    use crate::kit::Buf;
    let cur = Buf::of_image(current);
    let same = |b: &Buf| b.bounds == cur.bounds;
    let (first, second, a, b) = match input(now + 1.0).map(|i| Buf::of_image(&i)).filter(same) {
        Some(next) => (cur.clone(), next, now, now + 1.0),
        None => {
            let prev = input(now - 1.0).map(|i| Buf::of_image(&i)).filter(same)?;
            (prev, cur.clone(), now - 1.0, now)
        }
    };
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        ("card", a.to_bits(), b.to_bits(), quality, cur.bounds).hash(&mut h);
        h.finish()
    };
    let motion = crate::optical_flow::blur_motion(&first, &second, quality, key);
    let (w, h) = (cur.width().max(0) as usize, cur.height().max(0) as usize);
    let Some(blur) = s.blur(w, h, &motion) else {
        return Some(Plan::Same);
    };
    // The mask with its rows from the top, as the card's pictures.
    let mut mask = Vec::with_capacity(w * h);
    for row in blur.mask.chunks(w.max(1)).rev() {
        mask.extend(
            row.iter()
                .map(|&m| if m { [255.0f32; 4] } else { [0.0; 4] }),
        );
    }
    Some(Plan::MotionBlur {
        mask: Arc::new(mask),
        kernel: Arc::new(blur.kernel),
        size: blur.size as u32,
    })
}

/// Flicker Control's Temporal blend's plan (see [`temporal_plan`]): the
/// window's frames the clip has, as the plug-in takes them.
fn flicker_blend_plan(
    values: Values<'_>,
    at: Moment,
    current: &bv_core::video::ImageRgbaF32,
    input: &dyn Fn(f64) -> Option<Arc<bv_core::video::ImageRgbaF32>>,
    numbers: &dyn Fn(&str, f64) -> Option<Vec<f64>>,
) -> Option<Plan> {
    let s = flicker_control::Settings::of_values(values);
    let (w, center) = flicker_control::weights(s.window, s.sensitivity);
    let start = at.frames - f64::from(s.before());
    let same =
        |i: &bv_core::video::ImageRgbaF32| i.width == current.width && i.height == current.height;
    let mut frames = Vec::new();
    let mut times = Vec::new();
    for (k, &weight) in w.iter().enumerate() {
        let t = start + k as f64;
        if !flicker_control::has_frame([0.0, at.length], t) {
            continue;
        }
        let picture = if t == at.frames {
            None
        } else {
            match input(t).filter(|i| same(i)) {
                Some(p) => Some(p),
                None => continue,
            }
        };
        frames.push(FlickerFrame {
            picture,
            weight,
            map: None,
        });
        times.push(t);
    }
    if s.motion {
        let size = (current.width as c_int, current.height as c_int);
        let corner = |name: &CStr, t: f64| {
            let v = numbers(name.to_str().ok()?, t)?;
            Some([*v.first()?, *v.get(1)?])
        };
        let corners: Option<Vec<_>> = times
            .iter()
            .map(|&t| {
                let parts = flicker_control::CORNERS.map(|c| corner(c, t));
                parts.iter().all(Option::is_some).then(|| {
                    flicker_control::corners_in(parts.map(|p| p.unwrap_or_default()), size)
                })
            })
            .collect();
        let maps = flicker_control::maps(&corners?, center);
        for (f, m) in frames.iter_mut().zip(maps) {
            f.map = m;
        }
    }
    Some(Plan::FlickerBlend {
        frames,
        center,
        tones: s.tones,
        mask: s.mask,
    })
}

/// Slow Motion's plan (see [`temporal_plan`]).
fn slow_motion_plan(
    values: Values<'_>,
    at: Moment,
    input: &dyn Fn(f64) -> Option<Arc<bv_core::video::ImageRgbaF32>>,
    numbers: &dyn Fn(&str, f64) -> Option<Vec<f64>>,
) -> Option<Plan> {
    let number = |name: &str, t: f64| numbers(name, t).and_then(|v| v.first().copied());
    use crate::kit::Buf;
    let (quality, nearest, show) = crate::optical_flow::slow_settings_of(values);
    if show {
        return None;
    }
    let rate = |k: f64| number(crate::optical_flow::RATE_NAME, k).unwrap_or(1.0);
    let s = crate::optical_flow::summed_rate(&rate, at.frames);
    let (s0, frac) = (s.floor(), s - s.floor());
    let first = input(s0)?;
    if frac < 1e-9 {
        return Some(Plan::Frame { picture: first });
    }
    let second = input(s0 + 1.0)?;
    let (a, b) = (Buf::of_image(&first), Buf::of_image(&second));
    if a.bounds != b.bounds {
        return Some(Plan::Frame { picture: first });
    }
    let key = flow_key("slow", s0 + 1.0, s0, quality, a.bounds);
    let (cells, table, k) = crate::optical_flow::flow_table(&a, &b, quality, key);
    Some(Plan::FlowBetween {
        first,
        second,
        between: FlowBetween {
            cells,
            table: Arc::new(table),
            k,
            t: frac as f32,
            nearest,
        },
    })
}

/// Pixelate's step between `n` cells over `size` pixels, in 16,384ths of
/// a pixel (`pixelate::step`).
pub fn pixelate_step(n: u32, size: u32) -> i32 {
    pixelate::step(n, size) as i32
}

/// Mirror's numbers on the card at a picture `w` × `h` of a frame
/// `canonical` big (`mirror::Layout::numbers`).
pub fn mirror_numbers(
    angle: f64,
    center: [f64; 2],
    canonical: [f64; 2],
    w: u32,
    h: u32,
) -> ([f32; 5], [u32; 2]) {
    let s = mirror::Settings {
        angle,
        center,
        background: [0.0; 4],
    };
    let scale = f64::from(w) / canonical[0].max(1.0);
    s.layout(w, h, [scale; 2], pixel_shape(canonical, w, h))
        .numbers()
}

/// How many times as wide as tall the pixels of a picture `w` × `h` of a
/// frame `canonical` big show.
pub fn pixel_shape(canonical: [f64; 2], w: u32, h: u32) -> f64 {
    if w > 0 && h > 0 && canonical[1] > 0.0 {
        (canonical[0] / f64::from(w)) / (canonical[1] / f64::from(h))
    } else {
        1.0
    }
}

/// Glint on the card at a picture `w` × `h`: the lights' numbers and
/// flags (`kit_glint_lights`), the streaks' kernel (its side and cells,
/// rows up; none without reach), Effect only and the mask's mode and
/// numbers.
#[derive(Clone, Debug)]
pub struct GlintCard {
    pub lights: Vec<f32>,
    pub flags: [u32; 4],
    pub kernel: Option<(u32, Arc<Vec<[f32; 4]>>)>,
    pub alone: bool,
    pub mask: (u32, [f32; 13]),
}

/// Glint on the card at a picture `w` × `h` (none: as it is).
pub fn glint_card(g: &crate::looks::Glint, w: u32, h: u32) -> Option<GlintCard> {
    let kernel = g
        .kernel(f64::from(w), f64::from(h))
        .map(|(size, cells)| (size as u32, Arc::new(cells)));
    if kernel.is_none() && g.mask.mode != 3 {
        return None;
    }
    Some(GlintCard {
        lights: g.light_numbers(),
        flags: [0, u32::from(g.smooth), 0, g.mask.mode],
        kernel,
        alone: g.effect_only,
        mask: (g.mask.mode, g.mask.numbers),
    })
}

/// What the card does for Starburst at a picture `w` × `h` (none: the
/// picture as it is): the star's weights (rows up) and side, the lights'
/// bloom, threshold and soft edges, whether they are smoothed first,
/// Strength and Effect only.
#[derive(Clone, Debug)]
pub struct StarburstCard {
    pub star: Arc<Vec<f32>>,
    pub size: u32,
    pub bloom: Option<[f32; 2]>,
    pub threshold: f32,
    pub edges: Option<[f32; 5]>,
    pub smooth: bool,
    pub strength: f32,
    pub alone: bool,
}

/// Starburst on the card at a picture `w` × `h` (none: as it is).
pub fn starburst_card(
    numbers: [f64; 11],
    canonical: [f64; 2],
    w: u32,
    h: u32,
) -> Option<StarburstCard> {
    let st = crate::looks::Starburst::of(
        numbers,
        f64::from(w),
        f64::from(h),
        pixel_shape(canonical, w, h),
    );
    let (size, star) = st.star().filter(|_| st.shows())?;
    Some(StarburstCard {
        star: Arc::new(star),
        size: size as u32,
        bloom: st.bloom(),
        threshold: st.threshold(),
        edges: st.edges(),
        smooth: st.smooth,
        strength: st.strength as f32,
        alone: st.effect_only,
    })
}

/// What the card does for Defocus at a picture `w` × `h` (none: the
/// picture as it is): the iris's weights (rows up) and side, the bloom's
/// threshold and slope, and whether the picture is smoothed first — the
/// smoothing alone through a one-pixel iris, as the processor's.
#[allow(clippy::type_complexity)]
pub fn defocus_card(
    numbers: [f64; 7],
    canonical: [f64; 2],
    w: u32,
    h: u32,
) -> Option<(Arc<Vec<f32>>, u32, Option<[f32; 2]>, bool)> {
    let par = if w > 0 && h > 0 && canonical[1] > 0.0 {
        (canonical[0] / f64::from(w)) / (canonical[1] / f64::from(h))
    } else {
        1.0
    };
    let d = crate::filters::Defocus::of(numbers, f64::from(w), f64::from(h), par);
    match d.iris() {
        Some((size, kernel)) => Some((Arc::new(kernel), size as u32, d.bloom(), d.smooth)),
        None => d.smooth.then(|| (Arc::new(vec![1.0]), 1, None, true)),
    }
}

/// Lens Flare's elements for the card at a picture `w` × `h` (rows from
/// the top), as the processor's `looks::flare_spots`: two rows each (the
/// middle and half sizes; the shape and color), how many, and Blend.
pub fn lens_flare_card(
    numbers: [f64; 11],
    canonical: [f64; 2],
    w: u32,
    h: u32,
) -> (Vec<[f32; 4]>, i32, f32) {
    let par = if w > 0 && h > 0 && canonical[1] > 0.0 {
        (canonical[0] / f64::from(w)) / (canonical[1] / f64::from(h))
    } else {
        1.0
    };
    let spots = crate::looks::flare_spots(numbers, w as usize, h as usize, par);
    let mut table: Vec<[f32; 4]> = spots
        .iter()
        .flat_map(|s| {
            [
                [s.at[0], s.at[1], s.half[0], s.half[1]],
                [s.shape as f32, s.color[0], s.color[1], s.color[2]],
            ]
        })
        .collect();
    if table.is_empty() {
        table.push([0.0; 4]);
    }
    (table, spots.len() as i32, numbers[8].clamp(0.0, 1.0) as f32)
}

/// Newsprint's numbers for the card at a picture `w` × `h` (rows from the
/// top), as the processor's `looks::Newsprint`: the table (the rims'
/// coverage, 256 rows, then room for the averaged picture), the averaged
/// picture's columns and rows, i (those, the rows' parity, the dots' kind)
/// and f (the cell, how many fit, the grid's shift, the source's share).
pub fn newsprint_card(
    dot: f64,
    kind: i32,
    canonical: [f64; 2],
    w: u32,
    h: u32,
) -> (Vec<[f32; 4]>, [u32; 2], [i32; 4], [f32; 7]) {
    let par = if w > 0 && h > 0 && canonical[1] > 0.0 {
        (canonical[0] / f64::from(w)) / (canonical[1] / f64::from(h))
    } else {
        1.0
    };
    let n = crate::looks::Newsprint::new(dot, kind, w as usize, h as usize, par);
    let [cols, rows] = n.cells;
    let mut table: Vec<[f32; 4]> = n
        .edges
        .iter()
        .map(|&e| [f32::from(e) / 255.0, 0.0, 0.0, 0.0])
        .collect();
    table.resize(256 + cols * rows, [0.0; 4]);
    (
        table,
        [cols as u32, rows as u32],
        [cols as i32, rows as i32, n.parity, n.kind as i32],
        [
            n.cell[0] as f32,
            n.cell[1] as f32,
            n.span[0] as f32,
            n.span[1] as f32,
            n.shift[0] as f32,
            n.shift[1] as f32,
            n.blend,
        ],
    )
}

/// TV Simulator's numbers for the card at a picture `w` × `h` (rows from
/// the top), as the processor's `looks::Tv`: each row's move, i (the
/// roll), j (the static's seed) and f (the grill's share, Interlacing, the
/// band, the phosphors' powers, Static, the grill's and the scanlines'
/// cycles).
pub fn tv_simulator_card(
    numbers: [f64; 8],
    time: f64,
    canonical: [f64; 2],
    w: u32,
    h: u32,
) -> (Vec<[f32; 4]>, [i32; 4], [u32; 4], [f32; 10]) {
    let par = if w > 0 && h > 0 && canonical[1] > 0.0 {
        (canonical[0] / f64::from(w)) / (canonical[1] / f64::from(h))
    } else {
        1.0
    };
    let tv = crate::looks::Tv::new(numbers, w as usize, h as usize, par, time);
    (
        tv.table(),
        [tv.roll as i32, 0, 0, 0],
        [tv.seed, 0, 0, 0],
        [
            tv.grill,
            tv.interlace,
            tv.band[0],
            tv.band[1],
            tv.phosphor[0],
            tv.phosphor[1],
            tv.phosphor[2],
            tv.snow,
            tv.pitch,
            tv.lines,
        ],
    )
}

/// Film Effects' numbers for the card at a picture `w` × `h` (rows from
/// the top), as the processor's `looks::Film`: its marks (two rows each),
/// i (the shake, grayscale, how many marks), j (the grain's seed) and f
/// (Grain, Tint, the color, the flicker).
pub fn film_effects_card(
    numbers: [f64; 8],
    time: f64,
    w: u32,
    h: u32,
) -> (Vec<[f32; 4]>, [i32; 4], [u32; 4], [f32; 6]) {
    let f = crate::looks::Film::new(numbers, w as usize, h as usize, time);
    (
        f.table(),
        [
            f.shake[0],
            f.shake[1],
            i32::from(f.gray),
            f.marks.len() as i32,
        ],
        [f.seed, 0, 0, 0],
        [
            f.grain, f.tint, f.color[0], f.color[1], f.color[2], f.flicker,
        ],
    )
}

/// Light Rays' numbers for the card at a picture `w` × `h` (rows from the
/// top), as the processor's `looks::Rays`: the bound's rows (for each |dy|
/// from the light, the last |dx| it leaves whole and the first it hides);
/// the parameters' i (the light's column and row, the rays' weight, the
/// threshold), j (the bound, the noise, the noise's first number) and f
/// (the ellipse's reciprocal half axes, where its feather starts and that
/// feather's reciprocal, the blend, the tint).
pub fn light_rays_card(
    numbers: [f64; 13],
    time: f64,
    w: u32,
    h: u32,
) -> (Vec<[f32; 4]>, [i32; 4], [u32; 4], [f32; 8]) {
    let (w, h) = (w as usize, h as usize);
    let r = crate::looks::Rays::new(numbers, w, h, time);
    let [lx, ly] = r.light;
    let rows = ly.max(h.saturating_sub(ly + 1)) + 1;
    let table = r
        .bound_rows(w, rows)
        .into_iter()
        .map(|[whole, hidden]| [whole as f32, hidden as f32, 0.0, 0.0])
        .collect();
    (
        table,
        [lx as i32, ly as i32, r.weight, r.threshold],
        [u32::from(r.bound), r.noise as u32, r.seed, 0],
        [
            r.per[0] as f32,
            r.per[1] as f32,
            r.inner as f32,
            r.feather as f32,
            r.blend as f32,
            r.tint[0] as f32,
            r.tint[1] as f32,
            r.tint[2] as f32,
        ],
    )
}

/// How many times Height Map's box smooths the heights each way.
pub const HEIGHT_PASSES: usize = crate::compositors::HEIGHT_PASSES;

/// The kernels of [`CompositorPlan`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositorKernel {
    Displacement,
    Height,
    BumpMap,
}

/// What the card draws for compositor `id` with `values` over a frame
/// `frame` canonical units big; `None` for another maker's.
pub fn compositor_plan(id: &str, values: Values<'_>, frame: [f64; 2]) -> Option<CompositorPlan> {
    let vals = |specs: &[crate::kit::Spec]| {
        crate::kit::Vals::of(specs, &|spec| {
            values
                .nums(spec.name())
                .map_or_else(|| spec.default(), <[f64]>::to_vec)
        })
    };
    if let Some(kind) = crate::compositors::Kind::of(id) {
        return Some(kind.card_plan(&vals(kind.specs())));
    }
    let kind = crate::filters::Kind::of(id).filter(|k| *k == crate::filters::Kind::BumpMap)?;
    let s = vals(kind.specs());
    let (f, light) = crate::looks::bump_numbers(&s, frame);
    Some(CompositorPlan {
        kernel: CompositorKernel::BumpMap,
        f,
        j: [light, 0, 0, 0],
        flip: s.b(c"flip"),
        smoothness: 0,
    })
}

/// The kernels of [`Plan::Pixel`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelKernel {
    Threshold,
    BlackRestore,
    ChannelBlend,
    MaskGenerator,
    BroadcastColors,
    LabAdjust,
    ColorMatch,
}

/// The kernels of [`Plan::Remap`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemapKernel {
    Spherize,
    OffsetWrap,
    TinyPlanet,
    Convolution,
    QuickBlur,
    BumpMap,
}

/// One of Bézier Masking's masks: its outline (canonical units from the
/// frame's lower left), its weight (Opacity), its feather (canonical
/// units) and Feather Style (0 In, 1 Out, 2 Both).
#[derive(Clone, Debug, PartialEq)]
pub struct MaskShape {
    pub outline: Vec<[f32; 2]>,
    pub weight: f32,
    pub feather: f32,
    pub style: u32,
}

/// Whether effect `id` draws over its chain's input (Bézier Masking's
/// Mask FX): the card keeps that picture for it.
pub fn takes_start(id: &str) -> bool {
    bezier_masking::ID.to_str().ok() == Some(id)
}

/// Where a mask's edge softens, in pixels inside its outline, for a
/// feather of `feather` pixels in `style` (as [`MaskShape`]'s).
pub fn mask_softening(feather: f64, style: u32) -> [f64; 2] {
    bezier_masking::softening(feather, bezier_masking::Feather::of(style as c_int))
}

/// Cookie Cutter's settings: the shape (0 circle, 1 oval, 2 oval on its
/// side, 3 rectangle, 4 diamond, 5 square, 6 triangle, 7 arrowhead,
/// turned `quarters` a quarter clockwise), cut away or kept (`hole`), its
/// border's color, feather, border, repeats, size and center (0–1 across
/// and up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cookie {
    pub color: [f32; 3],
    pub shape: u32,
    pub quarters: u32,
    pub hole: bool,
    pub feather: f32,
    pub border: f32,
    pub repeat: [u32; 2],
    pub size: f32,
    pub center: [f32; 2],
}

/// Layer Dimensionality's settings, worked out: its types (shadow 0 none,
/// 1 dropped, 2 inside; glow 0 none, 1 outer, 2 inner, 3 edge, 4 dual),
/// the four blurs of the picture's alpha (canonical units: the shadow's,
/// the glow's around the edge and inside it, the bevel's — 0, none), the
/// shadow's move (canonical units, y up), the picture's and the ground's
/// rise as the shadow lets them, the light's direction (y up) and the
/// numbers its pixels use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    pub shadow: u32,
    pub glow: u32,
    pub sigmas: [f64; 4],
    pub offset: [f64; 2],
    pub shadow_opacity: f32,
    pub glow_color: [f32; 3],
    pub relief: [f32; 2],
    pub embossed: bool,
    pub light: [f32; 3],
    pub ambience: f32,
    pub intensity: f32,
    pub shininess: f32,
    pub opacity: f32,
    /// The bevel's steepness, per pixel of its blur (at least one).
    pub steep: f32,
    /// How much of the shine and of the ground's shading show.
    pub shine: f32,
    pub ground: f32,
}

/// The three box widths the processor's Gaussian of `sigma` pixels is
/// made of (`bv-effects`' table effects).
pub fn gauss_boxes(sigma: f32) -> [usize; 3] {
    crate::kit::gauss_boxes(sigma)
}

/// Linear Blur's shear of a `w` × `h` picture of a frame `canonical` big:
/// whether the line goes across, its slope, the blur's radius along it
/// and the sheared picture's window (`u` along its rows, then `w` across
/// them).
pub fn line_shear(
    amount: f64,
    angle: f64,
    (w, h): (u32, u32),
    canonical: [f64; 2],
) -> Option<(bool, f64, f64, [i32; 4])> {
    let shear = crate::filters::LineShear::of(
        amount,
        angle,
        f64::from(w.max(h)),
        pixel_shape(canonical, w, h),
        [0, 0, w as c_int, h as c_int],
    )?;
    Some((shear.across, shear.slope, shear.radius, shear.lines))
}

/// Median's and Min and Max's table for `kit_median` over a frame whose
/// longer side is `longer` pixels: the area's weights, then their changes
/// a pixel to the right; and how many of the first there are.
pub fn ranked_table(range: [f64; 2], round: bool, longer: f64) -> (Vec<[f32; 4]>, u32) {
    crate::filters::ranked_table(range, round, longer)
}

/// The rank `kit_median` takes: `rank` (0-1) in 16384ths.
pub fn ranked_rank(rank: f64) -> i32 {
    (rank.clamp(0.0, 1.0) * 16384.0) as i32
}

/// Gaussian Blur's filter on the card at a picture `w` × `h` of a frame
/// `canonical` big: VEGAS's radii for `ranges`, as the recursive filter's
/// numbers across and down.
pub fn gaussian_sections(ranges: [f64; 2], canonical: [f64; 2], w: u32, h: u32) -> [[f32; 16]; 2] {
    let s = gaussian_blur::Settings {
        horizontal: ranges[0],
        vertical: ranges[1],
        channels: [true; 4],
    };
    s.radii([f64::from(w), f64::from(h)], pixel_shape(canonical, w, h))
        .map(blur_sections)
}

/// Unsharp Mask's blur on the card for Radius `radius` at a picture `w` ×
/// `h` of a frame `canonical` big: VEGAS's radii, as the recursive
/// filter's numbers across and down.
pub fn unsharp_sections(radius: f64, canonical: [f64; 2], w: u32, h: u32) -> [[f32; 16]; 2] {
    let s = unsharp_mask::Settings {
        amount: 1.0,
        radius,
        threshold: 0.0,
    };
    s.radii([f64::from(w), f64::from(h)], pixel_shape(canonical, w, h))
        .map(blur_sections)
}

/// Fill Light's blur on the card reaching `reach` of the frame's longer
/// side at a picture `w` × `h` of a frame `canonical` big: VEGAS's radii,
/// as the recursive filter's numbers across and down (none without
/// reach).
pub fn fill_light_sections(
    reach: f64,
    canonical: [f64; 2],
    w: u32,
    h: u32,
) -> Option<[[f32; 16]; 2]> {
    let light = crate::looks::FillLight {
        table: Vec::new(),
        reach,
        amount: 0.0,
    };
    light
        .radii([f64::from(w), f64::from(h)], pixel_shape(canonical, w, h))
        .map(|r| r.map(blur_sections))
}

/// Black Bar Fill's background blur on the card for Blur `blur` at a
/// picture `w` × `h` of a frame `canonical` big: VEGAS's radii, as the
/// recursive filter's numbers across and down (none without blur).
pub fn fill_background_sections(
    blur: f64,
    canonical: [f64; 2],
    w: u32,
    h: u32,
) -> Option<[[f32; 16]; 2]> {
    let size = [f64::from(w), f64::from(h)];
    black_bar_fill::background_radii(blur, size, pixel_shape(canonical, w, h))
        .map(|r| r.map(blur_sections))
}

/// A Gaussian's taps for a sigma of `sigma` pixels, as the processor's
/// spatial effects take them.
pub fn gaussian(sigma: f64) -> Vec<f32> {
    crate::spatial::gaussian(sigma)
}

/// Wave's moves on a picture `w` × `h`, in 16,384ths of a pixel: each
/// row's across, then each column's up.
pub fn wave_offsets(numbers: &[f64; 6], w: u32, h: u32) -> (Vec<i32>, Vec<i32>) {
    crate::filters::wave_offsets(numbers, w, h)
}

/// Mesh Warp's table for the card on a picture `w` × `h`, its size and
/// the numbers it is read with: Target's reduced map (rows up) and the
/// enlargement's ratios, or Source's columns' and rows' cells and shares
/// followed by the points (the points across and up its size).
pub fn mesh_warp_table(
    points: &[[f32; 2]],
    cells: [u32; 2],
    target: bool,
    w: u32,
    h: u32,
) -> (Vec<[f32; 4]>, [u32; 2], [f32; 2]) {
    mesh_warp::card_table(points, cells, target, w, h)
}

/// Lens Correction's numbers for the card on a picture `w` × `h` (its
/// slots): the model of `settings` as the processor works it out.
pub fn lens_correction_numbers(settings: &[f32; 6], ptlens: bool, w: u32, h: u32) -> [f32; 12] {
    lens_correction::card_numbers(settings, ptlens, w, h)
}

/// The 360° effects' table of each column's and row's direction on a
/// picture `w` × `h` (`stereo` 1 top and bottom, 2 side by side), worked
/// out on the processor for `kit_sphere_turn`.
pub fn sphere_table(stereo: u32, w: u32, h: u32) -> Vec<[f32; 4]> {
    stabilization_360::card_table(stereo as c_int, w, h)
}

/// Dual Fish Eye Stitching's table on a picture `w` × `h`: each column's
/// longitude's cosine and sine, then each row's latitude's from the top.
pub fn stitch_table(w: u32, h: u32) -> Vec<[f32; 4]> {
    stitching::card_table(w, h)
}

/// Stereoscopic 3D Adjust on a picture `w` × `h`: whether its map is
/// affine, the table of where its pixels read (`kit_stereo_warp`'s), the
/// floating window's whole columns and partly covered column, and that
/// column's share.
/// Smart Zoom's Lanczos taps on a picture `w` × `h`: each column's first
/// tap and eight weights (three entries), then each row's from the top.
pub fn smart_zoom_table(numbers: &[f32; 5], w: u32, h: u32) -> Vec<[f32; 4]> {
    smart_zoom::card_table(numbers, w, h)
}

/// AI Smoothen's and AI Sharpen's small copy's size and margin on a
/// picture `w` × `h`.
pub fn guided_layout(numbers: &[f32; 4], low: [u32; 3], w: u32, h: u32) -> ([u32; 2], u32) {
    ai_guided::card_layout(numbers, low, w, h)
}

pub fn stereo_card(
    numbers: &[f32; 9],
    flips: [u32; 2],
    w: u32,
    h: u32,
) -> (bool, Vec<[f32; 4]>, [i32; 3], f32) {
    let c = stereo_adjust::card(numbers, flips, w, h);
    (c.affine, c.table, c.band, c.share)
}

/// Video Stabilization's map on a picture `w` × `h` pixels big, as the
/// card works it out (`kit_stabilize`).
pub fn stabilize_numbers(map: &[f64; 6], w: u32, h: u32) -> [f32; 8] {
    stabilization::card_numbers(map, w, h)
}

/// Swirl's and Pinch/Punch's area on a picture `w` × `h`: its center
/// pixel (rows from the top), its half width and height in pixels, and
/// the card's table — each column's offset over the half width and its
/// square, then each row's.
pub fn lens(settings: &[f64; 5], w: u32, h: u32) -> ([i32; 2], [f64; 2], Vec<[f32; 4]>) {
    let lens = crate::filters::Lens::of(settings, w, h);
    (lens.center, lens.reach, lens.table(w, h))
}

/// Deform on a picture `w` × `h`: the card's numbers (the box's origin,
/// the middle, the shears, squeezes and curves), which of them apply
/// (bits) and the deformed picture's box, rows from the top.
pub fn deform(settings: &[f64; 16], w: u32, h: u32) -> ([f32; 14], u32, [i32; 4]) {
    crate::filters::Warp::of(settings, w, h).numbers()
}

/// Sharpen's taps (its neighbours' average), across and up.
pub fn sharpen_taps() -> [f32; 3] {
    sharpen::Settings::taps()
}

/// Radial Blur's steps on a frame whose shorter side is `shorter`
/// pixels: Proportional's part, the share taken, Fixed's reach.
pub fn radial_steps(strength: f64, shorter: f64) -> [f64; 3] {
    crate::filters::radial_steps(strength, shorter)
}

/// Radial Pixelate's blocks across and up a `w` × `h` picture.
pub fn pixelate_blocks(strength: f64, radial: bool, (w, h): (u32, u32)) -> [u32; 2] {
    crate::filters::pixelate_blocks(strength, radial, [w as usize, h as usize]).map(|n| n as u32)
}

/// Chroma Keyer's mask blur, as a Gaussian's sigma: Blur amount over a
/// picture whose longer side is `longer` pixels.
pub fn mask_sigma(amount: f64, longer: f64) -> f64 {
    crate::classics::mask_sigma(amount, longer)
}

/// The radius of the box blurs (three, each way) a Gaussian blur of
/// `sigma` pixels is made of; 0 for none.
pub fn box_radius(sigma: f64) -> usize {
    layer_dimensionality::box_radius(sigma)
}

/// What the card draws for effect `id` with `values`, on a frame `frame`
/// canonical units big at `at`; `None` for an effect that is not
/// BananaV's own.
pub fn plan(id: &str, values: Values<'_>, frame: [f64; 2], at: Moment) -> Option<Plan> {
    let is = |c: &CStr| c.to_str().ok() == Some(id);
    if is(flicker_control::ID) {
        let s = flicker_control::Settings::of_values(values);
        if s.addition {
            Some(Plan::FlickerAdd {
                amount: flicker_control::added(&s, at.frames, at.rate) as f32,
                tones: s.tones,
                mask: s.mask,
            })
        } else if s.histogram {
            let text = values.text(c"histograms").unwrap_or("");
            Some(Plan::FlickerMatch {
                table: flicker_control::levels_table(&s, text, at.frames).map(Arc::new),
                range: flicker_control::mapped_levels(s.tones),
                mask: s.mask,
            })
        } else {
            None
        }
    } else if is(crop::ID) {
        let w = crop::settings_of(values).window([0.0, 0.0], frame);
        let f = |v: [f64; 2]| v.map(|x| x as f32);
        Some(Plan::Crop {
            pivot: f(w.pivot),
            lo: f(w.lo),
            hi: f(w.hi),
            turn: f(w.turn),
            opacity: crop::settings_of(values).opacity as f32,
        })
    } else if is(color_curves::ID) {
        Some(Plan::Curves(color_curves::tables_of(values)))
    } else if is(color_grading::ID) {
        let grade = color_grading::Settings::of(values).grade();
        Some(Plan::Grade(Box::new(grade.plan())))
    } else if is(color_corrector::ID) {
        let c = color_corrector::Corrector::new(&color_corrector::Settings::of(values));
        Some(Plan::Corrector {
            shifts: c.shifts.map(|s| s.map(|x| x as f32)),
            saturation: c.saturation as f32,
            power: c.power as f32,
            gain: c.gain as f32,
            offset: c.offset as f32,
        })
    } else if is(cookie_cutter::ID) {
        Some(Plan::Cookie(cookie_cutter::Settings::of(values).cookie()))
    } else if is(layer_dimensionality::ID) {
        Some(Plan::Layer(
            layer_dimensionality::Settings::of(values).layer(frame),
        ))
    } else if is(pixelate::ID) {
        Some(Plan::Pixelate {
            cells: pixelate::Settings::of(values).cells(frame, 1.0),
        })
    } else if is(picture_in_picture::ID) {
        let s = picture_in_picture::Settings::of(values);
        let aspect = frame[0] / frame[1].max(1e-9);
        Some(match s.warp(aspect) {
            Some(w) => Plan::Warp {
                inverse: std::array::from_fn(|i| w.inverse[i / 3][i % 3] as f32),
                samples: w.samples,
                opacity: s.opacity as f32,
            },
            // Nothing of the picture shows.
            None => Plan::Warp {
                inverse: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                samples: 1,
                opacity: 0.0,
            },
        })
    } else if is(hsl_adjust::ID) {
        let s = hsl_adjust::Settings::of(values);
        Some(Plan::Hsl {
            hue: s.hue.rem_euclid(1.0) as f32,
            saturation: s.saturation as f32,
            luminance: s.luminance as f32,
        })
    } else if is(black_bar_fill::ID) {
        let s = black_bar_fill::Settings::of(values);
        let flat = |m: black_bar_fill::M3| std::array::from_fn(|i| m[i / 3][i % 3] as f32);
        Some(Plan::Fill {
            background: flat(black_bar_fill::inverse(s.bg_location, s.bg_scale)),
            background_samples: black_bar_fill::samples(s.bg_scale),
            background_blur: s.bg_blur,
            foreground: flat(black_bar_fill::inverse(s.fg_location, s.fg_scale)),
            foreground_samples: black_bar_fill::samples(s.fg_scale),
            shadow_sigma: s.shadow_sigma(frame[0]),
            shadow: if s.shadow_blur > 0.0 {
                black_bar_fill::SHADOW_OPACITY
            } else {
                0.0
            },
        })
    } else if is(border::ID) {
        let b = border::Settings::of(values).on(frame);
        Some(Plan::Border {
            kind: b.kind.index(),
            frame: b.frame.map(|v| v as f32),
            width: b.width as f32,
            sigma: b.sigma as f32,
            depth: b.depth.map(|v| v as f32),
            shades: b.shades,
            color: b.color,
        })
    } else if is(saturation_adjust::ID) {
        let s = saturation_adjust::Settings::of(values);
        Some(Plan::Saturation {
            amount: s.amount as f32,
            center: s.center as f32,
            spread: s.spread as f32,
            low: s.low as f32,
            high: s.high as f32,
        })
    } else if is(color_corrector_secondary::ID) {
        let s = color_corrector_secondary::Settings::of(values);
        let c = color_corrector_secondary::Secondary::new(&s);
        let f = |v: [f64; 3]| v.map(|x| x as f32);
        let none = [0.0; 3];
        Some(Plan::Secondary {
            limits: [
                c.luminance.map_or(none, f),
                c.saturation_range.map_or(none, f),
                c.hue.map_or(none, f),
            ],
            flags: c.luminance.is_some() as u32
                | (c.saturation_range.is_some() as u32) << 1
                | (c.hue.is_some() as u32) << 2
                | (c.invert as u32) << 3
                | (c.show_mask as u32) << 4,
            turn: c.turn.map(|x| x as f32),
            shift: c.shift.map(|x| x as f32),
            saturation: c.saturation as f32,
            power: c.power as f32,
            gain: c.gain as f32,
            offset: c.offset as f32,
            alpha: c.alpha as f32,
        })
    } else if is(color_balance::ID) {
        let s = color_balance::Settings::of(values);
        Some(Plan::Balance {
            shifts: s.shifts(),
            range: s.range.index(),
        })
    } else if is(brightness_contrast::ID) {
        let (k, offset) = brightness_contrast::Settings::of(values).line();
        Some(Plan::Affine {
            matrix: [[k, 0.0, 0.0], [0.0, k, 0.0], [0.0, 0.0, k]],
            offset: [offset; 3],
            equalize: false,
        })
    } else if is(levels::ID) {
        // Alpha's levels: the processor.
        levels::Settings::of(values).tables().map(Plan::Curves)
    } else if is(black_and_white::ID) {
        let (matrix, offset) = black_and_white::Settings::of(values).affine();
        Some(Plan::Affine {
            matrix,
            offset,
            equalize: false,
        })
    } else if is(gradient_map::ID) {
        let s = gradient_map::Settings::of(values);
        Some(if s.changes_nothing() {
            Plan::Same
        } else {
            Plan::GradientMap {
                table: gradient_map::Gradient::new(&s.stops).table(),
                amount: s.amount.clamp(0.0, 1.0) as f32,
                add: s.add,
            }
        })
    } else if is(invert::ID) {
        let (k, offset) = invert::Settings::of(values).line();
        Some(Plan::Affine {
            matrix: [[k, 0.0, 0.0], [0.0, k, 0.0], [0.0, 0.0, k]],
            offset: [offset; 3],
            equalize: false,
        })
    } else if is(sepia::ID) {
        // With falloff the tint follows each color's gray: the processor.
        sepia::Settings::of(values)
            .affine()
            .map(|(matrix, offset)| Plan::Affine {
                matrix,
                offset,
                equalize: false,
            })
    } else if is(white_balance::ID) {
        let s = white_balance::Settings::of(values);
        let (matrix, offset) = s.affine();
        Some(Plan::Affine {
            matrix,
            offset,
            equalize: s.equalize,
        })
    } else if is(color_match::ID) {
        Some(color_match::plan(&values))
    } else if is(lut_filter::ID) {
        let s = lut_filter::Settings::of(values);
        Some(Plan::Lut {
            lut: lut_filter::table(&s.file),
            tetrahedral: s.tetrahedral,
            strength: s.strength as f32,
        })
    } else if is(unsharp_mask::ID) {
        let s = unsharp_mask::Settings::of(values);
        Some(Plan::Unsharp {
            radius: if s.changes_nothing() { 0.0 } else { s.radius },
            amount: s.amount as f32,
            threshold: s.threshold as f32,
        })
    } else if is(add_noise::ID) {
        let s = add_noise::Settings::of(values);
        Some(Plan::Noise {
            width: s.width(),
            monochromatic: s.monochromatic,
            gaussian: s.gaussian,
            seed: s.seed(at.frames),
        })
    } else if is(gaussian_blur::ID) {
        let s = gaussian_blur::Settings::of(values);
        Some(if s.changes_nothing() {
            Plan::Same
        } else {
            Plan::Gaussian {
                ranges: [s.horizontal, s.vertical],
                channels: s.channels,
            }
        })
    } else if is(sharpen::ID) {
        let s = sharpen::Settings::of(values);
        Some(if s.changes_nothing() {
            Plan::Same
        } else {
            Plan::Sharpen { gain: s.gain() }
        })
    } else if is(glow::ID) {
        let s = glow::Settings::of(values);
        Some(if s.changes_nothing() {
            Plan::Same
        } else {
            Plan::Glow {
                reach: s.reach(),
                intensity: s.intensity as f32,
                suppression: s.suppression as f32,
                color: s.color.map(|v| v as f32),
            }
        })
    } else if is(vignette::ID) {
        let s = vignette::Settings::of(values);
        Some(if s.changes_nothing() {
            Plan::Same
        } else {
            Plan::Vignette {
                numbers: s.numbers(),
                invert: s.invert,
            }
        })
    } else if is(mirror::ID) {
        let s = mirror::Settings::of(values);
        Some(Plan::Mirror {
            angle: s.angle,
            center: s.center,
            beyond: s.beyond(),
        })
    } else if is(bezier_masking::ID) {
        Some(bezier_masking::Settings::of(values).plan(frame))
    } else if is(lens_correction::ID) {
        Some(lens_correction::Settings::of_values(values).plan())
    } else if is(stabilization::ID) {
        Some(stabilization::Settings::of_values(values, at.rate).plan(at.frames, frame))
    } else if is(stabilization_360::ID) {
        Some(stabilization_360::Settings::of_values(values).plan(at.frames))
    } else if is(scene_rotation::ID) {
        Some(scene_rotation::Settings::of_values(values).plan())
    } else if is(stitching::ID) {
        Some(stitching::Settings::of_values(values).plan())
    } else if let Some(s) = ai_guided::Settings::of_values(id, values) {
        Some(s.plan())
    } else if is(smart_zoom::ID) {
        Some(smart_zoom::Settings::of_values(values).plan())
    } else if is(stereo_adjust::ID) {
        Some(stereo_adjust::Settings::of_values(values).plan())
    } else if is(stabilize::ID) {
        Some(stabilize::Settings::of_values(values).plan(at.frames, frame, at.rate))
    } else if is(mesh_warp::ID) {
        Some(mesh_warp::Settings::of_values(values).plan())
    } else if let Some(kind) = crate::filters::Kind::of(id) {
        let vals = crate::kit::Vals::of(kind.specs(), &|spec| {
            values
                .nums(spec.name())
                .map_or_else(|| spec.default(), <[f64]>::to_vec)
        });
        kind.card_plan(&vals, frame, at.frames)
    } else if is(timecode::ID) {
        let settings = timecode::Settings::of(values);
        let text = settings.text(at.ruler, at.rate, bv_core::locale::decimal_separator());
        Some(Plan::Timecode(Caption { text, settings }))
    } else {
        None
    }
}
