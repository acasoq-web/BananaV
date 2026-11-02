//! X11, through libX11 and libXrandr loaded at run time (as the window
//! library loads them): the mouse and the keypad outside BananaV's windows,
//! the screen's pixels, the displays, the active window, the owners of
//! dialog windows and the windows plug-ins' editors live in. BananaV's windows are X11 ones under Wayland too
//! (XWayland), unless `BANANAV_WAYLAND` asks for Wayland's own; without an
//! X server these give nothing.
//!
//! One connection of BananaV's own, used under a lock: X calls made from
//! one thread at a time.

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_ulong};
use std::ptr;
use std::sync::{Mutex, MutexGuard, OnceLock};

use x11_dl::xlib::{self, Display, Window, Xlib};
use x11_dl::xrandr::Xrandr;

struct X {
    lib: Xlib,
    randr: Option<Xrandr>,
    display: *mut Display,
    root: Window,
    atoms: HashMap<&'static str, c_ulong>,
    /// The mouse is held for the eyedropper (see [`capture_mouse`]).
    grabbed: bool,
    /// The eyedropper's cursor (0 until made).
    cross: c_ulong,
    /// The plug-ins' windows: what happened to them.
    plugin_windows: HashMap<Window, PluginNote>,
}

/// What happened to a plug-in's window since its owner last looked.
#[derive(Debug, Default)]
pub(crate) struct PluginNote {
    pub(crate) closed: bool,
    pub(crate) resized: Option<(i32, i32)>,
    /// Its size as last set or seen (a move is no resize).
    size: (i32, i32),
    resizable: bool,
}

// SAFETY: the connection is only used under the lock, by one thread at a
// time.
unsafe impl Send for X {}

/// The connection, when there is an X server.
fn x() -> Option<MutexGuard<'static, X>> {
    static X11: OnceLock<Option<Mutex<X>>> = OnceLock::new();
    X11.get_or_init(connect).as_ref()?.lock().ok()
}

/// X errors (a window gone between two calls) are not fatal: they are
/// left out.
unsafe extern "C" fn ignore_error(_: *mut Display, _: *mut xlib::XErrorEvent) -> c_int {
    0
}

fn connect() -> Option<Mutex<X>> {
    std::env::var_os("DISPLAY")?;
    let lib = Xlib::open().ok()?;
    // SAFETY: plain Xlib calls on a connection opened here.
    unsafe {
        let display = (lib.XOpenDisplay)(ptr::null());
        if display.is_null() {
            return None;
        }
        // Xlib's own handler ends the process on an error; the window
        // library's (when it runs on X11) is kept.
        let before = (lib.XSetErrorHandler)(Some(ignore_error));
        if before.is_some() {
            (lib.XSetErrorHandler)(before);
        }
        let root = (lib.XDefaultRootWindow)(display);
        Some(Mutex::new(X {
            randr: Xrandr::open().ok(),
            lib,
            display,
            root,
            atoms: HashMap::new(),
            grabbed: false,
            cross: 0,
            plugin_windows: HashMap::new(),
        }))
    }
}

impl X {
    fn atom(&mut self, name: &'static str) -> c_ulong {
        if let Some(&a) = self.atoms.get(name) {
            return a;
        }
        let Ok(c) = CString::new(name) else {
            return 0;
        };
        // SAFETY: an open connection and a NUL-terminated name.
        let a = unsafe { (self.lib.XInternAtom)(self.display, c.as_ptr(), xlib::False) };
        self.atoms.insert(name, a);
        a
    }

    /// A window property: its item size in bits, item count and bytes (as
    /// Xlib gives them: a `long` for each 32-bit item).
    fn property(
        &mut self,
        w: Window,
        atom: c_ulong,
        kind: c_ulong,
    ) -> Option<(c_int, usize, Vec<u8>)> {
        let (mut actual, mut format, mut items, mut after) = (0, 0, 0, 0);
        let mut data: *mut u8 = ptr::null_mut();
        // SAFETY: Xlib fills the out arguments; its data is copied, then
        // freed.
        unsafe {
            let r = (self.lib.XGetWindowProperty)(
                self.display,
                w,
                atom,
                0,
                4096,
                xlib::False,
                kind,
                &mut actual,
                &mut format,
                &mut items,
                &mut after,
                &mut data,
            );
            if r != xlib::Success as c_int || data.is_null() {
                return None;
            }
            let size = match format {
                32 => std::mem::size_of::<c_ulong>(),
                16 => 2,
                _ => 1,
            };
            let bytes = std::slice::from_raw_parts(data, items as usize * size).to_vec();
            (self.lib.XFree)(data.cast());
            (actual != 0).then_some((format, items as usize, bytes))
        }
    }

    /// A window property of 32-bit items (windows, cardinals).
    fn cardinals(&mut self, w: Window, name: &'static str) -> Vec<c_ulong> {
        let atom = self.atom(name);
        match self.property(w, atom, xlib::AnyPropertyType as c_ulong) {
            Some((32, _, bytes)) => bytes
                .as_chunks::<{ std::mem::size_of::<c_ulong>() }>()
                .0
                .iter()
                .map(|b| c_ulong::from_ne_bytes(*b))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// A window's title (`_NET_WM_NAME`, else `WM_NAME`).
    fn title(&mut self, w: Window) -> Option<String> {
        let (name, utf8) = (self.atom("_NET_WM_NAME"), self.atom("UTF8_STRING"));
        if let Some((8, _, bytes)) = self.property(w, name, utf8) {
            return Some(String::from_utf8_lossy(&bytes).into_owned());
        }
        let mut raw: *mut c_char = ptr::null_mut();
        // SAFETY: Xlib fills `raw` (freed below) or leaves it null.
        unsafe {
            if (self.lib.XFetchName)(self.display, w, &mut raw) == 0 || raw.is_null() {
                return None;
            }
            let t = CStr::from_ptr(raw).to_string_lossy().into_owned();
            (self.lib.XFree)(raw.cast());
            Some(t)
        }
    }

    /// This process's top-level windows: (window, title).
    fn own_windows(&mut self) -> Vec<(Window, String)> {
        let pid = std::process::id() as c_ulong;
        let root = self.root;
        let mut out = Vec::new();
        for w in self.cardinals(root, "_NET_CLIENT_LIST") {
            if self.cardinals(w, "_NET_WM_PID").first() == Some(&pid)
                && let Some(t) = self.title(w)
            {
                out.push((w, t));
            }
        }
        out
    }

    /// The pointer: where it is on the screen and the buttons' state.
    fn pointer(&self) -> Option<(i32, i32, c_uint)> {
        let (mut root, mut child) = (0, 0);
        let (mut rx, mut ry, mut wx, mut wy, mut mask) = (0, 0, 0, 0, 0);
        // SAFETY: an open connection; Xlib fills the out arguments.
        let ok = unsafe {
            (self.lib.XQueryPointer)(
                self.display,
                self.root,
                &mut root,
                &mut child,
                &mut rx,
                &mut ry,
                &mut wx,
                &mut wy,
                &mut mask,
            )
        };
        (ok != 0).then_some((rx, ry, mask))
    }
}

pub fn cursor() -> Option<(i32, i32)> {
    x()?.pointer().map(|(x, y, _)| (x, y))
}

pub fn main_button_down() -> bool {
    let Some(mut x) = x() else {
        return false;
    };
    // X maps the buttons itself: the first is the main one, swapped or not.
    let down = x
        .pointer()
        .is_some_and(|(_, _, mask)| mask & xlib::Button1Mask != 0);
    if down && x.grabbed {
        // The eyedropper's click ends the hold, as on Windows.
        // SAFETY: an open connection that holds the pointer.
        unsafe {
            (x.lib.XUngrabPointer)(x.display, xlib::CurrentTime);
            (x.lib.XFlush)(x.display);
        }
        x.grabbed = false;
    }
    down
}

pub fn numpad_down(c: char) -> bool {
    use x11_dl::keysym::*;
    let sym = match c {
        '0'..='9' => XK_KP_0 + (c as u32 - '0' as u32),
        '*' => XK_KP_Multiply,
        '+' => XK_KP_Add,
        '-' => XK_KP_Subtract,
        '.' => XK_KP_Decimal,
        '/' => XK_KP_Divide,
        _ => return false,
    };
    let Some(x) = x() else {
        return false;
    };
    let mut keys = [0u8; 32];
    // SAFETY: an open connection; the keymap is 32 bytes.
    let code = unsafe {
        let code = (x.lib.XKeysymToKeycode)(x.display, sym as c_ulong);
        (x.lib.XQueryKeymap)(x.display, keys.as_mut_ptr().cast::<c_char>());
        code as usize
    };
    code != 0 && keys[code / 8] & (1 << (code % 8)) != 0
}

/// A channel of a pixel as 8 bits, from its mask.
fn channel(pixel: c_ulong, mask: c_ulong) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let bits = (mask >> shift).count_ones();
    let v = (pixel & mask) >> shift;
    if bits >= 8 {
        (v >> (bits - 8)) as u8
    } else {
        // Fewer bits (16-bit screens): spread over the 8.
        ((v << (8 - bits)) | (v >> (2 * bits).saturating_sub(8))) as u8
    }
}

pub fn screen_color(px: i32, py: i32) -> Option<[u8; 3]> {
    let x = x()?;
    // SAFETY: an open connection; the image (one pixel inside the screen)
    // is freed with its own function.
    unsafe {
        let screen = (x.lib.XDefaultScreen)(x.display);
        let (w, h) = (
            (x.lib.XDisplayWidth)(x.display, screen),
            (x.lib.XDisplayHeight)(x.display, screen),
        );
        if px < 0 || py < 0 || px >= w || py >= h {
            return None;
        }
        let img = (x.lib.XGetImage)(x.display, x.root, px, py, 1, 1, !0, xlib::ZPixmap);
        if img.is_null() {
            return None;
        }
        let pixel = (*img).funcs.get_pixel.map(|get| get(img, 0, 0));
        let rgb = pixel.map(|p| {
            [
                channel(p, (*img).red_mask),
                channel(p, (*img).green_mask),
                channel(p, (*img).blue_mask),
            ]
        });
        if let Some(destroy) = (*img).funcs.destroy_image {
            destroy(img);
        }
        rgb
    }
}

/// Holds the pointer until the next click (or [`release_mouse`]): the click
/// acts on no window, while [`main_button_down`] still sees it. The cursor
/// is a cross meanwhile.
pub fn capture_mouse(_title: &str) -> bool {
    let Some(mut x) = x() else {
        return false;
    };
    // XC_crosshair.
    const CROSSHAIR: c_uint = 34;
    let events =
        (xlib::ButtonPressMask | xlib::ButtonReleaseMask | xlib::PointerMotionMask) as c_uint;
    // SAFETY: an open connection; the pointer is held by this connection.
    let r = unsafe {
        if x.cross == 0 {
            x.cross = (x.lib.XCreateFontCursor)(x.display, CROSSHAIR);
        }
        let r = (x.lib.XGrabPointer)(
            x.display,
            x.root,
            xlib::False,
            events,
            xlib::GrabModeAsync,
            xlib::GrabModeAsync,
            0,
            x.cross,
            xlib::CurrentTime,
        );
        (x.lib.XFlush)(x.display);
        r
    };
    x.grabbed = r == xlib::GrabSuccess;
    x.grabbed
}

pub fn release_mouse() {
    let Some(mut x) = x() else {
        return;
    };
    if x.grabbed {
        // SAFETY: an open connection that holds the pointer.
        unsafe {
            (x.lib.XUngrabPointer)(x.display, xlib::CurrentTime);
            (x.lib.XFlush)(x.display);
        }
        x.grabbed = false;
    }
}

pub fn app_is_active() -> Option<bool> {
    let mut x = x()?;
    let root = x.root;
    let active = *x.cardinals(root, "_NET_ACTIVE_WINDOW").first()?;
    if active == 0 {
        return Some(false);
    }
    let pid = x.cardinals(active, "_NET_WM_PID");
    Some(pid.first() == Some(&(std::process::id() as c_ulong)))
}

/// Makes the windows titled `child` transient for the one titled `owner`:
/// window managers keep them in front of it, minimize them with it and
/// leave them out of the taskbar.
pub fn set_owner(child: &str, owner: &str) -> bool {
    let Some(mut x) = x() else {
        // No X server: the window manager places them as it likes.
        return true;
    };
    let windows = x.own_windows();
    let Some(&(main, _)) = windows.iter().find(|(_, t)| t == owner) else {
        return false;
    };
    let mut exists = false;
    for &(w, ref title) in &windows {
        if title != child || w == main {
            continue;
        }
        exists = true;
        let mut now: Window = 0;
        // SAFETY: an open connection and live windows of this process.
        unsafe {
            (x.lib.XGetTransientForHint)(x.display, w, &mut now);
            if now != main {
                (x.lib.XSetTransientForHint)(x.display, w, main);
                (x.lib.XFlush)(x.display);
            }
        }
    }
    exists
}

impl X {
    /// A fixed-size window keeps `w` × `h` (the window manager offers no
    /// resizing).
    fn fix_size(&self, window: Window, w: i32, h: i32) {
        // SAFETY: an open connection; the hints are freed after use.
        unsafe {
            let hints = (self.lib.XAllocSizeHints)();
            if hints.is_null() {
                return;
            }
            // PMinSize | PMaxSize.
            (*hints).flags = (1 << 4) | (1 << 5);
            (*hints).min_width = w;
            (*hints).max_width = w;
            (*hints).min_height = h;
            (*hints).max_height = h;
            (self.lib.XSetWMNormalHints)(self.display, window, hints);
            (self.lib.XFree)(hints.cast());
        }
    }

    /// Reads the events waiting on the connection: the plug-ins' windows
    /// closed by their button or resized by the user (others are left
    /// out: nothing else here asks for events).
    fn plugin_events(&mut self) {
        let delete = self.atom("WM_DELETE_WINDOW") as std::ffi::c_long;
        // SAFETY: an open connection; each event is read whole.
        unsafe {
            while (self.lib.XPending)(self.display) > 0 {
                let mut e: xlib::XEvent = std::mem::zeroed();
                (self.lib.XNextEvent)(self.display, &mut e);
                match e.get_type() {
                    xlib::ClientMessage => {
                        let m = e.client_message;
                        if m.data.get_long(0) == delete
                            && let Some(n) = self.plugin_windows.get_mut(&m.window)
                        {
                            n.closed = true;
                        }
                    }
                    xlib::ConfigureNotify => {
                        let c = e.configure;
                        if let Some(n) = self.plugin_windows.get_mut(&c.window)
                            && n.size != (c.width, c.height)
                        {
                            n.size = (c.width, c.height);
                            n.resized = Some(n.size);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// A top-level window for a plug-in's editor, `w` × `h`, titled `title`:
/// transient for BananaV's active window, its close button noted (not
/// obeyed: its owner closes it), resizable or not. Not shown yet.
pub(crate) fn open_plugin_window(title: &str, w: i32, h: i32, resizable: bool) -> Option<Window> {
    let mut x = x()?;
    let (w, h) = (w.max(1), h.max(1));
    let root = x.root;
    let owner = x
        .cardinals(root, "_NET_ACTIVE_WINDOW")
        .first()
        .copied()
        .filter(|&a| {
            a != 0
                && x.cardinals(a, "_NET_WM_PID").first() == Some(&(std::process::id() as c_ulong))
        });
    let (net_name, utf8, pid_atom) = (
        x.atom("_NET_WM_NAME"),
        x.atom("UTF8_STRING"),
        x.atom("_NET_WM_PID"),
    );
    let mut delete = x.atom("WM_DELETE_WINDOW");
    // SAFETY: an open connection; the strings and values outlive the
    // calls.
    let window = unsafe {
        let screen = (x.lib.XDefaultScreen)(x.display);
        let black = (x.lib.XBlackPixel)(x.display, screen);
        let window = (x.lib.XCreateSimpleWindow)(
            x.display,
            root,
            0,
            0,
            w as c_uint,
            h as c_uint,
            0,
            black,
            black,
        );
        if window == 0 {
            return None;
        }
        if let Ok(c) = CString::new(title.replace('\0', "")) {
            (x.lib.XStoreName)(x.display, window, c.as_ptr());
        }
        (x.lib.XChangeProperty)(
            x.display,
            window,
            net_name,
            utf8,
            8,
            xlib::PropModeReplace,
            title.as_ptr(),
            title.len() as c_int,
        );
        let pid = std::process::id() as c_ulong;
        (x.lib.XChangeProperty)(
            x.display,
            window,
            pid_atom,
            xlib::XA_CARDINAL,
            32,
            xlib::PropModeReplace,
            (&pid as *const c_ulong).cast(),
            1,
        );
        (x.lib.XSetWMProtocols)(x.display, window, &mut delete, 1);
        (x.lib.XSelectInput)(x.display, window, xlib::StructureNotifyMask);
        if let Some(o) = owner {
            (x.lib.XSetTransientForHint)(x.display, window, o);
        }
        window
    };
    if !resizable {
        x.fix_size(window, w, h);
    }
    x.plugin_windows.insert(
        window,
        PluginNote {
            size: (w, h),
            resizable,
            ..PluginNote::default()
        },
    );
    // SAFETY: an open connection.
    unsafe { (x.lib.XFlush)(x.display) };
    Some(window)
}

pub(crate) fn show_plugin_window(window: Window) {
    if let Some(x) = x() {
        // SAFETY: an open connection and a window made here.
        unsafe {
            (x.lib.XMapRaised)(x.display, window);
            (x.lib.XFlush)(x.display);
        }
    }
}

/// The plug-in asks for another size: the window takes it.
pub(crate) fn resize_plugin_window(window: Window, w: i32, h: i32) {
    let Some(mut x) = x() else {
        return;
    };
    let (w, h) = (w.max(1), h.max(1));
    let fixed = match x.plugin_windows.get_mut(&window) {
        Some(n) => {
            n.size = (w, h);
            !n.resizable
        }
        None => return,
    };
    if fixed {
        x.fix_size(window, w, h);
    }
    // SAFETY: an open connection and a window made here.
    unsafe {
        (x.lib.XResizeWindow)(x.display, window, w as c_uint, h as c_uint);
        (x.lib.XFlush)(x.display);
    }
}

/// What happened to plug-in window `window` (the events read first).
pub(crate) fn plugin_window_note<T: Default>(
    window: Window,
    look: impl FnOnce(&mut PluginNote) -> T,
) -> T {
    let Some(mut x) = x() else {
        return T::default();
    };
    x.plugin_events();
    x.plugin_windows
        .get_mut(&window)
        .map(look)
        .unwrap_or_default()
}

pub(crate) fn close_plugin_window(window: Window) {
    let Some(mut x) = x() else {
        return;
    };
    if x.plugin_windows.remove(&window).is_some() {
        // SAFETY: an open connection and a window made here.
        unsafe {
            (x.lib.XDestroyWindow)(x.display, window);
            (x.lib.XFlush)(x.display);
        }
    }
}

/// The character a keysym types (Latin-1, Unicode keysyms and the dead
/// accents).
fn keysym_char(sym: c_ulong) -> Option<char> {
    let c = match sym {
        0x20..=0x7e | 0xa0..=0xff => sym as u32,
        0x0100_0000..=0x0110_ffff => (sym - 0x0100_0000) as u32,
        0xfe50 => '`' as u32,
        0xfe51 => '´' as u32,
        0xfe52 => '^' as u32,
        0xfe53 => '~' as u32,
        0xfe54 => '¯' as u32,
        0xfe57 => '¨' as u32,
        0xfe58 => '°' as u32,
        0xfe5b => '¸' as u32,
        _ => return None,
    };
    char::from_u32(c).filter(|c| !c.is_control())
}

/// The character the key of this scan code (PC set 1, as Windows numbers
/// them) types in the user's layout: X numbers the keys 8 above Linux's
/// codes, which are those scan codes.
pub fn key_char(scancode: u32) -> Option<char> {
    if !(1..0x80).contains(&scancode) {
        return None;
    }
    let x = x()?;
    // SAFETY: an open connection; a plain lookup.
    let sym = unsafe { (x.lib.XkbKeycodeToKeysym)(x.display, (scancode + 8) as u8, 0, 0) };
    keysym_char(sym)
}

/// The scale X's resources set (`Xft.dpi`, 96 per 1).
fn resource_scale(x: &X) -> f32 {
    // SAFETY: an open connection; the string is Xlib's.
    let text = unsafe {
        let s = (x.lib.XResourceManagerString)(x.display);
        if s.is_null() {
            return 1.0;
        }
        CStr::from_ptr(s).to_string_lossy().into_owned()
    };
    text.lines()
        .find_map(|l| l.strip_prefix("Xft.dpi:"))
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|&dpi| dpi > 0.0)
        .map_or(1.0, |dpi| dpi / 96.0)
}

/// The displays, in RandR's order of outputs (the window library's order
/// too).
pub fn displays() -> Vec<super::Display> {
    let Some(x) = x() else {
        return Vec::new();
    };
    let Some(randr) = &x.randr else {
        return Vec::new();
    };
    let scale = resource_scale(&x);
    let mut out = Vec::new();
    // SAFETY: an open connection; every RandR structure got is freed with
    // its own function, and read within its counts.
    unsafe {
        let res = (randr.XRRGetScreenResourcesCurrent)(x.display, x.root);
        if res.is_null() {
            return out;
        }
        let primary = (randr.XRRGetOutputPrimary)(x.display, x.root);
        let outputs = std::slice::from_raw_parts((*res).outputs, (*res).noutput.max(0) as usize);
        let modes = std::slice::from_raw_parts((*res).modes, (*res).nmode.max(0) as usize);
        for &o in outputs {
            let info = (randr.XRRGetOutputInfo)(x.display, res, o);
            if info.is_null() {
                continue;
            }
            if (*info).connection as c_int == x11_dl::xrandr::RR_Connected && (*info).crtc != 0 {
                let crtc = (randr.XRRGetCrtcInfo)(x.display, res, (*info).crtc);
                if !crtc.is_null() {
                    let name = std::slice::from_raw_parts(
                        (*info).name.cast::<u8>(),
                        (*info).nameLen.max(0) as usize,
                    );
                    let hz = modes.iter().find(|m| m.id == (*crtc).mode).map_or(0, |m| {
                        let mut lines = m.vTotal as f64;
                        if m.modeFlags & x11_dl::xrandr::RR_DoubleScan as c_ulong != 0 {
                            lines *= 2.0;
                        }
                        if m.modeFlags & x11_dl::xrandr::RR_Interlace as c_ulong != 0 {
                            lines /= 2.0;
                        }
                        let total = m.hTotal as f64 * lines;
                        if total > 0.0 {
                            (m.dotClock as f64 / total).round() as u32
                        } else {
                            0
                        }
                    });
                    out.push(super::Display {
                        name: String::from_utf8_lossy(name).into_owned(),
                        x: (*crtc).x,
                        y: (*crtc).y,
                        width: (*crtc).width,
                        height: (*crtc).height,
                        hz,
                        primary: o == primary,
                        scale,
                    });
                    (randr.XRRFreeCrtcInfo)(crtc);
                }
            }
            (randr.XRRFreeOutputInfo)(info);
        }
        (randr.XRRFreeScreenResources)(res);
    }
    // Without a primary output set, the first display is.
    if !out.iter().any(|d| d.primary)
        && let Some(first) = out.first_mut()
    {
        first.primary = true;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_of_any_depth_give_8_bits() {
        assert_eq!(channel(0x00ff_8000, 0x00ff_0000), 0xff);
        assert_eq!(channel(0x00ff_8000, 0x0000_ff00), 0x80);
        // 5-6-5: full and half.
        assert_eq!(channel(0xf800, 0xf800), 0xff);
        assert_eq!(channel(0x0400, 0x07e0), 0x82);
    }

    #[test]
    fn keysyms_type_their_characters() {
        assert_eq!(keysym_char(0x5b), Some('['));
        assert_eq!(keysym_char(0xe7), Some('ç'));
        assert_eq!(keysym_char(0xfe51), Some('´'));
        assert_eq!(keysym_char(0x0100_20ac), Some('€'));
        assert_eq!(keysym_char(0xff0d), None);
    }

    /// With an X server (`xvfb-run` in the CI), every service answers.
    #[test]
    fn the_x_server_answers() {
        if std::env::var_os("DISPLAY").is_none() {
            return;
        }
        assert!(cursor().is_some());
        let d = displays();
        assert!(!d.is_empty(), "no display");
        assert!(d.iter().filter(|d| d.primary).count() == 1);
        let (cx, cy) = (d[0].x + 1, d[0].y + 1);
        assert!(screen_color(cx, cy).is_some());
        assert_eq!(screen_color(-5, -5), None);
        assert!(!numpad_down('5'));
        assert!(!main_button_down());
        assert!(key_char(0x1e).is_some_and(|c| c.eq_ignore_ascii_case(&'a')));
        assert!(capture_mouse("anything"));
        release_mouse();
    }
}
