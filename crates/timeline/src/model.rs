use bv_core::{ChannelLayout, FrameRate, SampleRate, Tick, TimeRange};
use serde::{Deserialize, Serialize};

use crate::fx::{Effect, FadeCurve, PluginFx, Transform, TransformKey, Transition};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MediaId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TrackId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClipId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GroupId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackKind {
    Video,
    Audio,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoStreamInfo {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    /// The shape of its pixels (square for most video; HDV's 1440 × 1080
    /// frames have wide ones).
    #[serde(default, skip_serializing_if = "PixelAspect::is_square")]
    pub pixel_aspect: PixelAspect,
    /// Its fields' order: the file's, or what its Media Properties say.
    #[serde(default, skip_serializing_if = "is_default")]
    pub field_order: bv_core::FieldOrder,
    /// A 360° video or photo: the whole sphere around the camera, laid out
    /// equirectangularly, as its file's spherical metadata says.
    #[serde(default, skip_serializing_if = "is_default")]
    pub spherical: bool,
    /// Its Media Properties' Color Range: Full, its Y′CbCr taken as full
    /// range (no level conversion); else Limited, 16–235 brought out to
    /// 0–255. Full-range projects only (video levels ones keep Y′CbCr as
    /// it is anyway).
    #[serde(default, skip_serializing_if = "is_default")]
    pub full_range: bool,
    /// How its pictures are read, as its Media Properties say: their alpha
    /// channel, Color range's Limited and their rotation (the width and
    /// height above are the turned picture's).
    #[serde(default, skip_serializing_if = "is_default")]
    pub reading: VideoReading,
}

impl VideoStreamInfo {
    /// The size of the file's own frames, before its Rotation.
    pub fn coded_size(&self) -> (u32, u32) {
        if self.reading.rotation.quarters() % 2 == 1 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    /// Turned to `rotation` (Media Properties' Rotation) from its rotation
    /// now: a quarter turn more or less swaps its width and height, and its
    /// pixels' shape.
    pub fn turned_to(self, rotation: MediaRotation) -> VideoStreamInfo {
        let quarter = (rotation.quarters() + 4 - self.reading.rotation.quarters()) % 2 == 1;
        let mut v = self;
        v.reading.rotation = rotation;
        if quarter {
            (v.width, v.height) = (v.height, v.width);
            v.pixel_aspect = PixelAspect::new(v.pixel_aspect.den, v.pixel_aspect.num);
        }
        v
    }
}

/// How a video stream's pictures are read, as its Media Properties say
/// (VEGAS's Alpha channel, its Background color, Color range and
/// Rotation); by default as the file says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VideoReading {
    #[serde(default, skip_serializing_if = "is_default")]
    pub alpha: AlphaChannel,
    /// Premultiplied (dirty)'s Background color: what the picture was
    /// matted against, in its own values (8 bits).
    #[serde(default, skip_serializing_if = "is_default")]
    pub background: [u8; 3],
    /// Color range's Limited: its Y′CbCr taken as 16–235 even where the
    /// file says full range (Full is [`VideoStreamInfo::full_range`];
    /// neither, Undefined: as the file says).
    #[serde(default, skip_serializing_if = "is_default")]
    pub limited: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub rotation: MediaRotation,
}

/// How a picture's alpha channel is read (Media Properties' Alpha
/// channel).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlphaChannel {
    /// As its reader takes it: straight for most files, premultiplied for
    /// OpenEXR's.
    #[default]
    Undefined,
    /// No alpha: every pixel opaque, its colors as they are.
    None,
    /// Straight (unmatted): the colors are the picture's own.
    Straight,
    /// The colors already multiplied by the alpha.
    Premultiplied,
    /// Premultiplied against a background color (the Background color),
    /// not black: that color taken out.
    PremultipliedDirty,
}

impl AlphaChannel {
    /// VEGAS's list's order.
    pub const ALL: [AlphaChannel; 5] = [
        AlphaChannel::Undefined,
        AlphaChannel::None,
        AlphaChannel::Straight,
        AlphaChannel::Premultiplied,
        AlphaChannel::PremultipliedDirty,
    ];
}

/// How far a media's pictures are turned (Media Properties' Rotation),
/// clockwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaRotation {
    /// As the file has them.
    #[default]
    Original,
    Clockwise,
    Inverted,
    Counterclockwise,
}

impl MediaRotation {
    /// VEGAS's list's order.
    pub const ALL: [MediaRotation; 4] = [
        MediaRotation::Original,
        MediaRotation::Clockwise,
        MediaRotation::Inverted,
        MediaRotation::Counterclockwise,
    ];

    /// Quarter turns clockwise (0–3).
    pub fn quarters(self) -> u8 {
        match self {
            MediaRotation::Original => 0,
            MediaRotation::Clockwise => 1,
            MediaRotation::Inverted => 2,
            MediaRotation::Counterclockwise => 3,
        }
    }
}

/// The shape of a pixel: its width over its height, as a fraction (1 : 1 is
/// square; NTSC DV is 10 : 11, HDV 1080 is 4 : 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelAspect {
    pub num: u32,
    pub den: u32,
}

impl Default for PixelAspect {
    fn default() -> PixelAspect {
        PixelAspect::SQUARE
    }
}

impl PixelAspect {
    pub const SQUARE: PixelAspect = PixelAspect { num: 1, den: 1 };

    /// A fraction; nonsense (a zero) is square.
    pub fn new(num: u32, den: u32) -> PixelAspect {
        if num == 0 || den == 0 {
            PixelAspect::SQUARE
        } else {
            PixelAspect { num, den }
        }
    }

    pub fn ratio(self) -> f32 {
        if self.den == 0 {
            1.0
        } else {
            self.num as f32 / self.den as f32
        }
    }

    pub fn is_square(&self) -> bool {
        self.num == self.den
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioStreamInfo {
    pub sample_rate: SampleRate,
    pub channels: u16,
}

/// A run of numbered pictures (`shot_0001.png`, `shot_0002.png`…) taken as
/// one video.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageSequence {
    /// How many pictures, from the first one.
    pub count: u32,
}

impl ImageSequence {
    /// The number in a picture's name (the last run of digits before the
    /// extension): where it starts and ends in the name, and its value.
    pub fn number_of(path: &str) -> Option<(usize, usize, u64)> {
        let name_start = path.rfind(['/', '\\']).map_or(0, |i| i + 1);
        let stem_end = path[name_start..]
            .rfind('.')
            .map_or(path.len(), |i| name_start + i);
        let digits_end = stem_end;
        let digits_start = path[name_start..digits_end]
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_ascii_digit())
            .last()
            .map(|(i, _)| name_start + i)?;
        let value = path[digits_start..digits_end].parse().ok()?;
        Some((digits_start, digits_end, value))
    }

    /// The path of picture `index` (0 = the first) of the run starting at
    /// `first`, its number padded like the first's.
    pub fn path(first: &str, index: u32) -> Option<String> {
        let (start, end, value) = ImageSequence::number_of(first)?;
        let width = end - start;
        Some(format!(
            "{}{:0width$}{}",
            &first[..start],
            value + index as u64,
            &first[end..]
        ))
    }
}

/// De onde vêm os quadros e amostras de uma mídia.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MediaSource {
    File {
        path: String,
    },
    /// Mídia gerada pelo próprio BananaV (o "Media Generator" do VEGAS).
    Generated(Generator),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Generator {
    /// A solid color: video's values (BT.709-encoded, as VEGAS takes a
    /// picked color's), 0–1.
    SolidColor { rgba: [f32; 4] },
    /// Barras de cor SMPTE (75%).
    ColorBars,
    /// Xadrez de `size` pixels.
    Checkerboard { size: u32 },
    /// A linear gradient between two colors (video's values), at an angle in
    /// degrees.
    Gradient {
        from: [f32; 4],
        to: [f32; 4],
        angle_deg: f32,
    },
    /// Ruído animado (chuvisco), monocromático ou colorido.
    Noise { amount: f32, color: bool },
    /// Senoide em todos os canais.
    TestTone { frequency_hz: f32, level_db: f32 },
    /// Pictures an OFX plug-in draws (a generator), with its parameters'
    /// values.
    Plugin(Box<PluginFx>),
    /// An adjustment event's: no picture of its own — its effects work on
    /// what the tracks below it show (Insert › Adjustment Event).
    Adjustment,
}

impl Generator {
    /// Duração disponível de uma mídia gerada (efetivamente ilimitada).
    pub const DURATION: Tick = Tick::from_seconds(24 * 3600);
    /// Duração do clipe quando a mídia gerada é colocada na timeline.
    pub const DEFAULT_CLIP: Tick = Tick::from_seconds(5);

    pub fn display_name(&self) -> String {
        match self {
            Generator::SolidColor { .. } => "Solid Color".into(),
            Generator::ColorBars => "Color Bars".into(),
            Generator::Checkerboard { .. } => "Checkerboard".into(),
            Generator::Gradient { .. } => "Gradient".into(),
            Generator::Noise { .. } => "Noise".into(),
            Generator::TestTone { frequency_hz, .. } => format!("Tone {frequency_hz:.0} Hz"),
            Generator::Plugin(fx) if fx.name.is_empty() => fx.id.clone(),
            Generator::Plugin(fx) => fx.name.clone(),
            Generator::Adjustment => "Adjustment Event".into(),
        }
    }
}

/// Uma mídia do projeto: arquivo importado ou mídia gerada.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
    pub id: MediaId,
    pub source: MediaSource,
    pub name: String,
    pub duration: Tick,
    pub video: Option<VideoStreamInfo>,
    /// The first audio stream.
    pub audio: Option<AudioStreamInfo>,
    /// The file's other audio streams (a recording with the microphone and
    /// the game on separate streams, say): each goes to its own audio track.
    #[serde(default)]
    pub extra_audio: Vec<AudioStreamInfo>,
    /// Imagem parada: pode ficar na timeline por qualquer tempo.
    #[serde(default)]
    pub still: bool,
    /// Numbered pictures shown one a frame (VEGAS's "Open sequence"); the
    /// source's path is the first one's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<ImageSequence>,
    /// Efeitos de mídia: valem para todos os clipes que usam esta mídia.
    #[serde(default)]
    pub effects: Vec<Effect>,
    /// Markers and regions of the media file (the Trimmer's), in media
    /// time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<MediaMarker>,
    /// Under ACES, the color space its values are in (Media Properties'
    /// Color space; `None`: VEGAS's Default, see `color::MEDIA_SPACE`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_space: Option<crate::ColorSpace>,
    /// Its tape name and timecode (Media Properties' Media tab).
    #[serde(default, skip_serializing_if = "MediaReel::is_empty")]
    pub reel: MediaReel,
    /// One layer of a layered Photoshop document (its index among the
    /// picture layers, bottom first): the layer alone on the canvas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<u16>,
}

/// What a media's Properties say of where it came from: the name of the
/// tape it was captured from and the timecode of its first frame, when
/// typed in (Use custom timecode) instead of the file's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaReel {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tape_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timecode: Option<Tick>,
}

impl MediaReel {
    pub fn is_empty(&self) -> bool {
        *self == MediaReel::default()
    }
}

/// A marker or region of a media file: made in the Trimmer or read from
/// the file, shown in the events that use it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaMarker {
    /// Where, in the media.
    pub time: Tick,
    /// A region's length (zero: a marker).
    #[serde(default)]
    pub length: Tick,
    #[serde(default)]
    pub name: String,
}

impl MediaMarker {
    pub fn is_region(&self) -> bool {
        self.length > Tick::ZERO
    }

    pub fn end(&self) -> Tick {
        self.time + self.length
    }
}

/// A media file's markers kept beside it, when its type has no place for
/// them (`<file>.bvmarkers`).
#[derive(Serialize, Deserialize)]
struct MarkerFile {
    ticks_per_second: i64,
    markers: Vec<MediaMarker>,
}

/// `markers` as the text of a markers file beside the media.
pub fn markers_to_json(markers: &[MediaMarker]) -> String {
    serde_json::to_string_pretty(&MarkerFile {
        ticks_per_second: bv_core::TICKS_PER_SECOND,
        markers: markers.to_vec(),
    })
    .unwrap_or_default()
}

/// The markers of a markers file's text.
pub fn markers_from_json(text: &str) -> Option<Vec<MediaMarker>> {
    let f: MarkerFile = serde_json::from_str(text).ok()?;
    if f.ticks_per_second == bv_core::TICKS_PER_SECOND {
        return Some(f.markers);
    }
    let scale = bv_core::TICKS_PER_SECOND as f64 / f.ticks_per_second.max(1) as f64;
    let at = |t: Tick| Tick((t.0 as f64 * scale).round() as i64);
    Some(
        f.markers
            .into_iter()
            .map(|m| MediaMarker {
                time: at(m.time),
                length: at(m.length),
                name: m.name,
            })
            .collect(),
    )
}

impl MediaInfo {
    /// Whether it is an adjustment event's (no picture: the tracks below
    /// give it one).
    pub fn is_adjustment(&self) -> bool {
        matches!(self.source, MediaSource::Generated(Generator::Adjustment))
    }

    /// Whether it is a file of frames that end (not a still or generated
    /// media, which last as long as their events).
    pub fn has_frames(&self) -> bool {
        !self.still && matches!(self.source, MediaSource::File { .. })
    }

    pub fn has(&self, kind: TrackKind) -> bool {
        match kind {
            TrackKind::Video => self.video.is_some(),
            TrackKind::Audio => self.audio.is_some(),
        }
    }

    /// How many audio streams the media has.
    pub fn audio_stream_count(&self) -> usize {
        usize::from(self.audio.is_some()) + self.extra_audio.len()
    }

    /// Audio stream `index` (0 = the first).
    pub fn audio_stream(&self, index: u16) -> Option<AudioStreamInfo> {
        match index {
            0 => self.audio,
            i => self.extra_audio.get(i as usize - 1).copied(),
        }
    }

    /// How long an event of this media is when it is added to the
    /// timeline, with the usual lengths (see [`NewEvents::length`]).
    pub fn insert_duration(&self) -> Tick {
        NewEvents::default().length(self)
    }

    /// Mídia gerada com as dimensões do projeto.
    pub fn generated(generator: Generator, settings: &ProjectSettings) -> MediaInfo {
        let is_audio = matches!(generator, Generator::TestTone { .. });
        MediaInfo {
            id: MediaId(0),
            name: generator.display_name(),
            duration: Generator::DURATION,
            video: (!is_audio).then_some(VideoStreamInfo {
                width: settings.width,
                height: settings.height,
                frame_rate: settings.frame_rate,
                pixel_aspect: Default::default(),
                field_order: Default::default(),
                spherical: false,
                full_range: false,
                reading: Default::default(),
            }),
            audio: is_audio.then_some(AudioStreamInfo {
                sample_rate: settings.sample_rate,
                channels: settings.channels.channel_count() as u16,
            }),
            extra_audio: Vec::new(),
            sequence: None,
            still: false,
            effects: Vec::new(),
            markers: Vec::new(),
            source: MediaSource::Generated(generator),
            color_space: None,
            reel: Default::default(),
            layer: None,
        }
    }
}

/// How new events start: the editor's settings (Options › Preferences ›
/// Editing), given to [`Project::insert_media_as`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NewEvents {
    /// "Enable looping on events by default": the events' Loop switch.
    pub looped: bool,
    /// "Preserve pitch when stretching audio events".
    pub preserve_pitch: bool,
    /// "New still image length (seconds)".
    pub still_length: Tick,
    /// The curves of new audio and video events' fades (Options ›
    /// Preferences › External Control & Automation: "New audio event gain",
    /// "New video event opacity").
    pub audio_fades: FadeCurve,
    pub video_fades: FadeCurve,
}

impl Default for NewEvents {
    /// Loop on, the pitch following the speed, still images 5 s long,
    /// smooth fades.
    fn default() -> NewEvents {
        NewEvents {
            looped: true,
            preserve_pitch: false,
            still_length: Generator::DEFAULT_CLIP,
            audio_fades: FadeCurve::Smooth,
            video_fades: FadeCurve::Smooth,
        }
    }
}

impl NewEvents {
    /// How long an event of `info` is when it is added to the timeline: the
    /// whole file, `still_length` for a still image, 5 seconds of generated
    /// media.
    pub fn length(&self, info: &MediaInfo) -> Tick {
        match info.source {
            MediaSource::File { .. } if info.still => self.still_length.min(info.duration),
            MediaSource::File { .. } => info.duration,
            _ => Generator::DEFAULT_CLIP.min(info.duration),
        }
    }
}

/// Um trecho de mídia posicionado numa trilha (o "evento" do VEGAS).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: ClipId,
    pub media: MediaId,
    /// Posição na timeline.
    pub start: Tick,
    pub duration: Tick,
    /// Instante da mídia de origem que aparece em `start`.
    pub source_in: Tick,
    /// Clipes do mesmo grupo se movem, aparam e dividem juntos.
    pub group: Option<GroupId>,
    /// Opacidade (vídeo) ou ganho linear (áudio).
    pub level: f32,
    /// Fade de entrada a partir do início do clipe (além do crossfade
    /// automático quando ele se sobrepõe ao clipe anterior).
    #[serde(default)]
    pub fade_in: Tick,
    #[serde(default)]
    pub fade_out: Tick,
    #[serde(default)]
    pub fade_in_curve: FadeCurve,
    #[serde(default)]
    pub fade_out_curve: FadeCurve,
    /// Transição usada na entrada (fade-in e crossfade com o clipe anterior).
    #[serde(default)]
    pub transition: Transition,
    /// The entrance transition drawn by an OFX plug-in instead of
    /// `transition` (`None`: BananaV's own).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_fx: Option<Box<PluginFx>>,
    /// The transition's plug-in switched off (its button's check box in the
    /// FX window, as VEGAS's): a plain crossfade meanwhile, its settings
    /// kept.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub transition_bypass: bool,
    #[serde(default)]
    pub effects: Vec<Effect>,
    /// How many of `effects` come before its Pan/Crop (the Video Event FX
    /// window's chain, where its button is dragged): they work on the
    /// media's own picture, the others on the framed one. 0, VEGAS's
    /// default: the Pan/Crop first.
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub pan_crop_at: usize,
    #[serde(default)]
    pub transform: Transform,
    /// Quadros-chave do enquadramento (vazio = `transform` fixo).
    #[serde(default)]
    pub transform_keys: Vec<TransformKey>,
    /// Its Pan/Crop masks (the Pan/Crop window's Mask row): shapes on its
    /// media that cut what shows of it, or keep its effects inside them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<crate::mask::PanCropMask>,
    #[serde(default)]
    pub muted: bool,
    /// Clipe travado não se move nem é aparado.
    #[serde(default)]
    pub locked: bool,
    /// Velocidade de reprodução: 1 = normal, 2 = o dobro, 0,5 = câmera lenta.
    #[serde(default = "one_f64")]
    pub rate: f64,
    /// Audio: stretched (`rate` ≠ 1), the event keeps its pitch instead of
    /// rising when sped up and falling when slowed down (Options ›
    /// Preferences › Editing: "Preserve pitch when stretching audio events"
    /// gives new events theirs).
    #[serde(default)]
    pub preserve_pitch: bool,
    /// Its sync link (VEGAS's one-way group): the event it follows, and
    /// how far after that event's start it starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_link: Option<SyncLink>,
    /// Its motion tracks (Motion Tracking's regions followed through it).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub motion_tracks: Vec<crate::MotionTrack>,
    /// Video: the share of its media's frames it shows (VEGAS's Undersample
    /// rate, 0.5: every other frame, each shown twice), for a choppier
    /// look; 1 shows them all.
    #[serde(default = "one", skip_serializing_if = "is_one_f32")]
    pub undersample: f32,
    /// Audio: its pitch change, in semitones (VEGAS's = and − keys and the
    /// Event Properties' Pitch change), its length kept.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub pitch: f32,
    /// Quadro congelado: o instante da mídia mostrado durante o clipe todo.
    #[serde(default)]
    pub freeze: Option<Tick>,
    /// Audio: invert the polarity of the signal.
    #[serde(default)]
    pub invert_phase: bool,
    /// Audio: how the source channels feed the output pair.
    #[serde(default)]
    pub channels: ChannelMap,
    /// Audio: linear gain set by "Normalize" (`None` = not normalized).
    #[serde(default)]
    pub normalize: Option<f32>,
    /// Edit › Switches › Loop: past the end of the media (or before its
    /// start), the media repeats.
    #[serde(default = "yes")]
    pub looped: bool,
    /// Edit › Switches › Hold Last Frame: without Loop, the last frame stays
    /// on screen past the end of the media (otherwise nothing shows).
    #[serde(default)]
    pub hold_last: bool,
    /// Audio: which of the media's audio streams the event plays (0 = the
    /// first).
    #[serde(default)]
    pub audio_stream: u16,
    /// Edit › Switches › Trim Event to Include All Frames (video): the
    /// event is as long as its media's frames from where it starts in it,
    /// whatever its playback rate, and its edges cannot be trimmed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub all_frames: bool,
    /// Edit › Switches › Auto Normalize (audio): the normalize gain is
    /// worked out again whenever the event's part of its media changes (a
    /// trim, a split).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto_normalize: bool,
    /// Edit › Switches › Reduce Interlace Flicker (video): the picture
    /// filtered down its columns, each line half itself and a quarter of
    /// each line next to it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reduce_flicker: bool,
    /// Edit › Switches' Resample mode (video): `None`, the project's
    /// (VEGAS's Use Project Resample Mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resample: Option<ResampleMode>,
    /// Where the event snaps from when it is moved: its snap offset flag,
    /// this far from its start (at most its length).
    #[serde(default, skip_serializing_if = "Tick::is_zero")]
    pub snap_offset: Tick,
    /// Its own envelopes (video events: velocity, transition progress),
    /// their times from its start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub envelopes: Vec<crate::envelope::Envelope>,
    /// Reverse (the event's shortcut menu): it plays its media backward,
    /// mirrored around this media time — what it would show at a moment
    /// is this time minus it (`source_in` and the rest count as if it
    /// played forward, so trims and splits keep what it shows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverse: Option<Tick>,
    /// Its takes in order, the active one among them, when it has more
    /// than one (VEGAS's takes: other media it can play instead;
    /// [`crate::takes`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub takes: Vec<crate::takes::Take>,
    /// Its sound's loudness as Auto Ducking read it, when its track is a
    /// controller (and its own reductions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duck_levels: Option<crate::ducking::DuckLevels>,
    /// Which of `takes` plays: the event's `media` and `source_in`.
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub active_take: usize,
}

fn is_zero_usize(v: &usize) -> bool {
    *v == 0
}

/// Channel routing of an audio event (Edit › Channels).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChannelMap {
    #[default]
    Both,
    LeftOnly,
    RightOnly,
    /// Left and right mixed to mono on both sides.
    Combine,
    Swap,
}

fn one_f64() -> f64 {
    1.0
}

impl Clip {
    pub fn new(id: ClipId, media: MediaId, start: Tick, duration: Tick) -> Clip {
        Clip {
            id,
            media,
            start,
            duration,
            source_in: Tick::ZERO,
            group: None,
            level: 1.0,
            fade_in: Tick::ZERO,
            fade_out: Tick::ZERO,
            fade_in_curve: FadeCurve::default(),
            fade_out_curve: FadeCurve::default(),
            transition: Transition::default(),
            transition_fx: None,
            transition_bypass: false,
            effects: Vec::new(),
            pan_crop_at: 0,
            transform: Transform::default(),
            transform_keys: Vec::new(),
            mask: None,
            muted: false,
            locked: false,
            rate: 1.0,
            preserve_pitch: false,
            sync_link: None,
            motion_tracks: Vec::new(),
            pitch: 0.0,
            undersample: 1.0,
            freeze: None,
            invert_phase: false,
            channels: ChannelMap::Both,
            normalize: None,
            looped: true,
            hold_last: false,
            audio_stream: 0,
            all_frames: false,
            auto_normalize: false,
            reduce_flicker: false,
            resample: None,
            snap_offset: Tick::ZERO,
            envelopes: Vec::new(),
            reverse: None,
            takes: Vec::new(),
            duck_levels: None,
            active_take: 0,
        }
    }

    /// Its `kind` envelope, if it has one.
    pub fn envelope(
        &self,
        kind: crate::envelope::EnvelopeKind,
    ) -> Option<&crate::envelope::Envelope> {
        self.envelopes.iter().find(|e| e.kind == kind)
    }

    /// Its velocity envelope (video events), if it has one.
    pub fn velocity(&self) -> Option<&crate::envelope::Envelope> {
        self.envelope(crate::envelope::EnvelopeKind::Velocity)
    }

    /// How far into its media the event has gone `d` after its start: its
    /// playback rate times its velocity envelope's speeds summed — never
    /// back past the media's first frame, as VEGAS's reversed events stop
    /// there —; before its start, the first speed.
    pub fn source_offset(&self, d: Tick) -> Tick {
        let Some(env) = self.velocity() else {
            return self.to_source(d);
        };
        let rate = self.rate.max(1e-6);
        let floor = -(self.source_in.0 as f64) / rate;
        Tick((env.travel(d, floor) * rate).round() as i64)
    }

    /// Its entrance transition's progress `d` after its start, over a
    /// transition `length` long: its transition progress envelope's, or a
    /// straight line.
    pub fn transition_progress(&self, d: Tick, length: Tick) -> f32 {
        match self.envelope(crate::envelope::EnvelopeKind::TransitionProgress) {
            Some(env) => env.value_at(d).clamp(0.0, 1.0),
            None if length <= Tick::ZERO => 1.0,
            None => (d.0 as f64 / length.0 as f64).clamp(0.0, 1.0) as f32,
        }
    }

    /// Its snap offset flag's place, within it.
    pub fn snap_point(&self) -> Tick {
        self.snap_offset.clamp(Tick::ZERO, self.duration)
    }

    pub fn end(&self) -> Tick {
        self.start + self.duration
    }

    pub fn range(&self) -> TimeRange {
        TimeRange::new(self.start, self.duration)
    }

    /// Instante da mídia de origem correspondente ao instante `t` da timeline.
    pub fn source_time(&self, t: Tick) -> Tick {
        if let Some(f) = self.freeze {
            return f;
        }
        let s = self.source_in + self.source_offset(t - self.start);
        match self.reverse {
            Some(mirror) => mirror - s,
            None => s,
        }
    }

    /// Media time shown at timeline time `t`, for a media `media_len` long:
    /// with Loop it wraps around (the media repeats); without it, it may lie
    /// past the media's end.
    pub fn media_time(&self, t: Tick, media_len: Tick) -> Tick {
        let s = self.source_time(t);
        if self.freeze.is_some() || !self.looped || media_len <= Tick::ZERO {
            return s;
        }
        Tick(s.0.rem_euclid(media_len.0))
    }

    /// Timeline instants inside the event that show media time `at` (each
    /// repeat, with Loop), for a media `media_len` long.
    pub fn media_points(&self, at: Tick, media_len: Tick) -> Vec<Tick> {
        if self.freeze.is_some() || self.velocity().is_some() || self.reverse.is_some() {
            return Vec::new();
        }
        let place = |m: Tick| self.start + self.to_timeline(m - self.source_in);
        let inside = |t: &Tick| *t >= self.start && *t <= self.end();
        if !self.looped || media_len <= Tick::ZERO {
            return Some(place(at)).filter(inside).into_iter().collect();
        }
        let last = self.source_time(self.end());
        let mut k = (self.source_in.0 - at.0).div_euclid(media_len.0);
        let mut out = Vec::new();
        while at.0 + k * media_len.0 <= last.0 && out.len() < 1000 {
            let t = place(Tick(at.0 + k * media_len.0));
            if inside(&t) {
                out.push(t);
            }
            k += 1;
        }
        out
    }

    /// Timeline instants inside the event where the media starts over
    /// (VEGAS's loop notches), for a media `media_len` long.
    pub fn loop_points(&self, media_len: Tick) -> Vec<Tick> {
        if !self.looped
            || self.freeze.is_some()
            || self.velocity().is_some()
            || self.reverse.is_some()
            || media_len <= Tick::ZERO
        {
            return Vec::new();
        }
        let first = self.source_in;
        let last = self.source_time(self.end());
        let mut k = first.0.div_euclid(media_len.0) + 1;
        let mut out = Vec::new();
        while k * media_len.0 < last.0 && out.len() < 1000 {
            let at = self.start + self.to_timeline(Tick(k * media_len.0) - self.source_in);
            if at > self.start && at < self.end() {
                out.push(at);
            }
            k += 1;
        }
        out
    }

    /// Duração na mídia correspondente a `d` na timeline (considera a velocidade).
    pub fn to_source(&self, d: Tick) -> Tick {
        if self.rate == 1.0 {
            d
        } else {
            Tick((d.0 as f64 * self.rate).round() as i64)
        }
    }

    /// Duração na timeline correspondente a `d` na mídia.
    pub fn to_timeline(&self, d: Tick) -> Tick {
        if self.rate == 1.0 {
            d
        } else {
            Tick((d.0 as f64 / self.rate).floor() as i64)
        }
    }

    /// Enquadramento no instante `t` da timeline (com quadros-chave).
    pub fn transform_at(&self, t: Tick) -> Transform {
        crate::fx::transform_at(&self.transform_keys, &self.transform, t - self.start)
    }

    /// How many times higher its sound plays: its pitch change, and its
    /// speed too unless it preserves its pitch.
    pub fn pitch_factor(&self) -> f64 {
        let shift = 2f64.powf(f64::from(self.pitch) / 12.0);
        if self.preserve_pitch {
            shift
        } else {
            shift * self.rate
        }
    }

    /// Fim do trecho usado da mídia.
    pub fn source_end(&self) -> Tick {
        self.source_in + self.to_source(self.duration)
    }
}

/// An event's sync link: the event it follows when that one moves (not
/// the other way round), and its offset from that event's start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncLink {
    pub to: ClipId,
    pub offset: Tick,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub kind: TrackKind,
    pub name: String,
    pub muted: bool,
    pub solo: bool,
    /// Opacidade (vídeo) ou ganho linear (áudio) da trilha inteira.
    #[serde(default = "one")]
    pub level: f32,
    /// Pan de áudio: −1 esquerda, 0 centro, +1 direita.
    #[serde(default)]
    pub pan: f32,
    /// How the pan places the sound (its header's Pan Type), for its pan
    /// slider and its pan envelope.
    #[serde(default, skip_serializing_if = "PanMode::is_default")]
    pub pan_mode: PanMode,
    /// Video: how its picture combines with what is below it (its
    /// Compositing Mode); a compositing parent's, with its children.
    #[serde(default, skip_serializing_if = "CompositeMode::is_default")]
    pub composite_mode: CompositeMode,
    /// Video, a compositing parent: how its group combines with what is
    /// below the group (its Parent Composite Mode).
    #[serde(default, skip_serializing_if = "CompositeMode::is_default")]
    pub parent_mode: CompositeMode,
    /// Video, its Compositing Mode Custom: the compositor plug-in it runs
    /// (VEGAS's Compositors window) — its picture guides how what is below
    /// it, its children's group for a parent, is drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compositor: Option<Box<PluginFx>>,
    /// Video, a compositing parent whose Parent Composite Mode is Custom:
    /// the compositor its group goes through onto what is below it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_compositor: Option<Box<PluginFx>>,
    /// Video: how far in it is among compositing children (0: not a
    /// child; a child is one in from the video track above it at least).
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub composite_depth: u8,
    /// Video: an adjustment track (Insert › Adjustment Track): its events
    /// go over what the tracks below it show (its compositing children's
    /// only, when it has some), then its effects work on all of that, its
    /// level their strength.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub adjustment: bool,
    /// Video: its Track Motion (Tools › Video › Track Motion, its header's
    /// button): where its picture goes in the frame, its 2D shadow and glow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<crate::motion::TrackMotion>,
    /// Video: a compositing parent's Parent Motion, its whole group moved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_motion: Option<crate::motion::TrackMotion>,
    /// The colors its Fade to Color envelope fades to: toward the top and
    /// toward the bottom (8-bit video values, alpha); `None`: white and
    /// black.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fade_colors: Option<[[u8; 4]; 2]>,
    #[serde(default)]
    pub effects: Vec<Effect>,
    /// Video: where its chain's Composite button stands (the Video Track
    /// FX window's, dragged there): the effects before it work on the
    /// track's own picture, those after it on its compositing group's
    /// once the track composited into it (VEGAS's post-compositing
    /// effects; only compositing parents and children have that step).
    /// `None`, VEGAS's default: the button last, all of them before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composite_at: Option<usize>,
    /// Ordenados por `start` (e por id em caso de empate).
    pub clips: Vec<Clip>,
    /// Volume, pan and mute automation (Insert › Envelopes).
    #[serde(default)]
    pub envelopes: Vec<crate::envelope::Envelope>,
    /// Audio: the whole track's polarity reversed (Invert Track Phase).
    #[serde(default)]
    pub invert_phase: bool,
    /// Video: left out of the video bus's motion blur and supersampling
    /// (its header's Bypass Motion Blur): drawn at the frame's time only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bypass_motion_blur: bool,
    /// Audio: armed for recording (its header's Arm for Record).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub armed: bool,
    /// Audio: where it records from (its header's Record Input); `None`:
    /// the default input, in stereo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_input: Option<RecordInput>,
    /// Audio: its input monitoring while armed (its Record Input menu).
    #[serde(default, skip_serializing_if = "crate::InputMonitor::is_off")]
    pub input_monitor: crate::InputMonitor,
    /// How its envelopes play and record (its header's Automation
    /// Settings), and its automation controls.
    #[serde(default)]
    pub automation: TrackAutomation,
    /// Audio, 5.1 projects: its Surround Panner (`None`, never panned:
    /// in the middle of the room, `surround::MIDDLE`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surround: Option<crate::surround::SurroundPan>,
    /// The track group it is in ([`crate::track_groups`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<crate::track_groups::TrackGroupId>,
    /// The track's color: one of the eight of the interface's palette
    /// (0 = the first); `None`: its kind's (the first for video tracks, the
    /// second for audio ones).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<u8>,
    /// Auto Ducking: an audio track whose sound lowers the listeners', by
    /// these reductions ([`crate::ducking`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ducking: Option<crate::ducking::Ducking>,
    /// An Auto Ducking listener: the controllers' sound lowers its own.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub duck_listener: bool,
    /// Audio: where it plays (its header's bus button): the Master bus or
    /// an audio bus ([`crate::busses`]).
    #[serde(default, skip_serializing_if = "crate::busses::Output::is_master")]
    pub output: crate::busses::Output,
    /// Audio: its sends to busses and assignable effects chains (its
    /// multipurpose slider).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sends: Vec<crate::busses::Send>,
}

fn is_zero_u8(v: &u8) -> bool {
    *v == 0
}

/// How a video track's picture combines with what is below it (its
/// Compositing Mode), and a compositing parent's group with what is below
/// the group (its Parent Composite Mode), as VEGAS's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompositeMode {
    /// The colors multiplied, and a light picture's darkness taking away
    /// what is below: a parent's mask (white shows its children, black
    /// hides them; where it is transparent it hides nothing).
    MultiplyMask,
    /// The picture over what is below, by its alpha channel.
    #[default]
    SourceAlpha,
    /// The colors added.
    Add,
    /// The picture's colors taken from what is below.
    Subtract,
    /// The picture cut out of what is below.
    Cut,
    /// The inverses multiplied: lighter.
    Screen,
    /// Multiply on the dark parts below, Screen on the light ones.
    Overlay,
    /// Multiply or Screen by the picture's own darkness or lightness.
    HardLight,
    /// What is below brightened by the picture.
    Dodge,
    /// What is below darkened by the picture.
    Burn,
    /// The darker of the two, channel by channel.
    Darken,
    /// The lighter of the two.
    Lighten,
    /// The darker taken from the lighter.
    Difference,
    /// The difference squared.
    DifferenceSquared,
    /// A compositor plug-in (the track's `compositor`, or its
    /// `parent_compositor` as a Parent Composite Mode): what is below drawn
    /// as the picture guides it — VEGAS's Displacement, Height and Bump
    /// Map. Without one, Source Alpha.
    Custom,
    /// 3D Source Alpha: the picture on a box in space (`space.rs`), drawn
    /// with the 3D tracks next to it, then over what is below by its
    /// alpha.
    ThreeDSourceAlpha,
}

impl CompositeMode {
    /// In VEGAS's order.
    pub const ALL: [CompositeMode; 14] = [
        CompositeMode::MultiplyMask,
        CompositeMode::SourceAlpha,
        CompositeMode::Add,
        CompositeMode::Subtract,
        CompositeMode::Cut,
        CompositeMode::Screen,
        CompositeMode::Overlay,
        CompositeMode::HardLight,
        CompositeMode::Dodge,
        CompositeMode::Burn,
        CompositeMode::Darken,
        CompositeMode::Lighten,
        CompositeMode::Difference,
        CompositeMode::DifferenceSquared,
    ];

    /// The modes a menu offers, as VEGAS's: 3D Source Alpha first.
    pub const MENU: [CompositeMode; 15] = [
        CompositeMode::ThreeDSourceAlpha,
        CompositeMode::MultiplyMask,
        CompositeMode::SourceAlpha,
        CompositeMode::Add,
        CompositeMode::Subtract,
        CompositeMode::Cut,
        CompositeMode::Screen,
        CompositeMode::Overlay,
        CompositeMode::HardLight,
        CompositeMode::Dodge,
        CompositeMode::Burn,
        CompositeMode::Darken,
        CompositeMode::Lighten,
        CompositeMode::Difference,
        CompositeMode::DifferenceSquared,
    ];

    /// 3D Source Alpha.
    pub fn is_three_d(self) -> bool {
        self == CompositeMode::ThreeDSourceAlpha
    }

    pub fn is_default(&self) -> bool {
        *self == CompositeMode::default()
    }

    /// The mode nearest a Photoshop layer's blend mode (its key in the
    /// document: `mul `, `scrn`…): Source Alpha for Normal and for those
    /// BananaV has no mode like.
    pub fn from_photoshop(key: [u8; 4]) -> CompositeMode {
        match &key {
            b"mul " => CompositeMode::MultiplyMask,
            b"scrn" => CompositeMode::Screen,
            b"over" => CompositeMode::Overlay,
            b"hLit" => CompositeMode::HardLight,
            b"div " => CompositeMode::Dodge,
            b"idiv" => CompositeMode::Burn,
            b"dark" => CompositeMode::Darken,
            b"lite" => CompositeMode::Lighten,
            b"diff" => CompositeMode::Difference,
            b"lddg" => CompositeMode::Add,
            b"fsub" => CompositeMode::Subtract,
            _ => CompositeMode::SourceAlpha,
        }
    }

    /// As VEGAS names it.
    pub fn name(self) -> &'static str {
        match self {
            CompositeMode::MultiplyMask => "Multiply (Mask)",
            CompositeMode::SourceAlpha => "Source Alpha",
            CompositeMode::Add => "Add",
            CompositeMode::Subtract => "Subtract",
            CompositeMode::Cut => "Cut",
            CompositeMode::Screen => "Screen",
            CompositeMode::Overlay => "Overlay",
            CompositeMode::HardLight => "Hard Light",
            CompositeMode::Dodge => "Dodge",
            CompositeMode::Burn => "Burn",
            CompositeMode::Darken => "Darken",
            CompositeMode::Lighten => "Lighten",
            CompositeMode::Difference => "Difference",
            CompositeMode::DifferenceSquared => "Difference Squared",
            CompositeMode::Custom => "Custom",
            CompositeMode::ThreeDSourceAlpha => "3D Source Alpha",
        }
    }

    /// Its number for the graphics card's kernel (`ALL`'s index).
    pub fn index(self) -> u32 {
        CompositeMode::ALL
            .iter()
            .position(|&m| m == self)
            .unwrap_or(1) as u32
    }

    /// `top` (premultiplied RGBA) combined with `under` as this mode says:
    /// the separable modes as the W3C's Compositing and Blending's
    /// formulas (each blended color where both show, the other's where
    /// one does); Add and Subtract on the colors; Cut takes `top`'s
    /// shape out of `under`; Multiply (Mask) multiplies `under` by `top`
    /// (its alpha by `top`'s luma), transparent parts of `top` leaving
    /// `under` as it is.
    pub fn blend(self, top: [f32; 4], under: [f32; 4]) -> [f32; 4] {
        let (sa, da) = (top[3], under[3]);
        match self {
            CompositeMode::SourceAlpha
            | CompositeMode::Custom
            | CompositeMode::ThreeDSourceAlpha => {
                let k = 1.0 - sa;
                [
                    top[0] + under[0] * k,
                    top[1] + under[1] * k,
                    top[2] + under[2] * k,
                    sa + da * k,
                ]
            }
            CompositeMode::Add => [
                top[0] + under[0],
                top[1] + under[1],
                top[2] + under[2],
                (sa + da).min(1.0),
            ],
            CompositeMode::Subtract => [
                (under[0] - top[0]).max(0.0),
                (under[1] - top[1]).max(0.0),
                (under[2] - top[2]).max(0.0),
                da,
            ],
            CompositeMode::Cut => under.map(|c| c * (1.0 - sa)),
            CompositeMode::MultiplyMask => {
                let m = [0, 1, 2].map(|i| 1.0 - sa + top[i]);
                let luma = 0.2126 * top[0] + 0.7152 * top[1] + 0.0722 * top[2];
                [
                    under[0] * m[0],
                    under[1] * m[1],
                    under[2] * m[2],
                    da * (1.0 - sa + luma),
                ]
            }
            separable => {
                let straight = |c: f32, a: f32| if a > 0.0 { c / a } else { 0.0 };
                let mut out = [0.0f32; 4];
                for i in 0..3 {
                    let (cs, cb) = (straight(top[i], sa), straight(under[i], da));
                    let b = separable.mix(cb, cs);
                    out[i] = top[i] * (1.0 - da) + under[i] * (1.0 - sa) + sa * da * b;
                }
                out[3] = sa + da * (1.0 - sa);
                out
            }
        }
    }

    /// A separable mode's blended color of `cb` (below) and `cs` (top),
    /// straight colors.
    fn mix(self, cb: f32, cs: f32) -> f32 {
        let multiply = |a: f32, b: f32| a * b;
        let screen = |a: f32, b: f32| a + b - a * b;
        let hard = |b: f32, s: f32| {
            if s <= 0.5 {
                multiply(b, 2.0 * s)
            } else {
                screen(b, 2.0 * s - 1.0)
            }
        };
        match self {
            CompositeMode::Screen => screen(cb, cs),
            CompositeMode::Overlay => hard(cs, cb),
            CompositeMode::HardLight => hard(cb, cs),
            CompositeMode::Dodge => {
                if cb <= 0.0 {
                    0.0
                } else if cs >= 1.0 {
                    1.0
                } else {
                    (cb / (1.0 - cs)).min(1.0)
                }
            }
            CompositeMode::Burn => {
                if cb >= 1.0 {
                    1.0
                } else if cs <= 0.0 {
                    0.0
                } else {
                    1.0 - ((1.0 - cb) / cs).min(1.0)
                }
            }
            CompositeMode::Darken => cb.min(cs),
            CompositeMode::Lighten => cb.max(cs),
            CompositeMode::Difference => (cb - cs).abs(),
            CompositeMode::DifferenceSquared => (cb - cs) * (cb - cs),
            _ => cs,
        }
    }
}

/// How a track's pan places its sound between the speakers: its header's
/// Pan Type, as VEGAS's (Audio panning modes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PanMode {
    /// Panned toward a side, the other side's channel is added to it (a
    /// straight line), until both play on that side at full volume.
    AddChannels,
    /// Balance: the side panned toward goes from the center's level to
    /// 0 dB, the other from it to silence (straight lines); the center
    /// at 0 dB.
    #[default]
    Balance0,
    /// Balance with the center 3 dB down.
    Balance3,
    /// Balance with the center 6 dB down.
    Balance6,
    /// The sound keeps its power as it moves (the sine law): 3 dB down
    /// each side at the center, one channel alone at either end.
    ConstantPower,
    /// Constant power between neighbouring speakers; in stereo, as
    /// Constant Power.
    Film,
}

impl PanMode {
    pub const ALL: [PanMode; 6] = [
        PanMode::AddChannels,
        PanMode::Balance0,
        PanMode::Balance3,
        PanMode::Balance6,
        PanMode::ConstantPower,
        PanMode::Film,
    ];

    pub fn is_default(&self) -> bool {
        *self == PanMode::default()
    }

    /// As VEGAS lists it.
    pub fn name(self) -> &'static str {
        match self {
            PanMode::AddChannels => "Add Channels",
            PanMode::Balance0 => "Balance (0 dB center)",
            PanMode::Balance3 => "Balance (-3 dB center)",
            PanMode::Balance6 => "Balance (-6 dB center)",
            PanMode::ConstantPower => "Constant Power",
            PanMode::Film => "Film",
        }
    }

    /// What the left and right outputs take of the left and right
    /// channels at `pan` (−1 left … +1 right): `[[left from left, left
    /// from right], [right from left, right from right]]`.
    pub fn matrix(self, pan: f32) -> [[f32; 2]; 2] {
        let p = pan.clamp(-1.0, 1.0);
        let (right, amount) = (p > 0.0, p.abs());
        match self {
            PanMode::AddChannels => {
                if right {
                    [[1.0 - amount, 0.0], [amount, 1.0]]
                } else {
                    [[1.0, amount], [0.0, 1.0 - amount]]
                }
            }
            PanMode::Balance0 | PanMode::Balance3 | PanMode::Balance6 => {
                let center = match self {
                    PanMode::Balance0 => 1.0,
                    PanMode::Balance3 => 0.707_945_8,
                    _ => 0.501_187_2,
                };
                let toward = center + (1.0 - center) * amount;
                let away = center * (1.0 - amount);
                if right {
                    [[away, 0.0], [0.0, toward]]
                } else {
                    [[toward, 0.0], [0.0, away]]
                }
            }
            PanMode::ConstantPower | PanMode::Film => {
                let angle = (p + 1.0) * std::f32::consts::FRAC_PI_4;
                [[angle.cos(), 0.0], [0.0, angle.sin()]]
            }
        }
    }
}

/// How a track's envelopes play and record: its header's Automation
/// Settings, as VEGAS's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AutomationMode {
    /// The envelopes do not play: the automation controls set the track's
    /// volume, pan or level (a second trim).
    Off,
    /// The envelopes play; the automation controls follow them.
    #[default]
    Read,
    /// Moving an automation control while playing records it, until it is
    /// let go.
    Touch,
    /// Moving an automation control while playing records it, until
    /// playback stops.
    Latch,
}

impl AutomationMode {
    pub const ALL: [AutomationMode; 4] = [
        AutomationMode::Off,
        AutomationMode::Read,
        AutomationMode::Touch,
        AutomationMode::Latch,
    ];

    /// Whether moving a control records.
    pub fn writes(self) -> bool {
        matches!(self, AutomationMode::Touch | AutomationMode::Latch)
    }
}

/// A track's automation settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackAutomation {
    pub mode: AutomationMode,
    /// The header's volume and pan (video: level) sliders are automation
    /// controls instead of trims ("Activate Automation Controls").
    pub controls: bool,
    /// With automation off: the values its automation controls hold.
    pub volume: f32,
    pub pan: f32,
    pub level: f32,
    /// Video: the Fade slider's (fade to color, −1 bottom … 1 top).
    pub fade: f32,
}

impl Default for TrackAutomation {
    fn default() -> TrackAutomation {
        TrackAutomation {
            mode: AutomationMode::Read,
            controls: false,
            volume: 1.0,
            pan: 0.0,
            level: 1.0,
            fade: 0.0,
        }
    }
}

/// How many colors the interface's track palette has.
pub const TRACK_PALETTE: u8 = 8;

impl Track {
    /// The palette color the track shows: its own, else its kind's (the
    /// first for video, the second for audio).
    pub fn color_index(&self) -> usize {
        let kind = match self.kind {
            TrackKind::Video => 0,
            TrackKind::Audio => 1,
        };
        (self.color.unwrap_or(kind) % TRACK_PALETTE) as usize
    }

    /// What playback takes from the track's `kind` envelope at `t`: `None`
    /// without one. With automation off the envelopes do not play: the
    /// automation controls' volume, pan and level, and nothing else.
    pub fn automation_at(&self, kind: crate::envelope::EnvelopeKind, t: Tick) -> Option<f32> {
        use crate::envelope::EnvelopeKind;
        if self.automation.mode == AutomationMode::Off {
            return match kind {
                EnvelopeKind::Volume => Some(self.automation.volume),
                EnvelopeKind::Pan => Some(self.automation.pan),
                EnvelopeKind::CompositeLevel => Some(self.automation.level),
                EnvelopeKind::FadeToColor => Some(self.automation.fade),
                _ => None,
            };
        }
        self.envelopes
            .iter()
            .find(|e| e.kind == kind)
            .map(|e| e.value_at(t))
    }

    /// Whether playback takes anything from automation (envelopes, or the
    /// automation controls with automation off).
    pub fn has_automation(&self) -> bool {
        !self.envelopes.is_empty() || self.automation.mode == AutomationMode::Off
    }
}

fn one() -> f32 {
    1.0
}

fn is_one_f32(v: &f32) -> bool {
    *v == 1.0
}

fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}

/// Where an audio track records from: an input device and its channels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordInput {
    /// The input device's name; empty: the system's default input.
    #[serde(default)]
    pub device: String,
    /// The first channel taken (0: the left one) and how many: 1 (mono)
    /// or 2 (stereo).
    #[serde(default)]
    pub first: u16,
    #[serde(default = "two")]
    pub channels: u16,
    /// An input bus instead (its input, through its effects).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bus: Option<crate::InputBusId>,
}

fn two() -> u16 {
    2
}

impl Default for RecordInput {
    fn default() -> RecordInput {
        RecordInput {
            device: String::new(),
            first: 0,
            channels: 2,
            bus: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MarkerId(pub u64);

/// Marcador numa posição da timeline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    pub id: MarkerId,
    pub time: Tick,
    pub name: String,
}

/// Região nomeada da timeline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub id: MarkerId,
    pub range: TimeRange,
    pub name: String,
}

/// The master audio bus.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MasterBus {
    pub gain_db: f32,
    pub muted: bool,
    /// In 5.1 projects, the Surround Master's faders: each channel's trim
    /// (dB; L, R, C, LFE, Ls, Rs), never automated, after the volume.
    #[serde(default, skip_serializing_if = "is_flat")]
    pub channel_db: [f32; 6],
    /// Lock Fader off: the channel faders move one by one (on, together).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unlocked: bool,
    /// Its Audio Device Selector: the pair of the output device's channels
    /// (0: its first two) each of its own pairs plays on — L/R (a stereo
    /// project's), C/LFE and Ls/Rs, in its channels' order. Playback only.
    #[serde(default = "same_pairs", skip_serializing_if = "is_same_pairs")]
    pub device_pairs: [u8; 3],
}

fn is_flat(db: &[f32; 6]) -> bool {
    db.iter().all(|&d| d == 0.0)
}

/// Each of the Master's pairs on the device's pair in its place.
pub const SAME_PAIRS: [u8; 3] = [0, 1, 2];

fn same_pairs() -> [u8; 3] {
    SAME_PAIRS
}

fn is_same_pairs(p: &[u8; 3]) -> bool {
    *p == SAME_PAIRS
}

impl Default for MasterBus {
    fn default() -> MasterBus {
        MasterBus {
            gain_db: 0.0,
            muted: false,
            channel_db: [0.0; 6],
            unlocked: false,
            device_pairs: SAME_PAIRS,
        }
    }
}

impl Track {
    /// Clipes que contêm o instante `t`.
    pub fn clips_at(&self, t: Tick) -> impl Iterator<Item = &Clip> {
        self.clips.iter().filter(move |c| c.range().contains(t))
    }

    /// O clipe visível em `t`: entre os que contêm `t`, o que começa por último.
    pub fn top_clip_at(&self, t: Tick) -> Option<&Clip> {
        self.clips_at(t).last()
    }

    pub fn clips_overlapping(&self, range: TimeRange) -> impl Iterator<Item = &Clip> {
        self.clips
            .iter()
            .filter(move |c| c.range().intersects(range))
    }

    /// How far other events of the track overlap the clip's start (the
    /// event before it) and its end (the event after it), muted ones left
    /// out: the automatic crossfades at its edges.
    pub fn crossfades(&self, clip: &Clip) -> (Option<Tick>, Option<Tick>) {
        let (mut fade_in, mut fade_out) = (None::<Tick>, None::<Tick>);
        for other in self.clips.iter().filter(|o| o.id != clip.id && !o.muted) {
            if other.start <= clip.start && other.end() > clip.start {
                let o = (other.end() - clip.start).min(clip.duration);
                fade_in = Some(fade_in.map_or(o, |f| f.max(o)));
            }
            if other.start > clip.start && other.start < clip.end() {
                let o = clip.end() - other.start;
                fade_out = Some(fade_out.map_or(o, |f| f.max(o)));
            }
        }
        (fade_in, fade_out)
    }

    /// The clip's fades (in, out). Where another event overlaps it, the
    /// automatic crossfade takes the place of its own fade, as VEGAS's:
    /// both would fade it twice.
    pub fn effective_fades(&self, clip: &Clip, auto_crossfade: bool) -> (Tick, Tick) {
        if !auto_crossfade {
            return (clip.fade_in, clip.fade_out);
        }
        let (fade_in, fade_out) = self.crossfades(clip);
        (
            fade_in.unwrap_or(clip.fade_in),
            fade_out.unwrap_or(clip.fade_out),
        )
    }

    pub(crate) fn sort(&mut self) {
        self.clips.sort_by_key(|c| (c.start, c.id));
    }
}

/// File › Properties › Video › Pixel format: the precision and range the
/// video is processed in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelFormat {
    /// Legacy 8-bit (video levels).
    Legacy8BitVideoLevels,
    /// 8-bit (full range): composited in video gamma, values clipped to
    /// 0–1 — new projects', as VEGAS's: effects get the pictures' values,
    /// as VEGAS's measured ones do.
    #[default]
    Full8Bit,
    /// 32-bit floating point (video levels).
    Float32VideoLevels,
    /// 32-bit floating point (full range): composited in linear light
    /// (or video gamma, the compositing gamma says).
    Float32Full,
}

/// The pixel format of projects saved before it was a setting: they were
/// composited in 32-bit linear light, and keep it.
fn older_projects_format() -> PixelFormat {
    PixelFormat::Float32Full
}

impl PixelFormat {
    pub const ALL: [PixelFormat; 4] = [
        PixelFormat::Legacy8BitVideoLevels,
        PixelFormat::Full8Bit,
        PixelFormat::Float32VideoLevels,
        PixelFormat::Float32Full,
    ];

    pub fn is_float(self) -> bool {
        matches!(
            self,
            PixelFormat::Float32VideoLevels | PixelFormat::Float32Full
        )
    }

    /// The video levels formats: Y′CbCr media comes in as studio RGB and
    /// goes out as it is (its 16–235 kept both ways), as VEGAS's.
    pub fn is_video_levels(self) -> bool {
        matches!(
            self,
            PixelFormat::Legacy8BitVideoLevels | PixelFormat::Float32VideoLevels
        )
    }
}

/// The gamma video is composited in: blends, fades, levels and effects work
/// on light (linear) or on the encoded values (video gamma, like 8-bit
/// editors).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompositingGamma {
    /// 1.000 (Linear).
    #[default]
    Linear,
    /// 2.222 (Video).
    Video,
}

/// File › Properties › Video › Output rotation: how the finished frame is
/// turned for the preview and the rendered files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputRotation {
    /// 0° (original).
    #[default]
    None,
    /// 90° clockwise.
    Clockwise90,
    /// 180° (inverted).
    Inverted180,
    /// 90° counterclockwise.
    Counterclockwise90,
}

impl OutputRotation {
    pub const ALL: [OutputRotation; 4] = [
        OutputRotation::None,
        OutputRotation::Clockwise90,
        OutputRotation::Inverted180,
        OutputRotation::Counterclockwise90,
    ];

    /// `true` when width and height trade places.
    pub fn is_quarter_turn(self) -> bool {
        matches!(
            self,
            OutputRotation::Clockwise90 | OutputRotation::Counterclockwise90
        )
    }
}

/// File › Properties › Video › Full-resolution rendering quality: how
/// images are resampled when they are placed in the frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RenderQuality {
    /// Nearest pixel, no filtering.
    Draft,
    /// Bilinear, no prefiltering of large reductions.
    Preview,
    /// Bilinear; large reductions are averaged first.
    #[default]
    Good,
    /// Bicubic; large reductions are averaged first.
    Best,
}

impl RenderQuality {
    pub const ALL: [RenderQuality; 4] = [
        RenderQuality::Draft,
        RenderQuality::Preview,
        RenderQuality::Good,
        RenderQuality::Best,
    ];
}

/// File › Properties › Audio: quality of the audio resampling and stretching
/// and of the LFE low-pass filter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioQuality {
    /// Resampling: linear interpolation. LFE filter: 12 dB/octave.
    Preview,
    /// Resampling: cubic interpolation. LFE filter: 24 dB/octave.
    #[default]
    Good,
    /// Resampling: windowed sinc. LFE filter: 48 dB/octave.
    Best,
}

impl AudioQuality {
    pub const ALL: [AudioQuality; 3] = [
        AudioQuality::Preview,
        AudioQuality::Good,
        AudioQuality::Best,
    ];
}

/// How the time ruler shows time (File › Properties › Ruler, Options ›
/// Ruler Format).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RulerFormat {
    Samples,
    Time,
    Seconds,
    TimeAndFrames,
    AbsoluteFrames,
    /// measures.beats.ticks at the project tempo (64 ticks per beat).
    MeasuresBeats,
    Feet16,
    Feet35,
    SmpteFilmSyncIvtc,
    SmpteFilm,
    SmpteEbu,
    SmpteNonDrop,
    SmpteDrop,
    Smpte30,
    /// hh:mm:ss:ff at 75 frames a second, an audio CD's (Red Book) frames;
    /// never shifted by the ruler's offset (a CD starts at 0).
    AudioCd,
}

impl RulerFormat {
    /// In VEGAS's order.
    pub const ALL: [RulerFormat; 15] = [
        RulerFormat::Samples,
        RulerFormat::Time,
        RulerFormat::Seconds,
        RulerFormat::TimeAndFrames,
        RulerFormat::AbsoluteFrames,
        RulerFormat::MeasuresBeats,
        RulerFormat::Feet16,
        RulerFormat::Feet35,
        RulerFormat::SmpteFilmSyncIvtc,
        RulerFormat::SmpteFilm,
        RulerFormat::SmpteEbu,
        RulerFormat::SmpteNonDrop,
        RulerFormat::SmpteDrop,
        RulerFormat::Smpte30,
        RulerFormat::AudioCd,
    ];

    /// Whether the ruler's start time (its offset) applies: an audio CD
    /// always starts at 00:00:00:00.
    pub fn uses_offset(self) -> bool {
        self != RulerFormat::AudioCd
    }

    /// The SMPTE format that matches a frame rate (the ruler's format while
    /// the project has not picked one).
    pub fn for_rate(rate: FrameRate) -> RulerFormat {
        match (rate.num, rate.den) {
            (24000, 1001) => RulerFormat::SmpteFilmSyncIvtc,
            (24, 1) => RulerFormat::SmpteFilm,
            (25, 1) | (50, 1) => RulerFormat::SmpteEbu,
            (30000, 1001) | (60000, 1001) => RulerFormat::SmpteDrop,
            (30, 1) | (60, 1) => RulerFormat::Smpte30,
            _ => RulerFormat::TimeAndFrames,
        }
    }
}

/// The project tempo (File › Properties › Ruler › Measures & Beats): the
/// scale of the Measures & Beats ruler and the metronome.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tempo {
    pub beats_per_minute: f64,
    pub beats_per_measure: u32,
    /// The note that gets one beat: 1 (whole), 2, 4 (quarter), 8, 16 or 32.
    pub beat_note: u32,
}

impl Default for Tempo {
    fn default() -> Tempo {
        Tempo {
            beats_per_minute: 120.0,
            beats_per_measure: 4,
            beat_note: 4,
        }
    }
}

impl Tempo {
    /// Length of one beat.
    pub fn beat(&self) -> Tick {
        Tick::from_seconds_f64(60.0 / self.beats_per_minute.max(1e-3))
    }
}

/// File › Properties › Audio CD: what a Disc-at-Once audio CD of the
/// project carries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioCd {
    /// Universal Product Code / Media Catalog Number (13 digits), written
    /// to the disc to identify it.
    #[serde(default)]
    pub catalog_number: String,
    /// Number of the disc's first track (1 to 99).
    #[serde(default = "first_cd_track")]
    pub first_track: u32,
}

fn first_cd_track() -> u32 {
    1
}

impl Default for AudioCd {
    fn default() -> AudioCd {
        AudioCd {
            catalog_number: String::new(),
            first_track: 1,
        }
    }
}

/// File › Properties › Summary: information about the project.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    /// Who mixed or edited the project.
    #[serde(default)]
    pub engineer: String,
    #[serde(default)]
    pub copyright: String,
    #[serde(default)]
    pub comments: String,
}

fn default_bit_depth() -> u8 {
    16
}

fn default_lfe_cutoff() -> u32 {
    120
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectSettings {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    pub sample_rate: SampleRate,
    pub channels: ChannelLayout,
    #[serde(default = "older_projects_format")]
    pub pixel_format: PixelFormat,
    /// Used by the 32-bit formats; the 8-bit ones always composite in video
    /// gamma.
    #[serde(default)]
    pub compositing_gamma: CompositingGamma,
    #[serde(default)]
    pub rotation: OutputRotation,
    #[serde(default)]
    pub render_quality: RenderQuality,
    /// File › Properties › Video › Motion blur type: how the frames the
    /// video bus's motion blur averages are weighed.
    #[serde(default)]
    pub motion_blur: crate::video_bus::MotionBlurType,
    /// File › Properties › Video › Resample mode: how the frames a media
    /// lacks are made, when it shows at another rate than its own (events
    /// may say otherwise: `Clip::resample`).
    #[serde(default, skip_serializing_if = "is_default")]
    pub resample: ResampleMode,
    /// File › Properties › Video › Field order: the frames' (rendering
    /// fields is still to come); media of another order is deinterlaced.
    #[serde(default, skip_serializing_if = "is_default")]
    pub field_order: bv_core::FieldOrder,
    /// File › Properties › Video › Deinterlace method: how interlaced
    /// media becomes whole frames.
    #[serde(default, skip_serializing_if = "is_default")]
    pub deinterlace: DeinterlaceMethod,
    /// Bits per audio sample (8, 16 or 24): the default of rendered WAV
    /// files and of recordings.
    #[serde(default = "default_bit_depth")]
    pub bit_depth: u8,
    /// How audio media is resampled to the project rate and stretched.
    #[serde(default)]
    pub resample_quality: AudioQuality,
    /// 5.1 projects: low-pass filter on the LFE channel.
    #[serde(default = "yes")]
    pub lfe_lowpass: bool,
    /// Cutoff of the LFE low-pass filter, in Hz.
    #[serde(default = "default_lfe_cutoff")]
    pub lfe_cutoff_hz: u32,
    /// Sharpness of the LFE low-pass filter.
    #[serde(default)]
    pub lfe_quality: AudioQuality,
    /// `None` = the SMPTE format of the frame rate.
    #[serde(default)]
    pub ruler_format: Option<RulerFormat>,
    #[serde(default)]
    pub tempo: Tempo,
    /// The shape of the frame's pixels (File › Properties › Video: "Pixel
    /// aspect ratio"); square for most projects.
    #[serde(default, skip_serializing_if = "PixelAspect::is_square")]
    pub pixel_aspect: PixelAspect,
    /// ACES (File › Properties › Video); it works with 32-bit floating
    /// point (full range) only.
    #[serde(default, skip_serializing_if = "is_default")]
    pub color: crate::ColorManagement,
    /// File › Properties › Advanced › 360 Output: the frames are the whole
    /// sphere around the camera, equirectangular. The Video Preview can
    /// look around in them (its 360° View) and renders say so (spherical
    /// metadata in MP4 and MOV files).
    #[serde(default, skip_serializing_if = "is_default")]
    pub output_360: bool,
    /// File › Properties › Video › "Adjust source media to better match
    /// project or render settings": standard definition media (or
    /// projects) whose shape is a few percent off the frame's fill it
    /// instead of leaving thin bars ([`crate::scene::adjust_factor`]). On
    /// for new projects, as VEGAS's; off for projects saved before it.
    #[serde(default, skip_serializing_if = "is_default")]
    pub adjust_source: bool,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// How interlaced media becomes whole frames (File › Properties › Video),
/// as VEGAS's Deinterlace method: only at Good and Best quality, where
/// a media's field order is not the project's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeinterlaceMethod {
    None,
    /// Both fields: each line half itself and a quarter of each line next
    /// to it (high detail, little motion). VEGAS's default.
    #[default]
    BlendFields,
    /// One field: its lines kept, the other's the mean of the lines around
    /// them (much motion, little detail).
    Interpolate,
    /// Where the lines of the other field match the field kept (no
    /// motion: still detail), they stay; where they stand out from what
    /// the lines around them make (the comb motion leaves), they are made
    /// from those — a blend of Interpolate and keeping both fields, line
    /// by line (`SMART_THRESHOLDS`). VEGAS's runs on the graphics card;
    /// BananaV's on both.
    SmartAdaptive,
}

/// Smart adaptive deinterlacing: below the first difference (of the luma,
/// 0–1) between a line of the other field and the mean of the lines around
/// it, the line stays; past the second, it is that mean; in between, a
/// mix.
pub const SMART_THRESHOLDS: [f32; 2] = [0.03, 0.10];

/// How the frames a media lacks are made when it shows at another rate
/// than its own — its frame rate not the project's, an event slowed or
/// sped up —, as VEGAS's Resample mode says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResampleMode {
    /// The frames before and after the instant shown mixed, by how near it
    /// is to each.
    FrameBlend,
    /// Frames made by following what moves: the frame between made along
    /// the dense optical flow from the frame before to the next.
    OpticalFlow,
    /// The frame of the instant shown, as it is: VEGAS's default for new
    /// projects.
    #[default]
    Disable,
}

impl Default for ProjectSettings {
    fn default() -> ProjectSettings {
        ProjectSettings {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::FPS_29_97,
            sample_rate: SampleRate::HZ_48000,
            channels: ChannelLayout::Stereo,
            pixel_format: PixelFormat::default(),
            compositing_gamma: CompositingGamma::default(),
            rotation: OutputRotation::default(),
            render_quality: RenderQuality::default(),
            motion_blur: Default::default(),
            resample: ResampleMode::default(),
            field_order: Default::default(),
            deinterlace: DeinterlaceMethod::default(),
            bit_depth: default_bit_depth(),
            resample_quality: AudioQuality::default(),
            lfe_lowpass: true,
            lfe_cutoff_hz: default_lfe_cutoff(),
            lfe_quality: AudioQuality::default(),
            ruler_format: None,
            tempo: Tempo::default(),
            pixel_aspect: PixelAspect::SQUARE,
            color: crate::ColorManagement::default(),
            output_360: false,
            adjust_source: true,
        }
    }
}

impl ProjectSettings {
    /// Size of the finished frame: the project size, turned by the output
    /// rotation.
    pub fn output_size(&self) -> (u32, u32) {
        if self.rotation.is_quarter_turn() {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    /// `true` when video is composited on the encoded values instead of on
    /// light. Under ACES too, as VEGAS: with 2.222 its view takes the
    /// pictures' encoded values for ACES's light (measured on the
    /// calibration card: every gray to half a level).
    pub fn composites_in_video_gamma(&self) -> bool {
        // Only 32-bit floating point (full range) has a compositing gamma
        // to choose; the others work on the encoded values (2.222).
        self.pixel_format != PixelFormat::Float32Full
            || self.compositing_gamma == CompositingGamma::Video
    }

    /// Whether ACES works: 32-bit floating point (full range) and a view
    /// transform.
    pub fn color_managed(&self) -> bool {
        self.pixel_format == PixelFormat::Float32Full && self.color.view.is_some()
    }

    /// Whether the look modification transform applies: under ACES in
    /// linear light (VEGAS greys it out with the 2.222 gamma).
    pub fn look_applies(&self) -> bool {
        self.color_managed() && self.compositing_gamma == CompositingGamma::Linear
    }

    /// What the compositors do with colors, when ACES works.
    pub fn color_pipeline(&self) -> Option<std::sync::Arc<crate::ColorPipeline>> {
        if !self.color_managed() {
            return None;
        }
        let mut color = self.color;
        if !self.look_applies() {
            color.look = crate::AcesLook::None;
        }
        color.pipeline()
    }

    /// The ruler format in use.
    pub fn effective_ruler_format(&self) -> RulerFormat {
        self.ruler_format
            .unwrap_or_else(|| RulerFormat::for_rate(self.frame_rate))
    }
}

/// Uma camada de vídeo a compor num instante: qual clipe e em que ponto da mídia.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoLayer<'a> {
    pub track: &'a Track,
    pub clip: &'a Clip,
    pub source_time: Tick,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// Versão do formato do arquivo de projeto.
    pub format_version: u32,
    pub settings: ProjectSettings,
    pub media: Vec<MediaInfo>,
    /// Ordem de exibição: índice 0 é a trilha do topo.
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub markers: Vec<Marker>,
    #[serde(default)]
    pub regions: Vec<Region>,
    /// Command markers (Insert › Command, [`crate::commands`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<crate::commands::CommandMarker>,
    /// The audio CD layout's tracks and index markers ([`crate::cd`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cd_tracks: Vec<crate::cd::CdTrack>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cd_indices: Vec<crate::cd::CdIndex>,
    /// Seleção de tempo / região de loop (a barra acima da régua).
    #[serde(default)]
    pub loop_region: Option<TimeRange>,
    #[serde(default)]
    pub master: MasterBus,
    /// Clipes sobrepostos na mesma trilha fazem crossfade automático.
    #[serde(default = "yes")]
    pub auto_crossfade: bool,
    /// Project Notes' text before they were notes: brought into
    /// `project_notes` when the project opens ([`crate::notes`]).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// Project Notes (View › Window › Project Notes, [`crate::notes`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub project_notes: Vec<crate::notes::ProjectNote>,
    /// Short fades at every audio event edge to avoid clicks
    /// (Options › Quick Fade Audio Edits).
    #[serde(default = "yes")]
    pub quick_fades: bool,
    /// Options › Bypass All Audio FX.
    #[serde(default)]
    pub bypass_audio_fx: bool,
    /// Options › Mute All Video.
    #[serde(default)]
    pub mute_all_video: bool,
    /// Options › Ignore Event Grouping: every edit acts on single events.
    #[serde(default)]
    pub ignore_groups: bool,
    /// Options › Slip All Takes: a slip moves all of an event's takes, not
    /// only the active one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub slip_all_takes: bool,
    /// Options › Lock Envelopes to Events: a track envelope's points under
    /// an event move with it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lock_envelopes: bool,
    /// Effects applied to the whole video output (Tools › Video › Video
    /// Output FX).
    #[serde(default)]
    pub video_output_fx: Vec<Effect>,
    /// The video bus (View › Video Bus Track): the output's mute, its
    /// bypass and its envelopes.
    #[serde(
        default,
        skip_serializing_if = "crate::video_bus::VideoBus::is_default"
    )]
    pub video_bus: crate::video_bus::VideoBus,
    /// Time shown on the ruler at the project start (Options › Ruler Format
    /// › Set Time at Cursor).
    #[serde(default)]
    pub ruler_offset: Tick,
    /// Folder for prerendered video (File › Properties › Video); empty is
    /// the default folder.
    #[serde(default)]
    pub prerender_folder: String,
    /// Folder for recorded audio (File › Properties › Audio); empty is the
    /// default folder.
    #[serde(default)]
    pub recorded_folder: String,
    /// File › Properties › Summary.
    #[serde(default)]
    pub summary: Summary,
    /// File › Properties › Audio CD.
    #[serde(default)]
    pub audio_cd: AudioCd,
    /// Parts drawn ahead into files (Tools › Selectively Prerender Video).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prerenders: Vec<crate::prerender::Prerender>,
    /// Project Media's bins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bins: Vec<crate::bins::MediaBin>,
    /// Subclips: Project Media's items standing for part of a media
    /// ([`crate::subclips`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subclips: Vec<crate::subclips::Subclip>,
    /// Project Media's tags ([`crate::tags`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_tags: Vec<crate::tags::MediaTags>,
    /// Project Media's smart bins: searches kept ([`crate::tags`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub smart_bins: Vec<crate::tags::SmartBin>,
    /// Project Media's storyboards but the Main Timeline's
    /// ([`crate::storyboards`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub storyboards: Vec<crate::storyboards::Storyboard>,
    /// Audio busses (A to Z) and assignable effects chains
    /// ([`crate::busses`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub busses: Vec<crate::busses::Bus>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fx_chains: Vec<crate::busses::FxChain>,
    /// Input busses (A to Z, [`crate::input_busses`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_busses: Vec<crate::InputBus>,
    /// Track groups ([`crate::track_groups`]); each track names its own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub track_groups: Vec<crate::track_groups::TrackGroup>,
    /// The Master bus's effects (its Master FX), before its volume unless
    /// `master_fx_post_fader`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub master_fx: Vec<Effect>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub master_fx_post_fader: bool,
    /// Render's channel mapping ([`crate::busses::MappedStrip`]): the
    /// busses whose sound multichannel renders put in the file's channels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channel_mapping: Vec<crate::busses::MappedStrip>,
    pub(crate) next_id: u64,
}

fn yes() -> bool {
    true
}

impl Default for Project {
    fn default() -> Project {
        Project::new(ProjectSettings::default())
    }
}

impl Project {
    /// 2: fade-outs are named as VEGAS names them (`FadeCurve::out_shape`).
    pub const FORMAT_VERSION: u32 = 2;

    /// Projeto vazio com uma trilha de vídeo e uma de áudio.
    pub fn new(settings: ProjectSettings) -> Project {
        let mut p = Project {
            format_version: Self::FORMAT_VERSION,
            settings,
            media: Vec::new(),
            tracks: Vec::new(),
            markers: Vec::new(),
            regions: Vec::new(),
            commands: Vec::new(),
            cd_tracks: Vec::new(),
            cd_indices: Vec::new(),
            loop_region: None,
            master: MasterBus::default(),
            auto_crossfade: true,
            notes: String::new(),
            project_notes: Vec::new(),
            quick_fades: true,
            bypass_audio_fx: false,
            mute_all_video: false,
            ignore_groups: false,
            slip_all_takes: false,
            lock_envelopes: false,
            video_output_fx: Vec::new(),
            video_bus: Default::default(),
            ruler_offset: Tick::ZERO,
            prerender_folder: String::new(),
            recorded_folder: String::new(),
            summary: Summary::default(),
            audio_cd: AudioCd::default(),
            prerenders: Vec::new(),
            bins: Vec::new(),
            subclips: Vec::new(),
            media_tags: Vec::new(),
            smart_bins: Vec::new(),
            storyboards: Vec::new(),
            busses: Vec::new(),
            fx_chains: Vec::new(),
            input_busses: Vec::new(),
            track_groups: Vec::new(),
            master_fx: Vec::new(),
            master_fx_post_fader: false,
            channel_mapping: Vec::new(),
            next_id: 1,
        };
        p.add_track(TrackKind::Video);
        p.add_track(TrackKind::Audio);
        p
    }

    /// The next id to be given out: everything made after now (tracks,
    /// events, media) has an id at least this.
    pub fn id_counter(&self) -> u64 {
        self.next_id
    }

    pub(crate) fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn media(&self, id: MediaId) -> Option<&MediaInfo> {
        self.media.iter().find(|m| m.id == id)
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn track_index(&self, id: TrackId) -> Option<usize> {
        self.tracks.iter().position(|t| t.id == id)
    }

    /// Localiza um clipe: (índice da trilha, índice do clipe).
    pub fn find_clip(&self, id: ClipId) -> Option<(usize, usize)> {
        self.tracks
            .iter()
            .enumerate()
            .find_map(|(ti, t)| t.clips.iter().position(|c| c.id == id).map(|ci| (ti, ci)))
    }

    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.find_clip(id).map(|(t, c)| &self.tracks[t].clips[c])
    }

    /// Acesso direto para editar propriedades que não afetam a posição
    /// (nível, fades, efeitos, enquadramento). Para mover e aparar, use as
    /// operações de edição, que mantêm a ordem dos clipes.
    pub fn clip_mut(&mut self, id: ClipId) -> Option<&mut Clip> {
        self.find_clip(id)
            .map(|(t, c)| &mut self.tracks[t].clips[c])
    }

    pub fn track_by_id_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    pub fn clip_track(&self, id: ClipId) -> Option<&Track> {
        self.find_clip(id).map(|(t, _)| &self.tracks[t])
    }

    pub fn clips(&self) -> impl Iterator<Item = (&Track, &Clip)> {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(move |c| (t, c)))
    }

    /// Fim do último clipe.
    pub fn duration(&self) -> Tick {
        self.clips()
            .map(|(_, c)| c.end())
            .max()
            .unwrap_or(Tick::ZERO)
    }

    /// Todos os clipes do grupo de `id` (ou só ele, se não tiver grupo).
    /// The events an edit of `ids` reaches, sorted: with `grouped`, the
    /// members of their groups too (unless grouping is ignored).
    pub fn edit_reach(&self, ids: &[ClipId], grouped: bool) -> Vec<ClipId> {
        let mut all: Vec<ClipId> = if grouped {
            ids.iter().flat_map(|&id| self.group_members(id)).collect()
        } else {
            ids.iter()
                .copied()
                .filter(|&id| self.clip(id).is_some())
                .collect()
        };
        all.sort();
        all.dedup();
        all
    }

    pub fn group_members(&self, id: ClipId) -> Vec<ClipId> {
        let Some(clip) = self.clip(id) else {
            return Vec::new();
        };
        match clip.group {
            _ if self.ignore_groups => vec![id],
            None => vec![id],
            Some(g) => self
                .clips()
                .filter(|(_, c)| c.group == Some(g))
                .map(|(_, c)| c.id)
                .collect(),
        }
    }

    /// Camadas de vídeo visíveis em `t`, de baixo para cima (ordem de composição).
    pub fn video_layers_at(&self, t: Tick) -> Vec<VideoLayer<'_>> {
        self.tracks
            .iter()
            .rev()
            .filter(|tr| tr.kind == TrackKind::Video && !tr.muted)
            .filter_map(|track| {
                track.top_clip_at(t).map(|clip| VideoLayer {
                    track,
                    clip,
                    source_time: clip.source_time(t),
                })
            })
            .collect()
    }

    /// Trilhas de áudio audíveis, respeitando mute e solo.
    /// Video tracks that show: not muted and, when any video track is
    /// soloed, soloed.
    /// The project with every video effect bypassed — the events', the
    /// media's, the video tracks' and the Video Output FX: what the Video
    /// Preview's Split-Screen View shows beside it.
    pub fn video_fx_bypassed(&self) -> Project {
        let mut p = self.clone();
        let off = |fx: &mut Vec<Effect>| fx.iter_mut().for_each(|e| e.enabled = false);
        for t in p.tracks.iter_mut().filter(|t| t.kind == TrackKind::Video) {
            off(&mut t.effects);
            for c in &mut t.clips {
                off(&mut c.effects);
            }
        }
        for m in p.media.iter_mut().filter(|m| m.video.is_some()) {
            off(&mut m.effects);
        }
        off(&mut p.video_output_fx);
        p
    }

    /// The video tracks shown, as VEGAS's Help describes mute and solo
    /// with compositing parents: a track is hidden when it or a parent of
    /// it is muted (their groups' mute and solo counting, `track_muted`);
    /// soloing works among the tracks of one level under one parent — a
    /// soloed track hides its siblings that are not soloed, with their
    /// children, while the other levels play as they are.
    pub fn visible_video_tracks(&self) -> impl Iterator<Item = &Track> {
        let shown = self.video_tracks_shown();
        self.tracks
            .iter()
            .enumerate()
            .filter(move |(i, _)| shown.contains(i))
            .map(|(_, t)| t)
    }

    /// The indices of the video tracks shown (see
    /// [`visible_video_tracks`](Self::visible_video_tracks)).
    fn video_tracks_shown(&self) -> std::collections::HashSet<usize> {
        let videos: Vec<usize> = (0..self.tracks.len())
            .filter(|&i| self.tracks[i].kind == TrackKind::Video)
            .collect();
        // Each video track's compositing parent: the nearest video track
        // above it one level out.
        let mut parent: std::collections::HashMap<usize, Option<usize>> =
            std::collections::HashMap::new();
        for (k, &i) in videos.iter().enumerate() {
            let d = self.tracks[i].composite_depth;
            let p = (d > 0)
                .then(|| {
                    videos[..k]
                        .iter()
                        .rev()
                        .copied()
                        .find(|&j| self.tracks[j].composite_depth + 1 == d)
                })
                .flatten();
            parent.insert(i, p);
        }
        // The sibling sets (by parent) that have a soloed track.
        let soloed_sets: std::collections::HashSet<Option<usize>> = videos
            .iter()
            .filter(|&&i| self.track_soloed(&self.tracks[i]))
            .map(|i| parent[i])
            .collect();
        videos
            .iter()
            .copied()
            .filter(|&i| {
                // Up through its parents: none muted, none bypassed by a
                // soloed sibling.
                let mut at = Some(i);
                while let Some(n) = at {
                    let t = &self.tracks[n];
                    if self.track_muted(t)
                        || (soloed_sets.contains(&parent[&n]) && !self.track_soloed(t))
                    {
                        return false;
                    }
                    at = parent[&n];
                }
                true
            })
            .collect()
    }

    /// Ctrl+click on a track's Mute, as VEGAS's: on an unmuted track it
    /// mutes that one alone; on a muted one it unmutes every track.
    pub fn mute_alone(&mut self, id: TrackId) {
        let Some(muted) = self.track(id).map(|t| t.muted) else {
            return;
        };
        for t in &mut self.tracks {
            t.muted = !muted && t.id == id;
        }
    }

    /// Ctrl+click on a track's Solo, as VEGAS's: on an unsoloed track it
    /// solos that one alone; on a soloed one it unsoloes every track.
    pub fn solo_alone(&mut self, id: TrackId) {
        let Some(solo) = self.track(id).map(|t| t.solo) else {
            return;
        };
        for t in &mut self.tracks {
            t.solo = !solo && t.id == id;
        }
    }

    /// Whether audio track `t` is heard: not muted, and soloed when any
    /// audio track is.
    pub fn is_audible(&self, t: &Track) -> bool {
        t.kind == TrackKind::Audio
            && !self.track_muted(t)
            && (self.track_soloed(t)
                || !self
                    .tracks
                    .iter()
                    .any(|x| x.kind == TrackKind::Audio && self.track_soloed(x)))
    }

    pub fn audible_tracks(&self) -> impl Iterator<Item = &Track> {
        let any_solo = self
            .tracks
            .iter()
            .any(|t| t.kind == TrackKind::Audio && self.track_soloed(t));
        self.tracks.iter().filter(move |t| {
            t.kind == TrackKind::Audio
                && !self.track_muted(t)
                && (!any_solo || self.track_soloed(t))
        })
    }

    /// Instantes onde o cursor e as bordas de clipe "grudam" ao arrastar.
    pub fn snap_points(&self) -> Vec<Tick> {
        let mut pts: Vec<Tick> = std::iter::once(Tick::ZERO)
            .chain(self.clips().flat_map(|(_, c)| [c.start, c.end()]))
            .collect();
        pts.sort();
        pts.dedup();
        pts
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("projeto sempre serializável")
    }

    pub fn from_json(s: &str) -> Result<Project, serde_json::Error> {
        let mut p: Project = serde_json::from_str(s)?;
        for t in &mut p.tracks {
            t.sort();
        }
        // Track groups together, whatever wrote the file.
        p.normalize_groups();
        p.upgrade_notes();
        p.upgrade_plugin_fx();
        p.upgrade_fade_outs();
        Ok(p)
    }

    /// Projects saved before format 2 named a fade-out by its fade-in
    /// turned around in time; VEGAS, and BananaV since, by how it begins:
    /// their Fast and Slow swap, so each keeps its shape.
    fn upgrade_fade_outs(&mut self) {
        if self.format_version >= 2 {
            return;
        }
        for t in &mut self.tracks {
            for c in &mut t.clips {
                c.fade_out_curve = c.fade_out_curve.out_shape();
            }
        }
        self.format_version = Self::FORMAT_VERSION;
    }

    /// Brings the plug-in effects' values an older BananaV saved up to
    /// date ([`PluginFx::upgrade`]).
    fn upgrade_plugin_fx(&mut self) {
        let frame = (self.settings.width, self.settings.height);
        let upgrade = |fx: &mut Vec<Effect>| {
            for e in fx {
                if let Some(p) = e.plugin.as_deref_mut() {
                    p.upgrade(frame);
                }
            }
        };
        for t in &mut self.tracks {
            upgrade(&mut t.effects);
            for c in &mut t.clips {
                upgrade(&mut c.effects);
            }
        }
        for m in &mut self.media {
            upgrade(&mut m.effects);
        }
        upgrade(&mut self.video_output_fx);
    }

    /// Confere as invariantes do modelo. Usado nos testes e ao carregar projetos.
    pub fn check_invariants(&self) -> Result<(), String> {
        self.check_busses()?;
        self.check_track_groups()?;
        let mut seen = std::collections::HashSet::new();
        for t in &self.tracks {
            if !seen.insert(t.id.0) {
                return Err(format!("duplicate track id: {:?}", t.id));
            }
            for w in t.clips.windows(2) {
                if (w[0].start, w[0].id) > (w[1].start, w[1].id) {
                    return Err(format!("clips out of order on track {:?}", t.id));
                }
            }
            for c in &t.clips {
                if !seen.insert(c.id.0) {
                    return Err(format!("duplicate clip id: {:?}", c.id));
                }
                if c.id.0 >= self.next_id {
                    return Err(format!("id {:?} beyond the counter", c.id));
                }
                if c.duration <= Tick::ZERO {
                    return Err(format!("clip {:?} with length {}", c.id, c.duration));
                }
                if c.start < Tick::ZERO {
                    return Err(format!("clip {:?} before zero", c.id));
                }
                // With Loop an event may start before its media's start (it
                // repeats), as VEGAS's.
                if c.source_in < Tick::ZERO && !c.looped && c.freeze.is_none() {
                    return Err(format!("clip {:?} starts before its media", c.id));
                }
                let m = self
                    .media(c.media)
                    .ok_or_else(|| format!("clip {:?} points to missing media", c.id))?;
                if c.takes.len() == 1 || (!c.takes.is_empty() && c.active_take >= c.takes.len()) {
                    return Err(format!(
                        "clip {:?} with a lone or missing active take",
                        c.id
                    ));
                }
                for take in &c.takes {
                    let tm = self
                        .media(take.media)
                        .ok_or_else(|| format!("clip {:?} has a take of missing media", c.id))?;
                    if !tm.has(t.kind) {
                        return Err(format!("clip {:?} has a take without its stream", c.id));
                    }
                }
                if !m.has(t.kind) {
                    return Err(format!(
                        "clip {:?} without stream {:?} in the media",
                        c.id, t.kind
                    ));
                }
                // An event may run past its media's end, as VEGAS's: it loops,
                // holds the last frame or shows nothing (`Project::trim_end`).
                if !(0.01..=100.0).contains(&c.rate) {
                    return Err(format!("invalid rate on clip {:?}", c.id));
                }
                if c.fade_in < Tick::ZERO
                    || c.fade_out < Tick::ZERO
                    || c.fade_in + c.fade_out > c.duration
                {
                    return Err(format!("fades of clip {:?} longer than the clip", c.id));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod sequence_tests {
    use super::ImageSequence;

    #[test]
    fn numbers_and_paths() {
        assert_eq!(
            ImageSequence::number_of(r"C:\renders\shot_0042.png"),
            Some((16, 20, 42))
        );
        assert_eq!(
            ImageSequence::path(r"C:\renders\shot_0042.png", 9).as_deref(),
            Some(r"C:\renders\shot_0051.png")
        );
        // Past the padding: more digits.
        assert_eq!(
            ImageSequence::path("a/9.tga", 1).as_deref(),
            Some("a/10.tga")
        );
        // Digits in the folder do not count.
        assert_eq!(ImageSequence::number_of("take2/frame.png"), None);
    }
}

#[cfg(test)]
mod media_marker_tests {
    use super::*;

    #[test]
    fn media_markers_show_where_the_event_shows_their_media() {
        let mut p = Project::new(ProjectSettings::default());
        let v = p.tracks[0].id;
        let mut info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        info.duration = Tick::from_seconds(4);
        let m = p.add_media(info);
        let id = p
            .insert_media_range(
                m,
                Tick::from_seconds(10),
                Tick::from_seconds(1),
                Tick::from_seconds(10),
                Some(v),
                None,
            )
            .unwrap()[0];
        let s = Tick::from_seconds;
        // Extended past the media's end: it loops.
        p.clip_mut(id).unwrap().duration = s(10);
        let c = p.clip(id).unwrap().clone();
        // The media from 1 s, looping every 4 s: its 2 s at 11 s, 15 s, 19 s.
        assert_eq!(c.media_points(s(2), s(4)), vec![s(11), s(15), s(19)]);
        // Before the event's media start: only its repeats.
        assert_eq!(c.media_points(Tick::ZERO, s(4)), vec![s(13), s(17)]);
        let mut once = c.clone();
        once.looped = false;
        assert_eq!(once.media_points(s(2), s(4)), vec![s(11)]);
        assert!(once.media_points(Tick::ZERO, s(4)).is_empty());
        // Twice as fast: half the distance.
        let mut fast = once.clone();
        fast.rate = 2.0;
        assert_eq!(fast.media_points(s(3), s(4)), vec![s(11)]);

        // The markers file beside a media keeps them as they are.
        let marks = vec![MediaMarker {
            time: s(1),
            length: s(2),
            name: "Take".into(),
        }];
        assert_eq!(markers_from_json(&markers_to_json(&marks)), Some(marks));
    }
}

#[cfg(test)]
mod format_tests {
    use super::*;
    use crate::FadeCurve;

    #[test]
    fn pan_types_place_the_sound_as_vegas_tells() {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        // Centered: balance at its center's level, constant power 3 dB down.
        let m = PanMode::Balance0.matrix(0.0);
        assert_eq!(m, [[1.0, 0.0], [0.0, 1.0]]);
        let m = PanMode::Balance6.matrix(0.0);
        assert!(near(m[0][0], 0.501_187) && near(m[1][1], 0.501_187));
        let m = PanMode::ConstantPower.matrix(0.0);
        assert!(near(m[0][0], std::f32::consts::FRAC_1_SQRT_2));
        // Fully right: balance's left is silent, its right at 0 dB; Add
        // Channels sends both channels right.
        let m = PanMode::Balance3.matrix(1.0);
        assert!(near(m[0][0], 0.0) && near(m[1][1], 1.0));
        assert_eq!(PanMode::AddChannels.matrix(1.0), [[0.0, 0.0], [1.0, 1.0]]);
        assert_eq!(PanMode::AddChannels.matrix(-0.5), [[1.0, 0.5], [0.0, 0.5]]);
        let m = PanMode::Film.matrix(-1.0);
        assert!(near(m[0][0], 1.0) && near(m[1][1], 0.0));
    }

    /// Events may run past their media's end, and with Loop start before
    /// its start (`trim_end`, `trim_start`): such a project opens again.
    #[test]
    fn events_past_their_media_open_again() {
        let mut p = Project::new(ProjectSettings::default());
        let v = p.tracks[0].id;
        let mut info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        info.source = MediaSource::File {
            path: "clip.mov".into(),
        };
        info.duration = Tick::from_seconds(4);
        let m = p.add_media(info);
        let id = p
            .insert_media(m, Tick::from_seconds(2), Some(v), None)
            .unwrap()[0];
        p.trim_end(id, Tick::from_seconds(10)).unwrap();
        p.trim_start(id, Tick::from_seconds(1)).unwrap();
        let c = p.clip(id).unwrap();
        assert!(c.source_in < Tick::ZERO && c.source_end() > Tick::from_seconds(4));
        p.check_invariants().unwrap();
        let reopened = Project::from_json(&p.to_json()).unwrap();
        reopened.check_invariants().unwrap();
        // Without Loop, never before the media's start.
        let mut q = reopened.clone();
        q.clip_mut(id).unwrap().looped = false;
        assert!(q.check_invariants().is_err());
    }

    /// A project saved before format 2 named its fade-outs by their fade-in
    /// turned around: opened, they swap Fast and Slow and keep their shape.
    #[test]
    fn older_projects_keep_their_fade_outs_shape() {
        let mut p = Project::new(ProjectSettings::default());
        let v = p.tracks[0].id;
        let m = p.add_media(MediaInfo::generated(Generator::ColorBars, &p.settings));
        let id = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        p.clip_mut(id).unwrap().fade_out_curve = FadeCurve::Fast;
        let reopened = Project::from_json(&p.to_json()).unwrap();
        assert_eq!(reopened.clip(id).unwrap().fade_out_curve, FadeCurve::Fast);
        p.format_version = 1;
        let older = Project::from_json(&p.to_json()).unwrap();
        assert_eq!(older.clip(id).unwrap().fade_out_curve, FadeCurve::Slow);
        assert_eq!(older.format_version, Project::FORMAT_VERSION);
        // The same shape: the gain where a third of it is still to go.
        let then = FadeCurve::Fast.eval(1.0 / 3.0);
        assert!((FadeCurve::Slow.eval_out(1.0 / 3.0) - then).abs() < 1e-6);
    }
}

#[cfg(test)]
mod reading_tests {
    use super::*;

    /// Turning a stream: a quarter turn swaps its size and its pixels'
    /// shape, a half turn keeps them; turned back, it is as it was.
    #[test]
    fn a_stream_turned_swaps_its_size() {
        let v = VideoStreamInfo {
            width: 1440,
            height: 1080,
            frame_rate: bv_core::FrameRate::FPS_29_97,
            pixel_aspect: PixelAspect::new(4, 3),
            field_order: Default::default(),
            spherical: false,
            full_range: false,
            reading: VideoReading::default(),
        };
        let q = v.turned_to(MediaRotation::Clockwise);
        assert_eq!((q.width, q.height), (1080, 1440));
        assert_eq!(q.pixel_aspect, PixelAspect::new(3, 4));
        assert_eq!(q.coded_size(), (1440, 1080));
        let h = q.turned_to(MediaRotation::Inverted);
        assert_eq!((h.width, h.height), (1440, 1080));
        assert_eq!(h.coded_size(), (1440, 1080));
        assert_eq!(h.turned_to(MediaRotation::Original), v);
    }
}
