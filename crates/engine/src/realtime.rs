//! File › Real-Time Render (VEGAS's): the project played from the start of
//! the timeline and what plays — the program, without the metronome's
//! clicks or the Master Bus's monitoring switches — written to a WAV file
//! as it plays, at the project's sample rate, bit depth and channels, up to
//! the project's end. Stopping playback stops it: the file keeps what was
//! played. Without external sound coming in, it gives what File › Render
//! gives, taking as long as the project lasts.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;

use bv_core::Tick;
use bv_wav::{SampleFormat, WavWriter};

use crate::{AudioStatus, Engine, PlayOptions};

/// A real-time render going on.
pub(crate) struct RealTimeRender {
    path: PathBuf,
    /// Samples written so far, of the project's `total`.
    written: Arc<AtomicU64>,
    total: u64,
    rate: u32,
    writer: Option<JoinHandle<Result<(), String>>>,
}

/// How far a real-time render is, in seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct RealTimeProgress {
    pub path: PathBuf,
    pub done: f64,
    pub total: f64,
}

/// How a real-time render ended: its file, and whether it reached the
/// project's end (else playback was stopped first).
#[derive(Clone, Debug, PartialEq)]
pub struct RealTimeDone {
    pub path: PathBuf,
    pub whole: bool,
}

/// Writes what `blocks` brings — each block's first sample and its
/// channels — into `w`, from sample 0 up to `end`: done there, or once
/// the playback sends no more.
pub(crate) fn write_blocks<W: std::io::Write + std::io::Seek>(
    mut w: WavWriter<W>,
    blocks: Receiver<(i64, Vec<Vec<f32>>)>,
    end: i64,
    written: &AtomicU64,
) -> Result<W, String> {
    let mut next = 0i64;
    while let Ok((start, block)) = blocks.recv() {
        let len = block.first().map_or(0, Vec::len) as i64;
        // What follows what was written, up to the end.
        let from = (next - start).clamp(0, len);
        let to = (end - start).clamp(0, len);
        if to > from {
            let part: Vec<Vec<f32>> = block
                .iter()
                .map(|c| c[from as usize..to as usize].to_vec())
                .collect();
            w.write_planar(&part).map_err(|e| e.to_string())?;
            next = start + to;
            written.store(next as u64, Ordering::Relaxed);
        }
        if start + len >= end {
            break;
        }
    }
    w.finish().map_err(|e| e.to_string())
}

impl Engine {
    /// File › Real-Time Render: plays the project from the start of the
    /// timeline (no loop, at normal speed) and writes what plays to the
    /// WAV file at `path`. It needs an audio output: its clock paces it.
    pub fn start_real_time_render(&mut self, path: &Path) -> Result<(), String> {
        self.stop();
        self.real_time = None;
        let p = self.project().clone();
        let s = p.settings;
        let rate = s.sample_rate;
        // The first sample at or after the project's end.
        let mut end = rate.sample_at(p.duration());
        if rate.sample_start(end) < p.duration() {
            end += 1;
        }
        let format = SampleFormat::Int(match s.bit_depth {
            8 => 8,
            24 => 24,
            _ => 16,
        });
        let channels = s.channels.channel_count() as u16;
        let file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let w = WavWriter::new(
            BufWriter::with_capacity(1 << 20, file),
            rate.0,
            channels,
            format,
        )
        .map_err(|e| e.to_string())?;
        let (to, blocks) = std::sync::mpsc::channel();
        let written = Arc::new(AtomicU64::new(0));
        let writer = {
            let written = written.clone();
            std::thread::Builder::new()
                .name("bv-real-time-render".into())
                .spawn(move || write_blocks(w, blocks, end, &written).map(drop))
                .map_err(|e| e.to_string())?
        };
        crate::playback::set_capture(Some(to));
        self.play(Tick::ZERO, PlayOptions::default());
        if let AudioStatus::Unavailable(why) = self.audio_status() {
            let why = why.clone();
            crate::playback::set_capture(None);
            self.stop();
            let _ = writer.join();
            let _ = std::fs::remove_file(path);
            return Err(why);
        }
        self.real_time = Some(RealTimeRender {
            path: path.to_path_buf(),
            written,
            total: end.max(0) as u64,
            rate: rate.0,
            writer: Some(writer),
        });
        Ok(())
    }

    /// How far the real-time render is, while one goes on.
    pub fn real_time_progress(&self) -> Option<RealTimeProgress> {
        let r = self.real_time.as_ref()?;
        let rate = r.rate.max(1) as f64;
        Some(RealTimeProgress {
            path: r.path.clone(),
            done: r.written.load(Ordering::Relaxed) as f64 / rate,
            total: r.total as f64 / rate,
        })
    }

    /// The real-time render's end, once it came: the project's end
    /// written (playback is stopped then), or playback stopped before it.
    /// `None` while it goes on.
    pub fn finish_real_time_render(&mut self) -> Option<Result<RealTimeDone, String>> {
        let r = self.real_time.as_ref()?;
        let written_all = r.writer.as_ref().is_none_or(|w| w.is_finished());
        if !written_all && self.is_playing() {
            return None;
        }
        // No more blocks: the writer finishes with those sent.
        crate::playback::set_capture(None);
        if self.is_playing() {
            self.stop();
        }
        let mut r = self.real_time.take()?;
        let result = match r.writer.take() {
            Some(w) => w
                .join()
                .unwrap_or_else(|_| Err("the writing thread failed".into())),
            None => Ok(()),
        };
        let whole = r.written.load(Ordering::Relaxed) >= r.total;
        Some(result.map(|()| RealTimeDone {
            path: r.path,
            whole,
        }))
    }

    /// Whether a real-time render goes on (no recording meanwhile).
    pub fn is_real_time_rendering(&self) -> bool {
        self.real_time.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Blocks of `n` samples counting up from `start` (left) and down
    /// (right).
    fn block(start: i64, n: usize) -> (i64, Vec<Vec<f32>>) {
        let left = (0..n).map(|i| (start + i as i64) as f32 / 1000.0).collect();
        let right = (0..n)
            .map(|i| -(start + i as i64) as f32 / 1000.0)
            .collect();
        (start, vec![left, right])
    }

    #[test]
    fn what_plays_is_written_up_to_the_project_s_end() {
        let w =
            WavWriter::new(Cursor::new(Vec::new()), 48_000, 2, SampleFormat::Float(32)).unwrap();
        let (to, blocks) = std::sync::mpsc::channel();
        for k in 0..5 {
            to.send(block(k * 100, 100)).unwrap();
        }
        let written = AtomicU64::new(0);
        // The project ends at sample 250: half of the third block.
        let data = write_blocks(w, blocks, 250, &written).unwrap().into_inner();
        assert_eq!(written.load(Ordering::Relaxed), 250);
        let info = bv_wav::read_info(&mut Cursor::new(&data)).unwrap();
        assert_eq!(info.frames, 250, "nothing past the end");
        let mut r = bv_wav::WavReader::new(Cursor::new(data)).unwrap();
        let samples = r.read_planar(0, 250).unwrap();
        assert!((samples[0][249] - 0.249).abs() < 1e-6);
        assert!((samples[1][100] + 0.1).abs() < 1e-6);
    }

    #[test]
    fn stopped_playback_keeps_what_was_played() {
        let w = WavWriter::new(Cursor::new(Vec::new()), 48_000, 2, SampleFormat::Int(16)).unwrap();
        let (to, blocks) = std::sync::mpsc::channel();
        to.send(block(0, 100)).unwrap();
        to.send(block(100, 100)).unwrap();
        // Playback stops: no more blocks.
        drop(to);
        let written = AtomicU64::new(0);
        let data = write_blocks(w, blocks, 48_000, &written)
            .unwrap()
            .into_inner();
        assert_eq!(written.load(Ordering::Relaxed), 200);
        let info = bv_wav::read_info(&mut Cursor::new(&data)).unwrap();
        assert_eq!(info.frames, 200);
    }
}
