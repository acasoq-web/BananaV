//! Tools › Audio › Apply Non-Real-Time Event FX, in the background: the
//! sound an event plays — its media's samples between the media times of
//! its start and end — through an audio effects chain
//! (`bv_mixer::dsp::Chain`) into a new WAV file, which the interface adds
//! to the event as its new take.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bv_core::Tick;
use bv_mixer::dsp::Chain;
use bv_timeline::{ClipId, Effect, MediaInfo};

use crate::RepaintFn;

/// The samples of the new file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NrtFormat {
    #[default]
    Pcm16,
    Pcm24,
    Float32,
}

impl NrtFormat {
    pub const ALL: [NrtFormat; 3] = [NrtFormat::Pcm16, NrtFormat::Pcm24, NrtFormat::Float32];

    /// Its name (in English, as the interface translates it).
    pub fn name(self) -> &'static str {
        match self {
            NrtFormat::Pcm16 => "16-bit PCM",
            NrtFormat::Pcm24 => "24-bit PCM",
            NrtFormat::Float32 => "32-bit floating point",
        }
    }

    fn sample_format(self) -> bv_wav::SampleFormat {
        match self {
            NrtFormat::Pcm16 => bv_wav::SampleFormat::Int(16),
            NrtFormat::Pcm24 => bv_wav::SampleFormat::Int(24),
            NrtFormat::Float32 => bv_wav::SampleFormat::Float(32),
        }
    }
}

/// How far the rendering went.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NrtProgress {
    pub clip: Option<ClipId>,
    pub path: PathBuf,
    /// Seconds of sound rendered, of `total`.
    pub done: f64,
    pub total: f64,
    pub finished: bool,
    /// It failed (or was canceled): why; the file is gone.
    pub error: Option<String>,
}

/// What to render: the event, its media and stream, the media times of its
/// start and end, the chain and the file.
#[derive(Clone, Debug)]
pub(crate) struct NrtSource {
    pub clip: ClipId,
    pub info: MediaInfo,
    pub stream: u16,
    pub from: Tick,
    pub to: Tick,
    pub effects: Vec<Effect>,
    pub path: PathBuf,
    pub format: NrtFormat,
}

/// Media samples read at a time.
const BLOCK: usize = 1 << 15;
/// Rendering stopped: the job was canceled.
const CANCELED: &str = "canceled";

pub(crate) struct NrtJob {
    progress: Arc<Mutex<NrtProgress>>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl NrtJob {
    pub(crate) fn start(src: NrtSource, repaint: RepaintFn) -> NrtJob {
        let progress = Arc::new(Mutex::new(NrtProgress {
            clip: Some(src.clip),
            path: src.path.clone(),
            total: (src.to - src.from).as_seconds_f64().max(1e-3),
            ..NrtProgress::default()
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let thread = {
            let progress = progress.clone();
            let cancel = cancel.clone();
            std::thread::Builder::new()
                .name("bv-nonrealtime-fx".into())
                .spawn(move || {
                    let mut last = Instant::now();
                    let mut tick = |seconds: f64| {
                        progress.lock().expect("progress").done += seconds;
                        if last.elapsed() > Duration::from_millis(100) {
                            last = Instant::now();
                            repaint();
                        }
                        !cancel.load(Ordering::Relaxed)
                    };
                    let result = run(&src, &mut tick);
                    if result.is_err() {
                        let _ = std::fs::remove_file(&src.path);
                    }
                    let mut p = progress.lock().expect("progress");
                    p.finished = true;
                    p.done = p.total;
                    p.error = result.err();
                    drop(p);
                    repaint();
                })
                .expect("could not start the non-real-time effects' thread")
        };
        NrtJob {
            progress,
            cancel,
            thread: Some(thread),
        }
    }

    pub(crate) fn progress(&self) -> NrtProgress {
        self.progress.lock().expect("progress").clone()
    }

    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for NrtJob {
    fn drop(&mut self) {
        self.cancel();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Renders `src`'s sound through its chain into its file.
fn run(src: &NrtSource, tick: &mut dyn FnMut(f64) -> bool) -> Result<(), String> {
    let sound = Sound {
        info: &src.info,
        stream: src.stream,
        from: src.from,
        to: src.to,
        effects: &src.effects,
        path: &src.path,
        format: src.format,
    };
    render(&sound, tick)
}

/// A media's sound of `info` between media times `from` and `to`, as it
/// is (24-bit PCM), into the WAV file `path` (Save As's trimmed copies of
/// the media); `tick` hears the seconds written and stops it by answering
/// `false`. When it fails or is stopped the file goes.
pub fn trim_sound(
    info: &MediaInfo,
    from: Tick,
    to: Tick,
    path: &Path,
    tick: &mut dyn FnMut(f64) -> bool,
) -> Result<(), String> {
    let sound = Sound {
        info,
        stream: 0,
        from,
        to,
        effects: &[],
        path,
        format: NrtFormat::Pcm24,
    };
    let result = render(&sound, tick);
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}

/// What `render` writes: a media's stream between two media times,
/// through effects, into a file of a format.
struct Sound<'a> {
    info: &'a MediaInfo,
    stream: u16,
    from: Tick,
    to: Tick,
    effects: &'a [Effect],
    path: &'a Path,
    format: NrtFormat,
}

fn render(src: &Sound<'_>, tick: &mut dyn FnMut(f64) -> bool) -> Result<(), String> {
    let mut reader =
        bv_media::open_audio_stream(src.info, src.stream).map_err(|e| e.to_string())?;
    let rate = reader.sample_rate().0;
    let channels = reader.channels().max(1);
    let at = |t: Tick| (t.as_seconds_f64() * f64::from(rate)).round() as i64;
    let (a, b) = (at(src.from), at(src.to));
    let mut chain = Chain::default();
    let effects: Vec<&Effect> = src.effects.iter().collect();
    chain.sync(&effects, rate, channels, false);
    let file = std::fs::File::create(src.path).map_err(|e| e.to_string())?;
    let mut w = bv_wav::WavWriter::new(
        std::io::BufWriter::new(file),
        rate,
        channels as u16,
        src.format.sample_format(),
    )
    .map_err(|e| e.to_string())?;
    let mut pos = a;
    while pos < b {
        let n = ((b - pos) as usize).min(BLOCK);
        let buf = reader.read(pos, n).map_err(|e| e.to_string())?;
        let mut planes: Vec<Vec<f32>> = (0..channels)
            .map(|c| {
                let mut v = buf.channels.get(c).cloned().unwrap_or_default();
                v.resize(n, 0.0);
                v
            })
            .collect();
        chain.process(&mut planes);
        w.write_planar(&planes).map_err(|e| e.to_string())?;
        pos += n as i64;
        if !tick(n as f64 / f64::from(rate)) {
            return Err(CANCELED.into());
        }
    }
    w.finish().map_err(|e| e.to_string())?;
    Ok(())
}
