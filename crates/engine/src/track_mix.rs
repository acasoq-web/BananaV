//! Track buffering (Options › Preferences › Audio Device: "Enable track
//! buffering"): each audible track mixed on a thread of its own, the
//! playback's mixer summing them. A track's readers and effects stay on its
//! thread.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

use bv_core::AudioBuffer;
use bv_mixer::AudioPool;
use bv_timeline::{Project, TrackId};

/// A block to mix: the project, its first sample and length, and its
/// plug-in delay compensation along the routing.
type Job = (Arc<Project>, i64, usize, Arc<bv_mixer::Delays>);
/// A track's sound in a block (for its output and sends) and its peaks
/// (`None`: silent), and its VU/PPM readings.
type Done = (Option<(bv_mixer::Sums, Vec<f32>)>, VuLevels);
type VuLevels = Vec<(bv_mixer::meters::MeterOf, Vec<f32>)>;

struct Worker {
    jobs: Option<Sender<Job>>,
    done: Receiver<Done>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    fn start(track: TrackId) -> Worker {
        let (jobs, rx) = channel::<Job>();
        let (tx, done) = channel::<Done>();
        let thread = std::thread::Builder::new()
            .name("bv-track-mix".into())
            .spawn(move || {
                let mut pool = AudioPool::new();
                // Playing: what takes long is made on threads of its own.
                pool.set_live(true);
                while let Ok((project, start, frames, delays)) = rx.recv() {
                    let out =
                        bv_mixer::mix_track(&project, track, start, frames, &delays, &mut pool);
                    if tx.send((out, pool.take_vu_levels())).is_err() {
                        break;
                    }
                }
                // The readers close on the thread that opened them.
                pool.clear();
            })
            .expect("a thread for a track's mix");
        Worker {
            jobs: Some(jobs),
            done,
            thread: Some(thread),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.jobs = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The playback's tracks, each mixed on its own thread.
#[derive(Default)]
pub(crate) struct TrackThreads {
    workers: Vec<(TrackId, Worker)>,
    /// The master bus's state (the LFE filter).
    master: AudioPool,
    /// The tracks' VU/PPM readings of the last block.
    vu: VuLevels,
}

impl TrackThreads {
    /// What [`bv_mixer::mix`] gives for the block, and each track's peaks.
    pub(crate) fn mix(
        &mut self,
        project: &Arc<Project>,
        start: i64,
        frames: usize,
    ) -> (AudioBuffer, Vec<(TrackId, Vec<f32>)>) {
        let mut sums = bv_mixer::Sums::silent(project, frames);
        if project.master.muted && !project.plays_to_hardware() {
            return (sums.master, Vec::new());
        }
        let tracks: Vec<TrackId> = bv_mixer::mixed_tracks(project)
            .iter()
            .map(|t| t.id)
            .collect();
        self.workers.retain(|(id, _)| tracks.contains(id));
        for &id in &tracks {
            if !self.workers.iter().any(|(w, _)| *w == id) {
                self.workers.push((id, Worker::start(id)));
            }
        }
        // The busses' and chains' latencies, from where they mix.
        let delays = Arc::new(bv_mixer::delays(project, &mut self.master));
        // All of them at work at once, then summed in the tracks' order.
        let mut asked = Vec::new();
        for &id in &tracks {
            if let Some((_, w)) = self.workers.iter().find(|(w, _)| *w == id)
                && let Some(jobs) = &w.jobs
                && jobs
                    .send((project.clone(), start, frames, delays.clone()))
                    .is_ok()
            {
                asked.push(id);
            }
        }
        let mut peaks = Vec::new();
        self.vu.clear();
        for id in asked {
            let Some((_, w)) = self.workers.iter().find(|(w, _)| *w == id) else {
                continue;
            };
            if let Ok((done, vu)) = w.done.recv() {
                self.vu.extend(vu);
                if let Some((own, p)) = done {
                    sums.add(&own);
                    peaks.push((id, p));
                }
            }
        }
        let out = bv_mixer::finish_mix(project, start, &mut self.master, sums);
        (out, peaks)
    }

    /// The busses' and chains' peaks in the last block mixed.
    pub(crate) fn take_strip_peaks(&mut self) -> Vec<(bv_timeline::Strip, Vec<f32>)> {
        self.master.take_strip_peaks()
    }

    /// What the busses on hardware outputs played in the last block.
    pub(crate) fn take_hardware(&mut self) -> Vec<(u8, AudioBuffer)> {
        self.master.take_hardware()
    }

    /// The VU/PPM readings of the last block mixed: the tracks', the
    /// busses' and chains', the Master's.
    pub(crate) fn take_vu_levels(&mut self) -> VuLevels {
        let mut all = std::mem::take(&mut self.vu);
        all.extend(self.master.take_vu_levels());
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_core::Tick;
    use bv_timeline::{Generator, MediaInfo, ProjectSettings, TrackKind};

    #[test]
    fn tracks_on_threads_sound_as_the_whole_mix() {
        let mut p = Project::new(ProjectSettings::default());
        let a = p.tracks[1].id;
        let b = p.add_track(TrackKind::Audio);
        for (track, hz) in [(a, 330.0), (b, 550.0)] {
            let m = p.add_media(MediaInfo::generated(
                Generator::TestTone {
                    frequency_hz: hz,
                    level_db: -9.0,
                },
                &p.settings,
            ));
            p.insert_media(m, Tick::ZERO, None, Some(track)).unwrap();
        }
        let p = Arc::new(p);
        let mut threads = TrackThreads::default();
        let mut pool = AudioPool::new();
        for k in 0..4 {
            let start = k * 1024;
            let (out, peaks) = threads.mix(&p, start, 1024);
            let whole = bv_mixer::mix(&p, start, 1024, &mut pool);
            assert_eq!(out.channels, whole.channels, "block {k}");
            assert_eq!(peaks.len(), 2);
        }
    }
}
