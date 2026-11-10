//! The 32-bit bridge's window for a plug-in's editor (Windows): a window of
//! its own, sized to the editor, closed by its button; and the bridge's
//! thread's message queue, run between requests.

#![allow(unsafe_code)]

use std::cell::Cell;
use std::ffi::c_void;

type WndProc = unsafe extern "system" fn(isize, u32, usize, isize) -> isize;

#[repr(C)]
struct WndClassEx {
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

#[repr(C)]
struct Msg {
    hwnd: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt: [i32; 2],
    private: u32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassExW(c: *const WndClassEx) -> u16;
    fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: isize,
        menu: isize,
        instance: isize,
        param: *mut c_void,
    ) -> isize;
    fn DefWindowProcW(h: isize, m: u32, w: usize, l: isize) -> isize;
    fn DestroyWindow(h: isize) -> i32;
    fn ShowWindow(h: isize, cmd: i32) -> i32;
    fn SetWindowPos(h: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    fn AdjustWindowRectEx(r: *mut Rect, style: u32, menu: i32, ex: u32) -> i32;
    fn SetForegroundWindow(h: isize) -> i32;
    fn LoadCursorW(instance: isize, name: *const u16) -> isize;
    fn PeekMessageW(msg: *mut Msg, h: isize, min: u32, max: u32, remove: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn MsgWaitForMultipleObjects(
        count: u32,
        handles: *const isize,
        wait_all: i32,
        ms: u32,
        wake: u32,
    ) -> u32;
    fn PostThreadMessageW(thread: u32, msg: u32, w: usize, l: isize) -> i32;
    fn IsWindow(h: isize) -> i32;
    fn SetParent(child: isize, parent: isize) -> isize;
    #[cfg(target_pointer_width = "64")]
    fn SetWindowLongPtrW(h: isize, index: i32, value: isize) -> isize;
    #[cfg(target_pointer_width = "64")]
    fn GetWindowLongPtrW(h: isize, index: i32) -> isize;
    // 32-bit Windows has the 32-bit ones alone (the others are macros).
    #[cfg(target_pointer_width = "32")]
    #[link_name = "SetWindowLongW"]
    fn SetWindowLongPtrW(h: isize, index: i32, value: isize) -> isize;
    #[cfg(target_pointer_width = "32")]
    #[link_name = "GetWindowLongW"]
    fn GetWindowLongPtrW(h: isize, index: i32) -> isize;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
    fn GetCurrentThreadId() -> u32;
}

const WS_CAPTION: u32 = 0x00C0_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_MINIMIZEBOX: u32 = 0x0002_0000;
const STYLE: u32 = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
const CW_USEDEFAULT: i32 = 0x8000_0000_u32 as i32;
const WM_CLOSE: u32 = 0x0010;
const WM_APP: u32 = 0x8000;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const SW_SHOW: i32 = 5;
const SW_SHOWNA: i32 = 8;
const GWL_STYLE: i32 = -16;
const WS_CHILD: u32 = 0x4000_0000;
const WS_CLIPCHILDREN: u32 = 0x0200_0000;
const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
const SWP_FRAMECHANGED: u32 = 0x0020;
const PM_REMOVE: u32 = 1;
const QS_ALLINPUT: u32 = 0x04FF;
const IDC_ARROW: usize = 32512;

thread_local! {
    /// The user closed the editor's window.
    static CLOSED: Cell<bool> = const { Cell::new(false) };
}

unsafe extern "system" fn proc_(h: isize, m: u32, w: usize, l: isize) -> isize {
    if m == WM_CLOSE {
        // The bridge closes the editor first, then the window.
        CLOSED.with(|c| c.set(true));
        return 0;
    }
    // SAFETY: the system's default handling.
    unsafe { DefWindowProcW(h, m, w, l) }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// The window's outer size for a client area of `w` × `h`.
fn outer(w: i32, h: i32) -> (i32, i32) {
    let mut r = Rect {
        left: 0,
        top: 0,
        right: w,
        bottom: h,
    };
    // SAFETY: a rectangle to adjust.
    unsafe { AdjustWindowRectEx(&mut r, STYLE, 0, 0) };
    (r.right - r.left, r.bottom - r.top)
}

/// The editor's window.
#[derive(Debug)]
pub struct EditorWindow {
    hwnd: isize,
}

impl EditorWindow {
    /// A window titled `title` with a client area of `w` × `h` (hidden
    /// until [`EditorWindow::show`]).
    pub fn open(title: &str, w: i32, h: i32) -> Option<EditorWindow> {
        let class = wide("BananaVBridgeEditor");
        let name = wide(title);
        // SAFETY: plain calls; the class is registered once per process
        // (a second registration fails harmlessly).
        let hwnd = unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let c = WndClassEx {
                size: std::mem::size_of::<WndClassEx>() as u32,
                style: 0,
                proc_,
                cls_extra: 0,
                wnd_extra: 0,
                instance,
                icon: 0,
                cursor: LoadCursorW(0, IDC_ARROW as *const u16),
                background: 0,
                menu_name: std::ptr::null(),
                class_name: class.as_ptr(),
                icon_small: 0,
            };
            RegisterClassExW(&c);
            let (ow, oh) = outer(w.max(64), h.max(32));
            CreateWindowExW(
                0,
                class.as_ptr(),
                name.as_ptr(),
                STYLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                ow,
                oh,
                0,
                0,
                instance,
                std::ptr::null_mut(),
            )
        };
        CLOSED.with(|c| c.set(false));
        (hwnd != 0).then_some(EditorWindow { hwnd })
    }

    pub fn handle(&self) -> *mut c_void {
        self.hwnd as *mut c_void
    }

    pub fn show(&self) {
        // SAFETY: the window's own.
        unsafe {
            ShowWindow(self.hwnd, SW_SHOW);
            SetForegroundWindow(self.hwnd);
        }
    }

    /// Whether the user closed it.
    pub fn closed(&self) -> bool {
        CLOSED.with(Cell::get)
    }

    /// A resizer for the plug-in's requests (any thread of this process).
    pub fn resizer(&self) -> impl Fn(i32, i32) + Send + 'static {
        let hwnd = self.hwnd;
        move |w, h| resize(hwnd, w, h)
    }

    /// Puts the window inside BananaV's window `parent` (another
    /// process's, a system's handle): a child of it at its corner, `w` ×
    /// `h` big, shown, its frame gone. The plug-in's own windows stay this
    /// process's (children of this one): it never deals with a window of
    /// another process. Whether it could.
    pub fn embed(&self, parent: isize, w: i32, h: i32) -> bool {
        // SAFETY: this process's window, and a handle the system checks.
        unsafe {
            if IsWindow(parent) == 0 {
                return false;
            }
            SetWindowLongPtrW(
                self.hwnd,
                GWL_STYLE,
                (WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS) as isize,
            );
            if SetParent(self.hwnd, parent) == 0 {
                return false;
            }
            SetWindowPos(
                self.hwnd,
                0,
                0,
                0,
                w.max(1),
                h.max(1),
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            ShowWindow(self.hwnd, SW_SHOWNA);
        }
        true
    }
}

/// Gives window `hwnd` a client area of `w` × `h` (a child's, inside
/// BananaV's window, its whole size).
pub fn resize(hwnd: isize, w: i32, h: i32) {
    // SAFETY: a window of this process.
    let child = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32 & WS_CHILD != 0;
    let (ow, oh) = if child { (w, h) } else { outer(w, h) };
    // SAFETY: a window of this process.
    unsafe {
        SetWindowPos(
            hwnd,
            0,
            0,
            0,
            ow,
            oh,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
}

impl Drop for EditorWindow {
    fn drop(&mut self) {
        // SAFETY: the window's own.
        unsafe { DestroyWindow(self.hwnd) };
    }
}

/// This thread's message queue made (before others post to it); its id.
pub fn queue() -> u32 {
    let mut msg = std::mem::MaybeUninit::<Msg>::zeroed();
    // SAFETY: a peek that removes nothing; the thread's id.
    unsafe {
        PeekMessageW(msg.as_mut_ptr(), 0, 0, 0, 0);
        GetCurrentThreadId()
    }
}

/// Wakes thread `thread` from [`wait`].
pub fn wake(thread: u32) {
    // SAFETY: a plain post.
    unsafe { PostThreadMessageW(thread, WM_APP, 0, 0) };
}

/// Waits for a message (`ms` at most: `u32::MAX` for ever), then runs the
/// windows' messages waiting.
pub fn wait(ms: u32) {
    let mut msg = std::mem::MaybeUninit::<Msg>::zeroed();
    // SAFETY: a wait on this thread's queue, then its messages run.
    unsafe {
        MsgWaitForMultipleObjects(0, std::ptr::null(), 0, ms, QS_ALLINPUT);
        while PeekMessageW(msg.as_mut_ptr(), 0, 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(msg.as_ptr());
            DispatchMessageW(msg.as_ptr());
        }
    }
}
