//! Events whose effects change the sound's length — Time Stretch, Pitch
//! Shift, Gapper/Snipper, Vibrato… — play, as VEGAS's do, their sound
//! made through the whole chain: the event's sound is gathered and runs
//! through its media's and its own effects all at once, as Non-Real-Time
//! Event FX makes it, and the event plays that from its start, keeping its
//! length (what the effects made past its end is cut, what they did not
//! make is silence). While it plays the sound is made on a thread of its
//! own, the newest one made playing meanwhile; elsewhere (a render) at
//! once.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bv_core::{SampleRate, Tick};
use bv_timeline::{AudioQuality, Clip, ClipId, Effect, MediaInfo, Tempo};

use crate::{apply_switches, dsp, first_sample_at_or_after, loop_segments, read_clip, stretch};

/// How much sound is read, and goes through the chain, at a time.
const BLOCK: usize = 4096;

/// Blocks an event may go unasked before its sound is let go.
const KEPT_FOR: u64 = 2_000;

/// A sound being made: where it lands, and the flag that stops it.
type Job = (Arc<Mutex<Option<Arc<Sound>>>>, Arc<AtomicBool>);

/// An event's sound through its chain: a buffer a channel, from its start.
pub(crate) type Sound = Vec<Vec<f32>>;

/// What an event's sound is made of: the media, the event (its place on
/// the timeline, its fades and its level left out: they come after), the
/// effects, the rate, the resampling and the tempo.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    media: MediaInfo,
    clip: Clip,
    effects: Vec<Effect>,
    rate: SampleRate,
    quality: AudioQuality,
    tempo: Tempo,
}

#[derive(Debug, Default)]
struct Made {
    key: Option<Key>,
    /// The newest sound finished (it plays while a newer one is made).
    ready: Option<Arc<Sound>>,
    job: Option<Job>,
    /// The block it was last asked for in.
    seen: u64,
}

/// The events' sounds made through their chains.
#[derive(Debug, Default)]
pub(crate) struct Prerenders {
    made: HashMap<ClipId, Made>,
    /// Made on threads of their own (while playing), or at once.
    pub(crate) live: bool,
}

impl Prerenders {
    /// `clip`'s sound through `effects` (its media's and its own), made
    /// anew when what it is made of changed; `None` while none was made.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sound(
        &mut self,
        media: &MediaInfo,
        clip: &Clip,
        effects: &[&Effect],
        rate: SampleRate,
        quality: AudioQuality,
        tempo: Tempo,
        block: u64,
    ) -> Option<Arc<Sound>> {
        let mut placeless = clip.clone();
        placeless.start = Tick::ZERO;
        placeless.fade_in = Tick::ZERO;
        placeless.fade_out = Tick::ZERO;
        placeless.level = 1.0;
        let mut media = media.clone();
        media.markers.clear();
        let key = Key {
            media,
            clip: placeless,
            effects: effects.iter().map(|e| (*e).clone()).collect(),
            rate,
            quality,
            tempo,
        };
        let made = self.made.entry(clip.id).or_default();
        made.seen = block;
        if made.key.as_ref() != Some(&key) {
            if let Some((_, stop)) = made.job.take() {
                stop.store(true, Ordering::Relaxed);
            }
            made.key = Some(key.clone());
            let slot = Arc::new(Mutex::new(None));
            let stop = Arc::new(AtomicBool::new(false));
            let (to, flag) = (slot.clone(), stop.clone());
            let work = move || {
                if let Some(sound) = render(&key, &flag)
                    && let Ok(mut s) = to.lock()
                {
                    *s = Some(Arc::new(sound));
                }
            };
            if self.live {
                let started = std::thread::Builder::new()
                    .name("bv-event-fx".into())
                    .spawn(work);
                if started.is_err() {
                    made.key = None;
                    return made.ready.clone();
                }
            } else {
                work();
            }
            made.job = Some((slot, stop));
        }
        let done = made
            .job
            .as_ref()
            .and_then(|(slot, _)| slot.try_lock().ok().and_then(|s| s.clone()));
        if let Some(sound) = done {
            made.ready = Some(sound);
            made.job = None;
        }
        made.ready.clone()
    }

    /// Lets go of the sounds of events not asked for in a while.
    pub(crate) fn forget_old(&mut self, block: u64) {
        self.made.retain(|_, m| {
            let keep = block.saturating_sub(m.seen) < KEPT_FOR;
            if !keep && let Some((_, stop)) = &m.job {
                stop.store(true, Ordering::Relaxed);
            }
            keep
        });
    }

    pub(crate) fn clear(&mut self) {
        self.forget_old(u64::MAX);
    }
}

/// The sound `key` makes: the event read as it plays, its channel
/// switches, then its effects — those between the length-changing ones a
/// stretch at a time, their lateness taken back — each on all of it.
/// `None` when stopped or unreadable.
fn render(key: &Key, stop: &AtomicBool) -> Option<Sound> {
    let (media, clip, rate) = (&key.media, &key.clip, key.rate);
    let mut reader = bv_media::open_audio_stream(media, clip.audio_stream).ok()?;
    let channels = usize::from(media.audio?.channels).max(1);
    let s0 = first_sample_at_or_after(rate, clip.start);
    let s1 = first_sample_at_or_after(rate, clip.end());
    let keep_pitch = ((clip.preserve_pitch && (clip.rate - 1.0).abs() > 1e-9) || clip.pitch != 0.0)
        && clip.reverse.is_none()
        && clip.velocity().is_none();
    let mut shifts = stretch::Shifts::default();
    let mut planes: Sound = vec![Vec::with_capacity((s1 - s0).max(0) as usize); channels];
    let mut pos = s0;
    while pos < s1 {
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        let n = ((s1 - pos) as usize).min(BLOCK);
        let mut part = vec![vec![0.0f32; n]; channels];
        for (a, b, shift) in loop_segments(clip, rate, pos, pos + n as i64, media.duration) {
            let len = (b - a) as usize;
            let got = if keep_pitch {
                stretch::read(reader.as_mut(), &mut shifts, clip, rate, a, len, shift)
            } else {
                read_clip(reader.as_mut(), clip, rate, a, len, key.quality, shift)
            };
            let Ok(got) = got else {
                continue;
            };
            for (d, g) in part.iter_mut().zip(&got) {
                for (o, v) in d[(a - pos) as usize..].iter_mut().zip(g) {
                    *o = *v;
                }
            }
        }
        apply_switches(clip, &mut part);
        for (p, q) in planes.iter_mut().zip(part) {
            p.extend_from_slice(&q);
        }
        pos += n as i64;
    }
    let mut stretch: Vec<&Effect> = Vec::new();
    for e in key
        .effects
        .iter()
        .filter(|e| e.kind.is_audio() && e.enabled)
    {
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        if e.kind.works_offline() {
            through(&mut stretch, &mut planes, rate.0, key.tempo);
            if let Some(out) = crate::fx::offline::render(e, rate.0, std::mem::take(&mut planes)) {
                planes = out;
            }
        } else {
            stretch.push(e);
        }
    }
    through(&mut stretch, &mut planes, rate.0, key.tempo);
    Some(planes)
}

/// `planes` through the real-time effects gathered in `effects` (then
/// emptied), as long as they were: what the chain holds back comes out
/// and its lateness is taken back.
fn through(effects: &mut Vec<&Effect>, planes: &mut Sound, rate: u32, tempo: Tempo) {
    if effects.is_empty() {
        return;
    }
    let mut chain = dsp::Chain::default();
    chain.set_tempo(tempo);
    chain.sync(effects, rate, planes.len(), false);
    let late = chain.latency();
    let len = planes.first().map_or(0, Vec::len);
    for p in planes.iter_mut() {
        p.resize(len + late, 0.0);
    }
    let mut at = 0;
    while at < len + late {
        let end = (at + BLOCK).min(len + late);
        let mut part: Sound = planes.iter().map(|p| p[at..end].to_vec()).collect();
        chain.process(&mut part);
        for (p, q) in planes.iter_mut().zip(part) {
            p[at..end].copy_from_slice(&q);
        }
        at = end;
    }
    for p in planes.iter_mut() {
        p.drain(..late);
    }
    effects.clear();
}
