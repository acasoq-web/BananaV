//! Calls into plug-ins' code behind a guard, for BananaV's stability with
//! other makers' plug-ins. On Windows a crash in a plug-in — an access
//! violation, a C++ exception it lets out, a stack overflow — comes back as
//! an error instead of ending the program (`guard.c`'s structured exception
//! handling). Its binary is then switched off until BananaV starts again
//! (what crashed may have left it broken: its other calls answer
//! `kOfxStatFailed`), and the crash is kept for the interface to tell
//! ([`take_crashes`]). On macOS the same, through `bv_apple::guard`'s
//! signal handlers. Elsewhere the calls go straight in.
#![allow(unsafe_code)]

use std::cell::RefCell;
use std::ffi::{CStr, c_uint, c_void};
use std::sync::{Arc, Mutex};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::plugin::Binary;

/// A plug-in that crashed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crash {
    /// Its binary's file.
    pub binary: std::path::PathBuf,
    /// What it was doing: the action, or a thread of the multi-thread
    /// suite.
    pub doing: String,
    /// The exception's code.
    pub code: u32,
    /// Where: the module and the offset in it of the instruction that
    /// crashed ("Sapphire.ofx+0x1a2b3c"), or its address.
    pub at: String,
}

static CRASHES: Mutex<Vec<Crash>> = Mutex::new(Vec::new());

/// The crashes since this was last asked.
pub fn take_crashes() -> Vec<Crash> {
    std::mem::take(&mut *CRASHES.lock().unwrap_or_else(|e| e.into_inner()))
}

thread_local! {
    /// The binaries whose code this thread is in, the innermost last: the
    /// multi-thread suite's threads run it on their behalf.
    static RUNNING: RefCell<Vec<Arc<Binary>>> = const { RefCell::new(Vec::new()) };
}

/// The binary whose code this thread is in.
pub(crate) fn running() -> Option<Arc<Binary>> {
    RUNNING.with_borrow(|r| r.last().cloned())
}

#[cfg(all(bv_ofx_guard, windows))]
unsafe extern "C" {
    fn bv_ofx_guarded_entry(
        f: OfxPluginEntryPoint,
        action: *const std::ffi::c_char,
        handle: *const c_void,
        in_args: OfxPropertySetHandle,
        out_args: OfxPropertySetHandle,
        status: *mut OfxStatus,
        address: *mut *mut c_void,
    ) -> std::ffi::c_ulong;
    fn bv_ofx_guarded_thread(
        f: OfxThreadFunctionV1,
        index: c_uint,
        max: c_uint,
        arg: *mut c_void,
        address: *mut *mut c_void,
    ) -> std::ffi::c_ulong;
}

/// macOS: the guard's calls as Windows' `guard.c` has them, through
/// `bv_apple::guard` (the bodies hold nothing to drop).
#[cfg(all(bv_ofx_guard, target_os = "macos"))]
mod mac {
    use std::ffi::{c_uint, c_ulong, c_void};

    use bv_ofx_sys::*;

    fn code(outcome: Result<(), (u32, *mut c_void)>, address: *mut *mut c_void) -> c_ulong {
        match outcome {
            Ok(()) => 0,
            Err((signal, at)) => {
                // SAFETY: the caller's place for the address.
                unsafe { *address = at };
                c_ulong::from(signal)
            }
        }
    }

    pub(super) unsafe fn bv_ofx_guarded_entry(
        f: OfxPluginEntryPoint,
        action: *const std::ffi::c_char,
        handle: *const c_void,
        in_args: OfxPropertySetHandle,
        out_args: OfxPropertySetHandle,
        status: *mut OfxStatus,
        address: *mut *mut c_void,
    ) -> c_ulong {
        // SAFETY: as the guard's caller promises.
        let outcome =
            unsafe { bv_apple::guard::run(&mut || *status = f(action, handle, in_args, out_args)) };
        code(outcome, address)
    }

    pub(super) unsafe fn bv_ofx_guarded_thread(
        f: OfxThreadFunctionV1,
        index: c_uint,
        max: c_uint,
        arg: *mut c_void,
        address: *mut *mut c_void,
    ) -> c_ulong {
        // SAFETY: as the guard's caller promises.
        let outcome = unsafe { bv_apple::guard::run(&mut || f(index, max, arg)) };
        code(outcome, address)
    }
}

#[cfg(all(bv_ofx_guard, target_os = "macos"))]
use mac::{bv_ofx_guarded_entry, bv_ofx_guarded_thread};

/// Runs `binary`'s entry point `f` for `action`: its status, or
/// `kOfxStatFailed` when the binary is switched off or crashes now.
///
/// # Safety
/// `f` is the binary's entry point; `handle` and the arguments are what
/// the action takes.
pub(crate) unsafe fn entry(
    binary: &Arc<Binary>,
    f: OfxPluginEntryPoint,
    action: &CStr,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if binary.crashed() {
        return kOfxStatFailed;
    }
    RUNNING.with_borrow_mut(|r| r.push(binary.clone()));
    #[cfg(bv_ofx_guard)]
    let outcome = {
        let mut status = kOfxStatFailed;
        let mut address = std::ptr::null_mut();
        // SAFETY: as the caller promises; the guard only adds a handler.
        let code = unsafe {
            bv_ofx_guarded_entry(
                f,
                action.as_ptr(),
                handle,
                in_args,
                out_args,
                &mut status,
                &mut address,
            )
        };
        if code == 0 {
            Ok(status)
        } else {
            Err((code as u32, address))
        }
    };
    #[cfg(not(bv_ofx_guard))]
    // SAFETY: as the caller promises.
    let outcome: Result<OfxStatus, (u32, *mut c_void)> =
        Ok(unsafe { f(action.as_ptr(), handle, in_args, out_args) });
    RUNNING.with_borrow_mut(|r| {
        r.pop();
    });
    match outcome {
        Ok(status) => status,
        Err((code, address)) => {
            crashed(binary, action.to_string_lossy().into_owned(), code, address);
            kOfxStatFailed
        }
    }
}

/// Runs a thread function `f` a plug-in gave the multi-thread suite, on
/// behalf of `binary` (when known): whether it ran through.
///
/// # Safety
/// `f` is the plug-in's thread function, `arg` its argument.
pub(crate) unsafe fn thread(
    binary: Option<&Arc<Binary>>,
    f: OfxThreadFunctionV1,
    index: c_uint,
    max: c_uint,
    arg: *mut c_void,
) -> bool {
    if binary.is_some_and(|b| b.crashed()) {
        return false;
    }
    if let Some(b) = binary {
        RUNNING.with_borrow_mut(|r| r.push(b.clone()));
    }
    #[cfg(bv_ofx_guard)]
    let (code, address) = {
        let mut address = std::ptr::null_mut();
        // SAFETY: as the caller promises; the guard only adds a handler.
        let code = unsafe { bv_ofx_guarded_thread(f, index, max, arg, &mut address) };
        (code as u32, address)
    };
    #[cfg(not(bv_ofx_guard))]
    let (code, address) = {
        // SAFETY: as the caller promises.
        unsafe { f(index, max, arg) };
        (0u32, std::ptr::null_mut())
    };
    if binary.is_some() {
        RUNNING.with_borrow_mut(|r| {
            r.pop();
        });
    }
    if code == 0 {
        return true;
    }
    if let Some(b) = binary {
        crashed(b, format!("thread {index} of {max}"), code, address);
    }
    false
}

/// `binary` crashed doing `doing`: switched off, the crash noted.
fn crashed(binary: &Binary, doing: String, code: u32, address: *mut c_void) {
    binary.switch_off();
    let at = place(address);
    eprintln!(
        "ofx: {} crashed ({doing}, exception {code:#010x} at {at}): switched off until BananaV restarts",
        binary.path.display()
    );
    CRASHES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(Crash {
            binary: binary.path.clone(),
            doing,
            code,
            at,
        });
}

/// Where `address` is: the module it is in and the offset in it, to tell
/// whose code crashed (the plug-in's, or code it called).
#[cfg(windows)]
fn place(address: *mut c_void) -> String {
    const FROM_ADDRESS: u32 = 0x4;
    const UNCHANGED_REFCOUNT: u32 = 0x2;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleExW(flags: u32, name: *const c_void, module: *mut *mut c_void) -> i32;
        fn GetModuleFileNameW(module: *mut c_void, name: *mut u16, size: u32) -> u32;
    }
    let mut module = std::ptr::null_mut();
    // SAFETY: a lookup of the module holding an address, its count of
    // references unchanged; then its file's name into a buffer of the
    // length given.
    let name = unsafe {
        if GetModuleHandleExW(FROM_ADDRESS | UNCHANGED_REFCOUNT, address, &mut module) == 0 {
            return format!("{address:p}");
        }
        let mut buf = [0u16; 512];
        let n = GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) as usize;
        String::from_utf16_lossy(&buf[..n.min(buf.len())])
    };
    let file = name.rsplit(['\\', '/']).next().unwrap_or(&name).to_string();
    format!("{file}+{:#x}", address as usize - module as usize)
}

#[cfg(target_os = "macos")]
fn place(address: *mut c_void) -> String {
    bv_apple::guard::place(address as usize)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn place(address: *mut c_void) -> String {
    format!("{address:p}")
}
