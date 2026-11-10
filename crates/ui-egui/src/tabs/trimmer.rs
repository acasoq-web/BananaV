//! Trimmer: look at a media file, mark in and out and take only that part
//! to the timeline, as VEGAS 23's (its Help's "Using the Trimmer" and the
//! user's prints of it). At its top the history — the media's name, then
//! its folder — with Sort, Clear, Remove and Trimmer on External Monitor;
//! then the picture (a sound: its ruler, its waveform and a scroll bar),
//! the loop bar with the media file's markers and regions, the time bar;
//! under them the transport (the buttons Edit Visible Button Set ticks,
//! the others in More) and the time boxes. Right-clicks give VEGAS's
//! menus: the picture's, the waveform's, the loop region's, the bar's
//! (everything) and the ruler's time formats. With the Trimmer focused,
//! the keys of its keyboard context (Options › Customize Keyboard ›
//! Trimmer) act here, Space and the transport keys playing its media: see
//! [`run`].
//!
//! Its time selection is the loop region selected: a drag across the
//! picture, the waveform or the loop bar, I and O, Select Loop Region; a
//! click elsewhere lets it go, the loop region staying on its bar (the
//! time boxes then show the cursor and the region's length dimmed). Its
//! markers and regions belong to the media file: the events using it show
//! them, and they are saved with the file (Save Markers/Regions, or at
//! once with Options › Preferences › General's "Automatically save trimmer
//! markers and regions with media file"). A subclip opens alone, its part
//! the whole of what is shown (Select Parent Media opens its parent, the
//! part selected). Three-point edits from or up to the cursor, Fit to
//! Fill, Enable Timeline Overwrite, T and Shift+T (the selection as long
//! as the timeline's), Tab cycling the streams, Copy (the selection as
//! events for Paste), snapshots of the frame, Loop Playback over the loop
//! region.

use crate::i18n::tr;
use bv_core::{FrameRate, Tick, TimeRange};
use bv_timeline::{MediaId, MediaInfo, MediaMarker, MediaSource, Project, TrackKind};

use crate::app::{BananaApp, MediaDrag};
use crate::egui::{
    self, Atom, Color32, ColorImage, Id, Rect, Sense, Stroke, TextureHandle, TextureOptions, pos2,
    vec2,
};
use crate::icons::Icon;
use crate::keymap::{Action, Context, Extra};
use crate::theme;
use crate::timefmt::{RulerFormat, RulerFormatExt};
use crate::timeline::toolbar::{TimeBox, paint_box_icon_in};
use crate::transport_buttons::{Bar, TransportButton as B, paint_trimmer_icon};

/// Which streams of the media the Trimmer selects and adds to the timeline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Streams {
    #[default]
    VideoAndAudio,
    VideoOnly,
    AudioOnly,
}

/// Commands of the Trimmer (its keyboard context, its buttons and menus).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrimmerOp {
    Start,
    End,
    /// One frame left (-1) or right (+1).
    Frame(i8),
    /// One pixel of the time bar left or right.
    Pixel(i8),
    /// The cursor goes to the in (or out) point.
    GoToIn,
    GoToOut,
    MarkIn,
    MarkOut,
    /// Adds the selection to the timeline starting at the timeline cursor
    /// (no selection: from the Trimmer's cursor to the media's end).
    AddFromCursor,
    /// Adds the selection to the timeline ending at the timeline cursor.
    AddToCursor,
    Select(Streams),
    /// Opens the next (+1) or previous (-1) media of the project.
    NextMedia(i8),
    OpenInAudioEditor,
    /// M: a marker at the cursor; R: a region over the loop region.
    InsertMarker,
    InsertRegion,
    /// S: Save Markers/Regions, with the media file.
    SaveMarkers,
    /// Shift+S: Options › Preferences › General's "Automatically save
    /// trimmer markers and regions with media file", on or off.
    AutoSaveMarkers,
    /// The cursor to the marker named `n` (0–9).
    GoToMarker(u8),
    /// The cursor to the previous (-1) or next (+1) marker or region edge.
    MarkerStep(i8),
    /// The media's beats as markers (Beat Detection).
    BeatDetection,
    /// Enable Timeline Overwrite, on or off.
    Overwrite,
    /// Fit to Fill: the selection stretched over the timeline's time
    /// selection.
    FitToFill,
    /// The history sorted by name (`true`: Z to A).
    SortHistory(bool),
    /// Clear Trimmer History; Remove Current Media from Trimmer History;
    /// the media no event uses taken out of it.
    ClearHistory,
    RemoveFromHistory,
    PurgeHistory,
    /// T and Shift+T: the selection as long as the timeline's time
    /// selection, from (or up to) the cursor.
    SyncSelection {
        up_to: bool,
    },
    /// Tab and Shift+Tab: the next or previous of video and audio, video
    /// only, audio only (the Trimmer's "tracks").
    CycleStreams(i8),
    /// Zoom Selection: the time bar shows the loop region.
    ZoomSelection,
    /// Video Height Larger (1), Audio Height Larger (-1), Video and Audio
    /// Equal Height (0): Ctrl+Shift+Up and Down.
    Heights(i8),
    /// Open Copy in Audio Editor.
    OpenCopyInAudioEditor,
    /// Save Snapshot to File: the frame at the cursor as a picture,
    /// added to Project Media.
    SaveSnapshot,
    /// Create Subclip: the selection as an item of Project Media.
    CreateSubclip,
    /// The transport: Play, Pause (Enter), Stop, Play From Start
    /// (Shift+Space), Space's play or stop.
    Play,
    Pause,
    Stop,
    PlayFromStart,
    PlayToggle,
    /// Select Loop Region (Shift+L, Shift+Q): the loop region selected.
    SelectLoop,
    /// Set Selection to View: the loop region as the time bar shows.
    SelectView,
    /// Show Video in Preview Window (Ctrl+P), on or off.
    TogglePreviewWindow,
    /// Trimmer on External Monitor, on or off.
    ExternalMonitor,
    /// Copy: the selection as events, for Paste on the timeline.
    Copy,
    /// Select Parent Media: a subclip's parent, its part selected.
    SelectParentMedia,
    /// Detect Scenes and Add to Timeline from Cursor.
    DetectScenes,
    /// Stream and Channels: the audio stream and the pair of channels
    /// shown; Use All Streams and Channels, on or off.
    AudioStream(u8),
    ChannelPair(u8),
    UseAllStreams,
}

/// A drag on the loop bar.
#[derive(Clone, Copy, Debug, PartialEq)]
enum LoopDrag {
    /// One of its handles.
    Start,
    End,
    /// The bar itself, grabbed this far from its start.
    Move(Tick),
    /// A new region, from this time.
    New(Tick),
}

#[derive(Default)]
pub struct TrimmerState {
    media: Option<MediaId>,
    /// The media opened in the Trimmer, the latest first (its history).
    history: Vec<MediaId>,
    cursor: Tick,
    /// The loop region's ends (none: all of the media).
    in_point: Option<Tick>,
    out_point: Option<Tick>,
    /// The time selection is on: the loop region selected.
    selected: bool,
    streams: Streams,
    /// The audio stream (Stream) and the pair of channels (Channels) shown.
    audio_stream: usize,
    channel_pair: usize,
    /// Width of the time bar, for pixel steps.
    bar_width: f32,
    texture: Option<TextureHandle>,
    shown: Option<(u64, Tick, u32, u32)>,
    /// A media marker being renamed: its index and the text.
    renaming: Option<(usize, String)>,
    /// A media marker being dragged: its index and whether it is a
    /// region's end.
    marker_drag: Option<(usize, bool)>,
    /// The media marker a right-click opened the menu of.
    marker_menu: Option<usize>,
    loop_drag: Option<LoopDrag>,
    /// The in (`false`) or out line of the picture being dragged.
    point_drag: Option<bool>,
    /// Enable Timeline Overwrite: what is added replaces the time it takes
    /// on its tracks.
    pub(crate) overwrite: bool,
    /// Trimmer on External Monitor: the external monitor shows the
    /// Trimmer's picture while the Trimmer has the focus.
    pub(crate) external: bool,
    /// The part of the media the time bar shows (`None`: all of it).
    view: Option<TimeRange>,
    /// The waveform's share of the height under the picture (Audio Height
    /// Larger; none at first, as VEGAS's).
    audio_share: f32,
    /// A time box being typed in.
    typing: Option<(TimeBox, String)>,
    /// Enable Hover Scrub: the media's time under the pointer over the
    /// picture (shown instead of the cursor's).
    hover: Option<Tick>,
    /// A drag across the picture or the waveform selecting, from this
    /// time.
    selecting: Option<Tick>,
    /// Where the picture or the waveform was drawn last, and the loop bar,
    /// the time bar, a sound's ruler and More (screen tests).
    media_rect: Option<Rect>,
    drawn: [Option<Rect>; 4],
    /// A subclip shown alone: where its part lies in its parent's file and
    /// whether it plays backward (a preview's times are the file's).
    part: Option<(TimeRange, bool)>,
}

impl std::fmt::Debug for TrimmerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrimmerState")
            .field("media", &self.media)
            .finish()
    }
}

/// What the Trimmer shows: a media of the project, or a subclip alone —
/// its time the part's, from its start.
#[derive(Clone, Debug)]
struct Showing {
    info: MediaInfo,
    /// A subclip: its parent, where its part lies there and whether it
    /// plays backward.
    part: Option<(MediaId, TimeRange, bool)>,
}

impl Showing {
    fn of(p: &Project, id: MediaId) -> Option<Showing> {
        let Some(s) = p.subclip(id) else {
            return Some(Showing {
                info: p.media(id)?.clone(),
                part: None,
            });
        };
        let mut info = p.subclip_info(id)?;
        // The parent's markers within the part, in the part's time.
        if !s.reverse {
            info.markers = p
                .media(s.parent)?
                .markers
                .iter()
                .filter(|m| s.range.contains(m.time))
                .map(|m| MediaMarker {
                    time: m.time - s.range.start,
                    length: m.length.min(s.range.end() - m.time),
                    name: m.name.clone(),
                })
                .collect();
        }
        Some(Showing {
            info,
            part: Some((s.parent, s.range, s.reverse)),
        })
    }

    /// The file's time of time `t` of what is shown.
    fn file_time(&self, t: Tick) -> Tick {
        match self.part {
            Some((_, r, true)) => r.end() - t.clamp(Tick::ZERO, r.duration),
            Some((_, r, false)) => r.start + t.clamp(Tick::ZERO, r.duration),
            None => t,
        }
    }

    /// The part of the file of `r` of what is shown.
    fn file_range(&self, r: TimeRange) -> TimeRange {
        let (a, b) = (self.file_time(r.start), self.file_time(r.end()));
        TimeRange::from_bounds(a.min(b), a.max(b))
    }

    /// The media whose file it is (a subclip's parent).
    fn file_media(&self) -> MediaId {
        self.part.map_or(self.info.id, |(p, ..)| p)
    }

    /// Its markers can be changed here (a subclip's are its parent's).
    fn own_markers(&self) -> bool {
        self.part.is_none()
    }
}

/// What the Trimmer shows of a media: its length (generated media and stills
/// last forever: 60 s are shown; a subclip, its part) and frame rate.
fn extent(show: &Showing, project_rate: FrameRate) -> (Tick, FrameRate) {
    let info = &show.info;
    let rate = info.video.map_or(project_rate, |v| v.frame_rate);
    let length = if let Some((_, part, _)) = show.part {
        part.duration
    } else if info.still || matches!(info.source, MediaSource::Generated(_)) {
        Tick::from_seconds(60)
    } else {
        info.duration
    };
    (length, rate)
}

/// Where the media ends for its loop region and what is added of it: its
/// length (a subclip's part's), 5 s of generated media, stills as long as
/// Options › Preferences › Editing says (`still`).
fn media_end(show: &Showing, still: Tick) -> Tick {
    let info = &show.info;
    if let Some((_, part, _)) = show.part {
        part.duration
    } else if info.still {
        still
    } else if matches!(info.source, MediaSource::Generated(_)) {
        bv_timeline::Generator::DEFAULT_CLIP
    } else {
        info.duration
    }
}

/// The loop region: in to out, all of the media without them (media with
/// no end, generated or still: from the in point as long as a new event).
fn selection(st: &TrimmerState, show: &Showing, still: Tick) -> TimeRange {
    let end = media_end(show, still);
    let start = st.in_point.unwrap_or(Tick::ZERO);
    let open_ended = show.part.is_none()
        && (show.info.still || matches!(show.info.source, MediaSource::Generated(_)));
    let stop = match st.out_point {
        Some(out) => out,
        None if open_ended => start + end,
        None => end,
    };
    TimeRange::from_bounds(start.min(stop), stop.max(start))
}

/// What is taken without a selection: from the cursor to the media's end
/// (media with no end: as long as a new event).
fn from_cursor(st: &TrimmerState, show: &Showing, still: Tick) -> TimeRange {
    let end = media_end(show, still);
    let open_ended = show.part.is_none()
        && (show.info.still || matches!(show.info.source, MediaSource::Generated(_)));
    if open_ended {
        TimeRange::new(st.cursor, end)
    } else {
        TimeRange::from_bounds(st.cursor.min(end), end)
    }
}

/// The time selection, when there is one: the loop region selected.
fn active(st: &TrimmerState, show: &Showing, still: Tick) -> Option<TimeRange> {
    st.selected
        .then(|| selection(st, show, still))
        .filter(|r| r.duration > Tick::ZERO)
}

impl TrimmerState {
    fn mark_in(&mut self) {
        self.in_point = Some(self.cursor);
        if self.out_point.is_some_and(|o| o <= self.cursor) {
            self.out_point = None;
        }
        self.selected = true;
    }

    fn mark_out(&mut self) {
        self.out_point = Some(self.cursor);
        if self.in_point.is_some_and(|i| i >= self.cursor) {
            self.in_point = None;
        }
        self.selected = true;
    }

    /// The cursor's time in the media.
    pub(crate) fn marks_cursor(&self) -> Tick {
        self.cursor
    }

    /// The media it shows.
    pub(crate) fn media_id(&self) -> Option<MediaId> {
        self.media
    }

    /// The cursor moved (a preview playing).
    pub(crate) fn set_cursor(&mut self, t: Tick) {
        self.cursor = t.max(Tick::ZERO);
    }

    /// The cursor moved to time `f` of the file (a preview playing a
    /// subclip's parent).
    pub(crate) fn set_cursor_file(&mut self, f: Tick) {
        let t = match self.part {
            Some((r, true)) => r.end() - f,
            Some((r, false)) => f - r.start,
            None => f,
        };
        self.set_cursor(t);
    }

    /// Cursor, in and out points (tests).
    #[cfg(test)]
    pub(crate) fn marks(&self) -> (Tick, Option<Tick>, Option<Tick>) {
        (self.cursor, self.in_point, self.out_point)
    }

    pub fn open(&mut self, id: MediaId) {
        self.history.retain(|&m| m != id);
        self.history.insert(0, id);
        self.history.truncate(HISTORY);
        if self.media != Some(id) {
            self.media = Some(id);
            self.cursor = Tick::ZERO;
            self.in_point = None;
            self.out_point = None;
            self.selected = false;
            self.audio_stream = 0;
            self.channel_pair = 0;
            self.shown = None;
            self.renaming = None;
            self.marker_drag = None;
            self.marker_menu = None;
            self.loop_drag = None;
            self.view = None;
            self.typing = None;
            self.part = None;
        }
    }

    /// Opens media `id` with `part` selected (a subclip's, an event's).
    pub fn open_part(&mut self, id: MediaId, part: TimeRange) {
        self.open(id);
        self.in_point = Some(part.start);
        self.out_point = Some(part.end());
        self.cursor = part.start;
        self.selected = true;
    }

    /// The part of the media the time bar shows.
    fn shown(&self, length: Tick) -> TimeRange {
        self.view
            .filter(|v| v.duration > Tick::ZERO)
            .unwrap_or(TimeRange::new(Tick::ZERO, length.max(Tick(1))))
    }

    /// With Loop Playback on, where a preview at `t` goes back to: the
    /// loop region's start once it is past its end.
    pub(crate) fn loop_back(&self, t: Tick) -> Option<Tick> {
        let (a, b) = (self.in_point?, self.out_point?);
        (b > a && t >= b).then_some(a)
    }
}

impl BananaApp {
    /// Changes the markers of media `id`: one undo (one per drag, with
    /// `key`). Then, unless `dragging`, they are put in time order and, with
    /// Options › Preferences › General's "Automatically save trimmer markers
    /// and regions with media file", saved with the file.
    pub(crate) fn edit_media_markers(
        &mut self,
        id: MediaId,
        key: Option<egui::Id>,
        dragging: bool,
        f: impl FnOnce(&mut Vec<MediaMarker>),
    ) {
        let edit = move |p: &mut bv_timeline::Project| {
            if let Some(m) = p.media.iter_mut().find(|m| m.id == id) {
                f(&mut m.markers);
                if !dragging {
                    m.markers.sort_by_key(|m| m.time);
                }
            }
        };
        match key {
            Some(key) => self.modify_merged(key, edit),
            None => self.modify(edit),
        }
        if !dragging && self.settings.prefs.save_trimmer_markers {
            self.save_media_markers(id, false);
        }
    }

    /// Trimmer › Save Markers/Regions: the markers of media `id` saved with
    /// its file (in it, or beside it); `tell` says where.
    pub(crate) fn save_media_markers(&mut self, id: MediaId, tell: bool) {
        let Some(m) = self.project().media(id) else {
            return;
        };
        let name = crate::i18n::media_name(m);
        let MediaSource::File { path } = &m.source else {
            if tell {
                self.notify(tr("Generated media has no file to keep markers in."));
            }
            return;
        };
        if !bv_engine::can_save_markers(path) {
            self.report_error(trf!(
                "{} is read-only: its markers and regions cannot be saved.",
                name
            ));
            return;
        }
        match bv_engine::save_markers(path, &m.markers) {
            Ok(bv_engine::MarkerStore::InFile) if tell => {
                self.notify(trf!("Markers and regions saved in {}.", name));
            }
            Ok(bv_engine::MarkerStore::Beside) if tell => {
                self.notify(trf!("Markers and regions saved beside {}.", name));
            }
            Ok(_) => {}
            Err(e) => self.report_error(trf!(
                "Could not save the markers and regions of {}: {}",
                name,
                e
            )),
        }
    }

    /// The Trimmer's media (or subclip) shown.
    fn trimmer_showing(&self) -> Option<Showing> {
        Showing::of(self.project(), self.trimmer.media?)
    }

    /// The Trimmer's time text of `t`: its format (its ruler's menu), at
    /// the media's frame rate or, with Use Project Frame Rate, the
    /// project's.
    fn trimmer_time(&self, info: &MediaInfo, t: Tick) -> String {
        let (format, settings) = self.trimmer_format(info);
        format.format_ruler_time(t, &settings)
    }

    /// The Trimmer's time format and the settings it counts with.
    fn trimmer_format(&self, info: &MediaInfo) -> (RulerFormat, bv_timeline::ProjectSettings) {
        let format = self
            .settings
            .trimmer_format
            .unwrap_or_else(|| self.effective_ruler_format());
        let mut settings = self.project().settings;
        if !self.settings.trimmer_project_rate
            && let Some(v) = info.video
        {
            settings.frame_rate = v.frame_rate;
        }
        (format, settings)
    }
}

/// Whether the Trimmer can save the markers of `info` with its file.
fn can_save(info: &MediaInfo) -> bool {
    matches!(&info.source, MediaSource::File { path } if bv_engine::can_save_markers(path))
}

/// The marker or region edge of `markers` under `x` on the Trimmer's marker
/// strip: its index, and whether it is a region's end.
fn marker_at(markers: &[MediaMarker], x: f32, to_x: impl Fn(Tick) -> f32) -> Option<(usize, bool)> {
    let mut best: Option<(f32, usize, bool)> = None;
    for (i, m) in markers.iter().enumerate() {
        let mut edges = vec![(to_x(m.time), false)];
        if m.is_region() {
            edges.push((to_x(m.end()), true));
        }
        for (ex, end) in edges {
            // The flag reaches right of the line (VEGAS's, 18 points).
            let d = if !end && x >= ex && x <= ex + 19.0 {
                0.0
            } else {
                (x - ex).abs()
            };
            if d <= 5.0 && best.is_none_or(|(bd, _, _)| d < bd) {
                best = Some((d, i, end));
            }
        }
    }
    best.map(|(_, i, end)| (i, end))
}

/// Media files the Trimmer's history keeps.
const HISTORY: usize = 30;

/// The yellow of the Trimmer's loop region handles and time boxes, the
/// pale yellow of their poles, their dark edges; the bars' dark and light
/// lines and the loop region's grey, as VEGAS's.
const LOOP_YELLOW: Color32 = Color32::from_rgb(0xE8, 0xC7, 0x3E);
const LOOP_POLE: Color32 = Color32::from_rgb(0xD1, 0xC4, 0x94);
const LOOP_EDGE: Color32 = Color32::from_rgb(0x17, 0x15, 0x00);
const DARK_LINE: Color32 = Color32::from_gray(20);
const LIGHT_LINE: Color32 = Color32::from_gray(120);
const REGION_GREY: Color32 = Color32::from_gray(61);

/// The heights of the Trimmer's rows: its header, the transport, the time
/// boxes, the loop bar with the markers, the time bar, a sound's ruler and
/// scroll bar.
const HEADER_H: f32 = 24.0;
const TRANSPORT_H: f32 = 30.0;
const BOXES_H: f32 = 26.0;
/// As VEGAS's: a dark band under the picture, the loop bar (taller with
/// the media's markers, their flags above the loop region's handles) and
/// the time bar.
const BAND_H: f32 = 4.0;
const STRIP_H: f32 = 9.0;
const STRIP_MARKS_H: f32 = 22.0;
const BAR_H: f32 = 17.0;
const FRAMES_H: f32 = 30.0;
const RULER_H: f32 = 20.0;
const SCROLL_H: f32 = 16.0;

/// A name cut in its middle when long, as VEGAS's history shows it
/// ("Characters_walking_in_loop_anima..._20261020224214.mp4").
fn middle_cut(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= 52 {
        return name.to_string();
    }
    let head: String = chars[..30].iter().collect();
    let tail: String = chars[chars.len() - 19..].iter().collect();
    format!("{head}...{tail}")
}

/// A media file as the Trimmer's history lists it: its name, then its
/// folder in brackets ("take.mp4   [C:\Videos\]"), or (Options ›
/// Preferences › General: "Show trimmer history with filename first, then
/// folder" off) its whole path.
fn history_name(m: &MediaInfo, name_first: bool) -> String {
    let name = crate::i18n::media_name(m);
    let MediaSource::File { path } = &m.source else {
        return name;
    };
    let path = std::path::Path::new(path);
    match path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(folder) if name_first => {
            let sep = std::path::MAIN_SEPARATOR;
            format!(
                "{}   [{}{sep}]",
                middle_cut(&name),
                folder.display().to_string().trim_end_matches(sep)
            )
        }
        Some(folder) => folder.join(&name).display().to_string(),
        None => name,
    }
}

/// The header: the history's list (VEGAS's, the media shown in it) and
/// its buttons — Sort Trimmer History (Ctrl: Z to A), Clear Trimmer
/// History, Remove Current Media from Trimmer History, Trimmer on External
/// Monitor. `true` when one of them ran (the frame ends there).
fn header(ui: &mut egui::Ui, app: &mut BananaApp, whole: Rect) -> bool {
    let name_first = app.settings.prefs.trimmer_name_first;
    let project = app.project();
    let media: Vec<(MediaId, String)> = app
        .trimmer
        .history
        .iter()
        .filter_map(|&id| {
            project
                .media(id)
                .cloned()
                .or_else(|| project.subclip_info(id))
        })
        .map(|m| (m.id, history_name(&m, name_first)))
        .collect();
    let known = |app: &BananaApp, id: MediaId| {
        app.project().media(id).is_some() || app.project().subclip(id).is_some()
    };
    if app.trimmer.media.is_none_or(|id| !known(app, id)) {
        app.trimmer.media = None;
    }
    let mut op = None;
    // In the pane's width: the list takes what the four buttons leave.
    let row = Rect::from_min_size(whole.min, vec2(whole.width(), HEADER_H));
    let list_w = (row.width() - 4.0 * 26.0 - 4.0).max(80.0);
    ui.scope_builder(egui::UiBuilder::new().max_rect(row), |ui| {
        ui.set_clip_rect(row.intersect(ui.clip_rect()));
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let current = app
                .trimmer
                .media
                .and_then(|id| media.iter().find(|m| m.0 == id))
                .map_or(String::new(), |m| m.1.clone());
            ui.allocate_ui(vec2(list_w, HEADER_H - 2.0), |ui| {
                ui.set_max_width(list_w);
                egui::ComboBox::from_id_salt("trimmer-midia")
                    .width(list_w)
                    .truncate()
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (id, name) in &media {
                            if ui
                                .selectable_label(app.trimmer.media == Some(*id), name)
                                .clicked()
                            {
                                app.trimmer.open(*id);
                            }
                        }
                    });
            });
            let sort = header_button(
                ui,
                HeaderIcon::Sort,
                tr("Sort Trimmer History"),
                true,
                false,
            );
            if sort.clicked() {
                let reverse = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
                op = Some(TrimmerOp::SortHistory(reverse));
            }
            if header_button(
                ui,
                HeaderIcon::Clear,
                tr("Clear Trimmer History"),
                true,
                false,
            )
            .clicked()
            {
                op = Some(TrimmerOp::ClearHistory);
            }
            let has = app.trimmer.media.is_some();
            if header_button(
                ui,
                HeaderIcon::Remove,
                tr("Remove Current Media from Trimmer History"),
                has,
                false,
            )
            .clicked()
            {
                op = Some(TrimmerOp::RemoveFromHistory);
            }
            if header_button(
                ui,
                HeaderIcon::External,
                tr("Trimmer on External Monitor"),
                true,
                app.trimmer.external,
            )
            .clicked()
            {
                op = Some(TrimmerOp::ExternalMonitor);
            }
        });
    });
    ui.advance_cursor_after_rect(row);
    match op {
        Some(op) => {
            run(app, op);
            true
        }
        None => false,
    }
}

/// The header's buttons, drawn as VEGAS's.
#[derive(Clone, Copy, PartialEq)]
enum HeaderIcon {
    Sort,
    Clear,
    Remove,
    External,
}

fn header_button(
    ui: &mut egui::Ui,
    icon: HeaderIcon,
    tip: &str,
    enabled: bool,
    on: bool,
) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 22.0), sense);
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, 3.0, theme::button_highlight());
    } else if resp.hovered() && enabled {
        painter.rect_filled(rect, 3.0, theme::hover());
    }
    let ink = if enabled {
        theme::text()
    } else {
        theme::text_disabled()
    };
    let r = Rect::from_center_size(rect.center(), vec2(16.0, 16.0));
    match icon {
        HeaderIcon::Sort => {
            // "A" over "Z", an arrow down beside them.
            let font = egui::FontId::proportional(8.5);
            painter.text(
                pos2(r.left() + 4.0, r.top() + 4.0),
                egui::Align2::CENTER_CENTER,
                "A",
                font.clone(),
                ink,
            );
            painter.text(
                pos2(r.left() + 4.0, r.bottom() - 4.0),
                egui::Align2::CENTER_CENTER,
                "Z",
                font,
                ink,
            );
            let x = r.left() + 11.5;
            let s = Stroke::new(1.2, ink);
            painter.line_segment([pos2(x, r.top() + 1.0), pos2(x, r.bottom() - 1.0)], s);
            painter.line_segment(
                [pos2(x, r.bottom() - 1.0), pos2(x - 3.0, r.bottom() - 4.0)],
                s,
            );
            painter.line_segment(
                [pos2(x, r.bottom() - 1.0), pos2(x + 3.0, r.bottom() - 4.0)],
                s,
            );
        }
        HeaderIcon::Clear => {
            // A list, a red cross over its corner.
            painter.rect_filled(
                Rect::from_min_max(r.min + vec2(1.0, 2.0), r.min + vec2(12.0, 13.0)),
                1.0,
                ink,
            );
            let bg = theme::panel();
            for y in [5.0, 9.0] {
                painter.line_segment(
                    [r.min + vec2(6.0, y), r.min + vec2(10.5, y)],
                    Stroke::new(1.2, bg),
                );
                painter.rect_filled(
                    Rect::from_min_size(r.min + vec2(2.5, y - 1.0), vec2(2.0, 2.0)),
                    0.0,
                    bg,
                );
            }
            cross(
                painter,
                Rect::from_min_size(r.min + vec2(8.0, 8.0), vec2(7.0, 7.0)),
                enabled,
            );
        }
        HeaderIcon::Remove => cross(painter, r.shrink(2.0), enabled),
        HeaderIcon::External => crate::icons::paint(painter, r, Icon::ExternalMonitor, ink),
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));
    resp.on_hover_text(tip)
}

/// VEGAS's red cross.
fn cross(painter: &egui::Painter, r: Rect, enabled: bool) {
    let red = if enabled {
        theme::error()
    } else {
        theme::text_disabled()
    };
    let s = Stroke::new(2.0, red);
    painter.line_segment([r.left_top(), r.right_bottom()], s);
    painter.line_segment([r.right_top(), r.left_bottom()], s);
}

pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    // The pane, measured before anything is laid out in it.
    let whole = ui.available_rect_before_wrap();
    if header(ui, app, whole) {
        return;
    }
    // A file of the Explorer previewing: its picture here, as VEGAS's
    // ("Video is previewed in the Trimmer window").
    if let Some(p) = app.previews.playing.clone()
        && p.media.is_none()
        && p.info.video.is_some()
        && !app.settings.trimmer_no_monitor
    {
        file_preview(ui, app, &p.info);
        return;
    }
    let Some(show) = app.trimmer_showing() else {
        super::hint(
            ui,
            tr("Click a media file (Project Media or Explorer) to open it here."),
        );
        return;
    };
    let info = show.info.clone();
    let (length, rate) = extent(&show, app.frame_rate());
    {
        let st = &mut app.trimmer;
        st.part = show.part.map(|(_, r, back)| (r, back));
        st.cursor = st.cursor.clamp(Tick::ZERO, length);
        st.audio_stream = st.audio_stream.min(info.extra_audio.len());
    }
    let full = Rect::from_min_max(pos2(whole.left(), whole.top() + HEADER_H), whole.max);
    let boxes = Rect::from_min_max(pos2(full.left(), full.bottom() - BOXES_H), full.max);
    let transport = Rect::from_min_max(
        pos2(full.left(), boxes.top() - TRANSPORT_H),
        pos2(full.right(), boxes.top()),
    );
    let body = Rect::from_min_max(full.min, pos2(full.right(), transport.top()));
    let picture = info.video.is_some() && !app.settings.trimmer_no_monitor;
    let mut ops = if picture {
        video_body(ui, app, &show, body, length, rate)
    } else {
        sound_body(ui, app, &show, body, length, rate)
    };
    ops.extend(transport_row(ui, app, &show, transport));
    time_boxes(ui, app, &show, boxes);
    ui.advance_cursor_after_rect(full);
    for op in ops {
        run(app, op);
    }
}

/// Where times fall across `rect`, the part `shown` from side to side.
#[derive(Clone, Copy)]
struct Span {
    rect: Rect,
    shown: TimeRange,
}

impl Span {
    fn x(&self, t: Tick) -> f32 {
        self.rect.left()
            + ((t - self.shown.start).0 as f64 / self.shown.duration.0.max(1) as f64) as f32
                * self.rect.width()
    }

    fn t(&self, x: f32) -> Tick {
        let f = ((x - self.rect.left()) / self.rect.width().max(1.0)).clamp(0.0, 1.0) as f64;
        self.shown.start + Tick((self.shown.duration.0 as f64 * f) as i64)
    }
}

/// A video's body: its picture (a waveform under it when Audio Height
/// Larger gives it room), the loop bar and the time bar (its frames with
/// Show Video Frames).
fn video_body(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    show: &Showing,
    body: Rect,
    length: Tick,
    rate: FrameRate,
) -> Vec<TrimmerOp> {
    let info = &show.info;
    let frames = app.settings.trimmer_frames;
    let bar_h = if frames { FRAMES_H } else { BAR_H };
    let strip_h = strip_height(app, show);
    let room = body.height() - BAND_H - strip_h - bar_h - 2.0;
    let wave_h = if info.audio.is_some() {
        (room * app.trimmer.audio_share).floor()
    } else {
        0.0
    };
    let pic_area = Rect::from_min_max(
        body.min + vec2(0.0, 2.0),
        pos2(body.right(), body.top() + 2.0 + room - wave_h),
    );
    let wave_area = Rect::from_min_max(
        pos2(body.left(), pic_area.bottom()),
        pos2(body.right(), pic_area.bottom() + wave_h),
    );
    let band = Rect::from_min_max(
        pos2(body.left(), wave_area.bottom()),
        pos2(body.right(), wave_area.bottom() + BAND_H),
    );
    let strip = Rect::from_min_max(
        pos2(body.left(), band.bottom()),
        pos2(body.right(), band.bottom() + strip_h),
    );
    let bar = Rect::from_min_max(
        pos2(strip.left(), strip.bottom()),
        pos2(strip.right(), strip.bottom() + bar_h),
    );
    ui.painter().rect_filled(band, 0.0, DARK_LINE);
    let playing = app.trimmer_playing();
    let st = &mut app.trimmer;
    st.bar_width = bar.width();
    st.drawn[..3].copy_from_slice(&[Some(strip), Some(bar), None]);
    let shown = st.shown(length);
    // The picture at the hover's time or the cursor's (playing, the
    // cursor's: the frames drawn ahead are of it).
    let at = if playing {
        st.cursor
    } else {
        st.hover.unwrap_or(st.cursor)
    };
    let ppp = ui.ctx().pixels_per_point();
    app.engine.request_trimmer(
        info,
        show.file_time(at),
        (pic_area.width() * ppp) as u32,
        (pic_area.height() * ppp) as u32,
    );
    take_frame(ui, &mut app.trimmer, &app.engine);
    let painter = ui.painter().clone();
    painter.rect_filled(pic_area, 0.0, Color32::BLACK);
    let img = match (info.video, &app.trimmer.texture) {
        (Some(v), Some(t)) => {
            let aspect = v.width as f32 / v.height.max(1) as f32;
            let mut size = pic_area.size();
            if size.x / size.y > aspect {
                size.x = size.y * aspect;
            } else {
                size.y = size.x / aspect;
            }
            let r = Rect::from_center_size(pic_area.center(), size);
            painter.image(
                t.id(),
                r,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            r
        }
        _ => pic_area,
    };
    // Times run across the picture as along the time bar.
    let across = Span { rect: img, shown };
    let still = app.settings.prefs.new_events().still_length;
    marks_over(&painter, &app.trimmer, show, still, across, img);
    let mut ops = media_area(ui, app, show, pic_area, across, rate, Area::Picture);
    if wave_h > 4.0 {
        let span = Span {
            rect: wave_area,
            shown,
        };
        paint_waveform(ui, app, show, wave_area, shown);
        marks_over(&painter, &app.trimmer, show, still, span, wave_area);
        ops.extend(media_area(
            ui,
            app,
            show,
            wave_area,
            span,
            rate,
            Area::Waveform,
        ));
    }
    // The time bar first: the loop region's grey and the flags' poles run
    // down over it.
    ops.extend(time_bar(
        ui,
        app,
        show,
        bar,
        Span { rect: bar, shown },
        rate,
        length,
        frames,
    ));
    ops.extend(loop_strip(
        ui,
        app,
        show,
        strip,
        Span { rect: strip, shown },
        rate,
        bar.bottom(),
    ));
    paint_cursor(
        ui.painter(),
        Span { rect: bar, shown }.x(app.trimmer.cursor),
        bar.top(),
        bar.bottom(),
    );
    ops
}

/// The loop bar's height: taller while the media has markers (or Beat
/// Detection's beats show), their flags above the loop region's handles.
fn strip_height(app: &BananaApp, show: &Showing) -> f32 {
    if !show.info.markers.is_empty() || app.beat_preview(show.file_media()).is_some() {
        STRIP_MARKS_H
    } else {
        STRIP_H
    }
}

/// A sound's body (or a video's with Show Video Monitor off): the loop
/// bar, the ruler, the waveform and its scroll bar with + and −.
fn sound_body(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    show: &Showing,
    body: Rect,
    length: Tick,
    rate: FrameRate,
) -> Vec<TrimmerOp> {
    let info = &show.info;
    let strip = Rect::from_min_max(
        pos2(body.left(), body.top() + 2.0),
        pos2(body.right(), body.top() + 2.0 + strip_height(app, show)),
    );
    let ruler = Rect::from_min_max(
        pos2(strip.left(), strip.bottom()),
        pos2(strip.right(), strip.bottom() + RULER_H),
    );
    let scroll = Rect::from_min_max(
        pos2(strip.left(), body.bottom() - SCROLL_H - 2.0),
        pos2(strip.right(), body.bottom() - 2.0),
    );
    let wave = Rect::from_min_max(
        pos2(strip.left(), ruler.bottom()),
        pos2(strip.right(), scroll.top() - 2.0),
    );
    let st = &mut app.trimmer;
    st.bar_width = wave.width();
    st.drawn[..3].copy_from_slice(&[Some(strip), None, Some(ruler)]);
    let shown = st.shown(length);
    let span = Span { rect: wave, shown };
    let mut ops = Vec::new();
    if info.audio.is_some() {
        paint_waveform(ui, app, show, wave, shown);
    } else {
        let painter = ui.painter();
        painter.rect_filled(wave, 0.0, theme::panel_dark());
        painter.text(
            wave.center(),
            egui::Align2::CENTER_CENTER,
            tr("The picture shows in the Video Preview window (Show Video in Preview Window)."),
            egui::FontId::proportional(12.0),
            theme::text_dim(),
        );
    }
    let still = app.settings.prefs.new_events().still_length;
    marks_over(ui.painter(), &app.trimmer, show, still, span, wave);
    ops.extend(media_area(ui, app, show, wave, span, rate, Area::Waveform));
    ops.extend(loop_strip(
        ui,
        app,
        show,
        strip,
        Span { rect: strip, shown },
        rate,
        strip.bottom(),
    ));
    paint_ruler(ui, app, info, ruler, Span { rect: ruler, shown });
    scroll_bar(ui, app, scroll, length);
    ops
}

/// The frame the engine has for the Trimmer, into its texture.
fn take_frame(ui: &egui::Ui, st: &mut TrimmerState, engine: &bv_engine::Engine) {
    let Some(f) = engine.trimmer_frame() else {
        return;
    };
    let key = (f.generation, f.time, f.width, f.height);
    if st.shown == Some(key) {
        return;
    }
    let img = super::preview::frame_image([f.width as usize, f.height as usize], &f.rgba);
    match &mut st.texture {
        Some(t) => t.set(img, TextureOptions::LINEAR),
        None => {
            st.texture = Some(
                ui.ctx()
                    .load_texture("trimmer", img, TextureOptions::LINEAR),
            )
        }
    }
    st.shown = Some(key);
}

/// Over the picture or the waveform: outside the time selection darker,
/// its ends as white lines, the cursor as a faint one.
fn marks_over(
    painter: &egui::Painter,
    st: &TrimmerState,
    show: &Showing,
    still: Tick,
    span: Span,
    area: Rect,
) {
    let painter = painter.with_clip_rect(area);
    if let Some(r) = active(st, show, still) {
        let (a, b) = (span.x(r.start), span.x(r.end()));
        let shade = Color32::from_black_alpha(80);
        painter.rect_filled(
            Rect::from_min_max(area.min, pos2(a.max(area.left()), area.bottom())),
            0.0,
            shade,
        );
        painter.rect_filled(
            Rect::from_min_max(pos2(b.min(area.right()), area.top()), area.max),
            0.0,
            shade,
        );
        for x in [a, b] {
            painter.line_segment(
                [pos2(x, area.top()), pos2(x, area.bottom())],
                Stroke::new(1.0, Color32::WHITE),
            );
        }
    }
    let x = span.x(st.cursor);
    painter.line_segment(
        [pos2(x, area.top()), pos2(x, area.bottom())],
        Stroke::new(1.0, Color32::from_white_alpha(110)),
    );
}

/// Which of the Trimmer's places a right-click was on: its menus differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Area {
    Picture,
    Waveform,
    /// The time bar, or the loop bar out of the loop region: everything.
    Bar,
    LoopRegion,
}

/// The picture or the waveform: a click puts the cursor there (the
/// selection let go), a drag across selects (from left to right, the in
/// point first; on an end of the selection, that end alone), one starting
/// up or down takes the selection to the timeline, a double-click selects
/// all of it; hover scrub; the menu.
fn media_area(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    show: &Showing,
    area: Rect,
    span: Span,
    rate: FrameRate,
    kind: Area,
) -> Vec<TrimmerOp> {
    let id = show.info.id;
    let resp = ui.interact(
        area,
        ui.id().with(("trimmer-area", kind as u8)),
        Sense::click_and_drag(),
    );
    app.trimmer.media_rect = Some(area);
    let still = app.settings.prefs.new_events().still_length;
    let time_at = |x: f32| rate.snap_floor(span.t(x));
    let st = &mut app.trimmer;
    if resp.drag_started() {
        let origin = ui.input(|i| i.pointer.press_origin());
        let now = resp.interact_pointer_pos();
        let across = match (origin, now) {
            (Some(o), Some(p)) => (p.x - o.x).abs() >= (p.y - o.y).abs(),
            _ => false,
        };
        let sel = active(st, show, still);
        // An end of the selection grabbed: it alone moves.
        let end_at = origin
            .zip(sel)
            .and_then(|(o, r)| selection_end_at(o.x, (span.x(r.start), span.x(r.end()))));
        match (origin.filter(|_| across), end_at) {
            (Some(_), Some(end)) => st.point_drag = Some(end),
            (Some(o), None) => st.selecting = Some(time_at(o.x)),
            (None, _) => {
                app.media_drag = Some(match sel {
                    Some(r) => MediaDrag::MediaPart(id, r),
                    None => MediaDrag::Media(id),
                });
            }
        }
    }
    let st = &mut app.trimmer;
    if let Some(p) = resp.interact_pointer_pos()
        && resp.dragged()
    {
        let t = time_at(p.x);
        if let Some(from) = st.selecting {
            st.in_point = Some(from.min(t));
            st.out_point = Some(from.max(t));
            st.selected = true;
            st.cursor = t;
        } else if let Some(end) = st.point_drag {
            let r = selection(st, show, still);
            let (a, b) = if end {
                (r.start, t.max(r.start + rate.frame_duration()))
            } else {
                (t.min(r.end() - rate.frame_duration()), r.end())
            };
            st.in_point = Some(a);
            st.out_point = Some(b);
            st.cursor = t;
        }
    }
    if resp.drag_stopped() {
        st.selecting = None;
        st.point_drag = None;
    }
    // An end of the selection under the pointer, or dragged: ↔, as VEGAS's
    // time selection's edges.
    let on_end = st.point_drag.is_some()
        || (st.selecting.is_none()
            && resp
                .hover_pos()
                .zip(active(st, show, still))
                .is_some_and(|(p, r)| {
                    selection_end_at(p.x, (span.x(r.start), span.x(r.end()))).is_some()
                }));
    if on_end {
        crate::cursors::set(ui.ctx(), crate::cursors::Cursor::ResizeEw);
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        st.cursor = time_at(p.x);
        st.selected = false;
    }
    if resp.double_clicked() {
        st.in_point = None;
        st.out_point = None;
        st.selected = true;
    }
    let hover_scrub = !app.settings.trimmer_no_hover_scrub;
    let st = &mut app.trimmer;
    if kind == Area::Picture {
        let hover = (hover_scrub && st.selecting.is_none() && st.point_drag.is_none())
            .then(|| resp.hover_pos())
            .flatten()
            .filter(|p| span.rect.contains(*p))
            .map(|p| time_at(p.x));
        if hover != st.hover {
            st.hover = hover;
            ui.ctx().request_repaint();
        }
    }
    let mut ops = Vec::new();
    resp.context_menu(|ui| {
        if let Some(op) = trimmer_menu(ui, app, show, kind) {
            ops.push(op);
            ui.close();
        }
    });
    ops
}

/// The end of the selection whose start and end are at `ends` (across)
/// within reach of `x`: whether it is its end (else its start).
fn selection_end_at(x: f32, ends: (f32, f32)) -> Option<bool> {
    [(false, ends.0), (true, ends.1)]
        .into_iter()
        .find(|(_, e)| (x - e).abs() <= 4.0)
        .map(|(end, _)| end)
}

/// The loop bar: the loop region (its handles dragged, the region moved, a
/// new one drawn on the bar; a double-click selects it) and the media's
/// markers and regions (numbered flags: a click moves the cursor there, a
/// drag moves them, a double-click names them, a right-click offers Rename
/// and Delete). Its right-clicks: the loop region's menu on it, the whole
/// one elsewhere.
#[allow(clippy::too_many_arguments)]
fn loop_strip(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    show: &Showing,
    strip: Rect,
    span: Span,
    rate: FrameRate,
    bottom: f32,
) -> Vec<TrimmerOp> {
    let info = &show.info;
    let id = info.id;
    let still = app.settings.prefs.new_events().still_length;
    let whole = Rect::from_min_max(strip.min, pos2(strip.right(), bottom));
    let painter = ui.painter().with_clip_rect(whole);
    painter.rect_filled(strip, 0.0, theme::panel());
    // The loop region, as VEGAS's: grey from its handles down (the time
    // bar's own grey runs on under them), a yellow handle at each end with
    // its pole.
    let region = selection(&app.trimmer, show, still);
    let (a, b) = (span.x(region.start).round(), span.x(region.end()).round());
    let top = strip.bottom() - 6.0;
    painter.rect_filled(
        Rect::from_min_max(pos2(a, top), pos2(b, strip.bottom())),
        0.0,
        REGION_GREY,
    );
    for (x, way) in [(a, 1.0), (b, -1.0)] {
        let x = x + 0.5;
        paint_pole(&painter, x, top + 5.0, bottom);
        painter.add(egui::Shape::convex_polygon(
            loop_handle(x, top, way),
            LOOP_YELLOW,
            Stroke::new(1.0, LOOP_EDGE),
        ));
    }
    // The markers and regions.
    let beats = app.beat_preview(show.file_media());
    paint_markers(&painter, info, span, strip, bottom, beats.as_deref(), show);

    let resp = ui.interact(
        strip,
        ui.id().with("trimmer-barra-loop"),
        Sense::click_and_drag(),
    );
    let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
    let to_x = |t: Tick| span.x(t);
    let hit = pointer.and_then(|p| marker_at(&info.markers, p.x, to_x));
    let on_handle = |x: f32| {
        if (x - a).abs() <= 5.0 {
            Some(LoopDrag::Start)
        } else if (x - b).abs() <= 5.0 {
            Some(LoopDrag::End)
        } else {
            None
        }
    };
    let in_band = |x: f32| x > a && x < b;
    let mut out = MarkersEdit::None;
    let mut ops = Vec::new();
    let st = &mut app.trimmer;
    if resp.drag_started() {
        let origin = ui.input(|i| i.pointer.press_origin());
        if let Some(o) = origin {
            let t = rate.snap_nearest(span.t(o.x));
            if let Some(m) = marker_at(&info.markers, o.x, to_x).filter(|_| show.own_markers()) {
                st.marker_drag = Some(m);
            } else if let Some(h) = on_handle(o.x) {
                st.loop_drag = Some(h);
            } else if in_band(o.x) {
                st.loop_drag = Some(LoopDrag::Move(t - region.start));
            } else {
                st.loop_drag = Some(LoopDrag::New(t));
            }
        }
    }
    if let Some(p) = pointer
        && (resp.dragged() || resp.drag_stopped())
    {
        let t = rate.snap_nearest(span.t(p.x));
        let done = resp.drag_stopped();
        if let Some((index, end)) = st.marker_drag {
            out = MarkersEdit::Drag {
                index,
                end,
                to: t,
                done,
            };
        } else if let Some(drag) = st.loop_drag {
            let frame = rate.frame_duration();
            let (from, to) = match drag {
                LoopDrag::Start => (t.min(region.end() - frame), region.end()),
                LoopDrag::End => (region.start, t.max(region.start + frame)),
                LoopDrag::Move(grab) => {
                    let start = (t - grab).max(Tick::ZERO);
                    (start, start + region.duration)
                }
                LoopDrag::New(from) => (from.min(t), from.max(t)),
            };
            if to > from {
                st.in_point = Some(from);
                st.out_point = Some(to);
                st.selected = true;
            }
        }
        if done {
            st.marker_drag = None;
            st.loop_drag = None;
        }
    }
    // The bar of markers and the loop region, as VEGAS's: the arrow with
    // ↔ over it and while something on it is dragged.
    if resp.hovered() || resp.dragged() {
        crate::cursors::set(ui.ctx(), crate::cursors::Cursor::ArrowLeftRight);
    }
    if resp.clicked()
        && let Some(p) = pointer
    {
        match hit {
            Some((i, end)) => {
                let m = &info.markers[i];
                st.cursor = if end { m.end() } else { m.time };
            }
            None if !in_band(p.x) => {
                st.cursor = rate.snap_floor(span.t(p.x));
                st.selected = false;
            }
            None => {}
        }
    }
    if resp.double_clicked() {
        match hit {
            Some((i, _)) if show.own_markers() => {
                st.renaming = Some((i, info.markers[i].name.clone()));
            }
            Some(_) => {}
            None => st.selected = true,
        }
    }
    if resp.secondary_clicked() {
        st.marker_menu = hit.map(|(i, _)| i).filter(|_| show.own_markers());
    }
    if let Some((i, _)) = hit
        && st.marker_drag.is_none()
    {
        let m = &info.markers[i];
        let at = |t: Tick| app.trimmer_time(info, t);
        let text = if m.is_region() {
            trf!("Region {} · {} – {}", i + 1, at(m.time), at(m.end()))
        } else {
            trf!("Marker {} · {}", i + 1, at(m.time))
        };
        let text = if m.name.is_empty() {
            text
        } else {
            format!("{}\n{text}", m.name)
        };
        resp.clone().on_hover_text(text);
    }
    let marker_menu = app.trimmer.marker_menu.filter(|&i| i < info.markers.len());
    // On the loop region (or a handle): its menu; elsewhere, the whole one.
    let menu_area = match resp.interact_pointer_pos().or(resp.hover_pos()) {
        Some(p) if on_handle(p.x).is_some() || in_band(p.x) => Area::LoopRegion,
        _ => Area::Bar,
    };
    if resp.secondary_clicked() {
        ui.data_mut(|d| d.insert_temp(Id::new("trimmer-menu-loop"), menu_area));
    }
    let area = ui
        .data(|d| d.get_temp::<Area>(Id::new("trimmer-menu-loop")))
        .unwrap_or(menu_area);
    resp.context_menu(|ui| {
        if let Some(i) = marker_menu {
            if ui.button(tr("Rename")).clicked() {
                app.trimmer.renaming = Some((i, info.markers[i].name.clone()));
                ui.close();
            }
            if ui.button(tr("Delete")).clicked() {
                out = MarkersEdit::Delete(i);
                ui.close();
            }
        } else if let Some(op) = trimmer_menu(ui, app, show, area) {
            ops.push(op);
            ui.close();
        }
    });
    rename_marker(ui, &mut app.trimmer, info, span, strip, &mut out);
    apply_markers_edit(app, id, out, rate, ui.id());
    ops
}

/// A loop region handle as VEGAS's: a yellow pennant hanging from `top`
/// at `x`, toward `way` (1 right, the start's; −1 left, the end's).
fn loop_handle(x: f32, top: f32, way: f32) -> Vec<egui::Pos2> {
    let points = vec![pos2(x, top), pos2(x + 6.0 * way, top), pos2(x, top + 6.0)];
    // egui's turning order, whichever way it points.
    if way > 0.0 {
        points
    } else {
        points.into_iter().rev().collect()
    }
}

/// A loop region handle's pole: a pale yellow line between dark ones.
fn paint_pole(painter: &egui::Painter, x: f32, top: f32, bottom: f32) {
    if bottom <= top {
        return;
    }
    for (dx, color) in [(-1.0, LOOP_EDGE), (0.0, LOOP_POLE), (1.0, LOOP_EDGE)] {
        painter.line_segment(
            [pos2(x + dx, top), pos2(x + dx, bottom)],
            Stroke::new(1.0, color),
        );
    }
}

/// The media's markers (VEGAS's orange numbered flags, their names after
/// them, their poles down to `bottom`) and regions (green flags at both
/// ends) on the loop bar; Beat Detection's beats while its sensitivity is
/// set.
fn paint_markers(
    painter: &egui::Painter,
    info: &MediaInfo,
    span: Span,
    strip: Rect,
    bottom: f32,
    beats: Option<&[Tick]>,
    show: &Showing,
) {
    use crate::marker_flag::{self as flag, Kind};
    let size = flag::TRIMMER;
    let top = strip.top() + 3.0;
    for (i, m) in info.markers.iter().enumerate() {
        let number = (i + 1).to_string();
        let x = span.x(m.time);
        let ends = if m.is_region() {
            vec![(x, Kind::RegionStart), (span.x(m.end()), Kind::RegionEnd)]
        } else {
            vec![(x, Kind::Marker)]
        };
        for (x, kind) in ends {
            flag::paint_pole(painter, x, top + size.height - 1.0, bottom, kind);
            let f = flag::paint(painter, x, top, &number, kind, size, false);
            if kind != Kind::RegionEnd {
                flag::paint_name(painter, f, &m.name, size, kind);
            }
        }
    }
    if let Some(beats) = beats {
        for &t in beats {
            let x = span.x(show_time(show, t));
            painter.line_segment(
                [pos2(x, top), pos2(x, bottom)],
                Stroke::new(1.0, flag::MARKER),
            );
        }
    }
}

/// What is shown's time of the file's time `f`.
fn show_time(show: &Showing, f: Tick) -> Tick {
    match show.part {
        Some((_, r, true)) => r.end() - f,
        Some((_, r, false)) => f - r.start,
        None => f,
    }
}

/// Naming a marker: a box over its flag; Enter keeps the name, Esc drops
/// it.
fn rename_marker(
    ui: &egui::Ui,
    st: &mut TrimmerState,
    info: &MediaInfo,
    span: Span,
    strip: Rect,
    out: &mut MarkersEdit,
) {
    let Some((i, text)) = &mut st.renaming else {
        return;
    };
    let Some(m) = info.markers.get(*i) else {
        st.renaming = None;
        return;
    };
    let at = pos2(
        span.x(m.time).min(strip.right() - 160.0),
        strip.top() - 26.0,
    );
    let mut done = None;
    egui::Area::new(ui.id().with("trimmer-nome-marcador"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let edit = ui.add(
                    egui::TextEdit::singleline(text)
                        .desired_width(150.0)
                        .hint_text(tr("Name")),
                );
                if !edit.has_focus() && !edit.lost_focus() {
                    edit.request_focus();
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    done = Some(false);
                } else if edit.lost_focus() {
                    done = Some(true);
                }
            });
        });
    match done {
        Some(true) => {
            *out = MarkersEdit::Rename {
                index: *i,
                name: text.trim().to_string(),
            };
            st.renaming = None;
        }
        Some(false) => st.renaming = None,
        None => {}
    }
}

/// What the loop bar asks of the markers.
enum MarkersEdit {
    None,
    /// A marker, or a region's start or end, dragged to `to`; `done` when
    /// let go.
    Drag {
        index: usize,
        end: bool,
        to: Tick,
        done: bool,
    },
    Rename {
        index: usize,
        name: String,
    },
    Delete(usize),
}

fn apply_markers_edit(
    app: &mut BananaApp,
    id: MediaId,
    edit: MarkersEdit,
    rate: FrameRate,
    salt: Id,
) {
    match edit {
        MarkersEdit::None => {}
        MarkersEdit::Drag {
            index,
            end,
            to,
            done,
        } => {
            let key = salt.with(("trimmer-marker", id, index));
            app.edit_media_markers(id, Some(key), !done, move |m| {
                if let Some(m) = m.get_mut(index) {
                    match (end, m.is_region()) {
                        (true, _) => m.length = (to - m.time).max(rate.frame_duration()),
                        (false, true) => {
                            let end = m.end();
                            m.time = to.min(end - rate.frame_duration());
                            m.length = end - m.time;
                        }
                        (false, false) => m.time = to,
                    }
                }
            });
        }
        MarkersEdit::Rename { index, name } => {
            app.edit_media_markers(id, None, false, move |m| {
                if let Some(m) = m.get_mut(index) {
                    m.name = name;
                }
            });
        }
        MarkersEdit::Delete(index) => {
            app.edit_media_markers(id, None, false, move |m| {
                if index < m.len() {
                    m.remove(index);
                }
            });
        }
    }
}

/// The time bar: two lines and their ticks (the media's frames along it
/// with Show Video Frames), the cursor's pin. A click puts the cursor there
/// (the selection let go), a drag scrubs it, a double-click selects all,
/// the wheel zooms around the pointer; a right-click gives the whole menu.
#[allow(clippy::too_many_arguments)]
fn time_bar(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    show: &Showing,
    bar: Rect,
    span: Span,
    rate: FrameRate,
    length: Tick,
    frames: bool,
) -> Vec<TrimmerOp> {
    let painter = ui.painter().with_clip_rect(bar);
    painter.rect_filled(bar, 0.0, theme::panel());
    if frames {
        film_strip(ui, &app.engine, &mut app.textures, show, bar, span.shown);
    } else {
        // VEGAS's: a dark line on top, a light one, a dark one, a light
        // one; the loop region's grey down over all but the dark middle
        // line; dots at round times beside the light lines.
        let still = app.settings.prefs.new_events().still_length;
        let region = selection(&app.trimmer, show, still);
        let (a, b) = (span.x(region.start).round(), span.x(region.end()).round());
        let y0 = bar.top().round();
        let line = |y: f32, color: Color32| {
            painter.line_segment(
                [
                    pos2(bar.left(), y0 + y + 0.5),
                    pos2(bar.right(), y0 + y + 0.5),
                ],
                Stroke::new(1.0, color),
            );
        };
        line(0.0, DARK_LINE);
        painter.rect_filled(
            Rect::from_min_max(pos2(a, y0), pos2(b, bar.bottom())),
            0.0,
            REGION_GREY,
        );
        line(4.0, LIGHT_LINE);
        line(8.0, DARK_LINE);
        line(12.0, LIGHT_LINE);
        let per_px = span.shown.duration.as_seconds_f64() / bar.width().max(1.0) as f64;
        let step = [
            0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0,
        ]
        .into_iter()
        .find(|s| s / per_px >= 70.0)
        .unwrap_or(3600.0);
        let first = (span.shown.start.as_seconds_f64() / step).ceil() as i64;
        let last = (span.shown.end().as_seconds_f64() / step).floor() as i64;
        for k in first..=last {
            let x = span.x(Tick::from_seconds_f64(k as f64 * step)).round();
            for y in [3.0, 5.0, 11.0, 13.0] {
                painter.rect_filled(
                    Rect::from_min_size(pos2(x, y0 + y), vec2(1.0, 1.0)),
                    0.0,
                    LIGHT_LINE,
                );
            }
        }
    }
    let resp = ui.interact(bar, ui.id().with("trimmer-barra"), Sense::click_and_drag());
    let st = &mut app.trimmer;
    if let Some(p) = resp.interact_pointer_pos()
        && (resp.clicked() || resp.dragged())
    {
        st.cursor = rate.snap_floor(span.t(p.x));
        if resp.clicked() {
            st.selected = false;
        }
    }
    if resp.double_clicked() {
        st.in_point = None;
        st.out_point = None;
        st.selected = true;
    }
    zoom_with_wheel(ui, st, &resp, span, rate, length);
    let mut ops = Vec::new();
    resp.context_menu(|ui| {
        if let Some(op) = trimmer_menu(ui, app, show, Area::Bar) {
            ops.push(op);
            ui.close();
        }
    });
    ops
}

/// The cursor in a bar from `top` to `bottom`, as VEGAS's: a white head
/// 7, 5 and 3 pixels wide under the bar's top, then a white line edged
/// dark.
fn paint_cursor(painter: &egui::Painter, x: f32, top: f32, bottom: f32) {
    let (x, top) = (x.round(), top.round());
    let white = theme::cursor();
    for (row, half) in [(1.0, 3.0), (2.0, 2.0), (3.0, 1.0)] {
        painter.rect_filled(
            Rect::from_min_max(
                pos2(x - half, top + row),
                pos2(x + half + 1.0, top + row + 1.0),
            ),
            0.0,
            white,
        );
    }
    let line = Rect::from_min_max(pos2(x, top + 4.0), pos2(x + 1.0, bottom));
    painter.rect_filled(line.expand2(vec2(1.0, 0.0)), 0.0, DARK_LINE);
    painter.rect_filled(line, 0.0, white);
}

/// The wheel over `resp` zooms the time bar around the pointer.
fn zoom_with_wheel(
    ui: &egui::Ui,
    st: &mut TrimmerState,
    resp: &egui::Response,
    span: Span,
    rate: FrameRate,
    length: Tick,
) {
    let wheel = ui.input(|i| i.smooth_scroll_delta.y);
    if !resp.hovered() || wheel == 0.0 {
        return;
    }
    let Some(p) = resp.hover_pos() else {
        return;
    };
    zoom_around(st, span.t(p.x), (-wheel as f64 / 200.0).exp(), rate, length);
}

/// The time bar's view `factor` times as long, `at` staying in place.
fn zoom_around(st: &mut TrimmerState, at: Tick, factor: f64, rate: FrameRate, length: Tick) {
    let shown = st.shown(length);
    let min = rate.frame_duration().0 * 10;
    let len = ((shown.duration.0 as f64 * factor) as i64).clamp(min, length.0.max(min));
    let f = (at - shown.start).0 as f64 / shown.duration.0.max(1) as f64;
    let start = (at.0 - (len as f64 * f) as i64).clamp(0, (length.0 - len).max(0));
    st.view = (len < length.0).then(|| TimeRange::new(Tick(start), Tick(len)));
}

/// A sound's ruler: its times in the Trimmer's format, ticks between them,
/// the cursor's pin; a right-click gives the time formats.
fn paint_ruler(ui: &mut egui::Ui, app: &mut BananaApp, info: &MediaInfo, ruler: Rect, span: Span) {
    let painter = ui.painter().with_clip_rect(ruler);
    painter.rect_filled(ruler, 0.0, theme::panel_dark());
    let font = egui::FontId::proportional(10.5);
    // Labels at least 110 points apart, on round times.
    let per_px = span.shown.duration.as_seconds_f64() / ruler.width().max(1.0) as f64;
    let steps = [
        0.01, 0.02, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0,
        600.0, 900.0, 1800.0, 3600.0,
    ];
    let step = steps
        .into_iter()
        .find(|s| s / per_px >= 110.0)
        .unwrap_or(7200.0);
    let minor = step / 10.0;
    // On the frames of the Trimmer's count, so labels read whole times.
    let rate = app.trimmer_format(info).1.frame_rate;
    let first = (span.shown.start.as_seconds_f64() / minor).floor() as i64;
    let last = (span.shown.end().as_seconds_f64() / minor).ceil() as i64;
    let ink = theme::text_dim();
    for k in first..=last {
        let t = rate.snap_nearest(Tick::from_seconds_f64(k as f64 * minor));
        let x = span.x(t).round() + 0.5;
        let major = k % 10 == 0;
        let h = if major { 9.0 } else { 4.0 };
        painter.line_segment(
            [pos2(x, ruler.bottom() - h), pos2(x, ruler.bottom())],
            Stroke::new(1.0, ink),
        );
        if major {
            painter.text(
                pos2(x + 2.0, ruler.top() + 1.0),
                egui::Align2::LEFT_TOP,
                app.trimmer_time(info, t),
                font.clone(),
                ink,
            );
        }
    }
    paint_cursor(
        &painter,
        span.x(app.trimmer.cursor),
        ruler.bottom() - 10.0,
        ruler.bottom(),
    );
    let resp = ui.interact(
        ruler,
        ui.id().with("trimmer-regua"),
        Sense::click_and_drag(),
    );
    if let Some(p) = resp.interact_pointer_pos()
        && (resp.clicked() || resp.dragged())
    {
        let rate = info.video.map_or(app.frame_rate(), |v| v.frame_rate);
        app.trimmer.cursor = rate.snap_floor(span.t(p.x));
    }
    resp.context_menu(|ui| format_menu(ui, app, info));
}

/// The ruler's menu (and the time boxes'): Use Media Frame Rate or Use
/// Project Frame Rate, then the time formats, as VEGAS's.
fn format_menu(ui: &mut egui::Ui, app: &mut BananaApp, info: &MediaInfo) {
    let (now, _) = app.trimmer_format(info);
    let project_rate = app.settings.trimmer_project_rate;
    let mut changed = false;
    for (label, on, value) in [
        (tr("Use Media Frame Rate"), !project_rate, false),
        (tr("Use Project Frame Rate"), project_rate, true),
    ] {
        if entry(ui, label, Mark::Radio(on), "", true) {
            app.settings.trimmer_project_rate = value;
            changed = true;
        }
    }
    ui.separator();
    for f in RulerFormat::ALL {
        // VEGAS's names in this list (its 29.97 ones without "Frame").
        let label = match f {
            RulerFormat::SmpteNonDrop | RulerFormat::SmpteDrop => f.dialog_label(),
            _ => f.label(),
        };
        if entry(
            ui,
            label,
            Mark::Radio(now == f),
            "",
            f != RulerFormat::AudioCd,
        ) {
            app.settings.trimmer_format = Some(f);
            changed = true;
        }
    }
    if changed {
        let _ = app.settings.save();
        ui.close();
    }
}

/// A sound's scroll bar: ◂ and ▸ move the view a tenth, its thumb drags
/// it; + and − zoom in and out.
fn scroll_bar(ui: &mut egui::Ui, app: &mut BananaApp, row: Rect, length: Tick) {
    let painter = ui.painter().clone();
    let btn = row.height();
    let left = Rect::from_min_size(row.min, vec2(btn, btn));
    let minus = Rect::from_min_size(pos2(row.right() - btn, row.top()), vec2(btn, btn));
    let plus = minus.translate(vec2(-btn - 2.0, 0.0));
    let right = plus.translate(vec2(-btn - 6.0, 0.0));
    let track = Rect::from_min_max(
        pos2(left.right() + 2.0, row.top()),
        pos2(right.left() - 2.0, row.bottom()),
    );
    painter.rect_filled(track, 2.0, theme::panel_dark());
    let st = &mut app.trimmer;
    let shown = st.shown(length);
    let total = length.0.max(1) as f32;
    let thumb = Rect::from_min_max(
        pos2(
            track.left() + track.width() * shown.start.0 as f32 / total,
            track.top() + 3.0,
        ),
        pos2(
            track.left() + track.width() * shown.end().0 as f32 / total,
            track.bottom() - 3.0,
        ),
    );
    painter.rect_filled(thumb, 2.0, theme::text_dim().gamma_multiply(0.5));
    let ink = theme::text();
    let arrow = |r: Rect, way: f32| {
        let c = r.center();
        painter.add(egui::Shape::convex_polygon(
            vec![
                c + vec2(3.0 * way, 0.0),
                c + vec2(-2.0 * way, -4.0),
                c + vec2(-2.0 * way, 4.0),
            ],
            ink,
            Stroke::NONE,
        ));
    };
    arrow(left, -1.0);
    arrow(right, 1.0);
    for (r, plus_sign) in [(plus, true), (minus, false)] {
        painter.rect_filled(r, 2.0, theme::panel_dark());
        let c = r.center();
        let s = Stroke::new(1.4, ink);
        painter.line_segment([c - vec2(4.0, 0.0), c + vec2(4.0, 0.0)], s);
        if plus_sign {
            painter.line_segment([c - vec2(0.0, 4.0), c + vec2(0.0, 4.0)], s);
        }
    }
    let rate = FrameRate::FPS_30;
    let id = ui.id();
    let click = |r: Rect, salt: &str| {
        ui.interact(r, id.with(("trimmer-rolagem", salt)), Sense::click())
            .clicked()
    };
    let step = Tick(shown.duration.0 / 10);
    let pan = |st: &mut TrimmerState, by: i64| {
        let start = (shown.start.0 + by).clamp(0, (length.0 - shown.duration.0).max(0));
        st.view = (shown.duration < length).then(|| TimeRange::new(Tick(start), shown.duration));
    };
    if click(left, "esquerda") {
        pan(st, -step.0);
    }
    if click(right, "direita") {
        pan(st, step.0);
    }
    if click(plus, "mais") {
        let at = st.cursor.clamp(shown.start, shown.end());
        zoom_around(st, at, 0.5, rate, length);
    }
    if click(minus, "menos") {
        let at = st.cursor.clamp(shown.start, shown.end());
        zoom_around(st, at, 2.0, rate, length);
    }
    let drag = ui.interact(
        thumb.expand2(vec2(0.0, 3.0)),
        id.with("trimmer-polegar"),
        Sense::drag(),
    );
    if drag.dragged() {
        let by = (drag.drag_delta().x / track.width().max(1.0) * total) as i64;
        pan(st, by);
    }
}

/// The transport, in the middle: the buttons Edit Visible Button Set
/// ticks, More with the others and Edit Visible Button Set.
fn transport_row(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    show: &Showing,
    row: Rect,
) -> Vec<TrimmerOp> {
    let shown_buttons = app.transport_shown(Bar::Trimmer);
    let hidden = app.transport_hidden(Bar::Trimmer);
    let size = vec2(24.0, 24.0);
    let gap = 4.0;
    let count = shown_buttons.len() as f32 + 1.0;
    let width = count * size.x + (count - 1.0) * gap;
    let mut x = row.center().x - width / 2.0;
    let y = row.center().y - size.y / 2.0;
    let mut ops = Vec::new();
    let mut loop_toggle = false;
    for b in &shown_buttons {
        let r = Rect::from_min_size(pos2(x, y), size);
        x += size.x + gap;
        let (enabled, on) = button_state(app, show, *b);
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let resp = ui.interact(r, ui.id().with(("trimmer-transporte", *b as u8)), sense);
        let painter = ui.painter();
        if on {
            painter.rect_filled(r, 3.0, theme::button_highlight());
        } else if resp.hovered() && enabled {
            painter.rect_filled(r, 3.0, theme::hover());
        }
        paint_trimmer_icon(painter, r.shrink(4.0), *b, enabled);
        let keys = button_keys(app, *b);
        let tip = if keys.is_empty() {
            b.name().to_string()
        } else {
            format!("{} ({keys})", b.name())
        };
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, b.name()));
        if resp.on_hover_text(tip).clicked() {
            match button_op(*b) {
                Some(op) => ops.push(op),
                None => loop_toggle = true,
            }
        }
    }
    let more_rect = Rect::from_min_size(pos2(x, y), size);
    app.trimmer.drawn[3] = Some(more_rect);
    let more = ui.interact(more_rect, ui.id().with("trimmer-mais"), Sense::click());
    if more.hovered() {
        ui.painter().rect_filled(more_rect, 3.0, theme::hover());
    }
    crate::icons::paint(
        ui.painter(),
        more_rect.shrink(5.0),
        Icon::More,
        theme::text(),
    );
    more.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tr("More")));
    let more = more.on_hover_text(tr("More"));
    let mut edit_buttons = false;
    egui::Popup::menu(&more).show(|ui| {
        for b in &hidden {
            let (enabled, on) = button_state(app, show, *b);
            let keys = button_keys(app, *b);
            let mark = match b {
                B::Overwrite => Mark::Check(on),
                B::Loop => Mark::Check(on),
                _ => Mark::Button(*b),
            };
            if entry(ui, b.name(), mark, &keys, enabled) {
                match button_op(*b) {
                    Some(op) => ops.push(op),
                    None => loop_toggle = true,
                }
                ui.close();
            }
        }
        if !hidden.is_empty() {
            ui.separator();
        }
        if entry(ui, tr("Edit Visible Button Set…"), Mark::None, "", true) {
            edit_buttons = true;
            ui.close();
        }
    });
    if loop_toggle {
        crate::commands::Cmd::LoopPlayback.run(app);
    }
    if edit_buttons {
        app.open_transport_buttons(Bar::Trimmer);
    }
    ops
}

/// What a button of the Trimmer's bar does (Loop Playback: `None`, the
/// project's switch).
fn button_op(b: B) -> Option<TrimmerOp> {
    Some(match b {
        B::Loop => return None,
        B::PlayFromStart => TrimmerOp::PlayFromStart,
        B::Play => TrimmerOp::Play,
        B::Pause => TrimmerOp::Pause,
        B::Stop => TrimmerOp::Stop,
        B::GoStart => TrimmerOp::Start,
        B::GoEnd => TrimmerOp::End,
        B::PrevFrame => TrimmerOp::Frame(-1),
        B::NextFrame => TrimmerOp::Frame(1),
        B::AddFromCursor => TrimmerOp::AddFromCursor,
        B::FitToFill => TrimmerOp::FitToFill,
        B::Overwrite => TrimmerOp::Overwrite,
        B::AddToCursor => TrimmerOp::AddToCursor,
        B::CreateSubclip => TrimmerOp::CreateSubclip,
        B::SetIn => TrimmerOp::MarkIn,
        B::SetOut => TrimmerOp::MarkOut,
        B::InsertMarker => TrimmerOp::InsertMarker,
        B::InsertRegion => TrimmerOp::InsertRegion,
        B::SaveMarkers => TrimmerOp::SaveMarkers,
        B::DetectScenes => TrimmerOp::DetectScenes,
        B::BeatDetection => TrimmerOp::BeatDetection,
    })
}

/// The keys of a button in the Trimmer's keyboard context.
fn button_keys(app: &BananaApp, b: B) -> String {
    let action = match button_op(b) {
        Some(op) => Action::Extra(Extra::Trimmer(op)),
        None => Action::Cmd(crate::commands::Cmd::LoopPlayback),
    };
    app.keymap.text_in(action, Context::Trimmer)
}

/// Whether a button of the Trimmer's bar can be pressed now, and whether
/// it is lit.
fn button_state(app: &BananaApp, show: &Showing, b: B) -> (bool, bool) {
    let info = &show.info;
    let still = app.settings.prefs.new_events().still_length;
    let timeline_selection = app
        .project()
        .loop_region
        .is_some_and(|r| r.duration > Tick::ZERO);
    let playing = app
        .previews
        .playing
        .as_ref()
        .is_some_and(|p| p.media == Some(info.id));
    match b {
        B::Loop => (true, app.loop_enabled),
        B::Play => (true, playing),
        B::FitToFill => (timeline_selection, false),
        B::AddToCursor => (active(&app.trimmer, show, still).is_some(), false),
        B::Overwrite => (true, app.trimmer.overwrite),
        B::SaveMarkers => (show.own_markers() && can_save(info), false),
        B::InsertMarker | B::InsertRegion => (show.own_markers() || can_save(info), false),
        B::BeatDetection => (info.audio.is_some(), false),
        B::DetectScenes => (info.video.is_some() && !info.still, false),
        _ => (true, false),
    }
}

/// The time boxes, in the middle under the transport, as the timeline's:
/// with a time selection its start, end and length; otherwise where the
/// cursor is, the end's icon alone and the loop region's length dimmed. A
/// double-click types into a lit one; a right-click gives the time
/// formats.
fn time_boxes(ui: &mut egui::Ui, app: &mut BananaApp, show: &Showing, row: Rect) {
    let info = &show.info;
    let still = app.settings.prefs.new_events().still_length;
    let st = &app.trimmer;
    let region = selection(st, show, still);
    let boxes = match active(st, show, still) {
        Some(r) => [
            (TimeBox::Start, Some(app.trimmer_time(info, r.start)), true),
            (TimeBox::End, Some(app.trimmer_time(info, r.end())), true),
            (
                TimeBox::Length,
                Some(app.trimmer_time(info, r.duration)),
                true,
            ),
        ],
        None => [
            (
                TimeBox::Cursor,
                Some(app.trimmer_time(info, st.cursor)),
                true,
            ),
            (TimeBox::End, None, false),
            (
                TimeBox::Length,
                Some(app.trimmer_time(info, region.duration)),
                false,
            ),
        ],
    };
    let painter = ui.painter().clone();
    painter.line_segment(
        [row.left_top(), row.right_top()],
        Stroke::new(1.0, theme::panel_dark()),
    );
    let font = egui::FontId::proportional(12.5);
    let measure = |t: &str| {
        painter
            .layout_no_wrap(t.to_string(), font.clone(), theme::text())
            .size()
            .x
    };
    let widest = boxes
        .iter()
        .filter_map(|b| b.1.as_deref())
        .map(measure)
        .fold(
            measure(&app.trimmer_time(info, Tick::from_seconds(36_000))),
            f32::max,
        );
    let slot = 14.0 + 8.0 + widest.ceil();
    let space = 34.0;
    let first = row.center().x - (3.0 * slot + 2.0 * space) / 2.0;
    let cy = row.center().y + 1.0;
    let mut typed = None;
    for (i, (kind, text, lit)) in boxes.into_iter().enumerate() {
        let x = first + i as f32 * (slot + space);
        let icon = Rect::from_center_size(pos2(x + 7.0, cy), vec2(14.0, 14.0));
        paint_box_icon_in(&painter, icon, kind, LOOP_YELLOW);
        let field = Rect::from_min_max(
            pos2(icon.right() + 8.0, cy - 10.0),
            pos2(x + slot, cy + 10.0),
        );
        if let Some((editing, text)) = &mut app.trimmer.typing
            && *editing == kind
        {
            let resp = ui.put(
                field.expand2(vec2(3.0, 0.0)),
                egui::TextEdit::singleline(text)
                    .font(font.clone())
                    .margin(vec2(2.0, 1.0)),
            );
            if !resp.has_focus() && !resp.lost_focus() {
                resp.request_focus();
            }
            let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if resp.lost_focus() || cancel {
                if !cancel {
                    typed = Some((kind, text.clone()));
                }
                app.trimmer.typing = None;
            }
            continue;
        }
        if let Some(text) = &text {
            let color = if lit {
                theme::text()
            } else {
                theme::text_disabled()
            };
            painter.text(
                field.left_center(),
                egui::Align2::LEFT_CENTER,
                text,
                font.clone(),
                color,
            );
        }
        let area = Rect::from_min_max(pos2(icon.left(), field.top()), field.max);
        let resp = ui.interact(area, ui.id().with(("trimmer-caixa", i)), Sense::click());
        if lit && resp.double_clicked() {
            app.trimmer.typing = Some((kind, text.unwrap_or_default()));
        }
        let name = match kind {
            TimeBox::Cursor => tr("Cursor Position"),
            TimeBox::Start => tr("Selection Start"),
            TimeBox::End => tr("Selection End"),
            TimeBox::Length => tr("Selection Length"),
        };
        let resp = if lit {
            resp.on_hover_text(trf!("{} · double-click to type it", name))
        } else {
            resp.on_hover_text(name)
        };
        resp.context_menu(|ui| format_menu(ui, app, info));
    }
    if let Some((kind, text)) = typed {
        type_into(app, show, kind, &text);
    }
}

/// What a time box typed into sets: the cursor, or the selection's start,
/// end or length (the selection then on).
fn type_into(app: &mut BananaApp, show: &Showing, kind: TimeBox, text: &str) {
    let (format, settings) = app.trimmer_format(&show.info);
    let rate = settings.frame_rate;
    let Some(t) = format
        .parse(text, &settings)
        .or_else(|| bv_core::timecode::parse(text, rate))
    else {
        return;
    };
    let still = app.settings.prefs.new_events().still_length;
    let st = &mut app.trimmer;
    let r = selection(st, show, still);
    match kind {
        TimeBox::Cursor => st.cursor = t,
        TimeBox::Start => {
            st.in_point = Some(t.min(r.end()));
            st.out_point = Some(r.end().max(t));
            st.selected = true;
        }
        TimeBox::End => {
            st.in_point = Some(r.start.min(t));
            st.out_point = Some(t.max(r.start));
            st.selected = true;
        }
        TimeBox::Length => {
            st.in_point = Some(r.start);
            st.out_point = Some(r.start + t);
            st.selected = true;
        }
    }
}

/// A menu item's mark in the icons' column.
#[derive(Clone, Copy)]
enum Mark {
    None,
    /// A Trimmer button's icon.
    Button(B),
    /// VEGAS's check mark while on (nothing off).
    Check(bool),
    /// VEGAS's dot while chosen.
    Radio(bool),
}

/// A menu item with its mark and keys, greyed unless `enabled`; clicked or
/// not.
fn entry(ui: &mut egui::Ui, label: &str, mark: Mark, keys: &str, enabled: bool) -> bool {
    let id = ui.make_persistent_id(("trimmer-menu", label));
    let button = egui::Button::new((Atom::custom(id, vec2(16.0, 16.0)), label)).shortcut_text(keys);
    let resp = ui.add_enabled_ui(enabled, |ui| button.atom_ui(ui)).inner;
    if let Some(r) = resp.rect(id) {
        let ink = if enabled {
            theme::text()
        } else {
            theme::text_disabled()
        };
        let painter = ui.painter();
        match mark {
            Mark::None => {}
            Mark::Button(b) => paint_trimmer_icon(painter, r, b, enabled),
            Mark::Check(on) => {
                if on {
                    let c = r.center();
                    painter.add(egui::Shape::line(
                        vec![
                            pos2(c.x - 4.5, c.y),
                            pos2(c.x - 1.5, c.y + 3.0),
                            pos2(c.x + 4.5, c.y - 3.5),
                        ],
                        Stroke::new(1.8, ink),
                    ));
                }
            }
            Mark::Radio(on) => {
                if on {
                    painter.circle_filled(r.center(), 4.0, ink);
                }
            }
        }
    }
    resp.response.clicked()
}

/// The Trimmer's menus, as VEGAS 23's: the picture's, the waveform's, the
/// bar's (everything: a right-click on the time bar or on the loop bar out
/// of the loop region) and the loop region's. The command chosen, unless
/// the menu did it itself (its switches).
fn trimmer_menu(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    show: &Showing,
    area: Area,
) -> Option<TrimmerOp> {
    let info = &show.info;
    let still = app.settings.prefs.new_events().still_length;
    let keys = |app: &BananaApp, op: TrimmerOp| {
        app.keymap
            .text_in(Action::Extra(Extra::Trimmer(op)), Context::Trimmer)
    };
    let selected = active(&app.trimmer, show, still).is_some();
    let picture = info.video.is_some() && !info.still;
    let sound = info.audio.is_some();
    let file = matches!(info.source, MediaSource::File { .. });
    let timeline_selection = app
        .project()
        .loop_region
        .is_some_and(|r| r.duration > Tick::ZERO);
    let mut op = None;
    if area == Area::LoopRegion {
        if entry(
            ui,
            tr("Insert Region"),
            Mark::Button(B::InsertRegion),
            &keys(app, TrimmerOp::InsertRegion),
            true,
        ) {
            op = Some(TrimmerOp::InsertRegion);
        }
        ui.separator();
        let loop_keys = app.keymap.text_in(
            Action::Cmd(crate::commands::Cmd::LoopPlayback),
            Context::Trimmer,
        );
        if entry(
            ui,
            tr("Loop Playback"),
            Mark::Check(app.loop_enabled),
            &loop_keys,
            true,
        ) {
            crate::commands::Cmd::LoopPlayback.run(app);
            ui.close();
        }
        ui.separator();
        if entry(ui, tr("Zoom Selection"), Mark::None, "", true) {
            op = Some(TrimmerOp::ZoomSelection);
        }
        if entry(ui, tr("Set Selection to View"), Mark::None, "", true) {
            op = Some(TrimmerOp::SelectView);
        }
        if entry(
            ui,
            tr("Select Loop Region"),
            Mark::None,
            &keys(app, TrimmerOp::SelectLoop),
            true,
        ) {
            op = Some(TrimmerOp::SelectLoop);
        }
        return op;
    }
    let audio_items = area == Area::Waveform || (area == Area::Bar && sound);
    if audio_items
        && entry(
            ui,
            tr("Open in Audio Editor"),
            Mark::None,
            &keys(app, TrimmerOp::OpenInAudioEditor),
            file && sound,
        )
    {
        op = Some(TrimmerOp::OpenInAudioEditor);
    }
    if entry(ui, tr("Copy"), Mark::None, "", selected) {
        op = Some(TrimmerOp::Copy);
    }
    if entry(
        ui,
        tr("Copy Snapshot to Clipboard"),
        Mark::None,
        "",
        picture,
    ) {
        if let Some(f) = app.engine.trimmer_frame() {
            ui.ctx().copy_image(ColorImage::from_rgba_unmultiplied(
                [f.width as usize, f.height as usize],
                &f.rgba,
            ));
        }
        ui.close();
    }
    if entry(ui, tr("Save Snapshot to File…"), Mark::None, "", picture) {
        op = Some(TrimmerOp::SaveSnapshot);
    }
    ui.separator();
    if area != Area::Waveform && info.video.is_some() && sound {
        for (s, label, step) in [
            (Streams::VideoAndAudio, tr("Select Video and Audio"), None),
            (Streams::VideoOnly, tr("Select Video Only"), Some(1)),
            (Streams::AudioOnly, tr("Select Audio Only"), Some(-1)),
        ] {
            let k = step.map_or(String::new(), |d| keys(app, TrimmerOp::CycleStreams(d)));
            if entry(ui, label, Mark::Radio(app.trimmer.streams == s), &k, true) {
                op = Some(TrimmerOp::Select(s));
            }
        }
        ui.separator();
    }
    let own = show.own_markers();
    crate::menus::sub(ui, tr("Markers/Regions"), true, |ui| {
        for (b, o) in [
            (B::InsertMarker, TrimmerOp::InsertMarker),
            (B::InsertRegion, TrimmerOp::InsertRegion),
            (B::SaveMarkers, TrimmerOp::SaveMarkers),
        ] {
            let enabled = o != TrimmerOp::SaveMarkers || (own && can_save(info));
            if entry(ui, b.name(), Mark::Button(b), &keys(app, o), enabled) {
                op = Some(o);
                ui.close();
            }
        }
        let auto = app.settings.prefs.save_trimmer_markers;
        if entry(
            ui,
            tr("Auto-Save Trimmer Markers/Regions"),
            Mark::Check(auto),
            &keys(app, TrimmerOp::AutoSaveMarkers),
            true,
        ) {
            op = Some(TrimmerOp::AutoSaveMarkers);
            ui.close();
        }
    });
    if entry(
        ui,
        tr("Beat Detection"),
        Mark::Button(B::BeatDetection),
        &keys(app, TrimmerOp::BeatDetection),
        sound,
    ) {
        op = Some(TrimmerOp::BeatDetection);
    }
    ui.separator();
    if entry(
        ui,
        tr("Sync Timeline Selection Time from Cursor"),
        Mark::None,
        &keys(app, TrimmerOp::SyncSelection { up_to: false }),
        timeline_selection,
    ) {
        op = Some(TrimmerOp::SyncSelection { up_to: false });
    }
    if entry(
        ui,
        tr("Sync Timeline Selection Time up to Cursor"),
        Mark::None,
        &keys(app, TrimmerOp::SyncSelection { up_to: true }),
        timeline_selection,
    ) {
        op = Some(TrimmerOp::SyncSelection { up_to: true });
    }
    if entry(
        ui,
        tr("Zoom Selection"),
        Mark::None,
        &keys(app, TrimmerOp::ZoomSelection),
        selected && area != Area::Picture,
    ) {
        op = Some(TrimmerOp::ZoomSelection);
    }
    ui.separator();
    // Stream: the video's on the picture, else the sound's.
    let streams = if area == Area::Picture {
        1
    } else {
        info.audio.map_or(1, |_| 1 + info.extra_audio.len())
    };
    let chosen = if area == Area::Picture {
        0
    } else {
        app.trimmer.audio_stream
    };
    crate::menus::sub(ui, tr("Stream"), true, |ui| {
        for k in 0..streams {
            if entry(
                ui,
                &trf!("Stream {}", k + 1),
                Mark::Radio(chosen == k),
                "",
                true,
            ) {
                if area != Area::Picture {
                    op = Some(TrimmerOp::AudioStream(k as u8));
                }
                ui.close();
            }
        }
    });
    if audio_items && let Some(a) = info.audio_stream(app.trimmer.audio_stream as u16) {
        let pairs = channel_pairs(a.channels);
        crate::menus::sub(ui, tr("Channels"), true, |ui| {
            for (k, label) in pairs.iter().enumerate() {
                if entry(
                    ui,
                    label,
                    Mark::Radio(app.trimmer.channel_pair == k),
                    "",
                    true,
                ) {
                    op = Some(TrimmerOp::ChannelPair(k as u8));
                    ui.close();
                }
            }
        });
    }
    let all = !app.settings.trimmer_shown_streams_only;
    if entry(
        ui,
        tr("Use All Streams and Channels"),
        Mark::Check(all),
        "",
        true,
    ) {
        op = Some(TrimmerOp::UseAllStreams);
    }
    ui.separator();
    if entry(
        ui,
        tr("Enable Overwrite of Timeline Events"),
        Mark::Check(app.trimmer.overwrite),
        &keys(app, TrimmerOp::Overwrite),
        true,
    ) {
        op = Some(TrimmerOp::Overwrite);
    }
    if entry(
        ui,
        tr("Add Media from Cursor"),
        Mark::Button(B::AddFromCursor),
        &keys(app, TrimmerOp::AddFromCursor),
        true,
    ) {
        op = Some(TrimmerOp::AddFromCursor);
    }
    if entry(
        ui,
        tr("Add Media up to Cursor"),
        Mark::Button(B::AddToCursor),
        &keys(app, TrimmerOp::AddToCursor),
        selected,
    ) {
        op = Some(TrimmerOp::AddToCursor);
    }
    if entry(
        ui,
        tr("Fit Media to Fill Selection"),
        Mark::Button(B::FitToFill),
        &keys(app, TrimmerOp::FitToFill),
        timeline_selection,
    ) {
        op = Some(TrimmerOp::FitToFill);
    }
    if entry(
        ui,
        tr("Create Subclip…"),
        Mark::Button(B::CreateSubclip),
        &keys(app, TrimmerOp::CreateSubclip),
        true,
    ) {
        op = Some(TrimmerOp::CreateSubclip);
    }
    if entry(
        ui,
        tr("Select Parent Media"),
        Mark::Button(B::CreateSubclip),
        "",
        show.part.is_some(),
    ) {
        op = Some(TrimmerOp::SelectParentMedia);
    }
    ui.separator();
    // VEGAS's switches of the picture.
    let s = &app.settings;
    let monitor = !s.trimmer_no_monitor;
    let window = !s.trimmer_no_preview_window;
    let frames = s.trimmer_frames;
    let hover = !s.trimmer_no_hover_scrub;
    let mut changed = false;
    if entry(ui, tr("Show Video Monitor"), Mark::Check(monitor), "", true) {
        app.settings.trimmer_no_monitor = monitor;
        changed = true;
    }
    if entry(
        ui,
        tr("Show Video Frames"),
        Mark::Check(frames),
        "",
        info.video.is_some(),
    ) {
        app.settings.trimmer_frames = !frames;
        changed = true;
    }
    if entry(
        ui,
        tr("Show Video in Preview Window"),
        Mark::Check(window),
        &keys(app, TrimmerOp::TogglePreviewWindow),
        !monitor && info.video.is_some(),
    ) {
        op = Some(TrimmerOp::TogglePreviewWindow);
    }
    if entry(
        ui,
        tr("Trimmer on External Monitor"),
        Mark::Check(app.trimmer.external),
        "",
        true,
    ) {
        op = Some(TrimmerOp::ExternalMonitor);
    }
    if entry(ui, tr("Enable Hover Scrub"), Mark::Check(hover), "", true) {
        app.settings.trimmer_no_hover_scrub = hover;
        changed = true;
    }
    if changed {
        let _ = app.settings.save();
        ui.close();
    }
    op
}

/// The Channels menu's pairs of a sound with `channels`: "Channel 1"
/// alone, else "Channels 1/2", "Channels 3/4"…
fn channel_pairs(channels: u16) -> Vec<String> {
    if channels <= 1 {
        return vec![tr("Channel 1").to_string()];
    }
    (0..channels.div_ceil(2))
        .map(|k| {
            let a = 2 * k + 1;
            if a < channels {
                trf!("Channels {}/{}", a, a + 1)
            } else {
                trf!("Channel {}", a)
            }
        })
        .collect()
}

/// Show Video Frames: the media's frames side by side along the time bar
/// `bar`, at their times in the part `shown` (read in the background).
fn film_strip(
    ui: &egui::Ui,
    engine: &bv_engine::Engine,
    textures: &mut crate::tabs::TextureCache,
    show: &Showing,
    bar: Rect,
    shown: TimeRange,
) {
    let info = &show.info;
    let Some(v) = info.video else {
        return;
    };
    let h = bar.height() - 2.0;
    let w = (h * v.width as f32 / v.height.max(1) as f32).max(8.0);
    let ppp = ui.ctx().pixels_per_point();
    let painter = ui.painter().with_clip_rect(bar);
    let mut x = bar.left();
    while x < bar.right() {
        let f = ((x + w / 2.0 - bar.left()) / bar.width()).clamp(0.0, 1.0) as f64;
        let t = shown.start + Tick((shown.duration.0 as f64 * f) as i64);
        let r = Rect::from_min_size(pos2(x, bar.top() + 1.0), vec2(w - 1.0, h));
        match engine.thumbnail(info, show.file_time(t), (h * ppp) as u32) {
            Some(th) => {
                let tex = textures.get(ui.ctx(), &th);
                painter.image(
                    tex,
                    r,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::from_white_alpha(200),
                );
            }
            None => {
                painter.rect_filled(r, 0.0, Color32::from_gray(40));
            }
        }
        x += w;
    }
}

/// The Trimmer's picture in the Video Preview window (its own monitor off,
/// Show Video in Preview Window on): media `info` at `at`, fitted in
/// `area`.
pub(crate) fn paint_in_preview(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    info: &MediaInfo,
    at: Tick,
    area: Rect,
) {
    let ppp = ui.ctx().pixels_per_point();
    app.engine.request_trimmer(
        info,
        at,
        (area.width() * ppp) as u32,
        (area.height() * ppp) as u32,
    );
    take_frame(ui, &mut app.trimmer, &app.engine);
    let painter = ui.painter();
    painter.rect_filled(area, 0.0, Color32::BLACK);
    if let (Some(v), Some(tex)) = (info.video, &app.trimmer.texture) {
        let aspect = v.width as f32 / v.height.max(1) as f32;
        let mut size = area.size();
        if size.x / size.y > aspect {
            size.x = size.y * aspect;
        } else {
            size.y = size.x / aspect;
        }
        painter.image(
            tex.id(),
            Rect::from_center_size(area.center(), size),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}

impl BananaApp {
    /// The demo's first media (color bars) in the Trimmer, its frames along
    /// the time bar, a part selected; the Mixing Console with the Preview
    /// Bus's channel (screen tests; the settings in memory).
    pub fn trimmer_frames_for_review(&mut self) -> bool {
        let Some(id) = self.project().media.first().map(|m| m.id) else {
            return false;
        };
        self.trimmer.open(id);
        self.trimmer.in_point = Some(Tick::from_seconds(12));
        self.trimmer.out_point = Some(Tick::from_seconds(30));
        self.trimmer.selected = true;
        self.trimmer.cursor = Tick::from_seconds(20);
        self.settings.trimmer_frames = true;
        self.console.show_preview = true;
        self.show_tab_named("Trimmer");
        true
    }

    /// What the Video Preview window shows of the Trimmer, if it shows it:
    /// with the Trimmer's own monitor off and Show Video in Preview Window
    /// on, while the Trimmer has the focus — the file previewing, else its
    /// media at the hover's time or the cursor's (a subclip: its parent's
    /// file at the part's time).
    pub(crate) fn trimmer_in_preview(&self) -> Option<(MediaInfo, Tick)> {
        let s = &self.settings;
        if !s.trimmer_no_monitor
            || s.trimmer_no_preview_window
            || self.keys_tab != Some(crate::dock::Tab::Trimmer)
        {
            return None;
        }
        self.trimmer_picture()
    }

    /// The Trimmer's picture: the file previewing, else its media at the
    /// hover's time or the cursor's (Show Video in Preview Window, and the
    /// external monitor with Trimmer on External Monitor).
    pub(crate) fn trimmer_picture(&self) -> Option<(MediaInfo, Tick)> {
        if let Some(p) = &self.previews.playing
            && p.media.is_none()
            && p.info.video.is_some()
        {
            let at = self.engine.media_preview_position().unwrap_or(Tick::ZERO);
            return Some((p.info.clone(), at));
        }
        let show = self.trimmer_showing()?;
        show.info.video?;
        let t = show.file_time(self.trimmer.hover.unwrap_or(self.trimmer.cursor));
        Some((show.info, t))
    }

    /// Plays what the Trimmer shows from `from` (its time) through the
    /// preview bus; with Loop Playback on and the cursor out of the loop
    /// region, from the region's start.
    fn trimmer_play(&mut self, from: Tick) {
        let Some(show) = self.trimmer_showing() else {
            return;
        };
        let from = match (self.trimmer.in_point, self.trimmer.out_point) {
            (Some(a), Some(b)) if self.loop_enabled && b > a && !(a..b).contains(&from) => a,
            _ => from,
        };
        self.trimmer.cursor = from;
        let Some(file) = self.project().media(show.file_media()).cloned() else {
            return;
        };
        self.start_preview(file, Some(show.info.id), show.file_time(from));
    }

    /// Whether what the Trimmer shows is playing.
    pub(crate) fn trimmer_playing(&self) -> bool {
        let id = self.trimmer.media;
        self.previews
            .playing
            .as_ref()
            .is_some_and(|p| p.media.is_some() && p.media == id)
    }
}

/// Loop Playback: the preview of what the Trimmer shows played again from
/// `t` (its time).
pub(crate) fn play_again_from(app: &mut BananaApp, t: Tick) {
    app.engine.stop_media_preview();
    app.previews.playing = None;
    app.trimmer_play(t);
}

fn file_preview(ui: &mut egui::Ui, app: &mut BananaApp, info: &MediaInfo) {
    let full = ui.available_rect_before_wrap();
    let area = Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - 22.0));
    let t = app.engine.media_preview_position().unwrap_or(Tick::ZERO);
    paint_in_preview(ui, app, info, t, area);
    ui.painter().text(
        pos2(area.left() + 4.0, area.bottom() + 11.0),
        egui::Align2::LEFT_CENTER,
        trf!("Previewing {}", crate::i18n::media_name(info)),
        egui::FontId::proportional(12.0),
        theme::text_dim(),
    );
    ui.ctx().request_repaint();
    ui.allocate_rect(full, Sense::hover());
}

/// The Trimmer's part `range` of what it shows added at `at`: its streams
/// on the tracks (only those it shows with Use All Streams and Channels
/// off), stretched by `stretch` (Fit to Fill), the time it takes
/// overwritten on its tracks with Enable Timeline Overwrite — one undo.
fn add_from_trimmer(
    app: &mut BananaApp,
    id: MediaId,
    at: Tick,
    range: TimeRange,
    streams: Streams,
    stretch: Option<f64>,
) {
    let (video, audio) = match streams {
        Streams::VideoAndAudio => (true, true),
        Streams::VideoOnly => (true, false),
        Streams::AudioOnly => (false, true),
    };
    let track = app.selected_track;
    let edits = app.history.edits();
    app.place_media_streams(id, at, Some(range), track, video, audio);
    if app.history.edits() == edits {
        return;
    }
    // The new events: the placement selected them.
    let ids: Vec<bv_timeline::ClipId> = app.selection.iter().copied().collect();
    let overwrite = app.trimmer.overwrite;
    let shown_only = app
        .settings
        .trimmer_shown_streams_only
        .then_some(app.trimmer.audio_stream);
    app.history.amend(|p| {
        // Only the stream shown: the others' events go.
        let mut ids = ids.clone();
        if let Some(stream) = shown_only {
            let others: Vec<bv_timeline::ClipId> = ids
                .iter()
                .copied()
                .filter(|&c| {
                    p.clip_track(c).is_some_and(|t| t.kind == TrackKind::Audio)
                        && p.clip(c).is_some_and(|c| c.audio_stream as usize != stream)
                })
                .collect();
            if !others.is_empty() {
                let _ = p.delete_events(&others, false, false);
                ids.retain(|c| !others.contains(c));
            }
        }
        if let (Some(rate), Some(&first)) = (stretch, ids.first()) {
            let _ = p.set_clip_rate(first, rate);
        }
        if overwrite {
            let _ = p.overwrite_under(&ids);
        }
    });
}

/// The waveform of the media's sound (its stream shown), over the part the
/// time bar shows: a lane per channel of the pair shown (all of a mono or
/// stereo sound).
fn paint_waveform(ui: &egui::Ui, app: &BananaApp, show: &Showing, area: Rect, shown: TimeRange) {
    let info = &show.info;
    let painter = ui.painter().with_clip_rect(area);
    painter.rect_filled(area, 0.0, Color32::from_gray(46));
    let Some(wf) = app.engine.waveform(info, app.trimmer.audio_stream as u16) else {
        painter.text(
            area.center(),
            egui::Align2::CENTER_CENTER,
            tr("Reading the waveform…"),
            egui::FontId::proportional(12.0),
            theme::text_dim(),
        );
        return;
    };
    let all = wf.peaks.len().max(1);
    let pair = app.trimmer.channel_pair;
    let lanes: Vec<usize> = if all <= 2 {
        (0..all).collect()
    } else {
        (2 * pair..(2 * pair + 2).min(all)).collect()
    };
    let lane_h = area.height() / lanes.len().max(1) as f32;
    let color = Color32::from_gray(150);
    let width = area.width().max(1.0) as i64;
    for (k, &ch) in lanes.iter().enumerate() {
        let mid = area.top() + lane_h * (k as f32 + 0.5);
        if k > 0 {
            let y = area.top() + lane_h * k as f32;
            painter.line_segment(
                [pos2(area.left(), y), pos2(area.right(), y)],
                Stroke::new(1.0, Color32::from_gray(28)),
            );
        }
        for x in 0..width {
            let t0 = shown.start + Tick(shown.duration.0 * x / width);
            let t1 = shown.start + Tick(shown.duration.0 * (x + 1) / width);
            let (f0, f1) = (show.file_time(t0), show.file_time(t1));
            let (lo, hi) = wf.range(ch, f0.min(f1), f0.max(f1));
            let px = area.left() + x as f32 + 0.5;
            painter.line_segment(
                [
                    pos2(px, mid - hi.clamp(-1.0, 1.0) * lane_h * 0.48),
                    pos2(px, mid - lo.clamp(-1.0, 1.0) * lane_h * 0.48),
                ],
                Stroke::new(1.0, color),
            );
        }
        painter.line_segment(
            [pos2(area.left(), mid), pos2(area.right(), mid)],
            Stroke::new(1.0, Color32::from_gray(96)),
        );
    }
}

/// Runs a Trimmer command.
pub(crate) fn run(app: &mut BananaApp, op: TrimmerOp) {
    if let TrimmerOp::NextMedia(dir) = op {
        let media = &app.project().media;
        if media.is_empty() {
            return;
        }
        let n = media.len() as i64;
        let i = app
            .trimmer
            .media
            .and_then(|id| media.iter().position(|m| m.id == id))
            .map_or(if dir > 0 { 0 } else { n - 1 }, |i| {
                (i as i64 + dir as i64).rem_euclid(n)
            });
        let id = media[i as usize].id;
        app.trimmer.open(id);
        return;
    }
    // The history's commands and the switches need no media.
    match op {
        TrimmerOp::Overwrite => {
            let on = !app.trimmer.overwrite;
            app.trimmer.overwrite = on;
            app.notify(if on {
                tr("Timeline overwrite on: what the Trimmer adds replaces what lies there.")
            } else {
                tr("Timeline overwrite off: what the Trimmer adds lies over the events.")
            });
            return;
        }
        TrimmerOp::SortHistory(reverse) => {
            let p = app.project();
            let names: std::collections::HashMap<MediaId, String> = app
                .trimmer
                .history
                .iter()
                .filter_map(|&id| p.media(id).cloned().or_else(|| p.subclip_info(id)))
                .map(|m| (m.id, crate::i18n::media_name(&m).to_lowercase()))
                .collect();
            let name = |id: &MediaId| names.get(id).cloned().unwrap_or_default();
            app.trimmer.history.sort_by_key(name);
            if reverse {
                app.trimmer.history.reverse();
            }
            return;
        }
        TrimmerOp::ClearHistory => {
            app.trimmer.history.clear();
            app.trimmer.media = None;
            return;
        }
        TrimmerOp::PurgeHistory => {
            let p = app.project();
            let used: std::collections::HashSet<MediaId> = p
                .clips()
                .flat_map(|(_, c)| c.take_media().collect::<Vec<_>>())
                .collect();
            app.trimmer.history.retain(|m| used.contains(m));
            if app.trimmer.media.is_some_and(|m| !used.contains(&m)) {
                app.trimmer.media = None;
            }
            return;
        }
        TrimmerOp::ExternalMonitor => {
            let on = !app.trimmer.external;
            app.trimmer.external = on;
            // The external monitor comes on with it, and goes with it.
            if on != app.external.is_some() {
                app.toggle_external_monitor();
            }
            return;
        }
        TrimmerOp::UseAllStreams => {
            let s = &mut app.settings;
            s.trimmer_shown_streams_only = !s.trimmer_shown_streams_only;
            let _ = s.save();
            return;
        }
        TrimmerOp::TogglePreviewWindow => {
            if !app.settings.trimmer_no_monitor {
                app.notify(tr(
                    "Turn Show Video Monitor off first: the picture is in the Trimmer.",
                ));
                return;
            }
            let s = &mut app.settings;
            s.trimmer_no_preview_window = !s.trimmer_no_preview_window;
            let _ = s.save();
            return;
        }
        TrimmerOp::Stop => {
            if app.trimmer_playing() {
                app.stop_preview(true);
            }
            return;
        }
        _ => {}
    }
    let Some(show) = app.trimmer_showing() else {
        return;
    };
    let info = show.info.clone();
    let id = info.id;
    let file = show.file_media();
    let (length, rate) = extent(&show, app.frame_rate());
    let still = app.settings.prefs.new_events().still_length;
    let st = &mut app.trimmer;
    match op {
        TrimmerOp::Overwrite
        | TrimmerOp::SortHistory(_)
        | TrimmerOp::ClearHistory
        | TrimmerOp::PurgeHistory
        | TrimmerOp::NextMedia(_)
        | TrimmerOp::ExternalMonitor
        | TrimmerOp::UseAllStreams
        | TrimmerOp::TogglePreviewWindow
        | TrimmerOp::Stop => {}
        TrimmerOp::RemoveFromHistory => {
            st.history.retain(|&m| m != id);
            st.media = None;
            return;
        }
        TrimmerOp::Play => {
            if !app.trimmer_playing() {
                let from = app.trimmer.cursor;
                app.trimmer_play(from);
            }
        }
        TrimmerOp::Pause => {
            if app.trimmer_playing() {
                app.stop_preview(false);
            } else {
                let from = app.trimmer.cursor;
                app.trimmer_play(from);
            }
        }
        TrimmerOp::PlayToggle => {
            if app.trimmer_playing() {
                app.stop_preview(true);
            } else {
                let from = app.trimmer.cursor;
                app.trimmer_play(from);
            }
        }
        TrimmerOp::PlayFromStart => {
            if app.trimmer_playing() {
                app.stop_preview(false);
            }
            app.trimmer_play(Tick::ZERO);
        }
        TrimmerOp::SyncSelection { up_to } => {
            let Some(sel) = app
                .project()
                .loop_region
                .filter(|r| r.duration > Tick::ZERO)
            else {
                app.notify(tr("Make a time selection on the timeline first."));
                return;
            };
            let st = &mut app.trimmer;
            let (a, b) = if up_to {
                ((st.cursor - sel.duration).max(Tick::ZERO), st.cursor)
            } else {
                (st.cursor, (st.cursor + sel.duration).min(length))
            };
            st.in_point = Some(a);
            st.out_point = Some(b);
            st.selected = true;
        }
        TrimmerOp::CycleStreams(dir) => {
            let all = [
                Streams::VideoAndAudio,
                Streams::VideoOnly,
                Streams::AudioOnly,
            ];
            let k = all.iter().position(|&s| s == st.streams).unwrap_or(0) as i64;
            st.streams = all[(k + dir as i64).rem_euclid(3) as usize];
        }
        TrimmerOp::ZoomSelection => {
            let r = selection(st, &show, still);
            st.view = (r.duration > Tick::ZERO && r.duration < length).then_some(r);
        }
        TrimmerOp::SelectView => {
            let v = st.shown(length);
            st.in_point = Some(v.start);
            st.out_point = Some(v.end());
            st.selected = true;
        }
        TrimmerOp::SelectLoop => {
            if selection(st, &show, still).duration > Tick::ZERO {
                st.selected = true;
            }
        }
        TrimmerOp::Heights(k) => {
            st.audio_share = match k {
                1 => (st.audio_share - 0.25).max(0.0),
                -1 => (st.audio_share + 0.25).min(0.75),
                _ => 0.5,
            };
        }
        TrimmerOp::AudioStream(k) => {
            st.audio_stream = (k as usize).min(info.extra_audio.len());
            st.channel_pair = 0;
        }
        TrimmerOp::ChannelPair(k) => st.channel_pair = k as usize,
        TrimmerOp::OpenCopyInAudioEditor => app.open_media_copy_in_audio_editor(file),
        TrimmerOp::SaveSnapshot => {
            let at = show.file_time(st.cursor);
            app.save_trimmer_snapshot(&info, at);
        }
        TrimmerOp::CreateSubclip => {
            let range = active(st, &show, still).unwrap_or_else(|| selection(st, &show, still));
            app.open_subclip_dialog(file, show.file_range(range));
        }
        TrimmerOp::SelectParentMedia => {
            if let Some((parent, part, _)) = show.part {
                app.trimmer.open_part(parent, part);
            }
            // Its own length now.
            return;
        }
        TrimmerOp::Copy => {
            let Some(range) = active(st, &show, still) else {
                return;
            };
            let streams = st.streams;
            app.copy_trimmer_part(id, range, streams);
        }
        TrimmerOp::DetectScenes => {
            let part = active(st, &show, still).unwrap_or_else(|| from_cursor(st, &show, still));
            let streams = match st.streams {
                Streams::VideoAndAudio => (true, true),
                Streams::VideoOnly => (true, false),
                Streams::AudioOnly => (false, true),
            };
            if show.part.is_some() {
                app.notify(tr(
                    "Open the subclip's parent media to look for its scenes.",
                ));
                return;
            }
            let at = app.cursor;
            app.detect_media_scenes(
                file,
                Some(part),
                crate::media_menu::AfterScenes::AddAt(at),
                streams,
            );
        }
        TrimmerOp::FitToFill => {
            let Some(target) = app
                .project()
                .loop_region
                .filter(|r| r.duration > Tick::ZERO)
            else {
                app.notify(tr("Make a time selection on the timeline to fill."));
                return;
            };
            let st = &app.trimmer;
            let range = active(st, &show, still).unwrap_or_else(|| from_cursor(st, &show, still));
            if range.duration <= Tick::ZERO {
                return;
            }
            let stretch = range.duration.0 as f64 / target.duration.0 as f64;
            if !(0.05..=20.0).contains(&stretch) {
                app.notify(tr(
                    "The timeline's selection is too short or too long to fill.",
                ));
                return;
            }
            let streams = app.trimmer.streams;
            add_from_trimmer(app, id, target.start, range, streams, Some(stretch));
        }
        TrimmerOp::Start => st.cursor = Tick::ZERO,
        TrimmerOp::End => st.cursor = length,
        TrimmerOp::Frame(dir) => {
            st.cursor = rate.frame_start((rate.frame_at(st.cursor) + dir as i64).max(0));
        }
        TrimmerOp::Pixel(dir) => {
            let px = (st.shown(length).duration.0 as f64 / st.bar_width.max(100.0) as f64).max(1.0);
            st.cursor = Tick(st.cursor.0 + (px * dir as f64) as i64);
        }
        TrimmerOp::GoToIn => st.cursor = st.in_point.unwrap_or(Tick::ZERO),
        TrimmerOp::GoToOut => st.cursor = st.out_point.unwrap_or(length),
        TrimmerOp::MarkIn => st.mark_in(),
        TrimmerOp::MarkOut => st.mark_out(),
        TrimmerOp::Select(s) => st.streams = s,
        TrimmerOp::AddFromCursor => {
            // The selection; without one, from the cursor to the end.
            let range = active(st, &show, still).unwrap_or_else(|| from_cursor(st, &show, still));
            if range.duration <= Tick::ZERO {
                return;
            }
            let streams = st.streams;
            let at = app.cursor;
            add_from_trimmer(app, id, at, range, streams, None);
        }
        TrimmerOp::AddToCursor => {
            let Some(range) = active(st, &show, still) else {
                app.notify(tr(
                    "Select a part of the media first: it ends at the cursor.",
                ));
                return;
            };
            let streams = st.streams;
            let at = (app.cursor - range.duration).max(Tick::ZERO);
            add_from_trimmer(app, id, at, range, streams, None);
        }
        TrimmerOp::OpenInAudioEditor => app.open_media_in_audio_editor(file),
        TrimmerOp::InsertMarker => {
            let at = show.file_time(st.cursor);
            let n = info.markers.iter().filter(|m| m.time <= st.cursor).count();
            app.edit_media_markers(file, None, false, |m| {
                m.push(MediaMarker {
                    time: at,
                    ..MediaMarker::default()
                })
            });
            // "Automatically name regions and markers if not playing".
            if show.own_markers() && app.settings.prefs.name_markers && !app.is_playing() {
                app.trimmer.renaming = Some((n, String::new()));
            }
        }
        TrimmerOp::InsertRegion => {
            let r = selection(st, &show, still);
            if r.duration <= Tick::ZERO {
                return;
            }
            let n = info.markers.iter().filter(|m| m.time <= r.start).count();
            let in_file = show.file_range(r);
            app.edit_media_markers(file, None, false, |m| {
                m.push(MediaMarker {
                    time: in_file.start,
                    length: in_file.duration,
                    name: String::new(),
                })
            });
            if show.own_markers() && app.settings.prefs.name_markers && !app.is_playing() {
                app.trimmer.renaming = Some((n, String::new()));
            }
        }
        TrimmerOp::SaveMarkers => app.save_media_markers(file, true),
        TrimmerOp::BeatDetection => app.beat_detection(file),
        TrimmerOp::AutoSaveMarkers => {
            let p = &mut app.settings.prefs;
            p.save_trimmer_markers = !p.save_trimmer_markers;
            let on = p.save_trimmer_markers;
            let _ = app.settings.save();
            if on {
                app.save_media_markers(file, false);
                app.notify(tr(
                    "Trimmer markers and regions are now saved with the media file.",
                ));
            } else {
                app.notify(tr(
                    "Trimmer markers and regions are no longer saved automatically.",
                ));
            }
        }
        TrimmerOp::GoToMarker(n) => {
            let name = n.to_string();
            match info.markers.iter().find(|m| m.name == name) {
                Some(m) => st.cursor = m.time,
                None => app.notify(trf!("There is no marker {}.", n)),
            }
        }
        TrimmerOp::MarkerStep(dir) => {
            let mut edges: Vec<Tick> = info
                .markers
                .iter()
                .flat_map(|m| [m.time, m.end()])
                .collect();
            edges.sort();
            edges.dedup();
            let next = if dir > 0 {
                edges.into_iter().find(|&t| t > st.cursor)
            } else {
                edges.into_iter().rev().find(|&t| t < st.cursor)
            };
            if let Some(t) = next {
                st.cursor = t;
            }
        }
    }
    let st = &mut app.trimmer;
    st.cursor = st.cursor.clamp(Tick::ZERO, length);
}

impl BananaApp {
    /// The Trimmer's Copy: part `range` of media `id` (a subclip: of its
    /// parent) as events of its streams, which Paste puts on the timeline.
    fn copy_trimmer_part(&mut self, id: MediaId, range: TimeRange, streams: Streams) {
        let p = self.project();
        let (media, range, reverse) = match p.subclip(id) {
            Some(s) => {
                let r = TimeRange::new(s.range.start + range.start, range.duration);
                (s.parent, r, s.reverse)
            }
            None => (id, range, false),
        };
        let Some(info) = p.media(media).cloned() else {
            return;
        };
        let (video, audio) = match streams {
            Streams::VideoAndAudio => (true, true),
            Streams::VideoOnly => (true, false),
            Streams::AudioOnly => (false, true),
        };
        // Made on a copy of the project, then copied as the timeline's
        // events are.
        let mut scratch = p.clone();
        let track = |p: &mut Project, kind: TrackKind| {
            p.tracks
                .iter()
                .find(|t| t.kind == kind && !t.adjustment)
                .map(|t| t.id)
                .unwrap_or_else(|| p.add_track(kind))
        };
        let v = (video && info.video.is_some()).then(|| track(&mut scratch, TrackKind::Video));
        let a = (audio && info.audio.is_some()).then(|| track(&mut scratch, TrackKind::Audio));
        let Ok(ids) =
            scratch.insert_media_range(media, Tick::ZERO, range.start, range.duration, v, a)
        else {
            return;
        };
        if reverse {
            for c in &ids {
                let _ = scratch.reverse_event(*c);
            }
        }
        let data = scratch.copy_events(&ids, true);
        self.notify(trn!(
            data.len(),
            "{} clip copied.",
            "{} clips copied.",
            data.len()
        ));
        self.clipboard = Some(data);
    }
}

/// A silent mono WAV of `seconds` at 48 kHz (tests).
#[cfg(test)]
pub(crate) fn tests_wav(path: &std::path::Path, seconds: u32) {
    tests::wav(path, seconds);
}

impl BananaApp {
    /// The Trimmer open on the project's media `index` with a part
    /// selected, Enable Timeline Overwrite on and the time bar zoomed in
    /// (screen tests).
    pub fn trimmer_for_review(&mut self, index: usize) {
        let Some(id) = self.project().media.get(index).map(|m| m.id) else {
            return;
        };
        let s = Tick::from_seconds;
        self.trimmer.open(id);
        self.trimmer.in_point = Some(s(2));
        self.trimmer.out_point = Some(s(5));
        self.trimmer.selected = true;
        self.trimmer.cursor = s(3);
        self.trimmer.overwrite = true;
        self.trimmer.view = Some(TimeRange::new(s(1), s(8)));
        self.show_tab(crate::dock::Tab::Trimmer, false);
    }

    /// The Trimmer as the user's prints show it: the file at `path`
    /// imported and opened, a marker named "toreja" near its start, the
    /// loop region from 30 % to 60 % of it, the cursor out of it (screen
    /// tests; the settings in memory). Whether the file could be read.
    pub fn trimmer_vegas_for_review(&mut self, path: &std::path::Path) -> bool {
        self.settings = crate::settings::UiSettings::in_memory();
        let Some(&id) = self.import_paths(&[path.to_path_buf()]).first() else {
            return false;
        };
        let Some(length) = self.project().media(id).map(|m| m.duration) else {
            return false;
        };
        let at = |f: f64| Tick((length.0 as f64 * f) as i64);
        self.trimmer.open(id);
        self.edit_media_markers(id, None, false, |m| {
            m.push(MediaMarker {
                time: at(0.08),
                length: Tick::ZERO,
                name: "toreja".into(),
            })
        });
        self.trimmer.in_point = Some(at(0.3));
        self.trimmer.out_point = Some(at(0.6));
        self.trimmer.selected = false;
        self.trimmer.cursor = at(0.2);
        self.show_tab(crate::dock::Tab::Trimmer, false);
        true
    }

    /// Where the Trimmer last drew its picture or waveform, its loop bar,
    /// its time bar, a sound's ruler and More (screen tests).
    pub fn trimmer_rects_for_review(&self) -> [Option<Rect>; 5] {
        let [strip, bar, ruler, more] = self.trimmer.drawn;
        [self.trimmer.media_rect, strip, bar, ruler, more]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A silent mono WAV of `seconds` at 48 kHz.
    pub(super) fn wav(path: &std::path::Path, seconds: u32) {
        let frames = 48_000 * seconds;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + frames * 2).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&48_000u32.to_le_bytes());
        b.extend_from_slice(&96_000u32.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(frames * 2).to_le_bytes());
        b.resize(b.len() + frames as usize * 2, 0);
        std::fs::write(path, b).unwrap();
    }

    #[test]
    fn markers_and_regions_belong_to_the_media_file() {
        let dir = std::env::temp_dir().join(format!("bv-trimmer-markers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("take.wav");
        wav(&path, 6);
        let mut app = BananaApp::new(egui::Context::default());
        let id = app.import_paths(std::slice::from_ref(&path))[0];
        app.trimmer.open(id);
        let markers = |app: &BananaApp| app.project().media(id).unwrap().markers.clone();
        assert!(markers(&app).is_empty());

        // M: a marker at the cursor, named at once (the preference).
        app.trimmer.cursor = Tick::from_seconds(2);
        run(&mut app, TrimmerOp::InsertMarker);
        assert_eq!(markers(&app).len(), 1);
        assert_eq!(markers(&app)[0].time, Tick::from_seconds(2));
        assert_eq!(app.trimmer.renaming, Some((0, String::new())));
        app.trimmer.renaming = None;
        app.edit_media_markers(id, None, false, |m| m[0].name = "Hit".into());
        // R: a region over in and out, in time order.
        app.trimmer.cursor = Tick::from_seconds(1);
        run(&mut app, TrimmerOp::MarkIn);
        app.trimmer.cursor = Tick::from_seconds(3);
        run(&mut app, TrimmerOp::MarkOut);
        run(&mut app, TrimmerOp::InsertRegion);
        let m = markers(&app);
        assert_eq!(m.len(), 2);
        assert_eq!(
            (m[0].time, m[0].length),
            (Tick::from_seconds(1), Tick::from_seconds(2))
        );
        assert_eq!(m[1].name, "Hit");
        // Ctrl+Left/Right and the number keys move the cursor to them.
        app.trimmer.cursor = Tick::ZERO;
        run(&mut app, TrimmerOp::MarkerStep(1));
        assert_eq!(app.trimmer.cursor, Tick::from_seconds(1));
        run(&mut app, TrimmerOp::MarkerStep(1));
        assert_eq!(app.trimmer.cursor, Tick::from_seconds(2));
        app.edit_media_markers(id, None, false, |m| m[1].name = "7".into());
        app.trimmer.cursor = Tick::ZERO;
        run(&mut app, TrimmerOp::GoToMarker(7));
        assert_eq!(app.trimmer.cursor, Tick::from_seconds(2));

        // Saved with the file only when asked (S), or at once with the
        // preference.
        let file = path.to_string_lossy().to_string();
        assert!(bv_engine::read_markers(&file).is_empty());
        run(&mut app, TrimmerOp::SaveMarkers);
        assert_eq!(bv_engine::read_markers(&file), markers(&app));
        app.settings.prefs.save_trimmer_markers = true;
        app.edit_media_markers(id, None, false, |m| {
            m.remove(0);
        });
        assert_eq!(bv_engine::read_markers(&file).len(), 1, "saved at once");
        // An undo, then: the file follows when it is next saved.
        app.undo();
        assert_eq!(markers(&app).len(), 2);
        // Imported again: the markers come with the file.
        assert_eq!(
            bv_engine::Engine::probe_file(&file).unwrap().markers.len(),
            1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A drag across the picture selects, from left to right or back; one
    /// starting downward takes the media to the timeline; a click lets the
    /// selection go (its loop region staying); a double-click selects all
    /// again. With its monitor off (Show Video in Preview Window on) the
    /// picture goes to the Video Preview while the Trimmer has the focus.
    #[test]
    fn drags_on_the_picture_and_the_picture_in_the_preview() {
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.settings = crate::settings::UiSettings::in_memory();
        app.load_demo();
        let bars = app.project().media[0].id;
        app.trimmer.open(bars);
        let frame = |app: &mut BananaApp, events: Vec<egui::Event>| {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    pos2(0.0, 0.0),
                    egui::vec2(900.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().show(ui, |ui| super::ui(ui, app));
            });
            out.textures_delta.clear();
        };
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        frame(&mut app, Vec::new());
        frame(&mut app, Vec::new());
        let area = app.trimmer.media_rect.expect("drawn");
        let at = |f: f32| pos2(area.left() + area.width() * f, area.center().y);
        // From 70 % back to 30 %: the out point first.
        frame(&mut app, vec![egui::Event::PointerMoved(at(0.7))]);
        frame(&mut app, vec![button(at(0.7), true)]);
        for k in 1..=8 {
            let f = 0.7 - 0.05 * k as f32;
            frame(&mut app, vec![egui::Event::PointerMoved(at(f))]);
        }
        frame(&mut app, vec![button(at(0.3), false)]);
        let (_, a, b) = app.trimmer.marks();
        let sixty = Tick::from_seconds(60).0 as f64;
        let near =
            |t: Option<Tick>, f: f64| t.is_some_and(|t| (t.0 as f64 / sixty - f).abs() < 0.03);
        assert!(near(a, 0.3) && near(b, 0.7), "{a:?} {b:?}");
        assert!(app.trimmer.selected);
        assert!(app.media_drag.is_none());
        // Downward: the media to the timeline, the selection kept.
        let from = at(0.5);
        frame(&mut app, vec![egui::Event::PointerMoved(from)]);
        frame(&mut app, vec![button(from, true)]);
        for k in 1..=6 {
            let p = from + egui::vec2(1.0, 12.0 * k as f32);
            frame(&mut app, vec![egui::Event::PointerMoved(p)]);
        }
        assert!(
            matches!(app.media_drag, Some(MediaDrag::MediaPart(m, _)) if m == bars),
            "the selection taken to the timeline"
        );
        assert!(near(app.trimmer.marks().1, 0.3));
        app.media_drag = None;
        frame(&mut app, vec![button(from + egui::vec2(1.0, 72.0), false)]);
        // A click: the cursor there, the selection let go, its region kept.
        let click = at(0.9);
        frame(&mut app, vec![egui::Event::PointerMoved(click)]);
        frame(&mut app, vec![button(click, true)]);
        frame(&mut app, vec![button(click, false)]);
        assert!(!app.trimmer.selected);
        assert!(near(Some(app.trimmer.marks().0), 0.9));
        assert!(near(app.trimmer.marks().1, 0.3), "the loop region stays");
        // Hovering shows another time than the cursor's; away, the cursor's.
        frame(&mut app, vec![egui::Event::PointerMoved(at(0.6))]);
        assert!(
            app.trimmer
                .hover
                .is_some_and(|h| h != app.trimmer.marks().0)
        );
        frame(&mut app, vec![egui::Event::PointerMoved(pos2(5.0, 5.0))]);
        assert!(app.trimmer.hover.is_none());
        // The picture in the Video Preview: only with the monitor off,
        // Show Video in Preview Window on and the Trimmer focused.
        assert!(app.trimmer_in_preview().is_none());
        app.settings.trimmer_no_monitor = true;
        assert!(app.trimmer_in_preview().is_none(), "not focused");
        app.keys_tab = Some(crate::dock::Tab::Trimmer);
        let (info, t) = app.trimmer_in_preview().expect("in the preview");
        assert_eq!((info.id, t), (bars, app.trimmer.marks().0));
        app.settings.trimmer_no_preview_window = true;
        assert!(app.trimmer_in_preview().is_none());
        // Ctrl+P's switch: only with the monitor off.
        run(&mut app, TrimmerOp::TogglePreviewWindow);
        assert!(!app.settings.trimmer_no_preview_window);
        app.settings.trimmer_no_monitor = false;
        run(&mut app, TrimmerOp::TogglePreviewWindow);
        assert!(!app.settings.trimmer_no_preview_window, "greyed");
    }

    #[test]
    fn keyboard_commands_mark_and_add_the_selection() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        // Color bars: generated video, no audio.
        let bars = app.project().media[0].id;
        app.trimmer.open(bars);
        let rate = app.frame_rate();
        run(&mut app, TrimmerOp::Frame(1));
        assert_eq!(app.trimmer.cursor, rate.frame_start(1));
        run(&mut app, TrimmerOp::End);
        assert_eq!(app.trimmer.cursor, Tick::from_seconds(60));
        app.trimmer.cursor = Tick::from_seconds(1);
        run(&mut app, TrimmerOp::MarkIn);
        app.trimmer.cursor = Tick::from_seconds(3);
        run(&mut app, TrimmerOp::MarkOut);
        run(&mut app, TrimmerOp::GoToIn);
        assert_eq!(app.trimmer.cursor, Tick::from_seconds(1));

        let clips = |app: &BananaApp| app.project().clips().count();
        let before = clips(&app);
        app.cursor = Tick::from_seconds(20);
        // Shift+A: the two marked seconds end at the timeline cursor.
        run(&mut app, TrimmerOp::AddToCursor);
        assert_eq!(clips(&app), before + 1);
        let added = app
            .project()
            .clips()
            .map(|(_, c)| c)
            .find(|c| c.end() == Tick::from_seconds(20))
            .expect("added clip");
        assert_eq!(added.duration, Tick::from_seconds(2));
        assert_eq!(added.source_in, Tick::from_seconds(1));
        // The selection let go: Shift+A has nothing to end at the cursor,
        // A adds from the Trimmer's cursor to the media's end.
        app.trimmer.selected = false;
        app.trimmer.cursor = Tick::from_seconds(2);
        run(&mut app, TrimmerOp::AddToCursor);
        assert_eq!(clips(&app), before + 1);
        app.cursor = Tick::from_seconds(40);
        run(&mut app, TrimmerOp::AddFromCursor);
        assert_eq!(clips(&app), before + 2);
        let from_cursor = app
            .project()
            .clips()
            .map(|(_, c)| c)
            .find(|c| c.start == Tick::from_seconds(40))
            .expect("added from the cursor");
        assert_eq!(from_cursor.source_in, Tick::from_seconds(2));
        assert_eq!(
            from_cursor.duration,
            Tick::from_seconds(5),
            "generated media: as long as a new event"
        );
        // Audio only: the color bars have no audio, nothing is added.
        run(&mut app, TrimmerOp::Select(Streams::AudioOnly));
        app.cursor = Tick::from_seconds(50);
        run(&mut app, TrimmerOp::AddFromCursor);
        assert_eq!(clips(&app), before + 2);
        // History Next opens the next media of the project.
        run(&mut app, TrimmerOp::NextMedia(1));
        assert_eq!(app.trimmer.media, Some(app.project().media[1].id));
    }

    #[test]
    fn fit_to_fill_overwrite_and_the_selection_from_the_timeline() {
        let s = Tick::from_seconds;
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let bars = app.project().media[0].id;
        app.trimmer.open(bars);
        app.trimmer.cursor = s(1);
        run(&mut app, TrimmerOp::MarkIn);
        app.trimmer.cursor = s(3);
        run(&mut app, TrimmerOp::MarkOut);
        // Fit to Fill: two seconds fill four of the timeline.
        app.set_loop_region(Some(TimeRange::new(s(30), s(4))));
        let undo = app.history.edits();
        run(&mut app, TrimmerOp::FitToFill);
        assert_eq!(app.history.edits(), undo + 1, "one undo");
        let fit = app
            .project()
            .clips()
            .map(|(_, c)| c.clone())
            .find(|c| c.start == s(30))
            .expect("added");
        assert_eq!(fit.duration, s(4));
        assert!(
            (fit.rate - 0.5).abs() < 1e-9,
            "slowed to fill: {}",
            fit.rate
        );
        // Overwrite: what is added replaces the time it takes.
        run(&mut app, TrimmerOp::Overwrite);
        assert!(app.trimmer.overwrite);
        app.cursor = s(31);
        run(&mut app, TrimmerOp::AddFromCursor);
        let track = app.project().clip_track(fit.id).unwrap().id;
        let spans: Vec<(Tick, Tick)> = app
            .project()
            .track(track)
            .unwrap()
            .clips
            .iter()
            .filter(|c| c.start >= s(30))
            .map(|c| (c.start, c.end()))
            .collect();
        assert_eq!(spans, vec![(s(30), s(31)), (s(31), s(33)), (s(33), s(34))]);
        // T: the selection as long as the timeline's, from the cursor.
        app.set_loop_region(Some(TimeRange::new(s(0), s(5))));
        app.trimmer.cursor = s(10);
        run(&mut app, TrimmerOp::SyncSelection { up_to: false });
        assert_eq!(app.trimmer.marks(), (s(10), Some(s(10)), Some(s(15))));
        run(&mut app, TrimmerOp::SyncSelection { up_to: true });
        assert_eq!(app.trimmer.marks(), (s(10), Some(s(5)), Some(s(10))));
    }

    #[test]
    fn the_history_sorts_clears_and_streams_cycle() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let ids: Vec<MediaId> = app.project().media.iter().map(|m| m.id).collect();
        for &id in &ids {
            app.trimmer.open(id);
        }
        run(&mut app, TrimmerOp::SortHistory(false));
        let names = |app: &BananaApp| -> Vec<String> {
            app.trimmer
                .history
                .iter()
                .map(|&id| crate::i18n::media_name(app.project().media(id).unwrap()).to_lowercase())
                .collect()
        };
        let mut sorted = names(&app);
        sorted.sort();
        assert_eq!(names(&app), sorted, "A to Z");
        run(&mut app, TrimmerOp::SortHistory(true));
        sorted.reverse();
        assert_eq!(names(&app), sorted, "Z to A");
        let current = app.trimmer.media;
        run(&mut app, TrimmerOp::RemoveFromHistory);
        assert!(app.trimmer.media.is_none(), "the Trimmer is blank");
        assert!(!app.trimmer.history.contains(&current.unwrap()));
        run(&mut app, TrimmerOp::ClearHistory);
        assert!(app.trimmer.history.is_empty());
        app.trimmer.open(ids[0]);
        run(&mut app, TrimmerOp::CycleStreams(1));
        assert_eq!(app.trimmer.streams, Streams::VideoOnly);
        run(&mut app, TrimmerOp::CycleStreams(1));
        assert_eq!(app.trimmer.streams, Streams::AudioOnly);
        run(&mut app, TrimmerOp::CycleStreams(1));
        assert_eq!(app.trimmer.streams, Streams::VideoAndAudio);
        run(&mut app, TrimmerOp::CycleStreams(-1));
        assert_eq!(app.trimmer.streams, Streams::AudioOnly);
    }

    /// The loop region: Select Loop Region and Set Selection to View
    /// select it; Loop Playback brings a preview back to its start; Copy
    /// puts the selection on the clipboard as events Paste places.
    #[test]
    fn the_loop_region_selects_loops_and_copies() {
        let s = Tick::from_seconds;
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let bars = app.project().media[0].id;
        app.trimmer.open(bars);
        app.trimmer.in_point = Some(s(2));
        app.trimmer.out_point = Some(s(4));
        app.trimmer.cursor = s(10);
        assert!(!app.trimmer.selected);
        // Copy needs the selection.
        run(&mut app, TrimmerOp::Copy);
        assert!(app.clipboard.is_none());
        run(&mut app, TrimmerOp::SelectLoop);
        assert!(app.trimmer.selected);
        run(&mut app, TrimmerOp::Copy);
        let data = app.clipboard.clone().expect("copied");
        assert_eq!(data.len(), 1, "the bars' picture");
        app.cursor = s(45);
        let before = app.project().clips().count();
        app.paste();
        assert_eq!(app.project().clips().count(), before + 1);
        let pasted = app
            .project()
            .clips()
            .map(|(_, c)| c.clone())
            .find(|c| c.start == s(45))
            .expect("pasted");
        assert_eq!((pasted.source_in, pasted.duration), (s(2), s(2)));
        // Loop Playback: past its end, back to its start.
        assert_eq!(app.trimmer.loop_back(s(4)), Some(s(2)));
        assert_eq!(app.trimmer.loop_back(s(3)), None);
        // Set Selection to View: the region as long as the bar shows.
        app.trimmer.view = Some(TimeRange::new(s(5), s(10)));
        run(&mut app, TrimmerOp::SelectView);
        assert_eq!(app.trimmer.marks().1, Some(s(5)));
        assert_eq!(app.trimmer.marks().2, Some(s(15)));
        // Zoom Selection: the bar shows the region.
        app.trimmer.view = None;
        run(&mut app, TrimmerOp::ZoomSelection);
        assert_eq!(app.trimmer.view, Some(TimeRange::new(s(5), s(10))));
    }

    /// A subclip opens alone (its part the whole of what is shown, its
    /// times from 0); Select Parent Media opens the parent with the part
    /// selected; what is added from it is of its part.
    #[test]
    fn a_subclip_opens_alone_and_its_parent_on_request() {
        let s = Tick::from_seconds;
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let parent = app.project().media[0].id;
        let sub = app
            .modify(|p| p.add_subclip(parent, TimeRange::new(s(10), s(4)), false, "Part"))
            .unwrap();
        app.open_in_trimmer(sub);
        assert_eq!(app.trimmer.media, Some(sub));
        let show = app.trimmer_showing().unwrap();
        assert_eq!(show.info.duration, s(4));
        assert_eq!(show.file_time(s(1)), s(11));
        // From its second 1 to its end: of the parent, 11 to 14.
        app.trimmer.cursor = s(1);
        app.cursor = s(50);
        run(&mut app, TrimmerOp::AddFromCursor);
        let added = app
            .project()
            .clips()
            .map(|(_, c)| c.clone())
            .find(|c| c.start == s(50))
            .expect("added");
        assert_eq!((added.media, added.source_in), (parent, s(11)));
        run(&mut app, TrimmerOp::SelectParentMedia);
        assert_eq!(app.trimmer.media, Some(parent));
        assert_eq!(app.trimmer.marks(), (s(10), Some(s(10)), Some(s(14))));
        assert!(app.trimmer.selected);
    }

    /// The time boxes and the ruler count in the Trimmer's format: the
    /// project's ruler format at first, the media's frame rate unless Use
    /// Project Frame Rate.
    #[test]
    fn times_follow_the_trimmers_format() {
        let mut app = BananaApp::new(egui::Context::default());
        app.settings = crate::settings::UiSettings::in_memory();
        app.load_demo();
        let info = app.project().media[0].clone();
        app.settings.trimmer_format = Some(RulerFormat::Seconds);
        assert_eq!(app.trimmer_time(&info, Tick::from_seconds(90)), "90.000");
        app.settings.trimmer_format = Some(RulerFormat::AbsoluteFrames);
        let rate = info.video.unwrap().frame_rate;
        assert_eq!(
            app.trimmer_time(&info, rate.frame_start(12)),
            "12",
            "frames at the media's rate"
        );
        app.settings.trimmer_format = Some(RulerFormat::SmpteEbu);
        assert_eq!(
            app.trimmer_time(&info, Tick::from_seconds(61)),
            "00:01:01:00"
        );
        assert_eq!(channel_pairs(1), ["Channel 1"]);
        assert_eq!(channel_pairs(2), ["Channels 1/2"]);
        assert_eq!(
            channel_pairs(6),
            ["Channels 1/2", "Channels 3/4", "Channels 5/6"]
        );
    }

    /// The selection's ends within 4 points of the pointer: where its
    /// arrows show and a drag moves that end alone.
    #[test]
    fn the_selection_s_ends_are_taken_near_them() {
        let ends = (100.0, 180.0);
        assert_eq!(selection_end_at(97.0, ends), Some(false));
        assert_eq!(selection_end_at(184.0, ends), Some(true));
        assert_eq!(selection_end_at(140.0, ends), None);
        assert_eq!(selection_end_at(95.0, ends), None);
    }
}
