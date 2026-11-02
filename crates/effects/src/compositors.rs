//! VEGAS's compositors: 2-to-1 plug-ins a track's Custom compositing mode
//! runs (its Compositors window). The track's picture — a compositing
//! parent's, over its children — is the map (the `Map` input) that guides
//! how what is below it (`Source`: the children's group) is drawn:
//! Displacement Map moves the source's pixels by the map's channels,
//! Height Map bends them as light through a water surface the map's
//! heights shape, and Bump Map (in `looks.rs`, a video effect too) lights
//! them by the map's bumps. OpenFX's General context with a second input;
//! their settings, presets and sums VEGAS's, in its 8-bit steps.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::kit::{Buf, Spec, Vals, define_all};
use crate::ofx::{Fx, Props, for_rows, is, suites};
use crate::{Preset, PresetValue};

/// The input that guides a compositor: the track's picture.
pub(crate) const MAP_CLIP: &CStr = c"Map";

/// One of the compositors written here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    DisplacementMap,
    HeightMap,
}

const RGB: &[&CStr] = &[c"Red", c"Green", c"Blue"];
const EDGES: &[&CStr] = &[c"Transparent background", c"Wrap pixels around"];
const EDGE: Spec = Spec::Choice(
    c"edgePixelHandling",
    c"Edge pixel handling",
    EDGES,
    0,
    c"What shows where pixels would come from outside the frame: nothing, or the picture tiled.",
);
const FLIP: Spec = Spec::Toggle(
    c"flip",
    c"Flip input surfaces",
    false,
    c"Swaps the picture and the map.",
);

const DISPLACEMENT_MAP: &[Spec] = &[
    Spec::Group(c"scale", c"Scale"),
    Spec::Number(
        c"scaleHorizontal",
        c"Horizontal",
        0.1,
        0.0,
        1.0,
        c"How far pixels move across: 1 is the frame's width.",
    ),
    Spec::Number(
        c"scaleVertical",
        c"Vertical",
        0.1,
        0.0,
        1.0,
        c"How far pixels move up or down: 1 is the frame's height.",
    ),
    Spec::GroupEnd,
    EDGE,
    Spec::Group(c"channels", c"Channels"),
    Spec::Choice(
        c"xAxisChannel",
        c"X axis channel",
        RGB,
        0,
        c"The map's channel that moves pixels across: 128 leaves them, brighter brings them from the right, darker from the left.",
    ),
    Spec::Toggle(
        c"xAxisInvert",
        c"X axis invert",
        false,
        c"Moves pixels across the other way.",
    ),
    Spec::Choice(
        c"yAxisChannel",
        c"Y axis channel",
        RGB,
        1,
        c"The map's channel that moves pixels up or down: 128 leaves them, brighter brings them from below, darker from above.",
    ),
    Spec::Toggle(
        c"yAxisInvert",
        c"Y axis invert",
        false,
        c"Moves pixels up or down the other way.",
    ),
    Spec::GroupEnd,
    Spec::Toggle(
        c"alphaScales",
        c"Alpha channel scales RGB channels",
        false,
        c"Where the map is see-through, pixels move less.",
    ),
    FLIP,
];

const HEIGHT_MAP: &[Spec] = &[
    Spec::Group(c"heightModifiers", c"Height modifiers"),
    Spec::Number(
        c"amplitude",
        c"Amplitude",
        0.1,
        0.0,
        1.0,
        c"How high the map's brightest parts stand: 1 is the frame's longer side.",
    ),
    Spec::Number(
        c"elevation",
        c"Elevation",
        0.1,
        0.0,
        1.0,
        c"How deep below the surface the picture lies: deeper bends it more.",
    ),
    Spec::Number(
        c"heightScale",
        c"Height scale",
        1.0,
        0.0,
        1.0,
        c"How much the surface's own height adds to the picture's depth.",
    ),
    Spec::GroupEnd,
    EDGE,
    Spec::Choice(
        c"sourceChannel",
        c"Source channel",
        &[c"Intensity", c"Red", c"Green", c"Blue"],
        0,
        c"The map's channel the heights come from: Intensity averages red, green and blue.",
    ),
    Spec::Int(
        c"smoothness",
        c"Smoothness",
        14,
        1,
        31,
        c"How much the surface is smoothed.",
    ),
    Spec::Toggle(
        c"invert",
        c"Invert data source",
        false,
        c"Low parts high and high parts low.",
    ),
    Spec::Toggle(
        c"alphaScales",
        c"Alpha channel scales RGB channels",
        false,
        c"Where the map is see-through, it is lower.",
    ),
    FLIP,
];

impl Kind {
    pub(crate) const ALL: [Kind; 2] = [Kind::DisplacementMap, Kind::HeightMap];

    pub(crate) fn id(self) -> &'static CStr {
        match self {
            Kind::DisplacementMap => c"org.bananav.DisplacementMap",
            Kind::HeightMap => c"org.bananav.HeightMap",
        }
    }

    fn label(self) -> &'static CStr {
        match self {
            Kind::DisplacementMap => c"Displacement Map",
            Kind::HeightMap => c"Height Map",
        }
    }

    fn description(self) -> &'static CStr {
        match self {
            Kind::DisplacementMap => {
                c"Moves the pixels of what is below the track as the track's picture says: its channels push them across and up or down, middle gray leaving them."
            }
            Kind::HeightMap => {
                c"Bends what is below the track as light through water whose surface the track's picture shapes: lighter is higher, and the slopes move the pixels."
            }
        }
    }

    pub(crate) fn specs(self) -> &'static [Spec] {
        match self {
            Kind::DisplacementMap => DISPLACEMENT_MAP,
            Kind::HeightMap => HEIGHT_MAP,
        }
    }

    /// The kind whose identifier is `id`.
    pub(crate) fn of(id: &str) -> Option<Kind> {
        Kind::ALL
            .into_iter()
            .find(|k| k.id().to_str().ok() == Some(id))
    }

    /// `source` drawn as `map` guides it.
    fn process(self, s: &Vals, source: &Buf, map: &Buf) -> Buf {
        match self {
            Kind::DisplacementMap => displacement(s, source, map),
            Kind::HeightMap => height(s, source, map),
        }
    }

    /// The numbers both the processor and the card draw settings `s`
    /// with (`accel::compositor_plan`).
    pub(crate) fn card_plan(self, s: &Vals) -> crate::accel::CompositorPlan {
        use crate::accel::{CompositorKernel, CompositorPlan};
        let wrap = u32::from(s.i(c"edgePixelHandling") == 1);
        let alpha_scales = u32::from(s.b(c"alphaScales"));
        match self {
            Kind::DisplacementMap => {
                // Each scale a part of the picture's size, the other way
                // when inverted.
                let scale = |name: &CStr, invert: &CStr| {
                    let sign = if s.b(invert) { -1.0 } else { 1.0 };
                    s.f(name).clamp(0.0, 1.0) as f32 * sign
                };
                let channel = |name: &CStr| s.i(name).clamp(0, 2) as u32;
                CompositorPlan {
                    kernel: CompositorKernel::Displacement,
                    // VEGAS counts rows from the top: a brighter map
                    // brings pixels from below (rows go up here).
                    f: vec![
                        scale(c"scaleHorizontal", c"xAxisInvert"),
                        -scale(c"scaleVertical", c"yAxisInvert"),
                    ],
                    j: [
                        channel(c"xAxisChannel"),
                        channel(c"yAxisChannel"),
                        alpha_scales,
                        wrap,
                    ],
                    flip: s.b(c"flip"),
                    smoothness: 0,
                }
            }
            Kind::HeightMap => CompositorPlan {
                kernel: CompositorKernel::Height,
                f: [c"amplitude", c"elevation", c"heightScale"]
                    .map(|n| s.f(n).clamp(0.0, 1.0) as f32)
                    .to_vec(),
                j: [
                    s.i(c"sourceChannel").clamp(0, 3) as u32,
                    u32::from(s.b(c"invert")),
                    alpha_scales,
                    wrap,
                ],
                flip: s.b(c"flip"),
                smoothness: s.i(c"smoothness"),
            },
        }
    }
}

// VEGAS's compositors work their maps out in 8-bit steps, premultiplied
// as its pictures hold them: the card does the same sums, so both draw
// the same.

/// Channel `ch` (0 red, 1 green, 2 blue, 3 alpha) of `map` at pixel x,
/// y — edges held — in 8-bit steps.
fn byte(map: &Buf, x: c_int, y: c_int, ch: usize) -> i32 {
    crate::kit::step8(map.at(x, y)[ch]) as i32
}

/// The neighbours a map's pixel is averaged with: its diagonal ones.
const DIAGONALS: [(c_int, c_int); 4] = [(-1, -1), (1, -1), (-1, 1), (1, 1)];

/// 128 ⁄ 255, the middle gray VEGAS's maps leave pixels at.
const GRAY: f32 = 0.501_960_8;

/// Displacement Map's move along channel `ch` at map pixel x, y, in parts
/// of the scale's reach: the pixel counted twice with its four diagonal
/// neighbours (edges held) less the middle gray, averaged where they
/// differ little — their squared differences under 101 steps —, else the
/// pixel alone; in 128ths of a step. The map's straight colors where it
/// is see-through, unless its alpha scales them (`alpha_scales`).
pub(crate) fn displacement_at(map: &Buf, x: c_int, y: c_int, ch: usize, alpha_scales: bool) -> f32 {
    let at = |(dx, dy): (c_int, c_int), c: usize| byte(map, x + dx, y + dy, c);
    let alpha = at((0, 0), 3);
    let alphas = 2 * alpha + DIAGONALS.iter().map(|&d| at(d, 3)).sum::<i32>();
    let c = at((0, 0), ch);
    let n = DIAGONALS.map(|d| at(d, ch));
    if alpha_scales || alphas >= 1530 {
        let spread: i32 = n.iter().map(|&v| (v - c) * (v - c)).sum();
        let v = if spread < 101 {
            let sum = 2 * c + n[0] + n[1] + n[2] + n[3];
            let off = if alphas == 1530 {
                (sum - 768) as f32
            } else {
                sum as f32 - alphas as f32 * GRAY
            };
            off * 0.166_666_67
        } else {
            c as f32 - alpha as f32 * GRAY
        };
        return v * 0.007_812_5;
    }
    // Each value by its own alpha, in 0–1.
    let unit = |(dx, dy): (c_int, c_int)| {
        let a = at((dx, dy), 3);
        if a == 0 {
            0.0
        } else {
            at((dx, dy), ch) as f32 * 0.003_921_569 * (1.0 / (a as f32 * 0.003_921_569))
        }
    };
    let k = unit((0, 0));
    let m = DIAGONALS.map(unit);
    let spread = (m[0] - k) * (m[0] - k)
        + (m[1] - k) * (m[1] - k)
        + (m[2] - k) * (m[2] - k)
        + (m[3] - k) * (m[3] - k);
    let v = if spread <= 0.001_537_87 {
        (k + k + m[0] + m[1] + m[2] + m[3] - 3.011_765) * 0.166_666_67
    } else {
        k - GRAY
    };
    v * 1.992_187_5
}

/// `src` at x, y — pixels from its first, rows up — between its four
/// pixels, nothing outside it or (`wrap`) tiled: VEGAS's compositors'
/// sampling, its weights in its order.
fn sample(src: &Buf, x: f32, y: f32, wrap: bool) -> [f32; 4] {
    let (w, h) = (src.width(), src.height());
    let texel = |xi: c_int, yi: c_int| -> [f32; 4] {
        let (xi, yi) = if wrap {
            (xi.rem_euclid(w), yi.rem_euclid(h))
        } else if xi < 0 || xi >= w || yi < 0 || yi >= h {
            return [0.0; 4];
        } else {
            (xi, yi)
        };
        src.data[(yi * w + xi) as usize]
    };
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (xi, yi) = (x0 as c_int, y0 as c_int);
    let (a, b, c, d) = (
        texel(xi, yi),
        texel(xi + 1, yi),
        texel(xi, yi + 1),
        texel(xi + 1, yi + 1),
    );
    std::array::from_fn(|i| {
        b[i] * ((1.0 - fy) * fx)
            + a[i] * ((1.0 - fx) * (1.0 - fy))
            + c[i] * ((1.0 - fx) * fy)
            + d[i] * (fy * fx)
    })
}

/// Displacement Map, as VEGAS's: each pixel shows the picture as far
/// across and down (rows counted from the top, as VEGAS counts them) as
/// the map's channels say there, times the scales' reach (1: the
/// picture's width or height).
fn displacement(s: &Vals, source: &Buf, map: &Buf) -> Buf {
    let plan = Kind::DisplacementMap.card_plan(s);
    let reach = [
        plan.f[0] * source.width() as f32,
        plan.f[1] * source.height() as f32,
    ];
    let channels = [plan.j[0] as usize, plan.j[1] as usize];
    let (alpha_scales, wrap) = (plan.j[2] != 0, plan.j[3] != 0);
    let [x1, y1, ..] = source.bounds;
    source.map(|x, y| {
        let dx = displacement_at(map, x, y, channels[0], alpha_scales) * reach[0];
        let dy = displacement_at(map, x, y, channels[1], alpha_scales) * reach[1];
        sample(source, (x - x1) as f32 + dx, (y - y1) as f32 + dy, wrap)
    })
}

/// Height Map's height at map pixel `p` in fixed point — 765ths of the
/// full height, `bits` more below: its channel (0 the three's sum, 1 red,
/// 2 green, 3 blue — a byte counting three) in 8-bit steps, straight
/// unless the alpha scales it; inverted, the full height less it. Exact
/// integers, so that the processor and the card smooth them alike: the
/// slopes magnify the smallest difference.
fn height_fixed(p: [f32; 4], j: [u32; 4], bits: u32) -> i32 {
    let b = crate::kit::step8;
    let (r, g, bl, a) = (b(p[0]), b(p[1]), b(p[2]), b(p[3]));
    let (sum, n) = match j[0] {
        1 => (r, 1),
        2 => (g, 1),
        3 => (bl, 1),
        _ => (r + g + bl, 3),
    };
    let full = 765 << bits;
    let h = if j[2] != 0 || a == 255 {
        (sum * (3 / n)) << bits
    } else if a == 0 {
        0
    } else {
        // Over its alpha, rounded, no more than the full height.
        let d = n * a;
        let q = (sum * 765).min(d * 765);
        ((q / d) << bits) + (((q % d) << bits) + d / 2) / d
    };
    (if j[1] != 0 { full - h.min(full) } else { h }) as i32
}

/// A plane `w` wide, its rows top first, through a box of radius `r`
/// across — the edge values held, each sum rounded back to the plane's
/// units —, as the card's `kit_box_rows` sums it.
fn box_rows(plane: &[i32], w: usize, r: usize) -> Vec<i32> {
    let n = (2 * r + 1) as i32;
    let r = r as isize;
    let mut out = vec![0i32; plane.len()];
    for (row, line) in out.chunks_mut(w).zip(plane.chunks(w)) {
        let at = |i: isize| line[i.clamp(0, w as isize - 1) as usize];
        let mut acc: i32 = (-r..=r).map(at).sum();
        for (x, o) in row.iter_mut().enumerate() {
            *o = (acc + r as i32) / n;
            acc += at(x as isize + r + 1) - at(x as isize - r);
        }
    }
    out
}

/// The same down the columns, as the card's `kit_box_columns`.
fn box_columns(plane: &[i32], w: usize, r: usize) -> Vec<i32> {
    let h = plane.len() / w.max(1);
    let n = (2 * r + 1) as i32;
    let r = r as isize;
    let row = |i: isize| &plane[i.clamp(0, h as isize - 1) as usize * w..][..w];
    let mut acc = vec![0i32; w];
    for i in -r..=r {
        for (a, v) in acc.iter_mut().zip(row(i)) {
            *a += v;
        }
    }
    let mut out = vec![0i32; plane.len()];
    for (y, line) in out.chunks_mut(w).enumerate() {
        for (o, a) in line.iter_mut().zip(&acc) {
            *o = (a + r as i32) / n;
        }
        let (add, sub) = (row(y as isize + r + 1), row(y as isize - r));
        for ((a, p), m) in acc.iter_mut().zip(add).zip(sub) {
            *a += p - m;
        }
    }
    out
}

/// How many times Height Map's box smooths the heights each way.
pub(crate) const HEIGHT_PASSES: usize = 4;

/// Height Map's numbers for a picture `w` × `h` (`plan`'s settings): the
/// smoothing box's radius — Smoothness 720ths of the longer side (no more
/// than 127,079 pixels over that side), at least 1 —, the fixed point's
/// bits below the 765ths — as many as a box that wide can add up without
/// overflowing, 14 at most (an exact single-precision number) —, the
/// brightest parts' height in pixels and a fixed-point unit's, Height
/// scale and Elevation's depth in pixels; `None` where VEGAS draws
/// nothing.
pub(crate) fn height_numbers(
    plan: &crate::accel::CompositorPlan,
    w: u32,
    h: u32,
) -> Option<crate::accel::HeightNumbers> {
    let longer = w.max(h) as i32;
    if longer <= 0 {
        return None;
    }
    let smoothness = plan.smoothness.min(127_079 / longer);
    if smoothness < 1 {
        return None;
    }
    let radius = ((f64::from(smoothness) / 720.0 * f64::from(longer) + 0.5) as i32).max(1) as u32;
    let width = u64::from(2 * radius + 1);
    let bits = (0..=14u32)
        .rev()
        .find(|&b| (width * 765) << b <= i32::MAX as u64)
        .unwrap_or(0);
    let side = f64::from(longer);
    let height = side * f64::from(plan.f[0]);
    Some(crate::accel::HeightNumbers {
        radius,
        bits,
        height: height as f32,
        unit: (height / f64::from(765u32 << bits)) as f32,
        scale: plan.f[2],
        depth: (side * f64::from(plan.f[1])) as f32,
    })
}

/// Height Map, as VEGAS's: the map's heights, smoothed by a box four
/// times each way, make a surface over the picture; each pixel shows the
/// picture moved along the surface's slopes by its depth there — its
/// height times Height scale, plus Elevation's.
fn height(s: &Vals, source: &Buf, map: &Buf) -> Buf {
    let plan = Kind::HeightMap.card_plan(s);
    let (w, h) = (
        source.width().max(0) as usize,
        source.height().max(0) as usize,
    );
    let Some(n) = height_numbers(&plan, w as u32, h as u32) else {
        return source.clone();
    };
    let [x1, y1, ..] = source.bounds;
    // The heights with their rows top first, as the card holds them.
    let mut plane: Vec<i32> = (0..h)
        .flat_map(|k| (0..w).map(move |x| (x, h - 1 - k)))
        .map(|(x, y)| height_fixed(map.at(x1 + x as c_int, y1 + y as c_int), plan.j, n.bits))
        .collect();
    for _ in 0..HEIGHT_PASSES {
        plane = box_columns(
            &box_rows(&plane, w, n.radius as usize),
            w,
            n.radius as usize,
        );
    }
    // The heights in fixed point: the slopes' differences are taken there,
    // before any product, as the card takes them.
    let at = |x: c_int, y: c_int| {
        let (x, y) = (x.clamp(0, w as c_int - 1), y.clamp(0, h as c_int - 1));
        plane[(h - 1 - y as usize) * w + x as usize] as f32
    };
    let wrap = plan.j[3] != 0;
    source.map(|x, y| {
        let (x, y) = (x - x1, y - y1);
        let depth = at(x, y) * n.unit * n.scale + n.depth;
        let (dx, dy) = if depth > 0.0 {
            let across = at(x + 1, y) - at(x - 1, y);
            let up = at(x, y + 1) - at(x, y - 1);
            (across * n.unit * 0.5 * depth, up * n.unit * 0.5 * depth)
        } else {
            (0.0, 0.0)
        };
        sample(source, x as f32 + dx, y as f32 + dy, wrap)
    })
}

// The plug-ins.

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
    displacement_map => Kind::DisplacementMap;
    height_map => Kind::HeightMap;
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
        let p = fx.props();
        p.set_str(kOfxPropLabel, kind.label());
        p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Compositors");
        p.set_str(kOfxPropPluginDescription, kind.description());
        p.set_strs(
            kOfxImageEffectPropSupportedContexts,
            &[kOfxImageEffectContextGeneral],
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
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        let comps = [kOfxImageComponentRGBA];
        fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
        fx.define_clip(MAP_CLIP, &comps);
        fx.define_clip(kOfxImageEffectOutputClipName, &comps);
        define_all(fx, kind.specs());
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        render(kind, fx, args)
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // Pixels come from anywhere: all of both.
        let (origin, frame) = fx.frame();
        let all = [
            origin[0],
            origin[1],
            origin[0] + frame[0],
            origin[1] + frame[1],
        ];
        out.set_dbls(c"OfxImageClipPropRoI_Source", &all);
        out.set_dbls(c"OfxImageClipPropRoI_Map", &all);
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

fn render(kind: Kind, fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    let map = fx.clip(MAP_CLIP).image(time);
    for img in [&src, &map].into_iter().flatten() {
        if img.depth != out.depth || img.channels != out.channels {
            return kOfxStatErrImageFormat;
        }
    }
    let vals = Vals::of(kind.specs(), &|s| s.read(fx, time));
    let (mut source, mut guide) = (Buf::read(src.as_ref()), Buf::read(map.as_ref()));
    drop(src);
    drop(map);
    if vals.b(c"flip") {
        std::mem::swap(&mut source, &mut guide);
    }
    // Without a map, the picture guides itself.
    if guide.data.is_empty() {
        guide = source.clone();
    }
    let result = kind.process(&vals, &source, &guide);
    write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

/// `result` into the output's render window.
pub(crate) fn write(
    out: &crate::ofx::Image,
    result: &Buf,
    (x1, y1, x2, y2): (c_int, c_int, c_int, c_int),
) {
    let out_straight = out.channels == 4 && out.unpremultiplied;
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(row) = out.row(y) else { return };
        for x in columns.clone() {
            let mut c = result.inside(x, y);
            if out_straight && c[3] > 0.0 {
                c = [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]];
            }
            // SAFETY: a row the output has, a column inside its bounds.
            unsafe { crate::colors::put_pixel(out, row, x, c) };
        }
    };
    for_rows(y1.max(out.bounds[1])..y2.min(out.bounds[3]), &fill);
}

/// The factory presets of compositor `kind`.
pub(crate) fn presets(kind: Kind) -> Vec<Preset> {
    let num = |v: f64| PresetValue::Num(vec![v]);
    let p = |name: &'static str, values: Vec<(&'static str, PresetValue)>| Preset { name, values };
    let flag = |b: bool| f64::from(u8::from(b));
    match kind {
        Kind::DisplacementMap => {
            let m = |name: &'static str, scale: f64, wrap: bool, alpha: bool| {
                p(
                    name,
                    vec![
                        ("scaleHorizontal", num(scale)),
                        ("scaleVertical", num(scale)),
                        ("edgePixelHandling", num(flag(wrap))),
                        ("xAxisChannel", num(0.0)),
                        ("xAxisInvert", num(0.0)),
                        ("yAxisChannel", num(1.0)),
                        ("yAxisInvert", num(0.0)),
                        ("alphaScales", num(flag(alpha))),
                        ("flip", num(0.0)),
                    ],
                )
            };
            vec![
                m(
                    "Red Shifts Horizontally, Green Shifts Vertically",
                    0.1,
                    false,
                    false,
                ),
                m("Pixels Wrap Around Image", 0.1, true, false),
                m("Alpha Scaled Displacement", 0.2, false, true),
            ]
        }
        Kind::HeightMap => {
            let m = |name: &'static str,
                     [amplitude, elevation, scale]: [f64; 3],
                     wrap: bool,
                     smoothness: f64,
                     alpha: bool| {
                p(
                    name,
                    vec![
                        ("amplitude", num(amplitude)),
                        ("elevation", num(elevation)),
                        ("heightScale", num(scale)),
                        ("edgePixelHandling", num(flag(wrap))),
                        ("sourceChannel", num(0.0)),
                        ("smoothness", num(smoothness)),
                        ("invert", num(0.0)),
                        ("alphaScales", num(flag(alpha))),
                        ("flip", num(0.0)),
                    ],
                )
            };
            vec![
                m("Low Extrusion", [0.1, 0.1, 1.0], false, 14.0, false),
                m("High Extrusion", [0.4, 0.5, 0.2], false, 14.0, false),
                m(
                    "Light Blend, Wrap Pixels Around",
                    [0.1, 0.1, 1.0],
                    true,
                    5.0,
                    false,
                ),
                m(
                    "Heavy Blend, Alpha Scales Channels",
                    [0.1, 0.1, 1.0],
                    false,
                    30.0,
                    true,
                ),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture whose red rises across and green rises up (rows go up).
    fn ramps() -> Buf {
        let bounds = [0, 0, 64, 36];
        let mut b = Buf::solid(bounds, [0.0; 4]);
        for y in 0..36 {
            for x in 0..64 {
                b.data[(y * 64 + x) as usize] = [x as f32 / 63.0, y as f32 / 35.0, 0.0, 1.0];
            }
        }
        b
    }

    fn vals(kind: Kind, set: &[(&str, f64)]) -> Vals {
        Vals::of(kind.specs(), &|s: Spec| {
            set.iter()
                .find(|(n, _)| s.name().to_str() == Ok(*n))
                .map(|(_, v)| vec![*v])
                .unwrap_or_else(|| s.default())
        })
    }

    #[test]
    fn a_bright_map_brings_pixels_from_the_right_and_below() {
        let source = ramps();
        // A white map: 255 is 127 steps over the middle gray, nearly the
        // whole reach — pixels come from the right (red higher) and from
        // below (green lower).
        let map = Buf::solid([0, 0, 64, 36], [1.0, 1.0, 1.0, 1.0]);
        let s = vals(Kind::DisplacementMap, &[]);
        assert!((displacement_at(&map, 5, 5, 0, false) - 127.0 / 128.0).abs() < 1e-6);
        let out = displacement(&s, &source, &map);
        let (before, after) = (source.at(20, 18), out.at(20, 18));
        assert!(after[0] > before[0] + 0.05, "{before:?} → {after:?}");
        assert!(after[1] < before[1] - 0.05, "{before:?} → {after:?}");
        // The middle gray (128) moves nothing; past the frame, nothing.
        let gray = Buf::solid([0, 0, 64, 36], [128.0 / 255.0, 128.0 / 255.0, 0.5, 1.0]);
        let still = displacement(&s, &source, &gray);
        assert_eq!(still.at(20, 18), source.at(20, 18));
        assert_eq!(out.at(63, 35), [0.0; 4]);
    }

    #[test]
    fn a_map_is_averaged_where_it_is_smooth_and_kept_at_its_edges() {
        // A step of 4 between columns: smooth enough to average.
        let mut map = Buf::solid([0, 0, 8, 8], [0.0, 0.0, 0.0, 1.0]);
        for y in 0..8 {
            for x in 0..8 {
                map.data[(y * 8 + x) as usize][0] = (128 + 4 * x) as f32 / 255.0;
            }
        }
        // At x 3: 2 × 140 + 136 + 144 + 136 + 144 = 840, less 768, over 6.
        let v = displacement_at(&map, 3, 3, 0, false);
        assert!((v - 12.0 / 128.0).abs() < 1e-6, "{v}");
        // A step of 20: too sharp, the pixel alone.
        for y in 0..8 {
            for x in 0..8 {
                map.data[(y * 8 + x) as usize][0] = (100 + 20 * x) as f32 / 255.0;
            }
        }
        let v = displacement_at(&map, 3, 3, 0, false);
        assert!((v - 32.0 / 128.0).abs() < 1e-6, "{v}");
    }

    #[test]
    fn a_hill_of_the_map_enlarges_what_is_under_it() {
        let source = ramps();
        // A dome in the middle.
        let mut map = Buf::solid([0, 0, 64, 36], [0.0, 0.0, 0.0, 1.0]);
        for y in 0..36 {
            for x in 0..64 {
                let d = ((x as f64 - 32.0).powi(2) + (y as f64 - 18.0).powi(2)).sqrt() / 16.0;
                let v = (1.0 - d * d).max(0.0) as f32;
                map.data[(y * 64 + x) as usize] = [v, v, v, 1.0];
            }
        }
        let s = vals(Kind::HeightMap, &[("smoothness", 1.0)]);
        let out = height(&s, &source, &map);
        // Right of the middle, the picture comes from nearer the middle:
        // less red than there.
        let (before, after) = (source.at(40, 18), out.at(40, 18));
        assert!(after[0] < before[0] - 0.01, "{before:?} → {after:?}");
        // Far from the dome, nothing moves.
        assert_eq!(out.at(2, 2), source.at(2, 2));
    }

    #[test]
    fn the_smoothing_grows_with_the_frame_as_vegas_s() {
        let plan = Kind::HeightMap.card_plan(&vals(Kind::HeightMap, &[]));
        let n = height_numbers(&plan, 1920, 1080).expect("numbers");
        // 14 720ths of 1,920 pixels.
        assert_eq!(n.radius, 37);
        assert!((n.height - 192.0).abs() < 1e-3 && (n.depth - 192.0).abs() < 1e-3);
        // An 8K frame allows 16 at most.
        let wide = Kind::HeightMap.card_plan(&vals(Kind::HeightMap, &[("smoothness", 31.0)]));
        assert_eq!(height_numbers(&wide, 7680, 4320).expect("8K").radius, 171);
    }

    #[test]
    fn the_presets_are_vegass() {
        let names = |k| presets(k).into_iter().map(|p| p.name).collect::<Vec<_>>();
        assert_eq!(
            names(Kind::DisplacementMap),
            [
                "Red Shifts Horizontally, Green Shifts Vertically",
                "Pixels Wrap Around Image",
                "Alpha Scaled Displacement"
            ]
        );
        assert_eq!(
            names(Kind::HeightMap),
            [
                "Low Extrusion",
                "High Extrusion",
                "Light Blend, Wrap Pixels Around",
                "Heavy Blend, Alpha Scales Channels"
            ]
        );
    }
}
