//! Envelope painting, as VEGAS's: Shift and a press on an envelope's line
//! start a stroke — a point there, with the value the envelope already has
//! (no step) —, and each place the pointer goes after it puts a point,
//! taking away the points the pointer passed over (backward too: the
//! stroke paints over what was there). Let go, the stroke is thinned when
//! Options › Preferences › External Control & Automation says so ("Smooth
//! and thin automation data after recording or drawing"): its ends, its
//! turns (peaks and valleys) and the plateaus that matter stay, joined by
//! Smooth curves.

use bv_core::Tick;

use crate::edit::EditError;
use crate::envelope::{Envelope, EnvelopeOwner, EnvelopePoint};
use crate::fx::KeyCurve;
use crate::model::Project;

/// A stroke as far as it went: the time of its press, then each place the
/// pointer went (time, value), in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stroke {
    pub press: Tick,
    pub moves: Vec<(Tick, f32)>,
}

impl Stroke {
    pub fn new(press: Tick) -> Stroke {
        Stroke {
            press,
            moves: Vec::new(),
        }
    }

    /// The times the stroke spans: from its earliest place to its latest.
    pub fn span(&self) -> (Tick, Tick) {
        self.moves
            .iter()
            .fold((self.press, self.press), |(lo, hi), &(t, _)| {
                (lo.min(t), hi.max(t))
            })
    }
}

impl Envelope {
    /// A point at `at` with `value`, its curve the one of the point before
    /// it (Linear before the first); nothing when a point is there already.
    /// Whether it was put.
    fn put_painted(&mut self, at: Tick, value: f32) -> bool {
        let i = self.points.partition_point(|p| p.at < at);
        if self.points.get(i).is_some_and(|p| p.at == at) {
            return false;
        }
        let curve = i
            .checked_sub(1)
            .map_or(KeyCurve::Linear, |b| self.points[b].curve);
        self.points.insert(i, EnvelopePoint { at, value, curve });
        true
    }

    /// The press of a stroke at `at`: a point there with the value the
    /// envelope has there.
    pub fn paint_press(&mut self, at: Tick) {
        let value = self.value_at(at);
        self.put_painted(at, value);
    }

    /// The pointer gone from `from` to `at` (its value `value`): the points
    /// strictly between the two go (never the first), and a point goes at
    /// `at`.
    pub fn paint_move(&mut self, from: Tick, at: Tick, value: f32) {
        let (lo, hi) = (from.min(at), from.max(at));
        let mut i = 0;
        self.points.retain(|p| {
            let keep = i == 0 || p.at <= lo || p.at >= hi;
            i += 1;
            keep
        });
        self.put_painted(at, value);
    }

    /// A stroke over `start`..`end` let go: thinned. The points from the
    /// last one at or before `start` to the last one at or before `end` are
    /// looked at; those [`stroke_keeps`] leaves out go, from the last to the
    /// first, the point before each taking a Smooth curve (a mute's: held,
    /// and only its changes stay). `tolerance`: the time two points kept
    /// may be apart at most before both stay (20 pixels at the zoom it was
    /// drawn at). Returns how many points went.
    pub fn thin_stroke(&mut self, start: Tick, end: Tick, tolerance: Tick) -> usize {
        if start >= end {
            return 0;
        }
        let last_at = |t: Tick| {
            self.points
                .iter()
                .enumerate()
                .skip(1)
                .take_while(|(_, p)| p.at <= t)
                .last()
                .map_or(0, |(i, _)| i)
        };
        let (a, b) = (last_at(start), last_at(end));
        if b <= a {
            return 0;
        }
        let held = self.kind.is_held();
        let keep = if held {
            changes_keep(&self.points[a..=b])
        } else {
            stroke_keeps(&self.points[a..=b], tolerance)
        };
        let curve = if held {
            KeyCurve::Hold
        } else {
            KeyCurve::Smooth
        };
        let mut gone = 0;
        for i in (a + 1..b).rev() {
            if !keep[i - a] {
                self.points[i - 1].curve = curve;
                self.points.remove(i);
                gone += 1;
            }
        }
        gone
    }
}

/// Which points of a painted stretch thinning keeps, as VEGAS's: the first
/// and the last; where the line turns (a point going up after going down,
/// or down after up), the point it turned on — and when that point ends a
/// plateau (points of one value), the point the plateau began on too; and
/// both points of each pair further apart than `tolerance` (a plateau
/// counts from the point before it to its last). A plateau the line goes on
/// past the same way loses its points. The first point's way is its first
/// segment's; a plateau right at the start counts as going down.
pub fn stroke_keeps(points: &[EnvelopePoint], tolerance: Tick) -> Vec<bool> {
    let n = points.len();
    let mut keep = vec![false; n];
    if n < 2 {
        return vec![true; n];
    }
    // Each point's way (up), and whether it is on a plateau.
    let mut up = vec![false; n];
    let mut level = vec![false; n];
    let mut first = true;
    // The last point a slope reached: where a plateau after it began.
    let mut slope_end: Option<usize> = None;
    let mut cur = 0;
    while cur + 1 < n {
        let prev = cur;
        cur += 1;
        let mut reached = slope_end;
        if points[cur].value == points[prev].value {
            // A plateau: on while the value holds, each point taking the way
            // of the one before.
            level[cur] = true;
            up[cur] = up[prev];
            while cur + 1 < n && points[cur + 1].value == points[cur].value {
                cur += 1;
                level[cur] = true;
                up[cur] = up[cur - 1];
            }
        } else {
            let rising = points[cur].value > points[prev].value;
            up[cur] = rising;
            reached = Some(cur);
            if !first && rising != up[prev] {
                // It turned on `prev`.
                keep[prev] = true;
                if level[prev]
                    && let Some(s) = slope_end
                {
                    keep[s] = true;
                }
            }
        }
        if first {
            level[prev] = false;
            up[prev] = up[cur];
            keep[prev] = true;
            first = false;
        }
        if points[cur].at - points[prev].at > tolerance {
            keep[cur] = true;
            keep[prev] = true;
        }
        slope_end = reached;
    }
    keep[n - 1] = true;
    keep
}

/// A held envelope's stretch (a mute's): the first and last points stay,
/// and those where the value changes.
fn changes_keep(points: &[EnvelopePoint]) -> Vec<bool> {
    let n = points.len();
    (0..n)
        .map(|i| i == 0 || i + 1 == n || points[i].value != points[i - 1].value)
        .collect()
}

impl Project {
    /// Paints `stroke` on `owner`'s `kind` envelope (its times and values
    /// held within the envelope's); `thin`: the stroke was let go, and is
    /// thinned with this tolerance (see [`Envelope::thin_stroke`]).
    pub fn paint_envelope(
        &mut self,
        owner: impl Into<EnvelopeOwner>,
        kind: crate::envelope::EnvelopeKind,
        stroke: &Stroke,
        thin: Option<Tick>,
    ) -> Result<(), EditError> {
        let owner = owner.into();
        let end = self.envelope_end(owner);
        let env = self.envelope_mut(owner, kind)?;
        let (lo, hi) = kind.range();
        let held_in = |t: Tick| t.clamp(Tick::ZERO, end);
        let mut last = held_in(stroke.press);
        env.paint_press(last);
        for &(at, value) in &stroke.moves {
            let at = held_in(at);
            env.paint_move(last, at, value.clamp(lo, hi));
            last = at;
        }
        if let Some(tolerance) = thin {
            let (start, stop) = stroke.span();
            env.thin_stroke(held_in(start), held_in(stop), tolerance);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::EnvelopeKind;

    /// A composite level envelope: one point at 0, 100 %, Smooth.
    fn level() -> Envelope {
        Envelope::with_curve(EnvelopeKind::CompositeLevel, KeyCurve::Smooth)
    }

    fn times(e: &Envelope) -> Vec<i64> {
        e.points.iter().map(|p| p.at.0).collect()
    }

    #[test]
    fn the_press_keeps_the_value_there_and_the_curve_before() {
        let mut e = level();
        e.points.push(EnvelopePoint {
            at: Tick(100),
            value: 0.0,
            curve: KeyCurve::Linear,
        });
        e.paint_press(Tick(50));
        assert_eq!(times(&e), [0, 50, 100]);
        let p = e.points[1];
        assert!((p.value - e.value_at(Tick(50))).abs() < 1e-6);
        // Smooth from 1 to 0 is at a half halfway.
        assert!((p.value - 0.5).abs() < 1e-6, "{}", p.value);
        assert_eq!(p.curve, KeyCurve::Smooth, "the point before's");
        // Pressed on a point: nothing more.
        e.paint_press(Tick(100));
        assert_eq!(times(&e), [0, 50, 100]);
    }

    #[test]
    fn a_move_takes_the_points_it_passes_over_backward_too() {
        let mut e = level();
        for t in [10, 20, 30, 40] {
            e.points.push(EnvelopePoint {
                at: Tick(t),
                value: 0.5,
                curve: KeyCurve::Linear,
            });
        }
        e.paint_move(Tick(5), Tick(35), 0.2);
        assert_eq!(times(&e), [0, 35, 40], "10, 20 and 30 passed over");
        assert_eq!(e.points[1].curve, KeyCurve::Smooth, "the first's curve");
        assert!((e.points[1].value - 0.2).abs() < 1e-6);
        // Back over the first point: it stays.
        e.paint_move(Tick(35), Tick(0), 0.9);
        assert_eq!(times(&e), [0, 35, 40], "a point at 0 already");
        assert_eq!(e.points[0].value, 1.0, "left as it was");
        // A point at the same time is not put twice, its value kept.
        e.paint_move(Tick(0), Tick(40), 0.1);
        assert_eq!(times(&e), [0, 40]);
        assert!((e.points[1].value - 0.5).abs() < 1e-6);
    }

    /// The stroke VEGAS was measured with: pressed on a composite level
    /// envelope at 100 % (a 67-pixel lane), 143 pixels into the view, then
    /// 90 steps of 6 pixels right, going down and up 21 pixels every 14
    /// steps, and 30 pixels down from step 41 to 52.
    fn measured_stroke(px: i64) -> Stroke {
        let mut s = Stroke::new(Tick(143 * px));
        for k in 1..=90i64 {
            let f = k % 14;
            let dy = if (41..=52).contains(&k) {
                30
            } else {
                3 * f.min(14 - f)
            };
            // The lane's 67 pixels: a 65th of the range each.
            let v = (65 - dy) as f32 * (1.0f32 / 65.0);
            s.moves.push((Tick((143 + 6 * k) * px), v));
        }
        s
    }

    #[test]
    fn the_measured_stroke_paints_a_point_a_step() {
        let px = 1_000_000;
        let mut p = Project::default();
        let v = p.tracks[0].id;
        p.tracks[0].envelopes.push(level());
        p.paint_envelope(v, EnvelopeKind::CompositeLevel, &measured_stroke(px), None)
            .unwrap();
        let e = &p.tracks[0].envelopes[0];
        assert_eq!(e.points.len(), 92, "the one at 0, the press and 90 steps");
        assert_eq!(e.points[1].at, Tick(143 * px));
        assert_eq!(e.points[1].value, 1.0, "no step at the press");
        assert!(e.points.iter().all(|p| p.curve == KeyCurve::Smooth));
    }

    #[test]
    fn the_measured_stroke_thins_to_what_vegas_kept() {
        let px = 1_000_000;
        let mut p = Project::default();
        let v = p.tracks[0].id;
        p.tracks[0].envelopes.push(level());
        // Let go: 20 pixels of tolerance.
        p.paint_envelope(
            v,
            EnvelopeKind::CompositeLevel,
            &measured_stroke(px),
            Some(Tick(20 * px)),
        )
        .unwrap();
        let e = &p.tracks[0].envelopes[0];
        // VEGAS's sixteen points (pixels from the view's start, 65ths).
        let want: [(i64, i64); 16] = [
            (0, 65),
            (143, 65),
            (185, 44),
            (227, 65),
            (269, 44),
            (311, 65),
            (353, 44),
            (383, 59),
            (389, 35),
            (455, 35),
            (479, 65),
            (521, 44),
            (563, 65),
            (605, 44),
            (647, 65),
            (683, 47),
        ];
        let got: Vec<(i64, i64)> = e
            .points
            .iter()
            .map(|p| (p.at.0 / px, (p.value * 65.0).round() as i64))
            .collect();
        assert_eq!(got, want);
        assert!(e.points.iter().all(|p| p.curve == KeyCurve::Smooth));
    }

    #[test]
    fn thinning_needs_a_stroke_that_went_somewhere() {
        let mut e = level();
        e.paint_press(Tick(10));
        assert_eq!(e.thin_stroke(Tick(10), Tick(10), Tick(5)), 0);
        assert_eq!(times(&e), [0, 10], "a press alone stays");
    }

    #[test]
    fn a_long_gap_keeps_both_its_points() {
        let mut e = level();
        // Straight down: no turn, only the ends — unless points are far apart.
        let mut s = Stroke::new(Tick(100));
        for (t, v) in [(110, 0.9), (120, 0.8), (200, 0.7), (210, 0.6), (220, 0.5)] {
            s.moves.push((Tick(t), v));
        }
        let mut p = Project::default();
        let id = p.tracks[0].id;
        p.tracks[0].envelopes.push(e.clone());
        p.paint_envelope(id, EnvelopeKind::CompositeLevel, &s, Some(Tick(50)))
            .unwrap();
        e = p.tracks[0].envelopes[0].clone();
        assert_eq!(times(&e), [0, 100, 120, 200, 220]);
    }

    #[test]
    fn a_mute_keeps_its_changes_held() {
        let mut p = Project::default();
        let id = p.tracks[0].id;
        p.tracks[0]
            .envelopes
            .push(Envelope::new(EnvelopeKind::Mute));
        let mut s = Stroke::new(Tick(100));
        for (t, v) in [(110, 1.0), (120, 0.0), (130, 0.0), (140, 1.0), (150, 1.0)] {
            s.moves.push((Tick(t), v));
        }
        p.paint_envelope(id, EnvelopeKind::Mute, &s, Some(Tick(1_000_000)))
            .unwrap();
        let e = &p.tracks[0].envelopes[0];
        assert_eq!(times(e), [0, 100, 120, 140, 150]);
        assert!(e.points.iter().all(|p| p.curve == KeyCurve::Hold));
    }
}
