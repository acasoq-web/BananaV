//! Edit Details (View › Window › Edit Details, Ctrl+Alt+4), as VEGAS
//! 2026's Help and its picture show it: the project as a database — the
//! Audio CD Track List (the PQ sheet), Commands, Events, Markers, Media
//! (BananaV's own, kept from before), Regions or Selected Events, picked in
//! Show — in a table whose columns are dragged into another order, sorted
//! by a click on their heading (again: the other way), hidden by dragging
//! their heading off the window and resized at their border; the Preset box
//! keeps the view (the list shown, its columns, their order and widths, the
//! sort) under a name, "All fields" giving every column back. A
//! double-click (or F2) edits a cell — times in the ruler's format, as the
//! Help says — and the timeline follows; check boxes switch with a click.
//! Cells are picked as in a spreadsheet (a drag or Shift a rectangle, Ctrl
//! one more), rows by their number, everything by the corner box; Copy
//! puts them on the clipboard as tab-separated text (whole rows with their
//! headings: the PQ sheet a CD replicator wants), Paste fills the picked
//! cells, Cut copies and empties them, Delete removes the rows' items. The
//! shortcut menu has the commands of VEGAS's Edit Details keyboard list:
//! Edit, Cut, Copy, Paste, Delete and, for events, Choose Active Take, Open
//! in Trimmer, Select in Project Media and Selectively Paste Event
//! Attributes.

use std::collections::{HashMap, HashSet};

use crate::i18n::{tr, trc};
use bv_core::{Tick, TimeRange};
use bv_timeline::{ClipId, MarkerId, MediaId, MediaSource, Project, TrackKind};
use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::cursors::Cursor;
use crate::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, pos2, vec2};
use crate::icons::Icon;
use crate::theme;
use crate::timefmt;

const ROW_H: f32 = 18.0;
const HEAD_H: f32 = 20.0;
/// The row numbers' column (its top is the corner box).
const NUM_W: f32 = 38.0;
const MIN_COL_W: f32 = 24.0;

/// What the window lists (its Show box), in VEGAS's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Show {
    CdTracks,
    Commands,
    #[default]
    Events,
    Markers,
    Media,
    Regions,
    SelectedEvents,
}

impl Show {
    pub const ALL: [Show; 7] = [
        Show::CdTracks,
        Show::Commands,
        Show::Events,
        Show::Markers,
        Show::Media,
        Show::Regions,
        Show::SelectedEvents,
    ];

    fn label(self) -> &'static str {
        match self {
            Show::CdTracks => tr("Audio CD Track List"),
            Show::Commands => tr("Commands"),
            Show::Events => tr("Events"),
            Show::Markers => tr("Markers"),
            Show::Media => tr("Media"),
            Show::Regions => tr("Regions"),
            Show::SelectedEvents => tr("Selected Events"),
        }
    }

    /// Its columns, as VEGAS lists them (the Help's picture for events).
    fn columns(self) -> &'static [Col] {
        use Col::*;
        match self {
            Show::Events | Show::SelectedEvents => &[
                Track,
                Start,
                End,
                Length,
                Takes,
                TakeName,
                TakeStart,
                TimecodeIn,
                TimecodeOut,
                FilePath,
                TapeName,
                Select,
                Mute,
                Lock,
                Normalize,
                SnapOffset,
            ],
            Show::Markers => &[Position, Name],
            Show::Regions => &[Start, End, Length, Name],
            Show::Commands => &[Position, Command, Parameter, Comment],
            Show::CdTracks => &[
                CdTrack, Index, Position, End, Length, Name, Protect, Emphasis, Isrc,
            ],
            Show::Media => &[Name, Length, Video, Audio, FilePath, TapeName],
        }
    }
}

/// A column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Col {
    Track,
    Start,
    End,
    Length,
    Takes,
    TakeName,
    TakeStart,
    TimecodeIn,
    TimecodeOut,
    FilePath,
    TapeName,
    Select,
    Mute,
    Lock,
    Normalize,
    SnapOffset,
    Position,
    Name,
    Command,
    Parameter,
    Comment,
    CdTrack,
    Index,
    Protect,
    Emphasis,
    Isrc,
    Video,
    Audio,
}

impl Col {
    fn label(self) -> &'static str {
        match self {
            Col::Track => tr("Track"),
            Col::Start => tr("Start"),
            Col::End => tr("End"),
            Col::Length => tr("Length"),
            Col::Takes => tr("Number of Takes"),
            Col::TakeName => tr("Active Take Name"),
            Col::TakeStart => tr("Take Start"),
            Col::TimecodeIn => tr("Timecode In"),
            Col::TimecodeOut => tr("Timecode Out"),
            Col::FilePath => tr("File Path"),
            Col::TapeName => tr("Tape Name"),
            Col::Select => tr("Select"),
            Col::Mute => tr("Mute"),
            Col::Lock => tr("Lock"),
            Col::Normalize => tr("Normalize"),
            Col::SnapOffset => tr("Snap Offset"),
            Col::Position => tr("Position"),
            Col::Name => tr("Name"),
            Col::Command => tr("Command"),
            Col::Parameter => tr("Parameter"),
            Col::Comment => tr("Comment"),
            Col::CdTrack => trc("audio CD", "Track"),
            Col::Index => tr("Index"),
            Col::Protect => tr("Prot"),
            Col::Emphasis => tr("Emph"),
            Col::Isrc => "ISRC",
            Col::Video => tr("Video"),
            Col::Audio => tr("Audio"),
        }
    }

    /// Its width at first.
    fn width(self) -> f32 {
        match self {
            Col::Track | Col::CdTrack | Col::Index => 48.0,
            Col::Select | Col::Mute | Col::Lock | Col::Protect | Col::Emphasis => 44.0,
            Col::Normalize => 64.0,
            Col::Takes => 104.0,
            Col::TakeName | Col::FilePath | Col::Parameter | Col::Comment => 150.0,
            Col::Name => 160.0,
            Col::Video | Col::Audio => 140.0,
            Col::Command | Col::TapeName | Col::Isrc => 100.0,
            _ => 92.0,
        }
    }

    fn is_check(self) -> bool {
        matches!(
            self,
            Col::Select | Col::Mute | Col::Lock | Col::Normalize | Col::Protect | Col::Emphasis
        )
    }
}

/// What a row is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Item {
    Event(ClipId),
    Marker(MarkerId),
    Region(MarkerId),
    Command(MarkerId),
    CdTrack(MarkerId),
    CdIndex(MarkerId),
    Media(MediaId),
}

/// What a cell holds.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Text(String),
    /// A timeline time (the ruler's offset added when shown).
    Time(Tick),
    /// A length, or a time within a media.
    Length(Tick),
    Count(i64),
    Check(bool),
    /// Nothing to show (an index's name, a video event's Normalize).
    Blank,
}

impl Value {
    fn text(&self, p: &Project) -> String {
        use timefmt::RulerFormatExt;
        match self {
            Value::Text(s) => s.replace('\n', " "),
            Value::Time(t) => p.settings.effective_ruler_format().format(*t, p),
            Value::Length(d) => timefmt::length_text(p, *d),
            Value::Count(n) => n.to_string(),
            Value::Check(on) => if *on { "1" } else { "0" }.to_string(),
            Value::Blank => String::new(),
        }
    }

    /// For sorting.
    fn order(&self, other: &Value) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (Value::Text(a), Value::Text(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
            (Value::Time(a) | Value::Length(a), Value::Time(b) | Value::Length(b)) => a.cmp(b),
            (Value::Count(a), Value::Count(b)) => a.cmp(b),
            (Value::Check(a), Value::Check(b)) => a.cmp(b),
            (Value::Blank, Value::Blank) => Ordering::Equal,
            (Value::Blank, _) => Ordering::Greater,
            (_, Value::Blank) => Ordering::Less,
            _ => Ordering::Equal,
        }
    }

    fn right_aligned(&self) -> bool {
        matches!(self, Value::Time(_) | Value::Length(_) | Value::Count(_))
    }
}

/// A view's columns (in order, with their widths) and sort (the column,
/// and whether descending).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewLayout {
    pub columns: Vec<(Col, f32)>,
    #[serde(default)]
    pub sort: Option<(Col, bool)>,
}

impl ViewLayout {
    fn all_fields(show: Show) -> ViewLayout {
        ViewLayout {
            columns: show.columns().iter().map(|&c| (c, c.width())).collect(),
            sort: None,
        }
    }
}

/// A view kept under a name (the Preset box).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DetailsPreset {
    pub name: String,
    pub show: Show,
    pub layout: ViewLayout,
}

/// The commands of the window (its shortcut menu and keyboard list).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailsOp {
    Edit,
    Cut,
    Copy,
    Paste,
    Delete,
    /// Ctrl+K (VEGAS's Set Marker Time): the picked marker, region or
    /// command to the playback position, the next row picked.
    SetTime,
}

/// A heading being dragged: its border (a resize) or itself (a move).
#[derive(Clone, Copy, Debug)]
enum HeadDrag {
    Resize {
        col: usize,
        from_w: f32,
        from_x: f32,
    },
    Move {
        col: usize,
        grab: f32,
    },
    Cells {
        from: (usize, usize),
    },
}

/// The window's state.
#[derive(Debug, Default)]
pub(crate) struct DetailsView {
    pub show: Show,
    layouts: HashMap<Show, ViewLayout>,
    preset: String,
    /// The picked cells (their items and columns: they stay picked when
    /// the rows are sorted).
    picked: HashSet<(Item, Col)>,
    /// The picked cells are whole rows (Copy then adds the headings).
    whole_rows: bool,
    /// Where Shift ranges start: a row and a column of the table.
    anchor: Option<(usize, usize)>,
    /// The cell being typed in.
    editing: Option<(Item, Col, String)>,
    focus_edit: bool,
    drag: Option<HeadDrag>,
    /// Each media file's own timecode (read once).
    timecodes: HashMap<MediaId, Option<Tick>>,
}

impl DetailsView {
    fn layout(&self) -> ViewLayout {
        self.layouts
            .get(&self.show)
            .cloned()
            .unwrap_or_else(|| ViewLayout::all_fields(self.show))
    }

    fn set_layout(&mut self, l: ViewLayout) {
        self.layouts.insert(self.show, l);
    }
}

/// The rows of `show`, sorted by `sort`.
fn rows(
    app: &BananaApp,
    view: &mut DetailsView,
    show: Show,
    sort: Option<(Col, bool)>,
) -> Vec<Item> {
    let p = app.project();
    let mut items: Vec<Item> = match show {
        Show::Events => p.clips().map(|(_, c)| Item::Event(c.id)).collect(),
        Show::SelectedEvents => p
            .clips()
            .filter(|(_, c)| app.selection.contains(&c.id))
            .map(|(_, c)| Item::Event(c.id))
            .collect(),
        Show::Markers => p.markers.iter().map(|m| Item::Marker(m.id)).collect(),
        Show::Regions => p.regions.iter().map(|r| Item::Region(r.id)).collect(),
        Show::Commands => p.commands.iter().map(|c| Item::Command(c.id)).collect(),
        Show::CdTracks => p
            .cd_rows()
            .iter()
            .map(|r| {
                if r.index == 1 {
                    Item::CdTrack(r.id)
                } else {
                    Item::CdIndex(r.id)
                }
            })
            .collect(),
        Show::Media => p.media.iter().map(|m| Item::Media(m.id)).collect(),
    };
    if let Some((col, descending)) = sort {
        let mut keyed: Vec<(Value, Item)> = items
            .iter()
            .map(|&i| (value(app, view, i, col), i))
            .collect();
        keyed.sort_by(|a, b| {
            let o = a.0.order(&b.0);
            if descending { o.reverse() } else { o }
        });
        items = keyed.into_iter().map(|(_, i)| i).collect();
    }
    items
}

/// A media's first frame's timecode: its Properties' own, or its file's.
fn media_timecode(app: &BananaApp, view: &mut DetailsView, id: MediaId) -> Tick {
    let Some(m) = app.project().media(id) else {
        return Tick::ZERO;
    };
    if let Some(t) = m.reel.timecode {
        return t;
    }
    *view
        .timecodes
        .entry(id)
        .or_insert_with(|| {
            bv_engine::media_stamps(m)
                .timecode
                .map(Tick::from_seconds_f64)
        })
        .as_ref()
        .unwrap_or(&Tick::ZERO)
}

fn media_path(m: &bv_timeline::MediaInfo) -> String {
    match &m.source {
        MediaSource::File { path } => path.clone(),
        MediaSource::Generated(_) => crate::i18n::media_name(m),
    }
}

/// The value of `item`'s `col`.
fn value(app: &BananaApp, view: &mut DetailsView, item: Item, col: Col) -> Value {
    let p = app.project();
    match item {
        Item::Event(id) => {
            let (Some(c), Some(track)) = (p.clip(id), p.clip_track(id)) else {
                return Value::Blank;
            };
            let media = p.media(c.media);
            match col {
                Col::Track => Value::Count(p.track_index(track.id).map_or(0, |i| i as i64 + 1)),
                Col::Start => Value::Time(c.start),
                Col::End => Value::Time(c.end()),
                Col::Length => Value::Length(c.duration),
                Col::Takes => Value::Count(c.take_count() as i64),
                Col::TakeName => Value::Text(p.take_name(id, c.active_take()).unwrap_or_default()),
                Col::TakeStart => Value::Length(c.source_in),
                Col::TimecodeIn => Value::Length(media_timecode(app, view, c.media) + c.source_in),
                Col::TimecodeOut => Value::Length(
                    media_timecode(app, view, c.media) + c.source_in + c.to_source(c.duration),
                ),
                Col::FilePath => media.map_or(Value::Blank, |m| Value::Text(media_path(m))),
                Col::TapeName => {
                    media.map_or(Value::Blank, |m| Value::Text(m.reel.tape_name.clone()))
                }
                Col::Select => Value::Check(app.selection.contains(&id)),
                Col::Mute => Value::Check(c.muted),
                Col::Lock => Value::Check(c.locked),
                Col::Normalize if track.kind == TrackKind::Audio => {
                    Value::Check(c.normalize.is_some())
                }
                Col::SnapOffset => Value::Length(c.snap_offset),
                _ => Value::Blank,
            }
        }
        Item::Marker(id) => match p.markers.iter().find(|m| m.id == id) {
            Some(m) if col == Col::Position => Value::Time(m.time),
            Some(m) if col == Col::Name => Value::Text(m.name.clone()),
            _ => Value::Blank,
        },
        Item::Region(id) => match p.regions.iter().find(|r| r.id == id) {
            Some(r) => match col {
                Col::Start => Value::Time(r.range.start),
                Col::End => Value::Time(r.range.end()),
                Col::Length => Value::Length(r.range.duration),
                Col::Name => Value::Text(r.name.clone()),
                _ => Value::Blank,
            },
            None => Value::Blank,
        },
        Item::Command(id) => match p.command(id) {
            Some(c) => match col {
                Col::Position => Value::Time(c.time),
                Col::Command => Value::Text(c.command.clone()),
                Col::Parameter => Value::Text(c.parameter.clone()),
                Col::Comment => Value::Text(c.comment.clone()),
                _ => Value::Blank,
            },
            None => Value::Blank,
        },
        Item::CdTrack(id) | Item::CdIndex(id) => {
            let rows = p.cd_rows();
            let Some(k) = rows.iter().position(|r| r.id == id) else {
                return Value::Blank;
            };
            let row = &rows[k];
            let track = matches!(item, Item::CdTrack(_))
                .then(|| p.cd_track(id))
                .flatten();
            match col {
                Col::CdTrack => Value::Count(row.track as i64),
                Col::Index => Value::Count(row.index as i64),
                Col::Position => Value::Time(row.start),
                Col::End => Value::Time(track.map_or(row.end, |t| t.range.end())),
                // A track's length runs to the next track (the pause
                // between them included, as the Help says).
                Col::Length => match track {
                    Some(t) => {
                        let next = p
                            .cd_tracks
                            .iter()
                            .find(|o| o.range.start > t.range.start)
                            .map_or(t.range.end(), |o| o.range.start);
                        Value::Length(next - t.range.start)
                    }
                    None => Value::Length(row.end - row.start),
                },
                Col::Name => track.map_or(Value::Blank, |t| Value::Text(t.name.clone())),
                Col::Protect => track.map_or(Value::Blank, |t| Value::Check(t.protect)),
                Col::Emphasis => track.map_or(Value::Blank, |t| Value::Check(t.emphasis)),
                Col::Isrc => track.map_or(Value::Blank, |t| Value::Text(t.isrc.clone())),
                _ => Value::Blank,
            }
        }
        Item::Media(id) => match p.media(id) {
            Some(m) => match col {
                Col::Name => Value::Text(crate::i18n::media_name(m)),
                Col::Length => Value::Length(m.duration),
                Col::Video => Value::Text(m.video.map_or(String::new(), |v| {
                    format!("{}×{} {}", v.width, v.height, crate::fmt::fps(v.frame_rate))
                })),
                Col::Audio => Value::Text(m.audio.map_or(String::new(), |a| {
                    trn!(
                        a.channels,
                        "{} Hz, {} channel",
                        "{} Hz, {} channels",
                        a.sample_rate.0,
                        a.channels
                    )
                })),
                Col::FilePath => Value::Text(media_path(m)),
                Col::TapeName => Value::Text(m.reel.tape_name.clone()),
                _ => Value::Blank,
            },
            None => Value::Blank,
        },
    }
}

/// Whether `item`'s `col` can be typed in or switched.
fn editable(p: &Project, item: Item, col: Col) -> bool {
    match item {
        Item::Event(id) => {
            let audio = p.clip_track(id).is_some_and(|t| t.kind == TrackKind::Audio);
            match col {
                Col::Normalize => audio,
                Col::Takes | Col::FilePath => false,
                _ => true,
            }
        }
        Item::Marker(_) => matches!(col, Col::Position | Col::Name),
        Item::Region(_) => matches!(col, Col::Start | Col::End | Col::Length | Col::Name),
        Item::Command(_) => {
            matches!(
                col,
                Col::Position | Col::Command | Col::Parameter | Col::Comment
            )
        }
        Item::CdTrack(_) => matches!(
            col,
            Col::Position
                | Col::End
                | Col::Length
                | Col::Name
                | Col::Protect
                | Col::Emphasis
                | Col::Isrc
        ),
        Item::CdIndex(_) => col == Col::Position,
        Item::Media(_) => col == Col::TapeName,
    }
}

/// An ISRC as typed (dashes and spaces left out): twelve letters and
/// digits, or nothing.
fn isrc(text: &str) -> Option<String> {
    let code: String = text
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .collect::<String>()
        .to_uppercase();
    (code.is_empty() || (code.len() == 12 && code.chars().all(|c| c.is_ascii_alphanumeric())))
        .then_some(code)
}

/// Typed `text` into `item`'s `col`: the project changed as it says;
/// `false` when it cannot be read or done. `timecode` is the event's
/// media's first timecode (Timecode In and Out).
/// Moves a marker, region (its length kept), command or CD track to `t`.
fn set_time(p: &mut Project, item: Item, t: Tick) {
    match item {
        Item::Marker(id) => p.move_marker(id, t),
        Item::Region(id) => {
            if let Some(g) = p.regions.iter_mut().find(|g| g.id == id) {
                g.range = TimeRange::new(t, g.range.duration);
            }
            p.regions.sort_by_key(|g| (g.range.start, g.id));
        }
        Item::Command(id) => p.set_command(id, |c| c.time = t),
        Item::CdTrack(id) => {
            p.set_cd_track(id, |c| c.range = TimeRange::new(t, c.range.duration));
        }
        _ => {}
    }
}

fn apply_text(p: &mut Project, item: Item, col: Col, text: &str, timecode: Tick) -> bool {
    let position = |p: &Project| timefmt::parse_position(p, text);
    let length = |p: &Project| timefmt::parse_length(p, text);
    match item {
        Item::Event(id) => {
            let Some(c) = p.clip(id).cloned() else {
                return false;
            };
            // Shows another part of the media: `source_in` becomes `to`.
            let slip_to = |p: &mut Project, to: Tick| {
                let delta = c.to_timeline(c.source_in - to);
                p.slip_takes(id, delta).is_ok()
            };
            match col {
                Col::Track => {
                    let kind = p.clip_track(id).map(|t| t.kind);
                    let target = text
                        .trim()
                        .parse::<usize>()
                        .ok()
                        .and_then(|n| n.checked_sub(1))
                        .and_then(|i| p.tracks.get(i))
                        .filter(|t| Some(t.kind) == kind)
                        .map(|t| t.id);
                    target.is_some_and(|t| p.move_clip(id, c.start, Some(t)).is_ok())
                }
                Col::Start => position(p).is_some_and(|t| p.move_clip(id, t, None).is_ok()),
                Col::End => position(p).is_some_and(|t| p.trim_end(id, t).is_ok()),
                Col::Length => length(p).is_some_and(|d| p.trim_end(id, c.start + d).is_ok()),
                Col::TakeName => p.rename_take(id, c.active_take(), text).is_ok(),
                Col::TakeStart => length(p).is_some_and(|t| slip_to(p, t)),
                Col::TimecodeIn => length(p).is_some_and(|t| slip_to(p, t - timecode)),
                Col::TimecodeOut => length(p).is_some_and(|t| {
                    let used = t - timecode - c.source_in;
                    used > Tick::ZERO && p.trim_end(id, c.start + c.to_timeline(used)).is_ok()
                }),
                Col::TapeName => match p.media.iter_mut().find(|m| m.id == c.media) {
                    Some(m) => {
                        m.reel.tape_name = text.trim().to_string();
                        true
                    }
                    None => false,
                },
                Col::SnapOffset => length(p).is_some_and(|d| p.set_snap_offset(id, d).is_ok()),
                Col::Mute | Col::Lock => {
                    let on = check_text(text);
                    on.is_some_and(|on| {
                        p.clip_mut(id).is_some_and(|c| {
                            if col == Col::Mute {
                                c.muted = on;
                            } else {
                                c.locked = on;
                            }
                            true
                        })
                    })
                }
                _ => false,
            }
        }
        Item::Marker(id) => match col {
            Col::Position => position(p).is_some_and(|t| {
                p.move_marker(id, t);
                true
            }),
            Col::Name => {
                p.rename_marker(id, text.to_string());
                true
            }
            _ => false,
        },
        Item::Region(id) => {
            let Some(r) = p.regions.iter().find(|r| r.id == id).map(|r| r.range) else {
                return false;
            };
            let range = match col {
                Col::Start => position(p).map(|t| {
                    TimeRange::from_bounds(t.max(Tick::ZERO).min(r.end() - Tick(1)), r.end())
                }),
                Col::End => {
                    position(p).map(|t| TimeRange::from_bounds(r.start, t.max(r.start + Tick(1))))
                }
                Col::Length => length(p)
                    .filter(|d| *d > Tick::ZERO)
                    .map(|d| TimeRange::new(r.start, d)),
                Col::Name => {
                    p.rename_marker(id, text.to_string());
                    return true;
                }
                _ => None,
            };
            match range {
                Some(range) => {
                    if let Some(g) = p.regions.iter_mut().find(|g| g.id == id) {
                        g.range = range;
                    }
                    p.regions.sort_by_key(|g| (g.range.start, g.id));
                    true
                }
                None => false,
            }
        }
        Item::Command(id) => {
            let at = if col == Col::Position {
                match position(p) {
                    Some(t) => Some(t),
                    None => return false,
                }
            } else {
                None
            };
            let text = text.to_string();
            p.set_command(id, |c| match col {
                Col::Position => c.time = at.unwrap_or(c.time),
                Col::Command => c.command = text.trim().to_string(),
                Col::Parameter => c.parameter = text,
                _ => c.comment = text,
            });
            true
        }
        Item::CdTrack(id) => {
            let Some(r) = p.cd_track(id).map(|t| t.range) else {
                return false;
            };
            match col {
                Col::Position => position(p).is_some_and(|t| {
                    p.set_cd_track(id, |c| c.range = TimeRange::from_bounds(t, r.end()))
                }),
                Col::End => position(p).is_some_and(|t| {
                    p.set_cd_track(id, |c| c.range = TimeRange::from_bounds(r.start, t))
                }),
                Col::Length => length(p)
                    .is_some_and(|d| p.set_cd_track(id, |c| c.range = TimeRange::new(r.start, d))),
                Col::Name => {
                    let name = text.trim().to_string();
                    p.set_cd_track(id, |c| c.name = name)
                }
                Col::Isrc => isrc(text).is_some_and(|code| p.set_cd_track(id, |c| c.isrc = code)),
                Col::Protect | Col::Emphasis => check_text(text).is_some_and(|on| {
                    p.set_cd_track(id, |c| {
                        if col == Col::Protect {
                            c.protect = on;
                        } else {
                            c.emphasis = on;
                        }
                    })
                }),
                _ => false,
            }
        }
        Item::CdIndex(id) => {
            col == Col::Position && position(p).is_some_and(|t| p.move_cd_index(id, t))
        }
        Item::Media(id) => match p.media.iter_mut().find(|m| m.id == id) {
            Some(m) if col == Col::TapeName => {
                m.reel.tape_name = text.trim().to_string();
                true
            }
            _ => false,
        },
    }
}

/// A check box's value as text (what Copy writes, what Paste reads).
fn check_text(text: &str) -> Option<bool> {
    match text.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "x" => Some(true),
        "0" | "false" | "no" | "off" | "" => Some(false),
        _ => None,
    }
}

/// Switches a check box: those the project keeps through `apply_text`,
/// Select through the selection and Normalize through its gain.
fn toggle(app: &mut BananaApp, item: Item, col: Col, now: bool) {
    match (item, col) {
        (Item::Event(id), Col::Select) => {
            if now {
                app.selection.remove(&id);
            } else {
                app.selection.insert(id);
            }
        }
        (Item::Event(id), Col::Normalize) => {
            let Some(c) = app.project().clip(id).cloned() else {
                return;
            };
            let gain = if now {
                Some(None)
            } else {
                match app.normalize_gain(&c) {
                    Some(Some(g)) => Some(Some(g)),
                    Some(None) => None,
                    None => {
                        app.notify(tr("Wait for the waveforms to finish computing."));
                        None
                    }
                }
            };
            if let Some(gain) = gain {
                app.modify(|p| {
                    if let Some(c) = p.clip_mut(id) {
                        c.normalize = gain;
                    }
                });
            }
        }
        _ => {
            let text = if now { "0" } else { "1" };
            app.modify(|p| apply_text(p, item, col, text, Tick::ZERO));
        }
    }
}

/// Where the pointer is in the table.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Hit {
    Corner,
    Head(usize),
    /// A heading's right border.
    Border(usize),
    RowNumber(usize),
    Cell(usize, usize),
    Nothing,
}

/// The table's pointer, as VEGAS's grids: over a heading's border and
/// while it is dragged, ↔ with a bar (the column resized); over a heading
/// and while it is dragged, the arrow moving a column — off the window
/// (`off`), where letting it go hides the column (never the last of
/// `columns`), the arrow taking one away —; elsewhere none of BananaV's.
fn table_cursor(hover: Hit, drag: Option<HeadDrag>, off: bool, columns: usize) -> Option<Cursor> {
    match drag {
        Some(HeadDrag::Resize { .. }) => Some(Cursor::ResizeBarEw),
        Some(HeadDrag::Move { .. }) if off && columns > 1 => Some(Cursor::ColumnDelete),
        Some(HeadDrag::Move { .. }) => Some(Cursor::ColumnMove),
        Some(HeadDrag::Cells { .. }) => None,
        None => match hover {
            Hit::Border(_) => Some(Cursor::ResizeBarEw),
            Hit::Head(_) => Some(Cursor::ColumnMove),
            _ => None,
        },
    }
}

/// The table's places: its content's origin and what is visible.
struct Geometry {
    origin: egui::Pos2,
    visible: Rect,
    /// Each column's left edge from the origin, and its width.
    lefts: Vec<(f32, f32)>,
    rows: usize,
}

impl Geometry {
    fn head_rect(&self, c: usize) -> Rect {
        let (x, w) = self.lefts[c];
        Rect::from_min_size(pos2(self.origin.x + x, self.visible.top()), vec2(w, HEAD_H))
    }

    fn cell_rect(&self, r: usize, c: usize) -> Rect {
        let (x, w) = self.lefts[c];
        Rect::from_min_size(
            pos2(self.origin.x + x, self.origin.y + HEAD_H + r as f32 * ROW_H),
            vec2(w, ROW_H),
        )
    }

    fn number_rect(&self, r: usize) -> Rect {
        Rect::from_min_size(
            pos2(self.origin.x, self.origin.y + HEAD_H + r as f32 * ROW_H),
            vec2(NUM_W, ROW_H),
        )
    }

    fn hit(&self, p: egui::Pos2) -> Hit {
        if !self.visible.contains(p) {
            return Hit::Nothing;
        }
        let x = p.x - self.origin.x;
        if p.y < self.visible.top() + HEAD_H {
            if x < NUM_W {
                return Hit::Corner;
            }
            for (c, &(left, w)) in self.lefts.iter().enumerate() {
                if (x - (left + w)).abs() <= 3.0 {
                    return Hit::Border(c);
                }
                if x >= left && x < left + w {
                    return Hit::Head(c);
                }
            }
            return Hit::Nothing;
        }
        let r = ((p.y - self.origin.y - HEAD_H) / ROW_H).floor();
        if r < 0.0 || r as usize >= self.rows {
            return Hit::Nothing;
        }
        let r = r as usize;
        if x < NUM_W {
            return Hit::RowNumber(r);
        }
        self.lefts
            .iter()
            .position(|&(left, w)| x >= left && x < left + w)
            .map_or(Hit::Nothing, |c| Hit::Cell(r, c))
    }
}

pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    let mut view = std::mem::take(&mut app.details);
    draw(ui, app, &mut view);
    app.details = view;
}

fn draw(ui: &mut egui::Ui, app: &mut BananaApp, view: &mut DetailsView) {
    top_bar(ui, app, view);
    // The shortcut menu's Paste: the clipboard comes as a paste event.
    if app.details_paste_pending
        && let Some(text) = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Paste(s) => Some(s.clone()),
                _ => None,
            })
        })
    {
        app.details_paste_pending = false;
        run_with(app, view, DetailsOp::Paste, Some(text));
    }
    let layout = view.layout();
    let show = view.show;
    let items = rows(app, view, show, layout.sort);
    let cols: Vec<Col> = layout.columns.iter().map(|c| c.0).collect();
    let mut lefts = Vec::with_capacity(cols.len());
    let mut x = NUM_W;
    for &(_, w) in &layout.columns {
        lefts.push((x, w));
        x += w;
    }
    let total = vec2(x, HEAD_H + items.len() as f32 * ROW_H);
    // Every value once a frame (what is visible is painted).
    let values: Vec<Vec<Value>> = items
        .iter()
        .map(|&i| cols.iter().map(|&c| value(app, view, i, c)).collect())
        .collect();
    let window = ui.max_rect();
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, vp| {
            let (rect, _) = ui.allocate_exact_size(
                vec2(total.x.max(vp.width()), total.y.max(vp.height())),
                Sense::hover(),
            );
            let g = Geometry {
                origin: rect.min,
                visible: Rect::from_min_size(rect.min + vp.min.to_vec2(), vp.size()),
                lefts,
                rows: items.len(),
            };
            table(ui, app, view, &g, &items, &cols, &values, window);
        });
}

/// Show, the Preset box with Save and Delete.
fn top_bar(ui: &mut egui::Ui, app: &mut BananaApp, view: &mut DetailsView) {
    ui.horizontal(|ui| {
        ui.label(tr("Show:"));
        let before = view.show;
        egui::ComboBox::from_id_salt("detalhes-mostrar")
            .selected_text(view.show.label())
            .width(180.0)
            .show_ui(ui, |ui| {
                for s in Show::ALL {
                    ui.selectable_value(&mut view.show, s, s.label());
                }
            });
        if view.show != before {
            view.picked.clear();
            view.anchor = None;
            view.editing = None;
        }
        ui.add_space(8.0);
        let mut names: Vec<String> = vec![tr("All fields").to_string()];
        names.extend(app.settings.details_presets.iter().map(|p| p.name.clone()));
        if view.preset.is_empty() {
            view.preset = names[0].clone();
        }
        if let Some(k) =
            crate::command_ui::editable_combo(ui, "detalhes-predefinicao", &mut view.preset, &names)
        {
            view.preset = names[k].clone();
            if k == 0 {
                view.set_layout(ViewLayout::all_fields(view.show));
            } else if let Some(p) = app.settings.details_presets.get(k - 1).cloned() {
                view.show = p.show;
                view.set_layout(p.layout);
                view.picked.clear();
            }
        }
        let name = view.preset.trim().to_string();
        let own = !name.is_empty() && name != tr("All fields");
        if crate::widgets::icon_button_ex(
            ui,
            Icon::Save,
            tr("Save the view as this preset"),
            false,
            own,
        )
        .clicked()
        {
            let preset = DetailsPreset {
                name: name.clone(),
                show: view.show,
                layout: view.layout(),
            };
            let list = &mut app.settings.details_presets;
            match list.iter_mut().find(|p| p.name == name) {
                Some(p) => *p = preset,
                None => list.push(preset),
            }
            let _ = app.settings.save();
        }
        let known = app.settings.details_presets.iter().any(|p| p.name == name);
        if crate::widgets::icon_button_ex(ui, Icon::Delete, tr("Delete this preset"), false, known)
            .clicked()
        {
            app.settings.details_presets.retain(|p| p.name != name);
            view.preset = tr("All fields").to_string();
            let _ = app.settings.save();
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn table(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    view: &mut DetailsView,
    g: &Geometry,
    items: &[Item],
    cols: &[Col],
    values: &[Vec<Value>],
    window: Rect,
) {
    let project = app.project().clone();
    let id = ui.id().with("detalhes-tabela");
    let resp = ui.interact(g.visible, id, Sense::click_and_drag());
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let hover = pointer.map_or(Hit::Nothing, |p| g.hit(p));
    let shown = if resp.hovered() || view.drag.is_some() {
        hover
    } else {
        Hit::Nothing
    };
    let off = pointer.is_some_and(|p| !window.contains(p));
    if let Some(c) = table_cursor(shown, view.drag, off, cols.len()) {
        crate::cursors::set(ui.ctx(), c);
    }
    let modifiers = ui.input(|i| i.modifiers);
    let at = |r: usize, c: usize| (items[r], cols[c]);

    // Dragging: a heading's border resizes it, a heading moves (or, off the
    // window, hides) its column, cells pick a rectangle.
    if resp.drag_started()
        && let Some(o) = ui.input(|i| i.pointer.press_origin())
    {
        view.drag = match g.hit(o) {
            Hit::Border(c) => Some(HeadDrag::Resize {
                col: c,
                from_w: g.lefts[c].1,
                from_x: o.x,
            }),
            Hit::Head(c) => Some(HeadDrag::Move {
                col: c,
                grab: o.x - g.head_rect(c).left(),
            }),
            Hit::Cell(r, c) => Some(HeadDrag::Cells { from: (r, c) }),
            _ => None,
        };
    }
    if let (Some(drag), Some(p)) = (view.drag, pointer)
        && resp.dragged()
    {
        match drag {
            HeadDrag::Resize {
                col,
                from_w,
                from_x,
            } => {
                let mut l = view.layout();
                if let Some(c) = l.columns.get_mut(col) {
                    c.1 = (from_w + p.x - from_x).max(MIN_COL_W);
                }
                view.set_layout(l);
            }
            HeadDrag::Cells { from } => {
                if let Hit::Cell(r, c) = g.hit(p) {
                    view.picked = rectangle(items, cols, from, (r, c));
                    view.whole_rows = false;
                    view.anchor = Some(from);
                }
            }
            HeadDrag::Move { .. } => {}
        }
    }
    if resp.drag_stopped() {
        if let (Some(HeadDrag::Move { col, .. }), Some(p)) = (view.drag, pointer) {
            let mut l = view.layout();
            if !window.contains(p) {
                // Dragged off the window: the column is no longer shown.
                if l.columns.len() > 1 {
                    l.columns.remove(col);
                }
            } else {
                let x = p.x - g.origin.x;
                let to = g
                    .lefts
                    .iter()
                    .position(|&(left, w)| x < left + w / 2.0)
                    .unwrap_or(g.lefts.len());
                let moved = l.columns.remove(col);
                let to = if to > col { to - 1 } else { to };
                l.columns.insert(to.min(l.columns.len()), moved);
            }
            view.set_layout(l);
        }
        view.drag = None;
    }

    // Clicks.
    let click = resp
        .clicked()
        .then(|| resp.interact_pointer_pos())
        .flatten();
    if let Some(p) = click {
        commit_edit(app, view);
        app.key_focus = crate::keymap::Context::EditDetails;
        match g.hit(p) {
            Hit::Corner => {
                view.picked = items
                    .iter()
                    .flat_map(|&i| cols.iter().map(move |&c| (i, c)))
                    .collect();
                view.whole_rows = true;
            }
            Hit::Head(c) => {
                let mut l = view.layout();
                let col = cols[c];
                l.sort = match l.sort {
                    Some((s, false)) if s == col => Some((col, true)),
                    _ => Some((col, false)),
                };
                view.set_layout(l);
            }
            Hit::RowNumber(r) => {
                let row = |r: usize| cols.iter().map(move |&c| (items[r], c));
                if modifiers.shift
                    && let Some((a, _)) = view.anchor
                {
                    view.picked = (a.min(r)..=a.max(r)).flat_map(row).collect();
                } else if modifiers.ctrl || modifiers.command {
                    view.picked.extend(row(r));
                    view.anchor = Some((r, 0));
                } else {
                    view.picked = row(r).collect();
                    view.anchor = Some((r, 0));
                }
                view.whole_rows = true;
            }
            Hit::Cell(r, c) => {
                let (item, col) = at(r, c);
                let in_box = check_box(g.cell_rect(r, c)).expand(3.0).contains(p);
                if col.is_check()
                    && in_box
                    && editable(&project, item, col)
                    && let Value::Check(now) = values[r][c]
                {
                    toggle(app, item, col, now);
                }
                if modifiers.shift
                    && let Some(a) = view.anchor
                {
                    view.picked = rectangle(items, cols, a, (r, c));
                } else if modifiers.ctrl || modifiers.command {
                    if !view.picked.remove(&(item, col)) {
                        view.picked.insert((item, col));
                    }
                    view.anchor = Some((r, c));
                } else {
                    view.picked = HashSet::from([(item, col)]);
                    view.anchor = Some((r, c));
                }
                view.whole_rows = false;
                // The cursor goes to what the row is about.
                if let Some(t) = item_time(&project, item) {
                    app.seek(t);
                }
            }
            _ => {}
        }
    }
    if resp.double_clicked()
        && let Some(p) = resp.interact_pointer_pos()
        && let Hit::Cell(r, c) = g.hit(p)
    {
        start_edit(app, view, items[r], cols[c], &values[r][c]);
    }
    if resp.secondary_clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        app.key_focus = crate::keymap::Context::EditDetails;
        match g.hit(p) {
            Hit::Cell(r, c) if !view.picked.contains(&at(r, c)) => {
                view.picked = HashSet::from([at(r, c)]);
                view.anchor = Some((r, c));
                view.whole_rows = false;
            }
            Hit::RowNumber(r) if !view.picked.contains(&at(r, 0)) => {
                view.picked = cols.iter().map(|&c| (items[r], c)).collect();
                view.anchor = Some((r, 0));
                view.whole_rows = true;
            }
            _ => {}
        }
    }
    resp.context_menu(|ui| shortcut_menu(ui, app, view));

    paint(ui, &project, view, g, items, cols, values, hover);
    edit_box(ui, app, view, g, items, cols);
}

/// The cells between two corners.
fn rectangle(
    items: &[Item],
    cols: &[Col],
    a: (usize, usize),
    b: (usize, usize),
) -> HashSet<(Item, Col)> {
    let (r0, r1) = (a.0.min(b.0), a.0.max(b.0));
    let (c0, c1) = (a.1.min(b.1), a.1.max(b.1));
    (r0..=r1.min(items.len().saturating_sub(1)))
        .flat_map(|r| (c0..=c1.min(cols.len().saturating_sub(1))).map(move |c| (items[r], cols[c])))
        .collect()
}

/// Where the cursor goes when a row is clicked: what it is about.
fn item_time(p: &Project, item: Item) -> Option<Tick> {
    match item {
        Item::Event(id) => p.clip(id).map(|c| c.start),
        Item::Marker(id) => p.markers.iter().find(|m| m.id == id).map(|m| m.time),
        Item::Region(id) => p.regions.iter().find(|r| r.id == id).map(|r| r.range.start),
        Item::Command(id) => p.command(id).map(|c| c.time),
        Item::CdTrack(id) => p.cd_track(id).map(|t| t.range.start),
        Item::CdIndex(id) => p.cd_indices.iter().find(|i| i.id == id).map(|i| i.time),
        Item::Media(_) => None,
    }
}

/// The square a check box is drawn in, in its cell's middle.
fn check_box(cell: Rect) -> Rect {
    Rect::from_center_size(cell.center(), vec2(11.0, 11.0))
}

#[allow(clippy::too_many_arguments)]
fn paint(
    ui: &egui::Ui,
    p: &Project,
    view: &DetailsView,
    g: &Geometry,
    items: &[Item],
    cols: &[Col],
    values: &[Vec<Value>],
    hover: Hit,
) {
    let painter = ui.painter().with_clip_rect(g.visible);
    let lines = Stroke::new(1.0, theme::separator());
    let font = FontId::proportional(12.0);
    let body = Rect::from_min_max(
        pos2(g.visible.left(), g.visible.top() + HEAD_H),
        g.visible.max,
    );
    let rows_painter = painter.with_clip_rect(body);
    rows_painter.rect_filled(body, 0.0, theme::panel_dark());
    let first = (((body.top() - g.origin.y - HEAD_H) / ROW_H)
        .floor()
        .max(0.0)) as usize;
    let last = (((body.bottom() - g.origin.y - HEAD_H) / ROW_H)
        .ceil()
        .max(0.0)) as usize;
    let selection = theme::selection();
    for r in first..last.min(items.len()) {
        let num = g.number_rect(r);
        rows_painter.rect_filled(num, 0.0, theme::panel());
        let row_picked = cols.iter().all(|&c| view.picked.contains(&(items[r], c)));
        crate::widgets::fit_text(
            &rows_painter,
            pos2(num.right() - 4.0, num.center().y),
            Align2::RIGHT_CENTER,
            (r + 1).to_string(),
            font.clone(),
            if row_picked {
                theme::text_strong()
            } else {
                theme::text_dim()
            },
            NUM_W - 6.0,
        );
        for (c, &col) in cols.iter().enumerate() {
            let cell = g.cell_rect(r, c);
            let v = &values[r][c];
            if view.picked.contains(&(items[r], col)) {
                rows_painter.rect_filled(cell, 0.0, selection.gamma_multiply(0.55));
            } else if hover == Hit::Cell(r, c) {
                rows_painter.rect_filled(cell, 0.0, theme::hover());
            }
            match v {
                Value::Check(on) => {
                    let b = check_box(cell);
                    rows_painter.rect_filled(b, 0.0, Color32::from_gray(245));
                    rows_painter.rect_stroke(b, 0.0, lines, egui::StrokeKind::Inside);
                    if *on {
                        rows_painter.line_segment(
                            [
                                b.left_center() + vec2(2.0, 0.0),
                                b.center_bottom() - vec2(1.0, 3.0),
                            ],
                            Stroke::new(1.6, Color32::from_gray(20)),
                        );
                        rows_painter.line_segment(
                            [
                                b.center_bottom() - vec2(1.0, 3.0),
                                b.right_top() + vec2(-2.0, 2.0),
                            ],
                            Stroke::new(1.6, Color32::from_gray(20)),
                        );
                    }
                }
                Value::Blank => {}
                _ => {
                    let text = v.text(p);
                    let (x, align) = if v.right_aligned() {
                        (cell.right() - 4.0, Align2::RIGHT_CENTER)
                    } else {
                        (cell.left() + 4.0, Align2::LEFT_CENTER)
                    };
                    crate::widgets::fit_text(
                        &rows_painter,
                        pos2(x, cell.center().y),
                        align,
                        text,
                        font.clone(),
                        theme::text(),
                        cell.width() - 8.0,
                    );
                }
            }
            rows_painter.line_segment([cell.right_top(), cell.right_bottom()], lines);
            rows_painter.line_segment([cell.left_bottom(), cell.right_bottom()], lines);
        }
        rows_painter.line_segment([num.right_top(), num.right_bottom()], lines);
        rows_painter.line_segment([num.left_bottom(), num.right_bottom()], lines);
    }

    // The headings, on top, staying while the rows scroll.
    let head = Rect::from_min_size(g.visible.min, vec2(g.visible.width(), HEAD_H));
    painter.rect_filled(head, 0.0, theme::panel());
    let corner = Rect::from_min_size(pos2(g.origin.x, head.top()), vec2(NUM_W, HEAD_H));
    painter.rect_stroke(corner.shrink(4.0), 0.0, lines, egui::StrokeKind::Inside);
    let layout = view.layout();
    for (c, &col) in cols.iter().enumerate() {
        let r = g.head_rect(c);
        if hover == Hit::Head(c) {
            painter.rect_filled(r, 0.0, theme::hover());
        }
        let mut label = col.label().to_string();
        if let Some((s, descending)) = layout.sort
            && s == col
        {
            label = format!("{label} {}", if descending { "▼" } else { "▲" });
        }
        crate::widgets::fit_text(
            &painter,
            r.center(),
            Align2::CENTER_CENTER,
            label,
            font.clone(),
            theme::text(),
            r.width() - 6.0,
        );
        painter.line_segment([r.right_top(), r.right_bottom()], lines);
    }
    painter.line_segment([head.left_bottom(), head.right_bottom()], lines);
    // A heading being moved follows the pointer.
    if let Some(HeadDrag::Move { col, grab }) = view.drag
        && let Some(p) = ui.input(|i| i.pointer.latest_pos())
        && col < cols.len()
    {
        let w = g.lefts[col].1;
        let r = Rect::from_min_size(pos2(p.x - grab, head.top()), vec2(w, HEAD_H));
        let top = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("detalhes-arrasto"),
        ));
        top.rect_filled(r, 2.0, selection.gamma_multiply(0.8));
        top.text(
            r.center(),
            Align2::CENTER_CENTER,
            cols[col].label(),
            font,
            theme::text_strong(),
        );
    }
}

/// The text box over the cell being typed in: Enter or a click elsewhere
/// keeps it, Escape leaves the cell as it was.
fn edit_box(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    view: &mut DetailsView,
    g: &Geometry,
    items: &[Item],
    cols: &[Col],
) {
    let Some((item, col, mut text)) = view.editing.take() else {
        return;
    };
    let (Some(r), Some(c)) = (
        items.iter().position(|&i| i == item),
        cols.iter().position(|&o| o == col),
    ) else {
        return;
    };
    let cell = g.cell_rect(r, c);
    let edit = ui.put(
        cell,
        egui::TextEdit::singleline(&mut text)
            .margin(vec2(3.0, 1.0))
            .font(egui::TextStyle::Body),
    );
    if std::mem::take(&mut view.focus_edit) {
        edit.request_focus();
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        return;
    }
    if edit.lost_focus() {
        view.editing = Some((item, col, text));
        commit_edit(app, view);
        return;
    }
    view.editing = Some((item, col, text));
}

/// Starts typing in a cell (not a check box's).
fn start_edit(app: &BananaApp, view: &mut DetailsView, item: Item, col: Col, v: &Value) {
    if col.is_check() || !editable(app.project(), item, col) {
        return;
    }
    let text = match v {
        Value::Text(s) => s.clone(),
        other => other.text(app.project()),
    };
    view.editing = Some((item, col, text));
    view.focus_edit = true;
}

/// What was typed goes to the project.
fn commit_edit(app: &mut BananaApp, view: &mut DetailsView) {
    let Some((item, col, text)) = view.editing.take() else {
        return;
    };
    let timecode = event_timecode(app, view, item);
    if !app.modify(|p| apply_text(p, item, col, &text, timecode)) {
        app.notify(tr("That value cannot go there."));
    }
}

/// The first timecode of an event's media (Timecode In and Out).
fn event_timecode(app: &BananaApp, view: &mut DetailsView, item: Item) -> Tick {
    match item {
        Item::Event(id) => match app.project().clip(id).map(|c| c.media) {
            Some(m) => media_timecode(app, view, m),
            None => Tick::ZERO,
        },
        _ => Tick::ZERO,
    }
}

fn shortcut_menu(ui: &mut egui::Ui, app: &mut BananaApp, view: &mut DetailsView) {
    let any = !view.picked.is_empty();
    let events: Vec<ClipId> = picked_items(view)
        .into_iter()
        .filter_map(|i| match i {
            Item::Event(id) => Some(id),
            _ => None,
        })
        .collect();
    for (op, label, on) in [
        (DetailsOp::Edit, tr("Edit"), view.picked.len() == 1),
        (DetailsOp::Cut, tr("Cut"), any),
        (DetailsOp::Copy, tr("Copy"), any),
        (DetailsOp::Paste, tr("Paste"), any),
        (DetailsOp::Delete, tr("Delete"), any),
    ] {
        if ui.add_enabled(on, egui::Button::new(label)).clicked() {
            if op == DetailsOp::Paste {
                // The clipboard comes back as a paste event.
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                app.details_paste_pending = true;
            } else {
                run_with(app, view, op, None);
            }
            ui.close();
        }
    }
    if !events.is_empty() {
        ui.separator();
        app.selection = events.iter().copied().collect();
        crate::menus::cmd(ui, app, crate::commands::Cmd::TakeChooseActive);
        if ui.button(tr("Open in Trimmer")).clicked() {
            app.run_extra(crate::keymap::Extra::OpenInTrimmer);
            ui.close();
        }
        if ui.button(tr("Select in Project Media")).clicked() {
            app.run_extra(crate::keymap::Extra::SelectInMediaPool);
            ui.close();
        }
        crate::menus::cmd(ui, app, crate::commands::Cmd::PasteAttributesSelective);
    }
}

/// The items of the picked cells, in no order.
fn picked_items(view: &DetailsView) -> Vec<Item> {
    let mut v: Vec<Item> = view.picked.iter().map(|&(i, _)| i).collect();
    v.sort_by_key(|i| format!("{i:?}"));
    v.dedup();
    v
}

/// A command of the window (its keyboard list's Details.*), on what is
/// picked: `pasted` is the clipboard's text for Paste.
pub(crate) fn run(app: &mut BananaApp, op: DetailsOp, pasted: Option<String>) {
    let mut view = std::mem::take(&mut app.details);
    run_with(app, &mut view, op, pasted);
    app.details = view;
}

fn run_with(app: &mut BananaApp, view: &mut DetailsView, op: DetailsOp, pasted: Option<String>) {
    let layout = view.layout();
    let show = view.show;
    let items = rows(app, view, show, layout.sort);
    let cols: Vec<Col> = layout.columns.iter().map(|c| c.0).collect();
    // The picked cells in the table's order.
    let marked = &view.picked;
    let picked: Vec<(usize, usize)> = items
        .iter()
        .enumerate()
        .flat_map(|(r, &i)| {
            cols.iter()
                .enumerate()
                .filter(move |&(_, &c)| marked.contains(&(i, c)))
                .map(move |(c, _)| (r, c))
                .collect::<Vec<_>>()
        })
        .collect();
    match op {
        DetailsOp::Edit => {
            if let Some(&(r, c)) = picked.first() {
                let v = value(app, view, items[r], cols[c]);
                start_edit(app, view, items[r], cols[c], &v);
            }
        }
        DetailsOp::Copy | DetailsOp::Cut => {
            if picked.is_empty() {
                return;
            }
            let text = copy_text(app, view, &items, &cols, &picked);
            app.to_clipboard = Some(text);
            if op == DetailsOp::Cut {
                app.modify(|p| {
                    for &(r, c) in &picked {
                        let (item, col) = (items[r], cols[c]);
                        if matches!(
                            col,
                            Col::Name
                                | Col::Comment
                                | Col::Parameter
                                | Col::TapeName
                                | Col::TakeName
                                | Col::Isrc
                        ) && editable(p, item, col)
                        {
                            apply_text(p, item, col, "", Tick::ZERO);
                        }
                    }
                });
            }
        }
        DetailsOp::Paste => {
            let Some(text) = pasted else {
                return;
            };
            let Some(&(r0, c0)) = picked.first() else {
                return;
            };
            let grid: Vec<Vec<String>> = text
                .trim_end_matches(['\n', '\r'])
                .lines()
                .map(|l| l.split('\t').map(str::to_string).collect())
                .collect();
            // One value fills every picked cell; a block goes from the
            // first picked cell on.
            let targets: Vec<(usize, usize, String)> = if grid.len() == 1 && grid[0].len() == 1 {
                picked
                    .iter()
                    .map(|&(r, c)| (r, c, grid[0][0].clone()))
                    .collect()
            } else {
                grid.iter()
                    .enumerate()
                    .flat_map(|(i, line)| {
                        line.iter()
                            .enumerate()
                            .map(move |(j, v)| (r0 + i, c0 + j, v.clone()))
                    })
                    .filter(|&(r, c, _)| r < items.len() && c < cols.len())
                    .collect()
            };
            let codes: Vec<Tick> = targets
                .iter()
                .map(|&(r, _, _)| event_timecode(app, view, items[r]))
                .collect();
            let done = app.modify(|p| {
                let mut done = 0;
                for (k, (r, c, v)) in targets.iter().enumerate() {
                    let (item, col) = (items[*r], cols[*c]);
                    if editable(p, item, col) && col != Col::Normalize && col != Col::Select {
                        done += apply_text(p, item, col, v, codes[k]) as usize;
                    }
                }
                done
            });
            if done == 0 {
                app.notify(tr("That value cannot go there."));
            }
        }
        DetailsOp::SetTime => {
            // As VEGAS's Help places captions while the project plays:
            // the picked one goes where playback is (the cursor when
            // stopped) and the next one is picked.
            let Some(&(r, _)) = picked.iter().find(|&&(r, _)| {
                matches!(
                    items[r],
                    Item::Marker(_) | Item::Region(_) | Item::Command(_) | Item::CdTrack(_)
                )
            }) else {
                app.notify(tr(
                    "Pick a marker, region or command in Edit Details first.",
                ));
                return;
            };
            let (item, t) = (items[r], app.mark_time());
            app.modify(|p| set_time(p, item, t));
            if let Some(&next) = items.get(r + 1) {
                view.picked = cols.iter().map(|&c| (next, c)).collect();
                view.whole_rows = true;
                view.anchor = Some((r + 1, 0));
            }
        }
        DetailsOp::Delete => {
            let mut events = Vec::new();
            let mut marks = Vec::new();
            for &(r, _) in &picked {
                match items[r] {
                    Item::Event(id) => events.push(id),
                    Item::Marker(id)
                    | Item::Region(id)
                    | Item::Command(id)
                    | Item::CdTrack(id)
                    | Item::CdIndex(id) => marks.push(id),
                    Item::Media(_) => {}
                }
            }
            events.sort();
            events.dedup();
            marks.sort();
            marks.dedup();
            if events.is_empty() && marks.is_empty() {
                return;
            }
            app.modify(|p| {
                let _ = p.delete_clips(&events, false);
                for id in marks {
                    p.remove_marker(id);
                }
            });
            for id in events {
                app.selection.remove(&id);
            }
            view.picked.clear();
        }
    }
}

/// The picked cells as tab-separated text: a line per row, whole rows
/// headed by the columns' names.
fn copy_text(
    app: &BananaApp,
    view: &mut DetailsView,
    items: &[Item],
    cols: &[Col],
    picked: &[(usize, usize)],
) -> String {
    let p = app.project().clone();
    let mut lines = Vec::new();
    let used_cols: Vec<usize> = {
        let mut v: Vec<usize> = picked.iter().map(|&(_, c)| c).collect();
        v.sort();
        v.dedup();
        v
    };
    if view.whole_rows {
        lines.push(
            std::iter::once("#".to_string())
                .chain(used_cols.iter().map(|&c| cols[c].label().to_string()))
                .collect::<Vec<_>>()
                .join("\t"),
        );
    }
    let mut rows_used: Vec<usize> = picked.iter().map(|&(r, _)| r).collect();
    rows_used.dedup();
    for r in rows_used {
        let mut cells = Vec::new();
        if view.whole_rows {
            cells.push((r + 1).to_string());
        }
        for &c in &used_cols {
            let text = if picked.contains(&(r, c)) {
                value(app, view, items[r], cols[c]).text(&p)
            } else {
                String::new()
            };
            cells.push(text);
        }
        lines.push(cells.join("\t"));
    }
    lines.join("\n")
}

impl BananaApp {
    /// Edit Details open on `show` ("events", "cd", "commands" or
    /// "markers") over a project with markers, a region, command markers
    /// and a CD layout, a few cells picked (screen tests; the settings in
    /// memory).
    pub fn details_for_review(&mut self, show: &str) {
        let s = Tick::from_seconds;
        self.settings = crate::settings::UiSettings::in_memory();
        self.modify(|p| {
            let m = p.add_marker(s(1));
            p.rename_marker(m, "Intro".into());
            let r = p.add_region(TimeRange::new(s(3), s(4)));
            p.rename_marker(r, "Scene one".into());
            p.add_command(s(2), "URL", "https://www.example.com", "Opening link");
            p.add_command(s(6), "Text", "Welcome to the show", "");
            p.add_command(s(9), "608CC1", "", "{RCL}{ENM}Hello{EOC}");
            if let Some(t) = p.add_cd_track(TimeRange::new(s(2), s(5))) {
                p.set_cd_track(t, |c| {
                    c.name = "Overture".into();
                    c.isrc = "USS1Z9900001".into();
                });
            }
            if let Some(t) = p.add_cd_track(TimeRange::new(s(9), s(6))) {
                p.set_cd_track(t, |c| {
                    c.name = "Finale".into();
                    c.protect = true;
                });
            }
            p.add_cd_index(s(4));
        });
        let mut view = std::mem::take(&mut self.details);
        view.show = match show {
            "cd" => Show::CdTracks,
            "commands" => Show::Commands,
            "markers" => Show::Markers,
            _ => Show::Events,
        };
        let layout = view.layout();
        let show = view.show;
        let items = rows(self, &mut view, show, layout.sort);
        let cols: Vec<Col> = layout.columns.iter().map(|c| c.0).collect();
        if items.len() > 1 && cols.len() > 2 {
            view.picked = rectangle(&items, &cols, (0, 1), (1, 2));
        }
        self.details = view;
        self.show_tab(crate::dock::Tab::EditDetails, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_events() -> BananaApp {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        app
    }

    #[test]
    fn events_list_every_event_and_sort_by_any_column() {
        let mut app = app_with_events();
        let mut view = DetailsView::default();
        let all = rows(&app, &mut view, Show::Events, None);
        assert_eq!(all.len(), app.project().clips().count());
        let by_start = rows(&app, &mut view, Show::Events, Some((Col::Start, true)));
        let starts: Vec<Tick> = by_start
            .iter()
            .map(|&i| match value(&app, &mut view, i, Col::Start) {
                Value::Time(t) => t,
                v => panic!("{v:?}"),
            })
            .collect();
        assert!(starts.windows(2).all(|w| w[0] >= w[1]), "descending");
        app.selection.clear();
        assert!(rows(&app, &mut view, Show::SelectedEvents, None).is_empty());
    }

    #[test]
    fn typed_values_edit_the_project_in_the_rulers_format() {
        let mut app = app_with_events();
        let (_, c) = app
            .project()
            .clips()
            .next()
            .map(|(t, c)| (t.id, c.clone()))
            .unwrap();
        let item = Item::Event(c.id);
        // Times on the frames, as the ruler's format writes them.
        let rate = app.project().settings.frame_rate;
        let later = rate.frame_start(rate.frame_at(c.start) + 90);
        let to = app.ruler_text(later);
        assert!(app.modify(|p| apply_text(p, item, Col::Start, &to, Tick::ZERO)));
        assert_eq!(app.project().clip(c.id).unwrap().start, later);
        let second = rate.frame_start(30);
        let short = timefmt::length_text(app.project(), second);
        assert!(app.modify(|p| apply_text(p, item, Col::Length, &short, Tick::ZERO)));
        assert_eq!(app.project().clip(c.id).unwrap().duration, second);
        assert!(!app.modify(|p| apply_text(p, item, Col::Start, "not a time", Tick::ZERO)));
        // A marker and a command by name and place.
        let m = app.modify(|p| p.add_marker(Tick::from_seconds(2)));
        assert!(app.modify(|p| apply_text(p, Item::Marker(m), Col::Name, "Intro", Tick::ZERO)));
        assert_eq!(
            app.project()
                .markers
                .iter()
                .find(|x| x.id == m)
                .unwrap()
                .name,
            "Intro"
        );
        let cmd = app.modify(|p| p.add_command(Tick::ZERO, "URL", "", ""));
        assert!(app.modify(|p| apply_text(
            p,
            Item::Command(cmd),
            Col::Parameter,
            "https://example.com",
            Tick::ZERO
        )));
        assert_eq!(
            app.project().command(cmd).unwrap().parameter,
            "https://example.com"
        );
    }

    #[test]
    fn the_cd_list_reads_and_writes_tracks() {
        let mut app = app_with_events();
        let t = app
            .modify(|p| {
                p.add_cd_track(TimeRange::new(
                    Tick::from_seconds(2),
                    Tick::from_seconds(10),
                ))
            })
            .unwrap();
        let i = app
            .modify(|p| p.add_cd_index(Tick::from_seconds(6)))
            .unwrap();
        let mut view = DetailsView::default();
        let list = rows(&app, &mut view, Show::CdTracks, None);
        assert_eq!(list, vec![Item::CdTrack(t), Item::CdIndex(i)]);
        assert_eq!(
            value(&app, &mut view, Item::CdIndex(i), Col::Index),
            Value::Count(2)
        );
        assert!(app.modify(|p| apply_text(
            p,
            Item::CdTrack(t),
            Col::Isrc,
            "US-S1Z-99-00001",
            Tick::ZERO
        )));
        assert_eq!(app.project().cd_track(t).unwrap().isrc, "USS1Z9900001");
        assert!(!app.modify(|p| apply_text(
            p,
            Item::CdTrack(t),
            Col::Isrc,
            "too short",
            Tick::ZERO
        )));
        toggle(&mut app, Item::CdTrack(t), Col::Protect, false);
        assert!(app.project().cd_track(t).unwrap().protect);
    }

    #[test]
    fn copy_and_paste_go_through_tabs_and_lines() {
        let mut app = BananaApp::new(egui::Context::default());
        let a = app.modify(|p| p.add_marker(Tick::from_seconds(1)));
        let b = app.modify(|p| p.add_marker(Tick::from_seconds(2)));
        app.details.show = Show::Markers;
        app.details.picked =
            HashSet::from([(Item::Marker(a), Col::Name), (Item::Marker(b), Col::Name)]);
        run(&mut app, DetailsOp::Paste, Some("Same".into()));
        assert!(
            app.project().markers.iter().all(|m| m.name == "Same"),
            "one value fills them all"
        );
        run(&mut app, DetailsOp::Paste, Some("First\nSecond\n".into()));
        let names: Vec<&str> = app
            .project()
            .markers
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(names, ["First", "Second"], "a block goes down the rows");
        // Whole rows copy with their headings.
        let mut view = std::mem::take(&mut app.details);
        view.whole_rows = true;
        let items = rows(&app, &mut view, Show::Markers, None);
        let cols = [Col::Position, Col::Name];
        let picked = [(0, 0), (0, 1), (1, 0), (1, 1)];
        let text = copy_text(&app, &mut view, &items, &cols, &picked);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].ends_with("\tName"), "{text}");
        assert!(lines[2].ends_with("\tSecond"), "{text}");
        app.details = view;
        run(&mut app, DetailsOp::Delete, None);
        assert!(
            app.project().markers.is_empty(),
            "Delete removes the rows' items"
        );
    }

    /// As VEGAS's grids: ↔ with a bar on a heading's border and while it
    /// resizes; the arrow moving a column on a heading and while it moves,
    /// the one taking it away off the window (but the last column); the
    /// cells none of BananaV's.
    #[test]
    fn the_headings_show_the_grid_s_pointers() {
        let resize = HeadDrag::Resize {
            col: 0,
            from_w: 80.0,
            from_x: 0.0,
        };
        let moving = HeadDrag::Move { col: 1, grab: 4.0 };
        let cells = HeadDrag::Cells { from: (0, 0) };
        let c = table_cursor;
        assert_eq!(c(Hit::Border(2), None, false, 5), Some(Cursor::ResizeBarEw));
        assert_eq!(c(Hit::Head(2), None, false, 5), Some(Cursor::ColumnMove));
        assert_eq!(c(Hit::Cell(0, 0), None, false, 5), None);
        assert_eq!(c(Hit::RowNumber(0), None, false, 5), None);
        assert_eq!(
            c(Hit::Nothing, Some(resize), true, 5),
            Some(Cursor::ResizeBarEw)
        );
        assert_eq!(
            c(Hit::Cell(1, 1), Some(moving), false, 5),
            Some(Cursor::ColumnMove)
        );
        assert_eq!(
            c(Hit::Nothing, Some(moving), true, 5),
            Some(Cursor::ColumnDelete)
        );
        assert_eq!(
            c(Hit::Nothing, Some(moving), true, 1),
            Some(Cursor::ColumnMove)
        );
        assert_eq!(c(Hit::Border(0), Some(cells), false, 5), None);
    }
}
