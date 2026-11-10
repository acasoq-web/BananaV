//! The binary interface VST 2 plug-ins use, as BananaV declares it: what a
//! plug-in's library exports, the object it hands the host, the numbers its
//! functions take and the host's answers. These are facts of how the
//! plug-ins behave — offsets, numbers, a calling convention — written in
//! BananaV's own words; no code, header or text of Steinberg's is used.
//!
//! A plug-in library exports an entry point (`VSTPluginMain`, older ones
//! `main`) that takes the host's callback and returns the plug-in's object.
//! Every function goes by the C calling convention (cdecl on 32-bit x86).

#![allow(unsafe_code)]

use std::ffi::c_void;

/// The object's first field: "VstP".
pub const MAGIC: i32 = i32::from_be_bytes(*b"VstP");

/// The host's callback: (plug-in, request, index, value, pointer, number).
pub type HostCallback =
    unsafe extern "C" fn(*mut PluginObject, i32, i32, isize, *mut c_void, f32) -> isize;
/// The plug-in's requests function, with the same arguments.
pub type Dispatcher =
    unsafe extern "C" fn(*mut PluginObject, i32, i32, isize, *mut c_void, f32) -> isize;
/// Processes `frames` samples of each channel (inputs, outputs).
pub type ProcessF32 = unsafe extern "C" fn(*mut PluginObject, *mut *mut f32, *mut *mut f32, i32);
pub type ProcessF64 = unsafe extern "C" fn(*mut PluginObject, *mut *mut f64, *mut *mut f64, i32);
pub type SetParameter = unsafe extern "C" fn(*mut PluginObject, i32, f32);
pub type GetParameter = unsafe extern "C" fn(*mut PluginObject, i32) -> f32;
/// The library's entry point.
pub type Entry = unsafe extern "C" fn(HostCallback) -> *mut PluginObject;

/// The object a plug-in hands out: its functions and what it is.
#[repr(C)]
#[derive(Debug)]
pub struct PluginObject {
    pub magic: i32,
    pub dispatcher: Option<Dispatcher>,
    /// Adds its output to the output buffers (old plug-ins; unused when it
    /// can replace).
    pub process_adding: Option<ProcessF32>,
    pub set_parameter: Option<SetParameter>,
    pub get_parameter: Option<GetParameter>,
    pub programs: i32,
    pub params: i32,
    pub inputs: i32,
    pub outputs: i32,
    pub flags: i32,
    /// For the host's own use (BananaV keeps its instance's data here).
    pub host_data: [isize; 2],
    /// Its latency (samples).
    pub latency: i32,
    pub unused: [i32; 2],
    pub unused_ratio: f32,
    /// For the plug-in's own use.
    pub object: *mut c_void,
    pub user: *mut c_void,
    /// The number it is known by (four letters, often), and its version.
    pub unique_id: i32,
    pub version: i32,
    /// Writes its output over the output buffers.
    pub process: Option<ProcessF32>,
    pub process_f64: Option<ProcessF64>,
    pub reserved: [u8; 56],
}

/// `PluginObject::flags`.
pub mod flags {
    pub const HAS_EDITOR: i32 = 1 << 0;
    pub const CAN_REPLACE: i32 = 1 << 4;
    pub const PROGRAM_CHUNKS: i32 = 1 << 5;
    pub const IS_SYNTH: i32 = 1 << 8;
    pub const NO_SOUND_IN_STOP: i32 = 1 << 9;
    pub const CAN_REPLACE_F64: i32 = 1 << 12;
}

/// The requests a plug-in takes (its dispatcher's second argument).
pub mod op {
    pub const OPEN: i32 = 0;
    pub const CLOSE: i32 = 1;
    pub const SET_PROGRAM: i32 = 2;
    pub const GET_PROGRAM: i32 = 3;
    pub const SET_PROGRAM_NAME: i32 = 4;
    pub const GET_PROGRAM_NAME: i32 = 5;
    pub const PARAM_LABEL: i32 = 6;
    pub const PARAM_DISPLAY: i32 = 7;
    pub const PARAM_NAME: i32 = 8;
    pub const SET_SAMPLE_RATE: i32 = 10;
    pub const SET_BLOCK_SIZE: i32 = 11;
    /// 1 starts it (resume), 0 stops it (suspend).
    pub const MAINS: i32 = 12;
    pub const EDITOR_RECT: i32 = 13;
    pub const EDITOR_OPEN: i32 = 14;
    pub const EDITOR_CLOSE: i32 = 15;
    pub const EDITOR_IDLE: i32 = 19;
    /// Its state as bytes: index 0 the whole bank, 1 the program.
    pub const GET_CHUNK: i32 = 23;
    pub const SET_CHUNK: i32 = 24;
    pub const PROCESS_EVENTS: i32 = 25;
    pub const CAN_BE_AUTOMATED: i32 = 26;
    pub const STRING_TO_PARAM: i32 = 27;
    pub const PROGRAM_NAME_INDEXED: i32 = 29;
    pub const PLUG_CATEGORY: i32 = 35;
    pub const SET_BYPASS: i32 = 44;
    pub const EFFECT_NAME: i32 = 45;
    pub const VENDOR: i32 = 47;
    pub const PRODUCT: i32 = 48;
    pub const VENDOR_VERSION: i32 = 49;
    pub const CAN_DO: i32 = 51;
    pub const TAIL_SIZE: i32 = 52;
    pub const INTERFACE_VERSION: i32 = 58;
    /// A shell's next plug-in: its number, its name in the pointer.
    pub const SHELL_NEXT: i32 = 70;
    pub const START_PROCESS: i32 = 71;
    pub const STOP_PROCESS: i32 = 72;
}

/// The host's answers (its callback's second argument).
pub mod host_op {
    pub const AUTOMATE: i32 = 0;
    pub const VERSION: i32 = 1;
    pub const CURRENT_ID: i32 = 2;
    pub const IDLE: i32 = 3;
    pub const GET_TIME: i32 = 7;
    pub const PROCESS_EVENTS: i32 = 8;
    pub const IO_CHANGED: i32 = 13;
    pub const SIZE_WINDOW: i32 = 15;
    pub const SAMPLE_RATE: i32 = 16;
    pub const BLOCK_SIZE: i32 = 17;
    pub const INPUT_LATENCY: i32 = 18;
    pub const OUTPUT_LATENCY: i32 = 19;
    pub const PROCESS_LEVEL: i32 = 23;
    pub const AUTOMATION_STATE: i32 = 24;
    pub const VENDOR: i32 = 32;
    pub const PRODUCT: i32 = 33;
    pub const VENDOR_VERSION: i32 = 34;
    pub const CAN_DO: i32 = 37;
    pub const LANGUAGE: i32 = 38;
    pub const DIRECTORY: i32 = 41;
    pub const UPDATE_DISPLAY: i32 = 42;
    pub const BEGIN_EDIT: i32 = 43;
    pub const END_EDIT: i32 = 44;
}

/// The interface's version BananaV answers.
pub const HOST_VERSION: isize = 2400;

/// Plug-ins' categories (`op::PLUG_CATEGORY`).
pub mod category {
    pub const EFFECT: isize = 1;
    pub const SYNTH: isize = 2;
    pub const SHELL: isize = 10;
    pub const GENERATOR: isize = 11;
}

/// Where the timeline is, as plug-ins ask for it (`host_op::GET_TIME`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TimeInfo {
    pub sample_pos: f64,
    pub sample_rate: f64,
    pub nanoseconds: f64,
    /// Quarter notes from the start.
    pub quarters: f64,
    pub tempo: f64,
    /// The bar's start, in quarter notes.
    pub bar_start: f64,
    pub cycle_start: f64,
    pub cycle_end: f64,
    pub numerator: i32,
    pub denominator: i32,
    pub smpte_offset: i32,
    pub smpte_rate: i32,
    pub samples_to_clock: i32,
    pub flags: i32,
}

/// `TimeInfo::flags`.
pub mod time {
    pub const CHANGED: i32 = 1;
    pub const PLAYING: i32 = 1 << 1;
    pub const CYCLE_ACTIVE: i32 = 1 << 2;
    pub const NANOS_VALID: i32 = 1 << 8;
    pub const QUARTERS_VALID: i32 = 1 << 9;
    pub const TEMPO_VALID: i32 = 1 << 10;
    pub const BARS_VALID: i32 = 1 << 11;
    pub const CYCLE_VALID: i32 = 1 << 12;
    pub const SIGNATURE_VALID: i32 = 1 << 13;
}

/// An editor's rectangle (`op::EDITOR_RECT` points to one).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub top: i16,
    pub left: i16,
    pub bottom: i16,
    pub right: i16,
}

/// The longest text BananaV takes from a plug-in (they write past the
/// lengths they promise).
pub const TEXT: usize = 256;

/// Text a plug-in wrote into `buf` (to its first zero; Windows Latin 1
/// read as such).
pub fn text_of(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let bytes = &buf[..end];
    match std::str::from_utf8(bytes) {
        Ok(s) => s.trim().to_string(),
        Err(_) => bytes
            .iter()
            .map(|&b| char::from(b))
            .collect::<String>()
            .trim()
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout plug-ins expect, on this processor.
    #[test]
    fn the_object_is_laid_out_as_plug_ins_expect() {
        let p = std::mem::size_of::<usize>();
        let base = std::mem::MaybeUninit::<PluginObject>::uninit();
        let at = |f: *const u8| f as usize - base.as_ptr() as usize;
        // SAFETY: only fields' addresses, nothing read.
        unsafe {
            let o = base.as_ptr();
            assert_eq!(at(std::ptr::addr_of!((*o).dispatcher).cast()), p);
            assert_eq!(at(std::ptr::addr_of!((*o).programs).cast()), 5 * p);
            assert_eq!(at(std::ptr::addr_of!((*o).flags).cast()), 5 * p + 16);
            assert_eq!(at(std::ptr::addr_of!((*o).unique_id).cast()), 10 * p + 32);
            assert_eq!(at(std::ptr::addr_of!((*o).process).cast()), 10 * p + 40);
        }
        if p == 8 {
            assert_eq!(std::mem::size_of::<PluginObject>(), 192);
        } else {
            assert_eq!(std::mem::size_of::<PluginObject>(), 144);
        }
        assert_eq!(std::mem::size_of::<TimeInfo>(), 88);
        assert_eq!(MAGIC, 0x5673_7450);
        assert_eq!(text_of(b"Gain\0junk"), "Gain");
    }
}
