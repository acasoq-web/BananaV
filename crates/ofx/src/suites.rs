//! The host plug-ins get (`OfxHost`) and the suites it hands out: memory,
//! threads, messages, progress and the time line (the property, parameter
//! and image effect suites live with their objects).
#![allow(unsafe_code)]

use std::alloc::{Layout, alloc, dealloc};
use std::cell::Cell;
use std::collections::BTreeSet;
use std::ffi::{CStr, c_char, c_double, c_int, c_uint, c_void};
use std::sync::atomic::Ordering;
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};
use std::thread::ThreadId;

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::effect::{Effect, IMAGE_EFFECT_SUITE};
use crate::param::{PARAMETRIC_SUITE, parameter_suite};
use crate::printf;
use crate::props::{PROPERTY_SUITE, PropSet};
use crate::templates;
use crate::varargs::{Va, as_variadic, variadic};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// How serious a plug-in's message is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Fatal,
    Error,
    Warning,
    Message,
    Log,
    Question,
}

impl Severity {
    fn from_ofx(s: &CStr) -> Severity {
        if s == kOfxMessageFatal {
            Severity::Fatal
        } else if s == kOfxMessageError {
            Severity::Error
        } else if s == kOfxMessageWarning {
            Severity::Warning
        } else if s == kOfxMessageLog {
            Severity::Log
        } else if s == kOfxMessageQuestion {
            Severity::Question
        } else {
            Severity::Message
        }
    }
}

/// A message a plug-in sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub severity: Severity,
    /// The plug-in's identifier for it (may be empty).
    pub id: String,
    pub text: String,
}

/// Messages sent to no instance (and every message, for reports).
static LOG: Mutex<Vec<Message>> = Mutex::new(Vec::new());

/// The latest messages plug-ins sent, oldest first.
pub fn recent_messages() -> Vec<Message> {
    lock(&LOG).clone()
}

fn log(m: Message) {
    let mut log = lock(&LOG);
    if log.len() >= 256 {
        log.remove(0);
    }
    log.push(m);
}

/// Suites plug-ins asked for that BananaV does not have (name, version).
static MISSING: Mutex<BTreeSet<(String, i32)>> = Mutex::new(BTreeSet::new());

/// Whether `BV_OFX_TRACE` is set: the host then tells on stderr each action
/// it calls and each property a plug-in asks for that is not there (to
/// find out why a plug-in fails or hangs).
pub fn tracing() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("BV_OFX_TRACE").is_some_and(|v| !v.is_empty()))
}

/// The suites plug-ins asked for and did not get.
pub fn missing_suites() -> Vec<(String, i32)> {
    lock(&MISSING).iter().cloned().collect()
}

/// The host every plug-in gets: its properties and `fetchSuite`.
pub(crate) struct Host {
    pub props: PropSet,
    ofx: OfxHost,
}

// SAFETY: the host's properties lock themselves; `ofx` only points at them.
unsafe impl Sync for Host {}
// SAFETY: as above.
unsafe impl Send for Host {}

pub(crate) fn host() -> &'static Host {
    static HOST: OnceLock<Box<Host>> = OnceLock::new();
    HOST.get_or_init(|| {
        // Before any plug-in runs.
        #[cfg(windows)]
        crate::plugin::keep_a_drive_folder();
        let props = templates::host();
        // Where NVIDIA's driver is, plug-ins may render with CUDA: their
        // pictures then stay on the graphics card (`Instance::render_on_card`).
        if bv_cuda::installed() {
            props.set_cstr(kOfxImageEffectPropCudaRenderSupported, c"true");
        }
        let mut h = Box::new(Host {
            props,
            ofx: OfxHost {
                host: std::ptr::null_mut(),
                fetch_suite,
            },
        });
        h.ofx.host = h.props.handle();
        h
    })
}

impl Host {
    /// The `OfxHost` plug-ins keep: it lives as long as the program.
    pub fn ofx(&'static self) -> *mut OfxHost {
        &self.ofx as *const OfxHost as *mut OfxHost
    }
}

unsafe extern "C" fn fetch_suite(
    _host: OfxPropertySetHandle,
    name: *const c_char,
    version: c_int,
) -> *const c_void {
    if name.is_null() {
        return std::ptr::null();
    }
    // SAFETY: a C string from the plug-in.
    let name = unsafe { CStr::from_ptr(name) };
    let ptr = |s: *const c_void| s;
    let suite: *const c_void = match (name, version) {
        (n, 1) if n == kOfxPropertySuite => {
            ptr((&PROPERTY_SUITE as *const OfxPropertySuiteV1).cast())
        }
        (n, 1) if n == kOfxImageEffectSuite => {
            ptr((&IMAGE_EFFECT_SUITE as *const OfxImageEffectSuiteV1).cast())
        }
        (n, 1) if n == kOfxParameterSuite => {
            ptr((parameter_suite() as *const OfxParameterSuiteV1).cast())
        }
        (n, 1) if n == kOfxParametricParameterSuite => {
            ptr((&PARAMETRIC_SUITE as *const OfxParametricParameterSuiteV1).cast())
        }
        (n, 1) if n == kOfxMemorySuite => ptr((&MEMORY_SUITE as *const OfxMemorySuiteV1).cast()),
        (n, 1) if n == kOfxMultiThreadSuite => {
            ptr((&MULTI_THREAD_SUITE as *const OfxMultiThreadSuiteV1).cast())
        }
        (n, 1) if n == kOfxMessageSuite => {
            ptr((message_suite_v1() as *const OfxMessageSuiteV1).cast())
        }
        (n, 2) if n == kOfxMessageSuite => {
            ptr((message_suite_v2() as *const OfxMessageSuiteV2).cast())
        }
        (n, 1) if n == kOfxProgressSuite => {
            ptr((&PROGRESS_SUITE_V1 as *const OfxProgressSuiteV1).cast())
        }
        (n, 2) if n == kOfxProgressSuite => {
            ptr((&PROGRESS_SUITE_V2 as *const OfxProgressSuiteV2).cast())
        }
        (n, 1) if n == kOfxTimeLineSuite => {
            ptr((&TIME_LINE_SUITE as *const OfxTimeLineSuiteV1).cast())
        }
        (n, 1) if n == kOfxInteractSuite => {
            ptr((&crate::interact::INTERACT_SUITE as *const OfxInteractSuiteV1).cast())
        }
        (n, 1) if n == kOfxDrawSuite => {
            ptr((&crate::interact::DRAW_SUITE as *const bv_ofx_sys::OfxDrawSuiteV1).cast())
        }
        _ => std::ptr::null(),
    };
    if suite.is_null() {
        lock(&MISSING).insert((name.to_string_lossy().into_owned(), version));
    }
    suite
}

// The memory suite: blocks with their size in front.

const HEADER: usize = 16;

unsafe extern "C" fn memory_alloc(
    _handle: *mut c_void,
    n_bytes: usize,
    data: *mut *mut c_void,
) -> OfxStatus {
    if data.is_null() {
        return kOfxStatErrValue;
    }
    let Ok(layout) = Layout::from_size_align(n_bytes + HEADER, HEADER) else {
        return kOfxStatErrMemory;
    };
    // SAFETY: a layout of non-zero size.
    let base = unsafe { alloc(layout) };
    if base.is_null() {
        return kOfxStatErrMemory;
    }
    // SAFETY: the block starts with room for its size.
    unsafe {
        (base as *mut usize).write(n_bytes);
        *data = base.add(HEADER) as *mut c_void;
    }
    kOfxStatOK
}

unsafe extern "C" fn memory_free(data: *mut c_void) -> OfxStatus {
    if data.is_null() {
        return kOfxStatOK;
    }
    // SAFETY: a block from `memory_alloc`: its size is in front.
    unsafe {
        let base = (data as *mut u8).sub(HEADER);
        let n = (base as *const usize).read();
        dealloc(base, Layout::from_size_align_unchecked(n + HEADER, HEADER));
    }
    kOfxStatOK
}

static MEMORY_SUITE: OfxMemorySuiteV1 = OfxMemorySuiteV1 {
    memory_alloc,
    memory_free,
};

// The multi-thread suite.

thread_local! {
    /// This thread's index, when the suite spawned it.
    static INDEX: Cell<Option<c_uint>> = const { Cell::new(None) };
}

fn cpus() -> c_uint {
    std::thread::available_parallelism().map_or(1, |n| n.get() as c_uint)
}

unsafe extern "C" fn multi_thread(
    func: OfxThreadFunctionV1,
    n_threads: c_uint,
    arg: *mut c_void,
) -> OfxStatus {
    let n = if n_threads == 0 {
        cpus()
    } else {
        n_threads.min(256)
    };
    let arg = arg as usize;
    let start = std::time::Instant::now();
    // The plug-in calling: a crash on these threads switches it off.
    let binary = crate::guard::running();
    let crashed = std::sync::atomic::AtomicBool::new(false);
    let spawned = std::thread::scope(|s| {
        let handles: Vec<_> = (0..n)
            .map(|i| {
                let (binary, crashed) = (binary.clone(), &crashed);
                std::thread::Builder::new()
                    .stack_size(crate::PLUGIN_STACK)
                    .spawn_scoped(s, move || {
                        INDEX.set(Some(i));
                        // SAFETY: the plug-in's function, with its argument.
                        let ran = unsafe {
                            crate::guard::thread(binary.as_ref(), func, i, n, arg as *mut c_void)
                        };
                        if !ran {
                            crashed.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    })
            })
            .collect();
        handles.into_iter().all(|h| h.is_ok())
    });
    if tracing() {
        let ms = start.elapsed().as_secs_f64() * 1e3;
        eprintln!("ofx: multiThread on {n} threads ({ms:.2} ms)");
    }
    if spawned && !crashed.into_inner() {
        kOfxStatOK
    } else {
        kOfxStatFailed
    }
}

unsafe extern "C" fn multi_thread_num_cpus(n: *mut c_uint) -> OfxStatus {
    if n.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: where the plug-in wants the count.
    unsafe { *n = cpus() };
    kOfxStatOK
}

unsafe extern "C" fn multi_thread_index(index: *mut c_uint) -> OfxStatus {
    if index.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: where the plug-in wants the index.
    unsafe { *index = INDEX.get().unwrap_or(0) };
    kOfxStatOK
}

unsafe extern "C" fn multi_thread_is_spawned_thread() -> c_int {
    INDEX.get().is_some() as c_int
}

/// The suite's mutexes are recursive: the thread holding one may lock it
/// again.
#[derive(Debug)]
struct RecursiveMutex {
    state: Mutex<(Option<ThreadId>, u32)>,
    free: Condvar,
}

impl RecursiveMutex {
    fn lock(&self) {
        let me = std::thread::current().id();
        let mut s = lock(&self.state);
        loop {
            match s.0 {
                None => {
                    *s = (Some(me), 1);
                    return;
                }
                Some(owner) if owner == me => {
                    s.1 += 1;
                    return;
                }
                Some(_) => s = self.free.wait(s).unwrap_or_else(|e| e.into_inner()),
            }
        }
    }

    fn try_lock(&self) -> bool {
        let me = std::thread::current().id();
        let mut s = lock(&self.state);
        match s.0 {
            None => {
                *s = (Some(me), 1);
                true
            }
            Some(owner) if owner == me => {
                s.1 += 1;
                true
            }
            Some(_) => false,
        }
    }

    fn unlock(&self) -> bool {
        let me = std::thread::current().id();
        let mut s = lock(&self.state);
        if s.0 != Some(me) {
            return false;
        }
        s.1 -= 1;
        if s.1 == 0 {
            s.0 = None;
            self.free.notify_one();
        }
        true
    }
}

unsafe extern "C" fn mutex_create(mutex: *mut OfxMutexHandle, lock_count: c_int) -> OfxStatus {
    if mutex.is_null() {
        return kOfxStatErrValue;
    }
    let m = Box::new(RecursiveMutex {
        state: Mutex::new((None, 0)),
        free: Condvar::new(),
    });
    for _ in 0..lock_count.max(0) {
        m.lock();
    }
    // SAFETY: where the plug-in wants the handle.
    unsafe { *mutex = Box::into_raw(m) as OfxMutexHandle };
    kOfxStatOK
}

/// # Safety
/// `m` is null or a mutex from `mutex_create`.
unsafe fn mutex<'a>(m: OfxMutexHandle) -> Option<&'a RecursiveMutex> {
    // SAFETY: as the caller promises.
    (!m.is_null()).then(|| unsafe { &*(m as *const RecursiveMutex) })
}

unsafe extern "C" fn mutex_destroy(m: OfxMutexHandle) -> OfxStatus {
    if m.is_null() {
        return kOfxStatErrBadHandle;
    }
    // SAFETY: made by `mutex_create`, destroyed once.
    drop(unsafe { Box::from_raw(m as *mut RecursiveMutex) });
    kOfxStatOK
}

unsafe extern "C" fn mutex_lock(m: OfxMutexHandle) -> OfxStatus {
    // SAFETY: the plug-in's mutex.
    match unsafe { mutex(m) } {
        Some(m) => {
            m.lock();
            kOfxStatOK
        }
        None => kOfxStatErrBadHandle,
    }
}

unsafe extern "C" fn mutex_unlock(m: OfxMutexHandle) -> OfxStatus {
    // SAFETY: the plug-in's mutex.
    match unsafe { mutex(m) } {
        Some(m) if m.unlock() => kOfxStatOK,
        Some(_) => kOfxStatFailed,
        None => kOfxStatErrBadHandle,
    }
}

unsafe extern "C" fn mutex_try_lock(m: OfxMutexHandle) -> OfxStatus {
    // SAFETY: the plug-in's mutex.
    match unsafe { mutex(m) } {
        Some(m) if m.try_lock() => kOfxStatOK,
        Some(_) => kOfxStatFailed,
        None => kOfxStatErrBadHandle,
    }
}

static MULTI_THREAD_SUITE: OfxMultiThreadSuiteV1 = OfxMultiThreadSuiteV1 {
    multi_thread,
    multi_thread_num_cpus,
    multi_thread_index,
    multi_thread_is_spawned_thread,
    mutex_create,
    mutex_destroy,
    mutex_lock,
    mutex_unlock,
    mutex_try_lock,
};

// The message suite.

impl printf::Args for Va {
    fn word(&mut self) -> u64 {
        Va::word(self)
    }

    fn double(&mut self) -> f64 {
        Va::double(self)
    }

    fn string(&mut self) -> Option<String> {
        let p = self.ptr::<c_char>();
        // SAFETY: a `%s` value is a C string or null.
        (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
    }
}

/// The message a call carries.
///
/// # Safety
/// The plug-in's arguments: C strings or null, the format's values in `va`.
unsafe fn read_message(
    ty: *mut c_void,
    id: *mut c_void,
    format: *mut c_void,
    va: &mut Va,
) -> Message {
    let text = |p: *mut c_void| {
        // SAFETY: as the caller promises.
        (!p.is_null()).then(|| unsafe { CStr::from_ptr(p as *const c_char) })
    };
    let severity = text(ty).map_or(Severity::Message, Severity::from_ofx);
    let id = text(id)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let text = text(format)
        .map(|f| printf::format(f.to_bytes(), va))
        .unwrap_or_default();
    Message { severity, id, text }
}

unsafe fn message(
    handle: *mut c_void,
    ty: *mut c_void,
    id: *mut c_void,
    format: *mut c_void,
    mut va: Va,
) -> OfxStatus {
    // SAFETY: the plug-in's arguments.
    let m = unsafe { read_message(ty, id, format, &mut va) };
    let severity = m.severity;
    // SAFETY: the handle is an effect, an interact or null.
    if let Some(run) = unsafe { Effect::from_handle(handle) }.and_then(|e| e.run.as_ref()) {
        run.push_message(m.clone());
    }
    log(m);
    // Nobody to ask while rendering: the default answer.
    if severity == Severity::Question {
        kOfxStatReplyDefault
    } else {
        kOfxStatOK
    }
}

unsafe fn set_persistent(
    handle: *mut c_void,
    ty: *mut c_void,
    id: *mut c_void,
    format: *mut c_void,
    mut va: Va,
) -> OfxStatus {
    // SAFETY: the plug-in's arguments.
    let m = unsafe { read_message(ty, id, format, &mut va) };
    // SAFETY: the handle is an effect.
    match unsafe { Effect::from_handle(handle) }.and_then(|e| e.run.as_ref()) {
        Some(run) => {
            *lock(&run.persistent) = Some(m.clone());
            log(m);
            kOfxStatOK
        }
        None => kOfxStatErrBadHandle,
    }
}

variadic!(fn message_v(h, ty, id, format) => message);
variadic!(fn set_persistent_message_v(h, ty, id, format) => set_persistent);

unsafe extern "C" fn clear_persistent_message(handle: *mut c_void) -> OfxStatus {
    // SAFETY: the handle is an effect.
    match unsafe { Effect::from_handle(handle) }.and_then(|e| e.run.as_ref()) {
        Some(run) => {
            *lock(&run.persistent) = None;
            kOfxStatOK
        }
        None => kOfxStatErrBadHandle,
    }
}

fn message_suite_v1() -> &'static OfxMessageSuiteV1 {
    static SUITE: OnceLock<OfxMessageSuiteV1> = OnceLock::new();
    // SAFETY: the function receives the call as its type passes it.
    SUITE.get_or_init(|| unsafe {
        OfxMessageSuiteV1 {
            message: as_variadic(message_v as *const ()),
        }
    })
}

fn message_suite_v2() -> &'static OfxMessageSuiteV2 {
    static SUITE: OnceLock<OfxMessageSuiteV2> = OnceLock::new();
    // SAFETY: the functions receive the calls as their types pass them.
    SUITE.get_or_init(|| unsafe {
        OfxMessageSuiteV2 {
            message: as_variadic(message_v as *const ()),
            set_persistent_message: as_variadic(set_persistent_message_v as *const ()),
            clear_persistent_message,
        }
    })
}

// The progress suite.

fn progress_run<'a>(instance: *mut c_void) -> Option<&'a crate::effect::Run> {
    // SAFETY: the handle is an effect instance.
    unsafe { Effect::from_handle(instance) }.and_then(|e| e.run.as_ref())
}

unsafe extern "C" fn progress_start(instance: *mut c_void, label: *const c_char) -> OfxStatus {
    let Some(run) = progress_run(instance) else {
        return kOfxStatErrBadHandle;
    };
    // SAFETY: a C string from the plug-in, or null.
    let label = unsafe { crate::props::text(label) };
    *lock(&run.progress) = Some((label, 0.0));
    kOfxStatOK
}

unsafe extern "C" fn progress_start_v2(
    instance: *mut c_void,
    label: *const c_char,
    _id: *const c_char,
) -> OfxStatus {
    // SAFETY: the plug-in's arguments.
    unsafe { progress_start(instance, label) }
}

unsafe extern "C" fn progress_update(instance: *mut c_void, progress: c_double) -> OfxStatus {
    let Some(run) = progress_run(instance) else {
        return kOfxStatErrBadHandle;
    };
    if let Some(p) = lock(&run.progress).as_mut() {
        p.1 = progress.clamp(0.0, 1.0);
    }
    if run.abort.load(Ordering::Relaxed) {
        kOfxStatReplyNo
    } else {
        kOfxStatOK
    }
}

unsafe extern "C" fn progress_end(instance: *mut c_void) -> OfxStatus {
    let Some(run) = progress_run(instance) else {
        return kOfxStatErrBadHandle;
    };
    *lock(&run.progress) = None;
    kOfxStatOK
}

static PROGRESS_SUITE_V1: OfxProgressSuiteV1 = OfxProgressSuiteV1 {
    progress_start,
    progress_update,
    progress_end,
};

static PROGRESS_SUITE_V2: OfxProgressSuiteV2 = OfxProgressSuiteV2 {
    progress_start: progress_start_v2,
    progress_update,
    progress_end,
};

// The time line suite.

unsafe extern "C" fn get_time(instance: *mut c_void, time: *mut c_double) -> OfxStatus {
    let Some(run) = progress_run(instance) else {
        return kOfxStatErrBadHandle;
    };
    if time.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: where the plug-in wants the time.
    unsafe { *time = f64::from_bits(run.timeline_time.load(Ordering::Relaxed)) };
    kOfxStatOK
}

unsafe extern "C" fn goto_time(instance: *mut c_void, _time: c_double) -> OfxStatus {
    // Plug-ins do not move BananaV's cursor.
    match progress_run(instance) {
        Some(_) => kOfxStatOK,
        None => kOfxStatErrBadHandle,
    }
}

unsafe extern "C" fn get_time_bounds(
    instance: *mut c_void,
    first: *mut c_double,
    last: *mut c_double,
) -> OfxStatus {
    // SAFETY: the handle is an effect instance.
    let Some(e) = (unsafe { Effect::from_handle(instance) }) else {
        return kOfxStatErrBadHandle;
    };
    let duration = e
        .props
        .dbl(kOfxImageEffectInstancePropEffectDuration, 0)
        .unwrap_or(0.0);
    // SAFETY: where the plug-in wants the bounds.
    unsafe {
        if !first.is_null() {
            *first = 0.0;
        }
        if !last.is_null() {
            *last = duration;
        }
    }
    kOfxStatOK
}

static TIME_LINE_SUITE: OfxTimeLineSuiteV1 = OfxTimeLineSuiteV1 {
    get_time,
    goto_time,
    get_time_bounds,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// The main window reaches plug-ins as the host's OS handle.
    #[test]
    fn the_main_window_is_the_hosts_os_handle() {
        crate::set_host_window(0x1234);
        assert_eq!(
            host().props.ptr(kOfxPropHostOSHandle, 0),
            Some(0x1234 as *mut c_void)
        );
        crate::set_host_window(0);
    }

    unsafe fn suite<T>(name: &CStr, version: c_int) -> &'static T {
        // SAFETY: the host's own suites.
        unsafe {
            let p = (host().ofx.fetch_suite)(host().props.handle(), name.as_ptr(), version);
            assert!(!p.is_null(), "{name:?}");
            &*(p as *const T)
        }
    }

    #[test]
    fn the_host_hands_out_its_suites() {
        // SAFETY: names and versions the host knows.
        unsafe {
            let _: &OfxPropertySuiteV1 = suite(kOfxPropertySuite, 1);
            let _: &OfxParameterSuiteV1 = suite(kOfxParameterSuite, 1);
            let _: &OfxMessageSuiteV2 = suite(kOfxMessageSuite, 2);
            let p = (host().ofx.fetch_suite)(host().props.handle(), c"NoSuchSuite".as_ptr(), 1);
            assert!(p.is_null());
        }
        assert!(missing_suites().contains(&("NoSuchSuite".into(), 1)));
    }

    #[test]
    fn threads_and_recursive_mutexes() {
        use std::sync::atomic::AtomicU32;
        static SUM: AtomicU32 = AtomicU32::new(0);
        unsafe extern "C" fn work(i: c_uint, n: c_uint, _arg: *mut c_void) {
            assert!(i < n);
            // SAFETY: the suite's own function.
            assert_eq!(unsafe { multi_thread_is_spawned_thread() }, 1);
            SUM.fetch_add(i + 1, Ordering::Relaxed);
        }
        // SAFETY: the host's suite.
        let s: &OfxMultiThreadSuiteV1 = unsafe { suite(kOfxMultiThreadSuite, 1) };
        // SAFETY: a valid function.
        assert_eq!(
            unsafe { (s.multi_thread)(work, 4, std::ptr::null_mut()) },
            kOfxStatOK
        );
        assert_eq!(SUM.load(Ordering::Relaxed), 1 + 2 + 3 + 4);
        let mut m: OfxMutexHandle = std::ptr::null_mut();
        // SAFETY: the suite's mutexes.
        unsafe {
            assert_eq!((s.mutex_create)(&mut m, 0), kOfxStatOK);
            assert_eq!((s.mutex_lock)(m), kOfxStatOK);
            assert_eq!((s.mutex_lock)(m), kOfxStatOK, "recursive");
            let other = m as usize;
            let busy = std::thread::spawn(move || {
                (MULTI_THREAD_SUITE.mutex_try_lock)(other as OfxMutexHandle)
            })
            .join()
            .unwrap();
            assert_eq!(busy, kOfxStatFailed);
            (s.mutex_unlock)(m);
            (s.mutex_unlock)(m);
            assert_eq!((s.mutex_destroy)(m), kOfxStatOK);
        }
    }

    #[test]
    fn messages_are_formatted_like_printf() {
        // SAFETY: the host's suite.
        let s: &OfxMessageSuiteV2 = unsafe { suite(kOfxMessageSuite, 2) };
        // SAFETY: a real variadic call, as a plug-in makes it.
        let status = unsafe {
            (s.message)(
                std::ptr::null_mut(),
                kOfxMessageWarning.as_ptr(),
                c"id".as_ptr(),
                c"%s needs %d frames at %.1f fps".as_ptr(),
                c"Stabilize".as_ptr(),
                12i32,
                29.97f64,
            )
        };
        assert_eq!(status, kOfxStatOK);
        let m = recent_messages()
            .into_iter()
            .rfind(|m| m.id == "id")
            .unwrap();
        assert_eq!(m.text, "Stabilize needs 12 frames at 30.0 fps");
        assert_eq!(m.severity, Severity::Warning);
    }

    #[test]
    fn memory_blocks() {
        // SAFETY: the host's suite.
        let s: &OfxMemorySuiteV1 = unsafe { suite(kOfxMemorySuite, 1) };
        let mut p: *mut c_void = std::ptr::null_mut();
        // SAFETY: the suite's own blocks.
        unsafe {
            assert_eq!(
                (s.memory_alloc)(std::ptr::null_mut(), 100, &mut p),
                kOfxStatOK
            );
            std::ptr::write_bytes(p as *mut u8, 7, 100);
            assert_eq!((s.memory_free)(p), kOfxStatOK);
        }
    }
}
