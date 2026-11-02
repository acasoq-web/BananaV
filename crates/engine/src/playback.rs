//! Playback com o relógio de áudio como mestre.
//!
//! Uma thread de mixagem mantém ~250 ms de áudio prontos num buffer circular.
//! O callback da placa de som consome o buffer e conta quantas amostras
//! realmente tocaram; a posição do playback vem dessa contagem. Se a mixagem
//! atrasar, o relógio para junto com o som, e a imagem espera — nunca o
//! contrário. Sem dispositivo de áudio, o relógio de parede assume.
//!
//! Loop: a mixagem volta ao início da região exatamente na amostra do fim, e
//! a posição é dobrada do mesmo jeito, então som e imagem continuam juntos.
//! Taxa: a mixagem é reamostrada (como uma fita mais rápida ou mais lenta) e
//! o relógio avança `taxa` vezes mais rápido.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bv_core::{SampleRate, TICKS_PER_SECOND, Tick, TimeRange};
use bv_mixer::AudioPool;
use bv_timeline::Project;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::metronome::Clicks;
use crate::resample::LinearResampler;
use crate::track_mix::TrackThreads;

/// Situação da saída de áudio, para mostrar na interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioStatus {
    NotStarted,
    Device {
        name: String,
        sample_rate: u32,
        channels: u16,
    },
    /// Sem áudio; o playback usa o relógio do sistema.
    Unavailable(String),
}

/// How to play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayOptions {
    /// Loops within this region.
    pub looping: Option<TimeRange>,
    /// 1.0: normal speed.
    pub rate: f64,
    /// Plays on past the project's end until stopped (the timeline's
    /// playback, as VEGAS's); otherwise it ends there (a media's preview, a
    /// real-time render).
    pub past_end: bool,
}

impl Default for PlayOptions {
    fn default() -> PlayOptions {
        PlayOptions {
            looping: None,
            rate: 1.0,
            past_end: false,
        }
    }
}

/// How much audio the mix keeps ready ahead of the device, in ms (Options ›
/// Preferences › Audio Device: "Playback buffering").
static PLAYBACK_BUFFER_MS: AtomicU32 = AtomicU32::new(100);
/// "Track buffering": how much more, in ms, when each track is mixed on a
/// thread of its own; 0 mixes the tracks on the playback's thread.
static TRACK_BUFFER_MS: AtomicU32 = AtomicU32::new(250);
/// The output device, by name (`None`: the system's default).
static OUTPUT_DEVICE: Mutex<Option<String>> = Mutex::new(None);
/// What went wrong with the audio output since the interface last asked:
/// opening it, or the device failing while open.
static AUDIO_ERRORS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Keeps an audio output failure for [`take_audio_errors`] (a few at most).
pub(crate) fn note_audio_error(what: String) {
    let mut errors = AUDIO_ERRORS.lock().unwrap_or_else(|e| e.into_inner());
    if errors.len() < 16 {
        errors.push(what);
    }
}

/// The audio output's failures since the last call, oldest first.
pub(crate) fn take_audio_errors() -> Vec<String> {
    std::mem::take(&mut *AUDIO_ERRORS.lock().unwrap_or_else(|e| e.into_inner()))
}
/// Options › Metronome: its clicks while it is on.
static METRONOME: Mutex<Option<Arc<Clicks>>> = Mutex::new(None);
/// The Master Bus's Downmix Output (0 off, 1 stereo, 2 mono) and Dim
/// Output: what playback sends the device, never what renders write.
static DOWNMIX: AtomicU32 = AtomicU32::new(0);
static DIM: AtomicBool = AtomicBool::new(false);

/// Dim Output's cut: 20 dB.
const DIM_GAIN: f32 = 0.1;

/// File › Real-Time Render: each block playback mixes — the program,
/// before the metronome's clicks and the Master Bus's monitoring
/// switches — with its first sample, while one goes on.
type Capture = std::sync::mpsc::Sender<(i64, Vec<Vec<f32>>)>;
static CAPTURE: Mutex<Option<Capture>> = Mutex::new(None);

pub(crate) fn set_capture(to: Option<Capture>) {
    *CAPTURE.lock().expect("captura") = to;
}

/// The Loudness Meters' engine: what plays, measured from the start of
/// each playback (or the last reset); its true peaks through the blocking
/// filter when it is on.
static LOUDNESS: Mutex<Option<bv_mixer::loudness::LiveMeter>> = Mutex::new(None);
static TRUE_PEAK_BLOCKING: AtomicBool = AtomicBool::new(false);
/// The project's sample rate and channels the meter was made for.
static LOUDNESS_FORMAT: Mutex<(u32, usize)> = Mutex::new((48_000, 2));

/// A fresh meter for `rate` and `channels`.
pub(crate) fn reset_loudness_for(rate: u32, channels: usize) {
    *LOUDNESS_FORMAT.lock().expect("medidor de loudness") = (rate, channels);
    let blocking = TRUE_PEAK_BLOCKING.load(Ordering::Relaxed);
    *LOUDNESS.lock().expect("medidor de loudness") =
        Some(bv_mixer::loudness::LiveMeter::new(rate, channels, blocking));
}

/// Reset Meter Measurement: the meters start again.
pub(crate) fn reset_loudness() {
    let (rate, channels) = *LOUDNESS_FORMAT.lock().expect("medidor de loudness");
    reset_loudness_for(rate, channels);
}

/// Reset Clip: the true peaks' over-the-limit mark starts again.
pub(crate) fn reset_true_peak_clip() {
    if let Some(m) = LOUDNESS.lock().expect("medidor de loudness").as_mut() {
        m.reset_clip();
    }
}

pub(crate) fn set_true_peak_blocking(on: bool) {
    TRUE_PEAK_BLOCKING.store(on, Ordering::Relaxed);
}

/// The meters' readings now (`None` before anything played).
pub(crate) fn loudness() -> Option<bv_mixer::loudness::LiveLoudness> {
    LOUDNESS
        .lock()
        .expect("medidor de loudness")
        .as_mut()
        .map(|m| m.read())
}

pub(crate) fn set_downmix(d: bv_mixer::Downmix) {
    let v = match d {
        bv_mixer::Downmix::Off => 0,
        bv_mixer::Downmix::Stereo => 1,
        bv_mixer::Downmix::Mono => 2,
    };
    DOWNMIX.store(v, Ordering::Relaxed);
}

pub(crate) fn downmix() -> bv_mixer::Downmix {
    match DOWNMIX.load(Ordering::Relaxed) {
        1 => bv_mixer::Downmix::Stereo,
        2 => bv_mixer::Downmix::Mono,
        _ => bv_mixer::Downmix::Off,
    }
}

pub(crate) fn set_dim(on: bool) {
    DIM.store(on, Ordering::Relaxed);
}

pub(crate) fn dim() -> bool {
    DIM.load(Ordering::Relaxed)
}

/// Sets the buffering: `playback` ahead of the device, and `track` more with
/// a thread per track (`None`: no track threads).
pub(crate) fn set_buffering(playback: Duration, track: Option<Duration>) {
    PLAYBACK_BUFFER_MS.store(
        playback.as_millis().clamp(10, 2000) as u32,
        Ordering::Relaxed,
    );
    let track = track.map_or(0, |t| t.as_millis().clamp(10, 2000) as u32);
    TRACK_BUFFER_MS.store(track, Ordering::Relaxed);
}

/// The output device by name (`None`: the system's default).
pub(crate) fn set_output_device(name: Option<String>) {
    *OUTPUT_DEVICE.lock().expect("dispositivo de saída") = name;
}

pub(crate) fn output_device() -> Option<String> {
    OUTPUT_DEVICE.lock().expect("dispositivo de saída").clone()
}

/// Options › Metronome on (with these clicks) or off.
pub(crate) fn set_metronome(clicks: Option<Clicks>) {
    *METRONOME.lock().expect("metrônomo") = clicks.map(Arc::new);
}

/// Tamanho do bloco de mixagem, em amostras do projeto.
const MIX_BLOCK: usize = 1024;

/// The metronome's clicks made for a rate: the sounds, the rate, and the
/// beat and accent at it.
type ClicksAt = (Arc<Clicks>, u32, (Vec<f32>, Vec<f32>));

struct Clock {
    /// Amostras do dispositivo efetivamente tocadas.
    frames: u64,
    /// Quando o último callback terminou e quantas amostras ele entregou;
    /// usado para interpolar a posição entre callbacks.
    at: Instant,
    last_len: u64,
}

/// Peaks of each track in a mixed block, and the device frame where the
/// block starts playing.
type TrackPeaks = Vec<(bv_timeline::TrackId, Vec<f32>)>;
type StripPeaks = Vec<(bv_timeline::Strip, Vec<f32>)>;
type VuLevels = Vec<(bv_mixer::meters::MeterOf, Vec<f32>)>;

struct AudioShared {
    ring: Mutex<VecDeque<f32>>,
    clock: Mutex<Clock>,
    /// Pico por canal desde a última leitura (medidores).
    peaks: Mutex<Vec<f32>>,
    /// Track, bus and chain peaks and the VU/PPM readings of the mixed
    /// blocks not heard yet, by start frame.
    track_queue: Mutex<VecDeque<(u64, TrackPeaks, StripPeaks, VuLevels)>>,
    /// Track peaks heard since the last reading (track meters).
    track_peaks: Mutex<TrackPeaks>,
    /// Bus and chain peaks heard since the last reading.
    strip_peaks: Mutex<StripPeaks>,
    /// The VU/PPM readings heard last (the latest of each meter).
    vu_levels: Mutex<VuLevels>,
    /// When the first sound of the playback reached the device's output.
    first_heard: Mutex<Option<cpal::StreamInstant>>,
}

pub(crate) struct Playback {
    start: Tick,
    options: PlayOptions,
    started_at: Instant,
    device_rate: u32,
    audio: Option<Arc<AudioShared>>,
    project: Arc<Mutex<Arc<Project>>>,
    stop: Arc<AtomicBool>,
    mixer: Option<JoinHandle<()>>,
    /// The output it plays to (kept by the engine between playbacks, or its
    /// own); it goes silent when the playback ends.
    output: Option<Arc<Output>>,
    status: AudioStatus,
}

/// What an output plays: the playback's buffer, or nothing (silence).
type Slot = Arc<Mutex<Option<Arc<AudioShared>>>>;

/// The audio output. The engine keeps one open between playbacks while
/// BananaV holds the device, as VEGAS does while it is the active
/// application: playback then starts at once. Silent while nothing plays.
pub(crate) struct Output {
    _stream: cpal::Stream,
    slot: Slot,
    /// Cleared when the device fails (unplugged): a new output is opened.
    alive: Arc<AtomicBool>,
    pub(crate) rate: u32,
    pub(crate) channels: u16,
    pub(crate) name: String,
}

impl Output {
    /// Opens the output (the device chosen in Options › Preferences › Audio
    /// Device, or the system's); f32 at `preferred_rate` when it can (no
    /// resampling).
    pub(crate) fn open(preferred_rate: u32) -> Result<Output, String> {
        let slot: Slot = Arc::new(Mutex::new(None));
        let alive = Arc::new(AtomicBool::new(true));
        let (stream, rate, channels, name) =
            open_output(preferred_rate, slot.clone(), alive.clone())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Output {
            _stream: stream,
            slot,
            alive,
            rate,
            channels,
            name,
        })
    }

    pub(crate) fn alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    fn attach(&self, audio: Arc<AudioShared>) {
        *self.slot.lock().expect("saída de áudio") = Some(audio);
    }

    fn detach(&self) {
        *self.slot.lock().expect("saída de áudio") = None;
    }

    pub(crate) fn status(&self) -> AudioStatus {
        AudioStatus::Device {
            name: self.name.clone(),
            sample_rate: self.rate,
            channels: self.channels,
        }
    }

    /// The name of the system's default output now.
    pub(crate) fn default_name() -> Option<String> {
        let device = cpal::default_host().default_output_device()?;
        device.description().ok().map(|d| d.name().to_string())
    }
}

/// Região de loop válida que contém (ou recebe) o ponto de partida.
fn effective_loop(options: &PlayOptions) -> Option<TimeRange> {
    options.looping.filter(|r| r.duration > Tick::ZERO)
}

impl Playback {
    /// Starts playing `project` from `from`, on `held` (the output the engine
    /// keeps open) or else on an output of its own.
    pub(crate) fn start(
        project: Arc<Project>,
        from: Tick,
        options: PlayOptions,
        held: Option<Arc<Output>>,
    ) -> Playback {
        let project_rate = project.settings.sample_rate;
        let rate = options.rate.clamp(0.1, 4.0);
        let options = PlayOptions { rate, ..options };
        // Fora da região de loop, começa no início dela.
        let from = match effective_loop(&options) {
            Some(r) if !r.contains(from) => r.start,
            _ => from,
        };
        // The Loudness Meters start again with each playback, as VEGAS's.
        reset_loudness_for(project_rate.0, project.settings.channels.channel_count());
        let shared_project = Arc::new(Mutex::new(project));
        let stop = Arc::new(AtomicBool::new(false));
        let audio = Arc::new(AudioShared {
            ring: Mutex::new(VecDeque::new()),
            clock: Mutex::new(Clock {
                frames: 0,
                at: Instant::now(),
                last_len: 0,
            }),
            peaks: Mutex::new(Vec::new()),
            track_queue: Mutex::new(VecDeque::new()),
            track_peaks: Mutex::new(Vec::new()),
            strip_peaks: Mutex::new(Vec::new()),
            vu_levels: Mutex::new(Vec::new()),
            first_heard: Mutex::new(None),
        });

        let output = match held.filter(|o| o.alive()) {
            Some(o) => Ok(o),
            None => Output::open(project_rate.0).map(Arc::new),
        };
        let (output, status, device_rate, device_channels) = match output {
            Ok(o) => {
                o.attach(audio.clone());
                let (status, rate, channels) = (o.status(), o.rate, o.channels as usize);
                (Some(o), status, rate, channels)
            }
            Err(e) => {
                note_audio_error(format!("The audio output could not be opened: {e}"));
                (None, AudioStatus::Unavailable(e), project_rate.0, 2)
            }
        };

        let mixer = output.is_some().then(|| {
            let project = shared_project.clone();
            let stop = stop.clone();
            let audio = audio.clone();
            std::thread::Builder::new()
                .name("bv-mixer".into())
                .spawn(move || {
                    mix_loop(
                        project,
                        from,
                        options,
                        device_rate,
                        device_channels,
                        audio,
                        stop,
                    )
                })
                .expect("não foi possível criar a thread de mixagem")
        });
        Playback {
            start: from,
            options,
            started_at: Instant::now(),
            device_rate,
            audio: output.is_some().then_some(audio),
            project: shared_project,
            stop,
            mixer,
            output,
            status,
        }
    }

    /// How fast it plays (1 = normal speed).
    pub(crate) fn rate(&self) -> f64 {
        self.options.rate
    }

    pub(crate) fn audio_status(&self) -> AudioStatus {
        self.status.clone()
    }

    pub(crate) fn set_project(&self, project: Arc<Project>) {
        *self.project.lock().expect("projeto do playback") = project;
    }

    /// Tempo da timeline percorrido desde o início, sem considerar o loop.
    fn elapsed(&self) -> Tick {
        let seconds = match &self.audio {
            Some(a) => {
                let c = a.clock.lock().expect("relógio de áudio");
                // Interpola dentro do último buffer entregue, sem passar dele.
                let since = c.at.elapsed().as_secs_f64() * self.device_rate as f64;
                let extra = (since as u64).min(c.last_len);
                (c.frames + extra) as f64 / self.device_rate as f64
            }
            None => self.started_at.elapsed().as_secs_f64(),
        };
        Tick((seconds * self.options.rate * TICKS_PER_SECOND as f64) as i64)
    }

    pub(crate) fn position(&self) -> Tick {
        let raw = self.start + self.elapsed();
        match effective_loop(&self.options) {
            Some(r) if raw >= r.end() => r.start + Tick((raw - r.end()).0 % r.duration.0),
            _ => raw,
        }
    }

    pub(crate) fn finished(&self) -> bool {
        if effective_loop(&self.options).is_some() || self.options.past_end {
            return false;
        }
        let end = self.project.lock().expect("projeto do playback").duration();
        self.position() >= end
    }

    /// Pico por canal desde a última chamada (0 a 1+).
    pub(crate) fn take_peaks(&self) -> Option<Vec<f32>> {
        let a = self.audio.as_ref()?;
        let mut p = a.peaks.lock().expect("medidores");
        Some(std::mem::take(&mut *p))
    }

    /// When the playback's first sound reached the output (recording lines
    /// the takes up with it).
    pub(crate) fn first_heard(&self) -> Option<cpal::StreamInstant> {
        *self
            .audio
            .as_ref()?
            .first_heard
            .lock()
            .expect("relógio de áudio")
    }

    /// Peak per channel of each track heard since the last call.
    pub(crate) fn take_track_peaks(&self) -> TrackPeaks {
        self.audio.as_ref().map_or_else(Vec::new, |a| {
            std::mem::take(&mut *a.track_peaks.lock().expect("medidores"))
        })
    }

    /// The latest VU/PPM reading of each meter heard since the last call.
    pub(crate) fn take_vu_levels(&self) -> VuLevels {
        self.audio.as_ref().map_or_else(Vec::new, |a| {
            std::mem::take(&mut *a.vu_levels.lock().expect("medidores"))
        })
    }

    /// Peak per channel of each bus and chain heard since the last call.
    pub(crate) fn take_strip_peaks(&self) -> StripPeaks {
        self.audio.as_ref().map_or_else(Vec::new, |a| {
            std::mem::take(&mut *a.strip_peaks.lock().expect("medidores"))
        })
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        if let Some(o) = &self.output {
            o.detach();
        }
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.mixer.take() {
            let _ = t.join();
        }
    }
}

/// Abre a saída padrão. Prefere f32 na taxa do projeto (sem reamostragem).
fn open_output(
    project_rate: u32,
    slot: Slot,
    alive: Arc<AtomicBool>,
) -> Result<(cpal::Stream, u32, u16, String), String> {
    let host = cpal::default_host();
    let named = output_device().and_then(|name| {
        host.output_devices()
            .ok()?
            .find(|d| d.description().is_ok_and(|x| x.name() == name))
    });
    let device = named
        .or_else(|| host.default_output_device())
        .ok_or("no audio output device")?;
    let name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "default output".into());
    let preferred = device.supported_output_configs().ok().and_then(|configs| {
        configs
            .filter(|c| c.sample_format() == cpal::SampleFormat::F32 && c.channels() >= 2)
            .find(|c| c.min_sample_rate() <= project_rate && project_rate <= c.max_sample_rate())
            .map(|c| c.with_sample_rate(project_rate))
    });
    let supported = match preferred {
        Some(c) => c,
        None => device.default_output_config().map_err(|e| e.to_string())?,
    };
    let config = supported.config();
    let channels = config.channels;
    let rate = config.sample_rate;
    // Errors (the device removed) stop the sound, and the clock with it
    // since nothing more is taken; the output is opened anew next time. The
    // first is kept for the interface to note down.
    let failed = name.clone();
    let err = move |e: cpal::Error| {
        if alive.swap(false, Ordering::Relaxed) {
            note_audio_error(format!("The audio device \"{failed}\" failed: {e}"));
        }
    };
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream::<f32, _, _>(
            config,
            move |out: &mut [f32], info| {
                play_into(out, &slot, channels, info.timestamp().playback, |s| s)
            },
            err,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_output_stream::<i16, _, _>(
            config,
            move |out: &mut [i16], info| {
                play_into(out, &slot, channels, info.timestamp().playback, |s| {
                    (s.clamp(-1.0, 1.0) * 32767.0) as i16
                })
            },
            err,
            None,
        ),
        other => return Err(format!("unsupported sample format: {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    Ok((stream, rate, channels, name))
}

/// The device's callback: the playing buffer, or silence.
fn play_into<T: Copy + Default>(
    out: &mut [T],
    slot: &Slot,
    channels: u16,
    at: cpal::StreamInstant,
    convert: impl Fn(f32) -> T,
) {
    let audio = slot.lock().expect("saída de áudio").clone();
    match audio {
        Some(a) => fill(out, &a, channels, at, convert),
        None => out.fill(T::default()),
    }
}

/// Callback da placa: copia do buffer circular, mede picos e avança o relógio.
fn fill<T: Copy + Default>(
    out: &mut [T],
    audio: &AudioShared,
    channels: u16,
    at: cpal::StreamInstant,
    convert: impl Fn(f32) -> T,
) {
    let channels = channels as usize;
    let mut peaks = vec![0.0f32; channels];
    let mut ring = audio.ring.lock().expect("buffer de áudio");
    // Só quadros completos (todas as amostras de um instante) contam.
    let available = (ring.len() / channels).min(out.len() / channels);
    for (i, (o, s)) in out
        .iter_mut()
        .zip(ring.drain(..available * channels))
        .enumerate()
    {
        let p = &mut peaks[i % channels];
        *p = p.max(s.abs());
        *o = convert(s);
    }
    drop(ring);
    for o in &mut out[available * channels..] {
        *o = T::default();
    }
    {
        let mut shared = audio.peaks.lock().expect("medidores");
        if shared.len() != channels {
            *shared = vec![0.0; channels];
        }
        for (s, p) in shared.iter_mut().zip(&peaks) {
            *s = s.max(*p);
        }
    }
    let heard = {
        let mut c = audio.clock.lock().expect("relógio de áudio");
        if c.frames == 0 && available > 0 {
            *audio.first_heard.lock().expect("relógio de áudio") = Some(at);
        }
        c.frames += available as u64;
        c.at = Instant::now();
        c.last_len = available as u64;
        c.frames
    };
    // The meters follow what is heard, not what is mixed ahead.
    let mut queue = audio.track_queue.lock().expect("medidores");
    let mut tracks = audio.track_peaks.lock().expect("medidores");
    let mut strips = audio.strip_peaks.lock().expect("medidores");
    let mut vu = audio.vu_levels.lock().expect("medidores");
    while queue.front().is_some_and(|(at, _, _, _)| *at <= heard) {
        let Some((_, block, strip_block, vu_block)) = queue.pop_front() else {
            break;
        };
        hold_peaks(&mut tracks, block);
        hold_peaks(&mut strips, strip_block);
        // A VU's needle is where it is now: the latest reading.
        for (of, levels) in vu_block {
            match vu.iter_mut().find(|(k, _)| *k == of) {
                Some((_, l)) => *l = levels,
                None => vu.push((of, levels)),
            }
        }
    }
}

/// Each one's peaks in `block` held in `shared` (the highest per channel).
fn hold_peaks<K: PartialEq>(shared: &mut Vec<(K, Vec<f32>)>, block: Vec<(K, Vec<f32>)>) {
    for (key, peaks) in block {
        match shared.iter_mut().find(|(k, _)| *k == key) {
            Some((_, p)) => {
                if p.len() < peaks.len() {
                    p.resize(peaks.len(), 0.0);
                }
                for (a, b) in p.iter_mut().zip(&peaks) {
                    *a = a.max(*b);
                }
            }
            None => shared.push((key, peaks)),
        }
    }
}

fn mix_loop(
    project: Arc<Mutex<Arc<Project>>>,
    from: Tick,
    options: PlayOptions,
    device_rate: u32,
    device_channels: usize,
    audio: Arc<AudioShared>,
    stop: Arc<AtomicBool>,
) {
    let mut pool = AudioPool::new();
    let mut threads = (TRACK_BUFFER_MS.load(Ordering::Relaxed) > 0).then(TrackThreads::default);
    // The metronome's clicks at the project's rate, made once.
    let mut clicks: Option<ClicksAt> = None;
    let first = project.lock().expect("projeto").settings;
    let rate = first.sample_rate;
    let mut next = rate.sample_at(from);
    let loop_samples = effective_loop(&options).map(|r| {
        (
            first_sample(rate, r.start),
            first_sample(rate, r.end()).max(first_sample(rate, r.start) + 1),
        )
    });
    let project_channels = first.channels.channel_count();
    // The project's channels, and the device's for the busses on its
    // hardware outputs.
    let width = project_channels.max(device_channels);
    let resample = options.rate != 1.0 || rate.0 != device_rate;
    let mut resampler = resample.then(|| {
        LinearResampler::with_step(rate.0 as f64 * options.rate / device_rate as f64, width)
    });
    while !stop.load(Ordering::Relaxed) {
        let ahead_ms = PLAYBACK_BUFFER_MS.load(Ordering::Relaxed)
            + if threads.is_some() {
                TRACK_BUFFER_MS.load(Ordering::Relaxed)
            } else {
                0
            };
        let target = (ahead_ms as f64 / 1000.0 * device_rate as f64) as usize * device_channels;
        let buffered = audio.ring.lock().expect("buffer de áudio").len();
        if buffered >= target {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        let mut n = MIX_BLOCK;
        if let Some((start, end)) = loop_samples {
            if next >= end {
                next = start;
            }
            n = n.min((end - next) as usize);
        }
        let p = project.lock().expect("projeto").clone();
        let (mut buf, track_peaks, strip_peaks, vu_levels, hardware) = match &mut threads {
            Some(t) => {
                let (b, peaks) = t.mix(&p, next, n);
                let (strips, vu) = (t.take_strip_peaks(), t.take_vu_levels());
                (b, peaks, strips, vu, t.take_hardware())
            }
            None => {
                let b = bv_mixer::mix(&p, next, n, &mut pool);
                let strips = pool.take_strip_peaks();
                let peaks = pool.take_track_peaks();
                (
                    b,
                    peaks,
                    strips,
                    pool.take_vu_levels(),
                    pool.take_hardware(),
                )
            }
        };
        // The Loudness Meters measure the program (no clicks, no monitoring),
        // and a real-time render writes it.
        if let Some(m) = LOUDNESS.lock().expect("medidor de loudness").as_mut() {
            m.push(&buf.channels);
        }
        if let Some(to) = CAPTURE.lock().expect("captura").as_ref() {
            let _ = to.send((next, buf.channels.clone()));
        }
        let metronome = METRONOME.lock().expect("metrônomo").clone();
        if let Some(c) = metronome {
            let rate = p.settings.sample_rate;
            if clicks
                .as_ref()
                .is_none_or(|(a, r, _)| !Arc::ptr_eq(a, &c) || *r != rate.0)
            {
                let at = c.at(rate.0);
                clicks = Some((c, rate.0, at));
            }
            if let Some((_, _, (normal, accent))) = &clicks {
                crate::metronome::add_clicks(
                    &mut buf,
                    next,
                    rate,
                    &p.settings.tempo,
                    p.ruler_offset,
                    (normal, accent),
                );
            }
        }
        // The Master Bus's monitoring switches.
        bv_mixer::downmix(&mut buf.channels, p.settings.channels, downmix());
        if dim() {
            for ch in &mut buf.channels {
                for s in ch.iter_mut() {
                    *s *= DIM_GAIN;
                }
            }
        }
        // The Master's pairs on the device's pairs its Audio Device
        // Selector picks, then the busses on hardware outputs on theirs
        // (never through the Master's switches).
        buf.channels = route_master(
            std::mem::take(&mut buf.channels),
            p.master.device_pairs,
            device_channels,
        );
        add_hardware(&mut buf.channels, &hardware, device_channels, width);
        next += n as i64;
        // The device frame where this block will be heard.
        let queued = (audio.ring.lock().expect("buffer de áudio").len() / device_channels) as u64;
        let heard_at = audio.clock.lock().expect("relógio de áudio").frames + queued;
        audio.track_queue.lock().expect("medidores").push_back((
            heard_at,
            track_peaks,
            strip_peaks,
            vu_levels,
        ));
        let channels = match &mut resampler {
            Some(r) => r.process(&buf.channels),
            None => buf.channels,
        };
        let frames = channels.first().map_or(0, Vec::len);
        let mut interleaved = Vec::with_capacity(frames * device_channels);
        for i in 0..frames {
            for c in 0..device_channels {
                // Canais do dispositivo além dos do projeto ficam em silêncio.
                interleaved.push(channels.get(c).map_or(0.0, |ch| ch[i]));
            }
        }
        audio
            .ring
            .lock()
            .expect("buffer de áudio")
            .extend(interleaved);
    }
}

/// The Master's pairs of channels (L/R, C/LFE, Ls/Rs) on the device's
/// pairs `pairs` names, summed where two meet; a channel past the
/// device's `device` ones is left where it was (unheard, as before).
fn route_master(channels: Vec<Vec<f32>>, pairs: [u8; 3], device: usize) -> Vec<Vec<f32>> {
    if pairs == bv_timeline::SAME_PAIRS {
        return channels;
    }
    let frames = channels.first().map_or(0, Vec::len);
    let mut out = vec![vec![0.0f32; frames]; channels.len()];
    for (i, ch) in channels.into_iter().enumerate() {
        let to = pairs.get(i / 2).map_or(i, |&p| usize::from(p) * 2 + i % 2);
        if to >= device {
            continue;
        }
        if to >= out.len() {
            out.resize(to + 1, vec![0.0; frames]);
        }
        for (o, s) in out[to].iter_mut().zip(ch) {
            *o += s;
        }
    }
    out
}

/// Adds what busses played on hardware outputs to their pairs of the
/// device's `device` channels (a pair past them left out), the block made
/// `width` channels wide first.
fn add_hardware(
    channels: &mut Vec<Vec<f32>>,
    hardware: &[(u8, bv_core::AudioBuffer)],
    device: usize,
    width: usize,
) {
    let frames = channels.first().map_or(0, Vec::len);
    while channels.len() < width {
        channels.push(vec![0.0; frames]);
    }
    for (pair, buf) in hardware {
        let first = usize::from(*pair) * 2;
        if first + 2 > device {
            continue;
        }
        for (dst, src) in channels[first..first + 2].iter_mut().zip(&buf.channels) {
            for (o, s) in dst.iter_mut().zip(src) {
                *o += s;
            }
        }
    }
}

/// Primeira amostra cujo início é ≥ `t`.
fn first_sample(rate: SampleRate, t: Tick) -> i64 {
    let s = rate.sample_at(t);
    if rate.sample_start(s) < t { s + 1 } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bus on the device's second pair plays on its channels 3 and 4,
    /// the stereo program staying on 1 and 2; a pair the device lacks is
    /// left out.
    #[test]
    fn hardware_outputs_land_on_their_pairs() {
        let rate = SampleRate(48_000);
        let mut bus = bv_core::AudioBuffer::silent(rate, bv_core::ChannelLayout::Stereo, 4);
        bus.channels[0].fill(0.5);
        bus.channels[1].fill(0.25);
        let mut channels = vec![vec![1.0; 4], vec![1.0; 4]];
        add_hardware(&mut channels, &[(1, bus.clone()), (3, bus)], 4, 4);
        assert_eq!(channels.len(), 4);
        assert_eq!(channels[0], vec![1.0; 4]);
        assert_eq!(channels[2], vec![0.5; 4]);
        assert_eq!(channels[3], vec![0.25; 4]);
    }

    /// The Audio Device Selector: a 5.1 Master's rear pair on the device's
    /// first pair with the front, its center and LFE on the third; a pair
    /// past the device left out.
    #[test]
    fn the_masters_pairs_land_where_the_selector_says() {
        let channels: Vec<Vec<f32>> = (1..=6).map(|v| vec![v as f32; 2]).collect();
        let out = route_master(channels.clone(), bv_timeline::SAME_PAIRS, 6);
        assert_eq!(out, channels);
        let out = route_master(channels.clone(), [0, 2, 0], 6);
        assert_eq!(out[0], vec![6.0; 2]);
        assert_eq!(out[1], vec![8.0; 2]);
        assert_eq!(out[2], vec![0.0; 2]);
        assert_eq!(out[4], vec![3.0; 2]);
        assert_eq!(out[5], vec![4.0; 2]);
        let out = route_master(channels, [0, 2, 1], 4);
        assert_eq!(out[2], vec![5.0; 2]);
        assert_eq!(out[4], vec![0.0; 2]);
    }
}
