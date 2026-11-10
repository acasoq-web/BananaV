//! Mirror: VEGAS's. One side of a line through Center kept, the other
//! showing it reflected.
//!
//! As VEGAS's renders show: in the frame's pixels, rows from the top,
//! the line passes through `((w − 1) Center x, (1 − Center y)(h − 1))`.
//! Within half a degree of 0 it keeps the rows above and shows each below
//! the row as far above, between the two rows there; of 180, the rows
//! below; of 90, the columns to the left; of −90, those to the right. Any
//! other Angle slants it at `tan(±180° − Angle)`: on each row the pixels
//! up to the line (a whole column past where it crosses) are kept or
//! reflected — left of it from 0° to −180°, right of it from 0° to 180° —,
//! each reflected pixel showing the picture where its reflection falls,
//! between the four pixels around. Past the picture is the Background's
//! color with no alpha — what VEGAS's pictures hold there (its preview
//! shows the color; over other tracks it adds to them); measured on the
//! calibration card through each preset.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::kit::{Buf, Grid};
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.Mirror";

/// The parameters (BananaV's earlier Mirror's names, so its projects
/// open).
const ANGLE: &CStr = c"angle";
const CENTER: &CStr = c"center";
const BACKGROUND: &CStr = c"background";

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    /// Degrees.
    pub angle: f64,
    /// 0–1 of the frame from its lower left.
    pub center: [f64; 2],
    /// Straight, with its alpha.
    pub background: [f64; 4],
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            angle: fx.param(ANGLE).f64_at(time),
            center: fx.param(CENTER).xy_at(time),
            background: fx.param(BACKGROUND).rgba_at(time),
        }
    }

    /// The settings in `values` (the frame's, as the card draws it).
    pub(crate) fn of(values: Values<'_>) -> Settings {
        Settings {
            angle: values.f64(ANGLE, 0.0),
            center: values.array(CENTER, [0.5, 0.5]),
            background: values.array(BACKGROUND, [0.0; 4]),
        }
    }

    /// What shows past the picture: the background's color, premultiplied
    /// by its alpha, with none of its own.
    pub(crate) fn beyond(&self) -> [f32; 4] {
        let [r, g, b, a] = self.background.map(|v| v.clamp(0.0, 1.0) as f32);
        [r * a, g * a, b * a, 0.0]
    }

    /// How it reflects on a picture `w` × `h` pixels at render scale
    /// `scale` of pixel aspect `par`: VEGAS's, in its floats (its own
    /// digits of a degree and of a turn).
    #[allow(clippy::approx_constant)]
    pub(crate) fn layout(&self, w: u32, h: u32, scale: [f64; 2], par: f64) -> Layout {
        let t = ((self.angle * 0.01745329252) % 6.28318530718) as f32;
        let within = |a: f32, b: f32| (a..=b).contains(&t);
        let way = if within(-0.008726646, 0.008726646) {
            Way::Top
        } else if within(3.132866, 3.1503193) || within(-3.1503193, -3.132866) {
            Way::Bottom
        } else if within(1.5620697, 1.579523) || within(-4.7211156, -4.7036624) {
            Way::Left
        } else if within(4.7036624, 4.7211156) || within(-1.579523, -1.5620697) {
            Way::Right
        } else {
            Way::Slanted
        };
        let base = if t <= 0.0 {
            -std::f32::consts::PI
        } else {
            std::f32::consts::PI
        };
        let (mut sx, mut sy) = (scale[0] as f32, scale[1] as f32);
        if par as f32 <= 1.0 {
            sy /= par as f32;
        } else {
            sx /= par as f32;
        }
        Layout {
            way,
            point: [
                ((w as i32 - 1) as f64 * self.center[0]) as f32,
                ((1.0 - self.center[1]) * (h as i32 - 1) as f64) as f32,
            ],
            slope: (base - t).tan(),
            left_reflects: base - t <= 0.0,
            scale: [sx, sy],
        }
    }

    /// The picture mirrored, on the frame `g` places.
    pub(crate) fn process(&self, g: &Grid, src: &Buf) -> Buf {
        // The frame in the picture's pixels: its first column and its top
        // row (rows up), its size.
        let left = (g.origin[0] * g.scale[0] / g.par).round() as c_int;
        let (w, h) = (
            g.width_px().round().max(1.0) as u32,
            g.height_px().round().max(1.0) as u32,
        );
        let top = (g.origin[1] * g.scale[1]).round() as c_int + h as c_int - 1;
        let layout = self.layout(w, h, g.scale, g.par);
        let beyond = self.beyond();
        // A pixel of the picture, columns from the left, rows from the top.
        let at = |c: i32, r: i32| src.at(left + c, top - r);
        src.map(|x, y| layout.pixel(x - left, top - y, w, h, &at, beyond))
    }
}

/// How VEGAS's Mirror reflects, by Angle: about a level line (near 0°,
/// the top kept; near 180°, the bottom), an upright one (near 90°, the
/// left kept; near −90°, the right) or a slanted one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Way {
    Top,
    Bottom,
    Left,
    Right,
    Slanted,
}

/// VEGAS's Mirror on a picture: which way, the line's point (pixels from
/// the top left), a slanted line's slope (rows down per column as its
/// scaled coordinates count them) and whether the part left of it
/// reflects, and the pixels' scale across and down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Layout {
    pub way: Way,
    pub point: [f32; 2],
    pub slope: f32,
    pub left_reflects: bool,
    pub scale: [f32; 2],
}

impl Layout {
    /// Pixel `c`, `r` (from the top left) of a picture `w` × `h` mirrored:
    /// `at` reads the picture, `beyond` is what shows past it.
    pub(crate) fn pixel(
        &self,
        c: i32,
        r: i32,
        w: u32,
        h: u32,
        at: &dyn Fn(i32, i32) -> [f32; 4],
        beyond: [f32; 4],
    ) -> [f32; 4] {
        let (wi, hi) = (w as i32, h as i32);
        let [px, py] = self.point;
        // Between two rows (or columns) `t` points to, the last one's
        // neighbour past the picture the background.
        let between = |t: f32, n: i32, read: &dyn Fn(i32) -> [f32; 4]| {
            let i = t as i32;
            let f = t - i as f32;
            if i < 0 || i >= n {
                return beyond;
            }
            let next = if i == n - 1 { beyond } else { read(i + 1) };
            std::array::from_fn(|k| f * next[k] + (1.0 - f) * read(i)[k])
        };
        match self.way {
            Way::Top => {
                let kept = (py.ceil() as i32).min(hi);
                let end = (1 - (py * -2.0) as i32).min(hi);
                if r < kept {
                    at(c, r)
                } else if r < end {
                    between(py + py - r as f32, hi, &|i| at(c, i))
                } else {
                    beyond
                }
            }
            Way::Bottom => {
                let start = ((py + py - hi as f32) as i32).min(hi).max(0);
                let end = (py.ceil() as i32).min(hi - 1);
                if r < start {
                    beyond
                } else if r <= end {
                    between(py + py - r as f32, hi, &|i| at(c, i))
                } else {
                    at(c, r)
                }
            }
            Way::Left => {
                if c < px.ceil() as i32 {
                    at(c, r)
                } else {
                    between(px + px - c as f32, wi, &|i| at(i, r))
                }
            }
            Way::Right => {
                if c <= px.ceil() as i32 {
                    between(px + px - c as f32, wi, &|i| at(i, r))
                } else {
                    at(c, r)
                }
            }
            Way::Slanted => {
                let [sx, sy] = self.scale;
                let m = self.slope;
                let (cx, cy) = (px * (1.0 / sx), py * (1.0 / sy));
                let y = r as f32 * (1.0 / sy);
                let cross = ((((y - cy) / m + cx) * sx) as i32 + 1).min(wi);
                if (c < cross) != self.left_reflects {
                    return at(c, r);
                }
                let x = c as f32 * (1.0 / sx);
                let foot = (((y - (-1.0 / m) * x) - cy) + m * cx) / (m - -1.0 / m);
                let foot_y = (foot - cx) * m + cy;
                let rx = ((foot + foot) - x) * sx;
                let ry = ((foot_y + foot_y) - y) * sy;
                let (ix, iy) = (rx.floor() as i32, ry.floor() as i32);
                if ix < -1 || ix >= wi || iy < -1 || iy >= hi {
                    return beyond;
                }
                let read = |i: i32, j: i32| {
                    if i < 0 || i >= wi || j < 0 || j >= hi {
                        beyond
                    } else {
                        at(i, j)
                    }
                };
                let (fx, fy) = (rx - ix as f32, ry - iy as f32);
                let (a, b) = (read(ix, iy), read(ix + 1, iy));
                let (d, e) = (read(ix, iy + 1), read(ix + 1, iy + 1));
                std::array::from_fn(|k| {
                    let upper = a[k] + (b[k] - a[k]) * fx;
                    let lower = d[k] + (e[k] - d[k]) * fx;
                    upper + (lower - upper) * fy
                })
            }
        }
    }

    /// Its numbers for the card's `kit_mirror`: the line's point, its
    /// slope, the pixels' scale; with `j` its way (0 Top, 1 Bottom, 2
    /// Left, 3 Right, 4 Slanted) and whether the left part reflects.
    pub(crate) fn numbers(&self) -> ([f32; 5], [u32; 2]) {
        let way = match self.way {
            Way::Top => 0,
            Way::Bottom => 1,
            Way::Left => 2,
            Way::Right => 3,
            Way::Slanted => 4,
        };
        (
            [
                self.point[0],
                self.point[1],
                self.slope,
                self.scale[0],
                self.scale[1],
            ],
            [way, u32::from(self.left_reflects)],
        )
    }
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
        let s = Settings::at(fx, args.dbl(kOfxPropTime, 0));
        crate::spatial::render(fx, args, &|g, src| s.process(g, src))
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        crate::spatial::whole_source(fx, args, out)
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

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Mirror");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Creative");
    p.set_str(
        kOfxPropPluginDescription,
        c"The picture reflected along a line through a center, at any angle.",
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
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);
    let p = fx.define_param(kOfxParamTypeDouble, ANGLE, c"Angle");
    p.set_str(kOfxParamPropDoubleType, kOfxParamDoubleTypeAngle);
    p.set_dbl(kOfxParamPropDefault, 0.0);
    p.set_dbl(kOfxParamPropMin, -180.0);
    p.set_dbl(kOfxParamPropMax, 180.0);
    p.set_dbl(kOfxParamPropDisplayMin, -180.0);
    p.set_dbl(kOfxParamPropDisplayMax, 180.0);
    p.set_dbl(kOfxParamPropIncrement, 1.0);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"Which side is reflected onto the other: 0 the top half down, 90 the left half, 180 the bottom half up.",
    );
    let p = fx.define_param(kOfxParamTypeDouble2D, CENTER, c"Center");
    p.set_str(
        kOfxParamPropDoubleType,
        kOfxParamDoubleTypeNormalisedXYAbsolute,
    );
    p.set_dbls(kOfxParamPropDefault, &[0.5, 0.5]);
    // VEGAS's three decimals ("0.500, 0.500").
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"Where the line of reflection passes: 0, 0 is the lower left corner.",
    );
    let p = fx.define_param(kOfxParamTypeRGBA, BACKGROUND, c"Background");
    p.set_dbls(kOfxParamPropDefault, &[0.0, 0.0, 0.0, 0.0]);
    p.set_str(
        kOfxParamPropHint,
        c"What shows where the reflected picture does not reach.",
    );
    // VEGAS's picker opens in RGB here.
    p.set_str(bv_ofx_sys::extensions::kBananaVParamPropColorModel, c"RGB");
}

/// The factory presets: VEGAS's, in its list's order.
pub(crate) fn presets() -> Vec<Preset> {
    let p = |name: &'static str, angle: f64, center: [f64; 2]| Preset {
        name,
        values: vec![
            ("angle", PresetValue::Num(vec![angle])),
            ("center", PresetValue::Num(center.to_vec())),
            ("background", PresetValue::Num(vec![0.0; 4])),
        ],
    };
    vec![
        p("Reflect Top", 0.0, [0.5, 0.5]),
        p("Reflect Bottom", 180.0, [0.5, 0.5]),
        p("Reflect Left", 90.0, [0.5, 0.5]),
        p("Reflect Right", -90.0, [0.5, 0.5]),
        p("Reflect to Upper Left", -135.0, [0.4, 0.6]),
        p("Reflect to Upper Right", 135.0, [0.6, 0.6]),
        p("Reflect to Lower Left", -45.0, [0.4, 0.4]),
        p("Reflect to Lower Right", 45.0, [0.6, 0.4]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(w: f64, h: f64) -> Grid {
        Grid {
            origin: [0.0, 0.0],
            frame: [w, h],
            scale: [1.0, 1.0],
            par: 1.0,
        }
    }

    /// A picture 8 × 6 whose pixels say where they are: red its column,
    /// green its row (rows up, as OpenFX counts them).
    fn places() -> Buf {
        let mut data = Vec::new();
        for y in 0..6 {
            for x in 0..8 {
                data.push([x as f32 / 8.0, y as f32 / 8.0, 0.0, 1.0]);
            }
        }
        Buf {
            bounds: [0, 0, 8, 6],
            data,
        }
    }

    fn settings(angle: f64) -> Settings {
        Settings {
            angle,
            center: [0.5, 0.5],
            background: [0.0; 4],
        }
    }

    #[test]
    fn the_axes_reflect_whole_pixels() {
        let (g, src) = (grid(8.0, 6.0), places());
        // Reflect Top: the bottom row shows the top one, the top stays.
        let out = settings(0.0).process(&g, &src);
        assert_eq!(out.at(3, 0), src.at(3, 5));
        assert_eq!(out.at(3, 2), src.at(3, 3));
        assert_eq!(out.at(3, 5), src.at(3, 5));
        // Reflect Left: the right half shows the left one.
        let out = settings(90.0).process(&g, &src);
        assert_eq!(out.at(7, 1), src.at(0, 1));
        assert_eq!(out.at(4, 1), src.at(3, 1));
        assert_eq!(out.at(4, 4), src.at(3, 4));
        assert_eq!(out.at(2, 1), src.at(2, 1));
        // Reflect Bottom and Reflect Right, the other ways: the row whose
        // top is on the line reflected all along, as VEGAS's.
        let out = settings(180.0).process(&g, &src);
        assert_eq!(out.at(3, 5), src.at(3, 0));
        assert_eq!(out.at(1, 2), src.at(1, 3));
        assert_eq!(out.at(6, 2), src.at(6, 3));
        let out = settings(-90.0).process(&g, &src);
        assert_eq!(out.at(0, 4), src.at(7, 4));
    }

    #[test]
    fn a_slanted_line_reflects_across_the_diagonal() {
        // −45° about the middle of an 8 × 8 picture: the upper right kept,
        // shown in the lower left (VEGAS's Reflect to Lower Left).
        let g = grid(8.0, 8.0);
        let mut data = Vec::new();
        for y in 0..8 {
            for x in 0..8 {
                data.push([x as f32 / 8.0, y as f32 / 8.0, 0.0, 1.0]);
            }
        }
        let src = Buf {
            bounds: [0, 0, 8, 8],
            data,
        };
        let s = Settings {
            angle: -45.0,
            center: [0.5, 0.5],
            background: [0.0; 4],
        };
        let layout = s.layout(8, 8, [1.0; 2], 1.0);
        assert_eq!(layout.way, Way::Slanted);
        assert!(layout.left_reflects && (layout.slope - 1.0).abs() < 1e-6);
        let out = s.process(&g, &src);
        // The bottom left corner shows the top right one; the top right
        // stays; a pixel on the diagonal's left reflects across it.
        assert_eq!(out.at(0, 0), src.at(7, 7));
        assert_eq!(out.at(7, 7), src.at(7, 7));
        let p = out.at(1, 3);
        let want = src.at(4, 6);
        assert!(
            p.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-5),
            "{p:?} {want:?}"
        );
    }

    #[test]
    fn past_the_picture_is_the_background_without_alpha() {
        let g = grid(8.0, 6.0);
        // Reflect Top about a line near the top: the bottom reflects from
        // above the frame.
        let s = Settings {
            angle: 0.0,
            center: [0.5, 0.9],
            background: [0.9, 0.23, 0.22, 1.0],
        };
        let out = s.process(&g, &places());
        let p = out.at(3, 0);
        assert!(
            (p[0] - 0.9).abs() < 1e-6 && (p[1] - 0.23).abs() < 1e-6,
            "{p:?}"
        );
        assert_eq!(p[3], 0.0);
    }

    #[test]
    fn the_presets_are_vegass() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            [
                "Reflect Top",
                "Reflect Bottom",
                "Reflect Left",
                "Reflect Right",
                "Reflect to Upper Left",
                "Reflect to Upper Right",
                "Reflect to Lower Left",
                "Reflect to Lower Right"
            ]
        );
    }
}
