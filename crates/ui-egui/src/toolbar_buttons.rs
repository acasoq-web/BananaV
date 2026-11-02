//! The buttons the toolbars can show (Options › Customize Toolbar for the
//! main toolbar, Options › Customize Timeline Toolbar for the timeline's),
//! in VEGAS's names and default order. Each toolbar is the user's list of
//! these, saved in the interface settings.

use bv_core::Tick;
use bv_timeline::EnvelopeKind;
use serde::{Deserialize, Serialize};

use crate::app::more_actions::Script;
use crate::app::{BananaApp, Tool};
use crate::commands::Cmd;
use crate::dock::Tab;
use crate::i18n::tr;
use crate::icons::Icon;
use crate::keymap::{Action, Extra};

/// The two customizable toolbars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolbarKind {
    /// Under the menus.
    Main,
    /// At the bottom of the timeline.
    Timeline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TbButton {
    Separator,
    // VEGAS's default main toolbar.
    NewEmptyProject,
    Open,
    Save,
    Render,
    Properties,
    Cut,
    Copy,
    Paste,
    Undo,
    Redo,
    // Available for the main toolbar.
    NewProject,
    SaveAs,
    IncrementalSave,
    TriggerFromMidiTimecode,
    GenerateMidiTimecode,
    GenerateMidiClock,
    OpenInAudioEditor,
    Metronome,
    StartCapture,
    ExtractAudioFromCd,
    ImportMedia,
    ImportDvdCamcorderDisc,
    AutoHideDockingArea,
    BurnDiscAtOnceAudioCd,
    BurnDvd,
    CreateSlideshow,
    TextToSpeech,
    SpeechToText,
    AiAssistant,
    MulticameraAudioSync,
    AiAutoReframe,
    HardwareDetection,
    Explorer,
    ToolsRender,
    PrerenderVideo,
    ApplyNonRealTimeEventFx,
    RenderToNewTrack,
    WelcomeScreen,
    /// A script of Tools › Scripting ([`SCRIPTS`]).
    Script(u8),
    // VEGAS's default timeline toolbar.
    Record,
    LoopPlayback,
    PlayFromStart,
    Play,
    Pause,
    Stop,
    GoToStart,
    GoToEnd,
    PreviousFrame,
    NextFrame,
    NormalTool,
    EnvelopeTool,
    SelectionTool,
    ZoomTool,
    Delete,
    Trim,
    TrimStart,
    TrimEnd,
    Split,
    SmartSplit,
    LockEvent,
    InsertMarker,
    InsertRegion,
    EnableSnapping,
    AutomaticCrossfades,
    AutoRipple,
    LockEnvelopesToEvents,
    IgnoreEventGrouping,
    ColorGrading,
    OpenParentTimeline,
    OpenNestedTimeline,
    CreateNestedTimeline,
    // Available, not on the toolbar by default.
    NewAudioTrack,
    ShowVolumeEnvelopes,
    ShowPanEnvelopes,
    InsertBus,
    InsertInputBus,
    InsertAssignableFx,
    BypassAllAudioFx,
    QuickFadeAudioEdits,
    InsertText,
    NewVideoTrack,
    NewAdjustmentTrack,
    MuteAllAudioTracks,
    MuteAllVideoTracks,
    ShowVideoEventEdgeFrames,
    ShowEventMediaMarkers,
    SnapToGrid,
    SnapToMarkers,
    SnapToAllEvents,
    QuantizeToFrames,
    SlipAllTakes,
    MotionTracking,
    DetectScenesAndSplit,
}

use TbButton as B;

/// Scripts of Tools › Scripting, in VEGAS's order, with BananaV's command
/// when it has one.
const SCRIPTS: [&str; 18] = [
    "Add Timecode to all Media",
    "Batch Render",
    "Export Chapters",
    "Export Closed Captioning for DVD Architect",
    "Export Closed Captioning for QuickTime",
    "Export Closed Captioning for Windows Media Player",
    "Export Closed Captioning for YouTube",
    "Export EDL",
    "Export Regions as Subtitles",
    "Group Video and Audio Events",
    "Promote Media Closed Captioning",
    "Promote Media Markers",
    "Remove Letterboxing",
    "Remove Timecode from all Media",
    "Render Audio Tracks",
    "Repair and Smooth unwanted Section",
    "Resize Generated Media",
    "Stereo to Mono",
];
const SCRIPT_CMDS: [Option<Cmd>; 18] = [
    None,
    None,
    Some(Cmd::RunScript(Script::ExportChapters)),
    None,
    None,
    None,
    None,
    Some(Cmd::ExportEdl),
    Some(Cmd::RunScript(Script::ExportRegionsAsSubtitles)),
    Some(Cmd::RunScript(Script::GroupVideoAndAudio)),
    None,
    None,
    Some(Cmd::RunScript(Script::RemoveLetterboxing)),
    None,
    None,
    None,
    None,
    Some(Cmd::RunScript(Script::StereoToMono)),
];

/// VEGAS's default main toolbar (no separators).
pub const MAIN_DEFAULT: &[TbButton] = &[
    B::NewEmptyProject,
    B::Open,
    B::Save,
    B::Render,
    B::Properties,
    B::Cut,
    B::Copy,
    B::Paste,
    B::Undo,
    B::Redo,
];

/// Every main-toolbar button, in the order of the "Available toolbar
/// buttons" list (VEGAS's extras first, then the default ones).
pub fn main_all() -> Vec<TbButton> {
    let mut v = vec![
        B::NewProject,
        B::SaveAs,
        B::IncrementalSave,
        B::TriggerFromMidiTimecode,
        B::GenerateMidiTimecode,
        B::GenerateMidiClock,
        B::OpenInAudioEditor,
        B::Metronome,
        B::StartCapture,
        B::ExtractAudioFromCd,
        B::ImportMedia,
        B::ImportDvdCamcorderDisc,
        B::AutoHideDockingArea,
        B::BurnDiscAtOnceAudioCd,
        B::BurnDvd,
        B::CreateSlideshow,
        B::TextToSpeech,
        B::SpeechToText,
        B::AiAssistant,
        B::MulticameraAudioSync,
        B::AiAutoReframe,
        B::ColorGrading,
        B::HardwareDetection,
        B::MotionTracking,
        B::Explorer,
        B::ToolsRender,
        B::PrerenderVideo,
        B::ApplyNonRealTimeEventFx,
        B::RenderToNewTrack,
        B::WelcomeScreen,
    ];
    v.extend((0..SCRIPTS.len() as u8).map(B::Script));
    v.extend_from_slice(MAIN_DEFAULT);
    v
}

impl ToolbarKind {
    pub fn default_buttons(self) -> &'static [TbButton] {
        match self {
            ToolbarKind::Main => MAIN_DEFAULT,
            ToolbarKind::Timeline => DEFAULT,
        }
    }

    /// Every button this toolbar can have, in the available list's order.
    pub fn all_buttons(self) -> Vec<TbButton> {
        match self {
            ToolbarKind::Main => main_all(),
            ToolbarKind::Timeline => ALL.to_vec(),
        }
    }
}

/// VEGAS's default timeline toolbar.
pub const DEFAULT: &[TbButton] = &[
    B::Record,
    B::LoopPlayback,
    B::PlayFromStart,
    B::Play,
    B::Pause,
    B::Stop,
    B::GoToStart,
    B::GoToEnd,
    B::PreviousFrame,
    B::NextFrame,
    B::Separator,
    B::NormalTool,
    B::EnvelopeTool,
    B::SelectionTool,
    B::ZoomTool,
    B::Separator,
    B::Delete,
    B::Trim,
    B::TrimStart,
    B::TrimEnd,
    B::Split,
    B::SmartSplit,
    B::LockEvent,
    B::Separator,
    B::InsertMarker,
    B::InsertRegion,
    B::Separator,
    B::EnableSnapping,
    B::AutomaticCrossfades,
    B::AutoRipple,
    B::LockEnvelopesToEvents,
    B::IgnoreEventGrouping,
    B::Separator,
    B::ColorGrading,
    B::Separator,
    B::OpenParentTimeline,
    B::OpenNestedTimeline,
    B::CreateNestedTimeline,
];

/// Every button, in the order of the "Available toolbar buttons" list
/// (VEGAS's extras first, then the default ones when they are removed).
pub const ALL: &[TbButton] = &[
    B::NewAudioTrack,
    B::ShowVolumeEnvelopes,
    B::ShowPanEnvelopes,
    B::InsertBus,
    B::InsertInputBus,
    B::InsertAssignableFx,
    B::BypassAllAudioFx,
    B::QuickFadeAudioEdits,
    B::InsertText,
    B::NewVideoTrack,
    B::NewAdjustmentTrack,
    B::MuteAllAudioTracks,
    B::MuteAllVideoTracks,
    B::ShowVideoEventEdgeFrames,
    B::ShowEventMediaMarkers,
    B::SnapToGrid,
    B::SnapToMarkers,
    B::SnapToAllEvents,
    B::QuantizeToFrames,
    B::SlipAllTakes,
    B::MotionTracking,
    B::DetectScenesAndSplit,
    B::Record,
    B::LoopPlayback,
    B::PlayFromStart,
    B::Play,
    B::Pause,
    B::Stop,
    B::GoToStart,
    B::GoToEnd,
    B::PreviousFrame,
    B::NextFrame,
    B::NormalTool,
    B::EnvelopeTool,
    B::SelectionTool,
    B::ZoomTool,
    B::Delete,
    B::Trim,
    B::TrimStart,
    B::TrimEnd,
    B::Split,
    B::SmartSplit,
    B::LockEvent,
    B::InsertMarker,
    B::InsertRegion,
    B::EnableSnapping,
    B::AutomaticCrossfades,
    B::AutoRipple,
    B::LockEnvelopesToEvents,
    B::IgnoreEventGrouping,
    B::ColorGrading,
    B::OpenParentTimeline,
    B::OpenNestedTimeline,
    B::CreateNestedTimeline,
];

/// What a button does.
enum Does {
    Cmd(Cmd),
    Extra(Extra),
    /// Its own action (the transport keeps its fixed keys).
    Own,
    /// Not in BananaV yet: shown greyed.
    Soon,
}

impl TbButton {
    pub fn label(self) -> &'static str {
        tr(match self {
            B::Separator => "Separator",
            B::NewEmptyProject => "New Empty Project",
            B::Open => "Open",
            B::Save => "Save",
            B::Render | B::ToolsRender => "Render",
            B::Properties => "Properties",
            B::Cut => "Cut",
            B::Copy => "Copy",
            B::Paste => "Paste",
            B::Undo => "Undo",
            B::Redo => "Redo",
            B::NewProject => "New Project",
            B::SaveAs => "Save As",
            B::IncrementalSave => "Incremental Save",
            B::TriggerFromMidiTimecode => "Trigger from MIDI Timecode",
            B::GenerateMidiTimecode => "Generate MIDI Timecode",
            B::GenerateMidiClock => "Generate MIDI Clock",
            B::OpenInAudioEditor => "Open in Audio Editor",
            B::Metronome => "Metronome",
            B::StartCapture => "Start Capture",
            B::ExtractAudioFromCd => "Extract Audio from CD…",
            B::ImportMedia => "Import Media…",
            B::ImportDvdCamcorderDisc => "Import DVD Camcorder Disc",
            B::AutoHideDockingArea => "Auto Hide Docking Area",
            B::BurnDiscAtOnceAudioCd => "Burn Disc-at-Once Audio CD",
            B::BurnDvd => "Burn DVD",
            B::CreateSlideshow => "Create Slideshow",
            B::TextToSpeech => "Text to Speech",
            B::SpeechToText => "Speech to Text",
            B::AiAssistant => "AI Assistant (Early Access)",
            B::MulticameraAudioSync => "Multicamera Audio Sync",
            B::AiAutoReframe => "AI Auto Reframe",
            B::HardwareDetection => "Hardware Detection",
            B::Explorer => "Explorer",
            B::PrerenderVideo => "Prerender Video",
            B::ApplyNonRealTimeEventFx => "Apply Non-Real-Time Event FX",
            B::RenderToNewTrack => "Render to New Track",
            B::WelcomeScreen => "Welcome Screen",
            B::Script(i) => SCRIPTS.get(i as usize).copied().unwrap_or("Script"),
            B::Record => "Record",
            B::LoopPlayback => "Loop Playback",
            B::PlayFromStart => "Play From Start",
            B::Play => "Play",
            B::Pause => "Pause",
            B::Stop => "Stop",
            B::GoToStart => "Go to Start",
            B::GoToEnd => "Go to End",
            B::PreviousFrame => "Previous Frame",
            B::NextFrame => "Next Frame",
            B::NormalTool => "Normal Edit Tool",
            B::EnvelopeTool => "Envelope Edit Tool",
            B::SelectionTool => "Selection Edit Tool",
            B::ZoomTool => "Zoom Edit Tool",
            B::Delete => "Delete",
            B::Trim => "Trim",
            B::TrimStart => "Trim Start",
            B::TrimEnd => "Trim End",
            B::Split => "Split",
            B::SmartSplit => "Smart Split",
            B::LockEvent => "Lock Event",
            B::InsertMarker => "Insert Marker",
            B::InsertRegion => "Insert Region",
            B::EnableSnapping => "Enable Snapping (Hold Shift to Override)",
            B::AutomaticCrossfades => "Automatic Crossfades",
            B::AutoRipple => "Auto Ripple",
            B::LockEnvelopesToEvents => "Lock Envelopes to Events",
            B::IgnoreEventGrouping => "Ignore Event Grouping",
            B::ColorGrading => "Color Grading",
            B::OpenParentTimeline => "Open Parent Timeline",
            B::OpenNestedTimeline => "Open Nested Timeline",
            B::CreateNestedTimeline => "Create Nested Timeline",
            B::NewAudioTrack => "New Audio Track",
            B::ShowVolumeEnvelopes => "Show Volume Envelopes",
            B::ShowPanEnvelopes => "Show Pan Envelopes",
            B::InsertBus => "Insert Bus",
            B::InsertInputBus => "Insert Input Bus",
            B::InsertAssignableFx => "Insert Assignable FX…",
            B::BypassAllAudioFx => "Bypass All Audio FX",
            B::QuickFadeAudioEdits => "Quick Fade Audio Edits",
            B::InsertText => "Insert Text",
            B::NewVideoTrack => "New Video Track",
            B::NewAdjustmentTrack => "New Adjustment Track",
            B::MuteAllAudioTracks => "Mute All Audio Tracks",
            B::MuteAllVideoTracks => "Mute All Video Tracks",
            B::ShowVideoEventEdgeFrames => "Show Video Event Edge Frames",
            B::ShowEventMediaMarkers => "Show Event Media Markers",
            B::SnapToGrid => "Snap to Grid",
            B::SnapToMarkers => "Snap to Markers",
            B::SnapToAllEvents => "Snap to All Events",
            B::QuantizeToFrames => "Quantize to Frames",
            B::SlipAllTakes => "Slip All Takes",
            B::MotionTracking => "Motion Tracking",
            B::DetectScenesAndSplit => "Detect Scenes and Split",
        })
    }

    pub fn icon(self) -> Option<Icon> {
        Some(match self {
            B::Separator => return None,
            B::NewEmptyProject | B::NewProject => Icon::New,
            B::Open => Icon::Open,
            B::Save => Icon::Save,
            B::Render => Icon::Render,
            B::Properties => Icon::Settings,
            B::Cut => Icon::Cut,
            B::Copy => Icon::Copy,
            B::Paste => Icon::Paste,
            B::Undo => Icon::Undo,
            B::Redo => Icon::Redo,
            B::SaveAs => Icon::SaveAs,
            B::IncrementalSave => Icon::SaveIncremental,
            B::TriggerFromMidiTimecode | B::GenerateMidiTimecode | B::GenerateMidiClock => {
                Icon::Midi
            }
            B::OpenInAudioEditor => Icon::FileAudio,
            B::Metronome => Icon::Metronome,
            B::StartCapture => Icon::Camera,
            B::ExtractAudioFromCd
            | B::ImportDvdCamcorderDisc
            | B::BurnDiscAtOnceAudioCd
            | B::BurnDvd => Icon::Disc,
            B::ImportMedia => Icon::Import,
            B::AutoHideDockingArea => Icon::Dock,
            B::CreateSlideshow => Icon::Slideshow,
            B::TextToSpeech | B::SpeechToText => Icon::Speech,
            B::AiAssistant => Icon::Sparkle,
            B::MulticameraAudioSync => Icon::AudioSync,
            B::AiAutoReframe => Icon::PanCrop,
            B::HardwareDetection
            | B::Explorer
            | B::ToolsRender
            | B::PrerenderVideo
            | B::ApplyNonRealTimeEventFx
            | B::RenderToNewTrack
            | B::WelcomeScreen
            | B::Script(_) => Icon::Script,
            B::Record => Icon::Record,
            B::LoopPlayback => Icon::Loop,
            B::PlayFromStart => Icon::PlayFromStart,
            B::Play => Icon::Play,
            B::Pause => Icon::Pause,
            B::Stop => Icon::Stop,
            B::GoToStart => Icon::GoStart,
            B::GoToEnd => Icon::GoEnd,
            B::PreviousFrame => Icon::PrevFrame,
            B::NextFrame => Icon::NextFrame,
            B::NormalTool => Icon::EditTool,
            B::EnvelopeTool => Icon::EnvelopeTool,
            B::SelectionTool => Icon::SelectTool,
            B::ZoomTool => Icon::ZoomTool,
            B::Delete => Icon::Delete,
            B::Trim => Icon::Trim,
            B::TrimStart => Icon::TrimStart,
            B::TrimEnd => Icon::TrimEnd,
            B::Split => Icon::Split,
            B::SmartSplit => Icon::SmartSplit,
            B::LockEvent => Icon::Lock,
            B::InsertMarker => Icon::Marker,
            B::InsertRegion => Icon::Region,
            B::EnableSnapping => Icon::Snap,
            B::AutomaticCrossfades => Icon::Crossfade,
            B::AutoRipple => Icon::Ripple,
            B::LockEnvelopesToEvents => Icon::LockEnvelopes,
            B::IgnoreEventGrouping => Icon::IgnoreGroups,
            B::ColorGrading => Icon::ColorGrading,
            B::OpenParentTimeline => Icon::ParentTimeline,
            B::OpenNestedTimeline => Icon::NestedTimeline,
            B::CreateNestedTimeline => Icon::NewNestedTimeline,
            B::NewAudioTrack => Icon::NewAudioTrack,
            B::ShowVolumeEnvelopes => Icon::VolumeEnvelope,
            B::ShowPanEnvelopes => Icon::PanEnvelope,
            B::InsertBus => Icon::Bus,
            B::InsertInputBus => Icon::InputBus,
            B::InsertAssignableFx => Icon::AssignableFx,
            B::BypassAllAudioFx => Icon::BypassFx,
            B::QuickFadeAudioEdits => Icon::QuickFade,
            B::InsertText => Icon::Text,
            B::NewVideoTrack => Icon::NewVideoTrack,
            B::NewAdjustmentTrack => Icon::AdjustmentTrack,
            B::MuteAllAudioTracks => Icon::MuteAudio,
            B::MuteAllVideoTracks => Icon::MuteVideo,
            B::ShowVideoEventEdgeFrames => Icon::EdgeFrames,
            B::ShowEventMediaMarkers => Icon::MediaMarkers,
            B::SnapToGrid => Icon::SnapGrid,
            B::SnapToMarkers => Icon::SnapMarkers,
            B::SnapToAllEvents => Icon::SnapEvents,
            B::QuantizeToFrames => Icon::Quantize,
            B::SlipAllTakes => Icon::SlipTakes,
            B::MotionTracking => Icon::MotionTracking,
            B::DetectScenesAndSplit => Icon::DetectScenes,
        })
    }

    fn does(self) -> Does {
        match self {
            B::Separator => Does::Own,
            B::NewEmptyProject | B::NewProject => Does::Cmd(Cmd::New),
            B::Open => Does::Cmd(Cmd::Open),
            B::Save => Does::Cmd(Cmd::Save),
            B::Render | B::ToolsRender => Does::Cmd(Cmd::Render),
            B::Properties => Does::Cmd(Cmd::Properties),
            B::Cut => Does::Cmd(Cmd::Cut),
            B::Copy => Does::Cmd(Cmd::Copy),
            B::Paste => Does::Cmd(Cmd::Paste),
            B::Undo => Does::Cmd(Cmd::Undo),
            B::Redo => Does::Cmd(Cmd::Redo),
            B::SaveAs => Does::Cmd(Cmd::SaveAs),
            B::IncrementalSave => Does::Cmd(Cmd::SaveIncremental),
            B::OpenInAudioEditor => Does::Cmd(Cmd::OpenInAudioEditor),
            B::ImportMedia => Does::Cmd(Cmd::ImportMedia),
            B::CreateSlideshow => Does::Cmd(Cmd::Slideshow),
            B::Explorer => Does::Cmd(Cmd::Window(Tab::Explorer)),
            B::WelcomeScreen => Does::Cmd(Cmd::Welcome),
            B::Script(i) => match SCRIPT_CMDS.get(i as usize).copied().flatten() {
                Some(c) => Does::Cmd(c),
                None => Does::Soon,
            },
            B::TriggerFromMidiTimecode => Does::Cmd(Cmd::TriggerMtc),
            B::GenerateMidiTimecode => Does::Cmd(Cmd::GenerateMtc),
            B::GenerateMidiClock => Does::Cmd(Cmd::GenerateClock),
            B::ImportDvdCamcorderDisc => Does::Cmd(Cmd::ImportDvdCamcorder),
            B::PrerenderVideo => Does::Cmd(Cmd::PrerenderVideo),
            B::AutoHideDockingArea => Does::Cmd(Cmd::AutoHideDock),
            B::StartCapture => Does::Own,
            B::ExtractAudioFromCd
            | B::TextToSpeech
            | B::SpeechToText
            | B::AiAssistant
            | B::MulticameraAudioSync
            | B::AiAutoReframe
            | B::HardwareDetection => Does::Soon,
            B::BurnDvd => Does::Cmd(Cmd::BurnDvd),
            B::BurnDiscAtOnceAudioCd => Does::Cmd(Cmd::BurnDaoCd),
            B::ApplyNonRealTimeEventFx => Does::Cmd(Cmd::NonRealTimeFx),
            B::RenderToNewTrack => Does::Cmd(Cmd::RenderToNewTrack),
            B::LoopPlayback => Does::Cmd(Cmd::LoopPlayback),
            B::Metronome => Does::Cmd(Cmd::Metronome),
            B::Record => Does::Cmd(Cmd::Record),
            B::PlayFromStart | B::Play | B::Pause | B::Stop => Does::Own,
            B::GoToStart | B::GoToEnd | B::PreviousFrame | B::NextFrame => Does::Own,
            B::NormalTool => Does::Cmd(Cmd::UseTool(Tool::Edit)),
            B::EnvelopeTool => Does::Cmd(Cmd::UseTool(Tool::Envelope)),
            B::SelectionTool => Does::Cmd(Cmd::UseTool(Tool::Select)),
            B::ZoomTool => Does::Cmd(Cmd::UseTool(Tool::Zoom)),
            B::Delete => Does::Cmd(Cmd::Delete),
            B::Trim => Does::Cmd(Cmd::Trim),
            B::TrimStart => Does::Cmd(Cmd::TrimStart),
            B::TrimEnd => Does::Cmd(Cmd::TrimEnd),
            B::Split => Does::Cmd(Cmd::Split),
            B::SmartSplit => Does::Cmd(Cmd::SmartSplit),
            B::LockEvent => Does::Cmd(Cmd::Lock),
            B::InsertMarker => Does::Cmd(Cmd::InsertMarker),
            B::InsertRegion => Does::Cmd(Cmd::InsertRegion),
            B::EnableSnapping => Does::Cmd(Cmd::Snapping),
            B::AutomaticCrossfades => Does::Cmd(Cmd::AutoCrossfade),
            B::AutoRipple => Does::Cmd(Cmd::AutoRipple),
            B::IgnoreEventGrouping => Does::Cmd(Cmd::IgnoreGrouping),
            B::ColorGrading => Does::Cmd(Cmd::Window(Tab::ColorGrading)),
            B::NewAudioTrack => Does::Cmd(Cmd::InsertAudioTrack),
            B::ShowVolumeEnvelopes => Does::Cmd(Cmd::ShowEnvelopeType(EnvelopeKind::Volume)),
            B::ShowPanEnvelopes => Does::Cmd(Cmd::ShowEnvelopeType(EnvelopeKind::Pan)),
            B::BypassAllAudioFx => Does::Cmd(Cmd::BypassAudioFx),
            B::QuickFadeAudioEdits => Does::Cmd(Cmd::QuickFades),
            B::NewVideoTrack => Does::Cmd(Cmd::InsertVideoTrack),
            B::MuteAllAudioTracks => Does::Cmd(Cmd::MuteAllAudio),
            B::MuteAllVideoTracks => Does::Cmd(Cmd::MuteAllVideo),
            B::ShowVideoEventEdgeFrames => Does::Cmd(Cmd::EdgeFrames),
            B::ShowEventMediaMarkers => Does::Cmd(Cmd::EventMediaMarkers),
            B::SnapToGrid => Does::Cmd(Cmd::SnapGrid),
            B::SnapToMarkers => Does::Cmd(Cmd::SnapMarkers),
            B::SnapToAllEvents => Does::Cmd(Cmd::SnapEvents),
            B::QuantizeToFrames => Does::Cmd(Cmd::Quantize),
            B::DetectScenesAndSplit => Does::Extra(crate::keymap::Extra::DetectScenes),
            B::LockEnvelopesToEvents => Does::Cmd(Cmd::LockEnvelopes),
            B::InsertText => Does::Cmd(Cmd::InsertTextMedia),
            B::NewAdjustmentTrack => Does::Cmd(Cmd::InsertAdjustmentTrack),
            B::InsertBus => Does::Cmd(Cmd::InsertAudioBus),
            B::InsertAssignableFx => Does::Cmd(Cmd::InsertAssignableFx),
            B::InsertInputBus => Does::Cmd(Cmd::InsertInputBus),
            B::MotionTracking => Does::Cmd(Cmd::MotionTracking),
            B::OpenParentTimeline => Does::Cmd(Cmd::OpenParentTimeline),
            B::OpenNestedTimeline => Does::Cmd(Cmd::OpenNestedTimeline),
            B::CreateNestedTimeline => Does::Cmd(Cmd::CreateNestedTimeline),
            B::SlipAllTakes => Does::Cmd(Cmd::SlipAllTakes),
        }
    }

    /// Whether BananaV has this button's action yet.
    pub fn available(self) -> bool {
        !matches!(self.does(), Does::Soon)
    }

    pub fn enabled(self, app: &BananaApp) -> bool {
        match self.does() {
            Does::Cmd(c) => c.enabled(app),
            Does::Extra(Extra::DetectScenes) => app.primary_clip().is_some(),
            Does::Soon => false,
            _ => true,
        }
    }

    /// `Some(on)` for buttons that stay pressed while their option is on.
    pub fn checked(self, app: &BananaApp) -> Option<bool> {
        match (self, self.does()) {
            (B::Play, _) => Some(app.is_playing()),
            (B::LoopPlayback, _) => Some(app.loop_enabled),
            (_, Does::Cmd(c)) => c.checked(app),
            _ => None,
        }
    }

    pub fn run(self, app: &mut BananaApp) {
        match self.does() {
            Does::Cmd(c) => {
                if c.enabled(app) {
                    c.run(app);
                }
            }
            Does::Extra(e) => app.run_extra(e),
            Does::Own => match self {
                B::PlayFromStart => app.play_from_start(),
                B::Play => app.toggle_play(),
                B::Pause => app.toggle_pause(),
                B::Stop => app.stop(),
                B::GoToStart => app.seek_playback(Tick::ZERO),
                B::GoToEnd => app.go_end(),
                B::PreviousFrame => app.step_frames(-1),
                B::NextFrame => app.step_frames(1),
                B::StartCapture => app.open_screen_capture(),
                _ => {}
            },
            Does::Soon => {}
        }
    }

    /// Tooltip: the name and the keys that run it ("Play (Space)").
    pub fn tooltip(self, app: &BananaApp) -> String {
        if !self.available() {
            return format!("{} — {}", self.label(), tr("Coming soon to BananaV"));
        }
        // Transport keys that live outside the keyboard map.
        let fixed: &[&str] = match self {
            B::Play => &["Space"],
            B::PlayFromStart => &["Shift+Space"],
            B::Pause => &["Enter"],
            _ => &[],
        };
        let mut keys: Vec<String> = fixed.iter().map(|k| tr(k).to_string()).collect();
        let action = match self.does() {
            Does::Cmd(c) => Some(Action::Cmd(c)),
            Does::Extra(e) => Some(Action::Extra(e)),
            Does::Own => match self {
                B::Stop => Some(Action::Extra(Extra::Stop)),
                B::PlayFromStart => Some(Action::Extra(Extra::PlayFromStart)),
                B::Pause => Some(Action::Extra(Extra::Pause)),
                B::GoToStart => Some(Action::Cmd(Cmd::ProjectStart)),
                B::GoToEnd => Some(Action::Cmd(Cmd::ProjectEnd)),
                B::PreviousFrame => Some(Action::Cmd(Cmd::FrameLeft)),
                B::NextFrame => Some(Action::Cmd(Cmd::FrameRight)),
                _ => None,
            },
            Does::Soon => None,
        };
        if let Some(a) = action {
            let text = app.keymap.text_for(a);
            if !text.is_empty() {
                keys.push(text);
            }
        }
        if keys.is_empty() {
            self.label().to_string()
        } else {
            format!("{} ({})", self.label(), keys.join("; "))
        }
    }
}

/// The toolbar in use: the user's list, or VEGAS's default.
pub fn current(app: &BananaApp, kind: ToolbarKind) -> Vec<TbButton> {
    let saved = match kind {
        ToolbarKind::Main => &app.settings.main_toolbar,
        ToolbarKind::Timeline => &app.settings.timeline_toolbar,
    };
    saved
        .clone()
        .unwrap_or_else(|| kind.default_buttons().to_vec())
}

/// Saves a toolbar's buttons (`None` when they are the default ones).
pub fn set_current(app: &mut BananaApp, kind: ToolbarKind, buttons: Vec<TbButton>) {
    let value = (buttons != kind.default_buttons()).then_some(buttons);
    match kind {
        ToolbarKind::Main => app.settings.main_toolbar = value,
        ToolbarKind::Timeline => app.settings.timeline_toolbar = value,
    }
    let _ = app.settings.save();
}

/// Draws a toolbar's buttons and runs the one clicked.
pub fn paint(ui: &mut crate::egui::Ui, app: &mut BananaApp, kind: ToolbarKind) {
    for b in current(app, kind) {
        let Some(icon) = b.icon() else {
            crate::widgets::toolbar_separator(ui);
            continue;
        };
        let on = b.checked(app).unwrap_or(false);
        let tip = b.tooltip(app);
        if crate::widgets::icon_button_ex(ui, icon, &tip, on, b.enabled(app)).clicked() {
            b.run(app);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_toolbar_lists_vegas_buttons() {
        let all = main_all();
        let mut seen = std::collections::HashSet::new();
        for b in &all {
            assert!(seen.insert(*b), "{b:?} twice");
            assert!(b.icon().is_some());
        }
        assert_eq!(all.len(), 48 + MAIN_DEFAULT.len());
        assert_eq!(B::Script(17).label(), "Stereo to Mono");
    }

    #[test]
    fn every_button_is_listed_once() {
        let mut seen = std::collections::HashSet::new();
        for b in ALL {
            assert!(seen.insert(*b), "{b:?} twice");
            assert!(b.icon().is_some());
        }
        for b in DEFAULT.iter().filter(|b| **b != B::Separator) {
            assert!(seen.contains(b), "{b:?} missing from ALL");
        }
        assert_eq!(DEFAULT.iter().filter(|b| **b != B::Separator).count(), 32);
    }
}
