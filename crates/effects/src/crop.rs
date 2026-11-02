//! Crop: keeps a window of the picture, as VEGAS's Crop effect does. The
//! picture stays where it is; what the window does not show becomes
//! transparent.
//!
//! The window is `X Crop` × `Y Crop` of the frame's size, its middle at
//! `Location` (0–1 across and up, as VEGAS gives it: its Crop Left Half
//! keeps the right half with its middle at 0.75). `Angle` turns it around
//! there (clockwise, as VEGAS's), `Opacity` fades what it shows; each pixel
//! is kept or not by its corner, as VEGAS's (no smoothing). `Anchor Point`
//! is the edge, corner or middle
//! that stays where it is when the crop changes: X Crop and Y Crop
//! changed by hand move `Location` with them so that it does.
//!
//! On the preview (an overlay drawn with the draw suite): the window's
//! outline, a yellow handle on each edge (the anchor's hollow, a black
//! dot in it) and the rotation control around `Location`. Dragging an
//! edge's handle makes the opposite edge the anchor and moves that edge,
//! the anchor staying; dragging inside moves the window; dragging the
//! rotation handle turns it.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::Mutex;

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Draw, Fx, Props, for_rows, interact_props, is, redraw, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.Crop";

const ANCHOR: &CStr = c"anchor";
const X_CROP: &CStr = c"xCrop";
const Y_CROP: &CStr = c"yCrop";
const LOCATION: &CStr = c"location";
const ANGLE: &CStr = c"angle";
const OPACITY: &CStr = c"opacity";

/// The most of the frame a crop keeps: all of it (VEGAS's crops only go
/// down from 1).
const MAX_CROP: f64 = 1.0;

/// Where the crops' sliders end: 1 sits in their middle, as in VEGAS.
const CROP_SLIDER_MAX: f64 = 2.0;

/// Which part of the window stays put when it shrinks: −1, 0 or 1 across
/// (left, middle, right) and up (bottom, middle, top).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Anchor {
    pub h: i8,
    pub v: i8,
}

const fn anchor(h: i8, v: i8) -> Anchor {
    Anchor { h, v }
}

/// The anchors, in the order of the parameter's options.
pub(crate) const ANCHORS: [(&CStr, Anchor); 9] = [
    (c"Center", anchor(0, 0)),
    (c"Left", anchor(-1, 0)),
    (c"Right", anchor(1, 0)),
    (c"Top", anchor(0, 1)),
    (c"Bottom", anchor(0, -1)),
    (c"Top Left", anchor(-1, 1)),
    (c"Top Right", anchor(1, 1)),
    (c"Bottom Left", anchor(-1, -1)),
    (c"Bottom Right", anchor(1, -1)),
];

/// Right, VEGAS's default.
const DEFAULT_ANCHOR: c_int = 2;

fn anchor_index(a: Anchor) -> c_int {
    ANCHORS
        .iter()
        .position(|(_, x)| *x == a)
        .map_or(DEFAULT_ANCHOR, |i| i as c_int)
}

/// The window's settings at one time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub anchor: Anchor,
    /// The parts kept across and up (1: the frame's size).
    pub crop: [f64; 2],
    /// Where its middle is: 0–1 across and up the frame.
    pub location: [f64; 2],
    /// Degrees, clockwise.
    pub angle: f64,
    pub opacity: f64,
}

/// The window: the part of a frame-sized window around its pivot that is
/// kept, before it turns; and how it turns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Window {
    pub pivot: [f64; 2],
    /// Its sides, from the pivot, unturned: x from `lo[0]` to `hi[0]`, y
    /// from `lo[1]` to `hi[1]`.
    pub lo: [f64; 2],
    pub hi: [f64; 2],
    /// The cosine and sine of its turn (counterclockwise: the angle's
    /// negative).
    pub turn: [f64; 2],
}

impl Settings {
    /// The window over a frame at `origin`, `size` big (canonical units).
    pub fn window(&self, origin: [f64; 2], size: [f64; 2]) -> Window {
        let half = |k: usize| self.crop[k].clamp(0.0, MAX_CROP) * size[k] / 2.0;
        let r = self.angle.to_radians();
        Window {
            pivot: [
                origin[0] + self.location[0] * size[0],
                origin[1] + self.location[1] * size[1],
            ],
            lo: [-half(0), -half(1)],
            hi: [half(0), half(1)],
            turn: [r.cos(), -r.sin()],
        }
    }

    /// `Location` once the crop changed from `old` to what it is, so that
    /// the anchor stays where it was (on a frame `size` big).
    pub fn kept_anchor(&self, old: [f64; 2], size: [f64; 2]) -> [f64; 2] {
        let a = [f64::from(self.anchor.h), f64::from(self.anchor.v)];
        let part = |v: f64| v.clamp(0.0, MAX_CROP);
        // How far the anchor would go from the middle, unturned.
        let d = [0, 1].map(|k| a[k] * (part(old[k]) - part(self.crop[k])) * size[k] / 2.0);
        let r = self.angle.to_radians();
        let (c, s) = (r.cos(), -r.sin());
        [
            self.location[0] + (d[0] * c - d[1] * s) / size[0],
            self.location[1] + (d[0] * s + d[1] * c) / size[1],
        ]
    }
}

impl Window {
    /// A point of the picture in the window's own coordinates (from its
    /// pivot, unturned).
    pub fn local(&self, p: [f64; 2]) -> [f64; 2] {
        let (dx, dy) = (p[0] - self.pivot[0], p[1] - self.pivot[1]);
        let [c, s] = self.turn;
        [dx * c + dy * s, -dx * s + dy * c]
    }

    /// A point of the window's coordinates in the picture's.
    pub fn picture(&self, q: [f64; 2]) -> [f64; 2] {
        let [c, s] = self.turn;
        [
            self.pivot[0] + q[0] * c - q[1] * s,
            self.pivot[1] + q[0] * s + q[1] * c,
        ]
    }

    /// Whether `q` (the window's coordinates) is inside it or on its
    /// sides: VEGAS's test of a pixel's corner.
    pub fn covers(&self, q: [f64; 2]) -> bool {
        (self.lo[0]..=self.hi[0]).contains(&q[0]) && (self.lo[1]..=self.hi[1]).contains(&q[1])
    }

    /// Whether the picture's point `p` is inside (or on its sides).
    pub fn contains(&self, p: [f64; 2]) -> bool {
        let q = self.local(p);
        let eps = 1e-6 * (1.0 + self.hi[0].abs() + self.hi[1].abs());
        (self.lo[0] - eps..=self.hi[0] + eps).contains(&q[0])
            && (self.lo[1] - eps..=self.hi[1] + eps).contains(&q[1])
    }

    /// Its corners in the picture, around it.
    pub fn corners(&self) -> [[f64; 2]; 4] {
        let (lo, hi) = (self.lo, self.hi);
        [
            [lo[0], lo[1]],
            [hi[0], lo[1]],
            [hi[0], hi[1]],
            [lo[0], hi[1]],
        ]
        .map(|q| self.picture(q))
    }

    fn middle(&self) -> [f64; 2] {
        [
            (self.lo[0] + self.hi[0]) / 2.0,
            (self.lo[1] + self.hi[1]) / 2.0,
        ]
    }

    /// The middle of each edge, in [`EDGES`]' order, in the picture.
    pub fn handles(&self) -> [[f64; 2]; 4] {
        let m = self.middle();
        EDGES.map(|e| {
            self.picture(match e {
                Edge::Left => [self.lo[0], m[1]],
                Edge::Right => [self.hi[0], m[1]],
                Edge::Bottom => [m[0], self.lo[1]],
                Edge::Top => [m[0], self.hi[1]],
            })
        })
    }

    /// Where `a` sits on the window, in the picture.
    pub fn anchor_point(&self, a: Anchor) -> [f64; 2] {
        let m = self.middle();
        let pick = |s: i8, lo: f64, mid: f64, hi: f64| match s.signum() {
            -1 => lo,
            0 => mid,
            _ => hi,
        };
        self.picture([
            pick(a.h, self.lo[0], m[0], self.hi[0]),
            pick(a.v, self.lo[1], m[1], self.hi[1]),
        ])
    }
}

/// An edge of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edge {
    Left,
    Right,
    Bottom,
    Top,
}

pub(crate) const EDGES: [Edge; 4] = [Edge::Left, Edge::Right, Edge::Bottom, Edge::Top];

impl Edge {
    /// The anchor on it.
    fn own(self) -> Anchor {
        match self {
            Edge::Left => anchor(-1, 0),
            Edge::Right => anchor(1, 0),
            Edge::Bottom => anchor(0, -1),
            Edge::Top => anchor(0, 1),
        }
    }

    /// The anchor dragging it makes: the opposite edge's.
    pub fn opposite(self) -> Anchor {
        let a = self.own();
        anchor(-a.h, -a.v)
    }

    /// The edge across the window.
    fn across(self) -> Edge {
        match self {
            Edge::Left => Edge::Right,
            Edge::Right => Edge::Left,
            Edge::Bottom => Edge::Top,
            Edge::Top => Edge::Bottom,
        }
    }

    /// The crop it changes: 0 across, 1 up.
    fn axis(self) -> usize {
        match self {
            Edge::Left | Edge::Right => 0,
            Edge::Bottom | Edge::Top => 1,
        }
    }

    fn index(self) -> usize {
        EDGES.iter().position(|&e| e == self).unwrap_or(0)
    }
}

/// An edge being dragged: the middle of the edge across the window, which
/// stays, and the way out of the window through the dragged edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pull {
    edge: Edge,
    fixed: [f64; 2],
    out: [f64; 2],
}

impl Pull {
    pub fn new(w: &Window, edge: Edge) -> Pull {
        let [c, s] = w.turn;
        let out = match edge {
            Edge::Right => [c, s],
            Edge::Left => [-c, -s],
            Edge::Top => [-s, c],
            Edge::Bottom => [s, -c],
        };
        Pull {
            edge,
            fixed: w.handles()[edge.across().index()],
            out,
        }
    }

    /// The crop and the location (0–1) with the dragged edge at the pen,
    /// on a frame at `origin`, `size` big.
    pub fn at(&self, pen: [f64; 2], origin: [f64; 2], size: [f64; 2]) -> (f64, [f64; 2]) {
        let full = size[self.edge.axis()];
        let to = [pen[0] - self.fixed[0], pen[1] - self.fixed[1]];
        let len = (to[0] * self.out[0] + to[1] * self.out[1]).clamp(0.0, MAX_CROP * full);
        let mid = [
            self.fixed[0] + self.out[0] * len / 2.0,
            self.fixed[1] + self.out[1] * len / 2.0,
        ];
        (
            len / full,
            [
                (mid[0] - origin[0]) / size[0],
                (mid[1] - origin[1]) / size[1],
            ],
        )
    }
}

/// The settings of instance `fx` at `time`.
fn settings_at(fx: Fx, time: f64) -> Settings {
    let i = fx.param(ANCHOR).int_at(time);
    Settings {
        anchor: ANCHORS
            .get(i.max(0) as usize)
            .map_or(ANCHORS[DEFAULT_ANCHOR as usize].1, |a| a.1),
        crop: [fx.param(X_CROP).f64_at(time), fx.param(Y_CROP).f64_at(time)],
        location: fx.param(LOCATION).xy_at(time),
        angle: fx.param(ANGLE).f64_at(time),
        opacity: fx.param(OPACITY).f64_at(time).clamp(0.0, 1.0),
    }
}

/// The settings in `v`, a time's values as the host gives them (what the
/// graphics card draws with).
pub(crate) fn settings_of(v: Values<'_>) -> Settings {
    let i = v.int(ANCHOR, DEFAULT_ANCHOR);
    Settings {
        anchor: ANCHORS
            .get(i.max(0) as usize)
            .map_or(ANCHORS[DEFAULT_ANCHOR as usize].1, |a| a.1),
        crop: [v.f64(X_CROP, 1.0), v.f64(Y_CROP, 1.0)],
        location: v.array(LOCATION, [0.5; 2]),
        angle: v.f64(ANGLE, 0.0),
        opacity: v.f64(OPACITY, 1.0).clamp(0.0, 1.0),
    }
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
        is_identity(fx, args, out)
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // Each output pixel needs the source's pixel there only.
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &roi);
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) {
        let memory = Box::into_raw(Box::new(Mutex::new(Memory::default())));
        fx.props()
            .set_ptr(kOfxPropInstanceData, memory as *mut c_void);
        note_crop(fx, 0.0, settings_at(fx, 0.0).crop);
        kOfxStatOK
    } else if is(action, kOfxActionBeginInstanceEdit) {
        // Its settings open, with their values: the crop to start from.
        note_crop(fx, 0.0, settings_at(fx, 0.0).crop);
        kOfxStatOK
    } else if is(action, kOfxActionDestroyInstance) {
        let memory = fx.props().ptr(kOfxPropInstanceData) as *mut Mutex<Memory>;
        if !memory.is_null() {
            // SAFETY: made when the instance was, freed once.
            drop(unsafe { Box::from_raw(memory) });
            fx.props()
                .set_ptr(kOfxPropInstanceData, std::ptr::null_mut());
        }
        kOfxStatOK
    } else if is(action, kOfxActionBeginInstanceChanged) {
        let by_hand = args.is(kOfxPropChangeReason, kOfxChangeUserEdited);
        remember(fx, |m| {
            m.changing.clear();
            m.by_hand = by_hand;
        });
        kOfxStatOK
    } else if is(action, kOfxActionInstanceChanged) {
        if args.is(kOfxPropType, kOfxTypeParameter) {
            let time = args.dbl(kOfxPropTime, 0);
            remember(fx, |m| {
                m.time = time;
                m.changing.extend(
                    [X_CROP, Y_CROP, LOCATION]
                        .into_iter()
                        .filter(|&n| args.is(kOfxPropName, n)),
                );
            });
        }
        kOfxStatOK
    } else if is(action, kOfxActionEndInstanceChanged) {
        crops_changed(fx);
        kOfxStatOK
    } else if is(action, kOfxActionLoad) || is(action, kOfxActionUnload) {
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

/// What an instance remembers between its parameters' changes: the crop it
/// last saw, to keep the anchor where it was when the user changes it.
#[derive(Debug, Default)]
struct Memory {
    /// The crop, and the time it held at (none: at every time, neither
    /// crop being animated).
    seen: Option<(Option<f64>, [f64; 2])>,
    /// The crops and location changed since the host began telling, at
    /// what time, and whether by the user.
    changing: Vec<&'static CStr>,
    time: f64,
    by_hand: bool,
}

/// Runs `f` on instance `fx`'s memory (when it has one).
fn remember(fx: Fx, f: impl FnOnce(&mut Memory)) {
    let memory = fx.props().ptr(kOfxPropInstanceData) as *const Mutex<Memory>;
    if memory.is_null() {
        return;
    }
    // SAFETY: made when the instance was, alive until it is destroyed.
    let memory = unsafe { &*memory };
    let mut m = memory.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut m);
}

/// Notes the crop seen at `time`.
fn note_crop(fx: Fx, time: f64, crop: [f64; 2]) {
    let animated = [X_CROP, Y_CROP]
        .iter()
        .any(|&p| !fx.param(p).key_times().is_empty());
    remember(fx, |m| m.seen = Some((animated.then_some(time), crop)));
}

/// The host told what changed: a crop the user changed by hand (not with
/// the location, as a preset or an undo does) moves the location so that
/// the anchor stays where it was.
fn crops_changed(fx: Fx) {
    let mut moved = None;
    let mut time = 0.0;
    remember(fx, |m| {
        time = m.time;
        let by_hand = m.by_hand
            && !m.changing.contains(&LOCATION)
            && (m.changing.contains(&X_CROP) || m.changing.contains(&Y_CROP));
        if by_hand && let Some((when, old)) = m.seen {
            let s = settings_at(fx, time);
            if when.is_none_or(|t| (t - time).abs() < 1e-9) && old != s.crop {
                moved = Some(s.kept_anchor(old, fx.frame().1));
            }
        }
        m.changing.clear();
    });
    if let Some(location) = moved {
        fx.param(LOCATION).set_xy(location);
    }
    note_crop(fx, time, settings_at(fx, time).crop);
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Crop");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Utility");
    p.set_str(
        kOfxPropPluginDescription,
        c"Keeps a window of the picture, which can be moved, turned and faded.",
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
    let comps = [
        kOfxImageComponentRGBA,
        kOfxImageComponentRGB,
        kOfxImageComponentAlpha,
    ];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);

    let p = fx.define_param(kOfxParamTypeChoice, ANCHOR, c"Anchor Point");
    let names: Vec<&CStr> = ANCHORS.iter().map(|(n, _)| *n).collect();
    p.set_strs(kOfxParamPropChoiceOption, &names);
    p.set_int(kOfxParamPropDefault, DEFAULT_ANCHOR);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(
        kOfxParamPropHint,
        c"The edge or corner that stays where it is when the crop changes.",
    );

    let fraction = |name: &CStr, label: &CStr, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, 1.0);
        p.set_dbl(kOfxParamPropMin, 0.0);
        p.set_dbl(kOfxParamPropMax, MAX_CROP);
        p.set_dbl(kOfxParamPropDisplayMin, 0.0);
        p.set_dbl(kOfxParamPropDisplayMax, CROP_SLIDER_MAX);
        p.set_dbl(kOfxParamPropIncrement, 0.01);
        p.set_int(kOfxParamPropDigits, 3);
        p.set_str(kOfxParamPropHint, hint);
    };
    fraction(
        X_CROP,
        c"X Crop",
        c"How much of the width the window keeps (1: all of it).",
    );
    fraction(
        Y_CROP,
        c"Y Crop",
        c"How much of the height the window keeps (1: all of it).",
    );

    // 0–1 across and up, as VEGAS gives it (and its presets).
    let p = fx.define_param(kOfxParamTypeDouble2D, LOCATION, c"Location");
    p.set_str(
        kOfxParamPropDoubleType,
        kOfxParamDoubleTypeNormalisedXYAbsolute,
    );
    p.set_dbls(kOfxParamPropDefault, &[0.5, 0.5]);
    p.set_dbls(kOfxParamPropIncrement, &[0.01]);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"Where the window's middle is: 0.5, 0.5 is the middle of the frame.",
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
    p.set_str(kOfxParamPropHint, c"Turns the window, in degrees.");

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
        c"How much the window's picture shows: 1 all of it, 0 nothing.",
    );
}

fn is_identity(fx: Fx, args: Props, out: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let s = settings_at(fx, time);
    let (origin, size) = fx.frame();
    let w = s.window(origin, size);
    let frame = [
        origin,
        [origin[0] + size[0], origin[1]],
        [origin[0] + size[0], origin[1] + size[1]],
        [origin[0], origin[1] + size[1]],
    ];
    if s.opacity >= 1.0 && frame.iter().all(|&c| w.contains(c)) {
        out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

/// A channel of a pixel, of one of the depths the plug-in takes.
trait Channel: Copy {
    fn times(self, k: f32) -> Self;
    const ZERO: Self;
}

impl Channel for f32 {
    fn times(self, k: f32) -> f32 {
        self * k
    }
    const ZERO: f32 = 0.0;
}

impl Channel for u16 {
    fn times(self, k: f32) -> u16 {
        (self as f32 * k).round() as u16
    }
    const ZERO: u16 = 0;
}

impl Channel for u8 {
    fn times(self, k: f32) -> u8 {
        (self as f32 * k).round() as u8
    }
    const ZERO: u8 = 0;
}

/// Writes one row of the output: each pixel the source's there, times
/// `k(x)`.
///
/// # Safety
/// `out` starts the output's row at `out_x1`, `src` (when there is one)
/// the source's at `src_x`; both hold `channels` channels of `T` per pixel
/// over the columns they are read and written at.
#[allow(clippy::too_many_arguments)]
unsafe fn row<T: Channel>(
    out: *mut u8,
    out_x1: c_int,
    src: Option<(*const u8, std::ops::Range<c_int>)>,
    columns: std::ops::Range<c_int>,
    channels: usize,
    colors_kept: bool,
    k: impl Fn(c_int) -> f32,
) {
    let out = out as *mut T;
    for x in columns {
        let f = k(x);
        // SAFETY: as the caller promises.
        unsafe {
            let o = out.add((x - out_x1) as usize * channels);
            match &src {
                Some((s, xs)) if xs.contains(&x) => {
                    let s = (*s as *const T).add((x - xs.start) as usize * channels);
                    for c in 0..channels {
                        // Unpremultiplied colors stay: only alpha fades.
                        let k = if colors_kept && c < 3 { 1.0 } else { f };
                        *o.add(c) = (*s.add(c)).times(k);
                    }
                }
                _ => {
                    for c in 0..channels {
                        *o.add(c) = T::ZERO;
                    }
                }
            }
        }
    }
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
        && (s.depth != out.depth || s.channels != out.channels)
    {
        if crate::ofx::tracing() {
            eprintln!(
                "bananav crop: source {:?}×{} and output {:?}×{} differ",
                s.depth, s.channels, out.depth, out.channels
            );
        }
        return kOfxStatErrImageFormat;
    }
    let s = settings_at(fx, time);
    let (origin, size) = fx.frame();
    let w = s.window(origin, size);
    let par = out.par;
    if crate::ofx::tracing() {
        eprintln!(
            "bananav crop: time {time} window {:?} scale {scale:?} frame {origin:?} {size:?} {s:?}",
            [x1, y1, x2, y2]
        );
        let show =
            |i: &crate::ofx::Image| (i.bounds, i.depth, i.channels, i.unpremultiplied, i.par);
        eprintln!(
            "bananav crop: output {:?} source {:?}",
            show(&out),
            src.as_ref().map(show)
        );
    }
    let opacity = s.opacity as f32;
    let colors_kept = out.channels == 4 && src.as_ref().is_some_and(|s| s.unpremultiplied);
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(o) = out.row(y) else { return };
        let source = src.as_ref().and_then(|s| {
            let r = s.row(y)?;
            Some((r as *const u8, s.bounds[0]..s.bounds[2]))
        });
        // Each pixel by its corner, as VEGAS's.
        let cy = y as f64 / scale[1];
        let k = |x: c_int| {
            let cx = x as f64 * par / scale[0];
            if w.covers(w.local([cx, cy])) {
                opacity
            } else {
                0.0
            }
        };
        let (cols, n) = (columns.clone(), out.channels);
        // SAFETY: rows the images have, columns inside the output's bounds
        // (and checked against the source's), in the images' own depth.
        unsafe {
            match out.depth {
                crate::ofx::Depth::Float => {
                    row::<f32>(o, out.bounds[0], source, cols, n, colors_kept, k)
                }
                crate::ofx::Depth::Short => {
                    row::<u16>(o, out.bounds[0], source, cols, n, colors_kept, k)
                }
                crate::ofx::Depth::Byte => {
                    row::<u8>(o, out.bounds[0], source, cols, n, colors_kept, k)
                }
            }
        }
    };
    for_rows(y1.max(out.bounds[1])..y2.min(out.bounds[3]), &fill);
    drop(src);
    drop(out);
    kOfxStatOK
}

// The overlay.

/// What the pen is doing with the window.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// Moving it: where the pen went down, and the location then.
    Move {
        from: [f64; 2],
        location: [f64; 2],
    },
    Edge(Pull),
    /// Turning it: the handle's last angle (degrees).
    Turn {
        last: f64,
    },
}

/// A part of the overlay under the pen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Edge(Edge),
    Turn,
    Inside,
}

/// An overlay's own state.
#[derive(Debug, Default)]
struct Overlay {
    drag: Option<Drag>,
    hover: Option<Part>,
}

const YELLOW: [f32; 4] = [1.0, 0.9, 0.0, 1.0];
const RIM: [f32; 4] = [0.0, 0.0, 0.0, 0.6];
const LINE: [f32; 4] = [0.85, 0.85, 0.85, 0.9];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// The handles' size and how near the pen must be, in screen pixels.
const HANDLE: f64 = 5.0;
const REACH: f64 = 8.0;

/// The rotation control's radius, a part of the frame's width.
const TURN_RADIUS: f64 = 0.07;

/// The rotation handle: up from the pivot, turned with the window.
fn turn_handle(w: &Window, radius: f64) -> [f64; 2] {
    let [c, s] = w.turn;
    [w.pivot[0] - radius * s, w.pivot[1] + radius * c]
}

/// The angle of the direction `d` from up, clockwise, in degrees.
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

/// What the pen at `p` is over; `ps` is a screen pixel in canonical units.
fn hit(w: &Window, p: [f64; 2], ps: [f64; 2], size: [f64; 2]) -> Option<Part> {
    let dist = |a: [f64; 2]| ((p[0] - a[0]) / ps[0]).hypot((p[1] - a[1]) / ps[1]);
    if dist(turn_handle(w, TURN_RADIUS * size[0])) <= REACH {
        return Some(Part::Turn);
    }
    let nearest = w
        .handles()
        .iter()
        .enumerate()
        .map(|(i, &h)| (dist(h), i))
        .filter(|(d, _)| *d <= REACH)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, i)) = nearest {
        return Some(Part::Edge(EDGES[i]));
    }
    (dist(w.pivot) <= REACH || w.contains(p)).then_some(Part::Inside)
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
    if is(action, kOfxInteractActionDraw) {
        draw(fx, args, state, time, ps);
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    let (origin, size) = fx.frame();
    let s = settings_at(fx, time);
    let w = s.window(origin, size);
    if is(action, kOfxInteractActionPenDown) {
        let Some(part) = hit(&w, pen, ps, size) else {
            return kOfxStatReplyDefault;
        };
        state.drag = Some(match part {
            Part::Turn => Drag::Turn {
                last: angle_from_up([pen[0] - w.pivot[0], pen[1] - w.pivot[1]]),
            },
            Part::Edge(e) => {
                // The edge across becomes the anchor: it stays.
                let to = e.opposite();
                if to != s.anchor {
                    fx.param(ANCHOR).set_int(anchor_index(to));
                }
                Drag::Edge(Pull::new(&w, e))
            }
            Part::Inside => Drag::Move {
                from: pen,
                location: s.location,
            },
        });
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        match state.drag {
            None => {
                let hover = hit(&w, pen, ps, size);
                if hover != state.hover {
                    state.hover = hover;
                    redraw(handle);
                }
                return kOfxStatReplyDefault;
            }
            Some(Drag::Move { from, location }) => {
                let to = [
                    location[0] + (pen[0] - from[0]) / size[0],
                    location[1] + (pen[1] - from[1]) / size[1],
                ];
                fx.param(LOCATION).set_xy(to);
            }
            Some(Drag::Edge(pull)) => {
                let (part, location) = pull.at(pen, origin, size);
                let name = if pull.edge.axis() == 0 {
                    X_CROP
                } else {
                    Y_CROP
                };
                fx.param(name).set_f64(part);
                fx.param(LOCATION).set_xy(location);
            }
            Some(Drag::Turn { last }) => {
                let a = angle_from_up([pen[0] - w.pivot[0], pen[1] - w.pivot[1]]);
                fx.param(ANGLE).set_f64(s.angle + wrap(a - last));
                state.drag = Some(Drag::Turn { last: a });
            }
        }
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

fn draw(fx: Fx, args: Props, state: &Overlay, time: f64, ps: [f64; 2]) {
    let Some(d) = Draw::of(args) else { return };
    let (origin, size) = fx.frame();
    let s = settings_at(fx, time);
    // What the crop is now, for the user's next change of it.
    note_crop(fx, time, s.crop);
    let w = s.window(origin, size);
    let dot = |c: [f64; 2], r: f64| circle(c, [r * ps[0], r * ps[1]], 20);
    // The window's outline.
    d.colour(LINE);
    d.width(1.0);
    d.outline(&w.corners());
    // The rotation control: a ring around the pivot, an arm up to its
    // handle, a mark at the pivot.
    let radius = TURN_RADIUS * size[0];
    let arm = turn_handle(&w, radius);
    d.outline(&circle(w.pivot, [radius, radius], 64));
    d.lines(&[w.pivot, arm]);
    d.colour(WHITE);
    d.width(1.5);
    d.outline(&dot(w.pivot, HANDLE));
    d.fill(&dot(w.pivot, 1.5));
    let turning = matches!(state.drag, Some(Drag::Turn { .. })) || state.hover == Some(Part::Turn);
    d.fill(&dot(arm, if turning { 5.5 } else { 4.0 }));
    d.colour(RIM);
    d.width(1.0);
    d.outline(&dot(arm, if turning { 5.5 } else { 4.0 }));
    // The edges' handles; the anchor's hollow, a black dot in it.
    for (i, &h) in w.handles().iter().enumerate() {
        let e = EDGES[i];
        let hot = matches!(state.drag, Some(Drag::Edge(p)) if p.edge == e)
            || state.hover == Some(Part::Edge(e));
        let r = if hot { HANDLE + 1.5 } else { HANDLE };
        if e.own() == s.anchor {
            d.colour(YELLOW);
            d.width(2.5);
            d.outline(&dot(h, r - 1.0));
            d.colour([0.0, 0.0, 0.0, 1.0]);
            d.fill(&dot(h, r - 2.5));
        } else {
            d.colour(YELLOW);
            d.fill(&dot(h, r));
            d.colour(RIM);
            d.width(1.0);
            d.outline(&dot(h, r));
        }
    }
    // An anchor off the edges' middles (a corner, the middle): a hollow
    // mark of its own.
    if !EDGES.iter().any(|e| e.own() == s.anchor) {
        let a = w.anchor_point(s.anchor);
        d.colour(YELLOW);
        d.width(2.5);
        d.outline(&dot(a, HANDLE - 1.0));
        d.colour([0.0, 0.0, 0.0, 1.0]);
        d.fill(&dot(a, HANDLE - 2.5));
    }
}

// Presets.

/// The factory presets, VEGAS's: their names and values.
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    let preset = |name: &'static str, to: Anchor, crop: [f64; 2], location: [f64; 2]| Preset {
        name,
        values: vec![
            ("anchor", num(&[f64::from(anchor_index(to))])),
            ("xCrop", num(&[crop[0]])),
            ("yCrop", num(&[crop[1]])),
            ("location", num(&location)),
            ("angle", num(&[0.0])),
            ("opacity", num(&[1.0])),
        ],
    };
    // (VEGAS's top and bottom halves have their anchor on the edge that
    // moved.)
    vec![
        preset(
            "Crop Left Half, No Angle, Opaque",
            anchor(1, 0),
            [0.5, 1.0],
            [0.75, 0.5],
        ),
        preset(
            "Crop Right Half, No Angle, Opaque",
            anchor(-1, 0),
            [0.5, 1.0],
            [0.25, 0.5],
        ),
        preset(
            "Crop Top Half, No Angle, Opaque",
            anchor(0, 1),
            [1.0, 0.5],
            [0.5, 0.25],
        ),
        preset(
            "Crop Bottom Half, No Angle, Opaque",
            anchor(0, -1),
            [1.0, 0.5],
            [0.5, 0.75],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: [f64; 2] = [1920.0, 1080.0];
    const AT: [f64; 2] = [0.0, 0.0];

    fn settings() -> Settings {
        Settings {
            anchor: anchor(1, 0),
            crop: [1.0, 1.0],
            location: [0.5, 0.5],
            angle: 0.0,
            opacity: 1.0,
        }
    }

    fn near(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
    }

    #[test]
    fn at_its_defaults_the_window_is_the_frame() {
        let w = settings().window(AT, SIZE);
        assert_eq!(
            w.corners(),
            [[0.0, 0.0], [1920.0, 0.0], [1920.0, 1080.0], [0.0, 1080.0]]
        );
        // A pixel's corner inside (or on a side) keeps it; past, not.
        assert!(w.covers(w.local([0.0, 0.0])) && w.covers(w.local([1920.0, 5.0])));
        assert!(!w.covers(w.local([-0.5, 100.0])));
    }

    #[test]
    fn the_window_is_around_its_location_whatever_the_anchor() {
        // VEGAS's Crop Left Half: half the width, its middle at 0.75: the
        // right half.
        let s = Settings {
            crop: [0.5, 1.0],
            location: [0.75, 0.5],
            ..settings()
        };
        for (_, a) in ANCHORS {
            let w = Settings { anchor: a, ..s }.window(AT, SIZE);
            assert_eq!(w.corners()[0], [960.0, 0.0]);
            assert_eq!(w.corners()[2], [1920.0, 1080.0]);
        }
    }

    #[test]
    fn a_crop_changed_by_hand_keeps_the_anchor() {
        // Anchored right (the default), half the width: the right half,
        // VEGAS's Crop Left Half.
        let s = Settings {
            crop: [0.5, 1.0],
            ..settings()
        };
        let location = s.kept_anchor([1.0, 1.0], SIZE);
        assert!(near(location, [0.75, 0.5]), "{location:?}");
        // Anchored at the bottom left, a quarter each way: that corner.
        let s = Settings {
            anchor: anchor(-1, -1),
            crop: [0.25, 0.25],
            ..settings()
        };
        let location = s.kept_anchor([1.0, 1.0], SIZE);
        let w = Settings { location, ..s }.window(AT, SIZE);
        assert!(near(w.corners()[0], [0.0, 0.0]) && near(w.corners()[2], [480.0, 270.0]));
        // In the middle: both sides move in.
        let s = Settings {
            anchor: anchor(0, 0),
            crop: [0.5, 0.5],
            ..settings()
        };
        assert!(near(s.kept_anchor([1.0, 1.0], SIZE), [0.5, 0.5]));
        // Turned a quarter, the anchor (right) is below the middle: the
        // window shrinks toward it, its middle going down.
        let s = Settings {
            angle: 90.0,
            crop: [0.5, 1.0],
            ..settings()
        };
        let location = s.kept_anchor([1.0, 1.0], SIZE);
        assert!(near(location, [0.5, 0.5 - 480.0 / 1080.0]), "{location:?}");
    }

    #[test]
    fn location_moves_the_window_not_the_picture() {
        // VEGAS: Location 0.815, 0.565 shows the frame's part from 0.315
        // across and up to 1 - 0.065.
        let s = Settings {
            location: [0.815, 0.565],
            ..settings()
        };
        let w = s.window(AT, SIZE);
        assert!(near(w.corners()[0], [0.315 * 1920.0, 0.065 * 1080.0]));
        assert!(w.contains([1900.0, 100.0]));
        assert!(!w.contains([100.0, 100.0]));
    }

    #[test]
    fn turned_windows_turn_clockwise_around_the_location() {
        let s = Settings {
            angle: 90.0,
            crop: [0.5, 0.5],
            anchor: anchor(0, 0),
            ..settings()
        };
        let w = s.window(AT, SIZE);
        // The right edge's middle (480 right of the middle) is now below,
        // as VEGAS's turns it.
        assert!(near(w.handles()[1], [960.0, 540.0 - 480.0]));
        // And back.
        assert!(near(w.local(w.handles()[1]), [480.0, 0.0]));
        assert!(w.covers(w.local([960.0, 540.0 - 480.0])));
        assert!(!w.covers(w.local([960.0, 540.0 - 481.0])));
    }

    #[test]
    fn dragging_an_edge_crops_from_it_the_edge_across_staying() {
        // The left edge dragged to a quarter of the width: three quarters
        // stay, from there to the right edge.
        let w = settings().window(AT, SIZE);
        let (part, location) = Pull::new(&w, Edge::Left).at([480.0, 300.0], AT, SIZE);
        assert!((part - 0.75).abs() < 1e-9);
        assert!(near(location, [0.625, 0.5]), "{location:?}");
        // The top edge down to the middle: the bottom half.
        let (part, location) = Pull::new(&w, Edge::Top).at([960.0, 540.0], AT, SIZE);
        assert!((part - 0.5).abs() < 1e-9 && near(location, [0.5, 0.25]));
        // Never past the other edge, nor wider than the frame.
        assert_eq!(
            Pull::new(&w, Edge::Right).at([-5000.0, 0.0], AT, SIZE).0,
            0.0
        );
        assert_eq!(
            Pull::new(&w, Edge::Right).at([9000.0, 0.0], AT, SIZE).0,
            MAX_CROP
        );
        // Turned a quarter: the right edge is down, pulled further down.
        let s = Settings {
            angle: 90.0,
            crop: [0.5, 1.0],
            ..settings()
        };
        let w = s.window(AT, SIZE);
        let (part, _) = Pull::new(&w, Edge::Right).at([960.0, 540.0 - 960.0], AT, SIZE);
        assert!((part - 0.75).abs() < 1e-9, "{part}");
    }

    #[test]
    fn the_rotation_handle_follows_the_angle() {
        assert!((angle_from_up([0.0, 1.0])).abs() < 1e-9);
        assert!((angle_from_up([1.0, 0.0]) - 90.0).abs() < 1e-9);
        assert_eq!(wrap(190.0), -170.0);
        let s = Settings {
            angle: 90.0,
            ..settings()
        };
        let h = turn_handle(&s.window(AT, SIZE), 100.0);
        assert!(near(h, [1060.0, 540.0]));
    }

    #[test]
    fn the_presets_crop_vegass_halves() {
        let p = presets();
        assert_eq!(p.len(), 4);
        let window = |i: usize| {
            let v = |name: &str| match p[i].values.iter().find(|(n, _)| *n == name) {
                Some((_, PresetValue::Num(v))) => v.clone(),
                other => panic!("{name}: {other:?}"),
            };
            let s = Settings {
                anchor: ANCHORS[v("anchor")[0] as usize].1,
                crop: [v("xCrop")[0], v("yCrop")[0]],
                location: [v("location")[0], v("location")[1]],
                angle: v("angle")[0],
                opacity: v("opacity")[0],
            };
            let w = s.window(AT, SIZE);
            (w.corners()[0], w.corners()[2])
        };
        // Left half cut away: the right half stays; and so on.
        assert_eq!(window(0), ([960.0, 0.0], [1920.0, 1080.0]));
        assert_eq!(window(1), ([0.0, 0.0], [960.0, 1080.0]));
        assert_eq!(window(2), ([0.0, 0.0], [1920.0, 540.0]));
        assert_eq!(window(3), ([0.0, 540.0], [1920.0, 1080.0]));
        assert_eq!(p[0].name, "Crop Left Half, No Angle, Opaque");
    }
}
