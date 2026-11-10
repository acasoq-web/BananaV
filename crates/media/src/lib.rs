//! Fontes de mídia: transforma uma [`MediaInfo`] do projeto em leitores de
//! quadros (em luz linear) e de amostras de áudio.

mod files;
mod generator;
mod markers;
mod nested;
#[cfg(not(target_arch = "wasm32"))]
pub mod plugins;
mod pool;
mod reading;
mod stamps;

pub use bv_mp4::facts::TrackFacts;
pub use files::{
    AUDIO_EXTENSIONS, FileDescription, FileReader, IMAGE_EXTENSIONS, VIDEO_EXTENSIONS,
    describe_file,
};
pub use markers::{MarkerStore, can_save_markers, markers_path, read_markers, save_markers};
pub use nested::{
    NestedReaders, PROJECT_EXTENSION, is_project, nested_projects, nests_itself, read_project,
    same_file, set_nested_readers,
};
pub use pool::{
    Ask, NO_EVENT, Need, Onward, Owner, Picture, VideoPool, Want, blend_owner, needs_of, reduction,
};
pub use stamps::{MediaStamps, stamps};

use std::sync::Arc;

use bv_core::{AudioBuffer, ImageRgbaF32, SampleRate, Tick};
use bv_timeline::{MediaInfo, MediaSource};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaError {
    /// Formato, codec ou recurso ainda não suportado.
    Unsupported(String),
    /// A codec nothing here decodes: its name ("DNxHR"), and whether it is
    /// the video's (else the audio's).
    UnsupportedCodec {
        codec: String,
        video: bool,
    },
    /// A kind of file nothing here opens: its extension ("mxf").
    UnsupportedFile(String),
    Io(String),
    /// Dados corrompidos ou fora da especificação.
    Corrupt(String),
    /// A mídia não tem esse tipo de stream.
    NoStream,
    /// The reading was stopped: another picture is wanted now.
    Interrupted,
    /// A project nested in itself, at some depth (it would draw itself
    /// forever).
    NestedInItself,
}

impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaError::Unsupported(s) => write!(f, "unsupported: {s}"),
            MediaError::UnsupportedCodec { codec, video: true } => {
                write!(f, "{codec} video is not supported")
            }
            MediaError::UnsupportedCodec {
                codec,
                video: false,
            } => {
                write!(f, "{codec} audio is not supported")
            }
            MediaError::UnsupportedFile(ext) => write!(f, ".{ext} files are not supported"),
            MediaError::Io(s) => write!(f, "read error: {s}"),
            MediaError::Corrupt(s) => write!(f, "corrupt file: {s}"),
            MediaError::NoStream => f.write_str("the media has no such stream"),
            MediaError::Interrupted => f.write_str("interrupted"),
            MediaError::NestedInItself => f.write_str("the project is nested in itself"),
        }
    }
}

impl std::error::Error for MediaError {}

pub type Result<T> = std::result::Result<T, MediaError>;

/// Lê quadros de vídeo de uma mídia.
pub trait VideoReader: Send {
    /// O quadro visível no instante `t` da mídia, em RGB linear BT.709.
    fn frame_at(&mut self, t: Tick) -> Result<Arc<ImageRgbaF32>>;

    /// Como [`frame_at`](Self::frame_at), mas o leitor pode entregar uma
    /// versão reduzida que ainda cubra `min_w` × `min_h` (prévia).
    fn frame_at_scaled(&mut self, t: Tick, min_w: u32, min_h: u32) -> Result<Arc<ImageRgbaF32>> {
        let _ = (min_w, min_h);
        self.frame_at(t)
    }

    /// Tells the reader how long the event reading it is (`None` outside
    /// events): generated media whose pictures depend on it — a credit
    /// roll scrolls over it — hear it before each picture.
    fn set_span(&mut self, span: Option<Tick>) {
        let _ = span;
    }

    /// Tells the reader whether the project's pixel format is a video
    /// levels one: Y′CbCr pictures then come in as studio RGB
    /// (`bv_color::frame_to_linear_levels`).
    fn set_video_levels(&mut self, on: bool) {
        let _ = on;
    }

    /// Tells the reader what the project's pictures are: 8 bits a channel
    /// (`eight_bit`), composited on the encoded values (`encoded`, video
    /// gamma). Generated media follow it: a generator draws as an 8-bit
    /// editor's does there, in the values the project composites on.
    fn set_project(&mut self, eight_bit: bool, encoded: bool) {
        let _ = (eight_bit, encoded);
    }

    /// Tells the reader how to read its pictures' alpha channel (Media
    /// Properties' Alpha channel and, for Premultiplied (dirty), the
    /// Background color): readers of pictures with alpha follow it.
    fn set_alpha(&mut self, alpha: bv_timeline::AlphaChannel, background: [u8; 3]) {
        let _ = (alpha, background);
    }

    /// The picture at `t` as decoded, when the reader decodes pictures that
    /// still need converting (the graphics card converts them); otherwise
    /// as [`frame_at_scaled`](Self::frame_at_scaled).
    fn picture_at(&mut self, t: Tick, min_w: u32, min_h: u32) -> Result<Picture> {
        self.frame_at_scaled(t, min_w, min_h).map(Picture::Linear)
    }

    /// The picture at `t` as `want`, telling `on` about the way there: a
    /// reader that must decode many pictures to get to it (a seek) calls
    /// `on` as it goes — with a picture on the way now and then, `None`
    /// otherwise — and follows its answer: go on, show the next picture
    /// on the way at once and stop, or stop ([`MediaError::Interrupted`]).
    fn picture_on_the_way(
        &mut self,
        t: Tick,
        min_w: u32,
        min_h: u32,
        want: Want,
        on: &mut dyn FnMut(Option<Picture>) -> Onward,
    ) -> Result<Picture> {
        let _ = on;
        match want {
            Want::Linear => self.frame_at_scaled(t, min_w, min_h).map(Picture::Linear),
            Want::Decoded => self.picture_at(t, min_w, min_h),
        }
    }
}

/// Lê amostras de áudio de uma mídia, na taxa nativa dela.
pub trait AudioReader: Send {
    fn sample_rate(&self) -> SampleRate;
    fn channels(&self) -> usize;
    /// Lê `frames` amostras a partir da amostra `start`. Trechos antes do
    /// início ou depois do fim da mídia voltam como silêncio.
    fn read(&mut self, start: i64, frames: usize) -> Result<AudioBuffer>;
}

/// Lê o cabeçalho de um arquivo e descreve seus streams, sem decodificar.
pub fn probe_file(path: &str) -> Result<MediaInfo> {
    // A project file: nested in the project (its markers are its own).
    if nested::is_project(path) {
        return nested::probe(path);
    }
    let mut info = files::probe(path)?;
    info.markers = markers::read_markers(path);
    Ok(info)
}

/// Describes `count` numbered pictures from `first` (`shot_0001.png`…) as
/// one video of `rate` frames a second (VEGAS's "Open sequence").
pub fn probe_sequence(first: &str, count: u32, rate: bv_core::FrameRate) -> Result<MediaInfo> {
    files::probe_sequence(first, count, rate)
}

/// How many numbered pictures there are from `first` on without a gap
/// (itself counted; 0 when its name has no number).
pub fn sequence_length(first: &str) -> u32 {
    files::sequence_length(first)
}

/// Start time stored in a Broadcast Wave file (`bext` time reference), as a
/// timeline position. `None` when the file is not a BWF.
pub fn bwf_start(path: &str) -> Option<bv_core::Tick> {
    let mut f = std::io::BufReader::new(std::fs::File::open(path).ok()?);
    let samples = bv_wav::bext_time_reference(&mut f).ok()??;
    let rate = bv_wav::read_info(&mut f).ok()?.sample_rate;
    Some(bv_core::SampleRate(rate).sample_start(samples as i64))
}

/// Media files read from a copy instead: a file on a removable drive is
/// copied to the temporary files folder, so that the project goes on
/// working once the drive is gone (the project keeps the file's own path).
static LOCAL_COPIES: std::sync::Mutex<std::collections::BTreeMap<String, String>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// Reads `original` from `copy` from now on.
pub fn use_local_copy(original: &str, copy: &str) {
    if let Ok(mut m) = LOCAL_COPIES.lock() {
        m.insert(original.to_string(), copy.to_string());
    }
}

/// Reads every file from its own path again.
pub fn forget_local_copies() {
    if let Ok(mut m) = LOCAL_COPIES.lock() {
        m.clear();
    }
}

/// Proxies: smaller copies of large media files that previews read instead
/// (renders always read the files themselves).
static PROXIES: std::sync::Mutex<std::collections::BTreeMap<String, String>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// Previews read `original` from `proxy` from now on.
pub fn use_proxy(original: &str, proxy: &str) {
    if let Ok(mut m) = PROXIES.lock() {
        m.insert(original.to_string(), proxy.to_string());
    }
}

/// Previews read `original` itself again.
pub fn forget_proxy(original: &str) {
    if let Ok(mut m) = PROXIES.lock() {
        m.remove(original);
    }
}

/// The proxy previews read for `original`, if it has one.
pub fn proxy_of(original: &str) -> Option<String> {
    PROXIES.lock().ok().and_then(|m| m.get(original).cloned())
}

/// The media as its proxy is read (the same media, another file), when
/// it has one.
pub fn proxied(info: &MediaInfo) -> Option<MediaInfo> {
    let MediaSource::File { path } = &info.source else {
        return None;
    };
    if info.sequence.is_some() {
        return None;
    }
    let proxy = proxy_of(path)?;
    let mut info = info.clone();
    info.source = MediaSource::File { path: proxy };
    Some(info)
}

/// The path a media file is read from.
fn local(path: &str) -> String {
    LOCAL_COPIES
        .lock()
        .ok()
        .and_then(|m| m.get(path).cloned())
        .unwrap_or_else(|| path.to_string())
}

/// The names of a layered Photoshop document's picture layers, bottom
/// first (none for other files).
pub fn psd_layers(path: &str) -> Vec<String> {
    files::psd_layer_names(&local(path))
}

/// The blend modes' keys (`norm`, `mul `…) of a layered Photoshop
/// document's picture layers, bottom first (none for other files).
pub fn psd_layer_blends(path: &str) -> Vec<[u8; 4]> {
    files::psd_layer_blends(&local(path))
}

pub fn open_video(info: &MediaInfo) -> Result<Box<dyn VideoReader>> {
    let video = info.video.ok_or(MediaError::NoStream)?;
    match &info.source {
        // Drawn at its own size, then turned as its Media Properties say.
        MediaSource::Generated(bv_timeline::Generator::Plugin(fx)) => {
            let r = video.reading;
            let mut own = video;
            if r.rotation.quarters() % 2 == 1 {
                (own.width, own.height) = (video.height, video.width);
            }
            let mut reader = plugin_video(fx, own)?;
            reader.set_alpha(r.alpha, r.background);
            Ok(reading::Reading::around(reader, r))
        }
        MediaSource::Generated(g) => Ok(Box::new(generator::GeneratedVideo::new(
            g.clone(),
            video.width,
            video.height,
            video.frame_rate,
        )?)),
        // A nested project.
        MediaSource::File { path } if nested::is_project(path) => nested::open_video(&local(path)),
        // One layer of a Photoshop document.
        MediaSource::File { path } if info.layer.is_some() => {
            files::open_layer(&local(path), info.layer.unwrap_or(0))
        }
        MediaSource::File { path } => {
            let mut reader: Box<dyn VideoReader> = match info.sequence {
                Some(seq) => Box::new(files::SequenceVideo::new(path, seq.count, video.frame_rate)),
                None => files::open_video(&local(path))?,
            };
            // As its Media Properties say: its alpha, Limited, Rotation.
            let r = video.reading;
            reader.set_alpha(r.alpha, r.background);
            Ok(reading::Reading::around(reader, r))
        }
    }
}

/// Media an OFX generator draws.
#[cfg(not(target_arch = "wasm32"))]
fn plugin_video(
    fx: &bv_timeline::PluginFx,
    v: bv_timeline::VideoStreamInfo,
) -> Result<Box<dyn VideoReader>> {
    let aspect = f64::from(v.pixel_aspect.num) / f64::from(v.pixel_aspect.den.max(1));
    let reader = plugins::PluginVideo::new(fx, v.width, v.height, aspect, v.frame_rate)?;
    Ok(Box::new(reader))
}

#[cfg(target_arch = "wasm32")]
fn plugin_video(
    _fx: &bv_timeline::PluginFx,
    _v: bv_timeline::VideoStreamInfo,
) -> Result<Box<dyn VideoReader>> {
    Err(MediaError::Unsupported("OFX plug-ins".into()))
}

pub fn open_audio(info: &MediaInfo) -> Result<Box<dyn AudioReader>> {
    open_audio_stream(info, 0)
}

/// Reader of the media's audio stream `stream` (0 = the first).
pub fn open_audio_stream(info: &MediaInfo, stream: u16) -> Result<Box<dyn AudioReader>> {
    let audio = info.audio_stream(stream).ok_or(MediaError::NoStream)?;
    match &info.source {
        MediaSource::Generated(g) => Ok(Box::new(generator::GeneratedAudio::new(
            g.clone(),
            audio.sample_rate,
            audio.channels as usize,
        )?)),
        MediaSource::File { path } if nested::is_project(path) => {
            nested::open_audio(&local(path), stream)
        }
        MediaSource::File { path } => files::open_audio(&local(path), stream),
    }
}
