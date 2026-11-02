//! Track Motion (2D), as VEGAS's: where a video track's picture goes in
//! the frame — a box's center, size and angle, keyframed on the
//! timeline —, with a 2D shadow and a 2D glow under it; a compositing
//! parent's Parent Motion moves its whole group the same way.

use bv_core::Tick;
use serde::{Deserialize, Serialize};

use crate::edit::EditError;
use crate::fx::{KeyCurve, Transform, TransformKey};
use crate::model::{Project, TrackId, TrackKind};
use crate::space::{Box3d, Box3dKey};

/// How far a 2D shadow's or glow's Blur of 100 % spreads it: a part of the
/// frame's width (its radius, about two standard deviations).
pub const HALO_BLUR: f32 = 0.05;

/// A 2D shadow's or glow's look: the picture's shape moved, scaled,
/// blurred and colored.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HaloLook {
    /// Its offset from the picture, in parts of the frame (x to the right,
    /// y up).
    pub x: f32,
    pub y: f32,
    /// Its size against the picture's (1: the same).
    pub size: f32,
    /// How soft its edge is (Blur %), 0 (hard) to 1.
    pub blur: f32,
    /// How opaque it is (Intensity), 0–1.
    pub intensity: f32,
    /// Its color (video's values, straight).
    pub color: [f32; 4],
}

impl HaloLook {
    /// A new 2D shadow: black, a little down and to the right.
    pub const SHADOW: HaloLook = HaloLook {
        x: 0.012,
        y: -0.02,
        size: 1.0,
        blur: 0.3,
        intensity: 0.7,
        color: [0.0, 0.0, 0.0, 1.0],
    };
    /// A new 2D glow: white, all around.
    pub const GLOW: HaloLook = HaloLook {
        x: 0.0,
        y: 0.0,
        size: 1.0,
        blur: 0.4,
        intensity: 0.8,
        color: [1.0, 1.0, 1.0, 1.0],
    };

    fn lerp(&self, b: &HaloLook, f: f32) -> HaloLook {
        let m = |a: f32, b: f32| a + (b - a) * f;
        HaloLook {
            x: m(self.x, b.x),
            y: m(self.y, b.y),
            size: m(self.size, b.size),
            blur: m(self.blur, b.blur),
            intensity: m(self.intensity, b.intensity),
            color: std::array::from_fn(|i| m(self.color[i], b.color[i])),
        }
    }
}

/// A keyframe of a 2D shadow or glow, on the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HaloKey {
    pub at: Tick,
    pub value: HaloLook,
    #[serde(default)]
    pub curve: KeyCurve,
}

/// A track's 2D shadow or 2D glow (Track Motion's rows of those names):
/// drawn only while its box is checked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Halo {
    pub on: bool,
    /// Its look when it is not animated.
    pub look: HaloLook,
    /// Its keyframes (empty: `look` all along).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<HaloKey>,
}

impl Halo {
    fn new(look: HaloLook) -> Halo {
        Halo {
            on: false,
            look,
            keys: Vec::new(),
        }
    }

    /// Its look at `t`.
    pub fn at(&self, t: Tick) -> HaloLook {
        let Some(first) = self.keys.first() else {
            return self.look;
        };
        if t <= first.at {
            return first.value;
        }
        for w in self.keys.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            if t < b.at {
                let span = (b.at - a.at).0.max(1) as f64;
                let f = a.curve.eval(((t - a.at).0 as f64 / span) as f32);
                return a.value.lerp(&b.value, f);
            }
        }
        self.keys.last().map_or(self.look, |k| k.value)
    }

    /// Its look at `t` set to `value`: a keyframe there when it is
    /// animated (or `t` is past the timeline's start).
    pub fn set_at(&mut self, t: Tick, value: HaloLook, curve: KeyCurve) {
        if self.keys.is_empty() {
            if t == Tick::ZERO {
                self.look = value;
                return;
            }
            self.keys.push(HaloKey {
                at: Tick::ZERO,
                value: self.look,
                curve,
            });
        }
        match self.keys.iter_mut().find(|k| k.at == t) {
            Some(k) => k.value = value,
            None => {
                let before = self
                    .keys
                    .iter()
                    .rev()
                    .find(|k| k.at < t)
                    .map_or(curve, |k| k.curve);
                self.keys.push(HaloKey {
                    at: t,
                    value,
                    curve: before,
                });
                self.keys.sort_by_key(|k| k.at);
            }
        }
        self.look = self.keys[0].value;
    }

    /// The keyframe at `t` removed (the last one left becomes the look).
    pub fn remove_key(&mut self, t: Tick) {
        self.keys.retain(|k| k.at != t);
        if self.keys.len() == 1 {
            self.look = self.keys[0].value;
            self.keys.clear();
        }
    }
}

/// A track's motion (or a compositing parent's Parent Motion).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackMotion {
    /// The box when it is not animated, as a framing of the frame-sized
    /// picture: `x`, `y` its center's offset from the frame's in parts of
    /// the frame (y up), `scale` its width's part of the frame's,
    /// `stretch` its height's part over that, `rotation_deg` its angle.
    pub transform: Transform,
    /// Its keyframes, on the timeline (empty: `transform` all along).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<TransformKey>,
    pub shadow: Halo,
    pub glow: Halo,
    /// 3D Track Motion (with the 3D Source Alpha compositing mode): the
    /// box in space when it is not animated, and its keyframes.
    #[serde(default, skip_serializing_if = "Box3d::is_default")]
    pub space: Box3d,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub space_keys: Vec<Box3dKey>,
}

impl Default for TrackMotion {
    fn default() -> TrackMotion {
        TrackMotion {
            transform: Transform::default(),
            keys: Vec::new(),
            shadow: Halo::new(HaloLook::SHADOW),
            glow: Halo::new(HaloLook::GLOW),
            space: Box3d::default(),
            space_keys: Vec::new(),
        }
    }
}

impl TrackMotion {
    /// The box in space at `t`.
    pub fn space_at(&self, t: Tick) -> Box3d {
        crate::space::box_at(&self.space_keys, &self.space, t)
    }

    /// The box in space at `t` set to `value`: a keyframe there when it is
    /// animated (or `t` is past the timeline's start).
    pub fn set_space_at(&mut self, t: Tick, value: Box3d, curve: KeyCurve) {
        crate::space::set_box_at(&mut self.space_keys, &mut self.space, t, value, curve);
    }

    /// The box's keyframe at `t` removed.
    pub fn remove_space_key(&mut self, t: Tick) {
        crate::space::remove_box_key(&mut self.space_keys, &mut self.space, t);
    }
}

/// What a track's motion does at a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionAt {
    pub transform: Transform,
    /// Its 2D shadow and glow, when they are on (their colors in the
    /// compositing space, premultiplied by nothing: straight).
    pub shadow: Option<HaloLook>,
    pub glow: Option<HaloLook>,
    pub quality: crate::model::RenderQuality,
}

impl TrackMotion {
    /// The box at `t`.
    pub fn transform_at(&self, t: Tick) -> Transform {
        crate::fx::transform_at(&self.keys, &self.transform, t)
    }

    /// Whether it changes nothing: the picture where it was, no shadow,
    /// no glow.
    pub fn is_identity(&self) -> bool {
        self.keys.is_empty() && self.transform.is_identity() && !self.shadow.on && !self.glow.on
    }

    /// What it does at `t` (`None`: nothing).
    pub fn at(&self, t: Tick, quality: crate::model::RenderQuality) -> Option<MotionAt> {
        if self.is_identity() {
            return None;
        }
        Some(MotionAt {
            transform: self.transform_at(t),
            shadow: self.shadow.on.then(|| self.shadow.at(t)),
            glow: self.glow.on.then(|| self.glow.at(t)),
            quality,
        })
    }

    /// The box at `t` set to `value`: a keyframe there when it is animated
    /// (or `t` is past the timeline's start), a new one with `curve` and
    /// `smoothness`.
    pub fn set_at(&mut self, t: Tick, value: Transform, (curve, smoothness): (KeyCurve, f32)) {
        let key = |at: Tick, value: Transform, curve: KeyCurve| TransformKey {
            at,
            value,
            curve,
            smoothness,
        };
        if self.keys.is_empty() {
            if t == Tick::ZERO {
                self.transform = value;
                return;
            }
            self.keys.push(key(Tick::ZERO, self.transform, curve));
        }
        match self.keys.iter_mut().find(|k| k.at == t) {
            Some(k) => k.value = value,
            None => {
                let before = self
                    .keys
                    .iter()
                    .rev()
                    .find(|k| k.at < t)
                    .map_or(curve, |k| k.curve);
                self.keys.push(key(t, value, before));
                self.keys.sort_by_key(|k| k.at);
            }
        }
        self.transform = self.keys[0].value;
    }

    /// The box's keyframe at `t` removed (the last one left becomes the
    /// box).
    pub fn remove_key(&mut self, t: Tick) {
        self.keys.retain(|k| k.at != t);
        if self.keys.len() == 1 {
            self.transform = self.keys[0].value;
            self.keys.clear();
        }
    }
}

type Result<T> = std::result::Result<T, EditError>;

impl Project {
    /// The video track's Track Motion (`parent`: its Parent Motion), made
    /// when it has none.
    pub fn track_motion_mut(&mut self, track: TrackId, parent: bool) -> Result<&mut TrackMotion> {
        let t = self
            .track_by_id_mut(track)
            .ok_or(EditError::UnknownTrack(track))?;
        if t.kind != TrackKind::Video || t.adjustment {
            return Err(EditError::WrongTrackKind);
        }
        let m = if parent {
            &mut t.parent_motion
        } else {
            &mut t.motion
        };
        Ok(m.get_or_insert_with(TrackMotion::default))
    }

    /// The track's motion (`parent`: its Parent Motion), if it has one.
    pub fn track_motion(&self, track: TrackId, parent: bool) -> Option<&TrackMotion> {
        let t = self.track(track)?;
        if parent {
            t.parent_motion.as_ref()
        } else {
            t.motion.as_ref()
        }
    }

    /// The track's box at `t` (on the frame grid) set to `value`.
    pub fn set_track_motion_at(
        &mut self,
        track: TrackId,
        parent: bool,
        t: Tick,
        value: Transform,
        new: (KeyCurve, f32),
    ) -> Result<()> {
        let t = self.settings.frame_rate.snap_nearest(t).max(Tick::ZERO);
        self.track_motion_mut(track, parent)?.set_at(t, value, new);
        self.tidy_motion(track, parent);
        Ok(())
    }

    /// The track's 2D shadow (`shadow`) or glow at `t` set to `value`.
    pub fn set_track_halo_at(
        &mut self,
        track: TrackId,
        parent: bool,
        shadow: bool,
        t: Tick,
        value: HaloLook,
    ) -> Result<()> {
        let t = self.settings.frame_rate.snap_nearest(t).max(Tick::ZERO);
        let m = self.track_motion_mut(track, parent)?;
        let h = if shadow { &mut m.shadow } else { &mut m.glow };
        h.set_at(t, value, KeyCurve::Linear);
        Ok(())
    }

    /// The track's 2D shadow (`shadow`) or glow on or off.
    pub fn set_track_halo_on(
        &mut self,
        track: TrackId,
        parent: bool,
        shadow: bool,
        on: bool,
    ) -> Result<()> {
        let m = self.track_motion_mut(track, parent)?;
        let h = if shadow { &mut m.shadow } else { &mut m.glow };
        h.on = on;
        self.tidy_motion(track, parent);
        Ok(())
    }

    /// Back to no motion (Track Motion's Restore of everything).
    pub fn clear_track_motion(&mut self, track: TrackId, parent: bool) -> Result<()> {
        let t = self
            .track_by_id_mut(track)
            .ok_or(EditError::UnknownTrack(track))?;
        if parent {
            t.parent_motion = None;
        } else {
            t.motion = None;
        }
        Ok(())
    }

    /// A motion that does nothing and keeps nothing is dropped.
    fn tidy_motion(&mut self, track: TrackId, parent: bool) {
        if let Some(t) = self.track_by_id_mut(track) {
            let m = if parent {
                &mut t.parent_motion
            } else {
                &mut t.motion
            };
            if m.as_ref().is_some_and(|m| *m == TrackMotion::default()) {
                *m = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: i64) -> Tick {
        Tick::from_seconds(x)
    }

    #[test]
    fn the_box_and_its_halos_keyframe_on_the_timeline() {
        let mut p = Project::default();
        let v = p.tracks[0].id;
        let half = Transform {
            scale: 0.5,
            x: 0.25,
            ..Transform::default()
        };
        let new = (KeyCurve::Linear, 0.0);
        // At the start: the box itself, no keyframes.
        p.set_track_motion_at(v, false, Tick::ZERO, half, new)
            .unwrap();
        let m = p.track_motion(v, false).unwrap();
        assert!(m.keys.is_empty() && m.transform == half);
        assert_eq!(m.at(s(3), Default::default()).unwrap().transform, half);
        // Later: keyframes (on the frame grid: 2 s is frame 60 at 29.97),
        // the box moving between them.
        let left = Transform { x: -0.25, ..half };
        p.set_track_motion_at(v, false, s(2), left, new).unwrap();
        let m = p.track_motion(v, false).unwrap();
        assert_eq!(m.keys.len(), 2);
        assert!(m.transform_at(s(1)).x.abs() < 1e-3);
        // The shadow: off until checked, then its own keyframes.
        assert!(m.at(s(1), Default::default()).unwrap().shadow.is_none());
        p.set_track_halo_on(v, false, true, true).unwrap();
        let dark = HaloLook {
            intensity: 1.0,
            ..HaloLook::SHADOW
        };
        p.set_track_halo_at(v, false, true, s(4), dark).unwrap();
        let m = p.track_motion(v, false).unwrap();
        let at = m.at(s(2), Default::default()).unwrap();
        let k = at.shadow.unwrap().intensity;
        assert!((k - (0.7 + 0.3 * 0.5)).abs() < 1e-3, "{k}");
        assert!(at.glow.is_none());
        // Restored: nothing left.
        p.clear_track_motion(v, false).unwrap();
        assert!(p.track_motion(v, false).is_none());
        // Only video tracks move.
        let a = p.tracks[1].id;
        assert!(
            p.set_track_motion_at(a, false, Tick::ZERO, half, new)
                .is_err()
        );
        // A motion back to doing nothing is dropped.
        p.set_track_motion_at(v, true, Tick::ZERO, half, new)
            .unwrap();
        p.set_track_motion_at(v, true, Tick::ZERO, Transform::default(), new)
            .unwrap();
        assert!(p.track_motion(v, true).is_none());
    }
}
