//! Cookie Cutter: VEGAS's. Keeps the picture inside a shape — or, the
//! other way, cuts the shape out of it — with a border of a color along
//! the cut and an edge as soft as Feather says; the shape repeats across
//! and down the frame, as big as Size says and centred where Center says
//! (picture in picture, a quick crop).
//!
//! The shapes are measured on the frame as VEGAS's are (its presets'
//! pictures): a circle's radius is Size times the frame's diagonal; a
//! rectangle has the frame's shape, Size times its width and height each
//! way; a square is as big as the circle (the same area), a diamond is that
//! square turned; triangles are equilateral around the circle; an oval on
//! its side is a frame turned upright, Size times the frame's width across.
//! The border lies inside the shape's edge, half the frame's width thick
//! at Border 1; Feather blurs the edges over a third of the frame's width
//! at 1. Repeating, each part of the frame holds the frame's shape,
//! squeezed.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::{Cookie, Values};
use crate::ofx::{Draw, Fx, Props, for_rows, interact_props, is, near_handle, redraw, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.CookieCutter";

pub(crate) const COLOR: &CStr = c"color";
pub(crate) const SHAPE: &CStr = c"shape";
pub(crate) const METHOD: &CStr = c"method";
pub(crate) const FEATHER: &CStr = c"feather";
pub(crate) const BORDER: &CStr = c"border";
pub(crate) const REPEAT_X: &CStr = c"repeatX";
pub(crate) const REPEAT_Y: &CStr = c"repeatY";
pub(crate) const SIZE: &CStr = c"size";
pub(crate) const CENTER: &CStr = c"center";
pub(crate) const DEPTH: &CStr = c"depth";

/// The shapes, in VEGAS's order.
const SHAPES: [&CStr; 14] = [
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

const METHODS: [&CStr; 2] = [c"Cut away all but section", c"Cut away section"];

/// The most times a shape repeats each way.
const MOST_REPEATS: c_int = 10;

/// A shape, as the plug-in measures it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Shape {
    Circle,
    Oval,
    OvalSide,
    Rectangle,
    Diamond,
    Square,
    /// Turned a quarter clockwise this many times (0: pointing up).
    Triangle(u8),
    Arrowhead(u8),
}

impl Shape {
    pub(crate) fn of(index: c_int) -> Shape {
        match index {
            1 => Shape::Oval,
            2 => Shape::OvalSide,
            3 => Shape::Rectangle,
            4 => Shape::Diamond,
            5 => Shape::Square,
            6..=9 => Shape::Triangle((index - 6) as u8),
            10..=13 => Shape::Arrowhead((index - 10) as u8),
            _ => Shape::Circle,
        }
    }
}

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub color: [f64; 3],
    pub shape: Shape,
    /// The shape is the hole (Cut away section), not the window.
    pub hole: bool,
    pub feather: f64,
    pub border: f64,
    pub repeat: [u32; 2],
    pub size: f64,
    /// Where the shape's middle is, 0–1 across and up.
    pub center: [f64; 2],
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        Settings {
            color: v.array(COLOR, [0.0; 3]),
            shape: Shape::of(v.int(SHAPE, 0)),
            hole: v.int(METHOD, 0) == 1,
            feather: v.f64(FEATHER, 0.0).max(0.0),
            border: v.f64(BORDER, 0.0).max(0.0),
            repeat: [REPEAT_X, REPEAT_Y].map(|p| v.int(p, 1).clamp(1, MOST_REPEATS) as u32),
            size: v.f64(SIZE, 0.25).max(0.0),
            center: v.array(CENTER, [0.5, 0.5]),
        }
    }

    /// As the graphics card takes them.
    pub(crate) fn cookie(&self) -> Cookie {
        let (shape, quarters) = match self.shape {
            Shape::Circle => (0, 0),
            Shape::Oval => (1, 0),
            Shape::OvalSide => (2, 0),
            Shape::Rectangle => (3, 0),
            Shape::Diamond => (4, 0),
            Shape::Square => (5, 0),
            Shape::Triangle(q) => (6, q as u32),
            Shape::Arrowhead(q) => (7, q as u32),
        };
        Cookie {
            color: self.color.map(|c| c as f32),
            shape,
            quarters,
            hole: self.hole,
            feather: self.feather as f32,
            border: self.border as f32,
            repeat: self.repeat,
            size: self.size as f32,
            center: self.center.map(|c| c as f32),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        let [r, g, b] = fx.param(COLOR).rgb_at(time);
        Settings {
            color: [r, g, b],
            shape: Shape::of(fx.param(SHAPE).int_at(time)),
            hole: fx.param(METHOD).int_at(time) == 1,
            feather: fx.param(FEATHER).f64_at(time).max(0.0),
            border: fx.param(BORDER).f64_at(time).max(0.0),
            repeat: [REPEAT_X, REPEAT_Y]
                .map(|p| fx.param(p).int_at(time).clamp(1, MOST_REPEATS) as u32),
            size: fx.param(SIZE).f64_at(time).max(0.0),
            center: fx.param(CENTER).xy_at(time),
        }
    }
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// How far `p` is inside a polygon (negative outside), its points in order.
fn polygon(p: [f64; 2], v: &[[f64; 2]]) -> f64 {
    let sub = |a: [f64; 2], b: [f64; 2]| [a[0] - b[0], a[1] - b[1]];
    let mut d = dot(sub(p, v[0]), sub(p, v[0]));
    let mut inside = false;
    let mut j = v.len() - 1;
    for i in 0..v.len() {
        let e = sub(v[j], v[i]);
        let w = sub(p, v[i]);
        let t = (dot(w, e) / dot(e, e).max(1e-12)).clamp(0.0, 1.0);
        let b = [w[0] - e[0] * t, w[1] - e[1] * t];
        d = d.min(dot(b, b));
        let c = [p[1] >= v[i][1], p[1] < v[j][1], e[0] * w[1] > e[1] * w[0]];
        if c.iter().all(|&x| x) || c.iter().all(|&x| !x) {
            inside = !inside;
        }
        j = i;
    }
    if inside { d.sqrt() } else { -d.sqrt() }
}

/// How far `p` (from the shape's middle, in the frame's units) is inside
/// the shape: positive inside, negative outside. `frame` is the frame's
/// size, `size` the setting.
pub(crate) fn inside(shape: Shape, p: [f64; 2], size: f64, frame: [f64; 2]) -> f64 {
    let [w, h] = frame;
    let diagonal = w.hypot(h);
    let r = size * diagonal;
    let boxed = |half: [f64; 2], p: [f64; 2]| {
        let q = [p[0].abs() - half[0], p[1].abs() - half[1]];
        let outside = q[0].max(0.0).hypot(q[1].max(0.0));
        -(outside + q[0].max(q[1]).min(0.0))
    };
    let ellipse = |a: f64, b: f64| {
        let k = ((p[0] / a.max(1e-9)).powi(2) + (p[1] / b.max(1e-9)).powi(2)).sqrt();
        (1.0 - k) * a.min(b)
    };
    let turned = |quarters: u8| match quarters % 4 {
        1 => [-p[1], p[0]],
        2 => [-p[0], -p[1]],
        3 => [p[1], -p[0]],
        _ => p,
    };
    let s3 = 3f64.sqrt();
    match shape {
        Shape::Circle => r - p[0].hypot(p[1]),
        Shape::Oval => ellipse(size * w * w / h, size * w),
        Shape::OvalSide => ellipse(size * w, size * w * w / h),
        Shape::Rectangle => boxed([size * w, size * h], p),
        Shape::Square => {
            let half = r * std::f64::consts::PI.sqrt() / 2.0;
            boxed([half, half], p)
        }
        Shape::Diamond => {
            let k = r * std::f64::consts::PI.sqrt() / 2.0 * 2f64.sqrt();
            (k - p[0].abs() - p[1].abs()) / 2f64.sqrt()
        }
        Shape::Triangle(q) => polygon(turned(q), &[[0.0, 2.0 * r], [-s3 * r, -r], [s3 * r, -r]]),
        Shape::Arrowhead(q) => polygon(
            turned(q),
            &[
                [0.0, 2.0 * r],
                [-s3 * r, -r],
                [0.0, -0.25 * r],
                [s3 * r, -r],
            ],
        ),
    }
}

/// From 0 to 1 over `a`..`b`, smoothly.
fn smooth(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What a point of the frame (0–1 across and up) shows: how much of the
/// picture and how much of the border's color.
pub(crate) fn weights(s: &Settings, frame: [f64; 2], at: [f64; 2], pixel: f64) -> (f64, f64) {
    let [nx, ny] = s.repeat.map(f64::from);
    let cell = [(at[0] * nx).rem_euclid(1.0), (at[1] * ny).rem_euclid(1.0)];
    let p = [
        (cell[0] - s.center[0]) * frame[0],
        (cell[1] - s.center[1]) * frame[1],
    ];
    let d = inside(s.shape, p, s.size, frame);
    // Feathered over Feather's width, smoothed over a pixel at least
    // (squeezed with the cells).
    let soft = (s.feather * frame[0] / 3.0).max(pixel * nx.max(ny));
    let border = s.border * frame[0] / 2.0;
    let shape = smooth(-soft / 2.0, soft / 2.0, d);
    let core = smooth(-soft / 2.0, soft / 2.0, d - border);
    let rim = shape - core;
    if s.hole {
        (1.0 - shape, rim)
    } else {
        (core, rim)
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
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // Each output pixel needs the source's pixel there only.
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &roi);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionGetRegionOfDefinition) {
        // The frame, whatever the source covers (a border may go past it).
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

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Cookie Cutter");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Utility");
    p.set_str(
        kOfxPropPluginDescription,
        c"Keeps the picture inside a shape, or cuts the shape out of it, with a border and a soft edge: picture in picture.",
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
    // The center's handle on the preview.
    p.set_ptr(
        kOfxImageEffectPluginPropOverlayInteractV2,
        overlay as *mut c_void,
    );
}

fn describe_in_context(fx: Fx) {
    // A window shows the frame's part only: the output has alpha.
    fx.define_clip(
        kOfxImageEffectSimpleSourceClipName,
        &[kOfxImageComponentRGBA, kOfxImageComponentRGB],
    );
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);

    let p = fx.define_param(kOfxParamTypeRGB, COLOR, c"Color");
    p.set_dbls(kOfxParamPropDefault, &[0.0, 0.0, 0.0]);
    p.set_str(kOfxParamPropHint, c"The border's color.");

    let choice = |name: &CStr, label: &CStr, options: &[&CStr], hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeChoice, name, label);
        p.set_strs(kOfxParamPropChoiceOption, options);
        p.set_int(kOfxParamPropDefault, 0);
        p.set_str(kOfxParamPropHint, hint);
    };
    choice(SHAPE, c"Shape", &SHAPES, c"The shape cut.");
    choice(
        METHOD,
        c"Method",
        &METHODS,
        c"Whether the shape is a window on the picture or a hole in it.",
    );
    let number = |name: &CStr, label: &CStr, [default, max]: [f64; 2], hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, default);
        p.set_dbl(kOfxParamPropMin, 0.0);
        p.set_dbl(kOfxParamPropMax, max);
        p.set_dbl(kOfxParamPropDisplayMin, 0.0);
        p.set_dbl(kOfxParamPropDisplayMax, max);
        p.set_dbl(kOfxParamPropIncrement, 0.01);
        p.set_int(kOfxParamPropDigits, 3);
        p.set_str(kOfxParamPropHint, hint);
    };
    number(
        FEATHER,
        c"Feather",
        [0.0, 2.0],
        c"How softly the cut's edge fades.",
    );
    number(
        BORDER,
        c"Border",
        [0.0, 1.0],
        c"How thick the border along the cut is.",
    );
    let repeat = |name: &CStr, label: &CStr, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeInteger, name, label);
        p.set_int(kOfxParamPropDefault, 1);
        p.set_int(kOfxParamPropMin, 1);
        p.set_int(kOfxParamPropMax, MOST_REPEATS);
        p.set_int(kOfxParamPropDisplayMin, 1);
        p.set_int(kOfxParamPropDisplayMax, MOST_REPEATS);
        p.set_str(kOfxParamPropHint, hint);
    };
    repeat(
        REPEAT_X,
        c"Repeat X",
        c"How many times the shape repeats across.",
    );
    repeat(
        REPEAT_Y,
        c"Repeat Y",
        c"How many times the shape repeats down.",
    );
    number(SIZE, c"Size", [0.25, 1.0], c"How big the shape is.");

    let p = fx.define_param(kOfxParamTypeDouble2D, CENTER, c"Center");
    p.set_str(
        kOfxParamPropDoubleType,
        kOfxParamDoubleTypeNormalisedXYAbsolute,
    );
    p.set_dbls(kOfxParamPropDefault, &[0.5, 0.5]);
    p.set_dbls(kOfxParamPropIncrement, &[0.01]);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"Where the shape's middle is: 0, 0 the frame's lower left corner, 1, 1 its upper right.",
    );

    let p = fx.define_param(kOfxParamTypeDouble, DEPTH, c"Stereoscopic 3D depth");
    p.set_dbl(kOfxParamPropDefault, 0.0);
    p.set_dbl(kOfxParamPropMin, -1.0);
    p.set_dbl(kOfxParamPropMax, 1.0);
    p.set_dbl(kOfxParamPropDisplayMin, -1.0);
    p.set_dbl(kOfxParamPropDisplayMax, 1.0);
    p.set_int(kOfxParamPropDigits, 3);
    // Stereoscopic 3D projects are to come: shown, not used yet.
    p.set_int(kOfxParamPropEnabled, 0);
    p.set_str(
        kOfxParamPropHint,
        c"The left and right eyes' views apart, in stereoscopic 3D projects (coming to BananaV).",
    );
}

/// A channel of a pixel, of one of the depths the plug-in takes.
trait Channel: Copy {
    fn get(self) -> f32;
    fn put(v: f32) -> Self;
}

impl Channel for f32 {
    fn get(self) -> f32 {
        self
    }
    fn put(v: f32) -> f32 {
        v
    }
}

impl Channel for u16 {
    fn get(self) -> f32 {
        self as f32 / 65535.0
    }
    fn put(v: f32) -> u16 {
        (v.clamp(0.0, 1.0) * 65535.0).round() as u16
    }
}

impl Channel for u8 {
    fn get(self) -> f32 {
        self as f32 / 255.0
    }
    fn put(v: f32) -> u8 {
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    }
}

/// What a row needs to know: the settings, the frame, how pixels map to
/// it.
struct Place {
    settings: Settings,
    origin: [f64; 2],
    frame: [f64; 2],
    scale: [f64; 2],
    /// The source's colors are multiplied by its alpha.
    premultiplied: bool,
}

/// Writes one row of the output (RGBA): the source's pixels in the window
/// (or around the hole), the border's color along the cut, nothing
/// elsewhere.
///
/// # Safety
/// `out` starts the output's row at `out_x1` (4 channels of `T` a pixel),
/// `src` (when there is one) the source's at its range's start
/// (`channels` of `T` a pixel).
unsafe fn row<T: Channel>(
    out: *mut u8,
    out_x1: c_int,
    src: Option<(*const u8, std::ops::Range<c_int>, usize)>,
    columns: std::ops::Range<c_int>,
    y: c_int,
    at: &Place,
) {
    let out = out as *mut T;
    let s = &at.settings;
    let color = s.color.map(|c| c as f32);
    let v = ((y as f64 + 0.5) / at.scale[1] - at.origin[1]) / at.frame[1];
    let pixel = 1.0 / at.scale[0].max(1e-9);
    for x in columns {
        let u = ((x as f64 + 0.5) / at.scale[0] - at.origin[0]) / at.frame[0];
        let (picture, rim) = weights(s, at.frame, [u, v], pixel);
        let (picture, rim) = (picture as f32, rim as f32);
        // SAFETY: as the caller promises.
        unsafe {
            let px: [f32; 4] = match &src {
                Some((p, xs, n)) if xs.contains(&x) => {
                    let p = (*p as *const T).add((x - xs.start) as usize * n);
                    let a = if *n == 4 { (*p.add(3)).get() } else { 1.0 };
                    let k = if at.premultiplied || *n < 4 { 1.0 } else { a };
                    [
                        (*p).get() * k,
                        (*p.add(1)).get() * k,
                        (*p.add(2)).get() * k,
                        a,
                    ]
                }
                _ => [0.0; 4],
            };
            let o = out.add((x - out_x1) as usize * 4);
            for c in 0..3 {
                *o.add(c) = T::put(px[c] * picture + color[c] * rim);
            }
            *o.add(3) = T::put(px[3] * picture + rim);
        }
    }
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    if out.channels != 4 {
        return kOfxStatErrImageFormat;
    }
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    if let Some(s) = &src
        && (s.depth != out.depth || s.channels < 3)
    {
        return kOfxStatErrImageFormat;
    }
    let (origin, frame) = fx.frame();
    let place = Place {
        settings: Settings::at(fx, time),
        origin,
        frame,
        scale: scale.map(|v| if v > 0.0 { v } else { 1.0 }),
        premultiplied: !src.as_ref().is_some_and(|s| s.unpremultiplied),
    };
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(o) = out.row(y) else { return };
        let source = src.as_ref().and_then(|s| {
            let r = s.row(y)?;
            Some((r as *const u8, s.bounds[0]..s.bounds[2], s.channels))
        });
        let cols = columns.clone();
        // SAFETY: rows the images have, columns inside the output's bounds
        // (and checked against the source's), in the images' own depth.
        unsafe {
            match out.depth {
                crate::ofx::Depth::Float => row::<f32>(o, out.bounds[0], source, cols, y, &place),
                crate::ofx::Depth::Short => row::<u16>(o, out.bounds[0], source, cols, y, &place),
                crate::ofx::Depth::Byte => row::<u8>(o, out.bounds[0], source, cols, y, &place),
            }
        }
    };
    for_rows(y1.max(out.bounds[1])..y2.min(out.bounds[3]), &fill);
    drop(src);
    drop(out);
    kOfxStatOK
}

// The center's handle on the preview.

/// What the overlay remembers: the handle taken (where the pen took it,
/// from the center), and the handle under the pen.
#[derive(Debug, Default)]
struct Overlay {
    drag: Option<[f64; 2]>,
    hover: bool,
}

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
    let (origin, frame) = fx.frame();
    let center = fx.param(CENTER).xy_at(time);
    let at = [
        origin[0] + center[0] * frame[0],
        origin[1] + center[1] * frame[1],
    ];
    if is(action, kOfxInteractActionDraw) {
        if let Some(d) = Draw::of(args) {
            d.handle(at, ps, state.hover || state.drag.is_some());
        }
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    let near = near_handle(pen, at, ps);
    if is(action, kOfxInteractActionPenDown) {
        if !near {
            return kOfxStatReplyDefault;
        }
        state.drag = Some([pen[0] - at[0], pen[1] - at[1]]);
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        match state.drag {
            Some(grip) => {
                let to = [
                    (pen[0] - grip[0] - origin[0]) / frame[0],
                    (pen[1] - grip[1] - origin[1]) / frame[1],
                ];
                fx.param(CENTER).set_xy(to);
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

// Presets.

/// The factory presets, VEGAS's: their names and values.
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    #[allow(clippy::type_complexity)]
    let preset = |name: &'static str,
                  color: [f64; 3],
                  shape: f64,
                  hole: bool,
                  [feather, border, size]: [f64; 3],
                  center: [f64; 2],
                  [repeat_x, repeat_y]: [f64; 2]| {
        let mut values = vec![
            ("color", num(&color)),
            ("shape", num(&[shape])),
            ("method", num(&[if hole { 1.0 } else { 0.0 }])),
            ("feather", num(&[feather])),
            ("border", num(&[border])),
            ("size", num(&[size])),
            ("center", num(&center)),
        ];
        if repeat_x > 1.0 {
            values.push(("repeatX", num(&[repeat_x])));
        }
        if repeat_y > 1.0 {
            values.push(("repeatY", num(&[repeat_y])));
        }
        Preset { name, values }
    };
    const BLACK: [f64; 3] = [0.0, 0.0, 0.0];
    const WHITE: [f64; 3] = [1.0, 1.0, 1.0];
    const MIDDLE: [f64; 2] = [0.5, 0.5];
    const ONCE: [f64; 2] = [1.0, 1.0];
    vec![
        preset(
            "Circle, Center",
            BLACK,
            0.0,
            false,
            [0.0, 0.0, 0.2],
            MIDDLE,
            ONCE,
        ),
        preset(
            "Square, Center, White Border",
            WHITE,
            5.0,
            false,
            [0.0, 0.1, 0.2],
            MIDDLE,
            ONCE,
        ),
        preset(
            "Diamond, Center, Blurred",
            [0.0, 1.0, 0.0],
            4.0,
            false,
            [0.3, 0.0, 0.2],
            MIDDLE,
            ONCE,
        ),
        preset(
            "Circle Cut Out, Center, Blurred",
            BLACK,
            0.0,
            true,
            [0.5, 0.0, 0.25],
            MIDDLE,
            ONCE,
        ),
        preset(
            "Two Rectangles",
            BLACK,
            3.0,
            false,
            [0.0, 0.0, 0.2],
            MIDDLE,
            [1.0, 2.0],
        ),
        preset(
            "Triangle Cut Out, Blue Border",
            [0.0, 0.0, 1.0],
            6.0,
            true,
            [0.0, 0.1, 0.1],
            MIDDLE,
            ONCE,
        ),
        preset(
            "Square, Top Left, Border",
            [1.0, 0.0, 0.0],
            5.0,
            false,
            [0.0, 0.05, 0.15],
            [0.25, 0.65],
            ONCE,
        ),
        preset(
            "Diamond Cut Out, Bottom Right",
            BLACK,
            4.0,
            true,
            [0.1, 0.0, 0.15],
            [0.75, 0.35],
            ONCE,
        ),
        preset(
            "Top Left Portrait",
            WHITE,
            2.0,
            false,
            [0.0, 0.02, 0.1],
            [0.13, 0.7],
            ONCE,
        ),
        preset(
            "Yellow Glowing Portrait",
            [1.0, 1.0, 0.0],
            0.0,
            false,
            [0.28, 0.06, 0.13],
            [0.75, 0.75],
            ONCE,
        ),
        preset(
            "Pop-up, Bottom Right",
            WHITE,
            0.0,
            false,
            [0.0, 0.04, 0.28],
            [1.0, 0.0],
            ONCE,
        ),
        preset(
            "Picture-In-Picture",
            BLACK,
            3.0,
            false,
            [0.0, 0.01, 0.2],
            [0.75, 0.25],
            ONCE,
        ),
        preset(
            "Grate",
            BLACK,
            4.0,
            true,
            [0.0, 0.08, 0.15],
            MIDDLE,
            [10.0, 10.0],
        ),
        preset(
            "TicTacToe",
            WHITE,
            5.0,
            true,
            [0.0, 0.04, 0.25],
            MIDDLE,
            [3.0, 3.0],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const HD: [f64; 2] = [1280.0, 720.0];

    fn settings() -> Settings {
        Settings {
            color: [0.0; 3],
            shape: Shape::Circle,
            hole: false,
            feather: 0.0,
            border: 0.0,
            repeat: [1, 1],
            size: 0.25,
            center: [0.5, 0.5],
        }
    }

    #[test]
    fn the_shapes_are_as_big_as_vegass() {
        // VEGAS's (Default): a circle as tall as the frame, near enough.
        let r = |shape, size| {
            (0..2000)
                .map(|i| i as f64)
                .find(|&x| inside(shape, [x, 0.0], size, HD) < 0.0)
                .unwrap_or(0.0)
        };
        assert!((r(Shape::Circle, 0.25) - 367.0).abs() < 2.0);
        assert!((r(Shape::Circle, 0.2) - 294.0).abs() < 2.0);
        // A square of 0.2: 263 pixels from its middle to its side.
        assert!((r(Shape::Square, 0.2) - 261.0).abs() < 4.0);
        // A rectangle has the frame's shape.
        assert!((r(Shape::Rectangle, 0.2) - 256.0).abs() < 2.0);
        // On its side, an oval is the frame upright.
        let up = (0..2000)
            .map(|i| i as f64)
            .find(|&y| inside(Shape::OvalSide, [0.0, y], 0.1, HD) < 0.0)
            .unwrap_or(0.0);
        assert!((r(Shape::OvalSide, 0.1) - 128.0).abs() < 2.0 && (up - 228.0).abs() < 2.0);
        // Triangles point where they say.
        let tri = |q| inside(Shape::Triangle(q), [0.0, 150.0], 0.1, HD) > 0.0;
        assert!(tri(0) && !tri(2));
    }

    #[test]
    fn the_border_is_inside_the_cut_and_the_hole_turns_it_around() {
        let mut s = settings();
        s.border = 0.1;
        s.color = [1.0; 3];
        // Its middle shows the picture; past the edge, nothing; 30 pixels
        // in, the border (64 pixels thick).
        let at = |x: f64| [0.5 + x / HD[0], 0.5];
        assert_eq!(weights(&s, HD, at(0.0), 1.0), (1.0, 0.0));
        assert_eq!(weights(&s, HD, at(400.0), 1.0), (0.0, 0.0));
        let (picture, rim) = weights(&s, HD, at(367.0 - 30.0), 1.0);
        assert!(picture < 1e-9 && (rim - 1.0).abs() < 1e-9);
        // Cut away: the picture around, the hole in the middle.
        s.hole = true;
        assert_eq!(weights(&s, HD, at(0.0), 1.0), (0.0, 0.0));
        assert_eq!(weights(&s, HD, at(400.0), 1.0), (1.0, 0.0));
    }

    #[test]
    fn repeated_shapes_share_the_frame() {
        let mut s = settings();
        s.shape = Shape::Rectangle;
        s.size = 0.2;
        s.repeat = [1, 2];
        // Two rectangles, a quarter and three quarters up.
        let shows = |v: f64| weights(&s, HD, [0.5, v], 1.0).0 > 0.5;
        assert!(shows(0.25) && shows(0.75) && !shows(0.5) && !shows(0.05));
    }

    #[test]
    fn the_presets_are_vegass() {
        let p = presets();
        assert_eq!(p.len(), 14);
        assert_eq!(p[4].name, "Two Rectangles");
        assert!(
            p[4].values
                .contains(&("repeatY", PresetValue::Num(vec![2.0])))
        );
        assert!(!p[4].values.iter().any(|(n, _)| *n == "repeatX"));
        let grate = &p[12];
        assert_eq!(grate.name, "Grate");
        assert!(
            grate
                .values
                .contains(&("repeatX", PresetValue::Num(vec![10.0])))
        );
    }
}
