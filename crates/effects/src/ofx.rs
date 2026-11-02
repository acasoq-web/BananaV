//! The host as BananaV's plug-ins see it: the suites it hands out, and thin
//! wrappers over its handles — property sets, parameters, clips and their
//! images, threads and the draw suite. The plug-ins only call what the
//! OpenFX specification defines through them, so they run in any host.
//!
//! Nothing here may panic: a panic cannot cross back into the host.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_uint, c_void};
use std::ptr::null_mut;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::{Duration, Instant};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

static HOST: AtomicPtr<OfxHost> = AtomicPtr::new(null_mut());

/// The host's suites the plug-ins use.
#[derive(Debug)]
pub(crate) struct Suites {
    pub props: &'static OfxPropertySuiteV1,
    pub effect: &'static OfxImageEffectSuiteV1,
    pub params: &'static OfxParameterSuiteV1,
    pub threads: Option<&'static OfxMultiThreadSuiteV1>,
    pub interact: Option<&'static OfxInteractSuiteV1>,
    pub draw: Option<&'static OfxDrawSuiteV1>,
    pub parametric: Option<&'static OfxParametricParameterSuiteV1>,
    pub message: Option<&'static OfxMessageSuiteV2>,
}

/// Every plug-in's `setHost`.
pub(crate) unsafe extern "C" fn set_host(host: *mut OfxHost) {
    HOST.store(host, Ordering::Release);
}

/// The host's suites, once it gave itself; `None` before that, or when it
/// lacks one the plug-ins cannot do without.
pub(crate) fn suites() -> Option<&'static Suites> {
    static SUITES: OnceLock<Option<Suites>> = OnceLock::new();
    let host = HOST.load(Ordering::Acquire);
    if host.is_null() {
        return None;
    }
    SUITES
        .get_or_init(|| {
            // SAFETY: the host's own structure and fetch function; the suites
            // it hands out live as long as it does.
            unsafe {
                let get = |name: &CStr, version: c_int| {
                    let p = ((*host).fetch_suite)((*host).host, name.as_ptr(), version);
                    (!p.is_null()).then_some(p)
                };
                Some(Suites {
                    props: &*(get(kOfxPropertySuite, 1)? as *const OfxPropertySuiteV1),
                    effect: &*(get(kOfxImageEffectSuite, 1)? as *const OfxImageEffectSuiteV1),
                    params: &*(get(kOfxParameterSuite, 1)? as *const OfxParameterSuiteV1),
                    threads: get(kOfxMultiThreadSuite, 1)
                        .map(|p| &*(p as *const OfxMultiThreadSuiteV1)),
                    interact: get(kOfxInteractSuite, 1).map(|p| &*(p as *const OfxInteractSuiteV1)),
                    draw: get(kOfxDrawSuite, 1).map(|p| &*(p as *const OfxDrawSuiteV1)),
                    parametric: get(kOfxParametricParameterSuite, 1)
                        .map(|p| &*(p as *const OfxParametricParameterSuiteV1)),
                    message: get(kOfxMessageSuite, 2).map(|p| &*(p as *const OfxMessageSuiteV2)),
                })
            }
        })
        .as_ref()
}

/// Whether `BV_EFFECTS_TRACE` asks the plug-ins to tell what hosts give
/// them (on the standard error), to find out why one renders wrong in a
/// host.
pub(crate) fn tracing() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("BV_EFFECTS_TRACE").is_some())
}

/// Whether `action` (a C string from the host) is `name`.
pub(crate) fn is(action: *const c_char, name: &CStr) -> bool {
    // SAFETY: hosts pass actions as C strings.
    !action.is_null() && unsafe { CStr::from_ptr(action) } == name
}

/// A property set of the host's.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Props(pub OfxPropertySetHandle);

impl Props {
    fn suite(self) -> Option<&'static OfxPropertySuiteV1> {
        (!self.0.is_null()).then_some(suites()?.props)
    }

    pub fn set_str(self, name: &CStr, value: &CStr) {
        self.set_strs(name, &[value]);
    }

    pub fn set_strs(self, name: &CStr, values: &[&CStr]) {
        let Some(p) = self.suite() else { return };
        for (i, v) in values.iter().enumerate() {
            // SAFETY: the host's set, a name and a value it copies.
            unsafe { (p.prop_set_string)(self.0, name.as_ptr(), i as c_int, v.as_ptr()) };
        }
    }

    pub fn set_int(self, name: &CStr, value: c_int) {
        self.set_ints(name, &[value]);
    }

    pub fn set_ints(self, name: &CStr, values: &[c_int]) {
        let Some(p) = self.suite() else { return };
        for (i, v) in values.iter().enumerate() {
            // SAFETY: as above.
            unsafe { (p.prop_set_int)(self.0, name.as_ptr(), i as c_int, *v) };
        }
    }

    pub fn set_dbl(self, name: &CStr, value: f64) {
        self.set_dbls(name, &[value]);
    }

    pub fn set_dbls(self, name: &CStr, values: &[f64]) {
        let Some(p) = self.suite() else { return };
        for (i, v) in values.iter().enumerate() {
            // SAFETY: as above.
            unsafe { (p.prop_set_double)(self.0, name.as_ptr(), i as c_int, *v) };
        }
    }

    pub fn set_ptr(self, name: &CStr, value: *mut c_void) {
        let Some(p) = self.suite() else { return };
        // SAFETY: as above; the host keeps the pointer, not what it points to.
        unsafe { (p.prop_set_pointer)(self.0, name.as_ptr(), 0, value) };
    }

    /// Gives a property its default values back (a list's: none, as
    /// before it was first set).
    pub fn reset(self, name: &CStr) {
        let Some(p) = self.suite() else { return };
        // SAFETY: as above.
        unsafe { (p.prop_reset)(self.0, name.as_ptr()) };
    }

    /// How many values a property has (0 when it has none).
    pub fn count(self, name: &CStr) -> c_int {
        let mut n = 0;
        if let Some(p) = self.suite() {
            // SAFETY: the host's set; it writes one integer.
            unsafe { (p.prop_get_dimension)(self.0, name.as_ptr(), &mut n) };
        }
        n
    }

    /// Value `i` of an integer property (0 when it has none).
    pub fn int(self, name: &CStr, i: c_int) -> c_int {
        let mut v = 0;
        if let Some(p) = self.suite() {
            // SAFETY: the host's set; it writes one integer.
            unsafe { (p.prop_get_int)(self.0, name.as_ptr(), i, &mut v) };
        }
        v
    }

    pub fn ints<const N: usize>(self, name: &CStr) -> [c_int; N] {
        std::array::from_fn(|i| self.int(name, i as c_int))
    }

    /// Value `i` of a double property (0 when it has none).
    pub fn dbl(self, name: &CStr, i: c_int) -> f64 {
        let mut v = 0.0;
        if let Some(p) = self.suite() {
            // SAFETY: the host's set; it writes one double.
            unsafe { (p.prop_get_double)(self.0, name.as_ptr(), i, &mut v) };
        }
        v
    }

    pub fn dbls<const N: usize>(self, name: &CStr) -> [f64; N] {
        std::array::from_fn(|i| self.dbl(name, i as c_int))
    }

    /// A double property's first value, `default` when it has none (a
    /// host that does not know it).
    pub fn dbl_or(self, name: &CStr, default: f64) -> f64 {
        let mut v = default;
        if let Some(p) = self.suite() {
            // SAFETY: the host's set; it writes one double, or nothing
            // when it has no such property.
            let ok = unsafe { (p.prop_get_double)(self.0, name.as_ptr(), 0, &mut v) } == kOfxStatOK;
            if !ok {
                v = default;
            }
        }
        v
    }

    pub fn ptr(self, name: &CStr) -> *mut c_void {
        let mut v = null_mut();
        if let Some(p) = self.suite() {
            // SAFETY: the host's set; it writes one pointer.
            unsafe { (p.prop_get_pointer)(self.0, name.as_ptr(), 0, &mut v) };
        }
        v
    }

    /// Value `i` of a string property (empty when it has none).
    pub fn text(self, name: &CStr, i: c_int) -> String {
        let Some(p) = self.suite() else {
            return String::new();
        };
        let mut v: *mut c_char = null_mut();
        // SAFETY: the host's set; it hands out a string it keeps.
        unsafe {
            (p.prop_get_string)(self.0, name.as_ptr(), i, &mut v);
            if v.is_null() {
                String::new()
            } else {
                CStr::from_ptr(v).to_string_lossy().into_owned()
            }
        }
    }

    /// Whether a string property's first value is `value`.
    pub fn is(self, name: &CStr, value: &CStr) -> bool {
        let Some(p) = self.suite() else { return false };
        let mut v: *mut c_char = null_mut();
        // SAFETY: the host's set; it hands out a string it keeps.
        unsafe {
            (p.prop_get_string)(self.0, name.as_ptr(), 0, &mut v);
            !v.is_null() && CStr::from_ptr(v) == value
        }
    }
}

/// An image effect of the host's: a descriptor or an instance.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Fx(pub OfxImageEffectHandle);

impl Fx {
    pub fn props(self) -> Props {
        let mut p = null_mut();
        if let Some(s) = suites() {
            // SAFETY: the host's effect.
            unsafe { (s.effect.get_property_set)(self.0, &mut p) };
        }
        Props(p)
    }

    fn param_set(self) -> OfxParamSetHandle {
        let mut p = null_mut();
        if let Some(s) = suites() {
            // SAFETY: the host's effect.
            unsafe { (s.effect.get_param_set)(self.0, &mut p) };
        }
        p
    }

    /// Defines a parameter (while describing); its properties.
    pub fn define_param(self, ty: &CStr, name: &CStr, label: &CStr) -> Props {
        let mut props = null_mut();
        if let Some(s) = suites() {
            // SAFETY: the host's parameter set, names it copies.
            unsafe {
                (s.params.param_define)(self.param_set(), ty.as_ptr(), name.as_ptr(), &mut props)
            };
        }
        let props = Props(props);
        props.set_str(kOfxPropLabel, label);
        props
    }

    pub fn param(self, name: &CStr) -> Param {
        let mut h = null_mut();
        if let Some(s) = suites() {
            // SAFETY: the host's parameter set.
            unsafe {
                (s.params.param_get_handle)(self.param_set(), name.as_ptr(), &mut h, null_mut())
            };
        }
        Param(h)
    }

    /// Defines a clip (while describing) taking `components`.
    pub fn define_clip(self, name: &CStr, components: &[&CStr]) -> Props {
        let mut props = null_mut();
        if let Some(s) = suites() {
            // SAFETY: the host's effect, a name it copies.
            unsafe { (s.effect.clip_define)(self.0, name.as_ptr(), &mut props) };
        }
        let props = Props(props);
        props.set_strs(kOfxImageEffectPropSupportedComponents, components);
        props
    }

    pub fn clip(self, name: &CStr) -> Clip {
        let mut h = null_mut();
        if let Some(s) = suites() {
            // SAFETY: the host's effect.
            unsafe { (s.effect.clip_get_handle)(self.0, name.as_ptr(), &mut h, null_mut()) };
        }
        Clip(h)
    }

    /// The project's frame, in canonical coordinates: where it starts and
    /// its size.
    pub fn frame(self) -> ([f64; 2], [f64; 2]) {
        let p = self.props();
        let size: [f64; 2] = p.dbls(kOfxImageEffectPropProjectSize);
        let offset: [f64; 2] = p.dbls(kOfxImageEffectPropProjectOffset);
        (offset, size.map(|v| v.max(1.0)))
    }

    /// Shows `text` (an error) with the effect's settings until it is
    /// cleared (`None`), where the host has the message suite.
    pub fn persistent_message(self, text: Option<&CStr>) {
        self.persistent_message_as(kOfxMessageError, text);
    }

    /// Shows `text` as a message of `kind` (OpenFX's error, warning or
    /// message) with the effect's settings until it is cleared (`None`).
    pub fn persistent_message_as(self, kind: &CStr, text: Option<&CStr>) {
        let Some(m) = suites().and_then(|s| s.message) else {
            return;
        };
        let h = self.0 as *mut c_void;
        // SAFETY: the host's suite and this effect's handle; the text goes
        // through "%s", so nothing in it formats.
        unsafe {
            match text {
                Some(t) => (m.set_persistent_message)(
                    h,
                    kind.as_ptr(),
                    c"".as_ptr(),
                    c"%s".as_ptr(),
                    t.as_ptr(),
                ),
                None => (m.clear_persistent_message)(h),
            }
        };
    }
}

/// A parameter of the host's.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Param(pub OfxParamHandle);

impl Param {
    fn suite(self) -> Option<&'static OfxParameterSuiteV1> {
        (!self.0.is_null()).then_some(suites()?.params)
    }

    /// A double parameter's value at `time`.
    pub fn f64_at(self, time: f64) -> f64 {
        let mut v = 0.0f64;
        if let Some(p) = self.suite() {
            // SAFETY: a double parameter writes one double.
            unsafe { (p.param_get_value_at_time)(self.0, time, &mut v as *mut f64) };
        }
        v
    }

    /// A 2D double parameter's value at `time`.
    pub fn xy_at(self, time: f64) -> [f64; 2] {
        let (mut x, mut y) = (0.0f64, 0.0f64);
        if let Some(p) = self.suite() {
            // SAFETY: a 2D double parameter writes two doubles.
            unsafe {
                (p.param_get_value_at_time)(self.0, time, &mut x as *mut f64, &mut y as *mut f64)
            };
        }
        [x, y]
    }

    /// A 2D integer parameter's value at `time`.
    pub fn ij_at(self, time: f64) -> [c_int; 2] {
        let (mut x, mut y) = (0 as c_int, 0 as c_int);
        if let Some(p) = self.suite() {
            // SAFETY: a 2D integer parameter writes two ints.
            unsafe {
                (p.param_get_value_at_time)(
                    self.0,
                    time,
                    &mut x as *mut c_int,
                    &mut y as *mut c_int,
                )
            };
        }
        [x, y]
    }

    /// Sets a 2D integer parameter.
    pub fn set_ij(self, [x, y]: [c_int; 2]) {
        if let Some(p) = self.suite() {
            // SAFETY: a 2D integer parameter takes two ints.
            unsafe { (p.param_set_value)(self.0, x, y) };
        }
    }

    /// Its property set (an instance's: what the host shows of it).
    pub fn props(self) -> Props {
        let mut props = null_mut();
        if let Some(p) = self.suite() {
            // SAFETY: the host's parameter; it writes its set's handle.
            unsafe { (p.param_get_property_set)(self.0, &mut props) };
        }
        Props(props)
    }

    /// Shows it enabled, or greyed out.
    pub fn set_enabled(self, on: bool) {
        self.props().set_int(kOfxParamPropEnabled, on as c_int);
    }

    /// Hides it from the user (kept, working as before), or shows it.
    pub fn set_secret(self, hidden: bool) {
        self.props().set_int(kOfxParamPropSecret, hidden as c_int);
    }

    /// A 3D double parameter's value at `time`.
    pub fn xyz_at(self, time: f64) -> [f64; 3] {
        let (mut x, mut y, mut z) = (0.0f64, 0.0f64, 0.0f64);
        if let Some(p) = self.suite() {
            // SAFETY: a 3D double parameter writes three doubles.
            unsafe {
                (p.param_get_value_at_time)(
                    self.0,
                    time,
                    &mut x as *mut f64,
                    &mut y as *mut f64,
                    &mut z as *mut f64,
                )
            };
        }
        [x, y, z]
    }

    pub fn set_xyz(self, [x, y, z]: [f64; 3]) {
        if let Some(p) = self.suite() {
            // SAFETY: a 3D double parameter takes three doubles.
            unsafe { (p.param_set_value)(self.0, x, y, z) };
        }
    }

    /// A color parameter's red, green and blue at `time`.
    pub fn rgb_at(self, time: f64) -> [f64; 3] {
        let (mut r, mut g, mut b) = (0.0f64, 0.0f64, 0.0f64);
        if let Some(p) = self.suite() {
            // SAFETY: an RGB parameter writes three doubles.
            unsafe {
                (p.param_get_value_at_time)(
                    self.0,
                    time,
                    &mut r as *mut f64,
                    &mut g as *mut f64,
                    &mut b as *mut f64,
                )
            };
        }
        [r, g, b]
    }

    /// A color-with-alpha parameter's value at `time`.
    pub fn rgba_at(self, time: f64) -> [f64; 4] {
        let mut v = [0.0f64; 4];
        if let Some(p) = self.suite() {
            let [r, g, b, a] = &mut v;
            // SAFETY: an RGBA parameter writes four doubles.
            unsafe {
                (p.param_get_value_at_time)(
                    self.0,
                    time,
                    r as *mut f64,
                    g as *mut f64,
                    b as *mut f64,
                    a as *mut f64,
                )
            };
        }
        v
    }

    /// An integer or choice parameter's value at `time`.
    pub fn int_at(self, time: f64) -> c_int {
        let mut v: c_int = 0;
        if let Some(p) = self.suite() {
            // SAFETY: an integer parameter writes one int.
            unsafe { (p.param_get_value_at_time)(self.0, time, &mut v as *mut c_int) };
        }
        v
    }

    /// Sets a double parameter (a key at the current time when it is
    /// animated).
    pub fn set_f64(self, v: f64) {
        if let Some(p) = self.suite() {
            // SAFETY: a double parameter takes one double.
            unsafe { (p.param_set_value)(self.0, v) };
        }
    }

    /// Keys a double parameter at `time`.
    pub fn set_f64_at(self, time: f64, v: f64) {
        if let Some(p) = self.suite() {
            // SAFETY: a double parameter takes one double.
            unsafe { (p.param_set_value_at_time)(self.0, time, v) };
        }
    }

    /// Takes its keys away (its value stays what it was).
    pub fn delete_keys(self) {
        if let Some(p) = self.suite() {
            // SAFETY: the host's parameter.
            unsafe { (p.param_delete_all_keys)(self.0) };
        }
    }

    pub fn set_xy(self, [x, y]: [f64; 2]) {
        if let Some(p) = self.suite() {
            // SAFETY: a 2D double parameter takes two doubles.
            unsafe { (p.param_set_value)(self.0, x, y) };
        }
    }

    pub fn set_int(self, v: c_int) {
        if let Some(p) = self.suite() {
            // SAFETY: an integer parameter takes one int.
            unsafe { (p.param_set_value)(self.0, v) };
        }
    }

    /// Sets a color parameter's red, green and blue.
    pub fn set_rgb(self, [r, g, b]: [f64; 3]) {
        if let Some(p) = self.suite() {
            // SAFETY: an RGB parameter takes three doubles.
            unsafe { (p.param_set_value)(self.0, r, g, b) };
        }
    }

    /// Sets a color-with-alpha parameter.
    pub fn set_rgba(self, [r, g, b, a]: [f64; 4]) {
        if let Some(p) = self.suite() {
            // SAFETY: an RGBA parameter takes four doubles.
            unsafe { (p.param_set_value)(self.0, r, g, b, a) };
        }
    }

    /// A string parameter's value at `time` (empty when it has none).
    pub fn text_at(self, time: f64) -> String {
        let mut v: *const c_char = std::ptr::null();
        if let Some(p) = self.suite() {
            // SAFETY: a string parameter writes a pointer to a string the
            // host keeps.
            unsafe { (p.param_get_value_at_time)(self.0, time, &mut v as *mut *const c_char) };
        }
        if v.is_null() {
            return String::new();
        }
        // SAFETY: the host's string, alive during this action.
        unsafe { CStr::from_ptr(v) }.to_string_lossy().into_owned()
    }

    pub fn set_text(self, v: &str) {
        let Ok(v) = std::ffi::CString::new(v) else {
            return;
        };
        if let Some(p) = self.suite() {
            // SAFETY: a string parameter takes a string it copies.
            unsafe { (p.param_set_value)(self.0, v.as_ptr()) };
        }
    }

    /// Its text at `time` (a keyframe there).
    pub fn set_text_at(self, time: f64, v: &str) {
        let Ok(v) = std::ffi::CString::new(v) else {
            return;
        };
        if let Some(p) = self.suite() {
            // SAFETY: a string parameter takes a string it copies.
            unsafe { (p.param_set_value_at_time)(self.0, time, v.as_ptr()) };
        }
    }

    /// Curve `curve` of a parametric parameter at `x`, at `time`.
    pub fn curve_at(self, curve: c_int, time: f64, x: f64) -> f64 {
        let mut v = 0.0;
        if let Some(s) = suites().and_then(|s| s.parametric)
            && !self.0.is_null()
        {
            // SAFETY: the host's parametric parameter; it writes one double.
            unsafe { (s.parametric_param_get_value)(self.0, curve, time, x, &mut v) };
        }
        v
    }

    /// Gives curve `curve` of a parametric parameter these points (at
    /// `time`: a key there when it is animated).
    pub fn set_curve(self, curve: c_int, time: f64, points: &[[f64; 2]]) {
        let Some(s) = suites().and_then(|s| s.parametric) else {
            return;
        };
        if self.0.is_null() {
            return;
        }
        // SAFETY: the host's parametric parameter.
        unsafe {
            (s.parametric_param_delete_all_control_points)(self.0, curve);
            for &[x, y] in points {
                (s.parametric_param_add_control_point)(self.0, curve, time, x, y, false);
            }
        }
    }

    /// The times of its keys (none: it is not animated).
    pub fn key_times(self) -> Vec<f64> {
        let Some(p) = self.suite() else {
            return Vec::new();
        };
        let mut n: c_uint = 0;
        // SAFETY: the host's parameter; it writes the count.
        unsafe { (p.param_get_num_keys)(self.0, &mut n) };
        (0..n)
            .filter_map(|i| {
                let mut t = 0.0;
                // SAFETY: an index below the count.
                let ok = unsafe { (p.param_get_key_time)(self.0, i, &mut t) } == kOfxStatOK;
                ok.then_some(t)
            })
            .collect()
    }
}

/// A clip of an effect instance.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Clip(pub OfxImageClipHandle);

impl Clip {
    /// Its region of definition at `time` (canonical coordinates: x1, y1,
    /// x2, y2).
    pub fn rod(self, time: f64) -> Option<[f64; 4]> {
        let s = suites()?;
        if self.0.is_null() {
            return None;
        }
        let mut r = OfxRectD::default();
        // SAFETY: the host's clip, a rectangle to fill.
        (unsafe { (s.effect.clip_get_region_of_definition)(self.0, time, &mut r) } == kOfxStatOK)
            .then_some([r.x1, r.y1, r.x2, r.y2])
    }

    /// Its property set.
    pub fn props(self) -> Option<Props> {
        let s = suites()?;
        if self.0.is_null() {
            return None;
        }
        let mut p = null_mut();
        // SAFETY: the host's clip, a set to fill.
        (unsafe { (s.effect.clip_get_property_set)(self.0, &mut p) } == kOfxStatOK && !p.is_null())
            .then_some(Props(p))
    }

    /// Whether the host connected something to it.
    pub fn connected(self) -> bool {
        let Some(s) = suites() else {
            return false;
        };
        if self.0.is_null() {
            return false;
        }
        let mut p = null_mut();
        // SAFETY: the host's clip, a set to fill.
        if unsafe { (s.effect.clip_get_property_set)(self.0, &mut p) } != kOfxStatOK {
            return false;
        }
        Props(p).int(kOfxImageClipPropConnected, 0) != 0
    }

    /// Its image at `time`, while rendering.
    pub fn image(self, time: f64) -> Option<Image> {
        let s = suites()?;
        if self.0.is_null() {
            return None;
        }
        let mut h = null_mut();
        // SAFETY: the host's clip.
        if unsafe { (s.effect.clip_get_image)(self.0, time, std::ptr::null(), &mut h) }
            != kOfxStatOK
            || h.is_null()
        {
            return None;
        }
        let props = Props(h);
        let depth = if props.is(kOfxImageEffectPropPixelDepth, kOfxBitDepthFloat) {
            Depth::Float
        } else if props.is(kOfxImageEffectPropPixelDepth, kOfxBitDepthShort) {
            Depth::Short
        } else {
            Depth::Byte
        };
        let channels = if props.is(kOfxImageEffectPropComponents, kOfxImageComponentRGBA) {
            4
        } else if props.is(kOfxImageEffectPropComponents, kOfxImageComponentRGB) {
            3
        } else {
            1
        };
        let par = props.dbl(kOfxImagePropPixelAspectRatio, 0);
        Some(Image {
            handle: h,
            data: props.ptr(kOfxImagePropData) as *mut u8,
            row_bytes: props.int(kOfxImagePropRowBytes, 0) as isize,
            bounds: props.ints(kOfxImagePropBounds),
            depth,
            channels,
            unpremultiplied: props.is(
                kOfxImageEffectPropPreMultiplication,
                kOfxImageUnPreMultiplied,
            ),
            par: if par > 0.0 { par } else { 1.0 },
        })
    }
}

/// How an image keeps each channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Depth {
    Byte,
    Short,
    Float,
}

/// An image the host handed out (released when dropped).
#[derive(Debug)]
pub(crate) struct Image {
    handle: OfxPropertySetHandle,
    data: *mut u8,
    row_bytes: isize,
    /// Its pixels: x1, y1, x2, y2 (y up).
    pub bounds: [c_int; 4],
    pub depth: Depth,
    /// 4 (RGBA), 3 (RGB) or 1 (alpha).
    pub channels: usize,
    /// Its colors are not multiplied by its alpha.
    pub unpremultiplied: bool,
    pub par: f64,
}

// SAFETY: render threads read the source's rows and each writes rows of
// the output no other thread writes; the host keeps the memory until the
// image is released, after the threads ended.
unsafe impl Send for Image {}
// SAFETY: as above.
unsafe impl Sync for Image {}

impl Image {
    /// Where row `y` starts (at the bounds' x1), when the image has it.
    pub fn row(&self, y: c_int) -> Option<*mut u8> {
        let [_, y1, _, y2] = self.bounds;
        (!self.data.is_null() && (y1..y2).contains(&y)).then(|| {
            // SAFETY: a row inside the bounds the host gave.
            unsafe { self.data.offset((y - y1) as isize * self.row_bytes) }
        })
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        if let Some(s) = suites() {
            // SAFETY: an image the host handed out, released once.
            unsafe { (s.effect.clip_release_image)(self.handle) };
        }
    }
}

/// Runs `f` for each row of `rows`, on the host's threads (or here, when
/// it lends none).
pub(crate) fn for_rows(rows: std::ops::Range<c_int>, f: &(dyn Fn(c_int) + Sync)) {
    struct Job<'a> {
        f: &'a (dyn Fn(c_int) + Sync),
        rows: std::ops::Range<c_int>,
    }
    unsafe extern "C" fn run(index: c_uint, count: c_uint, arg: *mut c_void) {
        // SAFETY: the job below, alive until `multi_thread` returns.
        let job = unsafe { &*(arg as *const Job<'_>) };
        let step = count.max(1) as usize;
        for y in job.rows.clone().skip(index as usize).step_by(step) {
            (job.f)(y);
        }
    }
    let job = Job {
        f,
        rows: rows.clone(),
    };
    let threaded = suites().and_then(|s| s.threads).is_some_and(|t| {
        // As many threads as the host has processors for us (it launches
        // exactly as many as asked: none, for 0).
        let mut cpus: c_uint = 0;
        // SAFETY: the host writes one count.
        let told = unsafe { (t.multi_thread_num_cpus)(&mut cpus) } == kOfxStatOK;
        let n = threads(told.then_some(cpus), rows.len());
        // SAFETY: the job outlives the call, which returns when every
        // thread is done.
        unsafe { (t.multi_thread)(run, n, &job as *const Job<'_> as *mut c_void) == kOfxStatOK }
    });
    if !threaded {
        for y in rows {
            f(y);
        }
    }
}

/// How many threads to ask the host for: as many as its processors
/// (one when it does not say), no more than there are rows — never none:
/// hosts launch exactly as many as asked.
fn threads(cpus: Option<c_uint>, rows: usize) -> c_uint {
    cpus.unwrap_or(1).clamp(1, rows.max(1) as c_uint)
}

/// The property set behind an interact's handle.
pub(crate) fn interact_props(handle: *const c_void) -> Option<Props> {
    let s = suites()?.interact?;
    let mut p = null_mut();
    // SAFETY: the host's interact.
    let ok = unsafe { (s.interact_get_property_set)(handle as OfxInteractHandle, &mut p) };
    (ok == kOfxStatOK && !p.is_null()).then_some(Props(p))
}

/// Asks the host to draw an interact again.
pub(crate) fn redraw(handle: *const c_void) {
    if let Some(s) = suites().and_then(|s| s.interact) {
        // SAFETY: the host's interact.
        unsafe { (s.interact_redraw)(handle as OfxInteractHandle) };
    }
}

/// What an overlay draws on, during its draw action.
#[derive(Debug)]
pub(crate) struct Draw {
    ctx: OfxDrawContextHandle,
    suite: &'static OfxDrawSuiteV1,
}

impl Draw {
    /// The draw action's context, when the host has the draw suite.
    pub fn of(args: Props) -> Option<Draw> {
        let suite = suites()?.draw?;
        let ctx = args.ptr(kOfxInteractPropDrawContext) as OfxDrawContextHandle;
        (!ctx.is_null()).then_some(Draw { ctx, suite })
    }

    pub fn colour(&self, [r, g, b, a]: [f32; 4]) {
        let c = OfxRGBAColourF { r, g, b, a };
        // SAFETY: the host's context, during its draw action.
        unsafe { (self.suite.set_colour)(self.ctx, &c) };
    }

    pub fn width(&self, w: f32) {
        // SAFETY: as above.
        unsafe { (self.suite.set_line_width)(self.ctx, w) };
    }

    fn shape(&self, kind: OfxDrawPrimitive, points: &[[f64; 2]]) {
        let pts: Vec<OfxPointD> = points.iter().map(|&[x, y]| OfxPointD { x, y }).collect();
        // SAFETY: as above, with the points' count.
        unsafe { (self.suite.draw)(self.ctx, kind, pts.as_ptr(), pts.len() as c_int) };
    }

    /// Separate lines, the points in pairs.
    pub fn lines(&self, points: &[[f64; 2]]) {
        self.shape(kOfxDrawPrimitiveLines, points);
    }

    /// A closed outline.
    pub fn outline(&self, points: &[[f64; 2]]) {
        self.shape(kOfxDrawPrimitiveLineLoop, points);
    }

    /// A filled shape.
    pub fn fill(&self, points: &[[f64; 2]]) {
        self.shape(kOfxDrawPrimitivePolygon, points);
    }

    /// Text at a point, aligned as `align` says (`kOfxDrawTextAlignment…`).
    pub fn text(&self, text: &CStr, at: [f64; 2], align: c_int) {
        let p = OfxPointD { x: at[0], y: at[1] };
        // SAFETY: as above, with a string and a point it reads.
        unsafe { (self.suite.draw_text)(self.ctx, text.as_ptr(), &p, align) };
    }

    /// A handle on the preview at `at` (canonical units; `ps` the pixel
    /// scale): a ring with a cross through it, light over dark, warmer
    /// while the pen is on it (`hot`).
    pub fn handle(&self, at: [f64; 2], ps: [f64; 2], hot: bool) {
        let ring: Vec<[f64; 2]> = (0..32)
            .map(|i| {
                let t = std::f64::consts::TAU * i as f64 / 32.0;
                [
                    at[0] + HANDLE_RING * ps[0] * t.cos(),
                    at[1] + HANDLE_RING * ps[1] * t.sin(),
                ]
            })
            .collect();
        let arm = 13.0;
        let cross = [
            [at[0] - arm * ps[0], at[1]],
            [at[0] + arm * ps[0], at[1]],
            [at[0], at[1] - arm * ps[1]],
            [at[0], at[1] + arm * ps[1]],
        ];
        let light = if hot {
            [1.0, 0.9, 0.45, 1.0]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };
        for (colour, width) in [([0.0, 0.0, 0.0, 0.8], 3.0), (light, 1.4)] {
            self.colour(colour);
            self.width(width);
            self.outline(&ring);
            self.lines(&cross);
        }
    }
}

/// A handle's ring, in screen pixels ([`Draw::handle`]), and how near the
/// pen takes it.
pub(crate) const HANDLE_RING: f64 = 8.0;
pub(crate) const HANDLE_REACH: f64 = 13.0;

/// Whether the pen at `pen` takes a handle at `at` (canonical units; `ps`
/// the pixel scale).
pub(crate) fn near_handle(pen: [f64; 2], at: [f64; 2], ps: [f64; 2]) -> bool {
    ((pen[0] - at[0]) / ps[0]).hypot((pen[1] - at[1]) / ps[1]) <= HANDLE_REACH
}

/// Tells a double click among an interface's presses (OpenFX's pen has
/// none): a second press soon after the first and near it.
#[derive(Debug, Default)]
pub(crate) struct Clicks {
    last: Option<(Instant, [f64; 2])>,
}

impl Clicks {
    /// How soon and how near (in the interface's pixels) the second press.
    const SOON: Duration = Duration::from_millis(500);
    const NEAR: f64 = 6.0;

    /// Notes a press at `at`, `now`; whether it ends a double click.
    pub fn press(&mut self, at: [f64; 2], now: Instant) -> bool {
        let double = self.last.is_some_and(|(t, p)| {
            now.saturating_duration_since(t) <= Self::SOON
                && (p[0] - at[0]).hypot(p[1] - at[1]) <= Self::NEAR
        });
        // A third press starts again.
        self.last = (!double).then_some((now, at));
        double
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plug_ins_ask_for_threads_they_get() {
        // Natron launches none for 0 (the plug-in then never rendered).
        assert_eq!(threads(Some(0), 1080), 1);
        assert_eq!(threads(None, 1080), 1);
        assert_eq!(threads(Some(16), 1080), 16);
        assert_eq!(threads(Some(16), 3), 3);
        assert_eq!(threads(Some(16), 0), 1);
    }

    #[test]
    fn two_quick_presses_in_one_place_are_a_double_click() {
        let mut c = Clicks::default();
        let t = Instant::now();
        let ms = |n: u64| t + Duration::from_millis(n);
        assert!(!c.press([100.0, 60.0], t));
        assert!(c.press([102.0, 61.0], ms(300)));
        // A third press starts again.
        assert!(!c.press([102.0, 61.0], ms(400)));
        // Too late, or too far.
        assert!(!c.press([102.0, 61.0], ms(1000)));
        assert!(!c.press([130.0, 61.0], ms(1100)));
    }
}
