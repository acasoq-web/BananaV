//! The objects BananaV gives VST 3 plug-ins: the host application (its
//! name, and messages and attribute lists made for them), memory streams
//! for their states, the parameter changes of each block, the handler
//! their controllers report edits to, and the frame their editors ask to
//! be resized through — on Linux, with the run loop their editors' file
//! descriptors and timers go on ([`run_loop`]).

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use bv_vst3_sys::{
    FUnknownVtbl, IAttributeListVtbl, IBStreamVtbl, IComponentHandler2Vtbl, IComponentHandlerVtbl,
    IEventHandlerVtbl, IHostApplicationVtbl, IMessageVtbl, IParamValueQueueVtbl,
    IParameterChangesVtbl, IPlugFrameVtbl, IPlugViewVtbl, IRunLoopVtbl, ISizeableStreamVtbl,
    ITimerHandlerVtbl, Raw, TResult, Tuid, ViewRect, iid, result,
};

use crate::com::{HostObject, Own, query, unknown};

/// What a lock guards stays usable after a panic elsewhere (a plug-in's
/// call never unwinds through here, but a test's assertion may).
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The object an interface pointer of table `k` of a `T` belongs to.
///
/// # Safety
/// `this` is table `k` of a live `T`.
unsafe fn object<'a, T: HostObject>(this: Raw, k: usize) -> &'a T {
    // SAFETY: the caller's.
    unsafe {
        &*this
            .cast::<u8>()
            .sub(k * std::mem::size_of::<*const ()>())
            .cast::<T>()
    }
}

/// The name plug-ins are told.
pub(crate) const HOST_NAME: &str = "BananaV";

// ---- The host application -----------------------------------------------

#[repr(C)]
pub(crate) struct HostApplication {
    table: &'static IHostApplicationVtbl,
    refs: AtomicU32,
}

impl HostObject for HostApplication {
    fn table(id: &Tuid) -> Option<usize> {
        (*id == iid::HOST_APPLICATION).then_some(0)
    }

    fn refs(&self) -> &AtomicU32 {
        &self.refs
    }
}

static HOST_APPLICATION: IHostApplicationVtbl = IHostApplicationVtbl {
    unknown: unknown::<HostApplication, 0>(),
    get_name: host_name,
    create_instance: host_create,
};

impl HostApplication {
    pub(crate) fn new() -> Own<HostApplication> {
        Own::new(HostApplication {
            table: &HOST_APPLICATION,
            refs: AtomicU32::new(1),
        })
    }
}

unsafe extern "system" fn host_name(_this: Raw, name: *mut u16) -> TResult {
    if name.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: a String128, 128 characters.
    let field = unsafe { &mut *name.cast::<[u16; 128]>() };
    bv_vst3_sys::fill16(field, HOST_NAME);
    result::OK
}

/// Makes the objects plug-ins may ask the host for: messages and
/// attribute lists.
unsafe extern "system" fn host_create(
    _this: Raw,
    cid: *mut Tuid,
    id: *mut Tuid,
    obj: *mut Raw,
) -> TResult {
    if cid.is_null() || id.is_null() || obj.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: 16 bytes each; `obj` points at a pointer.
    unsafe {
        let cid = *cid;
        if cid == iid::MESSAGE {
            let m = Message::new();
            query::<Message, 0>(m.raw(0), id, obj)
        } else if cid == iid::ATTRIBUTE_LIST {
            let a = AttributeList::new();
            query::<AttributeList, 0>(a.raw(0), id, obj)
        } else {
            *obj = std::ptr::null_mut();
            result::FALSE
        }
    }
}

// ---- Messages and attribute lists ------------------------------------------

#[repr(C)]
pub(crate) struct Message {
    table: &'static IMessageVtbl,
    refs: AtomicU32,
    id: Mutex<CString>,
    attributes: Own<AttributeList>,
}

impl HostObject for Message {
    fn table(id: &Tuid) -> Option<usize> {
        (*id == iid::MESSAGE).then_some(0)
    }

    fn refs(&self) -> &AtomicU32 {
        &self.refs
    }
}

static MESSAGE: IMessageVtbl = IMessageVtbl {
    unknown: unknown::<Message, 0>(),
    get_message_id: message_id,
    set_message_id,
    get_attributes: message_attributes,
};

impl Message {
    pub(crate) fn new() -> Own<Message> {
        Own::new(Message {
            table: &MESSAGE,
            refs: AtomicU32::new(1),
            id: Mutex::new(CString::default()),
            attributes: AttributeList::new(),
        })
    }
}

unsafe extern "system" fn message_id(this: Raw) -> *const c_char {
    // SAFETY: a message's table 0.
    let m = unsafe { object::<Message>(this, 0) };
    // The text stays where it is until the message's ID changes.
    lock(&m.id).as_ptr()
}

unsafe extern "system" fn set_message_id(this: Raw, id: *const c_char) {
    // SAFETY: a message's table 0; `id` NUL-ended when not null.
    unsafe {
        let m = object::<Message>(this, 0);
        *lock(&m.id) = if id.is_null() {
            CString::default()
        } else {
            CStr::from_ptr(id).to_owned()
        };
    }
}

unsafe extern "system" fn message_attributes(this: Raw) -> Raw {
    // SAFETY: a message's table 0. The list is the message's: as the
    // SDK's hosts do, no reference is counted for the caller.
    unsafe { object::<Message>(this, 0) }.attributes.raw(0)
}

#[derive(Clone, Debug, PartialEq)]
enum Attr {
    Int(i64),
    Float(f64),
    Text(Vec<u16>),
    Binary(Vec<u8>),
}

#[repr(C)]
pub(crate) struct AttributeList {
    table: &'static IAttributeListVtbl,
    refs: AtomicU32,
    map: Mutex<HashMap<String, Attr>>,
}

impl HostObject for AttributeList {
    fn table(id: &Tuid) -> Option<usize> {
        (*id == iid::ATTRIBUTE_LIST).then_some(0)
    }

    fn refs(&self) -> &AtomicU32 {
        &self.refs
    }
}

static ATTRIBUTE_LIST: IAttributeListVtbl = IAttributeListVtbl {
    unknown: unknown::<AttributeList, 0>(),
    set_int: attr_set_int,
    get_int: attr_get_int,
    set_float: attr_set_float,
    get_float: attr_get_float,
    set_string: attr_set_string,
    get_string: attr_get_string,
    set_binary: attr_set_binary,
    get_binary: attr_get_binary,
};

impl AttributeList {
    pub(crate) fn new() -> Own<AttributeList> {
        Own::new(AttributeList {
            table: &ATTRIBUTE_LIST,
            refs: AtomicU32::new(1),
            map: Mutex::new(HashMap::new()),
        })
    }
}

/// An attribute's key.
///
/// # Safety
/// `id` is null or NUL-ended.
unsafe fn key(id: *const c_char) -> Option<String> {
    // SAFETY: the caller's.
    (!id.is_null()).then(|| unsafe { CStr::from_ptr(id) }.to_string_lossy().into_owned())
}

/// Sets an attribute of list `this`.
///
/// # Safety
/// `this` is an attribute list's table 0; `id` is null or NUL-ended.
unsafe fn attr_set(this: Raw, id: *const c_char, value: Attr) -> TResult {
    // SAFETY: the caller's.
    let (list, key) = unsafe { (object::<AttributeList>(this, 0), key(id)) };
    let Some(key) = key else {
        return result::INVALID_ARGUMENT;
    };
    lock(&list.map).insert(key, value);
    result::OK
}

/// Runs `f` on an attribute of list `this`; `FALSE` when it has none.
///
/// # Safety
/// As [`attr_set`].
unsafe fn attr_get(this: Raw, id: *const c_char, f: impl FnOnce(&Attr) -> TResult) -> TResult {
    // SAFETY: the caller's.
    let (list, key) = unsafe { (object::<AttributeList>(this, 0), key(id)) };
    let Some(key) = key else {
        return result::INVALID_ARGUMENT;
    };
    lock(&list.map).get(&key).map_or(result::FALSE, f)
}

unsafe extern "system" fn attr_set_int(this: Raw, id: *const c_char, v: i64) -> TResult {
    // SAFETY: a plug-in's call on an attribute list.
    unsafe { attr_set(this, id, Attr::Int(v)) }
}

unsafe extern "system" fn attr_get_int(this: Raw, id: *const c_char, v: *mut i64) -> TResult {
    // SAFETY: as above; `v` points at an i64.
    unsafe {
        attr_get(this, id, |a| match a {
            Attr::Int(n) if !v.is_null() => {
                *v = *n;
                result::OK
            }
            _ => result::FALSE,
        })
    }
}

unsafe extern "system" fn attr_set_float(this: Raw, id: *const c_char, v: f64) -> TResult {
    // SAFETY: a plug-in's call on an attribute list.
    unsafe { attr_set(this, id, Attr::Float(v)) }
}

unsafe extern "system" fn attr_get_float(this: Raw, id: *const c_char, v: *mut f64) -> TResult {
    // SAFETY: as above; `v` points at an f64.
    unsafe {
        attr_get(this, id, |a| match a {
            Attr::Float(n) if !v.is_null() => {
                *v = *n;
                result::OK
            }
            _ => result::FALSE,
        })
    }
}

unsafe extern "system" fn attr_set_string(this: Raw, id: *const c_char, s: *const u16) -> TResult {
    if s.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: `s` is NUL-ended UTF-16.
    let text = unsafe {
        let mut n = 0;
        while *s.add(n) != 0 {
            n += 1;
        }
        std::slice::from_raw_parts(s, n).to_vec()
    };
    // SAFETY: a plug-in's call on an attribute list.
    unsafe { attr_set(this, id, Attr::Text(text)) }
}

unsafe extern "system" fn attr_get_string(
    this: Raw,
    id: *const c_char,
    out: *mut u16,
    size_in_bytes: u32,
) -> TResult {
    let room = size_in_bytes as usize / 2;
    if out.is_null() || room == 0 {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: `out` has room for `room` characters.
    unsafe {
        attr_get(this, id, |a| match a {
            Attr::Text(t) => {
                let n = t.len().min(room - 1);
                std::ptr::copy_nonoverlapping(t.as_ptr(), out, n);
                *out.add(n) = 0;
                result::OK
            }
            _ => result::FALSE,
        })
    }
}

unsafe extern "system" fn attr_set_binary(
    this: Raw,
    id: *const c_char,
    data: *const c_void,
    size: u32,
) -> TResult {
    let bytes = if data.is_null() || size == 0 {
        Vec::new()
    } else {
        // SAFETY: `size` bytes at `data`.
        unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size as usize) }.to_vec()
    };
    // SAFETY: a plug-in's call on an attribute list.
    unsafe { attr_set(this, id, Attr::Binary(bytes)) }
}

unsafe extern "system" fn attr_get_binary(
    this: Raw,
    id: *const c_char,
    data: *mut *const c_void,
    size: *mut u32,
) -> TResult {
    if data.is_null() || size.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: the bytes stay in the list until the attribute changes,
    // as the SDK's hosts keep them.
    unsafe {
        attr_get(this, id, |a| match a {
            Attr::Binary(b) => {
                *data = b.as_ptr().cast();
                *size = b.len() as u32;
                result::OK
            }
            _ => result::FALSE,
        })
    }
}

// ---- Memory streams ------------------------------------------------------------

#[repr(C)]
pub(crate) struct Stream {
    table: &'static IBStreamVtbl,
    sizeable: &'static ISizeableStreamVtbl,
    refs: AtomicU32,
    /// The bytes and the position.
    data: Mutex<(Vec<u8>, usize)>,
}

impl HostObject for Stream {
    fn table(id: &Tuid) -> Option<usize> {
        if *id == iid::BSTREAM {
            Some(0)
        } else if *id == iid::SIZEABLE_STREAM {
            Some(1)
        } else {
            None
        }
    }

    fn refs(&self) -> &AtomicU32 {
        &self.refs
    }
}

static STREAM: IBStreamVtbl = IBStreamVtbl {
    unknown: unknown::<Stream, 0>(),
    read: stream_read,
    write: stream_write,
    seek: stream_seek,
    tell: stream_tell,
};

static SIZEABLE: ISizeableStreamVtbl = ISizeableStreamVtbl {
    unknown: unknown::<Stream, 1>(),
    get_stream_size: stream_size,
    set_stream_size: stream_set_size,
};

impl Stream {
    /// A stream of `bytes`, read from the start.
    pub(crate) fn new(bytes: Vec<u8>) -> Own<Stream> {
        Own::new(Stream {
            table: &STREAM,
            sizeable: &SIZEABLE,
            refs: AtomicU32::new(1),
            data: Mutex::new((bytes, 0)),
        })
    }

    pub(crate) fn bytes(&self) -> Vec<u8> {
        lock(&self.data).0.clone()
    }
}

unsafe extern "system" fn stream_read(
    this: Raw,
    buf: *mut c_void,
    n: i32,
    read: *mut i32,
) -> TResult {
    if n < 0 || (buf.is_null() && n > 0) {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: a stream's table 0; `buf` has room for `n` bytes.
    unsafe {
        let s = object::<Stream>(this, 0);
        let mut d = lock(&s.data);
        let (bytes, pos) = &mut *d;
        let take = (n as usize).min(bytes.len().saturating_sub(*pos));
        std::ptr::copy_nonoverlapping(bytes.as_ptr().add(*pos), buf.cast::<u8>(), take);
        *pos += take;
        if !read.is_null() {
            *read = take as i32;
        }
    }
    result::OK
}

unsafe extern "system" fn stream_write(
    this: Raw,
    buf: *mut c_void,
    n: i32,
    written: *mut i32,
) -> TResult {
    if n < 0 || (buf.is_null() && n > 0) {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: a stream's table 0; `n` bytes at `buf`.
    unsafe {
        let s = object::<Stream>(this, 0);
        let mut d = lock(&s.data);
        let (bytes, pos) = &mut *d;
        let end = *pos + n as usize;
        if bytes.len() < end {
            bytes.resize(end, 0);
        }
        std::ptr::copy_nonoverlapping(buf.cast::<u8>(), bytes.as_mut_ptr().add(*pos), n as usize);
        *pos = end;
        if !written.is_null() {
            *written = n;
        }
    }
    result::OK
}

unsafe extern "system" fn stream_seek(this: Raw, to: i64, mode: i32, at: *mut i64) -> TResult {
    // SAFETY: a stream's table 0.
    let s = unsafe { object::<Stream>(this, 0) };
    let mut d = lock(&s.data);
    let from = match mode {
        0 => 0,
        1 => d.1 as i64,
        2 => d.0.len() as i64,
        _ => return result::INVALID_ARGUMENT,
    };
    let pos = (from + to).max(0);
    d.1 = pos as usize;
    if !at.is_null() {
        // SAFETY: `at` points at an i64.
        unsafe { *at = pos };
    }
    result::OK
}

unsafe extern "system" fn stream_tell(this: Raw, at: *mut i64) -> TResult {
    if at.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: a stream's table 0; `at` points at an i64.
    unsafe {
        let s = object::<Stream>(this, 0);
        *at = lock(&s.data).1 as i64;
    }
    result::OK
}

unsafe extern "system" fn stream_size(this: Raw, size: *mut i64) -> TResult {
    if size.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: a stream's table 1; `size` points at an i64.
    unsafe {
        let s = object::<Stream>(this, 1);
        *size = lock(&s.data).0.len() as i64;
    }
    result::OK
}

unsafe extern "system" fn stream_set_size(this: Raw, size: i64) -> TResult {
    if size < 0 {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: a stream's table 1.
    let s = unsafe { object::<Stream>(this, 1) };
    let mut d = lock(&s.data);
    d.0.resize(size as usize, 0);
    d.1 = d.1.min(size as usize);
    result::OK
}

// ---- Parameter changes --------------------------------------------------------

#[repr(C)]
pub(crate) struct ParamValueQueue {
    table: &'static IParamValueQueueVtbl,
    refs: AtomicU32,
    id: AtomicU32,
    points: Mutex<Vec<(i32, f64)>>,
}

impl HostObject for ParamValueQueue {
    fn table(id: &Tuid) -> Option<usize> {
        (*id == iid::PARAM_VALUE_QUEUE).then_some(0)
    }

    fn refs(&self) -> &AtomicU32 {
        &self.refs
    }
}

static PARAM_VALUE_QUEUE: IParamValueQueueVtbl = IParamValueQueueVtbl {
    unknown: unknown::<ParamValueQueue, 0>(),
    get_parameter_id: queue_id,
    get_point_count: queue_count,
    get_point: queue_point,
    add_point: queue_add,
};

impl ParamValueQueue {
    fn new() -> Own<ParamValueQueue> {
        Own::new(ParamValueQueue {
            table: &PARAM_VALUE_QUEUE,
            refs: AtomicU32::new(1),
            id: AtomicU32::new(0),
            points: Mutex::new(Vec::new()),
        })
    }
}

unsafe extern "system" fn queue_id(this: Raw) -> u32 {
    // SAFETY: a queue's table 0.
    unsafe { object::<ParamValueQueue>(this, 0) }
        .id
        .load(Ordering::Acquire)
}

unsafe extern "system" fn queue_count(this: Raw) -> i32 {
    // SAFETY: a queue's table 0.
    lock(&unsafe { object::<ParamValueQueue>(this, 0) }.points).len() as i32
}

unsafe extern "system" fn queue_point(this: Raw, i: i32, offset: *mut i32, v: *mut f64) -> TResult {
    // SAFETY: a queue's table 0; `offset` and `v` point at their types.
    unsafe {
        let q = object::<ParamValueQueue>(this, 0);
        match lock(&q.points).get(i.max(0) as usize) {
            Some(&(o, value)) if i >= 0 && !offset.is_null() && !v.is_null() => {
                *offset = o;
                *v = value;
                result::OK
            }
            _ => result::INVALID_ARGUMENT,
        }
    }
}

unsafe extern "system" fn queue_add(this: Raw, offset: i32, v: f64, index: *mut i32) -> TResult {
    // SAFETY: a queue's table 0; `index` points at an i32 when not null.
    unsafe {
        let q = object::<ParamValueQueue>(this, 0);
        let mut p = lock(&q.points);
        p.push((offset, v));
        if !index.is_null() {
            *index = p.len() as i32 - 1;
        }
    }
    result::OK
}

#[repr(C)]
pub(crate) struct ParameterChanges {
    table: &'static IParameterChangesVtbl,
    refs: AtomicU32,
    /// The queues, those of the block first (`used` of them).
    queues: Mutex<Vec<Own<ParamValueQueue>>>,
    used: AtomicUsize,
}

impl HostObject for ParameterChanges {
    fn table(id: &Tuid) -> Option<usize> {
        (*id == iid::PARAMETER_CHANGES).then_some(0)
    }

    fn refs(&self) -> &AtomicU32 {
        &self.refs
    }
}

static PARAMETER_CHANGES: IParameterChangesVtbl = IParameterChangesVtbl {
    unknown: unknown::<ParameterChanges, 0>(),
    get_parameter_count: changes_count,
    get_parameter_data: changes_data,
    add_parameter_data: changes_add,
};

impl ParameterChanges {
    pub(crate) fn new() -> Own<ParameterChanges> {
        Own::new(ParameterChanges {
            table: &PARAMETER_CHANGES,
            refs: AtomicU32::new(1),
            queues: Mutex::new(Vec::new()),
            used: AtomicUsize::new(0),
        })
    }

    /// The block's changes: each parameter's new value from its first
    /// sample (the last given for a parameter wins).
    pub(crate) fn set(&self, changes: &[(u32, f64)]) {
        let mut queues = lock(&self.queues);
        let mut ids: Vec<u32> = Vec::new();
        for &(id, v) in changes {
            let i = match ids.iter().position(|&x| x == id) {
                Some(i) => i,
                None => {
                    ids.push(id);
                    if queues.len() < ids.len() {
                        queues.push(ParamValueQueue::new());
                    }
                    ids.len() - 1
                }
            };
            let q = queues[i].get();
            q.id.store(id, Ordering::Release);
            let mut p = lock(&q.points);
            p.clear();
            p.push((0, v));
        }
        self.used.store(ids.len(), Ordering::Release);
    }

    pub(crate) fn count(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }
}

unsafe extern "system" fn changes_count(this: Raw) -> i32 {
    // SAFETY: the changes' table 0.
    unsafe { object::<ParameterChanges>(this, 0) }.count() as i32
}

unsafe extern "system" fn changes_data(this: Raw, i: i32) -> Raw {
    // SAFETY: the changes' table 0. The queue is the list's: no reference
    // is counted for the caller (as the SDK's hosts do).
    let c = unsafe { object::<ParameterChanges>(this, 0) };
    if i < 0 || i as usize >= c.count() {
        return std::ptr::null_mut();
    }
    lock(&c.queues)[i as usize].raw(0)
}

unsafe extern "system" fn changes_add(this: Raw, id: *const u32, index: *mut i32) -> Raw {
    if id.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: the changes' table 0; `id` points at a u32, `index` at an
    // i32 when not null.
    unsafe {
        let c = object::<ParameterChanges>(this, 0);
        let mut queues = lock(&c.queues);
        let used = c.count();
        let found = (0..used).find(|&i| queues[i].get().id.load(Ordering::Acquire) == *id);
        let i = found.unwrap_or_else(|| {
            if queues.len() <= used {
                queues.push(ParamValueQueue::new());
            }
            let q = queues[used].get();
            q.id.store(*id, Ordering::Release);
            lock(&q.points).clear();
            c.used.store(used + 1, Ordering::Release);
            used
        });
        if !index.is_null() {
            *index = i as i32;
        }
        queues[i].raw(0)
    }
}

// ---- The component handler ---------------------------------------------------

/// What a plug-in's controller reports.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edit {
    /// The user took a control.
    Begin(u32),
    /// A parameter's new value (normalized).
    Perform(u32, f64),
    /// The user let go of a control.
    End(u32),
    /// Its parameters, their values or its layout changed
    /// (`IComponentHandler::restartComponent`'s flags).
    Restart(i32),
    /// Something besides its parameters changed.
    Dirty,
}

#[repr(C)]
pub(crate) struct ComponentHandler {
    table: &'static IComponentHandlerVtbl,
    table2: &'static IComponentHandler2Vtbl,
    refs: AtomicU32,
    edits: Mutex<Vec<Edit>>,
}

impl HostObject for ComponentHandler {
    fn table(id: &Tuid) -> Option<usize> {
        if *id == iid::COMPONENT_HANDLER {
            Some(0)
        } else if *id == iid::COMPONENT_HANDLER2 {
            Some(1)
        } else {
            None
        }
    }

    fn refs(&self) -> &AtomicU32 {
        &self.refs
    }
}

static COMPONENT_HANDLER: IComponentHandlerVtbl = IComponentHandlerVtbl {
    unknown: unknown::<ComponentHandler, 0>(),
    begin_edit,
    perform_edit,
    end_edit,
    restart_component,
};

static COMPONENT_HANDLER2: IComponentHandler2Vtbl = IComponentHandler2Vtbl {
    unknown: unknown::<ComponentHandler, 1>(),
    set_dirty,
    request_open_editor,
    start_group_edit: group_edit,
    finish_group_edit: group_edit,
};

impl ComponentHandler {
    pub(crate) fn new() -> Own<ComponentHandler> {
        Own::new(ComponentHandler {
            table: &COMPONENT_HANDLER,
            table2: &COMPONENT_HANDLER2,
            refs: AtomicU32::new(1),
            edits: Mutex::new(Vec::new()),
        })
    }

    pub(crate) fn take(&self) -> Vec<Edit> {
        std::mem::take(&mut *lock(&self.edits))
    }
}

/// Notes an edit on handler `this` (table `k`).
///
/// # Safety
/// `this` is a handler's table `k`.
unsafe fn note(this: Raw, k: usize, e: Edit) -> TResult {
    // SAFETY: the caller's.
    let h = unsafe { object::<ComponentHandler>(this, k) };
    lock(&h.edits).push(e);
    result::OK
}

unsafe extern "system" fn begin_edit(this: Raw, id: u32) -> TResult {
    // SAFETY: a handler's table 0.
    unsafe { note(this, 0, Edit::Begin(id)) }
}

unsafe extern "system" fn perform_edit(this: Raw, id: u32, v: f64) -> TResult {
    // SAFETY: a handler's table 0.
    unsafe { note(this, 0, Edit::Perform(id, v)) }
}

unsafe extern "system" fn end_edit(this: Raw, id: u32) -> TResult {
    // SAFETY: a handler's table 0.
    unsafe { note(this, 0, Edit::End(id)) }
}

unsafe extern "system" fn restart_component(this: Raw, flags: i32) -> TResult {
    // SAFETY: a handler's table 0.
    unsafe { note(this, 0, Edit::Restart(flags)) }
}

unsafe extern "system" fn set_dirty(this: Raw, dirty: u8) -> TResult {
    if dirty == 0 {
        return result::OK;
    }
    // SAFETY: a handler's table 1.
    unsafe { note(this, 1, Edit::Dirty) }
}

unsafe extern "system" fn request_open_editor(_this: Raw, _name: *const c_char) -> TResult {
    result::FALSE
}

unsafe extern "system" fn group_edit(_this: Raw) -> TResult {
    result::OK
}

// ---- The editor's frame ---------------------------------------------------------

/// What resizes the window an editor lives in (to its view's new width
/// and height).
pub(crate) type Resize = Box<dyn FnMut(i32, i32) + Send>;

#[repr(C)]
pub(crate) struct PlugFrame {
    table: &'static IPlugFrameVtbl,
    run_loop: &'static IRunLoopVtbl,
    refs: AtomicU32,
    resize: Mutex<Option<Resize>>,
}

impl HostObject for PlugFrame {
    fn table(id: &Tuid) -> Option<usize> {
        if *id == iid::PLUG_FRAME {
            Some(0)
        } else if *id == iid::RUN_LOOP && RUN_LOOP_OFFERED {
            Some(1)
        } else {
            None
        }
    }

    fn refs(&self) -> &AtomicU32 {
        &self.refs
    }
}

static PLUG_FRAME: IPlugFrameVtbl = IPlugFrameVtbl {
    unknown: unknown::<PlugFrame, 0>(),
    resize_view,
};

static FRAME_RUN_LOOP: IRunLoopVtbl = IRunLoopVtbl {
    unknown: unknown::<PlugFrame, 1>(),
    register_event_handler,
    unregister_event_handler,
    register_timer,
    unregister_timer,
};

impl PlugFrame {
    pub(crate) fn new() -> Own<PlugFrame> {
        Own::new(PlugFrame {
            table: &PLUG_FRAME,
            run_loop: &FRAME_RUN_LOOP,
            refs: AtomicU32::new(1),
            resize: Mutex::new(None),
        })
    }

    pub(crate) fn set_resize(&self, r: Option<Resize>) {
        *lock(&self.resize) = r;
    }
}

/// A view asks for another size: the window takes it, then the view is
/// told (in the same call, as the specification asks).
unsafe extern "system" fn resize_view(this: Raw, view: Raw, rect: *mut ViewRect) -> TResult {
    if view.is_null() || rect.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: the frame's table 0; `view` is the plug-in's view (its
    // table an IPlugView's) and `rect` points at a rectangle.
    unsafe {
        let f = object::<PlugFrame>(this, 0);
        let r = *rect;
        if let Some(resize) = lock(&f.resize).as_mut() {
            resize(r.right - r.left, r.bottom - r.top);
        }
        let table = &**(view as *const *const IPlugViewVtbl);
        (table.on_size)(view, rect);
    }
    result::OK
}

// ---- Linux's run loop ----------------------------------------------------------

/// Only Linux has no run loop of the system's for plug-ins' editors: the
/// frame offers its own there (VST 3's `Steinberg::Linux::IRunLoop`).
const RUN_LOOP_OFFERED: bool = cfg!(target_os = "linux");

/// What plug-ins asked the run loop for: file descriptors to watch, each
/// with its handler, and timers (their interval and when they are due
/// next); a reference to each handler is held while it is registered.
struct Loop {
    handlers: Vec<(usize, i32)>,
    timers: Vec<(usize, Duration, Instant)>,
}

static LOOP: Mutex<Loop> = Mutex::new(Loop {
    handlers: Vec::new(),
    timers: Vec::new(),
});

/// FUnknown's functions of a plug-in's object.
///
/// # Safety
/// `obj` is an interface pointer of the plug-in's.
unsafe fn unknown_of<'a>(obj: Raw) -> &'a FUnknownVtbl {
    // SAFETY: the caller's; every table begins with FUnknown's.
    unsafe { &**(obj as *const *const FUnknownVtbl) }
}

/// Takes a reference to handler `h` (kept while it is registered).
///
/// # Safety
/// `h` is an interface pointer of the plug-in's.
unsafe fn hold(h: Raw) {
    // SAFETY: the caller's.
    unsafe { (unknown_of(h).add_ref)(h) };
}

/// Gives back a reference taken with [`hold`].
///
/// # Safety
/// `h` is a handler a reference was taken to.
unsafe fn let_go(h: usize) {
    let h = h as Raw;
    // SAFETY: the caller's.
    unsafe { (unknown_of(h).release)(h) };
}

unsafe extern "system" fn register_event_handler(_this: Raw, handler: Raw, fd: i32) -> TResult {
    if handler.is_null() || fd < 0 {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: the plug-in's handler.
    unsafe { hold(handler) };
    lock(&LOOP).handlers.push((handler as usize, fd));
    result::OK
}

unsafe extern "system" fn unregister_event_handler(_this: Raw, handler: Raw) -> TResult {
    let gone: Vec<usize> = {
        let mut l = lock(&LOOP);
        let (gone, kept) = l
            .handlers
            .drain(..)
            .partition(|(h, _)| *h == handler as usize);
        l.handlers = kept;
        gone.into_iter().map(|(h, _)| h).collect()
    };
    for h in &gone {
        // SAFETY: held when it was registered.
        unsafe { let_go(*h) };
    }
    if gone.is_empty() {
        result::INVALID_ARGUMENT
    } else {
        result::OK
    }
}

unsafe extern "system" fn register_timer(_this: Raw, handler: Raw, ms: u64) -> TResult {
    if handler.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: the plug-in's handler.
    unsafe { hold(handler) };
    let every = Duration::from_millis(ms.max(1));
    lock(&LOOP)
        .timers
        .push((handler as usize, every, Instant::now() + every));
    result::OK
}

unsafe extern "system" fn unregister_timer(_this: Raw, handler: Raw) -> TResult {
    let gone: Vec<usize> = {
        let mut l = lock(&LOOP);
        let (gone, kept) = l
            .timers
            .drain(..)
            .partition(|(h, _, _)| *h == handler as usize);
        l.timers = kept;
        gone.into_iter().map(|(h, _, _)| h).collect()
    };
    for h in &gone {
        // SAFETY: held when it was registered.
        unsafe { let_go(*h) };
    }
    if gone.is_empty() {
        result::INVALID_ARGUMENT
    } else {
        result::OK
    }
}

/// The file descriptors among `fds` that can be read now (or are closed
/// or failed).
#[cfg(unix)]
fn readable(fds: &[i32]) -> Vec<i32> {
    #[repr(C)]
    struct PollFd {
        fd: i32,
        events: i16,
        revents: i16,
    }
    unsafe extern "C" {
        fn poll(fds: *mut PollFd, n: std::ffi::c_ulong, timeout: i32) -> i32;
    }
    const IN: i16 = 0x1;
    const ERR: i16 = 0x8;
    const HUP: i16 = 0x10;
    let mut all: Vec<PollFd> = fds
        .iter()
        .map(|&fd| PollFd {
            fd,
            events: IN,
            revents: 0,
        })
        .collect();
    // SAFETY: an array of `all.len()` entries; no waiting.
    let n = unsafe { poll(all.as_mut_ptr(), all.len() as std::ffi::c_ulong, 0) };
    if n <= 0 {
        return Vec::new();
    }
    all.iter()
        .filter(|p| p.revents & (IN | ERR | HUP) != 0)
        .map(|p| p.fd)
        .collect()
}

#[cfg(not(unix))]
fn readable(_fds: &[i32]) -> Vec<i32> {
    Vec::new()
}

/// Runs what plug-ins' editors put on the run loop: the handlers of the
/// file descriptors that can be read, and the timers that are due. Call
/// it often (each frame) on the thread the editors live on, while one is
/// open; nothing to do elsewhere than on Linux.
pub fn run_loop() {
    let now = Instant::now();
    // What to call, each handler held for the call (a handler may
    // unregister itself, or another, meanwhile).
    let (events, timers) = {
        let mut l = lock(&LOOP);
        if l.handlers.is_empty() && l.timers.is_empty() {
            return;
        }
        let fds: Vec<i32> = l.handlers.iter().map(|(_, fd)| *fd).collect();
        let ready = readable(&fds);
        let events: Vec<(usize, i32)> = l
            .handlers
            .iter()
            .filter(|(_, fd)| ready.contains(fd))
            .copied()
            .collect();
        let mut timers = Vec::new();
        for (h, every, next) in &mut l.timers {
            if *next <= now {
                // Late timers fire once, then keep their pace from now.
                *next = (*next + *every).max(now);
                timers.push(*h);
            }
        }
        for h in events.iter().map(|(h, _)| *h).chain(timers.iter().copied()) {
            // SAFETY: registered, so held: alive.
            unsafe { hold(h as Raw) };
        }
        (events, timers)
    };
    for (h, fd) in events {
        let raw = h as Raw;
        // SAFETY: a registered event handler (its table an
        // IEventHandler's), held for the call.
        unsafe {
            let table = &**(raw as *const *const IEventHandlerVtbl);
            (table.on_fd_is_set)(raw, fd);
            let_go(h);
        }
    }
    for h in timers {
        let raw = h as Raw;
        // SAFETY: a registered timer handler, held for the call.
        unsafe {
            let table = &**(raw as *const *const ITimerHandlerVtbl);
            (table.on_timer)(raw);
            let_go(h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::com::ComPtr;

    /// A timer handler as a plug-in's: counts its calls.
    #[repr(C)]
    struct Ticker {
        table: &'static ITimerHandlerVtbl,
        refs: AtomicU32,
        ticks: AtomicU32,
    }

    impl HostObject for Ticker {
        fn table(id: &Tuid) -> Option<usize> {
            (*id == iid::TIMER_HANDLER).then_some(0)
        }

        fn refs(&self) -> &AtomicU32 {
            &self.refs
        }
    }

    unsafe extern "system" fn tick(this: Raw) {
        // SAFETY: table 0 of a Ticker.
        unsafe { object::<Ticker>(this, 0) }
            .ticks
            .fetch_add(1, Ordering::Relaxed);
    }

    static TICKER: ITimerHandlerVtbl = ITimerHandlerVtbl {
        unknown: unknown::<Ticker, 0>(),
        on_timer: tick,
    };

    /// Linux's run loop: a timer runs once due, again at its pace, and no
    /// more once unregistered; its reference is held meanwhile.
    #[test]
    fn the_run_loop_runs_timers_until_they_go() {
        let t = Own::new(Ticker {
            table: &TICKER,
            refs: AtomicU32::new(1),
            ticks: AtomicU32::new(0),
        });
        let raw = t.raw(0);
        // SAFETY: the frame's run loop functions, called as a plug-in
        // would, with a handler of its own.
        unsafe {
            assert_eq!(register_timer(std::ptr::null_mut(), raw, 5), result::OK);
            assert_eq!(t.get().refs.load(Ordering::Relaxed), 2);
            run_loop();
            assert_eq!(t.get().ticks.load(Ordering::Relaxed), 0, "not due yet");
            std::thread::sleep(Duration::from_millis(12));
            run_loop();
            assert_eq!(t.get().ticks.load(Ordering::Relaxed), 1);
            std::thread::sleep(Duration::from_millis(12));
            run_loop();
            assert_eq!(t.get().ticks.load(Ordering::Relaxed), 2);
            assert_eq!(unregister_timer(std::ptr::null_mut(), raw), result::OK);
            assert_eq!(t.get().refs.load(Ordering::Relaxed), 1);
            std::thread::sleep(Duration::from_millis(12));
            run_loop();
            assert_eq!(t.get().ticks.load(Ordering::Relaxed), 2);
            assert_eq!(
                unregister_timer(std::ptr::null_mut(), raw),
                result::INVALID_ARGUMENT
            );
        }
        // The frame answers for the run loop where plug-ins need it.
        assert_eq!(
            PlugFrame::table(&iid::RUN_LOOP).is_some(),
            cfg!(target_os = "linux")
        );
    }

    #[test]
    fn streams_read_write_and_seek() {
        let s = Stream::new(b"abc".to_vec());
        // SAFETY: the stream's own table, called as a plug-in would.
        let p: ComPtr<IBStreamVtbl> = unsafe {
            let raw = s.raw(0);
            (STREAM.unknown.add_ref)(raw);
            ComPtr::from_raw(raw).unwrap()
        };
        let mut buf = [0u8; 8];
        let mut n = 0;
        // SAFETY: as above.
        unsafe {
            (p.vt().read)(p.raw(), buf.as_mut_ptr().cast(), 8, &mut n);
            assert_eq!((n, &buf[..3]), (3, &b"abc"[..]));
            (p.vt().seek)(p.raw(), 1, 0, std::ptr::null_mut());
            (p.vt().write)(p.raw(), b"XYZ".as_ptr().cast_mut().cast(), 3, &mut n);
        }
        assert_eq!(s.get().bytes(), b"aXYZ");
        // The sizeable side of the same object.
        let sized: ComPtr<ISizeableStreamVtbl> = p.query(&iid::SIZEABLE_STREAM).unwrap();
        let mut size = 0;
        // SAFETY: as above.
        unsafe { (sized.vt().get_stream_size)(sized.raw(), &mut size) };
        assert_eq!(size, 4);
        drop(sized);
        drop(p);
        // The host's reference is the last one.
        assert_eq!(s.get().refs().load(Ordering::Acquire), 1);
    }

    #[test]
    fn parameter_changes_hold_one_queue_a_parameter() {
        let c = ParameterChanges::new();
        c.get().set(&[(3, 0.25), (5, 1.0), (3, 0.5)]);
        // SAFETY: the changes' own table, called as a plug-in would.
        unsafe {
            let raw = c.raw(0);
            assert_eq!(changes_count(raw), 2);
            let q = changes_data(raw, 0);
            assert_eq!(queue_id(q), 3);
            let (mut off, mut v) = (0, 0.0);
            assert_eq!(queue_point(q, 0, &mut off, &mut v), result::OK);
            assert_eq!((off, v), (0, 0.5));
            assert!(changes_data(raw, 2).is_null());
        }
    }

    #[test]
    fn messages_carry_attributes() {
        let host = HostApplication::new();
        let mut obj: Raw = std::ptr::null_mut();
        let (mut cid, mut id) = (iid::MESSAGE, iid::MESSAGE);
        // SAFETY: the host's own tables, called as a plug-in would.
        unsafe {
            assert_eq!(
                host_create(host.raw(0), &mut cid, &mut id, &mut obj),
                result::OK
            );
            let m: ComPtr<IMessageVtbl> = ComPtr::from_raw(obj).unwrap();
            (m.vt().set_message_id)(m.raw(), c"Hello".as_ptr());
            assert_eq!(CStr::from_ptr((m.vt().get_message_id)(m.raw())), c"Hello");
            let list = (m.vt().get_attributes)(m.raw());
            attr_set_int(list, c"Version".as_ptr(), 7);
            let mut v = 0;
            assert_eq!(attr_get_int(list, c"Version".as_ptr(), &mut v), result::OK);
            assert_eq!(v, 7);
            assert_eq!(attr_get_int(list, c"Other".as_ptr(), &mut v), result::FALSE);
        }
    }
}
