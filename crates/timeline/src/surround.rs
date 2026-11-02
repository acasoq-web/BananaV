//! The Surround Panner (View › Window › Surround Panner), as VEGAS's Help
//! describes it: in 5.1 projects a track's — or a bus's, a chain's — pan
//! point in the room, −1 left to 1 right and −1 back to 1 front, the
//! middle of the room (0, 0) where it starts; the Center channel's trim;
//! LFE Only; each speaker muted or soloed; and its Surround Pan Keyframes,
//! the point over time (temporal curves as an envelope's, Smoothness
//! bending the path between them), the trim point added to them.
//!
//! The gains follow the levels the Help's pictures write at each speaker
//! (what a sound the same on both channels plays there) until listening
//! tests measure VEGAS's. Add Channels and the Balance types (every
//! speaker at −6 dB with the point in the middle): the front row (L, C,
//! R) and the back row (Ls, Rs) share the sound in proportion to the
//! point's front-back place; within a row the pair's left and right
//! channels are placed by the pan type at its left-right place, as its
//! stereo pan places them, the center getting their mean, whole in the
//! middle and nothing at the sides. Constant Power and Film pan a point
//! source — the pair as one, each channel at a quarter —: between the
//! front row's neighbouring speakers and the back row's by cosine and
//! sine, the rows too (the pictures: −7.4 dB from the center alone near
//! the front's middle, L and R silent; −15.8, −9.0 and −10.1 dB at the
//! center and the surrounds farther back, to a few tenths of a dB).

use bv_core::{ChannelLayout, Tick};
use serde::{Deserialize, Serialize};

use crate::busses::{Output, Strip};
use crate::fx::KeyCurve;
use crate::model::{PanMode, Project, TrackId, TrackKind};

/// 5.1's channels, as the mixer lays them out.
pub const L: usize = 0;
pub const R: usize = 1;
pub const C: usize = 2;
pub const LFE: usize = 3;
pub const LS: usize = 4;
pub const RS: usize = 5;

/// A Surround Pan Keyframe: the point at a time, and how it moves on to
/// the next.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurroundKey {
    pub at: Tick,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub curve: KeyCurve,
}

/// A track's (a bus's, a chain's) surround panning.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurroundPan {
    /// The pan point (its trim with keyframes): −1 left … 1 right, −1
    /// back … 1 front.
    pub x: f32,
    pub y: f32,
    /// The Center channel's trim (dB).
    #[serde(default)]
    pub center_db: f32,
    /// LFE Only: everything into the LFE channel.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lfe_only: bool,
    /// Each speaker muted or soloed (5.1's order: L, R, C, LFE, Ls, Rs).
    #[serde(default)]
    pub mute: [bool; 6],
    #[serde(default)]
    pub solo: [bool; 6],
    /// How curved the path between keyframes is: 0 straight.
    #[serde(default)]
    pub smoothness: f32,
    /// Surround Pan Keyframes (Insert › Audio Envelopes): the point over
    /// time, sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<SurroundKey>,
    /// Whether the keyframes' row shows under the track.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keyframes: bool,
}

/// A panner as it starts — and what a track, a bus or a chain never
/// panned plays as —: the point in the middle of the room (as the Help's
/// pictures show it), nothing muted, no keyframes.
pub static MIDDLE: SurroundPan = SurroundPan {
    x: 0.0,
    y: 0.0,
    center_db: 0.0,
    lfe_only: false,
    mute: [false; 6],
    solo: [false; 6],
    smoothness: 0.0,
    keys: Vec::new(),
    keyframes: false,
};

impl Default for SurroundPan {
    fn default() -> SurroundPan {
        MIDDLE.clone()
    }
}

impl SurroundPan {
    /// The point its keyframes give at `t` (none: the middle of the room,
    /// where the trim then places it).
    pub fn keyed_at(&self, t: Tick) -> [f32; 2] {
        let k = &self.keys;
        let Some(first) = k.first() else {
            return [0.0, 0.0];
        };
        if t <= first.at {
            return [first.x, first.y];
        }
        let Some(i) = k.windows(2).position(|w| t < w[1].at) else {
            let last = k[k.len() - 1];
            return [last.x, last.y];
        };
        let (a, b) = (k[i], k[i + 1]);
        let span = (b.at - a.at).0.max(1) as f32;
        let f = (t - a.at).0 as f32 / span;
        let f = a.curve.eval(f);
        // Smoothness bends the path: a Catmull-Rom curve through the
        // keyframes around, mixed in as far as it says.
        let p = |j: usize| [k[j].x, k[j].y];
        let (p0, p1, p2, p3) = (
            p(i.saturating_sub(1)),
            p(i),
            p(i + 1),
            p((i + 2).min(k.len() - 1)),
        );
        let straight = [p1[0] + (p2[0] - p1[0]) * f, p1[1] + (p2[1] - p1[1]) * f];
        let s = self.smoothness.clamp(0.0, 1.0);
        if s <= 0.0 {
            return straight;
        }
        let cr = |a: f32, b: f32, c: f32, d: f32| {
            let f2 = f * f;
            let f3 = f2 * f;
            0.5 * (2.0 * b
                + (-a + c) * f
                + (2.0 * a - 5.0 * b + 4.0 * c - d) * f2
                + (-a + 3.0 * b - 3.0 * c + d) * f3)
        };
        let curved = [
            cr(p0[0], p1[0], p2[0], p3[0]),
            cr(p0[1], p1[1], p2[1], p3[1]),
        ];
        [
            straight[0] + (curved[0] - straight[0]) * s,
            straight[1] + (curved[1] - straight[1]) * s,
        ]
    }

    /// The trim's point alone (its keyframes not playing), within the
    /// room.
    pub fn trim_point(&self) -> [f32; 2] {
        [self.x.clamp(-1.0, 1.0), self.y.clamp(-1.0, 1.0)]
    }

    /// The pan point at `t`: its keyframes' plus the trim, within the
    /// room.
    pub fn point_at(&self, t: Tick) -> [f32; 2] {
        let k = self.keyed_at(t);
        [
            (k[0] + self.x).clamp(-1.0, 1.0),
            (k[1] + self.y).clamp(-1.0, 1.0),
        ]
    }

    /// Each of 5.1's channels' gains from the pair's left and right
    /// channels with the point at `[x, y]`, by pan type `mode`.
    pub fn gains(&self, mode: PanMode, [x, y]: [f32; 2]) -> [[f32; 2]; 6] {
        let mut g = [[0.0f32; 2]; 6];
        let x = x.clamp(-1.0, 1.0);
        let center_trim = db_to_gain(self.center_db);
        if self.lfe_only {
            g[LFE] = [0.5, 0.5];
        } else if matches!(mode, PanMode::ConstantPower | PanMode::Film) {
            // A point source between neighbouring speakers.
            let (front, back) = rows(mode, y);
            let between = |t: f32| {
                let a = t.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
                (a.cos(), a.sin())
            };
            let (l, c, r) = if x <= 0.0 {
                let (a, b) = between(x + 1.0);
                (a, b, 0.0)
            } else {
                let (a, b) = between(x);
                (0.0, a, b)
            };
            let (ls, rs) = between((x + 1.0) / 2.0);
            for (ch, v) in [
                (L, l * front),
                (C, c * front * center_trim),
                (R, r * front),
                (LS, ls * back),
                (RS, rs * back),
            ] {
                g[ch] = [0.25 * v, 0.25 * v];
            }
        } else {
            let (front, back) = rows(mode, y);
            let m = mode.matrix(x);
            for (c, k) in [(L, 0), (R, 1)] {
                g[c] = [m[k][0] * front, m[k][1] * front];
            }
            for (c, k) in [(LS, 0), (RS, 1)] {
                g[c] = [m[k][0] * back, m[k][1] * back];
            }
            let center = (1.0 - x.abs()) * front * center_trim;
            g[C] = [0.5 * center, 0.5 * center];
        }
        let soloed = self.solo.iter().any(|s| *s);
        for (c, gc) in g.iter_mut().enumerate() {
            if self.mute[c] || (soloed && !self.solo[c]) {
                *gc = [0.0, 0.0];
            }
        }
        g
    }

    /// Insert › Audio Envelopes › Surround Pan Keyframes: the row shown
    /// and, when it has none, a keyframe at the project's start holding
    /// the panning as it is (the trim back in the middle: nothing moves).
    pub fn insert_keyframes(&mut self) {
        self.keyframes = true;
        if self.keys.is_empty() {
            self.keys.push(SurroundKey {
                at: Tick::ZERO,
                x: self.x,
                y: self.y,
                curve: KeyCurve::Linear,
            });
            self.x = 0.0;
            self.y = 0.0;
        }
    }

    /// The row and its keyframes taken away, the panning at the first one
    /// kept as the trim.
    pub fn remove_keyframes(&mut self) {
        if !self.keys.is_empty() {
            [self.x, self.y] = self.point_at(Tick::ZERO);
        }
        self.keys.clear();
        self.keyframes = false;
    }

    /// Keyframe `i` moved to `at` (none moves onto another); its new index.
    pub fn move_key(&mut self, i: usize, at: Tick) -> usize {
        let Some(k) = self.keys.get(i).copied() else {
            return i;
        };
        let at = at.max(Tick::ZERO);
        if self
            .keys
            .iter()
            .enumerate()
            .any(|(j, o)| j != i && o.at == at)
        {
            return i;
        }
        self.keys[i].at = at;
        self.keys.sort_by_key(|k| k.at);
        self.keys
            .iter()
            .position(|o| o.at == at && o.x == k.x && o.y == k.y)
            .unwrap_or(i)
    }

    /// A copy of keyframe `i` at `at` (Ctrl+drag); its index.
    pub fn copy_key(&mut self, i: usize, at: Tick) -> Option<usize> {
        let k = *self.keys.get(i)?;
        let at = at.max(Tick::ZERO);
        if self.keys.iter().any(|o| o.at == at) {
            return None;
        }
        self.keys.push(SurroundKey { at, ..k });
        self.keys.sort_by_key(|k| k.at);
        self.keys.iter().position(|o| o.at == at)
    }

    /// A keyframe added at `at` (a double-click on the row): the panning
    /// there, the curve of the keyframe before.
    pub fn add_key(&mut self, at: Tick) -> usize {
        let [x, y] = self.keyed_at(at);
        self.set_key(at, [x, y])
    }

    /// The keyframe at `at` showing the pan point `[x, y]` (the trim taken
    /// off), made when there is none; its index.
    pub fn set_point_at(&mut self, at: Tick, [x, y]: [f32; 2]) -> usize {
        let k = [(x - self.x).clamp(-2.0, 2.0), (y - self.y).clamp(-2.0, 2.0)];
        self.set_key(at, k)
    }

    /// A keyframe at `at` with the point `[x, y]` (replacing one there),
    /// its curve the one before's; the keyframes' index.
    pub fn set_key(&mut self, at: Tick, [x, y]: [f32; 2]) -> usize {
        let at = at.max(Tick::ZERO);
        if let Some(i) = self.keys.iter().position(|k| k.at == at) {
            self.keys[i].x = x;
            self.keys[i].y = y;
            return i;
        }
        let curve = self
            .keys
            .iter()
            .rev()
            .find(|k| k.at < at)
            .map_or(KeyCurve::Linear, |k| k.curve);
        self.keys.push(SurroundKey { at, x, y, curve });
        self.keys.sort_by_key(|k| k.at);
        self.keys.iter().position(|k| k.at == at).unwrap_or(0)
    }
}

/// Whose Surround Panner: a track's, or a bus's or a chain's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurroundOwner {
    Track(TrackId),
    Strip(Strip),
}

impl Project {
    /// `owner`'s Surround Panner (`None`: never panned, or no such owner).
    pub fn surround(&self, owner: SurroundOwner) -> Option<&SurroundPan> {
        match owner {
            SurroundOwner::Track(t) => self.track(t)?.surround.as_ref(),
            SurroundOwner::Strip(s) => self.strip_channel(s)?.surround.as_ref(),
        }
    }

    /// `owner`'s Surround Panner, made (in the middle of the room) when it
    /// had none.
    pub fn surround_mut(&mut self, owner: SurroundOwner) -> Option<&mut SurroundPan> {
        let slot = match owner {
            SurroundOwner::Track(t) => {
                let track = self.track_by_id_mut(t)?;
                if track.kind != TrackKind::Audio {
                    return None;
                }
                &mut track.surround
            }
            SurroundOwner::Strip(s) => &mut self.strip_channel_mut(s)?.surround,
        };
        Some(slot.get_or_insert_with(SurroundPan::default))
    }

    /// Whether `owner` is panned in the room: in 5.1 projects, an audio
    /// track, a bus or a chain that plays into the Master (the Surround
    /// Master; into a bus it goes stereo, and the bus pans).
    pub fn surround_pans(&self, owner: SurroundOwner) -> bool {
        if self.settings.channels != ChannelLayout::Surround51 {
            return false;
        }
        match owner {
            SurroundOwner::Track(t) => self
                .track(t)
                .is_some_and(|t| t.kind == TrackKind::Audio && t.output == Output::Master),
            SurroundOwner::Strip(s) => self
                .strip_channel(s)
                .is_some_and(|c| c.output == Output::Master),
        }
    }

    /// `owner`'s pan type (busses and chains have none of their own: Add
    /// Channels).
    pub fn surround_pan_mode(&self, owner: SurroundOwner) -> PanMode {
        match owner {
            SurroundOwner::Track(t) => self.track(t).map_or(PanMode::default(), |t| t.pan_mode),
            SurroundOwner::Strip(_) => PanMode::default(),
        }
    }
}

/// The front row's and the back row's shares at front-back place `y`.
fn rows(mode: PanMode, y: f32) -> (f32, f32) {
    let y = y.clamp(-1.0, 1.0);
    match mode {
        PanMode::ConstantPower | PanMode::Film => {
            let a = (1.0 - y) * std::f32::consts::FRAC_PI_4;
            (a.cos(), a.sin())
        }
        _ => ((1.0 + y) / 2.0, (1.0 - y) / 2.0),
    }
}

fn db_to_gain(db: f32) -> f32 {
    if db <= -96.0 {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mono sound (the same on both channels) from each speaker.
    fn mono(g: &[[f32; 2]; 6]) -> [f32; 6] {
        g.map(|c| c[0] + c[1])
    }

    fn db(v: f32) -> f32 {
        20.0 * v.log10()
    }

    /// In the middle of the room every speaker plays at −6 dB (the Help's
    /// picture, Add Channels); in front only the front row; a side's
    /// speakers alone at that side.
    #[test]
    fn the_point_shares_the_sound_among_the_speakers() {
        let s = SurroundPan::default();
        let g = mono(&s.gains(PanMode::AddChannels, [0.0, 0.0]));
        for c in [L, R, C, LS, RS] {
            assert!((db(g[c]) + 6.02).abs() < 0.05, "{c}: {}", db(g[c]));
        }
        assert_eq!(g[LFE], 0.0);
        let g = mono(&s.gains(PanMode::AddChannels, [0.0, 1.0]));
        assert!((g[L] - 1.0).abs() < 1e-6 && (g[C] - 1.0).abs() < 1e-6);
        assert_eq!(g[LS], 0.0);
        // Hard right at the front: the right speaker alone.
        let g = mono(&s.gains(PanMode::ConstantPower, [1.0, 1.0]));
        assert!(g[L].abs() < 1e-3 && g[C].abs() < 1e-3 && (g[R] - 0.5).abs() < 1e-3);
        // The center's trim; LFE Only; a muted speaker; a soloed one.
        let mut t = SurroundPan {
            center_db: -6.0206,
            ..SurroundPan::default()
        };
        let g = mono(&t.gains(PanMode::AddChannels, [0.0, 1.0]));
        assert!((g[C] - 0.5).abs() < 1e-3);
        t.lfe_only = true;
        let g = mono(&t.gains(PanMode::AddChannels, [0.0, 1.0]));
        assert_eq!(g[LFE], 1.0);
        assert_eq!(g[L], 0.0);
        let mut u = SurroundPan::default();
        u.mute[C] = true;
        assert_eq!(mono(&u.gains(PanMode::AddChannels, [0.0, 0.0]))[C], 0.0);
        u.solo[LS] = true;
        let g = mono(&u.gains(PanMode::AddChannels, [0.0, 0.0]));
        assert!(g[LS] > 0.0 && g[L] == 0.0 && g[RS] == 0.0);
    }

    /// Constant Power as the Help's pictures write it: the point near the
    /// front's middle — the center alone in front, −7.4 dB; the surrounds
    /// −14.4 and −14.6 dB — and back a little to the left — −15.8 dB at
    /// the center, −9.0 and −10.1 dB at the surrounds, L silent (under
    /// −30 dB, which the window writes as −∞). The points are where the
    /// pictures draw them, to a pixel.
    #[test]
    fn constant_power_is_a_point_source_as_the_pictures_show() {
        let s = SurroundPan::default();
        let near = |g: f32, db_: f32| (db(g) - db_).abs() < 0.8;
        let g = mono(&s.gains(PanMode::ConstantPower, [0.005, 0.231]));
        assert!(near(g[C], -7.4), "{}", db(g[C]));
        assert!(near(g[LS], -14.4) && near(g[RS], -14.6), "{g:?}");
        assert!(db(g[L]) < -30.0 && db(g[R]) < -30.0);
        let g = mono(&s.gains(PanMode::ConstantPower, [-0.075, -0.60]));
        assert!(near(g[C], -15.8), "{}", db(g[C]));
        assert!(near(g[LS], -9.0) && near(g[RS], -10.1), "{g:?}");
        assert!(db(g[L]) < -30.0, "{}", db(g[L]));
        // Constant power: the speakers' powers add up the same anywhere.
        let power = |p: [f32; 2]| {
            mono(&s.gains(PanMode::ConstantPower, p))
                .iter()
                .map(|v| v * v)
                .sum::<f32>()
        };
        for p in [[0.0, 0.0], [0.7, -0.3], [-1.0, 1.0], [0.2, -1.0]] {
            assert!((power(p) - 0.25).abs() < 1e-4, "{p:?}");
        }
    }

    /// A stereo pair keeps its sides: in the middle its left channel feeds
    /// the left speakers, its right the right ones, the center both.
    #[test]
    fn a_stereo_pair_keeps_its_sides() {
        let g = SurroundPan::default().gains(PanMode::AddChannels, [0.0, 0.0]);
        assert_eq!(g[L], [0.5, 0.0]);
        assert_eq!(g[RS], [0.0, 0.5]);
        assert_eq!(g[C], [0.25, 0.25]);
    }

    /// Keyframes move the point over time (their curves, Hold holding),
    /// the trim added; Smoothness bends the path.
    #[test]
    fn keyframes_move_the_point() {
        let s = |x: i64| Tick::from_seconds(x);
        let mut p = SurroundPan::default();
        p.set_key(s(0), [-1.0, 0.5]);
        p.set_key(s(2), [1.0, 0.5]);
        let mid = p.point_at(s(1));
        assert!(mid[0].abs() < 1e-5 && (mid[1] - 0.5).abs() < 1e-5);
        p.keys[0].curve = KeyCurve::Hold;
        assert!((p.point_at(s(1))[0] + 1.0).abs() < 1e-5);
        p.keys[0].curve = KeyCurve::Linear;
        p.x = 0.5;
        assert!((p.point_at(s(1))[0] - 0.5).abs() < 1e-5);
        // A third keyframe at the back: with Smoothness the path from the
        // first to the second bends toward it.
        p.x = 0.0;
        p.set_key(s(4), [1.0, -1.0]);
        let straight = p.point_at(s(1));
        p.smoothness = 1.0;
        let bent = p.point_at(s(1));
        assert!(
            (bent[1] - straight[1]).abs() > 1e-3,
            "{straight:?} {bent:?}"
        );
        // A keyframe where one is: replaced.
        assert_eq!(p.set_key(s(2), [0.0, 0.0]), 1);
        assert_eq!(p.keys.len(), 3);
    }

    /// Inserting the keyframes keeps the panning (a first keyframe holds
    /// it, the trim back in the middle); removing them keeps it too.
    /// Keyframes move, copy, and never land on one another.
    #[test]
    fn the_keyframes_row_keeps_the_panning() {
        let s = |x: i64| Tick::from_seconds(x);
        let mut p = SurroundPan {
            x: 0.4,
            y: -0.2,
            ..SurroundPan::default()
        };
        p.insert_keyframes();
        assert!(p.keyframes && p.keys.len() == 1);
        assert_eq!(p.point_at(s(3)), [0.4, -0.2]);
        assert_eq!([p.x, p.y], [0.0, 0.0]);
        // The window's point at a time: a keyframe there, the trim off.
        p.x = 0.1;
        let i = p.set_point_at(s(2), [0.6, 0.6]);
        assert_eq!(i, 1);
        assert!((p.point_at(s(2))[0] - 0.6).abs() < 1e-6);
        assert!((p.keys[1].x - 0.5).abs() < 1e-6);
        // Moved past the first, copied, not onto another.
        assert_eq!(p.move_key(1, s(4)), 1);
        assert_eq!(p.copy_key(1, s(1)), Some(1));
        assert_eq!(p.keys.len(), 3);
        assert_eq!(p.copy_key(0, s(4)), None);
        assert_eq!(p.move_key(0, s(1)), 0);
        // A double-click on the row: the panning there.
        let k = p.add_key(s(3));
        assert_eq!(p.keys[k].at, s(3));
        p.remove_keyframes();
        assert!(!p.keyframes && p.keys.is_empty());
        assert!((p.x - 0.5).abs() < 1e-6 && (p.y + 0.2).abs() < 1e-6);
    }

    /// In 5.1 only what plays into the Master is panned in the room; a
    /// track's panner is made when first moved.
    #[test]
    fn only_what_plays_into_the_master_is_panned_in_the_room() {
        use crate::model::ProjectSettings;
        let mut p = Project::new(ProjectSettings::default());
        let a = p.tracks[1].id;
        let owner = SurroundOwner::Track(a);
        assert!(!p.surround_pans(owner));
        p.settings.channels = ChannelLayout::Surround51;
        assert!(p.surround_pans(owner));
        assert!(!p.surround_pans(SurroundOwner::Track(p.tracks[0].id)));
        assert!(p.surround(owner).is_none());
        p.surround_mut(owner).unwrap().y = 1.0;
        assert_eq!(p.surround(owner).unwrap().y, 1.0);
        let bus = p.add_bus().unwrap();
        p.set_track_output(a, Output::Bus(bus));
        assert!(!p.surround_pans(owner));
        assert!(p.surround_pans(SurroundOwner::Strip(Strip::Bus(bus))));
    }
}
