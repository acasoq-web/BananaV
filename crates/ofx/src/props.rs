//! Property sets: what the host and a plug-in tell each other about every
//! object (the host, an effect, a clip, an image, a parameter, an action's
//! arguments), and the property suite that reads and writes them.
//!
//! A set is a map from a property's name to its values, all of one type
//! (strings, doubles, ints or pointers). The host defines each set's
//! properties with the specification's defaults; a plug-in may also set
//! properties of its own. The suite is lenient where the specification
//! leaves room: an int property read as a double (or the reverse) gives the
//! converted value, as other hosts do.
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::ffi::{CStr, CString, c_char, c_double, c_int, c_void};
use std::sync::{Mutex, MutexGuard};

use bv_ofx_sys::names::*;
use bv_ofx_sys::{OfxPropertySetHandle, OfxPropertySuiteV1, OfxStatus};

/// What starts every property set, to tell it from other handles.
const MAGIC: u64 = u64::from_le_bytes(*b"bvOFXprp");

/// What sort of object a set describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetKind {
    Host,
    EffectDescriptor,
    EffectInstance,
    ClipDescriptor,
    ClipInstance,
    Image,
    ParamDescriptor,
    ParamInstance,
    ParamSet,
    /// An action's arguments.
    Arguments,
    Interact,
}

/// A property's values.
#[derive(Clone, Debug, PartialEq)]
pub enum Values {
    Str(Vec<CString>),
    Dbl(Vec<f64>),
    Int(Vec<i32>),
    /// Pointers, as addresses.
    Ptr(Vec<usize>),
}

impl Values {
    fn len(&self) -> usize {
        match self {
            Values::Str(v) => v.len(),
            Values::Dbl(v) => v.len(),
            Values::Int(v) => v.len(),
            Values::Ptr(v) => v.len(),
        }
    }

    /// `n` values of this type, all empty or zero.
    fn zeros(&self, n: usize) -> Values {
        match self {
            Values::Str(_) => Values::Str(vec![CString::default(); n]),
            Values::Dbl(_) => Values::Dbl(vec![0.0; n]),
            Values::Int(_) => Values::Int(vec![0; n]),
            Values::Ptr(_) => Values::Ptr(vec![0; n]),
        }
    }
}

#[derive(Clone, Debug)]
struct Prop {
    values: Values,
    default: Values,
    /// The fixed number of values, or `None` when it varies.
    dimension: Option<usize>,
}

/// One value a plug-in writes.
#[derive(Clone, Debug)]
enum One {
    Str(CString),
    Dbl(f64),
    Int(i32),
    Ptr(usize),
}

/// A property set. A handle (`OfxPropertySetHandle`) points at it, so it
/// must not move while plug-ins may hold one: sets live in boxes.
#[repr(C)]
#[derive(Debug)]
pub struct PropSet {
    magic: u64,
    kind: SetKind,
    props: Mutex<BTreeMap<CString, Prop>>,
}

impl PropSet {
    pub fn new(kind: SetKind) -> PropSet {
        PropSet {
            magic: MAGIC,
            kind,
            props: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn kind(&self) -> SetKind {
        self.kind
    }

    /// The handle plug-ins get.
    pub fn handle(&self) -> OfxPropertySetHandle {
        self as *const PropSet as OfxPropertySetHandle
    }

    /// The set behind a handle a plug-in gave, if it is one.
    ///
    /// # Safety
    /// `h` is null or points to memory that can be read; a set it points
    /// to stays alive while the returned reference is used.
    pub unsafe fn from_handle<'a>(h: OfxPropertySetHandle) -> Option<&'a PropSet> {
        if h.is_null() {
            return None;
        }
        let set = h as *const PropSet;
        // SAFETY: the caller says `h` can be read; the magic number tells a
        // set from another handle.
        unsafe { ((*set).magic == MAGIC).then(|| &*set) }
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<CString, Prop>> {
        // A plug-in that panicked inside the host (it cannot: the suite
        // never panics) would poison it; the data stay usable.
        self.props.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn define(&self, name: &CStr, dimension: Option<usize>, default: Values) {
        self.lock().insert(
            name.to_owned(),
            Prop {
                values: default.clone(),
                default,
                dimension,
            },
        );
    }

    /// Defines a string property. `dimension`: `None` when its number of
    /// values varies.
    pub fn define_str(&self, name: &CStr, dimension: Option<usize>, default: &[&CStr]) {
        let v = default.iter().map(|s| (*s).to_owned()).collect();
        self.define(name, dimension, Values::Str(v));
    }

    pub fn define_int(&self, name: &CStr, dimension: Option<usize>, default: &[i32]) {
        self.define(name, dimension, Values::Int(default.to_vec()));
    }

    pub fn define_dbl(&self, name: &CStr, dimension: Option<usize>, default: &[f64]) {
        self.define(name, dimension, Values::Dbl(default.to_vec()));
    }

    pub fn define_ptr(&self, name: &CStr, dimension: Option<usize>) {
        let n = dimension.unwrap_or(0);
        self.define(name, dimension, Values::Ptr(vec![0; n]));
    }

    /// Whether the set has the property.
    pub fn has(&self, name: &CStr) -> bool {
        self.lock().contains_key(name)
    }

    /// Replaces a property's values (defining it when it is not there).
    pub fn set(&self, name: &CStr, values: Values) {
        let mut map = self.lock();
        match map.get_mut(name) {
            Some(p) => p.values = values,
            None => {
                map.insert(
                    name.to_owned(),
                    Prop {
                        default: values.zeros(0),
                        values,
                        dimension: None,
                    },
                );
            }
        }
    }

    pub fn set_str(&self, name: &CStr, values: &[&str]) {
        let v = values.iter().map(|s| cstring(s)).collect();
        self.set(name, Values::Str(v));
    }

    pub fn set_cstr(&self, name: &CStr, value: &CStr) {
        self.set(name, Values::Str(vec![value.to_owned()]));
    }

    pub fn set_int(&self, name: &CStr, values: &[i32]) {
        self.set(name, Values::Int(values.to_vec()));
    }

    pub fn set_dbl(&self, name: &CStr, values: &[f64]) {
        self.set(name, Values::Dbl(values.to_vec()));
    }

    pub fn set_ptr(&self, name: &CStr, value: *const c_void) {
        self.set(name, Values::Ptr(vec![value as usize]));
    }

    /// A property's values, if the set has it.
    pub fn get(&self, name: &CStr) -> Option<Values> {
        self.lock().get(name).map(|p| p.values.clone())
    }

    /// The `i`th string value.
    pub fn str(&self, name: &CStr, i: usize) -> Option<String> {
        match self.lock().get(name)?.values {
            Values::Str(ref v) => v.get(i).map(|s| s.to_string_lossy().into_owned()),
            _ => None,
        }
    }

    /// Every string value.
    pub fn strs(&self, name: &CStr) -> Vec<String> {
        match self.lock().get(name).map(|p| &p.values) {
            Some(Values::Str(v)) => v.iter().map(|s| s.to_string_lossy().into_owned()).collect(),
            _ => Vec::new(),
        }
    }

    /// The `i`th value as an int (a double is rounded).
    pub fn int(&self, name: &CStr, i: usize) -> Option<i32> {
        match self.lock().get(name)?.values {
            Values::Int(ref v) => v.get(i).copied(),
            Values::Dbl(ref v) => v.get(i).map(|d| d.round() as i32),
            _ => None,
        }
    }

    /// Every value as ints.
    pub fn ints(&self, name: &CStr) -> Vec<i32> {
        match self.lock().get(name).map(|p| &p.values) {
            Some(Values::Int(v)) => v.clone(),
            Some(Values::Dbl(v)) => v.iter().map(|d| d.round() as i32).collect(),
            _ => Vec::new(),
        }
    }

    /// The `i`th value as a double (an int is converted).
    pub fn dbl(&self, name: &CStr, i: usize) -> Option<f64> {
        match self.lock().get(name)?.values {
            Values::Dbl(ref v) => v.get(i).copied(),
            Values::Int(ref v) => v.get(i).map(|&n| n as f64),
            _ => None,
        }
    }

    /// Every value as doubles.
    pub fn dbls(&self, name: &CStr) -> Vec<f64> {
        match self.lock().get(name).map(|p| &p.values) {
            Some(Values::Dbl(v)) => v.clone(),
            Some(Values::Int(v)) => v.iter().map(|&n| n as f64).collect(),
            _ => Vec::new(),
        }
    }

    /// The `i`th pointer value.
    pub fn ptr(&self, name: &CStr, i: usize) -> Option<*mut c_void> {
        match self.lock().get(name)?.values {
            Values::Ptr(ref v) => v.get(i).map(|&p| p as *mut c_void),
            _ => None,
        }
    }

    /// A boolean property (an int, non-zero for true).
    pub fn flag(&self, name: &CStr) -> Option<bool> {
        self.int(name, 0).map(|v| v != 0)
    }

    /// Copies every property of `from` (definitions and values).
    pub fn copy_from(&self, from: &PropSet) {
        let theirs = from.lock().clone();
        let mut ours = self.lock();
        for (k, v) in theirs {
            ours.insert(k, v);
        }
    }

    /// Every property's name and values (for reports and tests).
    pub fn snapshot(&self) -> BTreeMap<String, Values> {
        self.lock()
            .iter()
            .map(|(k, p)| (k.to_string_lossy().into_owned(), p.values.clone()))
            .collect()
    }

    fn set_one(&self, name: &CStr, index: c_int, value: One) -> OfxStatus {
        let Ok(i) = usize::try_from(index) else {
            return kOfxStatErrBadIndex;
        };
        let mut map = self.lock();
        let prop = map.entry(name.to_owned()).or_insert_with(|| {
            let values = match &value {
                One::Str(_) => Values::Str(Vec::new()),
                One::Dbl(_) => Values::Dbl(Vec::new()),
                One::Int(_) => Values::Int(Vec::new()),
                One::Ptr(_) => Values::Ptr(Vec::new()),
            };
            Prop {
                default: values.clone(),
                values,
                dimension: None,
            }
        });
        if prop.dimension.is_some_and(|n| i >= n) {
            trace_refusal("set", name, self.kind, kOfxStatErrBadIndex);
            return kOfxStatErrBadIndex;
        }
        match (&mut prop.values, value) {
            (Values::Str(v), One::Str(s)) => put(v, i, s),
            (Values::Dbl(v), One::Dbl(d)) => put(v, i, d),
            (Values::Dbl(v), One::Int(n)) => put(v, i, n as f64),
            (Values::Int(v), One::Int(n)) => put(v, i, n),
            (Values::Int(v), One::Dbl(d)) => put(v, i, d.round() as i32),
            (Values::Ptr(v), One::Ptr(p)) => put(v, i, p),
            _ => {
                trace_refusal("set", name, self.kind, kOfxStatErrValue);
                return kOfxStatErrValue;
            }
        }
        kOfxStatOK
    }

    fn set_many(&self, name: &CStr, values: Values) -> OfxStatus {
        let mut map = self.lock();
        match map.get_mut(name) {
            Some(prop) => {
                let merged = match (&prop.values, values) {
                    (Values::Str(_), v @ Values::Str(_))
                    | (Values::Dbl(_), v @ Values::Dbl(_))
                    | (Values::Int(_), v @ Values::Int(_))
                    | (Values::Ptr(_), v @ Values::Ptr(_)) => v,
                    (Values::Dbl(_), Values::Int(v)) => {
                        Values::Dbl(v.into_iter().map(f64::from).collect())
                    }
                    (Values::Int(_), Values::Dbl(v)) => {
                        Values::Int(v.into_iter().map(|d| d.round() as i32).collect())
                    }
                    _ => {
                        trace_refusal("set", name, self.kind, kOfxStatErrValue);
                        return kOfxStatErrValue;
                    }
                };
                // A property of fixed size keeps its size: fewer values set
                // the first ones.
                prop.values = match prop.dimension {
                    Some(n) if merged.len() < n => {
                        let mut full = prop.values.clone();
                        overwrite(&mut full, &merged);
                        full
                    }
                    _ => merged,
                };
            }
            None => {
                map.insert(
                    name.to_owned(),
                    Prop {
                        default: values.zeros(0),
                        values,
                        dimension: None,
                    },
                );
            }
        }
        kOfxStatOK
    }
}

/// Tells on stderr, when tracing, of a property call the host refused.
fn trace_refusal(what: &str, name: &CStr, kind: SetKind, status: OfxStatus) {
    if crate::suites::tracing() {
        eprintln!(
            "ofx: {what} {name:?} ({kind:?}) → {}",
            crate::info::status_name(status)
        );
    }
}

fn put<T: Clone + Default>(v: &mut Vec<T>, i: usize, value: T) {
    if i >= v.len() {
        v.resize(i + 1, T::default());
    }
    v[i] = value;
}

/// Writes `from`'s values over the first ones of `to` (same type).
fn overwrite(to: &mut Values, from: &Values) {
    match (to, from) {
        (Values::Str(t), Values::Str(f)) => t[..f.len()].clone_from_slice(f),
        (Values::Dbl(t), Values::Dbl(f)) => t[..f.len()].copy_from_slice(f),
        (Values::Int(t), Values::Int(f)) => t[..f.len()].copy_from_slice(f),
        (Values::Ptr(t), Values::Ptr(f)) => t[..f.len()].copy_from_slice(f),
        _ => {}
    }
}

/// A plug-in's C string as text (empty for a null pointer).
///
/// # Safety
/// `p` is null or a C string.
pub(crate) unsafe fn text(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        // SAFETY: as the caller promises.
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

/// A plug-in's C string, copied (empty for a null pointer).
///
/// # Safety
/// `p` is null or a C string.
pub(crate) unsafe fn owned(p: *const c_char) -> CString {
    if p.is_null() {
        CString::default()
    } else {
        // SAFETY: as the caller promises.
        unsafe { CStr::from_ptr(p) }.to_owned()
    }
}

/// A C string of `s` (an inner NUL ends it).
pub fn cstring(s: &str) -> CString {
    let end = s.find('\0').unwrap_or(s.len());
    CString::new(&s[..end]).unwrap_or_default()
}

// The property suite.

/// The set and the property's name a call is about.
///
/// # Safety
/// `h` and `name` are what the plug-in passed: null or readable.
unsafe fn target<'a>(
    h: OfxPropertySetHandle,
    name: *const c_char,
) -> Result<(&'a PropSet, &'a CStr), OfxStatus> {
    // SAFETY: as the caller promises.
    let set = unsafe { PropSet::from_handle(h) }.ok_or(kOfxStatErrBadHandle)?;
    if name.is_null() {
        return Err(kOfxStatErrUnknown);
    }
    // SAFETY: a non-null name is a C string.
    Ok((set, unsafe { CStr::from_ptr(name) }))
}

unsafe extern "C" fn prop_set_pointer(
    h: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: *mut c_void,
) -> OfxStatus {
    // SAFETY: the plug-in's arguments.
    match unsafe { target(h, name) } {
        Ok((set, name)) => set.set_one(name, index, One::Ptr(value as usize)),
        Err(e) => e,
    }
}

unsafe extern "C" fn prop_set_string(
    h: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: *const c_char,
) -> OfxStatus {
    // SAFETY: the plug-in's arguments.
    match unsafe { target(h, name) } {
        Ok((set, name)) => {
            // SAFETY: a plug-in's string, or null.
            let s = unsafe { owned(value) };
            set.set_one(name, index, One::Str(s))
        }
        Err(e) => e,
    }
}

unsafe extern "C" fn prop_set_double(
    h: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: c_double,
) -> OfxStatus {
    // SAFETY: the plug-in's arguments.
    match unsafe { target(h, name) } {
        Ok((set, name)) => set.set_one(name, index, One::Dbl(value)),
        Err(e) => e,
    }
}

unsafe extern "C" fn prop_set_int(
    h: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: c_int,
) -> OfxStatus {
    // SAFETY: the plug-in's arguments.
    match unsafe { target(h, name) } {
        Ok((set, name)) => set.set_one(name, index, One::Int(value)),
        Err(e) => e,
    }
}

/// `count` values the plug-in passes in an array.
///
/// # Safety
/// `values` points to `count` items when `count` > 0.
unsafe fn slice<'a, T>(values: *const T, count: c_int) -> Result<&'a [T], OfxStatus> {
    match usize::try_from(count) {
        Ok(0) => Ok(&[]),
        Ok(n) if !values.is_null() => {
            // SAFETY: as the caller promises.
            Ok(unsafe { std::slice::from_raw_parts(values, n) })
        }
        _ => Err(kOfxStatErrBadIndex),
    }
}

unsafe extern "C" fn prop_set_pointer_n(
    h: OfxPropertySetHandle,
    name: *const c_char,
    count: c_int,
    values: *const *mut c_void,
) -> OfxStatus {
    // SAFETY: the plug-in's arguments.
    let r = unsafe { target(h, name).and_then(|t| slice(values, count).map(|v| (t, v))) };
    match r {
        Ok(((set, name), v)) => {
            set.set_many(name, Values::Ptr(v.iter().map(|&p| p as usize).collect()))
        }
        Err(e) => e,
    }
}

unsafe extern "C" fn prop_set_string_n(
    h: OfxPropertySetHandle,
    name: *const c_char,
    count: c_int,
    values: *const *const c_char,
) -> OfxStatus {
    // SAFETY: the plug-in's arguments.
    let r = unsafe { target(h, name).and_then(|t| slice(values, count).map(|v| (t, v))) };
    match r {
        Ok(((set, name), v)) => {
            let strings = v
                .iter()
                // SAFETY: each item is a C string or null.
                .map(|&s| unsafe { owned(s) })
                .collect();
            set.set_many(name, Values::Str(strings))
        }
        Err(e) => e,
    }
}

unsafe extern "C" fn prop_set_double_n(
    h: OfxPropertySetHandle,
    name: *const c_char,
    count: c_int,
    values: *const c_double,
) -> OfxStatus {
    // SAFETY: the plug-in's arguments.
    let r = unsafe { target(h, name).and_then(|t| slice(values, count).map(|v| (t, v))) };
    match r {
        Ok(((set, name), v)) => set.set_many(name, Values::Dbl(v.to_vec())),
        Err(e) => e,
    }
}

unsafe extern "C" fn prop_set_int_n(
    h: OfxPropertySetHandle,
    name: *const c_char,
    count: c_int,
    values: *const c_int,
) -> OfxStatus {
    // SAFETY: the plug-in's arguments.
    let r = unsafe { target(h, name).and_then(|t| slice(values, count).map(|v| (t, v))) };
    match r {
        Ok(((set, name), v)) => set.set_many(name, Values::Int(v.to_vec())),
        Err(e) => e,
    }
}

/// Reads value `index` of a property with `read`, which gets the values
/// and the index and writes the answer.
///
/// # Safety
/// As [`target`].
unsafe fn get_one(
    h: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    read: impl FnOnce(&Values, usize) -> OfxStatus,
) -> OfxStatus {
    // SAFETY: the plug-in's arguments.
    let (set, name) = match unsafe { target(h, name) } {
        Ok(t) => t,
        Err(e) => return e,
    };
    let Ok(i) = usize::try_from(index) else {
        return kOfxStatErrBadIndex;
    };
    let map = set.lock();
    let Some(prop) = map.get(name) else {
        if crate::suites::tracing() {
            eprintln!("ofx: unknown property {name:?} ({:?})", set.kind());
        }
        return kOfxStatErrUnknown;
    };
    if i >= prop.values.len() {
        trace_refusal("get", name, set.kind(), kOfxStatErrBadIndex);
        return kOfxStatErrBadIndex;
    }
    let status = read(&prop.values, i);
    if status != kOfxStatOK {
        trace_refusal("get", name, set.kind(), status);
    }
    status
}

unsafe extern "C" fn prop_get_pointer(
    h: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: *mut *mut c_void,
) -> OfxStatus {
    if value.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: the plug-in's arguments; `value` is where it wants the answer.
    unsafe {
        get_one(h, name, index, |v, i| match v {
            Values::Ptr(p) => {
                *value = p[i] as *mut c_void;
                kOfxStatOK
            }
            _ => kOfxStatErrValue,
        })
    }
}

unsafe extern "C" fn prop_get_string(
    h: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: *mut *mut c_char,
) -> OfxStatus {
    if value.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: the plug-in's arguments. The string stays where it is until
    // the property changes, as the specification asks.
    unsafe {
        get_one(h, name, index, |v, i| match v {
            Values::Str(s) => {
                *value = s[i].as_ptr() as *mut c_char;
                kOfxStatOK
            }
            _ => kOfxStatErrValue,
        })
    }
}

unsafe extern "C" fn prop_get_double(
    h: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: *mut c_double,
) -> OfxStatus {
    if value.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: the plug-in's arguments.
    unsafe {
        get_one(h, name, index, |v, i| match v {
            Values::Dbl(d) => {
                *value = d[i];
                kOfxStatOK
            }
            Values::Int(n) => {
                *value = n[i] as f64;
                kOfxStatOK
            }
            _ => kOfxStatErrValue,
        })
    }
}

unsafe extern "C" fn prop_get_int(
    h: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: *mut c_int,
) -> OfxStatus {
    if value.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: the plug-in's arguments.
    unsafe {
        get_one(h, name, index, |v, i| match v {
            Values::Int(n) => {
                *value = n[i];
                kOfxStatOK
            }
            Values::Dbl(d) => {
                *value = d[i].round() as c_int;
                kOfxStatOK
            }
            _ => kOfxStatErrValue,
        })
    }
}

/// Reads `count` values of a property with `read`.
///
/// # Safety
/// As [`target`].
unsafe fn get_many(
    h: OfxPropertySetHandle,
    name: *const c_char,
    count: c_int,
    read: impl FnOnce(&Values, usize) -> OfxStatus,
) -> OfxStatus {
    // SAFETY: the plug-in's arguments.
    let (set, name) = match unsafe { target(h, name) } {
        Ok(t) => t,
        Err(e) => return e,
    };
    let Ok(n) = usize::try_from(count) else {
        return kOfxStatErrBadIndex;
    };
    let map = set.lock();
    let Some(prop) = map.get(name) else {
        if crate::suites::tracing() {
            eprintln!("ofx: unknown property {name:?} ({:?})", set.kind());
        }
        return kOfxStatErrUnknown;
    };
    if n > prop.values.len() {
        return kOfxStatErrBadIndex;
    }
    read(&prop.values, n)
}

unsafe extern "C" fn prop_get_pointer_n(
    h: OfxPropertySetHandle,
    name: *const c_char,
    count: c_int,
    values: *mut *mut c_void,
) -> OfxStatus {
    if values.is_null() && count > 0 {
        return kOfxStatErrValue;
    }
    // SAFETY: the plug-in's arguments; `values` has room for `count`.
    unsafe {
        get_many(h, name, count, |v, n| match v {
            Values::Ptr(p) => {
                for (i, &x) in p.iter().take(n).enumerate() {
                    *values.add(i) = x as *mut c_void;
                }
                kOfxStatOK
            }
            _ => kOfxStatErrValue,
        })
    }
}

unsafe extern "C" fn prop_get_string_n(
    h: OfxPropertySetHandle,
    name: *const c_char,
    count: c_int,
    values: *mut *mut c_char,
) -> OfxStatus {
    if values.is_null() && count > 0 {
        return kOfxStatErrValue;
    }
    // SAFETY: the plug-in's arguments; `values` has room for `count`.
    unsafe {
        get_many(h, name, count, |v, n| match v {
            Values::Str(s) => {
                for (i, x) in s.iter().take(n).enumerate() {
                    *values.add(i) = x.as_ptr() as *mut c_char;
                }
                kOfxStatOK
            }
            _ => kOfxStatErrValue,
        })
    }
}

unsafe extern "C" fn prop_get_double_n(
    h: OfxPropertySetHandle,
    name: *const c_char,
    count: c_int,
    values: *mut c_double,
) -> OfxStatus {
    if values.is_null() && count > 0 {
        return kOfxStatErrValue;
    }
    // SAFETY: the plug-in's arguments; `values` has room for `count`.
    unsafe {
        get_many(h, name, count, |v, n| match v {
            Values::Dbl(d) => {
                for (i, &x) in d.iter().take(n).enumerate() {
                    *values.add(i) = x;
                }
                kOfxStatOK
            }
            Values::Int(d) => {
                for (i, &x) in d.iter().take(n).enumerate() {
                    *values.add(i) = x as f64;
                }
                kOfxStatOK
            }
            _ => kOfxStatErrValue,
        })
    }
}

unsafe extern "C" fn prop_get_int_n(
    h: OfxPropertySetHandle,
    name: *const c_char,
    count: c_int,
    values: *mut c_int,
) -> OfxStatus {
    if values.is_null() && count > 0 {
        return kOfxStatErrValue;
    }
    // SAFETY: the plug-in's arguments; `values` has room for `count`.
    unsafe {
        get_many(h, name, count, |v, n| match v {
            Values::Int(d) => {
                for (i, &x) in d.iter().take(n).enumerate() {
                    *values.add(i) = x;
                }
                kOfxStatOK
            }
            Values::Dbl(d) => {
                for (i, &x) in d.iter().take(n).enumerate() {
                    *values.add(i) = x.round() as c_int;
                }
                kOfxStatOK
            }
            _ => kOfxStatErrValue,
        })
    }
}

unsafe extern "C" fn prop_reset(h: OfxPropertySetHandle, name: *const c_char) -> OfxStatus {
    // SAFETY: the plug-in's arguments.
    let (set, name) = match unsafe { target(h, name) } {
        Ok(t) => t,
        Err(e) => return e,
    };
    match set.lock().get_mut(name) {
        Some(p) => {
            p.values = p.default.clone();
            kOfxStatOK
        }
        None => kOfxStatErrUnknown,
    }
}

unsafe extern "C" fn prop_get_dimension(
    h: OfxPropertySetHandle,
    name: *const c_char,
    count: *mut c_int,
) -> OfxStatus {
    if count.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: the plug-in's arguments.
    let (set, name) = match unsafe { target(h, name) } {
        Ok(t) => t,
        Err(e) => return e,
    };
    match set.lock().get(name) {
        Some(p) => {
            // SAFETY: `count` is where the plug-in wants the answer.
            unsafe { *count = p.values.len() as c_int };
            kOfxStatOK
        }
        None => {
            if crate::suites::tracing() {
                eprintln!("ofx: unknown property {name:?} ({:?})", set.kind());
            }
            kOfxStatErrUnknown
        }
    }
}

/// The property suite plug-ins fetch.
pub static PROPERTY_SUITE: OfxPropertySuiteV1 = OfxPropertySuiteV1 {
    prop_set_pointer,
    prop_set_string,
    prop_set_double,
    prop_set_int,
    prop_set_pointer_n,
    prop_set_string_n,
    prop_set_double_n,
    prop_set_int_n,
    prop_get_pointer,
    prop_get_string,
    prop_get_double,
    prop_get_int,
    prop_get_pointer_n,
    prop_get_string_n,
    prop_get_double_n,
    prop_get_int_n,
    prop_reset,
    prop_get_dimension,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plug_ins_read_and_write_through_the_suite() {
        let set = PropSet::new(SetKind::EffectDescriptor);
        set.define_str(kOfxPropLabel, Some(1), &[c""]);
        set.define_int(kOfxImageEffectPropSupportsTiles, Some(1), &[1]);
        set.define_str(kOfxImageEffectPropSupportedContexts, None, &[]);
        let s = &PROPERTY_SUITE;
        let h = set.handle();
        // SAFETY: valid handles and C strings, as a plug-in would pass.
        unsafe {
            assert_eq!(
                (s.prop_set_string)(h, kOfxPropLabel.as_ptr(), 0, c"Blur".as_ptr()),
                0
            );
            let mut out: *mut c_char = std::ptr::null_mut();
            assert_eq!(
                (s.prop_get_string)(h, kOfxPropLabel.as_ptr(), 0, &mut out),
                0
            );
            assert_eq!(CStr::from_ptr(out), c"Blur");
            // Index 1 of a one-value property.
            assert_eq!(
                (s.prop_set_string)(h, kOfxPropLabel.as_ptr(), 1, c"x".as_ptr()),
                kOfxStatErrBadIndex
            );
            // Variable size: grows.
            let contexts = [
                kOfxImageEffectContextFilter.as_ptr(),
                kOfxImageEffectContextGeneral.as_ptr(),
            ];
            let name = kOfxImageEffectPropSupportedContexts.as_ptr();
            assert_eq!((s.prop_set_string_n)(h, name, 2, contexts.as_ptr()), 0);
            let mut n = 0;
            assert_eq!((s.prop_get_dimension)(h, name, &mut n), 0);
            assert_eq!(n, 2);
            // An int read as a double, and the reverse.
            let mut d = 0.0;
            let tiles = kOfxImageEffectPropSupportsTiles.as_ptr();
            assert_eq!((s.prop_get_double)(h, tiles, 0, &mut d), 0);
            assert_eq!(d, 1.0);
            assert_eq!((s.prop_set_double)(h, tiles, 0, 0.0), 0);
            assert_eq!(set.flag(kOfxImageEffectPropSupportsTiles), Some(false));
            // A string is no number.
            assert_eq!(
                (s.prop_get_double)(h, kOfxPropLabel.as_ptr(), 0, &mut d),
                kOfxStatErrValue
            );
            // Unknown properties, and back to the default.
            assert_eq!(
                (s.prop_get_int)(h, c"NoSuchProp".as_ptr(), 0, &mut 0),
                kOfxStatErrUnknown
            );
            assert_eq!((s.prop_reset)(h, kOfxPropLabel.as_ptr()), 0);
            assert_eq!(set.str(kOfxPropLabel, 0).as_deref(), Some(""));
        }
    }

    #[test]
    fn a_plug_in_may_add_properties_of_its_own() {
        let set = PropSet::new(SetKind::EffectInstance);
        let h = set.handle();
        // SAFETY: a valid handle and C strings.
        unsafe {
            assert_eq!(
                (PROPERTY_SUITE.prop_set_int)(h, c"MyCount".as_ptr(), 2, 7),
                0
            );
        }
        assert_eq!(set.ints(c"MyCount"), vec![0, 0, 7]);
    }

    #[test]
    fn other_handles_are_refused() {
        let not_a_set = [0u64; 4];
        let h = not_a_set.as_ptr() as OfxPropertySetHandle;
        // SAFETY: readable memory that is no set.
        unsafe {
            assert_eq!(
                (PROPERTY_SUITE.prop_set_int)(h, kOfxPropTime.as_ptr(), 0, 1),
                kOfxStatErrBadHandle
            );
            assert_eq!(
                (PROPERTY_SUITE.prop_set_int)(std::ptr::null_mut(), kOfxPropTime.as_ptr(), 0, 1),
                kOfxStatErrBadHandle
            );
        }
    }

    #[test]
    fn fewer_values_set_the_first_ones_of_a_fixed_size() {
        let set = PropSet::new(SetKind::Image);
        set.define_int(kOfxImagePropBounds, Some(4), &[0, 0, 0, 0]);
        let h = set.handle();
        // SAFETY: valid handle, two ints.
        unsafe {
            let two = [5, 6];
            let name = kOfxImagePropBounds.as_ptr();
            assert_eq!((PROPERTY_SUITE.prop_set_int_n)(h, name, 2, two.as_ptr()), 0);
        }
        assert_eq!(set.ints(kOfxImagePropBounds), vec![5, 6, 0, 0]);
    }
}
