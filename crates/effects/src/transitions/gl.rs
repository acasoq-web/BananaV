//! GL Transition, as VEGAS's: a transition written in GLSL the way the
//! open gl-transitions collection writes them — `vec4 transition(vec2 uv)`
//! reading `getFromColor` and `getToColor` at `progress`, `ratio` the
//! frame's width over its height —, the code typed in or a preset's (the
//! collection's transitions, their code hidden). Uniforms named after the
//! panel's controls (`kParamFloat0` to `3`, `kParamInt0` and `1`,
//! `kParamBool0`, `kParamVec20`, `kParamIVec20`, `kParamVec40` and `41`)
//! show those controls, with the defaults and slider ranges their comments
//! give (`// = 0.5; [0, 1]`) and the names their `…Name` settings hold.
//! Run by BananaV's GLSL interpreter on the processor ([`crate::glsl`]) and
//! as a kernel on the graphics card (`glsl::wgsl`); the pictures read as
//! VEGAS's textures — bilinearly, repeated past their edges —, each pixel
//! held within 0–1. The collection's authors and license are in
//! `crates/effects/licenses/gl-transitions.md`.

use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::sync::{Arc, Mutex};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::glsl::wgsl::{SLOTS, slot_offset};
use crate::glsl::{Base, Error, Inputs, Program, Ty, Val};
use crate::ofx::{Fx, Image, Props, is, suites};
use crate::{Preset, PresetValue};

mod presets;

/// The plug-in's identifier.
pub(crate) const ID: &CStr = c"org.bananav.GLTransition";

/// Whether the code is a preset's (its number; 0: the user's own).
const IS_PRESET: &CStr = c"IsPreset";
/// The code.
const CODE: &CStr = c"GLSLString";
/// VEGAS's messages for code that cannot run.
const LOCALIZATION: &CStr = c"Localization";
/// The controls' group.
const PARAMETER: &CStr = c"Parameter";

/// The code a new transition starts with: a crossfade.
const DEFAULT_CODE: &str = "vec4 transition (vec2 uv) {\n  return mix(\n    getFromColor(uv),\n    getToColor(uv),\n    progress\n  );\n}\n";

const MESSAGES: [&CStr; 2] = [
    c"Your graphics card is not compatible with GL Transitions.",
    c"Your video card (or driver) cannot perform this GL transition.",
];

/// Each control, in [`SLOTS`]' order, and the setting holding its name.
const CONTROLS: [(&CStr, &CStr); 11] = [
    (c"kParamFloat0", c"kParamFloat0Name"),
    (c"kParamFloat1", c"kParamFloat1Name"),
    (c"kParamFloat2", c"kParamFloat2Name"),
    (c"kParamFloat3", c"kParamFloat3Name"),
    (c"kParamInt0", c"kParamInt0Name"),
    (c"kParamInt1", c"kParamInt1Name"),
    (c"kParamBool0", c"kParamBool0Name"),
    (c"kParamVec20", c"kParamVec20Name"),
    (c"kParamIVec20", c"kParamIVec20Name"),
    (c"kParamVec40", c"kParamVec40Name"),
    (c"kParamVec41", c"kParamVec41Name"),
];

/// What a uniform's comment says: its default and its slider's range.
#[derive(Clone, Debug, Default, PartialEq)]
struct Comment {
    default: Option<Vec<f64>>,
    range: Option<[f64; 2]>,
}

/// Numbers in `text`: the first a number begins with (`stod`'s), or a
/// constructor's (`vec2(0.5)`, `vec4(0., 0., 0., .6)`), or `true` and
/// `false`.
fn numbers(text: &str) -> Option<Vec<f64>> {
    let t = text.trim();
    if let Some(open) = t.find('(') {
        let close = t[open..].find(')')? + open;
        let inner = &t[open + 1..close];
        return inner.split(',').map(|v| leading(v.trim())).collect();
    }
    match t.split(|c: char| c.is_whitespace() || c == ';').next()? {
        "true" => Some(vec![1.0]),
        "false" => Some(vec![0.0]),
        _ => leading(t).map(|v| vec![v]),
    }
}

/// The number `text` begins with.
fn leading(text: &str) -> Option<f64> {
    let end = text
        .char_indices()
        .take_while(|&(k, c)| {
            c.is_ascii_digit()
                || c == '.'
                || (k == 0 && (c == '-' || c == '+'))
                || c == 'e'
                || c == 'E'
        })
        .map(|(k, c)| k + c.len_utf8())
        .last()?;
    text[..end].parse().ok()
}

/// Whether `code` declares uniform `name` of type `ty` (VEGAS's search:
/// `uniform <type> <name>`), and what its line's comment says.
fn declared(code: &str, ty: &str, name: &str) -> Option<Comment> {
    let at = code.find(&format!("uniform {ty} {name}"))?;
    let line = code[at..].lines().next().unwrap_or("");
    let rest = line.find("//").map(|k| &line[k + 2..])?;
    let value = rest.trim_start().trim_start_matches('=').trim_start();
    let range = rest.find('[').and_then(|o| {
        let c = rest[o..].find(']')? + o;
        let v: Vec<f64> = rest[o + 1..c]
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        (v.len() >= 2).then(|| [v[0], v[1]])
    });
    Some(Comment {
        default: numbers(value),
        range,
    })
    .or(Some(Comment::default()))
}

/// The GLSL name of a control's type.
fn glsl_type(ty: Ty) -> &'static str {
    match ty {
        Ty::Vec(Base::Float, 1) => "float",
        Ty::Vec(Base::Int, 1) => "int",
        Ty::Vec(Base::Bool, 1) => "bool",
        Ty::Vec(Base::Int, 2) => "ivec2",
        Ty::Vec(Base::Float, 2) => "vec2",
        _ => "vec4",
    }
}

/// Code compiled, or what is wrong with it.
type Compiled = Arc<Result<Program, Error>>;

/// `code` compiled, the last one kept (every frame asks again).
fn compiled(code: &str) -> Compiled {
    static LAST: Mutex<Option<(String, Compiled)>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((c, p)) = last.as_ref()
        && c == code
    {
        return p.clone();
    }
    let p = Arc::new(Program::compile(code));
    *last = Some((code.to_string(), p.clone()));
    p
}

/// `code` made a kernel for the card (its WGSL), the last ones kept;
/// `None` when it cannot be one there.
fn kernel(code: &str) -> Option<Arc<String>> {
    static LAST: Mutex<Vec<(String, Option<Arc<String>>)>> = Mutex::new(Vec::new());
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, w)) = last.iter().find(|(c, _)| c == code) {
        return w.clone();
    }
    let w = crate::glsl::wgsl::translate(code).ok().map(Arc::new);
    if last.len() >= 8 {
        last.remove(0);
    }
    last.push((code.to_string(), w.clone()));
    w
}

/// The plug-in's entry point.
pub(crate) unsafe extern "C" fn main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if suites().is_none() {
        return kOfxStatErrMissingHostFeature;
    }
    let fx = Fx(handle as OfxImageEffectHandle);
    let (args, out) = (Props(in_args), Props(out_args));
    if is(action, kOfxActionDescribe) {
        describe(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        describe_in_context(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        render(fx, args)
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // The pictures are read anywhere: all of both.
        let (origin, frame) = fx.frame();
        let all = [
            origin[0],
            origin[1],
            origin[0] + frame[0],
            origin[1] + frame[1],
        ];
        out.set_dbls(c"OfxImageClipPropRoI_SourceFrom", &all);
        out.set_dbls(c"OfxImageClipPropRoI_SourceTo", &all);
        kOfxStatOK
    } else if is(action, kOfxActionInstanceChanged)
        || is(action, kOfxActionCreateInstance)
        || is(action, kOfxActionBeginInstanceEdit)
    {
        show(fx, args.dbl(kOfxPropTime, 0));
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

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"GL Transition");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV");
    p.set_str(
        kOfxPropPluginDescription,
        c"Applys transitions based on GLSL language.",
    );
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextTransition],
    );
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    p.set_int(kOfxImageEffectPropSupportsTiles, 0);
    p.set_int(kOfxImageEffectPropSupportsMultiResolution, 1);
    p.set_int(kOfxImageEffectPluginPropHostFrameThreading, 0);
    p.set_str(
        kOfxImageEffectPluginRenderThreadSafety,
        kOfxImageEffectRenderFullySafe,
    );
}

fn describe_in_context(fx: Fx) {
    let rgba = [kOfxImageComponentRGBA];
    fx.define_clip(kOfxImageEffectTransitionSourceFromClipName, &rgba);
    fx.define_clip(kOfxImageEffectTransitionSourceToClipName, &rgba);
    fx.define_clip(kOfxImageEffectOutputClipName, &rgba);
    let p = fx.define_param(
        kOfxParamTypeDouble,
        kOfxImageEffectTransitionParamName,
        c"Progress",
    );
    p.set_dbl(kOfxParamPropDefault, 0.0);
    p.set_dbl(kOfxParamPropDisplayMin, 0.0);
    p.set_dbl(kOfxParamPropDisplayMax, 1.0);
    p.set_dbl(kOfxParamPropIncrement, 0.1);
    p.set_int(kOfxParamPropDigits, 2);
    p.set_int(kOfxParamPropSecret, 1);
    let p = fx.define_param(kOfxParamTypeInteger, IS_PRESET, c"IsPreset");
    p.set_int(kOfxParamPropDefault, 0);
    p.set_int(kOfxParamPropSecret, 1);
    p.set_int(kOfxParamPropAnimates, 0);
    let p = fx.define_param(kOfxParamTypeString, CODE, c"Enter your GL transition code");
    p.set_str(kOfxParamPropStringMode, kOfxParamStringIsMultiLine);
    if let Ok(d) = CString::new(DEFAULT_CODE) {
        p.set_str(kOfxParamPropDefault, &d);
    }
    p.set_str(kOfxParamPropHint, c"Your custom transition code.");
    p.set_int(kOfxParamPropAnimates, 0);
    let p = fx.define_param(kOfxParamTypeChoice, LOCALIZATION, c"Localization");
    p.set_strs(kOfxParamPropChoiceOption, &MESSAGES);
    p.set_int(kOfxParamPropSecret, 1);
    let g = fx.define_param(kOfxParamTypeGroup, PARAMETER, c"Parameter");
    g.set_int(kOfxParamPropGroupOpen, 1);
    g.set_int(kOfxParamPropSecret, 1);
    for ((name, label), (_, ty)) in CONTROLS.into_iter().zip(SLOTS) {
        let n = fx.define_param(kOfxParamTypeString, label, label);
        n.set_str(kOfxParamPropDefault, c"");
        n.set_int(kOfxParamPropSecret, 1);
        n.set_int(kOfxParamPropAnimates, 0);
        let p = match ty {
            Ty::Vec(Base::Float, 1) => {
                let p = fx.define_param(kOfxParamTypeDouble, name, name);
                p.set_dbl(kOfxParamPropDefault, 0.0);
                p.set_dbl(kOfxParamPropMin, -1.0e9);
                p.set_dbl(kOfxParamPropMax, 1.0e9);
                p.set_dbl(kOfxParamPropDisplayMin, 0.0);
                p.set_dbl(kOfxParamPropDisplayMax, 1.0);
                p.set_dbl(kOfxParamPropIncrement, 0.1);
                p.set_int(kOfxParamPropDigits, 2);
                p
            }
            Ty::Vec(Base::Int, 1) => {
                let p = fx.define_param(kOfxParamTypeInteger, name, name);
                p.set_int(kOfxParamPropDefault, 0);
                p.set_int(kOfxParamPropMin, c_int::MIN);
                p.set_int(kOfxParamPropMax, c_int::MAX);
                p.set_int(kOfxParamPropDisplayMin, -1);
                p.set_int(kOfxParamPropDisplayMax, 10);
                p
            }
            Ty::Vec(Base::Bool, 1) => {
                let p = fx.define_param(kOfxParamTypeBoolean, name, name);
                p.set_int(kOfxParamPropDefault, 0);
                p
            }
            Ty::Vec(Base::Float, 2) => {
                let p = fx.define_param(kOfxParamTypeDouble2D, name, name);
                p.set_dbls(kOfxParamPropDefault, &[0.5, 0.5]);
                p.set_dbls(kOfxParamPropMin, &[-1.0e9, -1.0e9]);
                p.set_dbls(kOfxParamPropMax, &[1.0e9, 1.0e9]);
                p.set_dbls(kOfxParamPropDisplayMin, &[-1.0, -1.0]);
                p.set_dbls(kOfxParamPropDisplayMax, &[1.0, 1.0]);
                p.set_dbl(kOfxParamPropIncrement, 0.1);
                p.set_int(kOfxParamPropDigits, 2);
                p
            }
            Ty::Vec(Base::Int, 2) => {
                let p = fx.define_param(kOfxParamTypeInteger2D, name, name);
                p.set_ints(kOfxParamPropDefault, &[2, 2]);
                p.set_ints(kOfxParamPropMin, &[c_int::MIN, c_int::MIN]);
                p.set_ints(kOfxParamPropMax, &[c_int::MAX, c_int::MAX]);
                p.set_ints(kOfxParamPropDisplayMin, &[1, 1]);
                p.set_ints(kOfxParamPropDisplayMax, &[4, 4]);
                p
            }
            _ => {
                let p = fx.define_param(kOfxParamTypeRGBA, name, name);
                p.set_dbls(kOfxParamPropDefault, &[0.0, 0.0, 0.0, 1.0]);
                p
            }
        };
        p.set_str(kOfxParamPropParent, PARAMETER);
        p.set_int(kOfxParamPropSecret, 1);
        p.set_int(kOfxParamPropAnimates, 0);
    }
}

/// The panel as the code says it, as VEGAS's: the controls its uniforms
/// are shown — with the default and slider range their comments give
/// (the value moving to the new default while it is the old one) and the
/// names their `…Name` settings hold —, the others hidden, the Parameter
/// group shown (and open) when any is; a preset's code hidden; a message
/// when the code cannot run.
fn show(fx: Fx, time: f64) {
    let code = fx.param(CODE).text_at(time);
    fx.param(CODE)
        .set_secret(fx.param(IS_PRESET).int_at(time) > 0);
    let mut any = false;
    for ((name, label), (_, ty)) in CONTROLS.into_iter().zip(SLOTS) {
        let p = fx.param(name);
        let found = declared(&code, glsl_type(ty), &name.to_string_lossy());
        p.set_secret(found.is_none());
        let Some(comment) = found else { continue };
        any = true;
        let props = p.props();
        if let Some(d) = comment.default {
            match ty {
                Ty::Vec(Base::Float, 1) => {
                    let old = props.dbl(kOfxParamPropDefault, 0);
                    let v = d[0];
                    props.set_dbl(kOfxParamPropDefault, v);
                    if p.f64_at(time) == old {
                        p.set_f64(v);
                    }
                    if comment.range.is_none() && v > props.dbl(kOfxParamPropDisplayMax, 0) {
                        props.set_dbl(kOfxParamPropDisplayMax, v + v);
                    }
                }
                Ty::Vec(Base::Int | Base::Bool, 1) => {
                    let old = props.int(kOfxParamPropDefault, 0);
                    let v = d[0] as c_int;
                    props.set_int(kOfxParamPropDefault, v);
                    if p.int_at(time) == old {
                        p.set_int(v);
                    }
                }
                Ty::Vec(_, 2) if !d.is_empty() => {
                    let v = [d[0], d.get(1).copied().unwrap_or(d[0])];
                    if ty.base_is_int() {
                        let old: [c_int; 2] = props.ints(kOfxParamPropDefault);
                        let v = v.map(|x| x as c_int);
                        props.set_ints(kOfxParamPropDefault, &v);
                        if p.ij_at(time) == old {
                            p.set_ij(v);
                        }
                    } else {
                        let old: [f64; 2] = props.dbls(kOfxParamPropDefault);
                        props.set_dbls(kOfxParamPropDefault, &v);
                        if p.xy_at(time) == old {
                            p.set_xy(v);
                        }
                    }
                }
                _ if d.len() == 4 => {
                    let v = [d[0], d[1], d[2], d[3]];
                    let old: [f64; 4] = props.dbls(kOfxParamPropDefault);
                    props.set_dbls(kOfxParamPropDefault, &v);
                    if p.rgba_at(time) == old {
                        p.set_rgba(v);
                    }
                }
                _ => {}
            }
        }
        if let Some([lo, hi]) = comment.range {
            match ty {
                Ty::Vec(Base::Float, 1) => {
                    props.set_dbl(kOfxParamPropDisplayMin, lo);
                    props.set_dbl(kOfxParamPropDisplayMax, hi);
                }
                Ty::Vec(Base::Int, 1) => {
                    props.set_int(kOfxParamPropDisplayMin, lo as c_int);
                    props.set_int(kOfxParamPropDisplayMax, hi as c_int);
                }
                Ty::Vec(Base::Float, 2) => {
                    props.set_dbls(kOfxParamPropDisplayMin, &[lo, lo]);
                    props.set_dbls(kOfxParamPropDisplayMax, &[hi, hi]);
                }
                Ty::Vec(Base::Int, 2) => {
                    props.set_ints(kOfxParamPropDisplayMin, &[lo as c_int; 2]);
                    props.set_ints(kOfxParamPropDisplayMax, &[hi as c_int; 2]);
                }
                _ => {}
            }
        }
        // Its name, as its setting holds it (else its own).
        let named = fx.param(label).text_at(time);
        let shown = if named.trim().is_empty() {
            name.to_owned()
        } else {
            CString::new(named).unwrap_or_else(|_| name.to_owned())
        };
        props.set_str(kOfxPropLabel, &shown);
    }
    let g = fx.param(PARAMETER);
    g.set_secret(!any);
    g.props().set_int(kOfxParamPropGroupOpen, c_int::from(any));
    match compiled(&code).as_ref() {
        Ok(_) => fx.persistent_message(None),
        Err(_) => fx.persistent_message(Some(MESSAGES[1])),
    }
}

impl Ty {
    fn base_is_int(self) -> bool {
        matches!(self, Ty::Vec(Base::Int, _))
    }
}

/// Each of `p`'s uniforms' values: a control's when it is one (by name
/// and type), else zero, as VEGAS leaves them.
fn uniform_values(p: &Program, value: &dyn Fn(usize) -> Vec<f64>) -> Vec<Val> {
    p.uniforms
        .iter()
        .map(|u| {
            let mut v = [0.0f32; 16];
            if let Some(k) = SLOTS.iter().position(|(n, t)| *n == u.name && *t == u.ty) {
                for (o, x) in v.iter_mut().zip(value(k)) {
                    *o = x as f32;
                }
            }
            v
        })
        .collect()
}

/// Control `k`'s numbers at `time`.
fn control_value(fx: Fx, k: usize, time: f64) -> Vec<f64> {
    let p = fx.param(CONTROLS[k].0);
    match SLOTS[k].1 {
        Ty::Vec(Base::Float, 1) => vec![p.f64_at(time)],
        Ty::Vec(_, 1) => vec![f64::from(p.int_at(time))],
        Ty::Vec(Base::Int, 2) => p.ij_at(time).map(f64::from).to_vec(),
        Ty::Vec(_, 2) => p.xy_at(time).to_vec(),
        _ => p.rgba_at(time).to_vec(),
    }
}

/// A picture as the code reads it: premultiplied, rows from the bottom.
struct Texture {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
}

impl Texture {
    fn of(img: Option<&Image>) -> Texture {
        let Some(img) = img else {
            return Texture {
                w: 1,
                h: 1,
                px: vec![[0.0; 4]],
            };
        };
        let [x1, y1, x2, y2] = img.bounds;
        let (w, h) = ((x2 - x1).max(1) as usize, (y2 - y1).max(1) as usize);
        let straight = img.channels == 4 && img.unpremultiplied;
        let mut px = Vec::with_capacity(w * h);
        for y in y1..y1 + h as c_int {
            for x in x1..x1 + w as c_int {
                let c = match crate::colors::rgba_row(img, y) {
                    Some(row) => row.get((x - x1) as usize).copied(),
                    None => crate::colors::pixel(img, x, y),
                }
                .unwrap_or([0.0; 4]);
                px.push(if straight {
                    [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
                } else {
                    c
                });
            }
        }
        Texture { w, h, px }
    }

    /// Texel `i` across, `j` up, repeated past the edges.
    fn texel(&self, i: i64, j: i64) -> [f32; 4] {
        let x = i.rem_euclid(self.w as i64) as usize;
        let y = j.rem_euclid(self.h as i64) as usize;
        self.px[y * self.w + x]
    }

    /// The picture at `uv` (from the lower left), bilinearly: VEGAS's
    /// textures (linear, repeated); nothing at no number or past ten
    /// million pixels.
    fn sample(&self, uv: [f32; 2]) -> [f32; 4] {
        let px = uv[0] * self.w as f32 - 0.5;
        let py = uv[1] * self.h as f32 - 0.5;
        if !(px.abs() < 1.0e7 && py.abs() < 1.0e7) {
            return [0.0; 4];
        }
        let (fx, fy) = (px.floor(), py.floor());
        let (tx, ty) = (px - fx, py - fy);
        let (i, j) = (fx as i64, fy as i64);
        let (a, b) = (self.texel(i, j), self.texel(i + 1, j));
        let (c, d) = (self.texel(i, j + 1), self.texel(i + 1, j + 1));
        std::array::from_fn(|k| {
            let low = a[k] + (b[k] - a[k]) * tx;
            let high = c[k] + (d[k] - c[k]) * tx;
            low + (high - low) * ty
        })
    }
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let from = fx
        .clip(kOfxImageEffectTransitionSourceFromClipName)
        .image(time);
    let to = fx
        .clip(kOfxImageEffectTransitionSourceToClipName)
        .image(time);
    for img in [&from, &to].into_iter().flatten() {
        if img.depth != out.depth || img.channels != out.channels {
            return kOfxStatErrImageFormat;
        }
    }
    let progress = fx.param(kOfxImageEffectTransitionParamName).f64_at(time) as f32;
    let program = compiled(&fx.param(CODE).text_at(time));
    let values = match program.as_ref() {
        Ok(p) => uniform_values(p, &|k| control_value(fx, k, time)),
        Err(_) => Vec::new(),
    };
    let (a, b) = (Texture::of(from.as_ref()), Texture::of(to.as_ref()));
    let [ox, oy, ex, ey] = out.bounds;
    let (w, h) = ((ex - ox).max(1) as f32, (ey - oy).max(1) as f32);
    let inputs = Inputs {
        progress,
        ratio: w / h,
        values: &values,
    };
    let out_straight = out.channels == 4 && out.unpremultiplied;
    let columns = x1.max(ox)..x2.min(ex);
    let fill = |y: c_int| {
        thread_local! {
            static SLOTS_USED: RefCell<Vec<Val>> = const { RefCell::new(Vec::new()) };
        }
        let Some(row) = out.row(y) else { return };
        for x in columns.clone() {
            let uv = [(x - ox) as f32 + 0.5, (y - oy) as f32 + 0.5];
            let uv = [uv[0] / w, uv[1] / h];
            let mut c = match program.as_ref() {
                Ok(p) => {
                    let sample = |which: u8, q: [f32; 2]| {
                        if which == 0 { a.sample(q) } else { b.sample(q) }
                    };
                    SLOTS_USED.with(|s| p.run(&mut s.borrow_mut(), &inputs, &sample, uv))
                }
                Err(_) => [0.0; 4],
            };
            c = c.map(|v| if v >= 0.0 { v.min(1.0) } else { 0.0 });
            if out_straight && c[3] > 0.0 {
                c = [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]];
            }
            // SAFETY: a row the output has, a column inside its bounds.
            unsafe { crate::colors::put_pixel(&out, row, x, c) };
        }
    };
    super::for_rows(y1.max(oy)..y2.min(ey), &fill);
    drop(from);
    drop(to);
    drop(out);
    kOfxStatOK
}

/// The card's kernel for the transition with `values` at `progress` over
/// pictures `w` × `h`: its WGSL (after WGSL's prelude) and its numbers
/// (`P.f`: progress, the frame's ratio, the controls'); `None` when the
/// code cannot be a kernel (the processor draws it).
pub(crate) fn card(
    values: crate::accel::Values<'_>,
    progress: f64,
    w: u32,
    h: u32,
) -> Option<(Arc<String>, [f32; 24])> {
    let code = values.text(CODE).unwrap_or(DEFAULT_CODE);
    let wgsl = kernel(code)?;
    let mut f = [0.0f32; 24];
    f[0] = progress as f32;
    f[1] = w.max(1) as f32 / h.max(1) as f32;
    for (k, ((name, _), (_, ty))) in CONTROLS.into_iter().zip(SLOTS).enumerate() {
        let defaults: Vec<f64> = match ty {
            Ty::Vec(_, 2) if ty.base_is_int() => vec![2.0, 2.0],
            Ty::Vec(_, 2) => vec![0.5, 0.5],
            Ty::Vec(_, 4) => vec![0.0, 0.0, 0.0, 1.0],
            _ => vec![0.0],
        };
        let v = values.nums(name).map_or(defaults, <[f64]>::to_vec);
        for (i, x) in v.iter().take(ty.size()).enumerate() {
            f[slot_offset(k) + i] = *x as f32;
        }
    }
    Some((wgsl, f))
}

/// The factory presets: the collection's transitions, as VEGAS lists
/// them.
pub(crate) fn presets() -> Vec<Preset> {
    presets::PRESETS
        .iter()
        .map(|p| {
            let mut values = vec![("GLSLString", PresetValue::Text(p.code.to_string()))];
            for &(name, value) in p.values {
                let v = if name.ends_with("Name") {
                    PresetValue::Text(value.to_string())
                } else {
                    PresetValue::Num(
                        value
                            .split_whitespace()
                            .map(|t| match t {
                                "true" => 1.0,
                                "false" => 0.0,
                                _ => t.parse().unwrap_or(0.0),
                            })
                            .collect(),
                    )
                };
                values.push((name, v));
            }
            Preset {
                name: p.name,
                values,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_runs_on_the_processor_and_is_a_kernel() {
        let mut failed = Vec::new();
        for p in presets::PRESETS {
            if let Err(e) = Program::compile(p.code) {
                failed.push(format!("{}: {e}", p.name));
            }
            match crate::glsl::wgsl::translate(p.code) {
                Err(e) => failed.push(format!("{} on the card: {e}", p.name)),
                Ok(_) => {
                    if let Err(e) =
                        std::panic::catch_unwind(|| crate::glsl::wgsl::tests::check(p.code))
                    {
                        let m = e.downcast_ref::<String>().cloned().unwrap_or_default();
                        failed.push(format!(
                            "{} as WGSL: {}",
                            p.name,
                            m.lines().take(12).collect::<Vec<_>>().join(
                                "
"
                            )
                        ));
                    }
                }
            }
        }
        assert!(
            failed.is_empty(),
            "{}",
            failed.join(
                "
"
            )
        );
        assert_eq!(presets::PRESETS.len(), 62);
    }

    #[test]
    fn the_comments_give_defaults_and_ranges() {
        let code = "uniform float kParamFloat0; // = 0.075; [0.01,0.4]\nuniform vec4 kParamVec40; // = vec4(0.,0.,0.,.6)\nuniform bool kParamBool0; // = true;\nuniform ivec2 kParamIVec20; // = ivec2(10);";
        let f = declared(code, "float", "kParamFloat0").unwrap();
        assert_eq!(f.default, Some(vec![0.075]));
        assert_eq!(f.range, Some([0.01, 0.4]));
        let c = declared(code, "vec4", "kParamVec40").unwrap();
        assert_eq!(c.default, Some(vec![0.0, 0.0, 0.0, 0.6]));
        assert_eq!(
            declared(code, "bool", "kParamBool0").unwrap().default,
            Some(vec![1.0])
        );
        assert_eq!(
            declared(code, "ivec2", "kParamIVec20").unwrap().default,
            Some(vec![10.0])
        );
        assert!(declared(code, "float", "kParamFloat1").is_none());
        // Only its own type.
        assert!(declared(code, "vec2", "kParamVec40").is_none());
    }

    #[test]
    fn the_pictures_repeat_past_their_edges() {
        let t = Texture {
            w: 2,
            h: 1,
            px: vec![[0.0; 4], [1.0; 4]],
        };
        // Texel middles, and the edge between the last and the first.
        assert_eq!(t.sample([0.25, 0.5])[0], 0.0);
        assert_eq!(t.sample([0.75, 0.5])[0], 1.0);
        assert!((t.sample([1.0, 0.5])[0] - 0.5).abs() < 1e-6);
        assert!((t.sample([0.0, 0.5])[0] - 0.5).abs() < 1e-6);
        assert_eq!(t.sample([1.25, 0.5])[0], 0.0);
    }

    #[test]
    fn presets_carry_their_code_and_controls() {
        let all = presets();
        let bounce = all.iter().find(|p| p.name == "Bounce").unwrap();
        let get = |n: &str| {
            bounce
                .values
                .iter()
                .find(|(k, _)| *k == n)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(get("IsPreset"), Some(PresetValue::Num(vec![14.0])));
        assert_eq!(get("kParamFloat0"), Some(PresetValue::Num(vec![0.075])));
        assert_eq!(
            get("kParamVec40"),
            Some(PresetValue::Num(vec![0.0, 0.0, 0.0, 0.6]))
        );
        assert_eq!(
            get("kParamFloat0Name"),
            Some(PresetValue::Text("Shadow Height".into()))
        );
        assert!(
            matches!(get("GLSLString"), Some(PresetValue::Text(t)) if t.contains("kParamFloat1"))
        );
    }
}
