//! Pixelate: VEGAS's. The picture as cells of one color each, the average
//! of the pixels they cover, as wide as Horizontal pixelization says and
//! as tall as Vertical pixelization says: at 0 none that way, then cells
//! from two pixels up to the whole frame at 1.
//!
//! VEGAS's math (its presets measured on a 1280 × 720 frame): a frame `w`
//! pixels wide at full size holds `⌊w·(1 − h)/2·f + 1⌋` cells across
//! (Medium, 0.5: 321; Large, 0.75: 161), and the same down (181, 91) —
//! `f` the pixel aspect the wider way —, none below a pixelization of
//! 0.0001, no more than the picture's pixels. The cells run a step of the
//! picture's pixels over their number apart, in 16,384ths of a pixel from
//! its first column and its bottom row, the last to the picture's end;
//! each is the plain average, as the premultiplied values stand, of as
//! many whole pixels as the step holds from its first.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::OnceLock;

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::colors::Channel;
use crate::ofx::{Depth, Fx, Image, Props, for_rows, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.Pixelate";

const HORIZONTAL: &CStr = c"horizontal";
const VERTICAL: &CStr = c"vertical";

/// Whether pixelization `p` makes cells: VEGAS's, past 10⁻⁶.
fn pixelates(p: f64) -> bool {
    (p as f32).abs() > 1e-6
}

/// How many cells a line `pixels` long at full size holds at pixelization
/// `p`, its pixels shown `f` times as long (as many as its pixels below
/// 0.0001: nothing changes that way): VEGAS's.
pub(crate) fn cells(p: f64, pixels: f64, f: f64) -> u32 {
    let n = pixels.round().max(1.0);
    if 1.0 - p > 0.9999 {
        return n as u32;
    }
    ((pixels * (1.0 - p) * 0.5 * f + 1.0) as i64).clamp(1, n as i64) as u32
}

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub horizontal: f64,
    pub vertical: f64,
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        Settings {
            horizontal: v.f64(HORIZONTAL, 0.0),
            vertical: v.f64(VERTICAL, 0.0),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            horizontal: fx.param(HORIZONTAL).f64_at(time),
            vertical: fx.param(VERTICAL).f64_at(time),
        }
    }

    /// The cells across and down a frame `pixels` big (at full size) of
    /// pixel aspect `par`.
    pub(crate) fn cells(&self, pixels: [f64; 2], par: f64) -> [u32; 2] {
        let [fx, fy] = if par <= 1.0 {
            [1.0, 1.0 / par]
        } else {
            [par, 1.0]
        };
        [
            cells(self.horizontal, pixels[0], fx),
            cells(self.vertical, pixels[1], fy),
        ]
    }

    fn changes_nothing(&self) -> bool {
        !pixelates(self.horizontal) && !pixelates(self.vertical)
    }
}

/// The cells one way: `n` of them over a line of the frame `size` pixels
/// long (no more than its pixels), from pixel `start` — VEGAS's: a step of
/// `size ⁄ n` pixels in 16,384ths, the last cell to the line's end, each
/// averaging the step's whole pixels from its first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Cells {
    n: i64,
    size: i64,
    start: i64,
    step: i64,
}

impl Cells {
    pub(crate) fn new(n: u32, size: i64, start: i64) -> Cells {
        let size = size.max(1);
        let n = i64::from(n).clamp(1, size);
        Cells {
            n,
            size,
            start,
            step: step(n as u32, size as u32),
        }
    }

    /// The cell pixel `x` is in (those past the line's ends, its end
    /// cells').
    pub(crate) fn of(&self, x: i64) -> i64 {
        let d = (x - self.start).max(0);
        ((d + 1) * 16384 - 1).div_euclid(self.step).min(self.n - 1)
    }

    /// Cell `c`'s first pixel (after the last, the line's end).
    #[cfg(test)]
    fn first(&self, c: i64) -> i64 {
        if c >= self.n {
            return self.start + self.size;
        }
        self.start + ((c * self.step) >> 14)
    }

    /// The pixels cell `c` averages: the step's whole pixels from its
    /// first.
    pub(crate) fn averaged(&self, c: i64) -> std::ops::Range<i64> {
        let a = self.start + ((c * self.step) >> 14);
        a..a + (self.step >> 14).max(1)
    }
}

/// VEGAS's step between `n` cells over `size` pixels, in 16,384ths.
pub(crate) fn step(n: u32, size: u32) -> i64 {
    ((f64::from(size) / f64::from(n.max(1))) * 16384.0) as i64
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
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        if Settings::at(fx, args.dbl(kOfxPropTime, 0)).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // A pixel needs its whole cell: a cell (and a pixel) further each
        // way.
        let s = Settings::at(fx, args.dbl(kOfxPropTime, 0));
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
        let scale = scale.map(|v| if v > 0.0 { v } else { 1.0 });
        let (_, frame) = fx.frame();
        let par = project_par(fx);
        let [n, m] = s.cells([frame[0] / par, frame[1]], par);
        let mx = frame[0] / f64::from(n) + 2.0 * par / scale[0];
        let my = frame[1] / f64::from(m) + 2.0 / scale[1];
        out.set_dbls(
            c"OfxImageClipPropRoI_Source",
            &[roi[0] - mx, roi[1] - my, roi[2] + mx, roi[3] + my],
        );
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

/// The project's pixel aspect ratio (1 when the host does not say).
fn project_par(fx: Fx) -> f64 {
    let par = fx
        .props()
        .dbl(kOfxImageEffectPropProjectPixelAspectRatio, 0);
    if par > 0.0 { par } else { 1.0 }
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Pixelate");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Creative");
    p.set_str(
        kOfxPropPluginDescription,
        c"Turns the picture into blocks of one color, across, down or both: to hide a face, or for a retro look.",
    );
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextFilter, kOfxImageEffectContextGeneral],
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
    let comps = [
        kOfxImageComponentRGBA,
        kOfxImageComponentRGB,
        kOfxImageComponentAlpha,
    ];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);
    let number = |name: &CStr, label: &CStr, hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, 0.0);
        p.set_dbl(kOfxParamPropMin, 0.0);
        p.set_dbl(kOfxParamPropMax, 1.0);
        p.set_dbl(kOfxParamPropDisplayMin, 0.0);
        p.set_dbl(kOfxParamPropDisplayMax, 1.0);
        p.set_dbl(kOfxParamPropIncrement, 0.01);
        p.set_int(kOfxParamPropDigits, 3);
        p.set_str(kOfxParamPropHint, hint);
    };
    number(
        HORIZONTAL,
        c"Horizontal pixelization",
        c"How wide the blocks are: 0 none, 1 the whole frame.",
    );
    number(
        VERTICAL,
        c"Vertical pixelization",
        c"How tall the blocks are: 0 none, 1 the whole frame.",
    );
}

/// The channels of `img`'s pixel at `x`, `y` as it keeps them (nothing
/// outside it: transparent).
fn read(img: &Image, x: c_int, y: c_int) -> [f32; 4] {
    let mut v = [0.0; 4];
    let [x1, _, x2, _] = img.bounds;
    if !(x1..x2).contains(&x) {
        return v;
    }
    let Some(r) = img.row(y) else { return v };
    let i = (x - x1) as usize * img.channels;
    for (c, v) in v.iter_mut().enumerate().take(img.channels.min(4)) {
        // SAFETY: a channel of a pixel of a row the image has, in its
        // depth.
        *v = unsafe {
            match img.depth {
                Depth::Float => (*(r as *const f32).add(i + c)).get(),
                Depth::Short => (*(r as *const u16).add(i + c)).get(),
                Depth::Byte => (*r.add(i + c)).get(),
            }
        };
    }
    v
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|v| if v > 0.0 { v } else { 1.0 });
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    if let Some(s) = &src
        && (s.depth != out.depth || s.channels != out.channels)
    {
        return kOfxStatErrImageFormat;
    }
    let channels = out.channels.min(4);
    // Colors are averaged multiplied by their alpha (a see-through pixel
    // weighs less), and given back as the source keeps them.
    let straight = channels == 4 && src.as_ref().is_some_and(|s| s.unpremultiplied);
    let (origin, frame) = fx.frame();
    let par = out.par;
    let [n, m] = Settings::at(fx, time).cells([frame[0] / par, frame[1]], par);
    let across = Cells::new(
        n,
        (frame[0] * scale[0] / par).round() as i64,
        (origin[0] * scale[0] / par).round() as i64,
    );
    // Up from the frame's bottom row, as VEGAS counts them.
    let down = Cells::new(
        m,
        (frame[1] * scale[1]).round() as i64,
        (origin[1] * scale[1]).round() as i64,
    );

    let window = [
        x1.max(out.bounds[0]),
        y1.max(out.bounds[1]),
        x2.min(out.bounds[2]),
        y2.min(out.bounds[3]),
    ];
    if window[0] >= window[2] || window[1] >= window[3] {
        return kOfxStatOK;
    }
    // The cells the window shows: their averages first.
    let cx = across.of(i64::from(window[0]))..across.of(i64::from(window[2] - 1)) + 1;
    let cy = down.of(i64::from(window[1]))..down.of(i64::from(window[3] - 1)) + 1;
    let wide = (cx.end - cx.start) as usize;
    let averages: Vec<OnceLock<Vec<[f32; 4]>>> =
        (cy.start..cy.end).map(|_| OnceLock::new()).collect();
    let pixel = |x: i64, y: i64| -> [f32; 4] {
        let Some(s) = &src else { return [0.0; 4] };
        let (Ok(x), Ok(y)) = (c_int::try_from(x), c_int::try_from(y)) else {
            return [0.0; 4];
        };
        let mut p = read(s, x, y);
        if straight {
            for c in 0..3 {
                p[c] *= p[3];
            }
        }
        p
    };
    let average = |j: c_int| {
        let c = cy.start + i64::from(j);
        let rows = down.averaged(c);
        let (r0, r1) = (rows.start, rows.end);
        let mut cells = vec![[0.0f32; 4]; wide];
        for (k, cell) in cells.iter_mut().enumerate() {
            let col = cx.start + k as i64;
            let columns = across.averaged(col);
            let (a, b) = (columns.start, columns.end);
            // Each row's average, then theirs (as the card adds them up).
            let mut sum = [0.0f64; 4];
            for r in r0..r1 {
                let mut line = [0.0f64; 4];
                for x in a..b {
                    let p = pixel(x, r);
                    for (l, v) in line.iter_mut().zip(p) {
                        *l += f64::from(v);
                    }
                }
                for (s, l) in sum.iter_mut().zip(line) {
                    *s += l / (b - a) as f64;
                }
            }
            for (v, s) in cell.iter_mut().zip(sum) {
                *v = (s / (r1 - r0) as f64) as f32;
            }
            if straight {
                let a = cell[3];
                for v in cell.iter_mut().take(3) {
                    *v = if a > 0.0 { *v / a } else { 0.0 };
                }
            }
        }
        let _ = averages[j as usize].set(cells);
    };
    for_rows(0..(cy.end - cy.start) as c_int, &average);

    let depth = out.depth;
    let put = |y: c_int| {
        let Some(o) = out.row(y) else { return };
        let Some(line) = averages
            .get((down.of(i64::from(y)) - cy.start) as usize)
            .and_then(|a| a.get())
        else {
            return;
        };
        for x in window[0]..window[2] {
            let Some(px) = line.get((across.of(i64::from(x)) - cx.start) as usize) else {
                continue;
            };
            let k = (x - out.bounds[0]) as usize * out.channels;
            // SAFETY: a pixel of a row the output has, in its depth.
            unsafe {
                for (c, v) in px.iter().enumerate().take(channels) {
                    match depth {
                        Depth::Float => *(o as *mut f32).add(k + c) = f32::put(*v),
                        Depth::Short => *(o as *mut u16).add(k + c) = u16::put(*v),
                        Depth::Byte => *o.add(k + c) = u8::put(*v),
                    }
                }
            }
        }
    };
    for_rows(window[1]..window[3], &put);
    drop(src);
    drop(out);
    kOfxStatOK
}

// Presets.

/// The factory presets, VEGAS's: their names and values.
pub(crate) fn presets() -> Vec<Preset> {
    let preset = |name: &'static str, h: f64, v: f64| Preset {
        name,
        values: vec![
            ("horizontal", PresetValue::Num(vec![h])),
            ("vertical", PresetValue::Num(vec![v])),
        ],
    };
    vec![
        preset("Small", 0.12, 0.12),
        preset("Medium", 0.5, 0.5),
        preset("Large", 0.75, 0.75),
        preset("Medium Horizontal", 0.5, 0.0),
        preset("Medium Vertical", 0.0, 0.5),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cells_are_as_many_as_vegass() {
        // Measured on VEGAS's presets over a 1280 × 720 frame.
        assert_eq!(cells(0.5, 1280.0, 1.0), 321);
        assert_eq!(cells(0.5, 720.0, 1.0), 181);
        assert_eq!(cells(0.75, 1280.0, 1.0), 161);
        assert_eq!(cells(0.75, 720.0, 1.0), 91);
        assert_eq!(cells(0.12, 1280.0, 1.0), 564);
        // In double precision 1 − 0.9 is a hair under 0.1.
        assert_eq!(cells(0.9, 160.0, 1.0), 8);
        // None below 0.0001, the whole frame at 1; wide pixels more.
        assert_eq!(cells(0.0, 1280.0, 1.0), 1280);
        assert_eq!(cells(0.00005, 1280.0, 1.0), 1280);
        assert_eq!(cells(1.0, 1280.0, 1.0), 1);
        assert_eq!(cells(-0.5, 720.0, 1.0), 720);
        let s = Settings {
            horizontal: 0.5,
            vertical: 0.5,
        };
        assert_eq!(s.cells([960.0, 1080.0], 2.0), [481, 271]);
        assert_eq!(s.cells([1920.0, 1080.0], 0.5), [481, 541]);
    }

    #[test]
    fn cells_cover_the_frame_whole_from_end_to_end() {
        for (n, size) in [(161, 1280), (321, 1280), (91, 720), (5, 7), (7, 7), (1, 9)] {
            let c = Cells::new(n, size, 0);
            assert_eq!(c.first(0), 0);
            assert_eq!(c.first(i64::from(n)), size);
            for k in 0..i64::from(n) {
                let (a, b) = (c.first(k), c.first(k + 1));
                assert!(b > a, "{n} over {size}: cell {k}");
                for x in a..b {
                    assert_eq!(c.of(x), k, "{n} cells over {size}: pixel {x}");
                }
                // Each averages the step's whole pixels from its first.
                let avg = c.averaged(k);
                assert_eq!(avg.start, a);
                assert_eq!(avg.end - avg.start, (step(n, size as u32) >> 14).max(1));
            }
        }
        // VEGAS's steps: 1280 ⁄ 321 pixels in 16,384ths, the cells' firsts
        // their multiples' whole pixels.
        let c = Cells::new(321, 1280, 0);
        assert_eq!(step(321, 1280), 65331);
        assert_eq!((c.first(1), c.first(2), c.first(3)), (3, 7, 11));
        assert_eq!(c.averaged(1), 3..6);
        // Past the frame's ends, its end cells.
        let c = Cells::new(4, 8, 10);
        assert_eq!((c.of(9), c.of(8), c.of(18)), (0, 0, 3));
        // More cells than pixels: each pixel its own.
        let c = Cells::new(50, 10, 0);
        assert_eq!(
            (0..10).map(|x| c.of(x)).collect::<Vec<_>>(),
            (0..10).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_presets_are_vegass() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            [
                "Small",
                "Medium",
                "Large",
                "Medium Horizontal",
                "Medium Vertical"
            ]
        );
        let h = &presets()[3];
        assert_eq!(
            h.values,
            [
                ("horizontal", PresetValue::Num(vec![0.5])),
                ("vertical", PresetValue::Num(vec![0.0]))
            ]
        );
    }
}
