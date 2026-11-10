//! A VST 2 plug-in written for the host's tests (never shipped), on
//! BananaV's own declarations of the interface (`bv-vst2-sys`):
//!
//! - **BananaV Test Gain 2** (number "BvG2"): stereo; Gain (0–2 times, 1 in
//!   the middle: "dB" text), Bypass, and two read-only parameters that tell
//!   what the last block's time information said (the tempo ÷ 1000, the
//!   place ÷ 10⁷ samples); three programs ("Unity", "Half", "Silent"); its
//!   state as a chunk of its own (gain, bypass, program); a tail of
//!   [`TAIL`] samples; on Windows an editor (a child window) that asks the
//!   host for another size once open.
//!
//! [`VSTPluginMainCrash`] makes one that crashes as it processes (a write
//! where nothing is); `BV_VST2_TESTPLUG_CRASH=process` makes every one do
//! that, `=start` ends the process as one is made: for the tests of the
//! host's guard, of looking at plug-ins in a process of their own and of
//! the 32-bit bridge.

#![allow(unsafe_code)]

use std::ffi::c_void;
use std::sync::Mutex;

use bv_vst2_sys::{
    HostCallback, MAGIC, PluginObject, Rect, TimeInfo, category, flags, host_op, op,
};

/// The plug-in's number ("BvG2").
pub const UNIQUE_ID: i32 = i32::from_be_bytes(*b"BvG2");
pub const GAIN: u32 = 0;
pub const BYPASS: u32 = 1;
pub const TEMPO_SEEN: u32 = 2;
pub const PLACE_SEEN: u32 = 3;
pub const PROGRAM_NAMES: [&str; 3] = ["Unity", "Half", "Silent"];
pub const PROGRAM_GAINS: [f32; 3] = [0.5, 0.25, 0.0];
pub const TAIL: isize = 4800;
pub const EDITOR_START: (i16, i16) = (320, 120);
pub const EDITOR_ASKED: (i32, i32) = (330, 130);

fn crash_wanted(what: &str) -> bool {
    std::env::var("BV_VST2_TESTPLUG_CRASH").as_deref() == Ok(what)
}

/// A crash: a write where nothing is.
fn crash() {
    // SAFETY: none — an access violation is the point.
    unsafe { std::ptr::write_volatile(16 as *mut u32, 1) };
}

#[derive(Debug)]
struct State {
    gain: f32,
    bypass: f32,
    program: usize,
    tempo: f64,
    place: f64,
    /// The chunk handed out last (kept until the next request).
    chunk: Vec<u8>,
    rect: Rect,
    #[cfg(windows)]
    view: isize,
}

/// The plug-in: its object first (the host holds a pointer to it).
#[repr(C)]
struct Plug {
    object: PluginObject,
    host: HostCallback,
    crashes: bool,
    state: Mutex<State>,
}

fn lock(p: &Plug) -> std::sync::MutexGuard<'_, State> {
    p.state.lock().unwrap_or_else(|e| e.into_inner())
}

/// The plug-in of an object.
///
/// # Safety
/// `o` is an object this library made, not freed.
unsafe fn plug<'a>(o: *mut PluginObject) -> &'a Plug {
    // SAFETY: the caller's; the object is the plug's first field.
    unsafe { &*o.cast::<Plug>() }
}

/// Writes `text` (with its zero) into the host's buffer.
///
/// # Safety
/// `ptr` is null or a buffer of at least 64 bytes.
unsafe fn write(ptr: *mut c_void, text: &str) -> isize {
    if ptr.is_null() {
        return 0;
    }
    let bytes = text.as_bytes();
    let n = bytes.len().min(63);
    // SAFETY: the caller's.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast::<u8>(), n);
        *ptr.cast::<u8>().add(n) = 0;
    }
    1
}

fn program_of(value: isize) -> usize {
    usize::try_from(value)
        .unwrap_or(0)
        .min(PROGRAM_NAMES.len() - 1)
}

unsafe extern "C" fn dispatcher(
    o: *mut PluginObject,
    request: i32,
    index: i32,
    value: isize,
    ptr: *mut c_void,
    number: f32,
) -> isize {
    // SAFETY: the host's call on an object of ours.
    let p = unsafe { plug(o) };
    match request {
        op::CLOSE => {
            #[cfg(windows)]
            view::close(&mut lock(p));
            // SAFETY: the box `make` gave away; the host is done with it.
            drop(unsafe { Box::from_raw(o.cast::<Plug>()) });
            1
        }
        op::SET_PROGRAM => {
            let mut st = lock(p);
            st.program = program_of(value);
            st.gain = PROGRAM_GAINS[st.program];
            1
        }
        op::GET_PROGRAM => lock(p).program as isize,
        // SAFETY: the host's buffers, for these requests.
        op::GET_PROGRAM_NAME => unsafe { write(ptr, PROGRAM_NAMES[lock(p).program]) },
        op::PROGRAM_NAME_INDEXED => match usize::try_from(index)
            .ok()
            .and_then(|i| PROGRAM_NAMES.get(i))
        {
            // SAFETY: as above.
            Some(name) => unsafe { write(ptr, name) },
            None => 0,
        },
        op::PARAM_NAME => {
            let name = ["Gain", "Bypass", "Tempo", "Place"]
                .get(index as usize)
                .copied()
                .unwrap_or("");
            // SAFETY: as above.
            unsafe { write(ptr, name) }
        }
        op::PARAM_LABEL => {
            let units = ["dB", "", "bpm", "samples"]
                .get(index as usize)
                .copied()
                .unwrap_or("");
            // SAFETY: as above.
            unsafe { write(ptr, units) }
        }
        op::PARAM_DISPLAY => {
            let st = lock(p);
            let text = match index as u32 {
                GAIN => {
                    let g = 2.0 * st.gain;
                    if g <= 1e-6 {
                        "-oo".to_string()
                    } else {
                        format!("{:.1}", 20.0 * g.log10())
                    }
                }
                BYPASS => if st.bypass >= 0.5 { "On" } else { "Off" }.to_string(),
                TEMPO_SEEN => format!("{:.1}", st.tempo),
                PLACE_SEEN => format!("{:.0}", st.place),
                _ => String::new(),
            };
            drop(st);
            // SAFETY: as above.
            unsafe { write(ptr, &text) }
        }
        // SAFETY: as above.
        op::EFFECT_NAME => unsafe { write(ptr, "BananaV Test Gain 2") },
        // SAFETY: as above.
        op::VENDOR | op::PRODUCT => unsafe { write(ptr, "BananaV Tests") },
        op::VENDOR_VERSION => 1000,
        op::PLUG_CATEGORY => category::EFFECT,
        op::TAIL_SIZE => TAIL,
        op::INTERFACE_VERSION => 2400,
        op::GET_CHUNK => {
            if ptr.is_null() {
                return 0;
            }
            let mut st = lock(p);
            let mut c = st.gain.to_le_bytes().to_vec();
            c.extend_from_slice(&st.bypass.to_le_bytes());
            c.extend_from_slice(&(st.program as u32).to_le_bytes());
            st.chunk = c;
            // SAFETY: the host's pointer to fill with the chunk's place.
            unsafe { *ptr.cast::<*mut u8>() = st.chunk.as_mut_ptr() };
            st.chunk.len() as isize
        }
        op::SET_CHUNK => {
            if ptr.is_null() || value < 12 {
                return 0;
            }
            // SAFETY: the host's chunk of `value` bytes.
            let c = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), value as usize) };
            let mut st = lock(p);
            st.gain = f32::from_le_bytes(c[0..4].try_into().unwrap_or_default());
            st.bypass = f32::from_le_bytes(c[4..8].try_into().unwrap_or_default());
            st.program =
                program_of(u32::from_le_bytes(c[8..12].try_into().unwrap_or_default()) as isize);
            1
        }
        op::EDITOR_RECT => {
            if ptr.is_null() {
                return 0;
            }
            let st = lock(p);
            let r = &st.rect as *const Rect;
            drop(st);
            // SAFETY: the host's pointer to fill; the rectangle lives with
            // the plug.
            unsafe { *ptr.cast::<*const Rect>() = r };
            1
        }
        #[cfg(windows)]
        op::EDITOR_OPEN => {
            let made = view::open(&mut lock(p), ptr as isize);
            if made {
                // Asks for another size, as editors do once open.
                // SAFETY: the host's callback.
                unsafe {
                    (p.host)(
                        o,
                        host_op::SIZE_WINDOW,
                        EDITOR_ASKED.0,
                        EDITOR_ASKED.1 as isize,
                        std::ptr::null_mut(),
                        0.0,
                    )
                };
                let mut st = lock(p);
                st.rect.right = EDITOR_ASKED.0 as i16;
                st.rect.bottom = EDITOR_ASKED.1 as i16;
            }
            isize::from(made)
        }
        #[cfg(windows)]
        op::EDITOR_CLOSE => {
            view::close(&mut lock(p));
            1
        }
        op::SET_SAMPLE_RATE => {
            let _ = number;
            1
        }
        _ => 0,
    }
}

unsafe extern "C" fn process(
    o: *mut PluginObject,
    ins: *mut *mut f32,
    outs: *mut *mut f32,
    frames: i32,
) {
    // SAFETY: the host's call on an object of ours.
    let p = unsafe { plug(o) };
    if p.crashes || crash_wanted("process") {
        crash();
    }
    // What the host says of the timeline.
    // SAFETY: the host's callback; its answer a pointer to its time
    // information (or none).
    let t = unsafe { (p.host)(o, host_op::GET_TIME, 0, 0, std::ptr::null_mut(), 0.0) };
    let k = {
        let mut st = lock(p);
        if t != 0 {
            // SAFETY: the host's time information.
            let info = unsafe { *(t as *const TimeInfo) };
            st.tempo = info.tempo;
            st.place = info.sample_pos;
        }
        if st.bypass >= 0.5 { 1.0 } else { 2.0 * st.gain }
    };
    for c in 0..2 {
        // SAFETY: the host's buffers, `frames` samples each.
        unsafe {
            let i = *ins.add(c);
            let out = *outs.add(c);
            for s in 0..frames.max(0) as usize {
                *out.add(s) = *i.add(s) * k;
            }
        }
    }
}

unsafe extern "C" fn set_parameter(o: *mut PluginObject, index: i32, value: f32) {
    // SAFETY: the host's call on an object of ours.
    let mut st = lock(unsafe { plug(o) });
    match index as u32 {
        GAIN => st.gain = value,
        BYPASS => st.bypass = value,
        _ => {}
    }
}

unsafe extern "C" fn get_parameter(o: *mut PluginObject, index: i32) -> f32 {
    // SAFETY: the host's call on an object of ours.
    let st = lock(unsafe { plug(o) });
    match index as u32 {
        GAIN => st.gain,
        BYPASS => st.bypass,
        TEMPO_SEEN => (st.tempo / 1000.0) as f32,
        PLACE_SEEN => (st.place / 1e7) as f32,
        _ => 0.0,
    }
}

fn make(host: HostCallback, crashes: bool) -> *mut PluginObject {
    if crash_wanted("start") {
        std::process::abort();
    }
    let plug = Box::new(Plug {
        object: PluginObject {
            magic: MAGIC,
            dispatcher: Some(dispatcher),
            process_adding: None,
            set_parameter: Some(set_parameter),
            get_parameter: Some(get_parameter),
            programs: PROGRAM_NAMES.len() as i32,
            params: 4,
            inputs: 2,
            outputs: 2,
            flags: flags::CAN_REPLACE
                | flags::PROGRAM_CHUNKS
                | if cfg!(windows) { flags::HAS_EDITOR } else { 0 },
            host_data: [0; 2],
            latency: 0,
            unused: [0; 2],
            unused_ratio: 1.0,
            object: std::ptr::null_mut(),
            user: std::ptr::null_mut(),
            unique_id: UNIQUE_ID,
            version: 1000,
            process: Some(process),
            process_f64: None,
            reserved: [0; 56],
        },
        host,
        crashes,
        state: Mutex::new(State {
            gain: 0.5,
            bypass: 0.0,
            program: 0,
            tempo: 0.0,
            place: 0.0,
            chunk: Vec::new(),
            rect: Rect {
                top: 0,
                left: 0,
                bottom: EDITOR_START.1,
                right: EDITOR_START.0,
            },
            #[cfg(windows)]
            view: 0,
        }),
    });
    Box::into_raw(plug).cast()
}

/// The plug-in's entry point.
///
/// # Safety
/// Called by a VST 2 host with its callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn VSTPluginMain(host: HostCallback) -> *mut PluginObject {
    make(host, false)
}

/// The entry point of the plug-in that crashes as it processes.
///
/// # Safety
/// As [`VSTPluginMain`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn VSTPluginMainCrash(host: HostCallback) -> *mut PluginObject {
    make(host, true)
}

#[cfg(windows)]
mod view {
    use super::State;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn CreateWindowExW(
            ex: u32,
            class: *const u16,
            name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: isize,
            menu: isize,
            instance: isize,
            param: *mut std::ffi::c_void,
        ) -> isize;
        fn DestroyWindow(hwnd: isize) -> i32;
    }

    const WS_CHILD: u32 = 0x4000_0000;
    const WS_VISIBLE: u32 = 0x1000_0000;

    /// A child window in `parent`; whether it was made.
    pub(super) fn open(st: &mut State, parent: isize) -> bool {
        if parent == 0 {
            return false;
        }
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // SAFETY: a child window of the host's window.
        st.view = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                0,
                0,
                i32::from(st.rect.right),
                i32::from(st.rect.bottom),
                parent,
                0,
                0,
                std::ptr::null_mut(),
            )
        };
        st.view != 0
    }

    pub(super) fn close(st: &mut State) {
        if st.view != 0 {
            // SAFETY: the window this plug-in made.
            unsafe { DestroyWindow(st.view) };
            st.view = 0;
        }
    }
}
