//! Loudness scan (Tools › Generate Loudness Log): mixes the project in the
//! background and measures it with the BS.1770 / EBU R 128 meter.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use bv_core::{Tick, TimeRange};
use bv_mixer::AudioPool;
use bv_mixer::loudness::{LoudnessMeter, LoudnessReport};
use bv_timeline::Project;

use crate::RepaintFn;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoudnessProgress {
    /// Measured range of the timeline.
    pub range: Option<TimeRange>,
    pub done: u64,
    pub total: u64,
    pub finished: bool,
    pub report: Option<LoudnessReport>,
    /// Short-term loudness every second, for the log.
    pub timeline: Vec<(Tick, f64)>,
}

pub(crate) struct LoudnessJob {
    progress: Arc<Mutex<LoudnessProgress>>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl LoudnessJob {
    pub(crate) fn start(
        project: Arc<Project>,
        range: TimeRange,
        repaint: RepaintFn,
    ) -> LoudnessJob {
        let rate = project.settings.sample_rate;
        let first = rate.sample_at(range.start);
        let last = rate.sample_at(range.end());
        let progress = Arc::new(Mutex::new(LoudnessProgress {
            range: Some(range),
            total: (last - first).max(1) as u64,
            ..LoudnessProgress::default()
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let thread = {
            let progress = progress.clone();
            let cancel = cancel.clone();
            std::thread::Builder::new()
                .name("bv-sonoridade".into())
                .spawn(move || {
                    let channels = project.settings.channels.channel_count();
                    let mut meter = LoudnessMeter::new(rate.0, channels);
                    let mut pool = AudioPool::new();
                    let block = (rate.0 as i64).max(1);
                    let mut pos = first;
                    let mut timeline = Vec::new();
                    while pos < last && !cancel.load(Ordering::Relaxed) {
                        let n = block.min(last - pos) as usize;
                        let buf = bv_mixer::mix(&project, pos, n, &mut pool);
                        meter.push(&buf.channels);
                        pos += n as i64;
                        timeline.push((rate.sample_start(pos), meter.short_term()));
                        let mut p = progress.lock().expect("progress");
                        p.done = (pos - first) as u64;
                        drop(p);
                        repaint();
                    }
                    let mut p = progress.lock().expect("progress");
                    p.finished = true;
                    p.report = Some(meter.report());
                    p.timeline = timeline;
                    drop(p);
                    repaint();
                })
                .expect("could not start the loudness thread")
        };
        LoudnessJob {
            progress,
            cancel,
            thread: Some(thread),
        }
    }

    pub(crate) fn progress(&self) -> LoudnessProgress {
        self.progress.lock().expect("progress").clone()
    }
}

impl Drop for LoudnessJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
