//! The preview's frames, drawn on threads of their own, each with its own
//! painter and media readers. Only the latest request matters: older ones
//! not started are given up.
//!
//! During playback frames are drawn for the moment they will show: the
//! frame the playing position will have reached when it is done (how long
//! frames take is measured as they are drawn). A preview slower than the
//! project's rate so drops frames evenly, instead of drawing frames that
//! arrive after their time while the ones due wait. When one thread cannot
//! keep up drawing on the graphics card and the frames do not wait on the
//! media's decoders (effects do the work), a second thread draws alternate
//! frames at once, as long as that shows more of them (some plug-ins
//! render one frame at a time anyway, and the second thread's readers
//! decode every frame too). The frames asked for — a still, scrubbing,
//! playback's first — and Dynamic RAM Previews are the first thread's:
//! its readers are where they go on from.
//!
//! Scrubbing: a frame whose decoders must seek (a long-GOP recording has a
//! keyframe every few seconds) does not freeze the preview — pictures on
//! the way to it show meanwhile, and a newer position stops the older
//! seek. Exact frames drawn lately are kept: going back over them shows
//! them at once.
//!
//! `BV_PREVIEW_TRACE=1` prints each frame drawn: the thread, why, its time,
//! how long it took and how much of that it waited for the media's
//! pictures.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bv_core::Tick;
use bv_render::VideoPool;
use bv_timeline::{MediaInfo, Project, RenderQuality};

use crate::RepaintFn;
use crate::accel::{Accel, Painter};

/// A frame ready for the screen.
#[derive(Debug)]
pub struct PreviewFrame {
    pub time: Tick,
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    /// RGBA, 8 bits, encoded in BT.709 (as renders write it).
    pub rgba: Vec<u8>,
    /// `false` while scrubbing, when pictures on the way to the frame stood
    /// in for some of its pictures (the exact frame follows).
    pub exact: bool,
}

/// One channel of the picture, as the Video Preview's Overlays menu shows
/// it: the color's alone, or as grey.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Channel {
    #[default]
    All,
    Red,
    Green,
    Blue,
    RedGray,
    GreenGray,
    BlueGray,
    /// The alpha channel (what is transparent shows black).
    AlphaGray,
}

/// How the Video Preview draws its frames: its quality, its Split-Screen
/// View and the channel its overlays show.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreviewLook {
    /// How pictures are scaled (`None`: as the project renders them):
    /// Draft takes the nearest pixel, Preview and Good blend, Best is
    /// bicubic.
    pub quality: Option<RenderQuality>,
    /// Split-Screen View with FX Bypassed: the part of the frame (left,
    /// top, right, bottom, 0–1) drawn with every video effect bypassed.
    pub bypass: Option<[f32; 4]>,
    pub channel: Channel,
}

/// What frames are drawn with besides the project: the look and, for
/// Split-Screen View, the project with its video effects bypassed.
#[derive(Clone, Default)]
pub(crate) struct Drawing {
    pub(crate) look: PreviewLook,
    pub(crate) bypassed: Option<Arc<Project>>,
}

/// A frame for the screen as `drawing` says, and whether it is exact (no
/// picture on the way stood in).
fn paint(
    painter: &mut Painter,
    project: &Project,
    drawing: &Drawing,
    t: Tick,
    size: (u32, u32),
    pool: &mut VideoPool,
) -> (Vec<u8>, bool) {
    let look = drawing.look;
    // The alpha channel only shows over a transparent background.
    let background = if look.channel == Channel::AlphaGray {
        [0.0; 4]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };
    let mut rgba = painter.display_as(project, t, size, pool, look.quality, background);
    let mut exact = !pool.took_on_the_way();
    if let (Some(area), Some(bypassed)) = (look.bypass, &drawing.bypassed) {
        let plain = painter.display_as(bypassed, t, size, pool, look.quality, background);
        exact &= !pool.took_on_the_way();
        copy_area(&mut rgba, &plain, size, area);
    }
    isolate(&mut rgba, look.channel);
    (rgba, exact)
}

/// Copies the part `area` (left, top, right, bottom; 0–1) of `from` into
/// `into`, both `width` × `height` RGBA.
fn copy_area(into: &mut [u8], from: &[u8], (width, height): (u32, u32), area: [f32; 4]) {
    let (w, h) = (width as usize, height as usize);
    let at = |v: f32, n: usize| ((v.clamp(0.0, 1.0) * n as f32).round() as usize).min(n);
    let (x0, x1) = (at(area[0], w), at(area[2], w));
    let (y0, y1) = (at(area[1], h), at(area[3], h));
    if x0 >= x1 || y0 >= y1 || into.len() < w * h * 4 || from.len() < w * h * 4 {
        return;
    }
    for y in y0..y1 {
        let row = (y * w + x0) * 4..(y * w + x1) * 4;
        into[row.clone()].copy_from_slice(&from[row]);
    }
}

/// Keeps one channel of the picture.
fn isolate(rgba: &mut [u8], channel: Channel) {
    if channel == Channel::All {
        return;
    }
    for px in rgba.as_chunks_mut::<4>().0 {
        let [r, g, b, a] = *px;
        *px = match channel {
            Channel::All => [r, g, b, a],
            Channel::Red => [r, 0, 0, 255],
            Channel::Green => [0, g, 0, 255],
            Channel::Blue => [0, 0, b, 255],
            Channel::RedGray => [r, r, r, 255],
            Channel::GreenGray => [g, g, g, 255],
            Channel::BlueGray => [b, b, b, 255],
            Channel::AlphaGray => [a, a, a, 255],
        };
    }
}

/// How long a frame asked for while scrubbing is waited for before a
/// picture on the way to it shows.
const PATIENCE: Duration = Duration::from_millis(25);

/// Bytes of exact frames kept to show again at once, until the
/// preferences say (Options › Preferences › Video: "Dynamic RAM Preview
/// max").
const KEEP: usize = 160 << 20;

/// Threads drawing a preview's frames.
const WORKERS: usize = 2;

/// How far ahead of the playing position frames are drawn at most, in
/// seconds of playing.
const LOOKAHEAD: f64 = 1.0;

/// Playback's frames a second thread is tried for, from its first frame
/// on (its readers start cold: that one is not counted), before it is kept
/// (it showed more frames) or let go.
const TRIAL: usize = 12;

/// How long a second thread's first frame is waited for in a trial.
const FIRST_FRAME: Duration = Duration::from_secs(3);

/// How long playback goes on with one thread after a second one showed no
/// more frames.
const RETRY: Duration = Duration::from_secs(30);

/// How far ahead of what is drawn the plug-ins of events starting are
/// readied (making a plug-in's instance and drawing its first frame may
/// take a couple of seconds: Sapphire's).
const READY_AHEAD: f64 = 5.0;

/// Whether `BV_PREVIEW_TRACE` is set (see the module's description).
fn tracing() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("BV_PREVIEW_TRACE").is_some_and(|v| !v.is_empty()))
}

/// The prerendered parts the preview reads: whether each still shows what
/// the project does (found again at each edit) and the project each plays
/// as.
#[derive(Default)]
struct Prerendered {
    generation: u64,
    valid: std::collections::HashMap<usize, bool>,
    projects: std::collections::HashMap<String, Option<Arc<Project>>>,
}

impl Prerendered {
    /// The project to draw `t` of `project` from: a prerendered part's,
    /// when one shows `t` as the project does.
    fn source(&mut self, project: &Arc<Project>, generation: u64, t: Tick) -> Arc<Project> {
        if project.prerenders.is_empty() {
            return project.clone();
        }
        if generation != self.generation {
            self.generation = generation;
            self.valid.clear();
        }
        for (i, part) in project.prerenders.iter().enumerate() {
            if !part.range.contains(t) {
                continue;
            }
            let ok = *self.valid.entry(i).or_insert_with(|| {
                project.fingerprint(part.range) == part.fingerprint
                    && std::path::Path::new(&part.file).exists()
            });
            if !ok {
                continue;
            }
            let p = self
                .projects
                .entry(part.file.clone())
                .or_insert_with(|| crate::prerender::part_project(project, part).map(Arc::new));
            if let Some(p) = p {
                return p.clone();
            }
        }
        project.clone()
    }
}

/// A Dynamic RAM Preview being built: every frame of a range drawn into the
/// frames kept, so that it plays at full rate.
struct Build {
    project: Arc<Project>,
    generation: u64,
    drawing: Drawing,
    frames: VecDeque<Tick>,
    width: u32,
    height: u32,
    done: usize,
    total: usize,
    /// Its frames being drawn.
    drawn_now: usize,
}

/// How a Dynamic RAM Preview build goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildProgress {
    /// Frames drawn of all.
    Building(usize, usize),
    /// Done: the frames drawn (fewer than all when the memory was full).
    Done(usize, usize),
}

#[derive(Clone)]
struct Request {
    project: Arc<Project>,
    /// The project's generation, told apart by the look too.
    generation: u64,
    drawing: Drawing,
    time: Tick,
    width: u32,
    height: u32,
    /// The frame after it, while playing forward.
    ahead: Option<Tick>,
    /// Counts the frames asked for: an older request's frame never
    /// replaces a newer one's.
    serial: u64,
}

impl Request {
    /// The frame asked for, exactly.
    fn same_as(&self, f: &PreviewFrame) -> bool {
        f.exact && self.matches(f, self.time)
    }

    fn matches(&self, f: &PreviewFrame, time: Tick) -> bool {
        self.of_this(f) && time == f.time
    }

    /// A frame of this project and size.
    fn of_this(&self, f: &PreviewFrame) -> bool {
        self.generation == f.generation && self.width == f.width && self.height == f.height
    }

    fn key(&self, time: Tick) -> Key {
        Key {
            generation: self.generation,
            time,
            width: self.width,
            height: self.height,
        }
    }
}

/// A frame by its project's generation, time and size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key {
    generation: u64,
    time: Tick,
    width: u32,
    height: u32,
}

impl Key {
    /// Of the project and size `req` asks for.
    fn of(&self, req: &Request) -> bool {
        self.generation == req.generation && self.width == req.width && self.height == req.height
    }
}

/// How playback's frames have been drawn lately, which plans the next ones.
#[derive(Default)]
struct Pace {
    /// Seconds a frame takes to draw (a moving average; 0: not known).
    cost: f64,
    /// The part of it spent waiting for the media's pictures (0–1).
    waiting: f64,
    /// The last frames' costs (seconds), the newest last.
    recent: VecDeque<f64>,
    /// Two threads draw ahead at once (else one).
    two: bool,
    /// When playback's frames were done lately, the newest last.
    done: VecDeque<Instant>,
    /// A second thread being tried.
    trial: Option<Trial>,
    /// No second thread before then (it showed no more frames).
    not_before: Option<Instant>,
}

/// A second thread being tried for playback.
#[derive(Clone, Copy, Debug)]
struct Trial {
    /// Frames a second one thread showed.
    one: f64,
    since: Instant,
    /// Frames done since the second thread's first.
    after: Option<usize>,
}

impl Pace {
    fn lanes(&self) -> usize {
        if self.two { 2 } else { 1 }
    }

    /// A playback frame took `took`, `waited` of it for its pictures
    /// (drawn by the `second` thread).
    fn note(&mut self, took: Duration, waited: Duration, second: bool) {
        let secs = took.as_secs_f64().max(1e-4);
        let wait = (waited.as_secs_f64() / secs).min(1.0);
        if self.cost == 0.0 {
            (self.cost, self.waiting) = (secs, wait);
        } else {
            self.cost += 0.3 * (secs - self.cost);
            self.waiting += 0.3 * (wait - self.waiting);
        }
        self.recent.push_back(secs);
        while self.recent.len() > 7 {
            self.recent.pop_front();
        }
        self.done.push_back(Instant::now());
        while self.done.len() > TRIAL + 1 {
            self.done.pop_front();
        }
        if let Some(t) = &mut self.trial {
            t.after = match t.after {
                Some(n) => Some(n + 1),
                None => second.then_some(0),
            };
        }
    }

    /// What frames cost lately, one slow frame aside (a median).
    fn typical(&self) -> f64 {
        let mut costs: Vec<f64> = self.recent.iter().copied().collect();
        costs.sort_by(f64::total_cmp);
        costs.get(costs.len() / 2).copied().unwrap_or(self.cost)
    }

    /// Frames done a second over the last `n` (`None` until there are
    /// that many).
    fn speed(&self, n: usize) -> Option<f64> {
        let last = *self.done.back()?;
        let first = *self.done.get(self.done.len().checked_sub(n + 1)?)?;
        Some(n as f64 / last.duration_since(first).as_secs_f64().max(1e-6))
    }

    /// One thread or two for frames shown every `interval` seconds; the
    /// new choice, when it changed. Two when one cannot keep up drawing
    /// `on_the_card` (on the processor a frame keeps its threads busy) and
    /// the frames do not wait on the media's decoders (the second thread's
    /// readers would decode every frame as well), kept while they show
    /// more frames than one did, and let go once one would keep up.
    fn plan(&mut self, interval: f64, on_the_card: bool) -> Option<bool> {
        let before = self.two;
        if !on_the_card || self.waiting > 0.5 || self.done.len() < 4 {
            self.two = false;
            self.trial = None;
        } else if !self.two {
            let behind = self.typical() > interval * 1.05;
            let may = self.not_before.is_none_or(|t| Instant::now() >= t);
            if behind && may {
                self.trial = Some(Trial {
                    one: 1.0 / self.typical(),
                    since: Instant::now(),
                    after: None,
                });
                self.two = true;
            }
        } else if let Some(t) = self.trial {
            let judged = t.after.is_some_and(|n| n >= TRIAL);
            let late = t.after.is_none() && t.since.elapsed() > FIRST_FRAME;
            if judged || late {
                self.trial = None;
                if late || !self.speed(TRIAL).is_some_and(|two| two > t.one * 1.1) {
                    self.two = false;
                    self.not_before = Some(Instant::now() + RETRY);
                }
            }
        } else if self.typical() < interval * 0.7 {
            self.two = false;
        }
        (before != self.two).then_some(self.two)
    }

    /// Playback stopped: the next one starts afresh, with one thread,
    /// drawing frames one after another until it knows what they take.
    fn stop(&mut self) {
        *self = Pace {
            not_before: self.not_before,
            ..Pace::default()
        };
    }
}

/// Frames whose plug-ins a thread readied: the project, the times they
/// are between and their size.
type Readied = (Arc<Project>, (Tick, Tick), (u32, u32));

#[derive(Default)]
struct State {
    /// The latest request.
    asked: Option<Request>,
    /// Counts the frames asked for (asked again, a frame keeps its number).
    serial: u64,
    /// The newest request whose frame a thread took.
    taken: u64,
    latest: Option<Arc<PreviewFrame>>,
    /// The request the latest frame answered.
    latest_serial: u64,
    /// Frames drawn ahead while playing, the earliest first.
    ahead: VecDeque<Arc<PreviewFrame>>,
    /// Playback's frames being drawn.
    drawing: Vec<Key>,
    /// Exact frames drawn lately, the newest last.
    kept: VecDeque<Arc<PreviewFrame>>,
    /// How many bytes of them are kept.
    keep_bytes: Option<usize>,
    build: Option<Build>,
    built: Option<BuildProgress>,
    /// Counts the requests to close the media files (each thread closes
    /// its own).
    release: u64,
    /// The frames whose plug-ins a thread readied last, so that the others
    /// ready their own (each thread draws with instances of its own), and
    /// how many times one did.
    ready: Option<Readied>,
    readied: u64,
    quit: bool,
    pace: Pace,
    /// The still frame that kept the input of the effect an FX window
    /// shows last (generation, time, size) and that effect's number then
    /// (`plugins::watching`).
    inputs_for: Option<(u64, Tick, u32, u32, u64)>,
}

impl State {
    fn budget(&self) -> usize {
        self.keep_bytes.unwrap_or(KEEP)
    }

    fn kept_bytes(&self) -> usize {
        self.kept.iter().map(|k| k.rgba.len()).sum()
    }

    fn keep(&mut self, f: &Arc<PreviewFrame>) {
        if !f.exact || self.kept.iter().any(|k| Arc::ptr_eq(k, f)) {
            return;
        }
        self.kept.push_back(f.clone());
        let mut bytes = self.kept_bytes();
        while bytes > self.budget()
            && let Some(old) = self.kept.pop_front()
        {
            bytes -= old.rgba.len();
        }
    }

    /// Shows `f`, the answer to request `serial`, unless a newer request's
    /// frame shows already.
    fn show(&mut self, f: Arc<PreviewFrame>, serial: u64) {
        if serial >= self.latest_serial {
            self.latest = Some(f);
            self.latest_serial = serial;
        }
    }

    /// The next frame to draw ahead for `req` (playing): while frames keep
    /// up, the next one; else the one showing when it is done — the frame
    /// the position will be in then, so that a frame only a little late
    /// still shows rather than being skipped —; after every frame of this
    /// playback drawn, being drawn or shown, no further than `LOOKAHEAD`.
    fn next_ahead(&self, req: &Request) -> Option<Tick> {
        let next = req.ahead?;
        let step = Tick((next - req.time).0.max(1));
        let rate = req.project.settings.frame_rate;
        // Seconds of the timeline a second of playing shows.
        let speed = step.0 as f64 / rate.frame_duration().0.max(1) as f64;
        // A little longer than frames take lately, and the screen's
        // refresh.
        let lead = Tick::from_seconds_f64((self.pace.cost * 1.25 + 0.008) * speed);
        // Frames slower than `LOOKAHEAD` are still drawn, for when they
        // show (none would be drawn again for the whole playback).
        let horizon = req.time
            + Tick::from_seconds_f64(LOOKAHEAD * speed.max(1.0)).max(Tick(lead.0 + step.0));
        let keeps_up = self.pace.typical() < rate.frame_duration().as_seconds_f64() * 0.9;
        let mut t = match keeps_up {
            true => next,
            false => rate.snap_floor(req.time + lead).max(next),
        };
        let newest = self
            .ahead
            .iter()
            .filter(|f| req.of_this(f))
            .map(|f| f.time)
            .chain(self.drawing.iter().filter(|k| k.of(req)).map(|k| k.time))
            .chain(
                self.latest
                    .iter()
                    .filter(|f| req.of_this(f))
                    .map(|f| f.time),
            )
            .filter(|&x| x >= req.time && x <= horizon)
            .max();
        if let Some(n) = newest {
            t = t.max(rate.snap_floor(n + step));
        }
        (t <= horizon).then_some(t)
    }
}

/// What a thread draws.
enum Job {
    /// The frame asked for (a still, scrubbing, playback's first frame):
    /// exact, drawn again while scrubbing until it is.
    Asked(Request),
    /// A frame of playback, for when it shows.
    Ahead(Request, Tick),
    /// A frame of a Dynamic RAM Preview being built.
    Build(Arc<Project>, u64, Drawing, Tick, (u32, u32)),
    /// The plug-ins another thread readied, readied for this one's too.
    Ready(Arc<Project>, (Tick, Tick), (u32, u32)),
}

type Shared = Arc<(Mutex<State>, Condvar)>;

fn lock(shared: &Shared) -> MutexGuard<'_, State> {
    shared.0.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) struct PreviewRenderer {
    shared: Shared,
    threads: Vec<JoinHandle<()>>,
}

impl PreviewRenderer {
    pub(crate) fn new(repaint: RepaintFn, accel: Arc<Accel>) -> PreviewRenderer {
        let shared: Shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let threads = (0..WORKERS)
            .map(|me| {
                let (shared, repaint, accel) = (shared.clone(), repaint.clone(), accel.clone());
                std::thread::Builder::new()
                    .name(format!("bv-preview-{me}"))
                    // Plug-in effects run on it.
                    .stack_size(bv_ofx::PLUGIN_STACK)
                    .spawn(move || Worker::new(me, accel).run(&shared, &repaint))
                    .expect("a thread for the preview")
            })
            .collect();
        PreviewRenderer { shared, threads }
    }

    pub(crate) fn request(
        &self,
        project: Arc<Project>,
        generation: u64,
        time: Tick,
        size: (u32, u32),
        ahead: Option<Tick>,
    ) {
        self.request_with(project, generation, time, size, ahead, Drawing::default());
    }

    /// [`request`](Self::request), drawn as `drawing` says (`generation`
    /// must tell its looks apart).
    pub(crate) fn request_with(
        &self,
        project: Arc<Project>,
        generation: u64,
        time: Tick,
        (width, height): (u32, u32),
        ahead: Option<Tick>,
        drawing: Drawing,
    ) {
        let mut st = lock(&self.shared);
        let same_frame = st.asked.as_ref().is_some_and(|a| {
            (a.generation, a.time, a.width, a.height) == (generation, time, width, height)
        });
        if !same_frame {
            st.serial += 1;
        }
        let req = Request {
            project,
            generation,
            drawing,
            time,
            width,
            height,
            ahead,
            serial: st.serial,
        };
        // Frames of an older project, or another size, are of no use any
        // more.
        st.kept.retain(|k| k.generation == generation);
        st.ahead.retain(|f| req.of_this(f));
        if ahead.is_none() {
            st.ahead.clear();
            st.pace.stop();
        }
        if !st.latest.as_ref().is_some_and(|f| req.same_as(f)) {
            // Drawn ahead already, or lately: shown at once. While playing,
            // a frame drawn ahead the position passed stands in for one
            // that never came (frames dropped evenly): the newest of them.
            let exact = st
                .ahead
                .iter()
                .position(|f| req.same_as(f))
                .and_then(|i| st.ahead.remove(i))
                .or_else(|| st.kept.iter().rev().find(|f| req.same_as(f)).cloned());
            let passed = || {
                let newer = |f: &&Arc<PreviewFrame>| {
                    st.latest
                        .as_ref()
                        .is_none_or(|l| !req.of_this(l) || l.time < f.time)
                };
                st.ahead
                    .iter()
                    .filter(|f| f.time <= time)
                    .filter(newer)
                    .max_by_key(|f| f.time)
                    .cloned()
            };
            let ready = exact.or_else(|| ahead.and_then(|_| passed()));
            if let Some(f) = ready {
                let serial = st.serial;
                st.show(f, serial);
            }
        }
        // Frames drawn ahead that the position passed go.
        st.ahead.retain(|f| f.time > time);
        st.asked = Some(req);
        self.shared.1.notify_all();
    }

    /// How many bytes of frames are kept to show again at once.
    pub(crate) fn set_cache_bytes(&self, bytes: usize) {
        let mut st = lock(&self.shared);
        st.keep_bytes = Some(bytes);
        while st.kept_bytes() > bytes && st.kept.pop_front().is_some() {}
    }

    /// Draws every frame in `times` into the frames kept (a Dynamic RAM
    /// Preview), when no frame is asked for, as `drawing` says.
    pub(crate) fn build_with(
        &self,
        project: Arc<Project>,
        generation: u64,
        times: Vec<Tick>,
        (width, height): (u32, u32),
        drawing: Drawing,
    ) {
        let mut st = lock(&self.shared);
        st.kept.retain(|k| k.generation == generation);
        let total = times.len();
        st.build = Some(Build {
            project,
            generation,
            drawing,
            frames: times.into(),
            width,
            height,
            done: 0,
            total,
            drawn_now: 0,
        });
        st.built = Some(BuildProgress::Building(0, total));
        self.shared.1.notify_all();
    }

    pub(crate) fn build_progress(&self) -> Option<BuildProgress> {
        lock(&self.shared).built
    }

    /// Forgets the frames kept (and stops a build).
    pub(crate) fn clear_cache(&self) {
        let mut st = lock(&self.shared);
        st.kept.clear();
        st.ahead.clear();
        st.build = None;
        st.built = None;
    }

    /// Closes the media files this preview reads (they open again when
    /// needed).
    pub(crate) fn release(&self) {
        lock(&self.shared).release += 1;
        self.shared.1.notify_all();
    }

    pub(crate) fn latest(&self) -> Option<Arc<PreviewFrame>> {
        lock(&self.shared).latest.clone()
    }
}

impl Drop for PreviewRenderer {
    fn drop(&mut self) {
        lock(&self.shared).quit = true;
        self.shared.1.notify_all();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// A thread's own: its painter, media readers and prerendered parts.
struct Worker {
    me: usize,
    accel: Arc<Accel>,
    painter: Painter,
    pool: VideoPool,
    prerendered: Prerendered,
    /// The media the readers were opened for, and how video was decoded:
    /// the readers open again when either changes (an edit's generation
    /// does not).
    pool_media: Vec<MediaInfo>,
    decoding: u64,
    released: u64,
    /// The plug-ins readied last ([`State::readied`]).
    readied: u64,
}

impl Worker {
    fn new(me: usize, accel: Arc<Accel>) -> Worker {
        Worker {
            me,
            painter: Painter::new(accel.clone(), crate::plugins::Purpose::Preview),
            decoding: accel.decoding_serial(),
            accel,
            pool: VideoPool::new(),
            prerendered: Prerendered::default(),
            pool_media: Vec::new(),
            released: 0,
            readied: 0,
        }
    }

    fn run(mut self, shared: &Shared, repaint: &RepaintFn) {
        loop {
            let job = {
                let mut st = lock(shared);
                loop {
                    if st.quit {
                        return;
                    }
                    if st.release != self.released {
                        self.released = st.release;
                        drop(st);
                        self.pool.clear();
                        self.pool_media.clear();
                        st = lock(shared);
                        continue;
                    }
                    if let Some(job) = next_job(&mut st, self.me) {
                        break job;
                    }
                    if st.readied != self.readied
                        && let Some((project, window, size)) = st.ready.clone()
                    {
                        self.readied = st.readied;
                        break Job::Ready(project, window, size);
                    }
                    st = shared.1.wait(st).unwrap_or_else(|e| e.into_inner());
                }
            };
            match job {
                Job::Asked(req) => self.asked(shared, repaint, &req),
                Job::Ahead(req, t) => self.ahead(shared, repaint, &req, t),
                Job::Ready(project, window, size) => self.painter.ready(&project, window, size),
                Job::Build(project, generation, drawing, t, size) => {
                    self.ready_for(shared, &project, size);
                    self.pool.set_patience(None);
                    let start = Instant::now();
                    let waited = self.pool.waited();
                    let (rgba, exact) = paint(
                        &mut self.painter,
                        &project,
                        &drawing,
                        t,
                        size,
                        &mut self.pool,
                    );
                    let waited = self.pool.waited() - waited;
                    self.trace_took("build", t, exact, start.elapsed(), waited);
                    let frame = Arc::new(PreviewFrame {
                        time: t,
                        generation,
                        width: size.0,
                        height: size.1,
                        rgba,
                        exact,
                    });
                    let mut st = lock(shared);
                    built(&mut st, frame);
                    drop(st);
                    repaint();
                }
            }
        }
    }

    /// The readers for `project`'s media at `size`: opened again when its
    /// media or the decoding changed (the frames kept go too); below full
    /// size, media with a proxy are read from it (the files themselves at
    /// full size, as Best (Full) shows them).
    fn ready_for(&mut self, shared: &Shared, project: &Project, (w, h): (u32, u32)) {
        if project.media != self.pool_media || self.accel.decoding_serial() != self.decoding {
            self.pool.clear();
            self.pool_media = project.media.clone();
            self.decoding = self.accel.decoding_serial();
            lock(shared).kept.clear();
        }
        let (ow, oh) = project.settings.output_size();
        self.pool.set_proxies(w < ow || h < oh);
    }

    /// `req`'s frame at `time` (a prerendered part plays from its file),
    /// and how long drawing it took and waited for its pictures.
    fn draw(&mut self, req: &Request, time: Tick) -> (Arc<PreviewFrame>, Duration, Duration) {
        let source = self.prerendered.source(&req.project, req.generation, time);
        let size = (req.width, req.height);
        let start = Instant::now();
        let waited = self.pool.waited();
        let (rgba, exact) = paint(
            &mut self.painter,
            &source,
            &req.drawing,
            time,
            size,
            &mut self.pool,
        );
        let frame = Arc::new(PreviewFrame {
            time,
            generation: req.generation,
            width: req.width,
            height: req.height,
            rgba,
            exact,
        });
        (frame, start.elapsed(), self.pool.waited() - waited)
    }

    /// The frame asked for. Scrubbing, pictures on the way stand in after
    /// a while, and the frame is drawn again until it is exact — unless
    /// another one is asked for first.
    fn asked(&mut self, shared: &Shared, repaint: &RepaintFn, req: &Request) {
        self.ready_for(shared, &req.project, (req.width, req.height));
        let scrubbing = req.ahead.is_none();
        // A still frame keeps the input of the effect an FX window shows,
        // for the window to read; playback's first frame does not.
        let watch = crate::plugins::watching().filter(|_| scrubbing);
        crate::plugins::keep_watched_input(watch.is_some());
        loop {
            self.pool
                .set_patience(scrubbing.then(|| Instant::now() + PATIENCE));
            let (frame, took, waited) = self.draw(req, req.time);
            self.trace_took("asked", req.time, frame.exact, took, waited);
            let exact = frame.exact;
            let newer = {
                let mut st = lock(shared);
                // Its readers may have sought: what it took tells nothing of
                // playback's frames.
                st.keep(&frame);
                if let Some(w) = watch.filter(|_| exact) {
                    st.inputs_for = Some((req.generation, req.time, req.width, req.height, w));
                }
                st.show(frame, req.serial);
                st.serial != req.serial
            };
            shared.1.notify_all();
            repaint();
            if exact || newer {
                break;
            }
        }
        crate::plugins::keep_watched_input(false);
        self.pool.set_patience(None);
        // Playback may start here: the plug-ins of what comes next get
        // ready meanwhile.
        self.ready(shared, &req.project, req.time, (req.width, req.height));
    }

    /// Readies the plug-ins of the events starting within `READY_AHEAD`
    /// after `t` (see [`Painter::ready`]), and has the other threads ready
    /// theirs.
    fn ready(&mut self, shared: &Shared, project: &Arc<Project>, t: Tick, size: (u32, u32)) {
        let window = (t, t + Tick::from_seconds_f64(READY_AHEAD));
        self.painter.ready(project, window, size);
        let mut st = lock(shared);
        st.ready = Some((project.clone(), window, size));
        st.readied += 1;
        self.readied = st.readied;
        drop(st);
        shared.1.notify_all();
    }

    /// Playback's frame at `t`, kept until its time comes; then the readers
    /// of the events starting soon get ready.
    fn ahead(&mut self, shared: &Shared, repaint: &RepaintFn, req: &Request, t: Tick) {
        self.ready_for(shared, &req.project, (req.width, req.height));
        self.pool.set_patience(None);
        let (frame, took, waited) = self.draw(req, t);
        self.trace_took("ahead", t, frame.exact, took, waited);
        {
            let mut st = lock(shared);
            let key = req.key(t);
            if let Some(i) = st.drawing.iter().position(|k| *k == key) {
                st.drawing.swap_remove(i);
            }
            st.pace.note(took, waited, self.me > 0);
            let interval = req
                .project
                .settings
                .frame_rate
                .frame_duration()
                .as_seconds_f64();
            if let Some(two) = st.pace.plan(interval, self.painter.on_the_card())
                && tracing()
            {
                eprintln!(
                    "preview: {} thread{} drawing ahead",
                    if two { "two" } else { "one" },
                    if two { "s" } else { "" }
                );
            }
            st.keep(&frame);
            // Still wanted: of the project and size asked for, after what
            // shows.
            let wanted = st
                .asked
                .as_ref()
                .is_some_and(|a| a.ahead.is_some() && a.of_this(&frame))
                && st
                    .latest
                    .as_ref()
                    .is_none_or(|l| !req.of_this(l) || l.time < frame.time);
            if wanted {
                let at = st.ahead.partition_point(|f| f.time < frame.time);
                st.ahead.insert(at, frame);
            }
        }
        shared.1.notify_all();
        repaint();
        // The events coming within a few seconds get their readers now (a
        // long recording takes a while to open and to seek in), and their
        // plug-ins, the other threads' too.
        let soon = (t, t + Tick::from_seconds_f64(2.5));
        self.painter
            .warm(&req.project, soon, (req.width, req.height), &mut self.pool);
        self.ready(shared, &req.project, t, (req.width, req.height));
        // Plug-in effects no longer drawn let go of their instances.
        self.painter.sweep_plugins();
    }

    fn trace_took(&self, why: &str, t: Tick, exact: bool, took: Duration, waited: Duration) {
        if tracing() {
            eprintln!(
                "preview {}: {why} {:.3} s, {:.1} ms ({:.1} ms waiting for pictures){}",
                self.me,
                t.as_seconds_f64(),
                took.as_secs_f64() * 1e3,
                waited.as_secs_f64() * 1e3,
                if exact { "" } else { ", not exact" }
            );
        }
    }
}

/// The next job for thread `me`: the frame asked for when nothing of it
/// shows; while playing, the next frame ahead (the second thread only when
/// two draw ahead); else a frame of a Dynamic RAM Preview being built. The
/// frames asked for and the builds are the first thread's: its readers
/// are where they go on from (another's would seek, decoding twice).
fn next_job(st: &mut State, me: usize) -> Option<Job> {
    let asked = st.asked.clone();
    if let Some(req) = &asked {
        let playing = req.ahead.is_some();
        let have = st.latest.as_ref().is_some_and(|f| req.same_as(f));
        let current = st.latest.as_ref().is_some_and(|f| req.of_this(f));
        // A still frame that did not keep the input of the effect an FX
        // window shows (drawn while playing, kept, or before the window
        // opened) is drawn again for it, once.
        let input_wanted = !playing
            && crate::plugins::watching().is_some_and(|w| {
                st.inputs_for != Some((req.generation, req.time, req.width, req.height, w))
            });
        // Playback draws ahead once a frame of it shows.
        if me == 0 && (!playing || !current) && ((!have && st.taken < req.serial) || input_wanted) {
            st.taken = req.serial;
            return Some(Job::Asked(req.clone()));
        }
        if playing && current && me < st.pace.lanes() {
            let busy = st.drawing.iter().filter(|k| k.of(req)).count();
            if busy < st.pace.lanes()
                && let Some(t) = st.next_ahead(req)
            {
                st.drawing.push(req.key(t));
                return Some(Job::Ahead(req.clone(), t));
            }
        }
        if playing {
            return None;
        }
    }
    // Nothing asked for: the next frame of a Dynamic RAM Preview being
    // built, and when its last one is done, the build is.
    if me > 0 {
        return None;
    }
    let b = st.build.as_mut()?;
    if let Some(t) = b.frames.pop_front() {
        b.drawn_now += 1;
        let size = (b.width, b.height);
        return Some(Job::Build(
            b.project.clone(),
            b.generation,
            b.drawing.clone(),
            t,
            size,
        ));
    }
    if b.drawn_now == 0 {
        let (done, total) = (b.done, b.total);
        st.build = None;
        st.built = Some(BuildProgress::Done(done, total));
    }
    None
}

/// A frame of a Dynamic RAM Preview drawn: kept, unless the memory for
/// them is full (it would push the first ones out): the build ends there.
fn built(st: &mut State, frame: Arc<PreviewFrame>) {
    let Some((done, total)) = st
        .build
        .as_mut()
        .filter(|b| b.generation == frame.generation)
        .map(|b| {
            b.drawn_now = b.drawn_now.saturating_sub(1);
            (b.done, b.total)
        })
    else {
        return;
    };
    if st.kept_bytes() + frame.rgba.len() > st.budget() {
        st.build = None;
        st.built = Some(BuildProgress::Done(done, total));
        return;
    }
    if let Some(b) = &mut st.build {
        b.done += 1;
    }
    st.keep(&frame);
    st.built = Some(BuildProgress::Building(done + 1, total));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::{Generator, MediaInfo, ProjectSettings};
    use std::time::{Duration, Instant};

    fn wait_for(r: &PreviewRenderer, t: Tick) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            if r.latest().is_some_and(|f| f.time == t) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        false
    }

    #[test]
    fn a_dynamic_ram_preview_shows_its_frames_at_once() {
        let mut p = Project::new(ProjectSettings::default());
        let info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        let m = p.add_media(info);
        let v = p.tracks[0].id;
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let p = Arc::new(p);
        let rate = p.settings.frame_rate;
        let r = PreviewRenderer::new(Arc::new(|| {}), Accel::new(Arc::new(|| {})));
        let times: Vec<Tick> = (0..20).map(|k| rate.frame_start(k)).collect();
        r.build_with(p.clone(), 1, times.clone(), (64, 36), Drawing::default());
        let start = Instant::now();
        while !matches!(r.build_progress(), Some(BuildProgress::Done(..))) {
            assert!(start.elapsed() < Duration::from_secs(20), "built");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(r.build_progress(), Some(BuildProgress::Done(20, 20)));
        for &t in &times {
            r.request(p.clone(), 1, t, (64, 36), None);
            assert_eq!(r.latest().map(|f| f.time), Some(t), "shown at once");
        }
        // Memory for fewer frames: the build stops when it is full.
        r.clear_cache();
        r.set_cache_bytes(64 * 36 * 4 * 5);
        r.build_with(p.clone(), 1, times, (64, 36), Drawing::default());
        let start = Instant::now();
        while !matches!(r.build_progress(), Some(BuildProgress::Done(..))) {
            assert!(start.elapsed() < Duration::from_secs(20), "built");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(r.build_progress(), Some(BuildProgress::Done(5, 20)));
    }

    #[test]
    fn split_screen_and_channels_change_the_frame() {
        let mut rgba = vec![10, 20, 30, 255, 40, 50, 60, 128];
        isolate(&mut rgba, Channel::Green);
        assert_eq!(rgba, [0, 20, 0, 255, 0, 50, 0, 255]);
        let mut rgba = vec![10, 20, 30, 255, 40, 50, 60, 128];
        isolate(&mut rgba, Channel::AlphaGray);
        assert_eq!(rgba, [255, 255, 255, 255, 128, 128, 128, 255]);
        // The right half of a 4 × 2 frame from the other one.
        let mut a = vec![0u8; 4 * 2 * 4];
        let b = vec![9u8; 4 * 2 * 4];
        copy_area(&mut a, &b, (4, 2), [0.5, 0.0, 1.0, 1.0]);
        for y in 0..2 {
            for x in 0..4 {
                let v = a[(y * 4 + x) * 4];
                assert_eq!(v, if x >= 2 { 9 } else { 0 }, "{x},{y}");
            }
        }
    }

    #[test]
    fn the_next_frame_is_drawn_ahead() {
        let mut p = Project::new(ProjectSettings::default());
        let info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        let m = p.add_media(info);
        let v = p.tracks[0].id;
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let p = Arc::new(p);
        let rate = p.settings.frame_rate;
        let r = PreviewRenderer::new(Arc::new(|| {}), Accel::new(Arc::new(|| {})));
        let (t0, t1) = (rate.frame_start(10), rate.frame_start(11));
        r.request(p.clone(), 1, t0, (64, 36), Some(t1));
        assert!(wait_for(&r, t0));
        // Let the next one be drawn, then ask for it: it shows at once.
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) && lock(&r.shared).ahead.is_empty() {
            std::thread::sleep(Duration::from_millis(2));
        }
        let drawn = lock(&r.shared).ahead.front().map(|f| f.time);
        assert!(drawn.is_some_and(|t| t > t0), "a frame after it: {drawn:?}");
        let t2 = drawn.unwrap();
        r.request(p.clone(), 1, t2, (64, 36), Some(t2 + rate.frame_duration()));
        assert_eq!(
            r.latest().map(|f| f.time),
            Some(t2),
            "shown without waiting"
        );
        // Another edit (generation) is drawn again.
        r.request(p, 2, t2, (64, 36), None);
        let start = Instant::now();
        while r.latest().is_some_and(|f| f.generation != 2) {
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Playback slower than the project's rate: frames are drawn for when
    /// they show (further ahead than the next), and a frame drawn ahead
    /// the position passed stands in for those that never came.
    #[test]
    fn slow_playback_draws_frames_for_when_they_show() {
        let mut st = State::default();
        let p = Arc::new(Project::new(ProjectSettings::default()));
        let rate = p.settings.frame_rate;
        let req = |serial, n: i64| Request {
            project: p.clone(),
            generation: 1,
            drawing: Drawing::default(),
            time: rate.frame_start(n),
            width: 64,
            height: 36,
            ahead: Some(rate.frame_start(n + 1)),
            serial,
        };
        let frame = |n: i64| {
            Arc::new(PreviewFrame {
                time: rate.frame_start(n),
                generation: 1,
                width: 64,
                height: 36,
                rgba: Vec::new(),
                exact: true,
            })
        };
        // Fast frames: the next one.
        let r = req(1, 100);
        st.latest = Some(frame(100));
        assert_eq!(st.next_ahead(&r), Some(rate.frame_start(101)));
        // A frame takes a tenth of a second: the one the position reaches
        // by then (and a little more).
        st.pace.cost = 0.1;
        st.pace.recent.push_back(0.1);
        let lead = (0.1 * 1.25 + 0.008) * rate.as_f64();
        let due = 100 + lead.floor() as i64;
        assert_eq!(st.next_ahead(&r), Some(rate.frame_start(due)));
        // Being drawn already: the next after it.
        st.drawing.push(r.key(rate.frame_start(due)));
        assert_eq!(st.next_ahead(&r), Some(rate.frame_start(due + 1)));
        // No further than a second ahead.
        let last = rate.snap_floor(rate.frame_start(100) + Tick::from_seconds(1));
        st.drawing.push(r.key(last));
        assert_eq!(st.next_ahead(&r), None);
        // Frames slower than that: still the one showing when it is done,
        // and none after the next.
        st.drawing.clear();
        st.pace.cost = 0.9;
        st.pace.recent = [0.9].into();
        let lead = (0.9 * 1.25 + 0.008) * rate.as_f64();
        let due = 100 + lead.floor() as i64;
        assert_eq!(st.next_ahead(&r), Some(rate.frame_start(due)));
        st.drawing.push(r.key(rate.frame_start(due)));
        st.drawing.push(r.key(rate.frame_start(due + 1)));
        assert_eq!(st.next_ahead(&r), None);
    }

    /// A second thread is tried when one cannot keep up and the frames do
    /// not wait on the decoders, and let go when it shows no more frames.
    #[test]
    fn a_second_thread_is_tried_and_kept_only_when_it_helps() {
        let mut pace = Pace::default();
        let interval = 1.0 / 60.0;
        for _ in 0..4 {
            pace.note(Duration::from_millis(40), Duration::from_millis(2), false);
        }
        assert!(pace.plan(interval, false).is_none(), "not on the processor");
        assert_eq!(pace.plan(interval, true), Some(true), "behind: two threads");
        // The second thread's first frame starts the count.
        pace.note(Duration::from_millis(400), Duration::from_millis(390), true);
        assert_eq!(pace.trial.and_then(|t| t.after), Some(0));
        // As many frames a second as one gave: let go.
        if let Some(t) = &mut pace.trial {
            (t.one, t.after) = (1e9, Some(TRIAL));
        }
        assert_eq!(
            pace.plan(interval, true),
            Some(false),
            "no more frames: one"
        );
        assert!(pace.plan(interval, true).is_none(), "not again at once");
        // Waiting on the decoders: one.
        let mut pace = Pace::default();
        for _ in 0..4 {
            pace.note(Duration::from_millis(40), Duration::from_millis(35), false);
        }
        assert!(pace.plan(interval, true).is_none());
        assert!(!pace.two);
    }
}
