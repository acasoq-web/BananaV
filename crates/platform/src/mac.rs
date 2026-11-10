//! macOS: the services of `lib.rs` through AppKit (the program's windows,
//! the mouse, the displays' names, the trash, the color sampler, Quit and
//! the files Finder opens), Core Graphics (the displays), the keyboard
//! layouts (Text Input Sources), LaunchServices (which program opens
//! project files), Disk Arbitration (removable drives), GameController
//! (game pads) and the kernel (memory, disks, the clock).
//!
//! AppKit is touched on the main thread only (the interface's): elsewhere
//! each answer is what can be told without it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use block2::RcBlock;
use bv_apple::cf::{self, Owned, Ref};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationWillFinishLaunchingNotification, NSColor, NSColorSampler,
    NSColorSpace, NSEvent, NSEventMask, NSEventType, NSScreen, NSWindow, NSWindowOrderingMode,
};
use objc2_foundation::{NSArray, NSFileManager, NSNotification, NSNotificationCenter, NSURL};

// ---------------------------------------------------------------------------
// The program's windows.

/// This program's windows (on the main thread).
fn windows(mtm: MainThreadMarker) -> Vec<Retained<NSWindow>> {
    NSApplication::sharedApplication(mtm).windows().to_vec()
}

fn title_of(w: &NSWindow) -> String {
    w.title().to_string()
}

/// This program's window titled `title`: its `NSWindow`'s address.
pub fn find_window(title: &str) -> Option<isize> {
    let mtm = MainThreadMarker::new()?;
    windows(mtm)
        .into_iter()
        .find(|w| title_of(w) == title)
        .map(|w| Retained::as_ptr(&w) as isize)
}

/// Makes the windows titled `child` children of the one titled `owner`:
/// they stay in front of it and go with it (AppKit's child windows).
pub fn set_owner(child: &str, owner: &str) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let all = windows(mtm);
    let Some(main) = all.iter().find(|w| title_of(w) == owner) else {
        return false;
    };
    let mut exists = false;
    for w in &all {
        if title_of(w) != child || Retained::as_ptr(w) == Retained::as_ptr(main) {
            continue;
        }
        exists = true;
        let owned = w
            .parentWindow()
            .is_some_and(|p| Retained::as_ptr(&p) == Retained::as_ptr(main));
        if !owned {
            // SAFETY: two live windows of this program, neither a child of
            // the other.
            unsafe { main.addChildWindow_ordered(w, NSWindowOrderingMode::Above) };
        }
    }
    exists
}

/// Windows only (the program covers its windows' contents itself).
pub fn block_input(_keep: &[&str]) -> Vec<isize> {
    Vec::new()
}

pub fn unblock_input(_windows: &[isize]) {}

/// The scale of the program's main window (its screen's, 2 on a Retina
/// display): physical pixels per point.
fn backing_scale(mtm: MainThreadMarker) -> f64 {
    let app = NSApplication::sharedApplication(mtm);
    if let Some(w) = app.mainWindow().or_else(|| app.keyWindow()) {
        return w.backingScaleFactor();
    }
    NSScreen::mainScreen(mtm).map_or(1.0, |s| s.backingScaleFactor())
}

/// The height of the first screen (the one with the menu bar), which
/// AppKit's coordinates count up from.
fn primary_height(mtm: MainThreadMarker) -> f64 {
    NSScreen::screens(mtm)
        .firstObject()
        .map_or(0.0, |s| s.frame().size.height)
}

/// The mouse on the screen, in pixels from the first screen's top left
/// (the window library's coordinates, times the main window's scale).
pub fn cursor() -> Option<(i32, i32)> {
    let mtm = MainThreadMarker::new()?;
    let p = NSEvent::mouseLocation();
    let scale = backing_scale(mtm);
    let top = primary_height(mtm);
    Some((
        (p.x * scale).round() as i32,
        ((top - p.y) * scale).round() as i32,
    ))
}

/// The screen's pixels are read by the system's color sampler instead
/// ([`start_color_sampler`]): no recording of the screen asked for.
pub fn screen_color(_x: i32, _y: i32) -> Option<[u8; 3]> {
    None
}

pub fn main_button_down() -> bool {
    NSEvent::pressedMouseButtons() & 1 != 0
}

/// The user's double-click interval (System Settings › Mouse), seconds.
pub fn double_click_time() -> f64 {
    NSEvent::doubleClickInterval()
}

/// Clicks elsewhere go to the system's color sampler, which takes them.
pub fn capture_mouse(_title: &str) -> bool {
    true
}

pub fn release_mouse() {}

// ---------------------------------------------------------------------------
// The keyboard.

/// The keypad's keys held down, by the Mac's key codes (from a watch on
/// this program's key events: no permission needed).
static KEYPAD: Mutex<[bool; 128]> = Mutex::new([false; 128]);

/// The Mac's key code of the keypad's `c`.
fn keypad_code(c: char) -> Option<u16> {
    Some(match c {
        '0' => 0x52,
        '1' => 0x53,
        '2' => 0x54,
        '3' => 0x55,
        '4' => 0x56,
        '5' => 0x57,
        '6' => 0x58,
        '7' => 0x59,
        '8' => 0x5B,
        '9' => 0x5C,
        '.' => 0x41,
        '*' => 0x43,
        '+' => 0x45,
        '/' => 0x4B,
        '-' => 0x4E,
        '=' => 0x51,
        _ => return None,
    })
}

const KEYPAD_CODES: [u16; 16] = [
    0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5B, 0x5C, 0x41, 0x43, 0x45, 0x4B, 0x4E, 0x51,
];

/// Watches this program's key events for the keypad's keys (once, on the
/// main thread).
fn watch_keys(mtm: MainThreadMarker) {
    thread_local! {
        static WATCHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    let _ = mtm;
    if WATCHING.get() {
        return;
    }
    WATCHING.set(true);
    let block = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: the event AppKit hands the watch, alive during the call.
        let e = unsafe { event.as_ref() };
        let code = e.keyCode();
        if KEYPAD_CODES.contains(&code) {
            let down = e.r#type() == NSEventType::KeyDown;
            if let Ok(mut k) = KEYPAD.lock() {
                k[code as usize & 127] = down;
            }
        }
        event.as_ptr()
    });
    // SAFETY: the block gives back the event it was given (a valid one).
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::KeyDown | NSEventMask::KeyUp,
            &block,
        )
    };
    // Watched for as long as the program runs.
    std::mem::forget(monitor);
}

pub fn numpad_down(c: char) -> bool {
    if let Some(mtm) = MainThreadMarker::new() {
        watch_keys(mtm);
    }
    let Some(code) = keypad_code(c) else {
        return false;
    };
    KEYPAD.lock().is_ok_and(|k| k[code as usize])
}

/// The Mac's key code of a key by its scan code (the PC keyboard's, set
/// 1, as BananaV names keys by place): the main block's keys.
fn mac_key(scancode: u32) -> Option<u16> {
    Some(match scancode {
        0x02 => 0x12,
        0x03 => 0x13,
        0x04 => 0x14,
        0x05 => 0x15,
        0x06 => 0x17,
        0x07 => 0x16,
        0x08 => 0x1A,
        0x09 => 0x1C,
        0x0A => 0x19,
        0x0B => 0x1D,
        0x0C => 0x1B,
        0x0D => 0x18,
        0x10 => 0x0C,
        0x11 => 0x0D,
        0x12 => 0x0E,
        0x13 => 0x0F,
        0x14 => 0x11,
        0x15 => 0x10,
        0x16 => 0x20,
        0x17 => 0x22,
        0x18 => 0x1F,
        0x19 => 0x23,
        0x1A => 0x21,
        0x1B => 0x1E,
        0x1E => 0x00,
        0x1F => 0x01,
        0x20 => 0x02,
        0x21 => 0x03,
        0x22 => 0x05,
        0x23 => 0x04,
        0x24 => 0x26,
        0x25 => 0x28,
        0x26 => 0x25,
        0x27 => 0x29,
        0x28 => 0x27,
        0x29 => 0x32,
        0x2B => 0x2A,
        0x2C => 0x06,
        0x2D => 0x07,
        0x2E => 0x08,
        0x2F => 0x09,
        0x30 => 0x0B,
        0x31 => 0x2D,
        0x32 => 0x2E,
        0x33 => 0x2B,
        0x34 => 0x2F,
        0x35 => 0x2C,
        0x56 => 0x0A,
        _ => return None,
    })
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn TISCopyCurrentKeyboardLayoutInputSource() -> Ref;
    fn TISGetInputSourceProperty(source: Ref, key: Ref) -> Ref;
    static kTISPropertyUnicodeKeyLayoutData: Ref;
    fn UCKeyTranslate(
        layout: *const c_void,
        code: u16,
        action: u16,
        modifiers: u32,
        keyboard: u32,
        options: u32,
        dead: *mut u32,
        max: usize,
        len: *mut usize,
        chars: *mut u16,
    ) -> i32;
    fn LMGetKbdType() -> u8;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFDataGetBytePtr(data: Ref) -> *const u8;
}

/// The character the key types in the current layout, unshifted (a dead
/// key gives its accent). On the main thread (the layouts' service wants
/// it); the labels are kept a second.
pub fn key_char(scancode: u32) -> Option<char> {
    type Labels = (Option<Instant>, HashMap<u32, Option<char>>);
    thread_local! {
        static LABELS: RefCell<Labels> = RefCell::new((None, HashMap::new()));
    }
    let code = mac_key(scancode)?;
    if !bv_apple::is_main_thread() {
        return None;
    }
    LABELS.with_borrow_mut(|(at, labels)| {
        if at.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
            labels.clear();
            *at = Some(Instant::now());
        }
        *labels.entry(scancode).or_insert_with(|| translate(code))
    })
}

fn translate(code: u16) -> Option<char> {
    // SAFETY: the current layout (owned), its key layout data (borrowed
    // from it while it lives), and a translation into a buffer of the
    // length given.
    unsafe {
        let source = Owned::take(TISCopyCurrentKeyboardLayoutInputSource())?;
        let data = TISGetInputSourceProperty(source.get(), kTISPropertyUnicodeKeyLayoutData);
        if data.is_null() {
            return None;
        }
        let layout = CFDataGetBytePtr(data);
        let mut dead = 0u32;
        let mut chars = [0u16; 8];
        let mut len = 0usize;
        // A key going down, no modifiers, dead keys giving their accent.
        let r = UCKeyTranslate(
            layout.cast(),
            code,
            0,
            0,
            u32::from(LMGetKbdType()),
            1,
            &mut dead,
            chars.len(),
            &mut len,
            chars.as_mut_ptr(),
        );
        if r != 0 || len == 0 {
            return None;
        }
        char::decode_utf16(chars[..len.min(chars.len())].iter().copied())
            .next()?
            .ok()
            .filter(|c| !c.is_control())
    }
}

// ---------------------------------------------------------------------------
// Memory, disks and the clock.

fn sysctl_u64(name: &CStr) -> Option<u64> {
    let mut value = 0u64;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: a read of a number the system keeps into one of its size.
    let r = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&raw mut value).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (r == 0).then_some(value)
}

pub fn total_memory() -> Option<u64> {
    sysctl_u64(c"hw.memsize")
}

/// The file system holding `path`.
fn statfs(path: &Path) -> Option<libc::statfs> {
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: a path ended by a 0; the structure all of whose bits may be
    // zero, filled by the call.
    unsafe {
        let mut s: libc::statfs = std::mem::zeroed();
        (libc::statfs(c.as_ptr(), &mut s) == 0).then_some(s)
    }
}

pub fn free_space(dir: &Path) -> Option<u64> {
    let s = statfs(dir)?;
    Some(s.f_bavail * u64::from(s.f_bsize))
}

fn local_tm() -> Option<libc::tm> {
    // SAFETY: the time now, converted into a structure of the caller's.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        (!libc::localtime_r(&now, &mut tm).is_null()).then_some(tm)
    }
}

pub fn local_time() -> Option<(u32, u32, u32)> {
    let tm = local_tm()?;
    Some((tm.tm_hour as u32, tm.tm_min as u32, tm.tm_sec as u32))
}

pub fn utc_offset() -> i64 {
    local_tm().map_or(0, |tm| tm.tm_gmtoff)
}

/// Windows' protection of the user's folders: macOS asks the user itself
/// the first time a folder is written.
pub fn controlled_folder_access() -> bool {
    false
}

/// To the Trash, as Finder moves it (it can be put back).
pub fn to_recycle_bin(path: &Path) -> std::io::Result<()> {
    let url = NSURL::from_file_path(path).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a file's path")
    })?;
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| std::io::Error::other(e.localizedDescription().to_string()))
}

// ---------------------------------------------------------------------------
// Removable drives (Disk Arbitration).

#[link(name = "DiskArbitration", kind = "framework")]
unsafe extern "C" {
    fn DASessionCreate(alloc: Ref) -> Ref;
    fn DADiskCreateFromBSDName(alloc: Ref, session: Ref, name: *const c_char) -> Ref;
    fn DADiskCopyDescription(disk: Ref) -> Ref;
    static kDADiskDescriptionMediaRemovableKey: Ref;
    static kDADiskDescriptionMediaEjectableKey: Ref;
    static kDADiskDescriptionDeviceInternalKey: Ref;
    static kDADiskDescriptionVolumeNameKey: Ref;
    static kDADiskDescriptionMediaKindKey: Ref;
}

/// What Disk Arbitration tells of the disk mounted from `device`
/// (`/dev/disk4s1`): removable or ejectable or external, its volume's name,
/// and whether it is a disc.
fn disk_of(session: &Owned, device: &str) -> Option<(bool, Option<String>, bool)> {
    let name = CString::new(device.trim_start_matches("/dev/")).ok()?;
    // SAFETY: a live session and a name ended by a 0; the disk and its
    // description are owned; the values are borrowed from the description.
    unsafe {
        let disk = Owned::take(DADiskCreateFromBSDName(
            std::ptr::null(),
            session.get(),
            name.as_ptr(),
        ))?;
        let d = Owned::take(DADiskCopyDescription(disk.get()))?;
        let flag = |key: Ref| cf::to_bool(cf::get_by(d.get(), key)).unwrap_or(false);
        let removable = flag(kDADiskDescriptionMediaRemovableKey)
            || flag(kDADiskDescriptionMediaEjectableKey)
            || cf::to_bool(cf::get_by(d.get(), kDADiskDescriptionDeviceInternalKey)) == Some(false);
        let label = cf::to_string(cf::get_by(d.get(), kDADiskDescriptionVolumeNameKey));
        let kind =
            cf::to_string(cf::get_by(d.get(), kDADiskDescriptionMediaKindKey)).unwrap_or_default();
        let disc = ["IOCDMedia", "IODVDMedia", "IOBDMedia"].contains(&kind.as_str());
        Some((removable, label, disc))
    }
}

/// The file systems mounted now: (mount point, device, type, flags).
fn mounts() -> Vec<(PathBuf, String, String, u32)> {
    let text = |chars: &[c_char]| {
        // SAFETY: the system ends these names with a 0 within the array.
        unsafe { CStr::from_ptr(chars.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    };
    // SAFETY: the system's own array of the mounts (valid until the next
    // call on this thread), `n` of them.
    unsafe {
        let mut list: *mut libc::statfs = std::ptr::null_mut();
        let n = libc::getmntinfo(&mut list, libc::MNT_NOWAIT);
        if n <= 0 || list.is_null() {
            return Vec::new();
        }
        std::slice::from_raw_parts(list, n as usize)
            .iter()
            .map(|m| {
                (
                    PathBuf::from(text(&m.f_mntonname)),
                    text(&m.f_mntfromname),
                    text(&m.f_fstypename),
                    m.f_flags,
                )
            })
            .collect()
    }
}

/// The volumes under `/Volumes` on disks that come and go (cards, sticks,
/// discs, outside drives).
pub fn removable_drives() -> Vec<super::Drive> {
    const DONT_BROWSE: u32 = 0x0010_0000;
    // SAFETY: a new session, owned.
    let Some(session) = (unsafe { Owned::take(DASessionCreate(std::ptr::null())) }) else {
        return Vec::new();
    };
    mounts()
        .into_iter()
        .filter(|(at, from, _, flags)| {
            at.starts_with("/Volumes") && from.starts_with("/dev/disk") && flags & DONT_BROWSE == 0
        })
        .filter_map(|(at, from, fs, _)| {
            let (removable, label, disc) = disk_of(&session, &from)?;
            let optical = disc || ["cd9660", "udf", "cddafs"].contains(&fs.as_str());
            (removable || optical).then(|| super::Drive {
                label: label.unwrap_or_else(|| {
                    at.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                }),
                root: at,
                optical,
            })
        })
        .collect()
}

pub fn is_removable(path: &Path) -> bool {
    let Some(s) = statfs(path) else {
        return false;
    };
    // SAFETY: the system ends the name with a 0 within the array.
    let at = unsafe { CStr::from_ptr(s.f_mntonname.as_ptr()) }.to_string_lossy();
    removable_drives()
        .iter()
        .any(|d| d.root == Path::new(at.as_ref()))
}

// ---------------------------------------------------------------------------
// The program, the displays, game pads.

pub fn app_is_active() -> Option<bool> {
    let mtm = MainThreadMarker::new()?;
    Some(NSApplication::sharedApplication(mtm).isActive())
}

#[link(name = "GameController", kind = "framework")]
unsafe extern "C" {}

/// The first game pad's left stick (Apple's GameController: the pads the
/// Mac knows, Xbox's, PlayStation's and others').
pub fn joystick() -> Option<(f32, f32)> {
    MainThreadMarker::new()?;
    let class = AnyClass::get(c"GCController")?;
    // SAFETY: GameController's documented messages, each answer of the
    // type it declares (the pad's sticks and their axes' values).
    unsafe {
        let pads: Retained<NSArray<AnyObject>> = msg_send![class, controllers];
        for pad in pads.iter() {
            let extended: Option<Retained<AnyObject>> = msg_send![&*pad, extendedGamepad];
            let Some(extended) = extended else {
                continue;
            };
            let stick: Retained<AnyObject> = msg_send![&*extended, leftThumbstick];
            let x_axis: Retained<AnyObject> = msg_send![&*stick, xAxis];
            let y_axis: Retained<AnyObject> = msg_send![&*stick, yAxis];
            let x: f32 = msg_send![&*x_axis, value];
            let y: f32 = msg_send![&*y_axis, value];
            return Some((x.clamp(-1.0, 1.0), y.clamp(-1.0, 1.0)));
        }
    }
    None
}

/// Core Graphics' point, size and rectangle (`CGPoint`…), in points.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CgPoint {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CgSize {
    pub(crate) width: f64,
    pub(crate) height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CgRect {
    pub(crate) origin: CgPoint,
    pub(crate) size: CgSize,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayBounds(display: u32) -> CgRect;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayCopyDisplayMode(display: u32) -> *mut c_void;
    fn CGDisplayModeGetPixelWidth(mode: *mut c_void) -> usize;
    fn CGDisplayModeGetWidth(mode: *mut c_void) -> usize;
    fn CGDisplayModeGetRefreshRate(mode: *mut c_void) -> f64;
    fn CGDisplayModeRelease(mode: *mut c_void);
}

/// The displays' names AppKit gives ("Built-in Retina Display"), by Core
/// Graphics' number, read on the main thread.
static DISPLAY_NAMES: Mutex<Vec<(u32, String)>> = Mutex::new(Vec::new());

fn read_display_names(mtm: MainThreadMarker) {
    let mut names = Vec::new();
    for screen in NSScreen::screens(mtm).iter() {
        let number = screen
            .deviceDescription()
            .objectForKey(&objc2_foundation::NSString::from_str("NSScreenNumber"))
            .and_then(|n| n.downcast::<objc2_foundation::NSNumber>().ok())
            .map(|n| n.unsignedIntValue());
        if let Some(number) = number {
            names.push((number, screen.localizedName().to_string()));
        }
    }
    if let Ok(mut all) = DISPLAY_NAMES.lock() {
        *all = names;
    }
}

/// Core Graphics' displays, the main one first: their places and sizes in
/// pixels (points times their scale, from the first display's top left,
/// as the window library places windows), named "Built-in Retina Display
/// 1", "DELL U2720Q 2"… (the number is each one's in the list).
pub fn displays() -> Vec<super::Display> {
    if let Some(mtm) = MainThreadMarker::new() {
        read_display_names(mtm);
    }
    let mut ids = [0u32; 16];
    let mut n = 0u32;
    // SAFETY: an array of the length given, the count written back.
    if unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut n) } != 0 {
        return Vec::new();
    }
    // SAFETY: a plain query.
    let main = unsafe { CGMainDisplayID() };
    let names = DISPLAY_NAMES.lock().map(|n| n.clone()).unwrap_or_default();
    let mut list: Vec<u32> = ids[..(n as usize).min(ids.len())].to_vec();
    list.sort_by_key(|&id| id != main);
    list.iter()
        .enumerate()
        .map(|(i, &id)| {
            // SAFETY: a display of the list: its bounds, and its mode
            // (owned, released here).
            let (bounds, scale, hz) = unsafe {
                let bounds = CGDisplayBounds(id);
                let mode = CGDisplayCopyDisplayMode(id);
                let (scale, hz) = if mode.is_null() {
                    (1.0, 0.0)
                } else {
                    let pixels = CGDisplayModeGetPixelWidth(mode) as f64;
                    let points = CGDisplayModeGetWidth(mode).max(1) as f64;
                    let hz = CGDisplayModeGetRefreshRate(mode);
                    CGDisplayModeRelease(mode);
                    ((pixels / points).max(1.0), hz)
                };
                (bounds, scale, hz)
            };
            let name = names
                .iter()
                .find(|(n, _)| *n == id)
                .map_or("Display", |(_, s)| s.as_str());
            super::Display {
                name: format!("{name} {}", i + 1),
                x: (bounds.origin.x * scale).round() as i32,
                y: (bounds.origin.y * scale).round() as i32,
                width: (bounds.size.width * scale).round() as u32,
                height: (bounds.size.height * scale).round() as u32,
                // Built-in displays tell none (0: unknown).
                hz: hz.max(0.0).round() as u32,
                primary: id == main,
                scale: scale as f32,
            }
        })
        .collect()
}

/// No ASIO on a Mac: Core Audio's own devices are low-latency.
pub fn asio_drivers() -> Vec<String> {
    Vec::new()
}

// ---------------------------------------------------------------------------
// Project files (LaunchServices).

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn UTTypeCreatePreferredIdentifierForTag(tag_class: Ref, tag: Ref, conforming: Ref) -> Ref;
    static kUTTagClassFilenameExtension: Ref;
    fn LSCopyDefaultRoleHandlerForContentType(content_type: Ref, role: u32) -> Ref;
    fn LSSetDefaultRoleHandlerForContentType(content_type: Ref, role: u32, bundle: Ref) -> i32;
}

const ROLES_ALL: u32 = 0xFFFF_FFFF;

/// The type the system gives files ending in `.ext` ("org.bananav.project"
/// once BananaV.app is known to it).
fn content_type(ext: &str) -> Option<Owned> {
    let tag = cf::string(ext);
    // SAFETY: the system's constant and a live string; the type is owned.
    unsafe {
        Owned::take(UTTypeCreatePreferredIdentifierForTag(
            kUTTagClassFilenameExtension,
            tag.get(),
            std::ptr::null(),
        ))
    }
}

/// BananaV's bundle identifier ("org.bananav.BananaV"), when it runs from
/// its application bundle.
fn own_identifier() -> Option<String> {
    bv_apple::Bundle::main()?.identifier()
}

pub fn opens_project_files(ext: &str) -> Option<bool> {
    let me = own_identifier()?;
    let kind = content_type(ext)?;
    // SAFETY: a live type; the handler's identifier is owned.
    let handler = unsafe {
        Owned::take(LSCopyDefaultRoleHandlerForContentType(
            kind.get(),
            ROLES_ALL,
        ))
    };
    // SAFETY: a live string.
    let handler = handler.and_then(|h| unsafe { cf::to_string(h.get()) });
    Some(handler.is_some_and(|h| h.eq_ignore_ascii_case(&me)))
}

pub fn associate_project_files(
    ext: &str,
    _prog_id: &str,
    _description: &str,
) -> Result<(), String> {
    let me = own_identifier()
        .ok_or_else(|| "BananaV runs outside its application bundle".to_string())?;
    let kind = content_type(ext).ok_or_else(|| "no type for the files".to_string())?;
    let id = cf::string(&me);
    // SAFETY: a live type and a live identifier.
    let r = unsafe { LSSetDefaultRoleHandlerForContentType(kind.get(), ROLES_ALL, id.get()) };
    if r == 0 {
        Ok(())
    } else {
        Err(format!("LaunchServices refused ({r})"))
    }
}

// ---------------------------------------------------------------------------
// The color sampler.

/// What the sampler gave: `None` while it shows, `Some(None)` when the
/// user cancelled it.
static SAMPLED: Mutex<Option<Option<[u8; 3]>>> = Mutex::new(None);

thread_local! {
    /// The sampler shown (kept until it answers).
    static SAMPLER: RefCell<Option<Retained<NSColorSampler>>> = const { RefCell::new(None) };
}

fn to_byte(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Shows the system's color sampler (its magnifier follows the mouse over
/// every screen; a click takes the color, Esc cancels). Whether it shows.
pub fn start_color_sampler() -> bool {
    if MainThreadMarker::new().is_none() {
        return false;
    }
    if let Ok(mut s) = SAMPLED.lock() {
        *s = None;
    }
    let sampler = NSColorSampler::new();
    let block = RcBlock::new(|color: *mut NSColor| {
        // SAFETY: the color the sampler hands over (or null), alive during
        // the call.
        let picked = unsafe { color.as_ref() }.and_then(|c| {
            let srgb = c.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
            Some([
                to_byte(srgb.redComponent()),
                to_byte(srgb.greenComponent()),
                to_byte(srgb.blueComponent()),
            ])
        });
        if let Ok(mut s) = SAMPLED.lock() {
            *s = Some(picked);
        }
        SAMPLER.with_borrow_mut(|s| *s = None);
    });
    // SAFETY: a sampler and a block of the type it calls.
    unsafe { sampler.showSamplerWithSelectionHandler(&block) };
    SAMPLER.with_borrow_mut(|s| *s = Some(sampler));
    true
}

pub fn take_sampled_color() -> Option<Option<[u8; 3]>> {
    SAMPLED.lock().ok()?.take()
}

// ---------------------------------------------------------------------------
// Quit and the files Finder opens.

/// Quit (the menu's, the Dock's, the system's at log out) asked for: the
/// interface closes as its own Exit does, asking about unsaved changes.
static QUIT_ASKED: AtomicBool = AtomicBool::new(false);

/// Files Finder asked BananaV to open (a project double-clicked).
static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

pub fn take_quit_request() -> bool {
    QUIT_ASKED.swap(false, Ordering::AcqRel)
}

pub fn take_opened_files() -> Vec<PathBuf> {
    OPENED
        .lock()
        .map(|mut o| std::mem::take(&mut *o))
        .unwrap_or_default()
}

/// `applicationShouldTerminate:` — never now: the interface is told, and
/// ends the program itself once the project is dealt with.
unsafe extern "C-unwind" fn should_terminate(
    _this: *mut AnyObject,
    _cmd: Sel,
    _app: *mut AnyObject,
) -> usize {
    QUIT_ASKED.store(true, Ordering::Release);
    super::wake();
    // NSTerminateCancel.
    0
}

/// `application:openURLs:` — the files are queued for the interface.
unsafe extern "C-unwind" fn open_urls(
    _this: *mut AnyObject,
    _cmd: Sel,
    _app: *mut AnyObject,
    urls: *mut NSArray<NSURL>,
) {
    // SAFETY: the array AppKit passes, alive during the call.
    let Some(urls) = (unsafe { urls.as_ref() }) else {
        return;
    };
    let paths: Vec<PathBuf> = urls.iter().filter_map(|u| u.to_file_path()).collect();
    if let Ok(mut o) = OPENED.lock() {
        o.extend(paths);
    }
    super::wake();
}

/// Gives the application's delegate (the window library's) BananaV's
/// answers to Quit and to files opened: `applicationShouldTerminate:` and
/// `application:openURLs:`, added to its class.
fn hook_delegate(mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    let Some(delegate) = app.delegate() else {
        return;
    };
    let object = Retained::as_ptr(&delegate).cast::<AnyObject>();
    // SAFETY: the delegate's class; methods added with the types AppKit
    // calls them with (an unsigned integer back; nothing back), each once
    // (adding one the class has fails, harmlessly).
    let added = unsafe {
        let class = objc2::ffi::object_getClass(object).cast_mut();
        let terminate: unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize =
            should_terminate;
        let quit = objc2::ffi::class_addMethod(
            class,
            sel!(applicationShouldTerminate:),
            std::mem::transmute::<
                unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize,
                Imp,
            >(terminate),
            c"Q@:@".as_ptr(),
        )
        .as_bool();
        let open: unsafe extern "C-unwind" fn(
            *mut AnyObject,
            Sel,
            *mut AnyObject,
            *mut NSArray<NSURL>,
        ) = open_urls;
        let files = objc2::ffi::class_addMethod(
            class,
            sel!(application:openURLs:),
            std::mem::transmute::<
                unsafe extern "C-unwind" fn(
                    *mut AnyObject,
                    Sel,
                    *mut AnyObject,
                    *mut NSArray<NSURL>,
                ),
                Imp,
            >(open),
            c"v@:@@".as_ptr(),
        )
        .as_bool();
        quit || files
    };
    // AppKit reads which of its methods a delegate has when it is set: set
    // again, it sees the new ones.
    if added {
        app.setDelegate(None);
        app.setDelegate(Some(&delegate));
    }
}

/// Before the window library runs the application: when it is about to
/// finish launching (its delegate set, the files it was opened with not
/// handed over yet), BananaV's answers are added to the delegate.
pub fn before_run() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let _ = NSApplication::sharedApplication(mtm);
    let block = RcBlock::new(|_n: NonNull<NSNotification>| {
        if let Some(mtm) = MainThreadMarker::new() {
            hook_delegate(mtm);
        }
    });
    // SAFETY: AppKit's notification's name; the block is copied and kept
    // by the center, the observer for as long as the program runs.
    let observer = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSApplicationWillFinishLaunchingNotification),
            None,
            None,
            &block,
        )
    };
    std::mem::forget(observer);
}

/// A process of BananaV's that shows nothing (a plug-in scan's child):
/// no icon in the Dock, though the plug-ins it loads may start AppKit.
pub fn background_process() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm)
            .setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Prohibited);
    }
}

/// The interface's thread has started (its windows are up): the key watch
/// starts, the displays' names are read, and the delegate gets BananaV's
/// answers if `before_run` could not give them.
pub fn ui_thread_started() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    watch_keys(mtm);
    read_display_names(mtm);
    hook_delegate(mtm);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_to_the_mac_s_codes() {
        // [ ] ; ' and the keypad.
        assert_eq!(mac_key(0x1A), Some(0x21));
        assert_eq!(mac_key(0x1B), Some(0x1E));
        assert_eq!(mac_key(0x27), Some(0x29));
        assert_eq!(mac_key(0x28), Some(0x27));
        assert_eq!(mac_key(0x7F), None);
        assert_eq!(keypad_code('5'), Some(0x57));
        assert!(KEYPAD_CODES.iter().all(|&c| c < 128));
    }

    #[test]
    fn the_clock_and_the_disk_answer() {
        assert!(local_time().is_some());
        assert!(total_memory().is_some_and(|m| m > 1 << 30));
        assert!(free_space(&std::env::temp_dir()).is_some());
    }
}
