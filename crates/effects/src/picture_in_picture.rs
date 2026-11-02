//! Picture In Picture: VEGAS's. The picture moved, turned, scaled and
//! sheared over a transparent frame, or pinned by its four corners.
//!
//! As VEGAS's pictures show it: `Location` is where the picture's middle
//! goes (0–1 across and up the frame), `Scale in X` and `Scale in Y` its
//! size as a part of the frame's (0.5: half; their sliders logarithmic, 1
//! in their middle), `Shear` slants it (its right side lower than its left
//! by that part of its width, in the frame's true proportions) and `Angle`
//! turns it clockwise around its middle (degrees). `Mode`: Fixed Shape
//! keeps the frame's proportions (Scale in Y follows Scale in X, no
//! shear), Fixed Parallelogram scales each way and shears, Free Form puts
//! the picture's corners at `Corner points` (0–1 across and up). The two
//! descriptions stay together, as in VEGAS: in the fixed modes the corners
//! follow the picture (greyed out); in Free Form the location becomes the
//! corners' mean and the angle, scales and shear the parallelogram nearest
//! them, and changing those moves the corners with them. A corner dragged
//! on the preview stops where the shape would fold (a triangle). `Opacity`
//! fades the picture; what it does not cover is transparent. Each pixel
//! averages a few samples of the picture (more as it shrinks), each from
//! its four nearest pixels.
//!
//! On the preview (an overlay drawn with the draw suite): the picture's
//! outline, a handle on each corner — the picture scaled about its middle,
//! proportionally (Fixed Shape) or each way (Fixed Parallelogram), or that
//! corner moved (Free Form) —, the ring around its middle whose handle
//! turns and scales it, and its middle: dragging inside moves it.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Draw, Fx, Image, Props, for_rows, interact_props, is, redraw, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.PictureInPicture";

const LOCATION: &CStr = c"location";
const ANGLE: &CStr = c"angle";
const SCALE_X: &CStr = c"scaleX";
const SCALE_Y: &CStr = c"scaleY";
const MODE: &CStr = c"mode";
const SHEAR: &CStr = c"shear";
const OPACITY: &CStr = c"opacity";
const CORNER_POINTS: &CStr = c"cornerPoints";
const TOP_LEFT: &CStr = c"topLeft";
const TOP_RIGHT: &CStr = c"topRight";
const BOTTOM_LEFT: &CStr = c"bottomLeft";
const BOTTOM_RIGHT: &CStr = c"bottomRight";

/// The corners' parameters, in VEGAS's order.
const CORNERS: [&CStr; 4] = [TOP_LEFT, TOP_RIGHT, BOTTOM_LEFT, BOTTOM_RIGHT];

/// The picture's own corners (0–1 across and up), in [`CORNERS`]' order.
const OWN_CORNERS: [[f64; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]];

/// The corners at first: the default picture's (half the frame, in its
/// middle), as VEGAS has them.
const DEFAULT_CORNERS: [[f64; 2]; 4] = [[0.25, 0.75], [0.75, 0.75], [0.25, 0.25], [0.75, 0.25]];

/// The scales' sliders: logarithmic from a tenth to ten times the frame.
const SCALE_SLIDER: [f64; 2] = [0.1, 10.0];

/// How the picture keeps its shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    FixedShape,
    FixedParallelogram,
    FreeForm,
}

/// The modes, in the order of the parameter's options.
const MODES: [(&CStr, Mode); 3] = [
    (c"Fixed Shape", Mode::FixedShape),
    (c"Fixed Parallelogram", Mode::FixedParallelogram),
    (c"Free Form", Mode::FreeForm),
];

impl Mode {
    fn of(i: c_int) -> Mode {
        MODES
            .get(i.max(0) as usize)
            .map_or(Mode::FixedShape, |m| m.1)
    }
}

/// A 3 × 3 matrix, rows: points as (x, y, 1).
type M3 = [[f64; 3]; 3];

fn mul(a: &M3, b: &M3) -> M3 {
    std::array::from_fn(|i| {
        std::array::from_fn(|j| a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j])
    })
}

fn inverse(m: &M3) -> Option<M3> {
    let c = |r: usize, k: usize| {
        let (r1, r2) = ((r + 1) % 3, (r + 2) % 3);
        let (k1, k2) = ((k + 1) % 3, (k + 2) % 3);
        m[r1][k1] * m[r2][k2] - m[r1][k2] * m[r2][k1]
    };
    let det = m[0][0] * c(0, 0) + m[0][1] * c(0, 1) + m[0][2] * c(0, 2);
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    Some(std::array::from_fn(|i| {
        std::array::from_fn(|j| c(j, i) / det)
    }))
}

/// Where `m` puts the point `p` (none past the horizon).
fn apply(m: &M3, p: [f64; 2]) -> Option<[f64; 2]> {
    let w = m[2][0] * p[0] + m[2][1] * p[1] + m[2][2];
    (w > 1e-12).then(|| {
        [
            (m[0][0] * p[0] + m[0][1] * p[1] + m[0][2]) / w,
            (m[1][0] * p[0] + m[1][1] * p[1] + m[1][2]) / w,
        ]
    })
}

fn translation(x: f64, y: f64) -> M3 {
    [[1.0, 0.0, x], [0.0, 1.0, y], [0.0, 0.0, 1.0]]
}

fn scaling(x: f64, y: f64) -> M3 {
    [[x, 0.0, 0.0], [0.0, y, 0.0], [0.0, 0.0, 1.0]]
}

/// A clockwise turn of `degrees`.
fn clockwise(degrees: f64) -> M3 {
    let r = degrees.to_radians();
    let (c, s) = (r.cos(), r.sin());
    [[c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0]]
}

/// The map of the unit square (0–1 across and up) onto the quadrilateral
/// `q` (its corners in [`CORNERS`]' order); none when it is flat.
fn square_to(q: [[f64; 2]; 4]) -> Option<M3> {
    let [tl, tr, bl, br] = q;
    let (p0, p1, p2, p3) = (bl, br, tr, tl);
    let (dx1, dy1) = (p1[0] - p2[0], p1[1] - p2[1]);
    let (dx2, dy2) = (p3[0] - p2[0], p3[1] - p2[1]);
    let (dx3, dy3) = (p0[0] - p1[0] + p2[0] - p3[0], p0[1] - p1[1] + p2[1] - p3[1]);
    let (g, h) = if dx3.abs() < 1e-12 && dy3.abs() < 1e-12 {
        (0.0, 0.0)
    } else {
        let den = dx1 * dy2 - dx2 * dy1;
        if den.abs() < 1e-12 {
            return None;
        }
        ((dx3 * dy2 - dx2 * dy3) / den, (dx1 * dy3 - dx3 * dy1) / den)
    };
    let m = [
        [p1[0] - p0[0] + g * p1[0], p3[0] - p0[0] + h * p3[0], p0[0]],
        [p1[1] - p0[1] + g * p1[1], p3[1] - p0[1] + h * p3[1], p0[1]],
        [g, h, 1.0],
    ];
    inverse(&m).map(|_| m)
}

/// Whether the corners (in [`CORNERS`]' order) go round a convex shape, a
/// triangle's sides at the least: no corner folded in.
fn convex(q: &[[f64; 2]; 4]) -> bool {
    let ring = [q[0], q[1], q[3], q[2]];
    let mut turn = 0.0f64;
    for i in 0..4 {
        let (a, b, c) = (ring[i], ring[(i + 1) % 4], ring[(i + 2) % 4]);
        let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
        if cross.abs() <= 1e-6 {
            // On the line between its neighbours: as far as it goes.
            if (b[0] - a[0]).hypot(b[1] - a[1]) > 1e-9 {
                return false;
            }
            continue;
        }
        if turn == 0.0 {
            turn = cross.signum();
        } else if cross.signum() != turn {
            return false;
        }
    }
    turn != 0.0
}

/// The corners with corner `i` moved toward `to`, as far as the shape
/// stays convex: VEGAS stops it on the line between its neighbours (a
/// triangle) rather than fold it in.
fn pinned(q: [[f64; 2]; 4], i: usize, to: [f64; 2]) -> [[f64; 2]; 4] {
    let with = |p: [f64; 2]| {
        let mut n = q;
        n[i] = p;
        n
    };
    if convex(&with(to)) || !convex(&q) {
        return with(to);
    }
    let from = q[i];
    let at = |t: f64| {
        [
            from[0] + (to[0] - from[0]) * t,
            from[1] + (to[1] - from[1]) * t,
        ]
    };
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..48 {
        let mid = (lo + hi) / 2.0;
        if convex(&with(at(mid))) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    with(at(lo))
}

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    /// Where the picture's middle is: 0–1 across and up the frame.
    pub location: [f64; 2],
    /// Degrees, clockwise.
    pub angle: f64,
    /// Its size across and up, parts of the frame's.
    pub scale: [f64; 2],
    pub mode: Mode,
    pub shear: f64,
    pub opacity: f64,
    /// Its corners, 0–1 across and up, in [`CORNERS`]' order: Free Form's
    /// own, the picture's in the other modes.
    pub corners: [[f64; 2]; 4],
}

/// What the frame's pixels go through: the map from the frame (0–1 across
/// and up) back into the picture, and how many samples each pixel
/// averages across and up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Warp {
    pub inverse: M3,
    pub samples: u32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            location: [0.5; 2],
            angle: 0.0,
            scale: [0.5; 2],
            mode: Mode::FixedShape,
            shear: 0.0,
            opacity: 1.0,
            corners: DEFAULT_CORNERS,
        }
    }
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        let d = Settings::default();
        Settings {
            location: v.array(LOCATION, d.location),
            angle: v.f64(ANGLE, 0.0),
            scale: [v.f64(SCALE_X, 0.5), v.f64(SCALE_Y, 0.5)],
            mode: Mode::of(v.int(MODE, 0)),
            shear: v.f64(SHEAR, 0.0),
            opacity: v.f64(OPACITY, 1.0).clamp(0.0, 1.0),
            corners: std::array::from_fn(|i| v.array(CORNERS[i], DEFAULT_CORNERS[i])),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            location: fx.param(LOCATION).xy_at(time),
            angle: fx.param(ANGLE).f64_at(time),
            scale: [
                fx.param(SCALE_X).f64_at(time),
                fx.param(SCALE_Y).f64_at(time),
            ],
            mode: Mode::of(fx.param(MODE).int_at(time)),
            shear: fx.param(SHEAR).f64_at(time),
            opacity: fx.param(OPACITY).f64_at(time).clamp(0.0, 1.0),
            corners: std::array::from_fn(|i| fx.param(CORNERS[i]).xy_at(time)),
        }
    }

    /// The scales and shear that count: Fixed Shape's height follows its
    /// width, with no shear.
    fn shape(&self) -> ([f64; 2], f64) {
        match self.mode {
            Mode::FixedShape => ([self.scale[0]; 2], 0.0),
            _ => (self.scale, self.shear),
        }
    }

    /// The parallelogram the location, angle, scales and shear make: the
    /// picture's map onto a frame `aspect` wide for 1 high (both 0–1 across
    /// and up) — its middle to the origin, scaled, sheared (the right side
    /// down), turned clockwise, all in the frame's true proportions, then
    /// moved to the location.
    fn placement(&self, aspect: f64) -> M3 {
        let (scale, shear) = self.shape();
        let slant = [[1.0, 0.0, 0.0], [-shear, 1.0, 0.0], [0.0, 0.0, 1.0]];
        [
            scaling(1.0 / aspect, 1.0),
            translation(self.location[0] * aspect, self.location[1]),
            clockwise(self.angle),
            slant,
            scaling(scale[0] * aspect, scale[1]),
            translation(-0.5, -0.5),
        ]
        .iter()
        .fold(scaling(1.0, 1.0), |m, x| mul(&m, x))
    }

    /// The parallelogram's corners, in [`CORNERS`]' order.
    fn placed_corners(&self, aspect: f64) -> [[f64; 2]; 4] {
        let m = self.placement(aspect);
        OWN_CORNERS.map(|c| apply(&m, c).unwrap_or(c))
    }

    /// Where the picture's corners are on the frame (0–1 across and up),
    /// in [`CORNERS`]' order.
    pub(crate) fn quad(&self, aspect: f64) -> [[f64; 2]; 4] {
        match self.mode {
            Mode::FreeForm => self.corners,
            _ => self.placed_corners(aspect),
        }
    }

    /// The map of the picture (0–1 across and up) onto the frame: none
    /// when it is flat.
    pub(crate) fn map(&self, aspect: f64) -> Option<M3> {
        match self.mode {
            Mode::FreeForm => square_to(self.corners),
            _ => {
                let m = self.placement(aspect);
                inverse(&m).map(|_| m)
            }
        }
    }

    /// The settings with the location, angle, scales and shear of the
    /// parallelogram nearest the corners, as VEGAS shows them in Free
    /// Form: its middle the corners' mean, its sides the means of the
    /// opposite edges.
    pub(crate) fn fitted(&self, aspect: f64) -> Settings {
        let q = self.corners.map(|c| [c[0] * aspect, c[1]]);
        let [tl, tr, bl, br] = q;
        let across = [
            (tr[0] + br[0] - tl[0] - bl[0]) / 2.0,
            (tr[1] + br[1] - tl[1] - bl[1]) / 2.0,
        ];
        let up = [
            (tl[0] + tr[0] - bl[0] - br[0]) / 2.0,
            (tl[1] + tr[1] - bl[1] - br[1]) / 2.0,
        ];
        let mut s = *self;
        s.location = [
            self.corners.iter().map(|c| c[0]).sum::<f64>() / 4.0,
            self.corners.iter().map(|c| c[1]).sum::<f64>() / 4.0,
        ];
        let height = up[0].hypot(up[1]);
        if height < 1e-12 {
            return s;
        }
        // Up turned clockwise by the angle; across turned back is the
        // width and the shear.
        let angle = up[0].atan2(up[1]);
        let (c, sn) = (angle.cos(), angle.sin());
        let back = [
            c * across[0] - sn * across[1],
            sn * across[0] + c * across[1],
        ];
        s.angle = angle.to_degrees();
        s.scale = [back[0] / aspect, height];
        s.shear = if back[0].abs() > 1e-12 {
            -back[1] / back[0]
        } else {
            0.0
        };
        s
    }

    /// What the frame's pixels go through (none: nothing of the picture
    /// shows).
    pub(crate) fn warp(&self, aspect: f64) -> Option<Warp> {
        let m = self.map(aspect)?;
        let mut inverse = inverse(&m)?;
        // Its sign: the picture's points ahead (w above 0), as its middle.
        let middle = apply(&m, [0.5, 0.5])?;
        if inverse[2][0] * middle[0] + inverse[2][1] * middle[1] + inverse[2][2] < 0.0 {
            inverse = inverse.map(|r| r.map(|v| -v));
        }
        // Samples enough for how much the picture shrinks: its area's part
        // of the frame's.
        let [tl, tr, bl, br] = self.quad(aspect);
        let ring = [tl, tr, br, bl];
        let area = (0..4)
            .map(|i| {
                let (a, b) = (ring[i], ring[(i + 1) % 4]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>()
            .abs()
            / 2.0;
        let shrink = area.sqrt().max(1e-6);
        let samples = (1.0 / shrink).ceil().clamp(2.0, 4.0) as u32;
        Some(Warp { inverse, samples })
    }

    fn changes_nothing(&self, aspect: f64) -> bool {
        self.opacity >= 1.0
            && self.map(aspect).is_some_and(|m| {
                (0..3).all(|i| (0..3).all(|j| (m[i][j] - f64::from(u8::from(i == j))).abs() < 1e-9))
            })
    }

    /// The settings with the picture moved by `m` (a map of the frame, 0–1
    /// across and up): Free Form's corners, the location, angle and the
    /// rest following them (`aspect`: the frame's proportions).
    fn moved(&self, m: &M3, aspect: f64) -> Settings {
        let mut s = *self;
        s.corners = self.quad(aspect).map(|c| apply(m, c).unwrap_or(c));
        s.mode = Mode::FreeForm;
        let mut f = s.fitted(aspect);
        f.mode = self.mode;
        if self.mode != Mode::FreeForm {
            f.corners = f.placed_corners(aspect);
        }
        f
    }
}

/// Writes the settings that differ from `was`.
fn write(fx: Fx, was: &Settings, now: &Settings) {
    let differs = |a: f64, b: f64| (a - b).abs() > 1e-12;
    if differs(was.location[0], now.location[0]) || differs(was.location[1], now.location[1]) {
        fx.param(LOCATION).set_xy(now.location);
    }
    if differs(was.angle, now.angle) {
        fx.param(ANGLE).set_f64(now.angle);
    }
    if differs(was.scale[0], now.scale[0]) {
        fx.param(SCALE_X).set_f64(now.scale[0]);
    }
    if differs(was.scale[1], now.scale[1]) {
        fx.param(SCALE_Y).set_f64(now.scale[1]);
    }
    if differs(was.shear, now.shear) {
        fx.param(SHEAR).set_f64(now.shear);
    }
    for ((name, a), b) in CORNERS.into_iter().zip(was.corners).zip(now.corners) {
        if differs(a[0], b[0]) || differs(a[1], b[1]) {
            fx.param(name).set_xy(b);
        }
    }
}

/// Keeps the picture's two descriptions together after `changed` changed
/// (`by_hand`: the user changed it), as VEGAS does: Fixed Shape's height
/// follows its width; in the fixed modes the corners follow the picture;
/// in Free Form the location, angle, scales and shear follow the corners,
/// and those changed by hand move the corners with them.
fn keep_together(fx: Fx, time: f64, changed: impl Fn(&CStr) -> bool, by_hand: bool) {
    let was = Settings::at(fx, time);
    let aspect = aspect_of(fx.frame().1);
    let placed = [LOCATION, ANGLE, SCALE_X, SCALE_Y, SHEAR, MODE]
        .into_iter()
        .any(&changed);
    let mut now = was;
    match was.mode {
        Mode::FreeForm if by_hand => {
            if CORNERS.into_iter().any(&changed) {
                now = was.fitted(aspect);
            } else if placed && !changed(MODE) {
                // From the parallelogram the corners made to the one the
                // settings make now.
                let fit = was.fitted(aspect);
                let from = Settings {
                    mode: Mode::FixedParallelogram,
                    ..fit
                };
                let to = Settings {
                    mode: Mode::FixedParallelogram,
                    ..was
                };
                if let Some(back) = inverse(&from.placement(aspect)) {
                    let m = mul(&to.placement(aspect), &back);
                    now.corners = was.corners.map(|c| apply(&m, c).unwrap_or(c));
                }
            }
        }
        Mode::FreeForm => {}
        _ => {
            if by_hand && was.mode == Mode::FixedShape && (changed(SCALE_X) || changed(MODE)) {
                now.scale[1] = now.scale[0];
            }
            if placed {
                now.corners = now.placed_corners(aspect);
            }
        }
    }
    write(fx, &was, &now);
}

/// Greys out what the mode turns off, as VEGAS does.
fn enable(fx: Fx, time: f64) {
    let mode = Settings::at(fx, time).mode;
    fx.param(SCALE_Y).set_enabled(mode != Mode::FixedShape);
    fx.param(SHEAR).set_enabled(mode != Mode::FixedShape);
    for c in CORNERS {
        fx.param(c).set_enabled(mode == Mode::FreeForm);
    }
}

/// The frame's proportions (canonical units).
fn aspect_of(size: [f64; 2]) -> f64 {
    size[0] / size[1].max(1e-9)
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
        render(fx, args)
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        let s = Settings::at(fx, args.dbl(kOfxPropTime, 0));
        if s.changes_nothing(aspect_of(fx.frame().1)) {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // Any pixel may come from anywhere in the picture.
        let time = args.dbl(kOfxPropTime, 0);
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        let roi = fx
            .clip(kOfxImageEffectSimpleSourceClipName)
            .rod(time)
            .unwrap_or(roi);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &roi);
        kOfxStatOK
    } else if is(action, kOfxActionInstanceChanged) {
        let time = args.dbl(kOfxPropTime, 0);
        if args.is(kOfxPropType, kOfxTypeParameter) {
            let by_hand = args.is(kOfxPropChangeReason, kOfxChangeUserEdited);
            keep_together(fx, time, |n| args.is(kOfxPropName, n), by_hand);
        }
        enable(fx, time);
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) || is(action, kOfxActionBeginInstanceEdit) {
        enable(fx, 0.0);
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

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Picture In Picture");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Utility");
    p.set_str(
        kOfxPropPluginDescription,
        c"Moves, turns and scales the picture over a transparent frame, or pins it by its corners.",
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
    p.set_ptr(
        kOfxImageEffectPluginPropOverlayInteractV2,
        overlay as *mut c_void,
    );
}

fn describe_in_context(fx: Fx) {
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);

    let position = |name: &CStr, label: &CStr, default: [f64; 2], hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble2D, name, label);
        p.set_str(
            kOfxParamPropDoubleType,
            kOfxParamDoubleTypeNormalisedXYAbsolute,
        );
        p.set_dbls(kOfxParamPropDefault, &default);
        p.set_dbls(kOfxParamPropIncrement, &[0.01]);
        p.set_int(kOfxParamPropDigits, 3);
        p.set_str(kOfxParamPropHint, hint);
        p
    };
    // 0–1 across and up, as VEGAS gives it (and its presets).
    position(
        LOCATION,
        c"Location",
        [0.5, 0.5],
        c"Where the picture's middle is: 0.5, 0.5 is the middle of the frame.",
    );

    let p = fx.define_param(kOfxParamTypeDouble, ANGLE, c"Angle");
    p.set_str(kOfxParamPropDoubleType, kOfxParamDoubleTypeAngle);
    p.set_dbl(kOfxParamPropDefault, 0.0);
    p.set_dbl(kOfxParamPropMin, -36_000.0);
    p.set_dbl(kOfxParamPropMax, 36_000.0);
    p.set_dbl(kOfxParamPropDisplayMin, -180.0);
    p.set_dbl(kOfxParamPropDisplayMax, 180.0);
    p.set_dbl(kOfxParamPropIncrement, 1.0);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"Turns the picture clockwise around its middle, in degrees.",
    );

    let scale = |name: &CStr, label: &CStr, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, 0.5);
        p.set_dbl(kOfxParamPropMin, 0.0);
        p.set_dbl(kOfxParamPropMax, 100.0);
        p.set_dbl(kOfxParamPropDisplayMin, SCALE_SLIDER[0]);
        p.set_dbl(kOfxParamPropDisplayMax, SCALE_SLIDER[1]);
        p.set_dbl(kOfxParamPropIncrement, 0.01);
        p.set_int(kOfxParamPropDigits, 3);
        // VEGAS's sliders: 1 in the middle (BananaV's extension).
        p.set_int(bv_ofx_sys::extensions::kBananaVParamPropLogarithmic, 1);
        p.set_str(kOfxParamPropHint, hint);
    };
    scale(
        SCALE_X,
        c"Scale in X",
        c"The picture's width, a part of the frame's (1: the frame's).",
    );
    scale(
        SCALE_Y,
        c"Scale in Y",
        c"The picture's height, a part of the frame's (1: the frame's).",
    );

    let p = fx.define_param(kOfxParamTypeChoice, MODE, c"Mode");
    let names: Vec<&CStr> = MODES.iter().map(|(n, _)| *n).collect();
    p.set_strs(kOfxParamPropChoiceOption, &names);
    p.set_int(kOfxParamPropDefault, 0);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(
        kOfxParamPropHint,
        c"Fixed Shape keeps the frame's proportions, Fixed Parallelogram scales each way and shears, Free Form pins each corner.",
    );

    let p = fx.define_param(kOfxParamTypeDouble, SHEAR, c"Shear");
    p.set_dbl(kOfxParamPropDefault, 0.0);
    p.set_dbl(kOfxParamPropMin, -10.0);
    p.set_dbl(kOfxParamPropMax, 10.0);
    p.set_dbl(kOfxParamPropDisplayMin, -1.0);
    p.set_dbl(kOfxParamPropDisplayMax, 1.0);
    p.set_dbl(kOfxParamPropIncrement, 0.01);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"Slants the picture: its right side lower than its left by this part of its width.",
    );

    let p = fx.define_param(kOfxParamTypeDouble, OPACITY, c"Opacity");
    p.set_dbl(kOfxParamPropDefault, 1.0);
    p.set_dbl(kOfxParamPropMin, 0.0);
    p.set_dbl(kOfxParamPropMax, 1.0);
    p.set_dbl(kOfxParamPropDisplayMin, 0.0);
    p.set_dbl(kOfxParamPropDisplayMax, 1.0);
    p.set_dbl(kOfxParamPropIncrement, 0.01);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"How much the picture shows: 1 all of it, 0 nothing.",
    );

    // VEGAS's section, closed at first.
    let p = fx.define_param(kOfxParamTypeGroup, CORNER_POINTS, c"Corner points");
    p.set_int(kOfxParamPropGroupOpen, 0);
    let labels = [c"Top left", c"Top right", c"Bottom left", c"Bottom right"];
    for i in 0..4 {
        let p = position(
            CORNERS[i],
            labels[i],
            DEFAULT_CORNERS[i],
            c"Where this corner of the picture is, 0–1 across and up the frame: set in Free Form, following the picture in the other modes.",
        );
        p.set_str(kOfxParamPropParent, CORNER_POINTS);
    }
}

/// The source's color at `p` (pixels, their middles at .5), premultiplied,
/// from its four nearest pixels; nothing outside it.
pub(crate) fn sample(src: &Image, premultiplied: bool, p: [f64; 2]) -> [f32; 4] {
    let (x, y) = (p[0] - 0.5, p[1] - 0.5);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
    let (x0, y0) = (x0 as c_int, y0 as c_int);
    let at = |xi: c_int, yi: c_int| match crate::colors::pixel(src, xi, yi) {
        Some(px) if premultiplied => px,
        Some(px) => [px[0] * px[3], px[1] * px[3], px[2] * px[3], px[3]],
        None => [0.0; 4],
    };
    let (a, b, c, d) = (
        at(x0, y0),
        at(x0 + 1, y0),
        at(x0, y0 + 1),
        at(x0 + 1, y0 + 1),
    );
    std::array::from_fn(|i| {
        (a[i] * (1.0 - fx) + b[i] * fx) * (1.0 - fy) + (c[i] * (1.0 - fx) + d[i] * fx) * fy
    })
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|s| if s > 0.0 { s } else { 1.0 });
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    if let Some(s) = &src
        && s.depth != out.depth
    {
        return kOfxStatErrImageFormat;
    }
    let s = Settings::at(fx, time);
    let (origin, size) = fx.frame();
    let warp = s.warp(aspect_of(size));
    let par = out.par;
    let src_premultiplied = src
        .as_ref()
        .is_some_and(|i| i.channels < 4 || !i.unpremultiplied);
    let out_straight = out.channels == 4 && out.unpremultiplied;
    let opacity = s.opacity as f32;
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(row) = out.row(y) else { return };
        for x in columns.clone() {
            let mut v = [0.0f32; 4];
            if let (Some(w), Some(src)) = (&warp, &src) {
                let n = w.samples;
                for j in 0..n {
                    for i in 0..n {
                        let ox = x as f64 + (i as f64 + 0.5) / n as f64;
                        let oy = y as f64 + (j as f64 + 0.5) / n as f64;
                        let p = [
                            (ox * par / scale[0] - origin[0]) / size[0],
                            (oy / scale[1] - origin[1]) / size[1],
                        ];
                        let Some(m) = apply(&w.inverse, p) else {
                            continue;
                        };
                        let at = [
                            (origin[0] + m[0] * size[0]) * scale[0] / par,
                            (origin[1] + m[1] * size[1]) * scale[1],
                        ];
                        let c = sample(src, src_premultiplied, at);
                        for (a, b) in v.iter_mut().zip(c) {
                            *a += b;
                        }
                    }
                }
                let k = opacity / (n * n) as f32;
                v = v.map(|c| c * k);
            }
            if out_straight && v[3] > 0.0 {
                v = [v[0] / v[3], v[1] / v[3], v[2] / v[3], v[3]];
            }
            // SAFETY: a row the output has, a column inside its bounds.
            unsafe { crate::colors::put_pixel(&out, row, x, v) };
        }
    };
    for_rows(y1.max(out.bounds[1])..y2.min(out.bounds[3]), &fill);
    drop(src);
    drop(out);
    kOfxStatOK
}

// The overlay.

/// What the pen is doing with the picture, and the settings when it began.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// Moving it: where the pen went down (0–1 across and up).
    Move { from: [f64; 2], start: Settings },
    /// Turning and scaling it by the ring's handle: the pen's first
    /// direction from the middle (degrees, clockwise from up) and distance
    /// (canonical units).
    Ring {
        angle: f64,
        distance: f64,
        start: Settings,
    },
    /// A corner.
    Corner { corner: usize, start: Settings },
}

/// A part of the overlay under the pen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Corner(usize),
    Ring,
    Inside,
}

/// An overlay's own state.
#[derive(Debug, Default)]
struct Overlay {
    drag: Option<Drag>,
    hover: Option<Part>,
}

const RIM: [f32; 4] = [0.0, 0.0, 0.0, 0.6];
const LINE: [f32; 4] = [0.85, 0.85, 0.85, 0.9];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// The handles' size and how near the pen must be, in screen pixels.
const HANDLE: f64 = 5.0;
const REACH: f64 = 8.0;

/// The ring's radius, a part of the frame's width.
const RING_RADIUS: f64 = 0.07;

/// A frame's points: canonical units and 0–1 across and up.
#[derive(Clone, Copy, Debug)]
struct Frame {
    origin: [f64; 2],
    size: [f64; 2],
}

impl Frame {
    fn canonical(&self, p: [f64; 2]) -> [f64; 2] {
        [
            self.origin[0] + p[0] * self.size[0],
            self.origin[1] + p[1] * self.size[1],
        ]
    }

    fn unit(&self, p: [f64; 2]) -> [f64; 2] {
        [
            (p[0] - self.origin[0]) / self.size[0],
            (p[1] - self.origin[1]) / self.size[1],
        ]
    }

    fn aspect(&self) -> f64 {
        aspect_of(self.size)
    }
}

/// The ring's handle: up from the middle, turned with the picture.
fn ring_handle(s: &Settings, f: &Frame) -> [f64; 2] {
    let c = f.canonical(s.location);
    let r = RING_RADIUS * f.size[0];
    let a = s.angle.to_radians();
    [c[0] + r * a.sin(), c[1] + r * a.cos()]
}

/// The direction of `d` from up, clockwise, in degrees.
fn angle_from_up(d: [f64; 2]) -> f64 {
    d[0].atan2(d[1]).to_degrees()
}

/// `a` within ±180°.
fn wrap(a: f64) -> f64 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

/// A circle (an ellipse in canonical units) of `n` points.
fn circle(c: [f64; 2], r: [f64; 2], n: usize) -> Vec<[f64; 2]> {
    (0..n)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / n as f64;
            [c[0] + r[0] * t.cos(), c[1] + r[1] * t.sin()]
        })
        .collect()
}

/// Whether `p` is inside the quadrilateral `q` (corners around it).
fn inside(q: &[[f64; 2]; 4], p: [f64; 2]) -> bool {
    let mut odd = false;
    for i in 0..4 {
        let (a, b) = (q[i], q[(i + 1) % 4]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                odd = !odd;
            }
        }
    }
    odd
}

/// The picture's corners in canonical units, in [`CORNERS`]' order.
fn corners(s: &Settings, f: &Frame) -> [[f64; 2]; 4] {
    s.quad(f.aspect()).map(|c| f.canonical(c))
}

/// What the pen at `p` is over; `ps` is a screen pixel in canonical units.
fn hit(s: &Settings, f: &Frame, p: [f64; 2], ps: [f64; 2]) -> Option<Part> {
    let dist = |a: [f64; 2]| ((p[0] - a[0]) / ps[0]).hypot((p[1] - a[1]) / ps[1]);
    if dist(ring_handle(s, f)) <= REACH {
        return Some(Part::Ring);
    }
    let q = corners(s, f);
    let nearest = q
        .iter()
        .enumerate()
        .map(|(i, &c)| (dist(c), i))
        .filter(|(d, _)| *d <= REACH)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, i)) = nearest {
        return Some(Part::Corner(i));
    }
    let [tl, tr, bl, br] = q;
    (dist(f.canonical(s.location)) <= REACH || inside(&[tl, tr, br, bl], p)).then_some(Part::Inside)
}

/// A map of the frame (0–1 across and up) turning by `degrees` clockwise
/// and scaling by `k` around `middle`, in the frame's true proportions.
fn turn_about(middle: [f64; 2], degrees: f64, k: f64, aspect: f64) -> M3 {
    [
        scaling(1.0 / aspect, 1.0),
        translation(middle[0] * aspect, middle[1]),
        clockwise(degrees),
        scaling(k, k),
        translation(-middle[0] * aspect, -middle[1]),
        scaling(aspect, 1.0),
    ]
    .iter()
    .fold(scaling(1.0, 1.0), |m, x| mul(&m, x))
}

/// The settings once the pen is at `pen` (canonical units) in `drag`.
fn dragged(drag: &Drag, f: &Frame, pen: [f64; 2]) -> Settings {
    let a = f.aspect();
    match *drag {
        Drag::Move { from, start } => {
            let p = f.unit(pen);
            let d = [p[0] - from[0], p[1] - from[1]];
            if start.mode == Mode::FreeForm {
                start.moved(&translation(d[0], d[1]), a)
            } else {
                let mut s = start;
                s.location = [start.location[0] + d[0], start.location[1] + d[1]];
                s.corners = s.placed_corners(a);
                s
            }
        }
        Drag::Ring {
            angle,
            distance,
            start,
        } => {
            let middle = f.canonical(start.location);
            let d = [pen[0] - middle[0], pen[1] - middle[1]];
            let turn = wrap(angle_from_up(d) - angle);
            let k = if distance > 1e-9 {
                (d[0].hypot(d[1]) / distance).max(1e-3)
            } else {
                1.0
            };
            if start.mode == Mode::FreeForm {
                start.moved(&turn_about(start.location, turn, k, a), a)
            } else {
                let mut s = start;
                s.angle = start.angle + turn;
                s.scale = start.scale.map(|v| v * k);
                s.corners = s.placed_corners(a);
                s
            }
        }
        Drag::Corner { corner, start } => {
            let mut s = start;
            let middle = f.canonical(start.location);
            match start.mode {
                Mode::FixedShape => {
                    let from = corners(&start, f)[corner];
                    let d0 = (from[0] - middle[0]).hypot(from[1] - middle[1]);
                    let d1 = (pen[0] - middle[0]).hypot(pen[1] - middle[1]);
                    if d0 > 1e-9 {
                        let k = (start.scale[0] * d1 / d0).max(1e-3);
                        s.scale = [k, k];
                    }
                }
                Mode::FixedParallelogram => {
                    // The pen in the picture's own frame, from its middle, in
                    // frame heights: turned back, its width then its height.
                    let r = start.angle.to_radians();
                    let (c, sn) = (r.cos(), r.sin());
                    let d = [
                        (pen[0] - middle[0]) / f.size[1],
                        (pen[1] - middle[1]) / f.size[1],
                    ];
                    let p = [c * d[0] - sn * d[1], sn * d[0] + c * d[1]];
                    let u = [OWN_CORNERS[corner][0] - 0.5, OWN_CORNERS[corner][1] - 0.5];
                    let wide = p[0] / u[0];
                    if wide > 1e-3 {
                        s.scale[0] = wide / a;
                        let high = (p[1] + start.shear * wide * u[0]) / u[1];
                        if high > 1e-3 {
                            s.scale[1] = high;
                        }
                    }
                }
                Mode::FreeForm => {
                    s.corners = pinned(start.corners, corner, f.unit(pen));
                    return s.fitted(a);
                }
            }
            s.corners = s.placed_corners(a);
            s
        }
    }
}

/// The overlay's entry point.
unsafe extern "C" fn overlay(
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
        let state = Box::into_raw(Box::new(Overlay::default()));
        props.set_ptr(kOfxPropInstanceData, state as *mut c_void);
        return kOfxStatOK;
    }
    let state = props.ptr(kOfxPropInstanceData) as *mut Overlay;
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
    let f = Frame { origin, size };
    let s = Settings::at(fx, time);
    if is(action, kOfxInteractActionDraw) {
        draw(args, state, &s, &f, ps);
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    if is(action, kOfxInteractActionPenDown) {
        let Some(part) = hit(&s, &f, pen, ps) else {
            return kOfxStatReplyDefault;
        };
        let middle = f.canonical(s.location);
        let d = [pen[0] - middle[0], pen[1] - middle[1]];
        state.drag = Some(match part {
            Part::Ring => Drag::Ring {
                angle: angle_from_up(d),
                distance: d[0].hypot(d[1]),
                start: s,
            },
            Part::Corner(corner) => Drag::Corner { corner, start: s },
            Part::Inside => Drag::Move {
                from: f.unit(pen),
                start: s,
            },
        });
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        let Some(drag) = state.drag else {
            let hover = hit(&s, &f, pen, ps);
            if hover != state.hover {
                state.hover = hover;
                redraw(handle);
            }
            return kOfxStatReplyDefault;
        };
        write(fx, &s, &dragged(&drag, &f, pen));
        kOfxStatOK
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

fn draw(args: Props, state: &Overlay, s: &Settings, f: &Frame, ps: [f64; 2]) {
    let Some(d) = Draw::of(args) else { return };
    let dot = |c: [f64; 2], r: f64| circle(c, [r * ps[0], r * ps[1]], 20);
    let [tl, tr, bl, br] = corners(s, f);
    // The picture's outline.
    d.colour(LINE);
    d.width(1.0);
    d.outline(&[tl, tr, br, bl]);
    // The ring around the middle, its handle up.
    let middle = f.canonical(s.location);
    let radius = RING_RADIUS * f.size[0];
    d.outline(&circle(middle, [radius, radius], 64));
    let held = matches!(state.drag, Some(Drag::Ring { .. })) || state.hover == Some(Part::Ring);
    let handle = ring_handle(s, f);
    let r = if held { 5.5 } else { 4.0 };
    d.colour(WHITE);
    d.fill(&dot(handle, r));
    d.colour(RIM);
    d.outline(&dot(handle, r));
    // Its middle: a black dot in a white ring.
    d.colour(WHITE);
    d.fill(&dot(middle, 4.0));
    d.colour(BLACK);
    d.fill(&dot(middle, 2.5));
    // The corners' handles.
    for (i, &c) in [tl, tr, bl, br].iter().enumerate() {
        let hot = matches!(state.drag, Some(Drag::Corner { corner, .. }) if corner == i)
            || state.hover == Some(Part::Corner(i));
        let r = if hot { HANDLE + 1.5 } else { HANDLE };
        d.colour(WHITE);
        d.fill(&dot(c, r));
        d.colour(RIM);
        d.width(1.0);
        d.outline(&dot(c, r));
    }
}

// Presets.

/// The factory presets, VEGAS's: their names and values. They keep the
/// corners where they start, as VEGAS's do (its fifth's picture shows
/// them there, greyed, the picture elsewhere): the corners follow the
/// picture once it is changed by hand.
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    let preset = |name: &'static str, location: [f64; 2], angle: f64, scale: f64, opacity: f64| {
        let mut values = vec![
            ("location", num(&location)),
            ("angle", num(&[angle])),
            ("scaleX", num(&[scale])),
            ("scaleY", num(&[scale])),
            ("mode", num(&[0.0])),
            ("shear", num(&[0.0])),
            ("opacity", num(&[opacity])),
        ];
        for (c, at) in ["topLeft", "topRight", "bottomLeft", "bottomRight"]
            .into_iter()
            .zip(DEFAULT_CORNERS)
        {
            values.push((c, num(&at)));
        }
        Preset { name, values }
    };
    vec![
        preset(
            "Upper Left, Scaled, No Angle, Opaque",
            [0.25, 0.75],
            0.0,
            0.333,
            1.0,
        ),
        preset(
            "Upper Right, Scaled, No Angle, Opaque",
            [0.75, 0.75],
            0.0,
            0.333,
            1.0,
        ),
        preset(
            "Lower Left, Scaled, No Angle, Opaque",
            [0.25, 0.25],
            0.0,
            0.333,
            1.0,
        ),
        preset(
            "Lower Right, Scaled, No Angle, Opaque",
            [0.75, 0.25],
            0.0,
            0.333,
            1.0,
        ),
        preset(
            "Middle Left, Scaled, Angled, Translucent",
            [0.25, 0.6],
            -35.0,
            0.433,
            0.75,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VEGAS's prints' frames: 1280 × 720.
    const ASPECT: f64 = 16.0 / 9.0;

    fn near(a: [f64; 2], b: [f64; 2], within: f64) -> bool {
        (a[0] - b[0]).abs() < within && (a[1] - b[1]).abs() < within
    }

    #[test]
    fn at_its_defaults_the_picture_is_half_the_frame_in_its_middle() {
        let s = Settings::default();
        let q = s.quad(ASPECT);
        for i in 0..4 {
            assert!(near(q[i], DEFAULT_CORNERS[i], 1e-9), "{q:?}");
        }
        // Free Form with its corners where they start: the same picture.
        let free = Settings {
            mode: Mode::FreeForm,
            ..s
        };
        let m = free.map(ASPECT).unwrap();
        for i in 0..4 {
            assert!(near(
                apply(&m, OWN_CORNERS[i]).unwrap(),
                DEFAULT_CORNERS[i],
                1e-9
            ));
        }
    }

    #[test]
    fn vegass_presets_put_a_third_of_the_picture_in_each_corner() {
        // VEGAS's Upper Left: its prints show it from 0.081 to 0.416
        // across and 0.087 to 0.421 down.
        let s = Settings {
            location: [0.25, 0.75],
            scale: [0.333; 2],
            ..Settings::default()
        };
        let [tl, _, _, br] = s.quad(ASPECT);
        assert!((tl[0] - 0.0835).abs() < 0.003 && (br[0] - 0.4165).abs() < 0.003);
        assert!((1.0 - tl[1] - 0.0835).abs() < 0.004 && (1.0 - br[1] - 0.4165).abs() < 0.004);
        // The presets keep the corners where they start, as VEGAS's.
        let p = &presets()[0];
        assert_eq!(p.name, "Upper Left, Scaled, No Angle, Opaque");
        assert!(
            p.values
                .contains(&("topLeft", PresetValue::Num(vec![0.25, 0.75])))
        );
    }

    #[test]
    fn vegass_fifth_preset_turns_a_bigger_picture_left_of_the_middle() {
        // VEGAS's Middle Left, Scaled, Angled, Translucent: its print's
        // corners (its handles, on a 654 × 368 frame).
        let p = &presets()[4];
        assert_eq!(p.name, "Middle Left, Scaled, Angled, Translucent");
        for (name, v) in [
            ("location", vec![0.25, 0.6]),
            ("angle", vec![-35.0]),
            ("scaleX", vec![0.433]),
            ("opacity", vec![0.75]),
        ] {
            assert!(p.values.contains(&(name, PresetValue::Num(v))), "{name}");
        }
        let s = Settings {
            location: [0.25, 0.6],
            angle: -35.0,
            scale: [0.433; 2],
            opacity: 0.75,
            ..Settings::default()
        };
        let vegas = [
            [0.0011, 0.5533],
            [0.3567, 0.9957],
            [0.1413, 0.1970],
            [0.4974, 0.6399],
        ];
        let q = s.quad(ASPECT);
        for i in 0..4 {
            assert!(near(q[i], vegas[i], 0.006), "{q:?}");
        }
    }

    #[test]
    fn the_angle_turns_clockwise_and_the_shear_lowers_the_right_side() {
        // VEGAS's pictures: 45° shows these corners...
        let turned = Settings {
            angle: 45.0,
            ..Settings::default()
        };
        let vegas = [
            [0.423, 0.991],
            [0.776, 0.363],
            [0.224, 0.637],
            [0.577, 0.009],
        ];
        let q = turned.quad(ASPECT);
        for i in 0..4 {
            assert!(near(q[i], vegas[i], 0.0006), "{q:?}");
        }
        // ...and Fixed Parallelogram's shear of 0.5 these.
        let sheared = Settings {
            mode: Mode::FixedParallelogram,
            shear: 0.5,
            ..Settings::default()
        };
        let vegas = [[0.25, 0.972], [0.75, 0.528], [0.25, 0.472], [0.75, 0.028]];
        let q = sheared.quad(ASPECT);
        for i in 0..4 {
            assert!(near(q[i], vegas[i], 0.0006), "{q:?}");
        }
        // Fixed Shape has neither its own height nor shear.
        let fixed = Settings {
            mode: Mode::FixedShape,
            scale: [0.5, 0.2],
            ..sheared
        };
        assert_eq!(fixed.quad(ASPECT), Settings::default().quad(ASPECT));
    }

    #[test]
    fn free_form_shows_the_corners_and_their_nearest_parallelogram() {
        // VEGAS's picture: its top right corner dragged.
        let mut s = Settings {
            mode: Mode::FreeForm,
            ..Settings::default()
        };
        s.corners[1] = [0.948, 0.894];
        let m = s.map(ASPECT).unwrap();
        assert!(near(
            apply(&m, OWN_CORNERS[1]).unwrap(),
            [0.948, 0.894],
            1e-9
        ));
        // Its location the corners' mean, as VEGAS's (0.550, 0.536); the
        // angle, scales and shear near VEGAS's (18.6°, 0.567, 0.612, -0.42).
        let f = s.fitted(ASPECT);
        assert!(near(f.location, [0.5495, 0.536], 1e-9), "{f:?}");
        assert!((f.angle - 17.1).abs() < 0.1, "{f:?}");
        assert!((f.scale[0] - 0.561).abs() < 0.002 && (f.scale[1] - 0.598).abs() < 0.002);
        assert!((f.shear + 0.383).abs() < 0.002);
        // A parallelogram's own settings come back from its corners.
        let p = Settings {
            location: [0.4, 0.6],
            angle: 30.0,
            scale: [0.6, 0.35],
            mode: Mode::FixedParallelogram,
            shear: 0.4,
            ..Settings::default()
        };
        let back = Settings {
            corners: p.quad(ASPECT),
            mode: Mode::FreeForm,
            ..Settings::default()
        }
        .fitted(ASPECT);
        assert!(near(back.location, p.location, 1e-9) && (back.angle - 30.0).abs() < 1e-9);
        assert!(near(back.scale, p.scale, 1e-9) && (back.shear - 0.4).abs() < 1e-9);
    }

    #[test]
    fn pinned_pictures_map_back_from_inside() {
        let mut s = Settings {
            mode: Mode::FreeForm,
            ..Settings::default()
        };
        s.corners = [[0.1, 0.9], [0.8, 0.7], [0.2, 0.1], [0.9, 0.35]];
        let m = s.map(ASPECT).unwrap();
        let w = s.warp(ASPECT).unwrap();
        for p in [[0.5, 0.5], [0.2, 0.3], [0.9, 0.1], [0.0, 1.0]] {
            let q = apply(&m, p).unwrap();
            let back = apply(&w.inverse, q).expect("ahead");
            assert!(near(back, p, 1e-9));
        }
    }

    fn frame() -> Frame {
        Frame {
            origin: [0.0, 0.0],
            size: [1280.0, 720.0],
        }
    }

    #[test]
    fn a_corner_stops_where_the_shape_would_fold() {
        let f = frame();
        let start = Settings {
            mode: Mode::FreeForm,
            ..Settings::default()
        };
        // The top left corner dragged far past the line between its
        // neighbours (the bottom left and the top right): it stops on it,
        // a triangle, as VEGAS's.
        let drag = Drag::Corner { corner: 0, start };
        let s = dragged(&drag, &f, f.canonical([0.9, 0.2]));
        let [tl, tr, bl, _] = s.corners;
        let cross = (tr[0] - bl[0]) * (tl[1] - bl[1]) - (tr[1] - bl[1]) * (tl[0] - bl[0]);
        assert!(cross.abs() < 1e-4, "{:?}", s.corners);
        assert!(s.map(ASPECT).is_some(), "the picture still shows");
        // Within the shape it goes where the pen is.
        let s = dragged(&drag, &f, f.canonical([0.2, 0.8]));
        assert!(near(s.corners[0], [0.2, 0.8], 1e-9));
        // The settings follow the corners.
        assert_eq!(s.location, s.fitted(ASPECT).location);
    }

    #[test]
    fn the_rings_handle_turns_and_scales() {
        let f = frame();
        let start = Settings::default();
        let middle = f.canonical(start.location);
        let handle = ring_handle(&start, &f);
        let d = [handle[0] - middle[0], handle[1] - middle[1]];
        let drag = Drag::Ring {
            angle: angle_from_up(d),
            distance: d[0].hypot(d[1]),
            start,
        };
        // A quarter turn clockwise, twice as far: 90° and twice as big.
        let to = [middle[0] + 2.0 * d[1], middle[1]];
        let s = dragged(&drag, &f, to);
        assert!(
            (s.angle - 90.0).abs() < 1e-9 && (s.scale[0] - 1.0).abs() < 1e-9,
            "{s:?}"
        );
        assert_eq!(s.corners, s.placed_corners(ASPECT));
        // In Free Form the corners turn and grow about the middle.
        let free = Settings {
            mode: Mode::FreeForm,
            ..start
        };
        let ring = Drag::Ring {
            angle: angle_from_up(d),
            distance: d[0].hypot(d[1]),
            start: free,
        };
        let s = dragged(&ring, &f, to);
        assert!(
            (s.angle - 90.0).abs() < 1e-9 && (s.scale[0] - 1.0).abs() < 1e-9,
            "{s:?}"
        );
        let turned = Settings {
            angle: 90.0,
            scale: [1.0; 2],
            ..start
        };
        for i in 0..4 {
            assert!(near(s.corners[i], turned.quad(ASPECT)[i], 1e-9));
        }
    }

    #[test]
    fn dragging_corners_scales_the_fixed_shapes() {
        let f = frame();
        let s = Settings::default();
        // Fixed Shape: the top right corner twice as far from the middle
        // doubles the picture.
        let tr = corners(&s, &f)[1];
        let pen = [640.0 + 2.0 * (tr[0] - 640.0), 360.0 + 2.0 * (tr[1] - 360.0)];
        let n = dragged(
            &Drag::Corner {
                corner: 1,
                start: s,
            },
            &f,
            pen,
        );
        assert!((n.scale[0] - 1.0).abs() < 1e-9 && n.scale[1] == n.scale[0]);
        // Fixed Parallelogram: each way on its own.
        let p = Settings {
            mode: Mode::FixedParallelogram,
            ..s
        };
        let n = dragged(
            &Drag::Corner {
                corner: 1,
                start: p,
            },
            &f,
            [1280.0, 432.0],
        );
        assert!(
            (n.scale[0] - 1.0).abs() < 1e-9 && (n.scale[1] - 0.2).abs() < 1e-9,
            "{n:?}"
        );
    }

    #[test]
    fn the_warp_samples_more_as_the_picture_shrinks() {
        let w = Settings::default().warp(ASPECT).unwrap();
        assert_eq!(w.samples, 2);
        let small = Settings {
            scale: [0.2; 2],
            ..Settings::default()
        };
        assert_eq!(small.warp(ASPECT).unwrap().samples, 4);
        // A frame-sized picture changes nothing.
        let whole = Settings {
            scale: [1.0; 2],
            ..Settings::default()
        };
        assert!(whole.changes_nothing(ASPECT));
        // Nothing big: nothing to draw.
        let none = Settings {
            scale: [0.0; 2],
            ..Settings::default()
        };
        assert!(none.warp(ASPECT).is_none());
    }
}
