//! Rendering the timeline to a file (File › Render As…), in the background.
//!
//! The video is composited at the file's size and frame rate (letterboxed
//! when its shape differs from the project's, unless stretched), the audio
//! is mixed at the file's sample rate and channel count, and both go to the
//! format's writer: BananaV's own (MP4/MOV, WAV, images…) fed by the
//! encoders of this computer (processor or graphics card).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use bv_core::{ChannelLayout, FrameRate, ImageRgbaF32, SampleRate, Tick, TimeRange};
use bv_mixer::AudioPool;
use bv_render::VideoPool;
use bv_timeline::Project;
use serde::{Deserialize, Serialize};

mod audio;
mod dvd;
mod formats;
pub(crate) mod images;
mod mpeg;
mod smart;
mod video;

pub use formats::{Availability, Category, Encoder, FileFormat, ProResProfile};

/// How a compressed video's bits are spent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum RateMode {
    /// Constant bit rate.
    Cbr,
    /// Variable bit rate: the average, up to the maximum.
    #[default]
    Vbr,
    /// Constant quality: the bit rate follows the picture.
    Quality,
}

/// The video of a rendered file.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct VideoParams {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    /// Bits per sample: 8, 10 (HEVC, AV1, ProRes, DPX), 16 (PNG, TIFF),
    /// 16 or 32 float (EXR).
    pub bit_depth: u8,
    /// Keep the alpha channel (formats that have one).
    pub alpha: bool,
    /// The processor or a graphics card (compressed video).
    pub encoder: Encoder,
    pub rate_mode: RateMode,
    /// Bits per second (average).
    pub bitrate: u32,
    pub max_bitrate: u32,
    /// 0–100: constant-quality level, JPEG/HEIC quality.
    pub quality: u8,
    /// Frames from one keyframe to the next (0: one per second).
    pub keyframe_interval: u32,
    pub prores: ProResProfile,
    /// The template's Project tab: its video rendering quality (`None`:
    /// the project's) and, under ACES, the color space frames leave in
    /// (`None`: VEGAS's Default, through the project's view transform).
    #[serde(default)]
    pub rendering_quality: Option<bv_timeline::RenderQuality>,
    #[serde(default)]
    pub color_space: Option<bv_timeline::ColorSpace>,
    /// Interlaced frames (the template's Field order): two fields a frame,
    /// the one shown first drawn at the frame's time and the other half a
    /// frame later.
    #[serde(default, with = "field_order_serde")]
    pub field_order: bv_core::FieldOrder,
}

/// `FieldOrder` in templates: "progressive", "upper" or "lower".
mod field_order_serde {
    use bv_core::FieldOrder;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(f: &FieldOrder, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(match f {
            FieldOrder::Progressive => "progressive",
            FieldOrder::UpperFirst => "upper",
            FieldOrder::LowerFirst => "lower",
        })
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<FieldOrder, D::Error> {
        Ok(match String::deserialize(d)?.as_str() {
            "upper" => FieldOrder::UpperFirst,
            "lower" => FieldOrder::LowerFirst,
            _ => FieldOrder::Progressive,
        })
    }
}

/// `later`'s rows of the field shown second woven into `first` (rows of
/// `row_bytes` from byte `at`, `rows` of them): upper field first, the odd
/// rows; lower first, the even ones.
fn weave(first: &mut [u8], later: &[u8], at: usize, rows: usize, row_bytes: usize, upper: bool) {
    let second = usize::from(upper);
    for r in (second..rows).step_by(2) {
        let a = at + r * row_bytes;
        if let (Some(dst), Some(src)) =
            (first.get_mut(a..a + row_bytes), later.get(a..a + row_bytes))
        {
            dst.copy_from_slice(src);
        }
    }
}

/// Two frames packed 4:2:0 (`size`, 8 or 10 bits) woven into an interlaced
/// one: the luma's rows and the chroma's alike.
fn weave_packed(first: &mut [u8], later: &[u8], (w, h): (u32, u32), ten_bit: bool, upper: bool) {
    let row = w as usize * if ten_bit { 2 } else { 1 };
    let h = h as usize;
    weave(first, later, 0, h, row, upper);
    weave(first, later, h * row, h / 2, row, upper);
}

#[cfg(test)]
mod weave_tests {
    use super::*;

    /// Upper field first: the odd rows from the later picture (the luma's
    /// and the chroma's); lower first, the even ones.
    #[test]
    fn fields_are_woven() {
        let (w, h) = (4u32, 4u32);
        let frame = |v: u8| vec![v; (w * h + w * h / 2) as usize];
        let mut a = frame(1);
        weave_packed(&mut a, &frame(2), (w, h), false, true);
        let rows: Vec<u8> = (0..6).map(|r| a[r * 4]).collect();
        assert_eq!(rows, [1, 2, 1, 2, 1, 2]);
        let mut b = frame(1);
        weave_packed(&mut b, &frame(2), (w, h), false, false);
        let rows: Vec<u8> = (0..6).map(|r| b[r * 4]).collect();
        assert_eq!(rows, [2, 1, 2, 1, 2, 1]);
        let mut img = ImageRgbaF32::transparent(2, 3);
        let mut later = ImageRgbaF32::transparent(2, 3);
        later.data.iter_mut().for_each(|p| *p = [1.0; 4]);
        weave_image(&mut img, &later, true);
        assert_eq!(
            (img.data[0][0], img.data[2][0], img.data[4][0]),
            (0.0, 1.0, 0.0)
        );
    }
}

/// Two pictures woven into an interlaced frame.
fn weave_image(first: &mut ImageRgbaF32, later: &ImageRgbaF32, upper: bool) {
    let w = first.width as usize;
    let second = usize::from(upper);
    for y in (second..first.height as usize).step_by(2) {
        let row = y * w..(y + 1) * w;
        if let Some(src) = later.data.get(row.clone()) {
            first.data[row].copy_from_slice(src);
        }
    }
}

impl Default for VideoParams {
    fn default() -> VideoParams {
        VideoParams {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::FPS_29_97,
            bit_depth: 8,
            alpha: false,
            encoder: Encoder::Software,
            rate_mode: RateMode::Vbr,
            bitrate: 16_000_000,
            max_bitrate: 24_000_000,
            quality: 90,
            keyframe_interval: 0,
            prores: ProResProfile::Standard,
            rendering_quality: None,
            color_space: None,
            field_order: bv_core::FieldOrder::Progressive,
        }
    }
}

/// The audio of a rendered file.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioParams {
    pub sample_rate: u32,
    pub channels: u16,
    /// PCM bits per sample (8, 16, 24, 32).
    pub bit_depth: u8,
    /// 32-bit IEEE float instead of integers.
    pub float: bool,
    /// Bits per second (compressed audio).
    pub bitrate: u32,
}

impl Default for AudioParams {
    fn default() -> AudioParams {
        AudioParams {
            sample_rate: 48_000,
            channels: 2,
            bit_depth: 16,
            float: false,
            bitrate: 192_000,
        }
    }
}

/// The Render dialog's options (its gear menu).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderOptions {
    /// Fill the file's frame instead of letterboxing a frame of another
    /// shape.
    pub stretch: bool,
    /// Turn the video by File › Properties › Video › Output rotation.
    pub use_rotation: bool,
    /// The project's markers become the file's markers or chapters.
    pub save_markers: bool,
    /// Write `<name>_loud.txt` with the loudness of the rendered audio.
    pub loudness_log: bool,
    /// Swap video files on render: a file already there is replaced only
    /// once the render has finished well (it is rendered beside it, then
    /// put in its place).
    #[serde(default)]
    pub swap_files: bool,
    /// Save project as path reference in rendered file: the project's path
    /// kept in the file (MP4, MOV and M4A files' user data, WAV files'
    /// `bvpr` chunk), for Edit Source Project.
    #[serde(default)]
    pub path_reference: bool,
    /// Enable multichannel mapping: Wave and Wave64 files get the busses
    /// the project's channel mapping picks (`Project::mapped_strips`), each
    /// in channels of its own.
    #[serde(default)]
    pub multichannel: bool,
    /// Options › Preferences › File I/O: "Enable no-recompress rendering":
    /// frames left as their media had them pass into the file as they are
    /// (smart rendering, `smart.rs`), where the format allows it.
    #[serde(default)]
    pub no_recompress: bool,
    /// A DVD-Video folder (`VIDEO_TS`, `AUDIO_TS`) at the path, of an
    /// MPEG-2 template's video (Tools › Burn Disc › DVD).
    #[serde(default)]
    pub dvd: bool,
    /// The DVD's picture shape, 16:9 (`true`) or 4:3, the project's
    /// picture fitted in it (letterboxed) before its pixels are stretched
    /// to the frame's (`None`: the project's own shape, stretched).
    #[serde(default)]
    pub dvd_wide: Option<bool>,
}

impl RenderOptions {
    /// How the project's picture fills the file's frame.
    pub(crate) fn fit(&self) -> Fit {
        match self.dvd_wide {
            Some(true) if self.dvd => Fit::Within(16, 9),
            Some(false) if self.dvd => Fit::Within(4, 3),
            _ if self.stretch => Fit::Stretch,
            _ => Fit::Letterbox,
        }
    }
}

/// How the project's picture fills a file's frame of another shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fit {
    /// As large as fits, the rest black (or clear).
    Letterbox,
    /// Stretched to fill it.
    Stretch,
    /// Letterboxed in a picture of this shape whose pixels are then
    /// stretched to the frame's (a DVD's 4:3 or 16:9 in 720 × 480).
    Within(u32, u32),
}

/// What to render and where.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderSettings {
    pub format: FileFormat,
    /// The file. For image sequences, the files are numbered from it:
    /// `name_000000.ext`, `name_000001.ext`…
    pub path: PathBuf,
    pub range: TimeRange,
    pub video: Option<VideoParams>,
    pub audio: Option<AudioParams>,
    pub options: RenderOptions,
    /// The project's file (saved), for the path reference.
    pub project_file: Option<PathBuf>,
}

impl RenderSettings {
    /// The path the file keeps of its project: the project's file when
    /// the option asks for it and the project is saved.
    pub(crate) fn reference(&self) -> Option<String> {
        self.project_file
            .as_ref()
            .filter(|_| self.options.path_reference)
            .map(|p| p.display().to_string())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderProgress {
    pub stage: String,
    pub done: u64,
    pub total: u64,
    pub finished: bool,
    pub cancelled: bool,
    pub error: Option<String>,
    pub elapsed_secs: f64,
    /// Bytes written so far.
    pub bytes: u64,
    /// Frames per second of the last second.
    pub fps: f64,
    /// Frames per second since the start.
    pub avg_fps: f64,
    /// `done` counts video frames (else blocks of audio).
    pub video: bool,
    /// The encoder doing the video ("NVIDIA H.264 Encoder MFT").
    pub encoder: String,
    /// The files written (one, or every image of a sequence).
    pub files: Vec<PathBuf>,
    /// Frames passed into the file as their media had them (smart
    /// rendering).
    pub passed: u64,
}

impl RenderProgress {
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.done as f32 / self.total as f32
        }
    }
}

/// The `n`-th file of an image sequence rendered to `path`.
pub fn sequence_path(path: &Path, n: u64) -> PathBuf {
    let stem = path
        .file_stem()
        .map_or("BananaV".into(), |s| s.to_string_lossy().to_string());
    let ext = path
        .extension()
        .map_or(String::new(), |e| e.to_string_lossy().to_string());
    path.with_file_name(format!("{stem}_{n:06}.{ext}"))
}

/// Where a render's frames and samples go. It is written on a thread of
/// its own while the next frames are drawn.
pub(crate) trait Output: Send {
    /// Frame `n` (from 0) of the render.
    fn video(&mut self, _img: &ImageRgbaF32, _n: u64) -> Result<(), String> {
        Ok(())
    }

    /// Whether the video encoder takes its frames packed as 4:2:0 (NV12,
    /// or P010 when `Some(true)`) instead of as images: the graphics card
    /// then converts them.
    fn packed_420(&self) -> Option<bool> {
        None
    }

    /// Frame `n` packed as [`packed_420`](Self::packed_420) says.
    fn video_packed(&mut self, _data: &[u8], _n: u64) -> Result<(), String> {
        Ok(())
    }

    /// What compressed frames it takes as they are (smart rendering).
    fn passable(&self) -> Option<smart::Passable> {
        None
    }

    /// Frame `n`, compressed already as [`passable`](Self::passable) says.
    fn compressed(&mut self, _data: &[u8], _n: u64) -> Result<(), String> {
        Err("this file takes no compressed frames".into())
    }

    /// The next block of audio: one vector per channel.
    fn audio(&mut self, _planar: &[Vec<f32>]) -> Result<(), String> {
        Ok(())
    }

    /// The encoder of the video, for the progress window.
    fn encoder_name(&self) -> String {
        String::new()
    }

    /// Bytes written so far.
    fn bytes(&self) -> u64;

    /// Closes the file(s).
    fn finish(self: Box<Self>) -> Result<Vec<PathBuf>, String>;
}

/// How often a render tells the interface how far it is, and how often a
/// frame goes to the preview ("Show Video in Preview Window"): told at
/// each frame, the interface drew itself as fast as the render went and
/// took the processor and the graphics card the render needs.
pub(crate) const TELL_EVERY: std::time::Duration = std::time::Duration::from_millis(250);

/// How many painters draw a render's frames at once: two on the graphics
/// card when plug-ins draw them (`heavy`) — one painter's frame leaves the
/// card idle while plug-ins wait on the processor: Sapphire's S_ZapFrom
/// over a 1080p60 recording, 22 frames a second with one painter, 32 with
/// two —; else one — without plug-ins the encoder or the readers set the
/// pace, and two painters only read the media twice (a crossfade 4 %
/// slower) —, on the processor (one frame keeps all its threads busy) and
/// for smart rendering (its frames come out of the file in order).
/// `BV_RENDER_PAINTERS` sets it, to measure.
fn render_painters(on_card: bool, heavy: bool, smart: bool) -> usize {
    #[cfg(test)]
    {
        let n = FORCED_PAINTERS.load(Ordering::Relaxed);
        if n > 0 {
            return n;
        }
    }
    if let Some(n) = std::env::var("BV_RENDER_PAINTERS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
    {
        return n.clamp(1, 8);
    }
    if on_card && heavy && !smart { 2 } else { 1 }
}

/// The painters the tests ask a render for (0: none asked).
#[cfg(test)]
static FORCED_PAINTERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The frames a render shows in the preview while it runs.
#[derive(Default)]
pub(crate) struct Shown {
    /// Show them ("Show Video in Preview Window").
    pub on: AtomicBool,
    pub frame: Mutex<Option<Arc<crate::PreviewFrame>>>,
}

pub(crate) struct RenderJob {
    progress: Arc<Mutex<RenderProgress>>,
    cancel: Arc<AtomicBool>,
    shown: Arc<Shown>,
    thread: Option<JoinHandle<()>>,
}

impl RenderJob {
    pub(crate) fn start(
        project: Arc<Project>,
        settings: RenderSettings,
        repaint: crate::RepaintFn,
        accel: Arc<crate::accel::Accel>,
    ) -> RenderJob {
        let progress = Arc::new(Mutex::new(RenderProgress {
            stage: "Preparing".into(),
            ..RenderProgress::default()
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let shown = Arc::new(Shown::default());
        let thread = {
            let progress = progress.clone();
            let cancel = cancel.clone();
            let shown = shown.clone();
            std::thread::Builder::new()
                .name("bv-render".into())
                // Plug-in effects run on it.
                .stack_size(bv_ofx::PLUGIN_STACK)
                .spawn(move || {
                    let started = Instant::now();
                    let mut painter =
                        crate::accel::Painter::new(accel, crate::plugins::Purpose::File);
                    // Swap video files on render: into a file beside the
                    // one there, put in its place when all went well.
                    let swap = swap_target(&settings);
                    let mut s = settings.clone();
                    if let Some((_, temp)) = &swap {
                        s.path = temp.clone();
                    }
                    let result = run(
                        &project,
                        &s,
                        &progress,
                        &cancel,
                        &shown,
                        &repaint,
                        started,
                        &mut painter,
                    );
                    let result = match swap {
                        Some((target, temp)) => swapped(result, &target, &temp, &cancel),
                        None => result,
                    };
                    let mut p = progress.lock().expect("progress");
                    p.finished = true;
                    p.elapsed_secs = started.elapsed().as_secs_f64();
                    p.cancelled = cancel.load(Ordering::Relaxed);
                    match result {
                        Ok(files) => p.files = files,
                        Err(e) => p.error = Some(e),
                    }
                    drop(p);
                    repaint();
                })
                .expect("could not start the render thread")
        };
        RenderJob {
            progress,
            cancel,
            shown,
            thread: Some(thread),
        }
    }

    /// Whether frames go to the preview while rendering.
    pub(crate) fn show_video(&self, on: bool) {
        self.shown.on.store(on, Ordering::Relaxed);
        if !on {
            *self.shown.frame.lock().expect("frame") = None;
        }
    }

    /// The last frame rendered, while frames go to the preview.
    pub(crate) fn shown_frame(&self) -> Option<Arc<crate::PreviewFrame>> {
        self.shown.frame.lock().expect("frame").clone()
    }

    pub(crate) fn progress(&self) -> RenderProgress {
        self.progress.lock().expect("progress").clone()
    }

    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for RenderJob {
    fn drop(&mut self) {
        self.cancel();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn layout_of(channels: u16) -> ChannelLayout {
    match channels {
        1 => ChannelLayout::Mono,
        2 => ChannelLayout::Stereo,
        6 => ChannelLayout::Surround51,
        n => ChannelLayout::Discrete(n),
    }
}

/// The project as the file wants it: its sample rate and channels (its
/// own with multichannel mapping: the busses are mixed as they are), and
/// without the output rotation when the option is off.
fn project_for(project: &Project, s: &RenderSettings, mapped: bool) -> Project {
    let mut p = project.clone();
    if let Some(a) = &s.audio {
        p.settings.sample_rate = SampleRate(a.sample_rate);
        if !mapped {
            p.settings.channels = layout_of(a.channels);
        }
    }
    if !s.options.use_rotation {
        p.settings.rotation = bv_timeline::OutputRotation::None;
    }
    // The template's Project tab.
    if let Some(v) = &s.video {
        if let Some(q) = v.rendering_quality {
            p.settings.render_quality = q;
        }
        p.settings.color.output = v.color_space;
    }
    p
}

/// Multichannel mapping's strips, when the option is on for a Wave or
/// Wave64 file and the project maps any.
fn mapped_strips(project: &Project, s: &RenderSettings) -> Vec<bv_timeline::MappedStrip> {
    let wave = matches!(s.format, FileFormat::Wav | FileFormat::Wave64);
    if s.options.multichannel && wave && s.audio.is_some() {
        project.mapped_strips()
    } else {
        Vec::new()
    }
}

/// The file's channels with multichannel mapping: as many as the mapping
/// fills, or the template's when it has more (the rest silent).
fn with_mapped_channels(s: &RenderSettings, project: &Project, mapped: bool) -> RenderSettings {
    let mut s = s.clone();
    if let Some(a) = s.audio.as_mut().filter(|_| mapped) {
        a.channels = a.channels.max(project.mapped_channels() as u16);
    }
    s
}

/// The file's `channels` with multichannel mapping: each mapped strip's
/// sound in order — the Master's mix, a bus's output — in its layout's
/// channels, or their mean with Mono downmix; the file's other channels
/// silent.
fn mapped_planar(
    mapped: &[bv_timeline::MappedStrip],
    master: Vec<Vec<f32>>,
    busses: &[(bv_timeline::BusId, bv_core::AudioBuffer)],
    channels: usize,
) -> Vec<Vec<f32>> {
    let frames = master.first().map_or(0, Vec::len);
    let layout = master.len();
    let mut out: Vec<Vec<f32>> = Vec::with_capacity(channels);
    for m in mapped {
        let from = match m.bus {
            None => Some(&master),
            Some(id) => busses
                .iter()
                .find(|(b, _)| *b == id)
                .map(|(_, buf)| &buf.channels),
        };
        match from {
            Some(chs) if m.mono => {
                let k = 1.0 / chs.len().max(1) as f32;
                out.push(
                    (0..frames)
                        .map(|i| chs.iter().map(|c| c[i]).sum::<f32>() * k)
                        .collect(),
                );
            }
            Some(chs) => out.extend(chs.iter().cloned()),
            None => {
                let n = if m.mono { 1 } else { layout };
                out.extend((0..n).map(|_| vec![0.0; frames]));
            }
        }
    }
    out.resize_with(channels, || vec![0.0; frames]);
    out
}

/// The frame at `t` in the file's `w`×`h`: the project's output fitted in
/// it as `fit` says.
pub(crate) fn frame_at(
    project: &Project,
    t: Tick,
    (w, h): (u32, u32),
    fit: Fit,
    alpha: bool,
    pool: &mut VideoPool,
    painter: &mut crate::accel::Painter,
) -> ImageRgbaF32 {
    let background = if alpha {
        [0.0; 4]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };
    let (pw, ph) = project.settings.output_size();
    let (pw, ph) = (pw.max(1) as u64, ph.max(1) as u64);
    let same_shape = pw * h as u64 == ph * w as u64;
    if same_shape && !matches!(fit, Fit::Within(..)) {
        return painter.linear(project, t, (w, h), pool, background);
    }
    // The project's shape, as large as fits.
    let (fw, fh) = if pw * h as u64 > ph * w as u64 {
        (w, ((w as u64 * ph / pw) as u32).max(2) & !1)
    } else {
        (((h as u64 * pw / ph) as u32).max(2) & !1, h)
    };
    let mut img = painter.linear(project, t, (fw, fh), pool, background);
    let (fw, fh) = match fit {
        Fit::Letterbox => (fw, fh),
        Fit::Stretch => return resize(&img, w, h),
        Fit::Within(sw, sh) => {
            // The picture's share of the shape, in the frame's pixels.
            let (sw, sh) = (sw as u64, sh as u64);
            let (iw, ih) = if pw * sh > ph * sw {
                (w, ((h as u64 * ph * sw / (pw * sh)) as u32).max(2) & !1)
            } else {
                (((w as u64 * pw * sh / (ph * sw)) as u32).max(2) & !1, h)
            };
            img = resize(&img, iw, ih);
            (iw, ih)
        }
    };
    let mut out = ImageRgbaF32::filled(w, h, background);
    let (ox, oy) = ((w - fw) / 2, (h - fh) / 2);
    for y in 0..fh {
        let src = &img.data[(y * fw) as usize..((y + 1) * fw) as usize];
        let at = ((oy + y) * w + ox) as usize;
        out.data[at..at + fw as usize].copy_from_slice(src);
    }
    out
}

/// Bilinear resize (stretching a frame to another shape).
fn resize(img: &ImageRgbaF32, w: u32, h: u32) -> ImageRgbaF32 {
    let mut out = ImageRgbaF32::transparent(w, h);
    let (sw, sh) = (img.width as f32, img.height as f32);
    for y in 0..h {
        let fy = ((y as f32 + 0.5) * sh / h as f32 - 0.5).clamp(0.0, sh - 1.0);
        let (y0, ty) = (fy.floor() as u32, fy.fract());
        let y1 = (y0 + 1).min(img.height - 1);
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * sw / w as f32 - 0.5).clamp(0.0, sw - 1.0);
            let (x0, tx) = (fx.floor() as u32, fx.fract());
            let x1 = (x0 + 1).min(img.width - 1);
            let (a, b, c, d) = (
                img.pixel(x0, y0),
                img.pixel(x1, y0),
                img.pixel(x0, y1),
                img.pixel(x1, y1),
            );
            let mut px = [0.0f32; 4];
            for k in 0..4 {
                let top = a[k] + (b[k] - a[k]) * tx;
                let bottom = c[k] + (d[k] - c[k]) * tx;
                px[k] = top + (bottom - top) * ty;
            }
            *out.pixel_mut(x, y) = px;
        }
    }
    out
}

pub(crate) fn open_output(
    project: &Project,
    s: &RenderSettings,
) -> Result<Box<dyn Output>, String> {
    use FileFormat::*;
    if let Some(dir) = s.path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    match s.format {
        AvcMp4 | HevcMp4 | Av1Mp4 => video::mp4(project, s),
        Wmv => video::wmv(s, project.settings.pixel_format.is_video_levels()),
        ProRes => video::prores(project, s),
        Png | Bmp | Tiff | Dpx | Exr | Dds | Jpeg | Heic | WmPhoto => images::open(s),
        Wav | Wave64 | Aiff | Flac | Mp3 | Aac | Wma => audio::open(project, s),
        Mpeg2 if s.options.dvd => dvd::open(project, s),
        Mpeg1 | Mpeg2 => mpeg::open(project, s),
        f => Err(format!("{} is not available yet", f.name())),
    }
}

/// The frame at `t` packed for the video encoder (see
/// `Output::packed_420`): drawn and converted on the graphics card when the
/// file's frame has the project's shape; letterboxed or stretched, then
/// converted, on the processor otherwise.
#[allow(clippy::too_many_arguments)]
fn packed_at(
    project: &Project,
    t: Tick,
    (w, h): (u32, u32),
    fit: Fit,
    ten_bit: bool,
    pool: &mut VideoPool,
    painter: &mut crate::accel::Painter,
) -> Vec<u8> {
    let (pw, ph) = project.settings.output_size();
    if pw as u64 * h as u64 == ph as u64 * w as u64 && !matches!(fit, Fit::Within(..)) {
        return painter.yuv420(project, t, (w, h), pool, ten_bit);
    }
    let img = frame_at(project, t, (w, h), fit, false, pool, painter);
    let video_levels = project.settings.pixel_format.is_video_levels();
    bv_color::linear_to_420_levels(&img, ten_bit, video_levels)
}

/// What the drawing thread hands the writing one.
enum Work {
    /// Frame `n` as an image.
    Image(ImageRgbaF32, u64),
    /// Frame `n` packed for the video encoder.
    Packed(Vec<u8>, u64),
    /// Frame `n` compressed already (smart rendering).
    Compressed(Vec<u8>, u64),
    /// A block of audio: one vector per channel.
    Audio(Vec<Vec<f32>>),
}

/// The writing thread: everything sent goes to `out` (its bytes counted
/// in `bytes`) until the sender is gone or writing fails. Gives `out` back
/// with how it went.
fn write(
    mut out: Box<dyn Output>,
    work: Receiver<Work>,
    bytes: &AtomicU64,
) -> (Box<dyn Output>, Result<(), String>) {
    for w in work {
        let r = match w {
            Work::Image(img, n) => out.video(&img, n),
            Work::Packed(data, n) => out.video_packed(&data, n),
            Work::Compressed(data, n) => out.compressed(&data, n),
            Work::Audio(planar) => out.audio(&planar),
        };
        bytes.store(out.bytes(), Ordering::Relaxed);
        if let Err(e) = r {
            return (out, Err(e));
        }
    }
    (out, Ok(()))
}

/// A small copy, for the preview, of a frame packed for the encoder
/// (BT.709, limited range).
fn preview_of_packed(
    data: &[u8],
    (w, h): (u32, u32),
    ten_bit: bool,
    time: Tick,
    n: u64,
) -> crate::PreviewFrame {
    let scale = (640.0 / w.max(1) as f32)
        .min(360.0 / h.max(1) as f32)
        .min(1.0);
    let (sw, sh) = (
        ((w as f32 * scale) as u32).max(1),
        ((h as f32 * scale) as u32).max(1),
    );
    let (wu, hu) = (w as usize, h as usize);
    let depth = if ten_bit { 10 } else { 8 };
    let sample = |i: usize| -> f32 {
        if ten_bit {
            (u16::from_le_bytes([data[2 * i], data[2 * i + 1]]) >> 6) as f32
        } else {
            data[i] as f32
        }
    };
    let n_ = bv_color::matrix::Normalizer::new(bv_core::ColorRange::Limited, depth);
    let (kr, kb) = (0.2126, 0.0722);
    let mut rgba = Vec::with_capacity(sw as usize * sh as usize * 4);
    for y in 0..sh {
        let sy = (((y as f32 + 0.5) / scale) as usize).min(hu - 1);
        for x in 0..sw {
            let sx = (((x as f32 + 0.5) / scale) as usize).min(wu - 1);
            let c = wu * hu + (sy / 2) * wu + (sx / 2) * 2;
            let [r, g, b] = bv_color::matrix::ycbcr_to_rgb(
                kr,
                kb,
                n_.luma(sample(sy * wu + sx)),
                n_.chroma(sample(c)),
                n_.chroma(sample(c + 1)),
            );
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            rgba.extend_from_slice(&[q(r), q(g), q(b), 255]);
        }
    }
    crate::PreviewFrame {
        time,
        generation: u64::MAX - n,
        exact: true,
        width: sw,
        height: sh,
        drawn: (sw, sh),
        room: (0, 0),
        rgba,
    }
}

/// A small copy of a rendered frame for the preview (over black).
fn preview_of(img: &ImageRgbaF32, time: Tick, n: u64) -> crate::PreviewFrame {
    let scale = (640.0 / img.width.max(1) as f32)
        .min(360.0 / img.height.max(1) as f32)
        .min(1.0);
    let w = ((img.width as f32 * scale) as u32).max(1);
    let h = ((img.height as f32 * scale) as u32).max(1);
    let mut small = ImageRgbaF32::transparent(w, h);
    for y in 0..h {
        let sy = ((y as f32 + 0.5) / scale) as u32;
        for x in 0..w {
            let sx = ((x as f32 + 0.5) / scale) as u32;
            let px = img.pixel(sx.min(img.width - 1), sy.min(img.height - 1));
            *small.pixel_mut(x, y) = [px[0], px[1], px[2], 1.0];
        }
    }
    crate::PreviewFrame {
        time,
        generation: u64::MAX - n,
        exact: true,
        width: w,
        height: h,
        drawn: (w, h),
        room: (0, 0),
        rgba: bv_color::linear_to_display_rgba8(&small, bv_core::TransferCharacteristic::Bt709),
    }
}

/// A canceled render leaves nothing behind: everything rendered is lost.
fn discard(out: Box<dyn Output>, s: &RenderSettings) {
    let mut files = out.finish().unwrap_or_default();
    if s.format.category() != Category::ImageSequence {
        files.push(s.path.clone());
    }
    for f in files {
        let _ = std::fs::remove_file(f);
    }
}

/// Writes `<name>_loud.txt` next to the file.
fn write_loudness_log(path: &Path, r: &bv_mixer::loudness::LoudnessReport) -> Result<(), String> {
    let stem = path
        .file_stem()
        .map_or("BananaV".into(), |s| s.to_string_lossy().to_string());
    let log = path.with_file_name(format!("{stem}_loud.txt"));
    let fmt = |v: f64| {
        if v <= -199.0 {
            "-inf".to_string()
        } else {
            format!("{v:.2}")
        }
    };
    let text = format!(
        "Loudness of {}\r\n\r\nIntegrated: {} LUFS\r\nLoudness range: {:.2} LU\r\nMaximum momentary: {} LUFS\r\nMaximum short-term: {} LUFS\r\nSample peak: {} dBFS\r\nDuration: {:.3} s\r\n",
        path.file_name()
            .map_or(String::new(), |n| n.to_string_lossy().to_string()),
        fmt(r.integrated),
        r.range,
        fmt(r.max_momentary),
        fmt(r.max_short_term),
        fmt(r.sample_peak),
        r.duration,
    );
    std::fs::write(&log, text).map_err(|e| format!("{}: {e}", log.display()))
}

/// The settings as the format can take them: AAC and MP3 rates and
/// channel counts, FLAC depths, even sizes for subsampled chroma.
fn normalized(s: &RenderSettings) -> RenderSettings {
    use FileFormat::*;
    let mut s = s.clone();
    let format = s.format;
    if let Some(a) = &mut s.audio {
        match format {
            AvcMp4 | HevcMp4 | Av1Mp4 | Aac => {
                a.sample_rate = video::aac_rate(a.sample_rate);
                if !matches!(a.channels, 1 | 2 | 6) {
                    a.channels = 2;
                }
            }
            Mp3 => {
                if !matches!(a.sample_rate, 32_000 | 44_100 | 48_000) {
                    a.sample_rate = 48_000;
                }
                a.channels = a.channels.clamp(1, 2);
            }
            Wma | Wmv => {
                if !matches!(a.sample_rate, 44_100 | 48_000) {
                    a.sample_rate = 48_000;
                }
                a.channels = a.channels.clamp(1, 2);
            }
            // MPEG audio: Layer II's rates, mono or stereo.
            Mpeg1 | Mpeg2 => {
                if !matches!(a.sample_rate, 32_000 | 44_100 | 48_000) {
                    a.sample_rate = 48_000;
                }
                a.channels = a.channels.clamp(1, 2);
            }
            Flac => a.bit_depth = if a.bit_depth > 16 { 24 } else { 16 },
            _ => {}
        }
    }
    if let Some(v) = &mut s.video {
        if matches!(format, AvcMp4 | HevcMp4 | Av1Mp4 | Wmv | ProRes) {
            v.width = (v.width & !1).max(2);
            v.height = (v.height & !1).max(2);
        }
        v.width = v.width.max(1);
        v.height = v.height.max(1);
    }
    // MPEG: widths of whole macroblocks, even heights; DVD's, HDV's and
    // Video CD's frames are filled (their shape is the sequence headers').
    if matches!(format, Mpeg1 | Mpeg2)
        && let Some(v) = &mut s.video
    {
        v.width = (v.width / 16 * 16).max(16);
        v.height = (v.height & !1).max(2);
        if matches!(
            (v.width, v.height),
            (720 | 704, 480 | 576) | (1440, 1080) | (352, 240 | 288)
        ) {
            s.options.stretch = true;
        }
    }
    s
}

#[allow(clippy::too_many_arguments)]
fn run(
    project: &Project,
    s: &RenderSettings,
    progress: &Mutex<RenderProgress>,
    cancel: &AtomicBool,
    shown: &Shown,
    repaint: &crate::RepaintFn,
    started: Instant,
    painter: &mut crate::accel::Painter,
) -> Result<Vec<PathBuf>, String> {
    let s = &normalized(s);
    // Multichannel mapping: the busses' sound in the file's channels.
    let mapped = mapped_strips(project, s);
    let s = &with_mapped_channels(s, project, !mapped.is_empty());
    let project = &project_for(project, s, !mapped.is_empty());
    let video = s.video.filter(|_| s.format.has_video());
    let audio = s.audio.filter(|_| s.format.has_audio());
    let (first_frame, frames, rate) = match &video {
        Some(v) => {
            let rate = v.frame_rate;
            let first = rate.frame_at(s.range.start);
            let last = rate.frame_at(s.range.end() - Tick(1)) + 1;
            (first, (last - first).max(0) as u64, rate)
        }
        None => (0, 0, FrameRate::FPS_30),
    };
    let sr = SampleRate(audio.map_or(48_000, |a| a.sample_rate));
    let first_sample = sr.sample_at(s.range.start);
    let end_sample = sr.sample_at(s.range.end());
    let total = if video.is_some() {
        frames
    } else {
        ((end_sample - first_sample).max(0) as u64).div_ceil(4800)
    };
    let out = open_output(project, s)?;
    let encoder = out.encoder_name();
    let packed = out.packed_420();
    // Smart rendering (Options › Preferences › File I/O's "Enable
    // no-recompress rendering"), where the format takes compressed frames.
    let mut smart = s
        .options
        .no_recompress
        .then(|| out.passable())
        .flatten()
        .map(smart::SmartRender::new);
    // The file is written on a thread of its own while the next frames
    // are drawn (a few frames ahead at most).
    let bytes = Arc::new(AtomicU64::new(0));
    let (work, received): (SyncSender<Work>, Receiver<Work>) = std::sync::mpsc::sync_channel(4);
    let writer = {
        let bytes = bytes.clone();
        std::thread::Builder::new()
            .name("bv-render-writer".into())
            .spawn(move || write(out, received, &bytes))
            .map_err(|e| e.to_string())?
    };
    // What the writer says when it stops: its output and how it went.
    let finish_writer = |work: SyncSender<Work>| -> Result<Box<dyn Output>, String> {
        drop(work);
        let (out, r) = writer
            .join()
            .map_err(|_| "the writing thread failed".to_string())?;
        r.map(|()| out)
    };
    let mut meter = (s.options.loudness_log && audio.is_some()).then(|| {
        bv_mixer::loudness::LoudnessMeter::new(sr.0, project.settings.channels.channel_count())
    });
    let mut last_report = Instant::now();
    let mut frames_since = 0u64;
    let mut fps = 0.0;
    let mut told: Option<(Instant, String)> = None;
    let mut update = |stage: &str, done: u64, frames_now: u64| {
        let bytes = bytes.load(Ordering::Relaxed);
        frames_since += frames_now;
        let since = last_report.elapsed().as_secs_f64();
        if since >= 1.0 {
            fps = frames_since as f64 / since;
            frames_since = 0;
            last_report = Instant::now();
        }
        let mut p = progress.lock().expect("progress");
        p.stage = stage.to_string();
        p.done = done;
        p.total = total.max(1);
        p.elapsed_secs = started.elapsed().as_secs_f64();
        p.bytes = bytes;
        p.fps = fps;
        p.video = video.is_some();
        p.avg_fps = if video.is_some() && p.elapsed_secs > 0.0 {
            done as f64 / p.elapsed_secs
        } else {
            0.0
        };
        p.encoder = encoder.clone();
        drop(p);
        // The interface hears of it a few times a second, and when the
        // stage changes.
        let due = told
            .as_ref()
            .is_none_or(|(at, was)| was != stage || at.elapsed() >= TELL_EVERY);
        if due {
            told = Some((Instant::now(), stage.to_string()));
            repaint();
        }
    };
    let mut apool = AudioPool::new();
    apool.keep_bus_outputs(!mapped.is_empty());
    let file_channels = audio.map_or(0, |a| a.channels as usize);
    // The writer gone (it failed): its error says why.
    let gone = || "the file could not be written".to_string();
    let mut mix = |a0: i64, a1: i64, work: &SyncSender<Work>| -> Result<(), String> {
        if a1 <= a0 {
            return Ok(());
        }
        let buf = bv_mixer::mix(project, a0, (a1 - a0) as usize, &mut apool);
        if let Some(m) = &mut meter {
            m.push(&buf.channels);
        }
        let planar = if mapped.is_empty() {
            buf.channels
        } else {
            let busses = apool.take_bus_outputs();
            mapped_planar(&mapped, buf.channels, &busses, file_channels)
        };
        work.send(Work::Audio(planar)).map_err(|_| gone())
    };
    let mut sent: Result<(), String> = Ok(());

    if let Some(v) = video {
        let alpha = v.alpha && s.format.supports_alpha();
        let mut last_shown: Option<Instant> = None;
        let size = (v.width, v.height);
        let fit = s.options.fit();
        let upper = v.field_order == bv_core::FieldOrder::UpperFirst;
        // Frame `n` of the render drawn by `painter`, packed for the encoder
        // or as an image; interlaced, the field shown second half a frame
        // later.
        let draw = |n: u64, pool: &mut VideoPool, painter: &mut crate::accel::Painter| {
            let f = first_frame + n as i64;
            let (t, next) = (rate.frame_start(f), rate.frame_start(f + 1));
            let later = (v.field_order != bv_core::FieldOrder::Progressive)
                .then(|| t + Tick((next - t).0 / 2));
            match packed {
                Some(ten_bit) => {
                    let mut data = packed_at(project, t, size, fit, ten_bit, pool, painter);
                    if let Some(t2) = later {
                        let second = packed_at(project, t2, size, fit, ten_bit, pool, painter);
                        weave_packed(&mut data, &second, size, ten_bit, upper);
                    }
                    Work::Packed(data, n)
                }
                None => {
                    let mut img = frame_at(project, t, size, fit, alpha, pool, painter);
                    if let Some(t2) = later {
                        let second = frame_at(project, t2, size, fit, alpha, pool, painter);
                        weave_image(&mut img, &second, upper);
                    }
                    Work::Image(img, n)
                }
            }
        };
        // Frame `n` handed on, in order: some go to the preview (four a
        // second), the writer gets it, then its audio, sample-exact.
        let mut hand_on = |n: u64, job: Work| -> Result<(), String> {
            let f = first_frame + n as i64;
            let (t, next) = (rate.frame_start(f), rate.frame_start(f + 1));
            if shown.on.load(Ordering::Relaxed)
                && last_shown.is_none_or(|at| at.elapsed() >= TELL_EVERY)
            {
                let picture = match (&job, packed) {
                    (Work::Packed(data, _), Some(ten_bit)) => {
                        Some(preview_of_packed(data, size, ten_bit, t, n))
                    }
                    (Work::Image(img, _), _) => Some(preview_of(img, t, n)),
                    _ => None,
                };
                if let Some(picture) = picture {
                    *shown.frame.lock().expect("frame") = Some(Arc::new(picture));
                    last_shown = Some(Instant::now());
                }
            }
            work.send(job).map_err(|_| gone())?;
            if audio.is_some() {
                let a0 = sr.sample_at(t).max(first_sample);
                let a1 = if n + 1 == frames {
                    end_sample
                } else {
                    sr.sample_at(next).min(end_sample)
                };
                mix(a0, a1, &work)?;
            }
            update("Rendering", n + 1, 1);
            Ok(())
        };
        let painters = render_painters(
            painter.on_the_card(),
            crate::accel::has_plugins(project),
            smart.is_some(),
        );
        let mut cancelled = false;
        if painters <= 1 {
            let mut pool = VideoPool::new();
            for n in 0..frames {
                if cancel.load(Ordering::Relaxed) {
                    cancelled = true;
                    break;
                }
                let t = rate.frame_start(first_frame + n as i64);
                // Smart rendering: the media's own compressed frame, when the
                // project leaves it as it was.
                let job = match smart.as_mut().and_then(|sm| sm.frame(project, t)) {
                    Some(data) => {
                        progress.lock().expect("progress").passed += 1;
                        Work::Compressed(data, n)
                    }
                    None => draw(n, &mut pool, painter),
                };
                if let Err(e) = hand_on(n, job) {
                    sent = Err(e);
                    break;
                }
            }
        } else {
            // Painters of their own — plug-in instances, readers — each
            // draw every `painters`th frame on a thread of its own, a frame
            // or two ahead at most; the frames are handed on in order.
            let mine: Vec<crate::accel::Painter> = (0..painters)
                .map(|_| painter.another(crate::plugins::Purpose::File))
                .collect();
            let outcome = std::thread::scope(|scope| -> Result<bool, String> {
                let draw = &draw;
                let mut drawn = Vec::with_capacity(painters);
                for (i, mut own) in mine.into_iter().enumerate() {
                    let (tx, rx) = std::sync::mpsc::sync_channel::<Work>(2);
                    drawn.push(rx);
                    std::thread::Builder::new()
                        .name(format!("bv-render-{i}"))
                        // Plug-in effects run on it.
                        .stack_size(bv_ofx::PLUGIN_STACK)
                        .spawn_scoped(scope, move || {
                            let mut pool = VideoPool::new();
                            for n in (i as u64..frames).step_by(painters) {
                                if cancel.load(Ordering::Relaxed)
                                    || tx.send(draw(n, &mut pool, &mut own)).is_err()
                                {
                                    break;
                                }
                            }
                        })
                        .map_err(|e| e.to_string())?;
                }
                for n in 0..frames {
                    if cancel.load(Ordering::Relaxed) {
                        return Ok(true);
                    }
                    let job = drawn[(n % painters as u64) as usize]
                        .recv()
                        .map_err(|_| "a drawing thread stopped".to_string())?;
                    hand_on(n, job)?;
                }
                Ok(false)
            });
            match outcome {
                Ok(c) => cancelled = c,
                Err(e) => sent = Err(e),
            }
        }
        if cancelled {
            if let Ok(out) = finish_writer(work) {
                discard(out, s);
            }
            return cancelled_result();
        }
    } else if audio.is_some() {
        let mut pos = first_sample;
        let mut done = 0;
        while pos < end_sample {
            if cancel.load(Ordering::Relaxed) {
                if let Ok(out) = finish_writer(work) {
                    discard(out, s);
                }
                return cancelled_result();
            }
            let next = (pos + 4800).min(end_sample);
            if let Err(e) = mix(pos, next, &work) {
                sent = Err(e);
                break;
            }
            pos = next;
            done += 1;
            update("Mixing audio", done, 0);
        }
    }
    update("Finishing", total, 0);
    // The writer's own error says more than "could not be written".
    let out = finish_writer(work)?;
    sent?;
    let files = out.finish()?;
    if let Some(m) = &meter {
        write_loudness_log(&s.path, &m.report())?;
    }
    bytes.store(
        files
            .iter()
            .filter_map(|f| f.metadata().ok())
            .map(|m| m.len())
            .sum(),
        Ordering::Relaxed,
    );
    update("Done", total, 0);
    Ok(files)
}

/// A canceled render: no files.
/// For Swap video files on render: the file there and the one rendered
/// beside it (`None`: nothing there to swap, or an image sequence's
/// numbered files).
fn swap_target(s: &RenderSettings) -> Option<(PathBuf, PathBuf)> {
    if !s.options.swap_files || s.format.category() == Category::ImageSequence || !s.path.is_file()
    {
        return None;
    }
    let stem = s.path.file_stem()?.to_string_lossy().to_string();
    let ext = s
        .path
        .extension()
        .map_or(String::new(), |e| format!(".{}", e.to_string_lossy()));
    let temp = s
        .path
        .with_file_name(format!("{stem}.swap-{}{ext}", std::process::id()));
    Some((s.path.clone(), temp))
}

/// The render into `temp` finished: well, it takes `target`'s place;
/// else it goes and `target` stays.
fn swapped(
    result: Result<Vec<PathBuf>, String>,
    target: &Path,
    temp: &Path,
    cancel: &AtomicBool,
) -> Result<Vec<PathBuf>, String> {
    match result {
        Ok(files) if !cancel.load(Ordering::Relaxed) && temp.is_file() => {
            std::fs::rename(temp, target).map_err(|e| e.to_string())?;
            // The loudness log written beside it takes the file's name.
            let log = |p: &Path| {
                let stem = p
                    .file_stem()
                    .map_or(String::new(), |s| s.to_string_lossy().to_string());
                p.with_file_name(format!("{stem}_loud.txt"))
            };
            if log(temp).is_file() {
                let _ = std::fs::rename(log(temp), log(target));
            }
            Ok(files
                .into_iter()
                .map(|f| if f == temp { target.to_path_buf() } else { f })
                .collect())
        }
        other => {
            let _ = std::fs::remove_file(temp);
            other
        }
    }
}

fn cancelled_result() -> Result<Vec<PathBuf>, String> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swapped_files_take_the_old_one_s_place_only_when_rendered_well() {
        let dir = std::env::temp_dir().join(format!("bv-swap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("movie.wav");
        std::fs::write(&target, b"old").unwrap();
        let mut s = RenderSettings {
            format: FileFormat::Wav,
            path: target.clone(),
            range: TimeRange::new(Tick::ZERO, Tick::from_seconds(1)),
            video: None,
            audio: None,
            options: RenderOptions::default(),
            project_file: None,
        };
        assert!(swap_target(&s).is_none(), "off");
        s.options.swap_files = true;
        let (t, temp) = swap_target(&s).expect("a file to swap");
        assert_eq!(t, target);
        assert_ne!(temp, target);
        assert_eq!(temp.parent(), target.parent(), "beside it: one rename");
        // Rendered well: the new file in its place.
        std::fs::write(&temp, b"new").unwrap();
        let no = AtomicBool::new(false);
        let files = swapped(Ok(vec![temp.clone()]), &target, &temp, &no).unwrap();
        assert_eq!(files, std::slice::from_ref(&target));
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert!(!temp.exists());
        // Failed or canceled: the old file stays, the new one goes.
        std::fs::write(&temp, b"half").unwrap();
        assert!(swapped(Err("broken".into()), &target, &temp, &no).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert!(!temp.exists());
        std::fs::write(&temp, b"half").unwrap();
        let _ = swapped(
            Ok(vec![temp.clone()]),
            &target,
            &temp,
            &AtomicBool::new(true),
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert!(!temp.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A render tells the interface how far it is a few times a second, not
    /// at each frame (the interface drew itself as fast as the render went).
    #[test]
    fn a_render_tells_the_interface_a_few_times_a_second() {
        use std::sync::atomic::AtomicUsize;
        let dir = std::env::temp_dir().join(format!("bv-told-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut p = Project::default();
        p.settings.frame_rate = FrameRate::FPS_30;
        p.settings.width = 32;
        p.settings.height = 18;
        let v = p.tracks[0].id;
        let bars = p.add_media(bv_timeline::MediaInfo::generated(
            bv_timeline::Generator::ColorBars,
            &p.settings,
        ));
        let e = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
        let _ = p.trim_end(e, Tick::from_seconds(2));
        let told = Arc::new(AtomicUsize::new(0));
        let count = told.clone();
        let mut engine = crate::Engine::new(Arc::new(move || {
            count.fetch_add(1, Ordering::Relaxed);
        }));
        engine.set_project(&p);
        let before = told.load(Ordering::Relaxed);
        engine.start_render(RenderSettings {
            format: FileFormat::Png,
            path: dir.join("frame.png"),
            range: TimeRange::new(Tick::ZERO, Tick::from_seconds(2)),
            video: Some(VideoParams {
                width: 32,
                height: 18,
                frame_rate: FrameRate::FPS_30,
                ..VideoParams::default()
            }),
            audio: None,
            options: RenderOptions::default(),
            project_file: None,
        });
        let started = Instant::now();
        let progress = loop {
            let pr = engine.render_progress().unwrap();
            if pr.finished || started.elapsed().as_secs() > 60 {
                break pr;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert!(progress.error.is_none(), "{:?}", progress.error);
        assert_eq!(progress.files.len(), 60, "every frame rendered");
        let times = told.load(Ordering::Relaxed) - before;
        assert!(
            (1..30).contains(&times),
            "told {times} times for 60 frames in {:.2} s",
            progress.elapsed_secs
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Drawn by two painters at once, a render's frames are the ones one
    /// painter draws, in their order (the Timecode effect writes each
    /// frame's time on it).
    #[test]
    fn two_painters_draw_the_frames_one_does_in_order() {
        let mut p = Project::default();
        p.settings.frame_rate = FrameRate::FPS_30;
        p.settings.width = 96;
        p.settings.height = 54;
        let v = p.tracks[0].id;
        let bars = p.add_media(bv_timeline::MediaInfo::generated(
            bv_timeline::Generator::ColorBars,
            &p.settings,
        ));
        let e = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
        let _ = p.trim_end(e, Tick::from_seconds(1));
        p.clip_mut(e).unwrap().effects = vec![bv_timeline::Effect::plugin(bv_timeline::PluginFx {
            id: "org.bananav.Timecode".into(),
            version: [1, 0],
            name: "Timecode".into(),
            params: Default::default(),
        })];
        let mut engine = crate::Engine::new(Arc::new(|| {}));
        // The card, when there is one, ready before both renders.
        let started = Instant::now();
        while engine.gpu_status() == crate::GpuStatus::Starting && started.elapsed().as_secs() < 20
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        engine.set_project(&p);
        let mut frames = |painters: usize| -> Vec<Vec<u8>> {
            let dir =
                std::env::temp_dir().join(format!("bv-painters-{painters}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            FORCED_PAINTERS.store(painters, Ordering::Relaxed);
            engine.start_render(RenderSettings {
                format: FileFormat::Png,
                path: dir.join("frame.png"),
                range: TimeRange::new(Tick::ZERO, Tick::from_seconds(1)),
                video: Some(VideoParams {
                    width: 96,
                    height: 54,
                    frame_rate: FrameRate::FPS_30,
                    ..VideoParams::default()
                }),
                audio: None,
                options: RenderOptions::default(),
                project_file: None,
            });
            let progress = loop {
                let pr = engine.render_progress().unwrap();
                if pr.finished {
                    break pr;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            };
            FORCED_PAINTERS.store(0, Ordering::Relaxed);
            assert!(progress.error.is_none(), "{:?}", progress.error);
            let pictures: Vec<Vec<u8>> = progress
                .files
                .iter()
                .map(|f| std::fs::read(f).unwrap())
                .collect();
            let _ = std::fs::remove_dir_all(&dir);
            pictures
        };
        let one = frames(1);
        let two = frames(2);
        assert_eq!(one.len(), 30);
        assert_ne!(one[0], one[29], "each frame shows its time");
        assert!(one == two, "the same frames, in the same order");
    }

    #[test]
    fn sequences_number_their_files() {
        let p = Path::new("C:/out/Render.png");
        assert_eq!(sequence_path(p, 0), Path::new("C:/out/Render_000000.png"));
        assert_eq!(sequence_path(p, 41), Path::new("C:/out/Render_000041.png"));
    }

    #[test]
    fn other_shapes_are_letterboxed() {
        let mut p = Project::default();
        p.settings.width = 1920;
        p.settings.height = 1080;
        let red = p.add_media(bv_timeline::MediaInfo::generated(
            bv_timeline::Generator::SolidColor {
                rgba: [1.0, 0.0, 0.0, 1.0],
            },
            &p.settings.clone(),
        ));
        let v = p.tracks[0].id;
        p.insert_media(red, Tick::ZERO, Some(v), None).unwrap();
        let mut painter = crate::accel::Painter::new(
            crate::accel::Accel::new(std::sync::Arc::new(|| {})),
            crate::plugins::Purpose::File,
        );
        let img = frame_at(
            &p,
            Tick::ZERO,
            (64, 64),
            Fit::Letterbox,
            true,
            &mut VideoPool::new(),
            &mut painter,
        );
        assert_eq!((img.width, img.height), (64, 64));
        // Transparent bands above and below a 64×36 picture.
        assert_eq!(img.pixel(32, 2)[3], 0.0);
        assert_eq!(img.pixel(32, 32)[3], 1.0);
    }
}
