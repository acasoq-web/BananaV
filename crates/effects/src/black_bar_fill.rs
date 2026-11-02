//! Black Bar Fill: VEGAS's. A picture of other proportions (a portrait
//! video in a landscape frame) shown over itself enlarged and blurred, so
//! no bars are left: the foreground (the picture, moved and scaled, with a
//! soft black shadow) over the background (the picture enlarged, moved
//! and blurred), each placed by a crosshair on the preview.
//!
//! VEGAS's math, measured on its pictures: both layers are the effect's
//! picture (an event's effects get its framed picture, the bars
//! transparent) scaled about the frame's middle, which goes to their
//! Location; the background is blurred by VEGAS's Gaussian
//! (`recursive_gauss`), its radius Blur × the frame's longer side — up and
//! down times how many times as wide as tall the pixels show —, its edges
//! held (the frame's edge pixels go on; none at 0); the shadow is the
//! foreground's alpha blurred by 0.022 × the frame's width × Shadow Blur,
//! black at 0.95 (none at 0).
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::Mutex;

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::kit::Buf;
use crate::layer_dimensionality::{blur, box_radius};
use crate::ofx::{Draw, Fx, Props, for_rows, interact_props, is, redraw, suites};
use crate::picture_in_picture::sample;
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.BlackBarFill";

const FOREGROUND: &CStr = c"foreground";
const FG_LOCATION: &CStr = c"fgLocation";
const FG_SCALE: &CStr = c"fgScale";
const SHADOW_BLUR: &CStr = c"shadowBlur";
const BACKGROUND: &CStr = c"background";
const BG_LOCATION: &CStr = c"bgLocation";
const BG_SCALE: &CStr = c"bgScale";
const BG_BLUR: &CStr = c"bgBlur";

/// The shadow's: its sigma, a part of the frame's width, per unit of
/// Shadow Blur.
pub(crate) const SHADOW: f64 = 0.022;
/// How dark the shadow is.
pub(crate) const SHADOW_OPACITY: f32 = 0.95;

const DEFAULT_SHADOW: f64 = 0.2;
const DEFAULT_BG_SCALE: f64 = 3.185;
const DEFAULT_BLUR: f64 = 0.071;

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    /// Where the foreground's middle goes (0–1 across and up).
    pub fg_location: [f64; 2],
    pub fg_scale: f64,
    pub shadow_blur: f64,
    pub bg_location: [f64; 2],
    pub bg_scale: f64,
    pub bg_blur: f64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            fg_location: [0.5; 2],
            fg_scale: 1.0,
            shadow_blur: DEFAULT_SHADOW,
            bg_location: [0.5; 2],
            bg_scale: DEFAULT_BG_SCALE,
            bg_blur: DEFAULT_BLUR,
        }
    }
}

/// A 3 × 3 matrix, rows.
pub(crate) type M3 = [[f64; 3]; 3];

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        let d = Settings::default();
        Settings {
            fg_location: v.array(FG_LOCATION, d.fg_location),
            fg_scale: v.f64(FG_SCALE, d.fg_scale),
            shadow_blur: v.f64(SHADOW_BLUR, d.shadow_blur),
            bg_location: v.array(BG_LOCATION, d.bg_location),
            bg_scale: v.f64(BG_SCALE, d.bg_scale),
            bg_blur: v.f64(BG_BLUR, d.bg_blur),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            fg_location: fx.param(FG_LOCATION).xy_at(time),
            fg_scale: fx.param(FG_SCALE).f64_at(time),
            shadow_blur: fx.param(SHADOW_BLUR).f64_at(time),
            bg_location: fx.param(BG_LOCATION).xy_at(time),
            bg_scale: fx.param(BG_SCALE).f64_at(time),
            bg_blur: fx.param(BG_BLUR).f64_at(time),
        }
    }

    /// The shadow's blur (canonical units) on a frame `width` wide.
    pub(crate) fn shadow_sigma(&self, width: f64) -> f64 {
        SHADOW * width * self.shadow_blur.max(0.0)
    }
}

/// The background's blur radii across and down, in pixels, on a frame
/// `size` pixels whose pixels show `shape` times as wide as tall: VEGAS's,
/// `blur` × the longer side, up and down times the pixels' shape (none at
/// 0).
pub(crate) fn background_radii(blur: f64, size: [f64; 2], shape: f64) -> Option<[f64; 2]> {
    let r = size[0].max(size[1]) * blur.max(0.0);
    (r > 0.0).then_some([r, r * shape])
}

/// A layer's map from the frame back to the picture (both 0–1 across and
/// up): the picture scaled by `scale` about the frame's middle, which goes
/// to `location`.
pub(crate) fn inverse(location: [f64; 2], scale: f64) -> M3 {
    let k = 1.0 / scale.max(1e-3);
    [
        [k, 0.0, 0.5 - location[0] * k],
        [0.0, k, 0.5 - location[1] * k],
        [0.0, 0.0, 1.0],
    ]
}

/// How many samples a pixel of a layer scaled by `scale` averages across
/// and up: more as it shrinks.
pub(crate) fn samples(scale: f64) -> u32 {
    (1.0 / scale.max(1e-3)).ceil().clamp(1.0, 4.0) as u32
}

fn apply(m: &M3, p: [f64; 2]) -> [f64; 2] {
    [
        m[0][0] * p[0] + m[0][1] * p[1] + m[0][2],
        m[1][0] * p[0] + m[1][1] * p[1] + m[1][2],
    ]
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
        // The layers may come from anywhere in the picture.
        let time = args.dbl(kOfxPropTime, 0);
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        let roi = fx
            .clip(kOfxImageEffectSimpleSourceClipName)
            .rod(time)
            .unwrap_or(roi);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &roi);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionGetRegionOfDefinition) {
        // The whole frame: the background fills it.
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
    p.set_str(kOfxPropLabel, c"Black Bar Fill");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Utility");
    p.set_str(
        kOfxPropPluginDescription,
        c"Fills the bars around a picture of other proportions (a portrait video in a landscape frame) with the picture itself, enlarged and blurred behind it.",
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

    let group = |name: &CStr, label: &CStr| {
        let p = fx.define_param(kOfxParamTypeGroup, name, label);
        p.set_int(kOfxParamPropGroupOpen, 1);
    };
    let position = |name: &CStr, parent: &CStr, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble2D, name, c"Location");
        p.set_str(
            kOfxParamPropDoubleType,
            kOfxParamDoubleTypeNormalisedXYAbsolute,
        );
        p.set_dbls(kOfxParamPropDefault, &[0.5, 0.5]);
        p.set_dbls(kOfxParamPropIncrement, &[0.01]);
        p.set_int(kOfxParamPropDigits, 3);
        p.set_str(kOfxParamPropHint, hint);
        p.set_str(kOfxParamPropParent, parent);
    };
    let number =
        |name: &CStr, label: &CStr, parent: &CStr, [min, max, default]: [f64; 3], hint: &CStr| {
            let p = fx.define_param(kOfxParamTypeDouble, name, label);
            p.set_dbl(kOfxParamPropDefault, default);
            p.set_dbl(kOfxParamPropMin, min);
            p.set_dbl(kOfxParamPropMax, max);
            p.set_dbl(kOfxParamPropDisplayMin, min);
            p.set_dbl(kOfxParamPropDisplayMax, max);
            p.set_dbl(kOfxParamPropIncrement, 0.01);
            p.set_int(kOfxParamPropDigits, 3);
            p.set_str(kOfxParamPropHint, hint);
            p.set_str(kOfxParamPropParent, parent);
        };
    group(FOREGROUND, c"Foreground");
    position(
        FG_LOCATION,
        FOREGROUND,
        c"Where the picture's middle goes, 0–1 across and up the frame.",
    );
    number(
        FG_SCALE,
        c"Scale",
        FOREGROUND,
        [0.0, 10.0, 1.0],
        c"How big the picture is: 1 as it comes.",
    );
    number(
        SHADOW_BLUR,
        c"Shadow Blur",
        FOREGROUND,
        [0.0, 1.0, DEFAULT_SHADOW],
        c"How soft the shadow around the picture is (0: no shadow).",
    );
    group(BACKGROUND, c"Background");
    position(
        BG_LOCATION,
        BACKGROUND,
        c"Where the middle of the picture behind goes, 0–1 across and up the frame.",
    );
    number(
        BG_SCALE,
        c"Scale",
        BACKGROUND,
        [0.0, 10.0, DEFAULT_BG_SCALE],
        c"How much the picture behind is enlarged.",
    );
    number(
        BG_BLUR,
        c"Blur",
        BACKGROUND,
        [0.0, 1.0, DEFAULT_BLUR],
        c"How blurred the picture behind is.",
    );
}

/// The picture through `inverse`, `n` × `n` samples a pixel: a layer the
/// frame's size at render scale.
struct Grid<'a> {
    src: Option<&'a crate::ofx::Image>,
    premultiplied: bool,
    /// The frame's first pixel and its size in pixels.
    first: [c_int; 2],
    size: [usize; 2],
    origin: [f64; 2],
    frame: [f64; 2],
    scale: [f64; 2],
    par: f64,
}

impl Grid<'_> {
    fn pixel(&self, m: &M3, n: u32, x: usize, y: usize) -> [f32; 4] {
        let Some(src) = self.src else {
            return [0.0; 4];
        };
        let mut v = [0.0f32; 4];
        for j in 0..n {
            for i in 0..n {
                let ox = (self.first[0] + x as c_int) as f64 + (i as f64 + 0.5) / n as f64;
                let oy = (self.first[1] + y as c_int) as f64 + (j as f64 + 0.5) / n as f64;
                let p = [
                    (ox * self.par / self.scale[0] - self.origin[0]) / self.frame[0],
                    (oy / self.scale[1] - self.origin[1]) / self.frame[1],
                ];
                let q = apply(m, p);
                let at = [
                    (self.origin[0] + q[0] * self.frame[0]) * self.scale[0] / self.par,
                    (self.origin[1] + q[1] * self.frame[1]) * self.scale[1],
                ];
                let c = sample(src, self.premultiplied, at);
                for (a, b) in v.iter_mut().zip(c) {
                    *a += b;
                }
            }
        }
        let k = 1.0 / (n * n) as f32;
        v.map(|c| c * k)
    }

    /// The layer through `m`, row by row on the host's threads.
    fn layer(&self, m: &M3, n: u32) -> Vec<[f32; 4]> {
        let [w, h] = self.size;
        let rows: Vec<Mutex<Vec<[f32; 4]>>> = (0..h).map(|_| Mutex::new(Vec::new())).collect();
        for_rows(0..h as c_int, &|y| {
            let row: Vec<[f32; 4]> = (0..w).map(|x| self.pixel(m, n, x, y as usize)).collect();
            if let Ok(mut r) = rows[y as usize].lock() {
                *r = row;
            }
        });
        rows.into_iter()
            .flat_map(|r| r.into_inner().unwrap_or_default())
            .collect()
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
        && s.depth != out.depth
    {
        return kOfxStatErrImageFormat;
    }
    let s = Settings::at(fx, time);
    let (origin, frame) = fx.frame();
    let par = out.par;
    let grid = Grid {
        src: src.as_ref(),
        premultiplied: src
            .as_ref()
            .is_some_and(|i| i.channels < 4 || !i.unpremultiplied),
        first: [
            (origin[0] * scale[0] / par).round() as c_int,
            (origin[1] * scale[1]).round() as c_int,
        ],
        size: [
            ((frame[0] * scale[0] / par).round() as usize).max(1),
            ((frame[1] * scale[1]).round() as usize).max(1),
        ],
        origin,
        frame,
        scale,
        par,
    };
    let [w, h] = grid.size;
    let shadow_sigma = s.shadow_sigma(frame[0]);
    let px = scale[0] / par;
    // The background over the whole frame, blurred with its edges held.
    let mut bg = grid.layer(&inverse(s.bg_location, s.bg_scale), samples(s.bg_scale));
    let shape = par * scale[1] / scale[0];
    if let Some(radii) = background_radii(s.bg_blur, [w as f64, h as f64], shape) {
        let mut b = Buf {
            bounds: [0, 0, w as c_int, h as c_int],
            data: bg,
        };
        crate::recursive_gauss::blur(&mut b, radii);
        bg = b.data;
    }
    let fg = grid.layer(&inverse(s.fg_location, s.fg_scale), samples(s.fg_scale));
    // The foreground's shadow: its alpha blurred, nothing beyond the frame.
    let shadow = (s.shadow_blur > 0.0).then(|| {
        let sigma = shadow_sigma * px;
        let m = box_radius(sigma) * 3 + 2;
        let fw = w + 2 * m;
        let mut a = vec![0.0f32; fw * (h + 2 * m)];
        for y in 0..h {
            for x in 0..w {
                a[(y + m) * fw + x + m] = fg[y * w + x][3];
            }
        }
        blur(&mut a, fw, h + 2 * m, sigma);
        (a, fw, m)
    });
    let out_straight = out.channels == 4 && out.unpremultiplied;
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(row) = out.row(y) else { return };
        for x in columns.clone() {
            let (gx, gy) = (x - grid.first[0], y - grid.first[1]);
            let inside = gx >= 0 && gy >= 0 && (gx as usize) < w && (gy as usize) < h;
            let mut v = if inside {
                let (gx, gy) = (gx as usize, gy as usize);
                let i = gy * w + gx;
                let (f, b) = (fg[i], bg[i]);
                let a = shadow.as_ref().map_or(0.0, |(field, fw, m)| {
                    field[(gy + m) * fw + gx + m] * SHADOW_OPACITY
                });
                let under = [
                    b[0] * (1.0 - a),
                    b[1] * (1.0 - a),
                    b[2] * (1.0 - a),
                    b[3] * (1.0 - a) + a,
                ];
                std::array::from_fn(|c| f[c] + under[c] * (1.0 - f[3]))
            } else {
                [0.0; 4]
            };
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

// The crosshairs on the preview.

/// A layer's crosshair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Foreground,
    Background,
}

impl Mark {
    fn param(self) -> &'static CStr {
        match self {
            Mark::Foreground => FG_LOCATION,
            Mark::Background => BG_LOCATION,
        }
    }

    fn location(self, s: &Settings) -> [f64; 2] {
        match self {
            Mark::Foreground => s.fg_location,
            Mark::Background => s.bg_location,
        }
    }
}

/// An overlay's own state: the crosshair taken (where the pen went down,
/// 0–1 across and up, and its location then), the one under the pen.
#[derive(Debug, Default)]
struct Overlay {
    drag: Option<(Mark, [f64; 2], [f64; 2])>,
    hover: Option<Mark>,
}

/// How near the pen must be, and the crosshairs' sizes, in screen pixels.
const REACH: f64 = 11.0;
const RING: f64 = 7.0;
const TICK: f64 = 12.0;
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const RIM: [f32; 4] = [0.0, 0.0, 0.0, 0.6];

fn canonical(origin: [f64; 2], size: [f64; 2], p: [f64; 2]) -> [f64; 2] {
    [origin[0] + p[0] * size[0], origin[1] + p[1] * size[1]]
}

/// The crosshair under the pen `p` (the foreground's first: it is drawn
/// over the background's); `ps` a screen pixel in canonical units.
fn hit(s: &Settings, origin: [f64; 2], size: [f64; 2], p: [f64; 2], ps: [f64; 2]) -> Option<Mark> {
    [Mark::Foreground, Mark::Background].into_iter().find(|m| {
        let c = canonical(origin, size, m.location(s));
        ((p[0] - c[0]) / ps[0]).hypot((p[1] - c[1]) / ps[1]) <= REACH
    })
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
    let s = Settings::at(fx, time);
    if is(action, kOfxInteractActionDraw) {
        draw(args, state, &s, origin, size, ps);
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    let unit = [
        (pen[0] - origin[0]) / size[0],
        (pen[1] - origin[1]) / size[1],
    ];
    if is(action, kOfxInteractActionPenDown) {
        let Some(mark) = hit(&s, origin, size, pen, ps) else {
            return kOfxStatReplyDefault;
        };
        state.drag = Some((mark, unit, mark.location(&s)));
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        let Some((mark, from, start)) = state.drag else {
            let hover = hit(&s, origin, size, pen, ps);
            if hover != state.hover {
                state.hover = hover;
                redraw(handle);
            }
            return kOfxStatReplyDefault;
        };
        fx.param(mark.param())
            .set_xy([start[0] + unit[0] - from[0], start[1] + unit[1] - from[1]]);
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

/// A circle of `r` screen pixels around `c`.
fn circle(c: [f64; 2], r: f64, ps: [f64; 2]) -> Vec<[f64; 2]> {
    (0..32)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / 32.0;
            [c[0] + r * ps[0] * t.cos(), c[1] + r * ps[1] * t.sin()]
        })
        .collect()
}

/// Each layer's crosshair, VEGAS's: a ring and four ticks; the background's
/// under the foreground's.
fn draw(
    args: Props,
    state: &Overlay,
    s: &Settings,
    origin: [f64; 2],
    size: [f64; 2],
    ps: [f64; 2],
) {
    let Some(d) = Draw::of(args) else { return };
    for mark in [Mark::Background, Mark::Foreground] {
        let c = canonical(origin, size, mark.location(s));
        let hot = state.drag.is_some_and(|(m, ..)| m == mark) || state.hover == Some(mark);
        let r = if hot { RING + 1.5 } else { RING };
        let ticks: Vec<[f64; 2]> = [[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0]]
            .iter()
            .flat_map(|[x, y]| {
                [
                    [c[0] + x * (r - 3.0) * ps[0], c[1] + y * (r - 3.0) * ps[1]],
                    [c[0] + x * TICK * ps[0], c[1] + y * TICK * ps[1]],
                ]
            })
            .collect();
        for (colour, width) in [(RIM, 3.0), (WHITE, 1.5)] {
            d.colour(colour);
            d.width(width);
            d.outline(&circle(c, r, ps));
            d.lines(&ticks);
        }
    }
}

// Presets.

/// The factory presets: VEGAS's.
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    let preset =
        |name: &'static str, values: Vec<(&'static str, PresetValue)>| Preset { name, values };
    vec![
        preset("Foreground Scaled", vec![("fgScale", num(&[0.95]))]),
        preset(
            "Foreground Left, No Shadow",
            vec![
                ("fgLocation", num(&[0.158, 0.5])),
                ("shadowBlur", num(&[0.0])),
            ],
        ),
        preset(
            "Foreground Right, No Shadow",
            vec![
                ("fgLocation", num(&[0.842, 0.5])),
                ("shadowBlur", num(&[0.0])),
            ],
        ),
        preset("Foreground No Shadow", vec![("shadowBlur", num(&[0.0]))]),
        preset(
            "Foreground Maximum Shadow",
            vec![("shadowBlur", num(&[1.0]))],
        ),
        preset("Background No Blur", vec![("bgBlur", num(&[0.0]))]),
        preset("Background Maximum Blur", vec![("bgBlur", num(&[1.0]))]),
        preset("Background Maximum Scale", vec![("bgScale", num(&[10.0]))]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layer_is_the_picture_scaled_about_the_middle() {
        // At its location, the picture's middle.
        let m = inverse([0.158, 0.5], 1.0);
        let q = apply(&m, [0.158, 0.5]);
        assert!((q[0] - 0.5).abs() < 1e-12 && (q[1] - 0.5).abs() < 1e-12);
        // The background 3.185 times: the frame's edge shows the picture
        // 0.157 from its middle.
        let m = inverse([0.5, 0.5], 3.185);
        let q = apply(&m, [1.0, 0.5]);
        assert!((q[0] - (0.5 + 0.5 / 3.185)).abs() < 1e-12);
        assert_eq!(samples(1.0), 1);
        assert_eq!(samples(0.95), 2);
        assert_eq!(samples(0.1), 4);
    }

    #[test]
    fn the_blurs_are_vegass() {
        // VEGAS's prints, 654 × 368: Blur 0.071 is a sigma of 14.25 pixels
        // (its radius 46.4: 14.39), 1.0 of 200 (196.9); none at 0. Shadow
        // Blur 0.2 of about 3, 1.0 of 15.
        let d = Settings::default();
        let [r, up] = background_radii(d.bg_blur, [654.0, 368.0], 1.0).unwrap();
        assert!((r - 46.434).abs() < 1e-9 && r == up, "{r}");
        for (blur, measured) in [(0.071, 14.25), (1.0, 200.0)] {
            let [r, _] = background_radii(blur, [654.0, 368.0], 1.0).unwrap();
            let sigma = crate::recursive_gauss::sigma(r);
            assert!((sigma - measured).abs() < 3.2, "{blur}: {sigma}");
        }
        assert_eq!(background_radii(0.0, [654.0, 368.0], 1.0), None);
        // Wide pixels: farther up and down.
        assert_eq!(
            background_radii(0.5, [100.0, 80.0], 2.0),
            Some([50.0, 100.0])
        );
        assert!((d.shadow_sigma(654.0) - 2.9).abs() < 0.3);
        let max = Settings {
            shadow_blur: 1.0,
            ..d
        };
        assert!((max.shadow_sigma(654.0) - 14.7).abs() < 0.6);
    }

    #[test]
    fn the_presets_are_vegass() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            [
                "Foreground Scaled",
                "Foreground Left, No Shadow",
                "Foreground Right, No Shadow",
                "Foreground No Shadow",
                "Foreground Maximum Shadow",
                "Background No Blur",
                "Background Maximum Blur",
                "Background Maximum Scale"
            ]
        );
    }
}
