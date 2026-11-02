//! BananaV's VST 3 host: modules (bundles or libraries) loaded and their
//! audio effects listed ([`load`], [`Module`]), effects run — buses set for
//! the channels given, blocks processed with parameter changes, states read
//! and given back — ([`Instance`]), their parameters and their editors
//! shown in a window the caller gives ([`Editor`]). Written from the VST 3
//! SDK's interface headers (MIT since VST 3.8: `bv-vst3-sys`), their
//! binary interface by hand; nothing of the SDK's own code.

mod com;
mod host;
mod instance;
mod module;

pub use bv_vst3_sys::{PLATFORM_HWND, PLATFORM_X11, restart};
pub use host::{Edit, run_loop};
pub use instance::{Editor, Instance, ParamInfo};
pub use module::{ClassInfo, Error, Module, library_of, linked, load};
