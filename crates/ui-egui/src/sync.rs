//! Synchronization over MIDI (Options › Timecode, Options › Preferences ›
//! Sync): MIDI timecode and MIDI clock sent while BananaV plays, and
//! playback that follows MIDI timecode coming in.
//!
//! What goes out is timed by a thread of its own that knows where playback
//! is (the interface tells it each frame) and works out where it has got
//! to since; what comes in is pieced together on the MIDI input's thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bv_core::Tick;

use crate::settings::{LocateMessages, MtcRate};

/// A timecode: hours, minutes, seconds, frames.
pub(crate) type Timecode = [u8; 4];

/// The timecode of frame `frame` (counted from zero) at `rate`; drop-frame
/// timecode skips frame numbers 0 and 1 at each minute but every tenth.
pub(crate) fn timecode(frame: i64, rate: MtcRate) -> Timecode {
    let mut n = frame.max(0) as u64;
    let base = rate.base() as u64;
    if rate.drop_frame() {
        let (per_10_min, per_min) = (base * 600 - 18, base * 60 - 2);
        let (tens, rem) = (n / per_10_min, n % per_10_min);
        n += 18 * tens;
        if rem > 2 {
            n += 2 * ((rem - 2) / per_min);
        }
    }
    let ff = n % base;
    let total = n / base;
    [
        ((total / 3600) % 24) as u8,
        ((total / 60) % 60) as u8,
        (total % 60) as u8,
        ff as u8,
    ]
}

/// The frame (counted from zero) a timecode names at `rate`.
pub(crate) fn frame_of(tc: Timecode, rate: MtcRate) -> i64 {
    let [h, m, s, f] = tc.map(|x| x as i64);
    let base = rate.base() as i64;
    let mut frame = ((h * 60 + m) * 60 + s) * base + f;
    if rate.drop_frame() {
        let minutes = h * 60 + m;
        frame -= 2 * (minutes - minutes / 10);
    }
    frame
}

/// A timecode as text, "01:02:03:04" (";" before the frames of drop-frame).
pub(crate) fn timecode_text(tc: Timecode, rate: MtcRate) -> String {
    let sep = if rate.drop_frame() { ';' } else { ':' };
    format!("{:02}:{:02}:{:02}{sep}{:02}", tc[0], tc[1], tc[2], tc[3])
}

/// Quarter-frame message `piece` (0–7) of `tc`: each carries four bits of
/// it, the last the rate too.
pub(crate) fn quarter_frame(piece: u8, tc: Timecode, rate: MtcRate) -> [u8; 2] {
    let [h, m, s, f] = tc;
    let nibble = match piece & 7 {
        0 => f & 0x0F,
        1 => f >> 4,
        2 => s & 0x0F,
        3 => s >> 4,
        4 => m & 0x0F,
        5 => m >> 4,
        6 => h & 0x0F,
        _ => (h >> 4) & 1 | rate.code() << 1,
    };
    [0xF1, (piece & 7) << 4 | nibble]
}

/// The full-frame message of `tc`, telling devices where to go.
pub(crate) fn full_frame(tc: Timecode, rate: MtcRate) -> [u8; 10] {
    let [h, m, s, f] = tc;
    [
        0xF0,
        0x7F,
        0x7F,
        0x01,
        0x01,
        rate.code() << 5 | (h & 0x1F),
        m,
        s,
        f,
        0xF7,
    ]
}

/// Timecode pieced together from quarter-frame messages.
#[derive(Debug, Default)]
pub(crate) struct MtcDecoder {
    pieces: [u8; 8],
    /// Which pieces came since the last whole timecode.
    have: u8,
    /// The last whole one.
    pub latest: Option<Received>,
}

/// Timecode received: the frame the sender was at when it began sending
/// it, its rate's code, and when its last piece came.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Received {
    pub tc: Timecode,
    pub code: u8,
    pub at: Instant,
}

impl MtcDecoder {
    /// A quarter-frame message's data byte, come in `at`.
    pub fn quarter_frame(&mut self, data: u8, at: Instant) {
        let piece = (data >> 4) & 7;
        self.pieces[piece as usize] = data & 0x0F;
        self.have |= 1 << piece;
        if piece == 7 && self.have == 0xFF {
            let p = self.pieces;
            self.latest = Some(Received {
                tc: [
                    p[6] | (p[7] & 1) << 4,
                    p[4] | p[5] << 4,
                    p[2] | p[3] << 4,
                    p[0] | p[1] << 4,
                ],
                code: (p[7] >> 1) & 3,
                at,
            });
            self.have = 0;
        } else if piece == 0 {
            self.have = 1;
        }
    }
}

impl Received {
    /// Where the sender is `now`, in seconds of timecode: a whole
    /// timecode takes two frames to send, so it is two frames on from the
    /// one it names; `quarters` more quarter frames (Sync › Advanced:
    /// "Offset adjust").
    pub fn seconds_at(&self, now: Instant, rate: MtcRate, quarters: i32) -> f64 {
        let frames = frame_of(self.tc, rate) as f64 + 2.0 + quarters as f64 / 4.0;
        frames / rate.fps() + now.saturating_duration_since(self.at).as_secs_f64()
    }
}

/// Where playback is, as the interface last saw it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Transport {
    pub playing: bool,
    /// The position, and when it was there.
    pub position: Tick,
    pub at: Instant,
    pub rate: f64,
    /// The ruler's start (timecode counts from it).
    pub offset: Tick,
    pub beats_per_minute: f64,
    /// Bumped when playback starts or jumps, or the cursor moves.
    pub serial: u64,
}

impl Transport {
    pub fn stopped() -> Transport {
        Transport {
            playing: false,
            position: Tick::ZERO,
            at: Instant::now(),
            rate: 1.0,
            offset: Tick::ZERO,
            beats_per_minute: 120.0,
            serial: 0,
        }
    }

    /// Seconds of the ruler `now`.
    fn seconds(&self, now: Instant) -> f64 {
        let mut s = (self.position + self.offset).as_seconds_f64();
        if self.playing {
            s += now.saturating_duration_since(self.at).as_secs_f64() * self.rate;
        }
        s.max(0.0)
    }
}

/// MIDI timecode to send: where, at what rate, when full frames go.
pub(crate) struct MtcOut {
    pub port: Arc<bv_platform::MidiOut>,
    pub rate: MtcRate,
    pub full_frames: LocateMessages,
}

/// MIDI clock to send: where, Start instead of Continue, when song
/// position pointers go.
pub(crate) struct ClockOut {
    pub port: Arc<bv_platform::MidiOut>,
    pub send_start: bool,
    pub song_position: LocateMessages,
}

/// The thread sending MIDI timecode and MIDI clock.
#[derive(Debug)]
pub(crate) struct Generator {
    transport: Arc<Mutex<Transport>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// The timecode sent last (Time Display: MIDI Timecode Out).
    pub sent: Arc<Mutex<Option<(Timecode, MtcRate)>>>,
}

impl Generator {
    pub fn start(mtc: Option<MtcOut>, clock: Option<ClockOut>) -> Generator {
        let transport = Arc::new(Mutex::new(Transport::stopped()));
        let stop = Arc::new(AtomicBool::new(false));
        let sent = Arc::new(Mutex::new(None));
        let thread = {
            let (transport, stop, sent) = (transport.clone(), stop.clone(), sent.clone());
            std::thread::spawn(move || {
                let mut state = Sending::default();
                while !stop.load(Ordering::Relaxed) {
                    let t = transport.lock().map(|t| *t).ok();
                    if let Some(t) = t {
                        state.step(&t, Instant::now(), mtc.as_ref(), clock.as_ref(), &sent);
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                if let Some(c) = &clock
                    && state.playing
                {
                    c.port.send(&[0xFC]);
                }
            })
        };
        Generator {
            transport,
            stop,
            thread: Some(thread),
            sent,
        }
    }

    /// Where playback is now.
    pub fn publish(&self, t: Transport) {
        if let Ok(mut slot) = self.transport.lock() {
            *slot = t;
        }
    }
}

impl Drop for Generator {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// What the sending thread keeps between its steps.
#[derive(Debug, Default)]
struct Sending {
    playing: bool,
    serial: u64,
    /// The last quarter frame and clock pulse sent (counted from the
    /// ruler's start).
    quarter: Option<i64>,
    pulse: Option<i64>,
}

impl Sending {
    fn step(
        &mut self,
        t: &Transport,
        now: Instant,
        mtc: Option<&MtcOut>,
        clock: Option<&ClockOut>,
        sent: &Mutex<Option<(Timecode, MtcRate)>>,
    ) {
        let started = t.playing && (!self.playing || t.serial != self.serial);
        let stopped = !t.playing && self.playing;
        let moved = !t.playing && t.serial != self.serial;
        let seconds = t.seconds(now);
        if let Some(m) = mtc {
            let frame = (seconds * m.rate.fps() + 1e-6).floor() as i64;
            let locate = match m.full_frames {
                LocateMessages::Never => false,
                LocateMessages::PlaybackStart => started,
                LocateMessages::PlaybackStartAndCursor => started || moved,
            };
            if locate {
                let tc = timecode(frame, m.rate);
                let _ = m.port.send_sysex(&full_frame(tc, m.rate));
                if let Ok(mut s) = sent.lock() {
                    *s = Some((tc, m.rate));
                }
            }
            if started {
                self.quarter = None;
            }
            if t.playing {
                let q = (seconds * m.rate.fps() * 4.0 + 1e-6).floor() as i64;
                let from = match self.quarter {
                    Some(last) if last < q && q - last <= 8 => last + 1,
                    _ => q,
                };
                for k in from..=q {
                    // The eight pieces of a whole timecode give the frame
                    // the first one was sent at.
                    let tc = timecode(k.div_euclid(8) * 2, m.rate);
                    m.port
                        .send(&quarter_frame(k.rem_euclid(8) as u8, tc, m.rate));
                }
                self.quarter = Some(q);
                if let Ok(mut s) = sent.lock() {
                    *s = Some((timecode(frame, m.rate), m.rate));
                }
            }
        }
        if let Some(c) = clock {
            let beats = seconds * t.beats_per_minute / 60.0;
            let locate = match c.song_position {
                LocateMessages::Never => false,
                LocateMessages::PlaybackStart => started,
                LocateMessages::PlaybackStartAndCursor => started || moved,
            };
            if locate {
                c.port.send(&song_position(beats));
            }
            if started {
                // From the very start, or when asked to: Start, else
                // Continue from the song position.
                let first = c.send_start || beats < 1e-6;
                c.port.send(&[if first { 0xFA } else { 0xFB }]);
                self.pulse = Some((beats * 24.0).floor() as i64);
            } else if t.playing {
                let p = (beats * 24.0 + 1e-6).floor() as i64;
                let last = self.pulse.unwrap_or(p);
                for _ in last.max(p - 8)..p {
                    c.port.send(&[0xF8]);
                }
                self.pulse = Some(p);
            }
            if stopped {
                c.port.send(&[0xFC]);
            }
        }
        self.playing = t.playing;
        self.serial = t.serial;
    }
}

/// A song position pointer at `beats` (counted in sixteenth notes).
pub(crate) fn song_position(beats: f64) -> [u8; 3] {
    let sixteenths = ((beats * 4.0).floor() as i64).clamp(0, 0x3FFF) as u16;
    [0xF2, (sixteenths & 0x7F) as u8, (sixteenths >> 7) as u8]
}

/// What the time display shows (its right-click menu).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TimeDisplay {
    /// The time at the cursor (playback's while playing).
    #[default]
    Cursor,
    /// MIDI timecode coming in, or what following it is doing.
    MtcIn,
    /// MIDI timecode going out.
    MtcOut,
    /// MIDI clock going out: where it is in measures and beats.
    ClockOut,
}

/// What the Time Display shows of MIDI timecode coming in.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MtcStatus {
    /// Following is on and nothing comes.
    Listening,
    /// Timecode comes at another rate than Sync's.
    WrongFormat,
    /// Locked: where it is.
    Locked(Timecode),
}

/// Following MIDI timecode (Options › Timecode › Trigger from MIDI
/// Timecode).
#[derive(Debug, Default)]
pub(crate) struct Chase {
    /// Playback is following.
    pub following: bool,
    /// Waiting to start: where (Sync › Advanced: "Synchronization delay
    /// time").
    pub starting_at: Option<Tick>,
    /// Timecode stopped coming at: playback free-wheels until it has been
    /// gone so long.
    pub lost: Option<Instant>,
    pub status: Option<MtcStatus>,
}

/// What following timecode asks of playback this time round.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ChaseAction {
    Nothing,
    /// Put the cursor here, ready to start.
    Cue(Tick),
    /// Play from here (or jump here while playing).
    Play(Tick),
    Stop,
}

/// The settings following timecode works by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChaseSettings {
    pub rate: MtcRate,
    pub freewheel: bool,
    pub slack_frames: u32,
    pub freewheel_seconds: f32,
    pub sync_delay: f32,
    pub offset_quarters: i32,
}

impl Chase {
    /// One step of following: `latest` the timecode received last, `now`
    /// the time, `playhead` where playback is (`None`: stopped), `offset`
    /// the ruler's start.
    pub fn step(
        &mut self,
        latest: Option<Received>,
        now: Instant,
        playhead: Option<Tick>,
        offset: Tick,
        s: &ChaseSettings,
    ) -> ChaseAction {
        let slack = Duration::from_secs_f64(s.slack_frames.max(1) as f64 / s.rate.fps());
        let fresh = latest.filter(|r| now.saturating_duration_since(r.at) <= slack);
        let Some(r) = fresh else {
            self.status = Some(MtcStatus::Listening);
            self.starting_at = None;
            if !self.following {
                return ChaseAction::Nothing;
            }
            // Lost: free-wheel a while, or stop.
            let lost = *self.lost.get_or_insert(now);
            let wheel = Duration::from_secs_f32(s.freewheel_seconds.max(0.0));
            if s.freewheel && now.saturating_duration_since(lost) < wheel {
                return ChaseAction::Nothing;
            }
            self.following = false;
            self.lost = None;
            return ChaseAction::Stop;
        };
        self.lost = None;
        if r.code != s.rate.code() {
            self.status = Some(MtcStatus::WrongFormat);
            return ChaseAction::Nothing;
        }
        let seconds = r.seconds_at(now, s.rate, s.offset_quarters);
        let frame = (seconds * s.rate.fps()).floor() as i64;
        self.status = Some(MtcStatus::Locked(timecode(frame, s.rate)));
        let target = (Tick::from_seconds_f64(seconds) - offset).max(Tick::ZERO);
        match (playhead, self.starting_at) {
            (None, None) if !self.following => {
                let at = target + Tick::from_seconds_f64(s.sync_delay.max(0.0) as f64);
                self.starting_at = Some(at);
                ChaseAction::Cue(at)
            }
            (None, Some(at)) => {
                if target >= at {
                    self.starting_at = None;
                    self.following = true;
                    ChaseAction::Play(target)
                } else {
                    ChaseAction::Nothing
                }
            }
            (Some(p), _) if self.following => {
                // More than two frames apart: jump.
                let apart = (p - target).abs().as_seconds_f64() * s.rate.fps();
                if apart > 2.0 {
                    ChaseAction::Play(target)
                } else {
                    ChaseAction::Nothing
                }
            }
            (None, None) => {
                // Following, but playback stopped (by hand): start again.
                ChaseAction::Play(target)
            }
            _ => ChaseAction::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_frame_timecode_skips_numbers_as_clocks_do() {
        let df = MtcRate::Drop2997;
        // The first frame of minute one is 00:01:00;02.
        assert_eq!(timecode(1800, df), [0, 1, 0, 2]);
        // Minute ten keeps its 00 and 01.
        assert_eq!(timecode(17_982, df), [0, 10, 0, 0]);
        for frame in [0, 1799, 1800, 17_981, 17_982, 107_892, 123_456] {
            assert_eq!(frame_of(timecode(frame, df), df), frame, "{frame}");
        }
        let nd = MtcRate::NonDrop30;
        assert_eq!(timecode(30 * 3661 + 7, nd), [1, 1, 1, 7]);
        assert_eq!(timecode_text([1, 2, 3, 4], df), "01:02:03;04");
    }

    #[test]
    fn quarter_frames_and_full_frames_carry_the_time() {
        let rate = MtcRate::Ebu25;
        let tc = [1, 23, 45, 19];
        let mut d = MtcDecoder::default();
        let at = Instant::now();
        // From the middle of a sequence: nothing until a whole one.
        for piece in 4..8 {
            d.quarter_frame(quarter_frame(piece, [0, 0, 0, 0], rate)[1], at);
        }
        assert_eq!(d.latest, None);
        for piece in 0..8 {
            let [status, data] = quarter_frame(piece, tc, rate);
            assert_eq!(status, 0xF1);
            d.quarter_frame(data, at);
        }
        assert_eq!(d.latest.map(|r| (r.tc, r.code)), Some((tc, 1)));
        let ff = full_frame([2, 3, 4, 5], MtcRate::Drop2997);
        assert_eq!(ff, [0xF0, 0x7F, 0x7F, 1, 1, 2 << 5 | 2, 3, 4, 5, 0xF7]);
        // Two frames on from the timecode named.
        let r = Received {
            tc: [2, 3, 4, 5],
            code: 2,
            at,
        };
        let df = MtcRate::Drop2997;
        let expected = (frame_of([2, 3, 4, 5], df) + 2) as f64 / df.fps();
        assert!((r.seconds_at(at, df, 0) - expected).abs() < 1e-9);
        assert_eq!(song_position(10.0), [0xF2, 40, 0]);
    }

    #[test]
    fn following_timecode_cues_starts_follows_and_stops() {
        let s = ChaseSettings {
            rate: MtcRate::NonDrop30,
            freewheel: true,
            slack_frames: 6,
            freewheel_seconds: 1.0,
            sync_delay: 0.5,
            offset_quarters: 0,
        };
        let t0 = Instant::now();
        let received = |tc: Timecode, at: Instant| Received { tc, code: 3, at };
        let mut c = Chase::default();
        // Nothing comes: listening.
        assert_eq!(c.step(None, t0, None, Tick::ZERO, &s), ChaseAction::Nothing);
        assert_eq!(c.status, Some(MtcStatus::Listening));
        // 10 s comes in: cued half a second on.
        let r = received([0, 0, 10, 0], t0);
        let ChaseAction::Cue(at) = c.step(Some(r), t0, None, Tick::ZERO, &s) else {
            panic!("cued");
        };
        let two = 2.0 / 30.0;
        assert!((at.as_seconds_f64() - (10.0 + two + 0.5)).abs() < 1e-3);
        // Timecode reaches it: playback starts there.
        let later = t0 + Duration::from_millis(540);
        let r = received([0, 0, 10, 16], later);
        assert!(matches!(
            c.step(Some(r), later, None, Tick::ZERO, &s),
            ChaseAction::Play(_)
        ));
        // Playing in step: nothing to do; far off: a jump.
        let now = later + Duration::from_millis(10);
        let target = Tick::from_seconds_f64(r.seconds_at(now, s.rate, 0));
        assert_eq!(
            c.step(Some(r), now, Some(target), Tick::ZERO, &s),
            ChaseAction::Nothing
        );
        assert!(matches!(
            c.step(Some(r), now, Some(Tick::from_seconds(30)), Tick::ZERO, &s),
            ChaseAction::Play(_)
        ));
        // Timecode stops: free-wheeling for a second, then stopped.
        let gone = now + Duration::from_millis(300);
        assert_eq!(
            c.step(Some(r), gone, Some(target), Tick::ZERO, &s),
            ChaseAction::Nothing
        );
        let long_gone = gone + Duration::from_millis(1100);
        assert_eq!(
            c.step(Some(r), long_gone, Some(target), Tick::ZERO, &s),
            ChaseAction::Stop
        );
        // Another rate: wrong format.
        let mut c = Chase::default();
        let r = Received {
            tc: [0, 0, 1, 0],
            code: 1,
            at: t0,
        };
        assert_eq!(
            c.step(Some(r), t0, None, Tick::ZERO, &s),
            ChaseAction::Nothing
        );
        assert_eq!(c.status, Some(MtcStatus::WrongFormat));
    }

    #[test]
    fn the_sending_thread_counts_quarter_frames_and_pulses() {
        // Without ports: the counting alone, through `Sending::step`'s
        // arithmetic on a transport that plays one second.
        let t0 = Instant::now();
        let t = Transport {
            playing: true,
            position: Tick::ZERO,
            at: t0,
            rate: 1.0,
            offset: Tick::ZERO,
            beats_per_minute: 120.0,
            serial: 1,
        };
        let one = t0 + Duration::from_secs(1);
        assert!((t.seconds(one) - 1.0).abs() < 1e-9);
        // At 30 fps a second is 120 quarter frames, and at 120 bpm 48
        // pulses.
        assert_eq!((t.seconds(one) * 30.0 * 4.0 + 1e-6).floor() as i64, 120);
        assert_eq!((t.seconds(one) * 2.0 * 24.0 + 1e-6).floor() as i64, 48);
        let stopped = Transport {
            playing: false,
            ..t
        };
        assert_eq!(stopped.seconds(one), 0.0, "stopped: where it was");
    }
}
