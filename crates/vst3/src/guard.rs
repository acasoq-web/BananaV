//! Calls into plug-ins' code behind a guard, for BananaV's stability with
//! other makers' plug-ins. On Windows a crash in a plug-in — an access
//! violation, a C++ exception it lets out, a stack overflow — comes back as
//! an error instead of ending the program (`guard.c`'s structured exception
//! handling). Its module is then switched off until BananaV starts again
//! (what crashed may have left it broken: its effects let the sound through
//! untouched), and the crash is kept for the interface to tell
//! ([`take_crashes`]). On macOS the same, through `bv_apple::guard`'s
//! signal handlers. Elsewhere the calls go straight in.
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::Mutex;

use bv_vst3_sys::{Raw, TResult};

use crate::module::Module;

/// A plug-in that crashed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crash {
    /// Its module (a bundle or a library).
    pub module: PathBuf,
    /// The effect, by its name.
    pub effect: String,
    /// What it was doing: the function of the plug-in's called.
    pub doing: String,
    /// The exception's code.
    pub code: u32,
    /// Where: the module and the offset in it of the instruction that
    /// crashed ("Plugs.vst3+0x1a2b3c"), or its address.
    pub at: String,
}

static CRASHES: Mutex<Vec<Crash>> = Mutex::new(Vec::new());

/// The crashes since this was last asked.
pub fn take_crashes() -> Vec<Crash> {
    std::mem::take(&mut *CRASHES.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Notes `crash` (one in the 32-bit bridge) for the interface to tell.
pub(crate) fn note(crash: Crash) {
    eprintln!(
        "vst3: {} ({}) crashed in {} (exception {:#010x} at {}): switched off until BananaV restarts",
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

/// What a guarded call gave: the plug-in's result, or the code and the
/// address of the exception it crashed with.
pub(crate) type Outcome = Result<TResult, (u32, *mut c_void)>;

type Pp = unsafe extern "system" fn(Raw, Raw) -> TResult;
type Pb = unsafe extern "system" fn(Raw, u8) -> TResult;
type P = unsafe extern "system" fn(Raw) -> TResult;

#[cfg(all(bv_vst3_guard, windows))]
unsafe extern "C" {
    fn bv_vst3_guard_pp(
        f: Pp,
        this: Raw,
        arg: Raw,
        result: *mut TResult,
        address: *mut *mut c_void,
    ) -> std::ffi::c_ulong;
    fn bv_vst3_guard_pb(
        f: Pb,
        this: Raw,
        arg: u8,
        result: *mut TResult,
        address: *mut *mut c_void,
    ) -> std::ffi::c_ulong;
    fn bv_vst3_guard_p(
        f: P,
        this: Raw,
        result: *mut TResult,
        address: *mut *mut c_void,
    ) -> std::ffi::c_ulong;
}

/// macOS: the guard's calls as Windows' `guard.c` has them, through
/// `bv_apple::guard` (the bodies hold nothing to drop).
#[cfg(all(bv_vst3_guard, target_os = "macos"))]
mod mac {
    use super::{P, Pb, Pp, Raw, TResult};
    use std::ffi::{c_ulong, c_void};

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

    pub(super) unsafe fn bv_vst3_guard_pp(
        f: Pp,
        this: Raw,
        arg: Raw,
        result: *mut TResult,
        address: *mut *mut c_void,
    ) -> c_ulong {
        // SAFETY: as the guard's caller promises.
        code(
            unsafe { bv_apple::guard::run(&mut || *result = f(this, arg)) },
            address,
        )
    }

    pub(super) unsafe fn bv_vst3_guard_pb(
        f: Pb,
        this: Raw,
        arg: u8,
        result: *mut TResult,
        address: *mut *mut c_void,
    ) -> c_ulong {
        // SAFETY: as the guard's caller promises.
        code(
            unsafe { bv_apple::guard::run(&mut || *result = f(this, arg)) },
            address,
        )
    }

    pub(super) unsafe fn bv_vst3_guard_p(
        f: P,
        this: Raw,
        result: *mut TResult,
        address: *mut *mut c_void,
    ) -> c_ulong {
        // SAFETY: as the guard's caller promises.
        code(
            unsafe { bv_apple::guard::run(&mut || *result = f(this)) },
            address,
        )
    }
}

#[cfg(all(bv_vst3_guard, target_os = "macos"))]
use mac::{bv_vst3_guard_p, bv_vst3_guard_pb, bv_vst3_guard_pp};

/// `f(this, arg)`, a plug-in's function of an object and a pointer.
///
/// # Safety
/// `f` is `this`'s function, `arg` what it takes.
#[allow(clippy::let_and_return)] // a binding for each system
pub(crate) unsafe fn pp<A>(
    f: unsafe extern "system" fn(Raw, *mut A) -> TResult,
    this: Raw,
    arg: *mut A,
) -> Outcome {
    // SAFETY: the same function: its second argument is a pointer either
    // way (one calling convention on these systems).
    let f: Pp =
        unsafe { std::mem::transmute::<unsafe extern "system" fn(Raw, *mut A) -> TResult, Pp>(f) };
    #[cfg(bv_vst3_guard)]
    let outcome = {
        let (mut r, mut address) = (0, std::ptr::null_mut());
        // SAFETY: as the caller promises; the guard only adds a handler.
        let code = unsafe { bv_vst3_guard_pp(f, this, arg.cast(), &mut r, &mut address) };
        if code == 0 {
            Ok(r)
        } else {
            Err((code as u32, address))
        }
    };
    #[cfg(not(bv_vst3_guard))]
    // SAFETY: as the caller promises.
    let outcome = Ok(unsafe { f(this, arg.cast()) });
    outcome
}

/// `f(this, arg)`, a plug-in's function of an object and a byte.
///
/// # Safety
/// `f` is `this`'s function.
#[allow(clippy::let_and_return)] // a binding for each system
pub(crate) unsafe fn pb(f: Pb, this: Raw, arg: u8) -> Outcome {
    #[cfg(bv_vst3_guard)]
    let outcome = {
        let (mut r, mut address) = (0, std::ptr::null_mut());
        // SAFETY: as the caller promises; the guard only adds a handler.
        let code = unsafe { bv_vst3_guard_pb(f, this, arg, &mut r, &mut address) };
        if code == 0 {
            Ok(r)
        } else {
            Err((code as u32, address))
        }
    };
    #[cfg(not(bv_vst3_guard))]
    // SAFETY: as the caller promises.
    let outcome = Ok(unsafe { f(this, arg) });
    outcome
}

/// `f(this)`, a plug-in's function of its object alone.
///
/// # Safety
/// `f` is `this`'s function.
#[allow(clippy::let_and_return)] // a binding for each system
pub(crate) unsafe fn p(f: P, this: Raw) -> Outcome {
    #[cfg(bv_vst3_guard)]
    let outcome = {
        let (mut r, mut address) = (0, std::ptr::null_mut());
        // SAFETY: as the caller promises; the guard only adds a handler.
        let code = unsafe { bv_vst3_guard_p(f, this, &mut r, &mut address) };
        if code == 0 {
            Ok(r)
        } else {
            Err((code as u32, address))
        }
    };
    #[cfg(not(bv_vst3_guard))]
    // SAFETY: as the caller promises.
    let outcome = Ok(unsafe { f(this) });
    outcome
}

/// `module`'s effect `effect` crashed doing `doing`: the module switched
/// off, the crash noted.
pub(crate) fn crashed(
    module: &Module,
    effect: &str,
    doing: &'static str,
    code: u32,
    address: *mut c_void,
) {
    module.switch_off();
    let at = place(address);
    eprintln!(
        "vst3: {} ({effect}) crashed in {doing} (exception {code:#010x} at {at}): switched off until BananaV restarts",
        module.path().display()
    );
    CRASHES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(Crash {
            module: module.path().to_path_buf(),
            effect: effect.to_string(),
            doing: doing.to_string(),
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
