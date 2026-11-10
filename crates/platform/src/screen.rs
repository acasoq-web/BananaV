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

/// macOS: Core Graphics' window list and its pictures of the screen and
/// of windows (macOS asks the user once to let BananaV record the screen;
/// until then other programs' windows come out as the desktop). The
/// pointer is not drawn in.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod imp {
    use std::ffi::c_void;

    use bv_apple::cf::{self, Owned, Ref};

    use super::{AppWindow, Grab};
    use crate::mac::{CgPoint, CgRect as Rect, CgSize};

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect {
            origin: CgPoint { x, y },
            size: CgSize {
                width: w,
                height: h,
            },
        }
    }

    /// On screen only, without the desktop's own windows.
    const ON_SCREEN: u32 = 1 << 0;
    const EXCLUDE_DESKTOP: u32 = 1 << 4;
    const INCLUDING_WINDOW: u32 = 1 << 3;
    const IGNORE_FRAMING: u32 = 1 << 0;
    const BEST_RESOLUTION: u32 = 1 << 3;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> Ref;
        fn CGWindowListCreateImage(
            bounds: Rect,
            option: u32,
            window: u32,
            image: u32,
        ) -> *mut c_void;
        fn CGRectMakeWithDictionaryRepresentation(dict: Ref, rect: *mut Rect) -> bool;
        fn CGImageGetWidth(image: *mut c_void) -> usize;
        fn CGImageGetHeight(image: *mut c_void) -> usize;
        fn CGImageGetBytesPerRow(image: *mut c_void) -> usize;
        fn CGImageGetBitsPerPixel(image: *mut c_void) -> usize;
        fn CGImageGetBitmapInfo(image: *mut c_void) -> u32;
        fn CGImageGetDataProvider(image: *mut c_void) -> *mut c_void;
        fn CGDataProviderCopyData(provider: *mut c_void) -> Ref;
        fn CGImageRelease(image: *mut c_void);
        fn CGMainDisplayID() -> u32;
        fn CGDisplayPixelsWide(display: u32) -> usize;
        fn CGDisplayBounds(display: u32) -> Rect;
        static kCGWindowNumber: Ref;
        static kCGWindowName: Ref;
        static kCGWindowOwnerName: Ref;
        static kCGWindowOwnerPID: Ref;
        static kCGWindowLayer: Ref;
        static kCGWindowBounds: Ref;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFDataGetBytePtr(data: Ref) -> *const u8;
        fn CFDataGetLength(data: Ref) -> isize;
    }

    /// The main display's scale (pixels a point).
    fn scale() -> f64 {
        // SAFETY: plain queries.
        unsafe {
            let id = CGMainDisplayID();
            let points = CGDisplayBounds(id).size.width.max(1.0);
            (CGDisplayPixelsWide(id) as f64 / points).max(1.0)
        }
    }

    pub fn app_windows() -> Vec<AppWindow> {
        let me = std::process::id() as i64;
        let k = scale();
        // SAFETY: the window list (owned); each entry's values borrowed
        // from it while it lives.
        unsafe {
            let Some(list) =
                Owned::take(CGWindowListCopyWindowInfo(ON_SCREEN | EXCLUDE_DESKTOP, 0))
            else {
                return Vec::new();
            };
            cf::items(list.get())
                .into_iter()
                .filter_map(|w| {
                    // Ordinary windows (layer 0) of other programs.
                    if cf::to_i64(cf::get_by(w, kCGWindowLayer)) != Some(0)
                        || cf::to_i64(cf::get_by(w, kCGWindowOwnerPID)) == Some(me)
                    {
                        return None;
                    }
                    let id = cf::to_i64(cf::get_by(w, kCGWindowNumber))?;
                    let owner =
                        cf::to_string(cf::get_by(w, kCGWindowOwnerName)).unwrap_or_default();
                    let name = cf::to_string(cf::get_by(w, kCGWindowName)).unwrap_or_default();
                    let title = match (owner.is_empty(), name.is_empty()) {
                        (_, true) => owner,
                        (true, false) => name,
                        (false, false) => format!("{name} - {owner}"),
                    };
                    let mut r = Rect::default();
                    if !CGRectMakeWithDictionaryRepresentation(
                        cf::get_by(w, kCGWindowBounds),
                        &mut r,
                    ) || title.is_empty()
                        || r.size.width < 2.0
                        || r.size.height < 2.0
                    {
                        return None;
                    }
                    Some(AppWindow {
                        id: id as isize,
                        title,
                        width: (r.size.width * k).round() as u32,
                        height: (r.size.height * k).round() as u32,
                    })
                })
                .collect()
        }
    }

    /// The pixels of `image` as opaque BGRA, `w` × `h` (cut or filled with
    /// black where the picture is smaller).
    fn pixels(image: *mut c_void, w: usize, h: usize) -> Option<Grab> {
        // SAFETY: a live image; its data copied (owned) and read within its
        // length.
        unsafe {
            let (iw, ih) = (CGImageGetWidth(image), CGImageGetHeight(image));
            let row = CGImageGetBytesPerRow(image);
            if CGImageGetBitsPerPixel(image) != 32 || iw == 0 || ih == 0 {
                return None;
            }
            let data = Owned::take(CGDataProviderCopyData(CGImageGetDataProvider(image)))?;
            let bytes = std::slice::from_raw_parts(
                CFDataGetBytePtr(data.get()),
                CFDataGetLength(data.get()).max(0) as usize,
            );
            // The screen's pictures are BGRA (32-bit little-endian, alpha
            // first): as they are; others, RGBA, swapped.
            let little = CGImageGetBitmapInfo(image) & 0x7000 == 0x2000;
            let mut bgra = vec![0u8; w * h * 4];
            for y in 0..h.min(ih) {
                let src = bytes.get(y * row..y * row + iw.min(w) * 4)?;
                let dst = &mut bgra[y * w * 4..y * w * 4 + iw.min(w) * 4];
                for (d, s) in dst
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(src.as_chunks::<4>().0)
                {
                    if little {
                        d.copy_from_slice(&[s[0], s[1], s[2], 255]);
                    } else {
                        d.copy_from_slice(&[s[2], s[1], s[0], 255]);
                    }
                }
            }
            for px in bgra.as_chunks_mut::<4>().0 {
                px[3] = 255;
            }
            Some(Grab {
                width: w as u32,
                height: h as u32,
                bgra,
            })
        }
    }

    pub fn grab_area(x: i32, y: i32, w: u32, h: u32) -> Option<Grab> {
        // The display's own scale (each one's pixels are its points times
        // it: `crate::displays`): the display the area begins at, else one
        // holding its corner.
        let all = crate::displays();
        let k = all
            .iter()
            .find(|d| d.x == x && d.y == y)
            .or_else(|| {
                all.iter().find(|d| {
                    x >= d.x && y >= d.y && x < d.x + d.width as i32 && y < d.y + d.height as i32
                })
            })
            .map_or_else(scale, |d| f64::from(d.scale));
        let bounds = rect(
            f64::from(x) / k,
            f64::from(y) / k,
            f64::from(w) / k,
            f64::from(h) / k,
        );
        // SAFETY: a picture of the screen (owned, released here).
        unsafe {
            let image = CGWindowListCreateImage(bounds, ON_SCREEN, 0, BEST_RESOLUTION);
            if image.is_null() {
                return None;
            }
            let g = pixels(image, w as usize, h as usize);
            CGImageRelease(image);
            g
        }
    }

    pub fn grab_window(id: isize) -> Option<Grab> {
        let id = u32::try_from(id).ok()?;
        // A null rectangle: the window's own bounds.
        let bounds = rect(f64::INFINITY, f64::INFINITY, 0.0, 0.0);
        // SAFETY: a picture of one window (owned, released here).
        unsafe {
            let image = CGWindowListCreateImage(
                bounds,
                INCLUDING_WINDOW,
                id,
                IGNORE_FRAMING | BEST_RESOLUTION,
            );
            if image.is_null() {
                return None;
            }
            let (w, h) = (CGImageGetWidth(image), CGImageGetHeight(image));
            let g = (w > 0 && h > 0).then(|| pixels(image, w, h)).flatten();
            CGImageRelease(image);
            g
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
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
