//! The system's codecs: Media Foundation on Windows, FFmpeg on Linux and
//! macOS.
//!
//! Reads any file the system can open (MP4/MOV with H.264 or HEVC, AAC,
//! MP3, WMV, AVI, MKV…), giving pictures as [`VideoFrame`]s (Y′CbCr planes
//! of 8 or 10 bits) and sound as floats, decoded on the graphics card when
//! it can; and encodes with the system's encoders, the processor's and the
//! graphics cards' (NVENC, AMD, Quick Sync on Windows; x264, x265,
//! SVT-AV1, NVENC and VA-API on Linux; VideoToolbox, the Mac's own, on
//! macOS).
//!
//! Elsewhere, and on Linux or macOS built with `--cfg bv_no_ffmpeg`, every
//! function returns [`Error::Unavailable`].

#[cfg(not(any(
    windows,
    all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg))
)))]
use bv_core::AudioBuffer;
use bv_core::{FrameRate, SampleRate, Tick, VideoFrame};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// This system has no media framework BananaV uses.
    Unavailable,
    /// Windows failed to open or decode.
    Windows(String),
    /// FFmpeg failed to open or decode.
    Ffmpeg(String),
    /// The file has no stream of this kind.
    NoStream,
    /// The system does not read files of this kind.
    UnsupportedFile,
    /// The decoding was stopped (another picture is wanted now).
    Interrupted,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unavailable if cfg!(windows) => f.write_str("Media Foundation unavailable"),
            Error::Unavailable => f.write_str("FFmpeg unavailable"),
            Error::Windows(s) => write!(f, "Media Foundation: {s}"),
            Error::Ffmpeg(s) => write!(f, "FFmpeg: {s}"),
            Error::NoStream => f.write_str("the file has no such stream"),
            Error::UnsupportedFile if cfg!(windows) => {
                f.write_str("Windows does not read files of this kind")
            }
            Error::UnsupportedFile => f.write_str("FFmpeg does not read files of this kind"),
            Error::Interrupted => f.write_str("interrupted"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoInfo {
    /// The size the frames come in (non-square pixels already widened, as
    /// DV's).
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    /// The compressed format, as Windows names it: the subtype's FourCC
    /// (`codec::H264`, `codec::PRORES_422`…).
    pub codec: u32,
    /// Windows has a decoder for it.
    pub decodable: bool,
    /// The shape of its pixels, width : height (1 : 1 square; HDV's
    /// 1440 × 1080 frames have 4 : 3 ones).
    pub pixel_aspect: (u32, u32),
    /// Its fields' order, as the file says (progressive when it does not).
    pub field_order: bv_core::FieldOrder,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioInfo {
    pub sample_rate: SampleRate,
    pub channels: u16,
    /// The compressed format: the subtype's first 32 bits (a WAVE format
    /// tag, a FourCC or `codec::AC3`).
    pub codec: u32,
    /// Windows has a decoder for it.
    pub decodable: bool,
}

/// Codes of some compressed formats in [`VideoInfo::codec`] and
/// [`AudioInfo::codec`].
pub mod codec {
    /// A FourCC as Windows keeps it in a subtype.
    pub const fn fourcc(name: &[u8; 4]) -> u32 {
        u32::from_le_bytes(*name)
    }

    pub const H264: u32 = fourcc(b"H264");
    pub const HEVC: u32 = fourcc(b"HEVC");
    pub const PRORES_PROXY: u32 = fourcc(b"apco");
    pub const PRORES_LT: u32 = fourcc(b"apcs");
    pub const PRORES_422: u32 = fourcc(b"apcn");
    pub const PRORES_HQ: u32 = fourcc(b"apch");
    pub const PRORES_4444: u32 = fourcc(b"ap4h");
    pub const PRORES_4444_XQ: u32 = fourcc(b"ap4x");
    /// Dolby Digital (AC-3), as Windows' demuxers mark it.
    pub const AC3: u32 = 0xe06d_802c;
    /// AC-3 in AVI and WAV (`WAVE_FORMAT_DOLBY_AC3`).
    pub const AC3_WAVE: u32 = 0x2000;
    /// Dolby Digital Plus (E-AC-3).
    pub const EAC3: u32 = 0xa7fb_87af;
    /// DTS, as Windows' demuxers mark it.
    pub const DTS: u32 = 0xe06d_8033;
    /// DTS in AVI and WAV (`WAVE_FORMAT_DTS`).
    pub const DTS_WAVE: u32 = 0x2001;

    /// The FourCC a QuickTime sample entry names: Windows' MPEG-4 reader
    /// keeps the ones it does not know backwards (`ncpa` for `apcn`).
    pub fn normalized(code: u32) -> u32 {
        let known = |c: u32| NAMED.iter().any(|(k, _)| *k == c);
        if !known(code) && known(code.swap_bytes()) {
            code.swap_bytes()
        } else {
            code
        }
    }

    /// Formats with a name for messages.
    const NAMED: [(u32, &str); 28] = [
        (H264, "H.264"),
        (HEVC, "HEVC"),
        (PRORES_PROXY, "ProRes 422 Proxy"),
        (PRORES_LT, "ProRes 422 LT"),
        (PRORES_422, "ProRes 422"),
        (PRORES_HQ, "ProRes 422 HQ"),
        (PRORES_4444, "ProRes 4444"),
        (PRORES_4444_XQ, "ProRes 4444 XQ"),
        (fourcc(b"aprn"), "ProRes RAW"),
        (fourcc(b"aprh"), "ProRes RAW HQ"),
        (fourcc(b"AVdn"), "DNxHD"),
        (fourcc(b"AVdh"), "DNxHR"),
        (fourcc(b"CFHD"), "GoPro CineForm"),
        (fourcc(b"rle "), "QuickTime Animation"),
        (fourcc(b"HFYU"), "HuffYUV"),
        (fourcc(b"FFV1"), "FFV1"),
        (fourcc(b"brr1"), "Blackmagic RAW"),
        (fourcc(b"xd5e"), "XDCAM HD422"),
        (fourcc(b"ai5p"), "AVC-Intra"),
        (fourcc(b"ai1p"), "AVC-Intra"),
        (fourcc(b"dvh5"), "DVCPRO HD"),
        (fourcc(b"dvhp"), "DVCPRO HD"),
        (fourcc(b"r210"), "10-bit RGB"),
        (fourcc(b"v210"), "10-bit uncompressed"),
        (AC3, "Dolby Digital (AC-3)"),
        (AC3_WAVE, "Dolby Digital (AC-3)"),
        (EAC3, "Dolby Digital Plus (E-AC-3)"),
        (DTS, "DTS"),
    ];

    /// Whether `code` is one of the ProRes flavours.
    pub fn is_prores(code: u32) -> bool {
        [
            PRORES_PROXY,
            PRORES_LT,
            PRORES_422,
            PRORES_HQ,
            PRORES_4444,
            PRORES_4444_XQ,
        ]
        .contains(&normalized(code))
    }

    /// Whether `code` is Dolby Digital (AC-3).
    pub fn is_ac3(code: u32) -> bool {
        code == AC3 || code == AC3_WAVE
    }

    /// The name of a known format ("H.264", "ProRes 422"…), for messages.
    pub fn name(code: u32) -> String {
        let code = normalized(code);
        if let Some((_, n)) = NAMED.iter().find(|(k, _)| *k == code) {
            return (*n).to_string();
        }
        match code {
            DTS_WAVE => "DTS".into(),
            c => {
                let b = c.to_le_bytes();
                if b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b' ') {
                    String::from_utf8_lossy(&b).trim().to_string()
                } else {
                    format!("0x{c:08x}")
                }
            }
        }
    }
}

/// Decodes the packets of an audio stream Windows reads but has no decoder
/// for (AC-3 on Windows editions without Dolby's decoder).
pub trait PacketDecoder: Send {
    /// Appends the interleaved samples of `packet` (one or more whole frames).
    fn decode(&mut self, packet: &[u8], out: &mut Vec<f32>) -> std::result::Result<(), String>;
    /// Forgets what the earlier packets left (after a seek).
    fn reset(&mut self);
    /// Of the last packet decoded.
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> usize;
}

/// The video of a Windows Media file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AsfVideo {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    /// Bits per second.
    pub bitrate: u32,
}

/// The audio of a Windows Media file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AsfAudio {
    pub sample_rate: u32,
    pub channels: u16,
    /// Bits per second (the closest the encoder offers).
    pub bitrate: u32,
    /// WMA Lossless instead of WMA 9.
    pub lossless: bool,
}

/// A Windows Media file (`.wmv` with video, `.wma` without).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AsfSettings {
    pub video: Option<AsfVideo>,
    pub audio: Option<AsfAudio>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Probe {
    pub duration: Tick,
    pub video: Option<VideoInfo>,
    /// The first audio stream.
    pub audio: Option<AudioInfo>,
    /// Every audio stream, in file order (the first one included).
    pub audio_streams: Vec<AudioInfo>,
}

/// A compressed format a Windows encoder may write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Codec {
    H264,
    Hevc,
    Av1,
    Vp9,
    /// Windows Media Video 9.
    Wmv,
    Aac,
    Mp3,
    /// Dolby Digital.
    Ac3,
    /// MPEG-1 Layer II audio (DVD's and MPEG-2 files' MPEG audio).
    Mp2,
    /// Windows Media Audio 9 (standard).
    Wma,
    WmaLossless,
    Flac,
    Alac,
    Opus,
}

impl Codec {
    pub const ALL: [Codec; 14] = [
        Codec::H264,
        Codec::Hevc,
        Codec::Av1,
        Codec::Vp9,
        Codec::Wmv,
        Codec::Aac,
        Codec::Mp3,
        Codec::Ac3,
        Codec::Mp2,
        Codec::Wma,
        Codec::WmaLossless,
        Codec::Flac,
        Codec::Alac,
        Codec::Opus,
    ];
}

/// Who runs an encoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Vendor {
    /// The processor (a Windows or installed software encoder).
    Software,
    /// NVIDIA NVENC.
    Nvidia,
    /// AMD VCE/AMF.
    Amd,
    /// Intel Quick Sync Video.
    Intel,
    /// Qualcomm Adreno (Snapdragon computers).
    Qualcomm,
    /// The Mac's own media engine (VideoToolbox).
    Apple,
    OtherHardware,
}

/// One encoder of the system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncoderInfo {
    pub codec: Codec,
    pub vendor: Vendor,
    /// As Windows lists it ("NVIDIA H.264 Encoder MFT").
    pub name: String,
    pub hardware: bool,
    /// Its position in Windows' list of encoders of `codec` of its kind
    /// (hardware or software).
    pub index: usize,
}

/// Samples the AAC encoder puts before the sound (its delay), skipped by
/// the MP4 file's edit list: none with Windows' encoder.
#[cfg(not(all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg))))]
pub const AAC_PRIMING: u32 = 0;

/// Samples the AAC encoder puts before the sound (its delay), skipped by
/// the MP4 file's edit list: FFmpeg's encoder primes one frame.
#[cfg(all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg)))]
pub const AAC_PRIMING: u32 = 1024;

/// A compressed frame or audio packet from an encoder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    pub data: Vec<u8>,
    /// Presentation time, in 100 ns units.
    pub time: i64,
    /// In 100 ns units.
    pub duration: i64,
    /// Decode time, when the encoder gives one.
    pub decode_time: Option<i64>,
    pub keyframe: bool,
}

/// How a video encoder spends bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RateControl {
    /// Constant bit rate.
    Cbr,
    /// Variable bit rate around the average, up to the maximum.
    Vbr,
    /// Constant quality (0–100): the bit rate follows the picture.
    Quality(u32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoEncoderSettings {
    /// H.264, HEVC, AV1 or VP9.
    pub codec: Codec,
    /// Which encoder: the processor's or a graphics card's.
    pub vendor: Vendor,
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    /// Bits per second.
    pub bitrate: u32,
    pub max_bitrate: u32,
    pub rate_control: RateControl,
    /// Frames from one keyframe to the next.
    pub gop: u32,
    /// 10 bits per sample (Main 10) instead of 8.
    pub ten_bit: bool,
    /// Interlaced frames: which field comes first (the encoder told so
    /// where it takes it).
    pub field_order: bv_core::FieldOrder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioEncoderSettings {
    /// AAC, MP3, ALAC…
    pub codec: Codec,
    pub sample_rate: u32,
    pub channels: u16,
    /// Bits per second (lossy formats).
    pub bitrate: u32,
    /// Bits of the PCM fed to the encoder (16 or 24).
    pub bits: u16,
}

pub mod camera;

#[cfg(any(
    windows,
    all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg))
))]
mod common;

#[cfg(all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg)))]
#[allow(unsafe_code)]
mod ff;

#[cfg_attr(not(windows), allow(dead_code))]
mod mp4;

#[cfg_attr(not(windows), allow(dead_code))]
mod mp3;

#[cfg(windows)]
#[allow(unsafe_code)]
mod encoders;

#[cfg(windows)]
#[allow(unsafe_code)]
mod encode;

#[cfg(windows)]
#[allow(unsafe_code)]
mod d3d;

#[cfg(windows)]
#[allow(unsafe_code)]
mod dxva;

#[cfg(windows)]
#[allow(unsafe_code)]
mod imp;

#[cfg(windows)]
#[allow(unsafe_code)]
mod writer;

/// What decoding toward a frame does on the way there (see
/// `VideoDecoder::frame_at_with`): after a seek, the frames from the
/// keyframe on.
pub trait Way {
    /// `false` stops the decoding ([`Error::Interrupted`]): another frame
    /// is wanted now.
    fn go_on(&mut self) -> bool {
        true
    }

    /// Whether a picture on the way is wanted now: it then goes to
    /// [`show`](Self::show). Others are not even copied out of the decoder.
    fn want(&mut self) -> bool {
        false
    }

    fn show(&mut self, frame: &std::sync::Arc<VideoFrame>) {
        let _ = frame;
    }
}

/// Decoding that just goes to the frame.
#[derive(Debug)]
pub struct Straight;

impl Way for Straight {}

/// Whether video is decoded on the graphics card (Options › Preferences ›
/// File I/O): Windows' decoders drive the card's video engine (DXVA) on
/// NVIDIA, AMD and Intel cards alike, and decode on the processor what the
/// card cannot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum HardwareDecoding {
    /// On the processor only.
    #[default]
    Off,
    /// On the fastest graphics card.
    Auto,
    /// On the card of this name (as [`decode_adapters`] lists it).
    Card(String),
}

static HARDWARE: std::sync::RwLock<HardwareDecoding> =
    std::sync::RwLock::new(HardwareDecoding::Off);

/// `BV_HARDWARE_DECODING` (`auto`, `off` or a card's name) sets the
/// starting value: to run the tests on the card.
fn from_env() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let Ok(v) = std::env::var("BV_HARDWARE_DECODING") else {
            return;
        };
        let h = match v.to_ascii_lowercase().as_str() {
            "auto" | "on" => HardwareDecoding::Auto,
            "" | "off" => HardwareDecoding::Off,
            _ => HardwareDecoding::Card(v),
        };
        if let Ok(mut s) = HARDWARE.write() {
            *s = h;
        }
    });
}

/// How decoders opened from now on decode (the processor until set).
pub fn set_hardware_decoding(h: HardwareDecoding) {
    from_env();
    if let Ok(mut s) = HARDWARE.write() {
        *s = h;
    }
}

pub fn hardware_decoding() -> HardwareDecoding {
    from_env();
    HARDWARE.read().map(|s| s.clone()).unwrap_or_default()
}

thread_local! {
    static PROCESSOR_ONLY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Decoders opened by this thread from now on decode on the processor,
/// whatever [`set_hardware_decoding`] says (thumbnails: many small decoders
/// would fill the card's memory).
pub fn processor_only_on_this_thread() {
    PROCESSOR_ONLY.set(true);
}

/// The graphics cards that can decode video, the fastest first.
pub fn decode_adapters() -> Vec<String> {
    #[cfg(windows)]
    {
        dxva::adapters()
    }
    #[cfg(all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg)))]
    {
        ff::hw::adapters()
    }
    #[cfg(not(any(
        windows,
        all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg))
    )))]
    {
        Vec::new()
    }
}

/// A picture read by the system's codecs (WebP, AVIF, HEIF…): RGBA of 16
/// bits per channel, not premultiplied, in the picture's own transfer
/// (sRGB for most).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Still {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u16>,
}

#[cfg(windows)]
pub use encode::{AudioEncoder, VideoEncoder};
#[cfg(windows)]
pub use encoders::encoders;
#[cfg(windows)]
pub use imp::{AudioDecoder, PacketReader, VideoDecoder, probe};
#[cfg(windows)]
pub use writer::AsfWriter;

#[cfg(all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg)))]
pub use ff::{
    AudioDecoder, AudioEncoder, PacketReader, VideoDecoder, VideoEncoder, encoders, probe,
    read_still, still_size,
};

/// Every encoder of the system: none without a media framework.
#[cfg(not(any(
    windows,
    all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg))
)))]
pub fn encoders() -> &'static [EncoderInfo] {
    &[]
}

/// The size of a picture the system reads (outside Windows, which reads
/// them with its Imaging Component).
#[cfg(not(any(
    windows,
    all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg))
)))]
pub fn still_size(_path: &str) -> Result<(u32, u32)> {
    Err(Error::Unavailable)
}

/// A picture the system reads (outside Windows, which reads them with its
/// Imaging Component).
#[cfg(not(any(
    windows,
    all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg))
)))]
pub fn read_still(_path: &str) -> Result<Still> {
    Err(Error::Unavailable)
}

#[cfg(not(any(
    windows,
    all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg))
)))]
mod imp_stub {
    use super::*;

    pub fn probe(_path: &str) -> Result<Probe> {
        Err(Error::Unavailable)
    }

    #[derive(Debug)]
    pub struct VideoDecoder;

    impl VideoDecoder {
        pub fn open(_path: &str) -> Result<VideoDecoder> {
            Err(Error::Unavailable)
        }

        pub fn on_graphics_card(&self) -> bool {
            false
        }

        pub fn info(&self) -> VideoInfo {
            VideoInfo {
                width: 0,
                height: 0,
                frame_rate: FrameRate::FPS_30,
                codec: 0,
                decodable: false,
                pixel_aspect: (1, 1),
                field_order: bv_core::FieldOrder::Progressive,
            }
        }

        pub fn frame_at_with(
            &mut self,
            _t: Tick,
            _way: &mut dyn crate::Way,
        ) -> Result<std::sync::Arc<VideoFrame>> {
            Err(Error::Unavailable)
        }

        pub fn frame_at(&mut self, _t: Tick) -> Result<std::sync::Arc<VideoFrame>> {
            Err(Error::Unavailable)
        }
    }

    #[derive(Debug)]
    pub struct AudioDecoder;

    impl AudioDecoder {
        pub fn open(_path: &str) -> Result<AudioDecoder> {
            Err(Error::Unavailable)
        }

        pub fn open_stream(_path: &str, _index: usize) -> Result<AudioDecoder> {
            Err(Error::Unavailable)
        }

        pub fn open_with(
            _path: &str,
            _index: usize,
            _decoder: Box<dyn PacketDecoder>,
        ) -> Result<AudioDecoder> {
            Err(Error::Unavailable)
        }

        pub fn info(&self) -> AudioInfo {
            AudioInfo {
                sample_rate: SampleRate::HZ_48000,
                channels: 2,
                codec: 0,
                decodable: false,
            }
        }

        pub fn read(&mut self, _start: i64, _frames: usize) -> Result<AudioBuffer> {
            Err(Error::Unavailable)
        }
    }

    #[derive(Debug)]
    pub struct PacketReader;

    impl PacketReader {
        pub fn video(_path: &str) -> Result<PacketReader> {
            Err(Error::Unavailable)
        }

        pub fn info(&self) -> VideoInfo {
            VideoInfo {
                width: 0,
                height: 0,
                frame_rate: FrameRate::FPS_30,
                codec: 0,
                decodable: false,
                pixel_aspect: (1, 1),
                field_order: bv_core::FieldOrder::Progressive,
            }
        }

        pub fn seek(&mut self, _t: Tick) -> Result<()> {
            Err(Error::Unavailable)
        }

        pub fn next_packet(&mut self) -> Result<Option<Packet>> {
            Err(Error::Unavailable)
        }
    }

    #[derive(Debug)]
    pub struct VideoEncoder;

    impl VideoEncoder {
        pub fn new(_s: VideoEncoderSettings) -> Result<VideoEncoder> {
            Err(Error::Unavailable)
        }

        pub fn name(&self) -> &str {
            ""
        }

        pub fn sequence_header(&self) -> Option<Vec<u8>> {
            None
        }

        pub fn encode(&mut self, _f: &VideoFrame, _t: i64, _d: i64) -> Result<Vec<Packet>> {
            Err(Error::Unavailable)
        }

        pub fn encode_packed(&mut self, _data: &[u8], _t: i64, _d: i64) -> Result<Vec<Packet>> {
            Err(Error::Unavailable)
        }

        pub fn finish(&mut self) -> Result<Vec<Packet>> {
            Err(Error::Unavailable)
        }
    }

    #[derive(Debug)]
    pub struct AudioEncoder;

    impl AudioEncoder {
        pub fn new(_s: AudioEncoderSettings) -> Result<AudioEncoder> {
            Err(Error::Unavailable)
        }

        pub fn name(&self) -> &str {
            ""
        }

        pub fn config(&self) -> &[u8] {
            &[]
        }

        pub fn bitrate(&self) -> u32 {
            0
        }

        pub fn encode(&mut self, _s: &[f32]) -> Result<Vec<Packet>> {
            Err(Error::Unavailable)
        }

        pub fn finish(&mut self) -> Result<Vec<Packet>> {
            Err(Error::Unavailable)
        }
    }
}

/// Windows Media files are written by Windows only.
#[cfg(not(windows))]
mod asf_stub {
    use super::*;

    #[derive(Debug)]
    pub struct AsfWriter;

    impl AsfWriter {
        pub fn create(_path: &str, _s: AsfSettings) -> Result<AsfWriter> {
            Err(Error::Unavailable)
        }

        pub fn write_video(&mut self, _f: &VideoFrame, _t: Tick, _d: Tick) -> Result<()> {
            Err(Error::Unavailable)
        }

        pub fn write_audio(&mut self, _s: &[f32], _t: Tick, _d: Tick) -> Result<()> {
            Err(Error::Unavailable)
        }

        pub fn finish(self) -> Result<()> {
            Err(Error::Unavailable)
        }
    }
}

#[cfg(not(windows))]
pub use asf_stub::AsfWriter;
#[cfg(not(any(
    windows,
    all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg))
)))]
pub use imp_stub::{AudioDecoder, AudioEncoder, PacketReader, VideoDecoder, VideoEncoder, probe};
