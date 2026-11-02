//! AI Smoothen, AI Sharpen and AI Dehaze, with VEGAS's panels — Details
//! (Low, Medium, High) and Amount, AI Dehaze's Artifact Reduction group
//! (Border, Gain, Offset) — and the frame BananaV puts around the work: the
//! alpha set aside; the picture framed by a reflected margin of Border ×
//! its shorter side (0.25 for the two without the group, their
//! processor's), taken as `(x + 0.1 Offset) Gain` (1 and 0 without the
//! group); a small copy of it, the guide, brought down bilinearly
//! (half-pixel centers, between four pixels) to the size Details picks;
//! what is worked out on that copy taken back to the full picture by a
//! guided filter's layer — each channel's local linear map (`a`, `b`:
//! 3 × 3 means, ε 10⁻⁴) of the small guide to the small result, averaged
//! again and enlarged corners aligned, applied to the full picture —;
//! then Amount of the way from the picture to that, held within 0 and its
//! alpha, the alpha kept. Where trained networks would work, BananaV
//! works the small copy out itself: Smoothen a binomial 5 × 5,
//! Sharpen its difference from a binomial 3 × 3 twice over, Dehaze the
//! dark channel's way (He, Sun and Tang's: the air light the mean color
//! where the 3 × 3 dark channel is within 1 % of its brightest, the
//! transmission 1 − 0.95 × the dark channel of the colors over it, held at
//! 0.1). On the card `Plan::Guided`: `kit_guided_low`, `kit_guided_op`
//! (Dehaze's `kit_dehaze_dark`, `kit_dehaze_air`, `kit_dehaze_op`),
//! `kit_guided_coef`, `kit_guided_mean`, `kit_guided_apply`.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::{Plan, Values};
use crate::kit::Buf;
use crate::ofx::{Fx, Props, is, suites};

pub(crate) const SMOOTHEN_ID: &CStr = c"org.bananav.AiSmoothen";
pub(crate) const SHARPEN_ID: &CStr = c"org.bananav.AiSharpen";
pub(crate) const DEHAZE_ID: &CStr = c"org.bananav.AiDehaze";

const DETAILS: &CStr = c"Details";
const STRENGTH: &CStr = c"Strength";
const GROUP: &CStr = c"Group";
const BORDER: &CStr = c"Border";
const GAIN: &CStr = c"Gain";
const OFFSET: &CStr = c"Offset";

const LEVELS: [&CStr; 3] = [c"Low", c"Medium", c"High"];

/// The guided filter's regularization.
const EPS: f32 = 1e-4;

/// Which of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Smoothen,
    Sharpen,
    Dehaze,
}

impl Kind {
    fn of(id: &str) -> Option<Kind> {
        if Some(id) == SMOOTHEN_ID.to_str().ok() {
            Some(Kind::Smoothen)
        } else if Some(id) == SHARPEN_ID.to_str().ok() {
            Some(Kind::Sharpen)
        } else if Some(id) == DEHAZE_ID.to_str().ok() {
            Some(Kind::Dehaze)
        } else {
            None
        }
    }

    /// The small copy's size for each Details (columns, rows).
    fn low(self, details: c_int) -> [usize; 2] {
        let sizes = match self {
            Kind::Dehaze => [[96, 64], [192, 128], [384, 256]],
            _ => [[95, 64], [191, 128], [382, 256]],
        };
        sizes[details.clamp(0, 2) as usize]
    }
}

/// What a frame is worked out with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    kind: Kind,
    details: c_int,
    strength: f32,
    border: f32,
    gain: f32,
    offset: f32,
}

impl Settings {
    fn make(
        kind: Kind,
        details: c_int,
        strength: f64,
        [border, gain, offset]: [f64; 3],
    ) -> Settings {
        Settings {
            kind,
            details: details.clamp(0, 2),
            strength: strength as f32,
            border: border.clamp(0.0, 0.9) as f32,
            gain: gain as f32,
            offset: offset as f32,
        }
    }

    fn at(kind: Kind, fx: Fx, time: f64) -> Settings {
        let reduction = if kind == Kind::Dehaze {
            [BORDER, GAIN, OFFSET].map(|n| fx.param(n).f64_at(time))
        } else {
            [0.25, 1.0, 0.0]
        };
        Settings::make(
            kind,
            fx.param(DETAILS).int_at(time),
            fx.param(STRENGTH).f64_at(time),
            reduction,
        )
    }

    pub(crate) fn of_values(id: &str, v: Values<'_>) -> Option<Settings> {
        let kind = Kind::of(id)?;
        let reduction = if kind == Kind::Dehaze {
            [v.f64(BORDER, 0.25), v.f64(GAIN, 1.0), v.f64(OFFSET, 0.0)]
        } else {
            [0.25, 1.0, 0.0]
        };
        Some(Settings::make(
            kind,
            v.int(DETAILS, 1),
            v.f64(STRENGTH, 0.5),
            reduction,
        ))
    }

    fn changes_nothing(&self) -> bool {
        self.strength == 0.0
    }

    /// The card's numbers: Border, Gain, Offset, Amount; the small copy's
    /// size and what is worked out on it.
    fn numbers(&self) -> ([f32; 4], [u32; 3]) {
        let [lw, lh] = self.kind.low(self.details);
        (
            [self.border, self.gain, self.offset, self.strength],
            [lw as u32, lh as u32, self.kind as u32],
        )
    }

    pub(crate) fn plan(&self) -> Plan {
        if self.changes_nothing() {
            return Plan::Same;
        }
        let (numbers, low) = self.numbers();
        Plan::Guided { numbers, low }
    }
}

/// The margin, in pixels, of a `w` × `h` picture.
pub(crate) fn margin(border: f32, w: usize, h: usize) -> usize {
    (w.min(h) as f32 * border).floor().max(0.0) as usize
}

/// `i` reflected into `0..n` about its ends (the edge pixel repeated).
fn reflect(i: isize, n: usize) -> usize {
    let n = n as isize;
    let mut i = i;
    while i < 0 || i >= n {
        i = if i < 0 { -i - 1 } else { 2 * n - i - 1 };
    }
    i as usize
}

/// Where output index `i` of `n` reads a line `size` long, bilinearly
/// with half-pixel centers: the first pixel and the second's share.
fn resize_tap(i: usize, n: usize, size: usize) -> (usize, f32) {
    let scale = size as f64 / n as f64;
    let f = ((i as f64 + 0.5) * scale - 0.5) as f32;
    let s = f.floor();
    if s < 0.0 {
        (0, 0.0)
    } else if s as usize >= size - 1 {
        (size - 1, 0.0)
    } else {
        (s as usize, f - s)
    }
}

/// The small copy (rows up, `lw` × `lh`) of the framed, adjusted picture.
fn low_copy(src: &[[f32; 4]], [w, h]: [usize; 2], s: &Settings) -> (Vec<[f32; 3]>, [usize; 2]) {
    let [lw, lh] = s.kind.low(s.details);
    let b = margin(s.border, w, h);
    let (pw, ph) = (w + 2 * b, h + 2 * b);
    let high = |x: usize, y: usize| {
        let p = src[reflect(y as isize - b as isize, h) * w + reflect(x as isize - b as isize, w)];
        [0, 1, 2].map(|c| (p[c] + 0.1 * s.offset) * s.gain)
    };
    let columns: Vec<(usize, f32)> = (0..lw).map(|i| resize_tap(i, lw, pw)).collect();
    let mut out = Vec::with_capacity(lw * lh);
    for j in 0..lh {
        let (y0, fy) = resize_tap(j, lh, ph);
        let y1 = (y0 + 1).min(ph - 1);
        for &(x0, fx) in &columns {
            let x1 = (x0 + 1).min(pw - 1);
            let (a, b2, c, d) = (high(x0, y0), high(x1, y0), high(x0, y1), high(x1, y1));
            out.push([0, 1, 2].map(|k| {
                let top = a[k] * (1.0 - fx) + b2[k] * fx;
                let bottom = c[k] * (1.0 - fx) + d[k] * fx;
                top * (1.0 - fy) + bottom * fy
            }));
        }
    }
    (out, [lw, lh])
}

/// A small grid's value at `(i, j)`, its edges held.
fn held<T: Copy>(g: &[T], [w, h]: [usize; 2], i: isize, j: isize) -> T {
    g[j.clamp(0, h as isize - 1) as usize * w + i.clamp(0, w as isize - 1) as usize]
}

/// The dark channel's air light of a small copy: the mean color where its
/// 3 × 3 dark channel is within 1 % of its brightest.
fn air_light(x: &[[f32; 3]], size: [usize; 2]) -> [f32; 3] {
    let [w, _] = size;
    let dark: Vec<f32> = (0..x.len())
        .map(|o| {
            let (i, j) = ((o % w) as isize, (o / w) as isize);
            let mut d = f32::MAX;
            for dj in -1..=1 {
                for di in -1..=1 {
                    let p = held(x, size, i + di, j + dj);
                    d = d.min(p[0].min(p[1].min(p[2])));
                }
            }
            d
        })
        .collect();
    let top = dark.iter().fold(0.0f32, |a, &b| a.max(b)) * 0.99;
    let (mut sum, mut n) = ([0.0f32; 3], 0.0f32);
    for (p, &d) in x.iter().zip(&dark) {
        if d >= top {
            for c in 0..3 {
                sum[c] += p[c];
            }
            n += 1.0;
        }
    }
    sum.map(|v| v / n.max(1.0))
}

/// What BananaV works out on the small copy (VEGAS's network's place).
fn worked(kind: Kind, x: &[[f32; 3]], size: [usize; 2]) -> Vec<[f32; 3]> {
    let [w, _] = size;
    let air = if kind == Kind::Dehaze {
        air_light(x, size).map(|v| v.max(1e-6))
    } else {
        [1.0; 3]
    };
    (0..x.len())
        .map(|o| {
            let (i, j) = ((o % w) as isize, (o / w) as isize);
            let at = |di: isize, dj: isize| held(x, size, i + di, j + dj);
            match kind {
                Kind::Smoothen => {
                    const K: [f32; 5] = [1.0, 4.0, 6.0, 4.0, 1.0];
                    let mut acc = [0.0f32; 3];
                    for (dj, wy) in (-2..=2).zip(K) {
                        let mut row = [0.0f32; 3];
                        for (di, wx) in (-2..=2).zip(K) {
                            let p = at(di, dj);
                            for c in 0..3 {
                                row[c] += p[c] * wx;
                            }
                        }
                        for c in 0..3 {
                            acc[c] += row[c] * wy;
                        }
                    }
                    acc.map(|v| v * (1.0 / 256.0))
                }
                Kind::Sharpen => {
                    let p = at(0, 0);
                    let row =
                        |dj: isize, c: usize| at(-1, dj)[c] + at(0, dj)[c] * 2.0 + at(1, dj)[c];
                    std::array::from_fn(|c| {
                        let blur = (row(-1, c) + row(0, c) * 2.0 + row(1, c)) * 0.0625;
                        p[c] + (p[c] - blur) * 2.0
                    })
                }
                Kind::Dehaze => {
                    let mut dark = f32::MAX;
                    for dj in -1..=1 {
                        for di in -1..=1 {
                            let q = at(di, dj);
                            let v = [q[0] / air[0], q[1] / air[1], q[2] / air[2]];
                            dark = dark.min(v[0].min(v[1].min(v[2])));
                        }
                    }
                    let t = (1.0 - 0.95 * dark).max(0.1);
                    let p = at(0, 0);
                    std::array::from_fn(|c| (p[c] - air[c]) / t + air[c])
                }
            }
        })
        .collect()
}

/// The 3 × 3 mean of a small grid's values, its edges held.
fn mean3(g: &[[f32; 3]], size: [usize; 2], o: usize) -> [f32; 3] {
    let [w, _] = size;
    let (i, j) = ((o % w) as isize, (o / w) as isize);
    let mut acc = [0.0f32; 3];
    for dj in -1..=1 {
        for di in -1..=1 {
            let p = held(g, size, i + di, j + dj);
            for c in 0..3 {
                acc[c] += p[c];
            }
        }
    }
    acc.map(|v| v * (1.0 / 9.0))
}

/// The guided filter's layer on the small grids: each channel's `a` and
/// `b` (means of the local linear maps of `x` to `p`), averaged again.
fn coefficients(
    x: &[[f32; 3]],
    p: &[[f32; 3]],
    size: [usize; 2],
) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    let n = x.len();
    let xx: Vec<[f32; 3]> = x.iter().map(|v| v.map(|c| c * c)).collect();
    let xp: Vec<[f32; 3]> = x
        .iter()
        .zip(p)
        .map(|(a, b)| [a[0] * b[0], a[1] * b[1], a[2] * b[2]])
        .collect();
    let mut a = vec![[0.0f32; 3]; n];
    let mut b = vec![[0.0f32; 3]; n];
    for o in 0..n {
        let (mx, mp, mxx, mxp) = (
            mean3(x, size, o),
            mean3(p, size, o),
            mean3(&xx, size, o),
            mean3(&xp, size, o),
        );
        for c in 0..3 {
            let var = mxx[c] - mx[c] * mx[c];
            let cov = mxp[c] - mx[c] * mp[c];
            a[o][c] = cov / (var + EPS);
            b[o][c] = mp[c] - a[o][c] * mx[c];
        }
    }
    let ma = (0..n).map(|o| mean3(&a, size, o)).collect();
    let mb = (0..n).map(|o| mean3(&b, size, o)).collect();
    (ma, mb)
}

/// A small grid read at `(lx, ly)` between its four values (corners
/// aligned with the full picture's).
fn upsampled(g: &[[f32; 3]], size: [usize; 2], lx: f32, ly: f32) -> [f32; 3] {
    let [w, h] = size;
    let (x0, y0) = (
        lx.floor().clamp(0.0, (w - 1) as f32),
        ly.floor().clamp(0.0, (h - 1) as f32),
    );
    let (fx, fy) = ((lx - x0).clamp(0.0, 1.0), (ly - y0).clamp(0.0, 1.0));
    let (i, j) = (x0 as usize, y0 as usize);
    let (i1, j1) = ((i + 1).min(w - 1), (j + 1).min(h - 1));
    let (a, b, c, d) = (g[j * w + i], g[j * w + i1], g[j1 * w + i], g[j1 * w + i1]);
    std::array::from_fn(|k| {
        let top = a[k] * (1.0 - fx) + b[k] * fx;
        let bottom = c[k] * (1.0 - fx) + d[k] * fx;
        top * (1.0 - fy) + bottom * fy
    })
}

/// The small grids' coefficients of a picture, as the card reads them:
/// each small pixel's `a` then `b` (rows up), and the grid's size.
fn small_coefficients(
    src: &[[f32; 4]],
    size: [usize; 2],
    s: &Settings,
) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, [usize; 2]) {
    let (x, low) = low_copy(src, size, s);
    let p = worked(s.kind, &x, low);
    let (a, b) = coefficients(&x, &p, low);
    (a, b, low)
}

/// The picture worked out.
fn processed(source: &Buf, s: &Settings) -> Buf {
    let (w, h) = (
        source.width().max(0) as usize,
        source.height().max(0) as usize,
    );
    if w < 2 || h < 2 || s.changes_nothing() {
        return source.clone();
    }
    let (a, b, low) = small_coefficients(&source.data, [w, h], s);
    let m = margin(s.border, w, h);
    let (pw, ph) = (w + 2 * m, h + 2 * m);
    let (kx, ky) = (
        (low[0] - 1) as f32 / (pw - 1) as f32,
        (low[1] - 1) as f32 / (ph - 1) as f32,
    );
    let [bx, by] = [source.bounds[0], source.bounds[1]];
    source.map(|x, y| {
        let (i, j) = ((x - bx) as usize, (y - by) as usize);
        let c = source.data[j * w + i];
        let (lx, ly) = ((i + m) as f32 * kx, (j + m) as f32 * ky);
        let (ca, cb) = (upsampled(&a, low, lx, ly), upsampled(&b, low, lx, ly));
        let mut out = [0.0f32; 4];
        for k in 0..3 {
            let guide = (c[k] + 0.1 * s.offset) * s.gain;
            let q = ca[k] * guide + cb[k];
            out[k] = ((1.0 - s.strength) * c[k] + s.strength * q).clamp(0.0, c[3].max(0.0));
        }
        out[3] = c[3];
        out
    })
}

/// The card's small copy's size and the margin, in pixels, for a `w` ×
/// `h` picture: what its kernels lay out.
pub(crate) fn card_layout(numbers: &[f32; 4], low: [u32; 3], w: u32, h: u32) -> ([u32; 2], u32) {
    let m = margin(numbers[0], w as usize, h as usize) as u32;
    ([low[0], low[1]], m)
}

macro_rules! entry {
    ($($name:ident => $kind:expr;)*) => {
        $(
            /// The plug-in's entry point.
            pub(crate) unsafe extern "C" fn $name(
                action: *const c_char,
                handle: *const c_void,
                in_args: OfxPropertySetHandle,
                out_args: OfxPropertySetHandle,
            ) -> OfxStatus {
                main($kind, action, handle, in_args, out_args)
            }
        )*
    };
}

entry! {
    smoothen_main => Kind::Smoothen;
    sharpen_main => Kind::Sharpen;
    dehaze_main => Kind::Dehaze;
}

fn main(
    kind: Kind,
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
        describe(kind, fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        describe_in_context(kind, fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        render(kind, fx, args)
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        let time = args.dbl(kOfxPropTime, 0);
        if Settings::at(kind, fx, time).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        let time = args.dbl(kOfxPropTime, 0);
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        let all = fx
            .clip(kOfxImageEffectSimpleSourceClipName)
            .rod(time)
            .unwrap_or(roi);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &all);
        kOfxStatOK
    } else if is(action, kOfxActionLoad)
        || is(action, kOfxActionUnload)
        || is(action, kOfxActionCreateInstance)
        || is(action, kOfxActionDestroyInstance)
        || is(action, kOfxActionInstanceChanged)
    {
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

fn describe(kind: Kind, fx: Fx) {
    let p = fx.props();
    let (label, description) = match kind {
        Kind::Smoothen => (c"AI Smoothen", c"AI Smoothen."),
        Kind::Sharpen => (c"AI Sharpen", c"AI Sharpen."),
        Kind::Dehaze => (c"AI Dehaze", c"AI Dehaze."),
    };
    p.set_str(kOfxPropLabel, label);
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/AI");
    p.set_str(kOfxPropPluginDescription, description);
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextFilter],
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

fn describe_in_context(kind: Kind, fx: Fx) {
    let comps = [kOfxImageComponentRGBA];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);
    let p = fx.define_param(kOfxParamTypeChoice, DETAILS, c"Details");
    p.set_strs(kOfxParamPropChoiceOption, &LEVELS);
    p.set_int(kOfxParamPropDefault, 1);
    let p = fx.define_param(kOfxParamTypeDouble, STRENGTH, c"Amount");
    p.set_dbl(kOfxParamPropDefault, 0.5);
    p.set_dbl(kOfxParamPropMin, -f64::MAX);
    p.set_dbl(kOfxParamPropMax, f64::MAX);
    p.set_dbl(kOfxParamPropDisplayMin, 0.0);
    p.set_dbl(kOfxParamPropDisplayMax, 1.0);
    p.set_str(
        kOfxParamPropHint,
        match kind {
            Kind::Smoothen => c"Slide to adjust the smoothen effect.",
            Kind::Sharpen => c"Slide to adjust the sharpen effect.",
            Kind::Dehaze => c"Slide to adjust the dehaze effect.",
        },
    );
    if kind != Kind::Dehaze {
        return;
    }
    fx.define_param(kOfxParamTypeGroup, GROUP, c"Artifact Reduction");
    for (name, label, default, [min, max], [shown_min, shown_max]) in [
        (BORDER, c"Border", 0.25, [0.0, 0.9], [0.0, 0.3]),
        (GAIN, c"Gain", 1.0, [-f64::MAX, f64::MAX], [0.7, 1.2]),
        (OFFSET, c"Offset", 0.0, [-f64::MAX, f64::MAX], [-2.0, 2.0]),
    ] {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, default);
        p.set_dbl(kOfxParamPropMin, min);
        p.set_dbl(kOfxParamPropMax, max);
        p.set_dbl(kOfxParamPropDisplayMin, shown_min);
        p.set_dbl(kOfxParamPropDisplayMax, shown_max);
        p.set_str(kOfxParamPropParent, GROUP);
    }
}

fn render(kind: Kind, fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    if let Some(s) = &src
        && (s.depth != out.depth || s.channels != out.channels)
    {
        return kOfxStatErrImageFormat;
    }
    let source = Buf::read(src.as_ref());
    drop(src);
    let result = processed(&source, &Settings::at(kind, fx, time));
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(w: usize, h: usize, f: impl Fn(usize, usize) -> f32) -> Buf {
        Buf {
            bounds: [0, 0, w as c_int, h as c_int],
            data: (0..w * h)
                .map(|i| {
                    let v = f(i % w, i / w);
                    [v, v, v, 1.0]
                })
                .collect(),
        }
    }

    #[test]
    fn opencv_reflects_and_resizes() {
        assert_eq!(
            [-1, -2, 0, 3, 4, 5].map(|i| reflect(i, 4)),
            [0, 1, 0, 3, 3, 2]
        );
        // Halving: each output reads the middle of two pixels.
        assert_eq!(resize_tap(0, 2, 4), (0, 0.5));
        assert_eq!(resize_tap(1, 2, 4), (2, 0.5));
        // Past the last pixel: held.
        assert_eq!(resize_tap(3, 4, 2), (1, 0.0));
    }

    #[test]
    fn a_flat_picture_stays_flat() {
        for kind in [Kind::Smoothen, Kind::Sharpen, Kind::Dehaze] {
            let src = buf(40, 30, |_, _| 0.4);
            let out = processed(&src, &Settings::make(kind, 1, 1.0, [0.25, 1.0, 0.0]));
            assert!(
                out.data.iter().all(|c| (c[0] - 0.4).abs() < 1e-4),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn smoothen_calms_noise_and_sharpen_lifts_an_edge() {
        let noisy = buf(200, 120, |x, y| {
            0.5 + if (x * 7 + y * 13) % 5 == 0 { 0.02 } else { 0.0 }
        });
        let out = processed(
            &noisy,
            &Settings::make(Kind::Smoothen, 1, 1.0, [0.25, 1.0, 0.0]),
        );
        let spread = |b: &Buf| {
            let m = b.data.iter().map(|c| c[0]).sum::<f32>() / b.data.len() as f32;
            b.data.iter().map(|c| (c[0] - m).abs()).sum::<f32>() / b.data.len() as f32
        };
        assert!(
            spread(&out) < spread(&noisy) * 0.8,
            "{} {}",
            spread(&out),
            spread(&noisy)
        );
        let edge = buf(200, 120, |x, _| if x < 100 { 0.3 } else { 0.7 });
        let out = processed(
            &edge,
            &Settings::make(Kind::Sharpen, 1, 1.0, [0.25, 1.0, 0.0]),
        );
        let row = |x: usize| out.data[60 * 200 + x][0];
        let low = (90..100).map(row).fold(1.0f32, f32::min);
        let high = (100..110).map(row).fold(0.0f32, f32::max);
        assert!(low < 0.29 && high > 0.71, "{low} {high}");
    }

    #[test]
    fn dehaze_deepens_a_hazy_picture() {
        // A gray veil over a ramp: dehazed, its contrast grows.
        let hazy = buf(160, 100, |x, _| 0.6 + 0.2 * x as f32 / 160.0);
        let s = Settings::make(Kind::Dehaze, 1, 1.0, [0.25, 1.0, 0.0]);
        let out = processed(&hazy, &s);
        let span = |b: &Buf| b.data[50 * 160 + 150][0] - b.data[50 * 160 + 10][0];
        assert!(
            span(&out) > span(&hazy) * 1.5,
            "{} {}",
            span(&out),
            span(&hazy)
        );
    }
}
