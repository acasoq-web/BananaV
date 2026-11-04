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
    /// The project's tempo, for effects with tempo sync.
    pub tempo: bv_timeline::Tempo,
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
        tempo: src.tempo,
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
        tempo: bv_timeline::Tempo::default(),
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
    tempo: bv_timeline::Tempo,
}

fn render(src: &Sound<'_>, tick: &mut dyn FnMut(f64) -> bool) -> Result<(), String> {
    let mut reader =
        bv_media::open_audio_stream(src.info, src.stream).map_err(|e| e.to_string())?;
    let rate = reader.sample_rate().0;
    let channels = reader.channels().max(1);
    let at = |t: Tick| (t.as_seconds_f64() * f64::from(rate)).round() as i64;
    let (a, b) = (at(src.from), at(src.to));
    let file = std::fs::File::create(src.path).map_err(|e| e.to_string())?;
    let mut w = bv_wav::WavWriter::new(
        std::io::BufWriter::new(file),
        rate,
        channels as u16,
        src.format.sample_format(),
    )
    .map_err(|e| e.to_string())?;
    // Effects that change the sound's length work on all of it at once:
    // then the sound is gathered first, and the chain runs a stretch at a
    // time between them.
    if src.effects.iter().any(|e| e.kind.works_offline()) {
        let mut planes: Vec<Vec<f32>> = vec![Vec::new(); channels];
        let mut pos = a;
        while pos < b {
            let n = ((b - pos) as usize).min(BLOCK);
            let buf = reader.read(pos, n).map_err(|e| e.to_string())?;
            for (c, plane) in planes.iter_mut().enumerate() {
                let mut v = buf.channels.get(c).cloned().unwrap_or_default();
                v.resize(n, 0.0);
                plane.extend_from_slice(&v);
            }
            pos += n as i64;
            if !tick(n as f64 / f64::from(rate) / 2.0) {
                return Err(CANCELED.into());
            }
        }
        let mut stretch: Vec<&Effect> = Vec::new();
        let run = |stretch: &mut Vec<&Effect>, planes: &mut Vec<Vec<f32>>| {
            if stretch.is_empty() {
                return;
            }
            let mut chain = Chain::default();
            chain.set_tempo(src.tempo);
            chain.sync(stretch, rate, channels, false);
            // What the chain holds back comes out, its lateness taken back.
            let late = chain.latency();
            let len = planes.first().map_or(0, Vec::len);
            for p in planes.iter_mut() {
                p.resize(len + late, 0.0);
            }
            let mut at = 0;
            while at < len + late {
                let end = (at + BLOCK).min(len + late);
                let mut part: Vec<Vec<f32>> = planes.iter().map(|p| p[at..end].to_vec()).collect();
                chain.process(&mut part);
                for (p, q) in planes.iter_mut().zip(part) {
                    p[at..end].copy_from_slice(&q);
                }
                at = end;
            }
            for p in planes.iter_mut() {
                p.drain(..late);
            }
            stretch.clear();
        };
        for e in src
            .effects
            .iter()
            .filter(|e| e.kind.is_audio() && e.enabled)
        {
            if e.kind.works_offline() {
                run(&mut stretch, &mut planes);
                if let Some(out) = bv_mixer::render_offline(e, rate, std::mem::take(&mut planes)) {
                    planes = out;
                }
            } else {
                stretch.push(e);
            }
        }
        run(&mut stretch, &mut planes);
        w.write_planar(&planes).map_err(|e| e.to_string())?;
        if !tick((b - a) as f64 / f64::from(rate) / 2.0) {
            return Err(CANCELED.into());
        }
        w.finish().map_err(|e| e.to_string())?;
        return Ok(());
    }
    let mut chain = Chain::default();
    let effects: Vec<&Effect> = src.effects.iter().collect();
    chain.set_tempo(src.tempo);
    chain.sync(&effects, rate, channels, false);
    // Effects that answer late (FFT filters): the sound goes on through
    // them that much past its end, and as much is dropped from the start.
    let mut skip = chain.latency();
    let end = b + skip as i64;
    let mut pos = a;
    while pos < end {
        let n = ((end - pos) as usize).min(BLOCK);
        let mut planes: Vec<Vec<f32>> = if pos < b {
            let buf = reader
                .read(pos, n.min((b - pos) as usize))
                .map_err(|e| e.to_string())?;
            (0..channels)
                .map(|c| {
                    let mut v = buf.channels.get(c).cloned().unwrap_or_default();
                    v.resize(n, 0.0);
                    v
                })
                .collect()
        } else {
            vec![vec![0.0; n]; channels]
        };
        chain.process(&mut planes);
        let drop = skip.min(n);
        skip -= drop;
        if drop < n {
            let kept: Vec<Vec<f32>> = planes.iter().map(|p| p[drop..].to_vec()).collect();
            w.write_planar(&kept).map_err(|e| e.to_string())?;
        }
        pos += n as i64;
        if !tick(n as f64 / f64::from(rate)) {
            return Err(CANCELED.into());
        }
    }
    w.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::{EffectKind, Generator, ProjectSettings};

    /// Through effects that answer late (the Modern Equalizer, oversampled
    /// with its FIR filters), the sound written stays where it was and
    /// whole: their lateness is taken back, with and without an effect
    /// that changes the sound's length in the chain.
    #[test]
    fn late_effects_leave_the_sound_in_place() {
        let settings = ProjectSettings::default();
        let info = MediaInfo::generated(
            Generator::TestTone {
                frequency_hz: 1000.0,
                level_db: -6.0,
            },
            &settings,
        );
        let from = Tick::ZERO;
        let to = Tick::from_seconds_f64(0.25);
        let dir = std::env::temp_dir().join(format!("bv_nrt_late_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let read = |path: &Path| {
            let mut r = bv_wav::WavReader::new(std::fs::File::open(path).unwrap()).unwrap();
            r.read_planar(0, 12_000).unwrap()
        };
        let plain = dir.join("plain.wav");
        fn sound<'a>(
            info: &'a MediaInfo,
            span: (Tick, Tick),
            effects: &'a [Effect],
            path: &'a Path,
        ) -> Sound<'a> {
            Sound {
                info,
                stream: 0,
                from: span.0,
                to: span.1,
                effects,
                path,
                format: NrtFormat::Float32,
                tempo: bv_timeline::Tempo::default(),
            }
        }
        render(&sound(&info, (from, to), &[], &plain), &mut |_| true).unwrap();
        let reference = read(&plain);
        let eq = Effect::new(EffectKind::ModernEq);
        let mut gapper = Effect::new(EffectKind::GapperSnipper);
        // As long as it was: its gaps off (the effect's own default inserts).
        gapper.enabled = false;
        for (name, chain) in [("eq", vec![eq.clone()]), ("eq_gather", vec![eq, gapper])] {
            let path = dir.join(format!("{name}.wav"));
            render(&sound(&info, (from, to), &chain, &path), &mut |_| true).unwrap();
            let got = read(&path);
            assert_eq!(got[0].len(), reference[0].len(), "{name}");
            let worst = got[0]
                .iter()
                .zip(&reference[0])
                .skip(200)
                .take(11_000)
                .map(|(a, b)| (a - b).abs())
                .fold(0f32, f32::max);
            assert!(worst < 1e-3, "{name}: {worst}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
