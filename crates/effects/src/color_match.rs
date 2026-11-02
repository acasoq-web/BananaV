//! Color Match, as VEGAS's: the colors of the picture it works on brought
//! to those of a reference — the source image — as a sample of that
//! picture — the target image — shows they differ. Each image is kept as
//! a summary: its colors' means and spreads in CIE L*a*b* (VEGAS's
//! measure: every pixel's 8-bit values), and a mosaic of it drawn under
//! its row. Each pixel's a and b — L too with Match brightness — move from
//! the target's means by the ratio of the spreads to the source's
//! (Reinhard's transfer, as VEGAS works it out), Strength of the way;
//! VEGAS's presets are source images' means and spreads. The images come
//! from the host when one of their buttons is pushed — BananaV's
//! `kBananaVParamPropPicture`: the frame the Trimmer shows, a file, the
//! Video Preview's frame (the target's: the picture at the cursor before
//! this effect) — on the optional clips `SourceImage` and `TargetImage`,
//! which other hosts may connect instead (then read at each frame).
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::extensions::{
    kBananaVParamPropInteract, kBananaVParamPropPicture, kBananaVParamPropPictureClip,
};
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::kit::Buf;
use crate::ofx::{Draw, Fx, Props, interact_props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.ColorMatch";

const MATCH: &CStr = c"matchBrightness";
const STRENGTH: &CStr = c"strength";
/// The Result Image's row (its mosaic under it).
const RESULT: &CStr = c"result";

/// The size of a mosaic's interface under its row (pixels).
const MOSAIC_SIZE: [c_int; 2] = [180, 64];

/// One of the two images: the source (the reference the colors go to) or
/// the target (a sample of the picture worked on).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    Source,
    Target,
}

/// A button that takes an image: its name, label, the picture it asks
/// the host for and its hint.
type Button = (&'static CStr, &'static CStr, &'static CStr, &'static CStr);

impl Which {
    const BOTH: [Which; 2] = [Which::Source, Which::Target];

    /// The clip it arrives on.
    fn clip(self) -> &'static CStr {
        match self {
            Which::Source => c"SourceImage",
            Which::Target => c"TargetImage",
        }
    }

    fn label(self) -> &'static CStr {
        match self {
            Which::Source => c"Source Image",
            Which::Target => c"Target Image",
        }
    }

    /// Its row: its size, its mosaic under it.
    fn shown(self) -> &'static CStr {
        match self {
            Which::Source => c"source",
            Which::Target => c"target",
        }
    }

    /// Its summary, kept hidden.
    fn look(self) -> &'static CStr {
        match self {
            Which::Source => c"sourceLook",
            Which::Target => c"targetLook",
        }
    }

    /// Its buttons, in VEGAS's order.
    fn buttons(self) -> [Button; 5] {
        match self {
            Which::Source => [
                (
                    c"sourceTrimmer",
                    c"Trimmer",
                    c"Trimmer",
                    c"Takes the source image from the frame the Trimmer shows.",
                ),
                (
                    c"sourceClipboard",
                    c"Clipboard",
                    c"Clipboard",
                    c"Takes the source image from a picture on the clipboard.",
                ),
                (
                    c"sourceFile",
                    c"File",
                    c"File",
                    c"Takes the source image from a picture file.",
                ),
                (
                    c"sourceScreen",
                    c"Screen",
                    c"Screen",
                    c"Takes the source image from a part of the screen.",
                ),
                (
                    c"sourcePreview",
                    c"Preview",
                    c"Preview",
                    c"Takes the source image from the frame the Video Preview shows.",
                ),
            ],
            Which::Target => [
                (
                    c"targetTrimmer",
                    c"Trimmer",
                    c"Trimmer",
                    c"Takes the target image from the frame the Trimmer shows.",
                ),
                (
                    c"targetClipboard",
                    c"Clipboard",
                    c"Clipboard",
                    c"Takes the target image from a picture on the clipboard.",
                ),
                (
                    c"targetFile",
                    c"File",
                    c"File",
                    c"Takes the target image from a picture file.",
                ),
                (
                    c"targetScreen",
                    c"Screen",
                    c"Screen",
                    c"Takes the target image from a part of the screen.",
                ),
                (
                    c"targetPreview",
                    c"Preview",
                    c"Input",
                    c"Takes the target image from the picture at the cursor, as it is before this effect.",
                ),
            ],
        }
    }

    /// The image whose button is `name`.
    fn of_button(name: &dyn Fn(&CStr) -> bool) -> Option<Which> {
        Which::BOTH
            .into_iter()
            .find(|w| w.buttons().iter().any(|b| name(b.0)))
    }
}

// L*a*b*: LAB Adjust's (D65, the values through sRGB's curve).

use crate::looks::{from_lab, lab};

/// A color's 8-bit value, as VEGAS reads the images it measures.
fn byte(v: f32) -> f32 {
    (v.clamp(0.0, 1.0) * 255.0).round() / 255.0
}

/// An image's summary: its size, its colors' means and spreads (standard
/// deviations) in L*a*b* — L, a, b —, and a mosaic of it — `cols` ×
/// `rows` straight colors, the top row first. A preset's has no size and
/// no mosaic.
#[derive(Clone, Debug, PartialEq)]
struct Look {
    size: [u32; 2],
    mean: [f64; 3],
    spread: [f64; 3],
    cols: usize,
    rows: usize,
    mosaic: Vec<[f32; 3]>,
}

impl Look {
    /// How many rows a mosaic has.
    const ROWS: usize = 9;

    /// `b`'s summary (none when it is empty), as VEGAS measures it: every
    /// pixel's 8-bit values as they are in L*a*b*, their running means and
    /// their spreads over one less than how many there are.
    fn of(b: &Buf) -> Option<Look> {
        let (w, h) = (b.width(), b.height());
        if w <= 0 || h <= 0 {
            return None;
        }
        let rows = Look::ROWS;
        let cols = ((rows as f64 * f64::from(w) / f64::from(h)).round() as usize).clamp(3, 24);
        let (mut mean, mut squares, mut n) = ([0.0f64; 3], [0.0f64; 3], 0.0f64);
        let mut cells = vec![[0.0f64; 4]; cols * rows];
        for y in b.bounds[1]..b.bounds[3] {
            // The picture's rows go up; the mosaic's top row first.
            let up = (y - b.bounds[1]) as usize * rows / h as usize;
            let row = rows - 1 - up.min(rows - 1);
            for x in b.bounds[0]..b.bounds[2] {
                let c = b.at(x, y);
                let col = ((x - b.bounds[0]) as usize * cols / w as usize).min(cols - 1);
                let cell = &mut cells[row * cols + col];
                for k in 0..4 {
                    cell[k] += f64::from(c[k]);
                }
                let v = lab([byte(c[0]), byte(c[1]), byte(c[2])]);
                n += 1.0;
                for k in 0..3 {
                    let before = mean[k];
                    mean[k] += (v[k] - before) / n;
                    squares[k] += (v[k] - mean[k]) * (v[k] - before);
                }
            }
        }
        let spread = if n > 1.0 {
            squares.map(|s| (s / (n - 1.0)).max(0.0).sqrt())
        } else {
            [0.0; 3]
        };
        let mosaic = cells
            .iter()
            .map(|c| {
                if c[3] > 0.0 {
                    [c[0] / c[3], c[1] / c[3], c[2] / c[3]].map(|v| v as f32)
                } else {
                    [0.0; 3]
                }
            })
            .collect();
        Some(Look {
            size: [w as u32, h as u32],
            mean,
            spread,
            cols,
            rows,
            mosaic,
        })
    }

    /// A preset's: only the means and spreads.
    fn measured(mean: [f64; 3], spread: [f64; 3]) -> Look {
        Look {
            size: [0, 0],
            mean,
            spread,
            cols: 0,
            rows: 0,
            mosaic: Vec::new(),
        }
    }

    /// As text: the size, the means and spreads, the mosaic's columns,
    /// rows and colors.
    fn text(&self) -> String {
        let num = |v: f64| {
            let s = format!("{v:.6}");
            let s = s.trim_end_matches('0').trim_end_matches('.');
            if s.is_empty() || s == "-0" {
                "0".to_string()
            } else {
                s.to_string()
            }
        };
        let mut parts = vec![self.size[0].to_string(), self.size[1].to_string()];
        parts.extend(self.mean.iter().chain(&self.spread).map(|&v| num(v)));
        parts.push(self.cols.to_string());
        parts.push(self.rows.to_string());
        parts.extend(self.mosaic.iter().flatten().map(|&v| num(f64::from(v))));
        parts.join(" ")
    }

    fn parse(text: &str) -> Option<Look> {
        let t: Vec<&str> = text.split_ascii_whitespace().collect();
        let num = |i: usize| t.get(i)?.parse::<f64>().ok().filter(|v| v.is_finite());
        let size = [num(0)? as u32, num(1)? as u32];
        let mean = [num(2)?, num(3)?, num(4)?];
        let spread = [num(5)?, num(6)?, num(7)?];
        let (cols, rows) = (num(8)? as usize, num(9)? as usize);
        if cols > 64 || rows > 64 || (cols == 0) != (rows == 0) || t.len() != 10 + 3 * cols * rows {
            return None;
        }
        let mosaic = (0..cols * rows)
            .map(|i| {
                let at = 10 + 3 * i;
                Some([num(at)? as f32, num(at + 1)? as f32, num(at + 2)? as f32])
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Look {
            size,
            mean,
            spread,
            cols,
            rows,
            mosaic,
        })
    }

    /// Whether its a or b do not vary at all: VEGAS's "solid color".
    fn solid(&self) -> bool {
        self.spread[1] < 1e-6 || self.spread[2] < 1e-6
    }

    /// Whether its a or b vary less than a unit: too little to match well.
    fn poor(&self) -> bool {
        self.spread[1] < 1.0 || self.spread[2] < 1.0
    }
}

/// VEGAS's transfer from the target image (`from`, a sample of the
/// picture) to the source image (`to`): each of a and b — L too with
/// Match brightness — moved from the target's mean by the ratio of their
/// spreads to the source's mean, in single precision; `keep`, one less
/// Strength, of the picture stays.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Transfer {
    from: [f32; 3],
    ratio: [f32; 3],
    to: [f32; 3],
    brightness: bool,
    keep: f32,
}

impl Transfer {
    /// The transfer of `s` (none without both images, when one of them is
    /// a solid color, or at no strength: the picture stays).
    fn of(s: &Settings) -> Option<Transfer> {
        let (from, to) = (s.target.as_ref()?, s.source.as_ref()?);
        if from.solid() || to.solid() || s.strength <= 0.0 {
            return None;
        }
        Some(Transfer {
            from: from.mean.map(|v| v as f32),
            ratio: std::array::from_fn(|k| {
                let (a, b) = (to.spread[k] as f32, from.spread[k] as f32);
                if b > 0.0 { a / b } else { 1.0 }
            }),
            to: to.mean.map(|v| v as f32),
            brightness: s.brightness,
            keep: (1.0 - f64::from(s.strength)) as f32,
        })
    }

    /// The straight color `c` matched.
    fn matched(&self, c: [f32; 3]) -> [f32; 3] {
        let v = lab(c);
        let m = |k: usize| {
            f64::from(self.ratio[k]) * (v[k] - f64::from(self.from[k])) + f64::from(self.to[k])
        };
        from_lab([if self.brightness { m(0) } else { v[0] }, m(1), m(2)])
    }

    /// A premultiplied pixel, as VEGAS's float pictures: transparent ones
    /// go to nothing, opaque ones stay opaque, the others matched straight
    /// and premultiplied again; then `keep` of the pixel mixed back.
    fn pixel(&self, c: [f32; 4]) -> [f32; 4] {
        let a = c[3];
        if a <= 1e-6 {
            return [0.0; 4];
        }
        let (n, alpha) = if a >= 1.0 {
            (self.matched([c[0], c[1], c[2]]), 1.0)
        } else {
            (
                self.matched([c[0] / a, c[1] / a, c[2] / a]).map(|v| v * a),
                a,
            )
        };
        [
            (c[0] - n[0]) * self.keep + n[0],
            (c[1] - n[1]) * self.keep + n[1],
            (c[2] - n[2]) * self.keep + n[2],
            alpha,
        ]
    }

    /// The card's numbers: the target's means, the ratios, the source's
    /// means and what stays.
    fn numbers(&self) -> Vec<f32> {
        let mut f = Vec::with_capacity(10);
        f.extend(self.from);
        f.extend(self.ratio);
        f.extend(self.to);
        f.push(self.keep);
        f
    }
}

/// The settings at one time.
#[derive(Clone, Debug, PartialEq)]
struct Settings {
    brightness: bool,
    strength: f32,
    source: Option<Look>,
    target: Option<Look>,
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            brightness: fx.param(MATCH).int_at(time) != 0,
            strength: fx.param(STRENGTH).f64_at(time).clamp(0.0, 1.0) as f32,
            source: Look::parse(&fx.param(Which::Source.look()).text_at(time)),
            target: Look::parse(&fx.param(Which::Target.look()).text_at(time)),
        }
    }

    /// From a project's values (the card's plan).
    fn of(v: &crate::accel::Values<'_>) -> Settings {
        Settings {
            brightness: v.int(MATCH, 0) != 0,
            strength: v.f64(STRENGTH, 1.0).clamp(0.0, 1.0) as f32,
            source: v.text(Which::Source.look()).and_then(Look::parse),
            target: v.text(Which::Target.look()).and_then(Look::parse),
        }
    }
}

/// `src` with its colors matched.
fn process(src: &Buf, t: &Transfer) -> Buf {
    src.map(|x, y| t.pixel(src.at(x, y)))
}

/// What the card draws for Color Match with `values`: the transfer in
/// `kit_color_match`, or the picture as it is.
pub(crate) fn plan(values: &crate::accel::Values<'_>) -> crate::accel::Plan {
    let s = Settings::of(values);
    match Transfer::of(&s) {
        Some(t) => crate::accel::Plan::Pixel {
            kernel: crate::accel::PixelKernel::ColorMatch,
            f: t.numbers(),
            j: [u32::from(t.brightness), 0, 0, 0],
        },
        None => crate::accel::Plan::Same,
    }
}

/// What VEGAS says of the images once one is taken or a preset picked:
/// a warning when one is a solid color, a note when one varies too little,
/// when they differ much, or when the target image is still missing.
fn advice(s: &Settings) -> Option<(bool, &'static CStr)> {
    let source = s.source.as_ref();
    let target = s.target.as_ref();
    let solid = (
        source.is_some_and(Look::solid),
        target.is_some_and(Look::solid),
    );
    let poor = (
        source.is_some_and(Look::poor),
        target.is_some_and(Look::poor),
    );
    let note = |t| Some((false, t));
    match (solid, poor) {
        ((true, true), _) => Some((
            true,
            c"The source and target images are solid colors. Color matching will not be applied.",
        )),
        ((true, false), _) => Some((
            true,
            c"The source image is a solid color. Color matching will not be applied.",
        )),
        ((false, true), _) => Some((
            true,
            c"The target image is a solid color. Color matching will not be applied.",
        )),
        (_, (true, true)) => note(
            c"The source and target images do not contain sufficient color variation. Color matching may not work as expected.",
        ),
        (_, (true, false)) => note(
            c"The source image does not contain sufficient color variation. Color matching may not work as expected.",
        ),
        (_, (false, true)) => note(
            c"The target image does not contain sufficient color variation. Color matching may not work as expected.",
        ),
        _ => match (source, target) {
            (Some(a), Some(b)) => {
                // Their a or b means apart by more than 2.5 of the larger
                // spread (at least one).
                let apart = |k: usize| {
                    let reach = a.spread[k].max(b.spread[k]).max(1.0) * 2.5;
                    (a.mean[k] - b.mean[k]).abs() > reach
                };
                (apart(1) || apart(2)).then_some((
                    false,
                    c"Significant color differences exist in your source and target images. Color matching may not work as expected.",
                ))
            }
            (Some(a), None) => note(if a.size == [0, 0] {
                c"Please select a target image for comparison with the preset."
            } else {
                c"Please select a target image for comparison with the source image."
            }),
            _ => None,
        },
    }
}

/// Shows (or clears) what VEGAS says of the images at `time`.
fn advise(fx: Fx, time: f64) {
    match advice(&Settings::at(fx, time)) {
        Some((warning, text)) => fx.persistent_message_as(
            if warning {
                kOfxMessageWarning
            } else {
                kOfxMessageMessage
            },
            Some(text),
        ),
        None => fx.persistent_message(None),
    }
}

/// VEGAS's presets: a source image's means and spreads each — L, a, b —,
/// the target image still to take.
pub(crate) fn presets() -> Vec<Preset> {
    let p = |name: &'static str, brightness: bool, mean: [f64; 3], spread: [f64; 3]| Preset {
        name,
        values: vec![
            ("strength", PresetValue::Num(vec![1.0])),
            (
                "matchBrightness",
                PresetValue::Num(vec![f64::from(u8::from(brightness))]),
            ),
            (
                "sourceLook",
                PresetValue::Text(Look::measured(mean, spread).text()),
            ),
            ("targetLook", PresetValue::Text(String::new())),
            ("source", PresetValue::Text("\u{2014}".to_string())),
            ("target", PresetValue::Text("\u{2014}".to_string())),
        ],
    };
    vec![
        p(
            "Pastels",
            false,
            [48.824455, 12.193736, 13.697423],
            [28.831833, 25.368612, 24.063503],
        ),
        p(
            "Inner city",
            false,
            [49.294394, -0.127588, 9.679296],
            [28.104695, 1.038456, 4.80676],
        ),
        p(
            "Noir",
            false,
            [17.00343, 0.001107, -0.002187],
            [27.647539, 0.001493, 0.002949],
        ),
        p(
            "Night Sky",
            true,
            [12.067911, 4.492493, -2.918555],
            [8.730578, 5.220805, 2.929778],
        ),
        p(
            "Morning Sunrise",
            false,
            [46.483857, 6.813671, 20.895265],
            [22.432459, 5.040056, 12.256998],
        ),
        p(
            "South Pacific Sunset",
            false,
            [31.107364, 40.413643, 23.958885],
            [21.357453, 21.858463, 15.773185],
        ),
        p(
            "Teal and Orange",
            false,
            [30.760866, 1.674001, 6.819404],
            [23.533161, 14.459847, 27.119051],
        ),
    ]
}

// The plug-in.

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
        let s = Settings::at(fx, args.dbl(kOfxPropTime, 0));
        let connected = Which::BOTH.iter().any(|w| fx.clip(w.clip()).connected());
        if Transfer::of(&s).is_none() && !connected {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // The images' summaries are of the whole of them.
        let time = args.dbl(kOfxPropTime, 0);
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &roi);
        for w in Which::BOTH {
            let all = fx.clip(w.clip()).rod(time).unwrap_or(roi);
            let name = format!("OfxImageClipPropRoI_{}", w.clip().to_string_lossy());
            if let Ok(name) = std::ffi::CString::new(name) {
                out.set_dbls(&name, &all);
            }
        }
        kOfxStatOK
    } else if is(action, kOfxActionInstanceChanged) {
        let time = args.dbl(kOfxPropTime, 0);
        if args.is(kOfxPropType, kOfxTypeParameter) {
            if let Some(w) = Which::of_button(&|n| args.is(kOfxPropName, n)) {
                take(fx, w, time);
            }
            // A preset or an undo brings other images: what VEGAS says of
            // them.
            if Which::BOTH.iter().any(|w| args.is(kOfxPropName, w.look())) {
                advise(fx, time);
            }
        }
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) {
        advise(fx, 0.0);
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

/// Keeps image `w`'s summary, from the picture on its clip, and says what
/// VEGAS says of the images then.
fn take(fx: Fx, w: Which, time: f64) {
    let Some(img) = fx.clip(w.clip()).image(time) else {
        if crate::ofx::tracing() {
            eprintln!("bananav color match: no picture on {:?}", w.clip());
        }
        return;
    };
    let b = Buf::read(Some(&img));
    drop(img);
    let Some(look) = Look::of(&b) else {
        return;
    };
    fx.param(w.look()).set_text(&look.text());
    fx.param(w.shown())
        .set_text(&format!("{} × {}", look.size[0], look.size[1]));
    advise(fx, time);
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Color Match");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"Brings the picture's colors to those of another picture, the source image, as a sample of it, the target image, shows they differ.",
    );
    // The images arrive on clips of their own: OpenFX's General context.
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextGeneral],
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

/// VEGAS's panel: Strength, Match brightness, then the Source, Target and
/// Result images (their sizes, their mosaics under them, the buttons that
/// take them).
fn describe_in_context(fx: Fx) {
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    for w in Which::BOTH {
        let c = fx.define_clip(w.clip(), &comps);
        c.set_int(kOfxImageClipPropOptional, 1);
        c.set_str(kOfxPropLabel, w.label());
    }
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);

    let p = fx.define_param(kOfxParamTypeDouble, STRENGTH, c"Strength");
    p.set_dbl(kOfxParamPropDefault, 1.0);
    p.set_dbl(kOfxParamPropMin, 0.0);
    p.set_dbl(kOfxParamPropMax, 1.0);
    p.set_dbl(kOfxParamPropDisplayMin, 0.0);
    p.set_dbl(kOfxParamPropDisplayMax, 1.0);
    p.set_dbl(kOfxParamPropIncrement, 0.01);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_str(
        kOfxParamPropHint,
        c"How much of the matched colors shows: 0 the picture as it is, 1 matched only.",
    );
    let p = fx.define_param(kOfxParamTypeBoolean, MATCH, c"Match brightness");
    p.set_int(kOfxParamPropDefault, 0);
    p.set_str(
        kOfxParamPropHint,
        c"Brings the brightness along with the colors; cleared, only the colors are matched.",
    );
    let entries: [OfxPluginEntryPoint; 2] = [source_mosaic, target_mosaic];
    for (w, entry) in Which::BOTH.into_iter().zip(entries) {
        let p = fx.define_param(kOfxParamTypeString, w.shown(), w.label());
        p.set_str(kOfxParamPropStringMode, kOfxParamStringIsLabel);
        p.set_str(kOfxParamPropDefault, c"\u{2014}");
        p.set_int(kOfxParamPropAnimates, 0);
        p.set_str(
            kOfxParamPropHint,
            match w {
                Which::Source => c"The picture whose colors the picture takes: its size, and a mosaic of it under it.",
                Which::Target => c"A sample of the picture, as it is before this effect: its size, and a mosaic of it under it.",
            },
        );
        p.set_ptr(kBananaVParamPropInteract, entry as *mut c_void);
        p.set_ints(kOfxParamPropInteractPreferedSize, &MOSAIC_SIZE);
        let buttons = w.buttons();
        for (i, (name, label, picture, hint)) in buttons.iter().enumerate() {
            let p = fx.define_param(kOfxParamTypePushButton, name, label);
            p.set_str(kOfxParamPropHint, hint);
            p.set_str(kBananaVParamPropPicture, picture);
            p.set_str(kBananaVParamPropPictureClip, w.clip());
            if i + 1 < buttons.len() {
                p.set_int(kOfxParamPropLayoutHint, kOfxParamPropLayoutHintNoNewLine);
            }
        }
    }
    let p = fx.define_param(kOfxParamTypeString, RESULT, c"Result Image");
    p.set_str(kOfxParamPropStringMode, kOfxParamStringIsLabel);
    p.set_str(kOfxParamPropDefault, c"");
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(
        kOfxParamPropHint,
        c"The target image with its colors matched: a mosaic of it under it.",
    );
    p.set_ptr(kBananaVParamPropInteract, result_mosaic as *mut c_void);
    p.set_ints(kOfxParamPropInteractPreferedSize, &MOSAIC_SIZE);
    for w in Which::BOTH {
        let p = fx.define_param(kOfxParamTypeString, w.look(), w.label());
        p.set_str(kOfxParamPropDefault, c"");
        p.set_int(kOfxParamPropAnimates, 0);
        p.set_int(kOfxParamPropSecret, 1);
    }
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let Some(src) = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time) else {
        return kOfxStatFailed;
    };
    if src.depth != out.depth {
        return kOfxStatErrImageFormat;
    }
    let mut s = Settings::at(fx, time);
    // Hosts that connect the images: their summaries now.
    let live = |w: Which| {
        let img = fx.clip(w.clip()).image(time)?;
        Look::of(&Buf::read(Some(&img)))
    };
    if let Some(l) = live(Which::Source) {
        s.source = Some(l);
    }
    if let Some(l) = live(Which::Target) {
        s.target = Some(l);
    }
    let cur = Buf::read(Some(&src));
    drop(src);
    let result = match Transfer::of(&s) {
        Some(t) => process(&cur, &t),
        None => cur,
    };
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

// The mosaics.

unsafe extern "C" fn source_mosaic(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    mosaic(Shown::Image(Which::Source), action, handle, in_args)
}

unsafe extern "C" fn target_mosaic(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    mosaic(Shown::Image(Which::Target), action, handle, in_args)
}

unsafe extern "C" fn result_mosaic(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    mosaic(Shown::Result, action, handle, in_args)
}

/// What a mosaic shows: an image, or the target image matched.
#[derive(Clone, Copy)]
enum Shown {
    Image(Which),
    Result,
}

/// A mosaic, drawn under its row (nothing to do with the pen).
fn mosaic(
    shown: Shown,
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
) -> OfxStatus {
    if is(action, kOfxActionDescribe)
        || is(action, kOfxActionCreateInstance)
        || is(action, kOfxActionDestroyInstance)
    {
        return kOfxStatOK;
    }
    if !is(action, kOfxInteractActionDraw) {
        return kOfxStatReplyDefault;
    }
    let Some(props) = interact_props(handle) else {
        return kOfxStatReplyDefault;
    };
    let args = Props(in_args);
    let fx = Fx(args.ptr(kOfxPropEffectInstance) as OfxImageEffectHandle);
    let Some(d) = Draw::of(args) else {
        return kOfxStatReplyDefault;
    };
    if fx.0.is_null() {
        return kOfxStatReplyDefault;
    }
    let time = args.dbl(kOfxPropTime, 0);
    let size: [c_int; 2] = props.ints(kOfxInteractPropViewportSize);
    let look = match shown {
        Shown::Image(w) => Look::parse(&fx.param(w.look()).text_at(time)),
        Shown::Result => {
            let s = Settings::at(fx, time);
            match (Transfer::of(&s), s.target) {
                (Some(t), Some(mut target)) => {
                    for c in &mut target.mosaic {
                        let m = t.pixel([c[0], c[1], c[2], 1.0]);
                        *c = [m[0], m[1], m[2]];
                    }
                    Some(target)
                }
                _ => None,
            }
        }
    };
    draw_mosaic(&d, look.as_ref(), [f64::from(size[0]), f64::from(size[1])]);
    kOfxStatOK
}

/// A mosaic in a viewport `size` big (pixels, y up from its bottom): as
/// high as it is, as wide as the picture's proportions say; an empty
/// frame without a picture.
fn draw_mosaic(d: &Draw, look: Option<&Look>, size: [f64; 2]) {
    let h = (size[1] - 4.0).max(4.0);
    let aspect = look
        .filter(|l| l.size[1] > 0)
        .map_or(16.0 / 9.0, |l| f64::from(l.size[0]) / f64::from(l.size[1]));
    let w = (h * aspect).min(size[0] - 4.0).max(4.0);
    let (x0, y0) = (2.0, 2.0);
    let rect = |a: [f64; 2], b: [f64; 2]| [[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]]];
    if let Some(l) = look.filter(|l| l.cols > 0 && l.rows > 0) {
        let (cw, ch) = (w / l.cols as f64, h / l.rows as f64);
        for (i, c) in l.mosaic.iter().enumerate() {
            let (col, row) = (i % l.cols, i / l.cols);
            // The top row first; y goes up.
            let top = y0 + h - row as f64 * ch;
            let left = x0 + col as f64 * cw;
            d.colour([
                c[0].clamp(0.0, 1.0),
                c[1].clamp(0.0, 1.0),
                c[2].clamp(0.0, 1.0),
                1.0,
            ]);
            d.fill(&rect([left, top - ch], [left + cw, top]));
        }
    }
    d.colour([0.0, 0.0, 0.0, 0.8]);
    d.width(1.0);
    d.outline(&rect([x0, y0], [x0 + w, y0 + h]));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture of two halves: `left` and `right` (straight colors).
    fn halves(left: [f32; 3], right: [f32; 3]) -> Buf {
        let mut b = Buf::solid([0, 0, 64, 36], [0.0; 4]);
        for y in 0..36 {
            for x in 0..64 {
                let c = if x < 32 { left } else { right };
                b.data[(y * 64 + x) as usize] = [c[0], c[1], c[2], 1.0];
            }
        }
        b
    }

    fn settings(source: &Look, target: &Look, brightness: bool, strength: f32) -> Settings {
        Settings {
            brightness,
            strength,
            source: Some(source.clone()),
            target: Some(target.clone()),
        }
    }

    #[test]
    fn a_look_keeps_as_text() {
        let l = Look::of(&halves([0.2, 0.3, 0.4], [0.8, 0.7, 0.6])).unwrap();
        assert_eq!(l.size, [64, 36]);
        assert_eq!((l.cols, l.rows), (16, 9));
        // The mosaic's left is the left half.
        assert_eq!(l.mosaic[0], [0.2, 0.3, 0.4]);
        assert_eq!(l.mosaic[15], [0.8, 0.7, 0.6]);
        let back = Look::parse(&l.text()).unwrap();
        assert_eq!(back.size, l.size);
        assert_eq!((back.cols, back.rows), (l.cols, l.rows));
        for k in 0..3 {
            assert!((back.mean[k] - l.mean[k]).abs() < 1e-6);
            assert!((back.spread[k] - l.spread[k]).abs() < 1e-6);
        }
        assert_eq!(Look::parse("64 36 0.5"), None);
        assert_eq!(Look::parse(""), None);
        // A preset's: no size, no mosaic.
        let p = Look::measured([50.0, 1.0, -2.0], [20.0, 3.0, 4.0]);
        assert_eq!(Look::parse(&p.text()), Some(p));
    }

    /// VEGAS's measure: L*a*b* of the 8-bit values, spreads over n − 1.
    #[test]
    fn the_spreads_are_vegass() {
        let l = Look::of(&halves([0.0, 0.0, 0.0], [1.0, 1.0, 1.0])).unwrap();
        // Half black (L 0), half white (L 100).
        assert!((l.mean[0] - 50.0).abs() < 1e-3, "{:?}", l.mean);
        let n = 64.0 * 36.0;
        let spread = (50.0f64 * 50.0 * n / (n - 1.0)).sqrt();
        assert!((l.spread[0] - spread).abs() < 1e-3, "{:?}", l.spread);
        assert!(l.spread[1] < 1e-3 && l.spread[2] < 1e-3);
        assert!(l.poor());
        // One color: nothing varies.
        let one = Look::of(&halves([0.3, 0.5, 0.7], [0.3, 0.5, 0.7])).unwrap();
        assert_eq!(one.spread, [0.0; 3]);
        assert!(one.solid());
    }

    #[test]
    fn a_cast_goes_and_the_brightness_follows_when_asked() {
        // The target: warm and dark; the source: cool and brighter.
        let target = Look::of(&halves([0.5, 0.3, 0.2], [0.3, 0.15, 0.1])).unwrap();
        let source = Look::of(&halves([0.5, 0.6, 0.7], [0.3, 0.4, 0.5])).unwrap();
        let warm = [0.4, 0.225, 0.15, 1.0];
        let lab_of = |c: [f32; 4]| lab([c[0], c[1], c[2]]);
        let t = Transfer::of(&settings(&source, &target, true, 1.0)).unwrap();
        let m = t.pixel(warm);
        // Its a and b, and its L, where the source's are.
        let (got, want) = (lab_of(m), lab_of([0.4, 0.5, 0.6, 1.0]));
        for k in 0..3 {
            assert!((got[k] - want[k]).abs() < 1.5, "{got:?} {want:?}");
        }
        // Without Match brightness, L stays.
        let kept = Transfer::of(&settings(&source, &target, false, 1.0))
            .unwrap()
            .pixel(warm);
        assert!((lab_of(kept)[0] - lab_of(warm)[0]).abs() < 1e-3);
        // Half the strength: half way, on the colors.
        let half = Transfer::of(&settings(&source, &target, true, 0.5))
            .unwrap()
            .pixel(warm);
        for k in 0..3 {
            assert!((half[k] - (warm[k] + m[k]) / 2.0).abs() < 1e-5, "{half:?}");
        }
        // Transparent pixels go; translucent ones keep their alpha.
        assert_eq!(t.pixel([0.1, 0.1, 0.1, 0.0]), [0.0; 4]);
        assert_eq!(t.pixel([0.2, 0.1, 0.05, 0.5])[3], 0.5);
    }

    /// What VEGAS says: a solid color first, then too little variation,
    /// then images far apart, then the target missing.
    #[test]
    fn the_messages_are_vegass() {
        let plain = Look::of(&halves([0.5, 0.5, 0.5], [0.5, 0.5, 0.5])).unwrap();
        let colors = Look::of(&halves([0.9, 0.2, 0.1], [0.1, 0.3, 0.9])).unwrap();
        let s = |source: Option<&Look>, target: Option<&Look>| Settings {
            brightness: false,
            strength: 1.0,
            source: source.cloned(),
            target: target.cloned(),
        };
        let said = |s: Settings| advice(&s).map(|(w, t)| (w, t.to_str().unwrap().to_string()));
        assert_eq!(
            said(s(Some(&plain), Some(&colors))),
            Some((
                true,
                "The source image is a solid color. Color matching will not be applied.".into()
            ))
        );
        assert_eq!(said(s(Some(&colors), Some(&colors))), None);
        assert_eq!(
            said(s(Some(&colors), None)).unwrap().1,
            "Please select a target image for comparison with the source image."
        );
        let preset = presets().remove(0);
        let PresetValue::Text(text) = &preset.values[2].1 else {
            panic!("the source's look");
        };
        let look = Look::parse(text).unwrap();
        assert_eq!(
            said(s(Some(&look), None)).unwrap().1,
            "Please select a target image for comparison with the preset."
        );
        // Noir's a and b hardly vary.
        let noir = presets().into_iter().find(|p| p.name == "Noir").unwrap();
        let PresetValue::Text(text) = &noir.values[2].1 else {
            panic!("the source's look");
        };
        let look = Look::parse(text).unwrap();
        assert!(
            said(s(Some(&look), None))
                .unwrap()
                .1
                .starts_with("The source image does not")
        );
    }

    #[test]
    fn the_presets_are_vegass() {
        let p = presets();
        assert_eq!(p.len(), 7);
        assert_eq!(p[3].name, "Night Sky");
        assert_eq!(p[3].values[1].1, PresetValue::Num(vec![1.0]));
    }
}
