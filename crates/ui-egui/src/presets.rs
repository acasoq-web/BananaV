//! Predefinições de efeitos e de Pan/Crop: as que vêm com o BananaV e as
//! salvas pelo usuário (em `%APPDATA%\BananaV\predefinicoes.json`), OFX
//! plug-ins' included (by plug-in, their parameters' values) and VST 3
//! effects' (by module and class, their values and states).

use crate::i18n::tr;
use std::collections::BTreeMap;
use std::path::PathBuf;

use bv_timeline::{EffectKind, PluginParam, Transform};
use serde::{Deserialize, Serialize};

/// An OFX plug-in's values: its parameters' that differ from its defaults.
pub type PluginValues = BTreeMap<String, PluginParam>;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Presets {
    /// Por tipo de efeito (nome Debug do `EffectKind`): (nome, parâmetros).
    #[serde(default)]
    effects: BTreeMap<String, Vec<(String, Vec<f32>)>>,
    #[serde(default)]
    pan_crop: Vec<(String, Transform)>,
    /// OFX plug-ins', by plug-in identifier: (name, values).
    #[serde(default)]
    plugins: BTreeMap<String, Vec<(String, PluginValues)>>,
    /// VST 3 effects', by [`vst_key`]: (name, preset).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    vst: BTreeMap<String, Vec<(String, VstPreset)>>,
}

/// A VST 3 effect's preset: its parameters' values and its component's
/// and controller's states, as the effect keeps them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VstPreset {
    #[serde(default)]
    pub params: BTreeMap<u32, f64>,
    #[serde(default, with = "bv_core::base64::text")]
    pub state: Vec<u8>,
    #[serde(default, with = "bv_core::base64::text")]
    pub controller_state: Vec<u8>,
}

impl VstPreset {
    /// Effect `v`'s settings as a preset.
    pub fn of(v: &bv_timeline::VstFx) -> VstPreset {
        VstPreset {
            params: v.params.clone(),
            state: v.state.clone(),
            controller_state: v.controller_state.clone(),
        }
    }
}

/// What a VST 3 effect's presets are kept under: its module's file name
/// and its class (another copy of the module finds them too).
pub fn vst_key(v: &bv_timeline::VstFx) -> String {
    let file = v
        .path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().to_string());
    format!("{file}#{}", v.class_id)
}

fn path() -> PathBuf {
    bv_platform::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("BananaV")
        .join("predefinicoes.json")
}

fn key(k: EffectKind) -> String {
    format!("{k:?}")
}

/// A preset's string with BananaV's looks (LUT Filter's presets' tables)
/// in `%APPDATA%\BananaV\LUTs`, written there the first time they are
/// needed.
fn looks_in_place(text: &str) -> String {
    static DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    if !text.starts_with(bv_engine::plugins::LOOKS) {
        return text.to_string();
    }
    let dir = DIR.get_or_init(|| {
        let dir = bv_platform::config_dir()?.join("BananaV").join("LUTs");
        bv_engine::plugins::write_looks(&dir).ok()?;
        Some(dir)
    });
    match dir {
        Some(d) => bv_engine::plugins::resolve_looks(text, d),
        None => text.to_string(),
    }
}

/// The factory presets of BananaV's own plug-in `id`: their names (in
/// the interface's language) and values.
pub fn builtin_plugin(id: &str) -> Vec<(String, PluginValues)> {
    bv_engine::plugins::own_presets(id)
        .into_iter()
        .map(|p| (tr(p.name).to_string(), plugin_values(p.values)))
        .collect()
}

/// The values of the factory preset of `id` whose name (as the code has it,
/// not translated) passes `wanted`.
pub fn builtin_plugin_named(id: &str, wanted: impl Fn(&str) -> bool) -> Option<PluginValues> {
    bv_engine::plugins::own_presets(id)
        .into_iter()
        .find(|p| wanted(p.name))
        .map(|p| plugin_values(p.values))
}

fn plugin_values(values: Vec<(&'static str, bv_engine::plugins::PresetValue)>) -> PluginValues {
    use bv_engine::plugins::PresetValue;
    use bv_timeline::PluginValue;
    values
        .into_iter()
        .map(|(name, v)| {
            let value = match v {
                PresetValue::Num(n) => PluginValue::Num(n),
                PresetValue::Curves(c) => PluginValue::Curves(c),
                PresetValue::Text(t) => PluginValue::Text(looks_in_place(&t)),
            };
            (
                name.to_string(),
                PluginParam {
                    value,
                    keys: Vec::new(),
                },
            )
        })
        .collect()
}

/// Predefinições de fábrica: (nome, pares índice→valor sobre o padrão).
pub fn builtin(k: EffectKind) -> Vec<(&'static str, Vec<(usize, f32)>)> {
    use bv_timeline::fx::grade::*;
    // The audio effects that follow VEGAS's: their presets name every value.
    if let Some(list) = bv_timeline::audio_fx::presets(k) {
        return list
            .iter()
            .map(|(name, values)| (tr(name), values.iter().copied().enumerate().collect()))
            .collect();
    }
    match k {
        EffectKind::ColorGrade => vec![
            (tr("Warm"), vec![(TEMPERATURE, 0.35), (TINT, 0.05)]),
            (tr("Cool"), vec![(TEMPERATURE, -0.35)]),
            (
                tr("High Contrast"),
                vec![(CONTRAST, 0.35), (SATURATION, 1.15)],
            ),
            (
                tr("Faded"),
                vec![
                    (CONTRAST, -0.25),
                    (SATURATION, 0.7),
                    (LIFT, 0.06),
                    (LIFT + 1, 0.06),
                    (LIFT + 2, 0.06),
                ],
            ),
            (
                tr("Cinema (teal-orange)"),
                vec![
                    (LIFT + 2, 0.06),
                    (GAIN, 0.06),
                    (GAIN + 2, -0.04),
                    (CONTRAST, 0.15),
                ],
            ),
        ],
        EffectKind::BrightnessContrast => vec![
            (tr("Brighter"), vec![(0, 0.15)]),
            (tr("More Contrast"), vec![(1, 0.3)]),
        ],
        EffectKind::GaussianBlur => vec![
            (tr("Smooth"), vec![(0, 3.0)]),
            (tr("Strong"), vec![(0, 30.0)]),
        ],
        EffectKind::Vignette => vec![
            (tr("Light"), vec![(0, 0.3)]),
            (tr("Strong"), vec![(0, 0.85), (1, 0.4)]),
        ],
        EffectKind::Sharpen => vec![
            (tr("Light"), vec![(0, 0.4)]),
            (tr("Strong"), vec![(0, 1.6)]),
        ],
        EffectKind::ChromaKey => vec![
            (tr("Green Screen"), vec![(0, 0.0), (1, 1.0), (2, 0.0)]),
            (tr("Blue Screen"), vec![(0, 0.0), (1, 0.0), (2, 1.0)]),
        ],
        EffectKind::Eq3 => vec![
            (
                tr("Clearer Voice"),
                vec![(0, -3.0), (2, 2.0), (3, 2500.0), (5, 3.0)],
            ),
            (tr("Bass Boost"), vec![(0, 6.0)]),
            (
                tr("Telephone"),
                vec![(0, -15.0), (2, 6.0), (3, 1500.0), (5, -15.0)],
            ),
        ],
        EffectKind::Compressor => vec![
            (
                tr("Voice"),
                vec![(0, -20.0), (1, 3.0), (2, 5.0), (3, 120.0), (4, 4.0)],
            ),
            (
                tr("Strong"),
                vec![(0, -28.0), (1, 8.0), (2, 2.0), (3, 80.0), (4, 8.0)],
            ),
        ],
        EffectKind::Reverb => vec![
            (tr("Small Room"), vec![(0, 0.3), (2, 0.15)]),
            (tr("Hall"), vec![(0, 0.75), (2, 0.3)]),
            (tr("Cathedral"), vec![(0, 0.95), (1, 0.3), (2, 0.45)]),
        ],
        EffectKind::Delay => vec![
            (tr("Short Echo"), vec![(0, 120.0), (1, 0.2), (2, 0.25)]),
            (tr("Long Echo"), vec![(0, 600.0), (1, 0.45), (2, 0.35)]),
        ],
        EffectKind::HighPass => vec![
            (tr("Remove Rumble (80 Hz)"), vec![(0, 80.0)]),
            (tr("Remove Wind (150 Hz)"), vec![(0, 150.0)]),
        ],
        EffectKind::ModernEq => modern_eq(),
        _ => Vec::new(),
    }
}

/// The Modern Equalizer's band types and routings, as its parameters
/// count them.
mod eq_type {
    pub const HIGH_PASS: f32 = 0.0;
    pub const LOW_PASS: f32 = 1.0;
    pub const LOW_SHELF: f32 = 2.0;
    pub const HIGH_SHELF: f32 = 3.0;
    pub const BAND_PASS: f32 = 4.0;
    pub const PEAK: f32 = 5.0;
}
mod eq_route {
    pub const STEREO: f32 = 0.0;
    pub const MID: f32 = 3.0;
    pub const SIDE: f32 = 4.0;
}

/// One of the Modern Equalizer's factory presets: Global Scale (%), Mono
/// Below (Hz), Stereo Width (%), Output Gain (dB) and Oversampling's
/// choice; then its eight bands, each in use and on — type, slope (dB an
/// octave), frequency, gain, Q and routing.
fn eq_preset(globals: [f32; 5], bands: [(f32, f32, f32, f32, f32, f32); 8]) -> Vec<(usize, f32)> {
    use bv_timeline::fx::modern_eq::*;
    let mut v = vec![
        (GLOBAL_SCALE, globals[0]),
        (MONO_BELOW, globals[1]),
        (STEREO_WIDTH, globals[2]),
        (OUTPUT_GAIN, globals[3]),
        (OVERSAMPLING, globals[4]),
    ];
    for (b, (kind, slope, freq, gain, q, route)) in bands.into_iter().enumerate() {
        v.extend([
            (at(b, USED), 1.0),
            (at(b, ON), 1.0),
            (at(b, TYPE), kind),
            (at(b, SLOPE), slope),
            (at(b, FREQUENCY), freq),
            (at(b, GAIN), gain),
            (at(b, Q), q),
            (at(b, ROUTING), route),
        ]);
    }
    v
}

/// The Modern Equalizer's factory presets, VEGAS's: each sets all eight
/// bands.
fn modern_eq() -> Vec<(&'static str, Vec<(usize, f32)>)> {
    use eq_route::*;
    use eq_type::*;
    vec![
        (
            tr("Flat"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (LOW_SHELF, 12.0, 100.0, 0.0, 0.714, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 505.779, -0.0691643, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 5000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 6000.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 9931.17, -0.0691643, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("2000 Hz Notch"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (LOW_SHELF, 12.0, 100.0, 0.0, 0.714, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 2000.0, -20.0, 10.0, STEREO),
                    (PEAK, 12.0, 5000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 6000.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 10_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("2000 Hz Resonance"),
            eq_preset(
                [100.0, 20.0, 100.0, -13.0, 1.0],
                [
                    (LOW_SHELF, 12.0, 100.0, 0.0, 0.714066, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 2000.0, 20.0, 10.0, STEREO),
                    (PEAK, 12.0, 2000.0, 20.0, 10.0, STEREO),
                    (PEAK, 12.0, 5000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 6000.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 10_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bass Boost"),
            eq_preset(
                [100.0, 20.0, 100.0, -3.0, 1.0],
                [
                    (PEAK, 12.0, 100.0, 10.0, 0.714066, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 5000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 6000.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 10_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Loudness"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (LOW_SHELF, 12.0, 100.0, 6.0, 0.707, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 5000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 6000.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 16_000.0, 4.0, 0.5, STEREO),
                ],
            ),
        ),
        (
            tr("Low Cut Filter 100 Hz"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (HIGH_PASS, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 5000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 6000.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 10_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Low Pass 500 Hz"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (LOW_SHELF, 12.0, 100.0, 0.0, 0.714066, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (LOW_PASS, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 5000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 6000.0, 0.0, 0.707, STEREO),
                    (LOW_PASS, 12.0, 500.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Mid Boost"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (LOW_SHELF, 12.0, 100.0, 0.0, 0.714066, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 6.0, 0.707, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 5000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 6000.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 10_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Phone"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (HIGH_PASS, 12.0, 790.0, 10.0, 0.714066, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 5000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 6000.0, 0.0, 0.707, STEREO),
                    (LOW_PASS, 12.0, 3000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Sub Bass"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (PEAK, 12.0, 30.0, 10.0, 0.714066, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 5000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 6000.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 10_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Treble Boost"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (LOW_SHELF, 12.0, 100.0, 0.0, 0.714066, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 5000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 6000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 16_000.0, 10.0, 0.5, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: Bell Notch"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 817.997, -12.0, 10.0, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 4000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 817.997, -12.0, 10.0, STEREO),
                    (PEAK, 12.0, 10_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: Wide Bell"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 817.997, 2.0, 0.382329, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 4000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 817.997, 2.0, 0.382329, STEREO),
                    (PEAK, 12.0, 10_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: Band Pass 6 dB"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (BAND_PASS, 6.0, 800.0, 0.0, 1.0, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 4000.0, 0.0, 0.707, STEREO),
                    (BAND_PASS, 6.0, 800.0, 0.0, 1.0, STEREO),
                    (PEAK, 12.0, 10_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: Band Pass 12 dB"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (BAND_PASS, 12.0, 800.0, 0.0, 1.0, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 4000.0, 0.0, 0.707, STEREO),
                    (BAND_PASS, 12.0, 800.0, 0.0, 1.0, STEREO),
                    (PEAK, 12.0, 10_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: Band Pass 18 dB"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (BAND_PASS, 18.0, 800.0, 0.0, 1.0, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 4000.0, 0.0, 0.707, STEREO),
                    (BAND_PASS, 18.0, 800.0, 0.0, 1.0, STEREO),
                    (PEAK, 12.0, 10_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: High Pass 6 dB"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (HIGH_PASS, 6.0, 30.4626, 0.0, 0.767937, STEREO),
                    (PEAK, 12.0, 104.28, 2.10131, 1.11575, STEREO),
                    (LOW_SHELF, 12.0, 59.8715, -0.943626, 0.707, STEREO),
                    (PEAK, 12.0, 300.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 400.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 600.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 800.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: High Pass 12 dB"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (HIGH_PASS, 12.0, 30.4626, 0.0, 0.767937, STEREO),
                    (PEAK, 12.0, 104.28, 2.10131, 1.11575, STEREO),
                    (LOW_SHELF, 12.0, 59.8715, -0.943626, 0.707, STEREO),
                    (PEAK, 12.0, 300.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 400.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 600.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 800.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: High Pass 24 dB"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (HIGH_PASS, 24.0, 30.4626, 0.0, 0.767937, STEREO),
                    (PEAK, 12.0, 104.28, 2.10131, 1.11575, STEREO),
                    (LOW_SHELF, 12.0, 59.8715, -0.943626, 0.707, STEREO),
                    (PEAK, 12.0, 300.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 400.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 600.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 800.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: High Pass 36 dB"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (HIGH_PASS, 36.0, 30.4626, 0.0, 0.767937, STEREO),
                    (PEAK, 12.0, 104.28, 2.10131, 1.11575, STEREO),
                    (LOW_SHELF, 12.0, 59.8715, -0.943626, 0.707, STEREO),
                    (PEAK, 12.0, 300.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 400.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 600.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 800.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: Low + High Pass 6 dB"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (HIGH_PASS, 6.0, 30.4626, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (LOW_SHELF, 12.0, 59.8715, -0.943626, 0.707, STEREO),
                    (PEAK, 12.0, 300.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 400.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 600.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 800.0, 0.0, 0.707, STEREO),
                    (LOW_PASS, 6.0, 18_000.0, -3.38905, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: Low + High Pass 12 dB"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (HIGH_PASS, 12.0, 30.4626, 0.0, 0.767937, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 4000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 6000.0, 0.0, 0.707, STEREO),
                    (LOW_PASS, 12.0, 18_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: Low + High Pass 24 dB"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (HIGH_PASS, 24.0, 30.4626, 0.0, 0.767937, STEREO),
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (LOW_SHELF, 12.0, 59.8715, -0.943626, 0.707, STEREO),
                    (PEAK, 12.0, 300.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 400.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 600.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 800.0, 0.0, 0.707, STEREO),
                    (LOW_PASS, 24.0, 18_000.0, -3.80403, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: Low + High Pass 36 dB"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (HIGH_PASS, 36.0, 30.4626, 0.0, 0.767937, STEREO),
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (LOW_SHELF, 12.0, 59.8715, -0.943626, 0.707, STEREO),
                    (PEAK, 12.0, 300.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 400.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 600.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 800.0, 0.0, 0.707, STEREO),
                    (LOW_PASS, 36.0, 18_000.0, -4.08069, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: Low Pass 6 dB"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (LOW_SHELF, 12.0, 100.0, 0.0, 0.767937, STEREO),
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (LOW_SHELF, 12.0, 59.8715, -0.943626, 0.707, STEREO),
                    (PEAK, 12.0, 300.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 400.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 600.0, 0.0, 0.707, STEREO),
                    (LOW_SHELF, 12.0, 817.997, -12.0, 10.0, STEREO),
                    (LOW_PASS, 6.0, 17_800.0, -6.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: Low Pass 12 dB"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (LOW_SHELF, 12.0, 100.0, 0.0, 0.767937, STEREO),
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (LOW_SHELF, 12.0, 59.8715, -0.943626, 0.707, STEREO),
                    (PEAK, 12.0, 300.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 400.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 600.0, 0.0, 0.707, STEREO),
                    (LOW_SHELF, 12.0, 817.997, -12.0, 10.0, STEREO),
                    (LOW_PASS, 12.0, 17_800.0, -6.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: Low Pass 24 dB"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (LOW_SHELF, 12.0, 100.0, 0.0, 0.767937, STEREO),
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (LOW_SHELF, 12.0, 59.8715, -0.943626, 0.707, STEREO),
                    (PEAK, 12.0, 300.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 400.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 600.0, 0.0, 0.707, STEREO),
                    (LOW_SHELF, 12.0, 817.997, -12.0, 10.0, STEREO),
                    (LOW_PASS, 24.0, 17_800.0, -6.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: Low Pass 36 dB"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (LOW_SHELF, 12.0, 100.0, 0.0, 0.767937, STEREO),
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (LOW_SHELF, 12.0, 59.8715, -0.943626, 0.707, STEREO),
                    (PEAK, 12.0, 300.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 400.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 600.0, 0.0, 0.707, STEREO),
                    (LOW_SHELF, 12.0, 817.997, -12.0, 10.0, STEREO),
                    (LOW_PASS, 36.0, 17_800.0, -6.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Bands: Low + High Shelf"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (LOW_SHELF, 12.0, 100.0, 0.0, 0.767937, STEREO),
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (LOW_SHELF, 12.0, 59.8715, -0.943626, 0.707, STEREO),
                    (PEAK, 12.0, 300.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 400.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 600.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 800.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 10_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Drums"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (HIGH_PASS, 24.0, 28.8747, 0.0, 0.809353, STEREO),
                    (LOW_SHELF, 12.0, 59.87, -0.900002, 0.707, STEREO),
                    (PEAK, 12.0, 104.28, 2.10131, 1.11575, STEREO),
                    (PEAK, 12.0, 726.891, 1.59078, 0.707, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 4000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 5320.0, 2.6, 0.587, STEREO),
                    (LOW_PASS, 24.0, 19_300.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Master: Mid/Side"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (LOW_SHELF, 12.0, 31.0103, -5.48515, 0.544715, SIDE),
                    (PEAK, 12.0, 87.5096, 0.613861, 1.16713, MID),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 2030.0, 1.0, 0.65, MID),
                    (PEAK, 12.0, 4000.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 7580.0, 2.4, 0.571, SIDE),
                    (PEAK, 12.0, 10_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Master: Mid/Side, Basic"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (LOW_SHELF, 12.0, 31.0103, -5.48515, 0.544715, SIDE),
                    (PEAK, 12.0, 180.7, -0.700001, 1.066, MID),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 2030.0, 1.0, 0.65, MID),
                    (PEAK, 12.0, 4000.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 7580.0, 2.4, 0.571, SIDE),
                    (PEAK, 12.0, 10_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Master: Mid/Side Wide Radio Bass"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (PEAK, 12.0, 43.5764, 0.0, 0.707, SIDE),
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 300.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 400.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 600.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 800.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1018.57, 0.0594063, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Master: Smiley Face"),
            eq_preset(
                [100.0, 78.0877, 100.0, 0.0, 1.0],
                [
                    (PEAK, 12.0, 49.688, 1.05, 0.707, STEREO),
                    (PEAK, 12.0, 251.149, -0.918591, 0.703296, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 2750.0, 1.1, 0.511, STEREO),
                    (PEAK, 12.0, 4000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 6000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 10_000.0, 0.0, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Master: Smooth Top End"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 300.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 400.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 600.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 800.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 20_000.0, 1.4, 0.25, STEREO),
                ],
            ),
        ),
        (
            tr("Mix: Mid/Side More Clarity"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (PEAK, 12.0, 100.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 200.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 1720.0, 1.0, 0.713, MID),
                    (PEAK, 12.0, 600.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 800.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1018.57, 0.0594063, 0.707, STEREO),
                ],
            ),
        ),
        (
            tr("Vocal"),
            eq_preset(
                [100.0, 20.0, 100.0, 0.0, 1.0],
                [
                    (HIGH_PASS, 12.0, 43.5106, 0.0, 0.707, STEREO),
                    (LOW_SHELF, 12.0, 104.28, -1.93069, 0.711529, STEREO),
                    (PEAK, 12.0, 500.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 1000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 2000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 4000.0, 0.0, 0.707, STEREO),
                    (PEAK, 12.0, 5320.0, 1.1, 0.707, STEREO),
                    (HIGH_SHELF, 12.0, 9300.0, 2.9, 0.534, STEREO),
                ],
            ),
        ),
    ]
}

/// Predefinições de fábrica do Pan/Crop.
/// The Pan/Crop's presets of a shape: their names and their widths over
/// their heights, on screen.
pub const PAN_CROP_SHAPES: [(&str, f64); 4] = [
    ("1:1 Square aspect ratio", 1.0),
    ("4:3 Standard TV aspect ratio", 4.0 / 3.0),
    ("16:9 Widescreen TV aspect ratio", 16.0 / 9.0),
    ("1.85:1 Academy aspect ratio", 1.85),
];

pub fn builtin_pan_crop() -> Vec<(&'static str, Transform)> {
    let d = Transform::default();
    vec![
        ("Zoom 125%", Transform { scale: 1.25, ..d }),
        ("Zoom 150%", Transform { scale: 1.5, ..d }),
        ("Zoom 200%", Transform { scale: 2.0, ..d }),
        (
            tr("Rotate 90°"),
            Transform {
                rotation_deg: 90.0,
                ..d
            },
        ),
        (
            tr("Rotate −90°"),
            Transform {
                rotation_deg: -90.0,
                ..d
            },
        ),
        (tr("Mirror"), Transform { flip_h: true, ..d }),
        (tr("Upside Down"), Transform { flip_v: true, ..d }),
    ]
}

impl Presets {
    pub fn load() -> Presets {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self) -> Result<(), String> {
        let p = path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&p, json).map_err(|e| e.to_string())
    }

    pub fn user(&self, k: EffectKind) -> &[(String, Vec<f32>)] {
        self.effects.get(&key(k)).map_or(&[], Vec::as_slice)
    }

    pub fn add(&mut self, k: EffectKind, name: String, params: Vec<f32>) -> Result<(), String> {
        let list = self.effects.entry(key(k)).or_default();
        list.retain(|(n, _)| *n != name);
        list.push((name, params));
        self.save()
    }

    pub fn remove(&mut self, k: EffectKind, name: &str) -> Result<(), String> {
        if let Some(list) = self.effects.get_mut(&key(k)) {
            list.retain(|(n, _)| n != name);
        }
        self.save()
    }

    /// The presets the user saved for the OFX plug-in `id`.
    pub fn user_plugin(&self, id: &str) -> &[(String, PluginValues)] {
        self.plugins.get(id).map_or(&[], Vec::as_slice)
    }

    pub fn add_plugin(
        &mut self,
        id: &str,
        name: String,
        values: PluginValues,
    ) -> Result<(), String> {
        let list = self.plugins.entry(id.to_string()).or_default();
        list.retain(|(n, _)| *n != name);
        list.push((name, values));
        self.save()
    }

    pub fn remove_plugin(&mut self, id: &str, name: &str) -> Result<(), String> {
        if let Some(list) = self.plugins.get_mut(id) {
            list.retain(|(n, _)| n != name);
        }
        self.save()
    }

    /// The presets the user saved for the VST 3 effect `key` names.
    pub fn user_vst(&self, key: &str) -> &[(String, VstPreset)] {
        self.vst.get(key).map_or(&[], Vec::as_slice)
    }

    pub fn add_vst(&mut self, key: &str, name: String, preset: VstPreset) -> Result<(), String> {
        let list = self.vst.entry(key.to_string()).or_default();
        list.retain(|(n, _)| *n != name);
        list.push((name, preset));
        self.save()
    }

    pub fn remove_vst(&mut self, key: &str, name: &str) -> Result<(), String> {
        if let Some(list) = self.vst.get_mut(key) {
            list.retain(|(n, _)| n != name);
        }
        self.save()
    }

    pub fn user_pan_crop(&self) -> &[(String, Transform)] {
        &self.pan_crop
    }

    pub fn add_pan_crop(&mut self, name: String, tf: Transform) -> Result<(), String> {
        self.pan_crop.retain(|(n, _)| *n != name);
        self.pan_crop.push((name, tf));
        self.save()
    }

    pub fn remove_pan_crop(&mut self, name: &str) -> Result<(), String> {
        self.pan_crop.retain(|(n, _)| n != name);
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_presets_go_through_the_file_by_plugin() {
        let mut p = Presets::default();
        let values: PluginValues = [(
            "amount".to_string(),
            PluginParam {
                value: bv_timeline::PluginValue::Num(vec![0.5]),
                keys: Vec::new(),
            },
        )]
        .into();
        p.plugins
            .entry("org.example.Glow".into())
            .or_default()
            .push(("Soft".into(), values.clone()));
        let back: Presets =
            serde_json::from_str(&serde_json::to_string(&p).expect("json")).expect("read");
        assert_eq!(
            back.user_plugin("org.example.Glow"),
            [("Soft".to_string(), values)]
        );
        assert!(back.user_plugin("org.example.Other").is_empty());
        // A file from before plug-in presets reads too.
        let old: Presets = serde_json::from_str(r#"{"effects":{},"pan_crop":[]}"#).expect("read");
        assert!(old.user_plugin("org.example.Glow").is_empty());
    }

    /// A VST 3 effect's presets keep its values and states, by its
    /// module's file and its class.
    #[test]
    fn vst_presets_go_through_the_file_by_module_and_class() {
        let fx = bv_timeline::VstFx {
            path: PathBuf::from("C:/Plug-Ins/Gain.vst3"),
            class_id: "0123456789ABCDEF0123456789ABCDEF".into(),
            name: "Gain".into(),
            params: [(7u32, 0.25)].into(),
            state: vec![1, 2, 3, 250],
            controller_state: vec![9],
            ..Default::default()
        };
        let key = vst_key(&fx);
        assert_eq!(key, "Gain.vst3#0123456789ABCDEF0123456789ABCDEF");
        let mut p = Presets::default();
        p.vst
            .entry(key.clone())
            .or_default()
            .push(("Quiet".into(), VstPreset::of(&fx)));
        let back: Presets =
            serde_json::from_str(&serde_json::to_string(&p).expect("json")).expect("read");
        assert_eq!(
            back.user_vst(&key),
            [("Quiet".to_string(), VstPreset::of(&fx))]
        );
        assert!(back.user_vst("Other.vst3#").is_empty());
    }

    #[test]
    fn predefinicoes_de_fabrica_usam_parametros_validos() {
        for k in EffectKind::VIDEO.into_iter().chain(EffectKind::AUDIO) {
            let n = k.params().len();
            for (name, pairs) in builtin(k) {
                for (i, v) in pairs {
                    assert!(i < n, "{k:?} {name}: índice {i}");
                    let spec = k.params()[i];
                    assert!((spec.min..=spec.max).contains(&v), "{k:?} {name}: {v}");
                }
            }
        }
    }
}
