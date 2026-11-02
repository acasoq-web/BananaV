//! User settings kept between sessions in `%APPDATA%\BananaV\interface.json`:
//! recent projects, saved window layouts, the interface language, toolbars,
//! saved video templates and the settings new projects start with.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::egui;
use crate::i18n::{tr, trc};

/// How many recent projects the welcome screen remembers.
const MAX_RECENT: usize = 12;

/// A saved window layout (View › Window Layouts).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedLayout {
    pub name: String,
    /// The serialized dock state.
    pub dock: serde_json::Value,
    #[serde(default)]
    pub timeline_top: bool,
    /// Its file (`crate::layouts`), when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
}

/// The main window as BananaV was left: its windows' layout and its bars.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// The serialized dock state (read by `crate::layouts::dock_from_value`).
    pub dock: serde_json::Value,
    #[serde(default)]
    pub timeline_top: bool,
    /// The menu's layout in use (View › Window Layouts), if one is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_slot: Option<usize>,
    /// View › Toolbar and View › Status Bar turned off.
    #[serde(default)]
    pub hide_toolbar: bool,
    #[serde(default)]
    pub hide_status_bar: bool,
    /// The View menu's switches of the timeline.
    #[serde(default)]
    pub timeline: crate::timeline::TimelineSwitches,
}

/// A video template saved in File › Properties › Video (Save Template).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VideoTemplate {
    pub name: String,
    /// Only the Video tab's settings are used.
    pub settings: bv_timeline::ProjectSettings,
}

/// What new projects start with (File › Properties › "Start all new
/// projects with these settings").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewProjectDefaults {
    pub settings: bv_timeline::ProjectSettings,
    #[serde(default)]
    pub prerender_folder: String,
    #[serde(default)]
    pub recorded_folder: String,
    #[serde(default)]
    pub ruler_offset: bv_core::Tick,
    #[serde(default)]
    pub summary: bv_timeline::Summary,
}

/// What video event thumbnails show (Options › Preferences › Video).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Thumbnails {
    None,
    Head,
    HeadTail,
    HeadCenterTail,
    #[default]
    All,
}

impl Thumbnails {
    pub const ALL: [Thumbnails; 5] = [
        Thumbnails::None,
        Thumbnails::Head,
        Thumbnails::HeadTail,
        Thumbnails::HeadCenterTail,
        Thumbnails::All,
    ];
}

/// What the box on event thumbnails shows: the source's frame number,
/// time or timecode (Options › Preferences › Video).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameNumbers {
    #[default]
    None,
    Frames,
    Time,
    Timecode,
}

impl FrameNumbers {
    pub const ALL: [FrameNumbers; 4] = [
        FrameNumbers::None,
        FrameNumbers::Frames,
        FrameNumbers::Time,
        FrameNumbers::Timecode,
    ];
}

/// The Video Preview window's background (Options › Preferences › Video).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewBackground {
    #[default]
    Default,
    White,
    Black,
}

impl PreviewBackground {
    pub const ALL: [PreviewBackground; 3] = [
        PreviewBackground::Default,
        PreviewBackground::White,
        PreviewBackground::Black,
    ];
}

/// Video: "Automatically create video proxies for greater than 4K
/// resolution media".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoProxies {
    On,
    Off,
    /// For media larger than 4K when the computer is not fast enough to
    /// play it.
    #[default]
    Auto,
}

impl AutoProxies {
    pub const ALL: [AutoProxies; 3] = [AutoProxies::On, AutoProxies::Off, AutoProxies::Auto];
}

/// Audio: "Waveform display while recording".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordWaveforms {
    None,
    /// Only while a single track records.
    Single,
    #[default]
    All,
}

impl RecordWaveforms {
    pub const ALL: [RecordWaveforms; 3] = [
        RecordWaveforms::None,
        RecordWaveforms::Single,
        RecordWaveforms::All,
    ];
}

/// Audio Device: "Audio device type".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioDeviceType {
    /// The system's default output and input, followed when they change.
    #[default]
    SystemDefault,
    /// The devices chosen below.
    Windows,
}

/// Editing: "JKL / shuttle speed".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum JklSpeed {
    Slow,
    #[default]
    Medium,
    Fast,
}

impl JklSpeed {
    pub const ALL: [JklSpeed; 3] = [JklSpeed::Slow, JklSpeed::Medium, JklSpeed::Fast];

    /// The speeds L plays at: the first press, then doubling up to the
    /// last.
    pub fn rates(self) -> (f64, f64) {
        match self {
            JklSpeed::Slow => (0.5, 2.0),
            JklSpeed::Medium => (1.0, 4.0),
            JklSpeed::Fast => (2.0, 4.0),
        }
    }

    /// The speed L plays at: the first one when stopped (`playing` is
    /// `None`), else twice the one playing, up to the last.
    pub fn next_rate(self, playing: Option<f64>) -> f64 {
        let (first, last) = self.rates();
        playing.map_or(first, |r| (r * 2.0).clamp(first, last))
    }
}

/// Editing: where "Cut-to-overlap conversion" puts the overlap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CutAlign {
    /// The overlap ends at the cut: the event after it grows back.
    BeforeCut,
    #[default]
    Centered,
    /// The overlap starts at the cut: the event before it grows on.
    AfterCut,
}

impl CutAlign {
    pub const ALL: [CutAlign; 3] = [CutAlign::BeforeCut, CutAlign::Centered, CutAlign::AfterCut];

    /// How much of an overlap `amount` long lies before the cut and after
    /// it.
    pub fn split(self, amount: bv_core::Tick) -> (bv_core::Tick, bv_core::Tick) {
        let half = bv_core::Tick(amount.0 / 2);
        match self {
            CutAlign::BeforeCut => (amount, bv_core::Tick::ZERO),
            CutAlign::Centered => (amount - half, half),
            CutAlign::AfterCut => (bv_core::Tick::ZERO, amount),
        }
    }
}

/// Editing: where "Time selection envelope fades" go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FadeAlign {
    /// Before the selection's start and after its end.
    Outside,
    #[default]
    Centered,
    /// Within the selection.
    Inside,
}

impl FadeAlign {
    pub const ALL: [FadeAlign; 3] = [FadeAlign::Outside, FadeAlign::Centered, FadeAlign::Inside];

    /// For a selection `sel` and fades `fade` long: where the envelope
    /// keeps its value (outer) and the part that moves (inner).
    pub fn ranges(
        self,
        sel: bv_core::TimeRange,
        fade: bv_core::Tick,
    ) -> (bv_core::TimeRange, bv_core::TimeRange) {
        use bv_core::{Tick, TimeRange};
        let half = Tick(fade.0 / 2);
        let (a, b) = (sel.start, sel.end());
        let (outer, inner) = match self {
            FadeAlign::Outside => ((a - fade, b + fade), (a, b)),
            FadeAlign::Centered => ((a - half, b + half), (a + half, b - half)),
            FadeAlign::Inside => ((a, b), (a + fade, b - fade)),
        };
        // Fades longer than the selection meet in its middle.
        let mid = Tick(a.0 + (b.0 - a.0) / 2);
        let inner = if inner.0 <= inner.1 {
            inner
        } else {
            (mid, mid)
        };
        (
            TimeRange::from_bounds(outer.0.max(Tick::ZERO), outer.1.max(inner.1)),
            TimeRange::from_bounds(inner.0.max(Tick::ZERO), inner.1.max(Tick::ZERO)),
        )
    }
}

/// Sync: the frame rate of MIDI timecode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MtcRate {
    Film24,
    Ebu25,
    Drop2997,
    NonDrop2997,
    #[default]
    NonDrop30,
}

impl MtcRate {
    pub const ALL: [MtcRate; 5] = [
        MtcRate::Film24,
        MtcRate::Ebu25,
        MtcRate::Drop2997,
        MtcRate::NonDrop2997,
        MtcRate::NonDrop30,
    ];

    /// Frames per second.
    pub fn fps(self) -> f64 {
        match self {
            MtcRate::Film24 => 24.0,
            MtcRate::Ebu25 => 25.0,
            MtcRate::Drop2997 | MtcRate::NonDrop2997 => 30_000.0 / 1001.0,
            MtcRate::NonDrop30 => 30.0,
        }
    }

    /// The frames a timecode second counts.
    pub fn base(self) -> u32 {
        match self {
            MtcRate::Film24 => 24,
            MtcRate::Ebu25 => 25,
            _ => 30,
        }
    }

    pub fn drop_frame(self) -> bool {
        self == MtcRate::Drop2997
    }

    /// The rate MIDI timecode says it runs at: 0 is 24, 1 is 25, 2 is 30
    /// drop-frame, 3 is 30 (29.97 non-drop sends 3 too).
    pub fn code(self) -> u8 {
        match self {
            MtcRate::Film24 => 0,
            MtcRate::Ebu25 => 1,
            MtcRate::Drop2997 => 2,
            MtcRate::NonDrop2997 | MtcRate::NonDrop30 => 3,
        }
    }
}

/// Sync › Advanced: when full-frame timecode (or song position) messages
/// are sent, so that devices find the place before they follow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocateMessages {
    Never,
    #[default]
    PlaybackStart,
    /// And whenever the cursor moves while stopped.
    PlaybackStartAndCursor,
}

impl LocateMessages {
    pub const ALL: [LocateMessages; 3] = [
        LocateMessages::Never,
        LocateMessages::PlaybackStart,
        LocateMessages::PlaybackStartAndCursor,
    ];
}

/// A control surface in External Control & Automation's Active control
/// devices, and the MIDI ports it talks through.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlDevice {
    pub kind: SurfaceKind,
    pub input: Option<String>,
    pub output: Option<String>,
    /// The track its first channel strip controls (0: the first).
    #[serde(default)]
    pub first_track: u32,
    /// A Generic Control's mapping.
    #[serde(default)]
    pub generic: crate::generic_surface::GenericMap,
    /// A Mackie Control Extender (Configure Mackie Control's Device
    /// Type): its eight strips only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub extender: bool,
    /// A Mackie Control's buttons as the user maps them, over the
    /// defaults.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mackie_map: Vec<crate::mackie_config::MackieMapping>,
}

/// The control surfaces BananaV talks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceKind {
    /// Mackie Control Universal (and the surfaces that speak its
    /// protocol).
    MackieControl,
    /// Any MIDI controller, doing what its mapping says
    /// (`generic_surface.rs`).
    Generic,
}

/// MIDI: what comes in on an input sent on to an output ("MIDI Thru").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiRoute {
    pub input: String,
    pub output: String,
}

/// What an edit snapped to, for the color of the line shown there
/// (Options › Preferences › Display › Snap Colors, in VEGAS's order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SnapKind {
    Cursor,
    EnvelopePoint,
    Loop,
    DragOrigin,
    Marker,
    Region,
    CdIndex,
    CdTrack,
    CommandMarker,
    Event,
    EventFade,
    Grid,
}

impl SnapKind {
    pub const ALL: [SnapKind; 12] = [
        SnapKind::Cursor,
        SnapKind::EnvelopePoint,
        SnapKind::Loop,
        SnapKind::DragOrigin,
        SnapKind::Marker,
        SnapKind::Region,
        SnapKind::CdIndex,
        SnapKind::CdTrack,
        SnapKind::CommandMarker,
        SnapKind::Event,
        SnapKind::EventFade,
        SnapKind::Grid,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SnapKind::Cursor => tr("Cursor"),
            SnapKind::EnvelopePoint => tr("Envelope Point"),
            SnapKind::Loop => trc("snap", "Loop"),
            SnapKind::DragOrigin => tr("Drag Origin"),
            SnapKind::Marker => tr("Marker"),
            SnapKind::Region => tr("Region"),
            SnapKind::CdIndex => tr("CD Index"),
            SnapKind::CdTrack => tr("CD Track"),
            SnapKind::CommandMarker => tr("Command Marker"),
            SnapKind::Event => tr("Event"),
            SnapKind::EventFade => tr("Event Fade"),
            SnapKind::Grid => tr("Grid"),
        }
    }

    /// Whether BananaV snaps to it (it has no CD layout or command markers
    /// yet).
    pub fn ready(self) -> bool {
        !matches!(
            self,
            SnapKind::CdIndex | SnapKind::CdTrack | SnapKind::CommandMarker
        )
    }

    fn index(self) -> usize {
        SnapKind::ALL.iter().position(|&k| k == self).unwrap_or(0)
    }

    /// BananaV's own colors.
    pub fn default_color(self) -> [u8; 4] {
        rgba(match self {
            SnapKind::Cursor => 0xFFE15C,
            SnapKind::EnvelopePoint => 0x4D9BFF,
            SnapKind::Loop => 0x6C8ED8,
            SnapKind::DragOrigin => 0xC0C4CC,
            SnapKind::Marker => 0xF09A3E,
            SnapKind::Region => 0x5DC07A,
            SnapKind::CdIndex => 0xC080FF,
            SnapKind::CdTrack => 0xA060E0,
            SnapKind::CommandMarker => 0x40C8C8,
            SnapKind::Event => 0x5CC8FF,
            SnapKind::EventFade => 0xFF8A5C,
            SnapKind::Grid => 0x9AA0A8,
        })
    }
}

/// The envelopes of Options › Preferences › Display › Envelope Colors, in
/// VEGAS's order (the numbered ones count from 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeColor {
    Volume,
    Pan,
    AssignableFxVol(u8),
    AuxVol(u8),
    FxParameter(u8),
    CompositeLevel,
    FadeToColor,
    EventVelocity,
    MotionBlurLength,
    VideoSupersampling,
    TransitionProgress,
    Mute,
}

impl EnvelopeColor {
    pub fn all() -> Vec<EnvelopeColor> {
        use EnvelopeColor::*;
        let mut v = vec![Volume, Pan];
        v.extend((1..=4).map(AssignableFxVol));
        v.extend((1..=4).map(AuxVol));
        v.extend((1..=10).map(FxParameter));
        v.extend([
            CompositeLevel,
            FadeToColor,
            EventVelocity,
            MotionBlurLength,
            VideoSupersampling,
            TransitionProgress,
            Mute,
        ]);
        v
    }

    /// The color a track envelope of BananaV takes.
    pub fn of(kind: bv_timeline::EnvelopeKind) -> EnvelopeColor {
        use bv_timeline::EnvelopeKind;
        match kind {
            EnvelopeKind::Volume => EnvelopeColor::Volume,
            // VEGAS's four Aux and Assignable FX colors, in turn.
            EnvelopeKind::Send(bv_timeline::SendTo::Bus(b)) => {
                EnvelopeColor::AuxVol((b.0.saturating_sub(1) % 4 + 1) as u8)
            }
            EnvelopeKind::Send(bv_timeline::SendTo::Fx(c)) => {
                EnvelopeColor::AssignableFxVol((c.0.saturating_sub(1) % 4 + 1) as u8)
            }
            EnvelopeKind::Pan => EnvelopeColor::Pan,
            EnvelopeKind::Mute => EnvelopeColor::Mute,
            EnvelopeKind::MotionBlur => EnvelopeColor::MotionBlurLength,
            EnvelopeKind::Supersampling => EnvelopeColor::VideoSupersampling,
            EnvelopeKind::CompositeLevel => EnvelopeColor::CompositeLevel,
            EnvelopeKind::FadeToColor => EnvelopeColor::FadeToColor,
            EnvelopeKind::Velocity => EnvelopeColor::EventVelocity,
            EnvelopeKind::TransitionProgress => EnvelopeColor::TransitionProgress,
            // VEGAS's ten FX Parameter colors, by the parameter.
            EnvelopeKind::FxParam { param, .. } => {
                EnvelopeColor::FxParameter((param % 10) as u8 + 1)
            }
            EnvelopeKind::FxBypass(_) => EnvelopeColor::Mute,
        }
    }

    pub fn name(self) -> String {
        use EnvelopeColor::*;
        match self {
            Volume => tr("Volume").into(),
            Pan => tr("Pan").into(),
            AssignableFxVol(n) => trf!("Assignable FX Vol {}", n),
            AuxVol(n) => trf!("Aux Vol {}", n),
            FxParameter(n) => trf!("FX Parameter {}", n),
            CompositeLevel => tr("Composite Level").into(),
            FadeToColor => tr("Fade to Color").into(),
            EventVelocity => tr("Event Velocity").into(),
            MotionBlurLength => tr("Motion Blur Length").into(),
            VideoSupersampling => tr("Video Supersampling").into(),
            TransitionProgress => tr("Transition Progress").into(),
            Mute => tr("Mute").into(),
        }
    }

    /// Whether BananaV has these envelopes (bus sends, effect parameters,
    /// event velocity, motion blur, supersampling and transition progress
    /// are to come).
    pub fn ready(self) -> bool {
        // Every one colors an envelope BananaV has now: sends, effect
        // parameters, the video bus's.
        true
    }

    fn index(self) -> usize {
        EnvelopeColor::all()
            .iter()
            .position(|&e| e == self)
            .unwrap_or(0)
    }

    /// BananaV's own colors.
    pub fn default_color(self) -> [u8; 4] {
        use EnvelopeColor::*;
        const FX_VOL: [u32; 4] = [0x6FD08C, 0x5BBF7A, 0x48AE68, 0x359D56];
        const AUX: [u32; 4] = [0xFFB347, 0xF2A03A, 0xE58D2D, 0xD87A20];
        const PARAMS: [u32; 10] = [
            0xFF5C8A, 0xFF7AB6, 0xE07CFF, 0xB08CFF, 0x8CA0FF, 0x6FC3FF, 0x5CE1E6, 0x7CF2B0,
            0xC8F27C, 0xF2E27C,
        ];
        let nth = |n: u8, len: usize| (n.max(1) as usize - 1) % len;
        rgba(match self {
            // VEGAS's, measured on the user's prints.
            Volume | CompositeLevel => 0x493FAB,
            Pan => 0xFF6B5B,
            AssignableFxVol(n) => FX_VOL[nth(n, 4)],
            AuxVol(n) => AUX[nth(n, 4)],
            FxParameter(n) => PARAMS[nth(n, 10)],
            FadeToColor => 0xE8E8E8,
            EventVelocity => 0x66D17A,
            MotionBlurLength => 0xF2D45C,
            VideoSupersampling => 0x5CD6F2,
            TransitionProgress => 0xD67CF2,
            Mute => 0xB88CFF,
        })
    }
}

/// Color A … Color H of Options › Preferences › Display › Track Colors.
pub fn track_color_name(index: usize) -> &'static str {
    [
        tr("Color A"),
        tr("Color B"),
        tr("Color C"),
        tr("Color D"),
        tr("Color E"),
        tr("Color F"),
        tr("Color G"),
        tr("Color H"),
    ][index % 8]
}

/// The track colors, VEGAS's (measured on its screenshots): a new track
/// takes the one of its place, round again after the eighth; tracks
/// without one, video Color A and audio Color B.
pub const TRACK_COLORS: [u32; 8] = [
    0x3C8CDC, 0x64C8DC, 0x50C878, 0xDCB43C, 0xDC8C3C, 0xDC6450, 0xC878DC, 0x783CF0,
];

/// The palette BananaV had before it took VEGAS's: settings saved with it
/// unchanged get the new one ([`UiSettings::load`]).
const OLD_TRACK_COLORS: [u32; 8] = [
    0x3D82D6, 0x5EC4D8, 0x5DBE6E, 0xC9B23A, 0xE0883C, 0xD9534F, 0xC25AB9, 0x8C6FE0,
];

/// The volume and composite level envelopes' blue before they took
/// VEGAS's purple: left so, they get the new one ([`Prefs::follow_new_defaults`]).
const OLD_ENVELOPE_BLUE: u32 = 0x4D9BFF;

/// An opaque color as RGBA bytes.
const fn rgba(c: u32) -> [u8; 4] {
    [(c >> 16) as u8, (c >> 8) as u8, c as u8, 0xFF]
}

fn color32([r, g, b, a]: [u8; 4]) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// The usual VST search folders: the system's shared VST3 folder on
/// Windows; the user's and the system's VST3 and VST folders on Linux.
pub fn default_vst_folders() -> Vec<PathBuf> {
    if !cfg!(windows) {
        let home = bv_platform::home_dir().unwrap_or_default();
        return vec![
            home.join(".vst3"),
            PathBuf::from("/usr/lib/vst3"),
            PathBuf::from("/usr/local/lib/vst3"),
            home.join(".vst"),
            PathBuf::from("/usr/lib/vst"),
            PathBuf::from("/usr/local/lib/vst"),
        ];
    }
    let common = std::env::var_os("CommonProgramFiles")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Program Files\\Common Files"));
    vec![common.join("VST3")]
}

/// Options › Preferences settings, with VEGAS's defaults (BananaV's own
/// colors).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// General: "Automatically open last project on startup" (the project
    /// open when BananaV was closed).
    pub open_last_project: bool,
    /// General: "Confirm media file deletion when still in use".
    pub confirm_media_removal: bool,
    /// General: "Save active prerenders on project close".
    pub keep_prerenders: bool,
    /// General: "Close media files when not the active application".
    pub release_media_inactive: bool,
    /// General: "Close audio and MIDI ports when not the active
    /// application".
    pub release_audio_inactive: bool,
    /// General: "Prompt to adjust project settings to match first media
    /// added to timeline".
    pub match_first_media: bool,
    /// General: "Check project file type associations at startup".
    pub check_associations: bool,
    /// General: "Enable joystick support" (game controllers move panning
    /// and color wheels).
    pub joystick: bool,
    /// General: "Allow pulldown removal when opening 24p DV".
    pub dv_pulldown_removal: bool,
    /// General: "AAF Export - Use frame unit for audio".
    pub aaf_frame_units: bool,
    /// General: "AAF Export - Use clip-based audio envelope".
    pub aaf_clip_envelopes: bool,
    /// General: "Import stereo as dual mono".
    pub stereo_as_dual_mono: bool,
    /// File I/O: "Enable auto-detection of 360° media": importing a 360°
    /// media turns the project's 360 Output on.
    pub detect_360: bool,
    /// File I/O: "Enable no-recompress rendering": frames left as their
    /// media had them pass into rendered files as they are (smart
    /// rendering).
    pub no_recompress: bool,
    /// Display: "Reverse Event Background color": events take their
    /// track's color, their waveforms the darker one.
    pub reverse_event_colors: bool,
    /// Display: "Display timeline at bottom of main window" (else above
    /// the docking area).
    pub timeline_at_bottom: bool,
    /// File I/O: the logs of plug-ins found, media read and renders
    /// (`logs.rs`).
    pub log_plugins: crate::logs::LogLevel,
    pub log_reads: crate::logs::LogLevel,
    pub log_renders: crate::logs::LogLevel,
    /// General: "Use GPU processing for ACES color management".
    pub aces_on_gpu: bool,
    /// General: "Prompt to keep files after recording".
    pub ask_keep_recordings: bool,
    /// General: "Create undos for FX parameter changes".
    pub fx_undo: bool,
    /// General: "Keep bypassed FX running (to avoid pause on
    /// bypass/enable)".
    pub keep_bypassed_fx: bool,
    /// General: "Automatically name regions and markers if not playing".
    pub name_markers: bool,
    /// General: "Use linear scrub range".
    pub linear_scrub: bool,
    /// General: "Always draw marker lines" (otherwise only with snapping
    /// on).
    pub marker_lines: bool,
    /// General: "Double click on media file loads into Trimmer instead of
    /// tracks".
    pub media_to_trimmer: bool,
    /// General: "Show trimmer history with filename first, then folder".
    pub trimmer_name_first: bool,
    /// General: "Automatically save trimmer markers and regions with media
    /// file".
    pub save_trimmer_markers: bool,
    /// General: "Provide a warning if a track being deleted holds at least
    /// one event".
    pub warn_track_delete: bool,
    /// General: "Warn if creating a nested timeline may adversely affect
    /// the project".
    pub warn_nesting: bool,
    /// General: "Warn to save modified nested timeline prior to opening
    /// parent timeline".
    pub warn_nested_save: bool,
    /// General: "Enable autosave".
    pub autosave: bool,
    /// General: "Double Click event edge to toggle Expanded Edit Mode".
    pub edge_expanded_edit: bool,
    /// General: "Enable Automatic FX Window Resizing".
    pub fx_window_resize: bool,
    /// General: "Do not create .bvpk files to store peak data".
    pub no_peak_files: bool,
    /// General: "Hide new .bvpk files".
    pub hide_peak_files: bool,
    /// General: "Temporary files folder" (`None`: BananaV's in the system's
    /// temporary folder).
    pub temp_folder: Option<PathBuf>,
    /// General: "Allow edit cursor to be dragged" (the needles' lines in
    /// the timeline).
    pub cursor_drag: bool,
    /// General: "Allow Ctrl+drag cursor style scrub over events".
    pub ctrl_scrub_over_events: bool,
    /// General: "Make spacebar and F12 Play/Stop instead of Play/Pause".
    pub space_play_stop: bool,
    /// General: "Recently used project list" and how many.
    pub recent_list: bool,
    pub recent_count: u32,
    /// Video: "Dynamic RAM Preview max", in MB (frames kept for playback).
    pub ram_preview_mb: u32,
    /// Video: "Maximum number of rendering threads" (the processor's
    /// compositor).
    pub render_threads: u32,
    /// Video: "Show source frame numbers on event thumbnails as".
    pub frame_numbers: FrameNumbers,
    /// Video: "Thumbnails to show in video events".
    pub thumbnails: Thumbnails,
    /// Video: the safe areas' borders (percent of the frame) and the grid.
    pub action_safe: u32,
    pub title_safe: u32,
    pub grid_horizontal: u32,
    pub grid_vertical: u32,
    /// Video: "Automatically create video proxies for greater than 4K
    /// resolution media".
    pub auto_proxies: AutoProxies,
    /// Video: "Display at project size" (never enlarged past 100%).
    pub display_at_project_size: bool,
    /// Video: "Simulate device aspect ratio" (non-square pixels shown as
    /// a device shows them).
    pub simulate_device_aspect: bool,
    /// Video: "Background color" of the Video Preview window.
    pub preview_background: PreviewBackground,
    /// Video: multicamera editing's "Display take names" and "Display take
    /// numbers".
    pub take_names: bool,
    pub take_numbers: bool,
    /// Video: multicamera editing's "Active take indicator" color (RGBA).
    pub take_indicator: [u8; 4],
    /// Video: "Default track fade colors", the fade to color envelopes'
    /// top and bottom (RGBA).
    pub fade_top: [u8; 4],
    pub fade_bottom: [u8; 4],

    /// Preview Device: the external monitor's display, by the OS's name of
    /// it (`None`: the first one that is not the main display).
    pub preview_display: Option<String>,
    /// Preview Device (the graphics card): "Scale output to fit display",
    /// "Apply deinterlace filter", "Adjust levels from studio RGB to computer
    /// RGB", "Optimize GPU display performance" and "Sync Offset (frames)".
    pub preview_scale_to_fit: bool,
    pub preview_deinterlace: bool,
    pub preview_studio_rgb: bool,
    pub preview_optimize_gpu: bool,
    pub preview_sync_offset: i32,
    /// Preview Device: "Recompress edited frames", "Display frames in video
    /// preview window during playback", "Use project output rotation
    /// setting".
    pub preview_recompress: bool,
    pub preview_in_window: bool,
    pub preview_rotation: bool,

    /// Audio: "Waveform display while recording".
    pub record_waveforms: RecordWaveforms,
    /// Audio: "Normalize peak level (dB)", the level Normalize brings events
    /// to.
    pub normalize_peak_db: f32,
    /// Audio: "Record to Broadcast Wave Format".
    pub record_bwf: bool,
    /// Audio: "Track prefader sends listen to mute".
    pub prefader_sends_listen_to_mute: bool,
    /// Audio: "Prompt user when no track has been armed for record".
    pub ask_no_armed_track: bool,
    /// Audio: "Preferred audio editor" (`None`: the system's program for
    /// the file).
    pub audio_editor: Option<PathBuf>,
    /// Audio: the metronome's sounds, BananaV's own or these files (a beat,
    /// the accented first beat of a measure).
    pub metronome_custom: bool,
    pub metronome_normal: Option<PathBuf>,
    pub metronome_accent: Option<PathBuf>,
    /// Audio: "Open editor when new loudness log is generated".
    pub open_loudness_log: bool,

    /// Audio Device: "Audio device type" and "Default stereo and front
    /// playback device" (`None`: the system's default).
    pub audio_device_type: AudioDeviceType,
    pub playback_device: Option<String>,
    /// Audio Device: "Playback buffering (seconds)".
    pub playback_buffer: f32,
    /// Audio Device: "Enable track buffering", "Track buffering (seconds)".
    pub track_buffering: bool,
    pub track_buffer: f32,
    /// Audio Device: "Default audio recording device": the input tracks
    /// record from when they have none of their own.
    pub record_device: bv_timeline::RecordInput,
    /// Audio Device: "Automatically detect and offset for hardware
    /// recording latency", and the offset used otherwise (ms).
    pub record_latency_auto: bool,
    pub record_latency_ms: f32,

    /// MIDI: the output and input ports made available (by name), and the
    /// inputs sent on to outputs.
    pub midi_outputs: Vec<String>,
    pub midi_inputs: Vec<String>,
    pub midi_thru: Vec<MidiRoute>,

    /// VST Effects: the folders searched for VST plug-ins, the plug-ins
    /// left out (by file), "Generic VST editor".
    pub vst_folders: Vec<PathBuf>,
    pub vst_left_out: Vec<PathBuf>,
    pub vst_generic_editor: bool,

    /// Editing: "Enable looping on events by default".
    pub loop_events: bool,
    /// Editing: "Preserve pitch when stretching audio events" (new events).
    pub preserve_pitch: bool,
    /// Editing: "Cut, copy, and delete grouped events".
    pub group_edits: bool,
    /// Editing: "Do not quantize to frames for audio-only edits".
    pub no_quantize_audio: bool,
    /// Editing: "Collapse loop region when no time selection is present".
    pub collapse_loop_region: bool,
    /// Editing: "JKL / shuttle speed".
    pub jkl_speed: JklSpeed,
    /// Editing: "Quick fade length for audio events (ms)".
    pub quick_fade_ms: u32,
    /// Editing: "New still image length (seconds)".
    pub still_length: f64,
    /// Editing: "Default time between CD tracks" (seconds).
    pub cd_track_gap: f64,
    /// Editing: "Cursor preview duration (seconds)".
    pub cursor_preview: f64,
    /// Editing: "Automatically overlap multiple selected media when added".
    pub overlap_added_media: bool,
    /// Editing: "Cut-to-overlap conversion (seconds)" and its alignment.
    pub cut_overlap: f64,
    pub cut_overlap_align: CutAlign,
    /// Editing: "Time selection envelope fades (ms)" and their alignment.
    pub envelope_fades: bool,
    pub envelope_fade_ms: u32,
    pub envelope_fade_align: FadeAlign,
    /// Editing: "Automatically crop video files added to timeline", "…still
    /// images…".
    pub crop_video: bool,
    pub crop_stills: bool,
    /// Editing: "Smart split default length (frames)": the overlap Edit ›
    /// Smart Split joins the parts with.
    pub smart_split_frames: u32,
    /// Editing: "Change cursor and selected event color when in Auto Ripple
    /// mode".
    pub ripple_colors: bool,
    /// Editing: "Use right mouse button to switch to the Selection Edit
    /// Tool" (a right-drag draws a selection rectangle).
    pub right_button_selection: bool,
    /// Editing: "Move timeline cursor when clicking on an event".
    pub click_moves_cursor: bool,

    /// External Control & Automation: "Smooth and thin automation data
    /// after recording or drawing", "Set controls to default values when
    /// automation is turned off".
    pub thin_automation: bool,
    pub automation_off_defaults: bool,
    /// External Control & Automation: the control surfaces added; Options
    /// › External Control: they are in use.
    pub control_devices: Vec<ControlDevice>,
    pub external_control: bool,
    /// External Control & Automation: the fade types of new envelopes,
    /// events' fades and keyframes, and the smoothness of new keyframes.
    pub new_audio_envelopes: bv_timeline::KeyCurve,
    pub new_audio_event_gain: bv_timeline::FadeCurve,
    pub new_audio_fx_envelopes: bv_timeline::KeyCurve,
    pub new_video_envelopes: bv_timeline::KeyCurve,
    pub new_video_event_opacity: bv_timeline::FadeCurve,
    pub new_video_fx_keyframes: bv_timeline::KeyCurve,
    pub new_motion_keyframes: bv_timeline::KeyCurve,
    pub track_motion_smoothness: f32,
    pub pan_crop_smoothness: f32,

    /// Sync: the MIDI output that MIDI timecode goes to, and its rate; the
    /// one MIDI clock goes to; the MIDI input timecode comes in on, and
    /// its rate.
    pub mtc_output: Option<String>,
    pub mtc_output_rate: MtcRate,
    pub clock_output: Option<String>,
    pub mtc_input: Option<String>,
    pub mtc_input_rate: MtcRate,
    /// Sync › Advanced: full-frame timecode messages sent; MIDI clock's
    /// Start instead of Continue and song position pointers; following
    /// timecode: free-wheeling when it is lost (after how many frames, for
    /// how long), the time taken to lock, an offset in quarter frames.
    pub mtc_full_frames: LocateMessages,
    pub clock_send_start: bool,
    pub clock_song_position: LocateMessages,
    pub mtc_freewheel: bool,
    pub mtc_slack_frames: u32,
    pub mtc_freewheel_seconds: f32,
    pub mtc_sync_delay: f32,
    pub mtc_offset_quarters: i32,

    /// Display: the eight track colors, Color A to Color H; each track
    /// shows one of them.
    pub track_colors: [[u8; 4]; 8],
    /// Display: the envelopes' colors, in [`EnvelopeColor::all`]'s order.
    pub envelope_colors: Vec<[u8; 4]>,
    /// Display: the colors of the line shown where an edit snaps, by what
    /// it snapped to, in [`SnapKind::ALL`]'s order.
    pub snap_colors: Vec<[u8; 4]>,
    /// Display: "Icon color strength" (0–100 %).
    pub icon_strength: u8,
    /// Display: "Button Highlight Color" (active toolbar buttons).
    pub button_highlight: [u8; 4],
    /// Display: "Interface Type".
    pub interface: crate::theme::Interface,
    /// Display: "Automatically hide docking area", "Position tabs at top
    /// of docked windows", "Allow floating windows to dock (hold Ctrl to
    /// override)", "Use High DPI scaling (where available)", "Show system
    /// statistics in the status bar".
    pub auto_hide_dock: bool,
    pub tabs_at_top: bool,
    pub floating_dock: bool,
    pub high_dpi: bool,
    pub system_stats: bool,
}

/// The computer's memory in MB.
pub fn memory_mb() -> u32 {
    bv_platform::total_memory().map_or(8192, |b| (b as f64 / (1u64 << 20) as f64).round() as u32)
}

/// The Dynamic RAM Preview's usual size: 5% of the computer's memory.
pub fn default_ram_preview_mb() -> u32 {
    memory_mb() / 20
}

impl Prefs {
    /// Where BananaV keeps its temporary files.
    pub fn temp_dir(&self) -> PathBuf {
        self.temp_folder
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join("BananaV"))
    }
}

/// The processor's threads.
pub fn cores() -> u32 {
    std::thread::available_parallelism().map_or(1, |n| n.get() as u32)
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs {
            open_last_project: true,
            confirm_media_removal: true,
            keep_prerenders: true,
            release_media_inactive: true,
            release_audio_inactive: true,
            match_first_media: true,
            check_associations: true,
            joystick: false,
            dv_pulldown_removal: true,
            aaf_frame_units: true,
            aaf_clip_envelopes: false,
            stereo_as_dual_mono: false,
            detect_360: true,
            no_recompress: true,
            reverse_event_colors: false,
            timeline_at_bottom: true,
            log_plugins: Default::default(),
            log_reads: Default::default(),
            log_renders: Default::default(),
            aces_on_gpu: true,
            ask_keep_recordings: true,
            fx_undo: true,
            keep_bypassed_fx: false,
            name_markers: true,
            linear_scrub: true,
            marker_lines: true,
            media_to_trimmer: false,
            trimmer_name_first: true,
            save_trimmer_markers: false,
            warn_track_delete: false,
            warn_nesting: true,
            warn_nested_save: true,
            autosave: true,
            edge_expanded_edit: false,
            fx_window_resize: true,
            no_peak_files: false,
            hide_peak_files: false,
            temp_folder: None,
            cursor_drag: true,
            ctrl_scrub_over_events: false,
            space_play_stop: false,
            recent_list: true,
            recent_count: 4,
            ram_preview_mb: default_ram_preview_mb(),
            render_threads: cores(),
            frame_numbers: FrameNumbers::None,
            thumbnails: Thumbnails::All,
            action_safe: 10,
            title_safe: 20,
            grid_horizontal: 10,
            grid_vertical: 10,
            auto_proxies: AutoProxies::Auto,
            display_at_project_size: true,
            simulate_device_aspect: true,
            preview_background: PreviewBackground::Default,
            take_names: true,
            take_numbers: true,
            take_indicator: [64, 255, 255, 255],
            fade_top: [255, 255, 255, 255],
            fade_bottom: [0, 0, 0, 255],
            preview_display: None,
            preview_scale_to_fit: true,
            preview_deinterlace: false,
            preview_studio_rgb: false,
            preview_optimize_gpu: true,
            preview_sync_offset: 0,
            preview_recompress: true,
            preview_in_window: true,
            preview_rotation: true,
            record_waveforms: RecordWaveforms::All,
            normalize_peak_db: -0.1,
            record_bwf: true,
            prefader_sends_listen_to_mute: false,
            ask_no_armed_track: true,
            audio_editor: None,
            metronome_custom: false,
            metronome_normal: None,
            metronome_accent: None,
            open_loudness_log: true,
            audio_device_type: AudioDeviceType::SystemDefault,
            playback_device: None,
            playback_buffer: 0.10,
            track_buffering: true,
            track_buffer: 0.25,
            record_device: bv_timeline::RecordInput::default(),
            record_latency_auto: true,
            record_latency_ms: 0.0,
            midi_outputs: Vec::new(),
            midi_inputs: Vec::new(),
            midi_thru: Vec::new(),
            vst_folders: default_vst_folders(),
            vst_left_out: Vec::new(),
            vst_generic_editor: false,
            loop_events: true,
            preserve_pitch: true,
            group_edits: true,
            no_quantize_audio: true,
            collapse_loop_region: false,
            jkl_speed: JklSpeed::Medium,
            quick_fade_ms: 10,
            still_length: 5.0,
            cd_track_gap: 2.0,
            cursor_preview: 2.0,
            overlap_added_media: false,
            cut_overlap: 1.0,
            cut_overlap_align: CutAlign::Centered,
            envelope_fades: true,
            envelope_fade_ms: 50,
            envelope_fade_align: FadeAlign::Centered,
            crop_video: false,
            crop_stills: false,
            smart_split_frames: 5,
            ripple_colors: true,
            right_button_selection: false,
            click_moves_cursor: true,
            thin_automation: true,
            automation_off_defaults: true,
            control_devices: Vec::new(),
            external_control: false,
            new_audio_envelopes: bv_timeline::KeyCurve::Smooth,
            new_audio_event_gain: bv_timeline::FadeCurve::Fast,
            new_audio_fx_envelopes: bv_timeline::KeyCurve::Linear,
            new_video_envelopes: bv_timeline::KeyCurve::Smooth,
            new_video_event_opacity: bv_timeline::FadeCurve::Smooth,
            new_video_fx_keyframes: bv_timeline::KeyCurve::Linear,
            new_motion_keyframes: bv_timeline::KeyCurve::Linear,
            track_motion_smoothness: 0.0,
            pan_crop_smoothness: 0.0,
            mtc_output: None,
            mtc_output_rate: MtcRate::NonDrop30,
            clock_output: None,
            mtc_input: None,
            mtc_input_rate: MtcRate::NonDrop30,
            mtc_full_frames: LocateMessages::PlaybackStart,
            clock_send_start: false,
            clock_song_position: LocateMessages::PlaybackStart,
            mtc_freewheel: true,
            mtc_slack_frames: 6,
            mtc_freewheel_seconds: 2.0,
            mtc_sync_delay: 0.5,
            mtc_offset_quarters: 0,
            track_colors: TRACK_COLORS.map(rgba),
            envelope_colors: EnvelopeColor::all()
                .into_iter()
                .map(EnvelopeColor::default_color)
                .collect(),
            snap_colors: SnapKind::ALL.map(SnapKind::default_color).to_vec(),
            icon_strength: 100,
            button_highlight: crate::theme::DEFAULT_HIGHLIGHT,
            interface: crate::theme::Interface::Dark,
            auto_hide_dock: false,
            tabs_at_top: false,
            floating_dock: false,
            high_dpi: true,
            system_stats: false,
        }
    }
}

impl Prefs {
    /// How new events start (Editing: their Loop switch, their pitch when
    /// stretched, the length of still images).
    pub fn new_events(&self) -> bv_timeline::NewEvents {
        bv_timeline::NewEvents {
            looped: self.loop_events,
            preserve_pitch: self.preserve_pitch,
            still_length: bv_core::Tick::from_seconds_f64(self.still_length.max(0.001)),
            audio_fades: self.new_audio_event_gain,
            video_fades: self.new_video_event_opacity,
        }
    }

    /// The curve the points of a new `kind` envelope start with
    /// (External Control & Automation: "New audio envelopes", "New video
    /// envelopes").
    pub fn new_envelope_curve(&self, kind: bv_timeline::EnvelopeKind) -> bv_timeline::KeyCurve {
        use bv_timeline::EnvelopeKind;
        match kind {
            EnvelopeKind::Volume
            | EnvelopeKind::Pan
            | EnvelopeKind::Mute
            | EnvelopeKind::Send(_) => self.new_audio_envelopes,
            EnvelopeKind::CompositeLevel
            | EnvelopeKind::FadeToColor
            | EnvelopeKind::Velocity
            | EnvelopeKind::TransitionProgress
            | EnvelopeKind::MotionBlur
            | EnvelopeKind::Supersampling => self.new_video_envelopes,
            EnvelopeKind::FxParam { .. } | EnvelopeKind::FxBypass(_) => self.new_audio_fx_envelopes,
        }
    }

    /// New Pan/Crop keyframes' curve and smoothness (External Control &
    /// Automation: "New video motion keyframes", "Pan/Crop default
    /// smoothness").
    pub fn new_motion_keys(&self) -> (bv_timeline::KeyCurve, f32) {
        (self.new_motion_keyframes, self.pan_crop_smoothness)
    }

    /// New Track Motion keyframes' curve and smoothness (External Control &
    /// Automation: "New video motion keyframes", "Track Motion default
    /// smoothness").
    pub fn new_track_motion_keys(&self) -> (bv_timeline::KeyCurve, f32) {
        (self.new_motion_keyframes, self.track_motion_smoothness)
    }

    /// The overlap of a cut turned into one for a transition, before and
    /// after the cut (Editing: "Cut-to-overlap conversion").
    pub fn cut_overlap_split(&self) -> (bv_core::Tick, bv_core::Tick) {
        let amount = bv_core::Tick::from_seconds_f64(self.cut_overlap.max(0.0));
        self.cut_overlap_align.split(amount)
    }

    /// Color A … H (Display: Track Colors).
    pub fn track_color(&self, index: usize) -> egui::Color32 {
        color32(self.track_colors[index % 8])
    }

    /// A track's color (its own, or its kind's).
    pub fn color_of(&self, track: &bv_timeline::Track) -> egui::Color32 {
        self.track_color(track.color_index())
    }

    /// Display: Envelope Colors.
    /// Colors saved as an old default of BananaV's, never chosen, take
    /// the new one: the track colors before VEGAS's palette, the volume and
    /// composite level envelopes' blue before VEGAS's purple.
    pub fn follow_new_defaults(&mut self) {
        if self.track_colors == OLD_TRACK_COLORS.map(rgba) {
            self.track_colors = TRACK_COLORS.map(rgba);
        }
        for which in [EnvelopeColor::Volume, EnvelopeColor::CompositeLevel] {
            if let Some(c) = self.envelope_colors.get_mut(which.index())
                && *c == rgba(OLD_ENVELOPE_BLUE)
            {
                *c = which.default_color();
            }
        }
    }

    pub fn envelope_color(&self, which: EnvelopeColor) -> egui::Color32 {
        let rgba = self.envelope_colors.get(which.index()).copied();
        color32(rgba.unwrap_or_else(|| which.default_color()))
    }

    pub fn envelope_color_mut(&mut self, which: EnvelopeColor) -> &mut [u8; 4] {
        let all = EnvelopeColor::all();
        while self.envelope_colors.len() < all.len() {
            let next = all[self.envelope_colors.len()].default_color();
            self.envelope_colors.push(next);
        }
        &mut self.envelope_colors[which.index()]
    }

    /// Display: Snap Colors.
    pub fn snap_color(&self, kind: SnapKind) -> egui::Color32 {
        let rgba = self.snap_colors.get(kind.index()).copied();
        color32(rgba.unwrap_or_else(|| kind.default_color()))
    }

    pub fn snap_color_mut(&mut self, kind: SnapKind) -> &mut [u8; 4] {
        while self.snap_colors.len() < SnapKind::ALL.len() {
            let next = SnapKind::ALL[self.snap_colors.len()].default_color();
            self.snap_colors.push(next);
        }
        &mut self.snap_colors[kind.index()]
    }

    /// The Display page's settings put back to their defaults.
    pub fn reset_display(&mut self) {
        let d = Prefs::default();
        self.track_colors = d.track_colors;
        self.envelope_colors = d.envelope_colors;
        self.snap_colors = d.snap_colors;
        self.icon_strength = d.icon_strength;
        self.button_highlight = d.button_highlight;
        self.interface = d.interface;
        self.auto_hide_dock = d.auto_hide_dock;
        self.tabs_at_top = d.tabs_at_top;
        self.floating_dock = d.floating_dock;
        self.reverse_event_colors = d.reverse_event_colors;
        self.timeline_at_bottom = d.timeline_at_bottom;
        self.high_dpi = d.high_dpi;
        self.system_stats = d.system_stats;
    }
}

/// A generator preset saved in the Video Media Generators window: the
/// plug-in it is of (the Media Generator window's key), its name and the
/// generator.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeneratorPreset {
    pub plugin: String,
    pub name: String,
    pub generator: bv_timeline::Generator,
}

/// The Video Preview's settings kept between sessions.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PreviewPrefs {
    /// Preview Quality.
    #[serde(default)]
    pub quality: crate::tabs::preview::PreviewQuality,
    /// Split-Screen View as it was left, while "Retain Split Screen
    /// Setting" is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<crate::tabs::preview::SplitScreen>,
    /// The window's shortcut menu: "Enable Preview Scaling" off (the frame
    /// never shown larger than its pixels) and "Adjust Size and Quality for
    /// Optimal Playback".
    #[serde(default)]
    pub scaling_off: bool,
    #[serde(default)]
    pub optimal_playback: bool,
    /// The window's toolbar, status bar and transport bar hidden.
    #[serde(default)]
    pub hide_toolbar: bool,
    #[serde(default)]
    pub hide_status_bar: bool,
    #[serde(default)]
    pub hide_transport_bar: bool,
}

fn first_time() -> bool {
    true
}

/// The Loudness Meters' options (Options › Loudness Meters, and the
/// window's shortcut menu).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoudnessPrefs {
    /// ATSC A/85 (−24 LUFS, true peaks to −2 dBTP) instead of EBU R 128
    /// (−23 LUFS, −1 dBTP).
    #[serde(default)]
    pub atsc: bool,
    /// EBU +18 (−36 to +18 LU) instead of EBU +9 (−18 to +9 LU).
    #[serde(default)]
    pub wide: bool,
    /// Values as LUFS instead of LU against the target.
    #[serde(default)]
    pub absolute: bool,
    #[serde(default)]
    pub blocking: bool,
    #[serde(default = "yes")]
    pub true_peak: bool,
    #[serde(default = "yes")]
    pub labels: bool,
    #[serde(default = "yes")]
    pub hold_peaks: bool,
    #[serde(default)]
    pub hold_valleys: bool,
}

fn yes() -> bool {
    true
}

impl Default for LoudnessPrefs {
    fn default() -> LoudnessPrefs {
        LoudnessPrefs {
            atsc: false,
            wide: false,
            absolute: false,
            blocking: false,
            true_peak: true,
            labels: true,
            hold_peaks: true,
            hold_valleys: false,
        }
    }
}

impl LoudnessPrefs {
    /// The integrated loudness to aim at (LUFS) and the true peaks' limit
    /// (dBTP).
    pub fn targets(self) -> (f64, f64) {
        if self.atsc {
            (-24.0, -2.0)
        } else {
            (-23.0, -1.0)
        }
    }

    /// The meters' range, in LU around the target.
    pub fn range(self) -> (f64, f64) {
        if self.wide {
            (-36.0, 18.0)
        } else {
            (-18.0, 9.0)
        }
    }
}

/// The Time Display's colors chosen for one interface type (`None`: the
/// type's own).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeDisplayColors {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<[u8; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<[u8; 4]>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UiSettings {
    #[serde(default)]
    pub recent: Vec<PathBuf>,
    /// Slots 0–9 (Alt+D, 0…9). `None` is an empty slot.
    #[serde(default)]
    pub layouts: Vec<Option<SavedLayout>>,
    /// Where window layouts are saved (`None`: `Layouts` beside these
    /// settings).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_folder: Option<PathBuf>,
    /// The main window as BananaV was left, which it opens with again, as
    /// VEGAS does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<Session>,
    /// Show the welcome screen when BananaV starts (on the first time).
    #[serde(default = "first_time")]
    pub show_welcome: bool,
    /// Interface language (English unless the user picked another).
    #[serde(default)]
    pub language: crate::i18n::Language,
    /// The timeline toolbar's buttons (`None` = VEGAS's default).
    #[serde(default)]
    pub timeline_toolbar: Option<Vec<crate::toolbar_buttons::TbButton>>,
    /// The main toolbar's buttons (`None` = VEGAS's default).
    #[serde(default)]
    pub main_toolbar: Option<Vec<crate::toolbar_buttons::TbButton>>,
    /// Video templates the user saved.
    #[serde(default)]
    pub video_templates: Vec<VideoTemplate>,
    /// `None` = BananaV's defaults.
    #[serde(default)]
    pub new_project: Option<NewProjectDefaults>,
    /// The Render dialog's favorites, templates, filters and options.
    #[serde(default)]
    pub render: crate::render::RenderPrefs,
    /// Options › Preferences › Video: the graphics card that composites
    /// video.
    #[serde(default)]
    pub video_gpu: bv_engine::GpuChoice,
    /// Options › Preferences › File I/O: where video is decoded.
    #[serde(default)]
    pub decoding: bv_engine::DecodeChoice,
    /// Options › Preferences › General and Video.
    #[serde(default)]
    pub prefs: Prefs,
    /// The project open when BananaV was last closed (Options › Preferences
    /// › General: "Automatically open last project on startup").
    #[serde(default)]
    pub last_session: Option<PathBuf>,
    /// The plug-ins marked with a star in the Media Generator, Transitions
    /// and Video FX windows.
    #[serde(default)]
    pub favorites: Vec<String>,
    /// The Explorer's favorite folders (Add to Favorites).
    #[serde(default)]
    pub explorer_favorites: Vec<PathBuf>,
    /// The Video Preview's quality and split screen.
    #[serde(default)]
    pub preview: PreviewPrefs,
    /// Generator presets of the user's own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generator_presets: Vec<GeneratorPreset>,
    /// The audio tracks' output meters' range (the mouse wheel over one
    /// changes it).
    #[serde(default)]
    pub track_meter_range: MeterRange,
    /// The Time Display's own colors, per interface type (its shortcut
    /// menu's Text Color and Background Color › Custom…).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub time_display_colors: std::collections::BTreeMap<String, TimeDisplayColors>,
    /// The Loudness Meters' options.
    #[serde(default)]
    pub loudness: LoudnessPrefs,
    /// Tools › Multicamera › Edit Multicamera Audio with Video, off:
    /// choosing a take switches only the video.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multicam_audio_apart: bool,
    /// Tools › Multicamera › Show Full Frame on External Monitor: in
    /// multicamera editing the external monitor shows the program, not the
    /// takes' tiles.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multicam_full_frame: bool,
    /// The Titles & Text preset Insert Subtitles starts with (its "Use
    /// current preset as default").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle_preset: Option<String>,
    /// The audio tracks' output meters are hidden (Show Output Meters, in
    /// a track header's shortcut menu).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_track_meters: bool,
    /// The audio tracks' output meters across the header, under the name
    /// (the meters' Use Vertical Meters off).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub track_meters_across: bool,
    /// Folders searched for OFX plug-ins besides the standard ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ofx_folders: Vec<PathBuf>,
    /// The columns a plug-in's settings take in the FX window (its
    /// Columns buttons; 0 is 1).
    #[serde(default)]
    pub fx_columns: u8,
    /// The Pan/Crop window's Show Properties (off: the workspace alone, as
    /// VEGAS's opens).
    #[serde(default)]
    pub pan_crop_properties: bool,
    /// The Video Scopes' view, modes and settings.
    #[serde(default)]
    pub scopes: crate::tabs::scopes::ScopePrefs,
    /// A track header's Set Default Track Properties: what new tracks start
    /// with.
    #[serde(default, skip_serializing_if = "TrackDefaults::is_empty")]
    pub track_defaults: TrackDefaults,
    /// More › Edit Visible Button Set: the buttons the track headers show
    /// besides Mute and Solo.
    #[serde(default, skip_serializing_if = "HeaderButtons::is_empty")]
    pub header_buttons: HeaderButtons,
    /// The Video Preview's and the Trimmer's transport bars' Edit Visible
    /// Button Set (`None`: VEGAS's buttons).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_transport: Option<Vec<crate::transport_buttons::TransportButton>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trimmer_transport: Option<Vec<crate::transport_buttons::TransportButton>>,
    /// The track list's width (its border dragged); 0 is the default.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub track_list_width: f32,
    /// Project Notes' Note Default Color (an index of
    /// `bv_timeline::notes::NOTE_COLORS`) and default label.
    #[serde(default)]
    pub note_color: u8,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note_label: String,
    /// Insert › Command's templates (Command Properties' Template box).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command_templates: Vec<crate::command_ui::CommandTemplate>,
    /// View › Mixer Preview Fader: the Master Bus window's Preview strip,
    /// and its level in dB.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub preview_fader: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub preview_gain_db: f32,
    /// Auto Preview in Project Media and in the Explorer: a click on a
    /// media plays it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub media_auto_preview: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub explorer_auto_preview: bool,
    /// The Trimmer's shortcut menu, VEGAS's switches on by default: Show
    /// Video Monitor off (its picture in the Video Preview window instead,
    /// with Show Video in Preview Window), Show Video in Preview Window
    /// off, Enable Hover Scrub off; Show Video Frames, off by default, on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trimmer_no_monitor: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trimmer_no_preview_window: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trimmer_frames: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trimmer_no_hover_scrub: bool,
    /// Use All Streams and Channels off: the Trimmer adds only the stream
    /// and channels it shows.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trimmer_shown_streams_only: bool,
    /// The Trimmer's time format (its ruler's menu; `None`: the project's
    /// ruler format), counted at the project's frame rate instead of the
    /// media's (Use Project Frame Rate).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trimmer_format: Option<bv_timeline::RulerFormat>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trimmer_project_rate: bool,
    /// Project Media's Quick Tags: the tags Ctrl+1 to Ctrl+9 give.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quick_tags: Vec<String>,
    /// Project Media's Views › Media Tags: the tags' panel shows.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub media_tags_panel: bool,
    /// Automatically Synchronize Main Timeline Storyboard: it follows the
    /// timeline after each edit (else Refresh Storyboard does).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub storyboard_auto_sync: bool,
    /// File › Copy Media with Project: Save As copies the project's media
    /// next to the project file.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub save_as_copy_media: bool,
    /// File › Screen Capture's Settings.
    #[serde(default)]
    pub capture: crate::screen_capture::CaptureSettings,
    /// Tools › Burn Disc › DVD's choices.
    #[serde(default)]
    pub burn_dvd: crate::burn_dvd::BurnDvdSettings,
    /// Track Motion's keyframe presets (2D, 3D, shadow, glow).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub track_motion_presets: Vec<crate::track_motion::MotionPreset>,
    /// Tools › Burn Disc's audio CD dialogs' choices.
    #[serde(default)]
    pub burn_cd: crate::burn_cd::BurnCdSettings,
    /// Its dialog's choice: Create trimmed copies of source media (else
    /// Copy source media), and their Extra head and tail in seconds.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub copy_media_trimmed: bool,
    #[serde(default)]
    pub copy_media_extra: f64,
    /// Media Properties' video profiles: the settings files of a kind get
    /// when imported.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub video_profiles: Vec<crate::media_properties::VideoProfile>,
    /// Edit Details' views kept under names (its Preset box).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details_presets: Vec<crate::tabs::details::DetailsPreset>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

/// A button a track header can show (More › Edit Visible Button Set), in
/// the dialog's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HeaderButton {
    BypassMotionBlur,
    TrackMotion,
    ArmForRecord,
    InvertPhase,
    /// Activate Automation Controls, its arrow Automation Settings.
    Automation,
    TrackFx,
    Mute,
    Solo,
    CompositingMode,
    MakeParent,
    MakeChild,
    RecordInput,
    /// Minimize and Maximize (settings saved before kept them apart).
    #[serde(alias = "Minimize", alias = "Maximize")]
    MinMax,
    ColorGrading,
    DuckController,
    DuckListener,
}

impl HeaderButton {
    /// A video track's buttons, as VEGAS 23's dialog lists them, then an
    /// audio track's.
    pub const VIDEO: [HeaderButton; 11] = [
        HeaderButton::BypassMotionBlur,
        HeaderButton::TrackMotion,
        HeaderButton::TrackFx,
        HeaderButton::Automation,
        HeaderButton::Mute,
        HeaderButton::Solo,
        HeaderButton::CompositingMode,
        HeaderButton::MakeParent,
        HeaderButton::MakeChild,
        HeaderButton::MinMax,
        HeaderButton::ColorGrading,
    ];
    pub const AUDIO: [HeaderButton; 10] = [
        HeaderButton::ArmForRecord,
        HeaderButton::InvertPhase,
        HeaderButton::Automation,
        HeaderButton::TrackFx,
        HeaderButton::Mute,
        HeaderButton::Solo,
        HeaderButton::RecordInput,
        HeaderButton::MinMax,
        HeaderButton::DuckController,
        HeaderButton::DuckListener,
    ];
    /// What a header shows at first, as VEGAS's.
    pub const DEFAULT: [HeaderButton; 2] = [HeaderButton::Mute, HeaderButton::Solo];

    pub fn of(kind: bv_timeline::TrackKind) -> &'static [HeaderButton] {
        match kind {
            bv_timeline::TrackKind::Video => &Self::VIDEO,
            bv_timeline::TrackKind::Audio => &Self::AUDIO,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            HeaderButton::BypassMotionBlur => tr("Bypass Motion Blur"),
            HeaderButton::TrackMotion => tr("Track Motion"),
            HeaderButton::ArmForRecord => tr("Arm For Record"),
            HeaderButton::InvertPhase => tr("Invert Track Phase"),
            HeaderButton::Automation => tr("Activate Automation Controls"),
            HeaderButton::TrackFx => tr("Track FX"),
            HeaderButton::Mute => tr("Mute"),
            HeaderButton::Solo => tr("Solo"),
            HeaderButton::CompositingMode => tr("Compositing Mode"),
            HeaderButton::MakeParent => tr("Make Compositing Parent"),
            HeaderButton::MakeChild => tr("Make Compositing Child"),
            HeaderButton::RecordInput => tr("Record Input"),
            HeaderButton::MinMax => tr("Min/Max"),
            HeaderButton::ColorGrading => tr("Color Grading"),
            HeaderButton::DuckController => tr("Auto Ducking Controller"),
            HeaderButton::DuckListener => tr("Auto Ducking Listener"),
        }
    }

    /// `buttons` once each, in the dialog's order for `kind`.
    pub fn in_order(kind: bv_timeline::TrackKind, buttons: &[HeaderButton]) -> Vec<HeaderButton> {
        Self::of(kind)
            .iter()
            .copied()
            .filter(|b| buttons.contains(b))
            .collect()
    }
}

/// The buttons the track headers show, for each kind (`None`: VEGAS's,
/// Mute and Solo).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "StoredHeaderButtons")]
pub struct HeaderButtons {
    #[serde(rename = "video_set", skip_serializing_if = "Option::is_none")]
    video: Option<Vec<HeaderButton>>,
    #[serde(rename = "audio_set", skip_serializing_if = "Option::is_none")]
    audio: Option<Vec<HeaderButton>>,
}

/// The settings file's `header_buttons`: before Mute and Solo were among
/// the buttons, `video` and `audio` listed those shown besides them.
#[derive(Default, Deserialize)]
#[serde(default)]
struct StoredHeaderButtons {
    video: Vec<HeaderButton>,
    audio: Vec<HeaderButton>,
    video_set: Option<Vec<HeaderButton>>,
    audio_set: Option<Vec<HeaderButton>>,
}

impl From<StoredHeaderButtons> for HeaderButtons {
    fn from(s: StoredHeaderButtons) -> Self {
        let kind = |set: Option<Vec<HeaderButton>>, older: Vec<HeaderButton>, k| {
            set.or_else(|| {
                (!older.is_empty()).then(|| {
                    let mut all = older;
                    all.extend(HeaderButton::DEFAULT);
                    HeaderButton::in_order(k, &all)
                })
            })
        };
        HeaderButtons {
            video: kind(s.video_set, s.video, bv_timeline::TrackKind::Video),
            audio: kind(s.audio_set, s.audio, bv_timeline::TrackKind::Audio),
        }
    }
}

impl HeaderButtons {
    pub fn is_empty(&self) -> bool {
        self.video.is_none() && self.audio.is_none()
    }

    /// The buttons `kind`'s headers show, in the dialog's order.
    pub fn of(&self, kind: bv_timeline::TrackKind) -> Vec<HeaderButton> {
        let set = match kind {
            bv_timeline::TrackKind::Video => &self.video,
            bv_timeline::TrackKind::Audio => &self.audio,
        };
        set.clone()
            .unwrap_or_else(|| HeaderButton::DEFAULT.to_vec())
    }

    /// Shows `buttons` on `kind`'s headers.
    pub fn set(&mut self, kind: bv_timeline::TrackKind, buttons: &[HeaderButton]) {
        let buttons = HeaderButton::in_order(kind, buttons);
        let set = (buttons != HeaderButton::DEFAULT).then_some(buttons);
        match kind {
            bv_timeline::TrackKind::Video => self.video = set,
            bv_timeline::TrackKind::Audio => self.audio = set,
        }
    }
}

/// What new tracks of a kind start with, taken from a track by Set Default
/// Track Properties (`None`: BananaV's own default).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KindDefaults {
    /// Audio: the volume (its gain).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
    /// Video: the compositing mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub composite_mode: Option<bv_timeline::CompositeMode>,
    /// Audio: the pan type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pan_mode: Option<bv_timeline::PanMode>,
    /// The height in the timeline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    /// The track's effects and their settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects: Option<Vec<bv_timeline::Effect>>,
    /// Audio: record input monitoring (Off, On or Auto).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_monitor: Option<bv_timeline::InputMonitor>,
    /// The Automation Settings button and the automation mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automation: Option<bv_timeline::TrackAutomation>,
}

impl KindDefaults {
    pub fn is_empty(&self) -> bool {
        *self == KindDefaults::default()
    }

    /// Gives a new track these defaults.
    pub fn apply(&self, t: &mut bv_timeline::Track) {
        if let Some(v) = self.volume {
            t.level = v;
        }
        if let Some(m) = self.composite_mode {
            t.composite_mode = m;
        }
        if let Some(m) = self.pan_mode {
            t.pan_mode = m;
        }
        if let Some(fx) = &self.effects {
            t.effects = fx.clone();
        }
        if let Some(m) = self.input_monitor {
            t.input_monitor = m;
        }
        if let Some(a) = self.automation {
            t.automation = a;
        }
    }
}

/// New tracks' defaults, one set for each kind.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackDefaults {
    #[serde(skip_serializing_if = "KindDefaults::is_empty")]
    pub audio: KindDefaults,
    #[serde(skip_serializing_if = "KindDefaults::is_empty")]
    pub video: KindDefaults,
}

impl TrackDefaults {
    pub fn is_empty(&self) -> bool {
        self.audio.is_empty() && self.video.is_empty()
    }

    pub fn of(&self, kind: bv_timeline::TrackKind) -> &KindDefaults {
        match kind {
            bv_timeline::TrackKind::Audio => &self.audio,
            bv_timeline::TrackKind::Video => &self.video,
        }
    }

    pub fn of_mut(&mut self, kind: bv_timeline::TrackKind) -> &mut KindDefaults {
        match kind {
            bv_timeline::TrackKind::Audio => &mut self.audio,
            bv_timeline::TrackKind::Video => &mut self.video,
        }
    }
}

/// How far down the audio tracks' output meters reach, and how their scale
/// is marked: VEGAS's four, the mouse wheel over a meter going from one to
/// the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeterRange {
    /// 10.5 dB, numbered every 3.
    Fine,
    /// 42 dB, numbered every 9.
    #[default]
    Normal,
    /// 78 dB, numbered every 18.
    Wide,
    /// 120 dB, numbered every 27.
    Widest,
}

impl MeterRange {
    pub const ALL: [MeterRange; 4] = [
        MeterRange::Fine,
        MeterRange::Normal,
        MeterRange::Wide,
        MeterRange::Widest,
    ];

    /// The meter's bottom, in dB.
    pub fn floor(self) -> f32 {
        match self {
            MeterRange::Fine => -10.5,
            MeterRange::Normal => -42.0,
            MeterRange::Wide => -78.0,
            MeterRange::Widest => -120.0,
        }
    }

    /// dB between the scale's numbers.
    pub fn step(self) -> u32 {
        match self {
            MeterRange::Fine => 3,
            MeterRange::Normal => 9,
            MeterRange::Wide => 18,
            MeterRange::Widest => 27,
        }
    }

    /// dB between the scale's ticks (a third of a step, but on the finest).
    pub fn tick(self) -> u32 {
        match self {
            MeterRange::Fine => 3,
            r => r.step() / 3,
        }
    }

    /// `notches` of the mouse wheel on: rolled away (up), closer ranges;
    /// toward the user, wider ones.
    pub fn rolled(self, notches: i32) -> MeterRange {
        let i = MeterRange::ALL.iter().position(|&r| r == self).unwrap_or(1) as i32;
        MeterRange::ALL[(i - notches).clamp(0, 3) as usize]
    }
}

/// `%APPDATA%\BananaV` on Windows, `~/.config/BananaV` on Linux.
fn default_path() -> Option<PathBuf> {
    Some(
        bv_platform::config_dir()?
            .join("BananaV")
            .join("interface.json"),
    )
}

impl UiSettings {
    pub fn load() -> UiSettings {
        let path = default_path();
        let mut s: UiSettings = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            // The first time: the welcome screen, to choose the language
            // and start a project.
            .unwrap_or_else(|| UiSettings {
                show_welcome: true,
                ..UiSettings::default()
            });
        s.path = path;
        s.layouts.resize(10, None);
        s.prefs.follow_new_defaults();
        s
    }

    /// The folder window layouts are saved to: the one chosen, else
    /// `Layouts` beside these settings (none for settings never written:
    /// the tests').
    pub fn layouts_folder(&self) -> Option<PathBuf> {
        self.layout_folder
            .clone()
            .or_else(|| Some(self.path.as_ref()?.parent()?.join("Layouts")))
    }

    /// Settings that are never written to disk (tests).
    pub fn in_memory() -> UiSettings {
        UiSettings {
            layouts: vec![None; 10],
            ..UiSettings::default()
        }
    }

    /// Kept in memory only, never written (screen tests).
    pub(crate) fn is_in_memory(&self) -> bool {
        self.path.is_none()
    }

    /// Settings kept in the file `path` (tests).
    #[cfg(test)]
    pub(crate) fn in_file(path: PathBuf) -> UiSettings {
        UiSettings {
            path: Some(path),
            ..UiSettings::in_memory()
        }
    }

    /// The same settings, never saved (the screen tests' `*_for_review`
    /// hooks).
    pub(crate) fn unsaved(&self) -> UiSettings {
        UiSettings {
            path: None,
            ..self.clone()
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Keeps `session` and writes it into the settings' file alone: what
    /// another BananaV window wrote there meanwhile stays (written as it
    /// changes, it would otherwise put back older settings).
    pub fn save_session(&mut self, session: Session) -> Result<(), String> {
        self.session = Some(session);
        let Some(path) = &self.path else {
            return Ok(());
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return self.save();
        };
        let mut on_disk: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let serde_json::Value::Object(fields) = &mut on_disk else {
            return self.save();
        };
        let session = serde_json::to_value(&self.session).map_err(|e| e.to_string())?;
        fields.insert("session".into(), session);
        let json = serde_json::to_string_pretty(&on_disk).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Moves `path` to the top of the recent list.
    pub fn touch_recent(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(MAX_RECENT);
        let _ = self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_core::{Tick, TimeRange};

    #[test]
    fn the_wheel_goes_through_the_output_meters_ranges() {
        let normal = MeterRange::default();
        assert_eq!(normal, MeterRange::Normal);
        assert_eq!(
            (normal.floor(), normal.step(), normal.tick()),
            (-42.0, 9, 3)
        );
        // Rolled away: closer; toward the user: wider; no further than the
        // ends.
        assert_eq!(normal.rolled(1), MeterRange::Fine);
        assert_eq!(normal.rolled(3), MeterRange::Fine);
        assert_eq!(normal.rolled(-1), MeterRange::Wide);
        assert_eq!(normal.rolled(-2), MeterRange::Widest);
        assert_eq!(MeterRange::Widest.rolled(-1), MeterRange::Widest);
        let scales: Vec<_> = [MeterRange::Fine, MeterRange::Wide, MeterRange::Widest]
            .iter()
            .map(|r| (r.floor(), r.step(), r.tick()))
            .collect();
        assert_eq!(scales, [(-10.5, 3, 3), (-78.0, 18, 6), (-120.0, 27, 9)]);
    }

    /// The session goes into the settings' file alone: what another
    /// BananaV window wrote there meanwhile stays.
    #[test]
    fn the_session_is_written_alone() {
        let dir = std::env::temp_dir().join(format!("bananav-session-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("interface.json");
        let mut mine = UiSettings::in_file(path.clone());
        mine.save().unwrap();
        let mut other = UiSettings::in_file(path.clone());
        other.recent = vec![PathBuf::from("other.veg")];
        other.save().unwrap();
        let session = Session {
            dock: serde_json::json!({ "surfaces": [] }),
            hide_toolbar: true,
            ..Session::default()
        };
        mine.save_session(session.clone()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let back: UiSettings = serde_json::from_str(&text).unwrap();
        assert_eq!(back.recent, [PathBuf::from("other.veg")]);
        assert_eq!(back.session.as_ref(), Some(&session));
        assert_eq!(mine.session, Some(session));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The volume envelope's old blue, saved but never chosen, takes
    /// VEGAS's purple; a color chosen stays.
    #[test]
    fn an_old_default_envelope_blue_follows_the_new_purple() {
        let mut p = Prefs::default();
        let (volume, composite) = (
            EnvelopeColor::Volume.index(),
            EnvelopeColor::CompositeLevel.index(),
        );
        p.envelope_colors[volume] = rgba(OLD_ENVELOPE_BLUE);
        p.envelope_colors[composite] = [1, 2, 3, 255];
        p.follow_new_defaults();
        assert_eq!(p.envelope_colors[volume], rgba(0x493FAB));
        assert_eq!(p.envelope_colors[composite], [1, 2, 3, 255]);
    }

    #[test]
    fn display_starts_as_vegas_does() {
        let p = Prefs::default();
        assert_eq!(p.interface, crate::theme::Interface::Dark);
        assert_eq!(p.icon_strength, 100);
        assert!(p.high_dpi, "Use High DPI scaling: on");
        assert!(!p.auto_hide_dock && !p.tabs_at_top && !p.floating_dock && !p.system_stats);
        // VEGAS's lists: 27 envelope types and 12 snap types.
        assert_eq!(EnvelopeColor::all().len(), 27);
        assert_eq!(p.envelope_colors.len(), 27);
        assert_eq!(p.snap_colors.len(), SnapKind::ALL.len());
        assert_eq!(
            EnvelopeColor::all()[2..6],
            (1..=4)
                .map(EnvelopeColor::AssignableFxVol)
                .collect::<Vec<_>>()
        );
        assert_eq!(EnvelopeColor::all()[26], EnvelopeColor::Mute);
        // Each of BananaV's envelopes has its color, and can change it.
        for kind in [
            bv_timeline::EnvelopeKind::Volume,
            bv_timeline::EnvelopeKind::Pan,
            bv_timeline::EnvelopeKind::Mute,
            bv_timeline::EnvelopeKind::CompositeLevel,
            bv_timeline::EnvelopeKind::FadeToColor,
            bv_timeline::EnvelopeKind::Velocity,
            bv_timeline::EnvelopeKind::TransitionProgress,
            bv_timeline::EnvelopeKind::FxParam { fx: 1, param: 12 },
            bv_timeline::EnvelopeKind::FxBypass(1),
        ] {
            assert!(EnvelopeColor::of(kind).ready(), "{kind:?}");
        }
        // FX automation's parameters take VEGAS's FX Parameter colors in
        // turn.
        assert_eq!(
            EnvelopeColor::of(bv_timeline::EnvelopeKind::FxParam { fx: 1, param: 12 }),
            EnvelopeColor::FxParameter(3)
        );
        assert!(!SnapKind::CdIndex.ready() && SnapKind::DragOrigin.ready());
    }

    #[test]
    fn colors_saved_before_a_list_grew_keep_their_place() {
        let mut p = Prefs {
            envelope_colors: vec![[1, 2, 3, 255]],
            snap_colors: Vec::new(),
            ..Prefs::default()
        };
        // Read: what is missing has its default.
        let mute = EnvelopeColor::Mute;
        assert_eq!(p.envelope_color(mute).to_array(), mute.default_color());
        assert_eq!(
            p.envelope_color(EnvelopeColor::Volume).to_array(),
            [1, 2, 3, 255]
        );
        // Changed: the list grows to its full length first.
        *p.snap_color_mut(SnapKind::Grid) = [9, 9, 9, 255];
        assert_eq!(p.snap_colors.len(), SnapKind::ALL.len());
        assert_eq!(p.snap_color(SnapKind::Grid).to_array(), [9, 9, 9, 255]);
        assert_eq!(
            p.snap_color(SnapKind::Cursor).to_array(),
            SnapKind::Cursor.default_color()
        );
        *p.envelope_color_mut(mute) = [4, 5, 6, 255];
        assert_eq!(p.envelope_colors.len(), 27);
        assert_eq!(p.envelope_color(mute).to_array(), [4, 5, 6, 255]);
        // Settings of an older BananaV, without the Display page: defaults.
        let old: Prefs = serde_json::from_str("{}").unwrap();
        assert_eq!(old.track_colors, Prefs::default().track_colors);
        assert!(old.high_dpi);
        // Default All puts the page back.
        p.interface = crate::theme::Interface::White;
        p.track_colors[0] = [0, 0, 0, 255];
        p.reset_display();
        let d = Prefs::default();
        assert_eq!(p.interface, d.interface);
        assert_eq!(p.track_colors, d.track_colors);
        assert_eq!(p.envelope_colors, d.envelope_colors);
    }

    #[test]
    fn jkl_speeds_step_up_to_their_last() {
        let steps = |s: JklSpeed| {
            let mut r = s.next_rate(None);
            let mut out = vec![r];
            for _ in 0..3 {
                r = s.next_rate(Some(r));
                out.push(r);
            }
            out
        };
        assert_eq!(steps(JklSpeed::Slow), [0.5, 1.0, 2.0, 2.0]);
        assert_eq!(steps(JklSpeed::Medium), [1.0, 2.0, 4.0, 4.0]);
        assert_eq!(steps(JklSpeed::Fast), [2.0, 4.0, 4.0, 4.0]);
        // Playing at normal speed (Space), L goes faster at once.
        assert_eq!(JklSpeed::Slow.next_rate(Some(1.0)), 2.0);
    }

    #[test]
    fn overlaps_and_fades_placed_as_aligned() {
        let s = Tick::from_seconds;
        assert_eq!(CutAlign::BeforeCut.split(s(1)), (s(1), Tick::ZERO));
        assert_eq!(CutAlign::AfterCut.split(s(1)), (Tick::ZERO, s(1)));
        let (a, b) = CutAlign::Centered.split(s(1));
        assert_eq!(a + b, s(1));
        // A 2–6 s selection with 1 s fades.
        let sel = TimeRange::from_bounds(s(2), s(6));
        let ranges = |a: FadeAlign| {
            let (o, i) = a.ranges(sel, s(1));
            (o.start, o.end(), i.start, i.end())
        };
        assert_eq!(ranges(FadeAlign::Outside), (s(1), s(7), s(2), s(6)));
        assert_eq!(ranges(FadeAlign::Inside), (s(2), s(6), s(3), s(5)));
        let half = Tick(s(1).0 / 2);
        assert_eq!(
            ranges(FadeAlign::Centered),
            (s(2) - half, s(6) + half, s(2) + half, s(6) - half)
        );
        // Fades longer than the selection meet in its middle.
        let (_, inner) = FadeAlign::Inside.ranges(sel, s(5));
        assert_eq!((inner.start, inner.end()), (s(4), s(4)));
    }
}
