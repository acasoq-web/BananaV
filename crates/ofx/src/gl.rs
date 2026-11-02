//! OpenGL for the overlays of OpenFX's first kind
//! (`kOfxImageEffectPluginPropOverlayInteractV1`), which draw with OpenGL
//! themselves — Sapphire's, offered so to hosts it does not know. On
//! Windows: an offscreen context of the graphics card's, drawing into a
//! framebuffer object of the view's size, its projection set so the
//! plug-in draws in canonical coordinates, as the specification asks; the
//! drawing is read back as a picture laid over the preview. Where the card
//! has no such OpenGL (Windows' own software one is version 1.1) there is
//! none: the overlay draws nothing.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_void};
use std::sync::Mutex;

type Handle = *mut c_void;

/// What an overlay drew: premultiplied RGBA, rows from the top.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawing {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

// The OpenGL names used (gl.h, glext.h).
const GL_PROJECTION: u32 = 0x1701;
const GL_MODELVIEW: u32 = 0x1700;
const GL_DEPTH_TEST: u32 = 0x0B71;
const GL_LIGHTING: u32 = 0x0B50;
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_BLEND: u32 = 0x0BE2;
const GL_SRC_ALPHA: u32 = 0x0302;
const GL_ONE_MINUS_SRC_ALPHA: u32 = 0x0303;
const GL_ONE: u32 = 1;
const GL_COLOR_BUFFER_BIT: u32 = 0x4000;
const GL_DEPTH_BUFFER_BIT: u32 = 0x0100;
const GL_STENCIL_BUFFER_BIT: u32 = 0x0400;
const GL_ALL_ATTRIB_BITS: u32 = 0x000F_FFFF;
const GL_PACK_ALIGNMENT: u32 = 0x0D05;
const GL_RGBA: u32 = 0x1908;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_FRAMEBUFFER: u32 = 0x8D40;
const GL_RENDERBUFFER: u32 = 0x8D41;
const GL_RGBA8: u32 = 0x8058;
const GL_DEPTH24_STENCIL8: u32 = 0x88F0;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
const GL_DEPTH_STENCIL_ATTACHMENT: u32 = 0x821A;
const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;

#[link(name = "opengl32")]
unsafe extern "system" {
    fn wglCreateContext(dc: Handle) -> Handle;
    fn wglMakeCurrent(dc: Handle, rc: Handle) -> i32;
    fn wglGetCurrentContext() -> Handle;
    fn wglGetCurrentDC() -> Handle;
    fn wglGetProcAddress(name: *const std::ffi::c_char) -> *const c_void;
    fn glViewport(x: i32, y: i32, w: i32, h: i32);
    fn glClearColor(r: f32, g: f32, b: f32, a: f32);
    fn glClear(mask: u32);
    fn glMatrixMode(mode: u32);
    fn glPushMatrix();
    fn glPopMatrix();
    fn glLoadMatrixd(m: *const f64);
    fn glLoadIdentity();
    fn glPushAttrib(mask: u32);
    fn glPopAttrib();
    fn glEnable(cap: u32);
    fn glDisable(cap: u32);
    fn glBlendFunc(s: u32, d: u32);
    fn glPixelStorei(name: u32, value: i32);
    fn glReadPixels(x: i32, y: i32, w: i32, h: i32, format: u32, ty: u32, data: *mut c_void);
    fn glFinish();
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn ChoosePixelFormat(dc: Handle, pfd: *const PixelFormat) -> i32;
    fn SetPixelFormat(dc: Handle, format: i32, pfd: *const PixelFormat) -> i32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: Handle,
        menu: Handle,
        instance: Handle,
        param: *const c_void,
    ) -> Handle;
    fn GetDC(window: Handle) -> Handle;
}

/// Windows' PIXELFORMATDESCRIPTOR.
#[repr(C)]
#[derive(Default)]
struct PixelFormat {
    size: u16,
    version: u16,
    flags: u32,
    pixel_type: u8,
    color_bits: u8,
    red_bits: u8,
    red_shift: u8,
    green_bits: u8,
    green_shift: u8,
    blue_bits: u8,
    blue_shift: u8,
    alpha_bits: u8,
    alpha_shift: u8,
    accum_bits: u8,
    accum_red_bits: u8,
    accum_green_bits: u8,
    accum_blue_bits: u8,
    accum_alpha_bits: u8,
    depth_bits: u8,
    stencil_bits: u8,
    aux_buffers: u8,
    layer_type: u8,
    reserved: u8,
    layer_mask: u32,
    visible_mask: u32,
    damage_mask: u32,
}

/// The framebuffer objects' functions (OpenGL 3, or the extension), and
/// glBlendFuncSeparate (1.4): what the card's driver gives.
struct Functions {
    gen_framebuffers: unsafe extern "system" fn(i32, *mut u32),
    bind_framebuffer: unsafe extern "system" fn(u32, u32),
    gen_renderbuffers: unsafe extern "system" fn(i32, *mut u32),
    bind_renderbuffer: unsafe extern "system" fn(u32, u32),
    renderbuffer_storage: unsafe extern "system" fn(u32, u32, i32, i32),
    framebuffer_renderbuffer: unsafe extern "system" fn(u32, u32, u32, u32),
    check_framebuffer_status: unsafe extern "system" fn(u32) -> u32,
    blend_func_separate: Option<unsafe extern "system" fn(u32, u32, u32, u32)>,
}

impl Functions {
    /// The functions, with the context current; none when the driver lacks
    /// one (Windows' software OpenGL).
    unsafe fn load() -> Option<Functions> {
        unsafe fn get(name: &CStr) -> Option<*const c_void> {
            // SAFETY: a NUL-terminated name, with a context current.
            let p = unsafe { wglGetProcAddress(name.as_ptr()) };
            // Some drivers answer 1, 2, 3 or -1 for none.
            (!matches!(p as isize, -1..=3)).then_some(p)
        }
        /// The driver's function `name` (or its extension's, `ext`) as `F`.
        ///
        /// # Safety
        /// `F` is the function's signature, a function pointer.
        unsafe fn function<F: Copy>(name: &CStr, ext: &CStr) -> Option<F> {
            // SAFETY: as the caller promises; a function pointer is a
            // pointer's size.
            let p = unsafe { get(name).or_else(|| get(ext))? };
            Some(unsafe { std::mem::transmute_copy::<*const c_void, F>(&p) })
        }
        // SAFETY: each field's type is its function's signature, as
        // OpenGL's specification gives it.
        unsafe {
            Some(Functions {
                gen_framebuffers: function(c"glGenFramebuffers", c"glGenFramebuffersEXT")?,
                bind_framebuffer: function(c"glBindFramebuffer", c"glBindFramebufferEXT")?,
                gen_renderbuffers: function(c"glGenRenderbuffers", c"glGenRenderbuffersEXT")?,
                bind_renderbuffer: function(c"glBindRenderbuffer", c"glBindRenderbufferEXT")?,
                renderbuffer_storage: function(
                    c"glRenderbufferStorage",
                    c"glRenderbufferStorageEXT",
                )?,
                framebuffer_renderbuffer: function(
                    c"glFramebufferRenderbuffer",
                    c"glFramebufferRenderbufferEXT",
                )?,
                check_framebuffer_status: function(
                    c"glCheckFramebufferStatus",
                    c"glCheckFramebufferStatusEXT",
                )?,
                blend_func_separate: function(c"glBlendFuncSeparate", c"glBlendFuncSeparateEXT"),
            })
        }
    }
}

/// The offscreen context: a hidden window's (contexts need a window's
/// device context), drawing into a framebuffer object.
struct Canvas {
    dc: Handle,
    rc: Handle,
    gl: Functions,
    framebuffer: u32,
    /// Color, then depth and stencil.
    renderbuffers: [u32; 2],
    size: [i32; 2],
}

// SAFETY: the handles are used only under the canvas's lock, the context
// made current on the calling thread and released before it is let go.
unsafe impl Send for Canvas {}

/// The canvas, made when first needed; `None` inside once making it failed.
static CANVAS: Mutex<Option<Option<Canvas>>> = Mutex::new(None);

/// The context current on this thread before, put back when dropped.
struct Previous(Handle, Handle);

impl Previous {
    fn current() -> Previous {
        // SAFETY: plain queries.
        unsafe { Previous(wglGetCurrentDC(), wglGetCurrentContext()) }
    }
}

impl Drop for Previous {
    fn drop(&mut self) {
        // SAFETY: the context that was current here (or none).
        unsafe { wglMakeCurrent(self.0, self.1) };
    }
}

impl Canvas {
    /// A hidden window, its pixel format and the card's context.
    fn make() -> Option<Canvas> {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        const WS_POPUP: u32 = 0x8000_0000;
        // SAFETY: a window of a predefined class, never shown; its device
        // context; a pixel format the card offers; a context for it, made
        // current on this thread to read the functions (the previous one
        // put back when `_previous` drops).
        unsafe {
            let window = CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            if window.is_null() {
                return None;
            }
            let dc = GetDC(window);
            const DRAW_TO_WINDOW: u32 = 0x4;
            const SUPPORT_OPENGL: u32 = 0x20;
            const DOUBLE_BUFFER: u32 = 0x1;
            let pfd = PixelFormat {
                size: std::mem::size_of::<PixelFormat>() as u16,
                version: 1,
                flags: DRAW_TO_WINDOW | SUPPORT_OPENGL | DOUBLE_BUFFER,
                color_bits: 32,
                alpha_bits: 8,
                depth_bits: 24,
                stencil_bits: 8,
                ..Default::default()
            };
            let format = ChoosePixelFormat(dc, &pfd);
            if format == 0 || SetPixelFormat(dc, format, &pfd) == 0 {
                return None;
            }
            let rc = wglCreateContext(dc);
            if rc.is_null() {
                return None;
            }
            let _previous = Previous::current();
            if wglMakeCurrent(dc, rc) == 0 {
                return None;
            }
            let gl = Functions::load()?;
            let mut framebuffer = 0;
            let mut renderbuffers = [0; 2];
            (gl.gen_framebuffers)(1, &mut framebuffer);
            (gl.gen_renderbuffers)(2, renderbuffers.as_mut_ptr());
            Some(Canvas {
                dc,
                rc,
                gl,
                framebuffer,
                renderbuffers,
                size: [0, 0],
            })
        }
    }

    /// Its framebuffer bound at `size`, the context current. Whether it is
    /// complete.
    ///
    /// # Safety
    /// The canvas's context is current.
    unsafe fn bind(&mut self, size: [i32; 2]) -> bool {
        let gl = &self.gl;
        // SAFETY: the canvas's own objects, its context current.
        unsafe {
            (gl.bind_framebuffer)(GL_FRAMEBUFFER, self.framebuffer);
            if self.size != size {
                let [color, depth] = self.renderbuffers;
                (gl.bind_renderbuffer)(GL_RENDERBUFFER, color);
                (gl.renderbuffer_storage)(GL_RENDERBUFFER, GL_RGBA8, size[0], size[1]);
                (gl.framebuffer_renderbuffer)(
                    GL_FRAMEBUFFER,
                    GL_COLOR_ATTACHMENT0,
                    GL_RENDERBUFFER,
                    color,
                );
                (gl.bind_renderbuffer)(GL_RENDERBUFFER, depth);
                (gl.renderbuffer_storage)(GL_RENDERBUFFER, GL_DEPTH24_STENCIL8, size[0], size[1]);
                (gl.framebuffer_renderbuffer)(
                    GL_FRAMEBUFFER,
                    GL_DEPTH_STENCIL_ATTACHMENT,
                    GL_RENDERBUFFER,
                    depth,
                );
                (gl.bind_renderbuffer)(GL_RENDERBUFFER, 0);
                self.size = size;
            }
            (gl.check_framebuffer_status)(GL_FRAMEBUFFER) == GL_FRAMEBUFFER_COMPLETE
        }
    }
}

/// Whether overlays that draw with OpenGL can be shown here (the card's
/// OpenGL could be set up).
pub fn available() -> bool {
    let mut canvas = CANVAS.lock().unwrap_or_else(|e| e.into_inner());
    canvas.get_or_insert_with(Canvas::make).is_some()
}

/// The OpenGL projection taking canonical coordinates through `to_frame`
/// (to the view's 0–1, y down) to OpenGL's −1–1, y up: column by column.
pub(crate) fn projection(to_frame: [[f64; 3]; 2]) -> [f64; 16] {
    let [[a, b, c], [d, e, f]] = to_frame;
    [
        2.0 * a,
        -2.0 * d,
        0.0,
        0.0,
        2.0 * b,
        -2.0 * e,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        2.0 * c - 1.0,
        1.0 - 2.0 * f,
        0.0,
        1.0,
    ]
}

/// Runs `draw` (a plug-in's overlay drawing) with the offscreen context
/// current, cleared to nothing at `size` (the view's pixels) and set to
/// `projection` ([`projection`]); what it drew. `None` where there is no
/// OpenGL for it, or a size of nothing.
pub(crate) fn drawn(
    size: [i32; 2],
    projection: &[f64; 16],
    draw: impl FnOnce(),
) -> Option<Drawing> {
    let [w, h] = size;
    if w <= 0 || h <= 0 || w > 16384 || h > 16384 {
        return None;
    }
    let mut canvas = CANVAS.lock().unwrap_or_else(|e| e.into_inner());
    let canvas = canvas.get_or_insert_with(Canvas::make).as_mut()?;
    let _previous = Previous::current();
    // SAFETY: the canvas's context, current on this thread until
    // `_previous` puts the one before back; the state the plug-in changes
    // is kept in the attribute and matrix stacks and put back after it.
    unsafe {
        if wglMakeCurrent(canvas.dc, canvas.rc) == 0 || !canvas.bind(size) {
            return None;
        }
        glViewport(0, 0, w, h);
        glPushAttrib(GL_ALL_ATTRIB_BITS);
        glMatrixMode(GL_PROJECTION);
        glPushMatrix();
        glLoadMatrixd(projection.as_ptr());
        glMatrixMode(GL_MODELVIEW);
        glPushMatrix();
        glLoadIdentity();
        glDisable(GL_DEPTH_TEST);
        glDisable(GL_LIGHTING);
        glDisable(GL_TEXTURE_2D);
        glEnable(GL_BLEND);
        // Colors over nothing come out premultiplied, alpha adding up.
        match canvas.gl.blend_func_separate {
            Some(separate) => separate(
                GL_SRC_ALPHA,
                GL_ONE_MINUS_SRC_ALPHA,
                GL_ONE,
                GL_ONE_MINUS_SRC_ALPHA,
            ),
            None => glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA),
        }
        glClearColor(0.0, 0.0, 0.0, 0.0);
        glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT | GL_STENCIL_BUFFER_BIT);
        draw();
        glMatrixMode(GL_MODELVIEW);
        glPopMatrix();
        glMatrixMode(GL_PROJECTION);
        glPopMatrix();
        glPopAttrib();
        glFinish();
        // The plug-in may have bound its own: the canvas's is read.
        (canvas.gl.bind_framebuffer)(GL_FRAMEBUFFER, canvas.framebuffer);
        let row = w as usize * 4;
        let mut up = vec![0u8; row * h as usize];
        glPixelStorei(GL_PACK_ALIGNMENT, 1);
        glReadPixels(
            0,
            0,
            w,
            h,
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            up.as_mut_ptr().cast(),
        );
        (canvas.gl.bind_framebuffer)(GL_FRAMEBUFFER, 0);
        // OpenGL's rows go up from the bottom.
        let rgba = up.chunks_exact(row).rev().flatten().copied().collect();
        Some(Drawing {
            width: w as u32,
            height: h as u32,
            rgba,
        })
    }
}
