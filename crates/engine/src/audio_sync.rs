//! Lining events up by their sound (Tools › Multicamera › Sync Mixed Audio
//! to Align Events), in the background: each event's sound is read once
//! for its onsets' envelope, every other event's envelope is fitted to the
//! target's (`bv_mixer::align`), and a few seconds where the target is
//! busiest are read again from both for the fine step.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bv_core::{AudioBuffer, Tick};
use bv_media::AudioReader;
use bv_mixer::align::{self, FINE_RATE, HOPS_PER_SECOND, Matcher, PRE_EMPHASIS};
use bv_timeline::{ClipId, MediaInfo};

use crate::RepaintFn;

/// An event whose sound takes part: what it plays and where.
#[derive(Clone, Debug)]
pub(crate) struct SyncSource {
    pub clip: ClipId,
    pub info: MediaInfo,
    pub stream: u16,
    /// Its start on the timeline.
    pub start: Tick,
    /// Its media's time at its start.
    pub source_in: Tick,
    pub duration: Tick,
    /// Its playback rate.
    pub rate: f64,
}

impl SyncSource {
    fn hops(&self) -> usize {
        (self.duration.as_seconds_f64() * HOPS_PER_SECOND as f64).floor() as usize
    }

    /// The sample of its media (at `rate` a second) it plays at timeline
    /// time `t` (seconds), its start at `start`.
    fn media_sample(&self, start: f64, t: f64, rate: f64) -> f64 {
        (self.source_in.as_seconds_f64() + (t - start) * self.rate) * rate
    }
}

/// What became of an event.
#[derive(Clone, Debug, PartialEq)]
pub enum SyncOutcome {
    /// It moves by this much.
    Move(Tick),
    /// Its sound fits the target's nowhere.
    NoMatch,
    /// Its sound could not be read.
    Unreadable(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioSyncProgress {
    /// The event the others are lined up with.
    pub target: Option<ClipId>,
    pub done: u64,
    pub total: u64,
    pub finished: bool,
    /// Every other event, in order, and what became of it.
    pub outcomes: Vec<(ClipId, SyncOutcome)>,
    /// The target's sound could not be read.
    pub error: Option<String>,
}

/// Overlap the fit asks for at least, in hops (`Matcher::fit`).
const MIN_OVERLAP: usize = 5 * HOPS_PER_SECOND as usize;
/// Seconds of sound the fine step compares.
const FINE_SECONDS: f64 = 6.0;
/// How far around the coarse fit the fine step looks, in its samples
/// (2.5 of the envelope's hops).
const REACH: usize = 100;
/// The fine step is believed from this correlation of the two sounds on.
const MIN_FINE_SCORE: f64 = 0.2;
/// Media samples read at a time.
const BLOCK: usize = 1 << 16;

pub(crate) struct AudioSyncJob {
    progress: Arc<Mutex<AudioSyncProgress>>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AudioSyncJob {
    /// `sources[0]` is the target.
    pub(crate) fn start(sources: Vec<SyncSource>, repaint: RepaintFn) -> AudioSyncJob {
        let progress = Arc::new(Mutex::new(AudioSyncProgress {
            target: sources.first().map(|s| s.clip),
            total: sources.iter().map(|s| s.hops() as u64).sum::<u64>().max(1),
            ..AudioSyncProgress::default()
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let thread = {
            let progress = progress.clone();
            let cancel = cancel.clone();
            std::thread::Builder::new()
                .name("bv-audio-sync".into())
                .spawn(move || {
                    let mut last = Instant::now();
                    let mut tick = |hops: u64| {
                        progress.lock().expect("progress").done += hops;
                        if last.elapsed() > Duration::from_millis(100) {
                            last = Instant::now();
                            repaint();
                        }
                        !cancel.load(Ordering::Relaxed)
                    };
                    let result = run(&sources, &mut tick);
                    let mut p = progress.lock().expect("progress");
                    p.finished = true;
                    p.done = p.total;
                    match result {
                        Ok(o) => p.outcomes = o,
                        Err(e) => p.error = Some(e),
                    }
                    drop(p);
                    repaint();
                })
                .expect("could not start the audio synchronization's thread")
        };
        AudioSyncJob {
            progress,
            cancel,
            thread: Some(thread),
        }
    }

    pub(crate) fn progress(&self) -> AudioSyncProgress {
        self.progress.lock().expect("progress").clone()
    }

    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for AudioSyncJob {
    fn drop(&mut self) {
        self.cancel();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Reading stopped: the job was canceled.
const CANCELED: &str = "canceled";

/// Each event after the target and what became of it; `Err` when the
/// target's sound cannot be read (or the job was canceled).
fn run(
    sources: &[SyncSource],
    tick: &mut dyn FnMut(u64) -> bool,
) -> Result<Vec<(ClipId, SyncOutcome)>, String> {
    let Some((target, others)) = sources.split_first() else {
        return Ok(Vec::new());
    };
    let mut target_reader =
        bv_media::open_audio_stream(&target.info, target.stream).map_err(|e| e.to_string())?;
    let target_power = hop_powers(target, target_reader.as_mut(), tick)?;
    // The others' envelopes first, so that the target's spectrum is worked
    // out once for the longest of them.
    let mut read = Vec::new();
    for s in others {
        let r = bv_media::open_audio_stream(&s.info, s.stream)
            .map_err(|e| e.to_string())
            .and_then(|mut r| {
                let p = hop_powers(s, r.as_mut(), tick)?;
                Ok((r, align::onsets(&p)))
            });
        if matches!(&r, Err(e) if e == CANCELED) {
            return Err(CANCELED.into());
        }
        read.push(r);
    }
    let longest = read
        .iter()
        .filter_map(|r| r.as_ref().ok().map(|(_, e)| e.len()))
        .max()
        .unwrap_or(0);
    let matcher = Matcher::new(&align::onsets(&target_power), longest);
    let mut out = Vec::new();
    for (s, r) in others.iter().zip(read) {
        let outcome = match r {
            Err(e) => SyncOutcome::Unreadable(e),
            Ok((mut reader, envelope)) => match matcher.fit(&envelope, MIN_OVERLAP) {
                None => SyncOutcome::NoMatch,
                Some(fit) => {
                    let coarse =
                        (target.start - s.start).as_seconds_f64() + fit.at / HOPS_PER_SECOND as f64;
                    let fine = refine(
                        (target, target_reader.as_mut(), &target_power),
                        (s, reader.as_mut()),
                        coarse,
                    )
                    .unwrap_or(0.0);
                    SyncOutcome::Move(Tick::from_seconds_f64(coarse + fine))
                }
            },
        };
        out.push((s.clip, outcome));
    }
    Ok(out)
}

/// The mean power of each hop of what `s` plays — its channels mixed, a
/// first difference taking the rumble away —, hop `i` from its start plus
/// `i ⁄ HOPS_PER_SECOND` on the timeline.
fn hop_powers(
    s: &SyncSource,
    reader: &mut dyn AudioReader,
    tick: &mut dyn FnMut(u64) -> bool,
) -> Result<Vec<f32>, String> {
    let rate = reader.sample_rate().0 as f64;
    let hops = s.hops();
    let start = s.start.as_seconds_f64();
    // Hop `i`'s first sample of the media.
    let first = |i: usize| {
        s.media_sample(start, start + i as f64 / HOPS_PER_SECOND as f64, rate)
            .round() as i64
    };
    let end = first(hops);
    let mut out = Vec::with_capacity(hops);
    let (mut acc, mut count, mut prev) = (0.0f64, 0usize, 0.0f32);
    let mut pos = first(0);
    let mut next = first(1);
    while pos < end && out.len() < hops {
        let n = ((end - pos) as usize).min(BLOCK);
        let buf = reader.read(pos, n).map_err(|e| e.to_string())?;
        let before = out.len();
        for (i, m) in mono(&buf).into_iter().enumerate() {
            let y = m - PRE_EMPHASIS * prev;
            prev = m;
            acc += (y * y) as f64;
            count += 1;
            if pos + i as i64 + 1 >= next {
                out.push((acc / count.max(1) as f64) as f32);
                acc = 0.0;
                count = 0;
                next = first(out.len() + 1);
                if out.len() == hops {
                    break;
                }
            }
        }
        pos += n as i64;
        if !tick((out.len() - before) as u64) {
            return Err(CANCELED.into());
        }
    }
    out.resize(hops, 0.0);
    Ok(out)
}

/// A buffer's channels mixed into one.
pub(crate) fn mono(buf: &AudioBuffer) -> Vec<f32> {
    let n = buf.frames();
    let k = buf.channel_count().max(1) as f32;
    (0..n)
        .map(|i| buf.channels.iter().map(|c| c[i]).sum::<f32>() / k)
        .collect()
}

/// The fine step: how many seconds more the event must move, once moved by
/// `shift` seconds, so that its sound matches the target's where the
/// target is busiest over both (`None`: not believed).
fn refine(
    (target, target_reader, target_power): (&SyncSource, &mut dyn AudioReader, &[f32]),
    (s, reader): (&SyncSource, &mut dyn AudioReader),
    shift: f64,
) -> Option<f64> {
    let h = HOPS_PER_SECOND as f64;
    let t0 = target.start.as_seconds_f64();
    let t1 = t0 + target.duration.as_seconds_f64();
    let s0 = s.start.as_seconds_f64() + shift;
    let s1 = s0 + s.duration.as_seconds_f64();
    let (o0, o1) = (t0.max(s0), t1.min(s1));
    if o1 - o0 < 0.25 {
        return None;
    }
    let w = FINE_SECONDS.min(o1 - o0);
    // The window of the target's hops with the most power, within the
    // overlap.
    let wh = ((w * h) as usize).max(1);
    let lo = ((o0 - t0) * h).ceil().max(0.0) as usize;
    let hi = (((o1 - w - t0) * h).floor().max(0.0) as usize).max(lo);
    let mut sums = vec![0.0f64];
    for &p in target_power {
        sums.push(sums[sums.len() - 1] + p as f64);
    }
    let power = |a: usize| sums[(a + wh).min(sums.len() - 1)] - sums[a.min(sums.len() - 1)];
    let best = (lo..=hi).max_by(|&a, &b| power(a).total_cmp(&power(b)))?;
    let from = t0 + best as f64 / h;
    let n = (w * FINE_RATE as f64) as usize;
    let reach = REACH as f64 / FINE_RATE as f64;
    let x = fine_sound(target, t0, target_reader, from, n).ok()?;
    let y = fine_sound(s, s0, reader, from - reach, n + 2 * REACH).ok()?;
    let (samples, score) = align::refine(&x, &y, REACH)?;
    (score >= MIN_FINE_SCORE).then_some(samples / FINE_RATE as f64)
}

/// What `s` plays from timeline time `from` on (seconds), its start at
/// `start`: `count` samples at `FINE_RATE`, its channels mixed, smoothed so
/// that nothing above half that rate folds back, first-differenced.
fn fine_sound(
    s: &SyncSource,
    start: f64,
    reader: &mut dyn AudioReader,
    from: f64,
    count: usize,
) -> Result<Vec<f32>, String> {
    let rate = reader.sample_rate().0 as f64;
    let at = |t: f64| s.media_sample(start, t, rate);
    // Two boxes as wide as a fine sample's media samples: a triangle.
    let half = ((s.rate * rate / FINE_RATE as f64) / 2.0).ceil().max(1.0) as usize;
    let a = at(from).floor() as i64 - 2 * half as i64 - 2;
    let b = at(from + count as f64 / FINE_RATE as f64).ceil() as i64 + 2 * half as i64 + 2;
    let buf = reader
        .read(a, (b - a).max(1) as usize)
        .map_err(|e| e.to_string())?;
    let smooth = boxed(&boxed(&mono(&buf), half), half);
    let sample = |i: usize| smooth.get(i).copied().unwrap_or(0.0);
    let mut out = Vec::with_capacity(count);
    let mut prev = 0.0;
    for j in 0..count {
        let m = (at(from + j as f64 / FINE_RATE as f64) - a as f64).max(0.0);
        let (i, f) = (m.floor() as usize, (m - m.floor()) as f32);
        let v = sample(i) * (1.0 - f) + sample(i + 1) * f;
        out.push(v - PRE_EMPHASIS * prev);
        prev = v;
    }
    Ok(out)
}

/// The mean of each sample's `half` neighbours on each side and itself
/// (fewer at the ends).
fn boxed(x: &[f32], half: usize) -> Vec<f32> {
    let mut sums = Vec::with_capacity(x.len() + 1);
    sums.push(0.0f64);
    for &v in x {
        sums.push(sums[sums.len() - 1] + v as f64);
    }
    (0..x.len())
        .map(|i| {
            let (a, b) = (i.saturating_sub(half), (i + half + 1).min(x.len()));
            ((sums[b] - sums[a]) / (b - a) as f64) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_core::{ChannelLayout, SampleRate};

    /// A recording in memory: `data` from its sample 0, silence around it.
    struct Memory {
        rate: u32,
        data: Vec<f32>,
    }

    impl AudioReader for Memory {
        fn sample_rate(&self) -> SampleRate {
            SampleRate(self.rate)
        }
        fn channels(&self) -> usize {
            1
        }
        fn read(&mut self, start: i64, frames: usize) -> bv_media::Result<AudioBuffer> {
            let ch = (0..frames as i64)
                .map(|i| {
                    usize::try_from(start + i)
                        .ok()
                        .and_then(|k| self.data.get(k).copied())
                        .unwrap_or(0.0)
                })
                .collect();
            Ok(AudioBuffer {
                sample_rate: SampleRate(self.rate),
                layout: ChannelLayout::Mono,
                channels: vec![ch],
            })
        }
    }

    /// Bursts of noise like speech, `seconds` long.
    fn speech(seconds: f64, rate: u32, seed: u64) -> Vec<f32> {
        let mut state = seed | 1;
        let mut rnd = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut level = 0.0;
        let mut left = 0usize;
        (0..(seconds * rate as f64) as usize)
            .map(|_| {
                if left == 0 {
                    level = if rnd() < 0.35 { 0.0 } else { 0.1 + rnd() * 0.8 };
                    left = (rate as f64 * (0.05 + rnd() * 0.3)) as usize;
                }
                left -= 1;
                ((rnd() - 0.5) * 2.0 * level) as f32
            })
            .collect()
    }

    fn source(clip: u64, start: f64, seconds: f64) -> SyncSource {
        let p = bv_timeline::Project::default();
        SyncSource {
            clip: ClipId(clip),
            info: MediaInfo::generated(bv_timeline::Generator::ColorBars, &p.settings),
            stream: 0,
            start: Tick::from_seconds_f64(start),
            source_in: Tick::ZERO,
            duration: Tick::from_seconds_f64(seconds),
            rate: 1.0,
        }
    }

    /// The target at 1 s, a second microphone (at 44.1 kHz, quieter) that
    /// started 7.3 s later placed at 20 s: it must move to 8.3 s.
    #[test]
    fn a_second_recording_moves_to_where_it_was_heard() {
        let sound = speech(40.0, 48_000, 5);
        let resampled: Vec<f32> = (0..(25.0 * 44_100.0) as usize)
            .map(|i| {
                let t = 7.3 + i as f64 / 44_100.0;
                let x = t * 48_000.0;
                let (k, f) = (x.floor() as usize, (x - x.floor()) as f32);
                0.4 * (sound[k] * (1.0 - f) + sound[k + 1] * f)
            })
            .collect();
        let target = source(1, 1.0, 40.0);
        let other = source(2, 20.0, 25.0);
        let mut a = Memory {
            rate: 48_000,
            data: sound,
        };
        let mut b = Memory {
            rate: 44_100,
            data: resampled,
        };
        let mut tick = |_| true;
        let pa = hop_powers(&target, &mut a, &mut tick).unwrap();
        let pb = hop_powers(&other, &mut b, &mut tick).unwrap();
        let fit = Matcher::new(&align::onsets(&pa), pb.len())
            .fit(&align::onsets(&pb), MIN_OVERLAP)
            .expect("it fits");
        let coarse =
            (target.start - other.start).as_seconds_f64() + fit.at / HOPS_PER_SECOND as f64;
        assert!((coarse - -11.7).abs() < 0.003, "{coarse}");
        let fine = refine((&target, &mut a, &pa), (&other, &mut b), coarse).expect("refined");
        let shift = coarse + fine;
        assert!((shift - -11.7).abs() < 0.0002, "{shift}");
    }
}
