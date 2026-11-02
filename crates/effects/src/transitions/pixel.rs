//! The transitions VEGAS works out pixel by pixel from the two pictures —
//! Linear Wipe, Clock Wipe, Iris, Star Wipe, Dissolve, Flash and Cross
//! Effect — with VEGAS's panels, presets and math: in the frame's pixels
//! (x from its left, y from its bottom as OpenFX counts them, or v from its
//! top where VEGAS counts so), the pixels' shape taken in as VEGAS takes
//! it ([`Frame::shape`]), in floats as VEGAS's are. Flash and Cross Effect
//! blur as VEGAS blurs ([`crate::recursive_gauss`]). The card runs the same
//! numbers ([`plan`]).

use std::ffi::{CStr, c_int};
use std::sync::Mutex;

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use super::Kind;
use crate::accel::{TransitionKernel, TransitionPlan, TransitionSteps};
use crate::kit::{Buf, Spec, Vals};
use crate::ofx::{Fx, Props, for_rows};
use crate::recursive_gauss;

const TAU: f32 = 6.283_185_5;

// Panels, as VEGAS's.

pub(super) const STEREO: Spec = Spec::Ranged(
    c"stereoscopicDepth",
    c"Stereoscopic 3D depth",
    0.0,
    [-0.5, 0.5],
    [-0.1, 0.1],
    c"How far apart the two eyes' pictures are in stereoscopic 3D projects (none in others).",
);

pub(super) const LINEAR_WIPE: &[Spec] = &[
    Spec::Number(
        c"angle",
        c"Angle",
        0.0,
        0.0,
        360.0,
        c"Where the second picture comes from: 0 the left, 90 the top, 180 the right, 270 the bottom.",
    ),
    Spec::Number(
        c"feather",
        c"Feather",
        0.1,
        0.0,
        1.0,
        c"How soft the edge is: 0 a hard edge, 1 a quarter of the frame each way.",
    ),
];

pub(super) const CLOCK_WIPE: &[Spec] = &[
    Spec::Choice(
        c"direction",
        c"Direction",
        &[c"Clockwise", c"Counterclockwise"],
        0,
        c"Which way the hand sweeps, from twelve o'clock.",
    ),
    Spec::Number(
        c"featherAngle",
        c"Feather angle",
        30.0,
        0.0,
        360.0,
        c"How many degrees the hand's edge is softened over.",
    ),
];

pub(super) const IRIS_SHAPES: &[&CStr] = &[
    c"Circle",
    c"Oval",
    c"Oval side",
    c"Rectangle",
    c"Diamond",
    c"Square",
    c"Triangle",
    c"Triangle right",
    c"Triangle upside down",
    c"Triangle left",
    c"Arrowhead",
    c"Arrowhead right",
    c"Arrowhead upside down",
    c"Arrowhead left",
];

pub(super) const IRIS: &[Spec] = &[
    Spec::Rgb(c"color", c"Color", [1.0, 0.0, 1.0], c"The border's color."),
    Spec::Point(
        c"center",
        c"Center",
        [0.5, 0.5],
        c"Where the shape grows from, in each of the repeated cells.",
    ),
    STEREO,
    Spec::Choice(c"shape", c"Shape", IRIS_SHAPES, 0, c"The iris's shape."),
    Spec::Choice(
        c"direction",
        c"Direction",
        &[c"Iris in", c"Iris out"],
        1,
        c"Iris out: the shape opens onto the next picture. Iris in: it closes onto it.",
    ),
    Spec::Number(
        c"feather",
        c"Feather",
        0.0,
        0.0,
        1.0,
        c"How soft the shape's edge is.",
    ),
    Spec::Number(
        c"border",
        c"Border",
        0.0,
        0.0,
        1.0,
        c"How wide the colored border along the shape's edge is.",
    ),
    Spec::Int(
        c"repeatX",
        c"Repeat X",
        1,
        1,
        10,
        c"How many times the shape repeats across.",
    ),
    Spec::Int(
        c"repeatY",
        c"Repeat Y",
        1,
        1,
        10,
        c"How many times the shape repeats down.",
    ),
];

pub(super) const STAR_WIPE: &[Spec] = &[
    Spec::Int(c"arms", c"Arms", 5, 2, 32, c"How many points the star has."),
    Spec::Number(
        c"ratio",
        c"Ratio",
        0.381,
        0.0,
        1.0,
        c"How deep the cuts between the points are: 0 a cut to the middle, 1 none.",
    ),
    Spec::Number(
        c"angle",
        c"Angle",
        18.0,
        0.0,
        360.0,
        c"The star turned, in degrees.",
    ),
    Spec::Number(
        c"feather",
        c"Feather",
        0.0,
        0.0,
        1.0,
        c"How soft the star's edge is.",
    ),
    Spec::Number(
        c"border",
        c"Border",
        0.0,
        0.0,
        1.0,
        c"How wide the colored border along the star's edge is.",
    ),
    Spec::Rgb(
        c"borderColor",
        c"Border color",
        [1.0, 0.0, 1.0],
        c"The border's color.",
    ),
    Spec::Point(
        c"center",
        c"Center",
        [0.5, 0.5],
        c"Where the star grows from.",
    ),
    Spec::Int(
        c"cycles",
        c"Cycles",
        1,
        1,
        5,
        c"How many stars grow one inside the other.",
    ),
    Spec::Choice(
        c"waveform",
        c"Waveform",
        &[c"Seesaw", c"Tooth"],
        0,
        c"Seesaw: the star grows from the middle. Tooth: from the middle and the edges at once.",
    ),
    Spec::Toggle(
        c"horizontalFlip",
        c"Horizontal flip",
        false,
        c"The star and its center turned upside down.",
    ),
    Spec::Toggle(
        c"verticalFlip",
        c"Vertical flip",
        false,
        c"The star and its center mirrored left to right.",
    ),
    Spec::Toggle(
        c"horizontalMirror",
        c"Horizontal mirror",
        false,
        c"The top half of the wipe mirrored onto the bottom.",
    ),
    Spec::Toggle(
        c"verticalMirror",
        c"Vertical mirror",
        false,
        c"The left half of the wipe mirrored onto the right.",
    ),
    Spec::Choice(
        c"direction",
        c"Direction",
        &[c"Forward", c"Reverse"],
        0,
        c"Forward: the next picture shows inside the star first. Reverse: outside it first.",
    ),
];

pub(super) const FLASH: &[Spec] = &[
    Spec::Number(
        c"horizontalDiffusion",
        c"Horizontal diffusion",
        0.0,
        0.0,
        1.0,
        c"How far the light spreads across.",
    ),
    Spec::Number(
        c"verticalDiffusion",
        c"Vertical diffusion",
        0.0,
        0.0,
        1.0,
        c"How far the light spreads up and down.",
    ),
    Spec::Rgb(
        c"tint",
        c"Tint",
        [0.0, 0.0, 0.0],
        c"The color the picture's spread light takes: black leaves only the white flash.",
    ),
];

pub(super) const DISSOLVE_TYPES: &[&CStr] = &[
    c"Additive dissolve",
    c"Subtractive dissolve",
    c"Subtractive crossfade",
    c"Color bleed",
    c"Pixel color morph",
    c"Threshold dissolve",
    c"Threshold appear",
    c"Fade through grayscale",
    c"Fade through color",
    c"R then G then B crossfade",
];

const fn speed(name: &'static CStr, label: &'static CStr) -> Spec {
    Spec::Number(
        name,
        label,
        0.0,
        0.0,
        1.0,
        c"How much faster this channel changes than the slowest.",
    )
}

pub(super) const DISSOLVE: &[Spec] = &[
    Spec::Choice(
        c"type",
        c"Type",
        DISSOLVE_TYPES,
        0,
        c"How one picture turns into the other.",
    ),
    Spec::Group(c"colorBleedSpeed", c"Color bleed speed"),
    speed(c"colorBleedSpeedRed", c"Red"),
    speed(c"colorBleedSpeedGreen", c"Green"),
    speed(c"colorBleedSpeedBlue", c"Blue"),
    speed(c"colorBleedSpeedAlpha", c"Alpha"),
    Spec::GroupEnd,
    Spec::Group(c"colorMorphSpeed", c"Color morph speed"),
    speed(c"colorMorphSpeedRed", c"Red"),
    speed(c"colorMorphSpeedGreen", c"Green"),
    speed(c"colorMorphSpeedBlue", c"Blue"),
    speed(c"colorMorphSpeedAlpha", c"Alpha"),
    Spec::GroupEnd,
    Spec::Color(
        c"fadeThroughColor",
        c"Fade through color",
        [0.0, 0.0, 0.0, 1.0],
        c"The color the pictures fade through halfway.",
    ),
];

pub(super) const CROSS_EFFECT: &[Spec] = &[
    Spec::Choice(
        c"effectType",
        c"Effect",
        &[c"None", c"Zoom", c"Pixelate", c"Blur"],
        0,
        c"What happens to the pictures while one fades into the other.",
    ),
    Spec::Choice(
        c"applyTo",
        c"Apply to",
        &[c"Source", c"Destination", c"Both"],
        2,
        c"Which pictures it happens to: the one going, the one coming or both.",
    ),
    Spec::Number(
        c"fadeRange",
        c"Fade range",
        1.0,
        0.0,
        1.0,
        c"How much of the transition the fade takes, around its middle: 0 a cut.",
    ),
    Spec::Group(c"effectSettings", c"Effect settings"),
    Spec::Number(
        c"maxZoom",
        c"Max zoom",
        1.0,
        1.0,
        128.0,
        c"How many times larger the pictures grow at most.",
    ),
    STEREO,
    Spec::Number(
        c"maxX",
        c"Max X",
        1.0,
        0.0,
        1.0,
        c"How wide the blocks grow: 1 the whole frame.",
    ),
    Spec::Number(
        c"maxY",
        c"Max Y",
        1.0,
        0.0,
        1.0,
        c"How tall the blocks grow: 1 the whole frame.",
    ),
    Spec::Number(
        c"maxBlur",
        c"Max blur",
        1.0,
        0.0,
        1.0,
        c"How much the pictures blur at most.",
    ),
    Spec::Point(
        c"source",
        c"Source",
        [0.5, 0.5],
        c"Where the picture going zooms into.",
    ),
    Spec::Point(
        c"destination",
        c"Destination",
        [0.5, 0.5],
        c"Where the picture coming zooms from.",
    ),
    Spec::GroupEnd,
];

/// The kinds this module draws.
pub(super) fn draws(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::LinearWipe
            | Kind::ClockWipe
            | Kind::Iris
            | Kind::StarWipe
            | Kind::Dissolve
            | Kind::Flash
            | Kind::CrossEffect
    )
}

/// Greys out (Dissolve) or hides (Cross Effect) what the choices leave
/// unused, as VEGAS does.
pub(super) fn show(kind: Kind, fx: Fx, time: f64) {
    let vals = Vals::of(kind.specs(), &|spec| spec.read(fx, time));
    match kind {
        Kind::Dissolve => {
            let t = vals.i(c"type");
            for c in ["Red", "Green", "Blue", "Alpha"] {
                let bleed = format!("colorBleedSpeed{c}\0");
                let morph = format!("colorMorphSpeed{c}\0");
                if let (Ok(b), Ok(m)) = (
                    CStr::from_bytes_with_nul(bleed.as_bytes()),
                    CStr::from_bytes_with_nul(morph.as_bytes()),
                ) {
                    fx.param(b).set_enabled(t == 3);
                    fx.param(m).set_enabled(t == 4);
                }
            }
            fx.param(c"fadeThroughColor").set_enabled(t == 8);
        }
        Kind::CrossEffect => {
            let (effect, apply) = (vals.i(c"effectType"), vals.i(c"applyTo"));
            fx.param(c"effectSettings").set_secret(effect == 0);
            fx.param(c"applyTo").set_secret(effect == 0);
            fx.param(c"maxZoom").set_secret(effect != 1);
            fx.param(c"stereoscopicDepth").set_secret(effect != 1);
            fx.param(c"maxX").set_secret(effect != 2);
            fx.param(c"maxY").set_secret(effect != 2);
            fx.param(c"maxBlur").set_secret(effect != 3);
            fx.param(c"source").set_secret(effect != 1 || apply == 1);
            fx.param(c"destination")
                .set_secret(effect != 1 || apply == 0);
        }
        _ => {}
    }
}

// The frame and the settings.

/// The frame in the render's pixels: `w` × `h`, its pixels `par` times as
/// wide as tall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    pub w: usize,
    pub h: usize,
    pub par: f64,
}

impl Frame {
    /// How VEGAS takes the pixels' shape in: across times the aspect when
    /// it is under 1, down divided by it otherwise.
    fn shape(&self) -> (f32, f32) {
        let par = self.par as f32;
        if par >= 1.0 {
            (1.0, 1.0 / par)
        } else {
            (par, 1.0)
        }
    }
}

/// Iris's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Iris {
    color: [f32; 3],
    center: [f32; 2],
    shape: i32,
    out: bool,
    feather: f32,
    border: f32,
    repeat: [i32; 2],
}

/// Star Wipe's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Star {
    arms: i32,
    ratio: f32,
    angle: f32,
    feather: f32,
    border: f32,
    color: [f32; 3],
    center: [f32; 2],
    cycles: i32,
    tooth: bool,
    flip: [bool; 2],
    mirror: [bool; 2],
    reverse: bool,
}

/// Cross Effect's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Cross {
    effect: i32,
    apply: i32,
    fade_range: f64,
    max_zoom: f64,
    max: [f64; 2],
    max_blur: f64,
    source: [f64; 2],
    destination: [f64; 2],
}

/// A transition's settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Settings {
    Linear {
        angle: f32,
        feather: f32,
    },
    Clock {
        counter: bool,
        feather: f32,
    },
    Iris(Iris),
    Star(Star),
    Dissolve {
        kind: i32,
        bleed: [f32; 4],
        morph: [f32; 4],
        color: [f32; 4],
    },
    Flash {
        spread: [f64; 2],
        tint: [f32; 3],
    },
    Cross(Cross),
}

impl Settings {
    fn read(kind: Kind, get: &dyn Fn(Spec) -> Vec<f64>) -> Settings {
        let v = Vals::of(kind.specs(), get);
        let f = |n: &CStr| v.f(n) as f32;
        // Points as VEGAS keeps them: 0–1 up from the bottom.
        let up = |n: &CStr| {
            let p = v.point(n);
            [p[0], 1.0 - p[1]]
        };
        let rgb = |n: &CStr| v.get_rgb(n).map(|c| c as f32);
        match kind {
            Kind::LinearWipe => Settings::Linear {
                angle: f(c"angle"),
                feather: f(c"feather"),
            },
            Kind::ClockWipe => Settings::Clock {
                counter: v.i(c"direction") > 0,
                feather: f(c"featherAngle"),
            },
            Kind::Iris => Settings::Iris(Iris {
                color: rgb(c"color"),
                center: up(c"center").map(|c| c.clamp(0.0, 1.0) as f32),
                shape: v.i(c"shape").clamp(0, 13),
                out: v.i(c"direction") != 0,
                feather: f(c"feather").clamp(0.0, 1.0),
                border: f(c"border").clamp(0.0, 1.0),
                repeat: [v.i(c"repeatX").clamp(1, 10), v.i(c"repeatY").clamp(1, 10)],
            }),
            Kind::StarWipe => Settings::Star(Star {
                arms: v.i(c"arms").clamp(2, 32),
                ratio: f(c"ratio").clamp(0.0, 1.0),
                angle: f(c"angle").clamp(0.0, 360.0),
                feather: f(c"feather"),
                border: f(c"border"),
                color: rgb(c"borderColor"),
                center: up(c"center").map(|c| c.clamp(0.0, 1.0) as f32),
                cycles: v.i(c"cycles").clamp(1, 5),
                tooth: v.i(c"waveform") != 0,
                flip: [v.b(c"horizontalFlip"), v.b(c"verticalFlip")],
                mirror: [v.b(c"horizontalMirror"), v.b(c"verticalMirror")],
                reverse: v.i(c"direction") != 0,
            }),
            Kind::Dissolve => {
                let four = |p: &str| {
                    ["Red", "Green", "Blue", "Alpha"].map(|c| {
                        let name = format!("{p}{c}\0");
                        CStr::from_bytes_with_nul(name.as_bytes()).map_or(0.0, |n| v.f(n) as f32)
                    })
                };
                Settings::Dissolve {
                    kind: v.i(c"type").clamp(0, 9),
                    bleed: four("colorBleedSpeed"),
                    morph: four("colorMorphSpeed"),
                    color: v.rgba(c"fadeThroughColor"),
                }
            }
            Kind::Flash => Settings::Flash {
                spread: [v.f(c"horizontalDiffusion"), v.f(c"verticalDiffusion")],
                tint: rgb(c"tint"),
            },
            _ => Settings::Cross(Cross {
                effect: v.i(c"effectType").clamp(0, 3),
                apply: v.i(c"applyTo").clamp(0, 2),
                fade_range: v.f(c"fadeRange"),
                max_zoom: v.f(c"maxZoom"),
                max: [v.f(c"maxX"), v.f(c"maxY")],
                max_blur: v.f(c"maxBlur"),
                source: up(c"source"),
                destination: up(c"destination"),
            }),
        }
    }
}

// Iris's shapes.

/// How far out iris shape `shape` reaches to the point `x`, `y` (pixels
/// from its center, y up), `a` the cell's height over its width: the
/// shapes grow as it does.
fn iris_norm(shape: i32, x: f32, y: f32, a: f32) -> f32 {
    const R3: f32 = 0.577_350_26;
    const R3X2: f32 = 1.154_700_5;
    const S3: f32 = 1.732_050_8;
    const TWO3: f32 = 0.666_666_7;
    const ONE3: f32 = 0.333_333_34;
    const LONG: f32 = 1.118_034;
    const SHORT: f32 = 0.559_017;
    match shape {
        1 => ((x * a) * (x * a) + y * y).sqrt(),
        2 => (x * x + (y * a) * (y * a)).sqrt(),
        3 => (x.abs() * a).max(y.abs()),
        4 => y.abs() + x.abs(),
        5 => x.abs().max(y.abs()),
        // The triangles: how far to the nearest of their sides.
        6 => {
            let t = x * R3;
            if x <= 0.0 && t < y {
                ((TWO3 - y) * -R3 + (0.0 - x)).abs()
            } else if y <= t && y < -t {
                ((-ONE3 - y) * R3X2).abs()
            } else {
                ((-ONE3 - y) * -R3 - (R3 - x)).abs()
            }
        }
        7 => {
            let t = -(x * S3);
            if y <= t || y < 0.0 {
                if y <= t && x * S3 < y {
                    ((-ONE3 - x) * R3X2).abs()
                } else {
                    ((-R3 - y) - (-ONE3 - x) * R3).abs()
                }
            } else {
                (y - (TWO3 - x) * R3).abs()
            }
        }
        8 => {
            if 0.0 <= x && y < x * R3 {
                ((-TWO3 - y) * R3 - (0.0 - x)).abs()
            } else if x < 0.0 && y <= -(x * R3) {
                ((ONE3 - y) * R3 + (-R3 - x)).abs()
            } else {
                ((ONE3 - y) * -R3X2).abs()
            }
        }
        9 => {
            if 0.0 < y && x * S3 <= y {
                (-(R3 - y) - (ONE3 - x) * -R3).abs()
            } else if y <= 0.0 && y < -(x * S3) {
                (-(-R3 - y) - (ONE3 - x) * R3).abs()
            } else {
                ((ONE3 - x) * R3X2).abs()
            }
        }
        // The arrowheads: pieces of lines of slopes 0.4, 0.8 and 1.6.
        10 => {
            if 0.0 <= x && y < -x {
                ((-0.4 - y) * 0.8 - (0.0 - x) * -0.4).abs() * LONG
            } else if 0.0 < x && -x <= y {
                ((-0.8 - y) * -0.8 - (0.8 - x) * 1.6).abs() * SHORT
            } else if x <= 0.0 && x < y {
                ((0.8 - y) * -0.8 - (0.0 - x) * -1.6).abs() * SHORT
            } else {
                ((-0.8 - y) * 0.8 - (-0.8 - x) * 0.4).abs() * LONG
            }
        }
        11 => {
            if 0.0 <= y && -x < y {
                ((0.0 - y) * -1.6 - (0.8 - x) * 0.8).abs() * SHORT
            } else if 0.0 < y && y <= -x {
                ((0.8 - y) * 0.4 - (-0.8 - x) * -0.8).abs() * LONG
            } else if 0.0 < y || y <= x {
                ((-0.8 - y) * 1.6 - (-0.8 - x) * 0.8).abs() * SHORT
            } else {
                ((0.0 - y) * -0.4 - (-0.4 - x) * -0.8).abs() * LONG
            }
        }
        12 => {
            if 0.0 < x && x <= y {
                ((0.8 - y) * -0.8 - (0.8 - x) * -0.4).abs() * LONG
            } else if 0.0 < x {
                ((-0.8 - y) * 0.8 - (0.0 - x) * 1.6).abs() * SHORT
            } else if -x < y {
                ((0.4 - y) * -0.8 - (0.0 - x) * 0.4).abs() * LONG
            } else if 0.0 <= x {
                ((-0.8 - y) * 0.8 - (0.0 - x) * 1.6).abs() * SHORT
            } else {
                ((0.8 - y) * 0.8 - (-0.8 - x) * -1.6).abs() * SHORT
            }
        }
        13 => {
            if 0.0 <= y && y < x {
                ((0.0 - y) * 0.4 - (0.4 - x) * 0.8).abs() * LONG
            } else if 0.0 < y && x <= y {
                ((0.8 - y) * -1.6 - (0.8 - x) * -0.8).abs() * SHORT
            } else if y <= 0.0 && y < -x {
                ((0.0 - y) * 1.6 - (-0.8 - x) * -0.8).abs() * SHORT
            } else {
                ((-0.8 - y) * -0.4 - (0.8 - x) * 0.8).abs() * LONG
            }
        }
        _ => (x * x + y * y).sqrt(),
    }
}

/// Iris's numbers on a frame: the cells, the shape's aspect and the
/// largest reach on the frame's edge (each pixel's reach is divided by it).
#[derive(Clone, Copy, Debug, PartialEq)]
struct IrisFrame {
    cell: [f32; 2],
    first: [f32; 2],
    aspect: f32,
    scale: (f32, f32),
    most: f32,
}

impl Iris {
    fn frame(&self, f: &Frame) -> IrisFrame {
        let [rx, ry] = self.repeat;
        let cw = ((f.w as f32 / rx as f32) + 0.5) as i32;
        let ch = ((f.h as f32 / ry as f32) + 0.5) as i32;
        let (sx, sy) = f.shape();
        let mut n = IrisFrame {
            cell: [cw as f32, ch as f32],
            first: [
                (f64::from(cw) * f64::from(self.center[0])) as f32,
                (f64::from(ch) * f64::from(self.center[1])) as f32,
            ],
            aspect: (ch as f32 * sy) / (cw as f32 * sx),
            scale: (sx, sy),
            most: 1.0,
        };
        // The largest reach on the frame's edge, at least 1.
        let (w, h) = (f.w as i32, f.h as i32);
        let mut most = 1.0f32;
        for y in 0..h {
            for x in [0, w - 1] {
                most = most.max(self.norm(&n, x, y));
            }
        }
        for x in 0..w {
            for y in [0, h - 1] {
                most = most.max(self.norm(&n, x, y));
            }
        }
        n.most = most;
        n
    }

    /// The shape's reach to pixel `x`, `y` (from the bottom): the nearest
    /// of the cells' shapes.
    fn norm(&self, n: &IrisFrame, x: i32, y: i32) -> f32 {
        let (px, py) = (x as f32, y as f32);
        let cell = |p: f32, first: f32, size: f32, count: i32| {
            if p >= first {
                (((p - first) as i32) / (size as i32).max(1)).min(count - 1)
            } else {
                -1
            }
        };
        let ix = cell(px, n.first[0], n.cell[0], self.repeat[0]);
        let iy = cell(py, n.first[1], n.cell[1], self.repeat[1]);
        let mut best = f32::MAX;
        for j in (iy - 1).max(0)..=(iy + 2).min(self.repeat[1] - 1) {
            for i in (ix - 1).max(0)..=(ix + 2).min(self.repeat[0] - 1) {
                let cx = n.first[0] + i as f32 * n.cell[0];
                let cy = n.first[1] + j as f32 * n.cell[1];
                let d = iris_norm(
                    self.shape,
                    (px - cx) * n.scale.0,
                    (py - cy) * n.scale.1,
                    n.aspect,
                );
                best = best.min(d);
            }
        }
        best
    }
}

/// Mixes for a wipe with a colored band: the pixel's share of the picture
/// inside (`inner`), outside (`outer`) and the color, and which alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Band {
    inner: f32,
    outer: f32,
    color: f32,
    /// The alpha: the inner picture's (0), the outer one's (1), the
    /// color's (2).
    alpha: u8,
}

impl Band {
    const INNER: Band = Band {
        inner: 1.0,
        outer: 0.0,
        color: 0.0,
        alpha: 0,
    };
    const OUTER: Band = Band {
        inner: 0.0,
        outer: 1.0,
        color: 0.0,
        alpha: 1,
    };
    const COLOR: Band = Band {
        inner: 0.0,
        outer: 0.0,
        color: 1.0,
        alpha: 2,
    };

    fn paint(self, inner: [f32; 4], outer: [f32; 4], color: [f32; 3]) -> [f32; 4] {
        let mut out: [f32; 4] = std::array::from_fn(|c| {
            let k = if c < 3 { color[c] } else { 1.0 };
            self.inner * inner[c] + self.outer * outer[c] + self.color * k
        });
        out[3] = match self.alpha {
            0 => inner[3],
            1 => outer[3],
            _ => 1.0,
        };
        out
    }
}

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// Iris's band at `v` (the pixel's reach over the largest), `at` of the
/// way (its own way, inverted for Iris in).
fn iris_band(v: f32, at: f32, border: f32, feather: f32) -> Band {
    if border >= 1e-6 {
        let lo = (at - border).max(0.0).min(at);
        let hi = (at + border).min(1.0).max(at);
        let lo2 = (lo - feather).max(0.0).min(lo);
        let hi2 = (hi + feather).min(1.0).max(hi);
        if v < lo2 {
            Band::INNER
        } else if v > hi2 {
            Band::OUTER
        } else if lo < v {
            if v < hi {
                Band::COLOR
            } else {
                let t = clamp01((v - hi) * (1.0 / (hi2 - hi)));
                Band {
                    inner: 0.0,
                    outer: t,
                    color: 1.0 - t,
                    alpha: 1,
                }
            }
        } else {
            let t = clamp01((1.0 / (lo - lo2)) * (v - lo2));
            Band {
                inner: 1.0 - t,
                outer: 0.0,
                color: t,
                alpha: 0,
            }
        }
    } else if feather >= 1e-6 {
        let lo = (at - feather).max(0.0).min(at);
        let hi = (feather + at).min(1.0).max(at);
        if v < lo {
            Band::INNER
        } else if v > hi {
            Band::OUTER
        } else {
            let t = clamp01((1.0 / (hi - lo)) * (v - lo));
            Band {
                inner: 1.0 - t,
                outer: t,
                color: 0.0,
                alpha: u8::from(t >= 0.5),
            }
        }
    } else if at <= v {
        Band::OUTER
    } else {
        Band::INNER
    }
}

// Star Wipe's map.

/// Star Wipe's numbers on a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
struct StarFrame {
    center: [f32; 2],
    cos: f32,
    sin: f32,
    sector: f32,
    slope: f32,
    scale: (f32, f32),
    /// Cycles over the largest reach on the frame's edge.
    per: f32,
    /// The map is all 1 (the frame too small for the star).
    flat: bool,
    /// The mirrored maps' lowest value and the scale that takes them to
    /// 0–1, and the widths' factor (1, or 2 for Tooth) times it.
    low: f32,
    stretch: f32,
    factor: f32,
}

impl Star {
    fn frame(&self, f: &Frame) -> StarFrame {
        let (w, h) = (f.w as i32, f.h as i32);
        let sector = TAU / self.arms as f32;
        let half = sector * 0.5;
        let slope = (half.sin() * -self.ratio) / (1.0 - half.cos() * self.ratio);
        let (mut cx, mut cy, mut angle) = (self.center[0], 1.0 - self.center[1], self.angle);
        if self.flip[0] {
            cy = 1.0 - cy;
            angle = 360.0 - angle;
        }
        if self.flip[1] {
            angle = 180.0 - angle;
            cx = 1.0 - cx;
            if angle < 0.0 {
                angle += 360.0;
            }
        }
        let rad = angle * 0.017_453_292;
        let mut n = StarFrame {
            center: [(w - 1) as f32 * cx, (1.0 - cy) * (h - 1) as f32],
            cos: rad.cos(),
            sin: rad.sin(),
            sector,
            slope,
            scale: f.shape(),
            per: 1.0,
            flat: false,
            low: 0.0,
            stretch: 1.0,
            factor: 1.0,
        };
        let mut most = 0.0f32;
        for y in 0..h {
            most = most.max(star_norm(&n, 0, y)).max(star_norm(&n, w - 1, y));
        }
        for x in 0..w {
            most = most.max(star_norm(&n, x, 0)).max(star_norm(&n, x, h - 1));
        }
        if most < 1.0 {
            n.flat = true;
            return n;
        }
        n.per = self.cycles as f32 / most;
        n.factor = if self.tooth { 2.0 } else { 1.0 };
        if self.mirror[0] || self.mirror[1] {
            // The part worked out, its values stretched to 0–1 — the
            // highest taken only among values not under the lowest so far,
            // as VEGAS takes it.
            let (rows, cols) = self.mirrored_part(w, h);
            let (mut low, mut high) = (1.0f32, 0.0f32);
            for y in rows.clone() {
                for x in cols.clone() {
                    let v = self.wave(&n, x, y, true);
                    if low <= v {
                        high = high.max(v);
                    } else {
                        low = v;
                    }
                }
            }
            let stretch = if (high - low).abs() > 1e-6 {
                1.0 / (high - low)
            } else {
                1.0
            };
            n.low = low;
            n.stretch = stretch;
            n.factor *= stretch;
        }
        n
    }

    /// The rows (from the bottom) and columns of the map the mirrors copy
    /// from: the top half, the left half or the top left quarter.
    fn mirrored_part(&self, w: i32, h: i32) -> (std::ops::Range<i32>, std::ops::Range<i32>) {
        let rows = if self.mirror[0] { h / 2..h } else { 0..h };
        let cols = if self.mirror[1] { 0..w / 2 } else { 0..w };
        (rows, cols)
    }

    /// The map's wave at pixel `x`, `y` before any stretching (`mirrored`:
    /// as the mirrored maps take its whole cycles off).
    fn wave(&self, n: &StarFrame, x: i32, y: i32, mirrored: bool) -> f32 {
        let mut v = n.per * star_norm(n, x, y);
        if mirrored {
            if v > 1.0 {
                v -= (v as i32) as f32;
            }
        } else {
            while v > 1.0 {
                v -= 1.0;
            }
        }
        if self.tooth {
            if v >= 0.5 {
                v = 1.0 - v;
            }
            v += v;
        }
        if self.reverse {
            v = 1.0 - v;
        }
        clamp01(v)
    }

    /// The map at pixel `x`, `y` (from the bottom).
    fn map(&self, n: &StarFrame, f: &Frame, x: i32, y: i32) -> f32 {
        if n.flat {
            return 1.0;
        }
        if !self.mirror[0] && !self.mirror[1] {
            return self.wave(n, x, y, false);
        }
        let (w, h) = (f.w as i32, f.h as i32);
        let mut y = y;
        let mut x = x;
        if self.mirror[0] && y < h / 2 {
            y = 2 * (h / 2) - 1 - y;
        }
        if self.mirror[1] && x >= w / 2 {
            // An odd width's middle column is left out, at 0.
            if w % 2 == 1 && x == w / 2 {
                return 0.0;
            }
            x = w - 1 - x;
        }
        (self.wave(n, x, y, true) - n.low) * n.stretch
    }
}

/// The star's reach to pixel `x`, `y` (from the bottom), at least 1.
fn star_norm(n: &StarFrame, x: i32, y: i32) -> f32 {
    let (px, py) = (x as f32, y as f32);
    if (px - n.center[0]).abs() < 1e-6 && (py - n.center[1]).abs() < 1e-6 {
        return 1.0;
    }
    let dx = (px - n.center[0]) * n.scale.0;
    let dy = (py - n.center[1]) * n.scale.1;
    let u = dx * n.cos + dy * n.sin;
    let v = dy * n.cos - dx * n.sin;
    let mut phi = v.atan2(u);
    if phi < 0.0 {
        phi += TAU;
    }
    let half = n.sector * 0.5;
    let arm = if half < phi && phi < TAU - half {
        (((half + phi) / n.sector) as i32) as f32 * n.sector
    } else {
        0.0
    };
    let along = (arm.sin() * v + arm.cos() * u).abs();
    let across = (arm.cos() * v - arm.sin() * u).abs();
    let norm = across - along * n.slope;
    if norm >= 1.0 { norm } else { 1.0 }
}

/// Star Wipe's band at map value `v`, `p` of the way: the first picture
/// is the outer one, the second the inner.
fn star_band(v: f32, p: f32, border: f32, feather: f32) -> Band {
    if border >= 1e-6 && feather >= 1e-6 {
        let (lo, hi) = (p - border, p + border);
        let (lo2, hi2) = (lo - feather, hi + feather);
        if v < lo2 {
            Band::INNER
        } else if v > hi2 {
            Band::OUTER
        } else if v < lo2 || lo < v {
            if lo < v && v < hi {
                Band::COLOR
            } else {
                let t = clamp01((v - hi) * (1.0 / (hi2 - hi)));
                Band {
                    inner: 0.0,
                    outer: t,
                    color: 1.0 - t,
                    alpha: 1,
                }
            }
        } else {
            let t = clamp01((v - lo2) * (1.0 / (lo - lo2)));
            Band {
                inner: 1.0 - t,
                outer: 0.0,
                color: t,
                alpha: 0,
            }
        }
    } else if border >= 1e-6 {
        if v < p - border {
            Band::INNER
        } else if v <= p + border {
            Band::COLOR
        } else {
            Band::OUTER
        }
    } else if feather >= 1e-6 {
        let (lo, hi) = (p - feather, feather + p);
        if v < lo {
            Band::INNER
        } else if v > hi {
            Band::OUTER
        } else {
            let t = clamp01(1.0 - (v - lo) * (1.0 / (hi - lo)));
            Band {
                inner: t,
                outer: 1.0 - t,
                color: 0.0,
                alpha: u8::from(t < 0.5),
            }
        }
    } else if p <= v {
        Band::OUTER
    } else {
        Band::INNER
    }
}

/// Dissolve's pixel from `a` and `b`, `p` of the way.
fn dissolve(
    kind: i32,
    bleed: [f32; 4],
    morph: [f32; 4],
    color: [f32; 4],
    p: f32,
    a: [f32; 4],
    b: [f32; 4],
) -> [f32; 4] {
    let lerp = |x: f32, y: f32, t: f32| x + (y - x) * t;
    let alpha = lerp(a[3], b[3], p);
    let rgb = |f: &dyn Fn(usize) -> f32| [f(0), f(1), f(2), alpha];
    match kind {
        0 => {
            let mut out: [f32; 4] = if p < 0.5 {
                std::array::from_fn(|c| (p + p) * b[c] + a[c])
            } else {
                let k = 1.0 - ((p - 0.5) + (p - 0.5));
                std::array::from_fn(|c| k * a[c] + b[c])
            };
            out[3] = clamp01(out[3]);
            out
        }
        1 => {
            if p < 0.5 {
                rgb(&|c| (a[c] - (p + p) * b[c]).abs())
            } else {
                let k = 1.0 - ((p - 0.5) + (p - 0.5));
                rgb(&|c| (b[c] - k * a[c]).abs())
            }
        }
        2 => rgb(&|c| ((a[c] + b[c]) * p - a[c]).abs()),
        3 => {
            // The blue's speed goes unused: the blue follows the green's.
            let s = [
                bleed[0] + 1.0,
                bleed[1] + 1.0,
                bleed[1] + 1.0,
                bleed[3] + 1.0,
            ];
            let most = (1.0 / s[0]).max(1.0 / s[1]).max(1.0 / s[3]);
            let q = most * p;
            let r = most - q;
            let mut out: [f32; 4] = std::array::from_fn(|c| {
                (a[c] - clamp01(s[c] * q)).max(0.0) + (b[c] - clamp01(s[c] * r)).max(0.0)
            });
            out[3] = clamp01(out[3]);
            out
        }
        4 => {
            let s = morph.map(|v| v + 1.0);
            let most = (1.0 / s[0]).max(1.0 / s[1]).max(1.0 / s[2]).max(1.0 / s[3]);
            std::array::from_fn(|c| {
                let step = clamp01(s[c] * most * p);
                if a[c] >= b[c] {
                    (a[c] - step).max(b[c])
                } else {
                    (a[c] + step).min(b[c])
                }
            })
        }
        5 => {
            if (a[1] + a[0] + a[2]) / 3.0 <= p {
                b
            } else {
                a
            }
        }
        6 => {
            if (b[1] + b[0] + b[2]) / 3.0 < 1.0 - p {
                a
            } else {
                b
            }
        }
        7 => {
            let g = (a[0] + a[1] + a[2] + b[0] + b[1] + b[2]) / 6.0;
            if p < 0.5 {
                rgb(&|c| lerp(a[c], g, p + p))
            } else {
                rgb(&|c| lerp(g, b[c], (p - 0.5) + (p - 0.5)))
            }
        }
        8 => {
            let c = [
                color[0] * color[3],
                color[1] * color[3],
                color[2] * color[3],
                color[3],
            ];
            if p < 0.5 {
                std::array::from_fn(|k| lerp(a[k], c[k], p + p))
            } else {
                let t = clamp01((p - 0.5) + (p - 0.5));
                std::array::from_fn(|k| lerp(c[k], b[k], t))
            }
        }
        _ => {
            let third = |k: f32| clamp01((p - k / 3.0) * 3.0);
            [
                lerp(a[0], b[0], third(0.0)),
                lerp(a[1], b[1], third(1.0)),
                lerp(a[2], b[2], third(2.0)),
                alpha,
            ]
        }
    }
}

// Cross Effect's cells.

/// Cross Effect's cells: each cell column's first pixel and width, each
/// cell row's (rows from the bottom), and each pixel's column and row of
/// cells.
#[derive(Clone, Debug, Default, PartialEq)]
struct Cells {
    columns: Vec<(i32, i32)>,
    rows: Vec<(i32, i32)>,
    of_x: Vec<u32>,
    of_y: Vec<u32>,
}

impl Cells {
    /// `count` cells over `size` pixels, as VEGAS lays them: each from the
    /// whole pixel its share reaches, the last to the end.
    fn line(size: usize, count: usize) -> (Vec<(i32, i32)>, Vec<u32>) {
        let count = count.clamp(1, size.max(1));
        let step = size as f32 / count as f32;
        let mut cells = Vec::with_capacity(count);
        for i in 0..count {
            let start = (i as f32 * step) as i32;
            let mut width = ((i + 1) as f32 * step - i as f32 * step) as i32;
            if i + 1 == count {
                width = size as i32 - start;
            }
            cells.push((start, width.max(1)));
        }
        let mut of = vec![0u32; size];
        let mut k = 0usize;
        for (x, o) in of.iter_mut().enumerate() {
            while k + 1 < count && cells[k + 1].0 <= x as i32 {
                k += 1;
            }
            *o = k as u32;
        }
        (cells, of)
    }

    fn of(f: &Frame, counts: [usize; 2]) -> Cells {
        let (columns, of_x) = Cells::line(f.w, counts[0]);
        let (rows, of_y) = Cells::line(f.h, counts[1]);
        Cells {
            columns,
            rows,
            of_x,
            of_y,
        }
    }

    /// The table the card reads: the columns, the rows, then each pixel's
    /// column and each row's (from the bottom).
    fn table(&self) -> Vec<[f32; 4]> {
        let mut t: Vec<[f32; 4]> = Vec::new();
        t.extend(
            self.columns
                .iter()
                .map(|&(s, w)| [s as f32, w as f32, 0.0, 0.0]),
        );
        t.extend(
            self.rows
                .iter()
                .map(|&(s, h)| [s as f32, h as f32, 0.0, 0.0]),
        );
        t.extend(self.of_x.iter().map(|&i| [i as f32, 0.0, 0.0, 0.0]));
        t.extend(self.of_y.iter().map(|&j| [j as f32, 0.0, 0.0, 0.0]));
        t
    }

    /// `pic` (the frame's pixels, rows from the bottom) with each pixel
    /// its cell's average.
    fn averaged(&self, pic: &[[f32; 4]], w: usize) -> Vec<[f32; 4]> {
        let nx = self.columns.len();
        let mut means = vec![[0.0f32; 4]; nx * self.rows.len()];
        for (j, &(y0, hh)) in self.rows.iter().enumerate() {
            for (i, &(x0, ww)) in self.columns.iter().enumerate() {
                let mut acc = [0.0f64; 4];
                for y in y0..y0 + hh {
                    let row = &pic[y as usize * w..];
                    for px in &row[x0 as usize..(x0 + ww) as usize] {
                        for c in 0..4 {
                            acc[c] += f64::from(px[c]);
                        }
                    }
                }
                means[j * nx + i] = acc.map(|s| (s / f64::from(ww) / f64::from(hh)) as f32);
            }
        }
        let mut out = vec![[0.0f32; 4]; pic.len()];
        for (y, row) in out.chunks_mut(w).enumerate() {
            let j = self.of_y[y] as usize;
            for (x, px) in row.iter_mut().enumerate() {
                *px = means[j * nx + self.of_x[x] as usize];
            }
        }
        out
    }
}

impl Cross {
    /// How far the fade is: 0 the first picture, 1 the second.
    fn mix(&self, p: f32) -> f32 {
        if self.fade_range >= 0.001 {
            let r = self.fade_range as f32;
            clamp01((p - (1.0 - r) * 0.5) / r)
        } else if p <= 0.5 {
            0.0
        } else {
            1.0
        }
    }

    /// How strong the effect is on each picture (Pixelate's and Blur's).
    fn strengths(&self, p: f32) -> [f32; 2] {
        match self.apply {
            0 => [p, 0.0],
            1 => [0.0, 1.0 - p],
            _ => {
                let e = if p > 0.5 {
                    (1.0 - p) + (1.0 - p)
                } else {
                    p + p
                };
                [e, e]
            }
        }
    }

    /// Pixelate's cell counts across and down at strength `e`.
    fn counts(&self, f: &Frame, e: f32) -> [usize; 2] {
        let (sx, sy) = f.shape();
        let q = e.powf(0.25);
        let fx = (1.0 - q * (self.max[0] as f32 * 0.25 + 0.75)) * sx;
        let fy = (1.0 - q * (self.max[1] as f32 * 0.25 + 0.75)) * sy;
        let n = |size: usize, k: f32| {
            if k > 0.999 {
                size
            } else {
                ((size as f32 * k * 0.5 + 1.0) as i32).max(1) as usize
            }
        };
        [n(f.w, fx), n(f.h, fy)]
    }

    /// Blur's radii at strength `e`.
    fn radii(&self, f: &Frame, e: f32) -> [f64; 2] {
        let (sx, sy) = f.shape();
        let longest = (f.w as f32 * sx).max(f.h as f32 * sy);
        let r = self.max_blur as f32 * e * longest * 0.25;
        [f64::from(r * sx), f64::from(r * sy)]
    }

    /// Zoom's factors for the first and the second picture.
    fn zooms(&self, p: f32) -> [f32; 2] {
        let z = self.max_zoom as f32;
        match self.apply {
            0 => [z.powf(p), 1.0],
            1 => [1.0, z.powf(1.0 - p)],
            _ => [z.powf(p), z.powf(1.0 - p)],
        }
    }
}

/// Flash's numbers `p` of the way: whether the second picture lights, the
/// flash's white (f²) and the light's weight.
fn flash_numbers(p: f32) -> (bool, f32, f32) {
    let second = p >= 0.5;
    let q = if second { 1.0 - p } else { p };
    let f = q + q;
    (second, f * f, 1.0 - (1.0 - f) * (1.0 - f))
}

fn flash_radii(f: &Frame, spread: [f64; 2], w: f32) -> [f64; 2] {
    let (sx, sy) = f.shape();
    let longest = ((f.w as f32 * sx).max(f.h as f32 * sy) as i32) as f64;
    [
        spread[0] * 0.25 * longest * f64::from(w) / f.par,
        spread[1] * 0.25 * longest * f64::from(w),
    ]
}

fn flash_pixel(base: [f32; 4], light: [f32; 4], white: f32, w: f32, tint: [f32; 3]) -> [f32; 4] {
    let y = (light[0] * 0.299 + light[1] * 0.587 + light[2] * 0.114) * w;
    let a = y + white + base[3];
    [
        y * tint[0] + white + base[0],
        y * tint[1] + white + base[1],
        y * tint[2] + white + base[2],
        if a > 1.0 { 1.0 } else { a },
    ]
}

/// Linear Wipe's numbers: the direction (pixels' shape in), the lowest
/// value on the frame's corners and one over the span.
fn linear_numbers(f: &Frame, angle: f32) -> [f32; 4] {
    let (sx, sy) = f.shape();
    let a = f64::from(angle).to_radians();
    let n = [(a.cos() as f32) * sx, (a.sin() as f32) * sy];
    let (w, h) = ((f.w as f32 - 1.0).max(0.0), (f.h as f32 - 1.0).max(0.0));
    let corners = [0.0f32, n[0] * w, n[1] * h, n[0] * w + n[1] * h];
    let lo = corners.iter().copied().fold(f32::MAX, f32::min);
    let hi = corners.iter().copied().fold(f32::MIN, f32::max);
    let span = hi - lo;
    [n[0], n[1], lo, if span > 1e-6 { 1.0 / span } else { 0.0 }]
}

/// Linear Wipe's share of the first picture at pixel `x`, `v` (from the
/// top).
fn linear_share(n: [f32; 4], feather: f32, p: f32, x: f32, v: f32) -> f32 {
    let d = (n[0] * x + n[1] * v - n[2]) * n[3];
    let e = feather * 0.25;
    let at = (e + 1.0 + e) * p - e;
    if d < at - e {
        0.0
    } else if d > at + e {
        1.0
    } else {
        let inv = if feather > 0.0 { 2.0 / feather } else { 0.0 };
        let t = ((d - at) + e) * inv;
        (3.0 - (t + t)) * t * t
    }
}

/// Clock Wipe's share of the second picture at pixel `x`, `v` (from the
/// top).
#[allow(clippy::too_many_arguments)]
fn clock_share(f: &Frame, counter: bool, feather: f32, p: f32, x: f32, v: f32) -> f32 {
    let (sx, sy) = f.shape();
    let phi = feather * 0.017_453_292;
    let full = phi + TAU;
    let edge = full * p;
    let (hw, hh) = ((f.w / 2) as f32, (f.h / 2) as f32);
    let dy = ((hh + 0.5) - v) * sy;
    let dx = (if counter { hw - x } else { x - hw }) * sx;
    let mut th = dx.atan2(dy);
    if th < 0.0 {
        th += TAU;
    }
    if edge <= th {
        return 0.0;
    }
    let r = (dx * dx + dy * dy).sqrt().max(2.0);
    let w = (std::f32::consts::SQRT_2 / r).asin().max(phi);
    let start = (edge - w).min(full);
    if p >= 1.0 || th <= start {
        return 1.0;
    }
    let s = 1.0 - (th - start) / w;
    (3.0 - (s + s)) * s * s
}

// Rendering on the processor.

/// The numbers a frame needs (worked out once a frame).
enum Numbers {
    Linear([f32; 4]),
    Clock,
    Iris(IrisFrame),
    Star(StarFrame),
    Dissolve,
    /// Flash: the picture lighting (the second?), the white, the weight and
    /// the light's picture (blurred, or the lighting picture itself).
    Flash(bool, f32, f32, Option<Buf>),
    /// Cross Effect: the fade, and the pictures as the effect leaves them
    /// (none: as they are); the zooms; the blur after the mix.
    Cross {
        mix: f32,
        a: Option<Vec<[f32; 4]>>,
        b: Option<Vec<[f32; 4]>>,
        zooms: [f32; 2],
        after: Option<[f64; 2]>,
    },
}

/// The frame's numbers that do not change with the progress, kept for the
/// next frame (Iris's and Star Wipe's reach over the whole frame).
static KEPT: Mutex<Vec<(Settings, Frame, KeptFrame)>> = Mutex::new(Vec::new());

#[derive(Clone, Copy, Debug)]
enum KeptFrame {
    Iris(IrisFrame),
    Star(StarFrame),
}

fn kept(settings: &Settings, f: &Frame) -> Option<KeptFrame> {
    if !matches!(settings, Settings::Iris(_) | Settings::Star(_)) {
        return None;
    }
    if let Ok(list) = KEPT.lock()
        && let Some((_, _, k)) = list.iter().find(|(s, fr, _)| s == settings && fr == f)
    {
        return Some(*k);
    }
    let k = match settings {
        Settings::Iris(i) => KeptFrame::Iris(i.frame(f)),
        Settings::Star(s) => KeptFrame::Star(s.frame(f)),
        _ => return None,
    };
    if let Ok(mut list) = KEPT.lock() {
        if list.len() >= 8 {
            list.remove(0);
        }
        list.push((*settings, *f, k));
    }
    Some(k)
}

/// The frame's pictures with rows from the bottom, `w` × `h`, from a
/// picture read whole (nothing outside it).
pub(crate) fn framed(pic: &Buf, origin: (c_int, c_int), f: &Frame) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0f32; 4]; f.w * f.h];
    for (y, row) in out.chunks_mut(f.w).enumerate() {
        for (x, px) in row.iter_mut().enumerate() {
            *px = pic.inside(origin.0 + x as c_int, origin.1 + y as c_int);
        }
    }
    out
}

/// A picture (rows from the bottom) blurred as VEGAS blurs.
fn blur_frame(pic: Vec<[f32; 4]>, f: &Frame, radii: [f64; 2]) -> Vec<[f32; 4]> {
    let mut buf = Buf {
        bounds: [0, 0, f.w as c_int, f.h as c_int],
        data: pic,
    };
    recursive_gauss::blur(&mut buf, radii);
    buf.data
}

/// Pixel `x`, `y` of a frame picture between pixels (rows from the
/// bottom), the last column and row held, as VEGAS reads a zoomed one.
fn zoomed(pic: &[[f32; 4]], f: &Frame, x: f32, y: f32) -> [f32; 4] {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (w, h) = (f.w as i32, f.h as i32);
    let at = |i: i32, j: i32| pic[(j.clamp(0, h - 1) * w + i.clamp(0, w - 1)) as usize];
    let (i, j) = (x0 as i32, y0 as i32);
    let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
    std::array::from_fn(|k| {
        let top = (b[k] - a[k]) * fx + a[k];
        let bottom = c[k] + (d[k] - c[k]) * fx;
        (bottom - top) * fy + top
    })
}

fn numbers(settings: &Settings, f: &Frame, p: f32, a: &[[f32; 4]], b: &[[f32; 4]]) -> Numbers {
    match settings {
        Settings::Linear { angle, .. } => Numbers::Linear(linear_numbers(f, *angle)),
        Settings::Clock { .. } => Numbers::Clock,
        Settings::Iris(_) | Settings::Star(_) => match kept(settings, f) {
            Some(KeptFrame::Iris(n)) => Numbers::Iris(n),
            Some(KeptFrame::Star(n)) => Numbers::Star(n),
            None => Numbers::Clock,
        },
        Settings::Dissolve { .. } => Numbers::Dissolve,
        Settings::Flash { spread, .. } => {
            let (second, white, w) = flash_numbers(p);
            let r = flash_radii(f, *spread, w);
            let light = (r[0] > 0.0 || r[1] > 0.0).then(|| Buf {
                bounds: [0, 0, f.w as c_int, f.h as c_int],
                data: blur_frame(if second { b.to_vec() } else { a.to_vec() }, f, r),
            });
            Numbers::Flash(second, white, w, light)
        }
        Settings::Cross(c) => {
            let mix = c.mix(p);
            let [ea, eb] = c.strengths(p);
            let (mut pa, mut pb, mut after) = (None, None, None);
            match c.effect {
                2 => {
                    let cells = if c.apply == 1 {
                        Cells::of(f, c.counts(f, eb))
                    } else {
                        Cells::of(f, c.counts(f, ea))
                    };
                    if c.apply != 1 {
                        pa = Some(cells.averaged(a, f.w));
                    }
                    if c.apply != 0 {
                        pb = Some(cells.averaged(b, f.w));
                    }
                }
                3 => match c.apply {
                    0 => {
                        let r = c.radii(f, ea);
                        if r[0] > 0.0 || r[1] > 0.0 {
                            pa = Some(blur_frame(a.to_vec(), f, r));
                        }
                    }
                    1 => {
                        let r = c.radii(f, eb);
                        if r[0] > 0.0 || r[1] > 0.0 {
                            pb = Some(blur_frame(b.to_vec(), f, r));
                        }
                    }
                    _ => {
                        let r = c.radii(f, ea);
                        if r[0] > 0.0 || r[1] > 0.0 {
                            after = Some(r);
                        }
                    }
                },
                _ => {}
            }
            Numbers::Cross {
                mix,
                a: pa,
                b: pb,
                zooms: c.zooms(p),
                after,
            }
        }
    }
}

impl Settings {
    /// Pixel `x`, `y` (from the bottom) of the frame, `p` of the way, from
    /// the frame pictures `a` and `b` (rows from the bottom).
    #[allow(clippy::too_many_arguments)]
    fn pixel(
        &self,
        n: &Numbers,
        f: &Frame,
        p: f32,
        x: usize,
        y: usize,
        a: &[[f32; 4]],
        b: &[[f32; 4]],
    ) -> [f32; 4] {
        let i = y * f.w + x;
        let (pa, pb) = (a[i], b[i]);
        let v = (f.h - 1 - y) as f32;
        let mix = |k: f32, from: [f32; 4], to: [f32; 4]| -> [f32; 4] {
            std::array::from_fn(|c| (to[c] - from[c]) * k + from[c])
        };
        match (self, n) {
            (Settings::Linear { feather, .. }, Numbers::Linear(numbers)) => {
                let k = linear_share(*numbers, *feather, p, x as f32, v);
                mix(k, pb, pa)
            }
            (Settings::Clock { counter, feather }, _) => {
                let k = clock_share(f, *counter, *feather, p, x as f32, v);
                mix(k, pa, pb)
            }
            (Settings::Iris(s), Numbers::Iris(n)) => {
                let reach = s.norm(n, x as i32, y as i32) * (1.0 / n.most);
                let (at, inner, outer) = if s.out {
                    (p, pb, pa)
                } else {
                    (1.0 - p, pa, pb)
                };
                let feather = if s.feather >= 0.01 {
                    s.feather
                } else {
                    s.repeat[0] as f32 * 0.01 * s.repeat[1] as f32
                } * 0.05;
                iris_band(reach, at, s.border * 0.05, feather).paint(inner, outer, s.color)
            }
            (Settings::Star(s), Numbers::Star(n)) => {
                let m = s.map(n, f, x as i32, y as i32);
                let border = s.border * n.factor * 0.125;
                let feather = s.feather * n.factor * 0.125;
                star_band(m, p, border, feather).paint(pb, pa, s.color)
            }
            (
                Settings::Dissolve {
                    kind,
                    bleed,
                    morph,
                    color,
                },
                _,
            ) => dissolve(*kind, *bleed, *morph, *color, p, pa, pb),
            (Settings::Flash { tint, .. }, Numbers::Flash(second, white, w, light)) => {
                let base = if *second { pb } else { pa };
                let lit = light.as_ref().map_or(base, |l| l.data[i]);
                flash_pixel(base, lit, *white, *w, *tint)
            }
            (
                Settings::Cross(c),
                Numbers::Cross {
                    mix: k,
                    a: ea,
                    b: eb,
                    zooms,
                    ..
                },
            ) => {
                let mut from = ea.as_ref().map_or(pa, |e| e[i]);
                let mut to = eb.as_ref().map_or(pb, |e| e[i]);
                if c.effect == 1 {
                    let (w, h) = (f.w as f32, f.h as f32);
                    if c.apply != 1 {
                        let (cx, cy) = (c.source[0] as f32 * w, c.source[1] as f32 * h);
                        let z = 1.0 / zooms[0];
                        from = zoomed(a, f, (x as f32 - cx) * z + cx, (y as f32 - cy) * z + cy);
                    }
                    if c.apply != 0 {
                        let (cx, cy) = (c.destination[0] as f32 * w, c.destination[1] as f32 * h);
                        let z = 1.0 / zooms[1];
                        to = zoomed(b, f, (x as f32 - cx) * z + cx, (y as f32 - cy) * z + cy);
                    }
                }
                mix(*k, from, to)
            }
            _ => pa,
        }
    }
}

/// Renders transition `kind` on the processor.
pub(super) fn render(kind: Kind, fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|v| if v > 0.0 { v } else { 1.0 });
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let from = fx
        .clip(kOfxImageEffectTransitionSourceFromClipName)
        .image(time);
    let to = fx
        .clip(kOfxImageEffectTransitionSourceToClipName)
        .image(time);
    for img in [&from, &to].into_iter().flatten() {
        if img.depth != out.depth || img.channels != out.channels {
            return kOfxStatErrImageFormat;
        }
    }
    let progress = fx.param(kOfxImageEffectTransitionParamName).f64_at(time);
    let settings = Settings::read(kind, &|spec| spec.read(fx, time));
    let (origin, frame) = fx.frame();
    let par = if out.par > 0.0 { out.par } else { 1.0 };
    let f = Frame {
        w: (frame[0] * scale[0] / par).round().max(1.0) as usize,
        h: (frame[1] * scale[1]).round().max(1.0) as usize,
        par,
    };
    let at = (
        (origin[0] * scale[0] / par).round() as c_int,
        (origin[1] * scale[1]).round() as c_int,
    );
    let (a, b) = (Buf::read(from.as_ref()), Buf::read(to.as_ref()));
    let (a, b) = (framed(&a, at, &f), framed(&b, at, &f));
    drop(from);
    drop(to);
    let p = progress as f32;
    let n = numbers(&settings, &f, p, &a, &b);
    // Cross Effect blurring the mix: the frame worked out whole first.
    let whole: Option<Vec<[f32; 4]>> = match &n {
        Numbers::Cross { after: Some(r), .. } => {
            let mut mixed = vec![[0.0f32; 4]; f.w * f.h];
            for (y, row) in mixed.chunks_mut(f.w).enumerate() {
                for (x, px) in row.iter_mut().enumerate() {
                    *px = settings.pixel(&n, &f, p, x, y, &a, &b);
                }
            }
            Some(blur_frame(mixed, &f, *r))
        }
        _ => None,
    };
    let out_straight = out.channels == 4 && out.unpremultiplied;
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(row) = out.row(y) else { return };
        let fy = y - at.1;
        for x in columns.clone() {
            let fxx = x - at.0;
            let mut c = if fxx < 0 || fy < 0 || fxx >= f.w as c_int || fy >= f.h as c_int {
                [0.0; 4]
            } else if let Some(w) = &whole {
                w[fy as usize * f.w + fxx as usize]
            } else {
                settings.pixel(&n, &f, p, fxx as usize, fy as usize, &a, &b)
            };
            if out_straight && c[3] > 0.0 {
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

// On the card.

/// What the card draws for transition `kind` at `progress`, in a frame of
/// `size` pixels (`canonical` its size in canonical units: the pixels'
/// shape).
pub(super) fn plan(
    kind: Kind,
    values: crate::accel::Values<'_>,
    canonical: [f64; 2],
    size: [f64; 2],
    progress: f64,
) -> TransitionPlan {
    let settings = Settings::read(kind, &|spec| {
        values
            .nums(spec.name())
            .map(<[f64]>::to_vec)
            .unwrap_or_else(|| spec.default())
    });
    let par = if size[0] > 0.0 && size[1] > 0.0 && canonical[1] > 0.0 {
        (canonical[0] / size[0]) / (canonical[1] / size[1])
    } else {
        1.0
    };
    let f = Frame {
        w: size[0].max(1.0) as usize,
        h: size[1].max(1.0) as usize,
        par,
    };
    let p = progress as f32;
    let (sx, sy) = f.shape();
    let mut fv = [0.0f32; 24];
    let mut i = [0i32; 4];
    let mut steps = TransitionSteps::None;
    let kernel = match settings {
        Settings::Linear { angle, feather } => {
            let n = linear_numbers(&f, angle);
            fv[..4].copy_from_slice(&n);
            fv[4] = feather;
            fv[5] = p;
            TransitionKernel::LinearWipe
        }
        Settings::Clock { counter, feather } => {
            fv[..6].copy_from_slice(&[sx, sy, feather, p, (f.w / 2) as f32, (f.h / 2) as f32]);
            i[0] = i32::from(counter);
            TransitionKernel::ClockWipe
        }
        Settings::Iris(s) => {
            let n = match kept(&settings, &f) {
                Some(KeptFrame::Iris(n)) => n,
                _ => s.frame(&f),
            };
            let feather = if s.feather >= 0.01 {
                s.feather
            } else {
                s.repeat[0] as f32 * 0.01 * s.repeat[1] as f32
            } * 0.05;
            fv[..15].copy_from_slice(&[
                p,
                s.border * 0.05,
                feather,
                1.0 / n.most,
                n.scale.0,
                n.scale.1,
                n.aspect,
                n.cell[0],
                n.cell[1],
                n.first[0],
                n.first[1],
                s.color[0],
                s.color[1],
                s.color[2],
                0.0,
            ]);
            i = [s.shape, s.repeat[0], s.repeat[1], i32::from(s.out)];
            TransitionKernel::Iris
        }
        Settings::Star(s) => {
            let n = match kept(&settings, &f) {
                Some(KeptFrame::Star(n)) => n,
                _ => s.frame(&f),
            };
            fv[..20].copy_from_slice(&[
                p,
                s.border * n.factor * 0.125,
                s.feather * n.factor * 0.125,
                n.center[0],
                n.center[1],
                n.cos,
                n.sin,
                n.sector,
                n.slope,
                n.per,
                n.low,
                n.stretch,
                n.scale.0,
                n.scale.1,
                s.color[0],
                s.color[1],
                s.color[2],
                0.0,
                0.0,
                0.0,
            ]);
            let mirrors = u32::from(s.mirror[0]) | (u32::from(s.mirror[1]) << 1);
            i = [
                i32::from(s.tooth),
                i32::from(s.reverse),
                mirrors as i32,
                i32::from(n.flat),
            ];
            TransitionKernel::StarWipe
        }
        Settings::Dissolve {
            kind: t,
            bleed,
            morph,
            color,
        } => {
            fv[0] = p;
            fv[1..5].copy_from_slice(&bleed);
            fv[5..9].copy_from_slice(&morph);
            fv[9..13].copy_from_slice(&color);
            i[0] = t;
            TransitionKernel::Dissolve
        }
        Settings::Flash { spread, tint } => {
            let (second, white, w) = flash_numbers(p);
            let r = flash_radii(&f, spread, w);
            fv[..5].copy_from_slice(&[white, w, tint[0], tint[1], tint[2]]);
            steps = TransitionSteps::Light {
                second,
                blur: (r[0] > 0.0 || r[1] > 0.0).then(|| {
                    [
                        recursive_gauss::sections(recursive_gauss::sigma(r[0])),
                        recursive_gauss::sections(recursive_gauss::sigma(r[1])),
                    ]
                }),
            };
            TransitionKernel::Flash
        }
        Settings::Cross(c) => {
            let [ea, eb] = c.strengths(p);
            let zooms = c.zooms(p);
            let (w, h) = (f.w as f32, f.h as f32);
            fv[..7].copy_from_slice(&[
                c.mix(p),
                1.0 / zooms[0],
                1.0 / zooms[1],
                c.source[0] as f32 * w,
                c.source[1] as f32 * h,
                c.destination[0] as f32 * w,
                c.destination[1] as f32 * h,
            ]);
            let zoom_bits = match c.apply {
                0 => 1,
                1 => 2,
                _ => 3,
            };
            i[0] = if c.effect == 1 { zoom_bits } else { 0 };
            let pictures = match c.apply {
                0 => 1,
                1 => 2,
                _ => 3,
            };
            steps = match c.effect {
                2 => {
                    let e = if c.apply == 1 { eb } else { ea };
                    let cells = Cells::of(&f, c.counts(&f, e));
                    TransitionSteps::Cells {
                        pictures,
                        counts: [cells.columns.len() as u32, cells.rows.len() as u32],
                        table: cells.table(),
                    }
                }
                3 => {
                    let r = c.radii(&f, if c.apply == 1 { eb } else { ea });
                    if r[0] > 0.0 || r[1] > 0.0 {
                        TransitionSteps::Blur {
                            pictures: if c.apply == 2 { 4 } else { pictures },
                            taps: [
                                recursive_gauss::sections(recursive_gauss::sigma(r[0])),
                                recursive_gauss::sections(recursive_gauss::sigma(r[1])),
                            ],
                        }
                    } else {
                        TransitionSteps::None
                    }
                }
                _ => TransitionSteps::None,
            };
            TransitionKernel::CrossEffect
        }
    };
    TransitionPlan {
        kernel,
        f: fv,
        i,
        j: [0; 4],
        steps,
    }
}

// Presets, VEGAS's.

/// The factory presets of transition `kind`.
pub(super) fn presets(kind: Kind) -> Vec<crate::Preset> {
    use crate::{Preset, PresetValue};
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    let p = |name: &'static str, values: Vec<(&'static str, PresetValue)>| Preset { name, values };
    match kind {
        Kind::LinearWipe => {
            let w = |name, angle: f64, feather: f64| {
                p(
                    name,
                    vec![("angle", num(&[angle])), ("feather", num(&[feather]))],
                )
            };
            vec![
                w("Top-Left, Diagonal, Soft Edge", 45.0, 0.1),
                w("Top-Down, Hard Edge", 90.0, 0.0),
                w("Left-Right, Hard Edge", 0.0, 0.0),
                w("Bottom-Up, Hard Edge", 270.0, 0.0),
                w("Right-Left, Hard Edge", 180.0, 0.0),
                w("Right-Left, Soft Edge", 180.0, 0.5),
                w("Top-Down, Soft Edge", 90.0, 0.5),
                w("Left-Right, Soft Edge", 0.0, 0.5),
                w("Top-Left, Diagonal, Blend In", 45.0, 1.0),
                w("Bottom-Left, Diagonal, Soft Edge", 315.0, 0.1),
                w("Bottom-Right, Diagonal, Soft Edge", 225.0, 0.1),
                w("Top-Right, Diagonal, Soft Edge", 135.0, 0.1),
            ]
        }
        Kind::ClockWipe => {
            let w = |name, dir: f64, feather: f64| {
                p(
                    name,
                    vec![
                        ("direction", num(&[dir])),
                        ("featherAngle", num(&[feather])),
                    ],
                )
            };
            vec![
                w("Clockwise, Soft Edge", 0.0, 30.0),
                w("Counter Clockwise, Soft Edge", 1.0, 30.0),
                w("Clockwise, Hard Edge", 0.0, 0.0),
                w("Counter Clockwise, Hard Edge", 1.0, 0.0),
                w("Clockwise, Blend In", 0.0, 220.0),
                w("Counter Clockwise, Blend In", 1.0, 220.0),
            ]
        }
        Kind::Iris => {
            // Name, color, center, shape, direction, feather, border, repeats.
            #[allow(clippy::type_complexity)]
            let rows: [(&'static str, [f64; 3], [f64; 2], f64, f64, f64, f64, f64);
                14] = [
                (
                    "Circle, Out, Center",
                    [0.0; 3],
                    [0.5, 0.5],
                    0.0,
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                ),
                (
                    "Square, In, Center",
                    [0.0; 3],
                    [0.5, 0.5],
                    5.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                ),
                (
                    "Diamond, Out, Center",
                    [0.0; 3],
                    [0.5, 0.5],
                    4.0,
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                ),
                (
                    "Rectangle, In, Center",
                    [0.0; 3],
                    [0.5, 0.5],
                    3.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                ),
                (
                    "Triangle Down, Out, Center",
                    [0.0; 3],
                    [0.5, 0.5],
                    8.0,
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                ),
                (
                    "Circle, Out, White Border",
                    [1.0; 3],
                    [0.5, 0.5],
                    0.0,
                    1.0,
                    0.0,
                    0.4,
                    1.0,
                ),
                (
                    "Square, Out, Black Border",
                    [0.0; 3],
                    [0.5, 0.5],
                    5.0,
                    1.0,
                    0.0,
                    0.1,
                    1.0,
                ),
                (
                    "Diamond, In, Feathered Edge",
                    [0.0; 3],
                    [0.5, 0.5],
                    4.0,
                    0.0,
                    0.5,
                    0.0,
                    1.0,
                ),
                (
                    "Oval, Out, Feathered Blue Border",
                    [0.0, 0.0, 1.0],
                    [0.5, 0.5],
                    1.0,
                    1.0,
                    0.5,
                    0.1,
                    1.0,
                ),
                (
                    "Triangle, Out, Bottom, Black Border",
                    [0.0; 3],
                    [0.5, 1.0],
                    6.0,
                    1.0,
                    0.0,
                    0.1,
                    1.0,
                ),
                (
                    "Four Diamonds, Out, Red Border",
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0],
                    4.0,
                    1.0,
                    0.0,
                    0.03,
                    2.0,
                ),
                (
                    "Circle, Out, Upper Left, White Border",
                    [1.0; 3],
                    [0.0, 1.0],
                    0.0,
                    1.0,
                    0.0,
                    0.1,
                    1.0,
                ),
                (
                    "Four Circles, Out, White Border",
                    [1.0; 3],
                    [0.5, 0.5],
                    0.0,
                    1.0,
                    0.0,
                    0.5,
                    2.0,
                ),
                (
                    "Nine Diamonds, In, Yellow Border",
                    [1.0, 1.0, 0.0],
                    [0.5, 0.5],
                    4.0,
                    0.0,
                    1.0,
                    0.3,
                    3.0,
                ),
            ];
            rows.into_iter()
                .map(
                    |(name, color, center, shape, dir, feather, border, repeat)| {
                        p(
                            name,
                            vec![
                                ("color", num(&color)),
                                ("center", num(&center)),
                                ("shape", num(&[shape])),
                                ("direction", num(&[dir])),
                                ("feather", num(&[feather])),
                                ("border", num(&[border])),
                                ("repeatX", num(&[repeat])),
                                ("repeatY", num(&[repeat])),
                            ],
                        )
                    },
                )
                .collect()
        }
        Kind::StarWipe => {
            // Name, arms, ratio, angle, border color, center, cycles,
            // waveform, both mirrors, direction (feather 0.2, border 0.1).
            #[allow(clippy::type_complexity)]
            let rows: [(
                &'static str,
                f64,
                f64,
                f64,
                [f64; 3],
                [f64; 2],
                f64,
                f64,
                f64,
                f64,
            ); 18] = [
                (
                    "Star",
                    5.0,
                    0.381,
                    18.0,
                    [1.0, 0.0, 1.0],
                    [0.5, 0.5],
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                ),
                (
                    "Gear",
                    16.0,
                    0.7,
                    0.0,
                    [1.0, 0.0, 1.0],
                    [0.5, 0.5],
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                ),
                (
                    "Double Gears",
                    16.0,
                    0.7,
                    0.0,
                    [1.0, 0.5, 0.0],
                    [0.5, 0.5],
                    1.0,
                    1.0,
                    0.0,
                    1.0,
                ),
                (
                    "Double Circles",
                    32.0,
                    1.0,
                    0.0,
                    [1.0, 0.5, 0.0],
                    [0.5, 0.5],
                    1.0,
                    1.0,
                    0.0,
                    1.0,
                ),
                (
                    "Rings",
                    32.0,
                    1.0,
                    0.0,
                    [1.0, 0.5, 0.0],
                    [0.5, 0.5],
                    5.0,
                    1.0,
                    0.0,
                    0.0,
                ),
                (
                    "Diamond",
                    2.0,
                    0.5,
                    0.0,
                    [0.0, 0.0, 1.0],
                    [0.5, 0.5],
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                ),
                (
                    "Hexagon",
                    6.0,
                    0.866,
                    0.0,
                    [1.0, 0.0, 0.0],
                    [0.5, 0.5],
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                ),
                (
                    "Double Squares",
                    2.0,
                    1.0,
                    45.0,
                    [1.0, 0.5, 0.5],
                    [0.5, 0.5],
                    1.0,
                    1.0,
                    0.0,
                    0.0,
                ),
                (
                    "Three Way Split",
                    3.0,
                    0.0,
                    90.0,
                    [0.0, 1.0, 1.0],
                    [0.5, 0.5],
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                ),
                (
                    "Four Way Split",
                    4.0,
                    0.0,
                    45.0,
                    [0.0, 1.0, 1.0],
                    [0.5, 0.5],
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                ),
                (
                    "Six Way Split",
                    6.0,
                    0.0,
                    30.0,
                    [0.0, 1.0, 1.0],
                    [0.5, 0.5],
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                ),
                (
                    "Sun Rays",
                    32.0,
                    0.0,
                    0.0,
                    [1.0, 1.0, 0.0],
                    [0.5, 0.5],
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                ),
                (
                    "Corner Sun Rays",
                    32.0,
                    0.1,
                    0.0,
                    [1.0, 1.0, 0.0],
                    [0.0, 1.0],
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                ),
                (
                    "Vertical Tooth Blinds",
                    2.0,
                    0.5,
                    90.0,
                    [1.0, 0.0, 0.0],
                    [0.5, 0.0],
                    3.0,
                    1.0,
                    0.0,
                    0.0,
                ),
                (
                    "Horizontal Tooth Blinds",
                    2.0,
                    0.5,
                    0.0,
                    [1.0, 0.0, 0.0],
                    [0.0, 0.5],
                    3.0,
                    1.0,
                    0.0,
                    0.0,
                ),
                (
                    "Corner Tooth",
                    2.0,
                    0.5,
                    29.357,
                    [1.0, 0.0, 0.0],
                    [0.0, 0.0],
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                ),
                (
                    "Four Diamonds",
                    2.0,
                    0.5,
                    0.0,
                    [0.5, 1.0, 0.25],
                    [0.25, 0.75],
                    1.0,
                    1.0,
                    1.0,
                    0.0,
                ),
                (
                    "Opening Eye",
                    32.0,
                    1.0,
                    0.0,
                    [1.0, 0.0, 1.0],
                    [0.5, 0.0],
                    1.0,
                    0.0,
                    1.0,
                    0.0,
                ),
            ];
            rows.into_iter()
                .map(
                    |(name, arms, ratio, angle, color, center, cycles, tooth, mirrors, reverse)| {
                        p(
                            name,
                            vec![
                                ("arms", num(&[arms])),
                                ("ratio", num(&[ratio])),
                                ("angle", num(&[angle])),
                                ("feather", num(&[0.2])),
                                ("border", num(&[0.1])),
                                ("borderColor", num(&color)),
                                ("center", num(&center)),
                                ("cycles", num(&[cycles])),
                                ("waveform", num(&[tooth])),
                                ("horizontalMirror", num(&[mirrors])),
                                ("verticalMirror", num(&[mirrors])),
                                ("direction", num(&[reverse])),
                            ],
                        )
                    },
                )
                .collect()
        }
        Kind::Flash => {
            let w = |name, spread: f64, tint: [f64; 3]| {
                p(
                    name,
                    vec![
                        ("horizontalDiffusion", num(&[spread])),
                        ("verticalDiffusion", num(&[spread])),
                        ("tint", num(&tint)),
                    ],
                )
            };
            vec![
                w("Hard Flash", 0.0, [1.0; 3]),
                w("Soft Flash", 0.5, [1.0; 3]),
                w("Yellow Flash", 0.6, [1.0, 1.0, 0.0]),
            ]
        }
        Kind::Dissolve => {
            let w = |name, t: f64, extra: Option<(&'static str, f64)>, color: [f64; 4]| {
                let mut values = vec![("type", num(&[t])), ("fadeThroughColor", num(&color))];
                if let Some((n, v)) = extra {
                    values.push((n, num(&[v])));
                }
                p(name, values)
            };
            let clear = [0.0; 4];
            vec![
                w("Additive Dissolve", 0.0, None, clear),
                w("Subtractive Dissolve", 1.0, None, clear),
                w("Subtractive Crossfade", 2.0, None, clear),
                w("Color Bleed", 3.0, None, clear),
                w(
                    "Color Bleed Fast Red",
                    3.0,
                    Some(("colorBleedSpeedRed", 1.0)),
                    clear,
                ),
                w(
                    "Color Bleed Fast Green",
                    3.0,
                    Some(("colorBleedSpeedGreen", 1.0)),
                    clear,
                ),
                w(
                    "Color Bleed Fast Blue",
                    3.0,
                    Some(("colorBleedSpeedBlue", 1.0)),
                    clear,
                ),
                w(
                    "Color Bleed Fast Alpha",
                    3.0,
                    Some(("colorBleedSpeedAlpha", 1.0)),
                    clear,
                ),
                w("Color Morph", 4.0, None, clear),
                w(
                    "Color Morph Fast Red",
                    4.0,
                    Some(("colorMorphSpeedRed", 1.0)),
                    clear,
                ),
                w(
                    "Color Morph Fast Green",
                    4.0,
                    Some(("colorMorphSpeedGreen", 1.0)),
                    clear,
                ),
                w(
                    "Color Morph Fast Blue",
                    4.0,
                    Some(("colorMorphSpeedBlue", 1.0)),
                    clear,
                ),
                w(
                    "Color Morph Fast Alpha",
                    4.0,
                    Some(("colorMorphSpeedAlpha", 1.0)),
                    clear,
                ),
                w("Threshold Dissolve", 5.0, None, clear),
                w("Threshold Appear", 6.0, None, clear),
                w("Fade Through Grayscale", 7.0, None, clear),
                w("Fade Through Black", 8.0, None, [0.0, 0.0, 0.0, 1.0]),
                w("Fade Through White", 8.0, None, [1.0; 4]),
                w("Fade Through Red", 8.0, None, [1.0, 0.0, 0.0, 1.0]),
                w(
                    "Fade Through Green",
                    8.0,
                    None,
                    [0.0, 128.0 / 255.0, 0.0, 1.0],
                ),
                w("Fade Through Blue", 8.0, None, [0.0, 0.0, 1.0, 1.0]),
                w("Fade Through Transparent", 8.0, None, clear),
                w("Red then Green then Blue Crossfade", 9.0, None, clear),
            ]
        }
        _ => {
            let w = |name, effect: f64, apply: f64, zoom: f64| {
                p(
                    name,
                    vec![
                        ("effectType", num(&[effect])),
                        ("applyTo", num(&[apply])),
                        ("fadeRange", num(&[1.0])),
                        ("maxZoom", num(&[zoom])),
                    ],
                )
            };
            vec![
                w("Cross Zoom A/B Slow", 1.0, 2.0, 4.0),
                w("Cross Zoom A/B", 1.0, 2.0, 64.0),
                w("Cross Zoom A Only", 1.0, 0.0, 64.0),
                w("Cross Zoom B Only", 1.0, 1.0, 64.0),
                w("Cross Pixelate A/B", 2.0, 2.0, 64.0),
                w("Cross Pixelate A Only", 2.0, 0.0, 64.0),
                w("Cross Pixelate B Only", 2.0, 1.0, 64.0),
                w("Cross Blur A/B", 3.0, 2.0, 64.0),
                w("Cross Blur A Only", 3.0, 0.0, 64.0),
                w("Cross Blur B Only", 3.0, 1.0, 64.0),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: usize, h: usize) -> Frame {
        Frame { w, h, par: 1.0 }
    }

    #[test]
    fn the_linear_wipe_comes_from_where_its_angle_says() {
        let f = frame(40, 20);
        for (angle, at) in [
            (0.0, (0.0, 10.0)),
            (90.0, (20.0, 0.0)),
            (180.0, (39.0, 10.0)),
            (270.0, (20.0, 19.0)),
        ] {
            let n = linear_numbers(&f, angle);
            // A fifth of the way, the second picture (share 0) where it
            // comes from, the first far from it.
            assert_eq!(linear_share(n, 0.0, 0.2, at.0, at.1), 0.0, "{angle}");
            assert_eq!(
                linear_share(n, 0.0, 0.2, 39.0 - at.0, 19.0 - at.1),
                1.0,
                "{angle}"
            );
        }
    }

    #[test]
    fn the_clock_sweeps_clockwise_from_twelve() {
        let f = frame(41, 41);
        // A quarter of the way plus the feather's share: three o'clock is
        // reached, nine o'clock not.
        let p = 0.3;
        let right = clock_share(&f, false, 0.0, p, 35.0, 20.0);
        let left = clock_share(&f, false, 0.0, p, 5.0, 20.0);
        assert!(right > 0.99 && left < 0.01, "{right} {left}");
        let left = clock_share(&f, true, 0.0, p, 5.0, 20.0);
        assert!(left > 0.99);
    }

    #[test]
    fn iris_shapes_grow_and_the_band_paints_the_border() {
        for shape in 0..14 {
            let near = iris_norm(shape, 3.0, 2.0, 1.0);
            let far = iris_norm(shape, 30.0, 20.0, 1.0);
            assert!(near < far, "{shape}: {near} {far}");
        }
        let b = iris_band(0.5, 0.5, 0.05, 0.01);
        assert_eq!(b, Band::COLOR);
        assert_eq!(iris_band(0.1, 0.5, 0.05, 0.01), Band::INNER);
        assert_eq!(iris_band(0.9, 0.5, 0.05, 0.01), Band::OUTER);
    }

    #[test]
    fn the_star_has_its_points() {
        let s = Star {
            arms: 5,
            ratio: 0.381,
            angle: 18.0,
            feather: 0.0,
            border: 0.0,
            color: [1.0, 0.0, 1.0],
            center: [0.5, 0.5],
            cycles: 1,
            tooth: false,
            flip: [false; 2],
            mirror: [false; 2],
            reverse: false,
        };
        let f = frame(101, 101);
        let n = s.frame(&f);
        // A point's tip reaches further out than the cut beside it, at the
        // same distance.
        let r = 30.0f32;
        let tip = (90.0f32 + 18.0).to_radians();
        let cut = tip + 36f32.to_radians();
        let at = |a: f32| {
            star_norm(
                &n,
                (50.0 + r * a.cos()).round() as i32,
                (50.0 + r * a.sin()).round() as i32,
            )
        };
        assert!(at(tip) < at(cut), "{} {}", at(tip), at(cut));
        assert!((0.0..=1.0).contains(&s.map(&n, &f, 10, 10)));
    }

    #[test]
    fn dissolves_start_on_the_first_picture_and_end_on_the_second() {
        let (a, b) = ([0.8, 0.2, 0.4, 1.0], [0.1, 0.6, 0.9, 1.0]);
        for kind in 0..10 {
            let start = dissolve(kind, [0.0; 4], [0.0; 4], [0.0, 0.0, 0.0, 1.0], 0.0, a, b);
            let end = dissolve(kind, [0.0; 4], [0.0; 4], [0.0, 0.0, 0.0, 1.0], 1.0, a, b);
            for c in 0..4 {
                assert!((start[c] - a[c]).abs() < 1e-6, "{kind} start {start:?}");
                assert!((end[c] - b[c]).abs() < 1e-6, "{kind} end {end:?}");
            }
        }
    }

    #[test]
    fn cross_effect_cells_cover_the_frame() {
        let (cells, of) = Cells::line(10, 4);
        assert_eq!(cells, vec![(0, 2), (2, 2), (5, 2), (7, 3)]);
        assert_eq!(of, vec![0, 0, 1, 1, 1, 2, 2, 3, 3, 3]);
        let c = Cross {
            effect: 2,
            apply: 2,
            fade_range: 1.0,
            max_zoom: 1.0,
            max: [1.0, 1.0],
            max_blur: 1.0,
            source: [0.5, 0.5],
            destination: [0.5, 0.5],
        };
        assert_eq!(c.counts(&frame(64, 36), 0.0), [64, 36]);
        assert_eq!(c.counts(&frame(64, 36), 1.0), [1, 1]);
    }
}
