//! The video bus (View › Video Bus Track), as VEGAS's: the main video
//! output's track — its mute, its Bypass FX and Envelopes, and its
//! envelopes: Fade to Color (over everything the tracks make), Motion Blur
//! Amount and Video Supersampling (the frames a frame averages, weighed as
//! File › Properties › Video › Motion blur type says) and Mute.

use bv_core::{TICKS_PER_SECOND, Tick};
use serde::{Deserialize, Serialize};

use crate::edit::EditError;
use crate::envelope::{Envelope, EnvelopeKind, EnvelopeOwner};
use crate::fx::KeyCurve;
use crate::model::{AutomationMode, Project, TrackAutomation};

/// File › Properties › Video › Motion blur type: the curve that weighs the
/// frames a motion blur averages. The asymmetric ones take only the
/// curve's half before the frame: a hard leading edge, the blur trailing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MotionBlurType {
    /// The middle frame weighs most, the outer ones less, along a bell.
    #[default]
    Gaussian,
    /// The middle frame weighs most, the outer ones least, along a line.
    Pyramid,
    /// Every frame alike: their average.
    Box,
    GaussianAsymmetric,
    PyramidAsymmetric,
    BoxAsymmetric,
}

impl MotionBlurType {
    pub const ALL: [MotionBlurType; 6] = [
        MotionBlurType::Gaussian,
        MotionBlurType::Pyramid,
        MotionBlurType::Box,
        MotionBlurType::GaussianAsymmetric,
        MotionBlurType::PyramidAsymmetric,
        MotionBlurType::BoxAsymmetric,
    ];

    /// Only the frames before the middle one.
    pub fn asymmetric(self) -> bool {
        matches!(
            self,
            MotionBlurType::GaussianAsymmetric
                | MotionBlurType::PyramidAsymmetric
                | MotionBlurType::BoxAsymmetric
        )
    }

    /// The weight of a frame `x` of the way (−1 to 1) from the middle one
    /// to an end.
    fn weight(self, x: f64) -> f64 {
        let x = x.abs().min(1.0);
        match self {
            MotionBlurType::Gaussian | MotionBlurType::GaussianAsymmetric => {
                // Half the half-width a standard deviation.
                (-0.5 * (x / 0.5).powi(2)).exp()
            }
            // The outer frames still weigh a little.
            MotionBlurType::Pyramid | MotionBlurType::PyramidAsymmetric => 1.0 - 0.95 * x,
            MotionBlurType::Box | MotionBlurType::BoxAsymmetric => 1.0,
        }
    }
}

/// The video bus: the main video output's track.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VideoBus {
    /// Its Fade to Color, Motion Blur Amount, Video Supersampling and Mute
    /// envelopes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub envelopes: Vec<Envelope>,
    /// Its Mute: no video comes out.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub muted: bool,
    /// Bypass FX and Envelopes: the Video Output FX and these envelopes
    /// do nothing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
    /// The colors its Fade to Color goes to: top (up) and bottom (down);
    /// `None`: the default ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fade_colors: Option<[[u8; 4]; 2]>,
    /// Automation Settings: its Motion Blur and Supersampling sliders and
    /// its Mute are automation controls, and how its envelopes play.
    #[serde(default, skip_serializing_if = "is_default")]
    pub automation: TrackAutomation,
    /// The Motion Blur and Supersampling sliders' own values (seconds; how
    /// many frames a frame): the bus's where it has no such envelope, or
    /// with automation off.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub motion_blur: f32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub supersampling: f32,
}

fn is_default(a: &TrackAutomation) -> bool {
    *a == TrackAutomation::default()
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

impl VideoBus {
    pub fn is_default(&self) -> bool {
        *self == VideoBus::default()
    }

    /// Its `kind` envelope's value at `t`, when it has one and is not
    /// bypassed.
    pub fn envelope_at(&self, kind: EnvelopeKind, t: Tick) -> Option<f32> {
        if self.bypass {
            return None;
        }
        self.envelopes
            .iter()
            .find(|e| e.kind == kind)
            .map(|e| e.value_at(t))
    }

    /// Whether its envelopes play: not with automation off on its
    /// automation controls.
    fn envelopes_play(&self) -> bool {
        !(self.automation.controls && self.automation.mode == AutomationMode::Off)
    }

    /// Its `kind` control at `t` (Motion Blur Amount, Video Supersampling):
    /// the envelope where it plays, else the slider's own value.
    pub fn control_at(&self, kind: EnvelopeKind, t: Tick) -> f32 {
        let own = match kind {
            EnvelopeKind::MotionBlur => self.motion_blur,
            EnvelopeKind::Supersampling => self.supersampling,
            _ => 0.0,
        };
        if self.bypass {
            return 0.0;
        }
        let envelope = self
            .envelopes_play()
            .then(|| self.envelope_at(kind, t))
            .flatten();
        envelope.unwrap_or(own)
    }

    /// Whether no video comes out at `t`: its Mute, or its mute envelope
    /// where it plays.
    pub fn muted_at(&self, t: Tick) -> bool {
        self.muted
            || (self.envelopes_play()
                && self
                    .envelope_at(EnvelopeKind::Mute, t)
                    .is_some_and(|v| v < 0.5))
    }
}

/// The most frames a motion blur averages.
const MAX_SAMPLES: usize = 64;

impl Project {
    /// What the video bus's Motion Blur Amount and Video Supersampling ask
    /// of the frame at `t`: the times it averages and their weights
    /// (summing to 1), or `None` for the frame alone. The amount is a span
    /// (seconds) with the frame in its middle — half of it before and half
    /// after, or only before for the asymmetric types — sampled at the
    /// project's frame rate times the supersampling; supersampling alone
    /// averages that many frames over the frame's own span.
    pub fn motion_blur_at(&self, t: Tick) -> Option<Vec<(Tick, f32)>> {
        let bus = &self.video_bus;
        let amount = bus.control_at(EnvelopeKind::MotionBlur, t).max(0.0) as f64;
        let sub = bus
            .control_at(EnvelopeKind::Supersampling, t)
            .round()
            .max(1.0) as f64;
        let frame = 1.0 / self.settings.frame_rate.as_f64().max(1.0);
        let step = frame / sub;
        let tick = |s: f64| Tick((s * TICKS_PER_SECOND as f64).round() as i64);
        let mut samples: Vec<(f64, f64)> = Vec::new();
        if amount > 1e-6 {
            let kind = self.settings.motion_blur;
            let half = amount / 2.0;
            let k = ((half / step + 1e-9).floor() as i64).min((MAX_SAMPLES / 2) as i64);
            if k < 1 {
                return None;
            }
            let last = if kind.asymmetric() { 0 } else { k };
            for i in -k..=last {
                let x = i as f64 * step;
                samples.push((x, kind.weight(x / half)));
            }
        } else if sub >= 2.0 {
            let n = (sub as usize).min(MAX_SAMPLES);
            for i in 0..n {
                let x = (i as f64 - (n - 1) as f64 / 2.0) * step;
                samples.push((x, 1.0));
            }
        } else {
            return None;
        }
        let total: f64 = samples.iter().map(|s| s.1).sum();
        Some(
            samples
                .into_iter()
                .map(|(x, w)| ((t + tick(x)).max(Tick::ZERO), (w / total) as f32))
                .collect(),
        )
    }

    /// Adds the video bus's `kind` envelope (Fade to Color, Motion Blur
    /// Amount, Video Supersampling, Mute), or removes it when present.
    /// Returns whether it has it now.
    pub fn toggle_video_bus_envelope(
        &mut self,
        kind: EnvelopeKind,
        curve: KeyCurve,
    ) -> Result<bool, EditError> {
        if !EnvelopeKind::VIDEO_BUS.contains(&kind) {
            return Err(EditError::WrongTrackKind);
        }
        let envelopes = &mut self.video_bus.envelopes;
        if let Some(i) = envelopes.iter().position(|e| e.kind == kind) {
            envelopes.remove(i);
            Ok(false)
        } else {
            envelopes.push(Envelope::with_curve(kind, curve));
            Ok(true)
        }
    }

    /// Whose envelopes the video bus's are.
    pub fn video_bus_owner(&self) -> EnvelopeOwner {
        EnvelopeOwner::VideoBus
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProjectSettings;

    /// Its sliders' own values play where it has no envelope, the
    /// envelopes elsewhere — but with automation off on its automation
    /// controls, where the sliders hold; bypassed, nothing.
    #[test]
    fn the_sliders_or_the_envelopes() {
        let mut p = Project::new(ProjectSettings::default());
        let t = Tick::from_seconds(1);
        p.video_bus.motion_blur = 0.2;
        p.video_bus.supersampling = 3.0;
        let bus = |p: &Project| {
            (
                p.video_bus.control_at(EnvelopeKind::MotionBlur, t),
                p.video_bus.control_at(EnvelopeKind::Supersampling, t),
            )
        };
        assert_eq!(bus(&p), (0.2, 3.0));
        p.toggle_video_bus_envelope(EnvelopeKind::MotionBlur, KeyCurve::Linear)
            .unwrap();
        p.set_envelope_point(
            EnvelopeOwner::VideoBus,
            EnvelopeKind::MotionBlur,
            Tick::ZERO,
            0.5,
        )
        .unwrap();
        p.toggle_video_bus_envelope(EnvelopeKind::Mute, KeyCurve::Hold)
            .unwrap();
        p.set_envelope_point(EnvelopeOwner::VideoBus, EnvelopeKind::Mute, Tick::ZERO, 0.0)
            .unwrap();
        assert_eq!(bus(&p), (0.5, 3.0));
        assert!(p.video_bus.muted_at(t));
        p.video_bus.automation.controls = true;
        p.video_bus.automation.mode = AutomationMode::Off;
        assert_eq!(bus(&p), (0.2, 3.0));
        assert!(!p.video_bus.muted_at(t));
        p.video_bus.automation.mode = AutomationMode::Read;
        assert_eq!(bus(&p), (0.5, 3.0));
        p.video_bus.bypass = true;
        assert_eq!(bus(&p), (0.0, 0.0));
        assert!(p.motion_blur_at(t).is_none());
    }

    /// A motion blur averages the frames within its span — half before,
    /// half after, or only before — at the frame rate (times the
    /// supersampling), weighed by the type; supersampling alone spreads
    /// over the frame's span; bypassed, nothing.
    #[test]
    fn motion_blur_samples_the_frames_around() {
        let mut p = Project::new(ProjectSettings::default());
        let s = |x: f64| Tick::from_seconds_f64(x);
        assert!(p.motion_blur_at(s(1.0)).is_none());
        p.toggle_video_bus_envelope(EnvelopeKind::MotionBlur, KeyCurve::Linear)
            .unwrap();
        // 0.21 s at 29.97 frames a second: three frames each way.
        p.set_envelope_point(
            EnvelopeOwner::VideoBus,
            EnvelopeKind::MotionBlur,
            Tick::ZERO,
            0.21,
        )
        .unwrap();
        p.settings.motion_blur = MotionBlurType::Box;
        let rate = p.settings.frame_rate.as_f64();
        let blur = p.motion_blur_at(s(1.0)).unwrap();
        assert_eq!(blur.len(), 7);
        assert!(blur.iter().all(|(_, w)| (w - 1.0 / 7.0).abs() < 1e-6));
        assert!((blur[0].0.as_seconds_f64() - (1.0 - 3.0 / rate)).abs() < 1e-6);
        // Asymmetric: only the frames before.
        p.settings.motion_blur = MotionBlurType::BoxAsymmetric;
        let trailing = p.motion_blur_at(s(1.0)).unwrap();
        assert_eq!(trailing.len(), 4);
        assert!(trailing.iter().all(|(at, _)| *at <= s(1.0)));
        // Gaussian and Pyramid: the middle frame weighs most.
        for kind in [MotionBlurType::Gaussian, MotionBlurType::Pyramid] {
            p.settings.motion_blur = kind;
            let b = p.motion_blur_at(s(1.0)).unwrap();
            let total: f32 = b.iter().map(|x| x.1).sum();
            assert!((total - 1.0).abs() < 1e-5);
            assert!(b[3].1 > b[2].1 && b[2].1 > b[0].1, "{kind:?}: {b:?}");
        }
        // Supersampling 2: twice the frames.
        p.toggle_video_bus_envelope(EnvelopeKind::Supersampling, KeyCurve::Linear)
            .unwrap();
        p.set_envelope_point(
            EnvelopeOwner::VideoBus,
            EnvelopeKind::Supersampling,
            Tick::ZERO,
            2.0,
        )
        .unwrap();
        p.settings.motion_blur = MotionBlurType::Box;
        assert_eq!(p.motion_blur_at(s(1.0)).unwrap().len(), 13);
        // Supersampling alone: two frames over the frame's span.
        p.toggle_video_bus_envelope(EnvelopeKind::MotionBlur, KeyCurve::Linear)
            .unwrap();
        let alone = p.motion_blur_at(s(1.0)).unwrap();
        assert_eq!(alone.len(), 2);
        // Bypassed: the frame alone.
        p.video_bus.bypass = true;
        assert!(p.motion_blur_at(s(1.0)).is_none());
    }
}
