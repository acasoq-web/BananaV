//! OpenFX's C interface for Rust, as the headers in `vendor/openfx`
//! (OpenFX 1.5) declare it: the handles, the plug-in and host structures,
//! the suites' function tables and, in [`names`], every name of the
//! specification. No logic lives here: the host is `bv-ofx`.
//!
//! The suites' functions that C declares with `...` (the parameter suite's
//! value functions, the message suite) keep that type here: plug-ins call
//! them as variadic functions. `bv-ofx` explains how it receives them.

// The specification's names, as C spells them.
#![allow(non_upper_case_globals)]

use std::ffi::{c_char, c_double, c_float, c_int, c_uint, c_void};

pub mod names;

/// BananaV's additions to OpenFX's names, for what the specification
/// announces and has not named yet. A host without them refuses them (a
/// property it does not have), and plug-ins then do without.
pub mod extensions {
    use std::ffi::CStr;

    /// A parameter's own interface drawn with the draw suite, as the second
    /// version of overlays is: a pointer to the interact's entry point
    /// (`kOfxParamPropInteractV1`'s draws with OpenGL, and ofxParam.h says
    /// "APIs will be added later to support using the new Draw Suite").
    /// The host shows it `kOfxParamPropInteractPreferedSize` big, under the
    /// parameter's own controls; it draws and takes the pen in screen
    /// pixels, x to the right and y up from its bottom left corner.
    pub const kBananaVParamPropInteract: &CStr = c"BananaVParamPropInteract";

    /// Whether that interface shows at first (1, the default: a color
    /// wheel) or waits under its triangle (0: a light's box), an integer.
    pub const kBananaVParamPropInteractOpen: &CStr = c"BananaVParamPropInteractOpen";

    /// Among an overlay's actions' arguments: the keyboard's modifier keys
    /// held, an integer — 1 Shift, 2 Control, 4 Alt (OpenFX's pen actions
    /// say none; Mesh Warp's Shift keeps a drag to a point's neighbors).
    pub const kBananaVInteractPropModifiers: &CStr = c"BananaVInteractPropModifiers";

    /// The model a color parameter's picker shows at first: "HSV" (the
    /// default), "HSL", "RGB" or "Lab", a string.
    pub const kBananaVParamPropColorModel: &CStr = c"BananaVParamPropColorModel";

    /// A number's slider runs logarithmically over its display range (both
    /// ends above 0), as VEGAS's scales' do: the range's geometric middle
    /// sits in its middle. An integer, 0 (the default) or 1.
    pub const kBananaVParamPropLogarithmic: &CStr = c"BananaVParamPropLogarithmic";

    /// The frame's place on the host's timeline, as its ruler shows it, in
    /// seconds: a double among a render action's arguments. An event's
    /// effect counts its own time from the event's start; this is the
    /// project's (what a timecode burned into the picture shows).
    pub const kBananaVImageEffectPropTimelineTime: &CStr = c"BananaVImageEffectPropTimelineTime";

    /// The picture a push button asks the host for, a string: when the
    /// button is pushed, the host connects that picture to the clip
    /// `kBananaVParamPropPictureClip` names (an optional one of the
    /// effect's) before it tells the plug-in of the push, so its
    /// instance-changed action reads it there. "Preview": the frame the
    /// host's video preview shows; "Input": the effect's own input at the
    /// time; "Trimmer": the frame its trimmer shows; "File": a picture file
    /// the user picks; "Clipboard": a picture on the system's clipboard;
    /// "Screen": a part of the screen the user picks. A host that cannot
    /// give one greys the button out.
    pub const kBananaVParamPropPicture: &CStr = c"BananaVParamPropPicture";

    /// The clip a push button's picture arrives on, a string.
    pub const kBananaVParamPropPictureClip: &CStr = c"BananaVParamPropPictureClip";

    /// What a push button asks the host to analyze, a string: "Motion",
    /// the motion of the effect's input from each frame to the next, over
    /// its event (or its media, for a media's effect). The host works it
    /// out in the background and writes it into the string parameter
    /// `kBananaVParamPropAnalysisParam` names, a line a frame after a
    /// first `bvmotion 2 <count>` line: the shift across and down as parts
    /// of the frame's width, the turn in radians, the scale, then the
    /// move's linear part about the frame's middle (`a b c d`: x′ = a x +
    /// b y, y′ = c x + d y, rows down). "Histogram":
    /// the input's light (Rec. 709's weights of its straight colors, as the
    /// effect gets them, every pixel of the whole frame) counted over the
    /// event's frames in `<bins>` bins over 0–1, after a first `bvlight 1
    /// <bins>` line. "Histograms": the same, each frame's apart — a first
    /// `bvlights 1 <bins>` line, then a line a frame, its time (the
    /// effect's) before its counts. "ColorHistograms": each frame's red,
    /// green and blue apart, every pixel's channels as the effect gets
    /// them (premultiplied) counted in `<levels>` levels (a value's level:
    /// it times the levels, rounded, within them) — a first `bvcolors 1
    /// <levels>` line, then a line a frame, its time before its red, green
    /// and blue counts. A plug-in may set it as its settings change
    /// (empty: the button asks for nothing).
    pub const kBananaVParamPropAnalysis: &CStr = c"BananaVParamPropAnalysis";

    /// The string parameter an analysis's result goes to.
    pub const kBananaVParamPropAnalysisParam: &CStr = c"BananaVParamPropAnalysisParam";

    /// A choice parameter saying how hard to look: its first option fast,
    /// the others accurate (optional).
    pub const kBananaVParamPropAnalysisMethod: &CStr = c"BananaVParamPropAnalysisMethod";

    /// For "Rotation" (an equirectangular picture's turns, a rotation
    /// vector a line, radians): a choice parameter leaving poles out of
    /// the analysis (none, the north, the south, both), and one saying how
    /// a stereoscopic picture holds its eyes (none, top and bottom, side by
    /// side: the first eye's picture is analyzed). Both optional.
    pub const kBananaVParamPropAnalysisExclude: &CStr = c"BananaVParamPropAnalysisExclude";

    /// For "Motion": the integer parameters setting the analysis's own
    /// work, their names apart by spaces — the frames' width, how many
    /// features are followed, the steps of each follow (optional; else
    /// the method's).
    pub const kBananaVParamPropAnalysisTuning: &CStr = c"BananaVParamPropAnalysisTuning";
    pub const kBananaVParamPropAnalysisLayout: &CStr = c"BananaVParamPropAnalysisLayout";

    /// For "Motion": the area analyzed — a two-dimensional double
    /// parameter of its middle (parts of the frame, rows up) and two
    /// double ones of its width and height (parts of the frame), their
    /// names apart by spaces (optional; else the whole frame but a
    /// margin).
    pub const kBananaVParamPropAnalysisRegion: &CStr = c"BananaVParamPropAnalysisRegion";

    /// For "Motion": the integer parameters of a grid of the frame's
    /// cells the features are spread over — its columns, its rows and
    /// each cell's most —, their names apart by spaces (optional).
    pub const kBananaVParamPropAnalysisGrid: &CStr = c"BananaVParamPropAnalysisGrid";

    /// For "Motion": a choice parameter saying how features are followed —
    /// Lucas–Kanade, the least sum of absolute differences, or the second
    /// refined by the first (optional).
    pub const kBananaVParamPropAnalysisFollow: &CStr = c"BananaVParamPropAnalysisFollow";

    /// For "Motion": a choice parameter saying how many times the frames
    /// are halved for the analysis — not, once, twice (optional; else the
    /// tuning's width or the method's).
    pub const kBananaVParamPropAnalysisReduction: &CStr = c"BananaVParamPropAnalysisReduction";

    /// On a multi-line string parameter: the host styles parts of its
    /// text, a string of names apart by spaces — first a hidden string
    /// parameter holding the runs, then the parameters a part may set.
    /// While part of the text is selected, a change of one of those sets
    /// that part instead of the whole: a line `start end name=value` is
    /// added to the runs (characters `start..end` of the text, the
    /// parameter's value as text: a number, or a string without `;`, `=`
    /// or line breaks; later lines over earlier ones; several settings of
    /// one line apart by `;`). As the text is typed the host moves the
    /// runs along with it.
    pub const kBananaVParamPropRichText: &CStr = c"BananaVParamPropRichText";

    /// On a push button: the host follows one of the effect's masks from
    /// the cursor through its event's picture and keyframes it, a string
    /// of words apart by spaces — the mask's number (the host knows the
    /// effect's masks' boxes), the names of its location, angle, width and
    /// height parameters, then of the parameters choosing the tracking's
    /// precision (Normal, High), mode (Location; Rotation & Location; Size,
    /// Rotation & Location), direction (Forward, Backward, Both) and
    /// keyframe interval (frames).
    pub const kBananaVParamPropMaskTracking: &CStr = c"BananaVParamPropMaskTracking";

    /// On a push button: the host makes the effect's event as long as the
    /// rest of its media needs played at the rate the double parameter
    /// this names holds, a string (Slow Motion's Adjust Event Length).
    pub const kBananaVParamPropEventLength: &CStr = c"BananaVParamPropEventLength";
}

/// What every OFX function returns: `names::kOfxStatOK` and the others.
pub type OfxStatus = c_int;

/// A time, in frames.
pub type OfxTime = c_double;

macro_rules! handle {
    ($(#[$doc:meta])* $handle:ident, $target:ident) => {
        $(#[$doc])*
        #[repr(C)]
        #[derive(Debug)]
        pub struct $target {
            _private: [u8; 0],
        }
        $(#[$doc])*
        pub type $handle = *mut $target;
    };
}

handle!(
    /// A set of properties: a host, a plug-in's descriptor, an image, the
    /// arguments of an action…
    OfxPropertySetHandle,
    OfxPropertySetStruct
);
handle!(
    /// An image effect: a descriptor or an instance.
    OfxImageEffectHandle,
    OfxImageEffectStruct
);
handle!(
    /// One of an image effect's clips.
    OfxImageClipHandle,
    OfxImageClipStruct
);
handle!(
    /// Memory from the image effect suite's allocator.
    OfxImageMemoryHandle,
    OfxImageMemoryStruct
);
handle!(
    /// One parameter.
    OfxParamHandle,
    OfxParamStruct
);
handle!(
    /// An effect's parameters.
    OfxParamSetHandle,
    OfxParamSetStruct
);
handle!(
    /// A mutex of the multi-thread suite.
    OfxMutexHandle,
    OfxMutex
);
handle!(
    /// An interact (an overlay or a parameter's own interface).
    OfxInteractHandle,
    OfxInteract
);
handle!(
    /// What the draw suite draws on.
    OfxDrawContextHandle,
    OfxDrawContext
);

/// A plug-in's entry point: every action goes through it.
pub type OfxPluginEntryPoint = unsafe extern "C" fn(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus;

/// What a host gives its plug-ins.
#[repr(C)]
#[derive(Debug)]
pub struct OfxHost {
    /// The host's properties.
    pub host: OfxPropertySetHandle,
    /// A suite by name and version, or null.
    pub fetch_suite: unsafe extern "C" fn(
        host: OfxPropertySetHandle,
        suite_name: *const c_char,
        suite_version: c_int,
    ) -> *const c_void,
}

/// One plug-in of a binary.
#[repr(C)]
#[derive(Debug)]
pub struct OfxPlugin {
    /// `names::kOfxImageEffectPluginApi` for image effects.
    pub plugin_api: *const c_char,
    pub api_version: c_int,
    /// Unique, reverse domain name style ("com.example.Blur").
    pub plugin_identifier: *const c_char,
    pub plugin_version_major: c_uint,
    pub plugin_version_minor: c_uint,
    pub set_host: Option<unsafe extern "C" fn(host: *mut OfxHost)>,
    pub main_entry: Option<OfxPluginEntryPoint>,
}

/// `OfxGetNumberOfPlugins`, exported by every binary.
pub type OfxGetNumberOfPluginsFn = unsafe extern "C" fn() -> c_int;
/// `OfxGetPlugin`, exported by every binary.
pub type OfxGetPluginFn = unsafe extern "C" fn(nth: c_int) -> *mut OfxPlugin;
/// `OfxSetHost` (OpenFX 1.5), exported by some binaries: called first.
pub type OfxSetHostFn = unsafe extern "C" fn(host: *const OfxHost) -> OfxStatus;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OfxRangeI {
    pub min: c_int,
    pub max: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OfxRangeD {
    pub min: c_double,
    pub max: c_double,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OfxPointI {
    pub x: c_int,
    pub y: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OfxPointD {
    pub x: c_double,
    pub y: c_double,
}

/// A rectangle in pixels: `x1`, `y1` inclusive, `x2`, `y2` exclusive.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OfxRectI {
    pub x1: c_int,
    pub y1: c_int,
    pub x2: c_int,
    pub y2: c_int,
}

/// A rectangle in canonical coordinates.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OfxRectD {
    pub x1: c_double,
    pub y1: c_double,
    pub x2: c_double,
    pub y2: c_double,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OfxRGBAColourF {
    pub r: c_float,
    pub g: c_float,
    pub b: c_float,
    pub a: c_float,
}

/// `OfxBytes` (a parameter of bytes).
#[repr(C)]
#[derive(Debug)]
pub struct OfxBytes {
    pub data: *const u8,
    pub length: usize,
}

/// The property suite (`kOfxPropertySuite`, version 1).
#[repr(C)]
#[derive(Debug)]
pub struct OfxPropertySuiteV1 {
    pub prop_set_pointer: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: *mut c_void,
    ) -> OfxStatus,
    pub prop_set_string: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: *const c_char,
    ) -> OfxStatus,
    pub prop_set_double: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: c_double,
    ) -> OfxStatus,
    pub prop_set_int: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: c_int,
    ) -> OfxStatus,
    pub prop_set_pointer_n: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *const *mut c_void,
    ) -> OfxStatus,
    pub prop_set_string_n: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *const *const c_char,
    ) -> OfxStatus,
    pub prop_set_double_n: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *const c_double,
    ) -> OfxStatus,
    pub prop_set_int_n: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *const c_int,
    ) -> OfxStatus,
    pub prop_get_pointer: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: *mut *mut c_void,
    ) -> OfxStatus,
    pub prop_get_string: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: *mut *mut c_char,
    ) -> OfxStatus,
    pub prop_get_double: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: *mut c_double,
    ) -> OfxStatus,
    pub prop_get_int: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: *mut c_int,
    ) -> OfxStatus,
    pub prop_get_pointer_n: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *mut *mut c_void,
    ) -> OfxStatus,
    pub prop_get_string_n: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *mut *mut c_char,
    ) -> OfxStatus,
    pub prop_get_double_n: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *mut c_double,
    ) -> OfxStatus,
    pub prop_get_int_n: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *mut c_int,
    ) -> OfxStatus,
    pub prop_reset: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
    ) -> OfxStatus,
    pub prop_get_dimension: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: *mut c_int,
    ) -> OfxStatus,
}

/// The image effect suite (`kOfxImageEffectSuite`, version 1).
#[repr(C)]
#[derive(Debug)]
pub struct OfxImageEffectSuiteV1 {
    pub get_property_set: unsafe extern "C" fn(
        image_effect: OfxImageEffectHandle,
        prop_handle: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    pub get_param_set: unsafe extern "C" fn(
        image_effect: OfxImageEffectHandle,
        param_set: *mut OfxParamSetHandle,
    ) -> OfxStatus,
    pub clip_define: unsafe extern "C" fn(
        image_effect: OfxImageEffectHandle,
        name: *const c_char,
        property_set: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    pub clip_get_handle: unsafe extern "C" fn(
        image_effect: OfxImageEffectHandle,
        name: *const c_char,
        clip: *mut OfxImageClipHandle,
        property_set: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    pub clip_get_property_set: unsafe extern "C" fn(
        clip: OfxImageClipHandle,
        prop_handle: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    pub clip_get_image: unsafe extern "C" fn(
        clip: OfxImageClipHandle,
        time: OfxTime,
        region: *const OfxRectD,
        image_handle: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    pub clip_release_image: unsafe extern "C" fn(image_handle: OfxPropertySetHandle) -> OfxStatus,
    pub clip_get_region_of_definition: unsafe extern "C" fn(
        clip: OfxImageClipHandle,
        time: OfxTime,
        bounds: *mut OfxRectD,
    ) -> OfxStatus,
    pub abort: unsafe extern "C" fn(image_effect: OfxImageEffectHandle) -> c_int,
    pub image_memory_alloc: unsafe extern "C" fn(
        instance_handle: OfxImageEffectHandle,
        n_bytes: usize,
        memory_handle: *mut OfxImageMemoryHandle,
    ) -> OfxStatus,
    pub image_memory_free: unsafe extern "C" fn(memory_handle: OfxImageMemoryHandle) -> OfxStatus,
    pub image_memory_lock: unsafe extern "C" fn(
        memory_handle: OfxImageMemoryHandle,
        returned_ptr: *mut *mut c_void,
    ) -> OfxStatus,
    pub image_memory_unlock: unsafe extern "C" fn(memory_handle: OfxImageMemoryHandle) -> OfxStatus,
}

/// The parameter suite (`kOfxParameterSuite`, version 1). The value
/// functions are variadic: their values' count and types follow the
/// parameter's type.
#[repr(C)]
#[derive(Debug)]
pub struct OfxParameterSuiteV1 {
    pub param_define: unsafe extern "C" fn(
        param_set: OfxParamSetHandle,
        param_type: *const c_char,
        name: *const c_char,
        property_set: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    pub param_get_handle: unsafe extern "C" fn(
        param_set: OfxParamSetHandle,
        name: *const c_char,
        param: *mut OfxParamHandle,
        property_set: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    pub param_set_get_property_set: unsafe extern "C" fn(
        param_set: OfxParamSetHandle,
        prop_handle: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    pub param_get_property_set: unsafe extern "C" fn(
        param: OfxParamHandle,
        prop_handle: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    pub param_get_value: unsafe extern "C" fn(param_handle: OfxParamHandle, ...) -> OfxStatus,
    pub param_get_value_at_time:
        unsafe extern "C" fn(param_handle: OfxParamHandle, time: OfxTime, ...) -> OfxStatus,
    pub param_get_derivative:
        unsafe extern "C" fn(param_handle: OfxParamHandle, time: OfxTime, ...) -> OfxStatus,
    pub param_get_integral: unsafe extern "C" fn(
        param_handle: OfxParamHandle,
        time1: OfxTime,
        time2: OfxTime,
        ...
    ) -> OfxStatus,
    pub param_set_value: unsafe extern "C" fn(param_handle: OfxParamHandle, ...) -> OfxStatus,
    pub param_set_value_at_time:
        unsafe extern "C" fn(param_handle: OfxParamHandle, time: OfxTime, ...) -> OfxStatus,
    pub param_get_num_keys: unsafe extern "C" fn(
        param_handle: OfxParamHandle,
        number_of_keys: *mut c_uint,
    ) -> OfxStatus,
    pub param_get_key_time: unsafe extern "C" fn(
        param_handle: OfxParamHandle,
        nth_key: c_uint,
        time: *mut OfxTime,
    ) -> OfxStatus,
    pub param_get_key_index: unsafe extern "C" fn(
        param_handle: OfxParamHandle,
        time: OfxTime,
        direction: c_int,
        index: *mut c_int,
    ) -> OfxStatus,
    pub param_delete_key:
        unsafe extern "C" fn(param_handle: OfxParamHandle, time: OfxTime) -> OfxStatus,
    pub param_delete_all_keys: unsafe extern "C" fn(param_handle: OfxParamHandle) -> OfxStatus,
    pub param_copy: unsafe extern "C" fn(
        param_to: OfxParamHandle,
        param_from: OfxParamHandle,
        dst_offset: OfxTime,
        frame_range: *const OfxRangeD,
    ) -> OfxStatus,
    pub param_edit_begin:
        unsafe extern "C" fn(param_set: OfxParamSetHandle, name: *const c_char) -> OfxStatus,
    pub param_edit_end: unsafe extern "C" fn(param_set: OfxParamSetHandle) -> OfxStatus,
}

/// A custom parameter's interpolation (`kOfxParamPropCustomInterpCallbackV1`).
pub type OfxCustomParamInterpFuncV1 = unsafe extern "C" fn(
    instance: OfxParamSetHandle,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus;

/// The parametric parameter suite (`kOfxParametricParameterSuite`, version 1).
#[repr(C)]
#[derive(Debug)]
pub struct OfxParametricParameterSuiteV1 {
    pub parametric_param_get_value: unsafe extern "C" fn(
        param: OfxParamHandle,
        curve_index: c_int,
        time: OfxTime,
        parametric_position: c_double,
        return_value: *mut c_double,
    ) -> OfxStatus,
    pub parametric_param_get_n_control_points: unsafe extern "C" fn(
        param: OfxParamHandle,
        curve_index: c_int,
        time: c_double,
        return_value: *mut c_int,
    ) -> OfxStatus,
    pub parametric_param_get_nth_control_point: unsafe extern "C" fn(
        param: OfxParamHandle,
        curve_index: c_int,
        time: c_double,
        nth_ctl: c_int,
        key: *mut c_double,
        value: *mut c_double,
    ) -> OfxStatus,
    pub parametric_param_set_nth_control_point: unsafe extern "C" fn(
        param: OfxParamHandle,
        curve_index: c_int,
        time: c_double,
        nth_ctl: c_int,
        key: c_double,
        value: c_double,
        add_animation_key: bool,
    ) -> OfxStatus,
    pub parametric_param_add_control_point: unsafe extern "C" fn(
        param: OfxParamHandle,
        curve_index: c_int,
        time: c_double,
        key: c_double,
        value: c_double,
        add_animation_key: bool,
    ) -> OfxStatus,
    pub parametric_param_delete_control_point: unsafe extern "C" fn(
        param: OfxParamHandle,
        curve_index: c_int,
        nth_ctl: c_int,
    ) -> OfxStatus,
    pub parametric_param_delete_all_control_points:
        unsafe extern "C" fn(param: OfxParamHandle, curve_index: c_int) -> OfxStatus,
}

/// The memory suite (`kOfxMemorySuite`, version 1).
#[repr(C)]
#[derive(Debug)]
pub struct OfxMemorySuiteV1 {
    pub memory_alloc: unsafe extern "C" fn(
        handle: *mut c_void,
        n_bytes: usize,
        allocated_data: *mut *mut c_void,
    ) -> OfxStatus,
    pub memory_free: unsafe extern "C" fn(allocated_data: *mut c_void) -> OfxStatus,
}

/// A function the multi-thread suite runs on its threads.
pub type OfxThreadFunctionV1 =
    unsafe extern "C" fn(thread_index: c_uint, thread_max: c_uint, custom_arg: *mut c_void);

/// The multi-thread suite (`kOfxMultiThreadSuite`, version 1).
#[repr(C)]
#[derive(Debug)]
pub struct OfxMultiThreadSuiteV1 {
    pub multi_thread: unsafe extern "C" fn(
        func: OfxThreadFunctionV1,
        n_threads: c_uint,
        custom_arg: *mut c_void,
    ) -> OfxStatus,
    pub multi_thread_num_cpus: unsafe extern "C" fn(n_cpus: *mut c_uint) -> OfxStatus,
    pub multi_thread_index: unsafe extern "C" fn(thread_index: *mut c_uint) -> OfxStatus,
    pub multi_thread_is_spawned_thread: unsafe extern "C" fn() -> c_int,
    pub mutex_create:
        unsafe extern "C" fn(mutex: *mut OfxMutexHandle, lock_count: c_int) -> OfxStatus,
    pub mutex_destroy: unsafe extern "C" fn(mutex: OfxMutexHandle) -> OfxStatus,
    pub mutex_lock: unsafe extern "C" fn(mutex: OfxMutexHandle) -> OfxStatus,
    pub mutex_unlock: unsafe extern "C" fn(mutex: OfxMutexHandle) -> OfxStatus,
    pub mutex_try_lock: unsafe extern "C" fn(mutex: OfxMutexHandle) -> OfxStatus,
}

/// The message suite (`kOfxMessageSuite`, version 1): `format` is a
/// `printf` format, its values follow.
#[repr(C)]
#[derive(Debug)]
pub struct OfxMessageSuiteV1 {
    pub message: unsafe extern "C" fn(
        handle: *mut c_void,
        message_type: *const c_char,
        message_id: *const c_char,
        format: *const c_char,
        ...
    ) -> OfxStatus,
}

/// The message suite, version 2: messages that stay on an effect.
#[repr(C)]
#[derive(Debug)]
pub struct OfxMessageSuiteV2 {
    pub message: unsafe extern "C" fn(
        handle: *mut c_void,
        message_type: *const c_char,
        message_id: *const c_char,
        format: *const c_char,
        ...
    ) -> OfxStatus,
    pub set_persistent_message: unsafe extern "C" fn(
        handle: *mut c_void,
        message_type: *const c_char,
        message_id: *const c_char,
        format: *const c_char,
        ...
    ) -> OfxStatus,
    pub clear_persistent_message: unsafe extern "C" fn(handle: *mut c_void) -> OfxStatus,
}

/// The progress suite (`kOfxProgressSuite`, version 1).
#[repr(C)]
#[derive(Debug)]
pub struct OfxProgressSuiteV1 {
    pub progress_start:
        unsafe extern "C" fn(effect_instance: *mut c_void, label: *const c_char) -> OfxStatus,
    pub progress_update:
        unsafe extern "C" fn(effect_instance: *mut c_void, progress: c_double) -> OfxStatus,
    pub progress_end: unsafe extern "C" fn(effect_instance: *mut c_void) -> OfxStatus,
}

/// The progress suite, version 2: the label has an identifier.
#[repr(C)]
#[derive(Debug)]
pub struct OfxProgressSuiteV2 {
    pub progress_start: unsafe extern "C" fn(
        effect_instance: *mut c_void,
        message: *const c_char,
        message_id: *const c_char,
    ) -> OfxStatus,
    pub progress_update:
        unsafe extern "C" fn(effect_instance: *mut c_void, progress: c_double) -> OfxStatus,
    pub progress_end: unsafe extern "C" fn(effect_instance: *mut c_void) -> OfxStatus,
}

/// The time line suite (`kOfxTimeLineSuite`, version 1).
#[repr(C)]
#[derive(Debug)]
pub struct OfxTimeLineSuiteV1 {
    pub get_time: unsafe extern "C" fn(instance: *mut c_void, time: *mut c_double) -> OfxStatus,
    pub goto_time: unsafe extern "C" fn(instance: *mut c_void, time: c_double) -> OfxStatus,
    pub get_time_bounds: unsafe extern "C" fn(
        instance: *mut c_void,
        first_time: *mut c_double,
        last_time: *mut c_double,
    ) -> OfxStatus,
}

/// The interact suite (`kOfxInteractSuite`, version 1).
#[repr(C)]
#[derive(Debug)]
pub struct OfxInteractSuiteV1 {
    pub interact_swap_buffers: unsafe extern "C" fn(interact: OfxInteractHandle) -> OfxStatus,
    pub interact_redraw: unsafe extern "C" fn(interact: OfxInteractHandle) -> OfxStatus,
    pub interact_get_property_set: unsafe extern "C" fn(
        interact: OfxInteractHandle,
        property: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
}

/// `OfxStandardColour` of the draw suite.
pub type OfxStandardColour = c_int;
/// `OfxDrawLineStipplePattern` of the draw suite.
pub type OfxDrawLineStipplePattern = c_int;
/// `OfxDrawPrimitive` of the draw suite.
pub type OfxDrawPrimitive = c_int;

pub const kOfxStandardColourOverlayBackground: OfxStandardColour = 0;
pub const kOfxStandardColourOverlayActive: OfxStandardColour = 1;
pub const kOfxStandardColourOverlaySelected: OfxStandardColour = 2;
pub const kOfxStandardColourOverlayDeselected: OfxStandardColour = 3;
pub const kOfxStandardColourOverlayMarqueeFG: OfxStandardColour = 4;
pub const kOfxStandardColourOverlayMarqueeBG: OfxStandardColour = 5;
pub const kOfxStandardColourOverlayText: OfxStandardColour = 6;

pub const kOfxDrawPrimitiveLines: OfxDrawPrimitive = 0;
pub const kOfxDrawPrimitiveLineStrip: OfxDrawPrimitive = 1;
pub const kOfxDrawPrimitiveLineLoop: OfxDrawPrimitive = 2;
pub const kOfxDrawPrimitiveRectangle: OfxDrawPrimitive = 3;
pub const kOfxDrawPrimitivePolygon: OfxDrawPrimitive = 4;
pub const kOfxDrawPrimitiveEllipse: OfxDrawPrimitive = 5;

pub const kOfxDrawTextAlignmentLeft: c_int = 0x0001;
pub const kOfxDrawTextAlignmentRight: c_int = 0x0002;
pub const kOfxDrawTextAlignmentTop: c_int = 0x0004;
pub const kOfxDrawTextAlignmentBottom: c_int = 0x0008;
pub const kOfxDrawTextAlignmentBaseline: c_int = 0x0010;

/// The draw suite (`kOfxDrawSuite`, version 1): overlays drawn without
/// OpenGL.
#[repr(C)]
#[derive(Debug)]
pub struct OfxDrawSuiteV1 {
    pub get_colour: unsafe extern "C" fn(
        context: OfxDrawContextHandle,
        std_colour: OfxStandardColour,
        colour: *mut OfxRGBAColourF,
    ) -> OfxStatus,
    pub set_colour: unsafe extern "C" fn(
        context: OfxDrawContextHandle,
        colour: *const OfxRGBAColourF,
    ) -> OfxStatus,
    pub set_line_width:
        unsafe extern "C" fn(context: OfxDrawContextHandle, width: c_float) -> OfxStatus,
    pub set_line_stipple: unsafe extern "C" fn(
        context: OfxDrawContextHandle,
        pattern: OfxDrawLineStipplePattern,
    ) -> OfxStatus,
    pub draw: unsafe extern "C" fn(
        context: OfxDrawContextHandle,
        primitive: OfxDrawPrimitive,
        points: *const OfxPointD,
        point_count: c_int,
    ) -> OfxStatus,
    pub draw_text: unsafe extern "C" fn(
        context: OfxDrawContextHandle,
        text: *const c_char,
        pos: *const OfxPointD,
        alignment: c_int,
    ) -> OfxStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_structures_have_the_c_layout() {
        let ptr = size_of::<*const c_void>();
        // Two pointers, two ints and two function pointers.
        assert_eq!(size_of::<OfxPlugin>(), 2 * ptr + 2 * 4 + 4 + 4 + 2 * ptr);
        assert_eq!(size_of::<OfxHost>(), 2 * ptr);
        assert_eq!(size_of::<OfxRectD>(), 32);
        assert_eq!(size_of::<OfxRectI>(), 16);
        assert_eq!(size_of::<OfxPropertySuiteV1>(), 18 * ptr);
        assert_eq!(size_of::<OfxImageEffectSuiteV1>(), 13 * ptr);
        assert_eq!(size_of::<OfxParameterSuiteV1>(), 18 * ptr);
        assert_eq!(size_of::<OfxMultiThreadSuiteV1>(), 9 * ptr);
        assert_eq!(size_of::<OfxDrawSuiteV1>(), 6 * ptr);
    }

    #[test]
    fn names_match_the_specification() {
        assert_eq!(names::kOfxImageEffectPluginApi, c"OfxImageEffectPluginAPI");
        assert_eq!(names::kOfxStatReplyDefault, 14);
        assert_eq!(names::kOfxActionDescribeInteract, names::kOfxActionDescribe);
    }
}
