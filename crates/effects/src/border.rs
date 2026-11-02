//! Border: VEGAS's. A border around the frame: Blurred (the picture's
//! edges fade away, transparent), Beveled (a frame of four facets, lit
//! from Angle and tinted by Color) or Solid (a plain frame of Color).
//!
//! VEGAS's math, measured on its pictures (the frame's border: an event's
//! effects work on its framed picture):
//! - the border is Size × half the frame's width wide, on every side;
//! - Blurred: the picture times a rectangle that far in, blurred (a
//!   Gaussian whose sigma is 0.446 × the border's width): its edges
//!   transparent;
//! - Beveled: four facets from the frame's sides to the rectangle that far
//!   in (held at the middle: at Size 1, a pyramid), parted by the lines
//!   from its corners to the frame's; each facet's colors times Color and
//!   1.05 + 0.70 cos(the angle between its side's direction and the
//!   light's), the light at Angle (0 at the top, clockwise), clipped to
//!   0–1;
//! - Solid: Color (its alpha too) over the band, the picture inside.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Fx, Props, for_rows, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.Border";

const TYPE: &CStr = c"type";
const SIZE: &CStr = c"size";
const ANGLE: &CStr = c"angle";
const BEVEL_COLOR: &CStr = c"bevelColor";
const COLOR: &CStr = c"color";

/// The types, in VEGAS's list's order.
const TYPES: [&CStr; 3] = [c"Blurred", c"Beveled", c"Solid"];

/// Blurred's edge: the Gaussian's sigma, a part of the border's width.
pub(crate) const SOFTNESS: f64 = 0.446;

/// A facet's light: `AMBIENT + DIFFUSE` × the cosine of the angle between
/// its side's direction and the light's.
pub(crate) const AMBIENT: f64 = 1.05;
pub(crate) const DIFFUSE: f64 = 0.70;

/// The default angle and colors (VEGAS's Bevel's and Solid White Border's).
const DEFAULT_ANGLE: f64 = 325.0;
const WHITE: [f64; 4] = [1.0; 4];

/// A border's type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Blurred,
    Beveled,
    Solid,
}

impl Kind {
    fn of(i: c_int) -> Kind {
        match i {
            1 => Kind::Beveled,
            2 => Kind::Solid,
            _ => Kind::Blurred,
        }
    }

    /// Its place in the list (what the card's plan carries).
    pub(crate) fn index(self) -> u32 {
        match self {
            Kind::Blurred => 0,
            Kind::Beveled => 1,
            Kind::Solid => 2,
        }
    }
}

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub kind: Kind,
    /// The border's width: 1 is half the frame's width.
    pub size: f64,
    /// Where the light comes from, degrees clockwise from the top.
    pub angle: f64,
    pub bevel: [f64; 3],
    pub color: [f64; 4],
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            kind: Kind::Blurred,
            size: 0.0,
            angle: DEFAULT_ANGLE,
            bevel: [1.0; 3],
            color: WHITE,
        }
    }
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        Settings {
            kind: Kind::of(v.int(TYPE, 0)),
            size: v.f64(SIZE, 0.0),
            angle: v.f64(ANGLE, DEFAULT_ANGLE),
            bevel: v.array(BEVEL_COLOR, [1.0; 3]),
            color: v.array(COLOR, WHITE),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            kind: Kind::of(fx.param(TYPE).int_at(time)),
            size: fx.param(SIZE).f64_at(time),
            angle: fx.param(ANGLE).f64_at(time),
            bevel: fx.param(BEVEL_COLOR).rgb_at(time),
            color: fx.param(COLOR).rgba_at(time),
        }
    }

    pub(crate) fn changes_nothing(&self) -> bool {
        self.size <= 0.0
    }

    /// The border on a frame `frame` big (canonical units), worked out.
    pub(crate) fn on(&self, frame: [f64; 2]) -> Border {
        let width = self.size.clamp(0.0, 1.0) * frame[0] / 2.0;
        let lights: [f64; 4] = std::array::from_fn(|i| {
            AMBIENT + DIFFUSE * (90.0 * i as f64 - self.angle).to_radians().cos()
        });
        Border {
            kind: self.kind,
            frame,
            width,
            sigma: SOFTNESS * width,
            depth: [width.min(frame[0] / 2.0), width.min(frame[1] / 2.0)],
            shades: lights.map(|k| self.bevel.map(|c| (c.max(0.0) * k) as f32)),
            color: self.color.map(|c| c.clamp(0.0, 1.0) as f32),
        }
    }
}

/// A border worked out for a frame: what the card draws with too.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Border {
    pub kind: Kind,
    /// The frame's size (canonical units).
    pub frame: [f64; 2],
    /// The border's width.
    pub width: f64,
    /// Blurred's edge's blur.
    pub sigma: f64,
    /// How deep the facets across (left, right) and up (top, bottom) go:
    /// the width, held at the frame's middle.
    pub depth: [f64; 2],
    /// The facets' multipliers (top, right, bottom, left): Color times
    /// their light.
    pub shades: [[f32; 3]; 4],
    /// Solid's color, straight, with its alpha.
    pub color: [f32; 4],
}

/// A standard normal's cumulative distribution, through Abramowitz and
/// Stegun's 7.1.26 erf (as the card computes it).
pub(crate) fn phi(z: f64) -> f64 {
    let x = (z / std::f64::consts::SQRT_2).abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let erf = 1.0 - poly * (-x * x).exp();
    0.5 * (1.0 + if z < 0.0 { -erf } else { erf })
}

impl Border {
    /// Blurred: how much of the picture shows at `p` (canonical units from
    /// the frame's lower left).
    pub(crate) fn shown(&self, p: [f64; 2]) -> f64 {
        let (w, s) = (self.width, self.sigma);
        let axis = |t: f64, len: f64| {
            if s < 1e-9 {
                return if t > w && t < len - w { 1.0 } else { 0.0 };
            }
            (phi((t - w) / s) - phi((t - (len - w)) / s)).max(0.0)
        };
        axis(p[0], self.frame[0]) * axis(p[1], self.frame[1])
    }

    /// Beveled: the facet at `p` (0 top, 1 right, 2 bottom, 3 left), or
    /// `None` inside the border. Each side's distance as a part of its
    /// facet's depth: the nearest's facet, as the lines from the corners
    /// part them.
    pub(crate) fn facet(&self, p: [f64; 2]) -> Option<usize> {
        let [across, up] = self.depth;
        if across <= 0.0 || up <= 0.0 {
            return None;
        }
        let [w, h] = self.frame;
        let d = [
            (h - p[1]) / up,
            (w - p[0]) / across,
            p[1] / up,
            p[0] / across,
        ];
        let (i, least) = d
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, v)| (i, *v))?;
        (least < 1.0).then_some(i)
    }

    /// Solid: how much of a pixel around `p`, `pixel` canonical units wide
    /// and high, is inside the border (the picture's).
    pub(crate) fn inside(&self, p: [f64; 2], pixel: [f64; 2]) -> f64 {
        let w = self.width;
        let axis = |t: f64, size: f64, len: f64| {
            let lo = (t - size / 2.0).max(w);
            let hi = (t + size / 2.0).min(len - w);
            ((hi - lo) / size).clamp(0.0, 1.0)
        };
        axis(p[0], pixel[0], self.frame[0]) * axis(p[1], pixel[1], self.frame[1])
    }

    /// The pixel `px` (premultiplied) at `p`, a pixel being `pixel`
    /// canonical units wide and high.
    pub(crate) fn apply(&self, p: [f64; 2], pixel: [f64; 2], px: [f32; 4]) -> [f32; 4] {
        match self.kind {
            Kind::Blurred => {
                let k = self.shown(p) as f32;
                px.map(|c| c * k)
            }
            Kind::Beveled => match self.facet(p) {
                None => px,
                Some(i) => {
                    let m = self.shades[i];
                    let a = px[3];
                    [
                        (px[0] * m[0]).min(a),
                        (px[1] * m[1]).min(a),
                        (px[2] * m[2]).min(a),
                        a,
                    ]
                }
            },
            Kind::Solid => {
                let k = self.inside(p, pixel) as f32;
                let rim = (1.0 - k) * self.color[3];
                [
                    px[0] * k + self.color[0] * rim,
                    px[1] * k + self.color[1] * rim,
                    px[2] * k + self.color[2] * rim,
                    px[3] * k + rim,
                ]
            }
        }
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
        render(fx, args)
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        if Settings::at(fx, args.dbl(kOfxPropTime, 0)).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxActionInstanceChanged) {
        show(fx, args.dbl(kOfxPropTime, 0));
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) || is(action, kOfxActionBeginInstanceEdit) {
        show(fx, 0.0);
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

/// Shows only what the type uses, as VEGAS does: the angle and its color
/// for Beveled, the color with its alpha for Solid.
fn show(fx: Fx, time: f64) {
    let kind = Settings::at(fx, time).kind;
    fx.param(ANGLE).set_secret(kind != Kind::Beveled);
    fx.param(BEVEL_COLOR).set_secret(kind != Kind::Beveled);
    fx.param(COLOR).set_secret(kind != Kind::Solid);
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Border");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Creative");
    p.set_str(
        kOfxPropPluginDescription,
        c"Draws a border around the frame: the picture's edges blurred away, a bevel lit from one side, or a plain color.",
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
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);

    let p = fx.define_param(kOfxParamTypeChoice, TYPE, c"Type");
    p.set_strs(kOfxParamPropChoiceOption, &TYPES);
    p.set_int(kOfxParamPropDefault, 0);
    p.set_str(
        kOfxParamPropHint,
        c"Blurred: the picture's edges fade away. Beveled: a frame lit from one side. Solid: a plain frame of one color.",
    );

    let p = fx.define_param(kOfxParamTypeDouble, SIZE, c"Size");
    p.set_dbl(kOfxParamPropDefault, 0.0);
    p.set_dbl(kOfxParamPropMin, 0.0);
    p.set_dbl(kOfxParamPropMax, 1.0);
    p.set_dbl(kOfxParamPropDisplayMin, 0.0);
    p.set_dbl(kOfxParamPropDisplayMax, 1.0);
    p.set_dbl(kOfxParamPropIncrement, 0.01);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"How wide the border is: 1 is half the frame's width.",
    );

    let p = fx.define_param(kOfxParamTypeDouble, ANGLE, c"Angle");
    p.set_str(kOfxParamPropDoubleType, kOfxParamDoubleTypeAngle);
    p.set_dbl(kOfxParamPropDefault, DEFAULT_ANGLE);
    p.set_dbl(kOfxParamPropMin, 0.0);
    p.set_dbl(kOfxParamPropMax, 360.0);
    p.set_dbl(kOfxParamPropDisplayMin, 0.0);
    p.set_dbl(kOfxParamPropDisplayMax, 360.0);
    p.set_dbl(kOfxParamPropIncrement, 1.0);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"Where the light on the bevel comes from: 0 is the top, going clockwise, in degrees.",
    );

    let p = fx.define_param(kOfxParamTypeRGB, BEVEL_COLOR, c"Color");
    p.set_dbls(kOfxParamPropDefault, &[1.0, 1.0, 1.0]);
    p.set_str(kOfxParamPropHint, c"The bevel's color.");

    let p = fx.define_param(kOfxParamTypeRGBA, COLOR, c"Color");
    p.set_dbls(kOfxParamPropDefault, &WHITE);
    p.set_str(
        kOfxParamPropHint,
        c"The border's color; its alpha lets what is under it show.",
    );
    p.set_str(bv_ofx_sys::extensions::kBananaVParamPropColorModel, c"RGB");
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|s| if s > 0.0 { s } else { 1.0 });
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    if let Some(s) = &src
        && s.depth != out.depth
    {
        return kOfxStatErrImageFormat;
    }
    let (origin, frame) = fx.frame();
    let border = Settings::at(fx, time).on(frame);
    let par = out.par;
    let pixel = [par / scale[0], 1.0 / scale[1]];
    let src_premultiplied = src
        .as_ref()
        .is_some_and(|i| i.channels < 4 || !i.unpremultiplied);
    let out_straight = out.channels == 4 && out.unpremultiplied;
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(row) = out.row(y) else { return };
        let py = (y as f64 + 0.5) / scale[1] - origin[1];
        for x in columns.clone() {
            let px = src
                .as_ref()
                .and_then(|s| crate::colors::pixel(s, x, y))
                .map_or([0.0; 4], |c| {
                    if src_premultiplied {
                        c
                    } else {
                        [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
                    }
                });
            let p = [(x as f64 + 0.5) * par / scale[0] - origin[0], py];
            let mut v = border.apply(p, pixel, px);
            if out_straight && v[3] > 0.0 {
                v = [v[0] / v[3], v[1] / v[3], v[2] / v[3], v[3]];
            }
            // SAFETY: a row the output has, a column inside its bounds.
            unsafe { crate::colors::put_pixel(&out, row, x, v) };
        }
    };
    for_rows(y1.max(out.bounds[1])..y2.min(out.bounds[3]), &fill);
    drop(src);
    drop(out);
    kOfxStatOK
}

// Presets.

/// The factory presets: VEGAS's.
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    let preset =
        |name: &'static str, kind: f64, size: f64, rest: Vec<(&'static str, PresetValue)>| {
            let mut values = vec![("type", num(&[kind])), ("size", num(&[size]))];
            values.extend(rest);
            Preset { name, values }
        };
    // VEGAS's Pyramid's color, shown as HSV 56°, 0.31, 1.0.
    let pale_yellow = {
        let (s, v, h) = (0.31f64, 1.0f64, 56.0f64 / 60.0);
        let c = v * s;
        let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
        [c + v - c, x + v - c, v - c]
    };
    vec![
        preset("Soft Edge", 0.0, 0.05, vec![]),
        preset(
            "Bevel",
            1.0,
            0.1,
            vec![
                ("angle", num(&[325.0])),
                ("bevelColor", num(&[1.0, 1.0, 1.0])),
            ],
        ),
        preset(
            "Solid White Border",
            2.0,
            0.1,
            vec![("color", num(&[1.0, 1.0, 1.0, 1.0]))],
        ),
        preset(
            "Translucent Blue Border",
            2.0,
            0.1,
            vec![("color", num(&[0.0, 0.0, 1.0, 0.5]))],
        ),
        preset(
            "Pyramid",
            1.0,
            1.0,
            vec![("angle", num(&[240.0])), ("bevelColor", num(&pale_yellow))],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VEGAS's prints' frames: 654 × 368 pixels.
    const FRAME: [f64; 2] = [654.0, 368.0];

    fn border(kind: Kind, size: f64, angle: f64, bevel: [f64; 3], color: [f64; 4]) -> Border {
        Settings {
            kind,
            size,
            angle,
            bevel,
            color,
        }
        .on(FRAME)
    }

    #[test]
    fn phi_is_the_normal_distribution() {
        for (z, v) in [
            (0.0, 0.5),
            (1.0, 0.841_344_7),
            (-1.0, 0.158_655_3),
            (2.5, 0.993_790_3),
        ] {
            assert!((phi(z) - v).abs() < 2e-7, "{z}: {}", phi(z));
        }
    }

    #[test]
    fn soft_edge_fades_as_vegass() {
        // VEGAS's Soft Edge (0.05): half the picture 16.35 pixels in, a
        // Gaussian of 7.3 pixels; its measured profile, pixel by pixel.
        let b = border(Kind::Blurred, 0.05, 0.0, [1.0; 3], WHITE);
        let vegas = [
            0.008, 0.014, 0.025, 0.034, 0.049, 0.064, 0.096, 0.114, 0.144, 0.172, 0.230, 0.260,
            0.311, 0.346, 0.421, 0.461, 0.516, 0.557, 0.630, 0.672, 0.721, 0.755, 0.813, 0.843,
            0.874, 0.895, 0.927, 0.940, 0.955, 0.969, 0.982, 0.986, 0.993, 0.994, 1.0, 1.0,
        ];
        for (d, v) in vegas.iter().enumerate() {
            let k = b.shown([d as f64 + 0.5, 184.0]);
            assert!((k - v).abs() < 0.03, "{d}: {k} against {v}");
        }
        // A corner: both edges' fades.
        let k = b.shown([16.5, 16.5]);
        assert!((k - 0.271).abs() < 0.03, "{k}");
    }

    #[test]
    fn the_bevels_facets_are_lit_as_vegass() {
        // VEGAS's Bevel (325°, white): the bottom facet 0.476 times, the
        // right 0.645; its Pyramid (240°): top 0.694, right 0.444, bottom
        // 1.400, left 1.64 (over its color) — medians over each facet.
        let bevel = border(Kind::Beveled, 0.1, 325.0, [1.0; 3], WHITE);
        assert!((bevel.shades[2][0] - 0.476).abs() < 0.01);
        assert!((bevel.shades[1][0] - 0.645).abs() < 0.01);
        let pyramid = border(Kind::Beveled, 1.0, 240.0, [1.0; 3], WHITE);
        for (i, v) in [0.694, 0.444, 1.400, 1.64].into_iter().enumerate() {
            assert!(
                (pyramid.shades[i][0] - v).abs() < 0.02,
                "{i}: {:?}",
                pyramid.shades[i]
            );
        }
    }

    #[test]
    fn the_facets_meet_at_the_corners_lines() {
        // A band 32.7 pixels wide: mitered at 45°.
        let b = border(Kind::Beveled, 0.1, 0.0, [1.0; 3], WHITE);
        assert_eq!(b.facet([10.0, 184.0]), Some(3));
        assert_eq!(b.facet([327.0, 360.0]), Some(0));
        assert_eq!(b.facet([327.0, 5.0]), Some(2));
        assert_eq!(b.facet([650.0, 184.0]), Some(1));
        assert_eq!(b.facet([20.0, 368.0 - 10.0]), Some(0));
        assert_eq!(b.facet([10.0, 368.0 - 20.0]), Some(3));
        assert_eq!(b.facet([327.0, 184.0]), None);
        // Size 1: a pyramid, its facets parted by the frame's diagonals.
        let p = border(Kind::Beveled, 1.0, 0.0, [1.0; 3], WHITE);
        assert_eq!(p.facet([327.0, 300.0]), Some(0));
        assert_eq!(p.facet([100.0, 184.0]), Some(3));
        // Under the diagonal from the top left to the bottom right: the
        // bottom facet, over it the right one.
        assert_eq!(p.facet([600.0, 20.0]), Some(2));
        assert_eq!(p.facet([600.0, 60.0]), Some(1));
    }

    #[test]
    fn a_solid_border_covers_the_band_with_its_color() {
        // Translucent Blue Border: half see-through blue 32.7 pixels wide.
        let b = border(Kind::Solid, 0.1, 0.0, [1.0; 3], [0.0, 0.0, 1.0, 0.5]);
        let px = [0.8, 0.4, 0.2, 1.0];
        assert_eq!(b.apply([10.0, 184.0], [1.0, 1.0], px), [0.0, 0.0, 0.5, 0.5]);
        assert_eq!(b.apply([327.0, 184.0], [1.0, 1.0], px), px);
        // Its inner edge shares a pixel.
        let edge = b.apply([32.7, 184.0], [1.0, 1.0], px);
        assert!((edge[3] - 0.75).abs() < 1e-4, "{edge:?}");
    }

    #[test]
    fn no_size_changes_nothing() {
        for kind in [Kind::Blurred, Kind::Beveled, Kind::Solid] {
            let s = Settings {
                kind,
                ..Settings::default()
            };
            assert!(s.changes_nothing());
            let b = s.on(FRAME);
            let px = [0.3, 0.5, 0.7, 1.0];
            assert_eq!(b.apply([0.5, 0.5], [1.0, 1.0], px), px, "{kind:?}");
        }
    }

    #[test]
    fn the_presets_are_vegass() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            [
                "Soft Edge",
                "Bevel",
                "Solid White Border",
                "Translucent Blue Border",
                "Pyramid"
            ]
        );
    }
}
