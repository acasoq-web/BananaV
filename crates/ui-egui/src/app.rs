//! Estado da aplicação e as ações que menus, atalhos e painéis disparam.

use crate::i18n::tr;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bv_core::{Tick, TimeRange};
use bv_engine::{AudioStatus, Engine, PlayOptions};
use bv_timeline::{
    ClipId, ClipboardData, Generator, History, MarkerId, MediaId, MediaInfo, Project, TrackId,
    TrackKind,
};

use crate::dialogs::Dialogs;
use crate::dock::{self, Tab};
use crate::egui::{self, Key, KeyboardShortcut, Modifiers};
use crate::tabs::explorer::ExplorerState;
use crate::tabs::master::MeterState;
use crate::tabs::preview::PreviewState;
use crate::tabs::trimmer::TrimmerState;
use crate::timeline::TimelineView;
use crate::{menus, theme};

pub(crate) mod actions;
mod cd_actions;
mod display;
pub(crate) mod key_actions;
pub(crate) mod more_actions;

/// Something to do once the user has answered "Save changes?".
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Pending {
    New,
    Open,
    OpenPath(PathBuf),
    /// A new project of this size and rate (the welcome screen's Create
    /// Project), in Project Properties when `advanced`.
    NewSized {
        width: u32,
        height: u32,
        rate: bv_core::FrameRate,
        advanced: bool,
    },
    Close,
    Demo,
    Exit,
    /// File › Import › OpenTimelineIO: a timeline as a new project.
    ImportOtio,
    /// File › Import › Final Cut Pro 7/DaVinci Resolve XML (`false`) or
    /// FCPXML (`true`): its sequence as a new project.
    ImportFcp(bool),
}

/// Editing tool active on the timeline (Edit › Editing Tool).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// Move, trim, fades and level.
    Edit,
    /// Dragging an event puts it between others, which close up its place
    /// (Shuffle Events).
    Shuffle,
    /// Dragging an event shows another part of its media (slip).
    Slip,
    /// Dragging an event moves it while its neighbours stay attached (slide).
    Slide,
    /// Dragging an event edge changes its playback rate (time stretch).
    Stretch,
    /// Clicking an event splits it; edges still trim.
    SplitTrim,
    /// Dragging sets level and fades without moving events.
    Envelope,
    /// Selection rectangle.
    Select,
    /// Click zooms in; right button zooms out.
    Zoom,
}

impl Tool {
    /// Order used by "Next tool" (D) and "Previous tool" (Shift+D).
    pub const CYCLE: [Tool; 8] = [
        Tool::Edit,
        Tool::Slip,
        Tool::Slide,
        Tool::Stretch,
        Tool::SplitTrim,
        Tool::Envelope,
        Tool::Select,
        Tool::Zoom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Edit => tr("Normal Edit Tool"),
            Tool::Shuffle => tr("Shuffle Tool"),
            Tool::Slip => tr("Slip Tool"),
            Tool::Slide => tr("Slide Tool"),
            Tool::Stretch => tr("Time Stretch/Compress Tool"),
            Tool::SplitTrim => tr("Split Trim Tool"),
            Tool::Envelope => "Envelope",
            Tool::Select => tr("Selection"),
            Tool::Zoom => "Zoom",
        }
    }

    pub fn next(self, step: isize) -> Tool {
        let n = Tool::CYCLE.len() as isize;
        let i = Tool::CYCLE.iter().position(|&t| t == self).unwrap_or(0) as isize;
        Tool::CYCLE[(i + step).rem_euclid(n) as usize]
    }
}

/// O que está sendo arrastado para a timeline.
#[derive(Clone, Debug, PartialEq)]
pub enum MediaDrag {
    Media(MediaId),
    /// Part of a media of the project (a Project Media thumbnail's, the
    /// Trimmer's selection).
    MediaPart(MediaId, TimeRange),
    /// Several media of Project Media picked together: the one dragged
    /// first, then the others in the window's order.
    Many(Vec<MediaId>),
    /// Um arquivo do Explorador (importado ao soltar).
    File(PathBuf),
    /// Part of a file (a region the Explorer shows).
    FilePart(PathBuf, TimeRange),
    /// Uma mídia gerada (criada ao soltar).
    Generator(Generator),
    /// Files dragged in from the system (Explorer, the desktop).
    Files(Vec<PathBuf>),
    Transition(bv_timeline::Transition),
    /// An OFX plug-in's transition.
    PluginTransition(Box<bv_timeline::PluginFx>),
    /// A video effect with its preset's settings.
    Effect(bv_timeline::Effect),
    /// A filter package: its name and chain.
    Package(String, Vec<bv_timeline::Effect>),
    /// Storyboard cells (a storyboard bin's, or those picked in it).
    Cells(Vec<bv_timeline::StoryCell>),
}

impl MediaDrag {
    pub(crate) fn label(&self, project: &Project) -> String {
        match self {
            MediaDrag::Media(id) | MediaDrag::MediaPart(id, _) => project
                .media(*id)
                .cloned()
                .or_else(|| project.subclip_info(*id))
                .map_or(tr("Media").into(), |m| crate::i18n::media_name(&m)),
            MediaDrag::File(p) | MediaDrag::FilePart(p, _) => p
                .file_name()
                .map_or(tr("File").into(), |n| n.to_string_lossy().to_string()),
            MediaDrag::Generator(g) => crate::i18n::generator_name(g),
            MediaDrag::Files(paths) if paths.len() == 1 => paths[0]
                .file_name()
                .map_or(tr("File").into(), |n| n.to_string_lossy().to_string()),
            MediaDrag::Files(paths) => trn!(paths.len(), "{} file", "{} files", paths.len()),
            MediaDrag::Many(ids) => trn!(ids.len(), "{} media", "{} media", ids.len()),
            MediaDrag::Transition(t) => trf!("Transition: {}", tr(t.name())),
            MediaDrag::PluginTransition(fx) => trf!("Transition: {}", fx.name),
            MediaDrag::Effect(e) => trf!("Effect: {}", crate::ofx::fx_name(e)),
            MediaDrag::Package(name, _) => trf!("Filter package: {}", name),
            MediaDrag::Cells(cells) => trn!(
                cells.len(),
                "{} storyboard cell",
                "{} storyboard cells",
                cells.len()
            ),
        }
    }

    /// Arrastos que viram clipes (ocupam tempo na timeline).
    pub(crate) fn is_media(&self) -> bool {
        matches!(
            self,
            MediaDrag::Media(_)
                | MediaDrag::MediaPart(..)
                | MediaDrag::File(_)
                | MediaDrag::FilePart(..)
                | MediaDrag::Many(_)
                | MediaDrag::Generator(_)
                | MediaDrag::Files(_)
                | MediaDrag::Cells(_)
        )
    }
}

pub struct BananaApp {
    pub(crate) history: History<Project>,
    pub(crate) engine: Engine,
    /// Posição do cursor (playhead) na timeline.
    pub(crate) cursor: Tick,
    /// Playback position while playing; the edit cursor (`cursor`) stays
    /// put meanwhile, like VEGAS's two cursors.
    pub(crate) playhead: Option<Tick>,
    /// The playback needle is held by the mouse: playback waits, and goes
    /// on from where the needle is let go.
    pub(crate) held_playback: bool,
    pub(crate) selection: BTreeSet<ClipId>,
    /// The events the user picked — clicked, Ctrl+clicked, Shift+clicked,
    /// boxed — and the selection they were picked in. VEGAS outlines them
    /// in yellow, and the members of their groups selected along with them
    /// in blue. A selection changed in other ways is all picked.
    pub(crate) picked: (BTreeSet<ClipId>, BTreeSet<ClipId>),
    pub(crate) auto_ripple: bool,
    pub(crate) snapping: bool,
    pub(crate) quantize: bool,
    pub(crate) loop_enabled: bool,
    pub(crate) play_rate: f64,
    pub(crate) tool: Tool,
    pub(crate) media_drag: Option<MediaDrag>,
    /// The media drag began with the right mouse button: letting it go
    /// on the timeline asks how to add it (Add as Takes among the ways).
    pub(crate) media_drag_secondary: bool,
    pub(crate) clipboard: Option<ClipboardData>,
    pub(crate) timeline: TimelineView,
    pub(crate) preview: PreviewState,
    pub(crate) trimmer: TrimmerState,
    pub(crate) explorer: ExplorerState,
    /// Project Media's bins and view.
    pub(crate) media_window: crate::tabs::media::MediaWindow,
    pub(crate) meters: MeterState,
    /// Media previews (Start Preview, the Trimmer's Play) and the Preview
    /// fader's meter.
    pub(crate) previews: crate::media_preview::PreviewState,
    /// The meters in the audio track headers.
    pub(crate) track_meters: std::collections::HashMap<bv_timeline::TrackId, MeterState>,
    /// The audio busses' and assignable effects chains' meters (the Mixing
    /// Console).
    pub(crate) strip_meters: std::collections::HashMap<bv_timeline::Strip, MeterState>,
    /// The Mixing Console's VU/PPM meters as shown, and whether the console
    /// was drawn this frame (the mixer computes them only while it shows
    /// them).
    pub(crate) vu: crate::vu_meters::VuDisplay,
    pub(crate) console_drawn: bool,
    /// The Color Grading panel's level, section and views.
    pub(crate) color_panel: crate::tabs::color::ColorPanel,
    /// Shuttle rate of the scrub control under the track list (0 at rest).
    pub(crate) scrub_rate: f64,
    pub(crate) dialogs: Dialogs,
    pub(crate) textures: crate::tabs::TextureCache,
    pub(crate) clip_windows: crate::clip_windows::ClipWindows,
    /// The filter packages (saved effect chains) and their dialogs.
    pub(crate) fx_packages: crate::fx_packages::FxPackages,
    /// The docked window the last click was in: it has the keys.
    pub(crate) keys_tab: Option<crate::dock::Tab>,
    /// The Track Motion window (a track's, or a parent's Parent Motion).
    pub(crate) track_motion: Option<crate::track_motion::TrackMotionWindow>,
    /// Tools › Multicamera › Enable Multicamera Editing.
    pub(crate) multicam: bool,
    /// The event picked first, by a click (Ctrl+clicks add others): the
    /// target Sync … to Align Events lines the others up with.
    pub(crate) first_pick: Option<ClipId>,
    /// A Pan/Crop window showed this frame: its picture's readers stay
    /// (`Engine::source_picture`).
    pub(crate) source_watched: bool,
    /// Sync Mixed Audio to Align Events under way.
    pub(crate) audio_sync: Option<crate::multicam::PendingSync>,
    /// The Take Chooser and Rename Active Take, when open.
    pub(crate) take_chooser: Option<crate::takes::TakeChooser>,
    pub(crate) take_rename: Option<crate::takes::TakeRename>,
    /// Pedido para a timeline mostrar o projeto inteiro no próximo quadro.
    pub(crate) fit_timeline: bool,
    /// Mídia selecionada no painel de mídias.
    pub(crate) selected_media: Option<MediaId>,
    pub(crate) dock: Option<egui_dock::DockState<Tab>>,
    pub(crate) project_path: Option<PathBuf>,
    pub(crate) saved: Project,
    status: Option<(String, Instant, bool)>,
    /// Waiting for the "Save changes?" answer before doing this.
    confirm: Option<Pending>,
    /// A question waiting for its answer (see `ask.rs`), and the ones
    /// asked after it.
    pub(crate) ask: Option<crate::ask::Ask>,
    pub(crate) asks_later: std::collections::VecDeque<crate::ask::Ask>,
    /// The project's copies saved automatically.
    pub(crate) autosave: crate::autosave::Autosave,
    /// BananaV was the active program at the last frame.
    was_active: bool,
    /// The nested projects' files when BananaV stopped being the active
    /// program (`nested_ui`).
    pub(crate) files_seen: Option<crate::nested_ui::FileStamps>,
    /// TrackMotion context keys for the Track Motion window.
    pub(crate) track_motion_keys: Vec<crate::track_motion::TrackMotionKey>,
    /// The parent timelines waiting while a nested one is open
    /// (`nested_timelines`), the nearest last.
    pub(crate) parent_timelines: Vec<crate::nested_timelines::ParentTimeline>,
    /// No joystick was found: not asked again before then.
    joystick_retry: Option<Instant>,
    /// The windows refusing input while the render's window is up.
    input_block: Option<bv_platform::InputBlock>,
    /// Recording into the armed tracks, and the Recorded Files window after.
    pub(crate) recording: Option<crate::recording::Recording>,
    pub(crate) recorded_files: Option<crate::recording::RecordedFiles>,
    /// The input devices found, and when.
    pub(crate) input_devices: Option<(Instant, Vec<bv_engine::InputDevice>)>,
    /// Options › Metronome: a click on each beat while playing and
    /// recording.
    pub(crate) metronome: bool,
    /// Video Preview on External Monitor, while it is on.
    pub(crate) external: Option<crate::external_monitor::ExternalMonitor>,
    /// Options › Preferences › Preview Device › Identify Displays: until
    /// when the displays show their numbers.
    pub(crate) identify_until: Option<Instant>,
    /// The MIDI ports open (MIDI Thru, timecode, clock, control surfaces).
    pub(crate) midi: crate::midi::MidiPorts,
    /// The MIDI input Configure Generic Control learns from (asked for
    /// each frame while its Learn is on).
    pub(crate) midi_learn: Option<String>,
    /// Options › Timecode: Generate MIDI Timecode, Generate MIDI Clock,
    /// Trigger from MIDI Timecode; following it.
    pub(crate) generate_mtc: bool,
    pub(crate) generate_clock: bool,
    pub(crate) trigger_mtc: bool,
    pub(crate) chase: crate::sync::Chase,
    /// What the time display shows (its right-click menu).
    pub(crate) time_display: crate::sync::TimeDisplay,
    /// The control surfaces' state (Options › External Control).
    pub(crate) surfaces: Vec<crate::surface::Mackie>,
    /// The Surround Panner's point recorded while playing back.
    pub(crate) surround_write: Option<crate::tabs::surround::SurroundWrite>,
    /// The Motion Tracking window.
    pub(crate) motion: crate::motion_tracking::MotionWindow,
    /// File › Screen Capture's window.
    pub(crate) screen_capture: crate::screen_capture::ScreenCaptureWindow,
    /// Video Stabilization's motion being analyzed, and how far.
    pub(crate) analysis: Option<(crate::ofx::AnalysisAsk, f32)>,
    /// A Bézier Masking's mask being tracked (its Start).
    pub(crate) mask_tracking: Option<crate::mask_tracking::MaskTracking>,
    /// Configure Mackie Control is open (this frame): the surfaces show
    /// their channel mapping.
    pub(crate) mackie_mapping_shown: bool,
    /// File › Import › DVD Camcorder Disc and View › Window › Device
    /// Explorer (Options › Preferences › Deprecated Features).
    pub(crate) dvd_import: crate::camera_media::DvdImport,
    pub(crate) device_explorer: crate::camera_media::DeviceExplorer,
    /// An OpenTimelineIO bundle being packed or unpacked.
    pub(crate) otio_job: Option<crate::otio_ui::Job>,
    /// File › Export › Project Archive's dialog, while open.
    pub(crate) archive_dialog: Option<crate::archive::ArchiveDialog>,
    /// Media files being copied (an archive, Save As's copies).
    pub(crate) copy_job: Option<crate::archive::CopyJob>,
    /// The input busses' levels, as live input last measured them.
    pub(crate) input_bus_levels: std::collections::HashMap<bv_timeline::InputBusId, [f32; 2]>,
    /// Save As's Copy Media dialog, while open.
    pub(crate) copy_media_dialog: Option<crate::archive::CopyMediaDialog>,
    /// Transport › Cursor Preview: the part being played (playback stops
    /// at its end).
    pub(crate) cursor_preview: Option<TimeRange>,
    /// Automation being recorded (a control moved while playing).
    pub(crate) automation_pass: Option<crate::automation::AutomationPass>,
    /// The waveforms recorded so far, by track (Options › Preferences ›
    /// Audio: "Waveform display while recording").
    pub(crate) recording_waves: std::collections::HashMap<TrackId, bv_engine::RecordingWave>,
    /// Tools › Build Dynamic RAM Preview is drawing frames.
    pub(crate) building_ram_preview: bool,
    /// Media larger than 4K being timed for an automatic proxy.
    pub(crate) proxy_checks: Vec<(MediaId, std::thread::JoinHandle<bool>)>,
    /// A track's fade color being chosen (the track, or the video bus:
    /// `None`; 0 top / 1 bottom).
    pub(crate) track_color: Option<(Option<TrackId>, usize, crate::color_picker::ColorPicker)>,
    /// The Media Generator's Custom Color…: the solid color to place at
    /// the cursor.
    pub(crate) generator_color: Option<crate::color_picker::ColorPicker>,
    /// The Time Display's Text Color › Custom… (`false`) or Background
    /// Color › Custom… (`true`) being picked.
    pub(crate) time_display_color: Option<(bool, crate::color_picker::ColorPicker)>,
    /// Auto Normalize: what each event's gain was worked out for (its part
    /// of its media and the target level).
    pub(crate) auto_normalized:
        std::collections::HashMap<ClipId, crate::app::actions::NormalizedFor>,
    /// The Video Media Generators window, for a generated event.
    pub(crate) generated_media: Option<crate::generated_media::GeneratedMediaWindow>,
    /// The Master Bus's width once its edge was dragged (see
    /// `dock::MASTER_WIDTH`).
    pub(crate) master_width: Option<f32>,
    /// The Media Generator, Transitions and Video FX windows.
    pub(crate) choosers: crate::tabs::chooser::Choosers,
    /// The Video FX presets' pictures.
    pub(crate) fx_previews: crate::tabs::effects::Previews,
    /// The Compositors window's preset pictures.
    pub(crate) compositor_previews: crate::tabs::compositors::Previews,
    /// The track (and which mode: `true` its Parent Composite Mode) whose
    /// Custom compositing asked for a compositor.
    pub(crate) compositor_pick: Option<(TrackId, bool)>,
    /// The compositor settings' window, when open.
    pub(crate) compositor_window: Option<crate::compositors::CompositorWindow>,
    /// Beat Detection's and Tempo Detection's dialogs, while open.
    pub(crate) beat_dialog: Option<crate::beats_ui::BeatDialog>,
    pub(crate) tempo_dialog: Option<crate::beats_ui::TempoDialog>,
    /// Tools › Audio › Apply Non-Real-Time Event FX's dialog, while open.
    pub(crate) nrt_dialog: Option<crate::nonrealtime_ui::NrtDialog>,
    /// Set Default Track Properties, while it is open; and the history as
    /// the new tracks' defaults last saw it.
    pub(crate) track_defaults_dialog: Option<crate::track_defaults::DefaultsDialog>,
    /// More › Edit Visible Button Set, while it is open.
    pub(crate) header_buttons_dialog: Option<crate::header_buttons::ButtonsDialog>,
    /// The transport bars' Edit Visible Button Set, while it is open.
    pub(crate) transport_dialog: Option<crate::transport_buttons::TransportDialog>,
    /// A Render to New Track running: where its file's events go.
    pub(crate) new_track_render: Option<Tick>,
    /// The FX Automation Chooser, while it is open.
    pub(crate) fx_automation: Option<crate::fx_automation_ui::FxAutomationChooser>,
    pub(crate) defaults_watch: crate::track_defaults::DefaultsWatch,
    /// Events just placed of generated media others show: a copy each or
    /// a reference (VEGAS's question).
    pub(crate) generated_copies: Option<crate::generated_copies::Question>,
    /// The Auto Ducking dialog's track or event, while it is open.
    pub(crate) duck_dialog: Option<crate::ducking_ui::DuckTarget>,
    /// The Transitions window's pictures of plug-ins' transitions.
    pub(crate) transition_previews: crate::tabs::transitions::Previews,
    /// OFX plug-ins: the scan, the instances answering effects' settings.
    pub(crate) ofx: crate::ofx::OfxUi,
    /// VST 3 plug-ins: those found, the instances of effects' pages.
    pub(crate) vst: crate::vst_ui::VstUi,
    /// Tools › Selectively Prerender Video's window.
    pub(crate) prerender_dialog: Option<crate::prerender_ui::PrerenderDialog>,
    /// Media files being copied from removable drives: the file, and the
    /// copy when it is done.
    removable_copies: Vec<(String, std::thread::JoinHandle<std::io::Result<PathBuf>>)>,
    /// The media files as they were when BananaV let go of them (path, last
    /// change, size): changed ones are read again.
    released_media: Option<Vec<(String, Option<std::time::SystemTime>, u64)>>,
    allow_close: bool,
    /// Close the window at the end of this frame.
    exit_now: bool,
    title: String,
    /// The audio output's failure noted down last (`logs::audio`).
    last_audio_error: Option<String>,
    pub(crate) show_toolbar: bool,
    pub(crate) show_status_bar: bool,
    /// View › Window Layouts › Timeline on top.
    pub(crate) timeline_top: bool,
    /// Options › Preferences › Display: the interface type and the scale
    /// last given to egui.
    look_applied: Option<theme::Interface>,
    zoom_applied: Option<f32>,
    /// "Automatically hide docking area": the docking area shows now, and
    /// since when the pointer has been away from it.
    pub(crate) dock_peek: bool,
    /// F11 and its kin: the docking areas hidden (the layout as it was).
    pub(crate) docks_hidden: Option<crate::dock::Hidden>,
    /// A docked window maximized by its tab bar's button.
    pub(crate) dock_maximized: Option<crate::dock::Maximized>,
    /// What a key or a button asked of the layout, done where it is drawn.
    pub(crate) dock_request: Option<crate::dock::DockRequest>,
    /// Ctrl+Alt+Shift+F11: the timeline maximized (the docking areas
    /// hidden and the track headers folded by it).
    pub(crate) timeline_maximized: bool,
    dock_away_since: Option<f64>,
    /// "Show system statistics in the status bar".
    stats: Option<display::SystemStats>,
    /// The status bar's record time, and when it was looked up.
    record_time: Option<(Instant, Option<String>)>,
    pub(crate) settings: crate::settings::UiSettings,
    /// Saved layout slot last loaded or saved (for "Save layout" / "Reload").
    pub(crate) layout_slot: Option<usize>,
    /// The main window as last written to the settings as the session
    /// (see [`BananaApp::keep_session`]); `None` while it is not kept: the
    /// tests' apps never write the user's settings.
    session_kept: Option<crate::settings::Session>,
    /// When the window was last compared with the session kept.
    session_checked: Instant,
    pub(crate) menu_dialogs: crate::menu_dialogs::MenuDialogs,
    /// Earlier time selections, for "Restore previous time selection".
    selection_history: Vec<Option<TimeRange>>,
    last_selection: Option<TimeRange>,
    restoring_selection: bool,
    /// Alt+D was pressed: the next key picks a window layout.
    /// First key of a two-key shortcut waiting for the second one.
    pub(crate) key_chord: Option<(crate::keymap::Binding, Instant)>,
    /// The V key's press was seen (`keymap::dispatch`: a Ctrl+V with no
    /// text on the clipboard arrives only as its release).
    pub(crate) v_down: bool,
    /// Give the keyboard back to the timeline (View › Focus to Timeline).
    focus_timeline: bool,
    /// Options › Snap to Grid / Markers / All Events.
    pub(crate) snap_grid: bool,
    pub(crate) snap_markers: bool,
    pub(crate) snap_events: bool,
    /// Options › Grid Spacing.
    pub(crate) grid: crate::timefmt::GridSpacing,
    /// Options › Ripple: what an auto-ripple delete moves.
    pub(crate) ripple_scope: bv_timeline::RippleScope,
    /// Options › Ruler Format (`None` follows the project frame rate).
    /// Files being dragged in from the system.
    pub(crate) os_drag: Option<OsDrag>,
    /// Keyboard shortcuts in use (Options › Customize Keyboard).
    pub(crate) keymap: crate::keymap::Keymap,
    /// Which keyboard context gets the keys: TrackList after a click on the
    /// track headers, TrackView after a click in the timeline.
    pub(crate) key_focus: crate::keymap::Context,
    /// The selected track (highlighted header): the one track commands,
    /// Insert › envelopes and the track-list keys act on.
    pub(crate) selected_track: Option<TrackId>,
    /// The tracks picked with Shift and Ctrl on their headers (with the
    /// selected one: `track_selection`).
    pub(crate) track_picks: Vec<TrackId>,
    /// The features each Video Stabilization's analysis followed this
    /// session (its Show Points), by effect.
    pub(crate) stabilization_points: std::collections::HashMap<
        bv_engine::plugins::EffectKey,
        std::sync::Arc<bv_engine::FeatureTracks>,
    >,
    /// What the Mixing Console shows.
    pub(crate) console: crate::tabs::console::ConsoleView,
    /// The Loudness Meters' last readings.
    pub(crate) loudness_view: crate::tabs::loudness::LoudnessView,
    /// Project Notes' state.
    pub(crate) notes_view: crate::tabs::notes::NotesView,
    /// Edit Details' state.
    pub(crate) details: crate::tabs::details::DetailsView,
    /// Edit Details' shortcut menu's Paste waits for the clipboard's text.
    pub(crate) details_paste_pending: bool,
    /// The text of the paste the keyboard map took last (Edit Details'
    /// Paste reads it).
    pub(crate) pasted_text: Option<String>,
    /// Text a window copied (Edit Details' Copy), for the clipboard at
    /// the frame's end.
    pub(crate) to_clipboard: Option<String>,
    /// The Surround Panner's state.
    pub(crate) surround_view: crate::tabs::surround::SurroundView,
    /// Media info of dragged files (probed once), so the timeline can show
    /// where and how long they will be.
    drag_probe: std::collections::HashMap<PathBuf, Option<MediaInfo>>,
}

#[cfg(test)]
thread_local! {
    /// Cursor position the tests pretend the OS reports.
    pub(crate) static TEST_CURSOR: std::cell::Cell<Option<(i32, i32)>> =
        const { std::cell::Cell::new(None) };
}

fn os_cursor() -> Option<(i32, i32)> {
    #[cfg(test)]
    {
        TEST_CURSOR.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        bv_platform::cursor_screen_position()
    }
}

/// A drag of files from the system. The window library tells which files
/// but not where the mouse is, so the position comes from the OS.
#[derive(Clone, Debug)]
pub(crate) struct OsDrag {
    pub paths: Vec<PathBuf>,
    /// Mouse position in this window (points).
    pub pointer: Option<egui::Pos2>,
    /// The files were dropped this frame.
    pub dropped: bool,
}

impl std::fmt::Debug for BananaApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BananaApp")
            .field("cursor", &self.cursor)
            .finish()
    }
}

impl BananaApp {
    pub fn new(ctx: egui::Context) -> BananaApp {
        let repaint_ctx = ctx.clone();
        let engine = Engine::new(Arc::new(move || repaint_ctx.request_repaint()));
        let project = Project::default();
        let mut app = BananaApp {
            history: History::new(project.clone()),
            engine,
            cursor: Tick::ZERO,
            playhead: None,
            held_playback: false,
            selection: BTreeSet::new(),
            picked: (BTreeSet::new(), BTreeSet::new()),
            auto_ripple: false,
            snapping: true,
            quantize: true,
            loop_enabled: false,
            play_rate: 1.0,
            tool: Tool::Edit,
            media_drag: None,
            media_drag_secondary: false,
            clipboard: None,
            timeline: TimelineView::default(),
            preview: PreviewState::default(),
            trimmer: TrimmerState::default(),
            explorer: ExplorerState::new(),
            media_window: Default::default(),
            meters: MeterState::default(),
            previews: Default::default(),
            track_meters: std::collections::HashMap::new(),
            strip_meters: std::collections::HashMap::new(),
            vu: Default::default(),
            console_drawn: false,
            color_panel: Default::default(),
            scrub_rate: 0.0,
            dialogs: Dialogs::default(),
            textures: crate::tabs::TextureCache::default(),
            clip_windows: crate::clip_windows::ClipWindows::default(),
            fx_packages: if cfg!(test) {
                crate::fx_packages::FxPackages::in_memory()
            } else {
                crate::fx_packages::FxPackages::load()
            },
            keys_tab: None,
            track_motion: None,
            multicam: false,
            first_pick: None,
            source_watched: false,
            audio_sync: None,
            take_chooser: None,
            take_rename: None,
            fit_timeline: false,
            selected_media: None,
            dock: Some(dock::default_layout()),
            project_path: None,
            saved: project,
            status: None,
            confirm: None,
            ask: None,
            asks_later: std::collections::VecDeque::new(),
            autosave: crate::autosave::Autosave::default(),
            was_active: true,
            files_seen: None,
            track_motion_keys: Vec::new(),
            parent_timelines: Vec::new(),
            joystick_retry: None,
            input_block: None,
            recording: None,
            recorded_files: None,
            input_devices: None,
            recording_waves: std::collections::HashMap::new(),
            metronome: false,
            external: None,
            identify_until: None,
            midi: Default::default(),
            midi_learn: None,
            generate_mtc: false,
            generate_clock: false,
            trigger_mtc: false,
            chase: Default::default(),
            time_display: Default::default(),
            surfaces: Vec::new(),
            mackie_mapping_shown: false,
            motion: Default::default(),
            screen_capture: Default::default(),
            analysis: None,
            mask_tracking: None,
            surround_write: None,
            dvd_import: Default::default(),
            device_explorer: Default::default(),
            otio_job: None,
            archive_dialog: None,
            copy_job: None,
            input_bus_levels: Default::default(),
            copy_media_dialog: None,
            cursor_preview: None,
            automation_pass: None,
            building_ram_preview: false,
            track_color: None,
            generator_color: None,
            time_display_color: None,
            auto_normalized: Default::default(),
            generated_media: None,
            master_width: None,
            choosers: Default::default(),
            fx_previews: Default::default(),
            compositor_previews: Default::default(),
            compositor_pick: None,
            compositor_window: None,
            beat_dialog: None,
            tempo_dialog: None,
            nrt_dialog: None,
            track_defaults_dialog: None,
            header_buttons_dialog: None,
            transport_dialog: None,
            new_track_render: None,
            fx_automation: None,
            defaults_watch: Default::default(),
            generated_copies: None,
            duck_dialog: None,
            transition_previews: Default::default(),
            ofx: crate::ofx::OfxUi::new(ctx.clone()),
            vst: Default::default(),
            prerender_dialog: None,
            proxy_checks: Vec::new(),
            released_media: None,
            removable_copies: Vec::new(),
            allow_close: false,
            exit_now: false,
            title: String::new(),
            last_audio_error: None,
            show_toolbar: true,
            show_status_bar: true,
            timeline_top: false,
            look_applied: None,
            zoom_applied: None,
            dock_peek: false,
            docks_hidden: None,
            dock_maximized: None,
            dock_request: None,
            timeline_maximized: false,
            dock_away_since: None,
            stats: None,
            record_time: None,
            settings: if cfg!(test) {
                crate::settings::UiSettings::in_memory()
            } else {
                crate::settings::UiSettings::load()
            },
            layout_slot: None,
            session_kept: None,
            session_checked: Instant::now(),
            menu_dialogs: crate::menu_dialogs::MenuDialogs::default(),
            selection_history: Vec::new(),
            last_selection: None,
            restoring_selection: false,
            key_chord: None,
            v_down: false,
            focus_timeline: false,
            snap_grid: false,
            snap_markers: true,
            snap_events: true,
            grid: crate::timefmt::GridSpacing::RulerMarks,
            ripple_scope: bv_timeline::RippleScope::AffectedTracks,
            os_drag: None,
            keymap: if cfg!(test) {
                crate::keymap::Keymap::in_memory()
            } else {
                crate::keymap::Keymap::load()
            },
            drag_probe: std::collections::HashMap::new(),
            key_focus: crate::keymap::Context::TrackView,
            selected_track: None,
            track_picks: Vec::new(),
            stabilization_points: Default::default(),
            console: crate::tabs::console::ConsoleView::default(),
            loudness_view: Default::default(),
            notes_view: Default::default(),
            details: Default::default(),
            details_paste_pending: false,
            pasted_text: None,
            to_clipboard: None,
            surround_view: Default::default(),
        };
        // Unit tests never see it (on a clean computer it would open).
        app.menu_dialogs.welcome.open = app.settings.show_welcome && !cfg!(test);
        app.preview.restore(&app.settings.preview);
        crate::i18n::set_language(app.settings.language);
        let p = &app.settings.prefs;
        theme::set_interface(p.interface);
        theme::set_button_highlight(p.button_highlight);
        theme::set_icon_strength(p.icon_strength);
        // The graphics card opens in the background; until then (and if it
        // fails) the processor composites.
        app.engine
            .set_hardware_decoding(app.settings.decoding.clone());
        app.engine.set_gpu(app.settings.video_gpu.clone());
        bv_engine::Engine::set_render_threads(app.settings.prefs.render_threads as usize);
        let prefs = &app.settings.prefs;
        bv_engine::Engine::set_peak_files(!prefs.no_peak_files, prefs.hide_peak_files);
        app.engine
            .set_ram_preview((app.settings.prefs.ram_preview_mb as usize) << 20);
        bv_engine::Engine::set_keep_bypassed_fx(prefs.keep_bypassed_fx);
        bv_engine::Engine::set_aces_on_gpu(prefs.aces_on_gpu);
        bv_engine::Engine::set_true_peak_blocking(app.settings.loudness.blocking);
        app.apply_audio_prefs();
        let p = &app.settings.prefs;
        crate::logs::set_levels(p.log_plugins, p.log_reads, p.log_renders);
        if !app.settings.prefs.timeline_at_bottom {
            app.place_timeline(false);
        }
        let p = app.blank_project();
        app.history.reset(p.clone());
        app.saved = p;
        app
    }

    /// A new project, without tracks as VEGAS's (the first media brings
    /// them), with the settings new projects start with (File › Properties
    /// › "Start all new projects with these settings").
    pub(crate) fn blank_project(&self) -> Project {
        let mut p = match &self.settings.new_project {
            Some(d) => {
                let mut p = Project::new(d.settings);
                p.prerender_folder = d.prerender_folder.clone();
                p.recorded_folder = d.recorded_folder.clone();
                p.ruler_offset = d.ruler_offset;
                p.summary = d.summary.clone();
                p
            }
            None => Project::default(),
        };
        p.tracks.clear();
        p
    }

    /// Media Generator › Solid Color › Custom Color…: once chosen, the
    /// color goes to the cursor.
    fn generator_color_dialog(&mut self, ctx: &egui::Context) {
        let Some(picker) = &mut self.generator_color else {
            return;
        };
        match picker.show(ctx) {
            crate::color_picker::Outcome::Open => {}
            crate::color_picker::Outcome::Cancelled => self.generator_color = None,
            crate::color_picker::Outcome::Chosen([r, g, b, a]) => {
                self.generator_color = None;
                let f = |v: u8| v as f32 / 255.0;
                let at = self.cursor;
                self.add_generator(
                    bv_timeline::Generator::SolidColor {
                        rgba: [f(r), f(g), f(b), f(a)],
                    },
                    at,
                    None,
                );
            }
        }
    }

    /// The Time Display's Text Color / Background Color › Custom…: OK sets
    /// the color for the interface type in use (each type keeps its own,
    /// as VEGAS's color schemes do).
    fn time_display_color_dialog(&mut self, ctx: &egui::Context) {
        let Some((background, picker)) = &mut self.time_display_color else {
            return;
        };
        let background = *background;
        match picker.show(ctx) {
            crate::color_picker::Outcome::Open => {}
            crate::color_picker::Outcome::Cancelled => self.time_display_color = None,
            crate::color_picker::Outcome::Chosen(rgba) => {
                self.time_display_color = None;
                let key = format!("{:?}", crate::theme::interface());
                let colors = self.settings.time_display_colors.entry(key).or_default();
                if background {
                    colors.background = Some(rgba);
                } else {
                    colors.text = Some(rgba);
                }
                let _ = self.settings.save();
            }
        }
    }

    /// Track header › Fade Colors › Top… / Bottom… (the video bus
    /// track's too): the color picker; OK sets the track's color.
    fn track_color_dialog(&mut self, ctx: &egui::Context) {
        let Some((track, which, picker)) = &mut self.track_color else {
            return;
        };
        let (track, which) = (*track, *which);
        match picker.show(ctx) {
            crate::color_picker::Outcome::Open => {}
            crate::color_picker::Outcome::Cancelled => self.track_color = None,
            crate::color_picker::Outcome::Chosen(rgba) => {
                self.track_color = None;
                self.modify(|p| {
                    let slot = match track {
                        Some(id) => match p.tracks.iter_mut().find(|t| t.id == id) {
                            Some(t) => &mut t.fade_colors,
                            None => return,
                        },
                        None => &mut p.video_bus.fade_colors,
                    };
                    let mut colors = slot.unwrap_or(bv_timeline::scene::DEFAULT_FADE_COLORS);
                    colors[which] = rgba;
                    *slot = Some(colors);
                });
            }
        }
    }

    /// Plug-ins that crashed (`bv_ofx::take_crashes`: their binaries are
    /// switched off until BananaV restarts) are told in the status bar and
    /// written in the plug-ins' log.
    fn follow_plugin_crashes(&mut self) {
        let mut told: Vec<std::path::PathBuf> = Vec::new();
        for crash in bv_ofx::take_crashes() {
            crate::logs::log(
                crate::logs::LogKind::Plugins,
                &[format!(
                    "{} crashed ({}, exception {:#010x} at {}) and was switched off",
                    crash.binary.display(),
                    crash.doing,
                    crash.code,
                    crash.at
                )],
            );
            if told.contains(&crash.binary) {
                continue;
            }
            let name = crash.binary.file_stem().map_or_else(
                || crash.binary.display().to_string(),
                |s| s.to_string_lossy().into_owned(),
            );
            self.report_error(trf!(
                "The plug-in {} stopped working and was turned off until BananaV restarts.",
                name
            ));
            told.push(crash.binary);
        }
    }

    /// While another program is active, BananaV lets go of its audio output
    /// and of its media files, so others can use and change them (Options ›
    /// Preferences › General: "Close audio and MIDI ports…", "Close media
    /// files when not the active application"). Back, it reads again the
    /// files changed meanwhile.
    fn follow_activation(&mut self) {
        let active = bv_platform::app_is_active().unwrap_or(true);
        let prefs = &self.settings.prefs;
        let (audio, media) = (prefs.release_audio_inactive, prefs.release_media_inactive);
        self.engine.hold_audio(active || !audio);
        self.sync_midi(active);
        if active == self.was_active {
            return;
        }
        self.was_active = active;
        self.follow_changed_files(active);
        if active {
            self.engine.follow_default_audio();
            if let Some(before) = self.released_media.take()
                && before != media_stamps(self.project())
            {
                self.engine.refresh_thumbnails();
            }
        } else if media && !self.engine.is_playing() {
            self.engine.release_media();
            self.released_media = Some(media_stamps(self.project()));
        }
    }

    /// What BananaV does once it starts (not in tests): opens the file it
    /// was given (a project, or media placed at the cursor), or else the
    /// project open when it was last closed (Options › Preferences ›
    /// General: "Automatically open last project on startup"); then checks
    /// that it opens its project files.
    pub fn startup(&mut self, args: impl Iterator<Item = std::ffi::OsString>) {
        self.restore_session();
        // A file, or an OpenTimelineIO folder bundle (`.otiod`).
        let timeline = |p: &PathBuf| {
            crate::media_files::kind_of(p) == Some(crate::media_files::FileKind::Timeline)
        };
        let given = args
            .map(PathBuf::from)
            .find(|p| p.is_file() || (p.is_dir() && timeline(p)));
        match given {
            Some(path) => self.open_file(path),
            None => {
                if self.settings.prefs.open_last_project
                    && let Some(path) = self.settings.last_session.clone()
                    && path.is_file()
                {
                    self.open_path(path);
                }
            }
        }
        self.offer_recovery();
        self.check_associations();
        // OFX plug-ins: the cache's at once, new ones in the background.
        self.ofx.scan(&self.settings.ofx_folders);
        self.open_from_env();
    }

    /// The main window as BananaV was left — its windows' layout, the
    /// toolbar and the status bar —, as VEGAS opens; kept from now on as it
    /// changes. Only the program itself (`startup`) does this: the tests'
    /// apps start from the default layout and never write the settings.
    fn restore_session(&mut self) {
        if let Some(s) = self.settings.session.clone() {
            if let Ok(dock) = crate::layouts::dock_from_value(&s.dock) {
                self.dock = Some(dock);
                self.timeline_top = s.timeline_top;
                self.layout_slot = s
                    .layout_slot
                    .filter(|&n| self.settings.layouts.get(n).is_some_and(Option::is_some));
                // Preferences › Display's place for the timeline wins.
                let bottom = self.settings.prefs.timeline_at_bottom;
                if self.timeline_top == bottom {
                    self.place_timeline(bottom);
                }
            }
            self.show_toolbar = !s.hide_toolbar;
            self.show_status_bar = !s.hide_status_bar;
            self.timeline.set_switches(s.timeline);
        }
        // The windows `BANANAV_OPEN` opens (tools/capture_windows.ps1) are
        // not the user's.
        if std::env::var_os("BANANAV_OPEN").is_none() {
            self.session_kept = Some(self.session_now().unwrap_or_default());
        }
    }

    /// The main window as it is now, to open with again: its layout as the
    /// user arranged it (the docking areas F11 hid back, a maximized window
    /// at its size again) and its bars.
    fn session_now(&self) -> Option<crate::settings::Session> {
        let mut dock = match &self.docks_hidden {
            Some(hidden) => hidden.full().clone(),
            None => self.dock.clone()?,
        };
        if let Some(m) = &self.dock_maximized {
            dock::restore(&mut dock, m);
        }
        Some(crate::settings::Session {
            dock: serde_json::to_value(&dock).ok()?,
            timeline_top: self.timeline_top,
            layout_slot: self.layout_slot,
            hide_toolbar: !self.show_toolbar,
            hide_status_bar: !self.show_status_bar,
            timeline: self.timeline.switches(),
        })
    }

    /// Writes the session to the settings once it changed — looked at once
    /// a second, never while a button is down (a splitter being dragged) —
    /// so that BananaV opens as it was even after it was stopped without
    /// closing.
    fn keep_session(&mut self, ctx: &egui::Context) {
        if self.session_kept.is_none()
            || self.session_checked.elapsed() < Duration::from_secs(1)
            || ctx.input(|i| i.pointer.any_down())
        {
            return;
        }
        self.session_checked = Instant::now();
        let Some(now) = self.session_now() else {
            return;
        };
        if self.session_kept.as_ref() != Some(&now) {
            let _ = self.settings.save_session(now.clone());
            self.session_kept = Some(now);
        }
    }

    /// Development hook: `BANANAV_OPEN="Options.Preferences;File.Properties"`
    /// runs those commands at startup, once the project given opened — a
    /// Global one by its id, else another context's (opens dialogs to check
    /// them as OS windows; "Edit.Select.All;Tools.Video.VideoEventFX;
    /// Transport.Play" plays with an event's FX window open, to measure).
    fn open_from_env(&mut self) {
        let Ok(ids) = std::env::var("BANANAV_OPEN") else {
            return;
        };
        for id in ids.split(';').map(str::trim) {
            let commands = crate::keymap::registry();
            let action = commands
                .iter()
                .find(|c| c.context == crate::keymap::Context::Global && c.id == id)
                .or_else(|| commands.iter().find(|c| c.id == id))
                .map(|c| c.action);
            match action {
                Some(crate::keymap::Action::Cmd(c)) => c.run(self),
                Some(crate::keymap::Action::Extra(e)) => self.run_extra(e),
                _ => {}
            }
        }
    }

    /// Switches the interface language and remembers it.
    pub(crate) fn set_language(&mut self, lang: crate::i18n::Language) {
        crate::i18n::set_language(lang);
        self.settings.language = lang;
    }

    pub(crate) fn project(&self) -> &Project {
        self.history.current()
    }

    pub(crate) fn notify(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), Instant::now(), false));
    }

    pub(crate) fn report_error(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), Instant::now(), true));
    }

    /// A file that could not be saved; when Windows' Controlled folder
    /// access refused it, says so and how to allow BananaV.
    pub(crate) fn report_save_error(&mut self, path: &std::path::Path, e: &std::io::Error) {
        let dir = path.parent().unwrap_or(std::path::Path::new("."));
        if bv_platform::blocked_by_folder_protection(dir, e) {
            let problem = crate::render::folder::FolderProblem::new(dir, e);
            self.report_error(trf!("Could not save: {}", problem.message()));
        } else {
            self.report_error(trf!("Could not save: {}", e));
        }
    }

    /// Aplica uma edição registrando no histórico; erros viram mensagem.
    pub(crate) fn edit<R, E: std::fmt::Display>(
        &mut self,
        f: impl FnOnce(&mut Project) -> Result<R, E>,
    ) -> Option<R> {
        match self.history.apply(f) {
            Ok(r) => Some(r),
            Err(e) => {
                self.report_error(e.to_string());
                None
            }
        }
    }

    /// Edição que não falha.
    pub(crate) fn modify<R>(&mut self, f: impl FnOnce(&mut Project) -> R) -> R {
        match self
            .history
            .apply(|p| Ok::<_, std::convert::Infallible>(f(p)))
        {
            Ok(r) => r,
            Err(e) => match e {},
        }
    }

    /// A change that makes no undo: undo and redo keep it (see
    /// `History::apply_everywhere`).
    pub(crate) fn modify_untracked(&mut self, f: impl Fn(&mut Project)) {
        self.history.apply_everywhere(f);
    }

    /// An effect's parameters set: one undo while they are dragged, or none
    /// (Options › Preferences › General: "Create undos for FX parameter
    /// changes" off). `effects` is a chain whose effects changed only
    /// their parameters; `set` puts such a chain where it belongs in a
    /// project.
    pub(crate) fn set_fx_params(
        &mut self,
        key: egui::Id,
        effects: Vec<bv_timeline::Effect>,
        chain: impl Fn(&Project) -> Option<Vec<bv_timeline::Effect>>,
        set: impl Fn(&mut Project, Vec<bv_timeline::Effect>),
    ) {
        if self.settings.prefs.fx_undo {
            self.modify_merged(key, |p| set(p, effects));
            return;
        }
        self.modify_untracked(|p| {
            // In the states where the chain has the same effects.
            if let Some(mut old) = chain(p)
                && old.len() == effects.len()
                && old.iter().zip(&effects).all(|(a, b)| a.kind == b.kind)
            {
                for (a, b) in old.iter_mut().zip(&effects) {
                    a.params.clone_from(&b.params);
                    // A plug-in's values, when it is the same plug-in.
                    if a.plugin.as_ref().map(|p| &p.id) == b.plugin.as_ref().map(|p| &p.id) {
                        a.plugin.clone_from(&b.plugin);
                    }
                }
                set(p, old);
            }
        });
    }

    /// Edição contínua (controle sendo arrastado): todas as mudanças com a
    /// mesma chave viram uma única entrada de desfazer.
    pub(crate) fn modify_merged<R>(
        &mut self,
        key: egui::Id,
        f: impl FnOnce(&mut Project) -> R,
    ) -> R {
        self.history.apply_merged(key.value(), f)
    }

    /// Runs `f`, whose edits (imports, events placed) undo as one step.
    pub(crate) fn as_one_undo<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let mark = self.history.undo_mark();
        let r = f(self);
        self.history.join_since(mark);
        r
    }

    pub(crate) fn is_dirty(&self) -> bool {
        *self.project() != self.saved
    }

    pub(crate) fn frame_rate(&self) -> bv_core::FrameRate {
        self.project().settings.frame_rate
    }

    // ---- transporte ------------------------------------------------------

    pub(crate) fn is_playing(&self) -> bool {
        self.engine.is_playing() || self.held_playback
    }

    /// The playback needle grabbed while playing: playback waits (the
    /// needle follows the mouse and the preview shows its frames) until it
    /// is let go.
    pub(crate) fn hold_playback(&mut self) {
        if self.held_playback || !self.engine.is_playing() {
            return;
        }
        if let Some(pos) = self.engine.stop() {
            self.playhead = Some(pos);
            self.held_playback = true;
        }
    }

    /// Moves the held playback needle.
    pub(crate) fn move_playhead(&mut self, t: Tick) {
        if self.held_playback {
            self.playhead = Some(t.max(Tick::ZERO));
        }
    }

    /// Lets the playback needle go: playback goes on from there, like
    /// VEGAS's.
    pub(crate) fn release_playback(&mut self) {
        if std::mem::take(&mut self.held_playback) {
            let from = self.playhead.unwrap_or(self.cursor);
            self.start_playback(from);
        }
    }

    /// Space: Play/Pause, or Play/Stop when Options › Preferences › General
    /// says so ("Make spacebar and F12 Play/Stop instead of Play/Pause").
    pub(crate) fn space_bar(&mut self) {
        // Shuttling backwards (J): Space stops it.
        if self.scrub_rate != 0.0 {
            self.scrub_rate = 0.0;
            return;
        }
        if self.settings.prefs.space_play_stop {
            self.toggle_play();
        } else {
            self.toggle_pause();
        }
    }

    /// The timeline's playback: looping in the loop region when Loop
    /// Playback is on, else on past the project's end until stopped (as
    /// VEGAS's).
    pub(crate) fn play_options(&self) -> PlayOptions {
        PlayOptions {
            looping: if self.loop_enabled {
                self.project().loop_region
            } else {
                None
            },
            rate: self.play_rate,
            past_end: true,
        }
    }

    /// Enter: the other of Space's two, as in VEGAS — Play/Stop (back where
    /// playback started) while Space plays and pauses, Play/Pause while
    /// Space plays and stops (Options › Preferences › General).
    pub(crate) fn enter_key(&mut self) {
        if self.settings.prefs.space_play_stop {
            self.toggle_pause();
        } else {
            self.toggle_play();
        }
    }

    /// Space: plays from the edit cursor; when playing, stops. The edit
    /// cursor stays where it is (where playback started, or where it was
    /// clicked during playback), like in VEGAS.
    pub(crate) fn toggle_play(&mut self) {
        if self.is_playing() || self.engine.is_recording() {
            self.stop();
        } else {
            self.start_playback(self.cursor);
        }
    }

    /// Enter: plays; when playing, pauses and brings the edit cursor to the
    /// playback position.
    pub(crate) fn toggle_pause(&mut self) {
        if self.engine.is_recording() {
            self.stop();
            return;
        }
        if self.is_playing() {
            let held = std::mem::take(&mut self.held_playback).then_some(self.playhead);
            if let Some(pos) = self.engine.stop().or(held.flatten()) {
                self.cursor = self.frame_rate().snap_floor(pos);
            }
            self.playhead = None;
        } else {
            self.start_playback(self.cursor);
        }
    }

    /// Shift+Space: plays from the start (or the loop region) without moving
    /// the edit cursor.
    pub(crate) fn play_from_start(&mut self) {
        let start = if self.loop_enabled {
            self.project().loop_region.map_or(Tick::ZERO, |r| r.start)
        } else {
            Tick::ZERO
        };
        self.engine.stop();
        self.start_playback(start);
    }

    pub(crate) fn start_playback(&mut self, from: Tick) {
        let end = self.project().duration();
        if end == Tick::ZERO {
            self.notify(tr("The timeline is empty."));
            return;
        }
        let from = if from >= end && !self.loop_enabled {
            Tick::ZERO
        } else {
            from
        };
        self.engine.set_project(self.history.current());
        self.playhead = Some(from);
        self.engine.play(from, self.play_options());
        if let AudioStatus::Unavailable(why) = self.engine.audio_status() {
            let why = why.clone();
            self.notify(trf!("No audio ({}); using the system clock.", why));
        }
    }

    pub(crate) fn stop(&mut self) {
        if self.engine.is_recording() {
            self.finish_recording();
        }
        if self.engine.position().is_some() {
            self.engine.stop();
        }
        self.held_playback = false;
        self.playhead = None;
        self.scrub_rate = 0.0;
    }

    /// Reinicia o playback com as opções atuais (loop, taxa).
    pub(crate) fn restart_if_playing(&mut self) {
        if self.is_playing()
            && let Some(pos) = self.engine.position()
        {
            self.engine.play(pos, self.play_options());
        }
    }

    /// Moves the edit cursor. During playback the playback position keeps
    /// going (the "second needle" of VEGAS): click, then split with S.
    pub(crate) fn seek(&mut self, t: Tick) {
        self.cursor = t.max(Tick::ZERO);
    }

    /// Moves the edit cursor and, during playback, makes playback continue
    /// from there (ruler clicks, Go To, transport buttons).
    pub(crate) fn seek_playback(&mut self, t: Tick) {
        let t = t.max(Tick::ZERO);
        self.cursor = t;
        if self.is_playing() {
            self.engine.play(t, self.play_options());
            self.playhead = Some(t);
        }
    }

    /// Time shown by the preview and the time displays: the playback
    /// position while playing, otherwise the edit cursor.
    pub(crate) fn display_time(&self) -> Tick {
        self.playhead.unwrap_or(self.cursor)
    }

    /// Where markers and selection ends go: the playback position while
    /// playing (so they can be set on the fly), otherwise the edit cursor.
    pub(crate) fn mark_time(&self) -> Tick {
        let rate = self.frame_rate();
        self.playhead.map_or(self.cursor, |t| rate.snap_floor(t))
    }

    /// Move o cursor (para testes e automação).
    pub fn seek_seconds(&mut self, s: f64) {
        self.seek(Tick::from_seconds_f64(s));
    }

    /// The edit cursor's time, in seconds (for screen tests).
    pub fn cursor_seconds(&self) -> f64 {
        self.cursor.as_seconds_f64()
    }

    pub(crate) fn step_frames(&mut self, n: i64) {
        let rate = self.frame_rate();
        let frame = rate.frame_at(self.cursor) + n;
        self.seek(rate.frame_start(frame.max(0)));
    }

    pub(crate) fn go_end(&mut self) {
        let end = self.project().duration();
        self.seek_playback(end);
    }

    // ---- edição ----------------------------------------------------------

    pub(crate) fn selected(&self) -> Vec<ClipId> {
        self.selection.iter().copied().collect()
    }

    /// O clipe "principal" da seleção (o primeiro de vídeo, se houver).
    pub(crate) fn primary_clip(&self) -> Option<ClipId> {
        let p = self.project();
        self.selection
            .iter()
            .copied()
            .find(|&id| p.clip_track(id).is_some_and(|t| t.kind == TrackKind::Video))
            .or_else(|| self.selection.iter().next().copied())
    }

    /// S: divide os clipes selecionados no cursor (ou todos, sem seleção).
    pub(crate) fn split_at_cursor(&mut self) {
        let at = self.cursor;
        let selected = self.selected();
        let created = self.edit(|p| -> Result<Vec<ClipId>, bv_timeline::EditError> {
            if selected.is_empty() {
                return Ok(p.split_all_at(at));
            }
            let mut out = Vec::new();
            for id in selected {
                if let Some(c) = p.clip(id)
                    && at > c.start
                    && at < c.end()
                {
                    out.extend(p.split(id, at)?);
                }
            }
            Ok(out)
        });
        if created.is_some_and(|c| c.is_empty()) {
            self.notify(tr("No clip under the cursor to split."));
        }
    }

    pub(crate) fn delete_selection(&mut self, ripple: bool) {
        if self.selection.is_empty() {
            return;
        }
        let ids = self.selected();
        let scope = self.ripple_scope;
        // Options › Preferences › Editing: "Cut, copy, and delete grouped
        // events".
        let grouped = self.settings.prefs.group_edits;
        if ripple {
            self.edit(|p| p.delete_events_rippled(&ids, scope, grouped));
        } else {
            self.edit(|p| p.delete_events(&ids, false, grouped));
        }
        self.selection.clear();
    }

    pub(crate) fn copy(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        let grouped = self.settings.prefs.group_edits;
        let data = self.project().copy_events(&self.selected(), grouped);
        self.notify(trn!(
            data.len(),
            "{} clip copied.",
            "{} clips copied.",
            data.len()
        ));
        self.clipboard = Some(data);
    }

    pub(crate) fn cut(&mut self) {
        self.copy();
        self.delete_selection(self.auto_ripple);
    }

    pub(crate) fn paste(&mut self) {
        let Some(data) = self.clipboard.clone() else {
            return;
        };
        let at = self.cursor;
        let mark = self.project().id_counter();
        let ids = self.modify(|p| p.paste_clips(&data, at));
        self.selection = ids.into_iter().collect();
        self.ask_generated_copies(mark);
    }

    /// Apara o início (`start`) ou o fim dos clipes selecionados até o cursor.
    pub(crate) fn trim_to_cursor(&mut self, start: bool) {
        let at = self.cursor;
        let ids = self.selected();
        self.edit(|p| -> Result<(), bv_timeline::EditError> {
            for id in ids {
                let Some(c) = p.clip(id) else { continue };
                if at > c.start && at < c.end() {
                    if start {
                        p.trim_start(id, at)?;
                    } else {
                        p.trim_end(id, at)?;
                    }
                }
            }
            Ok(())
        });
    }

    /// Mantém só o trecho dos clipes selecionados dentro da região de loop.
    pub(crate) fn trim_to_loop(&mut self) {
        let Some(r) = self.project().loop_region else {
            self.notify(tr(
                "Create a loop region by dragging on the bar above the ruler.",
            ));
            return;
        };
        let ids = self.selected();
        self.edit(|p| -> Result<(), bv_timeline::EditError> {
            for id in ids {
                let Some(c) = p.clip(id) else { continue };
                if c.end() > r.start && c.start < r.end() {
                    if c.start < r.start {
                        p.trim_start(id, r.start)?;
                    }
                    if p.clip(id).is_some_and(|c| c.end() > r.end()) {
                        p.trim_end(id, r.end())?;
                    }
                }
            }
            Ok(())
        });
    }

    pub(crate) fn toggle_lock(&mut self) {
        let ids = self.selected();
        let p = self.project();
        let all_locked = ids.iter().all(|&id| p.clip(id).is_some_and(|c| c.locked));
        self.modify(|p| {
            for id in ids {
                if let Some(c) = p.clip_mut(id) {
                    c.locked = !all_locked;
                }
            }
        });
    }

    pub(crate) fn group_selection(&mut self) {
        let ids = self.selected();
        if ids.len() > 1 {
            self.modify(|p| p.group_clips(&ids));
        }
    }

    pub(crate) fn ungroup_selection(&mut self) {
        let ids = self.selected();
        self.modify(|p| p.ungroup_clips(&ids));
    }

    pub(crate) fn insert_marker(&mut self) {
        let at = self.mark_time();
        let id = self.modify(|p| p.add_marker(at));
        self.name_new_marker(id);
    }

    pub(crate) fn insert_region(&mut self) {
        match self.project().loop_region {
            Some(r) if r.duration > Tick::ZERO => {
                let id = self.modify(|p| p.add_region(r));
                self.name_new_marker(id);
            }
            _ => self.notify(tr(
                "Make a time selection (loop region) to turn into a region.",
            )),
        }
    }

    /// A marker or region placed while stopped: its name is asked for
    /// right away (Options › Preferences › General: "Automatically name
    /// regions and markers if not playing").
    fn name_new_marker(&mut self, id: MarkerId) {
        if !self.settings.prefs.name_markers || self.is_playing() {
            return;
        }
        let p = self.project();
        let name = p
            .markers
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.name.clone())
            .or_else(|| {
                p.regions
                    .iter()
                    .find(|r| r.id == id)
                    .map(|r| r.name.clone())
            })
            .unwrap_or_default();
        self.dialogs.rename = Some((id, name));
    }

    pub(crate) fn remove_marker(&mut self, id: MarkerId) {
        self.modify(|p| p.remove_marker(id));
    }

    pub(crate) fn set_loop_region(&mut self, r: Option<TimeRange>) {
        self.modify(|p| p.loop_region = r);
        self.restart_if_playing();
    }

    /// Selects the event and the members of its group (with `add`, as
    /// Ctrl+click: toggles them). The event counts as picked, its group
    /// mates as selected along with it.
    pub(crate) fn select_clip(&mut self, id: ClipId, add: bool) {
        let mut picked = if add { self.picked() } else { BTreeSet::new() };
        if !add {
            self.selection.clear();
        }
        let members = self.project().group_members(id);
        let already = add && self.selection.contains(&id);
        for m in members {
            if already {
                self.selection.remove(&m);
                picked.remove(&m);
            } else {
                self.selection.insert(m);
            }
        }
        if !already {
            picked.insert(id);
        }
        if !add || picked.len() == 1 {
            self.first_pick = (!already).then_some(id);
        } else if already && self.first_pick == Some(id) {
            self.first_pick = None;
        }
        self.set_picked(picked);
    }

    /// The events picked in the selection (all of it when it was changed
    /// in another way).
    pub(crate) fn picked(&self) -> BTreeSet<ClipId> {
        if self.picked.1 == self.selection {
            self.picked.0.clone()
        } else {
            self.selection.clone()
        }
    }

    /// Notes which events of the selection were picked.
    pub(crate) fn set_picked(&mut self, picked: BTreeSet<ClipId>) {
        self.picked = (picked, self.selection.clone());
    }

    /// Whether a selected event is so only for being in the group of a
    /// picked one (VEGAS outlines it in blue, the picked in yellow).
    pub(crate) fn selected_by_group(&self, id: ClipId) -> bool {
        let (picked, of) = &self.picked;
        *of == self.selection && self.selection.contains(&id) && !picked.contains(&id)
    }

    pub(crate) fn select_all(&mut self) {
        self.selection = self.project().clips().map(|(_, c)| c.id).collect();
        self.set_picked(self.selection.clone());
    }

    pub(crate) fn undo(&mut self) {
        if self.history.undo() {
            self.prune_selection();
        }
    }

    pub(crate) fn redo(&mut self) {
        if self.history.redo() {
            self.prune_selection();
        }
    }

    pub(crate) fn prune_selection(&mut self) {
        let p = self.history.current();
        self.selection.retain(|id| p.clip(*id).is_some());
    }

    /// Coloca a mídia na timeline. `track` é a trilha onde ela foi solta (ou
    /// `None` para usar as primeiras trilhas). O vídeo vai para uma trilha de
    /// vídeo e o áudio para uma de áudio; trilhas que faltarem são criadas.
    pub(crate) fn place_media(&mut self, media: MediaId, at: Tick, track: Option<TrackId>) {
        self.place_media_range(media, at, None, track);
    }

    /// Como [`Self::place_media`], mas com um trecho da mídia (Unidade de corte).
    pub(crate) fn place_media_range(
        &mut self,
        media: MediaId,
        at: Tick,
        range: Option<TimeRange>,
        track: Option<TrackId>,
    ) {
        self.place_media_streams(media, at, range, track, true, true);
    }

    /// Like `place_media_range`, taking only the video and/or the audio of
    /// the media.
    pub(crate) fn place_media_streams(
        &mut self,
        media: MediaId,
        at: Tick,
        range: Option<TimeRange>,
        track: Option<TrackId>,
        video: bool,
        audio: bool,
    ) {
        // A subclip: its parent's part, reversed when it says so (one undo).
        if let Some(sub) = self.project().subclip(media).cloned() {
            let part = match range {
                Some(r) => TimeRange::new(
                    sub.range.start + r.start,
                    r.duration
                        .min(sub.range.duration - r.start.min(sub.range.duration)),
                ),
                None => sub.range,
            };
            let edits = self.history.edits();
            self.place_media_streams(sub.parent, at, Some(part), track, video, audio);
            if sub.reverse && self.history.edits() != edits {
                let ids: Vec<ClipId> = self.selection.iter().copied().collect();
                self.history.amend(|p| {
                    for id in &ids {
                        let _ = p.reverse_event(*id);
                    }
                });
            }
            return;
        }
        let Some(info) = self.project().media(media).cloned() else {
            return;
        };
        let first = self.project().clips().next().is_none();
        // The first video of an empty timeline: the project may take its
        // frame size and rate (Options › Preferences › General: "Prompt to
        // adjust project settings to match first media added to timeline").
        let matched = (first
            && self.settings.prefs.match_first_media
            && matches!(info.source, bv_timeline::MediaSource::File { .. })
            && !info.still)
            .then_some(info.video)
            .flatten()
            .filter(|v| {
                let s = self.project().settings;
                (v.width & !1, v.height & !1, v.frame_rate, v.pixel_aspect)
                    != (s.width, s.height, s.frame_rate, s.pixel_aspect)
            });
        // New events as Options › Preferences › Editing says: their Loop
        // switch and pitch, the length of stills; media of another shape
        // than the frame may fill it ("Automatically crop video files /
        // still images added to timeline").
        let new_events = self.settings.prefs.new_events();
        let crop = if info.still {
            self.settings.prefs.crop_stills
        } else {
            self.settings.prefs.crop_video
                && matches!(info.source, bv_timeline::MediaSource::File { .. })
        };
        let ids = self.edit(|p| {
            let dropped_kind = track.and_then(|t| p.track(t)).map(|t| t.kind);
            // On the track it was given (an adjustment track's too, as
            // VEGAS's), else on the first of its kind that is not one.
            let pick = |p: &mut Project, kind: TrackKind| -> TrackId {
                if dropped_kind == Some(kind)
                    && let Some(t) = track
                {
                    return t;
                }
                p.tracks
                    .iter()
                    .find(|t| t.kind == kind && !t.adjustment)
                    .map(|t| t.id)
                    .unwrap_or_else(|| p.add_track(kind))
            };
            let v = (video && info.video.is_some()).then(|| pick(p, TrackKind::Video));
            let a = (audio && info.audio.is_some()).then(|| pick(p, TrackKind::Audio));
            let ids = match range {
                Some(r) => {
                    p.insert_media_range_as(media, at, r.start, r.duration, (v, a), &new_events)
                }
                None => p.insert_media_as(media, at, v, a, &new_events),
            }?;
            if crop {
                actions::fill_frame(p, &ids);
            }
            Ok::<_, bv_timeline::EditError>(ids)
        });
        // A stereo file as two mono events, panned hard left and right
        // (Options › Preferences › General: "Import stereo as dual mono").
        let ids = ids.map(|ids| {
            let stereo = |stream: u16| info.audio_stream(stream).is_some_and(|a| a.channels == 2);
            if !self.settings.prefs.stereo_as_dual_mono {
                return ids;
            }
            let mut all = ids.clone();
            for id in ids {
                let audio = self
                    .project()
                    .clip_track(id)
                    .is_some_and(|t| t.kind == TrackKind::Audio);
                let stream = self.project().clip(id).map_or(0, |c| c.audio_stream);
                if !audio || !stereo(stream) {
                    continue;
                }
                let split = self.modify(|p| {
                    let (copy, left, right) = p.split_dual_mono(id).ok()?;
                    // Tracks holding only the pair are panned to its sides.
                    for (track, pan) in [(left, -1.0), (right, 1.0)] {
                        if let Some(t) = p.tracks.iter_mut().find(|t| t.id == track)
                            && t.clips.iter().all(|c| c.id == id || c.id == copy)
                        {
                            t.pan = pan;
                        }
                    }
                    Some(copy)
                });
                all.extend(split);
            }
            all
        });
        if let Some(ids) = ids {
            self.selection = ids.into_iter().collect();
            if first {
                self.fit_timeline = true;
            }
            if let Some(v) = matched {
                let (width, height) = ((v.width & !1).max(16), (v.height & !1).max(16));
                self.ask_user(crate::ask::Ask {
                    title: tr("Project Video Settings").into(),
                    text: trf!(
                        "Make the project's video settings match this media: {}×{}, {}?",
                        width,
                        height,
                        crate::fmt::fps(v.frame_rate)
                    ),
                    what: crate::ask::Asked::MatchMedia {
                        width,
                        height,
                        rate: v.frame_rate,
                        pixel_aspect: v.pixel_aspect,
                    },
                    check: None,
                });
            }
        }
    }

    /// Transport › Cursor Preview (keypad 0 in VEGAS): plays the part of
    /// the timeline around the edit cursor, as long as Options ›
    /// Preferences › Editing says ("Cursor preview duration"), the cursor
    /// in its middle; with Loop Playback, over and over.
    pub(crate) fn preview_cursor(&mut self) {
        if self.project().duration() == Tick::ZERO {
            self.notify(tr("The timeline is empty."));
            return;
        }
        let r = preview_range(self.cursor, self.settings.prefs.cursor_preview);
        self.engine.stop();
        self.engine.set_project(self.history.current());
        let options = bv_engine::PlayOptions {
            looping: self.loop_enabled.then_some(r),
            rate: self.play_rate,
            past_end: false,
        };
        self.playhead = Some(r.start);
        self.engine.play(r.start, options);
        self.cursor_preview = Some(r);
    }

    /// Options › Preferences › Display › "Display timeline at bottom of
    /// main window": the timeline's window below the docking area, or
    /// above it (it is put back there with the default room).
    pub(crate) fn place_timeline(&mut self, bottom: bool) {
        self.timeline_top = !bottom;
        if let Some(dock) = &mut self.dock
            && let Some(at) = dock.find_tab(&Tab::Timeline)
        {
            dock.remove_tab(at);
        }
    }

    /// The demo's video track with Expand Track Layers on (screen tests).
    pub fn track_layers_for_review(&mut self) {
        if let Some(t) = self
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == bv_timeline::TrackKind::Video)
        {
            let id = t.id;
            self.timeline.toggle_layers(id);
        }
    }

    /// Display's "Reverse Event Background color" on and the timeline above
    /// the docking area (screen tests; the settings in memory).
    pub fn display_prefs_for_review(&mut self) {
        self.settings = crate::settings::UiSettings::in_memory();
        self.settings.prefs.reverse_event_colors = true;
        self.settings.prefs.timeline_at_bottom = false;
        self.place_timeline(false);
    }

    pub(crate) fn add_generator(&mut self, g: Generator, at: Tick, track: Option<TrackId>) {
        let info = MediaInfo::generated(g, &self.project().settings);
        self.as_one_undo(|app| {
            let id = app.modify(|p| p.add_media(info));
            app.place_media(id, at, track);
        });
    }

    /// Importa arquivos; devolve os ids das mídias importadas.
    pub(crate) fn import_paths(&mut self, paths: &[PathBuf]) -> Vec<MediaId> {
        let mut errors = Vec::new();
        let mut ids = Vec::new();
        let mut made_360 = false;
        for path in paths {
            let s = path.to_string_lossy().to_string();
            // A mesma mídia não entra duas vezes.
            if let Some(existing) = self.project().media.iter().find(
                |m| matches!(&m.source, bv_timeline::MediaSource::File { path } if *path == s),
            ) {
                ids.push(existing.id);
                continue;
            }
            // A project is never nested in itself (VEGAS's "Nesting
            // projects").
            if bv_engine::is_project(&s) && self.nests_open_project(path) {
                errors.push(format!(
                    "{}: {}",
                    path.file_name()
                        .map_or(s.clone(), |n| n.to_string_lossy().to_string()),
                    crate::media_files::error_text(&bv_engine::MediaError::NestedInItself)
                ));
                continue;
            }
            let read = Engine::probe_file(&s);
            // Options › Preferences › File I/O › Log Reads.
            if crate::logs::wants(crate::logs::LogKind::Reads) {
                let lines = match &read {
                    Ok(info) => std::iter::once(format!("Read {s}"))
                        .chain(crate::logs::media_lines(info))
                        .collect(),
                    Err(e) => vec![format!("Could not read {s}: {e}")],
                };
                crate::logs::log(crate::logs::LogKind::Reads, &lines);
            }
            match read {
                Ok(info) => {
                    // Options › Preferences › File I/O's "Enable
                    // auto-detection of 360° media": a 360° media turns the
                    // project's 360 Output on (one undo with the import).
                    let to_360 = self.settings.prefs.detect_360
                        && !self.project().settings.output_360
                        && info.video.is_some_and(|v| v.spherical);
                    // A video profile of its kind (Media Properties').
                    let mut info = info;
                    if let Some(p) =
                        crate::media_properties::profile_for(&self.settings.video_profiles, &info)
                    {
                        p.apply(&mut info);
                    }
                    let id = self.modify(|p| {
                        if to_360 {
                            p.settings.output_360 = true;
                        }
                        p.add_media(info)
                    });
                    made_360 |= to_360;
                    ids.push(id);
                    self.copy_if_removable(path);
                    self.proxy_for_new_media(id);
                }
                Err(e) => errors.push(format!(
                    "{}: {}",
                    path.file_name()
                        .map_or(s.clone(), |n| n.to_string_lossy().to_string()),
                    crate::media_files::error_text(&e)
                )),
            }
        }
        if !errors.is_empty() {
            self.report_error(errors.join(" · "));
        } else if made_360 {
            self.notify(tr("360° media imported: the project's 360 Output is on."));
        } else if !ids.is_empty() {
            self.notify(trn!(
                ids.len(),
                "{} media file imported.",
                "{} media files imported.",
                ids.len()
            ));
        }
        ids
    }

    /// A file on a removable drive is copied to the temporary files folder
    /// and read from there, so that the project goes on working without the
    /// drive (Options › Preferences › General: "Temporary files folder";
    /// the copies go when BananaV closes).
    fn copy_if_removable(&mut self, path: &std::path::Path) {
        if !bv_platform::is_removable(path) {
            return;
        }
        let dir = removable_folder(&self.settings.prefs);
        let n = self.removable_copies.len();
        let name = path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_default();
        let from = path.to_path_buf();
        let job = std::thread::spawn(move || {
            std::fs::create_dir_all(&dir)?;
            let mut to_name = std::ffi::OsString::from(format!("{n}_"));
            to_name.push(&name);
            let to = dir.join(to_name);
            std::fs::copy(&from, &to)?;
            Ok(to)
        });
        self.removable_copies
            .push((path.to_string_lossy().to_string(), job));
    }

    /// Copies from removable drives that are done are read from now on.
    fn finish_removable_copies(&mut self) {
        let (done, going): (Vec<_>, Vec<_>) = std::mem::take(&mut self.removable_copies)
            .into_iter()
            .partition(|(_, job)| job.is_finished());
        self.removable_copies = going;
        for (original, job) in done {
            let name = std::path::Path::new(&original)
                .file_name()
                .map_or(original.clone(), |n| n.to_string_lossy().to_string());
            match job.join() {
                Ok(Ok(copy)) => {
                    bv_media_copy(&original, &copy);
                    self.notify(trf!(
                        "{} was copied from the removable drive: the project keeps working without it.",
                        name
                    ));
                }
                Ok(Err(e)) => self.report_error(trf!(
                    "Could not copy {} from the removable drive: {}",
                    name,
                    e
                )),
                Err(_) => {}
            }
        }
    }

    pub(crate) fn import_dialog(&mut self) {
        let dialog = rfd::FileDialog::new().set_title(tr("Import Media"));
        if let Some(paths) = crate::media_files::media_filters(dialog).pick_files() {
            self.import_paths(&paths);
        }
    }

    pub(crate) fn remove_media(&mut self, id: MediaId) {
        self.edit(|p| p.remove_media(id));
        self.prune_selection();
        if self.selected_media == Some(id) {
            self.selected_media = None;
        }
    }

    // ---- arquivo -----------------------------------------------------------

    pub(crate) fn new_project(&mut self) {
        self.stop();
        self.closing_project();
        let p = self.blank_project();
        self.history.reset(p.clone());
        self.saved = p;
        self.project_path = None;
        self.selection.clear();
        self.cursor = Tick::ZERO;
    }

    /// A new project with this frame size and rate (the welcome screen's).
    pub(crate) fn new_project_sized(&mut self, width: u32, height: u32, rate: bv_core::FrameRate) {
        self.new_project();
        let mut p = self.project().clone();
        p.settings.width = width;
        p.settings.height = height;
        p.settings.frame_rate = rate;
        self.history.reset(p.clone());
        self.saved = p;
    }

    /// File › Open: a project replaces the current one; a media file goes
    /// into the project as an event at the cursor (as in VEGAS).
    pub(crate) fn open_dialog(&mut self) {
        let dialog = rfd::FileDialog::new().set_title(tr("Open"));
        let Some(path) = crate::media_files::open_filters(dialog).pick_file() else {
            return;
        };
        self.open_file(path);
    }

    /// Opens a project or a timeline (OpenTimelineIO's), or brings a media
    /// file in as an event at the cursor.
    pub(crate) fn open_file(&mut self, path: PathBuf) {
        use crate::media_files::{FileKind, kind_of};
        match kind_of(&path) {
            Some(FileKind::Project | FileKind::Timeline) => self.open_path(path),
            _ => {
                if let Some(&id) = self.import_paths(std::slice::from_ref(&path)).first() {
                    self.place_media(id, self.cursor, None);
                }
            }
        }
    }

    /// Opens a project file, or an OpenTimelineIO timeline as a new one.
    pub(crate) fn open_path(&mut self, path: PathBuf) {
        if crate::media_files::kind_of(&path) == Some(crate::media_files::FileKind::Timeline) {
            crate::otio_ui::open(self, path);
            return;
        }
        let loaded = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| Project::from_json(&s).map_err(|e| e.to_string()))
            .and_then(|p| p.check_invariants().map(|_| p));
        match loaded {
            Ok(p) => {
                self.stop();
                self.closing_project();
                self.history.reset(p.clone());
                self.saved = p;
                self.settings.touch_recent(&path);
                self.project_path = Some(path);
                self.selection.clear();
                self.cursor = Tick::ZERO;
                self.fit_timeline = true;
                self.find_proxies();
            }
            Err(e) => self.report_error(trf!("Could not open the project: {}", e)),
        }
    }

    /// A project made from another program's file (File › Import ›
    /// OpenTimelineIO): open as a new one, not saved yet.
    pub(crate) fn open_imported(&mut self, p: Project) {
        self.stop();
        self.closing_project();
        self.history.reset(p.clone());
        self.saved = self.blank_project();
        self.project_path = None;
        self.selection.clear();
        self.cursor = Tick::ZERO;
        self.fit_timeline = true;
        self.find_proxies();
    }

    pub(crate) fn save(&mut self, save_as: bool) -> bool {
        // The file asked for (Save As, or a project never saved): with File
        // › Copy Media with Project on, its media are copied next to it.
        let asked = save_as || self.project_path.is_none();
        let path = match (&self.project_path, save_as) {
            (Some(p), false) => p.clone(),
            _ => {
                let Some(p) = rfd::FileDialog::new()
                    .set_title(tr("Save Project"))
                    .add_filter(tr("BananaV Project"), &["bvproj"])
                    .set_file_name(tr("Untitled.bvproj"))
                    .save_file()
                else {
                    return false;
                };
                p
            }
        };
        match std::fs::write(&path, self.project().to_json()) {
            Ok(()) => {
                self.saved = self.project().clone();
                self.notify(trf!("Project saved to {}", path.display()));
                self.settings.touch_recent(&path);
                self.project_path = Some(path);
                if asked && self.settings.save_as_copy_media {
                    self.copy_media_dialog = Some(crate::archive::CopyMediaDialog {
                        trimmed: self.settings.copy_media_trimmed,
                        extra: self.settings.copy_media_extra,
                    });
                }
                true
            }
            Err(e) => {
                self.report_save_error(&path, &e);
                false
            }
        }
    }

    /// Edit Source Project: the project media file `path` was rendered
    /// from (its path reference) opens in another BananaV window.
    pub(crate) fn edit_source_project(&mut self, path: &std::path::Path) {
        let Some(project) = bv_engine::Engine::project_reference(path) else {
            self.notify(tr(
                "This file keeps no path to the project it was rendered from.",
            ));
            return;
        };
        if self.project_path.as_deref() == Some(project.as_path()) {
            self.notify(tr(
                "The project this file was rendered from is the one open.",
            ));
            return;
        }
        if !project.is_file() {
            self.report_error(trf!(
                "The project this file was rendered from is not there: {}",
                project.display()
            ));
            return;
        }
        let opened = std::env::current_exe()
            .and_then(|exe| std::process::Command::new(exe).arg(&project).spawn());
        match opened {
            Ok(_) => self.notify(trf!("Opening {} in another window.", project.display())),
            Err(e) => self.report_error(trf!("Could not open the project: {}", e)),
        }
    }

    /// The file of the media of the event the selection is about, when it
    /// keeps a path to its project (Edit Source Project's).
    pub(crate) fn selected_source_project(&self) -> Option<std::path::PathBuf> {
        let clip = self.primary_clip()?;
        let p = self.project();
        let m = p.clip(clip)?.media;
        let info = p.media(m).cloned().or_else(|| p.subclip_info(m))?;
        let bv_timeline::MediaSource::File { path } = &info.source else {
            return None;
        };
        let path = std::path::PathBuf::from(path);
        crate::archive::keeps_reference(&path).then_some(path)
    }

    pub(crate) fn open_render_dialog(&mut self) {
        let d = crate::render::RenderDialog::new(self);
        self.dialogs.render = Some(d);
    }

    /// Tools › Render to New Track.
    pub(crate) fn open_render_to_new_track(&mut self) {
        let d = crate::render::RenderDialog::to_new_track(self);
        self.dialogs.render = Some(d);
    }

    /// Set Project Tempo…: File › Properties on its Ruler tab, where the
    /// tempo is.
    pub(crate) fn open_project_tempo(&mut self) {
        self.dialogs.project_properties =
            Some(crate::project_properties::ProjectPropertiesDialog::new(self).on_ruler());
    }

    pub(crate) fn open_project_settings(&mut self) {
        self.dialogs.project_properties = Some(
            crate::project_properties::ProjectPropertiesDialog::new(self),
        );
    }

    /// Project Properties on its Audio tab (the Master Bus's and the Mixing
    /// Console's Audio Properties).
    pub(crate) fn open_project_audio_settings(&mut self) {
        self.dialogs.project_properties =
            Some(crate::project_properties::ProjectPropertiesDialog::new(self).on_audio_tab());
    }

    // ---- layout ------------------------------------------------------------

    pub(crate) fn is_tab_open(&self, tab: Tab) -> bool {
        self.dock
            .as_ref()
            .is_some_and(|d| d.find_tab(&tab).is_some())
    }

    /// Mostra (ou foca) uma aba; com `toggle`, fecha se já estiver aberta.
    pub(crate) fn show_tab(&mut self, tab: Tab, toggle: bool) {
        let Some(dock) = &mut self.dock else { return };
        match dock.find_tab(&tab) {
            Some(path) if toggle => {
                dock.remove_tab(path);
            }
            Some(path) => {
                let _ = dock.set_active_tab(path);
            }
            None => dock::open_tab(dock, tab),
        }
    }

    /// Mostra a aba com esse título (usado nos testes de tela).
    pub fn show_tab_named(&mut self, title: &str) {
        if let Some(tab) = Tab::ALL.into_iter().find(|t| t.title() == title) {
            self.show_tab(tab, false);
        }
    }

    // ---- interface ---------------------------------------------------------

    fn shortcuts(&mut self, ctx: &egui::Context) {
        // The Project Media thumbnail under the pointer last frame.
        let hovered = self.media_window.hovered.take();
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        // I and O over a thumbnail set its part's ends.
        self.media_part_keys(ctx, hovered);
        // Project Media's Quick Tags: Ctrl and a number while it has the
        // keys.
        self.media_tag_keys(ctx);
        // Its Delete: the picked media out of the project.
        self.media_delete_key(ctx);
        // In multicamera editing the number keys choose takes.
        self.multicam_keys(ctx);
        // The Marker Tool takes the arrows and Delete while it is on.
        self.marker_tool_keys(ctx);
        crate::keymap::dispatch(self, ctx);
        // Keys that are not menu items (transport, markers, snapping…).
        let cmd = Modifiers::COMMAND;
        let pressed = |m: Modifiers, k: Key| {
            ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)))
        };
        if pressed(cmd, Key::I) {
            self.import_dialog();
        }
        let none = Modifiers::NONE;
        let shift = Modifiers::SHIFT;
        if pressed(shift, Key::Space) {
            self.play_from_start();
        }
        if pressed(none, Key::Space) {
            self.space_bar();
        }
        if pressed(none, Key::Enter) {
            self.enter_key();
        }
        if pressed(none, Key::Escape) {
            if self.engine.is_recording() {
                self.stop();
            } else if self.timeline.edge_trim.is_some() {
                // Out of edge-trimming or Expanded Edit Mode.
                self.timeline.edge_trim = None;
            } else {
                self.selection.clear();
                self.tool = Tool::Edit;
            }
        }
        if pressed(none, Key::ArrowUp) {
            self.timeline.zoom_around(0.8, self.display_time());
        }
        if pressed(none, Key::ArrowDown) {
            self.timeline.zoom_around(1.25, self.display_time());
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // A render: Cancel, its progress and its file, like VEGAS's.
            if let Some(p) = self.engine.render_progress()
                && !p.finished
            {
                if ui.button(tr("Cancel")).clicked() {
                    match &mut self.dialogs.rendering {
                        Some(w) => w.ask_cancel(),
                        None => self.engine.cancel_render(),
                    }
                }
                ui.add(
                    egui::ProgressBar::new(p.fraction())
                        .desired_width(300.0)
                        .text(format!("{} %", (p.fraction() * 100.0).floor())),
                );
                let file = self
                    .rendering_file()
                    .and_then(|f| f.file_name().map(|n| n.to_string_lossy().to_string()))
                    .unwrap_or_default();
                ui.label(trf!("Rendering {}", file));
                ui.separator();
            }
            crate::otio_ui::status(self, ui);
            crate::archive::status(self, ui);
            crate::multicam::status(self, ui);
            crate::stabilize_ui::status(self, ui);
            crate::mask_tracking::status(self, ui);
            if let Some((msg, at, error)) = &self.status {
                if at.elapsed() < Duration::from_secs(8) {
                    let color = if *error {
                        theme::error()
                    } else {
                        theme::text()
                    };
                    ui.colored_label(color, msg);
                } else {
                    self.status = None;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // VEGAS's: its end 27 pixels from the window's.
                ui.add_space(19.0);
                if let Some(text) = self.record_time_text() {
                    ui.label(egui::RichText::new(text).size(12.0).color(theme::text()));
                }
                // The audio device only when it fails, why on hover.
                if let AudioStatus::Unavailable(why) = self.engine.audio_status() {
                    ui.separator();
                    ui.label(egui::RichText::new(tr("Audio: unavailable")).color(theme::error()))
                        .on_hover_text(why);
                }
                if let Some(s) = &self.stats {
                    ui.separator();
                    ui.label(egui::RichText::new(&s.text).color(theme::text_dim()));
                }
            });
        });
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let name = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or(tr("Untitled").to_string(), |s| {
                s.to_string_lossy().to_string()
            });
        let mut title = format!("{name}{} — BananaV", if self.is_dirty() { "*" } else { "" });
        // While rendering, its progress first ("3% - Untitled — BananaV").
        if let Some(p) = self.engine.render_progress()
            && !p.finished
        {
            title = format!("{}% - {title}", (p.fraction() * 100.0).floor());
        }
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            crate::floating::set_main_title(&title);
            self.title = title;
        }
    }

    /// Media info of a file that is being dragged (probed once and cached).
    pub(crate) fn probe_cached(&mut self, path: &std::path::Path) -> Option<MediaInfo> {
        if let Some(info) = self.drag_probe.get(path) {
            return info.clone();
        }
        let info = Engine::probe_file(&path.to_string_lossy()).ok();
        self.drag_probe.insert(path.to_path_buf(), info.clone());
        info
    }

    /// Reads files hovering over / dropped on the window from the system.
    fn update_os_drag(&mut self, ctx: &egui::Context) {
        let (hovered, dropped): (Vec<PathBuf>, Vec<PathBuf>) = ctx.input(|i| {
            (
                i.raw
                    .hovered_files
                    .iter()
                    .filter_map(|f| f.path.clone())
                    .collect(),
                i.raw
                    .dropped_files
                    .iter()
                    .map(|f| f.path().to_path_buf())
                    .collect(),
            )
        });
        // The OS cursor is in physical pixels on the screen; the window's
        // inner rect is in points on the screen.
        let pointer_now = || {
            os_cursor().and_then(|(x, y)| {
                let ppp = ctx.pixels_per_point();
                let inner = ctx.input(|i| i.viewport().inner_rect)?;
                Some(egui::pos2(x as f32 / ppp, y as f32 / ppp) - inner.min.to_vec2())
            })
        };
        // A project or a timeline dropped opens, as with File › Open (after
        // the question about unsaved changes) — but a project dropped on the
        // timeline is nested in this one (VEGAS's "Nesting projects"); media
        // go to the project.
        let on_timeline = !dropped.is_empty()
            && pointer_now()
                .zip(self.dock.as_ref().and_then(dock::timeline_rect))
                .is_some_and(|(p, r)| r.contains(p));
        let opens = |p: &&PathBuf| {
            use crate::media_files::{FileKind, kind_of};
            match kind_of(p) {
                Some(FileKind::Timeline) => true,
                Some(FileKind::Project) => !on_timeline,
                _ => false,
            }
        };
        if let Some(path) = dropped.iter().find(opens) {
            if matches!(self.media_drag, Some(MediaDrag::Files(_))) {
                self.media_drag = None;
            }
            self.os_drag = None;
            self.request(Pending::OpenPath(path.clone()));
            return;
        }
        if hovered.is_empty() && dropped.is_empty() {
            if self.os_drag.take().is_some() && matches!(self.media_drag, Some(MediaDrag::Files(_)))
            {
                self.media_drag = None;
            }
            return;
        }
        let was_dropped = !dropped.is_empty();
        let paths = if was_dropped { dropped } else { hovered };
        let pointer = pointer_now();
        for p in &paths {
            self.probe_cached(p);
        }
        self.media_drag = Some(MediaDrag::Files(paths.clone()));
        self.os_drag = Some(OsDrag {
            paths,
            pointer,
            dropped: was_dropped,
        });
        // No events arrive while the drag hovers: keep the ghost moving.
        ctx.request_repaint_after(Duration::from_millis(16));
    }

    /// Files dropped outside the timeline go to the project media.
    fn finish_os_drop(&mut self) {
        if let Some(d) = self.os_drag.take_if(|d| d.dropped) {
            if matches!(self.media_drag, Some(MediaDrag::Files(_))) {
                self.media_drag = None;
                let n = self.import_paths(&d.paths).len();
                if n > 0 {
                    self.show_tab(Tab::ProjectMedia, false);
                }
            }
            self.drag_probe.clear();
        }
    }

    pub(crate) fn cursor_timecode(&self) -> String {
        self.ruler_text(self.cursor)
    }

    /// The project's ruler format (File › Properties › Ruler, Options ›
    /// Ruler Format).
    pub(crate) fn effective_ruler_format(&self) -> crate::timefmt::RulerFormat {
        self.project().settings.effective_ruler_format()
    }

    /// Timeline time `t` in the ruler format.
    pub(crate) fn ruler_text(&self, t: Tick) -> String {
        use crate::timefmt::RulerFormatExt;
        self.effective_ruler_format().format(t, self.project())
    }

    /// Distance between grid lines (Options › Grid Spacing) at the zoom of
    /// `view` (the timeline being drawn: `self.timeline` is a stand-in
    /// then).
    pub(crate) fn grid_ticks(&self, view: &crate::timeline::TimelineView) -> Tick {
        use crate::timefmt::GridSpacing;
        let rate = self.frame_rate();
        // Musical spacings count the project's tempo (File › Properties ›
        // Ruler).
        if let Some(t) = self.grid.musical_length(&self.project().settings.tempo) {
            return t;
        }
        match self.grid {
            // Beats or measures on a Measures & Beats ruler.
            GridSpacing::RulerMarks
                if self.effective_ruler_format() == crate::timefmt::RulerFormat::MeasuresBeats =>
            {
                let tempo = self.project().settings.tempo;
                let (_, minor) = crate::timeline::beat_steps(view, &tempo);
                Tick((tempo.beat().0 as f64 * minor as f64).round() as i64)
            }
            // An audio CD's frames on an Audio CD Time ruler.
            GridSpacing::RulerMarks
                if self.effective_ruler_format() == crate::timefmt::RulerFormat::AudioCd =>
            {
                let cd = crate::timefmt::CD_FRAMES;
                Tick(cd.frame_duration().0 * view.grid_step(cd))
            }
            GridSpacing::RulerMarks => Tick(rate.frame_duration().0 * view.grid_step(rate)),
            GridSpacing::Minutes => Tick::from_seconds(60),
            GridSpacing::Seconds => Tick::from_seconds(1),
            GridSpacing::Milliseconds => Tick::from_seconds_f64(0.001),
            GridSpacing::Frames => rate.frame_duration(),
            _ => rate.frame_duration(),
        }
    }

    /// Where the grid's lines start: the ruler's zero (measure 1.1) for the
    /// spacings that count the tempo, the timeline's start otherwise.
    pub(crate) fn grid_origin(&self) -> Tick {
        use crate::timefmt::{GridSpacing, RulerFormat};
        let beats = self.grid == GridSpacing::RulerMarks
            && self.effective_ruler_format() == RulerFormat::MeasuresBeats;
        if self.grid.is_musical() || beats {
            Tick::ZERO - self.project().ruler_offset
        } else {
            Tick::ZERO
        }
    }

    /// The grid line nearest `t` (`step` from [`Self::grid_ticks`]).
    pub(crate) fn grid_line_near(&self, t: Tick, step: i64) -> Tick {
        let origin = self.grid_origin();
        let k = ((t - origin).0 as f64 / step.max(1) as f64).round() as i64;
        origin + Tick(k * step.max(1))
    }

    fn close_dialog(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.confirm.clone() else {
            return;
        };
        let mut choice = None;
        egui::Modal::new(egui::Id::new("fechar")).show(ctx, |ui| {
            ui.heading(tr("Save changes?"));
            ui.label(tr("The project has unsaved changes."));
            ui.horizontal(|ui| {
                if ui.button(tr("Save")).clicked() {
                    choice = Some(0);
                }
                if ui.button(tr("Discard")).clicked() {
                    choice = Some(1);
                }
                if ui.button(tr("Cancel")).clicked() {
                    choice = Some(2);
                }
            });
        });
        match choice {
            Some(0) => {
                self.confirm = None;
                if self.save(false) {
                    self.perform(pending);
                }
            }
            Some(1) => {
                self.confirm = None;
                self.perform(pending);
            }
            Some(_) => self.confirm = None,
            None => {}
        }
    }

    /// Cria um projeto de demonstração com mídia gerada, transições,
    /// efeitos, marcadores e uma região de loop.
    pub fn load_demo(&mut self) {
        use bv_timeline::{Effect, EffectKind, Transition};
        let s = Tick::from_seconds;
        let mut p = Project::default();
        let v1 = p.tracks[0].id;
        let a1 = p.tracks[1].id;
        let make = |p: &mut Project, g: Generator| {
            let info = MediaInfo::generated(g, &p.settings);
            p.add_media(info)
        };
        let bars = make(&mut p, Generator::ColorBars);
        let sunset = make(
            &mut p,
            Generator::Gradient {
                from: [0.98, 0.6, 0.2, 1.0],
                to: [0.35, 0.1, 0.45, 1.0],
                angle_deg: 90.0,
            },
        );
        let checker = make(&mut p, Generator::Checkerboard { size: 60 });
        let tone = make(
            &mut p,
            Generator::TestTone {
                frequency_hz: 1000.0,
                level_db: -18.0,
            },
        );
        let la = make(
            &mut p,
            Generator::TestTone {
                frequency_hz: 440.0,
                level_db: -12.0,
            },
        );
        let insert = |p: &mut Project, m, at, v, a| p.insert_media(m, at, v, a).expect("demo");
        insert(&mut p, bars, Tick::ZERO, Some(v1), None);
        let g = insert(&mut p, sunset, s(4), Some(v1), None)[0];
        let _ = p.trim_end(g, s(10));
        let c = insert(&mut p, checker, s(10), Some(v1), None)[0];
        insert(&mut p, tone, Tick::ZERO, None, Some(a1));
        let t2 = insert(&mut p, la, s(4), None, Some(a1))[0];
        let _ = p.trim_end(t2, s(10));
        if let Some(clip) = p.clip_mut(g) {
            clip.effects.push(Effect::new(EffectKind::Vignette));
        }
        if let Some(clip) = p.clip_mut(c) {
            clip.transition = Transition::Iris;
            clip.effects.push(Effect::new(EffectKind::ColorGrade));
        }
        let _ = p.set_fades(c, Some(Tick::ZERO), Some(s(1)));
        let m = p.add_marker(s(2));
        p.rename_marker(m, tr("Opening").into());
        let r = p.add_region(bv_core::TimeRange::new(s(4), s(6)));
        p.rename_marker(r, tr("Middle").into());
        p.loop_region = Some(bv_core::TimeRange::new(Tick::ZERO, s(5)));
        self.stop();
        self.closing_project();
        self.history.reset(p.clone());
        self.saved = p;
        self.project_path = None;
        self.selection.clear();
        self.select_clip(g, false);
        self.cursor = Tick::from_seconds_f64(4.5);
        self.timeline.fit(s(15));
    }

    /// Importa um arquivo e o coloca no cursor (atalho para testes e
    /// automação).
    pub fn import_and_place(&mut self, path: &std::path::Path) -> bool {
        match self.import_paths(&[path.to_path_buf()]).first() {
            Some(&id) => {
                let at = self.cursor;
                self.place_media(id, at, None);
                self.selected_media = Some(id);
                self.trimmer.open(id);
                true
            }
            None => false,
        }
    }

    /// Stretches the selected events (and their groups) to end at `seconds`
    /// and fits the timeline (for screen tests and automation).
    pub fn stretch_selection_to(&mut self, seconds: f64) {
        let ids = self.selected();
        let end = Tick::from_seconds_f64(seconds);
        self.modify(|p| {
            for id in ids {
                let _ = p.trim_end(id, end);
            }
        });
        self.fit_timeline = true;
    }

    /// The file being rendered (the progress window's).
    pub(crate) fn rendering_file(&self) -> Option<std::path::PathBuf> {
        self.dialogs
            .rendering
            .as_ref()
            .map(|w| w.file().to_path_buf())
    }

    /// Renders the project's first `seconds` to an H.264 MP4 of `size` at
    /// `path` and shows the progress window, without touching the user's
    /// settings (for screen tests).
    pub fn render_for_review(&mut self, path: std::path::PathBuf, seconds: f64, size: (u32, u32)) {
        let s = bv_engine::RenderSettings {
            project_file: None,
            format: bv_engine::FileFormat::AvcMp4,
            path: path.clone(),
            range: bv_core::TimeRange::new(Tick::ZERO, Tick::from_seconds_f64(seconds)),
            video: Some(bv_engine::VideoParams {
                width: size.0,
                height: size.1,
                frame_rate: self.project().settings.frame_rate,
                bitrate: 8_000_000,
                ..bv_engine::VideoParams::default()
            }),
            audio: Some(bv_engine::AudioParams::default()),
            options: bv_engine::RenderOptions::default(),
        };
        self.engine.set_project(self.history.current());
        self.engine.start_render(s);
        self.dialogs.rendering = Some(crate::render::progress::ProgressWindow::new(path));
    }

    /// Video events' thumbnails as Options › Preferences › Video can show
    /// them (`Thumbnails::ALL[thumbnails]` with `FrameNumbers::ALL[numbers]`,
    /// the lists' order), on settings that are never saved (for screen
    /// tests).
    pub fn thumbnails_for_review(&mut self, thumbnails: usize, numbers: usize) {
        self.settings = crate::settings::UiSettings::in_memory();
        self.settings.prefs.thumbnails = crate::settings::Thumbnails::ALL[thumbnails];
        self.settings.prefs.frame_numbers = crate::settings::FrameNumbers::ALL[numbers];
    }

    /// The demo's audio track with its automation controls on (in Touch,
    /// with a volume envelope as if recorded), the Device Explorer open and
    /// the time display listening for MIDI timecode, on settings that are
    /// never saved (for screen tests).
    /// The demo's video track moved with Track Motion (a smaller box,
    /// turned, with a 2D shadow and glow) and its window open on the
    /// shadow's row (for screen tests).
    /// For the screen captures: the sound file `path` in the project and on
    /// a new audio track, and Beat Detection (with `tempo`, Tempo
    /// Detection) run on it, its sound read.
    pub fn beats_for_review(&mut self, path: &str, tempo: bool) {
        let Ok(info) = bv_engine::Engine::probe_file(path) else {
            return;
        };
        let media = self.modify(|p| {
            let m = p.add_media(info);
            let t = p.add_track(TrackKind::Audio);
            let _ = p.insert_media(m, Tick::ZERO, None, Some(t));
            m
        });
        if tempo {
            self.tempo_detection(media);
        } else {
            self.beat_detection(media);
        }
        for _ in 0..2000 {
            if self.engine.beat_progress().is_some_and(|p| p.finished) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// For the screen captures: the demo's first audio event with a Gain
    /// and a Reverb (bypassed), selected, and Tools › Audio › Apply
    /// Non-Real-Time Event FX's dialog open for it.
    pub fn nonrealtime_for_review(&mut self) {
        let p = self.project();
        let Some(clip) = p
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .flat_map(|t| t.clips.iter())
            .map(|c| c.id)
            .next()
        else {
            return;
        };
        self.modify(|p| {
            if let Some(c) = p.clip_mut(clip) {
                let mut gain = bv_timeline::Effect::new(bv_timeline::EffectKind::Gain);
                gain.set_param(0, -3.0);
                let mut reverb = bv_timeline::Effect::new(bv_timeline::EffectKind::Reverb);
                reverb.enabled = false;
                c.effects.push(gain);
                c.effects.push(reverb);
            }
        });
        self.select_clip(clip, false);
        self.open_nonrealtime_fx();
    }

    /// For the screen captures: VST 3 plug-ins searched for in `bundle`'s
    /// folder alone, BananaV's generic editor (no window of the plug-in's
    /// own), on settings never saved; the demo's first audio event picked
    /// and its FX window asked for (its chain is empty: the Plug-In
    /// Chooser opens). Whether there was an audio event.
    pub fn vst_chooser_for_review(&mut self, bundle: &std::path::Path) -> bool {
        self.settings = self.settings.unsaved();
        let prefs = &mut self.settings.prefs;
        prefs.vst_generic_editor = true;
        prefs.vst_folders = bundle
            .parent()
            .map(|f| vec![f.to_path_buf()])
            .unwrap_or_default();
        prefs.vst_left_out.clear();
        let p = self.project();
        let Some(clip) = p
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .flat_map(|t| t.clips.iter())
            .map(|c| c.id)
            .next()
        else {
            return false;
        };
        self.select_clip(clip, false);
        self.open_clip_window("fx-audio");
        true
    }

    /// For the screen captures: as [`BananaApp::vst_chooser_for_review`],
    /// with the first effect of the VST 3 module at `bundle` already on the
    /// event (its gain at a quarter: BananaV's test plug-ins), its page in
    /// the FX window. Whether it could.
    pub fn vst_effect_for_review(&mut self, bundle: &std::path::Path) -> bool {
        let Some(class) = bv_vst3::load(bundle)
            .ok()
            .and_then(|m| m.effect("").cloned())
        else {
            return false;
        };
        let fx = bv_timeline::VstFx {
            path: bundle.to_path_buf(),
            class_id: class.id(),
            name: class.name.clone(),
            vendor: class.vendor.clone(),
            params: [(0, 0.25)].into_iter().collect(),
            ..Default::default()
        };
        if !self.vst_chooser_for_review(bundle) {
            return false;
        }
        let Some(clip) = self.primary_clip() else {
            return false;
        };
        self.modify(|p| {
            if let Some(c) = p.clip_mut(clip) {
                c.effects.push(bv_timeline::Effect::vst(fx));
            }
        });
        true
    }

    /// For the screen captures: the known OFX plug-in `id` at its defaults
    /// on the demo's event at the cursor, its FX window open on it. Whether
    /// it could.
    pub fn plugin_effect_for_review(&mut self, id: &str) -> bool {
        let Some(info) = bv_ofx::registry().info(id) else {
            return false;
        };
        let Some(clip) = self.primary_clip() else {
            return false;
        };
        self.modify(|p| {
            if let Some(c) = p.clip_mut(clip) {
                c.effects.push(crate::ofx::new_effect(&info));
            }
        });
        self.clip_windows.clip_fx_last = Some(clip);
        true
    }

    /// Whether a VST 3 plug-in's own window is open (for screen tests).
    pub fn vst_window_open_for_review(&self) -> bool {
        self.vst.window_open()
    }

    /// For the screen captures: the demo's first audio event with a Modern
    /// Busses and assignable effects for review: two busses (B named
    /// "Drums"), a chain with a Reverb, the first audio track into bus A
    /// with sends to B (before its volume) and to the chain, bus A's
    /// Compressor after its volume and its send to the chain; the Mixing
    /// Console shown. Whether there was an audio track.
    pub fn busses_for_review(&mut self) -> bool {
        use bv_timeline::{Effect, EffectKind, Output, Send, SendTo};
        let Some(audio) = self
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
        else {
            return false;
        };
        let made = self.modify(|p| {
            let a = p.add_bus()?;
            let b = p.add_bus()?;
            p.bus_mut(b)?.channel.name = "Drums".into();
            let fx = p.add_fx_chain(vec![Effect::new(EffectKind::Reverb)])?;
            p.fx_chain_mut(fx)?.input_db = -2.0;
            p.set_track_output(audio, Output::Bus(a));
            let send = |to, gain_db| Send {
                gain_db,
                ..Send::new(to)
            };
            p.set_send(audio, send(SendTo::Bus(b), -6.0));
            p.set_send(audio, send(SendTo::Fx(fx), -12.0));
            p.set_bus_send(a, send(SendTo::Fx(fx), -3.0));
            let ch = &mut p.bus_mut(a)?.channel;
            ch.gain_db = -4.5;
            ch.fx_post_fader = true;
            ch.effects.push(Effect::new(EffectKind::Compressor));
            Some(())
        });
        self.show_tab(crate::dock::Tab::MixingConsole, false);
        made.is_some()
    }

    /// The bus tracks for review (View › Audio Bus Tracks and Video Bus
    /// Track): the busses and chain of [`Self::busses_for_review`], bus A's
    /// volume and pan envelopes, the chain's volume, the audio track's send
    /// envelope to Drums; the video bus's fade to black and motion blur.
    /// Whether they were made.
    pub fn bus_tracks_for_review(&mut self) -> bool {
        use bv_timeline::{EnvelopeKind, EnvelopeOwner, KeyCurve, SendTo, Strip};
        if !self.busses_for_review() {
            return false;
        }
        let s = Tick::from_seconds;
        let made = self.modify(|p| {
            let (a, b) = (p.busses.first()?.id, p.busses.get(1)?.id);
            let fx = p.fx_chains.first()?.id;
            let audio = p.tracks.iter().find(|t| t.kind == TrackKind::Audio)?.id;
            let bus = Strip::Bus(a);
            p.toggle_strip_envelope(bus, EnvelopeKind::Volume, KeyCurve::Linear)
                .ok()?;
            let owner = EnvelopeOwner::Strip(bus);
            for (at, v) in [(s(2), 0.5), (s(4), 0.9), (s(6), 0.3)] {
                p.set_envelope_point(owner, EnvelopeKind::Volume, at, v)
                    .ok()?;
            }
            p.toggle_strip_envelope(bus, EnvelopeKind::Pan, KeyCurve::Linear)
                .ok()?;
            p.set_envelope_point(owner, EnvelopeKind::Pan, s(3), -0.6)
                .ok()?;
            let chain = Strip::Chain(fx);
            p.toggle_strip_envelope(chain, EnvelopeKind::Volume, KeyCurve::Smooth)
                .ok()?;
            p.set_envelope_point(EnvelopeOwner::Strip(chain), EnvelopeKind::Volume, s(5), 0.4)
                .ok()?;
            let send = EnvelopeKind::Send(SendTo::Bus(b));
            p.toggle_envelope(audio, send).ok()?;
            p.set_envelope_point(audio, send, s(3), 0.3).ok()?;
            // The video bus: a fade to black at the end, a motion blur.
            let video = EnvelopeOwner::VideoBus;
            p.toggle_video_bus_envelope(EnvelopeKind::FadeToColor, KeyCurve::Linear)
                .ok()?;
            p.set_envelope_point(video, EnvelopeKind::FadeToColor, s(4), 0.0)
                .ok()?;
            p.set_envelope_point(video, EnvelopeKind::FadeToColor, s(6), -1.0)
                .ok()?;
            p.toggle_video_bus_envelope(EnvelopeKind::MotionBlur, KeyCurve::Linear)
                .ok()?;
            p.set_envelope_point(video, EnvelopeKind::MotionBlur, Tick::ZERO, 0.1)
                .ok()?;
            // An animated Video Output FX: its keyframes under the video
            // bus track.
            let key = |at: Tick, v: f64| {
                bv_timeline::PluginKey::new(
                    at,
                    bv_timeline::PluginValue::Num(vec![v]),
                    KeyCurve::Linear,
                )
            };
            let mut fx = bv_timeline::PluginFx {
                id: "org.bananav.Pixelate".into(),
                version: [1, 0],
                name: "Pixelate".into(),
                params: Default::default(),
            };
            fx.params.insert(
                "horizontal".into(),
                bv_timeline::PluginParam {
                    value: bv_timeline::PluginValue::Num(vec![0.0]),
                    keys: vec![key(s(1), 0.0), key(s(3), 0.2), key(s(5), 0.0)],
                },
            );
            p.video_output_fx.push(bv_timeline::Effect::plugin(fx));
            Some(())
        });
        self.timeline.show_bus_tracks = true;
        self.timeline.show_video_bus = true;
        self.show_tab(crate::dock::Tab::Timeline, false);
        made.is_some()
    }

    /// A Color Grading picker waiting with a color under the pointer (0
    /// Lift's black point, 1 Gain's white point, 2 Select Neutral Tone, 3
    /// the HSL Curves' Color Selector) and Show Limits on Scopes on
    /// (screen tests).
    pub fn color_picker_for_review(&mut self, which: usize) {
        crate::tabs::color::picker_for_review(self, which);
        self.color_panel.show_limits = true;
    }

    /// The Color Grading panel for review in the Color Correction layout:
    /// the demo's first video event graded a little (Gain, a warmer Lift,
    /// an RL Shadows color, contrast, a red curve, a Hue vs Saturation
    /// curve) and group `section` shown (0 LUTs … 4 Curves; 5 the HSL
    /// Curves alone). Whether there was a video event.
    pub fn color_grading_for_review(&mut self, section: usize) -> bool {
        use crate::tabs::color::Section;
        let Some(clip) = self
            .project()
            .clips()
            .find(|(t, _)| t.kind == TrackKind::Video)
            .map(|(_, c)| c.id)
        else {
            return false;
        };
        self.apply_layout(crate::app::actions::LayoutPreset::ColorCorrection);
        self.selection = [clip].into_iter().collect();
        (self.color_panel.section, self.color_panel.item) = [
            (Section::Luts, None),
            (Section::Wheels, None),
            (Section::RlWheels, None),
            (Section::Utilities, None),
            (Section::Curves, None),
            (Section::Curves, Some(1)),
        ][section.min(5)];
        self.color_panel.pinned = [false; 2];
        crate::tabs::color::grade_for_review(self);
        self.show_tab(crate::dock::Tab::ColorGrading, false);
        true
    }

    /// The Color Grading panel for review with item `item` of group
    /// `section` alone (as [`BananaApp::color_grading_for_review`]), its
    /// group open in the navigation.
    pub fn color_grading_item_for_review(&mut self, section: usize, item: usize) -> bool {
        if !self.color_grading_for_review(section.min(4)) {
            return false;
        }
        self.color_panel.item = Some(item);
        self.color_panel.folded[section.min(4)] = false;
        true
    }

    /// The Color Curves for review, pinned beside the Color Wheels:
    /// Brightness, Red and Blue on the shared graph zoomed in on the
    /// shadows, a point of Red picked with its tangents (`wide`: the
    /// pinned curves dragged wide, side by side).
    pub fn grading_curves_for_review(&mut self, wide: bool) -> bool {
        if !self.color_grading_for_review(1) {
            return false;
        }
        let v = &mut self.color_panel;
        v.pinned = [true, false];
        v.pinned_width = if wide { 1000.0 } else { 470.0 };
        v.curves.active = [true, true, false, true];
        v.curves.views[0] = v.curves.views[0].zoomed(2.0, [0.25, 0.25]);
        v.curves.picked = Some((1, 1));
        true
    }

    /// The Mixing Console for review in the Audio Mixing layout:
    /// [`BananaApp::busses_for_review`]'s busses and chain, the control
    /// regions labelled (the VU meters shown, held still; the peak meters
    /// beside the faders, fed levels). Whether there was an audio track.
    /// An input bus in the Mixing Console: "Guitar", on the default input's
    /// left channel, through a Gain, its output off (screen tests: nothing
    /// opens the computer's audio devices).
    pub fn input_bus_for_review(&mut self) {
        let input = bv_timeline::RecordInput {
            first: 0,
            channels: 1,
            ..bv_timeline::RecordInput::default()
        };
        if let Some(id) = self.modify(|p| p.add_input_bus(input)) {
            self.modify(|p| {
                if let Some(b) = p.input_bus_mut(id) {
                    b.name = "Guitar".into();
                    b.to_master = false;
                    b.effects
                        .push(bv_timeline::Effect::new(bv_timeline::EffectKind::Gain));
                }
            });
        }
    }

    pub fn mixing_console_for_review(&mut self) -> bool {
        use bv_engine::meters::MeterOf;
        self.apply_layout(crate::app::actions::LayoutPreset::AudioMixing);
        if !self.busses_for_review() {
            return false;
        }
        // Every region but the peak meters apart (the faders show them).
        self.console.regions = [true, true, true, true, false, true];
        self.console.labels = true;
        let p = self.project();
        let tracks: Vec<TrackId> = p
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
            .collect();
        let strips = p.strips();
        let mut readings = Vec::new();
        for (i, id) in tracks.iter().enumerate() {
            let a = 0.09 * (i + 2) as f32;
            readings.push((MeterOf::Track(*id), vec![a, a * 0.8]));
            self.track_meters
                .entry(*id)
                .or_default()
                .update(Some(vec![a * 1.4, a * 1.1]));
        }
        for s in strips {
            readings.push((MeterOf::Strip(s), vec![0.16, 0.13]));
            self.strip_meters
                .entry(s)
                .or_default()
                .update(Some(vec![0.25, 0.2]));
        }
        readings.push((MeterOf::Master, vec![0.3, 0.27]));
        self.vu.update(readings, std::time::Instant::now());
        self.vu.hold();
        true
    }

    /// The Mixing Console in the Audio Mixing layout as a project shows it
    /// at first (VEGAS 23's regions), with eight audio tracks, the third
    /// one picked, their meters lit (for screen tests).
    pub fn mixing_console_default_for_review(&mut self) {
        self.apply_layout(crate::app::actions::LayoutPreset::AudioMixing);
        while self
            .project()
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .count()
            < 8
        {
            crate::commands::Cmd::InsertAudioTrack.run(self);
        }
        let tracks: Vec<TrackId> = self
            .project()
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
            .collect();
        for (i, id) in tracks.iter().enumerate() {
            let a = 0.06 * (i % 5 + 1) as f32;
            self.track_meters
                .entry(*id)
                .or_default()
                .update(Some(vec![a, a * 0.8]));
        }
        self.meters.update(Some(vec![0.3, 0.27]));
        if let Some(id) = tracks.get(2) {
            self.pick_track(*id, false, false);
        }
        self.console.selected = None;
    }

    /// The Mixing Console as VEGAS 23's prints show it after Show All: the
    /// Preview channel, the Meters and the Faders, eight audio tracks — the
    /// first three's Automation Settings on in Touch, then Read, Latch and
    /// Off —, three busses and the Master picked (for screen tests).
    pub fn mixing_console_show_all_for_review(&mut self) {
        use bv_timeline::AutomationMode;
        self.mixing_console_default_for_review();
        crate::tabs::console::show_all(self);
        for _ in 0..3 {
            crate::commands::Cmd::InsertAudioBus.run(self);
        }
        let tracks: Vec<TrackId> = self
            .project()
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
            .collect();
        for (i, id) in tracks.iter().enumerate() {
            let mode = match i {
                0..=2 => AutomationMode::Touch,
                4 => AutomationMode::Latch,
                5 => AutomationMode::Off,
                _ => AutomationMode::Read,
            };
            self.set_automation_mode(*id, mode);
            if i < 3 {
                self.toggle_automation_controls(*id);
            }
        }
        self.console.master_picked = true;
        self.console.selected = None;
    }

    /// Equalizer of a few bands — a high pass, a peak on the middle, a high
    /// shelf on the sides, a bypassed notch — Mono Below on, and its FX
    /// window open. Whether there was an audio event.
    pub fn modern_eq_for_review(&mut self) -> bool {
        use bv_engine::modern_eq::{Band, BandType, Route, Settings};
        let p = self.project();
        let Some(clip) = p
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .flat_map(|t| t.clips.iter())
            .map(|c| c.id)
            .next()
        else {
            return false;
        };
        let mut e = bv_timeline::Effect::new(bv_timeline::EffectKind::ModernEq);
        let mut s = Settings::of(&e);
        let band = |kind, freq, gain_db, q, route| Band {
            used: true,
            on: true,
            kind,
            freq,
            gain_db,
            q,
            slope: 24,
            route,
        };
        s.bands[0] = band(BandType::HighPass, 70.0, 0.0, 1.0, Route::Stereo);
        s.bands[1] = band(BandType::Peaking, 450.0, 4.5, 1.2, Route::Mid);
        s.bands[2] = band(BandType::HighShelf, 7000.0, -5.0, 1.0, Route::Side);
        s.bands[3] = Band {
            on: false,
            ..band(BandType::Peaking, 2500.0, -9.0, 6.0, Route::Stereo)
        };
        s.mono_below = 120.0;
        s.write(&mut e);
        self.modify(|p| {
            if let Some(c) = p.clip_mut(clip) {
                c.effects.push(e);
            }
        });
        self.select_clip(clip, false);
        self.open_clip_window("fx-audio");
        true
    }

    /// For the screen captures: the demo's audio track an Auto Ducking
    /// controller whose first event sounds for its first two seconds (its
    /// loudness as if read: no analysis), a new audio track below it a
    /// listener playing the demo's tone, and the controller's dialog open.
    pub fn ducking_for_review(&mut self) {
        let p = self.project();
        let Some(controller) = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
        else {
            return;
        };
        let Some((clip, media)) = p
            .track(controller)
            .and_then(|t| t.clips.first())
            .map(|c| (c.id, c.media))
        else {
            return;
        };
        self.modify(|p| {
            if let Some(c) = p.clip_mut(clip) {
                let from = c.source_in;
                let mut levels = vec![-15i8; 50];
                levels.extend(vec![-100i8; 200]);
                c.duck_levels = Some(bv_timeline::DuckLevels {
                    from,
                    levels,
                    own: None,
                });
            }
            if let Some(t) = p.track_by_id_mut(controller) {
                t.ducking = Some(bv_timeline::Ducking::default());
            }
            let listener = p.add_track(TrackKind::Audio);
            let _ = p.insert_media_range(
                media,
                Tick::ZERO,
                Tick::ZERO,
                Tick::from_seconds(10),
                None,
                Some(listener),
            );
            if let Some(t) = p.track_by_id_mut(listener) {
                t.duck_listener = true;
            }
        });
        self.duck_dialog = Some(crate::ducking_ui::DuckTarget::Track(controller));
    }

    /// For the screen captures: the beat dialogs closed.
    pub fn close_beat_dialogs_for_review(&mut self) {
        self.close_beat_dialogs();
    }

    /// For the screen captures: a ramp on a new top track, a compositing
    /// parent of the demo's video, its Compositing Mode Custom through a
    /// strong Displacement Map; the Compositors window shown, the
    /// compositor's settings open, the cursor at 2 s.
    pub fn compositors_for_review(&mut self) {
        self.compositor_for_review(
            "org.bananav.DisplacementMap",
            "Red Shifts Horizontally, Green Shifts Vertically",
        );
    }

    /// For the screen captures: a ramp on a new top track guiding the
    /// first video track through compositor `id` at its preset `preset`
    /// (Custom compositing), its settings window and the Compositors
    /// window open, the cursor at 2 s.
    pub fn compositor_for_review(&mut self, id: &str, preset: &str) {
        let Some(info) = crate::ofx::compositors()
            .into_iter()
            .find(|p| p.identifier == id)
        else {
            return;
        };
        let ramp = bv_timeline::MediaInfo::generated(
            bv_timeline::Generator::Gradient {
                from: [0.0, 0.0, 0.0, 1.0],
                to: [1.0, 1.0, 1.0, 1.0],
                angle_deg: 30.0,
            },
            &self.project().settings,
        );
        let video = self
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .map(|t| t.id);
        let Some(video) = video else {
            return;
        };
        let top = self.modify(|p| {
            let m = p.add_media(ramp);
            let top = p.insert_track_at(TrackKind::Video, 0);
            let _ = p.insert_media_range(
                m,
                Tick::ZERO,
                Tick::ZERO,
                Tick::from_seconds(15),
                Some(top),
                None,
            );
            let _ = p.make_compositing_child(video, false);
            top
        });
        let mut fx = crate::ofx::plugin_fx(&info);
        fx.params = crate::presets::builtin_plugin(id)
            .into_iter()
            .find(|(n, _)| n == preset)
            .map(|(_, v)| v)
            .unwrap_or_default();
        self.apply_compositor(top, false, fx);
        self.show_tab(crate::dock::Tab::Compositors, false);
        self.seek(Tick::from_seconds(2));
    }

    /// For the screen captures: the compositor's settings window closed.
    pub fn close_compositor_window_for_review(&mut self) {
        self.compositor_window = None;
    }

    /// For the screen captures: a second camera (the demo's gradient over
    /// its first ten seconds, on a track below) made with the demo's video
    /// into a multicamera track, multicamera editing on, the cursor at
    /// 2 s, no time selection.
    pub fn multicam_for_review(&mut self) {
        let p = self.project();
        let Some((v, gradient)) = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .and_then(|t| Some((t.id, t.clips.get(1)?.media)))
        else {
            return;
        };
        self.modify(|p| {
            let t2 = p.insert_track_at(TrackKind::Video, 1);
            let _ = p.insert_media_range(
                gradient,
                Tick::ZERO,
                Tick::ZERO,
                Tick::from_seconds(10),
                Some(t2),
                None,
            );
            let _ = p.create_multicamera_track(&[v, t2]);
        });
        self.modify(|p| p.loop_region = None);
        self.multicam = true;
        self.seek(Tick::from_seconds(2));
    }

    /// For the screen captures: the demo's first video event gets the
    /// second's media as a take (named) and the Take Chooser opens on it,
    /// take information shown on the events.
    pub fn takes_for_review(&mut self) {
        let Some((clip, other)) = self
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .and_then(|t| Some((t.clips.first()?.id, t.clips.get(1)?.media)))
        else {
            return;
        };
        self.add_take(clip, other);
        self.modify(|p| {
            let _ = p.rename_take(clip, 1, "Second try");
        });
        self.selection = vec![clip].into_iter().collect();
        self.timeline.show_take_info = true;
        self.open_take_chooser(false);
    }

    pub fn track_motion_for_review(&mut self) {
        let Some(v) = self
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .map(|t| t.id)
        else {
            return;
        };
        let tf = bv_timeline::Transform {
            scale: 0.55,
            x: 0.12,
            y: 0.08,
            rotation_deg: -8.0,
            ..bv_timeline::Transform::default()
        };
        self.modify(|p| {
            let new = (bv_timeline::KeyCurve::Linear, 0.0);
            let _ = p.set_track_motion_at(v, false, Tick::ZERO, tf, new);
            let _ = p.set_track_halo_on(v, false, true, true);
            let _ = p.set_track_halo_on(v, false, false, true);
            let glow = bv_timeline::HaloLook {
                color: [1.0, 0.85, 0.2, 1.0],
                size: 1.04,
                ..bv_timeline::HaloLook::GLOW
            };
            let _ = p.set_track_halo_at(v, false, false, Tick::ZERO, glow);
        });
        self.open_track_motion(v, false);
        if let Some(w) = &mut self.track_motion {
            w.row = crate::track_motion::Row::Shadow;
        }
    }

    /// 3D Track Motion (screen tests): the first video track in 3D Source
    /// Alpha, its box turned and nearer, the window open with `layout`'s
    /// panes (0 one, 5 four).
    pub fn track_motion_3d_for_review(&mut self, layout: usize) {
        let Some(v) = self
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .map(|t| t.id)
        else {
            return;
        };
        self.modify(|p| {
            if let Some(t) = p.track_by_id_mut(v) {
                t.composite_mode = bv_timeline::CompositeMode::ThreeDSourceAlpha;
            }
            if let Ok(m) = p.track_motion_mut(v, false) {
                let b = bv_timeline::space::Box3d {
                    z: -0.2,
                    width: 0.7,
                    height: 0.7,
                    rotation: [15.0, 30.0, 0.0],
                    ..bv_timeline::space::Box3d::default()
                };
                m.set_space_at(Tick::ZERO, b, bv_timeline::KeyCurve::Linear);
            }
        });
        self.open_track_motion(v, false);
        if let Some(w) = &mut self.track_motion {
            w.layout_for_review(layout);
        }
    }

    /// The demo's video events with their own envelopes, as VEGAS draws
    /// them: the gradient's velocity (half speed, then backward, then the
    /// frame frozen at 8 s), its crossfade's progress (up, back, up again)
    /// and the checkerboard reversed (for screen tests).
    pub fn event_envelopes_for_review(&mut self) {
        use bv_timeline::EnvelopeKind::{TransitionProgress, Velocity};
        let video: Vec<bv_timeline::ClipId> = self
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .map(|t| t.clips.iter().map(|c| c.id).collect())
            .unwrap_or_default();
        let [_, gradient, checker] = video[..] else {
            return;
        };
        let s = Tick::from_seconds_f64;
        self.modify(|p| {
            let _ = p.toggle_event_envelope(gradient, Velocity);
            let _ = p.set_envelope_point(gradient, Velocity, Tick::ZERO, 0.5);
            let _ = p.set_envelope_point(gradient, Velocity, s(2.0), -0.5);
            let _ = p.freeze_frame_at(gradient, s(8.0));
            let _ = p.toggle_event_envelope(gradient, TransitionProgress);
            let _ = p.set_envelope_point(gradient, TransitionProgress, s(0.5), 1.0);
            let _ = p.set_envelope_point(gradient, TransitionProgress, s(0.75), 0.4);
            let _ = p.reverse_event(checker);
        });
    }

    /// The Loudness Meters open, showing readings as if a program had
    /// played (LUFS, LU and dBTP), with `atsc` and `wide` as their mode and
    /// scale, on settings that are never saved (for screen tests).
    pub fn loudness_for_review(&mut self, readings: [f64; 5], atsc: bool, wide: bool) {
        self.settings = self.settings.unsaved();
        self.settings.loudness.atsc = atsc;
        self.settings.loudness.wide = wide;
        self.settings.loudness.hold_valleys = true;
        let [momentary, short_term, integrated, range, true_peak] = readings;
        self.loudness_view.show(bv_engine::LiveLoudness {
            momentary,
            short_term,
            integrated,
            range,
            max_momentary: momentary + 2.5,
            max_short_term: short_term + 1.2,
            min_momentary: momentary - 6.0,
            min_short_term: short_term - 3.0,
            true_peak,
            true_peaks: vec![true_peak, true_peak - 1.8],
        });
        self.show_tab(Tab::LoudnessMeters, false);
    }

    pub fn automation_for_review(&mut self) {
        self.settings = crate::settings::UiSettings::in_memory();
        self.device_explorer.open = true;
        let audio = self
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id);
        if let Some(t) = audio {
            self.toggle_automation_controls(t);
            self.set_automation_mode(t, bv_timeline::AutomationMode::Touch);
            let s = Tick::from_seconds;
            let samples: Vec<(Tick, f32)> = (0..=40)
                .map(|i| {
                    let at = s(1) + Tick(s(1).0 / 10 * i);
                    (at, 0.5 + 0.4 * (i as f32 / 6.0).sin())
                })
                .collect();
            let curve = self.settings.prefs.new_audio_envelopes;
            self.modify(|p| {
                let kind = bv_timeline::EnvelopeKind::Volume;
                let _ = p.write_automation(t, kind, &samples, true, curve);
            });
            self.timeline.show_envelopes = true;
        }
        self.trigger_mtc = true;
        self.time_display = crate::sync::TimeDisplay::MtcIn;
    }

    /// The project's media: names and, for image sequences, how many
    /// pictures (for screen tests).
    pub fn media_for_review(&self) -> Vec<(String, Option<u32>)> {
        self.project()
            .media
            .iter()
            .map(|m| (m.name.clone(), m.sequence.map(|s| s.count)))
            .collect()
    }

    /// Opens File › Import › Image Sequence for the run starting at `first`
    /// (for screen tests: no file dialog).
    pub fn image_sequence_for_review(&mut self, first: std::path::PathBuf) {
        crate::sequence_dialog::open_with(self, first);
    }

    /// Screen tests: Expanded Edit Mode at the start of the first track's
    /// event starting at `seconds`.
    pub fn expanded_edit_for_review(&mut self, seconds: f64) {
        let at = Tick::from_seconds_f64(seconds);
        let found = self
            .project()
            .tracks
            .first()
            .and_then(|t| t.clips.iter().find(|c| c.start == at).map(|c| c.id));
        if let Some(id) = found {
            self.select_clip(id, false);
            self.seek(at);
            self.toggle_expanded_edit();
        }
    }

    /// Screen tests: markers and regions (seconds, length, name) on the
    /// project's first media, opened in the Trimmer and shown with their
    /// names in the events. Nothing is saved with the file.
    pub fn media_markers_for_review(&mut self, marks: &[(f64, f64, &str)]) {
        let marks: Vec<bv_timeline::MediaMarker> = marks
            .iter()
            .map(|&(t, len, name)| bv_timeline::MediaMarker {
                time: Tick::from_seconds_f64(t),
                length: Tick::from_seconds_f64(len),
                name: name.to_string(),
            })
            .collect();
        self.modify_untracked(move |p| {
            if let Some(m) = p.media.first_mut() {
                m.markers = marks.clone();
            }
        });
        self.timeline.media_marker_labels = true;
        if let Some(id) = self.project().media.first().map(|m| m.id) {
            self.trimmer.open(id);
        }
    }

    /// Screen tests: the project in 32-bit floating point (full range)
    /// shown through ACES's `view` (`None`: color management off), in the
    /// project as it is open — nothing is saved.
    pub fn aces_for_review(&mut self, view: Option<bv_timeline::ViewTransform>) {
        self.modify_untracked(move |p| {
            p.settings.pixel_format = bv_timeline::PixelFormat::Float32Full;
            p.settings.color.view = view;
        });
    }

    /// Applies View › Window Layouts' `index`th built-in layout (for
    /// screen tests).
    pub fn layout_for_review(&mut self, index: usize) {
        if let Some(&p) = crate::app::actions::LayoutPreset::ALL.get(index) {
            self.apply_layout(p);
        }
    }

    /// Shows the Video Scopes' `view` (0–7, their list's order), on
    /// settings that are never saved (for screen tests).
    pub fn scopes_for_review(&mut self, view: usize) {
        self.settings = self.settings.unsaved();
        if let Some(&v) = crate::tabs::scopes::View::ALL.get(view) {
            self.settings.scopes.view = v;
        }
    }

    /// The Video Scopes' modes: the waveform's composite, the histogram's
    /// channels (0–6, their list's order), the parade combined with its
    /// channels shown; on settings that are never saved (for screen tests).
    pub fn scope_modes_for_review(
        &mut self,
        composite: bool,
        histogram: usize,
        combined: Option<[bool; 3]>,
    ) {
        use crate::tabs::scopes::{HistogramChannel, ParadeMode, WaveformMode};
        self.settings = self.settings.unsaved();
        let p = &mut self.settings.scopes;
        p.waveform = if composite {
            WaveformMode::Composite
        } else {
            WaveformMode::Luminance
        };
        p.histogram = HistogramChannel::ALL[histogram.min(6)];
        p.parade = if combined.is_some() {
            ParadeMode::Combined
        } else {
            ParadeMode::Separate
        };
        p.parade_channels = combined.unwrap_or([true; 3]);
    }

    /// The Video Preview at Best (Full) quality, the project's whole
    /// picture, as the user's VEGAS screenshots of the scopes were taken;
    /// on settings that are never saved (for screen tests).
    pub fn preview_best_full_for_review(&mut self) {
        self.settings = self.settings.unsaved();
        self.settings.preview.quality = crate::tabs::preview::PreviewQuality {
            level: bv_timeline::RenderQuality::Best,
            size: crate::tabs::preview::PreviewSize::Full,
        };
    }

    /// Opens Video Scope Settings (for screen tests: closed with Cancel).
    pub fn scope_settings_for_review(&mut self) {
        let settings = &self.settings.scopes.settings;
        self.menu_dialogs.more.scope_settings =
            Some(crate::tabs::scopes::SettingsDialog::new(settings));
    }

    /// Sets the project's pixel format (for screen tests).
    pub fn pixel_format_for_review(&mut self, format: bv_timeline::PixelFormat) {
        self.modify_untracked(move |p| p.settings.pixel_format = format);
    }

    /// Sets the project's compositing gamma (for screen tests).
    pub fn compositing_gamma_for_review(&mut self, gamma: bv_timeline::CompositingGamma) {
        self.modify_untracked(move |p| p.settings.compositing_gamma = gamma);
    }

    /// Opens Project Media's Properties for the project's first video
    /// file, else its first media (for screen tests).
    pub fn media_properties_for_review(&mut self) {
        let p = self.project();
        let id = p
            .media
            .iter()
            .find(|m| {
                m.video.is_some() && matches!(m.source, bv_timeline::MediaSource::File { .. })
            })
            .or_else(|| p.media.first())
            .map(|m| m.id);
        if let Some(id) = id {
            self.dialogs.media_properties =
                crate::media_properties::MediaPropertiesDialog::new(self, id);
        }
    }

    /// Sets the project's ACES version (for screen tests).
    pub fn aces_version_for_review(&mut self, version: bv_timeline::AcesVersion) {
        self.modify_untracked(move |p| p.settings.color.version = version);
    }

    /// Opens the Render window telling that Windows' Controlled folder
    /// access keeps BananaV out of `dir` (for screen tests).
    pub fn folder_blocked_for_review(&mut self, dir: &std::path::Path) {
        let mut d = crate::render::RenderDialog::new(self);
        d.set_problem(crate::render::folder::FolderProblem::blocked(dir));
        self.dialogs.render = Some(d);
    }

    /// The size the FX window's plug-in page takes: the window fits it
    /// (for screen tests).
    pub fn fx_page_size_for_review(&self) -> Option<egui::Vec2> {
        self.clip_windows.fx_page_size()
    }

    /// Where the Pan/Crop window drew its box's point (`sx`, `sy` from −1
    /// to 1 across and down; 0, 0 its middle), for screen tests.
    pub fn pan_crop_spot_for_review(&self, sx: f32, sy: f32) -> Option<egui::Pos2> {
        self.clip_windows.pan_crop_spot(sx, sy)
    }

    /// Where the Pan/Crop window's keyframe controller drew keyframe `i` of
    /// its Position row (its Mask row's, with `mask`), for screen tests.
    pub fn pan_crop_key_spot_for_review(&self, mask: bool, i: usize) -> Option<egui::Pos2> {
        self.clip_windows.pan_crop_key_spot(mask, i)
    }

    /// Where the Pan/Crop workspace shows the media's point (`fx`, `fy`
    /// parts of its width and height), for screen tests.
    pub fn pan_crop_media_spot_for_review(&self, fx: f32, fy: f32) -> Option<egui::Pos2> {
        self.clip_windows.pan_crop_media_spot(fx, fy)
    }

    /// A point `f` (0–1) of the way across the Pan/Crop window's Position
    /// row (its Mask row, with `mask`), for screen tests.
    pub fn pan_crop_lane_spot_for_review(&self, mask: bool, f: f32) -> Option<egui::Pos2> {
        self.clip_windows.pan_crop_lane_spot(mask, f)
    }

    /// The picked event's framing with five keyframes (its box moving and
    /// turning), and its Pan/Crop window open with its properties shown
    /// (screen tests; the settings are never saved there).
    pub fn pan_crop_keys_for_review(&mut self) -> bool {
        let Some(id) = self.primary_clip() else {
            return false;
        };
        let Some(c) = self.project().clip(id).cloned() else {
            return false;
        };
        let rate = self.project().settings.frame_rate;
        let keys: Vec<bv_timeline::TransformKey> = (0..5)
            .map(|i| {
                let at = rate.snap_floor(Tick(c.duration.0 * i / 6));
                let value = bv_timeline::Transform {
                    x: 0.05 * i as f32,
                    scale: 1.0 + 0.1 * i as f32,
                    rotation_deg: 4.0 * i as f32,
                    ..bv_timeline::Transform::default()
                };
                bv_timeline::TransformKey::new(at, value, bv_timeline::KeyCurve::Linear)
            })
            .collect();
        self.modify(|p| {
            if let Some(c) = p.clip_mut(id) {
                c.transform = keys[0].value;
                c.transform_keys = keys.clone();
            }
        });
        self.pan_crop_properties_for_review(true);
        self.open_clip_window("pancrop");
        true
    }

    /// The Pan/Crop window's Show Properties on or off (screen tests; the
    /// settings are never saved from then on).
    pub fn pan_crop_properties_for_review(&mut self, on: bool) {
        self.settings = self.settings.unsaved();
        self.settings.pan_crop_properties = on;
    }

    /// Gives the FX window a size, as the user dragging its edges (for
    /// screen tests).
    pub fn fx_window_size_for_review(&mut self, ctx: &egui::Context, size: egui::Vec2) {
        crate::floating::resize(ctx, egui::Id::new("janela-fx"), size);
    }

    /// The event the selection is about (for screen tests).
    pub fn clip_for_review(&self) -> Option<bv_timeline::Clip> {
        self.primary_clip()
            .and_then(|id| self.project().clip(id))
            .cloned()
    }

    /// Event `id` as it is now (for screen tests).
    pub fn clip_by_id_for_review(&self, id: bv_timeline::ClipId) -> Option<bv_timeline::Clip> {
        self.project().clip(id).cloned()
    }

    /// The media of the event the selection is about (for screen tests).
    pub fn clip_media_for_review(&self) -> Option<bv_timeline::MediaInfo> {
        let c = self.clip_for_review()?;
        self.project().media(c.media).cloned()
    }

    /// For screen tests: the top video track's effects (the Composite
    /// button `at` among them), its Video Track FX window open.
    pub fn track_chain_for_review(&mut self, effects: Vec<bv_timeline::Effect>, at: Option<usize>) {
        let Some(id) = self
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video && !t.adjustment)
            .map(|t| t.id)
        else {
            return;
        };
        self.modify(|p| {
            if let Some(t) = p.track_by_id_mut(id) {
                t.effects = effects;
                t.composite_at = at;
            }
        });
        self.clip_windows.track_fx = Some(id);
    }

    /// For screen tests: the selected event's effects (its Pan/Crop first),
    /// its Video Event FX window open.
    pub fn clip_chain_for_review(&mut self, effects: Vec<bv_timeline::Effect>) {
        let Some(id) = self.primary_clip() else {
            return;
        };
        self.modify(|p| {
            if let Some(c) = p.clip_mut(id) {
                c.effects = effects;
                c.pan_crop_at = 0;
            }
        });
        self.clip_windows.clip_fx = Some(id);
    }

    /// Abre uma janela do clipe selecionado (para testes de tela e automação):
    /// `"pancrop"`, `"fx"`, `"fx-audio"` ou `"extras"`.
    pub fn open_clip_window(&mut self, which: &str) {
        let p = self.project();
        let video = self.primary_clip();
        let audio = self
            .selection
            .iter()
            .copied()
            .find(|&id| p.clip_track(id).is_some_and(|t| t.kind == TrackKind::Audio));
        match which {
            "pancrop" => self.clip_windows.pan_crop = video,
            "fx" => self.clip_windows.clip_fx = video,
            "fx-audio" => self.clip_windows.clip_fx = audio,
            "extras" => {
                if let Some(clip) = video {
                    self.timeline.more_menu = Some(crate::timeline::MoreMenu {
                        clip,
                        at: egui::pos2(760.0, 640.0),
                        fresh: true,
                    });
                }
            }
            _ => {}
        }
    }

    /// Um quadro da interface inteira.
    pub fn frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let window = ui.max_rect();
        let rendering = self.dialogs.rendering.is_some();
        crate::floating::set_modal(&ctx, &self.modal_dialogs());

        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if rendering {
                // The render's window is modal: nothing closes under it.
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            } else if self.is_dirty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.confirm = Some(Pending::Exit);
            }
        }
        if std::mem::take(&mut self.exit_now) {
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if std::mem::take(&mut self.focus_timeline) {
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        self.track_selection_history();
        // A render stops playback and keeps it stopped, as VEGAS's: the
        // processor and the graphics card go to the render.
        if self.is_playing() && self.engine.render_progress().is_some_and(|p| !p.finished) {
            self.stop();
        }

        self.engine.set_project(self.history.current());
        if self.engine.poll_finished().is_some() {
            self.playhead = None;
        }
        if self.is_playing() {
            if let Some(pos) = self.engine.position() {
                self.playhead = Some(pos);
            }
            ctx.request_repaint();
        } else {
            self.playhead = None;
        }
        // Transport › Cursor Preview stops at the end of its part (it loops
        // with Loop Playback).
        if let (Some(r), Some(pos)) = (self.cursor_preview, self.playhead)
            && pos >= r.end()
            && !self.loop_enabled
        {
            self.stop();
        }
        if self.playhead.is_none() {
            self.cursor_preview = None;
        }
        self.meters.update(self.engine.take_peaks());
        // The audio output's failures, noted down (Logs\Audio.log) so what
        // went wrong can be found out later; the same one once in a row.
        for e in self.engine.take_audio_errors() {
            if self.last_audio_error.as_ref() != Some(&e) {
                crate::logs::audio(&e);
                self.last_audio_error = Some(e);
            }
        }
        self.follow_preview();
        // A media preview plays as the timeline does: each frame asks for
        // the picture of its moment (drawn ahead), shown when it comes.
        if self.previewing() {
            ctx.request_repaint();
        }
        let mut track_peaks: std::collections::HashMap<_, _> =
            self.engine.take_track_peaks().into_iter().collect();
        // Live input: armed tracks monitored show their input (through
        // their effects when heard), input busses theirs.
        self.follow_live();
        for (key, [l, r]) in self.engine.live_levels() {
            if key & crate::recording::INPUT_SEND_KEY != 0 {
                // An input bus's send: its chain's meters stay the project's.
                continue;
            } else if key & crate::recording::INPUT_BUS_KEY != 0 {
                let id = bv_timeline::InputBusId((key & 0xFFFF_FFFF) as u32);
                self.input_bus_levels.insert(id, [l, r]);
            } else {
                track_peaks.insert(TrackId(key), vec![l, r]);
            }
        }
        // Recording: the armed tracks' meters show their input.
        for (id, [l, r]) in self.engine.recording_levels() {
            track_peaks.insert(id, vec![l, r]);
        }
        if self.engine.is_recording() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        let audio: Vec<_> = self
            .project()
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
            .collect();
        self.track_meters.retain(|id, _| audio.contains(id));
        for id in audio {
            self.track_meters
                .entry(id)
                .or_default()
                .update(track_peaks.remove(&id));
        }
        let mut strip_peaks: std::collections::HashMap<_, _> =
            self.engine.take_strip_peaks().into_iter().collect();
        let strips = self.project().strips();
        self.strip_meters.retain(|s, _| strips.contains(s));
        for s in strips {
            self.strip_meters
                .entry(s)
                .or_default()
                .update(strip_peaks.remove(&s));
        }
        self.vu
            .update(self.engine.take_vu_levels(), std::time::Instant::now());
        if self.meters.active()
            || self.track_meters.values().any(MeterState::active)
            || self.strip_meters.values().any(MeterState::active)
            || self.vu.active()
        {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        self.update_os_drag(&ctx);
        if !rendering {
            self.shortcuts(&ctx);
        }
        self.update_title(&ctx);
        self.apply_look(&ctx);
        self.update_stats(&ctx);

        // The title bar, with the menus, as VEGAS 23's.
        egui::Panel::top("menu")
            .exact_size(crate::title_bar::HEIGHT)
            .frame(egui::Frame::new().fill(theme::panel_dark()))
            .show(ui, |ui| crate::title_bar::ui(ui, self));
        if self.show_toolbar {
            egui::Panel::top("toolbar")
                .frame(
                    egui::Frame::new()
                        .fill(theme::panel())
                        .inner_margin(egui::Margin::symmetric(6, 3)),
                )
                .show(ui, |ui| menus::toolbar(ui, self));
        }
        if self.show_status_bar {
            // As VEGAS 23's: 20 pixels under the windows, apart from them by
            // the docking area's dark.
            let (bar_h, gap) = (20.0, 8.0);
            egui::Panel::bottom("status")
                .exact_size(bar_h + gap)
                .frame(egui::Frame::new().fill(theme::panel_dark()))
                .show(ui, |ui| {
                    let r = ui.max_rect();
                    let bar =
                        egui::Rect::from_min_max(egui::pos2(r.left(), r.bottom() - bar_h), r.max);
                    ui.painter().rect_filled(bar, 0.0, theme::panel());
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(bar.shrink2(egui::vec2(8.0, 0.0))),
                        |ui| self.status_bar(ui),
                    );
                });
        }
        // Options › Preferences › Display › "Automatically hide docking
        // area": hidden, the timeline takes its room.
        if !self.settings.prefs.auto_hide_dock {
            self.dock_peek = false;
        }
        let dock_hidden = self.settings.prefs.auto_hide_dock && !self.dock_peek;
        let mut dock = self.dock.take().unwrap_or_else(dock::default_layout);
        self.console_drawn = false;
        dock::ensure_timeline(&mut dock, self.timeline_top);
        if dock_hidden {
            self.dock_strip(ui);
            let mut timeline = std::mem::take(&mut self.timeline);
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(theme::bg()))
                .show(ui, |ui| timeline.ui(ui, self));
            self.timeline = timeline;
            // Floating windows stay: the docked ones (the timeline's,
            // drawn above) have no room.
            let corner = egui::Rect::from_min_size(ui.max_rect().min, egui::Vec2::ZERO);
            ui.scope_builder(egui::UiBuilder::new().max_rect(corner), |ui| {
                dock::show(ui, &mut dock, self)
            });
        } else {
            // The timeline is one of the layout's windows.
            let area = egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(theme::bg()))
                .show(ui, |ui| dock::show(ui, &mut dock, self))
                .response
                .rect;
            self.follow_dock_peek(&ctx, area, dock::timeline_rect(&dock));
        }
        self.dock = Some(dock);
        self.keep_session(&ctx);
        // The VU/PPM meters: computed while the Mixing Console shows them.
        let vu = self
            .console_drawn
            .then(|| self.console.vu_wanted())
            .flatten();
        bv_engine::Engine::set_vu_meters(vu, self.console.vintage);

        let mut dialogs = std::mem::take(&mut self.dialogs);
        dialogs.show(&ctx, self);
        self.dialogs = dialogs;
        self.follow_modal(&ctx);
        self.finish_os_drop();
        let mut menu_dialogs = std::mem::take(&mut self.menu_dialogs);
        menu_dialogs.show(&ctx, self);
        self.menu_dialogs = menu_dialogs;
        // Text a window copied (Edit Details' Copy) goes to the clipboard.
        if let Some(text) = self.to_clipboard.take() {
            ctx.copy_text(text);
        }
        let mut windows = std::mem::take(&mut self.clip_windows);
        windows.show(&ctx, self);
        // Janelas abertas por elas mesmas durante este quadro entram também.
        merge_windows(&mut windows, std::mem::take(&mut self.clip_windows));
        self.clip_windows = windows;
        crate::fx_packages::dialog(&ctx, self);
        crate::tabs::media_tags::quick_tags_dialog(&ctx, self);
        crate::tabs::media_tags::search_dialog(&ctx, self);
        self.close_dialog(&ctx);
        self.ask_dialog(&ctx);
        self.generated_copies_dialog(&ctx);
        self.follow_activation();
        self.follow_plugin_crashes();
        follow_host_window();
        // MIDI: where playback is for the timecode and clock sent; the
        // timecode followed; the control surfaces; automation recorded.
        self.publish_transport();
        self.follow_timecode();
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        self.follow_surfaces(dt);
        self.follow_automation();
        self.follow_surround_write();
        self.follow_motion_tracking();
        self.follow_mask_tracking();
        self.follow_motion_analysis();
        self.follow_event_length();
        self.keep_switches();
        self.keep_sync_links();
        self.apply_track_defaults();
        if self.trigger_mtc
            || (self.settings.prefs.external_control
                && !self.settings.prefs.control_devices.is_empty())
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        self.autosave_tick();
        self.follow_ram_preview();
        self.track_color_dialog(&ctx);
        self.generator_color_dialog(&ctx);
        self.time_display_color_dialog(&ctx);
        self.generated_media_window(&ctx);
        self.track_motion_window(&ctx);
        self.take_windows(&ctx);
        self.compositor_window(&ctx);
        self.beat_dialogs(&ctx);
        self.nonrealtime_window(&ctx);
        self.track_defaults_window(&ctx);
        self.motion_tracking_window(&ctx);
        self.screen_capture_window(&ctx);
        self.header_buttons_window(&ctx);
        self.transport_buttons_window(&ctx);
        self.ducking_window(&ctx);
        self.poll_audio_sync();
        self.follow_proxies(&ctx);
        self.prerender_window(&ctx);
        self.follow_prerender(&ctx);
        self.follow_joystick(&ctx);
        self.follow_recording_waves();
        self.recorded_files_window(&ctx);
        self.external_monitor_window(&ctx);
        self.dvd_import_window(&ctx);
        self.device_explorer_window(&ctx);
        crate::otio_ui::follow(self, &ctx);
        crate::archive::follow(self, &ctx);
        crate::archive::archive_dialog(&ctx, self);
        crate::archive::copy_media_dialog(&ctx, self);
        if !self.removable_copies.is_empty() {
            self.finish_removable_copies();
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }

        // Rótulo flutuante do arrasto e fim do arrasto ao soltar.
        if let Some(drag) = &self.media_drag {
            let pos = match &self.os_drag {
                Some(d) => d.pointer,
                None => ctx.pointer_latest_pos(),
            };
            if let Some(pos) = pos {
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::new("arrasto-midia"),
                ));
                let label = drag.label(self.project());
                let galley = painter.layout_no_wrap(
                    label,
                    egui::FontId::proportional(13.0),
                    theme::banana(),
                );
                let at = pos + egui::vec2(16.0, 10.0);
                let rect = egui::Rect::from_min_size(at, galley.size()).expand(4.0);
                painter.rect_filled(rect, 3.0, theme::panel_dark());
                painter.galley(at, galley, theme::banana());
            }
            if ctx.input(|i| i.pointer.any_released()) {
                self.media_drag = None;
            }
        }
        // What plug-ins' buttons may ask for next frame, and what they
        // could not have.
        let video_gamma = self.project().settings.composites_in_video_gamma();
        self.ofx.set_pictures(crate::ofx_pictures::Sources {
            preview: self.engine.preview_frame(),
            trimmer: self.engine.trimmer_frame(),
            video_gamma,
        });
        if let Some(n) = self.ofx.take_notice() {
            self.notify(n);
        }
        // A Color Grading picker waits only while its panel shows.
        crate::tabs::color::end_frame(self);
        // Plug-in effects whose settings no window showed let go of their
        // instances.
        self.ofx.end_frame();
        // No Pan/Crop window: its picture's readers go.
        if !std::mem::take(&mut self.source_watched) {
            self.engine.close_source_view();
        }
        // Over everything: the window's edges size it.
        crate::title_bar::resize_edges(ui, window);
    }

    /// The window's title ("Untitled* — BananaV"), which the title bar
    /// shows.
    pub(crate) fn window_title(&self) -> &str {
        &self.title
    }
}

impl BananaApp {
    /// The dialogs that take input now: the render's window and its
    /// question while it is up (none: every window takes it).
    fn modal_dialogs(&self) -> Vec<egui::Id> {
        let mut ids = match &self.dialogs.rendering {
            Some(_) => crate::render::progress::ProgressWindow::ids().to_vec(),
            None => Vec::new(),
        };
        // Tools › Burn Disc's jobs.
        if self.menu_dialogs.more.burn_job.is_some() {
            ids.push(egui::Id::new(crate::burn_job::JOB_ID));
        }
        ids
    }

    /// While the render's window is up the rest of the program takes no
    /// input, as under VEGAS's (its progress window is modal): the OS
    /// blocks the other windows, a shield covers the main window's
    /// contents (and the dialogs', `floating`), and the keyboard's
    /// shortcuts, the joystick and the control surfaces wait.
    fn follow_modal(&mut self, ctx: &egui::Context) {
        crate::floating::set_modal(ctx, &self.modal_dialogs());
        // Embedded dialogs (the tests') are windows over the main one: the
        // OS has none of them to block, and the shield goes under them,
        // over the main window's panels.
        let embedded = ctx.embed_viewports();
        match (&self.dialogs.rendering, &self.input_block) {
            (Some(w), None) if !embedded => {
                self.input_block = Some(bv_platform::block_input(&[
                    w.title(),
                    crate::render::progress::cancel_warning_title(),
                ]));
            }
            (None, Some(_)) => {
                // Now, before the render's window closes: the OS gives the
                // focus to its owner only when that takes input.
                self.input_block = None;
                ctx.request_repaint();
            }
            _ => {}
        }
        if crate::floating::modal(ctx) {
            let order = if embedded {
                egui::Order::Background
            } else {
                egui::Order::Foreground
            };
            crate::floating::shield(ctx, order, !embedded);
        }
    }

    /// Options › Preferences › General: "Enable joystick support": the
    /// joystick's push, for the control under the mouse pointer (see
    /// `widgets::joystick_push`). Asked a few times a second while it is at
    /// rest, every frame while it is pushed; a missing one, every 2 s.
    fn follow_joystick(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new(crate::widgets::JOYSTICK);
        if !self.settings.prefs.joystick || crate::floating::modal(ctx) {
            ctx.data_mut(|d| d.remove::<egui::Vec2>(id));
            return;
        }
        let now = Instant::now();
        if self.joystick_retry.is_some_and(|t| now < t) {
            return;
        }
        match bv_platform::joystick() {
            None => {
                self.joystick_retry = Some(now + std::time::Duration::from_secs(2));
                ctx.data_mut(|d| d.remove::<egui::Vec2>(id));
            }
            Some((x, y)) => {
                self.joystick_retry = None;
                let v = crate::widgets::joystick_dead_zone(egui::vec2(x, y));
                ctx.data_mut(|d| d.insert_temp(id, v));
                if v == egui::Vec2::ZERO {
                    ctx.request_repaint_after(std::time::Duration::from_millis(150));
                } else {
                    ctx.request_repaint();
                }
            }
        }
    }
}

/// Where this session copies media from removable drives.
fn removable_folder(prefs: &crate::settings::Prefs) -> PathBuf {
    prefs
        .temp_dir()
        .join("Removable")
        .join(std::process::id().to_string())
}

/// Reads `original` from `copy` from now on.
fn bv_media_copy(original: &str, copy: &std::path::Path) {
    bv_engine::use_local_copy(original, &copy.to_string_lossy());
}

/// The project's media files as they are now: path, last change and size.
fn media_stamps(p: &Project) -> Vec<(String, Option<std::time::SystemTime>, u64)> {
    p.media
        .iter()
        .filter_map(|m| match &m.source {
            bv_timeline::MediaSource::File { path } => {
                let meta = std::fs::metadata(path).ok();
                Some((
                    path.clone(),
                    meta.as_ref().and_then(|m| m.modified().ok()),
                    meta.map_or(0, |m| m.len()),
                ))
            }
            bv_timeline::MediaSource::Generated(_) => None,
        })
        .collect()
}

impl eframe::App for BananaApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let start = std::time::Instant::now();
        self.frame(ui);
        frame_trace(start);
    }

    /// Remembers the project open now, for the next start, and deletes the
    /// copies of removable media.
    fn on_exit(&mut self) {
        self.closing_project();
        self.settings.last_session = self.project_path.clone();
        if self.session_kept.is_some() {
            self.settings.session = self.session_now().or(self.settings.session.take());
        }
        let _ = self.settings.save();
        self.autosave_cleanup();
        bv_engine::forget_local_copies();
        let _ = std::fs::remove_dir_all(removable_folder(&self.settings.prefs));
    }
}

/// Plug-ins get the main window as their host's (the parent of their own
/// dialogs) once the system has it: looked for in the first frames — the
/// tests' apps have none.
fn follow_host_window() {
    use std::sync::atomic::{AtomicU32, Ordering};
    static TRIES: AtomicU32 = AtomicU32::new(0);
    let tries = TRIES.load(Ordering::Relaxed);
    if tries >= 300 {
        return;
    }
    match bv_platform::window_handle(&crate::floating::main_title()) {
        Some(window) => {
            bv_ofx::set_host_window(window);
            TRIES.store(u32::MAX, Ordering::Relaxed);
        }
        None => TRIES.store(tries + 1, Ordering::Relaxed),
    }
}

/// `BV_FRAME_TRACE=1`: each frame of the main window on the standard error,
/// how long after the last one it started and how long it took (the
/// dialogs' windows, drawn inside it, included), to find out what slows
/// the interface down.
fn frame_trace(start: std::time::Instant) {
    static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    if !*TRACE.get_or_init(|| std::env::var_os("BV_FRAME_TRACE").is_some_and(|v| v == "1")) {
        return;
    }
    let took = start.elapsed().as_secs_f64() * 1000.0;
    let Ok(mut last) = LAST.lock() else { return };
    let since = last.map_or(0.0, |l| start.duration_since(l).as_secs_f64() * 1000.0);
    *last = Some(start);
    eprintln!("frame +{since:.1} ms, took {took:.1} ms");
}

/// Junta pedidos feitos durante o quadro (ex.: o menu "⋯" abrindo uma janela
/// enquanto as janelas estavam fora do app) ao estado das janelas.
fn merge_windows(
    dst: &mut crate::clip_windows::ClipWindows,
    src: crate::clip_windows::ClipWindows,
) {
    if src.pan_crop.is_some() {
        dst.pan_crop = src.pan_crop;
    }
    if src.clip_fx.is_some() {
        dst.clip_fx = src.clip_fx;
    }
    if src.media_fx.is_some() {
        dst.media_fx = src.media_fx;
    }
    if src.track_fx.is_some() {
        dst.track_fx = src.track_fx;
    }
    dst.output_fx |= src.output_fx;
    if src.rate.is_some() {
        dst.rate = src.rate;
    }
    if src.duration.is_some() {
        dst.duration = src.duration;
    }
    if src.paste.is_some() {
        dst.paste = src.paste;
    }
    if src.scenes.is_some() {
        dst.scenes = src.scenes;
    }
    if src.fx_target.is_some() {
        dst.fx_target = src.fx_target;
    }
    if src.fx_node.is_some() {
        dst.fx_node = src.fx_node;
    }
    dst.buttons_editor |= src.buttons_editor;
}

/// Transport › Cursor Preview's part: `seconds` long, the cursor in its
/// middle (never before the timeline's start).
pub(crate) fn preview_range(cursor: Tick, seconds: f64) -> TimeRange {
    let half = Tick::from_seconds_f64(seconds.max(0.1) / 2.0);
    TimeRange::from_bounds((cursor - half).max(Tick::ZERO), cursor + half)
}
