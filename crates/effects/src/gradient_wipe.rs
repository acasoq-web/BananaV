//! Gradient Wipe as VEGAS draws it: the second picture comes in where a
//! gradient image is dark first and where it is light last (or the other
//! way round, Reverse). The gradient is made 8-bit steps at the frame's
//! pixels — a picture file Search… picks (the host hands it over on a clip
//! of its own, BananaV's `kBananaVParamPropPicture`, and it is kept small in
//! the transition's settings), cut to the frame's shape and enlarged when
//! Maintain aspect ratio, else stretched; or one of BananaV's own shapes —,
//! its darkest step made black and its lightest white (Auto contrast
//! stretch), flipped and mirrored, and blurred by VEGAS's blur over
//! Gradient blur's reach. Each step's share of the first picture at a
//! progress is VEGAS's ramp, Threshold blend wide, in 8-bit steps; the
//! pictures are mixed by it. The card reads the same steps and ramp
//! ([`plan`]).
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::extensions::{kBananaVParamPropPicture, kBananaVParamPropPictureClip};
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::{TransitionKernel, TransitionPlan, TransitionSteps};
use crate::kit::Buf;
use crate::ofx::{Fx, Props, for_rows, is, suites};
use crate::recursive_gauss;
use crate::transitions::framed;
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.GradientWipe";

const SHAPE: &CStr = c"shape";
const SEARCH: &CStr = c"search";
const IMAGE: &CStr = c"gradient";
const FLIP_H: &CStr = c"flipHorizontally";
const FLIP_V: &CStr = c"flipVertically";
const MIRROR_H: &CStr = c"mirrorHorizontally";
const MIRROR_V: &CStr = c"mirrorVertically";
const BLEND: &CStr = c"thresholdBlend";
const BLUR: &CStr = c"gradientBlur";
const DIRECTION: &CStr = c"direction";
const ASPECT: &CStr = c"maintainAspect";
const STRETCH: &CStr = c"autoContrast";
const STEREO: &CStr = c"stereoscopicDepth";
/// The clip the gradient's picture arrives on.
const CLIP: &CStr = c"Gradient";

/// The gradients: the picture's, or one of BananaV's own shapes.
const SHAPES: [&CStr; 7] = [
    c"Image",
    c"Linear",
    c"Radial",
    c"Diagonal",
    c"Clock",
    c"Diamond",
    c"Clouds",
];
const DIRECTIONS: [&CStr; 2] = [c"Forward", c"Reverse"];

/// How wide the gradient is kept (its height in its shape).
const KEPT_WIDTH: usize = 256;

/// A gradient: its values 0 (dark) to 1 (light), rows from the top.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Map {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl Map {
    /// The gradient at `u`, `v` (0–1 of it, from its top left), its edges
    /// held.
    fn at(&self, u: f64, v: f64) -> f32 {
        let (w, h) = (self.width.max(1), self.height.max(1));
        let x = (u * w as f64 - 0.5).clamp(0.0, (w - 1) as f64);
        let y = (v * h as f64 - 0.5).clamp(0.0, (h - 1) as f64);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
        let p = |x: usize, y: usize| self.data[y * w + x];
        let top = p(x0, y0) + (p(x1, y0) - p(x0, y0)) * fx;
        let bottom = p(x0, y1) + (p(x1, y1) - p(x0, y1)) * fx;
        top + (bottom - top) * fy
    }

    /// As text, to keep in the settings: its size and its bytes in Base64.
    fn text(&self) -> String {
        let bytes: Vec<u8> = self
            .data
            .iter()
            .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect();
        format!(
            "{} {} {}",
            self.width,
            self.height,
            bv_core::base64::encode(&bytes)
        )
    }

    fn parse(text: &str) -> Option<Map> {
        let mut parts = text.split_whitespace();
        let width: usize = parts.next()?.parse().ok()?;
        let height: usize = parts.next()?.parse().ok()?;
        let bytes = bv_core::base64::decode(parts.next()?)?;
        (width > 0 && height > 0 && bytes.len() == width * height).then(|| Map {
            width,
            height,
            data: bytes.iter().map(|&b| f32::from(b) / 255.0).collect(),
        })
    }

    /// One of BananaV's shapes (1–6 of [`SHAPES`]) for a frame `aspect`
    /// wide for 1 high, kept small.
    #[cfg(test)]
    fn shape(kind: usize, aspect: f64) -> Map {
        let width = KEPT_WIDTH;
        let height = ((width as f64 / aspect.max(0.1)).round() as usize).max(1);
        let mut data = Vec::with_capacity(width * height);
        for j in 0..height {
            for i in 0..width {
                let (u, v) = (
                    (i as f64 + 0.5) / width as f64,
                    (j as f64 + 0.5) / height as f64,
                );
                data.push(shape_value(kind, u, v, aspect) as f32);
            }
        }
        Map {
            width,
            height,
            data,
        }
    }
}

/// BananaV's shape `kind` (1–6 of [`SHAPES`]) at `u`, `v` (0–1 from the
/// frame's top left) of a frame `aspect` wide for 1 high: 0 dark to 1
/// light.
fn shape_value(kind: usize, u: f64, v: f64, aspect: f64) -> f64 {
    let (x, y) = ((u - 0.5) * aspect, v - 0.5);
    let g = match kind {
        1 => u,
        2 => x.hypot(y) / (0.5 * aspect).hypot(0.5),
        3 => (u + v) / 2.0,
        4 => (x.atan2(-y) / std::f64::consts::TAU).rem_euclid(1.0),
        5 => (x.abs() + y.abs()) / (0.5 * aspect + 0.5),
        _ => clouds(u * aspect, v),
    };
    g.clamp(0.0, 1.0)
}

/// Soft clouds: a few octaves of smoothed value noise.
fn clouds(x: f64, y: f64) -> f64 {
    fn hash(i: i64, j: i64) -> f64 {
        let mut h = (i.wrapping_mul(374_761_393) ^ j.wrapping_mul(668_265_263)) as u64;
        h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
        ((h ^ (h >> 16)) & 0xFFFF) as f64 / 65535.0
    }
    fn noise(x: f64, y: f64) -> f64 {
        let (i, j) = (x.floor() as i64, y.floor() as i64);
        let (fx, fy) = (x - x.floor(), y - y.floor());
        let s = |t: f64| t * t * (3.0 - 2.0 * t);
        let (sx, sy) = (s(fx), s(fy));
        let top = hash(i, j) + (hash(i + 1, j) - hash(i, j)) * sx;
        let bottom = hash(i, j + 1) + (hash(i + 1, j + 1) - hash(i, j + 1)) * sx;
        top + (bottom - top) * sy
    }
    let (mut sum, mut amp, mut f) = (0.0, 0.5, 3.0);
    for _ in 0..4 {
        sum += amp * noise(x * f, y * f);
        amp *= 0.5;
        f *= 2.0;
    }
    sum / 0.9375
}

/// The settings at a time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    pub shape: usize,
    pub image: Option<Map>,
    pub flip: [bool; 2],
    pub mirror: [bool; 2],
    pub blend: f64,
    pub blur: f64,
    pub reverse: bool,
    pub aspect: bool,
    pub stretch: bool,
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        let p = |n| fx.param(n);
        Settings {
            shape: p(SHAPE).int_at(time).clamp(0, SHAPES.len() as c_int - 1) as usize,
            image: Map::parse(&p(IMAGE).text_at(time)),
            flip: [p(FLIP_H).int_at(time) != 0, p(FLIP_V).int_at(time) != 0],
            mirror: [p(MIRROR_H).int_at(time) != 0, p(MIRROR_V).int_at(time) != 0],
            blend: p(BLEND).f64_at(time).clamp(0.0, 1.0),
            blur: p(BLUR).f64_at(time).clamp(0.0, 1.0),
            reverse: p(DIRECTION).int_at(time) == 1,
            aspect: p(ASPECT).int_at(time) != 0,
            stretch: p(STRETCH).int_at(time) != 0,
        }
    }

    /// The gradient's 8-bit steps at a frame `w` × `h` of pixels `par`
    /// times as wide as tall, rows from the top, as VEGAS prepares them
    /// (kept for the next frame: they do not change with the progress).
    pub(crate) fn steps(&self, w: usize, h: usize, par: f64) -> Vec<u8> {
        type Kept = (Settings, usize, usize, u64, Vec<u8>);
        static KEPT: std::sync::Mutex<Option<Kept>> = std::sync::Mutex::new(None);
        let key = Settings {
            blend: 0.0,
            reverse: false,
            ..self.clone()
        };
        if let Ok(kept) = KEPT.lock()
            && let Some((s, kw, kh, kp, steps)) = kept.as_ref()
            && *s == key
            && (*kw, *kh, *kp) == (w, h, par.to_bits())
        {
            return steps.clone();
        }
        let steps = self.made(w, h, par);
        if let Ok(mut kept) = KEPT.lock() {
            *kept = Some((key, w, h, par.to_bits(), steps.clone()));
        }
        steps
    }

    fn made(&self, w: usize, h: usize, par: f64) -> Vec<u8> {
        let (wd, hd) = (w as f64 * par, h as f64);
        let mut steps = vec![0u8; w * h];
        // The picture's, or the shape's, at each pixel.
        match (&self.image, self.shape) {
            (Some(m), 0) => {
                let (mw, mh) = (m.width as f64, m.height as f64);
                // Kept in its shape: the picture's middle part as wide as the
                // frame's shape, enlarged to cover it.
                let (l, t, vw, vh) = if self.aspect {
                    let s = (wd / mw).max(hd / mh);
                    let (dx, dy) = (mw - wd / s, mh - hd / s);
                    (dx * 0.5, dy * 0.5, mw - dx, mh - dy)
                } else {
                    (0.0, 0.0, mw, mh)
                };
                for (j, row) in steps.chunks_mut(w).enumerate() {
                    for (i, s) in row.iter_mut().enumerate() {
                        let u = (l + (i as f64 + 0.5) / w as f64 * vw) / mw;
                        let v = (t + (j as f64 + 0.5) / h as f64 * vh) / mh;
                        *s = (m.at(u, v).clamp(0.0, 1.0) * 255.0).round() as u8;
                    }
                }
            }
            (image, k) => {
                let kind = if image.is_none() && k == 0 { 1 } else { k };
                for (j, row) in steps.chunks_mut(w).enumerate() {
                    for (i, s) in row.iter_mut().enumerate() {
                        let u = (i as f64 + 0.5) / w as f64;
                        let v = (j as f64 + 0.5) / h as f64;
                        *s = (shape_value(kind, u, v, wd / hd) * 255.0).round() as u8;
                    }
                }
            }
        }
        // Stretched from the darkest step to the lightest.
        if self.stretch {
            let (lo, hi) = steps
                .iter()
                .fold((255u8, 0u8), |(lo, hi), &v| (lo.min(v), hi.max(v)));
            if lo != 0 || hi != 255 {
                let (lo, hi) = (i32::from(lo), i32::from(hi));
                let table: Vec<u8> = (0..256)
                    .map(|v| {
                        if v < lo {
                            0
                        } else if v < hi {
                            ((v - lo) * 255 / (hi - lo)) as u8
                        } else {
                            255
                        }
                    })
                    .collect();
                for s in &mut steps {
                    *s = table[*s as usize];
                }
            }
        }
        // Mirrored (the second half the first's), then flipped.
        let place = |k: usize, n: usize, mirror: bool, flip: bool| {
            let k = if mirror && k > (n - 1) / 2 {
                n - 1 - k
            } else {
                k
            };
            if flip { n - 1 - k } else { k }
        };
        if self.flip.iter().chain(&self.mirror).any(|&b| b) {
            let from = steps.clone();
            for j in 0..h {
                let sj = place(j, h, self.mirror[1], self.flip[1]);
                for i in 0..w {
                    let si = place(i, w, self.mirror[0], self.flip[0]);
                    steps[j * w + i] = from[sj * w + si];
                }
            }
        }
        // Blurred as VEGAS blurs its 8-bit pictures.
        if self.blur > 1e-6 {
            let r = w.max(h) as f64 * 846.806_302_4f64.powf(self.blur * 0.8) * 0.001_153_677_4;
            let mut buf = Buf {
                bounds: [0, 0, w as c_int, h as c_int],
                data: steps.iter().map(|&v| [f32::from(v); 4]).collect(),
            };
            recursive_gauss::blur(&mut buf, [r / par, r]);
            for (s, v) in steps.iter_mut().zip(&buf.data) {
                *s = f64::from(v[0]).round_ties_even().clamp(0.0, 255.0) as u8;
            }
        }
        steps
    }
}

/// How much of the first picture each of the gradient's steps shows at
/// `progress`, in 255ths: VEGAS's ramp, `blend` wide, the dark steps going
/// first (the light ones in Reverse).
pub(crate) fn ramp(progress: f64, blend: f64, reverse: bool) -> [u8; 256] {
    // VEGAS's progress is a float.
    let mut p = f64::from(progress as f32);
    if reverse {
        p = 1.0 - p;
    }
    let half = blend * 0.5;
    let c = (half + half + 1.0) * p - (half + half) * 0.5;
    let (lo, hi) = (c - half, c + half);
    let mut k = if hi - lo > 1e-6 {
        255.0 / ((hi - lo) * 255.0)
    } else {
        0.0
    };
    let start = if reverse {
        k = -k;
        255.0
    } else {
        0.0
    };
    let (low, high) = ((lo * 255.0 + 0.5) as i32, (hi * 255.0 + 0.5) as i32);
    std::array::from_fn(|m| {
        let m = m as i32;
        if m <= low {
            if reverse { 255 } else { 0 }
        } else if m <= high {
            let v = (f64::from(m) - lo * 255.0) * k + start;
            if v >= 0.0 {
                v.min(255.0) as i32 as u8
            } else {
                0
            }
        } else if reverse {
            0
        } else {
            255
        }
    })
}

/// The plug-in's entry point.
pub(crate) unsafe extern "C" fn main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if suites().is_none() {
        return kOfxStatErrMissingHostFeature;
    }
    let fx = Fx(handle as OfxImageEffectHandle);
    let args = Props(in_args);
    if is(action, kOfxActionDescribe) {
        describe(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        describe_in_context(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        render(fx, args)
    } else if is(action, kOfxActionInstanceChanged) {
        if args.is(kOfxPropType, kOfxTypeParameter) && args.is(kOfxPropName, SEARCH) {
            take(fx, args.dbl(kOfxPropTime, 0));
        }
        kOfxStatOK
    } else if is(action, kOfxActionLoad)
        || is(action, kOfxActionUnload)
        || is(action, kOfxActionCreateInstance)
        || is(action, kOfxActionDestroyInstance)
    {
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

/// Keeps the picture on the gradient's clip, small and gray, as the
/// gradient (its shape then the image's).
fn take(fx: Fx, time: f64) {
    let Some(img) = fx.clip(CLIP).image(time) else {
        return;
    };
    let b = Buf::read(Some(&img));
    drop(img);
    let (bw, bh) = (b.width().max(1) as usize, b.height().max(1) as usize);
    let width = bw.min(KEPT_WIDTH);
    let height = ((bh * width) as f64 / bw as f64).round().max(1.0) as usize;
    let mut data = Vec::with_capacity(width * height);
    for j in 0..height {
        for i in 0..width {
            // Its rows go up: the map's from the top.
            let x = b.bounds[0] as f64 + (i as f64 + 0.5) * bw as f64 / width as f64 - 0.5;
            let y =
                b.bounds[1] as f64 + bh as f64 - (j as f64 + 0.5) * bh as f64 / height as f64 - 0.5;
            let c = b.bilinear(x, y);
            data.push((0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]).clamp(0.0, 1.0));
        }
    }
    let map = Map {
        width,
        height,
        data,
    };
    fx.param(IMAGE).set_text(&map.text());
    fx.param(SHAPE).set_int(0);
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Gradient Wipe");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Wipes");
    p.set_str(
        kOfxPropPluginDescription,
        c"The second picture comes in where a gradient image is dark first and where it is light last.",
    );
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextTransition],
    );
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    p.set_int(kOfxImageEffectPropSupportsTiles, 1);
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
    let c = fx.define_clip(CLIP, &[kOfxImageComponentRGBA, kOfxImageComponentRGB]);
    c.set_int(kOfxImageClipPropOptional, 1);
    c.set_str(kOfxPropLabel, c"Gradient");
    fx.define_clip(kOfxImageEffectOutputClipName, &rgba);
    let p = fx.define_param(
        kOfxParamTypeDouble,
        kOfxImageEffectTransitionParamName,
        c"Progress",
    );
    p.set_dbl(kOfxParamPropDefault, 0.0);
    p.set_dbl(kOfxParamPropMin, 0.0);
    p.set_dbl(kOfxParamPropMax, 1.0);

    let p = fx.define_param(kOfxParamTypeChoice, SHAPE, c"Gradient");
    p.set_strs(kOfxParamPropChoiceOption, &SHAPES);
    p.set_int(kOfxParamPropDefault, 1);
    p.set_str(
        kOfxParamPropHint,
        c"The gradient: the image Search… picked, or one of BananaV's shapes.",
    );
    let p = fx.define_param(kOfxParamTypePushButton, SEARCH, c"Search…");
    p.set_str(kBananaVParamPropPicture, c"File");
    p.set_str(kBananaVParamPropPictureClip, CLIP);
    p.set_str(
        kOfxParamPropHint,
        c"Picks a gradient image: grayscale ones work best, the transition beginning in the dark areas and ending in the light ones.",
    );
    let toggle = |name: &CStr, label: &CStr, v: bool, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeBoolean, name, label);
        p.set_int(kOfxParamPropDefault, c_int::from(v));
        p.set_str(kOfxParamPropHint, hint);
    };
    toggle(
        FLIP_H,
        c"Horizontal flip",
        false,
        c"The gradient image turned left to right.",
    );
    toggle(
        FLIP_V,
        c"Vertical flip",
        false,
        c"The gradient image turned top to bottom.",
    );
    toggle(
        MIRROR_H,
        c"Horizontal mirror",
        false,
        c"The gradient's left half mirrored on the right.",
    );
    toggle(
        MIRROR_V,
        c"Vertical mirror",
        false,
        c"The gradient's top half mirrored at the bottom.",
    );
    let number = |name: &CStr, label: &CStr, v: f64, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, v);
        p.set_dbl(kOfxParamPropMin, 0.0);
        p.set_dbl(kOfxParamPropMax, 1.0);
        p.set_dbl(kOfxParamPropDisplayMin, 0.0);
        p.set_dbl(kOfxParamPropDisplayMax, 1.0);
        p.set_dbl(kOfxParamPropIncrement, 0.01);
        p.set_int(kOfxParamPropDigits, 3);
        p.set_str(kOfxParamPropHint, hint);
    };
    number(
        BLEND,
        c"Threshold blend",
        0.1,
        c"How soft the leading edge between the two pictures is: 0 sharp.",
    );
    number(
        BLUR,
        c"Gradient blur",
        0.0,
        c"The gradient blurred, the transition smoother overall.",
    );
    let p = fx.define_param(kOfxParamTypeChoice, DIRECTION, c"Direction");
    p.set_strs(kOfxParamPropChoiceOption, &DIRECTIONS);
    p.set_int(kOfxParamPropDefault, 0);
    p.set_str(
        kOfxParamPropHint,
        c"Forward begins in the gradient's dark areas, Reverse in its light ones.",
    );
    toggle(
        ASPECT,
        c"Maintain aspect ratio",
        false,
        c"The gradient image kept in its shape, as big as covers the frame (its edges may be cut off); cleared, stretched to the frame.",
    );
    toggle(
        STRETCH,
        c"Auto contrast stretch",
        true,
        c"The gradient's darkest point made black and its lightest white, so the transition begins and ends with it.",
    );
    let p = fx.define_param(kOfxParamTypeDouble, STEREO, c"Stereoscopic 3D depth");
    p.set_dbl(kOfxParamPropDefault, 0.0);
    p.set_dbl(kOfxParamPropMin, -0.5);
    p.set_dbl(kOfxParamPropMax, 0.5);
    p.set_dbl(kOfxParamPropDisplayMin, -0.1);
    p.set_dbl(kOfxParamPropDisplayMax, 0.1);
    p.set_dbl(kOfxParamPropIncrement, 0.01);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"How far apart the two eyes' pictures are in stereoscopic 3D projects (none in others).",
    );
    // The image picked, kept small.
    let p = fx.define_param(kOfxParamTypeString, IMAGE, c"Gradient Image");
    p.set_str(kOfxParamPropDefault, c"");
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_int(kOfxParamPropSecret, 1);
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|v| if v > 0.0 { v } else { 1.0 });
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
    let progress = fx.param(kOfxImageEffectTransitionParamName).f64_at(time);
    let s = Settings::at(fx, time);
    let (origin, frame) = fx.frame();
    let par = if out.par > 0.0 { out.par } else { 1.0 };
    let f = crate::transitions::Frame {
        w: (frame[0] * scale[0] / par).round().max(1.0) as usize,
        h: (frame[1] * scale[1]).round().max(1.0) as usize,
        par,
    };
    let at = (
        (origin[0] * scale[0] / par).round() as c_int,
        (origin[1] * scale[1]).round() as c_int,
    );
    let (a, b) = (Buf::read(from.as_ref()), Buf::read(to.as_ref()));
    let (a, b) = (framed(&a, at, &f), framed(&b, at, &f));
    drop(from);
    drop(to);
    let steps = s.steps(f.w, f.h, f.par);
    let ramp = ramp(progress, s.blend, s.reverse);
    let (w, h) = (f.w as c_int, f.h as c_int);
    let out_straight = out.channels == 4 && out.unpremultiplied;
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(row) = out.row(y) else { return };
        let fy = y - at.1;
        for x in columns.clone() {
            let fx = x - at.0;
            let mut c = if fx < 0 || fy < 0 || fx >= w || fy >= h {
                [0.0; 4]
            } else {
                // The steps' rows go from the top.
                let top = (h - 1 - fy) as usize;
                let m = f32::from(ramp[steps[top * f.w + fx as usize] as usize]) * 0.003_921_569;
                let i = fy as usize * f.w + fx as usize;
                let (pa, pb) = (a[i], b[i]);
                std::array::from_fn(|k| (1.0 - m) * pb[k] + m * pa[k])
            };
            if out_straight && c[3] > 0.0 {
                c = [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]];
            }
            // SAFETY: a row the output has, a column inside its bounds.
            unsafe { crate::colors::put_pixel(&out, row, x, c) };
        }
    };
    for_rows(y1.max(out.bounds[1])..y2.min(out.bounds[3]), &fill);
    drop(out);
    kOfxStatOK
}

/// What the card draws for Gradient Wipe at `progress` in a frame of
/// `size` pixels (`canonical` its size in canonical units: the pixels'
/// shape): a table, its first row the frame's size, then the ramp (a row
/// a step), then the gradient's steps four to a word, four words a row.
pub(crate) fn plan(
    values: crate::accel::Values<'_>,
    canonical: [f64; 2],
    size: [f64; 2],
    progress: f64,
) -> TransitionPlan {
    let num = |n: &CStr| values.nums(n).and_then(|v| v.first().copied());
    let b = |n: &CStr| num(n).is_some_and(|v| v != 0.0);
    let s = Settings {
        shape: num(SHAPE)
            .unwrap_or(1.0)
            .clamp(0.0, SHAPES.len() as f64 - 1.0) as usize,
        image: values.text(IMAGE).and_then(Map::parse),
        flip: [b(FLIP_H), b(FLIP_V)],
        mirror: [b(MIRROR_H), b(MIRROR_V)],
        blend: num(BLEND).unwrap_or(0.1).clamp(0.0, 1.0),
        blur: num(BLUR).unwrap_or(0.0).clamp(0.0, 1.0),
        reverse: num(DIRECTION).unwrap_or(0.0) == 1.0,
        aspect: b(ASPECT),
        stretch: num(STRETCH).is_none_or(|v| v != 0.0),
    };
    let par = if size[0] > 0.0 && size[1] > 0.0 && canonical[1] > 0.0 {
        (canonical[0] / size[0]) / (canonical[1] / size[1])
    } else {
        1.0
    };
    let (w, h) = (size[0].max(1.0) as usize, size[1].max(1.0) as usize);
    let steps = s.steps(w, h, par);
    let mut table = vec![[w as i32, h as i32, 0, 0]];
    table.extend(ramp(progress, s.blend, s.reverse).map(|v| [i32::from(v), 0, 0, 0]));
    for row in steps.chunks(16) {
        let mut words = [0i32; 4];
        for (k, &v) in row.iter().enumerate() {
            words[k / 4] |= i32::from(v) << ((k % 4) * 8);
        }
        table.push(words);
    }
    TransitionPlan {
        kernel: TransitionKernel::GradientWipe,
        f: [0.0; 24],
        i: [0; 4],
        j: [0; 4],
        steps: TransitionSteps::Table { table },
    }
}

/// Gradient Wipe's factory presets: BananaV's shapes.
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: f64| PresetValue::Num(vec![v]);
    let p = |name: &'static str, values: Vec<(&'static str, PresetValue)>| Preset { name, values };
    vec![
        p("Linear", vec![]),
        p("Radial", vec![("shape", num(2.0))]),
        p(
            "Radial In",
            vec![("shape", num(2.0)), ("direction", num(1.0))],
        ),
        p(
            "Soft Diagonal",
            vec![("shape", num(3.0)), ("thresholdBlend", num(0.35))],
        ),
        p(
            "Clock",
            vec![("shape", num(4.0)), ("thresholdBlend", num(0.02))],
        ),
        p("Diamond", vec![("shape", num(5.0))]),
        p(
            "Clouds",
            vec![("shape", num(6.0)), ("thresholdBlend", num(0.2))],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(shape: usize) -> Settings {
        Settings {
            shape,
            image: None,
            flip: [false; 2],
            mirror: [false; 2],
            blend: 0.0,
            blur: 0.0,
            reverse: false,
            aspect: false,
            stretch: true,
        }
    }

    #[test]
    fn the_second_picture_comes_in_from_the_dark() {
        let s = settings(1);
        let steps = s.steps(64, 36, 1.0);
        // Linear: dark on the left. A third of the way, the left third
        // shows the second picture (none of the first).
        let r = ramp(1.0 / 3.0, s.blend, s.reverse);
        let first = |x: usize| r[steps[18 * 64 + x] as usize];
        assert_eq!((first(10), first(40)), (0, 255));
        // Reverse: from the right.
        let r = ramp(1.0 / 3.0, 0.0, true);
        assert_eq!(
            (
                r[steps[18 * 64 + 60] as usize],
                r[steps[18 * 64 + 10] as usize]
            ),
            (0, 255)
        );
        // Its ends: all of the first at 0, none at 1, a soft edge too.
        assert!(ramp(0.0, 0.3, false).iter().all(|&v| v == 255));
        assert!(ramp(1.0, 0.3, false).iter().all(|&v| v == 0));
        // A hard edge at the start already shows the second picture where
        // the gradient is black, as VEGAS's does.
        let hard = ramp(0.0, 0.0, false);
        assert!(hard[0] == 0 && hard[1..].iter().all(|&v| v == 255));
        assert!(ramp(1.0, 0.0, false).iter().all(|&v| v == 0));
        let soft = ramp(0.5, 0.2, false);
        assert!(soft[127] > 0 && soft[127] < 255, "{}", soft[127]);
    }

    #[test]
    fn a_gradient_keeps_as_text() {
        let m = Map::shape(2, 2.0);
        let back = Map::parse(&m.text()).expect("parsed");
        assert_eq!((back.width, back.height), (m.width, m.height));
        assert!(
            back.data
                .iter()
                .zip(&m.data)
                .all(|(a, b)| (a - b).abs() < 0.003)
        );
        assert!(Map::parse("3 2 abc").is_none());
    }

    #[test]
    fn mirrored_flipped_stretched_and_blurred() {
        // Mirrored, the linear gradient's left half shows twice: the right
        // edge as dark as the left.
        let s = Settings {
            mirror: [true, false],
            ..settings(1)
        };
        let steps = s.steps(64, 36, 1.0);
        assert_eq!(steps[64 * 18], steps[64 * 18 + 63]);
        // Flipped: light on the left.
        let s = Settings {
            flip: [true, false],
            ..settings(1)
        };
        let steps = s.steps(64, 36, 1.0);
        assert!(steps[64 * 18] > steps[64 * 18 + 63]);
        // Stretched: from black to white.
        let steps = settings(2).steps(64, 36, 1.0);
        assert_eq!(steps.iter().min(), Some(&0));
        assert_eq!(steps.iter().max(), Some(&255));
        // Blurred: the radial gradient's dark middle lighter.
        let blurred = Settings {
            blur: 0.5,
            ..settings(2)
        }
        .steps(64, 36, 1.0);
        assert!(blurred[18 * 64 + 32] > steps[18 * 64 + 32]);
    }
}
