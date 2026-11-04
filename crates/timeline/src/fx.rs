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
    /// One of [`ParamSpec::options`]: the value `min` is the first, each
    /// next one a whole number more.
    Choice,
    /// Data an effect's own editor keeps (a graph's points…): never shown
    /// as a control nor automated.
    Data,
    /// A graph's point count ([`ParamSpec::graph`]): its points follow it
    /// as [`ParamKind::Data`], and the panel draws and edits them.
    Graph,
}

/// How a value is shown and typed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    /// As it is, with [`ParamSpec::unit`].
    Value,
    /// A linear gain, shown in dB (0 as −Inf.).
    GainDb,
    /// A part (0–1, or more), shown as a percentage.
    Percent,
    /// A pan, −1 to 1, positive to the left: shown as "50.0 % L" or
    /// "50.0 % R", its slider's left the left.
    Pan,
    /// A level in dB, its lowest value shown as −Inf. (silence).
    Db,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamSpec {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub kind: ParamKind,
    /// What follows the number ("Hz", "ms"…).
    pub unit: &'static str,
    /// How many decimals the number shows.
    pub decimals: u8,
    /// A [`ParamKind::Choice`]'s options, in the order of their values.
    pub options: &'static [&'static str],
    /// Its slider moves on a logarithmic scale (frequencies, times).
    pub log: bool,
    pub show: Show,
    /// The heading it begins in its effect's panel ("": the one before's).
    pub group: &'static str,
    /// A [`ParamKind::Graph`]'s axes and size.
    pub graph: Option<Graph>,
    /// What it waits on: its control is greyed out unless each parameter
    /// named (by its place) holds one of the values its mask lets through
    /// (bit `v` for the whole value `v`: [`ON`], [`OFF`], [`either`]).
    pub when: &'static [(u8, u32)],
}

/// A graph an effect's panel draws and edits ([`ParamKind::Graph`]): its
/// axes and how many points it holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Graph {
    /// The most points it holds.
    pub points: u8,
    /// Its axes' ranges: x left to right, y bottom to top.
    pub x: (f32, f32),
    pub y: (f32, f32),
    /// The x axis on a logarithmic scale (frequencies).
    pub log_x: bool,
    /// The first and last points stay at the ends of the x axis.
    pub pinned_ends: bool,
    /// What follows each axis's numbers ("dB", "Hz"…).
    pub x_unit: &'static str,
    pub y_unit: &'static str,
}

impl ParamSpec {
    /// A graph of up to `g.points` points: this parameter holds how many
    /// it has (`count`), the [`Graph::params`] after it each point's x and
    /// y.
    pub const fn graph(name: &'static str, g: Graph, count: f32) -> ParamSpec {
        ParamSpec {
            kind: ParamKind::Graph,
            graph: Some(g),
            decimals: 0,
            ..ParamSpec::num(name, 2.0, g.points as f32, count)
        }
    }
}

impl Graph {
    /// How many parameters it takes after its count: x and y a point.
    pub const fn params(&self) -> usize {
        2 * self.points as usize
    }

    /// Its points' parameters (after the count), the first ones
    /// `defaults`, the rest at the start of each axis.
    pub const fn point_params<const N: usize>(&self, defaults: &[(f32, f32)]) -> [ParamSpec; N] {
        let mut out = [ParamSpec::data("", 0.0, 0.0, 0.0); N];
        let mut k = 0;
        while k < N / 2 {
            let (x, y) = if k < defaults.len() {
                defaults[k]
            } else {
                (self.x.0, self.y.0)
            };
            out[2 * k] = ParamSpec::data("", self.x.0, self.x.1, x);
            out[2 * k + 1] = ParamSpec::data("", self.y.0, self.y.1, y);
            k += 1;
        }
        out
    }
}

/// `a` then `b`, `C` long (its length, `A + B`, said where it is used).
pub const fn join<const A: usize, const B: usize, const C: usize>(
    a: [ParamSpec; A],
    b: [ParamSpec; B],
) -> [ParamSpec; C] {
    assert!(A + B == C, "the joined list is as long as its parts");
    let mut out = [ParamSpec::data("", 0.0, 0.0, 0.0); C];
    let mut i = 0;
    while i < A {
        out[i] = a[i];
        i += 1;
    }
    while i < C {
        out[i] = b[i - A];
        i += 1;
    }
    out
}

/// A [`ParamSpec::when`] mask: the switch on.
pub const ON: u32 = 1 << 1;
/// A [`ParamSpec::when`] mask: the switch off.
pub const OFF: u32 = 1 << 0;

/// A [`ParamSpec::when`] mask: a choice at `a` or at `b`.
pub const fn either(a: u32, b: u32) -> u32 {
    (1 << a) | (1 << b)
}

/// A [`ParamSpec::when`] mask: a whole number `v` or more.
pub const fn at_least(v: u32) -> u32 {
    !((1 << v) - 1)
}

impl ParamSpec {
    /// A number from `min` to `max`.
    pub const fn num(name: &'static str, min: f32, max: f32, default: f32) -> ParamSpec {
        ParamSpec {
            name,
            min,
            max,
            default,
            kind: ParamKind::Slider,
            unit: "",
            decimals: 2,
            options: &[],
            log: false,
            show: Show::Value,
            group: "",
            graph: None,
            when: &[],
        }
    }

    /// A switch, off (0) or on (1).
    pub const fn switch(name: &'static str, default: f32) -> ParamSpec {
        ParamSpec {
            kind: ParamKind::Toggle,
            ..ParamSpec::num(name, 0.0, 1.0, default)
        }
    }

    /// One of `options`, numbered from `first`.
    pub const fn choice(
        name: &'static str,
        first: f32,
        options: &'static [&'static str],
        default: f32,
    ) -> ParamSpec {
        ParamSpec {
            kind: ParamKind::Choice,
            options,
            decimals: 0,
            ..ParamSpec::num(name, first, first + options.len() as f32 - 1.0, default)
        }
    }

    /// Data for the effect's own editor, from `min` to `max`.
    pub const fn data(name: &'static str, min: f32, max: f32, default: f32) -> ParamSpec {
        ParamSpec {
            kind: ParamKind::Data,
            ..ParamSpec::num(name, min, max, default)
        }
    }

    pub const fn unit(self, unit: &'static str) -> ParamSpec {
        ParamSpec { unit, ..self }
    }

    pub const fn decimals(self, decimals: u8) -> ParamSpec {
        ParamSpec { decimals, ..self }
    }

    pub const fn log(self) -> ParamSpec {
        ParamSpec { log: true, ..self }
    }

    pub const fn show(self, show: Show) -> ParamSpec {
        ParamSpec { show, ..self }
    }

    pub const fn group(self, group: &'static str) -> ParamSpec {
        ParamSpec { group, ..self }
    }

    pub const fn when(self, when: &'static [(u8, u32)]) -> ParamSpec {
        ParamSpec { when, ..self }
    }

    /// Whether its control is live with the effect's values `param`
    /// (each parameter it waits on at a value it lets through).
    pub fn live(&self, param: impl Fn(usize) -> f32) -> bool {
        self.when.iter().all(|&(i, mask)| {
            let v = param(i as usize).round();
            (0.0..32.0).contains(&v) && mask & (1 << v as u32) != 0
        })
    }

    /// Whether it is a control of the effect's panel (not data).
    pub fn shown(&self) -> bool {
        self.kind != ParamKind::Data
    }

    /// Whether FX automation can follow it.
    pub fn automatable(&self) -> bool {
        matches!(
            self.kind,
            ParamKind::Slider | ParamKind::Toggle | ParamKind::Choice
        )
    }

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
            ParamKind::Choice => (self.min + u * (self.max - self.min)).round(),
            _ => self.min + u * (self.max - self.min),
        }
    }
}

const fn slider(name: &'static str, min: f32, max: f32, default: f32) -> ParamSpec {
    ParamSpec::num(name, min, max, default)
}

const fn toggle(name: &'static str, default: f32) -> ParamSpec {
    ParamSpec::switch(name, default)
}

const fn color(name: &'static str, default: f32) -> ParamSpec {
    ParamSpec {
        kind: ParamKind::Color,
        ..ParamSpec::num(name, 0.0, 1.0, default)
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
    /// VEGAS's Volume: a linear gain ([`crate::audio_fx::volume`]).
    Volume,
    /// VEGAS's Pan: one channel moved into the other ([`crate::audio_fx::pan`]).
    Pan,
    /// VEGAS's Smooth/Enhance ([`crate::audio_fx::smooth_enhance`]).
    SmoothEnhance,
    /// VEGAS's Noise Gate ([`crate::audio_fx::noise_gate`]); `NoiseGate`
    /// is BananaV's older one.
    NoiseGateFx,
    /// VEGAS's Track Noise Gate ([`crate::audio_fx::track_noise_gate`]).
    TrackNoiseGate,
    /// VEGAS's Simple Delay ([`crate::audio_fx::simple_delay`]).
    SimpleDelay,
    /// VEGAS's Dither ([`crate::audio_fx::dither`]).
    Dither,
    /// VEGAS's Resonant Filter ([`crate::audio_fx::resonant_filter`]).
    ResonantFilter,
    /// VEGAS's Parametric EQ ([`crate::audio_fx::parametric_eq`]).
    ParametricEq,
    /// VEGAS's Chorus ([`crate::audio_fx::chorus`]).
    Chorus,
    /// VEGAS's Flange/Wah-wah ([`crate::audio_fx::flange_wah_wah`]).
    FlangeWahWah,
    /// VEGAS's Reverb ([`crate::audio_fx::reverb`]); `Reverb` is
    /// BananaV's older one.
    ReverbFx,
    /// VEGAS's Track Compressor ([`crate::audio_fx::track_compressor`]).
    TrackCompressor,
    /// VEGAS's Track EQ ([`crate::audio_fx::track_eq`]).
    TrackEq,
    /// VEGAS's Paragraphic EQ ([`crate::audio_fx::paragraphic_eq`]).
    ParagraphicEq,
    /// VEGAS's Multi-Tap Delay ([`crate::audio_fx::multi_tap_delay`]).
    MultiTapDelay,
    /// VEGAS's Multi-Band Dynamics ([`crate::audio_fx::multi_band_dynamics`]).
    MultiBandDynamics,
    /// VEGAS's Delay of the Express set ([`crate::audio_fx::express_delay`]).
    ExpressDelay,
    /// VEGAS's Distortion of the Express set ([`crate::audio_fx::express_distortion`]).
    ExpressDistortion,
    /// VEGAS's Equalization of the Express set ([`crate::audio_fx::express_equalization`]).
    ExpressEqualization,
    /// VEGAS's Noise Gate of the Express set ([`crate::audio_fx::express_noise_gate`]).
    ExpressNoiseGate,
    /// VEGAS's Chorus of the Express set ([`crate::audio_fx::express_chorus`]).
    ExpressChorus,
    /// VEGAS's Flange/Wah-Wah of the Express set ([`crate::audio_fx::express_flange_wah_wah`]).
    ExpressFlangeWahWah,
    /// VEGAS's Dynamics of the Express set ([`crate::audio_fx::express_dynamics`]).
    ExpressDynamics,
    /// VEGAS's Stutter of the Express set ([`crate::audio_fx::express_stutter`]).
    ExpressStutter,
    /// VEGAS's Amplitude Modulation of the Express set ([`crate::audio_fx::express_amplitude_modulation`]).
    ExpressAmplitudeModulation,
    /// VEGAS's Graphic EQ of the Express set ([`crate::audio_fx::express_graphic_eq`]).
    ExpressGraphicEq,
    /// VEGAS's Reverb of the Express set ([`crate::audio_fx::express_reverb`]).
    ExpressReverb,
    /// VEGAS's Audio Restoration of the Express set
    /// ([`crate::audio_fx::express_audio_restoration`]).
    ExpressAudioRestoration,
    /// VEGAS's Amplitude Modulation ([`crate::audio_fx::amplitude_modulation`]).
    AmplitudeModulation,
    /// VEGAS's Graphic Dynamics ([`crate::audio_fx::graphic_dynamics`]).
    GraphicDynamics,
    /// VEGAS's Distortion ([`crate::audio_fx::distortion`]).
    Distortion,
    /// VEGAS's Graphic EQ ([`crate::audio_fx::graphic_eq`]).
    GraphicEq,
    /// VEGAS's Wave Hammer Surround ([`crate::audio_fx::wave_hammer`]).
    WaveHammer,
    /// the studio set's Gate ([`crate::audio_fx::efx_gate`]).
    StudioGate,
    /// the studio set's DeEsser ([`crate::audio_fx::efx_deesser`]).
    DeEsser,
    /// the studio set's Limiter; `Limiter` is BananaV's older one ([`crate::audio_fx::efx_limiter`]).
    StudioLimiter,
    /// the studio set's Tremolo Pan ([`crate::audio_fx::efx_tremolo_pan`]).
    TremoloPan,
    /// the studio set's Tube Stage ([`crate::audio_fx::efx_tube_stage`]).
    TubeStage,
    /// the studio set's Vocal Strip ([`crate::audio_fx::efx_vocal_strip`]).
    VocalStrip,
    /// the studio set's Compressor; `Compressor` is BananaV's older one ([`crate::audio_fx::efx_compressor`]).
    StudioCompressor,
    /// the studio set's Phaser ([`crate::audio_fx::efx_phaser`]).
    Phaser,
    /// the studio set's Chorus/Flanger ([`crate::audio_fx::efx_chorus_flanger`]).
    ChorusFlanger,
    /// the studio set's Reverb ([`crate::audio_fx::efx_reverb`]).
    StudioReverb,
    /// the studio set's Stereo Delay ([`crate::audio_fx::efx_stereo_delay`]).
    StereoDelay,
    /// VEGAS's Time Stretch ([`crate::audio_fx::time_stretch`]).
    TimeStretch,
    /// VEGAS's Time Stretch of the Express set ([`crate::audio_fx::express_time_stretch`]).
    ExpressTimeStretch,
    /// VEGAS's Pitch Shift ([`crate::audio_fx::pitch_shift`]).
    PitchShift,
    /// VEGAS's Gapper/Snipper ([`crate::audio_fx::gapper_snipper`]).
    GapperSnipper,
    /// VEGAS's Vibrato ([`crate::audio_fx::vibrato`]).
    Vibrato,
    /// A plug-in's effect (OpenFX): what it is lives in [`Effect::plugin`].
    Plugin,
    /// A VST 3 plug-in's audio effect: what it is lives in [`Effect::vst`].
    Vst,
}

impl EffectKind {
    pub const AUDIO: [EffectKind; 51] = [
        EffectKind::Volume,
        EffectKind::Pan,
        EffectKind::SmoothEnhance,
        EffectKind::NoiseGateFx,
        EffectKind::TrackNoiseGate,
        EffectKind::SimpleDelay,
        EffectKind::Dither,
        EffectKind::ResonantFilter,
        EffectKind::ParametricEq,
        EffectKind::Chorus,
        EffectKind::FlangeWahWah,
        EffectKind::ReverbFx,
        EffectKind::TrackCompressor,
        EffectKind::TrackEq,
        EffectKind::ParagraphicEq,
        EffectKind::MultiTapDelay,
        EffectKind::MultiBandDynamics,
        EffectKind::ExpressDelay,
        EffectKind::ExpressDistortion,
        EffectKind::ExpressEqualization,
        EffectKind::ExpressNoiseGate,
        EffectKind::ExpressChorus,
        EffectKind::ExpressFlangeWahWah,
        EffectKind::ExpressDynamics,
        EffectKind::ExpressStutter,
        EffectKind::ExpressAmplitudeModulation,
        EffectKind::ExpressGraphicEq,
        EffectKind::ExpressReverb,
        EffectKind::ExpressAudioRestoration,
        EffectKind::AmplitudeModulation,
        EffectKind::GraphicDynamics,
        EffectKind::Distortion,
        EffectKind::GraphicEq,
        EffectKind::WaveHammer,
        EffectKind::StudioGate,
        EffectKind::DeEsser,
        EffectKind::StudioLimiter,
        EffectKind::TremoloPan,
        EffectKind::TubeStage,
        EffectKind::VocalStrip,
        EffectKind::StudioCompressor,
        EffectKind::Phaser,
        EffectKind::ChorusFlanger,
        EffectKind::StudioReverb,
        EffectKind::StereoDelay,
        EffectKind::TimeStretch,
        EffectKind::ExpressTimeStretch,
        EffectKind::PitchShift,
        EffectKind::GapperSnipper,
        EffectKind::Vibrato,
        EffectKind::ModernEq,
    ];

    /// Whether it changes the sound's length (or needs all of it at once):
    /// then it works through Non-Real-Time Event FX, and a playing chain
    /// lets the sound through it.
    pub fn works_offline(self) -> bool {
        matches!(
            self,
            EffectKind::TimeStretch
                | EffectKind::ExpressTimeStretch
                | EffectKind::PitchShift
                | EffectKind::GapperSnipper
                | EffectKind::Vibrato
        )
    }

    /// Whether it works on sound: those offered and those only older
    /// projects have (the compiler asks for every new kind here).
    pub fn is_audio(self) -> bool {
        match self {
            EffectKind::Gain
            | EffectKind::Eq3
            | EffectKind::HighPass
            | EffectKind::LowPass
            | EffectKind::Compressor
            | EffectKind::Limiter
            | EffectKind::NoiseGate
            | EffectKind::Delay
            | EffectKind::Reverb
            | EffectKind::ModernEq
            | EffectKind::Volume
            | EffectKind::Pan
            | EffectKind::SmoothEnhance
            | EffectKind::NoiseGateFx
            | EffectKind::TrackNoiseGate
            | EffectKind::SimpleDelay
            | EffectKind::Dither
            | EffectKind::ResonantFilter
            | EffectKind::ParametricEq
            | EffectKind::Chorus
            | EffectKind::FlangeWahWah
            | EffectKind::ReverbFx
            | EffectKind::TrackCompressor
            | EffectKind::TrackEq
            | EffectKind::ParagraphicEq
            | EffectKind::MultiTapDelay
            | EffectKind::MultiBandDynamics
            | EffectKind::ExpressDelay
            | EffectKind::ExpressDistortion
            | EffectKind::ExpressEqualization
            | EffectKind::ExpressNoiseGate
            | EffectKind::ExpressChorus
            | EffectKind::ExpressFlangeWahWah
            | EffectKind::ExpressDynamics
            | EffectKind::ExpressStutter
            | EffectKind::ExpressAmplitudeModulation
            | EffectKind::ExpressGraphicEq
            | EffectKind::ExpressReverb
            | EffectKind::ExpressAudioRestoration
            | EffectKind::AmplitudeModulation
            | EffectKind::GraphicDynamics
            | EffectKind::Distortion
            | EffectKind::GraphicEq
            | EffectKind::WaveHammer
            | EffectKind::StudioGate
            | EffectKind::DeEsser
            | EffectKind::StudioLimiter
            | EffectKind::TremoloPan
            | EffectKind::TubeStage
            | EffectKind::VocalStrip
            | EffectKind::StudioCompressor
            | EffectKind::Phaser
            | EffectKind::ChorusFlanger
            | EffectKind::StudioReverb
            | EffectKind::StereoDelay
            | EffectKind::TimeStretch
            | EffectKind::ExpressTimeStretch
            | EffectKind::PitchShift
            | EffectKind::GapperSnipper
            | EffectKind::Vibrato
            | EffectKind::Vst => true,
            EffectKind::BrightnessContrast
            | EffectKind::ColorGrade
            | EffectKind::Levels
            | EffectKind::BlackAndWhite
            | EffectKind::Sepia
            | EffectKind::Invert
            | EffectKind::GaussianBlur
            | EffectKind::Sharpen
            | EffectKind::Vignette
            | EffectKind::Mirror
            | EffectKind::Pixelate
            | EffectKind::ChromaKey
            | EffectKind::Glow
            | EffectKind::Plugin => false,
        }
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
            EffectKind::Compressor => "Compressor (legacy)",
            EffectKind::Limiter => "Limiter (legacy)",
            EffectKind::NoiseGate => "Noise gate (legacy)",
            EffectKind::Delay => "Delay (Echo)",
            EffectKind::Reverb => "Reverb (legacy)",
            EffectKind::ModernEq => "Modern Equalizer",
            EffectKind::Volume => "Volume",
            EffectKind::Pan => "Pan",
            EffectKind::SmoothEnhance => "Smooth/Enhance",
            EffectKind::NoiseGateFx => "Noise Gate",
            EffectKind::TrackNoiseGate => "Track Noise Gate",
            EffectKind::SimpleDelay => "Simple Delay",
            EffectKind::Dither => "Dither",
            EffectKind::ResonantFilter => "Resonant Filter",
            EffectKind::ParametricEq => "Parametric EQ",
            EffectKind::Chorus => "Chorus",
            EffectKind::FlangeWahWah => "Flange/Wah-wah",
            EffectKind::ReverbFx => "Reverb",
            EffectKind::TrackCompressor => "Track Compressor",
            EffectKind::TrackEq => "Track EQ",
            EffectKind::ParagraphicEq => "Paragraphic EQ",
            EffectKind::MultiTapDelay => "Multi-Tap Delay",
            EffectKind::MultiBandDynamics => "Multi-Band Dynamics",
            EffectKind::ExpressDelay => "Express Delay",
            EffectKind::ExpressDistortion => "Express Distortion",
            EffectKind::ExpressEqualization => "Express Equalization",
            EffectKind::ExpressNoiseGate => "Express Noise Gate",
            EffectKind::ExpressChorus => "Express Chorus",
            EffectKind::ExpressFlangeWahWah => "Express Flange/Wah-Wah",
            EffectKind::ExpressDynamics => "Express Dynamics",
            EffectKind::ExpressStutter => "Express Stutter",
            EffectKind::ExpressAmplitudeModulation => "Express Amplitude Modulation",
            EffectKind::ExpressGraphicEq => "Express Graphic EQ",
            EffectKind::ExpressReverb => "Express Reverb",
            EffectKind::ExpressAudioRestoration => "Express Audio Restoration",
            EffectKind::AmplitudeModulation => "Amplitude Modulation",
            EffectKind::GraphicDynamics => "Graphic Dynamics",
            EffectKind::Distortion => "Distortion",
            EffectKind::GraphicEq => "Graphic EQ",
            EffectKind::WaveHammer => "Wave Hammer Surround",
            EffectKind::StudioGate => "Gate",
            EffectKind::DeEsser => "DeEsser",
            EffectKind::StudioLimiter => "Limiter",
            EffectKind::TremoloPan => "Tremolo Pan",
            EffectKind::TubeStage => "Tube Stage",
            EffectKind::VocalStrip => "Vocal Strip",
            EffectKind::StudioCompressor => "Compressor",
            EffectKind::Phaser => "Phaser",
            EffectKind::ChorusFlanger => "Chorus/Flanger",
            EffectKind::StudioReverb => "Studio Reverb",
            EffectKind::StereoDelay => "Stereo Delay",
            EffectKind::TimeStretch => "Time Stretch",
            EffectKind::ExpressTimeStretch => "Express Time Stretch",
            EffectKind::PitchShift => "Pitch Shift",
            EffectKind::GapperSnipper => "Gapper/Snipper",
            EffectKind::Vibrato => "Vibrato",
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
            EffectKind::Volume | EffectKind::Pan => "Utility",
            EffectKind::SmoothEnhance => "Equalization",
            EffectKind::NoiseGateFx | EffectKind::TrackNoiseGate => "Dynamics",
            EffectKind::SimpleDelay => "Ambience",
            EffectKind::Dither => "Utility",
            EffectKind::ResonantFilter | EffectKind::ParametricEq => "Equalization",
            EffectKind::Chorus | EffectKind::FlangeWahWah => "Modulation",
            EffectKind::ReverbFx => "Ambience",
            EffectKind::TrackCompressor => "Dynamics",
            EffectKind::TrackEq | EffectKind::ParagraphicEq => "Equalization",
            EffectKind::MultiTapDelay => "Ambience",
            EffectKind::MultiBandDynamics => "Dynamics",
            EffectKind::ExpressDelay => "Ambience",
            EffectKind::ExpressDistortion => "Distortion",
            EffectKind::ExpressEqualization => "Equalization",
            EffectKind::ExpressNoiseGate => "Dynamics",
            EffectKind::ExpressChorus => "Modulation",
            EffectKind::ExpressFlangeWahWah => "Modulation",
            EffectKind::ExpressDynamics => "Dynamics",
            EffectKind::ExpressStutter => "Modulation",
            EffectKind::ExpressAmplitudeModulation => "Modulation",
            EffectKind::ExpressGraphicEq => "Equalization",
            EffectKind::ExpressReverb => "Ambience",
            EffectKind::ExpressAudioRestoration => "Utility",
            EffectKind::AmplitudeModulation => "Modulation",
            EffectKind::GraphicDynamics => "Dynamics",
            EffectKind::Distortion => "Distortion",
            EffectKind::GraphicEq => "Equalization",
            EffectKind::WaveHammer => "Dynamics",
            EffectKind::StudioGate => "Dynamics",
            EffectKind::DeEsser => "Dynamics",
            EffectKind::StudioLimiter => "Dynamics",
            EffectKind::TremoloPan => "Modulation",
            EffectKind::TubeStage => "Distortion",
            EffectKind::VocalStrip => "Dynamics",
            EffectKind::StudioCompressor => "Dynamics",
            EffectKind::Phaser => "Modulation",
            EffectKind::ChorusFlanger => "Modulation",
            EffectKind::StudioReverb => "Ambience",
            EffectKind::StereoDelay => "Ambience",
            EffectKind::TimeStretch => "Pitch and time",
            EffectKind::ExpressTimeStretch => "Pitch and time",
            EffectKind::PitchShift => "Pitch and time",
            EffectKind::GapperSnipper => "Pitch and time",
            EffectKind::Vibrato => "Pitch and time",
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
            EffectKind::Volume => crate::audio_fx::volume::PARAMS,
            EffectKind::Pan => crate::audio_fx::pan::PARAMS,
            EffectKind::SmoothEnhance => crate::audio_fx::smooth_enhance::PARAMS,
            EffectKind::NoiseGateFx => crate::audio_fx::noise_gate::PARAMS,
            EffectKind::TrackNoiseGate => crate::audio_fx::track_noise_gate::PARAMS,
            EffectKind::SimpleDelay => crate::audio_fx::simple_delay::PARAMS,
            EffectKind::Dither => crate::audio_fx::dither::PARAMS,
            EffectKind::ResonantFilter => crate::audio_fx::resonant_filter::PARAMS,
            EffectKind::ParametricEq => crate::audio_fx::parametric_eq::PARAMS,
            EffectKind::Chorus => crate::audio_fx::chorus::PARAMS,
            EffectKind::FlangeWahWah => crate::audio_fx::flange_wah_wah::PARAMS,
            EffectKind::ReverbFx => crate::audio_fx::reverb::PARAMS,
            EffectKind::TrackCompressor => crate::audio_fx::track_compressor::PARAMS,
            EffectKind::TrackEq => crate::audio_fx::track_eq::PARAMS,
            EffectKind::ParagraphicEq => crate::audio_fx::paragraphic_eq::PARAMS,
            EffectKind::MultiTapDelay => crate::audio_fx::multi_tap_delay::PARAMS,
            EffectKind::MultiBandDynamics => crate::audio_fx::multi_band_dynamics::PARAMS,
            EffectKind::ExpressDelay => crate::audio_fx::express_delay::PARAMS,
            EffectKind::ExpressDistortion => crate::audio_fx::express_distortion::PARAMS,
            EffectKind::ExpressEqualization => crate::audio_fx::express_equalization::PARAMS,
            EffectKind::ExpressNoiseGate => crate::audio_fx::express_noise_gate::PARAMS,
            EffectKind::ExpressChorus => crate::audio_fx::express_chorus::PARAMS,
            EffectKind::ExpressFlangeWahWah => crate::audio_fx::express_flange_wah_wah::PARAMS,
            EffectKind::ExpressDynamics => crate::audio_fx::express_dynamics::PARAMS,
            EffectKind::ExpressStutter => crate::audio_fx::express_stutter::PARAMS,
            EffectKind::ExpressAmplitudeModulation => {
                crate::audio_fx::express_amplitude_modulation::PARAMS
            }
            EffectKind::ExpressGraphicEq => crate::audio_fx::express_graphic_eq::PARAMS,
            EffectKind::ExpressReverb => crate::audio_fx::express_reverb::PARAMS,
            EffectKind::ExpressAudioRestoration => {
                crate::audio_fx::express_audio_restoration::PARAMS
            }
            EffectKind::AmplitudeModulation => crate::audio_fx::amplitude_modulation::PARAMS,
            EffectKind::GraphicDynamics => crate::audio_fx::graphic_dynamics::PARAMS,
            EffectKind::Distortion => crate::audio_fx::distortion::PARAMS,
            EffectKind::GraphicEq => crate::audio_fx::graphic_eq::PARAMS,
            EffectKind::WaveHammer => crate::audio_fx::wave_hammer::PARAMS,
            EffectKind::StudioGate => crate::audio_fx::efx_gate::PARAMS,
            EffectKind::DeEsser => crate::audio_fx::efx_deesser::PARAMS,
            EffectKind::StudioLimiter => crate::audio_fx::efx_limiter::PARAMS,
            EffectKind::TremoloPan => crate::audio_fx::efx_tremolo_pan::PARAMS,
            EffectKind::TubeStage => crate::audio_fx::efx_tube_stage::PARAMS,
            EffectKind::VocalStrip => crate::audio_fx::efx_vocal_strip::PARAMS,
            EffectKind::StudioCompressor => crate::audio_fx::efx_compressor::PARAMS,
            EffectKind::Phaser => crate::audio_fx::efx_phaser::PARAMS,
            EffectKind::ChorusFlanger => crate::audio_fx::efx_chorus_flanger::PARAMS,
            EffectKind::StudioReverb => crate::audio_fx::efx_reverb::PARAMS,
            EffectKind::StereoDelay => crate::audio_fx::efx_stereo_delay::PARAMS,
            EffectKind::TimeStretch => crate::audio_fx::time_stretch::PARAMS,
            EffectKind::ExpressTimeStretch => crate::audio_fx::express_time_stretch::PARAMS,
            EffectKind::PitchShift => crate::audio_fx::pitch_shift::PARAMS,
            EffectKind::GapperSnipper => crate::audio_fx::gapper_snipper::PARAMS,
            EffectKind::Vibrato => crate::audio_fx::vibrato::PARAMS,
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
    /// Pass, Low Shelving, High Shelving, Band Pass, Peaking, Band Stop),
    /// frequency, gain, Q, slope (dB an octave) and routing (Stereo, Left,
    /// Right, Mid, Side).
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
    /// Band `b`'s phase (0 minimum, 1 linear), after Oversampling.
    pub const fn model(b: usize) -> usize {
        OVERSAMPLING + 1 + b
    }
    /// Linear Phase Quality (0 to 4: how long the linear-phase filter is).
    pub const QUALITY: usize = model(BANDS);
    /// How many parameters it has.
    pub const COUNT: usize = QUALITY + 1;
}

/// A Modern Equalizer band's parameters; where a band at rest sits.
macro_rules! eq_band {
    ($n:literal, $freq:expr) => {
        [
            toggle(concat!("Band ", $n), 0.0),
            toggle(concat!("Band ", $n, " On"), 1.0),
            slider(concat!("Band ", $n, " Type"), 0.0, 6.0, 5.0),
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
    slider("Mono Below (Hz)", 20.0, 200.0, 20.0),
    slider("Stereo Width (%)", 0.0, 100.0, 100.0),
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

/// Each band's phase: minimum (0) or linear (1).
const MODERN_EQ_MODELS: [ParamSpec; modern_eq::BANDS] = [
    toggle("Band 1 Linear Phase", 0.0),
    toggle("Band 2 Linear Phase", 0.0),
    toggle("Band 3 Linear Phase", 0.0),
    toggle("Band 4 Linear Phase", 0.0),
    toggle("Band 5 Linear Phase", 0.0),
    toggle("Band 6 Linear Phase", 0.0),
    toggle("Band 7 Linear Phase", 0.0),
    toggle("Band 8 Linear Phase", 0.0),
];

const MODERN_EQ: [ParamSpec; modern_eq::COUNT] = {
    let mut out = [MODERN_EQ_GLOBALS[0]; modern_eq::COUNT];
    out[modern_eq::OVERSAMPLING] = slider("Oversampling", 0.0, 3.0, 1.0);
    out[modern_eq::QUALITY] = slider("Linear Phase Quality", 0.0, 4.0, 2.0);
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
        out[modern_eq::model(b)] = MODERN_EQ_MODELS[b];
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
    /// The S turned around, as VEGAS's: fast at both ends, level in the
    /// middle — 4p³ − 6p² + 3p, its slopes 3, 0 and 3.
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
            FadeCurve::Sharp => p * (3.0 + p * (4.0 * p - 6.0)),
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
        // Sharp is the S turned around, VEGAS's 4p³ − 6p² + 3p: fast at
        // both ends, level in the middle.
        let sharp = |p: f32| FadeCurve::Sharp.eval(p);
        assert_eq!(sharp(0.5), 0.5);
        assert!((sharp(0.25) - 0.4375).abs() < 1e-6);
        assert!((sharp(0.75) - 0.5625).abs() < 1e-6);
        let slope = |p: f32| (sharp(p + 1e-3) - sharp(p - 1e-3)) / 2e-3;
        assert!(slope(0.5).abs() < 1e-3);
        assert!((sharp(1e-3) / 1e-3 - 3.0).abs() < 1e-2);
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
