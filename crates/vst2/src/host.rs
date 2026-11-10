//! BananaV's side of the conversation with a plug-in: the callback it is
//! given, and what each instance keeps for it (the rate, the block, where
//! the timeline is, what its editor changed). The callback finds an
//! instance's data through the object's host field; before the plug-in has
//! one (while it is being made) it answers for the host alone.

#![allow(unsafe_code)]

use std::cell::{Cell, UnsafeCell};
use std::ffi::{CStr, c_char, c_void};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use crate::abi::{HOST_VERSION, PluginObject, TimeInfo, host_op};

/// What a plug-in's editor reported.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edit {
    /// A parameter set to a value (normalized), by its index.
    Perform(u32, f32),
    /// A gesture on a parameter began or ended.
    Begin(u32),
    End(u32),
    /// Its names, its programs or its channels changed.
    Changed,
}

/// Resizes the window an editor lives in (width, height).
pub type Resize = Box<dyn Fn(i32, i32) + Send>;

/// What an instance keeps for its plug-in's callbacks.
pub(crate) struct HostData {
    rate: AtomicU64,
    block: AtomicUsize,
    /// Written before each block, read by the plug-in while it processes
    /// (the same thread).
    time: UnsafeCell<TimeInfo>,
    edits: Mutex<Vec<Edit>>,
    resize: Mutex<Option<Resize>>,
    changed: AtomicBool,
}

// SAFETY: `time` is written by the thread that then calls the plug-in's
// processing, which reads it on that thread; the rest is atomic or locked.
unsafe impl Sync for HostData {}
// SAFETY: as above.
unsafe impl Send for HostData {}

impl std::fmt::Debug for HostData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostData")
            .field("rate", &self.rate())
            .field("block", &self.block())
            .finish()
    }
}

impl HostData {
    pub(crate) fn new() -> Box<HostData> {
        Box::new(HostData {
            rate: AtomicU64::new(48_000f64.to_bits()),
            block: AtomicUsize::new(1024),
            time: UnsafeCell::new(TimeInfo::default()),
            edits: Mutex::new(Vec::new()),
            resize: Mutex::new(None),
            changed: AtomicBool::new(false),
        })
    }

    pub(crate) fn rate(&self) -> f64 {
        f64::from_bits(self.rate.load(Ordering::Relaxed))
    }

    pub(crate) fn set_rate(&self, rate: f64) {
        self.rate.store(rate.to_bits(), Ordering::Relaxed);
    }

    pub(crate) fn block(&self) -> usize {
        self.block.load(Ordering::Relaxed)
    }

    pub(crate) fn set_block(&self, block: usize) {
        self.block.store(block, Ordering::Relaxed);
    }

    /// Where the timeline is for the next block.
    ///
    /// # Safety
    /// Called on the thread that calls the plug-in's processing, not while
    /// it runs.
    pub(crate) unsafe fn set_time(&self, t: TimeInfo) {
        // SAFETY: the caller's.
        unsafe { *self.time.get() = t };
    }

    pub(crate) fn take_edits(&self) -> Vec<Edit> {
        let mut e = std::mem::take(&mut *self.edits.lock().unwrap_or_else(|e| e.into_inner()));
        if self.changed.swap(false, Ordering::AcqRel) {
            e.push(Edit::Changed);
        }
        e
    }

    fn edit(&self, e: Edit) {
        self.edits.lock().unwrap_or_else(|e| e.into_inner()).push(e);
    }

    pub(crate) fn set_resize(&self, r: Option<Resize>) {
        *self.resize.lock().unwrap_or_else(|e| e.into_inner()) = r;
    }
}

thread_local! {
    /// The shell's plug-in being made on this thread (its number), asked
    /// for while its library's entry point runs.
    pub(crate) static CURRENT_ID: Cell<i32> = const { Cell::new(0) };
}

/// The host's name, as plug-ins ask for it.
const NAME: &[u8] = b"BananaV\0";

/// Writes `text` (with its zero) into a plug-in's buffer of at least 64
/// bytes.
///
/// # Safety
/// `ptr` is null or such a buffer.
unsafe fn write_text(ptr: *mut c_void, text: &[u8]) -> isize {
    if ptr.is_null() {
        return 0;
    }
    // SAFETY: the caller's; `text` is shorter than 64 bytes.
    unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), ptr.cast::<u8>(), text.len()) };
    1
}

/// What BananaV says it can do.
fn can_do(what: &str) -> isize {
    match what {
        "sendVstTimeInfo" | "sizeWindow" | "startStopProcess" | "supplyIdle" | "shellCategory" => 1,
        _ => 0,
    }
}

/// The instance data of `plugin`, when it has some.
///
/// # Safety
/// `plugin` is null or a plug-in's object whose host field is null or an
/// instance's data that outlives the call.
unsafe fn data<'a>(plugin: *mut PluginObject) -> Option<&'a HostData> {
    if plugin.is_null() {
        return None;
    }
    // SAFETY: the caller's.
    let raw = unsafe { (*plugin).host_data[0] };
    // SAFETY: as above: set by `Local` to its boxed data.
    (raw != 0).then(|| unsafe { &*(raw as *const HostData) })
}

/// The callback every plug-in is given.
pub(crate) unsafe extern "C" fn callback(
    plugin: *mut PluginObject,
    request: i32,
    index: i32,
    value: isize,
    ptr: *mut c_void,
    number: f32,
) -> isize {
    // Nothing may unwind into the plug-in.
    std::panic::catch_unwind(|| {
        // SAFETY: the plug-in's object (or null), as it calls.
        let d = unsafe { data(plugin) };
        match request {
            host_op::VERSION => HOST_VERSION,
            host_op::CURRENT_ID => CURRENT_ID.with(Cell::get) as isize,
            host_op::IDLE => 1,
            host_op::AUTOMATE => {
                if let Some(d) = d {
                    d.edit(Edit::Perform(index.max(0) as u32, number));
                }
                1
            }
            host_op::BEGIN_EDIT => {
                if let Some(d) = d {
                    d.edit(Edit::Begin(index.max(0) as u32));
                }
                1
            }
            host_op::END_EDIT => {
                if let Some(d) = d {
                    d.edit(Edit::End(index.max(0) as u32));
                }
                1
            }
            host_op::UPDATE_DISPLAY | host_op::IO_CHANGED => {
                if let Some(d) = d {
                    d.changed.store(true, Ordering::Release);
                }
                1
            }
            host_op::GET_TIME => d.map_or(0, |d| d.time.get() as isize),
            host_op::SAMPLE_RATE => d.map_or(48_000, |d| d.rate().round() as isize),
            host_op::BLOCK_SIZE => d.map_or(1024, |d| d.block() as isize),
            host_op::SIZE_WINDOW => {
                let r = d.map(|d| d.resize.lock().unwrap_or_else(|e| e.into_inner()));
                match r.as_deref() {
                    Some(Some(resize)) => {
                        resize(index, value as i32);
                        1
                    }
                    _ => 0,
                }
            }
            // Real time; offline renders run as fast as they can.
            host_op::PROCESS_LEVEL => 2,
            host_op::LANGUAGE => 1,
            // SAFETY: plug-ins give buffers of 64 bytes for these.
            host_op::VENDOR | host_op::PRODUCT => unsafe { write_text(ptr, NAME) },
            host_op::VENDOR_VERSION => 200,
            host_op::CAN_DO if !ptr.is_null() => {
                // SAFETY: a zero-ended text of the plug-in's.
                let what = unsafe { CStr::from_ptr(ptr as *const c_char) };
                can_do(&what.to_string_lossy())
            }
            _ => 0,
        }
    })
    .unwrap_or(0)
}
