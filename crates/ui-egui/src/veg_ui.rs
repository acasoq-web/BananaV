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
const VEG_PLUGINS: &str = "com.vegascreativesoftware:";

/// A name without its spaces, signs and case: "Brightness and Contrast"
/// and `brightnessandcontrast` are one.
fn plain(s: &str) -> String {
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

/// The plug-in a `.veg`'s effect stands for: the same one installed, else
/// BananaV's own of its name; its parameters by their names, with their
/// keyframes.
fn effect_in(e: &DocEffect, rate: FrameRate) -> Option<Effect> {
    if let Some(class) = &e.native {
        return native_effect(class, &e.preset, &e.native_data);
    }
    let known = bv_ofx::registry().known();
    let short = e.plugin.rsplit(':').next().unwrap_or(&e.plugin);
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
    let imported = veg::build(&doc, &found, &base, &effect_in);
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
