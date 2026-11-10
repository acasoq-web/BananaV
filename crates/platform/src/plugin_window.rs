//! A system window of its own for a plug-in's editor (VST 3's editors are
//! views that live in a window of the system's): owned by the window
//! active when it opens (BananaV's), closed by its own button — noted, for
//! its owner to close it —, sized by its client area and resized by the
//! plug-in through a [`Resizer`]. On Windows, on Linux (an X11 window,
//! under XWayland too) and on macOS (an `NSWindow`); elsewhere none opens.

/// A plug-in's window.
#[derive(Debug)]
pub struct PluginWindow {
    #[cfg(windows)]
    hwnd: isize,
    #[cfg(windows)]
    style: u32,
    #[cfg(target_os = "linux")]
    window: std::ffi::c_ulong,
    #[cfg(target_os = "macos")]
    window: objc2::rc::Retained<objc2_app_kit::NSWindow>,
    /// Shown once (closed: hidden since).
    #[cfg(target_os = "macos")]
    shown: std::cell::Cell<bool>,
    /// Its content's size last known, in points.
    #[cfg(target_os = "macos")]
    size: std::cell::Cell<(i32, i32)>,
}

/// A plug-in's editor inside a window of BananaV's (the FX window), as
/// VEGAS shows one: a child window of it (a view of it on macOS), placed
/// where the window's page leaves room for the editor, moving with it
/// (Windows and macOS; elsewhere none opens). A plug-in in another process
/// (the 32-bit bridge) may live in it too.
#[derive(Debug)]
pub struct EmbeddedWindow {
    #[cfg(windows)]
    hwnd: isize,
    #[cfg(target_os = "macos")]
    view: objc2::rc::Retained<objc2_app_kit::NSView>,
}

/// Resizes an [`EmbeddedWindow`] from anywhere (its plug-in asks while
/// it is shown).
#[derive(Clone, Copy, Debug)]
pub struct EmbeddedSizer {
    #[cfg(windows)]
    hwnd: isize,
    /// The view's address (used on the main thread while it lives).
    #[cfg(target_os = "macos")]
    view: usize,
}

/// Resizes a plug-in's window from anywhere (the plug-in asks while it is
/// being shown).
#[derive(Clone, Copy, Debug)]
pub struct Resizer {
    #[cfg(windows)]
    hwnd: isize,
    #[cfg(windows)]
    style: u32,
    #[cfg(target_os = "linux")]
    window: std::ffi::c_ulong,
    /// The window's address (used on the main thread while it lives).
    #[cfg(target_os = "macos")]
    window: usize,
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod imp {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    use super::{PluginWindow, Resizer};

    type Hwnd = isize;
    type WndProc = unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize;

    #[repr(C)]
    struct WndClassExW {
        size: u32,
        style: u32,
        proc_: WndProc,
        cls_extra: i32,
        wnd_extra: i32,
        instance: isize,
        icon: isize,
        cursor: isize,
        background: isize,
        menu_name: *const u16,
        class_name: *const u16,
        icon_small: isize,
    }

    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterClassExW(class: *const WndClassExW) -> u16;
        fn CreateWindowExW(
            ex: u32,
            class: *const u16,
            name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: Hwnd,
            menu: isize,
            instance: isize,
            param: *mut std::ffi::c_void,
        ) -> Hwnd;
        fn DefWindowProcW(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> isize;
        fn DestroyWindow(hwnd: Hwnd) -> i32;
        fn ShowWindow(hwnd: Hwnd, cmd: i32) -> i32;
        fn SetWindowPos(hwnd: Hwnd, after: Hwnd, x: i32, y: i32, w: i32, h: i32, flags: u32)
        -> i32;
        fn AdjustWindowRectEx(rect: *mut Rect, style: u32, menu: i32, ex: u32) -> i32;
        fn GetActiveWindow() -> Hwnd;
        fn GetAncestor(hwnd: Hwnd, flags: u32) -> Hwnd;
        fn LoadCursorW(instance: isize, name: *const u16) -> isize;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleW(name: *const u16) -> isize;
    }

    const WM_SIZE: u32 = 0x0005;
    const WM_CLOSE: u32 = 0x0010;
    const WS_CAPTION: u32 = 0x00C0_0000;
    const WS_SYSMENU: u32 = 0x0008_0000;
    const WS_THICKFRAME: u32 = 0x0004_0000;
    const WS_MINIMIZEBOX: u32 = 0x0002_0000;
    const WS_MAXIMIZEBOX: u32 = 0x0001_0000;
    const WS_CLIPCHILDREN: u32 = 0x0200_0000;
    const CW_USEDEFAULT: i32 = 0x8000_0000_u32 as i32;
    const SW_SHOW: i32 = 5;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const GA_ROOT: u32 = 2;
    /// `COLOR_WINDOW + 1`.
    const WINDOW_BRUSH: isize = 6;
    /// `IDC_ARROW`.
    const ARROW: usize = 32512;

    /// What the windows' procedure noted for each window: closed, its
    /// client area's new size.
    #[derive(Clone, Copy, Debug, Default)]
    struct Notes {
        closed: bool,
        resized: Option<(i32, i32)>,
    }

    fn notes() -> &'static Mutex<HashMap<Hwnd, Notes>> {
        static NOTES: OnceLock<Mutex<HashMap<Hwnd, Notes>>> = OnceLock::new();
        NOTES.get_or_init(Default::default)
    }

    fn note(hwnd: Hwnd, f: impl FnOnce(&mut Notes)) {
        let mut all = notes().lock().unwrap_or_else(|e| e.into_inner());
        f(all.entry(hwnd).or_default());
    }

    unsafe extern "system" fn window_proc(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> isize {
        match msg {
            // The owner closes it (the plug-in's view first).
            WM_CLOSE => {
                note(hwnd, |n| n.closed = true);
                0
            }
            WM_SIZE => {
                let (cw, ch) = ((l & 0xFFFF) as i32, ((l >> 16) & 0xFFFF) as i32);
                note(hwnd, |n| n.resized = Some((cw, ch)));
                // SAFETY: the system's default handling.
                unsafe { DefWindowProcW(hwnd, msg, w, l) }
            }
            // SAFETY: the system's default handling.
            _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// The window class, registered once.
    fn class() -> &'static [u16] {
        static CLASS: OnceLock<Vec<u16>> = OnceLock::new();
        CLASS.get_or_init(|| {
            let name = wide("BananaVPluginWindow");
            // SAFETY: a class description whose strings outlive the call
            // (the name is kept in the static).
            unsafe {
                let instance = GetModuleHandleW(std::ptr::null());
                let class = WndClassExW {
                    size: std::mem::size_of::<WndClassExW>() as u32,
                    style: 0,
                    proc_: window_proc,
                    cls_extra: 0,
                    wnd_extra: 0,
                    instance,
                    icon: 0,
                    cursor: LoadCursorW(0, ARROW as *const u16),
                    background: WINDOW_BRUSH,
                    menu_name: std::ptr::null(),
                    class_name: name.as_ptr(),
                    icon_small: 0,
                };
                RegisterClassExW(&class);
            }
            name
        })
    }

    impl Resizer {
        /// Gives the window a client area `w` × `h` big.
        pub fn set_client_size(&self, w: i32, h: i32) {
            let mut r = Rect {
                left: 0,
                top: 0,
                right: w.max(1),
                bottom: h.max(1),
            };
            // SAFETY: the window's own handle and style.
            unsafe {
                AdjustWindowRectEx(&mut r, self.style, 0, 0);
                SetWindowPos(
                    self.hwnd,
                    0,
                    0,
                    0,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }

    impl PluginWindow {
        pub fn open(title: &str, w: i32, h: i32, resizable: bool) -> Option<PluginWindow> {
            let mut style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
            if resizable {
                style |= WS_THICKFRAME | WS_MAXIMIZEBOX;
            }
            let mut r = Rect {
                left: 0,
                top: 0,
                right: w.max(1),
                bottom: h.max(1),
            };
            let title = wide(title);
            // SAFETY: strings that outlive the call; the owner is a window
            // of this thread (or none).
            let hwnd = unsafe {
                AdjustWindowRectEx(&mut r, style, 0, 0);
                let active = GetActiveWindow();
                let owner = if active != 0 {
                    GetAncestor(active, GA_ROOT)
                } else {
                    0
                };
                CreateWindowExW(
                    0,
                    class().as_ptr(),
                    title.as_ptr(),
                    style,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    r.right - r.left,
                    r.bottom - r.top,
                    owner,
                    0,
                    GetModuleHandleW(std::ptr::null()),
                    std::ptr::null_mut(),
                )
            };
            (hwnd != 0).then_some(PluginWindow { hwnd, style })
        }

        pub fn handle(&self) -> *mut std::ffi::c_void {
            self.hwnd as *mut std::ffi::c_void
        }

        pub fn resizer(&self) -> Resizer {
            Resizer {
                hwnd: self.hwnd,
                style: self.style,
            }
        }

        pub fn show(&self) {
            // SAFETY: the window's own handle.
            unsafe { ShowWindow(self.hwnd, SW_SHOW) };
        }

        pub fn closed(&self) -> bool {
            notes()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&self.hwnd)
                .is_some_and(|n| n.closed)
        }

        pub fn take_resized(&self) -> Option<(i32, i32)> {
            notes()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_mut(&self.hwnd)
                .and_then(|n| n.resized.take())
        }
    }

    impl Drop for PluginWindow {
        fn drop(&mut self) {
            // SAFETY: the window this made.
            unsafe { DestroyWindow(self.hwnd) };
            notes()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&self.hwnd);
        }
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsWindow(hwnd: Hwnd) -> i32;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateSolidBrush(color: u32) -> isize;
    }

    const WS_CHILD: u32 = 0x4000_0000;
    const WS_VISIBLE: u32 = 0x1000_0000;
    const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
    const SW_HIDE: i32 = 0;
    const SW_SHOWNOACTIVATE: i32 = 4;

    /// The editors' class inside BananaV's windows: no frame, the
    /// interface's dark grey under the plug-in's drawing.
    fn child_class() -> &'static [u16] {
        static CLASS: OnceLock<Vec<u16>> = OnceLock::new();
        CLASS.get_or_init(|| {
            let name = wide("BananaVPluginEditor");
            // SAFETY: a class description whose strings outlive the call;
            // the brush lives as long as the program.
            unsafe {
                let instance = GetModuleHandleW(std::ptr::null());
                let class = WndClassExW {
                    size: std::mem::size_of::<WndClassExW>() as u32,
                    style: 0,
                    proc_: DefWindowProcW,
                    cls_extra: 0,
                    wnd_extra: 0,
                    instance,
                    icon: 0,
                    cursor: LoadCursorW(0, ARROW as *const u16),
                    // 0x00BBGGRR.
                    background: CreateSolidBrush(0x0026_2626),
                    menu_name: std::ptr::null(),
                    class_name: name.as_ptr(),
                    icon_small: 0,
                };
                RegisterClassExW(&class);
            }
            name
        })
    }

    impl super::EmbeddedWindow {
        /// A child of window `parent` (a handle of the system's), at `x`,
        /// `y` in its client area, `w` × `h` pixels big.
        pub fn open(parent: isize, x: i32, y: i32, w: i32, h: i32) -> Option<Self> {
            // SAFETY: a live window of this program as the parent (checked),
            // the class registered.
            let hwnd = unsafe {
                if parent == 0 || IsWindow(parent) == 0 {
                    return None;
                }
                CreateWindowExW(
                    0,
                    child_class().as_ptr(),
                    std::ptr::null(),
                    WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                    x,
                    y,
                    w.max(1),
                    h.max(1),
                    parent,
                    0,
                    GetModuleHandleW(std::ptr::null()),
                    std::ptr::null_mut(),
                )
            };
            (hwnd != 0).then_some(Self { hwnd })
        }

        pub fn handle(&self) -> *mut std::ffi::c_void {
            self.hwnd as *mut std::ffi::c_void
        }

        /// Its place and size in its parent's client area, in pixels.
        pub fn place(&self, x: i32, y: i32, w: i32, h: i32) {
            // SAFETY: the window's own handle.
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    0,
                    x,
                    y,
                    w.max(1),
                    h.max(1),
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }

        /// Shows or hides it (an egui menu over it shows through).
        pub fn set_visible(&self, on: bool) {
            // SAFETY: the window's own handle.
            unsafe { ShowWindow(self.hwnd, if on { SW_SHOWNOACTIVATE } else { SW_HIDE }) };
        }

        /// It is still there (its parent was not closed under it).
        pub fn alive(&self) -> bool {
            // SAFETY: a plain query.
            unsafe { IsWindow(self.hwnd) != 0 }
        }

        /// What resizes it from anywhere (the plug-in asks while it is
        /// shown).
        pub fn sizer(&self) -> super::EmbeddedSizer {
            super::EmbeddedSizer { hwnd: self.hwnd }
        }
    }

    impl super::EmbeddedSizer {
        /// Makes it `w` x `h` pixels big, where it is.
        pub fn set_size(&self, w: i32, h: i32) {
            // SAFETY: a window handle (a call on one gone fails, harmlessly).
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    0,
                    0,
                    0,
                    w.max(1),
                    h.max(1),
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }

    impl Drop for super::EmbeddedWindow {
        fn drop(&mut self) {
            // SAFETY: the window this made (gone already when its parent
            // was closed: the call then fails, harmlessly).
            unsafe { DestroyWindow(self.hwnd) };
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
impl EmbeddedSizer {
    pub fn set_size(&self, _w: i32, _h: i32) {}
}

#[cfg(not(any(windows, target_os = "macos")))]
impl EmbeddedWindow {
    pub fn sizer(&self) -> EmbeddedSizer {
        EmbeddedSizer {}
    }

    pub fn open(_parent: isize, _x: i32, _y: i32, _w: i32, _h: i32) -> Option<Self> {
        None
    }

    pub fn handle(&self) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }

    pub fn place(&self, _x: i32, _y: i32, _w: i32, _h: i32) {}

    pub fn set_visible(&self, _on: bool) {}

    pub fn alive(&self) -> bool {
        false
    }
}

/// Whether plug-ins' editors live inside BananaV's own windows on this
/// system (VEGAS's way); elsewhere they open in windows of their own.
pub const EMBEDDED_EDITORS: bool = cfg!(any(windows, target_os = "macos"));

/// Whether plug-ins measure their editors in points (macOS: Cocoa's
/// measure, the Retina display's two pixels a point) rather than pixels:
/// an [`EmbeddedWindow`]'s and a [`PluginWindow`]'s sizes are in their
/// measure.
pub const EDITOR_POINTS: bool = cfg!(target_os = "macos");

#[cfg(target_os = "linux")]
impl Resizer {
    pub fn set_client_size(&self, w: i32, h: i32) {
        crate::x11::resize_plugin_window(self.window, w, h);
    }
}

#[cfg(target_os = "linux")]
impl PluginWindow {
    pub fn open(title: &str, w: i32, h: i32, resizable: bool) -> Option<PluginWindow> {
        crate::x11::open_plugin_window(title, w, h, resizable).map(|window| PluginWindow { window })
    }

    /// The X11 window's id, as VST 3's X11EmbedWindowID wants it.
    pub fn handle(&self) -> *mut std::ffi::c_void {
        self.window as usize as *mut std::ffi::c_void
    }

    pub fn resizer(&self) -> Resizer {
        Resizer {
            window: self.window,
        }
    }

    pub fn show(&self) {
        crate::x11::show_plugin_window(self.window);
    }

    pub fn closed(&self) -> bool {
        crate::x11::plugin_window_note(self.window, |n| n.closed)
    }

    pub fn take_resized(&self) -> Option<(i32, i32)> {
        crate::x11::plugin_window_note(self.window, |n| n.resized.take())
    }
}

#[cfg(target_os = "linux")]
impl Drop for PluginWindow {
    fn drop(&mut self) {
        crate::x11::close_plugin_window(self.window);
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
impl Resizer {
    pub fn set_client_size(&self, _w: i32, _h: i32) {}
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
impl PluginWindow {
    pub fn open(_title: &str, _w: i32, _h: i32, _resizable: bool) -> Option<PluginWindow> {
        None
    }

    pub fn handle(&self) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }

    pub fn resizer(&self) -> Resizer {
        Resizer {}
    }

    pub fn show(&self) {}

    pub fn closed(&self) -> bool {
        true
    }

    pub fn take_resized(&self) -> Option<(i32, i32)> {
        None
    }
}

/// macOS: the plug-ins' views (VST 3's "NSView", VST 2's editors) in an
/// `NSView` of BananaV's — inside the FX window, kept at its place from the
/// window's top as it resizes — or in a window of their own, a child of
/// the window active when it opened. AppKit wants them touched on the main
/// thread: a plug-in resizing from another thread is sent there, and what
/// was closed meanwhile is left alone (the live views and windows are
/// listed).
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod mac {
    use std::sync::Mutex;

    use objc2::rc::Retained;
    use objc2::{MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSApplication, NSAutoresizingMaskOptions, NSBackingStoreType, NSView, NSWindow,
        NSWindowOrderingMode, NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

    use super::{EmbeddedSizer, EmbeddedWindow, PluginWindow, Resizer};

    /// The views and windows made here and not closed yet, by address.
    static LIVE: Mutex<Vec<usize>> = Mutex::new(Vec::new());

    fn live(addr: usize) -> bool {
        LIVE.lock().is_ok_and(|l| l.contains(&addr))
    }

    fn add(addr: usize) {
        if let Ok(mut l) = LIVE.lock() {
            l.push(addr);
        }
    }

    fn remove(addr: usize) {
        if let Ok(mut l) = LIVE.lock() {
            l.retain(|&a| a != addr);
        }
    }

    /// The program's window at `addr` (an `NSWindow`'s), if it is one.
    fn window_at(mtm: MainThreadMarker, addr: isize) -> Option<Retained<NSWindow>> {
        NSApplication::sharedApplication(mtm)
            .windows()
            .to_vec()
            .into_iter()
            .find(|w| Retained::as_ptr(w) as isize == addr)
    }

    fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
        NSRect::new(NSPoint::new(x, y), NSSize::new(w.max(1.0), h.max(1.0)))
    }

    impl EmbeddedWindow {
        /// A view in the content of window `parent` (an `NSWindow`, from
        /// `window_handle`), at `x`, `y` pixels from its top left, `w` × `h`
        /// points big (the editor's own measure on macOS).
        pub fn open(parent: isize, x: i32, y: i32, w: i32, h: i32) -> Option<Self> {
            let mtm = MainThreadMarker::new()?;
            let window = window_at(mtm, parent)?;
            let content = window.contentView()?;
            let scale = window.backingScaleFactor().max(1.0);
            let top = content.bounds().size.height;
            let (x, y, w, h) = (
                f64::from(x) / scale,
                f64::from(y) / scale,
                f64::from(w),
                f64::from(h),
            );
            let view = NSView::initWithFrame(NSView::alloc(mtm), rect(x, top - y - h, w, h));
            // Its distance from the window's top kept as the window grows.
            view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
            content.addSubview(&view);
            add(Retained::as_ptr(&view) as usize);
            Some(Self { view })
        }

        /// The view (what the plug-in's view goes into).
        pub fn handle(&self) -> *mut std::ffi::c_void {
            Retained::as_ptr(&self.view).cast_mut().cast()
        }

        /// Its place, `x`, `y` pixels from the window's top left, and its
        /// size, `w` × `h` points.
        pub fn place(&self, x: i32, y: i32, w: i32, h: i32) {
            let Some(window) = self.view.window() else {
                return;
            };
            let Some(content) = window.contentView() else {
                return;
            };
            let scale = window.backingScaleFactor().max(1.0);
            let top = content.bounds().size.height;
            let (x, y, w, h) = (
                f64::from(x) / scale,
                f64::from(y) / scale,
                f64::from(w),
                f64::from(h),
            );
            self.view.setFrame(rect(x, top - y - h, w, h));
        }

        /// Shows or hides it (an egui menu over it shows through).
        pub fn set_visible(&self, on: bool) {
            self.view.setHidden(!on);
        }

        /// It is still in its window (the window was not closed under it).
        pub fn alive(&self) -> bool {
            self.view.window().is_some()
        }

        pub fn sizer(&self) -> EmbeddedSizer {
            EmbeddedSizer {
                view: Retained::as_ptr(&self.view) as usize,
            }
        }
    }

    impl Drop for EmbeddedWindow {
        fn drop(&mut self) {
            remove(Retained::as_ptr(&self.view) as usize);
            self.view.removeFromSuperview();
        }
    }

    impl EmbeddedSizer {
        /// Makes it `w` × `h` points big, its top left where it is (on the
        /// main thread; nothing once it is closed).
        pub fn set_size(&self, w: i32, h: i32) {
            let addr = self.view;
            bv_apple::on_main(move || {
                if !live(addr) {
                    return;
                }
                // SAFETY: a view made here and not dropped yet (listed),
                // on the main thread.
                let view = unsafe { &*(addr as *const NSView) };
                let f = view.frame();
                let (w, h) = (f64::from(w).max(1.0), f64::from(h).max(1.0));
                let top = f.origin.y + f.size.height;
                view.setFrame(rect(f.origin.x, top - h, w, h));
            });
        }
    }

    impl Resizer {
        /// Gives the window a content area `w` × `h` points big (on the
        /// main thread; nothing once it is closed).
        pub fn set_client_size(&self, w: i32, h: i32) {
            let addr = self.window;
            bv_apple::on_main(move || {
                if !live(addr) {
                    return;
                }
                // SAFETY: a window made here and not dropped yet (listed),
                // on the main thread.
                let window = unsafe { &*(addr as *const NSWindow) };
                window.setContentSize(NSSize::new(f64::from(w).max(1.0), f64::from(h).max(1.0)));
            });
        }
    }

    impl PluginWindow {
        pub fn open(title: &str, w: i32, h: i32, resizable: bool) -> Option<PluginWindow> {
            let mtm = MainThreadMarker::new()?;
            let mut style = NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Miniaturizable;
            if resizable {
                style |= NSWindowStyleMask::Resizable;
            }
            let frame = rect(0.0, 0.0, f64::from(w), f64::from(h));
            // SAFETY: a new window, kept by this object (not released when
            // closed: its close button only hides it, the owner drops it).
            let window = unsafe {
                let window = NSWindow::initWithContentRect_styleMask_backing_defer(
                    NSWindow::alloc(mtm),
                    frame,
                    style,
                    NSBackingStoreType::Buffered,
                    false,
                );
                window.setReleasedWhenClosed(false);
                window
            };
            window.setTitle(&NSString::from_str(title));
            window.center();
            // In front of the window active now (BananaV's), with it.
            let app = NSApplication::sharedApplication(mtm);
            if let Some(owner) = app.keyWindow().or_else(|| app.mainWindow()) {
                // SAFETY: two live windows of this program.
                unsafe { owner.addChildWindow_ordered(&window, NSWindowOrderingMode::Above) };
            }
            add(Retained::as_ptr(&window) as usize);
            Some(PluginWindow {
                window,
                shown: std::cell::Cell::new(false),
                size: std::cell::Cell::new((w, h)),
            })
        }

        /// Its content view (what the plug-in's view goes into).
        pub fn handle(&self) -> *mut std::ffi::c_void {
            self.window.contentView().map_or(std::ptr::null_mut(), |v| {
                Retained::as_ptr(&v).cast_mut().cast()
            })
        }

        pub fn resizer(&self) -> Resizer {
            Resizer {
                window: Retained::as_ptr(&self.window) as usize,
            }
        }

        pub fn show(&self) {
            self.window.makeKeyAndOrderFront(None);
            self.shown.set(true);
        }

        /// Closed by its button (hidden, not minimized).
        pub fn closed(&self) -> bool {
            self.shown.get() && !self.window.isVisible() && !self.window.isMiniaturized()
        }

        /// Its content's new size, in points, when the user resized it.
        pub fn take_resized(&self) -> Option<(i32, i32)> {
            let s = self.window.contentView()?.frame().size;
            let now = (s.width.round() as i32, s.height.round() as i32);
            (now != self.size.replace(now)).then_some(now)
        }
    }

    impl Drop for PluginWindow {
        fn drop(&mut self) {
            remove(Retained::as_ptr(&self.window) as usize);
            if let Some(parent) = self.window.parentWindow() {
                parent.removeChildWindow(&self.window);
            }
            self.window.orderOut(None);
            self.window.close();
        }
    }
}

/// Whether plug-ins' windows open on this system.
pub const PLUGIN_WINDOWS: bool = cfg!(any(windows, target_os = "linux", target_os = "macos"));

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn a_window_takes_its_client_size_and_closes() {
        // Never shown.
        let w = PluginWindow::open("Test", 300, 120, true).expect("a window");
        assert!(!w.closed());
        w.resizer().set_client_size(330, 130);
        assert_eq!(w.take_resized(), Some((330, 130)));
        assert_eq!(w.take_resized(), None);
        drop(w);
    }
}
