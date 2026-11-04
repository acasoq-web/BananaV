//! Recording audio into tracks (the transport's Record): each armed track's
//! input captured into a WAV file of its own, written on a thread of its
//! own while the project plays. The files may carry Broadcast Wave metadata
//! (where they belong on the timeline); the waveforms recorded so far and
//! the levels are there for the interface; when the input's first sound was
//! captured lines the takes up with what was heard (the recording latency).

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use bv_core::{SampleRate, Tick};
use bv_timeline::{RecordInput, TrackId};

/// An input device of this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputDevice {
    pub name: String,
    pub channels: u16,
    /// The system's default input.
    pub default: bool,
}

/// The input devices of this computer, the default one first.
pub fn input_devices() -> Vec<InputDevice> {
    let host = cpal::default_host();
    let name_of = |d: &cpal::Device| d.description().ok().map(|d| d.name().to_string());
    let default = host.default_input_device().and_then(|d| name_of(&d));
    let mut out: Vec<InputDevice> = host
        .input_devices()
        .map(|devices| {
            devices
                .filter_map(|d| {
                    let name = name_of(&d)?;
                    let channels = d.default_input_config().ok()?.channels();
                    Some(InputDevice {
                        default: default.as_deref() == Some(name.as_str()),
                        name,
                        channels,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|d| !d.default);
    out
}

/// A track to record: its input, the file it goes to, and where it starts
/// on the timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordTake {
    pub track: TrackId,
    pub input: RecordInput,
    pub path: PathBuf,
    /// Where on the timeline the recording starts.
    pub start: Tick,
    /// Broadcast Wave metadata in the file (Options › Preferences › Audio:
    /// "Record to Broadcast Wave Format").
    pub broadcast_wave: bool,
    /// The effects the signal goes through before it is written (an input
    /// bus's: a wet recording).
    pub effects: Vec<bv_timeline::Effect>,
    /// The project's tempo, for effects with tempo sync.
    pub tempo: bv_timeline::Tempo,
}

/// A file recorded into a track.
#[derive(Clone, Debug, PartialEq)]
pub struct Recorded {
    pub track: TrackId,
    pub path: PathBuf,
    /// How long it is.
    pub length: Tick,
    /// How long after the playback's first sound was heard the file's first
    /// sample was captured (negative: before): where the file starts
    /// against what was heard, the recording latency measured. `None` when
    /// the devices could not tell.
    pub offset: Option<Tick>,
    /// What went wrong writing it, if anything.
    pub error: Option<String>,
}

/// The waveform recorded so far into a track: the lowest and highest sample
/// of each `frames` samples at `rate`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecordingWave {
    pub peaks: Vec<(f32, f32)>,
    pub frames: u32,
    pub rate: u32,
}

/// Samples of the waveform's each peak.
const WAVE_FRAMES: usize = 256;
/// A waveform peak with no samples yet.
const EMPTY_PEAK: (f32, f32, usize) = (f32::INFINITY, f32::NEG_INFINITY, 0);

/// A block of samples from an input device (its index, interleaved).
type Block = (usize, Vec<f32>);

/// What the recording shares with the interface.
#[derive(Default)]
struct Shared {
    /// The loudest sample of each track since it was last read (left,
    /// right).
    levels: Mutex<HashMap<TrackId, [f32; 2]>>,
    waves: Mutex<HashMap<TrackId, RecordingWave>>,
    /// When each device's first samples were captured.
    first_capture: Mutex<Vec<Option<cpal::StreamInstant>>>,
}

/// A file being written.
struct Writing {
    track: TrackId,
    device: usize,
    /// The device's channels in a frame, the first taken and how many.
    stride: usize,
    first: usize,
    channels: usize,
    path: PathBuf,
    writer: Option<bv_wav::WavWriter<BufWriter<File>>>,
    frames: u64,
    error: Option<String>,
    /// The waveform's peak being gathered: lowest, highest, samples.
    gathering: (f32, f32, usize),
    /// The input bus's effects, the signal through them before it is
    /// written.
    chain: Option<bv_mixer::dsp::Chain>,
}

pub(crate) struct Recorder {
    /// The input streams, one per device (dropped: they stop).
    streams: Vec<cpal::Stream>,
    to_writer: Option<Sender<Block>>,
    writer: Option<JoinHandle<Vec<(Recorded, usize)>>>,
    shared: Arc<Shared>,
    started: std::time::Instant,
}

/// Opens `name` (empty: the default input).
fn open_device(name: &str) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if name.is_empty() {
        return host.default_input_device();
    }
    host.input_devices()
        .ok()?
        .find(|d| d.description().is_ok_and(|x| x.name() == name))
        .or_else(|| host.default_input_device())
}

/// The date and time now (UTC) as Broadcast Wave writes them.
fn now_stamp() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days to a civil date (proleptic Gregorian calendar).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        format!("{year:04}-{month:02}-{day:02}"),
        format!("{:02}:{:02}:{:02}", rest / 3600, rest / 60 % 60, rest % 60),
    )
}

/// Converts and sends the device's samples, noting when the first ones
/// were captured.
fn send_block(
    send: &Sender<Block>,
    shared: &Shared,
    index: usize,
    block: Vec<f32>,
    info: &cpal::InputCallbackInfo,
) {
    if let Ok(mut first) = shared.first_capture.lock()
        && first.get(index).is_some_and(Option::is_none)
    {
        first[index] = Some(info.timestamp().capture);
    }
    let _ = send.send((index, block));
}

impl Recorder {
    /// Starts recording `takes` as `bits`-bit WAV files. Each file is at
    /// its device's own rate.
    pub(crate) fn start(takes: Vec<RecordTake>, bits: u16) -> Result<Recorder, String> {
        if takes.is_empty() {
            return Err("no armed track".into());
        }
        // One stream per device the takes use.
        let mut devices: Vec<String> = Vec::new();
        for t in &takes {
            if !devices.contains(&t.input.device) {
                devices.push(t.input.device.clone());
            }
        }
        let shared = Arc::new(Shared::default());
        *shared.first_capture.lock().expect("recording") = vec![None; devices.len()];
        let (date, time) = now_stamp();
        let (tx, rx) = channel::<Block>();
        let mut streams = Vec::new();
        let mut writing = Vec::new();
        for (index, name) in devices.iter().enumerate() {
            let device = open_device(name).ok_or("no audio input device")?;
            let supported = device.default_input_config().map_err(|e| e.to_string())?;
            let config = supported.config();
            let stride = config.channels as usize;
            let rate = config.sample_rate;
            for t in takes.iter().filter(|t| &t.input.device == name) {
                let first = (t.input.first as usize).min(stride.saturating_sub(1));
                let channels = (t.input.channels as usize).clamp(1, 2).min(stride - first);
                let format = match bits {
                    16 => bv_wav::SampleFormat::Int(16),
                    8 => bv_wav::SampleFormat::Int(8),
                    _ => bv_wav::SampleFormat::Int(24),
                };
                if let Some(dir) = t.path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let bext = t.broadcast_wave.then(|| bv_wav::Bext {
                    originator: "BananaV".into(),
                    originator_reference: format!("BV{}{}", date.replace('-', ""), t.track.0),
                    date: date.clone(),
                    time: time.clone(),
                    time_reference: SampleRate(rate).sample_at(t.start).max(0) as u64,
                    ..bv_wav::Bext::default()
                });
                let (writer, error) = match File::create(&t.path)
                    .map_err(|e| e.to_string())
                    .and_then(|f| {
                        bv_wav::WavWriter::with_metadata(
                            BufWriter::new(f),
                            rate,
                            channels as u16,
                            format,
                            &[],
                            bext.as_ref(),
                        )
                        .map_err(|e| e.to_string())
                    }) {
                    Ok(w) => (Some(w), None),
                    Err(e) => (None, Some(e)),
                };
                shared.waves.lock().expect("recording").insert(
                    t.track,
                    RecordingWave {
                        peaks: Vec::new(),
                        frames: WAVE_FRAMES as u32,
                        rate,
                    },
                );
                let chain = (!t.effects.is_empty()).then(|| {
                    let mut c = bv_mixer::dsp::Chain::default();
                    let list: Vec<&bv_timeline::Effect> = t.effects.iter().collect();
                    c.set_tempo(t.tempo);
                    c.sync(&list, rate, channels, false);
                    c
                });
                writing.push(Writing {
                    track: t.track,
                    device: index,
                    stride,
                    first,
                    channels,
                    path: t.path.clone(),
                    writer,
                    frames: 0,
                    error,
                    gathering: EMPTY_PEAK,
                    chain,
                });
            }
            let send = tx.clone();
            let mine = shared.clone();
            let err = |_: cpal::Error| {};
            let stream = match supported.sample_format() {
                cpal::SampleFormat::F32 => device.build_input_stream::<f32, _, _>(
                    config,
                    move |data: &[f32], info| send_block(&send, &mine, index, data.to_vec(), info),
                    err,
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_input_stream::<i16, _, _>(
                    config,
                    move |data: &[i16], info| {
                        let block = data.iter().map(|&s| s as f32 / 32768.0).collect();
                        send_block(&send, &mine, index, block, info);
                    },
                    err,
                    None,
                ),
                cpal::SampleFormat::I32 => device.build_input_stream::<i32, _, _>(
                    config,
                    move |data: &[i32], info| {
                        let block = data.iter().map(|&s| s as f32 / 2_147_483_648.0).collect();
                        send_block(&send, &mine, index, block, info);
                    },
                    err,
                    None,
                ),
                cpal::SampleFormat::U16 => device.build_input_stream::<u16, _, _>(
                    config,
                    move |data: &[u16], info| {
                        let block = data
                            .iter()
                            .map(|&s| (s as f32 - 32768.0) / 32768.0)
                            .collect();
                        send_block(&send, &mine, index, block, info);
                    },
                    err,
                    None,
                ),
                other => return Err(format!("input sample format {other:?}")),
            }
            .map_err(|e| e.to_string())?;
            streams.push((stream, rate));
        }
        let rates: Vec<u32> = streams.iter().map(|&(_, r)| r).collect();
        let to_thread = shared.clone();
        let writer = std::thread::Builder::new()
            .name("bv-record".into())
            .spawn(move || write(rx, writing, rates, &to_thread))
            .map_err(|e| e.to_string())?;
        let streams: Vec<cpal::Stream> = streams.into_iter().map(|(s, _)| s).collect();
        for s in &streams {
            s.play().map_err(|e| e.to_string())?;
        }
        Ok(Recorder {
            streams,
            to_writer: Some(tx),
            writer: Some(writer),
            shared,
            started: std::time::Instant::now(),
        })
    }

    /// How long it has been recording.
    pub(crate) fn elapsed(&self) -> std::time::Duration {
        self.started.elapsed()
    }

    /// The loudest input of each track since the last call.
    pub(crate) fn take_levels(&self) -> Vec<(TrackId, [f32; 2])> {
        self.shared
            .levels
            .lock()
            .map(|mut l| l.drain().collect())
            .unwrap_or_default()
    }

    /// The waveform of `track` from peak `from` on.
    fn wave(&self, track: TrackId, from: usize) -> Option<RecordingWave> {
        let waves = self.shared.waves.lock().ok()?;
        let w = waves.get(&track)?;
        Some(RecordingWave {
            peaks: w.peaks.get(from..).map(<[_]>::to_vec).unwrap_or_default(),
            frames: w.frames,
            rate: w.rate,
        })
    }

    /// Stops and closes the files; with `heard` (when the playback's first
    /// sound was heard), how far each file starts from it.
    pub(crate) fn stop(mut self, heard: Option<cpal::StreamInstant>) -> Vec<Recorded> {
        self.streams.clear();
        self.to_writer = None;
        let mut done = self
            .writer
            .take()
            .and_then(|w| w.join().ok())
            .unwrap_or_default();
        let captures = self
            .shared
            .first_capture
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default();
        for (r, device) in &mut done {
            let capture = captures.get(*device).copied().flatten();
            r.offset = match (capture, heard) {
                (Some(c), Some(h)) => Some(match c.checked_duration_since(h) {
                    Some(late) => Tick::from_seconds_f64(late.as_secs_f64()),
                    None => -Tick::from_seconds_f64(h.duration_since(c).as_secs_f64()),
                }),
                _ => None,
            };
        }
        done.into_iter().map(|(r, _)| r).collect()
    }
}

/// The writer thread: each block into the files of its device, until the
/// streams are gone. The files written, with their devices.
fn write(
    rx: Receiver<Block>,
    mut writing: Vec<Writing>,
    rates: Vec<u32>,
    shared: &Shared,
) -> Vec<(Recorded, usize)> {
    while let Ok((device, block)) = rx.recv() {
        for w in writing.iter_mut().filter(|w| w.device == device) {
            let n = block.len() / w.stride.max(1);
            let mut planar: Vec<Vec<f32>> = (0..w.channels)
                .map(|c| (0..n).map(|i| block[i * w.stride + w.first + c]).collect())
                .collect();
            // Through an input bus's effects: recorded wet.
            if let Some(chain) = &mut w.chain {
                chain.process(&mut planar);
            }
            let mut peak = [0.0f32; 2];
            for (c, ch) in planar.iter().enumerate() {
                let p = ch.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                peak[c.min(1)] = peak[c.min(1)].max(p);
            }
            if w.channels == 1 {
                peak[1] = peak[0];
            }
            if let Ok(mut l) = shared.levels.lock() {
                let e = l.entry(w.track).or_insert([0.0; 2]);
                e[0] = e[0].max(peak[0]);
                e[1] = e[1].max(peak[1]);
            }
            // The waveform: the lowest and highest of each few samples, the
            // channels together.
            let mut new = Vec::new();
            for i in 0..n {
                let (lo, hi, k) = &mut w.gathering;
                for ch in &planar {
                    *lo = lo.min(ch[i]);
                    *hi = hi.max(ch[i]);
                }
                *k += 1;
                if *k == WAVE_FRAMES {
                    new.push((*lo, *hi));
                    w.gathering = EMPTY_PEAK;
                }
            }
            if !new.is_empty()
                && let Ok(mut waves) = shared.waves.lock()
                && let Some(wave) = waves.get_mut(&w.track)
            {
                wave.peaks.extend(new);
            }
            if let Some(writer) = &mut w.writer {
                match writer.write_planar(&planar) {
                    Ok(()) => w.frames += n as u64,
                    Err(e) => {
                        w.error = Some(e.to_string());
                        w.writer = None;
                    }
                }
            }
        }
    }
    writing
        .into_iter()
        .map(|mut w| {
            if let Some(writer) = w.writer.take()
                && let Err(e) = writer.finish()
            {
                w.error.get_or_insert(e.to_string());
            }
            let rate = SampleRate(rates.get(w.device).copied().unwrap_or(48_000));
            let r = Recorded {
                track: w.track,
                path: w.path,
                length: rate.sample_start(w.frames as i64),
                offset: None,
                error: w.error,
            };
            (r, w.device)
        })
        .collect()
}

impl crate::Engine {
    /// Starts recording `takes` (as `bits`-bit WAV files) while the project
    /// plays from `from`, so what is recorded over is heard.
    pub fn start_recording(
        &mut self,
        takes: Vec<RecordTake>,
        bits: u16,
        from: Tick,
        options: crate::PlayOptions,
    ) -> Result<(), String> {
        let files: Vec<PathBuf> = takes.iter().map(|t| t.path.clone()).collect();
        match Recorder::start(takes, bits) {
            Ok(recorder) => {
                self.recorder = Some(recorder);
                self.play(from, options);
                Ok(())
            }
            Err(e) => {
                // No half-made files left behind.
                for f in files {
                    let _ = std::fs::remove_file(f);
                }
                Err(e)
            }
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recorder.is_some()
    }

    /// How long it has been recording.
    pub fn recording_time(&self) -> Option<std::time::Duration> {
        self.recorder.as_ref().map(Recorder::elapsed)
    }

    /// The loudest input of each recording track since the last call, for
    /// its meter.
    pub fn recording_levels(&self) -> Vec<(TrackId, [f32; 2])> {
        self.recorder
            .as_ref()
            .map(Recorder::take_levels)
            .unwrap_or_default()
    }

    /// The waveform recorded into `track` so far, from peak `from` on (the
    /// interface keeps the ones it has).
    pub fn recording_wave(&self, track: TrackId, from: usize) -> Option<RecordingWave> {
        self.recorder.as_ref()?.wave(track, from)
    }

    /// Stops recording (and playing); the files recorded.
    pub fn stop_recording(&mut self) -> Vec<Recorded> {
        let heard = self.playback.as_ref().and_then(|p| p.first_heard());
        self.stop();
        self.recorder
            .take()
            .map(|r| r.stop(heard))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file being written in `dir` for `track`, taking `channels` from
    /// `first` of a stereo device.
    fn writing(dir: &std::path::Path, track: u64, first: usize, channels: usize) -> Writing {
        let path = dir.join(format!("take{track}.wav"));
        let writer = bv_wav::WavWriter::new(
            BufWriter::new(File::create(&path).unwrap()),
            48_000,
            channels as u16,
            bv_wav::SampleFormat::Int(24),
        )
        .unwrap();
        Writing {
            track: TrackId(track),
            device: 0,
            stride: 2,
            first,
            channels,
            path,
            writer: Some(writer),
            frames: 0,
            error: None,
            gathering: EMPTY_PEAK,
            chain: None,
        }
    }

    #[test]
    fn each_track_gets_its_channels_of_the_input() {
        let dir = std::env::temp_dir().join(format!("bv-record-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, rx) = channel::<Block>();
        // 10 ms of a stereo input: left at 0.5, right at −0.25.
        for _ in 0..2 {
            let block: Vec<f32> = (0..240).flat_map(|_| [0.5, -0.25]).collect();
            tx.send((0, block)).unwrap();
        }
        drop(tx);
        let shared = Shared::default();
        for t in [1, 2] {
            shared
                .waves
                .lock()
                .unwrap()
                .insert(TrackId(t), RecordingWave::default());
        }
        let takes = vec![writing(&dir, 1, 0, 2), writing(&dir, 2, 1, 1)];
        let done = write(rx, takes, vec![48_000], &shared);
        assert_eq!(done.len(), 2);
        for (r, device) in &done {
            assert_eq!((r.error.as_deref(), *device), (None, 0));
            assert_eq!(r.length, Tick::from_seconds_f64(0.01));
        }
        let read = |p: &std::path::Path| {
            let mut r =
                bv_wav::WavReader::new(std::io::BufReader::new(File::open(p).unwrap())).unwrap();
            r.read_planar(0, 480).unwrap()
        };
        let stereo = read(&done[0].0.path);
        assert!((stereo[0][100] - 0.5).abs() < 1e-4 && (stereo[1][100] + 0.25).abs() < 1e-4);
        let right = read(&done[1].0.path);
        assert_eq!(right.len(), 1);
        assert!((right[0][479] + 0.25).abs() < 1e-4);
        // The meters' levels and the waveforms (480 samples: one peak of
        // 256, the rest still gathering).
        let l = shared.levels.lock().unwrap();
        assert!((l[&TrackId(1)][0] - 0.5).abs() < 1e-6);
        assert!((l[&TrackId(2)][1] - 0.25).abs() < 1e-6);
        let waves = shared.waves.lock().unwrap();
        assert_eq!(waves[&TrackId(1)].peaks, vec![(-0.25, 0.5)]);
        assert_eq!(waves[&TrackId(2)].peaks, vec![(-0.25, -0.25)]);
        drop((l, waves));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn broadcast_wave_dates_are_civil_dates() {
        let (date, time) = now_stamp();
        assert_eq!(date.len(), 10);
        assert!(date.starts_with("20"), "{date}");
        assert_eq!(time.len(), 8);
    }
}
