//! Calls into plug-ins' code behind a guard, for BananaV's stability with
//! other makers' plug-ins. On Windows a crash in a plug-in — an access
//! violation, a C++ exception it lets out, a stack overflow — comes back as
//! an error instead of ending the program (`guard.c`'s structured exception
//! handling). Its library is then switched off until BananaV starts again
//! (its effects let the sound through untouched), and the crash is kept for
//! the interface to tell ([`take_crashes`]). On macOS the same, through
//! `bv_apple::guard`'s signal handlers. Elsewhere the calls go straight in.

#![allow(unsafe_code)]

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::abi::{Dispatcher, Entry, GetParameter, PluginObject, ProcessF32, SetParameter};

/// A plug-in that crashed (or whose 32-bit bridge ended).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crash {
    /// Its library.
    pub module: PathBuf,
    /// The effect, by its name.
    pub effect: String,
    /// What it was doing.
    pub doing: String,
    /// The exception's code (0: the bridge's process ended).
    pub code: u32,
    /// Where: the library and the offset in it of the instruction that
    /// crashed, or its address.
    pub at: String,
}

static CRASHES: Mutex<Vec<Crash>> = Mutex::new(Vec::new());

/// The crashes since this was last asked.
pub fn take_crashes() -> Vec<Crash> {
    std::mem::take(&mut *CRASHES.lock().unwrap_or_else(|e| e.into_inner()))
}

pub(crate) fn note(crash: Crash) {
    eprintln!(
        "vst2: {} ({}) crashed in {} (exception {:#010x} at {}): switched off until BananaV restarts",
        crash.module.display(),
        crash.effect,
        crash.doing,
        crash.code,
        crash.at
    );
    CRASHES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(crash);
}

/// What a guarded call gave: its result, or the code and the address of
/// the exception it crashed with.
pub(crate) type Outcome<T> = Result<T, (u32, *mut c_void)>;

#[cfg(all(bv_vst2_guard, windows))]
unsafe extern "C" {
    fn bv_vst2_dispatch(
        f: Dispatcher,
        plugin: *mut PluginObject,
        request: i32,
        index: i32,
        value: isize,
        ptr: *mut c_void,
        number: f32,
        result: *mut isize,
        address: *mut *mut c_void,
    ) -> std::ffi::c_ulong;
    fn bv_vst2_process(
        f: ProcessF32,
        plugin: *mut PluginObject,
        inputs: *mut *mut f32,
        outputs: *mut *mut f32,
        frames: i32,
        address: *mut *mut c_void,
    ) -> std::ffi::c_ulong;
    fn bv_vst2_set(
        f: SetParameter,
        plugin: *mut PluginObject,
        index: i32,
        value: f32,
        address: *mut *mut c_void,
    ) -> std::ffi::c_ulong;
    fn bv_vst2_get(
        f: GetParameter,
        plugin: *mut PluginObject,
        index: i32,
        result: *mut f32,
        address: *mut *mut c_void,
    ) -> std::ffi::c_ulong;
    fn bv_vst2_entry(
        f: Entry,
        host: crate::abi::HostCallback,
        result: *mut *mut PluginObject,
        address: *mut *mut c_void,
    ) -> std::ffi::c_ulong;
}

/// macOS: the guard's calls as Windows' `guard.c` has them, through
/// `bv_apple::guard` (the bodies hold nothing to drop).
#[cfg(all(bv_vst2_guard, target_os = "macos"))]
mod mac {
    use std::ffi::{c_ulong, c_void};

    use crate::abi::{Dispatcher, Entry, GetParameter, PluginObject, ProcessF32, SetParameter};

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

    #[allow(clippy::too_many_arguments)]
    pub(super) unsafe fn bv_vst2_dispatch(
        f: Dispatcher,
        plugin: *mut PluginObject,
        request: i32,
        index: i32,
        value: isize,
        ptr: *mut c_void,
        number: f32,
        result: *mut isize,
        address: *mut *mut c_void,
    ) -> c_ulong {
        // SAFETY: as the guard's caller promises.
        let outcome = unsafe {
            bv_apple::guard::run(&mut || *result = f(plugin, request, index, value, ptr, number))
        };
        code(outcome, address)
    }

    pub(super) unsafe fn bv_vst2_process(
        f: ProcessF32,
        plugin: *mut PluginObject,
        inputs: *mut *mut f32,
        outputs: *mut *mut f32,
        frames: i32,
        address: *mut *mut c_void,
    ) -> c_ulong {
        // SAFETY: as the guard's caller promises.
        let outcome = unsafe { bv_apple::guard::run(&mut || f(plugin, inputs, outputs, frames)) };
        code(outcome, address)
    }

    pub(super) unsafe fn bv_vst2_set(
        f: SetParameter,
        plugin: *mut PluginObject,
        index: i32,
        value: f32,
        address: *mut *mut c_void,
    ) -> c_ulong {
        // SAFETY: as the guard's caller promises.
        let outcome = unsafe { bv_apple::guard::run(&mut || f(plugin, index, value)) };
        code(outcome, address)
    }

    pub(super) unsafe fn bv_vst2_get(
        f: GetParameter,
        plugin: *mut PluginObject,
        index: i32,
        result: *mut f32,
        address: *mut *mut c_void,
    ) -> c_ulong {
        // SAFETY: as the guard's caller promises.
        let outcome = unsafe { bv_apple::guard::run(&mut || *result = f(plugin, index)) };
        code(outcome, address)
    }

    pub(super) unsafe fn bv_vst2_entry(
        f: Entry,
        host: crate::abi::HostCallback,
        result: *mut *mut PluginObject,
        address: *mut *mut c_void,
    ) -> c_ulong {
        // SAFETY: as the guard's caller promises.
        let outcome = unsafe { bv_apple::guard::run(&mut || *result = f(host)) };
        code(outcome, address)
    }
}

#[cfg(all(bv_vst2_guard, target_os = "macos"))]
use mac::{bv_vst2_dispatch, bv_vst2_entry, bv_vst2_get, bv_vst2_process, bv_vst2_set};

/// The code a guard gave back as an outcome.
#[cfg(bv_vst2_guard)]
#[allow(clippy::unnecessary_cast)] // `c_ulong`: 32 bits on Windows, 64 on macOS
fn outcome<T>(code: std::ffi::c_ulong, value: T, address: *mut c_void) -> Outcome<T> {
    if code == 0 {
        Ok(value)
    } else {
        Err((code as u32, address))
    }
}

/// The plug-in's dispatcher `f` with a request.
///
/// # Safety
/// `f` is `plugin`'s dispatcher; `ptr` what the request takes.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::let_and_return)] // a binding for each system
pub(crate) unsafe fn dispatch(
    f: Dispatcher,
    plugin: *mut PluginObject,
    request: i32,
    index: i32,
    value: isize,
    ptr: *mut c_void,
    number: f32,
) -> Outcome<isize> {
    #[cfg(bv_vst2_guard)]
    let outcome = {
        let (mut r, mut address) = (0, std::ptr::null_mut());
        // SAFETY: as the caller promises; the guard only adds a handler.
        let code = unsafe {
            bv_vst2_dispatch(
                f,
                plugin,
                request,
                index,
                value,
                ptr,
                number,
                &mut r,
                &mut address,
            )
        };
        outcome(code, r, address)
    };
    #[cfg(not(bv_vst2_guard))]
    // SAFETY: as the caller promises.
    let outcome = Ok(unsafe { f(plugin, request, index, value, ptr, number) });
    outcome
}

/// The plug-in's processing `f` over `frames` samples.
///
/// # Safety
/// `f` is `plugin`'s; the buffers hold `frames` samples for each of its
/// inputs and outputs.
#[allow(clippy::let_and_return)] // a binding for each system
pub(crate) unsafe fn process(
    f: ProcessF32,
    plugin: *mut PluginObject,
    inputs: *mut *mut f32,
    outputs: *mut *mut f32,
    frames: i32,
) -> Outcome<()> {
    #[cfg(bv_vst2_guard)]
    let outcome = {
        let mut address = std::ptr::null_mut();
        // SAFETY: as the caller promises; the guard only adds a handler.
        let code = unsafe { bv_vst2_process(f, plugin, inputs, outputs, frames, &mut address) };
        outcome(code, (), address)
    };
    #[cfg(not(bv_vst2_guard))]
    let outcome = {
        // SAFETY: as the caller promises.
        unsafe { f(plugin, inputs, outputs, frames) };
        Ok(())
    };
    outcome
}

/// The plug-in's parameter setter `f`.
///
/// # Safety
/// `f` is `plugin`'s.
#[allow(clippy::let_and_return)] // a binding for each system
pub(crate) unsafe fn set(
    f: SetParameter,
    plugin: *mut PluginObject,
    index: i32,
    value: f32,
) -> Outcome<()> {
    #[cfg(bv_vst2_guard)]
    let outcome = {
        let mut address = std::ptr::null_mut();
        // SAFETY: as the caller promises; the guard only adds a handler.
        let code = unsafe { bv_vst2_set(f, plugin, index, value, &mut address) };
        outcome(code, (), address)
    };
    #[cfg(not(bv_vst2_guard))]
    let outcome = {
        // SAFETY: as the caller promises.
        unsafe { f(plugin, index, value) };
        Ok(())
    };
    outcome
}

/// The plug-in's parameter getter `f`.
///
/// # Safety
/// `f` is `plugin`'s.
#[allow(clippy::let_and_return)] // a binding for each system
pub(crate) unsafe fn get(f: GetParameter, plugin: *mut PluginObject, index: i32) -> Outcome<f32> {
    #[cfg(bv_vst2_guard)]
    let outcome = {
        let (mut r, mut address) = (0.0, std::ptr::null_mut());
        // SAFETY: as the caller promises; the guard only adds a handler.
        let code = unsafe { bv_vst2_get(f, plugin, index, &mut r, &mut address) };
        outcome(code, r, address)
    };
    #[cfg(not(bv_vst2_guard))]
    // SAFETY: as the caller promises.
    let outcome = Ok(unsafe { f(plugin, index) });
    outcome
}

/// The library's entry point `f` with the host's callback.
///
/// # Safety
/// `f` is a plug-in library's entry point.
#[allow(clippy::let_and_return)] // a binding for each system
pub(crate) unsafe fn entry(f: Entry, host: crate::abi::HostCallback) -> Outcome<*mut PluginObject> {
    #[cfg(bv_vst2_guard)]
    let outcome = {
        let (mut r, mut address) = (std::ptr::null_mut(), std::ptr::null_mut());
        // SAFETY: as the caller promises; the guard only adds a handler.
        let code = unsafe { bv_vst2_entry(f, host, &mut r, &mut address) };
        outcome(code, r, address)
    };
    #[cfg(not(bv_vst2_guard))]
    // SAFETY: as the caller promises.
    let outcome = Ok(unsafe { f(host) });
    outcome
}

/// Where `address` is: the library it is in and the offset in it.
#[cfg(windows)]
pub(crate) fn place(address: *mut c_void) -> String {
    const FROM_ADDRESS: u32 = 0x4;
    const UNCHANGED_REFCOUNT: u32 = 0x2;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleExW(flags: u32, name: *const c_void, module: *mut *mut c_void) -> i32;
        fn GetModuleFileNameW(module: *mut c_void, name: *mut u16, size: u32) -> u32;
    }
    let mut module = std::ptr::null_mut();
    // SAFETY: a lookup of the library holding an address, its count of
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
pub(crate) fn place(address: *mut c_void) -> String {
    bv_apple::guard::place(address as usize)
}

#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn place(address: *mut c_void) -> String {
    format!("{address:p}")
}
