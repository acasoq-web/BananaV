//! VST 3 plug-ins written for the host's tests (never shipped), on the
//! interfaces of `bv-vst3-sys`:
//!
//! - **BananaV Test Gain**, one object that is its component, processor
//!   and controller: Gain (0–2 times, 1 in the middle: "dB" text), Bypass
//!   (the plug-in's bypass) and a hidden meter; mono or stereo; a 16-byte
//!   state; on Windows an editor (a child window) that asks its frame for
//!   another size when attached.
//! - **BananaV Test Polarity**, a component and a controller of their
//!   own: Invert; when connected the component sends the controller a
//!   message made by the host ("Hello", with an attribute "Version" of 3),
//!   which the controller shows as its hidden parameter "Hello" (0.3).
//! - **BananaV Test Programs**, the gain with programs ("Unity", "Half",
//!   "Silent": a program-change parameter and its unit's program list),
//!   a tail of [`TAIL`] samples, and a state that also keeps the program
//!   and what the last block's process context said (tempo, place in
//!   quarter notes and in samples, flags).
//!
//! The gains follow their parameters' changes at the samples they come
//! at. A module of its own ([`GetPluginFactoryCrash`]) makes **BananaV
//! Test Crash**, a gain that crashes as it processes (a write where nothing
//! is). `BV_VST3_TESTPLUG_CRASH=process` makes every gain do that,
//! `=start` ends the process as one starts: for the tests of the host's
//! guard and of describing plug-ins in a process of their own.

#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_void};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use bv_vst3_sys::{
    AUDIO, BUS_DEFAULT_ACTIVE, BusInfo, FUnknownVtbl, IAttributeListVtbl, IAudioProcessorVtbl,
    IBStreamVtbl, IComponentVtbl, IConnectionPointVtbl, IEditControllerVtbl, IHostApplicationVtbl,
    IMessageVtbl, INPUT, IParamValueQueueVtbl, IParameterChangesVtbl, IPlugFrameVtbl,
    IPlugViewVtbl, IPluginBaseVtbl, IPluginFactory2Vtbl, IPluginFactory3Vtbl, IPluginFactoryVtbl,
    IUnitInfoVtbl, NO_PROGRAM_LIST, PClassInfo, PClassInfo2, PClassInfoW, PFactoryInfo,
    ParameterInfo, ProcessData, ProcessSetup, ProgramListInfo, Raw, RoutingInfo, TResult, Tuid,
    UnitInfo, ViewRect, fill8, fill16, iid, param_flags, result, speaker, uid,
};

/// The classes' identifiers ("BANANAVTESTGAIN1"…).
pub const GAIN: Tuid = uid(0x4241_4E41, 0x4E41_5654, 0x4553_5447, 0x4149_4E31);
pub const POLARITY: Tuid = uid(0x4241_4E41, 0x4E41_5654, 0x4553_5450, 0x4F4C_4152);
pub const POLARITY_CONTROLLER: Tuid = uid(0x4241_4E41, 0x4E41_5654, 0x4553_5450, 0x4F4C_4354);
/// "BANANAVTESTPROGS".
pub const PROGRAMS: Tuid = uid(0x4241_4E41, 0x4E41_5654, 0x4553_5450, 0x524F_4753);
/// "BANANAVTESTCRASH".
pub const CRASH: Tuid = uid(0x4241_4E41, 0x4E41_5654, 0x4553_5443, 0x5241_5348);

/// Whether `BV_VST3_TESTPLUG_CRASH` asks for `what`.
fn crash_wanted(what: &str) -> bool {
    std::env::var("BV_VST3_TESTPLUG_CRASH").as_deref() == Ok(what)
}

/// A crash: a write where nothing is.
fn crash() {
    // SAFETY: none — an access violation is the point.
    unsafe { std::ptr::write_volatile(16 as *mut u32, 1) };
}

const PTR: usize = std::mem::size_of::<*const ()>();

/// An `extern "system"` function written in place (closures cannot be
/// one): `sys!(|a: Raw, b: i32| -> TResult { … })`.
macro_rules! sys {
    (|$($a:tt : $t:ty),*| $(-> $r:ty)? $body:block) => {{
        #[allow(unused_variables)]
        unsafe extern "system" fn f($($a: $t),*) $(-> $r)? $body
        f
    }};
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ---- COM objects ---------------------------------------------------------------

/// An object of the plug-in's: its tables' pointers first.
trait Object: Sized + Send + Sync + 'static {
    fn table(id: &Tuid) -> Option<usize>;
    fn refs(&self) -> &AtomicU32;
}

/// The object table `k` of a `T` belongs to.
///
/// # Safety
/// `raw` is table `k` of a live `T`.
unsafe fn this<'a, T: Object>(raw: Raw, k: usize) -> &'a T {
    // SAFETY: the caller's.
    unsafe { &*raw.cast::<u8>().sub(k * PTR).cast::<T>() }
}

unsafe extern "system" fn query<T: Object, const K: usize>(
    raw: Raw,
    id: *const Tuid,
    obj: *mut Raw,
) -> TResult {
    // SAFETY: a plug-in object's table `K`; `id` 16 bytes, `obj` a pointer.
    unsafe {
        let base = raw.cast::<u8>().sub(K * PTR).cast::<T>();
        let table = if *id == iid::FUNKNOWN {
            Some(0)
        } else {
            T::table(&*id)
        };
        match table {
            Some(i) => {
                (*base).refs().fetch_add(1, Ordering::AcqRel);
                *obj = base.cast::<u8>().add(i * PTR).cast();
                result::OK
            }
            None => {
                *obj = std::ptr::null_mut();
                result::NO_INTERFACE
            }
        }
    }
}

unsafe extern "system" fn add_ref<T: Object, const K: usize>(raw: Raw) -> u32 {
    // SAFETY: as `query`.
    unsafe { this::<T>(raw, K) }
        .refs()
        .fetch_add(1, Ordering::AcqRel)
        + 1
}

unsafe extern "system" fn release<T: Object, const K: usize>(raw: Raw) -> u32 {
    // SAFETY: as `query`; the last reference frees the box `new` made.
    unsafe {
        let base = raw.cast::<u8>().sub(K * PTR).cast::<T>();
        let left = (*base).refs().fetch_sub(1, Ordering::AcqRel) - 1;
        if left == 0 {
            drop(Box::from_raw(base));
        }
        left
    }
}

const fn unknown<T: Object, const K: usize>() -> FUnknownVtbl {
    FUnknownVtbl {
        query_interface: query::<T, K>,
        add_ref: add_ref::<T, K>,
        release: release::<T, K>,
    }
}

/// A new object, handed out with one reference (table `k`'s pointer).
fn new<T: Object>(obj: T, k: usize) -> Raw {
    obj.refs().store(1, Ordering::Release);
    let base = Box::into_raw(Box::new(obj));
    // SAFETY: the object's first fields are its tables' pointers.
    unsafe { base.cast::<u8>().add(k * PTR).cast() }
}

/// A host's object a plug-in holds (counted).
struct HostRef(Raw);

// SAFETY: host objects are for any thread.
unsafe impl Send for HostRef {}

impl HostRef {
    /// Holds `raw` (counting a reference).
    ///
    /// # Safety
    /// `raw` is null or a live interface pointer.
    unsafe fn hold(raw: Raw) -> Option<HostRef> {
        if raw.is_null() {
            return None;
        }
        // SAFETY: the caller's.
        unsafe { ((**(raw as *const *const FUnknownVtbl)).add_ref)(raw) };
        Some(HostRef(raw))
    }

    /// The interface's table, `V`.
    ///
    /// # Safety
    /// The object's interface is the one `V` is the table of.
    unsafe fn table<V>(&self) -> &V {
        // SAFETY: the caller's.
        unsafe { &**(self.0 as *const *const V) }
    }
}

impl Drop for HostRef {
    fn drop(&mut self) {
        // SAFETY: the reference `hold` counted.
        unsafe { ((**(self.0 as *const *const FUnknownVtbl)).release)(self.0) };
    }
}

// ---- Streams and parameter changes (the host's) -------------------------------------

/// Reads `n` bytes from a host's stream.
///
/// # Safety
/// `stream` is null or a host's IBStream.
unsafe fn read_bytes(stream: Raw, n: usize) -> Option<Vec<u8>> {
    if stream.is_null() {
        return None;
    }
    let mut buf = vec![0u8; n];
    let mut got = 0;
    // SAFETY: the caller's.
    let r = unsafe {
        ((**(stream as *const *const IBStreamVtbl)).read)(
            stream,
            buf.as_mut_ptr().cast(),
            n as i32,
            &mut got,
        )
    };
    (r == result::OK && got as usize == n).then_some(buf)
}

/// # Safety
/// As [`read_bytes`].
unsafe fn write_bytes(stream: Raw, bytes: &[u8]) -> TResult {
    if stream.is_null() {
        return result::INVALID_ARGUMENT;
    }
    let mut put = 0;
    // SAFETY: the caller's.
    unsafe {
        ((**(stream as *const *const IBStreamVtbl)).write)(
            stream,
            bytes.as_ptr().cast_mut().cast(),
            bytes.len() as i32,
            &mut put,
        )
    }
}

/// The last value of each parameter changed in a block.
///
/// # Safety
/// `changes` is null or a host's IParameterChanges.
unsafe fn last_changes(changes: Raw) -> Vec<(u32, f64)> {
    if changes.is_null() {
        return Vec::new();
    }
    let mut out = Vec::new();
    // SAFETY: the caller's, and the queues it gives.
    unsafe {
        let c = &**(changes as *const *const IParameterChangesVtbl);
        for i in 0..(c.get_parameter_count)(changes) {
            let q = (c.get_parameter_data)(changes, i);
            if q.is_null() {
                continue;
            }
            let qv = &**(q as *const *const IParamValueQueueVtbl);
            let n = (qv.get_point_count)(q);
            let (mut off, mut v) = (0, 0.0);
            if n > 0 && (qv.get_point)(q, n - 1, &mut off, &mut v) == result::OK {
                out.push(((qv.get_parameter_id)(q), v));
            }
        }
    }
    out
}

/// Every point of every parameter changed in a block: (sample in the
/// block, parameter, value), in the order of their samples.
///
/// # Safety
/// As [`last_changes`].
unsafe fn all_points(changes: Raw) -> Vec<(i32, u32, f64)> {
    if changes.is_null() {
        return Vec::new();
    }
    let mut out = Vec::new();
    // SAFETY: the caller's, and the queues it gives.
    unsafe {
        let c = &**(changes as *const *const IParameterChangesVtbl);
        for i in 0..(c.get_parameter_count)(changes) {
            let q = (c.get_parameter_data)(changes, i);
            if q.is_null() {
                continue;
            }
            let qv = &**(q as *const *const IParamValueQueueVtbl);
            let id = (qv.get_parameter_id)(q);
            for k in 0..(qv.get_point_count)(q) {
                let (mut off, mut v) = (0, 0.0);
                if (qv.get_point)(q, k, &mut off, &mut v) == result::OK {
                    out.push((off, id, v));
                }
            }
        }
    }
    out.sort_by_key(|p| p.0);
    out
}

fn channels_of(arrangement: u64) -> i32 {
    arrangement.count_ones() as i32
}

fn bus_info(info: *mut BusInfo, dir: i32, channels: i32, name: &str) -> TResult {
    if info.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: the host's structure to fill.
    let i = unsafe { &mut *info };
    i.media_type = AUDIO;
    i.direction = dir;
    i.channel_count = channels;
    fill16(&mut i.name, name);
    i.bus_type = bv_vst3_sys::MAIN;
    i.flags = BUS_DEFAULT_ACTIVE;
    result::OK
}

/// Processes the main buses, sample by sample.
///
/// # Safety
/// `data`'s buffers are the host's, `num_samples` long.
unsafe fn each_sample(data: &mut ProcessData, f: impl Fn(f32) -> f32) {
    if data.num_samples <= 0 || data.num_inputs < 1 || data.num_outputs < 1 {
        return;
    }
    // SAFETY: the caller's.
    unsafe {
        let (ins, outs) = (&*data.inputs, &*data.outputs);
        let n = ins.num_channels.min(outs.num_channels).max(0) as usize;
        for c in 0..n {
            let i = *ins.buffers.add(c);
            let o = *outs.buffers.add(c);
            for s in 0..data.num_samples as usize {
                *o.add(s) = f(*i.add(s));
            }
        }
    }
}

/// [`each_sample`] with each sample's place in the block.
///
/// # Safety
/// As [`each_sample`].
unsafe fn each_sample_at(data: &mut ProcessData, f: impl Fn(usize, f32) -> f32) {
    if data.num_samples <= 0 || data.num_inputs < 1 || data.num_outputs < 1 {
        return;
    }
    // SAFETY: the caller's.
    unsafe {
        let (ins, outs) = (&*data.inputs, &*data.outputs);
        let n = ins.num_channels.min(outs.num_channels).max(0) as usize;
        for c in 0..n {
            let i = *ins.buffers.add(c);
            let o = *outs.buffers.add(c);
            for s in 0..data.num_samples as usize {
                *o.add(s) = f(s, *i.add(s));
            }
        }
    }
}

fn param_info(
    out: *mut ParameterInfo,
    id: u32,
    title: &str,
    units: &str,
    steps: i32,
    default: f64,
    flags: i32,
) -> TResult {
    if out.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: the host's structure to fill.
    let p = unsafe { &mut *out };
    p.id = id;
    fill16(&mut p.title, title);
    fill16(&mut p.short_title, title);
    fill16(&mut p.units, units);
    p.step_count = steps;
    p.default_normalized_value = default;
    p.unit_id = 0;
    p.flags = flags;
    result::OK
}

fn text_out(out: *mut u16, text: &str) -> TResult {
    if out.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: a String128 to fill.
    fill16(unsafe { &mut *out.cast::<[u16; 128]>() }, text);
    result::OK
}

fn sample32(size: i32) -> TResult {
    if size == bv_vst3_sys::SAMPLE32 {
        result::OK
    } else {
        result::FALSE
    }
}

// ---- The factory -----------------------------------------------------------------

#[repr(C)]
struct Factory {
    table: &'static IPluginFactory3Vtbl,
    refs: AtomicU32,
    /// The classes it lists.
    classes: &'static [Class],
}

impl Object for Factory {
    fn table(id: &Tuid) -> Option<usize> {
        (*id == iid::PLUGIN_FACTORY || *id == iid::PLUGIN_FACTORY2 || *id == iid::PLUGIN_FACTORY3)
            .then_some(0)
    }

    fn refs(&self) -> &AtomicU32 {
        &self.refs
    }
}

/// A class: identifier, category, name, sub-categories.
type Class = (Tuid, &'static str, &'static str, &'static str);

/// The test module's classes.
const CLASSES: [Class; 4] = [
    (GAIN, "Audio Module Class", "BananaV Test Gain", "Fx|Tools"),
    (
        POLARITY,
        "Audio Module Class",
        "BananaV Test Polarity",
        "Fx|Tools",
    ),
    (
        POLARITY_CONTROLLER,
        "Component Controller Class",
        "BananaV Test Polarity Controller",
        "",
    ),
    (
        PROGRAMS,
        "Audio Module Class",
        "BananaV Test Programs",
        "Fx|Tools",
    ),
];

/// The crash module's.
const CRASH_CLASSES: [Class; 1] = [(CRASH, "Audio Module Class", "BananaV Test Crash", "Fx")];

/// Class `i` of factory `raw`.
///
/// # Safety
/// `raw` is a factory of this library's.
unsafe fn class(raw: Raw, i: i32) -> Option<Class> {
    // SAFETY: the caller's.
    let f = unsafe { this::<Factory>(raw, 0) };
    usize::try_from(i)
        .ok()
        .and_then(|i| f.classes.get(i))
        .copied()
}

static FACTORY: IPluginFactory3Vtbl = IPluginFactory3Vtbl {
    factory2: IPluginFactory2Vtbl {
        factory: IPluginFactoryVtbl {
            unknown: unknown::<Factory, 0>(),
            get_factory_info: sys!(|_t: Raw, info: *mut PFactoryInfo| -> TResult {
                if info.is_null() {
                    return result::INVALID_ARGUMENT;
                }
                // SAFETY: the host's structure to fill.
                let i = unsafe { &mut *info };
                fill8(&mut i.vendor, "BananaV Tests");
                fill8(&mut i.url, "");
                fill8(&mut i.email, "");
                i.flags = 1 << 4;
                result::OK
            }),
            count_classes: sys!(|t: Raw| -> i32 {
                // SAFETY: the host's call on a factory of this library's.
                unsafe { this::<Factory>(t, 0) }.classes.len() as i32
            }),
            get_class_info: sys!(|t: Raw, i: i32, info: *mut PClassInfo| -> TResult {
                // SAFETY: as above.
                let class = unsafe { class(t, i) };
                let (Some((cid, cat, name, _)), false) = (class, info.is_null()) else {
                    return result::INVALID_ARGUMENT;
                };
                // SAFETY: the host's structure to fill.
                let c = unsafe { &mut *info };
                c.cid = cid;
                c.cardinality = 0x7FFF_FFFF;
                fill8(&mut c.category, cat);
                fill8(&mut c.name, name);
                result::OK
            }),
            create_instance: sys!(|_t: Raw,
                                   cid: *const c_char,
                                   id: *const c_char,
                                   obj: *mut Raw|
             -> TResult {
                if cid.is_null() || id.is_null() || obj.is_null() {
                    return result::INVALID_ARGUMENT;
                }
                // SAFETY: two 16-byte identifiers; `obj` a pointer to fill.
                unsafe {
                    let cid = *cid.cast::<Tuid>();
                    let made = if cid == GAIN {
                        new(Gain::new(Kind::Gain), 0)
                    } else if cid == PROGRAMS {
                        new(Gain::new(Kind::Programs), 0)
                    } else if cid == CRASH {
                        new(Gain::new(Kind::Crash), 0)
                    } else if cid == POLARITY {
                        new(Polarity::default(), 0)
                    } else if cid == POLARITY_CONTROLLER {
                        new(PolarityController::default(), 0)
                    } else {
                        *obj = std::ptr::null_mut();
                        return result::FALSE;
                    };
                    // The interface asked for, from the new object's first
                    // table; then the object is the caller's alone.
                    let table = &**(made as *const *const FUnknownVtbl);
                    let r = (table.query_interface)(made, id.cast(), obj);
                    (table.release)(made);
                    r
                }
            }),
        },
        get_class_info2: sys!(|t: Raw, i: i32, info: *mut PClassInfo2| -> TResult {
            // SAFETY: the host's call on a factory of this library's.
            let class = unsafe { class(t, i) };
            let (Some((cid, cat, name, subs)), false) = (class, info.is_null()) else {
                return result::INVALID_ARGUMENT;
            };
            // SAFETY: the host's structure to fill.
            let c = unsafe { &mut *info };
            c.cid = cid;
            c.cardinality = 0x7FFF_FFFF;
            fill8(&mut c.category, cat);
            fill8(&mut c.name, name);
            c.class_flags = 0;
            fill8(&mut c.sub_categories, subs);
            fill8(&mut c.vendor, "");
            fill8(&mut c.version, "1.0.0");
            fill8(&mut c.sdk_version, "VST 3.8.1");
            result::OK
        }),
    },
    get_class_info_unicode: sys!(|t: Raw, i: i32, info: *mut PClassInfoW| -> TResult {
        // SAFETY: the host's call on a factory of this library's.
        let class = unsafe { class(t, i) };
        let (Some((cid, cat, name, subs)), false) = (class, info.is_null()) else {
            return result::INVALID_ARGUMENT;
        };
        // SAFETY: the host's structure to fill.
        let c = unsafe { &mut *info };
        c.cid = cid;
        c.cardinality = 0x7FFF_FFFF;
        fill8(&mut c.category, cat);
        fill16(&mut c.name, name);
        c.class_flags = 0;
        fill8(&mut c.sub_categories, subs);
        fill16(&mut c.vendor, "");
        fill16(&mut c.version, "1.0.0");
        fill16(&mut c.sdk_version, "VST 3.8.1");
        result::OK
    }),
    set_host_context: sys!(|_t: Raw, _c: Raw| -> TResult { result::OK }),
};

/// The plug-ins' entry point.
///
/// # Safety
/// Called by a VST 3 host.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn GetPluginFactory() -> Raw {
    new(
        Factory {
            table: &FACTORY,
            refs: AtomicU32::new(1),
            classes: &CLASSES,
        },
        0,
    )
}

/// The crash module's entry point (a module of its own: one that crashes
/// is switched off, its neighbours not).
///
/// # Safety
/// Called by a VST 3 host.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn GetPluginFactoryCrash() -> Raw {
    new(
        Factory {
            table: &FACTORY,
            refs: AtomicU32::new(1),
            classes: &CRASH_CLASSES,
        },
        0,
    )
}

/// Windows' module entry and exit (optional ones).
///
/// # Safety
/// Called by a VST 3 host.
#[cfg(windows)]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn InitDll() -> u8 {
    1
}

/// # Safety
/// Called by a VST 3 host.
#[cfg(windows)]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn ExitDll() -> u8 {
    1
}

/// Linux's module entry and exit (required ones).
///
/// # Safety
/// Called by a VST 3 host.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn ModuleEntry(_handle: *mut c_void) -> u8 {
    1
}

/// # Safety
/// Called by a VST 3 host.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn ModuleExit() -> u8 {
    1
}

// ---- BananaV Test Gain ----------------------------------------------------------------

/// Its parameters.
pub const GAIN_ID: u32 = 0;
pub const BYPASS_ID: u32 = 1;
pub const METER_ID: u32 = 2;
/// BananaV Test Programs' program (its root unit's program list).
pub const PROGRAM_ID: u32 = 3;
pub const PROGRAM_LIST: i32 = 7;
pub const PROGRAM_NAMES: [&str; 3] = ["Unity", "Half", "Silent"];
/// Each program's gain (normalized).
pub const PROGRAM_GAINS: [f64; 3] = [0.5, 0.25, 0.0];
/// BananaV Test Programs' tail (samples).
pub const TAIL: u32 = 4800;

/// Which gain an object is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Gain,
    Programs,
    Crash,
}

/// What the last block's process context said: tempo, place in quarter
/// notes and in samples, its flags.
#[derive(Clone, Copy, Default)]
struct Seen {
    tempo: f64,
    music: f64,
    samples: i64,
    flags: u32,
}

struct GainState {
    gain: f64,
    bypass: f64,
    /// The program chosen (normalized).
    program: f64,
    seen: Seen,
    arrangement: u64,
    host: Option<HostRef>,
    handler: Option<HostRef>,
}

impl Default for GainState {
    fn default() -> GainState {
        GainState {
            gain: 0.5,
            bypass: 0.0,
            program: 0.0,
            seen: Seen::default(),
            arrangement: speaker::STEREO,
            host: None,
            handler: None,
        }
    }
}

impl GainState {
    /// A parameter's new value, as the processor and the controller take
    /// it: a program brings its gain.
    fn set(&mut self, id: u32, v: f64) -> bool {
        match id {
            GAIN_ID => self.gain = v,
            BYPASS_ID => self.bypass = v,
            PROGRAM_ID => {
                self.program = v;
                self.gain = PROGRAM_GAINS[program_index(v)];
            }
            _ => return false,
        }
        true
    }
}

/// The program a normalized value chooses.
fn program_index(v: f64) -> usize {
    let last = PROGRAM_NAMES.len() - 1;
    ((v.clamp(0.0, 1.0) * last as f64).round() as usize).min(last)
}

#[repr(C)]
struct Gain {
    component: &'static IComponentVtbl,
    processor: &'static IAudioProcessorVtbl,
    controller: &'static IEditControllerVtbl,
    units: &'static IUnitInfoVtbl,
    refs: AtomicU32,
    kind: Kind,
    state: Mutex<GainState>,
}

impl Gain {
    fn new(kind: Kind) -> Gain {
        Gain {
            component: &GAIN_COMPONENT,
            processor: &GAIN_PROCESSOR,
            controller: &GAIN_CONTROLLER,
            units: &GAIN_UNITS,
            refs: AtomicU32::new(1),
            kind,
            state: Mutex::new(GainState::default()),
        }
    }
}

impl Object for Gain {
    fn table(id: &Tuid) -> Option<usize> {
        if *id == iid::PLUGIN_BASE || *id == iid::COMPONENT {
            Some(0)
        } else if *id == iid::AUDIO_PROCESSOR {
            Some(1)
        } else if *id == iid::EDIT_CONTROLLER {
            Some(2)
        } else if *id == iid::UNIT_INFO {
            Some(3)
        } else {
            None
        }
    }

    fn refs(&self) -> &AtomicU32 {
        &self.refs
    }
}

/// Which gain table `k` of `raw` belongs to.
///
/// # Safety
/// `raw` is a gain's table `k`.
unsafe fn kind(raw: Raw, k: usize) -> Kind {
    // SAFETY: the caller's.
    unsafe { this::<Gain>(raw, k) }.kind
}

/// The gain's state from table `k`.
///
/// # Safety
/// `raw` is a gain's table `k`.
unsafe fn gain<'a>(raw: Raw, k: usize) -> std::sync::MutexGuard<'a, GainState> {
    // SAFETY: the caller's.
    lock(&unsafe { this::<Gain>(raw, k) }.state)
}

/// Reads the gain's state from a host's stream.
///
/// # Safety
/// `raw` is a gain's table `k`; `stream` a host's stream.
unsafe fn gain_read(raw: Raw, k: usize, stream: Raw) -> TResult {
    // The gain and its bypass; the program too for BananaV Test Programs
    // (what its context said is not read back).
    // SAFETY: the caller's.
    let programs = unsafe { kind(raw, k) } == Kind::Programs;
    // SAFETY: the caller's.
    let Some(b) = (unsafe { read_bytes(stream, if programs { 24 } else { 16 }) }) else {
        return result::FALSE;
    };
    let at = |i: usize| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap_or_default());
    // SAFETY: the caller's.
    let mut st = unsafe { gain(raw, k) };
    st.gain = at(0);
    st.bypass = at(1);
    if programs {
        st.program = at(2);
    }
    result::OK
}

static GAIN_COMPONENT: IComponentVtbl = IComponentVtbl {
    base: IPluginBaseVtbl {
        unknown: unknown::<Gain, 0>(),
        initialize: sys!(|raw: Raw, ctx: Raw| -> TResult {
            if crash_wanted("start") {
                std::process::abort();
            }
            // SAFETY: the host's call on table 0; its context counted.
            unsafe { gain(raw, 0).host = HostRef::hold(ctx) };
            result::OK
        }),
        terminate: sys!(|raw: Raw| -> TResult {
            // SAFETY: the host's call on table 0.
            let mut st = unsafe { gain(raw, 0) };
            st.host = None;
            st.handler = None;
            result::OK
        }),
    },
    get_controller_class_id: sys!(|_t: Raw, _c: *mut Tuid| -> TResult { result::FALSE }),
    set_io_mode: sys!(|_t: Raw, _m: i32| -> TResult { result::OK }),
    get_bus_count: sys!(|_t: Raw, ty: i32, _d: i32| -> i32 { i32::from(ty == AUDIO) }),
    get_bus_info: sys!(
        |raw: Raw, ty: i32, dir: i32, i: i32, info: *mut BusInfo| -> TResult {
            if ty != AUDIO || i != 0 {
                return result::INVALID_ARGUMENT;
            }
            // SAFETY: the host's call on table 0.
            let ch = channels_of(unsafe { gain(raw, 0) }.arrangement);
            bus_info(info, dir, ch, if dir == INPUT { "Input" } else { "Output" })
        }
    ),
    get_routing_info: sys!(
        |_t: Raw, _i: *mut RoutingInfo, _o: *mut RoutingInfo| -> TResult { result::FALSE }
    ),
    activate_bus: sys!(|_t: Raw, _a: i32, _b: i32, _c: i32, _d: u8| -> TResult { result::OK }),
    set_active: sys!(|_t: Raw, _s: u8| -> TResult { result::OK }),
    set_state: sys!(|raw: Raw, stream: Raw| -> TResult {
        // SAFETY: the host's call on table 0, with its stream.
        unsafe { gain_read(raw, 0, stream) }
    }),
    get_state: sys!(|raw: Raw, stream: Raw| -> TResult {
        // SAFETY: as above.
        unsafe {
            let programs = kind(raw, 0) == Kind::Programs;
            let st = gain(raw, 0);
            let mut b = st.gain.to_le_bytes().to_vec();
            b.extend_from_slice(&st.bypass.to_le_bytes());
            if programs {
                let s = st.seen;
                b.extend_from_slice(&st.program.to_le_bytes());
                b.extend_from_slice(&s.tempo.to_le_bytes());
                b.extend_from_slice(&s.music.to_le_bytes());
                b.extend_from_slice(&s.samples.to_le_bytes());
                b.extend_from_slice(&u64::from(s.flags).to_le_bytes());
            }
            drop(st);
            write_bytes(stream, &b)
        }
    }),
};

static GAIN_PROCESSOR: IAudioProcessorVtbl = IAudioProcessorVtbl {
    unknown: unknown::<Gain, 1>(),
    set_bus_arrangements: sys!(|raw: Raw,
                                ins: *mut u64,
                                n_in: i32,
                                outs: *mut u64,
                                n_out: i32|
     -> TResult {
        if n_in != 1 || n_out != 1 || ins.is_null() || outs.is_null() {
            return result::FALSE;
        }
        // SAFETY: the host's arrays of one; the host's call on table 1.
        unsafe {
            let (i, o) = (*ins, *outs);
            if i == o && (i == speaker::MONO || i == speaker::STEREO) {
                gain(raw, 1).arrangement = i;
                result::OK
            } else {
                result::FALSE
            }
        }
    }),
    get_bus_arrangement: sys!(|raw: Raw, _d: i32, i: i32, arr: *mut u64| -> TResult {
        if i != 0 || arr.is_null() {
            return result::INVALID_ARGUMENT;
        }
        // SAFETY: the host's call on table 1; `arr` to fill.
        unsafe { *arr = gain(raw, 1).arrangement };
        result::OK
    }),
    can_process_sample_size: sys!(|_t: Raw, size: i32| -> TResult { sample32(size) }),
    get_latency_samples: sys!(|_t: Raw| -> u32 { 0 }),
    setup_processing: sys!(|_t: Raw, _s: *mut ProcessSetup| -> TResult { result::OK }),
    set_processing: sys!(|_t: Raw, _s: u8| -> TResult { result::OK }),
    process: sys!(|raw: Raw, data: *mut ProcessData| -> TResult {
        if data.is_null() {
            return result::INVALID_ARGUMENT;
        }
        // SAFETY: the host's call on table 1 with its data.
        if unsafe { kind(raw, 1) } == Kind::Crash || crash_wanted("process") {
            crash();
        }
        // SAFETY: the host's call on table 1 with its data.
        unsafe {
            let data = &mut *data;
            let n = data.num_samples.max(0) as usize;
            // Each sample's factor, the parameters changed at the samples
            // their points give.
            let mut factors = Vec::with_capacity(n);
            let mut st = gain(raw, 1);
            if !data.process_context.is_null() {
                let c = &*data.process_context;
                st.seen = Seen {
                    tempo: c.tempo,
                    music: c.project_time_music,
                    samples: c.project_time_samples,
                    flags: c.state,
                };
            }
            let points = all_points(data.input_parameter_changes);
            let mut next = points.iter().peekable();
            for s in 0..n {
                while let Some(&&(at, id, v)) = next.peek() {
                    if at as usize > s {
                        break;
                    }
                    st.set(id, v);
                    next.next();
                }
                factors.push(if st.bypass >= 0.5 {
                    1.0
                } else {
                    (2.0 * st.gain) as f32
                });
            }
            // Points past the block (or a block of no samples).
            for &(_, id, v) in next {
                st.set(id, v);
            }
            drop(st);
            each_sample_at(data, |s, x| x * factors[s]);
        }
        result::OK
    }),
    get_tail_samples: sys!(|raw: Raw| -> u32 {
        // SAFETY: the host's call on table 1.
        if unsafe { kind(raw, 1) } == Kind::Programs {
            TAIL
        } else {
            0
        }
    }),
};

static GAIN_CONTROLLER: IEditControllerVtbl = IEditControllerVtbl {
    base: IPluginBaseVtbl {
        unknown: unknown::<Gain, 2>(),
        // One object: the component's start and end are its own.
        initialize: sys!(|_t: Raw, _c: Raw| -> TResult { result::OK }),
        terminate: sys!(|_t: Raw| -> TResult { result::OK }),
    },
    set_component_state: sys!(|raw: Raw, stream: Raw| -> TResult {
        // SAFETY: the host's call on table 2, with its stream.
        unsafe { gain_read(raw, 2, stream) }
    }),
    set_state: sys!(|_t: Raw, _s: Raw| -> TResult { result::OK }),
    get_state: sys!(|_t: Raw, _s: Raw| -> TResult { result::OK }),
    get_parameter_count: sys!(|raw: Raw| -> i32 {
        // SAFETY: the host's call on table 2.
        if unsafe { kind(raw, 2) } == Kind::Programs {
            4
        } else {
            3
        }
    }),
    get_parameter_info: sys!(|raw: Raw, i: i32, out: *mut ParameterInfo| -> TResult {
        // SAFETY: the host's call on table 2.
        let programs = unsafe { kind(raw, 2) } == Kind::Programs;
        match i {
            3 if programs => param_info(
                out,
                PROGRAM_ID,
                "Program",
                "",
                PROGRAM_NAMES.len() as i32 - 1,
                0.0,
                param_flags::CAN_AUTOMATE | param_flags::LIST | param_flags::PROGRAM_CHANGE,
            ),
            0 => param_info(
                out,
                GAIN_ID,
                "Gain",
                "dB",
                0,
                0.5,
                param_flags::CAN_AUTOMATE,
            ),
            1 => param_info(
                out,
                BYPASS_ID,
                "Bypass",
                "",
                1,
                0.0,
                param_flags::CAN_AUTOMATE | param_flags::BYPASS,
            ),
            2 => param_info(
                out,
                METER_ID,
                "Meter",
                "",
                0,
                0.0,
                param_flags::READ_ONLY | param_flags::HIDDEN,
            ),
            _ => result::INVALID_ARGUMENT,
        }
    }),
    get_param_string_by_value: sys!(|_t: Raw, id: u32, v: f64, out: *mut u16| -> TResult {
        match id {
            GAIN_ID => {
                let g = 2.0 * v;
                let text = if g <= 1e-6 {
                    "-oo".to_string()
                } else {
                    format!("{:.1}", 20.0 * g.log10())
                };
                text_out(out, &text)
            }
            BYPASS_ID => text_out(out, if v >= 0.5 { "On" } else { "Off" }),
            PROGRAM_ID => text_out(out, PROGRAM_NAMES[program_index(v)]),
            _ => text_out(out, ""),
        }
    }),
    get_param_value_by_string: sys!(
        |_t: Raw, _i: u32, _s: *const u16, _v: *mut f64| -> TResult { result::FALSE }
    ),
    normalized_param_to_plain: sys!(|_t: Raw, _i: u32, v: f64| -> f64 { v }),
    plain_param_to_normalized: sys!(|_t: Raw, _i: u32, v: f64| -> f64 { v }),
    get_param_normalized: sys!(|raw: Raw, id: u32| -> f64 {
        // SAFETY: the host's call on table 2.
        let st = unsafe { gain(raw, 2) };
        match id {
            GAIN_ID => st.gain,
            BYPASS_ID => st.bypass,
            PROGRAM_ID => st.program,
            _ => 0.0,
        }
    }),
    set_param_normalized: sys!(|raw: Raw, id: u32, v: f64| -> TResult {
        // SAFETY: the host's call on table 2.
        let mut st = unsafe { gain(raw, 2) };
        if st.set(id, v) {
            result::OK
        } else {
            result::FALSE
        }
    }),
    set_component_handler: sys!(|raw: Raw, h: Raw| -> TResult {
        // SAFETY: the host's call on table 2; its handler counted.
        unsafe { gain(raw, 2).handler = HostRef::hold(h) };
        result::OK
    }),
    create_view: sys!(|_t: Raw, name: *const c_char| -> Raw {
        // SAFETY: a NUL-ended name.
        if name.is_null() || unsafe { CStr::from_ptr(name) } != bv_vst3_sys::VIEW_EDITOR {
            return std::ptr::null_mut();
        }
        view::make()
    }),
};

/// The gains' units: the root alone; BananaV Test Programs' has its
/// program list.
static GAIN_UNITS: IUnitInfoVtbl = IUnitInfoVtbl {
    unknown: unknown::<Gain, 3>(),
    get_unit_count: sys!(|_t: Raw| -> i32 { 1 }),
    get_unit_info: sys!(|raw: Raw, i: i32, out: *mut UnitInfo| -> TResult {
        if i != 0 || out.is_null() {
            return result::INVALID_ARGUMENT;
        }
        // SAFETY: the host's call on table 3, a structure to fill.
        unsafe {
            let programs = kind(raw, 3) == Kind::Programs;
            let u = &mut *out;
            u.id = bv_vst3_sys::ROOT_UNIT;
            u.parent_unit_id = -1;
            fill16(&mut u.name, "Root");
            u.program_list_id = if programs {
                PROGRAM_LIST
            } else {
                NO_PROGRAM_LIST
            };
        }
        result::OK
    }),
    get_program_list_count: sys!(|raw: Raw| -> i32 {
        // SAFETY: the host's call on table 3.
        i32::from(unsafe { kind(raw, 3) } == Kind::Programs)
    }),
    get_program_list_info: sys!(|raw: Raw, i: i32, out: *mut ProgramListInfo| -> TResult {
        // SAFETY: the host's call on table 3.
        if i != 0 || out.is_null() || unsafe { kind(raw, 3) } != Kind::Programs {
            return result::INVALID_ARGUMENT;
        }
        // SAFETY: a structure to fill.
        let l = unsafe { &mut *out };
        l.id = PROGRAM_LIST;
        fill16(&mut l.name, "Factory");
        l.program_count = PROGRAM_NAMES.len() as i32;
        result::OK
    }),
    get_program_name: sys!(|_t: Raw, list: i32, i: i32, out: *mut u16| -> TResult {
        let name = usize::try_from(i).ok().and_then(|i| PROGRAM_NAMES.get(i));
        match name {
            Some(name) if list == PROGRAM_LIST => text_out(out, name),
            _ => result::INVALID_ARGUMENT,
        }
    }),
    get_program_info: sys!(|_t: Raw,
                            _l: i32,
                            _i: i32,
                            _a: *const c_char,
                            _o: *mut u16|
     -> TResult { result::FALSE }),
    has_program_pitch_names: sys!(|_t: Raw, _l: i32, _i: i32| -> TResult { result::FALSE }),
    get_program_pitch_name: sys!(
        |_t: Raw, _l: i32, _i: i32, _p: i16, _o: *mut u16| -> TResult { result::FALSE }
    ),
    get_selected_unit: sys!(|_t: Raw| -> i32 { bv_vst3_sys::ROOT_UNIT }),
    select_unit: sys!(|_t: Raw, _u: i32| -> TResult { result::OK }),
    get_unit_by_bus: sys!(|_t: Raw,
                           _ty: i32,
                           _d: i32,
                           _b: i32,
                           _c: i32,
                           _u: *mut i32|
     -> TResult { result::FALSE }),
    set_unit_program_data: sys!(|_t: Raw, _l: i32, _i: i32, _d: Raw| -> TResult { result::FALSE }),
};

// ---- BananaV Test Polarity --------------------------------------------------------------

pub const INVERT_ID: u32 = 0;
pub const HELLO_ID: u32 = 1;

#[derive(Default)]
struct PolarityState {
    invert: f64,
    host: Option<HostRef>,
    peer: Option<HostRef>,
}

#[repr(C)]
struct Polarity {
    component: &'static IComponentVtbl,
    processor: &'static IAudioProcessorVtbl,
    point: &'static IConnectionPointVtbl,
    refs: AtomicU32,
    state: Mutex<PolarityState>,
}

impl Default for Polarity {
    fn default() -> Polarity {
        Polarity {
            component: &POLARITY_COMPONENT,
            processor: &POLARITY_PROCESSOR,
            point: &POLARITY_POINT,
            refs: AtomicU32::new(1),
            state: Mutex::new(PolarityState::default()),
        }
    }
}

impl Object for Polarity {
    fn table(id: &Tuid) -> Option<usize> {
        if *id == iid::PLUGIN_BASE || *id == iid::COMPONENT {
            Some(0)
        } else if *id == iid::AUDIO_PROCESSOR {
            Some(1)
        } else if *id == iid::CONNECTION_POINT {
            Some(2)
        } else {
            None
        }
    }

    fn refs(&self) -> &AtomicU32 {
        &self.refs
    }
}

/// The polarity component's state from table `k`.
///
/// # Safety
/// `raw` is a polarity component's table `k`.
unsafe fn polarity<'a>(raw: Raw, k: usize) -> std::sync::MutexGuard<'a, PolarityState> {
    // SAFETY: the caller's.
    lock(&unsafe { this::<Polarity>(raw, k) }.state)
}

static POLARITY_COMPONENT: IComponentVtbl = IComponentVtbl {
    base: IPluginBaseVtbl {
        unknown: unknown::<Polarity, 0>(),
        initialize: sys!(|raw: Raw, ctx: Raw| -> TResult {
            // SAFETY: the host's call on table 0; its context counted.
            unsafe { polarity(raw, 0).host = HostRef::hold(ctx) };
            result::OK
        }),
        terminate: sys!(|raw: Raw| -> TResult {
            // SAFETY: the host's call on table 0.
            let mut st = unsafe { polarity(raw, 0) };
            st.host = None;
            st.peer = None;
            result::OK
        }),
    },
    get_controller_class_id: sys!(|_t: Raw, out: *mut Tuid| -> TResult {
        if out.is_null() {
            return result::INVALID_ARGUMENT;
        }
        // SAFETY: 16 bytes to fill.
        unsafe { *out = POLARITY_CONTROLLER };
        result::OK
    }),
    set_io_mode: sys!(|_t: Raw, _m: i32| -> TResult { result::OK }),
    get_bus_count: sys!(|_t: Raw, ty: i32, _d: i32| -> i32 { i32::from(ty == AUDIO) }),
    get_bus_info: sys!(
        |_t: Raw, ty: i32, dir: i32, i: i32, info: *mut BusInfo| -> TResult {
            if ty != AUDIO || i != 0 {
                return result::INVALID_ARGUMENT;
            }
            bus_info(info, dir, 2, "Main")
        }
    ),
    get_routing_info: sys!(
        |_t: Raw, _i: *mut RoutingInfo, _o: *mut RoutingInfo| -> TResult { result::FALSE }
    ),
    activate_bus: sys!(|_t: Raw, _a: i32, _b: i32, _c: i32, _d: u8| -> TResult { result::OK }),
    set_active: sys!(|_t: Raw, _s: u8| -> TResult { result::OK }),
    set_state: sys!(|raw: Raw, stream: Raw| -> TResult {
        // SAFETY: the host's call on table 0, with its stream.
        unsafe {
            let Some(b) = read_bytes(stream, 8) else {
                return result::FALSE;
            };
            polarity(raw, 0).invert = f64::from_le_bytes(b.try_into().unwrap_or_default());
        }
        result::OK
    }),
    get_state: sys!(|raw: Raw, stream: Raw| -> TResult {
        // SAFETY: as above.
        unsafe {
            let invert = polarity(raw, 0).invert;
            write_bytes(stream, &invert.to_le_bytes())
        }
    }),
};

static POLARITY_PROCESSOR: IAudioProcessorVtbl = IAudioProcessorVtbl {
    unknown: unknown::<Polarity, 1>(),
    // Stereo only.
    set_bus_arrangements: sys!(|_t: Raw,
                                ins: *mut u64,
                                n_in: i32,
                                outs: *mut u64,
                                n_out: i32|
     -> TResult {
        // SAFETY: the host's arrays of `n_in` and `n_out`.
        let stereo = n_in == 1
            && n_out == 1
            && !ins.is_null()
            && !outs.is_null()
            && unsafe { *ins == speaker::STEREO && *outs == speaker::STEREO };
        if stereo { result::OK } else { result::FALSE }
    }),
    get_bus_arrangement: sys!(|_t: Raw, _d: i32, i: i32, arr: *mut u64| -> TResult {
        if i != 0 || arr.is_null() {
            return result::INVALID_ARGUMENT;
        }
        // SAFETY: to fill.
        unsafe { *arr = speaker::STEREO };
        result::OK
    }),
    can_process_sample_size: sys!(|_t: Raw, size: i32| -> TResult { sample32(size) }),
    get_latency_samples: sys!(|_t: Raw| -> u32 { 0 }),
    setup_processing: sys!(|_t: Raw, _s: *mut ProcessSetup| -> TResult { result::OK }),
    set_processing: sys!(|_t: Raw, _s: u8| -> TResult { result::OK }),
    process: sys!(|raw: Raw, data: *mut ProcessData| -> TResult {
        if data.is_null() {
            return result::INVALID_ARGUMENT;
        }
        // SAFETY: the host's call on table 1 with its data.
        unsafe {
            let data = &mut *data;
            let invert = {
                let mut st = polarity(raw, 1);
                for (id, v) in last_changes(data.input_parameter_changes) {
                    if id == INVERT_ID {
                        st.invert = v;
                    }
                }
                st.invert >= 0.5
            };
            each_sample(data, |x| if invert { -x } else { x });
        }
        result::OK
    }),
    get_tail_samples: sys!(|_t: Raw| -> u32 { 0 }),
};

static POLARITY_POINT: IConnectionPointVtbl = IConnectionPointVtbl {
    unknown: unknown::<Polarity, 2>(),
    connect: sys!(|raw: Raw, other: Raw| -> TResult {
        // SAFETY: the host's call on table 2; `other` the controller's
        // connection point. The message is made by the host.
        unsafe {
            let mut st = polarity(raw, 2);
            st.peer = HostRef::hold(other);
            let (Some(host), Some(peer)) = (&st.host, &st.peer) else {
                return result::FALSE;
            };
            let app = host.table::<IHostApplicationVtbl>();
            let (mut cid, mut id) = (iid::MESSAGE, iid::MESSAGE);
            let mut msg: Raw = std::ptr::null_mut();
            if (app.create_instance)(host.0, &mut cid, &mut id, &mut msg) != result::OK
                || msg.is_null()
            {
                return result::FALSE;
            }
            let m = &**(msg as *const *const IMessageVtbl);
            (m.set_message_id)(msg, c"Hello".as_ptr());
            let attrs = (m.get_attributes)(msg);
            if !attrs.is_null() {
                let a = &**(attrs as *const *const IAttributeListVtbl);
                (a.set_int)(attrs, c"Version".as_ptr(), 3);
            }
            (peer.table::<IConnectionPointVtbl>().notify)(peer.0, msg);
            (m.unknown.release)(msg);
        }
        result::OK
    }),
    disconnect: sys!(|raw: Raw, _o: Raw| -> TResult {
        // SAFETY: the host's call on table 2.
        unsafe { polarity(raw, 2).peer = None };
        result::OK
    }),
    notify: sys!(|_t: Raw, _m: Raw| -> TResult { result::OK }),
};

#[derive(Default)]
struct ControllerState {
    invert: f64,
    hello: f64,
    host: Option<HostRef>,
    handler: Option<HostRef>,
}

#[repr(C)]
struct PolarityController {
    controller: &'static IEditControllerVtbl,
    point: &'static IConnectionPointVtbl,
    refs: AtomicU32,
    state: Mutex<ControllerState>,
}

impl Default for PolarityController {
    fn default() -> PolarityController {
        PolarityController {
            controller: &POLARITY_EDIT,
            point: &POLARITY_EDIT_POINT,
            refs: AtomicU32::new(1),
            state: Mutex::new(ControllerState::default()),
        }
    }
}

impl Object for PolarityController {
    fn table(id: &Tuid) -> Option<usize> {
        if *id == iid::PLUGIN_BASE || *id == iid::EDIT_CONTROLLER {
            Some(0)
        } else if *id == iid::CONNECTION_POINT {
            Some(1)
        } else {
            None
        }
    }

    fn refs(&self) -> &AtomicU32 {
        &self.refs
    }
}

/// The polarity controller's state from table `k`.
///
/// # Safety
/// `raw` is a polarity controller's table `k`.
unsafe fn controller<'a>(raw: Raw, k: usize) -> std::sync::MutexGuard<'a, ControllerState> {
    // SAFETY: the caller's.
    lock(&unsafe { this::<PolarityController>(raw, k) }.state)
}

static POLARITY_EDIT: IEditControllerVtbl = IEditControllerVtbl {
    base: IPluginBaseVtbl {
        unknown: unknown::<PolarityController, 0>(),
        initialize: sys!(|raw: Raw, ctx: Raw| -> TResult {
            // SAFETY: the host's call on table 0; its context counted.
            unsafe { controller(raw, 0).host = HostRef::hold(ctx) };
            result::OK
        }),
        terminate: sys!(|raw: Raw| -> TResult {
            // SAFETY: the host's call on table 0.
            let mut st = unsafe { controller(raw, 0) };
            st.host = None;
            st.handler = None;
            result::OK
        }),
    },
    set_component_state: sys!(|raw: Raw, stream: Raw| -> TResult {
        // SAFETY: the host's call on table 0, with its stream.
        unsafe {
            let Some(b) = read_bytes(stream, 8) else {
                return result::FALSE;
            };
            controller(raw, 0).invert = f64::from_le_bytes(b.try_into().unwrap_or_default());
        }
        result::OK
    }),
    set_state: sys!(|_t: Raw, _s: Raw| -> TResult { result::OK }),
    get_state: sys!(|_t: Raw, _s: Raw| -> TResult { result::OK }),
    get_parameter_count: sys!(|_t: Raw| -> i32 { 2 }),
    get_parameter_info: sys!(|_t: Raw, i: i32, out: *mut ParameterInfo| -> TResult {
        match i {
            0 => param_info(
                out,
                INVERT_ID,
                "Invert",
                "",
                1,
                0.0,
                param_flags::CAN_AUTOMATE,
            ),
            1 => param_info(
                out,
                HELLO_ID,
                "Hello",
                "",
                0,
                0.0,
                param_flags::READ_ONLY | param_flags::HIDDEN,
            ),
            _ => result::INVALID_ARGUMENT,
        }
    }),
    get_param_string_by_value: sys!(|_t: Raw, id: u32, v: f64, out: *mut u16| -> TResult {
        match id {
            INVERT_ID => text_out(out, if v >= 0.5 { "Inverted" } else { "Normal" }),
            _ => text_out(out, ""),
        }
    }),
    get_param_value_by_string: sys!(
        |_t: Raw, _i: u32, _s: *const u16, _v: *mut f64| -> TResult { result::FALSE }
    ),
    normalized_param_to_plain: sys!(|_t: Raw, _i: u32, v: f64| -> f64 { v }),
    plain_param_to_normalized: sys!(|_t: Raw, _i: u32, v: f64| -> f64 { v }),
    get_param_normalized: sys!(|raw: Raw, id: u32| -> f64 {
        // SAFETY: the host's call on table 0.
        let st = unsafe { controller(raw, 0) };
        match id {
            INVERT_ID => st.invert,
            HELLO_ID => st.hello,
            _ => 0.0,
        }
    }),
    set_param_normalized: sys!(|raw: Raw, id: u32, v: f64| -> TResult {
        // SAFETY: the host's call on table 0.
        let mut st = unsafe { controller(raw, 0) };
        if id == INVERT_ID {
            st.invert = v;
            result::OK
        } else {
            result::FALSE
        }
    }),
    set_component_handler: sys!(|raw: Raw, h: Raw| -> TResult {
        // SAFETY: the host's call on table 0; its handler counted.
        unsafe { controller(raw, 0).handler = HostRef::hold(h) };
        result::OK
    }),
    create_view: sys!(|_t: Raw, _n: *const c_char| -> Raw { std::ptr::null_mut() }),
};

static POLARITY_EDIT_POINT: IConnectionPointVtbl = IConnectionPointVtbl {
    unknown: unknown::<PolarityController, 1>(),
    connect: sys!(|_t: Raw, _o: Raw| -> TResult { result::OK }),
    disconnect: sys!(|_t: Raw, _o: Raw| -> TResult { result::OK }),
    notify: sys!(|raw: Raw, msg: Raw| -> TResult {
        if msg.is_null() {
            return result::INVALID_ARGUMENT;
        }
        // SAFETY: the host's message and its attribute list.
        unsafe {
            let m = &**(msg as *const *const IMessageVtbl);
            let id = (m.get_message_id)(msg);
            if id.is_null() || CStr::from_ptr(id) != c"Hello" {
                return result::FALSE;
            }
            let attrs = (m.get_attributes)(msg);
            let mut version = 0i64;
            if !attrs.is_null() {
                let a = &**(attrs as *const *const IAttributeListVtbl);
                (a.get_int)(attrs, c"Version".as_ptr(), &mut version);
            }
            controller(raw, 1).hello = version as f64 / 10.0;
        }
        result::OK
    }),
};

// ---- The gain's editor (Windows) ------------------------------------------------------

mod view {
    use super::*;

    /// The size it starts at, and the one it asks its frame for when
    /// attached.
    pub const START: (i32, i32) = (320, 120);
    pub const ASKED: (i32, i32) = (330, 130);

    #[derive(Default)]
    struct ViewState {
        size: (i32, i32),
        window: isize,
        frame: Option<HostRef>,
    }

    #[repr(C)]
    struct View {
        table: &'static IPlugViewVtbl,
        refs: AtomicU32,
        state: Mutex<ViewState>,
    }

    impl Object for View {
        fn table(id: &Tuid) -> Option<usize> {
            (*id == iid::PLUG_VIEW).then_some(0)
        }

        fn refs(&self) -> &AtomicU32 {
            &self.refs
        }
    }

    /// The view's state.
    ///
    /// # Safety
    /// `raw` is a view's table 0.
    unsafe fn state<'a>(raw: Raw) -> std::sync::MutexGuard<'a, ViewState> {
        // SAFETY: the caller's.
        lock(&unsafe { this::<View>(raw, 0) }.state)
    }

    #[cfg(windows)]
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
            param: *mut c_void,
        ) -> isize;
        fn DestroyWindow(hwnd: isize) -> i32;
        fn MoveWindow(hwnd: isize, x: i32, y: i32, w: i32, h: i32, repaint: i32) -> i32;
    }

    fn hwnd_kind(ty: *const c_char) -> bool {
        // SAFETY: a NUL-ended name.
        !ty.is_null() && unsafe { CStr::from_ptr(ty) } == bv_vst3_sys::PLATFORM_HWND
    }

    static VIEW: IPlugViewVtbl = IPlugViewVtbl {
        unknown: unknown::<View, 0>(),
        is_platform_type_supported: sys!(|_t: Raw, ty: *const c_char| -> TResult {
            if hwnd_kind(ty) && cfg!(windows) {
                result::OK
            } else {
                result::FALSE
            }
        }),
        attached: sys!(
            |raw: Raw, parent: *mut c_void, ty: *const c_char| -> TResult {
                if !hwnd_kind(ty) {
                    return result::FALSE;
                }
                // SAFETY: the host's window; the view's own state.
                unsafe {
                    #[cfg(windows)]
                    {
                        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
                        let text: Vec<u16> = "BananaV Test Gain\0".encode_utf16().collect();
                        let (w, h) = state(raw).size;
                        const WS_CHILD: u32 = 0x4000_0000;
                        const WS_VISIBLE: u32 = 0x1000_0000;
                        let hwnd = CreateWindowExW(
                            0,
                            class.as_ptr(),
                            text.as_ptr(),
                            WS_CHILD | WS_VISIBLE,
                            0,
                            0,
                            w,
                            h,
                            parent as isize,
                            0,
                            0,
                            std::ptr::null_mut(),
                        );
                        if hwnd == 0 {
                            return result::FALSE;
                        }
                        state(raw).window = hwnd;
                    }
                    #[cfg(not(windows))]
                    let _ = parent;
                    // Asks for another size, as plug-ins do once attached.
                    let frame = state(raw).frame.as_ref().map(|f| f.0);
                    if let Some(frame) = frame {
                        let mut r = ViewRect {
                            left: 0,
                            top: 0,
                            right: ASKED.0,
                            bottom: ASKED.1,
                        };
                        let f = &**(frame as *const *const IPlugFrameVtbl);
                        (f.resize_view)(frame, raw, &mut r);
                    }
                }
                result::OK
            }
        ),
        removed: sys!(|raw: Raw| -> TResult {
            // SAFETY: the view's own state.
            let hwnd = std::mem::take(&mut unsafe { state(raw) }.window);
            #[cfg(windows)]
            if hwnd != 0 {
                // SAFETY: the window `attached` made.
                unsafe { DestroyWindow(hwnd) };
            }
            #[cfg(not(windows))]
            let _ = hwnd;
            result::OK
        }),
        on_wheel: sys!(|_t: Raw, _d: f32| -> TResult { result::FALSE }),
        on_key_down: sys!(|_t: Raw, _k: u16, _c: i16, _m: i16| -> TResult { result::FALSE }),
        on_key_up: sys!(|_t: Raw, _k: u16, _c: i16, _m: i16| -> TResult { result::FALSE }),
        get_size: sys!(|raw: Raw, r: *mut ViewRect| -> TResult {
            if r.is_null() {
                return result::INVALID_ARGUMENT;
            }
            // SAFETY: the view's own state; a rectangle to fill.
            unsafe {
                let (w, h) = state(raw).size;
                *r = ViewRect {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: h,
                };
            }
            result::OK
        }),
        on_size: sys!(|raw: Raw, r: *mut ViewRect| -> TResult {
            if r.is_null() {
                return result::INVALID_ARGUMENT;
            }
            // SAFETY: the view's own state; the host's rectangle.
            unsafe {
                let r = *r;
                let mut st = state(raw);
                st.size = (r.right - r.left, r.bottom - r.top);
                #[cfg(windows)]
                if st.window != 0 {
                    MoveWindow(st.window, 0, 0, st.size.0, st.size.1, 1);
                }
            }
            result::OK
        }),
        on_focus: sys!(|_t: Raw, _f: u8| -> TResult { result::OK }),
        set_frame: sys!(|raw: Raw, frame: Raw| -> TResult {
            // SAFETY: the view's own state; the host's frame counted.
            unsafe { state(raw).frame = HostRef::hold(frame) };
            result::OK
        }),
        can_resize: sys!(|_t: Raw| -> TResult { result::OK }),
        check_size_constraint: sys!(|_t: Raw, r: *mut ViewRect| -> TResult {
            if r.is_null() {
                return result::INVALID_ARGUMENT;
            }
            // SAFETY: the host's rectangle.
            unsafe {
                let r = &mut *r;
                r.right = r.left + (r.right - r.left).max(200);
                r.bottom = r.top + (r.bottom - r.top).max(80);
            }
            result::OK
        }),
    };

    /// A new view (Windows' only: elsewhere none).
    pub fn make() -> Raw {
        if !cfg!(windows) {
            return std::ptr::null_mut();
        }
        new(
            View {
                table: &VIEW,
                refs: AtomicU32::new(1),
                state: Mutex::new(ViewState {
                    size: START,
                    ..ViewState::default()
                }),
            },
            0,
        )
    }
}

pub use view::{ASKED as EDITOR_ASKED, START as EDITOR_START};
