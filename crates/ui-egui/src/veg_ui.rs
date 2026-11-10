//! File › Import and File › Export › VEG Project (`.veg`), and File › Open
//! of its files: another editor's project read as a new, unsaved BananaV
//! project — its media found where it says (or beside it), its video
//! effects BananaV's own of the same names (or the same plug-ins
//! installed) — and a project written as one ([`bv_timeline::veg`]).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bv_core::{FrameRate, TICKS_PER_SECOND, Tick};
use bv_engine::Engine;
use bv_timeline::veg::{self, DocEffect, DocKey, ParamValue};
use bv_timeline::{
    Effect, KeyCurve, KeyHandles, MediaInfo, PluginFx, PluginKey, PluginParam, PluginValue,
};

use crate::app::BananaApp;
use crate::i18n::tr;

/// The id a `.veg` gives BananaV's own effects' twins.
pub(crate) const VEG_PLUGINS: &str = "com.vegascreativesoftware:";

/// A name without its spaces, signs and case: "Brightness and Contrast"
/// and `brightnessandcontrast` are one.
pub(crate) fn plain(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// How many numbers a parameter of type `ty` holds (`None`: not numbers).
fn dimensions(ty: bv_ofx::ParamType) -> Option<usize> {
    use bv_ofx::ParamType as T;
    Some(match ty {
        T::Integer | T::Double | T::Boolean | T::Choice => 1,
        T::Double2D | T::Integer2D => 2,
        T::Rgb | T::Double3D | T::Integer3D => 3,
        T::Rgba => 4,
        _ => return None,
    })
}

/// The transitions the file keeps in its own kind, by their classes: known
/// by their presets' names.
const NATIVE_TRANSITIONS: [(&str, &str); 7] = [
    (
        "5856937C-3603-4378-83CF-889423820F39",
        "org.bananav.Portals",
    ),
    (
        "BF8EF6DA-4ED5-4F9C-B549-919490F88B2F",
        "org.bananav.VenetianBlinds",
    ),
    (
        "F4D82040-D40F-44AD-BB24-6AAB29DC21F1",
        "org.bananav.Cascade3D",
    ),
    (
        "573FA5A1-F314-4539-9BB1-77FA92D4E7CD",
        "org.bananav.FlyInOut3D",
    ),
    (
        "E38B2B4A-BA99-4F67-B533-1997297850DD",
        "org.bananav.Shuffle3D",
    ),
    (
        "C843A552-AAEB-411C-8481-CF4C52A336E3",
        "org.bananav.Blinds3D",
    ),
    (
        "E97A8331-D099-440D-AAC5-9B7CC856AE9F",
        "org.bananav.GradientWipe",
    ),
];

/// A time in frames at `rate`.
fn frames_time(frames: f64, rate: FrameRate) -> Tick {
    Tick((frames * TICKS_PER_SECOND as f64 / rate.as_f64()).round() as i64)
}

/// A parameter's numbers as it holds them: a color with its alpha, as
/// many as it has.
fn fitted(mut v: Vec<f64>, ty: bv_ofx::ParamType) -> Vec<f64> {
    if v.len() == 3 && ty == bv_ofx::ParamType::Rgba {
        v.push(1.0);
    }
    if let Some(n) = dimensions(ty) {
        v.truncate(n);
    }
    v
}

/// A parameter's keyframes from the file's, their frames from the effect's
/// start.
fn plugin_keys(keys: &[DocKey], ty: bv_ofx::ParamType, rate: FrameRate) -> Vec<PluginKey> {
    keys.iter()
        .map(|k| {
            let curve = veg::interpolation_curve(k.interpolation);
            let mut key = PluginKey::new(
                frames_time(k.frame, rate),
                PluginValue::Num(fitted(k.value.clone(), ty)),
                curve,
            );
            if matches!(curve, KeyCurve::Bezier | KeyCurve::BezierSplit) {
                key.handles = k
                    .handles
                    .iter()
                    .zip(&k.value)
                    .map(|(h, v)| KeyHandles {
                        before: (frames_time(h[0] - k.frame, rate), h[1] - v),
                        after: (frames_time(h[2] - k.frame, rate), h[3] - v),
                    })
                    .collect();
            }
            key
        })
        .collect()
}

/// What a record of the file's own kind says of plug-in `id`'s parameters:
/// its 4-byte whole numbers, its numbers with a point and its colors, one
/// after the other, each plug-in with its own (the Gradient Wipe's holds its
/// picture's place among the other program's, which BananaV has not).
fn native_settings(id: &str, d: &[u8]) -> Vec<(&'static str, Vec<f64>)> {
    let bytes = |at: usize, n: usize| d.get(at..at + n);
    let whole = |at: usize| {
        bytes(at, 4)
            .and_then(|b| b.try_into().ok())
            .map(|b| f64::from(i32::from_le_bytes(b)))
    };
    let long = |at: usize| {
        bytes(at, 8)
            .and_then(|b| b.try_into().ok())
            .map(f64::from_le_bytes)
    };
    let fields: Vec<(&'static str, Option<Vec<f64>>)> = {
        let w = |name: &'static str, at: usize| (name, whole(at).map(|v| vec![v]));
        let f = |name: &'static str, at: usize| (name, long(at).map(|v| vec![v]));
        match id {
            "org.bananav.Portals" => {
                let channel = |at: usize| {
                    bytes(at, 4)
                        .and_then(|b| b.try_into().ok())
                        .map(|b| f64::from(f32::from_le_bytes(b)))
                };
                // Blue, green and red, as this file's colors go.
                let color = (|| Some(vec![channel(48)?, channel(44)?, channel(40)?]))();
                vec![
                    w("seed", 0),
                    w("squares", 4),
                    f("maxTransparency", 8),
                    f("maxOffset", 16),
                    f("maxScale", 24),
                    f("borderSize", 32),
                    ("borderColor", color),
                ]
            }
            "org.bananav.VenetianBlinds" => {
                vec![f("blinds", 0), f("angle", 8), f("feather", 16)]
            }
            "org.bananav.Cascade3D" => vec![
                w("divisions", 0),
                w("direction", 4),
                f("twist", 8),
                f("specularLight", 16),
            ],
            "org.bananav.Blinds3D" => vec![
                w("divisions", 0),
                w("extraSpins", 4),
                f("stagger", 8),
                f("specularLight", 16),
                w("direction", 24),
            ],
            "org.bananav.FlyInOut3D" => vec![
                f("farX", 0),
                f("farY", 8),
                f("farZ", 16),
                f("rotationsX", 24),
                f("rotationsY", 32),
                f("rotationsZ", 40),
                f("specularLight", 48),
                w("direction", 56),
            ],
            "org.bananav.Shuffle3D" => vec![f("specularLight", 0)],
            "org.bananav.GradientWipe" => vec![f("thresholdBlend", 40)],
            _ => Vec::new(),
        }
    };
    fields
        .into_iter()
        .filter_map(|(name, v)| Some((name, v?)))
        .collect()
}

/// The plug-in for one of the file's own kind: BananaV's of its class, at
/// what its record says, or at its preset of that name when it says nothing.
fn native_effect(class: &str, preset: &str, data: &[u8]) -> Option<Effect> {
    let known = bv_ofx::registry().known();
    let id = NATIVE_TRANSITIONS
        .iter()
        .find(|(c, _)| c.eq_ignore_ascii_case(class))?
        .1;
    let info = known.iter().find(|i| i.identifier == id)?;
    let mut params = crate::presets::builtin_plugin_named(id, |name| plain(name) == plain(preset))
        .unwrap_or_default();
    for (name, value) in native_settings(id, data) {
        params.insert(
            name.to_string(),
            PluginParam {
                value: PluginValue::Num(value),
                keys: Vec::new(),
            },
        );
    }
    Some(Effect::plugin(PluginFx {
        id: info.identifier.clone(),
        version: info.version,
        name: info.label.clone(),
        params,
    }))
}

/// BananaV's own generator `id` at `params`.
fn own_generator(id: &str, params: BTreeMap<String, PluginParam>) -> Option<Effect> {
    let known = bv_ofx::registry().known();
    let info = known.iter().find(|i| i.identifier == id)?;
    Some(Effect::plugin(PluginFx {
        id: info.identifier.clone(),
        version: info.version,
        name: info.label.clone(),
        params,
    }))
}

/// The plug-in a `.veg`'s effect stands for: the same one installed, else
/// BananaV's own of its name; its parameters by their names, with their
/// keyframes.
fn effect_in(e: &DocEffect, rate: FrameRate) -> Option<Effect> {
    if let Some(class) = &e.native {
        if class.eq_ignore_ascii_case(crate::veg_generators::COLOR_GRADIENT_CLASS) {
            return own_generator(
                "org.bananav.ColorGradient",
                crate::veg_generators::gradient_in(&e.native_data, &e.native_keys, rate)?,
            );
        }
        if class.eq_ignore_ascii_case(crate::veg_generators::CREDIT_ROLL_CLASS) {
            return own_generator(
                "org.bananav.CreditRoll",
                crate::veg_generators::credit_in(&e.native_data)?,
            );
        }
        return native_effect(class, &e.preset, &e.native_data);
    }
    let known = bv_ofx::registry().known();
    let short = e.plugin.rsplit(':').next().unwrap_or(&e.plugin);
    // Titles & Text: its rich text and settings as BananaV's.
    if short.eq_ignore_ascii_case("titlesandtext") {
        return own_generator(
            "org.bananav.TitlesText",
            crate::veg_generators::titles_in(e, rate),
        );
    }
    let names = [short, short.strip_suffix("Transition").unwrap_or(short)];
    let same = |i: &&std::sync::Arc<bv_ofx::PluginInfo>| {
        let last = i.identifier.rsplit('.').next().unwrap_or_default();
        i.identifier.starts_with("org.bananav.")
            && names
                .iter()
                .any(|n| plain(&i.label) == plain(n) || plain(last) == plain(n))
    };
    let info = known
        .iter()
        .find(|i| i.identifier == e.plugin)
        .or_else(|| known.iter().find(same))?;
    let context = if e.transition {
        bv_ofx::Context::Transition
    } else if e.generator {
        bv_ofx::Context::Generator
    } else {
        bv_ofx::Context::Filter
    };
    let described = info.context(context).or_else(|| info.described.first())?;
    let find = |name: &str| {
        described
            .params
            .iter()
            .find(|p| plain(&p.name) == plain(name) || plain(&p.label) == plain(name))
    };
    let mut params = BTreeMap::new();
    for (name, value) in &e.params {
        let Some(p) = find(name) else {
            continue;
        };
        // The host moves it along the transition.
        if p.name == bv_engine::plugins::TRANSITION_PARAM {
            continue;
        }
        let value = match value {
            ParamValue::Text(t) => PluginValue::Text(t.clone()),
            _ if dimensions(p.ty).is_none() => continue,
            ParamValue::Number(x) => PluginValue::Num(fitted(vec![*x], p.ty)),
            ParamValue::Integer(x) => PluginValue::Num(fitted(vec![*x as f64], p.ty)),
            ParamValue::Numbers(v) => PluginValue::Num(fitted(v.clone(), p.ty)),
        };
        params.insert(
            p.name.clone(),
            PluginParam {
                value,
                keys: Vec::new(),
            },
        );
    }
    for (name, keys) in &e.keys {
        let Some(p) = find(name).filter(|p| dimensions(p.ty).is_some()) else {
            continue;
        };
        if let Some(param) = params.get_mut(&p.name) {
            param.keys = plugin_keys(keys, p.ty, rate);
        }
    }
    Some(Effect::plugin(PluginFx {
        id: info.identifier.clone(),
        version: info.version,
        name: info.label.clone(),
        params,
    }))
}

/// A curve's number among a parameter's interpolations.
fn interpolation_number(c: KeyCurve) -> i32 {
    match c {
        KeyCurve::Linear => 1,
        KeyCurve::Fast => 2,
        KeyCurve::Slow => 3,
        KeyCurve::Smooth => 4,
        KeyCurve::Sharp => 5,
        KeyCurve::Hold => 6,
        KeyCurve::Bezier => 9,
        KeyCurve::BezierSplit => 10,
    }
}

/// A parameter's keyframes as the file keeps them: the numbers with a
/// point have handles, a tenth of a frame away unless they were set.
fn keys_out(keys: &[PluginKey], ty: Option<bv_ofx::ParamType>, rate: FrameRate) -> Vec<DocKey> {
    use bv_ofx::ParamType as T;
    let frames = |t: Tick| t.0 as f64 * rate.as_f64() / TICKS_PER_SECOND as f64;
    let handled = matches!(ty, Some(T::Double | T::Double2D | T::Double3D));
    let whole = matches!(ty, Some(T::Integer | T::Boolean | T::Choice));
    keys.iter()
        .filter_map(|k| {
            let PluginValue::Num(v) = &k.value else {
                return None;
            };
            let frame = frames(k.at);
            Some(DocKey {
                frame,
                interpolation: if whole {
                    0
                } else {
                    interpolation_number(k.curve)
                },
                value: match v.as_slice() {
                    // A color without its alpha, as the file's colors are.
                    [r, g, b, _] => vec![*r, *g, *b],
                    v => v.to_vec(),
                },
                handles: if handled {
                    v.iter()
                        .enumerate()
                        .map(|(c, x)| match k.handles.get(c) {
                            Some(h) => [
                                frame + frames(h.before.0),
                                x + h.before.1,
                                frame + frames(h.after.0),
                                x + h.after.1,
                            ],
                            None => [frame - 0.1, *x, frame + 0.1, *x],
                        })
                        .collect()
                } else {
                    Vec::new()
                },
            })
        })
        .collect()
}

/// An effect as a `.veg` holds it: BananaV's own as their twins of the
/// same name, other makers' plug-ins as they are; the parameters that
/// are numbers by their names (an effect's first letter upper case).
/// `None` for the transitions the file keeps in its own kind.
fn effect_out(e: &Effect, rate: FrameRate) -> Option<DocEffect> {
    let fx = e.plugin.as_ref()?;
    if NATIVE_TRANSITIONS.iter().any(|(_, id)| *id == fx.id) {
        return None;
    }
    let known = bv_ofx::registry().known();
    let info = known.iter().find(|i| i.identifier == fx.id);
    let plugin = match (fx.id.as_str(), info) {
        ("org.bananav.WarpFlow", _) => "de.magix:WarpFlowTransition".to_string(),
        ("org.bananav.GLTransition", _) => "de.magix:glTransition".to_string(),
        (id, Some(i)) if id.starts_with("org.bananav.") => {
            format!("{VEG_PLUGINS}{}", plain(&i.label))
        }
        _ => fx.id.clone(),
    };
    let ty = |name: &str| {
        info.and_then(|i| i.described.first())
            .and_then(|d| d.params.iter().find(|p| p.name == name))
            .map(|p| p.ty)
    };
    let integer = |name: &str| {
        use bv_ofx::ParamType;
        matches!(
            ty(name),
            Some(ParamType::Integer | ParamType::Boolean | ParamType::Choice)
        )
    };
    let mut params = Vec::new();
    let mut keys = Vec::new();
    for (name, p) in &fx.params {
        let PluginValue::Num(v) = &p.value else {
            continue;
        };
        let value = match v.as_slice() {
            [x] if integer(name) => ParamValue::Integer(x.round() as i64),
            [x] => ParamValue::Number(*x),
            // A color without its alpha, as the file's colors are.
            [r, g, b, _] => ParamValue::Numbers(vec![*r, *g, *b]),
            [] => continue,
            v => ParamValue::Numbers(v.to_vec()),
        };
        let mut n = name.clone();
        if let Some(first) = n.get(..1) {
            let upper = first.to_ascii_uppercase();
            n.replace_range(..1, &upper);
        }
        if !p.keys.is_empty() {
            keys.push((n.clone(), keys_out(&p.keys, ty(name), rate)));
        }
        params.push((n, value));
    }
    Some(DocEffect {
        plugin,
        preset: String::new(),
        params,
        keys,
        ..Default::default()
    })
}

/// A media file where the project says, else beside the project.
fn locate(path: &str, beside: &Path) -> PathBuf {
    let p = PathBuf::from(path);
    if p.exists() {
        return p;
    }
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let near = beside.join(name);
    if near.exists() { near } else { p }
}

/// File › Import › VEG Project.
pub(crate) fn import(app: &mut BananaApp) {
    let Some(path) = rfd::FileDialog::new()
        .set_title(tr("Import VEG Project"))
        .add_filter(tr("VEG Project"), &["veg"])
        .add_filter(tr("All Files"), &["*"])
        .pick_file()
    else {
        return;
    };
    open(app, &path);
}

/// Opens `path` as a new, unsaved project, saying what did not come in.
pub(crate) fn open(app: &mut BananaApp, path: &Path) {
    let doc = match std::fs::read(path)
        .map_err(|e| e.to_string())
        .and_then(|bytes| veg::read(&bytes).map_err(|e| e.to_string()))
    {
        Ok(doc) => doc,
        Err(e) => {
            app.report_error(trf!("Could not open the project: {}", e));
            return;
        }
    };
    let beside = path.parent().unwrap_or(Path::new("."));
    let found: Vec<Option<MediaInfo>> = doc
        .media
        .iter()
        .map(|m| {
            if m.generated || m.path.to_ascii_lowercase().ends_with(".veg") {
                return None;
            }
            let at = locate(&m.path, beside);
            Engine::probe_file(&at.to_string_lossy()).ok()
        })
        .collect();
    let base = app.blank_project().settings;
    let mut imported = veg::build(&doc, &found, &base, &effect_in);
    // Its VST 3 plug-ins, found among those installed here.
    let folders = app.settings.prefs.vst_folders.clone();
    let not_installed = crate::vst::find_wanted(&mut imported.project, &folders);
    let events = imported.project.clips().count();
    app.open_imported(imported.project);
    app.settings.touch_recent(path);
    let mut message = trn!(
        events,
        "Project opened: {} event.",
        "Project opened: {} events.",
        events
    );
    let mut add = |text: String| {
        message.push_str(" · ");
        message.push_str(&text);
    };
    let missing = imported.missing.len();
    if missing > 0 {
        let mut names = imported.missing[..missing.min(3)].join(", ");
        if missing > 3 {
            names.push_str(", …");
        }
        add(trn!(
            missing,
            "{} media file was not found: {}.",
            "{} media files were not found: {}.",
            missing,
            names
        ));
    }
    let left = [
        (
            imported.skipped_events,
            trn!(
                imported.skipped_events,
                "{} event of generated media was left out.",
                "{} events of generated media were left out.",
                imported.skipped_events
            ),
        ),
        (
            imported.effects_left,
            trn!(
                imported.effects_left,
                "{} effect has no plug-in in BananaV.",
                "{} effects have no plug-in in BananaV.",
                imported.effects_left
            ),
        ),
        (
            imported.audio_fx_left,
            trn!(
                imported.audio_fx_left,
                "{} audio effect was left out.",
                "{} audio effects were left out.",
                imported.audio_fx_left
            ),
        ),
        (
            not_installed,
            trn!(
                not_installed,
                "{} VST 3 plug-in is not installed here: the sound goes through it untouched.",
                "{} VST 3 plug-ins are not installed here: the sound goes through them untouched.",
                not_installed
            ),
        ),
        (
            imported.envelopes_left,
            trn!(
                imported.envelopes_left,
                "{} envelope was left out.",
                "{} envelopes were left out.",
                imported.envelopes_left
            ),
        ),
        (
            imported.transitions_left,
            trn!(
                imported.transitions_left,
                "{} transition at an event's end was left out.",
                "{} transitions at events' ends were left out.",
                imported.transitions_left
            ),
        ),
    ];
    let mut any = missing > 0;
    for (n, text) in left {
        if n > 0 {
            add(text);
            any = true;
        }
    }
    if any {
        app.report_error(message);
    } else {
        app.notify(message);
    }
}

/// The user's documents folder, as the file's header keeps one.
fn documents() -> String {
    std::env::var_os("USERPROFILE")
        .map(|h| {
            PathBuf::from(h)
                .join("Documents")
                .to_string_lossy()
                .into_owned()
        })
        .unwrap_or_default()
}

/// File › Export › VEG Project.
pub(crate) fn export(app: &mut BananaApp) {
    let stem = app
        .project_path
        .as_ref()
        .and_then(|p| p.file_stem())
        .map_or_else(
            || tr("Untitled").to_string(),
            |s| s.to_string_lossy().into_owned(),
        );
    let Some(path) = rfd::FileDialog::new()
        .set_title(tr("Export VEG Project"))
        .add_filter(tr("VEG Project"), &["veg"])
        .set_file_name(format!("{stem}.veg"))
        .save_file()
    else {
        return;
    };
    let rate = app.project().settings.frame_rate;
    let options = veg::WriteOptions {
        path: path.to_string_lossy().into_owned(),
        documents: documents(),
        effect: &|e| effect_out(e, rate),
        generator: &|fx| crate::veg_generators::generator_out(fx, rate),
    };
    let bytes = veg::write(app.project(), &options);
    match std::fs::write(&path, bytes) {
        Ok(()) => app.notify(trf!("Exported to {}", path.display())),
        Err(e) => app.report_error(trf!("Could not export: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_meet_without_spaces_or_case() {
        assert_eq!(plain("Brightness and Contrast"), "brightnessandcontrast");
        assert_eq!(plain("HSL Adjust"), "hsladjust");
        assert_eq!(plain("{Svfx:sepia}"), "svfxsepia");
    }

    /// The transitions of the samples (`BV_VEG_SAMPLES2`, the folder of
    /// `t01_…veg` … `t27_…veg`): each comes in as BananaV's own, its settings
    /// by their names.
    #[test]
    #[ignore = "needs VEGAS's sample projects"]
    fn the_samples_transitions_come_in() {
        let dir = PathBuf::from(std::env::var("BV_VEG_SAMPLES2").expect("BV_VEG_SAMPLES2"));
        bv_engine::plugins::register_own();
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .expect("folder")
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.starts_with('t') && n.ends_with(".veg"))
            .collect();
        names.sort();
        let (mut came, mut wrong) = (0, Vec::new());
        for name in &names {
            let bytes = std::fs::read(dir.join(name)).expect("sample");
            let doc = veg::read(&bytes).expect("reads");
            let found = vec![None; doc.media.len()];
            let made = veg::build(&doc, &found, &Default::default(), &effect_in);
            let fx = made
                .project
                .clips()
                .find_map(|(_, c)| c.transition_fx.clone());
            match fx {
                Some(fx) => {
                    came += 1;
                    let read: Vec<String> = fx
                        .params
                        .iter()
                        .map(|(k, v)| format!("{k}={:?}", v.value))
                        .collect();
                    eprintln!("{name}: {} {}", fx.id, read.join(", "));
                }
                None => wrong.push(name.clone()),
            }
            assert_eq!(made.effects_left, 0, "{name}");
        }
        eprintln!("{came} of {} came in; not: {wrong:?}", names.len());
        assert!(wrong.is_empty());
    }

    /// Another program's pictures of its fades, transitions at events' ends
    /// and starts, progress envelopes and crossfades (`BV_TR_SAMPLES`, the
    /// folder whose `validacao/vegas` and `validacao/vegas2` hold each
    /// round's `render.veg`, `casos.json` and frames): each project read
    /// and drawn by BananaV frame by frame — its colors over black, its
    /// alpha over nothing — against those frames, a level apart at most.
    #[test]
    #[ignore = "needs VEGAS's renders"]
    fn transitions_draw_as_the_measured_frames() {
        let dir = PathBuf::from(std::env::var("BV_TR_SAMPLES").expect("BV_TR_SAMPLES"));
        bv_engine::plugins::register_own();
        let mut worst_all = 0u8;
        for round in ["vegas", "vegas2"] {
            let folder = dir.join("validacao").join(round);
            let bytes = std::fs::read(folder.join("render.veg")).expect("render.veg");
            let doc = veg::read(&bytes).expect("reads");
            let found = vec![None; doc.media.len()];
            let made = veg::build(&doc, &found, &Default::default(), &effect_in);
            assert_eq!(made.effects_left, 0, "{round}: effects left out");
            assert_eq!(made.skipped_events, 0, "{round}: events left out");
            let cases: serde_json::Value = serde_json::from_slice(
                &std::fs::read(folder.join("casos.json")).expect("casos.json"),
            )
            .expect("cases");
            let per_case = cases["segundos_por_caso"].as_f64().unwrap_or(3.0);
            let fps = cases["fps"].as_f64().unwrap_or(30.0);
            let names: Vec<String> = cases["casos"]
                .as_array()
                .expect("cases")
                .iter()
                .map(|c| c["nome"].as_str().unwrap_or("?").to_string())
                .collect();
            let mut engine = bv_engine::Engine::new(std::sync::Arc::new(|| {}));
            if std::env::var("BV_TR_GPU").is_err() {
                engine.set_gpu(bv_engine::GpuChoice::Off);
            }
            engine.set_project(&made.project);
            let rate = made.project.settings.frame_rate;
            let mut worst = vec![(0u8, 0usize); names.len()];
            let mut n = 0usize;
            loop {
                let png = folder.join("quadros").join(format!("quadros_{n:06}.png"));
                let Ok(png) = std::fs::read(&png) else {
                    break;
                };
                let theirs = bv_png::decode(&png).expect("png").to_rgba8();
                let t = rate.frame_start(n as i64);
                let (_, _, over_black) = engine.snapshot(t, [0.0, 0.0, 0.0, 1.0]).expect("drawn");
                let (_, _, alone) = engine.snapshot(t, [0.0; 4]).expect("drawn");
                let mut most = 0u8;
                for (i, px) in theirs.as_chunks::<4>().0.iter().enumerate() {
                    for c in 0..3 {
                        most = most.max(px[c].abs_diff(over_black[i * 4 + c]));
                    }
                    most = most.max(px[3].abs_diff(alone[i * 4 + 3]));
                }
                let case = ((n as f64 / fps) / per_case) as usize;
                if let Some(w) = worst.get_mut(case)
                    && most > w.0
                {
                    *w = (most, n);
                }
                n += 1;
            }
            assert!(n > 0, "{round}: no frames");
            for (name, (most, at)) in names.iter().zip(&worst) {
                eprintln!("{round} {name}: {most} (frame {at})");
                worst_all = worst_all.max(*most);
            }
        }
        assert!(worst_all <= 1, "{worst_all} levels apart");
    }

    /// Another program's pictures of its media generators, a case a frame
    /// (`BV_MG_RENDERS`, the folder of its `render.veg`, `casos.json` and
    /// `quadros_cpu`): the project read and drawn by BananaV frame by frame
    /// — its colors over black, its alpha over nothing —, the generators
    /// whose pictures BananaV works out as that program's renders show
    /// matching them to the level.
    #[test]
    #[ignore = "needs VEGAS's renders"]
    fn generators_draw_as_the_measured_frames() {
        let dir = PathBuf::from(std::env::var("BV_MG_RENDERS").expect("BV_MG_RENDERS"));
        bv_engine::plugins::register_own();
        let bytes = std::fs::read(dir.join("render.veg")).expect("render.veg");
        let doc = veg::read(&bytes).expect("reads");
        let found = vec![None; doc.media.len()];
        let made = veg::build(&doc, &found, &Default::default(), &effect_in);
        let cases: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("casos.json")).expect("casos.json"))
                .expect("cases");
        let frames = cases["quadros"].as_array().expect("frames");
        let mut engine = bv_engine::Engine::new(std::sync::Arc::new(|| {}));
        if std::env::var("BV_MG_GPU").is_err() {
            engine.set_gpu(bv_engine::GpuChoice::Off);
        }
        engine.set_project(&made.project);
        let rate = made.project.settings.frame_rate;
        let pictures = std::env::var("BV_MG_FOLDER").unwrap_or_else(|_| "quadros_cpu".into());
        let only = std::env::var("BV_MG_ONLY").ok();
        // Each generator's frames: how many, how many alike, the worst.
        let mut tally: BTreeMap<String, (usize, usize, u8, usize)> = BTreeMap::new();
        for (n, case) in frames.iter().enumerate() {
            let kind = case["gerador"].as_str().unwrap_or("?").to_string();
            // (A case's API error is about its setting up: the frame is
            // what the project holds, as it is.)
            if only.as_ref().is_some_and(|o| !kind.contains(o.as_str())) {
                continue;
            }
            let png = dir.join(&pictures).join(format!("quadros_{n:06}.png"));
            let Ok(png) = std::fs::read(&png) else {
                continue;
            };
            let theirs = bv_png::decode(&png).expect("png").to_rgba8();
            let t = rate.frame_start(n as i64);
            let (w, h, over_black) = engine.snapshot(t, [0.0, 0.0, 0.0, 1.0]).expect("drawn");
            let (_, _, alone) = engine.snapshot(t, [0.0; 4]).expect("drawn");
            // BV_MG_DUMP: a folder for BananaV's pictures, to look at.
            if let Ok(out) = std::env::var("BV_MG_DUMP") {
                let png = bv_png::encode_rgba8(w, h, &over_black, 1);
                std::fs::write(PathBuf::from(out).join(format!("bv_{n:06}.png")), png)
                    .expect("dump");
            }
            let (mut most, mut off) = (0u8, 0usize);
            for (i, px) in theirs.as_chunks::<4>().0.iter().enumerate() {
                let mut d = 0u8;
                for c in 0..3 {
                    d = d.max(px[c].abs_diff(over_black[i * 4 + c]));
                }
                d = d.max(px[3].abs_diff(alone[i * 4 + 3]));
                most = most.max(d);
                off += usize::from(d > 0);
            }
            if most > 0 {
                eprintln!(
                    "{n}: {kind} {} {}: {most} levels, {off} pixels",
                    case["preset"].as_str().unwrap_or("-"),
                    case["variacao"].as_str().unwrap_or("")
                );
            }
            let t = tally.entry(kind).or_default();
            t.0 += 1;
            t.1 += usize::from(most == 0);
            if most > t.2 {
                (t.2, t.3) = (most, n);
            }
        }
        for (kind, (count, alike, most, at)) in &tally {
            eprintln!("{kind}: {alike} of {count} alike, worst {most} (frame {at})");
        }
        // Worked out as that program's renders show: to the level.
        for kind in [
            "Solid Color",
            "Checkerboard",
            "Test Pattern",
            "Noise Texture",
            "Gradiente de cor",
        ] {
            if let Some((count, alike, most, at)) = tally.get(kind) {
                assert_eq!(count, alike, "{kind}: {most} levels apart at frame {at}");
            }
        }
    }

    /// Another program's generated media (`BV_MG_SAMPLES`, the folder of
    /// its sample projects) read, written back by BananaV and read again:
    /// the same generators with the same settings, their length, size,
    /// pixels' shape, fields, alpha and rotation; the OpenFX ones' settings
    /// written as that program writes them, byte for byte.
    #[test]
    #[ignore = "needs VEGAS's sample projects"]
    fn generated_media_go_back_as_they_came() {
        let dir = PathBuf::from(std::env::var("BV_MG_SAMPLES").expect("BV_MG_SAMPLES"));
        bv_engine::plugins::register_own();
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .expect("folder")
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.ends_with(".veg"))
            .collect();
        names.sort();
        let (mut media, mut same_bytes, mut ofx) = (0, 0, 0);
        for name in &names {
            let bytes = std::fs::read(dir.join(name)).expect("sample");
            let doc = veg::read(&bytes).expect("reads");
            let found = vec![None; doc.media.len()];
            let made = veg::build(&doc, &found, &Default::default(), &effect_in);
            let rate = made.project.settings.frame_rate;
            let options = veg::WriteOptions {
                path: "C:\\x\\back.veg".into(),
                documents: String::new(),
                effect: &|e| effect_out(e, rate),
                generator: &|fx| crate::veg_generators::generator_out(fx, rate),
            };
            let written = veg::write(&made.project, &options);
            let again = veg::read(&written).expect("reads back");
            let back = veg::build(
                &again,
                &vec![None; again.media.len()],
                &Default::default(),
                &effect_in,
            );
            let generated = |p: &bv_timeline::Project| -> Vec<(PluginFx, Option<bv_timeline::VideoStreamInfo>, Tick)> {
                p.media
                    .iter()
                    .filter_map(|m| match &m.source {
                        bv_timeline::MediaSource::Generated(bv_timeline::Generator::Plugin(fx)) => {
                            Some(((**fx).clone(), m.video, m.duration))
                        }
                        _ => None,
                    })
                    .collect()
            };
            let (a, b) = (generated(&made.project), generated(&back.project));
            assert_eq!(a.len(), b.len(), "{name}: media");
            for ((fa, va, da), (fb, vb, db)) in a.iter().zip(&b) {
                media += 1;
                assert_eq!(fa.id, fb.id, "{name}");
                assert_eq!((va, da), (vb, db), "{name} {}", fa.id);
                for (k, pa) in &fa.params {
                    let pb = fb
                        .params
                        .get(k)
                        .unwrap_or_else(|| panic!("{name} {}: {k}", fa.id));
                    match (&pa.value, &pb.value) {
                        (PluginValue::Num(x), PluginValue::Num(y)) => {
                            assert_eq!(x.len(), y.len(), "{name} {k}");
                            for (u, v) in x.iter().zip(y) {
                                assert!((u - v).abs() < 1e-6, "{name} {} {k}: {x:?} {y:?}", fa.id);
                            }
                        }
                        (x, y) => assert_eq!(x, y, "{name} {} {k}", fa.id),
                    }
                    assert_eq!(pa.keys.len(), pb.keys.len(), "{name} {} {k}: keys", fa.id);
                }
            }
            // The OpenFX generators' settings as that program wrote them.
            let params = |d: &veg::Document| -> Vec<Vec<(String, veg::ParamValue)>> {
                d.media
                    .iter()
                    .filter_map(|m| m.generator.as_ref()?.effects.first().cloned())
                    .filter(|e| {
                        e.native.is_none() && e.keys.is_empty() && !e.plugin.contains("titles")
                    })
                    .map(|e| e.params)
                    .collect()
            };
            for (theirs, ours) in params(&doc).iter().zip(params(&again).iter()) {
                ofx += 1;
                same_bytes += usize::from(theirs == ours);
                assert_eq!(theirs, ours, "{name}");
            }
        }
        eprintln!("{media} generated media back; {same_bytes} of {ofx} OpenFX settings alike");
        assert!(media >= 160, "{media}");
    }

    #[test]
    fn media_are_found_beside_the_project() {
        let dir = std::env::temp_dir().join(format!("bv-veg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("clip.wav"), b"x").unwrap();
        assert_eq!(
            locate("D:\\elsewhere\\clip.wav", &dir),
            dir.join("clip.wav")
        );
        assert_eq!(
            locate("D:\\elsewhere\\gone.wav", &dir),
            PathBuf::from("D:\\elsewhere\\gone.wav")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
