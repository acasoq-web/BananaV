//! BananaV's VST 3 host: modules (bundles or libraries) loaded and their
//! audio effects listed ([`load`], [`Module`]), effects run — buses set for
//! the channels given, blocks processed with parameter changes (ramps too,
//! sample by sample) and the timeline's place and tempo, programs chosen,
//! states read and given back — ([`Instance`]), their parameters and their
//! editors shown in a window the caller gives ([`Editor`]); presets in the
//! standard preset file ([`preset`]) and plug-ins looked at in a process of
//! their own ([`scan`]); a 32-bit module runs in BananaV's 32-bit bridge
//! ([`bridge`]). Calls into a plug-in's code go behind a guard: one
//! that crashes is switched off instead of taking BananaV down
//! ([`take_crashes`]). Written from the VST 3 SDK's interface headers (MIT
//! since VST 3.8: `bv-vst3-sys`), their binary interface by hand; nothing of
//! the SDK's own code.

pub mod bridge;
mod com;
mod guard;
mod host;
mod instance;
mod module;
pub mod preset;
pub mod scan;

pub use bridge::Bridged;
pub use bv_vst3_sys::{PLATFORM_HWND, PLATFORM_NATIVE, PLATFORM_NSVIEW, PLATFORM_X11, restart};
pub use guard::{Crash, take_crashes};
pub use host::{Edit, run_loop};
pub use instance::{Editor, Instance, ParamInfo, Transport};
pub use module::{ClassInfo, Error, Module, is_32bit, library_32, library_of, linked, load};
