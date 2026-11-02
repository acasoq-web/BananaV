//! Open video readers, shared by the frames of a preview or a render.
//!
//! Each event gets a reader of its own: two events of one file (a crossfade
//! between two parts of a recording) would otherwise make a single reader
//! seek back and forth on every frame. Readers of video files run on
//! threads of their own, so the pictures of a frame are decoded at the same
//! time, and while a frame is composited they already decode the next ones
//! (playback and rendering go forward frame after frame).

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bv_core::{ImageRgbaF32, Tick, VideoFrame};
use bv_timeline::scene::{FrameScene, MediaLayer};
use bv_timeline::{MediaId, MediaInfo, MediaSource};

use crate::{MediaError, Result, VideoReader};

/// A picture of a media, as a reader gives it.
#[derive(Clone, Debug)]
pub enum Picture {
    /// In linear light (generated media, stills, or converted already).
    Linear(Arc<ImageRgbaF32>),
    /// As decoded: whoever shows it converts it (the graphics card does it
    /// faster than the processor).
    Decoded(Arc<VideoFrame>),
}

/// The form a picture is wanted in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Want {
    /// In linear light, converted by the reader (the processor's compositor).
    Linear,
    /// As decoded when the reader can (the graphics card's compositor).
    Decoded,
}

/// Who uses a reader: an event's id. Pictures asked for outside events
/// (snapshots of a media) use [`NO_EVENT`].
pub type Owner = u64;

pub const NO_EVENT: Owner = u64::MAX;

/// A picture asked of a reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ask {
    pub time: Tick,
    /// The size it shows at (see [`VideoReader::frame_at_scaled`]).
    pub min_size: (u32, u32),
    pub want: Want,
    /// The length of the event that shows it (see
    /// [`VideoReader::set_span`]).
    pub span: Option<Tick>,
    /// Y′CbCr as studio RGB (see [`VideoReader::set_video_levels`]).
    pub video_levels: bool,
}

impl Ask {
    /// What a compositor asks for to draw `m`.
    pub fn of(m: &MediaLayer<'_>, want: Want) -> Ask {
        Ask {
            time: m.time,
            min_size: m.min_size,
            want,
            span: Some(m.length),
            video_levels: m.video_levels,
        }
    }

    /// What a compositor also asks for to draw `m` with Frame Blend
    /// (`MediaLayer::blend`): the media's next frame, and the part of it
    /// mixed in. Its reader is [`blend_owner`]'s.
    pub fn blend(m: &MediaLayer<'_>, want: Want) -> Option<(Ask, f32)> {
        let (time, k) = m.blend?;
        Some((
            Ask {
                time,
                ..Ask::of(m, want)
            },
            k,
        ))
    }
}

/// Who reads an event's Frame Blend pictures (the next frames): a reader
/// of their own, going forward with them as the event's does.
pub fn blend_owner(owner: Owner) -> Owner {
    owner | 1 << 62
}

/// What a reader does next on its way to a picture (see
/// [`VideoReader::picture_on_the_way`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Onward {
    Go,
    /// A newer picture is wanted: show the next picture on the way at once
    /// (it stands in for the newer one), then stop.
    Hurry,
    Stop,
}

/// One picture a frame needs.
#[derive(Clone, Copy, Debug)]
pub struct Need<'a> {
    pub owner: Owner,
    pub info: &'a MediaInfo,
    pub ask: Ask,
}

/// The pictures drawing `m` (owned by `owner`) needs: its frame, and the
/// next one with Frame Blend.
pub fn needs_of<'a>(owner: Owner, m: &MediaLayer<'a>, want: Want) -> Vec<Need<'a>> {
    let mut v = vec![Need {
        owner,
        info: m.info,
        ask: Ask::of(m, want),
    }];
    if let Some((ask, _)) = Ask::blend(m, want) {
        v.push(Need {
            owner: blend_owner(owner),
            info: m.info,
            ask,
        });
    }
    v
}

/// How many times a `width` × `height` picture can be halved (1, 2, 4 or
/// 8) and still cover `min_w` × `min_h`: readers convert decoded pictures
/// that much smaller for previews.
pub fn reduction(width: u32, height: u32, min_w: u32, min_h: u32) -> u32 {
    let mut factor = 1u32;
    while factor < 8 && width / (factor * 2) >= min_w && height / (factor * 2) >= min_h {
        factor *= 2;
    }
    factor
}

/// Whether `BV_PREVIEW_TRACE` is set: long waits for pictures are told.
fn tracing() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("BV_PREVIEW_TRACE").is_some_and(|v| !v.is_empty()))
}

/// Readers of one media at most (more overlapping events of it share them).
const PER_MEDIA: usize = 6;
/// Readers kept open in all.
const MAX_READERS: usize = 16;
/// Frames a reader may go unused before it is closed.
const IDLE_FRAMES: u64 = 600;
/// Pictures a reader decodes ahead of the last one asked for.
const AHEAD: usize = 2;
/// Played backward: the pictures before the one asked for a reader
/// decodes beforehand, a stretch at a time.
const BEHIND: i64 = 8;

/// Played backward — `ask` before `last`, as close as playback asks —:
/// the stretch to decode beforehand, the pictures before `ask` at its
/// step, in the order a decoder goes (one seek, then on); none else.
fn stretch_behind(last: Ask, ask: Ask) -> Vec<Ask> {
    let backward = last.want == ask.want
        && last.min_size == ask.min_size
        && ask.time < last.time
        && last.time - ask.time <= Tick::from_seconds_f64(0.5);
    if !backward {
        return Vec::new();
    }
    let step = last.time - ask.time;
    (1..=BEHIND)
        .rev()
        .map(|k| Ask {
            time: ask.time - Tick(step.0 * k),
            ..ask
        })
        .filter(|a| a.time >= Tick::ZERO)
        .collect()
}

/// Open video readers (see the module's description).
#[derive(Default)]
pub struct VideoPool {
    /// Media with a proxy are read from it (previews below full size).
    proxies: bool,
    slots: Vec<Slot>,
    errors: HashMap<MediaId, MediaError>,
    /// Counts frames ([`begin_frame`](Self::begin_frame)): a reader used in
    /// this frame is not given to another event.
    frame: u64,
    /// How long a picture is waited for when a picture on the way to it
    /// can stand in (see [`set_patience`](Self::set_patience)).
    patience: Option<Instant>,
    /// A picture on the way stood in for one asked for in this frame.
    took_on_the_way: bool,
    /// How long pictures were waited for, in all.
    waited: Duration,
}

impl std::fmt::Debug for VideoPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoPool")
            .field("readers", &self.slots.len())
            .field("errors", &self.errors)
            .finish()
    }
}

struct Slot {
    media: MediaId,
    /// The media's video settings it was opened with (Media Properties'
    /// Rotation, Alpha channel…): other settings open another.
    video: Option<bv_timeline::VideoStreamInfo>,
    owner: Owner,
    /// The last media time asked for: an event that goes on from there
    /// takes this reader rather than one that has to seek.
    at: Option<Tick>,
    /// The frame it was last used in.
    used: u64,
    reader: Reader,
}

enum Reader {
    /// Cheap readers (generated media, still pictures) answer at once.
    Here(Box<dyn VideoReader>),
    /// Readers of video files work on a thread of their own.
    Thread(Worker),
}

/// What a reader's thread answers: the picture, or an error (`true` when
/// the media could not be opened at all).
type Answer = std::result::Result<Picture, (MediaError, bool)>;

/// A picture collected: the one asked for, or one on the way that stands
/// in while the reader goes on.
enum Got {
    Done(Answer),
    OnTheWay(Picture),
}

type Job = (Ask, mpsc::Sender<Answer>);

struct Worker {
    jobs: Option<mpsc::Sender<Job>>,
    /// The picture asked for and not collected yet.
    pending: Option<(Ask, mpsc::Receiver<Answer>)>,
    /// The latest picture the thread showed on the way: to the picture
    /// asked for, or to an older one given up for it (while scrubbing, the
    /// older seek shows one picture before it stops).
    shown: Arc<std::sync::Mutex<Option<Picture>>>,
    /// Set when a newer picture is asked for: the thread stops decoding
    /// toward the older one (scrubbing asks faster than seeks finish).
    interrupt: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Closing the queue ends the thread once its picture is done.
        self.interrupt.store(true, Ordering::Relaxed);
        self.jobs = None;
        self.pending = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Worker {
    fn start(info: MediaInfo) -> Result<Worker> {
        let (tx, rx) = mpsc::channel();
        let interrupt = Arc::new(AtomicBool::new(false));
        let shown = Arc::new(std::sync::Mutex::new(None));
        let (flag, mailbox) = (interrupt.clone(), shown.clone());
        let thread = std::thread::Builder::new()
            .name(format!("bv-reader-{}", info.id.0))
            .spawn(move || work(&info, &rx, &flag, &mailbox))
            .map_err(|e| MediaError::Io(e.to_string()))?;
        Ok(Worker {
            jobs: Some(tx),
            pending: None,
            shown,
            interrupt,
            thread: Some(thread),
        })
    }

    fn send(&mut self, ask: Ask) {
        if self.pending.as_ref().is_some_and(|(a, _)| *a == ask) {
            return;
        }
        if self.pending.is_some() {
            // The older picture is not wanted any more.
            self.interrupt.store(true, Ordering::Relaxed);
        }
        let (tx, rx) = mpsc::channel();
        if let Some(jobs) = &self.jobs
            && jobs.send((ask, tx)).is_ok()
        {
            self.pending = Some((ask, rx));
        }
    }

    /// The picture asked for; or, when `until` passes first, the latest
    /// picture the thread showed on the way (the answer keeps coming).
    fn take(&mut self, ask: Ask, until: Option<Instant>) -> Got {
        let stopped = || Got::Done(Err((MediaError::Io("the reader stopped".into()), true)));
        self.send(ask);
        let Some((_, rx)) = &self.pending else {
            return stopped();
        };
        let answer = match until {
            None => rx.recv(),
            Some(deadline) => loop {
                // After the deadline, a picture on the way is taken as soon
                // as there is one.
                let wait = deadline
                    .saturating_duration_since(Instant::now())
                    .max(std::time::Duration::from_millis(4));
                match rx.recv_timeout(wait) {
                    Ok(a) => break Ok(a),
                    Err(mpsc::RecvTimeoutError::Disconnected) => break Err(mpsc::RecvError),
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if Instant::now() >= deadline
                            && let Some(p) = self.shown.lock().ok().and_then(|s| s.clone())
                        {
                            return Got::OnTheWay(p);
                        }
                    }
                }
            },
        };
        self.pending = None;
        match answer {
            Ok(a) => Got::Done(a),
            Err(_) => stopped(),
        }
    }
}

fn fetch(reader: &mut dyn VideoReader, ask: Ask) -> Result<Picture> {
    reader.set_span(ask.span);
    reader.set_video_levels(ask.video_levels);
    let (w, h) = ask.min_size;
    match ask.want {
        Want::Linear => reader.frame_at_scaled(ask.time, w, h).map(Picture::Linear),
        Want::Decoded => reader.picture_at(ask.time, w, h),
    }
}

/// A reader's thread: answers the pictures asked for and, while nothing is
/// asked, decodes the next ones when the last were asked in steps forward.
fn work(
    info: &MediaInfo,
    jobs: &mpsc::Receiver<Job>,
    interrupt: &AtomicBool,
    shown: &std::sync::Mutex<Option<Picture>>,
) {
    let mut reader = match crate::open_video(info) {
        Ok(r) => r,
        Err(e) => {
            for (_, reply) in jobs {
                let _ = reply.send(Err((e.clone(), true)));
            }
            return;
        }
    };
    let mut ahead: VecDeque<(Ask, Result<Picture>)> = VecDeque::new();
    let mut last: Option<Ask> = None;
    // While pictures are asked for going forward (playback, rendering),
    // the media's next frames are decoded before they are asked for: one
    // frame after another, never further, since a picture asked for before
    // where the decoder stands makes it seek back to a keyframe.
    let frame = info
        .video
        .map_or(Tick::ZERO, |v| v.frame_rate.frame_duration());
    let mut forward = false;
    // Played backward, the stretch before the picture asked for is decoded
    // beforehand (`plan`, one picture each time nothing is asked) and kept
    // until it is asked for (`behind`).
    let mut plan: VecDeque<Ask> = VecDeque::new();
    let mut behind: Vec<(Ask, Result<Picture>)> = Vec::new();
    loop {
        let job = match jobs.try_recv() {
            Ok(job) => job,
            Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {
                // Nothing asked: decode the next picture, if any is due.
                let next = match ahead.back().map(|(a, _)| *a).or(last) {
                    Some(from) if forward && frame > Tick::ZERO && ahead.len() < AHEAD => {
                        Some(Ask {
                            time: from.time + frame,
                            ..from
                        })
                    }
                    _ => None,
                };
                if let Some(next) = next {
                    let r = fetch(reader.as_mut(), next);
                    let failed = r.is_err();
                    ahead.push_back((next, r));
                    if !failed {
                        continue;
                    }
                }
                if let Some(next) = plan.pop_front() {
                    let r = fetch(reader.as_mut(), next);
                    behind.push((next, r));
                    continue;
                }
                match jobs.recv() {
                    Ok(job) => job,
                    Err(_) => break,
                }
            }
        };
        // Only the latest picture asked for counts: older ones were given up.
        let (mut ask, mut reply) = job;
        while let Ok(newer) = jobs.try_recv() {
            (ask, reply) = newer;
        }
        interrupt.store(false, Ordering::Relaxed);
        // Pictures decoded ahead that were passed by are dropped.
        while ahead.front().is_some_and(|(a, _)| *a != ask) {
            let behind = ahead.front().is_some_and(|(a, _)| {
                a.want == ask.want && a.min_size == ask.min_size && a.time < ask.time
            });
            if behind {
                ahead.pop_front();
            } else {
                ahead.clear();
            }
        }
        // Pictures decoded behind that playback went past are dropped.
        behind.retain(|(a, _)| a.time <= ask.time);
        let kept = behind
            .iter()
            .position(|(a, _)| *a == ask)
            .map(|k| behind.swap_remove(k).1);
        let result = match ahead.pop_front().map(|(_, r)| r).or(kept) {
            Some(r) => r,
            None => {
                // A long decode shows its way; when a newer picture is asked
                // for, it shows one more (that stands in for it) and stops.
                let (w, h) = ask.min_size;
                let mut shown_any = false;
                reader.set_span(ask.span);
                reader.set_video_levels(ask.video_levels);
                reader.picture_on_the_way(ask.time, w, h, ask.want, &mut |p| {
                    if let Some(p) = p {
                        if let Ok(mut s) = shown.lock() {
                            *s = Some(p);
                        }
                        shown_any = true;
                    }
                    match (interrupt.load(Ordering::Relaxed), shown_any) {
                        (false, _) => Onward::Go,
                        (true, false) => Onward::Hurry,
                        (true, true) => Onward::Stop,
                    }
                })
            }
        };
        forward = last.is_some_and(|l| {
            l.want == ask.want
                && l.min_size == ask.min_size
                && ask.time > l.time
                && ask.time - l.time <= Tick::from_seconds_f64(0.5)
        });
        if !forward {
            ahead.clear();
        }
        // Backward: the stretch before it, once the last one is used up.
        let stretch = last.map_or_else(Vec::new, |l| stretch_behind(l, ask));
        if stretch.is_empty() {
            behind.clear();
            plan.clear();
        } else if plan.is_empty() && !behind.iter().any(|(a, _)| a.time < ask.time) {
            plan.extend(stretch);
        }
        last = Some(ask);
        if result.is_ok()
            && let Ok(mut s) = shown.lock()
        {
            // The picture is there: older pictures on the way are stale.
            *s = None;
        }
        let _ = reply.send(result.map_err(|e| (e, false)));
    }
    // Media Foundation's objects go before the thread ends.
    drop(ahead);
    drop(reader);
}

/// Whether a media's reader works on a thread of its own.
fn threaded(info: &MediaInfo) -> bool {
    !cfg!(target_arch = "wasm32") && !info.still && matches!(info.source, MediaSource::File { .. })
}

impl VideoPool {
    pub fn new() -> VideoPool {
        VideoPool::default()
    }

    /// Whether media with a proxy are read from it (previews below full
    /// size); the readers open again when this changes.
    pub fn set_proxies(&mut self, on: bool) {
        if on != self.proxies {
            self.clear();
            self.proxies = on;
        }
    }

    /// Starts a new frame: readers used in it are not given to other
    /// events. Readers unused for long are closed.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        self.took_on_the_way = false;
        let frame = self.frame;
        self.slots.retain(|s| frame - s.used <= IDLE_FRAMES);
        while self.slots.len() > MAX_READERS {
            let Some(i) = self
                .slots
                .iter()
                .enumerate()
                .filter(|(_, s)| s.used < frame)
                .min_by_key(|(_, s)| s.used)
                .map(|(i, _)| i)
            else {
                break;
            };
            self.slots.remove(i);
        }
    }

    /// Starts reading every picture a frame needs (returns at once; the
    /// pictures are collected with [`picture`](Self::picture)). The events
    /// that already have a reader keep it; the others get one.
    pub fn prefetch(&mut self, needs: &[Need<'_>]) {
        if tracing() {
            for n in needs {
                eprintln!(
                    "reader of {} (event {}): asked {:.3} s ahead",
                    n.info.name,
                    n.owner,
                    n.ask.time.as_seconds_f64()
                );
            }
        }
        for n in needs {
            if let Some(s) = self
                .slots
                .iter_mut()
                .find(|s| s.media == n.info.id && s.owner == n.owner)
            {
                s.used = self.frame;
            }
        }
        for n in needs {
            if self.errors.contains_key(&n.info.id) {
                continue;
            }
            if let Some(i) = self.slot(n.owner, n.info, n.ask.time) {
                let s = &mut self.slots[i];
                s.at = Some(n.ask.time);
                if let Reader::Thread(w) = &mut s.reader {
                    w.send(n.ask);
                }
            }
        }
    }

    /// Starts a new frame and asks the readers for every picture `scene`
    /// needs, as `want`, all at once (they are decoded at the same time).
    pub fn prefetch_scene(&mut self, scene: &FrameScene<'_>, want: Want) {
        let needs: Vec<Need<'_>> = scene
            .media_layers()
            .flat_map(|(clip, m)| needs_of(clip.0, m, want))
            .collect();
        self.begin_frame();
        self.prefetch(&needs);
    }

    /// From now on, a picture whose reader has to decode for long (a seek)
    /// is waited for until `until` at most: then a picture on the way to it
    /// stands in, if the reader showed one (scrubbing). `None`: pictures
    /// are always waited for.
    pub fn set_patience(&mut self, until: Option<Instant>) {
        self.patience = until;
    }

    /// Whether a picture on the way stood in for one asked for since
    /// [`begin_frame`](Self::begin_frame): the frame is not exact yet.
    pub fn took_on_the_way(&self) -> bool {
        self.took_on_the_way
    }

    /// How long pictures of files were waited for in all (their readers
    /// decoding or seeking): what a frame waited is the difference.
    pub fn waited(&self) -> Duration {
        self.waited
    }

    /// The picture of `owner`'s media at `ask.time` (waits for it).
    pub fn picture(&mut self, owner: Owner, info: &MediaInfo, ask: Ask) -> Result<Picture> {
        if let Some(e) = self.errors.get(&info.id) {
            return Err(e.clone());
        }
        let Some(i) = self.slot(owner, info, ask.time) else {
            return Err(self
                .errors
                .get(&info.id)
                .cloned()
                .unwrap_or(MediaError::NoStream));
        };
        let patience = self.patience;
        let s = &mut self.slots[i];
        s.at = Some(ask.time);
        let start = Instant::now();
        let got = match &mut s.reader {
            Reader::Here(r) => return fetch(r.as_mut(), ask),
            Reader::Thread(w) => w.take(ask, patience),
        };
        let waited = start.elapsed();
        self.waited += waited;
        if waited > Duration::from_millis(20) && tracing() {
            eprintln!(
                "reader of {} (event {owner}): {:.1} ms for {:.3} s",
                info.name,
                waited.as_secs_f64() * 1e3,
                ask.time.as_seconds_f64()
            );
        }
        let answer = match got {
            Got::Done(a) => a,
            Got::OnTheWay(p) => {
                self.took_on_the_way = true;
                return Ok(p);
            }
        };
        match answer {
            Ok(p) => Ok(p),
            Err((e, fatal)) => {
                if fatal {
                    self.errors.insert(info.id, e.clone());
                    self.slots.remove(i);
                }
                Err(e)
            }
        }
    }

    /// The media's frame at `t` in linear light, covering at least
    /// `min_w` × `min_h` (it may come reduced), outside events.
    pub fn frame_scaled(
        &mut self,
        info: &MediaInfo,
        t: Tick,
        min_w: u32,
        min_h: u32,
    ) -> Result<Arc<ImageRgbaF32>> {
        let ask = Ask {
            time: t,
            min_size: (min_w, min_h),
            want: Want::Linear,
            span: None,
            video_levels: false,
        };
        match self.picture(NO_EVENT, info, ask)? {
            Picture::Linear(img) => Ok(img),
            Picture::Decoded(f) => Ok(Arc::new(bv_color::frame_to_linear(&f))),
        }
    }

    /// The media's frame at `t`, full size, in linear light.
    pub fn frame(&mut self, info: &MediaInfo, t: Tick) -> Result<Arc<ImageRgbaF32>> {
        self.frame_scaled(info, t, u32::MAX, u32::MAX)
    }

    /// Forgets readers and errors (after a media is reconnected, say).
    pub fn clear(&mut self) {
        self.slots.clear();
        self.errors.clear();
    }

    pub fn error(&self, id: MediaId) -> Option<&MediaError> {
        self.errors.get(&id)
    }

    /// How many readers are open (tests).
    pub fn open_readers(&self) -> usize {
        self.slots.len()
    }

    /// Whether `owner` has a reader of `media` open.
    pub fn has_reader(&self, owner: Owner, media: MediaId) -> bool {
        self.slots
            .iter()
            .any(|s| s.media == media && s.owner == owner)
    }

    /// Whether `owner` has a reader of `media` there for its picture at
    /// `time` (asked for it, or just before it): one left further on —
    /// the event played before, then played again — would seek back.
    pub fn ready_at(&self, owner: Owner, media: MediaId, time: Tick) -> bool {
        self.slots.iter().any(|s| {
            s.media == media
                && s.owner == owner
                && s.at
                    .is_some_and(|at| at <= time && time - at < Tick::from_seconds_f64(0.1))
        })
    }

    /// The reader `owner` uses for `info` at `time`, opening one if needed
    /// (`None` when the media cannot be opened).
    fn slot(&mut self, owner: Owner, info: &MediaInfo, time: Tick) -> Option<usize> {
        let frame = self.frame;
        // Readers of the media opened before its settings changed.
        self.slots
            .retain(|s| s.media != info.id || s.video == info.video);
        if let Some(i) = self
            .slots
            .iter()
            .position(|s| s.media == info.id && s.owner == owner)
        {
            self.slots[i].used = frame;
            return Some(i);
        }
        // A reader of the media no event used in this frame: the one that
        // goes on without seeking, else the one unused for longest.
        let goes_on = |s: &Slot| {
            s.at.is_some_and(|at| at <= time && time - at < Tick::from_seconds(2))
        };
        let free = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.media == info.id && s.used < frame)
            .min_by_key(|(_, s)| (!goes_on(s), s.used))
            .map(|(i, _)| i);
        let count = self.slots.iter().filter(|s| s.media == info.id).count();
        let i = match free {
            Some(i) => i,
            None if count < PER_MEDIA => {
                let source = match self.proxies {
                    true => crate::proxied(info).unwrap_or_else(|| info.clone()),
                    false => info.clone(),
                };
                let reader = if threaded(&source) {
                    Worker::start(source).map(Reader::Thread)
                } else {
                    crate::open_video(&source).map(Reader::Here)
                };
                match reader {
                    Ok(reader) => {
                        self.slots.push(Slot {
                            media: info.id,
                            video: info.video,
                            owner,
                            at: None,
                            used: frame,
                            reader,
                        });
                        return Some(self.slots.len() - 1);
                    }
                    Err(e) => {
                        self.errors.insert(info.id, e);
                        return None;
                    }
                }
            }
            // Too many events of one media at once: they share.
            None => self
                .slots
                .iter()
                .enumerate()
                .filter(|(_, s)| s.media == info.id)
                .min_by_key(|(_, s)| s.used)
                .map(|(i, _)| i)?,
        };
        let s = &mut self.slots[i];
        s.owner = owner;
        s.used = frame;
        Some(i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Played backward a frame at a time, the eight frames before are
    /// planned from the first; forward, or a jump, plans nothing.
    #[test]
    fn played_backward_the_stretch_before_is_planned() {
        let f = Tick(Tick::from_seconds(1).0 / 25);
        let at = |n: i64| Ask {
            time: Tick(f.0 * n),
            min_size: (480, 270),
            want: Want::Linear,
            span: None,
            video_levels: false,
        };
        let plan = stretch_behind(at(100), at(99));
        let times: Vec<i64> = plan.iter().map(|a| a.time.0 / f.0).collect();
        assert_eq!(times, vec![91, 92, 93, 94, 95, 96, 97, 98]);
        assert!(stretch_behind(at(99), at(100)).is_empty(), "forward");
        assert!(stretch_behind(at(100), at(10)).is_empty(), "a jump");
        // Near the start, nothing before it.
        assert_eq!(stretch_behind(at(4), at(3)).len(), 3);
    }
    use bv_timeline::{Generator, ProjectSettings};

    #[test]
    fn events_get_readers_of_their_own() {
        let settings = ProjectSettings::default();
        let mut info = MediaInfo::generated(Generator::ColorBars, &settings);
        info.id = MediaId(7);
        let mut pool = VideoPool::new();
        let ask = Ask {
            time: Tick::ZERO,
            min_size: (32, 18),
            want: Want::Linear,
            span: None,
            video_levels: false,
        };
        let need = |owner| Need {
            owner,
            info: &info,
            ask,
        };
        pool.begin_frame();
        pool.prefetch(&[need(1), need(2)]);
        assert_eq!(pool.open_readers(), 2, "two events of one media");
        assert!(pool.picture(1, &info, ask).is_ok());
        // Next frame: the same two keep theirs.
        pool.begin_frame();
        pool.prefetch(&[need(2), need(1)]);
        assert_eq!(pool.open_readers(), 2);
        // A third event later takes a reader the others left.
        pool.begin_frame();
        pool.prefetch(&[need(3)]);
        assert_eq!(pool.open_readers(), 2);
        // Unused readers close.
        for _ in 0..=IDLE_FRAMES {
            pool.begin_frame();
        }
        assert_eq!(pool.open_readers(), 0);
    }
}
