//! Miniaturas e formas de onda, calculadas em segundo plano e guardadas em cache.
//!
//! A interface pede o que precisa a cada quadro; se ainda não estiver pronto,
//! recebe `None` e o pedido entra na fila. Quando fica pronto, a interface é
//! avisada para redesenhar.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use bv_core::{Tick, TransferCharacteristic};
use bv_media::{AudioReader, VideoReader, open_audio_stream, open_video};
use bv_timeline::{Generator, MediaInfo, MediaSource};

use crate::RepaintFn;

/// Imagem pequena em RGBA 8 bits.
#[derive(Debug)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Picos (mínimo, máximo) por canal, `per_second` vezes por segundo.
#[derive(Debug)]
pub struct Waveform {
    pub per_second: u32,
    pub peaks: Vec<Vec<(f32, f32)>>,
    /// A forma se repete indefinidamente (mídia gerada).
    pub repeats: bool,
}

impl Waveform {
    /// Pico (mín, máx) do canal `ch` no intervalo de tempo da mídia.
    pub fn range(&self, ch: usize, from: Tick, to: Tick) -> (f32, f32) {
        let Some(p) = self.peaks.get(ch).filter(|p| !p.is_empty()) else {
            return (0.0, 0.0);
        };
        let idx = |t: Tick| (t.as_seconds_f64() * self.per_second as f64).floor() as i64;
        let (a, b) = (idx(from), idx(to).max(idx(from) + 1));
        let mut lo = 0.0f32;
        let mut hi = 0.0f32;
        for i in a..b {
            let i = if self.repeats {
                i.rem_euclid(p.len() as i64)
            } else if i < 0 || i >= p.len() as i64 {
                continue;
            } else {
                i
            } as usize;
            lo = lo.min(p[i].0);
            hi = hi.max(p[i].1);
        }
        (lo, hi)
    }
}

/// One second of a media's audio, sample by sample: the waveform when the
/// timeline is zoomed in past the 100-per-second summary.
#[derive(Debug)]
pub struct WaveChunk {
    pub rate: u32,
    /// Media sample of `samples[c][0]`.
    pub start: i64,
    pub samples: Vec<Vec<f32>>,
}

/// Heights thumbnails are made at: a track's own height is drawn from the
/// next one up, so resizing tracks does not make them all again.
const THUMB_HEIGHTS: [u32; 9] = [24, 32, 48, 64, 96, 128, 192, 256, 384];

fn thumb_height(h: u32) -> u32 {
    THUMB_HEIGHTS
        .iter()
        .copied()
        .find(|&q| q >= h)
        .unwrap_or(THUMB_HEIGHTS[THUMB_HEIGHTS.len() - 1])
}

/// Readers kept open on a worker thread between jobs (opening a decoder and
/// seeking is most of a thumbnail's cost), at most `MAX_OPEN` of them.
const MAX_OPEN: usize = 8;

thread_local! {
    static VIDEO_READERS: RefCell<HashMap<String, Box<dyn VideoReader>>> =
        RefCell::new(HashMap::new());
    static AUDIO_READERS: RefCell<HashMap<String, Box<dyn AudioReader>>> =
        RefCell::new(HashMap::new());
}

/// Closes the readers this thread keeps open.
fn close_thread_readers() {
    VIDEO_READERS.with(|cell| cell.borrow_mut().clear());
    AUDIO_READERS.with(|cell| cell.borrow_mut().clear());
}

fn with_video<R>(info: &MediaInfo, f: impl FnOnce(&mut dyn VideoReader) -> R) -> Option<R> {
    VIDEO_READERS.with(|cell| {
        let mut map = cell.borrow_mut();
        let key = media_key(info);
        if !map.contains_key(&key) {
            if map.len() >= MAX_OPEN {
                map.clear();
            }
            map.insert(key.clone(), open_video(info).ok()?);
        }
        map.get_mut(&key).map(|r| f(r.as_mut()))
    })
}

fn with_audio<R>(
    info: &MediaInfo,
    stream: u16,
    f: impl FnOnce(&mut dyn AudioReader) -> R,
) -> Option<R> {
    AUDIO_READERS.with(|cell| {
        let mut map = cell.borrow_mut();
        let key = format!("{}#{stream}", media_key(info));
        if !map.contains_key(&key) {
            if map.len() >= MAX_OPEN {
                map.clear();
            }
            map.insert(key.clone(), open_audio_stream(info, stream).ok()?);
        }
        map.get_mut(&key).map(|r| f(r.as_mut()))
    })
}

/// Fila de trabalho genérica com cache e uma thread própria.
struct Service<K, V> {
    shared: Arc<(Mutex<ServiceState<K, V>>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}

type Job<V> = Box<dyn FnOnce() -> Option<V> + Send>;

struct ServiceState<K, V> {
    queue: VecDeque<(K, Job<V>)>,
    pending: HashSet<K>,
    done: HashMap<K, Option<Arc<V>>>,
    /// Results in the order they were made; past `cap` the oldest go.
    order: VecDeque<K>,
    cap: usize,
    /// Close the files the worker keeps open.
    release: bool,
    quit: bool,
}

impl<K: Eq + Hash + Clone + Send + 'static, V: Send + Sync + 'static> Service<K, V> {
    /// A service keeping at most `cap` results.
    fn new(name: &str, cap: usize, repaint: RepaintFn) -> Service<K, V> {
        let shared: Arc<(Mutex<ServiceState<K, V>>, Condvar)> = Arc::new((
            Mutex::new(ServiceState {
                queue: VecDeque::new(),
                pending: HashSet::new(),
                done: HashMap::new(),
                order: VecDeque::new(),
                cap,
                release: false,
                quit: false,
            }),
            Condvar::new(),
        ));
        let worker = shared.clone();
        let thread = std::thread::Builder::new()
            .name(name.into())
            // Media generated by plug-ins are drawn on it.
            .stack_size(bv_ofx::PLUGIN_STACK)
            .spawn(move || {
                // Thumbnails decode on the processor: many small decoders
                // on the graphics card would fill its memory.
                bv_mf::processor_only_on_this_thread();
                loop {
                    let (key, job) = {
                        let (lock, cv) = &*worker;
                        let mut st = lock.lock().expect("fila");
                        loop {
                            if st.quit {
                                drop(st);
                                // Close the decoders now: Media Foundation
                                // objects released while the thread itself
                                // ends can hang.
                                close_thread_readers();
                                return;
                            }
                            if std::mem::take(&mut st.release) {
                                drop(st);
                                close_thread_readers();
                                st = lock.lock().expect("fila");
                                continue;
                            }
                            if let Some(j) = st.queue.pop_back() {
                                break j;
                            }
                            st = cv.wait(st).expect("fila");
                        }
                    };
                    let value = job().map(Arc::new);
                    let mut st = worker.0.lock().expect("fila");
                    st.pending.remove(&key);
                    st.order.push_back(key.clone());
                    st.done.insert(key, value);
                    while st.order.len() > st.cap {
                        if let Some(old) = st.order.pop_front() {
                            st.done.remove(&old);
                        }
                    }
                    drop(st);
                    repaint();
                }
            })
            .expect("não foi possível criar a thread de miniaturas");
        Service {
            shared,
            thread: Some(thread),
        }
    }

    /// Resultado pronto, ou enfileira o cálculo. Pedidos recentes vêm primeiro.
    fn get(&self, key: K, job: impl FnOnce() -> Option<V> + Send + 'static) -> Option<Arc<V>> {
        let (lock, cv) = &*self.shared;
        let mut st = lock.lock().expect("fila");
        if let Some(v) = st.done.get(&key) {
            return v.clone();
        }
        if st.pending.insert(key.clone()) {
            st.queue.push_back((key, Box::new(job)));
            // Limita a fila: pedidos antigos (fora da tela) são descartados.
            while st.queue.len() > 256 {
                if let Some((k, _)) = st.queue.pop_front() {
                    st.pending.remove(&k);
                }
            }
            cv.notify_one();
        }
        None
    }

    fn clear(&self) {
        let mut st = self.shared.0.lock().expect("fila");
        st.done.clear();
        st.order.clear();
    }

    /// Has the worker close the files it keeps open.
    fn release(&self) {
        let (lock, cv) = &*self.shared;
        lock.lock().expect("fila").release = true;
        cv.notify_all();
    }
}

impl<K, V> Drop for Service<K, V> {
    fn drop(&mut self) {
        {
            let (lock, cv) = &*self.shared;
            lock.lock().expect("fila").quit = true;
            cv.notify_all();
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Chave de mídia que muda quando o conteúdo muda (arquivo ou parâmetros do gerador).
fn media_key(info: &MediaInfo) -> String {
    // Read otherwise (Media Properties' Rotation, Alpha channel…): other
    // thumbnails.
    let reading = info
        .video
        .map(|v| v.reading)
        .filter(|r| *r != Default::default())
        .map_or_else(String::new, |r| format!("~{r:?}"));
    match &info.source {
        // A Photoshop document's layer is a picture of its own.
        MediaSource::File { path } if info.layer.is_some() => {
            format!("f:{path}#{}{reading}", info.layer.unwrap_or(0))
        }
        MediaSource::File { path } => format!("f:{path}{reading}"),
        MediaSource::Generated(g) => format!("g:{g:?}"),
    }
}

/// Thumbnails already shown, by media and height and then by second: while
/// one is being made, the nearest of these stands in (no blank frames when
/// zooming or scrolling).
type Shown = HashMap<(String, u32), BTreeMap<i64, Arc<Thumbnail>>>;

pub(crate) struct Thumbs {
    images: Service<(String, i64, u32), Thumbnail>,
    waves: Service<String, Waveform>,
    chunks: Service<(String, i64), WaveChunk>,
    shown: Mutex<Shown>,
}

impl Thumbs {
    pub(crate) fn new(repaint: RepaintFn) -> Thumbs {
        Thumbs {
            images: Service::new("bv-thumbs", 4096, repaint.clone()),
            waves: Service::new("bv-waveforms", usize::MAX, repaint.clone()),
            chunks: Service::new("bv-wave-detail", 128, repaint),
            shown: Mutex::new(HashMap::new()),
        }
    }

    /// Closes the media files the workers keep open.
    pub(crate) fn release(&self) {
        self.images.release();
        self.waves.release();
        self.chunks.release();
    }

    pub(crate) fn clear(&self) {
        self.images.clear();
        self.waves.clear();
        self.chunks.clear();
        self.shown.lock().expect("miniaturas").clear();
    }

    /// One second (`index`) of the media's audio stream, sample by sample.
    pub(crate) fn wave_chunk(
        &self,
        info: &MediaInfo,
        stream: u16,
        index: i64,
    ) -> Option<Arc<WaveChunk>> {
        info.audio_stream(stream)?;
        let info = info.clone();
        self.chunks.get(
            (format!("{}#{stream}", media_key(&info)), index),
            move || {
                with_audio(&info, stream, |r| {
                    let rate = r.sample_rate().0;
                    let start = index * rate as i64;
                    let buf = r.read(start, rate as usize).ok()?;
                    Some(WaveChunk {
                        rate,
                        start,
                        samples: buf.channels,
                    })
                })
                .flatten()
            },
        )
    }

    /// Miniatura do quadro `t` da mídia, com `height` pixels de altura.
    pub(crate) fn thumbnail(
        &self,
        info: &MediaInfo,
        t: Tick,
        height: u32,
    ) -> Option<Arc<Thumbnail>> {
        let v = info.video?;
        // Nearby frames share a thumbnail (one a second); stills and the
        // generators that never change have one (a plug-in's — a credit
        // roll scrolling — and noise change).
        let constant = info.still
            || matches!(&info.source, MediaSource::Generated(g)
                if !matches!(g, Generator::Plugin(_) | Generator::Noise { .. }));
        let bucket = if constant { 0 } else { t.0 / Tick::SECOND.0 };
        let height = thumb_height(height);
        let key = media_key(info);
        let info = info.clone();
        let exact = self.images.get((key.clone(), bucket, height), move || {
            let h = height.max(2);
            let par = v.pixel_aspect.ratio();
            let w = ((v.width as f32 * par / v.height as f32) * h as f32)
                .round()
                .max(2.0) as u32;
            let img = with_video(&info, |r| {
                r.frame_at_scaled(Tick(bucket * Tick::SECOND.0), w, h)
            })?
            .ok()?;
            let small = bv_render::place_par(
                &img,
                w,
                h,
                &bv_timeline::Transform::default(),
                bv_timeline::RenderQuality::Good,
                par,
            );
            Some(Thumbnail {
                width: w,
                height: h,
                rgba: bv_color::linear_to_display_rgba8(&small, TransferCharacteristic::Bt709),
            })
        });
        let mut shown = self.shown.lock().expect("miniaturas");
        let seen = shown.entry((key, height)).or_default();
        match exact {
            Some(t) => {
                seen.insert(bucket, t.clone());
                Some(t)
            }
            // The nearest one shown before, until this one is ready.
            None => {
                let before = seen.range(..=bucket).next_back();
                let after = seen.range(bucket..).next();
                match (before, after) {
                    (Some((b, tb)), Some((a, ta))) => {
                        Some(if bucket - b <= a - bucket { tb } else { ta }.clone())
                    }
                    (Some((_, t)), None) | (None, Some((_, t))) => Some(t.clone()),
                    (None, None) => None,
                }
            }
        }
    }

    /// Waveform of the media's audio stream `stream` (0 = the first).
    pub(crate) fn waveform(&self, info: &MediaInfo, stream: u16) -> Option<Arc<Waveform>> {
        info.audio_stream(stream)?;
        let info = info.clone();
        self.waves
            .get(format!("{}#{stream}", media_key(&info)), move || {
                compute_waveform(&info, stream)
            })
    }
}

/// BananaV's peak files: a media file's waveform kept next to it
/// (`interview.wav.bvpk`), so that it shows at once the next time (Options ›
/// Preferences › General: "Do not create .bvpk files to store peak data",
/// "Hide new .bvpk files").
pub(crate) mod peak_files {
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU8, Ordering};

    use super::Waveform;

    const MAGIC: &[u8; 4] = b"BVPK";
    const VERSION: u32 = 1;
    const WRITE: u8 = 1;
    const HIDE: u8 = 2;
    static FLAGS: AtomicU8 = AtomicU8::new(WRITE);

    /// Whether new peak files are written, and hidden.
    pub(crate) fn set(write: bool, hide: bool) {
        FLAGS.store(
            if write { WRITE } else { 0 } | if hide { HIDE } else { 0 },
            Ordering::Relaxed,
        );
    }

    /// The peak file of a media file's audio stream.
    pub(crate) fn path(media: &str, stream: u16) -> PathBuf {
        match stream {
            0 => PathBuf::from(format!("{media}.bvpk")),
            s => PathBuf::from(format!("{media}.{s}.bvpk")),
        }
    }

    /// The media file's size and last change (a peak file of another
    /// version of it is not used).
    fn stamp(media: &str) -> Option<(u64, u64)> {
        let meta = std::fs::metadata(media).ok()?;
        let changed = meta
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos() as u64;
        Some((meta.len(), changed))
    }

    pub(crate) fn read(media: &str, stream: u16) -> Option<Waveform> {
        let (len, changed) = stamp(media)?;
        let mut data = Vec::new();
        std::fs::File::open(path(media, stream))
            .ok()?
            .read_to_end(&mut data)
            .ok()?;
        let u32_at = |i: usize| -> Option<u32> {
            Some(u32::from_le_bytes(data.get(i..i + 4)?.try_into().ok()?))
        };
        let u64_at = |i: usize| -> Option<u64> {
            Some(u64::from_le_bytes(data.get(i..i + 8)?.try_into().ok()?))
        };
        if data.get(..4)? != MAGIC || u32_at(4)? != VERSION {
            return None;
        }
        if (u64_at(8)?, u64_at(16)?) != (len, changed) {
            return None;
        }
        let per_second = u32_at(24)?;
        let channels = u32_at(28)? as usize;
        let buckets = u32_at(32)? as usize;
        let body = data.get(36..)?;
        if body.len() != channels * buckets * 4 {
            return None;
        }
        let sample = |b: &[u8]| i16::from_le_bytes([b[0], b[1]]) as f32 / 32767.0;
        let peaks = body
            .chunks(buckets * 4)
            .map(|ch| {
                ch.chunks(4)
                    .map(|b| (sample(&b[..2]), sample(&b[2..])))
                    .collect()
            })
            .collect();
        Some(Waveform {
            per_second,
            peaks,
            repeats: false,
        })
    }

    /// Media whose other editor's peak file is passed over: Rebuild Audio
    /// Peaks makes BananaV's own.
    static PASSED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

    /// A DOS date and time (`time << 16 | date`, two seconds apart) as
    /// seconds since 1970.
    pub(crate) fn dos_seconds(v: u32) -> Option<i64> {
        let (time, date) = (i64::from(v >> 16), i64::from(v & 0xFFFF));
        let (year, month, day) = (1980 + (date >> 9), (date >> 5) & 0xF, date & 0x1F);
        if !(1..=12).contains(&month) || day == 0 {
            return None;
        }
        // Days since 1970 of the Gregorian date.
        let y = if month <= 2 { year - 1 } else { year };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
        let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
        let (h, m, sec) = (time >> 11, (time >> 5) & 0x3F, (time & 0x1F) * 2);
        Some(days * 86_400 + h * 3600 + m * 60 + sec)
    }

    /// The peak file another editor leaves beside a media file (`tom.wav.sfk`
    /// or `tom.sfk` for `tom.wav`), as BananaV's waveform of `per_second` peaks a
    /// second. Its header of 64 bytes: `SFPK`, version 1, the header's
    /// size, the media's last change in local time as DOS writes it, the
    /// bits of each value (8 or 16), the channels, the samples each peak
    /// covers and the media's samples per channel, then its last change
    /// again in UTC; after it, block by block and channel by channel, the
    /// minimum and the maximum. A file made from another version of the
    /// media, or of other channels, is not used.
    pub(crate) fn read_sfk(
        media: &str,
        rate: u32,
        channels: usize,
        per_second: u32,
    ) -> Option<Waveform> {
        if PASSED.lock().ok()?.iter().any(|m| m == media) || rate == 0 || per_second == 0 {
            return None;
        }
        let data = std::fs::read(format!("{media}.sfk"))
            .or_else(|_| std::fs::read(std::path::Path::new(media).with_extension("sfk")))
            .ok()?;
        let u32_at = |i: usize| -> Option<u32> {
            Some(u32::from_le_bytes(data.get(i..i + 4)?.try_into().ok()?))
        };
        let u64_at = |i: usize| -> Option<u64> {
            Some(u64::from_le_bytes(data.get(i..i + 8)?.try_into().ok()?))
        };
        if data.get(..4)? != b"SFPK" || u32_at(4)? != 1 {
            return None;
        }
        let changed = std::fs::metadata(media)
            .ok()?
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs() as i64;
        if (dos_seconds(u32_at(0x24)?)? - changed).abs() > 2 {
            return None;
        }
        let header = u32_at(8)? as usize;
        let width = match u32_at(0x10)? {
            8 => 1,
            16 => 2,
            _ => return None,
        };
        let per_peak = u64::from(u32_at(0x18)?);
        let samples = u64_at(0x1C)?;
        if u32_at(0x14)? as usize != channels || channels == 0 || per_peak == 0 {
            return None;
        }
        let blocks = samples.div_ceil(per_peak) as usize;
        let body = data.get(header..header.checked_add(blocks * channels * 2 * width)?)?;
        let value = |i: usize| match width {
            1 => f32::from(body[i] as i8) / 128.0,
            _ => f32::from(i16::from_le_bytes([body[i], body[i + 1]])) / 32768.0,
        };
        let rate = u64::from(rate);
        let per_second = u64::from(per_second);
        let buckets = (samples * per_second).div_ceil(rate) as usize;
        let mut peaks = vec![Vec::with_capacity(buckets); channels];
        for k in 0..buckets as u64 {
            let from = k * rate / per_second;
            let to = ((k + 1) * rate / per_second).clamp(from + 1, samples.max(from + 1));
            let (b0, b1) = (
                (from / per_peak) as usize,
                (to.div_ceil(per_peak) as usize).min(blocks),
            );
            for (c, p) in peaks.iter_mut().enumerate() {
                let (mut lo, mut hi) = (0.0f32, 0.0f32);
                for b in b0..b1 {
                    let at = (b * channels + c) * 2 * width;
                    lo = lo.min(value(at));
                    hi = hi.max(value(at + width));
                }
                p.push((lo, hi));
            }
        }
        Some(Waveform {
            per_second: per_second as u32,
            peaks,
            repeats: false,
        })
    }

    /// Writes the peak file, unless the preferences say not to (or the
    /// folder takes no files: the waveform then lives in memory only).
    pub(crate) fn write(media: &str, stream: u16, w: &Waveform) {
        let flags = FLAGS.load(Ordering::Relaxed);
        if flags & WRITE == 0 {
            return;
        }
        let Some((len, changed)) = stamp(media) else {
            return;
        };
        let buckets = w.peaks.iter().map(Vec::len).max().unwrap_or(0);
        let mut data = Vec::with_capacity(36 + w.peaks.len() * buckets * 4);
        data.extend_from_slice(MAGIC);
        data.extend_from_slice(&VERSION.to_le_bytes());
        data.extend_from_slice(&len.to_le_bytes());
        data.extend_from_slice(&changed.to_le_bytes());
        data.extend_from_slice(&w.per_second.to_le_bytes());
        data.extend_from_slice(&(w.peaks.len() as u32).to_le_bytes());
        data.extend_from_slice(&(buckets as u32).to_le_bytes());
        let to16 = |v: f32| ((v.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes();
        for ch in &w.peaks {
            for i in 0..buckets {
                let (lo, hi) = ch.get(i).copied().unwrap_or((0.0, 0.0));
                data.extend_from_slice(&to16(lo));
                data.extend_from_slice(&to16(hi));
            }
        }
        let target = path(media, stream);
        let _ = std::fs::remove_file(&target);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(windows)]
        if flags & HIDE != 0 {
            use std::os::windows::fs::OpenOptionsExt;
            // FILE_ATTRIBUTE_HIDDEN.
            options.attributes(0x2);
        }
        if let Ok(mut f) = options.open(&target) {
            let _ = f.write_all(&data);
        }
    }

    /// Deletes a media file's peak files (they are made anew, another
    /// editor's beside it passed over).
    pub(crate) fn delete(media: &str) {
        for stream in 0..16 {
            let _ = std::fs::remove_file(path(media, stream));
        }
        if let Ok(mut passed) = PASSED.lock()
            && !passed.iter().any(|m| m == media)
        {
            passed.push(media.to_string());
        }
    }
}

fn compute_waveform(info: &MediaInfo, stream: u16) -> Option<Waveform> {
    const PER_SECOND: u32 = 100;
    let file = match &info.source {
        MediaSource::File { path } if info.sequence.is_none() => Some(path.as_str()),
        _ => None,
    };
    if let Some(w) = file.and_then(|f| peak_files::read(f, stream)) {
        return Some(w);
    }
    // The first stream's peaks another editor left beside the file.
    if let (Some(f), Some(a), 0) = (file, info.audio.as_ref(), stream)
        && let Some(w) = peak_files::read_sfk(f, a.sample_rate.0, a.channels.into(), PER_SECOND)
    {
        return Some(w);
    }
    let w = compute_peaks(info, stream, PER_SECOND)?;
    if let Some(f) = file {
        peak_files::write(f, stream, &w);
    }
    Some(w)
}

fn compute_peaks(info: &MediaInfo, stream: u16, per_second: u32) -> Option<Waveform> {
    const PER_SECOND: u32 = 100;
    debug_assert_eq!(per_second, PER_SECOND);
    let mut r = open_audio_stream(info, stream).ok()?;
    let rate = r.sample_rate().0 as usize;
    let generated = matches!(info.source, MediaSource::Generated(_));
    // Mídia gerada é periódica: 2 s bastam. Arquivos: até 6 h.
    let seconds = if generated {
        2.0
    } else {
        info.duration.as_seconds_f64().min(6.0 * 3600.0)
    };
    let per_bucket = (rate / PER_SECOND as usize).max(1);
    let buckets = (seconds * PER_SECOND as f64).ceil() as usize;
    let mut peaks = vec![Vec::with_capacity(buckets); r.channels()];
    let block = per_bucket * 100;
    let mut pos = 0i64;
    for _ in 0..buckets.div_ceil(100) {
        let buf = r.read(pos, block).ok()?;
        for (c, ch) in buf.channels.iter().enumerate() {
            for chunk in ch.chunks(per_bucket) {
                if peaks[c].len() >= buckets {
                    break;
                }
                let lo = chunk.iter().copied().fold(0.0f32, f32::min);
                let hi = chunk.iter().copied().fold(0.0f32, f32::max);
                peaks[c].push((lo, hi));
            }
        }
        pos += block as i64;
    }
    Some(Waveform {
        per_second: PER_SECOND,
        peaks,
        repeats: generated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_files_come_back_until_the_media_changes() {
        let dir = std::env::temp_dir().join(format!("bananav-peaks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let media = dir.join("voice.wav");
        std::fs::write(&media, [1u8; 64]).unwrap();
        let media = media.to_string_lossy().to_string();
        let w = Waveform {
            per_second: 100,
            peaks: vec![
                vec![(-0.5, 0.25), (0.0, 1.0)],
                vec![(-1.0, 0.0), (-0.25, 0.5)],
            ],
            repeats: false,
        };
        peak_files::set(true, false);
        peak_files::write(&media, 0, &w);
        let back = peak_files::read(&media, 0).expect("read back");
        assert_eq!(back.per_second, 100);
        for (a, b) in back.peaks.iter().flatten().zip(w.peaks.iter().flatten()) {
            assert!((a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4);
        }
        // Another version of the media: the peak file is not used.
        std::fs::write(&media, [2u8; 65]).unwrap();
        assert!(peak_files::read(&media, 0).is_none());
        // Not written when the preferences say not to.
        peak_files::delete(&media);
        peak_files::set(false, false);
        peak_files::write(&media, 0, &w);
        assert!(!peak_files::path(&media, 0).exists());
        peak_files::set(true, false);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dos_stamps_read_as_seconds_since_1970() {
        // 2026-11-03 14:53:26, as the other editor wrote it in UTC.
        assert_eq!(peak_files::dos_seconds(0x76AD_5D63), Some(1_793_717_606));
        // 1980-01-01 00:00:00, DOS's first day.
        assert_eq!(peak_files::dos_seconds(0x0000_0021), Some(315_532_800));
        assert_eq!(peak_files::dos_seconds(0), None, "no month");
    }

    #[test]
    fn another_editors_peaks_are_read_beside_the_media() {
        let dir = std::env::temp_dir().join(format!("bananav-sfk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let media = dir.join("tone.wav");
        std::fs::write(&media, [0u8; 64]).unwrap();
        // The media changed at 2026-11-03 14:53:26 UTC.
        let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_793_717_606);
        std::fs::File::options()
            .write(true)
            .open(&media)
            .unwrap()
            .set_modified(when)
            .unwrap();
        // One second of 48 kHz stereo, a peak every 256 samples: 188 blocks,
        // block b reaching -10 b and 10 b + the channel.
        let sfk = |stamp: u32, channels: u32| {
            let mut d = Vec::new();
            d.extend_from_slice(b"SFPK");
            for v in [1u32, 0x40, 0x5EAD_5D63, 16, channels, 256] {
                d.extend_from_slice(&v.to_le_bytes());
            }
            d.extend_from_slice(&48_000u64.to_le_bytes());
            d.extend_from_slice(&stamp.to_le_bytes());
            d.resize(0x40, 0);
            for b in 0..188i16 {
                for c in 0..channels as i16 {
                    d.extend_from_slice(&(-10 * b).to_le_bytes());
                    d.extend_from_slice(&(10 * b + c).to_le_bytes());
                }
            }
            d
        };
        let path = dir.join("tone.sfk");
        std::fs::write(&path, sfk(0x76AD_5D63, 2)).unwrap();
        let name = media.to_string_lossy().to_string();
        let w = peak_files::read_sfk(&name, 48_000, 2, 100).expect("read");
        assert_eq!(
            (w.per_second, w.peaks.len(), w.peaks[0].len()),
            (100, 2, 100)
        );
        let v = |n: i16| f32::from(n) / 32768.0;
        // The first hundredth (480 samples) spans blocks 0 and 1.
        assert_eq!(w.peaks[0][0], (v(-10), v(10)));
        assert_eq!(w.peaks[1][0], (v(-10), v(11)));
        // The last ends with block 187.
        assert_eq!(w.peaks[0][99], (v(-1870), v(1870)));
        // Named after the whole file, as the other editor names them too.
        std::fs::remove_file(&path).unwrap();
        let whole = dir.join("tone.wav.sfk");
        std::fs::write(&whole, sfk(0x76AD_5D63, 2)).unwrap();
        assert!(peak_files::read_sfk(&name, 48_000, 2, 100).is_some());
        std::fs::remove_file(&whole).unwrap();
        std::fs::write(&path, sfk(0x76AD_5D63, 2)).unwrap();
        // Another channel count, or another version of the media: not used.
        assert!(peak_files::read_sfk(&name, 48_000, 1, 100).is_none());
        std::fs::write(&path, sfk(0x76AD_5D73, 2)).unwrap();
        assert!(peak_files::read_sfk(&name, 48_000, 2, 100).is_none());
        // Rebuild Audio Peaks passes it over.
        std::fs::write(&path, sfk(0x76AD_5D63, 2)).unwrap();
        peak_files::delete(&name);
        assert!(peak_files::read_sfk(&name, 48_000, 2, 100).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `BV_SFK_SAMPLE`: a folder with `tom.wav` and the `tom.sfk` another
    /// editor made of it; their peaks agree.
    #[test]
    #[ignore]
    fn another_editors_peaks_match_the_sound() {
        let dir = std::path::PathBuf::from(std::env::var("BV_SFK_SAMPLE").expect("BV_SFK_SAMPLE"));
        let wav = dir.join("tom.wav").to_string_lossy().to_string();
        let info = crate::Engine::probe_file(&wav).expect("probe");
        let a = info.audio.expect("audio");
        let from_sfk =
            peak_files::read_sfk(&wav, a.sample_rate.0, a.channels.into(), 100).expect("sfk");
        let computed = compute_peaks(&info, 0, 100).expect("peaks");
        assert_eq!(from_sfk.peaks.len(), computed.peaks.len());
        for (s, c) in from_sfk.peaks.iter().zip(&computed.peaks) {
            assert_eq!(s.len(), c.len());
            for (x, y) in s.iter().zip(c) {
                // The blocks a hundredth spans reach a little past it.
                assert!(x.0 <= y.0 + 1e-4 && x.1 >= y.1 - 1e-4, "{x:?} {y:?}");
                assert!(
                    (x.0 - y.0).abs() < 0.05 && (x.1 - y.1).abs() < 0.05,
                    "{x:?} {y:?}"
                );
            }
        }
    }

    #[test]
    fn thumbnail_heights_come_in_steps() {
        assert_eq!(thumb_height(10), 24);
        assert_eq!(thumb_height(48), 48);
        assert_eq!(thumb_height(50), 64);
        assert_eq!(thumb_height(1000), 384);
    }

    #[test]
    fn service_keeps_at_most_its_cap() {
        let s: Service<u32, u32> = Service::new("test", 2, Arc::new(|| {}));
        for k in 0..3u32 {
            s.get(k, move || Some(k));
            // Wait for the worker.
            for _ in 0..200 {
                if s.get(k, move || Some(k)).is_some() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        let st = s.shared.0.lock().unwrap();
        assert_eq!(st.done.len(), 2);
        assert!(!st.done.contains_key(&0), "the oldest went");
    }

    #[test]
    fn wave_chunks_hold_one_second_of_samples() {
        let thumbs = Thumbs::new(Arc::new(|| {}));
        let mut p = bv_timeline::Project::default();
        let info = MediaInfo::generated(
            bv_timeline::Generator::TestTone {
                frequency_hz: 1000.0,
                level_db: 0.0,
            },
            &p.settings,
        );
        let id = p.add_media(info.clone());
        let info = p.media(id).unwrap().clone();
        let mut chunk = None;
        for _ in 0..200 {
            chunk = thumbs.wave_chunk(&info, 0, 2);
            if chunk.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let chunk = chunk.expect("the chunk is read");
        assert_eq!(chunk.rate, 48_000);
        assert_eq!(chunk.start, 96_000);
        assert_eq!(chunk.samples[0].len(), 48_000);
        let peak = chunk.samples[0].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.9, "{peak}");
    }
}
