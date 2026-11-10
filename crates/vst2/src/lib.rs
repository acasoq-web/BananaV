//! BananaV's own host for VST 2 plug-ins. Steinberg stopped licensing VST
//! 2 in 2018; BananaV uses none of its code or headers: the plug-ins'
//! binary interface is declared from how plug-ins behave ([`abi`]), in
//! BananaV's own words, for compatibility with the plug-ins people own.
//!
//! - [`Instance`]: a plug-in running — in this process when its library is
//!   made for this processor, behind a crash guard (`guard.c`); in
//!   BananaV's 32-bit bridge ([`bridge`], `bananav-bridge32.exe`) when it
//!   is a 32-bit Windows library, where a crash or a hang costs the
//!   bridge's process only (`bv-bridge`).
//! - [`scan`]: libraries looked at in a process of their own, cached.
//! - [`preset`]: the `.fxp` and `.fxb` preset files.

mod guard;
mod host;
mod instance;
mod local;
mod module;

pub mod bridge;
pub mod preset;
pub mod scan;

pub use bv_vst2_sys as abi;
pub use guard::{Crash, take_crashes};
pub use host::{Edit, Resize};
pub use instance::Instance;
pub use local::Local;
pub use module::{Error, Machine, Module, bundle_executable, linked, load, machine, this_machine};

/// What a plug-in is.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Description {
    pub name: String,
    pub vendor: String,
    pub product: String,
    pub version: i64,
    /// The number it is known by.
    pub unique_id: i32,
    pub category: i32,
    pub inputs: u32,
    pub outputs: u32,
    pub params: u32,
    pub programs: u32,
    pub latency: u32,
    pub tail: Option<u32>,
    pub editor: bool,
    pub synth: bool,
    /// It keeps its state as a chunk of its own.
    pub chunks: bool,
}

/// A parameter: its index, name and units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub index: u32,
    pub name: String,
    pub label: String,
}

/// Where the timeline is, for plug-ins that follow it: playing or not, the
/// project's tempo and meter, the loop region (quarter notes) when looping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transport {
    pub playing: bool,
    pub tempo: f64,
    pub numerator: i32,
    pub denominator: i32,
    pub cycle: Option<(f64, f64)>,
}

impl Default for Transport {
    fn default() -> Transport {
        Transport {
            playing: false,
            tempo: 120.0,
            numerator: 4,
            denominator: 4,
            cycle: None,
        }
    }
}

/// The number a plug-in is known by, as BananaV keeps it (eight
/// hexadecimal digits; a shell's plug-in's own number after a colon).
pub fn class_id(unique_id: i32, id: i32) -> String {
    if id == 0 {
        format!("{:08X}", unique_id as u32)
    } else {
        format!("{:08X}:{:08X}", unique_id as u32, id as u32)
    }
}

/// The shell's plug-in's number in a class id ([`class_id`]); 0 for none.
pub fn shell_id(class_id: &str) -> i32 {
    class_id
        .split_once(':')
        .and_then(|(_, id)| u32::from_str_radix(id, 16).ok())
        .map_or(0, |v| v as i32)
}
