//! A system window of its own for a plug-in's editor (VST 3's editors are
//! views that live in a window of the system's): owned by the window
//! active when it opens (BananaV's), closed by its own button — noted, for
//! its owner to close it —, sized by its client area and resized by the
//! plug-in through a [`Resizer`]. On Windows and on Linux (an X11 window,
//! under XWayland too); elsewhere none opens.

/// A plug-in's window.
#[derive(Debug)]
pub struct PluginWindow {
    #[cfg(windows)]
    hwnd: isize,
    #[cfg(windows)]
    style: u32,
    #[cfg(target_os = "linux")]
    window: std::ffi::c_ulong,
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
}

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

#[cfg(not(any(windows, target_os = "linux")))]
impl Resizer {
    pub fn set_client_size(&self, _w: i32, _h: i32) {}
}

#[cfg(not(any(windows, target_os = "linux")))]
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

/// Whether plug-ins' windows open on this system.
pub const PLUGIN_WINDOWS: bool = cfg!(any(windows, target_os = "linux"));

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
