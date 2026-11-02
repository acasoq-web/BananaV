//! Media from files: told apart by their content (and, for formats without
//! a signature, by their extension), with a reader per kind.
//!
//! - Pictures BananaV reads itself (PNG, JPEG, BMP, TIFF, TGA, DPX,
//!   OpenEXR, Photoshop) and the ones the system reads (WebP, HEIF/HEIC,
//!   AVIF, icons; on Windows also JPEG XR, camera RAW and DNG) are stills,
//!   decoded once per file and shared by the readers. Animated GIFs play as
//!   video.
//! - Sound files (WAV, Wave64, AIFF, AU, CAF) are read with random access.
//! - Everything else goes through the system's media framework (Media
//!   Foundation on Windows, FFmpeg on Linux: `bv_mf`), which demuxes;
//!   BananaV decodes ProRes video and Dolby Digital (AC-3) audio itself.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};

use bv_color::transfer::bt709_to_linear;
use bv_core::{
    AudioBuffer, ChannelLayout, ColorInfo, ColorPrimaries, ColorRange, FrameRate, ImageRgbaF32,
    MatrixCoefficients, PixelFormat, Plane, PlaneData, SampleRate, Tick, TransferCharacteristic,
    VideoFrame,
};
use bv_image::read::{Format, Gif, GifPlayer, Picture, PictureData};
use bv_timeline::{
    AlphaChannel, AudioStreamInfo, Generator, ImageSequence, MediaId, MediaInfo, MediaSource,
    VideoStreamInfo,
};

use crate::{AudioReader, MediaError, Result, VideoReader};

fn io_err(e: std::io::Error) -> MediaError {
    MediaError::Io(e.to_string())
}

fn display_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or(path.to_string(), |n| n.to_string_lossy().to_string())
}

fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Extensions of the files BananaV imports, by kind (for file dialogs and
/// the Explorer). Formats told apart by their content open whatever their
/// name; these are the names they usually have.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "qt", "avi", "wmv", "asf", "mkv", "webm", "mpg", "mpeg", "mpe", "m1v",
    "vob", "ts", "mts", "m2ts", "m2t", "tod", "mod", "3gp", "3g2",
];
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "wav", "bwf", "rf64", "w64", "aif", "aiff", "aifc", "au", "snd", "caf", "mp3", "mp2", "mpa",
    "m4a", "m4b", "aac", "wma", "flac", "ogg", "oga", "opus", "mka", "ac3", "amr",
];
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "apng", "jpg", "jpeg", "jpe", "jfif", "bmp", "dib", "gif", "tif", "tiff", "tga", "icb",
    "vda", "vst", "dpx", "exr", "psd", "psb", "webp", "heic", "heif", "hif", "avif", "jxr", "wdp",
    "hdp", "ico", "dng", "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "orf", "rw2",
    "raf", "pef", "srw", "x3f", "erf", "kdc", "dcr", "mrw", "3fr", "mef", "iiq", "rwl",
];

/// Camera RAW files: Windows' RAW extension develops them (many are TIFF
/// inside, which BananaV's TIFF reader would show undeveloped).
const RAW_EXTENSIONS: &[&str] = &[
    "dng", "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "orf", "rw2", "raf", "pef",
    "srw", "x3f", "erf", "kdc", "dcr", "mrw", "3fr", "mef", "iiq", "rwl",
];

/// How a file is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Png,
    /// BananaV's own image readers.
    Image(Format),
    Gif,
    Tga,
    /// The system's picture readers (the Windows Imaging Component; FFmpeg
    /// on Linux).
    SystemImage,
    /// WAV, Wave64, AIFF, AU, CAF.
    Sound,
    /// DTS audio (Windows has no decoder; FFmpeg has one).
    Dts,
    /// The system's media framework.
    Other,
}

/// Whether the system's media framework decodes DTS: FFmpeg does, Windows
/// has no decoder.
const SYSTEM_DTS: bool = cfg!(target_os = "linux");

/// The size of a picture the system reads.
fn system_image_size(path: &str) -> std::result::Result<(u32, u32), String> {
    #[cfg(windows)]
    {
        bv_wic::dimensions(path).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        bv_mf::still_size(path).map_err(|e| e.to_string())
    }
}

/// A picture the system reads: its size and RGBA of 16 bits.
fn system_image(path: &str) -> std::result::Result<(u32, u32, Vec<u16>), String> {
    #[cfg(windows)]
    {
        bv_wic::decode(path)
            .map(|d| (d.width, d.height, d.rgba))
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        bv_mf::read_still(path)
            .map(|s| (s.width, s.height, s.rgba))
            .map_err(|e| e.to_string())
    }
}

/// The first 256 KB of a file (headers live there).
fn head(path: &str) -> Result<Vec<u8>> {
    let f = File::open(path).map_err(io_err)?;
    let mut head = Vec::with_capacity(1 << 18);
    f.take(1 << 18).read_to_end(&mut head).map_err(io_err)?;
    Ok(head)
}

/// ISO media files that hold pictures (HEIF, HEIC, AVIF), not video.
fn is_heif(head: &[u8]) -> bool {
    if head.get(4..8) != Some(b"ftyp") {
        return false;
    }
    let brands = [
        b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx", b"mif1", b"msf1", b"avif", b"avis",
    ];
    let size = head
        .get(..4)
        .map_or(0, |b| u32::from_be_bytes(b.try_into().unwrap_or([0; 4]))) as usize;
    // The major brand, then the compatible ones.
    let end = size.min(head.len());
    (8..end.saturating_sub(3))
        .step_by(4)
        .filter(|&i| i != 12)
        .any(|i| brands.iter().any(|b| head.get(i..i + 4) == Some(&b[..])))
}

/// DTS: the sync word in one of its four packings.
fn is_dts(head: &[u8]) -> bool {
    let syncs: [[u8; 4]; 4] = [
        [0x7f, 0xfe, 0x80, 0x01],
        [0xfe, 0x7f, 0x01, 0x80],
        [0x1f, 0xff, 0xe8, 0x00],
        [0xff, 0x1f, 0x00, 0xe8],
    ];
    syncs.iter().any(|s| head.starts_with(s))
}

fn kind(path: &str, head: &[u8]) -> Kind {
    let ext = extension(path);
    if head.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Kind::Png;
    }
    match bv_image::read::sniff(head) {
        Some(Format::Gif) => return Kind::Gif,
        Some(Format::Tiff) if RAW_EXTENSIONS.contains(&ext.as_str()) => return Kind::SystemImage,
        Some(f) => return Kind::Image(f),
        None => {}
    }
    let webp = head.len() >= 12 && head.starts_with(b"RIFF") && &head[8..12] == b"WEBP";
    let jxr = head.starts_with(&[0x49, 0x49, 0xbc]);
    let ico = head.starts_with(&[0, 0, 1, 0]) && matches!(ext.as_str(), "ico" | "cur");
    if webp || jxr || ico || is_heif(head) || RAW_EXTENSIONS.contains(&ext.as_str()) {
        return Kind::SystemImage;
    }
    if matches!(ext.as_str(), "tga" | "icb" | "vda" | "vst")
        && bv_image::read::tga_dimensions(head).is_some()
    {
        return Kind::Tga;
    }
    if is_dts(head) {
        return Kind::Dts;
    }
    let mut r = std::io::Cursor::new(head);
    match bv_wav::read_info(&mut r) {
        Ok(_) => Kind::Sound,
        // A WAV of another codec (MP3 in WAV…): Windows' turn.
        Err(_) => Kind::Other,
    }
}

pub(crate) fn base(path: &str) -> MediaInfo {
    MediaInfo {
        id: MediaId(0),
        source: MediaSource::File {
            path: path.to_string(),
        },
        name: display_name(path),
        duration: Tick::ZERO,
        video: None,
        audio: None,
        extra_audio: Vec::new(),
        sequence: None,
        still: false,
        effects: Vec::new(),
        markers: Vec::new(),
        color_space: None,
        reel: Default::default(),
        layer: None,
    }
}

fn still_info(path: &str, (width, height): (u32, u32)) -> MediaInfo {
    MediaInfo {
        duration: Generator::DURATION,
        video: Some(VideoStreamInfo {
            width,
            height,
            frame_rate: FrameRate::FPS_30,
            pixel_aspect: Default::default(),
            field_order: Default::default(),
            spherical: false,
            full_range: false,
            reading: Default::default(),
        }),
        still: true,
        ..base(path)
    }
}

pub(crate) fn probe(path: &str) -> Result<MediaInfo> {
    let head = head(path)?;
    let mut info = probe_streams(path, &head)?;
    if let Some(v) = &mut info.video {
        v.spherical = spherical(path, &head);
    }
    Ok(info)
}

/// A 360° media: a photo whose XMP calls it an equirectangular panorama
/// (Google's Photo Sphere, `GPano:ProjectionType`), or a movie with
/// spherical video metadata.
fn spherical(path: &str, head: &[u8]) -> bool {
    const KEY: &[u8] = b"GPano:ProjectionType";
    const EQUI: &[u8] = b"equirectangular";
    let panorama = head
        .windows(KEY.len())
        .enumerate()
        .filter(|(_, w)| *w == KEY)
        .any(|(at, _)| {
            // `="equirectangular"` or `>equirectangular<`.
            let rest = &head[at + KEY.len()..head.len().min(at + KEY.len() + 24)];
            rest.windows(EQUI.len())
                .any(|w| w.eq_ignore_ascii_case(EQUI))
        });
    panorama || File::open(path).is_ok_and(|mut f| bv_mp4::read::spherical(&mut f))
}

fn probe_streams(path: &str, head: &[u8]) -> Result<MediaInfo> {
    let corrupt = |e: &dyn std::fmt::Display| MediaError::Corrupt(e.to_string());
    match kind(path, head) {
        Kind::Png => {
            let h = bv_png::read_header(head).map_err(|e| corrupt(&e))?;
            Ok(still_info(path, (h.width, h.height)))
        }
        Kind::Image(_) => {
            let size = match bv_image::read::dimensions(head) {
                Some(s) => s,
                None => {
                    let p = picture(path)?;
                    (p.width, p.height)
                }
            };
            Ok(still_info(path, size))
        }
        Kind::Tga => {
            let size = bv_image::read::tga_dimensions(head)
                .ok_or_else(|| MediaError::Corrupt("TGA header".into()))?;
            Ok(still_info(path, size))
        }
        Kind::SystemImage => {
            // The system lacks the reader (Windows' HEIF or RAW extension;
            // FFmpeg reads no RAW).
            let size = system_image_size(path)
                .map_err(|_| MediaError::UnsupportedFile(extension(path)))?;
            Ok(still_info(path, size))
        }
        Kind::Gif => {
            let gif = gif(path)?;
            if gif.frames.len() < 2 {
                return Ok(still_info(path, (gif.width, gif.height)));
            }
            Ok(MediaInfo {
                duration: Tick::from_timescale(gif.duration() as i64, 100),
                video: Some(VideoStreamInfo {
                    width: gif.width,
                    height: gif.height,
                    frame_rate: gif_rate(&gif),
                    pixel_aspect: Default::default(),
                    field_order: Default::default(),
                    spherical: false,
                    full_range: false,
                    reading: Default::default(),
                }),
                ..base(path)
            })
        }
        Kind::Sound => {
            let mut r = BufReader::new(File::open(path).map_err(io_err)?);
            let info = bv_wav::read_info(&mut r).map_err(|e| corrupt(&e))?;
            let rate = SampleRate(info.sample_rate);
            Ok(MediaInfo {
                duration: rate.sample_start(info.frames as i64),
                audio: Some(AudioStreamInfo {
                    sample_rate: rate,
                    channels: info.channels,
                }),
                ..base(path)
            })
        }
        Kind::Dts if !SYSTEM_DTS => Err(MediaError::UnsupportedCodec {
            codec: "DTS".into(),
            video: false,
        }),
        Kind::Dts | Kind::Other => system_probe(path),
    }
}

/// Who reads a file (a media's Properties' General tab).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileReader {
    /// BananaV's own readers.
    Own,
    /// The system's picture readers (the Windows Imaging Component; FFmpeg
    /// on Linux).
    SystemPictures,
    /// The system's media framework (Media Foundation; FFmpeg on Linux).
    SystemMedia,
}

/// What a file is, as a media's Properties' General tab tells it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileDescription {
    /// Its format: the container's or the picture's ("MPEG-4", "PNG"…).
    pub format: String,
    /// Who reads it, and who decodes its streams when that is BananaV
    /// (ProRes, AC-3) or the system.
    pub readers: Vec<FileReader>,
    /// Its video's compressed format ("H.264", "ProRes 422"…).
    pub video_codec: Option<String>,
    /// Each audio stream's compressed format.
    pub audio_codecs: Vec<String>,
    /// An MPEG-4 family file's major brand (`isom`, `mp42`, `qt  `…).
    pub brand: Option<[u8; 4]>,
    /// An MPEG-4 family file's tracks, as its boxes describe them.
    pub tracks: Vec<bv_mp4::facts::TrackFacts>,
    /// A picture's alpha: whether some pixel is not opaque (`None`: not
    /// looked at).
    pub alpha: Option<bool>,
    /// A Wave file's loop chunk (`acid`: its tempo, beats and root note,
    /// which loop libraries keep).
    pub loop_chunk: bool,
}

/// What the file at `path` is: its format, who reads it and its streams'
/// compressed formats (as far as they are told: nothing for a file that
/// cannot be read).
pub fn describe_file(path: &str) -> FileDescription {
    let Ok(head) = head(path) else {
        return FileDescription::default();
    };
    let ext = extension(path).to_uppercase();
    let own = |format: &str| FileDescription {
        format: format.to_string(),
        readers: vec![FileReader::Own],
        ..FileDescription::default()
    };
    // A picture: whether some of it is see-through.
    let picture_alpha = || {
        picture(path).ok().map(|p| match &p.data {
            PictureData::Rgba8(d) => d.as_chunks::<4>().0.iter().any(|px| px[3] < 255),
            PictureData::Rgba16(d) => d.as_chunks::<4>().0.iter().any(|px| px[3] < 65535),
            PictureData::RgbaF32(d) => d.as_chunks::<4>().0.iter().any(|px| px[3] < 1.0),
        })
    };
    let still = |mut d: FileDescription| {
        d.alpha = picture_alpha();
        d
    };
    match kind(path, &head) {
        Kind::Png => still(own("PNG")),
        Kind::Image(f) => still(own(match f {
            Format::Bmp => "BMP",
            Format::Gif => "GIF",
            Format::Jpeg => "JPEG",
            Format::Tiff => "TIFF",
            Format::Dpx => "DPX",
            Format::Exr => "OpenEXR",
            Format::Psd => "Photoshop",
        })),
        Kind::Gif => own("GIF"),
        Kind::Tga => still(own("TGA")),
        Kind::Sound => FileDescription {
            audio_codecs: vec!["PCM".into()],
            loop_chunk: riff_chunks(path).iter().any(|id| id == b"acid"),
            ..own(container_name(&head).unwrap_or(&ext))
        },
        Kind::SystemImage => still(FileDescription {
            format: ext,
            readers: vec![FileReader::SystemPictures],
            ..FileDescription::default()
        }),
        Kind::Dts | Kind::Other => {
            let probe = bv_mf::probe(path).ok();
            let video = probe.as_ref().and_then(|p| p.video);
            let audio = probe.as_ref().map_or(&[][..], |p| &p.audio_streams[..]);
            let mut readers = vec![FileReader::SystemMedia];
            let own_decoder = video.is_some_and(|v| bv_mf::codec::is_prores(v.codec))
                || audio
                    .iter()
                    .any(|a| bv_mf::codec::is_ac3(a.codec) && !a.decodable);
            if own_decoder {
                readers.push(FileReader::Own);
            }
            let tracks = File::open(path)
                .map(|mut f| bv_mp4::facts::tracks(&mut f))
                .unwrap_or_default();
            let alpha = tracks
                .iter()
                .find(|t| &t.handler == b"vide")
                .map(|t| t.alpha);
            FileDescription {
                format: container_name(&head).unwrap_or(&ext).to_string(),
                readers,
                video_codec: video.map(|v| bv_mf::codec::name(v.codec)),
                audio_codecs: audio.iter().map(|a| audio_codec_name(a.codec)).collect(),
                brand: bv_mp4::facts::major_brand(&head),
                tracks,
                alpha,
                loop_chunk: false,
            }
        }
    }
}

/// The kinds of a RIFF file's chunks, in order (none for other files).
fn riff_chunks(path: &str) -> Vec<[u8; 4]> {
    use std::io::{Seek, SeekFrom};
    let Ok(mut f) = File::open(path) else {
        return Vec::new();
    };
    let mut head = [0u8; 12];
    if f.read_exact(&mut head).is_err() || &head[..4] != b"RIFF" {
        return Vec::new();
    }
    let mut ids = Vec::new();
    let mut chunk = [0u8; 8];
    while ids.len() < 256 && f.read_exact(&mut chunk).is_ok() {
        let size = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]) as i64;
        ids.push([chunk[0], chunk[1], chunk[2], chunk[3]]);
        // Chunks are padded to even sizes.
        if f.seek(SeekFrom::Current(size + (size & 1))).is_err() {
            break;
        }
    }
    ids
}

/// The container a file's first bytes say it is, when they say.
fn container_name(head: &[u8]) -> Option<&'static str> {
    let at = |i: usize, s: &[u8]| head.get(i..i + s.len()) == Some(s);
    Some(if at(4, b"ftypqt  ") {
        "QuickTime"
    } else if at(4, b"ftyp") || at(4, b"moov") || at(4, b"mdat") {
        "MPEG-4"
    } else if at(0, &[0x1a, 0x45, 0xdf, 0xa3]) {
        if head.windows(4).any(|w| w == b"webm") {
            "WebM"
        } else {
            "Matroska"
        }
    } else if at(0, b"RIFF") && at(8, b"AVI ") {
        "AVI"
    } else if at(0, b"RIFF") && at(8, b"WAVE") {
        "Wave"
    } else if at(0, b"riff") {
        "Wave64"
    } else if at(0, b"FORM") && (at(8, b"AIFF") || at(8, b"AIFC")) {
        "AIFF"
    } else if at(0, b"caff") {
        "CAF"
    } else if at(0, b".snd") {
        "AU"
    } else if at(0, &[0x30, 0x26, 0xb2, 0x75]) {
        "Windows Media"
    } else if at(0, &[0x06, 0x0e, 0x2b, 0x34]) {
        "MXF"
    } else if at(0, &[0, 0, 1, 0xba]) {
        "MPEG program stream"
    } else if head.first() == Some(&0x47) && head.get(188) == Some(&0x47) {
        "MPEG transport stream"
    } else if at(0, b"fLaC") {
        "FLAC"
    } else if at(0, b"OggS") {
        "Ogg"
    } else if at(0, b"ID3") || (head.len() > 1 && head[0] == 0xff && head[1] & 0xe0 == 0xe0) {
        "MPEG audio"
    } else {
        return None;
    })
}

/// An audio stream's compressed format, by its WAVE format tag or FourCC.
fn audio_codec_name(code: u32) -> String {
    match code {
        0x0001 => "PCM".into(),
        0x0003 => "PCM (floating point)".into(),
        0x0050 => "MPEG audio".into(),
        0x0055 => "MP3".into(),
        0x0161..=0x0163 => "Windows Media Audio".into(),
        0x1610 | 0x1600 | 0x00ff => "AAC".into(),
        0xf1ac => "FLAC".into(),
        0x704f => "Opus".into(),
        0x6c61 => "ALAC".into(),
        c => bv_mf::codec::name(c),
    }
}

/// What the system's media framework finds, keeping what BananaV can
/// decode: video the system decodes or ProRes; audio it decodes or AC-3.
fn system_probe(path: &str) -> Result<MediaInfo> {
    let p = match bv_mf::probe(path) {
        Ok(p) => p,
        Err(bv_mf::Error::NoStream) => return Err(MediaError::NoStream),
        Err(bv_mf::Error::UnsupportedFile) => {
            return Err(MediaError::UnsupportedFile(extension(path)));
        }
        Err(e) => return Err(MediaError::Unsupported(e.to_string())),
    };
    let video = match p.video {
        Some(v) if v.decodable || bv_mf::codec::is_prores(v.codec) => Some(VideoStreamInfo {
            width: v.width,
            height: v.height,
            frame_rate: v.frame_rate,
            pixel_aspect: bv_timeline::PixelAspect::new(v.pixel_aspect.0, v.pixel_aspect.1),
            field_order: v.field_order,
            spherical: false,
            full_range: false,
            reading: Default::default(),
        }),
        Some(v) => {
            // Windows decodes H.264 only in 4:2:0 and 8 bits.
            let codec = match bv_mf::codec::name(v.codec).as_str() {
                "H.264" => "H.264 4:2:2 / 10-bit".to_string(),
                name => name.to_string(),
            };
            return Err(MediaError::UnsupportedCodec { codec, video: true });
        }
        None => None,
    };
    // Audio streams nothing here decodes (DTS, E-AC-3…) are left out. The
    // channels and rate are the decoder's (an MPEG-TS stream may claim
    // stereo and hold mono; an AC-3 stream says them in its frames).
    let mut audio: Vec<AudioStreamInfo> = p
        .audio_streams
        .iter()
        .enumerate()
        .filter(|(_, a)| a.decodable || bv_mf::codec::is_ac3(a.codec))
        .map(|(i, a)| {
            let decoder = if bv_mf::codec::is_ac3(a.codec) {
                bv_mf::AudioDecoder::open_with(path, i, Box::new(Ac3::default()))
            } else {
                bv_mf::AudioDecoder::open_stream(path, i)
            };
            match decoder {
                Ok(mut d) => {
                    let _ = d.read(0, 1);
                    let info = d.info();
                    AudioStreamInfo {
                        sample_rate: info.sample_rate,
                        channels: info.channels,
                    }
                }
                Err(_) => AudioStreamInfo {
                    sample_rate: a.sample_rate,
                    channels: a.channels,
                },
            }
        })
        .collect();
    if video.is_none() && audio.is_empty() {
        let codec = p.audio_streams.first().map_or(0, |a| a.codec);
        return Err(MediaError::UnsupportedCodec {
            codec: bv_mf::codec::name(codec),
            video: false,
        });
    }
    let first = (!audio.is_empty()).then(|| audio.remove(0));
    Ok(MediaInfo {
        duration: p.duration,
        video,
        audio: first,
        extra_audio: audio,
        ..base(path)
    })
}

/// The index the system gives the `stream`-th audio stream BananaV kept
/// (streams it cannot decode were left out at import).
fn system_audio_index(path: &str, stream: u16) -> Result<(usize, u32)> {
    let p = bv_mf::probe(path).map_err(mf_err)?;
    p.audio_streams
        .iter()
        .enumerate()
        .filter(|(_, a)| a.decodable || bv_mf::codec::is_ac3(a.codec))
        .nth(stream as usize)
        .map(|(i, a)| (i, a.codec))
        .ok_or(MediaError::NoStream)
}

// ---- Stills ----------------------------------------------------------------------------

/// Pictures decoded once and shared while some reader holds them.
fn shared<T>(
    cache: &'static Mutex<Option<HashMap<String, Weak<T>>>>,
    path: &str,
    load: impl FnOnce() -> Result<T>,
) -> Result<Arc<T>> {
    if let Ok(mut c) = cache.lock()
        && let Some(p) = c
            .get_or_insert_with(HashMap::new)
            .get(path)
            .and_then(Weak::upgrade)
    {
        return Ok(p);
    }
    let p = Arc::new(load()?);
    if let Ok(mut c) = cache.lock() {
        let map = c.get_or_insert_with(HashMap::new);
        map.retain(|_, w| w.strong_count() > 0);
        map.insert(path.to_string(), Arc::downgrade(&p));
    }
    Ok(p)
}

static PICTURES: Mutex<Option<HashMap<String, Weak<Picture>>>> = Mutex::new(None);
static LAYERS: Mutex<Option<HashMap<String, Weak<Picture>>>> = Mutex::new(None);

/// Layer `layer` of a layered Photoshop document, on its canvas.
fn layer_picture(path: &str, layer: u16) -> Result<Arc<Picture>> {
    shared(&LAYERS, &format!("{path}#{layer}"), || {
        let data = std::fs::read(path).map_err(io_err)?;
        bv_image::read::psd_layer(&data, layer as usize)
            .map_err(|e| MediaError::Unsupported(e.to_string()))
    })
}

/// A reader of one layer of a Photoshop document.
pub(crate) fn open_layer(path: &str, layer: u16) -> Result<Box<dyn VideoReader>> {
    Ok(Box::new(StillImage {
        picture: layer_picture(path, layer)?,
        last: None,
        alpha: AlphaReading::default(),
    }))
}

/// The blend modes' keys of a Photoshop document's picture layers, bottom
/// first (none for other files or flat documents).
pub(crate) fn psd_layer_blends(path: &str) -> Vec<[u8; 4]> {
    let lower = path.to_lowercase();
    if !(lower.ends_with(".psd") || lower.ends_with(".psb")) {
        return Vec::new();
    }
    std::fs::read(path)
        .ok()
        .and_then(|d| bv_image::read::psd_layer_blends(&d).ok())
        .unwrap_or_default()
}

/// The names of a Photoshop document's picture layers, bottom first (none
/// for other files or flat documents).
pub(crate) fn psd_layer_names(path: &str) -> Vec<String> {
    let lower = path.to_lowercase();
    if !(lower.ends_with(".psd") || lower.ends_with(".psb")) {
        return Vec::new();
    }
    std::fs::read(path)
        .ok()
        .and_then(|d| bv_image::read::psd_layer_names(&d).ok())
        .unwrap_or_default()
}
static GIFS: Mutex<Option<HashMap<String, Weak<Gif>>>> = Mutex::new(None);

/// The picture of an image file (any still kind).
fn picture(path: &str) -> Result<Arc<Picture>> {
    shared(&PICTURES, path, || {
        let data = std::fs::read(path).map_err(io_err)?;
        let unsupported = |e: &dyn std::fmt::Display| MediaError::Unsupported(e.to_string());
        match kind(path, &data[..data.len().min(1 << 18)]) {
            Kind::Png => {
                let png = bv_png::decode(&data).map_err(|e| MediaError::Corrupt(e.to_string()))?;
                let data = match png.pixels {
                    bv_png::Pixels::Rgba8(px) => PictureData::Rgba8(px),
                    bv_png::Pixels::Rgba16(px) => PictureData::Rgba16(px),
                };
                Ok(Picture {
                    width: png.width,
                    height: png.height,
                    data,
                    linear: false,
                    premultiplied: false,
                })
            }
            Kind::Tga => bv_image::read::read_tga(&data).map_err(|e| unsupported(&e)),
            Kind::SystemImage => {
                let (width, height, rgba) = system_image(path).map_err(|e| unsupported(&e))?;
                Ok(Picture {
                    width,
                    height,
                    data: PictureData::Rgba16(rgba),
                    linear: false,
                    premultiplied: false,
                })
            }
            _ => bv_image::read::read(&data).map_err(|e| unsupported(&e)),
        }
    })
}

fn gif(path: &str) -> Result<Arc<Gif>> {
    shared(&GIFS, path, || {
        let data = std::fs::read(path).map_err(io_err)?;
        Gif::parse(&data).map_err(|e| MediaError::Corrupt(e.to_string()))
    })
}

/// The rate of an animated GIF: that of its most common frame length.
fn gif_rate(gif: &Gif) -> FrameRate {
    let mut counts: HashMap<u32, usize> = HashMap::new();
    for f in &gif.frames {
        *counts.entry(f.delay).or_default() += 1;
    }
    let delay = counts
        .into_iter()
        .max_by_key(|&(d, n)| (n, std::cmp::Reverse(d)))
        .map_or(10, |(d, _)| d.max(1));
    // 100 hundredths of a second.
    let g = gcd(100, delay);
    FrameRate::new(100 / g, delay / g)
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// A picture's encoded value in linear light: taken as video's (BT.709's
/// curve), as VEGAS takes a picture's values — a grey of 128 is video's
/// 128, the preview and renders showing the file's values again — rather
/// than as the sRGB light its maker saw, which video's curve would show
/// darker (128 as 115).
fn picture_to_linear(v: f32) -> f32 {
    bt709_to_linear(v)
}

/// 8-bit values in linear light ([`picture_to_linear`]).
fn light8() -> &'static [f32; 256] {
    static T: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    T.get_or_init(|| std::array::from_fn(|i| picture_to_linear(i as f32 / 255.0)))
}

/// 16-bit values in linear light ([`picture_to_linear`]).
fn light16() -> &'static [f32] {
    static T: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        (0..65536)
            .map(|i| picture_to_linear(i as f32 / 65535.0))
            .collect()
    })
}

/// The largest halving (power of 2, up to 16) whose result still covers
/// `min_w` × `min_h`.
fn reduction(w: u32, h: u32, min_w: u32, min_h: u32) -> u32 {
    let mut factor = 1u32;
    while factor < 16 && w / (factor * 2) >= min_w && h / (factor * 2) >= min_h {
        factor *= 2;
    }
    factor
}

/// How a picture's alpha channel is read: Media Properties' Alpha channel
/// and, for Premultiplied (dirty), the Background color (its own values).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct AlphaReading {
    pub mode: AlphaChannel,
    pub background: [u8; 3],
}

/// A picture in linear light with premultiplied alpha, `k` times smaller
/// (each pixel the mean of a k×k block, in linear light), its alpha read
/// as `alpha` says: as its file keeps it (straight, OpenEXR's
/// premultiplied), straight, premultiplied — against black or, dirty, the
/// background —, or none (opaque, its colors as they are).
fn to_linear(p: &Picture, k: usize, alpha: AlphaReading) -> ImageRgbaF32 {
    let (w, h) = (p.width as usize, p.height as usize);
    let (ow, oh) = (w.div_ceil(k), h.div_ceil(k));
    let mut out = ImageRgbaF32::transparent(ow as u32, oh as u32);
    let (l8, l16) = (
        light8(),
        if matches!(p.data, PictureData::Rgba16(_)) {
            light16()
        } else {
            &[][..]
        },
    );
    let premultiplied = match alpha.mode {
        AlphaChannel::Straight => false,
        AlphaChannel::Premultiplied | AlphaChannel::PremultipliedDirty => true,
        _ => p.premultiplied,
    };
    let opaque = alpha.mode == AlphaChannel::None;
    let dirty = alpha.mode == AlphaChannel::PremultipliedDirty;
    // The background in the picture's own values (light, in linear ones).
    let back = alpha.background.map(|v| {
        let v = v as f32 / 255.0;
        if p.linear { picture_to_linear(v) } else { v }
    });
    // Its colors stored straight and encoded: the common case, by tables.
    let plain = !p.linear && !premultiplied && !opaque;
    // One pixel of stored values (0–1; light for linear pictures) and alpha.
    let general = |s: [f32; 3], a: f32| -> [f32; 4] {
        if opaque {
            let c = if p.linear {
                s
            } else {
                s.map(picture_to_linear)
            };
            return [c[0], c[1], c[2], 1.0];
        }
        let s: [f32; 3] = if dirty {
            std::array::from_fn(|c| s[c] - (1.0 - a) * back[c])
        } else {
            s
        };
        if p.linear {
            return if premultiplied {
                [s[0], s[1], s[2], a]
            } else {
                [s[0] * a, s[1] * a, s[2] * a, a]
            };
        }
        let straight = if premultiplied {
            if a <= 0.0 {
                return [0.0; 4];
            }
            s.map(|v| (v / a).clamp(0.0, 1.0))
        } else {
            s
        };
        let c = straight.map(picture_to_linear);
        [c[0] * a, c[1] * a, c[2] * a, a]
    };
    // One pixel, premultiplied linear.
    let pixel = |i: usize| -> [f32; 4] {
        match &p.data {
            PictureData::Rgba8(d) => {
                let s = &d[i * 4..i * 4 + 4];
                let a = s[3] as f32 / 255.0;
                if plain {
                    let c = [l8[s[0] as usize], l8[s[1] as usize], l8[s[2] as usize]];
                    return [c[0] * a, c[1] * a, c[2] * a, a];
                }
                general(
                    [
                        s[0] as f32 / 255.0,
                        s[1] as f32 / 255.0,
                        s[2] as f32 / 255.0,
                    ],
                    a,
                )
            }
            PictureData::Rgba16(d) => {
                let s = &d[i * 4..i * 4 + 4];
                let a = s[3] as f32 / 65535.0;
                if plain {
                    let c = [l16[s[0] as usize], l16[s[1] as usize], l16[s[2] as usize]];
                    return [c[0] * a, c[1] * a, c[2] * a, a];
                }
                general(
                    [
                        s[0] as f32 / 65535.0,
                        s[1] as f32 / 65535.0,
                        s[2] as f32 / 65535.0,
                    ],
                    a,
                )
            }
            PictureData::RgbaF32(d) => {
                let s = &d[i * 4..i * 4 + 4];
                general([s[0], s[1], s[2]], s[3])
            }
        }
    };
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(16));
    let rows_per = oh.div_ceil(threads).max(1);
    std::thread::scope(|s| {
        for (chunk, rows) in out.data.chunks_mut(rows_per * ow).enumerate() {
            let pixel = &pixel;
            s.spawn(move || {
                for (r, row) in rows.chunks_mut(ow).enumerate() {
                    let oy = chunk * rows_per + r;
                    for (ox, px) in row.iter_mut().enumerate() {
                        let mut acc = [0f32; 4];
                        let mut n = 0f32;
                        for y in oy * k..((oy + 1) * k).min(h) {
                            for x in ox * k..((ox + 1) * k).min(w) {
                                let v = pixel(y * w + x);
                                for c in 0..4 {
                                    acc[c] += v[c];
                                }
                                n += 1.0;
                            }
                        }
                        *px = acc.map(|v| v / n.max(1.0));
                    }
                }
            });
        }
    });
    out
}

/// A still picture: decoded once, given at the size asked for.
pub(crate) struct StillImage {
    picture: Arc<Picture>,
    last: Option<(u32, Arc<ImageRgbaF32>)>,
    alpha: AlphaReading,
}

impl StillImage {
    pub(crate) fn open(path: &str) -> Result<StillImage> {
        Ok(StillImage {
            picture: picture(path)?,
            last: None,
            alpha: AlphaReading::default(),
        })
    }
}

impl VideoReader for StillImage {
    fn frame_at(&mut self, t: Tick) -> Result<Arc<ImageRgbaF32>> {
        self.frame_at_scaled(t, 0, 0)
    }

    fn frame_at_scaled(&mut self, _t: Tick, min_w: u32, min_h: u32) -> Result<Arc<ImageRgbaF32>> {
        let p = &self.picture;
        // `frame_at` wants the whole picture.
        let factor = if min_w == 0 && min_h == 0 {
            1
        } else {
            reduction(p.width, p.height, min_w, min_h)
        };
        if let Some((f, img)) = &self.last
            && *f == factor
        {
            return Ok(img.clone());
        }
        let img = Arc::new(to_linear(p, factor as usize, self.alpha));
        self.last = Some((factor, img.clone()));
        Ok(img)
    }

    fn set_alpha(&mut self, alpha: AlphaChannel, background: [u8; 3]) {
        let alpha = AlphaReading {
            mode: alpha,
            background,
        };
        if alpha != self.alpha {
            self.alpha = alpha;
            self.last = None;
        }
    }
}

/// An animated GIF, frame by frame.
pub(crate) struct GifVideo {
    gif: Arc<Gif>,
    player: GifPlayer,
    last: Option<(usize, u32, Arc<ImageRgbaF32>)>,
    alpha: AlphaReading,
}

impl GifVideo {
    fn open(path: &str) -> Result<GifVideo> {
        let gif = gif(path)?;
        Ok(GifVideo {
            player: GifPlayer::new(&gif),
            gif,
            last: None,
            alpha: AlphaReading::default(),
        })
    }
}

impl VideoReader for GifVideo {
    fn frame_at(&mut self, t: Tick) -> Result<Arc<ImageRgbaF32>> {
        self.frame_at_scaled(t, 0, 0)
    }

    fn frame_at_scaled(&mut self, t: Tick, min_w: u32, min_h: u32) -> Result<Arc<ImageRgbaF32>> {
        let hundredths = (t.max(Tick::ZERO).as_seconds_f64() * 100.0).floor() as u64;
        let index = self.gif.frame_at(hundredths);
        let factor = if min_w == 0 && min_h == 0 {
            1
        } else {
            reduction(self.gif.width, self.gif.height, min_w, min_h)
        };
        if let Some((i, f, img)) = &self.last
            && *i == index
            && *f == factor
        {
            return Ok(img.clone());
        }
        let canvas = self.player.frame(&self.gif, index).to_vec();
        let p = Picture {
            width: self.gif.width,
            height: self.gif.height,
            data: PictureData::Rgba8(canvas),
            linear: false,
            premultiplied: false,
        };
        let img = Arc::new(to_linear(&p, factor as usize, self.alpha));
        self.last = Some((index, factor, img.clone()));
        Ok(img)
    }

    fn set_alpha(&mut self, alpha: AlphaChannel, background: [u8; 3]) {
        let alpha = AlphaReading {
            mode: alpha,
            background,
        };
        if alpha != self.alpha {
            self.alpha = alpha;
            self.last = None;
        }
    }
}

// ---- Image sequences --------------------------------------------------------------------

pub(crate) fn sequence_length(first: &str) -> u32 {
    let mut n = 0u32;
    while n < 10_000_000 {
        match ImageSequence::path(first, n) {
            Some(p) if Path::new(&p).is_file() => n += 1,
            _ => break,
        }
    }
    n
}

pub(crate) fn probe_sequence(first: &str, count: u32, rate: FrameRate) -> Result<MediaInfo> {
    let mut info = probe(first)?;
    let (start, end, value) = ImageSequence::number_of(first)
        .ok_or_else(|| MediaError::Unsupported("a picture name without a number".into()))?;
    if !info.still || count == 0 {
        return Err(MediaError::Unsupported("not a run of pictures".into()));
    }
    // "shot_0001–0120.png".
    let width = end - start;
    let name = display_name(first);
    let name_start = first.len() - name.len();
    let last = value + count as u64 - 1;
    info.name = format!(
        "{}{:0width$}–{:0width$}{}",
        &first[name_start..start],
        value,
        last,
        &first[end..]
    );
    info.still = false;
    info.duration = rate.frame_start(count as i64);
    if let Some(v) = &mut info.video {
        v.frame_rate = rate;
    }
    info.sequence = Some(ImageSequence { count });
    Ok(info)
}

/// A run of numbered pictures, one a frame; a missing one shows black (as
/// in VEGAS).
pub(crate) struct SequenceVideo {
    first: String,
    count: u32,
    rate: FrameRate,
    current: Option<(u32, Option<StillImage>)>,
    alpha: AlphaReading,
}

impl SequenceVideo {
    pub(crate) fn new(first: &str, count: u32, rate: FrameRate) -> SequenceVideo {
        SequenceVideo {
            first: first.to_string(),
            count: count.max(1),
            rate,
            current: None,
            alpha: AlphaReading::default(),
        }
    }
}

impl VideoReader for SequenceVideo {
    fn frame_at(&mut self, t: Tick) -> Result<Arc<ImageRgbaF32>> {
        self.frame_at_scaled(t, 0, 0)
    }

    fn frame_at_scaled(&mut self, t: Tick, min_w: u32, min_h: u32) -> Result<Arc<ImageRgbaF32>> {
        let index = self.rate.frame_at(t).clamp(0, self.count as i64 - 1) as u32;
        if self.current.as_ref().is_none_or(|(i, _)| *i != index) {
            let alpha = self.alpha;
            let still = ImageSequence::path(&self.first, index)
                .and_then(|p| StillImage::open(&p).ok())
                .map(|s| StillImage { alpha, ..s });
            self.current = Some((index, still));
        }
        match &mut self.current {
            Some((_, Some(still))) => still.frame_at_scaled(t, min_w, min_h),
            _ => {
                // The first picture's size, black.
                let mut first = StillImage::open(&self.first)?;
                first.alpha = self.alpha;
                let size = first.frame_at_scaled(t, min_w, min_h)?;
                Ok(Arc::new(ImageRgbaF32::filled(
                    size.width,
                    size.height,
                    [0.0, 0.0, 0.0, 1.0],
                )))
            }
        }
    }

    fn set_alpha(&mut self, alpha: AlphaChannel, background: [u8; 3]) {
        let alpha = AlphaReading {
            mode: alpha,
            background,
        };
        if alpha != self.alpha {
            self.alpha = alpha;
            self.current = None;
        }
    }
}

// ---- Sound files ----------------------------------------------------------------------

pub(crate) struct SoundFile {
    reader: bv_wav::WavReader<BufReader<File>>,
}

impl SoundFile {
    pub(crate) fn open(path: &str) -> Result<SoundFile> {
        let f = File::open(path).map_err(io_err)?;
        let reader = bv_wav::WavReader::new(BufReader::with_capacity(1 << 16, f))
            .map_err(|e| MediaError::Corrupt(e.to_string()))?;
        Ok(SoundFile { reader })
    }
}

impl AudioReader for SoundFile {
    fn sample_rate(&self) -> SampleRate {
        SampleRate(self.reader.info.sample_rate)
    }

    fn channels(&self) -> usize {
        self.reader.info.channels as usize
    }

    fn read(&mut self, start: i64, frames: usize) -> Result<AudioBuffer> {
        let channels = self
            .reader
            .read_planar(start, frames)
            .map_err(|e| MediaError::Io(e.to_string()))?;
        Ok(AudioBuffer {
            sample_rate: self.sample_rate(),
            layout: ChannelLayout::from_count(channels.len()),
            channels,
        })
    }
}

// ---- Opening ---------------------------------------------------------------------------

/// The right video reader for the file.
pub(crate) fn open_video(path: &str) -> Result<Box<dyn VideoReader>> {
    let head = head(path)?;
    match kind(path, &head) {
        Kind::Png | Kind::Image(_) | Kind::Tga | Kind::SystemImage => {
            Ok(Box::new(StillImage::open(path)?))
        }
        Kind::Gif => {
            if gif(path)?.frames.len() < 2 {
                Ok(Box::new(StillImage::open(path)?))
            } else {
                Ok(Box::new(GifVideo::open(path)?))
            }
        }
        Kind::Sound | Kind::Dts => Err(MediaError::NoStream),
        Kind::Other => match SystemVideo::open(path) {
            Ok(v) => Ok(Box::new(v)),
            Err(e) => {
                // The system reads ProRes files; BananaV decodes them.
                let reader = bv_mf::PacketReader::video(path).map_err(|_| e.clone())?;
                if bv_mf::codec::is_prores(reader.info().codec) {
                    Ok(Box::new(ProResVideo::new(reader)))
                } else {
                    Err(e)
                }
            }
        },
    }
}

pub(crate) fn open_audio(path: &str, stream: u16) -> Result<Box<dyn AudioReader>> {
    let head = head(path)?;
    match kind(path, &head) {
        Kind::Sound => Ok(Box::new(SoundFile::open(path)?)),
        k @ (Kind::Other | Kind::Dts) if k == Kind::Other || SYSTEM_DTS => {
            let (index, codec) = system_audio_index(path, stream)?;
            if bv_mf::codec::is_ac3(codec) {
                let dec = bv_mf::AudioDecoder::open_with(path, index, Box::new(Ac3::default()))
                    .map_err(mf_err)?;
                Ok(Box::new(SystemAudio { dec }))
            } else {
                Ok(Box::new(SystemAudio::open(path, index)?))
            }
        }
        Kind::Dts => Err(MediaError::UnsupportedCodec {
            codec: "DTS".into(),
            video: false,
        }),
        _ => Err(MediaError::NoStream),
    }
}

fn mf_err(e: bv_mf::Error) -> MediaError {
    match e {
        bv_mf::Error::NoStream => MediaError::NoStream,
        bv_mf::Error::UnsupportedFile => MediaError::UnsupportedFile(String::new()),
        bv_mf::Error::Interrupted => MediaError::Interrupted,
        other => MediaError::Unsupported(other.to_string()),
    }
}

/// Video the system decodes, in linear light.
pub(crate) struct SystemVideo {
    dec: bv_mf::VideoDecoder,
    /// The last frame converted (time, reduction, levels): the same frame
    /// is asked for several times.
    last: Option<(Tick, u32, bool, Arc<ImageRgbaF32>)>,
    /// Y′CbCr as studio RGB (the project's video levels formats).
    video_levels: bool,
}

impl SystemVideo {
    fn open(path: &str) -> Result<SystemVideo> {
        Ok(SystemVideo {
            dec: bv_mf::VideoDecoder::open(path).map_err(mf_err)?,
            last: None,
            video_levels: false,
        })
    }
}

impl VideoReader for SystemVideo {
    fn frame_at(&mut self, t: Tick) -> Result<Arc<ImageRgbaF32>> {
        self.frame_at_scaled(t, u32::MAX, u32::MAX)
    }

    fn frame_at_scaled(&mut self, t: Tick, min_w: u32, min_h: u32) -> Result<Arc<ImageRgbaF32>> {
        let frame = self.dec.frame_at(t).map_err(mf_err)?;
        // The largest halving (power of 2) that still covers the size.
        let factor = crate::reduction(frame.width, frame.height, min_w, min_h);
        let levels = self.video_levels;
        if let Some((pts, f, l, img)) = &self.last
            && *pts == frame.pts
            && *f == factor
            && *l == levels
        {
            return Ok(img.clone());
        }
        let img = Arc::new(bv_color::frame_to_linear_levels(&frame, factor, levels));
        self.last = Some((frame.pts, factor, levels, img.clone()));
        Ok(img)
    }

    fn set_video_levels(&mut self, on: bool) {
        self.video_levels = on;
    }

    fn picture_at(&mut self, t: Tick, _min_w: u32, _min_h: u32) -> Result<crate::Picture> {
        let frame = self.dec.frame_at(t).map_err(mf_err)?;
        Ok(crate::Picture::Decoded(frame))
    }

    fn picture_on_the_way(
        &mut self,
        t: Tick,
        min_w: u32,
        min_h: u32,
        want: crate::Want,
        on: &mut dyn FnMut(Option<crate::Picture>) -> crate::Onward,
    ) -> Result<crate::Picture> {
        let mut way = OnTheWay {
            on,
            want,
            video_levels: self.video_levels,
            min_size: (min_w, min_h),
            next: std::time::Instant::now() + OnTheWay::FIRST,
            hurry: false,
            stopped: false,
        };
        self.dec.frame_at_with(t, &mut way).map_err(mf_err)?;
        match want {
            crate::Want::Decoded => self.picture_at(t, min_w, min_h),
            crate::Want::Linear => self
                .frame_at_scaled(t, min_w, min_h)
                .map(crate::Picture::Linear),
        }
    }
}

/// A seek's way to its frame, shown now and then (see
/// `VideoReader::picture_on_the_way`).
struct OnTheWay<'a> {
    on: &'a mut dyn FnMut(Option<crate::Picture>) -> crate::Onward,
    want: crate::Want,
    video_levels: bool,
    min_size: (u32, u32),
    next: std::time::Instant,
    /// A newer picture is wanted: the next one on the way shows at once.
    hurry: bool,
    stopped: bool,
}

impl OnTheWay<'_> {
    /// Pictures on the way only when getting there takes a while, and not
    /// more often than the screen can show them.
    const FIRST: std::time::Duration = std::time::Duration::from_millis(25);
    const EVERY: std::time::Duration = std::time::Duration::from_millis(30);
}

impl bv_mf::Way for OnTheWay<'_> {
    fn go_on(&mut self) -> bool {
        if self.stopped {
            return false;
        }
        match (self.on)(None) {
            crate::Onward::Go => true,
            crate::Onward::Hurry => {
                self.hurry = true;
                true
            }
            crate::Onward::Stop => false,
        }
    }

    fn want(&mut self) -> bool {
        self.hurry || std::time::Instant::now() >= self.next
    }

    fn show(&mut self, f: &Arc<bv_core::VideoFrame>) {
        self.hurry = false;
        self.next = std::time::Instant::now() + Self::EVERY;
        let p = match self.want {
            crate::Want::Decoded => crate::Picture::Decoded(f.clone()),
            crate::Want::Linear => {
                let (w, h) = self.min_size;
                let factor = crate::reduction(f.width, f.height, w, h);
                crate::Picture::Linear(Arc::new(bv_color::frame_to_linear_levels(
                    f,
                    factor,
                    self.video_levels,
                )))
            }
        };
        if (self.on)(Some(p)) != crate::Onward::Go {
            self.stopped = true;
        }
    }
}

/// ProRes: the system demuxes, BananaV decodes. Every frame is a keyframe.
pub(crate) struct ProResVideo {
    reader: bv_mf::PacketReader,
    rate: FrameRate,
    /// The frame decoded last: its start and end (100 ns) and its frame.
    current: Option<(i64, i64, Arc<bv_prores::Decoded>)>,
    /// Where the reader stands (start of the next packet).
    next: Option<i64>,
    /// The last frame converted (start, reduction, levels).
    last: Option<(i64, u32, bool, Arc<ImageRgbaF32>)>,
    /// Y′CbCr as studio RGB (the project's video levels formats).
    video_levels: bool,
    /// How 4444's alpha is read (Media Properties' Alpha channel).
    alpha: AlphaReading,
}

impl ProResVideo {
    fn new(reader: bv_mf::PacketReader) -> ProResVideo {
        let rate = reader.info().frame_rate;
        ProResVideo {
            reader,
            rate,
            current: None,
            next: None,
            last: None,
            video_levels: false,
            alpha: AlphaReading::default(),
        }
    }

    /// The decoded frame showing at `t` (100 ns).
    fn decoded(&mut self, t: i64) -> Result<(i64, Arc<bv_prores::Decoded>)> {
        if let Some((s, e, d)) = &self.current
            && *s <= t
            && t < *e
        {
            return Ok((*s, d.clone()));
        }
        let nominal = self.rate.frame_duration().to_timescale(10_000_000).max(1);
        // Going on is cheaper than seeking.
        let ahead = self.next.is_some_and(|n| t >= n && t < n + 2 * 10_000_000);
        if !ahead {
            self.reader
                .seek(Tick::from_timescale(t.max(0), 10_000_000))
                .map_err(mf_err)?;
            self.next = None;
        }
        let mut found: Option<bv_mf::Packet> = None;
        while let Some(p) = self.reader.next_packet().map_err(mf_err)? {
            let end = p.time + if p.duration > 0 { p.duration } else { nominal };
            self.next = Some(end);
            let past = p.time > t;
            if past && found.is_some() {
                // The one before shows at `t`; this one waits.
                let f = found.take().expect("checked");
                let decoded = self.decode_packet(&f)?;
                // Keep the packet read too far: decoded next time.
                self.reader
                    .seek(Tick::from_timescale(p.time.max(0), 10_000_000))
                    .map_err(mf_err)?;
                self.next = Some(p.time);
                return Ok(decoded);
            }
            let contains = p.time <= t && t < end;
            found = Some(p);
            if contains || past {
                break;
            }
        }
        let f = found.ok_or_else(|| MediaError::Corrupt("no ProRes frames".into()))?;
        self.decode_packet(&f)
    }

    fn decode_packet(&mut self, p: &bv_mf::Packet) -> Result<(i64, Arc<bv_prores::Decoded>)> {
        let nominal = self.rate.frame_duration().to_timescale(10_000_000).max(1);
        let d =
            Arc::new(bv_prores::decode(&p.data).map_err(|e| MediaError::Corrupt(e.to_string()))?);
        let end = p.time + if p.duration > 0 { p.duration } else { nominal };
        self.current = Some((p.time, end, d.clone()));
        Ok((p.time, d))
    }
}

/// ProRes' H.273 color codes as BananaV's.
fn prores_color(d: &bv_prores::Decoded) -> ColorInfo {
    let mut c = ColorInfo::guess_for(d.width, d.height);
    c.range = ColorRange::Limited;
    match d.color.0 {
        1 => c.primaries = ColorPrimaries::Bt709,
        5 => c.primaries = ColorPrimaries::Bt601_625,
        6 => c.primaries = ColorPrimaries::Bt601_525,
        9 => c.primaries = ColorPrimaries::Bt2020,
        12 => c.primaries = ColorPrimaries::DisplayP3,
        _ => {}
    }
    match d.color.1 {
        16 => c.transfer = TransferCharacteristic::Pq,
        18 => c.transfer = TransferCharacteristic::Hlg,
        _ => c.transfer = TransferCharacteristic::Bt709,
    }
    match d.color.2 {
        1 => c.matrix = MatrixCoefficients::Bt709,
        5 | 6 => c.matrix = MatrixCoefficients::Bt601,
        9 => c.matrix = MatrixCoefficients::Bt2020Ncl,
        _ => {}
    }
    c
}

impl VideoReader for ProResVideo {
    fn frame_at(&mut self, t: Tick) -> Result<Arc<ImageRgbaF32>> {
        self.frame_at_scaled(t, u32::MAX, u32::MAX)
    }

    fn set_video_levels(&mut self, on: bool) {
        self.video_levels = on;
    }

    fn set_alpha(&mut self, alpha: AlphaChannel, background: [u8; 3]) {
        let alpha = AlphaReading {
            mode: alpha,
            background,
        };
        if alpha != self.alpha {
            self.alpha = alpha;
            self.last = None;
        }
    }

    fn frame_at_scaled(&mut self, t: Tick, min_w: u32, min_h: u32) -> Result<Arc<ImageRgbaF32>> {
        let (start, d) = self.decoded(t.to_timescale(10_000_000))?;
        let mut factor = 1u32;
        while factor < 8 && d.width / (factor * 2) >= min_w && d.height / (factor * 2) >= min_h {
            factor *= 2;
        }
        let levels = self.video_levels;
        if let Some((s, f, l, img)) = &self.last
            && *s == start
            && *f == factor
            && *l == levels
        {
            return Ok(img.clone());
        }
        let (w, h) = (d.width as usize, d.height as usize);
        let cw = if d.chroma444 { w } else { w.div_ceil(2) };
        let plane = |data: &[u16], pw: usize| Plane {
            width: pw,
            height: h,
            stride: pw,
            data: PlaneData::U16(data.to_vec()),
        };
        let frame = VideoFrame {
            width: d.width,
            height: d.height,
            format: if d.chroma444 {
                PixelFormat::Yuv444P10
            } else {
                PixelFormat::Yuv422P10
            },
            color: prores_color(&d),
            planes: vec![plane(&d.y, w), plane(&d.cb, cw), plane(&d.cr, cw)],
            pts: Tick::ZERO,
            duration: Tick::ZERO,
        };
        let mut img = bv_color::frame_to_linear_levels(&frame, factor, levels);
        let mode = self.alpha.mode;
        if let Some(alpha) = &d.alpha
            && mode != AlphaChannel::None
        {
            // 4444's alpha: averaged like the color, then premultiplied —
            // or, premultiplied already (against the background, dirty),
            // the colors divided by it in their own values first.
            let k = factor as usize;
            let ow = img.width as usize;
            let back = self.alpha.background.map(|v| v as f32 / 255.0);
            let dirty = mode == AlphaChannel::PremultipliedDirty;
            let premultiplied = dirty || mode == AlphaChannel::Premultiplied;
            for (i, px) in img.data.iter_mut().enumerate() {
                let (ox, oy) = (i % ow, i / ow);
                let (mut acc, mut n) = (0f32, 0f32);
                for y in oy * k..((oy + 1) * k).min(h) {
                    for x in ox * k..((ox + 1) * k).min(w) {
                        acc += alpha[y * w + x] as f32 / 65535.0;
                        n += 1.0;
                    }
                }
                let a = acc / n.max(1.0);
                *px = if !premultiplied {
                    [px[0] * a, px[1] * a, px[2] * a, a]
                } else if a <= 0.0 {
                    [0.0; 4]
                } else {
                    let un = |v: f32, b: f32| {
                        let e = bv_color::transfer::linear_to_bt709(v.max(0.0));
                        let e = if dirty { e - (1.0 - a) * b } else { e };
                        bv_color::transfer::bt709_to_linear((e / a).clamp(0.0, 1.0)) * a
                    };
                    [
                        un(px[0], back[0]),
                        un(px[1], back[1]),
                        un(px[2], back[2]),
                        a,
                    ]
                };
            }
        }
        let img = Arc::new(img);
        self.last = Some((start, factor, levels, img.clone()));
        Ok(img)
    }
}

/// Audio the system decodes (or AC-3 decoded by BananaV on the system's
/// packets).
pub(crate) struct SystemAudio {
    dec: bv_mf::AudioDecoder,
}

impl SystemAudio {
    fn open(path: &str, index: usize) -> Result<SystemAudio> {
        Ok(SystemAudio {
            dec: bv_mf::AudioDecoder::open_stream(path, index).map_err(mf_err)?,
        })
    }
}

impl AudioReader for SystemAudio {
    fn sample_rate(&self) -> SampleRate {
        self.dec.info().sample_rate
    }

    fn channels(&self) -> usize {
        self.dec.info().channels as usize
    }

    fn read(&mut self, start: i64, frames: usize) -> Result<AudioBuffer> {
        self.dec.read(start, frames).map_err(mf_err)
    }
}

/// BananaV's AC-3 decoder fed by the system's demuxers.
#[derive(Default)]
struct Ac3(bv_ac3::Decoder);

impl bv_mf::PacketDecoder for Ac3 {
    fn decode(&mut self, packet: &[u8], out: &mut Vec<f32>) -> std::result::Result<(), String> {
        self.0.decode(packet, out).map_err(|e| e.to_string())
    }

    fn reset(&mut self) {
        self.0.reset();
    }

    fn sample_rate(&self) -> u32 {
        self.0.sample_rate()
    }

    fn channels(&self) -> usize {
        self.0.channels()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_by_content_and_name() {
        assert_eq!(
            kind("a.jpg", &[0xff, 0xd8, 0xff, 0xe0]),
            Kind::Image(Format::Jpeg)
        );
        assert_eq!(kind("a.png", b"\x89PNG\r\n\x1a\n"), Kind::Png);
        assert_eq!(kind("a.gif", b"GIF89a......"), Kind::Gif);
        assert_eq!(kind("a.webp", b"RIFF\0\0\0\0WEBPVP8 "), Kind::SystemImage);
        assert_eq!(kind("photo.CR2", b"II*\0\x10\0\0\0CR"), Kind::SystemImage);
        assert_eq!(kind("a.dts", &[0x7f, 0xfe, 0x80, 0x01, 0, 0]), Kind::Dts);
        let heic = [&[0, 0, 0, 24][..], b"ftypheic", &[0, 0, 0, 0], b"mif1heic"].concat();
        assert_eq!(kind("a.heic", &heic), Kind::SystemImage);
        let mp4 = [&[0, 0, 0, 24][..], b"ftypisom", &[0, 0, 2, 0], b"isomiso2"].concat();
        assert_eq!(kind("a.mp4", &mp4), Kind::Other);
    }

    /// A picture file read as its Media Properties say: turned a quarter
    /// clockwise, its info's size swapped and its pixels moved; with no
    /// alpha, its see-through pixel opaque — through `open_video` and the
    /// pool, which opens its reader again when the settings change.
    #[test]
    fn a_picture_read_as_its_properties_say() {
        use bv_timeline::{AlphaChannel, MediaRotation};
        let dir = std::env::temp_dir().join(format!("bv-reading-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // 4 × 2: red, its top left pixel half see-through.
        let mut px = [255u8, 0, 0, 255].repeat(8);
        px[3] = 128;
        let file = dir.join("turn.png");
        std::fs::write(&file, bv_png::encode_rgba8(4, 2, &px, 6)).unwrap();
        let path = file.to_string_lossy().to_string();
        let mut info = crate::probe_file(&path).unwrap();
        let mut pool = crate::VideoPool::new();
        let ask = |w: u32, h: u32| crate::Ask {
            time: Tick::ZERO,
            min_size: (w, h),
            want: crate::Want::Linear,
            span: None,
            video_levels: false,
        };
        let linear = |p: crate::Picture| match p {
            crate::Picture::Linear(img) => img,
            crate::Picture::Decoded(_) => panic!("a picture in linear light"),
        };
        let img = linear(pool.picture(0, &info, ask(4, 2)).unwrap());
        assert_eq!((img.width, img.height), (4, 2));
        assert!((img.data[0][3] - 128.0 / 255.0).abs() < 1e-3);
        // Turned: 2 × 4, the top left pixel now at the top right.
        let v = info.video.unwrap().turned_to(MediaRotation::Clockwise);
        info.video = Some(v);
        assert_eq!((v.width, v.height), (2, 4));
        let img = linear(pool.picture(0, &info, ask(2, 4)).unwrap());
        assert_eq!((img.width, img.height), (2, 4));
        assert!(
            (img.data[1][3] - 128.0 / 255.0).abs() < 1e-3,
            "{:?}",
            img.data
        );
        // No alpha: opaque.
        let mut v = info.video.unwrap();
        v.reading.alpha = AlphaChannel::None;
        info.video = Some(v);
        let img = linear(pool.picture(0, &info, ask(2, 4)).unwrap());
        assert_eq!(img.data[1][3], 1.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Three pictures and a gap: the run is three long; asked for four,
    /// the fourth frame is black.
    #[test]
    fn image_sequences() {
        let dir = std::env::temp_dir().join(format!("bv-seq-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let colors = [[255u8, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]];
        for (i, c) in colors.iter().enumerate() {
            let px: Vec<u8> = c.repeat(4 * 2);
            let png = bv_png::encode_rgba8(4, 2, &px, 6);
            std::fs::write(dir.join(format!("shot_{:03}.png", 7 + i)), png).unwrap();
        }
        let first = dir.join("shot_007.png").to_string_lossy().to_string();
        assert_eq!(sequence_length(&first), 3);
        let info = probe_sequence(&first, 4, FrameRate::FPS_25).unwrap();
        assert_eq!(info.name, "shot_007–010.png");
        assert_eq!(info.duration, FrameRate::FPS_25.frame_start(4));
        assert!(!info.still);
        let mut r = SequenceVideo::new(&first, 4, FrameRate::FPS_25);
        let at = |n: i64| FrameRate::FPS_25.frame_start(n);
        let px = |img: &ImageRgbaF32| img.data[0];
        assert!(px(&r.frame_at(at(0)).unwrap())[0] > 0.9, "red");
        assert!(px(&r.frame_at(at(1)).unwrap())[1] > 0.9, "green");
        assert!(px(&r.frame_at(at(2)).unwrap())[2] > 0.9, "blue");
        assert_eq!(
            px(&r.frame_at(at(3)).unwrap()),
            [0.0, 0.0, 0.0, 1.0],
            "missing: black"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A picture's values are video's: made light and encoded again with
    /// video's curve (the preview's and the renders') they are the file's,
    /// as VEGAS shows them — a grey of 128 stays 128.
    #[test]
    fn pictures_values_are_videos() {
        let p = Picture {
            width: 2,
            height: 2,
            data: PictureData::Rgba8([128u8, 64, 200, 255].repeat(4)),
            linear: false,
            premultiplied: false,
        };
        let img = to_linear(&p, 1, AlphaReading::default());
        let back =
            img.data[0].map(|c| (bv_color::transfer::linear_to_bt709(c) * 255.0).round() as u8);
        assert_eq!(back, [128, 64, 200, 255]);
        let p = Picture {
            data: PictureData::Rgba16([32768u16, 1000, 60000, 65535].repeat(4)),
            ..p
        };
        let img = to_linear(&p, 1, AlphaReading::default());
        let back =
            img.data[3].map(|c| (bv_color::transfer::linear_to_bt709(c) * 65535.0).round() as u16);
        assert_eq!(back, [32768, 1000, 60000, 65535]);
    }

    /// The Alpha channel read as a media's Properties say: a half
    /// transparent grey of 128 is straight (its color 128) as the file
    /// says, premultiplied (its color 255, 128 divided by the alpha), dirty
    /// against white (the white taken out first), or not there (opaque).
    #[test]
    fn the_alpha_channel_is_read_as_its_properties_say() {
        let p = Picture {
            width: 1,
            height: 1,
            data: PictureData::Rgba8(vec![128, 128, 128, 128]),
            linear: false,
            premultiplied: false,
        };
        let a = 128.0 / 255.0;
        let read = |mode: AlphaChannel, background: [u8; 3]| {
            to_linear(&p, 1, AlphaReading { mode, background }).data[0]
        };
        // Back to the picture's values, per unit of alpha (straight).
        let value =
            |px: [f32; 4]| (bv_color::transfer::linear_to_bt709(px[0] / px[3]) * 255.0).round();
        let straight = read(AlphaChannel::Straight, [0; 3]);
        assert_eq!(straight, read(AlphaChannel::Undefined, [0; 3]));
        assert_eq!((value(straight), straight[3]), (128.0, a));
        let pre = read(AlphaChannel::Premultiplied, [0; 3]);
        assert_eq!((value(pre), pre[3]), (255.0, a));
        // Against white: (128 − (1 − a) × 255) ⁄ a, nearly black.
        let dirty = read(AlphaChannel::PremultipliedDirty, [255; 3]);
        assert!(value(dirty) <= 2.0, "{dirty:?}");
        let none = read(AlphaChannel::None, [0; 3]);
        assert_eq!((value(none), none[3]), (128.0, 1.0));
    }

    #[test]
    fn gif_rates() {
        let rate = |delays: &[u32]| {
            let mut data = b"GIF89a".to_vec();
            data.extend_from_slice(&[1, 0, 1, 0, 0, 0, 0]);
            for d in delays {
                data.extend_from_slice(&[0x21, 0xf9, 4, 0, *d as u8, 0, 0, 0]);
                data.extend_from_slice(&[0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0x80, 0, 0, 0, 0, 0, 0]);
                data.extend_from_slice(&[2, 2, 0x44, 0x01, 0]);
            }
            data.push(0x3b);
            gif_rate(&Gif::parse(&data).unwrap())
        };
        assert_eq!(rate(&[4, 4, 4]), FrameRate::new(25, 1));
        assert_eq!(rate(&[10, 10, 7]), FrameRate::new(10, 1));
        assert_eq!(rate(&[3, 3]), FrameRate::new(100, 3));
    }
}
