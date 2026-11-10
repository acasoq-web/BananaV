//! The timeline: time bars (loop, ruler, markers), track headers, lanes
//! with the events, scrolling and the bottom toolbar.

mod bars;
mod marker_bars;
pub(crate) mod marker_tool;
pub(crate) use bars::beat_steps;
mod bus_tracks;
pub(crate) use headers::scribble_name;
pub(crate) use lanes::edit_point_events;
pub(crate) use lanes::{Arrange, added_overlap_for, added_starts};
pub(crate) mod envelopes;
pub(crate) mod groups;
mod header_icons;
mod headers;
pub(crate) use headers::paint_meter_scaled;
pub(crate) use headers::pan_type_items;
pub(crate) use headers::record_input_menu;
mod lanes;
mod ofx_envelopes;
mod output_keys;
mod shortcut_menus;
pub(crate) mod surround_keys;
pub(crate) mod toolbar;

use bv_core::{TICKS_PER_SECOND, Tick};
use bv_timeline::{Gesture, Project, TrackId, TrackKind};

use crate::app::BananaApp;
use crate::cursors::{self, Cursor};
use crate::egui::{self, Pos2, Rect, pos2};
use crate::i18n::tr;
use crate::keymap::Context;
use crate::settings::SnapKind;
use crate::theme;

pub(crate) const HEADER_W: f32 = 232.0;
/// The narrowest the track list gets: the color strips and their numbers;
/// and the widest.
pub(crate) const MIN_HEADER_W: f32 = 26.0;
pub(crate) const MAX_HEADER_W: f32 = 4000.0;
const LOOP_H: f32 = 12.0;
const RULER_H: f32 = 24.0;
const MARKER_H: f32 = 16.0;
const TOP_H: f32 = LOOP_H + RULER_H + MARKER_H;
const SCROLL_H: f32 = 16.0;
const TOOLBAR_H: f32 = 34.0;
const VSCROLL_W: f32 = 12.0;
/// The dark splitter between the track list and the timeline, top to
/// bottom, as VEGAS's: dragged, the list's width.
pub(crate) const SPLIT_W: f32 = 6.0;
const MIN_TICKS_PER_PX: f64 = 2.0e6;
const MAX_TICKS_PER_PX: f64 = 4.0e12;

/// A minimized track: only the header's top row.
pub(crate) const MIN_TRACK_H: f32 = 26.0;
/// The tallest a track gets by dragging its edge.
pub(crate) const MAX_TRACK_H: f32 = 600.0;

pub(crate) fn track_height(kind: TrackKind, compact: bool) -> f32 {
    match (kind, compact) {
        (TrackKind::Video, false) => 78.0,
        (TrackKind::Audio, false) => 70.0,
        (_, true) => 48.0,
    }
}

pub(crate) struct Row {
    pub track: TrackId,
    pub kind: TrackKind,
    pub top: f32,
    pub height: f32,
    /// Its Surround Pan Keyframes row's height, at its bottom (0: none).
    pub keys: f32,
    /// The track group it is in (its header then starts after the
    /// grouping lane).
    pub group: Option<bv_timeline::TrackGroupId>,
}

impl Row {
    pub fn rect(&self, x0: f32, x1: f32) -> Rect {
        Rect::from_min_max(pos2(x0, self.top), pos2(x1, self.top + self.height))
    }

    /// The track without its keyframes row: where its events are.
    pub fn body(&self, x0: f32, x1: f32) -> Rect {
        Rect::from_min_max(
            pos2(x0, self.top),
            pos2(x1, self.top + self.height - self.keys),
        )
    }

    /// Its keyframes row.
    pub fn keys_rect(&self, x0: f32, x1: f32) -> Rect {
        Rect::from_min_max(
            pos2(x0, self.top + self.height - self.keys),
            pos2(x1, self.top + self.height),
        )
    }
}

pub(crate) struct Layout {
    pub tc_box: Rect,
    /// View › Audio Bus Tracks: the splitter over them, their headers and
    /// lanes, their rows and how tall they are together (empty when not
    /// shown).
    pub bus_split: Rect,
    pub bus_headers: Rect,
    pub bus_lanes: Rect,
    pub bus_rows: Vec<bus_tracks::BusRow>,
    pub bus_content: f32,
    pub loop_bar: Rect,
    pub ruler: Rect,
    pub marker_bar: Rect,
    /// The CD layout bar and the command bar, under the marker bar while
    /// the project has them (no height otherwise).
    pub cd_bar: Rect,
    pub command_bar: Rect,
    /// The Marker Tool's button, at the top right.
    pub marker_tool: Rect,
    pub headers: Rect,
    pub lanes: Rect,
    pub vscroll: Rect,
    pub rate: Rect,
    pub hscroll: Rect,
    pub toolbar: Rect,
    pub rows: Vec<Row>,
    /// The track groups' header rows (their collapsed tracks have none).
    pub groups: Vec<groups::GroupRow>,
    pub content_height: f32,
}

impl Layout {
    pub fn row_at(&self, y: f32) -> Option<&Row> {
        self.rows
            .iter()
            .find(|r| y >= r.top && y < r.top + r.height)
    }

    pub fn row_of(&self, track: TrackId) -> Option<&Row> {
        self.rows.iter().find(|r| r.track == track)
    }
}

/// O menu "⋯" de um clipe aberto na timeline.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MoreMenu {
    pub clip: bv_timeline::ClipId,
    pub at: Pos2,
    /// Aberto neste quadro: o clique que o abriu não o fecha.
    pub fresh: bool,
}

/// A needle being dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Needle {
    /// The edit cursor (playback goes on meanwhile).
    Cursor,
    /// The playback position: playback waits, and goes on from where it
    /// is let go.
    Playhead,
    /// Both (the ruler grabbed elsewhere than a needle's head).
    Both,
}

/// Edge-trimming mode (`[`, `]`) and Expanded Edit Mode (Edit › Select ›
/// Enter Expanded Edit Mode, or a double-click on an event edge): the event
/// edge being trimmed, marked by a red bracket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EdgeTrim {
    pub track: TrackId,
    pub clip: bv_timeline::ClipId,
    /// The event's end (else its start).
    pub end: bool,
    /// Expanded Edit Mode: the track shown as an A roll (the events before
    /// the edit point) over a B roll (the events after it).
    pub expanded: bool,
    /// Events starting here or later are on the B roll.
    pub pivot: Tick,
}

/// The events picked and the events selected (`BananaApp::picked`,
/// `selection`) when the selection rectangle was first dragged.
pub(crate) type BoxBase = (
    std::collections::BTreeSet<bv_timeline::ClipId>,
    std::collections::BTreeSet<bv_timeline::ClipId>,
);

/// Estado de visualização da timeline (zoom, rolagem, arrastos em curso).
pub struct TimelineView {
    /// Nome, timecode de origem e velocidade no cabeçalho dos clipes.
    pub show_take_info: bool,
    pub show_clip_headers: bool,
    pub(crate) buttons: crate::clip_windows::ClipButtons,
    pub(crate) more_menu: Option<MoreMenu>,
    pub(crate) view_start: Tick,
    pub(crate) ticks_per_px: f64,
    pub(crate) scroll_y: f32,
    pub show_thumbnails: bool,
    pub show_waveforms: bool,
    /// View › Event Buttons.
    pub show_buttons: bool,
    /// View › Event Transition Durations: fade lengths written on events.
    pub show_fade_durations: bool,
    /// View › Event Duration: duration shown in the event header.
    pub show_event_duration: bool,
    /// View › Show Envelopes: level lines and fade handles.
    pub show_envelopes: bool,
    /// View › Minimize All Tracks.
    pub compact_tracks: bool,
    /// Options › Quantize to Frames › Show Unquantized Event Edges.
    pub show_unquantized: bool,
    /// View › Audio/Video Envelopes: which envelope types are drawn.
    pub show_env_volume: bool,
    pub show_env_pan: bool,
    pub show_env_mute: bool,
    /// View › Video Envelopes: Track Composite Level, Track Fade to Color.
    pub show_env_composite: bool,
    pub show_env_fade: bool,
    /// View › Show Envelopes › Video Envelopes › Event Velocity.
    pub show_env_velocity: bool,
    /// View › Show Envelopes › Video Envelopes › Transition Progress.
    pub show_env_progress: bool,
    /// View › Event Media Markers: the media files' markers and regions in
    /// the events, with their names or without.
    pub show_media_markers: bool,
    pub media_marker_labels: bool,
    /// View › Audio Bus Tracks (B): a track for each bus and chain at the
    /// timeline's bottom.
    pub show_bus_tracks: bool,
    /// View › Video Bus Track (Ctrl+Shift+B): the video output's track
    /// there, above them.
    pub show_video_bus: bool,
    /// View › Show Envelopes › Video Envelopes: the video bus's Motion Blur
    /// Amount and Video Supersampling.
    pub show_env_motion_blur: bool,
    pub show_env_supersampling: bool,
    /// View › Show Envelopes › Audio Envelopes › Surround Pan Keyframes:
    /// the keyframes rows of 5.1 projects; a keyframe being dragged, the
    /// one picked and the one whose menu is open.
    pub show_surround_keys: bool,
    /// View › Show OFX Effect Envelopes (Beta): plug-in effects' animated
    /// parameters as envelopes (`ofx_envelopes.rs`); View › Video
    /// Envelopes › Track Keyframes: the video tracks' keyframes areas.
    pub show_ofx_envelopes: bool,
    pub show_track_keys: bool,
    /// Video tracks whose keyframes area is open (Expand Track Keyframes),
    /// and the effect whose envelopes show there.
    pub(crate) track_keys: std::collections::HashMap<TrackId, usize>,
    pub(crate) ofx_drag: Option<ofx_envelopes::OfxDrag>,
    pub(crate) ofx_drags: u64,
    /// The OFX envelope under the pointer (its name, its value, where), and
    /// the one a menu is for.
    pub(crate) ofx_hover: Option<(String, String, egui::Pos2)>,
    pub(crate) ofx_menu: Option<ofx_envelopes::OfxHit>,
    /// The effects' keyframe or envelope under the pointer, or dragged,
    /// this frame: its pointer shows over the tracks' under it.
    pub(crate) ofx_spot: Option<ofx_envelopes::OfxSpot>,
    /// The envelopes the keyframes areas hide, and a point's value being
    /// typed (Set to…).
    pub(crate) ofx_hidden: std::collections::HashSet<ofx_envelopes::EnvelopeKey>,
    pub(crate) ofx_set_to: Option<ofx_envelopes::OfxSetTo>,
    /// A track envelope point's Set To…: its value typed at the point.
    pub(crate) env_set_to: Option<envelopes::SetTo>,
    /// Where the envelopes' points and the keyframes areas' envelopes were
    /// drawn last (screen tests).
    pub(crate) ofx_painted: Vec<(usize, egui::Pos2)>,
    pub(crate) ofx_areas: Vec<egui::Rect>,
    /// The headers' sliders that take the wheel (drawn last frame): the
    /// tracks do not scroll under them.
    pub(crate) wheel_sliders: Vec<egui::Rect>,
    pub(crate) surround_drag: Option<surround_keys::KeyDrag>,
    pub(crate) surround_pick: Option<(bv_timeline::SurroundOwner, usize)>,
    pub(crate) surround_menu: Option<(bv_timeline::SurroundOwner, usize)>,
    /// The video bus track's keyframes rows: a Video Output FX's
    /// keyframes being dragged (the effect's place, where they are now),
    /// and the one a menu is for.
    pub(crate) output_key_drag: Option<(usize, Tick)>,
    pub(crate) output_key_menu: Option<(usize, Tick)>,
    /// Their area's height (its splitter dragged), each one's own height
    /// (its bottom edge dragged, Minimize, Maximize) and how far down they
    /// are scrolled.
    pub(crate) bus_area: f32,
    pub(crate) bus_heights: std::collections::HashMap<bus_tracks::BusTrack, f32>,
    /// The bus tracks' area was clicked last (not the tracks above):
    /// Ctrl+Shift+Up/Down size its tracks.
    pub(crate) bus_focus: bool,
    pub(crate) bus_scroll: f32,
    /// A bus track's envelope point being dragged; what its envelope menu
    /// is for.
    pub(crate) bus_drag: Option<bus_tracks::BusDrag>,
    pub(crate) bus_point: Option<bus_tracks::BusPoint>,
    /// The event edge being trimmed (edge-trimming or Expanded Edit Mode).
    pub(crate) edge_trim: Option<EdgeTrim>,
    /// Video tracks shown with Expand Track Layers (their header's menu):
    /// the A roll, the transition roll and the B roll; and, worked out each
    /// frame, their events' starts in order (they alternate between the
    /// rolls).
    pub(crate) layers: std::collections::HashSet<TrackId>,
    pub(crate) layer_starts: std::collections::HashMap<TrackId, Vec<Tick>>,
    pub(crate) drag: Option<lanes::Drag>,
    pub(crate) bar_drag: Option<bars::BarDrag>,
    /// The Marker Tool (the button at the top right).
    pub(crate) marker_tool: marker_tool::MarkerTool,
    /// The needle grabbed on the ruler.
    pub(crate) needle: Option<Needle>,
    pub(crate) gesture: Option<Gesture<Project>>,
    /// Where an edit snapped, and to what.
    pub(crate) snap_line: Option<(Tick, crate::settings::SnapKind)>,
    pub(crate) drop: Option<lanes::Drop>,
    /// An event dragged below the tracks: the new track it lands on.
    pub(crate) new_track: Option<lanes::NewTrack>,
    pub(crate) context: lanes::ContextTarget,
    pub(crate) lanes_width: f32,
    pub(crate) lanes_height: f32,
    /// The track list's width (from the settings each frame), and the full
    /// width a double-click on its border goes back to.
    pub(crate) header_w: f32,
    pub(crate) full_header_w: f32,
    /// Track whose name is being edited in its header, with the text.
    pub(crate) renaming: Option<(TrackId, String)>,
    /// A track group whose name is being edited, with the text.
    pub(crate) renaming_group: Option<(bv_timeline::TrackGroupId, String)>,
    /// A header value being typed (double-click on "Level", the dB or the
    /// pan): the track, which value and the text.
    pub(crate) value_edit: Option<(TrackId, headers::HeaderValue, String)>,
    /// The bus or chain each audio track's multipurpose slider shows (the
    /// first bus, else the first chain, by default).
    pub(crate) send_targets: std::collections::HashMap<TrackId, bv_timeline::SendTo>,
    /// A track header being dragged: to reorder the tracks, or its bottom
    /// edge to resize the track.
    pub(crate) track_drag: Option<headers::TrackDrag>,
    /// Heights set per track (the header's Minimize / Maximize).
    pub(crate) track_heights: std::collections::HashMap<TrackId, f32>,
    /// The heights tracks had before Minimize or Maximize, which the same
    /// button (or Restore Track Height) brings back.
    pub(crate) track_restore: std::collections::HashMap<TrackId, f32>,
    /// The scrub control's playback rate being typed (double-click on
    /// "Rate").
    pub(crate) rate_text: Option<String>,
    /// The toolbar's time box being typed into (double-clicked) and its
    /// text.
    pub(crate) time_edit: Option<(toolbar::TimeBox, String)>,
    /// Shift held: snapping is overridden while dragging (VEGAS's
    /// "Enable Snapping (Hold Shift to Override)"), or turned on when it
    /// is off.
    pub(crate) snap_override: bool,
    /// The event a Shift+click selects a range from: the last one clicked.
    pub(crate) range_anchor: Option<bv_timeline::ClipId>,
    /// Auto-scroll: the mouse wheel clicked (or held) in the tracks.
    pub(crate) autoscroll: Option<lanes::AutoScroll>,
    /// The press that stopped auto-scrolling: it does nothing else.
    pub(crate) ignore_press: bool,
    /// The drag in progress was started with the right mouse button.
    pub(crate) right_drag: bool,
    /// A right-click changed the selection rectangle's shape: it opens no
    /// menu.
    pub(crate) box_click: bool,
    /// The events picked and selected when the selection rectangle was
    /// first dragged: what it adds to (with Ctrl), while it picks the
    /// events it takes as it goes.
    pub(crate) box_base: Option<BoxBase>,
    /// The event clicked last and when (the input's time): pressed again
    /// at once and dragged, it extends the time selection.
    pub(crate) event_click: Option<(bv_timeline::ClipId, f64)>,
    /// Events dragged with the right mouse button and let go: the menu
    /// asking to move or copy them there.
    pub(crate) drop_menu: Option<lanes::DropMenu>,
    /// Media dragged in with the right mouse button and let go: the menu
    /// asking how to add it.
    pub(crate) media_drop_menu: Option<lanes::MediaDropMenu>,
    /// Mouse wheel notches not used yet (Ctrl+Shift+wheel on smooth
    /// wheels).
    pub(crate) wheel_rest: f32,
    /// Where the timeline was last drawn.
    pub(crate) last_rect: Rect,
}

/// The View menu's switches of the timeline — what events show, the
/// envelopes, the bus tracks… —, which BananaV opens with again, as VEGAS
/// keeps them (see `crate::settings::Session`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TimelineSwitches {
    pub take_info: bool,
    pub event_headers: bool,
    pub thumbnails: bool,
    pub waveforms: bool,
    pub buttons: bool,
    pub fade_durations: bool,
    pub event_duration: bool,
    pub envelopes: bool,
    pub unquantized: bool,
    pub env_volume: bool,
    pub env_pan: bool,
    pub env_mute: bool,
    pub env_composite: bool,
    pub env_fade: bool,
    pub env_velocity: bool,
    pub env_progress: bool,
    pub env_motion_blur: bool,
    pub env_supersampling: bool,
    pub media_markers: bool,
    pub media_marker_labels: bool,
    pub bus_tracks: bool,
    pub video_bus: bool,
    pub surround_keys: bool,
    pub ofx_envelopes: bool,
    pub track_keys: bool,
}

impl Default for TimelineSwitches {
    fn default() -> TimelineSwitches {
        TimelineView::default().switches()
    }
}

/// A drag within `EDGE_ZONE` pixels of the tracks' edges, or past them,
/// runs the view that way: `EDGE_SPEED` pixels a second at the edge,
/// `EDGE_ACCEL` more for each pixel past it, `EDGE_MAX` at most.
const EDGE_ZONE: f32 = 10.0;
const EDGE_SPEED: f32 = 300.0;
const EDGE_ACCEL: f32 = 18.0;
const EDGE_MAX: f32 = 4000.0;

/// How fast the view runs for a pointer `into` pixels into an edge's zone
/// (nothing outside it).
fn edge_speed(into: f32) -> f32 {
    if into <= 0.0 {
        0.0
    } else {
        (EDGE_SPEED + (into - EDGE_ZONE).max(0.0) * EDGE_ACCEL).min(EDGE_MAX)
    }
}

impl TimelineView {
    pub(crate) fn switches(&self) -> TimelineSwitches {
        TimelineSwitches {
            take_info: self.show_take_info,
            event_headers: self.show_clip_headers,
            thumbnails: self.show_thumbnails,
            waveforms: self.show_waveforms,
            buttons: self.show_buttons,
            fade_durations: self.show_fade_durations,
            event_duration: self.show_event_duration,
            envelopes: self.show_envelopes,
            unquantized: self.show_unquantized,
            env_volume: self.show_env_volume,
            env_pan: self.show_env_pan,
            env_mute: self.show_env_mute,
            env_composite: self.show_env_composite,
            env_fade: self.show_env_fade,
            env_velocity: self.show_env_velocity,
            env_progress: self.show_env_progress,
            env_motion_blur: self.show_env_motion_blur,
            env_supersampling: self.show_env_supersampling,
            media_markers: self.show_media_markers,
            media_marker_labels: self.media_marker_labels,
            bus_tracks: self.show_bus_tracks,
            video_bus: self.show_video_bus,
            surround_keys: self.show_surround_keys,
            ofx_envelopes: self.show_ofx_envelopes,
            track_keys: self.show_track_keys,
        }
    }

    pub(crate) fn set_switches(&mut self, s: TimelineSwitches) {
        self.show_take_info = s.take_info;
        self.show_clip_headers = s.event_headers;
        self.show_thumbnails = s.thumbnails;
        self.show_waveforms = s.waveforms;
        self.show_buttons = s.buttons;
        self.show_fade_durations = s.fade_durations;
        self.show_event_duration = s.event_duration;
        self.show_envelopes = s.envelopes;
        self.show_unquantized = s.unquantized;
        self.show_env_volume = s.env_volume;
        self.show_env_pan = s.env_pan;
        self.show_env_mute = s.env_mute;
        self.show_env_composite = s.env_composite;
        self.show_env_fade = s.env_fade;
        self.show_env_velocity = s.env_velocity;
        self.show_env_progress = s.env_progress;
        self.show_env_motion_blur = s.env_motion_blur;
        self.show_env_supersampling = s.env_supersampling;
        self.show_media_markers = s.media_markers;
        self.media_marker_labels = s.media_marker_labels;
        self.show_bus_tracks = s.bus_tracks;
        self.show_video_bus = s.video_bus;
        self.show_surround_keys = s.surround_keys;
        self.show_ofx_envelopes = s.ofx_envelopes;
        self.show_track_keys = s.track_keys;
    }
}

impl Default for TimelineView {
    fn default() -> TimelineView {
        TimelineView {
            show_take_info: false,
            show_clip_headers: true,
            buttons: crate::clip_windows::ClipButtons::default(),
            more_menu: None,
            view_start: Tick::ZERO,
            // 60 s em ~1000 px.
            ticks_per_px: 60.0 * TICKS_PER_SECOND as f64 / 1000.0,
            scroll_y: 0.0,
            show_thumbnails: true,
            show_waveforms: true,
            show_buttons: true,
            show_fade_durations: false,
            show_event_duration: false,
            show_envelopes: true,
            compact_tracks: false,
            show_unquantized: false,
            show_env_volume: true,
            show_env_pan: true,
            show_env_composite: true,
            show_env_fade: true,
            show_env_velocity: true,
            show_env_progress: true,
            show_env_mute: true,
            show_media_markers: true,
            media_marker_labels: false,
            show_bus_tracks: false,
            show_video_bus: false,
            show_env_motion_blur: true,
            show_env_supersampling: true,
            show_surround_keys: true,
            show_ofx_envelopes: false,
            show_track_keys: true,
            track_keys: Default::default(),
            ofx_drag: None,
            ofx_drags: 0,
            ofx_hover: None,
            ofx_menu: None,
            ofx_spot: None,
            ofx_hidden: Default::default(),
            ofx_set_to: None,
            env_set_to: None,
            ofx_painted: Vec::new(),
            ofx_areas: Vec::new(),
            wheel_sliders: Vec::new(),
            surround_drag: None,
            surround_pick: None,
            surround_menu: None,
            output_key_drag: None,
            output_key_menu: None,
            bus_area: bus_tracks::AREA,
            bus_heights: std::collections::HashMap::new(),
            bus_focus: false,
            bus_scroll: 0.0,
            bus_drag: None,
            bus_point: None,
            edge_trim: None,
            layers: Default::default(),
            layer_starts: Default::default(),
            drag: None,
            bar_drag: None,
            marker_tool: Default::default(),
            needle: None,
            gesture: None,
            snap_line: None,
            drop: None,
            new_track: None,
            context: lanes::ContextTarget::None,
            lanes_width: 1000.0,
            header_w: HEADER_W,
            full_header_w: HEADER_W,
            lanes_height: 400.0,
            renaming: None,
            renaming_group: None,
            value_edit: None,
            send_targets: std::collections::HashMap::new(),
            track_drag: None,
            track_heights: std::collections::HashMap::new(),
            track_restore: std::collections::HashMap::new(),
            rate_text: None,
            time_edit: None,
            snap_override: false,
            range_anchor: None,
            autoscroll: None,
            ignore_press: false,
            right_drag: false,
            box_click: false,
            box_base: None,
            event_click: None,
            drop_menu: None,
            media_drop_menu: None,
            wheel_rest: 0.0,
            last_rect: Rect::NOTHING,
        }
    }
}

impl TimelineView {
    /// The needle whose line is within reach of `x` in the tracks (the
    /// playback position's first, while playing).
    pub(crate) fn needle_line(&self, app: &BananaApp, l: &Layout, x: f32) -> Option<Needle> {
        const REACH: f32 = 4.0;
        if let Some(play) = app.playhead
            && (x - self.x_of(l, play)).abs() <= REACH
        {
            return Some(Needle::Playhead);
        }
        ((x - self.x_of(l, app.cursor)).abs() <= REACH).then_some(Needle::Cursor)
    }

    /// The needle whose head on the ruler is under `x`, while playing (two
    /// needles): the playback position's triangle or the edit cursor's
    /// flag.
    pub(crate) fn needle_head(&self, app: &BananaApp, l: &Layout, x: f32) -> Option<Needle> {
        let play = app.playhead?;
        if (x - self.x_of(l, play)).abs() <= 7.0 {
            return Some(Needle::Playhead);
        }
        let xc = self.x_of(l, app.cursor);
        (x >= xc - 3.0 && x <= xc + 11.0).then_some(Needle::Cursor)
    }

    pub(crate) fn x_of(&self, l: &Layout, t: Tick) -> f32 {
        l.lanes.left() + ((t - self.view_start).0 as f64 / self.ticks_per_px) as f32
    }

    pub(crate) fn time_at(&self, l: &Layout, x: f32) -> Tick {
        self.view_start + Tick(((x - l.lanes.left()) as f64 * self.ticks_per_px) as i64)
    }

    /// A drag at `x` near or past the tracks' left or right edge, as
    /// VEGAS's: the view runs on that way while it lasts — faster the
    /// further past the edge the pointer goes — and the drag goes on at
    /// the edge: `x` is brought back within the tracks, so what is dragged
    /// stays in sight.
    pub(crate) fn edge_scroll_x(&mut self, ui: &egui::Ui, l: &Layout, x: f32) -> f32 {
        let (left, right) = (l.lanes.left(), l.lanes.right());
        let v = edge_speed(x - (right - EDGE_ZONE)) - edge_speed(left + EDGE_ZONE - x);
        if v != 0.0 {
            let dt = ui.input(|i| i.stable_dt).min(0.1);
            let before = self.view_start;
            let step = Tick(((v * dt) as f64 * self.ticks_per_px) as i64);
            self.view_start = (before + step).max(Tick::ZERO);
            if self.view_start != before {
                ui.ctx().request_repaint();
            }
        }
        x.clamp(left, (right - 1.0).max(left))
    }

    /// The same up and down, near or past the tracks' top or bottom: the
    /// tracks scroll (events dragged to tracks out of sight, tracks
    /// reordered).
    pub(crate) fn edge_scroll_y(&mut self, ui: &egui::Ui, l: &Layout, y: f32) -> f32 {
        let (top, bottom) = (l.lanes.top(), l.lanes.bottom());
        let v = edge_speed(y - (bottom - EDGE_ZONE)) - edge_speed(top + EDGE_ZONE - y);
        if v != 0.0 {
            let dt = ui.input(|i| i.stable_dt).min(0.1);
            let most = (l.content_height - l.lanes.height() + 20.0).max(0.0);
            let before = self.scroll_y;
            self.scroll_y = (before + v * dt).clamp(0.0, most);
            if self.scroll_y != before {
                ui.ctx().request_repaint();
            }
        }
        y.clamp(top, (bottom - 1.0).max(top))
    }

    pub(crate) fn zoom_at(&mut self, anchor: Tick, anchor_px: f32, factor: f64) {
        self.ticks_per_px = (self.ticks_per_px * factor).clamp(MIN_TICKS_PER_PX, MAX_TICKS_PER_PX);
        self.view_start =
            (anchor - Tick((anchor_px as f64 * self.ticks_per_px) as i64)).max(Tick::ZERO);
    }

    /// Zooms by `factor` (below 1: in) toward `t`, as VEGAS zooms toward
    /// its needle: in view, `t` keeps its place on the screen; out of view,
    /// it comes to the middle.
    pub fn zoom_around(&mut self, factor: f64, t: Tick) {
        let px = ((t - self.view_start).0 as f64 / self.ticks_per_px) as f32;
        let px = if (0.0..=self.lanes_width).contains(&px) {
            px
        } else {
            self.lanes_width / 2.0
        };
        self.zoom_at(t, px, factor);
    }

    /// Frames between the major ruler marks at the current zoom (the "grid
    /// marks" of Edit › Navigate).
    pub(crate) fn grid_step(&self, rate: bv_core::FrameRate) -> i64 {
        bars::ruler_step(self, rate).0.max(1)
    }

    /// Mostra do início até `end` (com folga).
    pub fn fit(&mut self, end: Tick) {
        let end = end.max(Tick::from_seconds(10));
        self.ticks_per_px = (end.0 as f64 * 1.05 / self.lanes_width.max(100.0) as f64)
            .clamp(MIN_TICKS_PER_PX, MAX_TICKS_PER_PX);
        self.view_start = Tick::ZERO;
    }

    /// Moves `t` onto the nearest snap point (within a few pixels) or,
    /// with quantizing, onto the nearest frame. `offsets` are the edges of
    /// what moves, relative to `t`.
    pub(crate) fn snap(
        &mut self,
        app: &BananaApp,
        project: &Project,
        t: Tick,
        offsets: &[Tick],
        exclude: &[bv_timeline::ClipId],
    ) -> Tick {
        self.snap_with(app, project, t, offsets, exclude, &[])
    }

    /// Where the edit cursor goes when placed at `t` (a click or a drag on
    /// the ruler or in the timeline): with snapping, onto the nearest event
    /// edge, marker, region, time selection edge or the project's end; on a
    /// frame otherwise.
    pub(crate) fn snap_cursor(&mut self, app: &BananaApp, project: &Project, t: Tick) -> Tick {
        let end = [(project.duration(), SnapKind::Event)];
        let snapped = self.snap_points(app, project, t, &[Tick::ZERO], &[], &end, false);
        if self.snap_line.is_some() {
            // Exactly on the edge it snapped to.
            snapped
        } else {
            project.settings.frame_rate.snap_floor(t)
        }
    }

    /// [`snap`](Self::snap) with `extra` snap points too (an event's media
    /// edges while its edge is dragged, where a dragged event was).
    pub(crate) fn snap_with(
        &mut self,
        app: &BananaApp,
        project: &Project,
        t: Tick,
        offsets: &[Tick],
        exclude: &[bv_timeline::ClipId],
        extra: &[(Tick, SnapKind)],
    ) -> Tick {
        self.snap_points(app, project, t, offsets, exclude, extra, true)
    }

    /// The snap itself; `to_cursor`: the edit cursor is a snap point too
    /// (not when it is the cursor that moves). The line shown where it
    /// snapped takes the color of what it snapped to (Options › Preferences
    /// › Display › Snap Colors).
    #[allow(clippy::too_many_arguments)]
    fn snap_points(
        &mut self,
        app: &BananaApp,
        project: &Project,
        t: Tick,
        offsets: &[Tick],
        exclude: &[bv_timeline::ClipId],
        extra: &[(Tick, SnapKind)],
        to_cursor: bool,
    ) -> Tick {
        // How near a point draws what moves, as VEGAS's: 7 pixels.
        const SNAP_PX: f64 = 7.0;
        self.snap_line = None;
        // Shift: off when it is on, on when it is off.
        if app.snapping != self.snap_override {
            let threshold = Tick((SNAP_PX * self.ticks_per_px) as i64);
            let mut points: Vec<(Tick, SnapKind)> = Vec::new();
            if app.snap_events {
                for (_, c) in project.clips().filter(|(_, c)| !exclude.contains(&c.id)) {
                    points.push((c.start, SnapKind::Event));
                    points.push((c.end(), SnapKind::Event));
                    // Where its fades end.
                    if c.fade_in > Tick::ZERO {
                        points.push((c.start + c.fade_in, SnapKind::EventFade));
                    }
                    if c.fade_out > Tick::ZERO {
                        points.push((c.end() - c.fade_out, SnapKind::EventFade));
                    }
                }
            }
            if app.snap_markers {
                points.extend(project.markers.iter().map(|m| (m.time, SnapKind::Marker)));
                for r in &project.regions {
                    points.push((r.range.start, SnapKind::Region));
                    points.push((r.range.end(), SnapKind::Region));
                }
                if let Some(r) = project.loop_region {
                    points.push((r.start, SnapKind::Loop));
                    points.push((r.end(), SnapKind::Loop));
                }
                // The media markers the events show (View › Event Media
                // Markers).
                if self.show_media_markers {
                    for (_, c) in project.clips().filter(|(_, c)| !exclude.contains(&c.id)) {
                        let Some(m) = project.media(c.media) else {
                            continue;
                        };
                        for mk in &m.markers {
                            let kind = if mk.is_region() {
                                SnapKind::Region
                            } else {
                                SnapKind::Marker
                            };
                            let mut at = c.media_points(mk.time, m.duration);
                            if mk.is_region() {
                                at.extend(c.media_points(mk.end(), m.duration));
                            }
                            points.extend(at.into_iter().map(|t| (t, kind)));
                        }
                    }
                }
            }
            // The points of the envelopes shown.
            for track in &project.tracks {
                for env in track
                    .envelopes
                    .iter()
                    .filter(|e| envelopes::visible(self, e.kind))
                {
                    points.extend(env.points.iter().map(|p| (p.at, SnapKind::EnvelopePoint)));
                }
            }
            // Nearest grid line for each offset (Options › Snap to Grid).
            if app.snap_grid {
                let step = app.grid_ticks(self).0.max(1);
                for &off in offsets {
                    points.push((app.grid_line_near(t + off, step), SnapKind::Grid));
                }
            }
            points.extend_from_slice(extra);
            points.push((Tick::ZERO, SnapKind::Grid));
            if to_cursor {
                points.push((app.cursor, SnapKind::Cursor));
            }
            let mut best: Option<(Tick, Tick, SnapKind)> = None;
            for &(p, kind) in &points {
                for &off in offsets {
                    let d = p - (t + off);
                    if d.abs() <= threshold && best.is_none_or(|(bd, _, _)| d.abs() < bd.abs()) {
                        best = Some((d, p, kind));
                    }
                }
            }
            if let Some((d, p, kind)) = best {
                self.snap_line = Some((p, kind));
                return t + d;
            }
        }
        // Audio events on their own are not held to frames (Options ›
        // Preferences › Editing: "Do not quantize to frames for audio-only
        // edits"): `exclude` holds the events being edited.
        let audio_only = app.settings.prefs.no_quantize_audio
            && !exclude.is_empty()
            && exclude.iter().all(|&id| {
                project
                    .clip_track(id)
                    .is_some_and(|tr| tr.kind == TrackKind::Audio)
            });
        if app.quantize && !audio_only {
            project.settings.frame_rate.snap_nearest(t)
        } else {
            t
        }
    }

    fn layout(&self, rect: Rect, project: &Project) -> Layout {
        let top = rect.top();
        let left = rect.left();
        let right = rect.right();
        let bottom_tool = rect.bottom() - TOOLBAR_H;
        let scroll_top = bottom_tool - SCROLL_H;
        let list_right = left + self.header_w;
        let lanes_left = list_right + SPLIT_W;
        let cd_h = if project.cd_tracks.is_empty() {
            0.0
        } else {
            marker_bars::BAR_H
        };
        let command_h = if project.commands.is_empty() {
            0.0
        } else {
            marker_bars::BAR_H
        };
        let top_h = TOP_H + cd_h + command_h;
        let tc_box = Rect::from_min_max(rect.min, pos2(list_right, top + top_h));
        let loop_bar =
            Rect::from_min_max(pos2(lanes_left, top), pos2(right - VSCROLL_W, top + LOOP_H));
        let ruler = Rect::from_min_max(
            pos2(lanes_left, top + LOOP_H),
            pos2(right - VSCROLL_W, top + LOOP_H + RULER_H),
        );
        let marker_bar = Rect::from_min_max(
            pos2(lanes_left, top + LOOP_H + RULER_H),
            pos2(right - VSCROLL_W, top + TOP_H),
        );
        let cd_bar = Rect::from_min_max(
            pos2(lanes_left, top + TOP_H),
            pos2(right - VSCROLL_W, top + TOP_H + cd_h),
        );
        let command_bar = Rect::from_min_max(
            pos2(lanes_left, cd_bar.bottom()),
            pos2(right - VSCROLL_W, cd_bar.bottom() + command_h),
        );
        let marker_tool = Rect::from_min_max(
            pos2(right - VSCROLL_W, top + LOOP_H + RULER_H),
            pos2(right, top + TOP_H),
        );
        let headers = Rect::from_min_max(pos2(left, top + top_h), pos2(list_right, scroll_top));
        let lanes = Rect::from_min_max(
            pos2(lanes_left, top + top_h),
            pos2(right - VSCROLL_W, scroll_top),
        );
        let vscroll = Rect::from_min_max(
            pos2(right - VSCROLL_W, top + top_h),
            pos2(right, scroll_top),
        );
        // As VEGAS 23's: the track list goes down to the bottom, the Rate
        // there as tall as the scroll bar and the toolbar beside it.
        let rate = Rect::from_min_max(pos2(left, scroll_top), pos2(list_right, rect.bottom()));
        let hscroll = Rect::from_min_max(pos2(lanes_left, scroll_top), pos2(right, bottom_tool));
        let toolbar = Rect::from_min_max(pos2(lanes_left, bottom_tool), rect.max);
        // View › Video Bus Track and Audio Bus Tracks: at the bottom, below
        // a splitter, as tall as it was dragged (the tracks keep some
        // room); the video bus first.
        let mut strips = Vec::new();
        if self.show_video_bus {
            strips.push(bus_tracks::BusTrack::Video);
        }
        if self.show_bus_tracks {
            strips.extend(
                project
                    .strips()
                    .into_iter()
                    .map(bus_tracks::BusTrack::Strip),
            );
        }
        let room = (scroll_top - lanes.top() - 40.0).max(0.0);
        let area = if strips.is_empty() {
            0.0
        } else {
            self.bus_area
                .clamp(bus_tracks::MIN_AREA, room.max(bus_tracks::MIN_AREA))
                .min(room)
        };
        let tracks_bottom = scroll_top - area;
        let split_bottom = (tracks_bottom + bus_tracks::SPLIT_H).min(scroll_top);
        let (headers, lanes, vscroll) = if area > 0.0 {
            (
                Rect::from_min_max(headers.min, pos2(headers.right(), tracks_bottom)),
                Rect::from_min_max(lanes.min, pos2(lanes.right(), tracks_bottom)),
                Rect::from_min_max(vscroll.min, pos2(vscroll.right(), tracks_bottom)),
            )
        } else {
            (headers, lanes, vscroll)
        };
        let bus_split = Rect::from_min_max(pos2(left, tracks_bottom), pos2(right, split_bottom));
        let bus_headers =
            Rect::from_min_max(pos2(left, split_bottom), pos2(list_right, scroll_top));
        let bus_lanes = Rect::from_min_max(
            pos2(lanes_left, split_bottom),
            pos2(right - VSCROLL_W, scroll_top),
        );
        let mut bus_rows = Vec::new();
        let mut by = bus_lanes.top() - self.bus_scroll;
        for track in strips {
            let keys = match track {
                bus_tracks::BusTrack::Strip(s) => {
                    surround_keys::height(self, project, bv_timeline::SurroundOwner::Strip(s))
                }
                bus_tracks::BusTrack::Video => output_keys::height(self, project),
            };
            let h = bus_tracks::height_of(self, track) + keys;
            bus_rows.push(bus_tracks::BusRow {
                track,
                top: by,
                height: h,
                keys,
            });
            by += h;
        }
        let bus_content = by + self.bus_scroll - bus_lanes.top();
        let mut y = lanes.top() - self.scroll_y;
        let mut rows = Vec::new();
        let mut group_rows: Vec<groups::GroupRow> = Vec::new();
        let mut current = None;
        for t in &project.tracks {
            // A group's header row over its first track; collapsed, its
            // tracks are not shown.
            if t.group != current {
                current = t.group;
                if let Some(g) = t.group.and_then(|g| project.track_group(g)) {
                    group_rows.push(groups::GroupRow {
                        group: g.id,
                        top: y,
                        bottom: y + groups::GROUP_H,
                        collapsed: g.collapsed,
                    });
                    y += groups::GROUP_H;
                }
            }
            let open = group_rows.last_mut().filter(|g| Some(g.group) == t.group);
            if open.as_ref().is_some_and(|g| g.collapsed) {
                continue;
            }
            // An audio track's Surround Pan Keyframes, a video track's
            // keyframes area.
            let keys =
                surround_keys::height(self, project, bv_timeline::SurroundOwner::Track(t.id))
                    + ofx_envelopes::height(self, t);
            let h = self.height_of(t) + keys;
            rows.push(Row {
                track: t.id,
                kind: t.kind,
                top: y,
                height: h,
                keys,
                group: t.group,
            });
            y += h;
            if let Some(g) = open {
                g.bottom = y;
            }
        }
        Layout {
            tc_box,
            bus_split,
            bus_headers,
            bus_lanes,
            bus_rows,
            bus_content,
            loop_bar,
            ruler,
            marker_bar,
            cd_bar,
            command_bar,
            marker_tool,
            headers,
            lanes,
            vscroll,
            rate,
            hscroll,
            toolbar,
            content_height: y + self.scroll_y - lanes.top(),
            rows,
            groups: group_rows,
        }
    }

    /// A press on the track headers gives the keys to the track list
    /// (VEGAS's TrackList context) and selects that track; a press in the
    /// timeline gives them back to the track view and selects the track
    /// under the pointer.
    fn track_focus(&mut self, ui: &egui::Ui, app: &mut BananaApp, l: &Layout) {
        let Some(p) = ui.input(|i| {
            i.pointer
                .any_pressed()
                .then(|| i.pointer.interact_pos())
                .flatten()
        }) else {
            return;
        };
        // Not when the press lands on a menu or window over the timeline.
        if ui.ctx().layer_id_at(p) != Some(ui.layer_id()) {
            return;
        }
        // The Marker Tool stays on while the marker bars are used.
        if ![l.marker_bar, l.cd_bar, l.command_bar, l.marker_tool]
            .iter()
            .any(|r| r.contains(p))
        {
            self.marker_tool.active = false;
        }
        let track = l.row_at(p.y).map(|r| r.track);
        if l.headers.contains(p) {
            app.key_focus = Context::TrackList;
        } else if l.lanes.contains(p) {
            app.key_focus = Context::TrackView;
        } else {
            if [
                l.ruler,
                l.loop_bar,
                l.marker_bar,
                l.cd_bar,
                l.command_bar,
                l.toolbar,
            ]
            .iter()
            .any(|r| r.contains(p))
            {
                app.key_focus = Context::TrackView;
            }
            return;
        }
        // A right-click on one of the tracks selected keeps them: its menu
        // acts on them all, as VEGAS's.
        let secondary = ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary));
        if secondary && track.is_some_and(|t| app.track_selection().contains(&t)) {
            return;
        }
        // In the track list Shift and Ctrl pick several tracks.
        if let Some(t) = track {
            if l.headers.contains(p) {
                let m = ui.input(|i| i.modifiers);
                app.pick_track(t, m.shift, m.ctrl || m.command);
            } else {
                app.pick_track(t, false, false);
            }
        }
    }

    /// Height of a track: its own (Minimize / Maximize) or the default.
    pub(crate) fn height_of(&self, t: &bv_timeline::Track) -> f32 {
        let h = self
            .track_heights
            .get(&t.id)
            .copied()
            .unwrap_or_else(|| track_height(t.kind, self.compact_tracks));
        // Expanded Edit Mode: room for the A and B rolls; Expand Track
        // Layers: for them and the transition roll.
        match self.edge_trim {
            Some(e) if e.expanded && e.track == t.id => {
                (h.max(track_height(t.kind, false)) * 2.0).min(MAX_TRACK_H)
            }
            _ if self.layers.contains(&t.id) => {
                (h.max(track_height(t.kind, false)) * 2.0).min(MAX_TRACK_H)
            }
            _ => h,
        }
    }

    /// Expanded Edit Mode: whether an event of `track` starting at `start`
    /// is on the B roll (`Some(true)`), the A roll, or the track is not
    /// expanded (`None`).
    pub(crate) fn roll(&self, track: TrackId, start: Tick) -> Option<bool> {
        if let Some(e) = self.edge_trim.filter(|e| e.expanded && e.track == track) {
            return Some(start >= e.pivot);
        }
        // Expand Track Layers: the events alternate between the A and the
        // B roll.
        self.layer_starts
            .get(&track)
            .and_then(|starts| starts.iter().position(|s| *s == start))
            .map(|i| i % 2 == 1)
    }

    /// Where an event starting at `start` is drawn in `track`'s row: the
    /// share of the row's body above it and its own (the whole row, or
    /// Expanded Edit Mode's halves, or Expand Track Layers' A and B rolls,
    /// the transition roll between them).
    pub(crate) fn band(&self, track: TrackId, start: Tick) -> (f32, f32) {
        let layers = self.layer_starts.contains_key(&track)
            && !self
                .edge_trim
                .is_some_and(|e| e.expanded && e.track == track);
        match (self.roll(track, start), layers) {
            (None, _) => (0.0, 1.0),
            (Some(b), true) => (if b { 2.0 / 3.0 } else { 0.0 }, 1.0 / 3.0),
            (Some(b), false) => (if b { 0.5 } else { 0.0 }, 0.5),
        }
    }

    /// Expand Track Layers on `track`, or off.
    pub(crate) fn toggle_layers(&mut self, track: TrackId) {
        if !self.layers.remove(&track) {
            self.layers.insert(track);
        }
    }

    /// The expanded video tracks' events' starts, in order, for this frame.
    pub(crate) fn follow_layers(&mut self, project: &Project) {
        self.layers.retain(|t| {
            project
                .track(*t)
                .is_some_and(|t| t.kind == bv_timeline::TrackKind::Video)
        });
        self.layer_starts = self
            .layers
            .iter()
            .filter_map(|t| project.track(*t))
            .map(|t| {
                let mut starts: Vec<Tick> = t.clips.iter().map(|c| c.start).collect();
                starts.sort();
                (t.id, starts)
            })
            .collect();
    }

    /// Puts the red bracket on an edge of event `clip` (`end`: its end);
    /// `expanded`: in Expanded Edit Mode.
    pub(crate) fn trim_edge(
        &mut self,
        project: &Project,
        clip: bv_timeline::ClipId,
        end: bool,
        expanded: bool,
    ) {
        let Some(track) = project.clip_track(clip).map(|t| t.id) else {
            self.edge_trim = None;
            return;
        };
        self.edge_trim = Some(EdgeTrim {
            track,
            clip,
            end,
            expanded,
            pivot: Tick::ZERO,
        });
        self.follow_edge_trim(project);
    }

    /// Keeps the edit point with its event as the project changes (it
    /// ends when the event is gone).
    pub(crate) fn follow_edge_trim(&mut self, project: &Project) {
        let Some(e) = &mut self.edge_trim else {
            return;
        };
        let (Some(c), Some(track)) = (project.clip(e.clip), project.clip_track(e.clip)) else {
            self.edge_trim = None;
            return;
        };
        e.track = track.id;
        // The event itself is on the A roll when its end is the edit point,
        // on the B roll when its start is.
        e.pivot = if e.end { c.start + Tick(1) } else { c.start };
    }

    /// Sets a track's height (dragging the bottom edge of its header).
    pub(crate) fn set_track_height(&mut self, id: TrackId, height: f32) {
        self.track_heights
            .insert(id, height.clamp(MIN_TRACK_H, MAX_TRACK_H));
    }

    /// Header's Minimize: the track as short as it gets; again, back to the
    /// height it had.
    pub(crate) fn minimize_track(&mut self, id: TrackId) {
        if self.track_heights.get(&id) == Some(&MIN_TRACK_H) {
            self.restore_track(id);
        } else {
            self.remember_height(id);
            self.track_heights.insert(id, MIN_TRACK_H);
        }
    }

    /// Keeps the track's height before Minimize or Maximize (a height of
    /// its own; none: the default).
    fn remember_height(&mut self, id: TrackId) {
        let full = self.lanes_height.max(MIN_TRACK_H);
        match self.track_heights.get(&id) {
            Some(&h) if h != MIN_TRACK_H && h != full => {
                self.track_restore.insert(id, h);
            }
            // Minimized or maximized already: the height before stays.
            Some(_) => {}
            None => {
                self.track_restore.remove(&id);
            }
        }
    }

    /// Restore Track Height: the height the track had before Minimize or
    /// Maximize.
    pub(crate) fn restore_track(&mut self, id: TrackId) {
        match self.track_restore.remove(&id) {
            Some(h) => {
                self.track_heights.insert(id, h);
            }
            None => {
                self.track_heights.remove(&id);
            }
        }
    }

    /// Ctrl+`: every track at the default height.
    pub(crate) fn default_track_heights(&mut self) {
        self.compact_tracks = false;
        self.track_heights.clear();
        self.track_restore.clear();
    }

    /// Header's Maximize: the track fills the visible lanes; again, back to
    /// the height it had.
    pub(crate) fn maximize_track(&mut self, project: &Project, id: TrackId) {
        let full = self.lanes_height.max(MIN_TRACK_H);
        if self.track_heights.get(&id) == Some(&full) {
            self.restore_track(id);
        } else {
            self.remember_height(id);
            self.track_heights.insert(id, full);
            self.reveal_track(project, id);
        }
    }

    /// How many tracks fit in the visible lanes (Page Up/Down in the track
    /// list).
    pub(crate) fn tracks_per_page(&self) -> usize {
        (self.lanes_height / track_height(TrackKind::Audio, self.compact_tracks)) as usize
    }

    /// Scrolls vertically so that track `id` is fully visible (not when
    /// its group is collapsed).
    pub(crate) fn reveal_track(&mut self, project: &Project, id: TrackId) {
        let mut top = 0.0;
        let mut current = None;
        for t in &project.tracks {
            let group = t.group.and_then(|g| project.track_group(g));
            if t.group != current {
                current = t.group;
                if group.is_some() {
                    top += groups::GROUP_H;
                }
            }
            if group.is_some_and(|g| g.collapsed) {
                if t.id == id {
                    return;
                }
                continue;
            }
            let h = self.height_of(t);
            if t.id == id {
                if top < self.scroll_y {
                    self.scroll_y = top;
                } else if top + h > self.scroll_y + self.lanes_height {
                    self.scroll_y = (top + h - self.lanes_height).max(0.0);
                }
                return;
            }
            top += h;
        }
    }

    fn follow_playhead(&mut self, app: &BananaApp, l: &Layout) {
        if !app.is_playing() || self.drag.is_some() {
            return;
        }
        let t = app.display_time();
        let x = self.x_of(l, t);
        if x > l.lanes.right() - 10.0 || x < l.lanes.left() {
            let width = Tick((l.lanes.width() as f64 * self.ticks_per_px) as i64);
            self.view_start = (t - Tick(width.0 / 10)).max(Tick::ZERO);
        }
    }

    pub(crate) fn ui(&mut self, ui: &mut egui::Ui, app: &mut BananaApp) {
        let rect = ui.available_rect_before_wrap();
        self.last_rect = rect;
        let w = app.settings.track_list_width;
        self.header_w = if w > 0.0 {
            w.clamp(
                MIN_HEADER_W,
                MAX_HEADER_W.min(rect.width() - 120.0).max(MIN_HEADER_W),
            )
        } else {
            HEADER_W
        };
        self.lanes_width = (rect.width() - self.header_w - SPLIT_W - VSCROLL_W).max(50.0);
        let project = app.project().clone();
        if std::mem::take(&mut app.fit_timeline) {
            self.fit(project.duration());
        }
        self.follow_edge_trim(&project);
        self.follow_layers(&project);
        let l = self.layout(rect, &project);
        self.lanes_height = l.lanes.height();
        self.snap_override = ui.input(|i| i.modifiers.shift);
        ui.painter().rect_filled(rect, 0.0, theme::bg());
        self.follow_playhead(app, &l);
        self.track_focus(ui, app, &l);

        // Interações (podem editar o projeto).
        bars::interact(self, ui, app, &l);
        marker_bars::interact(self, ui, app, &l);
        marker_tool::button(self, ui, &l);
        ofx_envelopes::interact(self, ui, app, &l);
        lanes::interact(self, ui, app, &l);
        // The effects' keyframes and envelopes take the press before the
        // tracks under them: their pointer too.
        ofx_envelopes::show_pointer(self, ui);
        toolbar::scrollbars(self, ui, app, &l);

        // Desenha o estado já atualizado.
        let project = app.project().clone();
        self.follow_layers(&project);
        let l = self.layout(rect, &project);
        let prerendered = app.engine.prerendered().to_vec();
        lanes::paint(self, ui, app, &l, &project);
        ofx_envelopes::paint(self, ui, app, &l, &project);
        headers::ui(self, ui, app, &l, &project);
        ofx_envelopes::headers_ui(self, ui, &l, &project);
        groups::ui(self, ui, app, &l, &project);
        headers::drag_ui(self, ui, app, &l);
        self.list_border(ui, app, &l);
        bus_tracks::ui(self, ui, app, &l, &project);
        surround_keys::ui(self, ui, app, &l, &project);
        output_keys::ui(self, ui, app, &l, &project);
        bars::paint(self, ui, app, &l, &project, &prerendered);
        marker_bars::paint(self, ui, app, &l, &project);
        toolbar::rate(self, ui, app, &l);
        toolbar::ui(self, ui, app, &l);
        self.paint_cursor(ui, app, &l);
        self.more_menu_ui(ui, app);
        ui.allocate_rect(rect, egui::Sense::hover());
    }

    /// The lanes' left edge and the headers' width the layout gives (tests).
    #[cfg(test)]
    pub(crate) fn layout_for_test(&self, rect: Rect, project: &Project) -> (f32, f32) {
        let l = self.layout(rect, project);
        (l.lanes.left(), l.headers.width())
    }

    /// The splitter between the track list and the lanes, dark from the
    /// timeline's top to its bottom as VEGAS's: dragged, the list's width
    /// (its buttons give way as it narrows); double-clicked, the full width
    /// or only the color bars, in turn. Its pointer is the splitter's ↔.
    fn list_border(&mut self, ui: &egui::Ui, app: &mut BananaApp, l: &Layout) {
        let edge = Rect::from_min_max(
            pos2(l.headers.right(), l.tc_box.top()),
            pos2(l.lanes.left(), l.rate.bottom()),
        );
        ui.painter().rect_filled(edge, 0.0, theme::panel_dark());
        let resp = ui.interact(
            edge,
            egui::Id::new("track-list-border"),
            egui::Sense::click_and_drag(),
        );
        if resp.hovered() || resp.dragged() {
            cursors::set(ui.ctx(), Cursor::SplitterEw);
        }
        let left = l.headers.left();
        if resp.dragged()
            && let Some(p) = resp.interact_pointer_pos()
        {
            app.settings.track_list_width =
                (p.x - left - SPLIT_W / 2.0).clamp(MIN_HEADER_W, MAX_HEADER_W);
        }
        if resp.double_clicked() {
            let now = self.header_w;
            app.settings.track_list_width = if now > MIN_HEADER_W + 30.0 {
                self.full_header_w = now;
                MIN_HEADER_W
            } else {
                self.full_header_w.max(HEADER_W.min(MAX_HEADER_W))
            };
        }
        resp.on_hover_text(tr(
            "Drag to change the track list's width; double-click: the full width or only the color bars",
        ));
    }

    /// Mostra o menu "⋯" do clipe, se estiver aberto.
    fn more_menu_ui(&mut self, ui: &egui::Ui, app: &mut BananaApp) {
        let Some(menu) = self.more_menu else { return };
        // O menu edita o estado da timeline (cabeçalhos, informações): usa a
        // cópia de `self` que está fora do app durante este quadro.
        std::mem::swap(&mut app.timeline, self);
        let area = egui::Area::new(egui::Id::new("menu-extras-clipe"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu.at)
            .show(ui.ctx(), |ui| {
                egui::Frame::menu(ui.style())
                    .show(ui, |ui| {
                        crate::clip_windows::extras_menu(ui, app, menu.clip)
                    })
                    .inner
            });
        std::mem::swap(&mut app.timeline, self);
        let clicked_outside = ui.input(|i| i.pointer.any_pressed())
            && ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| !area.response.rect.contains(p));
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if area.inner || escape || (clicked_outside && !menu.fresh) {
            self.more_menu = None;
        } else if let Some(m) = &mut self.more_menu {
            m.fresh = false;
        }
    }

    /// The edit cursor and, while playing, the playback position: two
    /// needles, like VEGAS. The edit cursor stays where it was clicked.
    fn paint_cursor(&self, ui: &egui::Ui, app: &BananaApp, l: &Layout) {
        let painter = ui.painter();
        let top = l.ruler.top();
        let visible = |x: f32| x >= l.lanes.left() && x <= l.lanes.right();
        if let Some(play) = app.playhead {
            // Edit cursor during playback: yellow line with a flag on the ruler.
            let x = self.x_of(l, app.cursor);
            if visible(x) {
                painter.line_segment(
                    [pos2(x, top), pos2(x, l.lanes.bottom())],
                    egui::Stroke::new(1.5, theme::banana()),
                );
                painter.add(egui::Shape::convex_polygon(
                    vec![pos2(x, top), pos2(x + 9.0, top + 4.0), pos2(x, top + 8.0)],
                    theme::banana(),
                    egui::Stroke::NONE,
                ));
            }
            let x = self.x_of(l, play);
            if visible(x) {
                paint_needle_head(painter, l, x, theme::cursor());
            }
            return;
        }
        let x = self.x_of(l, app.cursor);
        if !visible(x) {
            return;
        }
        // Options › Preferences › Editing: "Change cursor and selected
        // event color when in Auto Ripple mode".
        let color = if app.auto_ripple && app.settings.prefs.ripple_colors {
            theme::ripple()
        } else {
            theme::cursor()
        };
        paint_needle_head(painter, l, x, color);
    }
}

/// The width of a needle's head on the ruler.
const NEEDLE_HEAD_W: f32 = 13.0;

/// A needle as VEGAS draws the cursor: a badge on the ruler — square on
/// top, pointed below, where the line goes down through the tracks.
fn paint_needle_head(painter: &egui::Painter, l: &Layout, x: f32, color: egui::Color32) {
    let (top, bottom) = (l.ruler.top() + 2.0, l.ruler.bottom());
    painter.line_segment(
        [pos2(x, bottom), pos2(x, l.lanes.bottom())],
        egui::Stroke::new(1.0, color),
    );
    let half = NEEDLE_HEAD_W / 2.0;
    let shoulder = bottom - 5.0;
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(x - half, top),
            pos2(x + half, top),
            pos2(x + half, shoulder),
            pos2(x, bottom),
            pos2(x - half, shoulder),
        ],
        color,
        egui::Stroke::new(1.0, egui::Color32::from_black_alpha(140)),
    ));
}

impl BananaApp {
    /// Where `seconds` is, in the middle of the row of the project's track
    /// `track` (the timeline as last drawn), for screen tests.
    pub fn timeline_spot_for_review(&self, track: usize, seconds: f64) -> Option<Pos2> {
        let view = &self.timeline;
        let project = self.project();
        let l = view.layout(view.last_rect, project);
        let row = l.row_of(project.tracks.get(track)?.id)?;
        Some(pos2(
            view.x_of(&l, Tick::from_seconds_f64(seconds)),
            row.top + row.height * 0.6,
        ))
    }

    /// Where `seconds` is in the space below the last track (the timeline
    /// as last drawn), for screen tests; `None` without room there.
    pub fn below_tracks_spot_for_review(&self, seconds: f64) -> Option<Pos2> {
        let view = &self.timeline;
        let l = view.layout(view.last_rect, self.project());
        let bottom = l
            .rows
            .iter()
            .map(|r| r.top + r.height)
            .fold(l.lanes.top(), f32::max);
        let y = bottom + 12.0;
        (y < l.lanes.bottom() - 2.0)
            .then(|| pos2(view.x_of(&l, Tick::from_seconds_f64(seconds)), y))
    }

    /// Where `seconds` is on the ruler (the timeline as last drawn), for
    /// screen tests.
    pub fn ruler_spot_for_review(&self, seconds: f64) -> Pos2 {
        let view = &self.timeline;
        let l = view.layout(view.last_rect, self.project());
        pos2(
            view.x_of(&l, Tick::from_seconds_f64(seconds)),
            l.ruler.center().y,
        )
    }

    /// Two media of the project let go with the right mouse button at
    /// `seconds` on the first track: the menu that asks how to add them
    /// (screen tests).
    pub fn media_drop_menu_for_review(&mut self, seconds: f64) {
        let at = self
            .timeline_spot_for_review(0, seconds)
            .unwrap_or_default();
        let media: Vec<bv_timeline::MediaId> =
            self.project().media.iter().take(2).map(|m| m.id).collect();
        self.timeline.media_drop_menu = Some(lanes::MediaDropMenu::for_review(
            at,
            media,
            Tick::from_seconds_f64(seconds),
        ));
    }

    /// A spot of the marker bar ("markers"), the CD layout bar ("cd") or
    /// the command bar ("commands") at `seconds`, or the Marker Tool's
    /// button ("tool"), for screen tests.
    pub fn bar_spot_for_review(&self, bar: &str, seconds: f64) -> Pos2 {
        let view = &self.timeline;
        let l = view.layout(view.last_rect, self.project());
        let x = view.x_of(&l, Tick::from_seconds_f64(seconds)) + 2.0;
        match bar {
            "cd" => pos2(x, l.cd_bar.center().y),
            "commands" => pos2(x, l.command_bar.center().y),
            "tool" => l.marker_tool.center(),
            _ => pos2(x, l.marker_bar.center().y),
        }
    }

    /// A spot of the project's track `track`'s header with nothing on it
    /// (its colored strip, near the bottom), for screen tests.
    pub fn track_header_spot_for_review(&self, track: usize) -> Option<Pos2> {
        let view = &self.timeline;
        let project = self.project();
        let l = view.layout(view.last_rect, project);
        let row = l.row_of(project.tracks.get(track)?.id)?;
        Some(pos2(l.headers.left() + 6.0, row.top + row.height - 8.0))
    }

    /// Gives the `event`th event of the project's track `track` fades of
    /// `fade_in` and `fade_out` seconds and the level `level`, for screen
    /// tests.
    pub fn fades_for_review(
        &mut self,
        track: usize,
        event: usize,
        fade_in: f64,
        fade_out: f64,
        level: f32,
    ) {
        let Some(id) = self
            .project()
            .tracks
            .get(track)
            .and_then(|t| t.clips.get(event))
            .map(|c| c.id)
        else {
            return;
        };
        self.edit(|p| -> Result<(), bv_timeline::EditError> {
            p.set_fades(
                id,
                Some(Tick::from_seconds_f64(fade_in)),
                Some(Tick::from_seconds_f64(fade_out)),
            )?;
            p.set_clip_level(id, level)
        });
    }

    /// Where the fade handle of the `event`th event of the project's track
    /// `track` is (its fade-out's with `out`), the timeline as last drawn,
    /// for screen tests.
    pub fn fade_handle_spot_for_review(
        &self,
        track: usize,
        event: usize,
        out: bool,
    ) -> Option<Pos2> {
        let view = &self.timeline;
        let project = self.project();
        let l = view.layout(view.last_rect, project);
        let t = project.tracks.get(track)?;
        let c = t.clips.get(event)?;
        let row = l.row_of(t.id)?;
        let r = lanes::clip_rect(view, &l, row, c.start, c.end());
        let x = if out {
            view.x_of(&l, c.end() - c.fade_out) - 2.0
        } else {
            view.x_of(&l, c.start + c.fade_in) + 2.0
        };
        Some(pos2(x, lanes::level_y(r, c.level)))
    }

    /// The project's tracks, for screen tests: whether each is a video
    /// track, and how many events it has.
    pub fn tracks_for_review(&self) -> Vec<(bool, usize)> {
        self.project()
            .tracks
            .iter()
            .map(|t| (t.kind == bv_timeline::TrackKind::Video, t.clips.len()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui::{Event, Modifiers, PointerButton, vec2};
    use egui_kittest::Harness;

    /// BananaV's pointer the last frame asked for (`None`: the system's).
    fn shown(h: &Harness<'_, BananaApp>) -> Option<Cursor> {
        let img = h.output().platform_output.cursor_image.clone()?;
        Cursor::ALL
            .into_iter()
            .find(|&c| std::sync::Arc::ptr_eq(&img.rgba, &cursors::image(c, img.size[0]).rgba))
    }

    /// The pointer over `at`.
    fn over(h: &mut Harness<'_, BananaApp>, at: Pos2) -> Option<Cursor> {
        h.event(Event::PointerMoved(at));
        h.run_steps(2);
        shown(h)
    }

    fn button(h: &mut Harness<'_, BananaApp>, at: Pos2, pressed: bool) {
        h.event(Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        });
        h.step();
    }

    /// A drag from `from` by `by`, a few steps at a time: the pointer on
    /// the way, at its end.
    fn drag(h: &mut Harness<'_, BananaApp>, from: Pos2, by: egui::Vec2) -> Option<Cursor> {
        over(h, from);
        button(h, from, true);
        let mut at = from;
        for _ in 0..6 {
            at += by / 6.0;
            h.event(Event::PointerMoved(at));
            h.step();
        }
        let during = shown(h);
        button(h, at, false);
        h.run_steps(2);
        during
    }

    /// The timeline's zones show VEGAS's pointers, drawn by BananaV: the
    /// splitters' (the track list's border, a track's and a bus track's
    /// bottom edges, the bus tracks' top), ↔ on the loop region's edges, the
    /// arrow with ↔ on what a drag moves along the bars (kept while it is
    /// dragged anywhere), the arrow on the ruler.
    #[test]
    fn the_zones_point_as_vegas_does() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        app.load_demo();
        let s = Tick::from_seconds;
        let marker = app.modify(|p| {
            p.add_bus();
            p.add_marker(s(6))
        });
        app.set_loop_region(Some(bv_core::TimeRange::new(s(2), s(3))));
        let mut h = Harness::builder()
            .with_size(vec2(1600.0, 960.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(4);
        let layout = |h: &Harness<'_, BananaApp>| {
            let view = &h.state().timeline;
            view.layout(view.last_rect, h.state().project())
        };
        let l = layout(&h);
        let x = |t: Tick| h.state().timeline.x_of(&l, t);
        let row = &l.rows[0];
        let border = pos2(
            (l.headers.right() + l.lanes.left()) / 2.0,
            l.lanes.center().y,
        );
        let edge = pos2(l.headers.left() + 80.0, row.top + row.height - 1.0);
        let flag = pos2(x(s(6)) + 4.0, l.marker_bar.center().y);
        let ruler = pos2(x(s(8)), l.ruler.center().y);
        let loop_edge = pos2(x(s(2)), l.loop_bar.center().y);
        let loop_middle = pos2(x(Tick::from_seconds_f64(3.5)), l.loop_bar.center().y);
        assert_eq!(over(&mut h, border), Some(Cursor::SplitterEw), "border");
        assert_eq!(over(&mut h, edge), Some(Cursor::SplitterNs), "track edge");
        assert_eq!(over(&mut h, flag), Some(Cursor::ArrowLeftRight), "marker");
        assert_eq!(over(&mut h, loop_edge), Some(Cursor::ResizeEw), "loop edge");
        assert_eq!(
            over(&mut h, loop_middle),
            Some(Cursor::ArrowLeftRight),
            "loop middle"
        );
        assert_eq!(over(&mut h, ruler), None, "ruler");
        assert_eq!(
            h.output().platform_output.cursor_icon,
            egui::CursorIcon::Default
        );
        // Dragged, a marker keeps its pointer over the tracks too, and
        // moves as before.
        assert_eq!(
            drag(&mut h, flag, vec2(30.0, 120.0)),
            Some(Cursor::ArrowLeftRight)
        );
        let moved = h.state().project().markers.iter().find(|m| m.id == marker);
        assert_ne!(moved.map(|m| m.time), Some(s(6)), "the marker moved");
        // A track's height, dragged.
        let before = h.state().timeline.track_heights.clone();
        assert_eq!(
            drag(&mut h, edge, vec2(0.0, 24.0)),
            Some(Cursor::SplitterNs)
        );
        assert_ne!(h.state().timeline.track_heights, before, "it resized");
        // The bus tracks: their splitter and a bus track's bottom edge.
        h.state_mut().timeline.show_bus_tracks = true;
        h.run_steps(2);
        let l = layout(&h);
        let bus = &l.bus_rows[0];
        let bus_top = l.bus_split.center();
        let bus_edge = pos2(
            l.bus_headers.left() + 80.0,
            bus.top + bus.height - bus.keys - 1.0,
        );
        assert_eq!(over(&mut h, bus_edge), Some(Cursor::SplitterNs), "bus edge");
        assert_eq!(over(&mut h, bus_top), Some(Cursor::SplitterNs), "bus top");
        let area = h.state().timeline.bus_area;
        assert_eq!(
            drag(&mut h, bus_top, vec2(0.0, -20.0)),
            Some(Cursor::SplitterNs)
        );
        assert!(h.state().timeline.bus_area > area, "the bus area grew");
    }
}
