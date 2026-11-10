//! The kinds of file BananaV renders to, what each holds and which
//! encoders of this computer can write it.

use serde::{Deserialize, Serialize};

/// The groups of the Render dialog's tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Category {
    Video,
    ImageSequence,
    Audio,
}

/// A kind of file the Render dialog writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileFormat {
    /// H.264 video and AAC audio in MP4.
    AvcMp4,
    /// HEVC (H.265) video and AAC audio in MP4.
    HevcMp4,
    /// AV1 video and AAC audio in MP4.
    Av1Mp4,
    /// Apple ProRes video and PCM audio in QuickTime MOV.
    ProRes,
    /// Windows Media Video 9 and Windows Media Audio in ASF.
    Wmv,
    /// MPEG-1 for Video CDs: BananaV's MPEG-1 video and MPEG audio
    /// (Layer II) in a system stream.
    Mpeg1,
    /// MPEG-2 for DVD and Blu-ray.
    Mpeg2,
    Bmp,
    /// DirectDraw Surface.
    Dds,
    /// SMPTE Digital Picture Exchange, 10-bit.
    Dpx,
    /// OpenEXR, half or full float.
    Exr,
    /// HEIF image with HEVC.
    Heic,
    Jpeg,
    Png,
    Tiff,
    /// JPEG XR (Windows Media Photo).
    WmPhoto,
    /// AAC audio in an MPEG-4 file (.m4a).
    Aac,
    /// Dolby Digital.
    Ac3,
    Aiff,
    Flac,
    Mp3,
    OggVorbis,
    /// Microsoft Wave.
    Wav,
    /// Wave64 (64-bit Wave, for files over 4 GB).
    Wave64,
    /// Windows Media Audio 9.
    Wma,
}

/// Whether a format can be rendered on this computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    Ready,
    /// Windows has no encoder for it (an extension or driver is missing).
    NoEncoder,
    /// BananaV does not write it yet.
    Soon,
}

/// Who encodes the video: the processor or a graphics card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Encoder {
    /// The processor (Windows' software encoder).
    #[default]
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
}

impl Encoder {
    /// How VEGAS-style template names tag it ("NVENC").
    pub fn short_name(self) -> &'static str {
        match self {
            Encoder::Software => "CPU",
            Encoder::Nvidia => "NVENC",
            Encoder::Amd => "AMD VCE",
            Encoder::Intel => "Intel QSV",
            Encoder::Qualcomm => "Qualcomm",
            Encoder::Apple => "VideoToolbox",
        }
    }

    pub(crate) fn vendor(self) -> bv_mf::Vendor {
        match self {
            Encoder::Software => bv_mf::Vendor::Software,
            Encoder::Nvidia => bv_mf::Vendor::Nvidia,
            Encoder::Amd => bv_mf::Vendor::Amd,
            Encoder::Intel => bv_mf::Vendor::Intel,
            Encoder::Qualcomm => bv_mf::Vendor::Qualcomm,
            Encoder::Apple => bv_mf::Vendor::Apple,
        }
    }

    fn from_vendor(v: bv_mf::Vendor) -> Option<Encoder> {
        match v {
            bv_mf::Vendor::Software => Some(Encoder::Software),
            bv_mf::Vendor::Nvidia => Some(Encoder::Nvidia),
            bv_mf::Vendor::Amd => Some(Encoder::Amd),
            bv_mf::Vendor::Intel => Some(Encoder::Intel),
            bv_mf::Vendor::Qualcomm => Some(Encoder::Qualcomm),
            bv_mf::Vendor::Apple => Some(Encoder::Apple),
            bv_mf::Vendor::OtherHardware => None,
        }
    }
}

/// ProRes flavors, by data rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ProResProfile {
    Proxy,
    Lt,
    #[default]
    Standard,
    Hq,
    P4444,
    P4444Xq,
}

impl ProResProfile {
    pub const ALL: [ProResProfile; 6] = [
        ProResProfile::Standard,
        ProResProfile::Hq,
        ProResProfile::Lt,
        ProResProfile::Proxy,
        ProResProfile::P4444,
        ProResProfile::P4444Xq,
    ];

    /// "422 (HQ)", as template names write it.
    pub fn name(self) -> &'static str {
        match self {
            ProResProfile::Proxy => "422 (Proxy)",
            ProResProfile::Lt => "422 (LT)",
            ProResProfile::Standard => "422",
            ProResProfile::Hq => "422 (HQ)",
            ProResProfile::P4444 => "444",
            ProResProfile::P4444Xq => "444 (XQ)",
        }
    }

    /// 4:4:4 with alpha.
    pub fn is_444(self) -> bool {
        matches!(self, ProResProfile::P4444 | ProResProfile::P4444Xq)
    }

    /// Apple's target data rate for 1920×1080 at 29.97 fps, in Mb/s.
    pub fn megabits_1080p30(self) -> f64 {
        match self {
            ProResProfile::Proxy => 45.0,
            ProResProfile::Lt => 102.0,
            ProResProfile::Standard => 147.0,
            ProResProfile::Hq => 220.0,
            ProResProfile::P4444 => 330.0,
            ProResProfile::P4444Xq => 500.0,
        }
    }

    pub fn fourcc(self) -> [u8; 4] {
        *match self {
            ProResProfile::Proxy => b"apco",
            ProResProfile::Lt => b"apcs",
            ProResProfile::Standard => b"apcn",
            ProResProfile::Hq => b"apch",
            ProResProfile::P4444 => b"ap4h",
            ProResProfile::P4444Xq => b"ap4x",
        }
    }
}

impl FileFormat {
    pub const ALL: [FileFormat; 25] = [
        FileFormat::AvcMp4,
        FileFormat::HevcMp4,
        FileFormat::Av1Mp4,
        FileFormat::ProRes,
        FileFormat::Wmv,
        FileFormat::Mpeg1,
        FileFormat::Mpeg2,
        FileFormat::Bmp,
        FileFormat::Dds,
        FileFormat::Dpx,
        FileFormat::Exr,
        FileFormat::Heic,
        FileFormat::Jpeg,
        FileFormat::Png,
        FileFormat::Tiff,
        FileFormat::WmPhoto,
        FileFormat::Aac,
        FileFormat::Ac3,
        FileFormat::Aiff,
        FileFormat::Flac,
        FileFormat::Mp3,
        FileFormat::OggVorbis,
        FileFormat::Wav,
        FileFormat::Wave64,
        FileFormat::Wma,
    ];

    pub fn category(self) -> Category {
        use FileFormat::*;
        match self {
            AvcMp4 | HevcMp4 | Av1Mp4 | ProRes | Wmv | Mpeg1 | Mpeg2 => Category::Video,
            Bmp | Dds | Dpx | Exr | Heic | Jpeg | Png | Tiff | WmPhoto => Category::ImageSequence,
            Aac | Ac3 | Aiff | Flac | Mp3 | OggVorbis | Wav | Wave64 | Wma => Category::Audio,
        }
    }

    /// The format's name in the Render dialog's tree.
    pub fn name(self) -> &'static str {
        use FileFormat::*;
        match self {
            AvcMp4 => "AVC/AAC MP4",
            HevcMp4 => "HEVC/AAC MP4",
            Av1Mp4 => "AV1/AAC MP4",
            ProRes => "Apple ProRes",
            Wmv => "Windows Media Video",
            Mpeg1 => "MPEG-1",
            Mpeg2 => "MPEG-2",
            Bmp => "BMP",
            Dds => "DDS",
            Dpx => "DPX",
            Exr => "EXR",
            Heic => "HEIC",
            Jpeg => "JPEG",
            Png => "PNG",
            Tiff => "TIFF",
            WmPhoto => "WMPhoto",
            Aac => "AAC Audio",
            Ac3 => "Dolby Digital AC-3",
            Aiff => "AIFF",
            Flac => "FLAC Audio",
            Mp3 => "MP3 Audio",
            OggVorbis => "Ogg Vorbis",
            Wav => "Wave (Microsoft)",
            Wave64 => "Wave64",
            Wma => "Windows Media Audio",
        }
    }

    /// The file extension, without the point.
    pub fn extension(self) -> &'static str {
        use FileFormat::*;
        match self {
            AvcMp4 | HevcMp4 | Av1Mp4 => "mp4",
            ProRes => "mov",
            Wmv => "wmv",
            Mpeg1 | Mpeg2 => "mpg",
            Bmp => "bmp",
            Dds => "dds",
            Dpx => "dpx",
            Exr => "exr",
            Heic => "heic",
            Jpeg => "jpg",
            Png => "png",
            Tiff => "tif",
            WmPhoto => "jxr",
            Aac => "m4a",
            Ac3 => "ac3",
            Aiff => "aif",
            Flac => "flac",
            Mp3 => "mp3",
            OggVorbis => "ogg",
            Wav => "wav",
            Wave64 => "w64",
            Wma => "wma",
        }
    }

    /// The codec, as the template details name it.
    pub fn codec_name(self) -> &'static str {
        use FileFormat::*;
        match self {
            AvcMp4 => "AVC (H.264)",
            HevcMp4 => "HEVC (H.265)",
            Av1Mp4 => "AV1",
            ProRes => "Apple ProRes",
            Wmv => "WMV 9",
            Mpeg1 => "MPEG-1",
            Mpeg2 => "MPEG-2",
            Bmp | Dds | Dpx | Exr | Heic | Jpeg | Png | Tiff | WmPhoto => "Image Sequence",
            Aac => "AAC",
            Ac3 => "AC-3",
            Aiff => "PCM",
            Flac => "FLAC",
            Mp3 => "MP3",
            OggVorbis => "Vorbis",
            Wav | Wave64 => "PCM",
            Wma => "WMA",
        }
    }

    /// The audio codec as a media type ("audio/aac").
    pub fn audio_mime(self) -> &'static str {
        use FileFormat::*;
        match self {
            AvcMp4 | HevcMp4 | Av1Mp4 | Aac => "audio/aac",
            ProRes | Wav | Wave64 | Aiff => "audio/pcm",
            Wmv | Wma => "audio/x-ms-wma",
            Ac3 => "audio/ac3",
            Flac => "audio/flac",
            Mp3 | Mpeg1 | Mpeg2 => "audio/mpeg",
            OggVorbis => "audio/vorbis",
            _ => "",
        }
    }

    pub fn has_video(self) -> bool {
        self.category() != Category::Audio
    }

    pub fn has_audio(self) -> bool {
        self.category() != Category::ImageSequence
    }

    /// Lossy audio: a bit rate instead of a bit depth.
    pub fn compressed_audio(self) -> bool {
        use FileFormat::*;
        matches!(
            self,
            AvcMp4 | HevcMp4 | Av1Mp4 | Wmv | Mpeg1 | Mpeg2 | Aac | Ac3 | Mp3 | OggVorbis | Wma
        )
    }

    /// Image formats that keep the alpha channel.
    pub fn supports_alpha(self) -> bool {
        use FileFormat::*;
        matches!(self, ProRes | Bmp | Dds | Exr | Png | Tiff | WmPhoto | Heic)
    }

    /// The Windows codec it needs, if any.
    fn system_codec(self) -> Option<bv_mf::Codec> {
        use FileFormat::*;
        match self {
            AvcMp4 => Some(bv_mf::Codec::H264),
            HevcMp4 => Some(bv_mf::Codec::Hevc),
            Av1Mp4 => Some(bv_mf::Codec::Av1),
            Wmv => Some(bv_mf::Codec::Wmv),
            Aac => Some(bv_mf::Codec::Aac),
            Ac3 => Some(bv_mf::Codec::Ac3),
            Mp3 => Some(bv_mf::Codec::Mp3),
            Wma => Some(bv_mf::Codec::Wma),
            _ => None,
        }
    }

    /// Whether this computer can render it.
    pub fn availability(self) -> Availability {
        use FileFormat::*;
        match self {
            OggVorbis => Availability::Soon,
            // Its sound must be MPEG audio: the system's encoder.
            Mpeg1 => {
                if bv_mf::encoders()
                    .iter()
                    .any(|e| e.codec == bv_mf::Codec::Mp2)
                {
                    Availability::Ready
                } else {
                    Availability::NoEncoder
                }
            }
            Heic | WmPhoto => {
                if crate::render::images::system_image_encoder(self) {
                    Availability::Ready
                } else {
                    Availability::NoEncoder
                }
            }
            _ => match self.system_codec() {
                Some(c) if !bv_mf::encoders().iter().any(|e| e.codec == c) => {
                    Availability::NoEncoder
                }
                _ => Availability::Ready,
            },
        }
    }

    /// The encoders of this computer that can write its video, the
    /// processor's first.
    pub fn video_encoders(self) -> Vec<Encoder> {
        let Some(codec) = self
            .system_codec()
            .filter(|_| self.category() == Category::Video)
        else {
            return Vec::new();
        };
        let mut out: Vec<Encoder> = Vec::new();
        for e in bv_mf::encoders().iter().filter(|e| e.codec == codec) {
            if let Some(enc) = Encoder::from_vendor(e.vendor)
                && !out.contains(&enc)
            {
                out.push(enc);
            }
        }
        out.sort_by_key(|e| *e != Encoder::Software);
        out
    }

    /// Whether `encoder` writes 10-bit video of this format.
    pub fn ten_bit(self, encoder: Encoder) -> bool {
        match self {
            FileFormat::HevcMp4 | FileFormat::Av1Mp4 => match encoder {
                // Windows' own HEVC encoder only writes Main (8-bit); x265
                // and SVT-AV1 (Linux) write 10 bits too.
                Encoder::Software => !cfg!(windows),
                // Not known of Qualcomm's: 8 bits.
                Encoder::Qualcomm => false,
                _ => true,
            },
            FileFormat::ProRes | FileFormat::Dpx => true,
            _ => false,
        }
    }
}
