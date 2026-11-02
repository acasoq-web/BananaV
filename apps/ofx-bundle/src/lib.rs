//! BananaV's effects as an OpenFX binary, for other OFX hosts (DaVinci
//! Resolve, Natron, Nuke…): the same plug-ins BananaV has built in.
//! `cargo build --release -p bananav-ofx` makes `bananav_ofx.dll`
//! (`libbananav_ofx.so` on Linux); copied as
//! `BananaV.ofx.bundle/Contents/Win64/BananaV.ofx` (`Linux-x86-64`…) into
//! an OFX plug-in folder, hosts find it.

use std::ffi::c_int;

use bv_ofx_sys::OfxPlugin;

/// The specification's `OfxGetNumberOfPlugins`.
///
/// # Safety
/// Called by hosts, as the specification says.
#[allow(unsafe_code, non_snake_case)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn OfxGetNumberOfPlugins() -> c_int {
    // SAFETY: as the host calls this one.
    unsafe { bv_effects::number_of_plugins() }
}

/// The specification's `OfxGetPlugin`.
///
/// # Safety
/// Called by hosts, as the specification says.
#[allow(unsafe_code, non_snake_case)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn OfxGetPlugin(nth: c_int) -> *mut OfxPlugin {
    // SAFETY: as the host calls this one.
    unsafe { bv_effects::get_plugin(nth) }
}
