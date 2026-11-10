//! The Render dialog's templates: BananaV's own (one per format, size,
//! rate and encoder of this computer, named like VEGAS's) and the ones the
//! user saved.

use bv_core::FrameRate;
use bv_engine::{
    AudioParams, Availability, Category, Encoder, FileFormat, ProResProfile, RateMode, VideoParams,
};
use bv_timeline::ProjectSettings;
use serde::{Deserialize, Serialize};

use crate::i18n::tr;

/// The Render dialog's use cases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UseCase {
    YouTube,
    SocialMedia,
    WebStreaming,
    VfxMaster,
    ImageCompositing,
    DvdBluray,
    AudioMaster,
    Podcast,
}

impl UseCase {
    pub const ALL: [UseCase; 8] = [
        UseCase::YouTube,
        UseCase::SocialMedia,
        UseCase::WebStreaming,
        UseCase::VfxMaster,
        UseCase::ImageCompositing,
        UseCase::DvdBluray,
        UseCase::AudioMaster,
        UseCase::Podcast,
    ];

    pub fn title(self) -> &'static str {
        match self {
            UseCase::YouTube => "YouTube",
            UseCase::SocialMedia => tr("Social Media"),
            UseCase::WebStreaming => tr("Web & Streaming"),
            UseCase::VfxMaster => tr("Video VFX & Master"),
            UseCase::ImageCompositing => tr("Image Compositing"),
            UseCase::DvdBluray => tr("DVD & Blu-ray"),
            UseCase::AudioMaster => tr("Audio Mix & Master"),
            UseCase::Podcast => tr("Podcast & Audio Streaming"),
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            UseCase::YouTube => tr("Create a video with the settings YouTube recommends."),
            UseCase::SocialMedia => tr("Create a video sized for social media platforms."),
            UseCase::WebStreaming => tr("Create a video that streams efficiently online."),
            UseCase::VfxMaster => tr("Create high-quality media for finishing and exchange."),
            UseCase::ImageCompositing => {
                tr("Create image sequences for motion graphics and compositing.")
            }
            UseCase::DvdBluray => tr("Create media ready for DVD or Blu-ray authoring."),
            UseCase::AudioMaster => tr("Create lossless audio for high-fidelity exchange."),
            UseCase::Podcast => tr("Create a podcast with balanced quality and file size."),
        }
    }
}

/// How a template of BananaV's is named; the name is written in the
/// interface's language from its settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Naming {
    /// "Internet HD 1080p 29.97 fps".
    Internet,
    /// "AV1 1920x1080 29.97p 8 bit 4:2:0".
    Av1,
    /// "ProRes 422 (HQ) - 1920x1080-29.97p".
    ProRes,
    /// "8 Mbps HD 1080-30p Video".
    Wmv,
    /// The format's name ("PNG").
    Image,
    /// "48,000 Hz; 16 Bit; Stereo, PCM".
    Pcm,
    /// "48,000 Hz; 24 Bit; Stereo".
    Lossless,
    /// "128 Kbps, CD Quality Audio".
    Mp3,
    /// "128 Kbps, 48,000 Hz, Stereo".
    Bitrate,
    /// A template of a format BananaV does not write yet.
    Fixed(&'static str),
}

/// A render template.
#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub id: String,
    pub format: FileFormat,
    naming: Naming,
    /// The user's name (their templates).
    pub custom_name: Option<String>,
    /// The user's notes (their templates).
    pub custom_notes: Option<String>,
    /// Size and rate; 0 means the project's.
    pub video: Option<VideoParams>,
    /// Sample rate and channels; 0 means the project's.
    pub audio: Option<AudioParams>,
    pub use_cases: Vec<UseCase>,
}

/// A template the user saved (`interface.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomTemplate {
    pub id: String,
    pub format: FileFormat,
    pub name: String,
    #[serde(default)]
    pub notes: String,
    pub video: Option<VideoParams>,
    pub audio: Option<AudioParams>,
}

impl From<&CustomTemplate> for Template {
    fn from(c: &CustomTemplate) -> Template {
        Template {
            id: c.id.clone(),
            format: c.format,
            naming: Naming::Image,
            custom_name: Some(c.name.clone()),
            custom_notes: Some(c.notes.clone()),
            video: c.video,
            audio: c.audio,
            use_cases: Vec::new(),
        }
    }
}

/// The frame rate of templates that take the project's.
pub const PROJECT_RATE: FrameRate = FrameRate { num: 0, den: 1 };

/// "29.97" / "30": a frame rate as template names write it.
pub fn rate_text(r: FrameRate) -> String {
    if r.den == 1 {
        r.num.to_string()
    } else {
        let s = crate::fmt::num(r.as_f64(), 3);
        s.trim_end_matches('0')
            .trim_end_matches([',', '.'])
            .to_string()
    }
}

/// "Stereo", "Mono", "5.1", "6 channels".
pub fn channels_text(n: u16) -> String {
    match n {
        1 => tr("Mono").to_string(),
        2 => tr("Stereo").to_string(),
        6 => "5.1".to_string(),
        n => trn!(n, "{} channel", "{} channels", n),
    }
}

fn mbps(bits: u32) -> String {
    crate::fmt::num(bits as f64 / 1e6, 1)
}

/// "16:9": the reduced shape of a frame.
pub fn aspect(w: u32, h: u32) -> String {
    fn gcd(a: u32, b: u32) -> u32 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let g = gcd(w, h).max(1);
    format!("{}:{}", w / g, h / g)
}

impl Template {
    pub fn is_custom(&self) -> bool {
        self.custom_name.is_some()
    }

    /// The name in the interface's language.
    pub fn name(&self) -> String {
        if let Some(n) = &self.custom_name {
            return n.clone();
        }
        let suffix = |v: &VideoParams| match v.encoder {
            Encoder::Software => String::new(),
            e => format!(" ({})", e.short_name()),
        };
        match (self.naming, &self.video, &self.audio) {
            (Naming::Internet, Some(v), _) => {
                let (w, h) = (v.width, v.height);
                let rate = rate_text(v.frame_rate);
                let depth = if v.bit_depth > 8 {
                    format!(" {} bit", v.bit_depth)
                } else {
                    String::new()
                };
                let base = match (w, h) {
                    (3840, 2160) => trf!("Internet UHD 2160p {} fps", rate),
                    (1920, 1080) => trf!("Internet HD 1080p {} fps", rate),
                    (1280, 720) => trf!("Internet HD 720p {} fps", rate),
                    (w, h) if w < h => trf!("Internet {}x{} {} fps (vertical)", w, h, rate),
                    (w, h) if w == h => trf!("Internet {}x{} {} fps (square)", w, h, rate),
                    (w, h) => trf!("Internet {}x{} {} fps", w, h, rate),
                };
                format!("{base}{depth}{}", suffix(v))
            }
            (Naming::Av1, Some(v), _) => format!(
                "AV1 {}x{} {}p {} bit 4:2:0{}",
                v.width,
                v.height,
                rate_text(v.frame_rate),
                v.bit_depth,
                suffix(v)
            ),
            (Naming::ProRes, Some(v), _) => format!(
                "ProRes {} - {}x{}-{}p",
                v.prores.name(),
                v.width,
                v.height,
                rate_text(v.frame_rate)
            ),
            (Naming::Wmv, Some(v), _) => trf!(
                "{} Mbps {} {}-{}p Video",
                mbps(v.bitrate),
                if v.height >= 720 { "HD" } else { "SD" },
                v.height,
                v.frame_rate.as_f64().round()
            ),
            (Naming::Image, ..) => self.format.name().to_string(),
            (Naming::Pcm | Naming::Lossless, _, Some(a)) => {
                let bits = if a.float {
                    tr("32 Bit (IEEE Float)").to_string()
                } else {
                    trf!("{} Bit", a.bit_depth)
                };
                let base = format!(
                    "{} Hz; {bits}; {}",
                    crate::fmt::grouped(a.sample_rate as u64),
                    channels_text(a.channels)
                );
                match (self.naming, a.float) {
                    (Naming::Pcm, true) => format!("{base}, PCM ({})", tr("float")),
                    (Naming::Pcm, false) => format!("{base}, PCM"),
                    _ => base,
                }
            }
            (Naming::Mp3, _, Some(a)) => {
                let k = a.bitrate / 1000;
                let quality = match k {
                    0..=64 => tr("FM Radio Quality Audio"),
                    65..=96 => tr("Near CD Quality Audio"),
                    97..=128 => tr("CD Quality Audio"),
                    _ => tr("CD Transparent Audio"),
                };
                format!("{k} Kbps, {quality}")
            }
            (Naming::Bitrate, _, Some(a)) => format!(
                "{} Kbps, {} Hz, {}",
                a.bitrate / 1000,
                crate::fmt::grouped(a.sample_rate as u64),
                channels_text(a.channels)
            ),
            (Naming::Fixed(n), ..) => tr(n).to_string(),
            _ => self.format.name().to_string(),
        }
    }

    /// The notes in the interface's language.
    pub fn notes(&self) -> String {
        if let Some(n) = &self.custom_notes {
            return n.clone();
        }
        let encoder = self.video.map_or(Encoder::Software, |v| v.encoder);
        let by = match encoder {
            Encoder::Software => String::new(),
            Encoder::Nvidia => format!(" {}", tr("Encoded by the NVIDIA graphics card (NVENC).")),
            Encoder::Amd => format!(" {}", tr("Encoded by the AMD graphics card (VCE/AMF).")),
            Encoder::Intel => format!(" {}", tr("Encoded by Intel Quick Sync Video.")),
            Encoder::Qualcomm => {
                format!(" {}", tr("Encoded by the Qualcomm graphics card (Adreno)."))
            }
            Encoder::Apple => format!(
                " {}",
                tr("Encoded by the Mac's media engine (VideoToolbox).")
            ),
        };
        let text = match self.format {
            FileFormat::AvcMp4 => {
                tr("Creates an MP4 file (AVC/AAC) for internet upload and progressive download.")
            }
            FileFormat::HevcMp4 => {
                tr("Creates an MP4 file (HEVC/AAC): smaller than AVC at the same quality.")
            }
            FileFormat::Av1Mp4 => tr("Creates an MP4 file (AV1/AAC) for modern web platforms."),
            FileFormat::ProRes if self.video.is_some_and(|v| v.prores.is_444()) => tr(
                "Creates a QuickTime file (Apple ProRes 4444 with alpha, PCM audio) for finishing and exchange.",
            ),
            FileFormat::ProRes => tr(
                "Creates a QuickTime file (Apple ProRes 422, PCM audio) for editing and finishing.",
            ),
            FileFormat::Wmv => {
                tr("Creates a Windows Media file (WMV 9 video, WMA audio) for older players.")
            }
            FileFormat::Mpeg1 => tr("Creates an MPEG-1 stream for Video CD authoring."),
            FileFormat::Mpeg2 => tr("Creates an MPEG-2 stream for DVD or Blu-ray authoring."),
            FileFormat::Exr => {
                tr("One OpenEXR file per frame: linear light, half float, with alpha.")
            }
            FileFormat::Dpx => tr("One DPX file per frame: 10-bit RGB."),
            FileFormat::Png | FileFormat::Tiff => tr("One file per frame, with alpha."),
            f if f.category() == Category::ImageSequence => tr("One file per frame."),
            _ => "",
        };
        format!("{text}{by}").trim().to_string()
    }

    /// The video settings with the project's values where the template
    /// takes them (size, rate).
    pub fn video_for(&self, p: &ProjectSettings) -> Option<VideoParams> {
        let mut v = self.video?;
        let (w, h) = p.output_size();
        if v.width == 0 || v.height == 0 {
            v.width = w;
            v.height = h;
        }
        if v.frame_rate.num == 0 {
            v.frame_rate = p.frame_rate;
        }
        Some(v)
    }

    /// The audio settings with the project's values where the template
    /// takes them.
    pub fn audio_for(&self, p: &ProjectSettings) -> Option<AudioParams> {
        let mut a = self.audio?;
        if a.sample_rate == 0 {
            a.sample_rate = p.sample_rate.0;
        }
        if a.channels == 0 {
            a.channels = p.channels.channel_count() as u16;
        }
        Some(a)
    }

    pub fn availability(&self) -> Availability {
        let a = self.format.availability();
        if a != Availability::Ready {
            return a;
        }
        match self.video {
            // An encoder of a card this computer has not.
            Some(v)
                if self.format.category() == Category::Video
                    && !matches!(self.format, FileFormat::ProRes)
                    && !self.format.video_encoders().contains(&v.encoder) =>
            {
                Availability::NoEncoder
            }
            _ => Availability::Ready,
        }
    }

    /// Text for a search: name, format and codec.
    pub fn search_text(&self) -> String {
        format!(
            "{} {} {} {}",
            self.name(),
            self.format.name(),
            tr(self.format.codec_name()),
            self.format.extension()
        )
        .to_lowercase()
    }
}

fn video(w: u32, h: u32, rate: FrameRate, bitrate: u32, encoder: Encoder) -> VideoParams {
    VideoParams {
        width: w,
        height: h,
        frame_rate: rate,
        bit_depth: 8,
        alpha: false,
        encoder,
        rate_mode: RateMode::Vbr,
        bitrate,
        max_bitrate: bitrate / 2 * 3,
        quality: 90,
        keyframe_interval: 0,
        prores: ProResProfile::Standard,
        rendering_quality: None,
        color_space: None,
        field_order: bv_core::FieldOrder::Progressive,
    }
}

fn audio(rate: u32, channels: u16, bits: u8, float: bool, bitrate: u32) -> AudioParams {
    AudioParams {
        sample_rate: rate,
        channels,
        bit_depth: bits,
        float,
        bitrate,
    }
}

fn rate_id(r: FrameRate) -> String {
    format!("{}-{}", r.num, r.den)
}

fn encoder_id(e: Encoder) -> &'static str {
    match e {
        Encoder::Software => "cpu",
        Encoder::Nvidia => "nvenc",
        Encoder::Amd => "amd",
        Encoder::Intel => "qsv",
        Encoder::Qualcomm => "qcom",
        Encoder::Apple => "vt",
    }
}

/// BananaV's templates, in the order the lists show them.
pub fn builtin() -> Vec<Template> {
    use FileFormat::*;
    use UseCase::*;
    let mut out = Vec::new();
    let mut add = |t: Template| out.push(t);
    let r5994 = FrameRate::FPS_59_94;
    let r2997 = FrameRate::FPS_29_97;
    let r23 = FrameRate::FPS_23_976;
    let aac = audio(48_000, 2, 16, false, 192_000);

    // Internet MP4: (w, h, rate, AVC bit rate, use cases).
    let internet: [(u32, u32, FrameRate, u32, &[UseCase]); 15] = [
        (1920, 1080, r5994, 20_000_000, &[YouTube]),
        (1920, 1080, FrameRate::FPS_50, 20_000_000, &[]),
        (
            1920,
            1080,
            r2997,
            16_000_000,
            &[YouTube, SocialMedia, WebStreaming],
        ),
        (1920, 1080, FrameRate::FPS_25, 16_000_000, &[]),
        (1920, 1080, r23, 12_000_000, &[]),
        (3840, 2160, r5994, 40_000_000, &[YouTube]),
        (3840, 2160, FrameRate::FPS_50, 40_000_000, &[]),
        (3840, 2160, r2997, 30_000_000, &[YouTube]),
        (3840, 2160, FrameRate::FPS_25, 30_000_000, &[]),
        (3840, 2160, r23, 24_000_000, &[]),
        (1280, 720, r5994, 12_000_000, &[]),
        (1280, 720, r2997, 8_000_000, &[WebStreaming]),
        (1280, 720, FrameRate::FPS_25, 8_000_000, &[]),
        (1080, 1920, r2997, 16_000_000, &[SocialMedia]),
        (1080, 1080, r2997, 12_000_000, &[SocialMedia]),
    ];
    for (format, share, tag) in [(AvcMp4, 1.0, "avc"), (HevcMp4, 0.6, "hevc")] {
        // Every encoder, the processor's first; VEGAS-style names tag the
        // graphics cards' ones.
        let mut encoders = format.video_encoders();
        if encoders.is_empty() {
            encoders.push(Encoder::Software);
        }
        for e in encoders {
            for (w, h, rate, bits, cases) in internet {
                let bitrate = (bits as f64 * share) as u32;
                add(Template {
                    id: format!("{tag}:{w}x{h}:{}:{}", rate_id(rate), encoder_id(e)),
                    format,
                    naming: Naming::Internet,
                    custom_name: None,
                    custom_notes: None,
                    video: Some(video(w, h, rate, bitrate, e)),
                    audio: Some(aac),
                    use_cases: cases.to_vec(),
                });
            }
            // 10-bit HEVC from the graphics cards.
            if format == HevcMp4 && format.ten_bit(e) {
                for (w, h, rate, bits) in [
                    (3840, 2160, r5994, 30_000_000),
                    (3840, 2160, r2997, 20_000_000),
                    (1920, 1080, r5994, 14_000_000),
                ] {
                    let mut v = video(w, h, rate, bits, e);
                    v.bit_depth = 10;
                    add(Template {
                        id: format!("hevc10:{w}x{h}:{}:{}", rate_id(rate), encoder_id(e)),
                        format,
                        naming: Naming::Internet,
                        custom_name: None,
                        custom_notes: None,
                        video: Some(v),
                        audio: Some(aac),
                        use_cases: Vec::new(),
                    });
                }
            }
        }
    }
    // AV1 (graphics cards).
    let mut av1 = Av1Mp4.video_encoders();
    if av1.is_empty() {
        av1.push(Encoder::Nvidia);
    }
    for e in av1 {
        for (w, h, rate, depth, bits, cases) in [
            (
                1920,
                1080,
                r2997,
                8,
                8_000_000,
                &[YouTube, WebStreaming][..],
            ),
            (1920, 1080, r5994, 8, 12_000_000, &[][..]),
            (3840, 2160, r2997, 10, 18_000_000, &[YouTube][..]),
            (3840, 2160, r5994, 10, 24_000_000, &[][..]),
        ] {
            let mut v = video(w, h, rate, bits, e);
            v.bit_depth = depth;
            add(Template {
                id: format!("av1:{w}x{h}:{}:{depth}:{}", rate_id(rate), encoder_id(e)),
                format: Av1Mp4,
                naming: Naming::Av1,
                custom_name: None,
                custom_notes: None,
                video: Some(v),
                audio: Some(aac),
                use_cases: cases.to_vec(),
            });
        }
    }
    // Apple ProRes.
    for profile in ProResProfile::ALL {
        for (w, h) in [(1920, 1080), (3840, 2160)] {
            for rate in [r5994, r2997, FrameRate::FPS_25, r23] {
                let master = w == 1920
                    && (matches!(profile, ProResProfile::Hq)
                        || (profile.is_444() && rate == r2997));
                let mut v = video(w, h, rate, 0, Encoder::Software);
                v.prores = profile;
                v.bit_depth = 10;
                v.alpha = profile.is_444();
                add(Template {
                    id: format!("prores:{:?}:{w}x{h}:{}", profile, rate_id(rate)),
                    format: ProRes,
                    naming: Naming::ProRes,
                    custom_name: None,
                    custom_notes: None,
                    video: Some(v),
                    audio: Some(audio(48_000, 2, 24, false, 0)),
                    use_cases: if master { vec![VfxMaster] } else { Vec::new() },
                });
            }
        }
    }
    // Windows Media Video.
    for (w, h, rate, bits) in [
        (1920, 1080, r2997, 8_000_000),
        (1280, 720, r2997, 6_000_000),
        (640, 480, r2997, 2_000_000),
    ] {
        add(Template {
            id: format!("wmv:{w}x{h}:{}", rate_id(rate)),
            format: Wmv,
            naming: Naming::Wmv,
            custom_name: None,
            custom_notes: None,
            video: Some(video(w, h, rate, bits, Encoder::Software)),
            audio: Some(audio(48_000, 2, 16, false, 192_000)),
            use_cases: Vec::new(),
        });
    }
    // MPEG-1 for Video CDs: 1,150 kbit/s, MPEG audio at 224.
    for (name, h, rate) in [
        ("VCD NTSC", 240, r2997),
        ("VCD PAL", 288, FrameRate::FPS_25),
    ] {
        add(Template {
            id: format!("mpeg1:352x{h}:{}", rate_id(rate)),
            format: Mpeg1,
            naming: Naming::Fixed(name),
            custom_name: None,
            custom_notes: None,
            video: Some(VideoParams {
                max_bitrate: 1_150_000,
                ..video(352, h, rate, 1_150_000, Encoder::Software)
            }),
            audio: Some(audio(44_100, 2, 16, false, 224_000)),
            use_cases: Vec::new(),
        });
    }
    // MPEG-2 for discs.
    for (name, w, h, rate) in [
        ("DVD NTSC video stream", 720, 480, r2997),
        ("DVD PAL video stream", 720, 576, FrameRate::FPS_25),
        ("Blu-ray 1920x1080-24p video stream", 1920, 1080, r23),
    ] {
        add(Template {
            id: format!("mpeg2:{w}x{h}:{}", rate_id(rate)),
            format: Mpeg2,
            naming: Naming::Fixed(name),
            custom_name: None,
            custom_notes: None,
            video: Some(video(w, h, rate, 8_000_000, Encoder::Software)),
            audio: Some(audio(48_000, 2, 16, false, 192_000)),
            use_cases: vec![DvdBluray],
        });
    }
    // Image sequences: the project's size and rate.
    for f in [Bmp, Dds, Dpx, Exr, Heic, Jpeg, Png, Tiff, WmPhoto] {
        let mut v = video(0, 0, PROJECT_RATE, 0, Encoder::Software);
        v.alpha = f.supports_alpha();
        v.bit_depth = match f {
            Dpx => 10,
            Exr => 16,
            _ => 8,
        };
        let cases = match f {
            Exr | Dpx => vec![VfxMaster, ImageCompositing],
            Png | Tiff => vec![ImageCompositing],
            _ => Vec::new(),
        };
        add(Template {
            id: format!("image:{}", f.extension()),
            format: f,
            naming: Naming::Image,
            custom_name: None,
            custom_notes: None,
            video: Some(v),
            audio: None,
            use_cases: cases,
        });
    }
    // Audio.
    let pcm = |format: FileFormat,
               naming: Naming,
               list: &[(u32, u16, u8, bool, &[UseCase])],
               add: &mut dyn FnMut(Template)| {
        for &(rate, ch, bits, float, cases) in list {
            add(Template {
                id: format!(
                    "{}:{rate}:{ch}:{bits}{}",
                    format.extension(),
                    if float { "f" } else { "" }
                ),
                format,
                naming,
                custom_name: None,
                custom_notes: None,
                video: None,
                audio: Some(audio(rate, ch, bits, float, 0)),
                use_cases: cases.to_vec(),
            });
        }
    };
    let none: &[UseCase] = &[];
    let master: &[UseCase] = &[AudioMaster];
    pcm(
        Flac,
        Naming::Lossless,
        &[
            (48_000, 2, 16, false, none),
            (48_000, 2, 24, false, master),
            (44_100, 2, 16, false, &[Podcast]),
            (44_100, 2, 24, false, none),
            (96_000, 2, 24, false, none),
            (22_050, 2, 16, false, none),
            (44_100, 1, 16, false, none),
        ],
        &mut add,
    );
    pcm(
        Wav,
        Naming::Pcm,
        &[
            (48_000, 2, 16, false, none),
            (48_000, 2, 24, false, master),
            (48_000, 2, 32, true, master),
            (44_100, 2, 16, false, none),
            (44_100, 2, 24, false, none),
            (44_100, 2, 32, true, none),
            (96_000, 2, 24, false, master),
            (96_000, 2, 32, true, none),
            (48_000, 6, 16, false, none),
            (48_000, 1, 16, false, none),
            (44_100, 1, 16, false, &[Podcast]),
            (22_050, 2, 16, false, none),
            (22_050, 1, 16, false, none),
        ],
        &mut add,
    );
    pcm(
        Wave64,
        Naming::Pcm,
        &[
            (48_000, 2, 24, false, master),
            (48_000, 2, 32, true, none),
            (96_000, 2, 24, false, none),
        ],
        &mut add,
    );
    pcm(
        Aiff,
        Naming::Lossless,
        &[(48_000, 2, 24, false, master), (44_100, 2, 16, false, none)],
        &mut add,
    );
    for (format, naming, list) in [
        (
            Mp3,
            Naming::Mp3,
            vec![
                (44_100, 64_000, none),
                (44_100, 96_000, none),
                (44_100, 128_000, &[Podcast][..]),
                (44_100, 192_000, &[Podcast][..]),
                (44_100, 256_000, none),
                (44_100, 320_000, none),
            ],
        ),
        (
            Aac,
            Naming::Bitrate,
            vec![
                (48_000, 96_000, none),
                (48_000, 128_000, &[Podcast][..]),
                (48_000, 192_000, none),
                (44_100, 128_000, none),
                (44_100, 192_000, none),
            ],
        ),
        (
            Wma,
            Naming::Bitrate,
            vec![
                (44_100, 64_000, none),
                (44_100, 128_000, none),
                (44_100, 192_000, none),
                (48_000, 192_000, none),
            ],
        ),
        (
            Ac3,
            Naming::Bitrate,
            vec![(48_000, 192_000, &[DvdBluray][..]), (48_000, 448_000, none)],
        ),
    ] {
        for (rate, bits, cases) in list {
            let channels = if format == Ac3 && bits > 300_000 {
                6
            } else {
                2
            };
            add(Template {
                id: format!("{}:{rate}:{}", format.extension(), bits / 1000),
                format,
                naming,
                custom_name: None,
                custom_notes: None,
                video: None,
                audio: Some(audio(rate, channels, 16, false, bits)),
                use_cases: cases.to_vec(),
            });
        }
    }
    for (name, rate) in [
        ("VBR, 44,100 Hz, Stereo, Quality 0.40", 44_100),
        ("VBR, 48,000 Hz, Stereo, Quality 0.60", 48_000),
    ] {
        add(Template {
            id: format!("ogg:{rate}"),
            format: OggVorbis,
            naming: Naming::Fixed(name),
            custom_name: None,
            custom_notes: None,
            video: None,
            audio: Some(audio(rate, 2, 16, false, 160_000)),
            use_cases: Vec::new(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let all = builtin();
        let mut seen = std::collections::HashSet::new();
        for t in &all {
            assert!(seen.insert(t.id.clone()), "{} twice", t.id);
        }
        assert!(all.len() > 100);
    }

    #[test]
    fn names_follow_vegas() {
        let all = builtin();
        let find = |id: &str| all.iter().find(|t| t.id == id).unwrap().name();
        assert_eq!(
            find("avc:1920x1080:60000-1001:cpu"),
            "Internet HD 1080p 59.94 fps"
        );
        assert_eq!(
            find("prores:Hq:1920x1080:30000-1001"),
            "ProRes 422 (HQ) - 1920x1080-29.97p"
        );
        assert_eq!(find("wav:48000:2:16"), "48,000 Hz; 16 Bit; Stereo, PCM");
        assert_eq!(find("flac:48000:2:24"), "48,000 Hz; 24 Bit; Stereo");
        assert_eq!(find("mp3:44100:128"), "128 Kbps, CD Quality Audio");
        assert_eq!(find("image:png"), "PNG");
    }
}
