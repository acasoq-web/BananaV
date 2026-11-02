//! Efeitos, transições, fades e enquadramento (pan/crop).
//!
//! Os efeitos guardam seus parâmetros como uma lista de números; cada tipo de
//! efeito publica a descrição deles ([`ParamSpec`]). Assim a interface monta
//! os controles sem conhecer cada efeito, e o arquivo de projeto continua
//! legível quando um efeito ganha parâmetros novos (os que faltam assumem o
//! valor padrão).

use serde::{Deserialize, Serialize};

/// Como o valor de um parâmetro deve ser editado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    Slider,
    /// 0 ou 1.
    Toggle,
    /// Este e os dois parâmetros seguintes formam uma cor sRGB (R, G, B).
    Color,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamSpec {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub kind: ParamKind,
}

impl ParamSpec {
    /// Where `v` sits in the parameter's range, 0–1 (what FX automation's
    /// envelopes hold).
    pub fn to_unit(&self, v: f32) -> f32 {
        if self.max > self.min {
            ((v - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// The value at `u` (0–1) of the range: a switch is on from the middle
    /// up.
    pub fn from_unit(&self, u: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        match self.kind {
            ParamKind::Toggle if u >= 0.5 => self.max,
            ParamKind::Toggle => self.min,
            _ => self.min + u * (self.max - self.min),
        }
    }
}

const fn slider(name: &'static str, min: f32, max: f32, default: f32) -> ParamSpec {
    ParamSpec {
        name,
        min,
        max,
        default,
        kind: ParamKind::Slider,
    }
}

const fn toggle(name: &'static str, default: f32) -> ParamSpec {
    ParamSpec {
        name,
        min: 0.0,
        max: 1.0,
        default,
        kind: ParamKind::Toggle,
    }
}

const fn color(name: &'static str, default: f32) -> ParamSpec {
    ParamSpec {
        name,
        min: 0.0,
        max: 1.0,
        default,
        kind: ParamKind::Color,
    }
}

/// Efeitos disponíveis (vídeo e áudio).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectKind {
    BrightnessContrast,
    /// Correção primária de cor: exposição, contraste, saturação, temperatura,
    /// matiz e as rodas lift/gamma/gain. É o que o painel "Matização de cores" edita.
    ColorGrade,
    Levels,
    BlackAndWhite,
    Sepia,
    Invert,
    GaussianBlur,
    Sharpen,
    Vignette,
    Mirror,
    Pixelate,
    ChromaKey,
    Glow,
    // Áudio.
    Gain,
    Eq3,
    HighPass,
    LowPass,
    Compressor,
    Limiter,
    NoiseGate,
    Delay,
    Reverb,
    /// Up to eight bands of any of six shapes, each on both channels, one
    /// of them, the middle or the sides; the parameters are
    /// [`modern_eq`]'s.
    ModernEq,
    /// A plug-in's effect (OpenFX): what it is lives in [`Effect::plugin`].
    Plugin,
    /// A VST 3 plug-in's audio effect: what it is lives in [`Effect::vst`].
    Vst,
}

impl EffectKind {
    pub const AUDIO: [EffectKind; 10] = [
        EffectKind::Gain,
        EffectKind::Eq3,
        EffectKind::ModernEq,
        EffectKind::HighPass,
        EffectKind::LowPass,
        EffectKind::Compressor,
        EffectKind::Limiter,
        EffectKind::NoiseGate,
        EffectKind::Delay,
        EffectKind::Reverb,
    ];

    pub fn is_audio(self) -> bool {
        EffectKind::AUDIO.contains(&self) || self == EffectKind::Vst
    }

    /// The video effects offered: the Color Grading panel's. The others
    /// are not offered any more: BananaV's own plug-ins, VEGAS's effects
    /// with its controls, took their places (Pixelate, Unsharp Mask,
    /// Brightness and Contrast, Levels, Black and White, Sepia, Invert,
    /// Gaussian Blur, Vignette, Mirror, Chroma Keyer and Glow,
    /// `org.bananav.Pixelate` and so on); projects that have them keep
    /// them.
    pub const VIDEO: [EffectKind; 1] = [EffectKind::ColorGrade];

    pub fn name(self) -> &'static str {
        match self {
            EffectKind::BrightnessContrast => "Brightness and Contrast",
            EffectKind::ColorGrade => "Color Correction",
            EffectKind::Levels => "Levels",
            EffectKind::BlackAndWhite => "Black and White",
            EffectKind::Sepia => "Sepia",
            EffectKind::Invert => "Invert",
            EffectKind::GaussianBlur => "Gaussian Blur",
            EffectKind::Sharpen => "Sharpen",
            EffectKind::Vignette => "Vignette",
            EffectKind::Mirror => "Mirror",
            EffectKind::Pixelate => "Pixelate",
            EffectKind::ChromaKey => "Chroma key",
            EffectKind::Glow => "Glow",
            EffectKind::Gain => "Gain",
            EffectKind::Eq3 => "3-Band EQ",
            EffectKind::HighPass => "High-Pass",
            EffectKind::LowPass => "Low-Pass",
            EffectKind::Compressor => "Compressor",
            EffectKind::Limiter => "Limiter",
            EffectKind::NoiseGate => "Noise gate",
            EffectKind::Delay => "Delay (Echo)",
            EffectKind::Reverb => "Reverb",
            EffectKind::ModernEq => "Modern Equalizer",
            EffectKind::Plugin => "Plug-in",
            EffectKind::Vst => "VST 3 Plug-in",
        }
    }

    pub fn category(self) -> &'static str {
        match self {
            EffectKind::BrightnessContrast
            | EffectKind::ColorGrade
            | EffectKind::Levels
            | EffectKind::BlackAndWhite
            | EffectKind::Sepia
            | EffectKind::Invert => "Color",
            EffectKind::GaussianBlur | EffectKind::Sharpen | EffectKind::Glow => "Focus and Light",
            EffectKind::Vignette | EffectKind::Mirror | EffectKind::Pixelate => "Style",
            EffectKind::ChromaKey => "Compositing",
            EffectKind::Gain
            | EffectKind::Eq3
            | EffectKind::ModernEq
            | EffectKind::HighPass
            | EffectKind::LowPass => "Equalization",
            EffectKind::Compressor | EffectKind::Limiter | EffectKind::NoiseGate => "Dynamics",
            EffectKind::Delay | EffectKind::Reverb => "Ambience",
            EffectKind::Plugin => "Plug-ins",
            EffectKind::Vst => "VST3",
        }
    }

    pub fn params(self) -> &'static [ParamSpec] {
        match self {
            EffectKind::BrightnessContrast => {
                const P: &[ParamSpec] = &[
                    slider("Brightness", -1.0, 1.0, 0.0),
                    slider("Contrast", -1.0, 1.0, 0.0),
                ];
                P
            }
            EffectKind::ColorGrade => &COLOR_GRADE,
            EffectKind::Levels => {
                const P: &[ParamSpec] = &[
                    slider("Input Black", 0.0, 1.0, 0.0),
                    slider("Input White", 0.0, 1.0, 1.0),
                    slider("Gamma", 0.1, 5.0, 1.0),
                    slider("Output Black", 0.0, 1.0, 0.0),
                    slider("Output White", 0.0, 1.0, 1.0),
                ];
                P
            }
            EffectKind::BlackAndWhite | EffectKind::Sepia | EffectKind::Invert => {
                const P: &[ParamSpec] = &[slider("Intensity", 0.0, 1.0, 1.0)];
                P
            }
            EffectKind::GaussianBlur => {
                const P: &[ParamSpec] = &[slider("Radius (px)", 0.0, 100.0, 8.0)];
                P
            }
            EffectKind::Sharpen => {
                const P: &[ParamSpec] = &[
                    slider("Amount", 0.0, 3.0, 0.8),
                    slider("Radius (px)", 0.5, 10.0, 1.5),
                ];
                P
            }
            EffectKind::Vignette => {
                const P: &[ParamSpec] = &[
                    slider("Intensity", 0.0, 1.0, 0.5),
                    slider("Size", 0.0, 1.0, 0.6),
                    slider("Softness", 0.01, 1.0, 0.5),
                ];
                P
            }
            EffectKind::Mirror => {
                const P: &[ParamSpec] = &[toggle("Horizontal", 1.0), toggle("Vertical", 0.0)];
                P
            }
            EffectKind::Pixelate => {
                const P: &[ParamSpec] = &[slider("Block Size (px)", 1.0, 200.0, 24.0)];
                P
            }
            EffectKind::ChromaKey => {
                const P: &[ParamSpec] = &[
                    color("Key Color", 0.0),
                    slider("G", 0.0, 1.0, 1.0),
                    slider("B", 0.0, 1.0, 0.0),
                    slider("Tolerance", 0.0, 1.0, 0.3),
                    slider("Softness", 0.0, 1.0, 0.1),
                    slider("Spill Removal", 0.0, 1.0, 0.5),
                ];
                P
            }
            EffectKind::Glow => {
                const P: &[ParamSpec] = &[
                    slider("Threshold", 0.0, 1.0, 0.7),
                    slider("Intensity", 0.0, 3.0, 1.0),
                    slider("Radius (px)", 1.0, 100.0, 20.0),
                ];
                P
            }
            EffectKind::Gain => {
                const P: &[ParamSpec] = &[slider("Gain (dB)", -24.0, 24.0, 0.0)];
                P
            }
            EffectKind::Eq3 => {
                const P: &[ParamSpec] = &[
                    slider("Low (dB)", -15.0, 15.0, 0.0),
                    slider("Low Freq. (Hz)", 30.0, 500.0, 120.0),
                    slider("Mid (dB)", -15.0, 15.0, 0.0),
                    slider("Mid Freq. (Hz)", 200.0, 8000.0, 1000.0),
                    slider("Mid Width (Q)", 0.3, 6.0, 1.0),
                    slider("High (dB)", -15.0, 15.0, 0.0),
                    slider("High Freq. (Hz)", 2000.0, 16000.0, 6000.0),
                ];
                P
            }
            EffectKind::HighPass => {
                const P: &[ParamSpec] = &[
                    slider("Frequency (Hz)", 20.0, 2000.0, 80.0),
                    slider("Resonance (Q)", 0.5, 4.0, 0.707),
                ];
                P
            }
            EffectKind::LowPass => {
                const P: &[ParamSpec] = &[
                    slider("Frequency (Hz)", 300.0, 20000.0, 12000.0),
                    slider("Resonance (Q)", 0.5, 4.0, 0.707),
                ];
                P
            }
            EffectKind::Compressor => {
                const P: &[ParamSpec] = &[
                    slider("Threshold (dB)", -60.0, 0.0, -18.0),
                    slider("Ratio", 1.0, 20.0, 4.0),
                    slider("Attack (ms)", 0.1, 200.0, 10.0),
                    slider("Release (ms)", 10.0, 2000.0, 150.0),
                    slider("Output Gain (dB)", 0.0, 24.0, 0.0),
                ];
                P
            }
            EffectKind::Limiter => {
                const P: &[ParamSpec] = &[
                    slider("Ceiling (dB)", -24.0, 0.0, -1.0),
                    slider("Release (ms)", 1.0, 1000.0, 50.0),
                ];
                P
            }
            EffectKind::NoiseGate => {
                const P: &[ParamSpec] = &[
                    slider("Threshold (dB)", -90.0, 0.0, -50.0),
                    slider("Release (ms)", 5.0, 1000.0, 100.0),
                ];
                P
            }
            EffectKind::Delay => {
                const P: &[ParamSpec] = &[
                    slider("Time (ms)", 10.0, 2000.0, 350.0),
                    slider("Feedback", 0.0, 0.95, 0.35),
                    slider("Mix", 0.0, 1.0, 0.3),
                ];
                P
            }
            EffectKind::Reverb => {
                const P: &[ParamSpec] = &[
                    slider("Room Size", 0.0, 1.0, 0.5),
                    slider("Damping", 0.0, 1.0, 0.5),
                    slider("Mix", 0.0, 1.0, 0.25),
                ];
                P
            }
            EffectKind::ModernEq => &MODERN_EQ,
            // Its plug-in describes them.
            EffectKind::Plugin | EffectKind::Vst => &[],
        }
    }
}

/// [`EffectKind::ModernEq`]'s parameters: four global ones, then eight a
/// band ([`modern_eq::PER_BAND`]).
pub mod modern_eq {
    pub const GLOBAL_SCALE: usize = 0;
    pub const MONO_BELOW: usize = 1;
    pub const STEREO_WIDTH: usize = 2;
    pub const OUTPUT_GAIN: usize = 3;
    pub const GLOBALS: usize = 4;
    pub const BANDS: usize = 8;
    pub const PER_BAND: usize = 8;
    /// A band's, from its first: in use (added), on, type (High Pass, Low
    /// Pass, Low Shelving, High Shelving, Band Pass, Peaking), frequency,
    /// gain, Q, slope (dB an octave) and routing (Stereo, Left, Right,
    /// Mid, Side).
    pub const USED: usize = 0;
    pub const ON: usize = 1;
    pub const TYPE: usize = 2;
    pub const FREQUENCY: usize = 3;
    pub const GAIN: usize = 4;
    pub const Q: usize = 5;
    pub const SLOPE: usize = 6;
    pub const ROUTING: usize = 7;

    /// Where band `b`'s parameter `p` is.
    pub const fn at(b: usize, p: usize) -> usize {
        GLOBALS + PER_BAND * b + p
    }

    /// Oversampling (Off, 2×, 4×, 8×: 0 to 3), after the bands.
    pub const OVERSAMPLING: usize = at(BANDS, 0);
    /// How many parameters it has.
    pub const COUNT: usize = OVERSAMPLING + 1;
}

/// A Modern Equalizer band's parameters; where a band at rest sits.
macro_rules! eq_band {
    ($n:literal, $freq:expr) => {
        [
            toggle(concat!("Band ", $n), 0.0),
            toggle(concat!("Band ", $n, " On"), 1.0),
            slider(concat!("Band ", $n, " Type"), 0.0, 5.0, 5.0),
            slider(
                concat!("Band ", $n, " Frequency (Hz)"),
                20.0,
                20_000.0,
                $freq,
            ),
            slider(concat!("Band ", $n, " Gain (dB)"), -30.0, 30.0, 0.0),
            slider(concat!("Band ", $n, " Q"), 0.1, 30.0, 1.0),
            slider(concat!("Band ", $n, " Slope (dB/oct)"), 6.0, 36.0, 12.0),
            slider(concat!("Band ", $n, " Routing"), 0.0, 4.0, 0.0),
        ]
    };
}

const MODERN_EQ_GLOBALS: [ParamSpec; modern_eq::GLOBALS] = [
    slider("Global Scale (%)", -200.0, 200.0, 100.0),
    slider("Mono Below (Hz)", 0.0, 1000.0, 0.0),
    slider("Stereo Width (%)", 0.0, 200.0, 100.0),
    slider("Output Gain (dB)", -24.0, 24.0, 0.0),
];

const MODERN_EQ_BANDS: [[ParamSpec; modern_eq::PER_BAND]; modern_eq::BANDS] = [
    eq_band!("1", 60.0),
    eq_band!("2", 150.0),
    eq_band!("3", 400.0),
    eq_band!("4", 1000.0),
    eq_band!("5", 2500.0),
    eq_band!("6", 5000.0),
    eq_band!("7", 10_000.0),
    eq_band!("8", 16_000.0),
];

const MODERN_EQ: [ParamSpec; modern_eq::COUNT] = {
    let mut out = [MODERN_EQ_GLOBALS[0]; modern_eq::COUNT];
    out[modern_eq::OVERSAMPLING] = slider("Oversampling", 0.0, 3.0, 0.0);
    let mut i = 0;
    while i < modern_eq::GLOBALS {
        out[i] = MODERN_EQ_GLOBALS[i];
        i += 1;
    }
    let mut b = 0;
    while b < modern_eq::BANDS {
        let mut p = 0;
        while p < modern_eq::PER_BAND {
            out[modern_eq::at(b, p)] = MODERN_EQ_BANDS[b][p];
            p += 1;
        }
        b += 1;
    }
    out
};

/// Índices dos parâmetros de [`EffectKind::ColorGrade`].
pub mod grade {
    pub const EXPOSURE: usize = 0;
    pub const CONTRAST: usize = 1;
    pub const SATURATION: usize = 2;
    pub const TEMPERATURE: usize = 3;
    pub const TINT: usize = 4;
    pub const LIFT: usize = 5;
    pub const GAMMA: usize = 8;
    pub const GAIN: usize = 11;
}

const COLOR_GRADE: [ParamSpec; 14] = [
    slider("Exposure (EV)", -4.0, 4.0, 0.0),
    slider("Contrast", -1.0, 1.0, 0.0),
    slider("Saturation", 0.0, 2.0, 1.0),
    slider("Temperature", -1.0, 1.0, 0.0),
    slider("Tint", -1.0, 1.0, 0.0),
    slider("Lift R", -0.5, 0.5, 0.0),
    slider("Lift G", -0.5, 0.5, 0.0),
    slider("Lift B", -0.5, 0.5, 0.0),
    slider("Gamma R", -0.5, 0.5, 0.0),
    slider("Gamma G", -0.5, 0.5, 0.0),
    slider("Gamma B", -0.5, 0.5, 0.0),
    slider("Gain R", -0.5, 0.5, 0.0),
    slider("Gain G", -0.5, 0.5, 0.0),
    slider("Gain B", -0.5, 0.5, 0.0),
];

/// An effect applied to an event, a track, a media file or the output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub kind: EffectKind,
    pub enabled: bool,
    pub params: Vec<f32>,
    /// A plug-in's effect (`kind` is [`EffectKind::Plugin`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<Box<PluginFx>>,
    /// A VST 3 plug-in's effect (`kind` is [`EffectKind::Vst`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vst: Option<Box<VstFx>>,
    /// Its number in its chain for FX automation's envelopes
    /// ([`crate::fx_automation`]): 0 until it is first automated.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: u32,
    /// Bypass FX Automation: its envelopes are left out, its own values
    /// hold.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub automation_bypassed: bool,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

impl Effect {
    pub fn new(kind: EffectKind) -> Effect {
        Effect {
            kind,
            enabled: true,
            params: kind.params().iter().map(|p| p.default).collect(),
            plugin: None,
            vst: None,
            id: 0,
            automation_bypassed: false,
        }
    }

    /// A plug-in's effect.
    pub fn plugin(fx: PluginFx) -> Effect {
        Effect {
            kind: EffectKind::Plugin,
            enabled: true,
            params: Vec::new(),
            plugin: Some(Box::new(fx)),
            vst: None,
            id: 0,
            automation_bypassed: false,
        }
    }

    /// A VST 3 plug-in's effect.
    pub fn vst(fx: VstFx) -> Effect {
        Effect {
            kind: EffectKind::Vst,
            enabled: true,
            params: Vec::new(),
            plugin: None,
            vst: Some(Box::new(fx)),
            id: 0,
            automation_bypassed: false,
        }
    }

    /// The name to show: the plug-in's, or the kind's (English; the
    /// interface translates the kinds' names).
    pub fn name(&self) -> &str {
        match (&self.plugin, &self.vst) {
            (Some(p), _) => &p.name,
            (_, Some(v)) => &v.name,
            _ => self.kind.name(),
        }
    }

    /// The effect at its defaults: a plug-in's keeps no value (its plug-in
    /// gives them).
    pub fn reset(&self) -> Effect {
        let mut fresh = match (&self.plugin, &self.vst) {
            (Some(p), _) => Effect::plugin(PluginFx {
                params: Default::default(),
                ..(**p).clone()
            }),
            (_, Some(v)) => Effect::vst(VstFx {
                path: v.path.clone(),
                class_id: v.class_id.clone(),
                name: v.name.clone(),
                vendor: v.vendor.clone(),
                ..VstFx::default()
            }),
            _ => Effect::new(self.kind),
        };
        fresh.enabled = self.enabled;
        // Its automation stays its own.
        fresh.id = self.id;
        fresh.automation_bypassed = self.automation_bypassed;
        fresh
    }

    /// Valor do parâmetro `i`, ou o padrão se o arquivo não tiver.
    pub fn param(&self, i: usize) -> f32 {
        self.params
            .get(i)
            .copied()
            .or_else(|| self.kind.params().get(i).map(|p| p.default))
            .unwrap_or(0.0)
    }

    pub fn set_param(&mut self, i: usize, v: f32) {
        let specs = self.kind.params();
        if self.params.len() < specs.len() {
            self.params
                .extend(specs[self.params.len()..].iter().map(|p| p.default));
        }
        if let (Some(spec), Some(slot)) = (specs.get(i), self.params.get_mut(i)) {
            *slot = v.clamp(spec.min, spec.max);
        }
    }

    /// `true` se todos os parâmetros estão no padrão.
    pub fn is_default(&self) -> bool {
        // A VST 3 plug-in's: as it starts, nothing of its own.
        if let Some(v) = &self.vst {
            return v.params.is_empty() && v.state.is_empty();
        }
        self.kind
            .params()
            .iter()
            .enumerate()
            .all(|(i, p)| (self.param(i) - p.default).abs() < 1e-6)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    #[default]
    Left,
    Right,
    Up,
    Down,
}

/// Como um clipe entra sobre o anterior (ou sobre o fundo, num fade-in).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Transition {
    #[default]
    Dissolve,
    FadeThroughBlack,
    Wipe(Direction),
    Push(Direction),
    Slide(Direction),
    Iris,
    Clock,
    Zoom,
}

impl Transition {
    pub const ALL: [Transition; 15] = [
        Transition::Dissolve,
        Transition::FadeThroughBlack,
        Transition::Wipe(Direction::Left),
        Transition::Wipe(Direction::Right),
        Transition::Wipe(Direction::Up),
        Transition::Wipe(Direction::Down),
        Transition::Push(Direction::Left),
        Transition::Push(Direction::Right),
        Transition::Push(Direction::Up),
        Transition::Push(Direction::Down),
        Transition::Slide(Direction::Left),
        Transition::Slide(Direction::Right),
        Transition::Iris,
        Transition::Clock,
        Transition::Zoom,
    ];

    /// English name (the interface translates it). Every name is spelled
    /// out, so that the catalogs' coverage check can see it.
    pub fn name(self) -> &'static str {
        use Direction::*;
        match self {
            Transition::Dissolve => "Cross Dissolve",
            Transition::FadeThroughBlack => "Fade Through Black",
            Transition::Wipe(Left) => "Wipe left",
            Transition::Wipe(Right) => "Wipe right",
            Transition::Wipe(Up) => "Wipe up",
            Transition::Wipe(Down) => "Wipe down",
            Transition::Push(Left) => "Push left",
            Transition::Push(Right) => "Push right",
            Transition::Push(Up) => "Push up",
            Transition::Push(Down) => "Push down",
            Transition::Slide(Left) => "Slide left",
            Transition::Slide(Right) => "Slide right",
            Transition::Slide(Up) => "Slide up",
            Transition::Slide(Down) => "Slide down",
            Transition::Iris => "Iris",
            Transition::Clock => "Clock Wipe",
            Transition::Zoom => "Zoom",
        }
    }
}

/// A VST 3 plug-in's effect: which (its module — a bundle or a library —
/// and its class), its name and maker when applied (shown while it is
/// missing), its parameters' values (normalized, 0–1, by identifier) and
/// its states as the plug-in wrote them (the component's, the
/// controller's own). The states hold the parameters too: the values are
/// what the processing follows while the states stay the same.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VstFx {
    pub path: std::path::PathBuf,
    /// The class's identifier (32 hexadecimal digits); empty for the
    /// module's first effect.
    #[serde(default)]
    pub class_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub vendor: String,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub params: std::collections::BTreeMap<u32, f64>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        with = "bv_core::base64::text"
    )]
    pub state: Vec<u8>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        with = "bv_core::base64::text"
    )]
    pub controller_state: Vec<u8>,
    /// Its automated parameters' names, as the plug-in titles them (FX
    /// Automation's envelopes are named after them).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub param_names: std::collections::BTreeMap<u32, String>,
}

/// A plug-in's effect (OpenFX): which plug-in, and the values its
/// parameters have here. Parameters without a value here have their
/// plug-in's default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginFx {
    /// The plug-in's identifier ("com.example.Blur").
    pub id: String,
    /// Its version when applied.
    #[serde(default)]
    pub version: [u32; 2],
    /// Its name when applied (shown while the plug-in is missing).
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub params: std::collections::BTreeMap<String, PluginParam>,
}

impl PluginFx {
    /// Brings values an older BananaV saved up to date, on a project whose
    /// frame is `frame` big: BananaV's own Crop counted its Location in
    /// the frame's units before 2.0 (now 0–1 of the frame, VEGAS's way).
    pub fn upgrade(&mut self, frame: (u32, u32)) {
        if self.id == "org.bananav.Crop" && self.version < [2, 0] {
            let (w, h) = (f64::from(frame.0.max(1)), f64::from(frame.1.max(1)));
            let part = |v: &mut PluginValue| {
                if let PluginValue::Num(n) = v
                    && n.len() >= 2
                {
                    n[0] /= w;
                    n[1] /= h;
                }
            };
            if let Some(p) = self.params.get_mut("location") {
                part(&mut p.value);
                for k in &mut p.keys {
                    part(&mut k.value);
                }
            }
            self.version = [2, 0];
        }
        // The Color Corrector's and its Secondary's Offset counted the whole
        // range before 1.1, 8-bit steps since (VEGAS's).
        let corrector = matches!(
            self.id.as_str(),
            "org.bananav.ColorCorrector" | "org.bananav.ColorCorrectorSecondary"
        );
        if corrector && self.version < [1, 1] {
            let steps = |v: &mut PluginValue| {
                if let PluginValue::Num(n) = v {
                    for x in n {
                        *x = (*x * 255.0).clamp(-254.0, 254.0);
                    }
                }
            };
            if let Some(p) = self.params.get_mut("offset") {
                steps(&mut p.value);
                for k in &mut p.keys {
                    steps(&mut k.value);
                }
            }
            self.version = [1, 1];
        }
        // Quick Blur blurred all the way before VEGAS's Blend amount, whose
        // default leaves the picture alone: it keeps blurring all the way.
        if self.id == "org.bananav.QuickBlur" && self.version < [2, 0] {
            self.params
                .entry("blendAmount".to_string())
                .or_insert(PluginParam {
                    value: PluginValue::Num(vec![1.0]),
                    keys: Vec::new(),
                });
            self.version = [2, 0];
        }
        // Bézier Masking took VEGAS's panel in 2.0: Type lists the oval
        // first, Feather Style goes In, Out, Both (Both at first), Feather
        // counts percent of the frame's shorter side, Blend brings back
        // what the masks leave out, and the Edit Tool lists Add before
        // Delete.
        if self.id == "org.bananav.BezierMasking" && self.version < [2, 0] {
            let (w, h) = (f64::from(frame.0.max(1)), f64::from(frame.1.max(1)));
            let each = |p: &mut PluginParam, f: &dyn Fn(f64) -> f64| {
                let one = |v: &mut PluginValue| {
                    if let PluginValue::Num(n) = v {
                        for x in n {
                            *x = f(*x);
                        }
                    }
                };
                one(&mut p.value);
                for k in &mut p.keys {
                    one(&mut k.value);
                }
            };
            let swap = |a: f64, b: f64| {
                move |x: f64| {
                    if x == a {
                        b
                    } else if x == b {
                        a
                    } else {
                        x
                    }
                }
            };
            for n in 1..=5 {
                if let Some(p) = self.params.get_mut(&format!("type{n}")) {
                    each(p, &swap(0.0, 1.0));
                }
                if let Some(p) = self.params.get_mut(&format!("editTool{n}")) {
                    each(p, &swap(1.0, 2.0));
                }
                // Out was the first style and the default.
                let style = self
                    .params
                    .entry(format!("featherStyle{n}"))
                    .or_insert(PluginParam {
                        value: PluginValue::Num(vec![0.0]),
                        keys: Vec::new(),
                    });
                each(style, &swap(0.0, 1.0));
                if let Some(p) = self.params.get_mut(&format!("feather{n}")) {
                    each(p, &|x| (x * 100.0 * w / w.min(h)).clamp(0.0, 50.0));
                }
            }
            if let Some(p) = self.params.get_mut("blend") {
                each(p, &|x| 1.0 - x);
            }
            self.version = [2, 0];
        }
        // Effects redone as VEGAS's whose values of before meant other
        // things (Broadcast Colors' were 0–1 of the picture, VEGAS's are
        // IRE): they start again from their defaults.
        const REDONE: &[(&str, [u32; 2])] = &[
            ("org.bananav.BroadcastColors", [2, 0]),
            ("org.bananav.MaskGenerator", [2, 0]),
            ("org.bananav.Median", [2, 0]),
            ("org.bananav.MinMax", [2, 0]),
            ("org.bananav.LabAdjust", [2, 0]),
            ("org.bananav.ConvolutionKernel", [2, 0]),
            ("org.bananav.LinearBlur", [2, 0]),
            ("org.bananav.RadialBlur", [2, 0]),
            ("org.bananav.Spherize", [2, 0]),
            ("org.bananav.Swirl", [2, 0]),
            ("org.bananav.PinchPunch", [2, 0]),
            ("org.bananav.OffsetWrap", [2, 0]),
            ("org.bananav.Wave", [2, 0]),
            ("org.bananav.Deform", [2, 0]),
            ("org.bananav.TinyPlanet", [2, 0]),
            ("org.bananav.RadialPixelate", [2, 0]),
            ("org.bananav.Checkerboard", [2, 0]),
            ("org.bananav.TestPattern", [2, 0]),
            ("org.bananav.ChromaKeyer", [2, 0]),
            ("org.bananav.BumpMap", [2, 0]),
            ("org.bananav.FillLight", [2, 0]),
            ("org.bananav.DisplacementMap", [2, 0]),
            ("org.bananav.HeightMap", [2, 0]),
            ("org.bananav.LinearWipe", [2, 0]),
            ("org.bananav.ClockWipe", [2, 0]),
            ("org.bananav.Iris", [2, 0]),
            ("org.bananav.StarWipe", [2, 0]),
            ("org.bananav.Flash", [2, 0]),
            ("org.bananav.Dissolve", [2, 0]),
            ("org.bananav.CrossEffect", [2, 0]),
            ("org.bananav.BarnDoor", [2, 0]),
            ("org.bananav.Push", [2, 0]),
            ("org.bananav.Slide", [2, 0]),
            ("org.bananav.Squeeze", [2, 0]),
            ("org.bananav.Split", [2, 0]),
            ("org.bananav.Swap", [2, 0]),
            ("org.bananav.Zoom", [2, 0]),
            ("org.bananav.Spiral", [2, 0]),
            ("org.bananav.VenetianBlinds", [2, 0]),
            ("org.bananav.Portals", [2, 0]),
            ("org.bananav.GradientWipe", [2, 0]),
            ("org.bananav.PageLoop", [2, 0]),
            ("org.bananav.PagePeel", [2, 0]),
            ("org.bananav.PageRoll", [2, 0]),
            ("org.bananav.Blinds3D", [2, 0]),
            ("org.bananav.Cascade3D", [2, 0]),
            ("org.bananav.FlyInOut3D", [2, 0]),
            ("org.bananav.Shuffle3D", [2, 0]),
            ("org.bananav.FilmGrain", [2, 0]),
            ("org.bananav.LightRays", [2, 0]),
            ("org.bananav.GradientMap", [2, 0]),
            ("org.bananav.FilmEffects", [2, 0]),
            ("org.bananav.TvSimulator", [2, 0]),
            ("org.bananav.Newsprint", [2, 0]),
            ("org.bananav.NoiseTexture", [2, 0]),
            ("org.bananav.LensFlare", [2, 0]),
            ("org.bananav.Looks", [2, 0]),
            ("org.bananav.ColorMatch", [2, 0]),
            ("org.bananav.Defocus", [2, 0]),
            ("org.bananav.Starburst", [2, 0]),
            ("org.bananav.SoftContrast", [2, 0]),
            ("org.bananav.Glint", [2, 0]),
            ("org.bananav.MotionBlur", [2, 0]),
            ("org.bananav.SlowMotion", [2, 0]),
            ("org.bananav.WarpFlow", [2, 0]),
            ("org.bananav.FlickerControl", [2, 0]),
            ("org.bananav.Denoise", [2, 0]),
            ("org.bananav.GLTransition", [2, 0]),
            ("org.bananav.LensCorrection", [2, 0]),
            ("org.bananav.MeshWarp", [2, 0]),
            ("org.bananav.VideoStabilization", [2, 0]),
            ("org.bananav.VideoStabilization360", [2, 0]),
            ("org.bananav.DualFishEyeStitching", [2, 0]),
        ];
        for &(id, version) in REDONE {
            if self.id == id && self.version < version {
                self.params.clear();
                self.version = version;
            }
        }
    }
}

/// A plug-in parameter's value over time: `value` while it has no keys.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginParam {
    pub value: PluginValue,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<PluginKey>,
}

/// A plug-in parameter's value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PluginValue {
    /// Numbers: one, a point's coordinates, a color's channels (whole
    /// numbers for integers, booleans and choices).
    Num(Vec<f64>),
    Text(String),
    Bytes(Vec<u8>),
    /// Curves of points (position, value and the handles the user set).
    Curves(Vec<Vec<bv_core::curve::CurvePoint>>),
}

/// A plug-in parameter's key: its value `at` (from its owner's start).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginKey {
    pub at: bv_core::Tick,
    pub value: PluginValue,
    #[serde(default)]
    pub curve: KeyCurve,
    /// A Bézier keyframe's handles, one for each number (none: they follow
    /// the keyframes around it).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handles: Vec<KeyHandles>,
}

impl PluginKey {
    pub fn new(at: bv_core::Tick, value: PluginValue, curve: KeyCurve) -> PluginKey {
        PluginKey {
            at,
            value,
            curve,
            handles: Vec::new(),
        }
    }
}

/// A Bézier keyframe's handles for one of its numbers: where each is from
/// the keyframe, in time and value — `before` toward the keyframe before
/// (its time at most 0), `after` toward the next (at least 0).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KeyHandles {
    pub before: (bv_core::Tick, f64),
    pub after: (bv_core::Tick, f64),
}

/// A fade's curve: how the gain or the opacity goes over the fade.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FadeCurve {
    Linear,
    /// Rises fast, then settles.
    Fast,
    /// Starts slowly.
    Slow,
    /// In sound the sine, of constant power in a crossfade; in video an S
    /// (`scene::video_curve`).
    #[default]
    Smooth,
    /// The S turned around, as VEGAS's picture of it: fast at both ends,
    /// slow in the middle (the smoothstep's inverse).
    Sharp,
}

impl FadeCurve {
    /// A fade-in's gain at `p` ∈ [0, 1] of its way (a fade-out's is the
    /// gain at the part of it still to go).
    pub fn eval(self, p: f32) -> f32 {
        let p = p.clamp(0.0, 1.0);
        match self {
            FadeCurve::Linear => p,
            FadeCurve::Fast => 1.0 - (1.0 - p) * (1.0 - p),
            FadeCurve::Slow => p * p,
            FadeCurve::Smooth => (p * std::f32::consts::FRAC_PI_2).sin(),
            // Exact at the ends.
            FadeCurve::Sharp if p <= 0.0 || p >= 1.0 => p,
            FadeCurve::Sharp => 0.5 - ((1.0 - 2.0 * p).asin() / 3.0).sin(),
        }
    }

    /// The curve a fade-out of this type follows, read from its end. VEGAS
    /// names a fade-out by how it begins, as an envelope going down — a
    /// Fast one drops at once, a Slow one late (its crossfades' pictures) —
    /// so a Fast fade-out is the Slow fade-in turned around in time, and a
    /// Fast fade-in with a Slow fade-out draws a dome.
    pub fn out_shape(self) -> FadeCurve {
        match self {
            FadeCurve::Fast => FadeCurve::Slow,
            FadeCurve::Slow => FadeCurve::Fast,
            c => c,
        }
    }

    /// A fade-out's gain where `p` ∈ [0, 1] of it is still to go.
    pub fn eval_out(self, p: f32) -> f32 {
        self.out_shape().eval(p)
    }
}

/// Enquadramento do clipe ("Pan/Crop"): posição, escala, rotação e corte.
/// Posição em frações do quadro do projeto (0 = centralizado).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    /// 1 = a mídia encaixada no quadro (letterbox).
    pub scale: f32,
    pub rotation_deg: f32,
    pub crop_left: f32,
    pub crop_right: f32,
    pub crop_top: f32,
    pub crop_bottom: f32,
    /// Espelha a mídia na horizontal / vertical.
    pub flip_h: bool,
    pub flip_v: bool,
    /// Escala vertical relativa à horizontal (1 = proporção da mídia).
    pub stretch: f32,
    // What the Pan/Crop window's box is besides the framing above, which
    // the renderers draw: they say how the box maps to it.
    /// Rotation › X Center and Y Center, as VEGAS's: where the box turns,
    /// from its middle in its own axes, as parts of its width and height
    /// ((0, 0): its middle).
    #[serde(skip_serializing_if = "is_origin")]
    pub pivot: [f32; 2],
    /// Source › Maintain aspect ratio: the box's picture keeps its
    /// proportions in the frame, what is around the box filling the rest
    /// (off: stretched to fill it). `None` in projects saved before it was
    /// a setting: on unless they stretch the picture ([`Self::keeps_aspect`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_aspect: Option<bool>,
    /// Source › Stretch to fill frame: the box's picture fills the frame
    /// (off: a pixel of the media is a pixel of the frame).
    #[serde(skip_serializing_if = "is_yes")]
    pub fill_frame: bool,
    /// The box within what the frame shows, as parts of its width and
    /// height ((1, 1): all of it; less where the box keeps its picture's
    /// proportions in a frame of another shape, or does not fill it).
    #[serde(skip_serializing_if = "is_whole")]
    pub box_size: [f32; 2],
}

fn is_origin(v: &[f32; 2]) -> bool {
    *v == [0.0; 2]
}

fn is_yes(v: &bool) -> bool {
    *v
}

fn is_whole(v: &[f32; 2]) -> bool {
    *v == [1.0; 2]
}

impl Default for Transform {
    fn default() -> Transform {
        Transform {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            rotation_deg: 0.0,
            crop_left: 0.0,
            crop_right: 0.0,
            crop_top: 0.0,
            crop_bottom: 0.0,
            flip_h: false,
            flip_v: false,
            stretch: 1.0,
            pivot: [0.0; 2],
            keep_aspect: None,
            fill_frame: true,
            box_size: [1.0; 2],
        }
    }
}

impl Transform {
    /// Whether it draws the media as it is: the box's own settings, which
    /// change nothing the renderers draw, aside.
    pub fn is_identity(&self) -> bool {
        Transform {
            pivot: [0.0; 2],
            keep_aspect: None,
            fill_frame: true,
            box_size: [1.0; 2],
            ..*self
        } == Transform::default()
    }

    /// Whether the box's picture keeps its proportions (Source › Maintain
    /// aspect ratio): as set, else unless the framing stretches it.
    pub fn keeps_aspect(&self) -> bool {
        self.keep_aspect
            .unwrap_or((self.stretch - 1.0).abs() < 1e-4)
    }

    /// Mistura entre `self` (t = 0) e `other` (t = 1). Espelhamentos
    /// trocam no meio do caminho.
    pub fn lerp(&self, other: &Transform, t: f32) -> Transform {
        let l = |a: f32, b: f32| a + (b - a) * t;
        Transform {
            x: l(self.x, other.x),
            y: l(self.y, other.y),
            // Escala interpolada em log: zoom com velocidade constante.
            scale: (self.scale.max(1e-4).ln()
                + (other.scale.max(1e-4).ln() - self.scale.max(1e-4).ln()) * t)
                .exp(),
            rotation_deg: l(self.rotation_deg, other.rotation_deg),
            crop_left: l(self.crop_left, other.crop_left),
            crop_right: l(self.crop_right, other.crop_right),
            crop_top: l(self.crop_top, other.crop_top),
            crop_bottom: l(self.crop_bottom, other.crop_bottom),
            flip_h: if t < 0.5 { self.flip_h } else { other.flip_h },
            flip_v: if t < 0.5 { self.flip_v } else { other.flip_v },
            stretch: l(self.stretch, other.stretch),
            pivot: [
                l(self.pivot[0], other.pivot[0]),
                l(self.pivot[1], other.pivot[1]),
            ],
            keep_aspect: if t < 0.5 {
                self.keep_aspect
            } else {
                other.keep_aspect
            },
            fill_frame: if t < 0.5 {
                self.fill_frame
            } else {
                other.fill_frame
            },
            box_size: [
                l(self.box_size[0], other.box_size[0]),
                l(self.box_size[1], other.box_size[1]),
            ],
        }
    }
}

/// Como a animação anda de um quadro-chave até o seguinte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyCurve {
    Linear,
    /// Changes fast at first, then settles (as a Fast fade).
    Fast,
    /// Changes slowly at first (as a Slow fade).
    Slow,
    /// Acelera e desacelera (smoothstep).
    #[default]
    Smooth,
    /// As a Sharp fade.
    Sharp,
    /// Mantém o valor até o próximo quadro-chave.
    Hold,
    /// A Bézier curve through the keyframes' handles
    /// (`PluginKey::handles`), which move together, equal and opposite
    /// (VEGAS's "Bézier curve (locked tangents)"; plug-in parameters'
    /// keyframes)...
    Bezier,
    /// ...or apart ("split tangents").
    BezierSplit,
}

impl KeyCurve {
    /// VEGAS's fade types, in its menus' order.
    pub const ALL: [KeyCurve; 6] = [
        KeyCurve::Linear,
        KeyCurve::Fast,
        KeyCurve::Slow,
        KeyCurve::Smooth,
        KeyCurve::Sharp,
        KeyCurve::Hold,
    ];

    pub fn name(self) -> &'static str {
        match self {
            KeyCurve::Linear => "Linear",
            KeyCurve::Fast => "Fast",
            KeyCurve::Slow => "Slow",
            KeyCurve::Smooth => "Smooth",
            KeyCurve::Sharp => "Sharp",
            KeyCurve::Hold => "Hold",
            KeyCurve::Bezier => "Bézier curve (locked tangents)",
            KeyCurve::BezierSplit => "Bézier curve (split tangents)",
        }
    }

    pub fn is_bezier(self) -> bool {
        matches!(self, KeyCurve::Bezier | KeyCurve::BezierSplit)
    }

    pub fn eval(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            KeyCurve::Linear => t,
            KeyCurve::Fast => FadeCurve::Fast.eval(t),
            KeyCurve::Slow => FadeCurve::Slow.eval(t),
            KeyCurve::Smooth => t * t * (3.0 - 2.0 * t),
            KeyCurve::Sharp => FadeCurve::Sharp.eval(t),
            KeyCurve::Hold => 0.0,
            // Without handles, a straight line.
            KeyCurve::Bezier | KeyCurve::BezierSplit => t,
        }
    }
}

/// Quadro-chave do enquadramento, a `at` do início do clipe.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransformKey {
    pub at: bv_core::Tick,
    pub value: Transform,
    #[serde(default)]
    pub curve: KeyCurve,
    /// How much the path of the position curves through this keyframe,
    /// 0–100 (0: straight lines between keyframes), as VEGAS's
    /// "Smoothness".
    #[serde(default)]
    pub smoothness: f32,
}

impl TransformKey {
    pub fn new(at: bv_core::Tick, value: Transform, curve: KeyCurve) -> TransformKey {
        TransformKey {
            at,
            value,
            curve,
            smoothness: 0.0,
        }
    }
}

/// Valor animado em `offset` (a partir do início do clipe).
pub fn transform_at(keys: &[TransformKey], base: &Transform, offset: bv_core::Tick) -> Transform {
    let Some(first) = keys.first() else {
        return *base;
    };
    if offset <= first.at {
        return first.value;
    }
    for (i, w) in keys.windows(2).enumerate() {
        let (a, b) = (&w[0], &w[1]);
        if offset < b.at {
            let span = (b.at - a.at).0.max(1) as f64;
            let t = ((offset - a.at).0 as f64 / span) as f32;
            let u = a.curve.eval(t);
            let mut v = a.value.lerp(&b.value, u);
            if a.smoothness > 0.0 || b.smoothness > 0.0 {
                let prev = i.checked_sub(1).map(|j| &keys[j]);
                (v.x, v.y) = smooth_path(prev, a, b, keys.get(i + 2), u);
            }
            return v;
        }
    }
    keys.last().map_or(*base, |k| k.value)
}

/// The position at `u` (0–1) of the way from keyframe `a` to `b`: a cubic
/// whose tangents at the keyframes lean from the straight line toward the
/// direction through their neighbours (Catmull-Rom) as far as their
/// smoothness says; straight at zero.
fn smooth_path(
    prev: Option<&TransformKey>,
    a: &TransformKey,
    b: &TransformKey,
    next: Option<&TransformKey>,
    u: f32,
) -> (f32, f32) {
    let p = |k: &TransformKey| (k.value.x, k.value.y);
    let (p0, p1) = (p(a), p(b));
    let chord = (p1.0 - p0.0, p1.1 - p0.1);
    let tangent = |before: (f32, f32), after: (f32, f32), smoothness: f32| {
        let s = (smoothness / 100.0).clamp(0.0, 1.0);
        let through = ((after.0 - before.0) / 2.0, (after.1 - before.1) / 2.0);
        (
            chord.0 + (through.0 - chord.0) * s,
            chord.1 + (through.1 - chord.1) * s,
        )
    };
    let m0 = tangent(prev.map_or(p0, p), p1, a.smoothness);
    let m1 = tangent(p0, next.map_or(p1, p), b.smoothness);
    let (u2, u3) = (u * u, u * u * u);
    let (h00, h10, h01, h11) = (
        2.0 * u3 - 3.0 * u2 + 1.0,
        u3 - 2.0 * u2 + u,
        -2.0 * u3 + 3.0 * u2,
        u3 - u2,
    );
    (
        h00 * p0.0 + h10 * m0.0 + h01 * p1.0 + h11 * m1.0,
        h00 * p0.1 + h10 * m0.1 + h01 * p1.1 + h11 * m1.1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn efeitos_novos_tem_parametros_padrao() {
        for k in EffectKind::VIDEO.into_iter().chain(EffectKind::AUDIO) {
            let e = Effect::new(k);
            assert_eq!(e.params.len(), k.params().len(), "{k:?}");
            assert!(e.is_default());
            for p in k.params() {
                assert!(p.min <= p.default && p.default <= p.max, "{k:?} {}", p.name);
            }
        }
    }

    #[test]
    fn a_vst_effect_keeps_its_states_and_values() {
        let fx = VstFx {
            path: "C:/VST3/Plugs.vst3".into(),
            class_id: "42414E414E4156544553544741494E31".into(),
            name: "Echo".into(),
            vendor: "Makers".into(),
            params: [(0, 0.25), (7, 1.0)].into_iter().collect(),
            state: vec![0, 1, 2, 250, 255],
            controller_state: Vec::new(),
            param_names: [(7, "Feedback".to_string())].into(),
        };
        let e = Effect::vst(fx.clone());
        assert!(e.kind.is_audio());
        assert_eq!(e.name(), "Echo");
        let text = serde_json::to_string(&e).unwrap();
        // The state as Base64 text, no controller state written.
        assert!(text.contains("\"AAEC+v8=\""), "{text}");
        assert!(!text.contains("controller_state"));
        let back: Effect = serde_json::from_str(&text).unwrap();
        assert_eq!(back, e);
        // Reset forgets the values and states, not which plug-in.
        let fresh = e.reset();
        let v = fresh.vst.as_deref().unwrap();
        assert_eq!(
            (v.path.clone(), v.params.len(), v.state.len()),
            (fx.path, 0, 0)
        );
    }

    #[test]
    fn an_old_crops_location_counts_the_frame() {
        let num = |v: &[f64]| PluginValue::Num(v.to_vec());
        let mut fx = PluginFx {
            id: "org.bananav.Crop".into(),
            version: [1, 0],
            name: "Crop".into(),
            params: std::collections::BTreeMap::from([(
                "location".to_string(),
                PluginParam {
                    value: num(&[1440.0, 540.0]),
                    keys: vec![PluginKey {
                        at: bv_core::Tick::ZERO,
                        value: num(&[960.0, 270.0]),
                        curve: KeyCurve::Linear,
                        handles: Vec::new(),
                    }],
                },
            )]),
        };
        fx.upgrade((1920, 1080));
        assert_eq!(fx.version, [2, 0]);
        assert_eq!(fx.params["location"].value, num(&[0.75, 0.5]));
        assert_eq!(fx.params["location"].keys[0].value, num(&[0.5, 0.25]));
        // Once only.
        fx.upgrade((1920, 1080));
        assert_eq!(fx.params["location"].value, num(&[0.75, 0.5]));
    }

    #[test]
    fn the_correctors_offset_becomes_8_bit_steps() {
        let num = |v: &[f64]| PluginValue::Num(v.to_vec());
        let mut fx = PluginFx {
            id: "org.bananav.ColorCorrector".into(),
            version: [1, 0],
            name: "Color Corrector".into(),
            params: std::collections::BTreeMap::from([(
                "offset".to_string(),
                PluginParam {
                    value: num(&[0.2]),
                    keys: vec![PluginKey {
                        at: bv_core::Tick::ZERO,
                        value: num(&[-1.0]),
                        curve: KeyCurve::Linear,
                        handles: Vec::new(),
                    }],
                },
            )]),
        };
        fx.upgrade((1920, 1080));
        assert_eq!(fx.version, [1, 1]);
        assert_eq!(fx.params["offset"].value, num(&[51.0]));
        assert_eq!(fx.params["offset"].keys[0].value, num(&[-254.0]));
        // Once only.
        fx.upgrade((1920, 1080));
        assert_eq!(fx.params["offset"].value, num(&[51.0]));
    }

    #[test]
    fn an_old_quick_blur_keeps_blurring_and_redone_effects_start_over() {
        let num = |v: &[f64]| PluginValue::Num(v.to_vec());
        let old = |id: &str| PluginFx {
            id: id.into(),
            version: [1, 0],
            name: String::new(),
            params: std::collections::BTreeMap::from([(
                "horizontalRange".to_string(),
                PluginParam {
                    value: num(&[4.0]),
                    keys: Vec::new(),
                },
            )]),
        };
        let mut blur = old("org.bananav.QuickBlur");
        blur.upgrade((1920, 1080));
        assert_eq!(blur.version, [2, 0]);
        assert_eq!(blur.params["blendAmount"].value, num(&[1.0]));
        for id in [
            "org.bananav.Median",
            "org.bananav.MinMax",
            "org.bananav.LabAdjust",
        ] {
            let mut fx = old(id);
            fx.upgrade((1920, 1080));
            assert_eq!(fx.version, [2, 0], "{id}");
            assert!(fx.params.is_empty(), "{id}");
        }
    }

    #[test]
    fn an_old_bezier_masking_keeps_its_masks_on_vegas_panel() {
        let num = |v: &[f64]| PluginValue::Num(v.to_vec());
        let param = |v: &[f64]| PluginParam {
            value: num(v),
            keys: Vec::new(),
        };
        let mut fx = PluginFx {
            id: "org.bananav.BezierMasking".into(),
            version: [1, 0],
            name: String::new(),
            params: std::collections::BTreeMap::from([
                ("type1".to_string(), param(&[0.0])),
                ("type2".to_string(), param(&[1.0])),
                ("feather1".to_string(), param(&[0.1])),
                ("featherStyle2".to_string(), param(&[2.0])),
                ("editTool1".to_string(), param(&[1.0])),
                ("blend".to_string(), param(&[0.75])),
            ]),
        };
        fx.upgrade((1920, 1080));
        assert_eq!(fx.version, [2, 0]);
        let at = |k: &str| fx.params[k].value.clone();
        // The rectangle and the oval keep their shapes; Out stays Out (the
        // second style now), Both stays Both; Delete stays Delete.
        assert_eq!(at("type1"), num(&[1.0]));
        assert_eq!(at("type2"), num(&[0.0]));
        assert_eq!(at("featherStyle1"), num(&[1.0]));
        assert_eq!(at("featherStyle2"), num(&[2.0]));
        assert_eq!(at("editTool1"), num(&[2.0]));
        assert_eq!(at("blend"), num(&[0.25]));
        // A tenth of the width, 192 pixels: 17.8% of the 1080 high.
        match at("feather1") {
            PluginValue::Num(v) => assert!((v[0] - 1920.0 / 1080.0 * 10.0).abs() < 1e-9),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parametro_ausente_usa_padrao_e_set_limita() {
        let mut e = Effect {
            kind: EffectKind::Levels,
            enabled: true,
            params: vec![0.1],
            plugin: None,
            vst: None,
            id: 0,
            automation_bypassed: false,
        };
        assert_eq!(e.param(2), 1.0);
        e.set_param(4, 9.0);
        assert_eq!(e.params.len(), 5);
        assert_eq!(e.param(4), 1.0);
    }

    #[test]
    fn curvas_vao_de_0_a_1() {
        for c in [
            FadeCurve::Linear,
            FadeCurve::Fast,
            FadeCurve::Slow,
            FadeCurve::Smooth,
            FadeCurve::Sharp,
        ] {
            assert_eq!(c.eval(0.0), 0.0);
            assert!((c.eval(1.0) - 1.0).abs() < 1e-6);
            assert!(c.eval(0.25) <= c.eval(0.75));
        }
        // Smooth in a crossfade keeps the power constant.
        let p = 0.3f32;
        let a = FadeCurve::Smooth.eval(1.0 - p);
        let b = FadeCurve::Smooth.eval(p);
        assert!((a * a + b * b - 1.0).abs() < 1e-6);
        // Sharp is the S turned around: the smoothstep's inverse, fast at
        // both ends and slow in the middle.
        for p in [0.1f32, 0.3, 0.5, 0.8] {
            let s = p * p * (3.0 - 2.0 * p);
            assert!((FadeCurve::Sharp.eval(s) - p).abs() < 1e-5, "{p}");
        }
        assert!(FadeCurve::Sharp.eval(0.25) > 0.25 && FadeCurve::Sharp.eval(0.75) < 0.75);
    }

    #[test]
    fn fade_outs_are_named_by_how_they_begin() {
        // VEGAS's: a Fast fade-out drops at once, a Slow one late.
        let just_begun = 0.9;
        assert!(FadeCurve::Fast.eval_out(just_begun) < 0.9);
        assert!(FadeCurve::Slow.eval_out(just_begun) > 0.9);
        // A Fast fade-out is a Fast fade-in upside down.
        for p in [0.1f32, 0.4, 0.8] {
            let upside_down = 1.0 - FadeCurve::Fast.eval(p);
            assert!((FadeCurve::Fast.eval_out(1.0 - p) - upside_down).abs() < 1e-6);
        }
        // The others are their fade-ins turned around in time.
        for c in [FadeCurve::Linear, FadeCurve::Smooth, FadeCurve::Sharp] {
            assert_eq!(c.eval_out(0.3), c.eval(0.3));
        }
    }

    #[test]
    fn the_six_key_curves_run_from_one_value_to_the_next() {
        for c in KeyCurve::ALL {
            if c == KeyCurve::Hold {
                assert_eq!(c.eval(0.9), 0.0, "holds");
                continue;
            }
            assert_eq!(c.eval(0.0), 0.0, "{c:?}");
            assert!((c.eval(1.0) - 1.0).abs() < 1e-6, "{c:?}");
        }
        assert!(KeyCurve::Fast.eval(0.25) > 0.25 && KeyCurve::Slow.eval(0.25) < 0.25);
    }

    #[test]
    fn smoothness_curves_the_path_through_the_keyframes() {
        let at = |x: f32, y: f32| Transform {
            x,
            y,
            ..Transform::default()
        };
        let s = bv_core::Tick::from_seconds;
        // Right, then up: the corner at (1, 0) one second in.
        let mut keys = vec![
            TransformKey::new(s(0), at(0.0, 0.0), KeyCurve::Linear),
            TransformKey::new(s(1), at(1.0, 0.0), KeyCurve::Linear),
            TransformKey::new(s(2), at(1.0, 1.0), KeyCurve::Linear),
        ];
        let half = bv_core::Tick(s(1).0 / 2);
        // Smoothness 0: straight lines.
        let v = transform_at(&keys, &Transform::default(), half);
        assert!((v.x - 0.5).abs() < 1e-5 && v.y.abs() < 1e-5);
        // Smooth at the corner: the path bends toward where it goes next.
        keys[1].smoothness = 100.0;
        let v = transform_at(&keys, &Transform::default(), half);
        assert!(v.y < -1e-3, "it bulges out, {v:?}");
        // It still passes through the keyframes.
        let v = transform_at(&keys, &Transform::default(), s(1));
        assert!((v.x - 1.0).abs() < 1e-5 && v.y.abs() < 1e-5);
    }
}
