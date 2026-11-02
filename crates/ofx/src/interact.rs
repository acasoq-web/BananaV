//! Overlays: a plug-in's on-screen controls over the picture (OFX
//! interacts). The plug-in draws them with the draw suite — lines, shapes
//! and text in canonical coordinates — and answers the pen; BananaV draws
//! what it says over the preview. Overlays that only draw with OpenGL (the
//! first version, `kOfxImageEffectPluginPropOverlayInteractV1`) are not
//! shown: BananaV draws with its own graphics, not OpenGL.
#![allow(unsafe_code)]
// OpenFX's names (`kOfxDrawPrimitiveLines`) are matched as they are.
#![allow(non_upper_case_globals)]

use std::ffi::{CStr, c_char, c_float, c_int, c_void};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};

use bv_ofx_sys::names::*;
use bv_ofx_sys::{
    OfxDrawContextHandle, OfxDrawLineStipplePattern, OfxDrawPrimitive, OfxDrawSuiteV1,
    OfxInteractHandle, OfxPluginEntryPoint, OfxPointD, OfxPropertySetHandle, OfxRGBAColourF,
    OfxStandardColour, OfxStatus, kOfxDrawPrimitiveEllipse, kOfxDrawPrimitiveLineLoop,
    kOfxDrawPrimitiveLineStrip, kOfxDrawPrimitiveLines, kOfxDrawPrimitivePolygon,
    kOfxDrawPrimitiveRectangle, kOfxStandardColourOverlayActive,
    kOfxStandardColourOverlayBackground, kOfxStandardColourOverlayDeselected,
    kOfxStandardColourOverlayMarqueeBG, kOfxStandardColourOverlayMarqueeFG,
    kOfxStandardColourOverlaySelected, kOfxStandardColourOverlayText,
};

use crate::props::{PropSet, SetKind};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// How lines are drawn: their color (0–1, straight alpha), width in
/// screen pixels and pattern (`kOfxDrawLineStipplePattern…`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pen {
    pub colour: [f32; 4],
    pub width: f32,
    pub stipple: i32,
}

impl Default for Pen {
    fn default() -> Pen {
        Pen {
            colour: [1.0, 1.0, 1.0, 1.0],
            width: 1.0,
            stipple: 0,
        }
    }
}

/// Something an overlay draws, in canonical coordinates (x to the right,
/// y up, the picture's full-size pixels).
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// Separate lines, the points in pairs.
    Lines { points: Vec<[f64; 2]>, pen: Pen },
    /// Connected lines; `closed`: the last point joins the first.
    Strip {
        points: Vec<[f64; 2]>,
        closed: bool,
        pen: Pen,
    },
    /// A filled rectangle between two opposite corners.
    Rect {
        a: [f64; 2],
        b: [f64; 2],
        colour: [f32; 4],
    },
    /// A filled polygon.
    Polygon {
        points: Vec<[f64; 2]>,
        colour: [f32; 4],
    },
    /// An ellipse's outline inside the rectangle between two corners.
    Ellipse { a: [f64; 2], b: [f64; 2], pen: Pen },
    /// Text at a point, aligned as `align` says
    /// (`kOfxDrawTextAlignment…` flags).
    Text {
        text: String,
        at: [f64; 2],
        align: i32,
        colour: [f32; 4],
    },
    /// What an overlay drew with OpenGL itself: a picture of the whole
    /// view, premultiplied RGBA, rows from the top.
    Picture {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
}

/// How the picture shows on screen, for an overlay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// Frames, the effect's time.
    pub time: f64,
    /// The picture's pixels over its full size.
    pub render_scale: f64,
    /// A screen pixel, in canonical units (x, y).
    pub pixel_scale: [f64; 2],
    /// The color around the picture (0–1).
    pub background: [f64; 3],
    /// The view's size in screen pixels.
    pub viewport: [i32; 2],
    /// Canonical coordinates to the view's (0–1 across, 0–1 down), for
    /// overlays that draw with OpenGL themselves (OpenFX's first kind):
    /// their projection.
    pub to_frame: Option<[[f64; 3]; 2]>,
}

/// What the pen (the mouse) did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenAction {
    Down,
    Motion,
    Up,
}

impl PenAction {
    fn ofx_name(self) -> &'static CStr {
        match self {
            PenAction::Down => kOfxInteractActionPenDown,
            PenAction::Motion => kOfxInteractActionPenMotion,
            PenAction::Up => kOfxInteractActionPenUp,
        }
    }
}

/// An overlay asked to be drawn again since the last call.
static REDRAW: AtomicBool = AtomicBool::new(false);

/// The modifier keys held, as overlays hear them
/// (`kBananaVInteractPropModifiers`).
static MODIFIERS: AtomicU32 = AtomicU32::new(0);

/// Shift held, for [`set_modifiers`].
pub const SHIFT: u32 = 1;
/// Control held.
pub const CONTROL: u32 = 2;
/// Alt held.
pub const ALT: u32 = 4;

/// The modifier keys held now ([`SHIFT`], [`CONTROL`], [`ALT`]): the
/// interface tells as its keys change, overlays hear them with every
/// action.
pub fn set_modifiers(keys: u32) {
    MODIFIERS.store(keys, Ordering::Relaxed);
}

/// Whether an overlay asked to be drawn again since the last call (and
/// forgets).
pub fn take_redraw() -> bool {
    REDRAW.swap(false, Ordering::Relaxed)
}

const DRAW_MAGIC: u64 = 0x4256_4f46_5844_5257; // "BVOFXDRW"

/// What a plug-in draws during one draw action.
#[repr(C)]
struct DrawContext {
    magic: u64,
    state: Mutex<(Pen, Vec<Mark>)>,
}

impl DrawContext {
    /// The context behind a handle the plug-in gave, if it is one.
    ///
    /// # Safety
    /// `h` is null or can be read; a context lives through its draw action.
    unsafe fn from_handle<'a>(h: OfxDrawContextHandle) -> Option<&'a DrawContext> {
        if h.is_null() {
            return None;
        }
        let c = h as *const DrawContext;
        // SAFETY: as the caller promises; the magic number tells it apart.
        unsafe { ((*c).magic == DRAW_MAGIC).then(|| &*c) }
    }
}

/// The colors BananaV's overlays use, as standard colors ask.
fn standard_colour(c: OfxStandardColour) -> Option<[f32; 4]> {
    Some(match c {
        kOfxStandardColourOverlayBackground => [0.0, 0.0, 0.0, 0.5],
        // BananaV's yellow for what is active or selected.
        kOfxStandardColourOverlayActive => [0.97, 0.79, 0.28, 1.0],
        kOfxStandardColourOverlaySelected => [1.0, 0.9, 0.45, 1.0],
        kOfxStandardColourOverlayDeselected => [0.85, 0.85, 0.85, 1.0],
        kOfxStandardColourOverlayMarqueeFG => [1.0, 1.0, 1.0, 1.0],
        kOfxStandardColourOverlayMarqueeBG => [0.0, 0.0, 0.0, 1.0],
        kOfxStandardColourOverlayText => [1.0, 1.0, 1.0, 1.0],
        _ => return None,
    })
}

unsafe extern "C" fn get_colour(
    context: OfxDrawContextHandle,
    std_colour: OfxStandardColour,
    colour: *mut OfxRGBAColourF,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    if unsafe { DrawContext::from_handle(context) }.is_none() {
        return kOfxStatFailed;
    }
    let (Some([r, g, b, a]), false) = (standard_colour(std_colour), colour.is_null()) else {
        return kOfxStatErrValue;
    };
    // SAFETY: where the plug-in wants the color.
    unsafe { *colour = OfxRGBAColourF { r, g, b, a } };
    kOfxStatOK
}

unsafe extern "C" fn set_colour(
    context: OfxDrawContextHandle,
    colour: *const OfxRGBAColourF,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(c) = (unsafe { DrawContext::from_handle(context) }) else {
        return kOfxStatFailed;
    };
    if colour.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: the plug-in's color.
    let OfxRGBAColourF { r, g, b, a } = unsafe { *colour };
    lock(&c.state).0.colour = [r, g, b, a];
    kOfxStatOK
}

unsafe extern "C" fn set_line_width(context: OfxDrawContextHandle, width: c_float) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(c) = (unsafe { DrawContext::from_handle(context) }) else {
        return kOfxStatFailed;
    };
    lock(&c.state).0.width = width.clamp(0.0, 64.0);
    kOfxStatOK
}

unsafe extern "C" fn set_line_stipple(
    context: OfxDrawContextHandle,
    pattern: OfxDrawLineStipplePattern,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(c) = (unsafe { DrawContext::from_handle(context) }) else {
        return kOfxStatFailed;
    };
    if !(0..=4).contains(&pattern) {
        return kOfxStatErrValue;
    }
    lock(&c.state).0.stipple = pattern;
    kOfxStatOK
}

unsafe extern "C" fn draw(
    context: OfxDrawContextHandle,
    primitive: OfxDrawPrimitive,
    points: *const OfxPointD,
    point_count: c_int,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(c) = (unsafe { DrawContext::from_handle(context) }) else {
        return kOfxStatFailed;
    };
    let n = usize::try_from(point_count).unwrap_or(0);
    if points.is_null() || n == 0 {
        return kOfxStatErrValue;
    }
    // SAFETY: the plug-in's `point_count` points.
    let pts: Vec<[f64; 2]> = unsafe { std::slice::from_raw_parts(points, n) }
        .iter()
        .map(|p| [p.x, p.y])
        .collect();
    let mut st = lock(&c.state);
    let pen = st.0;
    let mark = match primitive {
        kOfxDrawPrimitiveLines if n >= 2 && n % 2 == 0 => Mark::Lines { points: pts, pen },
        kOfxDrawPrimitiveLineStrip if n >= 2 => Mark::Strip {
            points: pts,
            closed: false,
            pen,
        },
        kOfxDrawPrimitiveLineLoop if n >= 2 => Mark::Strip {
            points: pts,
            closed: true,
            pen,
        },
        kOfxDrawPrimitiveRectangle if n == 2 => Mark::Rect {
            a: pts[0],
            b: pts[1],
            colour: pen.colour,
        },
        kOfxDrawPrimitivePolygon if n >= 3 => Mark::Polygon {
            points: pts,
            colour: pen.colour,
        },
        kOfxDrawPrimitiveEllipse if n == 2 => Mark::Ellipse {
            a: pts[0],
            b: pts[1],
            pen,
        },
        _ => return kOfxStatErrValue,
    };
    st.1.push(mark);
    kOfxStatOK
}

unsafe extern "C" fn draw_text(
    context: OfxDrawContextHandle,
    text: *const c_char,
    pos: *const OfxPointD,
    alignment: c_int,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(c) = (unsafe { DrawContext::from_handle(context) }) else {
        return kOfxStatFailed;
    };
    if text.is_null() || pos.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: the plug-in's text and point.
    let (text, at) = unsafe {
        (
            CStr::from_ptr(text).to_string_lossy().into_owned(),
            [(*pos).x, (*pos).y],
        )
    };
    let mut st = lock(&c.state);
    let colour = st.0.colour;
    st.1.push(Mark::Text {
        text,
        at,
        align: alignment,
        colour,
    });
    kOfxStatOK
}

/// The draw suite (version 1).
pub(crate) static DRAW_SUITE: OfxDrawSuiteV1 = OfxDrawSuiteV1 {
    get_colour,
    set_colour,
    set_line_width,
    set_line_stipple,
    draw,
    draw_text,
};

// The interact suite.

unsafe extern "C" fn interact_swap_buffers(_i: OfxInteractHandle) -> OfxStatus {
    kOfxStatOK
}

unsafe extern "C" fn interact_redraw(_i: OfxInteractHandle) -> OfxStatus {
    REDRAW.store(true, Ordering::Relaxed);
    kOfxStatOK
}

unsafe extern "C" fn interact_get_property_set(
    i: OfxInteractHandle,
    props: *mut OfxPropertySetHandle,
) -> OfxStatus {
    // An interact's handle is its property set.
    // SAFETY: the plug-in's handle.
    let Some(set) = (unsafe { PropSet::from_handle(i as OfxPropertySetHandle) }) else {
        return kOfxStatErrBadHandle;
    };
    if props.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: where the plug-in wants the set.
    unsafe { *props = set.handle() };
    kOfxStatOK
}

pub(crate) static INTERACT_SUITE: bv_ofx_sys::OfxInteractSuiteV1 = bv_ofx_sys::OfxInteractSuiteV1 {
    interact_swap_buffers,
    interact_redraw,
    interact_get_property_set,
};

/// An instance's overlay: the plug-in's entry for it, its descriptor and
/// its instance (whose property set is its handle).
#[derive(Debug)]
pub(crate) struct Interact {
    /// The plug-in's binary (its code is called through the guard).
    binary: std::sync::Arc<crate::plugin::Binary>,
    entry: OfxPluginEntryPoint,
    /// It draws with OpenGL itself (OpenFX's first kind of overlay).
    opengl: bool,
    _descriptor: Box<PropSet>,
    props: Box<PropSet>,
}

impl Interact {
    /// Describes and creates the overlay whose entry point is `entry`, for
    /// the effect instance `effect` (`opengl`: it draws with OpenGL); `None`
    /// when the plug-in refuses.
    pub(crate) fn new(
        binary: std::sync::Arc<crate::plugin::Binary>,
        entry: OfxPluginEntryPoint,
        effect: *const c_void,
        opengl: bool,
    ) -> Option<Interact> {
        let descriptor = Box::new(PropSet::new(SetKind::Interact));
        descriptor.define_int(kOfxInteractPropHasAlpha, Some(1), &[1]);
        descriptor.define_int(kOfxInteractPropBitDepth, Some(1), &[8]);
        // SAFETY: the plug-in's overlay entry, with a descriptor.
        let status = unsafe {
            crate::guard::entry(
                &binary,
                entry,
                kOfxActionDescribe,
                descriptor.handle() as *const c_void,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if !matches!(status, kOfxStatOK | kOfxStatReplyDefault) {
            return None;
        }
        let props = Box::new(PropSet::new(SetKind::Interact));
        props.copy_from(&descriptor);
        props.define_ptr(kOfxPropEffectInstance, Some(1));
        props.set_ptr(kOfxPropEffectInstance, effect);
        props.define_ptr(kOfxPropInstanceData, Some(1));
        props.define_dbl(kOfxInteractPropPixelScale, Some(2), &[1.0, 1.0]);
        props.define_dbl(kOfxInteractPropBackgroundColour, Some(3), &[0.0, 0.0, 0.0]);
        props.define_int(kOfxInteractPropViewportSize, Some(2), &[640, 360]);
        props.define_dbl(
            kOfxInteractPropSuggestedColour,
            Some(3),
            &[0.97, 0.79, 0.28],
        );
        if !props.has(kOfxInteractPropSlaveToParam) {
            props.define_str(kOfxInteractPropSlaveToParam, None, &[]);
        }
        let interact = Interact {
            binary,
            entry,
            opengl,
            _descriptor: descriptor,
            props,
        };
        let status = interact.call(kOfxActionCreateInstance, None);
        matches!(status, kOfxStatOK | kOfxStatReplyDefault).then_some(interact)
    }

    fn call(&self, action: &CStr, args: Option<&PropSet>) -> OfxStatus {
        let i = args.map_or(std::ptr::null_mut(), PropSet::handle);
        // SAFETY: the plug-in's overlay entry, with its instance.
        unsafe {
            crate::guard::entry(
                &self.binary,
                self.entry,
                action,
                self.props.handle() as *const c_void,
                i,
                std::ptr::null_mut(),
            )
        }
    }

    /// The arguments every overlay action shares, for `view`.
    fn args(&self, view: &View) -> PropSet {
        let props = &self.props;
        props.set_dbl(kOfxInteractPropPixelScale, &view.pixel_scale);
        props.set_dbl(kOfxInteractPropBackgroundColour, &view.background);
        props.set_int(kOfxInteractPropViewportSize, &view.viewport);
        let args = PropSet::new(SetKind::Arguments);
        if let Some(effect) = props.ptr(kOfxPropEffectInstance, 0) {
            args.set_ptr(kOfxPropEffectInstance, effect);
        }
        args.set_dbl(kOfxPropTime, &[view.time]);
        args.set_dbl(
            kOfxImageEffectPropRenderScale,
            &[view.render_scale, view.render_scale],
        );
        args.set_dbl(kOfxInteractPropPixelScale, &view.pixel_scale);
        args.set_dbl(kOfxInteractPropBackgroundColour, &view.background);
        args.set_int(
            bv_ofx_sys::extensions::kBananaVInteractPropModifiers,
            &[MODIFIERS.load(Ordering::Relaxed) as i32],
        );
        args
    }

    /// What the overlay draws for `view`.
    pub(crate) fn draw(&self, view: &View) -> Vec<Mark> {
        if self.opengl {
            return self.drawn_with_opengl(view).into_iter().collect();
        }
        let context = Box::new(DrawContext {
            magic: DRAW_MAGIC,
            state: Mutex::new((Pen::default(), Vec::new())),
        });
        let args = self.args(view);
        args.set_ptr(
            kOfxInteractPropDrawContext,
            &*context as *const DrawContext as *const c_void,
        );
        let _ = self.call(kOfxInteractActionDraw, Some(&args));
        std::mem::take(&mut lock(&context.state).1)
    }

    /// What an overlay of the first kind draws for `view`, with OpenGL
    /// itself: offscreen, read back as a picture of the view (`gl.rs`);
    /// nothing where OpenGL cannot be had or it drew nothing.
    #[cfg(windows)]
    fn drawn_with_opengl(&self, view: &View) -> Option<Mark> {
        let projection = crate::gl::projection(view.to_frame?);
        let args = self.args(view);
        let d = crate::gl::drawn(view.viewport, &projection, || {
            let _ = self.call(kOfxInteractActionDraw, Some(&args));
        })?;
        d.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] != 0)
            .then_some(Mark::Picture {
                width: d.width,
                height: d.height,
                rgba: d.rgba,
            })
    }

    #[cfg(not(windows))]
    fn drawn_with_opengl(&self, _view: &View) -> Option<Mark> {
        None
    }

    /// The pen at `at` (canonical coordinates, `screen` in the view's
    /// pixels); whether the overlay took it.
    pub(crate) fn pen(
        &self,
        action: PenAction,
        view: &View,
        at: [f64; 2],
        screen: [i32; 2],
        pressure: f64,
    ) -> bool {
        let args = self.args(view);
        args.set_dbl(kOfxInteractPropPenPosition, &at);
        args.set_int(kOfxInteractPropPenViewportPosition, &screen);
        args.set_dbl(kOfxInteractPropPenPressure, &[pressure]);
        self.call(action.ofx_name(), Some(&args)) == kOfxStatOK
    }

    /// The overlay gained (`true`) or lost the keyboard's attention.
    pub(crate) fn focus(&self, gained: bool, view: &View) {
        let args = self.args(view);
        let action = if gained {
            kOfxInteractActionGainFocus
        } else {
            kOfxInteractActionLoseFocus
        };
        let _ = self.call(action, Some(&args));
    }
}

impl Drop for Interact {
    fn drop(&mut self) {
        let _ = self.call(kOfxActionDestroyInstance, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Box<DrawContext> {
        Box::new(DrawContext {
            magic: DRAW_MAGIC,
            state: Mutex::new((Pen::default(), Vec::new())),
        })
    }

    #[test]
    fn the_draw_suite_records_what_is_drawn() {
        let c = context();
        let h = &*c as *const DrawContext as OfxDrawContextHandle;
        let pts = [OfxPointD { x: 0.0, y: 0.0 }, OfxPointD { x: 10.0, y: 5.0 }];
        // SAFETY: a live context and the points above.
        unsafe {
            let red = OfxRGBAColourF {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            };
            assert_eq!(set_colour(h, &red), kOfxStatOK);
            assert_eq!(set_line_width(h, 3.0), kOfxStatOK);
            assert_eq!(draw(h, kOfxDrawPrimitiveLines, pts.as_ptr(), 2), kOfxStatOK);
            assert_eq!(
                draw(h, kOfxDrawPrimitiveRectangle, pts.as_ptr(), 2),
                kOfxStatOK
            );
            // A rectangle takes two corners; a polygon three points.
            assert_eq!(
                draw(h, kOfxDrawPrimitivePolygon, pts.as_ptr(), 2),
                kOfxStatErrValue
            );
            assert_eq!(draw_text(h, c"hi".as_ptr(), &pts[1], 0x0003), kOfxStatOK);
            let mut got = OfxRGBAColourF {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            };
            assert_eq!(
                get_colour(h, kOfxStandardColourOverlayText, &mut got),
                kOfxStatOK
            );
            assert_eq!(got.r, 1.0);
            // Outside a draw action (no context): it fails.
            assert_eq!(set_line_width(std::ptr::null_mut(), 1.0), kOfxStatFailed);
        }
        let marks = std::mem::take(&mut lock(&c.state).1);
        let pen = Pen {
            colour: [1.0, 0.0, 0.0, 1.0],
            width: 3.0,
            stipple: 0,
        };
        assert_eq!(
            marks,
            vec![
                Mark::Lines {
                    points: vec![[0.0, 0.0], [10.0, 5.0]],
                    pen
                },
                Mark::Rect {
                    a: [0.0, 0.0],
                    b: [10.0, 5.0],
                    colour: pen.colour
                },
                Mark::Text {
                    text: "hi".into(),
                    at: [10.0, 5.0],
                    align: 0x0003,
                    colour: pen.colour
                },
            ]
        );
    }
}
