//! LUT Filter: VEGAS's. The picture's colors through a look-up table read
//! from a `.cube` file (a look, a camera's log made viewable), mixed with
//! the picture as Strength says. Interpolation picks how colors between
//! the table's points are worked out: Linear (good), trilinear, or
//! Tetrahedral (best). Picking a file names the table after its title (or
//! the file's name).
//!
//! The colors are the ones the host hands over (in BananaV's 8-bit
//! projects, as VEGAS's, the encoded ones), unpremultiplied; a file that
//! cannot be read changes nothing.
//!
//! VEGAS's has no presets; BananaV's are looks of its own ([`looks`]),
//! tables it makes and writes to a folder of the user's the first time
//! they are needed (their File starts with [`LOOKS`]).
#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use bv_lut::{Interpolation, Lut};
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.LutFilter";

const NAME: &CStr = c"name";
const FILE: &CStr = c"file";
const INTERPOLATION: &CStr = c"interpolation";
const STRENGTH: &CStr = c"strength";

/// The ways colors between points are worked out, in VEGAS's order.
const INTERPOLATIONS: [&CStr; 2] = [c"Linear (good)", c"Tetrahedral (best)"];

/// The table in the file at `path`, read once and again when the file
/// changes; `None` when there is none or it is not a table.
pub(crate) fn table(path: &str) -> Option<Arc<Lut>> {
    type Tables = HashMap<PathBuf, (Option<SystemTime>, Option<Arc<Lut>>)>;
    static TABLES: OnceLock<Mutex<Tables>> = OnceLock::new();
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let path = PathBuf::from(path);
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let mut tables = TABLES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some((when, lut)) = tables.get(&path)
        && *when == modified
    {
        return lut.clone();
    }
    let lut = match Lut::read(&path) {
        Ok(lut) => Some(Arc::new(lut)),
        Err(e) => {
            if crate::ofx::tracing() {
                eprintln!("bananav lut filter: {}: {e}", path.display());
            }
            None
        }
    };
    tables.insert(path, (modified, lut.clone()));
    lut
}

/// The settings at a time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    pub file: String,
    pub tetrahedral: bool,
    pub strength: f64,
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        Settings {
            file: v.text(FILE).unwrap_or_default().to_string(),
            tetrahedral: v.int(INTERPOLATION, 0) == 1,
            strength: v.f64(STRENGTH, 1.0).clamp(0.0, 1.0),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            file: fx.param(FILE).text_at(time),
            tetrahedral: fx.param(INTERPOLATION).int_at(time) == 1,
            strength: fx.param(STRENGTH).f64_at(time).clamp(0.0, 1.0),
        }
    }

    pub(crate) fn interpolation(&self) -> Interpolation {
        if self.tetrahedral {
            Interpolation::Tetrahedral
        } else {
            Interpolation::Linear
        }
    }
}

/// The name a table's file gives it: its title, or the file's name.
fn name_of(path: &str, lut: Option<&Lut>) -> String {
    match lut {
        Some(l) if !l.title.trim().is_empty() => l.title.trim().to_string(),
        _ => Path::new(path.trim())
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
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
        let s = Settings::at(fx, args.dbl(kOfxPropTime, 0));
        if s.strength <= 0.0 || table(&s.file).is_none() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxActionInstanceChanged) {
        // A file picked by the user names the table.
        if args.is(kOfxPropType, kOfxTypeParameter)
            && args.is(kOfxPropName, FILE)
            && args.is(kOfxPropChangeReason, kOfxChangeUserEdited)
        {
            let file = fx.param(FILE).text_at(args.dbl(kOfxPropTime, 0));
            let name = name_of(&file, table(&file).as_deref());
            if !name.is_empty() {
                fx.param(NAME).set_text(&name);
            }
        }
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // Each output pixel needs the source's pixel there only.
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &roi);
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

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"LUT Filter");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"Changes the picture's colors through a look-up table (a .cube file): a look, or a camera's log made viewable.",
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

    let p = fx.define_param(kOfxParamTypeString, NAME, c"Name");
    p.set_str(kOfxParamPropDefault, c"");
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(
        kOfxParamPropHint,
        c"The table's name: its file's title when one is picked.",
    );

    let p = fx.define_param(kOfxParamTypeString, FILE, c"File");
    p.set_str(kOfxParamPropStringMode, kOfxParamStringIsFilePath);
    p.set_int(kOfxParamPropStringFilePathExists, 1);
    p.set_str(kOfxParamPropDefault, c"");
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(kOfxParamPropHint, c"The look-up table's file (.cube).");

    let p = fx.define_param(kOfxParamTypeChoice, INTERPOLATION, c"Interpolation");
    p.set_strs(kOfxParamPropChoiceOption, &INTERPOLATIONS);
    p.set_int(kOfxParamPropDefault, 0);
    p.set_int(kOfxParamPropAnimates, 0);
    p.set_str(
        kOfxParamPropHint,
        c"How colors between the table's points are worked out: tetrahedral is the most exact.",
    );

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
        c"How much of the table's colors shows: 0 none, 1 all.",
    );
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let s = Settings::at(fx, args.dbl(kOfxPropTime, 0));
    let Some(lut) = table(&s.file) else {
        return crate::colors::render(fx, args, &|c| c);
    };
    let how = s.interpolation();
    let k = s.strength as f32;
    crate::colors::render(fx, args, &|c| {
        let o = lut.apply(c, how);
        std::array::from_fn(|i| c[i] + (o[i] - c[i]) * k)
    })
}

// BananaV's looks: the presets' tables.

/// Where a preset's File starts when its table is one of BananaV's looks:
/// the folder they are written to takes its place ([`resolve`]).
pub const LOOKS: &str = "<looks>/";

/// How many points each way BananaV's looks have.
const LOOK_POINTS: usize = 17;

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// `c` with its saturation times `k` (around its luma).
fn saturate(c: [f32; 3], k: f32) -> [f32; 3] {
    let y = luma(c);
    c.map(|v| y + (v - y) * k)
}

/// An S curve: `k` of the way to smoothstep.
fn s_curve(v: f32, k: f32) -> f32 {
    v + (v * v * (3.0 - 2.0 * v) - v) * k
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

fn overlay(a: f32, b: f32) -> f32 {
    if a < 0.5 {
        2.0 * a * b
    } else {
        1.0 - 2.0 * (1.0 - a) * (1.0 - b)
    }
}

/// BananaV's looks: their names (the presets') and what each does to a
/// color (0–1, encoded).
#[allow(clippy::type_complexity)]
const LOOK_LIST: [(&str, fn([f32; 3]) -> [f32; 3]); 8] = [
    ("Warm", |c| [c[0] * 1.06, c[1], c[2] * 0.9]),
    ("Cool", |c| [c[0] * 0.9, c[1] * 0.99, c[2] * 1.07]),
    ("Teal and Orange", |c| {
        // Shadows toward teal, lights toward orange, a little more color.
        let t = {
            let y = luma(c);
            let x = ((y - 0.2) / 0.6).clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        };
        let tint = mix([-0.5, 0.05, 0.1], [0.5, 0.1, -0.25], t);
        saturate(std::array::from_fn(|i| c[i] + tint[i] * 0.18), 1.15)
    }),
    ("Faded Film", |c| {
        // Blacks lifted, whites down, less color, a warm cast.
        let c = saturate(c.map(|v| 0.06 + v * 0.88), 0.85);
        [c[0] + 0.02, c[1] + 0.005, c[2] - 0.02]
    }),
    ("High Contrast", |c| {
        saturate(c.map(|v| s_curve(v, 0.6)), 1.1)
    }),
    ("Black and White", |c| [s_curve(luma(c), 0.3); 3]),
    ("Bleach Bypass", |c| {
        // The picture over its own gray, most of its color gone.
        let y = luma(c);
        let over = c.map(|v| overlay(v, y));
        saturate(mix(c, over, 0.7), 0.55)
    }),
    ("Vintage", |c| {
        // A faded sepia over what is left of the colors.
        let y = 0.05 + luma(c) * 0.9;
        let sepia = [y * 1.07 + 0.03, y * 0.98 + 0.02, y * 0.8 + 0.04];
        mix(saturate(c, 0.5), sepia, 0.6)
    }),
];

/// BananaV's looks: each one's name and file, and the `.cube` text of its
/// table.
pub fn looks() -> Vec<(&'static str, String, String)> {
    let n = LOOK_POINTS;
    let k = (n - 1) as f32;
    LOOK_LIST
        .iter()
        .map(|(name, f)| {
            let mut text =
                format!("# One of BananaV's looks.\nTITLE \"{name}\"\nLUT_3D_SIZE {n}\n");
            for b in 0..n {
                for g in 0..n {
                    for r in 0..n {
                        let c = f([r as f32 / k, g as f32 / k, b as f32 / k])
                            .map(|v| v.clamp(0.0, 1.0));
                        text += &format!("{:.6} {:.6} {:.6}\n", c[0], c[1], c[2]);
                    }
                }
            }
            (*name, format!("{name}.cube"), text)
        })
        .collect()
}

/// Writes BananaV's looks to `dir` (each that is missing or differs).
pub fn write_looks(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (_, file, text) in looks() {
        let path = dir.join(file);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            std::fs::write(&path, text)?;
        }
    }
    Ok(())
}

/// A preset's File with BananaV's looks written to `dir`.
pub fn resolve(file: &str, dir: &Path) -> String {
    match file.strip_prefix(LOOKS) {
        Some(rest) => dir.join(rest).display().to_string(),
        None => file.to_string(),
    }
}

/// The factory presets: BananaV's looks, worked out tetrahedrally.
pub(crate) fn presets() -> Vec<Preset> {
    looks()
        .into_iter()
        .map(|(name, file, _)| Preset {
            name,
            values: vec![
                ("name", PresetValue::Text(name.to_string())),
                ("file", PresetValue::Text(format!("{LOOKS}{file}"))),
                ("interpolation", PresetValue::Num(vec![1.0])),
                ("strength", PresetValue::Num(vec![1.0])),
            ],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_is_named_after_its_title_or_its_file() {
        let titled = Lut::parse("TITLE \"Warm Look\"\nLUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
        let untitled = Lut::parse("LUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
        assert_eq!(name_of("C:/luts/a.cube", Some(&titled)), "Warm Look");
        assert_eq!(
            name_of("C:/luts/Film Print.cube", Some(&untitled)),
            "Film Print"
        );
        assert_eq!(name_of("/luts/missing.cube", None), "missing");
    }

    #[test]
    fn tables_are_read_again_when_their_file_changes() {
        let dir = std::env::temp_dir().join(format!("bananav-lut-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.cube");
        let file = path.display().to_string();
        assert!(table(&file).is_none(), "no file yet");
        std::fs::write(&path, "TITLE \"One\"\nLUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
        assert_eq!(table(&file).unwrap().title, "One");
        // A newer file (its time moved on).
        std::fs::write(&path, "TITLE \"Two\"\nLUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
        let later = std::fs::File::options().write(true).open(&path).unwrap();
        later
            .set_modified(SystemTime::now() + std::time::Duration::from_secs(5))
            .unwrap();
        drop(later);
        assert_eq!(table(&file).unwrap().title, "Two");
        assert!(table("").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bananavs_looks_are_tables_written_once() {
        let looks = looks();
        assert_eq!(looks.len(), 8);
        for (name, _, text) in &looks {
            let lut = Lut::parse(text).unwrap();
            assert_eq!(lut.title, *name);
            // Black and white stay where a look puts them, inside 0–1.
            let w = lut.apply([1.0; 3], Interpolation::Tetrahedral);
            assert!(w.iter().all(|v| (0.0..=1.0).contains(v)), "{name}: {w:?}");
        }
        // Black and White: grays, from the picture's light.
        let bw = Lut::parse(&looks[5].2).unwrap();
        let c = bw.apply([0.8, 0.3, 0.1], Interpolation::Tetrahedral);
        assert!(
            (c[0] - c[1]).abs() < 1e-4 && (c[1] - c[2]).abs() < 1e-4,
            "{c:?}"
        );
        // Warm: more red than blue on a gray.
        let warm = Lut::parse(&looks[0].2).unwrap();
        let c = warm.apply([0.5; 3], Interpolation::Tetrahedral);
        assert!(c[0] > 0.52 && c[2] < 0.47, "{c:?}");
        // Written to a folder, the presets' files point there.
        let dir = std::env::temp_dir().join(format!("bananav-looks-{}", std::process::id()));
        write_looks(&dir).unwrap();
        let p = presets();
        let file = match &p[2].values[1].1 {
            PresetValue::Text(t) => resolve(t, &dir),
            other => panic!("{other:?}"),
        };
        assert!(file.ends_with("Teal and Orange.cube"), "{file}");
        assert_eq!(table(&file).unwrap().title, "Teal and Orange");
        assert_eq!(resolve("C:/mine.cube", &dir), "C:/mine.cube");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
