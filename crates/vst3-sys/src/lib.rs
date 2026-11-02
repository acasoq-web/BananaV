//! The VST 3 interfaces' binary form, as the SDK's headers declare them
//! (`pluginterfaces`, MIT since VST 3.8; the notice is in `licenses`):
//! interface identifiers, result codes, the structures passed and each
//! interface's table of functions — the layout of a C++ class of only
//! pure virtual functions, which the compilers of these systems share.
//! `PLUGIN_API` is `__stdcall` on Windows (the one convention on x86-64
//! and ARM64) and nothing elsewhere: `extern "system"`. No logic lives
//! here: the host is `bv-vst3`.

use std::ffi::{c_char, c_int, c_void};

/// A result code (`tresult`).
pub type TResult = i32;
/// A 16-byte identifier of an interface or a class (`TUID`).
pub type Tuid = [u8; 16];
/// An interface pointer: a pointer to the object's pointer to the table.
pub type Raw = *mut c_void;

/// The result codes: COM's on Windows, small numbers elsewhere
/// (`COM_COMPATIBLE` only on Windows).
pub mod result {
    use super::TResult;

    pub const OK: TResult = 0;
    pub const TRUE: TResult = 0;
    pub const FALSE: TResult = 1;
    #[cfg(windows)]
    pub const NO_INTERFACE: TResult = 0x8000_4002_u32 as i32;
    #[cfg(windows)]
    pub const INVALID_ARGUMENT: TResult = 0x8007_0057_u32 as i32;
    #[cfg(windows)]
    pub const NOT_IMPLEMENTED: TResult = 0x8000_4001_u32 as i32;
    #[cfg(windows)]
    pub const INTERNAL_ERROR: TResult = 0x8000_4005_u32 as i32;
    #[cfg(not(windows))]
    pub const NO_INTERFACE: TResult = -1;
    #[cfg(not(windows))]
    pub const INVALID_ARGUMENT: TResult = 2;
    #[cfg(not(windows))]
    pub const NOT_IMPLEMENTED: TResult = 3;
    #[cfg(not(windows))]
    pub const INTERNAL_ERROR: TResult = 4;
}

/// An identifier from its four numbers (`INLINE_UID`): on Windows in
/// COM's byte order (the first three fields little-endian), elsewhere as
/// written.
pub const fn uid(l1: u32, l2: u32, l3: u32, l4: u32) -> Tuid {
    const fn b(v: u32, shift: u32) -> u8 {
        (v >> shift) as u8
    }
    if cfg!(windows) {
        [
            b(l1, 0),
            b(l1, 8),
            b(l1, 16),
            b(l1, 24),
            b(l2, 16),
            b(l2, 24),
            b(l2, 0),
            b(l2, 8),
            b(l3, 24),
            b(l3, 16),
            b(l3, 8),
            b(l3, 0),
            b(l4, 24),
            b(l4, 16),
            b(l4, 8),
            b(l4, 0),
        ]
    } else {
        [
            b(l1, 24),
            b(l1, 16),
            b(l1, 8),
            b(l1, 0),
            b(l2, 24),
            b(l2, 16),
            b(l2, 8),
            b(l2, 0),
            b(l3, 24),
            b(l3, 16),
            b(l3, 8),
            b(l3, 0),
            b(l4, 24),
            b(l4, 16),
            b(l4, 8),
            b(l4, 0),
        ]
    }
}

/// An identifier's four numbers (the inverse of [`uid`]).
pub fn uid_numbers(id: &Tuid) -> [u32; 4] {
    let be = |i: usize| u32::from_be_bytes([id[i], id[i + 1], id[i + 2], id[i + 3]]);
    if cfg!(windows) {
        [
            u32::from_le_bytes([id[0], id[1], id[2], id[3]]),
            u32::from(u16::from_le_bytes([id[4], id[5]])) << 16
                | u32::from(u16::from_le_bytes([id[6], id[7]])),
            be(8),
            be(12),
        ]
    } else {
        [be(0), be(4), be(8), be(12)]
    }
}

/// An identifier as the SDK writes it (`FUID::toString`, the module
/// information's "CID"): its four numbers in 32 hexadecimal digits.
pub fn uid_to_hex(id: &Tuid) -> String {
    uid_numbers(id).iter().map(|n| format!("{n:08X}")).collect()
}

/// The identifier 32 hexadecimal digits write.
pub fn uid_from_hex(text: &str) -> Option<Tuid> {
    let text = text.trim();
    if text.len() != 32 || !text.is_ascii() {
        return None;
    }
    let n = |i: usize| u32::from_str_radix(&text[i * 8..i * 8 + 8], 16).ok();
    Some(uid(n(0)?, n(1)?, n(2)?, n(3)?))
}

/// The interfaces' identifiers.
pub mod iid {
    use super::{Tuid, uid};

    pub const FUNKNOWN: Tuid = uid(0x0000_0000, 0x0000_0000, 0xC000_0000, 0x0000_0046);
    pub const PLUGIN_BASE: Tuid = uid(0x2288_8DDB, 0x156E_45AE, 0x8358_B348, 0x0819_0625);
    pub const PLUGIN_FACTORY: Tuid = uid(0x7A4D_811C, 0x5211_4A1F, 0xAED9_D2EE, 0x0B43_BF9F);
    pub const PLUGIN_FACTORY2: Tuid = uid(0x0007_B650, 0xF24B_4C0B, 0xA464_EDB9, 0xF00B_2ABB);
    pub const PLUGIN_FACTORY3: Tuid = uid(0x4555_A2AB, 0xC123_4E57, 0x9B12_2910, 0x3687_8931);
    pub const COMPONENT: Tuid = uid(0xE831_FF31, 0xF2D5_4301, 0x928E_BBEE, 0x2569_7802);
    pub const AUDIO_PROCESSOR: Tuid = uid(0x4204_3F99, 0xB7DA_453C, 0xA569_E79D, 0x9AAE_C33D);
    pub const EDIT_CONTROLLER: Tuid = uid(0xDCD7_BBE3, 0x7742_448D, 0xA874_AACC, 0x979C_759E);
    pub const COMPONENT_HANDLER: Tuid = uid(0x93A0_BEA3, 0x0BD0_45DB, 0x8E89_0B0C, 0xC1E4_6AC6);
    pub const COMPONENT_HANDLER2: Tuid = uid(0xF040_B4B3, 0xA360_45EC, 0xABCD_C045, 0xB4D5_A2CC);
    pub const CONNECTION_POINT: Tuid = uid(0x70A4_156F, 0x6E6E_4026, 0x9891_48BF, 0xAA60_D8D1);
    pub const MESSAGE: Tuid = uid(0x936F_033B, 0xC6C0_47DB, 0xBB08_82F8, 0x13C1_E613);
    pub const ATTRIBUTE_LIST: Tuid = uid(0x1E5F_0AEB, 0xCC7F_4533, 0xA254_4011, 0x38AD_5EE4);
    pub const HOST_APPLICATION: Tuid = uid(0x58E5_95CC, 0xDB2D_4969, 0x8B6A_AF8C, 0x36A6_64E5);
    pub const BSTREAM: Tuid = uid(0xC3BF_6EA2, 0x3099_4752, 0x9B6B_F990, 0x1EE3_3E9B);
    pub const SIZEABLE_STREAM: Tuid = uid(0x04F9_549E, 0xE02F_4E6E, 0x87E8_6A87, 0x47F4_E17F);
    pub const PARAM_VALUE_QUEUE: Tuid = uid(0x0126_3A18, 0xED07_4F6F, 0x98C9_D356, 0x4686_F9BA);
    pub const PARAMETER_CHANGES: Tuid = uid(0xA477_9663, 0x0BB6_4A56, 0xB443_84A8, 0x466F_EB9D);
    pub const PLUG_VIEW: Tuid = uid(0x5BC3_2507, 0xD060_49EA, 0xA615_1B52, 0x2B75_5B29);
    pub const PLUG_FRAME: Tuid = uid(0x367F_AF01, 0xAFA9_4693, 0x8D4D_A2A0, 0xED08_82A3);
    /// Linux's run loop (`Steinberg::Linux`), which the host gives through
    /// the editor's frame, and its handlers.
    pub const RUN_LOOP: Tuid = uid(0x18C3_5366, 0x9776_4F1A, 0x9C5B_8385, 0x7A87_1389);
    pub const EVENT_HANDLER: Tuid = uid(0x561E_65C9, 0x13A0_496F, 0x813A_2C35, 0x654D_7983);
    pub const TIMER_HANDLER: Tuid = uid(0x10BD_D94F, 0x4142_4774, 0x821F_AD8F, 0xECA7_2CA9);
}

/// Class categories.
pub const AUDIO_EFFECT_CLASS: &str = "Audio Module Class";
pub const CONTROLLER_CLASS: &str = "Component Controller Class";

/// Media types, bus directions and bus types.
pub const AUDIO: i32 = 0;
pub const EVENT: i32 = 1;
pub const INPUT: i32 = 0;
pub const OUTPUT: i32 = 1;
pub const MAIN: i32 = 0;
pub const AUX: i32 = 1;
/// `BusInfo::flags`: the bus wants to be on.
pub const BUS_DEFAULT_ACTIVE: u32 = 1;

/// Processing modes and sample sizes.
pub const REALTIME: i32 = 0;
pub const OFFLINE: i32 = 2;
pub const SAMPLE32: i32 = 0;

/// Speakers and arrangements.
pub mod speaker {
    pub const L: u64 = 1 << 0;
    pub const R: u64 = 1 << 1;
    pub const C: u64 = 1 << 2;
    pub const LFE: u64 = 1 << 3;
    pub const LS: u64 = 1 << 4;
    pub const RS: u64 = 1 << 5;
    pub const M: u64 = 1 << 19;
    pub const MONO: u64 = M;
    pub const STEREO: u64 = L | R;
    pub const SURROUND_51: u64 = L | R | C | LFE | LS | RS;
}

/// `ParameterInfo::flags`.
pub mod param_flags {
    pub const CAN_AUTOMATE: i32 = 1 << 0;
    pub const READ_ONLY: i32 = 1 << 1;
    pub const WRAP_AROUND: i32 = 1 << 2;
    pub const LIST: i32 = 1 << 3;
    pub const HIDDEN: i32 = 1 << 4;
    pub const PROGRAM_CHANGE: i32 = 1 << 15;
    pub const BYPASS: i32 = 1 << 16;
}

/// `IComponentHandler::restartComponent`'s flags.
pub mod restart {
    pub const RELOAD_COMPONENT: i32 = 1 << 0;
    pub const IO_CHANGED: i32 = 1 << 1;
    pub const PARAM_VALUES_CHANGED: i32 = 1 << 2;
    pub const LATENCY_CHANGED: i32 = 1 << 3;
    pub const PARAM_TITLES_CHANGED: i32 = 1 << 4;
}

/// `ProcessContext::state`.
pub mod context {
    pub const PLAYING: u32 = 1 << 1;
    pub const PROJECT_TIME_MUSIC_VALID: u32 = 1 << 9;
    pub const TEMPO_VALID: u32 = 1 << 10;
    pub const TIME_SIG_VALID: u32 = 1 << 13;
    pub const CONT_TIME_VALID: u32 = 1 << 17;
}

/// The window kinds a plug-in view can live in.
pub const PLATFORM_HWND: &std::ffi::CStr = c"HWND";
pub const PLATFORM_X11: &std::ffi::CStr = c"X11EmbedWindowID";
/// The one view type.
pub const VIEW_EDITOR: &std::ffi::CStr = c"editor";

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PFactoryInfo {
    pub vendor: [c_char; 64],
    pub url: [c_char; 256],
    pub email: [c_char; 128],
    pub flags: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PClassInfo {
    pub cid: Tuid,
    pub cardinality: i32,
    pub category: [c_char; 32],
    pub name: [c_char; 64],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PClassInfo2 {
    pub cid: Tuid,
    pub cardinality: i32,
    pub category: [c_char; 32],
    pub name: [c_char; 64],
    pub class_flags: u32,
    pub sub_categories: [c_char; 128],
    pub vendor: [c_char; 64],
    pub version: [c_char; 64],
    pub sdk_version: [c_char; 64],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PClassInfoW {
    pub cid: Tuid,
    pub cardinality: i32,
    pub category: [c_char; 32],
    pub name: [u16; 64],
    pub class_flags: u32,
    pub sub_categories: [c_char; 128],
    pub vendor: [u16; 64],
    pub version: [u16; 64],
    pub sdk_version: [u16; 64],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BusInfo {
    pub media_type: i32,
    pub direction: i32,
    pub channel_count: i32,
    pub name: [u16; 128],
    pub bus_type: i32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RoutingInfo {
    pub media_type: i32,
    pub bus_index: i32,
    pub channel: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ProcessSetup {
    pub process_mode: i32,
    pub symbolic_sample_size: i32,
    pub max_samples_per_block: i32,
    pub sample_rate: f64,
}

#[repr(C)]
#[derive(Debug)]
pub struct AudioBusBuffers {
    pub num_channels: i32,
    pub silence_flags: u64,
    /// `channelBuffers32` (a union with the 64-bit buffers).
    pub buffers: *mut *mut f32,
}

#[repr(C)]
#[derive(Debug)]
pub struct ProcessData {
    pub process_mode: i32,
    pub symbolic_sample_size: i32,
    pub num_samples: i32,
    pub num_inputs: i32,
    pub num_outputs: i32,
    pub inputs: *mut AudioBusBuffers,
    pub outputs: *mut AudioBusBuffers,
    pub input_parameter_changes: Raw,
    pub output_parameter_changes: Raw,
    pub input_events: Raw,
    pub output_events: Raw,
    pub process_context: *mut ProcessContext,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Chord {
    pub key_note: u8,
    pub root_note: u8,
    pub chord_mask: i16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameRate {
    pub frames_per_second: u32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessContext {
    pub state: u32,
    pub sample_rate: f64,
    pub project_time_samples: i64,
    pub system_time: i64,
    pub continous_time_samples: i64,
    pub project_time_music: f64,
    pub bar_position_music: f64,
    pub cycle_start_music: f64,
    pub cycle_end_music: f64,
    pub tempo: f64,
    pub time_sig_numerator: i32,
    pub time_sig_denominator: i32,
    pub chord: Chord,
    pub smpte_offset_subframes: i32,
    pub frame_rate: FrameRate,
    pub samples_to_next_clock: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ParameterInfo {
    pub id: u32,
    pub title: [u16; 128],
    pub short_title: [u16; 128],
    pub units: [u16; 128],
    pub step_count: i32,
    pub default_normalized_value: f64,
    pub unit_id: i32,
    pub flags: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// The tables. Each begins with its base interface's, so an interface
/// pointer works as a pointer to any of its bases.
#[repr(C)]
#[derive(Debug)]
pub struct FUnknownVtbl {
    pub query_interface: unsafe extern "system" fn(Raw, *const Tuid, *mut Raw) -> TResult,
    pub add_ref: unsafe extern "system" fn(Raw) -> u32,
    pub release: unsafe extern "system" fn(Raw) -> u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct IPluginBaseVtbl {
    pub unknown: FUnknownVtbl,
    pub initialize: unsafe extern "system" fn(Raw, Raw) -> TResult,
    pub terminate: unsafe extern "system" fn(Raw) -> TResult,
}

#[repr(C)]
#[derive(Debug)]
pub struct IPluginFactoryVtbl {
    pub unknown: FUnknownVtbl,
    pub get_factory_info: unsafe extern "system" fn(Raw, *mut PFactoryInfo) -> TResult,
    pub count_classes: unsafe extern "system" fn(Raw) -> i32,
    pub get_class_info: unsafe extern "system" fn(Raw, i32, *mut PClassInfo) -> TResult,
    pub create_instance:
        unsafe extern "system" fn(Raw, *const c_char, *const c_char, *mut Raw) -> TResult,
}

#[repr(C)]
#[derive(Debug)]
pub struct IPluginFactory2Vtbl {
    pub factory: IPluginFactoryVtbl,
    pub get_class_info2: unsafe extern "system" fn(Raw, i32, *mut PClassInfo2) -> TResult,
}

#[repr(C)]
#[derive(Debug)]
pub struct IPluginFactory3Vtbl {
    pub factory2: IPluginFactory2Vtbl,
    pub get_class_info_unicode: unsafe extern "system" fn(Raw, i32, *mut PClassInfoW) -> TResult,
    pub set_host_context: unsafe extern "system" fn(Raw, Raw) -> TResult,
}

#[repr(C)]
#[derive(Debug)]
pub struct IComponentVtbl {
    pub base: IPluginBaseVtbl,
    pub get_controller_class_id: unsafe extern "system" fn(Raw, *mut Tuid) -> TResult,
    pub set_io_mode: unsafe extern "system" fn(Raw, i32) -> TResult,
    pub get_bus_count: unsafe extern "system" fn(Raw, i32, i32) -> i32,
    pub get_bus_info: unsafe extern "system" fn(Raw, i32, i32, i32, *mut BusInfo) -> TResult,
    pub get_routing_info:
        unsafe extern "system" fn(Raw, *mut RoutingInfo, *mut RoutingInfo) -> TResult,
    pub activate_bus: unsafe extern "system" fn(Raw, i32, i32, i32, u8) -> TResult,
    pub set_active: unsafe extern "system" fn(Raw, u8) -> TResult,
    pub set_state: unsafe extern "system" fn(Raw, Raw) -> TResult,
    pub get_state: unsafe extern "system" fn(Raw, Raw) -> TResult,
}

#[repr(C)]
#[derive(Debug)]
pub struct IAudioProcessorVtbl {
    pub unknown: FUnknownVtbl,
    pub set_bus_arrangements:
        unsafe extern "system" fn(Raw, *mut u64, i32, *mut u64, i32) -> TResult,
    pub get_bus_arrangement: unsafe extern "system" fn(Raw, i32, i32, *mut u64) -> TResult,
    pub can_process_sample_size: unsafe extern "system" fn(Raw, i32) -> TResult,
    pub get_latency_samples: unsafe extern "system" fn(Raw) -> u32,
    pub setup_processing: unsafe extern "system" fn(Raw, *mut ProcessSetup) -> TResult,
    pub set_processing: unsafe extern "system" fn(Raw, u8) -> TResult,
    pub process: unsafe extern "system" fn(Raw, *mut ProcessData) -> TResult,
    pub get_tail_samples: unsafe extern "system" fn(Raw) -> u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct IEditControllerVtbl {
    pub base: IPluginBaseVtbl,
    pub set_component_state: unsafe extern "system" fn(Raw, Raw) -> TResult,
    pub set_state: unsafe extern "system" fn(Raw, Raw) -> TResult,
    pub get_state: unsafe extern "system" fn(Raw, Raw) -> TResult,
    pub get_parameter_count: unsafe extern "system" fn(Raw) -> i32,
    pub get_parameter_info: unsafe extern "system" fn(Raw, i32, *mut ParameterInfo) -> TResult,
    pub get_param_string_by_value: unsafe extern "system" fn(Raw, u32, f64, *mut u16) -> TResult,
    pub get_param_value_by_string:
        unsafe extern "system" fn(Raw, u32, *const u16, *mut f64) -> TResult,
    pub normalized_param_to_plain: unsafe extern "system" fn(Raw, u32, f64) -> f64,
    pub plain_param_to_normalized: unsafe extern "system" fn(Raw, u32, f64) -> f64,
    pub get_param_normalized: unsafe extern "system" fn(Raw, u32) -> f64,
    pub set_param_normalized: unsafe extern "system" fn(Raw, u32, f64) -> TResult,
    pub set_component_handler: unsafe extern "system" fn(Raw, Raw) -> TResult,
    pub create_view: unsafe extern "system" fn(Raw, *const c_char) -> Raw,
}

#[repr(C)]
#[derive(Debug)]
pub struct IConnectionPointVtbl {
    pub unknown: FUnknownVtbl,
    pub connect: unsafe extern "system" fn(Raw, Raw) -> TResult,
    pub disconnect: unsafe extern "system" fn(Raw, Raw) -> TResult,
    pub notify: unsafe extern "system" fn(Raw, Raw) -> TResult,
}

#[repr(C)]
#[derive(Debug)]
pub struct IPlugViewVtbl {
    pub unknown: FUnknownVtbl,
    pub is_platform_type_supported: unsafe extern "system" fn(Raw, *const c_char) -> TResult,
    pub attached: unsafe extern "system" fn(Raw, *mut c_void, *const c_char) -> TResult,
    pub removed: unsafe extern "system" fn(Raw) -> TResult,
    pub on_wheel: unsafe extern "system" fn(Raw, f32) -> TResult,
    pub on_key_down: unsafe extern "system" fn(Raw, u16, i16, i16) -> TResult,
    pub on_key_up: unsafe extern "system" fn(Raw, u16, i16, i16) -> TResult,
    pub get_size: unsafe extern "system" fn(Raw, *mut ViewRect) -> TResult,
    pub on_size: unsafe extern "system" fn(Raw, *mut ViewRect) -> TResult,
    pub on_focus: unsafe extern "system" fn(Raw, u8) -> TResult,
    pub set_frame: unsafe extern "system" fn(Raw, Raw) -> TResult,
    pub can_resize: unsafe extern "system" fn(Raw) -> TResult,
    pub check_size_constraint: unsafe extern "system" fn(Raw, *mut ViewRect) -> TResult,
}

/// The tables of the interfaces the host implements.
#[repr(C)]
#[derive(Debug)]
pub struct IHostApplicationVtbl {
    pub unknown: FUnknownVtbl,
    pub get_name: unsafe extern "system" fn(Raw, *mut u16) -> TResult,
    pub create_instance: unsafe extern "system" fn(Raw, *mut Tuid, *mut Tuid, *mut Raw) -> TResult,
}

#[repr(C)]
#[derive(Debug)]
pub struct IBStreamVtbl {
    pub unknown: FUnknownVtbl,
    pub read: unsafe extern "system" fn(Raw, *mut c_void, i32, *mut i32) -> TResult,
    pub write: unsafe extern "system" fn(Raw, *mut c_void, i32, *mut i32) -> TResult,
    pub seek: unsafe extern "system" fn(Raw, i64, i32, *mut i64) -> TResult,
    pub tell: unsafe extern "system" fn(Raw, *mut i64) -> TResult,
}

#[repr(C)]
#[derive(Debug)]
pub struct ISizeableStreamVtbl {
    pub unknown: FUnknownVtbl,
    pub get_stream_size: unsafe extern "system" fn(Raw, *mut i64) -> TResult,
    pub set_stream_size: unsafe extern "system" fn(Raw, i64) -> TResult,
}

#[repr(C)]
#[derive(Debug)]
pub struct IMessageVtbl {
    pub unknown: FUnknownVtbl,
    pub get_message_id: unsafe extern "system" fn(Raw) -> *const c_char,
    pub set_message_id: unsafe extern "system" fn(Raw, *const c_char),
    pub get_attributes: unsafe extern "system" fn(Raw) -> Raw,
}

#[repr(C)]
#[derive(Debug)]
pub struct IAttributeListVtbl {
    pub unknown: FUnknownVtbl,
    pub set_int: unsafe extern "system" fn(Raw, *const c_char, i64) -> TResult,
    pub get_int: unsafe extern "system" fn(Raw, *const c_char, *mut i64) -> TResult,
    pub set_float: unsafe extern "system" fn(Raw, *const c_char, f64) -> TResult,
    pub get_float: unsafe extern "system" fn(Raw, *const c_char, *mut f64) -> TResult,
    pub set_string: unsafe extern "system" fn(Raw, *const c_char, *const u16) -> TResult,
    pub get_string: unsafe extern "system" fn(Raw, *const c_char, *mut u16, u32) -> TResult,
    pub set_binary: unsafe extern "system" fn(Raw, *const c_char, *const c_void, u32) -> TResult,
    pub get_binary:
        unsafe extern "system" fn(Raw, *const c_char, *mut *const c_void, *mut u32) -> TResult,
}

#[repr(C)]
#[derive(Debug)]
pub struct IParamValueQueueVtbl {
    pub unknown: FUnknownVtbl,
    pub get_parameter_id: unsafe extern "system" fn(Raw) -> u32,
    pub get_point_count: unsafe extern "system" fn(Raw) -> i32,
    pub get_point: unsafe extern "system" fn(Raw, i32, *mut i32, *mut f64) -> TResult,
    pub add_point: unsafe extern "system" fn(Raw, i32, f64, *mut i32) -> TResult,
}

#[repr(C)]
#[derive(Debug)]
pub struct IParameterChangesVtbl {
    pub unknown: FUnknownVtbl,
    pub get_parameter_count: unsafe extern "system" fn(Raw) -> i32,
    pub get_parameter_data: unsafe extern "system" fn(Raw, i32) -> Raw,
    pub add_parameter_data: unsafe extern "system" fn(Raw, *const u32, *mut i32) -> Raw,
}

#[repr(C)]
#[derive(Debug)]
pub struct IComponentHandlerVtbl {
    pub unknown: FUnknownVtbl,
    pub begin_edit: unsafe extern "system" fn(Raw, u32) -> TResult,
    pub perform_edit: unsafe extern "system" fn(Raw, u32, f64) -> TResult,
    pub end_edit: unsafe extern "system" fn(Raw, u32) -> TResult,
    pub restart_component: unsafe extern "system" fn(Raw, i32) -> TResult,
}

#[repr(C)]
#[derive(Debug)]
pub struct IComponentHandler2Vtbl {
    pub unknown: FUnknownVtbl,
    pub set_dirty: unsafe extern "system" fn(Raw, u8) -> TResult,
    pub request_open_editor: unsafe extern "system" fn(Raw, *const c_char) -> TResult,
    pub start_group_edit: unsafe extern "system" fn(Raw) -> TResult,
    pub finish_group_edit: unsafe extern "system" fn(Raw) -> TResult,
}

#[repr(C)]
#[derive(Debug)]
pub struct IPlugFrameVtbl {
    pub unknown: FUnknownVtbl,
    pub resize_view: unsafe extern "system" fn(Raw, Raw, *mut ViewRect) -> TResult,
}

/// Linux's run loop: plug-ins register file descriptors to be told about
/// when they can be read, and timers.
#[repr(C)]
#[derive(Debug)]
pub struct IRunLoopVtbl {
    pub unknown: FUnknownVtbl,
    pub register_event_handler: unsafe extern "system" fn(Raw, Raw, c_int) -> TResult,
    pub unregister_event_handler: unsafe extern "system" fn(Raw, Raw) -> TResult,
    pub register_timer: unsafe extern "system" fn(Raw, Raw, u64) -> TResult,
    pub unregister_timer: unsafe extern "system" fn(Raw, Raw) -> TResult,
}

#[repr(C)]
#[derive(Debug)]
pub struct IEventHandlerVtbl {
    pub unknown: FUnknownVtbl,
    pub on_fd_is_set: unsafe extern "system" fn(Raw, c_int),
}

#[repr(C)]
#[derive(Debug)]
pub struct ITimerHandlerVtbl {
    pub unknown: FUnknownVtbl,
    pub on_timer: unsafe extern "system" fn(Raw),
}

/// The module's entry points.
pub type GetFactoryProc = unsafe extern "system" fn() -> Raw;

/// A NUL-ended string of at most `N` characters (a field of the
/// structures above) as text.
pub fn text_of<const N: usize>(chars: &[c_char; N]) -> String {
    let bytes: Vec<u8> = chars
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// A NUL-ended UTF-16 string as text.
pub fn text_of16(chars: &[u16]) -> String {
    let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
    String::from_utf16_lossy(&chars[..end])
}

/// `text` into a field of `N` UTF-16 characters, NUL-ended (cut short).
pub fn fill16<const N: usize>(field: &mut [u16; N], text: &str) {
    let mut n = 0;
    for c in text.encode_utf16().take(N.saturating_sub(1)) {
        field[n] = c;
        n += 1;
    }
    field[n] = 0;
}

/// `text` into a field of `N` characters, NUL-ended (cut short).
pub fn fill8<const N: usize>(field: &mut [c_char; N], text: &str) {
    let mut n = 0;
    for &b in text.as_bytes().iter().take(N.saturating_sub(1)) {
        field[n] = b as c_char;
        n += 1;
    }
    field[n] = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_keep_their_numbers_and_text() {
        let id = iid::PLUGIN_FACTORY3;
        assert_eq!(
            uid_numbers(&id),
            [0x4555_A2AB, 0xC123_4E57, 0x9B12_2910, 0x3687_8931]
        );
        assert_eq!(uid_to_hex(&id), "4555A2ABC1234E579B12291036878931");
        assert_eq!(uid_from_hex("4555A2ABC1234E579B12291036878931"), Some(id));
        assert_eq!(uid_from_hex("not an id"), None);
        // COM's byte order on Windows: FUnknown is IUnknown.
        if cfg!(windows) {
            assert_eq!(
                iid::FUNKNOWN,
                [0, 0, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46]
            );
        }
    }

    #[test]
    fn structures_have_the_sdks_sizes() {
        assert_eq!(std::mem::size_of::<ParameterInfo>(), 792);
        assert_eq!(std::mem::size_of::<BusInfo>(), 276);
        assert_eq!(std::mem::size_of::<AudioBusBuffers>(), 24);
        assert_eq!(std::mem::size_of::<ProcessSetup>(), 24);
        assert_eq!(std::mem::size_of::<ProcessContext>(), 112);
        assert_eq!(std::mem::size_of::<ViewRect>(), 16);
        assert_eq!(std::mem::size_of::<PClassInfo>(), 116);
        assert_eq!(std::mem::size_of::<PClassInfo2>(), 440);
        assert_eq!(std::mem::size_of::<PFactoryInfo>(), 452);
    }

    #[test]
    fn texts_go_into_fields_and_back() {
        let mut f = [0u16; 8];
        fill16(&mut f, "Gain and more");
        assert_eq!(text_of16(&f), "Gain an");
        let mut c = [0 as c_char; 6];
        fill8(&mut c, "Fx|EQ");
        assert_eq!(text_of(&c), "Fx|EQ");
    }
}
