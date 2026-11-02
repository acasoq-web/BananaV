//! Layer Dimensionality: VEGAS's. Gives a picture with transparency — a
//! title, a cut-out picture in picture — depth: a shadow dropped behind it
//! away from the light, or cast inside its edge as if the picture were a
//! hole; a glow of a color spread around it, inside its edge, hugging the
//! edge, or around and inside; and an embossed surface lit by the light
//! (the picture or what is around it raised or pressed in, the bevel hard
//! or round, matte or shiny), with the picture itself as opaque as
//! Foreground Opacity says.
//!
//! The light is somewhere over the frame: across and up from −3 to 3 (the
//! frame's edges) and how high (Light Location: a box and a slider of its
//! own, drawn with the draw suite, and a cross on the preview). It is far
//! away: it shines on every pixel from the same side, so a shadow moves as
//! a whole — a dropped one Height over the light's height times the frame
//! away from it, an inner one a tenth as far, and none when the light is
//! down on the frame. Those distances, the blurs and the glows were
//! measured on the pictures of VEGAS's presets; blurs and glows go with
//! the frame's width. Parameters a setting turns off are greyed out, as
//! VEGAS's are.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};
use std::time::Instant;

use bv_ofx_sys::extensions::{kBananaVParamPropInteract, kBananaVParamPropInteractOpen};
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::{self, Values};
use crate::ofx::{
    Clicks, Draw, Fx, Props, for_rows, interact_props, is, near_handle, redraw, suites,
};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.LayerDimensionality";

pub(crate) const SHADOW: &CStr = c"shadow";
pub(crate) const SHADOW_TYPE: &CStr = c"shadowType";
pub(crate) const SHADOW_HEIGHT: &CStr = c"shadowHeight";
pub(crate) const SHADOW_BLUR: &CStr = c"shadowBlur";
pub(crate) const SHADOW_OPACITY: &CStr = c"shadowOpacity";
pub(crate) const GLOW: &CStr = c"glow";
pub(crate) const GLOW_TYPE: &CStr = c"glowType";
pub(crate) const GLOW_AMOUNT: &CStr = c"glowAmount";
pub(crate) const GLOW_COLOR: &CStr = c"glowColor";
pub(crate) const EMBOSS: &CStr = c"emboss";
pub(crate) const FOREGROUND: &CStr = c"embossForeground";
pub(crate) const BACKGROUND: &CStr = c"embossBackground";
pub(crate) const INTENSITY: &CStr = c"lightIntensity";
pub(crate) const AMBIENCE: &CStr = c"lightAmbience";
pub(crate) const ROUNDNESS: &CStr = c"roundness";
pub(crate) const SHININESS: &CStr = c"shininess";
pub(crate) const OPACITY: &CStr = c"foregroundOpacity";
pub(crate) const LIGHT: &CStr = c"lightLocation";

const SHADOW_TYPES: [&CStr; 3] = [c"None", c"Drop", c"Inner"];
const GLOW_TYPES: [&CStr; 5] = [c"None", c"Outer", c"Inner", c"Edge", c"Dual"];

/// The shadow types: dropped behind the picture, inside its edge (0 is
/// none).
const DROP: c_int = 1;
const INNER: c_int = 2;
/// The glow types: spread around the picture, inside its edge, hugging its
/// edge outside, around and inside (0 is none).
const OUTER_GLOW: c_int = 1;
const INNER_GLOW: c_int = 2;
const EDGE_GLOW: c_int = 3;
const DUAL_GLOW: c_int = 4;

/// How steep a bevel is: its slope, rising 1 across its blur (VEGAS's
/// bevels are steep: a high light leaves their rim darker all around); how
/// much its shine shows; how much of the ground's shading shows around the
/// picture (fitted on VEGAS's presets' pictures).
const STEEP: f64 = 6.0;
const SHINE: f64 = 0.25;
const GROUND: f32 = 0.35;

/// The light's box: across and up, and its height (VEGAS's slider: 0 to
/// 6, its middle 3); where it is at first.
const LIGHT_SPAN: f64 = 3.0;
const LIGHT_HIGHEST: f64 = 6.0;
const LIGHT_DEFAULT: [f64; 3] = [-1.0, 1.0, 3.0];
/// The light's interface: its size in screen pixels.
const LIGHT_SIZE: [c_int; 2] = [300, 104];

/// The settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    /// 0 none, [`DROP`], [`INNER`].
    pub shadow: c_int,
    pub height: f64,
    pub blur: f64,
    pub shadow_opacity: f64,
    /// 0 none, [`OUTER_GLOW`], [`INNER_GLOW`], [`EDGE_GLOW`], [`DUAL_GLOW`].
    pub glow: c_int,
    pub amount: f64,
    pub glow_color: [f64; 3],
    pub foreground: f64,
    pub background: f64,
    pub intensity: f64,
    pub ambience: f64,
    pub roundness: f64,
    pub shininess: f64,
    pub opacity: f64,
    pub light: [f64; 3],
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            shadow: 0,
            height: 0.02,
            blur: 0.2,
            shadow_opacity: 1.0,
            glow: 0,
            amount: 0.15,
            glow_color: [1.0, 0.0, 0.0],
            foreground: 0.0,
            background: 0.0,
            intensity: 0.5,
            ambience: 0.1,
            roundness: 0.14,
            shininess: 0.5,
            opacity: 1.0,
            light: LIGHT_DEFAULT,
        }
    }
}

/// The blurs (Gaussian sigmas, canonical units) of the shadow, of the glow
/// around the edge and inside it, and of the bevel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Blurs {
    shadow: f64,
    around: f64,
    inside: f64,
    bevel: f64,
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        let d = Settings::default();
        Settings {
            shadow: v.int(SHADOW_TYPE, d.shadow),
            height: v.f64(SHADOW_HEIGHT, d.height).max(0.0),
            blur: v.f64(SHADOW_BLUR, d.blur).max(0.0),
            shadow_opacity: v.f64(SHADOW_OPACITY, d.shadow_opacity).clamp(0.0, 1.0),
            glow: v.int(GLOW_TYPE, d.glow),
            amount: v.f64(GLOW_AMOUNT, d.amount).max(0.0),
            glow_color: v.array(GLOW_COLOR, d.glow_color),
            foreground: v.f64(FOREGROUND, d.foreground),
            background: v.f64(BACKGROUND, d.background),
            intensity: v.f64(INTENSITY, d.intensity).max(0.0),
            ambience: v.f64(AMBIENCE, d.ambience).max(0.0),
            roundness: v.f64(ROUNDNESS, d.roundness).clamp(0.0, 1.0),
            shininess: v.f64(SHININESS, d.shininess).clamp(0.0, 1.0),
            opacity: v.f64(OPACITY, d.opacity).clamp(0.0, 1.0),
            light: v.array(LIGHT, d.light),
        }
    }

    /// As the graphics card takes them, on a frame `frame` canonical units
    /// big.
    pub(crate) fn layer(&self, frame: [f64; 2]) -> accel::Layer {
        let b = self.blurs(frame[0]);
        let f = |v: f64| v as f32;
        accel::Layer {
            shadow: self.shadow.max(0) as u32,
            glow: self.glow.max(0) as u32,
            sigmas: [b.shadow, b.around, b.inside, b.bevel],
            offset: self.offset(frame),
            shadow_opacity: f(self.shadow_opacity),
            glow_color: self.glow_color.map(f),
            relief: self.relief().map(f),
            embossed: self.embossed(),
            light: self.direction().map(f),
            ambience: f(self.ambience),
            intensity: f(self.intensity),
            shininess: f(self.shininess),
            opacity: f(self.opacity),
            steep: f(STEEP),
            shine: f(SHINE),
            ground: GROUND,
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        let f = |n: &CStr| fx.param(n).f64_at(time);
        Settings {
            shadow: fx.param(SHADOW_TYPE).int_at(time),
            height: f(SHADOW_HEIGHT).max(0.0),
            blur: f(SHADOW_BLUR).max(0.0),
            shadow_opacity: f(SHADOW_OPACITY).clamp(0.0, 1.0),
            glow: fx.param(GLOW_TYPE).int_at(time),
            amount: f(GLOW_AMOUNT).max(0.0),
            glow_color: fx.param(GLOW_COLOR).rgb_at(time),
            foreground: f(FOREGROUND),
            background: f(BACKGROUND),
            intensity: f(INTENSITY).max(0.0),
            ambience: f(AMBIENCE).max(0.0),
            roundness: f(ROUNDNESS).clamp(0.0, 1.0),
            shininess: f(SHININESS).clamp(0.0, 1.0),
            opacity: f(OPACITY).clamp(0.0, 1.0),
            light: fx.param(LIGHT).xyz_at(time),
        }
    }

    /// How far the picture and what is around it rise (Foreground and
    /// Background), as the shadow lets them: there is nothing around a
    /// picture whose shadow drops behind it to emboss, and a picture with
    /// an inner shadow is a hole, nothing raised (VEGAS greys those out).
    pub(crate) fn relief(&self) -> [f64; 2] {
        [
            if self.shadow == INNER {
                0.0
            } else {
                self.foreground
            },
            if self.shadow == DROP {
                0.0
            } else {
                self.background
            },
        ]
    }

    fn embossed(&self) -> bool {
        self.relief() != [0.0, 0.0]
    }

    /// Whether the light does something: for a shadow or an emboss.
    fn lit(&self) -> bool {
        self.shadow != 0 || self.embossed()
    }

    /// Whether it leaves the picture as it is.
    pub(crate) fn changes_nothing(&self) -> bool {
        self.shadow == 0 && self.glow == 0 && !self.embossed() && self.opacity == 1.0
    }

    /// How far the shadow moves (canonical units, y up) on a frame `frame`
    /// big: away from the light, Height over the light's height times the
    /// frame for a dropped one, a tenth of that inside the picture; none
    /// when the light is down on the frame (as VEGAS's).
    pub(crate) fn offset(&self, frame: [f64; 2]) -> [f64; 2] {
        let k = match self.shadow {
            DROP => self.height,
            INNER => self.height / 10.0,
            _ => return [0.0, 0.0],
        };
        let z = self.light[2];
        if z <= 1e-6 {
            return [0.0, 0.0];
        }
        [
            -self.light[0] * frame[0] * k / z,
            -self.light[1] * frame[1] * k / z,
        ]
    }

    /// The blurs on a frame `width` wide.
    fn blurs(&self, width: f64) -> Blurs {
        let shadow = match self.shadow {
            DROP => 0.023 * self.blur,
            // Sharper inside, softer the deeper the hole and the higher
            // the light.
            INNER => (0.0086 * self.blur).hypot(0.0045 * self.height * self.light[2].max(0.0)),
            _ => 0.0,
        };
        let around = matches!(self.glow, OUTER_GLOW | EDGE_GLOW | DUAL_GLOW);
        let inside = matches!(self.glow, INNER_GLOW | DUAL_GLOW);
        Blurs {
            shadow: shadow * width,
            around: if around {
                (0.0006 + 0.031 * self.amount) * width
            } else {
                0.0
            },
            inside: if inside {
                0.046 * self.amount * width
            } else {
                0.0
            },
            bevel: if self.embossed() {
                (0.0012 + 0.01 * self.roundness) * width
            } else {
                0.0
            },
        }
    }

    /// Where the light shines from: a unit vector, y up (straight down
    /// when it is nowhere).
    fn direction(&self) -> [f64; 3] {
        let [x, y, z] = [self.light[0], self.light[1], self.light[2].max(0.0)];
        let l = x.hypot(y).hypot(z);
        if l < 1e-9 {
            [0.0, 0.0, 1.0]
        } else {
            [x / l, y / l, z / l]
        }
    }
}

/// Greys out what the settings turn off, as VEGAS does.
fn enable(fx: Fx, time: f64) {
    let s = Settings::at(fx, time);
    for p in [SHADOW_HEIGHT, SHADOW_BLUR, SHADOW_OPACITY] {
        fx.param(p).set_enabled(s.shadow != 0);
    }
    for p in [GLOW_AMOUNT, GLOW_COLOR] {
        fx.param(p).set_enabled(s.glow != 0);
    }
    fx.param(FOREGROUND).set_enabled(s.shadow != INNER);
    fx.param(BACKGROUND).set_enabled(s.shadow != DROP);
    for p in [INTENSITY, AMBIENCE, ROUNDNESS, SHININESS] {
        fx.param(p).set_enabled(s.embossed());
    }
    fx.param(LIGHT).set_enabled(s.lit());
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
        // Glows and bevels reach for the source's pixels around, a shadow
        // for where it comes from.
        let time = args.dbl(kOfxPropTime, 0);
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        let (origin, frame) = fx.frame();
        let roi = reads(&Settings::at(fx, time), origin, frame, roi);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &roi);
        kOfxStatOK
    } else if is(action, kOfxActionInstanceChanged) {
        enable(fx, args.dbl(kOfxPropTime, 0));
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) || is(action, kOfxActionBeginInstanceEdit) {
        enable(fx, 0.0);
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

/// How far around a pixel its glows and bevel read (canonical units).
fn around(s: &Settings, width: f64) -> f64 {
    let b = s.blurs(width);
    3.0 * b.around.max(b.inside).max(b.bevel) + 2.0
}

/// Where the shadow over `rect` (x1, y1, x2, y2, canonical units) comes
/// from, on a frame `frame` big at `origin`: the rectangle moved back, as
/// far around as its blur reaches, within the frame — nothing beyond it
/// casts one. None when nothing can.
fn shadow_reads(
    s: &Settings,
    origin: [f64; 2],
    frame: [f64; 2],
    rect: [f64; 4],
) -> Option<[f64; 4]> {
    if s.shadow == 0 {
        return None;
    }
    let [dx, dy] = s.offset(frame);
    let m = 3.0 * s.blurs(frame[0]).shadow + 4.0;
    let r = [
        (rect[0] - dx - m).max(origin[0] - m),
        (rect[1] - dy - m).max(origin[1] - m),
        (rect[2] - dx + m).min(origin[0] + frame[0] + m),
        (rect[3] - dy + m).min(origin[1] + frame[1] + m),
    ];
    (r[0] < r[2] && r[1] < r[3]).then_some(r)
}

/// What the effect reads of its source for its output over `rect`
/// (canonical units): around it, and where its shadow comes from.
fn reads(s: &Settings, origin: [f64; 2], frame: [f64; 2], rect: [f64; 4]) -> [f64; 4] {
    let m = around(s, frame[0]);
    let r = [rect[0] - m, rect[1] - m, rect[2] + m, rect[3] + m];
    match shadow_reads(s, origin, frame, rect) {
        Some(c) => [
            r[0].min(c[0]),
            r[1].min(c[1]),
            r[2].max(c[2]),
            r[3].max(c[3]),
        ],
        None => r,
    }
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Layer Dimensionality");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Creative");
    p.set_str(
        kOfxPropPluginDescription,
        c"Gives a picture with transparency depth: a shadow, a glow and an embossed surface lit by a light.",
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
    // The light's cross on the preview.
    p.set_ptr(
        kOfxImageEffectPluginPropOverlayInteractV2,
        overlay as *mut c_void,
    );
}

fn describe_in_context(fx: Fx) {
    fx.define_clip(
        kOfxImageEffectSimpleSourceClipName,
        &[kOfxImageComponentRGBA, kOfxImageComponentRGB],
    );
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);

    // VEGAS's sections, closed at first.
    let group = |name: &CStr, label: &CStr| {
        let p = fx.define_param(kOfxParamTypeGroup, name, label);
        p.set_int(kOfxParamPropGroupOpen, 0);
    };
    let choice = |name: &CStr, label: &CStr, parent: &CStr, options: &[&CStr], hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeChoice, name, label);
        p.set_strs(kOfxParamPropChoiceOption, options);
        p.set_int(kOfxParamPropDefault, 0);
        p.set_str(kOfxParamPropParent, parent);
        p.set_str(kOfxParamPropHint, hint);
    };
    let number = |name: &CStr,
                  label: &CStr,
                  parent: Option<&CStr>,
                  [default, min, max]: [f64; 3],
                  hint: &CStr| {
        let p = fx.define_param(kOfxParamTypeDouble, name, label);
        p.set_dbl(kOfxParamPropDefault, default);
        p.set_dbl(kOfxParamPropMin, min);
        p.set_dbl(kOfxParamPropMax, max);
        p.set_dbl(kOfxParamPropDisplayMin, min);
        p.set_dbl(kOfxParamPropDisplayMax, max);
        p.set_dbl(kOfxParamPropIncrement, 0.01);
        p.set_int(kOfxParamPropDigits, 3);
        if let Some(parent) = parent {
            p.set_str(kOfxParamPropParent, parent);
        }
        p.set_str(kOfxParamPropHint, hint);
    };

    group(SHADOW, c"Shadow");
    choice(
        SHADOW_TYPE,
        c"Type",
        SHADOW,
        &SHADOW_TYPES,
        c"No shadow, a shadow dropped behind the picture away from the light, or one inside its edge, as if it were a hole.",
    );
    number(
        SHADOW_HEIGHT,
        c"Height",
        Some(SHADOW),
        [0.02, 0.0, 1.0],
        c"How far the shadow falls from the picture (the light's place too).",
    );
    number(
        SHADOW_BLUR,
        c"Blur",
        Some(SHADOW),
        [0.2, 0.0, 1.0],
        c"How softly the shadow fades.",
    );
    number(
        SHADOW_OPACITY,
        c"Opacity",
        Some(SHADOW),
        [1.0, 0.0, 1.0],
        c"How dark the shadow is.",
    );

    group(GLOW, c"Glow");
    choice(
        GLOW_TYPE,
        c"Type",
        GLOW,
        &GLOW_TYPES,
        c"No glow, a glow spread around the picture, one inside its edge, one hugging its edge, or one around and inside.",
    );
    number(
        GLOW_AMOUNT,
        c"Amount",
        Some(GLOW),
        [0.15, 0.0, 1.0],
        c"How far and how bright the glow is.",
    );
    let p = fx.define_param(kOfxParamTypeRGB, GLOW_COLOR, c"Color");
    p.set_dbls(kOfxParamPropDefault, &[1.0, 0.0, 0.0]);
    p.set_str(kOfxParamPropParent, GLOW);
    p.set_str(kOfxParamPropHint, c"The glow's color.");

    group(EMBOSS, c"Emboss");
    number(
        FOREGROUND,
        c"Foreground",
        Some(EMBOSS),
        [0.0, -1.0, 1.0],
        c"How far the picture rises (pressed in below 0).",
    );
    number(
        BACKGROUND,
        c"Background",
        Some(EMBOSS),
        [0.0, -1.0, 1.0],
        c"How far what is around the picture rises (pressed in below 0).",
    );
    number(
        INTENSITY,
        c"Light Intensity",
        Some(EMBOSS),
        [0.5, 0.0, 1.0],
        c"How strong the light is.",
    );
    number(
        AMBIENCE,
        c"Light Ambience",
        Some(EMBOSS),
        [0.1, 0.0, 1.0],
        c"How much light comes from all around.",
    );
    number(
        ROUNDNESS,
        c"Roundness",
        Some(EMBOSS),
        [0.14, 0.0, 1.0],
        c"A hard bevel at 0, a round one toward 1.",
    );
    number(
        SHININESS,
        c"Shininess",
        Some(EMBOSS),
        [0.5, 0.0, 1.0],
        c"How much the surface shines.",
    );

    number(
        OPACITY,
        c"Foreground Opacity",
        None,
        [1.0, 0.0, 1.0],
        c"How much of the picture itself shows (its shadow and glow stay).",
    );

    let p = fx.define_param(kOfxParamTypeDouble3D, LIGHT, c"Light Location");
    p.set_dbls(kOfxParamPropDefault, &LIGHT_DEFAULT);
    p.set_dbls(kOfxParamPropMin, &[-LIGHT_SPAN, -LIGHT_SPAN, 0.0]);
    p.set_dbls(kOfxParamPropMax, &[LIGHT_SPAN, LIGHT_SPAN, LIGHT_HIGHEST]);
    p.set_dbls(kOfxParamPropDisplayMin, &[-LIGHT_SPAN, -LIGHT_SPAN, 0.0]);
    p.set_dbls(
        kOfxParamPropDisplayMax,
        &[LIGHT_SPAN, LIGHT_SPAN, LIGHT_HIGHEST],
    );
    p.set_int(kOfxParamPropDigits, 3);
    p.set_dbl(kOfxParamPropIncrement, 0.01);
    p.set_str(
        kOfxParamPropHint,
        c"Where the light is: across and up over the frame (its edges at -3 and 3), and how high.",
    );
    // Its box and slider, where the host draws with the draw suite.
    p.set_ptr(kBananaVParamPropInteract, light_interface as *mut c_void);
    p.set_ints(kOfxParamPropInteractPreferedSize, &LIGHT_SIZE);
    // Closed under its triangle at first, as VEGAS's.
    p.set_int(kBananaVParamPropInteractOpen, 0);
}

// Rendering.

/// The radius of the box blurs (three each way) a Gaussian blur of
/// `sigma` pixels is made of; 0 for none.
pub(crate) fn box_radius(sigma: f64) -> usize {
    if sigma < 0.3 {
        return 0;
    }
    let width = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
    ((width - 1.0) / 2.0).round().max(1.0) as usize
}

/// A Gaussian blur of `buf` (`w` × `h`), as three box blurs; nothing
/// beyond its edges.
pub(crate) fn blur(buf: &mut [f32], w: usize, h: usize, sigma: f64) {
    let r = box_radius(sigma);
    if r == 0 || w == 0 || h == 0 {
        return;
    }
    let mut line = vec![0.0f32; w.max(h)];
    let norm = 1.0 / (2 * r + 1) as f32;
    for _ in 0..3 {
        for y in 0..h {
            let row = &mut buf[y * w..(y + 1) * w];
            let mut sum: f32 = row[..=r.min(w - 1)].iter().sum();
            for x in 0..w {
                line[x] = sum * norm;
                if x + r + 1 < w {
                    sum += row[x + r + 1];
                }
                if x >= r {
                    sum -= row[x - r];
                }
            }
            row.copy_from_slice(&line[..w]);
        }
        for x in 0..w {
            let mut sum = 0.0f32;
            for y in 0..=r.min(h - 1) {
                sum += buf[y * w + x];
            }
            for y in 0..h {
                line[y] = sum * norm;
                if y + r + 1 < h {
                    sum += buf[(y + r + 1) * w + x];
                }
                if y >= r {
                    sum -= buf[(y - r) * w + x];
                }
            }
            for y in 0..h {
                buf[y * w + x] = line[y];
            }
        }
    }
}

/// A picture read for the render: premultiplied colors and alpha over a
/// window (x0, y0 its lower left pixel).
struct Layer {
    x0: c_int,
    y0: c_int,
    w: usize,
    h: usize,
    rgba: Vec<[f32; 4]>,
}

impl Layer {
    fn at(&self, x: c_int, y: c_int) -> [f32; 4] {
        let (i, j) = (x - self.x0, y - self.y0);
        if i < 0 || j < 0 || i as usize >= self.w || j as usize >= self.h {
            return [0.0; 4];
        }
        self.rgba[j as usize * self.w + i as usize]
    }

    /// Its alpha, blurred by `sigma` pixels.
    fn alpha(&self, sigma: f64) -> Alpha {
        let mut buf: Vec<f32> = self.rgba.iter().map(|p| p[3]).collect();
        blur(&mut buf, self.w, self.h, sigma);
        Alpha {
            x0: self.x0,
            y0: self.y0,
            w: self.w,
            h: self.h,
            buf,
        }
    }
}

/// A picture's alpha over a window of pixels (x0, y0 its lower left),
/// blurred.
struct Alpha {
    x0: c_int,
    y0: c_int,
    w: usize,
    h: usize,
    buf: Vec<f32>,
}

impl Alpha {
    /// `alpha` over `rect` (x1, y1, x2, y2), blurred by `sigma` pixels.
    fn read(alpha: &dyn Fn(c_int, c_int) -> f32, rect: [c_int; 4], sigma: f64) -> Alpha {
        let [x1, y1, x2, y2] = rect;
        let (w, h) = ((x2 - x1).max(0) as usize, (y2 - y1).max(0) as usize);
        let mut buf = Vec::with_capacity(w * h);
        for j in 0..h as c_int {
            for i in 0..w as c_int {
                buf.push(alpha(x1 + i, y1 + j));
            }
        }
        blur(&mut buf, w, h, sigma);
        Alpha {
            x0: x1,
            y0: y1,
            w,
            h,
            buf,
        }
    }

    /// At a pixel: nothing outside the window read.
    fn at(&self, x: c_int, y: c_int) -> f32 {
        let (i, j) = (x - self.x0, y - self.y0);
        if i < 0 || j < 0 || i as usize >= self.w || j as usize >= self.h {
            return 0.0;
        }
        self.buf[j as usize * self.w + i as usize]
    }
}

/// The picture read around the window and its blurred alphas, ready for
/// each output pixel to be worked out (on any thread).
pub(crate) struct Plan {
    s: Settings,
    layer: Layer,
    /// The picture's alpha blurred for its shadow, where the shadow comes
    /// from.
    shadow: Option<Alpha>,
    /// For the glow around the edge, the glow inside it, and the bevel.
    around: Option<Alpha>,
    inside: Option<Alpha>,
    bevel: Option<Alpha>,
    /// The shadow's move, in pixels.
    off: [c_int; 2],
    /// How steep the bevel is, per unit of rise.
    steep: f64,
    /// Where the light shines from.
    light: [f64; 3],
}

impl Plan {
    /// The plan for the output pixels `window` (x1, y1, x2, y2) of the
    /// effect over `src` (premultiplied RGBA) at render scale `scale`, on
    /// a frame `frame` canonical units big whose lower left is `origin`.
    pub(crate) fn new(
        s: &Settings,
        src: &dyn Fn(c_int, c_int) -> [f32; 4],
        window: [c_int; 4],
        scale: f64,
        origin: [f64; 2],
        frame: [f64; 2],
    ) -> Plan {
        let [x1, y1, x2, y2] = window;
        let b = s.blurs(frame[0]);
        // The picture around the window, as far as the glows and the
        // bevel reach.
        let m = (around(s, frame[0]) * scale).ceil() as c_int;
        let layer = {
            let (x0, y0) = (x1 - m, y1 - m);
            let (w, h) = (
                (x2 - x1 + 2 * m).max(0) as usize,
                (y2 - y1 + 2 * m).max(0) as usize,
            );
            let mut rgba = Vec::with_capacity(w * h);
            for j in 0..h as c_int {
                for i in 0..w as c_int {
                    rgba.push(src(x0 + i, y0 + j));
                }
            }
            Layer { x0, y0, w, h, rgba }
        };
        let off = s.offset(frame).map(|v| (v * scale).round() as c_int);
        // The shadow's, where it comes from: the window moved back, within
        // the frame.
        let shadow = (s.shadow != 0).then(|| {
            let ms = (3.0 * b.shadow * scale).ceil() as c_int + 2;
            let px = |v: f64| (v * scale).round() as c_int;
            let rect = [
                (x1 - off[0] - ms).max(px(origin[0]) - ms),
                (y1 - off[1] - ms).max(px(origin[1]) - ms),
                (x2 - off[0] + ms).min(px(origin[0] + frame[0]) + ms),
                (y2 - off[1] + ms).min(px(origin[1] + frame[1]) + ms),
            ];
            Alpha::read(&|x, y| src(x, y)[3], rect, b.shadow * scale)
        });
        let around = matches!(s.glow, OUTER_GLOW | EDGE_GLOW | DUAL_GLOW)
            .then(|| layer.alpha(b.around * scale));
        let inside =
            matches!(s.glow, INNER_GLOW | DUAL_GLOW).then(|| layer.alpha(b.inside * scale));
        let bevel = s.embossed().then(|| layer.alpha(b.bevel * scale));
        Plan {
            s: *s,
            shadow,
            around,
            inside,
            bevel,
            off,
            steep: (b.bevel * scale).max(1.0) * STEEP,
            light: s.direction(),
            layer,
        }
    }

    /// The light on a bevel at a pixel, its surface rising `rise` as the
    /// blurred alpha `b` does: how much brighter than flat it is (1 as
    /// flat; toward the light brighter, away from it darker) and its shine.
    fn shade(&self, b: &Alpha, rise: f64, x: c_int, y: c_int) -> (f32, f32) {
        let s = &self.s;
        let h = |x: c_int, y: c_int| rise * b.at(x, y) as f64;
        let n = [
            -(h(x + 1, y) - h(x - 1, y)) / 2.0 * self.steep,
            -(h(x, y + 1) - h(x, y - 1)) / 2.0 * self.steep,
            1.0,
        ];
        let nl = n[0].hypot(n[1]).hypot(1.0);
        let n = n.map(|c| c / nl);
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let l = self.light;
        let flat = (s.ambience + s.intensity * l[2]).max(0.05);
        let lit = s.ambience + s.intensity * dot(n, l);
        let k = (lit / flat).clamp(0.0, 3.0) as f32;
        let half = [l[0], l[1], l[2] + 1.0];
        let hl = dot(half, half).sqrt().max(1e-9);
        let e = 4.0 + 60.0 * s.shininess;
        let spec = |n: [f64; 3]| (dot(n, half) / hl).max(0.0).powf(e);
        let shine = (SHINE * s.shininess * (spec(n) - spec([0.0, 0.0, 1.0])).max(0.0)) as f32;
        (k, shine)
    }

    /// The output pixel at `x`, `y` (premultiplied RGBA).
    pub(crate) fn pixel(&self, x: c_int, y: c_int) -> [f32; 4] {
        let s = &self.s;
        let mut px = self.layer.at(x, y);
        let a = px[3];
        let [rise, ground] = s.relief();
        // Under the picture, from the bottom up: the ground embossed, a
        // dropped shadow, the glow around.
        let mut under = [0.0f32; 4];
        if let Some(b) = &self.bevel {
            // The picture's surface…
            if rise != 0.0 && a > 0.0 {
                let (k, shine) = self.shade(b, rise, x, y);
                for c in px.iter_mut().take(3) {
                    *c = (*c * k + shine * a).max(0.0);
                }
            }
            // …and the ground around it, darker or lighter where it
            // slopes.
            if ground != 0.0 && a < 1.0 {
                let (k, shine) = self.shade(b, -ground, x, y);
                let open = 1.0 - a;
                let dark = (1.0 - k).clamp(0.0, 1.0) * GROUND * open;
                let light = (((k - 1.0).max(0.0) + shine) * GROUND).min(1.0) * open;
                under = [light, light, light, light + dark * (1.0 - light)];
            }
        }
        let back = |x: c_int, y: c_int, f: &Alpha| f.at(x - self.off[0], y - self.off[1]);
        // An inner shadow: what is around the picture, moved away from the
        // light, darkens its edge.
        if s.shadow == INNER
            && let Some(f) = &self.shadow
        {
            let k = ((1.0 - back(x, y, f)) * s.shadow_opacity as f32).clamp(0.0, 1.0);
            for c in px.iter_mut().take(3) {
                *c *= 1.0 - k;
            }
        }
        let color = s.glow_color.map(|c| c as f32);
        // A glow inside the edge, over the picture.
        if let Some(g) = &self.inside {
            let k = (1.0 - g.at(x, y)).clamp(0.0, 1.0);
            for (p, c) in px.iter_mut().zip(color) {
                *p += (c * a - *p) * k;
            }
        }
        let o = s.opacity as f32;
        let fg = px.map(|c| c * o);
        if s.shadow == DROP
            && let Some(f) = &self.shadow
        {
            let k = back(x, y, f) * s.shadow_opacity as f32;
            under = [
                under[0] * (1.0 - k),
                under[1] * (1.0 - k),
                under[2] * (1.0 - k),
                k + under[3] * (1.0 - k),
            ];
        }
        if let Some(g) = &self.around {
            // Spread around or hugging the edge, as the blurred alpha: half
            // at the edge (VEGAS's two look alike on its presets'
            // pictures).
            let k = g.at(x, y).min(1.0);
            under = [
                color[0] * k + under[0] * (1.0 - k),
                color[1] * k + under[1] * (1.0 - k),
                color[2] * k + under[2] * (1.0 - k),
                k + under[3] * (1.0 - k),
            ];
        }
        let left = 1.0 - fg[3];
        [
            fg[0] + under[0] * left,
            fg[1] + under[1] * left,
            fg[2] + under[2] * left,
            fg[3] + under[3] * left,
        ]
    }
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let window: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = if scale[0] > 0.0 { scale[0] } else { 1.0 };
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    if out.channels != 4 {
        return kOfxStatErrImageFormat;
    }
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    if let Some(s) = &src
        && (s.depth != out.depth || s.channels < 3)
    {
        return kOfxStatErrImageFormat;
    }
    let premultiplied = !src.as_ref().is_some_and(|s| s.unpremultiplied);
    let read = |x: c_int, y: c_int| -> [f32; 4] {
        let Some(p) = src.as_ref().and_then(|s| crate::colors::pixel(s, x, y)) else {
            return [0.0; 4];
        };
        if premultiplied {
            p
        } else {
            [p[0] * p[3], p[1] * p[3], p[2] * p[3], p[3]]
        }
    };
    let (origin, frame) = fx.frame();
    let settings = Settings::at(fx, time);
    let [x1, y1, x2, y2] = window;
    let window = [
        x1.max(out.bounds[0]),
        y1.max(out.bounds[1]),
        x2.min(out.bounds[2]),
        y2.min(out.bounds[3]),
    ];
    let plan = Plan::new(&settings, &read, window, scale, origin, frame);
    let depth = out.depth;
    let put = |y: c_int| {
        let Some(o) = out.row(y) else { return };
        for x in window[0]..window[2] {
            let px = plan.pixel(x, y);
            let k = (x - out.bounds[0]) as usize * 4;
            // SAFETY: a pixel of a row the output has, in its depth.
            unsafe {
                for (c, v) in px.iter().enumerate() {
                    match depth {
                        crate::ofx::Depth::Float => *(o as *mut f32).add(k + c) = *v,
                        crate::ofx::Depth::Short => {
                            *(o as *mut u16).add(k + c) =
                                (v.clamp(0.0, 1.0) * 65535.0).round() as u16
                        }
                        crate::ofx::Depth::Byte => {
                            *o.add(k + c) = (v.clamp(0.0, 1.0) * 255.0).round() as u8
                        }
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

// The light's cross on the preview.

/// Where the light is over the frame (canonical units): its place across
/// and up, the frame's edges at ±3, kept on the frame.
fn light_point(light: [f64; 3], origin: [f64; 2], frame: [f64; 2]) -> [f64; 2] {
    let f = |v: f64| ((v / LIGHT_SPAN).clamp(-1.0, 1.0) + 1.0) / 2.0;
    [
        origin[0] + f(light[0]) * frame[0],
        origin[1] + f(light[1]) * frame[1],
    ]
}

/// The light's place across and up for a point over the frame.
fn light_place(p: [f64; 2], origin: [f64; 2], frame: [f64; 2]) -> [f64; 2] {
    let f = |v: f64, o: f64, size: f64| ((v - o) / size * 2.0 - 1.0).clamp(-1.0, 1.0) * LIGHT_SPAN;
    [f(p[0], origin[0], frame[0]), f(p[1], origin[1], frame[1])]
}

/// What the overlay remembers: the cross taken (where the pen took it,
/// from its middle), and the cross under the pen.
#[derive(Debug, Default)]
struct Overlay {
    drag: Option<[f64; 2]>,
    hover: bool,
}

/// The overlay: the light's cross over the frame while the light does
/// something (as VEGAS's), dragged to move it across and up.
unsafe extern "C" fn overlay(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if is(action, kOfxActionDescribe) {
        return kOfxStatOK;
    }
    let Some(props) = interact_props(handle) else {
        return kOfxStatReplyDefault;
    };
    if is(action, kOfxActionCreateInstance) {
        let state = Box::into_raw(Box::new(Overlay::default()));
        props.set_ptr(kOfxPropInstanceData, state as *mut c_void);
        return kOfxStatOK;
    }
    let state = props.ptr(kOfxPropInstanceData) as *mut Overlay;
    if is(action, kOfxActionDestroyInstance) {
        if !state.is_null() {
            // SAFETY: made above, freed once.
            drop(unsafe { Box::from_raw(state) });
            props.set_ptr(kOfxPropInstanceData, std::ptr::null_mut());
        }
        return kOfxStatOK;
    }
    if state.is_null() {
        return kOfxStatReplyDefault;
    }
    // SAFETY: this overlay's state; hosts run one of its actions at a time.
    let state = unsafe { &mut *state };
    let args = Props(in_args);
    let fx = Fx(args.ptr(kOfxPropEffectInstance) as OfxImageEffectHandle);
    if fx.0.is_null() {
        return kOfxStatReplyDefault;
    }
    let time = args.dbl(kOfxPropTime, 0);
    let s = Settings::at(fx, time);
    if !s.lit() {
        *state = Overlay::default();
        return if is(action, kOfxInteractActionDraw) {
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        };
    }
    let ps: [f64; 2] = args.dbls(kOfxInteractPropPixelScale);
    let ps = ps.map(|v| if v.abs() > 1e-12 { v.abs() } else { 1.0 });
    let (origin, frame) = fx.frame();
    let at = light_point(s.light, origin, frame);
    if is(action, kOfxInteractActionDraw) {
        if let Some(d) = Draw::of(args) {
            d.handle(at, ps, state.hover || state.drag.is_some());
        }
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    let near = near_handle(pen, at, ps);
    if is(action, kOfxInteractActionPenDown) {
        if !near {
            return kOfxStatReplyDefault;
        }
        state.drag = Some([pen[0] - at[0], pen[1] - at[1]]);
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        match state.drag {
            Some(grip) => {
                let [x, y] = light_place([pen[0] - grip[0], pen[1] - grip[1]], origin, frame);
                fx.param(LIGHT).set_xyz([x, y, s.light[2]]);
                kOfxStatOK
            }
            None => {
                if near != state.hover {
                    state.hover = near;
                    redraw(handle);
                }
                kOfxStatReplyDefault
            }
        }
    } else if is(action, kOfxInteractActionPenUp) {
        if state.drag.take().is_some() {
            redraw(handle);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else {
        kOfxStatReplyDefault
    }
}

// The light's interface: a box for where it is over the frame, a slider
// for how high.

/// Where the parts are, in its pixels (y up): the box and the slider.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LightLayout {
    pub area: [f64; 4],
    pub slider: [f64; 4],
}

impl LightLayout {
    pub(crate) fn of(size: [f64; 2]) -> LightLayout {
        let side = (size[1] - 8.0).clamp(30.0, 96.0);
        let x = (size[0] * 0.55 - side / 2.0).max(4.0);
        let y = (size[1] - side) / 2.0;
        LightLayout {
            area: [x, y, x + side, y + side],
            slider: [x + side + 26.0, y, x + side + 46.0, y + side],
        }
    }

    /// The light's place across and up in the box, from a point.
    pub(crate) fn place_at(&self, p: [f64; 2]) -> [f64; 2] {
        let [x0, y0, x1, y1] = self.area;
        let f = |v: f64, a: f64, b: f64| {
            ((v - a) / (b - a)).clamp(0.0, 1.0) * 2.0 * LIGHT_SPAN - LIGHT_SPAN
        };
        [f(p[0], x0, x1), f(p[1], y0, y1)]
    }

    pub(crate) fn height_at(&self, y: f64) -> f64 {
        let [_, y0, _, y1] = self.slider;
        ((y - y0) / (y1 - y0)).clamp(0.0, 1.0) * LIGHT_HIGHEST
    }

    fn point_of(&self, light: [f64; 3]) -> [f64; 2] {
        let [x0, y0, x1, y1] = self.area;
        let f = |v: f64| (v + LIGHT_SPAN) / (2.0 * LIGHT_SPAN);
        [
            x0 + f(light[0]).clamp(0.0, 1.0) * (x1 - x0),
            y0 + f(light[1]).clamp(0.0, 1.0) * (y1 - y0),
        ]
    }
}

/// What the light's interface remembers: the part being dragged (0 the
/// box, 1 the slider), and its presses, to tell a double click.
#[derive(Debug, Default)]
struct LightState {
    drag: Option<u8>,
    clicks: Clicks,
}

unsafe extern "C" fn light_interface(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if is(action, kOfxActionDescribe) {
        return kOfxStatOK;
    }
    let Some(props) = interact_props(handle) else {
        return kOfxStatReplyDefault;
    };
    if is(action, kOfxActionCreateInstance) {
        let state = Box::into_raw(Box::new(LightState::default()));
        props.set_ptr(kOfxPropInstanceData, state as *mut c_void);
        return kOfxStatOK;
    }
    let state = props.ptr(kOfxPropInstanceData) as *mut LightState;
    if is(action, kOfxActionDestroyInstance) {
        if !state.is_null() {
            // SAFETY: made above, freed once.
            drop(unsafe { Box::from_raw(state) });
            props.set_ptr(kOfxPropInstanceData, std::ptr::null_mut());
        }
        return kOfxStatOK;
    }
    if state.is_null() {
        return kOfxStatReplyDefault;
    }
    // SAFETY: this interface's state; hosts run one of its actions at a
    // time.
    let state = unsafe { &mut *state };
    let args = Props(in_args);
    let fx = Fx(args.ptr(kOfxPropEffectInstance) as OfxImageEffectHandle);
    if fx.0.is_null() {
        return kOfxStatReplyDefault;
    }
    let time = args.dbl(kOfxPropTime, 0);
    let size: [c_int; 2] = props.ints(kOfxInteractPropViewportSize);
    let l = LightLayout::of([size[0] as f64, size[1] as f64]);
    let param = fx.param(LIGHT);
    let light = param.xyz_at(time);
    if is(action, kOfxInteractActionDraw) {
        if let Some(d) = Draw::of(args) {
            draw_light(&d, &l, light);
        }
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    let within = |r: [f64; 4], m: f64| {
        pen[0] >= r[0] - m && pen[0] <= r[2] + m && pen[1] >= r[1] - m && pen[1] <= r[3] + m
    };
    let set = |part: u8| {
        let v = if part == 0 {
            let [x, y] = l.place_at(pen);
            [x, y, light[2]]
        } else {
            [light[0], light[1], l.height_at(pen[1])]
        };
        param.set_xyz(v);
    };
    if is(action, kOfxInteractActionPenDown) {
        let part = if within(l.area, 2.0) {
            0
        } else if within(l.slider, 4.0) {
            1
        } else {
            return kOfxStatReplyDefault;
        };
        if state.clicks.press(pen, Instant::now()) {
            // A double click puts it back where it starts, as VEGAS's: the
            // box the whole light, the slider its height (its middle).
            param.set_xyz(if part == 0 {
                LIGHT_DEFAULT
            } else {
                [light[0], light[1], LIGHT_DEFAULT[2]]
            });
            state.drag = None;
            return kOfxStatOK;
        }
        state.drag = Some(part);
        set(part);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        match state.drag {
            Some(part) => {
                set(part);
                kOfxStatOK
            }
            None => kOfxStatReplyDefault,
        }
    } else if is(action, kOfxInteractActionPenUp) {
        if state.drag.take().is_some() {
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else {
        kOfxStatReplyDefault
    }
}

/// The box (the light a cross in it) and the slider (its knob at the
/// light's height), as VEGAS draws a position's box.
fn draw_light(d: &Draw, l: &LightLayout, light: [f64; 3]) {
    let rect = |r: [f64; 4]| [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]];
    d.colour([0.1, 0.1, 0.1, 1.0]);
    d.fill(&rect(l.area));
    d.colour([0.35, 0.35, 0.35, 1.0]);
    d.width(1.0);
    d.outline(&rect(l.area));
    let [cx, cy] = l.point_of(light);
    d.colour([0.25, 0.85, 0.95, 1.0]);
    d.width(1.5);
    d.lines(&[
        [cx - 7.0, cy],
        [cx + 7.0, cy],
        [cx, cy - 7.0],
        [cx, cy + 7.0],
    ]);
    // The slider: a groove and a knob.
    let [x0, y0, x1, y1] = l.slider;
    let mid = (x0 + x1) / 2.0;
    d.colour([0.18, 0.18, 0.18, 1.0]);
    d.fill(&rect([mid - 3.0, y0, mid + 3.0, y1]));
    d.colour([0.35, 0.35, 0.35, 1.0]);
    d.outline(&rect([mid - 3.0, y0, mid + 3.0, y1]));
    let ky = y0 + (light[2] / LIGHT_HIGHEST).clamp(0.0, 1.0) * (y1 - y0);
    let knob = [x0, ky - 5.0, x1, ky + 5.0];
    d.colour([0.75, 0.75, 0.75, 1.0]);
    d.fill(&rect(knob));
    d.colour([0.3, 0.3, 0.3, 1.0]);
    d.outline(&rect(knob));
}

// Presets.

/// A color as VEGAS's picker shows it: hue in degrees, saturation and
/// value.
fn hsv(h: f64, s: f64, v: f64) -> [f64; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let (c, lo) = (v * s, v * (1.0 - s));
    let x = |t: f64| lo + c * (1.0 - (t % 2.0 - 1.0).abs());
    match h as u32 {
        0 => [v, x(h), lo],
        1 => [x(h), v, lo],
        2 => [lo, v, x(h)],
        3 => [lo, x(h), v],
        4 => [x(h), lo, v],
        _ => [v, lo, x(h)],
    }
}

/// The factory presets, VEGAS's (read off its settings): their names and
/// the values not at their defaults.
pub(crate) fn presets() -> Vec<Preset> {
    let d = Settings::default();
    let preset = |name: &'static str,
                  [shadow, height, blur, shadow_opacity]: [f64; 4],
                  [glow, amount]: [f64; 2],
                  color: [f64; 3],
                  [fg, bg, intensity, ambience, roundness, shininess]: [f64; 6],
                  light: [f64; 3]| {
        let given: [(&'static CStr, Vec<f64>, Vec<f64>); 15] = [
            (SHADOW_TYPE, vec![shadow], vec![d.shadow as f64]),
            (SHADOW_HEIGHT, vec![height], vec![d.height]),
            (SHADOW_BLUR, vec![blur], vec![d.blur]),
            (SHADOW_OPACITY, vec![shadow_opacity], vec![d.shadow_opacity]),
            (GLOW_TYPE, vec![glow], vec![d.glow as f64]),
            (GLOW_AMOUNT, vec![amount], vec![d.amount]),
            (GLOW_COLOR, color.to_vec(), d.glow_color.to_vec()),
            (FOREGROUND, vec![fg], vec![d.foreground]),
            (BACKGROUND, vec![bg], vec![d.background]),
            (INTENSITY, vec![intensity], vec![d.intensity]),
            (AMBIENCE, vec![ambience], vec![d.ambience]),
            (ROUNDNESS, vec![roundness], vec![d.roundness]),
            (SHININESS, vec![shininess], vec![d.shininess]),
            (OPACITY, vec![1.0], vec![d.opacity]),
            (LIGHT, light.to_vec(), d.light.to_vec()),
        ];
        let values = given
            .into_iter()
            .filter(|(_, v, default)| v != default)
            .map(|(param, v, _)| {
                let param = param.to_str().expect("parameter names are ASCII");
                (param, PresetValue::Num(v))
            })
            .collect();
        Preset { name, values }
    };
    const NONE: f64 = 0.0;
    const DROPPED: f64 = DROP as f64;
    const INSIDE: f64 = INNER as f64;
    const EDGE: f64 = EDGE_GLOW as f64;
    const OUTER: f64 = OUTER_GLOW as f64;
    const BLACK: [f64; 3] = [0.0; 3];
    const RED: [f64; 3] = [1.0, 0.0, 0.0];
    const WHITE: [f64; 3] = [1.0; 3];
    vec![
        preset(
            "Drop Shadow With Dark Inner Glow",
            [DROPPED, 0.021, 0.158, 1.0],
            [INNER_GLOW as f64, 0.1],
            BLACK,
            [0.0, 0.0, 0.5, 1.0, 0.14, 0.5],
            [-3.0, -0.034, 4.382],
        ),
        preset(
            "Drop Shadow With Down Light",
            [DROPPED, 0.032, 0.389, 1.0],
            [NONE, 0.15],
            RED,
            [0.0, 0.0, 0.5, 1.0, 0.14, 0.5],
            [-0.068, 3.0, 3.0],
        ),
        preset(
            "Drop Shadow With Embossing",
            [DROPPED, 0.011, 0.379, 0.853],
            [NONE, 0.15],
            RED,
            [0.6, 0.0, 0.589, 0.221, 0.0, 0.358],
            [-3.0, 3.0, 3.0],
        ),
        preset(
            "Drop Shadow With Light Above",
            [DROPPED, 1.0, 0.621, 1.0],
            [NONE, 0.105],
            hsv(131.0, 1.0, 1.0),
            [0.0, 0.0, 1.0, 0.0, 1.0, 0.568],
            [2.591, 3.0, 0.0],
        ),
        preset(
            "Drop Shadow With Subtle Opacity",
            [DROPPED, 0.042, 0.284, 0.453],
            [NONE, 0.105],
            hsv(131.0, 1.0, 1.0),
            [0.0, 0.0, 1.0, 0.232, 1.0, 1.0],
            [1.432, 1.315, 3.573],
        ),
        preset(
            "Drop Shadow With Up Light",
            [DROPPED, 0.042, 0.263, 0.505],
            [NONE, 0.2],
            hsv(200.0, 1.0, 1.0),
            [0.0, 0.0, 1.0, 0.0, 1.0, 0.568],
            [0.068, -3.0, 4.921],
        ),
        preset(
            "Drop Shadow With Upper Left Light",
            [DROPPED, 0.021, 0.263, 0.453],
            [NONE, 0.15],
            RED,
            [0.284, 0.0, 1.0, 0.284, 0.035, 0.358],
            [-1.636, 1.989, 3.0],
        ),
        preset(
            "Edge Glow Blue",
            [NONE, 0.011, 0.116, 0.453],
            [EDGE, 0.147],
            hsv(223.0, 0.48, 0.91),
            [0.0, 0.0, 1.0, 0.284, 0.084, 0.516],
            [0.0, 3.0, 3.0],
        ),
        preset(
            "Edge Glow With Inner Shadow",
            [INSIDE, 0.189, 0.189, 0.811],
            [EDGE, 0.168],
            RED,
            [0.0, 0.0, 1.0, 0.0, 1.0, 0.568],
            [-1.5, 1.449, 3.303],
        ),
        preset(
            "Emboss Strong",
            [NONE, 0.02, 0.2, 1.0],
            [NONE, 0.042],
            hsv(344.0, 1.0, 0.95),
            [0.789, 0.0, 0.221, 0.179, 0.168, 0.368],
            [0.0, 3.0, 6.0],
        ),
        preset(
            "Emboss Subtle",
            [NONE, 0.02, 0.2, 1.0],
            [NONE, 0.042],
            hsv(344.0, 1.0, 0.95),
            [0.432, 0.0, 0.779, 0.937, 0.074, 0.5],
            [-3.0, 3.0, 3.0],
        ),
        preset(
            "Inner Shadow Top Left Light",
            [INSIDE, 0.134, 0.181, 0.863],
            [NONE, 0.15],
            RED,
            [0.0, 0.0, 1.0, 0.284, 0.035, 0.358],
            [-1.636, 1.989, 3.0],
        ),
        preset(
            "Inner Shadow With Down Light",
            [INSIDE, 0.295, 0.137, 0.8],
            [NONE, 0.126],
            BLACK,
            [0.0, 0.0, 1.0, 0.0, 1.0, 0.568],
            [-0.068, 3.0, 6.0],
        ),
        preset(
            "Inner Shadow With Edge Glow",
            [INSIDE, 0.274, 0.284, 1.0],
            [EDGE, 0.053],
            WHITE,
            [0.0, 0.0, 1.0, 0.2, 1.0, 1.0],
            [-2.182, 3.0, 6.0],
        ),
        // Its glow hugs the edge, as VEGAS's preset has it.
        preset(
            "Inner Shadow Outer Glow",
            [INSIDE, 0.253, 0.368, 1.0],
            [EDGE, 0.295],
            hsv(207.0, 1.0, 1.0),
            [0.0, -1.0, 1.0, 0.2, 1.0, 1.0],
            [2.795, -2.596, 0.0],
        ),
        preset(
            "Inner Shadow With Stroke",
            [INSIDE, 0.2, 0.2, 0.716],
            [EDGE, 0.042],
            hsv(0.0, 0.0, 0.3),
            [0.0, 0.0, 1.0, 0.0, 1.0, 0.568],
            [-1.091, 1.18, 2.764],
        ),
        preset(
            "Outer Glow With Drop Shadow",
            [DROPPED, 0.053, 0.116, 0.453],
            [OUTER, 0.422],
            WHITE,
            [0.0, 0.0, 1.0, 0.284, 0.035, 0.358],
            [0.205, 1.247, 3.0],
        ),
        preset(
            "Outer Glow with Embossing",
            [NONE, 0.011, 0.116, 0.453],
            [OUTER, 0.484],
            hsv(59.0, 1.0, 1.0),
            [0.389, 0.0, 0.516, 0.432, 0.074, 0.347],
            [0.0, 3.0, 3.0],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A white square, 20 pixels a side, in the middle of a clear 60 × 60
    /// frame.
    fn square(x: c_int, y: c_int) -> [f32; 4] {
        if (20..40).contains(&x) && (20..40).contains(&y) {
            [1.0; 4]
        } else {
            [0.0; 4]
        }
    }

    /// The effect over the square, for a 60 × 60 frame.
    fn run(s: &Settings) -> Plan {
        Plan::new(s, &square, [0, 0, 60, 60], 1.0, [0.0, 0.0], [60.0, 60.0])
    }

    fn at(m: &Plan, x: usize, y: usize) -> [f32; 4] {
        m.pixel(x as c_int, y as c_int)
    }

    #[test]
    fn at_its_defaults_it_changes_nothing() {
        let s = Settings::default();
        assert!(s.changes_nothing());
        let m = run(&s);
        assert_eq!(at(&m, 30, 30), [1.0; 4]);
        assert_eq!(at(&m, 5, 5), [0.0; 4]);
    }

    #[test]
    fn the_shadow_moves_as_vegass() {
        // Drop Shadow With Dark Inner Glow, on a 1280 × 720 frame: 18
        // pixels right of the picture (measured on VEGAS's picture: 18).
        let s = Settings {
            shadow: DROP,
            height: 0.021,
            light: [-3.0, -0.034, 4.382],
            ..Settings::default()
        };
        let [dx, dy] = s.offset([1280.0, 720.0]);
        assert!((dx - 18.4).abs() < 0.1 && dy.abs() < 0.2, "{dx} {dy}");
        // Up Light: 18 pixels up (VEGAS's: 18.7).
        let up = Settings {
            height: 0.042,
            light: [0.068, -3.0, 4.921],
            ..s
        };
        let [_, dy] = up.offset([1280.0, 720.0]);
        assert!((dy - 18.4).abs() < 0.1, "{dy}");
        // Inside, a tenth as far; with the light down on the frame,
        // nowhere.
        let inner = Settings { shadow: INNER, ..s };
        assert!((inner.offset([1280.0, 720.0])[0] - 1.84).abs() < 0.01);
        let low = Settings {
            light: [2.591, 3.0, 0.0],
            ..s
        };
        assert_eq!(low.offset([1280.0, 720.0]), [0.0, 0.0]);
        // Its blur: 0.023 of the width per unit (VEGAS's, measured).
        assert!((s.blurs(1280.0).shadow - 0.023 * 0.2 * 1280.0).abs() < 1e-9);
    }

    #[test]
    fn a_dropped_shadow_falls_away_from_the_light() {
        // The light up and left: the shadow down and right.
        let s = Settings {
            shadow: DROP,
            height: 0.5,
            blur: 0.0,
            ..Settings::default()
        };
        let [dx, dy] = s.offset([60.0, 60.0]);
        assert!(dx > 0.0 && dy < 0.0);
        let m = run(&s);
        let (x, y) = (39 + dx.round() as usize, (20.0 + dy).round() as usize);
        let shade = at(&m, x, y);
        assert!(shade[3] > 0.9 && shade[0] < 0.01, "{shade:?}");
        // Above and left of the square: nothing.
        assert_eq!(at(&m, 15, 45), [0.0; 4]);
        // The picture stays over it.
        assert_eq!(at(&m, 30, 30), [1.0; 4]);
    }

    #[test]
    fn a_far_shadow_reads_where_it_comes_from_or_nothing() {
        // The light low: the shadow far beyond the frame, nothing read
        // for it, nothing drawn.
        let s = Settings {
            shadow: DROP,
            height: 1.0,
            blur: 0.0,
            light: [-3.0, 3.0, 0.01],
            ..Settings::default()
        };
        assert!(shadow_reads(&s, [0.0, 0.0], [60.0, 60.0], [0.0, 0.0, 60.0, 60.0]).is_none());
        let m = run(&s);
        assert_eq!(at(&m, 45, 10), [0.0; 4]);
        assert_eq!(at(&m, 30, 30), [1.0; 4]);
        // Inside, the whole hole is in shadow.
        let inner = Settings {
            shadow: INNER,
            height: 1.0,
            light: [-3.0, 3.0, 0.001],
            ..s
        };
        let m = run(&inner);
        assert!(at(&m, 30, 30)[0] < 0.01 && at(&m, 30, 30)[3] == 1.0);
    }

    #[test]
    fn an_inner_shadow_darkens_the_edge_toward_the_light() {
        // The light left and up: the left and upper edges, inside, dark;
        // the right one not; nothing outside.
        let s = Settings {
            shadow: INNER,
            height: 1.0,
            blur: 0.0,
            light: [-3.0, 3.0, 3.0],
            ..Settings::default()
        };
        let m = run(&s);
        let left = at(&m, 20, 30);
        let right = at(&m, 39, 30);
        assert!(left[0] < 0.5 && left[3] == 1.0, "{left:?}");
        assert!(right[0] > 0.95, "{right:?}");
        assert!(at(&m, 29, 39)[0] < 0.5, "the upper edge");
        assert_eq!(at(&m, 10, 30), [0.0; 4]);
        // Its middle: lit.
        assert!(at(&m, 34, 25)[0] > 0.95);
    }

    #[test]
    fn the_glows_go_around_inside_along_or_both() {
        let glow = |glow| {
            run(&Settings {
                glow,
                amount: 0.5,
                glow_color: [0.0, 1.0, 0.0],
                ..Settings::default()
            })
        };
        // Around: green just outside, the picture as it was.
        let outer = glow(OUTER_GLOW);
        let g = at(&outer, 40, 30);
        assert!(g[1] > 0.3 && g[0] < 0.01, "{g:?}");
        assert_eq!(at(&outer, 30, 30), [1.0; 4]);
        // Hugging the edge: the same (VEGAS's look alike).
        let edge = glow(EDGE_GLOW);
        assert_eq!(at(&edge, 40, 30), g);
        // Inside: the edge green over the picture, nothing outside.
        let inner = glow(INNER_GLOW);
        let i = at(&inner, 20, 30);
        assert!(i[0] < 0.8 && i[1] == 1.0 && i[3] == 1.0, "{i:?}");
        assert_eq!(at(&inner, 41, 30), [0.0; 4]);
        // Both.
        let dual = glow(DUAL_GLOW);
        assert!(at(&dual, 20, 30)[0] < 0.8 && at(&dual, 40, 30)[1] > 0.3);
    }

    #[test]
    fn a_glow_of_its_color_around_and_the_picture_faded() {
        let s = Settings {
            glow: OUTER_GLOW,
            amount: 0.5,
            opacity: 0.5,
            ..Settings::default()
        };
        let m = run(&s);
        // Just outside the square: red.
        let g = at(&m, 40, 30);
        assert!(g[0] > 0.3 && g[1] < 0.01 && g[2] < 0.01, "{g:?}");
        // The picture half there, the glow under it.
        let p = at(&m, 30, 30);
        assert!(p[3] > 0.5 && p[1] >= 0.5 - 1e-6, "{p:?}");
    }

    #[test]
    fn the_embossed_edge_toward_the_light_is_brighter() {
        let s = Settings {
            foreground: 1.0,
            ..Settings::default()
        };
        let m = run(&s);
        // The light is up and left: the square's left edge (inside)
        // catches it, its right one does not.
        let lit = at(&m, 20, 30);
        let dark = at(&m, 39, 30);
        assert!(lit[0] > dark[0] + 0.05, "{lit:?} {dark:?}");
        // Its middle is flat: as it was.
        assert!((at(&m, 30, 30)[1] - 1.0).abs() < 0.02);
        // Nothing around it.
        assert_eq!(at(&m, 18, 30), [0.0; 4]);
    }

    #[test]
    fn the_ground_embossed_shades_around_the_picture() {
        // The ground pressed in around it, lit from the left: its slope up
        // to the square's left edge faces the light, light; the one right
        // of it, dark.
        let s = Settings {
            background: -1.0,
            intensity: 1.0,
            light: [-3.0, 0.0, 1.0],
            ..Settings::default()
        };
        let m = run(&s);
        let left = at(&m, 19, 30);
        let right = at(&m, 40, 30);
        assert!(left[3] > 0.0 && left[0] > 0.5 * left[3], "{left:?}");
        assert!(right[3] > 0.0 && right[0] < 0.01, "{right:?}");
        // A dropped shadow leaves the ground flat (VEGAS greys it out).
        let dropped = Settings { shadow: DROP, ..s };
        assert_eq!(dropped.relief(), [0.0, 0.0]);
    }

    #[test]
    fn the_lights_slider_has_3_in_its_middle() {
        // VEGAS's: from 0 to 6.
        let l = LightLayout::of([300.0, 104.0]);
        let [_, y0, _, y1] = l.slider;
        assert!((l.height_at((y0 + y1) / 2.0) - 3.0).abs() < 1e-9);
        assert_eq!(l.height_at(y1), 6.0);
    }

    #[test]
    fn the_lights_box_gives_back_its_place() {
        let l = LightLayout::of([300.0, 104.0]);
        let p = l.point_of([-1.0, 1.0, 3.0]);
        let back = l.place_at(p);
        assert!((back[0] + 1.0).abs() < 1e-9 && (back[1] - 1.0).abs() < 1e-9);
        // VEGAS's box: -1, 1 a third of the way from its upper left.
        let [x0, _, x1, y1] = l.area;
        assert!(((p[0] - x0) / (x1 - x0) - 1.0 / 3.0).abs() < 1e-9);
        assert!(((y1 - p[1]) / (x1 - x0) - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn the_lights_cross_is_on_the_frame() {
        let (o, f) = ([0.0, 0.0], [1280.0, 720.0]);
        // ±3 are the frame's edges; beyond them, kept on it.
        assert_eq!(light_point([-3.0, 0.0, 3.0], o, f), [0.0, 360.0]);
        assert_eq!(light_point([1.5, 3.0, 3.0], o, f), [960.0, 720.0]);
        assert_eq!(light_point([9.0, -9.0, 3.0], o, f), [1280.0, 0.0]);
        let back = light_place([960.0, 720.0], o, f);
        assert!((back[0] - 1.5).abs() < 1e-9 && (back[1] - 3.0).abs() < 1e-9);
    }

    #[test]
    fn the_presets_are_vegass() {
        let p = presets();
        assert_eq!(p.len(), 18);
        assert_eq!(p[17].name, "Outer Glow with Embossing");
        assert_eq!(p[0].name, "Drop Shadow With Dark Inner Glow");
        // Only what is not at its default.
        let up = &p[5];
        assert_eq!(up.name, "Drop Shadow With Up Light");
        assert!(
            up.values
                .contains(&("lightLocation", PresetValue::Num(vec![0.068, -3.0, 4.921])))
        );
        assert!(!up.values.iter().any(|(n, _)| *n == "foregroundOpacity"));
        // The colors as VEGAS's picker shows them.
        let [r, g, b] = hsv(131.0, 1.0, 1.0);
        assert!(r == 0.0 && g == 1.0 && (b - 11.0 / 60.0).abs() < 1e-12);
        let [r, g, b] = hsv(223.0, 0.48, 0.91);
        assert!((r - 0.4732).abs() < 1e-4 && (g - 0.59696).abs() < 1e-4 && b == 0.91);
    }
}
