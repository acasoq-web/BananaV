//! Track envelopes (Insert › Audio/Video Envelopes): volume, pan and mute
//! automation drawn over a track and applied by the mixer and renderer —
//! an audio track's sends too (VEGAS's bus envelopes) —; busses' and
//! assignable effects chains' volume, pan and mute (on their bus tracks);
//! and events' envelopes (an event's Insert/Remove Envelope): velocity and
//! transition progress, their times counted from the event's start.

use bv_core::{Tick, TimeRange};
use serde::{Deserialize, Serialize};

use crate::busses::{SendTo, Strip};
use crate::edit::EditError;
use crate::fx::KeyCurve;
use crate::model::{ClipId, Project, TrackId, TrackKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EnvelopeKind {
    /// Linear gain (1 = 0 dB, up to +12 dB). Audio only.
    Volume,
    /// −1 left … +1 right. Audio only.
    Pan,
    /// 1 = playing, 0 = muted (always held, no ramps). Audio and video.
    Mute,
    /// The track's opacity, 0–1 ("Track Composite Level"). Video only.
    CompositeLevel,
    /// Toward the track's top fade color (+1) or its bottom one (−1)
    /// ("Track Fade to Color"). Video only.
    FadeToColor,
    /// An event's speed (Insert/Remove Envelope › Velocity): 1 = 100 %, 0
    /// holds the frame, below 0 plays backward (−100 % to 300 %). Video
    /// events only.
    Velocity,
    /// How far an event's entrance transition has gone, 0–1 (without it, a
    /// straight line over the transition): it can hold, go back and repeat.
    /// Video events only.
    TransitionProgress,
    /// An audio track's send to a bus or a chain over time (VEGAS's bus
    /// envelopes): linear gain on the send's level (1 = its level, up to
    /// +12 dB).
    Send(SendTo),
    /// The video bus's Motion Blur Amount: the span (seconds, 0–1) of the
    /// frames each frame averages, half of it before and half after.
    MotionBlur,
    /// The video bus's Video Supersampling: how many frames are worked out
    /// for each of the project's (0–8; 0 and 1: just it).
    Supersampling,
    /// An audio effect's parameter (FX Automation, [`crate::fx_automation`]):
    /// 0–1 of its range; the effect is the one numbered `fx` in the
    /// track's (or the bus's, the chain's) chain.
    FxParam { fx: u32, param: u32 },
    /// An audio effect's bypass: 1 on, 0 bypassed (always held).
    FxBypass(u32),
}

/// Whose envelope: a track's (its times on the timeline), an event's (its
/// times from the event's start), a bus's or a chain's (on the timeline:
/// its bus track) or the video bus's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EnvelopeOwner {
    Track(TrackId),
    Event(ClipId),
    Strip(Strip),
    VideoBus,
}

impl From<Strip> for EnvelopeOwner {
    fn from(s: Strip) -> EnvelopeOwner {
        EnvelopeOwner::Strip(s)
    }
}

impl From<TrackId> for EnvelopeOwner {
    fn from(t: TrackId) -> EnvelopeOwner {
        EnvelopeOwner::Track(t)
    }
}

impl From<ClipId> for EnvelopeOwner {
    fn from(c: ClipId) -> EnvelopeOwner {
        EnvelopeOwner::Event(c)
    }
}

impl EnvelopeKind {
    pub const AUDIO: [EnvelopeKind; 3] =
        [EnvelopeKind::Mute, EnvelopeKind::Volume, EnvelopeKind::Pan];
    pub const VIDEO: [EnvelopeKind; 3] = [
        EnvelopeKind::Mute,
        EnvelopeKind::CompositeLevel,
        EnvelopeKind::FadeToColor,
    ];
    /// The envelopes video events have.
    pub const EVENT: [EnvelopeKind; 2] = [EnvelopeKind::Velocity, EnvelopeKind::TransitionProgress];
    /// The video bus's.
    pub const VIDEO_BUS: [EnvelopeKind; 4] = [
        EnvelopeKind::Mute,
        EnvelopeKind::FadeToColor,
        EnvelopeKind::MotionBlur,
        EnvelopeKind::Supersampling,
    ];

    /// Whether it holds each value until its next point (a mute's, an
    /// effect's bypass).
    pub fn is_held(self) -> bool {
        matches!(self, EnvelopeKind::Mute | EnvelopeKind::FxBypass(_))
    }

    /// An audio effect's automation.
    pub fn is_fx(self) -> bool {
        matches!(
            self,
            EnvelopeKind::FxParam { .. } | EnvelopeKind::FxBypass(_)
        )
    }

    /// Whether events have it rather than tracks.
    pub fn is_event(self) -> bool {
        matches!(
            self,
            EnvelopeKind::Velocity | EnvelopeKind::TransitionProgress
        )
    }

    pub fn default_value(self) -> f32 {
        match self {
            EnvelopeKind::Volume
            | EnvelopeKind::Send(_)
            | EnvelopeKind::Mute
            | EnvelopeKind::CompositeLevel
            | EnvelopeKind::Velocity
            | EnvelopeKind::FxBypass(_) => 1.0,
            EnvelopeKind::FxParam { .. } => 0.5,
            EnvelopeKind::Pan
            | EnvelopeKind::FadeToColor
            | EnvelopeKind::TransitionProgress
            | EnvelopeKind::MotionBlur
            | EnvelopeKind::Supersampling => 0.0,
        }
    }

    pub fn range(self) -> (f32, f32) {
        match self {
            EnvelopeKind::Volume | EnvelopeKind::Send(_) => (0.0, MAX_VOLUME),
            EnvelopeKind::Pan | EnvelopeKind::FadeToColor => (-1.0, 1.0),
            EnvelopeKind::Mute
            | EnvelopeKind::CompositeLevel
            | EnvelopeKind::TransitionProgress
            | EnvelopeKind::MotionBlur
            | EnvelopeKind::FxParam { .. }
            | EnvelopeKind::FxBypass(_) => (0.0, 1.0),
            EnvelopeKind::Velocity => (-1.0, 3.0),
            EnvelopeKind::Supersampling => (0.0, 8.0),
        }
    }

    /// Whether Flip All Points / Flip Selected Points can turn it over:
    /// VEGAS's volume, pan and composite level envelopes (an adjustment
    /// track's point menu has it, the user's prints).
    pub fn flips(self) -> bool {
        matches!(
            self,
            EnvelopeKind::Volume
                | EnvelopeKind::Pan
                | EnvelopeKind::Send(_)
                | EnvelopeKind::CompositeLevel
        )
    }

    pub fn applies_to(self, kind: TrackKind) -> bool {
        match kind {
            TrackKind::Audio => matches!(
                self,
                EnvelopeKind::Volume
                    | EnvelopeKind::Pan
                    | EnvelopeKind::Mute
                    | EnvelopeKind::Send(_)
                    | EnvelopeKind::FxParam { .. }
                    | EnvelopeKind::FxBypass(_)
            ),
            TrackKind::Video => matches!(
                self,
                EnvelopeKind::Mute | EnvelopeKind::CompositeLevel | EnvelopeKind::FadeToColor
            ),
        }
    }

    /// Vertical position (0 = bottom, 1 = top) of `value` on the lane.
    /// Volume is drawn as VEGAS draws it, linear in gain: +6 dB at the top
    /// and 0 dB in the middle (the user's print), louder held at the top.
    pub fn to_unit(self, value: f32) -> f32 {
        match self {
            EnvelopeKind::Volume | EnvelopeKind::Send(_) => {
                (value / VOLUME_ENVELOPE_TOP).clamp(0.0, 1.0)
            }
            EnvelopeKind::Pan | EnvelopeKind::FadeToColor => ((value + 1.0) / 2.0).clamp(0.0, 1.0),
            EnvelopeKind::CompositeLevel
            | EnvelopeKind::TransitionProgress
            | EnvelopeKind::MotionBlur
            | EnvelopeKind::FxParam { .. } => value.clamp(0.0, 1.0),
            EnvelopeKind::Supersampling => (value / 8.0).clamp(0.0, 1.0),
            // −100 % at the bottom, 300 % at the top: 100 % in the middle.
            EnvelopeKind::Velocity => ((value + 1.0) / 4.0).clamp(0.0, 1.0),
            EnvelopeKind::Mute | EnvelopeKind::FxBypass(_) => {
                if value >= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }

    /// Inverse of [`EnvelopeKind::to_unit`].
    pub fn from_unit(self, u: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        match self {
            EnvelopeKind::Volume | EnvelopeKind::Send(_) => u * VOLUME_ENVELOPE_TOP,
            EnvelopeKind::Pan | EnvelopeKind::FadeToColor => u * 2.0 - 1.0,
            EnvelopeKind::CompositeLevel
            | EnvelopeKind::TransitionProgress
            | EnvelopeKind::MotionBlur
            | EnvelopeKind::FxParam { .. } => u,
            EnvelopeKind::Supersampling => (u * 8.0).round(),
            EnvelopeKind::Velocity => u * 4.0 - 1.0,
            EnvelopeKind::Mute | EnvelopeKind::FxBypass(_) => (u >= 0.5) as u8 as f32,
        }
    }
}

/// +12 dB.
pub const MAX_VOLUME: f32 = 3.981_072;

/// +6 dB: the top of a volume envelope's lane, as far as VEGAS's goes
/// dragged (its track's fader goes to +12 dB).
pub const VOLUME_ENVELOPE_TOP: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnvelopePoint {
    pub at: Tick,
    pub value: f32,
    /// Shape of the segment that starts at this point.
    #[serde(default)]
    pub curve: KeyCurve,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub kind: EnvelopeKind,
    /// Sorted by time; never empty.
    pub points: Vec<EnvelopePoint>,
}

impl Envelope {
    pub fn new(kind: EnvelopeKind) -> Envelope {
        Envelope::with_curve(kind, KeyCurve::Linear)
    }

    /// A new envelope whose points start with `curve` (a mute envelope's
    /// always hold).
    pub fn with_curve(kind: EnvelopeKind, curve: KeyCurve) -> Envelope {
        Envelope {
            kind,
            points: vec![EnvelopePoint {
                at: Tick::ZERO,
                value: kind.default_value(),
                curve: if kind.is_held() {
                    KeyCurve::Hold
                } else {
                    curve
                },
            }],
        }
    }

    /// Value at timeline time `t`.
    pub fn value_at(&self, t: Tick) -> f32 {
        let Some(first) = self.points.first() else {
            return self.kind.default_value();
        };
        if t <= first.at {
            return first.value;
        }
        for w in self.points.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            if t < b.at {
                let curve = if self.kind.is_held() {
                    KeyCurve::Hold
                } else {
                    a.curve
                };
                let span = (b.at - a.at).0.max(1) as f64;
                let f = curve.eval(((t - a.at).0 as f64 / span) as f32);
                return a.value + (b.value - a.value) * f;
            }
        }
        self.points.last().map_or(first.value, |p| p.value)
    }

    /// The sum of the envelope's values from 0 to `d`, in ticks: how far a
    /// velocity envelope carries its event into its media. The sum never
    /// falls below `floor` (the media's start: an event playing backward
    /// stops on its first frame until it plays forward again); before 0 the
    /// first point's value holds.
    pub fn travel(&self, d: Tick, floor: f64) -> f64 {
        let first = self
            .points
            .first()
            .map_or(self.kind.default_value(), |p| p.value) as f64;
        if d.0 <= 0 {
            return first * d.0 as f64;
        }
        let end = d.0 as f64;
        let mut pos = 0.0f64;
        let mut x = 0.0f64;
        // Before the first point, its value.
        if let Some(p) = self.points.first() {
            let to = (p.at.0 as f64).min(end);
            if to > x {
                pos = (pos + first * (to - x)).max(floor);
                x = to;
            }
        }
        for w in self.points.windows(2) {
            if x >= end {
                break;
            }
            let (a, b) = (w[0], w[1]);
            let (xa, xb) = (a.at.0 as f64, b.at.0 as f64);
            let (s0, s1) = (x.max(xa), xb.min(end));
            if s1 <= s0 {
                continue;
            }
            let curve = if self.kind.is_held() {
                KeyCurve::Hold
            } else {
                a.curve
            };
            let span = (xb - xa).max(1.0);
            let v = |x: f64| -> f64 {
                let f = curve.eval(((x - xa) / span) as f32) as f64;
                a.value as f64 + (b.value - a.value) as f64 * f
            };
            let straight = matches!(curve, KeyCurve::Linear | KeyCurve::Hold);
            pos = segment_travel(pos, &v, s0, s1, straight, floor);
            x = s1;
        }
        if x < end {
            let last = self.points.last().map_or(first, |p| p.value as f64);
            pos = (pos + last * (end - x)).max(floor);
        }
        pos
    }

    /// `true` when the envelope never leaves its default value.
    pub fn is_flat_default(&self) -> bool {
        self.points
            .iter()
            .all(|p| (p.value - self.kind.default_value()).abs() < 1e-6)
    }
}

/// [`Envelope::travel`] over one segment whose values are `v`: where they
/// change sign, in two parts, each moving one way only (so the floor is
/// met where it is).
fn segment_travel(
    pos: f64,
    v: &dyn Fn(f64) -> f64,
    s0: f64,
    s1: f64,
    straight: bool,
    floor: f64,
) -> f64 {
    let area = |a: f64, b: f64| -> f64 {
        if straight {
            0.5 * (v(a) + v(b)) * (b - a)
        } else {
            // Simpson's rule.
            let n = 64;
            let h = (b - a) / n as f64;
            let mut sum = v(a) + v(b);
            for i in 1..n {
                let w = if i % 2 == 1 { 4.0 } else { 2.0 };
                sum += w * v(a + i as f64 * h);
            }
            sum * h / 3.0
        }
    };
    let (v0, v1) = (v(s0), v(s1));
    if v0 * v1 < 0.0 {
        // The curves are monotone: the values cross zero once.
        let (mut lo, mut hi) = (s0, s1);
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if (v(mid) < 0.0) == (v0 < 0.0) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let z = 0.5 * (lo + hi);
        let pos = (pos + area(s0, z)).max(floor);
        return (pos + area(z, s1)).max(floor);
    }
    (pos + area(s0, s1)).max(floor)
}

type Result<T> = std::result::Result<T, EditError>;

/// How long recorded automation takes to ramp from and back to what the
/// envelope was.
const RECORD_RAMP: Tick = Tick(bv_core::TICKS_PER_SECOND / 100);

/// How far (on an envelope's unit scale) the line between the samples
/// kept may pass from one left out.
const THIN_TOLERANCE: f32 = 0.004;

/// The samples worth keeping of a recorded line (Ramer–Douglas–Peucker):
/// the ends, and wherever leaving a sample out would move the line — drawn
/// between the samples kept as playback draws it — more than
/// [`THIN_TOLERANCE`] on the unit scale.
fn thin_samples(kind: EnvelopeKind, samples: &[(Tick, f32)]) -> Vec<(Tick, f32)> {
    if samples.len() <= 2 {
        return samples.to_vec();
    }
    let mut keep = vec![false; samples.len()];
    let (first, last) = (0, samples.len() - 1);
    keep[first] = true;
    keep[last] = true;
    let mut spans = vec![(first, last)];
    while let Some((a, b)) = spans.pop() {
        if b <= a + 1 {
            continue;
        }
        let (ta, tb) = (samples[a].0.0 as f64, samples[b].0.0 as f64);
        let mut worst = (0.0f32, a);
        for i in a + 1..b {
            let f = if tb > ta {
                ((samples[i].0.0 as f64 - ta) / (tb - ta)) as f32
            } else {
                0.0
            };
            let line = samples[a].1 + (samples[b].1 - samples[a].1) * f;
            let d = (kind.to_unit(samples[i].1) - kind.to_unit(line)).abs();
            if d > worst.0 {
                worst = (d, i);
            }
        }
        if worst.0 > THIN_TOLERANCE {
            keep[worst.1] = true;
            spans.push((a, worst.1));
            spans.push((worst.1, b));
        }
    }
    samples
        .iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(s, _)| *s)
        .collect()
}

/// Thin All Points / Thin Selected Points on `points` (sorted): within
/// `range` (all of them without one), the points of straight segments the
/// line passes close by anyway are left out, as [`thin_samples`] leaves
/// recorded ones. The first point, curved segments' points and the ends of
/// each straight run stay.
fn thin_points(
    kind: EnvelopeKind,
    points: &[EnvelopePoint],
    range: Option<TimeRange>,
) -> Vec<EnvelopePoint> {
    let inside = |p: &EnvelopePoint| range.is_none_or(|r| p.at >= r.start && p.at <= r.end());
    let straight = |p: &EnvelopePoint| p.curve == KeyCurve::Linear;
    let mut keep = vec![true; points.len()];
    let mut i = 0;
    while i < points.len() {
        // A run of straight segments within the range: points i..=j.
        if !inside(&points[i]) || !straight(&points[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j + 1 < points.len() && inside(&points[j + 1]) && straight(&points[j]) {
            j += 1;
        }
        if j > i + 1 {
            let samples: Vec<(Tick, f32)> = points[i..=j].iter().map(|p| (p.at, p.value)).collect();
            let kept = thin_samples(kind, &samples);
            for (k, p) in points[i..=j].iter().enumerate() {
                keep[i + k] = kept.iter().any(|s| s.0 == p.at);
            }
        }
        i = j.max(i + 1);
    }
    keep[0] = true;
    points
        .iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(p, _)| *p)
        .collect()
}

impl Project {
    /// Thin All Points (`range` `None`) or Thin Selected Points (within the
    /// time selection): fewer points, the envelope's line kept. Returns how
    /// many points were left out.
    pub fn thin_envelope(
        &mut self,
        owner: impl Into<EnvelopeOwner>,
        kind: EnvelopeKind,
        range: Option<TimeRange>,
    ) -> Result<usize> {
        let env = self
            .envelopes_mut(owner.into())?
            .iter_mut()
            .find(|e| e.kind == kind)
            .ok_or(EditError::NothingToInsert)?;
        if kind == EnvelopeKind::Mute {
            return Ok(0);
        }
        let before = env.points.len();
        env.points = thin_points(kind, &env.points, range);
        Ok(before - env.points.len())
    }

    /// Flip All Points (`range` `None`) or Flip Selected Points: the
    /// envelope turned over about the middle of its lane (pan's left and
    /// right swapped; volume's dB scale upside down).
    pub fn flip_envelope(
        &mut self,
        owner: impl Into<EnvelopeOwner>,
        kind: EnvelopeKind,
        range: Option<TimeRange>,
    ) -> Result<()> {
        if !kind.flips() {
            return Err(EditError::WrongTrackKind);
        }
        let env = self
            .envelopes_mut(owner.into())?
            .iter_mut()
            .find(|e| e.kind == kind)
            .ok_or(EditError::NothingToInsert)?;
        if let Some(r) = range {
            // The selection's edges keep the line's values where it was cut.
            for at in [r.start, r.end()] {
                if !env.points.iter().any(|p| p.at == at) {
                    let value = env.value_at(at);
                    let curve = env
                        .points
                        .iter()
                        .rev()
                        .find(|p| p.at <= at)
                        .map_or(KeyCurve::Linear, |p| p.curve);
                    env.points.push(EnvelopePoint { at, value, curve });
                }
            }
            env.points.sort_by_key(|p| p.at);
        }
        let (lo, hi) = kind.range();
        for p in &mut env.points {
            if range.is_none_or(|r| p.at >= r.start && p.at <= r.end()) {
                p.value = kind.from_unit(1.0 - kind.to_unit(p.value)).clamp(lo, hi);
            }
        }
        Ok(())
    }

    /// Options › Lock Envelopes to Events: the points of the track
    /// envelopes under events `ids` as they were in `before` go with them
    /// along their track, the points where they land giving way.
    pub fn carry_envelopes(&mut self, before: &Project, ids: &[ClipId]) {
        for t in &mut self.tracks {
            let Some(old_track) = before.track(t.id) else {
                continue;
            };
            // Each moved event of the track: where it was, and how far.
            let moved: Vec<(Tick, Tick, Tick)> = t
                .clips
                .iter()
                .filter(|c| ids.contains(&c.id))
                .filter_map(|c| {
                    let old = old_track.clips.iter().find(|o| o.id == c.id)?;
                    (c.start != old.start).then(|| (old.start, old.end(), c.start - old.start))
                })
                .collect();
            if moved.is_empty() {
                continue;
            }
            for env in &mut t.envelopes {
                let Some(old) = old_track.envelopes.iter().find(|e| e.kind == env.kind) else {
                    continue;
                };
                let mut points = Vec::new();
                for p in &old.points {
                    if let Some(&(_, _, d)) = moved.iter().find(|&&(s, e, _)| p.at >= s && p.at < e)
                    {
                        if p.at + d > Tick::ZERO {
                            points.push(EnvelopePoint { at: p.at + d, ..*p });
                        }
                    } else if p.at == Tick::ZERO
                        || !moved.iter().any(|&(s, e, d)| p.at >= s + d && p.at < e + d)
                    {
                        points.push(*p);
                    }
                }
                points.sort_by_key(|p| p.at);
                points.dedup_by_key(|p| p.at);
                if points.first().is_none_or(|p| p.at != Tick::ZERO) {
                    points.insert(
                        0,
                        EnvelopePoint {
                            at: Tick::ZERO,
                            value: old.value_at(Tick::ZERO),
                            curve: old.points[0].curve,
                        },
                    );
                }
                env.points = points;
            }
        }
    }

    /// Adds the envelope to the track, or removes it when present. Returns
    /// whether the track has it now.
    pub fn toggle_envelope(&mut self, track: TrackId, kind: EnvelopeKind) -> Result<bool> {
        self.toggle_envelope_as(track, kind, KeyCurve::Linear)
    }

    /// [`Self::toggle_envelope`], a new envelope's points starting with
    /// `curve` (Options › Preferences › External Control & Automation:
    /// "New audio envelopes", "New video envelopes").
    pub fn toggle_envelope_as(
        &mut self,
        track: TrackId,
        kind: EnvelopeKind,
        curve: KeyCurve,
    ) -> Result<bool> {
        // A send's envelope rides on its send: one at 0 dB when the track
        // had none to that bus or chain.
        if let EnvelopeKind::Send(to) = kind {
            let exists = match to {
                SendTo::Bus(b) => self.bus(b).is_some(),
                SendTo::Fx(c) => self.fx_chain(c).is_some(),
            };
            if !exists {
                return Err(EditError::NothingToInsert);
            }
        }
        let t = self
            .track_by_id_mut(track)
            .ok_or(EditError::UnknownTrack(track))?;
        if !kind.applies_to(t.kind) {
            return Err(EditError::WrongTrackKind);
        }
        if let Some(i) = t.envelopes.iter().position(|e| e.kind == kind) {
            t.envelopes.remove(i);
            Ok(false)
        } else {
            if let EnvelopeKind::Send(to) = kind
                && !t.sends.iter().any(|s| s.to == to)
            {
                t.sends.push(crate::busses::Send {
                    gain_db: 0.0,
                    ..crate::busses::Send::new(to)
                });
            }
            t.envelopes.push(Envelope::with_curve(kind, curve));
            Ok(true)
        }
    }

    /// Adds the envelope (volume, pan or mute) to the bus's or the chain's
    /// track, or removes it when present. Returns whether it has it now.
    pub fn toggle_strip_envelope(
        &mut self,
        strip: Strip,
        kind: EnvelopeKind,
        curve: KeyCurve,
    ) -> Result<bool> {
        if !EnvelopeKind::AUDIO.contains(&kind) {
            return Err(EditError::WrongTrackKind);
        }
        let envelopes = self.envelopes_mut(EnvelopeOwner::Strip(strip))?;
        if let Some(i) = envelopes.iter().position(|e| e.kind == kind) {
            envelopes.remove(i);
            Ok(false)
        } else {
            envelopes.push(Envelope::with_curve(kind, curve));
            Ok(true)
        }
    }

    /// `owner`'s envelopes.
    fn envelopes_mut(&mut self, owner: EnvelopeOwner) -> Result<&mut Vec<Envelope>> {
        match owner {
            EnvelopeOwner::Track(track) => Ok(&mut self
                .track_by_id_mut(track)
                .ok_or(EditError::UnknownTrack(track))?
                .envelopes),
            EnvelopeOwner::Event(clip) => Ok(&mut self
                .clip_mut(clip)
                .ok_or(EditError::UnknownClip(clip))?
                .envelopes),
            EnvelopeOwner::Strip(strip) => Ok(&mut self
                .strip_channel_mut(strip)
                .ok_or(EditError::NothingToInsert)?
                .envelopes),
            EnvelopeOwner::VideoBus => Ok(&mut self.video_bus.envelopes),
        }
    }

    pub(crate) fn envelope_mut(
        &mut self,
        owner: impl Into<EnvelopeOwner>,
        kind: EnvelopeKind,
    ) -> Result<&mut Envelope> {
        let owner = owner.into();
        let missing = match owner {
            EnvelopeOwner::Track(track) => EditError::UnknownTrack(track),
            EnvelopeOwner::Event(clip) => EditError::UnknownClip(clip),
            EnvelopeOwner::Strip(_) | EnvelopeOwner::VideoBus => EditError::NothingToInsert,
        };
        self.envelopes_mut(owner)?
            .iter_mut()
            .find(|e| e.kind == kind)
            .ok_or(missing)
    }

    /// The latest time a point of `owner`'s envelopes may take: an event's
    /// length (a track's have none).
    pub(crate) fn envelope_end(&self, owner: EnvelopeOwner) -> Tick {
        match owner {
            EnvelopeOwner::Track(_) | EnvelopeOwner::Strip(_) | EnvelopeOwner::VideoBus => {
                Tick(i64::MAX / 4)
            }
            EnvelopeOwner::Event(clip) => self.clip(clip).map_or(Tick::ZERO, |c| c.duration),
        }
    }

    /// The video event's `kind` envelope (velocity, transition progress)
    /// added, or removed when it has one; returns whether it has it now. A
    /// transition progress envelope needs a fade (a fade-in, a crossfade
    /// or a fade-out) and runs, as VEGAS's, from 0 to 100 % over the fade
    /// in and from 100 % back to 0 over the fade out, smooth.
    pub fn toggle_event_envelope(&mut self, clip: ClipId, kind: EnvelopeKind) -> Result<bool> {
        let track = self.clip_track(clip).ok_or(EditError::UnknownClip(clip))?;
        if !kind.is_event() || track.kind != TrackKind::Video {
            return Err(EditError::WrongTrackKind);
        }
        let c = self.clip(clip).ok_or(EditError::UnknownClip(clip))?;
        let (fade_in, fade_out) = track.effective_fades(c, self.auto_crossfade);
        // Its own fade-out: one an event overlaps is that event's.
        let fade_out = if track.crossfades(c).1.is_some() && self.auto_crossfade {
            Tick::ZERO
        } else {
            fade_out
        };
        let has = c.envelope(kind).is_some();
        if !has
            && kind == EnvelopeKind::TransitionProgress
            && fade_in <= Tick::ZERO
            && fade_out <= Tick::ZERO
        {
            return Err(EditError::NoTransition);
        }
        let duration = c.duration;
        let c = self.clip_mut(clip).ok_or(EditError::UnknownClip(clip))?;
        if has {
            c.envelopes.retain(|e| e.kind != kind);
            return Ok(false);
        }
        let mut env = Envelope::new(kind);
        if kind == EnvelopeKind::TransitionProgress {
            let point = |at: Tick, value: f32| EnvelopePoint {
                at,
                value,
                curve: KeyCurve::Smooth,
            };
            env.points.clear();
            if fade_in > Tick::ZERO {
                env.points
                    .extend([point(Tick::ZERO, 0.0), point(fade_in, 1.0)]);
            }
            if fade_out > Tick::ZERO {
                let from = (duration - fade_out).max(fade_in);
                env.points.retain(|p| p.at < from);
                env.points.extend([point(from, 1.0), point(duration, 0.0)]);
            }
        }
        c.envelopes.push(env);
        Ok(true)
    }

    /// Insert/Remove Envelope › Freeze Frame at Cursor: the video event's
    /// velocity envelope (added when it has none) drops to 0 at `at` (on the
    /// timeline), so the frame there stays to the event's end; what the
    /// envelope did before is kept.
    pub fn freeze_frame_at(&mut self, clip: ClipId, at: Tick) -> Result<()> {
        let c = self.clip(clip).ok_or(EditError::UnknownClip(clip))?;
        if at <= c.start || at >= c.end() {
            return Err(EditError::NotInsideClip);
        }
        if c.velocity().is_none() {
            self.toggle_event_envelope(clip, EnvelopeKind::Velocity)?;
        }
        let c = self.clip_mut(clip).ok_or(EditError::UnknownClip(clip))?;
        let d = at - c.start;
        let env = c
            .envelopes
            .iter_mut()
            .find(|e| e.kind == EnvelopeKind::Velocity)
            .expect("just added");
        let v = env.value_at(d);
        env.points.retain(|p| p.at <= d);
        let prev = env
            .points
            .iter()
            .rposition(|p| p.at < d)
            .expect("the first point is at the event's start");
        let flat = env.points[prev].value == v && env.points.last().is_some_and(|p| p.at < d);
        if flat {
            // Held to the cursor, then 0: two points where there was one.
            env.points[prev].curve = KeyCurve::Hold;
            env.points.push(EnvelopePoint {
                at: d,
                value: 0.0,
                curve: KeyCurve::Linear,
            });
        } else {
            env.points.retain(|p| p.at < d);
            env.points.push(EnvelopePoint {
                at: d,
                value: v,
                curve: KeyCurve::Hold,
            });
            env.points.push(EnvelopePoint {
                at: d + Tick(1),
                value: 0.0,
                curve: KeyCurve::Linear,
            });
        }
        Ok(())
    }

    /// Adds a point (or moves the value of the one already at `at`).
    /// Returns its index.
    pub fn set_envelope_point(
        &mut self,
        owner: impl Into<EnvelopeOwner>,
        kind: EnvelopeKind,
        at: Tick,
        value: f32,
    ) -> Result<usize> {
        let owner = owner.into();
        let end = self.envelope_end(owner);
        let env = self.envelope_mut(owner, kind)?;
        let (lo, hi) = kind.range();
        let value = value.clamp(lo, hi);
        let at = at.clamp(Tick::ZERO, end);
        if let Some(i) = env.points.iter().position(|p| p.at == at) {
            env.points[i].value = value;
            return Ok(i);
        }
        let curve = env
            .points
            .iter()
            .rev()
            .find(|p| p.at < at)
            .map_or(KeyCurve::Linear, |p| p.curve);
        env.points.push(EnvelopePoint { at, value, curve });
        env.points.sort_by_key(|p| p.at);
        Ok(env
            .points
            .iter()
            .position(|p| p.at == at)
            .expect("just added"))
    }

    /// Moves point `index` to `at` / `value`, keeping it between its
    /// neighbours. The first point stays at time zero.
    pub fn move_envelope_point(
        &mut self,
        owner: impl Into<EnvelopeOwner>,
        kind: EnvelopeKind,
        index: usize,
        at: Tick,
        value: f32,
    ) -> Result<()> {
        let owner = owner.into();
        let end = self.envelope_end(owner);
        let env = self.envelope_mut(owner, kind)?;
        let n = env.points.len();
        if index >= n {
            return Ok(());
        }
        let (lo, hi) = kind.range();
        let min = if index == 0 {
            Tick::ZERO
        } else {
            env.points[index - 1].at + Tick(1)
        };
        let max = if index + 1 < n {
            env.points[index + 1].at - Tick(1)
        } else {
            end
        };
        let p = &mut env.points[index];
        p.at = if index == 0 {
            Tick::ZERO
        } else {
            at.clamp(min, max.max(min))
        };
        p.value = value.clamp(lo, hi);
        Ok(())
    }

    /// Recorded automation (a control moved while playing, in Touch or
    /// Latch): the track's `kind` envelope — added, its points starting
    /// with `curve`, when it has none — takes `samples` (time, value) over
    /// their span, the points there replaced, ramping from and back to what
    /// it was just before and after. With `thin`, the samples the line
    /// passes close by anyway are left out (Options › Preferences ›
    /// External Control & Automation: "Smooth and thin automation data after
    /// recording or drawing").
    pub fn write_automation(
        &mut self,
        track: TrackId,
        kind: EnvelopeKind,
        samples: &[(Tick, f32)],
        thin: bool,
        curve: KeyCurve,
    ) -> Result<()> {
        self.write_automation_to(EnvelopeOwner::Track(track), kind, samples, thin, curve)
    }

    /// [`Self::write_automation`] on a track's envelope (a send's added
    /// with its send, at 0 dB, when the track had none to that bus or
    /// chain) or a bus track's (volume, pan, mute).
    pub fn write_automation_to(
        &mut self,
        owner: EnvelopeOwner,
        kind: EnvelopeKind,
        samples: &[(Tick, f32)],
        thin: bool,
        curve: KeyCurve,
    ) -> Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        match owner {
            EnvelopeOwner::Track(track) => {
                let t = self
                    .track_by_id_mut(track)
                    .ok_or(EditError::UnknownTrack(track))?;
                if !kind.applies_to(t.kind) {
                    return Err(EditError::WrongTrackKind);
                }
                if let EnvelopeKind::Send(to) = kind
                    && !t.sends.iter().any(|s| s.to == to)
                {
                    t.sends.push(crate::busses::Send {
                        gain_db: 0.0,
                        ..crate::busses::Send::new(to)
                    });
                }
            }
            EnvelopeOwner::Strip(_) if EnvelopeKind::AUDIO.contains(&kind) => {}
            EnvelopeOwner::VideoBus
                if matches!(
                    kind,
                    EnvelopeKind::Mute | EnvelopeKind::MotionBlur | EnvelopeKind::Supersampling
                ) => {}
            _ => return Err(EditError::WrongTrackKind),
        }
        let envelopes = self.envelopes_mut(owner)?;
        if !envelopes.iter().any(|e| e.kind == kind) {
            envelopes.push(Envelope::with_curve(kind, curve));
        }
        let env = envelopes
            .iter_mut()
            .find(|e| e.kind == kind)
            .expect("there now");
        let mut samples: Vec<(Tick, f32)> = samples.to_vec();
        samples.sort_by_key(|s| s.0);
        // One value a time: the last one recorded there.
        samples.reverse();
        samples.dedup_by_key(|s| s.0);
        samples.reverse();
        if thin {
            samples = thin_samples(kind, &samples);
        }
        let (first, last) = (samples[0].0.max(Tick::ZERO), samples[samples.len() - 1].0);
        let (lo, hi) = kind.range();
        let held = if kind == EnvelopeKind::Mute {
            KeyCurve::Hold
        } else {
            KeyCurve::Linear
        };
        // What the envelope was just before and after, and how it went on.
        let into = (first - RECORD_RAMP).max(Tick::ZERO);
        let out = last + RECORD_RAMP;
        let before = env.value_at(into);
        let after = env.value_at(out);
        let after_curve = env
            .points
            .iter()
            .rev()
            .find(|p| p.at <= out)
            .map_or(held, |p| p.curve);
        env.points
            .retain(|p| p.at == Tick::ZERO || p.at < into || p.at > out);
        if into > Tick::ZERO {
            env.points.push(EnvelopePoint {
                at: into,
                value: before,
                curve: held,
            });
        }
        for &(at, value) in &samples {
            let point = EnvelopePoint {
                at: at.max(Tick::ZERO),
                value: value.clamp(lo, hi),
                curve: held,
            };
            match env.points.iter_mut().find(|p| p.at == point.at) {
                Some(p) => *p = point,
                None => env.points.push(point),
            }
        }
        env.points.push(EnvelopePoint {
            at: out,
            value: after,
            curve: after_curve,
        });
        env.points.sort_by_key(|p| p.at);
        Ok(())
    }

    /// Raises (or lowers) an envelope within `inner` by `delta` on its unit
    /// scale (see [`EnvelopeKind::to_unit`]), with ramps out to `outer`'s
    /// edges, where it keeps its value (Options › Preferences › Editing:
    /// "Time selection envelope fades"). Points are added at the four edges;
    /// the ones within `inner` move with it. Called again with another
    /// `delta` on the envelope as it was, it drags the part up and down.
    pub fn lift_envelope(
        &mut self,
        track: TrackId,
        kind: EnvelopeKind,
        outer: TimeRange,
        inner: TimeRange,
        delta: f32,
    ) -> Result<()> {
        let env = self.envelope_mut(track, kind)?;
        // The ramps take at least a tick: two points cannot share a time.
        let a = inner.start.max(Tick::ZERO);
        let b = inner.end().max(a);
        let outer_start = outer.start.min(a - Tick(1)).max(Tick::ZERO);
        let outer_end = outer.end().max(b + Tick(1));
        let edges = [outer_start, a, b, outer_end];
        // Each new point lies on the line as it was.
        let values: Vec<f32> = edges.iter().map(|&t| env.value_at(t)).collect();
        for (&at, &value) in edges.iter().zip(&values) {
            if env.points.iter().any(|p| p.at == at) {
                continue;
            }
            let curve = env
                .points
                .iter()
                .rev()
                .find(|p| p.at < at)
                .map_or(KeyCurve::Linear, |p| p.curve);
            env.points.push(EnvelopePoint { at, value, curve });
        }
        env.points.sort_by_key(|p| p.at);
        let (lo, hi) = kind.range();
        for p in env.points.iter_mut().filter(|p| p.at >= a && p.at <= b) {
            p.value = kind.from_unit(kind.to_unit(p.value) + delta).clamp(lo, hi);
        }
        Ok(())
    }

    /// Removes point `index` (the first point cannot be removed).
    pub fn remove_envelope_point(
        &mut self,
        owner: impl Into<EnvelopeOwner>,
        kind: EnvelopeKind,
        index: usize,
    ) -> Result<()> {
        let env = self.envelope_mut(owner, kind)?;
        if index > 0 && index < env.points.len() {
            env.points.remove(index);
        }
        Ok(())
    }

    pub fn set_envelope_curve(
        &mut self,
        owner: impl Into<EnvelopeOwner>,
        kind: EnvelopeKind,
        index: usize,
        curve: KeyCurve,
    ) -> Result<()> {
        let env = self.envelope_mut(owner, kind)?;
        if let Some(p) = env.points.get_mut(index) {
            p.curve = curve;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: i64) -> Tick {
        Tick::from_seconds(x)
    }

    /// A pan envelope drawn as a ramp of many points, from left to right.
    fn ramp(p: &mut Project, a: TrackId) {
        p.toggle_envelope(a, EnvelopeKind::Pan).unwrap();
        for k in 0..=10 {
            p.set_envelope_point(a, EnvelopeKind::Pan, s(k), -1.0 + k as f32 * 0.2)
                .unwrap();
        }
    }

    fn pan(p: &Project, a: TrackId) -> &Envelope {
        p.track(a)
            .unwrap()
            .envelopes
            .iter()
            .find(|e| e.kind == EnvelopeKind::Pan)
            .unwrap()
    }

    #[test]
    fn thinning_leaves_the_line_and_its_ends() {
        let mut p = Project::default();
        let a = p.tracks[1].id;
        ramp(&mut p, a);
        // Within 3 to 7 s only the ends of that part stay.
        let gone = p.thin_envelope(
            a,
            EnvelopeKind::Pan,
            Some(TimeRange::from_bounds(s(3), s(7))),
        );
        assert_eq!(gone.unwrap(), 3);
        assert_eq!(pan(&p, a).points.len(), 8);
        let gone = p.thin_envelope(a, EnvelopeKind::Pan, None).unwrap();
        assert_eq!(gone, 6);
        let e = pan(&p, a);
        assert_eq!(e.points.len(), 2);
        assert!((e.value_at(s(5)) - 0.0).abs() < 1e-5);
    }

    #[test]
    fn flipping_turns_pan_around_and_keeps_the_selection_edges() {
        let mut p = Project::default();
        let a = p.tracks[1].id;
        ramp(&mut p, a);
        let mut all = p.clone();
        all.flip_envelope(a, EnvelopeKind::Pan, None).unwrap();
        assert!((pan(&all, a).value_at(s(0)) - 1.0).abs() < 1e-5);
        assert!((pan(&all, a).value_at(s(10)) + 1.0).abs() < 1e-5);
        // A part flipped: outside it, the line is what it was.
        let half = Tick::from_seconds_f64(2.5);
        p.flip_envelope(
            a,
            EnvelopeKind::Pan,
            Some(TimeRange::from_bounds(half, s(4))),
        )
        .unwrap();
        assert!((pan(&p, a).value_at(s(1)) + 0.8).abs() < 1e-5);
        assert!((pan(&p, a).value_at(s(3)) - 0.4).abs() < 1e-5);
        assert!((pan(&p, a).value_at(s(6)) - 0.2).abs() < 1e-5);
        // Volume flips on its dB scale; mute does not flip.
        assert!(p.flip_envelope(a, EnvelopeKind::Mute, None).is_err());
    }

    #[test]
    fn locked_envelopes_go_with_the_events() {
        use crate::model::{Generator, MediaInfo};
        let mut p = Project::default();
        let a = p.tracks[1].id;
        let tone = p.add_media(MediaInfo::generated(
            Generator::TestTone {
                frequency_hz: 440.0,
                level_db: -12.0,
            },
            &p.settings.clone(),
        ));
        let id = p.insert_media(tone, s(2), None, Some(a)).unwrap()[0];
        p.trim_end(id, s(4)).unwrap();
        p.toggle_envelope(a, EnvelopeKind::Volume).unwrap();
        p.set_envelope_point(a, EnvelopeKind::Volume, s(3), 0.5)
            .unwrap();
        p.set_envelope_point(a, EnvelopeKind::Volume, s(11), 0.25)
            .unwrap();
        let before = p.clone();
        p.move_clip(id, s(10), None).unwrap();
        p.carry_envelopes(&before, &[id]);
        let e = p.track(a).unwrap().envelopes[0].clone();
        let at: Vec<Tick> = e.points.iter().map(|q| q.at).collect();
        // The point under the event went 8 s later; the one where it
        // landed gave way.
        assert_eq!(at, vec![Tick::ZERO, s(11)]);
        assert!((e.value_at(s(11)) - 0.5).abs() < 1e-6);
    }

    /// A video event 10 s long at 25 s, 20 s into a minute of media.
    fn event(p: &mut Project) -> ClipId {
        use crate::model::{MediaId, MediaInfo, MediaSource, VideoStreamInfo};
        let m = p.add_media(MediaInfo {
            id: MediaId(0),
            source: MediaSource::File {
                path: "clip.mp4".into(),
            },
            name: "clip".into(),
            duration: s(60),
            video: Some(VideoStreamInfo {
                width: 1920,
                height: 1080,
                frame_rate: bv_core::FrameRate::FPS_30,
                pixel_aspect: Default::default(),
                field_order: Default::default(),
                spherical: false,
                full_range: false,
                reading: Default::default(),
            }),
            audio: None,
            extra_audio: Vec::new(),
            sequence: None,
            still: false,
            effects: Vec::new(),
            markers: Vec::new(),
            color_space: None,
            reel: Default::default(),
            layer: None,
        });
        let v = p.tracks[0].id;
        let id = p.insert_media(m, s(5), Some(v), None).unwrap()[0];
        p.trim_start(id, s(25)).unwrap();
        p.trim_end(id, s(35)).unwrap();
        let c = p.clip(id).unwrap();
        assert_eq!((c.start, c.duration, c.source_in), (s(25), s(10), s(20)));
        id
    }

    fn secs(x: f64) -> Tick {
        Tick::from_seconds_f64(x)
    }

    /// The media time event `id` shows `x` seconds after 25 s.
    fn shown(p: &Project, id: ClipId, x: f64) -> f64 {
        p.clip(id)
            .unwrap()
            .source_time(s(25) + secs(x))
            .as_seconds_f64()
    }

    #[test]
    fn velocity_changes_the_speed_and_plays_backward_to_the_first_frame() {
        let mut p = Project::default();
        let id = event(&mut p);
        let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
        assert!(p.toggle_event_envelope(id, EnvelopeKind::Velocity).unwrap());
        assert!(near(shown(&p, id, 2.0), 22.0), "100 %: as before");
        p.set_envelope_point(id, EnvelopeKind::Velocity, Tick::ZERO, 0.5)
            .unwrap();
        assert!(near(shown(&p, id, 2.0), 21.0), "50 %: half as far");
        // −100 %: backward, then the first frame held.
        p.set_envelope_point(id, EnvelopeKind::Velocity, Tick::ZERO, -1.0)
            .unwrap();
        assert!(near(shown(&p, id, 5.0), 15.0));
        p.clip_mut(id).unwrap().source_in = s(3);
        assert!(near(shown(&p, id, 2.0), 1.0));
        assert!(near(shown(&p, id, 5.0), 0.0), "{}", shown(&p, id, 5.0));
        // From −100 % to 100 % over 2 s, a quarter second in: back to the
        // first frame, held there until the speed turns, then forward.
        p.clip_mut(id).unwrap().source_in = secs(0.25);
        p.set_envelope_point(id, EnvelopeKind::Velocity, s(2), 1.0)
            .unwrap();
        assert!(near(shown(&p, id, 2.0), 0.5), "{}", shown(&p, id, 2.0));
        assert!(near(shown(&p, id, 3.0), 1.5));
        // A curve: Smooth from 0 to 200 % over 2 s goes as far as the line.
        p.clip_mut(id).unwrap().source_in = s(20);
        p.set_envelope_point(id, EnvelopeKind::Velocity, Tick::ZERO, 0.0)
            .unwrap();
        p.set_envelope_point(id, EnvelopeKind::Velocity, s(2), 2.0)
            .unwrap();
        p.set_envelope_curve(id, EnvelopeKind::Velocity, 0, KeyCurve::Smooth)
            .unwrap();
        assert!(near(shown(&p, id, 2.0), 22.0), "{}", shown(&p, id, 2.0));
        // Points stay within the event; the envelope goes as it came.
        p.set_envelope_point(id, EnvelopeKind::Velocity, s(30), 1.0)
            .unwrap();
        let c = p.clip(id).unwrap();
        assert_eq!(c.velocity().unwrap().points.last().unwrap().at, s(10));
        assert!(!p.toggle_event_envelope(id, EnvelopeKind::Velocity).unwrap());
        assert!(p.clip(id).unwrap().velocity().is_none());
    }

    #[test]
    fn freeze_frame_at_the_cursor_holds_the_frame_there() {
        let mut p = Project::default();
        let id = event(&mut p);
        p.freeze_frame_at(id, s(28)).unwrap();
        for (x, at) in [(2.0, 22.0), (3.0, 23.0), (5.0, 23.0), (9.5, 23.0)] {
            assert!(
                (shown(&p, id, x) - at).abs() < 1e-6,
                "{x}: {}",
                shown(&p, id, x)
            );
        }
        let env = p.clip(id).unwrap().velocity().unwrap();
        assert_eq!(env.points.len(), 2, "held, then 0");
        assert_eq!(env.points[0].curve, KeyCurve::Hold);
        // Earlier on a ramp: the ramp kept up to there.
        let mut q = Project::default();
        let id = event(&mut q);
        q.toggle_event_envelope(id, EnvelopeKind::Velocity).unwrap();
        q.set_envelope_point(id, EnvelopeKind::Velocity, s(4), 2.0)
            .unwrap();
        let before = shown(&q, id, 2.0);
        q.freeze_frame_at(id, s(27)).unwrap();
        assert!((shown(&q, id, 2.0) - before).abs() < 1e-6);
        assert!((shown(&q, id, 8.0) - before).abs() < 1e-6);
        assert!(q.freeze_frame_at(id, s(40)).is_err(), "outside the event");
    }

    #[test]
    fn splits_and_trims_keep_what_velocity_shows() {
        let mut p = Project::default();
        let id = event(&mut p);
        p.toggle_event_envelope(id, EnvelopeKind::Velocity).unwrap();
        p.set_envelope_point(id, EnvelopeKind::Velocity, Tick::ZERO, 0.5)
            .unwrap();
        p.set_envelope_point(id, EnvelopeKind::Velocity, s(4), 2.0)
            .unwrap();
        p.set_envelope_point(id, EnvelopeKind::Velocity, s(8), -0.5)
            .unwrap();
        let times: Vec<f64> = (0..20).map(|k| k as f64 * 0.5).collect();
        let before: Vec<f64> = times.iter().map(|&x| shown(&p, id, x)).collect();
        let right = p.split(id, secs(29.5)).unwrap()[0];
        for (&x, &b) in times.iter().zip(&before) {
            let now = if x < 4.5 {
                shown(&p, id, x)
            } else {
                shown(&p, right, x)
            };
            assert!((now - b).abs() < 1e-6, "{x}: {now} for {b}");
        }
        p.trim_start(right, s(31)).unwrap();
        for (&x, &b) in times.iter().zip(&before).filter(|(x, _)| **x >= 6.0) {
            assert!((shown(&p, right, x) - b).abs() < 1e-6, "{x}");
        }
    }

    #[test]
    fn reversed_events_play_their_part_backward() {
        let mut p = Project::default();
        let id = event(&mut p);
        let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
        p.reverse_event(id).unwrap();
        // 20 to 30 s of the media, from 30 s down.
        assert!(near(shown(&p, id, 0.0), 30.0));
        assert!(near(shown(&p, id, 4.0), 26.0));
        assert!(near(shown(&p, id, 10.0), 20.0));
        // Trims and splits keep what it shows.
        let right = p.split(id, s(29)).unwrap()[0];
        assert!(near(shown(&p, right, 6.0), 24.0));
        p.trim_start(id, s(27)).unwrap();
        assert!(near(shown(&p, id, 3.0), 27.0));
        // Forward again over the part it showed (28 down to 26 s, at 27 to
        // 29 s).
        p.reverse_event(id).unwrap();
        assert!(p.clip(id).unwrap().reverse.is_none());
        assert!(near(shown(&p, id, 2.0), 26.0), "{}", shown(&p, id, 2.0));
        assert!(near(shown(&p, id, 3.5), 27.5));
    }

    #[test]
    fn values_interpolate_and_hold() {
        let mut p = Project::default();
        let a = p.tracks[1].id;
        assert!(p.toggle_envelope(a, EnvelopeKind::Volume).unwrap());
        p.set_envelope_point(a, EnvelopeKind::Volume, s(2), 0.0)
            .unwrap();
        let env = &p.track(a).unwrap().envelopes[0];
        assert_eq!(env.value_at(Tick::ZERO), 1.0);
        assert!((env.value_at(s(1)) - 0.5).abs() < 1e-4, "linear ramp");
        assert_eq!(env.value_at(s(5)), 0.0);
        // Mute envelopes always jump.
        p.toggle_envelope(a, EnvelopeKind::Mute).unwrap();
        p.set_envelope_point(a, EnvelopeKind::Mute, s(2), 0.0)
            .unwrap();
        let mute = p
            .track(a)
            .unwrap()
            .envelopes
            .iter()
            .find(|e| e.kind == EnvelopeKind::Mute)
            .unwrap();
        assert_eq!(mute.value_at(s(1)), 1.0);
        assert_eq!(mute.value_at(s(2)), 0.0);
        // Video tracks only take the mute envelope.
        let v = p.tracks[0].id;
        assert!(p.toggle_envelope(v, EnvelopeKind::Pan).is_err());
        // Toggling again removes it.
        assert!(!p.toggle_envelope(a, EnvelopeKind::Volume).unwrap());
    }

    #[test]
    fn points_stay_ordered() {
        let mut p = Project::default();
        let a = p.tracks[1].id;
        p.toggle_envelope(a, EnvelopeKind::Pan).unwrap();
        let i = p
            .set_envelope_point(a, EnvelopeKind::Pan, s(4), 1.0)
            .unwrap();
        p.set_envelope_point(a, EnvelopeKind::Pan, s(8), -1.0)
            .unwrap();
        p.move_envelope_point(a, EnvelopeKind::Pan, i, s(20), 5.0)
            .unwrap();
        let env = &p.track(a).unwrap().envelopes[0];
        assert!(
            env.points[1].at < env.points[2].at,
            "cannot pass its neighbour"
        );
        assert_eq!(env.points[1].value, 1.0, "clamped to the range");
        p.remove_envelope_point(a, EnvelopeKind::Pan, 0).unwrap();
        assert_eq!(p.track(a).unwrap().envelopes[0].points.len(), 3);
    }

    #[test]
    fn unit_mapping_round_trips() {
        for v in [0.0f32, 0.1, 0.5, 1.0, 2.0] {
            let k = EnvelopeKind::Volume;
            assert!((k.from_unit(k.to_unit(v)) - v).abs() < 1e-3, "{v}");
        }
        // 0 dB in the middle, +6 dB at the top, as VEGAS draws them.
        assert_eq!(EnvelopeKind::Volume.to_unit(1.0), 0.5);
        assert_eq!(EnvelopeKind::Volume.to_unit(MAX_VOLUME), 1.0);
        assert_eq!(EnvelopeKind::Volume.from_unit(1.0), VOLUME_ENVELOPE_TOP);
    }

    #[test]
    fn a_part_lifted_with_ramps_to_its_edges() {
        let mut p = Project::default();
        let a = p.tracks[1].id;
        let k = EnvelopeKind::Volume;
        p.toggle_envelope(a, k).unwrap();
        // 2–4 s brought down to half, with ramps from 1.5 s and to 4.5 s.
        let half = Tick(s(1).0 / 2);
        let outer = TimeRange::from_bounds(s(2) - half, s(4) + half);
        let inner = TimeRange::from_bounds(s(2), s(4));
        let down = k.to_unit(0.5) - k.to_unit(1.0);
        p.lift_envelope(a, k, outer, inner, down).unwrap();
        let env = &p.track(a).unwrap().envelopes[0];
        assert_eq!(env.points.len(), 5, "the first point and the four edges");
        assert!((env.value_at(s(3)) - 0.5).abs() < 1e-3);
        assert_eq!((env.value_at(s(1)), env.value_at(s(5))), (1.0, 1.0));
        let ramp = env.value_at(s(2) - Tick(half.0 / 2));
        assert!(ramp < 1.0 && ramp > 0.5, "{ramp}");
        // Lifted again: its points are there already and move on.
        let before = p.clone();
        p.lift_envelope(a, k, outer, inner, 2.0 * down).unwrap();
        let env = &p.track(a).unwrap().envelopes[0];
        assert_eq!(env.points.len(), 5);
        assert!(env.value_at(s(3)) < before.track(a).unwrap().envelopes[0].value_at(s(3)));
    }

    /// Recorded automation on a bus track's envelope (volume, pan, mute
    /// only), and on a send's, which comes with its send at 0 dB.
    #[test]
    fn automation_is_written_to_bus_tracks_and_sends() {
        let mut p = Project::default();
        let a = p.tracks[1].id;
        let bus = p.add_bus().unwrap();
        let strip = Strip::Bus(bus);
        let owner = EnvelopeOwner::Strip(strip);
        let samples = [(s(1), 0.5), (s(2), 0.25)];
        p.write_automation_to(
            owner,
            EnvelopeKind::Volume,
            &samples,
            false,
            KeyCurve::Linear,
        )
        .unwrap();
        let env = &p.strip_channel(strip).unwrap().envelopes[0];
        assert!((env.value_at(s(2)) - 0.25).abs() < 1e-6);
        assert_eq!(env.value_at(Tick::ZERO), 1.0);
        let level = EnvelopeKind::CompositeLevel;
        assert!(
            p.write_automation_to(owner, level, &samples, false, KeyCurve::Linear)
                .is_err()
        );
        let kind = EnvelopeKind::Send(SendTo::Bus(bus));
        p.write_automation(a, kind, &samples, false, KeyCurve::Linear)
            .unwrap();
        let t = p.track(a).unwrap();
        assert!(
            t.sends
                .iter()
                .any(|x| x.to == SendTo::Bus(bus) && x.gain_db == 0.0)
        );
        assert!(t.envelopes.iter().any(|e| e.kind == kind));
    }

    #[test]
    fn recorded_automation_replaces_its_span_and_ramps_back() {
        let mut p = Project::default();
        let a = p.tracks[1].id;
        let k = EnvelopeKind::Pan;
        // An envelope with a point at 3 s inside what gets recorded, and one
        // at 9 s after it.
        p.toggle_envelope_as(a, k, KeyCurve::Linear).unwrap();
        p.set_envelope_point(a, k, s(3), 0.5).unwrap();
        p.set_envelope_point(a, k, s(9), -0.5).unwrap();
        let samples: Vec<(Tick, f32)> = (0..=20)
            .map(|i| (s(2) + Tick(s(1).0 / 10 * i), -1.0))
            .collect();
        let before = p.track(a).unwrap().envelopes[0].value_at(s(6));
        p.write_automation(a, k, &samples, true, KeyCurve::Linear)
            .unwrap();
        let env = &p.track(a).unwrap().envelopes[0];
        assert_eq!(env.value_at(s(3)), -1.0, "the old point is gone");
        assert_eq!(env.value_at(s(4)), -1.0);
        // A flat line thins to its two ends (plus the ramps and the first
        // point), and after it the envelope goes on as it was.
        assert_eq!(env.points.len(), 6, "{:?}", env.points);
        assert!((env.value_at(s(6)) - before).abs() < 0.05);
        assert_eq!(env.value_at(s(9)), -0.5);
        // A track without the envelope gets one.
        let v = p.tracks[0].id;
        let ok = p.write_automation(
            v,
            EnvelopeKind::CompositeLevel,
            &samples,
            false,
            KeyCurve::Smooth,
        );
        assert!(ok.is_ok());
        let env = p
            .track(v)
            .unwrap()
            .envelopes
            .iter()
            .find(|e| e.kind == EnvelopeKind::CompositeLevel);
        assert_eq!(env.map(|e| e.points[0].curve), Some(KeyCurve::Smooth));
    }
}
