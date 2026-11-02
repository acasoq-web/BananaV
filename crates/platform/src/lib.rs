//! Small OS services the window library does not provide.
//!
//! While files are dragged in from the system (Explorer, the desktop) the
//! window library reports which files they are but not where the mouse is,
//! so the timeline asks the OS directly. Dialog windows are made owned by
//! the main window here too, which the window library cannot express.
//! It also tells how much room a disk has left and the time of day, and
//! what long jobs need (see [`Usage`], [`taskbar_progress`]).

mod disc;
mod library;
mod midi;
mod monitor;
mod plugin_window;
mod screen;

pub use disc::{
    BurnError, Burner, CD_1X, CD_SECTOR, CdOptions, CdTrackData, DVD_1X, burn_audio_disc,
    burn_audio_track, burn_image, burners, close_disc, erase_disc, free_sectors,
};
pub use library::{Library, VersionTexts, read_library, version_texts};
pub use midi::{MidiHandler, MidiIn, MidiOut, midi_inputs, midi_outputs, unpack};
pub use monitor::{Usage, notify_sound, shutdown_in, taskbar_progress};
pub use plugin_window::{PLUGIN_WINDOWS, PluginWindow, Resizer};
pub use screen::{AppWindow, Grab, app_windows, grab_area, grab_window};

/// Where this user's settings go: `%APPDATA%` on Windows,
/// `$XDG_CONFIG_HOME` (or `~/.config`) elsewhere.
pub fn config_dir() -> Option<std::path::PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("APPDATA").map(std::path::PathBuf::from);
    }
    xdg_dir("XDG_CONFIG_HOME", ".config")
}

/// Where files that can be made again go (prerendered video):
/// `%LOCALAPPDATA%` on Windows, `$XDG_CACHE_HOME` (or `~/.cache`) elsewhere.
pub fn cache_dir() -> Option<std::path::PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
    }
    xdg_dir("XDG_CACHE_HOME", ".cache")
}

/// The user's home folder.
pub fn home_dir() -> Option<std::path::PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .filter(|h| !h.is_empty())
        .map(std::path::PathBuf::from)
}

/// An XDG base folder: the variable when it holds an absolute path, else
/// `~/<fallback>`.
fn xdg_dir(var: &str, fallback: &str) -> Option<std::path::PathBuf> {
    std::env::var_os(var)
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home_dir().map(|h| h.join(fallback)))
}

/// The user's folders the file browser starts with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserDir {
    Desktop,
    Documents,
    Downloads,
    Music,
    Pictures,
    Videos,
}

/// Where the system keeps one of the user's folders: under the home folder
/// on Windows; on Linux where `user-dirs.dirs` says (the names follow the
/// system's language: `~/Vídeos`), else the English name.
pub fn user_dir(which: UserDir) -> Option<std::path::PathBuf> {
    let english = match which {
        UserDir::Desktop => "Desktop",
        UserDir::Documents => "Documents",
        UserDir::Downloads => "Downloads",
        UserDir::Music => "Music",
        UserDir::Pictures => "Pictures",
        UserDir::Videos => "Videos",
    };
    let home = home_dir()?;
    if !cfg!(windows) {
        let key = match which {
            UserDir::Desktop => "XDG_DESKTOP_DIR",
            UserDir::Documents => "XDG_DOCUMENTS_DIR",
            UserDir::Downloads => "XDG_DOWNLOAD_DIR",
            UserDir::Music => "XDG_MUSIC_DIR",
            UserDir::Pictures => "XDG_PICTURES_DIR",
            UserDir::Videos => "XDG_VIDEOS_DIR",
        };
        let file = config_dir().map(|c| c.join("user-dirs.dirs"));
        if let Some(text) = file.and_then(|f| std::fs::read_to_string(f).ok())
            && let Some(dir) = user_dirs_entry(&text, key, &home)
        {
            return Some(dir);
        }
    }
    Some(home.join(english))
}

/// The folder `user-dirs.dirs` gives `key` (`XDG_VIDEOS_DIR="$HOME/Vídeos"`).
fn user_dirs_entry(text: &str, key: &str, home: &std::path::Path) -> Option<std::path::PathBuf> {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| l.strip_prefix(key).is_some_and(|r| r.starts_with('=')))?;
    let value = line.split_once('=')?.1.trim().trim_matches('"');
    let dir = match value.strip_prefix("$HOME") {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None => std::path::PathBuf::from(value),
    };
    // Set to the home folder itself: the folder is turned off.
    (dir.has_root() && dir != home).then_some(dir)
}

/// Mouse position on the screen, in physical pixels, if the OS can tell.
pub fn cursor_screen_position() -> Option<(i32, i32)> {
    imp::cursor()
}

/// The color of the screen's pixel at `(x, y)` (physical pixels, as
/// [`cursor_screen_position`] gives), 8-bit RGB: an eyedropper's sample.
pub fn screen_color(x: i32, y: i32) -> Option<[u8; 3]> {
    imp::screen_color(x, y)
}

/// Whether the mouse's main button is down now, wherever the mouse is.
pub fn main_button_down() -> bool {
    imp::main_button_down()
}

/// Whether the numeric keypad's key `c` ('0'–'9', '+', '-', '*', '/',
/// '.') is held now. Programs see a keypad digit as the digit itself; this
/// tells them apart (Windows, and X11 on Linux; elsewhere never).
pub fn numpad_down(c: char) -> bool {
    imp::numpad_down(c)
}

/// Sends the mouse to this process's window titled `title` until the next
/// click, so that a click on the program's other windows does not act on
/// them (an eyedropper picking from the screen). Returns whether the window
/// exists.
pub fn capture_mouse(title: &str) -> bool {
    imp::capture_mouse(title)
}

/// Ends [`capture_mouse`] before a click.
pub fn release_mouse() {
    imp::release_mouse()
}

/// Whether `path` is on a removable drive (a memory card, a USB stick, a
/// disc).
pub fn is_removable(path: &std::path::Path) -> bool {
    imp::is_removable(path)
}

/// A drive whose media comes and goes: a memory card, a camera, a disc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drive {
    /// Its root folder (`E:\`).
    pub root: std::path::PathBuf,
    /// The volume's name (empty without one).
    pub label: String,
    /// An optical drive (CD, DVD, Blu-ray).
    pub optical: bool,
}

/// The removable and optical drives that have media in them now.
pub fn removable_drives() -> Vec<Drive> {
    imp::removable_drives()
}

/// Whether one of this process's windows is the one the user works in
/// (`None`: the OS cannot tell).
pub fn app_is_active() -> Option<bool> {
    imp::app_is_active()
}

/// How far the first joystick (or gamepad) is pushed: x to the right and y
/// up, from −1 to 1, about zero at rest; `None` without one.
pub fn joystick() -> Option<(f32, f32)> {
    imp::joystick()
}

/// A display (monitor) of this computer.
#[derive(Clone, Debug, PartialEq)]
pub struct Display {
    /// The OS's name of it (`\\.\DISPLAY2`), the same while it stays
    /// plugged in.
    pub name: String,
    /// Where it is on the desktop and its size, in pixels.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Its refresh rate (Hz); 0 when unknown.
    pub hz: u32,
    pub primary: bool,
    /// Its scale (1 at 96 DPI).
    pub scale: f32,
}

/// This computer's displays, in the OS's order (the window library numbers
/// them in that order too).
pub fn displays() -> Vec<Display> {
    imp::displays()
}

/// The ASIO drivers installed (their names, as the drivers registered
/// them).
pub fn asio_drivers() -> Vec<String> {
    imp::asio_drivers()
}

/// Whether files ending in `.ext` open with this program (`None`: the OS
/// cannot tell).
pub fn opens_project_files(ext: &str) -> Option<bool> {
    imp::opens_project_files(ext)
}

/// Makes files ending in `.ext` open with this program, for this user:
/// they are of the type `prog_id`, described as `description`.
pub fn associate_project_files(ext: &str, prog_id: &str, description: &str) -> Result<(), String> {
    imp::associate_project_files(ext, prog_id, description)
}

/// The character the key with this scan code types in the user's keyboard
/// layout (unshifted; dead keys give their accent), e.g. scan code 0x1A is
/// `[` on a US keyboard and `´` on a Brazilian ABNT2 one.
pub fn key_char(scancode: u32) -> Option<char> {
    imp::key_char(scancode)
}

/// Makes this process's windows titled `child` owned by its window titled
/// `owner`, like dialog boxes: they stay in front of it, minimize with it
/// and have no taskbar button of their own. Returns whether a window titled
/// `child` exists yet (the caller retries until the OS has created it).
pub fn set_window_owner(child: &str, owner: &str) -> bool {
    imp::set_owner(child, owner)
}

/// This process's windows refusing input, as a modal window's owner does
/// while it is up (VEGAS's windows while it renders: a click on them brings
/// the modal one forward). Dropping it gives them input back.
#[must_use]
#[derive(Debug)]
pub struct InputBlock {
    windows: Vec<isize>,
}

/// Blocks every visible window of this process but those titled as in
/// `keep` (the modal ones). Windows only: elsewhere it blocks nothing, and
/// the program covers its windows' contents itself.
pub fn block_input(keep: &[&str]) -> InputBlock {
    InputBlock {
        windows: imp::block_input(keep),
    }
}

impl Drop for InputBlock {
    fn drop(&mut self) {
        imp::unblock_input(&self.windows);
    }
}

/// Bytes free for this user on the disk holding `path` (the nearest folder
/// of it that exists), if the OS can tell.
/// The computer's memory (RAM), in bytes.
pub fn total_memory() -> Option<u64> {
    imp::total_memory()
}

pub fn free_space(path: &std::path::Path) -> Option<u64> {
    let dir = path.ancestors().find(|p| p.is_dir())?;
    imp::free_space(dir)
}

/// The time of day on this computer's clock: (hours, minutes, seconds).
pub fn local_time() -> Option<(u32, u32, u32)> {
    imp::local_time()
}

/// How far this computer's clock is ahead of UTC, in seconds (its time
/// zone and daylight saving now).
pub fn utc_offset() -> i64 {
    imp::utc_offset()
}

/// Whether Windows Security's Controlled folder access is on: it keeps apps
/// it does not trust from writing to Documents, Pictures, Videos, Music and
/// Desktop (they get "file not found" or "access denied").
pub fn controlled_folder_access() -> bool {
    imp::controlled_folder_access()
}

/// `path` (a file or a folder) moved to the Recycle Bin — on Linux the
/// desktop's trash, as freedesktop.org's Trash specification says — from
/// where it can be put back. Never deleted for good: on Windows, when it
/// does not fit in the Recycle Bin, Windows asks first; on Linux, a file on
/// another disk than the trash's stays where it is (an error).
pub fn to_recycle_bin(path: &std::path::Path) -> std::io::Result<()> {
    if std::fs::symlink_metadata(path).is_err() {
        return Err(std::io::Error::from(std::io::ErrorKind::NotFound));
    }
    imp::to_recycle_bin(&std::path::absolute(path)?)
}

/// Whether `dir` takes new files, or why not.
pub fn try_write(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let probe = dir.join(format!(".bananav-write-{}.tmp", std::process::id()));
    std::fs::File::create(&probe)?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// Whether a failed write of a file in `dir` looks like Controlled folder
/// access refusing it: the error is "not found" or "access denied" though
/// the folder (or the one it would be made in) exists, and the protection
/// is on.
pub fn blocked_by_folder_protection(dir: &std::path::Path, e: &std::io::Error) -> bool {
    use std::io::ErrorKind::{NotFound, PermissionDenied};
    matches!(e.kind(), NotFound | PermissionDenied)
        && dir
            .ancestors()
            .find(|d| d.exists())
            .is_some_and(|d| d.is_dir())
        && controlled_folder_access()
}

/// This process's window titled `title`, as the OS's handle.
#[cfg(windows)]
fn find_window(title: &str) -> Option<isize> {
    imp::find_window(title)
}

/// This process's window titled `title`, as the system's handle (its HWND
/// on Windows; none elsewhere, where windows have no such handle to give).
pub fn window_handle(title: &str) -> Option<isize> {
    #[cfg(windows)]
    return find_window(title);
    #[cfg(not(windows))]
    {
        let _ = title;
        None
    }
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Foundation::{HWND, LPARAM, POINT};
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW};
    use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        EnableWindow, IsWindowEnabled, MAPVK_VK_TO_CHAR, MAPVK_VSC_TO_VK, MapVirtualKeyW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GW_OWNER, GWLP_HWNDPARENT, GetCursorPos, GetWindow, GetWindowThreadProcessId,
        HWND_TOP, InternalGetWindowText, IsWindowVisible, SWP_NOMOVE, SWP_NOSIZE,
        SetWindowLongPtrW, SetWindowPos,
    };
    use windows::core::BOOL;

    /// Top-level windows of this process, with their titles.
    struct Windows {
        pid: u32,
        found: Vec<(HWND, String)>,
    }

    #[allow(unsafe_code)]
    unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the `&mut Windows` passed to EnumWindows below,
        // alive and not otherwise borrowed while it enumerates.
        let w = unsafe { &mut *(data.0 as *mut Windows) };
        let mut pid = 0u32;
        // SAFETY: `pid` is a valid, writable u32.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == w.pid {
            let mut buf = [0u16; 512];
            // SAFETY: `buf` is a valid, writable buffer of its length. This
            // reads the title without sending the window a message (the
            // caller may be inside that window's own event handling).
            let n = unsafe { InternalGetWindowText(hwnd, &mut buf) }.max(0) as usize;
            w.found.push((hwnd, String::from_utf16_lossy(&buf[..n])));
        }
        BOOL(1)
    }

    /// This process's top-level windows and their titles.
    #[allow(unsafe_code)]
    fn own_windows() -> Windows {
        let mut w = Windows {
            // SAFETY: no arguments, no side effects.
            pid: unsafe { GetCurrentProcessId() },
            found: Vec::new(),
        };
        // SAFETY: `collect` only touches `w` through the pointer, and `w`
        // outlives the call.
        let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut w as *mut Windows as isize)) };
        w
    }

    pub fn find_window(title: &str) -> Option<isize> {
        own_windows()
            .found
            .into_iter()
            .find(|(_, t)| t == title)
            .map(|(h, _)| h.0 as isize)
    }

    #[allow(unsafe_code)]
    pub fn set_owner(child: &str, owner: &str) -> bool {
        let w = own_windows();
        let Some(&(main, _)) = w.found.iter().find(|(_, t)| t == owner) else {
            return false;
        };
        let mut exists = false;
        for &(hwnd, ref title) in &w.found {
            if title != child || hwnd == main {
                continue;
            }
            exists = true;
            // SAFETY: `hwnd` and `main` are live top-level windows of this
            // process; GWLP_HWNDPARENT on a top-level window sets its owner.
            // Owned now, it is brought in front (and made active, as it
            // opened): the main window may have come up over it meanwhile —
            // when the program starts, it shows after its first frame.
            unsafe {
                if GetWindow(hwnd, GW_OWNER).ok() != Some(main) {
                    SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, main.0 as isize);
                    let _ = SetWindowPos(hwnd, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
                }
            }
        }
        exists
    }

    #[allow(unsafe_code)]
    pub fn block_input(keep: &[&str]) -> Vec<isize> {
        let mut blocked = Vec::new();
        for (hwnd, title) in own_windows().found {
            if keep.contains(&title.as_str()) {
                continue;
            }
            // SAFETY: `hwnd` is a live top-level window of this process.
            unsafe {
                if IsWindowVisible(hwnd).as_bool() && IsWindowEnabled(hwnd).as_bool() {
                    let _ = EnableWindow(hwnd, false);
                    blocked.push(hwnd.0 as isize);
                }
            }
        }
        blocked
    }

    #[allow(unsafe_code)]
    pub fn unblock_input(windows: &[isize]) {
        // Only those still this process's: a handle closed since may name
        // another program's window now.
        for (hwnd, _) in own_windows().found {
            if windows.contains(&(hwnd.0 as isize)) {
                // SAFETY: `hwnd` is a live top-level window of this process.
                let _ = unsafe { EnableWindow(hwnd, true) };
            }
        }
    }

    #[allow(unsafe_code)]
    pub fn key_char(scancode: u32) -> Option<char> {
        // SAFETY: plain lookups in the current keyboard layout, no pointers.
        let vk = unsafe { MapVirtualKeyW(scancode, MAPVK_VSC_TO_VK) };
        if vk == 0 {
            return None;
        }
        // SAFETY: as above.
        let ch = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_CHAR) } & 0x7FFF_FFFF;
        char::from_u32(ch).filter(|c| !c.is_control())
    }

    #[allow(unsafe_code)]
    pub fn total_memory() -> Option<u64> {
        use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut m = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        // SAFETY: `m` is a valid, writable MEMORYSTATUSEX with its length set.
        unsafe { GlobalMemoryStatusEx(&mut m) }.ok()?;
        Some(m.ullTotalPhys)
    }

    #[allow(unsafe_code)]
    pub fn free_space(dir: &std::path::Path) -> Option<u64> {
        let name = windows::core::HSTRING::from(dir.as_os_str());
        let mut free = 0u64;
        // SAFETY: `name` is a valid NUL-terminated string and `free` a valid,
        // writable u64 for the duration of the call.
        unsafe { GetDiskFreeSpaceExW(&name, Some(&mut free), None, None) }.ok()?;
        Some(free)
    }

    #[allow(unsafe_code)]
    pub fn cursor() -> Option<(i32, i32)> {
        let mut p = POINT::default();
        // SAFETY: `p` is a valid, writable POINT for the duration of the call.
        unsafe { GetCursorPos(&mut p) }.ok()?;
        Some((p.x, p.y))
    }

    #[allow(unsafe_code)]
    pub fn screen_color(x: i32, y: i32) -> Option<[u8; 3]> {
        use windows::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC};
        // SAFETY: the screen's device context is taken and given back here,
        // and only read in between.
        let c = unsafe {
            let dc = GetDC(None);
            if dc.is_invalid() {
                return None;
            }
            let c = GetPixel(dc, x, y);
            ReleaseDC(None, dc);
            c.0
        };
        // CLR_INVALID: outside every screen.
        (c != 0xFFFF_FFFF).then_some([c as u8, (c >> 8) as u8, (c >> 16) as u8])
    }

    #[allow(unsafe_code)]
    pub fn numpad_down(c: char) -> bool {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        // VK_NUMPAD0… and the keypad's operators.
        let vk: i32 = match c {
            '0'..='9' => 0x60 + (c as i32 - '0' as i32),
            '*' => 0x6A,
            '+' => 0x6B,
            '-' => 0x6D,
            '.' => 0x6E,
            '/' => 0x6F,
            _ => return false,
        };
        // SAFETY: a plain query of the keyboard's state.
        unsafe { GetAsyncKeyState(vk) < 0 }
    }

    #[allow(unsafe_code)]
    pub fn main_button_down() -> bool {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON,
        };
        use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};
        // SAFETY: plain queries of the system's state. The keys are the
        // physical buttons: with them swapped the main one is the right.
        unsafe {
            let key = if GetSystemMetrics(SM_SWAPBUTTON) != 0 {
                VK_RBUTTON
            } else {
                VK_LBUTTON
            };
            GetAsyncKeyState(key.0 as i32) < 0
        }
    }

    #[allow(unsafe_code)]
    pub fn capture_mouse(title: &str) -> bool {
        use windows::Win32::UI::Input::KeyboardAndMouse::SetCapture;
        let Some(h) = find_window(title) else {
            return false;
        };
        // SAFETY: `h` is a live top-level window of this process, made by
        // the interface's thread, which calls this.
        unsafe { SetCapture(HWND(h as *mut core::ffi::c_void)) };
        true
    }

    #[allow(unsafe_code)]
    pub fn release_mouse() {
        use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
        // SAFETY: no arguments; it fails harmlessly without a capture.
        let _ = unsafe { ReleaseCapture() };
    }

    #[allow(unsafe_code)]
    pub fn removable_drives() -> Vec<super::Drive> {
        use windows::Win32::Storage::FileSystem::{
            GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
        };
        // SAFETY: no arguments.
        let mask = unsafe { GetLogicalDrives() };
        let mut out = Vec::new();
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let root = format!("{}:\\", (b'A' + i as u8) as char);
            let wide = windows::core::HSTRING::from(root.as_str());
            // SAFETY: `wide` is a valid NUL-terminated string for the call.
            let kind = unsafe { GetDriveTypeW(&wide) };
            // DRIVE_REMOVABLE and DRIVE_CDROM.
            if !matches!(kind, 2 | 5) {
                continue;
            }
            let mut name = [0u16; 261];
            // SAFETY: `wide` is valid and `name` writable; it fails when
            // there is no media in the drive (it is left out then).
            let ok =
                unsafe { GetVolumeInformationW(&wide, Some(&mut name), None, None, None, None) };
            if ok.is_err() {
                continue;
            }
            let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
            out.push(super::Drive {
                root: std::path::PathBuf::from(root),
                label: String::from_utf16_lossy(&name[..end]),
                optical: kind == 5,
            });
        }
        out
    }

    #[allow(unsafe_code)]
    pub fn is_removable(path: &std::path::Path) -> bool {
        use windows::Win32::Storage::FileSystem::GetDriveTypeW;
        let Some(std::path::Component::Prefix(prefix)) = path.components().next() else {
            return false;
        };
        let root =
            windows::core::HSTRING::from(format!("{}\\", prefix.as_os_str().to_string_lossy()));
        // DRIVE_REMOVABLE and DRIVE_CDROM.
        // SAFETY: `root` is a valid NUL-terminated string for the call.
        matches!(unsafe { GetDriveTypeW(&root) }, 2 | 5)
    }

    #[allow(unsafe_code)]
    unsafe extern "system" fn monitor(
        hm: windows::Win32::Graphics::Gdi::HMONITOR,
        _dc: windows::Win32::Graphics::Gdi::HDC,
        _rect: *mut windows::Win32::Foundation::RECT,
        data: LPARAM,
    ) -> BOOL {
        // SAFETY: `data` is the `&mut Vec` passed to EnumDisplayMonitors
        // below, alive and not otherwise borrowed while it enumerates.
        let list = unsafe { &mut *(data.0 as *mut Vec<windows::Win32::Graphics::Gdi::HMONITOR>) };
        list.push(hm);
        BOOL(1)
    }

    #[allow(unsafe_code)]
    pub fn displays() -> Vec<super::Display> {
        use windows::Win32::Graphics::Gdi::{
            DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplayMonitors, EnumDisplaySettingsW,
            GetMonitorInfoW, HMONITOR, MONITORINFO, MONITORINFOEXW,
        };
        use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
        let mut handles: Vec<HMONITOR> = Vec::new();
        // SAFETY: `monitor` only pushes into `handles` through the pointer,
        // and `handles` outlives the call.
        let _ = unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(monitor),
                LPARAM(&mut handles as *mut Vec<HMONITOR> as isize),
            )
        };
        handles
            .into_iter()
            .filter_map(|hm| {
                let mut info = MONITORINFOEXW::default();
                info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
                // SAFETY: `info` is a MONITORINFOEXW whose size is set, as the
                // call asks for the extended form.
                let ok = unsafe {
                    GetMonitorInfoW(hm, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO)
                };
                if !ok.as_bool() {
                    return None;
                }
                let len = info.szDevice.iter().position(|&c| c == 0).unwrap_or(32);
                let name = String::from_utf16_lossy(&info.szDevice[..len]);
                let mut mode = DEVMODEW {
                    dmSize: std::mem::size_of::<DEVMODEW>() as u16,
                    ..Default::default()
                };
                // SAFETY: the device name is NUL-terminated and `mode` is a
                // writable DEVMODEW whose size is set.
                let hz = if unsafe {
                    EnumDisplaySettingsW(
                        windows::core::PCWSTR(info.szDevice.as_ptr()),
                        ENUM_CURRENT_SETTINGS,
                        &mut mode,
                    )
                }
                .as_bool()
                {
                    mode.dmDisplayFrequency
                } else {
                    0
                };
                let (mut dx, mut dy) = (96u32, 96u32);
                // SAFETY: plain query; `dx` and `dy` are writable.
                let scale =
                    match unsafe { GetDpiForMonitor(hm, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) } {
                        Ok(()) => dx as f32 / 96.0,
                        Err(_) => 1.0,
                    };
                let r = info.monitorInfo.rcMonitor;
                Some(super::Display {
                    name,
                    x: r.left,
                    y: r.top,
                    width: (r.right - r.left).max(0) as u32,
                    height: (r.bottom - r.top).max(0) as u32,
                    hz,
                    // MONITORINFOF_PRIMARY.
                    primary: info.monitorInfo.dwFlags & 1 != 0,
                    scale,
                })
            })
            .collect()
    }

    #[allow(unsafe_code)]
    pub fn asio_drivers() -> Vec<String> {
        use windows::Win32::Foundation::ERROR_SUCCESS;
        use windows::Win32::System::Registry::{
            HKEY, KEY_READ, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW,
        };
        let mut key = HKEY::default();
        // SAFETY: `key` is writable; it is closed below.
        let opened = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                windows::core::w!("SOFTWARE\\ASIO"),
                None,
                KEY_READ,
                &mut key,
            )
        };
        if opened != ERROR_SUCCESS {
            return Vec::new();
        }
        let mut out = Vec::new();
        for i in 0..256 {
            let mut buf = [0u16; 256];
            let mut len = buf.len() as u32;
            // SAFETY: `buf` holds `len` characters, and `len` is writable.
            let r = unsafe {
                RegEnumKeyExW(
                    key,
                    i,
                    Some(windows::core::PWSTR(buf.as_mut_ptr())),
                    &mut len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            if r != ERROR_SUCCESS {
                break;
            }
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
        }
        // SAFETY: `key` was opened above.
        let _ = unsafe { RegCloseKey(key) };
        out
    }

    #[allow(unsafe_code)]
    pub fn joystick() -> Option<(f32, f32)> {
        use windows::Win32::Media::Multimedia::{
            JOY_RETURNX, JOY_RETURNY, JOYCAPSW, JOYERR_NOERROR, JOYINFOEX, JOYSTICKID1,
            joyGetDevCapsW, joyGetPosEx,
        };
        let mut info = JOYINFOEX {
            dwSize: std::mem::size_of::<JOYINFOEX>() as u32,
            dwFlags: (JOY_RETURNX | JOY_RETURNY) as u32,
            ..JOYINFOEX::default()
        };
        let mut caps = JOYCAPSW::default();
        // SAFETY: both structures are ours, writable and sized as the calls
        // are told.
        let ok = unsafe {
            joyGetPosEx(JOYSTICKID1, &mut info) == JOYERR_NOERROR
                && joyGetDevCapsW(
                    JOYSTICKID1 as usize,
                    &mut caps,
                    std::mem::size_of::<JOYCAPSW>() as u32,
                ) == JOYERR_NOERROR
        };
        if !ok {
            return None;
        }
        let axis = |v: u32, lo: u32, hi: u32| {
            if hi > lo {
                (v.clamp(lo, hi) - lo) as f32 / (hi - lo) as f32 * 2.0 - 1.0
            } else {
                0.0
            }
        };
        let (x, y) = (info.dwXpos, info.dwYpos);
        Some((
            axis(x, caps.wXmin, caps.wXmax),
            // The joystick's y grows toward the user.
            -axis(y, caps.wYmin, caps.wYmax),
        ))
    }

    #[allow(unsafe_code)]
    pub fn app_is_active() -> Option<bool> {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        // SAFETY: plain queries; `pid` is a valid, writable u32.
        unsafe {
            let w = GetForegroundWindow();
            if w.is_invalid() {
                return Some(false);
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(w, Some(&mut pid));
            Some(pid == GetCurrentProcessId())
        }
    }

    #[allow(unsafe_code)]
    pub fn opens_project_files(ext: &str) -> Option<bool> {
        use windows::Win32::UI::Shell::{ASSOCF_NONE, ASSOCSTR_EXECUTABLE, AssocQueryStringW};
        use windows::core::{HSTRING, PWSTR, w};
        let ext = HSTRING::from(format!(".{ext}"));
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        // SAFETY: `buf` is a writable buffer of `len` characters; the other
        // arguments are valid strings for the call.
        let r = unsafe {
            AssocQueryStringW(
                ASSOCF_NONE,
                ASSOCSTR_EXECUTABLE,
                &ext,
                w!("open"),
                Some(PWSTR(buf.as_mut_ptr())),
                &mut len,
            )
        };
        if r.is_err() {
            // Nothing opens them.
            return Some(false);
        }
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let opener = std::path::PathBuf::from(String::from_utf16_lossy(&buf[..n]));
        let here = std::env::current_exe().ok()?;
        let same =
            |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        Some(
            same(&opener).to_string_lossy().to_lowercase()
                == same(&here).to_string_lossy().to_lowercase(),
        )
    }

    #[allow(unsafe_code)]
    pub fn associate_project_files(
        ext: &str,
        prog_id: &str,
        description: &str,
    ) -> Result<(), String> {
        use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
        use windows::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
        use windows::core::{HSTRING, PCWSTR};
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = exe.display().to_string();
        let set = |key: &str, value: &str| -> Result<(), String> {
            let key = HSTRING::from(format!(r"Software\Classes\{key}"));
            let data: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
            // SAFETY: `key` and `data` live through the call; `data` is a
            // NUL-terminated UTF-16 string of the byte length given.
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    &key,
                    PCWSTR::null(),
                    REG_SZ.0,
                    Some(data.as_ptr() as *const core::ffi::c_void),
                    (data.len() * 2) as u32,
                )
            }
            .ok()
            .map_err(|e| e.to_string())
        };
        set(&format!(".{ext}"), prog_id)?;
        set(prog_id, description)?;
        set(&format!(r"{prog_id}\DefaultIcon"), &format!("\"{exe}\",0"))?;
        set(
            &format!(r"{prog_id}\shell\open\command"),
            &format!("\"{exe}\" \"%1\""),
        )?;
        // SAFETY: tells the shell that file types changed; no pointers.
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
        Ok(())
    }

    #[allow(unsafe_code)]
    pub fn controlled_folder_access() -> bool {
        let mut value = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: `value` and `size` are valid, writable and of the sizes
        // given; the key and value names are static strings.
        let r = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                windows::core::w!(
                    r"SOFTWARE\Microsoft\Windows Defender\Windows Defender Exploit Guard\Controlled Folder Access"
                ),
                windows::core::w!("EnableControlledFolderAccess"),
                RRF_RT_REG_DWORD,
                None,
                Some(&mut value as *mut u32 as *mut _),
                Some(&mut size),
            )
        };
        r.is_ok() && value != 0
    }

    pub fn to_recycle_bin(path: &std::path::Path) -> std::io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::UI::Shell::{
            FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
            FOF_WANTNUKEWARNING, SHFILEOPSTRUCTW, SHFileOperationW,
        };
        // A list of paths, each ended by a zero, the list by another.
        let mut from: Vec<u16> = path.as_os_str().encode_wide().collect();
        from.extend([0, 0]);
        let flags = FOF_ALLOWUNDO.0
            | FOF_NOCONFIRMATION.0
            | FOF_NOERRORUI.0
            | FOF_SILENT.0
            | FOF_WANTNUKEWARNING.0;
        let mut op = SHFILEOPSTRUCTW {
            wFunc: FO_DELETE,
            pFrom: windows::core::PCWSTR(from.as_ptr()),
            fFlags: flags as u16,
            ..Default::default()
        };
        // SAFETY: `from` is the list the call asks for and outlives it;
        // the structure is filled for a delete.
        #[allow(unsafe_code)]
        let r = unsafe { SHFileOperationW(&mut op) };
        if r != 0 {
            Err(std::io::Error::other(format!(
                "the Recycle Bin refused it (error {r:#x})"
            )))
        } else if op.fAnyOperationsAborted.as_bool() {
            Err(std::io::Error::from(std::io::ErrorKind::Interrupted))
        } else {
            Ok(())
        }
    }

    pub fn local_time() -> Option<(u32, u32, u32)> {
        // SAFETY: GetLocalTime only fills the returned structure.
        #[allow(unsafe_code)]
        let t = unsafe { GetLocalTime() };
        Some((t.wHour as u32, t.wMinute as u32, t.wSecond as u32))
    }

    pub fn utc_offset() -> i64 {
        // SAFETY: both only fill the returned structures.
        #[allow(unsafe_code)]
        let (local, utc) = unsafe { (GetLocalTime(), GetSystemTime()) };
        let seconds = |t: windows::Win32::Foundation::SYSTEMTIME| {
            super::days_from_civil(t.wYear as i64, t.wMonth as u32, t.wDay as u32) * 86_400
                + t.wHour as i64 * 3600
                + t.wMinute as i64 * 60
        };
        // Whole minutes: the two clocks were read a moment apart.
        (seconds(local) - seconds(utc)) / 60 * 60
    }
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod x11;

/// Linux: the disks, memory, clock and joystick from the kernel; the mouse,
/// keypad, screen, displays and windows from X11; file types as the
/// freedesktop.org standards say.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod imp {
    use std::path::{Path, PathBuf};

    pub use super::x11::{
        app_is_active, capture_mouse, cursor, displays, key_char, main_button_down, numpad_down,
        release_mouse, screen_color, set_owner,
    };

    /// X has no window that refuses input: the program covers them.
    pub fn block_input(_keep: &[&str]) -> Vec<isize> {
        Vec::new()
    }

    pub fn unblock_input(_windows: &[isize]) {}

    /// The desktop entry that opens BananaV's files.
    const DESKTOP: &str = "bananav.desktop";

    /// The type of `.ext` files: BananaV's own.
    fn mime_type(ext: &str) -> String {
        format!("application/x-{ext}")
    }

    /// Where the user's programs and types live (`~/.local/share`).
    fn data_home() -> Option<PathBuf> {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.has_root())
            .or_else(|| Some(super::home_dir()?.join(".local").join("share")))
    }

    /// What the desktop entry runs: the AppImage when BananaV is one (its
    /// files move at each start), else this program.
    fn launcher() -> Option<PathBuf> {
        std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .or_else(|| std::env::current_exe().ok())
    }

    /// The default program (desktop entry) of `mime` in a `mimeapps.list`.
    pub(super) fn default_app(list: &str, mime: &str) -> Option<String> {
        let mut defaults = false;
        for line in list.lines().map(str::trim) {
            if line.starts_with('[') {
                defaults = line == "[Default Applications]";
            } else if defaults
                && let Some(v) = line.strip_prefix(mime).and_then(|r| r.strip_prefix('='))
            {
                return v
                    .split(';')
                    .next()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
            }
        }
        None
    }

    /// `list` with `mime` opened by `app` by default.
    pub(super) fn with_default(list: &str, mime: &str, app: &str) -> String {
        let entry = format!("{mime}={app};");
        let mut out: Vec<String> = Vec::new();
        let (mut defaults, mut done) = (false, false);
        for line in list.lines() {
            let t = line.trim();
            if t.starts_with('[') {
                if defaults && !done {
                    out.push(entry.clone());
                    done = true;
                }
                defaults = t == "[Default Applications]";
            } else if defaults && t.strip_prefix(mime).is_some_and(|r| r.starts_with('=')) {
                if !done {
                    out.push(entry.clone());
                    done = true;
                }
                continue;
            }
            out.push(line.to_string());
        }
        if !done {
            if !defaults {
                out.push("[Default Applications]".into());
            }
            out.push(entry);
        }
        out.join("\n") + "\n"
    }

    /// A path as one argument of a desktop entry's `Exec`.
    fn exec_arg(path: &Path) -> String {
        let mut s = String::from("\"");
        for c in path.to_string_lossy().chars() {
            match c {
                // Quoting's escape, itself escaped as the entry's text.
                '"' | '`' | '$' => s.push_str(&format!("\\\\{c}")),
                '\\' => s.push_str("\\\\\\\\"),
                c => s.push(c),
            }
        }
        s.push('"');
        s
    }

    /// The program of a desktop entry's `Exec` (its first argument).
    pub(super) fn exec_program(exec: &str) -> String {
        let exec = exec.trim();
        if let Some(rest) = exec.strip_prefix('"') {
            let mut out = String::new();
            let mut chars = rest.chars();
            while let Some(c) = chars.next() {
                match c {
                    '"' => break,
                    '\\' => {
                        // Both levels of escaping.
                        if let Some(n) = chars.next() {
                            if n == '\\' {
                                match chars.next() {
                                    Some('\\') => {
                                        chars.next();
                                        out.push('\\');
                                    }
                                    Some(m) => out.push(m),
                                    None => {}
                                }
                            } else {
                                out.push(n);
                            }
                        }
                    }
                    c => out.push(c),
                }
            }
            out
        } else {
            exec.split_whitespace()
                .next()
                .unwrap_or_default()
                .to_string()
        }
    }

    pub fn opens_project_files(ext: &str) -> Option<bool> {
        let list =
            std::fs::read_to_string(super::config_dir()?.join("mimeapps.list")).unwrap_or_default();
        if default_app(&list, &mime_type(ext)).as_deref() != Some(DESKTOP) {
            return Some(false);
        }
        let entry =
            std::fs::read_to_string(data_home()?.join("applications").join(DESKTOP)).ok()?;
        let exec = entry.lines().find_map(|l| l.strip_prefix("Exec="))?;
        let program = PathBuf::from(exec_program(exec));
        let same = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        Some(launcher().is_some_and(|l| same(&l) == same(&program)))
    }

    /// Text for an XML file.
    fn xml(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }

    pub fn associate_project_files(
        ext: &str,
        prog_id: &str,
        description: &str,
    ) -> Result<(), String> {
        let err = |e: std::io::Error| e.to_string();
        let data = data_home().ok_or("no home folder")?;
        let mime = mime_type(ext);
        // The file type.
        let packages = data.join("mime").join("packages");
        std::fs::create_dir_all(&packages).map_err(err)?;
        let types = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <mime-info xmlns=\"http://www.freedesktop.org/standards/shared-mime-info\">\n\
             \x20 <mime-type type=\"{mime}\">\n\
             \x20   <comment>{}</comment>\n\
             \x20   <glob pattern=\"*.{}\"/>\n\
             \x20 </mime-type>\n\
             </mime-info>\n",
            xml(description),
            xml(ext)
        );
        std::fs::write(packages.join(format!("{prog_id}.xml")), types).map_err(err)?;
        let _ = std::process::Command::new("update-mime-database")
            .arg(data.join("mime"))
            .status();
        // The program that opens it.
        let apps = data.join("applications");
        std::fs::create_dir_all(&apps).map_err(err)?;
        let program = launcher().ok_or("this program's path is unknown")?;
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=BananaV\nComment={description}\n\
             Exec={} %f\nTerminal=false\nCategories=AudioVideo;Video;AudioVideoEditing;\n\
             MimeType={mime};\n",
            exec_arg(&program)
        );
        std::fs::write(apps.join(DESKTOP), entry).map_err(err)?;
        let _ = std::process::Command::new("update-desktop-database")
            .arg(&apps)
            .status();
        // Opened by it by default.
        let config = super::config_dir().ok_or("no configuration folder")?;
        std::fs::create_dir_all(&config).map_err(err)?;
        let list_path = config.join("mimeapps.list");
        let list = std::fs::read_to_string(&list_path).unwrap_or_default();
        std::fs::write(&list_path, with_default(&list, &mime, DESKTOP)).map_err(err)
    }

    /// ASIO is Windows' (Linux has JACK and PipeWire).
    pub fn asio_drivers() -> Vec<String> {
        Vec::new()
    }

    /// Windows Security's.
    pub fn controlled_folder_access() -> bool {
        false
    }

    /// The mounted file systems: device, mount point and type.
    fn mounts() -> Vec<(String, PathBuf, String)> {
        let Ok(text) = std::fs::read_to_string("/proc/self/mounts") else {
            return Vec::new();
        };
        text.lines()
            .filter_map(|l| {
                let mut f = l.split_whitespace();
                let (dev, at, fs) = (f.next()?, f.next()?, f.next()?);
                Some((dev.to_string(), PathBuf::from(unescape(at)), fs.to_string()))
            })
            .collect()
    }

    /// Mount points escape spaces, tabs and backslashes in octal (`\040`).
    fn unescape(s: &str) -> String {
        let b = s.as_bytes();
        let mut out = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            let code = (b[i] == b'\\')
                .then(|| b.get(i + 1..i + 4))
                .flatten()
                .and_then(|d| std::str::from_utf8(d).ok())
                .and_then(|d| u8::from_str_radix(d, 8).ok());
            match code {
                Some(c) => {
                    out.push(c);
                    i += 4;
                }
                None => {
                    out.push(b[i]);
                    i += 1;
                }
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    /// The disk a block device is on (`/dev/sdb1` → `sdb`), by sysfs.
    fn disk_of(dev: &str) -> Option<String> {
        let name = std::fs::canonicalize(dev).ok()?;
        let name = name.file_name()?.to_str()?;
        let real = std::fs::canonicalize(format!("/sys/class/block/{name}")).ok()?;
        if real.join("partition").exists() {
            real.parent()?.file_name()?.to_str().map(String::from)
        } else {
            Some(name.to_string())
        }
    }

    /// A disc drive's disk (`sr0`) or a disc's file system.
    fn optical(disk: &str, fs: &str) -> bool {
        disk.starts_with("sr") || fs == "iso9660" || fs == "udf"
    }

    /// Marked removable by the kernel (cards, sticks), or on USB.
    fn removable_disk(disk: &str) -> bool {
        let flag = std::fs::read_to_string(format!("/sys/block/{disk}/removable"))
            .is_ok_and(|s| s.trim() == "1");
        let usb = std::fs::canonicalize(format!("/sys/block/{disk}"))
            .is_ok_and(|p| p.to_string_lossy().contains("/usb"));
        flag || usb
    }

    /// The volume's name: its link in `/dev/disk/by-label` (escaped as
    /// `\x20`), else the mount point's name.
    fn label_of(dev: &str, at: &Path) -> String {
        let real = std::fs::canonicalize(dev).ok();
        let by_label = std::fs::read_dir("/dev/disk/by-label").ok().and_then(|rd| {
            rd.flatten().find_map(|e| {
                (std::fs::canonicalize(e.path()).ok() == real && real.is_some())
                    .then(|| unhex(&e.file_name().to_string_lossy()))
            })
        });
        by_label.unwrap_or_else(|| {
            at.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
    }

    /// `My\x20Card` → `My Card`.
    fn unhex(s: &str) -> String {
        let b = s.as_bytes();
        let mut out = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            let code = (b[i] == b'\\' && b.get(i + 1) == Some(&b'x'))
                .then(|| b.get(i + 2..i + 4))
                .flatten()
                .and_then(|d| std::str::from_utf8(d).ok())
                .and_then(|d| u8::from_str_radix(d, 16).ok());
            match code {
                Some(c) => {
                    out.push(c);
                    i += 4;
                }
                None => {
                    out.push(b[i]);
                    i += 1;
                }
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    pub fn removable_drives() -> Vec<super::Drive> {
        mounts()
            .into_iter()
            .filter_map(|(dev, at, fs)| {
                let disk = disk_of(&dev)?;
                let optical = optical(&disk, &fs);
                (optical || removable_disk(&disk)).then(|| super::Drive {
                    label: label_of(&dev, &at),
                    root: at,
                    optical,
                })
            })
            .collect()
    }

    pub fn is_removable(path: &Path) -> bool {
        let Ok(path) = std::fs::canonicalize(path) else {
            return false;
        };
        mounts()
            .into_iter()
            .filter(|(_, at, _)| path.starts_with(at))
            .max_by_key(|(_, at, _)| at.as_os_str().len())
            .and_then(|(dev, _, fs)| disk_of(&dev).map(|d| (d, fs)))
            .is_some_and(|(d, fs)| optical(&d, &fs) || removable_disk(&d))
    }

    pub fn free_space(dir: &Path) -> Option<u64> {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
        // SAFETY: a zeroed plain struct for the call to fill.
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: a valid C string and a struct to fill.
        (unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0)
            .then(|| s.f_bavail as u64 * s.f_frsize as u64)
    }

    pub fn total_memory() -> Option<u64> {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb: u64 = text
            .lines()
            .find_map(|l| l.strip_prefix("MemTotal:"))?
            .trim()
            .trim_end_matches("kB")
            .trim()
            .parse()
            .ok()?;
        Some(kb * 1024)
    }

    fn local_tm() -> Option<libc::tm> {
        // SAFETY: the current time, then its fields in the local zone.
        unsafe {
            let t = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            (!libc::localtime_r(&t, &mut tm).is_null()).then_some(tm)
        }
    }

    /// The home trash (`$XDG_DATA_HOME/Trash`): the file renamed into its
    /// `files`, a `.trashinfo` in its `info` saying where it was and when.
    pub fn to_recycle_bin(path: &Path) -> std::io::Result<()> {
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| super::home_dir().map(|h| h.join(".local/share")))
            .ok_or_else(|| std::io::Error::other("no home folder"))?;
        let trash = data.join("Trash");
        let (files, info) = (trash.join("files"), trash.join("info"));
        std::fs::create_dir_all(&files)?;
        std::fs::create_dir_all(&info)?;
        let name = path
            .file_name()
            .ok_or_else(|| std::io::Error::other("nothing to move"))?
            .to_string_lossy()
            .into_owned();
        let date = local_tm().map_or_else(String::new, |t| {
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                t.tm_year + 1900,
                t.tm_mon + 1,
                t.tm_mday,
                t.tm_hour,
                t.tm_min,
                t.tm_sec
            )
        });
        // Its path, percent-encoded but for the separators.
        let mut url = String::new();
        for b in path.as_os_str().as_encoded_bytes() {
            match *b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                    url.push(*b as char)
                }
                b => url.push_str(&format!("%{b:02X}")),
            }
        }
        for n in 1..10_000 {
            let unique = if n == 1 {
                name.clone()
            } else {
                format!("{name}.{n}")
            };
            let note = info.join(format!("{unique}.trashinfo"));
            // Its .trashinfo made first, alone (another program may be
            // trashing a file of the same name).
            let made = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&note);
            let mut f = match made {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            };
            use std::io::Write;
            write!(f, "[Trash Info]\nPath={url}\nDeletionDate={date}\n")?;
            drop(f);
            let to = files.join(&unique);
            if to.exists() {
                let _ = std::fs::remove_file(&note);
                continue;
            }
            return match std::fs::rename(path, &to) {
                Ok(()) => Ok(()),
                Err(e) => {
                    let _ = std::fs::remove_file(&note);
                    Err(e)
                }
            };
        }
        Err(std::io::Error::other(
            "the trash has too many files of that name",
        ))
    }

    pub fn local_time() -> Option<(u32, u32, u32)> {
        let tm = local_tm()?;
        Some((tm.tm_hour as u32, tm.tm_min as u32, tm.tm_sec as u32))
    }

    pub fn utc_offset() -> i64 {
        // `c_long`: 32 bits on 32-bit systems.
        #[allow(clippy::useless_conversion)]
        local_tm().map_or(0, |tm| i64::from(tm.tm_gmtoff))
    }

    /// The first joystick (`/dev/input/js0`): its events read as they come,
    /// without waiting; tried again every two seconds while there is none.
    pub fn joystick() -> Option<(f32, f32)> {
        use std::io::Read;
        use std::os::unix::fs::OpenOptionsExt;
        use std::time::{Duration, Instant};
        struct Pad {
            file: Option<std::fs::File>,
            tried: Option<Instant>,
            x: i16,
            y: i16,
        }
        static PAD: std::sync::Mutex<Pad> = std::sync::Mutex::new(Pad {
            file: None,
            tried: None,
            x: 0,
            y: 0,
        });
        let mut pad = PAD.lock().ok()?;
        if pad.file.is_none() {
            if pad
                .tried
                .is_some_and(|t| t.elapsed() < Duration::from_secs(2))
            {
                return None;
            }
            pad.tried = Some(Instant::now());
            pad.file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open("/dev/input/js0")
                .ok();
            pad.x = 0;
            pad.y = 0;
        }
        let mut event = [0u8; 8];
        loop {
            let read = pad.file.as_mut()?.read(&mut event);
            match read {
                Ok(8) => {
                    // value (i16), type (axis = 2, ignoring "initial"), number.
                    let value = i16::from_le_bytes([event[4], event[5]]);
                    if event[6] & !0x80 == 0x02 {
                        match event[7] {
                            0 => pad.x = value,
                            1 => pad.y = value,
                            _ => {}
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                // Unplugged.
                Err(_) | Ok(_) => {
                    pad.file = None;
                    return None;
                }
            }
        }
        let f = |v: i16| (v as f32 / 32_767.0).clamp(-1.0, 1.0);
        // Its y grows downward.
        Some((f(pad.x), -f(pad.y)))
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    pub fn removable_drives() -> Vec<super::Drive> {
        Vec::new()
    }

    pub fn to_recycle_bin(_path: &std::path::Path) -> std::io::Result<()> {
        Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
    }

    pub fn cursor() -> Option<(i32, i32)> {
        None
    }

    pub fn set_owner(_child: &str, _owner: &str) -> bool {
        true
    }

    pub fn block_input(_keep: &[&str]) -> Vec<isize> {
        Vec::new()
    }

    pub fn unblock_input(_windows: &[isize]) {}

    pub fn key_char(_scancode: u32) -> Option<char> {
        None
    }

    pub fn screen_color(_x: i32, _y: i32) -> Option<[u8; 3]> {
        None
    }

    pub fn numpad_down(_c: char) -> bool {
        false
    }

    pub fn main_button_down() -> bool {
        false
    }

    pub fn capture_mouse(_title: &str) -> bool {
        false
    }

    pub fn release_mouse() {}

    pub fn opens_project_files(_ext: &str) -> Option<bool> {
        None
    }

    pub fn app_is_active() -> Option<bool> {
        None
    }

    pub fn joystick() -> Option<(f32, f32)> {
        None
    }

    pub fn displays() -> Vec<super::Display> {
        Vec::new()
    }

    pub fn asio_drivers() -> Vec<String> {
        Vec::new()
    }

    pub fn is_removable(_path: &std::path::Path) -> bool {
        false
    }

    pub fn associate_project_files(
        _ext: &str,
        _prog_id: &str,
        _description: &str,
    ) -> Result<(), String> {
        Err("not on this system".into())
    }

    pub fn free_space(_dir: &std::path::Path) -> Option<u64> {
        None
    }

    pub fn total_memory() -> Option<u64> {
        None
    }

    pub fn local_time() -> Option<(u32, u32, u32)> {
        None
    }

    pub fn utc_offset() -> i64 {
        0
    }

    pub fn controlled_folder_access() -> bool {
        false
    }
}

pub use bv_core::date::{civil_from_days, days_from_civil};

#[cfg(test)]
mod folder_tests {
    use super::*;

    #[test]
    fn user_dirs_follow_the_system_file() {
        let home = std::path::Path::new("/home/ana");
        let text = "# written by xdg-user-dirs-update\nXDG_DESKTOP_DIR=\"$HOME/Área de trabalho\"\nXDG_VIDEOS_DIR=\"$HOME/Vídeos\"\nXDG_MUSIC_DIR=\"$HOME/\"\nXDG_DOWNLOAD_DIR=\"/data/baixados\"\n";
        assert_eq!(
            user_dirs_entry(text, "XDG_VIDEOS_DIR", home),
            Some(home.join("Vídeos"))
        );
        assert_eq!(
            user_dirs_entry(text, "XDG_DESKTOP_DIR", home),
            Some(home.join("Área de trabalho"))
        );
        assert_eq!(
            user_dirs_entry(text, "XDG_DOWNLOAD_DIR", home),
            Some(std::path::PathBuf::from("/data/baixados"))
        );
        // Turned off (the home folder itself) or missing.
        assert_eq!(user_dirs_entry(text, "XDG_MUSIC_DIR", home), None);
        assert_eq!(user_dirs_entry(text, "XDG_PICTURES_DIR", home), None);
    }

    #[test]
    fn the_settings_folder_is_the_systems() {
        let dir = config_dir().expect("a settings folder");
        assert!(dir.is_absolute());
        if cfg!(windows) {
            assert_eq!(
                Some(dir.as_os_str()),
                std::env::var_os("APPDATA").as_deref()
            );
        }
        assert!(user_dir(UserDir::Videos).is_some_and(|d| d.is_absolute()));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn displays_have_sizes_and_one_is_primary() {
        let all = super::displays();
        if cfg!(windows) {
            assert!(!all.is_empty(), "a display");
            assert_eq!(all.iter().filter(|d| d.primary).count(), 1);
            for d in &all {
                assert!(d.width > 0 && d.height > 0 && d.scale > 0.0, "{d:?}");
                assert!(d.name.starts_with("\\\\.\\"), "{d:?}");
            }
        }
        // ASIO drivers, when there are any, have names.
        assert!(super::asio_drivers().iter().all(|n| !n.is_empty()));
    }

    #[test]
    fn a_joystick_reads_within_its_range() {
        // Most computers have none; one that is there reads −1…1.
        if let Some((x, y)) = super::joystick() {
            assert!((-1.0..=1.0).contains(&x) && (-1.0..=1.0).contains(&y));
        }
    }

    #[test]
    fn cursor_position_is_available_on_windows() {
        let p = super::cursor_screen_position();
        // Headless sessions may have no cursor; the call must not panic.
        let _ = p;
    }

    #[test]
    fn a_temporary_folder_takes_files() {
        let dir = std::env::temp_dir().join("bananav-write-test");
        assert!(super::try_write(&dir).is_ok());
        assert!(
            std::fs::read_dir(&dir).unwrap().next().is_none(),
            "the probe is gone"
        );
        let _ = std::fs::remove_dir(&dir);
        // Reading the protection's setting must not fail.
        let _ = super::controlled_folder_access();
        // A file where a folder should be is not the protection.
        let file = std::env::temp_dir().join("bananav-write-test.txt");
        std::fs::write(&file, b"x").unwrap();
        let e = super::try_write(&file.join("inside")).unwrap_err();
        assert!(!super::blocked_by_folder_protection(
            &file.join("inside"),
            &e
        ));
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn free_space_of_a_missing_folder_uses_its_parent() {
        let here = std::env::temp_dir();
        let missing = here.join("bananav-no-such-folder").join("deeper");
        // The same disk's room, give or take what other tests write to it
        // meanwhile (8 KB apart once).
        match (super::free_space(&missing), super::free_space(&here)) {
            (Some(a), Some(b)) => assert!(a.abs_diff(b) < 256 << 20, "{a} and {b}"),
            (a, b) => assert_eq!(a, b),
        }
        if cfg!(windows) {
            assert!(super::free_space(&here).is_some_and(|b| b > 0));
        }
    }
}

#[cfg(all(test, windows))]
mod association_tests {
    #[test]
    fn an_unknown_file_type_opens_with_nothing() {
        assert_eq!(
            super::opens_project_files("bananav-nothing-opens-this"),
            Some(false)
        );
    }
}

#[cfg(all(test, windows))]
mod drive_tests {
    #[test]
    fn the_system_drive_is_not_removable() {
        let windir =
            std::env::var_os("WINDIR").map_or(r"C:\Windows".into(), std::path::PathBuf::from);
        assert!(!super::is_removable(&windir));
        assert!(!super::is_removable(std::path::Path::new("relative/path")));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod xdg_tests {
    use super::imp::{default_app, exec_program, with_default};

    #[test]
    fn mimeapps_lists_keep_the_rest() {
        let list = "[Added Associations]\ntext/plain=gedit.desktop;\n\n[Default Applications]\nvideo/mp4=vlc.desktop;\napplication/x-bvproj=old.desktop;\n";
        assert_eq!(
            default_app(list, "application/x-bvproj").as_deref(),
            Some("old.desktop")
        );
        let new = with_default(list, "application/x-bvproj", "bananav.desktop");
        assert_eq!(
            default_app(&new, "application/x-bvproj").as_deref(),
            Some("bananav.desktop")
        );
        assert!(new.contains("video/mp4=vlc.desktop;") && new.contains("text/plain=gedit"));
        assert_eq!(new.matches("application/x-bvproj=").count(), 1);
        // Without the section, it is made.
        let made = with_default("", "application/x-bvproj", "bananav.desktop");
        assert_eq!(
            made,
            "[Default Applications]\napplication/x-bvproj=bananav.desktop;\n"
        );
        // Not under Added Associations.
        assert_eq!(
            default_app("[Added Associations]\nx=y.desktop;\n", "x"),
            None
        );
    }

    #[test]
    fn exec_lines_give_their_program() {
        assert_eq!(
            exec_program(r#""/home/a b/BananaV.AppImage" %f"#),
            "/home/a b/BananaV.AppImage"
        );
        assert_eq!(exec_program("/usr/bin/bananav %f"), "/usr/bin/bananav");
        assert_eq!(exec_program(r#""/opt/x\\"y" %f"#), r#"/opt/x"y"#);
        assert_eq!(exec_program(r#""/opt/x\\\\y" %f"#), r"/opt/x\y");
    }
}
