//! What BananaV's plug-in hosts (`bv-vst3`, `bv-vst2`, `bv-ofx`) and its
//! OS services (`bv-platform`) share on macOS.
//!
//! - [`macho`]: Mach-O files — macOS's programs, libraries and plug-ins —
//!   read without running them: the processors a file holds code for (a
//!   universal file holds several) and the functions it exports. Plain
//!   reading of bytes: on every system, and tested on every one.
//! - [`cf`] (macOS): Core Foundation's strings, numbers, booleans,
//!   dictionaries and file URLs, owned and released.
//! - [`Bundle`] (macOS): a bundle — a plug-in's folder (`Name.vst3`,
//!   `Name.vst`, `Name.ofx.bundle`) or the application's —: its
//!   executable, its Info.plist's values, loading it and finding its
//!   functions.
//! - [`on_main`] (macOS): work run on the main thread, where AppKit wants
//!   its windows and views touched.
//! - [`guard`] (macOS): calls into plug-ins behind a guard (signals), a
//!   crash coming back as an error.

pub mod macho;

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub mod cf;

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod bundle;

#[cfg(target_os = "macos")]
pub use bundle::Bundle;

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub mod guard;

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod main_thread;

#[cfg(target_os = "macos")]
pub use main_thread::{is_main_thread, on_main};
