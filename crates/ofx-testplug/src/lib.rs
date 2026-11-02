//! OFX plug-ins written for the host's tests, never shipped. They use the
//! host as C plug-ins do: raw suite tables, variadic parameter calls,
//! images fetched and released, threads from the multi-thread suite.
//!
//! - `org.bananav.test.Invert` (filter, general): inverts its source, by
//!   an amount, on chosen channels, tinted; a Reset button; a parameter
//!   that enables another; a persistent message when there is almost
//!   nothing to do; an identity when there is nothing; an overlay (drawn
//!   with the draw suite) whose cross the pen drags to move its center.
//! - `org.bananav.test.Gradient` (generator): a gradient from its bottom
//!   color to its top one, in bytes.
//! - `org.bananav.test.Wipe` (transition): its second source wipes over the
//!   first, from the left.
//! - `org.bananav.test.Echo` (filter and general, temporal clip access):
//!   its source some frames earlier. Like Natron's tracker, it describes
//!   the filter context but refuses to be made in it: hosts fall back to
//!   the general one.
//!
//! `BV_OFX_TESTPLUG_CRASH=describe` makes Invert crash while describing
//! itself, for the tests of scanning plug-ins in their own process.
//!
//! On Windows, a binary of its own (`OfxGetNumberOfPluginsGl`,
//! `OfxGetPluginGl`): `org.bananav.test.GlCross` (filter), whose overlay is
//! of OpenFX's first kind, drawing a white cross with OpenGL itself.
//!
//! A binary of its own (`OfxGetNumberOfPluginsCrash`, `OfxGetPluginCrash`):
//! `org.bananav.test.Crash` (filter), which crashes as it renders — on the
//! render's own thread, or (its "where" choice 1) on a thread of the
//! multi-thread suite —, for the tests of the host's guard.
//!
//! A binary of its own (`OfxGetNumberOfPluginsCuda`, `OfxGetPluginCuda`):
//! `org.bananav.test.CudaInvert` and `org.bananav.test.CudaOnlyInvert`
//! (filters) render with CUDA (OpenFX 1.5's CUDA render, "true" and
//! "needed"): they invert their source's colors where the host puts its
//! pictures, the graphics card's memory or the processor's.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_double, c_int, c_uint, c_void};
use std::ptr::null_mut;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

static HOST: AtomicPtr<OfxHost> = AtomicPtr::new(null_mut());

struct Suites {
    props: &'static OfxPropertySuiteV1,
    effect: &'static OfxImageEffectSuiteV1,
    params: &'static OfxParameterSuiteV1,
    parametric: &'static OfxParametricParameterSuiteV1,
    threads: &'static OfxMultiThreadSuiteV1,
    messages: &'static OfxMessageSuiteV2,
    draw: &'static OfxDrawSuiteV1,
}

/// The host's suites (after `setHost`).
fn s() -> &'static Suites {
    static SUITES: OnceLock<Suites> = OnceLock::new();
    SUITES.get_or_init(|| {
        let host = HOST.load(Ordering::Acquire);
        assert!(!host.is_null(), "setHost first");
        // SAFETY: the host the host gave, and suites it hands out.
        unsafe {
            let fetch = |name: &CStr, v: c_int| {
                let p = ((*host).fetch_suite)((*host).host, name.as_ptr(), v);
                assert!(!p.is_null(), "{name:?} v{v}");
                p
            };
            Suites {
                props: &*(fetch(kOfxPropertySuite, 1) as *const OfxPropertySuiteV1),
                effect: &*(fetch(kOfxImageEffectSuite, 1) as *const OfxImageEffectSuiteV1),
                params: &*(fetch(kOfxParameterSuite, 1) as *const OfxParameterSuiteV1),
                parametric: &*(fetch(kOfxParametricParameterSuite, 1)
                    as *const OfxParametricParameterSuiteV1),
                threads: &*(fetch(kOfxMultiThreadSuite, 1) as *const OfxMultiThreadSuiteV1),
                messages: &*(fetch(kOfxMessageSuite, 2) as *const OfxMessageSuiteV2),
                draw: &*(fetch(kOfxDrawSuite, 1) as *const OfxDrawSuiteV1),
            }
        }
    })
}

unsafe extern "C" fn set_host(host: *mut OfxHost) {
    HOST.store(host, Ordering::Release);
}

#[repr(transparent)]
struct Shared(OfxPlugin);

// SAFETY: the structures are constant; the host only reads them.
unsafe impl Sync for Shared {}

const fn plugin(id: &'static CStr, entry: OfxPluginEntryPoint) -> Shared {
    Shared(OfxPlugin {
        plugin_api: kOfxImageEffectPluginApi.as_ptr(),
        api_version: 1,
        plugin_identifier: id.as_ptr(),
        plugin_version_major: 1,
        plugin_version_minor: 2,
        set_host: Some(set_host),
        main_entry: Some(entry),
    })
}

static PLUGINS: [Shared; 4] = [
    plugin(c"org.bananav.test.Invert", invert_main),
    plugin(c"org.bananav.test.Gradient", gradient_main),
    plugin(c"org.bananav.test.Wipe", wipe_main),
    plugin(c"org.bananav.test.Echo", echo_main),
];

/// # Safety
/// Called by hosts, as the specification says.
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn OfxGetNumberOfPlugins() -> c_int {
    PLUGINS.len() as c_int
}

/// # Safety
/// Called by hosts, as the specification says.
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn OfxGetPlugin(nth: c_int) -> *mut OfxPlugin {
    usize::try_from(nth)
        .ok()
        .and_then(|i| PLUGINS.get(i))
        .map_or(null_mut(), |p| &p.0 as *const OfxPlugin as *mut OfxPlugin)
}

static CRASH: [Shared; 1] = [plugin(c"org.bananav.test.Crash", crash_main)];

/// # Safety
/// Called by hosts, as the specification says.
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn OfxGetNumberOfPluginsCrash() -> c_int {
    CRASH.len() as c_int
}

/// # Safety
/// Called by hosts, as the specification says.
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn OfxGetPluginCrash(nth: c_int) -> *mut OfxPlugin {
    usize::try_from(nth)
        .ok()
        .and_then(|i| CRASH.get(i))
        .map_or(null_mut(), |p| &p.0 as *const OfxPlugin as *mut OfxPlugin)
}

static CUDA: [Shared; 2] = [
    plugin(c"org.bananav.test.CudaInvert", cuda_invert_main),
    plugin(c"org.bananav.test.CudaOnlyInvert", cuda_only_main),
];

/// # Safety
/// Called by hosts, as the specification says.
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn OfxGetNumberOfPluginsCuda() -> c_int {
    CUDA.len() as c_int
}

/// # Safety
/// Called by hosts, as the specification says.
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn OfxGetPluginCuda(nth: c_int) -> *mut OfxPlugin {
    usize::try_from(nth)
        .ok()
        .and_then(|i| CUDA.get(i))
        .map_or(null_mut(), |p| &p.0 as *const OfxPlugin as *mut OfxPlugin)
}

/// How many frames the CUDA plug-ins rendered with their pictures in the
/// graphics card's memory.
pub static CUDA_RENDERS: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn cuda_invert_main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    // SAFETY: the host's call, passed on.
    unsafe { cuda_main(action, handle, in_args, false) }
}

unsafe extern "C" fn cuda_only_main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    // SAFETY: the host's call, passed on.
    unsafe { cuda_main(action, handle, in_args, true) }
}

/// Inverts its source's colors (premultiplied: alpha less each), where
/// the host puts the pictures: in the card's memory when the render says
/// CUDA is enabled — copied out, inverted and copied back, CUDA's driver
/// being all a test has —, else in the processor's, which a plug-in that
/// `needs` CUDA refuses.
unsafe fn cuda_main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    needs: bool,
) -> OfxStatus {
    let effect = handle as OfxImageEffectHandle;
    // SAFETY: the host's handles, for the actions they come with.
    unsafe {
        if action_is(action, kOfxActionDescribe) {
            let p = effect_props(effect);
            let (label, cuda) = match needs {
                true => (c"CUDA Only Invert (test)", c"needed"),
                false => (c"CUDA Invert (test)", c"true"),
            };
            set_str(p, kOfxPropLabel, 0, label);
            set_strs(
                p,
                kOfxImageEffectPropSupportedContexts,
                &[kOfxImageEffectContextFilter],
            );
            set_strs(
                p,
                kOfxImageEffectPropSupportedPixelDepths,
                &[kOfxBitDepthFloat],
            );
            set_str(p, kOfxImageEffectPropCudaRenderSupported, 0, cuda);
            set_str(
                p,
                kOfxImageEffectPluginRenderThreadSafety,
                0,
                kOfxImageEffectRenderFullySafe,
            );
            kOfxStatOK
        } else if action_is(action, kOfxImageEffectActionDescribeInContext) {
            let rgba = [kOfxImageComponentRGBA];
            define_clip(effect, kOfxImageEffectSimpleSourceClipName, &rgba);
            define_clip(effect, kOfxImageEffectOutputClipName, &rgba);
            kOfxStatOK
        } else if action_is(action, kOfxImageEffectActionRender) {
            let on_card = get_int(in_args, kOfxImageEffectPropCudaEnabled, 0) == 1;
            if needs && !on_card {
                return kOfxStatFailed;
            }
            let time = get_dbl(in_args, kOfxPropTime, 0);
            let Some(src) = image(effect, kOfxImageEffectSimpleSourceClipName, time) else {
                return kOfxStatFailed;
            };
            let Some(out) = image(effect, kOfxImageEffectOutputClipName, time) else {
                src.release();
                return kOfxStatFailed;
            };
            let [x1, y1, x2, y2] = out.bounds;
            let status = if on_card {
                let rows = (y2 - y1).max(0) as usize;
                let row_len = (x2 - x1).max(0) as usize * 16;
                if invert_on_card(&src, &out, rows, row_len) {
                    CUDA_RENDERS.fetch_add(1, Ordering::Relaxed);
                    kOfxStatOK
                } else {
                    kOfxStatFailed
                }
            } else {
                for y in y1..y2 {
                    for x in x1..x2 {
                        let [r, g, b, a] = src.get(x, y);
                        out.put(x, y, [a - r, a - g, a - b, a]);
                    }
                }
                kOfxStatOK
            };
            src.release();
            out.release();
            status
        } else if action_is(action, kOfxActionCreateInstance)
            || action_is(action, kOfxActionDestroyInstance)
        {
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    }
}

/// `src`'s rows in the card's memory inverted into `out`'s.
///
/// # Safety
/// Both images' rows are `row_len` bytes of CUDA's first device's memory.
unsafe fn invert_on_card(src: &Image, out: &Image, rows: usize, row_len: usize) -> bool {
    let Some(device) = bv_cuda::devices().first() else {
        return false;
    };
    let row_at = |img: &Image, y: usize| {
        (img.data as u64).wrapping_add_signed(y as i64 * img.row_bytes as i64)
    };
    let mut row = vec![0u8; row_len];
    for y in 0..rows {
        // SAFETY: as the caller promises.
        if unsafe { device.download(&mut row, row_at(src, y)) }.is_err() {
            return false;
        }
        for px in row.as_chunks_mut::<16>().0 {
            let v: [f32; 4] = std::array::from_fn(|c| {
                f32::from_ne_bytes([px[c * 4], px[c * 4 + 1], px[c * 4 + 2], px[c * 4 + 3]])
            });
            for (c, inverted) in [v[3] - v[0], v[3] - v[1], v[3] - v[2]]
                .into_iter()
                .enumerate()
            {
                px[c * 4..c * 4 + 4].copy_from_slice(&inverted.to_ne_bytes());
            }
        }
        // SAFETY: as the caller promises.
        if unsafe { device.upload(row_at(out, y), &row) }.is_err() {
            return false;
        }
    }
    true
}

#[cfg(windows)]
static GL: [Shared; 1] = [plugin(c"org.bananav.test.GlCross", gl_main)];

/// # Safety
/// Called by hosts, as the specification says.
#[cfg(windows)]
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn OfxGetNumberOfPluginsGl() -> c_int {
    GL.len() as c_int
}

/// # Safety
/// Called by hosts, as the specification says.
#[cfg(windows)]
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn OfxGetPluginGl(nth: c_int) -> *mut OfxPlugin {
    usize::try_from(nth)
        .ok()
        .and_then(|i| GL.get(i))
        .map_or(null_mut(), |p| &p.0 as *const OfxPlugin as *mut OfxPlugin)
}

#[cfg(windows)]
#[link(name = "opengl32")]
unsafe extern "system" {
    fn glBegin(mode: u32);
    fn glEnd();
    fn glVertex2d(x: f64, y: f64);
    fn glColor4f(r: f32, g: f32, b: f32, a: f32);
    fn glLineWidth(width: f32);
}

#[cfg(windows)]
unsafe extern "C" fn gl_main(
    action: *const c_char,
    handle: *const c_void,
    _in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    let effect = handle as OfxImageEffectHandle;
    // SAFETY: the host's handles, for the actions they come with.
    unsafe {
        if action_is(action, kOfxActionDescribe) {
            let p = effect_props(effect);
            set_str(p, kOfxPropLabel, 0, c"GL Cross (test)");
            set_strs(
                p,
                kOfxImageEffectPropSupportedContexts,
                &[kOfxImageEffectContextFilter],
            );
            set_strs(
                p,
                kOfxImageEffectPropSupportedPixelDepths,
                &[kOfxBitDepthFloat],
            );
            (s().props.prop_set_pointer)(
                p,
                kOfxImageEffectPluginPropOverlayInteractV1.as_ptr(),
                0,
                gl_overlay as *mut c_void,
            );
            kOfxStatOK
        } else if action_is(action, kOfxImageEffectActionDescribeInContext) {
            let rgba = [kOfxImageComponentRGBA];
            define_clip(effect, kOfxImageEffectSimpleSourceClipName, &rgba);
            define_clip(effect, kOfxImageEffectOutputClipName, &rgba);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    }
}

/// The overlay: a white cross three pixels thick around canonical
/// (100, 50), drawn with OpenGL in the context the host made current.
#[cfg(windows)]
unsafe extern "C" fn gl_overlay(
    action: *const c_char,
    _handle: *const c_void,
    _in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if action_is(action, kOfxInteractActionDraw) {
        // SAFETY: OpenGL's calls, with the host's context current.
        unsafe {
            glColor4f(1.0, 1.0, 1.0, 1.0);
            glLineWidth(3.0);
            glBegin(0x0001); // GL_LINES
            glVertex2d(90.0, 50.0);
            glVertex2d(110.0, 50.0);
            glVertex2d(100.0, 40.0);
            glVertex2d(100.0, 60.0);
            glEnd();
        }
        kOfxStatOK
    } else if action_is(action, kOfxActionDescribe)
        || action_is(action, kOfxActionCreateInstance)
        || action_is(action, kOfxActionDestroyInstance)
    {
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

/// A crash: a write where nothing is.
fn crash() {
    // SAFETY: none — an access violation is the point.
    unsafe { std::ptr::write_volatile(16 as *mut u32, 1) };
}

unsafe extern "C" fn crash_thread(_index: c_uint, _max: c_uint, _arg: *mut c_void) {
    crash();
}

unsafe extern "C" fn crash_main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    let effect = handle as OfxImageEffectHandle;
    // SAFETY: the host's handles, for the actions they come with.
    unsafe {
        if action_is(action, kOfxActionDescribe) {
            let p = effect_props(effect);
            set_str(p, kOfxPropLabel, 0, c"Crash (test)");
            set_strs(
                p,
                kOfxImageEffectPropSupportedContexts,
                &[kOfxImageEffectContextFilter],
            );
            set_strs(
                p,
                kOfxImageEffectPropSupportedPixelDepths,
                &[kOfxBitDepthFloat],
            );
            kOfxStatOK
        } else if action_is(action, kOfxImageEffectActionDescribeInContext) {
            let rgba = [kOfxImageComponentRGBA];
            define_clip(effect, kOfxImageEffectSimpleSourceClipName, &rgba);
            define_clip(effect, kOfxImageEffectOutputClipName, &rgba);
            let p = define(effect, kOfxParamTypeInteger, c"where", c"Where");
            set_int(p, kOfxParamPropDefault, 0, 0);
            kOfxStatOK
        } else if action_is(action, kOfxImageEffectActionRender) {
            let time = get_dbl(in_args, kOfxPropTime, 0);
            let mut on_thread: c_int = 0;
            (s().params.param_get_value_at_time)(
                param(effect, c"where").0,
                time,
                &mut on_thread as *mut c_int,
            );
            if on_thread == 1 {
                (s().threads.multi_thread)(crash_thread, 2, null_mut())
            } else {
                crash();
                kOfxStatOK
            }
        } else {
            kOfxStatReplyDefault
        }
    }
}

// Property helpers.

unsafe fn set_str(h: OfxPropertySetHandle, name: &CStr, i: c_int, v: &CStr) {
    // SAFETY: the host's handle.
    unsafe { (s().props.prop_set_string)(h, name.as_ptr(), i, v.as_ptr()) };
}

unsafe fn set_strs(h: OfxPropertySetHandle, name: &CStr, values: &[&CStr]) {
    for (i, v) in values.iter().enumerate() {
        // SAFETY: as above.
        unsafe { set_str(h, name, i as c_int, v) };
    }
}

unsafe fn set_int(h: OfxPropertySetHandle, name: &CStr, i: c_int, v: c_int) {
    // SAFETY: the host's handle.
    unsafe { (s().props.prop_set_int)(h, name.as_ptr(), i, v) };
}

unsafe fn set_dbl(h: OfxPropertySetHandle, name: &CStr, values: &[f64]) {
    for (i, v) in values.iter().enumerate() {
        // SAFETY: the host's handle.
        unsafe { (s().props.prop_set_double)(h, name.as_ptr(), i as c_int, *v) };
    }
}

unsafe fn get_int(h: OfxPropertySetHandle, name: &CStr, i: c_int) -> c_int {
    let mut v = 0;
    // SAFETY: the host's handle.
    unsafe { (s().props.prop_get_int)(h, name.as_ptr(), i, &mut v) };
    v
}

unsafe fn get_dbl(h: OfxPropertySetHandle, name: &CStr, i: c_int) -> f64 {
    let mut v = 0.0;
    // SAFETY: the host's handle.
    unsafe { (s().props.prop_get_double)(h, name.as_ptr(), i, &mut v) };
    v
}

unsafe fn get_ptr(h: OfxPropertySetHandle, name: &CStr) -> *mut c_void {
    let mut v = null_mut();
    // SAFETY: the host's handle.
    unsafe { (s().props.prop_get_pointer)(h, name.as_ptr(), 0, &mut v) };
    v
}

unsafe fn get_str<'a>(h: OfxPropertySetHandle, name: &CStr) -> &'a CStr {
    let mut v: *mut c_char = null_mut();
    // SAFETY: the host's handle; its strings stay while unchanged.
    unsafe {
        (s().props.prop_get_string)(h, name.as_ptr(), 0, &mut v);
        if v.is_null() { c"" } else { CStr::from_ptr(v) }
    }
}

unsafe fn effect_props(effect: OfxImageEffectHandle) -> OfxPropertySetHandle {
    let mut p = null_mut();
    // SAFETY: the host's handle.
    unsafe { (s().effect.get_property_set)(effect, &mut p) };
    p
}

unsafe fn param_set(effect: OfxImageEffectHandle) -> OfxParamSetHandle {
    let mut p = null_mut();
    // SAFETY: the host's handle.
    unsafe { (s().effect.get_param_set)(effect, &mut p) };
    p
}

/// Defines a parameter; returns its properties.
unsafe fn define(
    effect: OfxImageEffectHandle,
    ty: &CStr,
    name: &CStr,
    label: &CStr,
) -> OfxPropertySetHandle {
    let mut props = null_mut();
    // SAFETY: the host's handles.
    unsafe {
        (s().params.param_define)(param_set(effect), ty.as_ptr(), name.as_ptr(), &mut props);
        set_str(props, kOfxPropLabel, 0, label);
    }
    props
}

unsafe fn param(
    effect: OfxImageEffectHandle,
    name: &CStr,
) -> (OfxParamHandle, OfxPropertySetHandle) {
    let (mut h, mut p) = (null_mut(), null_mut());
    // SAFETY: the host's handles.
    unsafe { (s().params.param_get_handle)(param_set(effect), name.as_ptr(), &mut h, &mut p) };
    (h, p)
}

unsafe fn define_clip(
    effect: OfxImageEffectHandle,
    name: &CStr,
    components: &[&CStr],
) -> OfxPropertySetHandle {
    let mut props = null_mut();
    // SAFETY: the host's handle.
    unsafe {
        (s().effect.clip_define)(effect, name.as_ptr(), &mut props);
        set_strs(props, kOfxImageEffectPropSupportedComponents, components);
    }
    props
}

/// An image the host gave: where its pixels are.
struct Image {
    handle: OfxPropertySetHandle,
    data: *mut u8,
    row_bytes: isize,
    bounds: [c_int; 4],
    float: bool,
    channels: usize,
}

unsafe fn image(effect: OfxImageEffectHandle, clip: &CStr, time: f64) -> Option<Image> {
    let (mut c, mut h) = (null_mut(), null_mut());
    // SAFETY: the host's handles.
    unsafe {
        (s().effect.clip_get_handle)(effect, clip.as_ptr(), &mut c, null_mut());
        if (s().effect.clip_get_image)(c, time, std::ptr::null(), &mut h) != kOfxStatOK {
            return None;
        }
        let depth = get_str(h, kOfxImageEffectPropPixelDepth);
        let comps = get_str(h, kOfxImageEffectPropComponents);
        Some(Image {
            handle: h,
            data: get_ptr(h, kOfxImagePropData) as *mut u8,
            row_bytes: get_int(h, kOfxImagePropRowBytes, 0) as isize,
            bounds: [0, 1, 2, 3].map(|i| get_int(h, kOfxImagePropBounds, i)),
            float: depth == kOfxBitDepthFloat,
            channels: if comps == kOfxImageComponentRGB { 3 } else { 4 },
        })
    }
}

impl Image {
    /// The pixel at (x, y) in the image's bounds, as floats (alpha 1 for RGB).
    unsafe fn get(&self, x: c_int, y: c_int) -> [f32; 4] {
        // SAFETY: (x, y) is inside the bounds the host gave.
        unsafe {
            let row = self
                .data
                .offset((y - self.bounds[1]) as isize * self.row_bytes);
            let i = (x - self.bounds[0]) as usize * self.channels;
            let mut px = [0.0, 0.0, 0.0, 1.0];
            for (c, v) in px.iter_mut().enumerate().take(self.channels) {
                *v = if self.float {
                    *(row as *const f32).add(i + c)
                } else {
                    *row.add(i + c) as f32 / 255.0
                };
            }
            px
        }
    }

    unsafe fn put(&self, x: c_int, y: c_int, px: [f32; 4]) {
        // SAFETY: as in `get`.
        unsafe {
            let row = self
                .data
                .offset((y - self.bounds[1]) as isize * self.row_bytes);
            let i = (x - self.bounds[0]) as usize * self.channels;
            for (c, v) in px.iter().enumerate().take(self.channels) {
                if self.float {
                    *(row as *mut f32).add(i + c) = *v;
                } else {
                    *row.add(i + c) = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
        }
    }

    unsafe fn release(self) {
        // SAFETY: an image the host gave, released once.
        unsafe { (s().effect.clip_release_image)(self.handle) };
    }
}

fn action_is(action: *const c_char, name: &CStr) -> bool {
    // SAFETY: hosts pass actions as C strings.
    !action.is_null() && unsafe { CStr::from_ptr(action) } == name
}

// Invert.

unsafe extern "C" fn invert_main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    let effect = handle as OfxImageEffectHandle;
    // SAFETY: the host's handles, for the actions they come with.
    unsafe {
        if action_is(action, kOfxActionLoad) {
            kOfxStatOK
        } else if action_is(action, kOfxActionDescribe) {
            if std::env::var("BV_OFX_TESTPLUG_CRASH").as_deref() == Ok("describe") {
                std::process::abort();
            }
            let p = effect_props(effect);
            set_str(p, kOfxPropLabel, 0, c"Invert (test)");
            set_str(p, kOfxImageEffectPluginPropGrouping, 0, c"BananaV Tests");
            set_str(p, kOfxPropPluginDescription, 0, c"Inverts the colors.");
            set_strs(
                p,
                kOfxImageEffectPropSupportedContexts,
                &[kOfxImageEffectContextFilter, kOfxImageEffectContextGeneral],
            );
            set_strs(
                p,
                kOfxImageEffectPropSupportedPixelDepths,
                &[kOfxBitDepthFloat, kOfxBitDepthByte],
            );
            set_int(p, kOfxImageEffectPropSupportsTiles, 0, 0);
            (s().props.prop_set_pointer)(
                p,
                kOfxImageEffectPluginPropOverlayInteractV2.as_ptr(),
                0,
                invert_overlay as *mut c_void,
            );
            set_str(
                p,
                kOfxImageEffectPluginRenderThreadSafety,
                0,
                kOfxImageEffectRenderInstanceSafe,
            );
            kOfxStatOK
        } else if action_is(action, kOfxImageEffectActionDescribeInContext) {
            describe_invert(effect);
            kOfxStatOK
        } else if action_is(action, kOfxImageEffectActionIsIdentity) {
            let time = get_dbl(in_args, kOfxPropTime, 0);
            let mut amount = 0.0f64;
            (s().params.param_get_value_at_time)(
                param(effect, c"amount").0,
                time,
                &mut amount as *mut f64,
            );
            if amount == 0.0 {
                set_str(
                    out_args,
                    kOfxPropName,
                    0,
                    kOfxImageEffectSimpleSourceClipName,
                );
                kOfxStatOK
            } else {
                kOfxStatReplyDefault
            }
        } else if action_is(action, kOfxImageEffectActionRender) {
            render_invert(effect, in_args)
        } else if action_is(action, kOfxActionInstanceChanged) {
            invert_changed(effect, in_args);
            kOfxStatOK
        } else if action_is(action, kOfxActionCreateInstance)
            || action_is(action, kOfxActionDestroyInstance)
        {
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    }
}

unsafe fn describe_invert(effect: OfxImageEffectHandle) {
    // SAFETY: the host's handles, while describing.
    unsafe {
        let rgba = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
        define_clip(effect, kOfxImageEffectSimpleSourceClipName, &rgba);
        define_clip(effect, kOfxImageEffectOutputClipName, &rgba);

        let p = define(effect, kOfxParamTypeDouble, c"amount", c"Amount");
        set_dbl(p, kOfxParamPropDefault, &[1.0]);
        set_dbl(p, kOfxParamPropMin, &[0.0]);
        set_dbl(p, kOfxParamPropMax, &[1.0]);
        set_dbl(p, kOfxParamPropDisplayMin, &[0.0]);
        set_dbl(p, kOfxParamPropDisplayMax, &[1.0]);
        set_str(p, kOfxParamPropHint, 0, c"How far toward the inverse.");

        let p = define(effect, kOfxParamTypeChoice, c"channels", c"Channels");
        set_strs(
            p,
            kOfxParamPropChoiceOption,
            &[c"All", c"Red", c"Green", c"Blue"],
        );
        set_int(p, kOfxParamPropDefault, 0, 0);

        let p = define(effect, kOfxParamTypeRGBA, c"tint", c"Tint");
        set_dbl(p, kOfxParamPropDefault, &[1.0, 1.0, 1.0, 1.0]);

        let p = define(effect, kOfxParamTypeDouble2D, c"center", c"Center");
        set_dbl(p, kOfxParamPropDefault, &[0.5, 0.5]);
        set_str(p, kOfxParamPropDoubleType, 0, kOfxParamDoubleTypeXYAbsolute);
        set_str(
            p,
            kOfxParamPropDefaultCoordinateSystem,
            0,
            kOfxParamCoordinatesNormalised,
        );

        let p = define(effect, kOfxParamTypeInteger, c"steps", c"Steps");
        set_int(p, kOfxParamPropDefault, 0, 3);
        set_int(p, kOfxParamPropMin, 0, 1);
        set_int(p, kOfxParamPropMax, 0, 10);

        let p = define(effect, kOfxParamTypeString, c"note", c"Note");
        set_str(p, kOfxParamPropDefault, 0, c"hello");

        define(effect, kOfxParamTypeGroup, c"advanced", c"Advanced");
        let p = define(
            effect,
            kOfxParamTypeBoolean,
            c"invertAlpha",
            c"Invert Alpha",
        );
        set_str(p, kOfxParamPropParent, 0, c"advanced");

        define(effect, kOfxParamTypePushButton, c"reset", c"Reset");

        define(effect, kOfxParamTypeParametric, c"curve", c"Curve");
        let (h, _) = param(effect, c"curve");
        (s().parametric.parametric_param_add_control_point)(h, 0, 0.0, 0.0, 0.0, false);
        (s().parametric.parametric_param_add_control_point)(h, 0, 0.0, 1.0, 1.0, false);
    }
}

/// What the render threads share.
struct Job {
    src: Image,
    out: Image,
    window: [c_int; 4],
    amount: f32,
    channels: c_int,
    tint: [f32; 4],
    invert_alpha: bool,
}

unsafe extern "C" fn invert_rows(index: c_uint, count: c_uint, arg: *mut c_void) {
    // SAFETY: the job `render_invert` passes, alive until the threads end.
    let job = unsafe { &*(arg as *const Job) };
    let [x1, y1, x2, y2] = job.window;
    let mut y = y1 + index as c_int;
    while y < y2 {
        for x in x1..x2 {
            // SAFETY: inside both images' bounds (the whole frame).
            unsafe {
                let px = job.src.get(x, y);
                let a = px[3];
                let mut out = px;
                for (c, v) in out.iter_mut().enumerate().take(3) {
                    let selected = job.channels == 0 || job.channels == c as c_int + 1;
                    if selected {
                        *v = (px[c] + ((a - px[c]) - px[c]) * job.amount) * job.tint[c];
                    }
                }
                if job.invert_alpha {
                    out[3] = 1.0 - a;
                }
                job.out.put(x, y, out);
            }
        }
        y += count as c_int;
    }
}

unsafe fn render_invert(effect: OfxImageEffectHandle, args: OfxPropertySetHandle) -> OfxStatus {
    // SAFETY: the host's handles, while rendering.
    unsafe {
        let time = get_dbl(args, kOfxPropTime, 0);
        let window = [0, 1, 2, 3].map(|i| get_int(args, kOfxImageEffectPropRenderWindow, i));
        let p = &s().params;
        let mut amount = 0.0f64;
        (p.param_get_value_at_time)(param(effect, c"amount").0, time, &mut amount as *mut f64);
        let mut channels: c_int = 0;
        (p.param_get_value_at_time)(
            param(effect, c"channels").0,
            time,
            &mut channels as *mut c_int,
        );
        let mut tint = [0.0f64; 4];
        let [r, g, b, a] = tint.each_mut().map(|v| v as *mut f64);
        (p.param_get_value_at_time)(param(effect, c"tint").0, time, r, g, b, a);
        let mut invert_alpha: c_int = 0;
        (p.param_get_value_at_time)(
            param(effect, c"invertAlpha").0,
            time,
            &mut invert_alpha as *mut c_int,
        );
        let Some(src) = image(effect, kOfxImageEffectSimpleSourceClipName, time) else {
            return kOfxStatFailed;
        };
        let Some(out) = image(effect, kOfxImageEffectOutputClipName, time) else {
            src.release();
            return kOfxStatFailed;
        };
        let job = Job {
            src,
            out,
            window,
            amount: amount as f32,
            channels,
            tint: tint.map(|v| v as f32),
            invert_alpha: invert_alpha != 0,
        };
        let status = (s().threads.multi_thread)(invert_rows, 0, &job as *const Job as *mut c_void);
        let Job { src, out, .. } = job;
        src.release();
        out.release();
        let m = &s().messages;
        if amount < 0.01 {
            (m.set_persistent_message)(
                effect as *mut c_void,
                kOfxMessageWarning.as_ptr(),
                c"small".as_ptr(),
                c"Amount %.3f inverts almost nothing".as_ptr(),
                amount,
            );
        } else {
            (m.clear_persistent_message)(effect as *mut c_void);
        }
        status
    }
}

unsafe fn invert_changed(effect: OfxImageEffectHandle, args: OfxPropertySetHandle) {
    // SAFETY: the host's handles, while instance changed.
    unsafe {
        let name = get_str(args, kOfxPropName);
        let p = &s().params;
        if name == c"reset" {
            (p.param_set_value)(param(effect, c"amount").0, 1.0f64);
            (p.param_set_value)(param(effect, c"note").0, c"reset".as_ptr());
        } else if name == c"amount" {
            let mut amount = 0.0f64;
            (p.param_get_value)(param(effect, c"amount").0, &mut amount as *mut f64);
            let (_, tint) = param(effect, c"tint");
            set_int(tint, kOfxParamPropEnabled, 0, (amount > 0.0) as c_int);
        }
    }
}

// Invert's overlay: a cross at its center, which the pen drags.

/// Whether the pen is dragging the center.
static DRAGGING: AtomicBool = AtomicBool::new(false);

/// The center, in canonical coordinates (its default, the middle, was
/// given normalised: the host made it canonical).
unsafe fn center_at(effect: OfxImageEffectHandle, time: f64) -> [f64; 2] {
    // SAFETY: the host's handle.
    unsafe {
        let (mut x, mut y) = (0.0f64, 0.0f64);
        (s().params.param_get_value_at_time)(
            param(effect, c"center").0,
            time,
            &mut x as *mut f64,
            &mut y as *mut f64,
        );
        [x, y]
    }
}

unsafe extern "C" fn invert_overlay(
    action: *const c_char,
    _handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if action_is(action, kOfxActionDescribe)
        || action_is(action, kOfxActionCreateInstance)
        || action_is(action, kOfxActionDestroyInstance)
    {
        return kOfxStatOK;
    }
    // SAFETY: the host's handles, for the actions they come with.
    unsafe {
        let effect = get_ptr(in_args, kOfxPropEffectInstance) as OfxImageEffectHandle;
        if effect.is_null() {
            return kOfxStatReplyDefault;
        }
        let time = get_dbl(in_args, kOfxPropTime, 0);
        let px = [
            get_dbl(in_args, kOfxInteractPropPixelScale, 0)
                .abs()
                .max(1e-9),
            get_dbl(in_args, kOfxInteractPropPixelScale, 1)
                .abs()
                .max(1e-9),
        ];
        let c = center_at(effect, time);
        let pen = || {
            [
                get_dbl(in_args, kOfxInteractPropPenPosition, 0),
                get_dbl(in_args, kOfxInteractPropPenPosition, 1),
            ]
        };
        if action_is(action, kOfxInteractActionDraw) {
            let ctx = get_ptr(in_args, kOfxInteractPropDrawContext) as OfxDrawContextHandle;
            let d = s().draw;
            let mut colour = OfxRGBAColourF {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            };
            if DRAGGING.load(Ordering::Relaxed) {
                (d.get_colour)(ctx, kOfxStandardColourOverlayActive, &mut colour);
            }
            (d.set_colour)(ctx, &colour);
            (d.set_line_width)(ctx, 1.5);
            let (rx, ry) = (12.0 * px[0], 12.0 * px[1]);
            let pts = [
                OfxPointD {
                    x: c[0] - rx,
                    y: c[1],
                },
                OfxPointD {
                    x: c[0] + rx,
                    y: c[1],
                },
                OfxPointD {
                    x: c[0],
                    y: c[1] - ry,
                },
                OfxPointD {
                    x: c[0],
                    y: c[1] + ry,
                },
            ];
            (d.draw)(ctx, kOfxDrawPrimitiveLines, pts.as_ptr(), 4);
            let label = OfxPointD {
                x: c[0] + rx,
                y: c[1] + ry,
            };
            (d.draw_text)(ctx, c"Center".as_ptr(), &label, kOfxDrawTextAlignmentLeft);
            kOfxStatOK
        } else if action_is(action, kOfxInteractActionPenDown) {
            let at = pen();
            let near = (at[0] - c[0]).abs() <= 12.0 * px[0] && (at[1] - c[1]).abs() <= 12.0 * px[1];
            DRAGGING.store(near, Ordering::Relaxed);
            if near {
                kOfxStatOK
            } else {
                kOfxStatReplyDefault
            }
        } else if action_is(action, kOfxInteractActionPenMotion) {
            if !DRAGGING.load(Ordering::Relaxed) {
                return kOfxStatReplyDefault;
            }
            let at = pen();
            (s().params.param_set_value)(param(effect, c"center").0, at[0], at[1]);
            kOfxStatOK
        } else if action_is(action, kOfxInteractActionPenUp) {
            if DRAGGING.swap(false, Ordering::Relaxed) {
                kOfxStatOK
            } else {
                kOfxStatReplyDefault
            }
        } else {
            kOfxStatReplyDefault
        }
    }
}

// Echo.

unsafe extern "C" fn echo_main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    let effect = handle as OfxImageEffectHandle;
    // SAFETY: the host's handles, for the actions they come with.
    unsafe {
        if action_is(action, kOfxActionDescribe) {
            let p = effect_props(effect);
            set_str(p, kOfxPropLabel, 0, c"Echo (test)");
            set_str(p, kOfxImageEffectPluginPropGrouping, 0, c"BananaV Tests");
            set_strs(
                p,
                kOfxImageEffectPropSupportedContexts,
                &[kOfxImageEffectContextFilter, kOfxImageEffectContextGeneral],
            );
            set_strs(
                p,
                kOfxImageEffectPropSupportedPixelDepths,
                &[kOfxBitDepthFloat],
            );
            set_int(p, kOfxImageEffectPropTemporalClipAccess, 0, 1);
            kOfxStatOK
        } else if action_is(action, kOfxImageEffectActionDescribeInContext) {
            let rgba = [kOfxImageComponentRGBA];
            let source = define_clip(effect, kOfxImageEffectSimpleSourceClipName, &rgba);
            set_int(source, kOfxImageEffectPropTemporalClipAccess, 0, 1);
            define_clip(effect, kOfxImageEffectOutputClipName, &rgba);
            let p = define(effect, kOfxParamTypeInteger, c"delay", c"Delay");
            set_int(p, kOfxParamPropDefault, 0, 1);
            set_int(p, kOfxParamPropMin, 0, 0);
            set_int(p, kOfxParamPropMax, 0, 30);
            kOfxStatOK
        } else if action_is(action, kOfxImageEffectActionRender) {
            let time = get_dbl(in_args, kOfxPropTime, 0);
            let [x1, y1, x2, y2] =
                [0, 1, 2, 3].map(|i| get_int(in_args, kOfxImageEffectPropRenderWindow, i));
            let mut delay: c_int = 0;
            (s().params.param_get_value_at_time)(
                param(effect, c"delay").0,
                time,
                &mut delay as *mut c_int,
            );
            let then = image(
                effect,
                kOfxImageEffectSimpleSourceClipName,
                time - delay as f64,
            );
            let out = image(effect, kOfxImageEffectOutputClipName, time);
            let (Some(then), Some(out)) = (then, out) else {
                return kOfxStatFailed;
            };
            for y in y1..y2 {
                for x in x1..x2 {
                    out.put(x, y, then.get(x, y));
                }
            }
            then.release();
            out.release();
            kOfxStatOK
        } else if action_is(action, kOfxActionCreateInstance) {
            // Made in the filter context, it fails (see the module's notes).
            let context = get_str(effect_props(effect), kOfxImageEffectPropContext);
            if context == kOfxImageEffectContextFilter {
                kOfxStatFailed
            } else {
                kOfxStatOK
            }
        } else if action_is(action, kOfxActionLoad) || action_is(action, kOfxActionDestroyInstance)
        {
            kOfxStatOK
        } else if action_is(action, kOfxActionUnload) {
            ECHO_UNLOADS.fetch_add(1, Ordering::Relaxed);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    }
}

/// How many times Echo heard the unload action (for the host's tests).
pub static ECHO_UNLOADS: AtomicUsize = AtomicUsize::new(0);

// Gradient.

unsafe extern "C" fn gradient_main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    let effect = handle as OfxImageEffectHandle;
    // SAFETY: the host's handles, for the actions they come with.
    unsafe {
        if action_is(action, kOfxActionDescribe) {
            let p = effect_props(effect);
            set_str(p, kOfxPropLabel, 0, c"Gradient (test)");
            set_str(p, kOfxImageEffectPluginPropGrouping, 0, c"BananaV Tests");
            set_strs(
                p,
                kOfxImageEffectPropSupportedContexts,
                &[kOfxImageEffectContextGenerator],
            );
            set_strs(
                p,
                kOfxImageEffectPropSupportedPixelDepths,
                &[kOfxBitDepthByte],
            );
            kOfxStatOK
        } else if action_is(action, kOfxImageEffectActionDescribeInContext) {
            define_clip(
                effect,
                kOfxImageEffectOutputClipName,
                &[kOfxImageComponentRGBA],
            );
            let p = define(effect, kOfxParamTypeRGB, c"top", c"Top");
            set_dbl(p, kOfxParamPropDefault, &[1.0, 0.0, 0.0]);
            let p = define(effect, kOfxParamTypeRGB, c"bottom", c"Bottom");
            set_dbl(p, kOfxParamPropDefault, &[0.0, 0.0, 1.0]);
            kOfxStatOK
        } else if action_is(action, kOfxImageEffectActionRender) {
            let time = get_dbl(in_args, kOfxPropTime, 0);
            let [x1, y1, x2, y2] =
                [0, 1, 2, 3].map(|i| get_int(in_args, kOfxImageEffectPropRenderWindow, i));
            let mut top = [0.0f64; 3];
            let mut bottom = [0.0f64; 3];
            let p = &s().params;
            let [a, b, c] = top.each_mut().map(|v| v as *mut f64);
            (p.param_get_value_at_time)(param(effect, c"top").0, time, a, b, c);
            let [a, b, c] = bottom.each_mut().map(|v| v as *mut f64);
            (p.param_get_value_at_time)(param(effect, c"bottom").0, time, a, b, c);
            let Some(out) = image(effect, kOfxImageEffectOutputClipName, time) else {
                return kOfxStatFailed;
            };
            for y in y1..y2 {
                let k = (y - y1) as f64 / ((y2 - y1 - 1).max(1)) as f64;
                let px = [0, 1, 2].map(|i| (bottom[i] + (top[i] - bottom[i]) * k) as f32);
                for x in x1..x2 {
                    out.put(x, y, [px[0], px[1], px[2], 1.0]);
                }
            }
            out.release();
            kOfxStatOK
        } else if action_is(action, kOfxActionLoad)
            || action_is(action, kOfxActionCreateInstance)
            || action_is(action, kOfxActionDestroyInstance)
        {
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    }
}

// Wipe.

unsafe extern "C" fn wipe_main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    let effect = handle as OfxImageEffectHandle;
    // SAFETY: the host's handles, for the actions they come with.
    unsafe {
        if action_is(action, kOfxActionDescribe) {
            let p = effect_props(effect);
            set_str(p, kOfxPropLabel, 0, c"Wipe (test)");
            set_str(p, kOfxImageEffectPluginPropGrouping, 0, c"BananaV Tests");
            set_strs(
                p,
                kOfxImageEffectPropSupportedContexts,
                &[kOfxImageEffectContextTransition],
            );
            set_strs(
                p,
                kOfxImageEffectPropSupportedPixelDepths,
                &[kOfxBitDepthFloat],
            );
            kOfxStatOK
        } else if action_is(action, kOfxImageEffectActionDescribeInContext) {
            let rgba = [kOfxImageComponentRGBA];
            define_clip(effect, kOfxImageEffectTransitionSourceFromClipName, &rgba);
            define_clip(effect, kOfxImageEffectTransitionSourceToClipName, &rgba);
            define_clip(effect, kOfxImageEffectOutputClipName, &rgba);
            let p = define(
                effect,
                kOfxParamTypeDouble,
                kOfxImageEffectTransitionParamName,
                c"Progress",
            );
            set_dbl(p, kOfxParamPropMin, &[0.0]);
            set_dbl(p, kOfxParamPropMax, &[1.0]);
            let p = define(effect, kOfxParamTypeDouble, c"softness", c"Softness");
            set_dbl(p, kOfxParamPropDefault, &[0.0]);
            set_dbl(p, kOfxParamPropMin, &[0.0]);
            set_dbl(p, kOfxParamPropMax, &[1.0]);
            set_dbl(p, kOfxParamPropDisplayMin, &[0.0]);
            set_dbl(p, kOfxParamPropDisplayMax, &[1.0]);
            set_str(p, kOfxParamPropHint, 0, c"How wide the edge blends.");
            kOfxStatOK
        } else if action_is(action, kOfxImageEffectActionRender) {
            let time = get_dbl(in_args, kOfxPropTime, 0);
            let [x1, y1, x2, y2] =
                [0, 1, 2, 3].map(|i| get_int(in_args, kOfxImageEffectPropRenderWindow, i));
            let mut t: c_double = 0.0;
            let (h, _) = param(effect, kOfxImageEffectTransitionParamName);
            (s().params.param_get_value_at_time)(h, time, &mut t as *mut f64);
            let mut soft: c_double = 0.0;
            let (h, _) = param(effect, c"softness");
            (s().params.param_get_value_at_time)(h, time, &mut soft as *mut f64);
            // The blend's width in pixels (none: a hard edge).
            let ramp = soft * (x2 - x1) as f64;
            let from = image(effect, kOfxImageEffectTransitionSourceFromClipName, time);
            let to = image(effect, kOfxImageEffectTransitionSourceToClipName, time);
            let out = image(effect, kOfxImageEffectOutputClipName, time);
            let (Some(from), Some(to), Some(out)) = (from, to, out) else {
                return kOfxStatFailed;
            };
            let edge = x1 + ((x2 - x1) as f64 * t).round() as c_int;
            for y in y1..y2 {
                for x in x1..x2 {
                    let px = if ramp < 0.5 {
                        if x < edge {
                            to.get(x, y)
                        } else {
                            from.get(x, y)
                        }
                    } else {
                        let k = ((edge - x) as f64 / ramp + 0.5).clamp(0.0, 1.0) as f32;
                        let (a, b) = (from.get(x, y), to.get(x, y));
                        [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * k)
                    };
                    out.put(x, y, px);
                }
            }
            from.release();
            to.release();
            out.release();
            kOfxStatOK
        } else if action_is(action, kOfxActionLoad)
            || action_is(action, kOfxActionCreateInstance)
            || action_is(action, kOfxActionDestroyInstance)
        {
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    }
}
