//! Screen recording (the Screen Capture window; VEGAS's Vegas Capture):
//! displays, application windows and cameras recorded as video, inputs
//! (microphones) and what the computer plays recorded as sound, all at
//! once, each into a file of its own in the recording folder — then each
//! goes into the project on a track of its own. While the window is open
//! every source shows live: a small picture, the sound's levels.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bv_core::{FrameRate, Tick, TimeRange};
use bv_timeline::{Project, ProjectSettings};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::{Deserialize, Serialize};

use crate::render::{AudioParams, FileFormat, RenderOptions, RenderSettings, VideoParams};

pub use bv_mf::camera::CameraMode;

/// Where a source's picture or sound comes from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceKind {
    /// A display, by the system's name of it (`bv_platform::displays`).
    Display { name: String },
    /// An application's window.
    Window { id: isize },
    /// A camera, by the system's link to it.
    Camera { link: String },
    /// An input (a microphone), by its name (`input_devices`).
    Input { device: String },
    /// What the computer plays (its default output, recorded back).
    SystemSound,
}

impl SourceKind {
    /// A picture's (else a sound's).
    pub fn is_video(&self) -> bool {
        matches!(
            self,
            SourceKind::Display { .. } | SourceKind::Window { .. } | SourceKind::Camera { .. }
        )
    }
}

/// How video is compressed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureVideoCodec {
    #[default]
    Avc,
    Hevc,
}

impl CaptureVideoCodec {
    pub const ALL: [CaptureVideoCodec; 2] = [CaptureVideoCodec::Avc, CaptureVideoCodec::Hevc];

    pub fn name(self) -> &'static str {
        match self {
            CaptureVideoCodec::Avc => "AVC (H.264)",
            CaptureVideoCodec::Hevc => "HEVC (H.265)",
        }
    }

    fn format(self) -> FileFormat {
        match self {
            CaptureVideoCodec::Avc => FileFormat::AvcMp4,
            CaptureVideoCodec::Hevc => FileFormat::HevcMp4,
        }
    }
}

/// How sound is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureAudioCodec {
    #[default]
    Mp3,
    Aac,
    /// Uncompressed (Wave).
    Pcm,
}

impl CaptureAudioCodec {
    pub const ALL: [CaptureAudioCodec; 3] = [
        CaptureAudioCodec::Mp3,
        CaptureAudioCodec::Aac,
        CaptureAudioCodec::Pcm,
    ];

    pub fn name(self) -> &'static str {
        match self {
            CaptureAudioCodec::Mp3 => "MP3",
            CaptureAudioCodec::Aac => "AAC",
            CaptureAudioCodec::Pcm => "PCM (Wave)",
        }
    }

    fn format(self) -> FileFormat {
        match self {
            CaptureAudioCodec::Mp3 => FileFormat::Mp3,
            CaptureAudioCodec::Aac => FileFormat::Aac,
            CaptureAudioCodec::Pcm => FileFormat::Wav,
        }
    }
}

/// How many bits a second a source gets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureQuality {
    Low,
    #[default]
    Medium,
    High,
}

impl CaptureQuality {
    pub const ALL: [CaptureQuality; 3] = [
        CaptureQuality::Low,
        CaptureQuality::Medium,
        CaptureQuality::High,
    ];

    /// Video's bits a second for `width` × `height` at `rate`: 5, 10 or 20
    /// Mbit/s for Full HD at 30 frames a second, in proportion to the
    /// pixels a second.
    pub fn video_bitrate(self, width: u32, height: u32, rate: FrameRate) -> u32 {
        let full_hd = match self {
            CaptureQuality::Low => 5_000_000.0,
            CaptureQuality::Medium => 10_000_000.0,
            CaptureQuality::High => 20_000_000.0,
        };
        let share = (width as f64 * height as f64 * rate.as_f64()) / (1920.0 * 1080.0 * 30.0);
        (full_hd * share).clamp(500_000.0, 200_000_000.0) as u32
    }

    /// Compressed sound's bits a second for `channels`: 128, 192 or 320
    /// kbit/s in stereo (half in mono).
    pub fn audio_bitrate(self, channels: u16) -> u32 {
        let stereo = match self {
            CaptureQuality::Low => 128_000,
            CaptureQuality::Medium => 192_000,
            CaptureQuality::High => 320_000,
        };
        if channels <= 1 { stereo / 2 } else { stereo }
    }
}

/// A source to record and how.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaptureSource {
    pub kind: SourceKind,
    /// Its name: the card's, the file's and the track's.
    pub name: String,
    /// Left out of the recording.
    #[serde(default)]
    pub muted: bool,
    /// A picture's size and rate, and how it is compressed.
    pub width: u32,
    pub height: u32,
    pub rate: FrameRate,
    pub video_codec: CaptureVideoCodec,
    pub video_quality: CaptureQuality,
    /// A sound's channels and rate, and how it is kept.
    pub channels: u16,
    pub sample_rate: u32,
    pub audio_codec: CaptureAudioCodec,
    pub audio_quality: CaptureQuality,
}

impl CaptureSource {
    fn new(kind: SourceKind, name: String, d: &CaptureDefaults) -> CaptureSource {
        CaptureSource {
            kind,
            name,
            muted: false,
            width: 1920,
            height: 1080,
            rate: FrameRate::FPS_30,
            video_codec: d.video_codec,
            video_quality: d.video_quality,
            channels: 2,
            sample_rate: 48_000,
            audio_codec: d.audio_codec,
            audio_quality: d.audio_quality,
        }
    }

    /// Its card's line: "AVC (H.264); 10 Mbit/s; 30 fps; 1920×1080", or
    /// "MP3; 192 kbit/s; Stereo; 48000 Hz" (`stereo`/`mono` in the
    /// interface's words).
    pub fn summary(&self, stereo: &str, mono: &str) -> String {
        if self.kind.is_video() {
            let rate = self
                .video_quality
                .video_bitrate(self.width, self.height, self.rate);
            format!(
                "{}; {:.2} Mbit/s; {} fps; {}×{}",
                self.video_codec.name(),
                rate as f64 / 1e6,
                fps_text(self.rate),
                self.width,
                self.height
            )
        } else {
            let layout = if self.channels <= 1 { mono } else { stereo };
            match self.audio_codec {
                CaptureAudioCodec::Pcm => format!(
                    "{}; {}; {} Hz",
                    self.audio_codec.name(),
                    layout,
                    self.sample_rate
                ),
                c => format!(
                    "{}; {} kbit/s; {}; {} Hz",
                    c.name(),
                    self.audio_quality.audio_bitrate(self.channels) / 1000,
                    layout,
                    self.sample_rate
                ),
            }
        }
    }
}

/// A frame rate as a number of frames a second ("30", "29.97").
pub fn fps_text(rate: FrameRate) -> String {
    let v = rate.as_f64();
    if (v - v.round()).abs() < 1e-6 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}")
    }
}

/// The settings new sources start with (the Screen Capture's Settings).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureDefaults {
    pub video_codec: CaptureVideoCodec,
    pub video_quality: CaptureQuality,
    pub audio_codec: CaptureAudioCodec,
    pub audio_quality: CaptureQuality,
}

/// The displays as sources ("Screen 1"… in the system's order, the name
/// numbered by `screen`).
pub fn display_sources(
    d: &CaptureDefaults,
    screen: &dyn Fn(usize) -> String,
) -> Vec<CaptureSource> {
    bv_platform::displays()
        .into_iter()
        .enumerate()
        .map(|(i, disp)| {
            let mut s = CaptureSource::new(
                SourceKind::Display {
                    name: disp.name.clone(),
                },
                screen(i + 1),
                d,
            );
            s.width = disp.width & !1;
            s.height = disp.height & !1;
            s
        })
        .collect()
}

/// The applications' windows as sources.
pub fn window_sources(d: &CaptureDefaults) -> Vec<CaptureSource> {
    bv_platform::app_windows()
        .into_iter()
        .map(|w| {
            let mut s = CaptureSource::new(SourceKind::Window { id: w.id }, w.title, d);
            s.width = (w.width & !1).max(2);
            s.height = (w.height & !1).max(2);
            s
        })
        .collect()
}

/// The cameras as sources (each in its largest format).
pub fn camera_sources(d: &CaptureDefaults) -> Vec<CaptureSource> {
    bv_mf::camera::cameras()
        .into_iter()
        .map(|c| {
            let mut s = CaptureSource::new(
                SourceKind::Camera {
                    link: c.link.clone(),
                },
                c.name,
                d,
            );
            if let Some(m) = bv_mf::camera::camera_modes(&c.link).first() {
                s.width = m.width & !1;
                s.height = m.height & !1;
                s.rate = m.rate;
            }
            s
        })
        .collect()
}

/// A camera's picture formats.
pub fn camera_modes(link: &str) -> Vec<CameraMode> {
    bv_mf::camera::camera_modes(link)
}

/// The inputs as sources, and what the computer plays (named `system`).
pub fn audio_sources(d: &CaptureDefaults, system: &str) -> Vec<CaptureSource> {
    let mut out: Vec<CaptureSource> = crate::record::input_devices()
        .into_iter()
        .map(|i| {
            let mut s = CaptureSource::new(
                SourceKind::Input {
                    device: i.name.clone(),
                },
                i.name,
                d,
            );
            s.channels = i.channels.clamp(1, 2);
            s
        })
        .collect();
    out.push(CaptureSource::new(
        SourceKind::SystemSound,
        system.to_string(),
        d,
    ));
    out
}

/// What a source shows now.
#[derive(Clone, Debug, Default)]
pub struct SourceLive {
    /// A small picture of it (BGRA, the top row first), for a video source.
    pub picture: Option<(u32, u32, Arc<Vec<u8>>)>,
    /// Counts the pictures given (a new one, a new number).
    pub serial: u64,
    /// Its sound's peak level per channel (0–1) lately.
    pub levels: Vec<f32>,
    /// Why it shows or records nothing.
    pub error: Option<String>,
}

/// A file recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Captured {
    /// The source's name.
    pub name: String,
    pub path: PathBuf,
    pub video: bool,
}

/// One source's thread: showing it, and recording it when told to.
struct Worker {
    name: String,
    video: bool,
    live: Arc<Mutex<SourceLive>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Option<Result<PathBuf, String>>>>,
}

impl Worker {
    fn start(source: &CaptureSource, record: Option<(PathBuf, Instant)>) -> Worker {
        let live = Arc::new(Mutex::new(SourceLive::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (source, live, stop) = (source.clone(), live.clone(), stop.clone());
            std::thread::Builder::new()
                .name("bv-capture".into())
                .spawn(move || {
                    if source.kind.is_video() {
                        video(&source, record, &live, &stop)
                    } else {
                        audio(&source, record, &live, &stop)
                    }
                })
                .ok()
        };
        Worker {
            name: source.name.clone(),
            video: source.kind.is_video(),
            live,
            stop,
            thread,
        }
    }

    /// Stops it; what it recorded, if it recorded.
    fn finish(mut self) -> Option<Result<Captured, String>> {
        self.stop.store(true, Ordering::Relaxed);
        let done = self.thread.take()?.join().ok()??;
        Some(done.map(|path| Captured {
            name: self.name.clone(),
            path,
            video: self.video,
        }))
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// The Screen Capture's sources, shown live and recorded together.
#[derive(Default)]
pub struct Capture {
    sources: Vec<CaptureSource>,
    workers: Vec<Worker>,
    /// When the recording started.
    recording: Option<Instant>,
}

impl std::fmt::Debug for Capture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Capture")
            .field("sources", &self.sources.len())
            .field("recording", &self.recording.is_some())
            .finish()
    }
}

impl Capture {
    pub fn new() -> Capture {
        Capture::default()
    }

    pub fn sources(&self) -> &[CaptureSource] {
        &self.sources
    }

    /// The sources to show and record (not while recording): each shows
    /// live from now on.
    pub fn set_sources(&mut self, sources: Vec<CaptureSource>) {
        if self.recording.is_some() {
            return;
        }
        let keep = self.sources.len() == sources.len()
            && self
                .sources
                .iter()
                .zip(&sources)
                .all(|(a, b)| a.kind == b.kind && same_stream(a, b));
        self.sources = sources;
        if !keep || self.workers.len() != self.sources.len() {
            self.workers.clear();
            self.workers = self
                .sources
                .iter()
                .map(|s| Worker::start(s, None))
                .collect();
        }
    }

    /// What source `i` shows now.
    pub fn live(&self, i: usize) -> SourceLive {
        self.workers
            .get(i)
            .and_then(|w| w.live.lock().ok().map(|l| l.clone()))
            .unwrap_or_default()
    }

    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    /// How long it has been recording (`None`: it is not).
    pub fn recording_time(&self) -> Option<Duration> {
        self.recording.map(|t| t.elapsed())
    }

    /// Starts recording the sources not muted into `folder`.
    pub fn start(&mut self, folder: &Path) -> Result<(), String> {
        if self.recording.is_some() {
            return Ok(());
        }
        if !self.sources.iter().any(|s| !s.muted) {
            return Err("no source to record".into());
        }
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        self.workers.clear();
        let at = Instant::now();
        let mut taken: Vec<PathBuf> = Vec::new();
        self.workers = self
            .sources
            .iter()
            .map(|s| {
                let record = (!s.muted).then(|| {
                    let path = free_path(folder, s, &taken);
                    taken.push(path.clone());
                    (path, at)
                });
                Worker::start(s, record)
            })
            .collect();
        self.recording = Some(at);
        Ok(())
    }

    /// Stops recording: the files recorded, and why the others were not.
    pub fn stop(&mut self) -> (Vec<Captured>, Vec<String>) {
        if self.recording.take().is_none() {
            return (Vec::new(), Vec::new());
        }
        let mut files = Vec::new();
        let mut errors = Vec::new();
        for w in std::mem::take(&mut self.workers) {
            let name = w.name.clone();
            match w.finish() {
                Some(Ok(c)) => files.push(c),
                Some(Err(e)) => errors.push(format!("{name}: {e}")),
                None => {}
            }
        }
        // Live again.
        self.workers = self
            .sources
            .iter()
            .map(|s| Worker::start(s, None))
            .collect();
        (files, errors)
    }

    /// Stops showing and recording (the window closed); a recording's
    /// files are finished.
    pub fn close(&mut self) -> Vec<Captured> {
        let (files, _) = self.stop();
        self.workers.clear();
        files
    }
}

/// Whether two sources' streams are made the same way (their workers can
/// go on).
fn same_stream(a: &CaptureSource, b: &CaptureSource) -> bool {
    (a.width, a.height, a.rate, a.channels, a.sample_rate)
        == (b.width, b.height, b.rate, b.channels, b.sample_rate)
}

/// A file name for `s` in `folder` not taken yet: its name (what a file
/// name cannot hold left out), numbered when needed.
fn free_path(folder: &Path, s: &CaptureSource, taken: &[PathBuf]) -> PathBuf {
    let ext = if s.kind.is_video() {
        s.video_codec.format().extension()
    } else {
        s.audio_codec.format().extension()
    };
    let stem: String = s
        .name
        .chars()
        .map(|c| {
            if "\\/:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let stem = stem.trim().trim_end_matches('.');
    let stem = if stem.is_empty() { "Capture" } else { stem };
    (1..)
        .map(|n| {
            let name = if n == 1 {
                format!("{stem}.{ext}")
            } else {
                format!("{stem} ({n}).{ext}")
            };
            folder.join(name)
        })
        .find(|p| !p.exists() && !taken.contains(p))
        .expect("a free name")
}

/// A file being written: the render's writer for its format.
fn open_output(
    path: &Path,
    format: FileFormat,
    video: Option<VideoParams>,
    audio: Option<AudioParams>,
) -> Result<Box<dyn crate::render::Output>, String> {
    let mut settings = ProjectSettings::default();
    if let Some(v) = video {
        settings.width = v.width;
        settings.height = v.height;
        settings.frame_rate = v.frame_rate;
    }
    let project = Project::new(settings);
    crate::render::open_output(
        &project,
        &RenderSettings {
            format,
            path: path.to_path_buf(),
            // The file's header gets room for ten minutes (longer ones
            // put it at the end).
            range: TimeRange::new(Tick::ZERO, Tick::from_seconds(600)),
            video,
            audio,
            options: RenderOptions::default(),
            project_file: None,
        },
    )
}

/// The width of the live pictures.
const LIVE_WIDTH: u32 = 320;

/// A small copy of a BGRA picture for the live view.
fn small(w: u32, h: u32, bgra: &[u8]) -> (u32, u32, Arc<Vec<u8>>) {
    let sw = w.clamp(1, LIVE_WIDTH);
    let sh = ((h as u64 * sw as u64 / w.max(1) as u64) as u32).max(1);
    let mut out = Vec::with_capacity((sw * sh * 4) as usize);
    for y in 0..sh {
        let sy = (y as u64 * h as u64 / sh as u64) as usize;
        for x in 0..sw {
            let sx = (x as u64 * w as u64 / sw as u64) as usize;
            let i = (sy * w as usize + sx) * 4;
            out.extend_from_slice(&bgra[i..i + 4]);
        }
    }
    (sw, sh, Arc::new(out))
}

/// A BGRA picture fitted into `w` × `h` (letterboxed in black) as NV12,
/// BT.709 at video levels.
fn to_nv12(pw: u32, ph: u32, bgra: &[u8], w: u32, h: u32) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let (pw, ph) = (pw as usize, ph as usize);
    // The picture's place: as large as fits, centered.
    let (fw, fh) = if pw * h > ph * w {
        (w, (ph * w / pw).max(1))
    } else {
        ((pw * h / ph).max(1), h)
    };
    let (ox, oy) = ((w - fw.min(w)) / 2, (h - fh.min(h)) / 2);
    let rgb = |x: usize, y: usize| -> [f32; 3] {
        if x < ox || y < oy || x >= ox + fw || y >= oy + fh {
            return [0.0; 3];
        }
        let sx = ((x - ox) * pw / fw).min(pw - 1);
        let sy = ((y - oy) * ph / fh).min(ph - 1);
        let i = (sy * pw + sx) * 4;
        [bgra[i + 2] as f32, bgra[i + 1] as f32, bgra[i] as f32]
    };
    let mut out = vec![0u8; w * h + w * h / 2];
    let (luma, chroma) = out.split_at_mut(w * h);
    for y in 0..h {
        for x in 0..w {
            let [r, g, b] = rgb(x, y);
            let v = 16.0 + (0.2126 * r + 0.7152 * g + 0.0722 * b) * 219.0 / 255.0;
            luma[y * w + x] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
    for y in 0..h / 2 {
        for x in 0..w / 2 {
            let mut sum = [0.0f32; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = rgb(2 * x + dx, 2 * y + dy);
                for c in 0..3 {
                    sum[c] += p[c] / 4.0;
                }
            }
            let [r, g, b] = sum;
            let cb = 128.0 + (-0.1146 * r - 0.3854 * g + 0.5 * b) * 224.0 / 255.0;
            let cr = 128.0 + (0.5 * r - 0.4542 * g - 0.0458 * b) * 224.0 / 255.0;
            let at = y * w + 2 * x;
            chroma[at] = cb.round().clamp(0.0, 255.0) as u8;
            chroma[at + 1] = cr.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// Where a video source's pictures come from.
enum Pictures {
    Area { x: i32, y: i32, w: u32, h: u32 },
    Window(isize),
    Camera(bv_mf::camera::CameraReader),
}

impl Pictures {
    fn open(s: &CaptureSource) -> Result<Pictures, String> {
        match &s.kind {
            SourceKind::Display { name } => bv_platform::displays()
                .into_iter()
                .find(|d| d.name == *name)
                .map(|d| Pictures::Area {
                    x: d.x,
                    y: d.y,
                    w: d.width,
                    h: d.height,
                })
                .ok_or_else(|| "the display is not connected".into()),
            SourceKind::Window { id } => Ok(Pictures::Window(*id)),
            SourceKind::Camera { link } => {
                let mode = CameraMode {
                    width: s.width,
                    height: s.height,
                    rate: s.rate,
                };
                bv_mf::camera::CameraReader::open(link, Some(mode))
                    .map(Pictures::Camera)
                    .map_err(|e| e.to_string())
            }
            _ => Err("not a picture".into()),
        }
    }

    /// The picture now (a camera's next one).
    fn take(&mut self) -> Result<(u32, u32, Vec<u8>), String> {
        match self {
            Pictures::Area { x, y, w, h } => bv_platform::grab_area(*x, *y, *w, *h)
                .map(|g| (g.width, g.height, g.bgra))
                .ok_or_else(|| "the screen cannot be read".into()),
            Pictures::Window(id) => bv_platform::grab_window(*id)
                .map(|g| (g.width, g.height, g.bgra))
                .ok_or_else(|| "the window is closed or minimized".into()),
            Pictures::Camera(c) => c
                .next_picture()
                .map(|p| (p.width, p.height, p.bgra))
                .map_err(|e| e.to_string()),
        }
    }

    fn paced(&self) -> bool {
        !matches!(self, Pictures::Camera(_))
    }
}

fn set_error(live: &Mutex<SourceLive>, e: Option<String>) {
    if let Ok(mut l) = live.lock() {
        l.error = e;
    }
}

/// A video source's thread: its live picture a few times a second and,
/// with `record`, every frame into its file (frames repeated when the
/// pictures come late, so the file keeps time).
fn video(
    s: &CaptureSource,
    record: Option<(PathBuf, Instant)>,
    live: &Mutex<SourceLive>,
    stop: &AtomicBool,
) -> Option<Result<PathBuf, String>> {
    let mut pictures = match Pictures::open(s) {
        Ok(p) => p,
        Err(e) => {
            set_error(live, Some(e.clone()));
            return record.map(|_| Err(e));
        }
    };
    let (w, h) = ((s.width & !1).max(2), (s.height & !1).max(2));
    let rate = s.rate;
    let mut out = match &record {
        Some((path, _)) => {
            let params = VideoParams {
                width: w,
                height: h,
                frame_rate: rate,
                bit_depth: 8,
                alpha: false,
                bitrate: s.video_quality.video_bitrate(w, h, rate),
                max_bitrate: s.video_quality.video_bitrate(w, h, rate) * 2,
                ..VideoParams::default()
            };
            match open_output(path, s.video_codec.format(), Some(params), None) {
                Ok(o) => Some(o),
                Err(e) => {
                    set_error(live, Some(e.clone()));
                    return Some(Err(e));
                }
            }
        }
        None => None,
    };
    let frame = Duration::from_secs_f64(1.0 / rate.as_f64().max(1.0));
    let start = record.as_ref().map_or_else(Instant::now, |&(_, at)| at);
    let mut next: u64 = 0;
    let mut shown = Instant::now() - Duration::from_secs(1);
    let mut failed: Option<String> = None;
    let mut last: Option<Vec<u8>> = None;
    while !stop.load(Ordering::Relaxed) {
        match pictures.take() {
            Ok((pw, ph, bgra)) => {
                set_error(live, None);
                if shown.elapsed() >= Duration::from_millis(150) {
                    shown = Instant::now();
                    if let Ok(mut l) = live.lock() {
                        l.picture = Some(small(pw, ph, &bgra));
                        l.serial += 1;
                    }
                }
                if out.is_some() {
                    last = Some(to_nv12(pw, ph, &bgra, w, h));
                }
            }
            Err(e) => {
                set_error(live, Some(e));
                if !pictures.paced() {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
        if let Some(o) = &mut out {
            let now = (start.elapsed().as_secs_f64() * rate.as_f64()).floor() as u64;
            if let Some(data) = &last {
                while next <= now {
                    if let Err(e) = o.video_packed(data, next) {
                        failed = Some(e);
                        break;
                    }
                    next += 1;
                }
            }
            if failed.is_some() {
                break;
            }
        }
        if pictures.paced() {
            // Until the next frame's time.
            let due = if out.is_some() {
                start + frame.mul_f64(next as f64)
            } else {
                Instant::now() + Duration::from_millis(150)
            };
            if let Some(wait) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait.min(Duration::from_millis(200)));
            }
        }
    }
    let (path, _) = record?;
    let out = out?;
    if let Some(e) = failed {
        return Some(Err(e));
    }
    Some(out.finish().map(|_| path))
}

/// The device of a sound source and the stream format it is read in.
fn sound_device(s: &CaptureSource) -> Result<(cpal::Device, cpal::SupportedStreamConfig), String> {
    let host = cpal::default_host();
    let name_of = |d: &cpal::Device| d.description().ok().map(|d| d.name().to_string());
    match &s.kind {
        SourceKind::Input { device } => {
            let d = host
                .input_devices()
                .map_err(|e| e.to_string())?
                .find(|d| name_of(d).as_deref() == Some(device.as_str()))
                .ok_or("the input is not connected")?;
            let c = d.default_input_config().map_err(|e| e.to_string())?;
            Ok((d, c))
        }
        SourceKind::SystemSound => {
            let d = host.default_output_device().ok_or("no sound output")?;
            let c = d.default_output_config().map_err(|e| e.to_string())?;
            Ok((d, c))
        }
        _ => Err("not a sound".into()),
    }
}

/// A sound source's stream, its blocks (interleaved) sent to `rx`.
fn sound_stream(
    d: &cpal::Device,
    c: &cpal::SupportedStreamConfig,
) -> Result<(cpal::Stream, Receiver<Vec<f32>>), String> {
    let (tx, rx) = channel::<Vec<f32>>();
    let config: cpal::StreamConfig = c.config();
    let err = |_: cpal::Error| {};
    let stream = match c.sample_format() {
        cpal::SampleFormat::F32 => d.build_input_stream::<f32, _, _>(
            config,
            move |data: &[f32], _| {
                let _ = tx.send(data.to_vec());
            },
            err,
            None,
        ),
        cpal::SampleFormat::I16 => d.build_input_stream::<i16, _, _>(
            config,
            move |data: &[i16], _| {
                let _ = tx.send(data.iter().map(|&v| v as f32 / 32768.0).collect());
            },
            err,
            None,
        ),
        cpal::SampleFormat::I32 => d.build_input_stream::<i32, _, _>(
            config,
            move |data: &[i32], _| {
                let _ = tx.send(data.iter().map(|&v| v as f32 / 2_147_483_648.0).collect());
            },
            err,
            None,
        ),
        other => return Err(format!("sample format {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, rx))
}

/// A stream resampled linearly to another rate, block by block.
struct Resample {
    /// Input samples a sample out.
    step: f64,
    /// Where the next sample out is, in input samples from the last one of
    /// the block before.
    pos: f64,
    /// That last sample, each channel's.
    last: Vec<f32>,
}

impl Resample {
    fn new(from: u32, to: u32, channels: usize) -> Resample {
        Resample {
            step: from as f64 / to.max(1) as f64,
            pos: 1.0,
            last: vec![0.0; channels],
        }
    }

    fn block(&mut self, planar: Vec<Vec<f32>>) -> Vec<Vec<f32>> {
        let n = planar.first().map_or(0, Vec::len);
        if (self.step - 1.0).abs() < 1e-9 || n == 0 {
            return planar;
        }
        let mut out = vec![Vec::with_capacity((n as f64 / self.step) as usize + 1); planar.len()];
        while self.pos < n as f64 {
            let i = self.pos.floor() as usize;
            let f = (self.pos - i as f64) as f32;
            for ((o, ch), last) in out.iter_mut().zip(&planar).zip(&self.last) {
                let a = if i == 0 { *last } else { ch[i - 1] };
                o.push(a + (ch[i] - a) * f);
            }
            self.pos += self.step;
        }
        self.pos -= n as f64;
        for (l, ch) in self.last.iter_mut().zip(&planar) {
            *l = ch[n - 1];
        }
        out
    }
}

/// A sound source's thread: its levels and, with `record`, its sound into
/// its file (in the source's channels and rate).
fn audio(
    s: &CaptureSource,
    record: Option<(PathBuf, Instant)>,
    live: &Mutex<SourceLive>,
    stop: &AtomicBool,
) -> Option<Result<PathBuf, String>> {
    let opened = sound_device(s).and_then(|(d, c)| {
        let (stream, rx) = sound_stream(&d, &c)?;
        Ok((stream, rx, c.channels().max(1) as usize, c.sample_rate()))
    });
    let (stream, rx, device_channels, rate) = match opened {
        Ok(o) => o,
        Err(e) => {
            set_error(live, Some(e.clone()));
            return record.map(|_| Err(e));
        }
    };
    let channels = s.channels.clamp(1, 2) as usize;
    let mut out = match &record {
        Some((path, _)) => {
            let params = AudioParams {
                sample_rate: s.sample_rate,
                channels: channels as u16,
                bit_depth: 16,
                float: false,
                bitrate: s.audio_quality.audio_bitrate(channels as u16),
            };
            match open_output(path, s.audio_codec.format(), None, Some(params)) {
                Ok(o) => Some(o),
                Err(e) => {
                    set_error(live, Some(e.clone()));
                    return Some(Err(e));
                }
            }
        }
        None => None,
    };
    let mut peaks = vec![0.0f32; channels];
    let mut resample = Resample::new(rate, s.sample_rate, channels);
    let mut failed = None;
    while !stop.load(Ordering::Relaxed) {
        let Ok(block) = rx.recv_timeout(Duration::from_millis(100)) else {
            continue;
        };
        let frames = block.len() / device_channels;
        let mut planar = vec![Vec::with_capacity(frames); channels];
        for f in block.chunks_exact(device_channels) {
            for (c, ch) in planar.iter_mut().enumerate() {
                // A mono input fills both sides; more channels than kept
                // are left out.
                ch.push(f[c.min(device_channels - 1)]);
            }
        }
        for (p, ch) in peaks.iter_mut().zip(&planar) {
            let m = ch.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            *p = m.max(*p * 0.8);
        }
        if let Ok(mut l) = live.lock() {
            l.levels.clone_from(&peaks);
            l.error = None;
        }
        if let Some(o) = &mut out
            && let Err(e) = o.audio(&resample.block(planar))
        {
            failed = Some(e);
            break;
        }
    }
    drop(stream);
    let (path, _) = record?;
    let out = out?;
    if let Some(e) = failed {
        return Some(Err(e));
    }
    Some(out.finish().map(|_| path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_and_black_come_out_at_video_levels() {
        let mut bgra = vec![255u8; 4 * 4 * 4];
        bgra[..16].fill(0);
        let nv12 = to_nv12(4, 4, &bgra, 4, 4);
        assert_eq!(nv12.len(), 24);
        assert_eq!(nv12[0], 16, "black");
        assert_eq!(nv12[15], 235, "white");
        assert!(
            nv12[16..].iter().all(|&c| (127..=129).contains(&c)),
            "grays"
        );
    }

    #[test]
    fn a_narrower_picture_is_letterboxed() {
        // 2 × 4 white in 8 × 4: black on both sides.
        let bgra = vec![255u8; 2 * 4 * 4];
        let nv12 = to_nv12(2, 4, &bgra, 8, 4);
        assert_eq!(nv12[0], 16);
        assert_eq!(nv12[4], 235);
        assert_eq!(nv12[7], 16);
    }

    #[test]
    fn files_are_named_after_their_sources() {
        let d = CaptureDefaults::default();
        let s = CaptureSource::new(SourceKind::SystemSound, "Speakers: A/B".into(), &d);
        let dir = std::env::temp_dir().join(format!("bv-capture-names-{}", std::process::id()));
        let first = free_path(&dir, &s, &[]);
        assert_eq!(first.file_name().unwrap(), "Speakers_ A_B.mp3");
        let second = free_path(&dir, &s, std::slice::from_ref(&first));
        assert_eq!(second.file_name().unwrap(), "Speakers_ A_B (2).mp3");
    }

    #[test]
    fn sound_is_resampled_across_blocks() {
        let mut r = Resample::new(48_000, 24_000, 1);
        let a = r.block(vec![(0..4).map(|v| v as f32).collect()]);
        let b = r.block(vec![(4..8).map(|v| v as f32).collect()]);
        assert_eq!(a[0], [0.0, 2.0]);
        assert_eq!(b[0], [4.0, 6.0]);
        let mut same = Resample::new(48_000, 48_000, 1);
        assert_eq!(same.block(vec![vec![1.0, 2.0]])[0], [1.0, 2.0]);
    }

    #[test]
    fn bit_rates_follow_the_quality_and_the_picture() {
        let q = CaptureQuality::Medium;
        assert_eq!(q.video_bitrate(1920, 1080, FrameRate::FPS_30), 10_000_000);
        assert_eq!(q.audio_bitrate(2), 192_000);
        let s = CaptureSource::new(
            SourceKind::Input {
                device: "Mic".into(),
            },
            "Mic".into(),
            &CaptureDefaults::default(),
        );
        assert_eq!(
            s.summary("Stereo", "Mono"),
            "MP3; 192 kbit/s; Stereo; 48000 Hz"
        );
    }
}
