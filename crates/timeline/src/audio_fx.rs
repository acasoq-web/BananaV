//! BananaV's audio effects that follow VEGAS's: each one's parameters — in
//! the order its processing reads them, with VEGAS's ranges and defaults —
//! and its factory presets. The processing is `bv_mixer`'s `fx`.

use crate::fx::{Graph, OFF, ON, ParamSpec, Show, at_least, either, join};

/// A factory preset: its name and every parameter's value, in order.
pub type Preset = (&'static str, &'static [f32]);

/// The factory presets of `k`, when it is one of these effects.
pub fn presets(k: crate::EffectKind) -> Option<&'static [Preset]> {
    use crate::EffectKind as K;
    Some(match k {
        K::Volume => volume::PRESETS,
        K::Pan => pan::PRESETS,
        K::SmoothEnhance => smooth_enhance::PRESETS,
        K::NoiseGateFx => noise_gate::PRESETS,
        K::TrackNoiseGate => track_noise_gate::PRESETS,
        K::SimpleDelay => simple_delay::PRESETS,
        K::Dither => dither::PRESETS,
        K::ResonantFilter => resonant_filter::PRESETS,
        K::ParametricEq => parametric_eq::PRESETS,
        K::Chorus => chorus::PRESETS,
        K::FlangeWahWah => flange_wah_wah::PRESETS,
        K::ReverbFx => reverb::PRESETS,
        K::TrackCompressor => track_compressor::PRESETS,
        K::TrackEq => track_eq::PRESETS,
        K::ParagraphicEq => paragraphic_eq::PRESETS,
        K::MultiTapDelay => multi_tap_delay::PRESETS,
        K::MultiBandDynamics => multi_band_dynamics::PRESETS,
        K::ExpressDelay => express_delay::PRESETS,
        K::ExpressDistortion => express_distortion::PRESETS,
        K::ExpressEqualization => express_equalization::PRESETS,
        K::ExpressNoiseGate => express_noise_gate::PRESETS,
        K::ExpressChorus => express_chorus::PRESETS,
        K::ExpressFlangeWahWah => express_flange_wah_wah::PRESETS,
        K::ExpressDynamics => express_dynamics::PRESETS,
        K::ExpressStutter => express_stutter::PRESETS,
        K::ExpressAmplitudeModulation => express_amplitude_modulation::PRESETS,
        K::ExpressGraphicEq => express_graphic_eq::PRESETS,
        K::ExpressReverb => express_reverb::PRESETS,
        K::AmplitudeModulation => amplitude_modulation::PRESETS,
        K::GraphicDynamics => graphic_dynamics::PRESETS,
        K::Distortion => distortion::PRESETS,
        K::GraphicEq => graphic_eq::PRESETS,
        K::WaveHammer => wave_hammer::PRESETS,
        K::StudioGate => efx_gate::PRESETS,
        K::DeEsser => efx_deesser::PRESETS,
        K::StudioLimiter => efx_limiter::PRESETS,
        K::TremoloPan => efx_tremolo_pan::PRESETS,
        K::TubeStage => efx_tube_stage::PRESETS,
        K::VocalStrip => efx_vocal_strip::PRESETS,
        K::StudioCompressor => efx_compressor::PRESETS,
        K::Phaser => efx_phaser::PRESETS,
        K::ChorusFlanger => efx_chorus_flanger::PRESETS,
        K::StudioReverb => efx_reverb::PRESETS,
        K::StereoDelay => efx_stereo_delay::PRESETS,
        K::TimeStretch => time_stretch::PRESETS,
        K::ExpressTimeStretch => express_time_stretch::PRESETS,
        K::PitchShift => pitch_shift::PRESETS,
        K::GapperSnipper => gapper_snipper::PRESETS,
        K::Vibrato => vibrato::PRESETS,
        K::ExpressAudioRestoration => express_audio_restoration::PRESETS,
        _ => return None,
    })
}

/// Volume: one linear gain, 0 to 2 (+6.02 dB), shown in dB.
pub mod volume {
    use super::*;

    pub const GAIN: usize = 0;

    pub const PARAMS: &[ParamSpec] = &[ParamSpec::num("Volume", 0.0, 2.0, 1.0)
        .show(Show::GainDb)
        .decimals(2)];

    pub const PRESETS: &[Preset] = &[];
}

/// Pan: a stereo balance that moves one channel into the other, −1 to 1,
/// positive to the left.
pub mod pan {
    use super::*;

    pub const PAN: usize = 0;

    pub const PARAMS: &[ParamSpec] = &[ParamSpec::num("Pan", -1.0, 1.0, 0.0).show(Show::Pan)];

    pub const PRESETS: &[Preset] = &[];
}

/// Smooth/Enhance: one first-order filter a channel, `Operation` from −5
/// (smoothest) to 5 (most enhanced), 0 nothing.
pub mod smooth_enhance {
    use super::*;

    pub const OPERATION: usize = 0;

    pub const PARAMS: &[ParamSpec] = &[ParamSpec::num("Operation", -5.0, 5.0, 0.0).decimals(0)];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0]),
        ("Boost high frequencies", &[3.0]),
    ];
}

/// The noise gates' parameters, in dB and milliseconds.
pub mod gate {
    pub const THRESHOLD: usize = 0;
    pub const ATTACK: usize = 1;
    pub const RELEASE: usize = 2;
}

/// Noise Gate: silence below a threshold, opened after the attack and
/// closed after the release.
pub mod noise_gate {
    use super::*;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Threshold level", -80.0, 0.0, -20.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Attack time", 1.0, 500.0, 3.0)
            .unit("ms")
            .decimals(0)
            .log(),
        ParamSpec::num("Release time", 1.0, 5000.0, 100.0)
            .unit("ms")
            .decimals(0)
            .log(),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[-20.0, 3.0, 100.0]),
        ("Strong beats only", &[-6.0, 1.0, 300.0]),
        ("Long decay time", &[-38.0, 10.0, 1000.0]),
        ("Noise gate 1", &[-60.0, 10.0, 500.0]),
        ("Noise gate 2", &[-32.0, 1.0, 500.0]),
        ("Slow attack", &[-40.0, 300.0, 1000.0]),
        ("Slow attack (ACX)", &[-40.0, 300.0, 2500.0]),
    ];
}

/// Track Noise Gate: the same gate, its attack from 0 ms.
pub mod track_noise_gate {
    use super::*;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Threshold level", -80.0, 0.0, -80.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Attack time", 0.0, 500.0, 3.0)
            .unit("ms")
            .decimals(0),
        ParamSpec::num("Release time", 1.0, 5000.0, 100.0)
            .unit("ms")
            .decimals(0)
            .log(),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[-80.0, 3.0, 100.0]),
        ("Strong beats only", &[-6.0, 1.0, 300.0]),
        ("Long decay time", &[-38.0, 10.0, 1000.0]),
        ("Noise gate 1", &[-60.0, 10.0, 500.0]),
        ("Noise gate 2", &[-32.0, 1.0, 500.0]),
        ("Slow attack", &[-40.0, 300.0, 1000.0]),
        ("Slow attack (ACX)", &[-40.0, 300.0, 2500.0]),
    ];
}

/// Tempo sync: times that follow the project's tempo, a note (or
/// measures) times a multiplier.
pub mod tempo {
    use crate::Tempo;

    /// The notes a time can follow, shortest first.
    pub const NOTES: &[&str] = &[
        "64th Note Triplet",
        "64th Note",
        "Dotted 64th Note",
        "32nd Note Triplet",
        "32nd Note",
        "Dotted 32nd Note",
        "16th Note Triplet",
        "16th Note",
        "Dotted 16th Note",
        "8th Note Triplet",
        "8th Note",
        "Dotted 8th Note",
        "Quarter Note Triplet",
        "Quarter Note",
        "Dotted Quarter Note",
        "Half Note Triplet",
        "Half Note",
        "Dotted Half Note",
    ];

    /// [`NOTES`] and the measure, last.
    pub const NOTES_OR_MEASURE: &[&str] = &[
        "64th Note Triplet",
        "64th Note",
        "Dotted 64th Note",
        "32nd Note Triplet",
        "32nd Note",
        "Dotted 32nd Note",
        "16th Note Triplet",
        "16th Note",
        "Dotted 16th Note",
        "8th Note Triplet",
        "8th Note",
        "Dotted 8th Note",
        "Quarter Note Triplet",
        "Quarter Note",
        "Dotted Quarter Note",
        "Half Note Triplet",
        "Half Note",
        "Dotted Half Note",
        "Measures",
    ];

    /// The place of the quarter note, the default.
    pub const QUARTER: f32 = 13.0;
    /// The place of the measure in [`NOTES_OR_MEASURE`].
    pub const MEASURE: f32 = 18.0;

    /// Each note's length in quarter notes.
    const QUARTERS: [f64; 18] = [
        1.0 / 24.0,
        1.0 / 16.0,
        3.0 / 32.0,
        1.0 / 12.0,
        1.0 / 8.0,
        3.0 / 16.0,
        1.0 / 6.0,
        1.0 / 4.0,
        3.0 / 8.0,
        1.0 / 3.0,
        1.0 / 2.0,
        3.0 / 4.0,
        2.0 / 3.0,
        1.0,
        3.0 / 2.0,
        4.0 / 3.0,
        2.0,
        3.0,
    ];

    /// How long note `note` of [`NOTES_OR_MEASURE`] lasts at `tempo`, in
    /// seconds (past the notes, a measure).
    pub fn seconds(note: f32, tempo: &Tempo) -> f64 {
        let per_beat = tempo.beat_note.max(1) as f64;
        // A quarter note: a beat, times 4 over the note that gets the beat.
        let quarter = 60.0 / tempo.beats_per_minute.max(1e-3) * per_beat / 4.0;
        let quarters = match QUARTERS.get(note.round().max(0.0) as usize) {
            Some(&q) => q,
            None => tempo.beats_per_measure.max(1) as f64 * 4.0 / per_beat,
        };
        quarter * quarters
    }
}

/// Simple Delay: one echo, or many (fed back) fading by the decay time;
/// each time in seconds or a note of the project's tempo.
pub mod simple_delay {
    use super::*;

    pub const DRY: usize = 0;
    pub const WET: usize = 1;
    pub const TIME: usize = 2;
    pub const SYNC: usize = 3;
    pub const MULTIPLIER: usize = 4;
    pub const NOTE: usize = 5;
    pub const MULTIPLE: usize = 6;
    pub const DECAY: usize = 7;
    pub const DECAY_SYNC: usize = 8;
    pub const DECAY_MULTIPLIER: usize = 9;
    pub const DECAY_NOTE: usize = 10;

    const TIMED: &[(u8, u32)] = &[(SYNC as u8, OFF)];
    const SYNCED: &[(u8, u32)] = &[(SYNC as u8, ON)];
    const FED_BACK: &[(u8, u32)] = &[(MULTIPLE as u8, ON)];
    const DECAY_TIMED: &[(u8, u32)] = &[(MULTIPLE as u8, ON), (DECAY_SYNC as u8, OFF)];
    const DECAY_SYNCED: &[(u8, u32)] = &[(MULTIPLE as u8, ON), (DECAY_SYNC as u8, ON)];

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Dry out", -80.0, 0.0, -6.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Delay out", -80.0, 0.0, -6.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Delay time", 0.001, 5.0, 0.1)
            .unit("s")
            .decimals(3)
            .log()
            .when(TIMED),
        ParamSpec::switch("Tempo sync delay", 0.0),
        ParamSpec::num("Delay multiplier", 0.001, 4.0, 1.0)
            .decimals(3)
            .log()
            .when(SYNCED),
        ParamSpec::choice("Delay note", 0.0, tempo::NOTES, tempo::QUARTER).when(SYNCED),
        ParamSpec::switch("Multiple delays (feedback)", 0.0),
        ParamSpec::num("Decay time", 0.1, 20.0, 0.1)
            .unit("s")
            .decimals(1)
            .log()
            .when(DECAY_TIMED),
        ParamSpec::switch("Tempo sync decay", 0.0).when(FED_BACK),
        ParamSpec::num("Decay multiplier", 0.001, 32.0, 1.0)
            .decimals(3)
            .log()
            .when(DECAY_SYNCED),
        ParamSpec::choice("Decay note", 0.0, tempo::NOTES_OR_MEASURE, tempo::MEASURE)
            .when(DECAY_SYNCED),
    ];

    pub const PRESETS: &[Preset] = &[
        (
            "Default all parameters",
            &[-6.0, -6.0, 0.1, 0.0, 1.0, 13.0, 0.0, 0.1, 0.0, 1.0, 18.0],
        ),
        (
            "Echo chamber",
            &[-9.0, -3.0, 0.4, 0.0, 1.0, 13.0, 1.0, 2.0, 0.0, 1.0, 18.0],
        ),
        (
            "Slap-back echo",
            &[-9.0, -12.0, 0.4, 0.0, 1.0, 13.0, 0.0, 2.0, 0.0, 1.0, 18.0],
        ),
        (
            "Grand Canyon",
            &[-4.0, -14.0, 1.0, 0.0, 1.0, 13.0, 1.0, 3.0, 0.0, 1.0, 18.0],
        ),
        (
            "Shift selection forwards (select one channel only)",
            &[-80.0, 0.0, 1.0, 0.0, 1.0, 13.0, 0.0, 2.0, 0.0, 1.0, 18.0],
        ),
        (
            "Pseudo-stereo (select one channel only)",
            &[-80.0, 0.0, 0.01, 0.0, 1.0, 13.0, 0.0, 0.1, 0.0, 1.0, 18.0],
        ),
    ];
}

/// Dither: the sound rounded to the levels of a bit depth, with a little
/// noise added first so the rounding's error is noise, not distortion,
/// and that noise moved where it is heard least.
pub mod dither {
    use super::*;

    pub const DEPTH: usize = 0;
    pub const NOISE: usize = 1;
    pub const SHAPING: usize = 2;

    /// Each depth's bits, in the order of its options.
    pub const BITS: [u32; 7] = [8, 12, 16, 18, 20, 24, 32];

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::choice(
            "Quantization depth",
            0.0,
            &[
                "8 bit", "12 bit", "16 bit", "18 bit", "20 bit", "24 bit", "32 bit",
            ],
            2.0,
        ),
        ParamSpec::choice(
            "Dither type",
            0.0,
            &[
                "None",
                "Half Rectangular",
                "Rectangular",
                "Triangular",
                "Highpass Triangular",
                "Gaussian",
            ],
            1.0,
        ),
        ParamSpec::choice(
            "Noise shaping",
            0.0,
            &["Off", "High-pass contour", "Equal loudness contour"],
            0.0,
        ),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[2.0, 1.0, 0.0]),
        ("Dither for 8-bit (low sample rate)", &[0.0, 3.0, 0.0]),
        ("Dither for 8-bit", &[0.0, 4.0, 1.0]),
        ("Dither for 16-bit (low sample rate)", &[2.0, 3.0, 2.0]),
        ("Dither for 16-bit", &[2.0, 4.0, 2.0]),
    ];
}

/// Resonant Filter: a low-, band- or high-pass with a resonance that
/// rings at its frequency, of the second or fourth order.
pub mod resonant_filter {
    use super::*;

    pub const FREQUENCY: usize = 0;
    pub const RESONANCE: usize = 1;
    pub const MIX: usize = 2;
    pub const TYPE: usize = 3;
    pub const ORDER: usize = 4;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Frequency", 20.0, 20000.0, 20000.0)
            .unit("Hz")
            .decimals(0)
            .log(),
        ParamSpec::num("Resonance", 0.0, 1.0, 0.0)
            .show(Show::Percent)
            .decimals(1),
        ParamSpec::num("Wet / Dry", 0.0, 1.0, 1.0)
            .show(Show::Percent)
            .decimals(1),
        ParamSpec::choice(
            "Filter type",
            0.0,
            &["Low Pass", "Band Pass", "High Pass"],
            0.0,
        ),
        ParamSpec::choice("Filter order", 0.0, &["Second", "Fourth"], 0.0),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[20000.0, 0.0, 1.0, 0.0, 0.0]),
        ("Bass Hum", &[135.0, 0.9, 1.0, 1.0, 1.0]),
        ("Big Boom Maker", &[60.0, 1.0, 1.0, 0.0, 0.0]),
        ("Chest Punch", &[150.0, 0.75, 1.0, 0.0, 0.0]),
        ("Ear Sugar", &[5750.0, 0.333, 1.0, 1.0, 0.0]),
        ("Hank on Escape Route", &[880.0, 0.25, 1.0, 1.0, 1.0]),
        ("High Howl", &[1500.0, 1.0, 1.0, 0.0, 0.0]),
        ("High Pierce", &[3600.0, 1.0, 1.0, 0.0, 0.0]),
        ("Light Chime", &[900.0, 0.75, 1.0, 0.0, 0.0]),
        ("Low Howl", &[150.0, 1.0, 1.0, 0.0, 0.0]),
        ("Mid Howl", &[500.0, 1.0, 1.0, 0.0, 0.0]),
        ("Muffler", &[350.0, 0.0, 1.0, 0.0, 0.0]),
        ("PipSqueaking", &[6000.0, 1.0, 1.0, 2.0, 1.0]),
        ("Ringing Chime", &[900.0, 1.0, 1.0, 1.0, 0.0]),
        ("Rumbly Growl", &[75.0, 0.5, 1.0, 0.0, 1.0]),
        ("Slicing Vocals", &[3000.0, 0.05, 1.0, 1.0, 1.0]),
        (
            "Teenage Late-Night Radio by the Bed",
            &[1500.0, 0.2, 1.0, 2.0, 0.0],
        ),
    ];
}

/// Parametric EQ: one shelf, band-pass or notch/boost, drawn as a
/// frequency response and applied as a long FIR filter through FFTs —
/// the longer, the finer in the lows.
pub mod parametric_eq {
    use super::*;

    pub const OUTPUT: usize = 0;
    pub const AMOUNT: usize = 1;
    pub const STYLE: usize = 2;
    pub const FREQUENCY: usize = 3;
    pub const WIDTH: usize = 4;
    pub const ACCURACY: usize = 5;

    /// Each accuracy's FFT length, in the order of its options.
    pub const FFT: [usize; 3] = [256, 1024, 4096];

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Output gain", -60.0, 20.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Amount", -60.0, 20.0, -4.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::choice(
            "Filter style",
            0.0,
            &[
                "Low-frequency shelf",
                "High-frequency shelf",
                "Band-pass",
                "Band-notch/boost",
            ],
            2.0,
        ),
        ParamSpec::num("Frequency", 20.0, 20000.0, 650.0)
            .unit("Hz")
            .decimals(0)
            .log(),
        ParamSpec::num("Width", 0.1, 5.0, 1.0)
            .unit("oct.")
            .decimals(1),
        ParamSpec::choice(
            "Accuracy",
            0.0,
            &[
                "Low (fast, good for high bands)",
                "Medium (great for mid and high bands)",
                "High (slow, great for all bands)",
            ],
            1.0,
        ),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, -4.0, 2.0, 650.0, 1.0, 1.0]),
        ("200 Hz high-pass", &[0.0, -15.0, 0.0, 200.0, 0.4, 2.0]),
        (
            "Hiss removal (cut frequencies above 6 kHz by 40 dB)",
            &[0.0, -40.0, 1.0, 6000.0, 0.5, 2.0],
        ),
        ("Phone line effect", &[0.0, -25.0, 2.0, 800.0, 2.0, 1.0]),
        (
            "Remove frequencies around 1500 Hz by 25 dB",
            &[0.0, -25.0, 3.0, 1500.0, 1.0, 2.0],
        ),
    ];
}

/// Chorus: one to three modulated delays in a row, each adding its copy
/// of the sound slightly out of tune and late.
pub mod chorus {
    use super::*;

    pub const INPUT: usize = 0;
    pub const DRY: usize = 1;
    pub const WET: usize = 2;
    pub const INVERT: usize = 3;
    pub const INVERT_FEEDBACK: usize = 4;
    pub const SIZE: usize = 5;
    pub const RATE: usize = 6;
    pub const DEPTH: usize = 7;
    pub const FEEDBACK: usize = 8;
    pub const DELAY: usize = 9;
    pub const ATTENUATE: usize = 10;
    pub const CUTOFF: usize = 11;
    pub const SYNC: usize = 12;
    pub const MULTIPLIER: usize = 13;
    pub const PERIOD: usize = 14;

    const FREE: &[(u8, u32)] = &[(SYNC as u8, OFF)];
    const SYNCED: &[(u8, u32)] = &[(SYNC as u8, ON)];
    const CUT: &[(u8, u32)] = &[(ATTENUATE as u8, ON)];

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Input gain", -60.0, 0.0, 0.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Dry out", -60.0, 0.0, -6.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Chorus out", -60.0, 0.0, -4.4)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::switch("Invert chorus phase", 0.0),
        ParamSpec::switch("Invert feedback phase", 0.0),
        ParamSpec::num("Chorus size", 1.0, 3.0, 3.0).decimals(0),
        ParamSpec::num("Modulation rate", 0.001, 20.0, 0.8)
            .unit("Hz")
            .decimals(3)
            .log()
            .when(FREE),
        ParamSpec::num("Modulation depth", 0.01, 1.0, 0.03)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Feedback", 0.0, 1.0, 0.0)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Chorus out delay", 0.1, 100.0, 40.0)
            .unit("ms")
            .decimals(1)
            .log(),
        ParamSpec::switch("Attenuate high frequencies", 0.0),
        ParamSpec::num("Attenuation frequency", 100.0, 10000.0, 5000.0)
            .unit("Hz")
            .decimals(0)
            .log()
            .when(CUT),
        ParamSpec::switch("Tempo sync", 0.0),
        ParamSpec::num("Period multiplier", 0.001, 32.0, 1.0)
            .decimals(3)
            .log()
            .when(SYNCED),
        ParamSpec::choice(
            "Modulation period",
            0.0,
            tempo::NOTES_OR_MEASURE,
            tempo::MEASURE,
        )
        .when(SYNCED),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, -6.0, -4.4, 0.0, 0.0, 3.0, 0.8, 0.03, 0.0, 40.0, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Fast flange 1", &[0.0, -3.4, -3.4, 0.0, 0.0, 1.0, 0.7, 1.0, 0.32, 1.2, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Fast flange 2", &[0.0, -5.7, -5.7, 0.0, 0.0, 1.0, 2.3, 0.16, 0.7, 5.0, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Slow flange 1", &[0.0, -4.9, -5.7, 0.0, 0.0, 1.0, 0.5, 0.75, 0.5, 0.8, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Slow flange 2", &[-2.0, -8.0, -4.4, 0.0, 0.0, 1.0, 0.5, 1.0, 0.6, 1.2, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Slow flange 3", &[0.0, -8.0, -3.1, 0.0, 0.0, 1.0, 0.7, 0.8, 0.2, 0.5, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Slow flange 4", &[0.0, -4.4, -3.1, 0.0, 0.0, 1.0, 0.2, 0.75, 0.0, 0.6, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Chorus/vibrato 1", &[0.0, -3.7, -3.7, 0.0, 0.0, 1.0, 0.5, 1.0, 0.57, 10.0, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Chorus/vibrato 2", &[0.0, -3.7, -6.9, 0.0, 0.0, 1.0, 7.0, 0.08, 0.37, 29.2, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Chorus 1", &[0.0, -6.0, -4.4, 0.0, 0.0, 3.0, 0.8, 0.03, 0.0, 40.0, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Chorus 2", &[0.0, -6.0, -6.0, 0.0, 0.0, 2.0, 0.5, 0.08, 0.15, 30.0, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Chorus 3", &[0.0, -6.0, -6.0, 0.0, 0.0, 3.0, 0.5, 0.08, 0.16, 40.5, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Chorus 4", &[0.0, -4.9, -5.2, 0.0, 0.0, 2.0, 0.5, 0.26, 0.26, 9.0, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Chorus 5", &[0.0, -4.9, -7.7, 0.0, 0.0, 2.0, 2.1, 0.12, 0.37, 29.2, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Cheap TV", &[-2.0, 0.0, 0.0, 1.0, 0.0, 2.0, 0.1, 1.0, 0.0, 0.1, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Echo-vibrato 1", &[0.0, -9.2, -14.0, 0.0, 0.0, 2.0, 6.0, 0.08, 0.15, 100.0, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Echo-vibrato 2", &[0.0, -9.2, -6.0, 0.0, 0.0, 1.0, 6.0, 0.05, 0.0, 100.0, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Echo-vibrato 3", &[0.0, -9.2, -8.0, 0.0, 0.0, 3.0, 20.0, 0.01, 0.5, 100.0, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Echo-vibrato 4", &[0.0, -9.2, -6.0, 0.0, 0.0, 1.0, 20.0, 0.01, 0.5, 100.0, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Vibrato-verb 1", &[0.0, -9.2, -14.0, 0.0, 0.0, 1.0, 5.0, 0.01, 0.6, 100.0, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Vibrato-verb 2", &[0.0, -9.2, -9.1, 0.0, 0.0, 3.0, 7.0, 0.01, 0.5, 100.0, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Vibrato-verb 3", &[0.0, -9.2, -6.0, 0.0, 0.0, 3.0, 5.6, 0.03, 0.6, 100.0, 0.0, 5000.0, 0.0, 1.0, 18.0]),
        ("Hard flange", &[-2.0, -3.1, -3.1, 0.0, 0.0, 1.0, 0.5, 0.75, 0.9, 0.8, 0.0, 5000.0, 0.0, 1.0, 18.0]),
    ];
}

/// Flange/Wah-wah: a flange (a short modulated delay fed back), a phaser
/// (an all-pass swept up and down, mixed with the sound) or a wah-wah
/// (the same sweep as a band-pass).
pub mod flange_wah_wah {
    use super::*;

    pub const EFFECT: usize = 0;
    pub const DRY: usize = 1;
    pub const WET: usize = 2;
    pub const RATE: usize = 3;
    pub const DEPTH: usize = 4;
    pub const CENTER: usize = 5;
    pub const RESONANCE: usize = 6;
    pub const SYNC: usize = 7;
    pub const MULTIPLIER: usize = 8;
    pub const PERIOD: usize = 9;

    const FREE: &[(u8, u32)] = &[(SYNC as u8, OFF)];
    const SYNCED: &[(u8, u32)] = &[(SYNC as u8, ON)];

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::choice("Effect", 0.0, &["Flange", "Phaser", "Wah-wah"], 0.0),
        ParamSpec::num("Dry out", -80.0, 12.0, -80.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Wet out", -80.0, 12.0, 0.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Rate", 0.001, 20.0, 10.0)
            .unit("Hz")
            .decimals(3)
            .log()
            .when(FREE),
        ParamSpec::num("Depth", 0.01, 1.0, 0.1)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Center frequency", 100.0, 5000.0, 100.0)
            .unit("Hz")
            .decimals(0)
            .log(),
        ParamSpec::num("Resonance", 0.01, 1.0, 0.01)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::switch("Tempo sync", 0.0),
        ParamSpec::num("Period multiplier", 0.001, 32.0, 1.0)
            .decimals(3)
            .log()
            .when(SYNCED),
        ParamSpec::choice("Period", 0.0, tempo::NOTES_OR_MEASURE, tempo::MEASURE).when(SYNCED),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, -80.0, 0.0, 10.0, 0.1, 100.0, 0.01, 0.0, 1.0, 18.0]),
        ("Phaser 1", &[1.0, -80.0, 0.0, 0.5, 0.5, 1000.0, 0.5, 0.0, 1.0, 18.0]),
        ("Phaser 2", &[1.0, -80.0, 0.0, 4.5, 0.5, 1000.0, 0.5, 0.0, 1.0, 18.0]),
        ("Phaser 3", &[1.0, -80.0, 0.0, 10.0, 0.5, 2000.0, 0.4, 0.0, 1.0, 18.0]),
        ("Wah-wah 1", &[2.0, -80.0, 0.0, 1.0, 0.5, 1000.0, 0.5, 0.0, 1.0, 18.0]),
        ("Wah-wah 2", &[2.0, -80.0, 0.0, 2.0, 0.5, 1000.0, 0.8, 0.0, 1.0, 18.0]),
        ("Wah-wah 3", &[2.0, -15.0, 0.0, 0.2, 0.5, 500.0, 1.0, 0.0, 1.0, 18.0]),
        ("Wah-wah 4", &[2.0, -80.0, 0.0, 0.5, 0.7, 3000.0, 0.5, 0.0, 1.0, 18.0]),
        ("Wah-wah 5", &[2.0, -80.0, 0.0, 10.0, 0.5, 2000.0, 0.5, 0.0, 1.0, 18.0]),
        ("1 Hz flange", &[0.0, -5.0, -6.0, 1.0, 0.85, 100.0, 0.01, 0.0, 1.0, 18.0]),
        ("Fast flange", &[0.0, -6.0, -6.0, 5.0, 0.5, 100.0, 0.01, 0.0, 1.0, 18.0]),
        ("Slow flange", &[0.0, -3.0, -2.0, 0.1, 1.0, 100.0, 0.01, 0.0, 1.0, 18.0]),
        ("Slow flange 2", &[0.0, -2.5, -2.5, 0.5, 0.85, 100.0, 0.01, 0.0, 1.0, 18.0]),
        ("Warble 1", &[0.0, -5.0, -5.0, 20.0, 0.85, 100.0, 0.01, 0.0, 1.0, 18.0]),
        ("Warble 2", &[0.0, -5.0, -5.0, 20.0, 0.2, 100.0, 0.01, 0.0, 1.0, 18.0]),
    ];
}

/// Reverb: a room of comb filters (19 of them, by mode) with three early
/// reflections (10 styles), its decay time, pre-delay and bass and high
/// cuts.
pub mod reverb {
    use super::*;

    pub const MODE: usize = 0;
    pub const DRY: usize = 1;
    pub const WET: usize = 2;
    pub const EARLY: usize = 3;
    pub const STYLE: usize = 4;
    pub const DECAY: usize = 5;
    pub const PREDELAY: usize = 6;
    pub const BASS: usize = 7;
    pub const BASS_FREQ: usize = 8;
    pub const HIGH: usize = 9;
    pub const HIGH_FREQ: usize = 10;

    const BASS_CUT: &[(u8, u32)] = &[(BASS as u8, ON)];
    const HIGH_CUT: &[(u8, u32)] = &[(HIGH as u8, ON)];

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::choice(
            "Reverberation mode",
            0.0,
            &[
                "Rich Hall",
                "Wide Open Hall",
                "Concert Hall",
                "Deep Hall",
                "Long Hall",
                "Medium Hall 1",
                "Medium Hall 2",
                "Warm Space",
                "Cavernous Space",
                "Bright Corridor",
                "Bright Small Room",
                "Bright Medium Room",
                "Medium Room",
                "Subtle Ambience",
                "Live Ambience",
                "Warm Ambience",
                "Plate 1",
                "Plate 2 Ringy",
                "Metal Tank",
            ],
            0.0,
        ),
        ParamSpec::num("Dry out", -80.0, 0.0, 0.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Reverb out", -80.0, 0.0, -35.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Early out", -80.0, 0.0, -20.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::choice(
            "Early reflection style",
            0.0,
            &[
                "Mode 1 - 6 ms",
                "Mode 2 - 8 ms",
                "Mode 3 - 10 ms",
                "Mode 4 - 16 ms",
                "Mode 5 - 20 ms",
                "Mode 6 - 36 ms",
                "Mode 7 - 8 ms pan right",
                "Mode 8 - 8 ms pan left",
                "Mode 9 - 16 ms pan right",
                "Mode 10 - 16 ms pan left",
            ],
            3.0,
        ),
        ParamSpec::num("Decay time", 0.5, 5.0, 2.5)
            .unit("s")
            .decimals(1),
        ParamSpec::num("Pre-delay", 0.0, 200.0, 30.0)
            .unit("ms")
            .decimals(0),
        ParamSpec::switch("Attenuate bass frequencies", 1.0),
        ParamSpec::num("Bass attenuation frequency", 10.0, 5000.0, 20.0)
            .unit("Hz")
            .decimals(0)
            .log()
            .when(BASS_CUT),
        ParamSpec::switch("Attenuate high frequencies", 1.0),
        ParamSpec::num("High attenuation frequency", 100.0, 15000.0, 12000.0)
            .unit("Hz")
            .decimals(0)
            .log()
            .when(HIGH_CUT),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, 0.0, -35.0, -20.0, 3.0, 2.5, 30.0, 1.0, 20.0, 1.0, 12000.0]),
        ("Bright hall", &[0.0, 0.0, -35.0, -20.0, 3.0, 2.5, 30.0, 1.0, 20.0, 1.0, 12000.0]),
        ("Cathedral", &[8.0, 0.0, -24.0, -20.0, 5.0, 3.0, 70.0, 1.0, 20.0, 1.0, 7000.0]),
        ("Dense room", &[10.0, 0.0, -25.0, -20.0, 1.0, 0.6, 1.0, 1.0, 20.0, 1.0, 5000.0]),
        ("Long hall", &[4.0, 0.0, -35.0, -30.0, 2.0, 2.5, 60.0, 1.0, 20.0, 1.0, 5000.0]),
        ("Medium hall", &[2.0, 0.0, -30.0, -30.0, 1.0, 1.2, 5.0, 1.0, 20.0, 1.0, 5000.0]),
        ("Medium room", &[13.0, 0.0, -20.0, -80.0, 0.0, 0.5, 1.0, 1.0, 20.0, 1.0, 5000.0]),
        ("Metal tank", &[18.0, 0.0, -16.0, -20.0, 5.0, 2.5, 10.0, 1.0, 400.0, 1.0, 3000.0]),
        ("Plate", &[16.0, 0.0, -35.0, -20.0, 3.0, 2.0, 15.0, 1.0, 20.0, 0.0, 5000.0]),
        ("Sewer", &[18.0, 0.0, -16.0, -20.0, 3.0, 2.0, 15.0, 1.0, 20.0, 1.0, 10000.0]),
        ("Small hall", &[2.0, 0.0, -24.0, -20.0, 1.0, 1.0, 10.0, 1.0, 20.0, 1.0, 8000.0]),
        ("Very long hall", &[4.0, 0.0, -41.8, -30.0, 5.0, 5.0, 45.0, 1.0, 100.0, 1.0, 10000.0]),
    ];
}

/// Track Compressor: a compressor (to a limiter) with input and output
/// gains, automatic gain compensation and a rounded corner.
pub mod track_compressor {
    use super::*;

    pub const INPUT: usize = 0;
    pub const OUTPUT: usize = 1;
    pub const THRESHOLD: usize = 2;
    pub const RATIO: usize = 3;
    pub const ATTACK: usize = 4;
    pub const RELEASE: usize = 5;
    pub const AUTO_GAIN: usize = 6;
    pub const SMOOTH: usize = 7;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Input gain", -60.0, 20.0, 0.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Output gain", -60.0, 20.0, 0.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Threshold", -60.0, 0.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Amount (x:1)", 1.0, 500.0, 1.0)
            .decimals(1)
            .log(),
        ParamSpec::num("Attack", 0.0, 500.0, 15.0)
            .unit("ms")
            .decimals(0),
        ParamSpec::num("Release", 0.0, 5000.0, 250.0)
            .unit("ms")
            .decimals(0),
        ParamSpec::switch("Auto gain compensation", 1.0),
        ParamSpec::switch("Smooth saturation", 0.0),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, 0.0, 0.0, 1.0, 15.0, 250.0, 1.0, 0.0]),
        ("Soft Limiter", &[0.0, 0.0, -6.0, 25.0, 0.0, 250.0, 0.0, 1.0]),
        ("1.5:1 compression starting at -24 dB", &[0.0, 0.0, -24.0, 1.5, 15.0, 250.0, 0.0, 1.0]),
        ("2:1 compression starting at -18 dB", &[0.0, 0.0, -18.0, 2.0, 15.0, 250.0, 1.0, 0.0]),
        ("3:1 compression starting at -15 dB", &[0.0, 0.0, -15.0, 3.0, 15.0, 250.0, 1.0, 1.0]),
        ("6:1 compression starting at -12 dB", &[0.0, 0.0, -12.0, 6.0, 15.0, 250.0, 1.0, 1.0]),
        ("10:1 compression starting at -9 dB", &[0.0, 0.0, -9.0, 10.0, 15.0, 250.0, 1.0, 1.0]),
        ("Limit levels to -6 dB (hard limiter)", &[0.0, 0.0, -6.0, 50.0, 0.0, 250.0, 0.0, 0.0]),
        ("2:1 compression starting at -18 dB (ACX)", &[0.0, 0.0, -18.0, 2.0, 15.0, 250.0, 1.0, 0.0]),
    ];
}

/// Track EQ: four bands (low shelf, band or high shelf, each switched on
/// or off), and the channels it works on.
pub mod track_eq {
    use super::*;

    pub const ENABLE: usize = 0;
    pub const MODE: usize = 1;
    pub const FREQUENCY: usize = 2;
    pub const GAIN: usize = 3;
    pub const BANDWIDTH: usize = 4;
    pub const ROLLOFF: usize = 5;
    /// Each band's parameters, from its place times this.
    pub const PER_BAND: usize = 6;
    /// The channels' switches: left, right, centre, left and right
    /// surround, LFE.
    pub const CHANNELS: usize = 4 * PER_BAND;

    const MODES: &[&str] = &["Low Shelf", "Band", "High Shelf"];
    const ROLLOFFS: &[&str] = &["6 dB/oct.", "12 dB/oct.", "18 dB/oct.", "24 dB/oct."];

    /// A band's parameters, its place `b` (from 0), its default frequency,
    /// mode and rolloff.
    const fn band(
        b: usize,
        group: &'static str,
        hz: f32,
        mode: f32,
        rolloff: f32,
    ) -> [ParamSpec; PER_BAND] {
        // Each band's own conditions.
        let (on, peak, shelf) = (BAND_ON[b], PEAK[b], SHELF[b]);
        [
            ParamSpec::switch("Enable", 1.0).group(group),
            ParamSpec::choice("Mode", 0.0, MODES, mode).when(on),
            ParamSpec::num("Frequency", 20.0, 20000.0, hz)
                .unit("Hz")
                .decimals(0)
                .log()
                .when(on),
            ParamSpec::num("Gain", -25.0, 15.0, 0.0)
                .unit("dB")
                .decimals(1)
                .when(on),
            ParamSpec::num("Bandwidth", 0.1, 4.0, 1.0)
                .unit("oct.")
                .decimals(1)
                .when(peak),
            ParamSpec::choice("Rolloff", 0.0, ROLLOFFS, rolloff).when(shelf),
        ]
    }

    const fn on(b: usize) -> (u8, u32) {
        ((b * PER_BAND + ENABLE) as u8, ON)
    }

    const fn mode(b: usize, mask: u32) -> (u8, u32) {
        ((b * PER_BAND + MODE) as u8, mask)
    }

    const BAND_ON: [&[(u8, u32)]; 4] = [&[on(0)], &[on(1)], &[on(2)], &[on(3)]];
    const PEAK: [&[(u8, u32)]; 4] = [
        &[on(0), mode(0, either(1, 1))],
        &[on(1), mode(1, either(1, 1))],
        &[on(2), mode(2, either(1, 1))],
        &[on(3), mode(3, either(1, 1))],
    ];
    const SHELF: [&[(u8, u32)]; 4] = [
        &[on(0), mode(0, either(0, 2))],
        &[on(1), mode(1, either(0, 2))],
        &[on(2), mode(2, either(0, 2))],
        &[on(3), mode(3, either(0, 2))],
    ];

    const B1: [ParamSpec; PER_BAND] = band(0, "Band 1", 100.0, 0.0, 1.0);
    const B2: [ParamSpec; PER_BAND] = band(1, "Band 2", 300.0, 1.0, 0.0);
    const B3: [ParamSpec; PER_BAND] = band(2, "Band 3", 1000.0, 1.0, 0.0);
    const B4: [ParamSpec; PER_BAND] = band(3, "Band 4", 5000.0, 2.0, 1.0);

    pub const PARAMS: &[ParamSpec] = &[
        B1[0],
        B1[1],
        B1[2],
        B1[3],
        B1[4],
        B1[5],
        B2[0],
        B2[1],
        B2[2],
        B2[3],
        B2[4],
        B2[5],
        B3[0],
        B3[1],
        B3[2],
        B3[3],
        B3[4],
        B3[5],
        B4[0],
        B4[1],
        B4[2],
        B4[3],
        B4[4],
        B4[5],
        ParamSpec::switch("Left", 1.0).group("Channels"),
        ParamSpec::switch("Right", 1.0),
        ParamSpec::switch("Center", 1.0),
        ParamSpec::switch("Left surround", 1.0),
        ParamSpec::switch("Right surround", 1.0),
        ParamSpec::switch("LFE", 1.0),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[1.0, 0.0, 100.0, 0.0, 1.0, 1.0, 1.0, 1.0, 300.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1000.0, 0.0, 1.0, 0.0, 1.0, 2.0, 5000.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
        ("15 dB notch centered at 1000 Hz", &[0.0, 0.0, 100.0, 0.0, 1.0, 0.0, 0.0, 1.0, 300.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1000.0, -15.0, 1.0, 0.0, 0.0, 2.0, 5000.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
        ("60 Hz hum notch using four stacked filters", &[1.0, 1.0, 60.0, -25.0, 0.3, 0.0, 1.0, 1.0, 60.0, -25.0, 0.3, 0.0, 1.0, 1.0, 60.0, -25.0, 0.3, 0.0, 1.0, 1.0, 60.0, -25.0, 0.3, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
        ("Add air (3 dB peak at 15 kHz)", &[0.0, 1.0, 100.0, 0.0, 1.0, 0.0, 0.0, 1.0, 300.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1000.0, 0.0, 1.0, 0.0, 1.0, 1.0, 15000.0, 3.0, 0.7, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
        ("Add air (boost above 16k by 6 dB)", &[0.0, 0.0, 100.0, 0.0, 1.0, 0.0, 0.0, 1.0, 300.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1000.0, 0.0, 1.0, 0.0, 1.0, 2.0, 16000.0, 6.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
        ("Anti-DTMF Talkoff", &[1.0, 1.0, 1209.0, -25.0, 0.3, 0.0, 1.0, 1.0, 1336.0, -25.0, 0.3, 0.0, 1.0, 1.0, 1447.0, -25.0, 0.3, 0.0, 1.0, 1.0, 1633.0, -25.0, 0.3, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
        ("Boost bass frequencies below 250 Hz by 4 dB", &[1.0, 0.0, 250.0, 4.0, 1.0, 0.0, 0.0, 1.0, 300.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1000.0, 0.0, 1.0, 0.0, 0.0, 2.0, 5000.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
        ("Boost frequencies above 7 kHz", &[0.0, 0.0, 100.0, 0.0, 1.0, 0.0, 0.0, 1.0, 300.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1000.0, 0.0, 1.0, 0.0, 1.0, 2.0, 7000.0, 3.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
        ("Fletcher-Munson curve (low volume compensation)", &[1.0, 0.0, 200.0, 6.0, 1.0, 0.0, 0.0, 1.0, 300.0, 0.0, 1.0, 0.0, 1.0, 1.0, 4000.0, -12.0, 1.0, 0.0, 1.0, 2.0, 8000.0, 6.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
        ("Hiss cut (cut above 8 kHz by 6 dB)", &[0.0, 0.0, 100.0, 0.0, 1.0, 0.0, 0.0, 1.0, 300.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1000.0, 0.0, 1.0, 0.0, 1.0, 2.0, 8000.0, -6.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
        ("Remove low rumble below 80 Hz", &[1.0, 0.0, 80.0, -25.0, 1.0, 0.0, 0.0, 1.0, 300.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1000.0, 0.0, 1.0, 0.0, 0.0, 2.0, 5000.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
        ("Remove very low and inaudible frequencies below 20 Hz", &[1.0, 0.0, 20.0, -25.0, 1.0, 0.0, 0.0, 1.0, 300.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1000.0, 0.0, 1.0, 0.0, 0.0, 2.0, 5000.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
    ];
}

/// Paragraphic EQ: four peak/notch bands and optional low and high
/// shelves, wet and dry.
pub mod paragraphic_eq {
    use super::*;

    pub const DRY: usize = 0;
    pub const WET: usize = 1;
    /// The bands' first parameter, each band's from its place times
    /// [`PER_BAND`] after it.
    pub const BANDS: usize = 2;
    pub const GAIN: usize = 0;
    pub const WIDTH: usize = 1;
    pub const CENTER: usize = 2;
    pub const PER_BAND: usize = 3;
    /// Each shelf's switch, then its frequency and gain.
    pub const LOW_SHELF: usize = BANDS + 4 * PER_BAND;
    pub const HIGH_SHELF: usize = LOW_SHELF + 3;

    const LOW_ON: &[(u8, u32)] = &[(LOW_SHELF as u8, ON)];
    const HIGH_ON: &[(u8, u32)] = &[(HIGH_SHELF as u8, ON)];

    const fn band(group: &'static str, hz: f32) -> [ParamSpec; PER_BAND] {
        [
            ParamSpec::num("Gain", -25.0, 25.0, 0.0)
                .unit("dB")
                .decimals(1)
                .group(group),
            ParamSpec::num("Width", 0.3, 2.5, 1.0)
                .unit("oct.")
                .decimals(1),
            ParamSpec::num("Center", 20.0, 15000.0, hz)
                .unit("Hz")
                .decimals(0)
                .log(),
        ]
    }

    const B1: [ParamSpec; PER_BAND] = band("Band 1", 100.0);
    const B2: [ParamSpec; PER_BAND] = band("Band 2", 300.0);
    const B3: [ParamSpec; PER_BAND] = band("Band 3", 1000.0);
    const B4: [ParamSpec; PER_BAND] = band("Band 4", 5000.0);

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Dry out", -25.0, 25.0, -25.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Wet out", -25.0, 25.0, 0.0)
            .unit("dB")
            .decimals(1),
        B1[0],
        B1[1],
        B1[2],
        B2[0],
        B2[1],
        B2[2],
        B3[0],
        B3[1],
        B3[2],
        B4[0],
        B4[1],
        B4[2],
        ParamSpec::switch("Enable low shelf", 0.0).group("Low shelf"),
        ParamSpec::num("Frequency", 20.0, 20000.0, 300.0)
            .unit("Hz")
            .decimals(0)
            .log()
            .when(LOW_ON),
        ParamSpec::num("Gain", -25.0, 25.0, 0.0)
            .unit("dB")
            .decimals(1)
            .when(LOW_ON),
        ParamSpec::switch("Enable high shelf", 0.0).group("High shelf"),
        ParamSpec::num("Frequency", 20.0, 20000.0, 500.0)
            .unit("Hz")
            .decimals(0)
            .log()
            .when(HIGH_ON),
        ParamSpec::num("Gain", -25.0, 25.0, 0.0)
            .unit("dB")
            .decimals(1)
            .when(HIGH_ON),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[-25.0, 0.0, 0.0, 1.0, 100.0, 0.0, 1.0, 300.0, 0.0, 1.0, 1000.0, 0.0, 1.0, 5000.0, 0.0, 300.0, 0.0, 0.0, 500.0, 0.0]),
        ("15 dB notch centered at 1000 Hz", &[-25.0, 0.0, 0.0, 1.0, 100.0, 0.0, 1.0, 300.0, -15.0, 1.0, 1000.0, 0.0, 1.0, 5000.0, 0.0, 300.0, 0.0, 0.0, 300.0, 0.0]),
        ("60 Hz hum notch using four stacked filters", &[-25.0, 0.0, -25.0, 0.3, 60.0, -25.0, 0.3, 60.0, -25.0, 0.3, 60.0, -25.0, 0.3, 60.0, 0.0, 300.0, 0.0, 0.0, 300.0, 0.0]),
        ("Add air (3 dB peak at 15 kHz)", &[-25.0, 0.0, 0.0, 1.0, 100.0, 0.0, 1.0, 300.0, 0.0, 1.0, 1000.0, 3.0, 0.7, 15000.0, 0.0, 300.0, 0.0, 0.0, 300.0, 0.0]),
        ("Add air (boost above 16k by 6 dB)", &[-25.0, 0.0, 0.0, 1.0, 100.0, 0.0, 1.0, 300.0, 0.0, 1.0, 1000.0, 0.0, 1.0, 5000.0, 0.0, 300.0, 0.0, 1.0, 16000.0, 6.0]),
        ("Anti-DTMF talk-off", &[-25.0, 0.0, -25.0, 0.3, 1209.0, -25.0, 0.3, 1336.0, -25.0, 0.3, 1447.0, -25.0, 0.3, 1633.0, 0.0, 300.0, 0.0, 0.0, 300.0, 0.0]),
        ("Boost bass frequencies below 250 Hz by 4 dB", &[-25.0, 0.0, 0.0, 1.0, 100.0, 0.0, 1.0, 300.0, 0.0, 1.0, 1000.0, 0.0, 1.0, 5000.0, 1.0, 250.0, 4.0, 0.0, 300.0, 0.0]),
        ("Boost frequencies above 7 kHz", &[-25.0, 0.0, 0.0, 1.0, 100.0, 0.0, 1.0, 300.0, 0.0, 1.0, 1000.0, 0.0, 1.0, 5000.0, 0.0, 300.0, 0.0, 1.0, 7000.0, 3.0]),
        ("Fletcher-Munson curve (low volume compensation)", &[-25.0, 0.0, 0.0, 1.0, 100.0, 0.0, 1.0, 300.0, -12.0, 1.0, 4000.0, 0.0, 1.0, 5000.0, 1.0, 200.0, 6.0, 1.0, 8000.0, 6.0]),
        ("Hiss cut (cut above 8 kHz by 6 dB)", &[-25.0, 0.0, 0.0, 1.0, 100.0, 0.0, 1.0, 300.0, 0.0, 1.0, 1000.0, 0.0, 1.0, 5000.0, 0.0, 300.0, 0.0, 1.0, 8000.0, -6.0]),
        ("Remove low rumble below 80 Hz", &[-25.0, 0.0, 0.0, 1.0, 100.0, 0.0, 1.0, 300.0, 0.0, 1.0, 1000.0, 0.0, 1.0, 5000.0, 1.0, 80.0, -25.0, 0.0, 300.0, 0.0]),
        ("Remove very low and inaudible frequencies below 20 Hz", &[-25.0, 0.0, 0.0, 1.0, 100.0, 0.0, 1.0, 300.0, 0.0, 1.0, 1000.0, 0.0, 1.0, 5000.0, 1.0, 20.0, -25.0, 0.0, 300.0, 0.0]),
    ];
}

/// Multi-Tap Delay: up to eight taps of one delay line, each with its
/// time, level and place, the line fed back, modulated and darkened.
pub mod multi_tap_delay {
    use super::*;

    pub const INPUT: usize = 0;
    pub const DRY: usize = 1;
    pub const WET: usize = 2;
    pub const RATE: usize = 3;
    pub const DEPTH: usize = 4;
    pub const FEEDBACK: usize = 5;
    pub const LOW_PASS: usize = 6;
    pub const LOW_FREQ: usize = 7;
    pub const TAPS: usize = 8;
    /// The taps' first parameter, each tap's from its place times
    /// [`PER_TAP`] after it.
    pub const TAP: usize = 9;
    pub const GAIN: usize = 0;
    pub const DELAY: usize = 1;
    pub const PAN: usize = 2;
    pub const PER_TAP: usize = 3;

    const LOW_ON: &[(u8, u32)] = &[(LOW_PASS as u8, ON)];

    /// Tap `k`'s parameters (from 1): live while there are `k` taps or more.
    const fn tap(k: usize, group: &'static str, ms: f32) -> [ParamSpec; PER_TAP] {
        let when = WHEN[k - 1];
        [
            ParamSpec::num("Tap gain", -1.0, 1.0, 1.0)
                .show(Show::Percent)
                .decimals(0)
                .group(group)
                .when(when),
            ParamSpec::num("Delay", 1.0, 2500.0, ms)
                .unit("ms")
                .decimals(0)
                .log()
                .when(when),
            ParamSpec::num("Pan", -1.0, 1.0, 0.0)
                .show(Show::Pan)
                .when(when),
        ]
    }

    const WHEN: [&[(u8, u32)]; 8] = [
        &[(TAPS as u8, at_least(1))],
        &[(TAPS as u8, at_least(2))],
        &[(TAPS as u8, at_least(3))],
        &[(TAPS as u8, at_least(4))],
        &[(TAPS as u8, at_least(5))],
        &[(TAPS as u8, at_least(6))],
        &[(TAPS as u8, at_least(7))],
        &[(TAPS as u8, at_least(8))],
    ];

    const T1: [ParamSpec; PER_TAP] = tap(1, "Tap 1", 100.0);
    const T2: [ParamSpec; PER_TAP] = tap(2, "Tap 2", 1.0);
    const T3: [ParamSpec; PER_TAP] = tap(3, "Tap 3", 1.0);
    const T4: [ParamSpec; PER_TAP] = tap(4, "Tap 4", 1.0);
    const T5: [ParamSpec; PER_TAP] = tap(5, "Tap 5", 1.0);
    const T6: [ParamSpec; PER_TAP] = tap(6, "Tap 6", 1.0);
    const T7: [ParamSpec; PER_TAP] = tap(7, "Tap 7", 1.0);
    const T8: [ParamSpec; PER_TAP] = tap(8, "Tap 8", 1.0);

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Input gain", -80.0, 0.0, 0.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Dry out", -80.0, 0.0, -6.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Wet out", -80.0, 0.0, -6.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Mod. rate", 0.1, 10.0, 0.1)
            .unit("Hz")
            .decimals(1)
            .log(),
        ParamSpec::num("Mod. depth", 0.0, 1.0, 0.0)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Feedback", 0.0, 1.5, 0.0)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::switch("Low-pass", 0.0),
        ParamSpec::num("Low-pass start frequency", 100.0, 20000.0, 5000.0)
            .unit("Hz")
            .decimals(0)
            .log()
            .when(LOW_ON),
        ParamSpec::num("Number of taps", 1.0, 8.0, 1.0).decimals(0),
        T1[0],
        T1[1],
        T1[2],
        T2[0],
        T2[1],
        T2[2],
        T3[0],
        T3[1],
        T3[2],
        T4[0],
        T4[1],
        T4[2],
        T5[0],
        T5[1],
        T5[2],
        T6[0],
        T6[1],
        T6[2],
        T7[0],
        T7[1],
        T7[2],
        T8[0],
        T8[1],
        T8[2],
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, -6.0, -6.0, 0.1, 0.0, 0.0, 0.0, 5000.0, 1.0, 1.0, 100.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0]),
        ("Small room 1", &[0.0, -9.2, -2.5, 1.5, 0.05, 0.3, 0.0, 5000.0, 6.0, -0.48, 17.0, 0.24, 0.16, 40.0, -0.18, -0.11, 85.0, 0.14, 0.05, 197.0, 0.38, -0.02, 275.0, 0.06, 0.01, 309.0, 0.38, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0]),
        ("Small hall 1", &[0.0, -1.4, -3.7, 1.5, 0.1, 0.4, 0.0, 5000.0, 8.0, 0.39, 34.0, 0.0, 0.21, 57.0, -0.15, -0.08, 113.0, -0.26, -0.17, 135.0, -0.69, -0.11, 174.0, -0.31, 0.12, 214.0, -0.64, 0.05, 281.0, -0.39, 0.03, 348.0, -0.5]),
        ("Chorus-echo", &[0.0, -14.0, 0.0, 0.1, 0.0, 0.0, 0.0, 5000.0, 8.0, -0.26, 6.0, 0.24, 0.25, 13.0, -0.18, -0.6, 19.0, 0.14, 0.57, 33.0, 0.38, -0.24, 101.0, 0.06, 0.1, 163.0, 0.38, -0.24, 191.0, 0.0, 0.28, 207.0, 0.18]),
        ("Chorus-reverse verb 2", &[0.0, -80.0, 0.0, 0.1, 0.0, 0.0, 0.0, 5000.0, 5.0, 0.12, 1.0, 0.24, -0.17, 12.0, -0.18, 0.37, 20.0, 0.14, -0.42, 32.0, -0.2, 0.52, 40.0, 0.22, -0.49, 57.0, 0.0, 0.66, 85.0, -0.56, -0.85, 113.0, 0.44]),
        ("Chorus-reverse verb 3", &[0.0, -26.0, -3.1, 0.1, 0.0, 0.0, 0.0, 5000.0, 5.0, 0.09, 14.0, -0.38, -0.11, 32.0, -0.59, 0.14, 59.0, -0.43, -0.24, 86.0, -0.6, 0.28, 113.0, -0.39, -0.35, 126.0, -0.5, 0.64, 144.0, -0.56, -0.85, 162.0, -0.33]),
        ("Chorus-reverse verb 1", &[0.0, -80.0, -3.1, 0.1, 0.0, 0.0, 0.0, 5000.0, 5.0, 0.05, 1.0, 0.24, -0.11, 12.0, -0.18, 0.14, 23.0, 0.14, -0.24, 29.0, -0.2, 0.28, 37.0, 0.22, -0.35, 44.0, 0.0, 0.64, 56.0, -0.56, -0.85, 65.0, 0.44]),
        ("Low-frequency verb", &[0.0, -4.4, 0.0, 0.1, 0.0, 0.7, 1.0, 500.0, 8.0, 0.35, 17.0, 0.24, -0.24, 187.0, -0.18, 0.1, 323.0, 0.14, -0.08, 361.0, -0.2, -0.05, 399.0, 0.22, -0.02, 427.0, 0.0, 0.01, 473.0, -0.56, -0.01, 500.0, 0.0]),
        ("Chorus-verb", &[0.0, -10.5, -6.0, 0.1, 0.0, 0.03, 0.0, 5000.0, 8.0, 0.35, 3.0, 0.24, -0.31, 7.0, -0.18, 0.37, 13.0, 0.14, -0.4, 19.0, -0.2, 0.3, 27.0, 0.22, -0.35, 33.0, 0.0, 0.34, 41.0, -0.56, -0.51, 53.0, 0.44]),
        ("Stadium", &[0.0, -6.0, 0.0, 1.5, 0.05, 0.4, 1.0, 2000.0, 2.0, 0.54, 366.0, 0.24, -0.1, 500.0, -0.18, 0.05, 500.0, 0.14, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0]),
        ("Medium hall 1", &[0.0, -9.2, -6.0, 1.0, 0.1, 0.4, 1.0, 4500.0, 5.0, -0.58, 71.0, 0.3, -0.28, 177.0, -0.31, -0.12, 315.0, 0.18, 0.05, 374.0, 0.12, -0.05, 424.0, -0.2, 0.04, 432.0, -0.28, 0.11, 453.0, 0.22, -0.09, 500.0, 0.0]),
        ("Large hall 2", &[0.0, -1.4, -9.1, 2.0, 0.15, 0.4, 1.0, 4500.0, 6.0, -0.51, 100.0, 0.3, -0.41, 130.0, -0.31, -0.22, 185.0, 0.18, -0.32, 269.0, 0.12, -0.09, 424.0, -0.2, 0.04, 437.0, -0.28, 0.11, 453.0, 0.22, -0.09, 500.0, 0.0]),
        ("Large hall 1", &[0.0, -9.2, -6.0, 1.0, 0.1, 0.6, 1.0, 4500.0, 8.0, 0.54, 90.0, -0.38, -0.14, 108.0, -0.59, 0.13, 212.0, -0.43, 0.05, 306.0, -0.65, -0.06, 361.0, -0.55, 0.05, 468.0, 0.0, -0.02, 482.0, 0.0, 0.05, 500.0, 0.0]),
        ("Large hall 3", &[0.0, -9.2, -6.0, 1.0, 0.1, 0.5, 1.0, 4500.0, 5.0, -0.58, 100.0, 0.3, -0.28, 177.0, -0.31, -0.12, 315.0, 0.18, 0.11, 403.0, 0.12, -0.05, 474.0, -0.2, 0.04, 432.0, -0.28, 0.11, 453.0, 0.22, -0.09, 500.0, 0.0]),
        ("Medium hall 2", &[0.0, -9.2, -6.0, 1.0, 0.1, 0.4, 1.0, 3500.0, 5.0, -0.58, 63.0, 0.3, -0.22, 177.0, -0.31, -0.05, 315.0, 0.18, 0.04, 403.0, 0.12, -0.05, 474.0, -0.2, 0.04, 432.0, -0.28, 0.11, 453.0, 0.22, -0.09, 500.0, 0.0]),
        ("Cathedral 1", &[0.0, -4.4, -4.4, 1.0, 0.05, 1.0, 1.0, 4000.0, 5.0, 0.3, 191.0, 0.2, -0.24, 227.0, -0.18, -0.15, 249.0, 0.28, -0.06, 267.0, 0.0, -0.02, 291.0, -0.06, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0]),
        ("Cathedral 2", &[0.0, -4.4, -2.0, 1.0, 0.1, 1.0, 1.0, 850.0, 4.0, -0.32, 191.0, 0.24, -0.24, 269.0, -0.18, -0.05, 428.0, 0.28, -0.06, 500.0, 0.0, -0.02, 291.0, -0.06, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0]),
        ("Cathedral 3", &[0.0, -2.0, -3.7, 1.0, 0.1, 0.7, 1.0, 4000.0, 5.0, -0.32, 311.0, 0.24, -0.24, 369.0, -0.18, -0.05, 483.0, 0.28, -0.06, 500.0, 0.0, 0.24, 119.0, -0.06, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0]),
        ("Large hall 4", &[0.0, -2.0, -3.7, 1.0, 0.1, 0.8, 1.0, 4000.0, 5.0, -0.32, 119.0, 0.24, -0.19, 311.0, -0.18, -0.09, 369.0, 0.28, -0.09, 461.0, 0.0, -0.06, 500.0, -0.06, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0]),
        ("Small hall 2", &[0.0, 0.0, -4.4, 1.0, 0.15, 0.5, 1.0, 1000.0, 7.0, -0.5, 31.0, 0.3, -0.12, 110.0, -0.31, -0.09, 151.0, 0.18, -0.09, 206.0, 0.12, -0.09, 231.0, -0.2, -0.05, 336.0, -0.28, -0.01, 500.0, 0.22, -0.09, 500.0, 0.0]),
        ("Small room 2", &[0.0, -9.2, -10.5, 0.1, 0.0, 0.15, 1.0, 2500.0, 5.0, 1.0, 27.0, -0.38, -0.21, 101.0, -0.59, 0.08, 193.0, -0.43, -0.05, 240.0, -0.65, 0.03, 302.0, -0.55, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0]),
        ("Tapped delay 1", &[0.0, 0.0, -8.0, 2.0, 0.0, 0.0, 1.0, 4500.0, 8.0, -0.91, 17.0, 0.24, 0.16, 47.0, -0.18, 0.77, 89.0, 0.14, -0.58, 164.0, 0.38, 0.73, 240.0, 0.06, -0.68, 332.0, 0.38, 0.7, 416.0, 0.0, -0.64, 500.0, 0.0]),
        ("Tapped delay 2", &[0.0, 0.0, -8.0, 2.0, 0.0, 0.15, 1.0, 4000.0, 8.0, -0.91, 17.0, 0.24, 0.16, 47.0, -0.18, 0.77, 89.0, 0.14, -0.58, 164.0, 0.38, 0.73, 240.0, 0.06, -0.68, 332.0, 0.38, 0.7, 416.0, 0.0, -0.64, 500.0, 0.0]),
        ("Tapped delay, stereo pan", &[0.0, 0.0, -8.0, 2.0, 0.0, 0.15, 1.0, 4000.0, 8.0, -0.91, 17.0, 1.0, 0.6, 72.0, -1.0, 0.4, 118.0, 1.0, -0.35, 181.0, -1.0, 0.31, 240.0, 1.0, -0.19, 332.0, -1.0, 0.14, 416.0, 1.0, -0.09, 500.0, -1.0]),
        ("Channel delay for stereo simulation", &[0.0, -80.0, 0.0, 1.0, 0.0, 0.0, 0.0, 4500.0, 2.0, 1.0, 1.0, 1.0, 1.0, 15.0, -1.0, 0.13, 212.0, -0.43, 0.05, 306.0, -0.65, -0.06, 361.0, -0.55, 0.05, 468.0, 0.0, -0.02, 482.0, 0.0, 0.05, 500.0, 0.0]),
    ];
}

/// Multi-Band Dynamics: four bands (a low shelf, a band or a high shelf),
/// each compressed on its own and raised or lowered, the rest of the
/// sound left as it is.
pub mod multi_band_dynamics {
    use super::*;

    pub const OUTPUT: usize = 0;
    pub const SYNC: usize = 1;
    /// The bands' first parameter, each band's from its place times
    /// [`PER_BAND`] after it.
    pub const BANDS: usize = 2;
    pub const BYPASS: usize = 0;
    pub const SOLO: usize = 1;
    pub const TYPE: usize = 2;
    pub const CENTER: usize = 3;
    pub const WIDTH: usize = 4;
    pub const THRESHOLD: usize = 5;
    pub const AMOUNT: usize = 6;
    pub const GAIN: usize = 7;
    pub const ATTACK: usize = 8;
    pub const RELEASE: usize = 9;
    pub const PER_BAND: usize = 10;

    const TYPES: &[&str] = &["Low-shelf", "Band-notch", "High-shelf"];

    const fn live(b: usize) -> (u8, u32) {
        ((BANDS + b * PER_BAND + BYPASS) as u8, OFF)
    }

    const ON_: [&[(u8, u32)]; 4] = [&[live(0)], &[live(1)], &[live(2)], &[live(3)]];
    const BAND_: [&[(u8, u32)]; 4] = [
        &[live(0), ((BANDS + TYPE) as u8, either(1, 1))],
        &[live(1), ((BANDS + PER_BAND + TYPE) as u8, either(1, 1))],
        &[live(2), ((BANDS + 2 * PER_BAND + TYPE) as u8, either(1, 1))],
        &[live(3), ((BANDS + 3 * PER_BAND + TYPE) as u8, either(1, 1))],
    ];

    const fn band(b: usize, group: &'static str, hz: f32, bypass: f32) -> [ParamSpec; PER_BAND] {
        let (on, width) = (ON_[b], BAND_[b]);
        [
            ParamSpec::switch("Bypass", bypass).group(group),
            ParamSpec::switch("Solo", 0.0).when(on),
            ParamSpec::choice("Type", 0.0, TYPES, 1.0).when(on),
            ParamSpec::num("Center", 50.0, 15000.0, hz)
                .unit("Hz")
                .decimals(0)
                .log()
                .when(on),
            ParamSpec::num("Width", 0.3, 3.0, 1.0)
                .unit("oct.")
                .decimals(1)
                .when(width),
            ParamSpec::num("Threshold", -60.0, 0.0, 0.0)
                .unit("dB")
                .decimals(1)
                .when(on),
            ParamSpec::num("Amount (x:1)", 1.0, 50.0, 1.0)
                .decimals(1)
                .log()
                .when(on),
            ParamSpec::num("Gain", -25.0, 25.0, 0.0)
                .unit("dB")
                .decimals(1)
                .when(on),
            ParamSpec::num("Attack", 0.0, 500.0, 1.0)
                .unit("ms")
                .decimals(1)
                .when(on),
            ParamSpec::num("Release", 0.0, 5000.0, 100.0)
                .unit("ms")
                .decimals(0)
                .when(on),
        ]
    }

    const B1: [ParamSpec; PER_BAND] = band(0, "Band 1", 200.0, 0.0);
    const B2: [ParamSpec; PER_BAND] = band(1, "Band 2", 1000.0, 1.0);
    const B3: [ParamSpec; PER_BAND] = band(2, "Band 3", 4000.0, 1.0);
    const B4: [ParamSpec; PER_BAND] = band(3, "Band 4", 10000.0, 1.0);

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Output gain", -25.0, 25.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::switch("Stereo sync channels", 1.0),
        B1[0],
        B1[1],
        B1[2],
        B1[3],
        B1[4],
        B1[5],
        B1[6],
        B1[7],
        B1[8],
        B1[9],
        B2[0],
        B2[1],
        B2[2],
        B2[3],
        B2[4],
        B2[5],
        B2[6],
        B2[7],
        B2[8],
        B2[9],
        B3[0],
        B3[1],
        B3[2],
        B3[3],
        B3[4],
        B3[5],
        B3[6],
        B3[7],
        B3[8],
        B3[9],
        B4[0],
        B4[1],
        B4[2],
        B4[3],
        B4[4],
        B4[5],
        B4[6],
        B4[7],
        B4[8],
        B4[9],
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, 1.0, 0.0, 0.0, 1.0, 200.0, 1.0, 0.0, 1.0, 0.0, 1.0, 100.0, 1.0, 0.0, 1.0, 1000.0, 1.0, 0.0, 1.0, 0.0, 1.0, 100.0, 1.0, 0.0, 1.0, 4000.0, 1.0, 0.0, 1.0, 0.0, 1.0, 100.0, 1.0, 0.0, 1.0, 10000.0, 1.0, 0.0, 1.0, 0.0, 1.0, 100.0]),
        ("Anti-DTMF talk-off", &[0.0, 1.0, 0.0, 0.0, 1.0, 1209.0, 0.7, -25.0, 30.0, 0.0, 1.0, 100.0, 0.0, 0.0, 1.0, 1336.0, 0.7, -25.0, 30.0, 0.0, 1.0, 100.0, 0.0, 0.0, 1.0, 1477.0, 0.7, -25.0, 30.0, 0.0, 1.0, 100.0, 0.0, 0.0, 1.0, 1633.0, 0.7, -25.0, 30.0, 0.0, 1.0, 100.0]),
        ("Reduce loud plosives", &[0.0, 1.0, 0.0, 0.0, 0.0, 500.0, 1.0, -15.0, 32.0, 0.0, 1.0, 100.0, 1.0, 0.0, 0.0, 1000.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1000.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1000.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0]),
        ("Reduce loud sibilants (de-esser)", &[0.0, 1.0, 0.0, 0.0, 1.0, 5000.0, 1.0, -30.0, 25.0, 2.0, 1.0, 100.0, 1.0, 0.0, 0.0, 1000.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1000.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1000.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0]),
        ("Reduce plosives and sibilants", &[0.0, 1.0, 0.0, 0.0, 0.0, 600.0, 2.0, -15.0, 40.0, 0.0, 1.0, 100.0, 0.0, 0.0, 1.0, 5000.0, 1.0, -32.0, 20.0, 2.0, 1.0, 100.0, 1.0, 0.0, 0.0, 1000.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1000.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0]),
    ];
}

/// Express Delay: an echo fed back by a share of itself, mixed with the
/// sound.
pub mod express_delay {
    use super::*;

    pub const DELAY: usize = 0;
    pub const DECAY: usize = 1;
    pub const MIX: usize = 2;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Delay", 0.0, 1000.0, 250.0)
            .unit("ms")
            .decimals(1),
        ParamSpec::num("Decay", 0.0, 0.9, 0.0)
            .show(Show::Percent)
            .decimals(1),
        ParamSpec::num("Delay / Original", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(1),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[250.0, 0.0, 0.5]),
        ("1/2 second delay", &[500.0, 0.0, 0.5]),
        ("Phased", &[3.0, 0.0, 0.5]),
        ("Robot", &[10.0, 0.9, 1.0]),
        ("Echo chamber", &[250.0, 0.5, 0.35]),
        ("Grand canyon", &[1000.0, 0.25, 0.25]),
        ("Dreamscape", &[100.0, 0.7, 0.4]),
    ];
}

/// Express Distortion: saturation past a knee set by the slew, a short
/// comb and a limit on how fast the sound may move, mixed with it.
pub mod express_distortion {
    use super::*;

    pub const DISTORTION: usize = 0;
    pub const SLEW: usize = 1;
    pub const HARD: usize = 2;
    pub const MIX: usize = 3;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Distortion", 0.0, 1.0, 0.0)
            .show(Show::Percent)
            .decimals(1),
        ParamSpec::num("Slew", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(1),
        ParamSpec::switch("Enable hard clipping", 0.0),
        ParamSpec::num("Distortion / Original", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(1),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, 0.5, 0.0, 0.5]),
        ("Mangle", &[0.0, 1.0, 1.0, 1.0]),
        ("Fuzz", &[1.0, 0.75, 0.0, 0.5]),
        ("Gristle", &[0.75, 0.25, 1.0, 0.5]),
        ("Spaced", &[0.05, 0.0, 0.0, 1.0]),
    ];
}

/// Express Equalization: bass, mid and treble (−25 dB cuts them out),
/// mixed with the sound.
pub mod express_equalization {
    use super::*;

    pub const BASS: usize = 0;
    pub const MID: usize = 1;
    pub const TREBLE: usize = 2;
    pub const MIX: usize = 3;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Bass", -25.0, 12.0, 0.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Mid", -25.0, 12.0, 0.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Treble", -25.0, 12.0, 0.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("EQ / Original", 0.0, 1.0, 1.0)
            .show(Show::Percent)
            .decimals(1),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, 0.0, 0.0, 1.0]),
        ("Bass only", &[0.0, -25.0, -25.0, 1.0]),
        ("Bass boost", &[12.0, 0.0, 0.0, 1.0]),
        ("Contour (drop middle)", &[6.0, -25.0, 6.0, 1.0]),
        ("Treble only", &[-25.0, -25.0, 6.0, 1.0]),
        ("Treble boost", &[0.0, 0.0, 12.0, 1.0]),
    ];
}

/// Express Noise Gate: the Noise Gate with its three controls.
pub mod express_noise_gate {
    use super::*;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Threshold", -80.0, 0.0, -20.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Attack", 1.0, 500.0, 3.0)
            .unit("ms")
            .decimals(0)
            .log(),
        ParamSpec::num("Release", 1.0, 5000.0, 100.0)
            .unit("ms")
            .decimals(0)
            .log(),
    ];

    pub const PRESETS: &[Preset] = noise_gate::PRESETS;
}

/// Express Chorus: one to three modulated 20 ms delays in a row, mixed
/// with the sound.
pub mod express_chorus {
    use super::*;

    pub const SIZE: usize = 0;
    pub const RATE: usize = 1;
    pub const DEPTH: usize = 2;
    pub const MIX: usize = 3;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::choice("Size", 1.0, &["1", "2", "3"], 1.0),
        ParamSpec::num("Rate", 0.001, 20.0, 0.1)
            .unit("Hz")
            .decimals(3)
            .log(),
        ParamSpec::num("Depth", 0.01, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Chorus / Original", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(1),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[1.0, 0.1, 0.5, 0.5]),
        ("Wacky 1", &[1.0, 4.0, 0.5, 1.0]),
        ("Wacky 2", &[1.0, 1.0, 0.5, 1.0]),
        ("Wacky 3", &[3.0, 5.0, 0.1, 0.75]),
        ("Wacky 4", &[2.0, 10.0, 0.5, 1.0]),
        ("Slow light chorus", &[1.0, 0.15, 0.25, 0.5]),
        ("Slow medium chorus", &[2.0, 0.3, 0.25, 0.5]),
        ("Slow heavy chorus", &[3.0, 0.3, 0.4, 0.75]),
        ("Fast light chorus", &[1.0, 5.0, 0.03, 0.4]),
        ("Fast medium chorus", &[2.0, 5.0, 0.05, 0.5]),
        ("Fast heavy chorus", &[3.0, 4.0, 0.07, 0.75]),
        ("Guitar", &[3.0, 4.0, 0.01, 1.0]),
    ];
}

/// Express Flange/Wah-Wah: the full one's flange or wah-wah (centre
/// 500 Hz, resonance 75 %), mixed with the sound.
pub mod express_flange_wah_wah {
    use super::*;

    pub const EFFECT: usize = 0;
    pub const RATE: usize = 1;
    pub const DEPTH: usize = 2;
    pub const MIX: usize = 3;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::choice("Effect", 0.0, &["Flange", "Wah-Wah"], 0.0),
        ParamSpec::num("Rate", 0.001, 20.0, 1.0)
            .unit("Hz")
            .decimals(3)
            .log(),
        ParamSpec::num("Depth", 0.01, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Effect / Original", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(1),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, 1.0, 0.5, 0.5]),
        ("Bouncing flange", &[0.0, 1.0, 0.5, 0.5]),
        ("Slow flange", &[0.0, 0.2, 0.8, 0.5]),
        ("Mad flange", &[0.0, 20.0, 1.0, 1.0]),
        ("Tremolo flange", &[0.0, 15.0, 0.25, 0.5]),
        ("Bouncing wah", &[1.0, 1.0, 0.5, 0.5]),
        ("Slow wah", &[1.0, 0.2, 0.8, 0.5]),
        ("Mad wah", &[1.0, 20.0, 1.0, 1.0]),
        ("Tremolo wah", &[1.0, 15.0, 0.25, 0.5]),
    ];
}

/// Express Dynamics: a compressor (to a limiter at its top ratio).
pub mod express_dynamics {
    use super::*;

    pub const THRESHOLD: usize = 0;
    pub const RATIO: usize = 1;
    pub const ATTACK: usize = 2;
    pub const RELEASE: usize = 3;
    pub const AUTO_GAIN: usize = 4;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Threshold", -80.0, -0.1, -20.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Ratio (x:1)", 1.0, 50.0, 1.0)
            .decimals(1)
            .log(),
        ParamSpec::num("Attack", 0.0, 500.0, 1.0)
            .unit("ms")
            .decimals(1),
        ParamSpec::num("Release", 0.0, 5000.0, 500.0)
            .unit("ms")
            .decimals(1),
        ParamSpec::switch("Auto gain compensate", 0.0),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[-20.0, 1.0, 1.0, 500.0, 0.0]),
        ("Limit levels to -6 dB", &[-6.0, 50.0, 1.0, 500.0, 0.0]),
        (
            "Maximize volume (-12 dB limiter with gain compensation)",
            &[-12.0, 50.0, 5.0, 100.0, 1.0],
        ),
        (
            "2:1 compression starting at -18 dB",
            &[-18.0, 2.0, 1.0, 500.0, 1.0],
        ),
        (
            "3:1 compression starting at -15 dB",
            &[-15.0, 3.0, 1.0, 500.0, 1.0],
        ),
        (
            "2:1 compression starting at -18 dB (ACX)",
            &[-18.0, 2.0, 1.0, 500.0, 1.0],
        ),
    ];
}

/// Express Stutter: the sound switched on and off in a rhythm (soft
/// edges), the second channel's rhythm moved on with Add space, mixed
/// with the sound.
pub mod express_stutter {
    use super::*;

    pub const SILENCE: usize = 0;
    pub const SOUND: usize = 1;
    pub const SPACE: usize = 2;
    pub const MIX: usize = 3;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Silence", 0.01, 200.0, 1.0)
            .unit("ms")
            .decimals(2)
            .log(),
        ParamSpec::num("Sound", 0.01, 200.0, 1.0)
            .unit("ms")
            .decimals(2)
            .log(),
        ParamSpec::switch("Add space", 0.0),
        ParamSpec::num("Stutter / Original", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(1),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[1.0, 1.0, 0.0, 0.5]),
        ("Crystal ball", &[20.0, 35.0, 1.0, 1.0]),
        ("Gravel", &[30.0, 20.0, 0.0, 0.75]),
        ("Low bell", &[1.0, 6.0, 1.0, 0.65]),
        ("Walkie-talkie", &[1.0, 1.0, 1.0, 0.5]),
    ];
}

/// Express Amplitude Modulation: the sound's level following a smooth
/// wave at the rate, as deep as the depth, mixed with the sound.
pub mod express_amplitude_modulation {
    use super::*;

    pub const RATE: usize = 0;
    pub const DEPTH: usize = 1;
    pub const MIX: usize = 2;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Rate", 0.1, 5000.0, 1.0)
            .unit("Hz")
            .decimals(1)
            .log(),
        ParamSpec::num("Depth", 0.0, 1.0, 1.0)
            .show(Show::Percent)
            .decimals(1),
        ParamSpec::num("Modulation / Original", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(1),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[1.0, 1.0, 0.5]),
        ("Slow modulation", &[2.0, 0.5, 0.75]),
        ("Fast modulation", &[15.0, 0.75, 1.0]),
        ("440 Hz noise", &[440.0, 1.0, 1.0]),
    ];
}

/// Express Graphic EQ: ten bands an octave apart, drawn as one response
/// and applied as a linear-phase FIR filter through FFTs.
pub mod express_graphic_eq {
    use super::*;

    /// The bands' gains, from the lowest.
    pub const BANDS: usize = 0;
    pub const OUTPUT: usize = 10;
    pub const ACCURACY: usize = 11;

    /// Each band's centre (Hz).
    pub const CENTERS: [f32; 10] = [
        28.0, 56.0, 113.0, 225.0, 450.0, 900.0, 1800.0, 3600.0, 7200.0, 15000.0,
    ];

    const fn band(name: &'static str) -> ParamSpec {
        ParamSpec::num(name, -60.0, 24.0, 0.0)
            .show(Show::Db)
            .decimals(1)
    }

    pub const PARAMS: &[ParamSpec] = &[
        band("28 Hz"),
        band("56 Hz"),
        band("113 Hz"),
        band("225 Hz"),
        band("450 Hz"),
        band("900 Hz"),
        band("1.8 kHz"),
        band("3.6 kHz"),
        band("7.2 kHz"),
        band("15 kHz"),
        ParamSpec::num("Output gain", -60.0, 20.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::choice(
            "Accuracy",
            0.0,
            &[
                "Low (fast, good for high bands)",
                "Medium (great for mid and high bands)",
                "High (slow, great for all bands)",
            ],
            1.0,
        ),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]),
        ("Boost bass around 125 Hz by 6 dB", &[0.0, 1.5, 6.0, 1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0]),
        ("Boost high frequencies above 4 kHz by 6 dB", &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 3.0, 6.0, 6.0, 6.0, 0.0, 2.0]),
        ("Boost mid-range around 1 kHz by 6 dB", &[0.0, 0.0, 0.0, 0.0, 4.0, 6.0, 4.0, 0.0, 0.0, 0.0, 0.0, 1.0]),
        ("Cut high frequencies above 4 kHz by 12 dB", &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -6.0, -12.0, -12.0, 0.0, 1.0]),
        ("Cut low frequencies below 125 Hz by 60 dB", &[-60.0, -60.0, -10.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0]),
        ("Cut mid-range around 1 kHz by 6 dB", &[0.0, 0.0, 0.0, 0.0, -4.0, -6.0, -4.0, 0.0, 0.0, 0.0, 0.0, 1.0]),
    ];
}

/// Express Reverb: the Reverb's rooms, its size setting the decay and its
/// liveliness the pre-delay, mixed with the sound.
pub mod express_reverb {
    use super::*;

    pub const ROOM: usize = 0;
    pub const SIZE: usize = 1;
    pub const LIVELINESS: usize = 2;
    pub const MIX: usize = 3;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec {
            name: "Room type",
            ..reverb::PARAMS[reverb::MODE]
        },
        ParamSpec::num("Room size", 0.0, 1.0, 0.1667)
            .show(Show::Percent)
            .decimals(1),
        ParamSpec::num("Liveliness", 0.0, 1.0, 0.15)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Reverb / Original", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(1),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, 0.1667, 0.15, 0.5]),
        ("Dream-verb", &[8.0, 1.0, 0.0, 1.0]),
        ("Indoor olympic-size pool", &[4.0, 1.0, 0.8, 0.75]),
        ("Hospital hallway", &[5.0, 0.6111, 0.35, 0.4]),
        ("Small metal room", &[9.0, 0.1667, 0.15, 1.0]),
        ("Small night club", &[7.0, 0.0556, 0.3, 0.5]),
    ];
}

/// Amplitude Modulation: the level following a wave drawn on a graph
/// (from 0 dB at its top to the amplitude at its bottom), at a frequency
/// or a note of the tempo, the right channel up to half a round behind.
pub mod amplitude_modulation {
    use super::*;

    pub const DRY: usize = 0;
    pub const WET: usize = 1;
    pub const AMPLITUDE: usize = 2;
    /// The wave's point count, its points after it.
    pub const GRAPH: usize = 3;
    pub const BLEND: usize = GRAPH + 1 + WAVE.params();
    pub const FREQUENCY: usize = BLEND + 1;
    pub const PAN: usize = BLEND + 2;
    pub const LOW_PASS: usize = BLEND + 3;
    pub const LOW_FREQ: usize = BLEND + 4;
    pub const SYNC: usize = BLEND + 5;
    pub const MULTIPLIER: usize = BLEND + 6;
    pub const PERIOD: usize = BLEND + 7;

    /// A round of the wave (0 to 1000), its level from 0 (top) to −100.
    pub const WAVE: Graph = Graph {
        points: 32,
        x: (0.0, 1000.0),
        y: (-100.0, 0.0),
        log_x: false,
        pinned_ends: true,
        x_unit: "",
        y_unit: "",
    };

    const FREE: &[(u8, u32)] = &[(SYNC as u8, OFF)];
    const SYNCED: &[(u8, u32)] = &[(SYNC as u8, ON)];
    const CUT: &[(u8, u32)] = &[(LOW_PASS as u8, ON)];

    const HEAD: [ParamSpec; 4] = [
        ParamSpec::num("Dry out", -80.0, 0.0, -80.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Wet out", -80.0, 24.0, 0.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Amplitude", -80.0, 0.0, -48.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::graph("Graph", WAVE, 2.0),
    ];
    const POINTS: [ParamSpec; 64] = WAVE.point_params(&[(0.0, -50.0), (1000.0, -50.0)]);
    const TAIL: [ParamSpec; 8] = [
        ParamSpec::switch("Blend graph edges", 0.0),
        ParamSpec::num("Mod. freq.", 0.001, 5000.0, 1.0)
            .unit("Hz")
            .decimals(3)
            .log()
            .when(FREE),
        ParamSpec::num("Stereo pan", 0.0, 1.0, 0.0)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::switch("Low-pass", 0.0),
        ParamSpec::num("Low-pass start frequency", 100.0, 10000.0, 400.0)
            .unit("Hz")
            .decimals(0)
            .log()
            .when(CUT),
        ParamSpec::switch("Tempo sync", 0.0),
        ParamSpec::num("Period multiplier", 0.001, 32.0, 1.0)
            .decimals(3)
            .log()
            .when(SYNCED),
        ParamSpec::choice(
            "Modulation period",
            0.0,
            tempo::NOTES_OR_MEASURE,
            tempo::MEASURE,
        )
        .when(SYNCED),
    ];
    const FRONT: [ParamSpec; 68] = join(HEAD, POINTS);
    const ALL: [ParamSpec; 76] = join(FRONT, TAIL);

    pub const PARAMS: &[ParamSpec] = &ALL;

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[-80.0, 0.0, -48.0, 2.0, 0.0, -50.0, 1000.0, -50.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, 1.0, 0.0, 0.0, 400.0, 0.0, 1.0, 18.0]),
        ("Triangle 1", &[-80.0, 0.0, -48.0, 4.0, 0.0, -50.0, 250.0, 0.0, 750.0, -100.0, 1000.0, -50.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, 1.0, 0.0, 0.0, 400.0, 0.0, 1.0, 18.0]),
        ("Sine", &[-80.0, 0.0, -48.0, 12.0, 0.0, -50.0, 125.0, -12.0, 188.0, -3.0, 250.0, 0.0, 313.0, -3.0, 375.0, -12.0, 625.0, -88.0, 688.0, -97.0, 750.0, -100.0, 813.0, -97.0, 875.0, -88.0, 1000.0, -50.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, 1.0, 0.0, 0.0, 400.0, 0.0, 1.0, 18.0]),
        ("Sine out of phase", &[-80.0, 0.0, -48.0, 12.0, 0.0, -50.0, 125.0, -12.0, 188.0, -3.0, 250.0, 0.0, 313.0, -3.0, 375.0, -12.0, 625.0, -88.0, 688.0, -97.0, 750.0, -100.0, 813.0, -97.0, 875.0, -88.0, 1000.0, -50.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 1.0, 5.0, 1.0, 0.0, 400.0, 0.0, 1.0, 18.0]),
        ("Soft square", &[-80.0, 0.0, -48.0, 6.0, 0.0, -50.0, 50.0, 0.0, 450.0, 0.0, 550.0, -100.0, 950.0, -100.0, 1000.0, -50.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, 5.0, 0.0, 0.0, 400.0, 0.0, 1.0, 18.0]),
        ("Hard square", &[-80.0, 0.0, -48.0, 4.0, 0.0, 0.0, 500.0, 0.0, 501.0, -100.0, 1000.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, 5.0, 0.0, 0.0, 400.0, 0.0, 1.0, 18.0]),
        ("Wacky", &[-80.0, 0.0, -48.0, 14.0, 0.0, -50.0, 90.0, -25.0, 125.0, -20.0, 160.0, -25.0, 340.0, -70.0, 375.0, -75.0, 410.0, -70.0, 590.0, -10.0, 625.0, -5.0, 660.0, -10.0, 840.0, -90.0, 875.0, -95.0, 910.0, -90.0, 1000.0, -50.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, 6.5, 0.25, 0.0, 400.0, 0.0, 1.0, 18.0]),
        ("Sawtooth 1", &[-80.0, 0.0, -48.0, 2.0, 0.0, -50.0, 1000.0, 0.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, 1.0, 0.0, 0.0, 400.0, 0.0, 0.001, 0.0]),
        ("Sawtooth 2", &[-80.0, 0.0, -48.0, 2.0, 0.0, -100.0, 1000.0, 0.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, 1.0, 0.0, 0.0, 400.0, 0.0, 0.001, 0.0]),
    ];
}

/// Graphic Dynamics: the level, a block of 20 ms at a time, through an
/// input/output curve drawn on a graph (compression, limiting, gating),
/// the gain moving by the attack and release.
pub mod graphic_dynamics {
    use super::*;

    /// The curve's point count, its points after it.
    pub const GRAPH: usize = 0;
    pub const ATTACK: usize = GRAPH + 1 + CURVE.params();
    pub const RELEASE: usize = ATTACK + 1;
    pub const OUTPUT: usize = ATTACK + 2;
    pub const AUTO_GAIN: usize = ATTACK + 3;
    pub const SYNC: usize = ATTACK + 4;

    /// Input level (dB) to output level (dB), −84 dB the bottom (−Inf.).
    pub const CURVE: Graph = Graph {
        points: 16,
        x: (-84.0, 0.0),
        y: (-84.0, 0.0),
        log_x: false,
        pinned_ends: true,
        x_unit: "dB",
        y_unit: "dB",
    };

    const HEAD: [ParamSpec; 1] = [ParamSpec::graph("Graph", CURVE, 3.0)];
    const POINTS: [ParamSpec; 32] =
        CURVE.point_params(&[(-84.0, -84.0), (-24.0, -24.0), (0.0, 0.0)]);
    const TAIL: [ParamSpec; 5] = [
        ParamSpec::num("Attack", 0.0, 500.0, 1.0)
            .unit("ms")
            .decimals(1),
        ParamSpec::num("Release", 0.0, 5000.0, 500.0)
            .unit("ms")
            .decimals(1),
        ParamSpec::num("Output gain", -25.0, 25.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::switch("Auto gain compensate", 0.0),
        ParamSpec::switch("Sync channel gains", 1.0),
    ];
    const FRONT: [ParamSpec; 33] = join(HEAD, POINTS);
    const ALL: [ParamSpec; 38] = join(FRONT, TAIL);

    pub const PARAMS: &[ParamSpec] = &ALL;

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[3.0, -84.0, -84.0, -24.0, -24.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 1.0, 500.0, 0.0, 0.0, 1.0]),
        ("Compressor/Gate", &[4.0, -84.0, -84.0, -61.4, -84.0, -15.0, -15.0, 0.0, -11.9, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 1.0, 500.0, 0.0, 1.0, 1.0]),
        ("Mute levels below -35 dB (hard noise gate)", &[4.0, -84.0, -84.0, -35.1, -84.0, -35.0, -35.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 1.0, 500.0, 0.0, 0.0, 1.0]),
        ("Limit levels to -6 dB (hard limiter)", &[3.0, -84.0, -84.0, -6.0, -6.0, 0.0, -6.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 1.0, 500.0, 0.0, 0.0, 1.0]),
        ("Soft limiter", &[4.0, -84.0, -84.0, -12.2, -12.4, -5.7, -7.8, 0.0, -6.8, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 1.0, 500.0, 0.0, 0.0, 1.0]),
        ("Maximize volume (-12 dB limiter with gain compensation)", &[3.0, -84.0, -84.0, -12.0, -12.0, 0.0, -12.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 5.0, 100.0, 0.0, 1.0, 0.0]),
        ("2:1 compression starting at -18 dB", &[3.0, -84.0, -84.0, -18.0, -18.0, 0.0, -9.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 1.0, 500.0, 0.0, 1.0, 1.0]),
        ("3:1 compression starting at -15 dB", &[3.0, -84.0, -84.0, -15.0, -15.0, 0.0, -10.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 1.0, 500.0, 0.0, 1.0, 1.0]),
        ("Soft knee compressor/gate (-24 dB threshold)", &[7.0, -84.0, -84.0, -45.6, -84.0, -41.9, -56.2, -33.4, -36.1, -24.0, -23.8, -11.0, -15.0, 0.0, -11.9, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 1.0, 500.0, 0.0, 1.0, 1.0]),
        ("Soft noise gate below -36 dB", &[6.0, -84.0, -84.0, -55.7, -84.0, -51.5, -67.2, -44.3, -49.0, -33.8, -34.1, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 1.0, 1000.0, 0.0, 0.0, 1.0]),
        ("2:1 compression starting at -18 dB (ACX)", &[3.0, -84.0, -84.0, -18.0, -18.0, 0.0, -9.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 1.0, 500.0, 0.0, 1.0, 1.0]),
    ];
}

/// Distortion: each sample's level (in dB) through an input/output curve
/// drawn on a graph — one for the positive half of the wave, one for the
/// negative — then optionally slowed in its changes of slope and darkened,
/// mixed dry and wet.
pub mod distortion {
    use super::*;

    pub const DRY: usize = 0;
    pub const WET: usize = 1;
    /// The positive half's curve: its point count, its points after it.
    pub const POSITIVE: usize = 2;
    /// The negative half's.
    pub const NEGATIVE: usize = POSITIVE + 1 + CURVE.params();
    pub const SLEW: usize = NEGATIVE + 1 + CURVE.params();
    pub const SLEW_RATE: usize = SLEW + 1;
    pub const LOW_PASS: usize = SLEW + 2;
    pub const LOW_FREQ: usize = SLEW + 3;

    /// Input level (dB) to output level (dB), −84 dB the bottom (−Inf.).
    pub const CURVE: Graph = Graph {
        points: 32,
        x: (-84.0, 0.0),
        y: (-84.0, 0.0),
        log_x: false,
        pinned_ends: true,
        x_unit: "dB",
        y_unit: "dB",
    };

    const SLEWED: &[(u8, u32)] = &[(SLEW as u8, ON)];
    const CUT: &[(u8, u32)] = &[(LOW_PASS as u8, ON)];

    const HEAD: [ParamSpec; 2] = [
        ParamSpec::num("Dry out", -60.0, 0.0, -60.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("Wet out", -60.0, 24.0, 0.0)
            .show(Show::Db)
            .decimals(1),
    ];
    const LINE: [(f32, f32); 2] = [(-84.0, -84.0), (0.0, 0.0)];
    const POS_HEAD: [ParamSpec; 1] = [ParamSpec::graph("Positive half", CURVE, 2.0)];
    const NEG_HEAD: [ParamSpec; 1] = [ParamSpec::graph("Negative half", CURVE, 2.0)];
    const POINTS: [ParamSpec; 64] = CURVE.point_params(&LINE);
    const TAIL: [ParamSpec; 4] = [
        ParamSpec::switch("Slew rate", 0.0),
        ParamSpec::num("Slew amount", 1.0, 100.0, 100.0)
            .decimals(0)
            .when(SLEWED),
        ParamSpec::switch("Low-pass", 0.0),
        ParamSpec::num("Low-pass start frequency", 100.0, 10000.0, 400.0)
            .unit("Hz")
            .decimals(0)
            .log()
            .when(CUT),
    ];
    const A: [ParamSpec; 3] = join(HEAD, POS_HEAD);
    const B: [ParamSpec; 67] = join(A, POINTS);
    const C: [ParamSpec; 68] = join(B, NEG_HEAD);
    #[allow(clippy::large_const_arrays)]
    const D: [ParamSpec; 132] = join(C, POINTS);
    #[allow(clippy::large_const_arrays)]
    const ALL: [ParamSpec; 136] = join(D, TAIL);

    pub const PARAMS: &[ParamSpec] = &ALL;

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[-60.0, 0.0, 2.0, -84.0, -84.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 2.0, -84.0, -84.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 0.0, 100.0, 0.0, 400.0]),
        ("2:1 Compression starting at -18 dB", &[-60.0, 0.0, 3.0, -84.0, -84.0, -18.0, -18.0, 0.0, -9.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 3.0, -84.0, -84.0, -18.0, -18.0, 0.0, -9.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 0.0, 100.0, 0.0, 400.0]),
        ("Soft noise gate", &[-60.0, 0.0, 6.0, -84.0, -84.0, -50.0, -84.0, -45.4, -60.0, -40.6, -46.0, -36.0, -36.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 6.0, -84.0, -84.0, -50.0, -84.0, -45.4, -60.0, -40.6, -46.0, -36.0, -36.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 0.0, 100.0, 0.0, 400.0]),
        ("Fuzz 1", &[-60.0, 0.0, 5.0, -84.0, -84.0, -44.0, -34.0, -24.0, -34.0, -24.0, -14.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 5.0, -84.0, -84.0, -44.0, -34.0, -24.0, -34.0, -24.0, -14.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 0.0, 100.0, 0.0, 400.0]),
        ("Fuzz 2", &[-60.0, 0.0, 5.0, -84.0, -84.0, -44.0, -34.0, -24.0, -34.0, -24.0, -14.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 5.0, -84.0, -84.0, -34.0, -44.0, -24.0, -34.0, -14.0, -24.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 0.0, 100.0, 0.0, 400.0]),
        ("Wet slew", &[-60.0, 0.0, 2.0, -84.0, -84.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 2.0, -84.0, -84.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 1.0, 5.0, 0.0, 400.0]),
        ("Slight chip fuzz", &[-60.0, 0.0, 2.0, -84.0, -84.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 2.0, -84.0, -84.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 1.0, 25.0, 0.0, 400.0]),
        ("Transistor 1", &[-60.0, 0.0, 4.0, -84.0, -84.0, -42.0, -66.0, -18.0, -18.0, 0.0, -8.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 4.0, -84.0, -84.0, -42.0, -66.0, -18.0, -18.0, 0.0, -8.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 0.0, 100.0, 0.0, 400.0]),
        ("Transistor 2", &[-60.0, 0.0, 4.0, -84.0, -84.0, -36.0, -72.0, -18.0, -18.0, 0.0, -12.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 4.0, -84.0, -84.0, -36.0, -72.0, -18.0, -18.0, 0.0, -12.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 0.0, 100.0, 0.0, 400.0]),
        ("Lil' Warmer", &[-60.0, 0.0, 3.0, -84.0, -84.0, -30.0, -24.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 3.0, -84.0, -84.0, -18.0, -24.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 0.0, 100.0, 0.0, 400.0]),
        ("Tube 1", &[-5.0, -1.0, 5.0, -84.0, -84.0, -71.0, -60.0, -54.0, -38.0, -30.0, -24.0, 0.0, -12.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 5.0, -84.0, -84.0, -71.0, -60.0, -54.0, -38.0, -30.0, -24.0, 0.0, -12.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 0.0, 100.0, 0.0, 400.0]),
        ("Boost positive signal", &[-60.0, -5.0, 4.0, -84.0, -84.0, -29.0, -29.0, -29.0, -10.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 4.0, -84.0, -84.0, -29.0, -29.0, -29.0, -10.0, 0.0, 0.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, -84.0, 0.0, 100.0, 0.0, 400.0]),
    ];
}

/// Graphic EQ: a frequency response drawn on a graph (straight lines in
/// dB along the octaves), applied as a linear-phase FIR filter through
/// FFTs.
pub mod graphic_eq {
    use super::*;

    /// The response's point count, its points after it.
    pub const GRAPH: usize = 0;
    pub const OUTPUT: usize = GRAPH + 1 + RESPONSE.params();
    pub const ACCURACY: usize = OUTPUT + 1;

    /// Frequency (Hz, on octaves) to gain (dB), −60 dB the bottom (−Inf.).
    pub const RESPONSE: Graph = Graph {
        points: 32,
        x: (20.0, 20000.0),
        y: (-60.0, 24.0),
        log_x: true,
        pinned_ends: true,
        x_unit: "Hz",
        y_unit: "dB",
    };

    const HEAD: [ParamSpec; 1] = [ParamSpec::graph("Graph", RESPONSE, 2.0)];
    const POINTS: [ParamSpec; 64] = RESPONSE.point_params(&[(20.0, 0.0), (20000.0, 0.0)]);
    const TAIL: [ParamSpec; 2] = [
        ParamSpec::num("Output gain", -60.0, 20.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::choice(
            "Accuracy",
            0.0,
            &[
                "Low (fast, good for high bands)",
                "Medium (great for mid and high bands)",
                "High (slow, great for all bands)",
            ],
            1.0,
        ),
    ];
    const FRONT: [ParamSpec; 65] = join(HEAD, POINTS);
    const ALL: [ParamSpec; 67] = join(FRONT, TAIL);

    pub const PARAMS: &[ParamSpec] = &ALL;

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[2.0, 20.0, 0.0, 20000.0, 0.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 0.0, 1.0]),
        ("Roll-off frequencies below 120 Hz", &[3.0, 20.0, -60.0, 120.0, 0.0, 20000.0, 0.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 0.0, 1.0]),
        ("Boost bass around 80 Hz by 6 dB", &[6.0, 20.0, 0.0, 36.4, 0.0, 43.5, 6.015, 126.8, 6.015, 151.5, 0.0, 20000.0, 0.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 0.0, 1.0]),
        ("Boost high frequencies above 7 kHz by 6 dB", &[4.0, 20.0, 0.0, 6402.4, 0.0, 6999.3, 6.015, 20000.0, 6.015, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 0.0, 1.0]),
        ("Boost mid-range around 1 kHz by 6 dB", &[6.0, 20.0, 0.0, 693.0, 0.0, 828.2, 6.015, 1183.0, 6.015, 1413.9, 0.0, 20000.0, 0.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 0.0, 1.0]),
        ("Boost upper three octaves", &[3.0, 20.0, 0.0, 2650.1, 0.0, 20000.0, 7.6078, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 0.0, 1.0]),
        ("Gentle high-frequency boost", &[3.0, 20.0, 0.0, 630.7, 0.0, 20000.0, 4.6507, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 0.0, 1.0]),
        ("Notch frequencies around 450 Hz", &[6.0, 20.0, 0.0, 309.1, 0.0, 369.5, -60.0, 527.7, -60.0, 630.7, 0.0, 20000.0, 0.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 20.0, -60.0, 0.0, 1.0]),
    ];
}

/// Wave Hammer Surround: a compressor and a volume maximizer (a
/// look-ahead limiter brought up to its output level), each with its own
/// bypass, the channels that steer it and those it works on.
pub mod wave_hammer {
    use super::*;

    pub const BYPASS: usize = 0;
    pub const THRESHOLD: usize = 1;
    pub const RATIO: usize = 2;
    pub const OUTPUT: usize = 3;
    pub const RMS: usize = 4;
    pub const AUTO_GAIN: usize = 5;
    pub const LOOK_AHEAD: usize = 6;
    pub const SMOOTH: usize = 7;
    pub const ATTACK: usize = 8;
    pub const RELEASE: usize = 9;
    pub const SMART: usize = 10;
    pub const MAX_BYPASS: usize = 11;
    pub const MAX_THRESHOLD: usize = 12;
    pub const MAX_OUTPUT: usize = 13;
    pub const MAX_LOOK_AHEAD: usize = 14;
    pub const MAX_RELEASE: usize = 15;
    /// The six channels that steer it (left, right, centre, left and
    /// right surround, LFE), then the six it works on.
    pub const CONTROL: usize = 16;
    pub const AFFECT: usize = 22;

    const COMPRESSING: &[(u8, u32)] = &[(BYPASS as u8, OFF)];
    const MAXIMIZING: &[(u8, u32)] = &[(MAX_BYPASS as u8, OFF)];

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::switch("Bypass compressor", 0.0).group("Compressor"),
        ParamSpec::num("Threshold", -60.0, 0.0, -12.0)
            .unit("dB")
            .decimals(1)
            .when(COMPRESSING),
        ParamSpec::num("Ratio (x:1)", 1.0, 50.0, 4.0)
            .decimals(1)
            .log()
            .when(COMPRESSING),
        ParamSpec::num("Output gain", -30.0, 30.0, 0.0)
            .unit("dB")
            .decimals(1)
            .when(COMPRESSING),
        ParamSpec::choice("Scan mode", 0.0, &["Peak", "RMS"], 1.0).when(COMPRESSING),
        ParamSpec::switch("Auto gain compensate", 0.0).when(COMPRESSING),
        ParamSpec::switch("Use longer look-ahead", 0.0).when(COMPRESSING),
        ParamSpec::switch("Smooth saturation", 1.0).when(COMPRESSING),
        ParamSpec::num("Attack time", 0.0, 500.0, 2.0)
            .unit("ms")
            .decimals(1)
            .when(COMPRESSING),
        ParamSpec::num("Release time", 0.0, 1000.0, 100.0)
            .unit("ms")
            .decimals(1)
            .when(COMPRESSING),
        ParamSpec::num("Smart release", 0.0, 100.0, 0.0)
            .decimals(0)
            .when(COMPRESSING),
        ParamSpec::switch("Bypass volume maximizer", 0.0).group("Volume maximizer"),
        ParamSpec::num("Threshold", -60.0, 0.0, 0.0)
            .unit("dB")
            .decimals(1)
            .when(MAXIMIZING),
        ParamSpec::num("Output level", -30.0, 0.0, 0.0)
            .unit("dB")
            .decimals(1)
            .when(MAXIMIZING),
        ParamSpec::switch("Use longer look-ahead", 0.0).when(MAXIMIZING),
        ParamSpec::num("Release time", 0.0, 1000.0, 100.0)
            .unit("ms")
            .decimals(1)
            .when(MAXIMIZING),
        ParamSpec::switch("L", 1.0).group("Channels to use for the control signal"),
        ParamSpec::switch("R", 1.0),
        ParamSpec::switch("C", 1.0),
        ParamSpec::switch("Ls", 1.0),
        ParamSpec::switch("Rs", 1.0),
        ParamSpec::switch("LFE", 0.0),
        ParamSpec::switch("L", 1.0).group("Channels to affect"),
        ParamSpec::switch("R", 1.0),
        ParamSpec::switch("C", 1.0),
        ParamSpec::switch("Ls", 1.0),
        ParamSpec::switch("Rs", 1.0),
        ParamSpec::switch("LFE", 0.0),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, -12.0, 4.0, 0.0, 1.0, 0.0, 0.0, 1.0, 2.0, 100.0, 0.0, 0.0, 0.0, 0.0, 0.0, 100.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        ("Deep compression", &[0.0, -35.0, 8.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 100.0, 0.0, 0.0, 0.0, 0.0, 0.0, 100.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        ("Medium compression", &[0.0, -20.0, 2.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 25.0, 0.0, 0.0, 0.0, 0.0, 0.0, 100.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        ("Master for 16-bit", &[0.0, -6.0, 1.5, 0.0, 1.0, 1.0, 0.0, 1.0, 150.0, 150.0, 0.0, 0.0, 0.0, 0.0, 0.0, 20.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        ("Voice", &[0.0, -10.0, 5.0, 0.0, 1.0, 1.0, 0.0, 0.0, 5.0, 50.0, 0.0, 0.0, 0.0, 0.0, 0.0, 20.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        ("Guitar", &[0.0, -20.0, 5.0, -6.0, 1.0, 1.0, 0.0, 0.0, 15.0, 15.0, 0.0, 0.0, 0.0, 0.0, 0.0, 100.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        ("Bass", &[0.0, -20.0, 6.0, -6.0, 1.0, 1.0, 0.0, 0.0, 40.0, 80.0, 0.0, 0.0, 0.0, 0.0, 0.0, 100.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        ("Smooth compression", &[0.0, -8.0, 3.5, 0.0, 1.0, 1.0, 0.0, 1.0, 10.0, 1000.0, 0.0, 0.0, 0.0, 0.0, 0.0, 20.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        ("Drums", &[0.0, -10.0, 3.5, 0.0, 0.0, 1.0, 0.0, 1.0, 3.0, 150.0, 0.0, 0.0, 0.0, 0.0, 0.0, 20.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        ("Limit at 6 dB and maximize", &[1.0, -6.0, 50.0, 0.0, 1.0, 0.0, 0.0, 1.0, 2.0, 100.0, 0.0, 0.0, -6.0, 0.0, 0.0, 100.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        ("Bypass", &[1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 2.0, 100.0, 0.0, 1.0, 0.0, 0.0, 0.0, 100.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        ("Guitar distortion 1", &[0.0, -30.0, 10.0, -10.0, 0.0, 1.0, 0.0, 0.0, 3.0, 5.0, 0.0, 0.0, 0.0, 0.0, 0.0, 100.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        ("Guitar distortion 2", &[0.0, -35.0, 20.0, -10.0, 1.0, 1.0, 1.0, 1.0, 0.3, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 100.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        ("Guitar distortion 3 (fuzzy)", &[0.0, -35.0, 20.0, -10.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 100.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
    ];
}

/// Gate: below the threshold the sound is lowered by the range (all the
/// way at 90 dB), opened and closed smoothly, held a while, steered by a
/// filtered copy of itself and seen a little ahead.
pub mod efx_gate {
    use super::*;

    pub const INPUT: usize = 0;
    pub const OUTPUT: usize = 1;
    pub const THRESHOLD: usize = 2;
    pub const RANGE: usize = 3;
    pub const ATTACK: usize = 4;
    pub const RELEASE: usize = 5;
    pub const HOLD: usize = 6;
    pub const SOFT_KNEE: usize = 7;
    pub const LOOK_AHEAD: usize = 8;
    pub const FILTER_TYPE: usize = 9;
    pub const FILTER: usize = 10;
    pub const LISTEN: usize = 11;
    pub const INPUT_HIGH_PASS: usize = 12;

    const FILTERED: &[(u8, u32)] = &[(FILTER_TYPE as u8, !1)];

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Input", -24.0, 24.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Output", -24.0, 24.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Threshold", -90.0, 0.0, -24.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Range", 0.0, 90.0, 12.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Attack", 0.05, 100.0, 0.05)
            .unit("ms")
            .decimals(2)
            .log(),
        ParamSpec::num("Release", 10.0, 2500.0, 250.0)
            .unit("ms")
            .decimals(0)
            .log(),
        ParamSpec::choice("Hold", 0.0, &["Short", "Medium", "Long"], 1.0),
        ParamSpec::switch("Soft knee", 0.0),
        ParamSpec::num("Look ahead", 0.0, 5.0, 0.5)
            .unit("ms")
            .decimals(2),
        ParamSpec::choice(
            "Side chain filter",
            0.0,
            &["Off", "High-pass", "Band-pass", "Low-pass"],
            1.0,
        )
        .group("Side chain"),
        ParamSpec::num("Filter frequency", 20.0, 10000.0, 20.0)
            .unit("Hz")
            .decimals(0)
            .log()
            .when(FILTERED),
        ParamSpec::switch("Listen to the side chain", 0.0),
        ParamSpec::num("Input high-pass", 0.0, 500.0, 20.0)
            .unit("Hz")
            .decimals(0),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default", &[0.0, 0.0, -24.0, 12.0, 0.05, 250.29, 1.0, 0.0, 0.5, 1.0, 20.0, 0.0, 20.0]),
        ("Attack Enhancer 6dB Kick", &[0.0, 6.0, -12.01, 6.0, 0.05, 80.12, 0.0, 0.0, 0.5, 3.0, 135.06, 0.0, 20.0]),
        ("Attack Enhancer 6dB Snare", &[0.0, 6.0, -12.01, 6.0, 0.05, 80.12, 0.0, 0.0, 0.5, 2.0, 806.47, 0.0, 20.0]),
        ("Delete Breath", &[0.0, 0.0, -16.98, 49.9, 0.87, 126.57, 2.0, 1.0, 0.5, 0.0, 2670.94, 0.0, 20.0]),
        ("Snare Cleanup", &[0.0, 0.0, -27.4, 16.04, 0.05, 118.99, 1.0, 0.0, 0.5, 2.0, 276.19, 0.0, 20.0]),
        ("Try On Drums", &[0.0, 0.0, -27.4, 14.49, 0.05, 109.3, 1.0, 1.0, 0.5, 2.0, 1079.53, 0.0, 20.0]),
        ("Voiceover Gate", &[0.0, 0.0, -40.96, 21.79, 4.99, 69.55, 1.0, 0.0, 0.5, 1.0, 60.06, 0.0, 20.0]),
        ("Vox Cleanup", &[0.0, 0.0, -43.49, 25.96, 5.43, 210.11, 2.0, 1.0, 0.5, 2.0, 821.31, 0.0, 20.0]),
    ];
}

/// DeEsser: the hiss of "s" sounds turned down when a band around its
/// frequency stands out from the rest of the sound.
pub mod efx_deesser {
    use super::*;

    pub const INPUT: usize = 0;
    pub const OUTPUT: usize = 1;
    pub const FREQUENCY: usize = 2;
    pub const REDUCTION: usize = 3;
    pub const Q: usize = 4;
    pub const LISTEN: usize = 5;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Input", -24.0, 24.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Output", -24.0, 24.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Frequency", 2000.0, 12000.0, 7000.0)
            .unit("Hz")
            .decimals(0)
            .log(),
        ParamSpec::num("Reduction", 0.0, 36.0, 6.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Filter Q", 0.1, 2.0, 0.18).decimals(2).log(),
        ParamSpec::switch("Listen", 0.0),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default", &[0.0, 0.0, 7000.0, 6.0, 0.18, 0.0]),
        ("Female Vox", &[0.0, 0.0, 8000.0, 9.3, 0.18, 0.0]),
        ("Male Vox", &[0.0, 0.0, 5950.0, 7.6, 0.18, 0.0]),
    ];
}

/// Limiter: the sound held under its threshold (seen ahead, by its peak
/// or its power), brought back up when asked, then pushed into a clipper
/// whose shape runs from soft to hard.
pub mod efx_limiter {
    use super::*;

    pub const INPUT: usize = 0;
    pub const OUTPUT: usize = 1;
    pub const THRESHOLD: usize = 2;
    pub const RELEASE: usize = 3;
    pub const CLIP_GAIN: usize = 4;
    pub const ATTACK: usize = 5;
    pub const ADAPTIVE: usize = 6;
    pub const PEAK_RMS: usize = 7;
    pub const MAKEUP: usize = 8;
    pub const INPUT_LOW_CUT: usize = 9;
    pub const SIDE_LOW_CUT: usize = 10;
    pub const CLIP_SHAPE: usize = 11;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Input", -24.0, 24.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Output", -24.0, 0.0, -0.1)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Threshold", -30.0, 0.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Release", 1.0, 1000.0, 10.0)
            .unit("ms")
            .decimals(1)
            .log(),
        ParamSpec::num("Clip gain", -12.0, 12.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Attack", 1.0, 100.0, 1.0)
            .unit("ms")
            .decimals(1)
            .log()
            .group("Tweak"),
        ParamSpec::num("Adaptive release", 0.0, 1.0, 1.0)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Peak vs. RMS", 0.0, 1.0, 0.0)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::switch("Auto makeup", 1.0),
        ParamSpec::num("Input low cut", 0.0, 100.0, 0.0)
            .unit("Hz")
            .decimals(0),
        ParamSpec::num("Side chain low cut", 0.0, 500.0, 0.0)
            .unit("Hz")
            .decimals(0),
        ParamSpec::num("Clip shape", 0.0, 1.0, 1.0)
            .show(Show::Percent)
            .decimals(0),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default", &[0.0, -0.1, 0.0, 10.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0]),
        ("Default - no AutoMakeup", &[0.0, -0.1, 0.0, 10.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]),
        ("Classical Limiter (Transparent)", &[0.0, -0.1, 0.0, 205.05, 0.0, 1.0, 1.0, 0.0, 1.0, 25.0, 0.0, 0.85]),
        ("Dance Limiter (Loud)", &[0.0, -0.1, 0.0, 7.01, 0.0, 1.43, 0.65, 0.1, 1.0, 20.0, 49.7, 0.85]),
        ("Dance Limiter (Moderate)", &[0.0, -0.1, 0.0, 12.19, 0.0, 1.21, 0.86, 0.0, 1.0, 20.0, 20.14, 0.86]),
        ("Dance Limiter (Tight Bass)", &[0.0, -0.1, 0.0, 19.68, 0.0, 5.0, 0.91, 0.16, 1.0, 40.0, 60.77, 0.96]),
        ("Drum Buss Fattener +6dB", &[0.0, -0.1, -3.0, 15.43, 3.0, 7.03, 0.5, 0.2, 1.0, 20.0, 39.65, 0.67]),
        ("HipHop Limiter +6dB (Tight Bass)", &[0.0, -0.1, -6.0, 5.05, 0.0, 2.81, 0.39, 0.22, 1.0, 21.88, 39.65, 0.75]),
        ("Pop Maximizer +6dB (Transparent)", &[0.0, -0.1, -6.0, 34.64, 0.0, 1.0, 1.0, 0.0, 1.0, 15.0, 25.18, 0.85]),
        ("Rock Clipper +6dB (Soft Transients)", &[0.0, -0.1, -3.0, 10.38, 3.0, 1.96, 0.81, 0.21, 1.0, 20.0, 65.22, 0.41]),
        ("Rock Maximizer +6dB", &[0.0, -0.1, -6.0, 5.16, 0.0, 10.46, 0.75, 0.2, 1.0, 20.0, 49.7, 0.76]),
    ];
}

/// Tremolo Pan: the level of each side following an oscillator from a
/// sine toward a square; the right side's oscillator moved on by the phase
/// (half a round: an auto-pan).
pub mod efx_tremolo_pan {
    use super::*;

    pub const INPUT: usize = 0;
    pub const OUTPUT: usize = 1;
    pub const RATE: usize = 2;
    pub const SHAPE: usize = 3;
    pub const PHASE: usize = 4;
    pub const DEPTH: usize = 5;
    pub const SYNC: usize = 6;
    pub const NOTE: usize = 7;

    const FREE: &[(u8, u32)] = &[(SYNC as u8, OFF)];
    const SYNCED: &[(u8, u32)] = &[(SYNC as u8, ON)];

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Input", -24.0, 24.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Output", -24.0, 24.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Rate", 0.2, 2850.0, 1.0)
            .unit("Hz")
            .decimals(2)
            .log()
            .when(FREE),
        ParamSpec::num("Shape", 0.0, 1.0, 0.0)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Phase", 0.0, 1.0, 0.0)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Depth", 0.0, 1.0, 1.0)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::switch("Tempo sync", 0.0),
        ParamSpec::choice("Note", 0.0, tempo::NOTES_OR_MEASURE, tempo::QUARTER).when(SYNCED),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default", &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 13.0]),
        ("440 Hz noise", &[0.0, 0.0, 440.0, 0.0, 0.0, 1.0, 0.0, 13.0]),
        (
            "Fast modulation",
            &[0.0, 0.0, 5.28, 0.75, 0.0, 1.0, 0.0, 13.0],
        ),
        (
            "Slow modulation",
            &[0.0, 0.0, 1.4, 0.5, 0.0, 1.0, 0.0, 13.0],
        ),
    ];
}

/// Tube Stage: one or two triode stages (class A or AB) driven by the
/// gain, an EQ before and after, and the stages run at twice the rate
/// when oversampling.
pub mod efx_tube_stage {
    use super::*;

    pub const INPUT: usize = 0;
    pub const OUTPUT: usize = 1;
    pub const PRE_EQ: usize = 2;
    pub const DRIVE: usize = 3;
    pub const POST_EQ: usize = 4;
    pub const OVERSAMPLE: usize = 5;
    pub const STAGES: usize = 6;
    pub const CLASS: usize = 7;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Input", -24.0, 24.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Output", -24.0, 24.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Pre EQ", -1.0, 1.0, 0.0).decimals(2),
        ParamSpec::num("Drive", 0.0, 24.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Post EQ", -1.0, 1.0, 0.0).decimals(2),
        ParamSpec::switch("Oversample", 1.0),
        ParamSpec::choice("Stages", 1.0, &["1", "2"], 1.0),
        ParamSpec::choice("Class", 0.0, &["A", "AB"], 0.0),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default", &[0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0]),
        ("Aggressive Bassdrum", &[10.08, -16.8, -0.57, 18.36, -0.57, 1.0, 1.0, 0.0]),
        ("Reamp", &[0.0, -9.36, 0.47, 11.87, -0.22, 1.0, 2.0, 1.0]),
        ("Bluesguitar", &[18.72, -22.56, -0.11, 0.91, 0.32, 1.0, 1.0, 0.0]),
        ("Bring Forward", &[5.28, -8.4, 0.29, 5.74, 0.32, 0.0, 1.0, 1.0]),
        ("Crunchy Guitar", &[18.72, -22.56, 0.18, 7.67, 0.28, 1.0, 1.0, 0.0]),
        ("Dirty Drums", &[10.08, -12.0, -0.22, 9.99, 0.36, 1.0, 1.0, 0.0]),
        ("Fatten", &[14.16, -19.2, -0.55, 13.11, 0.59, 0.0, 1.0, 1.0]),
        ("Loud Neighbours", &[5.28, -13.2, -1.0, 7.67, -1.0, 1.0, 1.0, 1.0]),
        ("Lush Mix", &[5.76, -10.08, -0.39, 11.32, 0.34, 0.0, 2.0, 1.0]),
        ("Megaphone", &[0.0, -15.12, 0.56, 15.15, 0.67, 1.0, 2.0, 0.0]),
        ("More Presence", &[5.28, -8.4, 0.48, 8.33, 0.43, 0.0, 2.0, 0.0]),
        ("Old Radio", &[5.52, -10.56, 0.02, 9.99, 0.0, 1.0, 2.0, 0.0]),
        ("Older Radio", &[5.52, -13.68, -0.5, 12.49, -0.27, 1.0, 2.0, 0.0]),
        ("Rhythm Guitar", &[9.36, -22.56, 0.22, 24.0, -0.46, 1.0, 2.0, 0.0]),
        ("Rough Vocals", &[11.04, -15.84, -0.26, 9.25, 0.21, 1.0, 2.0, 0.0]),
        ("Tube Fryer", &[0.0, -7.68, 1.0, 8.33, -0.33, 1.0, 2.0, 0.0]),
        ("Vintage Organ Toolkit", &[8.88, -16.32, -0.62, 6.87, 0.2, 1.0, 1.0, 0.0]),
        ("Warm Tube Preamp", &[0.0, -5.04, 0.26, 6.67, -0.27, 1.0, 2.0, 0.0]),
        ("Tube console - Master Buss", &[0.0, -5.28, 0.19, 5.93, -0.07, 1.0, 2.0, 1.0]),
        ("Tube console - place on every mixer channel", &[0.0, -0.96, 0.19, 1.94, -0.07, 1.0, 2.0, 1.0]),
    ];
}

/// Vocal Strip: a voice's chain in one — a high-pass, a gate, a de-esser,
/// a compressor and a tone tilt.
pub mod efx_vocal_strip {
    use super::*;

    pub const INPUT: usize = 0;
    pub const OUTPUT: usize = 1;
    pub const HIGH_PASS: usize = 2;
    pub const GATE: usize = 3;
    pub const DEESSER: usize = 4;
    pub const COMPRESSION: usize = 5;
    pub const TONE: usize = 6;
    pub const DEESS_FREQUENCY: usize = 7;
    pub const ATTACK: usize = 8;
    pub const ADAPTIVE: usize = 9;
    pub const MAKEUP: usize = 10;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Input", -24.0, 24.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Output", -24.0, 24.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("High-pass", 20.0, 500.0, 20.0)
            .unit("Hz")
            .decimals(0)
            .log(),
        ParamSpec::num("Gate", -90.0, 0.0, -90.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::num("De-esser", 0.0, 36.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Compression", 0.0, 48.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Tone", -1.0, 1.0, 0.0).decimals(2),
        ParamSpec::num("De-ess frequency", 4000.0, 12000.0, 7000.0)
            .unit("Hz")
            .decimals(0)
            .log()
            .group("Tweak"),
        ParamSpec::num("Compressor attack", 0.2, 10.0, 0.7)
            .unit("ms")
            .decimals(1)
            .log(),
        ParamSpec::num("Compressor adaptive release", 0.0, 10.0, 4.0).decimals(1),
        ParamSpec::switch("Compressor auto makeup", 1.0),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default", &[0.0, 0.0, 20.0, -90.0, 0.0, 0.0, 0.0, 7000.0, 0.7, 4.0, 1.0]),
        ("Vocal with de-squeak", &[0.0, 0.0, 50.0, -90.0, 18.36, 7.92, 0.16, 7000.0, 0.7, 4.0, 1.0]),
        ("Vocal", &[0.0, 0.0, 50.0, -90.0, 0.0, 7.92, 0.16, 7000.0, 0.7, 4.0, 1.0]),
        ("Bass Guitar channel", &[0.0, 0.0, 20.0, -90.0, 0.0, 11.28, 0.4, 7000.0, 0.7, 4.0, 1.0]),
        ("Breakbeats", &[0.0, 3.0, 50.0, -30.02, 9.0, 10.05, 0.25, 7000.0, 0.7, 4.0, 1.0]),
        ("Vocal (strong)", &[0.0, 0.0, 20.0, -90.0, 0.0, 14.64, 0.35, 7000.0, 0.7, 4.0, 1.0]),
        ("Vocal (gated)", &[0.0, 0.0, 57.9, -49.42, 0.0, 10.32, 0.53, 7000.0, 0.7, 4.0, 1.0]),
        ("Commercial", &[0.0, 4.8, 119.0, -37.73, 16.02, 32.16, 0.5, 7000.0, 0.7, 4.0, 1.0]),
        ("Female Rock", &[0.0, 4.0, 120.09, -42.36, 15.3, 24.96, 0.51, 7000.0, 0.7, 4.0, 1.0]),
        ("Female Up-Front", &[0.0, 3.12, 129.03, -27.31, 15.8, 30.24, 0.29, 7000.0, 1.45, 2.25, 1.0]),
        ("Female Voiceover Soft", &[6.0, 6.0, 79.65, -58.32, 10.8, 23.04, 0.25, 7000.0, 0.7, 4.0, 1.0]),
        ("Female Voiceover", &[0.0, 2.16, 100.02, -39.23, 10.08, 24.0, 0.41, 7000.0, 0.7, 4.0, 1.0]),
        ("Male Rap vocal", &[0.0, 0.0, 50.0, -90.0, 7.74, 15.36, 0.33, 5840.0, 0.7, 4.0, 1.0]),
        ("Male Rock", &[0.0, 3.6, 79.62, -43.0, 12.04, 28.56, 0.46, 6320.0, 1.5, 1.7, 1.0]),
        ("Male vocal", &[0.0, 0.0, 49.26, -90.0, 6.84, 13.44, 0.14, 6440.0, 0.7, 4.0, 1.0]),
        ("Male Voiceover", &[0.0, 2.4, 109.81, -45.2, 12.96, 24.96, 0.51, 6120.0, 0.7, 2.32, 1.0]),
        ("Rap", &[0.0, 2.0, 79.65, -42.02, 15.12, 30.7, 0.51, 5600.0, 0.7, 1.7, 1.0]),
        ("Talkback Drum Treatment", &[0.0, 3.12, 150.02, -90.0, 0.0, 24.0, -0.69, 4680.0, 0.2, 0.3, 1.0]),
        ("Vintage Vox Channel", &[5.04, 4.32, 129.03, -40.79, 6.3, 21.84, 0.19, 6800.0, 4.05, 5.34, 1.0]),
    ];
}

/// Compressor: a feedback compressor (its gain steers its own detector),
/// a three-speed release that follows the material, parallel mix, soft
/// clip, and a filtered side chain.
pub mod efx_compressor {
    use super::*;

    pub const INPUT: usize = 0;
    pub const OUTPUT: usize = 1;
    pub const THRESHOLD: usize = 2;
    pub const RATIO: usize = 3;
    pub const ATTACK: usize = 4;
    pub const RELEASE: usize = 5;
    pub const ADAPTIVE: usize = 6;
    pub const MAKEUP: usize = 7;
    pub const SOFT_CLIP: usize = 8;
    pub const MIX: usize = 9;
    pub const FILTER_TYPE: usize = 10;
    pub const FILTER: usize = 11;
    pub const LISTEN: usize = 12;

    const FILTERED: &[(u8, u32)] = &[(FILTER_TYPE as u8, !1)];

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Input", 0.0, 1.0, 1.0)
            .show(Show::GainDb)
            .decimals(1),
        ParamSpec::num("Output", -24.0, 24.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Threshold", -40.0, 0.0, 0.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Ratio (x:1)", 1.0, 20.0, 5.75)
            .decimals(1)
            .log(),
        ParamSpec::num("Attack", 0.1, 100.0, 3.16)
            .unit("ms")
            .decimals(2)
            .log(),
        ParamSpec::num("Release", 10.0, 2000.0, 141.4)
            .unit("ms")
            .decimals(0)
            .log(),
        ParamSpec::num("Adaptive release", 0.0, 10.0, 2.3)
            .decimals(1)
            .group("Tweak"),
        ParamSpec::switch("Auto makeup", 0.0),
        ParamSpec::switch("Soft clip", 0.0),
        ParamSpec::num("Mix", 0.0, 1.0, 1.0)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::choice(
            "Side chain filter",
            0.0,
            &["Off", "High-pass", "Band-pass", "Low-pass"],
            1.0,
        )
        .group("Side chain"),
        ParamSpec::num("Filter frequency", 20.0, 10000.0, 20.0)
            .unit("Hz")
            .decimals(0)
            .log()
            .when(FILTERED),
        ParamSpec::switch("Listen to the side chain", 0.0),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default", &[1.0, 0.0, 0.0, 5.75, 3.1623, 141.4, 2.3, 0.0, 0.0, 1.0, 1.0, 20.0, 0.0]),
        ("FET style 1", &[1.0, 3.84, -9.2667, 11.45, 0.1413, 952.5, 2.3, 0.0, 0.0, 1.0, 1.0, 20.0, 0.0]),
        ("FET - 1-Parallel compression - Female vocal", &[1.0, 1.92, -9.2667, 5.9743, 0.2723, 348.1, 2.3, 0.0, 1.0, 0.7562, 1.0, 20.0, 0.0]),
        ("FET style 2", &[1.0, 3.84, -11.27, 11.45, 0.1, 10.0, 2.3, 0.0, 0.0, 1.0, 1.0, 20.0, 0.0]),
        ("FET - Drum buss", &[1.0, 6.0, -14.47, 14.3, 0.1514, 17.91, 2.3, 0.0, 1.0, 1.0, 0.0, 20.0, 0.0]),
        ("FET - Parallel Comp. Drum buss", &[1.0, 6.24, -16.87, 14.3, 0.1514, 17.91, 2.3, 0.0, 1.0, 0.6625, 0.0, 20.0, 0.0]),
        ("FET - Parallel compression - Hairy", &[1.0, 3.6, -10.6, 20.0, 0.1, 10.0, 2.3, 0.0, 1.0, 0.9125, 1.0, 20.0, 0.0]),
        ("Bus Comp - Drums NY Style", &[1.0, 15.02, -25.78, 20.0, 0.1, 89.25, 2.3, 0.0, 0.0, 1.0, 1.0, 182.3, 0.0]),
        ("Bus Comp - snare enhancement", &[1.0, 4.387, -12.43, 20.0, 1.0469, 34.92, 2.3, 0.0, 0.0, 1.0, 1.0, 182.3, 0.0]),
        ("Bus Comp - snare enhancement 2", &[1.0, 17.6, -40.0, 3.2313, 15.73, 212.9, 2.3, 0.0, 0.0, 1.0, 1.0, 182.3, 0.0]),
        ("Classic Program Limiter", &[1.0, 0.0, -4.0, 20.0, 0.1719, 13.1, 3.0, 1.0, 1.0, 1.0, 1.0, 272.0, 0.0]),
        ("Acoustic Guitar", &[1.0, 3.2773, -12.728, 4.3057, 15.49, 74.15, 2.3, 0.0, 0.0, 1.0, 1.0, 20.0, 0.0]),
        ("Bass Guitar", &[1.0, 3.2773, -13.33, 5.5161, 10.85, 502.9, 2.3, 0.0, 0.0, 1.0, 1.0, 20.0, 0.0]),
        ("Drums", &[1.0, 4.4474, -8.5518, 6.2066, 0.5068, 45.9, 2.3, 0.0, 0.0, 1.0, 1.0, 20.0, 0.0]),
        ("Electric Guitar", &[1.0, 0.0, -9.3978, 1.7886, 0.2079, 31.45, 2.3, 0.0, 0.0, 1.0, 1.0, 20.0, 0.0]),
        ("Master Mixdown", &[1.0, 1.05, -7.113, 1.6097, 0.1312, 36.542, 2.3, 0.0, 0.0, 1.0, 1.0, 20.0, 0.0]),
        ("OverCompressed Vocals", &[1.0, 9.0152, -21.93, 17.99, 0.6415, 16.84, 2.3, 0.0, 0.0, 1.0, 1.0, 20.0, 0.0]),
        ("Premastering", &[1.0, 0.0, -7.113, 1.6008, 0.1705, 182.5, 2.3, 0.0, 0.0, 1.0, 1.0, 20.0, 0.0]),
        ("Vocals", &[1.0, 3.9787, -13.33, 2.2777, 0.1172, 16.0, 2.3, 0.0, 0.0, 1.0, 1.0, 20.0, 0.0]),
        ("Drum Buss Ambience & Fatness", &[1.0, 4.9148, -13.15, 11.73, 0.2335, 34.48, 2.3, 0.0, 1.0, 1.0, 1.0, 242.9, 0.0]),
        ("Drum Buss Ambience", &[1.0, 4.9148, -13.15, 11.73, 0.2335, 34.48, 2.3, 0.0, 1.0, 1.0, 1.0, 242.9, 0.0]),
        ("Drum Slammer", &[1.0, 4.9148, -22.55, 13.98, 0.1653, 34.48, 2.3, 0.0, 0.0, 1.0, 1.0, 126.5, 0.0]),
        ("Extreme Guitar Compression", &[1.0, 8.3636, -20.875, 7.0159, 1.4974, 484.4, 2.3, 0.0, 0.0, 1.0, 1.0, 77.0, 0.0]),
        ("Fat Kick", &[1.0, 2.64, -10.2, 3.6011, 3.1623, 397.4, 2.3, 0.0, 0.0, 1.0, 0.0, 20.0, 0.0]),
        ("Hot Master", &[1.0, 4.8, -13.2, 1.4275, 5.8884, 18.39, 2.3, 0.0, 1.0, 1.0, 1.0, 807.1, 0.0]),
        ("Peak Stop", &[1.0, 0.0, 0.0, 20.0, 0.1, 10.0, 2.3, 0.0, 1.0, 1.0, 0.0, 20.0, 0.0]),
        ("Reggae Guitar Strong", &[1.0, 5.8498, -13.22, 3.5303, 7.2066, 110.9, 2.3, 0.0, 0.0, 1.0, 1.0, 20.0, 0.0]),
        ("Sequencer Compression", &[1.0, 5.76, -14.931, 12.9, 0.5248, 44.4, 2.3, 0.0, 0.0, 1.0, 1.0, 178.0, 0.0]),
        ("Synth Bass Compression", &[1.0, 4.7383, -14.931, 10.18, 3.3114, 79.77, 2.3, 0.0, 0.0, 1.0, 1.0, 121.7, 0.0]),
        ("Vintage VCA style", &[1.0, 1.68, -10.6, 8.6613, 3.6308, 693.1, 2.3, 0.0, 0.0, 1.0, 1.0, 20.0, 0.0]),
        ("Parallel Compression - Glue", &[1.0, 3.6, -16.6, 5.8455, 0.1567, 85.49, 2.3, 0.0, 1.0, 0.7375, 1.0, 20.0, 0.0]),
        ("Parallel Kick Compression", &[1.0, 1.92, -11.2, 11.55, 0.1567, 83.26, 2.3, 0.0, 1.0, 0.7188, 1.0, 20.0, 0.0]),
        ("Parallel Overhead Drum Compression", &[1.0, 3.6, -16.6, 11.55, 0.1567, 95.05, 2.3, 0.0, 1.0, 0.7375, 1.0, 20.0, 0.0]),
    ];
}

/// Phaser: four, eight or twelve all-pass stages swept around a centre
/// by an oscillator (the right side's moved on by the phase), fed back,
/// mixed with the sound.
pub mod efx_phaser {
    use super::*;

    pub const MODE: usize = 0;
    pub const INPUT: usize = 1;
    pub const OUTPUT: usize = 2;
    pub const RATE: usize = 3;
    pub const DEPTH: usize = 4;
    pub const PHASE: usize = 5;
    pub const FEEDBACK: usize = 6;
    pub const CENTER: usize = 7;
    pub const MIX: usize = 8;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::choice("Stages", 0.0, &["4", "8", "12"], 0.0),
        ParamSpec::num("Input", 0.0, 1.0, 1.0)
            .show(Show::GainDb)
            .decimals(1),
        ParamSpec::num("Output", 0.0, 1.0, 1.0)
            .show(Show::GainDb)
            .decimals(1),
        ParamSpec::num("Rate", 0.01, 15.0, 3.008)
            .unit("Hz")
            .decimals(2)
            .log(),
        ParamSpec::num("Depth", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Phase offset", 0.0, 1.0, 0.0)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Feedback", 0.0, 1.0, 0.0)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Center frequency", 50.0, 5000.0, 850.0)
            .unit("Hz")
            .decimals(0)
            .log(),
        ParamSpec::num("Mix", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(0),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default", &[0.0, 1.0, 1.0, 3.008, 0.5, 0.0, 0.0, 850.0, 0.5]),
        ("4 Stage Feedback", &[0.0, 1.0, 0.5, 0.01, 0.0, 0.0, 1.0, 850.0, 1.0]),
        ("Bad Experience", &[2.0, 1.0, 1.0, 5.0316, 0.65, 0.0, 0.225, 850.0, 1.0]),
        ("Drum Resonator", &[1.0, 1.0, 0.725, 10.66, 0.2676, 0.0, 0.8836, 581.8, 0.6525]),
        ("Fast Vibrato", &[0.0, 1.0, 1.0, 14.21, 0.4534, 0.1676, 0.0, 421.4, 1.0]),
        ("Fat Synth", &[2.0, 1.0, 1.0, 5.8154, 0.9231, 0.0837, 0.5, 603.7, 0.6]),
        ("Full Power", &[0.0, 1.0, 0.5, 15.0, 1.0, 0.0, 1.0, 850.0, 1.0]),
        ("Guitar Stereo Swirl", &[0.0, 1.0, 1.0, 8.8998, 0.915, 0.25, 0.0, 850.0, 0.5]),
        ("Monovibe", &[0.0, 1.0, 1.0, 4.0573, 0.915, 0.0, 0.07, 850.0, 0.5]),
        ("Mushroomed Drums", &[2.0, 1.0, 0.705, 5.5962, 0.5253, 0.1356, 1.0, 850.0, 0.67]),
        ("Phasing pad", &[1.0, 1.0, 1.0, 0.7838, 0.2586, 0.0, 0.3921, 850.0, 0.5946]),
        ("Spring Fx", &[1.0, 1.0, 1.0, 13.117, 0.5061, 0.0, 0.6691, 850.0, 0.615]),
        ("Synth Pad Liquid", &[1.0, 1.0, 1.0, 4.5463, 0.55, 1.0, 0.0, 394.0, 0.385]),
        ("Synth Pad Mover", &[1.0, 1.0, 1.0, 1.5547, 1.0, 0.25, 0.205, 403.0, 0.575]),
        ("Under Water", &[0.0, 1.0, 0.685, 8.5303, 0.91, 0.0, 0.504, 850.0, 0.8]),
    ];
}

/// Chorus/Flanger: three modulated voices a side (a chorus, a flanger or
/// a wide ensemble), fed back, their lows cut, mixed with the sound.
pub mod efx_chorus_flanger {
    use super::*;

    pub const MODE: usize = 0;
    pub const INPUT: usize = 1;
    pub const OUTPUT: usize = 2;
    pub const RATE: usize = 3;
    pub const DEPTH: usize = 4;
    pub const PHASE: usize = 5;
    pub const FEEDBACK: usize = 6;
    pub const LOW_CUT: usize = 7;
    pub const MIX: usize = 8;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::choice("Mode", 0.0, &["Chorus", "Flanger", "Ensemble"], 0.0),
        ParamSpec::num("Input", 0.0, 1.0, 1.0)
            .show(Show::GainDb)
            .decimals(1),
        ParamSpec::num("Output", 0.0, 1.0, 1.0)
            .show(Show::GainDb)
            .decimals(1),
        ParamSpec::num("Rate", 0.01, 15.0, 0.993)
            .unit("Hz")
            .decimals(2)
            .log(),
        ParamSpec::num("Depth", 0.0, 1.0, 0.25)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Phase offset", 0.0, 1.0, 0.25)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Feedback", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Low cut", 20.0, 1000.0, 20.0)
            .unit("Hz")
            .decimals(0)
            .log(),
        ParamSpec::num("Mix", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(0),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default", &[0.0, 1.0, 1.0, 0.9929, 0.25, 0.25, 0.5, 20.0, 0.5]),
        ("Acoustic Guitar", &[0.0, 1.0, 1.0, 1.0697, 0.3171, 0.0556, 0.4412, 64.55, 0.5]),
        ("Chorus Classic Bass", &[0.0, 1.0, 1.0, 0.8642, 0.372, 0.0, 0.4519, 199.8, 0.5]),
        ("Chorus Fx Any Direction", &[0.0, 1.0, 1.0, 15.0, 0.2075, 0.5, 0.5491, 20.0, 0.71]),
        ("Chorus Vocal Spread", &[0.0, 1.0, 1.0, 0.548, 0.2944, 0.5, 0.63, 90.0, 0.4899]),
        ("Flanger Pad Enhancer", &[1.0, 1.0, 1.0, 0.2571, 0.6719, 0.6665, 0.45, 50.04, 0.5]),
        ("Heavy Stereo Chorus Fx", &[0.0, 1.0, 0.175, 0.01, 1.0, 0.0, 1.0, 20.0, 1.0]),
        ("Heavy Stereo Flanger Fx", &[1.0, 1.0, 0.175, 0.01, 1.0, 0.0, 1.0, 20.0, 1.0]),
        ("Keys Shimmering", &[2.0, 1.0, 1.0, 1.0697, 0.6487, 0.474, 0.6762, 90.0, 0.595]),
        ("Lush And Wide Open", &[2.0, 1.0, 1.0, 0.8364, 0.6905, 0.5, 0.25, 20.0, 0.5]),
        ("Reggae Guitar Chorus", &[0.0, 1.0, 1.0, 0.6109, 0.3971, 0.0, 0.6129, 20.0, 0.3396]),
        ("Electric Piano Chorus", &[0.0, 1.0, 1.0, 0.4925, 0.25, 0.5, 0.7, 50.04, 0.31]),
        ("Stereo Ensemble", &[2.0, 1.0, 1.0, 0.657, 0.6538, 1.0, 0.2391, 90.0, 0.81]),
        ("Strong Guitar Chorus", &[0.0, 1.0, 1.0, 0.4694, 0.7418, 0.0, 0.6129, 45.26, 0.3333]),
        ("Vox Alien Warble", &[2.0, 1.0, 0.94, 4.7682, 0.5563, 1.0, 0.775, 74.96, 0.82]),
        ("Vox Chorus", &[0.0, 1.0, 1.0, 0.3233, 0.5791, 1.0, 0.26, 60.38, 0.5]),
        ("Vox Vibrato Mono", &[1.0, 1.0, 0.94, 7.505, 0.6719, 0.0, 0.225, 50.04, 1.0]),
        ("Vox Vibrato Stereo", &[1.0, 1.0, 0.94, 7.505, 0.6719, 1.0, 0.225, 50.04, 1.0]),
        ("Subtle movement for pads", &[0.0, 1.0, 1.0, 2.138, 0.2109, 0.0556, 0.6403, 39.6, 0.6706]),
        ("Try on Drums at 120 BPM - use mix and depth", &[1.0, 1.0, 0.8099, 0.2008, 1.0, 0.0, 0.9425, 20.0, 0.1532]),
        ("Vox Ensemble Warm + Decoupled", &[2.0, 1.0, 0.94, 0.1454, 0.3427, 1.0, 0.125, 200.1, 0.87]),
    ];
}

/// Studio Reverb: a plate, a room, a hall or a vintage plate, made of
/// diffusers and delay lines tapped for its stereo, with its size, time,
/// damping and gentle modulation, pre-delay, diffusion and low cut, mixed
/// with the sound.
pub mod efx_reverb {
    use super::*;

    pub const MODE: usize = 0;
    pub const INPUT: usize = 1;
    pub const OUTPUT: usize = 2;
    pub const SIZE: usize = 3;
    pub const TIME: usize = 4;
    pub const DAMPING: usize = 5;
    pub const MODULATION: usize = 6;
    pub const PREDELAY: usize = 7;
    pub const DIFFUSION: usize = 8;
    pub const LOW_CUT: usize = 9;
    pub const MIX: usize = 10;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::choice(
            "Mode",
            0.0,
            &["Plate", "Room", "Hall", "Vintage plate"],
            1.0,
        ),
        ParamSpec::num("Input", 0.0, 1.0, 1.0)
            .show(Show::GainDb)
            .decimals(1),
        ParamSpec::num("Output", 0.0, 1.0, 1.0)
            .show(Show::GainDb)
            .decimals(1),
        ParamSpec::num("Size", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Time", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Damping", 0.0, 1.0, 0.25)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Modulation", 0.0, 1.0, 0.25)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Pre-delay", 0.0, 250.0, 4.55)
            .unit("ms")
            .decimals(1)
            .group("Tweak"),
        ParamSpec::num("Diffusion", 0.1, 1.0, 1.0)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Low cut", 50.0, 1000.0, 100.0)
            .unit("Hz")
            .decimals(0)
            .log(),
        ParamSpec::num("Mix", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(0),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default", &[1.0, 1.0, 1.0, 0.5, 0.5, 0.25, 0.25, 4.5469, 1.0, 100.0, 0.5]),
        ("Cathedral", &[2.0, 1.0, 1.0, 0.75, 0.545, 0.3721, 0.3658, 14.24, 0.838, 138.2, 0.31]),
        ("Central Station", &[2.0, 1.0, 1.0, 0.905, 0.505, 0.7195, 0.6008, 31.54, 0.829, 253.6, 0.335]),
        ("Classic Snare Plate - wet", &[3.0, 0.995, 1.0, 0.145, 0.3535, 0.2979, 0.55, 4.6021, 1.0, 230.2, 1.0]),
        ("Classic Vocal Plate", &[3.0, 1.0, 1.0, 0.5, 0.4685, 0.3876, 0.355, 12.52, 1.0, 147.3, 0.22]),
        ("Drum Plate", &[0.0, 1.0, 1.0, 0.2628, 0.3577, 0.0941, 0.4819, 2.3006, 1.0, 151.9, 0.5]),
        ("Intimate Room", &[1.0, 1.0, 1.0, 0.1229, 0.1324, 0.5158, 0.367, 3.2772, 1.0, 184.6, 0.2454]),
        ("Large Concert Hall", &[2.0, 1.0, 1.0, 0.5714, 0.4126, 0.36, 0.3382, 24.87, 0.8155, 104.7, 0.2612]),
        ("Light Guitar Plate", &[0.0, 1.0, 1.0, 0.6561, 0.4267, 0.5056, 0.3085, 3.5266, 1.0, 154.3, 0.1999]),
        ("Living Room", &[1.0, 1.0, 1.0, 0.48, 0.24, 0.5791, 0.1358, 9.0206, 0.6085, 127.3, 0.28]),
        ("Reverb Drum Room", &[1.0, 1.0, 1.0, 0.2572, 0.0602, 0.0, 0.174, 3.5266, 1.0, 100.0, 0.2026]),
        ("Reverb Empty Hall", &[2.0, 1.0, 1.0, 0.6219, 0.6385, 0.4145, 0.355, 37.5, 0.478, 131.6, 0.1542]),
        ("Reverb Vocal Ambient Room", &[1.0, 1.0, 1.0, 1.0, 0.0, 0.4145, 0.1808, 6.4229, 0.8605, 136.0, 0.0984]),
        ("Reverb Vocal Hall", &[2.0, 1.0, 1.0, 1.0, 0.2416, 0.25, 0.0, 17.97, 1.0, 100.0, 0.1542]),
        ("SlapBack", &[1.0, 1.0, 1.0, 0.8, 0.455, 0.1024, 0.1808, 25.83, 0.433, 200.0, 0.1]),
        ("Small Club", &[2.0, 1.0, 1.0, 0.385, 0.25, 0.6719, 0.3158, 16.81, 0.856, 154.3, 0.2]),
        ("Small Hall", &[2.0, 1.0, 1.0, 0.335, 0.34, 0.3287, 0.3408, 10.17, 0.4431, 361.2, 0.405]),
        ("Soupe Du Jour", &[2.0, 1.0, 1.0, 0.615, 0.87, 0.5345, 0.5358, 25.108, 0.91, 395.1, 0.62]),
        ("Very Large & Rich", &[2.0, 1.0, 0.955, 0.88, 0.695, 0.0263, 0.5258, 43.62, 1.0, 147.3, 0.315]),
        ("Very Light Guitar Room", &[1.0, 1.0, 1.0, 0.3194, 0.4699, 0.5322, 0.3477, 11.85, 0.712, 142.7, 0.0838]),
        ("Vox Decoupler", &[1.0, 1.0, 1.0, 0.3879, 0.0574, 0.3687, 0.587, 0.0, 0.7885, 136.0, 0.2304]),
    ];
}

/// Stereo Delay: a delay a side (in milliseconds or notes of the tempo),
/// fed back through damping and a low cut, digital, analog (darker) or a
/// bucket-brigade line (its stages, compander, clock drift and noise),
/// mixed with the sound.
pub mod efx_stereo_delay {
    use super::*;

    pub const MODE: usize = 0;
    pub const INPUT: usize = 1;
    pub const OUTPUT: usize = 2;
    pub const LEFT: usize = 3;
    pub const RIGHT: usize = 4;
    pub const SYNC: usize = 5;
    pub const LEFT_NOTE: usize = 6;
    pub const RIGHT_NOTE: usize = 7;
    pub const FEEDBACK: usize = 8;
    pub const DAMPING: usize = 9;
    pub const LOW_CUT: usize = 10;
    pub const MIX: usize = 11;
    pub const STAGES: usize = 12;
    pub const COMPANDER: usize = 13;
    pub const DRIFT: usize = 14;
    pub const NOISE: usize = 15;

    const FREE: &[(u8, u32)] = &[(SYNC as u8, OFF)];
    const SYNCED: &[(u8, u32)] = &[(SYNC as u8, ON)];
    const BUCKETS: &[(u8, u32)] = &[(MODE as u8, 1 << 2)];

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::choice("Mode", 0.0, &["Digital", "Analog", "Bucket brigade"], 0.0),
        ParamSpec::num("Input", 0.0, 1.0, 1.0)
            .show(Show::GainDb)
            .decimals(1),
        ParamSpec::num("Output", 0.0, 1.0, 1.0)
            .show(Show::GainDb)
            .decimals(1),
        ParamSpec::num("Delay left", 13.9, 4108.0, 148.6)
            .unit("ms")
            .decimals(0)
            .log()
            .when(FREE),
        ParamSpec::num("Delay right", 13.9, 4108.0, 148.6)
            .unit("ms")
            .decimals(0)
            .log()
            .when(FREE),
        ParamSpec::switch("Tempo sync", 1.0),
        ParamSpec::choice("Left note", 0.0, tempo::NOTES, 9.0).when(SYNCED),
        ParamSpec::choice("Right note", 0.0, tempo::NOTES, 9.0).when(SYNCED),
        ParamSpec::num("Feedback", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Damping", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Feedback low cut", 10.0, 1000.0, 29.7)
            .unit("Hz")
            .decimals(0)
            .log(),
        ParamSpec::num("Mix", 0.0, 1.0, 0.5)
            .show(Show::Percent)
            .decimals(0),
        ParamSpec::num("Stages", 1.0, 4.0, 3.0)
            .decimals(0)
            .group("Bucket brigade")
            .when(BUCKETS),
        ParamSpec::num("Compander", 0.0, 10.0, 5.0)
            .decimals(1)
            .when(BUCKETS),
        ParamSpec::num("Clock drift", 0.0, 1.0, 0.021)
            .show(Show::Percent)
            .decimals(1)
            .when(BUCKETS),
        ParamSpec::num("Noise", -100.0, -40.0, -100.0)
            .show(Show::Db)
            .decimals(1)
            .when(BUCKETS),
    ];

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default", &[0.0, 1.0, 1.0, 148.6, 148.6, 1.0, 9.0, 9.0, 0.5, 0.5, 29.67, 0.5, 3.0, 5.0, 0.0208, -100.0]),
        ("1984 Shack Reverb", &[2.0, 1.0, 1.0, 92.53, 92.53, 0.0, 5.0, 5.0, 0.72, 0.19, 164.4, 1.0, 2.0, 5.0625, 0.0, -60.0]),
        ("Analog Dual Delay", &[1.0, 1.0, 1.0, 576.4, 376.3, 1.0, 13.0, 11.0, 0.765, 0.755, 10.0, 0.365, 4.0, 5.0, 0.0, -80.0]),
        ("Analog Feedback", &[1.0, 1.0, 0.73, 383.5, 92.53, 1.0, 11.0, 5.0, 0.855, 0.43, 10.0, 0.825, 4.0, 5.0, 0.01, -60.0]),
        ("Analog SlapBack", &[1.0, 1.0, 1.0, 92.53, 92.53, 0.0, 5.0, 5.0, 0.6878, 0.6893, 10.0, 0.5, 2.0, 5.0, 0.0, -60.0]),
        ("Analog Slow SlapBack", &[1.0, 1.0, 1.0, 13.9, 30.82, 1.0, 0.0, 1.0, 0.3, 0.27, 10.0, 0.34, 3.0, 5.0, 0.1, -60.0]),
        ("BBD Voice Doubler", &[2.0, 1.0, 0.955, 37.96, 37.96, 0.0, 3.0, 3.0, 0.48, 0.0, 231.1, 0.3, 1.0, 6.45, 0.5, -60.0]),
        ("Both Sides Of The Story", &[1.0, 1.0, 0.705, 148.6, 148.6, 1.0, 9.0, 9.0, 0.4219, 0.33, 10.0, 1.0, 4.0, 5.0, 0.01, -60.0]),
        ("Broaden", &[1.0, 1.0, 0.52, 13.9, 13.9, 1.0, 0.0, 0.0, 0.3967, 0.2659, 10.0, 1.0, 4.0, 5.0, 0.01, -60.0]),
        ("Bucket Brigade In Space", &[2.0, 1.0, 1.0, 35.86, 35.86, 1.0, 1.0, 1.0, 0.0612, 0.22, 185.3, 0.385, 3.0, 8.9, 0.3427, -55.4]),
        ("Delay Analog Old Tape", &[1.0, 1.0, 1.0, 512.3, 495.8, 1.0, 13.0, 13.0, 0.7504, 0.4765, 10.0, 0.5, 4.0, 5.0, 0.0, -80.0]),
        ("Delay Digital Ping Pong", &[0.0, 1.0, 1.0, 238.7, 238.7, 1.0, 10.0, 10.0, 0.3615, 0.0, 10.0, 0.2148, 4.0, 5.0, 0.0, -80.0]),
        ("Delay Digital Robot", &[0.0, 1.0, 1.0, 52.88, 40.99, 0.0, 2.0, 3.0, 0.896, 0.0, 10.0, 0.2572, 4.0, 5.0, 0.0, -80.0]),
        ("Delay Digital Vocal Slap", &[0.0, 1.0, 1.0, 13.9, 13.9, 1.0, 0.0, 0.0, 0.4765, 0.0, 10.0, 0.1566, 4.0, 5.0, 0.0, -80.0]),
        ("Dub Delay", &[2.0, 1.0, 1.0, 92.53, 92.53, 1.0, 5.0, 5.0, 0.8612, 0.695, 245.3, 1.0, 3.0, 7.4, 0.0338, -57.0]),
        ("Endless MonoDelay", &[1.0, 1.0, 0.38, 446.3, 529.3, 1.0, 13.0, 13.0, 0.925, 0.0, 10.0, 1.0, 4.0, 5.0, 0.0, -80.0]),
        ("Endless PingPong", &[1.0, 1.0, 0.38, 446.3, 683.7, 1.0, 13.0, 15.0, 0.06, 0.07, 70.39, 0.89, 4.0, 5.0, 0.01, -60.0]),
        ("Groove Analog", &[1.0, 1.0, 1.0, 232.1, 127.7, 0.0, 10.0, 7.0, 0.885, 0.07, 84.29, 0.775, 3.0, 5.0, 0.1, -60.0]),
        ("Groove Digital", &[0.0, 1.0, 1.0, 39.8, 127.7, 1.0, 3.0, 7.0, 0.885, 0.07, 10.0, 0.775, 4.0, 5.0, 0.0, -80.0]),
        ("Krankmachine", &[2.0, 1.0, 1.0, 148.6, 35.86, 1.0, 9.0, 1.0, 0.93, 0.145, 310.1, 0.55, 3.0, 8.55, 0.2143, -48.2]),
        ("Longest PingPong", &[1.0, 1.0, 1.0, 3085.7, 1432.0, 1.0, 17.0, 17.0, 0.37, 0.0, 10.0, 0.5, 4.0, 5.0, 0.0, -80.0]),
        ("Mid Delay", &[1.0, 1.0, 0.5, 148.6, 148.6, 1.0, 9.0, 9.0, 0.4219, 0.33, 10.0, 1.0, 4.0, 5.0, 0.01, -60.0]),
        ("Ping Pong Delay for Pads", &[1.0, 1.0, 1.0, 135.1, 161.9, 1.0, 7.0, 9.0, 0.3149, 0.8689, 120.7, 0.5024, 4.0, 5.0, 0.0, -80.0]),
        ("PingPong", &[0.0, 1.0, 1.0, 452.7, 323.5, 0.0, 13.0, 12.0, 0.4289, 0.2864, 10.0, 0.5, 4.0, 5.0, 0.0, -80.0]),
        ("ShortDelay", &[0.0, 1.0, 0.58, 13.9, 13.96, 1.0, 0.0, 0.0, 0.3967, 0.2659, 10.0, 1.0, 4.0, 5.0, 0.01, -60.0]),
        ("SimpleDelay", &[0.0, 1.0, 1.0, 989.4, 1589.3, 1.0, 16.0, 17.0, 0.585, 0.33, 10.0, 0.5, 4.0, 5.0, 0.01, -60.0]),
        ("Sweet Echo 1-8 Dot.", &[0.0, 1.0, 1.0, 225.5, 207.1, 1.0, 11.0, 11.0, 0.23, 0.0, 10.0, 0.115, 4.0, 5.0, 0.0, -80.0]),
        ("Sweet Echo Tripl.", &[0.0, 1.0, 1.0, 398.3, 101.7, 1.0, 11.0, 5.0, 0.23, 0.0, 10.0, 0.115, 4.0, 5.0, 0.0, -80.0]),
        ("Sweet Echo", &[0.0, 1.0, 1.0, 610.2, 267.5, 1.0, 15.0, 10.0, 0.23, 0.0, 10.0, 0.115, 4.0, 5.0, 0.0, -80.0]),
        ("Synth Arp 8th 16th BBD", &[2.0, 1.0, 1.0, 148.6, 35.86, 1.0, 9.0, 1.0, 0.28, 0.08, 45.651, 0.44, 3.0, 0.0, 0.0, -80.0]),
        ("Synth Arp 8th BBD", &[2.0, 1.0, 1.0, 148.6, 148.6, 1.0, 9.0, 9.0, 0.28, 0.08, 45.651, 0.44, 4.0, 0.0, 0.0, -80.0]),
        ("TripHop Triplets", &[1.0, 1.0, 0.715, 383.5, 383.5, 1.0, 11.0, 11.0, 0.905, 0.5868, 200.2, 1.0, 4.0, 5.0, 0.01, -60.0]),
        ("TripleDelay", &[1.0, 1.0, 0.705, 383.5, 383.5, 1.0, 11.0, 11.0, 0.905, 0.5868, 10.0, 1.0, 4.0, 5.0, 0.01, -60.0]),
        ("Vox BBD Slapback", &[2.0, 1.0, 1.0, 148.6, 92.53, 0.0, 9.0, 5.0, 0.7712, 0.22, 192.6, 0.255, 2.0, 7.05, 0.01, -55.4]),
        ("100% Mono Digidelay 1-16", &[0.0, 1.0, 1.0, 44.6, 33.56, 1.0, 7.0, 7.0, 0.5, 0.0, 10.0, 1.0, 4.0, 5.0, 0.0, -80.0]),
        ("100% Mono Digidelay 1-16 Tripl.", &[0.0, 1.0, 1.0, 24.547, 23.86, 1.0, 6.0, 6.0, 0.5, 0.0, 10.0, 1.0, 4.0, 5.0, 0.0, -80.0]),
        ("Analog Mono 1-4 Tripl.", &[1.0, 1.0, 1.0, 365.7, 317.3, 1.0, 12.0, 12.0, 0.665, 0.775, 10.0, 0.335, 3.0, 5.0, 0.0, -60.0]),
        ("Analog Mono 1-8", &[1.0, 1.0, 1.0, 135.2, 151.5, 1.0, 10.0, 10.0, 0.665, 0.775, 10.0, 0.335, 3.0, 5.0, 0.0, -60.0]),
        ("Analog Mono 1-8 Dot.", &[1.0, 1.0, 1.0, 190.2, 190.2, 1.0, 11.0, 11.0, 0.665, 0.775, 10.0, 0.335, 3.0, 5.0, 0.0, -60.0]),
        ("Analog Mono 1-8 Tripl.", &[1.0, 1.0, 1.0, 96.11, 114.0, 1.0, 9.0, 9.0, 0.665, 0.775, 10.0, 0.335, 3.0, 5.0, 0.0, -60.0]),
        ("Analog MonoDelay 124ms", &[1.0, 1.0, 1.0, 131.4, 131.4, 0.0, 7.0, 7.0, 0.8, 0.06, 10.0, 0.38, 3.0, 5.0, 0.0, -60.0]),
        ("Analog MonoDelay 251ms", &[1.0, 1.0, 1.0, 238.7, 238.7, 0.0, 10.0, 10.0, 0.805, 0.19, 10.0, 0.38, 4.0, 5.0, 0.0, -60.0]),
        ("Analog MonoDelay 343ms", &[1.0, 1.0, 1.0, 326.4, 326.4, 0.0, 12.0, 12.0, 0.8, 0.06, 10.0, 0.38, 4.0, 5.0, 0.0, -60.0]),
        ("Analog MonoDelay 433ms", &[1.0, 1.0, 1.0, 421.6, 421.6, 0.0, 11.0, 11.0, 0.825, 0.19, 10.0, 0.38, 4.0, 5.0, 0.0, -60.0]),
        ("Analog PingPong 1-2", &[1.0, 1.0, 1.0, 1473.3, 1604.4, 1.0, 16.0, 16.0, 0.335, 0.78, 10.0, 0.5, 4.0, 5.0, 0.0, -80.0]),
        ("Analog PingPong 1-4", &[1.0, 1.0, 1.0, 529.3, 514.5, 1.0, 13.0, 13.0, 0.335, 0.78, 10.0, 0.5, 4.0, 5.0, 0.0, -80.0]),
        ("Analog PingPong 1-4 Dot.", &[1.0, 1.0, 1.0, 961.7, 788.1, 1.0, 14.0, 14.0, 0.335, 0.78, 10.0, 0.5, 4.0, 5.0, 0.0, -80.0]),
        ("Analog PingPong 1-4 Tripl.", &[1.0, 1.0, 1.0, 345.5, 446.3, 1.0, 12.0, 12.0, 0.335, 0.78, 10.0, 0.5, 4.0, 5.0, 0.0, -80.0]),
        ("Analog PingPong 1-8", &[1.0, 1.0, 1.0, 127.7, 127.7, 1.0, 10.0, 10.0, 0.335, 0.78, 10.0, 0.5, 4.0, 5.0, 0.0, -80.0]),
        ("Analog PingPong 1-8 Dot.", &[1.0, 1.0, 1.0, 238.7, 245.6, 1.0, 11.0, 11.0, 0.335, 0.78, 10.0, 0.5, 4.0, 5.0, 0.0, -80.0]),
        ("Analog PingPong 1-8 Tripl.", &[1.0, 1.0, 1.0, 76.55, 81.03, 1.0, 9.0, 9.0, 0.335, 0.78, 10.0, 0.5, 4.0, 5.0, 0.0, -80.0]),
        ("Analog PingPong 644ms", &[1.0, 1.0, 1.0, 683.7, 683.7, 0.0, 15.0, 15.0, 0.275, 0.07, 10.0, 1.0, 4.0, 5.0, 0.0, -80.0]),
        ("Analog PingPong 1-16", &[1.0, 1.0, 1.0, 37.6, 35.53, 1.0, 7.0, 7.0, 0.335, 0.78, 10.0, 0.5, 3.0, 5.0, 0.1, -60.0]),
        ("Analog PingPong 1-16 Dot.", &[1.0, 1.0, 1.0, 47.21, 49.972, 1.0, 8.0, 8.0, 0.335, 0.78, 10.0, 0.5, 3.0, 5.0, 0.1, -60.0]),
        ("Analog PingPong 120ms", &[1.0, 1.0, 1.0, 127.7, 127.7, 0.0, 7.0, 7.0, 0.34, 0.255, 10.0, 0.38, 3.0, 5.0, 0.1, -60.0]),
        ("Analog PingPong 134ms", &[1.0, 1.0, 1.0, 139.1, 139.1, 0.0, 7.0, 7.0, 0.34, 0.255, 10.0, 0.38, 3.0, 5.0, 0.1, -60.0]),
        ("Analog PlingPlong", &[1.0, 1.0, 1.0, 326.4, 317.3, 0.0, 12.0, 12.0, 0.805, 0.185, 10.0, 0.69, 4.0, 5.0, 0.0, -80.0]),
        ("Digidelay 1-4", &[0.0, 1.0, 1.0, 514.5, 645.9, 1.0, 13.0, 13.0, 0.5, 0.0, 10.0, 1.0, 4.0, 5.0, 0.0, -80.0]),
        ("Digidelay 1-4 Tripl.", &[0.0, 1.0, 1.0, 365.7, 326.4, 1.0, 12.0, 12.0, 0.5, 0.0, 10.0, 1.0, 4.0, 5.0, 0.0, -80.0]),
        ("Digidelay 1-4 dot.", &[0.0, 1.0, 1.0, 989.4, 858.3, 1.0, 14.0, 14.0, 0.5, 0.0, 10.0, 1.0, 4.0, 5.0, 0.0, -80.0]),
        ("Digidelay 1-8", &[0.0, 1.0, 1.0, 174.6, 174.6, 1.0, 10.0, 10.0, 0.5, 0.0, 10.0, 1.0, 4.0, 5.0, 0.0, -80.0]),
        ("Digidelay 1-8 Tripl.", &[0.0, 1.0, 1.0, 104.7, 101.7, 1.0, 9.0, 9.0, 0.5, 0.0, 10.0, 1.0, 4.0, 5.0, 0.0, -80.0]),
        ("Digidelay 1-8 dot.", &[0.0, 1.0, 1.0, 238.7, 238.7, 1.0, 11.0, 11.0, 0.5, 0.0, 10.0, 1.0, 4.0, 5.0, 0.0, -80.0]),
    ];
}

/// Time Stretch: the sound made longer or shorter (50 to 500 %) without
/// a change of pitch, its windows sized for the material; it changes the
/// sound's length, so it works through Non-Real-Time Event FX.
pub mod time_stretch {
    use super::*;

    pub const MODE: usize = 0;
    pub const LENGTH: usize = 1;

    /// The modes' names, in order.
    pub const MODES: &[&str] = &[
        "A01. Music 1 (minimum flange, may echo)",
        "A02. Music 2",
        "A03. Music 3 (less echo)",
        "A04. Music 4 (fast, good for bass)",
        "A05. Music 5",
        "A06. Music 6",
        "A07. Speech 1",
        "A08. Speech 2",
        "A09. Speech 3 (fast)",
        "A10. Solo instruments 1",
        "A11. Solo instruments 2",
        "A12. Solo instruments 3",
        "A13. Solo instruments 4 (less echo)",
        "A14. Solo instruments 5",
        "A15. Solo instruments 6",
        "A16. Solo instruments 7 (fast)",
        "A17. Drums, unpitched (minimum echo)",
        "A18. Drums (better for toms)",
        "A19. Drums (tiny echo)",
    ];

    /// Each mode's window, in milliseconds.
    pub const WINDOWS: [f64; 19] = [
        300.0, 275.0, 150.0, 220.0, 190.0, 150.0, 90.0, 78.0, 80.0, 150.0, 130.0, 110.0, 75.0,
        90.0, 85.0, 95.0, 50.0, 70.0, 50.0,
    ];

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::choice("Mode", 0.0, MODES, 0.0),
        ParamSpec::num("Final length", 50.0, 500.0, 100.0)
            .unit("%")
            .decimals(1)
            .log(),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, 100.0]),
        ("Time expand 110 % of original", &[0.0, 110.0]),
        ("Time compress 85 % of original", &[0.0, 85.0]),
    ];
}

/// Express Time Stretch: the same, in the Express set.
pub mod express_time_stretch {
    use super::*;

    pub const PARAMS: &[ParamSpec] = time_stretch::PARAMS;
    pub const PRESETS: &[Preset] = time_stretch::PRESETS;
}

/// Pitch Shift: the sound raised or lowered by semitones and cents —
/// faster or slower like a tape, or keeping its length (then stretched
/// back as Time Stretch does).
pub mod pitch_shift {
    use super::*;

    pub const SEMITONES: usize = 0;
    pub const CENTS: usize = 1;
    pub const ACCURACY: usize = 2;
    pub const ANTI_ALIAS: usize = 3;
    pub const PRESERVE: usize = 4;
    pub const MODE: usize = 5;

    const KEPT: &[(u8, u32)] = &[(PRESERVE as u8, ON)];

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Semitones", -50.0, 50.0, 0.0).decimals(0),
        ParamSpec::num("Cents", -100.0, 100.0, 0.0).decimals(1),
        ParamSpec::num("Accuracy", 1.0, 3.0, 2.0).decimals(0),
        ParamSpec::switch("Apply an anti-alias filter during pitch shift", 0.0),
        ParamSpec::switch("Preserve duration", 0.0),
        ParamSpec::choice("Mode", 0.0, time_stretch::MODES, 0.0).when(KEPT),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, 0.0, 2.0, 0.0, 0.0, 0.0]),
        ("Fifth down", &[-7.0, 0.0, 2.0, 0.0, 0.0, 0.0]),
        ("Fifth up", &[7.0, 0.0, 2.0, 0.0, 0.0, 0.0]),
        ("Third down", &[-4.0, 0.0, 2.0, 0.0, 0.0, 0.0]),
        ("Third up", &[4.0, 0.0, 2.0, 0.0, 0.0, 0.0]),
    ];
}

/// Gapper/Snipper: silences inserted into the sound (the gapper) or
/// stretches of it cut out (the snipper), so many times a second, their
/// edges faded.
pub mod gapper_snipper {
    use super::*;

    pub const EFFECT: usize = 0;
    pub const FREQUENCY: usize = 1;
    pub const LENGTH: usize = 2;
    pub const FADE: usize = 3;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::choice("Effect", 0.0, &["Gapper (insert)", "Snipper (cut)"], 0.0),
        ParamSpec::num("Frequency of events", 0.0001, 1000.0, 20.0)
            .unit("Hz")
            .decimals(4)
            .log(),
        ParamSpec::num("Length of one event", 0.0001, 1.0, 0.075)
            .unit("s")
            .decimals(4)
            .log(),
        ParamSpec::num("Fade edges of each event", 0.0, 50.0, 15.0)
            .unit("%")
            .decimals(0),
    ];

    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[0.0, 20.0, 0.075, 15.0]),
        ("Chopper 1", &[0.0, 20.0, 0.075, 15.0]),
        ("Chopper 2", &[0.0, 10.0, 0.05, 15.0]),
        ("Distort 1", &[0.0, 1000.0, 0.001, 15.0]),
        ("Distort 2", &[0.0, 90.0, 0.01, 15.0]),
        ("Fast & distorted 1", &[1.0, 200.0, 0.005, 15.0]),
        ("Fast & distorted 2", &[1.0, 1000.0, 0.001, 30.0]),
        ("Very fast clipper", &[1.0, 350.0, 0.01, 5.0]),
    ];
}

/// Vibrato: the sound read faster and slower, like a tape, following a
/// wave drawn on a graph (up to the semitones each way); its length
/// changes with it.
pub mod vibrato {
    use super::*;

    pub const SEMITONES: usize = 0;
    pub const FREQUENCY: usize = 1;
    pub const OUTPUT: usize = 2;
    /// The wave's point count, its points after it.
    pub const GRAPH: usize = 3;
    pub const BLEND: usize = GRAPH + 1 + WAVE.params();

    /// A round (0 to 1000), the speed from −100 (the semitones down) to
    /// 100 (up).
    pub const WAVE: Graph = Graph {
        points: 32,
        x: (0.0, 1000.0),
        y: (-100.0, 100.0),
        log_x: false,
        pinned_ends: true,
        x_unit: "",
        y_unit: "",
    };

    const HEAD: [ParamSpec; 4] = [
        ParamSpec::num("Semitones", 0.01, 24.0, 12.0).decimals(2),
        ParamSpec::num("Modulation freq.", 0.001, 1000.0, 1.0)
            .unit("Hz")
            .decimals(3)
            .log(),
        ParamSpec::num("Output gain", -80.0, 24.0, 0.0)
            .show(Show::Db)
            .decimals(1),
        ParamSpec::graph("Graph", WAVE, 2.0),
    ];
    const POINTS: [ParamSpec; 64] = WAVE.point_params(&[(0.0, 0.0), (1000.0, 0.0)]);
    const TAIL: [ParamSpec; 1] = [ParamSpec::switch("Blend graph edges", 0.0)];
    const FRONT: [ParamSpec; 68] = join(HEAD, POINTS);
    const ALL: [ParamSpec; 69] = join(FRONT, TAIL);

    pub const PARAMS: &[ParamSpec] = &ALL;

    #[rustfmt::skip]
    pub const PRESETS: &[Preset] = &[
        ("Default all parameters", &[12.0, 1.0, 0.0, 2.0, 0.0, 0.0, 1000.0, 0.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0]),
        ("Pitch shift octave up", &[12.0, 1.0, 0.0, 2.0, 0.0, 100.0, 1000.0, 100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0]),
        ("Pitch shift octave down", &[12.0, 1.0, 0.0, 2.0, 0.0, -100.0, 1000.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0]),
        ("Vibrato 1", &[12.0, 5.0, 0.0, 4.0, 0.0, 0.0, 250.0, 100.0, 750.0, -100.0, 1000.0, 0.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0]),
        ("Vibrato 2", &[12.0, 9.0, 0.0, 8.0, 0.0, 0.0, 200.0, 90.0, 250.0, 100.0, 300.0, 90.0, 700.0, -90.0, 750.0, -100.0, 800.0, -90.0, 1000.0, 0.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0]),
        ("Vibrato 3", &[12.0, 9.0, 0.0, 6.0, 0.0, 0.0, 50.0, 100.0, 450.0, 100.0, 550.0, -100.0, 950.0, -100.0, 1000.0, 0.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0]),
        ("Weird 1", &[12.0, 9.0, 0.0, 13.0, 0.0, 0.0, 50.0, 90.0, 100.0, 100.0, 150.0, 90.0, 350.0, -70.0, 400.0, -80.0, 450.0, -70.0, 600.0, 20.0, 650.0, 30.0, 700.0, 40.0, 800.0, 70.0, 850.0, 60.0, 1000.0, 0.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0]),
        ("Triangle", &[6.0, 9.0, 0.0, 2.0, 0.0, -100.0, 500.0, 100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0, -100.0, 0.0]),
    ];
}

/// Express Audio Restoration: clicks taken out and the steady hiss of an
/// old recording turned down.
pub mod express_audio_restoration {
    use super::*;

    pub const CLICKS: usize = 0;
    pub const REDUCTION: usize = 1;
    pub const ABOVE: usize = 2;
    pub const ATTACK: usize = 3;
    pub const RELEASE: usize = 4;
    pub const FLOOR: usize = 5;

    pub const PARAMS: &[ParamSpec] = &[
        ParamSpec::num("Click removal amount", 1.0, 20.0, 10.0).decimals(0),
        ParamSpec::num("Reduce noise by", 0.0, 100.0, 12.0)
            .unit("dB")
            .decimals(1),
        ParamSpec::num("Affect frequencies above", 100.0, 10000.0, 100.0)
            .unit("Hz")
            .decimals(0)
            .log(),
        ParamSpec::num("Attack speed", 1.0, 20.0, 10.0).decimals(0),
        ParamSpec::num("Release speed", 1.0, 20.0, 10.0).decimals(0),
        ParamSpec::num("Noise floor", -96.0, -40.0, -60.0)
            .show(Show::Db)
            .decimals(1),
    ];

    pub const PRESETS: &[Preset] = &[];
}

#[cfg(test)]
mod tests {
    use crate::EffectKind;
    use crate::fx::ParamKind;

    #[test]
    fn every_preset_holds_a_value_in_range_for_every_parameter() {
        for k in EffectKind::AUDIO {
            let Some(list) = super::presets(k) else {
                continue;
            };
            let specs = k.params();
            for (name, values) in list {
                assert_eq!(values.len(), specs.len(), "{k:?} {name}");
                for (v, s) in values.iter().zip(specs) {
                    assert!(
                        (s.min..=s.max).contains(v),
                        "{k:?} {name}: {} = {v}",
                        s.name
                    );
                    if s.kind == ParamKind::Choice || s.kind == ParamKind::Toggle {
                        assert_eq!(v.fract(), 0.0, "{k:?} {name}: {}", s.name);
                    }
                }
            }
        }
    }

    #[test]
    fn defaults_sit_in_range_and_choices_name_every_value() {
        for k in EffectKind::AUDIO {
            for s in k.params() {
                assert!((s.min..=s.max).contains(&s.default), "{k:?} {}", s.name);
                if s.kind == ParamKind::Choice {
                    assert_eq!(
                        s.options.len() as f32,
                        s.max - s.min + 1.0,
                        "{k:?} {}",
                        s.name
                    );
                }
            }
        }
    }
}
