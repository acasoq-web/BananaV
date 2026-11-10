//! Calls into plug-ins behind a guard (macOS): a crash in a plug-in comes
//! back as an error — the signal and the address of the instruction — and
//! the host switches the plug-in off, as `guard.c`'s structured exception
//! handling does on Windows. `guard.c` here keeps a jump point per thread
//! and signal handlers that go back to it; [`install`] puts the handlers
//! in before a plug-in's code is loaded.

use std::ffi::{c_char, c_int, c_ulong, c_void};

#[cfg(bv_apple_guard)]
unsafe extern "C" {
    fn bv_apple_guard_install();
    fn bv_apple_guarded(
        body: unsafe extern "C" fn(*mut c_void),
        context: *mut c_void,
        address: *mut *mut c_void,
    ) -> c_ulong;
}

/// Puts the guard's signal handlers in (once; before plug-ins load, so
/// that those installing their own are asked first).
pub fn install() {
    // SAFETY: a plain call, done once.
    #[cfg(bv_apple_guard)]
    unsafe {
        bv_apple_guard_install()
    };
}

/// Runs `body` behind the guard: `Err((signal, address))` when it crashed.
///
/// # Safety
/// `body` calls a plug-in as its maker asks, and holds nothing that must be
/// dropped: when the plug-in crashes, the guard goes back past it without
/// unwinding.
pub unsafe fn run(body: &mut dyn FnMut()) -> Result<(), (u32, *mut c_void)> {
    #[cfg(bv_apple_guard)]
    {
        unsafe extern "C" fn call(context: *mut c_void) {
            // SAFETY: the body `run` passes, alive during the call.
            let body = unsafe { &mut *context.cast::<&mut dyn FnMut()>() };
            body();
        }
        let mut body = body;
        let mut address = std::ptr::null_mut();
        // SAFETY: the body, called once on this thread; the guard only adds
        // a jump point.
        let code = unsafe { bv_apple_guarded(call, (&raw mut body).cast(), &mut address) };
        if code == 0 {
            Ok(())
        } else {
            Err((code as u32, address))
        }
    }
    #[cfg(not(bv_apple_guard))]
    {
        body();
        Ok(())
    }
}

#[repr(C)]
struct DlInfo {
    fname: *const c_char,
    fbase: *mut c_void,
    sname: *const c_char,
    saddr: *mut c_void,
}

unsafe extern "C" {
    fn dladdr(address: *const c_void, info: *mut DlInfo) -> c_int;
}

/// Where `address` is: the library it is in and the offset in it
/// ("Plugs+0x1a2b3c"), else the address.
pub fn place(address: usize) -> String {
    let address = address as *const c_void;
    let mut info = DlInfo {
        fname: std::ptr::null(),
        fbase: std::ptr::null_mut(),
        sname: std::ptr::null(),
        saddr: std::ptr::null_mut(),
    };
    // SAFETY: a lookup of the library holding an address; its name is the
    // system's, alive while the library is loaded.
    unsafe {
        if address.is_null() || dladdr(address, &mut info) == 0 || info.fname.is_null() {
            return format!("{address:p}");
        }
        let name = std::ffi::CStr::from_ptr(info.fname).to_string_lossy();
        let file = name.rsplit('/').next().unwrap_or(&name).to_string();
        format!("{file}+{:#x}", address as usize - info.fbase as usize)
    }
}

/// A signal's name, as a crash report gives it ("SIGSEGV").
pub fn signal_name(signal: u32) -> &'static str {
    match signal {
        4 => "SIGILL",
        5 => "SIGTRAP",
        6 => "SIGABRT",
        8 => "SIGFPE",
        10 => "SIGBUS",
        11 => "SIGSEGV",
        _ => "signal",
    }
}
