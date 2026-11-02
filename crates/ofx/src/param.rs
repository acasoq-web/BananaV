//! Parameters: the sets an effect's parameters live in, each parameter's
//! properties and animation, and the parameter suites (values, keys,
//! parametric curves).
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_double, c_int, c_uint, c_void};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, RwLock};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::props::{PropSet, SetKind, cstring};
use crate::templates;
use crate::value::{Animation, Interp, ParamType, SAME_TIME, Value, curve_at};
use crate::varargs::{Va, as_variadic, variadic};
use bv_core::curve::CurvePoint;

const PARAM_MAGIC: u64 = u64::from_le_bytes(*b"bvOFXprm");
const SET_MAGIC: u64 = u64::from_le_bytes(*b"bvOFXpst");

/// Texts given to a plug-in stay valid while this many newer ones are.
const KEPT_TEXTS: usize = 64;

/// One parameter (of a descriptor or of an instance).
#[repr(C)]
#[derive(Debug)]
pub struct Param {
    magic: u64,
    pub props: PropSet,
    pub name: String,
    pub ty: ParamType,
    state: Mutex<ParamState>,
    /// The set it belongs to (set once, when the set adds it).
    set: OnceLock<usize>,
    /// A parametric parameter's default: its descriptor's curves (an
    /// instance's own change).
    curves_default: OnceLock<Value>,
}

#[derive(Debug)]
struct ParamState {
    anim: Animation,
    /// Texts and bytes handed to the plug-in.
    texts: Vec<CString>,
    bytes: Vec<Vec<u8>>,
    /// Changed by the plug-in since the host last asked.
    changed: bool,
}

impl ParamState {
    /// A C string of `s` that stays valid a while (one already handed out
    /// when it is the same text).
    fn keep_text(&mut self, s: &str) -> *const c_char {
        let c = cstring(s);
        if let Some(k) = self.texts.iter().find(|k| **k == c) {
            return k.as_ptr();
        }
        if self.texts.len() >= KEPT_TEXTS {
            self.texts.remove(0);
        }
        self.texts.push(c);
        self.texts.last().map_or(std::ptr::null(), |k| k.as_ptr())
    }
}

impl Param {
    fn new(kind: SetKind, ty: ParamType, name: &CStr) -> Param {
        let props = templates::param_descriptor(ty, name);
        let props = if kind == SetKind::ParamInstance {
            let p = PropSet::new(SetKind::ParamInstance);
            p.copy_from(&props);
            p
        } else {
            props
        };
        Param {
            magic: PARAM_MAGIC,
            props,
            name: name.to_string_lossy().into_owned(),
            ty,
            state: Mutex::new(ParamState {
                anim: Animation::constant(Value::zero(ty)),
                texts: Vec::new(),
                bytes: Vec::new(),
                changed: false,
            }),
            set: OnceLock::new(),
            curves_default: OnceLock::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, ParamState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn handle(&self) -> OfxParamHandle {
        self as *const Param as OfxParamHandle
    }

    /// The parameter behind a plug-in's handle.
    ///
    /// # Safety
    /// `h` is null or readable, and a parameter it points to lives on.
    pub unsafe fn from_handle<'a>(h: OfxParamHandle) -> Option<&'a Param> {
        if h.is_null() {
            return None;
        }
        let p = h as *const Param;
        // SAFETY: as the caller promises; the magic number tells.
        unsafe { ((*p).magic == PARAM_MAGIC).then(|| &*p) }
    }

    fn param_set(&self) -> Option<&ParamSet> {
        let p = *self.set.get()? as *const ParamSet;
        // SAFETY: a set outlives its parameters.
        Some(unsafe { &*p })
    }

    /// The value its plug-in gave as default.
    pub fn default_value(&self) -> Value {
        match self.ty {
            t if t.is_text() => {
                Value::Text(self.props.str(kOfxParamPropDefault, 0).unwrap_or_default())
            }
            ParamType::Bytes => {
                let p = self.props.ptr(kOfxParamPropDefault, 0);
                // SAFETY: a plug-in's default bytes are an `OfxBytes`.
                Value::Bytes(
                    p.map(|p| unsafe { read_bytes(p as *const OfxBytes) })
                        .unwrap_or_default(),
                )
            }
            ParamType::Parametric => self
                .curves_default
                .get()
                .cloned()
                .unwrap_or_else(|| self.lock().anim.value.clone()),
            t if t.has_value() => Value::Num(self.props.dbls(kOfxParamPropDefault)),
            _ => Value::None,
        }
    }

    /// Its animation (or constant value).
    pub fn animation(&self) -> Animation {
        self.lock().anim.clone()
    }

    /// Replaces its animation (the host's doing: no change is reported).
    pub fn set_animation(&self, anim: Animation) {
        let animating = anim.is_animated();
        self.lock().anim = anim;
        self.props
            .set_int(kOfxParamPropIsAnimating, &[animating as i32]);
    }

    /// Its value at `time` (frames).
    pub fn value_at(&self, time: f64) -> Value {
        let anim = self.lock().anim.clone();
        if self.ty == ParamType::Custom
            && let Some(v) = self.custom_interpolation(&anim, time)
        {
            return v;
        }
        anim.at(time, self.ty)
    }

    /// A custom parameter's value between two keys, from its plug-in's
    /// interpolation function, when it has one.
    fn custom_interpolation(&self, anim: &Animation, time: f64) -> Option<Value> {
        let f = self.props.ptr(kOfxParamPropCustomInterpCallbackV1, 0)?;
        if f.is_null() {
            return None;
        }
        let i = anim.keys.partition_point(|k| k.time <= time);
        if i == 0 || i >= anim.keys.len() {
            return None;
        }
        let (a, b) = (&anim.keys[i - 1], &anim.keys[i]);
        let set = self.param_set()?;
        let args = PropSet::new(SetKind::Arguments);
        args.set_dbl(kOfxPropTime, &[time]);
        args.set_str(kOfxParamPropCustomValue, &[a.value.text(), b.value.text()]);
        args.set_dbl(kOfxParamPropInterpolationTime, &[a.time, b.time]);
        let amount = (time - a.time) / (b.time - a.time).max(SAME_TIME);
        args.set_dbl(kOfxParamPropInterpolationAmount, &[amount]);
        let out = PropSet::new(SetKind::Arguments);
        out.define_str(kOfxParamPropCustomValue, None, &[c""]);
        // SAFETY: the plug-in gave this function for this purpose.
        let status = unsafe {
            let f: OfxCustomParamInterpFuncV1 = std::mem::transmute(f);
            f(set.handle(), args.handle(), out.handle())
        };
        (status == kOfxStatOK)
            .then(|| Value::Text(out.str(kOfxParamPropCustomValue, 0).unwrap_or_default()))
    }

    /// Whether its plug-in changed it since the last call (and forgets).
    pub fn take_changed(&self) -> bool {
        std::mem::take(&mut self.lock().changed)
    }

    pub fn label(&self) -> String {
        self.props
            .str(kOfxPropLabel, 0)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.name.clone())
    }

    fn sync_animating(&self) {
        let animating = self.lock().anim.is_animated();
        self.props
            .set_int(kOfxParamPropIsAnimating, &[animating as i32]);
    }
}

/// An effect's parameters.
#[repr(C)]
#[derive(Debug)]
pub struct ParamSet {
    magic: u64,
    pub props: PropSet,
    kind: SetKind,
    // Boxes keep each parameter where it is while the list grows:
    // plug-ins hold their addresses.
    #[allow(clippy::vec_box)]
    params: RwLock<Vec<Box<Param>>>,
    /// The time `paramGetValue` and `paramSetValue` mean (frames).
    time: AtomicU64,
    /// Whether its plug-in may change values now (not while rendering).
    writable: AtomicBool,
}

impl ParamSet {
    /// A descriptor's set (`SetKind::ParamDescriptor`) or an instance's
    /// (`SetKind::ParamInstance`). Its parameters keep its address: it
    /// must not move once it has some.
    pub fn new(kind: SetKind) -> ParamSet {
        ParamSet {
            magic: SET_MAGIC,
            props: PropSet::new(SetKind::ParamSet),
            kind,
            params: RwLock::new(Vec::new()),
            time: AtomicU64::new(0f64.to_bits()),
            writable: AtomicBool::new(true),
        }
    }

    pub fn handle(&self) -> OfxParamSetHandle {
        self as *const ParamSet as OfxParamSetHandle
    }

    /// # Safety
    /// `h` is null or readable, and a set it points to lives on.
    pub unsafe fn from_handle<'a>(h: OfxParamSetHandle) -> Option<&'a ParamSet> {
        if h.is_null() {
            return None;
        }
        let p = h as *const ParamSet;
        // SAFETY: as the caller promises; the magic number tells.
        unsafe { ((*p).magic == SET_MAGIC).then(|| &*p) }
    }

    /// Adds a parameter; `None` when the name is taken.
    pub fn define(&self, ty: ParamType, name: &CStr) -> Option<&Param> {
        let mut list = self.params.write().unwrap_or_else(|e| e.into_inner());
        if list.iter().any(|p| p.name.as_bytes() == name.to_bytes()) {
            return None;
        }
        let param = Box::new(Param::new(self.kind, ty, name));
        let _ = param.set.set(self as *const ParamSet as usize);
        let ptr: *const Param = &*param;
        list.push(param);
        // SAFETY: the box never moves nor goes while the set lives.
        Some(unsafe { &*ptr })
    }

    /// The parameter named `name`.
    pub fn get(&self, name: &str) -> Option<&Param> {
        let list = self.params.read().unwrap_or_else(|e| e.into_inner());
        let ptr: *const Param = &**list.iter().find(|p| p.name == name)?;
        // SAFETY: the box never moves nor goes while the set lives.
        Some(unsafe { &*ptr })
    }

    /// Every parameter, in the order its plug-in defined them.
    pub fn all(&self) -> Vec<&Param> {
        let list = self.params.read().unwrap_or_else(|e| e.into_inner());
        list.iter()
            .map(|p| {
                let ptr: *const Param = &**p;
                // SAFETY: as in `get`.
                unsafe { &*ptr }
            })
            .collect()
    }

    /// Fills an instance's set (`self`, in its final place: its
    /// parameters keep its address) with the descriptor's parameters and
    /// their properties, at their defaults.
    pub fn instantiate(&self, descriptor: &ParamSet) {
        self.props.copy_from(&descriptor.props);
        for d in descriptor.all() {
            let name = cstring(&d.name);
            if let Some(p) = self.define(d.ty, &name) {
                p.props.copy_from(&d.props);
                // Parametric defaults are curves the descriptor holds.
                let value = if d.ty == ParamType::Parametric {
                    let v = d.lock().anim.value.clone();
                    let _ = p.curves_default.set(v.clone());
                    v
                } else {
                    d.default_value()
                };
                p.set_animation(Animation::constant(value));
            }
        }
    }

    /// Gives spatial parameters whose defaults are normalised (0.5: the
    /// middle) their defaults in canonical coordinates of a picture of
    /// `size`, as a host must when it creates an instance.
    pub fn denormalise_defaults(&self, (w, h): (f64, f64)) {
        let normalised = kOfxParamCoordinatesNormalised.to_string_lossy();
        for p in self.all() {
            if !matches!(p.ty, ParamType::Double | ParamType::Double2D)
                || p.props
                    .str(kOfxParamPropDefaultCoordinateSystem, 0)
                    .as_deref()
                    != Some(&*normalised)
            {
                continue;
            }
            let kind = p.props.str(kOfxParamPropDoubleType, 0).unwrap_or_default();
            let is = |n: &CStr| kind.as_bytes() == n.to_bytes();
            let scale: &[f64] = if is(kOfxParamDoubleTypeX) || is(kOfxParamDoubleTypeXAbsolute) {
                &[w]
            } else if is(kOfxParamDoubleTypeY) || is(kOfxParamDoubleTypeYAbsolute) {
                &[h]
            } else if is(kOfxParamDoubleTypeXY) || is(kOfxParamDoubleTypeXYAbsolute) {
                &[w, h]
            } else {
                continue;
            };
            let canonical: Vec<f64> = p
                .props
                .dbls(kOfxParamPropDefault)
                .iter()
                .zip(scale)
                .map(|(v, k)| v * k)
                .collect();
            p.props.set_dbl(kOfxParamPropDefault, &canonical);
            p.set_animation(Animation::constant(Value::Num(canonical)));
        }
    }

    pub fn set_time(&self, time: f64) {
        self.time.store(time.to_bits(), Ordering::Relaxed);
    }

    pub fn time(&self) -> f64 {
        f64::from_bits(self.time.load(Ordering::Relaxed))
    }

    pub fn set_writable(&self, writable: bool) {
        self.writable.store(writable, Ordering::Relaxed);
    }

    /// The parameters its plug-in changed since the last call.
    pub fn take_changed(&self) -> Vec<String> {
        self.all()
            .into_iter()
            .filter(|p| p.take_changed())
            .map(|p| p.name.clone())
            .collect()
    }
}

/// A plug-in's `OfxBytes`, copied.
///
/// # Safety
/// `p` is null or an `OfxBytes` whose data can be read.
unsafe fn read_bytes(p: *const OfxBytes) -> Vec<u8> {
    if p.is_null() {
        return Vec::new();
    }
    // SAFETY: as the caller promises.
    unsafe {
        let b = &*p;
        if b.data.is_null() || b.length == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(b.data, b.length).to_vec()
        }
    }
}

// The parameter suite.

unsafe extern "C" fn param_define(
    set: OfxParamSetHandle,
    param_type: *const c_char,
    name: *const c_char,
    props: *mut OfxPropertySetHandle,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(set) = (unsafe { ParamSet::from_handle(set) }) else {
        return kOfxStatErrBadHandle;
    };
    if param_type.is_null() || name.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: C strings from the plug-in.
    let (ty, name) = unsafe { (CStr::from_ptr(param_type), CStr::from_ptr(name)) };
    let Some(kind) = ParamType::from_ofx_name(ty) else {
        if crate::suites::tracing() {
            eprintln!("ofx: paramDefine {name:?} of unknown type {ty:?}");
        }
        return kOfxStatErrUnknown;
    };
    match set.define(kind, name) {
        Some(p) => {
            if !props.is_null() {
                // SAFETY: where the plug-in wants the handle.
                unsafe { *props = p.props.handle() };
            }
            kOfxStatOK
        }
        None => {
            if crate::suites::tracing() {
                eprintln!("ofx: paramDefine {name:?}: already defined");
            }
            kOfxStatErrExists
        }
    }
}

unsafe extern "C" fn param_get_handle(
    set: OfxParamSetHandle,
    name: *const c_char,
    param: *mut OfxParamHandle,
    props: *mut OfxPropertySetHandle,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(set) = (unsafe { ParamSet::from_handle(set) }) else {
        return kOfxStatErrBadHandle;
    };
    if name.is_null() {
        return kOfxStatErrUnknown;
    }
    // SAFETY: a C string from the plug-in.
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    let Some(p) = set.get(&name) else {
        if crate::suites::tracing() {
            eprintln!("ofx: paramGetHandle {name:?}: no such parameter");
        }
        return kOfxStatErrUnknown;
    };
    // SAFETY: where the plug-in wants the answers.
    unsafe {
        if !param.is_null() {
            *param = p.handle();
        }
        if !props.is_null() {
            *props = p.props.handle();
        }
    }
    kOfxStatOK
}

unsafe extern "C" fn param_set_get_property_set(
    set: OfxParamSetHandle,
    props: *mut OfxPropertySetHandle,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(set) = (unsafe { ParamSet::from_handle(set) }) else {
        return kOfxStatErrBadHandle;
    };
    if props.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: where the plug-in wants the handle.
    unsafe { *props = set.props.handle() };
    kOfxStatOK
}

unsafe extern "C" fn param_get_property_set(
    param: OfxParamHandle,
    props: *mut OfxPropertySetHandle,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(param) }) else {
        return kOfxStatErrBadHandle;
    };
    if props.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: where the plug-in wants the handle.
    unsafe { *props = p.props.handle() };
    kOfxStatOK
}

/// Writes `v` where the variadic pointers say, as `p`'s type wants.
///
/// # Safety
/// `va` holds the plug-in's pointers, of the types the parameter's type
/// calls for.
unsafe fn write_value(p: &Param, v: &Value, va: &mut Va) -> OfxStatus {
    let ty = p.ty;
    match ty {
        t if t.is_text() => {
            let out = va.ptr::<*const c_char>();
            if out.is_null() {
                return kOfxStatErrValue;
            }
            let ptr = p.lock().keep_text(v.text());
            // SAFETY: where the plug-in wants the text.
            unsafe { *out = ptr };
        }
        ParamType::Bytes => {
            let out = va.ptr::<OfxBytes>();
            if out.is_null() {
                return kOfxStatErrValue;
            }
            let mut st = p.lock();
            let bytes = match v {
                Value::Bytes(b) => b.clone(),
                _ => Vec::new(),
            };
            if st.bytes.len() >= KEPT_TEXTS {
                st.bytes.remove(0);
            }
            st.bytes.push(bytes);
            let b = st.bytes.last().map_or(&[][..], |b| &b[..]);
            // SAFETY: where the plug-in wants the bytes.
            unsafe {
                *out = OfxBytes {
                    data: b.as_ptr(),
                    length: b.len(),
                }
            };
        }
        t if t.is_int() => {
            for i in 0..t.dimension() {
                let out = va.ptr::<c_int>();
                if out.is_null() {
                    return kOfxStatErrValue;
                }
                // SAFETY: where the plug-in wants the number.
                unsafe { *out = v.num(i).round() as c_int };
            }
        }
        t if t.dimension() > 0 => {
            for i in 0..t.dimension() {
                let out = va.ptr::<c_double>();
                if out.is_null() {
                    return kOfxStatErrValue;
                }
                // SAFETY: where the plug-in wants the number.
                unsafe { *out = v.num(i) };
            }
        }
        _ => return kOfxStatErrUnsupported,
    }
    kOfxStatOK
}

/// The value the variadic arguments carry, as `ty` passes it.
///
/// # Safety
/// `va` holds values of the types the parameter's type calls for.
unsafe fn read_value(ty: ParamType, va: &mut Va) -> Option<Value> {
    Some(match ty {
        t if t.is_text() => {
            // SAFETY: a C string from the plug-in, or null.
            Value::Text(unsafe { crate::props::text(va.ptr::<c_char>()) })
        }
        // SAFETY: an `OfxBytes` from the plug-in, or null.
        ParamType::Bytes => Value::Bytes(unsafe { read_bytes(va.ptr::<OfxBytes>()) }),
        t if t.is_int() => Value::Num((0..t.dimension()).map(|_| va.int() as f64).collect()),
        t if t.dimension() > 0 => Value::Num((0..t.dimension()).map(|_| va.double()).collect()),
        _ => return None,
    })
}

unsafe fn get_value(h: *mut c_void, mut va: Va) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h as OfxParamHandle) }) else {
        return kOfxStatErrBadHandle;
    };
    let time = p.param_set().map_or(0.0, ParamSet::time);
    let v = p.value_at(time);
    // SAFETY: the plug-in's pointers.
    unsafe { write_value(p, &v, &mut va) }
}

unsafe fn get_value_at_time(h: *mut c_void, time: f64, mut va: Va) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h as OfxParamHandle) }) else {
        return kOfxStatErrBadHandle;
    };
    let v = p.value_at(time);
    // SAFETY: the plug-in's pointers.
    unsafe { write_value(p, &v, &mut va) }
}

unsafe fn get_derivative(h: *mut c_void, time: f64, mut va: Va) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h as OfxParamHandle) }) else {
        return kOfxStatErrBadHandle;
    };
    if p.ty.is_int() || p.ty.dimension() == 0 {
        return kOfxStatErrUnsupported;
    }
    let d = Value::Num(p.animation().derivative(time, p.ty));
    // SAFETY: the plug-in's pointers.
    unsafe { write_value(p, &d, &mut va) }
}

unsafe fn get_integral(h: *mut c_void, t1: f64, t2: f64, mut va: Va) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h as OfxParamHandle) }) else {
        return kOfxStatErrBadHandle;
    };
    if p.ty.is_int() || p.ty.dimension() == 0 {
        return kOfxStatErrUnsupported;
    }
    let i = Value::Num(p.animation().integral(t1, t2, p.ty));
    // SAFETY: the plug-in's pointers.
    unsafe { write_value(p, &i, &mut va) }
}

/// Stores a value the plug-in sets: at `time` as a key, or as the value
/// (a key at the set's time when the parameter already has keys).
fn store(p: &Param, time: Option<f64>, value: Value) -> OfxStatus {
    if let Some(set) = p.param_set()
        && !set.writable.load(Ordering::Relaxed)
    {
        return kOfxStatFailed;
    }
    {
        let mut st = p.lock();
        let at = time.or_else(|| {
            st.anim
                .is_animated()
                .then(|| p.param_set().map_or(0.0, ParamSet::time))
        });
        match at {
            Some(t) => st.anim.set_key(t, value, Interp::default()),
            None => st.anim.value = value,
        }
        st.changed = true;
    }
    p.sync_animating();
    kOfxStatOK
}

unsafe fn set_value(h: *mut c_void, mut va: Va) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h as OfxParamHandle) }) else {
        return kOfxStatErrBadHandle;
    };
    // SAFETY: the plug-in's values.
    match unsafe { read_value(p.ty, &mut va) } {
        Some(v) => store(p, None, v),
        None => kOfxStatErrUnsupported,
    }
}

unsafe fn set_value_at_time(h: *mut c_void, time: f64, mut va: Va) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h as OfxParamHandle) }) else {
        return kOfxStatErrBadHandle;
    };
    // SAFETY: the plug-in's values.
    match unsafe { read_value(p.ty, &mut va) } {
        Some(v) => store(p, Some(time), v),
        None => kOfxStatErrUnsupported,
    }
}

variadic!(fn param_get_value(h) => get_value);
variadic!(fn param_get_value_at_time(h; t) => get_value_at_time);
variadic!(fn param_get_derivative(h; t) => get_derivative);
variadic!(fn param_get_integral(h; t1, t2) => get_integral);
variadic!(fn param_set_value(h) => set_value);
variadic!(fn param_set_value_at_time(h; t) => set_value_at_time);

unsafe extern "C" fn param_get_num_keys(h: OfxParamHandle, n: *mut c_uint) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h) }) else {
        return kOfxStatErrBadHandle;
    };
    if n.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: where the plug-in wants the count.
    unsafe { *n = p.lock().anim.keys.len() as c_uint };
    kOfxStatOK
}

unsafe extern "C" fn param_get_key_time(
    h: OfxParamHandle,
    nth: c_uint,
    time: *mut OfxTime,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h) }) else {
        return kOfxStatErrBadHandle;
    };
    match p.lock().anim.keys.get(nth as usize) {
        Some(k) if !time.is_null() => {
            // SAFETY: where the plug-in wants the time.
            unsafe { *time = k.time };
            kOfxStatOK
        }
        Some(_) => kOfxStatErrValue,
        None => kOfxStatErrBadIndex,
    }
}

unsafe extern "C" fn param_get_key_index(
    h: OfxParamHandle,
    time: OfxTime,
    direction: c_int,
    index: *mut c_int,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h) }) else {
        return kOfxStatErrBadHandle;
    };
    let st = p.lock();
    let keys = &st.anim.keys;
    let found = match direction {
        0 => keys.iter().position(|k| (k.time - time).abs() < SAME_TIME),
        d if d < 0 => keys.iter().rposition(|k| k.time < time - SAME_TIME),
        _ => keys.iter().position(|k| k.time > time + SAME_TIME),
    };
    match found {
        Some(i) if !index.is_null() => {
            // SAFETY: where the plug-in wants the index.
            unsafe { *index = i as c_int };
            kOfxStatOK
        }
        Some(_) => kOfxStatErrValue,
        None => kOfxStatFailed,
    }
}

unsafe extern "C" fn param_delete_key(h: OfxParamHandle, time: OfxTime) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h) }) else {
        return kOfxStatErrBadHandle;
    };
    let deleted = {
        let mut st = p.lock();
        let deleted = st.anim.delete_key(time);
        st.changed |= deleted;
        deleted
    };
    p.sync_animating();
    if deleted {
        kOfxStatOK
    } else {
        kOfxStatErrBadIndex
    }
}

unsafe extern "C" fn param_delete_all_keys(h: OfxParamHandle) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h) }) else {
        return kOfxStatErrBadHandle;
    };
    {
        let mut st = p.lock();
        // The value stays what it was at the set's time.
        let now = p.param_set().map_or(0.0, ParamSet::time);
        if st.anim.is_animated() {
            st.anim.value = st.anim.at(now, p.ty);
            st.anim.keys.clear();
            st.changed = true;
        }
    }
    p.sync_animating();
    kOfxStatOK
}

unsafe extern "C" fn param_copy(
    to: OfxParamHandle,
    from: OfxParamHandle,
    offset: OfxTime,
    range: *const OfxRangeD,
) -> OfxStatus {
    // SAFETY: the plug-in's handles and range.
    let (Some(to), Some(from)) = (unsafe { Param::from_handle(to) }, unsafe {
        Param::from_handle(from)
    }) else {
        return kOfxStatErrBadHandle;
    };
    if to.ty != from.ty {
        return kOfxStatErrValue;
    }
    // SAFETY: a range from the plug-in, or null.
    let range = (!range.is_null()).then(|| unsafe { *range });
    let mut anim = from.animation();
    if let Some(r) = range {
        anim.keys.retain(|k| k.time >= r.min && k.time <= r.max);
    }
    for k in &mut anim.keys {
        k.time += offset;
    }
    {
        let mut st = to.lock();
        st.anim = anim;
        st.changed = true;
    }
    to.sync_animating();
    kOfxStatOK
}

unsafe extern "C" fn param_edit_begin(set: OfxParamSetHandle, _name: *const c_char) -> OfxStatus {
    // Undo grouping: the host's own undo records the whole change.
    // SAFETY: the plug-in's handle.
    match unsafe { ParamSet::from_handle(set) } {
        Some(_) => kOfxStatOK,
        None => kOfxStatErrBadHandle,
    }
}

unsafe extern "C" fn param_edit_end(set: OfxParamSetHandle) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    match unsafe { ParamSet::from_handle(set) } {
        Some(_) => kOfxStatOK,
        None => kOfxStatErrBadHandle,
    }
}

/// The parameter suite plug-ins fetch.
pub fn parameter_suite() -> &'static OfxParameterSuiteV1 {
    static SUITE: OnceLock<OfxParameterSuiteV1> = OnceLock::new();
    SUITE.get_or_init(|| {
        // SAFETY: each function receives its variadic call as its table
        // entry's type passes it (see `varargs`).
        unsafe {
            OfxParameterSuiteV1 {
                param_define,
                param_get_handle,
                param_set_get_property_set,
                param_get_property_set,
                param_get_value: as_variadic(param_get_value as *const ()),
                param_get_value_at_time: as_variadic(param_get_value_at_time as *const ()),
                param_get_derivative: as_variadic(param_get_derivative as *const ()),
                param_get_integral: as_variadic(param_get_integral as *const ()),
                param_set_value: as_variadic(param_set_value as *const ()),
                param_set_value_at_time: as_variadic(param_set_value_at_time as *const ()),
                param_get_num_keys,
                param_get_key_time,
                param_get_key_index,
                param_delete_key,
                param_delete_all_keys,
                param_copy,
                param_edit_begin,
                param_edit_end,
            }
        }
    })
}

// The parametric parameter suite: curves of (position, value) points.

/// How many curves a parametric parameter has.
fn curve_count(p: &Param) -> usize {
    p.props
        .int(kOfxParamPropParametricDimension, 0)
        .unwrap_or(1)
        .max(1) as usize
}

/// A parametric parameter's curves at `time` (frames), as many as it has.
fn curves_at(p: &Param, st: &ParamState, time: f64) -> Vec<Vec<CurvePoint>> {
    let mut curves = match st.anim.at(time, ParamType::Parametric) {
        Value::Curves(c) => c,
        _ => Vec::new(),
    };
    curves.resize(curve_count(p), Vec::new());
    curves
}

/// Curve `curve`'s points at `time`.
fn read_curve(p: &Param, curve: c_int, time: f64) -> Option<Vec<CurvePoint>> {
    if p.ty != ParamType::Parametric {
        return None;
    }
    let i = usize::try_from(curve)
        .ok()
        .filter(|&i| i < curve_count(p))?;
    let st = p.lock();
    curves_at(p, &st, time).into_iter().nth(i)
}

/// Changes curve `curve` at `time`: its value, or its key there when the
/// parameter is animated.
fn edit_curve<R>(
    p: &Param,
    curve: c_int,
    time: f64,
    f: impl FnOnce(&mut Vec<CurvePoint>) -> R,
) -> Option<R> {
    if p.ty != ParamType::Parametric {
        return None;
    }
    let i = usize::try_from(curve)
        .ok()
        .filter(|&i| i < curve_count(p))?;
    let mut st = p.lock();
    let mut curves = curves_at(p, &st, time);
    let r = f(&mut curves[i]);
    if st.anim.keys.is_empty() {
        st.anim.value = Value::Curves(curves);
    } else {
        st.anim
            .set_key(time, Value::Curves(curves), Interp::default());
    }
    st.changed = true;
    Some(r)
}

unsafe extern "C" fn parametric_get_value(
    h: OfxParamHandle,
    curve: c_int,
    time: OfxTime,
    position: c_double,
    out: *mut c_double,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h) }) else {
        return kOfxStatErrBadHandle;
    };
    match read_curve(p, curve, time) {
        Some(c) if !out.is_null() => {
            // SAFETY: where the plug-in wants the value.
            unsafe { *out = curve_at(&c, position) };
            kOfxStatOK
        }
        Some(_) => kOfxStatErrValue,
        None => kOfxStatErrBadIndex,
    }
}

unsafe extern "C" fn parametric_get_count(
    h: OfxParamHandle,
    curve: c_int,
    time: c_double,
    out: *mut c_int,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h) }) else {
        return kOfxStatErrBadHandle;
    };
    match read_curve(p, curve, time) {
        Some(c) if !out.is_null() => {
            // SAFETY: where the plug-in wants the count.
            unsafe { *out = c.len() as c_int };
            kOfxStatOK
        }
        Some(_) => kOfxStatErrValue,
        None => kOfxStatErrBadIndex,
    }
}

unsafe extern "C" fn parametric_get_nth(
    h: OfxParamHandle,
    curve: c_int,
    time: c_double,
    nth: c_int,
    key: *mut c_double,
    value: *mut c_double,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h) }) else {
        return kOfxStatErrBadHandle;
    };
    let point = read_curve(p, curve, time)
        .and_then(|c| usize::try_from(nth).ok().and_then(|i| c.get(i).copied()));
    match point {
        Some(q) => {
            // SAFETY: where the plug-in wants the point.
            unsafe {
                if !key.is_null() {
                    *key = q.x;
                }
                if !value.is_null() {
                    *value = q.y;
                }
            }
            kOfxStatOK
        }
        _ => kOfxStatErrBadIndex,
    }
}

unsafe extern "C" fn parametric_set_nth(
    h: OfxParamHandle,
    curve: c_int,
    time: c_double,
    nth: c_int,
    key: c_double,
    value: c_double,
    _add_key: bool,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h) }) else {
        return kOfxStatErrBadHandle;
    };
    // The point moves with its handles.
    let done = edit_curve(p, curve, time, |c| {
        let i = usize::try_from(nth).ok().filter(|&i| i < c.len())?;
        c[i].x = key;
        c[i].y = value;
        c.sort_by(|a, b| a.x.total_cmp(&b.x));
        Some(())
    });
    match done {
        Some(Some(())) => kOfxStatOK,
        _ => kOfxStatErrBadIndex,
    }
}

unsafe extern "C" fn parametric_add(
    h: OfxParamHandle,
    curve: c_int,
    time: c_double,
    key: c_double,
    value: c_double,
    _add_key: bool,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h) }) else {
        return kOfxStatErrBadHandle;
    };
    let done = edit_curve(p, curve, time, |c| {
        match c.iter_mut().find(|q| (q.x - key).abs() < 1e-12) {
            Some(q) => q.y = value,
            None => {
                let i = c.partition_point(|q| q.x < key);
                c.insert(i, CurvePoint::new(key, value));
            }
        }
    });
    match done {
        Some(()) => kOfxStatOK,
        None => kOfxStatErrBadIndex,
    }
}

unsafe extern "C" fn parametric_delete(h: OfxParamHandle, curve: c_int, nth: c_int) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h) }) else {
        return kOfxStatErrBadHandle;
    };
    let time = p.param_set().map_or(0.0, ParamSet::time);
    let done = edit_curve(p, curve, time, |c| {
        let i = usize::try_from(nth).ok().filter(|&i| i < c.len())?;
        c.remove(i);
        Some(())
    });
    match done {
        Some(Some(())) => kOfxStatOK,
        _ => kOfxStatErrBadIndex,
    }
}

unsafe extern "C" fn parametric_delete_all(h: OfxParamHandle, curve: c_int) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(p) = (unsafe { Param::from_handle(h) }) else {
        return kOfxStatErrBadHandle;
    };
    let time = p.param_set().map_or(0.0, ParamSet::time);
    match edit_curve(p, curve, time, Vec::clear) {
        Some(()) => kOfxStatOK,
        None => kOfxStatErrBadIndex,
    }
}

/// The parametric parameter suite plug-ins fetch.
pub static PARAMETRIC_SUITE: OfxParametricParameterSuiteV1 = OfxParametricParameterSuiteV1 {
    parametric_param_get_value: parametric_get_value,
    parametric_param_get_n_control_points: parametric_get_count,
    parametric_param_get_nth_control_point: parametric_get_nth,
    parametric_param_set_nth_control_point: parametric_set_nth,
    parametric_param_add_control_point: parametric_add,
    parametric_param_delete_control_point: parametric_delete,
    parametric_param_delete_all_control_points: parametric_delete_all,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn suite() -> &'static OfxParameterSuiteV1 {
        parameter_suite()
    }

    #[test]
    fn plug_ins_define_parameters_and_read_their_values() {
        let descriptor = ParamSet::new(SetKind::ParamDescriptor);
        let s = suite();
        let mut props: OfxPropertySetHandle = std::ptr::null_mut();
        // SAFETY: handles and C strings as a plug-in passes them.
        unsafe {
            let set = descriptor.handle();
            assert_eq!(
                (s.param_define)(
                    set,
                    kOfxParamTypeDouble.as_ptr(),
                    c"amount".as_ptr(),
                    &mut props
                ),
                kOfxStatOK
            );
            let p = PropSet::from_handle(props).unwrap();
            p.set_dbl(kOfxParamPropDefault, &[0.5]);
            assert_eq!(
                (s.param_define)(
                    set,
                    kOfxParamTypeRGBA.as_ptr(),
                    c"tint".as_ptr(),
                    &mut props
                ),
                kOfxStatOK
            );
            PropSet::from_handle(props)
                .unwrap()
                .set_dbl(kOfxParamPropDefault, &[1.0, 0.5, 0.25, 1.0]);
            assert_eq!(
                (s.param_define)(
                    set,
                    kOfxParamTypeString.as_ptr(),
                    c"note".as_ptr(),
                    &mut props
                ),
                kOfxStatOK
            );
            assert_eq!(
                (s.param_define)(
                    set,
                    kOfxParamTypeInteger2D.as_ptr(),
                    c"cells".as_ptr(),
                    &mut props
                ),
                kOfxStatOK
            );
            assert_eq!(
                (s.param_define)(
                    set,
                    kOfxParamTypeDouble.as_ptr(),
                    c"amount".as_ptr(),
                    &mut props
                ),
                kOfxStatErrExists
            );
        }
        let instance = ParamSet::new(SetKind::ParamInstance);
        instance.instantiate(&descriptor);
        // SAFETY: as above; the variadic calls pass what each type wants.
        unsafe {
            let set = instance.handle();
            let mut h: OfxParamHandle = std::ptr::null_mut();
            let null = std::ptr::null_mut();
            assert_eq!(
                (s.param_get_handle)(set, c"amount".as_ptr(), &mut h, null),
                kOfxStatOK
            );
            let mut v = 0.0f64;
            assert_eq!((s.param_get_value)(h, &mut v as *mut f64), kOfxStatOK);
            assert_eq!(v, 0.5);
            // A key, then a value between keys.
            assert_eq!((s.param_set_value_at_time)(h, 0.0, 0.0f64), kOfxStatOK);
            assert_eq!((s.param_set_value_at_time)(h, 10.0, 1.0f64), kOfxStatOK);
            let mut n = 0u32;
            (s.param_get_num_keys)(h, &mut n);
            assert_eq!(n, 2);
            assert_eq!(
                (s.param_get_value_at_time)(h, 5.0, &mut v as *mut f64),
                kOfxStatOK
            );
            assert!((v - 0.5).abs() < 1e-9, "{v}");
            let mut idx = -1;
            assert_eq!((s.param_get_key_index)(h, 5.0, 1, &mut idx), kOfxStatOK);
            assert_eq!(idx, 1);

            assert_eq!(
                (s.param_get_handle)(set, c"tint".as_ptr(), &mut h, null),
                kOfxStatOK
            );
            let mut c = [0.0f64; 4];
            let [r, g, b, a] = c.each_mut().map(|x| x as *mut f64);
            assert_eq!((s.param_get_value_at_time)(h, 0.0, r, g, b, a), kOfxStatOK);
            assert_eq!(c, [1.0, 0.5, 0.25, 1.0]);
            assert_eq!(
                (s.param_set_value)(h, 0.1f64, 0.2f64, 0.3f64, 0.4f64),
                kOfxStatOK
            );
            (s.param_get_value)(h, r, g, b, a);
            assert_eq!(c, [0.1, 0.2, 0.3, 0.4]);

            assert_eq!(
                (s.param_get_handle)(set, c"note".as_ptr(), &mut h, null),
                kOfxStatOK
            );
            assert_eq!((s.param_set_value)(h, c"hello".as_ptr()), kOfxStatOK);
            let mut text: *const c_char = std::ptr::null();
            (s.param_get_value)(h, &mut text as *mut *const c_char);
            assert_eq!(CStr::from_ptr(text), c"hello");

            assert_eq!(
                (s.param_get_handle)(set, c"cells".as_ptr(), &mut h, null),
                kOfxStatOK
            );
            assert_eq!((s.param_set_value)(h, 3i32, -4i32), kOfxStatOK);
            let (mut x, mut y) = (0i32, 0i32);
            (s.param_get_value)(h, &mut x as *mut i32, &mut y as *mut i32);
            assert_eq!((x, y), (3, -4));
        }
        let mut changed = instance.take_changed();
        changed.sort();
        assert_eq!(changed, ["amount", "cells", "note", "tint"]);
        assert!(instance.take_changed().is_empty());
    }

    #[test]
    fn values_cannot_change_while_rendering() {
        let set = ParamSet::new(SetKind::ParamInstance);
        let p = set.define(ParamType::Double, c"x").unwrap();
        set.set_writable(false);
        // SAFETY: a valid handle and a double.
        let status = unsafe { (suite().param_set_value)(p.handle(), 1.0f64) };
        assert_eq!(status, kOfxStatFailed);
    }

    #[test]
    fn parametric_curves() {
        let set = ParamSet::new(SetKind::ParamDescriptor);
        let p = set.define(ParamType::Parametric, c"curve").unwrap();
        let s = &PARAMETRIC_SUITE;
        // SAFETY: a valid handle.
        unsafe {
            let h = p.handle();
            assert_eq!(
                (s.parametric_param_add_control_point)(h, 0, 0.0, 0.0, 0.0, false),
                0
            );
            assert_eq!(
                (s.parametric_param_add_control_point)(h, 0, 0.0, 1.0, 1.0, false),
                0
            );
            let mut v = 0.0;
            assert_eq!((s.parametric_param_get_value)(h, 0, 0.0, 0.5, &mut v), 0);
            assert!((v - 0.5).abs() < 1e-9);
            assert_eq!(
                (s.parametric_param_add_control_point)(h, 3, 0.0, 0.0, 0.0, false),
                kOfxStatErrBadIndex
            );
        }
        let inst = ParamSet::new(SetKind::ParamInstance);
        inst.instantiate(&set);
        let curve = inst.get("curve").unwrap();
        let identity = Value::Curves(vec![bv_core::curve::IDENTITY.to_vec()]);
        assert_eq!(curve.animation().value, identity);
        // An instance's change is no new default.
        let h = curve.handle();
        // SAFETY: the suite's functions on the instance's parameter.
        unsafe {
            assert_eq!(
                (s.parametric_param_add_control_point)(h, 0, 0.0, 0.5, 0.9, false),
                0
            );
        }
        assert_ne!(curve.animation().value, identity);
        assert_eq!(curve.default_value(), identity);
    }
}
