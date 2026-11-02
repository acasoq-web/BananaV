//! An event's motion tracks (VEGAS 2026's Motion Tracking, as its Help
//! describes it): regions of the event's picture followed through time —
//! four corners at each tracked frame, 0–1 of the frame from its top left,
//! at times from the event's start — kept with the event (they move, trim
//! and split with it, `edit::rebase_envelopes`), and passed on to effects'
//! location parameters as keyframes.

use bv_core::Tick;
use serde::{Deserialize, Serialize};

use crate::fx::{KeyCurve, PluginKey, PluginParam, PluginValue};

/// A region's four corners, in order round it: 0–1 of the frame, from its
/// top left.
pub type Quad = [[f32; 2]; 4];

/// A region tracked at a frame of the event.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackKey {
    /// From the event's start.
    pub at: Tick,
    pub quad: Quad,
}

/// One of an event's regions (VEGAS's masks of the Motion Tracking window:
/// "Region 1"…).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MotionTrack {
    pub name: String,
    /// Track outside Region: what moves around it is followed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub outside: bool,
    /// The region where no frame is tracked: as it was drawn.
    pub region: Quad,
    /// The frames tracked, in time order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<TrackKey>,
}

/// A new region's corners: the middle of the frame, a third of it wide.
pub const NEW_REGION: Quad = [[0.35, 0.35], [0.65, 0.35], [0.65, 0.65], [0.35, 0.65]];

impl MotionTrack {
    pub fn new(name: String) -> MotionTrack {
        MotionTrack {
            name,
            outside: false,
            region: NEW_REGION,
            keys: Vec::new(),
        }
    }

    /// The key at `at` (within half of `frame`), if one is there.
    pub fn key_at(&self, at: Tick, frame: Tick) -> Option<usize> {
        let half = Tick(frame.0 / 2);
        self.keys.iter().position(|k| (k.at - at).abs() <= half)
    }

    /// The region at `at`: the frames tracked around it, between them in
    /// a straight line, held past the first and the last; the region drawn
    /// where none is.
    pub fn quad_at(&self, at: Tick) -> Quad {
        let i = self.keys.partition_point(|k| k.at <= at);
        match (i.checked_sub(1).map(|j| self.keys[j]), self.keys.get(i)) {
            (None, None) => self.region,
            (Some(a), None) => a.quad,
            (None, Some(b)) => b.quad,
            (Some(a), Some(b)) => {
                let span = (b.at - a.at).0.max(1) as f64;
                let t = ((at - a.at).0 as f64 / span) as f32;
                let mut q = a.quad;
                for (c, (p, r)) in q.iter_mut().zip(a.quad.iter().zip(&b.quad)) {
                    *c = [p[0] + (r[0] - p[0]) * t, p[1] + (r[1] - p[1]) * t];
                }
                q
            }
        }
    }

    /// The region's middle at `at` (its corners' mean).
    pub fn center_at(&self, at: Tick) -> [f32; 2] {
        let q = self.quad_at(at);
        [
            q.iter().map(|p| p[0]).sum::<f32>() / 4.0,
            q.iter().map(|p| p[1]).sum::<f32>() / 4.0,
        ]
    }

    /// Sets the region tracked at `at` (in place of a key within half of
    /// `frame`), in time order.
    pub fn set_key(&mut self, at: Tick, quad: Quad, frame: Tick) {
        match self.key_at(at, frame) {
            Some(i) => self.keys[i].quad = quad,
            None => {
                let i = self.keys.partition_point(|k| k.at < at);
                self.keys.insert(i, TrackKey { at, quad });
            }
        }
    }

    /// The region moved by hand at `at`: the key there when one is, else
    /// the region drawn (the frames tracked keep theirs).
    pub fn set_region_at(&mut self, at: Tick, quad: Quad, frame: Tick) {
        match self.key_at(at, frame) {
            Some(i) => self.keys[i].quad = quad,
            None if self.keys.is_empty() => self.region = quad,
            None => self.set_key(at, quad, frame),
        }
    }

    /// The keyframes a location parameter gets from the track (VEGAS's
    /// motion transfer): the region's middle at each frame tracked, from
    /// `offset` after the tracked event's start to the other event's start
    /// (times before it or past `duration` left out), as a plug-in counts
    /// a point — 0–1 from the bottom left when `normalised`, else pixels
    /// of a `size` frame.
    pub fn location_keys(
        &self,
        offset: Tick,
        duration: Tick,
        normalised: bool,
        size: (u32, u32),
    ) -> Option<PluginParam> {
        let (w, h) = if normalised {
            (1.0, 1.0)
        } else {
            (size.0 as f64, size.1 as f64)
        };
        let keys: Vec<PluginKey> = self
            .keys
            .iter()
            .filter_map(|k| {
                let at = k.at - offset;
                if at < Tick::ZERO || at > duration {
                    return None;
                }
                let c = [
                    k.quad.iter().map(|p| p[0]).sum::<f32>() as f64 / 4.0,
                    k.quad.iter().map(|p| p[1]).sum::<f32>() as f64 / 4.0,
                ];
                Some(PluginKey::new(
                    at,
                    PluginValue::Num(vec![c[0] * w, (1.0 - c[1]) * h]),
                    KeyCurve::Linear,
                ))
            })
            .collect();
        let value = keys.first()?.value.clone();
        Some(PluginParam { value, keys })
    }

    /// Its keys moved `delta` earlier (the event's start moved `delta`
    /// later into its media).
    pub(crate) fn rebase(&mut self, delta: Tick) {
        for k in &mut self.keys {
            k.at -= delta;
        }
    }

    /// The frames tracked as runs of time from the event's start (the
    /// window's indicator: green where there is motion data), runs broken
    /// where frames `frame` apart are missing.
    pub fn tracked_spans(&self, frame: Tick) -> Vec<(Tick, Tick)> {
        let mut out: Vec<(Tick, Tick)> = Vec::new();
        for k in &self.keys {
            match out.last_mut() {
                Some((_, end)) if (k.at - *end).0 <= frame.0 + frame.0 / 2 => *end = k.at,
                _ => out.push((k.at, k.at)),
            }
        }
        out
    }
}

/// The next free region's name: "Region 1", "Region 2"…
pub fn next_region_name(tracks: &[MotionTrack]) -> String {
    (1..)
        .map(|n| format!("Region {n}"))
        .find(|n| tracks.iter().all(|t| &t.name != n))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_between_and_past_their_keys() {
        let f = Tick::from_seconds(1);
        let mut t = MotionTrack::new(next_region_name(&[]));
        assert_eq!(t.name, "Region 1");
        assert_eq!(t.quad_at(Tick::ZERO), NEW_REGION);
        let shifted = |dx: f32| NEW_REGION.map(|p| [p[0] + dx, p[1]]);
        t.set_key(Tick::from_seconds(2), shifted(0.2), f);
        t.set_key(Tick::ZERO, shifted(0.0), f);
        // Halfway: halfway; past the last: held.
        let mid = t.quad_at(Tick::from_seconds(1));
        assert!((mid[0][0] - 0.45).abs() < 1e-6);
        assert_eq!(t.quad_at(Tick::from_seconds(9)), shifted(0.2));
        assert_eq!(t.tracked_spans(f).len(), 2);
        // To a location: the middle, y from the bottom; the other event
        // starting a second later counts from its start.
        let p = t
            .location_keys(
                Tick::from_seconds(1),
                Tick::from_seconds(5),
                true,
                (1920, 1080),
            )
            .unwrap();
        assert_eq!(p.keys.len(), 1);
        assert_eq!(p.keys[0].at, Tick::from_seconds(1));
        let PluginValue::Num(v) = &p.keys[0].value else {
            panic!()
        };
        assert!(
            (v[0] - 0.7).abs() < 1e-6 && (v[1] - 0.5).abs() < 1e-6,
            "{v:?}"
        );
        // A trim of the event's start moves them with it.
        t.rebase(Tick::from_seconds(1));
        assert_eq!(t.keys[0].at, Tick::ZERO - Tick::from_seconds(1));
    }
}
