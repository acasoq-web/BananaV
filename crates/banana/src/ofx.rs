//! The pack's effects as OpenFX plug-ins: one entry point each, all the
//! same code ([`main`]), told which effect by its place in the pack.
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr::null_mut;
use std::sync::OnceLock;

use bv_effects::host::{
    Fx as Host, Image, Param as HostParam, Props, for_chunks, is, put_pixel, read_pixel, rgba_row,
    set_host, suites,
};
use bv_ofx_sys::extensions::{kBananaVParamPropLogarithmic, kBananaVParamPropMaskTracking};
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::cpu::{Picture, run};
use crate::fx::{Frame, Fx, Kind, POINTS, Param, Shape, V, common};
use crate::overlay;

/// The name BananaV gives the binary the pack is built into.
pub const BINARY: &str = "Banana Split";

/// The identifiers of the pack's plug-ins start with it.
pub const ID_PREFIX: &str = "org.bananav.bananasplit.";

/// The pack's grouping, before each group's own name.
pub const GROUPING: &str = "Banana Split";

/// The effect whose identifier is `id`.
pub fn by_id(id: &str) -> Option<&'static Fx> {
    crate::find(id.strip_prefix(ID_PREFIX)?).map(|(_, f)| f)
}

/// The most effects the entry points below have room for.
const ROOM: usize = 20 * 16;

unsafe extern "C" fn entry<const I: usize>(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    match crate::all().get(I) {
        Some(fx) => main(fx, action, handle, in_args, out_args),
        None => kOfxStatErrBadIndex,
    }
}

unsafe extern "C" fn overlay_entry<const I: usize>(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    match crate::all().get(I) {
        Some(fx) => overlay::main(fx, action, handle, in_args),
        None => kOfxStatErrBadIndex,
    }
}

macro_rules! sixteen {
    ($f:ident, $h:literal) => {
        [
            $f::<{ $h * 16 }>,
            $f::<{ $h * 16 + 1 }>,
            $f::<{ $h * 16 + 2 }>,
            $f::<{ $h * 16 + 3 }>,
            $f::<{ $h * 16 + 4 }>,
            $f::<{ $h * 16 + 5 }>,
            $f::<{ $h * 16 + 6 }>,
            $f::<{ $h * 16 + 7 }>,
            $f::<{ $h * 16 + 8 }>,
            $f::<{ $h * 16 + 9 }>,
            $f::<{ $h * 16 + 10 }>,
            $f::<{ $h * 16 + 11 }>,
            $f::<{ $h * 16 + 12 }>,
            $f::<{ $h * 16 + 13 }>,
            $f::<{ $h * 16 + 14 }>,
            $f::<{ $h * 16 + 15 }>,
        ]
    };
}

macro_rules! every {
    ($f:ident) => {
        [
            sixteen!($f, 0),
            sixteen!($f, 1),
            sixteen!($f, 2),
            sixteen!($f, 3),
            sixteen!($f, 4),
            sixteen!($f, 5),
            sixteen!($f, 6),
            sixteen!($f, 7),
            sixteen!($f, 8),
            sixteen!($f, 9),
            sixteen!($f, 10),
            sixteen!($f, 11),
            sixteen!($f, 12),
            sixteen!($f, 13),
            sixteen!($f, 14),
            sixteen!($f, 15),
            sixteen!($f, 16),
            sixteen!($f, 17),
            sixteen!($f, 18),
            sixteen!($f, 19),
        ]
    };
}

static ENTRIES: [[OfxPluginEntryPoint; 16]; 20] = every!(entry);
static OVERLAYS: [[OfxPluginEntryPoint; 16]; 20] = every!(overlay_entry);

struct Plugins(Vec<OfxPlugin>, #[allow(dead_code)] Vec<CString>);

// SAFETY: made once, never changed; hosts only read them.
unsafe impl Send for Plugins {}
// SAFETY: as above.
unsafe impl Sync for Plugins {}

fn plugins() -> &'static Plugins {
    static ALL: OnceLock<Plugins> = OnceLock::new();
    ALL.get_or_init(|| {
        let ids: Vec<CString> = crate::all()
            .iter()
            .take(ROOM)
            .filter_map(|f| CString::new(format!("{ID_PREFIX}{}", f.id)).ok())
            .collect();
        let list = ids
            .iter()
            .enumerate()
            .map(|(i, id)| OfxPlugin {
                plugin_api: kOfxImageEffectPluginApi.as_ptr(),
                api_version: 1,
                plugin_identifier: id.as_ptr(),
                plugin_version_major: 1,
                plugin_version_minor: 0,
                set_host: Some(set_host),
                main_entry: Some(ENTRIES[i / 16][i % 16]),
            })
            .collect();
        Plugins(list, ids)
    })
}

/// `OfxGetNumberOfPlugins` of the pack.
///
/// # Safety
/// Called by hosts, as the specification says.
pub unsafe extern "C" fn number_of_plugins() -> c_int {
    plugins().0.len() as c_int
}

/// `OfxGetPlugin` of the pack.
///
/// # Safety
/// Called by hosts, as the specification says.
pub unsafe extern "C" fn get_plugin(nth: c_int) -> *mut OfxPlugin {
    usize::try_from(nth)
        .ok()
        .and_then(|i| plugins().0.get(i))
        .map_or(null_mut(), |p| p as *const OfxPlugin as *mut OfxPlugin)
}

/// A text as C has it (nothing after a zero inside it).
pub(crate) fn c(text: &str) -> CString {
    CString::new(text.split('\0').next().unwrap_or("")).unwrap_or_default()
}

fn main(
    fx: &'static Fx,
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if suites().is_none() {
        return kOfxStatErrMissingHostFeature;
    }
    let host = Host(handle as OfxImageEffectHandle);
    let (args, out) = (Props(in_args), Props(out_args));
    if is(action, kOfxActionDescribe) {
        describe(fx, host);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        let generator = args.is(kOfxImageEffectPropContext, kOfxImageEffectContextGenerator);
        describe_in_context(fx, host, generator);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        render(fx, host, args)
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // Pixels come from anywhere: all of the pictures.
        let (origin, size) = host.frame();
        let all = [
            origin[0],
            origin[1],
            origin[0] + size[0],
            origin[1] + size[1],
        ];
        if fx.kind == Kind::Transition {
            out.set_dbls(c"OfxImageClipPropRoI_SourceFrom", &all);
            out.set_dbls(c"OfxImageClipPropRoI_SourceTo", &all);
        } else {
            out.set_dbls(c"OfxImageClipPropRoI_Source", &all);
        }
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionGetRegionOfDefinition) && fx.kind == Kind::Generator {
        let (origin, size) = host.frame();
        out.set_dbls(
            kOfxImageEffectPropRegionOfDefinition,
            &[
                origin[0],
                origin[1],
                origin[0] + size[0],
                origin[1] + size[1],
            ],
        );
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionGetClipPreferences) && fx.moves {
        out.set_int(kOfxImageEffectFrameVarying, 1);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionGetFramesNeeded) && fx.is_temporal() {
        let time = args.dbl(kOfxPropTime, 0);
        let v = values(fx, host, time);
        let times = fx.times(&v, time);
        let low = times.iter().copied().fold(time, f64::min);
        let high = times.iter().copied().fold(time, f64::max);
        out.set_dbls(c"OfxImageClipPropFrameRange_Source", &[low, high]);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        let time = args.dbl(kOfxPropTime, 0);
        if fx.kind == Kind::Transition {
            // Its ends are the pictures themselves.
            let t = host.param(kOfxImageEffectTransitionParamName).f64_at(time);
            if t <= 0.0 {
                out.set_str(kOfxPropName, kOfxImageEffectTransitionSourceFromClipName);
                return kOfxStatOK;
            }
            if t >= 1.0 {
                out.set_str(kOfxPropName, kOfxImageEffectTransitionSourceToClipName);
                return kOfxStatOK;
            }
            return kOfxStatReplyDefault;
        }
        if fx.is_identity(&values(fx, host, time)) {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxActionInstanceChanged) {
        let time = args.dbl(kOfxPropTime, 0);
        if args.is(kOfxPropName, &c(common::POINTS))
            && args.is(kOfxPropChangeReason, kOfxChangeUserEdited)
        {
            place_points(fx, host, time);
        }
        show(fx, host, time);
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) || is(action, kOfxActionBeginInstanceEdit) {
        show(fx, host, 0.0);
        kOfxStatOK
    } else if is(action, kOfxActionLoad)
        || is(action, kOfxActionUnload)
        || is(action, kOfxActionDestroyInstance)
    {
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

/// A path's places asked for with the number alone: between the last one
/// and the path's end.
fn place_points(fx: &Fx, host: Host, time: f64) {
    let Some((_, to)) = overlay::path_of(fx) else {
        return;
    };
    let v = values(fx, host, time);
    let (n, placed) = (v.between().len(), v.i(common::PLACED).max(0) as usize);
    if placed >= n {
        return;
    }
    let before = if placed == 0 {
        overlay::path_of(fx).map_or([0.5; 2], |(from, _)| v.xy(from))
    } else {
        v.xy(POINTS[placed - 1])
    };
    let end = v.xy(to);
    for (k, name) in POINTS.into_iter().enumerate().take(n).skip(placed) {
        // One given a place without the number (an agent's) keeps it.
        let first = fx.params.iter().find(|p| p.name == name);
        let at = v.xy(name);
        if first.is_some_and(|p| [p.default[0], p.default[1]] != at) {
            continue;
        }
        let t = (k - placed + 1) as f64 / (n - placed + 1) as f64;
        let p = host.param(&c(name));
        p.delete_keys();
        p.set_xy([
            before[0] + (end[0] - before[0]) * t,
            before[1] + (end[1] - before[1]) * t,
        ]);
    }
    host.param(&c(common::PLACED)).set_int(n as c_int);
}

/// The mask's settings are greyed out while there is no mask, and a path's
/// places not in use are not shown.
fn show(fx: &Fx, host: Host, time: f64) {
    if overlay::path_of(fx).is_some() {
        let n = host.param(&c(common::POINTS)).int_at(time);
        for (k, name) in POINTS.into_iter().enumerate() {
            host.param(&c(name)).set_secret(k as c_int >= n);
        }
        host.param(&c(common::CURVE)).set_enabled(n > 0);
        if fx.params.iter().any(|p| p.name == "hold") {
            host.param(&c("hold")).set_enabled(n > 0);
        }
    }
    if !fx.params.iter().any(|p| p.name == common::MASK) {
        return;
    }
    let on = host.param(&c(common::MASK)).int_at(time) > 0;
    for name in [
        common::MASK_CENTER,
        common::MASK_WIDTH,
        common::MASK_HEIGHT,
        common::MASK_ANGLE,
        common::MASK_FEATHER,
        common::MASK_INVERT,
        common::MASK_SHOW,
        common::MASK_TRACK,
    ] {
        host.param(&c(name)).set_enabled(on);
    }
}

fn describe(fx: &'static Fx, host: Host) {
    let p = host.props();
    p.set_str(kOfxPropLabel, &c(fx.name));
    p.set_str(
        kOfxImageEffectPluginPropGrouping,
        &c(&format!("{GROUPING}/{}", fx.group.name())),
    );
    p.set_str(kOfxPropPluginDescription, &c(fx.about));
    let contexts: &[&CStr] = match fx.kind {
        Kind::Filter => &[kOfxImageEffectContextFilter],
        Kind::Generator => &[
            kOfxImageEffectContextGenerator,
            kOfxImageEffectContextFilter,
        ],
        Kind::Transition => &[kOfxImageEffectContextTransition],
    };
    p.set_strs(kOfxImageEffectPropSupportedContexts, contexts);
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    p.set_int(kOfxImageEffectPropSupportsTiles, 0);
    p.set_int(kOfxImageEffectPropSupportsMultiResolution, 1);
    p.set_int(
        kOfxImageEffectPropTemporalClipAccess,
        c_int::from(fx.is_temporal()),
    );
    p.set_int(kOfxImageEffectPluginPropHostFrameThreading, 0);
    p.set_str(
        kOfxImageEffectPluginRenderThreadSafety,
        kOfxImageEffectRenderFullySafe,
    );
    if overlay::has(fx)
        && let Some(i) = crate::find(fx.id).map(|(i, _)| i)
        && i < ROOM
    {
        p.set_ptr(
            kOfxImageEffectPluginPropOverlayInteractV2,
            OVERLAYS[i / 16][i % 16] as *mut c_void,
        );
    }
}

fn describe_in_context(fx: &Fx, host: Host, generator: bool) {
    let comps = [kOfxImageComponentRGBA];
    match fx.kind {
        Kind::Transition => {
            host.define_clip(kOfxImageEffectTransitionSourceFromClipName, &comps);
            host.define_clip(kOfxImageEffectTransitionSourceToClipName, &comps);
        }
        _ if generator => {}
        _ => {
            let clip = host.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
            clip.set_int(
                kOfxImageEffectPropTemporalClipAccess,
                c_int::from(fx.is_temporal()),
            );
        }
    }
    host.define_clip(kOfxImageEffectOutputClipName, &comps);
    if fx.kind == Kind::Transition {
        let p = host.define_param(
            kOfxParamTypeDouble,
            kOfxImageEffectTransitionParamName,
            c"Transition",
        );
        p.set_dbl(kOfxParamPropMin, 0.0);
        p.set_dbl(kOfxParamPropMax, 1.0);
    }
    let mut section: Option<CString> = None;
    // A generated media has nothing under it to lie on, or to mix with.
    let mut skipped = false;
    for p in &fx.params {
        match p.shape {
            Shape::Section(open) => {
                skipped = generator && matches!(p.name, "compose" | "mixing" | "tracking");
                if skipped {
                    continue;
                }
                let g = host.define_param(kOfxParamTypeGroup, &c(p.name), &c(p.label));
                g.set_int(kOfxParamPropGroupOpen, c_int::from(open));
                section = Some(c(p.name));
            }
            Shape::End => {
                skipped = false;
                section = None;
            }
            _ if skipped => {}
            _ => {
                let d = define(host, p);
                if !p.hint.is_empty() {
                    d.set_str(kOfxParamPropHint, &c(p.hint));
                }
                if p.still {
                    d.set_int(kOfxParamPropAnimates, 0);
                }
                if p.hidden {
                    d.set_int(kOfxParamPropSecret, 1);
                }
                if let Some(s) = &section {
                    d.set_str(kOfxParamPropParent, s);
                }
            }
        }
    }
}

/// How many digits a number between `lo` and `hi` shows.
fn digits(lo: f64, hi: f64) -> c_int {
    let wide = hi - lo;
    if wide >= 50.0 {
        1
    } else if wide >= 5.0 {
        2
    } else {
        3
    }
}

fn define(host: Host, p: &Param) -> Props {
    let (name, label) = (c(p.name), c(p.label));
    match p.shape {
        Shape::Num { hard, show } => {
            let d = host.define_param(kOfxParamTypeDouble, &name, &label);
            d.set_dbl(kOfxParamPropDefault, p.default[0]);
            d.set_dbl(kOfxParamPropMin, hard[0]);
            d.set_dbl(kOfxParamPropMax, hard[1]);
            d.set_dbl(kOfxParamPropDisplayMin, show[0]);
            d.set_dbl(kOfxParamPropDisplayMax, show[1]);
            d.set_dbl(kOfxParamPropIncrement, (show[1] - show[0]) / 100.0);
            d.set_int(kOfxParamPropDigits, digits(show[0], show[1]));
            // Scales and sizes whose ends are far apart.
            if show[0] > 0.0 && show[1] / show[0] >= 50.0 {
                d.set_int(kBananaVParamPropLogarithmic, 1);
            }
            d
        }
        Shape::Angle => {
            let d = host.define_param(kOfxParamTypeDouble, &name, &label);
            d.set_str(kOfxParamPropDoubleType, kOfxParamDoubleTypeAngle);
            d.set_dbl(kOfxParamPropDefault, p.default[0]);
            d.set_dbl(kOfxParamPropDisplayMin, -180.0);
            d.set_dbl(kOfxParamPropDisplayMax, 180.0);
            d.set_dbl(kOfxParamPropIncrement, 1.0);
            d.set_int(kOfxParamPropDigits, 1);
            d
        }
        Shape::Int { lo, hi } => {
            let d = host.define_param(kOfxParamTypeInteger, &name, &label);
            d.set_int(kOfxParamPropDefault, p.default[0] as c_int);
            d.set_int(kOfxParamPropMin, lo);
            d.set_int(kOfxParamPropMax, hi);
            d.set_int(kOfxParamPropDisplayMin, lo);
            d.set_int(kOfxParamPropDisplayMax, hi.min(lo.saturating_add(100)));
            d
        }
        Shape::Choice(options) => {
            let d = host.define_param(kOfxParamTypeChoice, &name, &label);
            let o: Vec<CString> = options.iter().map(|o| c(o)).collect();
            let refs: Vec<&CStr> = o.iter().map(CString::as_c_str).collect();
            d.set_strs(kOfxParamPropChoiceOption, &refs);
            d.set_int(kOfxParamPropDefault, p.default[0] as c_int);
            d
        }
        Shape::Toggle => {
            let d = host.define_param(kOfxParamTypeBoolean, &name, &label);
            d.set_int(kOfxParamPropDefault, c_int::from(p.default[0] >= 0.5));
            d
        }
        Shape::Point => {
            let d = host.define_param(kOfxParamTypeDouble2D, &name, &label);
            d.set_str(
                kOfxParamPropDoubleType,
                kOfxParamDoubleTypeNormalisedXYAbsolute,
            );
            d.set_dbls(kOfxParamPropDefault, &p.default[..2]);
            d
        }
        Shape::Color => {
            let d = host.define_param(kOfxParamTypeRGB, &name, &label);
            d.set_dbls(kOfxParamPropDefault, &p.default);
            d
        }
        Shape::Button(spec) => {
            let d = host.define_param(kOfxParamTypePushButton, &name, &label);
            d.set_str(kBananaVParamPropMaskTracking, &c(spec));
            d
        }
        Shape::Section(_) | Shape::End => host.define_param(kOfxParamTypeGroup, &name, &label),
    }
}

/// A setting's value as the host has it at `time`.
fn read(p: &Param, h: HostParam, time: f64) -> [f64; 3] {
    match p.shape {
        Shape::Point => {
            let [x, y] = h.xy_at(time);
            [x, y, 0.0]
        }
        Shape::Color => h.rgb_at(time),
        Shape::Int { .. } | Shape::Choice(_) | Shape::Toggle => {
            [f64::from(h.int_at(time)), 0.0, 0.0]
        }
        _ => [h.f64_at(time), 0.0, 0.0],
    }
}

/// An instance's values at `time`.
pub(crate) fn values<'a>(fx: &'a Fx, host: Host, time: f64) -> V<'a> {
    V::of(fx, |p| {
        if p.is_empty() {
            return None;
        }
        let h = host.param(&c(p.name));
        // What this context leaves out keeps its default.
        (!h.0.is_null()).then(|| read(p, h, time))
    })
}

/// A picture of the host's as the kernels read it: `bounds` of it, rows
/// from the top.
fn picture(img: Option<&Image>, bounds: [c_int; 4]) -> Picture {
    let [x1, y1, x2, y2] = bounds;
    let (w, h) = ((x2 - x1).max(1) as u32, (y2 - y1).max(1) as u32);
    let mut p = Picture::new(w, h);
    let Some(img) = img else { return p };
    let straight = img.channels == 4 && img.unpremultiplied;
    let same = img.bounds[0] == x1 && img.bounds[2] == x2;
    for_chunks(&mut p.data, w as usize, |j, row| {
        let y = y2 - 1 - j as c_int;
        match rgba_row(img, y) {
            Some(src) if same => row.copy_from_slice(&src[..row.len()]),
            _ => {
                for (x, px) in (x1..x2).zip(row.iter_mut()) {
                    *px = read_pixel(img, x, y).unwrap_or([0.0; 4]);
                }
            }
        }
        if straight {
            for c in row.iter_mut() {
                *c = [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]];
            }
        }
        // Nothing that is no number reaches the kernels.
        for c in row.iter_mut() {
            if !c.iter().all(|v| v.is_finite()) {
                *c = c.map(|v| if v.is_finite() { v } else { 0.0 });
            }
        }
    });
    p
}

/// Writes `p` over the output.
fn write(out: &Image, p: &Picture, window: [c_int; 4]) {
    let [x1, y1, x2, y2] = out.bounds;
    let w = (x2 - x1).max(0) as usize;
    let straight = out.channels == 4 && out.unpremultiplied;
    let rows = window[1].max(y1)..window[3].min(y2);
    let columns = window[0].max(x1)..window[2].min(x2);
    bv_effects::host::for_rows(rows, &|y| {
        let Some(row) = out.row(y) else { return };
        let j = (y2 - 1 - y) as usize;
        let Some(src) = p.data.get(j * p.w as usize..(j + 1) * p.w as usize) else {
            return;
        };
        for x in columns.clone() {
            let i = (x - x1) as usize;
            if i >= w {
                break;
            }
            let mut c = src.get(i).copied().unwrap_or([0.0; 4]);
            if !c.iter().all(|v| v.is_finite()) {
                c = c.map(|v| if v.is_finite() { v } else { 0.0 });
            }
            c[3] = c[3].clamp(0.0, 1.0);
            if straight && c[3] > 0.0 {
                c = [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]];
            }
            // SAFETY: a row the output has, a column inside its bounds.
            unsafe { put_pixel(out, row, x, c) };
        }
    });
}

fn render(fx: &Fx, host: Host, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let window: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|v| if v > 0.0 { v } else { 1.0 });
    let Some(out) = host.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let bounds = out.bounds;
    let (w, h) = (
        (bounds[2] - bounds[0]).max(1) as u32,
        (bounds[3] - bounds[1]).max(1) as u32,
    );
    let v = values(fx, host, time);
    let props = host.props();
    let rate = props.dbl_or(kOfxImageEffectPropFrameRate, 30.0);
    let length = props.dbl_or(kOfxImageEffectInstancePropEffectDuration, 0.0);
    let wide = f64::from(w) * out.par / scale[0];
    let tall = f64::from(h) / scale[1];
    let mut at = Frame {
        w,
        h,
        scale: scale[1] as f32,
        shape: (wide / tall.max(1e-6)) as f32,
        frame: time,
        rate: if rate > 0.0 { rate } else { 30.0 },
        length,
        progress: 0.0,
        alone: false,
        others: 0,
    };
    let mut pictures: Vec<Picture> = Vec::new();
    match fx.kind {
        Kind::Transition => {
            at.progress = host.param(kOfxImageEffectTransitionParamName).f64_at(time);
            for name in [
                kOfxImageEffectTransitionSourceFromClipName,
                kOfxImageEffectTransitionSourceToClipName,
            ] {
                let img = host.clip(name).image(time);
                pictures.push(picture(img.as_ref(), bounds));
            }
        }
        _ => {
            let clip = host.clip(kOfxImageEffectSimpleSourceClipName);
            at.alone = fx.kind == Kind::Generator && !clip.connected();
            if at.alone {
                pictures.push(Picture::new(1, 1));
            } else {
                let img = clip.image(time);
                pictures.push(picture(img.as_ref(), bounds));
                for t in fx.times(&v, time) {
                    let img = clip.image(t);
                    let p = match &img {
                        Some(i) => picture(Some(i), bounds),
                        // No frame there: the one nearest, this one.
                        None => pictures[0].clone(),
                    };
                    pictures.push(p);
                }
                at.others = pictures.len() - 1;
            }
        }
    }
    let graph = fx.graph(&v, at);
    let refs: Vec<&Picture> = pictures.iter().collect();
    let result = run(&graph, &refs, &|data, width, f| {
        for_chunks(data, width, |y, row| f(y, row));
    });
    write(&out, &result, window);
    drop(out);
    kOfxStatOK
}
