//! Every main-menu item as a command: label, shortcut, state and action live
//! together, and the same table drives the keyboard. Labels are pt-BR for
//! now; this is the single place to translate them later.

use crate::i18n::tr;
use bv_timeline::{ChannelMap, EnvelopeKind, RippleScope, TrackKind};

use crate::app::actions::LayoutPreset;
use crate::app::more_actions::Script;
use crate::app::{BananaApp, Pending, Tool};
use crate::dock::Tab;
use crate::egui::{Key, Modifiers};
use crate::icons::Icon;
use crate::timefmt::{GridSpacing, RulerFormat, RulerFormatExt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    // File
    New,
    Welcome,
    Open,
    Close,
    Save,
    SaveAs,
    SaveIncremental,
    CopyMediaWithProject,
    Render,
    /// Tools › Render to New Track.
    RenderToNewTrack,
    /// Insert › Audio Envelopes › FX Automation: the FX Automation Chooser
    /// on the selected audio track.
    FxAutomation,
    /// File › Real-Time Render.
    RealTimeRender,
    ImportMedia,
    ImportImageSequence,
    ImportProjectMedia,
    ImportEdl,
    ImportOtio,
    ImportXml,
    ImportFcpxml,
    /// File › Import › Closed Captioning.
    ImportClosedCaptioning,
    ImportBwf,
    ExportProject,
    ExportArchive,
    ExportXml,
    ExportFcpxml,
    ExportEdl,
    ExportOtio,
    Properties,
    Exit,
    // Edit
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    PasteRepeat,
    PasteInsert,
    PasteAttributes,
    PasteAttributesSelective,
    Delete,
    Trim,
    TrimStart,
    TrimEnd,
    Split,
    SmartSplit,
    Rejoin,
    CloseGaps,
    // Edit › Navigate
    GoTo,
    ProjectStart,
    ProjectEnd,
    SelectionStartGo,
    SelectionEndGo,
    GridLeft,
    GridRight,
    PixelLeft,
    PixelRight,
    FrameLeft,
    FrameRight,
    MarkerLeft,
    MarkerRight,
    SelectToMarker,
    EditPointLeft,
    EditPointRight,
    SelectedEventLeft,
    SelectedEventRight,
    CursorToPlayhead,
    CenterOnCursor,
    // Edit › Post-Edit Ripple
    Ripple(RippleScope),
    // Edit › Select
    SelectAll,
    SelectAfterCursor,
    SelectEventStart,
    SelectEventEnd,
    /// Edit › Select › Enter Expanded Edit Mode / Exit Edit Mode.
    ExpandedEditMode,
    ExitEditMode,
    /// Num 5: into Expanded Edit Mode, or out of it (and out of
    /// edge-trimming).
    EditModeKey,
    /// The transport's Record: into the armed tracks.
    Record,
    /// Options › Metronome.
    Metronome,
    /// Keypad 0: plays around the edit cursor (Options › Preferences ›
    /// Editing: "Cursor preview duration").
    CursorPreview,
    /// Options › Timecode: Generate MIDI Timecode, Generate MIDI Clock,
    /// Trigger from MIDI Timecode.
    GenerateMtc,
    GenerateClock,
    TriggerMtc,
    /// Options › External Control: the control surfaces in use.
    ExternalControl,
    /// The toolbar's Auto Hide Docking Area (Options › Preferences ›
    /// Display › "Automatically hide docking area").
    AutoHideDock,
    /// File › Import › DVD Camcorder Disc and View › Window › Device
    /// Explorer.
    ImportDvdCamcorder,
    DeviceExplorer,
    /// The focused track's Automation Settings, and its Activate
    /// Automation Controls.
    AutomationMode(bv_timeline::AutomationMode),
    AutomationControls,
    /// Video Preview › Video Preview on External Monitor.
    ExternalMonitor,
    /// Arm for Record and Set Recording Path.
    SetPathAndRecord,
    /// Arm for Record on the selected audio track, or off.
    ArmTrack,
    LoopPlayback,
    SelectionToView,
    SelectionToProject,
    SelectLoopRegion,
    SetSelectionStart,
    SetSelectionEnd,
    SelectionPrevEvent,
    SelectionNextEvent,
    RestoreSelection,
    // Edit › Editing Tool
    UseTool(Tool),
    NextTool,
    PrevTool,
    // Edit › Switches / Group / Channels
    Mute,
    Lock,
    HoldLastFrame,
    Loop,
    InvertPhase,
    Normalize,
    MaintainAspect,
    AllFrames,
    AutoNormalize,
    ReduceFlicker,
    /// Edit › Switches' Resample mode (`None`: the project's).
    Resample(Option<bv_timeline::ResampleMode>),
    GroupNew,
    GroupRemove,
    GroupClear,
    GroupSelectAll,
    GroupCut,
    GroupCopy,
    GroupDelete,
    Channels(ChannelMap),
    UndoAll,
    ClearHistory,
    // View
    Toolbar,
    StatusBar,
    OrganizeLayouts,
    SaveLayout,
    SaveLayoutAs,
    ReloadLayout,
    Layout(LayoutPreset),
    LayoutSlot(usize),
    FocusTimeline,
    Window(Tab),
    ActiveTakeInfo,
    Waveforms,
    EventButtons,
    FadeDurations,
    EdgeFrames,
    EventHeaders,
    EventDuration,
    ShowEnvelopes,
    MinimizeTracks,
    /// View › Audio Bus Tracks: a track for each bus and chain at the
    /// timeline's bottom.
    AudioBusTracks,
    /// View › Video Bus Track: the video output's track at the timeline's
    /// bottom.
    VideoBusTrack,
    RebuildPeaks,
    /// Tools › Build Dynamic RAM Preview: the time selection's frames drawn
    /// into memory, then played.
    BuildRamPreview,
    ClearRamPreview,
    /// View › Event Media Markers › Show Markers / Show Markers and Labels.
    EventMediaMarkers,
    EventMediaMarkerLabels,
    /// Tools › Selectively Prerender Video: the parts that do not play in
    /// real time drawn into files.
    PrerenderVideo,
    /// Tools › Clean Up Prerendered Video.
    CleanUpPrerender,
    /// View › Audio/Video Envelopes: show this envelope type.
    ShowEnvelopeType(EnvelopeKind),
    /// View › Show Envelopes › Audio Envelopes › Surround Pan Keyframes.
    ShowSurroundKeyframes,
    /// View › Show OFX Effect Envelopes (Beta), View › Video Envelopes ›
    /// Track Keyframes.
    ShowOfxEnvelopes,
    ShowTrackKeyframes,
    // Insert
    AudioEnvelope(EnvelopeKind),
    /// Insert › Audio Envelopes › Surround Pan Keyframes (5.1 projects).
    SurroundKeyframes,
    VideoMuteEnvelope,
    /// Insert › Video Envelopes: Track Composite Level, Track Fade to Color.
    VideoEnvelope(EnvelopeKind),
    /// Reverse: the selected events play backward (or forward again).
    Reverse,
    /// Tools › Video › Track Motion: the current video track's.
    TrackMotion,
    /// Insert/Remove Envelope › Freeze Frame at Cursor.
    FreezeFrameAtCursor,
    /// Edit › Take: the selected events' next or previous take; the Take
    /// Chooser to choose or delete one; the active one deleted or renamed.
    TakeNext,
    TakePrevious,
    TakeChooseActive,
    TakeDelete,
    TakeDeleteActive,
    TakeRenameActive,
    /// Options › Slip All Takes.
    SlipAllTakes,
    /// Tools › Multicamera: Create Multicamera Track, Expand to Multiple
    /// Tracks…, Enable Multicamera Editing, Edit Multicamera Audio with
    /// Video.
    MulticamSyncAudio,
    MulticamSyncTimecode,
    MulticamSyncDate,
    MulticamCreate,
    MulticamExpand,
    MulticamEditing,
    MulticamAudioWithVideo,
    MulticamFullFrame,
    InsertAudioTrack,
    InsertVideoTrack,
    EmptyEvent,
    Slideshow,
    InsertTime,
    InsertMarker,
    InsertRegion,
    /// Insert › Audio CD Track Region, Audio CD Track Index, Command….
    InsertCdTrack,
    InsertCdIndex,
    InsertCommand,
    /// Tools › Lay Out Audio CD from Events.
    LayOutCd,
    /// Tools › Burn Disc › DVD….
    BurnDvd,
    /// Tools › Burn Disc › Track-at-Once and Disc-at-Once Audio CD….
    BurnTaoCd,
    BurnDaoCd,
    /// View › Mixer Preview Fader.
    PreviewFader,
    AddMissingStream,
    InsertTextMedia,
    SubtitlesFromFile,
    SubtitlesFromRegions,
    InsertAdjustmentEvent,
    InsertAdjustmentTrack,
    /// Insert › Audio Bus, Input Bus, Audio Assignable FX….
    InsertAudioBus,
    InsertInputBus,
    /// Tools › Video › Motion Tracking.
    MotionTracking,
    InsertAssignableFx,
    ResizeSelectedAdjustments,
    ResizeAllAdjustments,
    // Tools
    OpenInAudioEditor,
    OpenCopyInAudioEditor,
    EditSourceProject,
    CreateNestedTimeline,
    OpenNestedTimeline,
    OpenParentTimeline,
    AudioEventFx,
    NonRealTimeFx,
    AudioTrackFx,
    MediaFx,
    PanCropEvent,
    VideoEventFx,
    VideoTrackFx,
    VideoOutputFx,
    LoudnessLog,
    AdvancedSave,
    CleanProjectMedia,
    RunScript(Script),
    // Options
    Quantize,
    ShowUnquantized,
    Snapping,
    SnapGrid,
    SnapMarkers,
    SnapEvents,
    Grid(GridSpacing),
    Ruler(RulerFormat),
    SetTimeAtCursor,
    SetProjectTime,
    SetProjectTempo,
    AutoCrossfade,
    QuickFades,
    AutoRipple,
    RippleMode(RippleScope),
    IgnoreGrouping,
    LockEnvelopes,
    BypassAudioFx,
    MuteAllAudio,
    MuteAllVideo,
    Downmix,
    Dim,
    ExportPreferences,
    ImportPreferences,
    Preferences,
    CustomizeKeyboard,
    CustomizeToolbar,
    CustomizeTimelineToolbar,
    // Help
    HelpContents,
    KeyboardShortcuts,
    About,
}

const NONE: Modifiers = Modifiers::NONE;
const CMD: Modifiers = Modifiers::COMMAND;
const SHIFT: Modifiers = Modifiers::SHIFT;
const ALT: Modifiers = Modifiers::ALT;
const CMD_SHIFT: Modifiers = Modifiers {
    shift: true,
    ..Modifiers::COMMAND
};
const ALT_SHIFT: Modifiers = Modifiers {
    shift: true,
    ..Modifiers::ALT
};
const CMD_ALT: Modifiers = Modifiers {
    alt: true,
    ..Modifiers::COMMAND
};
const CMD_ALT_SHIFT: Modifiers = Modifiers {
    alt: true,
    shift: true,
    ..Modifiers::COMMAND
};

type Keys = &'static [(Modifiers, Key)];

/// Every command, for the shortcut table.
pub(crate) fn all() -> Vec<Cmd> {
    use Cmd::*;
    let mut v = vec![
        New,
        Welcome,
        Open,
        Close,
        Save,
        SaveAs,
        SaveIncremental,
        CopyMediaWithProject,
        Render,
        RenderToNewTrack,
        RealTimeRender,
        FxAutomation,
        ImportMedia,
        ImportImageSequence,
        ImportProjectMedia,
        ImportEdl,
        ImportOtio,
        ImportXml,
        ImportFcpxml,
        ImportClosedCaptioning,
        ImportBwf,
        ExportProject,
        ExportArchive,
        ExportXml,
        ExportFcpxml,
        ExportEdl,
        ExportOtio,
        Properties,
        Exit,
        Undo,
        Redo,
        Cut,
        Copy,
        Paste,
        PasteRepeat,
        PasteInsert,
        PasteAttributes,
        PasteAttributesSelective,
        Delete,
        Trim,
        TrimStart,
        TrimEnd,
        Split,
        SmartSplit,
        Rejoin,
        CloseGaps,
        GoTo,
        ProjectStart,
        ProjectEnd,
        SelectionStartGo,
        SelectionEndGo,
        GridLeft,
        GridRight,
        PixelLeft,
        PixelRight,
        FrameLeft,
        FrameRight,
        MarkerLeft,
        MarkerRight,
        SelectToMarker,
        EditPointLeft,
        EditPointRight,
        SelectedEventLeft,
        SelectedEventRight,
        CursorToPlayhead,
        CenterOnCursor,
        Ripple(RippleScope::AffectedTracks),
        Ripple(RippleScope::AffectedTracksAndMarkers),
        Ripple(RippleScope::All),
        SelectAll,
        SelectAfterCursor,
        SelectEventStart,
        SelectEventEnd,
        ExpandedEditMode,
        ExitEditMode,
        EditModeKey,
        Record,
        Metronome,
        CursorPreview,
        GenerateMtc,
        GenerateClock,
        TriggerMtc,
        ExternalControl,
        AutoHideDock,
        ImportDvdCamcorder,
        DeviceExplorer,
        AutomationMode(bv_timeline::AutomationMode::Off),
        AutomationMode(bv_timeline::AutomationMode::Read),
        AutomationMode(bv_timeline::AutomationMode::Touch),
        AutomationMode(bv_timeline::AutomationMode::Latch),
        AutomationControls,
        ExternalMonitor,
        SetPathAndRecord,
        ArmTrack,
        LoopPlayback,
        SelectionToView,
        SelectionToProject,
        SelectLoopRegion,
        SetSelectionStart,
        SetSelectionEnd,
        SelectionPrevEvent,
        SelectionNextEvent,
        RestoreSelection,
        NextTool,
        PrevTool,
        Mute,
        Lock,
        HoldLastFrame,
        Loop,
        InvertPhase,
        Normalize,
        MaintainAspect,
        AllFrames,
        AutoNormalize,
        ReduceFlicker,
        Resample(None),
        Resample(Some(bv_timeline::ResampleMode::FrameBlend)),
        Resample(Some(bv_timeline::ResampleMode::OpticalFlow)),
        Resample(Some(bv_timeline::ResampleMode::Disable)),
        GroupNew,
        GroupRemove,
        GroupClear,
        GroupSelectAll,
        GroupCut,
        GroupCopy,
        GroupDelete,
        UndoAll,
        ClearHistory,
        Toolbar,
        StatusBar,
        OrganizeLayouts,
        SaveLayout,
        SaveLayoutAs,
        ReloadLayout,
        FocusTimeline,
        ActiveTakeInfo,
        Waveforms,
        EventButtons,
        FadeDurations,
        EdgeFrames,
        EventHeaders,
        EventDuration,
        ShowEnvelopes,
        MinimizeTracks,
        AudioBusTracks,
        VideoBusTrack,
        RebuildPeaks,
        BuildRamPreview,
        ClearRamPreview,
        PrerenderVideo,
        CleanUpPrerender,
        EventMediaMarkers,
        EventMediaMarkerLabels,
    ];
    v.extend([
        VideoMuteEnvelope,
        InsertAudioTrack,
        InsertVideoTrack,
        EmptyEvent,
        Slideshow,
        InsertTime,
        InsertMarker,
        InsertRegion,
        InsertCdTrack,
        InsertCdIndex,
        InsertCommand,
        LayOutCd,
        BurnDvd,
        BurnTaoCd,
        BurnDaoCd,
        PreviewFader,
        OpenInAudioEditor,
        OpenCopyInAudioEditor,
        EditSourceProject,
        CreateNestedTimeline,
        OpenNestedTimeline,
        OpenParentTimeline,
        AudioEventFx,
        NonRealTimeFx,
        AudioTrackFx,
        MediaFx,
        PanCropEvent,
        VideoEventFx,
        VideoTrackFx,
        VideoOutputFx,
        LoudnessLog,
        AdvancedSave,
        CleanProjectMedia,
        Quantize,
        ShowUnquantized,
        Snapping,
        SnapGrid,
        SnapMarkers,
        SnapEvents,
        SetTimeAtCursor,
        SetProjectTime,
        SetProjectTempo,
        AutoCrossfade,
        QuickFades,
        AutoRipple,
        IgnoreGrouping,
        LockEnvelopes,
        BypassAudioFx,
        MuteAllAudio,
        MuteAllVideo,
        Downmix,
        Dim,
        AddMissingStream,
        InsertTextMedia,
        SubtitlesFromFile,
        SubtitlesFromRegions,
        InsertAdjustmentEvent,
        InsertAdjustmentTrack,
        InsertAudioBus,
        InsertInputBus,
        MotionTracking,
        InsertAssignableFx,
        ResizeSelectedAdjustments,
        ResizeAllAdjustments,
        ExportPreferences,
        ImportPreferences,
        Preferences,
        CustomizeKeyboard,
        CustomizeToolbar,
        CustomizeTimelineToolbar,
        HelpContents,
        KeyboardShortcuts,
        About,
    ]);
    v.extend(EnvelopeKind::AUDIO.map(AudioEnvelope));
    v.extend(EnvelopeKind::AUDIO.map(ShowEnvelopeType));
    v.extend([SurroundKeyframes, ShowSurroundKeyframes]);
    v.extend([ShowOfxEnvelopes, ShowTrackKeyframes]);
    v.extend([
        VideoEnvelope(EnvelopeKind::CompositeLevel),
        VideoEnvelope(EnvelopeKind::FadeToColor),
        VideoEnvelope(EnvelopeKind::Velocity),
        VideoEnvelope(EnvelopeKind::TransitionProgress),
        ShowEnvelopeType(EnvelopeKind::CompositeLevel),
        ShowEnvelopeType(EnvelopeKind::FadeToColor),
        ShowEnvelopeType(EnvelopeKind::Velocity),
        ShowEnvelopeType(EnvelopeKind::TransitionProgress),
        ShowEnvelopeType(EnvelopeKind::MotionBlur),
        ShowEnvelopeType(EnvelopeKind::Supersampling),
        Reverse,
        FreezeFrameAtCursor,
        TrackMotion,
        TakeNext,
        TakePrevious,
        TakeChooseActive,
        TakeDelete,
        TakeDeleteActive,
        TakeRenameActive,
        SlipAllTakes,
        MulticamSyncAudio,
        MulticamSyncTimecode,
        MulticamSyncDate,
        MulticamCreate,
        MulticamExpand,
        MulticamEditing,
        MulticamAudioWithVideo,
        MulticamFullFrame,
    ]);
    v.extend(GridSpacing::ALL.map(Grid));
    v.extend(RulerFormat::ALL.map(Ruler));
    v.extend(
        [
            RippleScope::AffectedTracks,
            RippleScope::AffectedTracksAndMarkers,
            RippleScope::All,
        ]
        .map(RippleMode),
    );
    v.extend(
        [
            Script::ExportChapters,
            Script::ExportCaptionsSubRip,
            Script::ExportCaptionsSami,
            Script::ExportCaptionsQuickTime,
            Script::ExportCaptionsDvd,
            Script::ExportRegionsAsSubtitles,
            Script::GroupVideoAndAudio,
            Script::PromoteMediaCaptions,
            Script::RemoveLetterboxing,
            Script::StereoToMono,
        ]
        .map(RunScript),
    );
    v.extend(Tool::CYCLE.map(UseTool));
    v.push(UseTool(Tool::Shuffle));
    v.extend(Tab::ALL.map(Window));
    v.extend(
        [
            ChannelMap::Both,
            ChannelMap::LeftOnly,
            ChannelMap::RightOnly,
            ChannelMap::Combine,
            ChannelMap::Swap,
        ]
        .map(Channels),
    );
    v
}

impl Cmd {
    pub fn label(self, app: &BananaApp) -> String {
        use Cmd::*;
        let s: &str = match self {
            New => tr("New…"),
            Welcome => tr("Welcome Screen"),
            Open => tr("Open…"),
            Close => tr("Close"),
            Save => tr("Save"),
            SaveAs => tr("Save As…"),
            SaveIncremental => tr("Incremental Save"),
            Render => tr("Render…"),
            RenderToNewTrack => tr("Render to New Track…"),
            RealTimeRender => tr("Real-Time Render…"),
            ImportMedia => tr("Media…"),
            ImportImageSequence => tr("Image Sequence…"),
            ImportProjectMedia => tr("Project Media…"),
            ImportEdl => tr("EDL Text File (*.edl, *.txt)…"),
            ImportOtio => "OpenTimelineIO (*.otio, *.otioz)…",
            ImportXml => "Final Cut Pro 7/DaVinci Resolve (*.xml)…",
            ImportClosedCaptioning => tr("Closed Captioning…"),
            ImportFcpxml => "Final Cut Pro X (*.fcpxml)…",
            ImportBwf => tr("Broadcast Wave Format…"),
            ExportProject => tr("BananaV Project File (*.bvproj)…"),
            ExportArchive => tr("Project Archive…"),
            CopyMediaWithProject => tr("Copy Media with Project"),
            ExportXml => "Final Cut Pro 7/DaVinci Resolve (*.xml)…",
            ExportFcpxml => "Final Cut Pro X (*.fcpxml)…",
            ExportEdl => tr("EDL Text File (*.edl)…"),
            ExportOtio => "OpenTimelineIO (*.otio, *.otioz, *.otiod)…",
            Properties => tr("Properties…"),
            Exit => tr("Exit"),
            Undo => {
                return match app.undo_label() {
                    Some(l) => trf!("Undo {}", l),
                    None => tr("Undo").into(),
                };
            }
            Redo => {
                return match app.redo_label() {
                    Some(l) => trf!("Redo {}", l),
                    None => tr("Redo").into(),
                };
            }
            Cut => tr("Cut"),
            Copy => tr("Copy"),
            Paste => tr("Paste"),
            PasteRepeat => tr("Paste Repeat…"),
            PasteInsert => tr("Paste Insert"),
            PasteAttributes => tr("Paste Event Attributes"),
            PasteAttributesSelective => tr("Selectively Paste Event Attributes…"),
            Delete => tr("Delete"),
            Trim => tr("Trim"),
            TrimStart => tr("Trim Start"),
            TrimEnd => tr("Trim End"),
            Split => tr("Split"),
            SmartSplit => tr("Smart Split"),
            Rejoin => tr("Rejoin Events"),
            CloseGaps => tr("Close Gaps"),
            GoTo => tr("Go To…"),
            ProjectStart => tr("Go to Start"),
            ProjectEnd => tr("Go to End"),
            SelectionStartGo => tr("Go to Start of Selection/View"),
            SelectionEndGo => tr("Go to End of Selection/View"),
            GridLeft => tr("Move Left by Grid Marks"),
            GridRight => tr("Move Right by Grid Marks"),
            PixelLeft => tr("Move Left by Pixel"),
            PixelRight => tr("Move Right by Pixel"),
            FrameLeft => tr("Move Left by Frame"),
            FrameRight => tr("Move Right by Frame"),
            MarkerLeft => tr("Move Left by Marker/Region"),
            MarkerRight => tr("Move Right by Marker/Region"),
            SelectToMarker => tr("Select to Next Marker/Region"),
            EditPointLeft => tr("Move Left by Event Edit Points"),
            EditPointRight => tr("Move Right by Event Edit Points"),
            SelectedEventLeft => tr("Move Left by Selected Event"),
            SelectedEventRight => tr("Move Right by Selected Event"),
            CursorToPlayhead => tr("Move Cursor to Playback Position"),
            CenterOnCursor => tr("Center View Around Cursor"),
            Ripple(RippleScope::AffectedTracks) => tr("Affected Tracks"),
            Ripple(RippleScope::AffectedTracksAndMarkers) => {
                tr("Affected Tracks, Markers, and Regions")
            }
            Ripple(RippleScope::All) => tr("All Tracks, Markers, and Regions"),
            SelectAll => tr("Select All"),
            SelectAfterCursor => tr("Select All After Cursor"),
            SelectEventStart => tr("Select Event Start"),
            SelectEventEnd => tr("Select Event End"),
            ExpandedEditMode => tr("Enter Expanded Edit Mode"),
            ExitEditMode => tr("Exit Edit Mode"),
            EditModeKey => tr("Enter or Exit Expanded Edit Mode"),
            Record => tr("Record"),
            Metronome => tr("Metronome"),
            CursorPreview => tr("Cursor Preview"),
            GenerateMtc => tr("Generate MIDI Timecode"),
            GenerateClock => tr("Generate MIDI Clock"),
            TriggerMtc => tr("Trigger from MIDI Timecode"),
            ExternalControl => tr("External Control"),
            AutoHideDock => tr("Auto Hide Docking Area"),
            ImportDvdCamcorder => tr("DVD Camcorder Disc…"),
            DeviceExplorer => tr("Device Explorer"),
            AutomationMode(m) => automation_mode_name(m),
            AutomationControls => tr("Activate Automation Controls"),
            ExternalMonitor => tr("Video Preview on External Monitor"),
            SetPathAndRecord => tr("Set Recording Path and Record…"),
            ArmTrack => tr("Arm Track for Record"),
            LoopPlayback => tr("Loop Playback"),
            SelectionToView => tr("Set Selection to View"),
            SelectionToProject => tr("Set Selection to Project"),
            SelectLoopRegion => tr("Select Events in Loop Region"),
            SetSelectionStart => tr("Set Selection Start"),
            SetSelectionEnd => tr("Set Selection End"),
            SelectionPrevEvent => tr("Set Selection to Previous Event"),
            SelectionNextEvent => tr("Set Selection to Next Event"),
            RestoreSelection => tr("Restore Previous Time Selection"),
            UseTool(t) => t.label(),
            NextTool => tr("Next Tool"),
            PrevTool => tr("Previous Tool"),
            Mute => tr("Mute"),
            Lock => tr("Lock"),
            HoldLastFrame => tr("Hold Last Frame"),
            Loop => tr("Loop"),
            InvertPhase => tr("Invert Phase"),
            Normalize => tr("Normalize"),
            MaintainAspect => tr("Maintain Aspect Ratio"),
            AllFrames => tr("Trim Event to Include All Frames"),
            AutoNormalize => "Auto Normalize",
            ReduceFlicker => tr("Reduce Interlace Flicker"),
            Resample(mode) => match mode {
                None => tr("Use Project Resample Mode"),
                Some(bv_timeline::ResampleMode::FrameBlend) => tr("Frame Blend"),
                Some(bv_timeline::ResampleMode::OpticalFlow) => tr("Optical Flow"),
                Some(bv_timeline::ResampleMode::Disable) => tr("Disable Resample"),
            },
            AddMissingStream => tr("Add Missing Stream to Selected Event"),
            InsertTextMedia => tr("Text Media…"),
            SubtitlesFromFile => tr("Insert Subtitles from File"),
            SubtitlesFromRegions => tr("Insert Subtitles from Regions"),
            InsertAdjustmentEvent => tr("Adjustment Event"),
            InsertAdjustmentTrack => tr("Adjustment Track"),
            InsertAudioBus => tr("Audio Bus"),
            InsertInputBus => tr("Input Bus"),
            MotionTracking => tr("Motion Tracking"),
            InsertAssignableFx => tr("Audio Assignable FX…"),
            ResizeSelectedAdjustments => tr("Resize Selected Adjustment Events to Project"),
            ResizeAllAdjustments => tr("Resize All Adjustment Events to Project"),
            LockEnvelopes => tr("Lock Envelopes to Events"),
            Downmix => tr("Downmix Output"),
            Dim => tr("Dim Output"),
            GroupNew => tr("Create New"),
            GroupRemove => tr("Remove From"),
            GroupClear => tr("Clear"),
            GroupSelectAll => tr("Select All in Group"),
            GroupCut => tr("Cut All"),
            GroupCopy => tr("Copy All"),
            GroupDelete => tr("Delete All"),
            Channels(ChannelMap::Both) => tr("Both"),
            Channels(ChannelMap::LeftOnly) => tr("Left Only"),
            Channels(ChannelMap::RightOnly) => tr("Right Only"),
            Channels(ChannelMap::Combine) => tr("Combine"),
            Channels(ChannelMap::Swap) => tr("Swap"),
            UndoAll => tr("Undo All"),
            ClearHistory => tr("Clear Edit History…"),
            Toolbar => tr("Toolbar"),
            StatusBar => tr("Status Bar"),
            OrganizeLayouts => tr("Organize Layouts…"),
            SaveLayout => tr("Save Layout"),
            SaveLayoutAs => tr("Save Layout As…"),
            ReloadLayout => tr("Reload Selected Layout"),
            Layout(p) => p.label(),
            LayoutSlot(n) => {
                return match app.settings.layouts.get(n).and_then(|l| l.as_ref()) {
                    Some(l) => format!("{n} {}", l.name),
                    None => trf!("{} <EMPTY>", n),
                };
            }
            FocusTimeline => tr("Focus to Timeline"),
            Window(tab) => tab.title(),
            ActiveTakeInfo => tr("Active Take Information"),
            Waveforms => tr("Audio Event Waveforms"),
            EventButtons => tr("Event Buttons"),
            FadeDurations => tr("Event Fade Lengths"),
            EdgeFrames => tr("Video Event Frames"),
            EventHeaders => tr("Event Headers"),
            EventDuration => tr("Event Length"),
            ShowEnvelopes => tr("Show Envelopes"),
            MinimizeTracks => tr("Minimize All Tracks"),
            AudioBusTracks => tr("Audio Bus Tracks"),
            VideoBusTrack => tr("Video Bus Track"),
            RebuildPeaks => tr("Rebuild Audio Peaks"),
            BuildRamPreview => tr("Build Dynamic RAM Preview"),
            ClearRamPreview => tr("Clear Dynamic RAM Preview"),
            EventMediaMarkers => tr("Show Markers"),
            EventMediaMarkerLabels => tr("Show Markers and Labels"),
            PrerenderVideo => tr("Selectively Prerender Video…"),
            CleanUpPrerender => tr("Clean Up Prerendered Video"),
            ShowEnvelopeType(EnvelopeKind::Mute) => tr("Track Mute"),
            ShowEnvelopeType(EnvelopeKind::Volume) => tr("Track Volume"),
            ShowEnvelopeType(EnvelopeKind::Pan) => tr("Pan"),
            SurroundKeyframes | ShowSurroundKeyframes => tr("Surround Pan Keyframes"),
            ShowOfxEnvelopes => tr("Show OFX Effect Envelopes (Beta)"),
            ShowTrackKeyframes => tr("Track Keyframes"),
            AudioEnvelope(EnvelopeKind::Mute) => tr("Mute"),
            AudioEnvelope(EnvelopeKind::Volume) => "Volume",
            AudioEnvelope(EnvelopeKind::Pan) => tr("Pan"),
            // Each send's own (a track's menu names its bus or chain).
            ShowEnvelopeType(EnvelopeKind::Send(_)) | AudioEnvelope(EnvelopeKind::Send(_)) => {
                tr("Sends")
            }
            ShowEnvelopeType(EnvelopeKind::CompositeLevel)
            | AudioEnvelope(EnvelopeKind::CompositeLevel)
            | VideoEnvelope(EnvelopeKind::CompositeLevel) => tr("Track Composite Level"),
            ShowEnvelopeType(EnvelopeKind::FadeToColor)
            | AudioEnvelope(EnvelopeKind::FadeToColor)
            | VideoEnvelope(EnvelopeKind::FadeToColor) => tr("Track Fade to Color"),
            ShowEnvelopeType(EnvelopeKind::Velocity)
            | AudioEnvelope(EnvelopeKind::Velocity)
            | VideoEnvelope(EnvelopeKind::Velocity) => tr("Event Velocity"),
            ShowEnvelopeType(EnvelopeKind::TransitionProgress)
            | AudioEnvelope(EnvelopeKind::TransitionProgress)
            | VideoEnvelope(EnvelopeKind::TransitionProgress) => tr("Transition Progress"),
            ShowEnvelopeType(EnvelopeKind::MotionBlur)
            | AudioEnvelope(EnvelopeKind::MotionBlur) => tr("Motion Blur Amount"),
            ShowEnvelopeType(EnvelopeKind::Supersampling)
            | AudioEnvelope(EnvelopeKind::Supersampling) => tr("Video Supersampling"),
            ShowEnvelopeType(EnvelopeKind::FxParam { .. } | EnvelopeKind::FxBypass(_))
            | AudioEnvelope(EnvelopeKind::FxParam { .. } | EnvelopeKind::FxBypass(_)) => {
                tr("FX Automation Envelopes")
            }
            FxAutomation => tr("FX Automation…"),
            VideoEnvelope(_) => tr("Track Mute"),
            VideoMuteEnvelope => tr("Track Mute"),
            Reverse => tr("Reverse"),
            FreezeFrameAtCursor => tr("Freeze Frame at Cursor"),
            TakeNext => tr("Next Take"),
            TakePrevious => tr("Previous Take"),
            TakeChooseActive => tr("Choose Active…"),
            TakeDelete => tr("Delete…"),
            TakeDeleteActive => tr("Delete Active"),
            TakeRenameActive => tr("Rename Active"),
            SlipAllTakes => tr("Slip All Takes"),
            MulticamSyncAudio => tr("Sync Mixed Audio to Align Events"),
            MulticamSyncTimecode => tr("Sync Media Timecode to Align Events"),
            MulticamSyncDate => tr("Sync Date/Timestamp to Align Events"),
            MulticamFullFrame => tr("Show Full Frame on External Monitor"),
            MulticamCreate => tr("Create Multicamera Track"),
            MulticamExpand => tr("Expand to Multiple Tracks…"),
            MulticamEditing => tr("Enable Multicamera Editing"),
            MulticamAudioWithVideo => tr("Edit Multicamera Audio with Video"),
            TrackMotion => tr("Track Motion…"),
            InsertAudioTrack => tr("Audio Track"),
            InsertVideoTrack => tr("Video Track"),
            EmptyEvent => tr("Empty Event"),
            Slideshow => tr("Slideshow…"),
            InsertTime => tr("Time…"),
            InsertMarker => tr("Marker"),
            InsertRegion => tr("Region"),
            InsertCdTrack => tr("Audio CD Track Region"),
            InsertCdIndex => tr("Audio CD Track Index"),
            InsertCommand => tr("Command…"),
            LayOutCd => tr("Lay Out Audio CD from Events"),
            BurnDvd => "DVD…",
            BurnTaoCd => tr("Track-at-Once Audio CD…"),
            BurnDaoCd => tr("Disc-at-Once Audio CD…"),
            PreviewFader => tr("Mixer Preview Fader"),
            OpenInAudioEditor => tr("Open in Audio Editor"),
            OpenCopyInAudioEditor => tr("Open Copy in Audio Editor"),
            EditSourceProject => tr("Edit Source Project"),
            CreateNestedTimeline => tr("Create Nested Timeline"),
            OpenNestedTimeline => tr("Open Nested Timeline"),
            OpenParentTimeline => tr("Open Parent Timeline"),
            AudioEventFx => tr("Audio Event FX…"),
            NonRealTimeFx => tr("Apply Non-Real-Time Event FX…"),
            AudioTrackFx => tr("Track FX…"),
            MediaFx => tr("Media FX…"),
            PanCropEvent => tr("Video Event Pan/Crop…"),
            VideoEventFx => tr("Video Event FX…"),
            VideoTrackFx => tr("Video Track FX…"),
            VideoOutputFx => tr("Video Output FX…"),
            LoudnessLog => tr("Generate Loudness Log"),
            AdvancedSave => tr("Advanced Save…"),
            CleanProjectMedia => tr("Clean Project Media"),
            RunScript(s) => s.label(),
            Quantize => tr("Quantize to Frames"),
            ShowUnquantized => tr("Show Unquantized Event Edges"),
            Snapping => tr("Enable Snapping"),
            SnapGrid => tr("Snap to Grid"),
            SnapMarkers => tr("Snap to Markers"),
            SnapEvents => tr("Snap to All Events"),
            Grid(g) => g.label(),
            Ruler(r) => r.label(),
            SetTimeAtCursor => tr("Set Time at Cursor…"),
            SetProjectTime => tr("Set Project Time…"),
            SetProjectTempo => tr("Set Project Tempo…"),
            AutoCrossfade => tr("Automatic Crossfades"),
            QuickFades => tr("Quick Fade Audio Edits"),
            AutoRipple => tr("Auto Ripple"),
            RippleMode(RippleScope::AffectedTracks) => tr("Affected Tracks"),
            RippleMode(RippleScope::AffectedTracksAndMarkers) => {
                tr("Affected Tracks, Markers, and Regions")
            }
            RippleMode(RippleScope::All) => tr("All Tracks, Markers, and Regions"),
            IgnoreGrouping => tr("Ignore Event Grouping"),
            BypassAudioFx => tr("Bypass All Audio FX"),
            MuteAllAudio => tr("Mute All Audio"),
            MuteAllVideo => tr("Mute All Video"),
            ExportPreferences => tr("Export Preferences…"),
            ImportPreferences => tr("Import Preferences…"),
            Preferences => tr("Preferences…"),
            CustomizeKeyboard => tr("Customize Keyboard…"),
            CustomizeToolbar => tr("Customize Toolbar…"),
            CustomizeTimelineToolbar => tr("Customize Timeline Toolbar…"),
            HelpContents => tr("Contents and Index"),
            KeyboardShortcuts => tr("Keyboard Shortcuts"),
            About => tr("About"),
        };
        s.to_string()
    }

    /// Keyboard shortcuts; the first one is shown in the menu.
    pub fn keys(self) -> Keys {
        use Cmd::*;
        match self {
            New => &[(CMD, Key::N)],
            Open => &[(CMD, Key::O)],
            Close => &[(CMD, Key::F4)],
            Save => &[(CMD, Key::S)],
            SaveAs => &[(CMD_SHIFT, Key::S)],
            SaveIncremental => &[(CMD_ALT, Key::S)],
            Render => &[],
            Record => &[(CMD, Key::R)],
            SetPathAndRecord => &[(CMD_ALT_SHIFT, Key::R)],
            ArmTrack => &[(CMD_ALT, Key::R)],
            ImportMedia => &[(CMD, Key::I)],
            Properties => &[(ALT, Key::Enter)],
            Exit => &[(ALT, Key::F4)],
            Undo => &[(CMD, Key::Z)],
            Redo => &[(CMD, Key::Y), (CMD_SHIFT, Key::Z)],
            Cut => &[(CMD, Key::X)],
            Copy => &[(CMD, Key::C)],
            Paste => &[(CMD, Key::V)],
            PasteRepeat => &[(CMD, Key::B)],
            PasteInsert => &[(CMD_SHIFT, Key::V)],
            PasteAttributesSelective => &[(CMD_ALT_SHIFT, Key::V)],
            Delete => &[(NONE, Key::Delete)],
            Trim => &[(CMD, Key::T)],
            TrimStart => &[(ALT, Key::OpenBracket)],
            TrimEnd => &[(ALT, Key::CloseBracket)],
            Split => &[(NONE, Key::S)],
            TakeNext => &[(NONE, Key::T)],
            TakePrevious => &[(SHIFT, Key::T)],
            TakeRenameActive => &[(NONE, Key::F2)],
            MulticamEditing => &[(CMD_SHIFT, Key::D)],
            SmartSplit => &[(ALT, Key::S)],
            Rejoin => &[(NONE, Key::H)],
            GoTo => &[(CMD, Key::G)],
            ProjectStart => &[(CMD, Key::Home)],
            ProjectEnd => &[(CMD, Key::End)],
            SelectionStartGo => &[(NONE, Key::Home)],
            SelectionEndGo => &[(NONE, Key::End)],
            GridLeft => &[(NONE, Key::PageUp)],
            GridRight => &[(NONE, Key::PageDown)],
            PixelLeft => &[(NONE, Key::ArrowLeft)],
            PixelRight => &[(NONE, Key::ArrowRight)],
            FrameLeft => &[(ALT, Key::ArrowLeft)],
            FrameRight => &[(ALT, Key::ArrowRight)],
            MarkerLeft => &[(CMD, Key::ArrowLeft)],
            MarkerRight => &[(CMD, Key::ArrowRight)],
            SelectToMarker => &[(CMD_SHIFT, Key::ArrowRight)],
            EditPointLeft => &[(CMD_ALT, Key::ArrowLeft)],
            EditPointRight => &[(CMD_ALT, Key::ArrowRight)],
            SelectedEventLeft => &[(CMD, Key::OpenBracket)],
            SelectedEventRight => &[(CMD, Key::CloseBracket)],
            CursorToPlayhead => &[(ALT, Key::ArrowDown)],
            CenterOnCursor => &[(NONE, Key::Backslash)],
            Ripple(RippleScope::AffectedTracks) => &[(NONE, Key::F)],
            Ripple(RippleScope::AffectedTracksAndMarkers) => &[(CMD, Key::F)],
            Ripple(RippleScope::All) => &[(CMD_SHIFT, Key::F)],
            SelectAll => &[(CMD, Key::A)],
            LoopPlayback => &[(CMD_SHIFT, Key::L), (NONE, Key::Q)],
            SelectLoopRegion => &[(SHIFT, Key::L), (SHIFT, Key::Q)],
            SetSelectionStart => &[(NONE, Key::I)],
            SetSelectionEnd => &[(NONE, Key::O)],
            SelectionPrevEvent => &[(CMD_SHIFT, Key::OpenBracket)],
            SelectionNextEvent => &[(CMD_SHIFT, Key::CloseBracket)],
            RestoreSelection => &[(NONE, Key::Backspace)],
            NextTool => &[(NONE, Key::D)],
            PrevTool => &[(SHIFT, Key::D)],
            GroupNew => &[(NONE, Key::G)],
            GroupRemove => &[(NONE, Key::U)],
            GroupClear => &[(CMD, Key::U)],
            GroupSelectAll => &[(SHIFT, Key::G)],
            FocusTimeline => &[(ALT, Key::Num0)],
            Window(Tab::Explorer) => &[(ALT, Key::Num1)],
            Window(Tab::Trimmer) => &[(ALT, Key::Num2)],
            Window(Tab::Master) => &[(ALT, Key::Num3)],
            Window(Tab::Preview) => &[(ALT, Key::Num4)],
            Window(Tab::ProjectMedia) => &[(ALT, Key::Num5)],
            Window(Tab::Notes) => &[(ALT, Key::Num6)],
            Window(Tab::Transitions) => &[(ALT, Key::Num7)],
            Window(Tab::VideoFx) => &[(ALT, Key::Num8)],
            Window(Tab::Generators) => &[(ALT, Key::Num9)],
            Window(Tab::PluginManager) => &[(CMD_ALT, Key::Num1)],
            Window(Tab::Scopes) => &[(CMD_ALT, Key::Num2)],
            Window(Tab::SurroundPanner) => &[(CMD_ALT, Key::Num3)],
            Window(Tab::EditDetails) => &[(CMD_ALT, Key::Num4)],
            Window(Tab::LoudnessMeters) => &[(CMD_ALT, Key::Num7)],
            Window(Tab::MixingConsole) => &[(CMD_ALT, Key::Num5)],
            Window(Tab::ColorGrading) => &[(ALT, Key::G)],
            CreateNestedTimeline => &[(ALT, Key::C)],
            OpenNestedTimeline => &[(ALT, Key::N)],
            OpenParentTimeline => &[(ALT, Key::P)],
            ActiveTakeInfo => &[(CMD_SHIFT, Key::I)],
            Waveforms => &[(CMD_SHIFT, Key::W)],
            EventButtons => &[(CMD_SHIFT, Key::C)],
            FadeDurations => &[(CMD_SHIFT, Key::T)],
            EdgeFrames => &[(CMD_SHIFT, Key::O)],
            EventHeaders => &[(CMD_SHIFT, Key::M)],
            ShowEnvelopes => &[(CMD_SHIFT, Key::E)],
            RebuildPeaks => &[(SHIFT, Key::F5)],
            AudioBusTracks => &[(NONE, Key::B)],
            VideoBusTrack => &[(CMD_SHIFT, Key::B)],
            BuildRamPreview => &[(SHIFT, Key::B)],
            ClearRamPreview => &[(ALT_SHIFT, Key::B)],
            PrerenderVideo => &[(SHIFT, Key::M)],
            EventMediaMarkers => &[(CMD_SHIFT, Key::K)],
            ShowEnvelopeType(EnvelopeKind::Volume) => &[(NONE, Key::V)],
            ShowEnvelopeType(EnvelopeKind::Pan) => &[(NONE, Key::P)],
            AudioEnvelope(EnvelopeKind::Volume) => &[(SHIFT, Key::V)],
            AudioEnvelope(EnvelopeKind::Pan) => &[(SHIFT, Key::P)],
            InsertAudioTrack => &[(CMD, Key::Q)],
            InsertVideoTrack => &[(CMD_SHIFT, Key::Q)],
            InsertAdjustmentTrack => &[(CMD_ALT_SHIFT, Key::Q)],
            InsertMarker => &[(NONE, Key::M)],
            InsertRegion => &[(NONE, Key::R)],
            InsertCdTrack => &[(NONE, Key::N)],
            InsertCdIndex => &[(SHIFT, Key::N)],
            InsertCommand => &[(NONE, Key::C)],
            OpenInAudioEditor => &[(CMD, Key::E)],
            Quantize => &[(ALT, Key::F8)],
            Snapping => &[(NONE, Key::F8)],
            SnapGrid => &[(CMD, Key::F8)],
            SnapMarkers => &[(SHIFT, Key::F8)],
            SnapEvents => &[(CMD_SHIFT, Key::F8)],
            AutoCrossfade => &[(CMD_SHIFT, Key::X)],
            AutoRipple => &[(CMD, Key::L)],
            IgnoreGrouping => &[(CMD_SHIFT, Key::U)],
            HelpContents => &[(NONE, Key::F1)],
            _ => &[],
        }
    }

    pub fn icon(self) -> Option<Icon> {
        use Cmd::*;
        Some(match self {
            New => Icon::New,
            Open | ImportMedia => Icon::Open,
            Save | SaveAs | SaveIncremental => Icon::Save,
            Render => Icon::Render,
            Properties => Icon::Settings,
            Undo => Icon::Undo,
            Redo => Icon::Redo,
            Cut | GroupCut => Icon::Cut,
            Copy | GroupCopy => Icon::Copy,
            Paste | PasteInsert | PasteRepeat => Icon::Paste,
            Delete | GroupDelete => Icon::Delete,
            Trim => Icon::Crossfade,
            TrimStart => Icon::TrimStart,
            TrimEnd => Icon::TrimEnd,
            Split | SmartSplit => Icon::Split,
            ProjectStart => Icon::GoStart,
            ProjectEnd => Icon::GoEnd,
            FrameLeft => Icon::PrevFrame,
            FrameRight => Icon::NextFrame,
            SetSelectionStart | SetSelectionEnd => Icon::Region,
            LoopPlayback => Icon::Loop,
            Lock => Icon::Lock,
            GroupNew => Icon::Group,
            InsertAudioTrack => Icon::Speaker,
            InsertVideoTrack => Icon::Camera,
            InsertAudioBus => Icon::Bus,
            InsertAssignableFx => Icon::AssignableFx,
            InsertTextMedia => Icon::Text,
            InsertMarker => Icon::Marker,
            InsertRegion => Icon::Region,
            PanCropEvent => Icon::PanCrop,
            MotionTracking => Icon::MotionTracking,
            MediaFx | VideoEventFx | VideoTrackFx | VideoOutputFx | AudioEventFx | AudioTrackFx
            | NonRealTimeFx => Icon::Fx,
            AdvancedSave => Icon::Save,
            CreateNestedTimeline => Icon::NewNestedTimeline,
            OpenNestedTimeline => Icon::NestedTimeline,
            OpenParentTimeline => Icon::ParentTimeline,
            _ => return None,
        })
    }

    /// `Some(state)` for check/radio items.
    pub fn checked(self, app: &BananaApp) -> Option<bool> {
        use Cmd::*;
        let v = &app.timeline;
        // The layout in use, marked as VEGAS's (the others unmarked).
        if let LayoutSlot(n) = self {
            return (app.layout_slot == Some(n)).then_some(true);
        }
        Some(match self {
            CopyMediaWithProject => app.settings.save_as_copy_media,
            PreviewFader => app.settings.preview_fader,
            Toolbar => app.show_toolbar,
            StatusBar => app.show_status_bar,
            Window(tab) => app.is_tab_open(tab),
            ActiveTakeInfo => v.show_take_info,
            Waveforms => v.show_waveforms,
            EventButtons => v.show_buttons,
            FadeDurations => v.show_fade_durations,
            EdgeFrames => v.show_thumbnails,
            EventHeaders => v.show_clip_headers,
            EventDuration => v.show_event_duration,
            EventMediaMarkers => v.show_media_markers,
            Metronome => app.metronome,
            GenerateMtc => app.generate_mtc,
            GenerateClock => app.generate_clock,
            TriggerMtc => app.trigger_mtc,
            ExternalControl => app.settings.prefs.external_control,
            AutoHideDock => app.settings.prefs.auto_hide_dock,
            DeviceExplorer => app.device_explorer.open,
            AutomationMode(m) => app
                .focused_track()
                .and_then(|t| app.project().track(t))
                .is_some_and(|t| t.automation.mode == m),
            AutomationControls => app
                .focused_track()
                .and_then(|t| app.project().track(t))
                .is_some_and(|t| t.automation.controls),
            Record => app.engine.is_recording(),
            ExternalMonitor => app.external.is_some(),
            EventMediaMarkerLabels => v.show_media_markers && v.media_marker_labels,
            ShowEnvelopes => v.show_envelopes,
            MinimizeTracks => v.compact_tracks,
            AudioBusTracks => v.show_bus_tracks,
            VideoBusTrack => v.show_video_bus,
            UseTool(t) => app.tool == t,
            LoopPlayback => app.loop_enabled,
            Mute => app.all_selected(None, |c| c.muted).unwrap_or(false),
            Lock => app.all_selected(None, |c| c.locked).unwrap_or(false),
            Loop => app.all_selected(None, |c| c.looped).unwrap_or(false),
            HoldLastFrame => app
                .all_selected(Some(TrackKind::Video), |c| c.hold_last)
                .unwrap_or(false),
            InvertPhase => app
                .all_selected(Some(TrackKind::Audio), |c| c.invert_phase)
                .unwrap_or(false),
            Normalize => app
                .all_selected(Some(TrackKind::Audio), |c| c.normalize.is_some())
                .unwrap_or(false),
            MaintainAspect => app
                .all_selected(Some(TrackKind::Video), |c| c.transform.keeps_aspect())
                .unwrap_or(true),
            AllFrames => app
                .all_selected(Some(TrackKind::Video), |c| c.all_frames)
                .unwrap_or(false),
            AutoNormalize => app
                .all_selected(Some(TrackKind::Audio), |c| c.auto_normalize)
                .unwrap_or(false),
            ReduceFlicker => app
                .all_selected(Some(TrackKind::Video), |c| c.reduce_flicker)
                .unwrap_or(false),
            Resample(mode) => app
                .all_selected(Some(TrackKind::Video), |c| c.resample == mode)
                .unwrap_or(false),
            LockEnvelopes => app.project().lock_envelopes,
            SlipAllTakes => app.project().slip_all_takes,
            MulticamEditing => app.multicam,
            MulticamAudioWithVideo => !app.settings.multicam_audio_apart,
            MulticamFullFrame => app.settings.multicam_full_frame,
            Downmix => bv_engine::Engine::downmix() != bv_engine::Downmix::Off,
            Dim => bv_engine::Engine::dim(),
            Channels(m) => app
                .all_selected(Some(TrackKind::Audio), |c| c.channels == m)
                .unwrap_or(m == ChannelMap::Both),
            ShowEnvelopeType(EnvelopeKind::Volume) => v.show_env_volume,
            ShowEnvelopeType(EnvelopeKind::Pan) => v.show_env_pan,
            ShowEnvelopeType(EnvelopeKind::Mute) => v.show_env_mute,
            ShowEnvelopeType(EnvelopeKind::CompositeLevel) => v.show_env_composite,
            ShowEnvelopeType(EnvelopeKind::FadeToColor) => v.show_env_fade,
            ShowEnvelopeType(EnvelopeKind::Velocity) => v.show_env_velocity,
            ShowEnvelopeType(EnvelopeKind::TransitionProgress) => v.show_env_progress,
            ShowEnvelopeType(EnvelopeKind::MotionBlur) => v.show_env_motion_blur,
            ShowEnvelopeType(EnvelopeKind::Supersampling) => v.show_env_supersampling,
            AudioEnvelope(k) => app.has_envelope(TrackKind::Audio, k).unwrap_or(false),
            SurroundKeyframes => app.surround_keys_shown(),
            ShowSurroundKeyframes => v.show_surround_keys,
            ShowOfxEnvelopes => v.show_ofx_envelopes,
            ShowTrackKeyframes => v.show_track_keys,
            VideoEnvelope(k) if k.is_event() => app.has_event_envelope(k).unwrap_or(false),
            VideoEnvelope(k) => app.has_envelope(TrackKind::Video, k).unwrap_or(false),
            VideoMuteEnvelope => app
                .has_envelope(TrackKind::Video, EnvelopeKind::Mute)
                .unwrap_or(false),
            Quantize => app.quantize,
            ShowUnquantized => v.show_unquantized,
            Snapping => app.snapping,
            SnapGrid => app.snap_grid,
            SnapMarkers => app.snap_markers,
            SnapEvents => app.snap_events,
            Grid(g) => app.grid == g,
            Ruler(r) => app.effective_ruler_format() == r,
            AutoCrossfade => app.project().auto_crossfade,
            QuickFades => app.project().quick_fades,
            AutoRipple => app.auto_ripple,
            RippleMode(s) => app.ripple_scope == s,
            IgnoreGrouping => app.project().ignore_groups,
            BypassAudioFx => app.project().bypass_audio_fx,
            MuteAllAudio => app.project().master.muted,
            MuteAllVideo => app.project().mute_all_video,
            _ => return None,
        })
    }

    /// Radio items show a dot instead of a check mark.
    pub fn is_radio(self) -> bool {
        matches!(
            self,
            Cmd::UseTool(_)
                | Cmd::Channels(_)
                | Cmd::Grid(_)
                | Cmd::Ruler(_)
                | Cmd::RippleMode(_)
                | Cmd::AutomationMode(_)
                | Cmd::LayoutSlot(_)
        )
    }

    pub fn enabled(self, app: &BananaApp) -> bool {
        use Cmd::*;
        let sel = !app.selection.is_empty();
        let clip = app.clipboard.is_some();
        let p = app.project();
        let has = |kind| app.all_selected(Some(kind), |_| true).is_some();
        match self {
            Undo | UndoAll => app.history.can_undo(),
            Redo => app.history.can_redo(),
            ClearHistory => app.history.can_undo() || app.history.can_redo(),
            Delete => sel || app.track_list_selection().is_some(),
            Cut | Copy | TrimStart | TrimEnd | SelectEventStart | SelectEventEnd
            | SelectionPrevEvent | SelectionNextEvent | Mute | Lock | Loop | GroupRemove
            | GroupClear | GroupSelectAll | GroupCut | GroupCopy | GroupDelete | Ripple(_)
            | SelectedEventLeft | SelectedEventRight => sel,
            GroupNew => app.selection.len() > 1,
            // As VEGAS's: only where they would do something.
            Rejoin => app.selection.iter().any(|&id| p.can_rejoin(id)),
            CloseGaps => app.selection.len() > 1 && p.has_gaps(&app.selected()),
            Paste | PasteRepeat | PasteInsert => clip,
            PasteAttributes | PasteAttributesSelective => clip && sel,
            Trim => sel && p.loop_region.is_some(),
            SmartSplit | SelectLoopRegion => p.loop_region.is_some(),
            InvertPhase | Normalize | AutoNormalize | Channels(_) => has(TrackKind::Audio),
            MaintainAspect | HoldLastFrame | AllFrames | ReduceFlicker | Resample(_) => {
                has(TrackKind::Video)
            }
            AddMissingStream => app.can_add_missing_stream(),
            InsertTextMedia | SubtitlesFromFile => {
                crate::tabs::generators::titles_generator().is_some()
            }
            SubtitlesFromRegions => {
                crate::tabs::generators::titles_generator().is_some()
                    && p.regions.iter().any(|r| !r.name.trim().is_empty())
            }
            ResizeSelectedAdjustments => app.has_adjustments(false),
            ResizeAllAdjustments => app.has_adjustments(true),
            InsertAudioBus => p.busses.len() < bv_timeline::busses::MAX_BUSSES,
            InsertInputBus => p.input_busses.len() < bv_timeline::MAX_INPUT_BUSSES,
            FxAutomation => app
                .focused_track()
                .is_some_and(|t| app.has_automatable_fx(bv_timeline::EnvelopeOwner::Track(t))),
            RealTimeRender => {
                !app.engine.is_real_time_rendering()
                    && !app.engine.is_recording()
                    && p.duration() > bv_core::Tick::ZERO
            }
            InsertAssignableFx => p.fx_chains.len() < bv_timeline::busses::MAX_FX_CHAINS,
            RestoreSelection => app.can_restore_selection(),
            CursorToPlayhead => app.is_playing(),
            SaveLayout | ReloadLayout => app.layout_slot.is_some(),
            LayoutSlot(n) => app.settings.layouts.get(n).is_some_and(|l| l.is_some()),
            ExportXml | ExportFcpxml | ExportEdl | ExportOtio => p.clips().next().is_some(),
            AudioEnvelope(k) => !k.is_event() && app.current_track(TrackKind::Audio).is_some(),
            SurroundKeyframes => app.surround_keys_track().is_some(),
            VideoEnvelope(k) if k.is_event() => app.has_event_envelope(k).is_some(),
            VideoEnvelope(_) => app.current_track(TrackKind::Video).is_some(),
            Reverse => sel,
            FreezeFrameAtCursor => app.can_freeze_at_cursor(),
            TakeNext | TakePrevious | TakeDeleteActive => app.can_step_takes(),
            TakeChooseActive | TakeDelete => app.can_choose_take(),
            TakeRenameActive => app.can_rename_take(),
            MulticamSyncAudio => app.audio_sync.is_none() && app.can_sync_events(),
            MulticamSyncTimecode | MulticamSyncDate => app.can_sync_events(),
            MulticamCreate => app.can_create_multicam(),
            MulticamExpand => app.multicam_track().is_some(),
            TrackMotion => app
                .current_track(TrackKind::Video)
                .and_then(|t| p.track(t))
                .is_some_and(|t| !t.adjustment),
            VideoMuteEnvelope => app.current_track(TrackKind::Video).is_some(),
            InsertRegion | InsertCdTrack => p.loop_region.is_some(),
            InsertCdIndex => !p.cd_tracks.is_empty(),
            LayOutCd => p.clips().next().is_some(),
            BurnDvd | BurnTaoCd => {
                p.clips().next().is_some() && app.menu_dialogs.more.burn_job.is_none()
            }
            // The CD layout's tracks.
            BurnDaoCd => !p.cd_tracks.is_empty() && app.menu_dialogs.more.burn_job.is_none(),
            // An audio CD's time starts at zero: no ruler offset.
            SetTimeAtCursor | SetProjectTime => app.effective_ruler_format().uses_offset(),
            OpenInAudioEditor | OpenCopyInAudioEditor | AudioEventFx | NonRealTimeFx => {
                app.has_selected(TrackKind::Audio)
            }
            EditSourceProject => app.selected_source_project().is_some(),
            CreateNestedTimeline => !app.selection.is_empty(),
            OpenNestedTimeline => app.selected_nested_project().is_some(),
            OpenParentTimeline => app.in_nested_timeline(),
            MediaFx | PanCropEvent | VideoEventFx => app.has_selected(TrackKind::Video),
            AudioTrackFx => app.current_track(TrackKind::Audio).is_some(),
            VideoTrackFx => app.current_track(TrackKind::Video).is_some(),
            LoudnessLog => p.clips().next().is_some(),
            ExpandedEditMode => sel || app.timeline.edge_trim.is_some(),
            ExitEditMode => app.timeline.edge_trim.is_some(),
            ArmTrack => app.current_track(TrackKind::Audio).is_some(),
            PrerenderVideo => {
                !app.engine.is_prerendering()
                    && p.tracks
                        .iter()
                        .any(|t| t.kind == TrackKind::Video && !t.clips.is_empty())
            }
            CleanUpPrerender => !p.prerenders.is_empty(),
            RippleMode(_) => app.auto_ripple,
            _ => true,
        }
    }

    pub fn run(self, app: &mut BananaApp) {
        use Cmd::*;
        match self {
            New => app.request(Pending::New),
            Welcome => app.menu_dialogs.welcome.open = true,
            Open => app.request(Pending::Open),
            Close => app.request(Pending::Close),
            Save => {
                app.save(false);
            }
            SaveAs => {
                app.save(true);
            }
            SaveIncremental => app.save_incremental(),
            Render => app.open_render_dialog(),
            RenderToNewTrack => app.open_render_to_new_track(),
            RealTimeRender => app.real_time_render(),
            FxAutomation => {
                if let Some(t) = app.focused_track() {
                    app.open_fx_automation(bv_timeline::EnvelopeOwner::Track(t));
                }
            }
            ImportMedia => app.import_dialog(),
            ImportImageSequence => crate::sequence_dialog::open(app),
            ImportProjectMedia => crate::interchange_ui::import_project_media(app),
            ImportEdl => crate::interchange_ui::import_edl(app),
            ImportOtio => app.request(Pending::ImportOtio),
            ImportXml => app.request(Pending::ImportFcp(false)),
            ImportClosedCaptioning => app.import_closed_captioning(),
            ImportFcpxml => app.request(Pending::ImportFcp(true)),
            ImportBwf => crate::interchange_ui::import_bwf(app),
            ExportProject => app.export_project_copy(),
            ExportArchive => app.open_archive_dialog(),
            CopyMediaWithProject => {
                app.settings.save_as_copy_media = !app.settings.save_as_copy_media;
                let _ = app.settings.save();
            }
            ExportXml => crate::interchange_ui::export(app, crate::interchange_ui::Format::Xmeml),
            ExportFcpxml => {
                crate::interchange_ui::export(app, crate::interchange_ui::Format::Fcpxml)
            }
            ExportEdl => crate::interchange_ui::export(app, crate::interchange_ui::Format::Edl),
            ExportOtio => crate::otio_ui::export(app),
            Properties => app.open_project_settings(),
            Exit => app.request(Pending::Exit),
            Undo => app.undo(),
            Redo => app.redo(),
            Cut => app.cut(),
            Copy => app.copy(),
            Paste => app.paste(),
            PasteRepeat => app.menu_dialogs.open_paste_repeat(),
            PasteInsert => app.paste_insert(),
            PasteAttributes => app.paste_attributes(),
            PasteAttributesSelective => app.clip_windows.open_paste(),
            // With the track list focused, Delete removes the tracks
            // selected.
            Delete => match app.track_list_selection() {
                Some(t) => {
                    let tracks = app.tracks_with(t);
                    app.delete_tracks(&tracks);
                }
                None => app.delete_selection(app.auto_ripple),
            },
            Trim => app.trim_to_loop(),
            TrimStart => app.trim_to_cursor(true),
            TrimEnd => app.trim_to_cursor(false),
            Split => app.split_at_cursor(),
            SmartSplit => app.smart_split(),
            Rejoin => app.rejoin_selection(),
            CloseGaps => app.close_gaps(),
            GoTo => {
                let tc = app.cursor_timecode();
                app.menu_dialogs.open_go_to(tc);
            }
            ProjectStart => app.seek_playback(bv_core::Tick::ZERO),
            ProjectEnd => app.go_end(),
            SelectionStartGo => app.go_selection_edge(true),
            SelectionEndGo => app.go_selection_edge(false),
            GridLeft => app.move_by_grid(-1),
            GridRight => app.move_by_grid(1),
            PixelLeft => app.move_by_pixel(-1),
            PixelRight => app.move_by_pixel(1),
            FrameLeft => app.step_frames(-1),
            FrameRight => app.step_frames(1),
            MarkerLeft => app.move_by_marker(-1),
            MarkerRight => app.move_by_marker(1),
            SelectToMarker => app.select_to_marker(),
            EditPointLeft => app.move_by_edit_point(-1),
            EditPointRight => app.move_by_edit_point(1),
            SelectedEventLeft => app.move_by_selected_event(-1),
            SelectedEventRight => app.move_by_selected_event(1),
            CursorToPlayhead => app.cursor_to_playhead(),
            CenterOnCursor => app.center_on_cursor(),
            Ripple(scope) => app.post_edit_ripple(scope),
            SelectAll => app.select_all(),
            SelectAfterCursor => app.select_after_cursor(),
            SelectEventStart => app.select_event_edge(true),
            SelectEventEnd => app.select_event_edge(false),
            ExpandedEditMode => app.toggle_expanded_edit(),
            ExitEditMode => app.timeline.edge_trim = None,
            EditModeKey => {
                if app.timeline.edge_trim.is_some() {
                    app.timeline.edge_trim = None;
                } else {
                    app.toggle_expanded_edit();
                }
            }
            Record => app.toggle_recording(),
            Metronome => app.set_metronome(!app.metronome),
            CursorPreview => app.preview_cursor(),
            // The devices are the Sync tab's; without one, it says so.
            GenerateMtc => {
                app.generate_mtc ^= true;
                if app.generate_mtc && app.settings.prefs.mtc_output.is_none() {
                    app.notify(tr(
                        "Choose the MIDI output for timecode in Options › Preferences › Sync.",
                    ));
                }
            }
            GenerateClock => {
                app.generate_clock ^= true;
                if app.generate_clock && app.settings.prefs.clock_output.is_none() {
                    app.notify(tr(
                        "Choose the MIDI output for the clock in Options › Preferences › Sync.",
                    ));
                }
            }
            TriggerMtc => {
                app.trigger_mtc ^= true;
                if app.trigger_mtc && app.settings.prefs.mtc_input.is_none() {
                    app.notify(tr(
                        "Choose the MIDI input for timecode in Options › Preferences › Sync.",
                    ));
                }
            }
            ImportDvdCamcorder => app.dvd_import.open = true,
            DeviceExplorer => app.device_explorer.open ^= true,
            ExternalControl => {
                let p = &mut app.settings.prefs;
                p.external_control ^= true;
                if p.external_control && p.control_devices.is_empty() {
                    app.notify(tr(
                        "Add a control surface in Options › Preferences › External Control & Automation.",
                    ));
                }
                let _ = app.settings.save();
            }
            AutoHideDock => {
                app.settings.prefs.auto_hide_dock ^= true;
                app.dock_peek = false;
                let _ = app.settings.save();
            }
            AutomationMode(m) => {
                if let Some(t) = app.focused_track() {
                    app.set_automation_mode(t, m);
                }
            }
            AutomationControls => {
                if let Some(t) = app.focused_track() {
                    app.toggle_automation_controls(t);
                }
            }
            ExternalMonitor => app.toggle_external_monitor(),
            SetPathAndRecord => app.set_path_and_record(),
            ArmTrack => {
                if let Some(t) = app.current_track(TrackKind::Audio) {
                    app.toggle_arm(t);
                }
            }
            LoopPlayback => app.toggle_loop_playback(),
            SelectionToView => app.selection_to_view(),
            SelectionToProject => app.selection_to_project(),
            SelectLoopRegion => app.select_in_loop_region(),
            SetSelectionStart => app.set_selection_edge(true),
            SetSelectionEnd => app.set_selection_edge(false),
            SelectionPrevEvent => app.select_neighbour_event(-1),
            SelectionNextEvent => app.select_neighbour_event(1),
            RestoreSelection => app.restore_selection(),
            UseTool(t) => app.set_tool(t),
            NextTool => app.next_tool(1),
            PrevTool => app.next_tool(-1),
            Mute => app.toggle_mute_selection(),
            Lock => app.toggle_lock(),
            Loop => app.toggle_loop(),
            HoldLastFrame => app.toggle_hold_last(),
            InvertPhase => app.toggle_invert_phase(),
            Normalize => app.toggle_normalize(),
            MaintainAspect => app.toggle_maintain_aspect(),
            AllFrames => {
                let on = !Cmd::AllFrames.checked(app).unwrap_or(false);
                app.set_all_frames(on);
            }
            AutoNormalize => app.toggle_auto_normalize(),
            ReduceFlicker => app.toggle_reduce_flicker(),
            Resample(mode) => app.set_resample(mode),
            AddMissingStream => app.add_missing_streams(),
            InsertAdjustmentEvent => app.insert_adjustment_event(),
            InsertAdjustmentTrack => app.insert_adjustment_track(None),
            // The busses and chains show in the Mixing Console.
            InsertAudioBus => {
                if app.modify(|p| p.add_bus()).is_some() {
                    app.show_tab(Tab::MixingConsole, false);
                }
            }
            // An input bus on the default input; its strip in the console.
            MotionTracking => app.open_motion_tracking(None),
            InsertInputBus => {
                let input = app.default_record_input();
                if app.modify(|p| p.add_input_bus(input)).is_some() {
                    app.console.show_inputs = true;
                    app.show_tab(Tab::MixingConsole, false);
                }
            }
            InsertAssignableFx => {
                app.clip_windows.fx_target = Some(crate::fx_window::FxTarget::NewChain);
                app.show_tab(Tab::MixingConsole, false);
            }
            ResizeSelectedAdjustments => app.resize_adjustments(false),
            ResizeAllAdjustments => app.resize_adjustments(true),
            SubtitlesFromFile | SubtitlesFromRegions => {
                let from_file = self == SubtitlesFromFile;
                app.menu_dialogs.more.subtitles =
                    Some(crate::subtitles::SubtitlesDialog::new(app, from_file));
            }
            // Titles & Text at the cursor on the selected track, its window
            // open to type the text, as VEGAS's.
            InsertTextMedia => {
                if let Some(g) = crate::tabs::generators::titles_generator() {
                    crate::tabs::generators::add_and_edit(app, &g);
                }
            }
            LockEnvelopes => {
                app.modify(|p| p.lock_envelopes = !p.lock_envelopes);
            }
            // Stereo projects fold to mono; 5.1 ones to stereo, then mono.
            Downmix => {
                use bv_engine::{Downmix, Engine};
                let surround =
                    app.project().settings.channels == bv_core::ChannelLayout::Surround51;
                let next = match (Engine::downmix(), surround) {
                    (Downmix::Off, true) => Downmix::Stereo,
                    (Downmix::Off, false) | (Downmix::Stereo, _) => Downmix::Mono,
                    (Downmix::Mono, _) => Downmix::Off,
                };
                Engine::set_downmix(next);
            }
            Dim => bv_engine::Engine::set_dim(!bv_engine::Engine::dim()),
            GroupNew => app.group_selection(),
            GroupRemove => app.ungroup_selection(),
            GroupClear => app.clear_groups(),
            GroupSelectAll => app.select_groups(),
            GroupCut => {
                app.select_groups();
                app.cut();
            }
            GroupCopy => {
                app.select_groups();
                app.copy();
            }
            GroupDelete => {
                app.select_groups();
                app.delete_selection(app.auto_ripple);
            }
            Channels(m) => app.set_channels(m),
            UndoAll => app.undo_all(),
            ClearHistory => app.menu_dialogs.clear_history = true,
            Toolbar => app.show_toolbar = !app.show_toolbar,
            StatusBar => app.show_status_bar = !app.show_status_bar,
            OrganizeLayouts => {
                app.menu_dialogs.layouts = Some(crate::layouts::Organize::new(&app.settings));
            }
            SaveLayout => {
                if let Some(slot) = app.layout_slot {
                    let name = app
                        .settings
                        .layouts
                        .get(slot)
                        .and_then(|l| l.as_ref())
                        .map_or_else(|| format!("Layout {slot}"), |l| l.name.clone());
                    app.save_layout(slot, name);
                }
            }
            SaveLayoutAs => app
                .menu_dialogs
                .open_save_layout(&app.settings, app.layout_slot),
            ReloadLayout => app.reload_layout(),
            Layout(p) => app.apply_layout(p),
            LayoutSlot(n) => app.load_layout(n),
            FocusTimeline => app.focus_to_timeline(),
            Window(tab) => app.toggle_tab(tab),
            ActiveTakeInfo => app.timeline.show_take_info ^= true,
            Waveforms => app.timeline.show_waveforms ^= true,
            EventButtons => app.timeline.show_buttons ^= true,
            FadeDurations => app.timeline.show_fade_durations ^= true,
            EdgeFrames => app.timeline.show_thumbnails ^= true,
            EventHeaders => app.timeline.show_clip_headers ^= true,
            EventDuration => app.timeline.show_event_duration ^= true,
            EventMediaMarkers => app.timeline.show_media_markers ^= true,
            EventMediaMarkerLabels => {
                let v = &mut app.timeline;
                v.media_marker_labels = !(v.show_media_markers && v.media_marker_labels);
                v.show_media_markers |= v.media_marker_labels;
            }
            ShowEnvelopes => app.timeline.show_envelopes ^= true,
            MinimizeTracks => app.timeline.compact_tracks ^= true,
            AudioBusTracks => app.timeline.show_bus_tracks ^= true,
            VideoBusTrack => app.timeline.show_video_bus ^= true,
            RebuildPeaks => app.rebuild_peaks(),
            BuildRamPreview => app.build_ram_preview(),
            ClearRamPreview => app.engine.clear_ram_preview(),
            PrerenderVideo => app.open_prerender(),
            CleanUpPrerender => app.clean_up_prerenders(),
            // VEGAS's V and P: inserted on the selected audio tracks without
            // it, else shown or hidden.
            ShowEnvelopeType(k @ (EnvelopeKind::Volume | EnvelopeKind::Pan)) => {
                app.show_track_envelope(k)
            }
            ShowEnvelopeType(k) => {
                let v = &mut app.timeline;
                match k {
                    EnvelopeKind::Volume | EnvelopeKind::Send(_) => v.show_env_volume ^= true,
                    EnvelopeKind::Pan => v.show_env_pan ^= true,
                    EnvelopeKind::Mute => v.show_env_mute ^= true,
                    EnvelopeKind::CompositeLevel => v.show_env_composite ^= true,
                    EnvelopeKind::FadeToColor => v.show_env_fade ^= true,
                    EnvelopeKind::Velocity => v.show_env_velocity ^= true,
                    EnvelopeKind::TransitionProgress => v.show_env_progress ^= true,
                    EnvelopeKind::MotionBlur => v.show_env_motion_blur ^= true,
                    EnvelopeKind::Supersampling => v.show_env_supersampling ^= true,
                    EnvelopeKind::FxParam { .. } | EnvelopeKind::FxBypass(_) => {}
                }
            }
            AudioEnvelope(k) if k.is_event() => {}
            AudioEnvelope(k) => app.toggle_envelope(TrackKind::Audio, k),
            SurroundKeyframes => app.toggle_surround_keyframes(),
            ShowSurroundKeyframes => app.timeline.show_surround_keys ^= true,
            ShowOfxEnvelopes => app.timeline.show_ofx_envelopes ^= true,
            ShowTrackKeyframes => app.timeline.show_track_keys ^= true,
            VideoEnvelope(k) if k.is_event() => app.toggle_event_envelopes(k),
            VideoEnvelope(k) => app.toggle_envelope(TrackKind::Video, k),
            Reverse => app.reverse_selected(),
            FreezeFrameAtCursor => app.freeze_frames_at_cursor(),
            TakeNext => app.step_takes(false),
            TakePrevious => app.step_takes(true),
            TakeChooseActive => app.open_take_chooser(false),
            TakeDelete => app.open_take_chooser(true),
            TakeDeleteActive => app.delete_active_takes(),
            TakeRenameActive => app.open_take_rename(),
            SlipAllTakes => {
                app.modify(|p| p.slip_all_takes = !p.slip_all_takes);
            }
            MulticamSyncAudio => app.sync_audio(),
            MulticamSyncTimecode => app.sync_by(crate::multicam::SyncBy::Timecode),
            MulticamSyncDate => app.sync_by(crate::multicam::SyncBy::Date),
            MulticamFullFrame => {
                app.settings.multicam_full_frame ^= true;
                let _ = app.settings.save();
            }
            MulticamCreate => app.create_multicam(),
            MulticamExpand => app.ask_expand_multicam(),
            MulticamEditing => app.multicam ^= true,
            MulticamAudioWithVideo => {
                app.settings.multicam_audio_apart ^= true;
                let _ = app.settings.save();
            }
            TrackMotion => {
                if let Some(t) = app.current_track(TrackKind::Video) {
                    app.open_track_motion(t, false);
                }
            }
            VideoMuteEnvelope => app.toggle_envelope(TrackKind::Video, EnvelopeKind::Mute),
            InsertAudioTrack => app.insert_track(TrackKind::Audio),
            InsertVideoTrack => app.insert_track(TrackKind::Video),
            EmptyEvent => app.insert_empty_event(),
            Slideshow => app.menu_dialogs.more.open_slideshow(),
            InsertTime => app.menu_dialogs.more.open_insert_time(),
            InsertMarker => app.insert_marker(),
            InsertRegion => app.insert_region(),
            InsertCdTrack => app.insert_cd_track(),
            InsertCdIndex => app.insert_cd_index(),
            InsertCommand => {
                let at = app.mark_time();
                app.open_command_dialog(at);
            }
            LayOutCd => app.lay_out_cd(),
            BurnDvd => app.open_burn_dvd(),
            BurnTaoCd => app.open_burn_tao(),
            BurnDaoCd => app.open_burn_dao(),
            PreviewFader => {
                app.settings.preview_fader = !app.settings.preview_fader;
                let _ = app.settings.save();
            }
            OpenInAudioEditor => app.open_in_audio_editor(false),
            OpenCopyInAudioEditor => app.open_in_audio_editor(true),
            EditSourceProject => {
                if let Some(path) = app.selected_source_project() {
                    app.edit_source_project(&path);
                }
            }
            CreateNestedTimeline => app.request_nested_timeline(),
            OpenNestedTimeline => app.open_nested_timeline(),
            OpenParentTimeline => app.open_parent_timeline(),
            AudioEventFx => app.open_event_fx(TrackKind::Audio),
            NonRealTimeFx => app.open_nonrealtime_fx(),
            AudioTrackFx => app.open_track_fx(TrackKind::Audio),
            MediaFx => app.open_media_fx(),
            PanCropEvent => app.open_pan_crop(),
            VideoEventFx => app.open_event_fx(TrackKind::Video),
            VideoTrackFx => app.open_track_fx(TrackKind::Video),
            VideoOutputFx => app.clip_windows.output_fx = true,
            LoudnessLog => {
                app.engine.set_project(app.history.current());
                let range = app
                    .project()
                    .loop_region
                    .filter(|r| r.duration > bv_core::Tick::ZERO);
                if app.engine.start_loudness_scan(range) {
                    app.menu_dialogs.more.loudness = true;
                }
            }
            AdvancedSave => app.advanced_save(),
            CleanProjectMedia => app.clean_project_media(),
            RunScript(s) => app.run_script(s),
            Quantize => app.quantize ^= true,
            ShowUnquantized => app.timeline.show_unquantized ^= true,
            Snapping => app.snapping ^= true,
            SnapGrid => app.snap_grid ^= true,
            SnapMarkers => app.snap_markers ^= true,
            SnapEvents => app.snap_events ^= true,
            Grid(g) => app.grid = g,
            Ruler(r) => {
                if app.project().settings.ruler_format != Some(r) {
                    app.modify(|p| p.settings.ruler_format = Some(r));
                }
            }
            SetTimeAtCursor => {
                let tc = app.cursor_timecode();
                app.menu_dialogs.more.open_ruler_time(tc, false);
            }
            SetProjectTime => {
                let tc = bv_core::timecode::format(app.project().ruler_offset, app.frame_rate());
                app.menu_dialogs.more.open_ruler_time(tc, true);
            }
            SetProjectTempo => app.open_project_tempo(),
            AutoCrossfade => {
                app.modify(|p| p.auto_crossfade = !p.auto_crossfade);
            }
            QuickFades => {
                app.modify(|p| p.quick_fades = !p.quick_fades);
            }
            AutoRipple => app.auto_ripple ^= true,
            RippleMode(s) => app.ripple_scope = s,
            IgnoreGrouping => {
                app.modify(|p| p.ignore_groups = !p.ignore_groups);
            }
            BypassAudioFx => {
                app.modify(|p| p.bypass_audio_fx = !p.bypass_audio_fx);
            }
            MuteAllAudio => app.toggle_mute_all_audio(),
            MuteAllVideo => {
                app.modify(|p| p.mute_all_video = !p.mute_all_video);
            }
            ExportPreferences => crate::more_dialogs::export_preferences(app),
            ImportPreferences => crate::more_dialogs::import_preferences(app),
            Preferences => {
                app.menu_dialogs.more.preferences =
                    Some(crate::preferences::PreferencesDialog::new(app));
            }
            CustomizeToolbar => {
                app.menu_dialogs.more.toolbar = Some(crate::customize_toolbar::ToolbarDialog::new(
                    crate::toolbar_buttons::ToolbarKind::Main,
                ));
            }
            CustomizeTimelineToolbar => {
                app.menu_dialogs.more.toolbar = Some(crate::customize_toolbar::ToolbarDialog::new(
                    crate::toolbar_buttons::ToolbarKind::Timeline,
                ));
            }
            CustomizeKeyboard => {
                app.menu_dialogs.more.keyboard =
                    Some(crate::keyboard_dialog::KeyboardDialog::new(app));
            }
            HelpContents => app.menu_dialogs.more.help = Some(0),
            KeyboardShortcuts => app.menu_dialogs.more.help = Some(1),
            About => app.dialogs.about = true,
        }
    }
}

/// Every command with a shortcut in the keyboard map in use, as (name,
/// keys), for Help.
/// The Automation Settings items, as VEGAS names them.
pub(crate) fn automation_mode_name(m: bv_timeline::AutomationMode) -> &'static str {
    match m {
        bv_timeline::AutomationMode::Off => tr("Automation Off"),
        bv_timeline::AutomationMode::Read => tr("Automation Read"),
        bv_timeline::AutomationMode::Touch => tr("Automation Write (Touch)"),
        bv_timeline::AutomationMode::Latch => tr("Automation Write (Latch)"),
    }
}

pub fn shortcut_table(app: &BananaApp) -> Vec<(String, String)> {
    let reg = crate::keymap::registry();
    let mut v: Vec<(String, String)> = reg
        .iter()
        .zip(&app.keymap.bindings)
        .filter(|(c, b)| !b.is_empty() && c.action != crate::keymap::Action::Soon)
        .map(|(c, b)| {
            let name = match c.action {
                crate::keymap::Action::Cmd(cmd) => cmd.label(app),
                _ => c.id.clone(),
            };
            let keys = b.iter().map(|k| k.label()).collect::<Vec<_>>().join("; ");
            (name, keys)
        })
        .collect();
    // Options › Preferences › General decides what Space does; Enter does
    // the other.
    let (space, enter) = if app.settings.prefs.space_play_stop {
        (tr("Play / Stop"), tr("Play / Pause"))
    } else {
        (tr("Play / Pause"), tr("Play / Stop"))
    };
    v.extend([
        (space.to_string(), tr("Space").to_string()),
        (enter.to_string(), "Enter".to_string()),
        (
            tr("Play from Start").to_string(),
            tr("Shift+Space").to_string(),
        ),
        (
            tr("Zoom the timeline in / out").to_string(),
            tr("Up / Down Arrow").to_string(),
        ),
        (
            tr("Window Layouts").to_string(),
            tr("Alt+D, then D/A/C/S/T or 0–9").to_string(),
        ),
    ]);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui;

    fn frame(ctx: &egui::Context, app: &mut BananaApp, events: Vec<egui::Event>) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 960.0),
            )),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| app.frame(ui));
        out.textures_delta.clear();
    }

    fn press(ctx: &egui::Context, app: &mut BananaApp, modifiers: Modifiers, key: Key) {
        let event = |pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };
        frame(ctx, app, vec![event(true), event(false)]);
    }

    fn click(ctx: &egui::Context, app: &mut BananaApp, pos: egui::Pos2) {
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(ctx, app, vec![egui::Event::PointerMoved(pos), button(true)]);
        frame(ctx, app, vec![button(false)]);
    }

    #[test]
    fn switches_that_follow_edits_belong_to_them() {
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        let (id, media) = app
            .project()
            .clips()
            .find(|(t, _)| t.kind == TrackKind::Video)
            .map(|(_, c)| (c.id, c.media))
            .expect("a video event");
        // A movie's file (the demo's media are generated).
        app.modify_untracked(|p| {
            if let Some(m) = p.media.iter_mut().find(|m| m.id == media) {
                m.source = bv_timeline::MediaSource::File {
                    path: "movie.mp4".into(),
                };
            }
        });
        let before = app.project().clip(id).cloned().unwrap();
        // As the Properties tab sets it: the switch alone.
        app.modify(|p| p.clip_mut(id).unwrap().all_frames = true);
        app.keep_switches();
        let p = app.project();
        let info = p.media(media).unwrap();
        let c = p.clip(id).unwrap();
        assert_eq!(c.duration, c.to_timeline(info.duration - c.source_in));
        // One undo takes back the switch and the length it gave.
        app.history.undo();
        assert_eq!(app.project().clip(id), Some(&before));
        // Add Missing Stream: the demo's bars have no sound to add.
        app.selection.clear();
        app.selection.insert(id);
        assert!(!Cmd::AddMissingStream.enabled(&app));
    }

    #[test]
    fn track_motion_opens_for_the_current_video_track() {
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        let v = app.project().tracks[0].id;
        app.selected_track = Some(v);
        assert!(Cmd::TrackMotion.enabled(&app));
        Cmd::TrackMotion.run(&mut app);
        assert_eq!(
            app.track_motion.as_ref().map(|w| (w.track, w.parent)),
            Some((v, false))
        );
        // The window draws (embedded in the tests), and edits the track.
        frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, vec![]);
        assert!(app.track_motion.is_some(), "it stays open");
        let half = bv_timeline::Transform {
            scale: 0.5,
            ..Default::default()
        };
        app.edit(|p| {
            p.set_track_motion_at(
                v,
                false,
                bv_core::Tick::ZERO,
                half,
                (bv_timeline::KeyCurve::Linear, 0.0),
            )
        });
        frame(&ctx, &mut app, vec![]);
        assert_eq!(
            app.project()
                .track_motion(v, false)
                .map(|m| m.transform.scale),
            Some(0.5)
        );
        // Audio tracks have none.
        let a = app.project().tracks[1].id;
        app.track_motion = None;
        app.open_track_motion(a, false);
        assert!(app.track_motion.is_none());
    }

    #[test]
    fn multicamera_tracks_are_made_cut_and_expanded() {
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        app.multicam_for_review();
        let p = app.project();
        let track = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video && t.clips.iter().any(|c| c.take_count() > 1))
            .map(|t| t.id)
            .expect("a multicamera track");
        assert!(Cmd::MulticamEditing.checked(&app) == Some(true));
        // The cursor at 2 s: camera 2 from there.
        let before = app.project().track(track).unwrap().clips.len();
        app.multicam_switch(1, false);
        let t = app.project().track(track).unwrap();
        assert_eq!(t.clips.len(), before + 1);
        // On the frame at the cursor (29.97 fps: just before 2 s).
        let two = bv_core::Tick::from_seconds(2);
        let cut = t
            .clips
            .iter()
            .find(|c| c.start <= two && two - c.start < bv_core::Tick::from_seconds_f64(0.04))
            .unwrap();
        assert_eq!(cut.active_take(), 1);
        // Camera 1 plays from 4 s (the demo's sunset). With Ctrl, at 7 s,
        // camera 2 with a crossfade: the two events overlap by the
        // cut-to-overlap conversion's second.
        let s = bv_core::Tick::from_seconds;
        app.settings.prefs.cut_overlap = 1.0;
        app.cursor = s(7);
        app.multicam_switch(1, true);
        let t = app.project().track(track).unwrap();
        let after = t
            .clips
            .iter()
            .find(|c| c.active_take() == 1 && c.start > s(5))
            .unwrap();
        let before = t
            .clips
            .iter()
            .find(|c| c.id != after.id && c.end() > after.start && c.start < after.start)
            .unwrap();
        assert_eq!(before.end() - after.start, s(1));
        // A time selection while stopped, the cursor in it: camera 1 over
        // all of 8–9 s, then camera 2 again, which makes the three parts
        // one event again.
        app.modify(|p| p.loop_region = Some(bv_core::TimeRange::from_bounds(s(8), s(9))));
        app.cursor = s(8);
        app.multicam_switch(0, false);
        let t = app.project().track(track).unwrap();
        let recut = t.clips.iter().find(|c| c.start == s(8)).unwrap();
        assert_eq!((recut.end(), recut.active_take()), (s(9), 0));
        let n = t.clips.len();
        app.multicam_switch(1, false);
        assert_eq!(app.project().track(track).unwrap().clips.len(), n - 2);
        app.project().check_invariants().unwrap();
        // Expand to Multiple Tracks: a track per camera.
        app.selected_track = Some(track);
        assert!(Cmd::MulticamExpand.enabled(&app));
        app.expand_multicam(track, true);
        assert!(app.project().track(track).is_none());
        app.project().check_invariants().unwrap();
    }

    /// Custom compositing: without a compositor the track asks the
    /// Compositors window for one; given one, it composites through it and
    /// its settings open; picked again, Custom comes back with it.
    #[test]
    fn custom_compositing_asks_for_a_compositor_and_keeps_it() {
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx);
        app.load_demo();
        let v = app.project().tracks[0].id;
        app.choose_custom_compositing(v, false);
        assert_eq!(app.compositor_pick, Some((v, false)));
        assert_eq!(app.compositor_target(), Some((v, false)));
        assert!(app.compositor_window.is_none());
        let fx = bv_timeline::PluginFx {
            id: "org.bananav.HeightMap".into(),
            version: [1, 0],
            name: "Height Map".into(),
            params: Default::default(),
        };
        app.apply_compositor(v, false, fx);
        let t = app.project().track(v).unwrap();
        assert_eq!(t.composite_mode, bv_timeline::CompositeMode::Custom);
        assert_eq!(t.compositor.as_ref().unwrap().id, "org.bananav.HeightMap");
        assert!(app.compositor_pick.is_none());
        assert!(app.compositor_window.is_some());
        // Back to Source Alpha and to Custom: the compositor is kept.
        app.modify(|p| {
            p.track_by_id_mut(v).unwrap().composite_mode = bv_timeline::CompositeMode::SourceAlpha
        });
        app.compositor_window = None;
        app.choose_custom_compositing(v, false);
        let t = app.project().track(v).unwrap();
        assert_eq!(t.composite_mode, bv_timeline::CompositeMode::Custom);
        assert!(app.compositor_window.is_some());
    }

    /// Two recordings of one sound, the second begun 3.25 s later and put
    /// anywhere: Sync Mixed Audio to Align Events puts it where it was
    /// heard, against the event picked first.
    #[test]
    fn events_are_lined_up_by_their_sound() {
        let dir = std::env::temp_dir().join(format!("bananav-sync-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rate = 48_000u32;
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut rnd = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f32 / (1u64 << 53) as f32
        };
        let (mut level, mut left) = (0.0, 0usize);
        let sound: Vec<f32> = (0..20 * rate as usize)
            .map(|_| {
                if left == 0 {
                    level = if rnd() < 0.35 { 0.0 } else { 0.1 + rnd() * 0.8 };
                    left = (rate as f32 * (0.05 + rnd() * 0.3)) as usize;
                }
                left -= 1;
                (rnd() - 0.5) * 2.0 * level
            })
            .collect();
        let write = |name: &str, samples: Vec<f32>| {
            let path = dir.join(name);
            let file = std::fs::File::create(&path).unwrap();
            let mut w =
                bv_wav::WavWriter::new(file, rate, 1, bv_wav::SampleFormat::Int(16)).unwrap();
            w.write_planar(&[samples]).unwrap();
            w.finish().unwrap();
            bv_engine::Engine::probe_file(&path.to_string_lossy()).unwrap()
        };
        let a = write("a.wav", sound.clone());
        let late = (3.25 * rate as f64) as usize;
        let b = write(
            "b.wav",
            sound[late..late + 12 * rate as usize]
                .iter()
                .map(|s| s * 0.5)
                .collect(),
        );
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx);
        let s = bv_core::Tick::from_seconds;
        let (ca, cb) = app.modify(|p| {
            let (ma, mb) = (p.add_media(a), p.add_media(b));
            let (ta, tb) = (p.add_track(TrackKind::Audio), p.add_track(TrackKind::Audio));
            let ca = p.insert_media(ma, s(1), None, Some(ta)).unwrap()[0];
            let cb = p.insert_media(mb, s(10), None, Some(tb)).unwrap()[0];
            (ca, cb)
        });
        assert!(!Cmd::MulticamSyncAudio.enabled(&app));
        app.select_clip(ca, false);
        app.select_clip(cb, true);
        assert!(Cmd::MulticamSyncAudio.enabled(&app));
        app.sync_audio();
        for _ in 0..1000 {
            app.poll_audio_sync();
            if app.audio_sync.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(app.audio_sync.is_none(), "the synchronization ends");
        let start = app.project().clip(cb).unwrap().start.as_seconds_f64();
        // On the nearest frame (Quantize to Frames).
        assert!((start - 4.25).abs() < 0.02, "{start}");
        assert_eq!(app.project().clip(ca).unwrap().start, s(1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A beat of 120 a minute in a file: Beat Detection shows its kicks in
    /// the Trimmer, OK makes them the media's markers (none twice when run
    /// again); Tempo Detection makes 120 the project's tempo, the ruler in
    /// measures and beats, snapping to the grid.
    #[test]
    fn beat_and_tempo_detection_read_a_beat() {
        let dir = std::env::temp_dir().join(format!("bananav-beats-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rate = 44_100u32;
        let kicks: Vec<f32> = (0..12 * rate as usize)
            .map(|i| {
                let since = (i as f64 / rate as f64).rem_euclid(0.5);
                let thud = (std::f64::consts::TAU * 60.0 * since).sin();
                (0.8 * thud * (-since / 0.08).exp()) as f32
            })
            .collect();
        let path = dir.join("beat.wav");
        let file = std::fs::File::create(&path).unwrap();
        let mut w = bv_wav::WavWriter::new(file, rate, 1, bv_wav::SampleFormat::Int(16)).unwrap();
        w.write_planar(&[kicks]).unwrap();
        w.finish().unwrap();
        let info = bv_engine::Engine::probe_file(&path.to_string_lossy()).unwrap();
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx);
        let media = app.modify(|p| p.add_media(info));
        let wait = |app: &BananaApp| {
            for _ in 0..2000 {
                if app.engine.beat_progress().is_some_and(|p| p.finished) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("the sound is read");
        };
        app.beat_detection(media);
        assert!(app.beat_dialog.is_some());
        wait(&app);
        let shown = app.beat_preview(media).expect("the Trimmer shows them");
        assert!((22..=24).contains(&shown.len()), "{}", shown.len());
        for t in &shown {
            let off = (t.as_seconds_f64() + 0.25).rem_euclid(0.5) - 0.25;
            assert!(off.abs() < 0.005, "{t:?}");
        }
        app.keep_beats();
        assert!(app.beat_dialog.is_none());
        let count = |app: &BananaApp| app.project().media(media).unwrap().markers.len();
        assert_eq!(count(&app), shown.len());
        app.beat_detection(media);
        wait(&app);
        app.keep_beats();
        assert_eq!(count(&app), shown.len(), "none twice");
        app.tempo_detection(media);
        assert!(app.tempo_dialog.is_some());
        wait(&app);
        app.apply_tempo();
        let s = &app.project().settings;
        assert!(
            (s.tempo.beats_per_minute - 120.0).abs() < 0.05,
            "{}",
            s.tempo.beats_per_minute
        );
        assert_eq!(
            s.ruler_format,
            Some(crate::timefmt::RulerFormat::MeasuresBeats)
        );
        assert!(app.snapping && app.snap_grid);
        assert!(app.tempo_dialog.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Tools › Audio › Apply Non-Real-Time Event FX: an event's tone (from
    /// its second second on) through its Gain of −6 dB into a new file,
    /// which joins the project and becomes the event's take, the effect
    /// taken off the event.
    #[test]
    fn non_real_time_fx_render_the_event_into_a_take() {
        let dir = std::env::temp_dir().join(format!("bananav-nrt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let rate = 48_000u32;
        // A tone growing louder: 0.1 at the start, 0.5 at the end.
        let tone: Vec<f32> = (0..4 * rate as usize)
            .map(|i| {
                let t = i as f32 / rate as f32;
                (0.1 + 0.1 * t) * (std::f32::consts::TAU * 440.0 * t).sin()
            })
            .collect();
        let path = dir.join("tone.wav");
        let file = std::fs::File::create(&path).unwrap();
        let mut w = bv_wav::WavWriter::new(file, rate, 1, bv_wav::SampleFormat::Float(32)).unwrap();
        w.write_planar(&[tone]).unwrap();
        w.finish().unwrap();
        let info = bv_engine::Engine::probe_file(&path.to_string_lossy()).unwrap();
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx);
        let s = bv_core::Tick::from_seconds;
        let clip = app.modify(|p| {
            let m = p.add_media(info);
            let t = p.add_track(TrackKind::Audio);
            let c = p.insert_media(m, s(0), None, Some(t)).unwrap()[0];
            // The event plays the tone's seconds 1 to 3.
            let clip = p.clip_mut(c).unwrap();
            clip.source_in = s(1);
            clip.duration = s(2);
            let mut gain = bv_timeline::Effect::new(bv_timeline::EffectKind::Gain);
            gain.set_param(0, -6.0);
            clip.effects.push(gain);
            c
        });
        assert!(!Cmd::NonRealTimeFx.enabled(&app));
        app.select_clip(clip, false);
        assert!(Cmd::NonRealTimeFx.enabled(&app));
        Cmd::NonRealTimeFx.run(&mut app);
        let d = app.nrt_dialog.as_ref().expect("the dialog");
        assert!(d.path.ends_with("tone (FX).wav"), "{}", d.path);
        app.start_nonrealtime();
        for _ in 0..2000 {
            app.poll_nonrealtime();
            if app.nrt_dialog.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(app.nrt_dialog.is_none(), "rendered");
        let p = app.project();
        let c = p.clip(clip).unwrap();
        assert_eq!(c.take_count(), 2);
        let new = p.media(c.media).unwrap();
        assert!(new.name.contains("tone (FX)"), "{}", new.name);
        assert_eq!(c.source_in, s(0));
        assert!(c.effects.is_empty(), "the gain is in the take");
        // The new file: two seconds of the tone from its second 1, at half
        // its level (−6 dB).
        let file = std::fs::File::open(dir.join("tone (FX).wav")).unwrap();
        let mut r = bv_wav::WavReader::new(file).unwrap();
        let out = r.read_planar(0, 2 * rate as usize).unwrap().remove(0);
        assert_eq!(out.len(), 2 * rate as usize);
        let peak = |x: &[f32]| x.iter().fold(0f32, |m, v| m.max(v.abs()));
        let (start, end) = (peak(&out[..4800]), peak(&out[out.len() - 4800..]));
        assert!((start - 0.2 * 0.501).abs() < 0.01, "{start}");
        assert!((end - 0.4 * 0.501).abs() < 0.01, "{end}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Auto Ducking: a narration (a voice from 1 s to 3 s) made a
    /// controller has its loudness read; the music made a listener goes
    /// down 12 dB while the voice sounds, back up after; an event's own
    /// reductions win over its track's.
    #[test]
    fn auto_ducking_lowers_the_music_under_the_voice() {
        let dir = std::env::temp_dir().join(format!("bananav-ducking-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rate = 48_000u32;
        let voice: Vec<f32> = (0..5 * rate as usize)
            .map(|i| {
                let t = i as f32 / rate as f32;
                if (1.0..3.0).contains(&t) {
                    0.3 * (std::f32::consts::TAU * 200.0 * t).sin()
                } else {
                    0.0
                }
            })
            .collect();
        let path = dir.join("voice.wav");
        let file = std::fs::File::create(&path).unwrap();
        let mut w = bv_wav::WavWriter::new(file, rate, 1, bv_wav::SampleFormat::Int(16)).unwrap();
        w.write_planar(&[voice]).unwrap();
        w.finish().unwrap();
        let info = bv_engine::Engine::probe_file(&path.to_string_lossy()).unwrap();
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx);
        let s = bv_core::Tick::from_seconds_f64;
        let (narration, music, clip) = app.modify(|p| {
            let m = p.add_media(info);
            let narration = p.add_track(TrackKind::Audio);
            let music = p.add_track(TrackKind::Audio);
            let c = p.insert_media(m, s(0.0), None, Some(narration)).unwrap()[0];
            (narration, music, c)
        });
        app.toggle_duck_controller(narration);
        assert!(app.duck_dialog.is_some());
        for _ in 0..2000 {
            app.poll_ducking();
            if app.project().clip(clip).unwrap().duck_levels.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            app.project().clip(clip).unwrap().duck_levels.is_some(),
            "read"
        );
        app.toggle_duck_listener(music);
        let p = app.project();
        let m = p.track(music).unwrap();
        let gain = |t: f64| p.ducking_gain(m, s(t));
        assert!((gain(0.5) - 1.0).abs() < 1e-3, "{}", gain(0.5));
        assert!((gain(2.0) - 0.251).abs() < 0.01, "{}", gain(2.0));
        assert!((gain(4.5) - 1.0).abs() < 1e-3, "{}", gain(4.5));
        // Re-Analyze Auto Ducking: the event's own reductions.
        app.reanalyse_ducking(clip);
        app.modify(|p| {
            p.clip_mut(clip).unwrap().duck_levels.as_mut().unwrap().own =
                Some(bv_timeline::Ducking {
                    floor_db: -6.0,
                    peak: 0.0,
                })
        });
        for _ in 0..2000 {
            app.poll_ducking();
            if app.engine.ducking_progress().is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let p = app.project();
        let m = p.track(music).unwrap();
        assert!((p.ducking_gain(m, s(2.0)) - 0.501).abs() < 0.01, "kept");
        // Off: nothing more is lowered.
        app.toggle_duck_controller(narration);
        let p = app.project();
        assert_eq!(p.ducking_gain(p.track(music).unwrap(), s(2.0)), 1.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn takes_are_added_stepped_chosen_deleted_and_renamed() {
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        let p = app.project();
        let video = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .unwrap();
        let (first, own, other) = (
            video.clips[0].id,
            video.clips[0].media,
            video.clips[1].media,
        );
        app.selection = vec![first].into_iter().collect();
        assert!(!Cmd::TakeNext.enabled(&app), "one take only");
        assert!(Cmd::TakeRenameActive.enabled(&app));
        // Add as Takes: the new media plays.
        app.add_take(first, other);
        assert_eq!(app.project().clip(first).unwrap().media, other);
        assert!(Cmd::TakeNext.enabled(&app) && Cmd::TakeChooseActive.enabled(&app));
        Cmd::TakeNext.run(&mut app);
        assert_eq!(app.project().clip(first).unwrap().media, own);
        Cmd::TakePrevious.run(&mut app);
        assert_eq!(app.project().clip(first).unwrap().media, other);
        // The Take Chooser and Rename Active open on the event.
        Cmd::TakeChooseActive.run(&mut app);
        assert!(app.take_chooser.is_some());
        Cmd::TakeRenameActive.run(&mut app);
        assert!(app.take_rename.is_some());
        // Options › Slip All Takes is the project's.
        assert_eq!(Cmd::SlipAllTakes.checked(&app), Some(false));
        Cmd::SlipAllTakes.run(&mut app);
        assert!(app.project().slip_all_takes);
        // Delete Active: back to one take, the event's own.
        Cmd::TakeDeleteActive.run(&mut app);
        let c = app.project().clip(first).unwrap();
        assert_eq!((c.take_count(), c.media), (1, own));
        app.project().check_invariants().unwrap();
    }

    #[test]
    fn velocity_freeze_progress_and_reverse_on_the_selected_events() {
        use bv_timeline::EnvelopeKind;
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        let p = app.project();
        let video = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .unwrap();
        let (first, second) = (video.clips[0].id, video.clips[1].id);
        // The first event, the cursor inside it.
        app.selection = vec![first].into_iter().collect();
        app.seek_seconds(1.0);
        let vel = Cmd::VideoEnvelope(EnvelopeKind::Velocity);
        assert!(vel.enabled(&app) && vel.checked(&app) == Some(false));
        vel.run(&mut app);
        assert!(app.project().clip(first).unwrap().velocity().is_some());
        assert_eq!(vel.checked(&app), Some(true));
        vel.run(&mut app);
        assert!(app.project().clip(first).unwrap().velocity().is_none());
        // Freeze Frame at Cursor: the velocity to 0 at the cursor.
        assert!(Cmd::FreezeFrameAtCursor.enabled(&app));
        Cmd::FreezeFrameAtCursor.run(&mut app);
        let c = app.project().clip(first).unwrap().clone();
        assert_eq!(c.velocity().unwrap().points.len(), 2);
        let held = c.source_time(c.start + bv_core::Tick::from_seconds(1));
        assert_eq!(c.source_time(c.end() - bv_core::Tick(1)), held);
        // Transition progress: only where there is a transition (the second
        // event crossfades with the first).
        let progress = Cmd::VideoEnvelope(EnvelopeKind::TransitionProgress);
        assert!(!progress.enabled(&app), "the first event has no transition");
        app.selection = vec![second].into_iter().collect();
        assert!(progress.enabled(&app));
        progress.run(&mut app);
        let env = app
            .project()
            .clip(second)
            .unwrap()
            .envelope(EnvelopeKind::TransitionProgress);
        assert_eq!(env.map(|e| e.points.len()), Some(2));
        // Reverse, and back.
        let before = app.project().clip(second).unwrap().clone();
        Cmd::Reverse.run(&mut app);
        let c = app.project().clip(second).unwrap().clone();
        assert!(c.reverse.is_some());
        assert_eq!(c.source_time(c.start), before.source_time(before.end()));
        // Its subclip in Project Media, backward, made once.
        let reversed = |app: &BananaApp| {
            app.project()
                .subclips
                .iter()
                .filter(|s| s.reverse && s.parent == before.media)
                .count()
        };
        assert_eq!(reversed(&app), 1);
        Cmd::Reverse.run(&mut app);
        let c = app.project().clip(second).unwrap();
        assert_eq!(c.source_time(c.start), before.source_time(before.start));
        Cmd::Reverse.run(&mut app);
        assert_eq!(reversed(&app), 1);
        Cmd::Reverse.run(&mut app);
        // The view's switches.
        let show = Cmd::ShowEnvelopeType(EnvelopeKind::Velocity);
        assert_eq!(show.checked(&app), Some(true));
        show.run(&mut app);
        assert_eq!(show.checked(&app), Some(false));
    }

    #[test]
    fn loudness_meters_open_and_read_the_engine() {
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        let km = crate::keymap::Keymap::in_memory();
        let action = crate::keymap::Action::Cmd(Cmd::Window(Tab::LoudnessMeters));
        assert_eq!(km.text_for(action), "Ctrl+Alt+7");
        assert!(!app.is_tab_open(Tab::LoudnessMeters));
        frame(&ctx, &mut app, vec![]);
        press(&ctx, &mut app, CMD_ALT, Key::Num7);
        assert!(app.is_tab_open(Tab::LoudnessMeters));
        // The window draws with nothing measured yet and after a reset.
        bv_engine::Engine::reset_loudness();
        frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, vec![]);
        // ATSC A/85's targets, EBU +18's range.
        let p = crate::settings::LoudnessPrefs {
            atsc: true,
            wide: true,
            ..Default::default()
        };
        assert_eq!(p.targets(), (-24.0, -2.0));
        assert_eq!(p.range(), (-36.0, 18.0));
        assert_eq!(
            crate::settings::LoudnessPrefs::default().targets(),
            (-23.0, -1.0)
        );
    }

    #[test]
    fn adjustment_tracks_and_events_go_in_where_vegas_puts_them() {
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        Cmd::InsertAdjustmentTrack.run(&mut app);
        let p = app.project();
        let top = &p.tracks[0];
        assert!(top.adjustment && top.kind == TrackKind::Video);
        assert_eq!(app.selected_track, Some(top.id));
        // An adjustment event does not go on the adjustment track selected:
        // it goes to the first other video track.
        app.seek_seconds(1.0);
        Cmd::InsertAdjustmentEvent.run(&mut app);
        let p = app.project();
        let adj: Vec<_> = p
            .clips()
            .filter(|(_, c)| p.media(c.media).is_some_and(|m| m.is_adjustment()))
            .map(|(t, c)| (t.adjustment, c.id))
            .collect();
        assert_eq!(adj.len(), 1);
        assert!(!adj[0].0, "not on the adjustment track");
        assert!(Cmd::ResizeAllAdjustments.enabled(&app));
        Cmd::ResizeAllAdjustments.run(&mut app);
        let p = app.project();
        let c = p.clip(adj[0].1).unwrap();
        assert_eq!((c.start, c.end()), (bv_core::Tick::ZERO, p.duration()));
    }

    /// VEGAS's V and P, "Insert/show/hide track volume (panning)
    /// envelope": on the audio tracks selected, inserted where missing
    /// (one undo), then hidden and shown again; Shift+V takes it away.
    #[test]
    fn v_and_p_insert_show_and_hide_track_envelopes() {
        use bv_timeline::TrackId;
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        app.insert_track(TrackKind::Audio);
        let audio: Vec<TrackId> = app
            .project()
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
            .collect();
        assert!(audio.len() >= 2);
        app.pick_track(audio[0], false, false);
        app.pick_track(audio[1], false, true);
        let has = |app: &BananaApp, kind| {
            audio[..2].iter().all(|&t| {
                app.project()
                    .track(t)
                    .is_some_and(|t| t.envelopes.iter().any(|e| e.kind == kind))
            })
        };
        app.timeline.show_env_volume = false;
        press(&ctx, &mut app, NONE, Key::V);
        assert!(has(&app, EnvelopeKind::Volume), "on both tracks");
        assert!(app.timeline.show_envelopes && app.timeline.show_env_volume);
        press(&ctx, &mut app, NONE, Key::V);
        assert!(!app.timeline.show_env_volume, "hidden");
        assert!(has(&app, EnvelopeKind::Volume), "kept");
        press(&ctx, &mut app, NONE, Key::V);
        assert!(app.timeline.show_env_volume, "shown again");
        press(&ctx, &mut app, NONE, Key::P);
        assert!(has(&app, EnvelopeKind::Pan) && app.timeline.show_env_pan);
        app.undo();
        assert!(!has(&app, EnvelopeKind::Pan), "one undo for both");
        // Shift+V: the current track's taken away.
        press(&ctx, &mut app, SHIFT, Key::V);
        let current = app.current_track(TrackKind::Audio).expect("a track");
        assert!(
            app.project()
                .track(current)
                .is_some_and(|t| !t.envelopes.iter().any(|e| e.kind == EnvelopeKind::Volume))
        );
    }

    /// Media let go on an adjustment track go on it, as VEGAS's (its
    /// effects then work on them too); moved there, events stay.
    #[test]
    fn media_go_on_an_adjustment_track() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        Cmd::InsertAdjustmentTrack.run(&mut app);
        let adjust = app.project().tracks[0].id;
        let bars = app.project().media[0].id;
        let s = bv_core::Tick::from_seconds;
        app.place_media(bars, s(20), Some(adjust));
        let p = app.project();
        let on = |p: &bv_timeline::Project| p.track(adjust).unwrap().clips.len();
        assert_eq!(on(p), 1, "on the adjustment track");
        // An event of the track below moved up onto it.
        let below = p.tracks[1].clips[0].id;
        app.edit(|p| p.move_clip(below, s(40), Some(adjust)));
        assert_eq!(on(app.project()), 2);
    }

    #[test]
    fn downmix_and_dim_switch_playback_only() {
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        let before = app.project().clone();
        assert_eq!(Cmd::Downmix.checked(&app), Some(false));
        // A stereo project folds to mono, and back.
        Cmd::Downmix.run(&mut app);
        assert_eq!(bv_engine::Engine::downmix(), bv_engine::Downmix::Mono);
        Cmd::Downmix.run(&mut app);
        assert_eq!(bv_engine::Engine::downmix(), bv_engine::Downmix::Off);
        Cmd::Dim.run(&mut app);
        assert_eq!(Cmd::Dim.checked(&app), Some(true));
        Cmd::Dim.run(&mut app);
        assert!(!bv_engine::Engine::dim());
        // Nothing of the project changes.
        assert_eq!(*app.project(), before);
    }

    #[test]
    fn track_list_keys_act_on_the_selected_track() {
        use crate::keymap::Context;
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        frame(&ctx, &mut app, Vec::new());
        let tracks: Vec<_> = app.project().tracks.iter().map(|t| t.id).collect();
        // A click on the first track header (below its level slider) gives
        // the keys to the track list and selects the track.
        let header = app.track_header_spot_for_review(0).expect("header");
        click(&ctx, &mut app, header);
        assert_eq!(app.key_focus, Context::TrackList);
        assert_eq!(app.selected_track, Some(tracks[0]));
        press(&ctx, &mut app, NONE, Key::ArrowDown);
        assert_eq!(app.selected_track, Some(tracks[1]), "Down: next track");
        let audio = |app: &BananaApp| {
            let t = app.project().track(tracks[1]).expect("audio track");
            (t.level, t.pan)
        };
        press(&ctx, &mut app, Modifiers::CTRL, Key::ArrowRight);
        assert!(
            (audio(&app).0 - 10f32.powf(0.05)).abs() < 1e-4,
            "Ctrl+Right: +1 dB"
        );
        press(
            &ctx,
            &mut app,
            Modifiers::CTRL | Modifiers::SHIFT,
            Key::ArrowRight,
        );
        assert!(
            (audio(&app).1 - 0.02).abs() < 1e-6,
            "Ctrl+Shift+Right: pan right"
        );
        // Digits pick a track here (in the track view they go to markers).
        press(&ctx, &mut app, NONE, Key::Num1);
        assert_eq!(app.selected_track, Some(tracks[0]));
        press(&ctx, &mut app, NONE, Key::F2);
        assert_eq!(
            app.timeline.renaming.as_ref().map(|(t, _)| *t),
            Some(tracks[0]),
            "F2 renames the track"
        );
        // Esc leaves the name field without renaming.
        press(&ctx, &mut app, NONE, Key::Escape);
        assert!(app.timeline.renaming.is_none());
        frame(&ctx, &mut app, Vec::new());
        // Delete removes the selected track while the track list has the keys.
        press(&ctx, &mut app, NONE, Key::End);
        assert_eq!(app.selected_track, Some(tracks[1]));
        press(&ctx, &mut app, NONE, Key::Delete);
        assert_eq!(app.project().tracks.len(), 1);
        // A click in the timeline gives the keys back to the track view.
        let lane = app.timeline_spot_for_review(0, 20.0).expect("lane");
        click(&ctx, &mut app, lane);
        assert_eq!(app.key_focus, Context::TrackView);
    }

    #[test]
    fn a_click_in_the_explorer_gives_it_the_keys() {
        use crate::keymap::Context;
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        app.show_tab_named("Explorer");
        frame(&ctx, &mut app, Vec::new());
        let clips = |app: &BananaApp| app.project().clips().count();
        let before = clips(&app);
        click(&ctx, &mut app, egui::pos2(300.0, 400.0));
        assert_eq!(app.key_focus, Context::Explorer);
        // S is a TrackView key: it does not split while the Explorer has the keys.
        press(&ctx, &mut app, NONE, Key::S);
        assert_eq!(clips(&app), before);
        click(&ctx, &mut app, egui::pos2(900.0, 620.0));
        assert_eq!(app.key_focus, Context::TrackView);
        press(&ctx, &mut app, NONE, Key::S);
        assert_eq!(clips(&app), before + 1);
    }

    #[test]
    fn a_click_in_the_trimmer_gives_it_the_keys() {
        use crate::keymap::Context;
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        let bars = app.project().media[0].id;
        app.trimmer.open(bars);
        app.show_tab_named("Trimmer");
        frame(&ctx, &mut app, Vec::new());
        click(&ctx, &mut app, egui::pos2(1000.0, 250.0));
        assert_eq!(app.key_focus, Context::Trimmer);
        // A click on the picture puts the cursor there.
        let (clicked, _, _) = app.trimmer.marks();
        let timeline_cursor = app.cursor;
        press(&ctx, &mut app, Modifiers::ALT, Key::ArrowRight);
        press(&ctx, &mut app, NONE, Key::I);
        let (cursor, in_point, _) = app.trimmer.marks();
        let rate = app.frame_rate();
        assert_eq!(
            cursor,
            rate.frame_start(rate.frame_at(clicked) + 1),
            "Alt+Right: next frame"
        );
        assert_eq!(in_point, Some(cursor), "I marks the in point");
        assert_eq!(app.cursor, timeline_cursor, "the timeline cursor stays");
    }

    #[test]
    fn keyboard_runs_menu_commands() {
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        frame(&ctx, &mut app, Vec::new());
        let clips = |app: &BananaApp| app.project().clips().count();
        let before = clips(&app);
        // S splits the selected event at the cursor (4,5 s).
        press(&ctx, &mut app, NONE, Key::S);
        assert_eq!(clips(&app), before + 1);
        assert_eq!(
            Cmd::Undo.label(&app),
            "Undo Split Event",
            "the undo item names the edit"
        );
        press(&ctx, &mut app, CMD, Key::Z);
        assert_eq!(clips(&app), before);
        // I / O set the time selection around the cursor.
        app.seek_seconds(1.0);
        press(&ctx, &mut app, NONE, Key::I);
        app.seek_seconds(3.0);
        press(&ctx, &mut app, NONE, Key::O);
        let r = app.project().loop_region.expect("time selection");
        assert_eq!(r.start, bv_core::Tick::from_seconds(1));
        assert_eq!(r.end(), bv_core::Tick::from_seconds(3));
        // D cycles the editing tool.
        press(&ctx, &mut app, NONE, Key::D);
        assert_eq!(app.tool, Tool::Slip);
        // Alt+D, A loads the audio mixing layout.
        press(&ctx, &mut app, ALT, Key::D);
        press(&ctx, &mut app, NONE, Key::A);
        assert!(app.is_tab_open(Tab::MixingConsole));
        assert_eq!(
            app.tool,
            Tool::Slip,
            "the chord key did not change the tool"
        );
        // Ctrl+Alt+D, 3 saves the layout to slot 3 (Alt+D, 3 loads it).
        press(&ctx, &mut app, Modifiers::CTRL | ALT, Key::D);
        press(&ctx, &mut app, NONE, Key::Num3);
        assert!(app.settings.layouts[3].is_some(), "layout saved to slot 3");
        assert_eq!(app.tool, Tool::Slip, "the digit did not go to a marker");
        // Insert › Audio Envelopes › Volume (Shift+V) on the audio track.
        press(&ctx, &mut app, SHIFT, Key::V);
        let audio = app.current_track(TrackKind::Audio).expect("audio track");
        let has_volume = |app: &BananaApp| {
            app.project()
                .track(audio)
                .is_some_and(|t| t.envelopes.iter().any(|e| e.kind == EnvelopeKind::Volume))
        };
        assert!(has_volume(&app));
        assert_eq!(
            Cmd::AudioEnvelope(EnvelopeKind::Volume).checked(&app),
            Some(true)
        );
        // Ctrl+Q inserts an audio track; F8 and Ctrl+Shift+U are toggles.
        let tracks = app.project().tracks.len();
        press(&ctx, &mut app, CMD, Key::Q);
        assert_eq!(app.project().tracks.len(), tracks + 1);
        let snapping = app.snapping;
        press(&ctx, &mut app, NONE, Key::F8);
        assert_eq!(app.snapping, !snapping);
        press(&ctx, &mut app, CMD_SHIFT, Key::U);
        assert!(app.project().ignore_groups);
    }

    #[test]
    fn edit_cursor_moves_while_playback_continues() {
        use bv_core::Tick;
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        // Options › Preferences › General: Space is Play/Stop.
        app.settings.prefs.space_play_stop = true;
        app.load_demo(); // edit cursor at 4,5 s
        frame(&ctx, &mut app, Vec::new());
        press(&ctx, &mut app, NONE, Key::Space);
        assert!(app.is_playing());
        std::thread::sleep(std::time::Duration::from_millis(200));
        frame(&ctx, &mut app, Vec::new());
        let play = app.playhead.expect("playing");
        assert!(play >= Tick::from_seconds_f64(4.5), "{play:?}");
        // Clicking in the track area moves only the edit cursor.
        app.seek(Tick::from_seconds(2));
        frame(&ctx, &mut app, Vec::new());
        assert!(app.is_playing());
        assert!(
            app.playhead.expect("still playing") >= play,
            "playback did not jump"
        );
        assert_eq!(app.display_time(), app.playhead.expect("playing"));
        // S splits at the edit cursor while the video keeps playing.
        app.selection.clear();
        let clips = app.project().clips().count();
        press(&ctx, &mut app, NONE, Key::S);
        assert_eq!(
            app.project().clips().count(),
            clips + 2,
            "bars and tone split at 2 s"
        );
        assert!(
            app.project()
                .clips()
                .any(|(_, c)| c.start == Tick::from_seconds(2))
        );
        // M marks the playback position (markers on the fly).
        press(&ctx, &mut app, NONE, Key::M);
        let marker = app
            .project()
            .markers
            .iter()
            .map(|m| m.time)
            .max()
            .expect("marker");
        assert!(marker >= app.frame_rate().snap_floor(play), "{marker:?}");
        // Space stops; the edit cursor stays where it was clicked.
        press(&ctx, &mut app, NONE, Key::Space);
        assert!(!app.is_playing());
        assert_eq!(app.cursor, Tick::from_seconds(2));
        assert_eq!(app.playhead, None);
    }

    #[test]
    fn space_pauses_where_playback_is() {
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        frame(&ctx, &mut app, Vec::new());
        press(&ctx, &mut app, NONE, Key::Space);
        assert!(app.is_playing());
        std::thread::sleep(std::time::Duration::from_millis(200));
        frame(&ctx, &mut app, Vec::new());
        let play = app.playhead.expect("playing");
        // VEGAS's default: Play/Pause; the cursor stays where it paused.
        press(&ctx, &mut app, NONE, Key::Space);
        assert!(!app.is_playing());
        assert!(
            app.cursor >= app.frame_rate().snap_floor(play),
            "{:?}",
            app.cursor
        );
        // F12 does what Space does.
        let paused = app.cursor;
        press(&ctx, &mut app, NONE, Key::F12);
        assert!(app.is_playing());
        std::thread::sleep(std::time::Duration::from_millis(200));
        frame(&ctx, &mut app, Vec::new());
        let play = app.playhead.expect("playing");
        press(&ctx, &mut app, NONE, Key::F12);
        assert!(!app.is_playing());
        assert!(app.cursor >= app.frame_rate().snap_floor(play));
        assert!(app.cursor > paused, "{:?}", app.cursor);
    }

    /// Enter does the other of Space's two, as VEGAS's: with Space playing
    /// and pausing (the default) Enter stops, the cursor back where
    /// playback started; with Space playing and stopping, Enter pauses.
    #[test]
    fn enter_does_the_other_of_spaces_two() {
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        frame(&ctx, &mut app, Vec::new());
        let start = app.cursor;
        press(&ctx, &mut app, NONE, Key::Enter);
        assert!(app.is_playing());
        std::thread::sleep(std::time::Duration::from_millis(200));
        frame(&ctx, &mut app, Vec::new());
        assert!(app.playhead.expect("playing") > start);
        press(&ctx, &mut app, NONE, Key::Enter);
        assert!(!app.is_playing());
        assert_eq!(app.cursor, start, "back where playback started");
        app.settings.prefs.space_play_stop = true;
        press(&ctx, &mut app, NONE, Key::Enter);
        assert!(app.is_playing());
        std::thread::sleep(std::time::Duration::from_millis(200));
        frame(&ctx, &mut app, Vec::new());
        press(&ctx, &mut app, NONE, Key::Enter);
        assert!(!app.is_playing());
        assert!(app.cursor > start, "paused where it was: {:?}", app.cursor);
    }

    /// The timeline's playback goes on past the project's end until it is
    /// stopped, as VEGAS's (without Loop Playback).
    #[test]
    fn playback_goes_on_past_the_end() {
        use bv_core::Tick;
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        frame(&ctx, &mut app, Vec::new());
        let end = app.project().duration();
        app.cursor = end - Tick::from_seconds_f64(0.1);
        app.start_playback(app.cursor);
        std::thread::sleep(std::time::Duration::from_millis(400));
        frame(&ctx, &mut app, Vec::new());
        assert!(app.is_playing(), "still playing past the end");
        assert!(app.playhead.expect("playing") > end);
        app.stop();
        assert!(!app.is_playing());
    }

    #[test]
    fn the_playback_needle_held_and_let_go() {
        use bv_core::Tick;
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo(); // edit cursor at 4,5 s
        frame(&ctx, &mut app, Vec::new());
        press(&ctx, &mut app, NONE, Key::Space);
        let cursor = app.cursor;
        // Grabbed: playback waits, the needle and the preview follow it.
        app.hold_playback();
        assert!(app.is_playing() && !app.engine.is_playing());
        app.move_playhead(Tick::from_seconds(1));
        frame(&ctx, &mut app, Vec::new());
        assert_eq!(app.display_time(), Tick::from_seconds(1));
        assert_eq!(app.cursor, cursor, "the edit cursor stays");
        // Let go: playback goes on from there.
        app.release_playback();
        assert!(app.engine.is_playing());
        frame(&ctx, &mut app, Vec::new());
        let at = app.playhead.expect("playing");
        assert!(
            at >= Tick::from_seconds(1) && at < Tick::from_seconds(3),
            "{at:?}"
        );
        app.stop();
    }

    #[test]
    fn menu_shortcuts_come_from_the_keyboard_map() {
        let km = crate::keymap::Keymap::in_memory();
        let text = |c| km.text_for(crate::keymap::Action::Cmd(c));
        assert_eq!(text(Cmd::Redo), "Ctrl+Y; Ctrl+Shift+Z");
        assert_eq!(text(Cmd::FrameLeft), "Alt+Left");
        assert_eq!(text(Cmd::Layout(LayoutPreset::AudioMixing)), "Alt+D, A");
        assert_eq!(text(Cmd::Save), "Ctrl+S");
        assert_eq!(text(Cmd::Window(Tab::Explorer)), "Alt+1");
    }
}
