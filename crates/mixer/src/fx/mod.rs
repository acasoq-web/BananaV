//! BananaV's audio effects that follow VEGAS's (their parameters and
//! presets are `bv_timeline::audio_fx`'s): each one a state that takes its
//! values while it plays and processes blocks in place.
//!
//! Every block's loops run over plain slices with no branch inside, so
//! that the compiler turns them into vector instructions (SSE2 or AVX on
//! x86-64, NEON on ARM): the effects' "acceleration". Values that change
//! between blocks (FX automation, a slider dragged) ramp over the next
//! block instead of jumping, so they never click.

use bv_timeline::{Effect, EffectKind, Tempo, audio_fx};

use express::Mixed;

mod delays;
mod dither;
mod dynamics;
mod eq;
mod express;
mod filters;
mod fir;
mod graphs;
mod hammer;
mod modulation;
pub(crate) mod offline;
mod restoration;
mod reverb;
mod studio;
mod studio2;
mod utility;

/// An audio effect's state, ready to process.
pub(crate) trait Own: Send + std::fmt::Debug {
    /// Takes new values while it plays, keeping what it holds (filters'
    /// memory, delay lines…).
    fn set(&mut self, params: &[f32]);
    /// Processes one block in place: a buffer a channel, all as long.
    fn process(&mut self, channels: &mut [Vec<f32>]);
    /// How many samples late its output comes.
    fn latency(&self) -> usize {
        0
    }
    /// The project's tempo, for those that keep time with it.
    fn keep_time(&mut self, _tempo: &Tempo) {}
}

/// The state of `e` when it is one of these effects.
pub(crate) fn make(e: &Effect, rate: u32, channels: usize) -> Option<Box<dyn Own>> {
    let mut own: Box<dyn Own> = match e.kind {
        EffectKind::ModernEq => Box::new(crate::modern_eq::Eq::new(rate, channels)),
        EffectKind::Volume => Box::new(utility::Volume::default()),
        EffectKind::Pan => Box::new(utility::Pan::default()),
        EffectKind::SmoothEnhance => Box::new(filters::SmoothEnhance::default()),
        EffectKind::NoiseGateFx | EffectKind::TrackNoiseGate => Box::new(dynamics::Gate::new(rate)),
        EffectKind::SimpleDelay => Box::new(delays::SimpleDelay::new(rate)),
        EffectKind::Dither => Box::new(dither::Dither::default()),
        EffectKind::ResonantFilter => Box::new(filters::ResonantFilter::new(rate)),
        EffectKind::ParametricEq => Box::new(fir::ParametricEq::new(rate)),
        EffectKind::Chorus => Box::new(modulation::Chorus::new(rate)),
        EffectKind::FlangeWahWah => Box::new(modulation::FlangeWahWah::new(rate)),
        EffectKind::ReverbFx => Box::new(reverb::ReverbFx::new(rate)),
        EffectKind::TrackCompressor => Box::new(dynamics::TrackCompressor::new(rate)),
        EffectKind::TrackEq => Box::new(eq::TrackEq::new(rate)),
        EffectKind::ParagraphicEq => Box::new(eq::ParagraphicEq::new(rate)),
        EffectKind::MultiTapDelay => Box::new(delays::MultiTapDelay::new(rate)),
        EffectKind::MultiBandDynamics => Box::new(dynamics::MultiBand::new(rate)),
        EffectKind::ExpressDelay => Box::new(Mixed::new(
            express::ExpressDelay::new(rate),
            express::same,
            audio_fx::express_delay::MIX,
        )),
        EffectKind::ExpressDistortion => Box::new(Mixed::new(
            express::ExpressDistortion::default(),
            express::same,
            audio_fx::express_distortion::MIX,
        )),
        EffectKind::ExpressEqualization => Box::new(Mixed::new(
            express::ExpressEq::new(rate),
            express::same,
            audio_fx::express_equalization::MIX,
        )),
        EffectKind::ExpressNoiseGate => Box::new(dynamics::Gate::new(rate)),
        EffectKind::ExpressChorus => Box::new(Mixed::new(
            modulation::Chorus::new(rate),
            express::chorus_values,
            audio_fx::express_chorus::MIX,
        )),
        EffectKind::ExpressFlangeWahWah => Box::new(Mixed::new(
            modulation::FlangeWahWah::express(rate),
            express::flange_values,
            audio_fx::express_flange_wah_wah::MIX,
        )),
        EffectKind::ExpressDynamics => Box::new(express::Mapped::new(
            dynamics::TrackCompressor::new(rate),
            express::dynamics_values,
        )),
        EffectKind::ExpressStutter => Box::new(Mixed::new(
            express::ExpressStutter::new(rate),
            express::same,
            audio_fx::express_stutter::MIX,
        )),
        EffectKind::ExpressAmplitudeModulation => Box::new(Mixed::new(
            express::ExpressAm::new(rate),
            express::same,
            audio_fx::express_amplitude_modulation::MIX,
        )),
        EffectKind::ExpressGraphicEq => Box::new(fir::ExpressGraphicEq::new(rate)),
        EffectKind::AmplitudeModulation => Box::new(graphs::AmplitudeModulation::new(rate)),
        EffectKind::GraphicDynamics => Box::new(graphs::GraphicDynamics::new(rate)),
        EffectKind::Distortion => Box::new(graphs::Distortion::new(rate)),
        EffectKind::GraphicEq => Box::new(fir::GraphicEq::new(rate)),
        EffectKind::WaveHammer => Box::new(hammer::WaveHammer::new(rate)),
        EffectKind::StudioGate => Box::new(studio::Gate::new(rate)),
        EffectKind::DeEsser => Box::new(studio::DeEsser::new(rate)),
        EffectKind::StudioLimiter => Box::new(studio::Limiter::new(rate)),
        EffectKind::TremoloPan => Box::new(studio::TremoloPan::new(rate)),
        EffectKind::TubeStage => Box::new(studio::TubeStage::new(rate)),
        EffectKind::VocalStrip => Box::new(studio::VocalStrip::new(rate)),
        EffectKind::StudioCompressor => Box::new(studio2::Compressor::new(rate)),
        EffectKind::Phaser => Box::new(studio2::Phaser::new(rate)),
        EffectKind::ChorusFlanger => Box::new(studio2::ChorusFlanger::new(rate)),
        EffectKind::StudioReverb => Box::new(studio2::StudioReverb::new(rate)),
        EffectKind::StereoDelay => Box::new(studio2::StereoDelay::new(rate)),
        EffectKind::ExpressReverb => Box::new(Mixed::new(
            reverb::ReverbFx::new(rate),
            express::reverb_values,
            audio_fx::express_reverb::MIX,
        )),
        EffectKind::ExpressAudioRestoration => Box::new(restoration::Restoration::new(rate)),
        _ => return None,
    };
    own.set(&params_of(e));
    Some(own)
}

/// Every parameter of `e`, defaults for those it lacks.
pub(crate) fn params_of(e: &Effect) -> Vec<f32> {
    (0..e.kind.params().len()).map(|i| e.param(i)).collect()
}

/// A level in dB as a linear gain, `floor` and below silence.
pub(crate) fn db_gain(db: f32, floor: f32) -> f32 {
    if db <= floor {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

/// A value that moves to its new target over the next block: its value
/// at each sample of a block `n` long goes from where it was to `to`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ramp {
    now: f32,
    to: f32,
    fresh: bool,
}

impl Ramp {
    pub(crate) fn new(v: f32) -> Ramp {
        Ramp {
            now: v,
            to: v,
            fresh: true,
        }
    }

    /// The value it goes to; the first one it starts at.
    pub(crate) fn set(&mut self, v: f32) {
        self.to = v;
        if self.fresh {
            self.now = v;
            self.fresh = false;
        }
    }

    /// Its start and step for a block `n` long, then at its target.
    pub(crate) fn block(&mut self, n: usize) -> (f32, f32) {
        let start = self.now;
        let step = if n > 0 {
            (self.to - start) / n as f32
        } else {
            0.0
        };
        self.now = self.to;
        (start, step)
    }
}

/// `buf` times a gain going from `start` by `step` a sample.
pub(crate) fn gain_ramp(buf: &mut [f32], start: f32, step: f32) {
    if step == 0.0 {
        for s in buf.iter_mut() {
            *s *= start;
        }
        return;
    }
    for (i, s) in buf.iter_mut().enumerate() {
        *s *= start + step * i as f32;
    }
}
