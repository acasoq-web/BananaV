//! Mending a project [`Project::check_invariants`] refuses. An edit may
//! have left a project that saved and then would not open, and a file may
//! come from elsewhere: [`Project::repair`] sets right what has a safe way
//! of being set right — as little as makes the project sound again — and
//! says what it did.

use std::collections::HashSet;

use bv_core::Tick;

use crate::busses::{BusId, FxChainId, MAX_BUSSES, MAX_FX_CHAINS, Output, Send, SendTo, Strip};
use crate::edit::{clamp_fades, start_at_media};
use crate::envelope::{Envelope, EnvelopeKind};
use crate::model::{Clip, ClipId, MediaId, Project, Track, TrackId};

const NUMBERS: &str = "Numbers that were not finite were set to their defaults.";
const RATES: &str = "Playback rates out of range were set within it.";
const STRIPS: &str =
    "Audio busses or effects chains listed twice, or past their number, were removed.";
const ROUTES: &str = "Audio routed to a bus that does not exist now plays into the Master bus.";
const SENDS: &str = "Sends to a bus or an effects chain they cannot reach were removed.";
const CIRCLES: &str = "Audio routing that played in a circle was cut.";
const SEND_ENVELOPES: &str = "Send envelopes without their send were given it back, or removed.";
const TRACK_GROUPS: &str = "Track groups were put back together.";
const IDS: &str = "Internal numbers of tracks and events were set right.";
const ORDER: &str = "Events out of order on their track were put in order.";
const NO_MEDIA: &str = "Events whose media is missing or cannot play on their track were removed.";
const TAKES: &str = "Takes whose media is missing or cannot play on their track were removed.";
const ACTIVE_TAKE: &str = "Events without a valid active take play their last take.";
const NO_LENGTH: &str = "Events with no length were removed.";
const BEFORE_ZERO: &str = "Events that started before the timeline's start were moved to it.";
const BEFORE_MEDIA: &str = "Events that started before their media without Loop now start with it.";
const FADES: &str = "Fades longer than their event were shortened.";

/// Every line [`Project::repair`] may return, in the order it returns
/// them (English: the interface translates them).
pub const MENDS: [&str; 17] = [
    NUMBERS,
    RATES,
    STRIPS,
    ROUTES,
    SENDS,
    CIRCLES,
    SEND_ENVELOPES,
    TRACK_GROUPS,
    IDS,
    ORDER,
    NO_MEDIA,
    TAKES,
    ACTIVE_TAKE,
    NO_LENGTH,
    BEFORE_ZERO,
    BEFORE_MEDIA,
    FADES,
];

/// The mends made to the events.
#[derive(Debug, Default)]
struct EventMends {
    no_media: bool,
    takes: bool,
    active_take: bool,
    no_length: bool,
    before_zero: bool,
    before_media: bool,
    fades: bool,
}

/// `value` made `default` when it is an infinity or no number; whether it
/// was.
fn finite(value: &mut f32, default: f32) -> bool {
    if value.is_finite() {
        return false;
    }
    *value = default;
    true
}

/// The envelopes' values that are no numbers made their kind's default.
fn finite_envelopes(envelopes: &mut [Envelope]) -> bool {
    let mut made = false;
    for e in envelopes {
        let default = e.kind.default_value();
        for p in &mut e.points {
            made |= finite(&mut p.value, default);
        }
    }
    made
}

/// The sends' levels that are no levels made no level (−∞, a level as
/// sends start with it: only +∞ and no number are not).
fn finite_sends(sends: &mut [Send]) -> bool {
    let mut made = false;
    for s in sends {
        if s.gain_db.is_nan() || s.gain_db == f32::INFINITY {
            s.gain_db = f32::NEG_INFINITY;
            made = true;
        }
        made |= finite(&mut s.held, 1.0);
    }
    made
}

/// Event `c`'s media and takes such as its track plays (`plays`: a media
/// of the project with a stream for the track): the takes that do not
/// play go, the active one giving its place to the first that plays.
/// Returns whether the event stays: not with nothing left to play.
fn mend_takes(c: &mut Clip, plays: &dyn Fn(MediaId) -> bool, made: &mut EventMends) -> bool {
    if c.takes.is_empty() {
        made.no_media |= !plays(c.media);
        return plays(c.media);
    }
    // The active take as it is read everywhere: the last one, for a
    // number past them.
    if c.active_take >= c.takes.len() {
        c.active_take = c.active_take();
        made.active_take = true;
    }
    if plays(c.media) && c.takes.iter().all(|t| plays(t.media)) {
        return true;
    }
    c.takes = c.take_list();
    if !plays(c.media) {
        let Some(k) = c.takes.iter().position(|t| !t.none && plays(t.media)) else {
            made.no_media = true;
            return false;
        };
        c.activate(k);
    }
    made.takes = true;
    let active = c.active_take;
    let (mut i, mut now) = (0, active);
    c.takes.retain(|t| {
        let keep = i == active || t.none || plays(t.media);
        if !keep && i < active {
            now -= 1;
        }
        i += 1;
        keep
    });
    c.active_take = now;
    // An empty take never plays: the event's media does for it.
    let media = c.media;
    for t in c.takes.iter_mut().filter(|t| t.none) {
        t.media = media;
    }
    // Its only take left, unnamed: an event without takes.
    if c.takes.len() == 1 && c.takes[0].name.is_none() {
        c.takes.clear();
        c.active_take = 0;
    }
    true
}

impl Project {
    /// Mends what [`Self::check_invariants`] refuses wherever a safe mend
    /// exists, and numbers that are not finite: what cannot be kept goes
    /// (an event whose media is missing, a send to a bus that is not
    /// there), what can is brought back within its limits (fades, an
    /// event's start, its rate). Returns a short line for each kind of
    /// mend made ([`MENDS`]; none: nothing was wrong). After it,
    /// `check_invariants` succeeds.
    pub fn repair(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        let mut say = |made: bool, line: &str| {
            if made {
                lines.push(line.to_string());
            }
        };
        let unordered = self.tracks.iter().any(|t| {
            t.clips
                .windows(2)
                .any(|w| (w[0].start, w[0].id) > (w[1].start, w[1].id))
        });
        say(self.mend_numbers(), NUMBERS);
        say(self.mend_rates(), RATES);
        say(self.mend_strip_lists(), STRIPS);
        say(self.mend_routes(), ROUTES);
        say(self.mend_sends(), SENDS);
        say(self.mend_circles(), CIRCLES);
        say(self.mend_send_envelopes(), SEND_ENVELOPES);
        say(self.mend_track_groups(), TRACK_GROUPS);
        say(self.mend_ids(), IDS);
        say(unordered, ORDER);
        let events = self.mend_events();
        say(events.no_media, NO_MEDIA);
        say(events.takes, TAKES);
        say(events.active_take, ACTIVE_TAKE);
        say(events.no_length, NO_LENGTH);
        say(events.before_zero, BEFORE_ZERO);
        say(events.before_media, BEFORE_MEDIA);
        say(events.fades, FADES);
        for t in &mut self.tracks {
            t.sort();
        }
        lines
    }

    /// Levels, pans and envelope values that are no numbers: their
    /// defaults.
    fn mend_numbers(&mut self) -> bool {
        let mut made = false;
        for t in &mut self.tracks {
            made |= finite(&mut t.level, 1.0);
            made |= finite(&mut t.pan, 0.0);
            made |= finite_envelopes(&mut t.envelopes);
            made |= finite_sends(&mut t.sends);
            for c in &mut t.clips {
                made |= finite(&mut c.level, 1.0);
                made |= finite(&mut c.pitch, 0.0);
                made |= finite(&mut c.undersample, 1.0);
                made |= finite_envelopes(&mut c.envelopes);
                if c.normalize.is_some_and(|g| !g.is_finite()) {
                    c.normalize = None;
                    made = true;
                }
            }
        }
        for b in &mut self.busses {
            made |= finite(&mut b.channel.gain_db, 0.0);
            made |= finite(&mut b.channel.pan, 0.0);
            made |= finite_envelopes(&mut b.channel.envelopes);
            made |= finite_sends(&mut b.sends);
        }
        for c in &mut self.fx_chains {
            made |= finite(&mut c.input_db, 0.0);
            made |= finite(&mut c.channel.gain_db, 0.0);
            made |= finite(&mut c.channel.pan, 0.0);
            made |= finite_envelopes(&mut c.channel.envelopes);
        }
        for b in &mut self.input_busses {
            made |= finite(&mut b.gain_db, 0.0);
            made |= finite(&mut b.pan, 0.0);
            made |= finite_sends(&mut b.sends);
        }
        made |= finite(&mut self.master.gain_db, 0.0);
        for db in &mut self.master.channel_db {
            made |= finite(db, 0.0);
        }
        made |= finite_envelopes(&mut self.video_bus.envelopes);
        made
    }

    /// Playback rates within what an event plays at (0.01 to 100 times);
    /// one that is no speed is 1.
    fn mend_rates(&mut self) -> bool {
        let mut made = false;
        for c in self.tracks.iter_mut().flat_map(|t| &mut t.clips) {
            if !(0.01..=100.0).contains(&c.rate) {
                c.rate = if c.rate.is_finite() && c.rate > 0.0 {
                    c.rate.clamp(0.01, 100.0)
                } else {
                    1.0
                };
                made = true;
            }
        }
        made
    }

    /// Each bus and each chain once, and no more of them than a project
    /// has: the last ones go as when they are deleted.
    fn mend_strip_lists(&mut self) -> bool {
        let before = (self.busses.len(), self.fx_chains.len());
        let mut seen = HashSet::new();
        self.busses.retain(|b| seen.insert(b.id));
        let mut seen = HashSet::new();
        self.fx_chains.retain(|c| seen.insert(c.id));
        if self.busses.len() > MAX_BUSSES {
            self.set_bus_count(MAX_BUSSES);
        }
        while self.fx_chains.len() > MAX_FX_CHAINS {
            let Some(last) = self.fx_chains.last().map(|c| c.id) else {
                break;
            };
            self.remove_fx_chain(last);
        }
        before != (self.busses.len(), self.fx_chains.len())
    }

    /// Tracks, busses and chains play into a bus that exists (tracks and
    /// chains never into a hardware output): the Master otherwise.
    fn mend_routes(&mut self) -> bool {
        let busses: HashSet<BusId> = self.busses.iter().map(|b| b.id).collect();
        let mut made = false;
        let mut route = |output: &mut Output, hardware: bool| {
            let fits = match *output {
                Output::Master => true,
                Output::Hardware(_) => hardware,
                Output::Bus(b) => busses.contains(&b),
            };
            if !fits {
                *output = Output::Master;
                made = true;
            }
        };
        for b in &mut self.busses {
            route(&mut b.channel.output, true);
        }
        for c in &mut self.fx_chains {
            route(&mut c.channel.output, false);
        }
        for t in &mut self.tracks {
            route(&mut t.output, false);
        }
        made
    }

    /// Sends go where they may: a track's to a bus or a chain that exists,
    /// a bus's to a chain that exists or to another bus on a hardware
    /// output.
    fn mend_sends(&mut self) -> bool {
        let busses: HashSet<BusId> = self.busses.iter().map(|b| b.id).collect();
        let chains: HashSet<FxChainId> = self.fx_chains.iter().map(|c| c.id).collect();
        let hardware: HashSet<BusId> = self
            .busses
            .iter()
            .filter(|b| b.channel.output.is_hardware())
            .map(|b| b.id)
            .collect();
        let mut made = false;
        for b in &mut self.busses {
            let (id, before) = (b.id, b.sends.len());
            b.sends.retain(|s| match s.to {
                SendTo::Fx(c) => chains.contains(&c),
                SendTo::Bus(to) => to != id && hardware.contains(&to),
            });
            made |= b.sends.len() != before;
        }
        for t in &mut self.tracks {
            let before = t.sends.len();
            t.sends.retain(|s| match s.to {
                SendTo::Bus(b) => busses.contains(&b),
                SendTo::Fx(c) => chains.contains(&c),
            });
            made |= t.sends.len() != before;
        }
        made
    }

    /// Nothing plays in a circle: where what a bus or a chain plays or
    /// sends into comes back to it, its output goes to the Master and its
    /// send is removed.
    fn mend_circles(&mut self) -> bool {
        let mut made = false;
        for s in self.strips() {
            let back: Vec<Strip> = self
                .strip_feeds(s)
                .into_iter()
                .filter(|&n| self.reaches(n, s))
                .collect();
            if back.is_empty() {
                continue;
            }
            made = true;
            if let Strip::Bus(id) = s
                && let Some(b) = self.bus_mut(id)
            {
                b.sends.retain(|x| {
                    !back.contains(&match x.to {
                        SendTo::Bus(to) => Strip::Bus(to),
                        SendTo::Fx(to) => Strip::Chain(to),
                    })
                });
            }
            if let Some(c) = self.strip_channel_mut(s)
                && let Output::Bus(b) = c.output
                && back.contains(&Strip::Bus(b))
            {
                c.output = Output::Master;
            }
        }
        made
    }

    /// A send's envelope rides on its send: one without it gets its send
    /// back at no level, which sounds as no send did — or goes, when what
    /// it was sent to is not there.
    fn mend_send_envelopes(&mut self) -> bool {
        let busses: HashSet<BusId> = self.busses.iter().map(|b| b.id).collect();
        let chains: HashSet<FxChainId> = self.fx_chains.iter().map(|c| c.id).collect();
        let mut made = false;
        for t in &mut self.tracks {
            let Track {
                sends, envelopes, ..
            } = t;
            envelopes.retain(|e| {
                let EnvelopeKind::Send(to) = e.kind else {
                    return true;
                };
                if sends.iter().any(|s| s.to == to) {
                    return true;
                }
                made = true;
                let exists = match to {
                    SendTo::Bus(b) => busses.contains(&b),
                    SendTo::Fx(c) => chains.contains(&c),
                };
                if exists {
                    sends.push(Send::new(to));
                }
                exists
            });
        }
        made
    }

    /// Each track group's tracks together, every track in a group that
    /// exists ([`Self::normalize_groups`]).
    fn mend_track_groups(&mut self) -> bool {
        let before: Vec<_> = self.tracks.iter().map(|t| t.group).collect();
        let groups = self.track_groups.len();
        self.normalize_groups();
        groups != self.track_groups.len() || self.tracks.iter().map(|t| t.group).ne(before)
    }

    /// Every track and event under a number of its own, below the
    /// project's counter: the counter goes past the numbers given out, and
    /// the later of two that share one takes a new one.
    fn mend_ids(&mut self) -> bool {
        let events = self.clips().map(|(_, c)| c.id.0).max();
        let mut made = events.is_some_and(|id| id >= self.next_id);
        if made {
            // Past everything numbered by it, so that nothing new shares
            // a number.
            let top = self
                .clips()
                .flat_map(|(_, c)| [Some(c.id.0), c.group.map(|g| g.0)])
                .flatten()
                .chain(self.tracks.iter().map(|t| t.id.0))
                .chain(self.media.iter().map(|m| m.id.0))
                .chain(self.subclips.iter().map(|s| s.id.0))
                .chain(self.markers.iter().map(|m| m.id.0))
                .chain(self.regions.iter().map(|r| r.id.0))
                .chain(self.commands.iter().map(|c| c.id.0))
                .chain(self.cd_tracks.iter().map(|t| t.id.0))
                .chain(self.cd_indices.iter().map(|i| i.id.0))
                .chain(self.bins.iter().map(|b| b.id.0))
                .chain(self.smart_bins.iter().map(|b| b.id.0))
                .chain(self.storyboards.iter().map(|s| s.id.0))
                .max()
                .unwrap_or(0);
            self.next_id = self.next_id.max(top.saturating_add(1));
        }
        let mut seen = HashSet::new();
        let mut tracks = std::mem::take(&mut self.tracks);
        for t in &mut tracks {
            if !seen.insert(t.id.0) {
                t.id = TrackId(self.alloc_id());
                seen.insert(t.id.0);
                made = true;
            }
            for c in &mut t.clips {
                if !seen.insert(c.id.0) {
                    c.id = ClipId(self.alloc_id());
                    seen.insert(c.id.0);
                    made = true;
                }
            }
        }
        self.tracks = tracks;
        made
    }

    /// The events as a project has them: of media their track plays, with
    /// a length, from the timeline's start on, no earlier than their media
    /// without Loop, their fades within them.
    fn mend_events(&mut self) -> EventMends {
        let mut made = EventMends::default();
        let min = self.min_clip_duration();
        let Project { tracks, media, .. } = self;
        for t in tracks.iter_mut() {
            let kind = t.kind;
            let plays = |id: MediaId| {
                media
                    .iter()
                    .find(|m| m.id == id)
                    .is_some_and(|m| m.has(kind))
            };
            t.clips.retain_mut(|c| {
                if !mend_takes(c, &plays, &mut made) {
                    return false;
                }
                if c.duration <= Tick::ZERO {
                    made.no_length = true;
                    return false;
                }
                // Its fades as they came (its start, mended, takes them
                // along).
                let fades = c.fade_in < Tick::ZERO
                    || c.fade_out < Tick::ZERO
                    || c.fade_in + c.fade_out > c.duration;
                if c.start < Tick::ZERO {
                    c.start = Tick::ZERO;
                    made.before_zero = true;
                }
                if c.source_in < Tick::ZERO && !c.looped && c.freeze.is_none() {
                    start_at_media(c, min);
                    made.before_media = true;
                }
                if fades {
                    clamp_fades(c);
                    made.fades = true;
                }
                true
            });
        }
        made
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::busses::{Bus, Channel};
    use crate::model::{AudioStreamInfo, Generator, MediaInfo, MediaSource};
    use crate::track_groups::TrackGroupId;
    use bv_core::SampleRate;

    fn s(x: i64) -> Tick {
        Tick::from_seconds(x)
    }

    /// What the test project has: a 20 s file's picture and sound, grouped,
    /// from 0 to 10 s on a video and an audio track; another file; a bus
    /// and an effects chain.
    struct Has {
        video: ClipId,
        audio: ClipId,
        media: MediaId,
        other: MediaId,
        sound_track: TrackId,
        bus: BusId,
        chain: FxChainId,
    }

    fn movie(p: &mut Project) -> MediaId {
        let mut m = MediaInfo::generated(Generator::ColorBars, &p.settings);
        m.source = MediaSource::File {
            path: "clip.mp4".into(),
        };
        m.duration = s(20);
        m.audio = Some(AudioStreamInfo {
            sample_rate: SampleRate::HZ_48000,
            channels: 2,
        });
        p.add_media(m)
    }

    fn project() -> (Project, Has) {
        let mut p = Project::default();
        let (v, a) = (p.tracks[0].id, p.tracks[1].id);
        let media = movie(&mut p);
        let other = movie(&mut p);
        let ids = p.insert_media(media, Tick::ZERO, Some(v), Some(a)).unwrap();
        p.trim_end(ids[0], s(10)).unwrap();
        let bus = p.add_bus().unwrap();
        let chain = p.add_fx_chain(Vec::new()).unwrap();
        p.check_invariants().unwrap();
        let has = Has {
            video: ids[0],
            audio: ids[1],
            media,
            other,
            sound_track: a,
            bus,
            chain,
        };
        (p, has)
    }

    /// The project after `harm`, which `check_invariants` refuses saying
    /// `why`, is mended: the repair says `line` and nothing else, the
    /// project is sound, has nothing more to mend and opens from its file.
    fn mended(harm: impl FnOnce(&mut Project, &Has), why: &str, line: &str) -> (Project, Has) {
        let (mut p, has) = project();
        harm(&mut p, &has);
        let refused = p.check_invariants().expect_err(why);
        assert!(refused.contains(why), "{refused}");
        assert_eq!(p.repair(), vec![line.to_string()], "{why}");
        p.check_invariants().unwrap();
        assert!(p.repair().is_empty(), "{why}: mended at once");
        let back = Project::from_json(&p.to_json()).expect(why);
        back.check_invariants().unwrap();
        assert!(MENDS.contains(&line));
        (p, has)
    }

    #[test]
    fn a_sound_project_is_left_as_it_is() {
        let (mut p, has) = project();
        p.set_fades(has.video, Some(s(1)), Some(s(2))).unwrap();
        p.rename_take(has.video, 0, "Wide").unwrap();
        p.add_take(has.audio, has.other).unwrap();
        assert!(p.set_send(
            has.sound_track,
            Send {
                link_pan: true,
                ..Send::new(SendTo::Bus(has.bus))
            }
        ));
        let before = p.clone();
        assert!(p.repair().is_empty());
        assert_eq!(p, before);
        p.check_invariants().unwrap();
    }

    #[test]
    fn fades_longer_than_their_event_are_shortened() {
        let (p, has) = mended(
            |p, has| {
                let c = p.clip_mut(has.video).unwrap();
                (c.fade_in, c.fade_out) = (s(8), s(8));
            },
            "fades of clip",
            FADES,
        );
        let c = p.clip(has.video).unwrap();
        assert_eq!((c.fade_in, c.fade_out), (s(8), s(2)));
        mended(
            |p, has| p.clip_mut(has.audio).unwrap().fade_out = -s(1),
            "fades of clip",
            FADES,
        );
    }

    #[test]
    fn an_event_before_its_media_without_loop_starts_with_it() {
        // 2 s of it before its media: that part goes, the picture stays.
        let (p, has) = mended(
            |p, has| {
                let c = p.clip_mut(has.video).unwrap();
                (c.looped, c.source_in) = (false, -s(2));
            },
            "starts before its media",
            BEFORE_MEDIA,
        );
        let c = p.clip(has.video).unwrap();
        assert_eq!((c.start, c.duration, c.source_in), (s(2), s(8), Tick::ZERO));
        // All of it before its media: it plays from its media's start.
        let (p, has) = mended(
            |p, has| {
                let c = p.clip_mut(has.audio).unwrap();
                (c.looped, c.source_in) = (false, -s(30));
            },
            "starts before its media",
            BEFORE_MEDIA,
        );
        let c = p.clip(has.audio).unwrap();
        assert_eq!(
            (c.start, c.duration, c.source_in),
            (Tick::ZERO, s(10), Tick::ZERO)
        );
    }

    #[test]
    fn an_event_before_the_timelines_start_is_moved_to_it() {
        let (p, has) = mended(
            |p, has| p.clip_mut(has.video).unwrap().start = -s(3),
            "before zero",
            BEFORE_ZERO,
        );
        let c = p.clip(has.video).unwrap();
        assert_eq!((c.start, c.duration), (Tick::ZERO, s(10)));
    }

    #[test]
    fn events_of_missing_media_or_without_their_stream_are_removed() {
        let (p, has) = mended(
            |p, has| p.media.retain(|m| m.id != has.media),
            "points to missing media",
            NO_MEDIA,
        );
        assert!(p.clip(has.video).is_none() && p.clip(has.audio).is_none());
        // The media without sound: its sound's event goes, its picture's
        // stays.
        let (p, has) = mended(
            |p, has| {
                p.media
                    .iter_mut()
                    .find(|m| m.id == has.media)
                    .unwrap()
                    .audio = None
            },
            "without stream",
            NO_MEDIA,
        );
        assert!(p.clip(has.video).is_some() && p.clip(has.audio).is_none());
    }

    #[test]
    fn takes_of_missing_media_are_removed_and_another_plays() {
        // A take that is not playing loses its media: it goes.
        let (p, has) = mended(
            |p, has| {
                p.add_take(has.video, has.other).unwrap();
                p.set_active_take(has.video, 0).unwrap();
                p.media.retain(|m| m.id != has.other);
            },
            "take of missing media",
            TAKES,
        );
        let c = p.clip(has.video).unwrap();
        assert_eq!((c.take_count(), c.media), (1, has.media));
        // The one playing loses it: the other plays, from where it was.
        let (p, has) = mended(
            |p, has| {
                p.clip_mut(has.video).unwrap().source_in = s(3);
                p.add_take(has.video, has.other).unwrap();
                p.media.retain(|m| m.id != has.other);
            },
            "points to missing media",
            TAKES,
        );
        let c = p.clip(has.video).unwrap();
        assert_eq!((c.take_count(), c.media, c.source_in), (1, has.media, s(3)));
        // A take's media without the track's stream; the take that stays
        // keeps its name.
        let (p, has) = mended(
            |p, has| {
                p.add_take(has.video, has.other).unwrap();
                p.set_active_take(has.video, 0).unwrap();
                p.rename_take(has.video, 0, "Wide").unwrap();
                p.media
                    .iter_mut()
                    .find(|m| m.id == has.other)
                    .unwrap()
                    .video = None;
            },
            "take without its stream",
            TAKES,
        );
        assert_eq!(p.take_name(has.video, 0).as_deref(), Some("Wide"));
        assert_eq!(p.clip(has.video).unwrap().take_count(), 1);
    }

    #[test]
    fn a_missing_active_take_is_the_last_one() {
        let (p, has) = mended(
            |p, has| {
                p.add_take(has.video, has.other).unwrap();
                p.clip_mut(has.video).unwrap().active_take = 7;
            },
            "missing active take",
            ACTIVE_TAKE,
        );
        let c = p.clip(has.video).unwrap();
        assert_eq!((c.active_take, c.media), (1, has.other));
    }

    #[test]
    fn events_with_no_length_are_removed() {
        for length in [Tick::ZERO, -s(2)] {
            let (p, has) = mended(
                |p, has| p.clip_mut(has.audio).unwrap().duration = length,
                "with length",
                NO_LENGTH,
            );
            assert!(p.clip(has.video).is_some() && p.clip(has.audio).is_none());
        }
    }

    #[test]
    fn rates_out_of_range_are_set_within_it() {
        for (bad, good) in [(f64::NAN, 1.0), (0.0, 1.0), (-2.0, 1.0), (500.0, 100.0)] {
            let (p, has) = mended(
                |p, has| p.clip_mut(has.video).unwrap().rate = bad,
                "invalid rate",
                RATES,
            );
            assert_eq!(p.clip(has.video).unwrap().rate, good);
        }
    }

    #[test]
    fn numbers_shared_or_past_the_counter_are_set_right() {
        let (p, has) = mended(
            |p, has| p.clip_mut(has.audio).unwrap().id = has.video,
            "duplicate clip id",
            IDS,
        );
        assert_eq!(p.clips().count(), 2);
        assert!(p.clip(has.video).is_some());
        mended(
            |p, _| p.tracks[1].id = p.tracks[0].id,
            "duplicate track id",
            IDS,
        );
        let (mut p, _) = mended(
            |p, has| p.clip_mut(has.video).unwrap().id = ClipId(p.id_counter() + 10),
            "beyond the counter",
            IDS,
        );
        // What is made next has a number of its own.
        let new = p.add_track(crate::model::TrackKind::Audio);
        assert!(p.clips().all(|(_, c)| c.id.0 != new.0));
        p.check_invariants().unwrap();
    }

    #[test]
    fn events_out_of_order_are_put_in_order() {
        let (p, has) = mended(
            |p, has| {
                let v = p.tracks[0].id;
                p.insert_media(has.other, s(12), Some(v), None).unwrap();
                p.tracks[0].clips.swap(0, 1);
            },
            "out of order",
            ORDER,
        );
        assert_eq!(p.tracks[0].clips[0].id, has.video);
    }

    #[test]
    fn busses_and_chains_are_each_once_and_not_too_many() {
        let extra = |id| Bus {
            id: BusId(id),
            sends: Vec::new(),
            channel: Channel::default(),
        };
        let (p, _) = mended(
            |p, _| p.busses.extend((0..30).map(|k| extra(100 + k))),
            "too many busses",
            STRIPS,
        );
        assert_eq!(p.busses.len(), MAX_BUSSES);
        let (p, has) = mended(
            |p, has| p.busses.push(extra(has.bus.0)),
            "duplicate bus",
            STRIPS,
        );
        assert_eq!(p.busses.len(), 1);
        assert!(p.bus(has.bus).is_some());
        let (p, _) = mended(
            |p, _| {
                let twice = p.fx_chains[0].clone();
                p.fx_chains.push(twice);
            },
            "duplicate chain",
            STRIPS,
        );
        assert_eq!(p.fx_chains.len(), 1);
    }

    #[test]
    fn routes_to_no_bus_go_to_the_master() {
        let nowhere = Output::Bus(BusId(77));
        let (p, has) = mended(
            |p, has| p.bus_mut(has.bus).unwrap().channel.output = nowhere,
            "routed to no bus",
            ROUTES,
        );
        assert_eq!(p.bus(has.bus).unwrap().channel.output, Output::Master);
        mended(
            |p, has| p.fx_chain_mut(has.chain).unwrap().channel.output = Output::Hardware(0),
            "routed to no bus",
            ROUTES,
        );
        let (p, has) = mended(
            |p, has| p.track_by_id_mut(has.sound_track).unwrap().output = nowhere,
            "routed to no bus",
            ROUTES,
        );
        assert_eq!(p.track(has.sound_track).unwrap().output, Output::Master);
    }

    #[test]
    fn sends_to_what_they_cannot_reach_are_removed() {
        let (p, has) = mended(
            |p, has| {
                let b = p.bus_mut(has.bus).unwrap();
                b.sends.push(Send::of_bus(SendTo::Fx(FxChainId(77))));
                b.sends.push(Send::of_bus(SendTo::Fx(has.chain)));
            },
            "sends to what it may not",
            SENDS,
        );
        assert_eq!(p.bus(has.bus).unwrap().sends.len(), 1);
        let (p, has) = mended(
            |p, has| {
                let t = p.track_by_id_mut(has.sound_track).unwrap();
                t.sends.push(Send::new(SendTo::Bus(BusId(77))));
            },
            "sends to nothing",
            SENDS,
        );
        assert!(p.track(has.sound_track).unwrap().sends.is_empty());
    }

    #[test]
    fn routing_in_a_circle_is_cut() {
        let (p, has) = mended(
            |p, has| {
                let b = p.add_bus().unwrap();
                p.bus_mut(has.bus).unwrap().channel.output = Output::Bus(b);
                p.bus_mut(b).unwrap().channel.output = Output::Bus(has.bus);
            },
            "plays in a circle",
            CIRCLES,
        );
        assert_eq!(p.bus(has.bus).unwrap().channel.output, Output::Master);
        // A chain that plays into the bus that sends to it.
        let (p, has) = mended(
            |p, has| {
                p.fx_chain_mut(has.chain).unwrap().channel.output = Output::Bus(has.bus);
                let b = p.bus_mut(has.bus).unwrap();
                b.sends.push(Send::of_bus(SendTo::Fx(has.chain)));
            },
            "plays in a circle",
            CIRCLES,
        );
        assert!(p.bus(has.bus).unwrap().sends.is_empty());
        assert_eq!(
            p.fx_chain(has.chain).unwrap().channel.output,
            Output::Bus(has.bus)
        );
    }

    #[test]
    fn a_send_envelope_gets_its_send_back_or_goes() {
        let rides = |to: SendTo| Envelope::new(EnvelopeKind::Send(to));
        let (p, has) = mended(
            |p, has| {
                let t = p.track_by_id_mut(has.sound_track).unwrap();
                t.envelopes.push(rides(SendTo::Bus(has.bus)));
            },
            "send envelope without its send",
            SEND_ENVELOPES,
        );
        let t = p.track(has.sound_track).unwrap();
        assert_eq!(t.sends, vec![Send::new(SendTo::Bus(has.bus))]);
        assert_eq!(t.envelopes.len(), 1);
        // Sent to a bus that is not there: the envelope goes.
        let (p, has) = mended(
            |p, has| {
                let t = p.track_by_id_mut(has.sound_track).unwrap();
                t.envelopes.push(rides(SendTo::Bus(BusId(77))));
            },
            "send envelope without its send",
            SEND_ENVELOPES,
        );
        let t = p.track(has.sound_track).unwrap();
        assert!(t.sends.is_empty() && t.envelopes.is_empty());
    }

    #[test]
    fn track_groups_are_put_back_together() {
        let (p, _) = mended(
            |p, _| p.tracks[0].group = Some(TrackGroupId(9)),
            "missing group",
            TRACK_GROUPS,
        );
        assert_eq!(p.tracks[0].group, None);
    }

    #[test]
    fn numbers_that_are_not_finite_are_set_to_their_defaults() {
        let (mut p, has) = project();
        p.toggle_envelope(has.sound_track, EnvelopeKind::Volume)
            .unwrap();
        {
            let t = p.track_by_id_mut(has.sound_track).unwrap();
            t.pan = f32::NAN;
            t.envelopes[0].points[0].value = f32::INFINITY;
            t.sends.push(Send {
                gain_db: f32::NAN,
                ..Send::new(SendTo::Bus(has.bus))
            });
        }
        p.clip_mut(has.video).unwrap().level = f32::NEG_INFINITY;
        p.clip_mut(has.audio).unwrap().normalize = Some(f32::NAN);
        p.bus_mut(has.bus).unwrap().channel.gain_db = f32::INFINITY;
        p.master.gain_db = f32::NAN;
        assert_eq!(p.repair(), vec![NUMBERS.to_string()]);
        let t = p.track(has.sound_track).unwrap();
        assert_eq!((t.pan, t.envelopes[0].points[0].value), (0.0, 1.0));
        // A send at no level is as sends start: kept so.
        assert_eq!(t.sends[0].gain_db, f32::NEG_INFINITY);
        assert_eq!(p.clip(has.video).unwrap().level, 1.0);
        assert_eq!(p.clip(has.audio).unwrap().normalize, None);
        assert_eq!(p.bus(has.bus).unwrap().channel.gain_db, 0.0);
        assert_eq!(p.master.gain_db, 0.0);
        assert!(p.repair().is_empty());
        p.check_invariants().unwrap();
        let back = Project::from_json(&p.to_json()).unwrap();
        back.check_invariants().unwrap();
    }

    #[test]
    fn several_harms_are_each_said_once_in_order() {
        let (mut p, has) = project();
        {
            let c = p.clip_mut(has.video).unwrap();
            (c.fade_in, c.fade_out) = (s(9), s(9));
            (c.looped, c.source_in) = (false, -s(1));
            c.rate = 0.0;
        }
        p.clip_mut(has.audio).unwrap().start = -s(1);
        p.track_by_id_mut(has.sound_track).unwrap().output = Output::Bus(BusId(77));
        assert_eq!(
            p.repair(),
            vec![RATES, ROUTES, BEFORE_ZERO, BEFORE_MEDIA, FADES]
        );
        p.check_invariants().unwrap();
    }
}
