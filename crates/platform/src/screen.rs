//! Screen recording's pictures (the Screen Capture window): the
//! applications' windows on the desktop, and pictures of a part of the
//! desktop (a display) or of one window, taken as they are now, the mouse
//! pointer drawn in.

/// An application's window that can be recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppWindow {
    /// The OS's handle of it.
    pub id: isize,
    pub title: String,
    /// Its size now, in pixels.
    pub width: u32,
    pub height: u32,
}

/// A picture of the screen: `width` × `height` pixels, 8-bit BGRA (opaque),
/// the top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grab {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// The applications' windows shown on the desktop now (titled, neither
/// minimized nor tool windows), the frontmost first.
pub fn app_windows() -> Vec<AppWindow> {
    imp::app_windows()
}

/// A picture of the desktop's `width` × `height` pixels from (`x`, `y`)
/// (a display's area), the pointer drawn in.
pub fn grab_area(x: i32, y: i32, width: u32, height: u32) -> Option<Grab> {
    if width == 0 || height == 0 {
        return None;
    }
    imp::grab_area(x, y, width, height)
}

/// A picture of the window `id` as it is drawn now, even where other
/// windows cover it (`None` when it is gone or minimized).
pub fn grab_window(id: isize) -> Option<Grab> {
    imp::grab_window(id)
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod imp {
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleDC,
        CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HDC, HGDIOBJ, ReleaseDC,
        SRCCOPY, SelectObject,
    };
    use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
    use windows::Win32::UI::WindowsAndMessaging::{
        CURSOR_SHOWING, CURSORINFO, DI_NORMAL, DrawIconEx, EnumWindows, GW_OWNER, GWL_EXSTYLE,
        GetCursorInfo, GetIconInfo, GetWindow, GetWindowLongW, GetWindowRect, GetWindowTextLengthW,
        GetWindowTextW, HICON, ICONINFO, IsIconic, IsWindow, IsWindowVisible, WS_EX_TOOLWINDOW,
    };
    use windows::core::BOOL;

    use super::{AppWindow, Grab};

    /// PrintWindow's PW_RENDERFULLCONTENT: what DirectComposition draws too.
    const FULL_CONTENT: u32 = 2;

    pub fn app_windows() -> Vec<AppWindow> {
        let mut handles: Vec<HWND> = Vec::new();
        // SAFETY: `push` only pushes into `handles` through the pointer,
        // and `handles` outlives the call.
        let _ = unsafe { EnumWindows(Some(push), LPARAM(&mut handles as *mut Vec<HWND> as isize)) };
        handles.into_iter().filter_map(describe).collect()
    }

    unsafe extern "system" fn push(hwnd: HWND, list: LPARAM) -> BOOL {
        // SAFETY: `list` is the `Vec` `app_windows` passed.
        let list = unsafe { &mut *(list.0 as *mut Vec<HWND>) };
        list.push(hwnd);
        BOOL(1)
    }

    /// The window as one to record, when it is one.
    fn describe(hwnd: HWND) -> Option<AppWindow> {
        // SAFETY: plain queries of a window handle the OS gave.
        unsafe {
            if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
                return None;
            }
            if GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid()) {
                return None;
            }
            if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
                return None;
            }
            let len = GetWindowTextLengthW(hwnd);
            if len <= 0 {
                return None;
            }
            let mut buf = vec![0u16; len as usize + 1];
            let n = GetWindowTextW(hwnd, &mut buf);
            let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
            let (width, height) = size(hwnd)?;
            Some(AppWindow {
                id: hwnd.0 as isize,
                title,
                width,
                height,
            })
        }
    }

    fn size(hwnd: HWND) -> Option<(u32, u32)> {
        let mut r = RECT::default();
        // SAFETY: a query of a window handle into a local.
        unsafe { GetWindowRect(hwnd, &mut r) }.ok()?;
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        (w > 0 && h > 0).then_some((w as u32, h as u32))
    }

    /// A 32-bit top-down DIB section of `w` × `h` selected into a memory
    /// device context compatible with `screen`; `draw` paints it, then its
    /// pixels are copied out (opaque) and everything is let go.
    fn picture(screen: HDC, w: u32, h: u32, draw: impl FnOnce(HDC) -> bool) -> Option<Grab> {
        // SAFETY: the device contexts and the bitmap are created, used and
        // deleted here; `bits` points into the bitmap while it is alive, and
        // only `w` × `h` × 4 bytes of it are read.
        unsafe {
            let mem = CreateCompatibleDC(Some(screen));
            if mem.is_invalid() {
                return None;
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let Ok(dib) = CreateDIBSection(Some(mem), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            else {
                let _ = DeleteDC(mem);
                return None;
            };
            let old = SelectObject(mem, HGDIOBJ(dib.0));
            let drawn = draw(mem);
            let grab = (drawn && !bits.is_null()).then(|| {
                let n = w as usize * h as usize * 4;
                let mut bgra = std::slice::from_raw_parts(bits as *const u8, n).to_vec();
                for px in bgra.as_chunks_mut::<4>().0 {
                    px[3] = 255;
                }
                Grab {
                    width: w,
                    height: h,
                    bgra,
                }
            });
            SelectObject(mem, old);
            let _ = DeleteObject(HGDIOBJ(dib.0));
            let _ = DeleteDC(mem);
            grab
        }
    }

    /// The mouse pointer drawn into `dc`, whose top left corner is the
    /// desktop's (`x`, `y`).
    fn draw_pointer(dc: HDC, x: i32, y: i32) {
        // SAFETY: queries of the pointer into locals; the icon's bitmaps
        // the query hands over are deleted.
        unsafe {
            let mut ci = CURSORINFO {
                cbSize: std::mem::size_of::<CURSORINFO>() as u32,
                ..Default::default()
            };
            if GetCursorInfo(&mut ci).is_err() || ci.flags != CURSOR_SHOWING {
                return;
            }
            let icon = HICON(ci.hCursor.0);
            let mut ii = ICONINFO::default();
            if GetIconInfo(icon, &mut ii).is_err() {
                return;
            }
            let _ = DrawIconEx(
                dc,
                ci.ptScreenPos.x - x - ii.xHotspot as i32,
                ci.ptScreenPos.y - y - ii.yHotspot as i32,
                icon,
                0,
                0,
                0,
                None,
                DI_NORMAL,
            );
            for b in [ii.hbmMask, ii.hbmColor] {
                if !b.is_invalid() {
                    let _ = DeleteObject(HGDIOBJ(b.0));
                }
            }
        }
    }

    pub fn grab_area(x: i32, y: i32, w: u32, h: u32) -> Option<Grab> {
        // SAFETY: the screen's device context is taken and given back here.
        unsafe {
            let screen = GetDC(None);
            if screen.is_invalid() {
                return None;
            }
            let grab = picture(screen, w, h, |mem| {
                let ok = BitBlt(
                    mem,
                    0,
                    0,
                    w as i32,
                    h as i32,
                    Some(screen),
                    x,
                    y,
                    SRCCOPY | CAPTUREBLT,
                )
                .is_ok();
                if ok {
                    draw_pointer(mem, x, y);
                }
                ok
            });
            ReleaseDC(None, screen);
            grab
        }
    }

    pub fn grab_window(id: isize) -> Option<Grab> {
        let hwnd = HWND(id as *mut core::ffi::c_void);
        // SAFETY: the window handle is checked first; the screen's device
        // context is taken and given back here.
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() || IsIconic(hwnd).as_bool() {
                return None;
            }
            let mut r = RECT::default();
            GetWindowRect(hwnd, &mut r).ok()?;
            let (w, h) = (
                (r.right - r.left).max(0) as u32,
                (r.bottom - r.top).max(0) as u32,
            );
            if w == 0 || h == 0 {
                return None;
            }
            let screen = GetDC(None);
            if screen.is_invalid() {
                return None;
            }
            let grab = picture(screen, w, h, |mem| {
                let ok = PrintWindow(hwnd, mem, PRINT_WINDOW_FLAGS(FULL_CONTENT)).as_bool();
                if ok {
                    draw_pointer(mem, r.left, r.top);
                }
                ok
            });
            ReleaseDC(None, screen);
            grab
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{AppWindow, Grab};

    pub fn app_windows() -> Vec<AppWindow> {
        Vec::new()
    }

    pub fn grab_area(_x: i32, _y: i32, _w: u32, _h: u32) -> Option<Grab> {
        None
    }

    pub fn grab_window(_id: isize) -> Option<Grab> {
        None
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn a_display_s_area_comes_out_opaque_and_of_its_size() {
        let Some(d) = crate::displays().into_iter().next() else {
            return;
        };
        let w = d.width.min(64);
        let h = d.height.min(48);
        // A session without a desktop (a service) has nothing to take.
        let Some(g) = grab_area(d.x, d.y, w, h) else {
            return;
        };
        assert_eq!((g.width, g.height), (w, h));
        assert_eq!(g.bgra.len(), (w * h * 4) as usize);
        assert!(g.bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }
}
