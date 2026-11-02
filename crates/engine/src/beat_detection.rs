//! Beat Detection and Tempo Detection in the background: a media's sound
//! read once, its channels mixed, for the flux of its low end
//! (`bv_mixer::beats`), then its beats and its tempo.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bv_mixer::beats::{Beat, LowFlux};
use bv_timeline::{MediaId, MediaInfo};

use crate::RepaintFn;

/// How far the reading of a media's sound went, and what it found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BeatProgress {
    pub media: Option<MediaId>,
    /// Seconds of sound read, of `total`.
    pub done: f64,
    pub total: f64,
    pub finished: bool,
    /// Its beats: every peak of the flux, with how far it stands out (a
    /// sensitivity picks among them: [`crate::beats_kept`]).
    pub beats: Vec<Beat>,
    /// Its tempo (beats a minute), when it has a steady one.
    pub tempo: Option<f64>,
    /// Its sound could not be read.
    pub error: Option<String>,
}

/// Media samples read at a time.
const BLOCK: usize = 1 << 16;
/// Reading stopped: the job was canceled.
const CANCELED: &str = "canceled";

pub(crate) struct BeatJob {
    progress: Arc<Mutex<BeatProgress>>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl BeatJob {
    /// Reads stream `stream` of `info`'s sound.
    pub(crate) fn start(info: MediaInfo, stream: u16, repaint: RepaintFn) -> BeatJob {
        let progress = Arc::new(Mutex::new(BeatProgress {
            media: Some(info.id),
            total: info.duration.as_seconds_f64().max(1e-3),
            ..BeatProgress::default()
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let thread = {
            let progress = progress.clone();
            let cancel = cancel.clone();
            std::thread::Builder::new()
                .name("bv-beats".into())
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
                    let result = run(&info, stream, &mut tick);
                    let mut p = progress.lock().expect("progress");
                    p.finished = true;
                    p.done = p.total;
                    match result {
                        Ok((beats, tempo)) => {
                            p.beats = beats;
                            p.tempo = tempo;
                        }
                        Err(e) => p.error = Some(e),
                    }
                    drop(p);
                    repaint();
                })
                .expect("could not start the beat detection's thread")
        };
        BeatJob {
            progress,
            cancel,
            thread: Some(thread),
        }
    }

    pub(crate) fn progress(&self) -> BeatProgress {
        self.progress.lock().expect("progress").clone()
    }

    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for BeatJob {
    fn drop(&mut self) {
        self.cancel();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The beats and the tempo of stream `stream` of `info`'s sound.
fn run(
    info: &MediaInfo,
    stream: u16,
    tick: &mut dyn FnMut(f64) -> bool,
) -> Result<(Vec<Beat>, Option<f64>), String> {
    let mut reader = bv_media::open_audio_stream(info, stream).map_err(|e| e.to_string())?;
    let rate = reader.sample_rate().0;
    let total = (info.duration.as_seconds_f64() * f64::from(rate)).ceil() as i64;
    let mut flux = LowFlux::new(rate);
    let mut pos = 0i64;
    while pos < total {
        let n = ((total - pos) as usize).min(BLOCK);
        let buf = reader.read(pos, n).map_err(|e| e.to_string())?;
        flux.push(&crate::audio_sync::mono(&buf));
        pos += n as i64;
        if !tick(n as f64 / f64::from(rate)) {
            return Err(CANCELED.into());
        }
    }
    let found = flux.finish();
    Ok((found.beats(), found.tempo()))
}
